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

## Mixed draft transactions and receipt version 2

The shared draft accepts scalar and task-text changes in one atomic Apply request. Task
labels and the current scalar form page are submitted together. Expected values bind to
the current revision; retained edits keep their original source expectations for export.
Undo, redo, branching and read-only Refresh cover both kinds in the same history. A stale
text field rejects the entire transaction, including otherwise valid scalar changes.

History retains at most 64 transactions and 256 changed fields, with a conservative
16 MiB retained edit-payload budget. Immutable version snapshots are shared when a session
is cloned. The source file and viewport caches stay unchanged. Returning all fields to
their original values clears dirty state while preserving Undo. Task selectors show a
bounded 192-character preview; label editors keep the full source text. Dirty close and
explicit discard controls remain near the revision toolbar, above the expanding forms.

Scalar-only exports retain the version-1 scalar tool and receipt. When text edits are
present, `formats_project_sp_mixed_candidate_write` applies the scalar stage first and
then the text stage, producing a [mixed receipt version 2](schemas/sp-mixed-candidate-receipt-v2.schema.json).
Both original version-1 stage receipts remain embedded. Consumers must verify original
source to scalar output, scalar output to text source, and text output to final output
hashes, as well as the combined 256-field limit. Scalar offsets refer to the original
source; text offsets refer to the intermediate scalar output. Existing v1 receipts are
unchanged. A v1 artifact cannot become a mixed v2 receipt by changing its version number;
regenerate from the hash-bound original source and explicitly supplied edits. Unsupported
versions must be rejected. Native and derived-consistency flags remain false.

The real widget regression types Unicode task labels, applies duration/name/ID together,
undoes/redoes, recovers after a second client changes revision, exports a v2 candidate and
explicitly discards the draft without changing the source. Studio/Planning offscreen
images were inspected with the same pinned font input used by CI; builds without that
optional input cannot demonstrate Thai/CJK glyph coverage. See [font packaging](releasing.md).

An independent native control combined a four-day duration, coordinated raw utilization
125, renamed task and changed ID on the assigned geometry-bearing singleton. The mixed
candidate matched its decoded Tasks exactly and survived native Save/unload/reopen.
Observed General/assignment values, typed task pages, geometry inventories and protected
geometry/resource/calendar/profile payloads matched; both mesh instances remained visible
in the inspected focused native screenshot. This remains one controlled storage case,
not automatic quantity/rate coordination or general native authoring.

Frozen validation passed all 11 checks: main/CLI/check-tool builds, five draft-panel widget
and form regressions, 2,717 formats/engine/UI target tests (25 ignored), all-target clippy,
layers, locks, secrets, tools, wasm, doctests and formatting. The widget and controlled
native candidates' v2 receipts passed schema and hash-chain validation.

## Relationship and scheduling controls remain separate

Independent native type edits saved/reopened as FS=1, SF=2, SS=3 and FF=4; each isolated
change affected only the type window. On the observed Project Calendar, typed lag 2d
stored signed 57,600 and -1d stored -28,800, with only the eight-byte lag window changing.
These controls did not invoke Reschedule and do not prove a scheduling algorithm.

The observed native Reschedule options use Manual Data Date, ASAP, minute snapping,
upstream-task calendars for links and retained logic for progressed tasks. A default
Thai-era date display of 8/11/2569 produced raw task dates in Gregorian year 2569 after
Reschedule/save/reopen. Its cause is unresolved; separate explicit-date controls are recorded below.
Do not infer the scheduler anchor from localized display text or silently subtract 543
years. General rescheduling, critical paths, calendar editing and baselines remain open.

Separate controls explicitly entered Gregorian 8/11/2026 9:00 as the Manual Data Date.
With the recorded ASAP/upstream-calendar/minute-snap options, both one-day Planned tasks
survived native Reschedule/save/reopen: FS/lag 0 placed the predecessor at November 9
09:00 and successor at November 10 09:00; FS/lag 2d placed the successor at November 12
09:00. The zero-lag native Tasks payload exactly matched a source-bound candidate that
changed only the successor's stored planned start. This validates the explicit-anchor
fixture subset, not the cause of the localized default mismatch or other scheduling
policies, task kinds, constraints, hierarchies or critical paths.

## Declared FS earliest-date preview

