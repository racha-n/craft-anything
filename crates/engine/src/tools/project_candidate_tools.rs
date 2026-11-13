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
use craft_anything_formats::sp_project::task_strings::{self, TextEdit};
use craft_anything_formats::sp_project::writer::{self, ScalarEdit};
use craft_anything_formats::sp_project::{creation, deletion, hierarchy, mixed, ordered, relocation, structured};
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
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct TextRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    edits: Vec<TextEdit>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct MixedRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    #[serde(default)]
    edits: Vec<ScalarEdit>,
    #[serde(default)]
    text_edits: Vec<TextEdit>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct RelocationRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    relocation: relocation::Move,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct HierarchyRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    relocation: hierarchy::Reparent,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct CreationRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    insertion: creation::InsertAbove,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct DeletionRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    deletion: deletion::DeleteLeaf,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct StructuredRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    #[serde(default)]
    edits: Vec<ScalarEdit>,
    #[serde(default)]
    text_edits: Vec<TextEdit>,
    #[serde(default)]
    relocations: Vec<hierarchy::Reparent>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct OrderedRequest {
    path: String,
    expected_source_sha256: String,
    output_path: String,
    acknowledge_storage_only: bool,
    #[serde(default)]
    edits: Vec<ScalarEdit>,
    #[serde(default)]
    text_edits: Vec<TextEdit>,
    #[serde(default)]
    order_edits: Vec<relocation::Move>,
}
pub fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "formats_project_sp_candidate_write",
            "Write an experimental SP candidate",
            "Patch up to 256 guarded scalar storage fields in an admitted project copy, or publish a byte-identical no-edit control. Preserve all other section payloads and publish new SP/receipt files without replacement. Does not certify native reopen, dependent-date consistency or rescheduling.",
            "Create a test copy for independent native validation. Schedule and cost authoring are not certified.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<Request>,
            run,
        ),
        spec(
            "formats_project_sp_task_text_candidate_write",
            "Write an experimental SP task text candidate",
            "Change up to 256 source-hash, task-GUID and expected-text guarded task names or display IDs. Preserve other section payloads and publish new candidate and receipt files without replacement. Native compatibility remains a separate validation gate.",
            "Create a separate test copy with changed task labels. Validate the copy in the source application before relying on it.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<TextRequest>,
            run_text,
        ),
        spec(
            "formats_project_sp_mixed_candidate_write",
            "Write an experimental mixed SP candidate",
            "Compose up to 256 guarded scalar and task text fields in a new project copy. Publish a version-2 receipt retaining the original scalar and text stage receipts. Native compatibility and schedule consistency remain separate validation gates.",
            "Export a separate test copy containing both numeric edits and changed task labels. Validate it independently before relying on it.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<MixedRequest>,
            run_mixed,
        ),
        spec(
            "formats_project_sp_task_order_candidate_write",
            "Write an experimental task order candidate",
            "Reorder one existing Planned unassigned task subtree within its sibling list in a source-bound project of at most 512 tasks. Guard parent, position and collection lookup indices; preserve task content and other sections, rebuild sibling lookup indices, and publish separate candidate/version-2 receipt files without replacement. Does not change parents, recalculate schedules or certify native saving.",
            "Create a separate test copy with changed task order. Validate it in the source application before relying on it.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<RelocationRequest>,
            run_relocation,
        ),
        spec(
            "formats_project_sp_ordered_candidate_write",
            "Write an experimental ordered SP candidate",
            "Compose guarded numeric/text edits and up to eight sequential same-parent task-order operations in a separate project copy. Publish a version-3 receipt retaining prior stage receipts and their hash chain. Native saving and schedule coordination remain unverified.",
            "Export a separate test copy containing changed fields and task order, then validate it in the source application.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<OrderedRequest>,
            run_ordered,
        ),
        spec(
            "formats_project_sp_task_hierarchy_candidate_write",
            "Write an experimental task hierarchy candidate",
            "Move an existing Planned unassigned task subtree to a different parent in a source-bound project of at most 512 tasks. Guard source parent and position, refuse cycles/depth overflow, preserve task body fields and other sections, and rebuild child collections and GUID lookup indices. Publish a separate candidate and version-1 receipt without replacement; native saving and summary/schedule recalculation remain unverified.",
            "Create a separate test copy with a changed task parent. Validate its hierarchy and dates in the source application before relying on it.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<HierarchyRequest>,
            run_hierarchy,
        ),
        spec(
            "formats_project_sp_structured_candidate_write",
            "Write an experimental structural SP candidate",
            "Compose source-bound scalar/text edits and up to eight sequential existing-task order/reparent operations. Publish a version-4 receipt retaining typed mixed/order/hierarchy stages and their hash chain. Parent/position guards bind to each preceding stage; native saving and scheduling remain separate gates.",
            "Export a separate test copy containing field and hierarchy changes, then validate it in the source application.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<StructuredRequest>,
            run_structured,
        ),
        spec(
            "formats_project_sp_task_create_candidate_write",
            "Write an experimental default Work task candidate",
            "Insert one unassigned default Planned Work leaf above an existing Work leaf, using a source hash, guarded parent/position, validated default body/state/calendar and distinct task/Supply Chain identities. Support unassigned hierarchies or flat Planned roots with at most one retained assignment per task; omit template assignments and disclose their count in a version-2 receipt. Require explicit name/display ID and preserve other sections. Native transaction history and generic native saving remain unverified.",
            "Create a separate test copy with one new one-day task, then validate all affected rows in the source application.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<CreationRequest>,
            run_creation,
        ),
        spec(
            "formats_project_sp_task_delete_candidate_write",
            "Write an experimental task deletion candidate",
            "Remove one unassigned Planned root leaf from a flat default-Work project with at least two tasks. Bind source hash, task identity, display ID and position; rebuild collection lookups and validated presentation row/selection state. Reject associated tasks, links, multi-selection and unknown references. Publish separate no-clobber SP/receipt files; native saving and scheduling remain independent gates.",
            "Export a separate test copy with one supported task removed, then validate the remaining rows in the source application.",
            Toolset::Formats,
            Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<DeletionRequest>,
            run_deletion,
        ),
    ]
}
pub(super) fn output_paths(output: &str) -> Result<(PathBuf, PathBuf), ToolError> {
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
pub(super) fn publish(parent: &Path, output: &Path, receipt_path: &Path, bytes: &[u8], receipt: &[u8]) -> Result<(), ToolError> {
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

fn run_text(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: TextRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = task_strings::candidate(&input.bytes, &p.expected_source_sha256, &p.edits, |progress| {
        ctx.report(Some(progress), "Preparing experimental SP task text candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted task-text candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_task_text_candidate_write", input.subject, params)
        .summary("Prepared an experimental SP task text candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new("native_persistence_unverified", "Internal reparse passed; native open/save/reopen remains unverified."))
        .target_content(true)
        .build()?;
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let parent = output.parent().ok_or_else(|| ToolError::invalid("Choose an output parent directory."))?;
    publish(parent, &output, &receipt_path, &candidate.bytes, &receipt)?;
    Ok(ToolOutput::evidence(evidence))
}

fn run_relocation(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: RelocationRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = relocation::candidate(&input.bytes, &p.expected_source_sha256, &p.relocation, |progress| {
        ctx.report(Some(progress), "Preparing experimental task relocation candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted task relocation candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_task_order_candidate_write", input.subject, params)
        .summary("Prepared an experimental task relocation candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new(
            "native_persistence_unverified",
            "Internal reparse, task content preservation and sibling lookup validation passed; native acceptance and schedule coordination remain unverified.",
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

fn run_hierarchy(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: HierarchyRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = hierarchy::candidate(&input.bytes, &p.expected_source_sha256, &p.relocation, |progress| {
        ctx.report(Some(progress), "Preparing experimental task hierarchy candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted task hierarchy candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_task_hierarchy_candidate_write", input.subject, params)
        .summary("Prepared an experimental task hierarchy candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new(
            "native_persistence_unverified",
            "Task identity/body fields and lookup indices passed internal validation. Native persistence, summary dates and schedule coordination remain unverified.",
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

fn run_creation(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: CreationRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = creation::candidate(&input.bytes, &p.expected_source_sha256, &p.insertion, |progress| {
        ctx.report(Some(progress), "Preparing experimental default Work task candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted default Work task creation candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_task_create_candidate_write", input.subject, params)
        .summary("Prepared an experimental default Work task candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new("native_persistence_unverified", "Default-body/state/calendar, retained task bodies and lookup entries passed internal checks. Native persistence, geometry and scheduling need independent acceptance."))
        .limitation(Limitation::new("native_transaction_not_reproduced", "Only Tasks changes; transaction, thumbnail and presentation history are preserved. Automatic display IDs and task deletion are not supported."))
        .target_content(true)
        .build()?;
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let parent = output.parent().ok_or_else(|| ToolError::invalid("Choose an output parent directory."))?;
    publish(parent, &output, &receipt_path, &candidate.bytes, &receipt)?;
    Ok(ToolOutput::evidence(evidence))
}

fn run_deletion(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: DeletionRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = deletion::candidate(&input.bytes, &p.expected_source_sha256, &p.deletion, |progress| {
        ctx.report(Some(progress), "Preparing experimental task deletion candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted unassigned root-leaf deletion candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_task_delete_candidate_write", input.subject, params)
        .summary("Prepared an experimental unassigned root-leaf deletion candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new("native_persistence_unverified", "Task bodies/lookups, presentation rows/selection and identity references pass internal checks. Native persistence, row identity and scheduling require separate acceptance."))
        .limitation(Limitation::new("native_transaction_not_reproduced", "Only Tasks and validated PresentationData change; the other 46 stored sections remain exact. Native transaction/thumbnail history is not reproduced."))
        .limitation(Limitation::new("separate_file_publication", "SP and receipt publication each refuse replacement. A receipt publication failure after SP publication retains that candidate and reports recovery instructions."))
        .target_content(true)
        .build()?;
    if load_path(&p.path, 256 * 1024 * 1024)?.bytes != input.bytes {
        return Err(ToolError::invalid("Source changed during candidate generation; reload it before exporting."));
    }
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let parent = output.parent().ok_or_else(|| ToolError::invalid("Choose an output parent directory."))?;
    publish(parent, &output, &receipt_path, &candidate.bytes, &receipt)?;
    Ok(ToolOutput::evidence(evidence))
}

pub(super) fn run_structured(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: StructuredRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = structured::candidate(&input.bytes, &p.expected_source_sha256, &p.edits, &p.text_edits, &p.relocations, |progress| {
        ctx.report(Some(progress), "Preparing experimental structural candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted structural candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_structured_candidate_write", input.subject, params)
        .summary("Prepared an experimental structural candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new(
            "native_persistence_unverified",
            "Stage hash-chain and internal guards passed; native saving and schedule coordination remain separate acceptance gates.",
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

pub(super) fn run_ordered(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: OrderedRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = ordered::candidate(&input.bytes, &p.expected_source_sha256, &p.edits, &p.text_edits, &p.order_edits, |progress| {
        ctx.report(Some(progress), "Preparing experimental ordered SP candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted ordered candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_ordered_candidate_write", input.subject, params)
        .summary("Prepared an experimental ordered SP candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new(
            "native_persistence_unverified",
            "Internal field and task-order reparses passed; native saving and schedule coordination remain unverified.",
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

pub(super) fn run_mixed(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: MixedRequest = parse_params(params.clone())?;
    if !p.acknowledge_storage_only {
        return Err(ToolError::invalid("Acknowledge storage-only candidate generation; native compatibility is unverified."));
    }
    let (output, receipt_path) = output_paths(&p.output_path)?;
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let candidate = mixed::candidate(&input.bytes, &p.expected_source_sha256, &p.edits, &p.text_edits, |progress| {
        ctx.report(Some(progress), "Preparing experimental mixed SP candidate");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(|e| format_err(e, "an admitted mixed candidate"))?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = builder(ctx, "formats_project_sp_mixed_candidate_write", input.subject, params)
        .summary("Prepared an experimental mixed SP storage candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new(
            "native_persistence_unverified",
            "Internal reparse passed; native compatibility and schedule consistency remain unverified.",
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
    fn deletion_export_preserves_source_and_refuses_stale_guards_and_existing_outputs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let output = dir.path().join("deleted.sp");
        let raw = craft_anything_testkit::project_authoring::default_work_project_with_presentation(4, Some(1));
        std::fs::write(&path, &raw).unwrap();
        let metadata = craft_anything_formats::sp_project::parse_metadata(&raw, |_| true).unwrap();
        let target = &metadata.metadata["tasks"][1];
        let e = Engine::new(crate::testing::session());
        let call = |p| e.call("formats_project_sp_task_delete_candidate_write", p, CallOptions::new(Caller::Cli));
        let mut request = json!({"path":path,"expected_source_sha256":metadata.source_sha256,"output_path":output,"acknowledge_storage_only":false,
            "deletion":{"task_guid":target["guid"],"expected_parent_guid":null,"expected_sibling_index":1,"expected_display_id":target["display_id"]}});
        assert!(call(request.clone()).unwrap_err().message.contains("Acknowledge"));
        assert!(!output.exists());
        request["acknowledge_storage_only"] = json!(true);
        let mut stale = request.clone();
        stale["deletion"]["expected_sibling_index"] = json!(0);
        assert!(call(stale).is_err());
        assert!(!output.exists());
        let mut stale = request.clone();
        stale["expected_source_sha256"] = json!("0".repeat(64));
        assert!(call(stale).is_err());
        assert!(!output.exists());
        let value = call(request.clone()).unwrap().to_json();
        assert_eq!(value["result"]["receipt"]["task_count_after"], 3);
        assert_eq!(value["result"]["receipt"]["task_selection_cleared"], true);
        let published = std::fs::read(&output).unwrap();
        assert!(call(request).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), published);
        assert_eq!(std::fs::read(path).unwrap(), raw);
        let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
        assert_eq!(receipt, value["result"]["receipt"]);
    }
    #[test]
    fn task_relocation_requires_acknowledgement_and_valid_source() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("relocated.sp");
        let engine = Engine::new(crate::testing::session());
        let mut request = json!({"path":dir.path().join("absent.sp"),"expected_source_sha256":"0".repeat(64),
            "output_path":output,"acknowledge_storage_only":false,
            "relocation":{"task_guid":"11111111-1111-1111-1111-111111111111","expected_parent_guid":null,
                "expected_sibling_index":0,"sibling_index":0}});
        let call = |params| engine.call("formats_project_sp_task_order_candidate_write", params, CallOptions::new(Caller::Cli));
        assert!(call(request.clone()).unwrap_err().message.contains("Acknowledge"));
        request["acknowledge_storage_only"] = json!(true);
        assert!(call(request).is_err());
        assert!(!output.exists());
        assert!(!output.with_extension("sp.receipt.json").exists());
    }
    #[test]
    fn hierarchy_requires_acknowledgement_and_valid_source_without_publishing_files() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("hierarchy.sp");
        let engine = Engine::new(crate::testing::session());
        let mut request = json!({"path":dir.path().join("absent.sp"),"expected_source_sha256":"0".repeat(64),
            "output_path":output,"acknowledge_storage_only":false,
            "relocation":{"task_guid":"11111111-1111-1111-1111-111111111111","expected_parent_guid":null,
                "expected_sibling_index":0,"parent_guid":"22222222-2222-2222-2222-222222222222","sibling_index":0}});
        let call = |params| engine.call("formats_project_sp_task_hierarchy_candidate_write", params, CallOptions::new(Caller::Cli));
        assert!(call(request.clone()).unwrap_err().message.contains("Acknowledge"));
        request["acknowledge_storage_only"] = json!(true);
        assert!(call(request).is_err());
        assert!(!output.exists());
        assert!(!output.with_extension("sp.receipt.json").exists());
    }
    #[test]
    fn task_creation_requires_acknowledgement_and_valid_source_without_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("created.sp");
        let engine = Engine::new(crate::testing::session());
        let mut request = json!({"path":dir.path().join("absent.sp"),"expected_source_sha256":"0".repeat(64),
            "output_path":output,"acknowledge_storage_only":false,
            "insertion":{"template_task_guid":"11111111-1111-1111-1111-111111111111", "anchor_task_guid":"11111111-1111-1111-1111-111111111111",
                "expected_parent_guid":null,"expected_sibling_index":0,"task_guid":"22222222-2222-2222-2222-222222222222",
                "supply_chain_id":"33333333-3333-3333-3333-333333333333","name":"New task","display_id":"NEW-1"}});
        let call = |params| engine.call("formats_project_sp_task_create_candidate_write", params, CallOptions::new(Caller::Cli));
        assert!(call(request.clone()).unwrap_err().message.contains("Acknowledge"));
        request["acknowledge_storage_only"] = json!(true);
        assert!(call(request.clone()).is_err());
        request["insertion"]["unknown"] = json!(true);
        assert!(call(request).is_err());
        assert!(!output.exists());
        assert!(!output.with_extension("sp.receipt.json").exists());
    }
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
    fn task_text_and_mixed_tools_require_acknowledgement_and_valid_source() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("text.sp");
        let engine = Engine::new(crate::testing::session());
        let mut request = json!({"path":dir.path().join("absent.sp"), "expected_source_sha256":"0".repeat(64),
            "output_path":output,"acknowledge_storage_only":false,"edits":[]});
        for tool in ["formats_project_sp_task_text_candidate_write", "formats_project_sp_mixed_candidate_write"] {
            let call = |params| engine.call(tool, params, CallOptions::new(Caller::Cli));
            request["acknowledge_storage_only"] = json!(false);
            assert!(call(request.clone()).unwrap_err().message.contains("Acknowledge"));
            request["acknowledge_storage_only"] = json!(true);
            assert!(call(request.clone()).is_err());
        }
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
