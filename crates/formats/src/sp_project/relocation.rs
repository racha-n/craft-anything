//! Whole-record sibling order candidates. Reparenting needs separate native controls.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;

#[cfg(test)]
#[path = "relocation_tests.rs"]
mod tests;

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Move {
    pub task_guid: String,
    /// Null identifies the root collection; this operation cannot change parents.
    pub expected_parent_guid: Option<String>,
    pub expected_sibling_index: usize,
    /// Position in the same sibling list after removing the moving subtree.
    pub sibling_index: usize,
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub relocation: Move,
    pub task_count: usize,
    pub sections: Vec<writer::SectionGuard>,
    pub internal_reparse_passed: bool,
    pub task_subtree_bytes_preserved: bool,
    pub task_content_bytes_preserved: bool,
    pub sibling_lookup_index_validated: bool,
    pub sibling_lookup_index_rebuilt: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_recalculated: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeReason {
    NoTasks,
    TaskLimit,
    AssociatedOrProgressedTasks,
    InvalidSiblingLookup,
}
impl ScopeReason {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoTasks => "No tasks are available for reordering.",
            Self::TaskLimit => "Task order candidates support at most 512 tasks.",
            Self::AssociatedOrProgressedTasks => "Task ordering requires Planned tasks without assignments, links, costs, progress, codes or user values.",
            Self::InvalidSiblingLookup => "Task sibling lookup indices are inconsistent; use a valid source project.",
        }
    }
}
#[derive(Serialize)]
pub struct Position {
    pub task_guid: String,
    pub parent_guid: Option<String>,
    pub sibling_index: usize,
    pub sibling_total: usize,
}
pub struct Outline {
    pub reason: Option<ScopeReason>,
    pub positions: Vec<Position>,
    pub hierarchy_hints: Option<Vec<hierarchy::Hint>>,
    pub hierarchy_reason: String,
}
pub(super) fn scope(tasks: &[Task]) -> Option<ScopeReason> {
    if tasks.is_empty() {
        return Some(ScopeReason::NoTasks);
    }
    if tasks.len() > 512 {
        return Some(ScopeReason::TaskLimit);
    }
    tasks
        .iter()
        .any(|t| {
            t.status_storage_value != 1
                || !t.assignments.is_empty()
                || !t.links.is_empty()
                || !t.direct_costs.is_empty()
                || !t.progress_records.is_empty()
                || !t.actual_expense_reports.is_empty()
                || !t.activity_code_refs.is_empty()
                || !t.user_values.is_empty()
        })
        .then_some(ScopeReason::AssociatedOrProgressedTasks)
}
pub(super) fn source(raw: &[u8], expected: &str, callback: &mut dyn FnMut(f32) -> bool) -> Result<(String, Container)> {
    if raw.len() > FILE_LIMIT || expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(fail("invalid_identity", "Choose a source up to 256 MiB and its 64-digit SHA-256"));
    }
    let hash = writer::hash(raw, callback)?;
    if !hash.eq_ignore_ascii_case(expected) {
        return Err(fail("stale_source", "Source changed; reload before reordering tasks"));
    }
    Ok((hash, container(raw, callback)?))
}
// The collection interleaves each task body with one entry of its GUID-sorted
// lookup vector. An entry belongs to its collection slot, not to that task.
pub(super) use super::task_lookup::lookup_indices;
pub(super) fn all_lookups_valid(data: &[u8], tasks: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<bool> {
    Ok(task_lookup::audit(data, tasks, callback)?.status == task_lookup::Status::Valid)
}
/// Shared admission and complete sibling positions, independent of UI page boundaries.
pub fn outline(raw: &[u8], expected_source_sha256: &str, mut keep_going: impl FnMut(f32) -> bool) -> std::result::Result<Outline, FormatError> {
    let result = (|| {
        let (hash, container) = source(raw, expected_source_sha256, &mut keep_going)?;
        let data = container.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
        let evidence = inspect_container(container, hash, false, &mut keep_going)?;
        let mut reason = scope(&evidence.tasks);
        if reason.is_none() && !all_lookups_valid(&data, &evidence.tasks, &mut keep_going)? {
            reason = Some(ScopeReason::InvalidSiblingLookup);
        }
        let mut totals = BTreeMap::new();
        for task in &evidence.tasks {
            let total = totals.entry(task.parent_guid.as_deref()).or_insert(0usize);
            *total = checked_add(*total, 1)?;
        }
        let mut indices = BTreeMap::new();
        let mut positions = Vec::new();
        positions.try_reserve_exact(evidence.tasks.len()).map_err(|_| fail("limit", "Task order outline allocation"))?;
        for task in &evidence.tasks {
            check(&mut keep_going, 0.9)?;
            let index = indices.entry(task.parent_guid.as_deref()).or_insert(0usize);
            let total = totals.get(&task.parent_guid.as_deref()).copied().ok_or_else(|| fail("invalid_identity", "Sibling count missing"))?;
            positions.push(Position { task_guid: task.guid.clone(), parent_guid: task.parent_guid.clone(), sibling_index: *index, sibling_total: total });
            *index = checked_add(*index, 1)?;
        }
        let (hierarchy_hints, hierarchy_reason) = if let Some(reason) = reason {
            (None, reason.message().to_owned())
        } else {
            match hierarchy::hints(&data, &evidence.tasks, &mut keep_going) {
                Ok(hints) => (Some(hints), String::new()),
                Err(error) if error.code == "cancelled" => return Err(error),
                Err(error) => (None, error.message),
            }
        };
        Ok(Outline { reason, positions, hierarchy_hints, hierarchy_reason })
    })();
    result.map_err(format_error)
}

/// Move one existing task subtree within its sibling list without changing the input.
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    relocation: &Move,
    mut keep_going: impl FnMut(f32) -> bool,
) -> std::result::Result<Candidate, FormatError> {
    inner(raw, expected_source_sha256, relocation, &mut keep_going).map_err(format_error)
}

