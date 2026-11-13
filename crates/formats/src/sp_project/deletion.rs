//! Independent, bounded deletion of an unassigned Planned root leaf.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;
mod presentation;
#[cfg(test)]
#[path = "deletion_tests.rs"]
mod tests;

#[derive(Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteLeaf {
    pub task_guid: String,
    pub expected_parent_guid: Option<String>,
    pub expected_sibling_index: usize,
    pub expected_display_id: String,
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub deletion: DeleteLeaf,
    pub removed_supply_chain_id: String,
    pub task_count_before: usize,
    pub task_count_after: usize,
    pub sections: Vec<writer::SectionGuard>,
    pub internal_reparse_passed: bool,
    pub surviving_task_bodies_preserved: bool,
    pub collection_lookups_rebuilt: bool,
    pub presentation_rows_validated: bool,
    pub task_selection_cleared: bool,
    pub reference_sections_checked: usize,
    pub removed_identity_references_absent: bool,
    pub native_reopen_verified: bool,
    pub native_row_identity_verified: bool,
    pub derived_schedule_recalculated: bool,
    pub native_transaction_history_reproduced: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}

#[derive(Serialize, Deserialize)]
pub struct LayoutAvailability {
    pub version: u32,
    pub source_sha256: String,
    pub anchors: Vec<DeleteLeaf>,
    pub reason: Option<String>,
    /// Layout hints do not bypass the authoritative per-target reference scan.
    pub reference_scan_required: bool,
}
/// Inspect source/profile/presentation layout once, without constructing candidates per row.
pub fn layout_availability(raw: &[u8], expected: &str, mut keep: impl FnMut(f32) -> bool) -> std::result::Result<LayoutAvailability, FormatError> {
    if raw.len() > FILE_LIMIT || expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format_error(fail("invalid_identity", "Choose a source up to 256 MiB and its 64-digit SHA-256")));
    }
    let result: Result<(String, Vec<DeleteLeaf>)> = (|| {
        let (hash, _, evidence) = creation::unassigned_roots(raw, expected, &mut keep)?;
        let section = evidence
            .sections
            .get("PresentationData")
            .ok_or_else(|| fail("missing_dependency", "This project lacks the saved view state required for guarded deletion"))?;
        let data = decoded_section(raw, "PresentationData", section, &mut keep)?;
        presentation::validate_source(&data, &evidence.tasks, &mut keep)?;
        let anchors = evidence
            .tasks
            .iter()
            .enumerate()
            .map(|(i, t)| DeleteLeaf {
                task_guid: t.guid.clone(),
                expected_parent_guid: None,
                expected_sibling_index: i,
                expected_display_id: t.display_id.clone(),
            })
            .collect();
        Ok((hash, anchors))
    })();
    match result {
        Ok((hash, anchors)) => Ok(LayoutAvailability { version: 1, source_sha256: hash, anchors, reason: None, reference_scan_required: true }),
        Err(e) if e.code == "cancelled" || e.code == "stale_source" => Err(format_error(e)),
        Err(e) => Ok(LayoutAvailability {
            version: 1,
            source_sha256: expected.to_ascii_lowercase(),
            anchors: Vec::new(),
            reason: Some(e.message),
            reference_scan_required: true,
        }),
    }
}

