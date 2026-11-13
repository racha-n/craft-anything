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
    pub native_reopen_verified: bool,
    pub derived_schedule_recalculated: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
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
    if raw.len() > FILE_LIMIT || expected.len() != 64 || !expected.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(fail("invalid_identity", "Choose a source up to 256 MiB and its 64-digit SHA-256"));
    }
    let source_hash = writer::hash(raw, callback)?;
    if !source_hash.eq_ignore_ascii_case(expected) {
        return Err(fail("stale_source", "Source changed; reload before reordering tasks"));
    }
    let source = container(raw, callback)?;
    let data = source.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
    let evidence = inspect_container(source, source_hash.clone(), false, callback)?;
    if evidence.tasks.len() > 512 {
        return Err(fail("limit", "Task order candidates support at most 512 tasks"));
    }
    if evidence.tasks.iter().any(|t| {
        t.status_storage_value != 1
            || !t.assignments.is_empty()
            || !t.links.is_empty()
            || !t.direct_costs.is_empty()
            || !t.progress_records.is_empty()
            || !t.actual_expense_reports.is_empty()
            || !t.activity_code_refs.is_empty()
            || !t.user_values.is_empty()
    }) {
        return Err(fail(
            "unsupported_layout",
            "Task order candidates require Planned tasks without assignments, links, costs, progress, codes or user values",
        ));
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
    let mut updated = Vec::new();
    updated.try_reserve_exact(data.len()).map_err(|_| fail("limit", "Task order candidate allocation"))?;
    updated.extend_from_slice(bytes(&data, 0, first)?);
    for sibling in ordered {
        check(callback, 0.4)?;
        // Includes all descendants, unknown fields and the opaque trailing word.
        updated.extend_from_slice(bytes(&data, sibling.offset, sibling.bytes)?);
    }
    updated.extend_from_slice(bytes(&data, end, data.len().checked_sub(end).ok_or_else(|| fail("truncated", "Task order tail"))?)?);
    if updated.len() != data.len() {
        return Err(fail("invalid_output", "Reordering changed the decoded Tasks length"));
    }
    let (output, output_hash, guards) = writer::pack_tasks(raw, &updated, &evidence, updated != data, callback)?;
    let parsed_container = container(&output, callback)?;
    let after_data = parsed_container.decoded.get("Tasks").ok_or_else(|| fail("invalid_output", "Tasks section disappeared"))?.clone();
    let parsed = inspect_container(parsed_container, output_hash.clone(), false, callback)?;
    let actual_order: Vec<String> = parsed.tasks.iter().filter(|t| t.parent_guid == parent).map(|t| t.guid.clone()).collect();
    if parsed.tasks.len() != evidence.tasks.len() || actual_order != expected_order {
        return Err(fail("invalid_output", "Reparsed task order differs from the request"));
    }
    for original in &evidence.tasks {
        check(callback, 0.9)?;
        let after = writer::task_for(&parsed.tasks, &original.guid)?;
        // Ancestor bytes legitimately contain reordered descendants. Every moved sibling
        // subtree must remain exact; all other task identities/parents persist.
        if after.parent_guid != original.parent_guid {
            return Err(fail("invalid_output", "Task order candidate changed a parent"));
        }
        if original.parent_guid == parent && bytes(&data, original.offset, original.bytes)? != bytes(&after_data, after.offset, after.bytes)? {
            return Err(fail("invalid_output", "Task order candidate changed subtree bytes"));
        }
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 1,
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
            task_subtree_bytes_preserved: true,
            native_reopen_verified: false,
            derived_schedule_recalculated: false,
            limitations: vec![
                "Experimental sibling reordering of existing Planned unassigned tasks only; no reparenting, creation or deletion.",
                "Complete sibling subtree bytes, including opaque trailing words, are preserved; no schedule or summary recalculation.",
                "Other section payloads are preserved, not semantically certified; native open/save/reopen remains a separate gate.",
            ],
        },
    })
}
