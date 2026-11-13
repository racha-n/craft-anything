# Relationship scheduling controls

Observed native build: 6.5.5.28. Production code is unchanged by this study.
The runtime preview still admits only its declared FS/nonnegative-lag model.
General native rescheduling, constraints, calendar overrides and critical paths
remain separate acceptance gates.

Each control enters and reads back Gregorian `8/11/2026 9:00` as the Manual Data
Date. Its Manual/ASAP/Data Date, minute-snap and upstream-link-calendar options
match the earlier independent FS reference. The explicit, non-inherited Project
Calendar has no exceptions and works Monday-Friday 09:00-17:00. Displayed-clock
scalars are not converted to UTC or inferred from localized year labels.

Four single-day, two-task controls cover SS, FF, SF and FS lag -1d. Their input
dates already satisfy the resulting schedule. The complete decoded Tasks payload
remains identical through Reschedule/save/unload/reopen. These establish stability,
not propagation; the SF bound is inactive because the anchor/workday floor dominates.

Three additional controls give the predecessor two working days and place the
one-day successor on November 20. Native input readback verifies those deliberately
different values before invoking Reschedule. The predecessor schedules November
9-10 in all three cases:

| Relationship | Successor after native Reschedule | Observed bound |
| --- | --- | --- |
| SS, lag 0 | November 9, 09:00-17:00 | Same start as predecessor |
| FF, lag 0 | November 10, 09:00-17:00 | Same finish as predecessor |
| FS, lag -1d | November 10, 09:00-17:00 | One working day before predecessor finish |

An active SF control uses three tasks. Native UI creates a chain, verifies its
first FS edge and changes its second edge to SF. The saved/reopened input is
independently parsed: exactly three original task GUIDs and exactly the intended
FS-to-SF edges remain, with zero lag and valid calendar references. Its deliberately
later root and successor starts are read back before calculation. Native Reschedule
places the two-day root on November 9-10, its one-day FS successor on November 11
and the one-day SF successor on November 10. All three starts change.

The SF successor finishes November 10 at 17:00; its predecessor starts November 11
at 09:00. The sixteen-hour displayed-clock gap contains zero working seconds.
They are adjacent working boundaries in this profile. The default calendar profile
is checked through the existing strict constructor admission and its payload is
identical in the original, native input and native result. This does not establish
finish-boundary policy for different calendars, exceptions or task constraints.

The eight calculation controls use nine native Save/unload/reopen sequences,
including the newly constructed three-task graph. Post-calculation and reopened
General snapshots match in 697 individual field comparisons. Eleven native images
were inspected. Task GUIDs, full eighty-byte link records and calendar payloads
remain intact. All 57 protected inputs and 844 source/schema hashes remain exact;
the unchanged source retains its earlier fifteen checks and 2,820 all-target tests
(25 ignored). No new runtime build or expanded scheduler admission follows.

Existing source-bound scalar CLI operations reconstruct each of the four propagated
native Tasks payloads exactly by changing only the successor start, or all three
starts in the active SF case. They preserve the other 47 stored source sections.
These projections were not themselves opened/saved/reopened in native UI. They
prove the observed storage reconstruction, not native transaction history, generic
saving or a new scheduler command.
