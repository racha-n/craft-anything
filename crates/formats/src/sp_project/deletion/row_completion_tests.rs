use super::*;
use crate::sp_project::{draft::Draft, task_strings::TextEdit};

fn inspect(raw: &[u8]) -> Evidence {
    let hash = writer::hash(raw, &mut |_| true).unwrap();
    inspect_container(container(raw, &mut |_| true).unwrap(), hash, false, &mut |_| true).unwrap()
}
fn source(n: usize) -> Vec<u8> {
    craft_anything_testkit::project_authoring::default_work_project_with_presentation(n, Some(0))
}
fn insertion(raw: &[u8], serial: u32) -> creation::InsertAbove {
    let e = inspect(raw);
    let anchor = e.tasks.first().unwrap();
    creation::InsertAbove {
        template_task_guid: anchor.guid.clone(),
        anchor_task_guid: anchor.guid.clone(),
        expected_parent_guid: None,
        expected_sibling_index: 0,
        task_guid: format!("aaaaaaaa-aaaa-4aaa-8aaa-{serial:012x}"),
        supply_chain_id: format!("bbbbbbbb-bbbb-4bbb-8bbb-{serial:012x}"),
        name: format!("Created_{serial}"),
        display_id: format!("NEW-{serial}"),
    }
}
fn deletion(raw: &[u8], guid: &str) -> DeleteLeaf {
    let e = inspect(raw);
    let index = e.tasks.iter().position(|t| t.guid == guid).unwrap();
    DeleteLeaf { task_guid: guid.into(), expected_parent_guid: None, expected_sibling_index: index, expected_display_id: e.tasks[index].display_id.clone() }
}
fn identity(r: &creation::InsertAbove) -> CreatedIdentity {
    CreatedIdentity { task_guid: r.task_guid.clone(), supply_chain_id: r.supply_chain_id.clone() }
}
fn presentation(raw: &[u8]) -> Vec<u8> {
    let e = inspect(raw);
    decoded_section(raw, "PresentationData", &e.sections["PresentationData"], &mut |_| true).unwrap()
}
fn repack_presentation(raw: &[u8], data: &[u8]) -> Vec<u8> {
    writer::pack_sections(raw, &inspect(raw), &[("PresentationData", data)], &mut |_| true).unwrap().0
}
fn assert_chain(receipt: &serde_json::Value) {
    let mut hash = receipt["prefix"]["output_sha256"].as_str().unwrap();
    for stage in receipt["stages"].as_array().unwrap() {
        assert_eq!(stage["receipt"]["source_sha256"], hash);
        hash = stage["receipt"]["output_sha256"].as_str().unwrap();
    }
    assert_eq!(receipt["output_sha256"], hash);
}

#[test]
fn completion_preserves_tasks_existing_rows_selection_and_all_other_sections() {
    for n in [2, 4, 110] {
        let raw = source(n);
        let before = raw.clone();
        let r = insertion(&raw, 1);
        let c = creation::candidate(&raw, &inspect(&raw).source_sha256, &r, |_| true).unwrap();
        let original = c.bytes.clone();
        let hash = inspect(&c.bytes).source_sha256;
        assert!(layout_availability(&c.bytes, &hash, |_| true).unwrap().anchors.is_empty());
        let completed = candidate(&c.bytes, &hash, &[identity(&r)], |_| true).unwrap().unwrap();
        assert_eq!(completed.receipt.row_count_before, n);
        assert_eq!(completed.receipt.row_count_after, n + 1);
        assert_eq!(completed.receipt.added_rows[0].task_guid, r.task_guid);
        assert_eq!(completed.receipt.observed_row_opaque_hex, "01010000000101");
        assert_eq!(completed.receipt.sections.iter().filter(|s| s.stored_bytes_preserved).count(), 47);
        assert!(!completed.receipt.native_reopen_verified);
        assert!(candidate(&completed.bytes, &completed.receipt.output_sha256, &[identity(&r)], |_| true).unwrap().is_none());
        let req = deletion(&completed.bytes, &r.task_guid);
        let removed = super::super::candidate(&completed.bytes, &completed.receipt.output_sha256, &req, |_| true).unwrap();
        assert_eq!(container(&removed.bytes, &mut |_| true).unwrap().decoded["Tasks"], container(&raw, &mut |_| true).unwrap().decoded["Tasks"]);
        assert_eq!(presentation(&removed.bytes), presentation(&raw));
        assert_eq!(c.bytes, original);
        assert_eq!(raw, before);
    }
}

