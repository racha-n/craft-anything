//! Guarded existing-task reparenting; schedule coordination remains a separate gate.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;

#[cfg(test)]
#[path = "hierarchy_tests.rs"]
mod tests;

#[derive(Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Reparent {
    pub task_guid: String,
    pub expected_parent_guid: Option<String>,
    pub expected_sibling_index: usize,
    /// Null moves the subtree into the root collection.
    pub parent_guid: Option<String>,
    /// Position in the destination collection after removing the moving subtree.
    pub sibling_index: usize,
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub relocation: Reparent,
    pub task_count: usize,
    pub sections: Vec<writer::SectionGuard>,
    pub internal_reparse_passed: bool,
    pub task_guids_preserved: bool,
    pub task_body_fields_preserved: bool,
    pub collection_lookup_indices_validated: bool,
    pub collection_lookup_indices_rebuilt: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_recalculated: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}
type Groups<'a> = BTreeMap<Option<&'a str>, Vec<&'a Task>>;
#[derive(Serialize)]
pub struct Hint {
    pub task_guid: String,
    pub indent: Option<Reparent>,
    pub outdent: Option<Reparent>,
    pub indent_depth_limited: bool,
}
pub(super) fn hints(data: &[u8], tasks: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<Vec<Hint>> {
    let groups = groups(tasks);
    spans(data, tasks, &groups)?;
    let mut depths = BTreeMap::new();
    let mut positions = BTreeMap::new();
    for siblings in groups.values() {
        for (index, task) in siblings.iter().enumerate() {
            positions.insert(task.guid.as_str(), index);
        }
    }
    for task in tasks {
        let depth = match task.parent_guid.as_deref() {
            None => 0usize,
            Some(parent) => checked_add(*depths.get(parent).ok_or_else(|| fail("invalid_identity", "Parent depth missing"))?, 1)?,
        };
        depths.insert(task.guid.as_str(), depth);
    }
    let mut result = Vec::new();
    for siblings in groups.values() {
        for (index, task) in siblings.iter().enumerate() {
            check(callback, 0.95)?;
            let end = checked_add(task.offset, task.bytes)?;
            let mut max_depth = 0usize;
            for descendant in tasks.iter().filter(|t| t.offset >= task.offset && t.offset < end) {
                max_depth = max_depth.max(*depths.get(descendant.guid.as_str()).ok_or_else(|| fail("invalid_identity", "Task depth missing"))?);
            }
            let previous = index.checked_sub(1).and_then(|i| siblings.get(i));
            let indent_depth_limited = previous.is_some() && max_depth >= 64;
            let indent = previous.filter(|_| !indent_depth_limited).map(|parent| Reparent {
                task_guid: task.guid.clone(),
                expected_parent_guid: task.parent_guid.clone(),
                expected_sibling_index: index,
                parent_guid: Some(parent.guid.clone()),
                sibling_index: groups.get(&Some(parent.guid.as_str())).map_or(0, Vec::len),
            });
            let outdent = if let Some(parent) = task.parent_guid.as_deref() {
                let parent = writer::task_for(tasks, parent)?;
                let position = *positions.get(parent.guid.as_str()).ok_or_else(|| fail("invalid_identity", "Parent sibling position missing"))?;
                Some(Reparent {
                    task_guid: task.guid.clone(),
                    expected_parent_guid: task.parent_guid.clone(),
                    expected_sibling_index: index,
                    parent_guid: parent.parent_guid.clone(),
                    sibling_index: checked_add(position, 1)?,
                })
            } else {
                None
            };
            result.push(Hint { task_guid: task.guid.clone(), indent, outdent, indent_depth_limited });
        }
    }
    Ok(result)
}
struct Span {
    flag: usize,
    after_children: usize,
    body_end: usize,
}
fn body_parts<'a>(data: &'a [u8], task: &Task, span: &Span) -> Result<(&'a [u8], &'a [u8])> {
    let before = span.flag.checked_sub(task.offset).ok_or_else(|| fail("truncated", "Task body prefix"))?;
    let after = span.body_end.checked_sub(span.after_children).ok_or_else(|| fail("truncated", "Task body suffix"))?;
    Ok((bytes(data, task.offset, before)?, bytes(data, span.after_children, after)?))
}
fn groups(tasks: &[Task]) -> Groups<'_> {
    let mut result: Groups<'_> = BTreeMap::new();
    for task in tasks {
        result.entry(task.parent_guid.as_deref()).or_default().push(task);
    }
    result
}
fn spans<'a>(data: &[u8], tasks: &'a [Task], groups: &Groups<'a>) -> Result<BTreeMap<&'a str, Span>> {
    let mut result = BTreeMap::new();
    for task in tasks {
        let flag = checked_add(task.name_end, 16)?;
        let start = checked_add(flag, 1)?;
        let children = groups.get(&Some(task.guid.as_str())).map(Vec::as_slice).unwrap_or(&[]);
        let mut end = start;
        if !children.is_empty() {
            marker(data, flag, &[1])?;
            marker(data, start, OPEN)?;
            if count_at(data, checked_add(start, 4)?)? != children.len() {
                return Err(fail("unsupported_layout", "Task child collection count differs from its outline"));
            }
            end = checked_add(start, 8)?;
            for child in children {
                if child.offset != end {
                    return Err(fail("unsupported_layout", "Task child byte ranges are not contiguous"));
                }
                end = checked_add(end, child.bytes)?;
            }
            marker(data, end, CLOSE)?;
            end = checked_add(end, 4)?;
        } else {
            // The observed native writer omits empty child lists. Do not normalize unknown layouts.
            marker(data, flag, &[0])?;
        }
        let body_end = checked_add(task.offset, task.bytes)?.checked_sub(4).ok_or_else(|| fail("truncated", "Task lookup entry missing"))?;
        if end > body_end || flag < task.offset {
            return Err(fail("unsupported_layout", "Task body spans overlap"));
        }
        result.insert(task.guid.as_str(), Span { flag, after_children: end, body_end });
    }
    Ok(result)
}
fn extend(out: &mut Vec<u8>, value: &[u8]) -> Result<()> {
    if checked_add(out.len(), value.len())? > STREAM_LIMIT {
        return Err(fail("limit", "Hierarchy candidate Tasks exceed the 16 MiB section cap"));
    }
    out.try_reserve(value.len()).map_err(|_| fail("limit", "Hierarchy candidate allocation"))?;
    out.extend_from_slice(value);
    Ok(())
}
fn emit(
    data: &[u8],
    groups: &Groups<'_>,
    spans: &BTreeMap<&str, Span>,
    parent: Option<&str>,
    depth: usize,
    out: &mut Vec<u8>,
    callback: &mut dyn FnMut(f32) -> bool,
) -> Result<()> {
    if depth > 64 {
        return Err(fail("limit", "Reparenting exceeds the 64-level task depth limit"));
    }
    check(callback, 0.4)?;
    let siblings = groups.get(&parent).map(Vec::as_slice).unwrap_or(&[]);
    let lookup = relocation::lookup_indices(data, siblings)?;
    extend(out, OPEN)?;
    extend(out, &u32::try_from(siblings.len()).map_err(|_| fail("limit", "Hierarchy child count"))?.to_le_bytes())?;
    for (task, index) in siblings.iter().zip(lookup) {
        check(callback, 0.4)?;
        let span = spans.get(task.guid.as_str()).ok_or_else(|| fail("invalid_identity", "Task body span missing"))?;
        extend(out, bytes(data, task.offset, span.flag.checked_sub(task.offset).ok_or_else(|| fail("truncated", "Task prefix"))?)?)?;
        let has_children = groups.get(&Some(task.guid.as_str())).is_some_and(|children| !children.is_empty());
        extend(out, &[u8::from(has_children)])?;
        if has_children {
            emit(data, groups, spans, Some(task.guid.as_str()), checked_add(depth, 1)?, out, callback)?;
        }
        extend(out, bytes(data, span.after_children, span.body_end.checked_sub(span.after_children).ok_or_else(|| fail("truncated", "Task suffix"))?)?)?;
        extend(out, &index.to_le_bytes())?;
    }
    extend(out, CLOSE)
}

pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    request: &Reparent,
    mut keep_going: impl FnMut(f32) -> bool,
) -> std::result::Result<Candidate, FormatError> {
    inner(raw, expected_source_sha256, request, &mut keep_going).map_err(format_error)
}
fn inner(raw: &[u8], expected: &str, request: &Reparent, callback: &mut dyn FnMut(f32) -> bool) -> Result<Candidate> {
    let (source_hash, source) = relocation::source(raw, expected, callback)?;
    let data = source.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
    let evidence = inspect_container(source, source_hash.clone(), false, callback)?;
    if let Some(reason) = relocation::scope(&evidence.tasks) {
        return Err(fail("unsupported_layout", reason.message()));
    }
    if !relocation::all_lookups_valid(&data, &evidence.tasks)? {
        return Err(fail("unsupported_layout", "Source task collection lookup indices are inconsistent"));
    }
    let task = writer::task_for(&evidence.tasks, &request.task_guid)?;
    let expected_parent = request.expected_parent_guid.as_deref().map(|id| writer::task_for(&evidence.tasks, id)).transpose()?;
    let destination = request.parent_guid.as_deref().map(|id| writer::task_for(&evidence.tasks, id)).transpose()?;
    let old_parent = task.parent_guid.as_deref();
    let new_parent = destination.map(|t| t.guid.as_str());
    if old_parent != expected_parent.map(|t| t.guid.as_str()) {
        return Err(fail("stale_source", "Expected task parent differs from source"));
    }
    if old_parent == new_parent {
        return Err(fail("invalid_value", "Use the sibling-order tool when the parent is unchanged"));
    }
    let mut ancestor = destination;
    for _ in 0..=evidence.tasks.len() {
        let Some(parent) = ancestor else { break };
        check(callback, 0.25)?;
        if parent.guid == task.guid {
            return Err(fail("invalid_value", "A task cannot move beneath itself or its descendant"));
        }
        ancestor = parent.parent_guid.as_deref().map(|id| writer::task_for(&evidence.tasks, id)).transpose()?;
    }
    if ancestor.is_some() {
        return Err(fail("invalid_identity", "Task ancestor walk did not terminate"));
    }
    let mut outline = groups(&evidence.tasks);
    let original_spans = spans(&data, &evidence.tasks, &outline)?;
    let source_siblings = outline.get(&old_parent).ok_or_else(|| fail("invalid_identity", "Source sibling collection missing"))?;
    if source_siblings.get(request.expected_sibling_index).map(|t| &t.guid) != Some(&task.guid) {
        return Err(fail("stale_source", "Expected task sibling position differs from source"));
    }
    let destination_len = outline.get(&new_parent).map_or(0, Vec::len);
    if request.sibling_index > destination_len {
        return Err(fail("invalid_value", "Destination position exceeds its sibling collection"));
    }
    let removed_list = usize::from(old_parent.is_some() && source_siblings.len() == 1) * 12;
    let added_list = usize::from(new_parent.is_some() && destination_len == 0) * 12;
    let expected_len = checked_add(data.len().checked_sub(removed_list).ok_or_else(|| fail("limit", "Hierarchy section length"))?, added_list)?;
    if expected_len > STREAM_LIMIT {
        return Err(fail("limit", "Hierarchy candidate exceeds the 16 MiB Tasks cap"));
    }
    let source_siblings = outline.get_mut(&old_parent).ok_or_else(|| fail("invalid_identity", "Source sibling collection missing"))?;
    let moving = source_siblings.remove(request.expected_sibling_index);
    outline.entry(new_parent).or_default().insert(request.sibling_index, moving);
    let mut updated = Vec::new();
    updated.try_reserve_exact(expected_len).map_err(|_| fail("limit", "Hierarchy candidate allocation"))?;
    extend(&mut updated, bytes(&data, 0, 24)?)?;
    emit(&data, &outline, &original_spans, None, 0, &mut updated, callback)?;
    if updated.len() != expected_len {
        return Err(fail("invalid_output", "Hierarchy section length differs from its structural changes"));
    }
    let (output, output_hash, guards) = writer::pack_tasks(raw, &updated, &evidence, true, callback)?;
    let parsed_container = container(&output, callback)?;
    let after_data = parsed_container.decoded.get("Tasks").ok_or_else(|| fail("invalid_output", "Tasks section disappeared"))?.clone();
    let parsed = inspect_container(parsed_container, output_hash.clone(), false, callback)?;
    if parsed.tasks.len() != evidence.tasks.len() || !relocation::all_lookups_valid(&after_data, &parsed.tasks)? {
        return Err(fail("invalid_output", "Hierarchy candidate task count or lookup validation failed"));
    }
    let after_groups = groups(&parsed.tasks);
    let after_spans = spans(&after_data, &parsed.tasks, &after_groups)?;
    for (parent, tasks) in &outline {
        let actual = after_groups.get(parent).map(Vec::as_slice).unwrap_or(&[]);
        if !tasks.iter().map(|t| &t.guid).eq(actual.iter().map(|t| &t.guid)) {
            return Err(fail("invalid_output", "Hierarchy candidate sibling order differs from the request"));
        }
    }
    for original in &evidence.tasks {
        check(callback, 0.9)?;
        let after = writer::task_for(&parsed.tasks, &original.guid)?;
        let parent = if original.guid == task.guid { new_parent } else { original.parent_guid.as_deref() };
        if after.parent_guid.as_deref() != parent {
            return Err(fail("invalid_output", "Hierarchy candidate changed an unexpected parent"));
        }
        let a = original_spans.get(original.guid.as_str()).ok_or_else(|| fail("invalid_identity", "Source body span missing"))?;
        let b = after_spans.get(original.guid.as_str()).ok_or_else(|| fail("invalid_output", "Output body span missing"))?;
        if body_parts(&data, original, a)? != body_parts(&after_data, after, b)? {
            return Err(fail("invalid_output", "Hierarchy candidate changed task body fields"));
        }
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 1,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            relocation: Reparent {
                task_guid: task.guid.clone(),
                expected_parent_guid: old_parent.map(str::to_owned),
                expected_sibling_index: request.expected_sibling_index,
                parent_guid: new_parent.map(str::to_owned),
                sibling_index: request.sibling_index,
            },
            task_count: evidence.tasks.len(),
            sections: guards,
            internal_reparse_passed: true,
            task_guids_preserved: true,
            task_body_fields_preserved: true,
            collection_lookup_indices_validated: true,
            collection_lookup_indices_rebuilt: true,
            native_reopen_verified: false,
            derived_schedule_recalculated: false,
            limitations: vec![
                "Experimental reparenting of existing Planned unassigned tasks only; no creation, deletion or associated task records.",
                "Task bodies and GUIDs are preserved; child framing and collection lookup indices are rebuilt.",
                "Stored dates and durations are unchanged; summary dates, scheduling and native persistence require separate acceptance.",
            ],
        },
    })
}
