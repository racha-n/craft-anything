//! Scene-bound camera bookmarks. Source project bytes are never changed.
use super::{native_common::spec, scene_tools::LoadedScene};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
use craft_anything_evidence::ToolError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use craft_anything_engines::scene_render::camera::Camera as CameraPose;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewpoint {
    pub name: String,
    pub camera: CameraPose,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewpoints {
    pub version: u32,
    pub scene_id: String,
    pub views: Vec<Viewpoint>,
}
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
struct Params {
    scene_id: String,
    /// Omit both name and camera to list. Name with camera saves/replaces; name alone deletes.
    name: Option<String>,
    camera: Option<CameraPose>,
}
pub fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "formats_scene_viewpoints_manage",
        "Manage saved scene viewpoints",
        "List, save or delete up to 64 named camera poses in a versioned scene-bound sidecar. Does not edit the source project. A name and camera saves; a name without camera deletes; omit both to list.",
        "Save camera positions and restore them after reopening the same model.",
        Toolset::Formats,
        Effects::WRITES_FILESYSTEM.union(Effects::CARRIES_TARGET_CONTENT),
        Requires::Nothing,
        schema_of::<Params>,
        run,
    )]
}
fn validate_book(book: &Viewpoints, scene_id: &str) -> Result<(), ToolError> {
    if book.version != 1 || book.scene_id != scene_id || book.views.len() > 64 {
        return Err(ToolError::invalid("Unsupported viewpoint version, scene identity or view count. Keep a backup and create a new sidecar."));
    }
    let mut names = std::collections::BTreeSet::new();
    for view in &book.views {
        validate_name(&view.name)?;
        view.camera.validate()?;
        if !names.insert(&view.name) {
            return Err(ToolError::invalid("Duplicate saved viewpoint name."));
        }
    }
    Ok(())
}
fn validate_name(name: &str) -> Result<(), ToolError> {
    if name.trim().is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
        return Err(ToolError::invalid("Use a nonempty viewpoint name of at most 160 bytes without control characters."));
    }
    Ok(())
}
fn run(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let p: Params = parse_params(params)?;
    let loaded = ctx.session.ext.get::<LoadedScene>().ok_or_else(|| ToolError::invalid("Open a scene before managing viewpoints."))?;
    if loaded.scene.id != p.scene_id {
        return Err(ToolError::invalid("The viewpoint request belongs to a different scene."));
    }
    // Identity is also the filename. Never accept separators or caller-selected paths.
    if p.scene_id.len() != 64 || !p.scene_id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(ToolError::invalid("The scene identity is not a SHA-256 digest."));
    }
    let path = ctx.session.config_dir().join("scene-viewpoints").join(format!("{}.json", p.scene_id));
    let mut book = if path.try_exists().map_err(|e| ToolError::io(path.display(), e))? {
        let bytes = craft_anything_platform::fs::read_file_capped(&path, 128 * 1024)?;
        serde_json::from_slice::<Viewpoints>(&bytes)
            .map_err(|e| ToolError::invalid(format!("Cannot read saved viewpoints: {e}. Preserve the sidecar before repairing it.")))?
    } else {
        Viewpoints { version: 1, scene_id: p.scene_id.clone(), views: Vec::new() }
    };
    validate_book(&book, &p.scene_id)?;
    if let Some(name) = p.name {
        validate_name(&name)?;
        if let Some(camera) = p.camera {
            camera.validate()?;
            if let Some(view) = book.views.iter_mut().find(|v| v.name == name) {
                view.camera = camera;
            } else {
                if book.views.len() >= 64 {
                    return Err(ToolError::invalid("The 64 viewpoint limit is reached; delete a saved view first."));
                }
                book.views.push(Viewpoint { name, camera });
            }
        } else {
            book.views.retain(|v| v.name != name);
        }
        let bytes = serde_json::to_vec_pretty(&book).map_err(|e| ToolError::invalid(e.to_string()))?;
        craft_anything_platform::write_atomic_private(&path, &bytes)?;
    } else if p.camera.is_some() {
        return Err(ToolError::invalid("A camera pose requires a viewpoint name."));
    }
    Ok(ToolOutput::Value(serde_json::to_value(book).map_err(|e| ToolError::invalid(e.to_string()))?))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallOptions, Caller, Engine};
    use serde_json::json;
    fn pose() -> CameraPose {
        CameraPose { target: [1., 2., 3.], yaw: 0., pitch: 0.5, distance: 20., y_up: false }
    }
    #[test]
    fn persists_across_sessions_replaces_deletes_and_rejects_stale_identity() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source");
        std::fs::create_dir(&source).unwrap();
        for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
            std::fs::write(source.join(name), bytes).unwrap();
        }
        let config = dir.path().join("config");
        let open = || {
            let mut session = crate::Session::isolated(&config);
            session.settings.deep_mode = true;
            let engine = Engine::new(session);
            engine.call("formats_scene_bundle_open", json!({"path":source.join("scene.json")}), CallOptions::new(Caller::Cli)).unwrap();
            engine
        };
        let engine = open();
        let id = engine.session().ext.get::<LoadedScene>().unwrap().scene.id.clone();
        let call = |e: &Engine, p| e.call("formats_scene_viewpoints_manage", p, CallOptions::new(Caller::Cli));
        call(&engine, json!({"scene_id":id,"name":"Overview","camera":pose()})).unwrap();
        call(&engine, json!({"scene_id":id,"name":"Overview","camera":pose()})).unwrap();
        drop(engine);
        let reopened = open();
        let result = call(&reopened, json!({"scene_id":id})).unwrap();
        let ToolOutput::Value(value) = result else { panic!("value expected") };
        assert_eq!(value["views"].as_array().unwrap().len(), 1);
        assert_eq!(value["views"][0]["camera"]["target"], json!([1., 2., 3.]));
        assert!(call(&reopened, json!({"scene_id":"../wrong","name":"x","camera":pose()})).is_err());
        assert!(call(&reopened, json!({"scene_id":id,"name":" ","camera":pose()})).is_err());
        call(&reopened, json!({"scene_id":id,"name":"Overview"})).unwrap();
        let result = call(&reopened, json!({"scene_id":id})).unwrap();
        let ToolOutput::Value(value) = result else { panic!("value expected") };
        assert!(value["views"].as_array().unwrap().is_empty());
        let path = config.join("scene-viewpoints").join(format!("{id}.json"));
        std::fs::write(&path, b"{\"version\":2,\"scene_id\":\"x\",\"views\":[]}").unwrap();
        let before = std::fs::read(&path).unwrap();
        assert!(call(&reopened, json!({"scene_id":id,"name":"Overwrite","camera":pose()})).is_err());
        assert_eq!(before, std::fs::read(path).unwrap());
    }
    #[test]
    fn hostile_camera_and_version_are_rejected() {
        for distance in [f64::NAN, -1., 0., f64::INFINITY, 1e15] {
            let mut c = pose();
            c.distance = distance;
            assert!(c.validate().is_err());
        }
        let book = Viewpoints { version: 2, scene_id: "a".repeat(64), views: vec![] };
        assert!(validate_book(&book, &book.scene_id).is_err());
    }
}
