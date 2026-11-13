//! Synthetic source and widget-level regression controls.
use super::*;

fn metadata_fixture() -> Vec<u8> {
    metadata_fixture_count(1)
}
fn metadata_fixture_count(count: u8) -> Vec<u8> {
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
    tasks.extend(u32::from(count).to_le_bytes());
    for i in 0..count {
        let mut record = task.clone();
        record[12..28].fill(0x11 + i);
        record[77] = b'A' + i;
        let end = record.len();
        record[end - 4..].copy_from_slice(&u32::from(i).to_le_bytes());
        tasks.extend(record);
    }
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

fn retained_scene(path: &std::path::Path, hash: &str) -> Arc<craft_anything_engine::tools::scene_tools::LoadedScene> {
    use craft_anything_formats::scene_bundle::{Scene, math::Bounds};
    Arc::new(craft_anything_engine::tools::scene_tools::LoadedScene {
        manifest: path.to_owned(),
        subject: serde_json::from_value(json!({"kind":"project","sha256":hash,"name":"retained.sp"})).unwrap(),
        metadata: Arc::new(json!({"retained":true})),
        task_collection_lookup: None,
        scene: Arc::new(Scene {
            id: "retained".into(),
            source_sha256: hash.into(),
            unit_declaration: "unresolved".into(),
            axis_declaration: "unresolved".into(),
            mesh_uvs: Default::default(),
            mesh_textures: Default::default(),
            texture_assets: None,
            source_colors: Default::default(),
            object_colors: Default::default(),
            resources: vec![],
            resource_instance_counts: vec![],
            meshes: vec![],
            instances: vec![],
            positions: vec![],
            gpu_positions: vec![],
            gpu_indices: vec![],
            bounds: Bounds([[0.; 3], [1.; 3]]),
            render_origin: [0.; 3],
            triangles: 0,
            node_count: 0,
        }),
    })
}

#[test]
fn metadata_lookup_warning_uses_its_source_and_keeps_retained_scene_unknown() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad-lookup.sp");
    let raw = metadata_fixture_count(3);
    let parsed = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap();
    let section = &parsed.metadata["sections"]["Tasks"];
    let start = section["file_offset"].as_u64().unwrap() as usize;
    let end = start + section["stored_bytes"].as_u64().unwrap() as usize;
    let mut data = raw[start + 14..end - 4].to_vec();
    let first = &parsed.metadata["tasks"][0];
    let at = (first["offset"].as_u64().unwrap() + first["bytes"].as_u64().unwrap() - 4) as usize;
    data[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut stored = b"\x01\x05Tasks".to_vec();
    stored.extend(craft_anything_testkit::games::zlib_stored(&data));
    assert_eq!(stored.len(), end - start);
    let mut bad = raw;
    bad[start..end].copy_from_slice(&stored);
    std::fs::write(&path, &bad).unwrap();
    let hash = craft_anything_formats::sp_project::parse_metadata(&bad, |_| true).unwrap().source_sha256;
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let retained = retained_scene(&dir.path().join("retained.sp"), &"a".repeat(64));
    engine.session().ext.insert(retained.clone());
    let mut app = SceneApp::with_engine(engine.clone(), None, String::new());
    app.initial_open = false;
    app.model = Some(retained.clone());
    app.metadata = Some(super::super::metadata_panel::MetadataPanel::new(engine.clone(), path.to_string_lossy().into_owned()));
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200., 1000.)).build_ui_state(|ui, app: &mut SceneApp| app.show(ui), app);
    crate::theme::install_fonts(&h.ctx);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while h.state().metadata.as_ref().unwrap().draft_source().is_none() {
        h.step();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run();
    assert_eq!(engine.session().ext.get::<craft_anything_engine::tools::scene_project_tools::LatestMetadataFrame>().unwrap().source_sha256, hash);
    assert!(Arc::ptr_eq(h.state().model.as_ref().unwrap(), &retained));
    assert!(retained.task_collection_lookup.is_none());
    for (name, planning) in [("lookup-metadata-studio.png", false), ("lookup-metadata-planning.png", true)] {
        h.state_mut().state.planning.enabled = planning;
        h.run();
        let _ = h.get_by_label("Task lookup: inconsistent · Entries: 1 · Collections: 1 · Native rows unverified");
        if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
            let folder = std::path::PathBuf::from(folder);
            std::fs::create_dir_all(&folder).unwrap();
            let image = folder.join(name);
            assert!(!image.exists());
            h.render().unwrap().save(image).unwrap();
        }
    }
    h.get_by_label("Project draft").click();
    h.run_steps(2);
    while h.state().draft.as_ref().is_none_or(|p| p.state.snapshot.is_none() || p.jobs.busy()) {
        h.step();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let snapshot = h.state().draft.as_ref().unwrap().state.snapshot.as_ref().unwrap();
    assert_eq!(snapshot.task_collection_lookup.as_ref().unwrap().status, craft_anything_formats::sp_project::task_lookup::Status::Invalid);
    h.get_by_label("Close draft").click();
    h.run_steps(2);
    while h.state().draft.as_ref().is_some_and(DraftPanel::active) {
        h.step();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.get_by_label("Back to 3D viewer").click();
    h.run_steps(2);
    let _ = h.get_by_label("Task lookup: unknown · Native rows unverified");
    assert!(Arc::ptr_eq(h.state().model.as_ref().unwrap(), &retained));
    assert_eq!(std::fs::read(path).unwrap(), bad);
}

#[test]
fn metadata_entry_uses_its_ready_source_with_or_without_a_retained_scene() {
    use egui_kittest::kittest::Queryable as _;
    for keep_scene in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("retained.sp");
        let meta_path = dir.path().join("metadata.sp");
        let old_raw = metadata_fixture_count(1);
        let meta_raw = metadata_fixture_count(3);
        std::fs::write(&old_path, &old_raw).unwrap();
        std::fs::write(&meta_path, &meta_raw).unwrap();
        let old_hash = craft_anything_formats::sp_project::parse_metadata(&old_raw, |_| true).unwrap().source_sha256;
        let meta_hash = craft_anything_formats::sp_project::parse_metadata(&meta_raw, |_| true).unwrap().source_sha256;
        let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
        let retained = retained_scene(&old_path, &old_hash);
        let mut app = SceneApp::with_engine(engine.clone(), None, old_path.to_string_lossy().into_owned());
        app.initial_open = false;
        if keep_scene {
            app.model = Some(retained.clone());
            engine.session().ext.insert(retained.clone());
        }
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if keep_scene {
            let worker = release.clone();
            app.jobs.spawn("Retained viewport work", true, move |job| {
                let started = std::time::Instant::now();
                while !worker.load(std::sync::atomic::Ordering::SeqCst) && !job.cancel.is_cancelled() && started.elapsed() < std::time::Duration::from_secs(3) {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err("Retained viewport test job".into())
            });
        }
        app.metadata = Some(super::super::metadata_panel::MetadataPanel::new(engine.clone(), meta_path.to_string_lossy().into_owned()));
        assert!(app.metadata.as_ref().unwrap().draft_source().is_none());
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200., 1000.)).build_ui_state(|ui, app: &mut SceneApp| app.show(ui), app);
        crate::theme::install_fonts(&h.ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while h.state().metadata.as_ref().unwrap().draft_source().is_none() {
            h.step();
            assert!(std::time::Instant::now() < deadline, "metadata did not become ready");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.run();
        if keep_scene {
            assert!(h.state().jobs.busy());
        }
        h.get_by_label("Project draft").click();
        h.run_steps(2);
        release.store(true, std::sync::atomic::Ordering::SeqCst);
        while h.state().draft.as_ref().is_none_or(|d| d.state.snapshot.is_none() || d.jobs.busy()) {
            h.step();
            assert!(std::time::Instant::now() < deadline, "draft did not open");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        h.run();
        let panel = h.state().draft.as_ref().unwrap();
        assert_eq!(panel.source_path, meta_path.to_string_lossy());
        assert_eq!(panel.source_hash, meta_hash);
        assert_eq!(panel.state.snapshot.as_ref().unwrap().task_total, 3);
        assert_eq!(h.state().model.is_some(), keep_scene);
        if keep_scene {
            assert!(Arc::ptr_eq(h.state().model.as_ref().unwrap(), &retained));
            assert!(Arc::ptr_eq(&engine.session().ext.get::<craft_anything_engine::tools::scene_tools::LoadedScene>().unwrap(), &retained));
        }
        assert_eq!(std::fs::read(&old_path).unwrap(), old_raw);
        assert_eq!(std::fs::read(&meta_path).unwrap(), meta_raw);
        if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
            let name = if keep_scene { "metadata-draft-retained.png" } else { "metadata-draft-no-scene.png" };
            let target = std::path::PathBuf::from(folder).join(name);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            assert!(!target.exists());
            h.render().unwrap().save(target).unwrap();
        }
    }
}

#[test]
fn opening_another_source_preserves_an_active_draft_and_unapplied_forms() {
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let mut app = SceneApp::with_engine(engine, None, String::new());
    let source = DraftSource { path: "first.sp".into(), source_sha256: "a".repeat(64) };
    app.open_draft_from(source.clone());
    app.draft.as_mut().unwrap().state.labels.name = "Unapplied".into();
    app.open_draft_from(source);
    assert_eq!(app.draft.as_ref().unwrap().state.labels.name, "Unapplied");
    app.open_draft_from(DraftSource { path: "second.sp".into(), source_sha256: "b".repeat(64) });
    let original = app.draft.as_ref().unwrap();
    assert_eq!(original.source_path, "first.sp");
    assert_eq!(original.state.labels.name, "Unapplied");
    assert!(original.state.show);
    app.open_draft_from(DraftSource { path: "first.sp".into(), source_sha256: "b".repeat(64) });
    assert_eq!(app.draft.as_ref().unwrap().source_hash, "a".repeat(64));
    let original = app.draft.as_mut().unwrap();
    original.state.open_requested = false;
    original.state.show = false;
    app.open_draft_from(DraftSource { path: "second.sp".into(), source_sha256: "b".repeat(64) });
    assert_eq!(app.draft.as_ref().unwrap().source_path, "second.sp");
    assert_eq!(app.draft.as_ref().unwrap().state.labels.name, "");
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
fn deletion_widget_pages_cancel_undo_redo_and_export_follow_the_engine() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.sp");
    let raw = craft_anything_testkit::project_authoring::default_work_project_with_presentation(110, Some(100));
    std::fs::write(&path, &raw).unwrap();
    let hash = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let panel = DraftPanel::new(engine, path.to_string_lossy().into_owned(), hash);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100., 1000.)).build_ui_state(|ui, panel: &mut DraftPanel| panel.show(ui.ctx()), panel);
    crate::theme::install_fonts(&h.ctx);
    settle(&mut h);
    h.get_by_label("Next tasks").click();
    settle(&mut h);
    let target = h.state().state.snapshot.as_ref().unwrap().tasks[0].guid.clone();
    let next = h.state().state.snapshot.as_ref().unwrap().tasks[1].guid.clone();
    h.get_by_label("Delete task").scroll_to_me();
    h.run();
    h.get_by_label("Delete task").click();
    h.run();
    h.state_mut().state.labels.name = "Pending label".into();
    h.run();
    h.get_by_label("Cancel deletion").scroll_to_me();
    h.run();
    h.get_by_label("Cancel deletion").click();
    h.run();
    assert_eq!(h.state().state.labels.name, "Pending label");
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().revision, 0);
    h.state_mut().state.labels.name = "Task101".into();
    h.run();
    h.get_by_label("Delete task").scroll_to_me();
    h.run();
    h.get_by_label("Delete task").click();
    h.run();
    h.get_by_label("Delete from draft").scroll_to_me();
    h.run();
    h.get_by_label("Delete from draft").click();
    settle(&mut h);
    let deleted = h.state().state.snapshot.as_ref().unwrap();
    assert_eq!(deleted.task_total, 109);
    assert_eq!(deleted.offset, 100);
    assert_eq!(deleted.tasks[h.state().state.selected].guid, next);
    assert_eq!(deleted.undo_deletion_task_guid.as_deref(), Some(target.as_str()));
    h.get_by_label("Undo").scroll_to_me();
    h.run();
    h.get_by_label("Undo").click();
    settle(&mut h);
    let restored = h.state().state.snapshot.as_ref().unwrap();
    assert_eq!(restored.offset, 100);
    assert_eq!(restored.tasks[h.state().state.selected].guid, target);
    assert!(!restored.dirty);
    h.get_by_label("Redo").click();
    settle(&mut h);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().tasks[h.state().state.selected].guid, next);
    h.get_by_label("Delete task").scroll_to_me();
    h.run();
    h.get_by_label("Delete task").click();
    h.run();
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (label, planning) in [("deletion-widget-studio.png", false), ("deletion-widget-planning.png", true)] {
            h.state_mut().state.planning = planning;
            h.run();
            h.get_by_label("Delete from draft").scroll_to_me();
            h.run();
            let image = folder.join(label);
            assert!(!image.exists());
            h.render().unwrap().save(image).unwrap();
        }
    }
    let output = dir.path().join("deleted.sp");
    h.state_mut().state.output = output.to_string_lossy().into_owned();
    h.get_by_label("I will validate this experimental candidate in the native application.").scroll_to_me();
    h.run();
    h.get_by_label("I will validate this experimental candidate in the native application.").click();
    h.run();
    h.get_by_label("Export candidate").scroll_to_me();
    h.run();
    h.get_by_label("Export candidate").click();
    settle(&mut h);
    let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["version"], 6);
    assert_eq!(receipt["stages"][0]["kind"], "delete");
    assert_eq!(receipt["stages"][0]["receipt"]["deletion"]["task_guid"], target);
    assert_eq!(std::fs::read(path).unwrap(), raw);
}

