//! Synthetic source and widget-level regression controls.
use super::*;

fn metadata_fixture() -> Vec<u8> {
    let open = [0x7a, 0x54, 0xaf, 0x39];
    let close = [0x16, 0xe4, 0x99, 0x6f];
    let mut task = vec![0; 29];
    task[..4].copy_from_slice(&[0x76, 0x25, 0xec, 0x04]);
    task[12..28].fill(0x11);
    for _ in 0..2 {
        task.extend(open);
        task.extend(0u32.to_le_bytes());
        task.extend(close);
        task.extend([0; 4]);
    }
    task.truncate(task.len() - 4);
    task.extend([0; 16]);
    task.extend([255, 254, 255, 1, b'A', 0]);
    task.extend([0; 184]);
    task.extend([0xd0, 0xbc, 0xa1, 0x06]);
    task.extend(0u32.to_le_bytes());
    task.extend([0x2c, 0x57, 0xd9, 0xda]);
    task.extend([255, 254, 255, 1, b'1', 0]);
    let tail = task.len();
    task.extend([0; 121]);
    task[tail + 25..tail + 29].copy_from_slice(&1u32.to_le_bytes());
    task[tail + 101..tail + 105].copy_from_slice(&open);
    task[tail + 109..tail + 113].copy_from_slice(&close);
    task[tail + 113..tail + 117].copy_from_slice(&[0x92, 0xd4, 0x70, 0xa2]);
    let mut tasks = vec![0; 24];
    tasks.extend(open);
    tasks.extend(1u32.to_le_bytes());
    tasks.extend(task);
    tasks.extend(close);
    let mut empty = vec![0; 24];
    empty.extend(open);
    empty.extend(0u32.to_le_bytes());
    empty.extend(close);
    let names = ["Tasks", "Resources", "Use Profiles", "Activity Codes", "User Fields", "Entities", "IncludeLibrary", "Calendars"];
    let mut raw = vec![0; 2344];
    raw[..16].copy_from_slice(&[0xb6, 0x17, 0x67, 0x1e, 0x7a, 0xf3, 0x46, 0x40, 0xa8, 0x6e, 0xd5, 0xdc, 0x41, 0x7a, 0x2d, 0xe2]);
    let mut version = vec![255, 254, 255, 8];
    version.extend("6.5.5.28".encode_utf16().flat_map(u16::to_le_bytes));
    raw[20..40].copy_from_slice(&version);
    for i in 0..48 {
        let name = names.get(i).map(|s| (*s).to_owned()).unwrap_or_else(|| format!("Other{i}"));
        let at = 40 + i * 48;
        raw[at..at + name.len()].copy_from_slice(name.as_bytes());
        let offset = raw.len() as u64;
        raw[at + 40..at + 48].copy_from_slice(&offset.to_le_bytes());
        raw.extend([1, name.len() as u8]);
        raw.extend(name.as_bytes());
        raw.extend(craft_anything_testkit::games::zlib_stored(if i == 0 { &tasks } else { &empty }));
    }
    raw
}
fn settle(harness: &mut egui_kittest::Harness<'_, DraftPanel>) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        harness.step();
        if !harness.state().state.open_requested && !harness.state().jobs.busy() {
            harness.run();
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{}", harness.state().state.status);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
#[test]
fn widget_actions_refresh_shared_revision_and_export_without_changing_source() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.sp");
    let raw = metadata_fixture();
    std::fs::write(&path, &raw).unwrap();
    let hash = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let panel = DraftPanel::new(engine.clone(), path.to_string_lossy().into_owned(), hash.clone());
    assert!(panel.active());
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100., 1000.)).build_ui_state(|ui, panel: &mut DraftPanel| panel.show(ui.ctx()), panel);
    settle(&mut h);
    assert!(h.state().state.snapshot.is_some(), "{}", h.state().state.status);
    h.get_by_label("Advanced scalar fields").click();
    h.run();
    h.state_mut().state.fields.iter_mut().find(|f| matches!(f.original, ScalarEdit::TaskDuration { .. })).unwrap().text = "42".into();
    h.run();
    h.get_by_label("Apply to draft").click();
    settle(&mut h);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().revision, 1);
    h.get_by_label("Undo").click();
    settle(&mut h);
    assert!(!h.state().state.snapshot.as_ref().unwrap().dirty);
    h.get_by_label("Redo").click();
    settle(&mut h);
    let id = h.state().state.snapshot.as_ref().unwrap().draft_id;
    engine
        .call("formats_project_edit", json!({"action":"undo","draft_id":id,"expected_source_sha256":hash,"revision":3}), CallOptions::new(Caller::Cli))
        .unwrap();
    h.get_by_label("Refresh").click();
    settle(&mut h);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().revision, 4);
    assert!(!h.state().state.snapshot.as_ref().unwrap().dirty);
    h.get_by_label("Redo").click();
    settle(&mut h);
    let output = dir.path().join("candidate.sp");
    h.state_mut().state.output = output.to_string_lossy().into_owned();
    h.get_by_label("I will validate this experimental candidate in the native application.").click();
    h.run();
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (label, planning) in [("draft-studio.png", false), ("draft-planning.png", true)] {
            h.state_mut().state.planning = planning;
            h.run();
            let image = folder.join(label);
            assert!(!image.exists(), "Choose a new visual QA directory");
            h.render().unwrap().save(image).unwrap();
        }
    }
    h.get_by_label("Export candidate").click();
    settle(&mut h);
    let generated = std::fs::read(&output).unwrap();
    let parsed = craft_anything_formats::sp_project::parse_metadata(&generated, |_| true).unwrap();
    assert_eq!(parsed.metadata["tasks"][0]["duration_storage_scalar"], 42);
    assert_eq!(std::fs::read(&path).unwrap(), raw);
    assert!(h.state().state.snapshot.as_ref().unwrap().dirty);
    h.get_by_label("Discard draft changes when closing").click();
    h.run();
    h.get_by_label("Close draft").click();
    settle(&mut h);
    assert!(!h.state().active());
}

