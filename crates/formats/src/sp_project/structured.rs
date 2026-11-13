//! Ordered composition of field edits and existing-task relocations.
use super::{FormatError, fail, format_error, hierarchy, mixed, ordered::ORDER_LIMIT, relocation, task_strings, writer};
use serde::Serialize;

#[cfg(test)]
#[path = "structured_tests.rs"]
mod tests;

#[derive(Serialize)]
#[serde(tag = "kind", content = "receipt", rename_all = "snake_case")]
pub enum Stage {
    Order(relocation::Receipt),
    Hierarchy(hierarchy::Receipt),
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub mixed_stage: mixed::Receipt,
    pub structural_stages: Vec<Stage>,
    pub internal_reparse_passed: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_consistency_verified: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}
pub fn same_parent(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        _ => false,
    }
}
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    edits: &[writer::ScalarEdit],
    text_edits: &[task_strings::TextEdit],
    relocations: &[hierarchy::Reparent],
    mut keep_going: impl FnMut(f32) -> bool,
) -> Result<Candidate, FormatError> {
    if relocations.len() > ORDER_LIMIT || edits.len().checked_add(text_edits.len()).and_then(|n| n.checked_add(relocations.len())).is_none_or(|n| n > 256) {
        return Err(format_error(fail("limit", "Choose at most 256 edits, including at most eight task relocations")));
    }
    let mixed = mixed::candidate(raw, expected_source_sha256, edits, text_edits, &mut keep_going)?;
    let mut output = mixed.bytes;
    let mut hash = mixed.receipt.output_sha256.clone();
    let mut stages = Vec::new();
    for request in relocations {
        if same_parent(request.expected_parent_guid.as_deref(), request.parent_guid.as_deref()) {
            let movement = relocation::Move {
                task_guid: request.task_guid.clone(),
                expected_parent_guid: request.expected_parent_guid.clone(),
                expected_sibling_index: request.expected_sibling_index,
                sibling_index: request.sibling_index,
            };
            let next = relocation::candidate(&output, &hash, &movement, &mut keep_going)?;
            hash = next.receipt.output_sha256.clone();
            output = next.bytes;
            stages.push(Stage::Order(next.receipt));
        } else {
            let next = hierarchy::candidate(&output, &hash, request, &mut keep_going)?;
            hash = next.receipt.output_sha256.clone();
            output = next.bytes;
            stages.push(Stage::Hierarchy(next.receipt));
        }
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 4,
            source_sha256: mixed.receipt.source_sha256.clone(),
            output_sha256: hash,
            mixed_stage: mixed.receipt,
            structural_stages: stages,
            internal_reparse_passed: true,
            native_reopen_verified: false,
            derived_schedule_consistency_verified: false,
            limitations: vec![
                "Experimental field edits followed by up to eight sequential existing-task order/reparent operations.",
                "Fields bind to the original source; each structural stage binds to its preceding output hash, parent and position.",
                "Native saving, scheduling and resource coordination require separate acceptance; no task creation or deletion.",
            ],
        },
    })
}
