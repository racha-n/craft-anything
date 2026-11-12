//! Read-only, bounded measurements over a retained scene snapshot.
use super::{native_common::spec, scene_tools::LoadedScene, static_input::builder};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
use craft_anything_evidence::{Confidence, Coverage, CoverageStatus, Limitation, ToolError};
use craft_anything_formats::scene_quantities::{self, DEFAULT_TRIANGLE_BUDGET};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Params {
    scene_id: String,
    /// Omit to page all loaded instances. Supplied indices are deduplicated and sorted.
    #[serde(default)]
    instance_indices: Option<Vec<usize>>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "default_limit")]
    limit: usize,
    #[serde(default = "default_budget")]
    triangle_budget: usize,
}
fn default_limit() -> usize {
    100
}
fn default_budget() -> usize {
    DEFAULT_TRIANGLE_BUDGET
}
pub fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "formats_scene_quantities",
        "Measure scene surfaces",
        "Measure a bounded instance page in stored world coordinates. Exact welded-edge and vertex-fan tests gate signed algebraic volume; self-intersections and source units remain unverified. No aggregate or certified solid quantity is returned.",
        "Inspect transformed surface area and supported algebraic volume for loaded objects.",
        Toolset::Formats,
        Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
        Requires::Nothing,
        schema_of::<Params>,
        run,
    )]
}
fn run(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Params = parse_params(params.clone())?;
    let loaded = ctx.session.ext.get::<LoadedScene>().ok_or_else(|| ToolError::invalid("Open a project model or scene bundle before measuring surfaces."))?;
    if p.scene_id != loaded.scene.id {
        return Err(ToolError::invalid("The requested measurement belongs to a different scene."));
    }
    let page = scene_quantities::measure(&loaded.scene, p.instance_indices.as_deref(), p.offset, p.limit, p.triangle_budget, || !ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let page = page.map_err(|error| match error {
        scene_quantities::Error::Cancelled => ToolError::cancelled(),
        other => ToolError::invalid(other.to_string()),
    })?;
    let ev=builder(ctx,"formats_scene_quantities",loaded.subject.clone(),params)
        .summary("Transformed surface measurements for the requested instance page")
        .confidence(Confidence::Derived)
        .coverage(Coverage {status:CoverageStatus::Partial,..Default::default()})
        .result(json!({"scene_id":loaded.scene.id,"source_sha256":loaded.scene.source_sha256,
            "source_unit_declaration":loaded.scene.unit_declaration,"source_axis_declaration":loaded.scene.axis_declaration,"page":page}))
        .limitation(Limitation::new("unverified_units","Area and volume use stored world-coordinate units squared and cubed. Metres and cubic metres are not inferred from geometry or the source declaration."))
        .limitation(Limitation::new("algebraic_volume_only","Closed, consistently oriented edges and connected vertex fans are necessary checks. Self-intersections, nested-shell orientation, overlapping components, solid union and engineering suitability are not verified."))
        .limitation(Limitation::new("coordinate_policy","Transforms are applied per instance with translation rebased. Native group/root volume can follow a different coordinate policy; native quantity parity is not established."))
        .limitation(Limitation::new("page_scope","Only requested instances are measured. No project aggregate is returned; triangle and topology-memory budgets can stop the request with an error."))
        .target_content(true).build()?;
    Ok(ToolOutput::evidence(ev))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallOptions, Caller, CancelToken, Engine};
    use craft_anything_evidence::ErrorCode;

    fn loaded_triangle_fixture() -> (tempfile::TempDir, Engine, String) {
        let dir = tempfile::tempdir().unwrap();
        for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        let engine = Engine::new(crate::testing::session());
        engine.call("formats_scene_bundle_open", json!({"path":dir.path().join("scene.json")}), CallOptions::new(Caller::Cli)).unwrap();
        let scene_id = engine.session().ext.get::<LoadedScene>().unwrap().scene.id.clone();
        (dir, engine, scene_id)
    }

    #[test]
    fn quantity_request_requires_the_current_scene_identity() {
        let (_dir, engine, scene_id) = loaded_triangle_fixture();
        let missing = engine.call("formats_scene_quantities", json!({}), CallOptions::new(Caller::Cli)).unwrap_err();
        assert!(missing.message.contains("scene_id"));
        let stale = engine.call("formats_scene_quantities", json!({"scene_id":"a-different-scene"}), CallOptions::new(Caller::Cli)).unwrap_err();
        assert!(stale.message.contains("different scene"));
        assert!(engine.call("formats_scene_quantities", json!({"scene_id":scene_id,"instance_indices":[0]}), CallOptions::new(Caller::Cli)).is_ok());
    }

    #[test]
    fn open_triangle_has_area_but_no_claimed_solid_volume() {
        let (_dir, engine, scene_id) = loaded_triangle_fixture();
        let output = engine
            .call("formats_scene_quantities", json!({"scene_id":scene_id,"instance_indices":[0],"limit":1}), CallOptions::new(Caller::Cli))
            .unwrap()
            .to_json();
        assert_eq!(output["result"]["scene_id"], scene_id);
        let page = &output["result"]["page"];
        assert_eq!(page["total_selected_instances"], 1);
        assert_eq!(page["expanded_triangles_tested"], 1);
        assert_eq!(page["units_verified"], false);
        let item = &page["items"][0];
        assert_eq!(item["surface_area_stored_world_units_squared"].as_f64(), Some(2.));
        assert_eq!(item["closed_edges"], false);
        assert!(item["signed_tetrahedral_volume_stored_world_units_cubed"].is_null());
        assert!(item["absolute_algebraic_volume_stored_world_units_cubed"].is_null());
    }

    #[test]
    fn cancelled_quantity_request_does_not_return_measurements() {
        let (_dir, engine, scene_id) = loaded_triangle_fixture();
        let cancel = CancelToken::default();
        cancel.cancel();
        let error = engine.call("formats_scene_quantities", json!({"scene_id":scene_id}), CallOptions::new(Caller::Cli).with_cancel(cancel)).unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
    }

    #[test]
    fn default_clean_room_policy_denies_target_geometry_measurement() {
        let mut session = crate::testing::session();
        session.settings.deep_mode = false;
        let engine = Engine::new(session);
        let error = engine.call("formats_scene_quantities", json!({"scene_id":"synthetic-scene"}), CallOptions::new(Caller::Cli)).unwrap_err();
        assert_eq!(error.details.as_ref().and_then(|details| details.get("reason")).and_then(Value::as_str), Some("clean_room"));
    }
}
