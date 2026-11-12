//! Private verification helper: admitted-source audit, candidate witnesses and a headless video.
#![forbid(unsafe_code)]
use craft_anything_engine::{CallOptions, Caller, Engine, Session, tools::scene_tools::LoadedScene};
use craft_anything_engines::scene_render::{
    animation::{Animation, Keyframe},
    camera::Camera,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let source = args.first().ok_or("Supply source project and new output folder")?;
    let output = std::path::PathBuf::from(args.get(1).ok_or("Supply new output folder")?);
    std::fs::create_dir(&output)?;
    let mut session = Session::isolated(&output.join("config"));
    session.settings.deep_mode = true;
    let engine = Engine::new(session);
    engine.call("formats_project_open", json!({"path":source}), CallOptions::new(Caller::Cli))?;
    let loaded = engine.session().ext.get::<LoadedScene>().ok_or("No loaded scene")?;
    let audit = engine.call("formats_project_audit", json!({"scene_id":loaded.scene.id}), CallOptions::new(Caller::Cli))?.to_json();
    std::fs::write(output.join("audit.json"), serde_json::to_vec_pretty(&audit)?)?;
    let rows = loaded
        .scene
        .instances
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            loaded.scene.object_colors.get(&i).map(|rgb| json!({"instance":i,"entity_guid":s.entity_guid,"world_bounds":s.bounds,"object_rgb_candidate":rgb}))
        })
        .collect::<Vec<_>>();
    std::fs::write(
        output.join("appearance.json"),
        serde_json::to_vec_pretty(&json!({"version":1,"source_sha256":loaded.scene.source_sha256,"instances":rows}))?,
    )?;
    let texture = args.iter().any(|v| v == "--texture-top");
    let mut camera = Camera::default();
    camera.fit(loaded.scene.bounds);
    if texture {
        camera.pitch = std::f64::consts::FRAC_PI_2;
        camera.yaw = -std::f64::consts::FRAC_PI_2;
    }
    let animation = Animation {
        version: 1,
        scene_id: loaded.scene.id.clone(),
        keyframes: vec![Keyframe { seconds: 0., camera: camera.clone(), focus: 0 }, Keyframe { seconds: 1., camera, focus: 0 }],
    };
    let result=engine.call("formats_scene_video_export",json!({"scene_id":loaded.scene.id,"animation":animation,"output_path":output.join("inspection.avi"),"size":[256,256],"fps":1,"options":{"schedule":false,"texture_preview":texture,"flip_texture_v":args.iter().any(|v|v=="--flip-v"),"object_colors":!texture}}),CallOptions::new(Caller::Cli))?.to_json();
    std::fs::write(output.join("video-result.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("Source audit and headless scene video complete");
    Ok(())
}
