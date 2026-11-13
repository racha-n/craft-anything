use craft_anything_engine::{
    CallOptions, Caller, CancelToken, Engine, Session,
    tools::{scene_tools::LoadedScene, scene_view_tools::CameraPose},
};
use craft_anything_engines::scene_render::animation::{Animation, Keyframe};
use serde_json::{Value, json};
use std::sync::Arc;
fn setup() -> (tempfile::TempDir, Engine, String) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    std::fs::create_dir(&source).unwrap();
    for ((name, _), bytes) in craft_anything_formats::scene_bundle::MEMBERS.iter().zip(craft_anything_testkit::scene_bundle::fixture()) {
        std::fs::write(source.join(name), bytes).unwrap();
    }
    let mut session = Session::isolated(&dir.path().join("config"));
    session.settings.deep_mode = true;
    let engine = Engine::new(session);
    call(&engine, "formats_scene_bundle_open", json!({"path":source.join("scene.json")}));
    let id = {
        let mut session = engine.session();
        let s = session.ext.get::<LoadedScene>().unwrap();
        let id = s.scene.id.clone();
        let metadata = json!({"schema_version":4,"tasks":[{"guid":"task-a","name":"=Work, \"quoted\"","display_id":"","planned_start_wallclock_scalar":100,"duration_storage_scalar":20,"status_storage_value":2,"date_storage_windows":[100,200,230,32535244799i64],"assignments":[],"direct_costs":[{"guid":"fixed","name":"Setup","amount_storage":12.5,"type_storage_value":1}]}],"resources":[],"definition_storage":{}});
        session.ext.insert(Arc::new(LoadedScene {
            scene: s.scene.clone(),
            manifest: s.manifest.clone(),
            subject: s.subject.clone(),
            task_collection_lookup: None,
            metadata: Arc::new(metadata),
        }));
        id
    };
    (dir, engine, id)
}
fn call(e: &Engine, id: &str, p: Value) -> Value {
    e.call(id, p, CallOptions::new(Caller::Cli)).unwrap().to_json()
}
fn animation(id: &str) -> Animation {
    let camera = CameraPose::default();
    Animation {
        version: 1,
        scene_id: id.into(),
        keyframes: vec![Keyframe { seconds: 0., camera: camera.clone(), focus: 100 }, Keyframe { seconds: 1., camera, focus: 230 }],
    }
}
#[test]
fn audit_does_not_promote_blank_fields_or_previews_to_native_facts() {
    let (_d, e, id) = setup();
    let a = call(&e, "formats_project_audit", json!({"scene_id":id}));
    assert_eq!(a["result"]["reader_display_ids_blank"], 1);
    assert_eq!(a["result"]["native_display_id_semantics_verified"], false);
    assert_eq!(a["result"]["appearance"]["source_opacity_conversion_verified"], false);
}
#[test]
fn reports_match_source_focus_preserve_raw_text_and_do_not_overwrite() {
    let (d, e, id) = setup();
    let output = d.path().join("report");
    let p = json!({"scene_id":id,"output_dir":output,"focus":210,"options":{"best_dates":true}});
    let r = call(&e, "formats_project_report_export", p.clone());
    assert_eq!(r["result"]["manifest"]["totals"]["direct_budget_minor"], 1250);
    assert_eq!(r["result"]["manifest"]["totals"]["resource_forecast_minor"], 0);
    let snapshot: Value = serde_json::from_slice(&std::fs::read(output.join("snapshot.json")).unwrap()).unwrap();
    assert_eq!(snapshot["focus"], 210);
    assert_eq!(snapshot["tasks"][0]["name"], "=Work, \"quoted\"");
    let csv = std::fs::read_to_string(output.join("tasks.csv")).unwrap();
    assert!(csv.contains("'=Work,"));
    let before = std::fs::read(output.join("manifest.json")).unwrap();
    assert!(e.call("formats_project_report_export", p, CallOptions::new(Caller::Cli)).is_err());
    assert_eq!(before, std::fs::read(output.join("manifest.json")).unwrap());
}
#[test]
fn animation_reload_and_unknown_versions_preserve_saved_data() {
    let (d, e, id) = setup();
    let a = animation(&id);
    call(&e, "formats_scene_animation_manage", json!({"scene_id":id,"animation":a}));
    let loaded = call(&e, "formats_scene_animation_manage", json!({"scene_id":id}));
    assert_eq!(loaded["result"]["animation"]["keyframes"].as_array().unwrap().len(), 2);
    let path = d.path().join("config/scene-animations").join(format!("{id}.json"));
    let mut corrupt = serde_json::to_value(animation(&id)).unwrap();
    corrupt["version"] = json!(2);
    std::fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
    let before = std::fs::read(&path).unwrap();
    assert!(e.call("formats_scene_animation_manage", json!({"scene_id":id,"animation":animation(&id)}), CallOptions::new(Caller::Cli)).is_err());
    assert_eq!(before, std::fs::read(path).unwrap());
}
#[test]
fn stale_identity_and_cancel_do_not_create_reports() {
    let (d, e, id) = setup();
    let output = d.path().join("report");
    assert!(e.call("formats_project_report_export", json!({"scene_id":"wrong","output_dir":output,"focus":100}), CallOptions::new(Caller::Cli)).is_err());
    let cancel = CancelToken::default();
    cancel.cancel();
    assert!(
        e.call("formats_project_report_export", json!({"scene_id":id,"output_dir":output,"focus":100}), CallOptions::new(Caller::Cli).with_cancel(cancel))
            .is_err()
    );
    assert!(!output.exists());
}
#[test]
#[ignore = "requires a native graphics adapter"]
fn lossless_video_has_all_frames_and_no_clobber() {
    let (d, e, id) = setup();
    let path = d.path().join("demo.avi");
    let p = json!({"scene_id":id,"animation":animation(&id),"output_path":path,"size":[32,32],"fps":2,"options":{"schedule":false}});
    let result = call(&e, "formats_scene_video_export", p.clone());
    assert_eq!(result["result"]["frames"], 3);
    let video = std::fs::read(&path).unwrap();
    assert_eq!(&video[0..4], b"RIFF");
    assert_eq!(u32::from_le_bytes(video[4..8].try_into().unwrap()) as usize + 8, video.len());
    let m: Value = serde_json::from_slice(&std::fs::read(path.with_extension("avi.manifest.json")).unwrap()).unwrap();
    assert_eq!(m["frames"].as_array().unwrap().len(), 3);
    assert_eq!(m["frames"][2]["focus"], 230);
    assert!(e.call("formats_scene_video_export", p, CallOptions::new(Caller::Cli)).is_err());
    assert_eq!(video, std::fs::read(path).unwrap());
}

