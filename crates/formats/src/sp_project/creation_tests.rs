use super::*;
use crate::sp_project::tests::{empty_section, metadata_task_section, project_with_tasks};

fn text(s: &str) -> Vec<u8> {
    let mut b = vec![255, 254, 255, s.encode_utf16().count() as u8];
    b.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    b
}
fn id(byte: u8) -> String {
    guid_at(&[byte; 16], 0).unwrap()
}
fn set(data: &mut [u8], at: usize, value: &[u8]) {
    data[at..at + value.len()].copy_from_slice(value);
}
fn record(byte: u8, start: u64, duration: u64) -> Vec<u8> {
    let original = metadata_task_section(false);
    let mut row = original[32..105].to_vec();
    set(&mut row, 12, &[byte; 16]);
    set(&mut row, 41, &1u32.to_le_bytes());
    set(&mut row, 57, &[0x51; 16]);
    row.extend(text(&format!("Task{byte}")));
    let mut dynamic = vec![0; 184];
    set(&mut dynamic, 0, &start.to_le_bytes());
    set(&mut dynamic, 8, &duration.to_le_bytes());
    for at in [18, 26, 34, 42, 74, 82] {
        set(&mut dynamic, at, &32_535_244_799u64.to_le_bytes());
    }
    dynamic[52] = 1;
    for at in [54, 130] {
        set(&mut dynamic, at, &[0xd0, 0xbc, 0xa1, 0x06]);
        set(&mut dynamic, at + 8, &[0x2c, 0x57, 0xd9, 0xda]);
    }
    set(&mut dynamic, 66, &[0xdb, 0xff, 0x21, 0xbb, 1]);
    set(&mut dynamic, 90, &[0x97, 0xfb, 0x61, 0x1f, 0xb3, 0x0c, 0x9c, 0xd1]);
    set(&mut dynamic, 98, &[byte.wrapping_add(128); 16]);
    set(&mut dynamic, 142, &[0x6f, 0xa3, 0x1d, 0x05]);
    set(&mut dynamic, 163, &[1, 1]);
    set(&mut dynamic, 168, &[255, 254, 255, 0, 1]);
    row.extend(dynamic);
    row.extend([0xd0, 0xbc, 0xa1, 0x06, 0, 0, 0, 0, 0x2c, 0x57, 0xd9, 0xda]);
    row.extend(text(&format!("ID{byte}")));
    let mut tail = vec![0; 121];
    set(&mut tail, 9, &[0x61; 16]);
    set(&mut tail, 25, &1u32.to_le_bytes());
    for at in [29, 37, 45, 53, 93] {
        set(&mut tail, at, &32_535_244_799u64.to_le_bytes());
    }
    tail[61] = 1;
    set(&mut tail, 101, OPEN);
    set(&mut tail, 109, CLOSE);
    set(&mut tail, 113, &[0x92, 0xd4, 0x70, 0xa2]);
    row.extend(tail);
    row
}
fn section(rows: &[Vec<u8>]) -> Vec<u8> {
    let mut result = empty_section();
    result.truncate(28);
    result.extend((rows.len() as u32).to_le_bytes());
    let mut lookup: Vec<usize> = (0..rows.len()).collect();
    lookup.sort_by_key(|i| &rows[*i][12..28]);
    for (r, index) in rows.iter().zip(lookup) {
        result.extend(&r[..r.len() - 4]);
        result.extend((index as u32).to_le_bytes());
    }
    result.extend(CLOSE);
    result
}
fn nested(mut parent: Vec<u8>, children: &[Vec<u8>]) -> Vec<u8> {
    let parsed = tasks(&section(&[parent.clone()]), &mut |_| true).unwrap();
    let at = parsed[0].name_end - 32 + 16;
    parent[at] = 1;
    parent.splice(at + 1..at + 1, section(children)[24..].iter().copied());
    parent
}
fn calendars() -> Vec<u8> {
    let mut b = empty_section();
    b.truncate(28);
    b.extend(1u32.to_le_bytes());
    b.extend([0x1d, 0x26, 0xec, 0x7f]);
    b.extend([0; 8]);
    b.extend([0x51; 16]);
    b.extend(text("Default"));
    b.extend(OPEN);
    b.extend(0u32.to_le_bytes());
    b.extend(CLOSE);
    b.extend([0xa9, 0x5c, 0xcd, 0x47]);
    for day in [0x53, 0x52, 0x52, 0x52, 0x52, 0x52, 0x53] {
        b.extend([day; 16]);
    }
    b.extend([0x25, 0xf2, 0x78, 0x41]);
    b.extend(OPEN);
    b.extend(0u32.to_le_bytes());
    b.extend(CLOSE);
    b.extend(OPEN);
    b.extend(2u32.to_le_bytes());
    for (byte, kind, slots) in [(0x52, 1u32, vec![(118_800i64, 147_600i64)]), (0x53, 2, vec![])] {
        b.extend([0x1d, 0xc2, 0xf4, 0xc4]);
        b.extend([byte; 16]);
        b.extend(text("Day"));
        b.extend([0; 12]);
        b.extend(kind.to_le_bytes());
        b.extend((slots.len() as u32).to_le_bytes());
        for (a, z) in slots {
            b.extend(a.to_le_bytes());
            b.extend(z.to_le_bytes());
        }
        b.extend([0x19, 0x06, 0x8d, 0x63]);
        b.extend(u32::from(byte).to_le_bytes());
    }
    b.extend(CLOSE);
    b.extend([0x19, 0xea, 0x70, 0x7d]);
    b.extend([0; 4]);
    b.extend(CLOSE);
    b.extend([0x51; 16]);
    b
}
fn states() -> Vec<u8> {
    let mut b = empty_section();
    b.truncate(28);
    b.extend(3u32.to_le_bytes());
    for kind in 1u8..=3 {
        b.extend([0xf7, 0x38, 0xab, 0x60]);
        b.extend([0; 8]);
        b.extend([0x60 + kind; 16]);
        b.extend(text("State"));
        b.extend([1, 1]);
        b.extend(u32::from(kind).to_le_bytes());
        b.extend([0; 12]);
        b.extend([0x33, 0x58, 0x12, 0x76]);
        b.extend(u32::from(kind - 1).to_le_bytes());
    }
    b.extend(CLOSE);
    b
}
fn project_parts(task_data: &[u8], calendar_data: &[u8], state_data: &[u8]) -> Vec<u8> {
    let old = project_with_tasks(empty_section(), task_data.to_vec());
    let c = container(&old, &mut |_| true).unwrap();
    let mut b = old[..2344].to_vec();
    let mut slots: Vec<_> = c.sections.iter().collect();
    slots.sort_by_key(|(_, s)| s.file_offset);
    for (i, (old_name, s)) in slots.into_iter().enumerate() {
        let name = if i == 8 { "Task States" } else { old_name.as_str() };
        b[40 + i * 48..80 + i * 48].fill(0);
        b[40 + i * 48..40 + i * 48 + name.len()].copy_from_slice(name.as_bytes());
        let at = b.len() as u64;
        set(&mut b, 80 + i * 48, &at.to_le_bytes());
        if name == "Calendars" || name == "Task States" {
            b.extend([1, name.len() as u8]);
            b.extend(name.as_bytes());
            b.extend(miniz_oxide::deflate::compress_to_vec_zlib(if name == "Calendars" { calendar_data } else { state_data }, 6));
        } else {
            b.extend(&old[s.file_offset..s.file_offset + s.stored_bytes]);
        }
    }
    b
}
fn project(rows: &[Vec<u8>]) -> Vec<u8> {
    project_parts(&section(rows), &calendars(), &states())
}
fn request(anchor: u8, parent: Option<u8>, index: usize) -> InsertAbove {
    InsertAbove {
        template_task_guid: id(0x11),
        anchor_task_guid: id(anchor),
        expected_parent_guid: parent.map(id),
        expected_sibling_index: index,
        task_guid: id(0x77),
        supply_chain_id: id(0x78),
        name: "New_\u{0e07}\u{0e32}\u{0e19}_\u{6881}_\u{1f600}".into(),
        display_id: "NEW-1".into(),
    }
}
fn apply(raw: &[u8], r: &InsertAbove) -> Candidate {
    candidate(raw, &writer::hash(raw, &mut |_| true).unwrap(), r, |_| true).unwrap()
}
fn payload(raw: &[u8]) -> Vec<u8> {
    container(raw, &mut |_| true).unwrap().decoded.remove("Tasks").unwrap()
}
const MON: u64 = 1_794_214_800;
const WED: u64 = 1_794_387_600;

