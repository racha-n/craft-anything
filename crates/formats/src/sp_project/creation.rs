//! One observed default Work constructor; transactions and general native Save remain separate gates.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;

mod admission;
#[cfg(test)]
#[path = "creation_tests.rs"]
mod tests;
pub use admission::{Anchor, Availability, Unavailable, availability};

/// Shared default-body admission for the first independent deletion subset.
pub(super) fn unassigned_roots(raw: &[u8], expected: &str, callback: &mut dyn FnMut(f32) -> bool) -> Result<(String, Vec<u8>, Evidence)> {
    let p = admission::prepare(raw, expected, None, callback)?;
    if p.source_scope != SourceScope::PlannedUnassigned || p.evidence.tasks.len() < 2 || p.evidence.tasks.iter().any(|t| t.parent_guid.is_some()) {
        return Err(fail("unsupported_layout", "Deletion requires at least two unassigned Planned root leaves"));
    }
    for task in &p.evidence.tasks {
        p.admit_pair(task, task, callback)?;
    }
    Ok((p.source_hash, p.data, p.evidence))
}

const PROFILE: &str = "616828033f90c1a0c824a4ced3f41aae005e7f65765e6ca5af791686ad5a7642";
const DURATION: u64 = 28_800;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct InsertAbove {
    pub template_task_guid: String,
    pub anchor_task_guid: String,
    pub expected_parent_guid: Option<String>,
    pub expected_sibling_index: usize,
    pub task_guid: String,
    pub supply_chain_id: String,
    pub name: String,
    pub display_id: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceScope {
    PlannedUnassigned,
    FlatPlannedSingleAssignments,
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub insertion: InsertAbove,
    pub source_scope: SourceScope,
    pub template_assignments_omitted: usize,
    pub new_task_assignment_count: usize,
    pub default_profile_sha256: &'static str,
    pub planned_start_wallclock_scalar: u64,
    pub duration_storage_scalar: u64,
    pub task_count_before: usize,
    pub task_count_after: usize,
    pub sections: Vec<writer::SectionGuard>,
    pub internal_reparse_passed: bool,
    pub existing_task_bodies_preserved: bool,
    pub collection_lookup_indices_validated: bool,
    pub collection_lookup_indices_rebuilt: bool,
    pub task_state_definition_validated: bool,
    pub source_supply_chain_identities_validated: bool,
    pub native_transaction_history_reproduced: bool,
    pub native_reopen_verified: bool,
    pub native_row_identity_verified: bool,
    pub derived_schedule_recalculated: bool,
    pub geometry_validated: bool,
    pub constructor_completeness_verified: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}
fn append(out: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    if checked_add(out.len(), value.len())? > STREAM_LIMIT {
        return Err(fail("limit", "Task creation exceeds the 16 MiB Tasks budget"));
    }
    out.try_reserve(value.len()).map_err(|_| fail("limit", "Task creation allocation"))?;
    out.extend_from_slice(value);
    Ok(())
}
fn raw_guid(value: &str) -> Result<Vec<u8>> {
    if value.len() != 36 || [8, 13, 18, 23].into_iter().any(|at| value.as_bytes().get(at) != Some(&b'-')) {
        return Err(fail("invalid_identity", "Choose a nonzero UUID for each new identity"));
    }
    let mut raw = Vec::new();
    let mut high = None;
    for (at, ch) in value.bytes().enumerate() {
        if [8, 13, 18, 23].contains(&at) {
            continue;
        }
        let digit = char::from(ch).to_digit(16).ok_or_else(|| fail("invalid_identity", "UUID contains non-hexadecimal digits"))?;
        let digit = u8::try_from(digit).map_err(|_| fail("invalid_identity", "UUID digit"))?;
        if let Some(first) = high.take() {
            raw.push(first * 16 + digit);
        } else {
            high = Some(digit);
        }
    }
    if raw.len() != 16 || raw.iter().all(|b| *b == 0) || high.is_some() {
        return Err(fail("invalid_identity", "Choose a nonzero UUID for each new identity"));
    }
    for (at, count) in [(0, 4), (4, 2), (6, 2)] {
        raw.get_mut(at..checked_add(at, count)?).ok_or_else(|| fail("invalid_identity", "UUID byte order"))?.reverse();
    }
    Ok(raw)
}
fn absent(data: &[u8], task: &[u8], supply: &[u8], callback: &mut dyn FnMut(f32) -> bool) -> Result<()> {
    for at in (0..data.len()).step_by(65_536) {
        check(callback, 0.2)?;
        let count = data.len().saturating_sub(at).min(65_551);
        if bytes(data, at, count)?.windows(16).any(|w| w == task || w == supply) {
            return Err(fail("invalid_identity", "A new identity already occurs in the source"));
        }
    }
    Ok(())
}
fn profile(data: &[u8], task: &Task, all: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<()> {
    if all.iter().any(|t| t.parent_guid.as_deref() == Some(task.guid.as_str())) {
        return Err(fail("unsupported_layout", "Choose existing Work leaves for template and anchor"));
    }
    let assignment_bytes = task.assignments.len().checked_mul(235).ok_or_else(|| fail("limit", "Template assignment bytes"))?;
    if task.assignments.len() > 1
        || task.name_offset != checked_add(checked_add(task.offset, 73)?, assignment_bytes)?
        || task.display_id_offset != checked_add(task.name_end, 196)?
    {
        return Err(fail("unsupported_layout", "Default Work string framing differs from the observed profile"));
    }
    marker(data, checked_add(task.name_end, 16)?, &[0])?;
    marker(data, checked_add(task.name_end, 94)?, &[0xb3, 0x0c, 0x9c, 0xd1])?;
    let end = checked_add(task.offset, task.bytes)?.checked_sub(4).ok_or_else(|| fail("truncated", "Task body end"))?;
    let mut normal = Vec::new();
    append(&mut normal, bytes(data, task.offset, 12)?)?;
    append(&mut normal, &[0; 16])?;
    append(&mut normal, bytes(data, checked_add(task.offset, 28)?, 5)?)?;
    append(&mut normal, &[0; 4])?;
    let assignment_end = checked_add(checked_add(task.offset, 37)?, assignment_bytes)?;
    append(&mut normal, bytes(data, assignment_end, 20)?)?;
    append(&mut normal, &[0; 16])?;
    append(&mut normal, b"<NAME>")?;
    append(&mut normal, &[0; 16])?;
    append(&mut normal, bytes(data, checked_add(task.name_end, 16)?, 82)?)?;
    append(&mut normal, &[0; 16])?;
    append(&mut normal, bytes(data, checked_add(task.name_end, 114)?, 82)?)?;
    append(&mut normal, b"<ID>")?;
    append(&mut normal, bytes(data, task.display_id_end, 9)?)?;
    append(&mut normal, &[0; 16])?;
    let after_state = checked_add(task.display_id_end, 25)?;
    append(&mut normal, bytes(data, after_state, end.checked_sub(after_state).ok_or_else(|| fail("truncated", "Default task tail"))?)?)?;
    if writer::hash(&normal, callback)? != PROFILE {
        return Err(fail("unsupported_layout", "Default Work body contains fields outside the observed constructor profile"));
    }
    Ok(())
}
fn scope(all: &[Task]) -> Result<SourceScope> {
    let assigned = all.iter().any(|t| !t.assignments.is_empty());
    if all.iter().any(|t| {
        t.status_storage_value != 1
            || t.assignments.len() > 1
            || !t.links.is_empty()
            || !t.direct_costs.is_empty()
            || !t.progress_records.is_empty()
            || !t.actual_expense_reports.is_empty()
            || !t.activity_code_refs.is_empty()
            || !t.user_values.is_empty()
    }) {
        return Err(fail(
            "unsupported_layout",
            "Task insertion requires Planned tasks with at most one retained assignment and no links, costs, progress, codes or user values",
        ));
    }
    if assigned && all.iter().any(|t| t.parent_guid.is_some()) {
        return Err(fail("unsupported_layout", "Projects with retained assignments must contain only flat root tasks"));
    }
    Ok(if assigned { SourceScope::FlatPlannedSingleAssignments } else { SourceScope::PlannedUnassigned })
}
fn planned_state(raw: &[u8], source: &Container, callback: &mut dyn FnMut(f32) -> bool) -> Result<(String, Vec<u8>)> {
    let section = source.sections.get("Task States").ok_or_else(|| fail("missing_dependency", "Task States definitions are required"))?;
    let prefix = b"\x01\x0bTask States";
    marker(raw, section.file_offset, prefix)?;
    let payload = checked_add(section.file_offset, prefix.len())?;
    let length = section.stored_bytes.checked_sub(prefix.len()).ok_or_else(|| fail("truncated", "Task States wrapper"))?;
    let data = inflate(bytes(raw, payload, length)?, 65_536, callback)?;
    marker(&data, 24, OPEN)?;
    if count_at(&data, 28)? != 3 {
        return Err(fail("unsupported_layout", "Constructor requires the three observed task-state definitions"));
    }
    let mut states = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut at = 32;
    for _ in 0..3 {
        check(callback, 0.2)?;
        marker(&data, at, &[0xf7, 0x38, 0xab, 0x60])?;
        marker(&data, checked_add(at, 4)?, &[0; 8])?;
        let id = guid_at(&data, checked_add(at, 12)?)?;
        if bytes(&data, checked_add(at, 12)?, 16)?.iter().all(|v| *v == 0) || !ids.insert(id.clone()) {
            return Err(fail("invalid_identity", "Task-state identities must be nonzero and unique"));
        }
        let (_, end) = string_at(&data, checked_add(at, 28)?)?;
        marker(&data, end, &[1, 1])?;
        let kind = u32_at(&data, checked_add(end, 2)?)?;
        if !(1..=3).contains(&kind) || states.insert(kind, id).is_some() {
            return Err(fail("unsupported_layout", "Task-state kind differs from the observed default definitions"));
        }
        for color in [6, 10, 14] {
            if !f32::from_bits(u32_at(&data, checked_add(end, color)?)?).is_finite() {
                return Err(fail("unsupported_layout", "Nonfinite task-state color"));
            }
        }
        marker(&data, checked_add(end, 18)?, &[0x33, 0x58, 0x12, 0x76])?;
        // Preserve this definition's opaque collection word; it is not a task lookup entry.
        bytes(&data, checked_add(end, 22)?, 4)?;
        at = checked_add(end, 26)?;
    }
    marker(&data, at, CLOSE)?;
    if checked_add(at, 4)? != data.len() {
        return Err(fail("unsupported_layout", "Unconsumed task-state definition bytes"));
    }
    let planned = states.remove(&1).ok_or_else(|| fail("missing_dependency", "Observed Planned task-state definition missing"))?;
    Ok((planned, data))
}
fn calendar(data: &[u8], template: &Task, anchor: &Task) -> Result<()> {
    let c = crate::project_calendar::parse(data).map_err(|_| fail("unsupported_layout", "A decoded default project calendar is required"))?;
    let id = template.calendar_guid_storage.as_str();
    if anchor.calendar_guid_storage != id || c.default_calendar_guid.as_deref() != Some(id) {
        return Err(fail("unsupported_layout", "Template and anchor must use the same explicit default project calendar"));
    }
    let record = c.records.iter().find(|r| r.guid == id).ok_or_else(|| fail("missing_dependency", "Default calendar record missing"))?;
    if record.parent_guid.is_some() || !record.exceptions.is_empty() {
        return Err(fail("unsupported_layout", "Inherited calendars and date exceptions are outside this constructor"));
    }
    for (day, day_id) in record.weekday_day_type_guids.iter().enumerate() {
        let day_type = record
            .day_types
            .iter()
            .find(|t| Some(t.guid.as_str()) == day_id.as_deref())
            .ok_or_else(|| fail("unsupported_layout", "Inherited weekdays are outside this constructor"))?;
        let valid = if day == 0 || day == 6 {
            day_type.kind_storage == 2 && day_type.intervals.is_empty()
        } else {
            day_type.kind_storage == 1
                && day_type.intervals.len() == 1
                && day_type.intervals.first().is_some_and(|i| i.start_second == 32_400 && i.end_second == 61_200)
        };
        if !valid {
            return Err(fail("unsupported_layout", "Constructor requires Monday-Friday 09:00-17:00 work without exceptions"));
        }
    }
    let start = anchor.planned_start_wallclock_scalar;
    if start >= 32_535_244_799 || start % 86_400 != 32_400 {
        return Err(fail("unsupported_value", "Anchor must start at 09:00 on an admitted working date"));
    }
    let start = i64::try_from(start).map_err(|_| fail("unsupported_value", "Anchor date"))?;
    let allocation = c
        .working_segments_with_budget(Some(id), start, DURATION, crate::project_calendar::WorkingBudget { max_scanned_days: 7, max_segments: 2 }, || false)
        .map_err(|_| fail("unsupported_value", "Anchor working date is unresolved"))?;
    if allocation.segments.len() != 1
        || allocation.segments.first().is_none_or(|s| s.start_wallclock != start)
        || allocation.finish_wallclock != start.checked_add(28_800).ok_or_else(|| fail("limit", "Anchor finish overflow"))?
    {
        return Err(fail("unsupported_value", "Anchor must start on an admitted working weekday"));
    }
    Ok(())
}
fn source_supply_identities(data: &[u8], all: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<()> {
    let primary: BTreeSet<&str> = all.iter().map(|t| t.guid.as_str()).collect();
    let mut supplies = BTreeSet::new();
    for task in all {
        check(callback, 0.3)?;
        if bytes(data, checked_add(task.offset, 12)?, 16)?.iter().all(|v| *v == 0) {
            return Err(fail("invalid_identity", "Source task identities must be nonzero"));
        }
        let children: Vec<&Task> = all.iter().filter(|t| t.parent_guid.as_deref() == Some(task.guid.as_str())).collect();
        let extra = if children.is_empty() { 0 } else { children.iter().try_fold(12usize, |n, t| checked_add(n, t.bytes))? };
        let at = checked_add(checked_add(task.name_end, 94)?, extra)?;
        marker(data, at, &[0xb3, 0x0c, 0x9c, 0xd1])?;
        let raw = bytes(data, checked_add(at, 4)?, 16)?;
        let id = guid_at(raw, 0)?;
        if raw.iter().all(|v| *v == 0) || primary.contains(id.as_str()) || !supplies.insert(id) {
            return Err(fail("invalid_identity", "Source Supply Chain identities must be nonzero, unique and separate from task identities"));
        }
    }
    Ok(())
}

/// Insert one default Work leaf above an existing leaf without mutating the source.
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    request: &InsertAbove,
    mut keep_going: impl FnMut(f32) -> bool,
) -> std::result::Result<Candidate, FormatError> {
    inner(raw, expected_source_sha256, request, &mut keep_going).map_err(format_error)
}
fn inner(raw: &[u8], expected: &str, request: &InsertAbove, callback: &mut dyn FnMut(f32) -> bool) -> Result<Candidate> {
    let task_id = raw_guid(&request.task_guid)?;
    let supply_id = raw_guid(&request.supply_chain_id)?;
    if task_id == supply_id {
        return Err(fail("invalid_identity", "Task and Supply Chain identities must be distinct"));
    }
    let prepared = admission::prepare(raw, expected, Some((&task_id, &supply_id)), callback)?;
    let source_hash = prepared.source_hash.clone();
    let source_scope = prepared.source_scope;
    let data = &prepared.data;
    let evidence = &prepared.evidence;
    let state = &prepared.state;
    let template = writer::task_for(&evidence.tasks, &request.template_task_guid)?;
    let anchor = writer::task_for(&evidence.tasks, &request.anchor_task_guid)?;
    prepared.admit_pair(template, anchor, callback)?;
    let parent = request.expected_parent_guid.as_deref().map(|id| writer::task_for(&evidence.tasks, id).map(|t| t.guid.clone())).transpose()?;
    if anchor.parent_guid != parent {
        return Err(fail("stale_source", "Expected anchor parent differs from source"));
    }
    let siblings: Vec<&Task> = evidence.tasks.iter().filter(|t| t.parent_guid == parent).collect();
    if siblings.get(request.expected_sibling_index).map(|t| &t.guid) != Some(&anchor.guid) {
        return Err(fail("stale_source", "Expected anchor sibling index differs from source"));
    }
    if request.name.len() > 65_536
        || request.display_id.len() > 1024
        || request.name.trim().is_empty()
        || request.display_id.trim().is_empty()
        || request.display_id.encode_utf16().count() > 256
    {
        return Err(fail("unsupported_value", "Choose a name and a display ID of at most 256 UTF-16 units"));
    }
    if evidence.tasks.iter().any(|t| t.display_id.eq_ignore_ascii_case(&request.display_id)) {
        return Err(fail("invalid_identity", "Display ID is already used by a source task"));
    }
    let name = task_strings::encode(&request.name)?;
    let display = task_strings::encode(&request.display_id)?;
    let mut row = Vec::new();
    append(&mut row, bytes(data, template.offset, 12)?)?;
    append(&mut row, &task_id)?;
    append(&mut row, bytes(data, checked_add(template.offset, 28)?, 5)?)?;
    append(&mut row, &[0; 4])?;
    let assignment_bytes = template.assignments.len().checked_mul(235).ok_or_else(|| fail("limit", "Template assignment bytes"))?;
    let assignment_end = checked_add(checked_add(template.offset, 37)?, assignment_bytes)?;
    append(&mut row, bytes(data, assignment_end, 36)?)?;
    append(&mut row, &name)?;
    append(&mut row, &anchor.planned_start_wallclock_scalar.to_le_bytes())?;
    append(&mut row, &DURATION.to_le_bytes())?;
    append(&mut row, bytes(data, checked_add(template.name_end, 16)?, 82)?)?;
    append(&mut row, &supply_id)?;
    append(&mut row, bytes(data, checked_add(template.name_end, 114)?, 82)?)?;
    append(&mut row, &display)?;
    let template_end = checked_add(template.offset, template.bytes)?;
    append(
        &mut row,
        bytes(data, template.display_id_end, template_end.checked_sub(template.display_id_end).ok_or_else(|| fail("truncated", "Template tail"))?)?,
    )?;
    let count_at = if let Some(parent) = parent.as_deref() { checked_add(writer::task_for(&evidence.tasks, parent)?.name_end, 21)? } else { 28 };
    if u32_at(data, count_at)? != u32::try_from(siblings.len()).map_err(|_| fail("limit", "Sibling count"))? {
        return Err(fail("unsupported_layout", "Containing collection count differs from its outline"));
    }
    let mut updated = Vec::new();
    append(&mut updated, bytes(data, 0, anchor.offset)?)?;
    append(&mut updated, &row)?;
    append(&mut updated, bytes(data, anchor.offset, data.len().checked_sub(anchor.offset).ok_or_else(|| fail("truncated", "Insertion offset"))?)?)?;
    let total = u32::try_from(checked_add(siblings.len(), 1)?).map_err(|_| fail("limit", "Sibling count"))?;
    updated.get_mut(count_at..checked_add(count_at, 4)?).ok_or_else(|| fail("truncated", "Collection count write"))?.copy_from_slice(&total.to_le_bytes());
    let after = tasks(&updated, callback)?;
    let mut groups: BTreeMap<Option<&str>, Vec<&Task>> = BTreeMap::new();
    for task in &after {
        groups.entry(task.parent_guid.as_deref()).or_default().push(task);
    }
    for siblings in groups.values() {
        let lookup = task_lookup::lookup_indices(&updated, siblings)?;
        for (task, index) in siblings.iter().zip(lookup) {
            check(callback, 0.7)?;
            let end = checked_add(task.offset, task.bytes)?;
            let at = end.checked_sub(4).ok_or_else(|| fail("truncated", "Task lookup write"))?;
            updated.get_mut(at..end).ok_or_else(|| fail("truncated", "Task lookup write"))?.copy_from_slice(&index.to_le_bytes());
        }
    }
    let new = tasks(&updated, callback)?;
    if new.len() != checked_add(evidence.tasks.len(), 1)? || !relocation::all_lookups_valid(&updated, &new, callback)? {
        return Err(fail("invalid_output", "Inserted task count or lookup verification failed"));
    }
    hierarchy::verify_retained_bodies(data, &evidence.tasks, &updated, &new, callback)?;
    let id = guid_at(&task_id, 0)?;
    let inserted = writer::task_for(&new, &id)?;
    if inserted.parent_guid != parent
        || inserted.name != request.name
        || inserted.display_id != request.display_id
        || inserted.duration_storage_scalar != DURATION
        || inserted.calendar_guid_storage != template.calendar_guid_storage
        || !inserted.assignments.is_empty()
        || guid_at(&updated, checked_add(inserted.display_id_end, 9)?)? != *state
        || inserted.planned_start_wallclock_scalar != anchor.planned_start_wallclock_scalar
        || bytes(&updated, checked_add(inserted.name_end, 98)?, 16)? != supply_id
    {
        return Err(fail("invalid_output", "Inserted task fields differ from the request"));
    }
    profile(&updated, inserted, &new, callback)?;
    let mut expected_order: Vec<&str> = evidence.tasks.iter().map(|t| t.guid.as_str()).collect();
    let position = expected_order.iter().position(|g| *g == anchor.guid).ok_or_else(|| fail("invalid_identity", "Anchor preorder position missing"))?;
    expected_order.insert(position, &id);
    if !new.iter().map(|t| t.guid.as_str()).eq(expected_order) {
        return Err(fail("invalid_output", "Task preorder changed outside the insertion"));
    }
    let (output, output_hash, guards) = writer::pack_tasks(raw, &updated, evidence, true, callback)?;
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 2,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            insertion: request.clone(),
            source_scope,
            template_assignments_omitted: template.assignments.len(),
            new_task_assignment_count: 0,
            default_profile_sha256: PROFILE,
            planned_start_wallclock_scalar: anchor.planned_start_wallclock_scalar,
            duration_storage_scalar: DURATION,
            task_count_before: evidence.tasks.len(),
            task_count_after: new.len(),
            sections: guards,
            internal_reparse_passed: true,
            existing_task_bodies_preserved: true,
            collection_lookup_indices_validated: true,
            collection_lookup_indices_rebuilt: true,
            task_state_definition_validated: true,
            source_supply_chain_identities_validated: true,
            native_transaction_history_reproduced: false,
            native_reopen_verified: false,
            native_row_identity_verified: false,
            derived_schedule_recalculated: false,
            geometry_validated: false,
            constructor_completeness_verified: false,
            limitations: vec![
                "One default Planned Work leaf above an existing Work leaf; Monday-Friday 09:00-17:00 default calendar only.",
                "Unassigned Planned hierarchies or flat Planned roots retaining at most one assignment per task; template assignments are omitted from the new task.",
                "Names and display IDs are explicit; automatic display-ID generation, task deletion and general construction are not admitted.",
                "Only Tasks changes. Native transaction, thumbnail and presentation history are preserved rather than reproduced.",
                "Native persistence, geometry, summary dates and full scheduling require independent acceptance.",
            ],
        },
    })
}
