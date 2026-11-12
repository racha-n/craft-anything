# 4D/5D completion acceptance

The user explicitly expanded the goal to editing schedules/costs and saving native SP files
that reopen in the source application. Existing viewer/export completion does not finish this goal.

| Gate | Required behavior | Current evidence |
|---|---|---|
| Native persistence | New SP output, source hash precondition, preserved unknown sections, independent native open/save/reopen of intended edits | Reader offsets known; writer and native reopen not certified |
| 4D authoring | Task/hierarchy/resource assignments, calendars, dependency types/lags, rescheduling, CPM and baselines | Reader/calendar/preview available; editing and scheduling absent; link semantics need independent controls |
| 5D authoring | Source-bound quantities, resource rates, budgets/progress/expenses, sampled cost curves and persistence | Bounded source evaluators available; editing/charts and additional kinds/units/currencies remain open |
| Validation | Supported-case matrix, hostile-input tests, source guards, UI/CLI parity, native round trips and honest disabled controls | Prior viewer matrix exists; authoring gates remain open |

Work from new copies and preserve original/native guard hashes. Reparse success is an internal
format check, not native reopen proof. A storage scalar patch is not certified rescheduling;
derived dates and relationships require their own native controls. Do not infer engineering
units, mixed-currency aggregates or native assignment identity from raw storage fields.

First implementation: bounded scalar edits and no-clobber native-copy output, retaining
all opaque streams and recording each before/after byte range. Keep native persistence
admission distinct from candidate generation. Later phases must rebuild derived model caches
from edited source data and retain explicit source/plan identity through UI, CLI and MCP.

The current Windows Computer Use bridge reports native-pipe unavailable. Native UI verification
is outstanding until the supported connection is restored; static/reader checks cannot close it.

## Experimental scalar candidate contract

`formats_project_sp_candidate_write` accepts an explicit source path, expected source SHA-256,
new output `.sp` path, `acknowledge_storage_only: true` and zero to 256 guarded edits.
An empty edit list creates a byte-identical control, including the original compressed Tasks stream.
Supported fields are `task_start`, `task_duration`, `direct_fixed_amount` and
`assignment_planned_units`, plus the raw `assignment_planned_rate` utilization window.
Each edit identifies a task GUID and expected raw storage value.
New duration values are capped at 1,000,000 raw units, starts precede the observed unset-date
sentinel, and monetary/quantity values must be finite and between zero and 1e12 raw units.
Fixed costs additionally require their source cost GUID; assignment edits require their ordinal
within the task plus matching resource/profile GUIDs. A profile GUID is not an assignment ID.
The utilization window is not a monetary price or a universal units/day value. Quantity and
utilization are independently guarded fields that can be patched atomically in one transaction.
No automatic quantity/duration/rate normalization policy is admitted by this storage tool.

The candidate retains all 48 section payloads except Tasks, changes only the requested eight-byte
scalar ranges in decoded Tasks, recompresses that stream and updates directory offsets. It rejects
overlapping edits, stale source/value guards, unsupported containers and nonfinite amounts.
The new file is internally reparsed. The adjacent `.sp.receipt.json` records version 1, source/output
hashes, each byte change and per-section preservation hashes. Neither destination is replaced.
The receipt schema is [version 1](schemas/sp-candidate-receipt-v1.schema.json); there is no
older receipt version to migrate. Future readers must reject unrecognized versions.
If receipt publication fails after SP publication, the error explicitly reports the partial output.

This is a storage experiment, not an admitted native Save command. The receipt always records
`native_reopen_verified: false` and `derived_schedule_consistency_verified: false`.
Task scalar changes do not update assignments, dependencies, calendars or summary bounds.
Native Save and authoring controls remain disabled pending independent acceptance evidence.

## Isolated scalar drafts

`formats_project_edit` opens a source-hash-bound snapshot. Every subsequent request must match
the returned `draft_id`, source hash and revision. Applying a transaction validates all fields
before replacing history, then rebuilds parsed metadata and derived task/cost caches. The loaded
viewport and explicit-path metadata caches are separate and remain tied to their original source.
The draft is an isolated experimental preview, not an edited viewport or an admitted native Save.

Undo/redo advance revisions. Returning all scalars to their original bytes clears dirty state;
branching after Undo discards redo history. History is bounded to 64 transactions and at most
256 net edited fields. A dirty draft cannot be replaced by Open or closed unless the caller
explicitly supplies `discard_changes: true`. Publishing a candidate rechecks the source file's
current hash and retains no-clobber publication and the native acceptance limitations above.
Publishing does not clear dirty state or turn the candidate into the source. Close/reopen uses a
fresh draft identity, so a stale revision-zero client cannot address the replacement draft.
Drafts are session-local and are not persisted across engine restarts.

## Sampled cost curves

`formats_project_cost_curve` requires the expected source hash and samples 2..512 inclusive
displayed-clock points with a 250,000 task/sample cap and cancellation. It uses the same declared
evaluator as timeline frames. Direct budgets, resource forecasts, planned value, recorded expense
totals and partial actual/remaining values remain separate. Each series retains unresolved counts;
zero partial totals must never be presented as complete actual costs when those counts are nonzero.
Missing expense collections remain null, distinct from a known empty collection. The curve is a
headless experimental result; chart UI, native parity and currency conversion remain open.

## Remaining acceptance order

1. Validate byte-identical no-edit and separate one-field candidate controls in the native app:
   start, duration, direct Fixed amount and assignment planned quantity. Inspect intended values,
   unrelated task/resource/geometry inventories, native Save As/reopen, then parse native-saved output.
2. Integrate admitted edit sessions into UI with refreshed derived views and native-save receipts.
3. Establish independent relationship/lag/calendar/constraint controls before structural scheduling,
   resource/profile authoring and general 5D persistence.
4. Complete remaining exchange/report/workspace commands within their supported feature matrix.
5. Run fresh corpus/hostile/UI/CLI/MCP/native roundtrip and release/package checks on frozen source.

The native control connection was unavailable at the start of the 2026-11-13 continuation.
Headless tests and prepared candidates cannot satisfy step 1. No later native acceptance gate
is closed by the experimental draft or curve APIs.

Subsequent isolated native controls used the user's explicitly authorized alternative automation.
No-edit, start, duration, quantity and direct Fixed candidates have native persistence controls;
they do not establish full native edit equivalence. An independent native UI quantity change
from 5 to 7.5 also changes the planned utilization window from 166.66666666666666 to 250.
The quantity-only candidate retains the old window even after native save/reopen. Its decoded
Tasks payload differs from the native-edited control only at that rate window. This is a failed
authoring-equivalence check, retained as evidence. Coordinated fields require independent
native comparison before any friendly quantity/duration edit or native Save control is admitted.

## Checkpoint validation

Work is stopped pending explicit continuation. The earlier source freeze passed 1,772
library tests (10 ignored), all-target clippy, application builds, layers, locks, wasm,
tool documentation, secrets, doctests and formatting. The subsequent coordinated
quantity/utilization regression passed, but the latest rate-field changes still require
a fresh full validation and paired-field native Save/reopen controls. Build artifacts
were intentionally cleaned after stopping. This checkpoint does not close the roadmap
items or authorize native Save controls.