#[test]
fn root_insertion_uses_default_duration_anchor_date_and_independent_identities() {
    let raw = project(&[record(0x44, WED, DURATION), record(0x11, MON, DURATION), record(0x22, WED, 57_600)]);
    let r = request(0x22, None, 2);
    let result = apply(&raw, &r);
    let data = payload(&result.bytes);
    let parsed = tasks(&data, &mut |_| true).unwrap();
    assert_eq!(parsed.iter().map(|t| t.guid.clone()).collect::<Vec<_>>(), [id(0x44), id(0x11), id(0x77), id(0x22)]);
    let added = &parsed[2];
    assert_eq!(added.name, r.name);
    assert_eq!(added.display_id, r.display_id);
    assert_eq!(added.planned_start_wallclock_scalar, WED);
    assert_eq!(added.duration_storage_scalar, DURATION);
    assert_eq!(guid_at(&data, added.name_end + 98).unwrap(), id(0x78));
    assert!(result.receipt.sections.iter().filter(|s| s.name != "Tasks").all(|s| s.stored_bytes_preserved));
    assert!(result.receipt.existing_task_bodies_preserved && result.receipt.task_state_definition_validated);
    assert!(!result.receipt.native_reopen_verified && !result.receipt.native_transaction_history_reproduced && !result.receipt.native_row_identity_verified);
    assert_eq!(task_lookup::audit(&data, &parsed, &mut |_| true).unwrap().status, task_lookup::Status::Valid);
}
#[test]
fn child_insertion_preserves_parent_body_and_nonidentity_collection_vectors() {
    let raw = project(&[nested(record(0x44, WED, DURATION), &[record(0x22, WED, 57_600), record(0x11, MON, DURATION)])]);
    let r = request(0x11, Some(0x44), 1);
    let result = apply(&raw, &r);
    let data = payload(&result.bytes);
    let parsed = tasks(&data, &mut |_| true).unwrap();
    assert_eq!(parsed[2].guid, id(0x77));
    assert_eq!(parsed[2].parent_guid, Some(id(0x44)));
    assert_eq!(parsed[2].planned_start_wallclock_scalar, MON);
    assert_eq!(parsed[0].planned_start_wallclock_scalar, WED);
    hierarchy::verify_retained_bodies(&payload(&raw), &tasks(&payload(&raw), &mut |_| true).unwrap(), &data, &parsed, &mut |_| true).unwrap();
}
#[test]
fn rejects_stale_hash_parent_position_and_nonleaf_anchors_without_mutating_source() {
    let raw = project(&[nested(record(0x44, MON, DURATION), &[record(0x11, MON, DURATION)]), record(0x22, WED, DURATION)]);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    let original = raw.clone();
    for r in [request(0x11, None, 0), request(0x11, Some(0x44), 1), request(0x44, None, 0), request(0x55, None, 0)] {
        assert!(candidate(&raw, &hash, &r, |_| true).is_err());
    }
    assert!(candidate(&raw, &"0".repeat(64), &request(0x22, None, 1), |_| true).is_err());
    assert_eq!(raw, original);
}
#[test]
fn refuses_identity_collisions_nil_malformed_and_duplicate_display_ids() {
    let raw = project(&[record(0x11, MON, DURATION)]);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    let good = request(0x11, None, 0);
    for identity in [id(0x11), id(0x91), id(0x61), id(0x51), id(0), "bad".into(), "\u{6881}".repeat(12)] {
        for task in [true, false] {
            let mut r = good.clone();
            if task {
                r.task_guid = identity.clone()
            } else {
                r.supply_chain_id = identity.clone()
            }
            assert!(candidate(&raw, &hash, &r, |_| true).is_err());
        }
    }
    let mut r = good.clone();
    r.supply_chain_id = r.task_guid.clone();
    assert!(candidate(&raw, &hash, &r, |_| true).is_err());
    r = good;
    r.display_id = "id17".into();
    assert!(candidate(&raw, &hash, &r, |_| true).is_err());
}
#[test]
fn rejects_unknown_default_bytes_and_bad_lookup_indices() {
    let base = record(0x11, MON, DURATION);
    let data = section(std::slice::from_ref(&base));
    let task = tasks(&data, &mut |_| true).unwrap().remove(0);
    for at in [task.name_end + 52, task.name_end + 163, task.display_id_end + 61, task.offset + 8] {
        let mut bad = data.clone();
        bad[at] ^= 1;
        let raw = project_parts(&bad, &calendars(), &states());
        let hash = writer::hash(&raw, &mut |_| true).unwrap();
        assert!(candidate(&raw, &hash, &request(0x11, None, 0), |_| true).is_err());
    }
    let mut bad = data;
    let end = bad.len() - 8;
    bad[end..end + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let raw = project_parts(&bad, &calendars(), &states());
    assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
}
#[test]
fn rejects_unresolved_state_references_and_unobserved_calendars() {
    let data = section(&[record(0x11, MON, DURATION)]);
    let task = tasks(&data, &mut |_| true).unwrap().remove(0);
    let mut bad = data.clone();
    bad[task.display_id_end + 9..task.display_id_end + 25].fill(0x62);
    let raw = project_parts(&bad, &calendars(), &states());
    assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
    let mut cal = calendars();
    let start = cal.windows(8).position(|w| w == 147_600i64.to_le_bytes()).unwrap();
    set(&mut cal, start, &145_800i64.to_le_bytes());
    let raw = project_parts(&data, &cal, &states());
    assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
    for start in [MON + 60, MON - 86_400, 32_535_244_799] {
        let raw = project(&[record(0x11, start, DURATION)]);
        assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
    }
}
#[test]
fn bounds_strings_and_preserves_extended_unicode() {
    let raw = project(&[record(0x11, MON, DURATION)]);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    for (name, display) in
        [(String::new(), "X".into()), ("X".into(), " ".into()), ("X\0Y".into(), "X".into()), ("X".repeat(65_537), "X".into()), ("X".into(), "X".repeat(257))]
    {
        let mut r = request(0x11, None, 0);
        r.name = name;
        r.display_id = display;
        assert!(candidate(&raw, &hash, &r, |_| true).is_err());
    }
    let mut r = request(0x11, None, 0);
    r.name = "\u{1f600}".repeat(128);
    let result = apply(&raw, &r);
    assert_eq!(tasks(&payload(&result.bytes), &mut |_| true).unwrap()[0].name, r.name);
}
#[test]
fn refuses_task_limit_and_honors_cancellation() {
    let mut rows = Vec::new();
    for n in 1u16..=512 {
        let mut row = record(0x11, MON, DURATION);
        let mut guid = [0; 16];
        guid[..2].copy_from_slice(&n.to_le_bytes());
        set(&mut row, 12, &guid);
        rows.push(row);
    }
    let raw = project(&rows);
    let r = request(0x11, None, 0);
    let error = candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &r, |_| true).err().unwrap();
    assert!(error.to_string().contains("512"));
    let raw = project(&[record(0x11, MON, DURATION)]);
    let hash = writer::hash(&raw, &mut |_| true).unwrap();
    for allowed in [0, 5, 25] {
        let mut calls = 0;
        let error = candidate(&raw, &hash, &r, |_| {
            calls += 1;
            calls <= allowed
        })
        .err()
        .unwrap();
        assert!(error.to_string().contains("cancelled"));
    }
}

