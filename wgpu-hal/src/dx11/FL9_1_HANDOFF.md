# FL9_1 handoff

## Coordinator status — 2026-11-14

The latest committed code checkpoint is `cb6e8c0b583e5f327104fb39451b96997c864d4a`
(`dx11: add query-gated FL9_1 R8 3D textures`); the latest committed HEAD is
`1e623946cb316ad69a0b834b736347df950f67fc`, which records its verification.
The working tree contains a verified test-only addition in
`wgpu/tests/dx11_public.rs`, currently uncommitted under the workspace's
no-agent-commits instruction. It adds strict GPU coverage for the nonindexed
LineList chunk boundary. The R8 change adds query-gated R8 3D textures alongside RGBA8.
Format-sized storage now covers volume creation,
views, zero initialization, upload, ordered CPU-shadow copies/readback and
sampling. R8 remains excluded from the RGBA8-only 3D `textureLoad` emulation.
The previous committed base is `0f9fed45a` (`dx11: allow alpha-equivalent
coupled blend states`), which preserves rejection of genuinely separate-alpha
equations and validates accepted alpha-equivalent factors in both Core and HAL.

Full strict suites on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) explicitly requested and confirmed FL9_1 passed: public
`dx11_public` 171/171, private HAL 545/545, and Core fixture-enabled tests
155/155. The public LineList boundary test rendered six expected red pixels
after the 65,536-vertex split, with every other target pixel blue. The new
public R8 transfer test and the Core R8 format/view fixture
ran within those suites. The focused `wgpu-types` volume-shape test passed
1/1. Selected-device optional format cases may still log a prerequisite skip;
they are not counted as independent hardware successes. See
`FL9_1_PROGRESS.md` and `FL9_1_CTS.md` for prior check results.

The latest additional validation passed the strict public `mapAsync,read,*`
CTS selector 18/18 and `mapAsync,write,*` 10/10, both with zero skips. The
public wgpu regression suite passed 171/171 after adding the LineList boundary
readback. The Lumia Device Portal request timed out in this session, so the
CoreWindow smoke run could not be attempted on the phone.

The implemented subset covers desktop offscreen and HWND rendering, textured
depth-tested draws, tested multi-group resource patterns, uniforms and dynamic
ranges, selected texture formats/views/transfers, samplers, and query-gated
depth and multisample paths. ARM32 UWP library checking and a minimal explicit
DX11 consumer link passed using Rust 1.93.1 `build-std`; neither AppX activation
nor Lumia/CoreWindow GPU execution has occurred.

### Remaining support work

- **Implementation and validation:** the public capability-to-Core-to-HAL and
  curated CTS matrices remain incomplete. Scoped audits found no concrete
  public-valid request reaching a known-unsupported HAL path in texture/view/
  copy and pipeline descriptor areas, but coverage is not exhaustive. The R8
  3D path is now projected only when the selected format query and backend
  implementation both permit it; a public strict test verifies upload, copy
  and padded readback. Expand only against a concrete API contract or workload,
  with matching Core/HAL validation and GPU evidence. The separate-alpha
  concern is already guarded by Core/HAL validation and public regression
  tests. The D3D11 10Level9 `CreateBlendState` table lists FL9_1 restrictions
  without restricting ordinary blend operations; that method contract is
  relevant to D3D11 level-9 use, while the legacy D3D9 minimum-cap table's
  optional `D3DPMISCCAPS_BLENDOP` bit is not a D3D11 blend-state query. RTX
  execution covers all five operations; historical-driver execution remains
  external.
- **Shader and resource boundaries:** compute/UAV/storage execution, geometry
  shaders, multiple color outputs, indirect drawing, occlusion queries,
  ordinary 1D and 2D-array textures, vertex texture sampling, and broad WGSL
  semantics remain unavailable. Exact profile/resource restrictions must
  continue to fail safely; do not force-enable them from RTX behavior.
- **Application fit:** the production Horizon renderer is in the sibling
  `HorizonWiiU` checkout. Its Windows target currently selects DX12, and its
  terrain path relies on 2D-array sprite textures plus gradient/explicit-LOD
  sampling. That path does not fit the current FL9_1 projection. A small
  2D-textured mesh smoke test would prove app selection/presentation only;
  terrain needs an application-side atlas/shader route or a separately
  justified generic emulation.
- **External verification:** historical FL9_1 drivers, Lumia ARM32/CoreWindow
  runtime behavior, and physical TDR/device-removal recovery remain untested.
  The modern RTX forced to FL9_1 does not substitute for these systems.

The 2D copy CTS cases for R8/BGRA8/RGBA8 and their supported linear/sRGB pairs
passed 6/6 on DX11, DX12 and Vulkan. Query-gated `Rg16Snorm` was skipped because
the format tier is not projected; it is not a pass. Broad selectors that
include unsupported array or mip subcases remain excluded when the runner
cannot isolate the supported cases.

The latest independent contract review found no concrete public-valid request
reaching a known unsupported HAL path among the reviewed compute, indirect,
ray-tracing, pipeline-cache and query gates. The direct-HAL
`tlas_instance_to_bytes` helper still panics, but Core gates it behind the
unexposed ray-query feature. The CoreWindow source review found the dispatcher,
owner-thread and lifetime checks present; actual Lumia runtime behavior remains
unknown. Keep using agents for bounded audits and CTS triage; the coordinator
owns cross-layer implementation, strict GPU runs and commits, and serializes
hardware tests.

## Coordinator status — 2026-11-14

The latest verified code checkpoint is `e52cb32ea` (`dx11: allow a uniform
with direct FL9_1 texture gather`); `1f4b637f4` records its evidence. This
bounded Core/HAL/Naga proof permits one used `vec4f` uniform alongside a
direct selected fragment gather while preserving the emulation's reserved
registers. Its strict public GPU test checked all 64 pixels on the RTX 3090 Ti
at requested and actual FL9_1.

Latest suite totals are 156/156 public D3D11 test functions, 533/533 private
HAL tests, 152/152 Core DX11 fixture tests, 175/175 Naga tests and 136/136
targeted Core DX12 tests. These are test-function counts, not counts of GPU
renders; optional device/format subcases can log explicit SKIP. The public
suite includes the check that DX11 remains absent from default backend
selection. Explicit DX11 selection is available experimentally with a
conservative FL9_1 projection.

The backend remains **PARTIALLY COMPLETE** against the autonomous mission.
It is usable for constrained native desktop rendering through public wgpu,
but it is not a complete FL9_1 implementation. Broader graphics and shader
subsets, remaining Core/HAL validation and error-path audits, capability/CTS
coverage, historical FL9_1 devices, Lumia ARM32/CoreWindow runtime, and
physical device-removal recovery remain open. Native FL9_1 exclusions such as
compute/UAV execution, geometry shaders, multiple color outputs, indirect
drawing and occlusion queries remain unavailable and should be rejected
accurately.

## Earlier coordinator status — surface loss routing — 2026-11-12

The cross-device surface lifecycle now reports pending-present timeouts and
device loss through an additive fallible HAL unconfigure operation. Core keeps
the prior Presentation on timeout or recoverable error, retires it only after
the HAL says the lost swapchain was discarded, attributes loss to the old
device, and defers device-lost callbacks until surface and queue locks are
released. The replacement-device configure error path also defers callbacks.
Strict RTX 3090 Ti tests exercised synthetic unconfigure and configure loss;
the callback reentered `Device::poll` successfully. Actual device removal is
still unverified.

Current reruns passed the public surface error fixture 5/5, Core DX11 fixtures
150/150, private D3D11 HAL 525/525, public D3D11 157/157 and Core DX12 135/135.
The GPU-backed suites required the physical RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) and explicitly requested/asserted FL9_1. Device-loss
errors were injected; the other graphics cases performed real GPU work. HAL
Clippy completed with existing warnings; Core and public fixture Clippy also
completed without errors. Exact commands are recorded in `FL9_1_PROGRESS.md`.

Support remains a constrained public graphics subset, not near-complete wgpu
compatibility. FL9_1/profile limitations or withheld paths include compute and
storage, geometry shaders, MRT, ordinary 1D/array resources, indirect draws,
occlusion queries, vertex texture sampling and selected shader operations.
Implementation work remains in broader shader lowering and query-gated
format/usage combinations. Validation coverage is not an exhaustive Core to
HAL proof; curated CTS selectors and matrix coverage remain. Platform evidence
still lacks historical FL9_1 hardware, Windows 10 Mobile/CoreWindow runtime,
and physical device-removal/TDR recovery. The desktop HWND path has real
presentation coverage. Core teardown and surface unconfigure use bounded
waits, but the legacy blocking private-HAL `wait_for_idle` can still wait on a
pending present without a deadline. Read-only agents are effective for category
audits and CTS triage; keep each cross-layer feature slice coordinated by one
owner.

## Earlier coordinator checkpoint — queue errors and API audits — 2026-11-12

## Adapter exposure clarification — 2026-11-12

Explicit `Backends::DX11` selection exposes only physical adapters that
successfully create and confirm FL9_1 and pass the conservative capability
projection. DX11 is not in the default instance backend set. The test named
`experimental_dx11_is_not_enabled_by_default` checks only this default
isolation; it does not assert that explicitly selected DX11 enumeration is
empty. Older handoff entries saying public enumeration is disabled are
historical.

## ARM32 UWP cross-check — 2026-11-12

The current worktree passed `wgpu-hal` and public `wgpu` checks for
`thumbv7a-uwp-windows-msvc` using nightly `build-std`. This is compile-only
evidence. The configured Lumia Device Portal request timed out; Windows 10
Mobile CoreWindow runtime behavior remains unverified.

## Render-pass CTS capability boundary — 2026-11-12

Strict raw `render_pass,*` CTS ran 183 cases: 3 passed, 41 skipped and 139
failed. Failures are from subtests requiring multiple color targets, array or
multi-mip textures, or unsupported texture copy usages/formats. The valid
single-layer/single-mip RGBA8 store subcase ran; its sibling unsupported
subcases caused the aggregate CTS failure. Scoped strict selectors passed
clear/readback 1/1, ordered buffer-copy transitions 1/1 and color-write masks
16/16, buffer-to-texture upload 1/1 and RGBA8 texture-to-texture copy 1/1.
This did not identify a supported render-pass regression. See `FL9_1_CTS.md`
for the source-backed breakdown.

## Current verified increment — computed `textureLoad` helper coordinates — 2026-11-12

One selected fragment 2D float `textureLoad` helper may now apply the
documented bounded literal-based f32 coordinate expressions before converting
to `vec2i`. Core/HAL use matching proofs and FXC compiles the exact shader
profile. Two strict public RTX 3090 Ti tests passed at requested/actual FL9_1:
group 0/binding 7 and group 1/binding 42. Both checked all 64 output pixels,
including shifted samples and out-of-bounds zero. Focused Core and HAL tests
passed. Full regressions passed Naga 171/171, Core DX11 136/136, Core DX12
135/135, strict private HAL 517/517 and strict public D3D11 153/153. This does
not make general dynamic coordinates or integer arithmetic available. See
`FL9_1_PROGRESS.md` for commands and evidence.

## Latest verified increment — uniform-backed 3D `textureLoad` — 2026-11-12

The selected single-image fragment 3D `textureLoad` proof now permits up to
six used uniforms using the existing compact register allocator. The
group-1/binding-42 tint case executed on the physical RTX 3090 Ti at requested
and actual FL9_1 and matched `[127, 0, 64, 255]` across all 64 output pixels.
Synthetic Core, Naga and HAL checks reject seven uniforms; combined
load/dimensions with uniforms and unrelated resources remain unsupported.
Strict mode was also applied to four HAL queue/command/fence GPU tests that
previously could silently skip when hardware was absent. Current exact commands
are in `FL9_1_PROGRESS.md`.

## Latest validation — separate-group volume query samplers — 2026-11-12

The existing narrow 3D/cube `textureDimensions` and `textureNumLevels`
query-plus-same-image-sample paths now permit texture and sampler bindings in
different supported groups. Strict RTX 3090 Ti FL9_1 output and the full
affected CPU/GPU suites passed. This is only a logical group-placement
extension; the selected-entry proof and sampler/resource restrictions remain.
See `FL9_1_PROGRESS.md` for the exact run record.

## Latest validation — public Depth24Plus constant bias — 2026-11-12

Commit `df648a5a9` adds a strict public wgpu test that uses an attachment-only
Depth24Plus resource and checks zero, positive and negative constant bias.
All 64 readback pixels passed on the physical RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, `software=false`, requested/actual FL9_1). The two
filtered 24-bit depth-bias CTS selectors each failed 3/3 because their fixtures
request unsupported depth `COPY_SRC`; the black error-resource outputs do not
measure the valid render-attachment path. The complete strict public suite
passed 146/146. Details and exact commands are in `FL9_1_PROGRESS.md`.

## Latest validation — depth comparison CTS — 2026-11-12

Strict filtered CTS coverage passed 24/24 each for D16, Depth24Plus and
Depth24PlusStencil8 depth comparisons (72 total) on the physical RTX 3090 Ti
at requested/actual FL9_1, with no skips or failures. The CTS build exposed
two shared `DownlevelLimits` fields missing from the GLES adapter initializer;
both are now conservatively false there, and `wgpu-hal --features gles`
checks successfully. This did not widen GLES or D3D11 capabilities. Commands
and exact selectors are recorded in `FL9_1_PROGRESS.md` and
`FL9_1_CTS.md`.

## Latest verified increment — 2026-11-12

The narrow fragment `textureSampleLevel(..., 0.0)` subset now supports one
straight-line helper that computes its `vec2f` sample coordinate from the
helper's single `vec2f` input using literal-based arithmetic, composition,
splat or swizzle. The exact-zero LOD, direct D2 float texture and regular
sampler, single helper call/sample, single-mip bound view, equal min/mag and
anisotropy-one restrictions remain. Core and HAL validate the same bounded
expression shape. The Core bundle path now checks the live view too; a
hardware-backed fixture rejects multi-mip and accepts one-mip bindings.
Strict RTX 3090 Ti FL9_1 public readback verified the transformed output for
all 64 pixels. General `textureSampleLevel` remains unavailable. Full
regression results are in `FL9_1_PROGRESS.md`.

## Latest verified increment — 2026-11-12

The backend now exposes only the restricted fragment `textureGather` subset:
one direct operation on a single-mip `Rgba8Unorm` 2D view using a regular
clamp-to-edge sampler. Naga emulates gather using four point samples, and Core
validates the selected entry plus the live view and sampler before draw.
The physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) executed the
public test at requested/actual FL9_1 and matched `[151, 166, 106, 91]` for
all 64 pixels. The general `texture_gather` limit remains false.

Validation passed Naga 169/169, wgpu-types 118/118, Core DX11 136/136, Core
DX12 135/135, strict private HAL 514/514 and strict public D3D11 149/149; the
public suite had zero ignored Rust tests and three optional capability
subcase skips. Six existing HAL Clippy warnings remain. Exact commands and
diagnostics are in `FL9_1_PROGRESS.md`.

## Current mission status — 2026-11-12

The latest verified software slice permits narrow fragment 2D
`textureDimensions` and `textureNumLevels` queries to sample the same image
through a compatible sampler in a different supported bind group. D3/cube
sampler-group restrictions remain unchanged. The direct and helper forms
passed strict forced-FL9_1 readback on the physical RTX 3090 Ti. A separate
test also verifies a comparison sampler in a different group for the existing
constant-offset proof. The latest integrated validation passed the complete
strict public D3D11 suite 148/148, private HAL 512/512, Core DX11 133/133,
Core DX12 132/132 and Naga 167/167.
The public offscreen textured indexed cube verified 1,024 pixels, and the
public HWND example presented five textured depth-tested frames. The prior
vertex-only f32 `mat4x4` inverse lowering remains available. Exact commands,
device details, optional skips and lint warnings are recorded in
`FL9_1_PROGRESS.md`.

An earlier increment accepts query-gated `Bgra8Unorm` and `Bgra8UnormSrgb`
views in the narrow fragment 2D `textureLoad` subset. The strict RTX 3090 Ti
FL9_1 run verified BGRA channel order and sRGB decode for all 128 output
pixels. Full strict regressions passed: private HAL 510/510, Core DX11
132/132, Core DX12 131/131 and public D3D11 144/144, with three optional
format/input subcase skips and no hardware skips. Targeted Clippy, formatting
and diff checks passed; see the latest `FL9_1_PROGRESS.md` section for exact
commands and pre-existing warnings.

The latest verified change allows selected 3D and cube `textureDimensions`
queries and helpers to use nonzero bind groups while preserving the group-0
sampler restriction. Strict forced-FL9_1 readback on the physical RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) passed direct/helper view-transition
tests, including cube mip-1 output. Naga passed 164 tests, Core DX11 132,
Core DX12 131, private HAL 510, focused public nonzero-group cases 5, and the
full public target 143. Three optional capability subcases logged skips; no
hardware-required test skipped. Clippy, formatting and diff checks passed.
Exact commands and existing warnings are recorded in `FL9_1_PROGRESS.md`.

The explicit `Backends::DX11` public path is available with a conservative
FL9_1 projection; default backend selection excludes DX11. This supports
constrained opt-in applications, not arbitrary wgpu workloads. The HWND
presentation path now uses DXGI's frame-latency-waitable object, tracks pending
presents and drains them from queue idle and unconfigure; strict hidden-HWND
tests exercise presentation, resize and teardown. CoreWindow behavior and
Lumia ARM32 remain externally unverified. The earlier 3D RGBA8 copy path
remains a CPU-shadow fallback and cannot copy GPU-written 3D resources.
Historical FL9_1 drivers and physical device-removal recovery remain external
verification gaps.

This is a substantial constrained opt-in graphics subset, not feature-complete
FL9_1 wgpu. The explicit DX11 backend supports public wgpu adapter/device
creation, offscreen rendering and desktop HWND presentation; default backend
selection still excludes DX11. The public textured, depth-tested cube examples
and strict GPU readback tests exercise the public path. Recent separate
reviews found no confirmed Core-valid request that reaches an unsupported HAL
operation in their reviewed scopes, but the complete API/CTS contract audit is
still open. Remaining software breadth includes conservative shader subsets
and 3D render/storage paths; 3D texture copies are available only through the
CPU-shadow fallback. Compute/UAV, geometry shaders, MRT, indirect
execution, occlusion queries, 1D textures and ordinary texture arrays are
FL9_1/profile or native API exclusions, not a software backlog to bypass. The
shared CTS lists do not track DX11-specific results; the curated inventory is
in `FL9_1_CTS.md`.

Historical FL9_1 drivers, Lumia ARM32/CoreWindow execution and physical
device-removal recovery remain external verification gaps. Use agents for
independent audits, CTS triage and compiler probes, but assign each coupled
Core/Naga/HAL feature to one implementation owner and keep end-to-end GPU
validation under the coordinator. The matrix-inverse subset currently covers
f32 mat2 and mat3 in both shader stages and f32 mat4 only in the vertex stage.
General matrix inverse remains rejected.

## Current coordinator checkpoint — query-gated Rg16Snorm texture load — 2026-11-11

The selected-entry fragment 2D `textureLoad` proof now permits query-gated
`Rg16Snorm`, preserving its narrow selected-resource proof while excluding 3D
loads and sample-bias-offset. Strict forced-FL9_1 GPU readback passed on the
RTX 3090 Ti: four SNORM quadrants matched across 64 pixels, and 64
out-of-bounds loads returned zero. The strict HAL suite passed 509/509 and
the complete strict public target passed 133/133; Core DX11/DX12 and the
focused format-matrix tests passed, and affected Clippy, formatting and diff
checks passed. Exact commands and driver details are in `FL9_1_PROGRESS.md`.
This does not establish historical-driver support. The current tree remains
suitable for constrained opt-in applications, not arbitrary wgpu clients;
see the mission plan and capability matrix for hard FL9_1 limits and remaining
integration evidence.

## Current coordinator checkpoint — query-gated sRGB 2D `textureLoad` — 2026-11-11

The narrow selected-entry fragment 2D `textureLoad` path now accepts a
single-mip `Rgba8UnormSrgb` view when the existing selected-format projection
permits sampled/upload usage. Core checks the actual bound view; the HAL
keeps this allowance out of 3D loads and sample-bias-offset. Strict RTX 3090
FL9_1 GPU readback converted `[128, 128, 128, 128]` sRGB storage to linear
`[55, 55, 55, 128]` across 64 pixels. Core passed 128/128, the strict private
HAL suite 509/509, the focused strict public test 1/1, and the full strict
public target 132/132; Core DX12 passed 128/128 and default DX11 isolation
passed 1/1. Affected Clippy, format and diff checks passed. Historical-driver
behavior remains unknown.
See the latest `FL9_1_PROGRESS.md` entry for exact commands and warning
details.

## Current coordinator checkpoint — nonzero-group 2D query helper — 2026-11-11

The current working change widens only the narrow 2D `textureDimensions`
helper proof to allow any texture bind group. Core, HAL and Naga still require
one argument-free, straight-line query helper with no other used resources;
3D/cube helper paths and associated samplers remain group-0-restricted. The
GPU test used group 7/binding 42, transitioned a 16x8 view to an 8x4 view,
and matched all 64 pixels. Strict private HAL passed 509/509, strict public
`dx11_public` 132/132, Core DX11 fixtures 139/139, Core DX12 128/128, and
HLSL-enabled Naga 163/163. Clippy passed for all affected targets with six
pre-existing HAL warnings; formatting and diff checks passed. Commit
`19322e6e1` checkpoints the implementation. Historical FL9_1 drivers remain
unverified.

The public offscreen and HWND examples also build and run in this checkout.
The offscreen run validated all 1,024 pixels of a textured indexed,
depth-tested cube. The visible HWND run presented 60 frames. Both used the
physical RTX 3090 Ti at requested/actual FL9_1; exact commands and results are
in `FL9_1_PROGRESS.md`.

The overall mission remains PARTIALLY COMPLETE. Remaining work is the broad
public Core/HAL contract audit, useful shader/resource subsets where evidence
supports expansion, the desktop/HWND lifecycle edge audit, and external
historical FL9_1, Lumia/CoreWindow and physical device-removal verification.
Compute/UAV, geometry shaders, MRT, 1D textures and ordinary texture arrays
remain feature-level restrictions. Keep agents on independent audits/probes
and each implementation slice under the coordinator.

## Current coordinator checkpoint — 2026-11-11

The newest verified software slice extends the narrow fragment 2D
`textureLoad` emulation to query-gated `R8Unorm` and `Rg8Unorm`. Core and HAL
check format compatibility per selected operation; R8/RG8 are allowed only
for 2D load and load-plus-dimensions, not 3D load or sample-bias. The selected
format must be sampled-upload capable, but filterability is unnecessary
because the emulation samples at texel centers with an internal point sampler.
Core rejects both full multi-mip views and one-mip subviews from multi-mip
parent resources before HAL, matching its resource restriction. Strict
RTX 3090 Ti readback at requested/actual FL9_1 verified R8/RG8 channel
semantics and out-of-bounds results. Broader regression results and the exact
hardware report are in `FL9_1_PROGRESS.md`.

## Current coordinator checkpoint — 2026-11-11

The selected RTX query-gates sampled/upload mip chains for RGB5A1 and RGBA4.
Strict forced-FL9_1 GPU coverage uploaded mip 1, copied a partial region,
verified padded raw-shadow readback and sampled a single-mip view for each
format. Both returned `TEXTURE2D | MIP` in mask `0x000213A0`. This adds no mip
generation or render-target support and does not establish historical-driver
behavior.

RGB5A1 and RGBA4 now expose query-gated texture-to-buffer readback through
their execution-ordered packed-word CPU shadows. Strict forced-FL9_1 tests
verify upload, partial GPU copy, sample, raw source/destination words,
preserved borders, padded rows and sentinels. The full private HAL suite
passed 508/508 and the full public target 131/131. This is not native staging
readback; historical hardware remains unverified.

Current format slice: Rg16Snorm sampled/upload mip chains require the selected
device's positive `TEXTURE2D | MIP` query plus the backend path.
`SHADER_SAMPLE` separately controls filtering; the RTX query is point-only.
Strict public coverage samples mip 1 and checks a partial mip-1 copy/readback.
Historical-device behavior is not established.

The latest focused shader increment enables direct `vertex_index` reads from
float vectors for the opt-in FL9_1 projection. Core/HAL proofs and Naga's
static-swizzle lowering agree on the narrowly accepted entry point; helpers,
computed accesses, pointer escapes and writes remain rejected. The FXC probe
covers vec2/vec3/vec4 and the strict public RTX readback covers the vec3
rendering case. Full-suite results and exact commands are recorded in
`FL9_1_PROGRESS.md`.

The current follow-up enables query-gated `Rg16Snorm` texture-to-buffer
readback through the ordered CPU shadow. It verifies raw texels after a partial
GPU copy, including padded rows and a nonzero destination offset. This is a
CPU-memory-backed readback capability; it is not a native staging path.

The strict BC readback and bounded fragment sample-bias offset slices are
implemented and tested. BC readback uses an execution-ordered CPU shadow; it
avoids a prohibited FL9_x staging copy but adds CPU memory and depends on every
write route keeping the mirror current. The sample-offset path accepts a
single-mip 2D view, including mip 1 selected from a multi-mip parent, and
rejects unsupported shapes before execution. Focused strict public tests and
the full private/public DX11 suites passed; exact results are in
`FL9_1_PROGRESS.md`.

The backend is usable for the tested restricted application subset when DX11
is explicitly selected, but it is not a complete FL9_1 implementation or a
drop-in backend for arbitrary wgpu applications. Continue as coordinator:
parallelize read-only audits and bounded compiler probes, then integrate one
verified Core/Naga/HAL vertical slice at a time. Do not assign every missing
API category as an independent implementation task; some are hard FL9_1
limits, and some share compiler/validation infrastructure. Historical
FL9_1 hardware, Lumia/CoreWindow runtime, and physical device-removal recovery
remain external verification gaps.

Two read-only shader/pipeline and resource/transfer audits found no additional
confirmed mismatch in their reviewed paths. This narrows the next software
search to a concrete app request or a previously unreviewed API area. It does
not establish complete coverage, and the external historical-device and Lumia
runtime checks remain necessary.

## HAL panic reachability review — 2026-11-10

An independent bounded source review found no confirmed safe-public-wgpu-
reachable production panic in the D3D11 device, encoder, queue, transfer,
buffer, texture and command paths. Unsupported compute, pipeline-cache,
ray-tracing and query hooks are gated by capabilities the FL9_1 projection
omits. Core mapping-state checks protect HAL unmap, and render-pass state is
checked before encoder state unwraps. This review did not cover `surface.rs`,
`fence.rs`, shader/pipeline internals or capability assertions, so it does not
close the full contract audit. No code changed and no tests were run for this
review. See `FL9_1_PROGRESS.md` for the audit boundaries.

## Coordinator review — 2026-11-10

The current result is a substantial, opt-in public wgpu subset, not a complete
FL9_1 implementation or a drop-in choice for arbitrary wgpu applications. A
public offscreen indexed textured depth-tested cube is verified on a physical
RTX 3090 Ti forced to FL9_1, and a constrained desktop HWND presentation path
is tested. The latest strict runs recorded in `FL9_1_PROGRESS.md` passed
132/132 Core fixtures, 468/468 private HAL tests and 121/121 public DX11 tests;
they do not establish historical-driver compatibility or complete API
coverage. A source audit found that the HAL CoreWindow path's thread/lifecycle
contract matches the public surface contract; public capability reporting now
includes validated WinRT CoreWindow surfaces. Lumia/UWP runtime behavior is
still unverified, and surface calls must remain on the captured dispatcher
thread.

Recent independent audits found no new Core-valid-to-HAL failure in the
reviewed queue/query, graphics/resource, shader, surface/fence and HAL trait
paths, but the exhaustive contract audit remains open. The public adapter now
reports the already implemented CoreWindow surface capabilities; its strict
HWND path remains the only public presentation path executed on the desktop.
The current HAL, public `wgpu` libraries and offscreen/windowed examples
cross-check for ARM32 UWP, but Lumia runtime testing remains required. The
Device Portal timed out on the latest probe.
Ordinary texture arrays, compute/UAV, FL9_1 derivatives/gradients, occlusion
queries and other documented profile/resource restrictions are not gaps to
paper over. FXC probe results place basic derivatives and `SampleGrad` at
FL9_3 rather than FL9_1. Public timestamp queries remain disabled because
D3D11 disjoint intervals do not satisfy wgpu's cross-submission timestamp
contract. The stale status paragraph was corrected in `STATUS.md`.

Use agents for independent audits and feasibility probes, then integrate one
Core/Naga/HAL vertical slice at a time. Do not treat every unsupported matrix
row as a separate parallel implementation task: some are FL9_1 boundaries,
some need external hardware, and others share validation and compiler code.

## Current coordinator audit — 2026-11-10

Default backend selection excludes DX11, but public enumeration is available
when an application compiles the `dx11` feature and explicitly requests
`Backends::DX11` or `Backends::all()`. HAL returns only physical adapters that
create at the requested FL9_1 level and pass the restricted capability
projection. Treat the default-isolation test narrowly; it does not mean the
public adapter is globally disabled.

The latest read-only audit found no confirmed Core-valid path reaching an
unimplemented HAL panic in Device, CommandEncoder, Queue, resources, bindings or
pipelines. The strict CTS `image_copy:origins_and_extents` case is not a valid
downlevel transfer check because its generated cases include unsupported
dimensions and array-layer shapes; details are in `TESTING.md` and
`FL9_1_PROGRESS.md`. Continue the full contract and operation audit without
changing truthful limits to make broad CTS selectors pass.

## Zero-input draw range boundary — 2026-11-10

The HAL normalizes the effective first vertex to zero only when a draw has no
per-vertex inputs and no `vertex_index` use, covering generic and
topology-specific replay. A strict Core/HAL FL9_1 fixture passed count-2
TriangleList, LineList and LineStrip draws plus count-3 TriangleStrip draws at
`u32::MAX`; each readback retained all 64 clear pixels. Negative controls keep
real vertex-fetch and `vertex_index` overflow rejection. See
`FL9_1_PROGRESS.md` for the exact command.

## Timestamped depth-Never draws — 2026-11-10

Raster depth `Never` now executes draws with color and depth writes disabled,
so valid timestamped render passes no longer fail command encoding. A strict
public test verified unchanged color, preserved depth with a farther probe,
and ordered in-pass timestamps at forced FL9_1; the private HAL test also
passed. See `FL9_1_PROGRESS.md` and `TESTING.md` for exact commands.

## Strict CTS operation slices — 2026-11-10

The strict DX11 CTS route passed one RGBA8 clear-and-readback case and one
ordered buffer-copy/readback case on the RTX 3090 Ti at requested/actual
FL9_1, with zero failures or skips. `basic:*` remains 1/2 because its two
failures require unsupported RGBA8Uint copy usages. These focused CTS cases
are operation evidence, not conformance; commands and details are in
`TESTING.md` and `FL9_1_PROGRESS.md`.

## Core/HAL layout visibility fix — 2026-11-10

