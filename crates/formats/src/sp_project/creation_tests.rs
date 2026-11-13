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
fn assigned_record(byte: u8, start: u64, duration: u64, count: usize) -> Vec<u8> {
    let mut row = record(byte, start, duration);
    set(&mut row, 33, &(count as u32).to_le_bytes());
    let mut assignment = vec![0; 235];
    set(&mut assignment, 0, &[0x99, 0xd8, 0x23, 0x46]);
    set(&mut assignment, 12, &[0x22; 16]);
    set(&mut assignment, 131, &[0x33; 16]);
    set(&mut assignment, 163, &5f64.to_le_bytes());
    set(&mut assignment, 171, &(500f64 / 3.).to_le_bytes());
    for at in [179, 187, 195] {
        set(&mut assignment, at, &f64::NAN.to_le_bytes());
    }
    for _ in 0..count {
        row.splice(37..37, assignment.iter().copied());
    }
    row
}
fn assigned_project(rows: &[Vec<u8>]) -> Vec<u8> {
    fn list(b: &mut Vec<u8>) {
        b.extend(OPEN);
        b.extend([0; 4]);
        b.extend(CLOSE);
    }
    fn refs(b: &mut Vec<u8>) {
        b.extend([0xd0, 0xbc, 0xa1, 0x06, 0, 0, 0, 0, 0x2c, 0x57, 0xd9, 0xda]);
    }
    let mut resources = empty_section();
    resources.truncate(28);
    resources.extend(1u32.to_le_bytes());
    resources.extend([0x31, 0x5c, 0xa8, 0x57]);
    resources.extend([0; 8]);
    resources.extend([0x33; 16]);
    resources.extend(text("Material"));
    resources.extend(text(""));
    resources.extend([0; 32]);
    refs(&mut resources);
    resources.extend([0, 0, 0, 0, 4, 0, 0, 0]);
    list(&mut resources);
    resources.extend([0; 20]);
    resources.extend(text(""));
    resources.push(0);
    resources.extend(text(""));
    resources.extend(text(""));
    list(&mut resources);
    resources.extend([0; 31]);
    refs(&mut resources);
    resources.extend([0; 5]);
    refs(&mut resources);
    resources.extend([0; 16]);
    resources.extend([0xad, 0xe2, 0xb3, 0x4c]);
    resources.extend([0; 4]);
    resources.extend(CLOSE);
    let mut profiles = empty_section();
    profiles.truncate(28);
    profiles.extend(1u32.to_le_bytes());
    profiles.extend([0xc1, 0xcf, 0xd6, 0xe8]);
    profiles.extend([0; 8]);
    profiles.extend([0x22; 16]);
    profiles.extend(text(""));
    let mut tail = vec![0; 247];
    set(&mut tail, 2, &1u32.to_le_bytes());
    for at in [6, 69, 176] {
        set(&mut tail, at, &[0x10, 0x48, 0x69, 0x23]);
        set(&mut tail, at + 59, &[0x6c, 0x4a, 0x92, 0x90]);
    }
    set(&mut tail, 132, OPEN);
    set(&mut tail, 140, CLOSE);
    set(&mut tail, 239, &[0x3d, 0xc8, 0xb0, 0x03]);
    profiles.extend(tail);
    profiles.extend(CLOSE);
    let old = project(rows);
    let c = container(&old, &mut |_| true).unwrap();
    let mut slots: Vec<_> = c.sections.iter().collect();
    slots.sort_by_key(|(_, s)| s.file_offset);
    let mut result = old[..2344].to_vec();
    for (i, (name, s)) in slots.iter().enumerate() {
        let offset = result.len() as u64;
        set(&mut result, 80 + i * 48, &offset.to_le_bytes());
        if name.as_str() == "Resources" || name.as_str() == "Use Profiles" {
            let payload = if name.as_str() == "Resources" { &resources } else { &profiles };
            result.extend([1, name.len() as u8]);
            result.extend(name.as_bytes());
            result.extend(miniz_oxide::deflate::compress_to_vec_zlib(payload, 6));
        } else {
            result.extend(&old[s.file_offset..s.file_offset + s.stored_bytes]);
        }
    }
    result
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

#[cfg(test)]
mod draft_replay {
    use super::*;
    use crate::sp_project::{draft::Draft, hierarchy, task_strings::TextEdit, writer::ScalarEdit};
    fn open(raw: &[u8]) -> Draft {
        Draft::open(raw.to_vec(), &writer::hash(raw, &mut |_| true).unwrap(), |_| true).unwrap()
    }
    fn rename(id: u8, expected: &str, value: &str) -> TextEdit {
        TextEdit::TaskName { task_guid: super::id(id), expected: expected.into(), value: value.into() }
    }
    fn movement(from: usize, to: usize) -> hierarchy::Reparent {
        hierarchy::Reparent { task_guid: id(0x77), expected_parent_guid: None, expected_sibling_index: from, parent_guid: None, sibling_index: to }
    }
    fn stage_hash(stage: &serde_json::Value, key: &str) -> String {
        stage["receipt"][key].as_str().unwrap().to_owned()
    }
    #[test]
    fn chronological_creation_edits_order_and_parent_keep_complete_receipt_chain() {
        let raw = project(&[record(0x11, MON, DURATION), record(0x22, MON, DURATION)]);
        let mut d = open(&raw);
        d.apply_mixed(0, &[], &[rename(0x11, "Task17", "Earlier")], |_| true).unwrap();
        let r = request(0x22, None, 1);
        d.insert_above(1, &r, |_| true).unwrap();
        d.apply_mixed(
            2,
            &[ScalarEdit::TaskDuration { task_guid: r.task_guid.clone(), expected_storage: DURATION, value_storage: 57_600 }],
            &[rename(0x77, &r.name, "First")],
            |_| true,
        )
        .unwrap();
        d.apply_structural(3, &[], &[], &[movement(1, 0)], |_| true).unwrap();
        d.apply_mixed(4, &[], &[rename(0x77, "First", "Final")], |_| true).unwrap();
        let reparent = hierarchy::Reparent { parent_guid: Some(id(0x11)), ..movement(0, 0) };
        d.apply_structural(5, &[], &[], &[reparent], |_| true).unwrap();
        let c = d.authored_candidate(|_| true).unwrap();
        let parsed = tasks(&payload(&c.bytes), &mut |_| true).unwrap();
        assert_eq!(parsed.iter().map(|t| t.guid.clone()).collect::<Vec<_>>(), [id(0x11), id(0x77), id(0x22)]);
        assert_eq!(parsed[0].name, "Earlier");
        assert_eq!(parsed[1].name, "Final");
        assert_eq!(parsed[1].parent_guid, Some(id(0x11)));
        assert_eq!(parsed[1].duration_storage_scalar, 57_600);
        assert_eq!(d.structural_edit_count(), 3);
        assert_eq!(d.creation_count(), 1);
        let receipt = serde_json::to_value(c.receipt).unwrap();
        assert_eq!(receipt["version"], 5);
        let mut hash = receipt["prefix"]["output_sha256"].as_str().unwrap().to_owned();
        let stages = receipt["stages"].as_array().unwrap();
        assert_eq!(stages.iter().map(|s| s["kind"].as_str().unwrap()).collect::<Vec<_>>(), ["create", "fields", "order", "fields", "hierarchy"]);
        for stage in stages {
            assert_eq!(stage_hash(stage, "source_sha256"), hash);
            hash = stage_hash(stage, "output_sha256");
            assert_eq!(stage["receipt"]["native_reopen_verified"], false);
        }
        assert_eq!(hash, receipt["output_sha256"]);
        assert_eq!(hash, writer::hash(&c.bytes, &mut |_| true).unwrap());
        assert!(d.candidate(|_| true).is_err() && d.ordered_candidate(|_| true).is_err() && d.structural_candidate(|_| true).is_err());
    }
    #[test]
    fn stable_creation_identity_undo_redo_branching_and_failed_edits_are_atomic() {
        let raw = project(&[record(0x11, MON, DURATION)]);
        let mut d = open(&raw);
        let r = request(0x11, None, 0);
        d.insert_above(0, &r, |_| true).unwrap();
        let created = d.bytes(|_| true).unwrap();
        assert!(d.insert_above(1, &r, |_| true).is_err());
        assert!(d.apply_mixed(0, &[], &[rename(0x77, &r.name, "Stale")], |_| true).is_err());
        assert!(d.apply_mixed(1, &[], &[rename(0x77, "Wrong", "Rejected")], |_| true).is_err());
        assert!(d.apply_structural(1, &[], &[], &[movement(1, 0)], |_| true).is_err());
        assert!(d.apply_mixed(1, &[], &[rename(0x77, &r.name, "Cancelled")], |_| false).is_err());
        assert_eq!(d.revision, 1);
        assert_eq!(d.bytes(|_| true).unwrap(), created);
        d.undo(1).unwrap();
        assert_eq!(d.bytes(|_| true).unwrap(), raw);
        assert!(!d.has_creation_edits() && !d.is_dirty());
        d.redo(2).unwrap();
        assert_eq!(d.bytes(|_| true).unwrap(), created);
        d.apply_mixed(3, &[], &[rename(0x77, &r.name, "Branch A")], |_| true).unwrap();
        d.undo(4).unwrap();
        d.apply_mixed(5, &[], &[rename(0x77, &r.name, "Branch B")], |_| true).unwrap();
        assert!(!d.can_redo());
        assert!(d.creates_task(&r.task_guid.to_uppercase()));
        let parsed = tasks(&payload(&d.bytes(|_| true).unwrap()), &mut |_| true).unwrap();
        assert_eq!(parsed[0].name, "Branch B");
        assert_eq!(parsed[0].guid, r.task_guid);
    }
    #[test]
    fn consecutive_field_groups_merge_against_segment_input_and_remove_reverted_fields() {
        let raw = project(&[record(0x11, MON, DURATION)]);
        let mut d = open(&raw);
        let r = request(0x11, None, 0);
        d.insert_above(0, &r, |_| true).unwrap();
        d.apply_mixed(1, &[], &[rename(0x77, &r.name, "One")], |_| true).unwrap();
        d.apply_mixed(2, &[], &[rename(0x77, "One", "Two")], |_| true).unwrap();
        assert_eq!(d.edit_count(), 2);
        let v = serde_json::to_value(d.authored_candidate(|_| true).unwrap().receipt).unwrap();
        assert_eq!(v["stages"].as_array().unwrap().len(), 2);
        d.apply_mixed(3, &[], &[rename(0x77, "Two", &r.name)], |_| true).unwrap();
        assert_eq!(d.edit_count(), 1);
        let direct = apply(&raw, &r);
        assert_eq!(d.bytes(|_| true).unwrap(), direct.bytes);
    }
    #[test]
    fn assigned_creation_retains_scope_and_reparent_refuses_without_history_change() {
        let raw = assigned_project(&[assigned_record(0x11, MON, 86_400, 1)]);
        let mut d = open(&raw);
        let r = request(0x11, None, 0);
        d.insert_above(0, &r, |_| true).unwrap();
        let c = d.authored_candidate(|_| true).unwrap();
        assert_eq!(serde_json::to_value(c.receipt).unwrap()["stages"][0]["receipt"]["source_scope"], "flat_planned_single_assignments");
        let parent = hierarchy::Reparent { parent_guid: Some(id(0x11)), ..movement(0, 0) };
        assert!(d.apply_structural(1, &[], &[], &[parent], |_| true).is_err());
        assert_eq!(d.revision, 1);
        assert_eq!(d.bytes(|_| true).unwrap(), c.bytes);
    }
    #[test]
    fn creation_replay_and_legacy_prefix_share_eight_structural_operation_limit() {
        let raw = project(&[record(0x11, MON, DURATION), record(0x22, MON, DURATION)]);
        let mut d = open(&raw);
        d.insert_above(0, &request(0x11, None, 0), |_| true).unwrap();
        for revision in 1..8 {
            let (from, to) = if revision % 2 == 1 { (0, 1) } else { (1, 0) };
            d.apply_structural(revision, &[], &[], &[movement(from, to)], |_| true).unwrap();
        }
        assert_eq!(d.structural_edit_count(), 8);
        let before = d.bytes(|_| true).unwrap();
        assert!(d.apply_structural(8, &[], &[], &[movement(1, 0)], |_| true).is_err());
        assert_eq!(d.revision, 8);
        assert_eq!(d.bytes(|_| true).unwrap(), before);
    }
    #[test]
    fn legacy_prefix_operations_and_fields_count_toward_replay_limits() {
        let raw = project(&[record(0x11, MON, DURATION), record(0x22, MON, DURATION)]);
        let mut d = open(&raw);
        let prefix: Vec<_> = (0..7)
            .map(|i| hierarchy::Reparent {
                task_guid: id(0x11),
                expected_parent_guid: None,
                expected_sibling_index: i % 2,
                parent_guid: None,
                sibling_index: 1 - i % 2,
            })
            .collect();
        d.apply_structural(0, &[], &[], &prefix, |_| true).unwrap();
        d.insert_above(1, &request(0x11, None, 1), |_| true).unwrap();
        assert_eq!(d.structural_edit_count(), 8);
        assert!(d.apply_structural(2, &[], &[], &[movement(1, 0)], |_| true).is_err());
        assert_eq!(d.revision, 2);

        let rows: Vec<_> = (1..=70).map(|i| record(i, MON, DURATION)).collect();
        let raw = project(&rows);
        let mut d = open(&raw);
        let fields: Vec<_> = (1..=70)
            .flat_map(|i| {
                [
                    rename(i, &format!("Task{i}"), &format!("Prefix{i}")),
                    TextEdit::TaskDisplayId { task_guid: id(i), expected: format!("ID{i}"), value: format!("PREFIX-{i}") },
                ]
            })
            .collect();
        d.apply_mixed(0, &[], &fields, |_| true).unwrap();
        d.insert_above(1, &request(0x11, None, 16), |_| true).unwrap();
        let before = d.bytes(|_| true).unwrap();
        let fields: Vec<_> = (1..=70)
            .flat_map(|i| {
                [
                    rename(i, &format!("Prefix{i}"), &format!("After{i}")),
                    TextEdit::TaskDisplayId { task_guid: id(i), expected: format!("PREFIX-{i}"), value: format!("AFTER-{i}") },
                ]
            })
            .collect();
        assert!(d.apply_mixed(2, &[], &fields, |_| true).is_err());
        assert_eq!(d.revision, 2);
        assert_eq!(d.bytes(|_| true).unwrap(), before);
    }
    #[test]
    fn creation_history_limit_and_branch_after_undo_remain_shared() {
        let raw = project(&[record(0x11, MON, DURATION)]);
        let mut d = open(&raw);
        let r = request(0x11, None, 0);
        d.insert_above(0, &r, |_| true).unwrap();
        let mut name = r.name.clone();
        for revision in 1..64 {
            let next = format!("Revision {revision}");
            d.apply_mixed(revision, &[], &[rename(0x77, &name, &next)], |_| true).unwrap();
            name = next;
        }
        assert!(d.apply_mixed(64, &[], &[rename(0x77, &name, "Over budget")], |_| true).is_err());
        assert_eq!(d.revision, 64);
        d.undo(64).unwrap();
        d.apply_mixed(65, &[], &[rename(0x77, "Revision 62", "New branch")], |_| true).unwrap();
        assert!(!d.can_redo());
        assert_eq!(d.revision, 66);
    }
    #[test]
    fn real_text_payload_history_budget_rejects_atomically_before_transaction_limit() {
        let ids = [0x11, 0x22, 0x33, 0x44, 0x55];
        let raw = project(&ids.iter().map(|&i| record(i, MON, DURATION)).collect::<Vec<_>>());
        let mut d = open(&raw);
        let r = request(0x11, None, 0);
        d.insert_above(0, &r, |_| true).unwrap();
        let mut names: Vec<_> = ids.iter().map(|i| (*i, format!("Task{i}"))).chain([(0x77, r.name)]).collect();
        let long = "\u{0e07}".repeat(16_000);
        let mut rejected = false;
        for revision in 1..64 {
            let name = format!("{long}{revision}");
            let edits: Vec<_> = names.iter().map(|(i, expected)| rename(*i, expected, &name)).collect();
            let before = d.bytes(|_| true).unwrap();
            match d.apply_mixed(revision, &[], &edits, |_| true) {
                Ok(()) => names.iter_mut().for_each(|(_, old)| *old = name.clone()),
                Err(error) => {
                    assert!(error.contains("16 MiB"), "{error}");
                    assert_eq!(d.revision, revision);
                    assert_eq!(d.bytes(|_| true).unwrap(), before);
                    rejected = true;
                    break;
                }
            }
        }
        assert!(rejected);
    }
}

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
fn assigned_template_emits_unassigned_default_and_retains_original_assignment_bytes() {
    let raw = assigned_project(&[assigned_record(0x11, MON, 86_400, 1)]);
    let before = payload(&raw);
    let source = tasks(&before, &mut |_| true).unwrap();
    assert_eq!(source[0].assignments.len(), 1);
    let result = apply(&raw, &request(0x11, None, 0));
    let data = payload(&result.bytes);
    let after = tasks(&data, &mut |_| true).unwrap();
    assert!(after[0].assignments.is_empty());
    assert_eq!(after[0].duration_storage_scalar, DURATION);
    assert_eq!(after[1].assignments.len(), 1);
    assert_eq!(&before[source[0].offset..source[0].offset + source[0].bytes - 4], &data[after[1].offset..after[1].offset + after[1].bytes - 4]);
    assert_eq!(result.receipt.version, 2);
    assert_eq!(result.receipt.source_scope, SourceScope::FlatPlannedSingleAssignments);
    assert_eq!(result.receipt.template_assignments_omitted, 1);
    assert_eq!(result.receipt.new_task_assignment_count, 0);
}

#[test]
fn unassigned_root_template_can_retain_another_assigned_root() {
    let raw = assigned_project(&[record(0x11, MON, DURATION), assigned_record(0x44, WED, 57_600, 1)]);
    let result = apply(&raw, &request(0x11, None, 0));
    assert_eq!(result.receipt.source_scope, SourceScope::FlatPlannedSingleAssignments);
    assert_eq!(result.receipt.template_assignments_omitted, 0);
    let parsed = parse_metadata(&result.bytes, |_| true).unwrap();
    assert_eq!(parsed.metadata["tasks"][2]["assignments"].as_array().unwrap().len(), 1);
    assert!(result.receipt.sections.iter().filter(|s| s.name != "Tasks").all(|s| s.stored_bytes_preserved));
}

#[test]
fn assigned_source_rejects_children_multiple_assignments_and_nonplanned_tasks() {
    let cases = [
        assigned_project(&[nested(record(0x44, MON, DURATION), &[assigned_record(0x11, MON, DURATION, 1)])]),
        assigned_project(&[assigned_record(0x11, MON, DURATION, 2)]),
    ];
    for raw in cases {
        assert!(parse_metadata(&raw, |_| true).is_ok());
        assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
    }
    let row = assigned_record(0x11, MON, DURATION, 1);
    let data = section(std::slice::from_ref(&row));
    let task = tasks(&data, &mut |_| true).unwrap().remove(0);
    let mut row = row;
    set(&mut row, task.display_id_end + 25 - 32, &2u32.to_le_bytes());
    let raw = assigned_project(&[row]);
    assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
}

#[test]
fn assigned_source_requires_known_default_bodies_even_outside_the_chosen_template() {
    let mut other = assigned_record(0x44, WED, 57_600, 1);
    other[8] ^= 1;
    let raw = assigned_project(&[record(0x11, MON, DURATION), other]);
    assert!(parse_metadata(&raw, |_| true).is_ok());
    assert!(candidate(&raw, &writer::hash(&raw, &mut |_| true).unwrap(), &request(0x11, None, 0), |_| true).is_err());
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
