//! Schedule/cost presentation over retained engine results; no parsing or source I/O.
use craft_anything_engine::tools::scene_project_tools::{Frame, ProjectModel};
use egui::Ui;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    #[default]
    Model,
    Schedule,
    Cost,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    pub mode: Mode,
    pub focus: i64,
    pub best_dates: bool,
    pub playing: bool,
    pub search: String,
    #[serde(default)]
    pub texture_preview: bool,
    #[serde(default)]
    pub flip_texture_v: bool,
    pub source_colors: bool,
    pub object_colors: bool,
    pub interval_preview: bool,
    #[serde(default)]
    pub profile_effects: bool,
}
pub(super) fn lookup_coverage(ui: &mut Ui, report: Option<&craft_anything_formats::sp_project::task_lookup::Report>) {
    use craft_anything_formats::sp_project::task_lookup::Status;
    let report = report.filter(|r| r.shape_valid());
    let text = match report {
        None => "Task lookup: unknown · Native rows unverified".to_owned(),
        Some(r) if r.status == Status::Valid => "Task lookup: consistent · Native rows unverified".to_owned(),
        Some(r) => {
            format!("Task lookup: inconsistent · Entries: {} · Collections: {} · Native rows unverified", r.mismatched_entries, r.mismatched_collections)
        }
    };
    let response = if report.is_some_and(|r| r.status == Status::Invalid) { ui.strong(text) } else { ui.weak(text) };
    response.on_hover_text("Lookup coverage checks stored task collection indices, separately from readable task bodies. Inconsistent indices can make the source application show duplicate or missing rows. Prepared bundles without checked source bytes remain unknown.");
}
pub fn wallclock(value: i64) -> String {
    let mut days = value.div_euclid(86400);
    let seconds = value.rem_euclid(86400);
    if !(-135_140..=376_199).contains(&days) {
        return value.to_string();
    }
    let mut year = 1970i64;
    while days < 0 {
        year -= 1;
        days += if leap(year) { 366 } else { 365 };
    }
    loop {
        let count = if leap(year) { 366 } else { 365 };
        if days < count {
            break;
        }
        days -= count;
        year += 1;
    }
    let months = [31, if leap(year) { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1;
    for count in months {
        if days < count {
            break;
        }
        days -= count;
        month += 1;
    }
    format!("{year:04}-{month:02}-{:02} {:02}:{:02}", days + 1, seconds / 3600, seconds % 3600 / 60)
}
fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
pub fn money(value: Option<i64>) -> String {
    value.map(|n| format!("{}{}.{:02}", if n < 0 { "-" } else { "" }, n.unsigned_abs() / 100, n.unsigned_abs() % 100)).unwrap_or_else(|| "Unresolved".into())
}
pub fn controls(ui: &mut Ui, state: &mut State, frame: Option<&Frame>) -> bool {
    controls_inner(ui, state, frame, true)
}
pub fn metadata_controls(ui: &mut Ui, state: &mut State, frame: Option<&Frame>) -> bool {
    if state.mode == Mode::Model {
        state.mode = Mode::Schedule;
    }
    controls_inner(ui, state, frame, false)
}
fn controls_inner(ui: &mut Ui, state: &mut State, frame: Option<&Frame>, geometry: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        if geometry {
            changed |= ui.selectable_value(&mut state.mode, Mode::Model, "3D").changed();
        }
        changed |= ui.selectable_value(&mut state.mode, Mode::Schedule, "4D").changed();
        changed |= ui.selectable_value(&mut state.mode, Mode::Cost, "5D").changed();
        if geometry
            && ui
                .checkbox(&mut state.source_colors, "Source diffuse (experimental)")
                .on_hover_text("Raw material preview differs from the checked explicit object colours.")
                .changed()
        {
            if state.source_colors {
                state.object_colors = false;
            }
            changed = true;
        }
        if geometry
            && ui
                .checkbox(&mut state.object_colors, "Object colours (partial)")
                .on_hover_text("Admitted group colour overrides and explicit leaf RGB; full material fidelity remains scoped.")
                .changed()
        {
            if state.object_colors {
                state.source_colors = false;
            }
            changed = true;
        }
    });
    if geometry {
        changed |= ui
            .checkbox(&mut state.texture_preview, "Source textures (experimental)")
            .on_hover_text(
                "Embedded image and UV candidates; sampling and native precedence are unverified. Disable Object colours to inspect raw image candidates.",
            )
            .changed();
        if state.texture_preview {
            changed |= ui.checkbox(&mut state.flip_texture_v, "Flip texture V").changed();
        }
    }
    if state.mode != Mode::Model {
        if ui.checkbox(&mut state.profile_effects, "Profile effects preview").changed() {
            if state.profile_effects {
                state.interval_preview = false;
            }
            changed = true;
        }
        if ui.checkbox(&mut state.interval_preview, "Interval preview (growth and transparency omitted)").changed() {
            if state.interval_preview {
                state.profile_effects = false;
            }
            changed = true;
        }
        if state.profile_effects {
            ui.small("Admitted profile RGB, opacity and growth directions; declared elapsed/Pause-calendar progress and resource-bound clipping. Native modifiers and cut caps remain scoped.");
        }
        if state.interval_preview {
            ui.small("Custom Install profiles use whole-object intervals. Growth, transparency and source material precedence are not reproduced.");
        }
        ui.horizontal(|ui| {
            if ui.button(if state.playing { "Pause timeline" } else { "Play timeline" }).clicked() {
                state.playing = !state.playing;
            }
            if ui.button("Previous day").clicked() {
                state.focus = state.focus.saturating_sub(86400);
                changed = true;
            }
            if ui.button("Next day").clicked() {
                state.focus = state.focus.saturating_add(86400);
                changed = true;
            }
            changed |= ui.checkbox(&mut state.best_dates, "Actual / best dates").changed();
            ui.label(wallclock(state.focus));
        });
        if let Some(frame) = frame {
            if let Some((start, end)) = frame.timeline.start.zip(frame.timeline.end) {
                let range = start.saturating_sub(86400)..=end.saturating_add(86400);
                changed |= ui.add(egui::Slider::new(&mut state.focus, range).show_value(false).text("Focus time")).changed();
            }
            ui.small(format!(
                "{} tasks with dates · {} unresolved · displayed wallclock, no timezone conversion",
                frame.timeline.resolved_tasks, frame.timeline.unresolved_tasks
            ));
            let unresolved = frame.resource_phases.iter().filter(|r| r.unresolved_reason.is_some()).count();
            ui.small(format!("{} resources have unresolved appearance; their geometry remains visible", unresolved));
        } else {
            ui.small("Preparing source schedule and costs…");
        }
    }
    changed
}
/// Returns a selected task identity for linking the table back to source assignments.
pub fn details(ui: &mut Ui, state: &mut State, frame: &Frame, model: &ProjectModel) -> Option<String> {
    let mut selected = None;
    if state.mode == Mode::Cost {
        let costs = &frame.costs;
        ui.horizontal_wrapped(|ui| {
            ui.strong(if costs.unresolved_direct_budget_lines == 0 {
                format!("Direct budget: {}", money(Some(costs.direct_budget_minor)))
            } else {
                format!("Direct budget unresolved; resolved subtotal {}", money(Some(costs.direct_budget_minor)))
            });
            ui.label(if costs.unresolved_resource_budget_lines == 0 {
                format!("Resource forecast: {}", money(Some(costs.resource_budget_minor)))
            } else {
                format!("Resource forecast unresolved; resolved subtotal {}", money(Some(costs.resource_budget_minor)))
            });
            ui.label(format!("Unresolved budget lines: {}", costs.unresolved_budget_lines));
            ui.label(if costs.unresolved_cumulative_lines == 0 {
                format!("Planned at focus: {}", money(Some(costs.planned_cumulative_minor)))
            } else {
                format!("Planned at focus unresolved ({} lines)", costs.unresolved_cumulative_lines)
            });
        });
        if let Some(expenses) = &costs.recorded_actual_expenses {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!("Recorded actual expenses: {}", money(expenses.recorded_total_minor)));
                ui.label(format!("At focus: {}", money(expenses.recorded_at_focus_minor)));
                ui.label(format!("{} reports / {} items", expenses.report_count, expenses.item_count));
            });
            if expenses.recorded_total_minor.is_none() {
                ui.small(format!(
                    "Resolved expense subtotal: {}; {} task collections unknown",
                    money(Some(expenses.partial_recorded_total_minor)),
                    expenses.unknown_task_collections
                ));
            }
            egui::CollapsingHeader::new("Recorded expense items").show(ui, |ui| {
                egui::ScrollArea::vertical().id_salt("expense_items").max_height(120.).show_rows(ui, 22., expenses.items.len(), |ui, range| {
                    for index in range {
                        if let Some(item) = expenses.items.get(index) {
                            ui.horizontal(|ui| {
                                if ui.selectable_label(false, &item.name).clicked() {
                                    selected = Some(item.task_guid.clone());
                                }
                                ui.label(money(item.amount_minor));
                                ui.label(item.date_wallclock.map(wallclock).unwrap_or_else(|| "Unresolved date".into()));
                            });
                        }
                    }
                });
                for reason in &expenses.limitations {
                    ui.small(reason);
                }
            });
        }
        ui.small("Amounts retain source currency context. Recorded expenses remain separate from budgets and resource forecasts.");
        ui.add(egui::TextEdit::singleline(&mut state.search).hint_text("Find a task or resource"));
        let search = state.search.to_lowercase();
        let rows = costs
            .lines
            .iter()
            .filter(|r| search.is_empty() || r.task_name.to_lowercase().contains(&search) || r.resource_name.to_lowercase().contains(&search))
            .collect::<Vec<_>>();
        let table_height = (ui.available_height() * 0.38).clamp(72., 210.);
        egui::ScrollArea::both().id_salt("cost_rows").max_height(table_height).show_rows(ui, 72., rows.len(), |ui, range| {
            for row in range {
                if let Some(line) = rows.get(row) {
                    ui.vertical(|ui| {
                        if ui.selectable_label(false, line.task_name.chars().take(60).collect::<String>()).on_hover_text(&line.task_name).clicked() {
                            selected = Some(line.task_guid.clone());
                        }
                        if line.resource_guid.is_none() {
                            ui.label(format!("Direct budget {} / planned at focus {}", money(line.budget_minor), money(line.planned_cumulative_minor)))
                                .on_hover_text(line.unresolved_reasons.join("\n"));
                            return;
                        }
                        ui.label(format!(
                            "Budget {} · actual {} · remaining {} · planned at focus {}",
                            money(line.budget_minor),
                            money(line.actual_minor),
                            money(line.remaining_minor),
                            money(line.planned_cumulative_minor)
                        ))
                        .on_hover_text(line.unresolved_reasons.join("\n"));
                        let units = |value: Option<f64>| value.map(|n| format!("{n:.3}")).unwrap_or_else(|| "Unresolved".into());
                        let resource = line.resource_guid.as_ref().and_then(|id| model.resources.iter().find(|r| r.guid == *id));
                        let label = match (resource.and_then(|r| r.unit_storage_byte), resource.and_then(|r| r.kind_storage_byte)) {
                            (Some(0), Some(1..=3)) => "normalized nominal-day storage".into(),
                            (Some(7), _) => "Each".into(),
                            (Some(0), _) => "unspecified source unit".into(),
                            (Some(code), _) => format!("source unit code {code}"),
                            (None, _) => "unresolved source unit".into(),
                        };
                        ui.label(format!(
                            "Quantity: {} / {} / {} {label} (planned / actual / remaining)",
                            units(line.planned_units),
                            units(line.actual_units),
                            units(line.remaining_units)
                        ));
                    });
                }
            }
        });
    } else if state.mode == Mode::Schedule {
        egui::CollapsingHeader::new("Task schedule").show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("schedule_rows").max_height(170.).show_rows(ui, 24., model.tasks.len(), |ui, range| {
                for row in range {
                    if let Some(task) = model.tasks.get(row) {
                        let (start, end) = if state.best_dates { (task.best_start, task.best_finish) } else { (task.planned_start, task.planned_finish) };
                        let dates = start.zip(end).map(|(a, b)| format!("{} → {}", wallclock(a), wallclock(b))).unwrap_or_else(|| "Unresolved dates".into());
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, task.name.chars().take(55).collect::<String>()).on_hover_text(&task.name).clicked() {
                                selected = Some(task.guid.clone());
                            }
                            ui.label(dates);
                        });
                    }
                }
            });
        });
    }
    let phase_limits = frame
        .resource_phases
        .iter()
        .filter_map(|r| r.unresolved_reason.as_ref())
        .chain(frame.resource_phases.iter().flat_map(|r| &r.unresolved_profile_effects));
    let limits = frame.unresolved_reasons.iter().chain(phase_limits).map(String::as_str).collect::<std::collections::BTreeSet<_>>();
    if !limits.is_empty() {
        egui::CollapsingHeader::new(format!("{} interpretation limits", limits.len())).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("project_limits").max_height(100.).show(ui, |ui| {
                for reason in limits.iter().take(30) {
                    ui.small(*reason);
                }
            });
        });
    }
    selected
}

pub use craft_anything_engine::tools::scene_presentation_tools::phase_state;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wallclocks_do_not_apply_timezone_or_shift_epoch() {
        assert_eq!(wallclock(0), "1970-01-01 00:00");
        assert_eq!(wallclock(-1), "1969-12-31 23:59");
        assert_eq!(wallclock(1709164800), "2024-02-29 00:00");
    }
    #[test]
    fn absent_money_is_not_zero() {
        assert_eq!(money(None), "Unresolved");
        assert_eq!(money(Some(12501)), "125.01");
        assert_eq!(money(Some(-1)), "-0.01");
        assert_eq!(money(Some(i64::MIN)), "-92233720368547758.08");
    }
}
