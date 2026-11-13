//! Internal chronological completion for identities established by preceding creation stages.
use super::*;
#[cfg(test)]
#[path = "row_completion_tests.rs"]
mod tests;

#[derive(Clone, Serialize)]
pub struct CreatedIdentity {
    pub task_guid: String,
    pub supply_chain_id: String,
}
#[derive(Serialize)]
pub struct Receipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub added_rows: Vec<CreatedIdentity>,
    pub observed_row_opaque_hex: &'static str,
    pub task_count: usize,
    pub row_count_before: usize,
    pub row_count_after: usize,
    pub sections: Vec<writer::SectionGuard>,
    pub identity_pairs_validated: bool,
    pub internal_reparse_passed: bool,
    pub existing_rows_preserved: bool,
    pub selection_preserved: bool,
    pub other_presentation_bytes_preserved: bool,
    pub native_reopen_verified: bool,
    pub native_transaction_history_reproduced: bool,
    pub limitations: Vec<&'static str>,
}
pub struct Candidate {
    pub bytes: Vec<u8>,
    pub receipt: Receipt,
}

// No standalone tool exposes caller-supplied provenance. Replay derives these pairs
// from preceding source-bound creation stages, removing identities when deleted.
pub(crate) fn candidate(
    raw: &[u8],
    expected: &str,
    created: &[CreatedIdentity],
    mut keep: impl FnMut(f32) -> bool,
) -> std::result::Result<Option<Candidate>, FormatError> {
    inner(raw, expected, created, &mut keep).map_err(format_error)
}
pub(in crate::sp_project) fn inner(
    raw: &[u8],
    expected: &str,
    created: &[CreatedIdentity],
    callback: &mut dyn FnMut(f32) -> bool,
) -> Result<Option<Candidate>> {
    if created.is_empty() {
        return Ok(None);
    }
    if created.len() > ordered::ORDER_LIMIT {
        return Err(fail("limit", "At most eight active creation identity pairs are admitted"));
    }
    let (source_hash, tasks_data, evidence) = creation::unassigned_roots(raw, expected, callback)?;
    let mut known = Vec::new();
    let mut pairs = BTreeMap::new();
    for identity in created {
        check(callback, 0.2)?;
        let task = writer::task_for(&evidence.tasks, &identity.task_guid)?;
        let supply = guid_at(&tasks_data, checked_add(task.name_end, 98)?)?;
        if !supply.eq_ignore_ascii_case(&identity.supply_chain_id) || pairs.contains_key(&task.guid) {
            return Err(fail("invalid_identity", "Created task/Supply Chain provenance differs from the current task body"));
        }
        let raw_guid = bytes(&tasks_data, checked_add(task.offset, 12)?, 16)?.to_vec();
        known.push((task.guid.clone(), raw_guid));
        pairs.insert(task.guid.clone(), CreatedIdentity { task_guid: task.guid.clone(), supply_chain_id: supply });
    }
    let section = evidence.sections.get("PresentationData").ok_or_else(|| fail("missing_dependency", "PresentationData is required for row completion"))?;
    let data = decoded_section(raw, "PresentationData", section, callback)?;
    let Some((updated, added)) = presentation::complete_created(&data, &evidence.tasks, &known, callback)? else {
        return Ok(None);
    };
    let mut added_rows = Vec::new();
    for guid in added {
        let identity = pairs.get(&guid).ok_or_else(|| fail("invalid_identity", "Completed identity lacks creation provenance"))?;
        let task = writer::task_for(&evidence.tasks, &guid)?;
        let primary = bytes(&tasks_data, checked_add(task.offset, 12)?, 16)?;
        let supply = bytes(&tasks_data, checked_add(task.name_end, 98)?, 16)?;
        // An absent row must not hide some other saved identity reference.
        no_references(&data, &needles(primary, supply)?, "PresentationData", callback)?;
        added_rows.push(identity.clone());
    }
    let (output, output_hash, guards) = writer::pack_sections(raw, &evidence, &[("PresentationData", &updated)], callback)?;
    if guards.iter().any(|s| s.name != "PresentationData" && !s.stored_bytes_preserved) {
        return Err(fail("invalid_output", "Row completion changed another stored section"));
    }
    let parsed = container(&output, callback)?;
    let p = parsed.sections.get("PresentationData").ok_or_else(|| fail("invalid_output", "Completed presentation section disappeared"))?;
    if decoded_section(&output, "PresentationData", p, callback)? != updated {
        return Err(fail("invalid_output", "Completed presentation reparse differs"));
    }
    Ok(Some(Candidate {
        bytes: output,
        receipt: Receipt {
            version: 1,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            row_count_before: evidence.tasks.len().checked_sub(added_rows.len()).ok_or_else(|| fail("limit", "Original row count"))?,
            row_count_after: evidence.tasks.len(),
            task_count: evidence.tasks.len(),
            added_rows,
            observed_row_opaque_hex: "01010000000101",
            sections: guards,
            identity_pairs_validated: true,
            internal_reparse_passed: true,
            existing_rows_preserved: true,
            selection_preserved: true,
            other_presentation_bytes_preserved: true,
            native_reopen_verified: false,
            native_transaction_history_reproduced: false,
            limitations: vec![
                "Internal completion for active identities from preceding creation stages in an admitted unassigned Planned root-task draft.",
                "Only missing known task rows and the map count change; existing rows, saved selection and the other 47 stored sections remain exact.",
                "The observed seven row bytes remain opaque. Native history, broader task layouts, generic saving and scheduling are unadmitted.",
            ],
        },
    }))
}
