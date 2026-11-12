# CSV date labels and reusable templates

Both report export tools accept either `template` (an inline object) or `template_path`
(an absolute path to a saved JSON file). Omit both for unchanged legacy report v1 from a
loaded scene or v2 from an explicit metadata path. The new UI defaults to All tables with
dates; Legacy raw CSV remains selectable. `formats_project_report_templates` lists presets
without needing an open project.

The presets are All tables with dates, Schedule review (tasks and phases), and Cost review
(resources, costs and actual expenses). Report templates on the Reports ribbon opens the
same chooser. Metadata-only reports have this chooser inside Schedule only. Saved template
JSON enables reuse of an exported `template.json`; loading a template does not change it.
Selections are disabled while their export runs. The raw snapshot always contains the full
report dataset, including tables whose CSV files were not selected.

```json
{
  "version": 1,
  "name": "Schedule review",
  "tables": ["tasks", "phases"],
  "readable_dates": true
}
```

Choose one to five distinct table ids: tasks, resources, costs, actual_expenses, phases.
Names must be visible, nonblank and at most 120 UTF-8 bytes; files are capped at 16 KiB.
Unknown versions, unknown fields, duplicate tables, invalid ids and ambiguous inline/path
requests are refused before publication. There is no template script, HTML or expression
execution. Template version 1 has no prior migration; future readers must explicitly admit
a new version while preserving existing files.

With readable_dates enabled, tasks gain planned_start_display, planned_finish_display,
best_start_display and best_finish_display. Expenses gain date_wallclock_display. Resources
gain rate_effective_dates_display, a JSON array aligned with the original rates array.
Validated model date values are formatted as YYYY-MM-DD HH:MM:SS for Gregorian years
1600..9999. These are displayed-clock labels, without timezone conversion or a UTC suffix.
Null dates remain empty; non-null out-of-range dates display Unresolved. Durations, raw
storage windows and original scalar columns remain unchanged. Empty tables keep the
legacy no_rows header. The focus label is recorded as context even when CSV date labels
are disabled.

Templated reports use manifest/snapshot version 3 and include template.json plus its hash.
The manifest records the exact template, emitted column names, date policy, focus label,
source hash and source_report_version (1 for loaded-scene input, 2 for metadata-only input).
Scene ID/geometry scope continue to distinguish loaded_scene from not_inspected. The raw
model and frame inside snapshot.json are not rewritten with display strings. Each selected
CSV retains its own row count and hash. Existing size caps, formula guards, separate monetary
subtotals, cancellation, new-output-only checks and manifest-last completion remain.

Schemas: report-template-v1.json, project-report-manifest-v3.json and
project-report-snapshot-v3.json under docs/schemas. Existing v1/v2 outputs are immutable;
version 3 is a separate new export, with no in-place migration. CSV column selection/order,
page layouts and PDF reports remain future work and formatted layouts remain disabled.