#[test]
fn deletion_snapshot_rejects_stale_layout_guards_and_invalid_undo_cues() {
    let source = "a".repeat(64);
    let mut good = snapshot(1, 0);
    good.task_total = 2;
    good.tasks[0].sibling_total = 2;
    good.tasks[0].assignments.clear();
    let mut second = good.tasks[0].clone();
    second.guid = "22222222-2222-2222-2222-222222222222".into();
    second.sibling_index = 1;
    second.display_id = "ID2".into();
    good.tasks.push(second);
    good.draft_sha256 = "b".repeat(64);
    good.deletion_source_sha256 = good.draft_sha256.clone();
    good.deletion_layout_supported = true;
    good.deletion_reference_scan_required = true;
    good.tasks[0].deletion = Some(craft_anything_formats::sp_project::deletion::DeleteLeaf {
        task_guid: good.tasks[0].guid.clone(),
        expected_parent_guid: None,
        expected_sibling_index: 0,
        expected_display_id: "ID1".into(),
    });
    let mut state = State::default();
    state.accept(good.clone(), &source, None).unwrap();
    let mut stale = good.clone();
    stale.deletion_source_sha256 = "c".repeat(64);
    assert!(state.accept(stale, &source, None).is_err());
    let mut stale = good.clone();
    stale.tasks[0].deletion.as_mut().unwrap().expected_display_id = "wrong".into();
    assert!(state.accept(stale, &source, None).is_err());
    let mut stale = good.clone();
    stale.undo_deletion_task_guid = Some(good.tasks[0].guid.clone());
    assert!(state.accept(stale, &source, None).is_err());
    let mut stale = good;
    stale.deletion_reference_scan_required = false;
    assert!(state.accept(stale, &source, None).is_err());
}
#[test]
fn creation_widget_focus_history_cancel_and_export_follow_the_engine() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.sp");
    let raw = craft_anything_testkit::project_authoring::default_work_project(110);
    std::fs::write(&path, &raw).unwrap();
    let hash = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let panel = DraftPanel::new(engine, path.to_string_lossy().into_owned(), hash);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100., 1000.)).build_ui_state(|ui, panel: &mut DraftPanel| panel.show(ui.ctx()), panel);
    crate::theme::install_fonts(&h.ctx);
    settle(&mut h);
    h.get_by_label("Next tasks").click();
    settle(&mut h);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().offset, 100);
    h.get_by_label("Create task").click();
    h.run();
    h.state_mut().state.labels.name = "Pending original label".into();
    h.run();
    h.get_by_label("New task name").focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_by_label("New task name").type_text("Cancelled");
    h.run_steps(2);
    h.get_by_label("Cancel task creation").click();
    h.run();
    assert_eq!(h.state().state.labels.name, "Pending original label");
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().revision, 0);
    let original = h.state().state.snapshot.as_ref().unwrap().tasks[0].name.clone();
    h.state_mut().state.labels.name = original;
    h.run();
    let name = "Widget_\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}";
    for (label, value) in [("New task name", name), ("New task ID", "WORK-1")] {
        h.get_by_label(label).focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        h.get_by_label(label).type_text(value);
        h.run_steps(2);
    }
    h.get_by_label("Create one-day task above").click();
    settle(&mut h);
    let created = h.state().state.snapshot.as_ref().unwrap();
    assert_eq!(created.revision, 1);
    assert_eq!(created.offset, 100);
    assert_eq!(h.state().state.selected, 0);
    assert_eq!(created.tasks[0].name, name);
    let guid = created.tasks[0].guid.clone();
    assert_eq!(created.tasks[0].duration_storage_scalar, 28_800);
    h.get_by_label("Undo").scroll_to_me();
    h.run();
    h.get_by_label("Undo").click();
    settle(&mut h);
    assert!(!h.state().state.snapshot.as_ref().unwrap().dirty);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().redo_creation_task_guid.as_deref(), Some(guid.as_str()));
    h.get_by_label("Redo").click();
    settle(&mut h);
    let redone = h.state().state.snapshot.as_ref().unwrap();
    assert_eq!(redone.offset, 100);
    assert_eq!(redone.tasks[h.state().state.selected].guid, guid);
    h.get_by_label("Task labels").click();
    h.run();
    h.state_mut().state.labels.name = format!("{name}_Edited");
    h.run();
    h.get_by_label("Apply to draft").scroll_to_me();
    h.run();
    h.get_by_label("Apply to draft").click();
    settle(&mut h);
    let output = dir.path().join("created.sp");
    h.state_mut().state.output = output.to_string_lossy().into_owned();
    h.get_by_label("I will validate this experimental candidate in the native application.").scroll_to_me();
    h.run();
    h.get_by_label("I will validate this experimental candidate in the native application.").click();
    h.run();
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (label, planning) in [("creation-widget-studio.png", false), ("creation-widget-planning.png", true)] {
            h.state_mut().state.planning = planning;
            h.run();
            h.get_by_label("Create task").scroll_to_me();
            h.run();
            let image = folder.join(label);
            assert!(!image.exists());
            h.render().unwrap().save(image).unwrap();
        }
    }
    h.get_by_label("Export candidate").scroll_to_me();
    h.run();
    h.get_by_label("Export candidate").click();
    settle(&mut h);
    let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["version"], 5);
    assert_eq!(receipt["stages"][0]["kind"], "create");
    assert_eq!(receipt["stages"][1]["kind"], "fields");
    assert_eq!(receipt["stages"][0]["receipt"]["insertion"]["task_guid"], guid);
    let metadata = craft_anything_formats::sp_project::parse_metadata(&std::fs::read(output).unwrap(), |_| true).unwrap();
    assert_eq!(metadata.metadata["tasks"][100]["name"], format!("{name}_Edited"));
    assert_eq!(std::fs::read(path).unwrap(), raw);
}

