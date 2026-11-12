//! Camera navigation and scene-bound sidecar client. File work runs through the engine.
use super::{Done, SceneApp, camera::Camera};
use craft_anything_engine::{
    CallOptions, Caller,
    tools::scene_view_tools::{CameraPose, Viewpoints},
};
use craft_anything_formats::scene_bundle::math;
use egui::{Align2, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use serde_json::json;
impl From<&Camera> for CameraPose {
    fn from(c: &Camera) -> Self {
        Self { target: c.target, yaw: c.yaw, pitch: c.pitch, distance: c.distance, y_up: c.y_up }
    }
}
impl From<CameraPose> for Camera {
    fn from(c: CameraPose) -> Self {
        Self { target: c.target, yaw: c.yaw, pitch: c.pitch, distance: c.distance, y_up: c.y_up }
    }
}
impl SceneApp {
    pub(super) fn viewpoints_job(&mut self, name: Option<String>, camera: Option<CameraPose>) {
        let Some(model) = &self.model else { return };
        let scene_id = model.scene.id.clone();
        let engine = self.engine.clone();
        self.viewpoints_scene = Some(scene_id.clone());
        self.jobs.spawn("Saved viewpoints", true, move |j| {
            let result = engine
                .call(
                    "formats_scene_viewpoints_manage",
                    json!({"scene_id":scene_id,"name":name,"camera":camera}),
                    CallOptions::new(Caller::Ui).with_cancel(j.cancel.clone()),
                )
                .map_err(|e| e.for_people())?;
            let craft_anything_engine::ToolOutput::Value(value) = result else { return Err("Expected saved viewpoint data".into()) };
            let book: Viewpoints = serde_json::from_value(value).map_err(|e| e.to_string())?;
            Ok(Done::Viewpoints(book))
        });
    }
    pub(super) fn viewpoints_menu(&mut self, ui: &mut Ui) {
        ui.menu_button("Saved viewpoints", |ui| {
            ui.label("Camera poses saved separately from the source project");
            ui.add(egui::TextEdit::singleline(&mut self.state.planning.viewpoint_name).hint_text("Viewpoint name").char_limit(80));
            if ui
                .add_enabled(
                    self.model.is_some() && !self.jobs.busy() && !self.state.planning.viewpoint_name.trim().is_empty(),
                    egui::Button::new("Save current view"),
                )
                .clicked()
            {
                self.viewpoints_job(Some(self.state.planning.viewpoint_name.trim().to_owned()), Some(CameraPose::from(&self.state.camera)));
            }
            ui.separator();
            if self.viewpoints.is_empty() {
                ui.label("No saved views for this model.");
            }
            egui::ScrollArea::vertical().id_salt("saved_viewpoint_list").max_height(260.).show(ui, |ui| {
                for view in self.viewpoints.clone() {
                    ui.horizontal(|ui| {
                        if ui.button(&view.name).on_hover_text("Restore this camera pose").clicked() {
                            self.state.camera = view.camera.clone().into();
                            self.changed();
                            ui.close();
                        }
                        if ui.add_enabled(!self.jobs.busy(), egui::Button::new("Delete")).on_hover_text(format!("Delete saved view {}", view.name)).clicked() {
                            self.viewpoints_job(Some(view.name.clone()), None);
                        }
                    });
                }
            });
            if ui.add_enabled(self.model.is_some() && !self.jobs.busy(), egui::Button::new("Refresh saved views")).clicked() {
                self.viewpoints_job(None, None);
            }
            ui.small("Saving an existing name replaces its camera pose. Up to 64 views per scene.");
        });
    }
    pub(super) fn view_cube(&mut self, ui: &mut Ui, viewport: Rect) -> Option<Rect> {
        if !self.state.planning.show_cube || viewport.width() < 180. || viewport.height() < 130. {
            return None;
        }
        let rect = Rect::from_min_size(viewport.right_top() + egui::vec2(-126., 8.), egui::vec2(118., 118.));
        let mut child = ui.new_child(egui::UiBuilder::new().id_salt("scene_orientation_cube").max_rect(rect));
        let p = crate::theme::PlanningTokens::default();
        child.painter().rect_filled(rect, 0., p.surface.card);
        let center = rect.center() - egui::vec2(0., 9.);
        let (_, forward, right, up) = self.state.camera.basis();
        let eye = math::scale(forward, -1.);
        let faces = [([1., 0., 0.], "+X"), ([-1., 0., 0.], "-X"), ([0., 1., 0.], "+Y"), ([0., -1., 0.], "-Y"), ([0., 0., 1.], "+Z"), ([0., 0., -1.], "-Z")];
        let cube_response = crate::widgets::named(child.interact(rect, child.id().with("surface"), Sense::click()), "View cube surface");
        let mut chosen = None;
        for (normal, label) in faces {
            if math::dot(normal, eye) <= 0.001 {
                continue;
            }
            let tangent = if normal[0] != 0. { [0., 1., 0.] } else { [1., 0., 0.] };
            let bitangent = math::cross(normal, tangent);
            let points = [(-1., -1.), (1., -1.), (1., 1.), (-1., 1.)].map(|(a, b)| {
                let v = math::add(normal, math::add(math::scale(tangent, a), math::scale(bitangent, b)));
                center + egui::vec2(math::dot(v, right) as f32, -math::dot(v, up) as f32) * 24.
            });
            let inside = child.input(|i| i.pointer.hover_pos().is_some_and(|p| contains(&points, p)));
            child.painter().add(egui::Shape::convex_polygon(
                points.to_vec(),
                if inside { p.selected_row } else { p.surface.chrome },
                Stroke::new(1., p.surface.field_border),
            ));
            let mid = center + egui::vec2(math::dot(normal, right) as f32, -math::dot(normal, up) as f32) * 24.;
            child.painter().text(mid, Align2::CENTER_CENTER, label, FontId::proportional(crate::theme::CAPTION), p.surface.text);
            if cube_response.clicked() && inside {
                chosen = Some(normal);
            }
        }
        let iso = Rect::from_min_size(rect.left_bottom() + egui::vec2(3., -23.), Vec2::new(112., 20.));
        if child.put(iso, egui::Button::new("Isometric")).clicked() {
            self.state.camera.yaw = -0.8;
            self.state.camera.pitch = 0.55;
            self.changed();
        }
        if let Some(normal) = chosen {
            self.state.camera.orient(normal);
            self.changed();
        }
        Some(rect)
    }
}
fn contains(points: &[Pos2; 4], p: Pos2) -> bool {
    let mut positive = false;
    let mut negative = false;
    for (a, b) in points.iter().zip(points.iter().cycle().skip(1)).take(4) {
        let cross = (*b - *a).rot90().dot(p - *a);
        positive |= cross > 0.01;
        negative |= cross < -0.01;
    }
    !(positive && negative)
}

impl SceneApp {
    pub(super) fn planning_selection_links(&mut self, ui: &mut Ui) {
        let Some(project) = self.project_model.as_ref().map(|p| p.model.clone()) else { return };
        let Some(loaded) = self.model.clone() else { return };
        ui.separator();
        ui.strong("Selection links");
        if self.current_selection_key() != self.selection_link_key || self.selection_links.scene_id != loaded.scene.id {
            ui.small("Reading assignment links...");
            return;
        }
        if self.state.planning.selected_task.is_some() {
            let resources = self.selection_links.resource_guids.clone();
            ui.small(format!("{} directly assigned resources", resources.len()));
            for guid in resources.iter().take(64) {
                let found = loaded.scene.resources.iter().enumerate().find(|(_, r)| &r.guid == guid);
                if let Some((index, resource)) = found {
                    if ui.button(format!("Resource: {}", resource.name)).on_hover_text(guid).clicked() {
                        self.state.selected_resource = Some(index);
                        self.state.selected_instance = None;
                        self.state.planning.selected_task = None;
                        self.changed();
                    }
                } else {
                    ui.small(format!("No geometry: {guid}"));
                }
            }
        } else {
            let resource_ids = if let Some(id) = self.state.selected_resource {
                std::collections::BTreeSet::from([id])
            } else {
                self.state
                    .selected_instance
                    .and_then(|id| loaded.scene.instances.get(id))
                    .map(|i| i.resources.iter().copied().collect::<std::collections::BTreeSet<_>>())
                    .unwrap_or_default()
            };
            let tasks = self.selection_links.task_guids.clone();
            ui.small(format!("{} directly linked tasks", tasks.len()));
            for guid in tasks.iter().take(64) {
                if let Some(task) = project.tasks.iter().find(|t| &t.guid == guid)
                    && ui.button(format!("Task: {}", task.name)).on_hover_text(guid).clicked()
                {
                    self.state.planning.selected_task = Some(guid.clone());
                    self.state.planning.task_filter = guid.clone();
                    self.state.planning.show_schedule = true;
                    self.state.selected_instance = None;
                    self.changed();
                }
            }
            if self.state.selected_instance.is_some() {
                for id in resource_ids.iter().take(64) {
                    if let Some(resource) = loaded.scene.resources.get(*id)
                        && ui.button(format!("Resource: {}", resource.name)).on_hover_text(&resource.guid).clicked()
                    {
                        self.state.selected_resource = Some(*id);
                        self.state.selected_instance = None;
                        self.changed();
                    }
                }
            }
        }
        if let Some(bounds) = self.selection_bounds()
            && ui.button("Fit linked selection").clicked()
        {
            self.state.camera.fit(bounds);
            self.changed();
        }
        ui.small("Only direct source assignments are linked. Up to 64 links are displayed.");
    }
}

impl SceneApp {
    pub(super) fn current_selection_key(&self) -> Option<super::SelectionKey> {
        let loaded = self.model.as_ref()?;
        let resources = if self.state.planning.selected_task.is_some() {
            Vec::new()
        } else if let Some(id) = self.state.selected_resource {
            vec![id]
        } else {
            self.state.selected_instance.and_then(|id| loaded.scene.instances.get(id)).map(|i| i.resources.clone()).unwrap_or_default()
        };
        Some((loaded.scene.id.clone(), self.state.planning.selected_task.clone(), resources))
    }
    pub(super) fn update_selection_links(&mut self) {
        if self.jobs.busy() || self.project_model.is_none() {
            return;
        }
        let Some(key) = self.current_selection_key() else { return };
        if self.selection_link_key.as_ref() == Some(&key) {
            return;
        }
        self.selection_link_key = Some(key.clone());
        self.selection_links = Default::default();
        let Some(loaded) = &self.model else { return };
        let resources = key.2.iter().filter_map(|id| loaded.scene.resources.get(*id).map(|r| r.guid.clone())).collect::<Vec<_>>();
        let engine = self.engine.clone();
        self.jobs.spawn("Link selections", false, move |j| {
            let result = engine
                .call(
                    "formats_project_selection_links",
                    json!({"scene_id":key.0,"task_guid":key.1,"resource_guids":resources}),
                    CallOptions::new(Caller::Ui).with_cancel(j.cancel.clone()),
                )
                .map_err(|e| e.for_people())?;
            let value = result.to_json();
            let links = serde_json::from_value(value.get("result").cloned().ok_or("Missing selection result")?).map_err(|e| e.to_string())?;
            Ok(Done::SelectionLinks { key, links })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cube_hit_testing_excludes_empty_corners() {
        let poly = [Pos2::new(0., 1.), Pos2::new(1., 0.), Pos2::new(2., 1.), Pos2::new(1., 2.)];
        assert!(contains(&poly, Pos2::new(1., 1.)));
        assert!(!contains(&poly, Pos2::new(0., 0.)));
    }
}
