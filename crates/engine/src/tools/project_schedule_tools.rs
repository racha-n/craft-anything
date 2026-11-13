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

#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RelationshipModel {
    #[default]
    FsOnly,
    FourRelationshipsSignedLag,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Request {
    path: String,
    expected_source_sha256: String,
    anchor_wallclock: i64,
    declare_unconstrained_fixed_work: bool,
    /// Explicit opt-in; omitted requests retain the version 1 FS-only model.
    #[serde(default)]
    relationship_model: RelationshipModel,
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
        "Preview a declared task schedule",
        "Calculate bounded earliest dates for up to 512 Planned leaf tasks with positive minute-aligned durations, FS links and nonnegative working-time lag by default. Explicitly choose four_relationships_signed_lag for FS/SS/FF/SF and signed upstream-calendar lag with backward-normalized finish boundaries. Requires an explicit anchor and unconstrained fixed-work declaration. Does not change the source or certify native rescheduling, constraints, critical paths or baselines.",
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
    let model = match p.relationship_model {
        RelationshipModel::FsOnly => craft_anything_analysis::project_schedule::ScheduleModel::FsOnly,
        RelationshipModel::FourRelationshipsSignedLag => craft_anything_analysis::project_schedule::ScheduleModel::FourRelationshipsSignedLag,
    };
    let preview =
        craft_anything_analysis::project_schedule::from_metadata_with_model(&imported.metadata, p.anchor_wallclock, true, model, || ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let preview = preview.map_err(ToolError::invalid)?;
    let items = preview.tasks.iter().skip(p.offset).take(p.limit.clamp(1, 250)).collect::<Vec<_>>();
    let mut result = json!({"version":preview.version,"source_sha256":imported.source_sha256,"anchor_wallclock":preview.anchor_wallclock,
        "task_count":preview.task_count,"edge_count":preview.edge_count,"declared_unconstrained_fixed_work":preview.declared_unconstrained_fixed_work,
        "native_parity_verified":preview.native_parity_verified,"critical_path_calculated":preview.critical_path_calculated,
        "page":{"offset":p.offset,"total":preview.task_count,"items":items},"limitations":preview.limitations});
    if let Some(model) = preview.relationship_model {
        result["relationship_model"] = json!(model);
    }
    let evidence = builder(ctx, "formats_project_schedule_preview", input.subject, params)
        .summary(format!("Previewed earliest dates for {} declared fixed-work tasks", preview.task_count))
        .confidence(Confidence::Derived)
        .coverage(Coverage { status: CoverageStatus::Partial, ..Default::default() })
        .result(result)
        .limitation(Limitation::new(
            "declared_schedule_model",
            "Read-only declared relationship model; no native parity, source-constraint admission, critical paths or baselines.",
        ))
        .target_content(true)
        .build()?;
    Ok(ToolOutput::evidence(evidence))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn relationship_model_is_explicit_and_unknown_models_are_refused() {
        let args = json!({"path":"absent.sp", "expected_source_sha256":"0".repeat(64),
            "anchor_wallclock":1794128400, "declare_unconstrained_fixed_work":true});
        let old: Request = serde_json::from_value(args.clone()).unwrap();
        assert!(matches!(old.relationship_model, RelationshipModel::FsOnly));
        let mut expanded = args;
        expanded["relationship_model"] = json!("four_relationships_signed_lag");
        assert!(matches!(serde_json::from_value::<Request>(expanded.clone()).unwrap().relationship_model, RelationshipModel::FourRelationshipsSignedLag));
        expanded["relationship_model"] = json!("native_scheduler");
        assert!(serde_json::from_value::<Request>(expanded).is_err());
    }
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
