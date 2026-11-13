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
