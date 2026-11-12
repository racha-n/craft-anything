# Project audit, camera presentation and source reports

All capabilities dispatch through shared engine tools. The desktop remains a thin client.
The native GPU renderer, camera, draw planning, texture decoding and shader live in
`engines::scene_render` and are shared by preview and headless export. No source program
is needed and no source project bytes are written.

## Identity and appearance audit

Use Source audit in 3D/Navigator, or `formats_project_audit`. The result records the
scene/source hashes, populated/blank reader display-ID fields, colour candidates and
texture/UV coverage. Reader fields are not promoted to native UI ID semantics. Blank IDs
retain the explicitly labelled GUID fallback. Source-opacity conversion, complete material
inheritance, texture sampling and native visual parity remain unresolved. Retained saved
controls can verify their admitted RGB cases without proving other channels or versions.

## Camera/focus path

Open Camera animation on 4D Review. Restore a Saved viewpoint or position the model and focus time, then Capture keyframe.
The first keyframe starts at zero; later times must increase. Capture another pose, Save
animation, Load animation or Play path. The path-time slider samples the same interpolation
used by the exporter: shortest yaw, linear target/pitch/source-clock focus, logarithmic zoom.
All keys use the same manually selected up axis. Time remains the stored displayed clock;
there is no timezone conversion. Preview and timeline playback are separate controls.

Paths contain version, scene hash and 2-64 camera/focus keys over at most 600 seconds.
`formats_scene_animation_manage` stores them in the session configuration directory under
`scene-animations/<scene-id>.json`. Unknown versions, scene mismatches and malformed existing
sidecars are rejected without overwrite. Version 1 has no prior migration; a future writer
must explicitly migrate a backup of this version rather than silently replacing it.

## Video

Export AVI writes a new lossless 24-bit RGB AVI and `<name>.avi.manifest.json` using the
shared headless renderer. The manifest records every frame's camera, focus, draw counts,
RGBA hash, rendering options and adapter. It includes the last keyframe, so the encoded
video holds the endpoint for up to one frame interval. Limits: 1-30 fps, 2-900 frames,
2-1600 pixels per dimension, 512 MiB and a 15-minute render deadline. Lower resolution,
fps or duration when the uncompressed-size check refuses the request. Selection highlights
and UI overlays are omitted; current colour, visibility and 4D preview policies are retained.
Compressed MP4 remains disabled and visible as future work.

`formats_scene_video_export` provides the same operation headlessly. It validates the
output plan before GPU creation, stages the video privately and publishes new files without
replacement. A cancelled render does not publish the staged AVI. If manifest publication
fails after the video link succeeds, the error preserves that AVI and identifies the partial
publication. A compatible graphics adapter is required; initialization, submission and
readback waits are bounded. AVI layout follows the [Microsoft RIFF/AVI reference](https://learn.microsoft.com/en-us/windows/win32/directshow/avi-riff-file-reference).

## Reports

Schedule report / Cost report opens Export CSV reports for the loaded 3D project. Select a new output folder name. Metadata-only path export remains explicit future work; presentation/report windows are suspended while the metadata-only view is active.
`formats_project_report_export` writes tasks.csv, resources.csv, costs.csv,
actual_expenses.csv, phases.csv, snapshot.json and manifest.json. Every report is tied to
source hash, scene hash, focus and date/appearance policy. Direct budgets, resource forecasts
and recorded expense totals remain separate; unresolved counts and partial totals remain
explicit. Null amounts become empty CSV cells. Source currency context is retained with
minor-unit scale 100; no currency/unit conversion or engineering aggregate is inferred.
`display_id` is the reader field, with unverified native semantics; GUID is the stable source
reference. Potential spreadsheet-formula text is prefixed with an apostrophe and CSV quotes
are escaped. snapshot.json preserves unmodified text and structured values.

The destination directory must not exist. Files are created without replacement and a valid
manifest is published last. Cancellation/I/O failure may leave an incomplete directory;
preserve it and choose a new destination. Consumers must validate the manifest and file
hashes before treating the folder as complete. Individual CSV files are capped at 32 MiB
and the structured snapshot at 64 MiB. Report templates and formatted PDF reports remain
future work.

The version-1 schemas are under docs/schemas: scene-animation-v1.json,
scene-video-manifest-v1.json, project-report-manifest-v1.json and
project-report-snapshot-v1.json. Unknown versions require a future explicit migration;
existing report/video outputs are never updated in place.