The Core/HAL audit found one safe layout case that Core allowed but HAL
rejected: depth textures and comparison samplers with both vertex and fragment
visibility. HAL now maps these to fragment-only native slots. Core continues
to reject a vertex entry point that actually samples the depth texture. The
strict RTX 3090 Ti fixture verified both cases at requested/actual FL9_1; no
capability changed. Continue the wider Core/HAL audit. See `FL9_1_PROGRESS.md`
for exact commands and output.

## Strict CTS route — 2026-11-10

The xtask route now supports `--require-dx11-hardware` with mandatory
`--backend dx11`, forwards strict selection to the CTS runner, and enables
adapter-reported defaults for omitted Deno required limits only in this test
mode. The focused `basic:empty:*` selector passed 1/1 without skips after
logging the RTX 3090 Ti at requested/actual FL9_1; it is setup validation, not
rendering evidence. `basic:*` ran 3 cases: one pass and two failures due to
unsupported `Rgba8Uint` copy usages. No capability was broadened. Existing
strict public readback tests remain the GPU rendering proof. See
`FL9_1_PROGRESS.md` for commands and exact output.

## Current continuation — sample-bias compatibility validation — 2026-11-10

The DX11 Core contract now carries selected-fragment-entry
`textureSampleBias` use into draw validation and rejects only used samplers
whose min/mag filters differ or whose effective anisotropy is not one. The
restriction defaults off for other backends. Strict RTX FL9_1 tests passed for
the existing biased-mip rendering and the new pre-HAL Core rejection cases;
full strict public/Core/HAL suites also passed. Public render-bundle testing
cannot capture `RenderBundleEncoder::finish` validation errors because the
Rust wrapper uses a fatal handler; the Core bundle guard remains implemented.
See `FL9_1_PROGRESS.md` for exact counts and `TESTING.md` for commands.

## Latest execution evidence — 2026-11-09

Strict public GPU tests now cover BC1/BC2/BC3 partial initialization and
WGSL `determinant` for mat2/mat3/mat4 in vertex and fragment stages. All 7
focused tests passed on the physical RTX 3090 Ti at requested/actual FL9_1;
96 BC pixels and 768 determinant pixels were checked. No public capability
changed. Exact commands are in `TESTING.md`, with context in
`FL9_1_PROGRESS.md`. The capability audit, broad public workload coverage,
historical FL9_1 device testing and Lumia ARM32/CoreWindow execution remain
open.

## Current coordinator checkpoint — strict CTS preflight and blend audit — 2026-11-09

The Deno/xtask DX11 CTS path is opt-in. With
`WGPU_DX11_REQUIRE_HARDWARE=1`, it requires `DENO_WEBGPU_BACKEND=dx11`, rejects
adapter/configuration failures rather than resolving `null`, and validates the
adapter's DX11 identity plus the verified physical-FL9_1 driver-info marker.
The strict `basic:*` CTS slice ran 3 cases: 0 passed, 3 failed, 0 skipped.
Device creation failed because Deno's Core WebGPU defaults request 28 combined
buffers per stage, above the adapter's truthful limit of 22; no GPU operation
ran. This is expected for the non-Core-WebGPU adapter. The preflight correctly
reported failure rather than skip. Do not change public defaults or falsify the
projection to force these cases. See `FL9_1_PROGRESS.md` for exact commands.
Checkpoint: `bb34517a9ddb8a82c883f4a64c9a3154169e61f8` (`dx11: correct FL9_1 combined buffer limit`).

The separate-alpha audit found that Core requires the absent FL9_1
`SEPARATE_ALPHA_BLEND` flag when both RGB and alpha writes have different
equations; HAL repeats the guard. RGB-only and alpha-only masks are safe because
the other equation is unobservable. The strict Core fixture now tests the
alpha-only case. No capability changed.

Commits: `ee480928e` (strict Deno preflight) and `d4227636e` (alpha-only
blend validation coverage). Public API auditing, broader shader/resource
coverage, historical FL9_1 hardware, Lumia runtime and physical device removal
remain open.

## Current coordinator checkpoint — viewport remap contract — 2026-11-09

The optional viewport-remap pipeline experiment was withdrawn. FL9_x
`RSSetViewports` does not preserve fractional coordinates, and Core accepts
in-target fractional rectangles even though its downlevel limit rejects
out-of-target rectangles. Those draws need the adjusted shader. Pipeline
creation therefore remains fail-closed if FXC cannot compile the variant;
deferring failure until submission would admit a Core-valid operation the HAL
cannot execute. The strict private HAL suite passed 463/463, and the explicitly
enabled debug-layer indexed rendering test passed with zero error/corruption
messages and no unexpected live resources. Details are in
`FL9_1_PROGRESS.md`.

The larger completion mission remains active. Public Core/HAL auditing and
CTS integration remain actionable; real historical hardware and Lumia
CoreWindow runtime evidence require those devices.

## Current coordinator checkpoint — public fragment discard — 2026-11-09

The public WGSL fragment `discard` path is now verified at forced FL9_1.
Naga/FXC accepted the conditional shader for `ps_4_0_level_9_1`; readback
checked color preservation after discard and depth preservation through a
later depth-tested draw. All 128 pixels matched. The focused test passed 1/1,
and the full strict public D3D11 target passed 115/115 on the physical RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, requested/actual FL9_1,
software=false). Independent review found no correctness blocker. This is
execution evidence, not a capability expansion.

Rust 1.93 was used because the pinned 1.93.1 MSVC installation is incomplete.
Formatting, diff checks and targeted Clippy passed; existing HAL/Core warnings
are recorded in `FL9_1_PROGRESS.md`. The public operation audit, hardware
coverage matrix, historical FL9_1 and Lumia/CoreWindow verification remain
open.

Checkpoint: `834944c4e` (`dx11: verify FL9_1 fragment discard semantics`).

## Current coordinator checkpoint — public color write masks — 2026-11-09

The public API test now proves the advertised per-channel `ColorWrites`
behavior at forced FL9_1: `ALL`, each single-channel mask and an empty mask
are applied across scissored columns, then all 64 pixels are read back. The
focused strict test and complete strict public D3D11 target passed on the RTX
3090 Ti, requested/actual FL9_1 (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false). The full target passed 114/114. A read-only review found no
correctness blocker. The test adds evidence only; no capability or backend
implementation changed.

Rust 1.93 was used because rustup reports the pinned 1.93.1 MSVC toolchain
installation incomplete. Formatting and targeted Clippy passed; existing HAL
and Core warnings are recorded in `FL9_1_PROGRESS.md`. Continue the public
Core/HAL execution matrix, while keeping public D3D11 opt-in and historical
FL9_1/Lumia execution requirements open.

Checkpoint: `f71993b7c` (`dx11: verify FL9_1 color write masks`).

## Current coordinator checkpoint — RGBA8_SNORM multi-mip GPU copy — 2026-11-09

The public FL9_1 path now combines query-gated RGBA8_SNORM mips with GPU
texture copying: it uploads four mip levels, copies a 2x2 region within mip 1
between nonzero origins, samples a mip-1 suffix view into RGBA8, and checks all
16 output pixels including untouched texels. The RTX 3090 Ti selected at
requested/actual FL9_1 reported `COPY_SRC | COPY_DST | TEXTURE_BINDING`,
`FILTERABLE` and multi-mip support. Strict `dx11_snorm_copy` passed 2/2;
SNORM CPU texture readback remains unavailable and was not used. Clippy,
formatting and diff checks passed; five existing HAL lint warnings remain.
Commands and capability details are recorded in `FL9_1_PROGRESS.md`.

This adds operation evidence but no public capability. The next step is to
review the remaining public-valid format/usage intersections and choose another
compact query-gated GPU operation. Public D3D11 remains experimental and
historical FL9_1 driver behavior remains unverified.
Checkpoint: `effae2add` (`dx11: verify multi-mip FL9_1 SNORM copies`).

## Current coordinator checkpoint — narrow and RGBA/BGRA usage operations — 2026-11-09

The strict public combined `COPY_SRC | COPY_DST | TEXTURE_BINDING |
RENDER_ATTACHMENT` operation test now covers R8, RG8, RGB565, linear/sRGB
RGBA8 and linear/sRGB BGRA8. The RTX 3090 Ti reported the complete usage union
for all seven formats and `TEXTURE_FORMAT_16BIT_PACKED` for RGB565. At
requested/actual FL9_1, each format executed padded upload, GPU texture copy,
copied-image readback, same-format sampled rendering and attachment readback.
All 64 copied texels matched exactly; all 64 sampled output pixels matched
within one byte. The strict focused test passed 1/1 without skips and the full
public D3D11 target passed 117/117. Formatting, diff checks and targeted
Clippy passed; five existing HAL warnings remain. Exact commands are in
`FL9_1_PROGRESS.md`.

This proves one usage union across seven formats, not the full 21-format
operation matrix or all allowed usage combinations. Next, target an
independently query-gated SNORM or BC operation sequence. Public D3D11 remains
explicit opt-in and experimental; historical FL9_1 and Lumia/CoreWindow
execution remain unverified.
Checkpoint: `48cd0b95d` (`dx11: verify FL9_1 texture readback row padding`).

## Current coordinator checkpoint — combined RGBA8/BGRA8 usage operations — 2026-11-09

The public adapter now has a strict GPU operation test for the full combined
`COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT` usage set on
linear RGBA8 and BGRA8. It checks `FILTERABLE`, uploads patterned texels,
copies on the GPU, reads the copied image to CPU memory, samples it into a
same-format render attachment, and reads the output back. Strict RTX 3090 Ti
execution requested/selected FL9_1 and validated all 64 copied texels plus all
64 sampled output pixels for both formats without skips. The focused test
passed 1/1; the full strict public D3D11 target passed 117/117. Exact commands
are in `FL9_1_PROGRESS.md`.

This is the first slice of an execution matrix, not proof for the other 21
projected formats or all allowed usage combinations. The static projection
test still drops each texture after creation; expand by operation class and
query gate. The backend remains suited to opt-in applications that stay
inside the capability matrix, not general-purpose drop-in use. Historical
FL9_1 drivers and Lumia/CoreWindow runtime remain unverified.
Checkpoint: `989ce81f2` (`dx11: execute combined FL9_1 texture usages`).

## Current coordinator checkpoint — query-gated MSAA depth/stencil proof — 2026-11-09

The public 2x/8x RGBA8 resolve test now checks near/far depth rejection for
each query-positive depth format. Strict physical RTX 3090 Ti execution at
requested/actual FL9_1 ran six cases (2x/8x × D24S8, Depth24Plus and D16),
validated all 64 pixels per case and skipped none. D24S8 additionally proved
stencil clear, replacement and matching/mismatching reference comparisons.
This adds evidence for existing optional capability paths, not new support.
The test and its exact command are in `FL9_1_PROGRESS.md`.

The latest Core/HAL format audit found no confirmed mismatch in inspected
transfer, attachment or resolve paths. Its table checks every reported usage
combination at texture creation, but not operation execution. The next
coordinator task is a compact public usage-operation matrix for common combined
RGBA8/BGRA8 usages. No low-risk neighboring FL9_1 shader feature was identified;
the remaining nearby operations are profile-limited. The inspected HAL panic
stubs are gated from valid public object creation, though a full malformed
unsafe-HAL audit remains open.

At this checkpoint the backend remains appropriate for controlled applications
whose requested operations match `CAPABILITIES.md`; it is not a drop-in
general-purpose implementation. The main gap is systematic proof and closure
of the public capability contract, alongside unverified historical FL9_1 and
Lumia/CoreWindow runtime behavior.
Checkpoint: `fa15bc6f8` (`dx11: verify FL9_1 multisample depth and stencil`).

## Current coordinator checkpoint — selected `textureNumLevels` helper plus sample — 2026-11-09

The selected 2D fragment `textureNumLevels` helper may coexist with one direct
implicit sample of the same image. Core, Naga and HAL enforce matching
selected-entry rules; FXC compiles at `ps_4_0_level_9_1`. Strict readback on
the RTX 3090 Ti, requested/actual FL9_1, checked all 64 pixels for helper-only
and composed variants across one- and three-level views. The composed colors
were `[85,64,128,255]` and `[255,128,64,255]`. Full private HAL passed
462/462, Core 125/125, Naga 156/156, public D3D11 116/116 and Core DX12
116/116. The exact commands and limits are in `FL9_1_PROGRESS.md` and
`TESTING.md`. General `texture_query` remains disabled.

The implementation remains an opt-in FL9_1 subset, not a complete drop-in
wgpu backend. Intrinsic FL9_1 boundaries include compute/UAV, ordinary texture
arrays, 1D textures, MRT, occlusion queries and native 3D copy/readback.
Implementation/validation and test-coverage work remains for broader shader
operations, resource usages, Core-to-HAL operation correspondence and precise
errors. Historical FL9_1 hardware, Lumia/CoreWindow runtime and physical
device-removal recovery remain unverified.

Next: extend the existing optional 2x/8x MSAA depth test to prove near/far
rejection for each query-positive format and add stencil checks where
supported. Continue the public Core-acceptance/HAL-execution audit in parallel.
Specialists can own bounded independent audits or isolated test files; the
coordinator owns cross-crate design, integration, validation and commits.
Checkpoint: `8a8e6350f` (`dx11: sample through selected textureNumLevels helper`).

## Current coordinator checkpoint — selected dimensions helper plus sample — 2026-11-09

The narrow selected-entry 2D fragment `textureDimensions` helper may now
coexist with one direct level-less implicit sample of the same texture. Naga,
Core and HAL proofs agree on the one-helper/one-sample/resource shape, and FXC
accepts the result under the FL9_1 pixel profile. Strict public readback on the
RTX 3090 Ti (requested/actual FL9_1) verified all 64 pixels across a 4x2 to
8x4 bind-group transition. Full private HAL passed 460/460, strict public
D3D11 116/116, Naga 155/155, Core 124/124 and DX12 Core 115/115. The exact
commands and limitations are recorded in `FL9_1_PROGRESS.md` and
`TESTING.md`. The selected query limit remains narrow; general `texture_query`
is disabled.
Checkpoint: `f8cd666aa` (`dx11: sample through selected textureDimensions helper`).

The backend is useful for controlled opt-in prototypes but is not a general
drop-in wgpu backend. Remaining work includes a systematic review/test matrix
for public Core acceptance versus HAL execution, broader shader/resource edge
coverage, unsupported-operation error quality, and real Lumia/CoreWindow and
historical FL9_1 validation. Compute/UAV, ordinary arrays, 1D resources, MRT,
occlusion queries and native 3D copy/readback remain FL9_1 hardware boundaries.
The lead agent should continue owning cross-layer implementation, testing and
commits; specialists are best used for parallel read-only audits and narrowly
owned proof/test changes.

## Current coordinator checkpoint — slope bias, 3D load helpers and `mat2` inverse — 2026-11-09

The public D16 depth test now verifies positive and negative slope-scaled bias
on a genuinely sloped triangle. Strict forced-FL9_1 execution passed on the
RTX 3090 Ti and checked 64/64 red pixels for positive bias and 64/64 green for
negative bias. The full strict public suite passed 112/112; three optional
format/mip subcases were explicitly skipped because the selected device did
not report them. See `FL9_1_PROGRESS.md`; checkpoint `e53561240`.

The table-driven public format-projection test is committed as `5e82974e9`.
On the physical RTX 3090 Ti forced to FL9_1, it checked 21 projected formats,
189 advertised usage combinations, 136 rejected unreported combinations, 21
mip contracts and 40 sample-count contracts; no advertised combination
produced an internal HAL error.

Commit `ffafa8b2d` extends 3D fragment `textureLoad` through reachable
straight-line helpers; commit `9e1ef2f7d` adds the bounded f32 `mat2x2` inverse
translator subset. The 3D helper path passed Naga 152/152, strict Core fixture
120/120, strict private HAL 448/448, strict public D3D11 112/112 and DX12 Core
108/108. Its strict RTX 3090 Ti test explicitly requested/selected FL9_1 and
matched 320 output pixels while rejecting a branching helper before HAL.
Targeted Clippy, formatting and diff checks passed; existing HAL lint warnings
remain. The working tree is clean after the checkpoint commits.

The mat2 inverse public test executes inverse in both vertex and fragment
stages and checks all 64 pixels `[0,255,0,255]`; Core rejects `mat3` inverse
without an internal HAL error. This feature applies to GLSL/Naga IR inputs
because WGSL has no `inverse` builtin. Core checks the selected entry point and
all regular module functions compiled by Naga's HLSL writer; unused unsupported
`mat3` helpers are rejected too. The most recent focused strict GPU test
exercises inverse in both shader stages.

Recent audits found no confirmed format projection mismatch. Native 3D copy
and direct 3D readback are excluded by the 10Level9 contract; current 3D
upload/sampling is implemented. Keep implementation under coordinator review:
cross-crate Naga/Core/HAL capability edits share validation contracts.
Practical limitations still include unsupported intrinsic FL9_1 features,
the experimental opt-in public contract, historical hardware, Lumia/CoreWindow
execution and physical device-removal recovery. The next small coverage task is
public slope-scaled depth-bias readback; a selected shader/resource contract
audit follows.

## Latest verified shader increment — selected-entry `textureLoad` helpers — 2026-11-09

Commit `6e442d1d14a5873dd83344339de56b909dbe0306` adds a restricted 2D
fragment `textureLoad` helper subset. The selected entry point and its
reachable straight-line helpers may directly reference one shared global
single-mip `texture_2d<f32>`. Core validation, the HAL proof and Naga HLSL
generation use matching selected-entry reachability. Unsupported control flow,
other image operations/resources, additional texture identities and resource
handles passed through helpers remain rejected; sibling-only helpers are not
emitted. 3D `textureLoad` helper calls remain unsupported.

Verification on Rust 1.93.1: Naga 149/149, Core D3D11 fixture 119/119,
strict private HAL 447/447, strict public D3D11 110/110 and DX12 Core 110/110.
The focused strict public test passed on the RTX 3090 Ti at requested/actual
FL9_1 (driver `32.0.16.2101`, physical hardware, not software) and matched all
256 pixels across direct and nested helper loads, valid bounds, out-of-bounds
and nonzero-mip cases. Clippy, formatting and diff checks passed; existing
warnings are listed in `FL9_1_PROGRESS.md`. An independent selected-entry
review found no concrete correctness risk.

This is a bounded shader improvement, not practical completion. Explicit
`Backends::DX11` selection already exposes the experimental FL9_1 adapter, while
default backend selection still excludes DX11. Remaining material mission work
includes finishing the public capability-to-HAL audit, expanding ordinary
shader/resource subsets, and verification on historical FL9_1 hardware.
Public textured 3D scene and desktop `HWND` presentation paths already exist
and are documented in `TESTING.md`; CoreWindow, Lumia ARM32 and real
device-removal recovery remain unverified.

## Latest verified increment — query-gated RGB5A1/RGBA4 GPU copies — 2026-11-09

The opt-in D3D11 path supports same-format GPU texture-to-texture copies for
`Rgb5a1Unorm` and `Rgba4Unorm`, gated by their individual backend paths and
the selected-device `TEXTURE2D` query. It does not require `RENDER_TARGET`,
create render-target bindings or permit texture-to-buffer readback. Strict
forced-FL9_1 execution on the RTX 3090 Ti verified padded uploads, 2×2 copies
to destination origin (1, 1), sampling, all 64 output pixels for each packed
format, and preservation of the untouched border. Full strict public D3D11
passed 107/107, private HAL 446/446 and DX12 Core 108/108. The complete
command log and lint warnings are in `FL9_1_PROGRESS.md`. Checkpoint:
`317cb9a38` (`dx11: support packed FL9_1 GPU texture copies`).

The backend remains a restricted experimental implementation; this format
addition does not change that boundary. Next, continue the public projection
audit and consider the directly referenced helper subset of texture-load
emulation only with matching selected-entry validation in Core, Naga and HAL.
FL9_1 3D texture copies are excluded by the 10Level9 contract. Historical
hardware, Lumia/CoreWindow runtime and physical device-loss recovery remain
unverified. DX11 remains excluded from default backend selection.

## Previous verified format support — BC1/BC2/BC3 sRGB — 2026-11-09

The opt-in public D3D11 adapter now has a separate native-only
`TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB` feature. It is projected only when the
selected FL9_1 device, independent format queries and backend sampled-upload
paths support all three formats. Strict public GPU readback on the RTX 3090 Ti
at explicitly requested/actual FL9_1 verified BC1/2/3 sRGB mid-gray output near
`[59,57,59,255]` in all 64 pixels after GPU texture copies, demonstrating
decode. The narrow unorm feature remains separate; standard BC1–BC7, BC4–7,
reinterpreted BC views, mipmapped BC and CPU BC readback remain unavailable.
The private HAL passed 445/445, strict Core passed 119/119, focused public BC
passed 1/1, full strict public D3D11 passed 106/106, DX12 Core passed 108/108,
and targeted Clippy/format/diff checks passed. Exact commands and query results
are in `FL9_1_PROGRESS.md`.
Implementation checkpoint: `3a1cb5527` (`dx11: expose query-gated BC1-3 sRGB formats`).

The backend remains an opt-in, restricted FL9_1 renderer, not a complete or
drop-in general-purpose wgpu implementation. Explicit DX11 selection is
available; default backend selection remains unchanged. Remaining practical
work includes the broader Core/HAL contract audit, unsupported shader
operations and resource paths, public CoreWindow/UWP verification, historical
FL9_1 hardware and real device-removal recovery. Read-only specialist audits
recommend permitting a carefully proven helper-call subset of the existing
`textureLoad` emulation as one next contained shader task. Mat3 inverse has
level-9 FXC probe, Naga lowering tests and strict public GPU output evidence;
broader matrix shapes remain unavailable. Do not implement FL9_1 3D texture
copies because the existing 10Level9 audit records them as unavailable.

Use specialist agents for non-overlapping research and review, but keep
cross-crate Core/Naga/HAL changes and capability integration under one
coordinator; the shader paths share proof metadata and should not be edited in
parallel. The overall mission is still incomplete.

## Narrow public BC1/BC2/BC3 unorm feature — 2026-11-09

The opt-in public D3D11 adapter now exposes the native-only
`TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM` feature when all three selected-device
sampled-upload paths are available. Core accepts BC1/2/3 unorm through this
narrow feature without reporting the standard BC1–BC7 feature. BC sRGB,
BC4–BC7 and BC texture-to-buffer readback remain unavailable. Per-format
queries govern optional GPU copy-source usage.

The strict public forced-FL9_1 test on an NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) uploaded distinct BC1 red, BC2 green
and BC3 blue blocks, copied all three textures, sampled all three in one
fragment shader and verified all 64 output pixels were white. The same test
confirmed the full BC feature was absent and BC4 plus BC3 3D creation were
rejected by Core. The
strict Core fixture and CPU-only Core/HAL capability gates passed. This is a
restricted format addition only; it does not resolve the outstanding full
Core/HAL audit, shader subset, legacy hardware, Lumia/CoreWindow or physical
device-removal coverage.

## Latest verified public rendering — depth bias — 2026-11-09 — `27b6901bc97706617054f1ef2ff78af4eec630eb`

The public D16 depth test exercises zero-, positive- and negative-constant-bias
pipelines at forced FL9_1. Readback confirms positive bias fails and negative
bias passes the depth test across the expected 4×8 regions; Core rejects
nonzero depth-bias clamp. The focused GPU test passed 1/1 and the complete
strict public D3D11 suite passed 109/109 on RTX 3090 Ti. Clippy passed with
existing argument-count warnings, and format/diff checks passed. Exact commands
and optional format-query skips are recorded in `FL9_1_PROGRESS.md`.

## Latest verified test coverage — indexed LineStrip restart render bundles — 2026-11-09 — `42cecc33690e20c789e1aa59bdb0dcf28283fa86`

The public strict test `public_dx11_indexed_line_strip_restart_render_bundle_at_fl91`
now checks two consecutive restart-separated indexed LineStrip bundles (Uint16
green and Uint32 red), full-target readback, restart gaps, unexpected pixels,
and vertex-buffer retention after the caller drops its handle. It passed on the
RTX 3090 Ti at requested/actual FL9_1; per-segment pixel counts were
`[2, 2, 2, 2]`. The complete strict public suite passed 109/109. This is test
coverage for existing behavior, not new capability. Formatting, `git diff
--check` and public-test Clippy passed; only existing `too_many_arguments`
warnings were emitted. The full command and result are recorded in
`FL9_1_PROGRESS.md`.

## Latest verified checkpoint — depth-only MSAA, viewport remap, and shader gates — 2026-11-09 — `dbfb26b41`

The HAL permits a multisample pipeline without a color target when it has a
depth attachment whose selected format/sample count passed the capability
query. A color target still requires the implemented query-validated RGBA8
path. The strict public regression performed a 4x targetless depth prepass,
then a color+depth draw and resolve. D16, Depth24Plus and D24S8 blocked the
farther draw; all 64 pixels remained blue on the RTX 3090 Ti at requested and
actual FL9_1. The final strict public D3D11 suite passed 108/108 and the
complete strict private HAL suite passed 443/443 on that physical adapter.
Core's zero-adapter isolation test passed 1/1; the DX12 Core suite passed
106/106. Clippy, formatting and `git diff --check` passed with existing
warnings. Read-only shader/pipeline, resource/transfer, render/attachment,
surface/query, buffer/command, capability, panic/error and unsafe-contract
audits found no further confirmed execution or safety mismatch. The nonzero
1D dimension limit while 1D textures are
disabled is a reporting clarity issue because Core rejects that shape. The
broader audit and historical hardware/platform verification remain open.

The viewport-remap fix described below is also included in this checkpoint.
The public capability-gate test covers unrequestable clip distances and f16.

The HAL now fails pipeline creation with a vertex linkage error when FXC cannot
compile the FL9_1 viewport-remapped shader variant. This closes a conditional
path where a later fractional-viewport draw could surface an `Unexpected` HAL
error that Core treats as device loss. The 254-vector shader boundary test
proves base compilation succeeds while the remapped version exceeds FXC's
256-instruction limit at 258. A strict public regression caught the pipeline
error and then rendered/read back successfully on the same RTX 3090 Ti. The
strict fractional-viewport GPU test also passed.

The full strict private HAL suite passed 443/443 and public D3D11 suite passed
108/108 at explicitly requested/selected FL9_1 on RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, software=false). Three optional public format subcases
reported query-based skips; no required hardware test skipped. Core adapter
isolation passed 1/1 and DX12 Core passed 106/106. Clippy, formatting and
`git diff --check` passed; existing lint warnings remain. Rustc initially OOMed
with debug info enabled; the public suite passed with incremental compilation
and debug info disabled. The public shader-feature gates, viewport fix and
depth-only MSAA support are part of this checkpoint.

Status: the backend is useful for carefully scoped Windows graphics prototypes
that stay within `CAPABILITIES.md`, but it is not a complete or drop-in general
purpose FL9_1 wgpu implementation. A full operation-by-operation Core/HAL audit
remains open. Compute/UAV, geometry/tessellation, more than one simultaneous
color target, indirect draws, occlusion queries and additional shader/resource
combinations are absent or outside the FL9_1 contract. Public CoreWindow and
Lumia ARM32 runtime behavior, historical FL9_1 drivers, and physical device
removal/recovery remain unverified. Focused audits found one issue now fixed,
no issue in the reviewed query/surface paths, and one stale resource finding
already fixed by `f3926cf9b`. See `FL9_1_PROGRESS.md` for exact evidence.

## Latest verified checkpoint — public buffer lifetime before submit — 2026-11-09 — `67e8fcde45592a8a8ccc9c7759e9a0569313abd2`

The public GPU regression records a partial source-to-destination copy and a
destination-to-readback copy, finishes encoding, drops both public buffer
handles before submission, and validates all 64 bytes after completion. It
passed 1/1 on the RTX 3090 Ti at requested/actual FL9_1; the complete strict
public D3D11 suite passed 105/105. The preceding code checkpoint
`5552a8fa57ca1c34c89dcec70524150d15b2674f` adds TriangleStrip `vertex_index`; the private HAL suite passed
443/443, the controlled Core fixture 1/1 and DX12 Core 106/106 there. Two
read-only audits found no attachment lifecycle or buffer ordering/lifetime
defect; one identified and closed this missing public buffer-handle test.
Historical FL9_1 hardware, Lumia runtime and real device removal remain
unverified. Exact commands and skips are in `FL9_1_PROGRESS.md`.

## Mission status — 2026-11-08

At the time of this 2026-11-08 status entry, the opt-in public D3D11 path was
substantially usable for a restricted FL9_1 graphics subset, but the completion
mission remained partial. It is excluded from default backend selection. The
latest code checkpoint at that time was `90026ebf3` (`dx11: validate indexed
extents and padded volume uploads`). The
HAL index remap planners no longer impose an extra four-byte copied-extent
rule, while retaining offset, stride and bounds checks. A strict private test
verified a padded nonzero-offset partial 3D volume upload by sampling the
whole target at forced FL9_1. The private HAL suite passed 441/441 and strict
public D3D11 passed 103/103. The preceding checkpoint `b93747fb4` added
LineStrip; the latest checkpoint above adds TriangleStrip to the emulated
`vertex_index` subset.
Full commands and warnings are recorded in `FL9_1_PROGRESS.md`. This remains
a restricted incremental graphics subset, not mission completion.

The Lumia Device Portal retry for `/api/os/devicefamily` timed out. CoreWindow
surface capabilities remain withheld, and no Lumia runtime result is claimed.

The latest contract fix lets combined depth-attachment/sampled textures create
default views with both native DSV and SRV roles. The strict public regression
passed for D16, Depth24Plus and D24S8 at requested/actual FL9_1. Full strict
public tests passed 91/91 and private HAL tests passed 416/416 at that
checkpoint; the new
checkpoint is `f3926cf9bf1b5ef61101b0efbc1166b45aeeb6e5`; commands are in
`FL9_1_PROGRESS.md`.