#[test]
fn creation_snapshot_rejects_stale_hints_and_unresolved_focus() {
    let mut state = State::default();
    let source = "a".repeat(64);
    let mut value = snapshot(1, 0);
    value.creation_supported = true;
    value.creation_source_sha256 = "b".repeat(64);
    value.draft_sha256 = "c".repeat(64);
    assert!(state.accept(value, &source, None).is_err());
    let mut value = snapshot(1, 0);
    value.focused_task_guid = Some("22222222-2222-2222-2222-222222222222".into());
    assert!(state.accept(value, &source, None).is_err());
    let mut value = snapshot(1, 0);
    value.redo_creation_task_guid = Some("22222222-2222-2222-2222-222222222222".into());
    assert!(state.accept(value, &source, None).is_err());
    assert!(state.snapshot.is_none());
}
#[test]
fn widget_actions_refresh_shared_revision_and_export_mixed_without_changing_source() {
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
    crate::theme::install_fonts(&h.ctx);
    settle(&mut h);
    assert!(h.state().state.snapshot.is_some(), "{}", h.state().state.status);
    h.get_by_label("Task labels").click();
    h.run();
    for (label, value) in [("Task name", "\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}"), ("Display ID", "TASK_\u{03b1}")] {
        h.get_by_label(label).focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        h.get_by_label(label).type_text(value);
        h.run_steps(2);
    }
    h.get_by_label("Advanced scalar fields").click();
    h.run();
    h.state_mut().state.fields.iter_mut().find(|f| matches!(f.original, ScalarEdit::TaskDuration { .. })).unwrap().text = "42".into();
    h.run();
    h.get_by_label("Apply to draft").click();
    settle(&mut h);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().revision, 1);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().edit_count, 3);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().tasks[0].name, "\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}");

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
    h.get_by_label("I will validate this experimental candidate in the native application.").scroll_to_me();
    h.run();
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
    h.get_by_label("Export candidate").scroll_to_me();
    h.run();
    h.get_by_label("Export candidate").click();
    settle(&mut h);
    let generated = std::fs::read(&output).unwrap();
    let parsed = craft_anything_formats::sp_project::parse_metadata(&generated, |_| true).unwrap();
    assert_eq!(parsed.metadata["tasks"][0]["duration_storage_scalar"], 42);
    assert_eq!(parsed.metadata["tasks"][0]["name"], "\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}");
    assert_eq!(parsed.metadata["tasks"][0]["display_id"], "TASK_\u{03b1}");
    let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["version"], 2);
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let path = std::path::PathBuf::from(folder).join("mixed-widget-receipt.json");
        assert!(!path.exists());
        std::fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }

    assert_eq!(std::fs::read(&path).unwrap(), raw);
    assert!(h.state().state.snapshot.as_ref().unwrap().dirty);
    h.get_by_label("Discard draft changes when closing").scroll_to_me();
    h.run();
    h.get_by_label("Discard draft changes when closing").click();
    h.run();
    h.get_by_label("Close draft").click();
    settle(&mut h);
    assert!(!h.state().active());
}