fn needles(primary: &[u8], supply: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut result = Vec::new();
    for id in [primary, supply] {
        result.push(id.to_vec());
        let text = guid_at(id, 0)?;
        for spelling in [text.clone(), text.to_ascii_uppercase()] {
            result.push(spelling.as_bytes().to_vec());
            result.push(spelling.encode_utf16().flat_map(u16::to_le_bytes).collect());
        }
    }
    Ok(result)
}
fn no_references(data: &[u8], needles: &[Vec<u8>], section: &str, callback: &mut dyn FnMut(f32) -> bool) -> Result<()> {
    for at in (0..data.len()).step_by(65_536) {
        check(callback, 0.4)?;
        let count = data.len().saturating_sub(at).min(65_607);
        let part = bytes(data, at, count)?;
        for id in needles {
            if let Some(index) = part.windows(id.len()).position(|v| v == id.as_slice()) {
                let mut error = fail("unsupported_reference", "A removed identity has a reference outside the admitted task and presentation records");
                error.section = Some(section.to_owned());
                error.offset = Some(checked_add(at, index)?);
                return Err(error);
            }
        }
    }
    Ok(())
}
fn decoded_section(raw: &[u8], name: &str, section: &Section, callback: &mut dyn FnMut(f32) -> bool) -> Result<Vec<u8>> {
    let prefix = checked_add(name.len(), 2)?;
    let at = checked_add(section.file_offset, prefix)?;
    let count = section.stored_bytes.checked_sub(prefix).ok_or_else(|| fail("truncated", "Section wrapper"))?;
    inflate(bytes(raw, at, count)?, STREAM_LIMIT, callback)
}

