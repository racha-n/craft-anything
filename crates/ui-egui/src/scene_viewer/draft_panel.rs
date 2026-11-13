//! Isolated, experimental scalar forms over the shared draft tool.
use super::{SceneApp, project_panel};
use crate::jobs::Jobs;
use craft_anything_engine::{CallOptions, Caller, Engine, tools::scene_project_tools::TaskView};
use craft_anything_formats::sp_project::writer::ScalarEdit;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone, Serialize, Deserialize)]
struct Assignment {
    resource_guid: String,
    profile_guid: String,
    planned_units: f64,
    rate_storage_window: f64,
}
#[derive(Clone, Serialize, Deserialize)]
struct Cost {
    guid: String,
    name: String,
    amount_storage: f64,
    type_storage_value: u32,
}
#[derive(Clone, Serialize, Deserialize)]
struct Task {
    guid: String,
    name: String,
    #[serde(default)]
    display_id: String,
    planned_start_wallclock_scalar: u64,
    duration_storage_scalar: u64,
    assignments: Vec<Assignment>,
    direct_costs: Vec<Cost>,
}
impl Task {
    fn label(&self) -> String {
        if self.display_id.is_empty() { self.name.clone() } else { format!("{} · {}", self.display_id, self.name) }
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct DerivedPage {
    items: Vec<TaskView>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    version: u32,
    draft_id: u64,
    source_sha256: String,
    revision: u64,
    dirty: bool,
    can_undo: bool,
    can_redo: bool,
    edit_count: usize,
    task_total: usize,
    offset: usize,
    tasks: Vec<Task>,
    derived_tasks: DerivedPage,
}
#[derive(Clone, Serialize, Deserialize)]
struct Field {
    label: String,
    original: ScalarEdit,
    text: String,
}
impl Field {
    fn proposal(&self) -> Result<Option<ScalarEdit>, String> {
        let mut edit = self.original.clone();
        let changed = match &mut edit {
            ScalarEdit::TaskStart { expected_storage, value_storage, .. } | ScalarEdit::TaskDuration { expected_storage, value_storage, .. } => {
                *value_storage = self.text.parse().map_err(|_| format!("{} requires a whole source value.", self.label))?;
                *expected_storage != *value_storage
            }
            ScalarEdit::AssignmentPlannedUnits { expected_storage, value_storage, .. }
            | ScalarEdit::AssignmentPlannedRate { expected_storage, value_storage, .. }
            | ScalarEdit::DirectFixedAmount { expected_storage, value_storage, .. } => {
                let value: f64 = self.text.parse().map_err(|_| format!("{} requires a number.", self.label))?;
                if !value.is_finite() || !(0.0..=1e12).contains(&value) {
                    return Err(format!("{} requires a finite value from 0 to 1e12.", self.label));
                }
                *value_storage = value;
                expected_storage.to_bits() != value.to_bits()
            }
        };
        Ok(changed.then_some(edit))
    }
}
fn fields(task: &Task, offset: usize) -> Vec<Field> {
    let task_id = &task.guid;
    let task_fields = [
        Field {
            label: "Start (stored displayed-clock value)".into(),
            text: task.planned_start_wallclock_scalar.to_string(),
            original: ScalarEdit::TaskStart {
                task_guid: task_id.clone(),
                expected_storage: task.planned_start_wallclock_scalar,
                value_storage: task.planned_start_wallclock_scalar,
            },
        },
        Field {
            label: "Duration (stored value)".into(),
            text: task.duration_storage_scalar.to_string(),
            original: ScalarEdit::TaskDuration {
                task_guid: task_id.clone(),
                expected_storage: task.duration_storage_scalar,
                value_storage: task.duration_storage_scalar,
            },
        },
    ];
    let assignment_fields = task.assignments.iter().enumerate().flat_map(|(ordinal, a)| {
        [
            Field {
                label: format!("Assignment {} quantity (source units)", ordinal + 1),
                text: a.planned_units.to_string(),
                original: ScalarEdit::AssignmentPlannedUnits {
                    task_guid: task_id.clone(),
                    assignment_ordinal: ordinal,
                    resource_guid: a.resource_guid.clone(),
                    profile_guid: a.profile_guid.clone(),
                    expected_storage: a.planned_units,
                    value_storage: a.planned_units,
                },
            },
            Field {
                label: format!("Assignment {} utilization (stored value)", ordinal + 1),
                text: a.rate_storage_window.to_string(),
                original: ScalarEdit::AssignmentPlannedRate {
                    task_guid: task_id.clone(),
                    assignment_ordinal: ordinal,
                    resource_guid: a.resource_guid.clone(),
                    profile_guid: a.profile_guid.clone(),
                    expected_storage: a.rate_storage_window,
                    value_storage: a.rate_storage_window,
                },
            },
        ]
    });
    let costs = task.direct_costs.iter().filter(|c| c.type_storage_value == 1).map(|c| Field {
        label: format!("Fixed cost: {}", c.name),
        text: c.amount_storage.to_string(),
        original: ScalarEdit::DirectFixedAmount {
            task_guid: task_id.clone(),
            cost_guid: c.guid.clone(),
            expected_storage: c.amount_storage,
            value_storage: c.amount_storage,
        },
    });
    task_fields.into_iter().chain(assignment_fields).chain(costs).skip(offset).take(32).collect()
}
#[derive(Default, Serialize, Deserialize)]
struct State {
    show: bool,
    open_requested: bool,
    planning: bool,
    snapshot: Option<Snapshot>,
    selected: usize,
    field_offset: usize,
    fields: Vec<Field>,
    output: String,
    acknowledge: bool,
    discard: bool,
    status: String,
}
impl State {
    fn accept(&mut self, value: Snapshot, source: &str, expected_id: Option<u64>) -> Result<(), String> {
        if value.version != 1 || value.source_sha256 != source || expected_id.is_some_and(|id| id != value.draft_id) {
            return Err("Draft identity changed; this response was not applied.".into());
        }
        if value.tasks.len() > 100 || value.derived_tasks.items.len() > 100 {
            return Err("Draft response exceeded its page size.".into());
        }
        if self.snapshot.as_ref().is_some_and(|old| old.draft_id == value.draft_id && old.revision > value.revision) {
            return Err("An older draft response was not applied.".into());
        }
        self.selected = self.selected.min(value.tasks.len().saturating_sub(1));
        self.field_offset = 0;
        self.fields = value.tasks.get(self.selected).map(|t| fields(t, 0)).unwrap_or_default();
        self.snapshot = Some(value);
        self.discard = false;
        self.acknowledge = false;
        self.status = "Draft preview refreshed. The source viewport remains unchanged.".into();
        Ok(())
    }
}

enum Done {
    Snapshot(Box<Snapshot>, Option<u64>),
    Closed,
    Exported(String),
    Chosen(Option<String>),
}
pub(super) struct DraftPanel {
    engine: Arc<Engine>,
    source_path: String,
    source_hash: String,
    state: State,
    jobs: Jobs<Done>,
}
impl DraftPanel {
    fn new(engine: Arc<Engine>, source_path: String, source_hash: String) -> Self {
        Self { engine, source_path, source_hash, state: State { show: true, open_requested: true, ..Default::default() }, jobs: Jobs::default() }
    }
    pub(super) fn active(&self) -> bool {
        self.state.open_requested || self.state.snapshot.is_some() || self.jobs.busy()
    }
    fn needs_close_confirmation(&self) -> bool {
        self.state.open_requested || self.state.snapshot.as_ref().is_some_and(|s| s.dirty) || self.jobs.busy()
    }
    pub(super) fn cancel(&self) {
        for job in self.jobs.running() {
            job.cancel.cancel();
        }
    }
    fn send(&mut self, action: &str, extra: Value) {
        if self.jobs.busy() {
            return;
        }
        let expected_id = self.state.snapshot.as_ref().map(|s| s.draft_id);
        let mut params = json!({"action":action,"expected_source_sha256":self.source_hash});
        if action == "open" {
            params["path"] = json!(self.source_path);
        } else if let Some(s) = &self.state.snapshot {
            params["draft_id"] = json!(s.draft_id);
            if action != "refresh" {
                params["revision"] = json!(s.revision);
            }
        } else {
            return;
        }
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                params[key] = value.clone();
            }
        }
        let engine = self.engine.clone();
        let action = action.to_owned();
        self.state.status = "Working on draft…".into();
        self.jobs.spawn("Project draft", true, move |job| {
            let report = job.reporter();
            let output = engine
                .call("formats_project_edit", params.clone(), CallOptions::new(Caller::Ui).with_cancel(job.cancel.clone()).with_progress(&report))
                .map_err(|e| e.for_people())?
                .to_json();
            let value = output.get("result").ok_or("Draft tool returned no structured result")?;
            match action.as_str() {
                "close" if value.get("closed").and_then(Value::as_bool) == Some(true) => Ok(Done::Closed),
                "write_candidate" => Ok(Done::Exported(params.get("output_path").and_then(Value::as_str).unwrap_or_default().to_owned())),
                _ => serde_json::from_value(value.clone())
                    .map(|s| Done::Snapshot(Box::new(s), if action == "open" { None } else { expected_id }))
                    .map_err(|e| e.to_string()),
            }
        });
    }
    fn show(&mut self, ctx: &egui::Context) {
        self.jobs.set_repaint(ctx);
        if self.state.open_requested {
            self.state.open_requested = false;
            self.send("open", json!({}));
        }
        for result in self.jobs.poll() {
            match result.result {
                Ok(Done::Snapshot(value, id)) => {
                    if let Err(error) = self.state.accept(*value, &self.source_hash, id) {
                        self.state.status = error;
                    }
                }
                Ok(Done::Closed) => {
                    self.state.snapshot = None;
                    self.state.fields.clear();
                    self.state.show = false;
                    self.state.status = "Draft closed.".into();
                }
                Ok(Done::Exported(path)) => self.state.status = format!("Candidate exported: {path}. Validate it in the native application."),
                Ok(Done::Chosen(Some(path))) => self.state.output = path,
                Ok(Done::Chosen(None)) => {}
                Err(error) => {
                    self.state.status = error;
                    self.state.show = true;
                }
            }
        }
        if !self.state.show {
            return;
        }
        let mut show = self.state.show;
        let busy = self.jobs.busy();
        let mut action = None;
        let mut choose = false;
        let surface = if self.state.planning { crate::theme::PlanningTokens::default().surface } else { crate::theme::Tokens::get(ctx) };
        egui::Window::new(egui::RichText::new("Experimental project draft").color(ctx.global_style().visuals.strong_text_color()))
            .id(egui::Id::new("project_scalar_draft"))
            .frame(egui::Frame::window(ctx.global_style().as_ref()).fill(surface.card).stroke(egui::Stroke::new(1., surface.separator)))
            .open(&mut show).default_width(670.).default_height(650.).vscroll(true).show(ctx, |ui| {
            if self.state.planning { super::planning::style(ui); }
            ui.label(&self.source_path);
            ui.small("Preview stored values and export a separate SP candidate. Automatic scheduling and utilization coordination are not admitted for general projects.");
            ui.label(&self.state.status);
            if busy { ui.spinner(); if ui.button("Cancel draft operation").clicked() { self.cancel(); } }
            ui.add_enabled_ui(!busy, |ui| {
                let Some(snapshot) = &self.state.snapshot else {
                    if ui.button("Open draft").clicked() { action = Some(("open", json!({}))); }
                    return;
                };
                ui.horizontal(|ui| {
                    ui.label(format!("Revision {} · {} edited fields", snapshot.revision, snapshot.edit_count));
                    if ui.add_enabled(snapshot.can_undo, egui::Button::new("Undo")).clicked() { action = Some(("undo", json!({}))); }
                    if ui.add_enabled(snapshot.can_redo, egui::Button::new("Redo")).clicked() { action = Some(("redo", json!({}))); }
                    if ui.button("Refresh").clicked() { action = Some(("refresh", json!({"offset":snapshot.offset}))); }
                });
                ui.horizontal(|ui| {
                    if ui.add_enabled(snapshot.offset > 0, egui::Button::new("Previous tasks")).clicked() { action = Some(("refresh", json!({"offset":snapshot.offset.saturating_sub(100)}))); }
                    ui.label(format!("{} / {} tasks", snapshot.offset.saturating_add(snapshot.tasks.len()), snapshot.task_total));
                    if ui.add_enabled(snapshot.offset.saturating_add(snapshot.tasks.len()) < snapshot.task_total, egui::Button::new("Next tasks")).clicked() { action = Some(("refresh", json!({"offset":snapshot.offset.saturating_add(100)}))); }
                });
                let old = self.state.selected;
                egui::ComboBox::from_id_salt("draft_task").selected_text(snapshot.tasks.get(old).map(Task::label).unwrap_or_else(|| "No task".into())).show_ui(ui, |ui| {
                    for (index, task) in snapshot.tasks.iter().enumerate() { ui.selectable_value(&mut self.state.selected, index, task.label()).on_hover_text(&task.guid); }
                });
                if let Some(task) = snapshot.tasks.get(self.state.selected) {
                    if old != self.state.selected { self.state.field_offset = 0; self.state.fields = fields(task, 0); }
                    if let Some(derived) = snapshot.derived_tasks.items.iter().find(|t| t.guid == task.guid) {
                        ui.label(derived.planned_start.zip(derived.planned_finish).map(|(a,b)| format!("Preview dates: {} → {}", project_panel::wallclock(a), project_panel::wallclock(b))).unwrap_or_else(|| "Preview dates unresolved".into()));
                        for reason in derived.unresolved_reasons.iter().take(4) { ui.small(reason); }
                    }
                    ui.collapsing("Advanced scalar fields", |ui| {
                        ui.small("Values use source storage units. Quantity and utilization are separate fields; changed fields on this page are applied together.");
                        ui.horizontal(|ui| {
                            if ui.add_enabled(self.state.field_offset > 0, egui::Button::new("Previous fields")).clicked() { self.state.field_offset = self.state.field_offset.saturating_sub(32); self.state.fields = fields(task, self.state.field_offset); }
                            if ui.add_enabled(self.state.fields.len() == 32, egui::Button::new("Next fields")).clicked() { self.state.field_offset = self.state.field_offset.saturating_add(32); self.state.fields = fields(task, self.state.field_offset); }
                        });
                        egui::ScrollArea::vertical().id_salt("draft_fields").max_height(280.).show(ui, |ui| {
                            for (index, field) in self.state.fields.iter_mut().enumerate() { ui.push_id(index, |ui| { ui.horizontal(|ui| { let label_width = (ui.available_width() - 180.).clamp(120., 420.); let label = ui.add_sized(egui::vec2(label_width, ui.spacing().interact_size.y), egui::Label::new(&field.label).truncate()).on_hover_text(&field.label); ui.add(egui::TextEdit::singleline(&mut field.text).char_limit(64).desired_width(170.)).labelled_by(label.id); }); }); }
                        });
                        let proposals = self.state.fields.iter().map(Field::proposal).collect::<Result<Vec<_>, _>>();
                        match proposals {
                            Ok(values) => { let edits = values.into_iter().flatten().collect::<Vec<_>>(); if ui.add_enabled(!edits.is_empty(), egui::Button::new("Apply to draft")).clicked() { action = Some(("apply", json!({"edits":edits}))); } }
                            Err(error) => { ui.label(error); }
                        }
                    });
                }
                ui.separator();
                ui.horizontal(|ui| { ui.add(egui::TextEdit::singleline(&mut self.state.output).char_limit(2048).hint_text("New absolute .sp output path")); if ui.button("Choose output").clicked() { choose = true; } });
                ui.checkbox(&mut self.state.acknowledge, "I will validate this experimental candidate in the native application.");
                if ui.add_enabled(self.state.acknowledge && !self.state.output.is_empty(), egui::Button::new("Export candidate")).clicked() { action = Some(("write_candidate", json!({"output_path":self.state.output,"acknowledge_storage_only":true}))); }
                if snapshot.dirty { ui.checkbox(&mut self.state.discard, "Discard draft changes when closing"); }
                if ui.add_enabled(!snapshot.dirty || self.state.discard, egui::Button::new("Close draft")).clicked() { action = Some(("close", json!({"discard_changes":self.state.discard}))); }
            });
        });
        self.state.show = show;
        if choose {
            self.jobs.spawn("Choose candidate output", true, |_| {
                Ok(Done::Chosen(
                    rfd::FileDialog::new()
                        .add_filter("SP candidate", &["sp"])
                        .set_file_name("candidate.sp")
                        .save_file()
                        .map(|p| p.to_string_lossy().into_owned()),
                ))
            });
        } else if let Some((kind, extra)) = action {
            self.send(kind, extra);
        }
    }
}
impl SceneApp {
    pub(super) fn draft_active(&self) -> bool {
        self.draft.as_ref().is_some_and(DraftPanel::active)
    }
    pub(super) fn open_draft(&mut self) {
        if self.jobs.busy() {
            return;
        }
        if let Some(panel) = &mut self.draft {
            panel.state.show = true;
            return;
        }
        let Some(model) = &self.model else {
            return;
        };
        if !model.manifest.extension().is_some_and(|s| s.eq_ignore_ascii_case("sp")) {
            return;
        }
        self.draft = Some(DraftPanel::new(self.engine.clone(), model.manifest.to_string_lossy().into_owned(), model.scene.source_sha256.clone()));
        if let Some(ctx) = self.jobs.repaint_ctx() {
            ctx.request_repaint();
        }
    }
    pub(super) fn draft_window(&mut self, ctx: &egui::Context) {
        if let Some(panel) = &mut self.draft {
            panel.state.planning = self.state.planning.enabled;
            if ctx.input(|i| i.viewport().close_requested()) && panel.needs_close_confirmation() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                panel.state.show = true;
                panel.state.status = "Close or explicitly discard the draft before exiting.".into();
            }
            panel.show(ctx);
        }
    }
}

#[cfg(test)]
mod tests;