#[test]
fn rejects_source_nil_primary_and_duplicate_or_cross_class_supply_ids() {
    let first = record(0x11, MON, DURATION);
    let second = record(0x22, WED, DURATION);
    let source = section(&[first, second]);
    let parsed = tasks(&source, &mut |_| true).unwrap();
    for (at, value) in [
        (parsed[0].offset + 12, vec![0; 16]),
        (parsed[1].name_end + 98, vec![0x91; 16]),
        (parsed[1].name_end + 98, vec![0x11; 16]),
        (parsed[1].name_end + 98, vec![0; 16]),
    ] {
        let mut changed = source.clone();
        set(&mut changed, at, &value);
        let raw = project_parts(&changed, &calendars(), &states());
        let r = request(0x11, None, 0);
        assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &r, |_| true).is_err());
    }
}

#[test]
fn accepts_511_source_tasks_and_rebuilds_all_512_nonidentity_lookup_entries() {
    let mut rows = Vec::new();
    for n in 1u16..=511 {
        let mut row = record(0x11, MON, DURATION);
        let mut guid = [0; 16];
        guid[..2].copy_from_slice(&n.to_le_bytes());
        set(&mut row, 12, &guid);
        let data = section(std::slice::from_ref(&row));
        let parsed = tasks(&data, &mut |_| true).unwrap();
        let mut supply = [0; 16];
        supply[..4].copy_from_slice(&(0xa000_0000u32 + u32::from(n)).to_le_bytes());
        set(&mut row, parsed[0].name_end + 98 - 32, &supply);
        rows.push(row);
    }
    let raw = project(&rows);
    let first = tasks(&payload(&raw), &mut |_| true).unwrap().remove(0);
    let mut r = request(0x11, None, 0);
    r.anchor_task_guid = first.guid.clone();
    r.template_task_guid = first.guid;
    let result = apply(&raw, &r);
    let data = payload(&result.bytes);
    let parsed = tasks(&data, &mut |_| true).unwrap();
    assert_eq!(parsed.len(), 512);
    assert_eq!(parsed[0].guid, id(0x77));
    assert_eq!(task_lookup::audit(&data, &parsed, &mut |_| true).unwrap().status, task_lookup::Status::Valid);
}