/// Produce a separate candidate; never modifies an input or performs native saving.
pub fn candidate(raw: &[u8], expected: &str, deletion: &DeleteLeaf, mut keep: impl FnMut(f32) -> bool) -> std::result::Result<Candidate, FormatError> {
    inner(raw, expected, deletion, &mut keep).map_err(format_error)
}
fn inner(raw: &[u8], expected: &str, request: &DeleteLeaf, callback: &mut dyn FnMut(f32) -> bool) -> Result<Candidate> {
    let (source_hash, data, evidence) = creation::unassigned_roots(raw, expected, callback)?;
    let target = writer::task_for(&evidence.tasks, &request.task_guid)?;
    let index = evidence.tasks.iter().position(|t| t.guid == target.guid).ok_or_else(|| fail("invalid_identity", "Deletion task position missing"))?;
    if request.expected_parent_guid.is_some() || request.expected_sibling_index != index || request.expected_display_id != target.display_id {
        return Err(fail("stale_source", "Deletion parent, sibling position or display ID changed; reload this source"));
    }
    let primary = bytes(&data, checked_add(target.offset, 12)?, 16)?;
    let supply = bytes(&data, checked_add(target.name_end, 98)?, 16)?;
    let removed_supply_chain_id = guid_at(supply, 0)?;
    let needles = needles(primary, supply)?;
    let first = evidence.sections.values().map(|s| s.file_offset).min().ok_or_else(|| fail("missing_dependency", "Section directory is empty"))?;
    no_references(bytes(raw, 0, first)?, &needles, "header", callback)?;
    let end = checked_add(target.offset, target.bytes)?;
    let mut updated = Vec::new();
    updated.try_reserve_exact(data.len()).map_err(|_| fail("limit", "Deletion allocation"))?;
    updated.extend_from_slice(bytes(&data, 0, target.offset)?);
    updated.extend_from_slice(bytes(&data, end, data.len().checked_sub(end).ok_or_else(|| fail("truncated", "Deletion range"))?)?);
    if usize::try_from(u32_at(&data, 28)?).map_err(|_| fail("limit", "Root task count"))? != evidence.tasks.len() {
        return Err(fail("unsupported_layout", "Root collection count differs from the outline"));
    }
    let count = u32::try_from(evidence.tasks.len().checked_sub(1).ok_or_else(|| fail("limit", "Deletion task count"))?)
        .map_err(|_| fail("limit", "Deletion task count"))?;
    updated.get_mut(28..32).ok_or_else(|| fail("truncated", "Root count write"))?.copy_from_slice(&count.to_le_bytes());
    let after = tasks(&updated, callback)?;
    for (task, lookup) in after.iter().zip(task_lookup::lookup_indices(&updated, &after.iter().collect::<Vec<_>>())?) {
        let at = checked_add(task.offset, task.bytes)?.checked_sub(4).ok_or_else(|| fail("truncated", "Lookup write"))?;
        updated.get_mut(at..checked_add(at, 4)?).ok_or_else(|| fail("truncated", "Lookup range"))?.copy_from_slice(&lookup.to_le_bytes());
    }
    let after = tasks(&updated, callback)?;
    if !relocation::all_lookups_valid(&updated, &after, callback)? || after.len() != evidence.tasks.len().saturating_sub(1) {
        return Err(fail("invalid_output", "Deletion count or lookup verification failed"));
    }
    let expected_order = evidence.tasks.iter().filter(|t| t.guid != target.guid).map(|t| t.guid.as_str());
    if !after.iter().map(|t| t.guid.as_str()).eq(expected_order) {
        return Err(fail("invalid_output", "Deletion changed the surviving task order"));
    }
    for task in &after {
        let old = writer::task_for(&evidence.tasks, &task.guid)?;
        let length = old.bytes.checked_sub(4).ok_or_else(|| fail("truncated", "Task body"))?;
        if old.bytes != task.bytes || old.parent_guid != task.parent_guid || bytes(&data, old.offset, length)? != bytes(&updated, task.offset, length)? {
            return Err(fail("invalid_output", "Deletion changed a surviving task body"));
        }
    }
    no_references(&updated, &needles, "Tasks", callback)?;
    let psection = evidence.sections.get("PresentationData").ok_or_else(|| fail("missing_dependency", "PresentationData is required for deletion"))?;
    let source_presentation = decoded_section(raw, "PresentationData", psection, callback)?;
    let (new_presentation, selection_cleared) = presentation::remove(&source_presentation, &evidence.tasks, &after, primary, callback)?;
    no_references(&new_presentation, &needles, "PresentationData", callback)?;
    let mut total = checked_add(data.len(), source_presentation.len())?;
    for (name, section) in &evidence.sections {
        if matches!(name.as_str(), "Tasks" | "PresentationData") {
            continue;
        }
        check(callback, 0.5)?;
        if name == "Thumbnail" {
            no_references(bytes(raw, section.file_offset, section.stored_bytes)?, &needles, name, callback)?;
        } else {
            let decoded = decoded_section(raw, name, section, callback)?;
            total = checked_add(total, decoded.len())?;
            if total > TOTAL_LIMIT {
                return Err(fail("limit", "Deletion section scan exceeds the 64 MiB budget"));
            }
            no_references(&decoded, &needles, name, callback)?;
        }
    }
    let (output, output_hash, guards) = writer::pack_sections(raw, &evidence, &[("Tasks", &updated), ("PresentationData", &new_presentation)], callback)?;
    let parsed = container(&output, callback)?;
    let section = parsed.sections.get("PresentationData").ok_or_else(|| fail("invalid_output", "Presentation section disappeared"))?;
    if decoded_section(&output, "PresentationData", section, callback)? != new_presentation {
        return Err(fail("invalid_output", "Presentation reparse differs"));
    }
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 1,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            deletion: request.clone(),
            removed_supply_chain_id,
            task_count_before: evidence.tasks.len(),
            task_count_after: after.len(),
            sections: guards,
            internal_reparse_passed: true,
            surviving_task_bodies_preserved: true,
            collection_lookups_rebuilt: true,
            presentation_rows_validated: true,
            task_selection_cleared: selection_cleared,
            reference_sections_checked: evidence.sections.len(),
            removed_identity_references_absent: true,
            native_reopen_verified: false,
            native_row_identity_verified: false,
            derived_schedule_recalculated: false,
            native_transaction_history_reproduced: false,
            limitations: vec![
                "One unassigned Planned root leaf from a flat admitted default-Work project; at least one root must remain.",
                "No source assignments, links, progress, costs, child/summary deletion or multi-selection. Unknown identity references are rejected.",
                "Tasks and validated presentation rows/selection change; the other 46 stored sections and opaque native history are preserved.",
                "Internal validation is not native saving, full row identity, geometry or scheduling certification.",
            ],
        },
    })
}
