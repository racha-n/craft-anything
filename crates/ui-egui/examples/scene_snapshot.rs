//! Local GPU and interaction verification: scene_snapshot <scene.json> <output-directory>.
#![forbid(unsafe_code)]
use craft_anything_ui_egui::scene_viewer::SceneApp;
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::{
    path::Path,
    time::{Duration, Instant},
};
fn settle(h: &mut Harness<SceneApp>) -> Result<(), String> {
    let start = Instant::now();
    let mut quiet = 0;
    loop {
        h.step();
        if let Some(e) = h.state().error() {
            return Err(e.to_owned());
        }
        if h.state().ready() {
            quiet += 1;
        } else {
            quiet = 0;
        }
        if quiet >= 4 {
            return Ok(());
        }
        if start.elapsed() > Duration::from_secs(60) {
            return Err(format!("Viewer did not become ready: {}", h.state().status));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn snapshot(path: String, output: &Path) -> Result<(), String> {
    std::fs::create_dir_all(output).map_err(|e| e.to_string())?;
    let mut setup = eframe::egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.power_preference = eframe::wgpu::PowerPreference::HighPerformance;
    setup.native_adapter_selector = Some(std::sync::Arc::new(|adapters, _| {
        adapters
            .iter()
            .find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::DiscreteGpu)
            .or_else(|| adapters.iter().find(|a| a.get_info().device_type == eframe::wgpu::DeviceType::IntegratedGpu))
            .cloned()
            .ok_or_else(|| "A hardware GPU is required for this large-scene verifier".into())
    }));
    let config = output.join("isolated-config");
    let mut h = Harness::builder().with_size(egui::vec2(1400., 900.)).wgpu_setup(eframe::egui_wgpu::WgpuSetup::CreateNew(setup)).build_eframe(move |cc| {
        if let Some(g) = &cc.wgpu_render_state {
            eprintln!("Graphics adapter: {:?}", g.adapter.get_info());
        }
        egui_extras::install_image_loaders(&cc.egui_ctx);
        craft_anything_ui_egui::CraftApp::setup_context(&cc.egui_ctx);
        let mut session = craft_anything_engine::Session::isolated(&config);
        session.settings.deep_mode = true;
        SceneApp::with_engine(std::sync::Arc::new(craft_anything_engine::Engine::new(session)), cc.wgpu_render_state.clone(), path)
    });
    settle(&mut h)?;
    h.render()?.save(output.join("scene-fit.png")).map_err(|e| e.to_string())?;
    // Verify the actual sidecar controls and camera restoration with isolated settings.
    let saved = serde_json::to_value(&h.state().state.camera).map_err(|e| e.to_string())?;
    h.get_by_label("Saved viewpoints").click();
    h.run_steps(2);
    h.state_mut().state.planning.viewpoint_name = "QA Overview".into();
    h.step();
    h.get_by_label("Save current view").click();
    settle(&mut h)?;
    h.state_mut().state.camera.orbit([50., 20.]);
    h.state_mut().changed();
    settle(&mut h)?;
    h.get_by_label("Saved viewpoints").click();
    h.run_steps(2);
    h.get_by_label("QA Overview").click();
    settle(&mut h)?;
    if serde_json::to_value(&h.state().state.camera).map_err(|e| e.to_string())? != saved {
        return Err("Saved viewpoint did not restore the exact camera".into());
    }
    h.render()?.save(output.join("viewpoint-restored.png")).map_err(|e| e.to_string())?;
    h.get_by_label("View cube surface").click();
    settle(&mut h)?;
    if serde_json::to_value(&h.state().state.camera).map_err(|e| e.to_string())? == saved {
        return Err("Orientation cube did not change camera direction".into());
    }
    h.render()?.save(output.join("view-cube.png")).map_err(|e| e.to_string())?;
    h.state_mut().state.camera = serde_json::from_value(saved).map_err(|e| e.to_string())?;
    h.state_mut().changed();
    settle(&mut h)?;

    let model = h.state().model.clone().ok_or("No scene loaded")?;
    let scene = &model.scene;
    let appearances=scene.instances.iter().enumerate().filter_map(|(i,instance)|scene.object_colors.get(&i).map(|rgb|serde_json::json!({"instance":i,"entity_guid":instance.entity_guid,"object_rgb_candidate":rgb,"source_diffuse_candidate":scene.source_colors.get(&i),"world_bounds":instance.bounds}))).collect::<Vec<_>>();
    std::fs::write(
        output.join("appearance-witnesses.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"version":1,"source_sha256":scene.source_sha256,"instances":appearances,"native_material_parity_verified":false}),
        )
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    if h.state().state.planning.enabled
        && let Some(task) = model.metadata.get("tasks").and_then(|v| v.as_array()).and_then(|v| v.first())
        && let Some(guid) = task.get("guid").and_then(|v| v.as_str())
    {
        h.get_by_label(&format!("Task {guid}")).click();
        settle(&mut h)?;
        if h.state().state.planning.selected_task.as_deref() != Some(guid) {
            return Err("Task/Gantt selection did not reach the inspector".into());
        }
        h.render()?.save(output.join("planning-task-properties.png")).map_err(|e| e.to_string())?;
        h.state_mut().state.planning.selected_task = None;
    }
    if model.metadata.get("tasks").is_some() {
        h.get_by_label("Collapse all").click();
        h.run_steps(2);
        if model.metadata["tasks"].as_array().is_some_and(|tasks| tasks.iter().any(|t| t["parent_guid"].is_string()))
            && h.state().state.planning.collapsed_tasks.is_empty()
        {
            return Err("Hierarchy collapse did not change outline state".into());
        }
        h.render()?.save(output.join("gantt-collapsed.png")).map_err(|e| e.to_string())?;
        h.get_by_label("Expand all").click();
        h.run_steps(2);
        if !h.state().state.planning.collapsed_tasks.is_empty() {
            return Err("Expand all retained collapsed tasks".into());
        }
        h.get_by_label("Zoom time +").click();
        h.run_steps(2);
        if h.state().state.planning.gantt_zoom != 2. {
            return Err("Gantt time zoom did not change".into());
        }
        h.get_by_label("Later").click();
        h.run_steps(2);
        h.render()?.save(output.join("gantt-zoomed.png")).map_err(|e| e.to_string())?;
        h.get_by_label("Fit timeline").click();
        h.run_steps(2);
    }
    if model.metadata.get("tasks").is_some() {
        let original_camera = h.state().state.camera.clone();
        let original_focus = h.state().state.project.focus;
        h.state_mut().state.planning.tab = craft_anything_ui_egui::scene_viewer::planning::Tab::Review;
        h.step();
        h.get_by_label("Camera animation").click();
        h.run_steps(3);
        h.get_by_label("Capture keyframe").click();
        h.run_steps(2);
        let end = h.state().project_frame.as_ref().and_then(|f| f.frame.timeline.end).unwrap_or(original_focus + 86400);
        h.state_mut().state.project.focus = end.saturating_add(86400);
        h.state_mut().state.camera.orbit([70., 10.]);
        h.state_mut().changed();
        settle(&mut h)?;
        h.state_mut().state.presentation.next_seconds = 1.;
        h.step();
        h.get_by_label("Capture keyframe").click();
        h.run_steps(2);
        h.get_by_label("Save animation").click();
        settle(&mut h)?;
        let captured = h.state().state.presentation.keyframes.clone();
        h.state_mut().state.presentation.keyframes.clear();
        h.step();
        h.get_by_label("Load animation").click();
        settle(&mut h)?;
        if h.state().state.presentation.keyframes.len() != 2 {
            return Err("Saved camera path did not reload".into());
        }
        h.get_by_label("Play path").click();
        settle(&mut h)?;
        let last = captured.last().ok_or("Missing last camera keyframe")?;
        if h.state().state.camera != last.camera || h.state().state.project.focus != last.focus {
            return Err("Camera preview did not reach the last camera/focus keyframe".into());
        }
        h.render()?.save(output.join("camera-path-preview.png")).map_err(|e| e.to_string())?;
        h.state_mut().state.presentation.video_path = output.join("project-preview.avi").to_string_lossy().into_owned();
        h.state_mut().state.presentation.fps = 2;
        h.state_mut().state.presentation.width = 320;
        h.state_mut().state.presentation.height = 180;
        h.step();
        h.get_by_label("Export AVI").click();
        settle(&mut h)?;
        let video_path = output.join("project-preview.avi");
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(video_path.with_extension("avi.manifest.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if manifest["frame_count"] != 3 {
            return Err("Video did not contain all planned endpoint frames".into());
        }
        let graphics = h.state().graphics_state().ok_or("Missing preview graphics state")?;
        let project = h.state().project_snapshot().ok_or("Missing project snapshot")?;
        let options: craft_anything_engine::tools::scene_presentation_tools::RenderOptions =
            serde_json::from_value(manifest["options"].clone()).map_err(|e| e.to_string())?;
        let (frame_index, visible) = manifest["frames"]
            .as_array()
            .ok_or("Missing frame manifest")?
            .iter()
            .enumerate()
            .map(|(i, f)| (i, f["draw"]["visible"].as_u64().unwrap_or(0)))
            .max_by_key(|(_, n)| *n)
            .ok_or("No exported frames")?;
        if visible == 0 {
            return Err("The exported demonstration contains no visible geometry".into());
        }
        let sampled = h.state().camera_path().ok_or("Missing camera path")?.sample((frame_index as f64 / 2.).min(1.))?;
        let first = &sampled;
        let cancel = craft_anything_engine::CancelToken::default();
        let data = craft_anything_engine::tools::scene_presentation_tools::render_snapshot(
            &cancel,
            scene,
            &project.model,
            first.camera.clone(),
            first.focus,
            [320, 180],
            &options,
        )
        .map_err(|e| e.to_string())?;
        let mut preview = craft_anything_engines::scene_render::gpu::Gpu::new(&graphics.device, &graphics.queue, scene, &cancel)?;
        preview.render(scene, &data, &cancel)?;
        let reference = preview.rgba(&cancel)?;
        let bytes = std::fs::read(&video_path).map_err(|e| e.to_string())?;
        let stride = (320u32 * 3).div_ceil(4) * 4;
        let mut decoded = Vec::new();
        for row in (0..180usize).rev() {
            let start = 232 + frame_index * (8 + stride as usize * 180) + row * stride as usize;
            let row = bytes.get(start..start + 320 * 3).ok_or("Incomplete AVI frame")?;
            for [b, g, r] in row.as_chunks::<3>().0 {
                decoded.extend([*r, *g, *b, 255]);
            }
        }
        if decoded != reference {
            return Err(format!(
                "AVI geometry frame differs from the preview graphics device: {} byte mismatches",
                decoded.iter().zip(&reference).filter(|(a, b)| a != b).count()
            ));
        }
        h.state_mut().state.presentation.show = false;
        h.state_mut().state.presentation.show_reports = true;
        h.state_mut().state.presentation.report_dir = output.join("csv-report").to_string_lossy().into_owned();
        h.step();
        h.get_by_label("Export CSV reports").click();
        settle(&mut h)?;
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output.join("csv-report/manifest.json")).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if report["focus"].as_i64() != Some(h.state().state.project.focus) || report["source_sha256"] != scene.source_sha256 {
            return Err("Report identity or focus did not match the current project".into());
        }
        h.render()?.save(output.join("csv-report-ui.png")).map_err(|e| e.to_string())?;
        h.state_mut().state.presentation.show_reports = false;
        h.state_mut().state.presentation.show_audit = true;
        h.step();
        h.get_by_label("Refresh source audit").click();
        settle(&mut h)?;
        let audit = h.state().last_presentation_output.as_ref().and_then(|v| v.get("result")).ok_or("Missing source audit")?;
        std::fs::write(output.join("source-audit.json"), serde_json::to_vec_pretty(audit).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        if audit["native_display_id_semantics_verified"] != false || audit["appearance"]["native_material_parity_verified"] != false {
            return Err("Audit promoted unresolved native semantics".into());
        }
        h.render()?.save(output.join("source-audit-ui.png")).map_err(|e| e.to_string())?;
        std::fs::write(output.join("presentation-verification.json"),serde_json::to_vec_pretty(&serde_json::json!({"version":1,"camera_keyframes":2,"saved_path_reload":true,"preview_endpoint_camera_focus_match":true,"video_frames":3,"avi_pixels_equal_preview_device":true,"report_source_focus_match":true,"audit_keeps_native_unknowns":true})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        h.state_mut().state.presentation.show_audit = false;
        h.state_mut().state.project.mode = craft_anything_ui_egui::scene_viewer::project_panel::Mode::Model;
        h.state_mut().state.camera = original_camera;
        h.state_mut().state.project.focus = original_focus;
        h.state_mut().state.planning.tab = craft_anything_ui_egui::scene_viewer::planning::Tab::Plan;
        h.state_mut().changed();
        settle(&mut h)?;
    }
    let initial_draw = h.state().draw_stats().map(|(s, ms)| serde_json::json!({"draw":s,"elapsed_ms":ms}));
    if h.state().state.planning.enabled {
        h.state_mut().state.planning.tab = craft_anything_ui_egui::scene_viewer::planning::Tab::File;
        h.step();
        if !h.get_by_label("Save project").accesskit_node().is_disabled() {
            return Err("A future editing command was enabled".into());
        }
        h.render()?.save(output.join("planning-file-future.png")).map_err(|e| e.to_string())?;
        h.state_mut().state.planning.tab = craft_anything_ui_egui::scene_viewer::planning::Tab::Plan;
        h.step();
    }
    // Aim through a real triangle centroid rather than assuming screen centre contains geometry.
    let instance = scene.instances.iter().find(|i| scene.meshes.get(i.mesh).is_some_and(|m| m.triangle_count > 0)).ok_or("No drawable instance")?;
    let mesh = scene.meshes.get(instance.mesh).ok_or("Missing mesh")?;
    let mut local = [0.; 3];
    for corner in 0..3 {
        let at = (mesh.index_start + corner) * 4;
        let index = u32::from_le_bytes(scene.gpu_indices.get(at..at + 4).ok_or("Missing triangle")?.try_into().map_err(|_| "Bad triangle")?) as usize;
        let p = scene.positions.get(mesh.vertex_start + index).ok_or("Missing position")?;
        for (v, c) in local.iter_mut().zip(p) {
            *v += c / 3.;
        }
    }
    h.state_mut().state.camera.target = craft_anything_formats::scene_bundle::math::transform(&instance.matrix, local);
    h.state_mut().changed();
    settle(&mut h)?;
    h.state_mut().pick([0., 0.], 1.5);
    settle(&mut h)?;
    if h.state().state.selected_instance.is_none() {
        return Err(format!("Surface picking did not select geometry: {}", h.state().status));
    }
    h.render()?.save(output.join("scene-selected.png")).map_err(|e| e.to_string())?;
    h.get_by_label("Hide").click();
    h.step();
    settle(&mut h)?;
    if h.state().state.visibility.hidden_instances.len() != 1 {
        return Err("Hide action did not affect the selected instance".into());
    }
    h.get_by_label("Show all").click();
    h.step();
    settle(&mut h)?;
    if !h.state().state.visibility.hidden_instances.is_empty() {
        return Err("Show all did not restore visibility".into());
    }
    h.get_by_label("Isolate selection").click();
    h.step();
    settle(&mut h)?;
    h.get_by_label("Fit selection").click();
    h.step();
    settle(&mut h)?;
    h.render()?.save(output.join("scene-isolated.png")).map_err(|e| e.to_string())?;
    h.state_mut().state.camera.orbit([70., 20.]);
    h.state_mut().state.camera.pan([20., 5.], 700.);
    h.state_mut().state.camera.zoom(100.);
    h.state_mut().changed();
    settle(&mut h)?;
    h.render()?.save(output.join("scene-orbit.png")).map_err(|e| e.to_string())?;
    h.get_by_label("Show all").click();
    h.step();
    settle(&mut h)?;
    if let Some((id, r)) = scene.resources.iter().enumerate().find(|(id, _)| scene.resource_instance_counts.get(*id).is_some_and(|n| *n > 0)) {
        h.state_mut().state.search = r.name.clone();
        h.step();
        let label = format!("{} ({})", r.name.chars().take(120).collect::<String>(), scene.resource_instance_counts.get(id).copied().unwrap_or(0));
        h.get_by_label(&label).click();
        h.step();
        settle(&mut h)?;
        if h.state().state.selected_resource != Some(id) {
            return Err("Resource tree did not select the resource".into());
        }
        h.get_by_label("Hide").click();
        h.step();
        settle(&mut h)?;
        if !h.state().state.visibility.hidden_resources.contains(&id) {
            return Err("Hide resource did not apply its filter".into());
        }
        h.get_by_label("Show all").click();
        h.step();
        settle(&mut h)?;
        h.get_by_label("Isolate selection").click();
        h.step();
        settle(&mut h)?;
        h.get_by_label("Fit selection").click();
        h.step();
        settle(&mut h)?;
        h.render()?.save(output.join("scene-resource.png")).map_err(|e| e.to_string())?;
    }
    h.state_mut().state.visibility = Default::default();
    h.state_mut().state.selected_instance = None;
    h.state_mut().state.selected_resource = None;
    h.state_mut().state.camera.target = instance.bounds.center();
    h.state_mut().state.camera.distance = (instance.bounds.radius() * 4.).max(0.25);
    h.state_mut().changed();
    settle(&mut h)?;
    let focused_draw = h.state().draw_stats().map(|(s, ms)| serde_json::json!({"draw":s,"elapsed_ms":ms}));
    if scene.instances.len() > 100 && h.state().draw_stats().is_none_or(|(s, _)| s.frustum_culled == 0) {
        return Err("Focused large-scene view did not cull any instance".into());
    }
    h.render()?.save(output.join("scene-frustum.png")).map_err(|e| e.to_string())?;
    if !scene.mesh_textures.is_empty() {
        h.state_mut().state.camera.fit(scene.bounds);
        h.state_mut().state.project.object_colors = false;
        h.state_mut().state.project.texture_preview = true;
        h.state_mut().changed();
        settle(&mut h)?;
        h.render()?.save(output.join("scene-textures.png")).map_err(|e| e.to_string())?;
        h.state_mut().state.project.texture_preview = false;
    }
    let mut project_draws = Vec::new();
    if model.metadata.get("tasks").and_then(|v| v.as_array()).is_some_and(|v| !v.is_empty()) {
        use craft_anything_ui_egui::scene_viewer::project_panel::Mode;
        h.state_mut().state.visibility = Default::default();
        h.state_mut().state.search.clear();
        h.state_mut().state.camera.fit(scene.bounds);
        h.state_mut().state.project.mode = Mode::Schedule;
        h.state_mut().state.project.interval_preview = false;
        h.state_mut().changed();
        settle(&mut h)?;
        let timeline = &h.state().project_frame.as_ref().ok_or("Project frame missing")?.frame.timeline;
        let (start, end) = timeline.start.zip(timeline.end).ok_or("Project schedule has no resolved bounds")?;
        let mut phase_states = Vec::new();
        for (label, focus) in [("before", start.saturating_sub(86400)), ("middle", start + (end - start) / 2), ("after", end.saturating_add(86400))] {
            h.state_mut().state.project.focus = focus;
            h.state_mut().changed();
            settle(&mut h)?;
            let frame = &h.state().project_frame.as_ref().ok_or("Project frame missing")?.frame;
            if let Some((draw, ms)) = h.state().draw_stats() {
                project_draws.push(serde_json::json!({"label":label,"focus":focus,"draw":draw,"elapsed_ms":ms}));
            }
            phase_states.push(serde_json::to_value(&frame.resource_phases).map_err(|e| e.to_string())?);
            std::fs::write(output.join(format!("project-{label}.json")), serde_json::to_vec_pretty(frame).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            h.render()?.save(output.join(format!("project-4d-{label}.png"))).map_err(|e| e.to_string())?;
        }
        if phase_states.first() == phase_states.last() {
            return Err("Moving the focus did not change any resource appearance state".into());
        }
        h.state_mut().state.project.mode = Mode::Cost;
        h.state_mut().changed();
        settle(&mut h)?;
        h.render()?.save(output.join("project-5d.png")).map_err(|e| e.to_string())?;
        h.state_mut().state.project.mode = Mode::Model;
        h.state_mut().state.project.source_colors = true;
        h.state_mut().state.project.object_colors = false;
        h.state_mut().changed();
        settle(&mut h)?;
        h.render()?.save(output.join("project-source-diffuse.png")).map_err(|e| e.to_string())?;
    }
    let report = serde_json::json!({"schema_version":1,"scene_id":scene.id,"instances":scene.instances.len(),"definitions":scene.meshes.len(),"vertices":scene.positions.len(),"triangles_in_definitions":scene.triangles,"resources":scene.resources.len(),"initial_draw":initial_draw,"focused_draw":focused_draw,"source_diffuse_candidates":scene.source_colors.len(),"selected_instance":h.state().state.selected_instance,"isolate_instances":h.state().state.visibility.isolate_instances,"checks":["source or bundle load","GPU render","saved viewpoint UI persistence and restoration","orientation cube click","Gantt collapse expand and time navigation","ray surface pick","Hide UI action","Show all UI action","Isolate UI action","Fit selection UI action","camera orbit pan zoom","Resource tree selection and visibility","focused full-scene frustum culling","project schedule and costs when source tasks are present"],"units_verified":false,"axes_verified":false,"materials_verified":false,"native_visual_parity_verified":false});
    let mut report = report;
    report["object_colours"] = serde_json::json!({"instances":scene.object_colors.len(),"policy":craft_anything_formats::scene_bundle::OBJECT_COLOR_POLICY});
    report["project_draws"] = serde_json::json!(project_draws);
    std::fs::write(output.join("verification.json"), serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    println!("Verified viewport; snapshots saved in {}", output.display());
    Ok(())
}
fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    let Some((path, output)) = args.get(1).zip(args.get(2)) else {
        eprintln!("Usage: scene_snapshot <scene.json> <output-directory>");
        std::process::exit(2);
    };
    if let Err(e) = snapshot(path.clone(), Path::new(output)) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
