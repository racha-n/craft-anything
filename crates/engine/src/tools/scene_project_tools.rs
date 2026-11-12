//! Retained project schedules and costs share the scene source identity.
use super::{
    native_common::spec,
    scene_tools::LoadedScene,
    static_input::{builder, format_err, load_path},
};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
pub use craft_anything_analysis::project_model::{AppearancePolicy, DateMode, Frame};
pub use craft_anything_analysis::project_model::{ProjectModel, TaskView};
pub use craft_anything_analysis::project_scene::SceneAppearance;
pub use craft_anything_analysis::project_simulation::{Phase, PhaseState};
use craft_anything_evidence::{Confidence, Coverage, CoverageStatus, Limitation, Subject, ToolError};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

pub struct CachedProject {
    pub scene_id: String,
    pub model: Arc<ProjectModel>,
}
pub struct LatestProjectFrame {
    pub scene_id: String,
    pub frame: Arc<Frame>,
    pub scene_appearance: Arc<SceneAppearance>,
}
// Explicit-path metadata calls have their own identity and caches. They must not
// replace the scene-keyed snapshots consumed by an already open viewport.
struct CachedMetadataProject {
    source_sha256: String,
    model: Arc<ProjectModel>,
}
/// Full explicit-path frame, independent of a viewport's scene-keyed frame.
pub struct LatestMetadataFrame {
    pub source_sha256: String,
    pub frame: Arc<Frame>,
    pub model: Arc<ProjectModel>,
}
struct ProjectSource {
    scene_id: Option<String>,
    source_sha256: String,
    subject: Subject,
}
impl ProjectSource {
    fn identity(&self) -> Value {
        json!({"scene_id":self.scene_id,"source_sha256":self.source_sha256,
            "geometry_coverage":if self.scene_id.is_some() { "loaded_scene" } else { "unvalidated" }})
    }
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Page {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    offset: usize,
    #[serde(default = "page_size")]
    limit: usize,
}
fn page_size() -> usize {
    100
}
#[derive(Default, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum DatePolicy {
    Planned,
    #[default]
    Best,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Evaluate {
    #[serde(default)]
    path: Option<String>,
    focus_wallclock: i64,
    #[serde(default)]
    date_mode: DatePolicy,
    #[serde(default)]
    interval_preview: bool,
    #[serde(default)]
    profile_effects: bool,
    #[serde(default)]
    offset: usize,
    #[serde(default = "page_size")]
    limit: usize,
}
#[derive(Debug, Clone, Default, serde::Serialize, Deserialize)]
pub struct SourceSelectionLinks {
    pub scene_id: String,
    pub task_guids: Vec<String>,
    pub resource_guids: Vec<String>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SelectionLinks {
    scene_id: String,
    task_guid: Option<String>,
    #[serde(default)]
    resource_guids: Vec<String>,
}
fn selection_links(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: SelectionLinks = parse_params(params.clone())?;
    if p.resource_guids.len() > 1024 || p.resource_guids.iter().any(|v| v.len() > 256) || p.task_guid.as_ref().is_some_and(|v| v.len() > 256) {
        return Err(ToolError::invalid("Selection joins accept at most 1024 resource identifiers of 256 bytes each."));
    }
    let loaded = ctx.session.ext.get::<LoadedScene>().ok_or_else(|| ToolError::invalid("Open a project scene first."))?;
    if p.scene_id != loaded.scene.id {
        return Err(ToolError::invalid("Selection belongs to a different scene."));
    }
    let (source, model) = model(ctx, None)?;
    let resources = p.resource_guids.into_iter().collect::<std::collections::BTreeSet<_>>();
    let result = json!({"scene_id":p.scene_id,"task_guids":model.resource_task_guids(&resources),
        "resource_guids":p.task_guid.as_deref().map(|id|model.task_resource_guids(id)).unwrap_or_default()});
    let ev = builder(ctx, "formats_project_selection_links", source.subject, params)
        .summary("Direct source task/resource assignment links")
        .confidence(Confidence::Derived)
        .result(result)
        .limitation(Limitation::new(
            "direct_assignments",
            "Links use stored assignments only; resource ancestry is not inferred and missing display IDs remain unknown.",
        ))
        .target_content(true)
        .build()?;
    Ok(ToolOutput::evidence(ev))
}
pub fn specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "formats_project_selection_links",
            "Link project selections",
            "Join source task and resource GUIDs using retained direct assignments, with current scene identity validation.",
            "Find resources assigned to a task or tasks assigned to selected model resources.",
            Toolset::Formats,
            Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<SelectionLinks>,
            selection_links,
        ),
        spec(
            "formats_project_tasks_list",
            "List project tasks",
            "Page retained source tasks and calendar-derived schedule bounds. Raw wallclock values retain their displayed-clock meaning; no timezone conversion or dependency rescheduling.",
            "Inspect the project schedule.",
            Toolset::Formats,
            Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<Page>,
            formats_project_tasks_list,
        ),
        spec(
            "formats_project_costs_list",
            "List project cost inputs",
            "Page source resource rates, units and task fixed costs with explicit unresolved fields. Source units are separate from reconstructed mesh quantities.",
            "Inspect budget and cost inputs.",
            Toolset::Formats,
            Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<Page>,
            formats_project_costs_list,
        ),
        spec(
            "formats_project_frame_evaluate",
            "Evaluate a project time",
            "Evaluate supported task appearance and declared source cost semantics at one displayed wallclock time. Retain unresolved profiles, dates and cost allocations instead of inventing values.",
            "Move through the project timeline and inspect its cost state.",
            Toolset::Formats,
            Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT),
            Requires::Nothing,
            schema_of::<Evaluate>,
            formats_project_frame_evaluate,
        ),
    ]
}
fn model(ctx: &mut Ctx<'_>, path: Option<&str>) -> Result<(ProjectSource, Arc<ProjectModel>), ToolError> {
    if let Some(path) = path {
        let input = load_path(path, 256 * 1024 * 1024)?;
        let imported = craft_anything_formats::sp_project::parse_metadata(&input.bytes, |progress| {
            ctx.report(Some(progress), "Reading project schedules and costs");
            !ctx.cancel.is_cancelled()
        });
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::cancelled());
        }
        let imported = imported.map_err(|e| format_err(e, "admitted project metadata"))?;
        let source = ProjectSource { scene_id: None, source_sha256: imported.source_sha256, subject: input.subject };
        if let Some(cached) = ctx.session.ext.get::<CachedMetadataProject>()
            && cached.source_sha256 == source.source_sha256
        {
            return Ok((source, cached.model.clone()));
        }
        let parsed = ProjectModel::from_metadata_with_cancel(&imported.metadata, || ctx.cancel.is_cancelled());
        if ctx.cancel.is_cancelled() {
            return Err(ToolError::cancelled());
        }
        let parsed = Arc::new(parsed.map_err(ToolError::invalid)?);
        ctx.session.ext.insert(Arc::new(CachedMetadataProject { source_sha256: source.source_sha256.clone(), model: parsed.clone() }));
        return Ok((source, parsed));
    }
    let source = ctx.session.ext.get::<LoadedScene>().ok_or_else(|| ToolError::invalid("Open a source project with formats_project_open first."))?;
    let identity =
        ProjectSource { scene_id: Some(source.scene.id.clone()), source_sha256: source.scene.source_sha256.clone(), subject: source.subject.clone() };
    if let Some(cached) = ctx.session.ext.get::<CachedProject>()
        && cached.scene_id == source.scene.id
    {
        return Ok((identity, cached.model.clone()));
    }
    let parsed = ProjectModel::from_metadata_with_cancel(&source.metadata, || ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let parsed = parsed.map_err(ToolError::invalid)?;
    let parsed = Arc::new(parsed);
    ctx.session.ext.insert(Arc::new(CachedProject { scene_id: source.scene.id.clone(), model: parsed.clone() }));
    Ok((identity, parsed))
}
pub(super) fn scene_model(ctx: &mut Ctx<'_>) -> Result<Arc<ProjectModel>, ToolError> {
    model(ctx, None).map(|(_, model)| model)
}
fn formats_project_tasks_list(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Page = parse_params(params.clone())?;
    let (source, model) = model(ctx, p.path.as_deref())?;
    let result = serde_json::to_value(model.page_tasks(p.offset, p.limit.clamp(1, 1000))).map_err(|_| ToolError::invalid("Cannot serialize task page."))?;
    let ev = builder(ctx, "formats_project_tasks_list", source.subject.clone(), params)
        .summary("Source project task page")
        .confidence(Confidence::Derived)
        .coverage(Coverage { status: CoverageStatus::Partial, ..Default::default() })
        .result(json!({"source":source.identity(),"scene_id":source.scene_id,"page":result}))
        .limitation(Limitation::new("metadata_geometry_scope", "An explicit project path validates metadata only; drawable geometry is not inspected."))
        .target_content(true)
        .build()?;
    Ok(ToolOutput::evidence(ev))
}
fn formats_project_costs_list(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Page = parse_params(params.clone())?;
    let (source, model) = model(ctx, p.path.as_deref())?;
    let result = serde_json::to_value(model.page_resources(p.offset, p.limit.clamp(1, 1000))).map_err(|_| ToolError::invalid("Cannot serialize cost page."))?;
    let ev = builder(ctx, "formats_project_costs_list", source.subject.clone(), params)
        .summary("Source project cost inputs")
        .confidence(Confidence::Derived)
        .coverage(Coverage { status: CoverageStatus::Partial, ..Default::default() })
        .result(json!({"source":source.identity(),"scene_id":source.scene_id,"page":result}))
        .limitation(Limitation::new("metadata_geometry_scope", "An explicit project path validates metadata only; drawable geometry is not inspected."))
        .target_content(true)
        .build()?;
    Ok(ToolOutput::evidence(ev))
}
fn formats_project_frame_evaluate(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Evaluate = parse_params(params.clone())?;
    let (source, model) = model(ctx, p.path.as_deref())?;
    let policy = match p.date_mode {
        DatePolicy::Planned => DateMode::Planned,
        DatePolicy::Best => DateMode::Best,
    };
    if p.profile_effects && p.interval_preview {
        return Err(ToolError::invalid("Choose profile effects or interval preview, not both."));
    }
    let appearance = if p.profile_effects {
        AppearancePolicy::ProfileEffectsPreview
    } else if p.interval_preview {
        AppearancePolicy::IntervalPreview
    } else {
        AppearancePolicy::Strict
    };
    let frame = Arc::new(model.evaluate_with_policy(p.focus_wallclock, policy, appearance, || ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?);
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let limit = p.limit.clamp(1, 1000);
    let mut page = frame.as_ref().clone();
    let phase_total = page.resource_phases.len();
    let cost_total = page.costs.lines.len();
    let expense_total = page.costs.recorded_actual_expenses.as_ref().map_or(0, |summary| summary.items.len());
    let reasons_total = page.unresolved_reasons.len();
    page.resource_phases = page.resource_phases.into_iter().skip(p.offset).take(limit).collect();
    page.costs.lines = page.costs.lines.into_iter().skip(p.offset).take(limit).collect();
    if let Some(expenses) = &mut page.costs.recorded_actual_expenses {
        expenses.items = expenses.items.drain(..).skip(p.offset).take(limit).collect();
    }
    page.unresolved_reasons = page.unresolved_reasons.into_iter().skip(p.offset).take(limit).collect();
    let scene_appearance = if source.scene_id.is_some() {
        let loaded = ctx.session.ext.get::<LoadedScene>().ok_or_else(|| ToolError::invalid("Scene changed during appearance evaluation."))?;
        Some(Arc::new(craft_anything_analysis::project_scene::derive(&loaded.scene, &frame, || ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?))
    } else {
        None
    };
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let appearance_page = scene_appearance.as_ref().map(|plan| json!({"styles_total":plan.styles.len(),"styles":plan.styles.iter().skip(p.offset).take(limit).map(|(id,style)|json!({"instance":id,"style":style})).collect::<Vec<_>>(),"hidden_instances_total":plan.hidden.len(),"ambiguous_instances":plan.ambiguous_instances,"limitations":plan.limitations}));
    let result = json!({"source":source.identity(),"scene_id":source.scene_id,"frame":page,"scene_appearance":appearance_page,"page":{"offset":p.offset,"limit":limit,"resource_phases_total":phase_total,"cost_lines_total":cost_total,"recorded_expense_items_total":expense_total,"unresolved_reasons_total":reasons_total}});
    if let Some(scene_id) = &source.scene_id {
        ctx.session.ext.insert(Arc::new(LatestProjectFrame {
            scene_id: scene_id.clone(),
            frame,
            scene_appearance: scene_appearance.ok_or_else(|| ToolError::invalid("Scene appearance is absent"))?,
        }));
    } else {
        ctx.session.ext.insert(Arc::new(LatestMetadataFrame { source_sha256: source.source_sha256.clone(), frame, model }));
    }
    let ev=builder(ctx,"formats_project_frame_evaluate",source.subject.clone(),params).summary("Evaluated project appearance and cost state")
        .confidence(Confidence::Derived).coverage(Coverage {status:CoverageStatus::Partial,..Default::default()}).result(result)
        .limitation(Limitation::new("scoped_source_semantics","Unresolved profiles or monetary allocations remain explicit. Source-unit, calendar and phase support is limited to admitted structures; this is not application-wide parity."))
        .limitation(Limitation::new("metadata_geometry_scope", "An explicit project path validates metadata only; drawable geometry is not inspected."))
        .target_content(true).build()?;
    Ok(ToolOutput::evidence(ev))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallOptions, Caller, Engine, Session, Settings};
    fn empty_metadata_project() -> Vec<u8> {
        let mut empty = vec![0; 24];
        empty.extend([0x7a, 0x54, 0xaf, 0x39]);
        empty.extend(0u32.to_le_bytes());
        empty.extend([0x16, 0xe4, 0x99, 0x6f]);
        // A single stored DEFLATE block keeps this fixture independent of codecs.
        let mut compressed = vec![0x78, 0x01, 0x01];
        let length = empty.len() as u16;
        compressed.extend(length.to_le_bytes());
        compressed.extend((!length).to_le_bytes());
        compressed.extend(&empty);
        let (mut a, mut b) = (1u32, 0u32);
        for value in empty {
            a = (a + u32::from(value)) % 65521;
            b = (b + a) % 65521;
        }
        compressed.extend(((b << 16) | a).to_be_bytes());
        let names = ["Tasks", "Resources", "Use Profiles", "Activity Codes", "User Fields", "Entities", "IncludeLibrary", "Calendars"];
        let mut raw = vec![0; 2344];
        raw[..16].copy_from_slice(&[0xb6, 0x17, 0x67, 0x1e, 0x7a, 0xf3, 0x46, 0x40, 0xa8, 0x6e, 0xd5, 0xdc, 0x41, 0x7a, 0x2d, 0xe2]);
        let mut version = vec![255, 254, 255, 8];
        for value in "6.5.5.28".encode_utf16() {
            version.extend(value.to_le_bytes());
        }
        raw[20..40].copy_from_slice(&version);
        for i in 0..48 {
            let name = names.get(i).map(|n| (*n).to_owned()).unwrap_or_else(|| format!("Other{i}"));
            let at = 40 + i * 48;
            raw[at..at + name.len()].copy_from_slice(name.as_bytes());
            let offset = raw.len() as u64;
            raw[at + 40..at + 48].copy_from_slice(&offset.to_le_bytes());
            raw.extend([1, name.len() as u8]);
            raw.extend(name.as_bytes());
            raw.extend(&compressed);
        }
        raw
    }
    #[test]
    fn selection_links_use_direct_assignments_and_reject_other_scene_ids() {
        let dir = tempfile::tempdir().unwrap();
        for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        let engine = Engine::new(crate::testing::session());
        engine.call("formats_scene_bundle_open", json!({"path":dir.path().join("scene.json")}), CallOptions::new(Caller::Cli)).unwrap();
        let id = {
            let mut session = engine.session();
            let source = session.ext.get::<LoadedScene>().unwrap();
            let metadata = json!({"schema_version":4,"tasks":[{"guid":"task-a","name":"Task A","planned_start_wallclock_scalar":100,"duration_storage_scalar":20,"status_storage_value":2,
                "assignments":[{"resource_guid":"part","profile_guid":"profile","planned_units":1.}]}],"resources":[],"definition_storage":{}});
            let id = source.scene.id.clone();
            session.ext.insert(Arc::new(LoadedScene {
                scene: source.scene.clone(),
                manifest: source.manifest.clone(),
                subject: source.subject.clone(),
                metadata: Arc::new(metadata),
            }));
            id
        };
        let output = engine
            .call("formats_project_selection_links", json!({"scene_id":id,"task_guid":"task-a","resource_guids":["part"]}), CallOptions::new(Caller::Cli))
            .unwrap()
            .to_json();
        assert_eq!(output["result"]["resource_guids"], json!(["part"]));
        assert_eq!(output["result"]["task_guids"], json!(["task-a"]));
        assert!(engine.call("formats_project_selection_links", json!({"scene_id":"stale"}), CallOptions::new(Caller::Cli)).is_err());
    }
    #[test]
    fn explicit_metadata_path_works_without_a_scene() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("schedule.sp");
        std::fs::write(&path, empty_metadata_project()).unwrap();
        let engine = Engine::new(crate::testing::session());
        for tool in ["formats_project_tasks_list", "formats_project_costs_list"] {
            let output = engine.call(tool, json!({"path":path}), CallOptions::new(Caller::Cli)).unwrap().to_json();
            assert!(output["result"]["scene_id"].is_null());
            assert_eq!(output["result"]["source"]["geometry_coverage"], "unvalidated");
            assert_eq!(output["result"]["page"]["total"], 0);
        }
        assert!(engine.session().ext.get::<LoadedScene>().is_none());
    }
    #[test]
    fn explicit_metadata_frame_preserves_loaded_scene_snapshots() {
        let dir = tempfile::tempdir().unwrap();
        for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        let path = dir.path().join("schedule.sp");
        std::fs::write(&path, empty_metadata_project()).unwrap();
        let engine = Engine::new(crate::testing::session());
        engine.call("formats_scene_bundle_open", json!({"path":dir.path().join("scene.json")}), CallOptions::new(Caller::Cli)).unwrap();
        {
            let mut session = engine.session();
            let source = session.ext.get::<LoadedScene>().unwrap();
            let mut metadata = source.metadata.as_ref().clone();
            metadata["tasks"] = json!([]);
            metadata["definition_storage"] = json!({});
            session.ext.insert(Arc::new(LoadedScene {
                scene: source.scene.clone(),
                manifest: source.manifest.clone(),
                subject: source.subject.clone(),
                metadata: Arc::new(metadata),
            }));
        }
        engine.call("formats_project_frame_evaluate", json!({"focus_wallclock":0}), CallOptions::new(Caller::Cli)).unwrap();
        let (source, cached, frame) = {
            let session = engine.session();
            (session.ext.get::<LoadedScene>().unwrap(), session.ext.get::<CachedProject>().unwrap(), session.ext.get::<LatestProjectFrame>().unwrap())
        };
        let output =
            engine.call("formats_project_frame_evaluate", json!({"path":path,"focus_wallclock":0,"limit":1}), CallOptions::new(Caller::Cli)).unwrap().to_json();
        assert!(output["result"]["scene_id"].is_null());
        let session = engine.session();
        assert!(Arc::ptr_eq(&source, &session.ext.get::<LoadedScene>().unwrap()));
        assert!(Arc::ptr_eq(&cached, &session.ext.get::<CachedProject>().unwrap()));
        assert!(Arc::ptr_eq(&frame, &session.ext.get::<LatestProjectFrame>().unwrap()));
        let detached = session.ext.get::<LatestMetadataFrame>().unwrap();
        assert_eq!(detached.source_sha256, output["result"]["source"]["source_sha256"]);
        assert_eq!(detached.frame.resource_phases.len(), 0);
    }
    #[test]
    fn project_frame_is_paged_but_typed_cache_retains_all_resources() {
        let dir = tempfile::tempdir().unwrap();
        for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        let engine = Engine::new(Session::new(Settings { deep_mode: true, ..Default::default() }));
        engine.call("formats_scene_bundle_open", json!({"path":dir.path().join("scene.json")}), CallOptions::new(Caller::Cli)).unwrap();
        {
            let mut session = engine.session();
            let source = session.ext.get::<LoadedScene>().unwrap();
            let resources = (0..300).map(|i| json!({"guid":format!("resource-{i}"),"name":format!("Resource {i}")})).collect::<Vec<_>>();
            session.ext.insert(Arc::new(LoadedScene {
                scene: source.scene.clone(),
                manifest: source.manifest.clone(),
                subject: source.subject.clone(),
                metadata: Arc::new(json!({"schema_version":4,"tasks":[],"resources":resources,"definition_storage":{}})),
            }));
        }
        let result = engine
            .call("formats_project_frame_evaluate", json!({"focus_wallclock":0,"offset":20,"limit":10}), CallOptions::new(Caller::Cli))
            .unwrap()
            .to_json();
        assert_eq!(result["result"]["page"]["resource_phases_total"], 300);
        assert_eq!(result["result"]["frame"]["resource_phases"].as_array().unwrap().len(), 10);
        assert_eq!(engine.session().ext.get::<LatestProjectFrame>().unwrap().frame.resource_phases.len(), 300);
    }
    #[test]
    fn recorded_expense_items_are_paged_without_truncating_typed_frame() {
        let dir = tempfile::tempdir().unwrap();
        for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
            std::fs::write(dir.path().join(name), bytes).unwrap();
        }
        let engine = Engine::new(crate::testing::session());
        engine.call("formats_scene_bundle_open", json!({"path":dir.path().join("scene.json")}), CallOptions::new(Caller::Cli)).unwrap();
        {
            let mut session = engine.session();
            let source = session.ext.get::<LoadedScene>().unwrap();
            let items = (0..300)
                .map(|index| {
                    json!({"item_guid":format!("expense-item-{index}"),"name":"Original fixture expense",
                "amount_storage":1.,"type_storage_value":1,"definition_reference_guid_storage":"00000000-0000-0000-0000-000000000000"})
                })
                .collect::<Vec<_>>();
            let task = json!({"guid":"expense-task","name":"Synthetic expense task","planned_start_wallclock_scalar":0,
                "duration_storage_scalar":100,"status_storage_value":2,"date_storage_windows":[0,32535244799i64,100,32535244799i64],"assignments":[],
                "actual_expense_coverage":{"progress_collection_present":true,"report_collection_complete":true,"report_count":1,"item_count":300,
                    "policy":"observed_type5_report_with_fixed_cost_items"},
                "actual_expense_reports":[{"report_guid":"expense-report","date_storage_scalar":100,"label_storage":"Original fixture",
                    "report_duration_storage_scalar":0,"type_storage_value":5,"items":items}]});
            session.ext.insert(Arc::new(LoadedScene {
                scene: source.scene.clone(),
                manifest: source.manifest.clone(),
                subject: source.subject.clone(),
                metadata: Arc::new(json!({"schema_version":4,"tasks":[task],"resources":[],"definition_storage":{}})),
            }));
        }
        let output = engine
            .call("formats_project_frame_evaluate", json!({"focus_wallclock":100,"offset":20,"limit":10}), CallOptions::new(Caller::Cli))
            .unwrap()
            .to_json();
        assert_eq!(output["result"]["page"]["recorded_expense_items_total"], 300);
        assert_eq!(output["result"]["frame"]["costs"]["recorded_actual_expenses"]["items"].as_array().unwrap().len(), 10);
        assert_eq!(output["result"]["frame"]["costs"]["recorded_actual_expenses"]["recorded_total_minor"], 30000);
        let latest = engine.session().ext.get::<LatestProjectFrame>().unwrap();
        assert_eq!(latest.frame.costs.recorded_actual_expenses.as_ref().unwrap().items.len(), 300);
    }
    #[test]
    fn project_tools_require_deep_mode_and_open_source() {
        let engine = Engine::new(Session::new(Settings::default()));
        for id in ["formats_project_open", "formats_project_tasks_list", "formats_project_costs_list", "formats_project_frame_evaluate"] {
            assert!(engine.call(id, json!({}), CallOptions::new(Caller::Cli)).is_err());
        }
        let engine = Engine::new(Session::new(Settings { deep_mode: true, ..Default::default() }));
        assert!(engine.call("formats_project_tasks_list", json!({}), CallOptions::new(Caller::Cli)).unwrap_err().message.contains("formats_project_open"));
    }
}
