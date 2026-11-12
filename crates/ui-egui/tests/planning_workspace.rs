use craft_anything_engine::{Engine, testing};
use craft_anything_ui_egui::{
    CraftApp,
    scene_viewer::{SceneApp, planning},
};
use egui_kittest::{
    Harness,
    kittest::{NodeT, Queryable},
};
use std::sync::Arc;

#[test]
fn planning_ribbon_and_compact_switch_work_without_a_graphics_adapter() {
    let mut h = Harness::builder().with_size(egui::vec2(1400., 900.)).build_eframe(|cc| {
        CraftApp::setup_context(&cc.egui_ctx);
        SceneApp::with_engine(Arc::new(Engine::new(testing::session())), None, String::new())
    });
    h.run_steps(3);
    assert!(h.state().state.planning.enabled);
    h.get_by_label("Plan");
    h.get_by_label("File").click();
    h.run_steps(2);
    assert!(h.get_by_label("Save project").accesskit_node().is_disabled());
    assert!(h.get_by_label("New project").accesskit_node().is_disabled());
    h.get_by_label("4D Review").click();
    h.run_steps(2);
    assert_eq!(h.state().state.planning.tab, planning::Tab::Review);
    h.get_by_label("Windows").click();
    h.run_steps(2);
    h.get_by_label("Compact layout").click();
    h.run_steps(2);
    assert!(!h.state().state.planning.enabled);
    h.get_by_label("Planning layout").click();
    h.run_steps(2);
    assert!(h.state().state.planning.enabled);
}

#[test]
fn planning_workspace_remains_drawable_in_a_narrow_window() {
    let mut h = Harness::builder().with_size(egui::vec2(800., 650.)).build_eframe(|cc| {
        CraftApp::setup_context(&cc.egui_ctx);
        SceneApp::with_engine(Arc::new(Engine::new(testing::session())), None, String::new())
    });
    h.run_steps(4);
    let viewport = h.state().viewport_rect().expect("3D work surface");
    assert!(viewport.width() > 200. && viewport.height() > 100., "{viewport:?}");
    assert!(viewport.right() <= 810., "viewport overflows the narrow window: {viewport:?}");
}
#[test]
fn orientation_cube_click_changes_only_camera_direction() {
    let mut h = Harness::builder().with_size(egui::vec2(1400., 900.)).build_eframe(|cc| {
        CraftApp::setup_context(&cc.egui_ctx);
        SceneApp::with_engine(Arc::new(Engine::new(testing::session())), None, String::new())
    });
    h.run_steps(3);
    let before = h.state().state.camera.clone();
    h.get_by_label("View cube surface").click();
    h.run_steps(3);
    let after = &h.state().state.camera;
    assert!((before.yaw - after.yaw).abs() > 0.01 || (before.pitch - after.pitch).abs() > 0.01);
    assert_eq!(before.target, after.target);
    assert_eq!(before.distance, after.distance);
    assert!(h.state().state.selected_instance.is_none());
}
