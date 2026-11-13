//! Compose independently guarded scalar and text storage edits without changing v1 receipts.
use super::{FormatError, fail, format_error, task_strings, writer};
use serde::Serialize;

#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub scalar_stage: writer::Receipt,
    pub text_stage: task_strings::TextReceipt,
    pub internal_reparse_passed: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_consistency_verified: bool,
    pub limitations: Vec<&'static str>,
}

pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}

/// Scalar edits are applied first; text offsets bind to the resulting intermediate file.
/// Both stages retain all other section payloads and independently validate expected values.
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    edits: &[writer::ScalarEdit],
    text_edits: &[task_strings::TextEdit],
    mut keep_going: impl FnMut(f32) -> bool,
) -> Result<Candidate, FormatError> {
    if edits.len().checked_add(text_edits.len()).is_none_or(|n| n > 256) {
        return Err(format_error(fail("limit", "Choose at most 256 scalar and text fields in total")));
    }
    let scalar = writer::candidate(raw, expected_source_sha256, edits, &mut keep_going)?;
    let text = task_strings::candidate(&scalar.bytes, &scalar.receipt.output_sha256, text_edits, &mut keep_going)?;
    Ok(Candidate {
        bytes: text.bytes,
        receipt: Receipt {
            version: 2,
            source_sha256: scalar.receipt.source_sha256.clone(),
            output_sha256: text.receipt.output_sha256.clone(),
            scalar_stage: scalar.receipt,
            text_stage: text.receipt,
            internal_reparse_passed: true,
            native_reopen_verified: false,
            derived_schedule_consistency_verified: false,
            limitations: vec![
                "Experimental storage candidate only; native compatibility and derived schedule consistency require independent validation.",
                "Scalar-stage offsets refer to the original source; text-stage offsets refer to the scalar-stage output.",
                "No automatic scheduling, utilization coordination, task creation or external-reference rewriting is performed.",
            ],
        },
    })
}
