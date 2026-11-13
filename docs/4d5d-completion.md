# 4D/5D completion acceptance

The user explicitly expanded the goal to editing schedules/costs and saving native SP files
that reopen in the source application. Existing viewer/export completion does not finish this goal.

| Gate | Required behavior | Current evidence |
|---|---|---|
| Native persistence | New SP output, source hash precondition, preserved unknown sections, independent native open/save/reopen of intended edits | Scoped scalar controls passed on native 6.5.5.28; general native Save remains unadmitted |
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

The Windows Computer Use bridge was unavailable at the start of the study. The user
explicitly authorized an alternative native automation channel on isolated study copies.
Native acceptance still requires independently observed values and Save/reopen evidence;
static/reader checks cannot close that gate.

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

`formats_project_edit` opens a source-hash-bound snapshot. Mutations and revision-bound
inspection must match the returned `draft_id`, source hash and revision. Read-only `refresh`
requires the draft identity and source hash but returns the current revision, so a client
can recover after another client changes the same draft without weakening mutation guards. Applying a transaction validates all fields
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
authoring-equivalence check, retained as evidence. The coordinated comparisons below close that specific storage mismatch for the observed
fixture. They do not admit an automatic rate policy, other assignment settings, friendly
quantity/duration editing or a general native Save control.

## Checkpoint validation

At the prior checkpoint, work was stopped pending explicit continuation. That source freeze passed 1,772
library tests (10 ignored), all-target clippy, application builds, layers, locks, wasm,
tool documentation, secrets, doctests and formatting. The subsequent coordinated
quantity/utilization regression passed. Build artifacts were intentionally cleaned after
stopping. The subsequent continuation and fresh validation are recorded below. Neither
checkpoint authorizes a general native Save control.

## Coordinated native controls (2026-11-13)

Fresh CLI output was compared with expectations frozen from independent native UI edits
before generation. Both paired candidates retained all 47 non-Tasks stored sections and
matched the independent control's entire decoded Tasks stream, not only displayed totals.
Each candidate was opened, saved to a new native file, unloaded and reopened. All observed
General and assignment property values matched before/after; the native-saved decoded
Tasks and typed task page also matched. Geometry inventories and Entities, IncludeLibrary,
Resources, Calendars and Use Profiles remained unchanged. Focused screenshots after reopen
showed the retained geometry.

| Transaction | Native values after Save/reopen | Independent native editing control |
|---|---|---|
| Planned quantity 5 to 7.5 plus raw utilization 166.66666666666666 to 250 | Units 7.50, rate 2.50 u/d, duration 3 days | Decoded Tasks byte-identical |
| Duration 86400 to 115200 plus raw utilization 166.66666666666666 to 125 | Duration 4 days, units 5.00, rate 1.25 u/d | Decoded Tasks byte-identical |

These are observations for one Planned-status Work task, Fixed duration and one Linear
Material assignment with Fixed Units true and Driving false, on native build 6.5.5.28.
No nominal-day formula, multi-assignment behavior, started/finished status, dependency
rescheduling, other resource kinds or currency conversion is certified by this matrix.
The candidate receipt deliberately retains its storage-only contract and does not mutate
its native-verification flag after an external audit.

A separate quantity-only Material control also retained the native budget 551.25 for
7.5 units at its known PerUnit price 73.50; the current reader returned 55125 minor units
before and after native Save/reopen. That is cost/persistence evidence, not full editing
equivalence: its utilization rate remained 1.67 u/d.

The latest source passed 2,724 tests across 248 unit, integration and doctest harnesses
for formats, analysis and engine, with 19 ignored and no failures. All-target clippy,
layers, locks, secrets, tool documentation, wasm and formatting also passed on the same frozen source. The lean validation cache disables
incremental compilation and debug symbols; no source/compiler optimization policy changed.

## Planning draft interface

The scene viewer exposes a separate experimental project-draft window for SP sources.
It uses the shared draft tool through background jobs, retains typed source/draft/revision
identities, and leaves the original viewport and source-keyed caches unchanged. The window
shows derived preview dates and bounded task pages. Advanced scalar forms expose stored
start/duration values, assignment quantity/utilization pairs and admitted direct Fixed costs.
Changed fields on a form page are submitted together; source units are retained and no
universal utilization conversion or automatic scheduling policy is implied.

