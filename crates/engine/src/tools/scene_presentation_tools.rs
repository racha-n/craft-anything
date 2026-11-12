//! Read-only project audit, snapshot reports and scene-bound camera/video presentation.
use super::{
    native_common::spec,
    scene_project_tools::{self, ProjectModel},
    scene_tools::LoadedScene,
    static_input::builder,
};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
use craft_anything_analysis::{
    project_model::{AppearancePolicy, DateMode, Frame},
    project_simulation::{Phase, PhaseState},
};
use craft_anything_engines::scene_render::{
    self, Palette,
    animation::Animation,
    camera::Camera,
    gpu::{FrameData, InstanceStyle},
};
use craft_anything_evidence::{Confidence, Limitation, ToolError};
use craft_anything_formats::scene_bundle::{Scene, Visibility};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Write,
    path::PathBuf,
    sync::Arc,
};
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct SceneRequest {
    scene_id: String,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct AnimationRequest {
    scene_id: String,
    animation: Option<Animation>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Filters {
    pub hidden_resources: BTreeSet<usize>,
    pub isolate_resources: BTreeSet<usize>,
    pub hidden_instances: BTreeSet<usize>,
    pub isolate_instances: BTreeSet<usize>,
}
impl Filters {
    fn visibility(&self) -> Visibility {
        Visibility {
            hidden_resources: self.hidden_resources.clone(),
            isolate_resources: self.isolate_resources.clone(),
            hidden_instances: self.hidden_instances.clone(),
            isolate_instances: self.isolate_instances.clone(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct RenderOptions {
    pub best_dates: bool,
    pub schedule: bool,
    pub profile_effects: bool,
    pub interval_preview: bool,
    pub source_colors: bool,
    pub object_colors: bool,
    pub texture_preview: bool,
    pub flip_texture_v: bool,
    pub filters: Filters,
    pub palette: Palette,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            best_dates: false,
            schedule: true,
            profile_effects: false,
            interval_preview: false,
            source_colors: false,
            object_colors: true,
            texture_preview: false,
            flip_texture_v: false,
            filters: Filters::default(),
            palette: Palette::default(),
        }
    }
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct VideoRequest {
    scene_id: String,
    animation: Animation,
    output_path: String,
    #[serde(default = "default_size")]
    size: [u32; 2],
    #[serde(default = "default_fps")]
    fps: u32,
    #[serde(default)]
    options: RenderOptions,
}
fn default_size() -> [u32; 2] {
    [640, 360]
}
fn default_fps() -> u32 {
    12
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct ReportRequest {
    scene_id: String,
    output_dir: String,
    focus: i64,
    #[serde(default)]
    options: RenderOptions,
}
pub fn specs() -> Vec<ToolSpec> {
    let e = Effects::EVIDENCE.union(Effects::CARRIES_TARGET_CONTENT);
    let w = Effects::WRITES_FILESYSTEM.union(e);
    vec![
        spec(
            "formats_project_audit",
            "Audit project identity and appearance",
            "Inventory reader display-ID fields and admitted colour/texture evidence without promoting unknown native semantics to facts.",
            "See which source values are known and which still need validation.",
            Toolset::Formats,
            e,
            Requires::Nothing,
            schema_of::<SceneRequest>,
            audit,
        ),
        spec(
            "formats_scene_animation_manage",
            "Manage camera animation",
            "Load or save a bounded version-1 scene-bound camera/focus path in a sidecar. Saving does not modify the source project.",
            "Save camera keyframes and project times for 4D presentation.",
            Toolset::Formats,
            w,
            Requires::Nothing,
            schema_of::<AnimationRequest>,
            animation,
        ),
        spec(
            "formats_scene_video_export",
            "Export a project video",
            "Render a scene-bound camera/focus path with the shared renderer to a new lossless 24-bit AVI plus a versioned frame manifest. Up to 900 frames and 512 MiB. Last keyframe included; encoded video holds the last frame up to one frame interval. No existing files are replaced.",
            "Export the model and 4D camera path as a video.",
            Toolset::Formats,
            w,
            Requires::Nothing,
            schema_of::<VideoRequest>,
            video,
        ),
        spec(
            "formats_project_report_export",
            "Export project reports",
            "Write a new source-hash/focus-bound CSV report folder and JSON snapshot with separate budgets, forecasts and recorded expense totals. Missing amounts stay empty; no currency conversion or native project write.",
            "Export task, resource, schedule and cost tables.",
            Toolset::Formats,
            w,
            Requires::Nothing,
            schema_of::<ReportRequest>,
            report,
        ),
    ]
}
fn loaded(ctx: &mut Ctx<'_>, id: &str) -> Result<Arc<LoadedScene>, ToolError> {
    let s = ctx.session.ext.get::<LoadedScene>().ok_or_else(|| ToolError::invalid("Open a project or scene first."))?;
    if s.scene.id != id {
        return Err(ToolError::invalid("This request belongs to a different scene."));
    }
    Ok(s)
}
fn check(ctx: &Ctx<'_>) -> Result<(), ToolError> {
    if ctx.cancel.is_cancelled() { Err(ToolError::cancelled()) } else { Ok(()) }
}
fn audit_value(s: &LoadedScene, model: &ProjectModel) -> Value {
    let ids = model.tasks.iter().filter(|t| !t.display_id.is_empty()).count();
    json!({"schema_version":1,"scene_id":s.scene.id,"source_sha256":s.scene.source_sha256,"tasks":model.tasks.len(),"reader_display_ids_present":ids,"reader_display_ids_blank":model.tasks.len()-ids,"native_display_id_semantics_verified":false,"identity_policy":"blank reader fields retain GUID fallback; no generated native IDs","appearance":{"object_group_rgb_candidates":s.scene.object_colors.len(),"source_diffuse_candidates":s.scene.source_colors.len(),"textured_mesh_candidates":s.scene.mesh_textures.len(),"planar_uv_mesh_candidates":s.scene.mesh_uvs.len(),"object_colour_policy":craft_anything_formats::scene_bundle::OBJECT_COLOR_POLICY,"native_material_parity_verified":false,"source_opacity_conversion_verified":false,"texture_sampling_verified":false},"limitations":["Reader display-ID storage is not an independently validated native UI ID rule.","Controlled group/object RGB evidence does not verify all material channels, source-opacity conversion or texture sampling.","Source geometry units and engineering aggregate quantities are not certified."]})
}
fn audit(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: SceneRequest = parse_params(params.clone())?;
    let s = loaded(ctx, &p.scene_id)?;
    let model = scene_project_tools::scene_model(ctx)?;
    let value = audit_value(&s, &model);
    let ev = builder(ctx, "formats_project_audit", s.subject.clone(), params)
        .summary("Source identity and appearance coverage audit")
        .confidence(Confidence::Derived)
        .result(value)
        .limitation(Limitation::new(
            "unknown_native_semantics",
            "Native ID generation, source opacity and full material parity remain unresolved until independent controls establish them.",
        ))
        .target_content(true)
        .build()?;
    Ok(ToolOutput::evidence(ev))
}
fn animation(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: AnimationRequest = parse_params(params)?;
    let s = loaded(ctx, &p.scene_id)?;
    let id = &s.scene.id;
    if id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ToolError::invalid("Invalid scene digest"));
    }
    let path = ctx.session.config_dir().join("scene-animations").join(format!("{id}.json"));
    let previous = if path.try_exists().map_err(|e| ToolError::io(path.display(), e))? {
        let bytes = craft_anything_platform::fs::read_file_capped(&path, 128 * 1024)?;
        let value: Animation = serde_json::from_slice(&bytes).map_err(|e| ToolError::invalid(format!("Invalid animation sidecar: {e}")))?;
        value.validate().map_err(ToolError::invalid)?;
        if value.scene_id != *id {
            return Err(ToolError::invalid("Animation sidecar scene mismatch"));
        }
        Some(value)
    } else {
        None
    };
    let value = if let Some(value) = p.animation {
        value.validate().map_err(ToolError::invalid)?;
        if value.scene_id != *id {
            return Err(ToolError::invalid("Animation belongs to a different scene"));
        }
        check(ctx)?;
        craft_anything_platform::write_atomic_private(&path, &serde_json::to_vec_pretty(&value).map_err(|e| ToolError::invalid(e.to_string()))?)?;
        Some(value)
    } else {
        previous
    };
    Ok(ToolOutput::Value(json!({"version":1,"scene_id":id,"animation":value})))
}
fn validate_options(s: &Scene, o: &RenderOptions) -> Result<(), ToolError> {
    for color in [o.palette.canvas, o.palette.text_dim, o.palette.accent, o.palette.warning, o.palette.success] {
        if color.iter().any(|c| !c.is_finite() || !(0.0..=1.0).contains(c)) {
            return Err(ToolError::invalid("Render colours must be finite values within 0..1"));
        }
    }
    for set in [&o.filters.hidden_resources, &o.filters.isolate_resources] {
        if set.len() > 10000 || set.iter().any(|i| *i >= s.resources.len()) {
            return Err(ToolError::invalid("Invalid resource visibility filters"));
        }
    }
    for set in [&o.filters.hidden_instances, &o.filters.isolate_instances] {
        if set.len() > 100000 || set.iter().any(|i| *i >= s.instances.len()) {
            return Err(ToolError::invalid("Invalid instance visibility filters"));
        }
    }
    Ok(())
}
pub fn phase_state(scene: &Scene, frame: &Frame) -> (BTreeSet<usize>, BTreeMap<usize, u8>) {
    let ids = scene.resources.iter().enumerate().map(|(i, r)| (r.guid.as_str(), i)).collect::<BTreeMap<_, _>>();
    let mut phases = BTreeMap::new();
    for p in &frame.resource_phases {
        if let Some(id) = ids.get(p.resource_guid.as_str())
            && let PhaseState::Resolved { phase, .. } = p.state
        {
            phases.insert(
                *id,
                match phase {
                    Phase::Hidden => 0,
                    Phase::InstallActive => 1,
                    Phase::MaintainActive => 2,
                    Phase::OriginalMaterial => 3,
                },
            );
        }
    }
    let hidden = scene
        .instances
        .iter()
        .enumerate()
        .filter(|(_, i)| !i.resources.is_empty() && i.resources.iter().all(|r| phases.get(r) == Some(&0)))
        .map(|(i, _)| i)
        .collect();
    (hidden, phases)
}
fn evaluate(ctx: &Ctx<'_>, model: &ProjectModel, focus: i64, o: &RenderOptions) -> Result<Frame, ToolError> {
    model
        .evaluate_with_policy(
            focus,
            if o.best_dates { DateMode::Best } else { DateMode::Planned },
            if o.profile_effects {
                AppearancePolicy::ProfileEffectsPreview
            } else if o.interval_preview {
                AppearancePolicy::IntervalPreview
            } else {
                AppearancePolicy::Strict
            },
            || ctx.cancel.is_cancelled(),
        )
        .map_err(ToolError::invalid)
}
pub fn render_snapshot(
    cancel: &crate::CancelToken,
    s: &Scene,
    model: &ProjectModel,
    camera: Camera,
    focus: i64,
    size: [u32; 2],
    o: &RenderOptions,
) -> Result<FrameData, ToolError> {
    let mut visibility = o.filters.visibility();
    let (mut phases, mut styles) = (BTreeMap::new(), BTreeMap::new());
    if o.schedule {
        let frame = model
            .evaluate_with_policy(
                focus,
                if o.best_dates { DateMode::Best } else { DateMode::Planned },
                if o.profile_effects {
                    AppearancePolicy::ProfileEffectsPreview
                } else if o.interval_preview {
                    AppearancePolicy::IntervalPreview
                } else {
                    AppearancePolicy::Strict
                },
                || cancel.is_cancelled(),
            )
            .map_err(ToolError::invalid)?;
        let (hidden, p) = phase_state(s, &frame);
        visibility.hidden_instances.extend(hidden);
        phases = p;
        let appearance = craft_anything_analysis::project_scene::derive(s, &frame, || cancel.is_cancelled()).map_err(ToolError::invalid)?;
        visibility.hidden_instances.extend(&appearance.hidden);
        styles = appearance
            .styles
            .into_iter()
            .map(|(i, s)| (i, InstanceStyle { color: s.color, opacity: s.opacity.unwrap_or(1.), clip_plane: s.clip_plane }))
            .collect();
    }
    Ok(FrameData {
        camera,
        visibility,
        selected: None,
        selected_resource: None,
        size,
        tokens: o.palette.clone(),
        phases,
        source_colors: o.source_colors,
        object_colors: o.object_colors,
        instance_styles: styles,
        texture_preview: o.texture_preview,
        flip_texture_v: o.flip_texture_v,
    })
}
fn new_path(raw: &str) -> Result<PathBuf, ToolError> {
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        return Err(ToolError::invalid("Choose an absolute output path."));
    }
    if path.try_exists().map_err(|e| ToolError::io(path.display(), e))? {
        return Err(ToolError::invalid("Output already exists; choose a new name. Existing files are preserved."));
    }
    let parent = path.parent().ok_or_else(|| ToolError::invalid("Output has no parent folder"))?;
    if !parent.is_dir() {
        return Err(ToolError::invalid("Choose an existing parent folder."));
    }
    Ok(path)
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn video(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: VideoRequest = parse_params(params.clone())?;
    let s = loaded(ctx, &p.scene_id)?;
    if p.animation.scene_id != p.scene_id {
        return Err(ToolError::invalid("Video path belongs to another scene"));
    }
    validate_options(&s.scene, &p.options)?;
    let frames = p.animation.frame_count(p.fps).map_err(ToolError::invalid)?;
    let path = new_path(&p.output_path)?;
    if path.extension().and_then(|x| x.to_str()).is_none_or(|x| !x.eq_ignore_ascii_case("avi")) {
        return Err(ToolError::invalid("Lossless video export requires a new .avi filename."));
    }
    let manifest_path = path.with_extension("avi.manifest.json");
    new_path(&manifest_path.to_string_lossy())?;
    let model = scene_project_tools::scene_model(ctx)?;
    let parent = path.parent().ok_or_else(|| ToolError::invalid("No output parent"))?;
    let temp = tempfile::tempdir_in(parent).map_err(|e| ToolError::io(parent.display(), e))?;
    let staging = temp.path().join("video.avi");
    let file = std::fs::File::create(&staging).map_err(|e| ToolError::io(staging.display(), e))?;
    let mut writer = scene_render::avi::AviWriter::new(file, p.size[0], p.size[1], p.fps, frames).map_err(ToolError::invalid)?;
    let encoded_seconds = writer.encoded_seconds();
    check(ctx)?;
    let (mut gpu, adapter) = scene_render::headless(&s.scene, &ctx.cancel).map_err(ToolError::invalid)?;
    let mut records = Vec::new();
    let started = std::time::Instant::now();
    for index in 0..frames {
        check(ctx)?;
        if started.elapsed() > std::time::Duration::from_secs(900) {
            return Err(ToolError::invalid("Video export exceeded its 15 minute deadline; lower resolution or shorten the path."));
        }
        ctx.report(Some(index as f32 / frames as f32), "Rendering project video");
        let seconds = (f64::from(index) / f64::from(p.fps)).min(p.animation.duration());
        let key = p.animation.sample(seconds).map_err(ToolError::invalid)?;
        let data = render_snapshot(&ctx.cancel, &s.scene, &model, key.camera.clone(), key.focus, p.size, &p.options)?;
        gpu.render(&s.scene, &data, &ctx.cancel).map_err(ToolError::invalid)?;
        let pixels = gpu.rgba(&ctx.cancel).map_err(ToolError::invalid)?;
        writer.frame(&pixels).map_err(ToolError::invalid)?;
        records.push(json!({"index":index,"seconds":seconds,"focus":key.focus,"camera":key.camera,"rgba_sha256":digest(&pixels),"draw":gpu.stats}));
    }
    let file = writer.finish().map_err(ToolError::invalid)?;
    file.sync_all().map_err(|e| ToolError::io(staging.display(), e))?;
    drop(file);
    check(ctx)?;
    let meta = json!({"version":1,"scene_id":p.scene_id,"source_sha256":s.scene.source_sha256,"format":"AVI RGB24 lossless","size":p.size,"fps":p.fps,"frame_count":frames,"path_duration_seconds":p.animation.duration(),"encoded_seconds":encoded_seconds,"last_frame_policy":"include last keyframe and hold for up to one frame interval","adapter":adapter,"animation":p.animation,"options":p.options,"frames":records,"native_visual_parity_verified":false});
    let meta_staging = temp.path().join("manifest.json");
    craft_anything_platform::write_atomic_private(&meta_staging, &serde_json::to_vec_pretty(&meta).map_err(|e| ToolError::invalid(e.to_string()))?)?;
    std::fs::hard_link(&staging, &path).map_err(|e| ToolError::io(path.display(), e))?;
    std::fs::hard_link(&meta_staging, &manifest_path).map_err(|e| {
        ToolError::io(manifest_path.display(), e)
            .with_remediation("The AVI was created, but its manifest could not be published. Preserve it and choose a new export name.")
    })?;
    let ev=builder(ctx,"formats_scene_video_export",s.subject.clone(),params).summary(format!("Exported {frames} lossless video frames")).confidence(Confidence::Derived).result(json!({"path":path,"manifest":manifest_path,"frames":frames,"encoded_seconds":encoded_seconds,"scene_id":s.scene.id,"source_sha256":s.scene.source_sha256})).limitation(Limitation::new("preview_scope","Video uses admitted renderer and 4D policies. Source-opacity, full native material parity and engineering units are not certified. Selection highlights and UI overlays are omitted.")).target_content(true).build()?;
    Ok(ToolOutput::evidence(ev))
}
fn csv_cell(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => {
            let text = if s.trim_start().starts_with(['=', '+', '-', '@']) || s.starts_with(['\t', '\r']) { format!("'{s}") } else { s.clone() };
            format!("\"{}\"", text.replace('"', "\"\""))
        }
        Value::Bool(_) | Value::Number(_) => v.to_string(),
        _ => csv_cell(&Value::String(v.to_string())),
    }
}
fn csv(rows: &[Value]) -> Result<Vec<u8>, ToolError> {
    let headers = rows.iter().filter_map(Value::as_object).flat_map(|r| r.keys().cloned()).collect::<BTreeSet<_>>();
    let mut text =
        if headers.is_empty() { "no_rows\r\n".to_owned() } else { headers.iter().map(|h| csv_cell(&json!(h))).collect::<Vec<_>>().join(",") + "\r\n" };
    for row in rows {
        let line = headers.iter().map(|h| csv_cell(row.get(h).unwrap_or(&Value::Null))).collect::<Vec<_>>().join(",");
        if text.len().saturating_add(line.len()) > 32 * 1024 * 1024 {
            return Err(ToolError::invalid("CSV report exceeds 32 MiB; reduce the source dataset."));
        }
        text.push_str(&line);
        text.push_str("\r\n");
    }
    Ok(text.into_bytes())
}
fn values<T: Serialize>(items: &[T]) -> Result<Vec<Value>, ToolError> {
    items.iter().map(|i| serde_json::to_value(i).map_err(|e| ToolError::invalid(e.to_string()))).collect()
}
fn report(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: ReportRequest = parse_params(params.clone())?;
    let s = loaded(ctx, &p.scene_id)?;
    validate_options(&s.scene, &p.options)?;
    let output = new_path(&p.output_dir)?;
    let model = scene_project_tools::scene_model(ctx)?;
    let frame = evaluate(ctx, &model, p.focus, &p.options)?;
    let audit = audit_value(&s, &model);
    let tasks = values(&model.tasks)?;
    let resources = values(&model.resources)?;
    let mut costs = values(&frame.costs.lines)?;
    for row in &mut costs {
        let direct = row.get("resource_guid").is_some_and(Value::is_null);
        row["cost_basis"] = json!(if direct { "direct_budget" } else { "resource_forecast" });
    }
    let expenses = frame.costs.recorded_actual_expenses.as_ref().map(|e| values(&e.items)).transpose()?.unwrap_or_default();
    let phases = values(&frame.resource_phases)?;
    let snapshot = json!({"version":1,"scene_id":p.scene_id,"source_sha256":s.scene.source_sha256,"focus":p.focus,"tasks":model.tasks,"resources":model.resources,"frame":frame,"audit":audit});
    let mut files = BTreeMap::new();
    let mut tables = Vec::new();
    for (name, rows) in [("tasks.csv", tasks), ("resources.csv", resources), ("costs.csv", costs), ("actual_expenses.csv", expenses), ("phases.csv", phases)] {
        check(ctx)?;
        let bytes = csv(&rows)?;
        tables.push(json!({"file":name,"rows":rows.len(),"sha256":digest(&bytes)}));
        files.insert(name.to_owned(), bytes);
    }
    let raw = serde_json::to_vec_pretty(&snapshot).map_err(|e| ToolError::invalid(e.to_string()))?;
    if raw.len() > 64 * 1024 * 1024 {
        return Err(ToolError::invalid("Snapshot exceeds the 64 MiB report budget."));
    }
    files.insert("snapshot.json".into(), raw);
    let manifest = json!({"version":1,"scene_id":p.scene_id,"source_sha256":s.scene.source_sha256,"focus":p.focus,"options":p.options,"tables":tables,"snapshot_sha256":files.get("snapshot.json").map(|b|digest(b)),"totals":{"direct_budget_minor":frame.costs.direct_budget_minor,"resource_forecast_minor":frame.costs.resource_budget_minor,"unresolved_direct_lines":frame.costs.unresolved_direct_budget_lines,"unresolved_resource_lines":frame.costs.unresolved_resource_budget_lines,"recorded_expenses":frame.costs.recorded_actual_expenses.as_ref().map(|e|json!({"recorded_total_minor":e.recorded_total_minor,"recorded_at_focus_minor":e.recorded_at_focus_minor,"partial_recorded_total_minor":e.partial_recorded_total_minor,"partial_recorded_at_focus_minor":e.partial_recorded_at_focus_minor}))},"spreadsheet_text_policy":"Potential formula text is prefixed with an apostrophe; raw values remain in snapshot.json.","money_policy":"Stored source currency context, scale 100; no currency/unit conversion. Direct budgets, resource forecasts and recorded expenses are separate.","native_parity_verified":false,"completion_marker":"manifest.json is written last; a folder without it is incomplete"});
    check(ctx)?;
    std::fs::create_dir(&output).map_err(|e| ToolError::io(output.display(), e))?;
    for (name, bytes) in files {
        check(ctx)?;
        let path = output.join(name);
        let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|e| ToolError::io(path.display(), e))?;
        file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| ToolError::io(path.display(), e))?;
    }
    check(ctx)?;
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| ToolError::invalid(e.to_string()))?;
    let mut manifest_file = tempfile::NamedTempFile::new_in(&output).map_err(|e| ToolError::io(output.display(), e))?;
    manifest_file.write_all(&manifest_bytes).and_then(|_| manifest_file.as_file().sync_all()).map_err(|e| ToolError::io(output.display(), e))?;
    manifest_file.persist_noclobber(output.join("manifest.json")).map_err(|e| ToolError::io(output.display(), e.error))?;
    let ev=builder(ctx,"formats_project_report_export",s.subject.clone(),params).summary("Exported source-bound project report tables").confidence(Confidence::Derived).result(json!({"output_dir":output,"manifest":manifest})).limitation(Limitation::new("report_scope","Missing amounts remain empty. Partial subtotals retain unresolved counts; no engineering aggregate, currency conversion or complete native parity is inferred.")).target_content(true).build()?;
    Ok(ToolOutput::evidence(ev))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn csv_quotes_and_guards_text_without_rewriting_numbers() {
        let bytes = csv(&[json!({"name":"=SUM(1,2)","amount":-12,"note":"line1\n\"line2\""})]).unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("\"'=SUM(1,2)\""));
        assert!(text.contains("-12,"));
        assert!(text.contains("\"\"line2\"\""));
        assert!(!text.contains("'-12"));
    }
    #[test]
    fn existing_output_is_not_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keep.avi");
        std::fs::write(&path, b"keep").unwrap();
        assert!(new_path(&path.to_string_lossy()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"keep");
    }
}
