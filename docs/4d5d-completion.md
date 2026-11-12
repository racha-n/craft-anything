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
