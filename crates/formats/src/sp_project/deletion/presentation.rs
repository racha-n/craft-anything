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
fn validate(data: &[u8], tasks: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<Layout> {
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
            Some(id)
        }
        _ => return Err(fail("unsupported_layout", "Deletion admits at most one selected task")),
    };
    let mut identities: Vec<Vec<u8>> = tasks
        .iter()
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
