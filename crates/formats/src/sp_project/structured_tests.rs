use super::*;
use crate::sp_project::{
    draft::Draft,
    parse_metadata,
    tests::{empty_section, metadata_task_section, project_with_tasks},
};

fn id(byte: u8) -> String {
    let b = format!("{byte:02x}");
    format!("{}-{}-{}-{}-{}", b.repeat(4), b.repeat(2), b.repeat(2), b.repeat(2), b.repeat(6))
}
fn fixture() -> Vec<u8> {
    let source = metadata_task_section(false);
    let mut data = empty_section();
    data.truncate(28);
    data.extend(3u32.to_le_bytes());
    for i in 1..=3u8 {
        let mut row = source[32..source.len() - 4].to_vec();
        row[12..28].fill(i);
        let end = row.len();
        row[end - 4..].copy_from_slice(&(u32::from(i) - 1).to_le_bytes());
        data.extend(row);
    }
    data.extend(super::super::CLOSE);
    project_with_tasks(empty_section(), data)
}
fn hash(raw: &[u8]) -> String {
    writer::hash(raw, &mut |_| true).unwrap()
}
fn move_to(task: u8, old: Option<u8>, from: usize, parent: Option<u8>, to: usize) -> hierarchy::Reparent {
    hierarchy::Reparent { task_guid: id(task), expected_parent_guid: old.map(id), expected_sibling_index: from, parent_guid: parent.map(id), sibling_index: to }
}
fn rows(raw: &[u8]) -> Vec<serde_json::Value> {
    parse_metadata(raw, |_| true).unwrap().metadata["tasks"].as_array().unwrap().clone()
}

#[test]
fn mixed_fields_and_interleaved_relocations_retain_typed_receipts_and_hash_chain() {
    let raw = fixture();
    let h = hash(&raw);
    let fields = [writer::ScalarEdit::TaskDuration { task_guid: id(2), expected_storage: 0, value_storage: 57600 }];
    let texts = [task_strings::TextEdit::TaskName { task_guid: id(2), expected: "A".into(), value: "Nested_\u{0e07}\u{6881}\u{1f600}".into() }];
    let ops = [move_to(2, None, 1, Some(1), 0), move_to(3, None, 1, None, 0), move_to(2, Some(1), 0, None, 2)];
    let result = candidate(&raw, &h, &fields, &texts, &ops, |_| true).unwrap();
    let r = &result.receipt;
    assert_eq!(r.version, 4);
    assert_eq!(r.source_sha256, h);
    assert_eq!(r.structural_stages.len(), 3);
    let mut previous = r.mixed_stage.output_sha256.clone();
    for stage in &r.structural_stages {
        let (source, output, sections) = match stage {
            Stage::Order(s) => {
                assert_eq!(s.version, 2);
                (&s.source_sha256, &s.output_sha256, &s.sections)
            }
            Stage::Hierarchy(s) => {
                assert_eq!(s.version, 1);
                (&s.source_sha256, &s.output_sha256, &s.sections)
            }
        };
        assert_eq!(source, &previous);
        previous = output.clone();
        assert!(sections.iter().filter(|s| s.name != "Tasks").all(|s| s.stored_bytes_preserved));
    }
    assert!(matches!(&r.structural_stages[0], Stage::Hierarchy(_)));
    assert!(matches!(&r.structural_stages[1], Stage::Order(_)));
    assert!(matches!(&r.structural_stages[2], Stage::Hierarchy(_)));
    assert_eq!(previous, r.output_sha256);
    assert_eq!(hash(&result.bytes), r.output_sha256);
    assert!(!r.native_reopen_verified && !r.derived_schedule_consistency_verified);
    let tasks = rows(&result.bytes);
    assert_eq!(tasks.iter().map(|t| t["guid"].as_str().unwrap().to_owned()).collect::<Vec<_>>(), vec![id(3), id(1), id(2)]);
    assert!(tasks.iter().all(|t| t["parent_guid"].is_null()));
    assert_eq!(tasks[2]["duration_storage_scalar"], 57600);
    assert!(tasks[2]["name"].as_str().unwrap().ends_with('\u{1f600}'));
}

