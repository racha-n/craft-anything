//! Read-only declared-model scheduling, separate from native authoring admission.
use super::{
    native_common::spec,
    static_input::{builder, format_err, load_path},
};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
use craft_anything_evidence::{Confidence, Coverage, CoverageStatus, Limitation, ToolError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Request {
    path: String,
    expected_source_sha256: String,
    anchor_wallclock: i64,
    declare_unconstrained_fixed_work: bool,
    #[serde(default)]
    offset: usize,
    #[serde(default = "page_size")]
    limit: usize,
}
fn page_size() -> usize {
    100
}
pub fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "formats_project_schedule_preview",
        "Preview a declared FS schedule",
        "Calculate bounded earliest dates for up to 512 Planned leaf tasks with positive minute-aligned durations, FS links and nonnegative working-time lag. Requires an explicit anchor and unconstrained fixed-work declaration. Does not change the source or certify native rescheduling, constraints, critical paths or baselines.",
        "Try a limited calendar-based plan using an explicit date anchor. The source file stays unchanged.",
        Toolset::Formats,
        Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
        Requires::Nothing,
        schema_of::<Request>,
        run,
    )]
}
fn run(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Request = parse_params(params.clone())?;
    if !p.declare_unconstrained_fixed_work {
        return Err(ToolError::invalid("Declare unconstrained fixed-work tasks; source task kinds and constraints are not certified by this preview."));
    }
    let input = load_path(&p.path, 256 * 1024 * 1024)?;
    let imported = craft_anything_formats::sp_project::parse_metadata(&input.bytes, |_| !ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let imported = imported.map_err(|e| format_err(e, "admitted project metadata"))?;
    if !imported.source_sha256.eq_ignore_ascii_case(&p.expected_source_sha256) {
        return Err(ToolError::invalid("Source hash changed; reload before previewing dates."));
    }
    let preview = craft_anything_analysis::project_schedule::from_metadata(&imported.metadata, p.anchor_wallclock, true, || ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let preview = preview.map_err(ToolError::invalid)?;
    let items = preview.tasks.iter().skip(p.offset).take(p.limit.clamp(1, 250)).collect::<Vec<_>>();
    let result = json!({"version":preview.version,"source_sha256":imported.source_sha256,"anchor_wallclock":preview.anchor_wallclock,
        "task_count":preview.task_count,"edge_count":preview.edge_count,"declared_unconstrained_fixed_work":preview.declared_unconstrained_fixed_work,
        "native_parity_verified":preview.native_parity_verified,"critical_path_calculated":preview.critical_path_calculated,
        "page":{"offset":p.offset,"total":preview.task_count,"items":items},"limitations":preview.limitations});
    let evidence = builder(ctx, "formats_project_schedule_preview", input.subject, params)
        .summary(format!("Previewed earliest dates for {} declared fixed-work tasks", preview.task_count))
        .confidence(Confidence::Derived)
        .coverage(Coverage { status: CoverageStatus::Partial, ..Default::default() })
        .result(result)
        .limitation(Limitation::new(
            "declared_schedule_model",
            "Read-only FS model; no native parity, source-constraint admission, critical paths or baselines.",
        ))
        .target_content(true)
        .build()?;
    Ok(ToolOutput::evidence(evidence))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fs_preview_requires_declaration_without_reading_a_target() {
        let engine = crate::Engine::new(crate::testing::session());
        let error = engine
            .call(
                "formats_project_schedule_preview",
                json!({"path":"absent.sp","expected_source_sha256":"0".repeat(64),
            "anchor_wallclock":1794128400,"declare_unconstrained_fixed_work":false}),
                crate::CallOptions::new(crate::Caller::Cli),
            )
            .unwrap_err();
        assert!(error.message.contains("Declare"));
    }
}
