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
use craft_anything_formats::sp_project::{self, draft::Draft, task_strings::TextEdit, writer::ScalarEdit};
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
    },
    Undo {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
    },
    Redo {
        draft_id: u64,
        expected_source_sha256: String,
        revision: u64,
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
        "Open a source-bound scalar/text draft, apply atomic edits with bounded history, undo/redo or publish a separate experimental SP candidate. Does not admit native saving or rescheduling.",
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
                | Request::Undo { draft_id, expected_source_sha256, revision }
                | Request::Redo { draft_id, expected_source_sha256, revision }
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
        Request::Apply { revision, edits, text_edits, .. } => {
            session.draft.apply_mixed(revision, &edits, &text_edits, |_| !ctx.cancel.is_cancelled()).map_err(ToolError::invalid)?
        }
        Request::Undo { revision, .. } => session.draft.undo(revision).map_err(ToolError::invalid)?,
        Request::Redo { revision, .. } => session.draft.redo(revision).map_err(ToolError::invalid)?,
        Request::Inspect { offset: value, .. } | Request::Refresh { offset: value, .. } => offset = value,
        Request::WriteCandidate { output_path, acknowledge_storage_only, .. } => {
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
    let result = json!({"version":1,"draft_id":session.id,"source_sha256":session.draft.source_sha256,"draft_sha256":imported.source_sha256,
        "revision":session.draft.revision,"dirty":session.draft.is_dirty(),"can_undo":session.draft.can_undo(),"can_redo":session.draft.can_redo(),
        "edit_count":session.draft.edit_count(),"task_total":tasks.len(),"offset":offset,
        "tasks":tasks.iter().skip(offset).take(100).collect::<Vec<_>>(),"derived_tasks":model.page_tasks(offset,100),
        "native_reopen_verified":false,"derived_schedule_consistency_verified":false});
    ctx.session.ext.insert(Arc::new(DraftSequence(session.id)));
    ctx.session.ext.insert(Arc::new(session));
    Ok(ToolOutput::Value(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CallOptions, Caller, Engine};
    use sha2::{Digest, Sha256};

    fn fixture() -> Vec<u8> {
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
        section.extend(1u32.to_le_bytes());
        section.extend(task);
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
