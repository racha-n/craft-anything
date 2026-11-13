use super::*;
use crate::sp_project::tests::{empty_section, metadata_task_section, project_with_tasks};

fn id(byte: u8) -> String {
    let b = format!("{byte:02x}");
    format!("{}-{}-{}-{}-{}", b.repeat(4), b.repeat(2), b.repeat(2), b.repeat(2), b.repeat(6))
}
fn record(byte: u8, lookup_index: u32) -> Vec<u8> {
    let original = metadata_task_section(false);
    let mut task = original[32..original.len() - 4].to_vec();
    task[12..28].fill(byte);
    let end = task.len();
    task[end - 4..].copy_from_slice(&lookup_index.to_le_bytes());
    task
}
fn section(records: &[Vec<u8>]) -> Vec<u8> {
    let mut data = empty_section();
    data.truncate(28);
    data.extend((records.len() as u32).to_le_bytes());
    for task in records {
        data.extend(task);
    }
    data.extend(CLOSE);
    data
}
fn fixture() -> Vec<u8> {
    project_with_tasks(empty_section(), section(&[record(1, 0), record(2, 1), record(3, 2)]))
}
fn movement(task: u8, parent: Option<u8>, old_index: usize, index: usize) -> Move {
    Move { task_guid: id(task), expected_parent_guid: parent.map(id), expected_sibling_index: old_index, sibling_index: index }
}
fn apply(raw: &[u8], m: &Move) -> Candidate {
    let hash = writer::hash(raw, &mut |_| true).unwrap();
    candidate(raw, &hash, m, |_| true).unwrap()
}
fn payload(raw: &[u8]) -> Vec<u8> {
    container(raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap()
}
fn nest(mut parent: Vec<u8>, children: &[Vec<u8>]) -> Vec<u8> {
    let parsed = tasks(&section(&[parent.clone()]), &mut |_| true).unwrap();
    let at = parsed[0].name_end + 17 - 32;
    parent[at - 1] = 1;
    let collection = section(children);
    parent.splice(at..at, collection[24..].iter().copied());
    parent
}

#[test]
fn three_cycle_rebuilds_lookup_vector_and_preserves_task_content_and_other_sections() {
    let raw = fixture();
    let moved = apply(&raw, &movement(3, None, 2, 0));
    let expected = project_with_tasks(empty_section(), section(&[record(3, 1), record(1, 2), record(2, 0)]));
    assert_eq!(moved.bytes, expected);
    assert!(moved.receipt.sections.iter().filter(|s| s.name != "Tasks").all(|s| s.stored_bytes_preserved));
    assert!(!moved.receipt.task_subtree_bytes_preserved);
    assert!(moved.receipt.task_content_bytes_preserved);
    assert!(moved.receipt.sibling_lookup_index_validated);
    assert!(moved.receipt.sibling_lookup_index_rebuilt);
    assert!(!moved.receipt.native_reopen_verified);
    assert!(!moved.receipt.derived_schedule_recalculated);
    assert_eq!(apply(&moved.bytes, &movement(3, None, 0, 2)).bytes, raw);
    assert_eq!(apply(&raw, &movement(2, None, 1, 1)).bytes, raw);
}

#[test]
fn sibling_reorder_keeps_descendants_and_unrelated_roots_exact() {
    let child = nest(record(2, 0), &[record(4, 0)]);
    let parent = nest(record(1, 0), &[child.clone(), record(3, 1)]);
    let raw = project_with_tasks(empty_section(), section(&[parent, record(5, 1)]));
    let moved = apply(&raw, &movement(3, Some(1), 1, 0));
    let mut child = child;
    let end = child.len();
    child[end - 4..].copy_from_slice(&0u32.to_le_bytes());
    let expected = project_with_tasks(empty_section(), section(&[nest(record(1, 0), &[record(3, 1), child]), record(5, 1)]));
    assert_eq!(moved.bytes, expected);
    assert_eq!(apply(&moved.bytes, &movement(3, Some(1), 0, 1)).bytes, raw);
    let swapped = apply(&raw, &movement(5, None, 1, 0));
    assert_eq!(payload(&raw).len(), payload(&swapped.bytes).len());
    assert_eq!(parse_metadata(&swapped.bytes, |_| true).unwrap().metadata["tasks"][0]["guid"], id(5));
}

#[test]
fn source_parent_position_range_and_cancellation_are_guarded() {
    let raw = fixture();
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    let good = movement(2, None, 1, 0);
    assert!(candidate(&raw, &"0".repeat(64), &good, |_| true).is_err());
    assert!(candidate(&raw, &hash, &good, |_| false).is_err());
    for bad in [movement(2, Some(1), 1, 0), movement(2, None, 0, 0), movement(2, None, 1, 3), movement(9, None, 1, 0)] {
        assert!(candidate(&raw, &hash, &bad, |_| true).is_err());
    }
    let unsupported = serde_json::json!({"task_guid":id(2),"expected_parent_guid":null,"expected_sibling_index":1,"parent_guid":id(1),"sibling_index":0});
    assert!(serde_json::from_value::<Move>(unsupported).is_err());
}

#[test]
fn lookup_uses_guid_storage_bytes_and_refuses_invalid_vectors_even_for_noop() {
    let mut a = record(1, 1);
    let mut b = record(2, 0);
    // Printed GUID order differs from the lexicographic order of its stored bytes.
    a[12..16].copy_from_slice(&0x00000100u32.to_le_bytes());
    b[12..16].copy_from_slice(&0x00000001u32.to_le_bytes());
    // a's stored prefix starts 00, b's starts 01: lookup must be [0, 1].
    let invalid = project_with_tasks(empty_section(), section(&[a.clone(), b.clone()]));
    let hash = writer::hash(&invalid, &mut |_| true).unwrap();
    assert!(matches!(outline(&invalid, &hash, |_| true).unwrap().reason, Some(ScopeReason::InvalidSiblingLookup)));
    let guid = tasks(&payload(&invalid), &mut |_| true).unwrap()[0].guid.clone();
    let request = Move { task_guid: guid, expected_parent_guid: None, expected_sibling_index: 0, sibling_index: 0 };
    assert!(candidate(&invalid, &hash, &request, |_| true).is_err());
    let end = a.len();
    a[end - 4..].copy_from_slice(&0u32.to_le_bytes());
    let end = b.len();
    b[end - 4..].copy_from_slice(&1u32.to_le_bytes());
    let valid = project_with_tasks(empty_section(), section(&[a, b]));
    let hash = writer::hash(&valid, &mut |_| true).unwrap();
    assert!(outline(&valid, &hash, |_| true).unwrap().reason.is_none());
    assert_eq!(candidate(&valid, &hash, &request, |_| true).unwrap().bytes, valid);
}

#[test]
fn sequential_moves_keep_every_guid_lookup_correct_from_nonidentity_order() {
    let mut raw = fixture();
    for request in [movement(3, None, 2, 0), movement(1, None, 1, 2), movement(2, None, 1, 0), movement(3, None, 1, 2)] {
        raw = apply(&raw, &request).bytes;
        let data = payload(&raw);
        let rows = tasks(&data, &mut |_| true).unwrap();
        // Independent lookup: follow each serialized index and require ascending raw GUIDs.
        let mut reached = Vec::new();
        for row in &rows {
            let index = u32_at(&data, row.offset + row.bytes - 4).unwrap() as usize;
            let target = rows.get(index).unwrap();
            reached.push(data[target.offset + 12..target.offset + 28].to_vec());
        }
        assert_eq!(reached, vec![vec![1; 16], vec![2; 16], vec![3; 16]]);
    }
}

#[test]
fn refuses_linked_tasks_and_valid_sources_exceeding_the_task_budget() {
    let mut linked_tasks = metadata_task_section(true);
    // Bind the predecessor to the existing task so parser reference validation passes.
    let predecessor = tasks(&linked_tasks, &mut |_| true).unwrap()[0].links[0].offset + 28;
    linked_tasks[predecessor..predecessor + 16].fill(0x11);
    let linked = project_with_tasks(empty_section(), linked_tasks);
    assert!(parse_metadata(&linked, |_| true).is_ok());
    let h = writer::hash(&linked, &mut |_| true).unwrap();
    assert!(matches!(outline(&linked, &h, |_| true).unwrap().reason, Some(ScopeReason::AssociatedOrProgressedTasks)));
    assert!(candidate(&linked, &h, &movement(1, None, 0, 0), |_| true).is_err());
    let mut records = Vec::new();
    for i in 0u16..513 {
        let mut task = record(1, 0);
        task[12..28].fill(0);
        task[12..14].copy_from_slice(&(i + 1).to_le_bytes());
        records.push(task);
    }
    let many = project_with_tasks(empty_section(), section(&records));
    assert_eq!(parse_metadata(&many, |_| true).unwrap().metadata["tasks"].as_array().unwrap().len(), 513);
    let h = writer::hash(&many, &mut |_| true).unwrap();
    let positions = outline(&many, &h, |_| true).unwrap();
    assert!(matches!(positions.reason, Some(ScopeReason::TaskLimit)));
    assert_eq!(positions.positions.len(), 513);
    assert!(candidate(&many, &h, &movement(1, None, 0, 0), |_| true).is_err());
}

#[test]
fn outline_reports_complete_parent_positions_and_shared_admission() {
    let raw = project_with_tasks(empty_section(), section(&[nest(record(1, 0), &[nest(record(2, 0), &[record(4, 0)]), record(3, 1)]), record(5, 1)]));
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    let result = outline(&raw, &hash, |_| true).unwrap();
    assert!(result.reason.is_none());
    let rows: Vec<_> = result.positions.iter().map(|p| (p.task_guid.clone(), p.parent_guid.clone(), p.sibling_index, p.sibling_total)).collect();
    assert_eq!(rows, vec![(id(1), None, 0, 2), (id(2), Some(id(1)), 0, 2), (id(4), Some(id(2)), 0, 1), (id(3), Some(id(1)), 1, 2), (id(5), None, 1, 2)]);
    assert!(outline(&raw, &"0".repeat(64), |_| true).is_err());
    assert!(outline(&raw, &hash, |_| false).is_err());
    let empty = project_with_tasks(empty_section(), empty_section());
    let hash = writer::hash(&empty, &mut |_| true).unwrap();
    let result = outline(&empty, &hash, |_| true).unwrap();
    assert!(matches!(result.reason, Some(ScopeReason::NoTasks)));
    assert!(result.positions.is_empty());
}
