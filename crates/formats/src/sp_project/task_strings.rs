//! Guarded variable-length task text candidates. Native compatibility is a separate gate.
use super::*;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "field", rename_all = "snake_case", deny_unknown_fields)]
pub enum TextEdit {
    TaskName { task_guid: String, expected: String, value: String },
    TaskDisplayId { task_guid: String, expected: String, value: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextField {
    TaskName,
    TaskDisplayId,
}

#[derive(Debug, Serialize)]
pub struct TextChange {
    pub field: TextField,
    pub task_guid: String,
    pub source_decoded_offset: usize,
    pub source_encoded_bytes: usize,
    pub output_encoded_bytes: usize,
    pub expected: String,
    pub value: String,
}

#[derive(Serialize)]
pub struct TextReceipt {
    pub version: u32,
    pub source_sha256: String,
    pub output_sha256: String,
    pub admitted_build: &'static str,
    pub changes: Vec<TextChange>,
    pub sections: Vec<writer::SectionGuard>,
    pub internal_reparse_passed: bool,
    pub native_reopen_verified: bool,
    pub limitations: Vec<&'static str>,
}

pub struct TextCandidate {
    pub bytes: Vec<u8>,
    pub receipt: TextReceipt,
}

fn encode(value: &str) -> Result<Vec<u8>> {
    // Bound UTF-8 before scanning or allocating UTF-16 for hostile requests.
    if value.len() > 65_536 || value.contains('\0') {
        return Err(fail("unsupported_value", "Task text must be bounded and contain no NUL"));
    }
    let units = value.encode_utf16().count();
    if units > 16_384 {
        return Err(fail("limit", "Task text exceeds 16384 UTF-16 code units"));
    }
    let mut encoded = vec![255, 254, 255];
    if units < 255 {
        encoded.push(u8::try_from(units).map_err(|_| fail("limit", "Short text length"))?);
    } else {
        encoded.push(255);
        encoded.extend(u16::try_from(units).map_err(|_| fail("limit", "Extended text length"))?.to_le_bytes());
    }
    for unit in value.encode_utf16() {
        encoded.extend(unit.to_le_bytes());
    }
    Ok(encoded)
}

/// Create a separate storage candidate; never modify the source or certify native reopening.
pub fn candidate(
    raw: &[u8],
    expected_source_sha256: &str,
    edits: &[TextEdit],
    mut keep_going: impl FnMut(f32) -> bool,
) -> std::result::Result<TextCandidate, FormatError> {
    inner(raw, expected_source_sha256, edits, &mut keep_going).map_err(format_error)
}

fn inner(raw: &[u8], expected_hash: &str, edits: &[TextEdit], callback: &mut dyn FnMut(f32) -> bool) -> Result<TextCandidate> {
    if raw.len() > FILE_LIMIT || edits.len() > 256 {
        return Err(fail("limit", "Choose a source up to 256 MiB and at most 256 text edits"));
    }
    if expected_hash.len() != 64 || !expected_hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(fail("invalid_identity", "Expected source SHA-256 must contain 64 hexadecimal digits"));
    }
    let source_hash = writer::hash(raw, callback)?;
    if !source_hash.eq_ignore_ascii_case(expected_hash) {
        return Err(fail("stale_source", "Source hash changed; reload before editing"));
    }
    let source = container(raw, callback)?;
    let data = source.decoded.get("Tasks").ok_or_else(|| fail("missing_dependency", "Tasks section missing"))?.clone();
    let evidence = inspect_container(source, source_hash.clone(), false, callback)?;
    let mut changes = Vec::new();
    let mut replacements = Vec::new();
    let mut occupied = BTreeSet::new();
    let mut output_size = data.len();
    for edit in edits {
        check(callback, 0.3)?;
        let (id, expected, value, is_name) = match edit {
            TextEdit::TaskName { task_guid, expected, value } => (task_guid, expected, value, true),
            TextEdit::TaskDisplayId { task_guid, expected, value } => (task_guid, expected, value, false),
        };
        let task = writer::task_for(&evidence.tasks, id)?;
        let (at, end, observed, field) = if is_name {
            (task.name_offset, task.name_end, &task.name, TextField::TaskName)
        } else {
            (task.display_id_offset, task.display_id_end, &task.display_id, TextField::TaskDisplayId)
        };
        if observed != expected {
            return Err(fail("stale_source", "Expected task text differs from the source"));
        }
        if !occupied.insert((task.guid.clone(), is_name)) {
            return Err(fail("overlapping_edits", "A task text field occurs more than once"));
        }
        let encoded = encode(value)?;
        let old_len = end.checked_sub(at).ok_or_else(|| fail("truncated", "Text range"))?;
        // Preserve even a noncanonical source encoding for an unchanged text value.
        let replacement = if value == observed { bytes(&data, at, old_len)?.to_vec() } else { encoded };
        output_size = checked_add(output_size.checked_sub(old_len).ok_or_else(|| fail("limit", "Text size"))?, replacement.len())?;
        if output_size > FILE_LIMIT {
            return Err(fail("limit", "Decoded candidate exceeds 256 MiB"));
        }
        changes.push(TextChange {
            field,
            task_guid: task.guid.clone(),
            source_decoded_offset: at,
            source_encoded_bytes: old_len,
            output_encoded_bytes: replacement.len(),
            expected: expected.clone(),
            value: value.clone(),
        });
        replacements.push((at, end, replacement));
    }
    replacements.sort_by_key(|r| r.0);
    let mut updated = Vec::new();
    updated.try_reserve_exact(output_size).map_err(|_| fail("limit", "Text candidate allocation"))?;
    let mut cursor = 0usize;
    for (at, end, replacement) in replacements {
        let len = at.checked_sub(cursor).ok_or_else(|| fail("overlapping_edits", "Task text ranges overlap"))?;
        updated.extend_from_slice(bytes(&data, cursor, len)?);
        updated.extend(replacement);
        cursor = end;
    }
    updated.extend_from_slice(bytes(&data, cursor, data.len().checked_sub(cursor).ok_or_else(|| fail("truncated", "Text tail"))?)?);
    if updated.len() != output_size {
        return Err(fail("invalid_output", "Text candidate length differs from plan"));
    }
    let (output, output_hash, guards) = writer::pack_tasks(raw, &updated, &evidence, updated != data, callback)?;
    // Reparse verifies structure; also bind every requested semantic value to its task identity.
    let parsed = inspect_container(container(&output, callback)?, output_hash.clone(), false, callback)?;
    for change in &changes {
        let task = writer::task_for(&parsed.tasks, &change.task_guid)?;
        let actual = match change.field {
            TextField::TaskName => &task.name,
            TextField::TaskDisplayId => &task.display_id,
        };
        if actual != &change.value {
            return Err(fail("invalid_output", "Reparsed task text differs from the request"));
        }
    }
    Ok(TextCandidate {
        bytes: output,
        receipt: TextReceipt {
            version: 1,
            source_sha256: source_hash,
            output_sha256: output_hash,
            admitted_build: "6.5.5.28",
            changes,
            sections: guards,
            internal_reparse_passed: true,
            native_reopen_verified: false,
            limitations: vec![
                "Experimental task name and display ID storage edits only; native open/save/reopen is a separate gate.",
                "Other section payloads are preserved, not semantically validated; external text references are not rewritten.",
                "No task creation, deletion, hierarchy changes, uniqueness policy or scheduling is admitted.",
            ],
        },
    })
}
