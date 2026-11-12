//! Local metadata-only desktop verification, without source-application or geometry access.
#![forbid(unsafe_code)]
use craft_anything_engine::{Engine, Session, tools::scene_project_tools::LatestMetadataFrame};
use craft_anything_ui_egui::scene_viewer::SceneApp;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
fn fill(h: &mut Harness<SceneApp>, label: &str, value: &str) -> Result<(), String> {
    h.get_all_by_label(label).last().ok_or("Missing report text field")?.focus();
    h.run_steps(2);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(2);
    h.get_all_by_label(label).last().ok_or("Missing report text field")?.type_text(value);
    h.run_steps(2);
    Ok(())
}
fn export(h: &mut Harness<SceneApp>, output: &Path) -> Result<(), String> {
    fill(h, "Metadata report folder", &output.to_string_lossy())?;
    if h.get_by_label("Metadata report folder").value().as_deref() != Some(output.to_string_lossy().as_ref()) {
        return Err("Report destination was not replaced".into());
    }
    h.get_by_label("Export metadata CSV").click();
    let started = Instant::now();
    while !output.join("manifest.json").exists() {
        h.step();
        if started.elapsed() > Duration::from_secs(60) {
            return Err("Template export did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    h.run_steps(12);
    Ok(())
}
fn snapshot(path: String, output: &Path) -> Result<(), String> {
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut session = Session::isolated(&output.join("isolated-config"));
    session.settings.deep_mode = true;
    session.settings.first_run_done = true;
    let engine = Arc::new(Engine::new(session));
    let shared = engine.clone();
    eprintln!("Metadata UI: initializing hardware renderer");
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.power_preference = eframe::wgpu::PowerPreference::HighPerformance;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::DiscreteGpu)
            .or_else(|| adapters.iter().find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::IntegratedGpu))
            .cloned()
            .ok_or_else(|| "Hardware adapter required for this UI verifier".into())
    }));
    let mut h = Harness::builder().with_size(egui::vec2(1400., 900.)).wgpu_setup(eframe::egui_wgpu::WgpuSetup::CreateNew(setup)).build_eframe(move |cc| {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        craft_anything_ui_egui::CraftApp::setup_context(&cc.egui_ctx);
        SceneApp::with_engine(shared, cc.wgpu_render_state.clone(), String::new())
    });
    eprintln!("Metadata UI: opening source without a scene");
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
    eprintln!("Metadata UI: exporting reports");
    let report = output.join("metadata-csv-report");
    h.get_by_label("Metadata report folder").focus();
    h.run_steps(2);
    h.get_by_label("Metadata report folder").type_text(&report.to_string_lossy());
    h.run_steps(2);
    if h.get_by_label("Metadata report folder").value().as_deref() != Some(report.to_string_lossy().as_ref()) {
        return Err("Report folder text was not entered".into());
    }
    let start = Instant::now();
    while h.get_by_label("Export metadata CSV").accesskit_node().is_disabled() {
        h.step();
        if start.elapsed() > Duration::from_secs(60) {
            return Err("Metadata CSV control did not become ready".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    h.get_by_label("Export metadata CSV").click();
    let start = Instant::now();
    while !report.join("manifest.json").exists() {
        h.step();
        if start.elapsed() > Duration::from_secs(60) {
            return Err("Metadata CSV export did not finish".into());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    h.run_steps(12);
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report.join("manifest.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let metadata = engine.session().ext.get::<LatestMetadataFrame>().ok_or("Missing metadata frame")?;
    if manifest["version"] != 3
        || !manifest["scene_id"].is_null()
        || manifest["source_sha256"] != metadata.source_sha256
        || manifest["focus"] != metadata.frame.focus
        || manifest["geometry_coverage"] != "not_inspected"
    {
        return Err("Metadata report identity differs from displayed source/focus".into());
    }
    if engine.session().ext.get::<craft_anything_engine::tools::scene_tools::LoadedScene>().is_some() {
        return Err("CSV export loaded a 3D scene".into());
    }
    std::fs::write(
        output.join("metadata-report-verification.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"version":1,"export_via_ui":true,"no_scene_loaded":true,"source_focus_match":true,"manifest_version":3}))
            .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    h.render()?.save(output.join("metadata-4d.png")).map_err(|e| e.to_string())?;
    h.get_by_label("5D").click();
    h.run_steps(3);
    h.render()?.save(output.join("metadata-5d.png")).map_err(|e| e.to_string())?;
    let latest = engine.session().ext.get::<LatestMetadataFrame>().ok_or("Metadata result disappeared")?;
    std::fs::write(output.join("metadata-frame.json"), serde_json::to_vec_pretty(latest.frame.as_ref()).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    h.get_by_label("CSV template selection").click();
    h.run_steps(2);
    h.get_by_label("Schedule review").click();
    h.run_steps(2);
    let schedule = output.join("schedule-template-report");
    export(&mut h, &schedule)?;
    if schedule.join("costs.csv").exists() {
        return Err("Schedule template unexpectedly exported cost CSV".into());
    }
    h.render()?.save(output.join("schedule-template-ui.png")).map_err(|e| e.to_string())?;
    h.get_by_label("CSV template selection").click();
    h.run_steps(2);
    h.get_by_label("Saved template JSON").click();
    h.run_steps(2);
    fill(&mut h, "Template JSON path", &schedule.join("template.json").to_string_lossy())?;
    let reused = output.join("reused-template-report");
    export(&mut h, &reused)?;
    for name in ["tasks.csv", "phases.csv", "snapshot.json", "template.json", "manifest.json"] {
        if std::fs::read(schedule.join(name)).map_err(|e| e.to_string())? != std::fs::read(reused.join(name)).map_err(|e| e.to_string())? {
            return Err(format!("Reused template differs for {name}"));
        }
    }
    std::fs::write(output.join("template-verification.json"), serde_json::to_vec_pretty(&serde_json::json!({"version":1,"default_readable_all_export":true,"schedule_preset_via_ui":true,"saved_template_reused_via_ui":true,"all_reused_files_byte_identical":true})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    eprintln!("Metadata UI: verified metadata and CSV export");
    Ok(())
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("Supply project source and private output folder")?;
    let output = args.next().ok_or("Supply private output folder")?;
    snapshot(path, Path::new(&output))
}
