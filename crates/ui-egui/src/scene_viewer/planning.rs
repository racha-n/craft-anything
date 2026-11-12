//! Dense planning-workspace presentation. All dates and costs come from retained engine
//! results. Ribbon commands delegate to existing viewer actions; nothing edits the source.
use super::{SceneApp, project_panel};
use crate::{
    icons,
    theme::{self, PlanningTokens, Tokens},
};
use craft_anything_engine::tools::scene_project_tools::ProjectModel;
use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, Ui, Vec2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tab {
    File,
    #[default]
    Plan,
    Resources,
    Model,
    Review,
    Costs,
    Reports,
    Windows,
    Navigator,
}
/// Visible future work, rather than executable commands. Kept as data so the same list
/// can be checked and documented without inventing functionality or changing source files.
pub struct FutureCommand {
    pub tab: Tab,
    pub group: &'static str,
    pub icon: &'static str,
    pub label: &'static str,
    pub reason: &'static str,
}
pub const FUTURE_COMMANDS: &[FutureCommand] = &[
    FutureCommand { tab: Tab::File, group: "File editing", icon: "file", label: "New project", reason: "Planned: create and write a new project." },
    FutureCommand {
        tab: Tab::File,
        group: "File editing",
        icon: "download",
        label: "Save project",
        reason: "Planned: save edited project data. The current viewer is read only.",
    },
    FutureCommand { tab: Tab::File, group: "File editing", icon: "copy", label: "Save as", reason: "Planned: write a new native project file." },
    FutureCommand { tab: Tab::File, group: "Exchange", icon: "upload", label: "Import schedule", reason: "Planned: import and merge external schedules." },
    FutureCommand {
        tab: Tab::File,
        group: "Exchange",
        icon: "download",
        label: "Export project",
        reason: "Planned: export the project into other file formats.",
    },
    FutureCommand {
        tab: Tab::Plan,
        group: "Task editing",
        icon: "plus",
        label: "New task",
        reason: "Planned: create and edit tasks, with native project saving.",
    },
    FutureCommand { tab: Tab::Plan, group: "Task editing", icon: "list-tree", label: "Indent / outdent", reason: "Planned: edit the task hierarchy." },
    FutureCommand { tab: Tab::Plan, group: "Scheduling", icon: "link", label: "Link tasks", reason: "Planned: edit task dependencies and relationship types." },
    FutureCommand {
        tab: Tab::Plan,
        group: "Scheduling",
        icon: "history",
        label: "Reschedule",
        reason: "Planned: recalculate schedule dates from dependencies and constraints.",
    },
    FutureCommand { tab: Tab::Plan, group: "Scheduling", icon: "git-branch", label: "Critical path", reason: "Planned: critical-path and float analysis." },
    FutureCommand {
        tab: Tab::Plan,
        group: "Planning data",
        icon: "clipboard-list",
        label: "Edit calendars",
        reason: "Planned: edit working calendars. Source calendars are already read for supported date evaluation.",
    },
    FutureCommand { tab: Tab::Plan, group: "Planning data", icon: "copy", label: "Save baseline", reason: "Planned: capture and compare schedule baselines." },
    FutureCommand {
        tab: Tab::Resources,
        group: "Resource editing",
        icon: "plus",
        label: "New resource",
        reason: "Planned: create resources and resource groups.",
    },
    FutureCommand {
        tab: Tab::Resources,
        group: "Resource editing",
        icon: "link",
        label: "Assign resources",
        reason: "Planned: change task/resource assignments and save them.",
    },
    FutureCommand {
        tab: Tab::Resources,
        group: "Resource editing",
        icon: "wand-sparkles",
        label: "Resource wizard",
        reason: "Planned: build resource trees from imported 3D objects.",
    },
    FutureCommand {
        tab: Tab::Resources,
        group: "Appearance",
        icon: "layers",
        label: "Edit profiles",
        reason: "Planned: edit appearance profiles. Supported source profiles can already be previewed.",
    },
    FutureCommand { tab: Tab::Model, group: "3D editing", icon: "upload", label: "Import 3D", reason: "Planned: import and attach additional model files." },
    FutureCommand { tab: Tab::Model, group: "3D editing", icon: "download", label: "Export 3D", reason: "Planned: export the scene into other model formats." },
    FutureCommand {
        tab: Tab::Model,
        group: "3D editing",
        icon: "boxes",
        label: "Transform objects",
        reason: "Planned: edit object placement, rotation and scale.",
    },
    FutureCommand { tab: Tab::Model, group: "3D editing", icon: "eye", label: "Edit materials", reason: "Planned: edit and persist materials and textures." },
    FutureCommand {
        tab: Tab::Costs,
        group: "Cost editing",
        icon: "clipboard-list",
        label: "Edit costs",
        reason: "Planned: edit rates, budgets and actual expenses, then save the project.",
    },
    FutureCommand {
        tab: Tab::Costs,
        group: "Cost editing",
        icon: "gauge",
        label: "Progress entry",
        reason: "Planned: enter actual progress and update the source project.",
    },
    FutureCommand {
        tab: Tab::Costs,
        group: "Analysis",
        icon: "activity",
        label: "Cost curves",
        reason: "Planned: chart planned, actual and forecast costs over time.",
    },
    FutureCommand {
        tab: Tab::Reports,
        group: "Reports",
        icon: "file",
        label: "Formatted report layouts",
        reason: "Planned: page layouts and PDF reports. CSV table/date templates are available.",
    },
    FutureCommand {
        tab: Tab::Windows,
        group: "Window management",
        icon: "app-window",
        label: "Dock / undock",
        reason: "Planned: detach and dock work panes across windows.",
    },
    FutureCommand {
        tab: Tab::Windows,
        group: "Window management",
        icon: "copy",
        label: "Save workspace",
        reason: "Planned: save and restore named pane arrangements.",
    },
    FutureCommand {
        tab: Tab::Navigator,
        group: "Navigation",
        icon: "list-tree",
        label: "Saved searches",
        reason: "Planned: retain named selection and search sets.",
    },
];
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub enabled: bool,
    pub tab: Tab,
    pub show_schedule: bool,
    pub show_properties: bool,
    pub selected_task: Option<String>,
    pub task_filter: String,
    pub viewpoint_name: String,
    pub show_cube: bool,
    pub collapsed_tasks: std::collections::BTreeSet<String>,
    pub gantt_zoom: f64,
    pub gantt_position: f64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            enabled: true,
            tab: Tab::Plan,
            show_schedule: true,
            show_properties: true,
            selected_task: None,
            task_filter: String::new(),
            viewpoint_name: String::new(),
            show_cube: true,
            collapsed_tasks: Default::default(),
            gantt_zoom: 1.,
            gantt_position: 0.5,
        }
    }
}

