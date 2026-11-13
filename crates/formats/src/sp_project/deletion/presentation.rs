use super::*;
const MAP: &[u8] = &[0xf2, 0x96, 0x1c, 0x49, 0x50, 0xd7, 0xfc, 0xfa];
const MAP_END: &[u8] = &[0xac, 0xc1, 0x97, 0x5a, 0xf1, 0xe9, 0x13, 0x41];
const ROW: &[u8] = &[0x12, 0x24, 0xb4, 0x13, 0x17, 0, 0, 0];
const ROW_END: &[u8] = &[0xae, 0xa6, 0x38, 0x8a];
const SELECT: &[u8] = &[0xd0, 0xbc, 0xa1, 0x06];
const SELECT_END: &[u8] = &[0x2c, 0x57, 0xd9, 0xda];
struct Layout {
    map: usize,
    selected: Option<Vec<u8>>,
    entries: Vec<(Vec<u8>, usize)>,
}
pub(super) fn validate_source(data: &[u8], tasks: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<()> {
    validate(data, tasks, callback).map(|_| ())
}
fn validate(data: &[u8], tasks: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<Layout> {
    validate_without(data, tasks, &BTreeSet::new(), callback)
}
fn validate_without(data: &[u8], tasks: &[Task], missing: &BTreeSet<&str>, callback: &mut dyn FnMut(f32) -> bool) -> Result<Layout> {
    if data.len() > STREAM_LIMIT {
        return Err(fail("limit", "PresentationData exceeds 16 MiB"));
    }
    marker(data, 16, SELECT)?;
    let selected = match u32_at(data, 20)? {
        0 => {
            marker(data, 0, &[0; 16])?;
            marker(data, 24, SELECT_END)?;
            None
        }
        1 => {
            let id = bytes(data, 24, 16)?.to_vec();
            marker(data, 0, &id)?;
            marker(data, 40, &[0; 4])?;
            marker(data, 44, SELECT_END)?;
            let guid = guid_at(&id, 0)?;
            let _ = writer::task_for(tasks, &guid)?;
            if missing.contains(guid.as_str()) {
                return Err(fail("unsupported_layout", "A missing created row cannot be the saved selection"));
            }
            Some(id)
        }
        _ => return Err(fail("unsupported_layout", "Deletion admits at most one selected task")),
    };
    let mut identities: Vec<Vec<u8>> = tasks
        .iter()
        .filter(|t| !missing.contains(t.guid.as_str()))
        .map(|t| {
            // Raw storage order is also the presentation row-map key order.
            let text = t.guid.replace('-', "");
            let raw = (0..32)
                .step_by(2)
                .map(|i| {
                    let end = checked_add(i, 2)?;
                    let pair = text.get(i..end).ok_or_else(|| fail("invalid_identity", "Task GUID encoding"))?;
                    u8::from_str_radix(pair, 16).map_err(|_| fail("invalid_identity", "Task GUID encoding"))
                })
                .collect::<Result<Vec<_>>>()?;
            let mut storage = Vec::with_capacity(16);
            storage.extend(raw.get(0..4).ok_or_else(|| fail("invalid_identity", "GUID bytes"))?.iter().rev());
            storage.extend(raw.get(4..6).ok_or_else(|| fail("invalid_identity", "GUID bytes"))?.iter().rev());
            storage.extend(raw.get(6..8).ok_or_else(|| fail("invalid_identity", "GUID bytes"))?.iter().rev());
            storage.extend_from_slice(raw.get(8..16).ok_or_else(|| fail("invalid_identity", "GUID bytes"))?);
            Ok(storage)
        })
        .collect::<Result<Vec<_>>>()?;
    identities.sort();
    let mut candidates = 0usize;
    let mut matched = None;
    for (at, part) in data.windows(MAP.len()).enumerate() {
        if at % 65_536 == 0 {
            check(callback, 0.4)?;
        }
        if part != MAP {
            continue;
        }
        candidates = checked_add(candidates, 1)?;
        if candidates > 8 {
            return Err(fail("limit", "Presentation row-map candidate budget exceeded"));
        }
        if usize::try_from(u32_at(data, checked_add(at, 8)?)?).map_err(|_| fail("limit", "Row count"))? != identities.len() {
            continue;
        }
        let mut entries = Vec::new();
        let mut valid = true;
        for (index, id) in identities.iter().enumerate() {
            let offset = checked_add(checked_add(at, 12)?, index.checked_mul(35).ok_or_else(|| fail("limit", "Presentation row offset"))?)?;
            let row = bytes(data, offset, 35)?;
            if row.get(0..16) != Some(id.as_slice()) || row.get(16..24) != Some(ROW) || row.get(31..35) != Some(ROW_END) {
                valid = false;
                break;
            }
            entries.push((id.clone(), offset));
        }
        let end = checked_add(checked_add(at, 12)?, identities.len().checked_mul(35).ok_or_else(|| fail("limit", "Presentation row length"))?)?;
        if !valid || bytes(data, end, 8)? != MAP_END {
            continue;
        }
        if at < if selected.is_some() { 48 } else { 28 } || matched.is_some() {
            return Err(fail("unsupported_layout", "Ambiguous or overlapping presentation row map"));
        }
        matched = Some((at, entries));
    }
    let (map, entries) = matched.ok_or_else(|| fail("unsupported_layout", "No complete admitted task presentation row map"))?;
    Ok(Layout { map, selected, entries })
}

/// Complete only rows absent for validated preceding creation identities.
pub(super) fn complete_created(
    data: &[u8],
    tasks: &[Task],
    known: &[(String, Vec<u8>)],
    callback: &mut dyn FnMut(f32) -> bool,
) -> Result<Option<(Vec<u8>, Vec<String>)>> {
    if known.len() > ordered::ORDER_LIMIT || data.len() > STREAM_LIMIT {
        return Err(fail("limit", "Created presentation row budget exceeded"));
    }
    let mut absent = Vec::new();
    for (guid, raw) in known {
        if raw.len() != 16 {
            return Err(fail("invalid_identity", "Created row identity length"));
        }
        let mut prefix = raw.clone();
        prefix.extend_from_slice(ROW);
        let mut found = false;
        for at in (0..data.len()).step_by(65_536) {
            check(callback, 0.35)?;
            let length = data.len().saturating_sub(at).min(65_559);
            if bytes(data, at, length)?.windows(prefix.len()).any(|part| part == prefix) {
                found = true;
                break;
            }
        }
        if !found {
            absent.push((guid.as_str(), raw.as_slice()));
        }
    }
    let missing = absent.iter().map(|(guid, _)| *guid).collect::<BTreeSet<_>>();
    let layout = validate_without(data, tasks, &missing, callback)?;
    if absent.is_empty() {
        return Ok(None);
    }
    let extra = absent.len().checked_mul(35).ok_or_else(|| fail("limit", "Created row length"))?;
    let length = checked_add(data.len(), extra)?;
    if length > STREAM_LIMIT {
        return Err(fail("limit", "Completed presentation exceeds 16 MiB"));
    }
    let mut rows = Vec::new();
    for (_, at) in &layout.entries {
        rows.push(bytes(data, *at, 35)?.to_vec());
    }
    absent.sort_by(|a, b| a.1.cmp(b.1));
    for (_, raw) in &absent {
        let mut row = raw.to_vec();
        row.extend_from_slice(ROW);
        // Observed unchanged with and without explicit task selection; semantics stay opaque.
        row.extend_from_slice(&[1, 1, 0, 0, 0, 1, 1]);
        row.extend_from_slice(ROW_END);
        rows.push(row);
    }
    rows.sort();
    if rows.len() != tasks.len() {
        return Err(fail("invalid_output", "Completed task row count differs from the inventory"));
    }
    let mut output = Vec::new();
    output.try_reserve_exact(length).map_err(|_| fail("limit", "Presentation completion allocation"))?;
    output.extend_from_slice(bytes(data, 0, checked_add(layout.map, 8)?)?);
    output.extend_from_slice(&u32::try_from(rows.len()).map_err(|_| fail("limit", "Completed row count"))?.to_le_bytes());
    for row in rows {
        check(callback, 0.45)?;
        output.extend_from_slice(&row);
    }
    let old_rows = layout.entries.len().checked_mul(35).ok_or_else(|| fail("limit", "Original row length"))?;
    let tail = checked_add(checked_add(layout.map, 12)?, old_rows)?;
    output.extend_from_slice(bytes(data, tail, data.len().checked_sub(tail).ok_or_else(|| fail("truncated", "Presentation tail"))?)?);
    let final_layout = validate(&output, tasks, callback)?;
    if output.len() != length || final_layout.selected != layout.selected {
        return Err(fail("invalid_output", "Presentation completion changed selection or length"));
    }
    for (id, at) in &layout.entries {
        let new = final_layout.entries.iter().find(|(key, _)| key == id).ok_or_else(|| fail("invalid_output", "Retained row disappeared"))?.1;
        if bytes(data, *at, 35)? != bytes(&output, new, 35)? {
            return Err(fail("invalid_output", "Presentation completion changed an existing row"));
        }
    }
    Ok(Some((output, absent.into_iter().map(|(guid, _)| guid.to_owned()).collect())))
}

pub(super) fn remove(data: &[u8], before: &[Task], after: &[Task], primary: &[u8], callback: &mut dyn FnMut(f32) -> bool) -> Result<(Vec<u8>, bool)> {
    let layout = validate(data, before, callback)?;
    let row =
        layout.entries.iter().find(|(id, _)| id.as_slice() == primary).ok_or_else(|| fail("invalid_identity", "Deleted task presentation row missing"))?.1;
    let count = u32::try_from(after.len()).map_err(|_| fail("limit", "Presentation row count"))?;
    let mut edits = vec![(checked_add(layout.map, 8)?, 4, count.to_le_bytes().to_vec()), (row, 35, Vec::new())];
    let cleared = layout.selected.as_deref() == Some(primary);
    if cleared {
        edits.extend([(0, 16, vec![0; 16]), (20, 24, vec![0; 4])]);
    }
    edits.sort_by_key(|e| e.0);
    let mut output = Vec::new();
    output.try_reserve_exact(data.len()).map_err(|_| fail("limit", "Presentation deletion allocation"))?;
    let mut previous = 0;
    for (at, length, replacement) in edits {
        check(callback, 0.5)?;
        let length_before = at.checked_sub(previous).ok_or_else(|| fail("invalid_output", "Overlapping presentation edits"))?;
        output.extend_from_slice(bytes(data, previous, length_before)?);
        output.extend_from_slice(&replacement);
        previous = checked_add(at, length)?;
    }
    output.extend_from_slice(bytes(data, previous, data.len().checked_sub(previous).ok_or_else(|| fail("truncated", "Presentation tail"))?)?);
    let final_layout = validate(&output, after, callback)?;
    let retained: Vec<_> = layout.entries.iter().filter(|(id, _)| id.as_slice() != primary).collect();
    for ((id, old), (new_id, new)) in retained.iter().copied().zip(&final_layout.entries) {
        if id != new_id || bytes(data, *old, 35)? != bytes(&output, *new, 35)? {
            return Err(fail("invalid_output", "Surviving presentation row changed"));
        }
    }
    Ok((output, cleared))
}