`formats_project_schedule_preview` is read-only. It requires source SHA-256, an explicit
nonnegative minute-aligned naive wall-clock anchor, and a declaration that the tasks are
unconstrained fixed work. It does not infer an anchor from localized source-application
text. Source task kinds and constraints remain uncertified; this declaration is a model
assumption, not a native-admission flag.

The preview accepts up to 512 Planned leaf tasks, 1,024 FS links, 64 calendars and 4,096
calendar day types/exceptions. Durations are positive and lags nonnegative; both are
minute-aligned and capped at one million working seconds. Lag uses the predecessor's
effective calendar and task duration uses the task's calendar. Shared work-scan budgets,
cancellation, source identity, missing references and cycle checks bound computation.
Unsupported kinds, negative lag, hierarchies, progressed tasks or a failed calculation
return an error instead of a partial schedule. Results are paged, preserve input order,
and retain `native_parity_verified: false` and `critical_path_calculated: false`.

Six focused regressions cover working boundaries, positive lag, multiple predecessors,
calendar exceptions, upstream versus downstream calendars, input refusal and shared
budget exhaustion. Latest CLI results matched every native task start, finish and duration
in the two explicit-anchor FS/lag 0 and FS/lag 2d fixtures without changing either input.
These metadata-only controls do not establish general native rescheduling, critical paths,
baseline management, utilization coordination or a native write-back contract.

Frozen validation passed all 11 checks: six focused regressions, main/CLI/check-tool
builds, 949 analysis/engine target tests (9 ignored), all-target clippy, layers, locks,
secrets, tools, wasm, doctests and formatting. This adds a declared read-only model;
the general Plan rescheduling command and native authoring acceptance remain separate.

## Existing-task sibling order candidates

`formats_project_sp_task_order_candidate_write` moves one existing task subtree within
its current sibling collection. Source SHA-256, task GUID, expected parent GUID (null
for roots) and expected sibling position are mandatory guards. The destination index
is measured after removing the moving subtree. No-op candidates retain original file
bytes. A candidate preserves task bodies and descendants, validates each collection's
GUID lookup vector, rebuilds the affected sibling lookup vector, reparses parent/order
identity, and preserves all 47 other section payloads. The version-2 receipt has a
separate schema and keeps native-reopen
and derived-schedule flags false. Publication uses new candidate/receipt paths only.

The current subset contains at most 512 Planned tasks without assignments, links,
costs, progress records, activity codes or user values. It does not change parents,
create/delete tasks, recalculate schedules or enable the
general Plan movement commands. These capabilities remain separate pending changes.

An independent two-root native Up control saved/reopened in reversed order. Its whole
Tasks payload exactly matched swapping complete task records while preserving their
trailing words (1 then 0 after the swap). Those words are therefore not current display
positions. A separate mismatched-date indent/outdent pair kept parent stored dates
unchanged even though summary dates in the UI followed the child; physical Outdent
restored the original Tasks payload exactly. Reparenting changed the child's trailing
word, so no general hierarchy writer is admitted from these controls.

Latest CLI up/restore/no-op cases matched the independent native Tasks controls,
validated against the version-1 receipt schema and refused output replacement.
A three-root Up control matched another CLI candidate while preserving all other
sections. The two-root candidate passed native Save/unload/reopen: its decoded Tasks
remained exact, and all ten visible ID/name/duration/start/finish grid cells matched
the independent control. General properties were not rechecked in that round trip;
the native screenshot was inspected. This certifies those observed cases only, and
the generic receipt's native-acceptance and recalculation flags remain false.

The original whole-record preservation rule was incomplete: serialized trailing words
collectively form a GUID-sorted lookup vector into the physical sibling records. They
are not per-task fields. Adjacent swaps from the identity vector did not expose this
error; a three-task cycle does. Version-1 order receipts and those earlier native
controls do not establish general order compatibility. Current candidates require
valid source lookup vectors and rebuild their indices using stored GUID-byte order.
Task bodies and descendants remain exact; the legacy subtree-preservation field now
reports the actual result, while explicit content/index flags describe the new contract.
Regenerate historical version-1 order candidates from a verified original source;
inconsistent source lookup vectors are refused rather than silently repaired.