fn inner(raw: &[u8], expected: &str, request: &Move, callback: &mut dyn FnMut(f32) -> bool) -> Result<Candidate> {
    let (source_hash, source) = source(raw, expected, callback)?;
    let data = source.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
    let evidence = inspect_container(source, source_hash.clone(), false, callback)?;
    if let Some(reason) = scope(&evidence.tasks) {
        return Err(fail(if matches!(reason, ScopeReason::TaskLimit) { "limit" } else { "unsupported_layout" }, reason.message()));
    }
    if !all_lookups_valid(&data, &evidence.tasks, callback)? {
        return Err(fail("unsupported_layout", ScopeReason::InvalidSiblingLookup.message()));
    }
    let task = writer::task_for(&evidence.tasks, &request.task_guid)?;
    let parent = request.expected_parent_guid.as_deref().map(|id| writer::task_for(&evidence.tasks, id).map(|t| t.guid.clone())).transpose()?;
    if task.parent_guid != parent {
        return Err(fail("stale_source", "Expected task parent differs from source"));
    }
    let siblings: Vec<&Task> = evidence.tasks.iter().filter(|t| t.parent_guid == parent).collect();
    if siblings.get(request.expected_sibling_index).map(|t| &t.guid) != Some(&task.guid) {
        return Err(fail("stale_source", "Expected task sibling position differs from source"));
    }
    if request.sibling_index >= siblings.len() {
        return Err(fail("invalid_value", "Destination position exceeds the sibling list after removal"));
    }
    let first = siblings.first().ok_or_else(|| fail("invalid_identity", "Sibling list empty"))?.offset;
    let mut cursor = first;
    for sibling in &siblings {
        if sibling.offset != cursor {
            return Err(fail("unsupported_layout", "Sibling subtree byte ranges are not contiguous"));
        }
        cursor = checked_add(sibling.offset, sibling.bytes)?;
    }
    let end = cursor;
    let mut ordered = siblings;
    let moving = ordered.remove(request.expected_sibling_index);
    ordered.insert(request.sibling_index, moving);
    let expected_order: Vec<String> = ordered.iter().map(|t| t.guid.clone()).collect();
    let indices = lookup_indices(&data, &ordered)?;
    let mut updated = Vec::new();
    updated.try_reserve_exact(data.len()).map_err(|_| fail("limit", "Task order candidate allocation"))?;
    updated.extend_from_slice(bytes(&data, 0, first)?);
    for (sibling, index) in ordered.into_iter().zip(indices) {
        check(callback, 0.4)?;
        let body_len = sibling.bytes.checked_sub(4).ok_or_else(|| fail("truncated", "Task body length"))?;
        // Keep descendants and unknown task fields exact; rebuild this collection's lookup vector.
        updated.extend_from_slice(bytes(&data, sibling.offset, body_len)?);
        updated.extend_from_slice(&index.to_le_bytes());
    }
    updated.extend_from_slice(bytes(&data, end, data.len().checked_sub(end).ok_or_else(|| fail("truncated", "Task order tail"))?)?);
    if updated.len() != data.len() {
        return Err(fail("invalid_output", "Reordering changed the decoded Tasks length"));
    }
    let (output, output_hash, guards) = writer::pack_tasks(raw, &updated, &evidence, updated != data, callback)?;
    let parsed_container = container(&output, callback)?;
    let after_data = parsed_container.decoded.get("Tasks").ok_or_else(|| fail("invalid_output", "Tasks section disappeared"))?.clone();
    let parsed = inspect_container(parsed_container, output_hash.clone(), false, callback)?;
    if !all_lookups_valid(&after_data, &parsed.tasks, callback)? {
        return Err(fail("invalid_output", "Reordered sibling lookup indices are inconsistent"));
    }
    let actual_order: Vec<String> = parsed.tasks.iter().filter(|t| t.parent_guid == parent).map(|t| t.guid.clone()).collect();
    if parsed.tasks.len() != evidence.tasks.len() || actual_order != expected_order {
        return Err(fail("invalid_output", "Reparsed task order differs from the request"));
    }
    let mut subtree_bytes_preserved = true;
    for original in &evidence.tasks {
        check(callback, 0.9)?;
        let after = writer::task_for(&parsed.tasks, &original.guid)?;
        // Ancestors contain reordered descendants; each moved sibling's body stays exact.
        if after.parent_guid != original.parent_guid {
            return Err(fail("invalid_output", "Task order candidate changed a parent"));
        }
        if original.parent_guid == parent {
            let body_len = original.bytes.checked_sub(4).ok_or_else(|| fail("truncated", "Task body length"))?;
            if original.bytes != after.bytes || bytes(&data, original.offset, body_len)? != bytes(&after_data, after.offset, body_len)? {
                return Err(fail("invalid_output", "Task order candidate changed task content"));
            }
            subtree_bytes_preserved &= bytes(&data, original.offset, original.bytes)? == bytes(&after_data, after.offset, after.bytes)?;
        }
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 2,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            relocation: Move {
                task_guid: task.guid.clone(),
                expected_parent_guid: parent,
                expected_sibling_index: request.expected_sibling_index,
                sibling_index: request.sibling_index,
            },
            task_count: evidence.tasks.len(),
            sections: guards,
            internal_reparse_passed: true,
            task_subtree_bytes_preserved: subtree_bytes_preserved,
            task_content_bytes_preserved: true,
            sibling_lookup_index_validated: true,
            sibling_lookup_index_rebuilt: true,
            native_reopen_verified: false,
            derived_schedule_recalculated: false,
            limitations: vec![
                "Experimental sibling reordering of existing Planned unassigned tasks only; no reparenting, creation or deletion.",
                "Task content and descendants are preserved; sibling collection GUID lookup indices are rebuilt. No schedule or summary recalculation.",
                "Other section payloads are preserved, not semantically certified; native open/save/reopen remains a separate gate.",
            ],
        },
    })
}
