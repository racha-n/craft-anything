# Task deletion controls

Status: five native controls validated; the standalone deletion candidate below
passes its own CLI/native acceptance. Draft deletion and its UI remain pending.
The observed build is 6.5.5.28 only.

Deletion is more than removing a serialized task. The control set covers an
unassigned root leaf, a child leaf with a surviving sibling, an assigned root with
geometry, and each direction of a dependency. All controls use separate copies,
verify the selected task GUID before deletion, and save, unload and reopen the
native output. Original input hashes remain separate from native save hashes.

## Observed storage changes

For all five controls, native saving changes Tasks, PresentationData,
ProjectStateIndex, LastTransactionID and Thumbnail. No other stored section
changes. Removing a complete leaf record, reducing its containing collection count
and rebuilding raw-GUID-sorted lookup entries reproduces the native Tasks payload
in the root, child, assigned and linked-successor cases. Remaining ordinary task
bodies are exact. A summary record includes its children, so its complete record
changes when a child disappears; its own stored dates remain unchanged in the
observed child control. This does not prove general summary recalculation.

PresentationData contains both current selection and GUID-keyed row state. A
diagnostic projection validates the observed sorted row map, removes the target
entry and clears a matching single-task focus/selection. Its complete decoded
payload matches the root and child native outputs. In the assigned control,
clearing only task selection is insufficient: the native output also clears the
removed assignment's selected identity. Including that independently observed
focus/selection packet reproduces the complete presentation payload. The
assignment identity is separate from its appearance-profile identity.

The geometry-bearing native output retains the resource, two mesh instances and
one geometry definition after its assigned task disappears. The assignment is
gone. Declared frame evaluation consequently reports no assignments for that
resource; it must not reuse the former installation phase. A reopened native
image also shows the retained geometry. No general appearance, schedule or cost
parity follows from this control.

The linked input stores multiple selected tasks. Selecting one deletion target
changes selection before deletion, so the full presentation delta cannot be
attributed solely to deletion. That case does not admit multi-selection cleanup.
Deleting the predecessor removes the incoming link from the surviving successor;
its record shrinks by 80 bytes in this control. Simple leaf removal/count/lookup
rewriting therefore fails to reproduce that Tasks payload, as expected. Removing
a linked successor alone does not establish incoming-reference cleanup.

## Verification and limits

| Control | Task count before/after | Simple Tasks projection | Presentation projection |
| --- | --- | --- | --- |
| Unassigned root leaf | 4 / 3 | Exact | Exact |
| Child leaf, sibling retained | 4 / 3 | Exact | Exact |
| Assigned geometry root | 2 / 1 | Exact | Exact after assignment-selection cleanup |
| Linked successor | 2 / 1 | Exact | Multi-selection unadmitted |
| Linked predecessor | 2 / 1 | Requires surviving-link cleanup | Multi-selection unadmitted |

All five outputs pass native save/unload/reopen, valid lookup inspection and
remaining GUID inventory. Target GUID/name/display ID are verified before each
deletion; 205 General values were captured across those five target selections.
These are not 205 before/after comparisons of remaining task properties. Five
reopened native images were inspected. All ten source/work-copy hashes and the
57 protected source hashes remain unchanged. Existing production binaries are
reused; this study changes no production code and adds no runtime behavior.

## Candidate boundary

The first writer is restricted to an unassigned Planned root leaf in a
flat, admitted default-Work project with at least two tasks. Reject assignments,
links, summary/child deletion, last-task deletion, unsupported presentation maps,
multi-selection and unknown references. A source-bound request must validate the
task GUID, expected parent, sibling index and display ID before any publication.

Validate all collection lookups and known identity references before writing.
Preserve surviving task bodies and row-state bytes. Rewrite only Tasks and the
validated PresentationData structures; preserve the other 46 stored sections and
header padding. Keep opaque transaction/thumbnail history separate from native
semantic acceptance. Export new SP and versioned receipt files with individually
atomic, no-clobber publication and a final source-hash check. They are separate
publications: if the receipt fails after the SP is published, retain the candidate
and report recovery instructions. This is not a two-file transaction.

The standalone `formats_project_sp_task_delete_candidate_write` tool implements
this boundary. It accepts explicit source/hash/output/acknowledgement fields and a
`deletion` containing task GUID, expected null parent, sibling index and display ID.
Its [version-1 receipt schema](schemas/sp-task-delete-candidate-receipt-v1.schema.json)
records retained bodies, lookup and presentation checks, selection clearing and a
48-section reference scan. Raw GUID and canonical ASCII/UTF-16 UUID spellings are
checked; native transaction, geometry and scheduling certification remain false.
Bounds include 511 source tasks, 16 MiB per decoded section, 64 MiB scanned decoded
payload and a 256 MiB file. A final source-byte recheck precedes publication.

Chronological draft deletion, undo/redo focus and UI forms follow as a separate
change. Broader assignment/link, summary/last-child/last-task and multiple
selection rules need their own evidence and regression coverage.

Existing creation candidates preserve presentation state, including a row map
that can lack the newly created task until native saving materializes it. A draft
must not infer deletion availability for that composition from standalone deletion
acceptance. Keep it unavailable until row-state admission is validated separately.

## Standalone candidate acceptance

All twelve frozen checks pass across formats/engine/testkit: 2,553 target
tests (19 ignored), six format and one engine focused tests, strict
all-target clippy, layers, locks, staged secrets, tools, wasm, doctests and formatting.
The CLI and xtask are rebuilt; desktop and viewer binaries are reused.

The current CLI reproduces all five earlier explicit v2 creation SP files and
receipt values exactly after the shared repack change. Two deletion requests on
one four-root native source validate the selected-task and retained-selection
branches. Deleting its first row retains the non-involutive lookup vector
`[1, 2, 0]`. The selected-task candidate's decoded Tasks and PresentationData match
the independent native deletion control exactly; both preserve the other 46
stored sections. Thirteen refusal calls publish nothing new and preserve existing outputs.

Both actual CLI deletion files pass native Save As/unload/reopen and 492 complete
General comparisons for the three surviving tasks, before and after native saving.
Task payloads remain exact through that round trip; both candidate hashes and all
57 protected source hashes stay unchanged. Two final native images were inspected.
This admits the bounded storage candidate, not generic deletion, native Save,
scheduling, draft integration or a deletion UI.
