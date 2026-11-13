//! Bounded source-bound field/order draft history. Native acceptance remains separate.
use super::{
    FormatError, hierarchy, mixed, ordered, parse_metadata, relocation, structured,
    task_strings::TextEdit,
    writer::{self, ScalarEdit},
};
use std::sync::Arc;

const HISTORY_PAYLOAD_LIMIT: usize = 16 * 1024 * 1024;
pub mod replay;
#[derive(Clone, Default)]
struct Version {
    scalars: Vec<ScalarEdit>,
    texts: Vec<TextEdit>,
    orders: Vec<relocation::Move>,
    relocations: Vec<hierarchy::Reparent>,
    replay: Vec<replay::Operation>,
    payload_bytes: usize,
}
#[derive(Clone)]
pub struct Draft {
    original: Arc<[u8]>,
    pub source_sha256: String,
    versions: Vec<Arc<Version>>,
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
        Ok(Self { original: raw, source_sha256: parsed.source_sha256, versions: vec![Arc::new(Version::default())], cursor: 0, revision: 0 })
    }
    pub fn edits(&self) -> &[ScalarEdit] {
        self.versions.get(self.cursor).map(|v| v.scalars.as_slice()).unwrap_or_default()
    }
    pub fn text_edits(&self) -> &[TextEdit] {
        self.versions.get(self.cursor).map(|v| v.texts.as_slice()).unwrap_or_default()
    }
    pub fn edit_count(&self) -> usize {
        let (fields, structural) = self.replay_counts();
        self.edits()
            .len()
            .saturating_add(self.text_edits().len())
            .saturating_add(self.structural_edits().len())
            .saturating_add(fields)
            .saturating_add(structural)
    }
    fn replay_counts(&self) -> (usize, usize) {
        replay::counts(self.operations())
    }
    fn operations(&self) -> &[replay::Operation] {
        self.versions.get(self.cursor).map(|v| v.replay.as_slice()).unwrap_or_default()
    }
    pub fn has_creation_edits(&self) -> bool {
        !self.operations().is_empty()
    }
    pub fn structural_edit_count(&self) -> usize {
        self.structural_edits().len().saturating_add(self.replay_counts().1)
    }
    pub fn creation_count(&self) -> usize {
        self.operations().iter().filter(|o| matches!(o, replay::Operation::Create(_))).count()
    }
    pub fn creates_task(&self, guid: &str) -> bool {
        self.operations().iter().any(|o| matches!(o, replay::Operation::Create(r) if r.task_guid.eq_ignore_ascii_case(guid)))
    }
    pub fn redo_creation_task_guid(&self) -> Option<&str> {
        let next = self.versions.get(self.cursor.checked_add(1)?)?;
        next.replay.iter().find_map(|op| match op {
            replay::Operation::Create(r) if !self.creates_task(&r.task_guid) => Some(r.task_guid.as_str()),
            _ => None,
        })
    }
    pub fn order_edits(&self) -> &[relocation::Move] {
        self.versions.get(self.cursor).map(|v| v.orders.as_slice()).unwrap_or_default()
    }
    pub fn structural_edits(&self) -> &[hierarchy::Reparent] {
        self.versions.get(self.cursor).map(|v| v.relocations.as_slice()).unwrap_or_default()
    }
    pub fn has_hierarchy_edits(&self) -> bool {
        self.structural_edits().iter().any(|r| !structured::same_parent(r.expected_parent_guid.as_deref(), r.parent_guid.as_deref()))
    }
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }
    pub fn is_dirty(&self) -> bool {
        self.has_creation_edits() || !self.edits().is_empty() || !self.text_edits().is_empty() || !self.structural_edits().is_empty()
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
    pub fn candidate(&self, keep: impl FnMut(f32) -> bool) -> Result<mixed::Candidate, String> {
        self.require_legacy_export()?;
        if !self.structural_edits().is_empty() {
            return Err("This draft has task relocations; use its ordered or structured candidate export".into());
        }
        mixed::candidate(&self.original, &self.source_sha256, self.edits(), self.text_edits(), keep).map_err(error)
    }
    pub fn ordered_candidate(&self, keep: impl FnMut(f32) -> bool) -> Result<ordered::Candidate, String> {
        self.require_legacy_export()?;
        if self.has_hierarchy_edits() {
            return Err("This draft has parent changes; export its version-4 structured candidate".into());
        }
        ordered::candidate(&self.original, &self.source_sha256, self.edits(), self.text_edits(), self.order_edits(), keep).map_err(error)
    }
    pub fn structural_candidate(&self, keep: impl FnMut(f32) -> bool) -> Result<structured::Candidate, String> {
        self.require_legacy_export()?;
        structured::candidate(&self.original, &self.source_sha256, self.edits(), self.text_edits(), self.structural_edits(), keep).map_err(error)
    }
    fn require_legacy_export(&self) -> Result<(), String> {
        if self.has_creation_edits() {
            return Err("This draft creates tasks; export its version-5 chronological candidate".into());
        }
        Ok(())
    }
    pub fn authored_candidate(&self, keep: impl FnMut(f32) -> bool) -> Result<replay::Candidate, String> {
        replay::candidate(&self.original, &self.source_sha256, self.edits(), self.text_edits(), self.structural_edits(), self.operations(), keep).map_err(error)
    }
    pub fn bytes(&self, keep: impl FnMut(f32) -> bool) -> Result<Vec<u8>, String> {
        if self.has_creation_edits() {
            return self.authored_candidate(keep).map(|c| c.bytes);
        }
        if !self.is_dirty() {
            return Ok(self.original.to_vec());
        }
        if self.has_hierarchy_edits() {
            return self.structural_candidate(keep).map(|v| v.bytes);
        }
        if !self.order_edits().is_empty() {
            return self.ordered_candidate(keep).map(|v| v.bytes);
        }
        if self.text_edits().is_empty() {
            return writer::candidate(&self.original, &self.source_sha256, self.edits(), keep).map(|v| v.bytes).map_err(error);
        }
        self.candidate(keep).map(|v| v.bytes)
    }
    pub fn apply(&mut self, revision: u64, edits: &[ScalarEdit], keep: impl FnMut(f32) -> bool) -> Result<(), String> {
        self.apply_mixed(revision, edits, &[], keep)
    }
    pub fn apply_mixed(&mut self, revision: u64, edits: &[ScalarEdit], texts: &[TextEdit], keep: impl FnMut(f32) -> bool) -> Result<(), String> {
        self.apply_ordered(revision, edits, texts, &[], keep)
    }
    pub fn apply_ordered(
        &mut self,
        revision: u64,
        edits: &[ScalarEdit],
        texts: &[TextEdit],
        orders: &[relocation::Move],
        keep: impl FnMut(f32) -> bool,
    ) -> Result<(), String> {
        if orders.len() > ordered::ORDER_LIMIT {
            return Err("Choose at most eight task relocations".into());
        }
        let relocations: Vec<_> = orders
            .iter()
            .map(|r| hierarchy::Reparent {
                task_guid: r.task_guid.clone(),
                expected_parent_guid: r.expected_parent_guid.clone(),
                expected_sibling_index: r.expected_sibling_index,
                parent_guid: r.expected_parent_guid.clone(),
                sibling_index: r.sibling_index,
            })
            .collect();
        self.apply_structural(revision, edits, texts, &relocations, keep)
    }
    pub fn apply_structural(
        &mut self,
        revision: u64,
        edits: &[ScalarEdit],
        texts: &[TextEdit],
        relocations: &[hierarchy::Reparent],
        mut keep: impl FnMut(f32) -> bool,
    ) -> Result<(), String> {
        if self.has_creation_edits() {
            return self.apply_replay(revision, edits, texts, relocations, None, &mut keep);
        }
        let next = self.check_revision(revision)?;
        if edits.is_empty() && texts.is_empty() && relocations.is_empty() {
            return Err("Choose at least one draft edit".into());
        }
        if self.cursor >= 64 {
            return Err("Draft history is limited to 64 transactions; export a candidate before starting another draft".into());
        }
        let current = self.bytes(&mut keep)?;
        let parsed = parse_metadata(&current, &mut keep).map_err(error)?;
        // Validate current expectations atomically, including all cross-kind field limits.
        validate(&current, &parsed.source_sha256, edits, texts, relocations, &mut keep)?;
        let mut scalars = self.edits().to_vec();
        for edit in edits {
            let key = identity(edit);
            if let Some(old) = scalars.iter_mut().find(|old| identity(old) == key) {
                *old = baseline_expected(edit.clone(), old);
            } else {
                scalars.push(edit.clone());
            }
        }
        scalars.retain(|edit| !unchanged(edit));
        let mut merged_texts = self.text_edits().to_vec();
        for edit in texts {
            let key = text_identity(edit);
            if let Some(old) = merged_texts.iter_mut().find(|old| text_identity(old) == key) {
                *old = baseline_text_expected(edit.clone(), old)?;
            } else {
                merged_texts.push(edit.clone());
            }
        }
        merged_texts.retain(|edit| !unchanged_text(edit));
        let mut merged_relocations = self.structural_edits().to_vec();
        merged_relocations.extend(
            relocations
                .iter()
                .filter(|r| {
                    !structured::same_parent(r.expected_parent_guid.as_deref(), r.parent_guid.as_deref()) || r.expected_sibling_index != r.sibling_index
                })
                .cloned(),
        );
        // Validate the complete baseline-relative sequence before publishing history.
        if !merged_relocations.is_empty() {
            let candidate =
                structured::candidate(&self.original, &self.source_sha256, &scalars, &merged_texts, &merged_relocations, &mut keep).map_err(error)?;
            if graph_ids(&candidate.bytes, &mut keep)? == graph_ids(&self.original, &mut keep)? {
                merged_relocations.clear();
            }
        } else {
            validate(&self.original, &self.source_sha256, &scalars, &merged_texts, &[], &mut keep)?;
        }
        let payload_bytes = payload_cost(&scalars, &merged_texts)?
            .checked_add(merged_relocations.len().checked_mul(1024).ok_or_else(|| "Draft relocation payload size overflow".to_owned())?)
            .ok_or_else(|| "Draft payload size overflow".to_owned())?;
        let retained = self
            .versions
            .iter()
            .take(self.cursor + 1)
            .try_fold(0usize, |n, v| n.checked_add(v.payload_bytes))
            .ok_or_else(|| "Draft history payload size overflow".to_owned())?;
        if retained.checked_add(payload_bytes).is_none_or(|n| n > HISTORY_PAYLOAD_LIMIT) {
            return Err("Draft history exceeds the 16 MiB edit-payload budget; export before starting another draft".into());
        }
        self.versions.truncate(self.cursor + 1);
        let orders = if merged_relocations.iter().all(|r| structured::same_parent(r.expected_parent_guid.as_deref(), r.parent_guid.as_deref())) {
            merged_relocations
                .iter()
                .map(|r| relocation::Move {
                    task_guid: r.task_guid.clone(),
                    expected_parent_guid: r.expected_parent_guid.clone(),
                    expected_sibling_index: r.expected_sibling_index,
                    sibling_index: r.sibling_index,
                })
                .collect()
        } else {
            Vec::new()
        };
        self.versions.push(Arc::new(Version { scalars, texts: merged_texts, orders, relocations: merged_relocations, replay: Vec::new(), payload_bytes }));
        self.cursor += 1;
        self.revision = next;
        Ok(())
    }
    pub fn insert_above(&mut self, revision: u64, request: &super::creation::InsertAbove, mut keep: impl FnMut(f32) -> bool) -> Result<(), String> {
        self.apply_replay(revision, &[], &[], &[], Some(request), &mut keep)
    }
    fn apply_replay(
        &mut self,
        revision: u64,
        scalars: &[ScalarEdit],
        texts: &[TextEdit],
        relocations: &[hierarchy::Reparent],
        insertion: Option<&super::creation::InsertAbove>,
        keep: &mut dyn FnMut(f32) -> bool,
    ) -> Result<(), String> {
        let next_revision = self.check_revision(revision)?;
        if self.cursor >= 64 {
            return Err("Draft history is limited to 64 transactions; export before starting another draft".into());
        }
        if scalars.is_empty() && texts.is_empty() && relocations.is_empty() && insertion.is_none() {
            return Err("Choose at least one draft edit".into());
        }
        if scalars.len().saturating_add(texts.len()).saturating_add(relocations.len()) > 256 || relocations.len() > ordered::ORDER_LIMIT {
            return Err("Choose at most 256 edits including eight structural operations".into());
        }
        let current = self.bytes(&mut *keep)?;
        let parsed = parse_metadata(&current, &mut *keep).map_err(error)?;
        // Validate each new transaction against the current revision before merging recipes.
        if let Some(request) = insertion {
            super::creation::candidate(&current, &parsed.source_sha256, request, &mut *keep).map_err(error)?;
        } else {
            validate(&current, &parsed.source_sha256, scalars, texts, relocations, keep)?;
        }
        let mut version = self.versions.get(self.cursor).ok_or_else(|| "Draft history missing".to_owned())?.as_ref().clone();
        if !scalars.is_empty() || !texts.is_empty() {
            if let Some(replay::Operation::Fields { scalars: merged, texts: merged_texts }) = version.replay.last_mut() {
                merge_fields(merged, merged_texts, scalars, texts)?;
            } else {
                let mut merged = Vec::new();
                let mut merged_texts = Vec::new();
                merge_fields(&mut merged, &mut merged_texts, scalars, texts)?;
                version.replay.push(replay::Operation::Fields { scalars: merged, texts: merged_texts });
            }
        }
        version.replay.extend(
            relocations
                .iter()
                .filter(|r| {
                    !structured::same_parent(r.expected_parent_guid.as_deref(), r.parent_guid.as_deref()) || r.expected_sibling_index != r.sibling_index
                })
                .cloned()
                .map(replay::Operation::Relocate),
        );
        if let Some(request) = insertion {
            version.replay.push(replay::Operation::Create(request.clone()));
        }
        replay::candidate(&self.original, &self.source_sha256, &version.scalars, &version.texts, &version.relocations, &version.replay, &mut *keep)
            .map_err(error)?;
        let base = payload_cost(&version.scalars, &version.texts)?
            .checked_add(version.relocations.len().checked_mul(1024).ok_or_else(|| "Draft relocation payload overflow".to_owned())?)
            .ok_or_else(|| "Draft payload overflow".to_owned())?;
        version.payload_bytes = base.checked_add(replay::payload(&version.replay)?).ok_or_else(|| "Draft payload overflow".to_owned())?;
        let retained = self
            .versions
            .iter()
            .take(self.cursor + 1)
            .try_fold(0usize, |n, v| n.checked_add(v.payload_bytes))
            .ok_or_else(|| "Draft history payload overflow".to_owned())?;
        if retained.checked_add(version.payload_bytes).is_none_or(|n| n > HISTORY_PAYLOAD_LIMIT) {
            return Err("Draft history exceeds the 16 MiB edit-payload budget; export before starting another draft".into());
        }
        self.versions.truncate(self.cursor + 1);
        self.versions.push(Arc::new(version));
        self.cursor += 1;
        self.revision = next_revision;
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
fn merge_fields(scalars: &mut Vec<ScalarEdit>, texts: &mut Vec<TextEdit>, edits: &[ScalarEdit], new_texts: &[TextEdit]) -> Result<(), String> {
    for edit in edits {
        let key = identity(edit);
        if let Some(old) = scalars.iter_mut().find(|old| identity(old) == key) {
            *old = baseline_expected(edit.clone(), old);
        } else {
            scalars.push(edit.clone());
        }
    }
    scalars.retain(|e| !unchanged(e));
    for edit in new_texts {
        let key = text_identity(edit);
        if let Some(old) = texts.iter_mut().find(|old| text_identity(old) == key) {
            *old = baseline_text_expected(edit.clone(), old)?;
        } else {
            texts.push(edit.clone());
        }
    }
    texts.retain(|e| !unchanged_text(e));
    Ok(())
}
fn validate(
    raw: &[u8],
    hash: &str,
    scalars: &[ScalarEdit],
    texts: &[TextEdit],
    relocations: &[hierarchy::Reparent],
    keep: &mut dyn FnMut(f32) -> bool,
) -> Result<(), String> {
    if !relocations.is_empty() {
        structured::candidate(raw, hash, scalars, texts, relocations, keep).map(|_| ()).map_err(error)
    } else if texts.is_empty() {
        writer::candidate(raw, hash, scalars, keep).map(|_| ()).map_err(error)
    } else {
        mixed::candidate(raw, hash, scalars, texts, keep).map(|_| ()).map_err(error)
    }
}
fn graph_ids(raw: &[u8], keep: &mut dyn FnMut(f32) -> bool) -> Result<Vec<(String, Option<String>)>, String> {
    let parsed = parse_metadata(raw, keep).map_err(error)?;
    parsed
        .metadata
        .get("tasks")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "Draft task inventory missing".to_owned())?
        .iter()
        .map(|t| {
            let id = t.get("guid").and_then(serde_json::Value::as_str).ok_or_else(|| "Draft task identity missing".to_owned())?;
            let parent = match t.get("parent_guid") {
                None | Some(serde_json::Value::Null) => None,
                Some(serde_json::Value::String(value)) => Some(value.clone()),
                _ => return Err("Draft task parent is invalid".into()),
            };
            Ok((id.to_owned(), parent))
        })
        .collect()
}
fn text_identity(edit: &TextEdit) -> String {
    match edit {
        TextEdit::TaskName { task_guid, .. } => format!("name/{task_guid}"),
        TextEdit::TaskDisplayId { task_guid, .. } => format!("id/{task_guid}"),
    }
    .to_ascii_lowercase()
}
fn unchanged_text(edit: &TextEdit) -> bool {
    match edit {
        TextEdit::TaskName { expected, value, .. } | TextEdit::TaskDisplayId { expected, value, .. } => expected == value,
    }
}
fn baseline_text_expected(mut edit: TextEdit, old: &TextEdit) -> Result<TextEdit, String> {
    match (&mut edit, old) {
        (TextEdit::TaskName { expected, .. }, TextEdit::TaskName { expected: original, .. })
        | (TextEdit::TaskDisplayId { expected, .. }, TextEdit::TaskDisplayId { expected: original, .. }) => *expected = original.clone(),
        _ => return Err("Mismatched task text identity".into()),
    }
    Ok(edit)
}
fn payload_cost(scalars: &[ScalarEdit], texts: &[TextEdit]) -> Result<usize, String> {
    // Account for strings plus conservative per-field storage; shared Version snapshots
    // make cloning a session independent of the retained text payload size.
    let initial = scalars.len().checked_mul(512).ok_or_else(|| "Draft payload size overflow".to_owned())?;
    texts.iter().try_fold(initial, |n, edit| {
        let (id, expected, value) = match edit {
            TextEdit::TaskName { task_guid, expected, value } | TextEdit::TaskDisplayId { task_guid, expected, value } => (task_guid, expected, value),
        };
        n.checked_add(512)
            .and_then(|n| n.checked_add(id.len()))
            .and_then(|n| n.checked_add(expected.len()))
            .and_then(|n| n.checked_add(value.len()))
            .ok_or_else(|| "Draft payload size overflow".to_owned())
    })
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
        let mut draft = Draft {
            original: Arc::from([]),
            source_sha256: String::new(),
            versions: vec![Arc::new(Version::default()), Arc::new(Version::default())],
            cursor: 1,
            revision: 5,
        };
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