fn snapshot(id: u64, revision: u64) -> Snapshot {
    Snapshot {
        version: 1,
        draft_id: id,
        source_sha256: "a".repeat(64),
        revision,
        dirty: revision > 0,
        can_undo: revision > 0,
        can_redo: false,
        edit_count: usize::from(revision > 0),
        task_total: 1,
        offset: 0,
        derived_tasks: DerivedPage { items: vec![] },
        tasks: vec![Task {
            guid: "11111111-1111-1111-1111-111111111111".into(),
            name: "Task".into(),
            display_id: "ID1".into(),
            planned_start_wallclock_scalar: 100,
            duration_storage_scalar: 86400,
            assignments: vec![
                Assignment { resource_guid: "resource-a".into(), profile_guid: "shared-profile".into(), planned_units: 5., rate_storage_window: 500. / 3. },
                Assignment { resource_guid: "resource-b".into(), profile_guid: "shared-profile".into(), planned_units: 10., rate_storage_window: 200. },
            ],
            direct_costs: vec![],
        }],
    }
}
#[test]
fn rejected_source_identity_and_older_responses_preserve_dirty_preview() {
    let source = "a".repeat(64);
    let mut state = State::default();
    state.accept(snapshot(1, 3), &source, Some(1)).unwrap();
    assert!(state.accept(snapshot(2, 4), &source, Some(1)).is_err());
    assert!(state.accept(snapshot(1, 4), &"b".repeat(64), Some(1)).is_err());
    assert!(state.accept(snapshot(1, 2), &source, Some(1)).is_err());
    assert_eq!(state.snapshot.as_ref().unwrap().revision, 3);
    assert!(state.snapshot.as_ref().unwrap().dirty);
    let encoded = serde_json::to_string(&state).unwrap();
    let decoded: State = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded.snapshot.unwrap().revision, 3);
}
#[test]
fn paired_assignment_form_uses_ordinal_and_resource_even_with_shared_profile() {
    let snapshot = snapshot(1, 0);
    let task = &snapshot.tasks[0];
    let mut form = fields(task, 0);
    assert!(form.iter().all(|f| f.proposal().unwrap().is_none()));
    for field in &mut form {
        match &field.original {
            ScalarEdit::AssignmentPlannedUnits { assignment_ordinal: 1, .. } => field.text = "15".into(),
            ScalarEdit::AssignmentPlannedRate { assignment_ordinal: 1, .. } => field.text = "300".into(),
            _ => {}
        }
    }
    let edits = form.iter().filter_map(|f| f.proposal().unwrap()).collect::<Vec<_>>();
    assert_eq!(edits.len(), 2);
    let encoded = serde_json::to_value(&edits).unwrap();
    for edit in encoded.as_array().unwrap() {
        assert_eq!(edit["assignment_ordinal"], 1);
        assert_eq!(edit["resource_guid"], "resource-b");
        assert_eq!(edit["profile_guid"], "shared-profile");
    }
    let rate = form.iter_mut().find(|f| matches!(f.original, ScalarEdit::AssignmentPlannedRate { assignment_ordinal: 1, .. })).unwrap();
    rate.text = "NaN".into();
    assert!(rate.proposal().is_err());
}
