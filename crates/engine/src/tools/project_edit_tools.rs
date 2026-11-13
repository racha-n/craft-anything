//! Isolated draft editing; loaded source-scene caches remain unchanged.
use super::{
    native_common::spec,
    project_candidate_tools,
    static_input::{format_err, load_path},
};
use crate::{
    Ctx, Effects, Requires, ToolOutput, ToolSpec, Toolset,
    tool::{parse_params, schema_of},
};
use craft_anything_evidence::ToolError;
use craft_anything_formats::sp_project::{self, creation, deletion, draft::Draft, hierarchy, relocation, task_strings::TextEdit, writer::ScalarEdit};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

#[derive(Clone)]
struct EditSession {
    id: u64,
    path: String,
    draft: Draft,
}
#[derive(Default)]
struct DraftSequence(u64);
#[derive(Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Open {
        path: String,
        expected_source_sha256: String,
    },
    Inspect {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        #[serde(default)]
        offset: usize,
    },
    Refresh {
        draft_id: u64,
        expected_source_sha256: String,
        #[serde(default)]
        offset: usize,
    },
    Apply {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        edits: Vec<ScalarEdit>,
        #[serde(default)]
        text_edits: Vec<TextEdit>,
        #[serde(default)]
        order_edits: Vec<relocation::Move>,
        #[serde(default)]
        relocations: Vec<hierarchy::Reparent>,
        #[serde(default)]
        focus_task_guid: Option<String>,
    },
    InsertAbove {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        insertion: creation::InsertAbove,
    },
    CreateDefaultWork {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        anchor_task_guid: String,
        expected_parent_guid: Option<String>,
        expected_sibling_index: usize,
        name: String,
        display_id: String,
    },
    DeleteLeaf {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        deletion: deletion::DeleteLeaf,
    },
    Undo {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        #[serde(default)]
        focus_task_guid: Option<String>,
    },
    Redo {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        #[serde(default)]
        focus_task_guid: Option<String>,
    },
    Close {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        #[serde(default)]
        discard_changes: bool,
    },
    WriteCandidate {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
        output_path: String,
        acknowledge_storage_only: bool,
    },
}
pub fn specs() -> Vec<ToolSpec> {
    vec![spec(
        "formats_project_edit",
        "Edit a guarded project draft",
        "Open a source-bound draft, inspect creation/deletion layout hints and apply guarded fields/order/hierarchy, default Work insertion or unassigned root-leaf deletion. Retain bounded atomic history and GUID-focused undo/redo. Post-creation deletion completes only provenance-bound presentation rows and uses version 7; prior deletion/creation exports retain versions 6/5 and legacy drafts keep their versions. At most eight insertions/deletions/relocations; target-reference validation, native saving and general rescheduling remain independent gates.",
        "Preview guarded changes without changing the source project.",
        Toolset::Formats,
        Effects::WRITES_FILESYSTEM.union(Effects::EVIDENCE).union(Effects::CARRIES_TARGET_CONTENT),
        Requires::Nothing,
        schema_of::<Request>,
        run,
    )]
}
fn run(ctx: &mut Ctx<'_>, params: Value) -> Result<ToolOutput, ToolError> {
    let request: Request = parse_params(params)?;
    let mut offset = 0;
    let mut focus = None;
    let mut deleted_focus_index = None;
    let mut session = match &request {
        Request::Open { path, expected_source_sha256 } => {
            if ctx.session.ext.get::<EditSession>().is_some_and(|s| s.draft.is_dirty()) {
                return Err(ToolError::invalid("A dirty draft is already open. Publish it or explicitly close it with discard_changes before replacing it."));
            }
            let input = load_path(path, 256 * 1024 * 1024)?;
            let draft = Draft::open(input.bytes, expected_source_sha256, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
            let id = ctx
                .session
                .ext
                .get::<DraftSequence>()
                .map_or(0, |s| s.0)
                .checked_add(1)
                .ok_or_else(|| ToolError::invalid("Draft identity exhausted; start another engine session."))?;
            EditSession { id, path: path.clone(), draft }
        }
        _ => {
            let existing = ctx.session.ext.get::<EditSession>().ok_or_else(|| ToolError::invalid("Open an edit draft first."))?;
            let (id, expected, revision) = match &request {
                Request::Inspect { draft_id, expected_source_sha256, revision, .. }
                | Request::Apply { draft_id, expected_source_sha256, revision, .. }
                | Request::InsertAbove { draft_id, expected_source_sha256, revision, .. }
                | Request::CreateDefaultWork { draft_id, expected_source_sha256, revision, .. }
                | Request::DeleteLeaf { draft_id, expected_source_sha256, revision, .. }
                | Request::Undo { draft_id, expected_source_sha256, revision, .. }
                | Request::Redo { draft_id, expected_source_sha256, revision, .. }
                | Request::Close { draft_id, expected_source_sha256, revision, .. }
                | Request::WriteCandidate { draft_id, expected_source_sha256, revision, .. } => (draft_id, expected_source_sha256, Some(*revision)),
                Request::Refresh { draft_id, expected_source_sha256, .. } => (draft_id, expected_source_sha256, None),
                Request::Open { .. } => return Err(ToolError::invalid("Open request mismatch.")),
            };
            if existing.id != *id
                || !existing.draft.source_sha256.eq_ignore_ascii_case(expected)
                || revision.is_some_and(|revision| existing.draft.revision != revision)
            {
                return Err(ToolError::invalid("Draft source or revision changed; refresh before continuing."));
            }
            (*existing).clone()
        }
    };
    match request {
        Request::Close { discard_changes, .. } => {
            if session.draft.is_dirty() && !discard_changes {
                return Err(ToolError::invalid("This draft has changes. Publish a candidate or explicitly set discard_changes to close it."));
            }
            if ctx.cancel.is_cancelled() {
                return Err(ToolError::cancelled());
            }
            ctx.session.ext.remove::<EditSession>();
            return Ok(ToolOutput::Value(json!({"version":1,"draft_id":session.id,"closed":true})));
        }
        Request::Apply { revision, edits, text_edits, order_edits, relocations, focus_task_guid, .. } => {
            if !order_edits.is_empty() && !relocations.is_empty() {
                return Err(ToolError::invalid("Use either legacy order_edits or sequential relocations in one Apply request."));
            }
            if relocations.is_empty() {
                session.draft.apply_ordered(revision, &edits, &text_edits, &order_edits, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
            } else {
                session.draft.apply_structural(revision, &edits, &text_edits, &relocations, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
            }
            focus = focus_task_guid;
        }
        Request::InsertAbove { revision, insertion, .. } => {
            session.draft.insert_above(revision, &insertion, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
            focus = Some(insertion.task_guid);
        }
        Request::CreateDefaultWork { revision, anchor_task_guid, expected_parent_guid, expected_sibling_index, name, display_id, .. } => {
            let insertion = creation::InsertAbove {
                template_task_guid: anchor_task_guid.clone(),
                anchor_task_guid,
                expected_parent_guid,
                expected_sibling_index,
                task_guid: fresh_task_identity()?,
                supply_chain_id: fresh_task_identity()?,
                name,
                display_id,
            };
            session.draft.insert_above(revision, &insertion, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
            focus = Some(insertion.task_guid);
        }
        Request::DeleteLeaf { revision, deletion, .. } => {
            let index = deletion.expected_sibling_index;
            session.draft.delete_leaf(revision, &deletion, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
            deleted_focus_index = Some(index);
        }
        Request::Undo { revision, focus_task_guid, .. } => {
            let restored = session.draft.undo_deletion_task_guid().map(str::to_owned);
            let created_focus = focus_task_guid.as_deref().is_some_and(|id| session.draft.creates_task(id));
            session.draft.undo(revision).map_err(ToolError::invalid)?;
            // Undoing an insertion removes its GUID; fall back to the first page.
            focus = focus_task_guid.filter(|id| !created_focus || session.draft.creates_task(id)).or(restored);
        }
        Request::Redo { revision, focus_task_guid, .. } => {
            if let Some(removed) = session.draft.redo_deletion()
                && focus_task_guid.as_deref().is_none_or(|id| id.eq_ignore_ascii_case(&removed.task_guid))
            {
                deleted_focus_index = Some(removed.expected_sibling_index);
            }
            session.draft.redo(revision).map_err(ToolError::invalid)?;
            focus = if deleted_focus_index.is_some() { None } else { focus_task_guid };
        }
        Request::Inspect { offset: value, .. } | Request::Refresh { offset: value, .. } => offset = value,
        Request::WriteCandidate { output_path, acknowledge_storage_only, .. } => {
            if session.draft.has_authored_edits() {
                return write_authored(ctx, &session, &output_path, acknowledge_storage_only);
            }
            if session.draft.has_hierarchy_edits() {
                return project_candidate_tools::run_structured(
                    ctx,
                    json!({"path":session.path,"expected_source_sha256":session.draft.source_sha256,"output_path":output_path,
                    "acknowledge_storage_only":acknowledge_storage_only,"edits":session.draft.edits(),"text_edits":session.draft.text_edits(),"relocations":session.draft.structural_edits()}),
                );
            }
            if !session.draft.order_edits().is_empty() {
                return project_candidate_tools::run_ordered(
                    ctx,
                    json!({"path":session.path,"expected_source_sha256":session.draft.source_sha256,"output_path":output_path,
                        "acknowledge_storage_only":acknowledge_storage_only,"edits":session.draft.edits(),"text_edits":session.draft.text_edits(),
                        "order_edits":session.draft.order_edits()}),
                );
            }
            if !session.draft.text_edits().is_empty() {
                return project_candidate_tools::run_mixed(
                    ctx,
                    json!({"path":session.path,"expected_source_sha256":session.draft.source_sha256,"output_path":output_path,
                        "acknowledge_storage_only":acknowledge_storage_only,"edits":session.draft.edits(),"text_edits":session.draft.text_edits()}),
                );
            }
            return project_candidate_tools::run(
                ctx,
                json!({"path":session.path,"expected_source_sha256":session.draft.source_sha256,"output_path":output_path,"acknowledge_storage_only":acknowledge_storage_only,"edits":session.draft.edits()}),
            );
        }
        Request::Open { .. } => {}
    }
    let raw = session.draft.bytes(|_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?;
    let imported = sp_project::parse_metadata(&raw, |_| !ctx.cancel.is_cancelled()).map_err(|e| format_err(e, "edited draft metadata"))?;
    // Rebuild all derived caches for the edited revision, never mutate display vectors.
    let model = craft_anything_analysis::project_model::ProjectModel::from_metadata_with_cancel(&imported.metadata, || ctx.cancel.is_cancelled())
        .map_err(ToolError::invalid)?;
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let tasks = imported.metadata.get("tasks").and_then(Value::as_array).ok_or_else(|| ToolError::invalid("Draft task inventory is missing."))?;
    if let Some(index) = deleted_focus_index {
        let task = tasks.get(index.min(tasks.len().saturating_sub(1))).ok_or_else(|| ToolError::invalid("Deletion left no task for focus."))?;
        focus = Some(task.get("guid").and_then(Value::as_str).ok_or_else(|| ToolError::invalid("Surviving task identity missing."))?.to_owned());
    }
    let focused_task_guid = focus.clone();
    if let Some(id) = focus {
        if id.len() != 36 {
            return Err(ToolError::invalid("Choose a source task GUID for draft focus."));
        }
        let index = tasks
            .iter()
            .position(|t| t.get("guid").and_then(Value::as_str).is_some_and(|v| v.eq_ignore_ascii_case(&id)))
            .ok_or_else(|| ToolError::invalid("Focused task is absent from this draft."))?;
        offset = index / 100 * 100;
    }
    let outline = relocation::outline(&raw, &imported.source_sha256, |_| !ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let outline = outline.map_err(|e| format_err(e, "draft task-order admission"))?;
    let creation = creation::availability(&raw, &imported.source_sha256, |_| !ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let creation = creation.map_err(|e| format_err(e, "draft creation availability"))?;
    let deletion = session.draft.deletion_availability(&raw, &imported.source_sha256, |_| !ctx.cancel.is_cancelled());
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let deletion = deletion.map_err(ToolError::invalid)?;
    let deletion_anchors: std::collections::BTreeMap<_, _> = deletion.anchors.iter().map(|a| (a.task_guid.as_str(), a)).collect();
    let creation_anchors: std::collections::BTreeMap<_, _> = creation.anchors.iter().map(|a| (a.anchor_task_guid.as_str(), a)).collect();
    let creation_reasons: std::collections::BTreeMap<_, _> = creation.unavailable.iter().map(|r| (r.task_guid.as_str(), r.reason.as_str())).collect();
    let positions: std::collections::BTreeMap<_, _> = outline.positions.iter().map(|p| (p.task_guid.as_str(), p)).collect();
    let hierarchy_hints: std::collections::BTreeMap<_, _> =
        outline.hierarchy_hints.as_deref().unwrap_or(&[]).iter().map(|h| (h.task_guid.as_str(), h)).collect();
    let labels: std::collections::BTreeMap<_, _> = tasks.iter().filter_map(|t| t.get("guid").and_then(Value::as_str).map(|id| (id, t))).collect();
    let parent_label = |id: Option<&str>| -> String {
        let Some(id) = id else { return "Project root".into() };
        let Some(task) = labels.get(id) else { return "Unknown parent".into() };
        let name = task.get("name").and_then(Value::as_str).unwrap_or_default();
        let display = task.get("display_id").and_then(Value::as_str).unwrap_or_default();
        let separator = if display.is_empty() { "" } else { " · " };
        let mut chars = display.chars().chain(separator.chars()).chain(name.chars());
        let mut label: String = chars.by_ref().take(192).collect();
        if chars.next().is_some() {
            label.push('\u{2026}');
        }
        label
    };
    let page = tasks
        .iter()
        .skip(offset)
        .take(100)
        .map(|t| {
            let id = t.get("guid").and_then(Value::as_str).ok_or_else(|| ToolError::invalid("Draft task identity missing."))?;
            let position = positions.get(id).ok_or_else(|| ToolError::invalid("Draft sibling position missing."))?;
            let mut row = t.clone();
            let object = row.as_object_mut().ok_or_else(|| ToolError::invalid("Draft task record missing."))?;
            object.insert("sibling_index".into(), json!(position.sibling_index));
            object.insert("sibling_total".into(), json!(position.sibling_total));
            object.insert("parent_label".into(), json!(parent_label(position.parent_guid.as_deref())));
            object.insert("creation".into(), json!(creation_anchors.get(id)));
            object.insert("creation_reason".into(), json!(creation_reasons.get(id).copied().or(creation.reason.as_deref()).unwrap_or_default()));
            object.insert("deletion".into(), json!(deletion_anchors.get(id)));
            if let Some(hint) = hierarchy_hints.get(id) {
                object.insert("indent".into(), json!(hint.indent));
                object.insert("outdent".into(), json!(hint.outdent));
                object.insert("indent_depth_limited".into(), json!(hint.indent_depth_limited));
                object.insert("indent_label".into(), json!(hint.indent.as_ref().map(|r| parent_label(r.parent_guid.as_deref())).unwrap_or_default()));
                object.insert("outdent_label".into(), json!(hint.outdent.as_ref().map(|r| parent_label(r.parent_guid.as_deref())).unwrap_or_default()));
            }
            Ok(row)
        })
        .collect::<Result<Vec<_>, ToolError>>()?;
    let result = json!({"version":1,"draft_id":session.id,"source_sha256":session.draft.source_sha256,"draft_sha256":imported.source_sha256,
        "revision":session.draft.revision,"dirty":session.draft.is_dirty(),"can_undo":session.draft.can_undo(),"can_redo":session.draft.can_redo(),
        "edit_count":session.draft.edit_count(),"task_total":tasks.len(),"offset":offset,
        "order_edit_count":session.draft.structural_edit_count(),"order_edit_limit":sp_project::ordered::ORDER_LIMIT,
        "creation_count":session.draft.creation_count(),"deletion_count":session.draft.deletion_count(),"candidate_receipt_version":if session.draft.has_authored_edits() {session.draft.authored_receipt_version()} else if session.draft.has_hierarchy_edits() {4} else if !session.draft.order_edits().is_empty() {3} else if !session.draft.text_edits().is_empty() {2} else {1},
        "focused_task_guid":focused_task_guid,"creation_source_sha256":creation.source_sha256,"creation_supported":!creation.anchors.is_empty(),
        "redo_creation_task_guid":session.draft.redo_creation_task_guid(),
        "undo_deletion_task_guid":session.draft.undo_deletion_task_guid(),
        "deletion_source_sha256":deletion.source_sha256,"deletion_layout_supported":!deletion.anchors.is_empty(),
        "deletion_reason":deletion.reason.as_deref().unwrap_or_default(),"deletion_reference_scan_required":deletion.reference_scan_required,
        "creation_reason":creation.reason.as_deref().unwrap_or_default(),"creation_suggested_display_id":creation.suggested_display_id,
        "hierarchy_supported":outline.hierarchy_hints.is_some(),"hierarchy_reason":outline.hierarchy_reason,
        "task_collection_lookup":imported.coverage.task_collection_lookup,
        "order_supported":outline.reason.is_none(),"order_reason":outline.reason.map(|r|r.message()).unwrap_or_default(),
        "tasks":page,"derived_tasks":model.page_tasks(offset,100),
        "native_reopen_verified":false,"derived_schedule_consistency_verified":false});
    // The proposed session is still private until response validation completes.
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    ctx.session.ext.insert(Arc::new(DraftSequence(session.id)));
    ctx.session.ext.insert(Arc::new(session));
    Ok(ToolOutput::Value(result))
}

fn fresh_task_identity() -> Result<String, ToolError> {
    let mut bytes = [0u8; 16];
    getrandom::getrandom(&mut bytes).map_err(|e| {
        ToolError::invalid(format!("Unable to obtain OS entropy for task identities: {e}"))
            .with_remediation("Retry after the system random provider is available.")
    })?;
    let version = bytes.get_mut(6).ok_or_else(|| ToolError::invalid("Task identity buffer is incomplete."))?;
    *version = (*version & 0x0f) | 0x40;
    let variant = bytes.get_mut(8).ok_or_else(|| ToolError::invalid("Task identity buffer is incomplete."))?;
    *variant = (*variant & 0x3f) | 0x80;
    let encoded = hex::encode(bytes);
    [(0, 8), (8, 12), (12, 16), (16, 20), (20, 32)]
        .into_iter()
        .map(|(start, end)| encoded.get(start..end).map(str::to_owned).ok_or_else(|| ToolError::invalid("Task identity encoding is incomplete.")))
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("-"))
}

fn write_authored(ctx: &mut Ctx<'_>, session: &EditSession, output_path: &str, acknowledged: bool) -> Result<ToolOutput, ToolError> {
    use craft_anything_evidence::{Confidence, Limitation};
    use sha2::{Digest, Sha256};
    if !acknowledged {
        return Err(ToolError::invalid("Acknowledge storage-only generation; native saving and scheduling remain unverified."));
    }
    let (output, receipt_path) = project_candidate_tools::output_paths(output_path)?;
    let input = load_path(&session.path, 256 * 1024 * 1024)?;
    if !format!("{:x}", Sha256::digest(&input.bytes)).eq_ignore_ascii_case(&session.draft.source_sha256) {
        return Err(ToolError::invalid("Source changed; reload before exporting the draft."));
    }
    let candidate = session.draft.authored_candidate(|progress| {
        ctx.report(Some(progress), "Replaying guarded authored task edits");
        !ctx.cancel.is_cancelled()
    });
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let candidate = candidate.map_err(ToolError::invalid)?;
    let receipt = serde_json::to_vec_pretty(&candidate.receipt).map_err(|e| ToolError::invalid(e.to_string()))?;
    let evidence = super::static_input::builder(ctx, "formats_project_edit", input.subject,
        json!({"action":"write_candidate","draft_id":session.id,"revision":session.draft.revision,"expected_source_sha256":session.draft.source_sha256,"output_path":output_path,"acknowledge_storage_only":acknowledged}))
        .summary("Prepared a chronological task draft candidate")
        .confidence(Confidence::Derived)
        .result(json!({"output_path":output,"receipt_path":receipt_path,"receipt":candidate.receipt}))
        .limitation(Limitation::new("native_persistence_unverified", "Typed stage hash chains and internal guards passed. Native persistence, geometry and scheduling need independent acceptance."))
        .target_content(true).build()?;
    if ctx.cancel.is_cancelled() {
        return Err(ToolError::cancelled());
    }
    let parent = output.parent().ok_or_else(|| ToolError::invalid("Choose an output parent directory."))?;
    project_candidate_tools::publish(parent, &output, &receipt_path, &candidate.bytes, &receipt)?;
    Ok(ToolOutput::evidence(evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallOptions, Caller, Engine};
    use sha2::{Digest, Sha256};

    fn fixture() -> Vec<u8> {
        fixture_count(1)
    }
    #[test]
    fn created_task_deletion_uses_provenance_bound_hints_v7_and_paged_history() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = craft_anything_testkit::project_authoring::default_work_project_with_presentation(110, Some(100));
        std::fs::write(&path, &raw).unwrap();
        let hash = sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
        let engine = Engine::new(crate::testing::session());
        let call = |p| engine.call("formats_project_edit", p, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let req = |action, revision| json!({"action":action,"draft_id":opened["draft_id"],"expected_source_sha256":hash,"revision":revision});
        let mut inspect = req("inspect", 0);
        inspect["offset"] = json!(100);
        let page = call(inspect).unwrap();
        let anchor = &page["tasks"][0];
        let next = anchor["guid"].clone();
        let mut create = req("create_default_work", 0);
        create["anchor_task_guid"] = anchor["guid"].clone();
        create["expected_parent_guid"] = Value::Null;
        create["expected_sibling_index"] = anchor["sibling_index"].clone();
        create["name"] = json!("Created_งาน_梁_😀");
        create["display_id"] = json!("NEW-1");
        let created = call(create).unwrap();
        let guid = created["focused_task_guid"].clone();
        assert_eq!(created["offset"], 100);
        assert_eq!(created["deletion_layout_supported"], true);
        assert_eq!(created["deletion_source_sha256"], created["draft_sha256"]);
        let task = created["tasks"].as_array().unwrap().iter().find(|t| t["guid"] == guid).unwrap();
        let mut export = req("write_candidate", 1);
        export["output_path"] = json!(dir.path().join("created.sp"));
        export["acknowledge_storage_only"] = json!(true);
        let before = call(export).unwrap()["receipt"]["stages"][0].clone();
        let mut delete = req("delete_leaf", 1);
        delete["deletion"] = task["deletion"].clone();
        let mut forged = delete.clone();
        forged["created_identities"] = json!([]);
        assert!(call(forged).is_err());
        let mut stale = delete.clone();
        stale["deletion"]["expected_display_id"] = json!("wrong");
        assert!(call(stale).is_err());
        assert_eq!(call(req("inspect", 1)).unwrap()["draft_sha256"], created["draft_sha256"]);
        let deleted = call(delete).unwrap();
        assert_eq!(deleted["candidate_receipt_version"], 7);
        assert_eq!(deleted["task_total"], 110);
        assert_eq!(deleted["focused_task_guid"], next);
        assert_eq!(deleted["offset"], 100);
        let undone = call(req("undo", 2)).unwrap();
        assert_eq!(undone["draft_sha256"], created["draft_sha256"]);
        assert_eq!(undone["focused_task_guid"], guid);
        assert_eq!(undone["offset"], 100);
        let mut redo = req("redo", 3);
        redo["focus_task_guid"] = guid;
        let redone = call(redo).unwrap();
        assert_eq!(redone["draft_sha256"], deleted["draft_sha256"]);
        assert_eq!(redone["focused_task_guid"], next);
        let mut export = req("write_candidate", 4);
        export["output_path"] = json!(dir.path().join("deleted.sp"));
        export["acknowledge_storage_only"] = json!(true);
        let receipt = call(export).unwrap()["receipt"].clone();
        assert_eq!(receipt["version"], 7);
        assert_eq!(receipt["stages"][0], before);
        assert_eq!(receipt["stages"][1]["kind"], "presentation_rows");
        assert_eq!(receipt["stages"][1]["receipt"]["source_sha256"], created["draft_sha256"]);
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    #[test]
    fn deletion_draft_focus_undo_redo_and_v6_export_stay_source_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = craft_anything_testkit::project_authoring::default_work_project_with_presentation(110, Some(100));
        std::fs::write(&path, &raw).unwrap();
        let hash = sp_project::parse_metadata(&raw, |_| true).unwrap().source_sha256;
        let engine = Engine::new(crate::testing::session());
        let call = |p| engine.call("formats_project_edit", p, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let base = |action, revision| json!({"action":action,"draft_id":opened["draft_id"],"expected_source_sha256":hash,"revision":revision});
        let mut inspect = base("inspect", 0);
        inspect["offset"] = json!(100);
        let page = call(inspect).unwrap();
        let target = page["tasks"][0]["guid"].clone();
        let next = page["tasks"][1]["guid"].clone();
        assert_eq!(page["deletion_source_sha256"], hash);
        assert_eq!(page["deletion_reference_scan_required"], true);
        let mut delete = base("delete_leaf", 0);
        delete["deletion"] = page["tasks"][0]["deletion"].clone();
        let mut stale = delete.clone();
        stale["deletion"]["expected_display_id"] = json!("wrong");
        assert!(call(stale).is_err());
        assert_eq!(call(base("inspect", 0)).unwrap()["draft_sha256"], hash);
        let deleted = call(delete.clone()).unwrap();
        assert_eq!(deleted["task_total"], 109);
        assert_eq!(deleted["offset"], 100);
        assert_eq!(deleted["focused_task_guid"], next);
        assert_eq!(deleted["undo_deletion_task_guid"], target);
        assert_eq!(deleted["candidate_receipt_version"], 6);
        assert_eq!(deleted["deletion_count"], 1);
        let changed = deleted["draft_sha256"].clone();
        let mut bad_undo = base("undo", 1);
        bad_undo["focus_task_guid"] = json!("00000000-0000-0000-0000-000000000000");
        assert!(call(bad_undo).is_err());
        assert_eq!(call(base("inspect", 1)).unwrap()["draft_sha256"], changed);
        let undone = call(base("undo", 1)).unwrap();
        assert_eq!(undone["draft_sha256"], hash);
        assert_eq!(undone["focused_task_guid"], target);
        assert_eq!(undone["offset"], 100);
        let mut redo = base("redo", 2);
        redo["focus_task_guid"] = target;
        let redone = call(redo).unwrap();
        assert_eq!(redone["draft_sha256"], changed);
        assert_eq!(redone["focused_task_guid"], next);
        assert_eq!(redone["offset"], 100);
        assert!(call(delete).is_err());
        let output = dir.path().join("deleted.sp");
        let mut export = base("write_candidate", 3);
        export["output_path"] = json!(output);
        export["acknowledge_storage_only"] = json!(true);
        let published = call(export).unwrap();
        assert_eq!(published["receipt"]["version"], 6);
        assert_eq!(published["receipt"]["stages"][0]["kind"], "delete");
        assert_eq!(published["receipt"]["stages"][0]["receipt"]["deletion"]["task_guid"], page["tasks"][0]["guid"]);
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    #[test]
    fn failed_response_focus_keeps_apply_undo_redo_history_private() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = craft_anything_testkit::project_authoring::default_work_project_with_presentation(4, Some(0));
        std::fs::write(&path, &raw).unwrap();
        let parsed = sp_project::parse_metadata(&raw, |_| true).unwrap();
        let guid = parsed.metadata["tasks"][0]["guid"].clone();
        let hash = parsed.source_sha256;
        let engine = Engine::new(crate::testing::session());
        let call = |p| engine.call("formats_project_edit", p, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let req = |action, revision| json!({"action":action,"draft_id":opened["draft_id"],"expected_source_sha256":hash,"revision":revision});
        let mut apply = req("apply", 0);
        apply["edits"] = json!([]);
        apply["text_edits"] = json!([{"field":"task_name","task_guid":guid,"expected":"Task1","value":"Edited"}]);
        for bad in ["x", "00000000-0000-0000-0000-000000000000"] {
            apply["focus_task_guid"] = json!(bad);
            assert!(call(apply.clone()).is_err());
            let same = call(req("inspect", 0)).unwrap();
            assert_eq!(same["draft_sha256"], hash);
            assert_eq!(same["can_undo"], false);
            assert_eq!(same["tasks"][0]["name"], "Task1");
        }
        apply["focus_task_guid"] = guid.clone();
        let changed = call(apply).unwrap();
        let changed_hash = changed["draft_sha256"].clone();
        let mut undo = req("undo", 1);
        undo["focus_task_guid"] = json!("00000000-0000-0000-0000-000000000000");
        assert!(call(undo.clone()).is_err());
        assert_eq!(call(req("inspect", 1)).unwrap()["draft_sha256"], changed_hash);
        undo["focus_task_guid"] = guid.clone();
        let undone = call(undo).unwrap();
        assert_eq!(undone["draft_sha256"], hash);
        let mut redo = req("redo", 2);
        redo["focus_task_guid"] = json!("00000000-0000-0000-0000-000000000000");
        assert!(call(redo.clone()).is_err());
        let same = call(req("inspect", 2)).unwrap();
        assert_eq!(same["draft_sha256"], hash);
        assert_eq!(same["can_redo"], true);
        redo["focus_task_guid"] = guid;
        assert_eq!(call(redo).unwrap()["draft_sha256"], changed_hash);
        let cancel = crate::CancelToken::default();
        cancel.cancel();
        assert!(engine.call("formats_project_edit", req("undo", 3), CallOptions::new(Caller::Cli).with_cancel(cancel)).is_err());
        assert_eq!(call(req("inspect", 3)).unwrap()["draft_sha256"], changed_hash);
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    #[test]
    fn generated_creation_ids_focus_cross_pages_and_remain_stable_only_on_redo() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = craft_anything_testkit::project_authoring::default_work_project(110);
        let initial = sp_project::parse_metadata(&raw, |_| true).unwrap();
        let raw = sp_project::task_strings::candidate(
            &raw,
            &initial.source_sha256,
            &[TextEdit::TaskDisplayId {
                task_guid: initial.metadata["tasks"][109]["guid"].as_str().unwrap().to_owned(),
                expected: "ID110".into(),
                value: "work-1".into(),
            }],
            |_| true,
        )
        .unwrap()
        .bytes;
        std::fs::write(&path, &raw).unwrap();
        let parsed = sp_project::parse_metadata(&raw, |_| true).unwrap();
        let hash = parsed.source_sha256;
        let anchor = parsed.metadata["tasks"][100]["guid"].clone();
        let engine = Engine::new(crate::testing::session());
        let call = |params| engine.call("formats_project_edit", params, CallOptions::new(Caller::Cli)).map(|v| v.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        assert_eq!(opened["creation_supported"], true);
        assert_eq!(opened["creation_source_sha256"], hash);
        assert_eq!(opened["creation_suggested_display_id"], "WORK-2");
        let req = |action, revision| json!({"action":action,"draft_id":opened["draft_id"],"expected_source_sha256":hash,"revision":revision});
        let mut create = req("create_default_work", 0);
        create["anchor_task_guid"] = anchor.clone();
        create["expected_parent_guid"] = Value::Null;
        create["expected_sibling_index"] = json!(100);
        create["name"] = json!("New_\u{0e07}\u{0e32}\u{0e19}_\u{6881}");
        create["display_id"] = json!("WORK-2");
        let first = call(create.clone()).unwrap();
        let first_id = first["focused_task_guid"].as_str().unwrap().to_owned();
        assert_eq!(first["offset"], 100);
        assert_eq!(first["tasks"][0]["guid"], first_id);
        assert_eq!(first["creation_suggested_display_id"], "WORK-3");
        assert_eq!(first_id.as_bytes()[14], b'4');
        assert!(b"89ab".contains(&first_id.as_bytes()[19]));
        let mut undo = req("undo", 1);
        undo["focus_task_guid"] = json!(first_id);
        let undone = call(undo).unwrap();
        assert_eq!(undone["dirty"], false);
        assert!(undone["focused_task_guid"].is_null());
        assert_eq!(undone["redo_creation_task_guid"], first_id);
        let mut redo = req("redo", 2);
        redo["focus_task_guid"] = json!(first_id);
        let redone = call(redo).unwrap();
        assert_eq!(redone["draft_sha256"], first["draft_sha256"]);
        assert_eq!(redone["focused_task_guid"], first_id);
        call(req("undo", 3)).unwrap();
        create["revision"] = json!(4);
        let branch = call(create.clone()).unwrap();
        let branch_id = branch["focused_task_guid"].as_str().unwrap();
        assert_ne!(branch_id, first_id);
        assert_eq!(branch["can_redo"], false);
        assert!(call(create.clone()).is_err());
        create["revision"] = json!(5);
        assert!(call(create).is_err());
        assert_eq!(call(req("inspect", 5)).unwrap()["draft_sha256"], branch["draft_sha256"]);
        let output = dir.path().join("created.sp");
        let mut export = req("write_candidate", 5);
        export["output_path"] = json!(output);
        export["acknowledge_storage_only"] = json!(true);
        let published = call(export).unwrap();
        assert_eq!(published["receipt"]["version"], 5);
        let inserted = &published["receipt"]["stages"][0]["receipt"]["insertion"];
        assert_eq!(inserted["task_guid"], branch_id);
        assert_ne!(inserted["task_guid"], inserted["supply_chain_id"]);
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    #[test]
    fn rejected_insertion_does_not_replace_the_current_draft_revision() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.sp");
        let raw = fixture();
        std::fs::write(&source, &raw).unwrap();
        let hash = format!("{:x}", Sha256::digest(&raw));
        let engine = Engine::new(crate::testing::session());
        let call = |params| engine.call("formats_project_edit", params, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":source,"expected_source_sha256":hash})).unwrap();
        let task = opened["tasks"][0]["guid"].as_str().unwrap();
        let base = json!({"action":"inspect","draft_id":opened["draft_id"],"expected_source_sha256":hash,"revision":0});
        let before = call(base.clone()).unwrap();
        let mut insert = base.clone();
        insert["action"] = json!("insert_above");
        insert["insertion"] = json!({"template_task_guid":task,"anchor_task_guid":task,"expected_parent_guid":null,"expected_sibling_index":0,
            "task_guid":"77777777-7777-7777-7777-777777777777","supply_chain_id":"78787878-7878-7878-7878-787878787878","name":"New Work","display_id":"NEW-1"});
        // This otherwise readable source has no admitted default calendar/state definitions.
        assert!(call(insert).is_err());
        assert_eq!(call(base).unwrap(), before);
        assert_eq!(std::fs::read(source).unwrap(), raw);
    }
    fn fixture_count(count: u8) -> Vec<u8> {
        let open = [0x7a, 0x54, 0xaf, 0x39];
        let close = [0x16, 0xe4, 0x99, 0x6f];
        let mut task = vec![0; 29];
        task[..4].copy_from_slice(&[0x76, 0x25, 0xec, 0x04]);
        task[12..28].fill(0x11);
        task.extend(open);
        task.extend(0u32.to_le_bytes());
        task.extend(close);
        task.extend([0; 4]);
        task.extend(open);
        task.extend(0u32.to_le_bytes());
        task.extend(close);
        task.extend([0; 16]);
        task.extend([255, 254, 255, 1, b'A', 0]);
        task.extend([0; 184]);
        task.extend([0xd0, 0xbc, 0xa1, 0x06]);
        task.extend(0u32.to_le_bytes());
        task.extend([0x2c, 0x57, 0xd9, 0xda]);
        task.extend([255, 254, 255, 1, b'1', 0]);
        let tail = task.len();
        task.extend([0; 121]);
        task[tail + 25..tail + 29].copy_from_slice(&1u32.to_le_bytes());
        task[tail + 101..tail + 105].copy_from_slice(&open);
        task[tail + 109..tail + 113].copy_from_slice(&close);
        task[tail + 113..tail + 117].copy_from_slice(&[0x92, 0xd4, 0x70, 0xa2]);
        let mut section = vec![0; 24];
        section.extend(open);
        section.extend(u32::from(count).to_le_bytes());
        for i in 0..count {
            let mut row = task.clone();
            row[12..28].fill(0x11 + i);
            let end = row.len();
            row[end - 4..].copy_from_slice(&u32::from(i).to_le_bytes());
            section.extend(row);
        }
        section.extend(close);
        let mut raw = super::super::scene_project_tools::tests::empty_metadata_project();
        let old_end = u64::from_le_bytes(raw[128..136].try_into().unwrap()) as usize;
        let mut stored = b"\x01\x05Tasks".to_vec();
        stored.extend(craft_anything_testkit::games::zlib_stored(&section));
        let delta = stored.len() - (old_end - 2344);
        raw.splice(2344..old_end, stored);
        for i in 1..48 {
            let at = 80 + i * 48;
            let offset = u64::from_le_bytes(raw[at..at + 8].try_into().unwrap());
            raw[at..at + 8].copy_from_slice(&(offset + delta as u64).to_le_bytes());
        }
        raw
    }
    #[test]
    fn metadata_inspection_reports_bad_lookup_without_replacing_a_draft_or_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.sp");
        let bad_path = dir.path().join("bad-lookup.sp");
        let raw = fixture_count(3);
        let imported = sp_project::parse_metadata(&raw, |_| true).unwrap();
        let start = 2344;
        let end = u64::from_le_bytes(raw[128..136].try_into().unwrap()) as usize;
        // This fixture uses one stored deflate block; rebuild its checksum after corruption.
        let mut task_data = raw[start + 14..end - 4].to_vec();
        assert_eq!(task_data.len(), imported.metadata["sections"]["Tasks"]["decoded_bytes"].as_u64().unwrap() as usize);
        let task = &imported.metadata["tasks"][0];
        let at = (task["offset"].as_u64().unwrap() + task["bytes"].as_u64().unwrap() - 4) as usize;
        task_data[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        let mut stored = b"\x01\x05Tasks".to_vec();
        stored.extend(craft_anything_testkit::games::zlib_stored(&task_data));
        assert_eq!(stored.len(), end - start);
        let mut bad = raw.clone();
        bad[start..end].copy_from_slice(&stored);
        std::fs::write(&source, &raw).unwrap();
        std::fs::write(&bad_path, &bad).unwrap();
        let engine = Engine::new(crate::testing::session());
        let call = |id, params| engine.call(id, params, CallOptions::new(Caller::Cli)).map(|o| o.to_json());
        let before = call("formats_project_tasks_list", json!({"path":source})).unwrap()["result"].clone();
        let opened =
            call("formats_project_edit", json!({"action":"open","path":source,"expected_source_sha256":imported.source_sha256})).unwrap()["result"].clone();
        let inspected = call("formats_project_metadata_inspect", json!({"path":bad_path})).unwrap();
        let result = &inspected["result"];
        assert_eq!(result["version"], 1);
        assert_eq!(result["task_count"], 3);
        assert_eq!(result["task_record_framing_validated"], true);
        assert_eq!(result["coverage"]["geometry_validated"], false);
        let lookup = &result["coverage"]["task_collection_lookup"];
        assert_eq!(lookup["status"], "invalid");
        assert_eq!(lookup["entries_checked"], 3);
        assert_eq!(lookup["mismatched_entries"], 1);
        assert_eq!(lookup["native_row_identity_verified"], false);
        assert!(inspected["limitations"].as_array().unwrap().iter().any(|v| v["code"] == "invalid_task_lookup"));
        for (id, params) in
            [("formats_project_tasks_list", json!({"path":bad_path})), ("formats_project_frame_evaluate", json!({"path":bad_path,"focus_wallclock":0}))]
        {
            let output = call(id, params).unwrap();
            assert_eq!(output["result"]["source"]["task_collection_lookup"]["status"], "invalid");
            assert_eq!(output["result"]["source"]["native_row_identity_verified"], false);
            assert!(output["limitations"].as_array().unwrap().iter().any(|v| v["code"] == "invalid_task_lookup"));
        }
        let latest = engine.session().ext.get::<super::super::scene_project_tools::LatestMetadataFrame>().unwrap();
        assert_eq!(latest.source_sha256, format!("{:x}", Sha256::digest(&bad)));
        assert_eq!(latest.task_collection_lookup.as_ref().unwrap().status, sp_project::task_lookup::Status::Invalid);
        let valid = call("formats_project_metadata_inspect", json!({"path":source})).unwrap();
        assert_eq!(valid["result"]["coverage"]["task_collection_lookup"]["status"], "valid");
        let draft = call(
            "formats_project_edit",
            json!({"action":"inspect","draft_id":opened["draft_id"],"revision":0,"expected_source_sha256":imported.source_sha256}),
        )
        .unwrap();
        assert_eq!(draft["result"]["draft_sha256"], imported.source_sha256);
        assert_eq!(draft["result"]["dirty"], false);
        assert_eq!(draft["result"]["task_collection_lookup"]["status"], "valid");
        assert_eq!(call("formats_project_tasks_list", json!({"path":source})).unwrap()["result"], before);
        assert_eq!(std::fs::read(source).unwrap(), raw);
        assert_eq!(std::fs::read(bad_path).unwrap(), bad);
    }
    #[test]
    fn hierarchy_hints_cross_pages_and_same_preorder_parent_changes_stay_guarded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("hierarchy.sp");
        let raw = fixture_count(130);
        std::fs::write(&path, &raw).unwrap();
        let hash = format!("{:x}", Sha256::digest(&raw));
        let metadata = sp_project::parse_metadata(&raw, |_| true).unwrap();
        let selected = metadata.metadata["tasks"][100]["guid"].as_str().unwrap().to_owned();
        let parent = metadata.metadata["tasks"][99]["guid"].clone();
        let engine = Engine::new(crate::testing::session());
        engine.call("formats_project_tasks_list", json!({"path":path}), CallOptions::new(Caller::Cli)).unwrap();
        let call = |p| engine.call("formats_project_edit", p, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let id = opened["draft_id"].as_u64().unwrap();
        let request = |action, revision| json!({"action":action,"draft_id":id,"expected_source_sha256":hash,"revision":revision});
        let page = call(json!({"action":"refresh","draft_id":id,"expected_source_sha256":hash,"offset":100})).unwrap();
        assert_eq!(page["hierarchy_supported"], true);
        assert_eq!(page["tasks"][0]["indent"]["parent_guid"], parent);
        let mut apply = request("apply", 0);
        apply["edits"] = json!([]);
        apply["relocations"] = json!([page["tasks"][0]["indent"]]);
        apply["focus_task_guid"] = json!(selected);
        let changed = call(apply).unwrap();
        assert_eq!(changed["offset"], 100);
        assert_eq!(changed["tasks"][0]["guid"], selected);
        assert_eq!(changed["tasks"][0]["parent_guid"], parent);
        assert_eq!(changed["dirty"], true);
        assert_eq!(changed["order_edit_count"], 1);
        assert!(call(json!({"action":"open","path":path,"expected_source_sha256":hash})).is_err());
        let output = dir.path().join("hierarchy-candidate.sp");
        let mut export = request("write_candidate", 1);
        export["output_path"] = json!(output);
        export["acknowledge_storage_only"] = json!(true);
        call(export.clone()).unwrap();
        assert!(call(export).is_err());
        let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
        assert_eq!(receipt["version"], 4);
        assert_eq!(receipt["structural_stages"][0]["kind"], "hierarchy");
        let mut undo = request("undo", 1);
        undo["focus_task_guid"] = json!(selected);
        let undone = call(undo).unwrap();
        assert_eq!(undone["dirty"], false);
        assert!(undone["tasks"][0]["parent_guid"].is_null());
        let mut redo = request("redo", 2);
        redo["focus_task_guid"] = json!(selected);
        assert_eq!(call(redo).unwrap()["tasks"][0]["parent_guid"], parent);
        let original = engine.call("formats_project_tasks_list", json!({"path":path}), CallOptions::new(Caller::Cli)).unwrap().to_json();
        assert!(original["result"]["page"]["items"].as_array().unwrap().iter().all(|t| t["parent_guid"].is_null()));
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    #[test]
    fn invalid_focus_or_ambiguous_structural_request_does_not_publish_a_parent_change() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = fixture_count(3);
        std::fs::write(&path, &raw).unwrap();
        let hash = format!("{:x}", Sha256::digest(&raw));
        let engine = Engine::new(crate::testing::session());
        let call = |p| engine.call("formats_project_edit", p, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let open = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let id = open["draft_id"].clone();
        let mut apply = json!({"action":"apply","draft_id":id,"revision":0,"expected_source_sha256":hash,"edits":[],"relocations":[open["tasks"][1]["indent"]],"focus_task_guid":"00000000-0000-0000-0000-000000000000"});
        assert!(call(apply.clone()).is_err());
        apply.as_object_mut().unwrap().remove("focus_task_guid");
        apply["order_edits"] = json!([{"task_guid":open["tasks"][2]["guid"],"expected_parent_guid":null,"expected_sibling_index":2,"sibling_index":0}]);
        assert!(call(apply).is_err());
        let inspected = call(json!({"action":"inspect","draft_id":id,"revision":0,"expected_source_sha256":hash})).unwrap();
        assert_eq!(inspected["draft_sha256"], hash);
        assert_eq!(inspected["dirty"], false);
        assert_eq!(std::fs::read(path).unwrap(), raw);
    }
    #[test]
    fn ordered_draft_focus_pages_history_and_publication_stay_source_bound() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = fixture_count(130);
        std::fs::write(&path, &raw).unwrap();
        let hash = format!("{:x}", Sha256::digest(&raw));
        let metadata = sp_project::parse_metadata(&raw, |_| true).unwrap();
        let selected = metadata.metadata["tasks"][129]["guid"].as_str().unwrap().to_owned();
        let original_first = metadata.metadata["tasks"][0]["guid"].clone();
        let engine = Engine::new(crate::testing::session());
        engine.call("formats_project_tasks_list", json!({"path":path}), CallOptions::new(Caller::Cli)).unwrap();
        let call = |p| engine.call("formats_project_edit", p, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let id = opened["draft_id"].as_u64().unwrap();
        assert_eq!(opened["order_supported"], true);
        let request = |action, revision| json!({"action":action,"draft_id":id,"expected_source_sha256":hash,"revision":revision});
        let refreshed = call(json!({"action":"refresh","draft_id":id,"expected_source_sha256":hash,"offset":100})).unwrap();
        assert_eq!(refreshed["tasks"][29]["sibling_index"], 129);
        assert_eq!(refreshed["tasks"][29]["sibling_total"], 130);
        let mut apply = request("apply", 0);
        apply["edits"] = json!([]);
        apply["text_edits"] = json!([{"field":"task_name","task_guid":selected,"expected":"A","value":"Moved_\u{6881}_\u{1f600}"}]);
        apply["order_edits"] = json!([{"task_guid":selected,"expected_parent_guid":null,"expected_sibling_index":129,"sibling_index":0}]);
        apply["focus_task_guid"] = json!(selected);
        let changed = call(apply).unwrap();
        assert_eq!(changed["offset"], 0);
        assert_eq!(changed["tasks"][0]["guid"], selected);
        assert_eq!(changed["order_edit_count"], 1);
        let mut invalid = request("apply", 1);
        invalid["edits"] = json!([{"field":"task_duration","task_guid":original_first,"expected_storage":0,"value_storage":42}]);
        invalid["focus_task_guid"] = json!("00000000-0000-0000-0000-000000000000");
        assert!(call(invalid).is_err());
        assert_eq!(call(request("inspect", 1)).unwrap()["draft_sha256"], changed["draft_sha256"]);
        let mut undo = request("undo", 1);
        undo["focus_task_guid"] = json!(selected);
        let undone = call(undo).unwrap();
        assert_eq!(undone["offset"], 100);
        assert_eq!(undone["tasks"][29]["guid"], selected);
        assert_eq!(undone["dirty"], false);
        let mut redo = request("redo", 2);
        redo["focus_task_guid"] = json!(selected);
        let redone = call(redo).unwrap();
        assert_eq!(redone["offset"], 0);
        assert_eq!(redone["tasks"][0]["guid"], selected);
        let output = dir.path().join("ordered.sp");
        let mut export = request("write_candidate", 3);
        export["output_path"] = json!(output);
        export["acknowledge_storage_only"] = json!(true);
        call(export.clone()).unwrap();
        assert!(call(export).is_err());
        let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
        assert_eq!(receipt["version"], 3);
        assert_eq!(receipt["mixed_stage"]["output_sha256"], receipt["order_stages"][0]["source_sha256"]);
        assert_eq!(receipt["source_sha256"], hash);
        let original = engine.call("formats_project_tasks_list", json!({"path":path}), CallOptions::new(Caller::Cli)).unwrap().to_json();
        assert_eq!(original["result"]["page"]["items"][0]["guid"], original_first);
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        std::fs::write(&path, b"stale source").unwrap();
        let mut export = request("write_candidate", 3);
        export["output_path"] = json!(dir.path().join("stale.sp"));
        export["acknowledge_storage_only"] = json!(true);
        assert!(call(export).is_err());
        assert!(!dir.path().join("stale.sp").exists());
    }
    #[test]
    fn mixed_draft_exports_version_two_and_keeps_original_source_guards() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = fixture();
        std::fs::write(&path, &raw).unwrap();
        let hash = format!("{:x}", Sha256::digest(&raw));
        let engine = Engine::new(crate::testing::session());
        let call = |params| engine.call("formats_project_edit", params, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let id = opened["draft_id"].as_u64().unwrap();
        let request = |action, revision| json!({"action":action,"draft_id":id,"revision":revision,"expected_source_sha256":hash});
        let mut apply = request("apply", 0);
        apply["edits"] = json!([{"field":"task_duration","task_guid":"11111111-1111-1111-1111-111111111111","expected_storage":0,"value_storage":42}]);
        apply["text_edits"] = json!([{"field":"task_name","task_guid":"11111111-1111-1111-1111-111111111111","expected":"A","value":"\u{6881}_\u{1f600}"}]);
        let changed = call(apply).unwrap();
        assert_eq!(changed["edit_count"], 2);
        assert_eq!(changed["tasks"][0]["name"], "\u{6881}_\u{1f600}");
        assert_eq!(changed["tasks"][0]["duration_storage_scalar"], 42);
        let output = dir.path().join("mixed.sp");
        let mut export = request("write_candidate", 1);
        export["output_path"] = json!(output);
        export["acknowledge_storage_only"] = json!(true);
        call(export.clone()).unwrap();
        let receipt: Value = serde_json::from_slice(&std::fs::read(output.with_extension("sp.receipt.json")).unwrap()).unwrap();
        assert_eq!(receipt["version"], 2);
        assert_eq!(receipt["scalar_stage"]["output_sha256"], receipt["text_stage"]["source_sha256"]);
        assert_eq!(receipt["source_sha256"], hash);
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert!(call(export).is_err());
        let undone = call(request("undo", 1)).unwrap();
        assert_eq!(undone["dirty"], false);
        let redone = call(request("redo", 2)).unwrap();
        assert_eq!(redone["tasks"][0]["name"], "\u{6881}_\u{1f600}");
        let stale = dir.path().join("stale.sp");
        let mut export = request("write_candidate", 3);
        export["output_path"] = json!(stale);
        export["acknowledge_storage_only"] = json!(true);
        std::fs::write(&path, b"changed source").unwrap();
        assert!(call(export).is_err());
        assert!(!stale.exists());
        assert_eq!(call(request("inspect", 3)).unwrap()["dirty"], true);
    }
    #[test]
    fn engine_draft_guards_transactions_publication_and_reopened_session_identity() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.sp");
        let raw = fixture();
        std::fs::write(&path, &raw).unwrap();
        let hash = format!("{:x}", Sha256::digest(&raw));
        let engine = Engine::new(crate::testing::session());
        let call = |params| engine.call("formats_project_edit", params, CallOptions::new(Caller::Cli)).map(|o| o.to_json()["result"].clone());
        let opened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        let id = opened["draft_id"].as_u64().unwrap();
        let request = |action, revision| json!({"action":action,"draft_id":id,"revision":revision,"expected_source_sha256":hash});
        let edit = |expected, value| json!({"field":"task_duration","task_guid":"11111111-1111-1111-1111-111111111111","expected_storage":expected,"value_storage":value});
        let mut apply = request("apply", 0);
        apply["edits"] = json!([edit(0, 42)]);
        let changed = call(apply.clone()).unwrap();
        assert_eq!(changed["revision"], 1);
        assert_eq!(changed["tasks"][0]["duration_storage_scalar"], 42);
        assert_ne!(changed["draft_sha256"], opened["draft_sha256"]);
        assert!(call(apply).is_err());
        assert!(call(request("inspect", 0)).is_err());
        let refresh = json!({"action":"refresh","draft_id":id,"expected_source_sha256":hash});
        let refreshed = call(refresh.clone()).unwrap();
        assert_eq!(refreshed["revision"], 1);
        assert_eq!(refreshed["draft_sha256"], changed["draft_sha256"]);
        let mut wrong_source = refresh.clone();
        wrong_source["expected_source_sha256"] = json!("0".repeat(64));
        assert!(call(wrong_source).is_err());
        assert!(call(json!({"action":"open","path":path,"expected_source_sha256":hash})).is_err());
        assert!(call(request("close", 1)).is_err());
        let undone = call(request("undo", 1)).unwrap();
        assert_eq!(undone["dirty"], false);
        assert_eq!(undone["draft_sha256"], opened["draft_sha256"]);
        let redone = call(request("redo", 2)).unwrap();
        assert_eq!(redone["draft_sha256"], changed["draft_sha256"]);
        let output = dir.path().join("candidate.sp");
        let mut publish = request("write_candidate", 3);
        publish["output_path"] = json!(output);
        publish["acknowledge_storage_only"] = json!(true);
        call(publish.clone()).unwrap();
        assert!(call(publish.clone()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert!(std::fs::read_to_string(output.with_extension("sp.receipt.json")).unwrap().contains("\"native_reopen_verified\": false"));
        let parsed = sp_project::parse_metadata(&std::fs::read(&output).unwrap(), |_| true).unwrap();
        assert_eq!(parsed.metadata["tasks"][0]["duration_storage_scalar"], 42);
        // Changing the source on disk refuses publication, even though the draft snapshot survives.
        std::fs::write(&path, b"changed").unwrap();
        let rejected = dir.path().join("stale.sp");
        publish["output_path"] = json!(rejected);
        assert!(call(publish).is_err());
        assert!(!rejected.exists());
        std::fs::write(&path, &raw).unwrap();
        let mut close = request("close", 3);
        close["discard_changes"] = json!(true);
        call(close).unwrap();
        let reopened = call(json!({"action":"open","path":path,"expected_source_sha256":hash})).unwrap();
        assert_ne!(reopened["draft_id"], id);
        assert!(call(refresh).is_err());
        assert!(call(request("inspect", 0)).is_err());
    }
}
