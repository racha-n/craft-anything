use super::*;
use crate::sp_project::tests::{empty_section, metadata_task_section, project_with_tasks};

fn id(byte: u8) -> String {
    let b = format!("{byte:02x}");
    format!("{}-{}-{}-{}-{}", b.repeat(4), b.repeat(2), b.repeat(2), b.repeat(2), b.repeat(6))
}
fn record(byte: u8) -> Vec<u8> {
    let source = metadata_task_section(false);
    let mut row = source[32..source.len() - 4].to_vec();
    row[12..28].fill(byte);
    row
}
fn section(rows: &[Vec<u8>]) -> Vec<u8> {
    let mut data = empty_section();
    data.truncate(28);
    data.extend((rows.len() as u32).to_le_bytes());
    let mut lookup: Vec<usize> = (0..rows.len()).collect();
    lookup.sort_by_key(|i| &rows[*i][12..28]);
    for (row, index) in rows.iter().zip(lookup) {
        data.extend(&row[..row.len() - 4]);
        data.extend((index as u32).to_le_bytes());
    }
    data.extend(CLOSE);
    data
}
fn nest(mut parent: Vec<u8>, children: &[Vec<u8>]) -> Vec<u8> {
    let parsed = tasks(&section(&[parent.clone()]), &mut |_| true).unwrap();
    let at = parsed[0].name_end + 17 - 32;
    parent[at - 1] = 1;
    let collection = section(children);
    parent.splice(at..at, collection[24..].iter().copied());
    parent
}
fn project(rows: &[Vec<u8>]) -> Vec<u8> {
    project_with_tasks(empty_section(), section(rows))
}
fn request(task: u8, old_parent: Option<u8>, old_index: usize, parent: Option<u8>, index: usize) -> Reparent {
    Reparent {
        task_guid: id(task),
        expected_parent_guid: old_parent.map(id),
        expected_sibling_index: old_index,
        parent_guid: parent.map(id),
        sibling_index: index,
    }
}
fn apply(raw: &[u8], request: &Reparent) -> Candidate {
    candidate(raw, &writer::hash(raw, &mut |_| true).unwrap(), request, |_| true).unwrap()
}
fn payload(raw: &[u8]) -> Vec<u8> {
    container(raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap()
}

#[test]
fn indent_and_outdent_preserve_body_fields_and_all_other_sections() {
    let raw = project(&[record(1), record(2), record(3)]);
    let result = apply(&raw, &request(2, None, 1, Some(1), 0));
    assert_eq!(payload(&result.bytes), section(&[nest(record(1), &[record(2)]), record(3)]));
    assert!(result.receipt.task_body_fields_preserved && result.receipt.task_guids_preserved);
    assert!(result.receipt.collection_lookup_indices_rebuilt && result.receipt.collection_lookup_indices_validated);
    assert!(!result.receipt.native_reopen_verified && !result.receipt.derived_schedule_recalculated);
    assert!(result.receipt.sections.iter().filter(|s| s.name != "Tasks").all(|s| s.stored_bytes_preserved));
    let inverse = apply(&result.bytes, &request(2, Some(1), 0, None, 1));
    assert_eq!(payload(&inverse.bytes), payload(&raw));
}

#[test]
fn moves_whole_subtree_between_nonidentity_collections_and_removes_empty_child_frame() {
    let subtree = nest(record(2), &[record(5), record(4)]);
    let raw = project(&[record(3), nest(record(1), std::slice::from_ref(&subtree)), record(6)]);
    let result = apply(&raw, &request(2, Some(1), 0, Some(3), 0));
    assert_eq!(payload(&result.bytes), section(&[nest(record(3), &[subtree]), record(1), record(6)]));
    let parsed = parse_metadata(&result.bytes, |_| true).unwrap();
    let rows = parsed.metadata["tasks"].as_array().unwrap();
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[1]["parent_guid"], id(3));
    assert_eq!(rows[2]["parent_guid"], id(2));
    assert_eq!(rows[3]["parent_guid"], id(2));
}

