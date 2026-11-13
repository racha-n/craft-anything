//! One shared admission path for construction and read-only form hints.
use super::*;
use serde::Deserialize;

pub(super) struct Prepared {
    pub source_hash: String,
    pub data: Vec<u8>,
    pub calendars: Vec<u8>,
    pub evidence: Evidence,
    pub state: String,
    pub source_scope: SourceScope,
}
pub(super) fn prepare(raw: &[u8], expected: &str, identities: Option<(&[u8], &[u8])>, callback: &mut dyn FnMut(f32) -> bool) -> Result<Prepared> {
    let (source_hash, source) = relocation::source(raw, expected, callback)?;
    if let Some((task, supply)) = identities {
        absent(raw, task, supply, callback)?;
        for data in source.decoded.values() {
            absent(data, task, supply, callback)?;
        }
    }
    let (state, state_data) = planned_state(raw, &source, callback)?;
    let retained_bytes = source.decoded.values().try_fold(state_data.len(), |total, data| checked_add(total, data.len()))?;
    if retained_bytes > TOTAL_LIMIT {
        return Err(fail("limit", "Constructor retained sections exceed the 64 MiB budget"));
    }
    if let Some((task, supply)) = identities {
        absent(&state_data, task, supply, callback)?;
    }
    let data = source.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
    let calendars = source.decoded.get("Calendars").ok_or_else(|| fail("missing_dependency", "Calendars section missing"))?.clone();
    let evidence = inspect_container(source, source_hash.clone(), false, callback)?;
    if evidence.tasks.is_empty() {
        return Err(fail("unsupported_layout", "Choose a source with an existing Work leaf"));
    }
    if evidence.tasks.len() >= 512 {
        return Err(fail("limit", "Task insertion supports at most 512 output tasks"));
    }
    let source_scope = scope(&evidence.tasks)?;
    if !relocation::all_lookups_valid(&data, &evidence.tasks, callback)? {
        return Err(fail("unsupported_layout", "Source collection lookups are inconsistent"));
    }
    hierarchy::hints(&data, &evidence.tasks, callback)?;
    source_supply_identities(&data, &evidence.tasks, callback)?;
    if source_scope == SourceScope::FlatPlannedSingleAssignments {
        let calendar = &evidence.tasks.first().ok_or_else(|| fail("invalid_identity", "Task source is empty"))?.calendar_guid_storage;
        for task in &evidence.tasks {
            profile(&data, task, &evidence.tasks, callback)?;
            if &task.calendar_guid_storage != calendar || guid_at(&data, checked_add(task.display_id_end, 9)?)? != state {
                return Err(fail("unsupported_layout", "Assigned roots must share the admitted default calendar and Planned state definition"));
            }
        }
    }
    Ok(Prepared { source_hash, data, calendars, evidence, state, source_scope })
}
impl Prepared {
    pub(super) fn admit_pair(&self, template: &Task, anchor: &Task, callback: &mut dyn FnMut(f32) -> bool) -> Result<()> {
        for task in [template, anchor] {
            profile(&self.data, task, &self.evidence.tasks, callback)?;
            if guid_at(&self.data, checked_add(task.display_id_end, 9)?)? != self.state {
                return Err(fail("missing_dependency", "Default Work body does not reference the observed Planned task-state definition"));
            }
        }
        calendar(&self.calendars, template, anchor)
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Anchor {
    pub anchor_task_guid: String,
    pub template_task_guid: String,
    pub expected_parent_guid: Option<String>,
    pub expected_sibling_index: usize,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Unavailable {
    pub task_guid: String,
    pub code: String,
    pub reason: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Availability {
    pub version: u32,
    pub source_sha256: String,
    pub source_scope: Option<SourceScope>,
    pub anchors: Vec<Anchor>,
    pub unavailable: Vec<Unavailable>,
    pub reason: Option<String>,
    pub suggested_display_id: String,
}
/// Report self-template anchors using the same source/profile/calendar guards as writing.
pub fn availability(raw: &[u8], expected: &str, mut keep: impl FnMut(f32) -> bool) -> std::result::Result<Availability, FormatError> {
    if raw.len() > FILE_LIMIT || expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format_error(fail("invalid_identity", "Choose a source up to 256 MiB and its 64-digit SHA-256")));
    }
    let p = match prepare(raw, expected, None, &mut keep) {
        Ok(p) => p,
        Err(e) if e.code == "cancelled" || e.code == "stale_source" => return Err(format_error(e)),
        Err(e) => {
            return Ok(Availability {
                version: 1,
                source_sha256: expected.to_ascii_lowercase(),
                source_scope: None,
                anchors: Vec::new(),
                unavailable: Vec::new(),
                reason: Some(e.message),
                suggested_display_id: String::new(),
            });
        }
    };
    let mut anchors = Vec::new();
    let mut unavailable = Vec::new();
    let mut positions = BTreeMap::new();
    for task in &p.evidence.tasks {
        check(&mut keep, 0.8).map_err(format_error)?;
        let index = positions.entry(task.parent_guid.as_deref()).or_insert(0usize);
        match p.admit_pair(task, task, &mut keep) {
            Ok(()) => anchors.push(Anchor {
                anchor_task_guid: task.guid.clone(),
                template_task_guid: task.guid.clone(),
                expected_parent_guid: task.parent_guid.clone(),
                expected_sibling_index: *index,
            }),
            Err(e) if e.code == "cancelled" => return Err(format_error(e)),
            Err(e) => unavailable.push(Unavailable { task_guid: task.guid.clone(), code: e.code.to_owned(), reason: e.message }),
        }
        *index = checked_add(*index, 1).map_err(format_error)?;
    }
    let suggested_display_id = (1..=513)
        .map(|n| format!("WORK-{n}"))
        .find(|id| !p.evidence.tasks.iter().any(|t| t.display_id.eq_ignore_ascii_case(id)))
        .ok_or_else(|| format_error(fail("limit", "Unique display ID suggestion exhausted")))?;
    let reason = anchors.is_empty().then(|| "No Work leaf has the admitted default body, Planned state and working start/calendar.".to_owned());
    Ok(Availability { version: 1, source_sha256: p.source_hash, source_scope: Some(p.source_scope), anchors, unavailable, reason, suggested_display_id })
}