fn snapshot(id: u64, revision: u64) -> Snapshot {
    Snapshot {
        task_collection_lookup: None,
        version: 1,
        draft_id: id,
        source_sha256: "a".repeat(64),
        draft_sha256: String::new(),
        focused_task_guid: None,
        redo_creation_task_guid: None,
        undo_deletion_task_guid: None,
        deletion_source_sha256: String::new(),
        deletion_layout_supported: false,
        deletion_reference_scan_required: false,
        deletion_reason: String::new(),
        creation_source_sha256: String::new(),
        creation_supported: false,
        creation_suggested_display_id: String::new(),
        revision,
        dirty: revision > 0,
        can_undo: revision > 0,
        can_redo: false,
        edit_count: usize::from(revision > 0),
        order_edit_count: 0,
        order_edit_limit: ORDER_LIMIT,
        order_supported: false,
        order_reason: "Assigned-task ordering remains outside the admitted subset.".into(),
        hierarchy_supported: false,
        hierarchy_reason: String::new(),
        task_total: 1,
        offset: 0,
        derived_tasks: DerivedPage { items: vec![] },
        tasks: vec![Task {
            guid: "11111111-1111-1111-1111-111111111111".into(),
            creation: None,
            creation_reason: String::new(),
            deletion: None,
            name: "Task".into(),
            display_id: "ID1".into(),
            parent_guid: None,
            sibling_index: 0,
            sibling_total: 1,
            parent_label: "Project root".into(),
            indent: None,
            outdent: None,
            indent_label: String::new(),
            outdent_label: String::new(),
            indent_depth_limited: false,
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
fn reordered_snapshot_preserves_selected_guid_and_rejects_invalid_positions() {
    let source = "a".repeat(64);
    let mut initial = snapshot(1, 0);
    initial.order_supported = true;
    initial.order_reason.clear();
    initial.task_total = 2;
    let mut second = initial.tasks[0].clone();
    second.guid = "22222222-2222-2222-2222-222222222222".into();
    second.name = "Second".into();
    second.sibling_index = 1;
    second.sibling_total = 2;
    initial.tasks[0].sibling_total = 2;
    initial.tasks.push(second);
    let mut state = State::default();
    state.accept(initial.clone(), &source, None).unwrap();
    state.selected = 1;
    let mut next = initial;
    next.revision = 1;
    next.tasks.swap(0, 1);
    for (i, t) in next.tasks.iter_mut().enumerate() {
        t.sibling_index = i;
    }
    state.accept(next.clone(), &source, Some(1)).unwrap();
    assert_eq!(state.selected, 0);
    assert_eq!(state.order_position, 1);
    assert_eq!(state.labels.name, "Second");
    next.tasks[0].sibling_total = 0;
    state.order_position = 2;
    assert!(state.accept(next, &source, Some(1)).is_err());
    assert_eq!(state.order_position, 2);
}

#[test]
fn lookup_report_guards_preserve_forms_and_keep_older_responses_unknown() {
    let source = "a".repeat(64);
    let original = snapshot(1, 0);
    let mut legacy = serde_json::to_value(&original).unwrap();
    legacy.as_object_mut().unwrap().remove("task_collection_lookup");
    let decoded: Snapshot = serde_json::from_value(legacy).unwrap();
    assert!(decoded.task_collection_lookup.is_none());
    let mut state = State::default();
    state.accept(decoded, &source, None).unwrap();
    state.labels.name = "Unapplied".into();
    let report = craft_anything_formats::sp_project::parse_metadata(&metadata_fixture(), |_| true).unwrap().coverage.task_collection_lookup;
    for kind in 0..3 {
        let mut value = original.clone();
        value.revision = 1;
        let mut bad = report.clone();
        match kind {
            0 => bad.native_row_identity_verified = true,
            1 => bad.version = 2,
            _ => bad.entries_checked = 2,
        }
        value.task_collection_lookup = Some(bad);
        assert!(state.accept(value, &source, Some(1)).is_err());
        assert_eq!(state.snapshot.as_ref().unwrap().revision, 0);
        assert_eq!(state.labels.name, "Unapplied");
    }
}

#[test]
fn widget_hierarchy_labels_and_scalar_edits_share_history_and_version_four_export() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.sp");
    let raw = metadata_fixture_count(3);
    std::fs::write(&path, &raw).unwrap();
    let hash = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let panel = DraftPanel::new(engine, path.to_string_lossy().into_owned(), hash);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100., 1000.)).build_ui_state(|ui, panel: &mut DraftPanel| panel.show(ui.ctx()), panel);
    crate::theme::install_fonts(&h.ctx);
    settle(&mut h);
    let parent = h.state().state.snapshot.as_ref().unwrap().tasks[0].guid.clone();
    h.get_by_value("1 · A").click();
    h.run();
    h.get_by_label("1 · B").click();
    h.run();
    let selected = h.state().state.snapshot.as_ref().unwrap().tasks[1].guid.clone();
    h.get_by_label("Task hierarchy").click();
    h.run();
    h.get_by_label("Indent").click();
    h.run();
    assert_eq!(h.state().state.parent_change.as_ref().unwrap().parent_guid, Some(parent.clone()));
    h.get_by_label("Task labels").click();
    h.run();
    let renamed = "Nested_\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}";
    for (label, text) in [("Task name", renamed), ("Display ID", "ST_nested_20")] {
        h.get_by_label(label).focus();
        h.run_steps(2);
        h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
        h.run_steps(2);
        h.get_by_label(label).type_text(text);
        h.run_steps(2);
    }
    h.state_mut().state.fields.iter_mut().find(|f| matches!(f.original, ScalarEdit::TaskDuration { .. })).unwrap().text = "57600".into();
    h.run();
    h.get_by_label("Apply to draft").scroll_to_me();
    h.run();
    h.get_by_label("Apply to draft").click();
    settle(&mut h);
    let current = h.state().state.snapshot.as_ref().unwrap();
    assert!(current.dirty);
    assert_eq!(current.edit_count, 4);
    assert_eq!(current.order_edit_count, 1);
    assert_eq!(current.tasks[1].guid, selected);
    assert_eq!(current.tasks[1].parent_guid, Some(parent.clone()));
    assert!(h.state().state.parent_change.is_none());
    h.get_by_label("Undo").scroll_to_me();
    h.run();
    h.get_by_label("Undo").click();
    settle(&mut h);
    assert!(!h.state().state.snapshot.as_ref().unwrap().dirty);
    h.get_by_label("Redo").click();
    settle(&mut h);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().tasks[1].parent_guid, Some(parent));
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, planning) in [("hierarchy-draft-studio.png", false), ("hierarchy-draft-planning.png", true)] {
            h.state_mut().state.planning = planning;
            h.run();
            let image = folder.join(name);
            assert!(!image.exists());
            h.render().unwrap().save(image).unwrap();
            h.get_by_label("Export candidate").scroll_to_me();
            h.run();
            let footer = folder.join(format!("hierarchy-draft-{}-footer.png", if planning { "planning" } else { "studio" }));
            assert!(!footer.exists());
            h.render().unwrap().save(footer).unwrap();
        }
    }
    let output = dir.path().join("hierarchy.sp");
    h.state_mut().state.output = output.to_string_lossy().into_owned();
    h.get_by_label("I will validate this experimental candidate in the native application.").scroll_to_me();
    h.run();
    h.get_by_label("I will validate this experimental candidate in the native application.").click();
    h.run();
    h.get_by_label("Export candidate").scroll_to_me();
    h.run();
    h.get_by_label("Export candidate").click();
    settle(&mut h);
    let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["version"], 4);
    assert_eq!(receipt["structural_stages"][0]["kind"], "hierarchy");
    assert_eq!(receipt["mixed_stage"]["output_sha256"], receipt["structural_stages"][0]["receipt"]["source_sha256"]);
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        std::fs::write(std::path::PathBuf::from(folder).join("structural-widget-receipt.json"), serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
    h.get_by_label("Outdent").scroll_to_me();
    h.run();
    h.get_by_label("Outdent").click();
    h.run();
    h.get_by_label("Apply to draft").scroll_to_me();
    h.run();
    h.get_by_label("Apply to draft").click();
    settle(&mut h);
    let restored = h.state().state.snapshot.as_ref().unwrap();
    assert!(restored.tasks[1].parent_guid.is_none());
    assert_eq!(restored.order_edit_count, 0);
    assert_eq!(restored.edit_count, 3);
    assert_eq!(restored.tasks[1].name, renamed);
    assert_eq!(std::fs::read(path).unwrap(), raw);
}

