//! Version-3 composition of scalar/text edits and bounded same-parent moves.
use super::{FormatError, fail, format_error, mixed, relocation, task_strings, writer};
use serde::Serialize;

#[cfg(test)]
#[path = "ordered_tests.rs"]
mod tests;

pub const ORDER_LIMIT: usize = 8;

#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub mixed_stage: mixed::Receipt,
    pub order_stages: Vec<relocation::Receipt>,
    pub internal_reparse_passed: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_consistency_verified: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}

/// Numeric/text fields bind to the source; moves bind sequentially to prior output.
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    edits: &[writer::ScalarEdit],
    text_edits: &[task_strings::TextEdit],
    order_edits: &[relocation::Move],
    mut keep_going: impl FnMut(f32) -> bool,
) -> Result<Candidate, FormatError> {
    if order_edits.len() > ORDER_LIMIT || edits.len().checked_add(text_edits.len()).and_then(|n| n.checked_add(order_edits.len())).is_none_or(|n| n > 256) {
        return Err(format_error(fail("limit", "Choose at most 256 edits in total, including at most eight sibling-order operations")));
    }
    let mixed = mixed::candidate(raw, expected_source_sha256, edits, text_edits, &mut keep_going)?;
    let mut output = mixed.bytes;
    let mut hash = mixed.receipt.output_sha256.clone();
    let mut stages = Vec::new();
    for movement in order_edits {
        let next = relocation::candidate(&output, &hash, movement, &mut keep_going)?;
        output = next.bytes;
        hash = next.receipt.output_sha256.clone();
        stages.push(next.receipt);
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 3,
            source_sha256: mixed.receipt.source_sha256.clone(),
            output_sha256: hash,
            mixed_stage: mixed.receipt,
            order_stages: stages,
            internal_reparse_passed: true,
            native_reopen_verified: false,
            derived_schedule_consistency_verified: false,
            limitations: vec![
                "Experimental scalar/text edits followed by up to eight guarded same-parent task-order operations; no reparenting.",
                "Each order stage binds to the preceding output hash and position; numeric/text expectations bind to the original source.",
                "Native compatibility, schedule consistency, resource coordination and general authoring remain separate acceptance gates.",
            ],
        },
    })
}
