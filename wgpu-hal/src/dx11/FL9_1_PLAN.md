# FL9_1 completion plan

## Coordinator status and specialist review — 2026-11-12

The backend is now a substantial opt-in public wgpu graphics subset, not a
private-HAL prototype and not a complete FL9_1 implementation. Explicit
`Backends::DX11` selection can enumerate a physical adapter with a conservative
FL9_1 projection; default backend selection still excludes DX11. The public
path has tested multi-group resources, dynamic uniform ranges, textured
depth-tested rendering, selected texture formats and transfers, and desktop
HWND presentation on the RTX 3090 Ti forced to FL9_1.

Three read-only specialist reviews found no additional confirmed
Core-valid-to-unimplemented-HAL mismatch in their scoped audits. They agree
that the broad contract audit remains incomplete. Major work divides into:
ordinary graphics and shader subsets still implemented only narrowly;
device-query-gated format/usage/view combinations; incomplete curated CTS and
Core queue teardown now uses a bounded HAL idle-wait attempt. A timeout roots
the queue and its resources in the Hub without treating ordinary in-flight
work as device loss. `device.poll` and `poll_all` retry orphan retirement;
`poll_all` continues to report outstanding orphan queues, including after the
Device leaves the public registry. Device drop also has a bounded retry. If a
HAL error prevents idle confirmation, Core reports the error and retains the
queue; unresolved queues are intentionally leaked at Hub shutdown rather than
releasing possibly in-flight resources. Synthetic timeout, device-removal and
OOM fixtures cover the lifecycle. Polling an orphan may spend up to the
configured 10 ms reaper budget, with normal Windows scheduling granularity
applying. Synchronous D3D calls themselves are outside that deadline.
Historical FL9_1 hardware, physical device-removal recovery, and Lumia/ARM32
CoreWindow execution remain unverified.

Keep genuine FL9_1/profile boundaries rejected, including compute/UAV/storage
execution, geometry shaders, multiple color targets, indirect draws, occlusion
queries, ordinary 1D/array textures, and fragment derivatives/`SampleGrad`.
Do not assign broad feature-family implementations to concurrent agents:
hardware research, shader/compiler analysis, and Core/HAL review are useful
independent roles, while cross-layer implementation and integration remain
coordinator-owned. The timeout-aware queue and pending-present path is now in
place. Next priorities are a systematic advertised-capability-to-Core/HAL test
matrix, curated CTS coverage, and evidence-led graphics/shader feature slices.

## Adapter exposure clarification — 2026-11-12

The D3D11 backend is available through explicit public `Backends::DX11`
selection. It enumerates only physical adapters that create an explicitly
requested and confirmed FL9_1 device and pass the conservative capability
projection. DX11 remains absent from the default instance backend set; the
`experimental_dx11_is_not_enabled_by_default` test verifies that default
isolation, not zero adapters after explicit DX11 selection. Older journal
entries saying public enumeration is disabled describe the earlier state and
must not be read as current behavior.

## Current verified boundary — 2026-11-12

The worktree now verifies thirteen compact user constant-buffer slots per
shader stage, selected BGRA8 surface `COPY_SRC`, and a narrow helper-computed
LOD path for single-mip fragment sampling, plus a single straight-line helper
around the emulated fragment `textureGather`. Strict tests verify all thirteen
fragment registers, a surface copy/readback followed by reconfiguration to
render-only, the helper LOD output and matching direct/helper gather output on
the physical RTX 3090 Ti at requested and actual FL9_1. Computed LOD still
requires unchanged coordinates, one selected mip, equal min/mag filtering and
anisotropy one.

Independent audits found no confirmed Core-valid command reaching an
unsupported HAL path in the reviewed resource, map, copy, draw and dispatch
areas; this is not a complete Core/HAL validation proof. Surface reconfiguration
now has targeted coverage. Public timestamps remain disabled because D3D11
timestamp validity/frequency cannot currently be propagated across the
cross-submission behavior Core permits. Historical FL9_1 drivers, physical
device-removal recovery, and Lumia/CoreWindow runtime remain unverified.

The Core fence-error path drains pending `map_async` notifications with a
`DeviceLost` result. A strict forced-FL9_1 fixture submitted a real buffer
clear, scheduled a map, injected device removal during fence wait and observed
the map and device-lost callbacks exactly once. This exercises injected
failure handling, not physical device removal. Queue teardown now also retains
in-flight queue resources after unexpected idle-wait errors. Its remaining
presentation wait is still blocking and can hang if the frame-latency signal
never arrives; a resumable timeout-aware HAL wait is the next lifecycle task.

Next candidates should be tested individually rather than assigned as broad
feature families. The RTX query probe now confirms RGB565 multisample
render/resolve at 2x/4x/8x, and the path executes end to end. Fragment
`mat4x4` inverse was probed through FXC and exceeds the FL9_1 pixel instruction
budget; keep fragment inverse rejected. A separate FXC probe confirms native
`ddx`, `ddy`, `fwidth` and `SampleGrad` are unavailable under `ps_4_0_level_9_1`;
the selected-entry Core rejection remains correct. Compute/UAV, geometry
shaders, multiple color targets, indirect drawing, ordinary arrays and 1D
textures remain hardware or intentionally gated boundaries, not parallel
agent coding tasks.

## Current coordinator increment — fragment gather helper — 2026-11-12

The narrow `texture_gather_2d_float_fragment` subset now permits one selected
fragment entry point to forward its location-0 `vec2f` unchanged to one
straight-line helper that directly returns one gather. Naga's existing
four-point-sample emulation remains in use; this does not enable native HLSL
Gather or the general gather limit. Core, HAL and Naga reject nested or
repeated helper calls, transformed coordinates, extra gathers or image
operations, and unrelated resource use. The HAL compiles the helper output
through FXC at `ps_4_0_level_9_1`.