#[test]
fn widget_order_and_label_edits_share_undo_redo_and_version_three_export() {
    use egui_kittest::kittest::Queryable as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.sp");
    let raw = metadata_fixture_count(3);
    std::fs::write(&path, &raw).unwrap();
    let hash = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
    let engine = Arc::new(Engine::new(craft_anything_engine::testing::session()));
    let panel = DraftPanel::new(engine, path.to_string_lossy().into_owned(), hash);
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1100., 1000.)).build_ui_state(|ui, panel: &mut DraftPanel| panel.show(ui.ctx()), panel);
    crate::theme::install_fonts(&h.ctx);
    settle(&mut h);
    h.get_by_value("1 \u{00b7} A").click();
    h.run();
    h.get_by_label("1 \u{00b7} B").click();
    h.run();
    let selected = h.state().state.snapshot.as_ref().unwrap().tasks[h.state().state.selected].guid.clone();
    h.get_by_label("Task order").click();
    h.run();
    h.get_by_label("Earlier").click();
    h.run();
    assert_eq!(h.state().state.order_position, 1);
    h.get_by_label("Task labels").click();
    h.run();
    h.get_by_label("Task name").focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    let renamed = "Moved_\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}";
    h.get_by_label("Task name").type_text(renamed);
    h.run_steps(2);
    h.state_mut().state.fields.iter_mut().find(|f| matches!(f.original, ScalarEdit::TaskDuration { .. })).unwrap().text = "42".into();
    h.run();
    h.get_by_label("Apply to draft").scroll_to_me();
    h.run();
    h.get_by_label("Apply to draft").click();
    settle(&mut h);
    let current = h.state().state.snapshot.as_ref().unwrap();
    assert_eq!(current.edit_count, 3);
    assert_eq!(current.order_edit_count, 1);
    assert_eq!(current.tasks[0].guid, selected);
    assert_eq!(current.tasks[0].name, renamed);
    assert_eq!(h.state().state.selected, 0);
    h.get_by_label("Undo").scroll_to_me();
    h.run();
    h.get_by_label("Undo").click();
    settle(&mut h);
    let restored = h.state().state.snapshot.as_ref().unwrap();
    assert!(!restored.dirty);
    assert_eq!(restored.tasks[1].guid, selected);
    assert_eq!(h.state().state.selected, 1);
    h.get_by_label("Redo").click();
    settle(&mut h);
    assert_eq!(h.state().state.selected, 0);
    assert_eq!(h.state().state.snapshot.as_ref().unwrap().tasks[0].guid, selected);
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, planning) in [("ordered-draft-studio.png", false), ("ordered-draft-planning.png", true)] {
            h.state_mut().state.planning = planning;
            h.run();
            let image = folder.join(name);
            assert!(!image.exists());
            h.render().unwrap().save(image).unwrap();
        }
    }
    let output = dir.path().join("ordered.sp");
    h.state_mut().state.output = output.to_string_lossy().into_owned();
    h.get_by_label("I will validate this experimental candidate in the native application.").scroll_to_me();
    h.run();
    h.get_by_label("I will validate this experimental candidate in the native application.").click();
    h.run();
    h.get_by_label("Export candidate").scroll_to_me();
    h.run();
    h.get_by_label("Export candidate").click();
    settle(&mut h);
    let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
    assert_eq!(receipt["version"], 3);
    assert_eq!(receipt["mixed_stage"]["output_sha256"], receipt["order_stages"][0]["source_sha256"]);
    let parsed = craft_anything_formats::sp_project::parse_metadata(&std::fs::read(output).unwrap(), |_| true).unwrap();
    assert_eq!(parsed.metadata["tasks"][0]["guid"], selected);
    assert_eq!(parsed.metadata["tasks"][0]["name"], renamed);
    assert_eq!(parsed.metadata["tasks"][0]["duration_storage_scalar"], 42);
    assert_eq!(std::fs::read(path).unwrap(), raw);
    if let Ok(folder) = std::env::var("CRAFT_ANYTHING_DRAFT_UI_QA_DIR") {
        let target = std::path::PathBuf::from(folder).join("ordered-widget-receipt.json");
        assert!(!target.exists());
        std::fs::write(target, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }
}
#[test]
fn rejected_source_identity_and_older_responses_preserve_dirty_preview() {
    let source = "a".repeat(64);
    let mut state = State::default();
    state.accept(snapshot(1, 3), &source, Some(1)).unwrap();
    state.labels.name = "Unapplied label".into();
    assert!(state.accept(snapshot(2, 4), &source, Some(1)).is_err());
    assert!(state.accept(snapshot(1, 4), &"b".repeat(64), Some(1)).is_err());
    assert!(state.accept(snapshot(1, 2), &source, Some(1)).is_err());
    assert_eq!(state.snapshot.as_ref().unwrap().revision, 3);
    assert_eq!(state.labels.name, "Unapplied label");
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

#[test]
fn task_labels_bind_current_identity_and_count_utf16_units() {
    let snapshot = snapshot(1, 0);
    let task = &snapshot.tasks[0];
    let mut labels = TaskLabels::from_task(task);
    assert!(labels.proposals(task).unwrap().is_empty());
    labels.name = "\u{6881}".into();
    labels.display_id = "TASK_\u{03b1}".into();
    let edits = labels.proposals(task).unwrap();
    assert!(matches!(&edits[0],TextEdit::TaskName{task_guid,expected,..} if task_guid==&task.guid && expected==&task.name));
    assert!(matches!(&edits[1],TextEdit::TaskDisplayId{task_guid,expected,..} if task_guid==&task.guid && expected==&task.display_id));
    labels.name = "\u{1f600}".repeat(8193);
    assert!(labels.proposals(task).is_err());
    labels.name = "valid".into();
    labels.display_id = "bad\0id".into();
    assert!(labels.proposals(task).is_err());
}

#[test]
fn task_selector_bounds_preview_but_keeps_full_editor_text() {
    let mut snapshot = snapshot(1, 0);
    let task = &mut snapshot.tasks[0];
    task.display_id.clear();
    task.name = "\u{6881}".repeat(16384);
    let preview = task.label();
    assert_eq!(preview.chars().count(), 193);
    assert!(preview.ends_with('\u{2026}'));
    let labels = TaskLabels::from_task(task);
    assert_eq!(labels.name, task.name);
    assert!(labels.proposals(task).unwrap().is_empty());
}
