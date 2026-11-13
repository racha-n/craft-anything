//! Bounded declared-model FS earliest dates. This is not native rescheduling or CPM.
use craft_anything_formats::project_calendar::{self, Calendars, WorkAllocation, WorkingBudget};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const TASK_LIMIT: usize = 512;
const EDGE_LIMIT: usize = 1024;
const WORK_LIMIT: u64 = 1_000_000;
const TICK_LIMIT: usize = 200_000;

#[derive(Deserialize)]
struct Link {
    predecessor_guid: String,
    type_storage_window: u32,
    lag_storage_hex: String,
}
#[derive(Deserialize)]
struct Task {
    guid: String,
    name: String,
    parent_guid: Option<String>,
    calendar_guid_storage: String,
    planned_start_wallclock_scalar: u64,
    duration_storage_scalar: u64,
    status_storage_value: u32,
    #[serde(default)]
    links: Vec<Link>,
}
#[derive(Debug, Serialize)]
pub struct Dates {
    pub guid: String,
    pub name: String,
    pub original_start_wallclock: Option<i64>,
    pub proposed_start_wallclock: i64,
    pub proposed_finish_wallclock: i64,
    pub duration_working_seconds: u64,
    pub effective_calendar_guid: String,
}
#[derive(Debug, Serialize)]
pub struct Preview {
    pub version: u32,
    pub anchor_wallclock: i64,
    pub task_count: usize,
    pub edge_count: usize,
    pub tasks: Vec<Dates>,
    pub declared_unconstrained_fixed_work: bool,
    pub native_parity_verified: bool,
    pub critical_path_calculated: bool,
    pub limitations: Vec<&'static str>,
}

pub fn from_metadata(metadata: &Value, anchor: i64, declared_unconstrained_fixed_work: bool, mut cancelled: impl FnMut() -> bool) -> Result<Preview, String> {
    if cancelled() {
        return Err("Schedule preview cancelled".into());
    }
    if metadata.get("schema_version").and_then(Value::as_u64) != Some(4) {
        return Err("Schedule preview requires project metadata schema 4".into());
    }
    let values = metadata.get("tasks").and_then(Value::as_array).ok_or("Task metadata is absent")?;
    if values.len() > TASK_LIMIT {
        return Err("Schedule preview supports at most 512 tasks".into());
    }
    let mut tasks = Vec::new();
    let mut edges = 0usize;
    for value in values {
        if cancelled() {
            return Err("Schedule preview cancelled".into());
        }
        let count = value.get("links").and_then(Value::as_array).map_or(0, Vec::len);
        edges = edges.checked_add(count).ok_or("Schedule edge count overflow")?;
        if edges > EDGE_LIMIT {
            return Err("Schedule preview supports at most 1024 links".into());
        }
        tasks.push(serde_json::from_value::<Task>(value.clone()).map_err(|e| format!("Incomplete task metadata: {e}"))?);
    }
    let hex = metadata.pointer("/raw_sections/Calendars/data_hex").and_then(Value::as_str).ok_or("Source calendars are absent")?;
    let bytes = crate::project_model::decode_hex(hex, project_calendar::MAX_BYTES)?;
    let calendars = project_calendar::parse(&bytes).map_err(|e| e.to_string())?;
    calculate(&calendars, &tasks, anchor, declared_unconstrained_fixed_work, &mut cancelled)
}

fn allocation(
    calendars: &Calendars,
    calendar: Option<&str>,
    start: i64,
    duration: u64,
    ticks: &mut usize,
    cancelled: &mut dyn FnMut() -> bool,
) -> Result<WorkAllocation, String> {
    let mut exhausted = false;
    let result = calendars.working_segments_with_budget(calendar, start, duration, WorkingBudget { max_scanned_days: 4000, max_segments: 8192 }, || {
        *ticks = ticks.saturating_add(1);
        if *ticks > TICK_LIMIT {
            exhausted = true;
            true
        } else {
            cancelled()
        }
    });
    if exhausted {
        return Err("Schedule preview exhausted its working-time budget".into());
    }
    let work = result.map_err(|e| format!("Working-time preview unresolved: {e}"))?;
    if work.finish_wallclock >= project_calendar::UNSPECIFIED_WALLCLOCK {
        return Err("Schedule preview reaches the unset-date sentinel".into());
    }
    Ok(work)
}