#[test]
fn missing_original_rows_unknown_references_and_forged_pairs_are_refused() {
    let raw = source(4);
    let r = insertion(&raw, 1);
    let created = creation::candidate(&raw, &inspect(&raw).source_sha256, &r, |_| true).unwrap();
    let hash = inspect(&created.bytes).source_sha256;
    assert!(candidate(&created.bytes, &hash, &[], |_| true).unwrap().is_none());
    assert!(candidate(&created.bytes, &"0".repeat(64), &[identity(&r)], |_| true).is_err());
    assert!(candidate(&created.bytes, &hash, &[identity(&r)], |_| false).is_err());
    let bad = CreatedIdentity { supply_chain_id: r.task_guid.clone(), ..identity(&r) };
    assert!(candidate(&created.bytes, &hash, &[bad], |_| true).is_err());
    assert!(candidate(&created.bytes, &hash, &[identity(&r), identity(&r)], |_| true).is_err());
    let e = inspect(&created.bytes);
    let t = writer::task_for(&e.tasks, &r.task_guid).unwrap();
    let task_data = container(&created.bytes, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap();
    let raw_id = bytes(&task_data, t.offset + 12, 16).unwrap();
    for extra in [raw_id.to_vec(), r.task_guid.as_bytes().to_vec(), r.task_guid.encode_utf16().flat_map(u16::to_le_bytes).collect()] {
        let mut view = presentation(&created.bytes);
        view.extend(extra);
        let changed = repack_presentation(&created.bytes, &view);
        assert!(candidate(&changed, &inspect(&changed).source_sha256, &[identity(&r)], |_| true).is_err());
    }
    // An original task's absent row is never authorized by a different new task.
    let target = &inspect(&raw).tasks[1];
    let data = container(&raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap();
    let original_id = bytes(&data, target.offset + 12, 16).unwrap();
    let mut view = presentation(&raw);
    let at = view.windows(24).position(|v| v.starts_with(original_id) && v.get(16..24) == Some(&[0x12, 0x24, 0xb4, 0x13, 0x17, 0, 0, 0])).unwrap();
    view.drain(at..at + 35);
    let map = view.windows(8).position(|v| v == [0xf2, 0x96, 0x1c, 0x49, 0x50, 0xd7, 0xfc, 0xfa]).unwrap();
    view[map + 8..map + 12].copy_from_slice(&3u32.to_le_bytes());
    let broken = repack_presentation(&raw, &view);
    let mut draft = Draft::open(broken.clone(), &inspect(&broken).source_sha256, |_| true).unwrap();
    draft.insert_above(0, &r, |_| true).unwrap();
    let materialized = draft.bytes(|_| true).unwrap();
    let hints = draft.deletion_availability(&materialized, &inspect(&materialized).source_sha256, |_| true).unwrap();
    assert!(hints.anchors.is_empty());
    let req = deletion(&materialized, &r.task_guid);
    assert!(draft.delete_leaf(1, &req, |_| true).is_err());
    assert_eq!(draft.revision, 1);
    assert_eq!(draft.bytes(|_| true).unwrap(), materialized);
}

#[test]
fn post_creation_delete_retains_the_original_creation_stage_and_undo_bytes() {
    let raw = source(4);
    let r = insertion(&raw, 1);
    let mut d = Draft::open(raw.clone(), &inspect(&raw).source_sha256, |_| true).unwrap();
    d.insert_above(0, &r, |_| true).unwrap();
    let created = d.bytes(|_| true).unwrap();
    let old_receipt = serde_json::to_value(d.authored_candidate(|_| true).unwrap().receipt).unwrap();
    assert_eq!(old_receipt["version"], 5);
    let hash = inspect(&created).source_sha256;
    let hints = d.deletion_availability(&created, &hash, |_| true).unwrap();
    assert_eq!(hints.source_sha256, hash);
    assert_eq!(hints.anchors.len(), 5);
    let req = deletion(&created, &r.task_guid);
    d.delete_leaf(1, &req, |_| true).unwrap();
    let out = d.authored_candidate(|_| true).unwrap();
    let receipt = serde_json::to_value(out.receipt).unwrap();
    assert_eq!(receipt["version"], 7);
    assert_eq!(receipt["stages"][0], old_receipt["stages"][0]);
    assert_eq!(receipt["stages"][1]["kind"], "presentation_rows");
    assert_eq!(receipt["stages"][1]["receipt"]["source_sha256"], hash);
    assert_eq!(receipt["stages"][2]["kind"], "delete");
    assert_chain(&receipt);
    assert_eq!(d.undo_deletion_task_guid(), Some(r.task_guid.as_str()));
    let deleted = d.bytes(|_| true).unwrap();
    d.undo(2).unwrap();
    assert_eq!(d.bytes(|_| true).unwrap(), created);
    assert_eq!(d.authored_receipt_version(), 5);
    d.redo(3).unwrap();
    assert_eq!(d.bytes(|_| true).unwrap(), deleted);
    assert_eq!(container(&deleted, &mut |_| true).unwrap().decoded["Tasks"], container(&raw, &mut |_| true).unwrap().decoded["Tasks"]);
}

#[test]
fn existing_target_after_two_creations_completes_both_and_keeps_later_deletions_complete() {
    let raw = source(4);
    let original = inspect(&raw).tasks[0].guid.clone();
    let mut d = Draft::open(raw.clone(), &inspect(&raw).source_sha256, |_| true).unwrap();
    let a = insertion(&raw, 1);
    d.insert_above(0, &a, |_| true).unwrap();
    let b = insertion(&d.bytes(|_| true).unwrap(), 2);
    d.insert_above(1, &b, |_| true).unwrap();
    let current = d.bytes(|_| true).unwrap();
    d.delete_leaf(2, &deletion(&current, &original), |_| true).unwrap();
    let completed = d.bytes(|_| true).unwrap();
    let receipt = serde_json::to_value(d.authored_candidate(|_| true).unwrap().receipt).unwrap();
    assert_eq!(receipt["stages"][2]["receipt"]["added_rows"].as_array().unwrap().len(), 2);
    d.delete_leaf(3, &deletion(&completed, &a.task_guid), |_| true).unwrap();
    let receipt = serde_json::to_value(d.authored_candidate(|_| true).unwrap().receipt).unwrap();
    assert_eq!(receipt["stages"].as_array().unwrap().iter().filter(|s| s["kind"] == "presentation_rows").count(), 1);
    assert_chain(&receipt);
    assert!(d.creates_task(&b.task_guid));
    assert!(!d.creates_task(&a.task_guid));
}

#[test]
fn reused_created_guid_history_cues_follow_transactions_and_fields_keep_dynamic_offsets() {
    let raw = source(3);
    let r = insertion(&raw, 1);
    let mut d = Draft::open(raw.clone(), &inspect(&raw).source_sha256, |_| true).unwrap();
    d.insert_above(0, &r, |_| true).unwrap();
    let req = deletion(&d.bytes(|_| true).unwrap(), &r.task_guid);
    d.delete_leaf(1, &req, |_| true).unwrap();
    d.insert_above(2, &r, |_| true).unwrap();
    d.delete_leaf(3, &req, |_| true).unwrap();
    assert_eq!(d.undo_deletion_task_guid(), Some(r.task_guid.as_str()));
    d.undo(4).unwrap();
    assert!(d.creates_task(&r.task_guid));
    assert_eq!(d.redo_deletion().unwrap().task_guid, r.task_guid);
    d.undo(5).unwrap();
    assert!(!d.creates_task(&r.task_guid));
    assert_eq!(d.redo_creation_task_guid(), Some(r.task_guid.as_str()));
    d.redo(6).unwrap();
    d.apply_mixed(7, &[], &[TextEdit::TaskName { task_guid: r.task_guid.clone(), expected: r.name.clone(), value: "Renamed_งาน_梁_😀".into() }], |_| true)
        .unwrap();
    assert!(!d.can_redo());
    let current = d.bytes(|_| true).unwrap();
    d.delete_leaf(8, &deletion(&current, &r.task_guid), |_| true).unwrap();
    let receipt = serde_json::to_value(d.authored_candidate(|_| true).unwrap().receipt).unwrap();
    assert_chain(&receipt);
    assert_eq!(receipt["version"], 7);
}

#[test]
fn completion_deletion_cancellation_is_atomic_and_the_structural_budget_stays_shared() {
    let raw = source(3);
    let r = insertion(&raw, 1);
    let mut base = Draft::open(raw.clone(), &inspect(&raw).source_sha256, |_| true).unwrap();
    base.insert_above(0, &r, |_| true).unwrap();
    let current = base.bytes(|_| true).unwrap();
    let req = deletion(&current, &r.task_guid);
    let mut measure = base.clone();
    let mut total = 0;
    measure
        .delete_leaf(1, &req, |_| {
            total += 1;
            true
        })
        .unwrap();
    for stop in [1, total / 2, total - 1] {
        let mut d = base.clone();
        let mut calls = 0;
        assert!(
            d.delete_leaf(1, &req, |_| {
                calls += 1;
                calls < stop
            })
            .is_err()
        );
        assert_eq!(d.revision, 1);
        assert_eq!(d.bytes(|_| true).unwrap(), current);
    }
    let original = inspect(&raw).tasks[0].guid.clone();
    let mut d = Draft::open(raw.clone(), &inspect(&raw).source_sha256, |_| true).unwrap();
    for serial in 1..=7 {
        let r = insertion(&d.bytes(|_| true).unwrap(), serial);
        d.insert_above(u64::from(serial - 1), &r, |_| true).unwrap();
    }
    let current = d.bytes(|_| true).unwrap();
    d.delete_leaf(7, &deletion(&current, &original), |_| true).unwrap();
    let current = d.bytes(|_| true).unwrap();
    let guid = inspect(&current).tasks[0].guid.clone();
    assert!(d.delete_leaf(8, &deletion(&current, &guid), |_| true).is_err());
    assert_eq!(d.structural_edit_count(), 8);
    assert_eq!(d.bytes(|_| true).unwrap(), current);
}