Strict execution on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, physical adapter) explicitly requested and confirmed FL9_1.
Direct and helper draws each matched all 64 linear-view output pixels against
`[151, 166, 106, 91]`; direct sRGB gather matched all 64 decoded pixels.
Regression suites passed Naga 172/172, Core DX11 147/147, Core DX12 135/135,
private HAL 520/520 and strict public DX11 155/155. The public suite had no
Rust test failures or skips. The MSVC wrapper's existing `vError: Unknown
error` banner and four HAL `unused_qualifications` warnings remain.

## ARM32 UWP cross-check — 2026-11-12

On the current worktree, both the D3D11 HAL and public `wgpu` compile for
`thumbv7a-uwp-windows-msvc` using the installed nightly with `build-std`.
This verifies the ARM32 UWP compile path only. A Device Portal request to the
configured Lumia timed out, so CoreWindow startup, rendering and readback
remain unverified on Windows 10 Mobile.

## Render-pass CTS capability boundary — 2026-11-12

The strict raw `webgpu:api,operation,render_pass,*` selector produced 3 passes,
41 skips and 139 failures. Source and log triage attributed the failures to
fixtures requiring multiple color attachments, unsupported texture arrays or
depth mip chains, or texture copy usages/formats outside the selected FL9_1
projection. No failure of a confirmed supported render-pass operation was
found. Scoped strict CTS rechecks passed RGBA8 clear/readback (1/1), ordered
buffer-copy transitions (1/1), color-write-mask cases (16/16), buffer-to-
texture upload (1/1) and RGBA8 texture-to-texture copy (1/1). Details are
recorded in `FL9_1_CTS.md` and `FL9_1_PROGRESS.md`.

## Multiple render-target boundary — 2026-11-12

The one-color-target projection is not an unfinished FL9_1 backend feature.
Microsoft's 10Level9 resource-limits table defines
`D3D_FL9_1_SIMULTANEOUS_RENDER_TARGET_COUNT` as 1; four targets begin at
FL9_3. The multi-attachment render-pass CTS cases therefore exercise a native
FL9_1 hardware boundary. Preserve the one-target limit and reject additional
fragment outputs in Core. Source: [Microsoft Direct3D 11 resource limits](https://learn.microsoft.com/en-us/windows/desktop/direct3d11/overviews-direct3d-11-resources-limits).

## Current coordinator increment — sRGB fragment `textureGather` — 2026-11-12

The existing narrow direct fragment gather path now accepts a query-supported
single-mip `Rgba8UnormSrgb` view alongside `Rgba8Unorm`. Core retains and checks
the live view format because its downlevel limit cannot encode a format list;
other formats such as R8 fail before HAL. The physical RTX 3090 Ti at forced
FL9_1 matched the decoded sRGB gather output on all 64 pixels and retained the
linear baseline; exact commands/results are in `FL9_1_PROGRESS.md`.

An independent audit found no confirmed Core-valid command/resource request
reaching an unsupported HAL operation in the reviewed map/copy/draw/dispatch
paths. The public-usability audit found adapter classification remains
`Other`, which is intentional until DXGI can establish architecture
reliably; offscreen applications can enumerate/filter using name and PCI IDs.
Independent audits and compiler probes are useful agent tasks; cross-layer
implementation and strict end-to-end proof remain coordinator-owned.

## Current coordinator increment — computed helper coordinates for fragment 2D `textureLoad` — 2026-11-12

The existing selected-entry fragment 2D float `textureLoad` proof now permits
one straight-line helper to transform its unchanged `@location(0) vec2f`
argument with a bounded set of literal-based f32 operations before the
`vec2i` conversion. Core and HAL use matching fail-closed expression checks;
the HAL FXC proof compiles `ps_4_0_level_9_1`. Strict public readback on the
RTX 3090 Ti verified a one-column shift and zero for out-of-bounds results at
group 0/binding 7 and group 1/binding 42, requested/actual FL9_1. Entry-point
call-site arithmetic, nested helpers and other integer operations remain
rejected. Full regressions passed Naga 171/171, Core DX11 136/136, Core DX12
135/135, strict private HAL 517/517 and strict public D3D11 153/153. Details
and exact commands are in `FL9_1_PROGRESS.md`.

## Current coordinator increment — uniform-backed 3D `textureLoad` — 2026-11-12

The existing one-image fragment 3D `textureLoad` proof now permits up to six
used uniform buffers, matching the existing projected per-stage uniform limit.
Core, Naga and HAL preserve selected-entry checks and reserve `b13` for volume
dimensions. The RTX 3090 Ti executed a sparse group-1/binding-42 tint at forced
FL9_1 and all 64 pixels matched `[127, 0, 64, 255]`. Seven uniforms and
combined load/dimensions with uniforms remain rejected. See
`FL9_1_PROGRESS.md` for exact commands and the current regression state.

## Current coordinator increment — instance-indexed matrix uniforms — 2026-11-12

The selected vertex-stage dynamic uniform-array gate now permits a fixed
`mat4x4<f32>` array indexed directly by `instance_index`, through a 63-element
bound. Computed indices, non-square arrays and longer arrays remain rejected.
The HAL proof compiles the production HLSL through FXC, and a strict public
RTX 3090 Ti FL9_1 test renders and reads back two instances translated by
different matrix elements. FXC remains the aggregate shader-constant limit
check. Full regression verification passed Naga 171/171, Core DX11 136/136,
Core DX12 135/135, private HAL 517/517 and public D3D11 153/153; see the
progress journal for exact commands.

## Current coordinator increment — split-group volume query sampling — 2026-11-12

Removed the group-zero-only sampler restriction from the already narrow
selected-entry 3D/cube `textureDimensions` and `textureNumLevels` proofs.
Core, Naga and HAL continue to require the same queried/sampled image, at most
one compatible implicit sample, a supported non-comparison sampler and the
existing operation/view shape. Strict forced-FL9_1 public tests and the full
affected suites passed; see `FL9_1_PROGRESS.md` for exact results and commands.

## Current coordinator increment — public Depth24Plus bias and CTS fixture boundary — 2026-11-12

A strict public integration test now verifies zero, positive and negative
constant depth bias through a valid render-attachment-only `Depth24Plus`
texture. All 64 color readback pixels matched on the physical RTX 3090 Ti at
requested/actual FL9_1. The D24 bias CTS selectors each fail 3/3 because their
fixtures require unsupported depth-texture `COPY_SRC`; that pixel mismatch is
not valid evidence against the supported render path. Commit `df648a5a9`
records the test and triage. See `FL9_1_PROGRESS.md` for exact commands.

The mission remains partial. Continue the independent public API/Core/HAL and
shader-gap audits, then choose one evidenced, software-solvable vertical slice.
Keep historical FL9_1, Lumia/CoreWindow and device-removal verification gaps
separate from implementation work.

## Current coordinator increment — depth comparison CTS and cross-backend build — 2026-11-12

The three format-filtered `depth_compare_func` CTS runs now cover every
comparison function and clear value for D16, Depth24Plus and
Depth24PlusStencil8: 24/24 per format, 72 total, on the physical RTX 3090 Ti
at requested/actual FL9_1 with no skips or failures. Building the CTS runner
also exposed two newly added `DownlevelLimits` fields missing from GLES's
explicit initializer; GLES now conservatively sets both D3D11-specific limits
false. A standalone `wgpu-hal` GLES check passes. The limits remain false for
GLES and no GLES capability was changed.

## Current coordinator increment — explicit-LOD helper coordinates — 2026-11-12

The existing selected-fragment literal-LOD-zero proof now permits one
straight-line helper to compute the sampled `vec2f` coordinate from its single
`vec2f` input using only literal-based binary/unary arithmetic, composition,
splat and swizzle. Core and HAL enforce matching whitelists; resource handles,
uniform loads, math builtins, control flow, nested/repeated calls and extra
samples remain rejected. The existing single-mip view, equal min/mag and
anisotropy-one requirements keep lowering to implicit sampling semantics
bounded. Strict public FL9_1 readback on the RTX 3090 Ti verified the shifted
coordinate across all 64 pixels. Core render-bundle finish now validates the
bound view as render-pass draw validation already did; a hardware-backed Core
fixture rejected a two-mip view and accepted a one-mip view. Full regression
commands and results are recorded in `FL9_1_PROGRESS.md`.

## Current coordinator increment — restricted fragment `textureGather` emulation — 2026-11-12

The general gather capability remains disabled. The selected FL9_1 public
projection now advertises only a narrow fragment limit for one direct gather
from a single-mip `Rgba8Unorm` 2D view with a regular clamp-to-edge sampler.
Naga lowers the selected channel to four point samples at texel centers in
WGSL order; Core checks selected-entry proof and the currently bound view and
sampler before render or bundle draws. The strict public test rendered on the
physical RTX 3090 Ti at requested/actual FL9_1 and checked 64/64 pixels against
`[151, 166, 106, 91]`. Helper forms and multi-mip views are rejected.

The full verification run passed Naga 169/169, wgpu-types 118/118, Core DX11
136/136, Core DX12 135/135, strict private HAL 514/514, and strict public
D3D11 149/149 with no ignored Rust tests. The public suite logged three
optional subcase skips. Details and exact commands are in
`FL9_1_PROGRESS.md`. This does not establish historical FL9_1 driver behavior.

## Current coordinator increment — selected D2 query and comparison sampling groups — 2026-11-12

Selected fragment D2 `textureDimensions` and `textureNumLevels` proofs now
allow their one same-image implicit sample to use a compatible non-comparison
sampler from another supported bind group. Core, Naga and HAL retain the
selected entry, exact texture/sampler identities, one-sample, view-mip and
helper-shape restrictions. The separate-group 3D/cube query-sampler extension
was subsequently implemented and verified; see the progress journal's
2026-11-12 entry. No general image-query or unrestricted shader capability was
added.

The strict public `textureNumLevels` cases put the texture at group 2 and
sampler at group 3, switched between one- and three-level views, and matched
64/64 pixels in both direct and helper variants. A separate strict public
comparison-offset case puts the depth texture at group 1 and comparison
sampler at group 2 and checks all 64 pixels. Both ran on the physical RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, `software=false`), requested
and actual FL9_1. The updated full suite passed: strict public 148/148,
private HAL 512/512, Core DX11 133/133, Core DX12 132/132 and Naga 167/167.
Some optional device-specific subcases logged skips; no Rust hardware test
skipped. The public offscreen example verified 1,024 pixels of a textured
indexed cube, and the public HWND example presented five textured, indexed,
depth-tested frames.
Naga, Core, HAL and public-test Clippy completed; the HAL dependency still has
six existing warnings in index, transfer and texture code. Formatting and
`git diff --check` passed. The Visual Studio wrapper emits its existing
`vError: Unknown error` startup banner while Cargo completes successfully.

Independent Core/HAL and adapter-readiness reviews found no confirmed invalid
request in their scanned paths but do not close the full public-operation
audit. Separate read-only shader and feature-level audits are underway to
choose the next bounded slice. Compute/UAV, geometry shaders, MRT, indirect
drawing and occlusion queries remain unavailable at FL9_1; historical drivers,
Lumia/CoreWindow runtime and physical device removal remain external evidence
gaps.

## Current coordinator checkpoint — generalized fragment `textureLoad` groups and HWND frame pacing — 2026-11-12

The existing selected-entry Core proof and HAL/Naga binding path already retain
the full logical group/binding for the restricted fragment 2D `textureLoad`
subset. This checkpoint adds direct/helper Core proof coverage for group 7,
binding 42, including an unsupported sibling entry to verify entry isolation,
and a strict public readback test with an explicit empty group 0 and texture at
group 1/binding 42. The physical RTX 3090 Ti ran at requested/actual FL9_1 and
verified 384 output pixels across direct, nested-helper, dynamic-coordinate,
out-of-bounds, nonzero-mip and query-supported sRGB cases. No new texture
shape, format, query or capability gate was broadened. A stale Core-only
group-zero condition on the narrow direct/one-helper dynamic-coordinate proof
was removed; the group-1/binding-42 GPU test now checks all four source
quadrants through that helper.
Focused Core DX11/DX12 and strict public commands/results are recorded in
`FL9_1_PROGRESS.md`.

The HWND surface now creates a native DXGI frame-latency waitable swap chain,
sets its maximum frame latency to one, waits before acquisition, and drains
pending presentation from queue idle and unconfigure. Strict hidden-HWND tests
cover completion, resize/reconfigure and destruction with a pending present.
This closes the prior HWND implementation gap; CoreWindow runtime behavior and
Lumia execution remain unverified. Exact results are recorded in
`FL9_1_PROGRESS.md`.

The proposed group-zero restriction in 3D/cube query-sampler proofs has since
been removed and strictly tested. The direct instance-indexed fixed
`mat4x4<f32>` uniform-array candidate is now implemented and strictly tested;
see the current coordinator increment at the top of this plan and the detailed
progress entry. Independent follow-up audits are selecting the next software-
solvable feature slice. Compute/UAV, geometry shaders, MRT, indirect drawing,
1D textures and ordinary arrays remain FL9_1/profile or public API boundaries,
not candidates for emulation in that slice.

## Previous coordinator checkpoint — vertex-only mat4 inverse — 2026-11-12

Naga's HLSL writer now lowers f32 4x4 matrix inverse through explicit
cofactors. DX11 exposes a stage-specific `matrix_inverse_4x4_vertex` limit;
Core permits the operation only in the selected vertex entry and its reachable
helpers, while fragment mat4 inverse remains rejected. A strict RTX 3090 Ti
FL9_1 test uses a non-symmetric runtime vertex-input matrix and validates all
64 pixels as `[64, 128, 128, 255]` within two UNORM steps. The 2x2/3x3 paths
remain covered. Full validation passed: Naga 164, wgpu-types 117, Core DX11
132, Core DX12 131, strict private HAL 510 and strict public D3D11 144; the
focused strict public render and capability gate passed 1/1 each. Clippy and
format checks passed, with six existing HAL warnings. Exact commands and
hardware details are in `FL9_1_PROGRESS.md`.

The texture load proof generalization is now covered above. Remaining group-0
proof restrictions on optional sampler/query forms and the DXGI
presentation-completion/frame-latency contract remain under audit.

## Previous coordinator checkpoint — nonzero-group base-clamp sampling — 2026-11-12

The selected-entry fragment `textureSampleBaseClampToEdge` proof now accepts
its single 2D float image and non-comparison sampler from any supported bind
groups, including separate groups. The one-sample, helper-shape, single-mip,
selected-entry and other resource restrictions remain. Strict RTX 3090 Ti
execution at requested and actual FL9_1 used group 1/binding 7 for the image
and group 2/binding 42 for the sampler; all 64 patterned output pixels matched
for out-of-range coordinates. Naga, Core and HAL focused tests passed; full
Naga (164), Core DX11 (132), Core DX12 (131), strict HAL (510) and strict public
D3D11 (144) suites passed. Details and exact commands are in
`FL9_1_PROGRESS.md`.

The next worthwhile software slice is vertex-stage f32 `mat4x4` inverse: an
independent FXC probe stayed within FL9_1 vertex instruction and temporary
register budgets, but production Naga lowering, a stage-specific Core limit
and GPU validation remain. The presentation-completion contract also remains
open and needs a separate DXGI wait design plus UWP validation. Historical
FL9_1 drivers and Lumia/CoreWindow execution are external evidence gaps.

## Previous coordinator checkpoint — nonzero-group `textureNumLevels` — 2026-11-12

The narrow selected-entry fragment `textureNumLevels` proof now accepts its
queried float image from any bind group supported by the shared binding map.
The optional implicit sample remains restricted to a group-0 non-comparison
sampler; the direct/helper shape, view-mip emulation and general query
rejections remain unchanged. Strict RTX 3090 Ti execution at requested and
actual FL9_1 tested group 2/binding 42 and switched one- and three-level views.
Direct and helper forms, with and without a same-image implicit sample, each
matched all 64 output pixels. Naga, Core and HAL proof tests passed. Full
regression results and exact commands are in `FL9_1_PROGRESS.md`.

The selected query now uses any resource group while keeping its optional
sampler in group 0. Later base-clamp work has since generalized both resources
to separate groups.

## Previous coordinator checkpoint — query-gated BGRA8 texture loads — 2026-11-11

The narrow fragment 2D `textureLoad` path now accepts query-gated BGRA8
unorm and sRGB views. Core protects the single-mip and exact sRGB resource/view
format contract; HAL keeps the additions limited to 2D point-load emulation.
The RTX 3090 Ti at requested/actual FL9_1 reported sampled-upload support for
both formats, and strict GPU readback verified logical RGBA ordering for all
64 pixels per case, including sRGB decoding `[128, 64, 32, 255]` to
`[55, 13, 4, 255]`. Historical format support remains device-query-dependent.
Narrow format unit tests and strict GPU execution passed. Full regression
results: HAL 510/510, Core DX11 132/132, Core DX12 131/131 and public D3D11
144/144, with three optional format/input subcase skips and no hardware test
skips. Targeted Clippy, formatting and diff checks passed; six known HAL
warnings remain. Details are recorded in `FL9_1_PROGRESS.md`.

Independent probes recommend keeping agent work focused: vertex-only f32 mat4
inverse is feasible but needs a new stage-specific limit and exact Naga/HAL/GPU
proof; `textureSampleBaseClampToEdge` group placement is now generalized; and
`Queue::wait_for_idle` still does not prove DXGI present completion. Continue
with one integrated vertical slice at a time. Compute/UAV, geometry shaders,
MRT, indirect draws and other documented profile/API limits are not software
backlog. Historical FL9_1 and Lumia/CoreWindow execution remain external.

## Previous coordinator checkpoint — nonzero-group 3D/cube dimensions — 2026-11-11

Naga, Core and HAL now preserve the logical group/binding identity for selected
3D and cube `textureDimensions` proofs in any supported bind group. Sparse
group-2/binding-42 public GPU tests cover direct and helper 3D queries and
direct/helper cube queries with view transitions. The group-0 sampler
restriction for optional sampling remains unchanged. Strict RTX 3090 Ti
FL9_1 readback passed; the cube case also verified constant mip 1.

Validation passed: Naga 164/164, Core DX11 132/132, Core DX12 131/131,
private HAL 510/510, focused nonzero-group public cases 5/5, and the complete
strict public target 143/143. The full public run logged three optional
capability subcase skips. Clippy, formatting and diff checks passed; Clippy
reported six existing HAL warnings. Exact commands and hardware output are in
`FL9_1_PROGRESS.md`.

The presentation audit confirmed that `Queue::wait_for_idle` currently waits
for a D3D11 immediate-context event query but does not prove completion of
DXGI presentation. Correct closure requires presentation-aware tracking and a
safe wait design, not another event query. It remains an open contract item
pending desktop and UWP validation.

The explicit `Backends::DX11` public path is usable with its conservative
FL9_1 projection; DX11 remains excluded from default backend selection. This
is not general wgpu completeness. Fundamental FL9_1 exclusions remain
compute/UAV, geometry shaders, MRT, indirect draws, 1D textures and ordinary
texture arrays. Historical FL9_1 devices, Lumia ARM32/CoreWindow and physical
device-removal recovery remain external verification gaps. Keep arbitrary
`textureSampleLevel` LOD rejected until its single-mip semantics are proven.
The next useful shader probe is production Naga lowering and GPU verification
of vertex-only f32 mat4 inverse; a standalone FXC cofactor prototype compiled
within the 9_1 vertex instruction and temporary-register budget, but no
backend support is established yet. Fragment position/front-facing remain
rejected after FXC profile probes showed no semantics-preserving fallback.

The earlier same-format 3D RGBA8 copy remains a restricted execution-ordered
CPU-shadow fallback; it cannot consume GPU-written 3D resources. Its
implementation and test checkpoints are `74c011a50` and `731c79e9f`.

## Recent continuation — BGRA8 multisample resolve counts — 2026-11-11

The strict public resolve test now executes all query-advertised BGRA8 2x,
4x and 8x paths on the RTX 3090 Ti at requested/actual FL9_1. All 64 pixels
passed the one-byte UNORM tolerance for each count. The focused test passed
1/1 and the complete strict public target passed 135/135 functions, including
the default-enumeration isolation check. This closes a test-evidence gap
without changing capability projection. Exact commands are recorded in
`FL9_1_PROGRESS.md`.

## Recent continuation — query-gated BGRA8 multisample resolve — 2026-11-11

BGRA8 now uses its own selected-device format-support and per-sample-count
queries for the implemented 2x/4x/8x render-and-resolve path. The strict RTX
3090 Ti test explicitly requested FL9_1, resolved a non-symmetric BGRA clear,
and checked all 64 readback pixels with one-byte UNORM rounding tolerance.
The full private HAL suite passed 510/510, the strict public D3D11 suite
135/135, and Core DX12 passed 128/128. This remains optional device support,
not a FL9_1 guarantee; query-negative devices continue to omit the sample
count capability. Detailed commands are in `FL9_1_PROGRESS.md`.

## Recent continuation — query-gated 8-bit SNORM fragment `textureLoad` — 2026-11-11

The selected-entry fragment 2D `textureLoad` subset now accepts queried
`Rg8Snorm` and `Rgba8Snorm` alongside the previously supported formats. The
point-sample emulation preserves signed channel semantics; the sample-bias
subset and other operation gates remain unchanged. The strict public GPU test
verified both formats, all four input texels and out-of-bounds zero at forced
FL9_1. Core DX11/DX12, the strict private HAL suite and full strict public
target passed; exact outcomes are in `FL9_1_PROGRESS.md`.

## Recent continuation — safe winit surface integration — 2026-11-11

The Windows-only `dx11_fl91_winit` example exercises safe public
`Arc<Window>` surface creation, indexed textured depth-tested rendering,
resize/reconfiguration and bounded presentation. Strict RTX 3090 Ti execution
selected physical FL9_1 hardware, presented 60 frames and observed a
400x300-to-640x480 physical resize. This improves consumer integration
evidence; it does not establish historical-driver or Lumia behavior.

## Recent continuation — query-gated Rg16Snorm fragment `textureLoad` — 2026-11-11

The narrow selected-entry fragment 2D texture-load subset now accepts
query-gated `Rg16Snorm` alongside the existing RGBA8, sRGB, R8 and RG8 cases.
Core and HAL keep the new format out of 3D loads and sample-bias emulation.
The strict public test requested `TEXTURE_FORMAT_16BIT_NORM`, used the
selected device's point-only sampled/upload path, and verified all four
signed RG texels plus out-of-bounds zero at forced FL9_1. Exact commands and
device evidence are recorded in `FL9_1_PROGRESS.md`.

## Recent continuation — nonzero-group 2D `textureDimensions` helper — 2026-11-11

The selected-entry fragment 2D `textureDimensions` helper proof now accepts
the query texture from any bind group, matching direct 2D queries and the
shared logical resource map. The helper stays argument-free, straight-line,
query-only and rejects other used resources. 3D/cube helper queries and
samplers used with the query remain group-0-restricted. Core, HAL, Naga and
FXC checks passed; strict public FL9_1 rendering on the RTX 3090 Ti transitioned
group 7/binding 42 between 16x8 and 8x4 views and matched all 64 pixels.
Historical driver behavior remains unverified. Exact commands are in
`TESTING.md` and `FL9_1_PROGRESS.md`.

## Recent continuation — query-gated R8/RG8 fragment `textureLoad` — 2026-11-11

The narrow 2D fragment `textureLoad` path now permits query-gated `R8Unorm`
and `Rg8Unorm` views alongside its existing `Rgba8Unorm` views. Core validates
the selected binding and single-mip parent requirement; HAL retains
operation-specific format checks so 3D loads and sample-bias offsets do not
inherit these allowances. The internal point sampler allows R8/RG8 paths when
the selected format query lacks `SHADER_SAMPLE`, but each resource must still
be valid for sampled upload. Strict RTX 3090 Ti FL9_1 readback matched all
normalized channels and WGSL's missing-channel and out-of-bounds semantics.
The separate sample-bias rejection remains covered. Historical hardware
behavior is unverified; detailed commands and suite counts are in
`FL9_1_PROGRESS.md`.

## Recent continuation — helper-contained sample-bias offset — 2026-11-11

The bounded fragment `textureSampleBias` constant-offset path now accepts one
coordinate-forwarding helper in addition to its direct-entry form. Core, HAL
and Naga independently prove the selected entry shape; Naga rewrites the
sample using the bound view's dimensions. Nested calls, changed coordinates,
multiple samples, and out-of-range offsets remain rejected. FXC accepted the
helper at `ps_4_0_level_9_1`, and strict RTX readback verified all 64 pixels
for each of the two tested view shapes. Exact commands and results are in
`TESTING.md` and `FL9_1_PROGRESS.md`.

## Latest current state — 2026-11-11

RGB565, RGB5A1 and RGBA4 now have query-gated sampled/upload mip chains. The
selected device must report `TEXTURE2D | MIP`, and the native constructor and
capability projection use the same per-format backend gate. The RTX query was
`0x0026F3A0` for B5G6R5 and `0x000213A0` for B5G5R5A1/B4G4R4A4 at requested
and actual FL9_1. Strict public coverage samples the uploaded RGB565 mip 1
through implicit derivatives and verifies its packed word and row sentinels;
RGB5A1/RGBA4 tests upload mip 1, copy a partial region, check padded
raw-shadow readback and sample a single-mip view. Historical drivers remain
unverified. The final strict private HAL suite
passed 508/508 and the public D3D11 target passed 131/131 on the RTX 3090 Ti;
the public default-enumeration isolation test passed 1/1. The initial full
public run exposed stale assertions that expected the newly query-gated packed
formats to remain single-mip-only; they were corrected while retaining
single-mip reporting when a device query or backend path lacks mip support.

RGB5A1 and RGBA4 texture-to-buffer readback now uses a separately gated,
execution-ordered CPU shadow after the selected `TEXTURE2D` query and GPU
`COPY_SRC` path are available. Strict forced-FL9_1 coverage checks raw source
and partial-copy destination words, sampled pixels, untouched borders, row
padding and sentinels. The complete strict private HAL suite passed 507/507;
the complete strict public DX11 suite passed 131/131 on the RTX 3090 Ti.
Historical driver behavior remains unverified; see `FL9_1_PROGRESS.md`.

The Rg16Snorm path now also supports query-gated multi-mip sampled uploads
and same-format mip copies when the selected device reports
`TEXTURE2D | MIP` and the backend path is enabled. `SHADER_SAMPLE` remains
independent: the RTX 3090 Ti reports point-only support at forced FL9_1.
Strict public tests sample mip 1 and verify partial mip-1 copy/readback with
padded rows. Historical FL9_1 behavior remains unverified; results are in
`FL9_1_PROGRESS.md`.

The selected FL9_1 public projection now separately enables a proven direct
`vertex_index` read from `vec2f`, `vec3f` or `vec4f` values. Core and HAL
reject helper, computed, pointer-escaping and write cases; Naga lowers the
accepted access to static swizzles with the restricted-index fallback. FXC
accepted all three vector sizes at `vs_4_0_level_9_1`, and strict RTX 3090 Ti
readback verified the vec3 fullscreen triangle. The complete strict private
HAL suite passed 503/503, the public target 129/129, Core passed 128/128 with
DX11 and 127/127 with DX12, Naga 163/163, and the focused wgpu-types test
passed 1/1. Historical FL9_1 hardware remains unverified; commands and
diagnostics are recorded in `FL9_1_PROGRESS.md`.

`Rg16Snorm` texture-to-buffer readback now has a separately backend-gated,
execution-ordered CPU-shadow path. The strict test validates all destination
texels after a partial GPU copy, including the zeroed border, nonzero readback
offset, padded rows and sentinels. The shadow duplicates each `COPY_SRC`
texture's raw storage; it is not native staging readback. Historical-driver
behavior remains unverified. The full strict private suite passed 505/505 and
the public D3D11 target passed 129/129; Core DX12 passed 130/130. Three
optional format/vertex-input subcases logged skips.

Query-gated `Rg16Snorm` same-format texture-to-texture `COPY_SRC` is now
implemented without render-target binding. The strict public test partially
copies a 4×4 region at nonzero destination origin and samples both copied and
untouched texels. RTX forced-FL9_1 execution passes; historical-driver
behavior remains unverified. The full strict private suite passed 502/502 and
the full strict public D3D11 target passed 129/129, with three optional
format/vertex-input subcases reported as skips; exact commands are in
`FL9_1_PROGRESS.md`.

The shared cube-format mask now represents each BC1/2/3 UNORM and sRGB format
independently. Before this correction, HAL's selected-device cube queries
already considered all six formats, but five formats mapped to zero in
`DownlevelLimits` and were therefore not projected through Core. The strict
RTX test now reports cube mask `0x0003ff1f` at requested/actual FL9_1 and
uploads/readbacks all six faces for each BC format. Raw block verification
uses the ordered CPU shadow. The latest strict RTX run also samples all six
BC1/2/3 UNORM and sRGB cube formats, checking each face's decoded value in the
output; historical-driver execution remains unverified.

The separately gated fragment `textureSampleLevel(..., 0.0)` subset now also
accepts one proven straight-line helper that forwards the entry point's
`vec2f` coordinate directly. Core and HAL reject nested helpers, helper
computation or control flow, dynamic or nonzero LOD, and incompatible
resources. Strict public execution matched all 64 expected pixels through
both direct and helper paths on the forced-FL9_1 RTX 3090 Ti.

The public `dx11_fl91_windowed` example also ran in strict mode and presented
three textured, indexed, depth-tested frames through an HWND surface. This
adds desktop-example execution evidence, not historical-driver or Lumia
CoreWindow evidence.

The public format projection test also creates canonical D2 views for every
advertised sampled, render-attachment, and combined view-usage subset across
the 21-format matrix. It creates those views on advertised two-mip resources
and multisampled attachment resources as well. The RTX passed all 216 view
checks without Core validation or HAL internal errors; this is creation
coverage, not GPU operation proof for every projected format.

## Previous checkpoint — 2026-11-11

RGBA8_SNORM now has ordered raw-byte texture-to-buffer readback through the
existing CPU-shadow mechanism. Its public readback bit requires both the
selected device's positive `TEXTURE2D` format query and the backend shadow
implementation. A strict forced-FL9_1 RTX test verifies a padded queue upload,
partial GPU copy across differently sized textures, zeroed untouched texels,
nonzero destination offset and padded readback rows. The focused target passed
3/3; the private HAL suite passed 499/499 and the full public target passed
129/129 test functions (one query-dependent subcase skipped internally).
This is a software fallback for an SRV-only format and does not enable native
staging readback or render-target use.
Follow-up strict coverage also reads mip 1 after a partial multi-mip texture
copy, validating all texels, unchanged regions, the nonzero destination offset
and row padding through the same shadow path.

Strict public BC coverage also reads each BC1/BC2/BC3 sRGB source directly
through the ordered CPU shadow and validates exact blocks, nonzero offsets,
padded rows and sentinels. At this checkpoint the shared cube mask did not
yet represent five queried BC formats; the correction and expanded cube test
are recorded above.

The recent independent contract reviews still found no new confirmed
Core-valid request reaching an unsupported HAL path in the audited areas. The
best next work remains evidence-led: complete the format-operation matrix,
then test a real client workload. Native FL9_1 exclusions and missing legacy
GPU/Lumia execution are not software fixes that agents can safely invent.

The upstream `wgpu-gpu` capability selector now executes on the opt-in DX11
adapter through `cargo xtask test` with `WGPU_BACKEND=dx11` and strict hardware
mode. Its limit requirements no longer demand compute/storage, and it declares
the sampler restrictions imposed by this adapter. The run selected the RTX
3090 Ti at FL9_1 and passed `[Executed]` 1/1. The xtask build also exposed and
fixed two stale `DownlevelLimits` test initializers and the trace player's old
surface-configuration return shape. The entire 1,843-test GPU suite was not
run; that is a targeted harness-integration result.

The selected fragment uniform-array subset now accepts one proven helper
forwarding form. Core, HAL and the public path agree on a selected fragment
entry that passes the direct `u32` member of its canonical uniform to one
straight-line helper; the helper performs one `vec4f` array load and returns it
directly. Core and HAL tests reject computed, repeated, nested, cross-uniform
and side-use variants. Strict RTX 3090 Ti execution at requested/actual FL9_1
verified all 64 output pixels for the helper-forwarded index 23. This is a
small compiler-boundary expansion, not a general relaxation of runtime integer
semantics. Core passed 140/140, private HAL 493/493 and the full public target
129/129 after the final unused-float-input consistency edge was added. The
dedicated strict uniform-array target also passed 1/1.

The direct fragment `textureSampleLevel` candidate is now implemented as a
separate narrow capability. For an unconditional direct `texture_2d<f32>`
sample with literal positive LOD zero, Naga proof lowers the operation to
implicit sampling; Core checks the selected entry's currently bound view is
single-mip and sampler min/mag filters match with anisotropy one before each
draw. The generic explicit-LOD capability remains false. HAL/Naga/Core tests
and strict RTX 3090 Ti FL9_1 readback validate the subset; helper, control-flow,
dynamic/nonzero LOD, non-2D resources and incompatible bindings stay rejected.
Full regression results will be recorded in `FL9_1_PROGRESS.md` after the
checkpoint run.

Checkpoint `ab926004e` fixes a public Core/HAL mismatch for index buffer
ranges: offsets remain index-width aligned, but an in-bounds byte size may
have a trailing partial index. Core clamps draws to the complete indices.
Strict public FL9_1 rendering verified a 13-byte Uint32 range draws three
indices and rejects a four-index draw.

Commit `0049e0705` adds an execution-ordered RG8_SNORM raw-byte shadow for
partial texture copies and readback. It fixes a HAL format-query mask mismatch
that incorrectly required an RTV despite the fallback. Strict FL9_1 public
rendering verifies padded uploads, partial uploads/copies, preserved bytes
and sampled output; the complete private and public suites passed after
aligning their format expectations.

The backend is a substantial opt-in public implementation for a bounded
FL9_1 graphics subset, not a drop-in backend for arbitrary wgpu programs or a
WebGPU-conformant implementation. Current support includes multiple bind
groups and dynamic uniform ranges, selected depth/stencil and MSAA paths,
query-gated formats, texture transfer/readback and a public textured cube
example. Compute/UAV, geometry shaders, indirect drawing, occlusion queries,
ordinary texture arrays and many shader/profile combinations remain
unavailable or deliberately gated. Distinguish native FL9_1 limits from
potentially implementable format or shader subsets before selecting work.

The complete Core/HAL contract matrix is still open. Recent read-only reviews
found no additional confirmed public Core-valid request reaching an
unsupported HAL path in the reviewed shader, capability/resource and adapter
paths. They classify one color attachment, no compute/UAV, no geometry stage,
no ordinary arrays/1D textures and no occlusion queries as native FL9_1
boundaries. A tightly proven single-mip fragment
`textureSampleLevel(..., 0.0)` subset is now covered; general explicit LOD
remains gated. `textureGatherCompare` remains gated because fixed-point
coordinate snapping prevents a general exact emulation proof. Next work should
continue the resource/validation contract audit and select only Core-valid
requests with an implementable FL9_1 behavior; do not treat native API limits
as software backlog.

The current tree's narrowly proven direct `vertex_index` lookup into a
locally initialized `array<vec2f, 3>` also passes the strict WebGPU CTS
`basic:fullscreen_quad` selector on the physical RTX 3090 Ti at requested and
actual FL9_1 (1/1, zero skips). This does not enable general dynamic vector
indexing; a separately gated direct vertex-index float-vector form is now
accepted, while raw unlowered vector indexing still fails FXC with X3548.
The fresh command and hardware evidence are recorded in `FL9_1_PROGRESS.md`.

Public HWND presentation and the desktop cube example have hardware evidence;
historical FL9_1 drivers, real Lumia/ARM32 CoreWindow execution and physical
device removal/recovery remain external verification gaps.

Next: continue the remaining Core/HAL audit and choose a shader expansion
from demonstrated application demand. Current format candidates R32_FLOAT
sampling and packed format render targets are not supported by the RTX's relevant
`CheckFormatSupport` bits. Do not treat 1D/ordinary arrays, 3D GPU copies, MRT,
compute/UAV, indirect drawing or occlusion queries as software backlog where
FL9_1 forbids them.

Checkpoint `18afefed1` adds ordered BC1/2/3 CPU-shadow
texture-to-buffer readback and a direct fragment `textureSampleBias` texel
offset subset. Strict tests pass on the RTX 3090 Ti at requested/actual
FL9_1; details are in `FL9_1_PROGRESS.md`. The previous implementation
checkpoint is `c75e82cef`, which allows a direct fragment `textureSampleBias`
to select between two finite literal values within the FL9_1 range. The earlier
capability correction `6b1cff301` withholds sRGB blending through FL9_3 while
preserving sRGB attachments and query-backed linear blending. A read-only
RGBA8_SNORM audit confirmed the existing direct
GPU-to-staging readback rejection is required by the FL9_x SRV-only copy
restriction. A bounded fragment-bias subset now accepts a direct `select`
between two finite literal values within the FL9_1 `texldb` range, with
matching Core/HAL proofs and strict GPU evidence. Arbitrary dynamic
`textureSampleBias` remains rejected because WGSL's legal bias range exceeds
the defined FL9_1 `texldb` range; clamping is not semantics preserving.

BC readback is a CPU mirror fallback, not native GPU readback; correctness
depends on ordered mirror updates from every write route. The sample-bias
offset path uses the selected 2D single-mip view dimensions, including a
single-mip view into a multi-mip parent. Current verification passed the
focused strict BC and sample-offset targets, the complete strict private HAL
suite (488/488), and public D3D11 suite (126/126). Core passed 124/124, Naga
162/162 and wgpu-types 113/113; the DX12 HAL check passed. The broader
contract audit remains open. Select the next implementation from a concrete
Core-valid request that does not already fail validation at a documented
FL9_1/profile boundary. Historical-device, Lumia/CoreWindow runtime and
physical device-removal verification remain external gaps.

This older checkpoint records the scope and verification of `18afefed1`; the
current state and next queue are maintained above. Arbitrary dynamic sample
bias, SNORM readback and unsupported profile operations remain gated.

## Earlier current-state checkpoint — 2026-11-10

Checkpoint `816b85fa1` permits direct, statically indexed column or scalar
component stores to local `matCx2<f32>` struct members; whole-matrix, dynamic,
helper, nested and array-contained stores remain rejected. The selected-entry
uniform-array, matrix-store, D24S8 depth-fail/stencil mapping and sRGB blend
checks below all ran on the forced-FL9_1 RTX 3090 Ti. The strict public test
suite now passes 126/126 with zero skips. A render-attachment-only D24S8 test
covers all stencil operations on pass, stencil-fail and depth-fail branches,
including clamp/wrap boundaries. A fixture-only `ResizeBuffers` error test
also verifies public device-loss propagation and reservation cleanup. The
backend remains opt-in through explicit DX11 selection; it is not a general
or Core-WebGPU-compliant FL9_1 implementation.

The D24S8 depth-fail CTS selector could not reach its intended operation
because its helper requires unsupported D24S8 `COPY_DST`. A render-attachment-
only public test now covers the operation without that prerequisite. The sRGB
blend CTS selector passed on this NVIDIA driver, but Microsoft's downlevel
documentation leaves historical FL9_x sRGB blend behavior as a portability
concern; retain that limitation until tested on older hardware. Continue the
Core/HAL contract audit using operations with valid public resource descriptors.

Known external verification gaps are historical FL9_1 hardware, Lumia/CoreWindow
runtime and physical device-removal/recovery behavior. Native compute/UAV,
geometry shaders, ordinary texture arrays, 1D textures and other unsupported
profile/resource features remain rejected; do not treat these as software
milestones unless a correct FL9_1 emulation is demonstrated.

## Strict CTS buffer/texture-copy slice — 2026-11-10

The curated strict DX11 CTS run now has additional public evidence for
buffer-to-buffer copies, ordered overlapping copies, buffer-to-texture
initialization and filtered RGBA8 2D texture-to-texture copies. All four
selectors passed on the forced-FL9_1 RTX 3090 Ti with zero skips. See
`TESTING.md` and `FL9_1_PROGRESS.md` for exact commands and counts. Continue
the downlevel CTS inventory with one selector at a time; selector success is
not a claim of general WebGPU conformance.

## HAL panic reachability audit — 2026-11-10

An independent read-only review found no confirmed safe-public-wgpu-reachable
production panic in the audited D3D11 device, encoder, queue, transfer,
buffer, texture and command paths. The review traced compute, cache, ray-
tracing and query panics to absent Core capability gates, and checked mapping
and render-pass state invariants. Surface/fence internals, shader/pipeline
internals and capability assertions remain outside this bounded audit; the
overall Core/HAL contract audit remains incomplete.

## Selected FL9_1 3D `textureDimensions` helper — 2026-11-10

The selected fragment-entry 3D `textureDimensions` emulation now accepts one
parameterless, straight-line helper that directly returns the level-less
`vec3u` query, when the entry point directly converts the result to `vec3f`.
Core, Naga and HAL independently constrain the proof to the selected entry
and the same volume. FXC requires the proven helper's return and call-result
temporaries to use `int3`; the writer scopes that substitution to the exact
validated helper handle. FL9_1's 256 dimension limit keeps host-supplied values
within signed range. Other helper/query forms remain rejected, and no general
texture-query capability is enabled.

Continue the public Core/HAL contract audit and select the next task from a
reproducible workload or compiler boundary. Keep historical FL9_1 hardware,
Lumia/CoreWindow runtime and physical device-removal verification as external
gaps; do not treat them as implementation tasks that more code agents can
resolve.

## Public float-builtin execution checkpoint — 2026-11-10

Added a focused strict public test for runtime-uniform `modf`, `asinh`,
`acosh`, and `atanh`; each produced the expected complete 8x8 readback at
forced FL9_1 on the RTX 3090 Ti. The test also records `frexp` as a predictable
Core rejection because its integer exponent is covered by the deliberate
runtime-integer safety limit. No capability changed. Continue selecting work
from reproduced public workloads and compiler/runtime evidence. Use agents for
independent read-only audits and feasibility checks; keep any cross-layer
implementation (Core validation, Naga lowering and HAL execution) under one
integrating owner. Historical FL9_1 devices, Lumia/CoreWindow runtime,
physical device-removal behavior, and exhaustive API/CTS coverage remain open.

## Optional `Rgba8Uint` texture candidate — 2026-11-10

The strict integer vertex-format test now also queries
`DXGI_FORMAT_R8G8B8A8_UINT` on the physical RTX 3090 Ti after explicitly
creating an FL9_1 device. `CheckFormatSupport` returned `0x00000002`, which is
`IA_VERTEX_BUFFER` only; it did not report `TEXTURE2D`. Microsoft likewise
guarantees this format at FL9_1 only for vertex input. The `Rgba8Uint`
`COPY_SRC | COPY_DST` CTS cases therefore remain unsupported and are not an
actionable texture-transfer expansion on this device. Do not infer texture
support from the existing vertex-format mapping.

## Current confirmed shader-validation gap — 2026-11-10

The strict `basic:fullscreen_quad` CTS selector exposed dynamic indexing of a
function-local array that Core previously allowed through to the FL9_1 shader
path. A strict GPU run returned black output; FXC also rejects the tested
dynamic `vertex_index` vector-index form with X3548. Core and HAL now reject
selected-entry dynamic array/matrix/vector indexing before execution, while
preserving constant indexing and the separately proven uniform-array subset.
The regression follows helper calls and fails closed when bounded analysis
cannot prove the resource origin. The strict public/Core/HAL suites pass; this
CTS shader remains outside the adapter's reported downlevel subset and is
rejected explicitly.

Continue selecting new implementation work from reproducible ordinary API
gaps. The remaining broad work is contract-audit closure and representative
workload coverage, not indiscriminate expansion into FL9_1-incompatible APIs.
Historical FL9_1 hardware, Lumia ARM32/CoreWindow execution and physical
device-removal recovery require external hardware/runtime verification.

## Current coordinator audit — adapter selection and CTS boundaries — 2026-11-10

The Core/HAL unsupported-path audit found no confirmed public-API-reachable
panic or Core-valid operation reaching an unimplemented HAL panic in the
Device, CommandEncoder, Queue, resource, binding and pipeline paths reviewed.
The apparent vertex-plus-index buffer gap is already rejected by Core because
the DX11 projection omits `UNRESTRICTED_INDEX_BUFFER`; FL9_1 requires one native
vertex, index or constant-buffer bind role at a time.

The public adapter is opt-in, not globally hidden: default backend selection
excludes DX11, while an application with the Cargo feature enabled can request
`Backends::DX11` or `Backends::all()` and enumerate physical adapters passing
the FL9_1 projection. Documentation now states this distinction explicitly.

The strict CTS runner can validate curated compatible operations, but its
general image-copy cases generate unsupported dimensions and array shapes.
The filtered RGBA8/2D origins-and-extents case still generates array-layer
subcases, so its failure does not isolate the supported single-layer transfer
path. Keep those results separate from backend regressions. Continue auditing
Core validation and use strict public integration tests for downlevel-specific
operations; historical hardware and Lumia runtime remain external checks.

## Current verification checkpoint — ARM32 UWP and lifecycle audit — 2026-11-10

Strict public FL9_1 coverage now includes read-only stencil bundles and
fractional-viewport bundles (121/121 public tests pass). Independent audits
of render/Core-HAL, resource/transfer, shader and queue/fence contracts found
no confirmed correctness mismatch in the inspected paths. The surface audit
also found the opt-in HWND contract consistently gated; physical device
removal, display-mode changes and Lumia/CoreWindow runtime remain unverified.

Using Rust 1.95.0 nightly with `rust-src` and `-Z build-std=std,panic_unwind`,
both the D3D11 HAL and public `wgpu` DX11 feature set cross-check successfully
for `thumbv7a-uwp-windows-msvc`. This is compilation evidence only. The Lumia
Device Portal at `192.168.1.157` was unreachable from this host on ports 443
and 8080.

Continue the Core-valid-to-HAL audit, focusing on remaining query,
unsupported-operation and public surface error paths. Do not broaden limits to
turn intentionally unavailable FL9_1 APIs into accepted requests. Keep real
historical-GPU and Lumia testing as external verification requirements.

## Current Core/HAL fix — zero-input draw range — 2026-11-10

Zero-input, no-`vertex_index` draws now normalize the effective start to zero
when the API start/count arithmetic exceeds `u32`, across TriangleList,
LineList, LineStrip and TriangleStrip replay. A strict Core/HAL FL9_1 test
verifies submission and readback for each topology; negative controls retain
real-fetch and `vertex_index` rejection. Continue the broader contract audit;
do not change shared Core semantics for this backend-local case.

## Current Core/HAL fix — timestamped depth-Never draws — 2026-11-10

Valid depth-Never draws now execute with color and depth writes disabled, so
pass-boundary and explicit in-pass timestamps do not turn the draw into an
encoding error. Strict public readback verifies color, depth and timestamp
ordering at forced FL9_1. Continue the command/API contract sweep.

## Current strict CTS operation evidence — 2026-11-10

The strict CTS harness passes an RGBA8 clear/readback case and an ordered
buffer-copy/readback case on physical FL9_1 hardware. Keep expanding the
selected operation matrix only where the reported contract supports the
request. `basic:*` still has two expected RGBA8Uint copy-usage failures; do
not broaden format support to make the generic selector pass. Continue the
Core/HAL audit and keep broad CTS and historical-device checks outstanding.

## Current Core/HAL contract fix — comparison-resource visibility — 2026-11-10

An independent audit found and closed one Core-to-HAL layout mismatch.
Depth-texture and comparison-sampler entries may declare combined
vertex/fragment visibility when their query-gated capabilities are available;
the HAL maps them only to the pixel stage. A strict Core fixture confirms
layout creation succeeds while actual vertex-stage depth sampling is rejected
before HAL. No capability changed. Continue the audit across remaining public
Core-accepted operations and retain the strict hardware/validation boundary.

## Current continuation — strict CTS execution path — 2026-11-10

`xtask cts --backend dx11 --require-dx11-hardware` now explicitly selects and
requires the physical FL9_1 adapter, and the Deno runner uses reported adapter
limits for omitted required limits only in this strict mode. The focused
`basic:empty:*` CTS selector passed 1/1 on the RTX 3090 Ti at requested/actual
FL9_1 (no GPU rendering operation). `basic:*` now reaches real strict adapter
selection and records 1 pass, 2 failures, 0 skips; both failures request
Rgba8Uint copy usages unavailable under the truthful projection. Keep these
operations rejected unless a separately justified capability change is
proven. Next, continue the Core/HAL acceptance audit and implement only
isolated FL9_1-compatible gaps. The shader review found many nearby operations
are profile or stage limitations and should not be delegated as speculative
features. Historical FL9_1 hardware and Lumia ARM32/CoreWindow runtime remain
external verification gaps.

## Current continuation — direct 2D load plus dimensions query — 2026-11-10

The existing fragment `textureLoad` and `textureDimensions` emulations can now
be used together for one direct, level-less query and one load from the same
group-0 `texture_2d<f32>`. Core requires both existing downlevel limits and
proves the selected entry shape; Naga and HAL independently require the same
resource identity, one load, one query, constant coordinates, and no helpers,
control flow, sampling or extra used resources. FXC accepts the combined
shader at `ps_4_0_level_9_1`. Strict public rendering on the RTX 3090 Ti at
requested/actual FL9_1 verified 64/64 pixels as `[100, 60, 80, 255]`. This
adds no new reported capability and does not enable general texture queries.
The public Core/HAL contract audit, broad shader/resource coverage, historical
FL9_1 devices, Lumia runtime and physical device-removal recovery remain open.

## Current continuation — sample-bias compatibility validation — 2026-11-10

Core now rejects DX11 FL9_1 `textureSampleBias` sampler combinations that the
HAL cannot execute: used samplers must have equal min/mag filters and effective
anisotropy one. The restriction is represented by a new default-false
downlevel limit and enabled only by the DX11 projection. Strict RTX tests cover
the positive biased-mip path and public Core rejection of mixed filters and
anisotropy two. The full public, Core and private HAL regression sets passed.
See `FL9_1_PROGRESS.md` and `TESTING.md` for results. Broader Core/HAL contract
auditing, shader/API coverage, historical hardware and Lumia runtime
verification remain open.

## Current capability-projection increment — per-usage color formats

RGBA8/BGRA8 linear and sRGB format usages are now projected independently
instead of requiring a single query to pass for sampled, copy, and attachment
uses together. A synthetic negative-query test verifies sampled plus
copy-destination support remains available when render-target support is
absent; the production checker still rejects attachment and copy-source use.
The strict RTX projection and public suite remain unchanged. This closes a
conservative reporting gap, not a driver capability expansion. See
`FL9_1_PROGRESS.md` for regression results.

## Latest rendering coverage — multisample blend/write mask

A strict public readback test now covers the interaction of query-supported
RGBA8 multisampling, alpha blending and partial channel writes. The RTX 3090
Ti at forced FL9_1 passed 2x, 4x and 8x resolve with all 64 pixels matching
the deterministic blend result. No capabilities changed. Continue the public
contract and query-gated interaction audit; see `TESTING.md` for the command.

## Latest execution evidence — 2026-11-09

The strict public test matrix now includes BC1/BC2/BC3 partial initialization
and WGSL matrix determinant execution for mat2/mat3/mat4 in vertex and fragment
stages. All 7 focused hardware tests passed on the RTX 3090 Ti at explicitly
requested/actual FL9_1. These are verification of already projected paths, not
new capability claims. See `FL9_1_PROGRESS.md` and `TESTING.md` for the exact
operations and commands. Remaining work is broad Core/HAL contract auditing,
additional public workload coverage, and external verification on historical
FL9_1 hardware and Lumia ARM32; the adapter remains experimental and does not
meet Core WebGPU defaults.

## Current coordination checkpoint — 2026-11-09

Test readiness now includes opt-in Deno/xtask DX11 routing and a strict
hardware preflight. With `WGPU_DX11_REQUIRE_HARDWARE=1`, Deno requires the
backend mask to select only DX11 and rejects adapter/configuration failures
instead of resolving `null`; accepted adapter identity must carry the HAL's
verified physical-FL9_1 marker. Ordinary adapter selection/defaults remain
unchanged. The strict `basic:*` CTS slice ran 3 cases: 0 passed, 3 failed,
0 skipped. Deno requested Core WebGPU default limits (including a combined
buffers-per-stage limit of 28), while this experimental adapter reports 22;
device creation failed before test operations. This is an expected default-
contract mismatch, not GPU execution evidence. The combined limit was corrected
from a stale 17 to 22 (six vertex uniforms plus sixteen vertex input buffers).
CTS operation coverage still needs a downlevel-aware request path; other Core
defaults also exceed this adapter's projection. Commits:
`f87abf0d3`, `d2994cebd`, `ee480928e`.
The corrected limit and its verification are checkpointed at
`bb34517a9ddb8a82c883f4a64c9a3154169e61f8`.

The separate-alpha contract audit found the current Core `SEPARATE_ALPHA_BLEND`
gate and HAL translation fail closed for RGB+alpha writes; RGB-only and
alpha-only masks safely use the active equation. A strict Core fixture now
covers alpha-only pipeline creation (`d4227636e`). No capability was added.

Current main gaps remain: exhaustive Core-valid versus HAL-executable public
contract coverage, broader shader/resource/pipeline behavior, actual historical
FL9_1 hardware, Lumia ARM32/CoreWindow runtime, and physical device-removal
recovery. See the latest progress entries and handoff for boundaries and
verification detail.

## Resolved Core/HAL boundary finding — viewport remap shader — 2026-11-10

Core now retains whether each pipeline has the optional FL9_1 fractional-
viewport vertex variant. Pipeline creation succeeds when only that variant
exceeds FXC's level-9 instruction budget; direct and indexed draws reject at
Core validation only when the active viewport needs the unavailable transform.
Render bundles record this requirement and are checked against the viewport
active when they execute. Ordinary integer viewport rendering and the base
shader remain available. A strict RTX 3090 Ti FL9_1 public test verifies
ordinary pipeline creation and readback, then checks Core's fractional-viewport
rejection. Full Core, HAL and public regression suites passed; see
`FL9_1_PROGRESS.md`.

The next bounded infrastructure task is a strict, downlevel-aware CTS request
path. The previous `basic:*` attempt failed at device creation because the
runner requested Core WebGPU defaults beyond the experimental adapter's
limits. Keep this adaptation test-only and preserve normal adapter/device
defaults. Historical FL9_1 hardware, Lumia/CoreWindow runtime, physical device
removal/recovery, and the broad Core/HAL operation audit remain open.

## Latest shader execution proof — fragment `discard` — 2026-11-09

The public FL9_1 shader path now has an end-to-end conditional fragment
`discard` test. Naga generated HLSL that FXC accepted for
`ps_4_0_level_9_1`. The first draw discarded four of eight interpolated-alpha
columns; all 64 readback pixels showed the clear color in the discarded half
and red in the written half. A later `LessEqual` depth-tested draw used a
farther depth and showed green only in the discarded half, checking that the
first shader also left depth untouched. The two readbacks matched all 128
pixels.

Strict execution selected the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and confirmed FL9_1. The focused
test passed 1/1 and the full strict public D3D11 target passed 115/115.
Formatting, diff checks and targeted Clippy passed; pre-existing HAL/Core
warnings remain. No capability projection changed. See `FL9_1_PROGRESS.md`
for exact commands and diagnostics.

The public shader/API matrix and historical-driver verification remain open.
Continue with query-gated resource operation candidates and isolated shader
shapes; retain the strict hardware requirement and default adapter isolation.

Checkpoint: `834944c4e` (`dx11: verify FL9_1 fragment discard semantics`).

## Latest public render-state verification — color write masks — 2026-11-09

The public FL9_1 rendering test now transitions through `ALL`, `RED`,
`GREEN`, `BLUE`, `ALPHA` and empty color write masks in one render pass. Each
scissored column begins from a distinctive clear color; readback checks all 64
pixels, including untouched columns. Disabled channels retain their previous
values, and an empty mask writes nothing. This is public API execution
coverage, not a capability expansion.

Strict execution selected the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and confirmed FL9_1. The focused
test passed 1/1 and the complete strict public D3D11 target passed 114/114.
Formatting, diff checks and targeted Clippy passed. The installed Rust 1.93
toolchain was used because the pinned 1.93.1 installation is incomplete.
See `FL9_1_PROGRESS.md` for exact commands and diagnostics.

No public capability changed. The execution matrix and Core/HAL acceptance
audit remain incomplete; historical FL9_1 hardware, Lumia ARM32/CoreWindow and
physical device-loss recovery remain unverified. Continue with bounded
query-gated public operations and audits, preserving default adapter
enumeration isolation.

Checkpoint: `f71993b7c` (`dx11: verify FL9_1 color write masks`).

## Latest verified multi-mip SNORM copy operation — 2026-11-09

The public FL9_1 path now executes a partial same-format RGBA8_SNORM copy at
mip 1 of four-level textures. The test uploads distinct signed patterns,
copies a 2x2 rectangle between nonzero origins in the 4x4 mip-1 subresources,
then samples a mip-1 suffix view into RGBA8 and verifies all 16 copied and
untouched output texels. The copy-source, copy-destination, sampled and mip
paths remain query-gated. CPU texture readback for SNORM stays unavailable;
only the RGBA8 rendered output is read back. This is execution evidence, not a
capability expansion.

Strict execution used the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and selected FL9_1. The focused
test passed and the complete `dx11_snorm_copy` target passed 2/2. See
`FL9_1_PROGRESS.md` for the exact command and results. Next, review the
remaining public-valid format/usage intersections and prioritize another
independently query-gated operation path.

Checkpoint: `effae2add` (`dx11: verify multi-mip FL9_1 SNORM copies`).

## Latest verified public usage-operation matrix slice — 2026-11-09

The public D3D11 path now executes the complete combined
`COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT` usage set for
R8_UNORM, R8G8_UNORM, RGB565, linear and sRGB RGBA8, and linear and sRGB
BGRA8. The test performs a padded queue upload, same-format GPU texture copy,
padded CPU copy-readback with sentinel-checked row padding, sampling into a
same-format render attachment, and attachment readback. All 64 copied texels
matched exactly and all 64 sampled output pixels matched within one byte for
each format. This exercises narrow channel/packed conversion, BGRA raw-byte
ordering and sRGB decode/encode; it adds execution evidence only, not new
capability.

Strict execution selected the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and confirmed FL9_1. All seven
formats reported the full usage union and none skipped. The focused test
passed 1/1 and the complete strict public D3D11 target passed 117/117. See
`FL9_1_PROGRESS.md` for commands. The existing format projection audit still
checks 21 formats and many usage combinations by creation only. Next, extend
representative operation sequences to query-gated SNORM/BC or packed/depth
paths without assuming the same usage contract across formats.

Checkpoint: `48cd0b95d` (`dx11: verify FL9_1 texture readback row padding`).

## Latest verified MSAA depth/stencil execution coverage — 2026-11-09

The existing query-gated 2x/8x RGBA8 multisample resolve test now checks
depth rejection for each supported depth format (D24S8, Depth24Plus and
Depth16Unorm) at each selected sample count. The D24S8 path checks stencil
clear-to-zero, Replace(1), equal-reference success, mismatched-reference
rejection, and depth rejection with distinct output colors. Strict public
execution on the RTX 3090 Ti requested and selected FL9_1; the adapter
reported 2x and 8x support. Six depth/sample cases each validated all 64
resolved pixels, with no skipped subcases. This strengthens verification of
existing capability-gated behavior and does not expose new formats or sample
counts.

Next: extend the public format/usage projection test from descriptor creation
to actual operation sequences for a compact set of combined usages, starting
with the common RGBA8/BGRA8 paths. The current 21-format table checks 189
advertised usage combinations, 136 rejected combinations, 21 mip rules and
40 sample-count rules, but drops every texture without executing it. A fresh
Core/HAL audit found no confirmed mismatch in the inspected formats, transfer,
attachment or resolve paths; this remains a proof-coverage gap. The panic
reachability audit found no valid public path to the existing compute,
pipeline-cache, ray-tracing or acceleration-structure panic stubs. The shader
audit found no low-risk adjacent FL9_1 WGSL feature; explicit LOD, derivatives,
gather, front-facing and fragment position remain profile restrictions.
Checkpoint: `fa15bc6f8` (`dx11: verify FL9_1 multisample depth and stencil`).

## Latest verified shader-query increment — selected `textureNumLevels` helper plus sample — 2026-11-09

The selected 2D fragment `textureNumLevels` helper may now coexist with one
direct implicit sample of the same image. The helper remains a single
argument-free straight-line query; the entry point must use one group-zero
non-comparison sampler, `vec2f` coordinates and the same `texture_2d<f32>`.
Core, Naga and HAL validate the selected entry and reject mismatched images,
multiple samples, explicit LOD, helper sampling and extra selected resources.
FXC accepts the combined shader at `ps_4_0_level_9_1`. General
`texture_query` remains unavailable.

Strict public execution on the physical RTX 3090 Ti requested and selected
FL9_1 (`0x10de:0x2203`, driver `32.0.16.2101`, software=false). The test
verified helper-only and helper-plus-sample variants across one- and
three-level views; the composed readback matched `[85,64,128,255]` and
`[255,128,64,255]` for all 64 pixels. Full strict private HAL passed 462/462,
public D3D11 116/116, Naga 156/156, Core 125/125, and Core DX12 116/116.
See `FL9_1_PROGRESS.md` for exact commands and resolved failures.

The next small execution-coverage gap is depth testing in the optional 2x and
8x MSAA cases; the current test only verifies resolved color there. Add
near/far depth rejection for each query-positive format and stencil where the
format and sample count support it. In parallel, continue the public
Core-accepted-operation versus HAL-execution matrix; do not broaden public
exposure based on resource creation alone. Historical FL9_1 hardware and
Lumia/CoreWindow execution remain unverified.
Checkpoint: `8a8e6350f` (`dx11: sample through selected textureNumLevels helper`).

## Latest verified shader-query increment — selected 2D dimensions helper plus sample — 2026-11-09

The selected 2D fragment `textureDimensions` helper path now permits one
level-less implicit sample of the same texture in the entry point. Naga, Core
and private HAL use entry-point-specific proofs; FXC compiles the resulting
shader for `ps_4_0_level_9_1`. The helper remains one no-argument,
straight-line call returning dimensions for direct conversion to `vec2f`.
Only one same-image sample with a group-zero non-comparison sampler is added;
explicit LOD, helper sampling, extra resources, control flow, `select` and
other image operations remain rejected. General `texture_query` stays false.

Strict public execution on the physical RTX 3090 Ti requested and selected
FL9_1 (`0x10de:0x2203`, driver `32.0.16.2101`, software=false). It switched
between 4x2 and 8x4 views and verified all 64 output pixels, with expected
colors `[50,40,80,255]` and `[40,60,160,255]`. The complete private HAL suite
passed 460/460, full public D3D11 passed 116/116, Naga passed 155/155, Core
passed 124/124, and Core DX12 passed 115/115. See `FL9_1_PROGRESS.md` for the
exact commands. The next bounded shader candidate is the analogous selected
2D `textureNumLevels` helper plus same-image sample; continue auditing broader
public contract and resource/format coverage in parallel. Historical FL9_1
drivers and Lumia/CoreWindow execution remain unverified.
Checkpoint: `f8cd666aa` (`dx11: sample through selected textureDimensions helper`).

## Latest verified shader-query helper increment — 2026-11-09

The narrow 2D fragment `textureNumLevels` path now permits one selected,
argument-free straight-line helper that returns the query result for direct
conversion to `f32`. Core, Naga and HAL use matching selected-entry proofs.
The helper form is query-only; direct query-plus-sample remains a distinct
path, while 3D/cube query support remains direct-only. Other resources,
operations, calls and control flow are rejected.

Strict public execution on the RTX 3090 Ti explicitly requested and confirmed
FL9_1. It switched between one- and three-mip views and verified the full
readback for each view. The focused helper test, the 3D/cube query regression,
and all previous full Naga, Core, private HAL, public D3D11 and DX12 suites
passed. See `FL9_1_PROGRESS.md` for exact command results. This remains
metadata emulation, not general texture-query support or historical-driver
verification.

## Latest verified shader-query increment — mip count plus implicit sampling — 2026-11-09

The restricted fragment `textureNumLevels` path now permits one implicit sample
of the same direct texture using one group-zero non-comparison sampler. Core,
Naga and HAL require a dimension-matched float coordinate, one query, one sample,
no additional image operations, and no other bound resources. The proof metadata
records whether the selected entry samples the texture so command replay retains
the native SRV for query-plus-sample shaders while keeping the query-only path's
slot clearing behavior. General `texture_query` remains disabled.

Strict execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested and confirmed
FL9_1. A shader combined `textureNumLevels` with an implicit sample while
switching between one- and three-level views. All 64 pixels matched
`[64,64,128,255]` and `[191,128,64,255]`. Initial hardware execution revealed
that replay cleared pixel SRV slot `t0` for the query-only case even when the
shader sampled; the retained sample flag fixed this. Full private HAL passed
453/453, full public D3D11 passed 110/110, strict Core fixture 122/122, Naga
153/153, Core DX12 110/110 and wgpu-types 108/108. Details and exact commands
are recorded in `FL9_1_PROGRESS.md`. This is a narrow selected-entry form, not
general query support or historical-driver verification.

## Latest shader-query increment — constant-mip cube dimensions — 2026-11-09

Checkpoint: `1e8407af6` (`dx11: extend FL9_1 cube queries and SNORM copies`).
The existing selected-entry cube `textureDimensions` proof now accepts a
nonnegative literal or module-constant mip level when the entry point does not
also sample that cube. Core, Naga and HAL retain matching restrictions for
dynamic/negative levels, explicit-level-plus-sampling, helper use and unrelated
image operations. HAL supplies the selected view's face size at the requested
mip through `b13`; multi-mip resources still require the selected format query.

The strict public test on the RTX 3090 Ti explicitly requested/selected FL9_1
and queried mip 1 of a populated four-level 8x8 RGBA8 cube. All 64 pixels
matched `[128,128,0,255]` within one byte. The preexisting 8x8-to-4x4 cube view
transition still passed. Exact Core/Naga/HAL and full regression results are
in `FL9_1_PROGRESS.md`. This does not enable general texture queries or prove
historical-driver behavior.

## Latest format increment — RGBA8_SNORM GPU copies — 2026-11-09

The same checkpoint adds a selected-device `TEXTURE2D` query and backend-path
gate for same-format RGBA8_SNORM texture-to-texture copies. `COPY_SRC` no longer
implies CPU readback: the format remains absent from
`texture_to_buffer_copy_format_mask`, and the HAL explicitly rejects SNORM
texture-to-buffer readback. The synthetic capability test verifies that a
missing query or disabled path withholds `COPY_SRC`.

The strict public RTX 3090 Ti test explicitly requested/selected FL9_1, uploaded
`[127,-127,127,-127]` signed texels, copied RGBA8_SNORM to a second texture,
sampled it, and verified all 64 RGBA8 output pixels as `[255,0,255,0]`. The
selected format query returned `0x000213a0`. This confirms one modern driver
forced to FL9_1, not historical 9_1 hardware.

## Latest query increment — explicit in-pass timestamps — 2026-11-09

The selected-device timestamp/disjoint probe now gates
`TIMESTAMP_QUERY_INSIDE_PASSES` together with the existing timestamp features.
The private HAL records explicit render- and compute-pass timestamp writes as
ordered D3D11 `End` commands. The strict public test brackets a render draw,
adds explicit writes in an empty compute pass, resolves ten values and checks
them in execution order. The focused test passed 1/1 on the RTX 3090 Ti at
requested/actual FL9_1; full strict HAL passed 450/450 and public D3D11 passed
113/113. The optional timestamp feature remains absent if the selected-device
probe fails. Checkpoint: `af1f5d7ac` (`dx11: support FL9_1 timestamps inside
passes`). Next, consider constant-mip 3D `textureDimensions`; do not enable 3D
GPU texture copies, which the current 10Level9 audit records as unavailable.

## Latest shape-aware texture-query extension — 2026-11-09

The narrow fragment `textureNumLevels` emulation now has independent 2D, 3D
and cube downlevel limits. Core proves the selected entry-point shape for the
matching image dimension; Naga lowers only those proved cases; HAL retains and
checks the sampled view dimension and supplies its mip count. The 3D and cube
limits require selected-device sampled-format support but do not require mip
support, since a single-mip view reports one level. General texture queries,
helpers, control flow and additional resources remain unavailable.

Strict public execution on the physical RTX 3090 Ti at requested/actual FL9_1
verified 3D and cube views with visible mip counts 1 and 4, checking all 256
pixels. Full strict private HAL passed 450/450; strict public D3D11 passed
113/113; Core DX11 passed 110/110; DX12 Core passed 109/109; Naga passed
152/152; wgpu-types passed 108/108. Historical FL9_1 behavior remains
unverified. Checkpoint: `3ceba7844` (`dx11: support FL9_1 3D and cube
textureNumLevels`). Independent Core/HAL, rendering/resource and shader audits
found no confirmed unguarded texture-route mismatch. A useful small follow-up
is enabling timestamp writes inside passes under the existing positive device
query; constant-mip 3D dimensions and more selected-entry helper shapes are
also bounded shader candidates. Keep 3D GPU copies disabled: the current
10Level9 audit identifies that operation as unavailable.

## Latest public raster coverage — slope-scaled depth bias — 2026-11-09

The public D16 depth-attachment test now verifies slope-scaled depth bias with
a genuinely sloped triangle. Through public wgpu pipeline creation and GPU
readback, positive slope bias preserves red for all 64 pixels and negative
slope bias lets green pass for all 64. Existing constant-bias and nonzero
clamp-rejection coverage remains in the same test. The focused strict test
passed 1/1, and the complete strict public D3D11 suite passed 112/112 on the
physical RTX 3090 Ti at requested/actual FL9_1. The suite reported three
optional selected-device skips for unsupported vertex-input or mip features;
none were counted as test failures or hardware passes. Clippy, formatting and
`git diff --check` passed. This closes a public test gap for existing HAL
behavior, rather than adding a new capability. Checkpoint: `e53561240`.

Next, continue the shader/resource contract audit and implement only a narrow
gap whose Core, Naga and HAL proofs can be kept in agreement.

## Latest bounded shader increment — 3D fragment `textureLoad` helpers — 2026-11-09

The existing query-gated fragment 3D `textureLoad` emulation now supports
selected-entry reachable straight-line helpers. Core, the D3D11 HAL proof and
Naga HLSL generation enforce the same resource identity and helper-shape
restrictions: one directly referenced group-0 3D float image, no resource
handles passed through functions, no control flow or select, and no additional
image operations or bound resources. The operation is unchanged: Naga lowers
the load to point sampling at texel centers and the HAL supplies dimensions
through the reserved constant buffer.

The strict public test explicitly requested and selected FL9_1 on the RTX
3090 Ti. Direct loads and a nested helper load produced the expected output
across 320 pixels; an out-of-bounds coordinate and nonzero mip returned zero,
and Core rejected a branching helper before HAL. Naga passed 152/152, the
strict D3D11 Core fixture 120/120, strict private HAL 448/448, strict public
D3D11 112/112, and DX12 Core 108/108. Historical FL9_1 behavior remains
unverified.

The Core/HAL audit found no new mismatch in the inspected array, cube-view,
copy, and query routes; their unsupported shapes are already gated and tested.
The remaining mission is still broad. Public slope-scaled depth-bias coverage
is recorded above; continue the selected shader/resource contract audit. Do
not treat this helper increment as general
3D texture-load support. Checkpoint: `ffafa8b2d`.

## Bounded f32 matrix inverse support

The internal contract exposes separate `matrix_inverse_2x2` and
`matrix_inverse_3x3` limits. Naga's HLSL writer lowers f32 mat2 and mat3
inverse through scalar cofactors; Core tracks the shapes used by the selected
entry point and reachable helpers and rejects mat4/other shapes before HAL.
The general `matrix_inverse` limit stays false for the FL9_1 D3D11 projection.
WGSL has no inverse builtin, so the path applies to GLSL and other Naga IR
inputs. Strict RTX 3090 Ti readback at requested/actual FL9_1 exercises both
shapes in VS and PS; separate stage results matched `[64, 64, 64, 255]` in all
64 pixels, and mat4 was rejected as a Core validation error. The exact
commands and full verification record are in `FL9_1_PROGRESS.md`.

The broader mission remains incomplete. There are no confirmed mismatches in
the recent texture-format/Core/HAL audit, but systematic coverage remains
partial. Compute/UAV, ordinary texture arrays, 1D textures, MRT, occlusion
queries and native 3D copies/readback are FL9_1 boundaries. The 3D upload and
sample path is already present. Historical FL9_1 devices, Lumia/CoreWindow
runtime and physical device-removal/recovery remain unverified.

## Latest bounded shader increment — selected-entry helper `textureLoad` — 2026-11-09

Extended the restricted 2D fragment `textureLoad` proof through reachable
straight-line helper calls. Core validation, the D3D11 HAL proof and Naga HLSL
generation now agree on the selected entry point and its reachable call
closure. Helpers must reference the same image global directly and cannot
pass resource handles, use control flow or add other image operations. Naga
emits only the selected entry's reachable functions and used globals, so
unreachable helpers and sibling-entry-only resources do not constrain or
pollute the compiled shader. The 3D helper restriction remains unchanged.

Final-tree verification: Naga 149/149, Core D3D11 fixture 119/119, strict
private HAL 447/447, strict public D3D11 110/110, and DX12 Core 110/110. The
focused strict public readback passed 1/1 on the RTX 3090 Ti at explicitly
requested/actual FL9_1; all 256 pixels across direct/helper and in-bounds,
out-of-bounds and nonzero-mip cases matched. An initial regression run exposed
three stale tests that still expected helper loads to be rejected; those tests
now accept only the proven helper shape and continue rejecting control flow.
The independent selected-entry proof review found no concrete correctness
issue. Implementation checkpoint: `6e442d1d14a5873dd83344339de56b909dbe0306`
(`dx11: support selected-entry textureLoad helpers`).

Next, finish the public capability-to-HAL projection audit, beginning with a
data-driven matrix for query-gated format usages and transfer paths. Explicit
`Backends::DX11` selection already exposes the experimental FL9_1 projection;
keep DX11 out of default backend selection while auditing every Core-legal
operation for truthful implementation or tested rejection. A public textured
scene and HWND path also exist, but shader/resource breadth and historical
device verification remain substantial mission work.

## Latest bounded format increment — RGB5A1 and RGBA4 GPU copies — 2026-11-09

The opt-in FL9_1 path now supports same-format GPU texture copies from
`Rgb5a1Unorm` and `Rgba4Unorm` when the selected device reports
`TEXTURE2D` and the matching backend path is enabled. This does not require
`RENDER_TARGET`, create RTV-capable resources, or enable texture-to-buffer
readback. A strict public test on the RTX 3090 Ti at requested/actual FL9_1
verified padded uploads, partial copies to nonzero destination origins,
sampling, preserved destination pixels and all 64 readback pixels for each
format. Synthetic tests cover unsupported/unavailable queries and disabled
backend paths. Full strict public D3D11 passed 107/107, private HAL 446/446,
and DX12 Core 108/108; Clippy, format and diff checks passed. See
`FL9_1_PROGRESS.md` for exact commands and diagnostics. Implementation
checkpoint: `317cb9a38` (`dx11: support packed FL9_1 GPU texture copies`).

Continue the audit and prioritize useful bounded shader/resource additions.
The best shader candidate remains selected-entry helper support for the
existing direct-global `textureLoad` emulation, with coordinated Core, Naga
and HAL proofs; do not simply relax the helper rejection. A table-driven audit
of the opt-in public projection remains important. Do not implement FL9_1 3D
texture copies, which the 10Level9 contract excludes. Historical FL9_1
hardware, Lumia/CoreWindow runtime and physical device-removal recovery remain
unverified; D3D11 is explicit opt-in and remains absent from default backend
selection.

## Previous bounded format increment — public BC1/BC2/BC3 sRGB feature — 2026-11-09

The existing BC1/2/3 sampled-upload and same-format GPU-copy paths now include
the typed sRGB formats behind an independent native-only
`TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB` feature. Projection requires the FL9_1
guarantees, three separate selected-device format queries and backend paths.
Core accepts only these three sRGB formats with this feature; the unorm subset
and standard BC1–BC7 feature remain independent. Strict public execution on
the RTX 3090 Ti forced to FL9_1 sampled copied mid-gray BC blocks to approximately
linear `[59,57,59,255]` in all 64 target pixels, proving the sRGB decode path.
BC4–BC7, BC texture readback, mip chains and format reinterpretation remain
unavailable. The complete strict private HAL, Core and public regression runs
passed; see the latest `FL9_1_PROGRESS.md` entry for commands and diagnostics.
Implementation checkpoint: `3a1cb5527` (`dx11: expose query-gated BC1-3 sRGB formats`).

This is a useful format increment, not completion of FL9_1 support. Continue
with a safe bounded shader-composition or resource capability. Do not implement
3D texture copies: the existing 10Level9 audit documents that copy path as
unavailable at FL9_1. Public CoreWindow presentation and historical-device
behavior remain external verification requirements.

## Previous bounded format increment — public BC1/BC2/BC3 unorm feature — 2026-11-09

Added the native-only `TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM` feature for the
existing BC1/2/3 unorm sampled-upload path. The selected-device projection
requires every format's queried sampled-upload capability and backend path;
Core accepts only those three unorm formats through the narrow feature. At
that checkpoint the standard BC feature remained withheld, BC sRGB and BC4–BC7
remained rejected, and the FL9_x texture-to-buffer readback mask remained
clear. A strict public forced-FL9_1 test on the RTX 3090 Ti uploaded and copied
three differently
colored compressed textures, sampled each in one shader and verified 64 white
pixels; it also confirmed BC4 rejection. Synthetic capability and Core feature
gate tests cover partial/failed queries and the exact supported format set.
The full mission remains partially complete; proceed with another bounded
public-contract or graphics gap after recording complete regressions.

## Latest verified public rendering increment — depth bias — 2026-11-09 — `27b6901bc97706617054f1ef2ff78af4eec630eb`

The public D16 depth test now creates and executes zero-, positive- and
negative-constant-bias pipelines through wgpu Core and the D3D11 HAL. A full
8×8 readback verifies that positive bias fails the existing depth test and
negative bias passes; the same strict test rejects nonzero bias clamp before
native execution. The focused GPU test passed 1/1 and the full strict public
D3D11 suite passed 109/109 on RTX 3090 Ti at requested/actual FL9_1. This
verifies the existing constant-bias implementation across the public boundary;
it does not add a new capability. The capability-to-HAL audit remains open.

## Latest verified coverage increment — indexed LineStrip restart bundles — 2026-11-09 — `42cecc33690e20c789e1aa59bdb0dcf28283fa86`

Added a strict public regression that executes consecutive indexed LineStrip
render bundles at forced FL9_1. The first bundle uses Uint16 indices and draws
green restart-separated segments; the second uses Uint32 indices and draws red
segments in a separate viewport half. Readback confirms all four segments,
their restart gaps, no unexpected pixels, and retained vertex-buffer lifetime
after the caller drops its handle. The targeted case passed on the physical
RTX 3090 Ti, and the full strict public D3D11 suite passed 109/109. This closes
a render-bundle coverage gap; it does not add a new rendering capability.
The broad Core/HAL contract audit, historical-device compatibility and Lumia/
CoreWindow runtime verification remain open.

## Latest verified increment — depth-only MSAA and public-contract audits — 2026-11-09 — `dbfb26b41`

The HAL now accepts query-supported multisample pipelines with depth-only
attachments, while still rejecting attachment-free multisample pipelines and
requiring the existing RGBA8 support for color targets. A strict public GPU
regression ran a targetless four-sample depth prepass for D16, Depth24Plus and
D24S8, followed by a color+depth draw and resolve. The farther draw was
rejected in all three cases and all 64 resolved pixels stayed blue on the RTX
3090 Ti at explicitly requested/actual FL9_1.

The final strict public suite passed 108/108 and the private HAL suite 443/443.
The Core zero-adapter isolation test passed 1/1; DX12 Core passed 106/106.
Clippy, formatting and `git diff --check` passed, with existing lint warnings.
Independent shader/pipeline, resource/transfer, render/attachment,
surface/query, buffer/command, capability, panic/error and unsafe-contract
reviews found no additional confirmed Core/HAL mismatch or public-path safety
issue. The broad operation-by-operation audit
is still open; historical FL9_1 devices, Lumia/CoreWindow runtime and actual
device removal/recovery remain unverified. The changes are recorded in
checkpoint `dbfb26b41`. Continue with the next independent audit slice and
implement only a concrete, tested mismatch or a useful bounded feature gap.

## Latest verified increment — public buffer lifetime before submit — 2026-11-09 — `67e8fcde45592a8a8ccc9c7759e9a0569313abd2`

The strict public regression now records source-to-destination and
destination-to-readback copies, drops the public source and destination handles
before submission, then verifies the complete 64-byte result and untouched
sentinels. It closes a public lifecycle coverage gap; the Core/HAL audit found
no confirmed implementation defect. The focused GPU test passed 1/1 and the
strict public D3D11 suite passed 105/105 on the RTX 3090 Ti at requested/actual
FL9_1. Attachment load/store/discard, transient, depth/stencil and resolve
paths were separately audited with no reproducible mismatch. See
`FL9_1_PROGRESS.md` for exact commands and caveats.

## Latest verified increment — `vertex_index` with TriangleStrip — 2026-11-09 — `5552a8fa57ca1c34c89dcec70524150d15b2674f`

The selected FL9_1 projection and defensive HAL gate now permit the emulated
`vertex_index` subset for TriangleStrip. Direct strip batches preserve the source
identity sequence. Indexed Uint16/Uint32 strips resolve signed base vertices and
restart runs, then expand each run to winding-preserving triangle-list identity
values; Window and Gather remaps retain the corresponding IDs, and temporary
streams are restored after replay. Core fixture validation now accepts this
topology only for the opt-in D3D11 projection.

A strict public GPU test passed on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) with requested/actual FL9_1. It verified direct
`first_vertex=100000`, Uint16 and Uint32 restart, positive/negative base vertices
and wide Gather remapping; six output regions were populated (2,942 red pixels).
The private HAL suite passed 443/443, strict public D3D11 passed 104/104, the
controlled Core fixture passed 1/1, and the DX12 Core regression passed 106/106.
See `FL9_1_PROGRESS.md` for exact commands and diagnostics. Historical FL9_1
drivers remain unverified.

## Latest verified increment — indexed byte extents and padded volume upload — 2026-11-09 — `90026ebf3`

Indexed rebase, gather, point and strip planners now accept a two-byte copied
vertex extent without adding a four-byte size-alignment assumption. Offset,
stride, nonzero extent, bounds and checked arithmetic validation remain.
Synthetic planner coverage exercises native Uint16, Window, Gather, indexed
lines and strips. The RTX 3090 Ti does not report optional sub-four-byte IA
formats at forced FL9_1, so positive hardware execution of that format path
remains unavailable.

A strict private HAL test uploads an offset and padded 2×2×2 RGBA8 region into
a 4×4×4 volume at a nonzero origin, samples all four slices to an 8×8 target,
and checks eight written voxels, 56 initialized untouched voxels and padding.
It passed on the RTX 3090 Ti at requested/actual FL9_1. The HAL suite passed
441/441 and strict public D3D11 passed 103/103. See `FL9_1_PROGRESS.md` for
commands, Clippy warnings and exact output. 3D copy and readback remain
unavailable under the FL9_1 contract.

## Latest verified implementation — `vertex_index` with LineStrip — 2026-11-08 — `b93747fb4`

The working tree adds LineStrip to the existing emulated `vertex_index`
subset. Direct draws preserve `first_vertex` through native strip batching;
indexed Uint16/Uint32 draws split restart runs after signed base-vertex
resolution and attach corresponding IDs to compact Window or wide-pair Gather
segments. Temporary and hidden IA streams are restored after replay. A strict
RTX 3090 Ti test passes direct batching, both index widths, restart, positive
and negative base vertex, and a wide-index Gather pair at requested/actual
FL9_1. The private HAL suite passed 438/438, the strict public D3D11 target
passed 103/103, the Core fixture suite passed 115/115, and the DX12 Core suite
passed 106/106. The following historical checkpoint records the LineStrip-only
state that preceded this TriangleStrip increment.

## Previous verified increment — `vertex_index` with LineList — 2026-11-08 — `0835c23cf`

The selected FL9_1 projection and defensive HAL gate now permit emulated
`vertex_index` for LineList in addition to TriangleList and PointList. Direct
draw replay applies `first_vertex` once to user streams and the hidden identity
stream. Indexed Uint16 and Uint32 lists preserve line pairs, resolve effective
IDs after `base_vertex`, and use Window or Gather remapping when the native
FL9_1 index range is exceeded. Replay restores regular and hidden IA vertex
bindings after temporary rebasing/gathering. TriangleStrip `vertex_index` was
still rejected at this earlier checkpoint; the latest increment above adds it.

Strict public GPU tests on the RTX 3090 Ti at requested/actual FL9_1 verified
direct, Uint16 and Uint32 LineList output, plus a direct nonzero-`first_vertex`
draw and an indexed Uint32 Gather draw with nonempty vertex data followed by
distinct triangle output in the same pass. The full private HAL suite passed
437/437, the strict public D3D11 target passed 102/102, and the Core fixture
suite passed 115/115. Formatting and `git diff --check` passed. One stale Core
fixture feature expectation was updated to account for the already query-gated
timestamp features being present on this RTX. Historical FL9_1 behavior is
not established by forced-level execution on this modern device.

The adjacent D16-to-buffer experiment was withdrawn. Core intentionally
excludes depth formats from its texture-to-buffer copy mask, and forced-FL9_1
native probes rejected every tested R16-family CPU-readable staging texture.
That is not a currently exposed public-valid request; a future implementation
would need a separate depth-sampling/color-pack emulation, not a permissive
copy flag. See `FL9_1_PROGRESS.md` for the hardware evidence.

## Latest verified regression — ordered uniform GPU updates — 2026-11-08

The public path now tests two GPU copies into one uniform buffer interleaved
with draws in a single command buffer. The first draw reads red into the left
half, the second reads green into the right half, and the full 8×8 readback
matched all 64 pixels. This checks replay-time buffer contents and uniform
materialization ordering. It does not add a new capability. See
`FL9_1_PROGRESS.md` for strict hardware and regression results.

## Latest verified increment — cube fragment `textureDimensions` — 2026-11-08

The opt-in projection now has an independent `texture_dimensions_cube_float_fragment`
limit gated by the selected FL9_1 device's cube-format sampling query. Core,
Naga and HAL accept a direct no-level face-size query on one group-0
`texture_cube<f32>`, converted from `vec2u` to `vec2f`; helpers, explicit query
levels, unrelated image operations, extra resources and runtime integer work
remain rejected. Naga uses the shared logical-to-native binding map. FXC
accepted the generated shader under `ps_4_0_level_9_1`; HAL supplies the bound
view's width and height in the existing `b13` metadata constant because the
profile rejects `GetDimensions`. The strict public GPU test selected the RTX
3090 Ti at requested/actual FL9_1, initialized and switched between 8x8 and
4x4 cube views, and matched `[255,255,0,255]` and `[128,128,0,255]` across all
64 output pixels. Full suites passed: Naga 148/148, Core 107/107, private HAL
436/436, public strict D3D11 99/99, and `wgpu-types` 108/108 plus 14 doc tests.
The public suite included the default zero-adapter isolation test; optional
device-specific subcases logged skips. Broad contract/CTS coverage, historical
FL9_1 drivers, Lumia/CoreWindow execution and physical device-removal recovery
remain open. See `FL9_1_PROGRESS.md` for exact commands and warnings.

## Latest verified increment — 3D fragment `textureDimensions` plus implicit sampling — 2026-11-08

The separate `texture_dimensions_3d_float_fragment` proof now permits one
level-less implicit sample of the same 3D float texture through one
non-comparison sampler. Core, Naga and HAL require matching direct image
identity, a `vec3f` coordinate, no helper functions, no explicit LOD and no
extra bound resources or image operations. The existing selected-device RGBA8
volume-sampling query remains the gate; the feature-level identity is still
forced to FL9_1. Naga retains the shared binding map, while HAL supplies the
selected view's dimensions through `b13` and normal bind-group replay assigns
the texture and sampler registers. Strict RTX 3090 Ti readback switched from a
4x2x8 volume `[200,160,80,255]` to a 2x4x1 volume `[80,120,160,255]`; expected
outputs `[200,80,40,255]` and `[40,120,10,255]` matched all 64 pixels. FXC
accepted `ps_4_0_level_9_1`. Full suites passed: Naga 147/147, Core 106/106,
private HAL 434/434 and public strict D3D11 98/98. A DX12 HAL check and affected
Clippy/format checks passed; existing warnings are noted in the progress
journal. General texture queries remain disabled, and historical FL9_1 drivers,
Lumia/CoreWindow runtime and physical device-loss recovery remain unverified.
Implementation and documentation checkpoint: `0fd033d33`.

## Previous verified increment — narrow 3D fragment `textureLoad` — 2026-11-08

Added an independent `texture_load_3d_float_fragment` limit. At FL9_1 it is
projected only when the selected device positively reports the implemented
RGBA8 3D sampled-upload path. The Core/Naga/HAL path accepts one direct,
single-mip `textureLoad` from a non-array `texture_3d<f32>` fragment resource;
the writer emulates it with point sampling at texel centers, dimensions in
`b13`, and internal sampler `s15`. The RTX 3090 Ti at requested/actual FL9_1
verified two distinct volume texels and zero for out-of-bounds/nonzero-mip
loads across 256 output pixels. FXC accepted the generated shader under
`ps_4_0_level_9_1`. One first GPU run exposed that replay supplied depth 1;
the SetTexelLoad payload now supplies the actual volume depth. The full
integrated private HAL and public strict suites subsequently passed 434/434 and
98/98. General `texture_load` stays false; historical FL9_1 hardware and
Lumia/CoreWindow remain unverified.

## Previous verified increment — co-use 2D texture dimensions and implicit sampling — 2026-11-08

Extended the narrow group-0 fragment `textureDimensions` proof to permit one
level-less implicit sample of the same 2D float texture using one compatible
non-comparison sampler. Naga keeps using the shared binding map and emits the
host-provided `b13` dimensions plus ordinary implicit `Sample`. Core continues
to reject helper/control-flow use, `select`, additional resources and other
image operations. On the RTX 3090 Ti at explicitly requested/selected FL9_1, a
public draw switched between 4x2 and 8x4 textures with distinct colors and
verified all 64 pixels, so both query and sample values affected output. Naga
passed 145/145, Core 106/106, and private HAL 430/430. The focused strict public
test passed 1/1. The complete strict public target was subsequently rerun and
passed 97/97. General texture queries and nonzero-mip query-plus-sample remain
unsupported. Historical drivers and Lumia/CoreWindow remain unverified.

## Latest verified increment — FL9_1 base-clamp sampling — 2026-11-08

Added a narrow selected-entry `textureSampleBaseClampToEdge` path using
host-supplied single-mip view dimensions and implicit `Sample`. The proof
accepts direct use or one reachable straight-line helper called once with a
`vec2f` argument and returning `vec4f`; the entry may compute that argument.
Naga, Core and HAL reject nested/repeated calls, control flow, extra selected
resources and other image operations. FXC accepted the helper at
`ps_4_0_level_9_1`, and strict RTX public readback matched 64/64 pixels. The
general downlevel limit remains false. Core's finite repeat limit prevents
using Repeat/MirrorRepeat descriptors in the public test, so behavior for
those sampler modes remains unverified. Historical hardware/runtime gaps stay
open.

## Latest verified increment — 3D fragment `textureDimensions` — 2026-11-08

Added an independent narrow limit for a direct no-level fragment
`textureDimensions(texture_3d<f32>)` query converted from `vec3u` to `vec3f`.
It is gated by the selected device's existing RGBA8 3D sampled-format query.
Strict forced-FL9_1 readback on the RTX 3090 Ti verified 64/64 pixels while
switching between 4x2x8 and depth-one 4x2x1 views. The full private HAL suite
passed 429/429 and the strict public D3D11 suite passed 95/95, including the
normal-enumeration isolation test. 2D/3D limits remain independent and general
texture queries are still disabled. Continue the broad Core/HAL contract audit
and prioritize the next bounded shader or resource expansion from the current
independent gap reviews; historical-device verification remains open.

## Latest verified increment — constant-mip `textureDimensions` — 2026-11-08

The narrow fragment query now accepts a nonnegative literal or module-constant
mip level for one group-0 `texture_2d<f32>`, relative to the selected view's
base mip. Strict FL9_1 readback on the RTX 3090 Ti verified 64/64 pixels across
views with different base mips. Runtime/override/negative levels and general
texture queries remain disabled. See `FL9_1_PROGRESS.md` for commands and
results. The next candidate is an audit of whether the same proof can cover
supported 3D views; broad contract validation and historical-device testing
remain open.

## Latest verified increment — narrow fragment `textureNumLevels` — 2026-11-08

The opt-in FL9_1 projection now exposes one direct fragment
`textureNumLevels` query on a group-0, non-array `texture_2d<f32>` when its
result is directly converted to `f32`. Core's selected-entry proof rejects
helpers, control flow, other bound resources or image operations, and
unrelated integer semantics. Naga uses a separate proof option from the
existing texel-load emulation. HAL supplies the bound view's mip count through
the reserved host constant. The RTX 3090 Ti explicitly requested and selected
FL9_1 and verified all 64 pixels for one- and three-level views. General
`texture_query` remains disabled. See `FL9_1_PROGRESS.md` for commands and
test counts. Remaining gates include full public contract auditing, historical
FL9_1 hardware, Lumia/CoreWindow execution and real device-loss recovery.

## Latest verified increment — square and rectangular GLSL outer products

Naga's HLSL writer now lowers validated f32 `outerProduct` expressions, and
the GLSL overload resolver returns the correct dimensions for rectangular
products. DX11 enables the matching downlevel limit while retaining the
selected-entry/helper validation gate. A strict public test forced FL9_1 on
the RTX 3090 Ti and verified 64/64 pixels for both square and rectangular
operations. The full strict public suite passed 94/94; three optional
device-capability subcases logged skips. Naga's GLSL-enabled library tests
passed 142/142. DX12 continues to withhold the capability. The broad public
contract audit, historical FL9_1 driver coverage, Lumia/CoreWindow runtime and
device-removal recovery remain open.

## Latest verified increment — cube readback and narrow fragment texture loads

Cube-to-buffer copies are now query-gated for the supported cube shape and
copy format, using per-face staging readback. A strict HAL test checked all six
faces and padded output; the public cube test verified all 384 face texels.
The opt-in FL9_1 public projection also has a separate narrow limit for a
single direct fragment `textureLoad` from one single-mip `texture_2d<f32>`;
the shader path uses internal point sampling and dimensions, with strict
in-bounds and out-of-bounds readback. The physical RTX 3090 Ti passed at
requested/actual FL9_1. This remains deliberately short of general texture
load support. Historical FL9_1 compatibility and the broader public contract
audit remain open. Strict public readback checked 64 in-bounds, 64
out-of-bounds and 64 nonzero-mip pixels. Implementation checkpoint:
`061e2acca` (`dx11: add narrow FL9_1 texture loads and cube readback`).

The 2026-11-08 contract audit found and fixed one public-valid depth-view
mismatch: Core permits the default view of a query-enabled texture with both
`RENDER_ATTACHMENT | TEXTURE_BINDING` usage, but HAL had rejected the inherited
combined view usage. The HAL now retains both DSV and SRV roles when requested.
Strict FL9_1 public readback passed for D16 and Depth24Plus using the same
default view sequentially for depth writes and comparison sampling; D24S8
default attachment views and its required depth-only sample view also passed.
Implementation and GPU test checkpoint: `f3926cf9bf1b5ef61101b0efbc1166b45aeeb6e5`.
The overall contract audit remains open.

The 2026-11-08 independent follow-up audits found no new actionable public
contract gap in render-bundle replay, selected-entry shader validation,
texture format/usage gates, or unsupported HAL panic paths. The suggested 3D
texture-copy expansion was withdrawn: Microsoft's 10Level9 context contract
does not permit 3D GPU texture copies at FL9_1, and the current public/HAL
gates correctly reject them. Continue auditing other ordinary public-valid
operations; do not add this unsupported copy path. The timestamp-query
checkpoint is committed as `ede5c299f`.

The public buffer-map coverage now also verifies a nonzero mapped slice after
a submitted GPU buffer copy and checks that bytes outside the copied range
remain unchanged. It passed under strict mode at requested/actual FL9_1 on the
RTX 3090 Ti. The test-only checkpoint is `42414a6f9`; the complete strict
public target passed 90/90 with three optional feature subcases explicitly
skipped. This adds regression evidence without widening the exposed contract.

Follow-up coverage now exercises asynchronous `MapMode::Write` on a nonzero
slice after a prior GPU use, then verifies subsequent full and partial GPU
copies observe the update while preserving source and destination sentinels.
The focused test passed 1/1 and the complete strict public target passed
92/92 on the RTX 3090 Ti at requested/actual FL9_1. The checkpoint is
`fe0cc897a`; details and commands are recorded in `FL9_1_PROGRESS.md`.

The next candidate sweep also found no expansion to make: FL9_1 rejects the
read-only DSV creation probe on the RTX 3090 Ti (`CreateDepthStencilView` with
read-only flags returned `E_INVALIDARG`), so `READ_ONLY_DEPTH_STENCIL` remains
unexposed. A same-pass path would additionally need HAL render-pass support for
Core's combined `DEPTH_STENCIL_READ | RESOURCE` use and positive GPU readback
on a device that accepts the DSV. BC1/2/3 texture-to-buffer readback remains
blocked by the 10Level9 SRV-only readback restriction and by wgpu's all-family
BC feature gate; 3D render-target views are also excluded by the FL9_1 device
contract. Keep these cases rejected and continue the broader public-valid
contract audit. References: [10Level9 device methods](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device),
[10Level9 context methods](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context),
[CopySubresourceRegion](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-copysubresourceregion).

Latest verified increment: timestamp query sets, command-encoder writes and
pass-boundary writes are supported only when a selected device positively
executes timestamp and disjoint queries and reports a nonzero frequency. The
public projection still omits occlusion queries, pipeline statistics and
explicit `write_timestamp` calls inside passes. Resolving timestamps waits
for D3D11 `GetData` and can stall the CPU. On the RTX 3090 Ti at explicitly
requested/selected FL9_1, six query values covering encoder writes, render-pass
boundaries and an empty compute-pass boundary pair were read back in
nondecreasing order; the reported period was 1 ns/tick. Strict Rust
1.93.1 runs passed private D3D11 HAL 416/416, public D3D11 89/89, and DX12 core
101/101; three optional public device-specific subcases skipped. Exact commands
and warnings are recorded in `FL9_1_PROGRESS.md`. Public D3D11 remains explicit
opt-in and excluded from default backend selection.

The mission remains partially complete. Continue the core/HAL contract audit
and prioritize remaining feasible public FL9_1 work based on independent
hardware, shader and validation reviews. No historical FL9_1 driver, Lumia
ARM32 runtime or physical device-removal recovery has been verified. The
backend is restricted and experimental, not a complete WebGPU implementation;
applications must opt in explicitly and stay within the documented contract.
The latest focused shader and texture/view audits found no further confirmed
graphics mismatch in those scanned paths. A valid padded `vec2<f32>` uniform
array wrapper is now supported and verified; a plain uniform
`array<vec2f, N>` remains invalid WGSL. A `StencilOnly` render attachment is
invalid under core's renderable-view contract; stencil sampling would need an
unimplemented uint texture-load path and a positive selected format query, so
it remains deferred rather than exposed through the current DSV.

Latest shader increment: `instance_index` uses a host-updated b13 value, and
`vertex_index` uses a host-generated float IA stream for TriangleList,
PointList, LineList and LineStrip pipelines. `DownlevelLimits::vertex_index_topology_mask` communicates
that subset to core, which rejects other topologies before HAL pipeline
creation. Core and HAL acceptance/rejection were exercised in the strict RTX
fixture and public error-scope tests. Public readback verifies indexed and
nonindexed triangle IDs, negative base vertex, wide indices, `first_vertex=100000`
and post-remap state; point readback verifies direct and repeated indexed IDs.
Historical-driver compatibility remains open. See FL9_1_PROGRESS.md.
Latest format increment: the internal FL9_1 sampled/upload mip path now also
covers R8G8B8A8_SNORM. Microsoft lists its `TEXTURE2D`, `SHADER_SAMPLE` and
`MIP` guarantees at FL9_1; the selected device's `TEXTURE2D | MIP` query and
the backend path are still required. Strict RTX 3090 Ti execution at requested
and actual FL9_1 sampled mip 1 from a three-level upload and matched all 16
pixels. R8G8_SNORM coverage remains intact. Neither path adds render-target,
copy-source or readback support. See `FL9_1_PROGRESS.md`; the broader contract
audit, historical-driver compatibility and Lumia runtime verification remain
open.

Latest integration-safety increment: `Backends::default()` excludes the
experimental D3D11 backend, keeping public adapter exposure explicitly opted
in while preserving explicit DX11 selection and prior backend priority. The
strict public suite passed 77/77 on the RTX 3090 Ti at requested/actual FL9_1;
see `FL9_1_PROGRESS.md` for exact commands. The full capability and platform
completion audit remains open.

Latest increment: the optional `FullNonPow2TextureSupport` result now controls
the internal `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` projection for the
implemented D2 path, in addition to per-format MIP queries. The private HAL
accepts the same query-gated 2D mip shapes and sampled views. On the RTX 3090 Ti
explicitly forced to FL9_1, a public test uploaded a 6×10 four-mip RGBA8 chain
through buffer-to-texture copies and sampled mip 2; all 64 readback pixels
matched. Negative and unavailable 9_x queries remain conservative in synthetic
tests. This does not verify restrictive historical hardware, other formats or
all usages. See the latest `FL9_1_PROGRESS.md` entry; the overall completion
mission remains partial.

Latest texture-format increment: private BC1/BC2/BC3_UNORM now support queried
single-mip sampled uploads, aligned partial block updates and same-format GPU
texture copies. Strict FL9_1 readback matched 256/256 output pixels for each
format on the RTX 3090 Ti. The aggregate BC feature remains withheld because
BC4–BC7 are not implemented. The strict HAL suite passed 401/401, the public
D3D11 suite 73/73 (with one optional Float16x2 subcase skipped), the core
adapter-selection regression 1/1 and DX12 core 94/94. See the latest progress
entry for commands and exact driver details. Continue the public core/HAL
contract audit; historical FL9_1 and Lumia/CoreWindow runtime remain open.

The latest contract correction separates statically bounded from
runtime-dependent shader loops. Static vertex and fragment loops still render
on the forced-FL9_1 RTX path; core rejects dynamic loop bounds in the selected
entry point and reachable helpers before HAL compilation. The latest strict
public suite passed 71/71, private HAL suite 395/395, controlled core fixture
103/103, core library 95/95, and `wgpu-types` 94/94; DX12 core/HAL regressions
passed 94/94 and 4/4. Exact commands and compiler/hardware evidence are in
`FL9_1_PROGRESS.md`.

A subsequent strict public test also verified both front-face windings and
back-face culling with a 64-pixel readback. It passed 1/1 on the RTX 3090 Ti at
requested and actual FL9_1; the complete strict public suite was rerun with the
new test and passed 71/71.

The preceding public increment exercises same-format copies in both
directions between ordinary 2D textures and cube subresources, alongside
cube-face copies across mip levels. A copied cube face was sampled, and a cube
face copied back to 2D was read back as 64 matching pixels. Cube
texture-to-buffer readback remains blocked by the one-layer downlevel source
limit. Its verified public integration checkpoint is `7fb6cf3c9`.

The objective is a usable explicit D3D11 backend through the public native wgpu
API, with truthful downlevel capabilities, tested textured depth rendering and
Windows presentation. The opt-in public adapter exposes physical FL9_1 devices
for offscreen rendering and a constrained HWND surface. Surface use is
owner-thread-only; hidden lifecycle tests and a bounded visible-window example
have run at forced FL9_1. A captured client-area frame was visually inspected:
the rotating cube showed distinct red and green textured faces over the clear
background. This plan does not claim WebGPU conformance.

Latest increment: the query-gated FL9_1 sampled-depth comparison path now
reaches the opt-in public API, but only when the selected device positively
reports `D3D11_FEATURE_D3D9_SHADOW_SUPPORT`. Its downlevel limits allow
`LessEqual`, equal min/mag filters and comparison anisotropy 1; other
comparison states remain rejected. The public path now reports and executes
`Depth24Plus` as well as D16 and D24S8, with native D24S8 storage keeping the
stencil aspect hidden. Strict FL9_1 readback checks 32 failed and 32 passing
comparisons in each public depth format. A 4x depth-tested render also
verified D16, Depth24Plus and D24S8 against resolved RGBA8 output. This does
not enable general comparison sampler support or close the overall core/HAL
audit; see the latest entry in `FL9_1_PROGRESS.md`.

The optional shadow query also enables the D3D9-specific border-addressing
exception for comparison samplers. The public projection requires comparison
use for border mode, and the strict FL9_1 test verifies 64/64 out-of-range
pixels return opaque black. Ordinary border and clamp-to-zero samplers remain
rejected. This is query-selected driver behavior, not a universal FL9_1
guarantee.

The 2026-11-07 verification checkpoint reran the strict private HAL and public
D3D11 suites, the controlled core fixture, adapter selection and DX12
regression; results and exact commands are recorded in
`FL9_1_PROGRESS.md`. The public offscreen example also passed through wgpu on
the physical adapter. The tested public subset is usable, but the contract
audit is still in progress. An optional-format probe found no selected-device
float-texture path to add at forced FL9_1. ARM32 UWP cross-compilation passed,
but Lumia runtime execution remains unverified after Device Portal timed out.

Checkpoint `72e10f78f` added selected-device-query-gated 4x D16/D24S8 depth
attachments to matching 4x RGBA8 render/resolve passes. The strict public
depth test, all 377 private HAL tests and all 63 public D3D11 tests passed on
the forced-FL9_1 RTX 3090 Ti; DX12 core passed 92 tests. The HAL also
cross-compiled for ARM32 UWP. This is one more verified slice, not closure of
the broad API contract audit or proof of historical/Lumia runtime behavior.
The controlled DX11 core fixture was rerun after that capability change; its
stale depth-format flags assertion now compares against HAL's selected-device
query and confirms depth resolve remains absent. The strict fixture passed
8/8.

An opt-in D3D11 debug-layer run now covers the focused private indexed-render
and readback path on the RTX at forced FL9_1: it reported zero error or
corruption diagnostics and no live D3D11 resources after cleanup. This does
not replace the outstanding broad public contract audit or historical-driver
and Lumia runtime verification; exact evidence is in `FL9_1_PROGRESS.md`.

The public contract audit now has a strict regression test proving that
`PIPELINE_CACHE` and `EXPERIMENTAL_RAY_QUERY` remain unadvertised and are
rejected during device request because their HAL resource types are
unavailable. This verifies two specific gates; it does not close the broader
core/HAL audit.

The public `SurfaceTexture` lifecycle now shares presentation/discard state
across clones. A strict HWND test first reproduced premature discard when one
clone was dropped, then verified that a worker-thread alias drop before
presentation and an owner-thread alias drop after presentation preserve the
remaining frame; final unpresented drop allows reacquisition. The broader
surface and device-loss audit remains open.

The strict HWND smoke test now configures the surface through the public
`get_default_config` result. Core resolves `Auto` color-space/alpha modes and
clamps its generic latency hint to the D3D11 one-frame capability before HAL;
the default public path configures and presents successfully. A second
lifecycle case drops the surface before an acquired frame; the surviving frame
can then be dropped safely without looking up a removed surface ID. Presenting
such a retained frame reports `Surface is invalid` instead of looking up a
removed ID; a strict regression verifies the stale present does not affect a
second active HWND surface and separately covers both cleanup paths.

The HWND lifecycle audit found and fixed a consumed-frame leak when public
`Queue::present` is called from a non-owner thread. Core now discards that
acquired frame before returning the validation error; the strict public test
confirms the surface can acquire and present another frame afterward. The
implementation is checkpointed as `8d6c8f93e12c766a5f8fb413f2abf781c4b55408`.
The broader surface and device-loss audit remains open.

## Ordered work and gates

### Verified hardware boundary — occlusion queries

Microsoft's 10Level9 feature table marks occlusion queries unavailable at
FL9_1 and available from FL9_2. Keep `DownlevelFlags::OCCLUSION_QUERY` absent
and reject public query-set creation before HAL. The D3D11 event query used
internally by the fence is a synchronization primitive and does not change
this result. Timestamp and pipeline-statistics queries remain separately
unimplemented/unverified. The strict public query-validation test confirms
core rejects all three query-set types for the selected FL9_1 projection.
Reference: [Direct3D feature levels](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro).

### Verified compiler boundary — fragment sample-mask emulation

Do not emulate fragment `@builtin(sample_mask)` by discarding only when the
whole mask is zero. Even for a single-sample target, WebGPU uses bit 0, so a
mask with only higher bits set must discard. The exact low-bit test needs
bitwise extraction, which FXC rejects for `ps_4_0_level_9_1` with X3535.
The existing selected-entry core gate therefore remains correct. Revisit only
if a level-9_1-expressible exact parity operation is found and tested; the
RTX's modern driver does not relax the shader-profile restriction.

### Verified hardware boundary — ordinary 2D texture arrays

Ordinary texture arrays are unavailable at FL9_1: Microsoft's feature-level
capability table lists texture resource arrays as unsupported for all 9.x
levels. The supported six-face cube resource is a distinct shape and does not
imply general array-view support. The public adapter reports
`TEXTURE_2D_ARRAYS` as absent, and strict public coverage verifies that a
two-layer 2D texture is rejected before HAL. No array implementation is
planned for the FL9_1 path.

[Microsoft feature-level capability table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro).

### Verified resource boundary — 1D textures

Although generic wgpu limits retain a 2048-texel FL9_1 `max_texture_dimension_1d`,
Microsoft's 10Level9 device reference explicitly marks
`ID3D11Device::CreateTexture1D` unsupported for all 9.x levels. The D3D11
projection therefore keeps `DownlevelLimits::texture_1d` false. Do not treat
the generic dimension limit as a resource-creation guarantee. The strict
public offscreen regression asserts the reported gate and that core rejects a
D1 texture before HAL.

### Verified boundary — mutable format views unavailable at FL9_1

Compatible RGBA8/BGRA8 UNORM-to-sRGB view lists remain rejected by core and
`VIEW_FORMATS` stays absent. The 10Level9 FL9_1 format table lists the needed
typeless resource formats as unavailable, and a forced-FL9_1 native creation
probe returned `E_INVALIDARG` on the RTX 3090 Ti. A strict public validation
test covers both pairs. See `FL9_1_PROGRESS.md`; this does not block standalone
typed sRGB textures or ordinary same-format views.

### Verified indexed-fetch case — negative indices with zero vertex stride

The earlier open question is closed for the exposed shader subset. WebGPU's
vertex-fetch algorithm computes each attribute offset as the bound offset plus
the resolved vertex index times `arrayStride`; with a zero stride, a negative
base-adjusted index still addresses the same in-range attribute element. The
strict `fl91_indexed_points_order_instances_and_lifetime` test visibly renders
Uint16 and Uint32 indexed points with negative resolved indices and only
zero-stride vertex-rate streams. A strict public wgpu test repeats the case
through core validation and command recording and checks both full readbacks.
`vertex_index` remains unavailable, and these tests do not claim behavior for
historical FL9_1 drivers. See the current progress entry for the commands and
hardware result.

Continue the core/HAL audit using independently reproduced public-valid
operations. Prioritize validation boundaries and lifetime/device-loss paths,
and keep optional format reports query-gated.

The current resource audit includes a narrow query-gated RGBA8 3D
sampled/upload path. Mip chains are enabled only when the selected device's
format query positively reports mip support; 3D views must include the
lowest mip as required by FL9_1. It does not enable 3D texture copies,
readback, render attachments or storage. Preserve those negative limits while
validating the public copy and shader paths.

The optional-format audit also found that a synthetic `BLENDABLE`-without-
`RENDER_TARGET` query could project blendability without attachment support.
The projection now requires the format's implemented color-attachment path, and
a synthetic RGB565 case covers the missing-render-target query. Continue
auditing cross-capability implications in optional format reporting.

Three-dimensional texture copies are not a deferred FL9_1 backend path.
Microsoft's 10Level9 context contract permits only 2D textures and buffers to
be copied within GPU-accessible memory, forbids 3D GPU-to-CPU copies, and
forbids copies of mipmapped volume textures. The public projection therefore
keeps `texture_3d_texture_copies` false and excludes `COPY_SRC` from the 3D
usage mask. A strict public validation test rejects the unsupported 3D
`COPY_SRC` descriptor before HAL. See the 10Level9
[context restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context).

The 4x RGBA8 format report also exposed a core/HAL usage mismatch: core accepted
multisampled sampled/copy descriptors from ordinary per-format usage flags,
while the D3D11 path creates only render-target resources. A dedicated
`DownlevelLimits::multisampled_texture_usage_mask` now lets core reject those
combinations while retaining query-gated render/resolve. Continue auditing
sample-count and usage intersections for other formats and backends.

Core queue submission also has a synthetic removed-device replay test. It
confirms HAL error classification, one-shot core device-lost notification,
subsequent submission rejection, queue invalidation and reentrant device polling
from the loss callback on a physical adapter opened at FL9_1. Queue submit now
defers the callback until its submission locks are released. Synthetic errors
from both fence `GetData` polling and the wait path also propagate through core;
the callback can reenter device polling without holding the snatch lock. Physical
device removal/recovery remains unverified; continue reviewing safe public error
and lifetime paths. Ordinary `device_poll` now also has a synthetic
`GetDeviceRemovedReason` failure test; its lost callback reenters polling and
later queue submissions are rejected. A separate strict HAL fence test injects
one `GetData` error, verifies the pending query is retained, then waits for that
query's real GPU completion. These tests cover error reporting and bookkeeping;
they do not simulate physical device loss or recovery.

### Public texture-view checkpoint — non-suffix 2D mip view (`4d3ec6bb9`)

The strict public sampled-mip test now creates a 2D view over source mips 1–2
of a four-level RGBA8 texture, binds it with a sampler clamped to the view's
second level and checks that the original mip-2 data is returned across all 64
pixels. This exercises the public core-to-HAL non-suffix clone-and-refresh
path; the complete strict public suite passed 51/51 on forced FL9_1 hardware.
The existing cube mip and private 2D view paths remain covered independently.

Continue the selected-entry shader and core/HAL contract audit, prioritizing
remaining observable failures reachable from valid public descriptors.

### Resource checkpoint — query-gated FL9_1 cube mip chains (`75222323e`)

The public offscreen path now accepts caller-populated mip chains for the
supported six-face, square POT RGBA8 cube subset when the selected device
positively reports format mip support. Four mips across all faces and a
non-suffix mip view were sampled on the forced-FL9_1 RTX 3090 Ti; readback
matched all 960 pixels. Unsupported formats and unavailable queries fail
closed. Cube copies, readback, render-target views, ordinary arrays and cube
arrays remain unavailable. The latest strict suites passed HAL 370/370, public
D3D11 51/51, core fixture 96/96 and DX12 core 92/92. See
`FL9_1_PROGRESS.md` for exact commands and limitations.

Next continue the selected-entry shader, texture-view and core/HAL contract
audit. Prioritize a reproducible mismatch that can be closed at the public
validation boundary, while keeping optional format/usage exposure query-gated.

### Compiler boundary — constant texture-sampling offset (9f1617eed)

WGSL 30.0.1 has no standalone textureSampleOffset builtin; its constant texel
offset form is the final argument to textureSampleBias. The existing
texture_sample_bias_offset limit and private HAL guard already reject it.
Coverage now follows direct and helper-reached uses through core, HAL and the
strict public path. Naga-generated HLSL fails FXC with X4532 under both
ps_4_0_level_9_1 and ps_4_0_level_9_3, while the SM5 control compiles. No new
shader capability or production validation path was added. The strict HAL
suite passed 368/368, public D3D11 passed 50/50, the core fixture passed 96/96
and DX12-feature core passed 92/92. See FL9_1_PROGRESS.md for the exact
commands and hardware details.

Continue auditing other core-accepted shader operations and render state
against the selected FL9_1 compiler and HAL. Keep unsupported operations gated
before HAL and do not claim historical-driver behavior from the modern RTX.

### Shader-contract checkpoint — WGSL loops (`62d12ecd4`, updated)

The original blanket selected-entry loop rejection has been narrowed. The
FL9_1 path disables Naga's dynamic loop guard, allowing FXC to unroll
statically bounded loops in either shader stage. `shader_loops` reports this
subset and `shader_dynamic_loops` remains false. Core now rejects runtime-
dependent loop exits for the selected entry and helper call graph before FXC;
the strict public test keeps the static-loop 64-pixel GPU check. See the latest
`FL9_1_PROGRESS.md` entry for exact commands and hardware details.

Continue the selected-entry WGSL/FXC audit, prioritizing constructs that core
accepts but the current FL9_1 HLSL path cannot compile or preserve. Keep each
restriction selected-entry-aware and backed by a compiler or hardware test;
keep D3D11 explicitly opt-in and do not infer historical-driver behavior from
the modern RTX test system.

### Compiler-boundary check — `instance_index` (`70341b011`)

The previously documented unverified `instance_index` profile behavior is now
tested through Naga HLSL generation and FXC. The `vs_4_0_level_9_1` profile
rejects generated `SV_InstanceID` with X4016 for used and unused direct and
structure-member declarations; a safe sibling and a `vs_5_0` used-input
control compile. The existing selected-entry core gate remains the safe
public contract. This is profile/compiler evidence and does not establish
historical-driver behavior. Continue auditing other core-valid shader and
render-state combinations.

### Latest public binding checkpoint — independent pairs across sparse groups (`b89865ca5`)

The strict public wgpu integration binds stage-visible uniforms in group 0,
the sampled texture's sampler in group 4 and the texture in group 7. The
selected shader samples across the two resource groups. Replacing only group
7 while retaining groups 0 and 4 changes readback to 32 blue and 32
tinted-yellow pixels. The complete strict public D3D11 test binary passed
50/50 on the RTX 3090 Ti at requested/actual FL9_1; its optional Float16x2
subcase remains an explicit query-based skip. Aggregate layout edge cases and
additional group transitions still need audit. See `FL9_1_PROGRESS.md` for
the exact tests.

The sparse multi-texture test now also places two independent texture/sampler
pairs at groups 1/4 and 6/7 using sparse logical bindings. Patterned data and
nearest versus linear filtering make sampler-slot swaps observable; all 64
pixels match the checked output on forced FL9_1. A public Repeat sampler probe
was rejected by core because it cannot prove arbitrary interpolated
coordinates stay within the FL9_1 repeat bound. Keep that validation intact;
the follow-on `d921da527` checkpoint replaces only the group-7 sampler between
draws, retains the other resources and pipeline, and verifies both filtering
results across 128 pixels. Resume the broader selected-entry and core/HAL
contract audit.

### Render-bundle checkpoint — strip restart and pipeline transition (`f98b72200`)

The strict public D3D11 test records consecutive Uint16 and Uint32
TriangleStrip bundles with restart markers, each with different vertex and
fragment pipelines, then executes both in one pass at forced FL9_1. Readback
matched all 256 pixels, including the clear gap, with 37 green pixels from the
first bundle and 37 red from the second. The 50-test public suite passed on
the RTX 3090 Ti; one optional input-format subcase remains an explicit
capability skip. A separate strict public test now transitions sparse uniform
and sampled-resource groups between two bundles and checks both readback
colors after dropping the original handles. A further public test selects
different dynamic uniform offsets inside two bundles and validates both
readback regions after dropping the source resources. Broader bundle state
combinations, restart topologies and historical-driver behavior remain to be
audited. A third strict case switches between uniform-only and
texture/sampler-only pipeline layouts. See `FL9_1_PROGRESS.md` for commands and
output.

Some status descriptions within the audit sections below preserve earlier
checkpoints. Later commits may supersede them; the opening summary and the
numbered current tasks are authoritative for the present state.

The public HWND path serializes submissions, queue writes, mapped-buffer
updates and context-backed resource creation against `Present`. Reentrant
same-thread operations that would mutate those resources are rejected, while
other threads wait for presentation to finish. Configure/acquire/present are
checked against the HWND owner thread. The public test verifies two presents
and resize on a hidden window. A visible example runs a Win32 message pump,
renders an animated indexed textured cube with depth testing, and handles
resize; a 60-frame forced-FL9_1 run succeeded. Captured-image verification
has confirmed the distinct face colors and clear background. The capture was
made from the live client area on a 150%-scaled desktop; the temporary PNG is
not a repository artifact.

The core/HAL audit also found and closed a selected-entry sampling-count gap:
FL9_1 now reports a separate limit of sixteen distinct texture/sampler pairs
per shader stage, derived from Naga's validated entry-point relationships.
Core rejects an over-limit pipeline before HAL while leaving the eight-texture
and sixteen-sampler binding capacities intact. The 16/17 boundary and sibling
entry-point selection are covered by a strict public test.

1. Preserve the tested private backend; continue uniform layout and real
   constant-budget testing. FXC and the forced-FL9_1 RTX accept a dynamically
   indexed vertex constant array of 255 float4s and reject 256 at index 256;
   pixel FXC accepts 32 and rejects 33 at register 32. Forced-FL9_1 GPU
   execution now verifies that WGSL access to vertex constant c254 changes
   geometry and pixel constant c31 changes fragment color. Runtime integer
   indexing remains rejected, so dynamic near-limit indexing is still
   unavailable. Core pipeline override constants are now resolved through
   Naga before selected-entry checks and FL9_1 shader compilation; strict
   hardware readback covers explicit/default values and invalid overrides for
   both vertex and fragment stages.
   Strict readback also covers nested structures, an asymmetric matrix,
   stride-sensitive arrays and padding. Runtime integer indexing remains
   unavailable. At this checkpoint the selected-device core projection
   permitted twelve dynamic uniform buffers per pipeline layout, the largest
   disjoint six-vertex plus six-fragment split within the then-current native
   allocator. Strict public readback exercised all twelve sparse bindings and
   core rejected thirteen before HAL. The later stage-specific allocator
   raises the current pipeline-layout limit to thirteen dynamic uniforms;
   fourteen are rejected, while thirteen static uniforms per shader stage are
   supported. Continue checking shader instruction pressure and other untested
   layout shapes with checked translation. The selected FL9_1
   projection now reports 255 vertex and 32 fragment uniform float4 vectors;
   core conservatively counts complete statically used uniform layouts per
   selected entry point. Strict public readback used all 32 fragment vectors
   across two sparse bindings, while core rejected 33 before HAL. Per-stage
   aggregate validation is implemented. The public binding-size limit now
   allows the 4,080-byte vertex c0-c254 range; strict readback changes geometry
   from c254 and core rejects a 256-vector vertex shader. Fragment shaders
   remain limited to 32 vectors. Broader WGSL packing and instruction-budget
   cases still need coverage.
2. Extend depth/stencil coverage from the implemented D16 depth-only and D24S8
   attachment paths. D16 uses a required-format guarantee intersected with a
   selected-device query and is single-mip; strict public tests verify depth
   ordering in both draw orders, `DepthOnly` view use, rejection of stencil and
   multi-mip use, preservation across a second-pass LOAD and reset by CLEAR.
   A discard followed by LOAD also verifies that stale depth does not affect
   rendering. Public D24S8 tests verify both aspects survive LOAD, each
   aspect's CLEAR independently changes later comparisons, and each aspect's
   DISCARD followed by LOAD is initialized before use. Query-gated D24S8
   comparison sampling now passes through public core and HAL; DepthOnly SRVs
   are valid, while aspect-specific depth/stencil render attachments remain
   rejected by core. Continue auditing less common render-pass combinations
   and format aspects before widening public claims.
3. Broaden pipeline and resource functionality: vertex layouts, topology,
   indexed semantics, render state, mips/formats and multiple groups. Dynamic
   uniform offsets have restricted private coverage. A controlled core test
   still exercises a two-group fixture pipeline (uniform in group 0, sampled
   resources in group 1). The strict public integration now also binds
   uniforms in group 0, a sampler in group 4 and a texture in group 7, then
   replaces group 7 alone and verifies the new sample output. Broader
   aggregate group layouts remain to be audited. All wgpu vertex formats with
   direct DXGI input equivalents are
   now mapped except Float64 variants. Outside the ten FL9_1-required IA
   formats, core exposes a format only when the selected device positively
   reports `IA_VERTEX_BUFFER`; the forced-FL9_1 RTX reports no optional input
   formats, so optional-format GPU execution remains unverified. Each feature
   needs positive GPU and negative tests. Non-indexed
   line lists and line strips now have a
   bounded private FL9_1 implementation with strict GPU readback and the
   controlled core fixture verifies both through core command recording and
   readback. Indexed Uint16/Uint32 line lists now preserve line pairs through
   compact rebasing and wide-pair gathering, with strict HAL GPU output and a
   controlled core fixture. Indexed LineStrip now splits format-specific
   restart markers into independent runs, preserves connectivity at native
   draw limits, rebases bounded windows and gathers wide edges. Restart
   markers are matched after signed base-vertex resolution, following the
   current WebGPU index-resolution and primitive-assembly algorithms. Indexed
   TriangleStrip now splits the resolved markers, expands each run with
   alternating winding into bounded triangle-list draws, and gathers triangles
   whose index span exceeds the native FL9_1 window. Strict private readback
   compares both index formats against equivalent triangle lists across front
   face and cull settings; the controlled core fixture verifies both formats.
   Indexed LineList and TriangleList with only instance-rate or no vertex-rate
   inputs now validate the index range and replay a matching nonindexed draw
   sequence; strict private and public readbacks compare Uint16/Uint32 cases to
   direct draws. Indexed LineStrip and TriangleStrip now read the execution-time
   index contents, split effective restart runs, and replay matching per-run
   nonindexed strips when no vertex-rate inputs exist. Strict tests cover both
    index widths, layouts and strip topologies. PointList retains its separately
    tested instance-only path. Emulated `vertex_index` covers TriangleList,
    PointList, LineList, LineStrip and TriangleStrip.
    `instance_index` is limited to direct `u32`-to-`f32` use, with general
    integer semantics still unavailable.
   Broader instance semantics and indirect draws remain unavailable. Microsoft
   marks native `DrawInstancedIndirect` and `DrawIndexedInstancedIndirect` as
   unsupported at 9_1; wgpu-core's `INDIRECT_EXECUTION` flag is also coupled to
   compute support and GPU argument validation, so the public validation path
   cannot currently expose a private CPU-staging emulation independently.
    R8_UNORM is implemented as a queried sampled/upload format; mip chains
    require positive selected-device `TEXTURE2D | MIP` support. Color-target
    and copy-source combinations also need a positive `RENDER_TARGET` query
    and matching backend path. NPOT chains require unrestricted-NPOT support;
    `SHADER_SAMPLE` separately controls filterability. Its 2D upload, nearest
    sampling and query-gated mip chain pass on forced FL9_1. R8G8_UNORM, which is
   not a required FL9_1 sampled format, is exposed only after a positive
   selected-device `TEXTURE2D` query; `SHADER_SAMPLE` separately controls
   filterability. The RTX returned `0x0002c320`, and strict public
   upload/sample readback passed. R8_UNORM (`0x0002d3e0`) and R8G8_UNORM now
   additionally expose single-mip render-attachment use after their selected
   format query reports `RENDER_TARGET`; strict public draw-then-sample
   readback now covers R8_UNORM, R8G8_UNORM and B5G6R5 with all 64 pixels per
   format checked. B5G6R5 requires `TEXTURE_FORMAT_16BIT_PACKED`. R8_UNORM,
   R8G8_UNORM and B5G6R5 now also expose query-gated GPU copies and CPU
   readback when `RENDER_TARGET` is reported. Their blend capability is
   separately gated by `BLENDABLE`; strict public FL9_1 readback verifies
   identical-equation alpha blending on the RTX for all three. These remain
   single-mip. Other narrow formats remain without copy-source/readback paths.
   The required
   FL9_1 IA formats now include `Uint8x4`, `Sint16x2` and `Sint16x4` alongside
   the previously mapped formats, with a core per-format downlevel mask and
   strict GPU verification. Required RGBA8_SRGB and BGRA8_SRGB formats now
   have independent device queries and public sampled/render-target paths;
   strict FL9_1 readback verifies decoding and encoding for both channel
   orders. The public suite also samples four-level POT chains of both formats
   and verifies implicit LOD selects mip 1. The required FL9_1 table
   guarantees R16G16_SNORM as a 2D texture but requires `SHADER_SAMPLE` only
   from FL9_2. The RTX forced to FL9_1 returns `0x00021122`: it supports the
   2D resource but lacks any-filter support. Private and public tests verified
   point-filtered upload/sample/readback while core rejects a filtering bind.
   The public adapter exposes `TEXTURE_FORMAT_16BIT_NORM` only after the
   selected query establishes 2D support. The Windows 8 FL9_x packed formats
   B5G6R5, B5G5R5A1 and B4G4R4A4 now have individual selected-device queries
   and single-mip sampled/upload support behind `TEXTURE_FORMAT_16BIT_PACKED`;
   forced-FL9_1 public readback covers all three channel layouts and alpha.
   B5G6R5 also has query-gated single-mip render-target support; a public
   shader draw, sample and readback checks every pixel. The RTX reports
   `0x0026f3a0`, including BLENDABLE. R8_UNORM and R8G8_UNORM likewise report
   the optional bit. The public projection now exposes it only when the
   selected query and implemented pipeline path agree; a strict overlay test
   validates all three. Continue the format and usage audit from the actual
   query table.
4. Resolve core/HAL contract mismatches before exposure: core now models
   `viewport_rectangles_outside_target` and rejects clipped rectangles for the
   conservative FL9_1 fixture before HAL, because the optional remapped vertex
   shader can exceed FL9 compiler limits. The private HAL's fractional
   viewport remap is now readback-tested, including the core path with
   contained geometry; the native FL9_1 viewport stays integer-bounded and
   the shader transform preserves logical fractional coordinates.
   Historical-driver precision remains unverified. DXGI device-loss classification is
   centralized and covered for reset, removal, driver-internal and OOM codes;
   continue auditing mapping,
   teardown and remaining device-loss panic paths. Empty compute
   passes and unused blend constants are handled; compute dispatch remains
   unavailable. The downlevel combined-index usage rule has been corrected,
   and unavailable occlusion query sets are rejected by a downlevel flag.
   Depth-only clears and restricted draws work privately; handle read-only
   depth hazards and audit the remaining general render-pass combinations.
   Active asymmetric stencil state is now rejected by core unless the adapter
   reports `DownlevelFlags::TWO_SIDED_STENCIL`; the D3D11 fixture omits it at
   FL9_1. Comparison-sampler creation now similarly requires
   `DownlevelFlags::COMPARISON_SAMPLERS`, and the controlled fixture verifies
   rejection before HAL. Full-channel differing RGB/alpha blend equations now
   require `SEPARATE_ALPHA_BLEND`; the FL9_1 fixture checks this while
   preserving the masked-channel emulation path. Continue mapping core-valid
   render state and format combinations to explicit downlevel or format
   capabilities before considering production enumeration. Storage-buffer
   layouts now require the matching vertex, fragment or compute downlevel flag.
   The HAL accepts core-internal storage usage bits only alongside another
   supported buffer role; it still does not implement shader storage access.
    Texture-view layout and creation validation now requires explicit
    downlevel support for 2D arrays and cube-array views. The D3D11 projection
    leaves those flags absent and verifies rejection before HAL. The supported
    FL9_1 cube subset is query-gated by `CUBE_TEXTURES`, a separate cube limit
    and per-format queries; strict public rendering samples all six RGBA8
    faces. D1/D3 binding layouts are rejected when their texture dimension
    limits are zero, and storage-texture D2Array layouts share the array gate.
   The selected-entry audit also models `textureLoad` as a downlevel limit and
   rejects only entries that reach a load through their call graph; safe sibling
   entries remain usable. Continue auditing other WGSL operations accepted by
   core but rejected by the private FL9_1 compiler. The public FL9_1 projection
   now models the FXC bias subset (`abs(literal) <= 3`, no dynamic value or
   offset), and selected-entry core validation follows helper calls so
   unsupported bias forms fail before HAL. Strict public readback verifies
   implicit sampling and -3/+1/+3 literal bias mip selection. Continue the
   shader audit for additional core-accepted compiler failures. The
   `ps_4_0_level_9_1` projection also disables pixel derivatives and explicit
   texture gradients; selected-entry core validation follows helper calls and
   rejects `dpdx`, `dpdy` and `textureSampleGrad` before HAL.
   Reachable `textureSampleLevel` calls are separately gated by the selected
   entry point. General exact LOD and helper-forwarded LOD remain rejected;
   FXC rejects unlowered exact-LOD operations under the 9_1 and 9_3 pixel
   profiles. The current-state section documents the direct literal-zero,
   single-mip exception. Implicit sampling remains allowed. Selected-entry `textureDimensions` and
   `textureNumLevels` use is gated by narrow downlevel limits and host-known
   view metadata; general image queries remain unavailable.
   `textureSampleBaseClampToEdge` has a separate selected fragment 2D-float
   path using host dimensions, half-texel coordinate clamps and implicit
   sampling. It supports direct use or one reachable straight-line helper
   called once; nested calls and more general forms remain rejected. Continue checking other
   WGSL expressions against the FL9_1 compiler path. Fragment
   `@builtin(sample_mask)` output is separately gated: Naga lowers it to
   `SV_Coverage`, which Microsoft documents as writable only from `ps_4_1`,
   while FL9_1 compiles `ps_4_0_level_9_1`. FXC rejection and the selected-entry
   core error are tested; a possible single-sample discard emulation remains
   unimplemented. Fragment `@builtin(sample_index)` is already rejected at
   WGSL/Naga validation because the FL9_1 projection omits
   `MULTISAMPLED_SHADING`; a strict public regression now confirms this path
   and acceptance of a single-sample shader. FXC rejects fragment
   `@interpolate(flat)` and `@interpolate(linear)` because their HLSL forms
   `nointerpolation` and `noperspective` fail with X4568 under
   `ps_4_0_level_9_1`; core now gates only the selected fragment entry point.
   Vertex outputs with these modes and fragment centroid interpolation compile.
   A strict public render confirms centroid and perspective produce identical
   single-sample readback. The previously compilation-only structured
   color-plus-`frag_depth` case now has strict public depth-test readback:
   fragment depth overrides contradictory vertex depth in both draws as
   expected. WGSL `textureGather` is now a
   stage-agnostic downlevel limit. Microsoft documents HLSL Gather for
   graphics at shader model 4.1 (FL10_1+) and compute gather4 at SM5 (FL11_0);
   the all-stage guarantee therefore begins at FL11_0. FXC rejects the FL9_1
   lowering with X4532, while `ps_4_1`, `ps_5_0`, and `cs_5_0` controls compile.
   Core rejects direct/helper use in selected render and compute entries while
   compiling an implicit-sampling sibling. Continue auditing other core-valid
   operations against the selected FL9_1 compiler.
   Single-sample output masks now reach D3D11; zero WebGPU masks map to an
   equivalent nonzero FL9_1 native mask. RGBA8 4x render-and-resolve and
   D16/D24S8 depth attachments now require their own positive selected-device
   format and sample-count queries, and must use matching pass and pipeline
   sample counts. Multisampled sampling and depth resolve remain gated.
   Four-sample resolve does not imply alpha-to-coverage: a dedicated
   `DownlevelLimits::alpha_to_coverage` capability is false for FL9_1, and the
   controlled core fixture verifies rejection before HAL. Next close the
   selected-entry dynamic-loop/FXC mismatch and mapped `BufferView`
   destruction lifetime issue found in the continuing audit.
   Core texture creation now requires `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`
   for NPOT spatial dimensions when more than one mip is requested; the D3D11
   implementation remains POT-only even when the runtime reports full NPOT.
   The private single-target blend path now translates ordinary wgpu factors
   and all five operations, including blend constants; it rejects FL9_1
   `SRC_ALPHA_SAT`, dual-source factors and full-channel distinct RGB/alpha
   equations unless the latter are explicitly established. Strict forced-9_1
   readback covered custom constant factors, while CPU translation tests cover
   every supported factor/operation pair. Continue auditing other core-valid
   render-state combinations and shader limitations. Core now models the
   10Level9 `SRC_ALPHA_SAT` restriction as a downlevel capability and rejects
   all factor positions before HAL; continue the vertex-format and attachment
   audit before preparing the public capability snapshot.
   Raster depth comparison now maps all seven native D3D11 comparison functions;
   `Never` keeps its validated-draw and stencil-depth-fail emulation. A strict
   forced-FL9_1 render verifies pass and fail pixels for all eight wgpu compare
   functions. The 10Level9 `LessEqual` restriction is for comparison samplers,
   not raster depth state. FL9_1 triangle pipelines also pass finite slope-scale
   bias to the native rasterizer with the required zero clamp; strict readback
   verifies positive/negative slope effects. Solid points and lines remain
   rejected for nonzero bias because bias has no effect on them. Historical
   FL9_1 drivers remain unverified. All eight stencil operations are now
   verified on pass, stencil-fail and depth-fail outcomes with strict FL9_1
   readback; asymmetric front/back states remain rejected. Core now gates
   multi-mip depth/stencil attachments through a dedicated downlevel flag,
   matching the FL9_x D3D11 requirement that depth/stencil-bound resources use
   one mip. Vertex-stage sampled texture access now has a separate downlevel
   limit. Core counts resources used by the selected vertex entry point and
   rejects them before HAL; the controlled FL9_1 fixture reports zero and
   verifies that result. Conditional NPOT repeat use is represented by a
   downlevel capability and checked against Naga's selected-entry sampling
   pairs in core when the reported repeat limit allows such samplers. FL9_1's
   fixed repeat cap is 128, so the controlled core fixture now derives that
   value from the selected level and rejects Repeat and MirrorRepeat sampler
   creation: core cannot prove interpolated coordinate ranges for every draw.
   The fixture separately renders POT and NPOT clamp-to-edge resources. CPU
   tests cover the conditional NPOT relationship path under synthetic
   unrestricted-repeat settings; private HAL tests exercise bounded repeat.
   The RTX forced to 9_1 does not validate legacy overflow behavior. Test selection
   now checks downlevel limit requirements as well as flags. Next continue
   auditing core-valid attachment, viewport and render-state combinations
   before building the truthful public capability snapshot. The audit also
   found that core permits RGBA8/BGRA8 linear-to-sRGB texture copies while HAL
   required exact DXGI format equality. The D3D11 copy path now accepts only
   the corresponding same-family linear/sRGB pairs; strict FL9_1 readback
   verifies raw-bit preservation for both channel families.
5. Implement a truthful hardware adapter, device limits/features/downlevel and
   per-format reports. Audit every core-valid command against HAL behavior;
   restrict safely or implement/emulate it. No placeholder capabilities.
   An FL9_1-only wgpu-types projection now feeds the controlled core fixture
   and ordinary opt-in adapter enumeration, including selected-device format
   reports and explicit unsupported 1D/3D shape gates. A strict public wgpu
   integration test selects the physical RTX 3090 Ti, requests devices within
   the projection, uploads and samples a texture at sparse bindings, binds
   separate vertex transform and fragment tint uniforms in group 0, a sampler
   in group 4 and a sampled texture in group 7 of an eight-position layout,
   and reuses binding indices across those groups. Replacing only the texture
   group changes readback while the sampler and uniform groups remain bound.
   It performs indexed depth-tested draws, updates the
   transform between submissions using a dynamic uniform offset, and verifies
   both images by readback. Continue auditing the full core/HAL contract before
   widening the projection.
   Logical binding indices remain capped by the wgpu index limit while actual
   resource counts use separate backend limits. The projection exposes six
   uniform buffers per shader stage so disjoint vertex/fragment bindings fit
   the shared thirteen-slot HAL allocator; strict core GPU coverage exercises
   two independent fragment uniforms, a vertex uniform alongside a fragment
   uniform through the same selected-device projection, and rejects seven in
   one stage. The aggregate BC feature stays withheld because BC4 through BC7
   remain unsupported; BC1, BC2 and BC3 are now implemented only in the
   selected-format private texture path. Continue the descriptor and
   feature/format audit before treating the exposed subset as complete.
6. Keep the narrow HWND surface contract safe and tested alongside CoreWindow.
   Public operations serialize queue submission, writes, mapping flushes,
   polling and context-backed creation against `Present`; same-thread
   reentrant mutations fail safely, while other threads wait. Configure,
   acquire and present enforce window-thread affinity. Strict hardware tests
   cover hidden-window configure/present/resize and wrong-thread rejection; a
   bounded visible example presents an animated textured indexed cube for 60
   frames at forced FL9_1. A captured live client area has been visually
   inspected, and the controlled core fixture holds the presentation guard
   while verifying same-thread queue write/submit, poll, buffer creation and
   texture creation behavior. Continue testing lifecycle/error and remaining
   reentrancy edges. `unconfigure` detects same-thread reentry and returns
   without waiting, while other threads continue to wait for the active
   operation; CPU tests cover both reservation behaviors. Synchronous native
   callback delivery remains unverified. Preserve the CoreWindow path;
   `from_core_window` now requires dispatcher-thread access and records the
   owner thread for later operations. CoreWindow public surface capabilities
   remain withheld until the UWP lifecycle and render/readback path is tested.
   Windows 10 Mobile ARM32 remains unverified. Strict hidden-HWND HAL tests
   also verify that same-surface `configure` and `acquire_texture` fail
   promptly while another surface operation owns its operation guard; normal
   present/resize remains functional afterward.
7. Audit unsupported operations, synchronization, lifetime and device loss;
   run strict GPU, core, Naga and non-D3D11 regressions and relevant downlevel
   conformance cases. Independently review the public contract. The upstream
   `wgpu-gpu` Naga capability selector now uses HLSL's capability ceiling for
   the D3D11 backend instead of assuming no DX11 adapter can be selected; the
   focused selector ran against the physical FL9_1 adapter.

## Completion evidence

The public example must select a physical D3D11 adapter, report requested and
actual FL9_1, execute and read back representative depth-tested rendering, and
present to an HWND where an interactive desktop is available. Publicly
advertised operations must be implemented or rejected by valid core rules.
Document unavailable hardware features separately from missing implementation,
and historical hardware separately from modern GPUs forced to FL9_1.

Run only one heavyweight Cargo process at a time with bounded build jobs.
Record exact commands, hardware results, skips, failures and commit hashes in
FL9_1_PROGRESS.md. Maintain FL9_1_HANDOFF.md for interruption and final audit.

Latest sampled-format increments: R8_UNORM (`268431ec3`), R8G8_SNORM
(`881640796`), R8G8B8A8_SNORM (`312dba141`) and BC1_UNORM (`6c67e22be`)
use queried, single-mip sampled/upload paths and strict FL9_1 GPU readback.
BC1 upload uses 4x4 blocks and 8-byte block rows, including padded pitches,
nonzero source offsets and aligned partial updates. A selected-device-query-
gated, block-aligned BC1 GPU texture-copy source path now passes a partial-copy
and sampled-output test. `DownlevelLimits::texture_to_buffer_copy_format_mask`
keeps BC1 CPU readback unavailable before HAL because FL9_1 cannot copy an
SRV-only resource to CPU-visible memory. Render-target, sRGB and mip-chain
uses remain unsupported. The controlled
core fixture now validates BC1 through texture creation, queue upload, binding,
rendering and RGBA8 output readback, and creates its device via `adapter_request_device`
and the test-only HAL `Adapter::open`. It still injects hand-selected limits
and does not by itself represent the public adapter contract. The opt-in
production adapter now uses that same selected-device projection for public
offscreen rendering and a constrained HWND surface. Continue auditing the
public HAL/core capability boundary and every core-legal operation; do not
widen the current projection until those operations are implemented or
rejected. This checkpoint predates the later CoreWindow capability exposure:
current code reports capabilities for successfully constructed CoreWindow
surfaces, but UWP/Lumia runtime coverage remains open. Latest code checkpoints
`983eea619` and
`834cd9856` add core validation and cross-backend reporting for multi-mip
depth/stencil attachments. The earlier `6f7f12978` added native FL9_1
slope-scaled triangle depth bias; checkpoint
`1a362ec6108de9accb52e2f1978a174d1f923106` modeled hardware bias guarantees
separately from backend support across all seven feature-level identities.
The controlled core fixture now validates the per-format `SINGLE_MIP_ONLY`
capability for R8_UNORM, R8G8_SNORM, R8G8B8A8_SNORM, BC1_UNORM and D24S8;
POT multi-mip descriptors are rejected before HAL creation while one-mip
sampled-upload cases remain available. This closes the format-specific gap
left by the separate NPOT-mipmap and depth-attachment gates.

Per-format projection is centralized in
`Capabilities::texture_format_capabilities`, which intersects positive
selected-device format queries with feature-level shape checks and implemented
backend paths. Both the opt-in production adapter and the test-only core
fixture delegate to this internal method. Projection tests cover missing/
negative queries and unsupported backend paths. Continue the descriptor and
render-pass audit for core-valid requests the private HAL cannot execute; the
public offscreen and constrained HWND routes remain experimental and do not
complete this audit.

The latest audit found a core/HAL pipeline mismatch: core permits nonzero depth
bias when the selected fragment entry point writes `frag_depth`, while the
private HAL rejects it. A narrow selected-entry validation gate now rejects
that case before HAL pipeline creation, while unused depth-writing entries do
not trigger it. The per-format RGBA8/BGRA8 mip projection honors missing driver
MIP support and disabled backend mip paths; the strict RTX fixture reports MIP
support for both formats at FL9_1. The public adapter uses this projection
for offscreen and constrained HWND access; at this checkpoint CoreWindow was
still withheld. Later code exposes capabilities for constructed CoreWindow
surfaces, with UWP/Lumia execution still unverified.

The selected-entry depth-bias mismatch is now represented in core's
`DownlevelLimits` and covered by the controlled fixture. Next audit remaining
core-valid depth/stencil and render-pass combinations. The tested FL9_1 FXC
profile also rejects `SV_Depth` without `SV_Target0` at zero bias with X4530;
the new `fragment_depth_without_color_target_0` limit makes core reject only
that selected output shape before HAL, while structured color-plus-depth
remains compilable. Continue checking core-valid shader output shapes against
FXC before widening the projection. The tested FL9_1 FXC profile also rejects
fragment `@builtin(front_facing)`, and Microsoft documents
that 9_x pixel shaders cannot read fragment `@builtin(position)`. Core now
represents these with selected-entry `DownlevelLimits` gates, so safe sibling
entries remain usable. Continue auditing shader and render-state combinations
against the currently exposed projection; do not widen it until every permitted
operation is implemented or safely rejected. Public surface capabilities are
limited to the tested HWND contract at this checkpoint; later code exposes
CoreWindow capabilities for successfully constructed surfaces, but runtime
execution remains unverified.

The selected-entry audit now also covers vertex system-value inputs. The
controlled FL9_1 profile reports `vertex_index_input` and `instance_index_input`
as unavailable; core rejects declarations before HAL and permits safe sibling
entries. The backend-generated `SV_VertexID` interface fails to compile with
FXC X4555 under the tested FL9_1 vertex profile; `SV_InstanceID` fails with
X4016 for direct and structure-member inputs, used or unused. The safe
instance-index sibling compiles at FL9_1 and a used input compiles with the
`vs_5_0` control. These are compiler/backend boundaries, not universal
hardware guarantees. The opt-in adapter reports the selected FL9_1
projection; a strict public test confirms both built-ins are rejected before
HAL while a safe sibling pipeline compiles.
The current public surface remains limited to the HWND contract summarized at
the top of this plan. Continue the core/HAL audit before widening the public
capability contract.

The public offscreen integration now exercises sparse group-0 uniforms, a
group-4 sampler and group-7 sampled texture, group replacement, indexed
textured depth-tested output, dynamic uniform updates and two concurrently
submitting public devices. Its error
scopes additionally verify that core rejects vertex-visible storage-buffer
layouts and D2Array sampled-texture layouts, alongside unsupported 3D,
multisampled, format and mip requests. These checks expand the tested public
validation boundary but do not complete the core/HAL audit. The bounded
windowed example now exercises desktop HWND presentation; broader lifecycle,
device-loss and historical-driver behavior remain unverified.

The same public test now overlays near and far textured geometry in both draw
orders and verifies that all pixels show the nearer surface. This establishes
depth-test ordering through public wgpu; a representative desktop application
and presentation are still outstanding.

The public adapter projection's one-mip R8Unorm sampled/upload path is also
render-tested: a non-filtering sample from two uploaded values produced the
expected 64-pixel split. Continue validating other projected format/usage
combinations at the public boundary.

The indexed public scene now fills its vertex buffer through a mapped-at-
creation range before unmapping and drawing; successful FL9_1 readback
verifies the staged mapping contents reached GPU vertex input.

The public offscreen integration now also samples two independently uploaded
textures at sparse bindings 7 and 42 through one sampler at binding 100. The
layout, WGSL declaration and bind-group entry orders differ; the strict GPU
test verifies all 64 pixels contain both contributions and public core rejects
an incomplete group. Continue the exact core/HAL audit and broaden public
render coverage. HWND presentation now serializes selected context operations
and rejects tested same-thread reentry; native callback and lifecycle edge
coverage remains open.

The public path now exercises the entire projected eight-texture fragment
capacity in one shader. Each sparse-bound uploaded texture affects every
checked pixel; a ninth fragment texture is rejected by core before HAL. This
verifies the current count boundary on the RTX forced to FL9_1, not historical
driver behavior or every shader's instruction budget.

A public transfer test now sends a nonzero-offset, padded-row mapped buffer
through a partial buffer-to-texture update, copies a 2×2 source box between
different texture dimensions at nonzero origins, and reads all 36 destination
texels back through a padded buffer layout. The test also verifies untouched
destination texels retain their prior values.

The public RGBA8 sampled path has a four-level POT mip-chain execution test;
the query-gated R8_UNORM path uploads and samples a four-level chain, then
renders to mip 0 and reads it back when the selected query also permits render
targets. Distinct values were uploaded to each mip and mip selection was
verified through readback. R8 mip support is conditional on `TEXTURE2D | MIP`;
copy-source/render-target use additionally requires `RENDER_TARGET`, while
NPOT dimensions require unrestricted-NPOT support.

Public Rg8Snorm and Rgba8Snorm capabilities now have upload, sample and
conversion readback coverage through ordinary wgpu. The exact selected-device
reports remain non-filterable and single-mip-only; all 64 output pixels matched
the signed-to-unorm shader conversion.

Public vertex input coverage now includes emulated per-instance streams with a
nonzero `first_instance`; readback verifies the selected instance records and
the skipped record. The public selected-entry gate rejects `instance_index`,
which the FL9_1 shader path does not implement.

## Current checkpoint — outputless fragment with depth and stencil

The public FL9_1 projection already allowed an outputless fragment stage with
no color targets and a depth attachment, but the D3D11 HAL rejected that
pipeline shape. Direct Naga HLSL output failed with FXC X4530 because the
`ps_4_0_level_9_1` profile requires `SV_Target0`. The HAL now retains the
validated fragment body as a helper and adds a synthetic zero color result to
the entry point. The pass binds no RTV, so the added color has no observable
destination; discard and depth behavior remain active. A strict public D16
test uses conditional discard, then draws farther geometry in a color pass.
A D24S8 test verifies that the discard also suppresses stencil replacement.
Separate later stencil and depth comparisons validate 32 pixels on each half
of both readback images. The compiler test covers a location input, a helper
call and a sparse uniform mapping. A color-returning fragment with an
unmatched location-0 output is also accepted in a targetless D16 pass; a later
depth comparison verifies the output is ignored while depth writes remain.
The full strict public suite passed 38/38,
including the D24S8 case, on the physical RTX 3090 Ti at requested/actual
FL9_1. Historical FL9_1 profile behavior remains unverified.

The public FL9_1 projection now has separate downlevel limits for runtime
integer shader operations and stores through aggregates containing two-row
matrices. Core analyzes validated Naga IR per selected entry, follows helper
calls and pointer arguments, and rejects unsupported paths before HAL while
preserving direct integer-vector vertex-input conversion and safe matrix
reads. Public error-scope checks and CPU analysis tests cover both gates. The
first full public-suite run exposed an error-precedence regression for fragment
sample-mask output; the dedicated sample-mask gate now runs before the broad
integer gate, and the full suite passes. Next continue auditing core-valid
shader and render-state combinations against the actual HAL. Keep optional
formats query-gated and historical FL9_1 and ARM32 behavior explicitly
unverified.

The public command path also has strict render-bundle execution tests: core
records a nonindexed triangle, an indexed textured triangle and Uint16/Uint32
indexed TriangleStrips with primitive-restart markers. Each restart bundle
renders two separated shapes and checks all 256 pixels, including the clear
gap. A separate pair of bundles changes a uniform group and sampled-resource
group, then validates both outputs after releasing their source handles.
Consecutive bundles also select distinct dynamic uniform offsets and verify
both images after releasing source resources. A further case changes pipeline
layout and resource type between bundles. A writable D16 attachment case
also verifies that the farther of two bundled draws fails depth after the
near bundle writes it. One bundle executed twice in a pass now verifies that
pass-level scissor and blend-constant values survive bundle execution; the
two halves of the 64-pixel target read back exact red and green. Continue
checking remaining render-bundle state and the broader core/HAL contract. A
strict D24S8 case found that Core replayed bundle pipelines without restoring
the render pass's dynamic stencil reference. Core now reapplies the current
reference after each bundle pipeline that uses it; two stencil write values
from one bundle produce independently verified red and green halves.

## Follow-up — mapped-view destruction safety

The safe public `Buffer::destroy` API now checks the shared `MapContext` before
core can unmap and release the backend allocation. `MapContext::reset` checks
for active subranges before clearing its mapped range, so a caught panic keeps
the map state usable. A strict FL9_1 test verifies two disjoint views remain
readable after a rejected destruction and that destruction succeeds once both
views are dropped. The selected-entry dynamic-loop/FXC mismatch is now gated
and tested. Continue with a public sampler-group transition test, then resume
the selected-entry and core/HAL contract audit.

## Current increment — query-gated FL9_1 cube sampling

The opt-in public adapter exposes the 10Level9 six-face cube-resource subset
through a separate 512-pixel cube limit and selected-device per-format
`TEXTURECUBE` queries. Ordinary 2D arrays and cube arrays remain disabled.
Core rejects unsupported cube shapes, formats and views before HAL. A strict
public test uploaded a different RGBA8 color to each face and sampled all six
axis directions at explicitly requested/actual FL9_1; all 96 readback pixels
matched. The RTX 3090 Ti reported cube format mask `0x1f1f`, and core rejected
RG8 cube creation because the selected format query did not include cube
support. Cube copies and render-target views remain unavailable. Continue the
core/HAL audit; do not infer historical FL9_1 support from this modern driver.

The HWND Present device-loss path now has a fixture-only public regression:
synthetic `DXGI_ERROR_DEVICE_REMOVED` is classified as device loss, reaches the
public callback once and permits a callback-reentrant poll to return promptly
under the active presentation guard. Physical device removal and recovery
remain unverified; continue auditing other public surface lifecycle and
core/HAL error paths.

## Integration readiness checkpoint — 2026-11-07

The current strict public offscreen example was rerun from the clean checkpoint
on the RTX 3090 Ti at requested/actual FL9_1 and validated all 1,024 pixels.
An opt-in application trial is reasonable when its API usage matches the
capability matrix, but this is not a general-purpose wgpu backend yet. Continue
the exact core/HAL contract audit, prioritizing public-valid operations that
can still reach unsupported HAL paths and shader forms whose selected-entry
validation is incomplete. Then broaden public workload coverage. Keep the
desktop HWND and private CoreWindow paths separate in the claims: HWND output
has been qualitatively captured, while Lumia/CoreWindow runtime, historical
FL9_1 drivers and physical device-removal recovery remain unverified.

## Public draw-limit boundary — 2026-11-07

The public strict GPU path verifies a nonindexed point draw one primitive over
FL9_1's 65,535 per-draw limit and a triangle list split after 65,535 input
vertices. The only visible point and triangle are in their respective second
batches; readback proves HAL rebases point input and preserves whole triangles
through public core submission. Continue the public-valid core/HAL audit,
especially selected-entry shader validation and render-state combinations.

The strict public integration target also renders a perspective cube with a
sampled texture and D16 depth. At forced FL9_1, opposite face orders match over
all 1,024 pixels with depth, while the no-depth painter-order controls differ
at 223 pixels. This closes one representative-scene coverage gap; continue
auditing other public-valid combinations and keep the overall mission partial
until material implementation, safety and platform gaps are resolved or
explicitly blocked.

Latest checkpoint: query-gated FL9_1 RGBA8 multisample render/resolve now
supports the selected device's positive 2x/4x/8x queries, with independently
queried matching depth attachments. On the RTX 3090 Ti forced to FL9_1, the
strict private HAL suite passed 397/397, the public D3D11 suite passed 72/72
(one optional Float16x2 input subcase skipped), and x2/x8 color, depth and
resolve readbacks matched all pixels. The device reports no x16 support, which
remains unavailable. DX12 core passed 94/94. The mission remains partially
complete; broad API auditing, historical hardware and Lumia runtime remain
open. Exact evidence is appended in `FL9_1_PROGRESS.md`.

Latest checkpoint: `POLYGON_MODE_LINE` is now exposed only when the selected
D3D11 device accepts a wireframe rasterizer-state creation probe. The strict
RTX 3090 Ti FL9_1 test compared the full 16×16 triangle wireframe with explicit
edge-line rendering; the images matched at all pixels (17 green coverage
pixels). Private HAL passed 398/398 and public D3D11 passed 73/73 with one
optional Float16x2 subcase skipped. The mission remains partially complete;
see `FL9_1_PROGRESS.md` for verification and open API/platform gaps.

## Latest increment — query-gated R8_UNORM mip chains

R8_UNORM mip chains require selected-device TEXTURE2D | MIP support and the
backend path; NPOT chains additionally require a positive unrestricted-NPOT
query. Color-target and copy-source combinations require RENDER_TARGET and
matching implemented paths. SHADER_SAMPLE independently controls filterability.
A strict forced-FL9_1 public test on the RTX 3090 Ti uploaded four mips, sampled
mip 2, cleared mip 0 as an attachment and read mip 0 back; all 64 RGBA pixels
and 64 R8 texels matched. Complete private HAL passed 402/402, public D3D11
passed 75/75, adapter isolation passed 1/1, and DX12 core passed 94/94. The
feature remains bounded; partial mip-1 same-level copying and mip-1-to-mip-2 copying between mipmapped R8 textures pass, while other cross-level cases and historical hardware,
Lumia runtime and broad core/HAL contract coverage remain open.

The latest follow-up adds strict public coverage for a partial R8 copy at mip 1
between two mipmapped textures. All 16 destination texels, including the
unchanged border, matched on the RTX 3090 Ti at requested/actual FL9_1. The
complete public D3D11 suite now passes 76/76; copies between differing R8 mip
indices remain unverified.

### Latest hardware follow-up — R8 NPOT mips

The R8 four-mip public test now uses a 6x10 chain when the selected device
reports unrestricted NPOT sampling. The RTX 3090 Ti returned true at forced
FL9_1; mip 2 readback matched 64 pixels and mip 0 attachment readback matched
60 texels. The query-negative path is synthetic only. This does not establish
behavior on historical restrictive hardware; continue the remaining public
contract and platform audit.

## Strict upstream GPU-test harness checkpoint — 2026-11-07

The shared `wgpu-test` package and `wgpu-info` now have opt-in `dx11`
features, so an all-features workspace test build compiles the D3D11 path. The
native GPU-test harness checks `WGPU_DX11_REQUIRE_HARDWARE=1` requires
`WGPU_BACKEND=dx11`, and that `.gpuconfig` includes a physical adapter whose
driver info identifies FL9_1. The capability test's required downlevel
properties allow the backend's documented border-sampler restriction, so it
is not silently classified unsupported.

On the RTX 3090 Ti, strict `.gpuconfig` generation recorded one DX11 adapter
(0x10de:0x2203, driver 32.0.16.2101). The filtered `validate_capabilities`
test was listed as `[Executed]` and passed; a run with `WGPU_BACKEND=dx12`
failed at the strict gate. This capability test initializes the public test
context but does not issue a draw; strict rendering evidence remains in
`dx11_public` and the private HAL suite. Broader test-harness execution,
public contract auditing, historical FL9_1 drivers, and Lumia remain open.


## Current continuation checkpoint — surface loss cleanup — 2026-11-07

The strict upstream harness and substantial public FL9_1 rendering/resource
coverage are present, but the mission remains incomplete. A callback-reentrant
present after device loss exposed an early `Surface::present` exit that retained
the consumed core frame. The focused public regression now passes after
releasing that frame on the lost-device path; the full surface fixture passes
2/2 on the RTX 3090 Ti forced to FL9_1. See `FL9_1_PROGRESS.md` for the exact
command and output.

Continue by auditing core-valid public operations against HAL behavior and
reachable failure paths, then broaden representative public workloads. The
current opt-in path is suitable for a limited prototype whose API usage stays
inside `CAPABILITIES.md`; it is not a general-purpose FL9_1 implementation.
Intrinsic FL9_1 limits (including no compute/UAV, geometry stage, 1D textures,
ordinary arrays or occlusion queries) remain unavailable. Historical FL9_1
drivers, Lumia/CoreWindow runtime and physical device-loss recovery also remain
unverified.

## Follow-up — optional RG8_UNORM sampled mip chains

R8G8_UNORM mip uploads and biased sampling now follow a conservative selected-
device gate: `TEXTURE2D | MIP` must be positively reported and the backend path
enabled; `SHADER_SAMPLE` controls filtering independently, and NPOT mips also
require unrestricted NPOT support. The synthetic capability test covers these
branches. The RTX 3090 Ti at forced FL9_1 reports `0x0002c320`, which lacks MIP,
so the strict public test logs an optional-capability skip and does not claim a
GPU mip result. The core fixture verifies that this query projects single-mip
behavior and conditionally allows creation. The fixture also now matches the
already implemented query-gated BC2/BC3 support and clears the NPOT mip
downlevel flag in its restrictive validation case. See `FL9_1_PROGRESS.md` for
the exact run results.

## Public queue ordering and empty render-bundle state — 2026-11-08

Strict public regressions now verify that an empty render bundle resets pass
state and that `Queue::write_buffer` after command encoding but before submit
supplies the values observed by the draws. Both readback tests ran at explicitly
requested and selected FL9_1 on the RTX 3090 Ti. The complete strict public
D3D11 target passed 88/88 with three optional device-specific skips. No new
backend functionality was added; the core/HAL contract audit and historical
hardware/runtime verification remain open. See `FL9_1_PROGRESS.md`. Commits:
`9e07459a3eee540c349c9e28cc6a864221c3c01f` and
`818fee943b117018cae30c813d17fa0e389c7669`.

## Current continuation — narrow `textureDimensions` — 2026-11-08

A separate downlevel limit now exposes a narrowly proven fragment
`textureDimensions` form using the existing host-supplied dimensions constant.
General `texture_query` remains disabled. The strict RTX test verified a
bind-group transition between differently sized views; the full private HAL,
public D3D11, DX11/DX12 Core, wgpu-types and wgpu-info checks passed. See
`FL9_1_PROGRESS.md` and `TESTING.md` for exact results. Historical hardware,
CoreWindow runtime and broad API/CTS verification remain open.

The follow-up that permits one compatible implicit sample alongside the direct
2D dimensions query is verified by the complete strict public D3D11 target:
97/97 passed on the RTX 3090 Ti at requested/actual FL9_1. Continue with the
next isolated operation only after its Core, Naga and HAL validation contracts
are audited. Public adapter exposure remains gated.

## Verified continuation — constant-mip 3D `textureDimensions` — `201f55b7b` — 2026-11-09

The existing narrow fragment 3D dimensions proof now accepts a nonnegative
literal or module-constant mip level when the selected entry point does not
also sample. Core, Naga and HAL reject dynamic and negative levels; the
level-less query may retain its previously verified single implicit sample.
HAL provides all three mip-adjusted dimensions through the existing `b13`
emulation constant. A strict RTX 3090 Ti FL9_1 test verified level 1 of an
8x4x8 three-mip volume as 4x2x4. This requires the independently reported
`texture_3d_mipmaps` capability for resources with multiple mips; no broader
query, copy or volume operation is implied. See `FL9_1_PROGRESS.md`.

Next, perform a fresh Core/HAL contract audit to select another bounded,
publicly reachable gap. Do not enable ordinary adapter enumeration by default.
Public exposure remains experimental; public CoreWindow on the Lumia,
historical FL9_1 hardware, broad shader/API support and complete unsupported
operation auditing remain open.

## Latest public execution proof — invariant position

The strict public test `dx11_invariant_position` verifies an invariant
position through a depth-only prepass and Equal-depth color pass. It passed
1/1 on the RTX 3090 Ti at explicitly requested/actual FL9_1, with all 64
readback pixels correct. This covers one shader/pipeline shape and does not
change the capability report. The full Core/HAL contract audit,
unsupported-operation closure and historical-device/runtime checks remain
incomplete; see `FL9_1_PROGRESS.md` for the exact command and hardware record.

## Completed contract-audit slice — sampler LOD compatibility — 2026-11-10

Added `DownlevelLimits::sampler_lod_clamp`, set false for FL9_1/9_2 and true
for other backend defaults. Core now rejects non-default LOD ranges combined
with distinct min/mag filters or anisotropy before calling HAL, and rejects
non-finite LOD values as validation errors. For same-filter isotropic
single-mip cases the existing HAL normalization remains available. This avoids
turning an unsupported sampler descriptor into an unexpected HAL device error
and device loss. HAL now exposes translated mip compatibility metadata to
Core. Bind groups retain actual selected-view mip counts, and both render-pass
and render-bundle draw validation check only the sampler/texture relationships
used by the selected pipeline. Rebinding rechecks current group metadata.

CPU tests cover the capability comparison/compliance bit, capability
projection at all seven feature-level identities, predicate boundaries,
finite LOD validation and selected-view compatibility. The strict Core fixture
also rejects a two-mip view with a sampler translated as single-mip-only,
before HAL draw execution, on the forced-FL9_1 RTX 3090 Ti. Exact results are
recorded in `FL9_1_PROGRESS.md`.

Next, continue the Core-valid-to-HAL contract audit and select a focused public
workload that expands useful FL9_1 rendering coverage. Do not enable ordinary
adapter enumeration by default. Historical FL9_1 drivers, Lumia/CoreWindow
runtime, physical device-removal recovery, broad API/CTS coverage and remaining
unimplemented operations remain open.

## Latest audit result — uncalled dynamic-loop helpers — 2026-11-10

A strict FL9_1 fixture compiled a safe selected vertex/fragment entry while
the Naga module contained uncalled regular functions with dynamic loops. FXC
accepted the pipeline on the RTX 3090 Ti forced to FL9_1, so this shape does
not require an added Core rejection. See `FL9_1_PROGRESS.md` for the command.
The previously identified optional viewport-remap over-rejection was fixed in
`173165df0`: pipeline creation retains a valid base shader when the optional
remap variant fails, and fractional-viewport draws are checked before HAL
execution. The next task should come from the remaining capability inventory
or a reproduced application workload, rather than revisiting this closed case.

## Current audit checkpoint — 2026-11-10

The strict public `dx11_public` target passed 119/119 on the RTX 3090 Ti at
requested/actual FL9_1, including depth-only and read-only depth bundle
execution. A strict cube-dimensions integration test also passes with one
parameterless query-only helper lowered through existing face-size metadata.
The public/Core/HAL audit fixed writable remapping of noncoherent `MAP_WRITE`
buffers so prior bytes survive a partial host edit and later GPU copy. A
proposed runtime integer-comparison exception for emulated `instance_index`
was rejected after review found it could alter FL9_1 semantics for large
values; production validation remains conservative. Continue with the
Core-valid-to-HAL audit, including remaining shader/resource and command/pass
boundary cases. The later audit also verified read-only D24S8 stencil bundles
and fractional-viewport bundles with strict FL9_1 GPU readback. Physical
historical FL9_1 devices, Lumia ARM32 CoreWindow runtime, and actual hardware
device-removal/recovery remain external gaps.

## Timestamp query contract — 2026-11-10

Do not re-enable public D3D11 timestamp features based on a startup frequency
probe. Core permits timestamp pairs and resolves to cross submissions, while
D3D11 only certifies timestamp reliability and frequency inside a disjoint
interval. A correct future implementation needs a public representation that
preserves interval validity and period, or a demonstrated invariant from the
runtime. Native timestamp queries currently remain diagnostic-only; focused
hardware, Core, HAL, and public validation is recorded in
`FL9_1_PROGRESS.md`.

## Coordinator support inventory — 2026-11-10

The opt-in native Rust adapter has substantial tested FL9_1 coverage: desktop
HWND presentation, multi-group uniform and sampled resources, 2D mip/view and
copy operations, query-gated cube/3D sampling and format usages, depth/stencil,
and selected multisample color/depth paths. It is useful for applications
whose exact workload stays within that verified subset. It is not a general
WebGPU adapter or complete implementation of all behavior nominally available
on a modern D3D11 device.

Keep these categories separate when planning work:

1. **Feature/profile boundaries:** compute, UAV/storage execution, geometry
   shaders, gather and explicit texture LOD are outside this FL9_1 profile or
   its tested contract. Do not try to make them green by inventing CPU
   fallbacks or advertising modern D3D11 capability.
2. **Current deliberate software limits:** general runtime integer shader
   operations, derivatives/gradients, arbitrary dynamic indexing, ordinary
   texture arrays, broad 3D copies/readback/attachments, and occlusion or
   pipeline-statistics queries remain unavailable. Some may admit narrow,
   workload-backed subsets; each needs Core proof, matching HAL checks, FXC
   evidence and strict pixel/query results.
3. **Device-dependent subsets:** texture formats/usages, volume and cube
   paths, multisample counts and optional IA formats are admitted only from
   selected-device queries. A format available on the RTX is not guaranteed
   on historical FL9_1 hardware.
4. **External verification:** the RTX 3090 Ti forced to FL9_1 is not a
   historical-driver simulator. Lumia 950 XL installation, CoreWindow public
   presentation, ARM32 GPU execution and physical device-removal recovery need
   external device/runtime access. Desktop HWND presentation is independently
   verified.
5. **Audit coverage:** separate read-only resource/transfer, Core validation,
   render/API and shader reviews found no new concrete Core-valid request that
   reaches an unsupported HAL operation in their reviewed scopes. The full
   Core-to-HAL contract audit is still incomplete.

Coordinator workflow: use agents for independent read-only inventories and
feasibility probes, then assign one end-to-end vertical slice at a time.
Core validation, Naga lowering and HAL execution for one shader feature are
coupled and should be integrated by one owner. The next code candidate should
come from a reproduced application workload or a strict compiler/runtime
probe, not from unverified feature-count expansion. Avoid simultaneous edits
to shared capability, validation and public-test files.

## Current shader-query increment — selected 3D/cube helper — 2026-11-10

The restricted `textureNumLevels` helper proof now supports selected 3D and
cube fragment entries, matching the existing query limits for each image
dimension. Core, Naga and HAL require the one argument-free straight-line
helper to return the query directly; they preserve same-image sampling limits
and reject dimension mismatches. Strict FL9_1 readback verified helper-only
and direct-query-plus-sample paths for both 3D and cube views on the RTX 3090
Ti. Helper-plus-sample for 3D/cube is covered by Core/HAL proof and FXC
compilation, not GPU readback. This adds no general texture-query capability.
The complete Core/HAL/public/Naga and DX12 Core suites pass after the change.
See `FL9_1_PROGRESS.md` for exact commands. Continue by selecting gaps from a
real workload or a separately reproduced compiler/runtime boundary; do not
expand general texture queries or infer historical-driver support from this
modern adapter.

## Coordinator continuation — bounded fragment uniform indexing — 2026-11-10

A narrow selected-entry fragment uniform-indexing subset now has independent
Core, HAL/Naga, and strict GPU evidence. It accepts direct indexing of a
`vec4f` uniform array of lengths 2 through 24 by a scalar `u32` field in the
same group-0 structure; the separate downlevel limit defaults false elsewhere.
The 24-element shader uses 25 of the 32 fragment constant vectors and FXC
rejects length 25 at constant register 32. Unsupported shapes remain
rejected. The RTX 3090 Ti forced to FL9_1 rendered indices 0, 1 and 23
correctly. Core (134), private HAL (471), public D3D11 (122), and DX12 Core
(123) suites pass; see `FL9_1_PROGRESS.md` for exact commands. This is a
concrete compiler-bounded subset, not broad runtime-integer support.

Continue agent-assisted work by delegating independent audits, selector
triage, and compiler feasibility probes, then integrating one cross-layer
Core/Naga/HAL change at a time. Do not split a single vertical shader feature
among agents editing shared validation or capability files. The remaining
implementation gaps and external historical-device/Lumia verification are
still open.

## Coordinator continuation — measured fragment uniform-array boundary — 2026-11-10

The direct selected-entry fragment uniform-indexing proof now accepts array
lengths 2–24, aligned across Core and HAL. Naga-generated HLSL/FXC evidence
shows length 24 uses 40 Level9 instruction slots and length 25 fails with the
constant-register-32 diagnostic; the test now asserts that diagnostic.
Strict forced-FL9_1 readback verified three selected values on the RTX 3090
Ti. Full Core, HAL, public D3D11 and DX12 Core tests, Clippy, formatting and
diff checks are recorded in `FL9_1_PROGRESS.md`. This remains one narrowly
proved integer-indexing shape. Continue the remaining mission through
independent read-only audit/probe tasks and one integrated code slice at a
time; do not infer broad API usability from this increment.

## Coordinator continuation — direct interpolated 2D `textureLoad` coordinates — 2026-11-10

Core and the D3D11 HAL now accept one additional exact shader shape: a direct
fragment entry point with one `@location(0) vec2f` argument converts that value
to `vec2i` and uses it as the coordinate of its sole single-mip 2D float
`textureLoad`. The proof excludes coordinate arithmetic, helper-based dynamic
coordinates, and any other runtime integer operation. Strict forced-FL9_1
readback on the RTX 3090 Ti verified 16 distinct source texels and 48
out-of-bounds zero pixels. Full Core, private HAL, public D3D11, and DX12 Core
regressions passed; exact commands are in `FL9_1_PROGRESS.md`.

This small improvement does not make the remaining runtime-integer or shader
subset general. Continue with a reproduced workload or compiler/runtime
boundary, using agents for independent audits and probes rather than splitting
one Core/Naga/HAL vertical change. Historical FL9_1 and Lumia/CoreWindow
runtime verification remain external requirements.

## Coordinator continuation — forwarded dynamic 2D `textureLoad` coordinate — 2026-11-11

The selected-entry fragment `textureLoad` subset now also accepts one exact
helper form: the entry forwards its `@location(0) vec2f` unchanged to one
straight-line helper, which casts it directly to `vec2i`, performs the only
single-mip 2D float load and returns that value directly. Core and HAL retain
the same narrow proof and require exactly one helper return; computed
coordinates, nested helpers, branches, multiple returns and additional runtime
integer operations remain rejected. The strict RTX test
checked all 64 pixels in four quadrants against distinct texels from a 2x2
source at requested/actual FL9_1. This does not generalize dynamic indexing or
establish historical-driver support.

## Core/HAL textureLoad view contract — 2026-11-10

A follow-up Core-to-HAL audit found that the selected-entry 2D `textureLoad`
shader proof did not validate the actual texture view. The private HAL
emulation is narrower: it supports only single-mip, single-sample
`Rgba8Unorm` 2D and 3D views. Core now stores the proved `(group, binding,
dimension)` on the render pipeline and compatibility metadata on each bind
group, then checks the currently selected pipeline and bound group before
direct or bundle draw execution. Incompatible views produce validation errors
before HAL; sibling entry points without `textureLoad` are unaffected. Strict
RTX validation rejects R8 and multi-mip 2D views, rejects a multi-mip 3D view,
and executes compatible and unrelated-entry paths. The full Core, public
D3D11 and DX12 Core regressions now pass after the generalized 3D guard: 134
Core fixtures, 122 public DX11 tests, and 123 DX12 Core tests. Strict hardware
tests used the RTX 3090 Ti at requested/actual FL9_1. Exact commands and
counts are in `FL9_1_PROGRESS.md`.

## Query-gated 3D texture-to-buffer readback — 2026-11-10

The selected FL9_1 RGBA8 3D path now accepts `COPY_SRC` only for an ordered
CPU shadow. This is not a native 3D GPU copy: the shadow is authoritative
because the currently exposed 3D resource can only be zero-initialized or
updated by ordered buffer-to-texture uploads; render, storage and texture-copy
writes remain unavailable. Core still reports 3D texture-to-texture copies as
unsupported. Strict RTX execution verifies full and partial readback, padded
rows/depth, nonzero offsets and zero-initialized texels. The cost is one CPU
allocation matching every mip in each `COPY_SRC` volume plus a temporary
region-sized allocation for each read. See `STATUS.md` for the current
capability boundary and `FL9_1_PROGRESS.md` for exact results.

Next, continue the bounded Core-to-HAL audit and workload/compiler probes.
Recent agent review found no substantiated Core-valid request that reaches an
unimplemented HAL operation in the audited areas; avoid speculative feature
expansion until another concrete mismatch or useful application workload is
reproduced. Historical FL9_1 driver behavior, Lumia ARM32/CoreWindow execution
and device-removal recovery remain external verification requirements.

## Coordinator synthesis — remaining FL9_1 support — 2026-11-10

Three independent read-only audits covered public/Core contracts, resources
and transfers, and shaders/render state. Together with the existing coverage,
they describe a usable opt-in graphics subset: constrained vertex/fragment
shaders, indexed and non-indexed drawing, a single color target, selected
depth/stencil and multisample paths, multiple uniform/resource bindings,
selected 2D/3D/cube formats and transfers, and query-gated optional features.
Strict public tests exercise many paths on the RTX 3090 Ti explicitly forced
to FL9_1; this is not a complete FL9_1 API/CTS proof.

Compute/UAV, geometry shaders, ordinary texture arrays, 1D textures, indirect
drawing, MRT, and broad shader operations remain unavailable. Some are
feature-level/profile ceilings; others are deliberately narrow compiler,
resource or validation subsets and require separate evidence before expansion.
The audit found no new confirmed Core-valid request that reaches an
unimplemented HAL operation in the reviewed device/command/resource/pipeline
paths, but surface/fence, shader/pipeline and capability-assertion contracts
are not exhaustively closed. Historical FL9_1 devices, Lumia ARM32/CoreWindow
runtime and physical device-removal/recovery remain external verification.

Use agents for independent, non-overlapping inventories or compiler probes.
Keep each Core/Naga/HAL/test implementation slice under one integrating owner;
do not assign agents to “finish” a broad hardware-limited feature family.
Continue with a concrete public-valid mismatch or a reproducible shader/API
workload, and report the unsupported surface separately from hardware
portability evidence.

The subsequent compiler-boundary check now enables only direct scalar fragment
`frexp(value).fract`, backed by validated Core/HAL proofs, FXC Level9 bytecode
inspection and positive/negative/zero GPU readbacks. Full results and other
integer semantics remain gated. Continue the Core/HAL contract audit and
select the next implementation from a reproduced workload or profile boundary;
do not broaden this narrow capability based on modern hardware alone.

## Selected-entry 2D `textureLoad` with one uniform — 2026-11-12

Completed one bounded workload increment: a single used uniform may modulate
the one selected fragment 2D float `textureLoad` result. Core, HAL and Naga
share the one-uniform limit; other used resources, multiple uniforms, 3D loads
with uniforms and combined load/dimensions with uniforms remain rejected.
Strict public GPU tests verified sparse group-0 and group-1 bindings at forced
FL9_1. Continue using independent agents for read-only audits or compiler
probes; keep subsequent Core/Naga/HAL changes coordinated. The next feature
should follow a reproduced application workload or a specific Core-to-HAL
contract mismatch rather than expanding this emulation speculatively.

## Coordinator update — eight-image fragment 2D textureLoad — 2026-11-12

Completed and verified support for up to eight selected-entry fragment 2D float
textureLoad images at forced FL9_1. The production FXC path compiles 1–8 image
representative shaders after removing redundant coordinate clamping; Core/HAL
reject image nine. Full strict HAL and public D3D11 suites passed 516/516 and
152/152, with actual RTX 3090 Ti readbacks included. Full Naga/Core DX11/Core
DX12 suites passed 171/171, 136/136 and 135/135. Exact commands and hardware
outputs are in `FL9_1_PROGRESS.md`.

Remaining support is not just polishing. Compute/UAV, geometry shaders, MRT,
ordinary texture arrays, 1D textures and indirect drawing remain unavailable at
FL9_1 or deliberately withheld. The public D3D11 backend remains opt-in and
its API surface is a constrained subset, not complete WebGPU/downlevel
conformance. Historical FL9_1 devices, Lumia ARM32/CoreWindow, and device-loss
recovery remain external verification gaps. Use read-only agents for
independent contract audits and bounded compiler probes, but keep every
Core/Naga/HAL change and end-to-end GPU proof with one coordinator. The proposed
nonzero-group 3D/cube query-sampler follow-up has since been implemented and
verified in the 2026-11-12 progress entry; it is no longer an open candidate.
Choose the next software slice from a reproduced application workload or a
confirmed Core-to-HAL contract gap, and require an end-to-end proof before
expanding a path.

## Coordinator update — six-uniform fragment 2D textureLoad — 2026-11-12

Core, Naga and HAL now allow up to six used uniform buffers in the selected
fragment 2D float `textureLoad` proof and reject a seventh. The allocator remains
the normal compact uniform-register path, independent of sparse logical binding
numbers, while the internal dimension buffer stays reserved. Strict public GPU
readback verified both group-0 and group-1 layouts with six uniforms as
`[64, 96, 32, 223]` for every pixel. Full regressions passed: Naga 171, Core
DX11 136, Core DX12 135, private HAL 516 and strict public D3D11 152. This is
still one explicit emulation proof, not a general limit on all shader uniforms.

## Coordinator update — constant-buffer slots and surface readback — 2026-11-12

The public projection had split thirteen user constant-buffer slots in half and
exposed six per shader stage/twelve dynamic bindings. D3D11 vertex and pixel
constant-buffer registers are independent, while the backend reserves `b13`
and maps thirteen logical uniforms to `b0`–`b12`. The selected projection now
exposes thirteen uniforms per stage and thirteen dynamic uniforms per pipeline
layout, still bounded by the shared thirteen-binding allocator. A strict public
forced-FL9_1 draw used all thirteen fragment registers and matched all 64
readback pixels; Core rejects fourteen static or dynamic bindings before HAL.
FXC's independent 255 vertex and 32 pixel float4 constant budgets remain.
Special texture-load proofs retain their own narrower uniform limits.

The selected BGRA8 format projection now also controls HWND/CoreWindow surface
`COPY_SRC`. A strict RTX 3090 Ti HWND test rendered an acquired FL9_1 frame,
copied it before present with a nonzero destination offset and padded rows,
verified all 91 pixels and untouched sentinels, then presented the same frame.
CoreWindow/UWP behavior remains unverified.

## RGB565 multisample resolve — 2026-11-12

Added a query-gated RGB565 FL9_1 multisample render/resolve path. Initial HAL
execution found two implementation gates beyond capability projection: the
multisample initialization clear accepted only RGBA8/BGRA8, and pipeline
validation hard-coded RGBA8. Both now use the restricted packed-format support
and selected-device resolve query. On the RTX 3090 Ti, forced FL9_1 reports
RGB565 quality levels for 2x/4x/8x but not 16x. The strict HAL clear/resolve/
fence/readback test passed 64/64 packed red pixels. The public draw/resolve/
sample/readback test passed all 64 pixels at 2x, 4x and 8x. Multisampled RGB565
blending, alpha-to-coverage and direct multisampled sampling remain unverified.

## Surface unconfigure timeout recovery — 2026-11-12

A public/Core-to-HAL audit found a real cross-device surface transition gap:
Core removed the old presentation before D3D11 had completed a pending-present
wait, and the HAL contract could not return an unconfigure timeout. The HAL now
provides an additive fallible unconfigure hook; D3D11 reports failures, and
Core preserves the previous presentation association until unconfigure
succeeds. The strict HWND regression injects a one-shot timeout after an actual
present, confirms the previous configuration remains, then completes the present
and reconfigures onto a second FL9_1 device. The timeout is synthetic. Exact
command and adapter details are recorded in `FL9_1_PROGRESS.md`.
# Coordinator update — surface lifecycle and remaining work — 2026-11-12

The surface-unconfigure path now propagates `Device(Lost)` after retiring the
lost swapchain. Core defers callbacks for both unconfigure and replacement
configure errors until queue/surface locks are released. Strict synthetic
device-loss tests reentered `Device::poll` without deadlock; real HWND surface
present/resize behavior remains covered separately. The full current-tree
surface, Core DX11 fixture, HAL, public DX11 and Core DX12 suites passed 5/5,
150/150, 525/525, 157/157 and 135/135 respectively. See
`FL9_1_PROGRESS.md` for exact commands and the explicit GPU/software split.

Use specialist agents for read-only feature-contract audits, FXC probes and
CTS triage, with one coordinator owning a cross-layer Core/Naga/HAL change.
The next software priority is the feature-to-validation-to-HAL-to-test matrix
for the advertised subset, followed by curated CTS selectors for those exact
capabilities. Do not treat the broad renderer as complete wgpu support.
Profile/hardware limits still exclude compute/storage, geometry shaders,
multiple color targets, ordinary 1D/array resources, indirect drawing and
occlusion queries. Historical FL9_1 devices, Lumia/CoreWindow runtime and
physical device-removal recovery remain external verification work. Core queue
teardown and surface reconfiguration use bounded waits. The legacy private-HAL
`wait_for_idle` remains blocking for direct callers, but Core invokes queue
destruction only after a bounded wait confirms idle; no current Core route was
found that reaches the blocking call with a pending present. Changing the
direct-call semantics remains a separate contract decision.

## Coordinator follow-up — agent triage — 2026-11-12

Read-only shader, texture, and render/API follow-ups found no confirmed
Core-to-HAL mismatch in their reviewed areas. The concrete candidates are a
compiler-probed local matrix-struct store subset, costly CPU-shadow
materialization for non-suffix 3D mip views, and cube/3D plus curated CTS
coverage. Do not assign each as a broad parallel coding task: profile-level
exclusions are not software tasks, and any shader expansion spans Naga, Core,
HAL, and public tests. The next coordinator action is an isolated HLSL/FXC
probe for the local matrix-struct candidate; accept it only with strict public
FL9_1 readback and preserved negative boundaries. The read-only audits did not
run tests or change implementation.

# Coordinator update — matrix-store fix verified — 2026-11-12

The HLSL/FXC probe found and confirmed that generated split-matrix setters
passed a local struct by value. Naga now uses `inout` for those setters and the
generic split-matrix helpers. A narrow direct local-store proof is enabled in
Core and HAL for a direct `matCx2<f32>` member and a sibling `f32`; nested,
array, dynamic and helper-pointer stores stay rejected. Strict FL9_1 hardware
readback verified the public path. See the progress journal for exact
commands and outcomes. The next software step is capability-projection/CTS
matrix closure, with cube/3D verification ahead of any costly non-suffix 3D
view emulation. Historical hardware and excluded FL9_1 operations remain
outside that software matrix.

## Coordinator update — vertex-indexed matrix columns — 2026-11-13

The direct local matrix-store exception now includes one validated runtime
case: a straight-line vertex entry point may write a direct `matCx2<f32>` field
column selected by `@builtin(vertex_index)`. Naga uses signed conditional
helpers because FXC rejects the unsigned switch on the FL9_1 profile. Core and
HAL have matching shape checks, and strict public readback verifies all 64
pixels. Dynamic row, nested/array matrix, and helper-pointer stores remain
blocked; `matrix_aggregate_stores` remains false. The next priority remains a
systematic advertised-capability/Core/HAL/CTS evidence matrix. A good use of
agents is independent, read-only category audits and selector triage; keep
cross-layer changes owned and integrated by one coordinator.

## Coordinator update — computed LOD with transformed coordinates — 2026-11-13

The single-helper fragment `textureSampleLevel` proof now accepts both a
bounded computed LOD and the same validated coordinate-expression subset in
one helper. Core and HAL retain matching fail-closed checks; the HAL FXC test
compiles the lowered shader for `ps_4_0_level_9_1`. A strict public draw on the
physical RTX 3090 Ti requested and confirmed FL9_1 and matched all 64
readback pixels against the equivalent one-mip transformed sample. This does
not enable general explicit LOD, multiple mips, nested helpers or arbitrary
shader expressions. The next priority remains the advertised-capability to
Core/HAL/test evidence matrix, with one coordinator integrating any
cross-layer changes.

## Coordinator update — stage-specific uniform registers — 2026-11-13

A Core/HAL contract audit found that Core's thirteen-uniform-per-stage
projection was wider than the HAL's thirteen-register aggregate allocator.
The HAL now allocates constant-buffer slots independently for vertex and
fragment stages, permits up to twenty-six total static uniforms when each
stage uses at most thirteen, and maps stage-visible bindings through the same
plan used by Naga/FXC and command replay. A logical uniform visible to both
stages can use different native slots when the other stage has preceding
stage-only bindings. Core's thirteen dynamic-uniform-per-pipeline limit remains
separate. Strict RTX 3090 Ti FL9_1 rendering used thirteen distinct uniforms
in each stage and all sixty-four pixels included both results. No other
contract mismatch was confirmed by the scoped audit; the broader evidence
matrix remains incomplete.

# Coordinator update — multi-mip literal-zero sample views — 2026-11-13

The selected fragment `textureSampleLevel(..., 0.0)` proof now permits
multi-mip D2 views by copying the selected view-relative mip zero into a
one-mip sampled texture before each affected draw. The first direct base-only
SRV design violated the 10Level9 requirement that an FL9_1 SRV include the
resource's smallest mip and reproducibly lost the device; it was replaced with
a default-resource copy using an explicit source box and zero destination
offset. Strict RTX 3090 Ti FL9_1 readback now covers full and nonzero-base
views, an upload after view creation, and pipeline transition to minified
implicit sampling. General explicit LOD is still unavailable.

The backend is usable only as an opt-in, constrained graphics subset. Software
work remaining is primarily evidence closure: finish the advertised capability
→ Core validation → HAL operation → test/CTS matrix, then add only confirmed,
workload-backed gaps. Scoped audits have found no new confirmed public-valid
request reaching an unsupported HAL path, but are not exhaustive. Profile and
hardware exclusions remain compute/UAV/storage, geometry shaders, multiple
color targets, ordinary 1D/array textures, indirect draws and FL9_1 occlusion
queries. Historical 9_1 drivers, Lumia ARM32/CoreWindow execution and physical
device removal/recovery remain external verification gaps.

Use specialists for independent contract reviews, FXC probes, and strictly
scoped CTS selectors. Do not assign one agent to each broad feature family:
shader/API changes span Core, Naga, HAL and GPU tests and need one integration
owner. The coordinator keeps the cross-layer implementation, merges evidence,
runs the combined regression suite, and makes the checkpoint commit.

## Coordinator update — computed fragment gather-helper coordinates — 2026-11-13

The bounded coordinate-expression grammar now applies inside the one allowed
fragment gather helper while its call argument remains unchanged. Core, HAL
and Naga proofs plus strict FL9_1 GPU readback passed. General gather remains
disabled; this is only the existing emulated one-gather subset. Continue next
with the advertised-capability/Core/HAL/evidence matrix. Use agents for
independent audits, shader/profile probes and narrowly scoped CTS runs, while
one coordinator owns changes spanning multiple crates and their integration
tests.