The corrected CLI's three-task cycle exactly matched independent native Tasks and
preserved all 47 other stored sections. Two sequential moves produced the same file
as the single cycle; version-2 receipts and the version-3 envelope passed schemas and
hash-chain checks. The latest CLI candidate passed native Save/unload/reopen with
GUID/name/ID/type/status/start/finish/duration equal for all three tasks, and its Tasks
remained exact. Candidate/receipt bytes and 57 protected sources stayed unchanged.
All 11 frozen checks passed: seven focused formats regressions, nine widget tests,
main/CLI builds, 2,735 target tests (25 ignored), clippy, layers, locks, secrets,
tools, wasm, doctests and formatting. Both draft layouts were inspected. This remains
an experimental same-parent operation; general reparenting and native Save are separate.

Frozen validation passed all 11 checks: four focused regressions, main/CLI/check-tool
builds, 2,485 formats/engine target tests (19 ignored), all-target clippy, layers,
locks, secrets, tools, wasm, doctests and formatting. Nested synthetic cases preserve
complete descendant subtrees; the original opaque-tail preservation claim above is
superseded by the lookup correction. Stale identity/position, cancellation,
linked tasks and valid sources exceeding the 512-task budget are refused.

## Existing-task hierarchy candidates

Hierarchy authoring has a separate experimental tool,
`formats_project_sp_task_hierarchy_candidate_write`.
It moves an existing subtree to a different parent (or the root collection), with
source hash, GUID, expected parent and expected sibling position guards. The destination
position is measured after removing the subtree. Same-parent requests use the existing
order tool. Self/descendant destinations, excessive depth, invalid lookup vectors and
noncanonical empty child frames are refused. The subset remains at most 512 Planned
tasks without assignments, links, costs, progress, codes or user values.

Task GUIDs and body fields stay exact; child flags/frames/counts and collection lookup
vectors are rebuilt within the 16 MiB Tasks cap and 64-level depth limit. Reparse checks
every parent/order, all GUIDs and each task's body segments; other section payloads stay
unchanged. The version-1 hierarchy receipt has its own schema. Stored dates/durations
stay unchanged, and native persistence, summary dates and schedule coordination remain
separate gates. This tool is not yet integrated into shared draft history or Plan forms.

CLI indent, outdent and inverse candidates matched independent native Tasks, preserved
all 47 other sections, passed their receipt schema and refused replacement. Both native
indent/outdent candidates passed Save/unload/reopen with all three tasks' complete
observed General dictionaries equal to the independent controls before and after,
including derived parent summary dates. Task bytes, candidate/receipt hashes and 57
protected source hashes stayed unchanged; both native screenshots were inspected.

Main/CLI builds and 2,502 formats/engine target tests passed (19 ignored). A test-only
clone-to-borrow lint fix left production source unchanged; all eight current hierarchy
regressions and all-target clippy then passed, followed by layers, locks, secrets, tools,
wasm, doctests and formatting. The initial lint failure and carried-forward build/full
test evidence are retained. These native controls establish their observed cases;
generic native-save and derived-schedule flags remain false.

## Shared field and task-order drafts

The ordered draft pipeline extends existing scalar/text history with at most eight
sequential same-parent order operations and 256 fields/operations combined. Each move
guards the current task GUID, parent and sibling position. Field expectations remain
bound to the original source; moves run after the field stages and bind to the prior
intermediate hash. A version-3 receipt retains the original version-2 mixed receipt
and every order receipt. New order stages use version 2; the version-3 envelope schema
also accepts retained historical version-1 stages. Consumers must check the complete hash chain.
The standalone headless tool is `formats_project_sp_ordered_candidate_write`;
draft export selects it when order operations remain.
Existing scalar-only and text-containing exports retain their version-1/version-2
contracts whenever the draft has no remaining order operations.

Shared history remains bounded to 64 transactions and 16 MiB of retained edit payload.
Apply validates the current revision and the complete merged state before publishing
history. Undo/redo and branching include field and order changes together. No-op moves
do not consume the operation budget; returning every task to its source order clears
the order log while preserving undo history. Candidate export rereads and hash-checks
the original source and publishes new files without replacement.

The draft form shows one-based positions among siblings. Earlier/Later adjusts a
pending position; Apply combines it with changed labels and the current scalar page.
Shared typed admission disables ordering for unsupported sources while preserving
their field forms. Sibling positions cover the full source, independently of the
100-task display page. Apply/undo/redo can focus a GUID and return its new page; the
form retains selected-task identity across reordering. The original viewport and
source-keyed caches stay isolated. General hierarchy movement and native Save remain
separate acceptance gates; generic native and derived-schedule flags remain false.