#[test]
fn a_parent_change_with_identical_preorder_remains_dirty_and_cannot_use_legacy_exports() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_structural(0, &[], &[], &[move_to(2, None, 1, Some(1), 0)], |_| true).unwrap();
    assert!(draft.is_dirty() && draft.has_hierarchy_edits());
    assert_eq!(draft.edit_count(), 1);
    assert_eq!(draft.structural_edits().len(), 1);
    assert!(draft.candidate(|_| true).is_err());
    assert!(draft.ordered_candidate(|_| true).is_err());
    assert_eq!(draft.structural_candidate(|_| true).unwrap().receipt.version, 4);
    let changed = draft.bytes(|_| true).unwrap();
    let task = rows(&changed);
    assert_eq!(task[1]["guid"], id(2));
    assert_eq!(task[1]["parent_guid"], id(1));
    draft.undo(1).unwrap();
    assert_eq!(draft.bytes(|_| true).unwrap(), raw);
    draft.redo(2).unwrap();
    assert_eq!(draft.bytes(|_| true).unwrap(), changed);
}

#[test]
fn inverse_graph_clears_structural_dirty_state_but_keeps_undo_history() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_structural(0, &[], &[], &[move_to(2, None, 1, Some(1), 0)], |_| true).unwrap();
    draft.apply_structural(1, &[], &[], &[move_to(2, Some(1), 0, None, 1)], |_| true).unwrap();
    assert!(!draft.is_dirty());
    assert!(draft.structural_edits().is_empty() && draft.order_edits().is_empty());
    assert_eq!(draft.bytes(|_| true).unwrap(), raw);
    draft.undo(2).unwrap();
    assert!(draft.has_hierarchy_edits());
    assert_eq!(rows(&draft.bytes(|_| true).unwrap())[1]["parent_guid"], id(1));
}

#[test]
fn legacy_order_calls_append_to_existing_hierarchy_without_dropping_prior_steps() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_structural(0, &[], &[], &[move_to(2, None, 1, Some(1), 0)], |_| true).unwrap();
    let order = relocation::Move { task_guid: id(3), expected_parent_guid: None, expected_sibling_index: 1, sibling_index: 0 };
    draft.apply_ordered(1, &[], &[], &[order], |_| true).unwrap();
    draft.apply_structural(2, &[], &[], &[move_to(2, Some(1), 0, None, 2)], |_| true).unwrap();
    assert_eq!(draft.structural_edits().len(), 3);
    assert!(draft.has_hierarchy_edits());
    assert!(draft.ordered_candidate(|_| true).is_err());
    let tasks = rows(&draft.bytes(|_| true).unwrap());
    assert_eq!(tasks[0]["guid"], id(3));
    assert_eq!(tasks[2]["guid"], id(2));
    assert!(tasks.iter().all(|t| t["parent_guid"].is_null()));
}

#[test]
fn structural_operation_budget_is_shared_across_order_and_parent_changes() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_structural(0, &[], &[], &[move_to(2, None, 1, Some(1), 0)], |_| true).unwrap();
    for i in 1..8 {
        let (from, to) = if i % 2 == 1 { (1, 0) } else { (0, 1) };
        let order = relocation::Move { task_guid: id(3), expected_parent_guid: None, expected_sibling_index: from, sibling_index: to };
        draft.apply_ordered(draft.revision, &[], &[], &[order], |_| true).unwrap();
    }
    assert_eq!(draft.structural_edits().len(), 8);
    let before = draft.bytes(|_| true).unwrap();
    let revision = draft.revision;
    assert!(draft.apply_structural(revision, &[], &[], &[move_to(3, None, 0, None, 1)], |_| true).is_err());
    assert_eq!(draft.revision, revision);
    assert_eq!(draft.bytes(|_| true).unwrap(), before);
}

#[test]
fn failed_mixed_parent_change_preserves_revision_fields_and_history() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_structural(0, &[], &[], &[move_to(2, None, 1, Some(1), 0)], |_| true).unwrap();
    let before = draft.bytes(|_| true).unwrap();
    let fields = [writer::ScalarEdit::TaskDuration { task_guid: id(2), expected_storage: 0, value_storage: 28800 }];
    assert!(draft.apply_structural(1, &fields, &[], &[move_to(2, None, 1, Some(3), 0)], |_| true).is_err());
    assert_eq!(draft.revision, 1);
    assert!(draft.edits().is_empty());
    assert_eq!(draft.bytes(|_| true).unwrap(), before);
    assert!(candidate(&raw, &hash(&raw), &[], &[], &[move_to(2, None, 1, Some(1), 0)], |_| false).is_err());
}
