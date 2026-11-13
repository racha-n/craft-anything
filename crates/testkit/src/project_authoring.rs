//! Synthetic default-Work metadata projects for engine and widget acceptance.
//! Contains no geometry, client content or native-project bytes.
const OPEN: &[u8] = &[0x7a, 0x54, 0xaf, 0x39];
const CLOSE: &[u8] = &[0x16, 0xe4, 0x99, 0x6f];
fn empty_section() -> Vec<u8> {
    let mut b = vec![0; 24];
    b.extend(OPEN);
    b.extend(0u32.to_le_bytes());
    b.extend(CLOSE);
    b
}
fn text(s: &str) -> Vec<u8> {
    let mut b = vec![255, 254, 255, s.encode_utf16().count() as u8];
    b.extend(s.encode_utf16().flat_map(u16::to_le_bytes));
    b
}

fn set(data: &mut [u8], at: usize, value: &[u8]) {
    data[at..at + value.len()].copy_from_slice(value);
}

fn record(byte: u8, start: u64, duration: u64) -> Vec<u8> {
    let mut row = vec![0; 29];
    set(&mut row, 0, &[0x76, 0x25, 0xec, 0x04]);
    row.extend(OPEN);
    row.extend(0u32.to_le_bytes());
    row.extend(CLOSE);
    row.extend(1u32.to_le_bytes());
    row.extend(OPEN);
    row.extend(0u32.to_le_bytes());
    row.extend(CLOSE);
    row.extend([0x51; 16]);
    set(&mut row, 12, &[byte; 16]);
    row[12] = 0x40;
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

/// Build 1..=110 independent Planned leaves on the admitted synthetic calendar.
pub fn default_work_project(task_count: usize) -> Vec<u8> {
    assert!((1..=110).contains(&task_count));
    let rows: Vec<_> = (1..=task_count).map(|i| record(i as u8, 1_794_214_800, 28_800)).collect();
    let tasks = section(&rows);
    let calendars = calendars();
    let states = states();
    let names = ["Tasks", "Resources", "Use Profiles", "Activity Codes", "User Fields", "Entities", "IncludeLibrary", "Calendars", "Task States"];
    let mut raw = vec![0u8; 2344];
    raw[..16].copy_from_slice(&[0xb6, 0x17, 0x67, 0x1e, 0x7a, 0xf3, 0x46, 0x40, 0xa8, 0x6e, 0xd5, 0xdc, 0x41, 0x7a, 0x2d, 0xe2]);
    raw[20..40].copy_from_slice(&text("6.5.5.28"));
    for i in 0..48 {
        let name = names.get(i).map(|s| s.to_string()).unwrap_or_else(|| format!("Opaque{i}"));
        let data = match name.as_str() {
            "Tasks" => tasks.clone(),
            "Calendars" => calendars.clone(),
            "Task States" => states.clone(),
            _ => empty_section(),
        };
        let offset = raw.len() as u64;
        let at = 40 + i * 48;
        raw[at..at + name.len()].copy_from_slice(name.as_bytes());
        raw[at + 40..at + 48].copy_from_slice(&offset.to_le_bytes());
        raw.extend([1, name.len() as u8]);
        raw.extend(name.as_bytes());
        raw.extend(miniz_oxide::deflate::compress_to_vec_zlib(&data, 6));
    }
    raw
}
