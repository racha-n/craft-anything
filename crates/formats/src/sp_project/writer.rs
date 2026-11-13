//! Candidate storage edits for an admitted container. Native reopen is a separate gate.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "field", rename_all = "snake_case", deny_unknown_fields)]
pub enum ScalarEdit {
    TaskStart {
        task_guid: String,
        expected_storage: u64,
        value_storage: u64,
    },
    TaskDuration {
        task_guid: String,
        expected_storage: u64,
        value_storage: u64,
    },
    DirectFixedAmount {
        task_guid: String,
        cost_guid: String,
        expected_storage: f64,
        value_storage: f64,
    },
    AssignmentPlannedUnits {
        task_guid: String,
        assignment_ordinal: usize,
        resource_guid: String,
        profile_guid: String,
        expected_storage: f64,
        value_storage: f64,
    },
    /// Raw planned utilization window, not a monetary price or universal units/day value.
    AssignmentPlannedRate {
        task_guid: String,
        assignment_ordinal: usize,
        resource_guid: String,
        profile_guid: String,
        expected_storage: f64,
        value_storage: f64,
    },
}
#[derive(Debug, Serialize)]
pub struct Change {
    pub field: &'static str,
    pub task_guid: String,
    pub decoded_tasks_offset: usize,
    pub before_hex: String,
    pub after_hex: String,
}
#[derive(Debug, Serialize)]
pub struct SectionGuard {
    pub name: String,
    pub source_stored_sha256: String,
    pub output_stored_sha256: String,
    pub stored_bytes_preserved: bool,
}
#[derive(Debug, Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub changes: Vec<Change>,
    pub sections: Vec<SectionGuard>,
    pub internal_reparse_passed: bool,
    pub native_reopen_verified: bool,
    pub derived_schedule_consistency_verified: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}
