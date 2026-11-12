//! Thin animation/report UI over shared headless tools and camera-path sampling.
use super::{Done, SceneApp, project_panel};
use craft_anything_engine::{
    CallOptions, Caller,
    tools::scene_presentation_tools::{Filters, RenderOptions},
};
use craft_anything_engines::scene_render::animation::{Animation, Keyframe};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    pub scene_id: String,
    pub keyframes: Vec<Keyframe>,
    pub seconds: f64,
    pub next_seconds: f64,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub video_path: String,
    pub report_dir: String,
    pub show: bool,
    pub show_reports: bool,
    pub show_audit: bool,
    #[serde(skip)]
    pub playing: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            scene_id: String::new(),
            keyframes: Vec::new(),
            seconds: 0.,
            next_seconds: 0.,
            fps: 12,
            width: 640,
            height: 360,
            video_path: String::new(),
            report_dir: String::new(),
            show: false,
            show_reports: false,
            show_audit: false,
            playing: false,
        }
    }
}
impl SceneApp {
    fn presentation_window<'a>(&self, ui: &egui::Ui, title: &str) -> egui::Window<'a> {
        if self.state.planning.enabled {
            let p = crate::theme::PlanningTokens::default();
            egui::Window::new(egui::RichText::new(title).color(ui.visuals().strong_text_color()))
                .frame(egui::Frame::window(ui.style()).fill(p.surface.chrome).stroke(egui::Stroke::new(1., p.surface.separator)))
        } else {
            egui::Window::new(title.to_owned())
        }
    }

