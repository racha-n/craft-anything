//! Chronological stages after the first task insertion. History stores recipes only.
use super::*;
use crate::sp_project::{creation, fail, format_error};
use serde::Serialize;

#[derive(Clone)]
pub(super) enum Operation {
    Fields { scalars: Vec<ScalarEdit>, texts: Vec<TextEdit> },
    Relocate(hierarchy::Reparent),
    Create(creation::InsertAbove),
}
#[derive(Serialize)]
#[serde(tag = "kind", content = "receipt", rename_all = "snake_case")]
pub enum Stage {
    Fields(mixed::Receipt),
    Order(relocation::Receipt),
    Hierarchy(hierarchy::Receipt),
    Create(creation::Receipt),
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub prefix: structured::Receipt,
    pub stages: Vec<Stage>,
    pub internal_reparse_passed: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_consistency_verified: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}
pub(super) fn counts(operations: &[Operation]) -> (usize, usize) {
    operations.iter().fold((0usize, 0usize), |(fields, structural), op| match op {
        Operation::Fields { scalars, texts } => (fields.saturating_add(scalars.len()).saturating_add(texts.len()), structural),
        Operation::Relocate(_) | Operation::Create(_) => (fields, structural.saturating_add(1)),
    })
}
pub(super) fn payload(operations: &[Operation]) -> Result<usize, String> {
    operations.iter().try_fold(0usize, |total, op| {
        let bytes = match op {
            Operation::Fields { scalars, texts } => payload_cost(scalars, texts)?,
            Operation::Relocate(r) => [Some(r.task_guid.as_str()), r.expected_parent_guid.as_deref(), r.parent_guid.as_deref()]
                .into_iter()
                .flatten()
                .try_fold(1024usize, |n, s| n.checked_add(s.len()))
                .ok_or_else(|| "Draft relocation payload overflow".to_owned())?,
            Operation::Create(r) => [
                Some(r.template_task_guid.as_str()),
                Some(r.anchor_task_guid.as_str()),
                r.expected_parent_guid.as_deref(),
                Some(r.task_guid.as_str()),
                Some(r.supply_chain_id.as_str()),
                Some(r.name.as_str()),
                Some(r.display_id.as_str()),
            ]
            .into_iter()
            .flatten()
            .try_fold(1024usize, |n, s| n.checked_add(s.len()))
            .ok_or_else(|| "Draft creation payload overflow".to_owned())?,
        };
        total.checked_add(bytes).ok_or_else(|| "Draft replay payload overflow".to_owned())
    })
}
pub(super) fn candidate(
    raw: &[u8],
    hash: &str,
    scalars: &[ScalarEdit],
    texts: &[TextEdit],
    relocations: &[hierarchy::Reparent],
    operations: &[Operation],
    mut keep: impl FnMut(f32) -> bool,
) -> Result<Candidate, FormatError> {
    let (fields, structural) = counts(operations);
    let total = fields.saturating_add(structural).saturating_add(scalars.len()).saturating_add(texts.len()).saturating_add(relocations.len());
    if structural.saturating_add(relocations.len()) > ordered::ORDER_LIMIT || total > 256 || !operations.iter().any(|o| matches!(o, Operation::Create(_))) {
        return Err(format_error(fail("limit", "Creation replay needs an insertion and supports at most 256 edits including eight structural operations")));
    }
    let prefix = structured::candidate(raw, hash, scalars, texts, relocations, &mut keep)?;
    let mut output = prefix.bytes;
    let mut hash = prefix.receipt.output_sha256.clone();
    let mut stages = Vec::new();
    for operation in operations {
        match operation {
            Operation::Fields { scalars, texts } => {
                let next = mixed::candidate(&output, &hash, scalars, texts, &mut keep)?;
                hash = next.receipt.output_sha256.clone();
                output = next.bytes;
                stages.push(Stage::Fields(next.receipt));
            }
            Operation::Relocate(r) if structured::same_parent(r.expected_parent_guid.as_deref(), r.parent_guid.as_deref()) => {
                let movement = relocation::Move {
                    task_guid: r.task_guid.clone(),
                    expected_parent_guid: r.expected_parent_guid.clone(),
                    expected_sibling_index: r.expected_sibling_index,
                    sibling_index: r.sibling_index,
                };
                let next = relocation::candidate(&output, &hash, &movement, &mut keep)?;
                hash = next.receipt.output_sha256.clone();
                output = next.bytes;
                stages.push(Stage::Order(next.receipt));
            }
            Operation::Relocate(r) => {
                let next = hierarchy::candidate(&output, &hash, r, &mut keep)?;
                hash = next.receipt.output_sha256.clone();
                output = next.bytes;
                stages.push(Stage::Hierarchy(next.receipt));
            }
            Operation::Create(r) => {
                let next = creation::candidate(&output, &hash, r, &mut keep)?;
                hash = next.receipt.output_sha256.clone();
                output = next.bytes;
                stages.push(Stage::Create(next.receipt));
            }
        }
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 5,
            source_sha256: prefix.receipt.source_sha256.clone(),
            output_sha256: hash,
            prefix: prefix.receipt,
            stages,
            internal_reparse_passed: true,
            native_reopen_verified: false,
            derived_schedule_consistency_verified: false,
            limitations: vec![
                "Experimental chronological default-task insertion, guarded fields and existing-task relocation stages.",
                "Every stage binds to its preceding output; each stage retains its original admission and preservation limits.",
                "Native persistence, constructor completeness, geometry and scheduling require independent acceptance; deletion is unsupported.",
            ],
        },
    })
}