fn calculate(calendars: &Calendars, tasks: &[Task], anchor: i64, declared: bool, cancelled: &mut dyn FnMut() -> bool) -> Result<Preview, String> {
    if !declared {
        return Err("Explicitly declare unconstrained fixed-work tasks; source task kinds and constraints are not certified here".into());
    }
    if !(0..project_calendar::UNSPECIFIED_WALLCLOCK).contains(&anchor) || anchor % 60 != 0 {
        return Err("Choose an explicit minute-aligned naive wall-clock anchor before the unset-date sentinel".into());
    }
    if tasks.len() > TASK_LIMIT || calendars.records.len() > 64 {
        return Err("Schedule preview supports at most 512 tasks and 64 calendars".into());
    }
    let items = calendars
        .records
        .iter()
        .try_fold(0usize, |n, c| n.checked_add(c.day_types.len()).and_then(|n| n.checked_add(c.exceptions.len())))
        .ok_or("Calendar item count overflow")?;
    if items > 4096 {
        return Err("Schedule preview supports at most 4096 calendar day types and exceptions".into());
    }
    let mut nodes = BTreeMap::new();
    let mut indegree = BTreeMap::new();
    let mut successors: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut incoming: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
    let mut edge_count = 0usize;
    for task in tasks {
        if cancelled() {
            return Err("Schedule preview cancelled".into());
        }
        if task.guid.is_empty() || task.guid.len() > 128 || nodes.insert(task.guid.to_ascii_lowercase(), task).is_some() {
            return Err("Schedule task identities must be bounded, present and unique".into());
        }
        if task.parent_guid.is_some()
            || task.status_storage_value != 1
            || task.duration_storage_scalar == 0
            || task.duration_storage_scalar > WORK_LIMIT
            || task.duration_storage_scalar % 60 != 0
        {
            return Err("Only Planned leaf tasks with positive, minute-aligned durations up to one million working seconds are supported".into());
        }
        indegree.insert(task.guid.to_ascii_lowercase(), task.links.len());
        edge_count = edge_count.checked_add(task.links.len()).ok_or("Schedule edge count overflow")?;
        if edge_count > EDGE_LIMIT {
            return Err("Schedule preview supports at most 1024 links".into());
        }
    }
    for task in tasks {
        let id = task.guid.to_ascii_lowercase();
        for link in &task.links {
            if link.type_storage_window != 1 {
                return Err("Only observed FS storage type 1 is supported in this preview".into());
            }
            let raw = crate::project_model::decode_hex(&link.lag_storage_hex, 8)?;
            let bytes: [u8; 8] = raw.try_into().map_err(|_| "A link lag must contain exactly eight bytes")?;
            let lag = u64::try_from(i64::from_le_bytes(bytes)).map_err(|_| "Negative-lag scheduling remains unsupported")?;
            if lag > WORK_LIMIT || lag % 60 != 0 {
                return Err("Link lags must be minute-aligned and at most one million working seconds".into());
            }
            let predecessor = link.predecessor_guid.to_ascii_lowercase();
            if predecessor == id || !nodes.contains_key(&predecessor) {
                return Err("A link references itself or an absent predecessor".into());
            }
            successors.entry(predecessor.clone()).or_default().push(id.clone());
            incoming.entry(id.clone()).or_default().push((predecessor, lag));
        }
    }
    let mut ready: BTreeSet<String> = indegree.iter().filter(|(_, n)| **n == 0).map(|(id, _)| id.clone()).collect();
    let mut completed: BTreeMap<String, Dates> = BTreeMap::new();
    let mut ticks = 0usize;
    while let Some(id) = ready.pop_first() {
        if cancelled() {
            return Err("Schedule preview cancelled".into());
        }
        let task = nodes.get(&id).ok_or("Schedule node disappeared")?;
        let mut earliest = anchor;
        if let Some(links) = incoming.get(&id) {
            for (pred_id, lag) in links {
                let predecessor = completed.get(pred_id).ok_or("Predecessor was not scheduled")?;
                let bound = if *lag == 0 {
                    predecessor.proposed_finish_wallclock
                } else {
                    allocation(calendars, Some(&predecessor.effective_calendar_guid), predecessor.proposed_finish_wallclock, *lag, &mut ticks, cancelled)?
                        .finish_wallclock
                };
                earliest = earliest.max(bound);
            }
        }
        let calendar = (task.calendar_guid_storage != "00000000-0000-0000-0000-000000000000").then_some(task.calendar_guid_storage.as_str());
        let work = allocation(calendars, calendar, earliest, task.duration_storage_scalar, &mut ticks, cancelled)?;
        let start = work.first_work_start_wallclock.ok_or("A positive-duration task has no working interval")?;
        if start % 60 != 0 || work.finish_wallclock % 60 != 0 {
            return Err("Sub-minute calendar boundaries require an unverified snapping policy".into());
        }
        completed.insert(
            id.clone(),
            Dates {
                guid: task.guid.clone(),
                name: task.name.chars().take(256).collect(),
                original_start_wallclock: i64::try_from(task.planned_start_wallclock_scalar).ok().filter(|v| *v < project_calendar::UNSPECIFIED_WALLCLOCK),
                proposed_start_wallclock: start,
                proposed_finish_wallclock: work.finish_wallclock,
                duration_working_seconds: task.duration_storage_scalar,
                effective_calendar_guid: work.calendar_guid,
            },
        );
        if let Some(children) = successors.get(&id) {
            for child in children {
                let count = indegree.get_mut(child).ok_or("Schedule indegree disappeared")?;
                *count = count.checked_sub(1).ok_or("Schedule indegree underflow")?;
                if *count == 0 {
                    ready.insert(child.clone());
                }
            }
        }
    }
    if completed.len() != tasks.len() {
        return Err("The dependency graph contains a cycle; no partial schedule was returned".into());
    }
    let mut dates = Vec::new();
    for task in tasks {
        dates.push(completed.remove(&task.guid.to_ascii_lowercase()).ok_or("A scheduled task disappeared")?);
    }
    Ok(Preview {
        version: 1,
        anchor_wallclock: anchor,
        task_count: tasks.len(),
        edge_count,
        tasks: dates,
        declared_unconstrained_fixed_work: true,
        native_parity_verified: false,
        critical_path_calculated: false,
        limitations: vec![
            "Declared unconstrained fixed-work model only; source task kinds and constraints are not certified.",
            "Planned leaf tasks, FS links and nonnegative working-time lag only; upstream-task calendar policy and minute-aligned inputs.",
            "No source mutation, native rescheduling, utilization coordination, critical paths or baselines are performed.",
            "Calendar conventions retain the source reader's inferred evidence; localized date text and timezone/DST are not interpreted.",
        ],
    })
}

#[cfg(test)]
mod tests;
