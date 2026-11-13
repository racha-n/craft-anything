use super::*;
fn inspect(raw: &[u8], mut keep: impl FnMut(f32) -> bool) -> Result<Evidence> {
    let hash = writer::hash(raw, &mut keep)?;
    inspect_container(container(raw, &mut keep)?, hash, false, &mut keep)
}
fn source(n: usize, selected: Option<usize>) -> Vec<u8> {
    craft_anything_testkit::project_authoring::default_work_project_with_presentation(n, selected)
}
fn request(raw: &[u8], index: usize) -> (String, DeleteLeaf) {
    let e = inspect(raw, |_| true).unwrap();
    let t = &e.tasks[index];
    (
        e.source_sha256,
        DeleteLeaf { task_guid: t.guid.clone(), expected_parent_guid: None, expected_sibling_index: index, expected_display_id: t.display_id.clone() },
    )
}
fn replace(raw: &[u8], name: &str, data: &[u8]) -> Vec<u8> {
    let e = inspect(raw, |_| true).unwrap();
    writer::pack_sections(raw, &e, &[(name, data)], &mut |_| true).unwrap().0
}
#[test]
fn deletion_preserves_all_remaining_bodies_rows_sections_and_clears_only_matching_selection() {
    for n in [2, 4, 110] {
        for index in [0, n / 2, n - 1] {
            for selected in [None, Some(index), Some((index + 1) % n)] {
                let raw = source(n, selected);
                let (hash, req) = request(&raw, index);
                let original = raw.clone();
                let c = candidate(&raw, &hash, &req, |_| true).unwrap_or_else(|e| panic!("n={n},index={index},selected={selected:?}: {e}"));
                assert_eq!(raw, original);
                assert_eq!(c.receipt.task_count_after, n - 1);
                assert_eq!(c.receipt.task_selection_cleared, selected == Some(index));
                assert_eq!(c.receipt.sections.iter().filter(|s| s.stored_bytes_preserved).count(), 46);
                assert!(!c.receipt.native_reopen_verified);
                assert_eq!(c.receipt.reference_sections_checked, 48);
                let e = inspect(&c.bytes, |_| true).unwrap();
                assert_eq!(e.tasks.len(), n - 1);
                assert!(!e.tasks.iter().any(|t| t.guid == req.task_guid));
            }
        }
    }
}
#[test]
fn stale_identity_position_display_parent_and_hash_fail_without_output() {
    let raw = source(3, Some(1));
    let (hash, good) = request(&raw, 1);
    for req in [
        DeleteLeaf { task_guid: "00000000-0000-0000-0000-000000000000".into(), ..good.clone() },
        DeleteLeaf { expected_sibling_index: 0, ..good.clone() },
        DeleteLeaf { expected_display_id: "stale".into(), ..good.clone() },
        DeleteLeaf { expected_parent_guid: Some(good.task_guid.clone()), ..good.clone() },
    ] {
        assert!(candidate(&raw, &hash, &req, |_| true).is_err());
    }
    assert!(candidate(&raw, &"0".repeat(64), &good, |_| true).is_err());
    assert!(candidate(&raw, &hash, &good, |_| false).is_err());
}
#[test]
fn unknown_reference_and_ambiguous_or_multiselection_presentation_are_rejected() {
    let raw = source(3, Some(1));
    let e = inspect(&raw, |_| true).unwrap();
    let (_, req) = request(&raw, 1);
    let section = e.sections.get("PresentationData").unwrap();
    let p = decoded_section(&raw, "PresentationData", section, &mut |_| true).unwrap();
    let task_id = bytes(&container(&raw, &mut |_| true).unwrap().decoded["Tasks"], e.tasks[1].offset + 12, 16).unwrap().to_vec();
    let mut variants = Vec::new();
    let mut unknown = p.clone();
    unknown.extend(&task_id);
    variants.push(unknown);
    let task_data = container(&raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap();
    let supply = bytes(&task_data, e.tasks[1].name_end + 98, 16).unwrap();
    let mut unknown_supply = p.clone();
    unknown_supply.extend(supply);
    variants.push(unknown_supply);
    let mut many = p.clone();
    many[20..24].copy_from_slice(&2u32.to_le_bytes());
    variants.push(many);
    let mut duplicate = p.clone();
    duplicate.extend(&p[48..]);
    variants.push(duplicate);
    let mut broken = p.clone();
    broken[0] ^= 1;
    variants.push(broken);
    for bad in variants {
        let raw = replace(&raw, "PresentationData", &bad);
        let hash = writer::hash(&raw, &mut |_| true).unwrap();
        assert!(candidate(&raw, &hash, &req, |_| true).is_err());
    }
}
#[test]
fn missing_presentation_last_task_unknown_build_and_truncations_are_rejected() {
    let raw = source(2, None);
    let (_, req) = request(&raw, 0);
    for size in [0, 40, 2343, raw.len() - 1] {
        let hash = writer::hash(&raw[..size], &mut |_| true).unwrap();
        assert!(candidate(&raw[..size], &hash, &req, |_| true).is_err());
    }
    let mut wrong_build = raw.clone();
    wrong_build[24] = b'7';
    let hash = writer::hash(&wrong_build, &mut |_| true).unwrap();
    assert!(candidate(&wrong_build, &hash, &req, |_| true).is_err());
    let single = source(1, None);
    let (hash, req) = request(&single, 0);
    assert!(candidate(&single, &hash, &req, |_| true).is_err());
    let raw = craft_anything_testkit::project_authoring::default_work_project(2);
    let (hash, req) = request(&raw, 0);
    assert!(candidate(&raw, &hash, &req, |_| true).is_err());
}

#[test]
fn references_in_an_unrelated_decoded_section_and_header_are_rejected() {
    let original = source(3, Some(1));
    let (_, req) = request(&original, 1);
    let e = inspect(&original, |_| true).unwrap();
    let t = &e.tasks[1];
    let data = container(&original, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap();
    let primary = bytes(&data, t.offset + 12, 16).unwrap();
    let mut sections: Vec<_> = e.sections.iter().collect();
    sections.sort_by_key(|(_, s)| s.file_offset);
    let mut raw = original[..2344].to_vec();
    for (i, (name, s)) in sections.iter().enumerate() {
        let offset = raw.len() as u64;
        raw[80 + i * 48..88 + i * 48].copy_from_slice(&offset.to_le_bytes());
        if name.as_str() == "Opaque47" {
            raw.extend([1, name.len() as u8]);
            raw.extend(name.as_bytes());
            raw.extend(miniz_oxide::deflate::compress_to_vec_zlib(primary, 6));
        } else {
            raw.extend(bytes(&original, s.file_offset, s.stored_bytes).unwrap());
        }
    }
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    let error = candidate(&raw, &hash, &req, |_| true).err().unwrap();
    assert!(error.to_string().contains("unsupported_reference"));
    assert!(error.to_string().contains("Opaque47"));
    // The directory/header is included in the scan as well as section payloads.
    let mut raw = original.clone();
    let at = 40 + 47 * 48;
    raw[at..at + 36].copy_from_slice(t.guid.as_bytes());
    raw[at + 36..at + 40].fill(0);
    let offset = e.sections["Opaque47"].file_offset;
    let name = t.guid.as_bytes();
    let old_prefix = 2 + "Opaque47".len();
    let mut rebuilt = raw[..offset].to_vec();
    rebuilt.extend([1, name.len() as u8]);
    rebuilt.extend(name);
    rebuilt.extend(&raw[offset + old_prefix..]);
    let hash = writer::hash(&rebuilt, &mut |_| true).unwrap();
    assert!(candidate(&rebuilt, &hash, &req, |_| true).err().unwrap().to_string().contains("unsupported_reference"));
}
#[test]
fn progressed_source_is_not_admitted_for_deletion() {
    let raw = source(3, None);
    let (_, req) = request(&raw, 1);
    let e = inspect(&raw, |_| true).unwrap();
    let mut tasks = container(&raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap();
    let at = e.tasks[0].display_id_end + 25;
    tasks[at..at + 4].copy_from_slice(&2u32.to_le_bytes());
    let raw = replace(&raw, "Tasks", &tasks);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    assert!(candidate(&raw, &hash, &req, |_| true).is_err());
}

#[test]
fn deletion_layout_hints_are_source_bound_and_reference_validation_stays_authoritative() {
    let raw = source(110, Some(100));
    let (hash, req) = request(&raw, 100);
    let hints = layout_availability(&raw, &hash, |_| true).unwrap();
    assert_eq!(hints.anchors.len(), 110);
    assert!(hints.reference_scan_required);
    assert_eq!(hints.anchors[100].task_guid, req.task_guid);
    assert_eq!(hints.anchors[100].expected_sibling_index, 100);
    assert!(layout_availability(&raw, &"0".repeat(64), |_| true).is_err());
    assert!(layout_availability(&raw, &hash, |_| false).is_err());
    let e = inspect(&raw, |_| true).unwrap();
    let mut view = decoded_section(&raw, "PresentationData", &e.sections["PresentationData"], &mut |_| true).unwrap();
    let taskdata = container(&raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap();
    view.extend(bytes(&taskdata, e.tasks[100].offset + 12, 16).unwrap());
    let modified = replace(&raw, "PresentationData", &view);
    let hash = writer::hash(&modified, &mut |_| true).unwrap();
    assert_eq!(layout_availability(&modified, &hash, |_| true).unwrap().anchors.len(), 110);
    assert!(candidate(&modified, &hash, &req, |_| true).is_err());
    let single = source(1, None);
    let hash = writer::hash(&single, &mut |_| true).unwrap();
    assert!(layout_availability(&single, &hash, |_| true).unwrap().anchors.is_empty());
}

#[test]
fn deletion_draft_undo_redo_preserves_guids_bytes_and_requires_v6_export() {
    use crate::sp_project::draft::Draft;
    let raw = source(4, Some(1));
    let (hash, req) = request(&raw, 1);
    let expected = candidate(&raw, &hash, &req, |_| true).unwrap();
    let mut d = Draft::open(raw.clone(), &hash, |_| true).unwrap();
    d.delete_leaf(0, &req, |_| true).unwrap();
    assert_eq!(d.bytes(|_| true).unwrap(), expected.bytes);
    assert!(d.has_authored_edits() && d.has_deletion_edits() && !d.has_creation_edits());
    assert_eq!(d.undo_deletion_task_guid(), Some(req.task_guid.as_str()));
    assert_eq!(d.authored_candidate(|_| true).unwrap().receipt.version, 6);
    assert!(d.candidate(|_| true).is_err());
    assert!(d.ordered_candidate(|_| true).is_err());
    assert!(d.structural_candidate(|_| true).is_err());
    d.undo(1).unwrap();
    assert_eq!(d.bytes(|_| true).unwrap(), raw);
    assert!(!d.is_dirty());
    assert_eq!(d.redo_deletion().unwrap().task_guid, req.task_guid);
    d.redo(2).unwrap();
    assert_eq!(d.bytes(|_| true).unwrap(), expected.bytes);
    assert!(d.delete_leaf(3, &req, |_| true).is_err());
    assert_eq!(d.revision, 3);
    assert_eq!(d.bytes(|_| true).unwrap(), expected.bytes);
}

#[test]
fn deletion_field_order_stages_retain_chronology_and_hash_chain() {
    use crate::sp_project::{draft::Draft, task_strings::TextEdit};
    let raw = source(4, None);
    let (hash, req) = request(&raw, 0);
    let mut d = Draft::open(raw, &hash, |_| true).unwrap();
    d.delete_leaf(0, &req, |_| true).unwrap();
    let current = d.bytes(|_| true).unwrap();
    let e = inspect(&current, |_| true).unwrap();
    let renamed = &e.tasks[0];
    let moved = &e.tasks[2];
    d.apply_structural(1, &[], &[TextEdit::TaskName { task_guid: renamed.guid.clone(), expected: renamed.name.clone(), value: "Renamed".into() }], &[], |_| {
        true
    })
    .unwrap();
    d.apply_structural(
        2,
        &[],
        &[],
        &[hierarchy::Reparent { task_guid: moved.guid.clone(), expected_parent_guid: None, expected_sibling_index: 2, parent_guid: None, sibling_index: 0 }],
        |_| true,
    )
    .unwrap();
    let out = d.authored_candidate(|_| true).unwrap();
    let json = serde_json::to_value(out.receipt).unwrap();
    assert_eq!(json["version"], 6);
    assert_eq!(json["stages"].as_array().unwrap().iter().map(|x| x["kind"].as_str().unwrap()).collect::<Vec<_>>(), ["delete", "fields", "order"]);
    let mut previous = json["prefix"]["output_sha256"].as_str().unwrap();
    for s in json["stages"].as_array().unwrap() {
        assert_eq!(s["receipt"]["source_sha256"], previous);
        previous = s["receipt"]["output_sha256"].as_str().unwrap();
    }
    assert_eq!(json["output_sha256"], previous);
    let before = d.bytes(|_| true).unwrap();
    assert!(
        d.apply_structural(3, &[], &[TextEdit::TaskName { task_guid: req.task_guid, expected: "Task1".into(), value: "Missing".into() }], &[], |_| true)
            .is_err()
    );
    assert_eq!(d.bytes(|_| true).unwrap(), before);
    assert_eq!(d.revision, 3);
}

#[test]
fn deletion_history_limits_last_root_and_post_creation_completion_remain_guarded() {
    use crate::sp_project::draft::Draft;
    let raw = source(10, None);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    let mut d = Draft::open(raw, &hash, |_| true).unwrap();
    for revision in 0..8 {
        let current = d.bytes(|_| true).unwrap();
        let (_, req) = request(&current, 0);
        d.delete_leaf(revision, &req, |_| true).unwrap();
    }
    let current = d.bytes(|_| true).unwrap();
    let (_, req) = request(&current, 0);
    assert!(d.delete_leaf(8, &req, |_| true).is_err());
    assert_eq!(d.deletion_count(), 8);
    assert_eq!(d.bytes(|_| true).unwrap(), current);
    let raw = source(2, None);
    let (hash, req) = request(&raw, 0);
    let mut d = Draft::open(raw, &hash, |_| true).unwrap();
    d.delete_leaf(0, &req, |_| true).unwrap();
    let current = d.bytes(|_| true).unwrap();
    let (_, req) = request(&current, 0);
    assert!(d.delete_leaf(1, &req, |_| true).is_err());
    let raw = source(4, None);
    let (hash, req) = request(&raw, 0);
    let mut d = Draft::open(raw, &hash, |_| true).unwrap();
    d.delete_leaf(0, &req, |_| true).unwrap();
    let current = d.bytes(|_| true).unwrap();
    let e = inspect(&current, |_| true).unwrap();
    let anchor = &e.tasks[0];
    d.insert_above(
        1,
        &creation::InsertAbove {
            template_task_guid: anchor.guid.clone(),
            anchor_task_guid: anchor.guid.clone(),
            expected_parent_guid: None,
            expected_sibling_index: 0,
            task_guid: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa".into(),
            supply_chain_id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb".into(),
            name: "New".into(),
            display_id: "NEW".into(),
        },
        |_| true,
    )
    .unwrap();
    let current = d.bytes(|_| true).unwrap();
    let (_, req) = request(&current, 0);
    assert_eq!(d.authored_candidate(|_| true).unwrap().receipt.version, 6);
    assert!(candidate(&current, &inspect(&current, |_| true).unwrap().source_sha256, &req, |_| true).is_err());
    d.delete_leaf(2, &req, |_| true).unwrap();
    assert_eq!(d.revision, 3);
    assert_eq!(d.authored_candidate(|_| true).unwrap().receipt.version, 7);
    d.undo(3).unwrap();
    assert_eq!(d.bytes(|_| true).unwrap(), current);
}
