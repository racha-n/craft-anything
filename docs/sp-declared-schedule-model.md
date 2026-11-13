# Declared task scheduling preview

`formats_project_schedule_preview` calculates read-only earliest dates for an
explicitly declared unconstrained, fixed-work model. It requires a source SHA-256,
a minute-aligned naive wall-clock anchor and `declare_unconstrained_fixed_work`.
Source constraints, duration types and native scheduling policies are not admitted
by that declaration. The preview does not change the project.

Omitting `relationship_model`, or choosing `fs_only`, preserves the version 1
FS/nonnegative-lag model and its result layout. Choose
`four_relationships_signed_lag` explicitly for the version 2 model:

```json
{
  "path": "study.sp",
  "expected_source_sha256": "replace-with-the-current-source-sha256",
  "anchor_wallclock": 1794128400,
  "declare_unconstrained_fixed_work": true,
  "relationship_model": "four_relationships_signed_lag",
  "offset": 0,
  "limit": 100
}
```

The version 2 result also identifies `relationship_model`. Both models return
`native_parity_verified: false` and `critical_path_calculated: false`. Output task
order follows source order, independently of the topological calculation order.
Pages contain at most 250 tasks, while the whole admitted graph is calculated
before a page is returned. Cycles and unresolved calendars fail the whole preview.

| Observed storage type | Relationship | Predecessor boundary | Successor boundary |
| --- | --- | --- | --- |
| 1 | FS | Finish | Start |
| 2 | SF | Start | Finish |
| 3 | SS | Start | Start |
| 4 | FF | Finish | Finish |

Signed lag advances or subtracts working time in the predecessor's effective
calendar. A successor finish bound is converted to a start bound by subtracting
its duration in its own effective calendar. This backward calculation skips
non-working gaps before the bound. For the observed Monday-Friday 09:00-17:00
calendar, Tuesday 17:00 and Wednesday 09:00 are adjacent working boundaries.
An active SF bound can therefore give a Tuesday finish before the predecessor's
Wednesday start. This is working-time normalization, not wall-clock equality.
The latest incoming start bound and the explicit anchor determine each task's
earliest start; its own calendar then allocates its positive duration forward.

Admission remains bounded to 512 Planned flat leaf tasks, 1024 links, 64 calendars
and 4096 aggregate day types/exceptions. Durations are positive and at most one
million working seconds; signed lag magnitude has the same cap. Both are
minute-aligned. Each forward or reverse allocation has limits of 4000 scanned
days and 8192 intervals, sharing a 200000-operation cancellation budget across
the preview. Unknown relationship types, invalid dates, sub-minute result
boundaries and unresolved inherited calendar policies are refused.

The [native relationship controls](sp-relationship-scheduling-controls.md) establish
specific same-calendar date observations, including unequal-duration propagation
and an active three-task SF chain. Mixed calendars, exceptions, positive and
negative lags beyond those controls remain declared arithmetic, not general
native parity. Source constraints, calendar overrides, progress, resource
coordination, critical paths, baselines and native schedule writing remain open.

## Validation

All thirteen frozen checks pass: formats/analysis/engine all-target tests
(2,831 passed, 19 ignored), strict clippy, layers, locks, secrets, wasm,
tools, doctests and formatting; CLI and desktop were rebuilt. Focused suites pass
8 calendar, 9 scheduling and 2 tool-contract tests. The actual CLI/MCP matches
all 51 start/finish/duration values in eight previously verified native controls
(17 tasks), retains two exact default version-1 results and their explicit
`fs_only` equivalents, and passes 12 refusals across 35 MCP calls. All 57 protected
inputs stay unchanged. The 845-file source/schema freeze includes four changed
existing source files and one new module; 840 baseline hashes remain unchanged.
This comparison reuses the independently collected native observations and adds
no native round trips or native-parity certification.