pub(super) fn hash(data: &[u8], callback: &mut dyn FnMut(f32) -> bool) -> Result<String> {
    let mut h = Sha256::new();
    for block in data.chunks(65_536) {
        check(callback, 0.1)?;
        h.update(block);
    }
    Ok(format!("{:x}", h.finalize()))
}
pub(super) fn task_for<'a>(all: &'a [Task], id: &str) -> Result<&'a Task> {
    if id.len() != 36 {
        return Err(fail("invalid_identity", "Choose a source task GUID"));
    }
    all.iter().find(|t| t.guid.eq_ignore_ascii_case(id)).ok_or_else(|| fail("invalid_identity", "Task GUID is absent from this source"))
}
fn number(expected: f64, value: f64) -> Result<([u8; 8], [u8; 8])> {
    if !expected.is_finite() || !value.is_finite() || !(0.0..=1.0e12).contains(&value) {
        return Err(fail("unsupported_value", "Candidate values must be finite, nonnegative and at most 1e12 raw units"));
    }
    Ok((expected.to_le_bytes(), value.to_le_bytes()))
}
fn patch(data: &[u8], all: &[Task], edit: &ScalarEdit) -> Result<Change> {
    let (task, field, at, before, after) = match edit {
        ScalarEdit::TaskStart { task_guid, expected_storage, value_storage } => {
            if *value_storage >= 32_535_244_799 {
                return Err(fail("unsupported_value", "Candidate task starts must precede the observed unset-date sentinel"));
            }
            let task = task_for(all, task_guid)?;
            (task, "task_start", task.name_end, expected_storage.to_le_bytes(), value_storage.to_le_bytes())
        }
        ScalarEdit::TaskDuration { task_guid, expected_storage, value_storage } => {
            if *value_storage > 1_000_000 {
                return Err(fail("limit", "Candidate duration exceeds one million raw storage units"));
            }
            let task = task_for(all, task_guid)?;
            (task, "task_duration", checked_add(task.name_end, 8)?, expected_storage.to_le_bytes(), value_storage.to_le_bytes())
        }
        ScalarEdit::DirectFixedAmount { task_guid, cost_guid, expected_storage, value_storage } => {
            let task = task_for(all, task_guid)?;
            let cost = task
                .direct_costs
                .iter()
                .find(|c| c.guid.eq_ignore_ascii_case(cost_guid))
                .ok_or_else(|| fail("invalid_identity", "Fixed cost GUID is absent from the selected task"))?;
            if cost.type_storage_value != 1 {
                return Err(fail("unsupported_layout", "Only the admitted direct Fixed record is editable"));
            }
            let (_, at) = string_at(data, checked_add(cost.offset, 28)?)?;
            let (before, after) = number(*expected_storage, *value_storage)?;
            (task, "direct_fixed_amount", at, before, after)
        }
        ScalarEdit::AssignmentPlannedUnits { task_guid, assignment_ordinal, resource_guid, profile_guid, expected_storage, value_storage }
        | ScalarEdit::AssignmentPlannedRate { task_guid, assignment_ordinal, resource_guid, profile_guid, expected_storage, value_storage } => {
            let task = task_for(all, task_guid)?;
            let assignment =
                task.assignments.get(*assignment_ordinal).ok_or_else(|| fail("invalid_identity", "Assignment ordinal is outside the selected task"))?;
            if !assignment.resource_guid.eq_ignore_ascii_case(resource_guid) || !assignment.profile_guid.eq_ignore_ascii_case(profile_guid) {
                return Err(fail("invalid_identity", "Assignment resource/profile guard does not match the source ordinal"));
            }
            let (before, after) = number(*expected_storage, *value_storage)?;
            let (field, offset) =
                if matches!(edit, ScalarEdit::AssignmentPlannedUnits { .. }) { ("assignment_planned_units", 163) } else { ("assignment_planned_rate", 171) };
            (task, field, checked_add(assignment.offset, offset)?, before, after)
        }
    };
    if bytes(data, at, 8)? != before {
        return Err(fail("stale_value", "Expected scalar bytes differ from the source"));
    }
    Ok(Change { field, task_guid: task.guid.clone(), decoded_tasks_offset: at, before_hex: hex(&before), after_hex: hex(&after) })
}
/// Produce a new byte vector, never a filesystem write or a native compatibility claim.
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    edits: &[ScalarEdit],
    mut keep_going: impl FnMut(f32) -> bool,
) -> std::result::Result<Candidate, FormatError> {
    inner(raw, expected_source_sha256, edits, &mut keep_going).map_err(format_error)
}
fn inner(raw: &[u8], expected: &str, edits: &[ScalarEdit], callback: &mut dyn FnMut(f32) -> bool) -> Result<Candidate> {
    if raw.len() > FILE_LIMIT {
        return Err(fail("limit", "Source exceeds the 256 MiB file cap"));
    }
    if edits.len() > 256 {
        return Err(fail("limit", "Choose at most 256 guarded scalar edits"));
    }
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(fail("invalid_identity", "Expected source SHA-256 must contain 64 hexadecimal digits"));
    }
    let source_hash = hash(raw, callback)?;
    if !source_hash.eq_ignore_ascii_case(expected) {
        return Err(fail("stale_source", "Source hash changed; reload before editing"));
    }
    let c = container(raw, callback)?;
    let mut data = c.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
    let evidence = inspect_container(c, source_hash.clone(), false, callback)?;
    let mut changes = Vec::new();
    let mut occupied = BTreeSet::new();
    for edit in edits {
        check(callback, 0.3)?;
        let change = patch(&data, &evidence.tasks, edit)?;
        for at in change.decoded_tasks_offset..checked_add(change.decoded_tasks_offset, 8)? {
            if !occupied.insert(at) {
                return Err(fail("overlapping_edits", "Scalar edits overlap or repeat"));
            }
        }
        changes.push(change);
    }
    for (edit, change) in edits.iter().zip(&changes) {
        let value = match edit {
            ScalarEdit::TaskStart { value_storage, .. } | ScalarEdit::TaskDuration { value_storage, .. } => value_storage.to_le_bytes(),
            ScalarEdit::DirectFixedAmount { value_storage, .. }
            | ScalarEdit::AssignmentPlannedUnits { value_storage, .. }
            | ScalarEdit::AssignmentPlannedRate { value_storage, .. } => value_storage.to_le_bytes(),
        };
        let end = checked_add(change.decoded_tasks_offset, 8)?;
        data.get_mut(change.decoded_tasks_offset..end).ok_or_else(|| fail("truncated", "Scalar range disappeared"))?.copy_from_slice(&value);
    }
    check(callback, 0.4)?;
    let (output, output_hash, guards) = pack_tasks(raw, &data, &evidence, !edits.is_empty(), callback)?;
    Ok(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 1,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            changes,
            sections: guards,
            internal_reparse_passed: true,
            native_reopen_verified: false,
            derived_schedule_consistency_verified: false,
            limitations: vec![
                "Candidate storage edits only; source-application open/save/reopen is not certified.",
                "Task start/duration edits do not recalculate dependent dates, resource assignments, summaries or constraints.",
                "Quantity and utilization-rate storage are independent guarded fields; no automatic native quantity/duration/rate coordination is certified.",
                "No currency/unit conversion, dependency rescheduling, record creation/deletion or general SP writing is admitted.",
                "Opaque section payloads are retained byte-for-byte, not semantically validated.",
            ],
        },
    })
}

/// Repack only Tasks and verify every other stored payload before a full reparse.
pub(super) fn pack_tasks(
    raw: &[u8],
    data: &[u8],
    evidence: &Evidence,
    changed: bool,
    callback: &mut dyn FnMut(f32) -> bool,
) -> Result<(Vec<u8>, String, Vec<SectionGuard>)> {
    // No-edit controls preserve the source compressor's exact bytes.
    let replacements = if changed { vec![("Tasks", data)] } else { Vec::new() };
    pack_sections(raw, evidence, &replacements, callback)
}

