# Planning workspace interface

R3.14 follow-up, explicitly requested by the user on 2026-11-12.
Owner: Codex, branch `feature/r3.14-scene-bundle-viewport`.

The default project viewer uses a familiar desktop planning layout: a blue ribbon,
compact command groups, a resource navigator on the left, task rows and a synchronized
Gantt above the 3D view, and a properties inspector on the right. The previous compact
viewer remains available. This is original Rust presentation code over the existing
engine tools, not a copy of another application's code, icons or branding.

## Acceptance

- Open the same admitted SP project directly and retain the 3D/4D/5D results.
- Show source task IDs, names and resolved planned/best dates in aligned table/Gantt rows.
- Select a task to inspect it; isolate its assigned geometry only on explicit action.
- Keep resource selection, fit, hide/isolate, surface picking and camera controls working.
- Make ribbon, navigator, schedule and inspector responsive and resizable where applicable.
- Show unimplemented commands in grey, disabled, with a tooltip describing the future work;
  retain read-only and interpretation limits. This visibility was explicitly requested by the user.
- Check the real-project GPU render, interaction regressions and embedded desktop view.

Layout colours and radii are theme tokens. UI state is serde data. Parsing, schedule and
cost interpretation remain in the existing engine/analysis modules. Task bars are drawn
from resolved dates, not inferred from labels; unknown dates remain visible as unknown.

## Visible future work

The typed `FUTURE_COMMANDS` catalogue in `scene_viewer/planning.rs` is the UI inventory.
Every entry is disabled and has its own explanation; an empty selection merely disables
otherwise implemented commands and is distinguished from a missing capability.

| Area | Future capabilities shown in grey |
|---|---|
| File | New project, Save, Save as, import schedules, export project |
| Plan | New task, hierarchy editing, links, rescheduling, critical path, calendar editing, baselines |
| Resources | New resources, assignment editing, resource wizard, appearance-profile editing |
| 3D | Import/export models, edit transforms and materials |
| Project Controls | Cost editing, progress entry, time-series cost charts |
| Reports | Formatted page/PDF layouts (CSV table/date templates are available) |
| Windows / Navigator | Docking, named workspaces and saved searches |

These are visible backlog items, not promises that the reader/writer already implements them.

## Camera navigation and saved views

The orientation cube in the 3D view uses stored world axes (+X/-X, +Y/-Y,
+Z/-Z). Click a visible face for its exact axis view or Isometric to return to
an oblique view. It preserves the camera target and distance. The manual Y-up
setting changes camera interpretation; it does not verify the source axes.
The View cube checkbox toggles the overlay.

Saved viewpoints opens the camera sidecar menu. Enter a name and Save current
view, then click a saved name to restore it. Reusing a name replaces its pose;
Delete removes that view. Poses include target, yaw, pitch, distance and up-axis
choice. Visibility, task focus, selections and annotations are not saved.

`formats_scene_viewpoints_manage` provides the same operations without the UI.
The version-1 sidecar is stored below the isolated/session configuration directory
in `scene-viewpoints/<scene-sha256>.json`, separately from the source project.
Its schema is `docs/schemas/scene-viewpoints-v1.json`. The engine checks scene
identity, finite camera values, file size (128 KiB), names (160 bytes) and count
(64). Unknown versions and invalid files are rejected without overwrite.
There is no older version to migrate; any future format must explicitly migrate
version 1 and preserve the prior sidecar before updating it.

## Task and Gantt navigation

Use the +/- outline controls or Collapse all / Expand all. Search matches names,
native display IDs and GUIDs and reveals matches even within collapsed branches.
Time zoom changes only the visible Gantt interval. Earlier / Later pans the
interval; Fit timeline restores the full source date range. These operations
never edit task dates or relationships.

The ID / GUID column uses a short source GUID when the native display ID is
absent; the inspector retains the full GUID and explicitly marks Native ID as
unresolved. This fallback is not an inferred native display ID.

Task selection highlights the first directly assigned resource when that resource
has scene geometry. Selection links lists all direct resource joins, with a
64-link display limit. Resource and object inspection links back to directly
assigned tasks; choosing a linked task reveals it in the schedule. Fit linked
selection moves the camera. Isolation remains an explicit separate action.
`formats_project_selection_links` exposes the direct source joins headlessly.
Resource ancestry and native display IDs are never invented.

## Presentation and report follow-up

Camera animation, Export video, Schedule report and Cost report are active for a loaded
project. Source audit is available on 3D/Navigator. See [the workflow and format limits](scene-presentation.md).

MP4 video on 4D Review opens the shared presentation panel. Check MP4 encoder verifies
a local FFmpeg/libx264 build; Export MP4 writes a new compressed video with a version-2
manifest and decoded-frame checks. AVI remains available. See [video details](scene-presentation.md).

Schedule only has its own Export metadata CSV action. It exports the displayed explicit
source/focus snapshot without loading drawable geometry or using a retained 3D scene.
Changed source hashes require a reload; outputs are new source-bound v2 report folders.

Report templates is active for loaded scenes and the same CSV chooser appears in Schedule
only. Built-in table selections, readable date columns and saved template JSON reuse are
implemented. Formatted page/PDF layouts remain grey. See [template details](report-templates.md).
