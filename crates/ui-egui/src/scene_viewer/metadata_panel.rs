//! Metadata-only schedule and cost client; all source reads run through engine tools.
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
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.jobs.set_repaint(ui.ctx());
        self.poll();
        ui.heading("Schedule and costs only");
        ui.label("Geometry is not inspected. This view has no 3D model.");
        ui.label(&self.path);
        if let Some(sha) = &self.source_sha256 {
            ui.small(format!("Source SHA-256: {sha}"));
        }
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
    }
}
impl Drop for MetadataPanel {
    fn drop(&mut self) {
        for job in self.jobs.running() {
            job.cancel.cancel();
        }
    }
}
