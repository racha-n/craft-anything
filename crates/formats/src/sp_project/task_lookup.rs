//! Read-only validation of the observed task collection GUID lookup vectors.
use super::*;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Valid,
    Invalid,
}

/// Versioned coverage of lookup entries, independent of record framing or native row identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub version: u32,
    pub status: Status,
    pub collections_checked: usize,
    pub entries_checked: usize,
    pub mismatched_collections: usize,
    pub mismatched_entries: usize,
    pub native_row_identity_verified: bool,
}
impl Report {
    /// Check a transported report's shape, not the source lookup entries themselves.
    pub fn shape_valid(&self) -> bool {
        self.version == 1
            && !self.native_row_identity_verified
            && self.entries_checked <= RECORD_LIMIT
            && self.collections_checked <= self.entries_checked
            && (self.entries_checked == 0) == (self.collections_checked == 0)
            && self.mismatched_entries <= self.entries_checked
            && self.mismatched_collections <= self.collections_checked
            && self.mismatched_collections <= self.mismatched_entries
            && match self.status {
                Status::Valid => self.mismatched_entries == 0 && self.mismatched_collections == 0,
                Status::Invalid => self.mismatched_entries > 0 && self.mismatched_collections > 0,
            }
    }
}

pub(super) fn lookup_indices(data: &[u8], siblings: &[&Task]) -> Result<Vec<u32>> {
    if siblings.len() > RECORD_LIMIT {
        return Err(fail("limit", "Task lookup entry count"));
    }
    let mut keys = Vec::new();
    keys.try_reserve_exact(siblings.len()).map_err(|_| fail("limit", "Task lookup allocation"))?;
    for (index, task) in siblings.iter().enumerate() {
        let key = bytes(data, checked_add(task.offset, 12)?, 16)?;
        keys.push((key, u32::try_from(index).map_err(|_| fail("limit", "Task lookup index"))?));
    }
    keys.sort_by(|a, b| a.0.cmp(b.0));
    Ok(keys.into_iter().map(|(_, index)| index).collect())
}

pub(super) fn audit(data: &[u8], tasks: &[Task], callback: &mut dyn FnMut(f32) -> bool) -> Result<Report> {
    if tasks.len() > RECORD_LIMIT {
        return Err(fail("limit", "Task lookup entry count"));
    }
    let mut groups: BTreeMap<Option<&str>, Vec<&Task>> = BTreeMap::new();
    for task in tasks {
        check(callback, 0.55)?;
        groups.entry(task.parent_guid.as_deref()).or_default().push(task);
    }
    let mut report = Report {
        version: 1,
        status: Status::Valid,
        collections_checked: 0,
        entries_checked: 0,
        mismatched_collections: 0,
        mismatched_entries: 0,
        native_row_identity_verified: false,
    };
    // Groups contain retained entries. Empty collections contribute no entries or checked groups.
    for siblings in groups.values() {
        check(callback, 0.55)?;
        report.collections_checked = checked_add(report.collections_checked, 1)?;
        let mut mismatch = false;
        for (task, expected) in siblings.iter().zip(lookup_indices(data, siblings)?) {
            check(callback, 0.55)?;
            let end = checked_add(task.offset, task.bytes)?;
            let at = end.checked_sub(4).ok_or_else(|| fail("truncated", "Task lookup word"))?;
            report.entries_checked = checked_add(report.entries_checked, 1)?;
            if bytes(data, at, 4)? != expected.to_le_bytes() {
                mismatch = true;
                report.mismatched_entries = checked_add(report.mismatched_entries, 1)?;
            }
        }
        if mismatch {
            report.mismatched_collections = checked_add(report.mismatched_collections, 1)?;
            report.status = Status::Invalid;
        }
    }
    check(callback, 0.55)?;
    Ok(report)
}
