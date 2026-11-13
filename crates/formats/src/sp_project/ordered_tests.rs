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
    let record = &source[32..source.len() - 4];
    let mut data = empty_section();
    data.truncate(28);
    data.extend(3u32.to_le_bytes());
    for i in 1..=3 {
        let mut task = record.to_vec();
        task[12..28].fill(i);
        let end = task.len();
        task[end - 4..].copy_from_slice(&(u32::from(i) - 1).to_le_bytes());
        data.extend(task);
    }
    data.extend(super::super::CLOSE);
    project_with_tasks(empty_section(), data)
}
fn movement(task: u8, from: usize, to: usize) -> relocation::Move {
    relocation::Move { task_guid: id(task), expected_parent_guid: None, expected_sibling_index: from, sibling_index: to }
}
fn hash(raw: &[u8]) -> String {
    parse_metadata(raw, |_| true).unwrap().source_sha256
}
fn name() -> task_strings::TextEdit {
    task_strings::TextEdit::TaskName { task_guid: id(2), expected: "A".into(), value: "Moved_\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}".into() }
}

#[test]
fn ordered_receipt_chains_original_field_stages_and_each_move() {
    let raw = fixture();
    let scalar = writer::ScalarEdit::TaskDuration { task_guid: id(2), expected_storage: 0, value_storage: 28800 };
    let result = candidate(&raw, &hash(&raw), &[scalar], &[name()], &[movement(2, 1, 0), movement(3, 2, 1)], |_| true).unwrap();
    let r = &result.receipt;
    assert_eq!(r.version, 3);
    assert_eq!(r.source_sha256, hash(&raw));
    assert_eq!(r.output_sha256, hash(&result.bytes));
    assert_eq!(r.mixed_stage.source_sha256, r.source_sha256);
    assert_eq!(r.mixed_stage.output_sha256, r.order_stages[0].source_sha256);
    assert_eq!(r.order_stages[0].output_sha256, r.order_stages[1].source_sha256);
    assert_eq!(r.order_stages[1].output_sha256, r.output_sha256);
    assert!(!r.native_reopen_verified);
    assert!(!r.derived_schedule_consistency_verified);
    let parsed = parse_metadata(&result.bytes, |_| true).unwrap();
    let tasks = parsed.metadata["tasks"].as_array().unwrap();
    assert_eq!(tasks[0]["guid"], id(2));
    assert_eq!(tasks[1]["guid"], id(3));
    assert_eq!(tasks[0]["duration_storage_scalar"], 28800);
    assert!(tasks[0]["name"].as_str().unwrap().ends_with('\u{1f600}'));
}

#[test]
fn shared_draft_undo_redo_branch_and_failed_batch_preserve_atomic_state() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_ordered(0, &[], &[name()], &[movement(2, 1, 0)], |_| true).unwrap();
    let moved = draft.bytes(|_| true).unwrap();
    assert_eq!(draft.edit_count(), 2);
    let revision = draft.revision;
    let bad = writer::ScalarEdit::TaskDuration { task_guid: id(1), expected_storage: 0, value_storage: 1 };
    assert!(draft.apply_ordered(revision, &[bad], &[], &[movement(2, 1, 2)], |_| true).is_err());
    assert_eq!(draft.revision, revision);
    assert_eq!(draft.bytes(|_| true).unwrap(), moved);
    assert!(draft.candidate(|_| true).is_err());
    assert_eq!(draft.ordered_candidate(|_| true).unwrap().receipt.version, 3);
    draft.undo(revision).unwrap();
    assert_eq!(draft.bytes(|_| true).unwrap(), raw);
    draft.redo(draft.revision).unwrap();
    assert_eq!(draft.bytes(|_| true).unwrap(), moved);
    draft.undo(draft.revision).unwrap();
    draft.apply_ordered(draft.revision, &[], &[], &[movement(3, 2, 0)], |_| true).unwrap();
    assert!(!draft.can_redo());
    assert_eq!(draft.text_edits().len(), 0);
}

#[test]
fn inverse_order_clears_order_dirty_state_but_retains_undo_history() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_ordered(0, &[], &[], &[movement(2, 1, 0)], |_| true).unwrap();
    draft.apply_ordered(1, &[], &[], &[movement(2, 0, 1)], |_| true).unwrap();
    assert!(!draft.is_dirty());
    assert!(draft.order_edits().is_empty());
    assert_eq!(draft.bytes(|_| true).unwrap(), raw);
    draft.undo(2).unwrap();
    assert!(draft.is_dirty());
    assert_eq!(draft.order_edits().len(), 1);
}

#[test]
fn eight_move_budget_and_cancellation_do_not_publish_a_partial_transaction() {
    let raw = fixture();
    let mut draft = Draft::open(raw.clone(), &hash(&raw), |_| true).unwrap();
    draft.apply_ordered(0, &[], &[], &[movement(3, 2, 0)], |_| true).unwrap();
    for i in 1..8 {
        let from = if i % 2 == 1 { 1 } else { 2 };
        let to = if from == 1 { 2 } else { 1 };
        draft.apply_ordered(draft.revision, &[], &[], &[movement(1, from, to)], |_| true).unwrap();
    }
    assert_eq!(draft.order_edits().len(), 8);
    let before = draft.bytes(|_| true).unwrap();
    let revision = draft.revision;
    assert!(draft.apply_ordered(revision, &[], &[], &[movement(1, 2, 1)], |_| true).is_err());
    assert_eq!(draft.revision, revision);
    assert_eq!(draft.bytes(|_| true).unwrap(), before);
    let mut fresh = Draft::open(raw, &hash(&fixture()), |_| true).unwrap();
    let mut measured = 0usize;
    let mut probe = fresh.clone();
    probe
        .apply_ordered(0, &[], &[name()], &[movement(2, 1, 0)], |_| {
            measured += 1;
            true
        })
        .unwrap();
    let mut calls = 0usize;
    assert!(
        fresh
            .apply_ordered(0, &[], &[name()], &[movement(2, 1, 0)], |_| {
                calls += 1;
                calls < measured
            })
            .is_err()
    );
    assert_eq!(fresh.revision, 0);
    assert!(!fresh.is_dirty());
    assert!(!fresh.can_undo());
}