Frozen validation passed all 11 checks, including main/CLI/check-tool builds, seven
form/widget regressions, 2,731 formats/engine/UI target tests (25 ignored), all-target
clippy, layers, locks, secrets, tools, wasm, doctests and formatting. Focused cases
cover atomic rollback, inverse-order normalization, the eight-operation budget,
cancellation, branching and GUID focus across 130 tasks. Both layouts' Unicode
offscreen images were inspected; their source freeze equals the full validation
freeze. Version-2/version-3 widget receipts passed their schemas and hash-chain checks.

The latest CLI candidate combined duration, Unicode name, display ID and one sibling
move. Tasks and typed task pages exactly matched an independent native UI control;
all 47 other stored sections stayed equal to the source. Native Save/unload/reopen
preserved Tasks, the modified task's complete observed General values and the unchanged
task's baseline values. The native screenshot was inspected, and candidate/receipt bytes
plus 57 protected source hashes stayed unchanged. This is one Planned unassigned
two-root case; general native saving and structural authoring remain unadmitted.

The metadata-only schedule/cost workflow can open the same draft after its source
frame is ready. The request carries that panel's explicit path and SHA-256; a retained
3D scene and its background work cannot supply or block the source identity. Opening
the draft pauses metadata playback and requests a repaint even when no viewport jobs
have installed a repaint context. An active draft is brought forward for the same
source; a different path/hash preserves it and its unapplied forms until explicitly
closed. Completed closed panels can be replaced. No source file or retained scene/cache
is changed by this entry point.

## Sequential field, order and hierarchy drafts

The experimental structured pipeline applies scalar/text edits first and then up to
eight existing-task relocations, sharing the 256-field/operation limit. Same-parent
requests use the guarded order writer; different-parent requests use the subtree
writer. Each request guards the preceding output's parent and sibling position. The
version-4 receipt retains the mixed receipt and typed order/hierarchy stage receipts
with their complete hash chain. The headless tool is
`formats_project_sp_structured_candidate_write`.

Shared history records the whole sequence. Returning to the source graph compares
both GUID order and parent GUIDs: a parent change remains dirty even when preorder
stays identical. Restoring the graph clears structural operations while retaining
field changes and undo history. Exports retain versions 1/2/3 when no parent-changing
operations remain; version 4 is selected for a retained hierarchy sequence.

The draft form offers guarded Indent and Outdent actions. Indent appends under the
preceding direct sibling; Outdent inserts after the current parent in its parent's
collection. Hints use the complete inventory across display pages and respect the
64-level depth limit. Parent changes can apply atomically with labels and the current
scalar page. Summary preview dates aggregate children; advanced fields explicitly
show stored values, which can differ from summary dates. The original viewport and
source caches remain isolated. Task creation/deletion, general native saving and
scheduler parity remain separate acceptance gates.

Focused validation passed 59 format regressions, five engine draft cases and ten
widget tests, plus strict all-target clippy. Actual widgets exercised mixed labels,
duration, indent/outdent, undo/redo and version-4 export. Both layouts' complete
footers were inspected after scrolling. Three version-4 CLI/widget receipts and
the legacy version-2/version-3 widget receipts passed schemas and complete hash chains.

Two latest-CLI candidates combined duration, Unicode labels and either indent alone
or an interleaved indent/order/outdent sequence. Decoded Tasks matched independent
native controls exactly; all 47 other sections stayed unchanged and replacement was
refused. Both candidates passed native Save/unload/reopen, preserving Tasks and all
three complete General dictionaries before/after (480 property comparisons). Summary
and leaf preview bounds matched the controls, including child-derived two-day summary
dates despite a different stored parent start. Both native screenshots were inspected.
All 11 frozen checks passed: main/CLI builds, ten widget cases, 2,754 target tests
(25 ignored), strict clippy, layers, locks, secrets, generated tools, wasm, doctests
and formatting. All 57 protected source hashes remained unchanged. These proofs
cover the declared small Planned/unassociated subset; they do not admit general
native saving, task creation/deletion or scheduler parity.
