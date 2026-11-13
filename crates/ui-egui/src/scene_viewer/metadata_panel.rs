//! Metadata-only schedule and cost client; all source reads run through engine tools.
use super::draft_panel::DraftSource;
use super::project_panel::{self, Mode, State};
use crate::jobs::Jobs;
use craft_anything_engine::{
    CallOptions, Caller, Engine,
    tools::scene_project_tools::{AppearancePolicy, DateMode, LatestMetadataFrame},
};
use serde_json::json;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq, Eq)]
struct Key {
    path: String,
    focus: i64,
    best: bool,
    preview: bool,
    effects: bool,
}
struct Done {
    generation: u64,
    key: Key,
    result: Result<Arc<LatestMetadataFrame>, String>,
}

enum ReportDone {
    Path { generation: u64, path: Option<String> },
    Export { generation: u64, key: Key, source_sha256: String, output: String },
}
/// A separate workflow for projects whose drawable geometry is not inspected.
pub struct MetadataPanel {
    engine: Arc<Engine>,
    path: String,
    state: State,
    jobs: Jobs<Done>,
    generation: u64,
    active_job: Option<u64>,
    pending: bool,
    initialize_focus: bool,
    latest: Option<Arc<LatestMetadataFrame>>,
    source_sha256: Option<String>,
    timeline: Option<(i64, i64)>,
    error: Option<String>,
    selected_task: Option<String>,
    last_tick: Instant,
    report_jobs: Jobs<ReportDone>,
    report_job: Option<u64>,
    report_dir: String,
    report_template: super::report_templates::Choice,
    report_status: Option<String>,
}
impl MetadataPanel {
    pub fn new(engine: Arc<Engine>, path: String) -> Self {
        Self {
            engine,
            path,
            state: State { mode: Mode::Schedule, ..Default::default() },
            jobs: Jobs::default(),
            generation: 0,
            active_job: None,
            pending: true,
            initialize_focus: true,
            latest: None,
            source_sha256: None,
            timeline: None,
            error: None,
            selected_task: None,
            last_tick: Instant::now(),
            report_jobs: Jobs::default(),
            report_job: None,
            report_dir: String::new(),
            report_template: super::report_templates::Choice::default(),
            report_status: None,
        }
    }
    pub fn request_open(&mut self, path: String) {
        self.path = path;
        self.state = State { mode: Mode::Schedule, ..Default::default() };
        self.initialize_focus = true;
        self.source_sha256 = None;
        self.timeline = None;
        self.selected_task = None;
        self.invalidate();
    }
    fn key(&self) -> Key {
        Key {
            path: self.path.clone(),
            focus: self.state.focus,
            best: self.state.best_dates,
            preview: self.state.interval_preview,
            effects: self.state.profile_effects,
        }
    }
    fn invalidate(&mut self) {
        for job in self.report_jobs.running() {
            job.cancel.cancel();
        }
        self.report_status = None;
        self.report_job = None;
        for job in self.jobs.running() {
            job.cancel.cancel();
        }
        self.generation = self.generation.saturating_add(1);
        self.active_job = None;
        self.latest = None;
        self.error = None;
        self.pending = true;
    }
    fn cancel(&mut self) {
        self.invalidate();
        self.pending = false;
        self.state.playing = false;
        self.error = Some("Schedule and cost evaluation cancelled.".into());
    }
    fn poll(&mut self) {
        for finished in self.jobs.poll() {
            if self.active_job != Some(finished.job) {
                continue;
            }
            self.active_job = None;
            match finished.result {
                Ok(done) if done.generation == self.generation && done.key == self.key() => match done.result {
                    Ok(latest) => {
                        self.source_sha256 = Some(latest.source_sha256.clone());
                        self.timeline = latest.frame.timeline.start.zip(latest.frame.timeline.end);
                        if self.initialize_focus {
                            self.initialize_focus = false;
                            if let Some(start) = latest.frame.timeline.start
                                && start != self.state.focus
                            {
                                self.state.focus = start;
                                self.invalidate();
                                continue;
                            }
                        }
                        self.latest = Some(latest);
                        self.error = None;
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.state.playing = false;
                    }
                },
                Ok(_) => {}
                Err(error) => {
                    self.error = Some(error);
                    self.state.playing = false;
                }
            }
        }
    }
    fn evaluate(&mut self) {
        if !self.pending || self.jobs.busy() {
            return;
        }
        self.pending = false;
        self.latest = None;
        let key = self.key();
        let generation = self.generation;
        let engine = self.engine.clone();
        self.active_job = Some(self.jobs.spawn("Read schedule and costs", true, move |job| {
            let result = (|| {
                let reporter = job.reporter();
                let output = engine
                    .call(
                        "formats_project_frame_evaluate",
                        json!({"path":key.path,
                    "focus_wallclock":key.focus,"date_mode":if key.best {"best"} else {"planned"},
                    "interval_preview":key.preview,"profile_effects":key.effects,"limit":1}),
                        CallOptions::new(Caller::Ui).with_cancel(job.cancel.clone()).with_progress(&reporter),
                    )
                    .map_err(|error| error.for_people())?
                    .to_json();
                if job.cancel.is_cancelled() {
                    return Err("Schedule and cost evaluation cancelled.".into());
                }
                let sha = output
                    .get("result")
                    .and_then(|value| value.get("source"))
                    .and_then(|value| value.get("source_sha256"))
                    .and_then(|value| value.as_str())
                    .ok_or("Source identity is unavailable.")?;
                let latest = engine.session().ext.get::<LatestMetadataFrame>().ok_or("The metadata frame is unavailable.")?;
                let dates = if key.best { DateMode::Best } else { DateMode::Planned };
                let appearance = if key.effects {
                    AppearancePolicy::ProfileEffectsPreview
                } else if key.preview {
                    AppearancePolicy::IntervalPreview
                } else {
                    AppearancePolicy::Strict
                };
                if latest.source_sha256 != sha
                    || latest.frame.focus != key.focus
                    || latest.frame.date_mode != dates
                    || latest.frame.appearance_policy != appearance
                {
                    return Err("The source snapshot changed during evaluation. Retry this project.".into());
                }
                Ok(latest)
            })();
            Ok(Done { generation, key, result })
        }));
    }
    fn reports(&mut self, ui: &mut egui::Ui) {
        self.report_jobs.set_repaint(ui.ctx());
        for done in self.report_jobs.poll() {
            if self.report_job != Some(done.job) {
                continue;
            }
            self.report_job = None;
            match done.result {
                Ok(ReportDone::Path { generation, path }) if generation == self.generation => {
                    if let Some(path) = path {
                        self.report_dir = path;
                    }
                }
                Ok(ReportDone::Export { generation, key, source_sha256, output })
                    if generation == self.generation && key == self.key() && self.source_sha256.as_deref() == Some(&source_sha256) =>
                {
                    self.report_status = Some(format!("CSV reports exported: {output}"));
                }
                Ok(_) => {}
                Err(error) => self.report_status = Some(error),
            }
        }
        ui.separator();
        ui.label("Export metadata-only CSV reports");
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut self.report_dir);
            crate::widgets::named(response, "Metadata report folder");
            if ui.add_enabled(!self.report_jobs.busy(), egui::Button::new("Browse CSV folder")).clicked() {
                let generation = self.generation;
                self.report_job = Some(self.report_jobs.spawn("Choose metadata report folder", true, move |_| {
                    let path = rfd::FileDialog::new().set_file_name("metadata-report").save_file();
                    Ok(ReportDone::Path { generation, path: path.map(|p| p.to_string_lossy().into_owned()) })
                }));
            }
        });
        let template_valid = self.report_template.show(ui, !self.report_jobs.busy());
        let ready = template_valid && self.latest.is_some() && !self.pending && !self.jobs.busy() && !self.report_jobs.busy() && !self.report_dir.is_empty();
        if ui.add_enabled(ready, egui::Button::new("Export metadata CSV")).clicked() {
            self.state.playing = false;
            self.report_status = None;
            let generation = self.generation;
            let key = self.key();
            let output = self.report_dir.clone();
            let template = self.report_template.clone();
            if let Some(source_sha256) = self.source_sha256.clone() {
                let engine = self.engine.clone();
                self.report_job = Some(self.report_jobs.spawn("Export metadata-only reports", true, move |job| {
                    let reporter = job.reporter();
                    let mut params = json!({"path":key.path,"output_dir":output,"focus":key.focus,"expected_source_sha256":source_sha256,"best_dates":key.best,"interval_preview":key.preview,"profile_effects":key.effects});
                    template.apply(&mut params);
                    engine.call("formats_project_file_report_export", params, CallOptions::new(Caller::Ui).with_cancel(job.cancel.clone()).with_progress(&reporter)).map_err(|e|e.for_people())?;
                    Ok(ReportDone::Export { generation, key, source_sha256, output })
                }));
            }
        }
        if self.report_jobs.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Exporting metadata CSV...");
                if ui.button("Cancel CSV export").clicked() {
                    for job in self.report_jobs.running() {
                        job.cancel.cancel();
                    }
                }
            });
        }
        if let Some(status) = &self.report_status {
            ui.label(status);
        }
        ui.small("Choose a new folder name. Reports use the displayed source hash and focus; geometry is not inspected. A folder without manifest.json is incomplete.");
    }
    pub(super) fn draft_source(&self) -> Option<DraftSource> {
        if self.pending || self.jobs.busy() || self.error.is_some() {
            return None;
        }
        let latest = self.latest.as_ref()?;
        if self.source_sha256.as_deref() != Some(latest.source_sha256.as_str()) {
            return None;
        }
        Some(DraftSource { path: self.path.clone(), source_sha256: latest.source_sha256.clone() })
    }
    pub(super) fn show(&mut self, ui: &mut egui::Ui) -> Option<DraftSource> {
        self.jobs.set_repaint(ui.ctx());
        self.poll();
        let mut draft = None;
        ui.heading("Schedule and costs only");
        ui.label("Geometry is not inspected. This view has no 3D model.");
        ui.label(&self.path);
        if let Some(sha) = &self.source_sha256 {
            ui.small(format!("Source SHA-256: {sha}"));
        }
        if let Some(latest) = &self.latest {
            project_panel::lookup_coverage(ui, latest.task_collection_lookup.as_ref());
        }
        let source = self.draft_source();
        if ui
            .add_enabled(source.is_some(), egui::Button::new("Project draft"))
            .on_hover_text("Edit this source in an isolated draft and export a separate candidate.")
            .clicked()
        {
            self.state.playing = false;
            draft = source;
        }
        self.reports(ui);
        let key = self.key();
        project_panel::metadata_controls(ui, &mut self.state, self.latest.as_ref().map(|latest| latest.frame.as_ref()));
        if self.key() != key {
            self.invalidate();
        }
        if self.state.playing {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            if !self.jobs.busy() && !self.pending && self.error.is_none() && self.last_tick.elapsed() >= Duration::from_millis(100) {
                self.last_tick = Instant::now();
                if let Some((_, end)) = self.timeline {
                    let stop = end.saturating_add(86400);
                    self.state.focus = self.state.focus.saturating_add(43200).min(stop);
                    if self.state.focus >= stop {
                        self.state.playing = false;
                    }
                    self.invalidate();
                } else {
                    self.state.playing = false;
                }
            }
        } else {
            self.last_tick = Instant::now();
        }
        if self.jobs.busy() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("Reading schedule and costs...");
                if ui.button("Cancel").clicked() {
                    self.cancel();
                }
            });
        }
        if let Some(error) = self.error.clone() {
            ui.label(error);
            if ui.button("Retry schedule and costs").clicked() {
                self.invalidate();
            }
        }
        if let Some(latest) = &self.latest
            && let Some(guid) = project_panel::details(ui, &mut self.state, &latest.frame, &latest.model)
        {
            self.selected_task = Some(guid);
        }
        if let Some(guid) = &self.selected_task {
            ui.small(format!("Selected source task GUID: {guid}"));
        }
        self.evaluate();
        draft
    }
}
impl Drop for MetadataPanel {
    fn drop(&mut self) {
        for job in self.report_jobs.running() {
            job.cancel.cancel();
        }
        for job in self.jobs.running() {
            job.cancel.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_metadata_source_discards_a_delayed_report_error() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(Engine::new(craft_anything_engine::Session::isolated(dir.path())));
        let mut panel = MetadataPanel::new(engine, "first.sp".into());
        let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker = release.clone();
        panel.report_job = Some(panel.report_jobs.spawn("Delayed test report", true, move |_| {
            let started = Instant::now();
            while !worker.load(std::sync::atomic::Ordering::SeqCst) && started.elapsed() < Duration::from_secs(2) {
                std::thread::sleep(Duration::from_millis(1));
            }
            Err("Failure from previous source".into())
        }));
        panel.request_open("second.sp".into());
        release.store(true, std::sync::atomic::Ordering::SeqCst);
        let ctx = egui::Context::default();
        let started = Instant::now();
        loop {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| panel.reports(ui));
            output.textures_delta.clear();
            if !panel.report_jobs.busy() {
                break;
            }
            assert!(started.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(panel.report_status.is_none());
    }
}