#[test]
fn inserts_before_existing_children_without_changing_their_order() {
    let raw = project(&[record(1), nest(record(2), &[record(4), record(3)])]);
    let result = apply(&raw, &request(1, None, 0, Some(2), 1));
    assert_eq!(payload(&result.bytes), section(&[nest(record(2), &[record(4), record(1), record(3)])]));
}

#[test]
fn refuses_stale_identity_positions_self_descendants_and_unchanged_parent() {
    let raw = project(&[nest(record(1), &[nest(record(2), &[record(4)])]), record(3)]);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    for bad in [
        request(2, None, 0, Some(3), 0),
        request(2, Some(1), 1, Some(3), 0),
        request(2, Some(1), 0, Some(3), 1),
        request(2, Some(1), 0, Some(2), 0),
        request(1, None, 0, Some(4), 0),
        request(2, Some(1), 0, Some(1), 0),
        request(9, None, 0, Some(3), 0),
    ] {
        assert!(candidate(&raw, &hash, &bad, |_| true).is_err());
    }
    let good = request(2, Some(1), 0, Some(3), 0);
    assert!(candidate(&raw, &"0".repeat(64), &good, |_| true).is_err());
    assert!(candidate(&raw, &hash, &good, |_| false).is_err());
    assert_eq!(hash, writer::hash(&raw, &mut |_| true).unwrap());
}

#[test]
fn refuses_corrupt_lookup_and_unsupported_associated_tasks() {
    let mut bad = section(&[record(1), record(2)]);
    let first = tasks(&bad, &mut |_| true).unwrap().remove(0);
    bad[first.offset + first.bytes - 4..first.offset + first.bytes].copy_from_slice(&1u32.to_le_bytes());
    let raw = project_with_tasks(empty_section(), bad);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    assert!(candidate(&raw, &hash, &request(2, None, 1, Some(1), 0), |_| true).is_err());
    let mut linked = metadata_task_section(true);
    let predecessor = tasks(&linked, &mut |_| true).unwrap()[0].links[0].offset + 28;
    linked[predecessor..predecessor + 16].fill(0x11);
    let raw = project_with_tasks(empty_section(), linked);
    assert!(parse_metadata(&raw, |_| true).is_ok());
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    assert!(candidate(&raw, &hash, &request(0x11, None, 0, None, 0), |_| true).is_err());
}

#[test]
fn noncanonical_empty_child_lists_are_not_silently_normalized() {
    let raw = project(&[nest(record(1), &[]), record(2)]);
    assert!(parse_metadata(&raw, |_| true).is_ok());
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    assert!(candidate(&raw, &hash, &request(2, None, 1, Some(1), 0), |_| true).is_err());
}

#[test]
fn refuses_a_move_that_exceeds_the_depth_limit() {
    let mut tree = record(65);
    for i in (1..65).rev() {
        tree = nest(record(i), &[tree]);
    }
    let raw = project(&[tree, record(66)]);
    assert_eq!(tasks(&payload(&raw), &mut |_| true).unwrap().len(), 66);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    assert!(candidate(&raw, &hash, &request(66, None, 1, Some(65), 0), |_| true).is_err());
    let allowed = apply(&raw, &request(66, None, 1, Some(64), 1));
    assert_eq!(tasks(&payload(&allowed.bytes), &mut |_| true).unwrap().len(), 66);
}

#[test]
fn extended_unicode_names_and_ids_remain_exact_when_their_parent_changes() {
    let raw = project(&[record(1), record(2)]);
    let label = "\u{0e07}\u{6881}\u{1f600}".repeat(100);
    let edits = [
        task_strings::TextEdit::TaskName { task_guid: id(2), expected: "A".into(), value: label.clone() },
        task_strings::TextEdit::TaskDisplayId { task_guid: id(2), expected: "1".into(), value: "x".repeat(300) },
    ];
    let text = task_strings::candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &edits, |_| true).unwrap();
    let moved = apply(&text.bytes, &request(2, None, 1, Some(1), 0));
    let parsed = parse_metadata(&moved.bytes, |_| true).unwrap();
    assert_eq!(parsed.metadata["tasks"][1]["name"], label);
    assert_eq!(parsed.metadata["tasks"][1]["display_id"], "x".repeat(300));
}
