//! Experimental scalar storage edits; native compatibility remains a separate acceptance gate.
use super::{
    native_common::spec,
    static_input::{builder, format_err, load_path},
};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
use craft_anything_evidence::{Confidence, Limitation, ToolError};
use craft_anything_formats::sp_project::writer::{self, ScalarEdit};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Request {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    edits: Vec<ScalarEdit>,
}
pub fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "formats_project_sp_candidate_write",
        "Write an experimental SP candidate",
        "Patch up to 256 guarded scalar storage fields in an admitted project copy, or publish a byte-identical no-edit control. Preserve all other section payloads and publish new SP/receipt files without replacement. Does not certify native reopen, dependent-date consistency or rescheduling.",
        "Create a test copy for independent native validation. Schedule and cost authoring are not certified.",
        Toolset::Formats,
        Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
        Requires::Nothing,
        schema_of::<Request>,
        run,
    )]
}
fn output_paths(output: &str) -> Result<(PathBuf, PathBuf), ToolError> {
    let path = PathBuf::from(output);
    if !path.is_absolute() || !path.extension().is_some_and(|s| s.eq_ignore_ascii_case("sp")) {
        return Err(ToolError::invalid("Choose a new absolute output path ending in .sp."));
    }
    let mut receipt = path.as_os_str().to_os_string();
    receipt.push(".receipt.json");
    let receipt = PathBuf::from(receipt);
    if path.exists() || receipt.exists() {
        return Err(ToolError::invalid("The SP candidate or its receipt already exists; choose a new output name."));
    }
    Ok((path, receipt))
}
fn publish(parent: &Path, output: &Path, receipt_path: &Path, bytes: &[u8], receipt: &[u8]) -> Result<(), ToolError> {
    let temporary = tempfile::tempdir_in(parent).map_err(|e| ToolError::io(parent.display(), e))?;
    let staged = temporary.path().join("candidate.sp");
    let staged_receipt = temporary.path().join("receipt.json");
    craft_anything_platform::write_atomic_private(&staged, bytes)?;
    craft_anything_platform::write_atomic_private(&staged_receipt, receipt)?;
    // Hard links fail when a destination already exists, including concurrent publication.
    std::fs::hard_link(&staged, output).map_err(|e| ToolError::io(output.display(), e))?;
    std::fs::hard_link(&staged_receipt, receipt_path).map_err(|e| {
        ToolError::io(receipt_path.display(), e).with_remediation(
            "The SP candidate was created, but its receipt could not be published. Preserve the candidate for investigation and choose a new output name.",
        )
    })?;
    Ok(())
}
pub(super) fn run(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Request = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native reopen and schedule consistency are unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = writer::candidate(&input.bytes, &p.expected_source_sha256, &p.edits, |progress| {
        ctx.report(Some(progress), "Preparing experimental SP candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted scalar-write candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    // Build evidence before publication, so serialization/evidence errors cannot leave unexplained output.
    let evidence = builder(ctx, "formats_project_sp_candidate_write", input.subject, params)
        .summary("Prepared an experimental SP storage candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new(
            "native_persistence_unverified",
            "Internal reparse passed. Native open/save/reopen and dependent schedule consistency remain unverified.",
        ))
        .target_content(true)
        .build()?;
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let parent = output.parent().ok_or_else(|| ToolError::invalid("Choose an output parent directory."))?;
    publish(parent, &output, &receipt_path, &candidate.bytes, &receipt)?;
    Ok(ToolOutput::evidence(evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallOptions, Caller, Engine};
    #[test]
    fn candidate_tool_requires_storage_acknowledgement_and_valid_source() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("result.sp");
        let engine = Engine::new(crate::testing::session());
        let mut request = json!({"path":dir.path().join("absent.sp"), "expected_source_sha256":"0".repeat(64),
            "output_path":output,"acknowledge_storage_only":false,"edits":[]});
        let error = engine.call("formats_project_sp_candidate_write", request.clone(), CallOptions::new(Caller::Cli)).unwrap_err();
        assert!(error.message.contains("Acknowledge"));
        request["acknowledge_storage_only"] = json!(true);
        assert!(engine.call("formats_project_sp_candidate_write", request, CallOptions::new(Caller::Cli)).is_err());
        assert!(!output.exists());
        assert!(!output.with_extension("sp.receipt.json").exists());
    }
    #[test]
    fn candidate_publication_never_replaces_existing_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("candidate.sp");
        let receipt = dir.path().join("candidate.sp.receipt.json");
        publish(dir.path(), &output, &receipt, b"new", b"{}").unwrap();
        assert!(publish(dir.path(), &output, &receipt, b"replacement", b"replacement").is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"new");
        assert_eq!(std::fs::read(&receipt).unwrap(), b"{}");
        assert!(output_paths(output.to_str().unwrap()).is_err());
        assert!(output_paths("relative.sp").is_err());
        let partial = dir.path().join("partial.sp");
        assert!(publish(dir.path(), &partial, &receipt, b"candidate", b"replacement").is_err());
        assert_eq!(std::fs::read(partial).unwrap(), b"candidate");
        assert_eq!(std::fs::read(receipt).unwrap(), b"{}");
    }
}