#[test]
fn mp4_preflight_errors_do_not_create_outputs() {
    let (d, e, id) = setup();
    let path = d.path().join("demo.mp4");
    for (size, encoder) in [([33, 32], "ffmpeg"), ([32, 32], "craft-anything-missing-ffmpeg-test")] {
        let p = json!({"scene_id":id,"animation":animation(&id),"output_path":path,"size":size,"fps":2,"encoder_path":encoder});
        assert!(e.call("formats_scene_mp4_export", p, CallOptions::new(Caller::Cli)).is_err());
        assert!(!path.exists());
        assert!(!path.with_extension("mp4.manifest.json").exists());
    }
    std::fs::write(&path, b"preserve").unwrap();
    let p = json!({"scene_id":id,"animation":animation(&id),"output_path":path});
    assert!(e.call("formats_scene_mp4_export", p, CallOptions::new(Caller::Cli)).is_err());
    assert_eq!(std::fs::read(path).unwrap(), b"preserve");
}

#[test]
fn report_template_dates_selection_and_file_reuse_preserve_raw_snapshot() {
    let (d, e, id) = setup();
    let first = d.path().join("schedule-report");
    let p = json!({"scene_id":id,"output_dir":first,"focus":100,"template":{"version":1,"name":"Schedule review","tables":["tasks","phases"],"readable_dates":true}});
    let result = call(&e, "formats_project_report_export", p);
    let m = &result["result"]["manifest"];
    assert_eq!(m["version"], 3);
    assert_eq!(m["source_report_version"], 1);
    assert_eq!(m["geometry_coverage"], "loaded_scene");
    assert_eq!(m["tables"].as_array().unwrap().len(), 2);
    assert!(!first.join("costs.csv").exists());
    let csv = std::fs::read_to_string(first.join("tasks.csv")).unwrap();
    assert!(csv.contains("planned_start_display"));
    assert!(csv.contains("1970-01-01 00:01:40"));
    assert!(csv.contains("'=Work"));
    let raw: Value = serde_json::from_slice(&std::fs::read(first.join("snapshot.json")).unwrap()).unwrap();
    assert_eq!(raw["tasks"][0]["planned_start"], 100);
    assert!(raw["tasks"][0].get("planned_start_display").is_none());
    assert!(!raw["frame"]["costs"]["lines"].as_array().unwrap().is_empty());
    let second = d.path().join("reused");
    call(&e, "formats_project_report_export", json!({"scene_id":id,"output_dir":second,"focus":100,"template_path":first.join("template.json")}));
    for name in ["tasks.csv", "phases.csv", "snapshot.json", "template.json", "manifest.json"] {
        assert_eq!(std::fs::read(first.join(name)).unwrap(), std::fs::read(second.join(name)).unwrap());
    }
    let invalid = d.path().join("invalid-template.json");
    std::fs::write(&invalid, b"{\"version\":99,\"name\":\"Future\",\"tables\":[\"tasks\"],\"readable_dates\":true}").unwrap();
    let saved = std::fs::read(&invalid).unwrap();
    let out = d.path().join("invalid-output");
    assert!(
        e.call("formats_project_report_export", json!({"scene_id":id,"output_dir":out,"focus":100,"template_path":invalid}), CallOptions::new(Caller::Cli))
            .is_err()
    );
    assert!(!out.exists());
    assert_eq!(saved, std::fs::read(invalid).unwrap());
}
