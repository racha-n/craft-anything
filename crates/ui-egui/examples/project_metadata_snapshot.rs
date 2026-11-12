//! Local metadata-only desktop verification, without source-application or geometry access.
#![forbid(unsafe_code)]
use craft_anything_engine::{Engine, Session, tools::scene_project_tools::LatestMetadataFrame};
use craft_anything_ui_egui::scene_viewer::SceneApp;
use egui_kittest::{Harness, kittest::Queryable};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
fn snapshot(path: String, output: &Path) -> Result<(), String> {
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut session = Session::isolated(&output.join("isolated-config"));
    session.settings.deep_mode = true;
    session.settings.first_run_done = true;
    let engine = Arc::new(Engine::new(session));
    let shared = engine.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1400., 900.)).wgpu().build_eframe(move |cc| {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        craft_anything_ui_egui::CraftApp::setup_context(&cc.egui_ctx);
        SceneApp::with_engine(shared, cc.wgpu_render_state.clone(), String::new())
    });
    h.state_mut().path = path;
    h.state_mut().state.planning.tab = craft_anything_ui_egui::scene_viewer::planning::Tab::File;
    h.step();
    h.get_by_label("Schedule only").click();
    let start = Instant::now();
    let mut ready = 0;
    loop {
        h.step();
        if engine.session().ext.get::<LatestMetadataFrame>().is_some() {
            ready += 1;
        } else {
            ready = 0;
        }
        if ready >= 8 {
            break;
        }
        if start.elapsed() > Duration::from_secs(60) {
            return Err("Metadata-only desktop did not produce a frame".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    if h.state().model.is_some() {
        return Err("Metadata workflow unexpectedly loaded geometry".into());
    }
    h.render()?.save(output.join("metadata-4d.png")).map_err(|e| e.to_string())?;
    h.get_by_label("5D").click();
    h.run_steps(3);
    h.render()?.save(output.join("metadata-5d.png")).map_err(|e| e.to_string())?;
    let latest = engine.session().ext.get::<LatestMetadataFrame>().ok_or("Metadata result disappeared")?;
    std::fs::write(output.join("metadata-frame.json"), serde_json::to_vec_pretty(latest.frame.as_ref()).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("Supply project source and private output folder")?;
    let output = args.next().ok_or("Supply private output folder")?;
    snapshot(path, Path::new(&output))
}
