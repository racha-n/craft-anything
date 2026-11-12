//! Bounded scalar draft history. Native save and scheduling acceptance remain separate.
use super::{
    FormatError, parse_metadata,
    writer::{self, Candidate, ScalarEdit},
};
use std::sync::Arc;

#[derive(Clone)]
pub struct Draft {
    original: Arc<[u8]>,
    pub source_sha256: String,
    versions: Vec<Vec<ScalarEdit>>,
    cursor: usize,
    pub revision: u64,
}
impl Draft {
    pub fn open(raw: impl Into<Arc<[u8]>>, expected: &str, mut keep: impl FnMut(f32) -> bool) -> Result<Self, String> {
        let raw = raw.into();
        let parsed = parse_metadata(&raw, &mut keep).map_err(error)?;
        if !parsed.source_sha256.eq_ignore_ascii_case(expected) {
            return Err("Source changed; reload the project before editing".into());
        }
        Ok(Self { original: raw, source_sha256: parsed.source_sha256, versions: vec![vec![]], cursor: 0, revision: 0 })
    }
    pub fn edits(&self) -> &[ScalarEdit] {
        self.versions.get(self.cursor).map(Vec::as_slice).unwrap_or_default()
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn is_dirty(&self) -> bool {
        !self.edits().is_empty()
    }
    pub fn can_redo(&self) -> bool {
        self.cursor.checked_add(1).is_some_and(|n| n < self.versions.len())
    }
    fn check_revision(&self, revision: u64) -> Result<u64, String> {
        if revision != self.revision {
            return Err("Draft revision changed; refresh before editing".into());
        }
        self.revision.checked_add(1).ok_or_else(|| "Draft revision exhausted".into())
    }
    pub fn candidate(&self, keep: impl FnMut(f32) -> bool) -> Result<Candidate, String> {
        writer::candidate(&self.original, &self.source_sha256, self.edits(), keep).map_err(error)
    }
    pub fn bytes(&self, keep: impl FnMut(f32) -> bool) -> Result<Vec<u8>, String> {
        if self.edits().is_empty() {
            return Ok(self.original.to_vec());
        }
        self.candidate(keep).map(|v| v.bytes)
    }
    pub fn apply(&mut self, revision: u64, edits: &[ScalarEdit], mut keep: impl FnMut(f32) -> bool) -> Result<(), String> {
        let next = self.check_revision(revision)?;
        if edits.is_empty() {
            return Err("Choose at least one draft edit".into());
        }
        if self.cursor >= 64 {
            return Err("Draft history is limited to 64 transactions; export a candidate before starting another draft".into());
        }
        let current = self.bytes(&mut keep)?;
        let parsed = parse_metadata(&current, &mut keep).map_err(error)?;
        // Validate every expected current value and transaction overlap before touching history.
        writer::candidate(&current, &parsed.source_sha256, edits, &mut keep).map_err(error)?;
        let mut merged = self.edits().to_vec();
        for edit in edits {
            let key = identity(edit);
            if let Some(old) = merged.iter_mut().find(|old| identity(old) == key) {
                *old = baseline_expected(edit.clone(), old);
            } else {
                merged.push(edit.clone());
            }
        }
        // Returning a scalar to its original bytes clears its dirty state. History
        // still keeps the transaction, so Undo can restore the previous draft.
        merged.retain(|edit| !unchanged(edit));
        // This also caps the total edited fields at 256, not just the transaction size.
        writer::candidate(&self.original, &self.source_sha256, &merged, &mut keep).map_err(error)?;
        self.versions.truncate(self.cursor + 1);
        self.versions.push(merged);
        self.cursor += 1;
        self.revision = next;
        Ok(())
    }
    pub fn undo(&mut self, revision: u64) -> Result<(), String> {
        let next = self.check_revision(revision)?;
        if !self.can_undo() {
            return Err("No draft change to undo".into());
        }
        self.cursor -= 1;
        self.revision = next;
        Ok(())
    }
    pub fn redo(&mut self, revision: u64) -> Result<(), String> {
        let next = self.check_revision(revision)?;
        if !self.can_redo() {
            return Err("No draft change to redo".into());
        }
        self.cursor += 1;
        self.revision = next;
        Ok(())
    }
}
fn unchanged(edit: &ScalarEdit) -> bool {
    match edit {
        ScalarEdit::TaskStart { expected_storage, value_storage, .. } | ScalarEdit::TaskDuration { expected_storage, value_storage, .. } => {
            expected_storage == value_storage
        }
        ScalarEdit::DirectFixedAmount { expected_storage, value_storage, .. }
        | ScalarEdit::AssignmentPlannedUnits { expected_storage, value_storage, .. }
        | ScalarEdit::AssignmentPlannedRate { expected_storage, value_storage, .. } => expected_storage.to_bits() == value_storage.to_bits(),
    }
}
fn error(e: FormatError) -> String {
    e.to_string()
}
fn identity(edit: &ScalarEdit) -> String {
    match edit {
        ScalarEdit::TaskStart { task_guid, .. } => format!("start/{task_guid}"),
        ScalarEdit::TaskDuration { task_guid, .. } => format!("duration/{task_guid}"),
        ScalarEdit::DirectFixedAmount { task_guid, cost_guid, .. } => format!("fixed/{task_guid}/{cost_guid}"),
        ScalarEdit::AssignmentPlannedUnits { task_guid, assignment_ordinal, .. } => format!("units/{task_guid}/{assignment_ordinal}"),
        ScalarEdit::AssignmentPlannedRate { task_guid, assignment_ordinal, .. } => format!("utilization/{task_guid}/{assignment_ordinal}"),
    }
    .to_ascii_lowercase()
}
fn baseline_expected(mut new: ScalarEdit, old: &ScalarEdit) -> ScalarEdit {
    match (&mut new, old) {
        (ScalarEdit::TaskStart { expected_storage: a, .. }, ScalarEdit::TaskStart { expected_storage: b, .. })
        | (ScalarEdit::TaskDuration { expected_storage: a, .. }, ScalarEdit::TaskDuration { expected_storage: b, .. }) => *a = *b,
        (ScalarEdit::DirectFixedAmount { expected_storage: a, .. }, ScalarEdit::DirectFixedAmount { expected_storage: b, .. })
        | (ScalarEdit::AssignmentPlannedUnits { expected_storage: a, .. }, ScalarEdit::AssignmentPlannedUnits { expected_storage: b, .. })
        | (ScalarEdit::AssignmentPlannedRate { expected_storage: a, .. }, ScalarEdit::AssignmentPlannedRate { expected_storage: b, .. }) => *a = *b,
        _ => {}
    }
    new
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_edits_preserve_original_guard_and_case_insensitive_identity() {
        let old = ScalarEdit::TaskDuration { task_guid: "AB".into(), expected_storage: 1, value_storage: 2 };
        let new = ScalarEdit::TaskDuration { task_guid: "ab".into(), expected_storage: 2, value_storage: 3 };
        assert_eq!(identity(&old), identity(&new));
        assert!(matches!(baseline_expected(new, &old), ScalarEdit::TaskDuration { expected_storage: 1, value_storage: 3, .. }));
    }
    #[test]
    fn undo_redo_revision_guards_and_branching() {
        let mut draft = Draft { original: Arc::from([]), source_sha256: String::new(), versions: vec![vec![], vec![]], cursor: 1, revision: 5 };
        assert!(draft.undo(4).is_err());
        assert_eq!(draft.revision, 5);
        draft.undo(5).unwrap();
        assert_eq!(draft.revision, 6);
        assert!(draft.can_redo());
        assert!(draft.redo(5).is_err());
        draft.redo(6).unwrap();
        assert_eq!(draft.revision, 7);
        assert!(!draft.can_redo());
        assert!(Draft::open(vec![], "not-a-hash", |_| true).is_err());
    }
}
