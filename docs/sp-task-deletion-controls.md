# Task deletion controls

Status: five native controls validated. No production deletion codec, draft action or
UI command is admitted by this document. The observed build is 6.5.5.28 only.

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

The first proposed writer is restricted to an unassigned Planned root leaf in a
flat, admitted default-Work project with at least two tasks. Reject assignments,
links, summary/child deletion, last-task deletion, unsupported presentation maps,
multi-selection and unknown references. A source-bound request must validate the
task GUID, expected parent, sibling index and display ID before any publication.

Validate all collection lookups and known identity references before writing.
Preserve surviving task bodies and row-state bytes. Rewrite only Tasks and the
validated PresentationData structures; preserve the other 46 stored sections and
header padding. Keep opaque transaction/thumbnail history separate from native
semantic acceptance. Export a new file and a versioned receipt atomically, with
no-clobber and a final source-hash check.

This proposed boundary is not implemented. Actual CLI-generated output must pass
independent native save/unload/reopen checks and surviving task properties before
admission. Chronological draft deletion, undo/redo focus and UI forms follow as a
separate change. Broader assignment/link, summary/last-child/last-task and multiple
selection rules need their own evidence and regression coverage.