Undo, redo and refresh reload the authoritative snapshot. Older or differently bound
responses cannot replace a dirty preview. Within the scene viewer, another source cannot
replace an active draft; hiding the window preserves it. Closing a dirty draft requires
explicit discard. Candidate export requires native-validation acknowledgement, a separate
new output path and the existing no-clobber/source-hash checks. It does not clear dirty
state or enable the general Save command. Worker jobs receive the repaint context before
the initial draft-open operation starts.

Additional independent native controls located Driving at assignment byte 203 and Fixed
Units at byte 205: the isolated toggles changed 0 to 1 and 1 to 0 respectively, with no
other decoded Tasks changes. With Fixed Units false, changing duration from 3 to 4 days
kept raw utilization 166.66666666666666 but changed stored quantity 5 to 6.666666666666666
(native display 6.67). The earlier Fixed Units true duration control instead retained
quantity 5 and changed utilization to 125. These are scoped observations, not a general
editing policy; the current forms deliberately expose independent scalar transactions.

The frozen UI source passed the main application, CLI and check-tool builds; 925 unit/
integration/target tests (15 ignored), all-target clippy, layers, locks, secrets, tool
documentation, doctests and formatting. The widget regression exercises actual UI actions
through the shared engine, recovery after a second client changes revision, candidate
publication and unchanged source bytes. Offscreen Studio/Planning images were inspected;
title contrast and scroll clipping were corrected. This validates the experimental scalar
interface, not general authoring, desktop interaction parity or native Save admission.

## Task text storage candidates

A separate experimental headless tool, `formats_project_sp_task_text_candidate_write`,
changes task names and display IDs. Each transaction binds the source SHA-256, task GUID
and expected text. It accepts at most 256 distinct fields and 16,384 UTF-16 code units
per value, rejects NUL, and supports the observed short and extended length encodings.
Edits use original decoded offsets, so multiple changes can grow or shrink together.
Unchanged values preserve their original encoding; an unchanged Tasks payload preserves
its original compressed bytes. Every other stored section must remain byte-identical.

The candidate is reparsed and requested text is checked again by task identity before
publication. New SP and versioned text-receipt paths use the existing no-clobber publisher.
The scalar receipt contract is unchanged. Receipts retain `native_reopen_verified: false`;
independent native controls and round-trip results belong to validation evidence. This
is not task creation/deletion, hierarchy editing, Display ID uniqueness enforcement,
external-reference rewriting or scheduling, and it does not enable general Save.

The separate [text receipt version-1 schema](schemas/sp-task-text-candidate-receipt-v1.schema.json)
defines original decoded offsets, encoded lengths and expected/output text. It does not
change the scalar receipt schema. Consumers select the schema by the producing tool and
must reject unsupported versions. Version 1 is the first text format; a future incompatible
version must provide an explicit migration or regenerate the receipt from the unchanged,
hash-bound source. Migration must never turn external fixture results into a native-proof
flag on a newly generated candidate.

Four minimal native controls cover Unicode, a 342-code-unit name, an empty second-task
name and a Unicode display ID. Candidate Tasks matched the independent native-edited
controls exactly, and remained identical after native Save/unload/reopen. UI General
values and latest-reader task pages also matched. These are metadata-only controls with
no drawable meshes; the geometry opener correctly refuses them.

A separate assigned-task control changed both name and ID on a drawable project. Its
candidate matched independent native UI editing in decoded Tasks, survived native Save/
reopen, and retained equal task pages, geometry inventories and protected geometry/
resource/calendar/profile payloads. The inspected focused native image shows both mesh
instances. This covers one Planned Material singleton, not general project authoring.

Frozen-source validation passed CLI/check-tool builds, 2,476 formats/engine target tests
(19 ignored), all-target clippy, layers, locks, secrets, tools, wasm, doctests and formatting.
All nine generated text receipts passed schema validation. Original/native source hashes
were retained; private fixtures and native automation are not part of the public sources.