pub fn style(ui: &mut Ui) {
    let p = PlanningTokens::default();
    let s = ui.style_mut();
    s.visuals = egui::Visuals::light();
    s.visuals.panel_fill = p.surface.chrome;
    s.visuals.window_fill = p.surface.card;
    s.visuals.extreme_bg_color = p.surface.field;
    s.visuals.selection.bg_fill = p.selected_row;
    s.visuals.selection.stroke = Stroke::new(1., p.surface.text);
    s.visuals.widgets.inactive.bg_fill = p.surface.chrome;
    s.visuals.widgets.inactive.weak_bg_fill = p.surface.chrome;
    s.visuals.widgets.inactive.bg_stroke = Stroke::new(1., p.surface.separator);
    s.visuals.widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
    s.visuals.widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
    s.visuals.widgets.active.corner_radius = egui::CornerRadius::ZERO;
    s.spacing.item_spacing = egui::vec2(5., 4.);
    s.spacing.button_padding = egui::vec2(6., 4.);
    s.spacing.interact_size = egui::vec2(28., 22.);
    s.spacing.scroll.bar_width = 10.;
    s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(12.));
    s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(12.));
    s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(theme::CAPTION));
    s.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(15.));
}
pub fn frame(tokens: &Tokens) -> egui::Frame {
    egui::Frame::new().fill(tokens.card).stroke(Stroke::new(1., tokens.separator)).inner_margin(5)
}
pub fn panel_heading(ui: &mut Ui, title: &str) {
    let t = PlanningTokens::default();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 25.), Sense::hover());
    ui.painter().rect_filled(rect, 0., t.surface.chrome);
    ui.painter().text(rect.left_center() + egui::vec2(6., 0.), Align2::LEFT_CENTER, title, FontId::proportional(15.), t.surface.text);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1., t.surface.separator));
}

fn command(ui: &mut Ui, icon: &str, label: &str, enabled: bool) -> bool {
    command_note(ui, icon, label, enabled, "Open a project or select an object to use this command.")
}
fn command_note(ui: &mut Ui, icon: &str, label: &str, enabled: bool, note: &str) -> bool {
    let t = PlanningTokens::default();
    ui.add_enabled_ui(enabled, |ui| {
        let (rect, response) = ui.allocate_exact_size(egui::vec2(71., 64.), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 0., t.surface.hover);
        }
        let color = if enabled { t.ribbon } else { t.surface.text_faint.gamma_multiply(0.5) };
        icons::paint(ui, Rect::from_center_size(rect.center_top() + egui::vec2(0., 16.), Vec2::splat(25.)), icon, 24., color);
        let ink = if enabled { t.surface.text } else { t.surface.text_faint };
        let galley = ui.painter().layout(label.to_string(), FontId::proportional(theme::CAPTION), ink, 69.);
        ui.painter().galley(rect.center_top() + egui::vec2(-galley.size().x / 2., 33.), galley, ink);
        crate::widgets::named(response, label).on_hover_text(label).on_disabled_hover_text(note).clicked()
    })
    .inner
}
fn group<R>(ui: &mut Ui, label: &str, body: impl FnOnce(&mut Ui) -> R) -> R {
    let t = PlanningTokens::default();
    let result = ui
        .vertical(|ui| {
            let r = ui.horizontal(body).inner;
            ui.horizontal(|ui| {
                ui.add_space(5.);
                ui.label(RichText::new(label).size(theme::CAPTION).color(t.surface.text_dim));
            });
            r
        })
        .inner;
    ui.separator();
    result
}

