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
| 4D Review | Camera animation, video export, saved viewpoints |
| Project Controls | Cost editing, progress entry, time-series cost charts |
| Reports | Schedule/cost reports and report templates |
| Windows / Navigator | Docking, named workspaces, saved searches and orientation cube |

These are visible backlog items, not promises that the reader/writer already implements them.