    pub fn camera_path(&self) -> Option<Animation> {
        let id = self.model.as_ref()?.scene.id.clone();
        if self.state.presentation.scene_id != id {
            return None;
        }
        Some(Animation { version: 1, scene_id: id, keyframes: self.state.presentation.keyframes.clone() })
    }
    pub fn presentation_call(&mut self, tool: &str, params: Value) {
        if self.jobs.busy() {
            self.status = "Wait for the current job, then retry this command.".into();
            return;
        }
        let Some(model) = &self.model else { return };
        let scene_id = model.scene.id.clone();
        let engine = self.engine.clone();
        let tool = tool.to_owned();
        self.jobs.spawn("Project presentation / reports", true, move |j| {
            let reporter = j.reporter();
            let out =
                engine.call(&tool, params, CallOptions::new(Caller::Ui).with_cancel(j.cancel.clone()).with_progress(&reporter)).map_err(|e| e.for_people())?;
            Ok(Done::Presentation { scene_id, tool, output: out.to_json() })
        });
    }
    fn options(&self, ui: &egui::Ui) -> RenderOptions {
        let t = if self.state.planning.enabled { crate::theme::PlanningTokens::default().surface } else { crate::theme::Tokens::get(ui.ctx()) };
        let v = &self.state.visibility;
        RenderOptions {
            best_dates: self.state.project.best_dates,
            schedule: self.state.project.mode != project_panel::Mode::Model,
            profile_effects: self.state.project.profile_effects,
            interval_preview: self.state.project.interval_preview,
            source_colors: self.state.project.source_colors,
            object_colors: self.state.project.object_colors,
            texture_preview: self.state.project.texture_preview,
            flip_texture_v: self.state.project.flip_texture_v,
            filters: Filters {
                hidden_resources: v.hidden_resources.clone(),
                isolate_resources: v.isolate_resources.clone(),
                hidden_instances: v.hidden_instances.clone(),
                isolate_instances: v.isolate_instances.clone(),
            },
            palette: (&t).into(),
        }
    }
    pub fn capture_keyframe(&mut self) {
        let Some(model) = &self.model else { return };
        if self.state.presentation.scene_id != model.scene.id {
            self.state.presentation.scene_id = model.scene.id.clone();
            self.state.presentation.keyframes.clear();
        }
        let seconds = if self.state.presentation.keyframes.is_empty() { 0. } else { self.state.presentation.next_seconds };
        if self.state.presentation.keyframes.last().is_some_and(|k| seconds <= k.seconds) || self.state.presentation.keyframes.len() >= 64 || seconds > 600. {
            self.status = "Choose a later keyframe time, within 600 seconds and 64 keyframes.".into();
            return;
        }
        self.state.presentation.keyframes.push(Keyframe { seconds, camera: self.state.camera.clone(), focus: self.state.project.focus });
        self.state.presentation.next_seconds = seconds + 2.;
    }
    fn pick_export(&mut self, video: bool) {
        if self.jobs.busy() {
            return;
        }
        self.jobs.spawn("Choose export location", true, move |_| {
            let path = if video {
                rfd::FileDialog::new().add_filter("Lossless AVI video", &["avi"]).set_file_name("project-video.avi").save_file()
            } else {
                rfd::FileDialog::new().set_file_name("project-report").save_file()
            };
            Ok(Done::PresentationPath { video, path: path.map(|p| p.to_string_lossy().into_owned()) })
        });
    }
    pub(super) fn tick_presentation(&mut self, ui: &egui::Ui) {
        if !self.state.presentation.playing {
            return;
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
        if self.jobs.busy() {
            return;
        }
        let Some(path) = self.camera_path() else {
            self.state.presentation.playing = false;
            return;
        };
        let delta = self.presentation_tick.elapsed().as_secs_f64();
        self.presentation_tick = std::time::Instant::now();
        let seconds = (self.state.presentation.seconds + delta).min(path.duration());
        if let Ok(k) = path.sample(seconds) {
            self.state.presentation.seconds = seconds;
            self.state.camera = k.camera;
            self.state.project.focus = k.focus;
            self.state.project.mode = project_panel::Mode::Schedule;
            self.changed();
        } else {
            self.state.presentation.playing = false;
        }
        if seconds >= path.duration() {
            self.state.presentation.playing = false;
        }
    }
    pub(super) fn presentation_windows(&mut self, ui: &egui::Ui) {
        let mut show = self.state.presentation.show;
        self.presentation_window(ui,"4D camera presentation").open(&mut show).default_width(540.).show(ui.ctx(),|ui|{if self.state.planning.enabled{super::planning::style(ui);}
 ui.label("Capture camera poses and source focus times. The source project stays read only.");
 ui.horizontal(|ui|{ui.label("Next keyframe seconds");ui.add(egui::DragValue::new(&mut self.state.presentation.next_seconds).range(0.0..=600.0).speed(0.1));if ui.add_enabled(self.model.is_some(),egui::Button::new("Capture keyframe")).clicked(){self.capture_keyframe();}});
 egui::ScrollArea::vertical().id_salt("camera_keys").max_height(180.).show(ui,|ui|{let mut remove=None;for(i,k)in self.state.presentation.keyframes.iter_mut().enumerate(){ui.horizontal(|ui|{ui.label(format!("{}",i+1));ui.add(egui::DragValue::new(&mut k.seconds).range(0.0..=600.0).speed(0.1));ui.label(project_panel::wallclock(k.focus));if ui.button("Remove").clicked(){remove=Some(i);}});}if let Some(i)=remove{self.state.presentation.keyframes.remove(i);}});
 let path=self.camera_path();let valid=path.as_ref().is_some_and(|a|a.validate().is_ok());ui.horizontal(|ui|{
 if ui.add_enabled(valid&&!self.jobs.busy(),egui::Button::new("Save animation")).clicked()&&let Some(a)=path.clone(){self.presentation_call("formats_scene_animation_manage",json!({"scene_id":a.scene_id,"animation":a}));}
 if ui.add_enabled(self.model.is_some()&&!self.jobs.busy(),egui::Button::new("Load animation")).clicked()&&let Some(m)=&self.model{self.presentation_call("formats_scene_animation_manage",json!({"scene_id":m.scene.id}));}
 if ui.add_enabled(valid,egui::Button::new(if self.state.presentation.playing{"Pause path"}else{"Play path"})).clicked(){self.state.project.playing=false;self.state.presentation.playing= !self.state.presentation.playing;self.presentation_tick=std::time::Instant::now();if path.as_ref().is_some_and(|p|self.state.presentation.seconds>=p.duration()){self.state.presentation.seconds=0.;}}
 });if let Some(a)=&path{let duration=a.duration().max(0.01);if ui.add(egui::Slider::new(&mut self.state.presentation.seconds,0.0..=duration).text("Path time")).changed()&&let Ok(k)=a.sample(self.state.presentation.seconds){self.state.camera=k.camera;self.state.project.focus=k.focus;self.state.project.mode=project_panel::Mode::Schedule;self.changed();}
if let Err(e)=a.validate(){ui.small(e);}}
 ui.separator();ui.horizontal(|ui|{ui.label("AVI output");ui.text_edit_singleline(&mut self.state.presentation.video_path);if ui.button("Browse video").clicked(){self.pick_export(true);}});ui.horizontal(|ui|{ui.label("Width");ui.add(egui::DragValue::new(&mut self.state.presentation.width).range(2..=1600));ui.label("Height");ui.add(egui::DragValue::new(&mut self.state.presentation.height).range(2..=1600));ui.label("FPS");ui.add(egui::DragValue::new(&mut self.state.presentation.fps).range(1..=30));});
 if ui.add_enabled(valid&&!self.jobs.busy()&&!self.state.presentation.video_path.is_empty(),egui::Button::new("Export AVI")).clicked()&&let Some(a)=path{let params=json!({"scene_id":a.scene_id,"animation":a,"output_path":self.state.presentation.video_path,"fps":self.state.presentation.fps,"size":[self.state.presentation.width,self.state.presentation.height],"options":self.options(ui)});self.presentation_call("formats_scene_video_export",params);}
 ui.add_enabled(false,egui::Button::new("Compressed MP4 (planned)"));ui.small("Lossless AVI: larger files, maximum 512 MiB / 900 frames. Export uses current visibility, colour and 4D preview settings; UI overlays and selection highlights are omitted. The last keyframe is included.");
 });
        self.state.presentation.show = show;
        let mut show = self.state.presentation.show_reports;
        self.presentation_window(ui,"Export schedule and cost reports").open(&mut show).default_width(520.).show(ui.ctx(),|ui|{if self.state.planning.enabled{super::planning::style(ui);}ui.label("Creates a new report folder: tasks, resources, costs, expenses, 4D phases, raw snapshot and manifest.");ui.horizontal(|ui|{ui.text_edit_singleline(&mut self.state.presentation.report_dir);if ui.button("Browse report location").clicked(){self.pick_export(false);}});ui.label(format!("Source focus: {}",project_panel::wallclock(self.state.project.focus)));if ui.add_enabled(self.model.is_some()&&!self.jobs.busy()&&!self.state.presentation.report_dir.is_empty(),egui::Button::new("Export CSV reports")).clicked()&&let Some(m)=&self.model{self.presentation_call("formats_project_report_export",json!({"scene_id":m.scene.id,"output_dir":self.state.presentation.report_dir,"focus":self.state.project.focus,"options":self.options(ui)}));}ui.small("Direct budgets, resource forecasts and recorded expenses remain separate. Missing amounts stay empty; formulas in text are guarded. No currency/unit conversion.");});
        self.state.presentation.show_reports = show;
        let mut show = self.state.presentation.show_audit;
        self.presentation_window(ui, "Source identity and appearance audit").open(&mut show).default_width(550.).show(ui.ctx(), |ui| {
            if self.state.planning.enabled {
                super::planning::style(ui);
            }
            if ui.add_enabled(self.model.is_some() && !self.jobs.busy(), egui::Button::new("Refresh source audit")).clicked()
                && let Some(m) = &self.model
            {
                self.presentation_call("formats_project_audit", json!({"scene_id":m.scene.id}));
            }
            ui.small("Reader display IDs and appearance candidates are distinguished from verified native semantics.");
            if let Some(a) = &self.presentation_audit {
                egui::Grid::new("source_audit_summary").num_columns(2).show(ui, |ui| {
                    for (label, value) in [
                        ("Source tasks", a["tasks"].to_string()),
                        ("Reader IDs present", a["reader_display_ids_present"].to_string()),
                        ("Blank reader IDs", a["reader_display_ids_blank"].to_string()),
                        ("Object/group colour candidates", a["appearance"]["object_group_rgb_candidates"].to_string()),
                        ("Textured mesh candidates", a["appearance"]["textured_mesh_candidates"].to_string()),
                    ] {
                        ui.label(label);
                        ui.label(value);
                        ui.end_row();
                    }
                    for label in ["Native ID rule", "Complete native materials", "Source opacity conversion", "Texture sampling"] {
                        ui.label(label);
                        ui.label("Unresolved");
                        ui.end_row();
                    }
                });
                ui.small("GUID remains the source reference. Controlled RGB cases do not certify other material channels or engineering units.");
                ui.collapsing("Raw evidence", |ui| {
                    egui::ScrollArea::vertical().max_height(260.).show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut serde_json::to_string_pretty(a).unwrap_or_default())
                                .desired_width(ui.available_width())
                                .desired_rows(15)
                                .interactive(false),
                        );
                    });
                });
            }
        });
        self.state.presentation.show_audit = show;
    }
}