/// Repack an explicitly bounded set of decoded sections, preserving other stored bytes.
pub(super) fn pack_sections(
    raw: &[u8],
    evidence: &Evidence,
    replacements: &[(&str, &[u8])],
    callback: &mut dyn FnMut(f32) -> bool,
) -> Result<(Vec<u8>, String, Vec<SectionGuard>)> {
    if replacements.len() > 2 {
        return Err(fail("limit", "At most two section replacements are admitted"));
    }
    let mut encoded = BTreeMap::new();
    let mut output_size = raw.len();
    for (name, data) in replacements {
        if !matches!(*name, "Tasks" | "PresentationData") || data.len() > STREAM_LIMIT || encoded.contains_key(*name) {
            return Err(fail("unsupported_layout", "Invalid or oversized section replacement"));
        }
        let section = evidence.sections.get(*name).ok_or_else(|| fail("missing_dependency", "Replacement section missing"))?;
        let prefix_len = checked_add(name.len(), 2)?;
        let mut prefix = vec![1, u8::try_from(name.len()).map_err(|_| fail("limit", "Section name length"))?];
        prefix.extend_from_slice(name.as_bytes());
        marker(raw, section.file_offset, &prefix)?;
        let compressed = miniz_oxide::deflate::compress_to_vec_zlib(data, 6);
        let new_stored = checked_add(prefix_len, compressed.len())?;
        output_size = checked_add(output_size.checked_sub(section.stored_bytes).ok_or_else(|| fail("limit", "Section size"))?, new_stored)?;
        encoded.insert(*name, (prefix, compressed));
    }
    check(callback, 0.5)?;
    if output_size > FILE_LIMIT {
        return Err(fail("limit", "Candidate exceeds the 256 MiB file cap"));
    }
    let first = usize::try_from(u64_at(raw, 80)?).map_err(|_| fail("limit", "First section offset"))?;
    let mut output = Vec::new();
    output.try_reserve_exact(output_size).map_err(|_| fail("limit", "Candidate allocation"))?;
    output.extend_from_slice(bytes(raw, 0, first)?);
    for index in 0usize..48 {
        check(callback, 0.6)?;
        let slot = checked_add(80, index.checked_mul(48).ok_or_else(|| fail("limit", "Directory slot"))?)?;
        let start = usize::try_from(u64_at(raw, slot)?).map_err(|_| fail("limit", "Section offset"))?;
        let end = if index == 47 { raw.len() } else { usize::try_from(u64_at(raw, checked_add(slot, 48)?)?).map_err(|_| fail("limit", "Section end"))? };
        let offset = u64::try_from(output.len()).map_err(|_| fail("limit", "Output offset"))?.to_le_bytes();
        output.get_mut(slot..checked_add(slot, 8)?).ok_or_else(|| fail("truncated", "Directory write range"))?.copy_from_slice(&offset);
        let field = bytes(raw, slot.checked_sub(40).ok_or_else(|| fail("limit", "Section name slot"))?, 40)?;
        let name_end = field.iter().position(|b| *b == 0).ok_or_else(|| fail("invalid_directory", "Section name terminator"))?;
        let name = std::str::from_utf8(bytes(field, 0, name_end)?).map_err(|_| fail("invalid_directory", "Section name encoding"))?;
        if let Some((prefix, compressed)) = encoded.get(name) {
            output.extend_from_slice(prefix);
            output.extend_from_slice(compressed);
        } else {
            output.extend_from_slice(bytes(raw, start, end.checked_sub(start).ok_or_else(|| fail("limit", "Section length"))?)?);
        }
    }
    if output.len() != output_size {
        return Err(fail("invalid_output", "Candidate length differs from plan"));
    }
    let after = container(&output, callback)?;
    let mut guards = Vec::new();
    for (name, old) in &evidence.sections {
        let new = after.sections.get(name).ok_or_else(|| fail("invalid_output", "A section disappeared"))?;
        let old_bytes = bytes(raw, old.file_offset, old.stored_bytes)?;
        let new_bytes = bytes(&output, new.file_offset, new.stored_bytes)?;
        let preserved = old_bytes == new_bytes;
        if !encoded.contains_key(name.as_str()) && !preserved {
            return Err(fail("invalid_output", "An untouched section changed"));
        }
        guards.push(SectionGuard {
            name: name.clone(),
            source_stored_sha256: hash(old_bytes, callback)?,
            output_stored_sha256: hash(new_bytes, callback)?,
            stored_bytes_preserved: preserved,
        });
    }
    let output_hash = hash(&output, callback)?;
    let _ = inspect_container(after, output_hash.clone(), false, callback)?;
    check(callback, 1.)?;
    Ok((output, output_hash, guards))
}