The latest test-only increment adds async `MapMode::Write` coverage for a
nonzero buffer slice followed by GPU copies and readback. The focused test
passed 1/1 and the full strict public D3D11 target passed 92/92 on the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`,
software=false, requested/actual FL9_1. Optional selected-device subcases
logged skips; no required strict hardware test skipped. The last complete
private HAL run passed 416/416; the core DX11 fixture previously passed
112/112, DX12 core 101/101 and the strict HWND fixture 2/2. The detailed
commands and hardware readback evidence are in `FL9_1_PROGRESS.md`. The latest
test checkpoint is `fe0cc897a` (`dx11: test FL9_1 asynchronous write mapping`).

Feature-wise, dynamic offsets, multiple bind groups, multiple sampled
resources, broad tested vertex/index draw paths, depth/stencil, query-gated
MSAA resolve, multiple uniform ranges and a constrained HWND path are present.
Compute/UAV, geometry/tessellation, indirect draws, occlusion and pipeline
statistics queries, explicit in-pass timestamp methods, ordinary texture
arrays and several shader operations/resource combinations remain unavailable or
deliberately rejected. An exhaustive public core/HAL contract audit is
still open. A selected-entry audit found and closed a Core/HAL mismatch for
the emulated `textureNumLevels` and base-clamp paths: Core now rejects those
entries when the module declares regular helper functions, matching HAL's
current restriction. Historical FL9_1 driver behavior, physical device removal
and recovery, and Lumia/CoreWindow runtime execution are unverified. The proposed
3D copy expansion has been rejected: Microsoft's 10Level9 rules disallow 3D
GPU texture copies at FL9_1, and current public/HAL gates correctly omit them.
Recent independent audits found no confirmed texture or runtime request
mismatch in the examined paths. Continue the broad audit for ordinary
public-valid operations; do not treat the hardware/runtime verification gaps
as resolved.

The 2026-11-08 candidate sweep also rejected read-only depth sampling as an
RTX FL9_1 increment: a native D16 read-only DSV probe returned
`E_INVALIDARG`, and Core's combined read-only-plus-sampled use is not yet
handled by HAL render-pass validation. The flag remains absent. BC1/2/3
texture-to-buffer readback and 3D render-target views are likewise not
FL9_1-public expansion candidates under the documented 10Level9 restrictions
and current wgpu feature gates. These audit results and commands are in
`FL9_1_PROGRESS.md`.

## Latest surface lifecycle checkpoint — 2026-11-08

The public D3D11 HWND error fixture passed 2/2 on the RTX 3090 Ti at requested
and actual FL9_1. Public queue dispatch now passes the receiver queue ID to
core, where queue/device mismatch is rejected and the consumed frame is
released. The same fixture verifies a matching queue can reacquire afterward,
that a synthetic `DXGI_ERROR_DEVICE_REMOVED` invokes the lost callback with a
reentrant poll, and that dropping another acquired frame after device loss
allows later reconfiguration and acquisition. The invalid-device discard path
releases the retained HAL frame. Device removal is injected; actual removal
and recovery remain unverified. The first cleanup checkpoint is
`a7205baf35fe2799a4eb13b5fca9b129068d55b3`; queue identity is verified in
`d890ee6bb15537eddca530c9fd5b25b26836c9fc`. `cargo +1.93 check --offline
-j8 -p deno_webgpu` passed under the Visual Studio x64 environment. Historical
device removal and Lumia runtime behavior remain unverified. See
`FL9_1_PROGRESS.md` for details.

## Latest public topology-validation checkpoint — 2026-11-08

The opt-in public projection reports that emulated `vertex_index` is valid for
`TriangleList` and `PointList`. Public strict tests verify triangle index
remapping and point-list direct/indexed output. Core accepts both supported
topologies and rejects `LineList` use before HAL while retaining ordinary
LineList pipelines. The strict public D3D11 suite passed 83/83 and the private
strict HAL suite passed 409/409. The public point-list test read back three
direct points and one repeated indexed point on the RTX 3090 Ti at explicitly
requested and selected FL9_1. Optional Float16x2 IA and Rg8Unorm mip subcases
were explicitly skipped by selected-device capability checks. Historical
FL9_1 behavior remains unverified, and the backend remains partially complete.
See `FL9_1_PROGRESS.md` for the checkpoint and reproduction commands. Implementation checkpoint:
`2e2bd70bb038ea09ca7e6eb5fe60dd5630c18867`; public regression checkpoint:
`732c629a1788a316512f20097ce450c99d4e9449`.

## Latest shader increment — host-emulated instance index — 2026-11-07

The backend supports the selected vertex entry point's instance_index input
without using SV_InstanceID, which FXC rejects for 10Level9. Naga lowers the
builtin to a host-updated value in reserved VS constant register b13, sharing
the viewport-transform constant buffer. Replay updates it before each
emulated instance and preserves first_instance for indexed and non-indexed
draws. Core allows the supported builtin-to-f32 usage and keeps unrelated
integer operations gated. On the RTX 3090 Ti at requested/actual FL9_1, the
per-instance stream test and the indexed/non-indexed test passed; all 128
pixels in the latter matched. FXC accepted direct, struct-only and mixed input
forms. The later core topology gate and PointList extension are documented in
the latest checkpoints above; historical-device verification remains open.
## Latest format increment — query-gated R8G8_SNORM mip chain — 2026-11-07

Sampled/upload R8G8_SNORM textures may now use mip chains when the selected
device reports `TEXTURE2D | MIP` and the backend path is active. On the RTX
3090 Ti at explicitly requested/actual FL9_1, the strict public test uploaded
three signed levels and verified all 16 pixels sampled from mip 1. The private
HAL suite passed 402/402, public D3D11 suite 78/78 (one optional Float16x2
subcase skipped), core adapter isolation 1/1, and DX12 core 94/94. Render-target,
copy-source and readback uses remain unsupported. Historical driver and Lumia
runtime behavior remain unverified; continue the public core/HAL contract audit.

## Latest integration safety increment — 2026-11-07

`Backends::default()` now excludes experimental DX11, so a default public
`wgpu::Instance` does not initialize or enumerate the restricted FL9_1 path.
Explicit `Backends::DX11` and `Backends::all()` still include it. Core now
registers DX11 after the existing backend order. The default-path regression
passed on the RTX 3090 Ti, and the strict public D3D11 suite passed 77/77 at
requested/actual FL9_1. The mission remains partial; continue auditing the
public core/HAL contract and unsupported-operation boundaries.

## Latest NPOT mip capability increment — 2026-11-07

The D3D11 projection now sets `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` only when
the backend path exists and the selected feature level/optional query proves
unrestricted NPOT support. The HAL applies the same check to D2 creation and
sampled views; the existing per-format `MIP` query still gates the format. CPU
tests cover positive, negative, unavailable and backend-disabled inputs. A
strict public test on the RTX 3090 Ti explicitly requested FL9_1, observed
`FullNonPow2TextureSupport=true`, uploaded a 6×10 four-level RGBA8 texture and
rendered mip 2 to an 8×8 target; 64/64 pixels matched. This verifies the
positive driver path only, not historical restrictive hardware or every
format/usage. The mission remains partial; continue the contract audit rather
than treating this query-gated extension as general texture completion.

## Latest format checkpoint — BC1/BC2/BC3

The private FL9_1 subset now supports queried sampled upload, block-aligned
partial updates and same-format GPU texture copies for BC1_UNORM, BC2_UNORM and
BC3_UNORM. On the RTX 3090 Ti, all three returned support mask `0x000213a0` at
requested/actual FL9_1; strict rendered readback matched all 256 pixels for
each after an upload, partial replacement and texture copy. BC1 uses 8-byte
4×4 blocks; BC2/BC3 use 16-byte blocks. BC texture-to-buffer readback,
render targets, mip chains, sRGB views and BC4–BC7 remain unavailable. The
aggregate public BC feature stays withheld. The 401-test private HAL suite,
73-test public D3D11 suite, core adapter-selection test and 94-test DX12 core
suite passed; one optional Float16x2 IA subcase skipped. See the top of
`FL9_1_PROGRESS.md` for exact commands. Resume with the selected public
core/HAL contract audit; do not treat this format slice as mission completion.

## Latest public rasterizer check — 2026-11-07

The strict public D3D11 target now checks opposite triangle windings through
`FrontFace::Ccw` and `FrontFace::Cw` pipelines with back-face culling. On the
RTX 3090 Ti explicitly selected at FL9_1, readback verified 10 red CCW pixels,
10 green CW pixels and 44 clear pixels. The focused case and full public target
passed 1/1 and 71/71 respectively. The wider render-state and historical-driver
audits remain open.

## Latest shader audit note — literal sample bias through helpers

Core and the private HAL now propagate finite literal or unary-negated-literal
sample-bias arguments across nested helper calls at each call site. A strict
public FL9_1 render confirmed that a helper-forwarded +1 bias selects the same
mip as the direct +1 form; 384 pixels matched across the direct, helper and
implicit-sampling images. Dynamic, out-of-range, offset and mixed valid/invalid
call sites remain rejected. The strict public suite passed 70/70 and the
private HAL suite 395/395. The one initial public-suite invocation had a
transient empty adapter enumeration in the RGB565 test; the focused rerun and
complete rerun both passed on the RTX 3090 Ti at requested/actual FL9_1.

## Latest audit note — sample-mask compiler boundary

A sample-mask discard emulation was evaluated and not enabled. A single
sample observes only bit 0, but `ps_4_0_level_9_1` FXC rejects the required
`mask & 1` operation with X3535. Treating any nonzero mask as coverage would
change WGSL behavior for masks with higher bits only. Keep core's selected
entry-point rejection in place unless an exact low-bit test is shown to compile
for this profile. The negative FXC regression and strict public core-gate test
passed 1/1 each on the RTX 3090 Ti at requested/actual FL9_1.

## Previous audit note — query boundary

Microsoft's 10Level9 table confirms occlusion queries are unavailable at
FL9_1 (available from FL9_2). Keep public query sets rejected; the event query
used internally for fences is not an occlusion-query substitute. Strict public
validation already checks that occlusion, timestamp and pipeline-statistics
query-set requests fail before HAL. Timestamp and pipeline-statistics remain
unimplemented/unverified separately. The focused strict public test passed
1/1 on the RTX 3090 Ti, explicitly requested and selected at FL9_1. A
constant-expression false loop exit is also covered by the selected-entry
analyzer unit test. The overall mission remains partial.

## Latest mission state — selected-entry dynamic-loop validation

The FL9_1 projection now distinguishes statically bounded shader loops from
runtime-dependent loop termination. Core retains the tested static-loop
subset and rejects dynamic loops selected by a pipeline before HAL/FXC. Its
entry-point analysis follows reachable helpers and conservatively tracks
uniform/global values, local aliases, function arguments, local variables
passed to pointer parameters, and a literal-false loop exit. The strict public
test still rendered all 64 pixels with static loops in both stages; dynamic
vertex and fragment cases returned core validation errors. Synthetic core
coverage includes direct, helper-global, helper-parameter, local-alias, and
pointer-write bounds, a provably unbounded loop, and a safe sibling entry.

Latest verification: strict public D3D11 70/70, strict private HAL 393/393,
strict controlled core fixture 103/103, core library 95/95, `wgpu-types`
all-features 94/94 plus 14 doc tests passed (21 ignored), DX12 core 94/94 and
DX12 HAL 4/4. The strict public run includes a Float16x2 format-query branch
that may skip its optional subcase. The RTX 3090 Ti (`0x10de:0x2203`, NVIDIA
driver `32.0.16.2101`, `software=false`) was explicitly requested and selected
at FL9_1. See `FL9_1_PROGRESS.md` for commands, lint output and remaining
verification gaps. The mission remains **PARTIALLY COMPLETE**; historical
FL9_1 drivers, Lumia/CoreWindow runtime and broad public conformance remain
open.

The last read-only Lumia Device Portal request to `/api/os/devicefamily`
timed out and the device did not answer a LAN ping. No deployment was
attempted; mobile runtime verification remains unavailable.

## Previous checkpoint — cube-face GPU copies

The query-supported RGBA8 cube path supports GPU-to-GPU copies between cube
faces and ordinary 2D textures. The strict public test copied cube face 0 into
face 1 across four mips, copied a separately uploaded 2D texture into face 2,
and copied face 3 back into a 2D texture. Subsequent cube sampling matched
960/960 pixels, and CPU readback of the copied 2D texture matched all 64
texels. Core rejects cube texture-to-buffer readback using the one-layer
downlevel source limit; the HAL also refuses staging from an SRV-only resource.
This follows the FL9_x GPU-to-CPU copy restriction and does not establish
behavior on historical drivers. Checkpoint: `7fb6cf3c9` (`dx11: expand FL9_1
public integration coverage`).

## Current mission state — public query-gated depth comparison

The latest increment carries the private query-gated FL9_1 depth comparison
path through core and the opt-in public API. The internal downlevel model now
expresses a supported comparison-function mask, min/mag filter independence
and comparison anisotropy. On the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) forced to requested and actual FL9_1, the public projection
reports only `LessEqual`, equal min/mag filters and anisotropy 1. Public D16,
Depth24Plus and D24S8 sample comparisons each rendered and read back 32
failing black pixels and 32 passing red pixels. Depth24Plus uses D24S8 backing
without exposing stencil. Core rejected a different comparison function,
unequal filters and anisotropy 2 before HAL. Their typed SRV support
queries returned `E_FAIL`, but resource/view creation and rendering passed.
The public `textureSampleCompareLevel` path also passed its 64-pixel D16
readback, and FXC accepted Naga's `SampleCmpLevelZero` output for the FL9_1
pixel profile. See the newest `FL9_1_PROGRESS.md` entry for verification
commands and results. The complete strict public suite passed 67/67, the
private HAL suite passed 390/390, the core adapter-isolation and selected-device
fixture tests passed 1/1 each, and the core DX12 regression suite passed
93/93 on this tree.
The public 4x render/resolve test also verified depth ordering with D16,
Depth24Plus and D24S8; each resolved image had 28 full-red, 8 partially
covered and 28 clear pixels, with no farther green fragments.

The positive D3D9 shadow query also enables the documented FL9_1 border-mode
exception for depth comparison samplers. Strict public renders sampled beyond
the D16 view: opaque black failed and opaque white passed all 64 comparisons.
Core rejected an ordinary border sampler and the separate unrequested
zero-border feature. This does not enable general border sampling; older
drivers remain unverified.

The overall mission remains **PARTIALLY COMPLETE**. The public opt-in subset
is usable for matching applications, but is not a general drop-in backend.
Continue auditing public-valid paths and shader coverage; historical FL9_1
drivers, Lumia/CoreWindow runtime and physical device-removal recovery remain
unverified.

## Current mission state — query-gated 4x depth attachments

Checkpoint `72e10f78f` adds query-gated 4x D16/D24S8 depth attachments to
matching 4x RGBA8 render/resolve passes. The RTX 3090 Ti reported one quality
level for both depth formats while explicitly requested and actually running
at FL9_1. A public near/far triangle test verified depth rejection in resolved
pixels. The full strict HAL suite passed 377/377 and public D3D11 suite 63/63;
the optional Float16x2 vertex-input branch was visibly skipped after a
negative format query. Core adapter isolation passed 1/1, DX12 core passed
92/92, HAL and public-test Clippy passed, and the HAL ARM32 UWP target
cross-compiled. Details are in the latest `FL9_1_PROGRESS.md` entry.

The public adapter remains an explicit D3D11 opt-in with a tested offscreen
and constrained HWND subset. Applications matching that subset can trial it;
it is not a drop-in wgpu backend. Unsupported resource, shader and API paths
remain, the selected RTX does not stand in for historical FL9_1 GPUs, and
Lumia runtime plus physical device-removal recovery are unverified. Continue
the broad core/HAL contract audit.

The controlled core fixture initially exposed one stale assertion after depth
`MULTISAMPLE_X4` became query-visible. It now checks that core and HAL agree on
the optional X4 flag while resolve remains absent; strict fixture rerun passed
8/8. Checkpoint `eae4a05c7` records the fixture correction and verification.
Continue from the selected-entry shader and core/HAL contract gaps in
`FL9_1_PLAN.md`.

## Current mission state — cloned public frame lifetime

The public `SurfaceTexture` lifecycle is now shared across clones. Before the
fix, dropping one clone discarded the core-acquired image and presenting the
remaining clone failed with `NothingToPresent`. The strict HWND regression
covers dropping a non-final clone on a worker thread before presentation,
dropping an alias after another clone was presented, and dropping the last
unpresented clone followed by reacquisition. It passed on the RTX 3090 Ti at
requested/actual FL9_1; the complete strict public D3D11 suite passed 62/62,
with the existing optional Float16x2 IA subcase explicitly skipped. The wasm
WebGPU public crate and DX12 public crate both compile with the shared
lifecycle representation. The full
core/HAL audit, historical FL9_1 behavior, CoreWindow and Lumia runtime remain
open.

The HWND teardown audit also caught a second stale-ID path: calling public
`Queue::present` on a frame after dropping its `Surface` previously panicked in
core storage lookup. Queue presentation now serializes with surface destruction
and reports the normal `Surface is invalid` validation error for a dead surface.
The strict test reproduces the old panic, checks this error path while a new
surface on a second hidden HWND has an acquired frame, and then proves that the
new frame still presents normally. It separately drops another unpresented
frame after destroying that surface. The focused test passed 1/1 and the full
strict public suite passed 62/62; the optional Float16x2 input-format branch
remains a visible capability skip.

The public query rejection test covers all three query-set types: occlusion
sets fail through the missing downlevel flag, while timestamp and
pipeline-statistics sets fail through their absent features before reaching
the HAL. The focused strict test passed on the physical RTX at
requested/actual FL9_1. Checkpoint: `6a171483b`.

The HWND smoke test now also starts from `Surface::get_default_config` rather
than a hand-built descriptor. At forced FL9_1, core resolves `Auto` values and
clamps the generic two-frame hint to the surface's supported one-frame range;
the default configuration then presents successfully.

The same strict test exposed and fixes a related destruction hazard: dropping
the surface before its acquired frame previously caused the frame's later
discard to panic on a removed `SurfaceId`. Core surface-output details now
serialize cleanup with surface destruction and skip the stale lookup after
the owning surface is removed. The queue presentation path also checks that
same lifecycle state and reports a validation error rather than looking up a
removed surface. The expanded full strict public suite passed 62/62 on forced
FL9_1; one optional Float16x2 input-format branch was visibly skipped by the
device query.

## Current mission state — wrong-thread present cleanup

The public HWND lifecycle test now acquires a frame on the owner thread,
attempts to present it from a worker, verifies the expected validation error,
then reacquires and presents another frame on the owner thread. Core discards
the consumed frame on wrong-thread rejection, preventing the surface from
remaining stuck with `AlreadyAcquired`. The strict test passed on the physical
RTX 3090 Ti at requested/actual FL9_1; the full public suite passed 62/62, the
controlled core fixture 101/101, and the DX12 core regression 92/92. See the
latest `FL9_1_PROGRESS.md` entry for exact commands. Commit
`8d6c8f93e12c766a5f8fb413f2abf781c4b55408` contains the fix and test. Broader
device-loss and historical-driver coverage remain open.

## Current mission state — unimplemented feature rejection boundary

The strict public suite now verifies `PIPELINE_CACHE` and
`EXPERIMENTAL_RAY_QUERY` remain absent from the adapter feature set and that
requesting either fails before HAL open. The focused test passed on the
physical RTX 3090 Ti at requested/actual FL9_1; the full suite passed 62/62.
This closes a targeted feature-exposure regression gap only. The broader
public contract audit, historical-device coverage and Lumia runtime remain
open. Exact command and results are in the newest `FL9_1_PROGRESS.md` entry.

## Current mission state — public zero-stride indexed-fetch verification

The negative base-adjusted zero-stride point case is now verified through the
public wgpu → core → HAL path as well as the private HAL. The strict public
test passed for Uint16 and Uint32 index buffers at requested/actual FL9_1 on
the RTX 3090 Ti and checked both complete 8×8 images. It does not enable
`vertex_index` or establish historical-device behavior. See the latest
`FL9_1_PROGRESS.md` entry. The mission remains partially complete; continue
the broader public capability audit.

## Current mission state — indexed zero-stride semantics audit

The previously open negative base-adjusted index question is closed for the
supported indexed-point path. WebGPU computes a fetched attribute address as
the bound offset plus the resolved index multiplied by array stride; with
zero-stride vertex-rate streams, negative resolved values retain the same
in-range attribute address. The strict Uint16 and Uint32 indexed-point test
passed again at requested/actual FL9_1 on the RTX 3090 Ti and checked visible
pixel output. Historical FL9_1 behavior remains unverified. See the newest
`FL9_1_PROGRESS.md` entry. Continue the broader public core/HAL contract audit;
the mission remains partially complete.

## Current mission state — 2026-11-07 debug-layer check

The focused private indexed-render/readback test now supports an explicit
`WGPU_DX11_DEBUG_LAYER=1` mode. On the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) at requested and actual FL9_1, the debug layer emitted no
error/corruption diagnostics; its detailed live-object report showed only the
expected device and immediate context after resource cleanup. The complete
strict private HAL suite then passed 377/377, and non-test HAL compilation
passed. This closes a focused diagnostic check only: the full public suite
has not been run with the debug layer, and public contract, historical driver,
CoreWindow and Lumia runtime audits remain open. Exact commands are in the
top entry of `FL9_1_PROGRESS.md`.

## Current mission state — 2026-11-07 verification update

The current tested tree was `6be14b53f`; follow-up validation passed the full
strict private HAL suite (377/377), public D3D11 suite (60/60), controlled core
fixture (101/101), adapter selection (1/1) and DX12 core regression (92/92).
The public offscreen example executed through wgpu on the physical RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and selected at FL9_1; readback matched its expected 1,024-pixel red/green/
background distribution. One optional Float16x2 IA subcase skipped after a
negative selected-device query and is not counted as GPU execution.

The D3D11 HAL and both public examples cross-checked for ARM32 UWP
(`thumbv7a-uwp-windows-msvc`) using nightly 1.95.0 build-std. This is not Lumia
runtime validation. A fresh read-only Device Portal request timed out. The
optional format query also found no float-texture expansion supported by this
forced-FL9_1 RTX, so no capability was added. The practical public subset is
usable for applications within its documented constraints; this is not a
drop-in general wgpu backend or a WebGPU-compliant adapter. The mission remains
**PARTIALLY COMPLETE** while the broader public contract audit, historical
FL9_1 coverage and Lumia runtime remain unresolved.

The physical tests ran with `WGPU_DX11_REQUIRE_HARDWARE=1`; all x64 cargo tests
used Rust 1.93.1. Stable Cargo rejected `-Z build-std`, so ARM32 cross-checks
used the installed nightly toolchain. Exact commands and result details are in
the newest entry of `FL9_1_PROGRESS.md`.

## Current mission state — 2026-11-07 follow-up

Latest verified code checkpoint: `3de9d71ac` (`dx11: document FL9_1 volume
copy restrictions`). The increment records Microsoft's FL9_1 restriction on
3D texture copies, adds a capability-model assertion and a public validation
test that rejects 3D `COPY_SRC`, and preserves the existing query-gated
sample/upload path. The capability test passed 1/1; the strict public 3D test
passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false) at requested and actual FL9_1. Existing volume sampling
matched 64/64 pixels and upload, partial update, zero initialization and
sampling matched 128/128 pixels. HAL and public Clippy passed with existing
warnings; `git diff --check` passed. rustfmt still reports prior discrepancies
in these large files, with the new code aligned.

This is a confirmed hardware limitation, not a backend feature gap. The
broader practical completion mission remains **PARTIALLY COMPLETE**. Continue
the audit from `FL9_1_PLAN.md`; the offscreen and constrained HWND public paths
are usable for their tested subset, but this is not a drop-in general wgpu
backend or a WebGPU-compliant adapter. Historical FL9_1 drivers and ARM32 UWP
runtime remain unverified.

## Current mission state — 2026-11-07

Latest verified code checkpoint: `f336cc394` (`dx11: support statically
unrolled FL9_1 shader loops`). The strict private D3D11 suite passed 377/377,
the strict public D3D11 suite 60/60, the controlled core fixture 101/101,
selected physical-adapter enumeration 1/1 and the DX12 core regression
92/92. The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false) was explicitly requested and selected at FL9_1. A new public
test rendered 64/64 expected pixels with statically unrolled loops in both
shader stages. Tested dynamic loops remain unsupported and fail FXC during
pipeline compilation. One public Float16x2 vertex-input subcase skipped on a
negative device-format query; it is not counted as GPU execution. Clippy
passed with existing warnings, `git diff --check` passed, and rustfmt still
reports older formatting differences in touched files.

The backend is usable now for an application whose needs fit the documented
and exercised subset; it is not a drop-in general-purpose wgpu backend and
does not provide Core WebGPU conformance. Public adapter exposure is an
explicitly selected D3D11 path with constrained HWND presentation. The
selected modern GPU does not establish compatibility with historical FL9_1
drivers, and Windows 10 Mobile ARM32 runtime testing is still unavailable.
The broader public capability and core/HAL contract audit remains active, so
this mission is **PARTIALLY COMPLETE**. Continue from `FL9_1_PLAN.md` and the
latest dated entry in `FL9_1_PROGRESS.md`; do not treat the recent rendering
coverage as proof that every FL9_1-valid wgpu operation is implemented.

## Current mission state — 2026-11-06

This mission remains active on branch `uwp-dx11`; the latest code checkpoint
is `cb2e06cecae0a2b6dab1622558deb3ef2195dbeb` (`dx11: test public Present
device-loss propagation`), followed by documentation checkpoint `2deeffc49`.
The strict public D3D11 suite passed 60/60 on the physical RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), requested and actual FL9_1. The public suite explicitly
reported one unsupported Float16x2 input-format subcase; it is not counted as
GPU execution. The new public render-bundle case switches sparse uniform and
sampled-resource groups, drops their source handles, then validates all 64
readback pixels. Additional bundle tests select two dynamic uniform offsets
from one source buffer and switch from a uniform-only layout to a
texture/sampler layout; both verify output after their source resources are
dropped. A further pair uses writable D16 depth from a bundle; the farther
draw correctly fails against the first bundle's depth write, and all 64 color
pixels remain red. A single bundle also ran twice while pass-level scissor and
blend constants changed; readback verified all 64 red/green pixels.
One further test found that Core failed to replay the pass stencil reference
after changing to a bundle pipeline. Core now reapplies that value after each
bundle pipeline that uses it; the test stores references 1 and 2 in separate
scissor regions and verifies stencil-based red/green output across all pixels.
The 60-test public suite, core fixture (100/100), and DX12 core suite (92/92)
were rerun after this Core change. The private HAL suite and adapter
enumeration test last passed at checkpoint `f04b4f93e`. The CoreWindow admission check
passes CPU coverage and x64/ARM32-UWP compilation, but UWP runtime behavior
remains unverified. Public CoreWindow surface capabilities remain withheld.

The public `wgpu` library cross-check compiles with nightly `build-std` for
`thumbv7a-uwp-windows-msvc`, and the public offscreen example links as an ARM
executable when Visual Studio's ARM store environment and SDK 10.0.22621.0
are selected. The Lumia Device Portal timed out, so installation and runtime
behavior remain unverified. The backend remains an explicit opt-in,
non-WebGPU-compliant D3D11 path. It
supports controlled public offscreen rendering and constrained desktop HWND
presentation; the consumer example validated a 1,024-pixel textured indexed
cube. The core/HAL contract audit is ongoing, Windows 10 Mobile ARM32 runtime
verification is unavailable, and the mission remains **PARTIALLY COMPLETE**.
Exact commands and distinctions between GPU execution, synthetic checks and
skips are in `FL9_1_PROGRESS.md`. All checkpoint summaries below are
historical snapshots unless explicitly updated.

The latest checkpoint also verifies the WGSL constant texel-offset form of
`textureSampleBias`: core and the private HAL reject direct and helper-reached
uses, and FXC returns X4532 for Naga-generated shaders under both level-9
profiles while the SM5 control compiles. The full strict HAL suite is now
368/368; public D3D11 remains 50/50, the core fixture 96/96 and DX12 core
92/92. See the newest follow-up in `FL9_1_PROGRESS.md`; counts in older
checkpoint notes below are historical snapshots.

Strict physical tests ran on the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), hardware adapter, requested and
actual FL9_1. Latest results: private HAL 367/367, public D3D11 50/50,
controlled core fixture 96/96 and DX12 core 92/92. The optional Float16x2 IA
subcase visibly skipped because the selected device did not report its input
format support. `wgpu-types` passed 85 unit tests and the serde compatibility
test passed 1/1. HAL `dx11,gles`, `wgpu-info` and `wgpu-test --tests` checks
passed. Core Clippy has one existing fixture warning; HAL Clippy has nine
previously recorded warnings. Public-test and serde-enabled types Clippy,
changed-file Rustfmt and `git diff --check` passed.

The opt-in public D3D11 backend supports a conservative offscreen path and
constrained owner-thread HWND presentation; it is not WebGPU-compliant.
Historical FL9_1 drivers and ARM32 UWP execution remain unverified. The
development GPU positively reports unrestricted NPOT support, so the legacy
conditional-NPOT negative path is synthetic. Continue the selected-entry
shader/compiler, resource-usage and render-pass contract audits; do not treat
this checkpoint as completion.

**Outcome: PARTIALLY COMPLETE.** The mission is active. The explicit opt-in
`dx11` backend enumerates physical adapters that successfully create an FL9_1
device and exposes a conservative public projection for offscreen rendering
and a constrained HWND surface. A standalone public wgpu example renders and
validates an indexed textured depth-tested 3D cube. The adapter is not
WebGPU-compliant. The private HAL has tested D24S8 depth/stencil,
depth-only prepasses, stencil operations, read-only attachments, depth-bias
subsets, multiple texture formats and mip views, uniform layouts/ranges, and
indexed/nonindexed point, line and triangle topologies on a modern RTX 3090 Ti
explicitly forced to FL9_1. Historical FL9_1 compatibility has not been
verified. Public HWND presentation remains intentionally constrained to its
tested owner-thread contract; historical drivers and ARM32 UWP remain
unverified.

The latest core/HAL audit adds a selected-entry
`textureSampleBaseClampToEdge` downlevel limit. FXC rejects Naga's
`GetDimensions`/`SampleLevel` lowering with X4532 in both 9_1 and 9_3 pixel
profiles; public validation now catches direct and helper-reached use before
HAL while allowing safe sibling entries. Targeted, strict public, core fixture,
DX12 and full HAL suites passed; the physical RTX 3090 Ti executed the strict
FL9_1 GPU cases. Details are in `FL9_1_PROGRESS.md`.

Latest code checkpoint `3b1d4f964` handles indexed LineStrip and TriangleStrip
draws with no vertex-rate inputs. Replay reads current index-buffer contents,
matches restart after signed base-vertex resolution, splits runs and issues
bounded direct strips; instance-only layouts rebind their stream per emulated
instance. Strict private and public readback suites passed at requested/actual
FL9_1. Full results: HAL 343/343, public wgpu 13/13, D3D11 core fixture
86/86, and DX12 core 82/82. The GPU comparison is degenerate because the
supported shader cannot observe vertex identity without a vertex input; exact
run lengths and native batching are asserted by the CPU planner test. A
nonzero index-buffer binding offset and same-submission index update remain
useful follow-up coverage. Details and commands are in `FL9_1_PROGRESS.md`.

Latest core/HAL contract checkpoint: `a3a6a177c` adds the public downlevel
`texture_load` limit and rejects selected FL9_1 shader entries that reach
Naga `ImageLoad`, including through helper functions, before HAL pipeline
creation. Safe sibling entries remain accepted. Strict core and public tests
pass on forced FL9_1 hardware. Public HWND presentation remains the next
architectural investigation; see `FL9_1_PROGRESS.md` for exact test commands.

The public strict suite now has a third test for D24S8 stencil: it writes a
stencil reference, verifies passing and failing equality draws by checking all
64 output pixels, and confirms asymmetric front/back state is rejected before
HAL. Checkpoint `64894f9c8`; the full strict public suite passed 3/3 on the RTX
3090 Ti at requested/actual FL9_1. The obsolete initial backend README has
also been replaced by an accurate opt-in/offscreen summary (`ee05538a3`).

Latest public render checkpoint `28d3bbddd` adds a strict premultiplied blend
test through public wgpu; forced-FL9_1 readback matched `[128, 0, 128, 255]`
for all 64 pixels. Core rejects unsupported source-alpha-saturated and
separate-alpha descriptors before HAL. The complete public suite passed 4/4 on
the RTX 3090 Ti. Independent Present audits confirmed the current deadlock path
and found no safe lock-release-only fix. A correct surface design likely needs
ordered asynchronous per-device scheduling; keep public surface capabilities
unavailable. Exact analysis and Microsoft references are in
`FL9_1_PROGRESS.md`.

The shared upstream `wgpu-gpu` Naga capability selector now handles the
opt-in D3D11 backend using HLSL's capability ceiling. Its initial focused run
was skipped for oversized default limits; the FL9_1-specific test limits fixed
that, and the rerun executed and passed on the physical RTX 3090 Ti. In the
full upstream GPU harness, Cargo reported 351 tests passed, but only one
executed; 330 were unsupported by limits and 20 skipped by expectations. The
workspace test-report setup required a temporary DX11-only feature selection
because this Build Tools install lacks `atls.lib`; the workspace manifest and
existing `.gpuconfig` were restored. The D3D11 adapter is not WebGPU-compliant,
so do not add it to a CTS runner. See `FL9_1_PROGRESS.md` for exact outcomes.

The latest strict public suite passed 5/5 at forced FL9_1. The new scissor case
rendered 24 green pixels within a `4×6` rectangle and preserved 40 blue pixels
outside; a left-overflow viewport was rejected by core before HAL. Checkpoint
`ee430b9b2`; exact command and the complete public test inventory are in
`FL9_1_PROGRESS.md`.

## Current checkpoint — internal FL9_1 capability projection — `dfc93078a`

`Capabilities::wgpu_projection` now builds a selected-device FL9_1 snapshot of
limits, alignments, downlevel gates, features and per-format support. The
controlled core fixture consumes it rather than a duplicated handcrafted
capability set. It remains test-only; ordinary D3D11 enumeration is empty.
The projection withholds `TEXTURE_COMPRESSION_BC` because only BC1 is
implemented, while the fixture locally enables the aggregate feature solely
for its BC1 path. Core now rejects unsupported 1D and 3D shapes through
`DownlevelLimits` booleans while other backends retain true defaults.

The strict full core fixture suite passed 78/78 and the strict private D3D11
suite passed 337/337 on the RTX 3090 Ti at requested/actual FL9_1. Projection
tests passed 4/4; DX12 core passed 75/75; wgpu-types passed 65/65 (66/66 with
serde); the ordinary zero-adapter regression passed 1/1. Build/check, Clippy,
targeted formatting and diff checks passed. The suite records the initial
RGBA8 color-attachment limit correction (wgpu sample cost is 8, not its
four-byte storage size). Full commands and existing warning counts are in
`FL9_1_PROGRESS.md`.

Next audit every projected numeric limit, per-format usage and core-accepted
operation against the private HAL and its validation gates. The forced modern
GPU is not evidence for historical FL9_1 behavior. Public adapter and HWND
surface exposure remain gated.

The latest code checkpoint is `e8c7240da` (`core: gate FL9_1 fragment position
use`). `DownlevelLimits::fragment_position` is false for the controlled FL9_1
fixture. Core rejects a selected fragment entry that consumes
`@builtin(position)`, while allowing an unused declaration and safe sibling
entry. The analyzer uses validated Naga expression information and follows
helper arguments, including forwarded structs and vectors. The strict core
fixture suite passed 77/77 on the NVIDIA RTX 3090 Ti at requested and actual
FL9_1; the core DX12 suite passed 74/74, `wgpu-types` passed 63/63, and the
ordinary D3D11 zero-adapter check passed 1/1. HAL/GLES, wgpu-info and test
crate checks, core Clippy, targeted formatting and diff checks passed. The
private HAL suite was not rerun for this core validation-only change. See
FL9_1_PROGRESS.md for exact commands and warnings. Public adapter and HWND
surface gates remain closed.

The documentation-only checkpoint `e5fe9207c611234c1669da48892ec12b80ebcfab`
reconciles the capability/testing/status pages with the corrected repeat-cap
fixture. It does not change code or test behavior.

The preceding code checkpoint is `70e31a0f175803ced62134e10cd3f11cc8553886`
(`core: honor FL9_1 repeat limits in fixtures`). Both controlled core fixtures derive the documented per-primitive
repeat limit from their selected FL9_1 device instead of advertising
`u32::MAX`. The strict fixture rejects Repeat and MirrorRepeat sampler
creation at the 128 limit, then runs the remaining rendering cases with
clamp-to-edge POT/NPOT resources. The focused physical fixture test and full
75-test core suite passed under strict hardware mode on the RTX 3090 Ti at
requested/actual FL9_1; HAL/core Clippy, formatting and diff checks passed.
Private HAL repeat support and synthetic conditional-NPOT analysis remain
covered separately. This is still a reduced test profile, not the pending
public capability snapshot. See FL9_1_PROGRESS.md for the initial correction
and exact test results. Public enumeration and surface exposure stay closed.

The preceding code checkpoint is `95fb67e22` (`dx11: preserve fractional viewport
mapping at FL9_1`). The private HAL uses integral native viewport bounds and a
vertex remap based on the original fractional viewport. Strict RTX 3090 Ti
readback passed through both the HAL test and controlled core fixture at
requested/actual FL9_1. The core test uses contained geometry; it does not
misinterpret a viewport as a scissor. The full private HAL suite passed
333/333, the fixture-enabled core suite passed 75/75, normal D3D11 adapter
isolation passed, and the DX12 core suite passed 72/72. Exact commands, the
initial incorrect fixture expectation and compiler-environment diagnostic are
recorded in FL9_1_PROGRESS.md. Historical 9_1 viewport precision remains
unverified; public enumeration and surface exposure stay disabled.

The preceding code checkpoint is `19ff6dff6` (`core: gate clipped viewports on
downlevel capability`). Core rejects viewports extending past the active
target when `DownlevelLimits::viewport_rectangles_outside_target` is false;
this prevents optional FL9_1 viewport shader compilation failures from
surfacing as late draw failures. Defaults preserve existing backends, while
the controlled fixture reports false and tests all four edges in CPU logic and
the production validation path. The strict fixture-enabled core suite passed
75/75 on the RTX 3090 Ti (`0x10de:0x2203`) at requested/actual FL9_1; existing
render/readback cases passed, and the viewport rejection itself issued no GPU
draw. The strict private HAL suite passed 326/326, with six unsupported-shader
probes explicitly skipped after compiler rejection; core DX12 passed 72/72 and
zero-adapter isolation passed 1/1. Exact commands and pre-existing lint
warnings are recorded in FL9_1_PROGRESS.md. Public enumeration remains
disabled.

The historical core NPOT checkpoint is `bb22b4636` (`core: validate FL9_1 NPOT sampling
pairs`). That checkpoint preceded the finite-repeat fixture correction in
`70e31a0f1` and used an unbounded test-only repeat limit to reach draw-time NPOT
pair validation. The current FL9_1 core fixture correctly reports 128 and
rejects Repeat/MirrorRepeat sampler creation. Core retains selected-entry Naga sampling relationships and validates
current bound texture/sampler descriptors at draw encoding. A strict fixture
on the RTX 3090 Ti requested and selected FL9_1; the physical driver reports
full NPOT support, while the fixture synthetically forced the conservative
path. It rejected a used NPOT/repeat pair before submission and rendered 64/64
yellow pixels for POT/repeat plus NPOT/clamp despite an unused incompatible
cross-pair. The full strict core suite passed 73/73, the private HAL suite
passed 323/323, zero-adapter isolation passed 1/1, and the core DX12 suite
passed 70/70. Details and exact commands are in FL9_1_PROGRESS.md. Continue
the core/HAL capability-boundary audit; do not enable public D3D11 enumeration
or surface exposure.

Latest related code checkpoints are `983eea619` and `834cd9856`. The first
adds a core downlevel gate for multi-mip depth/stencil render attachments,
matching the FL9_x D3D11 requirement; the second preserves this shared flag in
Vulkan's explicit capability list. The strict controlled core fixture rejected
the descriptor before HAL on the RTX 3090 Ti at requested and actual FL9_1; its
full fixture suite passed 71/71 and the full strict private HAL suite passed
322/322. The prior `5fe21f035`
checkpoint verifies all eight native stencil operations in pass, stencil-fail
and depth-fail outcomes. Twenty-four full 8x8 color readbacks passed on the RTX
3090 Ti at requested and actual FL9_1; the stencil test uses later Equal draws
because this HAL cannot read DSV stencil storage directly. The prior
`6f7f12978` checkpoint adds finite slope-scaled triangle bias, while
`1a362ec61` separates FL9_1 hardware guarantees from backend implementation.
Exact commands and remaining restrictions are in FL9_1_PROGRESS.md. Continue
the public HAL/core capability-boundary audit and truthful capability snapshot
in FL9_1_PLAN.md. Do not enable normal enumeration or public HWND presentation.

The indexed TriangleStrip checkpoint `f1b8c09a7` and strict core-fixture
coverage `c46c4a908` are followed by static boundary execution tests for vertex
c254 and pixel c31. At forced FL9_1 on the RTX 3090 Ti, the test verifies that
the vertex uniform shifts geometry and the pixel uniform changes color (17
green and 4 red pixels). Feature checkpoint `0c0ff00ba` follows the original
c254 checkpoint `6cac808c9`. Dynamic integer indexing remains rejected. The
full strict HAL suite passed 305 tests (two filtered); see FL9_1_PROGRESS.md
for exact commands and the limits of that evidence. Continue from
FL9_1_PLAN.md and update the progress journal after each verified checkpoint.

Resume on `uwp-dx11`; inspect `git status` and recent history before editing.
Viewport emulation was checkpointed as `ec645e63e` and retains the
compiler-budget limitation recorded in FL9_1_PROGRESS.md. Dynamic uniform
offsets were checkpointed as `edb1efa6b` after strict GPU and regression tests.
The private swapchain locking and selected-entry shader-load validation
changes have passed strict tests; check the working tree and recent commits
for their checkpoint status. Swapchain locking does not resolve DXGI window-callback
reentrancy or presentation completion in `wait_for_idle`. Do not infer full
FL9_1 or historical-device compatibility from
modern GPU tests. Keep public adapter enumeration gated until core-valid
operations are safe. The latest strict private HAL suite passed 301 tests with
two filtered, and the current core suite passed 70/70 including strict FL9_1
GPU fixtures. The focused line-topology test and core fixture ran on the RTX
3090 Ti at explicitly requested and selected FL9_1. Older HAL tests may skip
internally, so use focused strict tests for hardware evidence.

The current public-surface gate is an unresolved cross-layer reentrancy issue:
DXGI may synchronously send messages to the HWND's window procedure during
`Present`; both core's queue-index lock and the HAL immediate-context lock span
that call. See the latest section of FL9_1_PROGRESS.md and the Microsoft
threading references there. Do not drop either lock without a replacement
serialization design. Continue independent private FL9_1 work while keeping
public D3D11 enumeration disabled; the next planned task is the remaining
core-valid descriptor and command audit before constructing the public
capability snapshot.

## Verified checkpoints

The autonomous work began with `26739b52d1ae042a6499b578521eb3250f1d9c21`
(generalized static uniform buffers), then added:

| Commit | Scope |
| --- | --- |
| `19e82da409dd0ceec12eb9e9dca8934900b5ff11` | Nested uniform layouts and mission records |
| `424dfaa2f46e757e9d3a0dad9c50b9f110a18592` | Private textured D24S8 depth rendering |
| `e09825a90b32dc52eb83cd27f084c2803abf96c5` | FL9 default buffer bind flags |
| `8949812c55514e106e7803e00d7e25801e516d44` | Downlevel index usage validation |
| `7bdffefe244101bf7374fbc10044424f08eea8c1` | Adapter error expectations |
| `26825d96e8da4d9864f741527286d99a9494b53d` | Empty compute passes and unused blend constants |
| `4afbb21da9f1f319ff39fe73f2dceed58935a279` | Zero-sized viewports |
| `d13062b65c53a5a5fcac1f644bbec0d581d7e078` | Non-finite viewport rejection in core |
| `721469cfeabbb86995d3654b84eff1bbeae5d4a6` | Attachment discard flags |
| `bf56c17b99352a354cae3a4929504880fdc395ef` | Private hidden HWND surface |
| `7e045c4ed83e9506054161da24c4f13a4986af7b` | Surface-view lifetime and resize |
| `ec645e63e084fad3c377ccaf0f3b21ed49ee9e13` | Restricted clipped viewport emulation |
| `edb1efa6b3c07ac080a5a38f85de83282079b112` | Dynamic uniform offsets |
| `9464c4aef6bd02cb37dfd8aba5adaa0bb697d5ad` | Private swapchain/context serialization |
| `7a7f249f5bc54cfc67678fc00755b9d8429e7338` | Selected-entry texture-load validation |
| `4bba5f9c53e1ed70fa8569cc9ed7eeec30f06a0f` | Single-target color write masks |
| `f8bd5183a770dd7e81568516f6a3e3411a8de7d8` | Triangle face culling and winding |
| `f4df16d8b` | Pipeline-wide register planning |
| `a914c9a1f95f7806069051deef55630b9a8244f5` | Private multi-group uniform bindings |
| `47e8488ef19be593b8d4e899a567f2c302d73861` | Private cross-group sampled resources |
| `f6aa8f241415822eb2a58f8f992e2ff7e7123ba4` | Gated core buffer round trip |
| `86db48c45` | Gated core offscreen rendering |
| `6fe0282e7` | Zero inter-stage validation limits |
| `28c4a2f1a` | Gated core textured depth rendering |
| `4c2255de2` | Private checked indexed base vertices |
| `3d716ab7b0ec497eb6e77c5f977969a630b258b7` | dx11: test public wgpu rendering with a gated fixture |
| `e7bf588fd2127ade3a346bf00eb9936d6b50fa5c` | dx11: discard out-of-bounds indexed draws |
| `cd2c653b9123e5d77cb1e39ed92aee1681a1d0ce` | dx11: test FL9_1 vertex index shader inputs |
| `8873489b092546367c1bac41b83f6a590644bb14` | dx11: support FL9_1 depth-only drawing |
| `555a77862e6ba00f95650893457fbab62e5b4adc` | dx11: translate bounded FL9_1 32-bit indices |
| `39b439cebe0aca0359f4fd08e252ef98968a9114` | dx11: rebase bounded 32-bit index windows |
| `ee5216e5e5b2412af2073ef985ac294ed78aa3d3` | dx11: emulate restricted FL9_1 direct instancing |
| `90238dedebc8739d2dd6eb1ef967d889b4121c02` | dx11: test indexed rendering through public wgpu |
| `730b4dfec684db09ebeec2bfd9d2ce7063f6df5f` | dx11: split 32-bit indices into FL9_1 windows |
| `1033fc9f623d7414cd3817c3683e701483c1da72` | dx11: emulate restricted indexed instancing |
| `e29a2e11b6cd6575b3edadbcba629cf27931ec29` | dx11: gather sparse FL9_1 triangle vertices |
| `d8c9f0fb9eb2773d5aee2ec56d4f458fc59a8d7f` | dx11: reject unsupported primitive index inputs |
| `1a1d7a14d0939da90c09ffac6204f8562536f083` | dx11: extend FL9_1 indexed draw compatibility |
| `26992d2957fe2ae04dbf1e7fdf157e83e3fe61cc` | dx11: support zero-input FL9_1 point rendering |
| `d01ace12a78f6a78e0ca4c8f12524f69bca718ed` | dx11: enforce FL9_1 blend capability limits |
| `86a5ddfb5ffce4dfeb3709d9d6cc0caf0a55f434` | dx11: support FL9_1 BGRA8 textures and transfers |
| `17b840c1d5e96ffda2c8a71efe9b5a160063559d` | core: validate downlevel occlusion query availability |
| `03cf563b1864e4dde958c8cb3d93963143d07cf7` | dx11: support restricted read-only depth attachments |
| `7c4fd052c5019b98b49855095bdabeeea5ef4369` | dx11: allow checked local vector indexing with uniforms |
| `d77a143874521e25052bf07c77367270d72b0d20` | dx11: reject unsafe matrix aggregate shader stores |
| `ec8dbf12e42a5217655cc9a819dc1247771bb77b` | dx11: support nonindexed FL9_1 triangle strips |
| `3cdbb28f6dc1b79ff464bdfa5649f6546bb8a589` | dx11: reject overlapping FL9_1 vertex elements |
| `07b95c80159b1ed535c6359cec5b65690e409653` | dx11: extend matrix guard regression coverage |
| `894aa40c442589b72ac07c9d72c8c55475cf1ea4` | dx11: bound FL9_1 direct triangle batches |
| `82a5384336c097aa310a0d5315e2472fde374333` | dx11: support sparse FL9_1 vertex streams |
| `9a1efde6ce5d97abeb0b4e8fb9d90168e7f3501e` | dx11: support sparse FL9_1 triangle strips |
| `9bee6efe5478fe09a6a960c3d7f6b0d14a4eb1d0` | dx11: support bounded u16 indexed vertex streams |
| `954e07c28173c667f109c367e49b82bec148a21c` | dx11: translate indexed draws across vertex streams |
| `672db3fb3b883a2f2119942cc98ea09206a44981` | dx11: support transient FL9_1 attachments |
| `53abef8bcda4600b61bce04dff3ec5d431ec3bf2` | dx11: support overlapping vertex attributes on FL9_1 |
| `c9a4105b222f531029fb470178d4ffe83383297d` | dx11: support inert FL9_1 stencil states |
| `cca55a8f043dac87b41c501d0c517bf46ee8a3fd` | dx11: support signed normalized vertex inputs |
| `cc8e1aa68d9b44592a584b515171293b42714136` | dx11: support zero-stride vertex streams |
| `8653ba625e6069996e5034f58374f4ac53fae381` | dx11: emulate FL9_1 instance-rate vertex streams |
| `fc5cfb75e085b5d9b90ea60d472a7b935ec426b6` | dx11: correct vertex stream capability count |
| `61d1e7a3e0f1079628087af2088a1f6e5bfc1e41` | dx11: materialize short uniform buffer ranges |
| `62ef2660b7d2ce7e8637da98694774fa4bf67b16` | dx11: preserve single-mip fractional LOD clamps |
| `6a64195adb09fed59984167368d41080e609894a` | dx11: retain copy-only texture views |
| `1f48812f7ced0cf906ae03ca776d17e3d4432bef` | dx11: reject unsafe level-9 sample bias |
| `352c415eb0b82bfb823112b992f7309be419f917` | dx11: support copy-only FL9_1 mip chains |
| `e066d405cf7b2eb110ffec9b562d0acdd26759fc` | dx11: sample full FL9_1 mip chains |
| `34794bb52b7389492ffcbd989c09d4de8121c95f` | dx11: support suffix FL9_1 mip views |
| `4a4b6eec90c12a8d3ba35629311ab1d58e052284` | dx11: support upload-only sampled mip textures |
| `68c6a804d13cada3d1cb0feccef6bbb3d1d58d7f` | dx11: support FL9_1 color attachment mip views |
| `2206731ded87a5cdc1c9bb3c557191027c10397b` | Render and sample FL9_1 mip textures |
| `d62f52eaca76e4a8ff50eabbc9ee09910d94ffb5` | Bounded FL9_1 texture sample bias |
| `9e9c21392b0996c1ea2b6489c11c268c37379206` | Emulate non-suffix sampled mip views |
| `b431ff5d013dc9050f729b00a92973f961676ff1` | Dual-use FL9_1 mip views |
| `07eaee830f8600b1b56391089ab3df03a20bcddf` | Disjoint native mip sampling |
| `3e0fdcb9f9e8330e7cb900a021303d0e6d22c878` | Disjoint clone-backed mip sampling |
| `8045637708e23b1d4de8c54f1ddde3d54bf0c6c8` | Emulate Never depth comparison |
| `3de27b02b7659b028db3606e0e3047e07f9e75b8` | Reject unsafe FL9 integer shader semantics |
| `4cba5bf6fb21ac23699fe1b012808d0852c6f0e9` | Point draws with vertex streams |
| `8c2be917a31a0e124007b77989e2365a83fc376a` | Ordered indexed point emulation |
| `064d68dc2d98573b28730de5ba5e5f9c3aa6f83f` | Restricted constant D24S8 depth bias |
| `ddb1b7cd9d559af88ee9d16c9d73a69b1101edc0` | Same-texture distinct-mip copies |
| `735a9755645b54c06c9a1084ebf66ef49e4848e8` | COPY_DST-only mip textures |
| `7404886b4dd14da39c4859b5a33243c31f9ddd36` | Non-filtering sampler layouts |
| `4ef1fbdaafd138b5981b5765c6a33a887b54b594` | Unfilterable float layout relationship validation |
| `4f3f608c229fd6e5b7bc309db5ac975bfd790c7e` | Combined sampled-resource visibility |
| `cac502305de4c2569bfc29da73eecdd9a59e92e8` | Premultiplied alpha blending |
| `ceaf4d603dc4ecbc6449b520f5a9ef546b5cfa64` | Safe D3D11 passthrough shader rejection |
| `a690866b4a6cdc0981a1ffa448f2c28f0f41b756` | Fallible mapped-range invalidation |
| `13a4defb9` | Fallible mapped-range flush and core error propagation |
| `2ff5a58e8` | dx11: emulate FL9_1 indexed line-strip restart |
| `f1b8c09a7` | dx11: emulate FL9_1 indexed triangle strips |
| `c46c4a908` | docs: record FL9_1 indexed triangle-strip checkpoint |
| `6cac808c9` | dx11: verify FL9_1 final vertex constant execution |
| `0c0ff00ba` | dx11: execute FL9_1 pixel constant boundary |
| `340147803` | dx11: support native FL9_1 depth comparisons |
| `6f7f1297891ac3477ed7055ded00934da1c46c2a` | dx11: support FL9_1 slope depth bias |
| `a281a4c3f2c9e5efa91be992c5cbd401b9cec33c` | dx11: verify FL9_1 depth clamp core gate |
| `5fe21f035` | Strict FL9_1 tests for all stencil operation outcomes |
| `1a362ec6108de9accb52e2f1978a174d1f923106` | dx11: model raster depth-bias capabilities |
| `983eea619` | core: gate multi-mip depth attachments by capability |
| `834cd9856` | hal: report multi-mip depth support on Vulkan |

## What is and is not usable

The `dx11` feature exposes an explicitly selected, opt-in public wgpu adapter
path. It enumerates physical FL9_1 D3D11 devices and uses a selected-device
capability projection; the default backend priority is unchanged. A public
wgpu example renders and validates a textured indexed depth-tested cube offscreen,
and a separate HWND example visibly presents a bounded 60-frame scene. These
prove a useful consumer path, not general rendering compatibility.

The current public subset includes restricted WGSL-to-Naga-to-HLSL-to-FXC
shaders, supported sampled and renderable formats, multiple bind groups,
static/dynamic uniform ranges, texture transfers and views, triangle/line
rendering, culling, depth/stencil subsets, and validated readback. Exact limits,
negative gates and unverified combinations are listed in `CAPABILITIES.md`.
This remains experimental and is not a WebGPU conformance claim.

Public presentation is limited to the tested Win32 HWND contract. The private
CoreWindow path is not exposed through public surface capabilities. CoreWindow
construction now verifies dispatcher access and captures the owner thread, but
there is no UWP render/readback run. ARM32 UWP and historical FL9_1 hardware
remain unverified. `wait_for_idle` has GPU event-query evidence, not evidence
that presentation completed.

The opt-in adapter does not make every core-valid state safe. Continue auditing
resource shapes, vertex/pipeline combinations, shader/compiler limits, errors
and lifetimes; add implementations or narrow validation where the public
projection still permits unsupported operations. Private HAL failures can
surface as device loss, though synthetic removal paths now verify core
invalidation and callback behavior. Clipped viewport emulation can exceed FXC
instruction limits. FL9_1 does not guarantee separate-alpha blending, so the
backend rejects modes requiring it while retaining tested masked-channel
cases. Native compute and UAV storage are unavailable in the current FL9_1
contract. Do not replace these gaps with fabricated capabilities.

Core's downlevel gates reject unsupported vertex/fragment storage layouts and
occlusion query sets before HAL. The unsafe owner-matched fixture remains
useful for cases that need internal device access, but public examples and
integration tests now also exercise ordinary wgpu adapter, device and queue
creation. Public access stays opt-in and its projected contract still needs
the remaining audit.
Private read-only D24S8 passes require load/store of both aspects, no sampled
aliases and no depth or stencil writes. Keep the public read-only flag off.
Runtime integer-based vector indexing is now rejected by the conservative
integer guard in checked and unchecked shaders. Earlier dynamic-index GPU
results are historical; static literal indexing and float-only controls remain.
Nonindexed triangle strips use parity-preserving, rebased batches. Indexed
LineStrip and TriangleStrip do not rely on native FL9_x strip-cut support:
replay resolves the base vertex, splits fixed-index restart markers, validates
all non-restart fetches, and translates the runs to bounded native draws.
TriangleStrip expands each run in winding-preserving triangle-list order,
using compact Uint16 windows or per-stream gathers for wide triangles. Both
indexed strip formats are covered by private and controlled-core GPU tests.
FL9_1 overlapping vertex attributes are partitioned into separate native input
slots that bind the same logical buffer and preserve its original bounds. Private
and controlled-core GPU tests passed; historical-driver behavior remains unverified.
Private direct triangle lists and nonindexed strips support up to sixteen sparse
vertex-rate slots. The existing slot-0 indexed path remains available. Multi-slot and nonzero-slot
indexed triangle lists accept Uint16 and Uint32. GPU index data is scanned
at replay against every stream; any out-of-range fetch discards the entire draw.
Native-safe Uint16 uses a direct path; other draws use common translated windows
or per-stream GPU gathers for wide triangles. All segment resources are prepared
before drawing. Index staging can stall and historical drivers remain unverified.
Naga's HLSL special handling of matrix stores in structures can omit an index
clamp or pass a destination by value. The private HAL now rejects stores through
matrix-containing aggregates in both shader modes with a bounded selected-entry
analysis. This intentionally rejects some safe writes and does not fix shared
Naga; historical compilers and drivers remain unverified.
The Dx11 unreachable in core's shader path is specifically in unsafe passthrough,
not normal validated WGSL; its reachability still needs an audit before exposure.

## Historical private-HAL reproduction snapshot

The commands and counts below are an early private-HAL snapshot and are
superseded by the current public integration results at the top of this file
and the latest checkpoint in `FL9_1_PROGRESS.md`.

On Windows 11, Rust 1.93.1 and the MSVC developer environment, use
`CARGO_HOME=C:\dev\personal\wgpu\target\codex-cargo-home`,
`CARGO_TARGET_DIR=target/codex-milestone1`, `CARGO_BUILD_JOBS=8`, and
`WGPU_DX11_REQUIRE_HARDWARE=1`. In an x64 Visual Studio developer shell:

```text
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 dx11:: -- --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test_preserves_near_geometry_and_loads_depth -- --nocapture
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo clippy --offline -p wgpu-hal --lib --tests --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture
```

The strict private suite most recently passed 244 tests with two filtered;
some older tests can skip internally. The focused physical-GPU color-mask test
reported NVIDIA RTX 3090 Ti, vendor 0x10de, device 0x2203, driver 32.0.16.2101,
requested and actual FL9_1. It checked all 64 pixels for each of 16 masks,
three transition regions, zero color writes with depth writes and masked
additive blending. Multiple uniform groups, dynamic offsets, changed register
plans, and group-0 texture sampling with a group-1 uniform also passed strict
GPU readback. Cross-group textures and samplers, shared and multiple samplers,
transitions and feedback rejection have also been exercised; see the progress
log and `git status` for checkpoint status. The focused hidden HWND test read
BGRA [0,0,255,255] and
reported successful present and recreation. Core's zero-adapter test passed;
the fixture-only core GPU upload/copy/map test matched 64/64 bytes, its
offscreen draw matched 64/64 red pixels, and the
fixture-enabled core library suite passed 67 tests. Ordinary enumeration and
normal adapter opening remain disabled. The fixture-on core Clippy check passed.
targeted Clippy completed with ten existing warnings. Earlier work also ran
Naga, core, public Noop validation and targeted DX12 regressions as recorded
in FL9_1_PROGRESS.md. No public D3D11 example or visual display test ran.

The opt-in `wgpu` integration test used the public Rust API with an unsafe
injected fixture. On the forced-FL9_1 RTX, GPU-uploaded Uint32 indexed textured
depth rendering produced 32 red and 32 green pixels. Ordinary D3D11 enumeration
remained empty before and after injection. This is a controlled test, not
general public exposure.

Ordinary BGRA8 textures and transfers are supported in the private subset.
Native copies require identical formats; upload/readback preserve raw BGRA
bytes, while shader sampling and render targets use logical RGBA channels.

Core's zero inter-stage limit panic was reproduced in a public Noop test and
fixed with signed deduction arithmetic. The expanded focused test and the
fixture-enabled core suite (68 tests) passed; see the progress log for details.
The controlled core fixture has also rendered a sampled texture with depth on
the forced-FL9_1 RTX: readback matched 32 red and 32 green pixels. This is an
unsafe, feature-gated fixture rather than ordinary public adapter enumeration.
The private indexed path also passed signed base-vertex GPU tests using
execution-time index data. Out-of-bounds vertex fetches discard the whole draw;
bounded u32 data converts to u16 scratch, while valid public draws beyond its
conservative native limits still require emulation.

Known costs include per-group scratch allocation, per-draw staging and mapping
of uniform ranges, and buffer-texture
uploads, and GPU readback for checked indexed draws. They can stall the CPU;
no representative frame-time measurement has been made. Device removal,
mapping error paths and surface thread reentrancy require additional audit.
Mapped-range invalidation now reports failures through HAL and core, and an
injected D3D11 OOM reached the map callback after releasing the shadow. Flush
and unmap still use infallible HAL methods; device-lost callback reentrancy
during mapping remains unaudited. Metal compilation and BLAS error execution
were not available on the Windows test host.

Resume by checking `git status`, `git log` and the latest progress entry.
Keep Cargo jobs bounded and do not overlap heavyweight builds. Prioritize a
sound presentation synchronization design and core/HAL contract coverage while
continuing verified FL9_1 increments.
Point lists with supported vertex and instance streams are supported privately,
including Uint16 and Uint32 indexed draws translated to ordered nonindexed runs.
Strict GPU readback covers sparse streams, transitions, rebased 65,537-point
batches, GPU-updated indices, signed bases, instances and whole-draw discard.
Indexed point replay maps GPU-authoritative indices and can stall or issue many
draws. It avoids mapping when only zero-stride or instance-rate inputs exist;
this depends on the current shader builtin and side-effect restrictions.
Full-channel separate-alpha diagnostics require an explicit test-only opt-in
and are not evidence of historical-driver support.
Private Uint32 conversion uses ordered effective-index windows spanning at most
65534 and explicitly rejects selected vertex-index shader inputs. It rebases
the vertex buffer, gathers wider triangles with GPU copies, allocates scratch at
replay, and may stall for index staging. Historical-driver high-offset and
buffer-copy behavior remains unverified.
Restricted direct and indexed instancing repeat ordered native draws for
shaders without instance-dependent inputs. Nonzero first instance is
unobservable in this subset; instance-index shaders remain unsupported.
DXGI may synchronously call a window procedure during presentation. A private
application can avoid the deadlock only by keeping its UI callback nonblocking
and queueing graphics work; that cooperation cannot be assumed for arbitrary
public callers. A worker or recursive mutex alone does not establish safety.
The documented FL9_1 minimum D3D9 caps do not establish depth write-mask support
on every historical device; the existing no-write path is GPU-tested on the
RTX but its historical portability remains unverified. A zero-write depth state
also does not make simultaneous depth sampling safe without a read-only DSV.
Only begin higher feature levels after the requested FL9_1 contract is met;
retain the shared capability-driven architecture when doing so.

Transient color and D24S8 attachments use ordinary D3D allocations; memoryless
savings are not claimed. Private GPU rendering and core texture/view creation
are tested; a core transient render pass and historical hardware remain unverified.

The private FL9_1 path now accepts identical front/back stencil states and
retains/rebinds the reference. Asymmetric active state is rejected because
Microsoft's 10Level9 restrictions exclude two-sided stencil at 9_1. Strict RTX
readback indirectly verifies Replace, Equal, changed references and a Never
depth-fail stencil update; direct stencil readback and historical-device
validation remain unavailable.

Snorm16x2/x4 vertex inputs passed strict private FL9_1 readback, and Snorm16x2
UV passed the controlled core fixture. Four-byte private vertex alignment and
other unsupported formats remain restrictions; SNORM texture support is not implied.

Zero-stride vertex streams work through direct, strip and indexed windows/gathers
in the tested subset. Negative effective indexed vertices with only zero-stride
streams remain a specification caveat; current supported triangles are degenerate.

Restricted instance-rate streams are emulated by rebinding a zero-stride native
stream for each repeated draw. Core and strict GPU fixtures pass; indexed draws
with only instance-rate streams and shader instance_index remain unsupported.

Short uniform ranges are staged with exact GPU source bytes and zero-padded
native buffers. The unsafe core fixture sets BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED
and passes 4/5/12/20-byte rendering; no public flag is currently exposed.
Earlier RTX integer-uniform results are superseded. FXC's embedded legacy shader
can round a runtime integer comparison against 16777217 to 16777216, so selected
FL9 shaders now reject runtime integer values and operations, integer-bearing
uniforms and interfaces, and unproven initializers. Three affected tests assert
linkage rejection and still render float-only controls; six runtime-index GPU
renders are skipped after their expected rejection. General integer semantics
and historical-driver behavior remain unresolved.

Equal min/mag, anisotropy-1 samplers can normalize finite LOD clamps on the
one-mip path; pair validation checks the actual sampled view mip count.

Current FL9_1 shader profiles accept offset-free finite literal
textureSampleBias in [-3, 3] with equal min/mag filtering and anisotropy 1 for
every used pair in the selected entry. Core and HAL propagate literal and
unary-negated literal arguments through nested helper calls, with independent
validation at each call site. Dynamic, out-of-range, offset and otherwise
unproven bias expressions remain rejected. Wider bias support needs
semantics-preserving lowering, and older drivers remain unverified.

POT RGBA8/BGRA8 mip chains support initialization, transfers and sampled mip
views. Native suffix views include the smallest resource mip; non-suffix views
use a GPU clone refreshed before each draw. Strict private and controlled core
tests verify view-relative sampling and updates. A single-mip dual-use view
can render to the original and sample through a refreshed clone in later passes.
The same texture can be sampled through a disjoint native or clone-backed view
while another mip is attached. Clone refresh temporarily unbinds and restores
the active attachments. Overlapping ranges remain rejected.

Sampled mip chains also accept COPY_DST|RESOURCE without COPY_SRC. They use
shader-resource-only native textures; copy-source use and direct readback stay
unavailable. Strict GPU tests sampled uploaded and untouched levels, and core
verified 64 blue and 64 zero pixels in separate cases.

Sampler LOD policy is validated against actual selected texture/sampler pairs
and mip counts; finite max normalization and anisotropy remain restricted.

## Current stencil checkpoint

The private FL9_1 path accepts identical active front/back stencil states,
retains and rebinds the dynamic reference, and rejects asymmetric active state.
The strict RTX test indirectly verifies Replace/Equal, changed references and
an active-stencil Never-depth emulation through color readback. The full strict
HAL suite passed 287 tests, the core zero-adapter test passed, and Clippy passed
with ten existing warnings. A DX12 HAL regression build/check hit Windows paging
file error 1455 while mapping Rust crate metadata; it remains unverified for
this increment.

The one-sided stencil subset now also runs through the unsafe, feature-gated
core fixture. `ActiveStencil` records a symmetric Always/Replace pipeline with
color writes masked, then switches to an Equal pipeline and renders. On the
forced-FL9_1 RTX 3090 Ti, all 64 readback pixels matched. The full fixture-
enabled core library suite passed 70/70 tests, including ordinary adapter
isolation; the focused core hardware fixture also passed in strict mode. The
fixture continues to assert that ordinary HAL enumeration is empty. This is
controlled core/HAL coverage, not public adapter exposure or historical-device
validation.

Core fixture checkpoint: `cff2cc29b` (`core: test FL9_1 stencil through
controlled fixture`). The HAL now also classifies effective native stencil
writes and rejects depth- or stencil-writing pipelines on a read-only D24S8
attachment. This is whole-attachment read-only only: both aspects must use
LOAD|STORE, and sampled aliasing is not enabled. Strict RTX read-only testing
preserved 32 green/32 blue pixels and rejected both write cases. The complete
strict private suite passed 287 tests with two filtered; the fixture-enabled
core suite passed all 70 tests, including its zero-adapter regression. Clippy
passed with ten existing warnings. Windows paging-file error 1455 prevented a
DX12 regression check; historical FL9 hardware and public D3D11 exposure remain
unverified.

Follow-up commit `cdd06e5ca` asserts the live public format report is
`RENDER_ATTACHMENT | TRANSIENT_ATTACHMENT` with `SINGLE_MIP_ONLY`, and checks
core rejects both aspect-specific D24S8 views while accepting the combined
view. The strict focused test and complete public suite passed again (1/1 and
20/20); see `FL9_1_PROGRESS.md`.

The read-only regression now also rejects a depth-Never pipeline whose requested
stencil depth-fail Replace is remapped to a native pass write. The focused strict
GPU test passed with the preserved 32 green/32 blue result. This validates the
guard against the effective native operations, not only the input descriptor.

The render-pass audit confirmed the fixture's reported capabilities prevent
core from recording the HAL's unsupported query, resolve, multisample and
multiview commands. Core rejects occlusion/timestamp query creation and now
has a 4x RGBA8 negative case; the D3D11 HAL independently rejects unsupported
render-pass descriptors. Fixture-enabled core tests passed 70/70 in strict
hardware mode, and core Clippy passed without warnings. Ordinary D3D11 adapter
enumeration remains empty.

Latest production checkpoint: `5ea3045f9` (read-only stencil guard); latest
core validation checkpoint: `8d8877e7a` (reject FL9_1 multisample textures).
The remapped stencil-write regression is checkpointed as `f2e746d83`. The core
fixture now also records a colorless D24S8 prepass, then LOADs depth in a color
pass; strict FL9_1 readback matched 32 blue and 32 green pixels. The complete
fixture-enabled core suite passed 70/70 and core Clippy passed without
warnings. Latest checkpoint: `a13699edd` (`core: test FL9_1 depth-only render
prepass`). Continue the device-loss/render-pass audit. Keep public adapter and
HWND surface exposure disabled until capability truthfulness and presentation
reentrancy are resolved.

The latest verified increment centralizes DXGI HRESULT classification and
recognizes `DXGI_ERROR_DRIVER_INTERNAL_ERROR` as device loss in queue, device,
surface and optional pipeline-variant paths. CPU error-mapping tests passed;
the strict private HAL suite passed 289 tests and the strict fixture-enabled
core suite passed 70 tests on the RTX 3090 Ti forced to FL9_1. This does not
simulate actual device removal. The follow-up audit must cover teardown and
failure recovery while preserving the existing public adapter and presentation
gates. Checkpoint: `da035664154d080ecb80a3518cbfa6e4eecbe4d8`.

The current uniform-limit follow-up replaces a sparse vertex constant probe
with a dynamically indexed 255-element `float4` array. FXC accepts 255 and
rejects 256/257 at constant index 256; 32 pixel constants compile and 33 fail
at register 32. The forced-FL9_1 RTX accepted native shader-object creation for
the 255-element vertex profile. This shader was not drawn because private WGSL
rejects runtime integer indexing. The near-limit execution gate remains open.
The focused strict `constant` filter passed 8 tests (284 filtered). The complete
strict HAL suite passed 290 tests with two filtered. The targeted HAL build and
Clippy passed; Clippy retained ten existing warnings. Core's zero-D3D11-adapter
test passed 1/1. Formatting and `git diff --check` passed. Commit this bounded
finding, then continue the loss/teardown and public-contract audit without
enabling ordinary adapter or HWND surface exposure.
Checkpoint: `f775292a2` (`dx11: verify FL9_1 vertex constant limits`).

## Latest checkpoint — line topologies through the controlled core fixture

`e79428c50` adds LineList and LineStrip to the unsafe, feature-gated core
fixture. Core creates the actual pipeline and records nonindexed draw commands;
the injected HAL device remains unavailable through ordinary enumeration. The
strict RTX 3090 Ti FL9_1 readback contained 8 sampled red pixels for LineList
and 12 for LineStrip, with the rest of the target remaining clear blue. The
fixture-enabled core library suite passed 70/70 tests and core Clippy passed.
The previous private HAL line-topology implementation remains checkpointed as
`3db26eed3` and had passed the strict private suite and targeted hardware tests.
The fixture change was independently reviewed, with no isolation or correctness
issue identified. Indexed LineList was subsequently implemented for Uint16/Uint32
and verified through both private HAL and the controlled core fixture. Indexed
strip support is recorded in the later checkpoints below.

## Previous checkpoint — non-indexed line topologies

`3db26eed3` adds private non-indexed LineList and LineStrip pipelines and draw
replay. Line-list batches preserve vertex pairs; line-strip batches overlap one
endpoint. Indexed line topologies remain rejected. Strict FL9_1 readback on the
RTX 3090 Ti verified 22 red and 12 green pixels, and an indexed line-list
command was rejected before submission. The full strict private HAL suite
passed 294 tests with two filtered; the targeted core zero-adapter isolation
test, HAL build and Clippy passed. The working tree is clean after this
checkpoint. Next validate line topology through the unsafe controlled core
fixture, then continue toward indexed semantics and the broader public contract.

## Previous checkpoint — indexed FL9_1 LineList

Indexed Uint16 and Uint32 LineList draws preserve pairs through compact
rebasing, gather wide pairs, ignore an incomplete tail and discard invalid
fetches before issuing native draws. The RTX 3090 Ti at requested/actual FL9_1
read back 22 red, 12 green, 24 blue and 14 white pixels. The strict private
suite passed 297 tests with two filtered; the strict core fixture suite passed
70/70 and its focused run confirmed eight red pixels for both index formats.
HAL build and Clippy passed (ten existing HAL warnings), core Clippy passed,
formatting and diff checks passed. Ordinary adapter enumeration remains empty.
Checkpoint `82c9b9228` (`dx11: add FL9_1 indexed line-list rendering`).

## Latest checkpoint — indexed FL9_1 LineStrip

`2ff5a58e8` emulates fixed-index LineStrip restart in the private HAL. It
resolves the signed base vertex before restart matching, splits runs, compacts
bounded windows, preserves connectivity across windows and gathers wide edges.
The strict RTX 3090 Ti tests requested and selected FL9_1. They verified both
index formats, marker creation/removal through base adjustment, format mismatch
rejection, invalid-fetch discard, and a checked pixel on a gathered wide edge.
The focused strict hardware test passed 1/1; the complete strict private suite
passed 301/301 with two filtered. The controlled core fixture passed 70/70,
including indexed Uint16/Uint32 LineList and LineStrip while ordinary adapter
enumeration remained empty. HAL build passed, HAL Clippy completed with ten
pre-existing warnings, core Clippy passed, targeted DX12 tests passed 2/2, and
format/diff checks passed. Historical FL9_1 hardware is unverified. Public
adapter exposure and HWND surface exposure remain open.

## Current implementation — indexed FL9_1 TriangleStrip

Indexed Uint16/Uint32 TriangleStrip restart is emulated at command replay.
Resolved indices are split into runs, validated against every vertex-rate
stream, expanded with alternating winding, and sent through compact native
triangle-list windows or wide-triangle gathers. Strict readback compared both
formats against equivalent triangle lists for every front-face/cull pairing;
the wide gather rendered 32 checked red pixels. The controlled core fixture
rendered both formats with 12 red pixels each while normal adapter enumeration
remained empty. The complete strict private suite passed 304 tests, the core
fixture suite passed 70, HAL build and Clippy passed (ten existing Clippy
warnings), core Clippy passed, and DX12 pipeline descriptor tests passed 2/2.
See `FL9_1_PROGRESS.md` for commands and format-check caveats. The feature
is checkpointed as `f1b8c09a7` (`dx11: emulate FL9_1 indexed triangle strips`).
Broader instancing, indirect draws, public adapter exposure and public HWND
surface exposure remain open.

## Latest checkpoint — required R8_UNORM sampled uploads

`268431ec3` adds single-mip R8_UNORM sampling and buffer-to-texture upload,
gated by actual device format support and an explicit FL9_1 capability model.
Strict execution on the RTX 3090 Ti (vendor/device `0x10de:0x2203`, driver
`32.0.16.2101`) requested and selected FL9_1, uploaded from byte offset 1 with
7-byte rows, and matched 80/80 readback pixels. The complete private HAL suite
passed 307 tests; core zero-adapter isolation and targeted DX12 tests passed.
R8 render targets, copy-source/readback, mip chains and linear-filter visual
validation remain unsupported or unverified. Full evidence and commands are
in `FL9_1_PROGRESS.md`.

The R8G8B8A8_SNORM follow-up noted below fulfilled the next-format task from
this checkpoint. Ordinary adapter enumeration remains disabled. Continue
auditing public-core/HAL contract gaps and presentation reentrancy; do not infer
historical-device support from the modern forced-level GPU.

`881640796` completes the required R8G8_SNORM sampled/upload increment. Strict
execution reported support mask `0x00021320`, uploaded padded signed data from
a nonzero source offset and matched 80/80 pixels within one UNORM code value.
The full strict private suite passed 309 tests (two filtered); core isolation,
DX12 regression, build, lint and formatting checks passed. See the latest
progress entry for the initial midpoint-rounding observation and its bounded
test tolerance.

## Latest checkpoint — required R8G8B8A8_SNORM sampled uploads

`312dba141` adds a queried single-mip R8G8B8A8_SNORM sampled/upload path.
Strict execution on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
requested and selected FL9_1, observed support mask `0x000213a0`, uploaded a
padded signed texture from a nonzero offset and matched all 80 rendered pixels
within one UNORM code value. The full strict private suite passed 311 tests
(two filtered); core zero-adapter isolation, DX12 regression, build, Clippy and
formatting checks passed. See FL9_1_PROGRESS.md for exact commands.

## Latest checkpoint — FL9_1 BC1 sampled uploads

`6c67e22be` adds queried BC1_UNORM sampled/upload textures, restricted to a
single mip/layer and 4x4-aligned dimensions. Block-row layout supports padded
pitch, source offsets and aligned partial updates. Copy-source, render-target,
readback, sRGB and mip chains remain rejected. The RTX 3090 Ti at requested and
actual FL9_1 reported support mask `0x000213a0`; strict sampled readback
matched 256/256 pixels. The complete strict private suite passed 315 tests
before a readability-only validation refactor, and the focused BC1 tests passed
3/3 after it. The HAL build, core zero-adapter test, DX12 pipeline tests,
Clippy (ten pre-existing warnings), formatting and diff checks passed.

`1e59db97a` extends the controlled core fixture through BC1 texture creation,
queue upload, binding, sampling and mapped RGBA8 readback; the tested red/green
regions matched all 64 pixels. The fixture reports only BC1 `SAMPLED | COPY_DST`
and rejects all other BC formats at both HAL capability query and core texture
creation. The strict fixture-enabled core library suite passed 70/70.

`6b86270ec` changes the main fixture device creation to
`Global::adapter_request_device`, which calls the fixture-gated HAL
`Adapter::open`. The strict forced-FL9_1 core render fixture passed, including
BC1. The regular HAL build also passed with the test-fixture feature disabled.
The test still injects hand-chosen limits/features and therefore does not
validate a production adapter capability report. Next audit and model that
core/HAL boundary without enabling ordinary adapter enumeration or surface
exposure.

## Current core validation fixes

`96eacac49c26596a021672c5d405bb99fdbc56b1` adds a core-visible
`TWO_SIDED_STENCIL` downlevel flag. The FL9_1 controlled fixture omits the flag
and confirms differing front/back stencil descriptors fail before HAL. GLES
and Vulkan report their existing two-sided stencil support.

`3c9a7fca156b45aad8fcfb4c99db39d36f5f8bed` implements the existing
`COMPARISON_SAMPLERS` downlevel check in core. On the forced-FL9_1 fixture, a
comparison sampler now returns the missing-flag validation error before the
private HAL rejects it. The full fixture-enabled core suite passed 70/70; the
GPU fixture ran on the RTX 3090 Ti at requested/actual FL9_1. The core
zero-adapter test passed 1/1 and core Clippy and formatting passed. Public
enumeration remains empty. Continue the core/HAL contract audit; next check
blend, sampler/layout and resource restrictions that lack a sufficiently
narrow core validation path.

`73ef1a6ada7a4db1fa8515de35745a7b7abd8720` adds a
`SEPARATE_ALPHA_BLEND` downlevel flag and requires it when a pipeline writes
both RGB and alpha using different blend equations. The forced-FL9_1 core
fixture verifies full-channel straight-alpha blending is rejected before HAL
and RGB-only masked blending remains accepted. Strict fixture-enabled core
tests passed 70/70 on the RTX 3090 Ti at requested/actual FL9_1; wgpu-types
tests passed 55/55, combined backend HAL check passed, core Clippy passed,
zero-adapter isolation passed 1/1 and DX12 pipeline tests passed 2/2. Continue
auditing public core/HAL contract mismatches and preserve the closed public
adapter gate.

## Current checkpoint — `58fbd32b4`

Core now rejects storage-buffer layouts when the corresponding vertex,
fragment or compute downlevel flag is missing. The private HAL accepts core
storage-use bits only when another supported buffer usage is present and does
not implement shader storage binding. Strict RTX 3090 Ti FL9_1 tests verified
the stage-specific rejections and a 64-byte storage/copy buffer upload, GPU
copy, fence and readback. Full fixture-enabled core tests passed 70/70, and the
strict private D3D11 suite passed 315/315. Exact checks and the known formatting
and Clippy warnings are recorded in `FL9_1_PROGRESS.md`.

Resume the core/HAL contract audit at render passes, texture/view usages and
resource lifetime/error paths. Keep both public D3D11 enumeration and public
surface exposure closed; this test fixture still injects limits and is not a
production adapter capability report.

## Latest checkpoint — texture-view dimension validation — `c844eab6e`

Core now gates `D2Array`, `Cube` and `CubeArray` sampled layouts and texture
views with explicit downlevel flags. This prevents the private D3D11 HAL's
unsupported view shapes from reaching HAL, including a one-layer `D2Array`
view that `max_texture_array_layers = 1` alone cannot reject. The RTX 3090 Ti
strict FL9_1 fixture observed the expected core errors; the full fixture core
suite passed 70/70, the strict private HAL suite passed 315/315, wgpu-types
passed 55/55, combined backend HAL checks, core/HAL Clippy, zero-adapter
isolation and a DX12 regression passed. Exact commands and existing warning
details are in `FL9_1_PROGRESS.md`.

Resume at texture descriptor and render-pass validation. Public adapter
enumeration and public surface exposure remain disabled; the controlled core
fixture still injects capabilities and is not production adapter reporting.

## Latest checkpoint — texture layout dimension validation — `8c072d4f7`

Core now rejects D1/D3 texture binding layouts when the requested device limit
for that dimension is zero, and storage-texture D2Array layouts require the
same `TEXTURE_2D_ARRAYS` flag as sampled array layouts. The strict forced-FL9_1
core fixture verified these validation errors on the RTX 3090 Ti and completed
its GPU readback cases. The fixture-enabled core suite passed 70/70; the core
DX12 suite passed 67/67; Clippy and targeted formatting checks passed. The
DX12-only suite had fifteen existing no-WGSL feature warnings. Exact commands
are recorded in `FL9_1_PROGRESS.md`.

Continue auditing texture descriptors and render-pass validation. The D3D11
fixture still injects restricted capabilities; normal adapter enumeration and
public surface exposure remain disabled.

## Latest checkpoint — single-sample output masks — `8eff9020a`

The private D3D11 pipeline now forwards `MultisampleState::mask` to
`OMSetBlendState` while still rejecting multisampling and alpha-to-coverage.
At FL9_1, a zero WebGPU mask is translated to `0xffff_fffe`, which masks the
only supported sample while meeting the nonzero native argument requirement.
The controlled core fixture rendered with a zero mask and verified that all
64 pixels remained the blue clear color.

The strict private suite passed 318 tests; the fixture-enabled core suite passed
70 and includes the zero-adapter isolation regression; the core DX12 suite
passed 67. The focused strict fixture and mask translation tests each passed
1/1. HAL build/check and Clippy passed (ten existing HAL Clippy warnings); core
Clippy, targeted formatting and `git diff --check` passed. Hardware was an RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1.
See `FL9_1_PROGRESS.md` for exact commands. Next continue the core/HAL audit at
texture descriptor and render-pass validation. The fixture's injected
capabilities do not validate a production adapter, and historical FL9_1
hardware is untested.

## Core texture capability checkpoint — `1eb21620c`

Core texture creation now requires `DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`
for a multi-mip resource with any non-power-of-two spatial dimension. D2 array
layer counts are excluded. This closes a descriptor path that reached the
private D3D11 HAL even though its mip implementation is POT-only. A strict
controlled FL9_1 core fixture verified a 6×8 RGBA8/two-mip descriptor fails
with the specific missing-flag validation error before HAL. The CPU dimension
test, full fixture-enabled core suite (71), core DX12 suite (68), explicit
zero-adapter regression (1), core Clippy, targeted formatting and diff check
passed. The core DX12 test build emitted existing warnings with WGSL disabled.
The HAL was unchanged; its last strict private suite passed 318 tests under
`8eff9020a`. See `FL9_1_PROGRESS.md` for exact commands. Continue auditing
custom blend-state validation and public adapter gaps; keep both public gates
closed.

## Checkpoint — general single-target blending — `f99cae113`

The HAL now translates standard single-target wgpu blend factors and all five
blend operations, including blend constants recorded and rebound in command
order. It rejects `SrcAlphaSaturated`, dual-source factors and differing
full-channel RGB/alpha equations unless separate-alpha support is established;
the production FL9_1 model still does not establish it. The strict D3D11 HAL
suite passed 321/321 on an RTX 3090 Ti at requested and actual FL9_1. The new
GPU case checked constant-factor changes and Add/Subtract/ReverseSubtract/Min/
Max, matching all 64 pixels. The controlled core suite passed 71/71 with a
64-pixel custom blend render, the core DX12 suite passed 68/68, and the ordinary
zero-adapter test passed. Clippy and targeted formatting/diff checks passed;
ten existing HAL Clippy warnings remain. See `FL9_1_PROGRESS.md` for commands,
driver identity and failure history.

## Latest checkpoint — native FL9_1 depth comparisons — `340147803`

All seven non-Never wgpu comparisons now map to their D3D11 depth-stencil
comparison functions. Never keeps its validated-draw and stencil-depth-fail
emulation. A strict RTX 3090 Ti test explicitly created an FL9_1 device and
verified both pass/fail samples for every comparison in 128 output pixels.
The full private HAL suite passed 322/322; the controlled core fixture passed
71/71 and the ordinary zero-adapter test passed. Ten pre-existing HAL Clippy
warnings remain. Exact commands and results are in `FL9_1_PROGRESS.md`.

Next continue the core/HAL render-state audit and build a truthful capability
snapshot without enabling ordinary adapter enumeration. Public adapter
enumeration and public surface exposure remain disabled. The HWND `Present`
reentrancy issue, historical FL9_1 hardware and production capability-reporting
gaps remain unresolved.

## Latest checkpoint — vertex-stage sampled-texture downlevel gate — `68cccdf11`

Core now validates statically used sampled/depth texture count for the selected vertex entry point against a separate downlevel limit before HAL pipeline creation. The controlled FL9_1 fixture reports zero and confirms the precise rejection; it then completes strict hardware rendering and readback. The RTX 3090 Ti was explicitly requested and selected at FL9_1. The fixture-enabled core suite passed 72/72, strict private D3D11 HAL suite 322/322, core DX12 suite 69/69, the sampled-texture analysis unit test 1/1, and the focused wgpu-types compliance test 1/1. Core and backend checks, Clippy, formatting and diff checks passed; ten existing HAL Clippy warnings remain. Exact commands and details are in `FL9_1_PROGRESS.md`.

Code commit: `68cccdf111f36553e36f8328ec80ec3195786c80`. Continue auditing the remaining core/HAL capability boundary. Production D3D11 adapter enumeration and public surface exposure stay disabled.

## Latest checkpoint — `SRC_ALPHA_SAT` validation — `e001cfcd0`

The core/HAL mismatch for source-alpha-saturated blending is closed through a
new `DownlevelLimits::src_alpha_saturated_blend` capability. Core rejects its
use in all four blend-factor positions on the strict FL9_1 fixture before HAL.
The capability participates in compliance reporting and appears in wgpu-info.
Static feature-level tests mark it absent through 9_3 and guaranteed from
10_0; backend implementation remains false for every feature level. This does
not add a higher-level execution path.

Verification: wgpu-types 58/58, fixture-enabled core 74/74 (strict physical
RTX 3090 Ti at requested/actual FL9_1), strict private D3D11 HAL 323/323, core
DX12 71/71, zero-adapter isolation 1/1, changed crate checks and targeted
Clippy/rustfmt/diff checks passed. HAL Clippy has ten existing warnings and
wgpu-types has fourteen existing unused-serde-import warnings. Full commands
and the one corrected test setup are in `FL9_1_PROGRESS.md`.

Continue the core/HAL audit. The next concrete gap is that core accepts vertex
attribute formats outside the private HAL's translation table; determine
whether to implement the FL9_1 formats or add narrow pre-HAL validation. Keep
normal D3D11 enumeration and public surface exposure closed. Historical FL9_1
hardware and safe HWND Present reentrancy remain unverified.

## Current checkpoint — per-format mip validation — `53484db82`

The working tree's latest code commit adds `SINGLE_MIP_ONLY` format metadata
from HAL through `TextureFormatFeatures` and core rejects oversized mip chains
before HAL. D3D11 reports its single-mip sampled/upload formats and D24S8.
Strict FL9_1 testing on the RTX 3090 Ti verified the metadata projection,
accepted one-mip creation, rejected POT two-mip descriptors before HAL, and
retained BC1 sampled readback. The complete strict private HAL suite passed
326/326, fixture-enabled core passed 74/74, zero-adapter isolation passed
1/1, DX12 core passed 71/71 and wgpu-types passed 59/59. Rust 1.93.1/MSVC
builds and Clippy passed with the recorded pre-existing warnings. Exact
commands and caveats are in `FL9_1_PROGRESS.md`.

Documentation for this checkpoint is pending its own docs commit. Resume by
checking `git status`; then continue the core/HAL format-usage and texture
descriptor audit. Public D3D11 enumeration and public surface exposure remain
closed; the mission is still partially complete.

## Latest checkpoint — reusable per-format capability projection

The fixture's duplicated per-format mapping is now in
`Capabilities::texture_format_capabilities`. It intersects queried device
support, actual feature-level shape checks and backend implementation flags.
The controlled core adapter calls this same internal projection; ordinary
D3D11 enumeration remains empty, and the helper is not a public capability
report. CPU tests exercise positive, unavailable and negative query cases.
Code checkpoint: `7b3b5be08` (`dx11: centralize FL9_1 format capability projection`).

Verification: four projection tests passed; strict controlled core tests passed
75/75 on the physical RTX 3090 Ti at requested/actual FL9_1; strict private
HAL tests passed 329/329 at forced FL9_1; core DX12 passed 72/72; HAL build and
Clippy passed (ten existing warnings). Exact commands are in
`FL9_1_PROGRESS.md`. Continue the core/HAL texture descriptor and render-pass
audit; public adapter and HWND surface exposure remain gated.

## Current checkpoint — conditional color-format mip projection

The capability fix sets `SINGLE_MIP_ONLY` for RGBA8/BGRA8 if the selected device
lacks the native MIP format bit or any required implemented mip path. The
controlled fixture asserts the projection against the actual query and
core-rejects multi-mip descriptors when the bit is set. Synthetic tests cover
missing MIP support; the RTX reports it for both formats at forced FL9_1. See
`FL9_1_PROGRESS.md` for current test results. Continue the core/HAL audit: core
permits nonzero depth bias with a selected fragment entry point that writes
`frag_depth`, while HAL rejects that pipeline. Gate it narrowly on selected-entry
analysis. Keep ordinary D3D11 enumeration disabled.

## Current checkpoint — selected-entry depth-bias validation

Code checkpoint `8f81fdc8f` adds `DownlevelLimits::depth_bias_with_fragment_depth`,
a selected-entry output query on the validated core interface, and a specific
pre-HAL pipeline rejection when the capability is absent. The controlled FL9_1
fixture checks biased depth output rejection, an unused depth-writing entry
being harmless when the color entry is selected, and zero-bias depth output
pipeline creation. Strict RTX FL9_1 fixture passed 75/75, private HAL passed
330/330, wgpu-types passed 61/61, core DX12 passed 72/72, and zero-adapter
isolation passed 1/1. Clippy, builds, formatting and diff checks passed with
the existing warnings recorded in `FL9_1_PROGRESS.md`. Continue with the
production adapter capability contract; do not enable normal D3D11 enumeration.

## Current checkpoint — vertex system-value input gates

Code commit `d85e51e7b` adds core selected-entry gates for vertex
`vertex_index` and `instance_index` inputs through two new
`DownlevelLimits` booleans. The controlled FL9_1 fixture rejects either before
HAL and permits a safe sibling entry. FXC X4555 confirms failure for generated
`SV_VertexID` entry interfaces at `vs_4_0_level_9_1`; `instance_index` FXC
acceptance remains unverified. Strict core and private HAL suites passed on
the physical RTX 3090 Ti at requested/actual FL9_1, with the counts and exact
commands in `FL9_1_PROGRESS.md`. Targeted cross-backend checks, Clippy,
formatting and DX12 regressions passed. The documentation checkpoint for this
change is still uncommitted. Public enumeration remains closed. Next continue
the core/HAL contract audit and build the testable capability snapshot.

Code commit `1bc5ba21f` rejects alpha-to-coverage when the multisample count is
one. This is required by WebGPU and avoids passing an invalid descriptor to the
single-sample-only D3D11 HAL. Its strict controlled core suite passed 78/78 on
the RTX 3090 Ti at requested/actual FL9_1; the DX12 core suite passed 75/75;
core Clippy, targeted rustfmt and `git diff --check` passed. Full details and
reproduction commands are in `FL9_1_PROGRESS.md`. Resume the audit at
core-valid pipeline/render state and the production capability projection.
Keep normal D3D11 adapter enumeration and HWND surface exposure disabled.

The current working core patch extends the existing `COMPUTE_SHADERS`
downlevel gate to compute-visible uniform, sampler and sampled-texture layout
entries. The strict fixture checks these classes before HAL; full core passed
78/78, DX12 passed 75/75, Clippy passed, and the physical FL9_1 render/readback
fixture still passed. Exact commands are in `FL9_1_PROGRESS.md`. Run final
format/diff checks, commit the code and journal, then continue the core/HAL
audit. The private HAL suite is unchanged and most recently passed 337/337.

The current working patch also gates comparison-sampler bind-group layouts on
`COMPARISON_SAMPLERS`, closing the layout-creation path that previously reached
the HAL even though comparison-sampler object creation was already rejected.
Focused strict fixture, full core 78/78, DX12 75/75, Clippy, rustfmt and diff
checks passed; details are in `FL9_1_PROGRESS.md`. Commit this code and journal,
then resume the audit at remaining bind-group layout shapes. Keep public D3D11
enumeration and HWND surface exposure disabled.

The current empty-visibility patch adds a `DownlevelLimits` capability and a
core BGL validation gate because the private HAL rejects empty visibility.
`wgpu-types` passed 66/66 and serde passed 67/67; strict core passed 78/78,
DX12 passed 75/75, the zero-adapter test passed 1/1, and the strict private HAL
suite passed 337/337 with 144 hardware identity reports on the RTX 3090 Ti at
forced FL9_1. Core Clippy, HAL/GLES compile, `wgpu-info`, `wgpu-test`, targeted
rustfmt and `git diff --check` passed. Whole-workspace rustfmt still reports
pre-existing differences in untouched Naga and core files. Full commands and
the new capability description are in `FL9_1_PROGRESS.md`. Keep public adapter
enumeration and HWND surface exposure disabled; next continue auditing core
validity against HAL layout/render support and complete the adapter capability
projection.

The current sampled-layout patch projects depth, signed-integer,
unsigned-integer and multisampled texture binding types as unavailable and
rejects them in core before HAL. It also rejects vertex-only sampled
texture/sampler layouts when the D3D11 vertex sample limit is zero, while
preserving fragment-visible layouts with extra vertex visibility. `wgpu-types`
passed 68/68; serde passed 70/70 including old-shape deserialization defaults;
strict core fixture passed 78/78 on forced RTX FL9_1; DX12 passed 75/75; strict
D3D11 projection passed 4/4; and the full strict HAL suite passed 337/337.
The HAL run logged 144 physical adapter identities and six explicit skips for
nested render checks whose unsupported dynamic-indexing shaders were rejected
before drawing; those skips are not counted as GPU work. HAL/GLES compilation,
wgpu-info and wgpu-test checks, core Clippy, targeted formatting and diff
checks passed. Full commands are in `FL9_1_PROGRESS.md`. Continue the core/HAL
audit. Public D3D11 enumeration and HWND surface exposure remain disabled.

## Current checkpoint — pipeline override constants

The private HAL resolves `ProgrammableStage.constants` through Naga's
`process_overrides` for the selected entry point before safety analysis,
binding discovery, HLSL and FXC. Both ordinary and viewport-remapped vertex
variants receive the same constants. The strict controlled core fixture
rendered explicit vertex and fragment values 0.75 as red 191 and WGSL defaults
0.25 as red 64 across 64 pixels; unknown overrides returned stage-specific
pipeline-constant validation errors. Strict HAL passed 337/337, controlled
core passed 78/78, DX12 passed 75/75 and zero-adapter isolation passed 1/1.
Hardware was the RTX 3090 Ti at requested and actual FL9_1. Exact commands and
the existing Visual Studio prompt banner are in `FL9_1_PROGRESS.md`.

Static uniform coverage already includes nested structures, asymmetric
matrices, stride-sensitive arrays, padding and bounded ranges. Runtime integer
indexing remains unavailable. Resume the core/HAL capability audit and the
production adapter projection. Public D3D11 enumeration and HWND surface
exposure remain disabled; the mission is not complete.

The latest working-tree audit makes the FL9_1 shader-module test fail closed
under `WGPU_DX11_REQUIRE_HARDWARE=1`, reports the selected GPU, asserts actual
FL9_1, and verifies runtime integer uniform indexing is rejected even when
shader runtime checks are unchecked. This is shader compilation/object
creation only, not a draw. Its focused strict test and HAL Clippy passed on the
RTX 3090 Ti. The code and progress entry are being checkpointed before further
work.

Next address the core/HAL sampler anisotropy mismatch: core currently assumes
the boolean `ANISOTROPIC_FILTERING` flag permits 16, but FL9_1 D3D11 permits at
most 2 and the private HAL rejects larger values. Add a numeric downlevel cap,
preserve old serialized defaults, project 2 for FL9_1, and test the core clamp
and native descriptor. Continue the capability and adapter contract audit;
ordinary enumeration and HWND surface exposure remain disabled.

## Checkpoint — numeric sampler-anisotropy capability

`DownlevelLimits::max_sampler_anisotropy` now carries the backend maximum
separately from `ANISOTROPIC_FILTERING`; core clamps descriptors to the cap.
FL9_1 projection reports 2. The strict core fixture created native samplers for
requests 2, 3 and 16 on the RTX 3090 Ti at requested/actual FL9_1. Full
`wgpu-types` suites passed 69/69 and 71/71 (serde), full strict core passed
80/80, strict private HAL passed 337/337, DX12 core passed 77/77, and
zero-adapter isolation passed. Cross-backend HAL tests compiled `dx11,gles,vulkan`;
wgpu-info/wgpu-test checks and affected Clippy also passed. Exact commands,
warning notes and the lack of visual anisotropy-quality evidence are recorded
in `FL9_1_PROGRESS.md`. Code commit: `0b38756c1`.

The checkpoint is ready for commit. Resume with the remaining core/HAL contract
audit and truthful production adapter projection. Do not enable ordinary
D3D11 enumeration. HWND `Present` reentrancy remains unresolved; no public
surface exposure until safe scheduling and synchronization are established.

Latest hardening: reachable FL9_1 `textureLoad` now fails closed regardless of
`ShaderRuntimeChecks`, while safe sibling entry points remain compilable. The
focused forced-FL9_1 shader test and the full strict private HAL suite (337/337)
passed on the RTX 3090 Ti. This verifies compiler rejection and prior suite
render/readback behavior, not texel-load execution. See the latest checkpoint
in `FL9_1_PROGRESS.md`. Continue the remaining core/HAL audit and truthful
non-enumerated adapter projection; ordinary enumeration and surface exposure
remain disabled.

Latest capability-audit fix: HAL mip creation now matches the format projection
for every nonempty subset of `RESOURCE`, `COLOR_TARGET`, `COPY_SRC` and
`COPY_DST`. The RTX 3090 Ti forced-FL9_1 sampled-mip test passed 74 cases with
no skips, including actual sample/readback of resource-only and
resource-plus-copy-source chains and staged readback of copy-source-only chains.
The controlled core fixture created all three previously mismatched usage
shapes. CPU tests cover all fifteen combinations and backend/query gates. See
the latest `FL9_1_PROGRESS.md` checkpoint. Continue with the remaining core/HAL
audit, production-shaped non-enumerated adapter validation and lifetime/device
loss review. Keep ordinary enumeration disabled; HWND Present reentrancy is
still unresolved.

Latest core-projection validation: a test-only, non-enumerated `ExposedAdapter`
uses the selected RTX 3090 Ti's unmodified FL9_1 capability projection to
request a core device and create every advertised RGBA8/BGRA8 usage subset,
plus projected sampled R8/RG and D24S8 views. Strict core fixture passed
81/81, DX12 passed 77/77 and zero-adapter isolation passed 1/1. This verifies
core request/resource compatibility for the exact projection; it does not
enable public adapter enumeration or surface exposure. Full commands and the
hardware identity are in `FL9_1_PROGRESS.md`. Continue the exhaustive
core/HAL operation audit, device lifetime/device-loss review, and truthful
production adapter report. HWND `Present` reentrancy remains unresolved.

Latest buffer-limit correction: FL9_1 now reports `max_buffer_size = u32::MAX -
COPY_BUFFER_ALIGNMENT` because core adds a sentinel byte to vertex buffers and
rounds allocations to four bytes before HAL creation. The old `u32::MAX` limit
could expand beyond D3D11's 32-bit `ByteWidth`. The synthetic projection tests
and the strict test-injected core adapter both assert the new cap. A logical
vertex-buffer request one byte above the cap is rejected by core as
`MaxBufferSize` before HAL. HAL projection tests passed 4/4 and strict core
fixture passed 81/81 on the RTX 3090 Ti at requested/actual FL9_1; exact
commands and lint notes are in `FL9_1_PROGRESS.md`. This narrows the test
projection but does not establish a production adapter contract. Continue the
full core/HAL audit and lifetime/device-loss review. Normal adapter enumeration
and surface capabilities remain disabled pending truthful production reporting
and safe Present scheduling.

The partial command replay lifetime fix and the exact-projection D1/D3, array,
cube and D24S8 aspect-view rejection cases were committed as
`e32a29732e672cc96e9e2b9dbff3474425e9ae66`.
Strict RTX 3090 Ti forced-FL9_1 HAL/core suites passed (337/337 and 81/81),
DX12 passed 77/77 and ordinary adapter isolation passed 1/1. If the event
query itself fails due to actual device removal, safe retention after an
unqueryable device has not been established.

The current uniform projection checkpoint exposes six uniform bindings per
shader stage, a conservative fit within thirteen aggregate private HAL slots.
The exact selected-device core projection rejects seven fragment uniforms
before HAL. A forced-FL9_1 core draw binds two independent sparse fragment
uniform buffers and verified all 64 sampled pixels with both values applied.
The latest strict core suite passed 81/81, HAL projection tests passed 4/4,
the full strict HAL suite passed 337/337, DX12 passed 77/77, ordinary
enumeration isolation passed 1/1, and Clippy/format/diff checks passed. There
were 148 selected-hardware reports in the HAL log and six explicit nested
shader-render skips after expected FXC rejection. See the latest progress
entry for commands and outputs.

The projection and two-fragment-uniform core case were committed as
`0d86bcdd240fe98f61cfc085f250edcacbd41255`. The controlled core fixture now
also exercises distinct vertex- and fragment-visible uniforms at sparse
bindings 17 and 23 through the selected device's unchanged uniform projection.
The vertex uniform changes geometry coverage and the fragment uniform changes
sampled color; strict forced-FL9_1 GPU readback validated all 64 pixels. The
complete core suite passed 81/81, the exact projection test passed 1/1, DX12
passed 77/77, ordinary adapter isolation passed 1/1, and Clippy/format checks
passed. See the latest progress entry for commands and rounding tolerance.

Next continue the exact core/HAL contract audit. Public adapter enumeration
and surface capabilities remain disabled; synchronous HWND `Present`
reentrancy and the broader public capability snapshot remain blockers.
The stage-specific uniform fixture was committed as `2ae141279`.

Commit `8d19d14c9` extends the controlled fixture to two simultaneous dynamic
uniforms through the selected-device core projection: logical binding 17 is
vertex-visible at base/dynamic offsets 256/256, and binding 23 is
fragment-visible at 256/512. Strict RTX 3090 Ti FL9_1 readback passed all 64
pixels. Core rejects three dynamic entries against the projected limit of two
at bind-group-layout creation. The full strict core suite passed 81/81,
DX12 passed 77/77, adapter isolation passed 1/1, and Clippy/format/diff checks
passed. The latest two-group core-to-HAL draw and complete verification results
are recorded in `FL9_1_PROGRESS.md`.

The production adapter remains absent, and the known synchronous HWND
`Present` reentrancy deadlock remains unresolved. Keep normal enumeration and
public surface capabilities disabled until those architectural and full
contract audit gates are closed.

## Current checkpoint — public offscreen adapter

Branch `uwp-dx11`; code checkpoint `6d077bdee` (`dx11: expose experimental FL9_1 offscreen adapters`). The opt-in D3D11 backend now enumerates physical DXGI adapters only when explicit FL9_1 device creation succeeds, rejects software adapters, and exposes the selected-device capability projection. Adapter info reports the DXGI driver version when available. This experimental adapter is offscreen-only: `surface_capabilities` returns `None` because synchronous HWND `Present` reentrancy remains unresolved. It is not a claim of complete public wgpu support.

The strict public wgpu integration test passed on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1. Sparse sampled bindings, queue texture upload, indexed geometry, D24S8 depth testing and full 8x8 readback produced 32 red and 32 green pixels. A second public device opened from the same adapter and verified a separate 64-pixel clear/readback. The shared underlying native D3D11 context was exercised sequentially, not concurrently.

Current verification: HAL build passed; strict private HAL 337/337; strict core 81/81; public wgpu integration 1/1; DX12 core 77/77; Clippy passed with only existing HAL/core test warnings; rustfmt and diff checks passed. Exact commands and details are in `FL9_1_PROGRESS.md`. Commit is `6d077bdee`.

The independent review found no confirmed core-valid route to the scanned unsupported compute, query, cache or acceleration-structure panic paths, but the projection contract audit is incomplete. The public test is not yet a full scene/application validation, no concurrent multiple-device test exists, and historical FL9_1 hardware is untested. Continue auditing all core-valid operations against HAL. Do not enable public surfaces until the cross-layer `Present` reentrancy issue has a safe design.

## Latest increment — public validation and multi-device execution

Commits `1e21ed312`, `e8f6fe5f2` and `4efecaf6e` extend the ordinary public integration path. It tests sampled resources at group 7 while reusing binding numbers 3 and 17 across groups; dynamic transform updates alter the full image; concurrent public queues on two opened devices each clear, copy and verify their independent targets. The strict RTX 3090 Ti FL9_1 run passed 1/1: first image 32 tinted-red/32 green, shifted image 16 blue/32 tinted-red/16 green, concurrent readbacks 64/64 red and 64/64 green.

The same public test uses error scopes and confirms core validation rejects D3 texture creation, 4x multisampled RGBA8, sampled R32Float and two-mip D24S8. Clippy, targeted rustfmt and diff checks passed. Exact command and hardware identity are in `FL9_1_PROGRESS.md`.

Continue the exact capability and core/HAL contract audit, then extend the public scene toward a larger representative 3D workload. The adapter remains experimental and offscreen-only. Public surface capability is unavailable until safe HWND Present scheduling resolves queue-lock and immediate-context reentrancy. Historical FL9_1 hardware remains untested.

## Latest continuation — public layout rejection coverage

The public strict test also now rejects vertex-visible storage-buffer and
fragment-visible `D2Array` sampled-texture layouts through validation error
scopes. Together with the earlier texture descriptor negatives, six
unsupported requests are verified to stop in public core validation. The
RTX 3090 Ti FL9_1 readback and concurrent two-device tests remain passing.
Exact command and output are in `FL9_1_PROGRESS.md`. The public integration
test is a bounded offscreen diagnostic and not a full core/HAL contract audit.
Code checkpoint: `c5580577e` (`dx11: verify public FL9_1 storage and array rejection`).

Next audit pipeline and render-pass combinations at the public boundary and
expand the public scene with an explicit depth-ordering case. Public surface
capabilities remain disabled because synchronous DXGI Present can reenter core
while queue and context locks are held; historical FL9_1 behavior remains
unverified.

The latest public-only increment also renders the same near/far textured
overlap in both draw orders and verifies all pixels select the nearer surface.
Strict forced-FL9_1 readback, public descriptor/layout rejection cases,
dynamic uniform updates and concurrent two-device queue work all pass. The
exact command and hardware evidence are recorded in `FL9_1_PROGRESS.md`.
Code checkpoint: `6390e4efe` (`dx11: verify public FL9_1 depth ordering`).

A separate strict public test now exercises the selected projection for
R8Unorm with an upload, non-filtering sample and complete RGBA8 readback. It
verified 32 pixels at each of two sampled channel values, and the adapter
reported `COPY_DST | TEXTURE_BINDING`, `SINGLE_MIP_ONLY`, and no filtering
flag. The exact test command and hardware identity are in
`FL9_1_PROGRESS.md`. Public HWND presentation and the exhaustive adapter
contract remain outstanding. Code checkpoint: `b965940ff` (`dx11: verify public FL9_1 R8 sampling`).

The main public render now initializes vertex input through `mapped_at_creation`
and unmaps before drawing; readback verifies the mapped bytes reached GPU
vertex fetch. The strict two-test public suite and wgpu Clippy pass on the RTX
at forced FL9_1. Exact commands and results are in `FL9_1_PROGRESS.md`.
Code checkpoint: `8be26c25f` (`dx11: verify public mapped vertex initialization`).

## Latest checkpoint — public sparse multi-texture sampling

Code commit `6280e36df` adds a sixth strict public offscreen test. On the RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested and actual FL9_1,
textures at logical bindings 7 and 42 were sampled through one sampler at 100.
The bind-group layout, WGSL declarations and bind-group entries used different
orders. GPU readback verified all 64 pixels as `[104, 128, 24, 255]`, with both
textures contributing. Public core rejected an incomplete group missing
binding 42. The complete strict public suite passed 6/6; Clippy, targeted
rustfmt and `git diff --check` passed. Exact commands are recorded in
`FL9_1_PROGRESS.md`.

The public adapter remains offscreen-only. Continue auditing the projected
core/HAL contract and expand representative public rendering; synchronous
HWND `Present` reentrancy is still unresolved and blocks surface capabilities.

Code commit `98c6879bd` adds public eight-texture boundary coverage: all eight
sparse-bound resources contribute to a verified image, and core rejects a
ninth fragment texture against the selected projection. Strict suite result
and hardware identity are recorded in `FL9_1_PROGRESS.md`. Surface
reentrancy, the full public contract audit and historical-device validation
remain open.

Code commit `118918d40` adds public transfer coverage for a nonzero source
buffer offset, padded upload rows, partial buffer-to-texture update, a
nonzero-origin texture copy between 8×8 and 6×6 resources, and padded
texture-to-buffer readback. All 36 output pixels matched, including unchanged
destination texels. The strict public suite now passes 8/8 on the forced-FL9_1
RTX. The full public contract audit, historical hardware and safe HWND
presentation remain outstanding.

Code commit `f191d4f97` adds public RGBA8 mip-chain upload and sampling at
forced FL9_1. The test verifies implicit derivatives select the distinct
level-1 color across 64 pixels and that core rejects a multi-mip R8 descriptor.
The full strict suite now passes 9/9; exact evidence is in
`FL9_1_PROGRESS.md`. Other formats, NPOT chains and historical FL9_1 drivers
remain separate validation questions.

Code commit `f62e819db` adds public upload/sample/readback for both projected
SNORM formats. Their signed-to-unorm conversion matched 64 pixels within ±1;
format reports correctly omit filtering and retain the single-mip flag. The
strict public suite passes 10/10. A read-only texture/view/copy audit found no
new undocumented contract gap; BC1 edge-partial upload limits remain the
already documented private-HAL restriction. Full core/HAL auditing and safe
HWND presentation remain open.

Code commit `aeb41c79e` adds public per-instance vertex-stream rendering with
`first_instance = 1`, validating two selected instance records and leaving the
first record absent. Public core also rejects `instance_index`. The strict
public suite now passes 11/11 on forced FL9_1; see `FL9_1_PROGRESS.md` for
pixel counts and the exact command. The public presentation and comprehensive
core/HAL audit are still unresolved.

The selected-entry bias capability checkpoint extends `DownlevelLimits` with
an absolute literal bias cap of 3 and explicit dynamic/offset support bits.
Core follows helper calls and rejects unsupported bias forms before HAL.
Strict public FL9_1 execution passed 11/11, the core fixture suite passed
82/82, the adapter selection test passed 1/1, and the type/serde, HAL,
wgpu-info, DX12 compile, Clippy, rustfmt and diff checks passed. Details and
exact commands are in the latest `FL9_1_PROGRESS.md` section. The code
checkpoint is `f6e4125fe` (`core: validate FL9_1 texture sample bias limits`).
Public HWND presentation and the remaining operation audit still block full
completion.

The next checkpoint `987f39cd3` adds a selected-entry fragment-derivative
limit. Core follows helper calls and rejects `dpdx`, `dpdy` and explicit
`textureSampleGrad` before HAL on FL9_1; FXC diagnostics confirm those forms
are unavailable in the selected profile. Strict forced-FL9_1 private HAL,
public wgpu and core fixture suites passed 338/338, 11/11 and 83/83 tests,
respectively; the types serde and DX12 core unit suites passed 73/73 and
79/79. Exact commands, warnings, and hardware identity are in
`FL9_1_PROGRESS.md`. The public API remains opt-in and offscreen-only.

The multiple-uniform constant-budget audit is resolved by code checkpoint
`76dd6656deec94d3832db6961a9516cdba08070e`; see the latest checkpoint below.
Continue auditing other core-accepted shader expressions and return to the
surface scheduler. The public surface gate remains the unresolved synchronous
`Present` reentrancy/serialization problem; do not enable HWND capabilities
until that cross-layer design is safe.

## Current checkpoint — aggregate FL9_1 uniform-vector budgets

`76dd6656deec94d3832db6961a9516cdba08070e` adds selected-entry uniform-vector
accounting and public FL9_1 budgets of 255 vertex / 32 fragment vectors. Core
rejects aggregate overages before HAL, counting complete declared uniform
layouts for used resources. Strict public rendering exercised 32 fragment
vectors across two sparse bindings, read back 64 expected pixels, and rejected
33 at pipeline creation. Strict suite results: public wgpu 12/12, private HAL
339/339, core fixture 84/84, and core DX12 80/80. Types serde passed 74/74
unit tests and 14 doc tests (21 ignored). Exact commands and compiler evidence
are in `FL9_1_PROGRESS.md`. Continue the public core/HAL contract audit; public
HWND presentation, historical FL9_1 drivers and remaining shader/resource gaps
remain unverified.

## Follow-up — D24S8 outputless fragment verification

The public test `public_dx11_outputless_fragment_depth_stencil_pass_at_fl91`
now exercises the outputless conditional-discard fragment with D24S8. Its
first pass writes depth and replaces stencil with 7 only for non-discarded
fragments. Later readback passes independently verify stencil and depth:
discarded pixels pass stencil-equal-zero and depth-less-than, while surviving
pixels fail both comparisons. All 128 pixels across the two output images
matched on the RTX 3090 Ti at requested/actual FL9_1. The full strict public
suite passed 38/38; one optional Float16x2 IA branch was explicitly skipped
for absent selected-device support. Code/test and journal commits are
`016f04844` and `5ae420357`; the D24S8 follow-up is ready for its own
verification checkpoint. Continue the broad core/HAL contract audit.

The latest code checkpoint `1dd629564` gates explicit `textureSampleLevel`
by selected entry point and reachable helper calls. FXC rejects literal and
dynamic exact LOD in both 9_1 and 9_3 pixel profiles; the public strict suite
still passes 12/12 on the forced-FL9_1 RTX and rejects explicit LOD before HAL
while accepting its implicit-sampling sibling. The full private HAL suite
passed 340/340 and the core fixture passed 85/85 during that checkpoint.

Code checkpoint `38b5a0077` gates selected-entry `textureDimensions` and
`textureNumLevels` use before HAL. Naga's current image-query HLSL lowering
fails FXC under the tested level-9 pixel profiles (X3548; direct float-output
control X4532), while SM5 controls compile. Strict public wgpu passed 12/12,
private HAL 341/341, core fixture 86/86, types serde 76/76 unit tests plus 14
docs (21 ignored), and core DX12 82/82. GPU cases used the RTX 3090 Ti at
requested/actual FL9_1 with software=false. Exact evidence is in
`FL9_1_PROGRESS.md` and `TESTING.md`.

## Current checkpoint — indexed lists without vertex-rate inputs — `d96650d72`

The private HAL now accepts indexed LineList and TriangleList draws whose
pipeline has only instance-rate inputs or no vertex inputs. It validates the
index range but replays the corresponding nonindexed primitive count because
FL9_1 shader validation excludes `vertex_index`. Indexed strips still require
vertex-rate inputs so restart segmentation is preserved. Strict HAL and public
wgpu readback cover Uint16/Uint32, nonzero first-index, negative base-vertex,
instance-only layouts and empty layouts. Full suites passed: HAL 342/342,
public wgpu 13/13, D3D11 core fixture 86/86, and DX12 core 82/82. All hardware
tests selected the RTX 3090 Ti at requested/actual FL9_1 with `software=false`;
none skipped for absent hardware. Exact commands and lint/format details are in
`FL9_1_PROGRESS.md`.

Continue the public core/HAL contract audit and the surface scheduler
investigation. The opt-in adapter remains offscreen-only because synchronous
HWND `Present` reentrancy is unresolved. Historical FL9_1 drivers and ARM32
UWP execution remain unverified.

## Current checkpoint — fragment depth output validation — `ed642d768`

Core's FL9_1 selected-entry validation now rejects a fragment shader that
writes `frag_depth` but omits color output at `@location(0)`, matching FXC's
X4530 failure before shader compilation. A structured color-plus-depth entry
is accepted; sibling entry points are evaluated independently. The public
strict test verifies rejection before HAL and acceptance of the supported
form. Strict forced-FL9_1 suites passed: private HAL 344/344, public wgpu
14/14, D3D11 core fixture 88/88, DX12 core 84/84; all hardware cases used the
RTX 3090 Ti at actual FL9_1. Types, checks and Clippy passed; nine existing
unrelated HAL warnings remain. Full commands and driver details are in
`FL9_1_PROGRESS.md`.

The preceding selected-entry `textureSampleBaseClampToEdge` gate is code
commit `4a5cde393`; its public/core/type updates are covered by the latest
verification. Continue auditing core-accepted shader operations and HAL
behavior, then resume the surface scheduler investigation. Public HWND
presentation remains gated by unresolved synchronous `Present` reentrancy;
the public adapter is still opt-in/offscreen-only. Historical FL9_1 drivers
and ARM32 UWP hardware remain unverified.

## Current checkpoint — fragment sample-mask output gate — `26321bf74`

The FL9_1 projection now disables `DownlevelLimits::fragment_sample_mask_output`.
WGSL `@builtin(sample_mask)` writes HLSL `SV_Coverage`, and FXC rejects it with
X4583 for `ps_4_0_level_9_1`. Core validates the selected fragment entry point
before HAL; a color-only sibling remains accepted. This is a tested compiler
and validation boundary, not a GPU draw test. The HAL test logs the forced
FL9_1 RTX 3090 Ti and exact FXC diagnostic. A possible single-sample discard
emulation remains unimplemented.

Verification passed: strict HAL 345/345, public D3D11 15/15, D3D11 core fixture
89/89, DX12 core 85/85, types 77/77 and serde types 79/79. HAL/GLES, `wgpu-info`
and `wgpu-test --tests` checks, all affected Clippy runs, rustfmt and diff checks
also passed. No strict hardware tests skipped. Nine existing unrelated HAL
Clippy warnings and fourteen non-serde types unused-import warnings remain.
See `FL9_1_PROGRESS.md` for exact commands and resolved build/test issues.

Continue the shader/core/HAL compatibility audit. Public HWND presentation is
still gated on a safe scheduling design for DXGI's synchronous reentrancy;
historical FL9_1 drivers and ARM32 UWP remain unverified.

## Current checkpoint — D16 depth and render-pass transitions — `e3807a1b3`

D16_UNORM is exposed as a single-mip depth-only attachment when the required
FL9_1 format guarantee, selected-device `CheckFormatSupport` query and backend
implementation all agree. The RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) returned format mask `0x210020` while explicitly forced to
FL9_1. The public strict test uses `DepthOnly`, verifies core rejects
`StencilOnly`, multi-mip D16 and stencil state, and confirms depth ordering in
both draw orders (128/128 pixels). Additional passes confirm LOAD preserves
depth, CLEAR resets it (128/128 pixels), and a discarded depth value followed
by LOAD cannot affect rendering (64/64 pixels). The full strict public suite
passed 19/19 on this physical adapter with no skips. The full private HAL,
core fixture, physical-adapter isolation and DX12 core suites passed 347/347,
89/89, 1/1 and 85/85 at code checkpoint `d0efe2753`; they were not rerun for
this test-only increment. Exact commands are in `FL9_1_PROGRESS.md` and
`TESTING.md`. Historical hardware remains unverified.

The mission remains active. Continue the shader/core/HAL and format audit, then
return to safe HWND presentation scheduling. The public adapter is
opt-in/offscreen-only; public surface capabilities remain unavailable.
Historical FL9_1 hardware and ARM32 UWP remain unverified.

## Current checkpoint — optional R16G16_SNORM query gate — `95a7d50e8`

R16G16_SNORM is not a required FL9_1 shader-sample format: Microsoft lists
`TEXTURE2D` at 9_1 and `SHADER_SAMPLE` from 9_2. The model records no static
FL9_1 sampling guarantee and requires a positive selected-device query for the
narrow single-mip sampled/upload path. The RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) forced to FL9_1 returned `0x00021122`, without `SHADER_SAMPLE`.
The private HAL rejects it, the public adapter reports no usages, and public
texture creation fails validation. Synthetic tests cover a positive query; the
conditional positive upload/sample path remains unverified on hardware.

Strict verification on this physical adapter: private HAL 349/349, public
D3D11 19/19, D3D11 core fixture 89/89, adapter enumeration 1/1, DX12 core
85/85. No strict hardware skips. HAL and public Clippy plus changed-file
rustfmt and `git diff --check` passed; nine existing unrelated HAL Clippy
warnings remain. Exact commands and output are recorded in
`FL9_1_PROGRESS.md`.

Resume from branch `uwp-dx11`, code checkpoint `95a7d50e8`, then progress-log
checkpoint commit. The tree currently has only the pending progress and
handoff journal edits. Next task: inspect and test D24S8 depth/stencil
render-pass load/store/discard transitions through public wgpu, then resume the
core/HAL contract audit. Do not enable public surface capabilities; HWND
presentation still needs a safe cross-layer scheduler because DXGI `Present`
can synchronously reenter the window procedure while core and HAL locks are
held. Historical FL9_1 and ARM32 UWP execution remain unverified.

## Current checkpoint — D24S8 depth/stencil transitions

Branch `uwp-dx11`, code commit `24bbe6bce`. The new strict public test verifies
D24S8 LOAD preservation of both aspects, independent depth/stencil CLEAR, and
per-aspect DISCARD followed by LOAD initialization through core. Strict public
D3D11 suite passed 20/20 on the RTX 3090 Ti at requested/actual FL9_1; the new
case checked 320 pixels across five outcomes. Sampled D24S8 is rejected by
core. The D16 public test separately rejects its unsupported stencil-only view.
See the latest `FL9_1_PROGRESS.md` section for exact commands and the observed
zero initialization used after depth discard.

Latest transfer contract checkpoint `0de479c09` fixes public-valid D3D11
copies between RGBA8/BGRA8 linear and sRGB formats. Strict forced-FL9_1 GPU
readback verified both format families copy to sRGB and back without changing
any stored bytes. Verification passed: private HAL 350/350, public wgpu
21/21, controlled core fixture 89/89 and DX12 core 85/85; HAL/public-test
Clippy and formatting/diff checks passed. Continue the public core/HAL audit;
HWND surface support remains gated on resolving `Present` reentrancy without
violating D3D11 context serialization. Exact commands and evidence are in
`FL9_1_PROGRESS.md`.

Worktree is expected clean after the forthcoming progress-journal commit.
Next continue the public core/HAL contract audit and evaluate remaining
depth/stencil aspect and pass combinations. Public HWND surface capabilities
remain disabled: synchronous DXGI `Present` may reenter core while its queue
and HAL context locks are held, and a safe cross-layer scheduling design has
not been established. Historical FL9_1 hardware and ARM32 UWP remain
unverified.

## Checkpoint — serialized public HWND presentation — `9ed1cc305`

The newest working-tree increment replaces the former surface gate with a
narrow public HWND contract: BGRA8 sRGB FIFO, opaque composition,
render-attachment usage and one frame of latency. Configure/acquire/present
are owner-thread-only. Core serializes submissions, queue writes, map flushes,
polling and selected resource operations with Present; same-thread reentrant
operations that could mutate queue state fail safely. The default backend
priority is unchanged, and the adapter remains opt-in and non-WebGPU-compliant.

Strict verification passed on the physical RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), requested/actual FL9_1, software=false: private HAL
351/351, controlled core fixture 90/90, public D3D11 22/22 with no skips, and
DX12 core 86/86. The public HWND test rendered, presented, resized from 64×64
to 80×48, presented again and rejected wrong-thread acquisition. It used a
hidden HWND; visible display output and a continuous message pump are not yet
verified. Clippy and changed-file formatting passed; existing unrelated
warnings and the Naga workspace-format discrepancy are recorded in
`FL9_1_PROGRESS.md`.

Code commit `9ed1cc305` (`dx11: add serialized public HWND presentation`)
contains the implementation and strict hidden-window evidence.

## Checkpoint — visible public HWND example — `ca0b026fd`

`wgpu/examples/dx11_fl91_windowed.rs` creates a visible window, pumps Win32
messages, renders an animated indexed textured depth-tested cube, and handles
resizing. A bounded 60-frame run passed on the physical RTX 3090 Ti at
requested/actual FL9_1 (`software=false`, driver `32.0.16.2101`) and verified
the HWND visibility state. The exact invocation and limits of this evidence
are in `FL9_1_PROGRESS.md`; no displayed-pixel capture was taken.

Commit `ca0b026fd` (`dx11: add visible FL9_1 windowed example`) contains the
example, documentation and 60-frame hardware verification record.

Commit `867122688978524082ada4ba8614a3c3f4f72c3b` maps optional half-float IA
formats while keeping them outside FL9_1 static guarantees. The forced-FL9_1
RTX reports `Float16x2` unsupported through the selected-device query; the
public test logs an explicit optional-path skip. Synthetic projection tests
cover a positive optional query. See the current checkpoint section in
`FL9_1_PROGRESS.md` for verification commands and results.

Next continue the public core/HAL contract audit and add focused tests for any
remaining surface or presentation reentrancy gaps. Historical FL9_1 and ARM32
UWP execution remain unverified.

The visible-window presentation checkpoint now has an inspected client-area
capture. A strict physical FL9_1 run completed 1,800 frames; a longer run
presented 80,871 frames before a clean `WM_CLOSE`. The reviewed frame showed
distinct red/green cube faces over the clear color. The PNG is temporary at
`%TEMP%\wgpu-dx11-fl91-windowed.png`, not tracked. Deterministic image checks
remain covered by offscreen GPU readback tests.

## Current checkpoint — complete direct-DXGI vertex input mapping (`a4e9bc360`)

Code commit: `a4e9bc360d3355081292c87c9ce6ab1800252d8e`.

The current uncommitted increment maps 41 direct DXGI IA formats, excluding
Float64, while preserving the ten-format FL9_1 guarantee set. Other mapped
formats enter the selected-device capability mask only after a positive
`IA_VERTEX_BUFFER` query. The forced-FL9_1 RTX reports no optional formats;
the public Float16x2 case logs a capability skip. Full test results and exact
commands are in `FL9_1_PROGRESS.md`. The implementation and documentation
changes are ready for review and a focused commit after final diff checks.

Next continue the public core/HAL contract audit and surface reentrancy edge
tests. Keep the opt-in adapter's limited contract explicit; historical FL9_1
drivers, ARM32 UWP, and GPU execution of optional IA formats remain
unverified. Do not push.

## Follow-up — core Present-guard reentrancy coverage

Commit `f6b6d93a6a1746882c5384e341d37b95f01bcc6a` adds core fixture assertions
that acquire the production queue presentation guard and exercise queue
write, queue submit, device poll, buffer creation and texture creation
reentrantly on the same thread. The targeted forced-FL9_1 fixture passed on
the RTX 3090 Ti at actual L9_1. It verifies guard responses, not DXGI callback
delivery during Present. Exact command and first-run resolution are in
`FL9_1_PROGRESS.md`. Core-test Clippy, formatting and diff checks passed. The
tree is clean after this focused checkpoint. Continue the public core/HAL
contract audit; this does not establish that every possible callback API is
safe or that historical drivers behave identically.

## Current checkpoint — query-backed packed 16-bit sampled formats

Code commit: `552a4d019` (`dx11: add query-backed FL9_1 packed texture formats`).

The selected-device format audit includes RGB565, RGB5A1 and RGBA4 single-mip
sampled uploads behind `TEXTURE_FORMAT_16BIT_PACKED`. Strict FL9_1 public
readback passed all three formats on the RTX 3090 Ti. The private HAL suite
passed 356/356, public D3D11 passed 24/24, core D3D11 fixtures passed 90/90
and DX12 core passed 86/86. GLES and Vulkan HAL feature checks passed. See
`FL9_1_PROGRESS.md` for exact commands and lint results. The implementation
and observed query masks are recorded in `CAPABILITIES.md`.

After this checkpoint, continue with the public core/HAL contract audit. The
modern RTX query results do not establish historical FL9_1 behavior or ARM32
Windows 10 Mobile compatibility.

The latest diagnostic-only audit queried R32_FLOAT, R32G32B32A32_FLOAT,
D32_FLOAT and D32_FLOAT_S8X24_UINT on that selected device. Neither R32 format
reported `TEXTURE2D`; D32 queries were unavailable. Do not expand formats from
those results. Exact query masks and the strict command are in
`FL9_1_PROGRESS.md`.

## Current checkpoint — query-backed R8/RG8 attachments

Code commit: `a6d958664` (`dx11: enable query-gated R8/RG8 color attachments`).

This commit adds single-mip R8_UNORM and R8G8_UNORM color
attachments when the selected device's `CheckFormatSupport` query positively
reports `RENDER_TARGET`. At this checkpoint, blending was not yet projected;
the later format-blending follow-up gates it on the separate `BLENDABLE` bit.
Both remain single-layer and single-sample; copy and readback are separately
query-gated. Strict public wgpu
readback on the RTX 3090 Ti forced to FL9_1 verified exactly 64 pixels of
`[64, 0, 0, 255]` for R8 and 64 pixels of `[64, 191, 0, 255]` for RG8 after
clear and sample. The full HAL suite passed 356/356, public D3D11 passed
25/25, core fixture passed 90/90, DX12 core passed 86/86, and selected
capability tests passed 28/28. Strict runs kept optional hardware skips
visible; details are in `FL9_1_PROGRESS.md`.

This is a modern-device observation, not proof for historical 9_1 hardware or
ARM32 UWP. Public D3D11 selection remains explicit opt-in. The next task is to
continue the core/HAL validation audit from the remaining render-pass,
resource-usage and shader-contract gaps in `FL9_1_PLAN.md`; do not expand
format exposure without a selected-device query, implementation path and
actual output test. No history was rewritten or pushed.

## Follow-up — HWND surface re-entry audit

Code commit: `be8e58729` (`dx11: test HWND surface operation reentrancy`).

Strict hidden-HWND tests now call `configure` and `acquire_texture`
reentrantly while the same surface operation
guard is held. Both reject with `D3D11 surface operation already in progress`,
and the normal present/resize/readback flow passes afterward. The targeted
strict suite passed 2/2 on the physical RTX 3090 Ti at requested/actual
FL9_1. HAL Clippy passed with nine previously recorded warnings and the changed
test file passed rustfmt. See the final follow-up section in
`FL9_1_PROGRESS.md` for the exact command.

An independent HAL scan found no confirmed valid-public-operation route to a
panic; disabled HAL hooks are protected by uninhabited resource types or core
capability gates. A separate render-pass/surface review found no confirmed
contract mismatch. It noted frame-latency observation as a possible test gap;
DXGI's getter returned `DXGI_ERROR_INVALID_CALL` for the non-waitable chain,
so no waitable-swapchain behavior was introduced. Microsoft's documented
default is one. Continue the broad API-contract audit and retain the explicit
unknowns for historical FL9_1 and ARM32.

This was the state at that checkpoint and was superseded on 2026-11-12. The
HWND surface now creates a waitable swap chain, explicitly sets maximum frame
latency to one, waits before acquisition, and drains pending presents during
queue idle and unconfigure. Strict hidden-HWND tests cover these behaviors;
CoreWindow runtime behavior and historical-driver behavior remain unverified.

## Follow-up — query-gated B5G6R5 attachment

Code commit: `4b013795f` (`dx11: enable query-gated RGB565 render attachments`).

Selected-query-gated B5G6R5 color-attachment usage is now exposed behind
`TEXTURE_FORMAT_16BIT_PACKED`. A strict public FL9_1 draw into the packed
target, followed by sampling and RGBA8 readback, passed on the RTX 3090 Ti;
all 64 pixels matched within one UNORM code. The selected query included
`RENDER_TARGET` and `BLENDABLE`; the public projection intentionally omits
blendability and copy-source use. The strict private HAL suite passed 357/357,
public D3D11 25/25, the controlled core fixture 1/1 and DX12 core 86/86.
HAL/core/public Clippy passed; existing warnings are recorded in
`FL9_1_PROGRESS.md`. Continue the broader core/HAL capability audit.
Historical FL9_1 and ARM32 behavior remain unverified.

## Latest verified checkpoint — outputless fragment in a depth-only pass

Commit `016f048447b45896757000f49d4e9fdc44484501` allows a fragment stage
without color outputs in a depth-only D16 pass. Naga's direct outputless HLSL
fails FL9_1 FXC with X4530, so the HAL clones the selected, override-processed
Naga module, moves its result-less entry-point body into a helper, adds a
wrapper returning zero `SV_Target0`, and regenerates validation metadata. No
RTV is bound, preserving discard and depth semantics while making the synthetic
color unobservable. The compiler test uses a location input, helper call and
sparse uniform mapping. The strict public RTX 3090 Ti test verified 32 red
pixels where discard prevented depth writes and 32 blue pixels where a farther
draw failed depth; all 64 pixels were read back.

Verification at requested/actual FL9_1 on NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false): private HAL 360/360,
public D3D11 37/37 (one optional Float16x2 IA branch explicitly skipped), core
FL9_1 adapter selection 1/1, DX12 core 86/86. HAL Clippy passed with nine
pre-existing warnings; public D3D11 Clippy, rustfmt and `git diff --check`
passed. Exact commands are in `FL9_1_PROGRESS.md` and `TESTING.md`.

Independent review found no concrete semantic or validation defect. An
optional untested case is a targetless fragment that returns only unmatched
nonzero color locations; there is no confirmed compiler issue. Next audit:
test the outputless fragment path with D24S8 stencil state, then resume the
broader core/HAL render-pass audit. Historical FL9_1 drivers and ARM32 UWP
remain unverified.

## Follow-up — outputless fragments with D24S8 stencil

Commit `967b1af89` (`dx11: verify outputless FL9_1 fragments with D24S8`)
adds a strict public stencil interaction test and updates capability/testing
notes. On the physical RTX 3090 Ti forced to FL9_1, outputless fragment
discard left stencil and depth unchanged on the left half; later stencil and
depth comparisons independently rendered red and green there while the
surviving half remained blue. Both complete 8×8 results matched. The full
strict public D3D11 suite passed 38/38; the optional Float16x2 IA branch was
explicitly skipped for lack of selected-device support. Rustfmt, public-test
Clippy and `git diff --check` passed. Continue the broader core/HAL
render-pass contract audit.

## Verified increment — query-gated FL9_1 cube sampling

The working tree now supports public six-face RGBA8 cube upload and sampling
through selected-device cube-format queries, with explicit size, shape, NPOT
and format validation in core. Forced-FL9_1 execution on the RTX 3090 Ti
checked all six faces and matched 96/96 pixels; RG8 cube creation was rejected
before HAL. The strict public suite passed 51/51 with one optional Float16x2
IA subcase logged as a skip. The private HAL suite passed 370/370, the core
fixture suite 96/96, and DX12 core 92/92. Cube copies and render-target cube
views remain unsupported. Continue the broader core/HAL audit; historical
FL9_1 hardware and ARM32 UWP remain unverified.

## Current increment — expose 255 vertex uniform vectors

The public selected-device `max_uniform_buffer_binding_size` is now 4,080
bytes, matching the FL9_1 vertex profile's 255 float4 constant budget. Core
still gates each selected stage separately: 255 vertex vectors and 32
fragment vectors. Strict public GPU readback on the RTX 3090 Ti at
requested/actual FL9_1 verified c254 geometry changes across two draws (17
green baseline pixels, 4 red shifted pixels), while core rejects 256 vertex
vectors and 33 fragment vectors before HAL. The public suite passed 46/46 and
the core fixture 94/94; the HAL projection unit test passed. Public Clippy is
clean; HAL Clippy retains nine existing warnings. See the follow-up entry in
`FL9_1_PROGRESS.md`. After final formatting/diff verification, commit this
increment and continue the broader core/HAL capability audit. Historical
FL9_1 devices and ARM32 remain unverified.

## Current increment — query-gated 4x RGBA8 resolve

The current working tree adds a selected-device-query-gated FL9_1 4x RGBA8
render-and-resolve path, leaving multisampled sampling/depth and other sample
counts unsupported. Strict readback on the RTX 3090 Ti at requested/actual
FL9_1 checked 64 triangle-coverage pixels and 64 blue clear pixels after a
zero-mask draw. HAL passed 364 tests, public D3D11 passed 45 and the core
fixture passed 94; the core DX12 regression suite passed 90. See the matching
follow-up entry in `FL9_1_PROGRESS.md` and the capability/test documentation.
After targeted Clippy, formatting and diff checks, checkpoint this increment
and continue the capability/core/HAL contract audit. Older FL9_1 devices and
ARM32 UWP remain unverified.

## Latest code checkpoint — query-gated narrow-format blending — `3774c7a2e`

The selected-device format projection now reports `COLOR_ATTACHMENT_BLEND`
for R8_UNORM, R8G8_UNORM and B5G6R5 only when both the implementation path and
the actual format query support it. The RTX 3090 Ti forced to FL9_1 reported
`BLENDABLE` for all three; strict readback checked 64 pixels per format after a
half-alpha red overlay. R8 and RG8 matched their expected bytes, while RGB565
matched within packed-format quantization tolerance. The private strict HAL
suite passed 363/363 and the public strict D3D11 suite passed 43/43, including
the indexed textured bundle retention test. The optional Float16x2 vertex
input subcase was explicitly skipped because the selected device did not
report its input-format support. Continue the core/HAL audit; old FL9_1
hardware and ARM32 UWP are still unverified.

## Follow-up — public render-bundle execution

The strict public test `public_dx11_render_bundle_executes_at_fl91` records a
nonindexed triangle in a render bundle, executes it through a public render
pass and validates all 64 output pixels. It passed on the RTX 3090 Ti at
requested/actual FL9_1; the full public suite passed 42/42 with one explicit
optional Float16x2 input-format skip. This verifies the basic pipeline,
vertex-buffer and draw bundle path only. Commit `d6b039960` contains the test
and its capability/testing notes. The next task remains the broader core/HAL
API audit; this checkpoint does not complete the mission.

A second strict public bundle test adds Uint16 indexed drawing and sampled
resources, then drops the source handles before command submission. At forced
FL9_1 on the same RTX, it produced the expected `[51, 128, 204, 255]` across
all 64 pixels, proving retained bundle resources for that shape. The strict
public suite passed 43/43 with one visible optional IA-format skip, and
public-test Clippy passed. Indexed restart and other bundle state combinations
remain unverified.

Commit `b5fb590ac` (`dx11: verify ignored targetless fragment outputs`)
verifies that a result-bearing fragment shader whose location 0 has no color
target is correctly ignored in a D16-only pipeline. The HAL's direct compiler
path already supports the shape. A later farther draw remained blue at all
64 pixels, proving the targetless pass updated depth while its color result
was unobservable. The combined targetless-fragment test and the full strict
public suite passed 1/1 and 38/38 respectively on the physical RTX 3090 Ti at
requested/actual FL9_1. The optional Float16x2 IA branch was explicitly
skipped. Public-test Clippy, rustfmt and `git diff --check` passed. Next add
selected-entry core gates for HAL-rejected FL9_1 integer semantics and matrix
aggregate stores.

## Follow-up — outputless fragments with D24S8 stencil

Commit `967b1af89` (`dx11: verify outputless FL9_1 fragments with D24S8`)
adds a strict public stencil interaction test and updates capability/testing
notes. On the physical RTX 3090 Ti forced to FL9_1, outputless fragment
discard left stencil and depth unchanged on the left half; later stencil and
depth comparisons independently rendered red and green there while the
surviving half remained blue. Both complete 8×8 results matched. The full
strict public D3D11 suite passed 38/38; the optional Float16x2 IA branch was
explicitly skipped for lack of selected-device support. Rustfmt, public-test
Clippy and `git diff --check` passed. Continue the broader core/HAL
render-pass contract audit.

## Latest increment — BC1 GPU copy source

BC1 now supports query-gated GPU texture-to-texture copies at FL9_1. A new
per-format `DownlevelLimits` mask keeps texture-to-buffer readback separate;
the strict core fixture copied a BC1 block region into another resource,
sampled the destination and validated 64/64 output pixels, then verified BC1
readback was rejected before HAL. Current strict counts: private HAL 376/376,
core fixture 100/100, public D3D11 60/60, adapter isolation 1/1 and DX12 core
92/92. The optional public Float16x2 vertex-input subcase was skipped by the
device query. Continue the remaining core/HAL capability audit; this does not
complete FL9_1 coverage, and older hardware plus ARM32 remain unverified.

The latest controlled cleanup test injects a synthetic device-removal result
from `wait_for_idle` during core queue destruction. Core reports one lost
callback, permits callback re-entry to `device_poll`, and finishes queue drop
without panic on the physical RTX opened at FL9_1. This is error-path coverage
only; physical removal and the broader lifetime audit remain open.

Latest follow-up: the public HWND Present path now has fixture-only synthetic
`DXGI_ERROR_DEVICE_REMOVED` coverage. The callback ran once and its reentrant
poll returned the expected presentation-guard timeout on the physical RTX
opened at requested/actual FL9_1. See the latest entry in
`FL9_1_PROGRESS.md`; physical device removal/recovery and the broader audit
remain outstanding. Implementation checkpoint: `cb2e06cecae0a2b6dab1622558deb3ef2195dbeb`.

Latest test-only follow-up extends 4x depth execution coverage to both
query-supported public formats. Forced FL9_1 on the RTX 3090 Ti queried and
render-tested D16 and D24S8, with 64/64 resolved pixels classified correctly
for each. The strict HAL suite passed 377/377, and the complete strict public
suite passed 63/63 with one explicitly skipped optional Float16x2 subcase.
The public test Clippy check passed; rustfmt reports only four older,
unrelated formatting differences. See the latest progress entry for commands
and the transient Windows paging-file issue at `-j16` (retry succeeded at
`-j2`). Next continue the audit of public-valid operations and their exact
core/HAL rejection or execution paths; the backend remains a constrained
opt-in implementation, not a general drop-in replacement.

The next strict public workload is a perspective-transformed indexed cube with
an uploaded RGBA8 face texture, WGSL transform uniform, nearest sampler and
D16 depth. The RTX 3090 Ti at requested/actual FL9_1 produced identical full
32×32 readbacks when the six face draw order was reversed: 154 red, 21 blue,
70 yellow and 779 clear pixels. The new test passed 1/1 and Clippy passed;
the complete public suite remains 63/63 from the immediately preceding
checkpoint, with one visible optional Float16x2 subcase skip. This supplies a
representative scene check but does not change the broader incomplete status.
Implementation and test checkpoint: `d4458f457c8b117c76eebd7e20abfebf386b7778`
(`dx11: verify public FL9_1 textured 3D scene`).

Checkpoint `08702e95f9c930e8e14a59d53c18711db39bb2bb` records execution for
both D16 and D24S8 in the 4x resolve test. The latest strict private suite was
377/377. A fresh Lumia reachability check failed: ping to `192.168.1.157:443`
timed out and the read-only Device Portal family request timed out after about
16 seconds. ARM32 UWP compilation is still compile-only evidence. CoreWindow
phone execution remains unverified; continue independent core/HAL audit work
while the device is unreachable.

## Follow-up — depth necessity control

The public textured cube integration test now renders a painter-order control
without depth. At forced FL9_1 on the RTX 3090 Ti, the two D16-tested face
orders match all 1,024 pixels while the no-depth orders differ at 223 pixels.
This verifies that the test scene contains actual overlap and D16 resolves it.
The strict scene test passed 1/1. This remains one bounded workload and does
not change the overall partial status.

## Latest increment — per-count query-gated multisampling — 2026-11-07

RGBA8 render/resolve and matching depth attachments now use independent
`CheckMultisampleQualityLevels` results for 2x, 4x, 8x and 16x. The RTX 3090 Ti
forced to FL9_1 reports RGBA8 quality levels 1, 1, 1 and 0. Strict public
draw/resolve/readback passed at 2x and 8x with Depth24Plus (64/64 green pixels
each); the existing 4x test still passed with D16, Depth24Plus and D24S8,
including zero sample-mask behavior. No 16x capability was projected.

The private strict D3D11 suite passed 397/397, full public D3D11 passed 72/72
with one optional Float16x2 input subcase skipped, the selected-device core
fixture passed, and DX12 core passed 94/94. HAL/public Clippy passed with only
existing HAL warnings. Rustfmt check still identifies unrelated pre-existing
format differences in the fixture and capability files. The capability/core
audit continues; this is not completion of the FL9_1 mission.

## Latest feature increment — query-gated wireframe — 2026-11-07

The D3D11 capability projection now includes `POLYGON_MODE_LINE` only if
`CreateRasterizerState(D3D11_FILL_WIREFRAME)` succeeds on the selected device.
Triangle wireframe is mapped to the native fill mode; point polygon mode stays
rejected. On the RTX 3090 Ti forced to FL9_1, a public GPU test compared the
wireframe triangle's complete 16×16 output against five explicit line segments
and matched all pixels. The private HAL suite passed 398/398 and public suite
73/73, with one unrelated optional IA-format subcase skipped. See the latest
progress entry; the wide capability and platform audit remains in progress.

## Latest feature increment — query-gated R8_UNORM mip chains

R8_UNORM mip chains now require selected-device TEXTURE2D | MIP support and
the enabled backend path. NPOT dimensions additionally require positive
unrestricted-NPOT support. Render-target and copy-source uses require
RENDER_TARGET and matching implemented paths; SHADER_SAMPLE independently
controls filterability. The strict public FL9_1 test sampled mip 2 from a
four-level upload, cleared mip 0 and read mip 0 back on the RTX 3090 Ti; all
64 RGBA pixels and 64 R8 texels matched. The full private HAL, public D3D11,
adapter-isolation and DX12 core suites passed 402/402, 75/75, 1/1 and 94/94.
A partial mip-1 R8 copy and a mip-1-to-mip-2 copy between mipmapped textures now pass; other cross-level combinations and historical/Lumia behavior remain unverified. Continue
the public-valid core/HAL contract audit rather than treating this as backend
completion.

## Follow-up — R8 mip subresource copy verified

The strict public D3D11 suite now passes 76/76 with a new FL9_1 partial mip-1
R8 copy test. Two four-level R8 textures copy a 2x3 region from mip 1 into a
nonzero destination origin; padded readback verified the 4x4 mip, copied values
and preserved border. A mip-1-to-mip-2 copy also passed; other cross-level
unverified. The mission remains partial; the broad API audit and historical
hardware/runtime checks continue.

### Follow-up — R8 NPOT mip execution

The latest strict test exercised a 6x10 four-level R8_UNORM chain because the
selected RTX 3090 Ti positively reported unrestricted NPOT mip sampling at
requested/actual FL9_1. All 64 sampled output pixels and all 60 mip-0 texels
matched. If the query is absent, the test reports the query state and exercises
the POT baseline; synthetic capability tests prove NPOT rejection.


## Surface loss cleanup — reentrant present

The public HWND fixture now covers a second acquired frame consumed by a
device-lost callback's reentrant `Queue::present`. The cleanup bug was one
layer earlier than the queue reentrancy guard: `Surface::present` checked the
configured device and returned `Lost` before invoking the queue, while the
public frame had already been marked presented. The surface's retained core
acquisition is now released on that early lost-device return.

On the physical RTX 3090 Ti at explicitly requested/actual FL9_1, the test
injected `DXGI_ERROR_DEVICE_REMOVED`, observed the callback, reconfigured the
other surface on a healthy device, and reacquired a frame. Both tests in
`dx11_surface_error_fixture` passed. This is injected error-path coverage, not
physical device removal/recovery. Full public API auditing, historical
FL9_1 drivers and Lumia/CoreWindow runtime remain open.

The latest verified format increment adds query-gated R8G8B8A8_SNORM sampled
mip chains. The format's FL9_1 required-format guarantee includes MIP, while
the selected-device `TEXTURE2D | MIP` query and backend path remain independent
gates. The strict RTX 3090 Ti test sampled mip 1 from a three-level upload and
matched all 16 pixels. Full private HAL passed 402/402, strict public D3D11
passed 79/79 (one optional Float16x2 subcase skipped), the core adapter test
passed 1/1 and DX12 core passed 94/94. See `FL9_1_PROGRESS.md` for commands
and details. Continue the public contract audit; historical drivers and
Lumia/CoreWindow remain unverified.

The current follow-up adds an optional RG8_UNORM sampled/upload mip path. It is
exposed only when `CheckFormatSupport` reports `TEXTURE2D | MIP` and the
backend path is enabled; filterability and NPOT remain independently gated.
The RTX 3090 Ti forced to FL9_1 reports `0x0002c320` without MIP, so its strict
test explicitly skips this optional operation. Synthetic capability checks
pass, and the controlled core fixture confirms the restricted projection.
The positive mip path has not run on hardware. Continue the wider public
contract audit and historical hardware/runtime verification.

The current uncommitted increment adds optional timestamp-query support after
a positive timestamp/disjoint probe on the selected device. A strict public
FL9_1 test now resolves encoder, render-pass and empty-compute-pass boundary
timestamps on the RTX 3090 Ti and verifies nondecreasing values at 1 ns/tick.
The strict public suite passed 93/93 and the private HAL suite passed 419/419.
The resolve path waits on the CPU; occlusion and pipeline-statistics queries,
as well as explicit timestamp-write methods inside a pass, remain unavailable.

The latest narrow capability increment adds fragment `textureDimensions` for
one direct group-0 `texture_2d<f32>` query in a simple entry point, directly
converted to `vec2f`. It uses the existing private `b13` dimensions path,
keeps general `texture_query` disabled and passed strict RTX FL9_1 readback
across a bind-group transition. The full private HAL and public D3D11 suites,
DX11/DX12 Core tests, `wgpu-types` tests, formatting and `wgpu-info` check also
passed. See `FL9_1_PROGRESS.md` for exact counts and the physical GPU output.
Historical hardware, CoreWindow runtime and broad API/CTS verification remain
open.

## Current continuation — narrow FL9_1 base-clamp sampling — 2026-11-08

The opt-in FL9_1 projection now permits the narrow direct fragment
`textureSampleBaseClampToEdge` form on one single-mip 2D float texture and one
non-comparison sampler. Naga uses selected-view dimensions from `b13` and an
implicit `Sample`, avoiding the level-9 profile's rejected `GetDimensions` and
`SampleLevel` lowering. Core proves the selected shader shape; HAL refreshes
the dimensions per selected resource before drawing. The physical RTX 3090 Ti
at requested/actual FL9_1 checked all 64 pixels for out-of-range coordinates.
Repeat and MirrorRepeat samplers remain rejected by Core's finite repeat limit,
and the test uses ClampToEdge; independence from those modes remains unverified.
See `FL9_1_PROGRESS.md` for commands and test counts. The broad Core/HAL
contract audit, historical FL9_1 drivers, Lumia/CoreWindow runtime and physical
device-removal/recovery remain open.

## Current continuation — narrow fragment `textureNumLevels` — 2026-11-08

The next narrow query increment adds an independent Core limit and validated
Naga/HAL proof for a direct fragment `textureNumLevels` on one group-0
`texture_2d<f32>`, directly converted to `f32`. Runtime metadata is the bound
view's mip count in the existing host constant. On the RTX 3090 Ti, explicitly
requested/actual FL9_1, a 1-level/3-level bind-group transition produced the
expected two red values in all 64 pixels. Verification counts and exact
commands are in `FL9_1_PROGRESS.md`. Naga HLSL-enabled tests passed 142/142,
private HAL 424/424, strict public D3D11 96/96, and focused Core/types tests
passed. DX12 Core `cargo check` passed after its HLSL options explicitly left
the FL9-only proof disabled; the full DX12 test build and Naga Clippy were
stopped by Windows memory-allocation failures. Continue the Core/HAL contract
audit and consider selected-mip `textureDimensions` only after checking the
host-side view and mip semantics. Historical drivers, Lumia/CoreWindow and
physical removal/recovery remain open.

## Current continuation — narrow 3D fragment `textureDimensions` — 2026-11-08

Added an independent 3D dimensions proof and capability gate. Only a direct
fragment query on a group-0 non-array `texture_3d<f32>`, with no explicit mip
argument and a direct `vec3f` conversion, is accepted. The selected-device
RGBA8 3D sampled-format query must pass. Strict readback on the RTX 3090 Ti at
requested/actual FL9_1 verified all 64 pixels for a 4x2x8 volume. Core, HAL,
types and Naga focused tests passed. The next candidate is a narrow
`textureSampleBaseClampToEdge` lowering; keep public-contract, historical
driver, Lumia/CoreWindow and physical device-loss gates open.

## Current continuation — 2D dimensions plus implicit sampling — 2026-11-08

The narrow direct fragment 2D `textureDimensions` capability can now coexist
with one implicit `textureSample` of the same group-0 `texture_2d<f32>` and one
group-0 non-comparison sampler. Sampling must be level-less; a constant-mip
dimensions query remains query-only. The selected-entry proof also rejects
helpers, extra resources, extra image operations, control flow, `select` and
unrelated runtime integer operations. General `texture_query` remains false.

An RTX 3090 Ti at forced/requested/actual FL9_1 rendered through a bind-group
transition between 4x2 and 8x4 sources with different texel colors. Readback
matched all 64 pixels, including values depending on both view dimensions and
sampled contents. Naga passed 145/145, Core 106/106, private HAL 430/430; the
focused public strict GPU test passed 1/1. The full public suite rebuild was
blocked by Windows commit pressure (346 MiB free during the attempt), despite
the focused test passing. See `FL9_1_PROGRESS.md` for the exact commands,
readback values and details of the binding-map regression caught by the HAL
suite and fixed before its final pass.

Current remaining work is not general WebGPU completeness. Public D3D11 stays
opt-in and experimental. The full public D3D11 suite now passes 97/97 on the
physical RTX 3090 Ti at forced FL9_1. There is no public CoreWindow path or
Lumia ARM32 execution; historical FL9_1 drivers and physical device-removal
recovery are unverified. The remaining shader/resource contract is deliberately
narrower than Core WebGPU. See the current plan for next candidate feature
slices and the full contract audit status.

## Latest continuation — narrow 3D fragment `textureLoad` — 2026-11-08

A separate 3D float fragment texture-load limit and Core/Naga/HAL path are now
implemented. Naga lowers one single-mip direct 3D load to point sampling at
texel centers; HAL binds the reserved point sampler and writes width, height
and depth through the existing `b13` emulation buffer. The actual FL9_1 FXC
profile accepted it. The RTX 3090 Ti at requested/actual FL9_1 verified two
different texels, an out-of-bounds result and a nonzero-mip result across 256
readback pixels. A first GPU run found and fixed a missing host depth value.
Focused tests pass; the full HAL/public/Core/types regression suites are the
next verification gate. General texture loading remains disabled, and the
query-gated 3D limit is distinct from the existing 2D subset. Historical FL9_1
drivers, Lumia/CoreWindow runtime, exhaustive public-contract review and
physical device-loss recovery remain open.

## Latest continuation — 3D fragment `textureDimensions` plus implicit sampling — 2026-11-08

The existing `texture_dimensions_3d_float_fragment` capability now permits one
direct level-less `textureDimensions` query converted to `vec3f` plus one
implicit sample of the same group-zero `texture_3d<f32>` through one
non-comparison sampler. Core, Naga and HAL reject helper functions, explicit
LOD, unrelated image operations and extra resources. The feature remains gated
by a positive selected-device RGBA8 3D sampled-format query, and general
`texture_query` remains false.

Strict readback ran on an NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), with requested and actual FL9_1. A bind-group
transition changed the input from a 4x2x8 volume to a 2x4x1 volume. The shader
multiplied distinguishable sampled colors by the returned dimensions; all 64
pixels matched `[200,80,40,255]` and `[40,120,10,255]` respectively. FXC
accepted the generated pixel shader for `ps_4_0_level_9_1`.

After integration, Naga passed 147/147, DX11 Core 106/106, strict private HAL
434/434 and the strict public D3D11 target 98/98. The default-enumeration
isolation test passed. DX12 HAL `cargo check`, affected Clippy, workspace fmt
and `git diff --check` passed; existing lint and line-ending notices are
recorded in `FL9_1_PROGRESS.md`. This addition does not provide general 3D
queries, explicit LOD, 3D copy/attachment/readback or historical-driver proof.

The public Core/HAL contract audit remains broad and incomplete. Other material
gaps are restricted WGSL semantics and resource usages, historical FL9_1
hardware, Lumia ARM32/CoreWindow runtime, and physical device removal/recovery.
The current feature is still an opt-in experimental adapter, not general
WebGPU-complete support.

## Latest public execution proof — invariant position

`dx11_invariant_position` verifies that the tested WGSL invariant position
survives a depth-only prepass and matches a later Equal-depth color pass. The
strict test passed on physical RTX 3090 Ti hardware at requested/actual FL9_1,
with 64/64 expected pixels. This records evidence only; it does not expand the
public feature report. The broad Core/HAL contract audit and historical
FL9_1/Lumia runtime verification remain open. See `FL9_1_PROGRESS.md` for the
exact command and device information.

## Selected-view sampler LOD validation — 2026-11-10

HAL sampler translation metadata now reaches Core through the defaulted
`DynSampler::mip_compatibility` method. Core records selected texture-view mip
counts in bind groups and validates only the current pipeline's used sampling
pairs during render-pass and render-bundle command validation. A strict forced-
FL9_1 Core fixture rejects a two-mip view for a sampler limited to one mip
before HAL draw execution, then continues the hardware suite successfully.
See `FL9_1_PROGRESS.md` for full verification. Next continue the API/Core/HAL
contract audit and select one bounded, user-visible FL9_1 workload. DX11
adapter selection remains disabled by default; explicitly selecting
`Backends::DX11` enumerates physical adapters and applies the restricted
projection. Historical FL9_1 devices, Lumia ARM32/CoreWindow execution, broad
CTS coverage and physical device-removal recovery are still open.

## Timestamp query contract correction — 2026-11-10

Public D3D11 FL9_1 timestamp features are now disabled. A startup disjoint
probe does not guarantee timestamp validity/frequency for user query pairs that
Core permits to cross submissions. The public queue period returns zero, and
Core rejects feature requests and timestamp query-set creation. The native HAL
query path remains diagnostic-only; a strict RTX 3090 Ti FL9_1 test verified
two writes inside one valid disjoint interval. Strict public, Core and HAL
suites passed 121/121, 132/132 and 468/468 respectively. See
`FL9_1_PROGRESS.md` for exact commands. This closes a capability-reporting
overclaim, not an FL9_1 implementation gap.

## Recent continuation — two direct fragment uniform-array reads

Core and HAL now permit at most two selected-entry direct fragment reads from
distinct uniform bindings, preserving the existing `u32` index plus
`array<vec4f, N>` structure proof and `2 <= N <= 24` bound. FXC accepted the
two-access `ps_4_0_level_9_1` shader. Strict GPU readback verified the original
single-array case and three two-binding index pairs on the RTX 3090 Ti at
requested/actual FL9_1. Core passed 123/123, the strict private HAL suite
478/478, the strict public D3D11 suite 126/126, and the focused integration
1/1. DX12 HAL compilation, affected Clippy, formatting and `git diff --check`
passed; two optional public-suite subcases were capability skips. Exact
commands and outputs are in `FL9_1_PROGRESS.md`.
Implementation checkpoint: `9fd21279b` (`dx11: allow two FL9_1 fragment
uniform array reads`).

## Current mission state — query-gated BGRA8 multisample resolve — 2026-11-11

BGRA8 now uses independent device format/sample-count queries for multisample
render-and-resolve. The strict RTX 3090 Ti FL9_1 path checked all 64 pixels;
the complete HAL, public D3D11 and Core DX12 suites passed 510/510, 135/135
and 128/128. The one-byte UNORM rounding tolerance is documented in
`TESTING.md` and exact evidence is in `FL9_1_PROGRESS.md`. The current
uncommitted BGRA8 work still needs final lint, formatting and diff checks
before its feature checkpoint.

This does not close the mission. Remaining software work is broad and
application-facing: the selected WGSL subset is still constrained, several
valid WebGPU/downlevel operations remain unavailable or need further contract
audit, and a useful public example must exercise adapter/device selection,
textured indexed 3D rendering and presentation. Compute/UAV, geometry shaders,
MRT, 1D textures and ordinary texture arrays are not FL9_1 capabilities and
must remain rejected. Historical FL9_1 hardware, Lumia ARM32/CoreWindow runtime
and physical device-removal recovery require external verification. Use
agents for independent audits and compiler probes; keep each Core/Naga/HAL
implementation slice under the coordinator so it can be verified end to end.

Latest verified increment: one used uniform can accompany the selected
fragment 2D float `textureLoad` emulation, with two strict physical FL9_1
readbacks and negative multi-uniform/3D checks. The full Core DX11, Core DX12,
Naga, private HAL and strict public D3D11 suites passed 136, 135, 170, 515 and
152 tests respectively. See the final `One uniform with selected fragment 2D
textureLoad` entry in `FL9_1_PROGRESS.md` for commands, warnings and GPU
identity. Public adapter exposure remains disabled, and the broader completion
mission remains open.

## Current coordinator state — eight-image fragment 2D textureLoad — 2026-11-12

The latest increment removes the one-image limit from the proven selected-entry
fragment 2D float `textureLoad` path, up to the FL9_1 eight-image binding cap.
An FXC probe first measured the old helper at 65 arithmetic slots for seven
images and 74 for eight versus the 64-slot profile limit. The production helper
then removed a redundant coordinate clamp while retaining point/clamp-to-edge
sampling and the explicit out-of-bounds zero mask; representative production
shaders for one through eight images now compile. A ninth is rejected by Core
and HAL. Strict D3D11 public and private suites passed 152/152 and 516/516;
the public run verified 64/64 pixels for the eight-image result on the RTX
3090 Ti at explicitly requested/actual FL9_1. Other full regressions: Naga
171/171, Core DX11 136/136, Core DX12 135/135. Formatting, diff check, D3D11
HAL check and affected Clippy commands completed. See `FL9_1_PROGRESS.md` for
exact commands and optional-device skips.

Continue as coordinator. Parallel agents are useful for separate read-only
capability inventories, compiler-limit probes and targeted audits. Avoid
parallel implementation of Core/Naga/HAL slices because they share the proof,
resource mapping and draw-validation contract. Current remaining work includes
narrow shader/resource/API support, opt-in backend usability and proof on
historical FL9_1 devices, Lumia ARM32/CoreWindow and device removal/recovery.
No commit was made.

## Current coordinator state — six-uniform fragment textureLoad — 2026-11-12

The selected-entry fragment 2D float `textureLoad` proof now allows up to six
used uniform buffers alongside up to eight images. This matches the projected
per-stage uniform-binding limit; other bound resource classes and uniform-backed
3D or combined load/dimensions emulation remain rejected. Core, Naga and HAL
accept six sparse bindings and reject seven. Strict physical GPU tests passed
for both group-0 and group-1 layouts at forced FL9_1, verifying the six-buffer
output `[64, 96, 32, 223]` at all 64 pixels. The complete public suite passed
152/152 and private HAL suite passed 516/516. Full Naga, Core DX11 and Core DX12
suites passed 171/171, 136/136 and 135/135. This remains a narrowly proven
shader path, not general multi-resource shader validation.

## Current coordinator state — instance-indexed matrix uniforms — 2026-11-12

Core and HAL now permit a selected vertex entry to directly index a fixed
`array<mat4x4<f32>, N>` with `instance_index` for `N <= 63`. Computed indices,
non-square matrices and longer arrays remain rejected. The production HAL path
compiles with FXC at `vs_4_0_level_9_1`, and strict public GPU readback verified
two separately translated instances and all 64 pixels on the physical RTX
3090 Ti at requested/actual FL9_1. Full suites passed Naga 171/171, Core DX11
136/136, Core DX12 135/135, private HAL 517/517 and public D3D11 153/153.
Formatting, `git diff --check`, D3D11 HAL check and affected Clippy passed;
HAL reports pre-existing warnings. Exact commands and observations are in
`FL9_1_PROGRESS.md`.

The backend is suitable for opt-in experiments and small workloads within its
reported subset, but is not a full FL9_1 WebGPU implementation. Remaining work
is still feature work and contract closure: constrained WGSL semantics and
resource/usages, any remaining accepted Core-to-HAL mismatches, example and
application integration polish, and broad CTS coverage. Compute/UAV,
geometry-stage shaders, multiple color attachments, indirect draws and
occlusion queries remain hard FL9_1/profile limits. Historical FL9_1 drivers,
Lumia ARM32/CoreWindow runtime and real device-removal recovery remain external
verification gaps. Independent audits are running; implementation remains
coordinated across Core, Naga and HAL.

## Latest verified increment — sRGB fragment gather — 2026-11-12

The narrow selected-entry fragment `textureGather` emulation now handles
single-mip `Rgba8UnormSrgb` views when the selected device reports sampled,
filterable support. Core validates the bound format at each draw because the
custom downlevel limit cannot express format restrictions; unsupported R8
views are rejected before HAL. Strict RTX 3090 Ti execution at requested and
actual FL9_1 matched 64/64 pixels for both the linear baseline and sRGB decoded
result. Focused Core, HAL and strict public test results are in
`FL9_1_PROGRESS.md`. After the change, Core DX11 136/136, Core DX12 135/135,
private HAL 517/517 and strict public D3D11 153/153 passed; formatting and
diff checks also passed. Public D3D11 remains opt-in and does not claim general
texture-gather support.

## Latest coordinator state — bounded computed explicit LOD — 2026-11-12

The direct one-mip `textureSampleLevel` proof now accepts finite LOD literals
and whitelisted side-effect-free scalar expressions over fragment inputs.
Core/HAL retain the same strict shape proof and live draw gate: selected view
has exactly one mip, min/mag filters match, anisotropy is one. The straight-line
helper still accepts only literal positive zero; mutable-local loads, helper
calls with dynamic LOD, branches, selects and other shapes stay rejected. The
physical RTX 3090 Ti at requested/actual FL9_1 rendered both literal and
computed LOD forms and matched 64/64 readback pixels. Full Core DX11/Core DX12,
private HAL and strict public tests passed 136, 135, 518 and 153 tests. The
public test explicitly selected the experimental `Backends::DX11` path;
ordinary default backend selection still excludes D3D11. Full details and
exact commands are recorded in `FL9_1_PROGRESS.md`.

The strict format-blending CTS probe separately found four supported linear
normalized cases passing and six boundary failures: FL9_1 sRGB blend semantics
are rejected, while four optional formats fail the CTS's generic preflight; device-specific format projection mismatch
adapter-specific format projection boundary. No Core-valid unsupported draw
reaching HAL was demonstrated. Continue contract auditing from concrete
reproductions; do not advertise those formats or widen sRGB blend semantics.

## Current coordinator checkpoint — queue errors and API audits — 2026-11-12

Core now completes pending map callbacks with `DeviceLost` if queue wait or
fence-value retrieval fails after device invalidation. The fixture submits a
real D3D11 buffer clear, schedules a map, injects device removal at wait, and
asserts a single callback. D3D11 queue-idle waits reuse one event query created
at device open; the query lifecycle is serialized across queue wrappers and
the query is poisoned after a failure. Strict hardware tests passed sequential
and concurrent reuse. The debug-layer indexed render reported zero errors and
only the device/context live after the test released the cached query.

Current regressions passed Naga 172, Core DX11 147, Core DX12 135, private HAL
523 and strict public DX11 156 tests. The public run explicitly required
hardware and selected the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
at requested/actual FL9_1. Core and HAL Clippy and pinned workspace formatting
passed; existing warnings remain. Exact commands are in `FL9_1_PROGRESS.md`.

Three independent audits at that checkpoint found no confirmed Core-valid request
reaching an unsupported HAL operation in their reviewed shader, texture/view,
copy/render-pass, capability and queue-lifetime paths. One generic robustness
gap remained then: Core queue-drop error retention and bounded pending-present
waits were implemented in subsequent entries below. The injected device-loss
tests do not reproduce physical device removal.

This remains an opt-in constrained FL9_1 subset. No historical FL9_1 adapter,
Windows 10 Mobile CoreWindow runtime or physical device-removal recovery has
been verified. Use agents for independent read-only audits and probes; keep
each Core/Naga/HAL feature implementation with one coordinator and require
current-tree GPU evidence before expanding shader/resource behavior.

## Earlier interruption status — superseded — 2026-11-12

**Outcome: PARTIALLY COMPLETE.** The backend is publicly usable by explicitly
selecting `Backends::DX11` on a physical FL9_1 adapter. The default backend set
still excludes D3D11. Public verification covers a textured depth-tested cube,
offscreen readback, a visible desktop HWND example, supported resource groups
and dynamic uniforms, transfers, render-pass state, selected depth/stencil,
and query-gated multisample/format paths. This is a constrained graphics
subset, not general WebGPU or full FL9_1 API support.

The latest current-tree test counts are Naga 172/172, Core DX11 147/147, Core
DX12 135/135, private D3D11 HAL 523/523, strict public D3D11 156/156, and the
separate strict HWND surface-copy target 1/1. The public and HAL tests
explicitly selected the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested FL9_1 and asserted actual FL9_1 with `software=false`. Debug-layer
indexed rendering reported zero messages/errors and only device/context live
after test cleanup. The standalone visible window sample was previously run
for 60 frames and inspected. It now uses a separate material uniform in group 2
alongside the transform uniform in group 0 and texture/sampler group 7; a
strict current-tree run presented five frames successfully. ARM32 HAL and public
`wgpu` cross-checks compile,
but the Lumia Device Portal timed out; no Lumia/CoreWindow runtime was run.

Unmet work at that checkpoint included broader Core/CTS contract closure;
real historical FL9_1 driver coverage;
Lumia/CoreWindow execution; and physical device-removal/recovery. Derivatives,
compute/UAV, geometry shaders, MRT, vertex texture sampling, ordinary 1D and
array textures, indirect draws, and FL9_1 occlusion queries are unavailable or
hardware/profile-limited. Other shader, format, view and transfer behavior is
intentionally gated to the tested subset. Microsoft's FL9_1 feature table
lists one simultaneous render target, no occlusion query or separate-alpha
blend, no ordinary 1D/array texture resources, and no indirect drawing.

Independent shader, public exposure/resource, and runtime audits found no new
Core-valid request reaching an unsupported HAL operation in the paths they
reviewed. The runtime audit confirmed the queue-drop error path above. These
were scoped reviews, not a complete formal API proof. The broad render-pass
CTS probe produced 3 passes, 41 skips and 139 failures, triaged largely to
unsupported descriptor and usage families; it is boundary evidence, not a
passing conformance suite.

The Git snapshot and test counts in that earlier checkpoint have been
superseded by the coordinator status above and later progress entries below.

## Queue idle-wait error retention — 2026-11-12

The queue-drop panic described above has been addressed. Global queue removal
now checks for idle while serialized. On an unexpected non-device-loss wait
error, Core marks the device lost and retains the queue in the Hub, preserving
the lifetime tracker so device polling can still complete submitted-work
callbacks. Dropping the Device retries orphan retirement when no other Core
references remain. If the retry still cannot prove idle, Hub shutdown leaks the
orphan for process lifetime rather than releasing potentially in-flight
resources. Queue's final destructor path also avoids panicking or dropping its
HAL queue, pending writes, lifetime tracker or Device after an unconfirmed idle
wait. D3D11 pending-present waits use bounded timeout-aware waits; this entry
records queue teardown behavior before the later surface-unconfigure recovery
work. The legacy blocking `wait_for_idle` API remains unbounded when called
directly, but Core's current queue-reaper path invokes it only after a bounded
wait has confirmed idle, so a pending present does not reach that path.

A strict synthetic OOM injection test on the physical RTX 3090 Ti at requested
and actual FL9_1 passed: the device-lost callback re-entered polling, the
submitted-work callback fired exactly once, and device destruction retried and
retired the queue. Full Core DX11 (148/148), Core DX12 (135/135) and private
D3D11 HAL (523/523) suites passed. This does not simulate a real driver OOM or
resolve the pending-present timeout contract.

## Bounded queue idle completion — 2026-11-12

The pending-present wait is now covered by an additive timeout-aware HAL
operation. D3D11 preserves an in-progress event query and pending CoreWindow
present handles across timeout, while Core retains queues and their resources
until a later idle confirmation. Queue/Device drop uses a 10 ms attempt; live
Device polling and `poll_all` retry orphan retirement. `poll_all` reports work
remaining while an orphan is retained, including after Device registry
removal. A backend that does not provide bounded waits returns `None` and
keeps the prior blocking drop semantics. Unresolved queues are intentionally
leaked when Hub shuts down.

On the physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at
requested/actual FL9_1, strict tests passed for query resumption and both Core
reaper paths. Synthetic error injection retained timeout, OOM, and device-loss
coverage. The full strict suites now pass: private HAL 525/525, Core DX11
150/150 with one explicit RG8 mip capability skip, public API 157/157 with no
skips, and Core DX12 135/135. The public suite includes query-gated RGB565
multisample blending at 2x/4x/8x. Previous nearby handoff counts predate these
tests. See `FL9_1_PROGRESS.md` for commands and details.

## Direct uniform-indexed vertex palette — 2026-11-14

Core and HAL now accept one narrow canonical vertex uniform-array access:
`palette.values[palette.index]`, where the uniform has a `u32` at byte offset
0 and a fixed `array<vec4f, N>` at byte offset 16 (`2 <= N <= 253`, exact
structure span). FXC accepts length 253 and rejects 254 with X4507 at
constant register index 256. The selected vertex entry must be straight-line
and perform only that one direct dynamic access. Computed indices, helpers, control flow,
selects, other array element types and additional dynamic accesses remain
rejected. The existing downlevel vertex-uniform-indexing limit is unchanged.

The strict public test ran on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) with requested and actual FL9_1 and `software=false`. Four
in-range palette indices selected distinct colors; all 64 pixels matched in
each readback. Current complete local runs: private D3D11 HAL 533/533, Core
DX11 153/153, strict public DX11 157/157, Core DX12 140/140. See
`FL9_1_PROGRESS.md` for exact commands. This does not verify historical
FL9_1 drivers, out-of-range palette indices, Lumia/CoreWindow execution, or
physical device-removal recovery. Public DX11 remains explicitly opt-in and
constrained; the completion mission remains active.

## Current coordinator handoff — 2026-11-14

Branch `uwp-dx11`, HEAD `a5a7a55ff`. The worktree contains ten modified
tracked files from follow-up tests and documentation. Current strict selected-
hardware runs pass: public `dx11_public` 167/167 and private D3D11 HAL
538/538. Hardware was the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), physical rather than WARP, with FL9_1 explicitly requested
and confirmed. These are broad selected-device results, not historical-device
or exhaustive contract proof.

New evidence includes public 8-varying success / 9-varying Core rejection,
Repeat and MirrorRepeat limit rejection at the projected limit, and actual
anisotropic minification output (stripe contrast 1 at anisotropy 2 versus 128
at anisotropy 1). Full commands and details are in `FL9_1_PROGRESS.md`.
Clippy and formatting passed; Clippy reports existing D3D11 warnings. No code
in this follow-up changes adapter selection policy.

This is suitable for experimentation and for applications that stay within
the tested, explicitly selected subset, but it is not a broadly usable,
feature-complete FL9_1 wgpu backend. Deliberate or hardware/profile limits
still exclude compute/UAV/storage, geometry shaders, multiple color outputs,
indirect draw, occlusion queries, ordinary 1D and 2D-array textures, vertex
texture sampling and unsupported shader operations. Capability/CTS evidence
is not exhaustive. The RTX forced to FL9_1 does not stand in for historical
9_1 drivers; Lumia/CoreWindow runtime and physical device-removal behavior
remain unverified, as does production Horizon terrain integration.

Continue with bounded, independent agent audits or tests where file ownership
is clean. Delegate specifications, capability-to-operation mapping, shader
compiler probes and workload reviews; keep cross-crate design and integration
coordinator-owned, and serialize Cargo/GPU runs. The prior “13 dynamic
uniforms” and “eight inter-stage variables” evidence gaps are closed; do not
reopen them without contrary evidence. Current follow-up should target a
concrete API/workload gap or a focused public capability boundary.
Recent agent reviews of texture/surface projection and shader boundaries found
no new confirmed contract defect or workload-independent shader feature. A
strict CTS buffer-to-texture initialization selector passed 1/1 on the RTX at
FL9_1 (source offsets 0 and 8). It is now in the shared passing list after
1/1 passes on DX11, DX12 and Vulkan and a passing list-sorting check. The
detailed agent synthesis and command are recorded in
`FL9_1_PROGRESS.md`.
## Current coordinator handoff — 2026-11-14

Branch `uwp-dx11`, HEAD `e62c3b7d0` (`dx11: verify FL9_1 public capability boundaries`), clean worktree, 308 commits ahead of `origin/uwp-dx11`. The focused checkpoint added strict public evidence for packed/SNORM vertex formats and the 2048-wide FL9_1 texture limit, updated the capability/CTS/testing records, and preserved the existing anisotropic minification test. No production capability was expanded.

Latest complete strict runs used the physical NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) with requested and actual FL9_1: public `dx11_public` 169/169 and private HAL library 538/538. The 2048-wide upload and final-texel readback matched `[29, 113, 197, 251]`; Core rejected width 2049. Both affected-crate Clippy commands succeeded with existing D3D11 HAL warnings; all-workspace formatting and `git diff --check` passed. Strict mode was `WGPU_DX11_REQUIRE_HARDWARE=1`.

Three disjoint read-only audits found no confirmed public-Core-valid request reaching a known HAL rejection in their reviewed scopes. Uniform support is broad for the tested subset, but accepted WGSL forms remain deliberately narrow around FL9_1 compiler constraints. Resource review found selected-device boundary evidence gaps rather than a concrete failure. The application-facing review considers this suitable for explicitly selected experiments, not a drop-in backend. The production Horizon renderer is present in the sibling `HorizonWiiU` checkout but was not integrated or built against this fork.

Remaining work is split between: (1) evidence-led implementation for concrete application/API gaps, while keeping the capability-to-Core-to-HAL audit non-exhaustive until covered systematically; (2) intentional FL9_1/profile restrictions such as compute/UAV/storage, geometry shaders, multiple color outputs, indirect draws, query families, 1D/array textures, vertex texture sampling and unsupported shader forms; and (3) external verification on historical FL9_1 drivers, Lumia ARM32/CoreWindow, and a physically removed/reset device. Forced FL9_1 on the modern RTX is not a substitute for those devices.

Agents are most useful for independent, disjoint audits and compiler/spec probes. Keep cross-crate implementation choices, GPU tests and final integration coordinated here; serialize heavyweight Cargo/GPU runs. Do not assign agents one feature category each for concurrent implementation. The next local increment should be chosen from a specific supported-workload or public-validation gap, not speculative expansion of blocked FL9_1 features.

## Coordinator update — actual Horizon path and Lumia check — 2026-11-14

The `HorizonWiiU` sibling source is available for read-only workload inspection. The Windows client currently enables only DX12; its ordinary terrain pipeline declares 2D-array sprite resources and therefore cannot run under the current FL9_1 projection. Optional culling/OIT can be disabled, but that does not remove the terrain array requirement. The first useful integration smoke test is an app-selected simple 2D-textured mesh; a terrain integration requires an application-side atlas/shader variant. No sibling project files were changed.

A read-only Lumia Device Portal query to `/api/os/devicefamily` timed out. This leaves CoreWindow/ARM32 and historical hardware verification open without indicating a D3D11 failure.

## Current coordinator handoff — optional RG8 3D increment — 2026-11-14

Branch `uwp-dx11`; verified code checkpoint `9152a4e8d89f2b37ac795870f1ad834119ae81fd`
(`dx11: expand FL9_1 formats and rendering coverage`). The checkpoint includes
query-gated RG8_UNORM 3D projection, public nonindexed line-list boundary and
textured depth-tested integration tests, and documentation. The working tree
was clean after that checkpoint. It is a selected-device implementation; no
push was made.

The private HAL suite passed 546/546. The complete strict public D3D11 target
passed 173/173 test functions in one process after the textured depth test was
added; four optional format subcases logged explicit skips. Fixture-enabled
Core passed 155/155, Core DX12 passed 139/139, and `wgpu-types` passed 120 unit
tests plus 14 doc tests (21 ignored). Earlier monolithic public runs
intermittently hit FXC internal `0x8007000e` errors in varying shader tests;
the latest complete run succeeded.

The strict selected adapter was the physical NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with FL9_1 requested and confirmed.
Its `R8G8_UNORM` format query was `0x0002c320` and did not include
`TEXTURE3D`; private and public RG8 3D tests correctly printed `SKIP`, so no
hardware execution is claimed for the optional RG8 volume path. Existing R8
and RGBA8 volume paths remain exercised.

A new focused public wgpu textured depth test passed 1/1 on that RTX at forced
FL9_1. Three sparse-bound sampled textures were drawn at increasing depth;
all 64 pixels retained the middle red sample after the farther blue draw was
rejected. The subsequent full 173-function strict public run also passed.

Current product status: explicitly selected FL9_1 is useful for the tested
subset, not drop-in wgpu compatibility. Remaining software priorities are
evidence-led Core/HAL/CTS contract coverage and concrete consumer integration.
Compute/UAV/storage, geometry, MRT, indirect draw, occlusion queries, ordinary
1D/2D-array textures and unsupported shader forms remain deliberate
restrictions. Historical FL9_1 hardware, Lumia/CoreWindow runtime and physical
device-removal recovery remain external gates. Use agents for disjoint
specification, capability and CTS audits; keep cross-crate implementation,
hardware validation and integration coordinated here.

## Coordinator update — ARM32 UWP build evidence — 2026-11-14

The pinned Rust 1.93.1 toolchain, using `rust-src` and `-Z build-std` for the low-tier target, successfully checked the full HAL/Core/public `wgpu` library chain for `thumbv7a-uwp-windows-msvc`. A minimal consumer selecting `Backends::DX11` linked for ARM32 UWP with the Visual Studio ARM linker and Windows SDK 10.0.22621.0. This reduces the platform source-build risk but is not an AppX deployment, CoreWindow run, adapter creation or GPU execution. The repository's `--tests` cross-check is blocked by `windows-sys 0.52.0` missing Win32 declarations under this target; tests are not needed for the library build. The Lumia address currently has no TCP 443 reachability and Device Portal requests time out.

## Current coordinator handoff — bounded coordinate-derivative gradients — 2026-11-15

Branch `uwp-dx11`; HEAD `8489758b0` (`dx11: accept bounded coordinate
gradients at FL9_1`); working tree clean. Core and the private HAL now permit
finite literal gradients or exact matching `dpdx(coordinate)` /
`dpdy(coordinate)` values used only by the corresponding gradient argument of
the restricted single-mip `textureSampleGrad` path. The direct form and the
existing one-call helper form have positive GPU tests. Mismatched axes,
transformed or dynamic gradients, derivative reuse, control flow, nested or
repeated helpers and general derivative use remain rejected. The
`fragment_derivatives` capability remains false.

Final-tree checks passed: strict public D3D11 175/175, private D3D11 HAL
547/547, Core fixture 155/155, Core DX12 139/139, targeted Core and HAL/FXC
tests, Clippy for Core/HAL/public tests, formatting and `git diff --check`.
Strict hardware is the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) at requested and selected FL9_1, `software=false`; direct and
helper derivative render targets each matched 64/64 pixels. Ordinary default
D3D11 adapter isolation still passes.

Mission status remains PARTIALLY COMPLETE. The public backend is an opt-in
tested subset, not a drop-in complete FL9_1 wgpu backend. Remaining local work
is systematic capability/Core/HAL/CTS closure and a consumer-shaped smoke
test. Historical FL9_1 driver behavior, Lumia/CoreWindow runtime and physical
device-removal/TDR recovery remain external verification gaps. Prior strict
monolithic suite instability has three later clean reruns, including this
175-test run, but its earlier FXC/adapter/access-violation cause is unknown.
Do not broaden derivative semantics or enable ordinary adapter enumeration
without separate evidence.