impl SceneApp {
    pub(super) fn choose_file(&mut self) {
        if !self.jobs.busy() {
            self.jobs.spawn("Choose project", true, |_| {
                Ok(super::Done::Chosen(
                    rfd::FileDialog::new().add_filter("Project or scene", &["sp", "json"]).pick_file().map(|p| p.to_string_lossy().into_owned()),
                ))
            });
        }
    }
    pub(super) fn planning_header(&mut self, ui: &mut Ui) {
        let p = PlanningTokens::default();
        egui::Frame::new().fill(p.ribbon).inner_margin(egui::Margin::symmetric(9, 5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                if icons::button(ui, "folder-open", 23., false, "Open a file…").clicked() {
                    self.choose_file();
                }
                let name = std::path::Path::new(&self.path).file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
                ui.label(RichText::new(format!("{name}  —  Craft Anything | Project viewer")).color(Color32::WHITE).size(13.));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new("READ ONLY").color(Color32::WHITE).size(theme::CAPTION));
                });
            });
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.;
                for (tab, label) in [
                    (Tab::File, "File"),
                    (Tab::Plan, "Plan"),
                    (Tab::Resources, "Assign Resources"),
                    (Tab::Model, "3D"),
                    (Tab::Review, "4D Review"),
                    (Tab::Costs, "Project Controls"),
                    (Tab::Reports, "Reports"),
                    (Tab::Windows, "Windows"),
                    (Tab::Navigator, "Navigator"),
                ] {
                    let active = self.state.planning.tab == tab;
                    let button = egui::Button::new(RichText::new(label).color(if active { p.ribbon } else { Color32::WHITE }))
                        .fill(if active { p.surface.chrome } else { p.ribbon })
                        .stroke(Stroke::NONE);
                    if ui.add_sized([if label.len() > 9 { 113. } else { 70. }, 27.], button).clicked() {
                        self.state.planning.tab = tab;
                        match tab {
                            Tab::Model => self.state.project.mode = project_panel::Mode::Model,
                            Tab::Review => self.state.project.mode = project_panel::Mode::Schedule,
                            Tab::Costs => self.state.project.mode = project_panel::Mode::Cost,
                            _ => {}
                        }
                        self.changed();
                    }
                }
            });
        });
        egui::Frame::new().fill(p.surface.chrome).inner_margin(egui::Margin::symmetric(7, 4)).show(ui, |ui| {
            egui::ScrollArea::horizontal().id_salt("planning_ribbon_scroll").show(ui, |ui| {
                ui.horizontal(|ui| {
                    let loaded = self.model.is_some();
                    group(ui, "Project", |ui| {
                        if command(ui, "folder-open", "Browse", !self.jobs.busy()) {
                            self.choose_file();
                        }
                        if command(ui, "file", "Open", !self.jobs.busy() && !self.path.is_empty()) {
                            self.open();
                        }
                    });
                    group(ui, "Workspace", |ui| {
                        if command(ui, "clipboard-list", "Task schedule", loaded) {
                            self.state.planning.show_schedule = !self.state.planning.show_schedule;
                        }
                        if command(ui, "info", "Properties", loaded) {
                            self.state.planning.show_properties = !self.state.planning.show_properties;
                        }
                    });
                    group(ui, "View", |ui| {
                        if command(ui, "search", "Fit all", loaded)
                            && let Some(model) = &self.model
                        {
                            self.state.camera.fit(model.scene.bounds);
                            self.changed();
                        }
                        if command(ui, "boxes", "3D model", loaded) {
                            self.state.project.mode = project_panel::Mode::Model;
                            self.changed();
                        }
                        if command(ui, "history", "4D timeline", loaded) {
                            self.state.project.mode = project_panel::Mode::Schedule;
                            self.changed();
                        }
                        if command(ui, "clipboard-list", "5D costs", loaded) {
                            self.state.project.mode = project_panel::Mode::Cost;
                            self.changed();
                        }
                    });
                    group(ui, "Navigation", |ui| {
                        self.viewpoints_menu(ui);
                        ui.checkbox(&mut self.state.planning.show_cube, "View cube");
                    });
                    if self.state.planning.tab == Tab::Review {
                        group(ui, "Presentation", |ui| {
                            if command(ui, "play", "Camera animation", loaded) {
                                self.state.presentation.show = true;
                            }
                            if command(ui, "download", "Export video", loaded) {
                                self.state.presentation.show = true;
                            }
                            if command(ui, "download", "MP4 video", loaded) {
                                self.state.presentation.mp4 = true;
                                self.set_video_extension();
                                self.state.presentation.show = true;
                            }
                        });
                    }
                    if self.state.planning.tab == Tab::Reports {
                        group(ui, "Reports", |ui| {
                            if command(ui, "file", "Report templates", loaded) {
                                self.state.presentation.show_reports = true;
                            }
                            if command(ui, "clipboard-list", "Schedule report", loaded) {
                                self.state.presentation.show_reports = true;
                            }
                            if command(ui, "clipboard-list", "Cost report", loaded) {
                                self.state.presentation.show_reports = true;
                            }
                        });
                    }
                    if matches!(self.state.planning.tab, Tab::Model | Tab::Navigator) {
                        group(ui, "Source evidence", |ui| {
                            if command(ui, "info", "Source audit", loaded) {
                                self.state.presentation.show_audit = true;
                            }
                        });
                    }
                    match self.state.planning.tab {
                        Tab::File => {
                            group(ui, "Input", |ui| {
                                if command(ui, "clipboard-list", "Schedule only", self.path.to_ascii_lowercase().ends_with(".sp") && !self.jobs.busy()) {
                                    self.metadata = Some(super::metadata_panel::MetadataPanel::new(self.engine.clone(), self.path.clone()));
                                }
                            });
                        }
                        Tab::Plan | Tab::Review => {
                            group(ui, "Time", |ui| {
                                if command(ui, "play", if self.state.project.playing { "Pause timeline" } else { "Play timeline" }, loaded) {
                                    self.state.project.mode = project_panel::Mode::Schedule;
                                    self.state.presentation.playing = false;
                                    self.state.project.playing = !self.state.project.playing;
                                }
                                if command(ui, "layers", "Isolate task", self.state.planning.selected_task.is_some())
                                    && let Some(id) = self.state.planning.selected_task.clone()
                                {
                                    self.select_task(&id);
                                }
                            });
                        }
                        Tab::Resources | Tab::Model => {
                            group(ui, "Camera", |ui| {
                                if command(ui, "boxes", "Isometric", loaded) {
                                    self.state.camera.yaw = -0.8;
                                    self.state.camera.pitch = 0.55;
                                    self.changed();
                                }
                                if command(ui, "chevron-up", "Top view", loaded) {
                                    self.state.camera.pitch = 1.5;
                                    self.changed();
                                }
                                if command(ui, "eye", "Front view", loaded) {
                                    self.state.camera.pitch = 0.;
                                    self.state.camera.yaw = -std::f64::consts::FRAC_PI_2;
                                    self.changed();
                                }
                            });
                        }
                        Tab::Costs => {
                            group(ui, "Project data", |ui| {
                                if command(ui, "clipboard-list", "Cost details", loaded) {
                                    self.state.project.mode = project_panel::Mode::Cost;
                                    self.changed();
                                }
                            });
                        }
                        Tab::Windows => {
                            group(ui, "Layout", |ui| {
                                if command(ui, "app-window", "Compact layout", true) {
                                    self.state.planning.enabled = false;
                                    self.changed();
                                }
                            });
                        }
                        Tab::Reports | Tab::Navigator => {}
                    }
                    let future = FUTURE_COMMANDS.iter().filter(|c| c.tab == self.state.planning.tab).collect::<Vec<_>>();
                    let groups = future.iter().map(|c| c.group).collect::<std::collections::BTreeSet<_>>();
                    for label in groups {
                        group(ui, label, |ui| {
                            for item in future.iter().filter(|c| c.group == label) {
                                command_note(ui, item.icon, item.label, false, &format!("Not supported yet. {}", item.reason));
                            }
                        });
                    }
                    group(ui, "Display", |ui| {
                        ui.menu_button("Display options", |ui| {
                            if project_panel::controls(ui, &mut self.state.project, self.project_frame.as_ref().map(|f| f.frame.as_ref())) {
                                self.changed();
                            }
                            if ui.checkbox(&mut self.state.camera.y_up, "Y up (manual)").changed() {
                                self.changed();
                            }
                        });
                    });
                })
            });
        });
        ui.small("Grey commands are not supported yet — hover for planned work. Coloured commands are available when their inputs are ready.");
        egui::Frame::new().fill(p.surface.chrome).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Project");
                ui.add(
                    egui::TextEdit::singleline(&mut self.path)
                        .desired_width((ui.available_width() * 0.36).max(120.))
                        .hint_text("Absolute path to .sp or scene.json"),
                );
                ui.separator();
                let dates = if self.state.project.best_dates { "Actual / best" } else { "Planned" };
                if ui.button(dates).on_hover_text("Switch displayed source dates").clicked() {
                    self.state.project.best_dates = !self.state.project.best_dates;
                    self.changed();
                }
                ui.label("Focus");
                ui.label(project_panel::wallclock(self.state.project.focus));
                if ui.button("◀").on_hover_text("Previous day").clicked() {
                    self.state.project.focus = self.state.project.focus.saturating_sub(86400);
                    self.changed();
                }
                if ui.button("▶").on_hover_text("Next day").clicked() {
                    self.state.project.focus = self.state.project.focus.saturating_add(86400);
                    self.changed();
                }
                let busy = self.jobs.busy();
                ui.add_visible(busy, egui::Spinner::new().size(14.));
                if ui.add_enabled(busy, egui::Button::new("Cancel")).clicked() {
                    for job in self.jobs.running() {
                        job.cancel.cancel();
                    }
                }
            });
            ui.horizontal(|ui| {
                let range = self.project_frame.as_ref().and_then(|f| f.frame.timeline.start.zip(f.frame.timeline.end));
                let (start, end) = range.unwrap_or((self.state.project.focus, self.state.project.focus.saturating_add(86400)));
                ui.spacing_mut().slider_width = (ui.available_width() - 100.).max(100.);
                if ui
                    .scope(|ui| {
                        ui.visuals_mut().widgets.inactive.bg_fill = p.surface.separator;
                        ui.visuals_mut().widgets.inactive.bg_stroke = Stroke::new(1., p.surface.field_border);
                        ui.add_enabled(
                            range.is_some(),
                            egui::Slider::new(&mut self.state.project.focus, start.saturating_sub(86400)..=end.saturating_add(86400))
                                .show_value(false)
                                .text("Focus time"),
                        )
                    })
                    .inner
                    .changed()
                {
                    self.changed();
                }
            });
        });
    }

    pub(super) fn planning_inspector(&mut self, ui: &mut Ui) {
        let title = if self.state.planning.selected_task.is_some() {
            "Task Properties"
        } else if self.state.selected_resource.is_some() {
            "Resource Properties"
        } else {
            "Object Properties"
        };
        panel_heading(ui, title);
        ui.horizontal(|ui| {
            ui.strong("General");
            ui.separator();
            ui.small("Read only");
        });
        egui::ScrollArea::vertical().id_salt("planning_properties").show(ui, |ui| {
            if let Some(task) =
                self.state.planning.selected_task.as_deref().and_then(|id| self.project_model.as_ref()?.model.tasks.iter().find(|t| t.guid == id))
            {
                section(ui, "Identity");
                property(ui, "Native ID", if task.display_id.is_empty() { "Unresolved; grid uses GUID" } else { &task.display_id });
                property(ui, "Name", &task.name);
                property(ui, "GUID", &task.guid);
                section(ui, "Dates");
                property(ui, "Planned start", &date(task.planned_start));
                property(ui, "Planned finish", &date(task.planned_finish));
                property(ui, "Best start", &date(task.best_start));
                property(ui, "Best finish", &date(task.best_finish));
                section(ui, "Assignments");
                property(ui, "Resources", &task.assignment_count.to_string());
                property(ui, "Calendar", task.effective_calendar_guid.as_deref().unwrap_or("Unresolved"));
                if let Some(frame) = &self.project_frame {
                    let lines = frame.frame.costs.lines.iter().filter(|line| line.task_guid == task.guid).collect::<Vec<_>>();
                    if !lines.is_empty() {
                        section(ui, "Budget");
                        for line in lines.iter().take(12) {
                            property(ui, if line.resource_guid.is_none() { "Direct" } else { "Resource forecast" }, &project_panel::money(line.budget_minor));
                        }
                    }
                }
                for reason in &task.unresolved_reasons {
                    ui.small(reason);
                }
            } else if let Some(model) = &self.model {
                if let Some(resource) = self.state.selected_resource.and_then(|id| model.scene.resources.get(id)) {
                    section(ui, "Identity");
                    property(ui, "Name", &resource.name);
                    property(ui, "GUID", &resource.guid);
                    property(ui, "Parent", resource.parent_guid.as_deref().unwrap_or("Root"));
                    property(
                        ui,
                        "3D instances",
                        &self.state.selected_resource.and_then(|id| model.scene.resource_instance_counts.get(id)).copied().unwrap_or(0).to_string(),
                    );
                } else if let Some(instance) = self.state.selected_instance.and_then(|id| model.scene.instances.get(id)) {
                    section(ui, "Identity");
                    property(ui, "Entity GUID", &instance.entity_guid);
                    property(ui, "Mesh", &instance.mesh.to_string());
                    property(ui, "Resource links", &instance.resources.len().to_string());
                    if let Some(mesh) = model.scene.meshes.get(instance.mesh) {
                        property(ui, "Vertices", &mesh.vertex_count.to_string());
                        property(ui, "Triangles", &mesh.triangle_count.to_string());
                    }
                } else {
                    ui.label("Select a task, resource or 3D object to inspect its source properties.");
                }
                section(ui, "Project");
                property(ui, "Instances", &model.scene.instances.len().to_string());
                property(ui, "Resources", &model.scene.resources.len().to_string());
            } else {
                ui.label("Open a project to inspect its contents.");
            }
            self.planning_selection_links(ui);
            section(ui, "Scope");
            ui.small("Experimental read-only viewer. Unsupported values remain unresolved. Native visual and engineering-quantity parity are not certified.");
        });
    }

    pub(super) fn planning_schedule(&mut self, ui: &mut Ui) {
        let p = PlanningTokens::default();
        ui.horizontal(|ui| {
            ui.strong("Tasks");
            ui.separator();
            ui.label("Gantt");
            ui.add(egui::TextEdit::singleline(&mut self.state.planning.task_filter).hint_text("Find task / ID").desired_width(170.));
            if let Some(frame) = &self.project_frame {
                ui.small(format!("{} scheduled · {} unresolved", frame.frame.timeline.resolved_tasks, frame.frame.timeline.unresolved_tasks));
            }
        });
        let Some(model) = self.project_model.as_ref().map(|p| p.model.clone()) else {
            ui.label(if self.model.as_ref().is_some_and(|m| m.metadata.get("tasks").is_none()) {
                "This scene bundle has no source task schedule."
            } else {
                "Reading source task schedule…"
            });
            return;
        };
        ui.horizontal(|ui| {
            if ui.button("Expand all").clicked() {
                self.state.planning.collapsed_tasks.clear();
            }
            if ui.button("Collapse all").clicked() {
                self.state.planning.collapsed_tasks = model.tasks.iter().filter_map(|t| t.parent_guid.clone()).collect();
            }
            ui.separator();
            if ui.button("Zoom time +").clicked() {
                self.state.planning.gantt_zoom = (self.state.planning.gantt_zoom * 2.).clamp(1., 128.);
            }
            if ui.button("Zoom time -").clicked() {
                self.state.planning.gantt_zoom = (self.state.planning.gantt_zoom / 2.).clamp(1., 128.);
            }
            if ui.button("Earlier").clicked() {
                self.state.planning.gantt_position = (self.state.planning.gantt_position - 0.25 / self.state.planning.gantt_zoom.max(1.)).clamp(0., 1.);
            }
            if ui.button("Later").clicked() {
                self.state.planning.gantt_position = (self.state.planning.gantt_position + 0.25 / self.state.planning.gantt_zoom.max(1.)).clamp(0., 1.);
            }
            if ui.button("Fit timeline").clicked() {
                self.state.planning.gantt_zoom = 1.;
                self.state.planning.gantt_position = 0.5;
            }
        });
        let parents = model.tasks.iter().map(|t| (t.guid.as_str(), t.parent_guid.as_deref())).collect::<BTreeMap<_, _>>();
        let query = self.state.planning.task_filter.to_lowercase();
        let rows = model
            .tasks
            .iter()
            .filter(|t| {
                if query.is_empty() {
                    !ancestor_collapsed(&t.guid, &parents, &self.state.planning.collapsed_tasks)
                } else {
                    t.name.to_lowercase().contains(&query) || t.display_id.to_lowercase().contains(&query) || t.guid.to_lowercase().contains(&query)
                }
            })
            .collect::<Vec<_>>();
        let widths = columns(ui.available_width());
        let range = timeline_range(&model, self.state.project.best_dates)
            .map(|r| visible_time_range(r, self.state.planning.gantt_zoom, self.state.planning.gantt_position));
        let (header, _) = ui.allocate_exact_size(egui::vec2(ui.available_width() - 12., 37.), Sense::hover());
        ui.painter().rect_filled(header, 0., p.surface.chrome);
        let mut x = header.left();
        for (w, label) in widths.table.into_iter().zip(["ID / GUID", "Name", "Duration", "Start", "Finish"]) {
            ui.painter().text(egui::pos2(x + 5., header.center().y), Align2::LEFT_CENTER, label, FontId::proportional(theme::CAPTION), p.surface.text);
            x += w;
            ui.painter().line_segment([egui::pos2(x, header.top()), egui::pos2(x, header.bottom())], Stroke::new(1., p.surface.separator));
        }
        let chart = Rect::from_min_max(egui::pos2(x, header.top()), header.right_bottom());
        if let Some((start, end)) = range {
            for tick in 0..5 {
                let ratio = f64::from(tick) / 4.;
                let time = start.saturating_add(((end as f64 - start as f64) * ratio) as i64);
                let px = chart.left() + ratio as f32 * chart.width();
                let label = project_panel::wallclock(time).chars().take(10).collect::<String>();
                ui.painter().text(egui::pos2(px + 4., header.center().y), Align2::LEFT_CENTER, label, FontId::proportional(theme::CAPTION), p.surface.text_dim);
            }
        } else {
            ui.painter().text(chart.center(), Align2::CENTER_CENTER, "Unresolved date range", FontId::proportional(theme::CAPTION), p.surface.text_dim);
        }
        let mut fold = None;
        let mut picked = None;
        egui::ScrollArea::vertical().id_salt("planning_task_gantt_rows").auto_shrink([false, false]).show_rows(ui, 22., rows.len(), |ui, visible| {
            for index in visible {
                let Some(task) = rows.get(index) else {
                    continue;
                };
                let (row, response) = ui.allocate_exact_size(egui::vec2(header.width(), 22.), Sense::click());
                let active = self.state.planning.selected_task.as_deref() == Some(task.guid.as_str());
                ui.painter().rect_filled(
                    row,
                    0.,
                    if active {
                        p.selected_row
                    } else if index % 2 == 0 {
                        p.surface.card
                    } else {
                        p.alternate_row
                    },
                );
                let (start, finish) = task_dates(task, self.state.project.best_dates);
                let is_parent = model.tasks.iter().any(|t| t.parent_guid.as_deref() == Some(task.guid.as_str()));
                let duration = if is_parent { "—".into() } else { format!("{:.1} h", task.duration_storage_scalar as f64 / 3600.) };
                let cells = [
                    if task.display_id.is_empty() { task.guid.chars().take(8).collect() } else { task.display_id.clone() },
                    task.name.clone(),
                    duration,
                    date_short(start),
                    date_short(finish),
                ];
                let mut left = row.left();
                for (cell, (width, text)) in widths.table.iter().zip(cells).enumerate() {
                    let cell_rect = Rect::from_min_max(egui::pos2(left, row.top()), egui::pos2(left + width, row.bottom()));
                    let indent = if cell == 1 { task_depth(&task.guid, &parents) as f32 * 9. + 18. } else { 0. };
                    if cell == 1 && is_parent {
                        let toggle = Rect::from_center_size(cell_rect.left_center() + egui::vec2(indent - 5., 0.), egui::vec2(16., 20.));
                        let response =
                            crate::widgets::named(ui.interact(toggle, ui.id().with(("fold", &task.guid)), Sense::click()), &format!("Fold task {}", task.guid));
                        ui.painter().text(
                            toggle.center(),
                            Align2::CENTER_CENTER,
                            if self.state.planning.collapsed_tasks.contains(&task.guid) { "+" } else { "-" },
                            FontId::proportional(theme::CAPTION),
                            p.surface.text,
                        );
                        if response.clicked() {
                            fold = Some(task.guid.clone());
                        }
                    }
                    ui.painter().with_clip_rect(cell_rect.shrink(3.)).text(
                        cell_rect.left_center() + egui::vec2(4. + indent, 0.),
                        Align2::LEFT_CENTER,
                        text,
                        FontId::proportional(theme::CAPTION),
                        p.surface.text,
                    );
                    left += width;
                    ui.painter().line_segment([egui::pos2(left, row.top()), egui::pos2(left, row.bottom())], Stroke::new(1., p.surface.separator));
                }
                let chart = Rect::from_min_max(egui::pos2(left, row.top()), row.right_bottom());
                for tick in 0..5 {
                    let x = chart.left() + tick as f32 / 4. * chart.width();
                    ui.painter().line_segment([egui::pos2(x, row.top()), egui::pos2(x, row.bottom())], Stroke::new(1., p.surface.separator));
                }
                if let Some((first, last)) = range {
                    if let Some((a, b)) = start.zip(finish)
                        && a <= last
                        && b >= first
                    {
                        let a = time_x(a, first, last, chart);
                        let b = time_x(b, first, last, chart);
                        let color = if is_parent { p.summary_bar } else { p.task_bar };
                        if (b - a).abs() < 2. {
                            let c = egui::pos2(a, row.center().y);
                            ui.painter().add(egui::Shape::convex_polygon(
                                vec![c + egui::vec2(0., -5.), c + egui::vec2(5., 0.), c + egui::vec2(0., 5.), c + egui::vec2(-5., 0.)],
                                color,
                                Stroke::NONE,
                            ));
                        } else {
                            let bar = Rect::from_min_max(egui::pos2(a, row.top() + 6.), egui::pos2(b.max(a + 2.), row.bottom() - 6.));
                            ui.painter().rect_filled(bar, 0., color);
                        }
                    }
                    let focus = time_x(self.state.project.focus, first, last, chart);
                    if (first..=last).contains(&self.state.project.focus) {
                        ui.painter().line_segment([egui::pos2(focus, row.top()), egui::pos2(focus, row.bottom())], Stroke::new(1.5, p.focus_line));
                    }
                }
                ui.painter().line_segment([row.left_bottom(), row.right_bottom()], Stroke::new(1., p.surface.separator));
                if crate::widgets::named(response, &format!("Task {}", task.guid))
                    .on_hover_text(format!(
                        "{}\nSource GUID: {}\n{} -> {}\nSource planned duration in hours; source remains read only",
                        task.name,
                        task.guid,
                        date(start),
                        date(finish)
                    ))
                    .clicked()
                {
                    picked = Some(task.guid.clone());
                }
            }
        });
        if let Some(id) = fold {
            if !self.state.planning.collapsed_tasks.remove(&id) {
                self.state.planning.collapsed_tasks.insert(id);
            }
        } else if let Some(id) = picked {
            self.state.selected_resource = None;
            self.state.planning.selected_task = Some(id);
            self.state.planning.show_properties = true;
            self.state.selected_instance = None;
            self.changed();
        }
    }
}
fn section(ui: &mut Ui, label: &str) {
    let p = PlanningTokens::default();
    egui::Frame::new().fill(p.surface.chrome).inner_margin(4).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.strong(label);
    });
}
fn property(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        ui.add_sized([85., 20.], egui::Label::new(RichText::new(label).color(PlanningTokens::default().surface.text_dim)).truncate());
        ui.add(egui::Label::new(value).wrap());
    });
    ui.separator();
}
fn date(v: Option<i64>) -> String {
    v.map(project_panel::wallclock).unwrap_or_else(|| "Unresolved".into())
}
fn date_short(v: Option<i64>) -> String {
    v.map(|v| project_panel::wallclock(v).chars().take(10).collect()).unwrap_or_else(|| "Unresolved".into())
}
struct Columns {
    table: [f32; 5],
}
fn columns(width: f32) -> Columns {
    let table = (width * 0.58).clamp(260., 660.);
    let fixed = if width > 850. { [70., 78., 100., 100.] } else { [70., 60., 78., 78.] };
    Columns { table: [fixed[0], (table - fixed.iter().sum::<f32>()).max(100.), fixed[1], fixed[2], fixed[3]] }
}
fn task_dates(task: &craft_anything_engine::tools::scene_project_tools::TaskView, best: bool) -> (Option<i64>, Option<i64>) {
    if best { (task.best_start, task.best_finish) } else { (task.planned_start, task.planned_finish) }
}
fn timeline_range(model: &ProjectModel, best: bool) -> Option<(i64, i64)> {
    let start = model.tasks.iter().filter_map(|t| task_dates(t, best).0).min()?;
    let end = model.tasks.iter().filter_map(|t| task_dates(t, best).1).max()?;
    Some((start, end.max(start.saturating_add(1))))
}
fn visible_time_range((start, end): (i64, i64), zoom: f64, position: f64) -> (i64, i64) {
    let zoom = if zoom.is_finite() { zoom.clamp(1., 128.) } else { 1. };
    let position = if position.is_finite() { position.clamp(0., 1.) } else { 0.5 };
    let span = (end as f64 - start as f64).max(1.);
    let width = (span / zoom).max(1.);
    let left = (position * span - width / 2.).clamp(0., (span - width).max(0.));
    let a = (start as f64 + left) as i64;
    let b = (start as f64 + left + width) as i64;
    (a, b.max(a.saturating_add(1)))
}
fn ancestor_collapsed(id: &str, parents: &BTreeMap<&str, Option<&str>>, collapsed: &std::collections::BTreeSet<String>) -> bool {
    let mut cursor = parents.get(id).copied().flatten();
    for _ in 0..32 {
        let Some(parent) = cursor else { return false };
        if collapsed.contains(parent) {
            return true;
        }
        cursor = parents.get(parent).copied().flatten();
    }
    false
}
fn time_x(time: i64, start: i64, end: i64, rect: Rect) -> f32 {
    let ratio = ((time as f64 - start as f64) / (end as f64 - start as f64).max(1.)).clamp(0., 1.);
    rect.left() + ratio as f32 * rect.width()
}
fn task_depth(id: &str, parents: &BTreeMap<&str, Option<&str>>) -> usize {
    let mut depth = 0;
    let mut cursor = parents.get(id).copied().flatten();
    while let Some(id) = cursor {
        depth += 1;
        if depth >= 32 {
            break;
        }
        cursor = parents.get(id).copied().flatten();
    }
    depth
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn future_command_inventory_has_unique_labels_real_icons_and_explanations() {
        let mut labels = std::collections::BTreeSet::new();
        for command in FUTURE_COMMANDS {
            assert!(labels.insert(command.label), "duplicate command {}", command.label);
            assert!(crate::icons::exists(command.icon), "missing icon {}", command.icon);
            assert!(command.reason.starts_with("Planned: ") && command.reason.len() > 25);
        }
        assert!(FUTURE_COMMANDS.iter().any(|c| c.tab == Tab::File && c.label == "Save project"));
    }
    #[test]
    fn time_zoom_and_hierarchy_filter_preserve_search_and_bounds() {
        assert_eq!(visible_time_range((0, 100), 2., 0.5), (25, 75));
        assert_eq!(visible_time_range((0, 100), 2., 0.), (0, 50));
        assert_eq!(visible_time_range((0, 100), 2., 1.), (50, 100));
        assert_eq!(visible_time_range((0, 100), f64::NAN, f64::NAN), (0, 100));
        let parents = BTreeMap::from([("root", None), ("child", Some("root")), ("leaf", Some("child"))]);
        let collapsed = std::collections::BTreeSet::from(["root".to_owned()]);
        assert!(!ancestor_collapsed("root", &parents, &collapsed));
        assert!(ancestor_collapsed("child", &parents, &collapsed));
        assert!(ancestor_collapsed("leaf", &parents, &collapsed));
    }
    #[test]
    fn gantt_uses_real_time_span_and_bounds_extreme_focus() {
        let rect = Rect::from_min_max(egui::pos2(100., 0.), egui::pos2(500., 22.));
        assert_eq!(time_x(150, 100, 300, rect), 200.);
        assert_eq!(time_x(i64::MIN, 100, 300, rect), 100.);
        assert_eq!(time_x(i64::MAX, 100, 300, rect), 500.);
        assert!(time_x(100, 100, 100, rect).is_finite());
    }
    #[test]
    fn hierarchy_depth_is_bounded_even_for_malformed_cycles() {
        let parents = BTreeMap::from([("root", None), ("child", Some("root")), ("leaf", Some("child"))]);
        assert_eq!(task_depth("leaf", &parents), 2);
        let cycle = BTreeMap::from([("a", Some("b")), ("b", Some("a"))]);
        assert_eq!(task_depth("a", &cycle), 32);
    }
    #[test]
    fn old_state_defaults_to_planning_and_compact_is_reversible() {
        let state: State = serde_json::from_str("{}").unwrap();
        assert!(state.enabled && state.show_schedule && state.show_properties);
        assert_eq!(state.tab, Tab::Plan);
        let mut compact = state;
        compact.enabled = false;
        let restored: State = serde_json::from_str(&serde_json::to_string(&compact).unwrap()).unwrap();
        assert!(!restored.enabled);
    }
}
