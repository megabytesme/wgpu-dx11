# Direct3D 11 validation plan

The backend is experimental and normal adapter enumeration is deliberately
empty. The existing HAL tests and local experiments do not establish WebGPU or
CTS conformance. Do not expose an adapter using placeholder capabilities to make
a test pass: many restrictions, including specific binding layouts, cannot be
expressed by `Limits` alone.

## 1. HAL tests and GPU readback

### Focused FL9_1 hardware test

With strict hardware mode enabled, run the private transient-attachment checks:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 transient_attachment -- --nocapture --test-threads=1
```

The filter passed two cases on the RTX 3090 Ti at requested and selected
FL9_1: one CPU usage-validation test and one physical GPU test. The GPU test
checked all 64 pixels for transient depth followed by readable color, and for
transient color followed by stored depth and readable color. The backend uses
ordinary textures for this path, so the test does not establish memoryless
allocation or memory savings. Historical FL9_1 devices remain unverified.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_sparse_vertex_ -- --nocapture --test-threads=1` checks direct sparse streams,
slot 15 and rejection at 16, range boundaries, pipeline transitions, a queued
GPU copy and resource retention. It binds one buffer in two slots with distinct
byte regions. Separate overlap tests bind overlapping physical bytes through
distinct native slots on the tested RTX; historical FL9_1 devices remain
unverified. The strip test also checks per-slot rebasing across a 65,536-vertex
split, but its opaque, unculled output does not independently establish parity
or instance order.

With strict hardware mode enabled, the signed-normalized vertex checks are:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 snorm_vertex -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_formats_map_signed_normalized_words_without_expanding_other_formats -- --nocapture
```

The strict RTX 3090 Ti test requested and selected FL9_1, observed format
support masks `0x21122` and `0x2` for `Snorm16x2` and `Snorm16x4`, and checked
all 64 pixels from direct and indexed draws using both formats at a nonzero
buffer offset. The CPU test checks the native mappings. A controlled core
fixture also rendered 32 red and 32 green pixels using `Snorm16x2` UVs.
Microsoft lists both IA formats as required at FL9_1, but historical devices
have not been tested here. The private backend still requires four-byte
attribute alignment and does not thereby support SNORM texture formats.

With strict hardware mode enabled, run the zero-stride tests with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 zero_stride -- --nocapture --test-threads=1
```

The filter passed one CPU case and one physical RTX 3090 Ti case at requested
and selected FL9_1. The GPU case checked five full-image executions with the
constant stream first or last, covering direct and strip draws, Uint16 and
Uint32 indexed windows and gather, a constant-resource update, and rejection of
a 19-byte range where the attribute needs 20 bytes. An all-zero-stream triangle
only established degenerate draw completion. The controlled core fixture used a
synthetic per-stage buffer limit of two and checked 64 green pixels. The correct
handling of a negative base-adjusted index when all streams have zero stride is
still an open specification question; there is no visible proof from the current
indexed triangle-only subset. Historical FL9_1 drivers remain unverified.

With strict hardware mode enabled, run the instance-stream checks with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 instance_stream -- --nocapture --test-threads=1
```

The filter passed three cases: two CPU arithmetic/range tests and one physical
RTX 3090 Ti test at requested and selected FL9_1. The GPU case checked four
full-image executions across direct, strip, Uint16 and Uint32 window/gather
draws, with sparse mixed-rate streams, nonzero `first_instance`, zero-stride
tint, GPU copies and retained resources. It also checked short-range rejection.
In the Uint32 case, a red gather overlaps a green window at pixel (4, 7), while
the green gather does not cover it. The expected green pixel therefore
distinguishes instance-outer ordering from segment-outer ordering, which would
leave it red.
The controlled core fixture's instance-rate case produced 32 red and 32 green
pixels. Indexed triangle draws containing only instance-rate streams and shaders
using `instance_index` remain unsupported. These results do not establish general
instancing or historical FL9_1-driver behavior.

With strict hardware mode enabled, run the point-input execution test with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_point_list_vertex_inputs_and_pipeline_transition -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and selected FL9_1, the test checked three full
8×8 images. These cover sparse vertex slots, nonzero binding offsets and first
vertex, a point-to-triangle pipeline transition, and 65,537 points spanning
rebased batches with nonzero `first_instance` and an instance-rate stream. An
out-of-range draw is rejected during encoding. The controlled core `PointInput`
fixture checked all 64 pixels; its initial fixture buffer limit was adjusted from
one to two so it could create the intended layout. Before implementation the HAL
baseline failed because the pipeline rejected point inputs. Historical FL9_1
behavior has not been verified. Public adapter enumeration remains disabled.

With strict hardware mode enabled, run the indexed-point test and CPU planners:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 indexed_point -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti test requested and selected FL9_1 and checked 12 complete
8×8 images. Uint16 and Uint32 cases cover reordered and repeated indices, signed
base vertices, sparse vertex and instance streams, nonzero `first_instance`, a
recorded GPU index-buffer update, retained resources and whole-draw discard for
a late out-of-bounds fetch. Separate zero-stride and instance-only cases cover
extreme index values and negative effective indices without reading the index
contents; a short index range is rejected. The controlled core fixture checked
64 pixels each for Uint16 and Uint32. These cases exercise the private emulation,
not native indexed points, and do not establish historical-driver behavior.

With `$env:WGPU_DX11_REQUIRE_HARDWARE = "1"`, the focused sparse indexed test is:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_sparse_vertex_indexed -- --nocapture --test-threads=1
```

On the RTX 3090 Ti, all three focused tests passed with requested and selected
FL9_1. The high-Uint16 case also checks gather-to-native transition and additive
instances without rebinding vertex streams. The Uint16 test checks sparse slots 0 and 15, GPU-written indices, a
nonzero index binding offset, positive and negative base vertices, a nonzero
first index and two additive instances. A later out-of-bounds index discards
the complete draw; a subsequent valid draw and fence complete. The Uint32 test
uses GPU-written streams and indices to check ordered window and wide-triangle
gather rendering, then checks whole-draw discard after a late out-of-bounds
index. It verifies all 64 readback pixels in each target. These tests do not
establish historical-driver portability or general public indexed-draw support.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_direct_triangle_ -- --nocapture --test-threads=1` checks direct triangle
batch boundaries and order on CPU and validates pixels on both sides of a split
on physical hardware. Opaque output does not independently prove instance order.

With strict hardware mode enabled, run the overlap execution test and allocator
unit test with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 overlapping_vertex -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_slots_reserve_logical_streams_and_partition_overlaps -- --nocapture
```

On the RTX 3090 Ti forced to FL9_1, the GPU test checked all pixels for nine
executions: exact and partial overlaps across triangle lists, nonindexed strips
and indexed lists, a pipeline transition, and u32 window and wide-gather draws. The CPU test checks slot
allocation, logical-slot reservations and the selected-level distinction. The
controlled core fixture also rendered both disjoint and overlapping layouts,
each producing 32 red and 32 green pixels. These results do not establish
older-driver behavior, unsupported vertex formats or maximum-stride support.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
triangle_strip -- --nocapture --test-threads=1` checks the private nonindexed
strip path. The strict GPU test compares culling and winding with triangle lists,
exercises a split boundary, and checks a nonzero-offset pipeline transition.
The separate FXC diagnostic does not establish flat interpolation execution.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_checked_ -- --nocapture --test-threads=1` includes six original
runtime local-vector-index shaders. Each now requires an explicit fragment
linkage rejection and reports that GPU rendering was skipped. The separate
no-index multi-group control still renders and checks all 64 pixels. Earlier
runtime-index GPU output is historical and superseded by the guard. Matrix-store analysis tests use
`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
matrix_ -- --nocapture` and cover checked/unchecked stores, helper provenance,
shared type graphs and fail-closed work limits; this does not fix shared Naga.

With strict hardware mode enabled, run the constant-depth-bias test with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_constant_depth_bias_and_pipeline_transition -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and selected FL9_1, five full 8×8 D24S8
render/readback cases checked positive, negative and zero constant bias,
depth-write control and pipeline transitions. Two controlled core cases each
checked 64 pixels. Nonzero slope and clamp, biased point pipelines and selected
`frag_depth` outputs with nonzero bias were rejected. With zero bias, FXC
rejected a direct-only fragment-depth output with X4530; a structured
color-and-depth output compiled. Selecting a separate color entry from the same
module succeeded with nonzero bias. These results do not establish bias
arithmetic precision on historical FL9 hardware, nor a public adapter contract.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_read_only_depth_preserves_depth_and_rejects_writes -- --nocapture` checks
the private read-only D24S8 path. It validates depth preservation, rejects
write-enabled pipeline transitions and invalid pass operations, then exercises
READ→WRITE→READ across submissions after caller resource destruction. This
does not establish sampled-depth alias support or a public read-only flag.

For dynamic uniform offsets, run `cargo test -p wgpu-hal --lib
--no-default-features --features dx11 dynamic_uniform -- --nocapture` with
strict hardware mode enabled. These tests cover sampled and indexed draws,
offset changes, GPU-upload ordering and range rejection; they exercise the
private four-byte alignment, not a future public device's reported alignment.

The private HWND path can be checked with a hidden desktop window using
`cargo test -p wgpu-hal --lib --no-default-features --features dx11
fl91_hidden_hwnd_surface -- --nocapture`. With strict hardware mode enabled,
the test reports its physical adapter and actual feature level, reads back a
HAL clear, presents, resizes and recreates the swapchain. The additional strict
clone-sampling case can be selected with
`fl91_hidden_hwnd_clone_sampling_presents_and_resizes`. On the RTX 3090 Ti at
requested and actual FL9_1 it read BGRA `[0, 0, 255, 255]`, present returned
`Ok`, and resize succeeded while the completed command buffer remained retained.
A hidden-window test does not verify visible public presentation or concurrent
surface use. Synchronous window callbacks can still deadlock on reentry.

From an x64 Visual Studio developer shell, require physical hardware with
`$env:WGPU_DX11_REQUIRE_HARDWARE = "1"` in PowerShell, then run:

```console
cargo test -p wgpu-hal --lib --no-default-features --features dx11 dx11::hal_encoder::tests::fl9_1_hardware_indexed_triangle -- --exact --nocapture
```

The test prints every DXGI adapter and selects the first non-software adapter.
It requests only FL9_1 and asserts the returned level. It writes vertex and
16-bit index data to an upload buffer, records copies into initially zeroed
vertex and index buffers, then submits the copies before an indexed draw through
the private HAL queue. The test checks an 8×8 target before submission, waits
for a fence with a five-second timeout, and checks every pixel after readback.
Resources remain alive until completion and are explicitly destroyed afterwards.
The shader goes through the existing Naga and D3D11 shader pipeline.
The same test checks positive and negative base vertices with nonzero binding
offsets and a preceding GPU index upload; all 64 pixels must be red. A late
out-of-bounds index discards a whole draw; a later GPU index update and draw
still complete, producing black on the left and red on the right.
It also checks Uint32 conversion, including raw values 65535–65537 with base
vertex -3, high effective indices via rebased vertex offsets, ordered windows,
GPU gathering for a triangle spanning over 65534, and a GPU rewrite following
a discarded draw. A later out-of-bounds index discards the entire logical draw.

The strict depth-render test also checks zero, one and two direct instances and
an overlapping blend case across the native primitive-count boundary. The
expected 64-pixel outputs distinguish instance order from batch order.
The indexed test also covers u16 chunk and u32 window ordering across instances,
including nonzero first instance and whole-draw out-of-bounds handling. It
checks GPU-written u16 values that require gather or window translation and
verifies that a later native draw restores its vertex binding.

`empty_vertex_input_pipeline_is_accepted` checks zero-input triangle and
nonindexed point pipelines, including a single interior point and transitions
between point and triangle pipelines. CPU tests verify point batching at the
65,535-primitive boundary. Indexed point emulation is covered separately above.
It also checks masked alpha, inversion and vignette blends against readback.
FL9_1 does not guarantee separate alpha blending: the production path rejects
those modes with both RGB and alpha writes. Some older full-channel blend tests
explicitly opt into a test-only RTX diagnostic and do not establish portability;
they can fail on hardware that lacks separate alpha support.

Read the printed result, not just libtest's final line:

- `PASS`: the hardware device and all operations completed with the expected pixels.
- `SKIP`: no non-software adapter was enumerated; no GPU test was executed.
  Libtest still reports this case as `ok`, so retain `--nocapture` and inspect
  the result when collecting evidence. With `WGPU_DX11_REQUIRE_HARDWARE=1`,
  absence of a hardware adapter is a failing test instead.
- `FAIL`: enumeration, device creation, execution, synchronization, or
  validation failed. The test exits unsuccessfully. An enumeration failure is
  reported as a failure because its cause cannot safely be inferred to be a
  missing prerequisite. A process or loader failure before the test starts
  also cannot produce a test summary and must be investigated separately.

The test reports the adapter name, PCI IDs, software flag, requested and actual
feature levels, feature-level guarantees, targeted driver queries, backend
implementation status, readback and one-case counts. It does not query driver versions;
record them separately on Windows, for example:

```powershell
Get-CimInstance Win32_VideoController | Select-Object Name,PNPDeviceID,DriverVersion,DriverDate
```

If several physical adapters are present, the first DXGI hardware adapter is
used; check its printed identity before treating a result as evidence for a
particular GPU. No WARP or higher-feature-level fallback is attempted. This
is a private HAL test, not a public wgpu or WebGPU conformance test. It does
not prove leak freedom; a debug-layer lifetime check remains future work.

### Recorded FL9_1 run

On 2026-11-02, Windows 11 Pro build 26300, Rust/Cargo 1.93.1 and 1.99.1,
and NVIDIA driver 32.0.16.2101, the focused test selected the NVIDIA GeForce
RTX 3090 Ti (PCI vendor `0x10de`, device `0x2203`, software `false`). The only requested
and returned feature level was 9_1. The ordered buffer copies, indexed draws,
fence wait and readback executed: all 64 pixels were blue before submission
and red after completion. The focused result was 1 executed, 1 passed, 0
failed, 0 skipped. The Microsoft Basic Render Driver was enumerated as software
and was not selected. No D3D11 debug layer was enabled.

Before the change, the DX11 unit-test command below reported 64 passed and 0
failed. Those tests did not report their hardware execution individually; the
native diagnostic separately confirmed that the RTX could create FL9_1. A
successful suite total therefore should not be interpreted as 64 proven GPU
tests. CPU-only cases include feature-level ordering, queue error mapping and
the no-adapter guard; other cases exercise native GPU paths when an adapter is
available. The same suite reported 64 passed and 0 failed after the change.
The DX11-disabled DX12 GPU tests for mapped-buffer copy/readback and capability
validation each passed on the RTX 3090 Ti. Repeat the focused command to
establish fresh hardware evidence.

### Sparse texture and sampler bindings

With `WGPU_DX11_REQUIRE_HARDWARE=1`, run:

```console
cargo test -p wgpu-hal --lib --no-default-features --features dx11 dx11::hal_encoder::textured_tests -- --test-threads=1 --nocapture
```

This includes sparse bindings 7 and 42 in either numerical order, reversed WGSL
declarations and layout entries, uniform coexistence, and a red-to-green texture
binding transition.
Each checks GPU readback at FL9_1. No-adapter runs fail in strict mode.

### Broader suite

On a Windows machine with the DX11 feature enabled, run:

```console
cargo test -p wgpu-hal --lib --no-default-features --features dx11 dx11::
cargo clippy -p wgpu-hal --lib --no-default-features --features dx11 --no-deps -- -D warnings
```

The private HAL tests cover buffer and texture transfers, mapped shadows,
resource ownership and bounds rejection, fences, shaders, binding layouts,
rendering, and pixel readback. The original WGSL fixture in
`wgpu-hal/tests/dx11/fixtures/` exercises a checked two-texture overlay;
the opt-in FL9 texel-load lowering tests are in `wgpu-hal/tests/dx11/` and
are compiled as unit modules because they access private backend internals.
They do not enable the experimental lowering in the HAL.

Some tests return successfully when no hardware adapter exists. Record the
adapter name, actual device feature level, test count and readback result, and
verify that the hardware test paths ran; a successful test command alone is
not evidence of a GPU execution. Cross-compile the HAL and strict UWP host for
each target separately, and run the CoreWindow surface/readback probe on real
UWP devices. A compile or package is not a runtime result.
Before upstreaming, move the hidden diagnostic-only surface helpers out of the
public HAL surface API or replace them with a test-only seam; keep the normal
backend free of application-specific fixtures and window-host assumptions.

## 2. wgpu-core and public API integration

A safe core integration test is deferred: the HAL `Adapter` is uninhabited,
its normal instance intentionally exposes no adapters, and core's shader module
path treats the DX11 backend as unreachable. A core device also needs truthful
capabilities and validation for its accepted operations; restrictions such as
specific binding layouts cannot be represented by limits alone. A test-only
bootstrap would require a separately contained adapter and validation contract,
not just an enumeration switch. Do not use placeholder limits to bypass it.

A future tightly scoped hardware integration test should cover actual
wgpu-core device and queue initialization, empty and nonempty layouts, mapped
upload/copy/readback, checked shaders, rendering, errors and destruction. An
isolated local diagnostic has exercised device startup, mapped copying and an
indexed triangle via public wgpu with deliberately unsafe placeholder limits;
it is not an adapter and is not sufficient for release. The future test must
be safely contained and must not advertise those placeholders. Test device
loss, lifetimes, synchronization, initialization, and surface resize and
presentation, including diagnostics from the validation layers.

## 3. Upstream wgpu GPU tests

Once a DX11 adapter can truthfully describe and fulfill the required wgpu
contract, run the upstream suite:

```console
cargo xtask test --test wgpu-gpu
```

Use the GPU-test harness's backend selection (for example `WGPU_BACKEND=dx11`)
and verify that its adapter report actually selects this fork's DX11 adapter.
Record passed, failed, skipped and unrun tests separately. A zero-adapter run
or a skipped test is not a pass. Keep other backend builds and tests working.

## 4. WebGPU CTS

Only after the public adapter and test-runner integration are ready, start with
selected CTS cases and then run the pinned full suite:

```console
cargo xtask cts --backend dx11 'webgpu:api,operation,command_buffer,basic:*'
cargo xtask cts --backend dx11
```

`--backend` selects the CTS backend and expectation handling; it does not
make an unavailable adapter usable. At present `deno_webgpu/Cargo.toml` does
not enable the DX11 core feature on Windows, so wire and verify the local CTS
runner before using these commands for DX11. Confirm the adapter and backend
used in the run. Preserve the CTS revision, selected cases, outcomes, skips,
platform and driver details. Do not claim conformance from a selected subset.

## Hardware and validation matrix

For each applicable phase, exercise native D3D feature levels 9_1, 9_3 and
11_0 on suitable hardware or explicitly forced supported levels, including
x64 Windows/UWP, ARM64 UWP and ARM32 Windows 10 Mobile where available.
Record the native level actually created; an ANGLE-reported level does not
by itself establish the native device's maximum. Include texture-size and
NPOT restrictions, 16-bit indexed drawing and FL9 draw limits in level-9
coverage. Where a target cannot run a phase, record it as untested with the
reason rather than extrapolating from another machine.

Add an opt-in D3D11 debug-layer device mode and run it where the Windows
Graphics Tools/debug layer is installed. Capture and investigate all debug
messages, including binding hazards, descriptor errors, lifetime leaks and
synchronization problems. The current normal device-creation path does not
enable the debug layer. Installing missing system components may require
administrator access; do not silently substitute a non-debug run.

### Multiple sampled resources

Set `WGPU_DX11_REQUIRE_HARDWARE=1` and run:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 multi_resource -- --nocapture --test-threads=1
```

The cases check three sparse textures, distinct and shared samplers, a texture
using two sampler states, changing one texture, a pipeline transition and eight
textures with a shared sampler or eight samplers, and sixteen distinct sampler
relationships. Each reads back GPU output. The `level9_fxc`
unit test separately checks the FXC boundary of sixteen distinct sampled
texture/sampler relationships versus seventeen; it is not a hardware test.

### Shader-aware NPOT compatibility

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampling_relationships -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 shader_pairs_ -- --nocapture --test-threads=1
```

The first command uses synthetic capabilities and validated WGSL entry-point
analysis. The second executes FL9_1 GPU readback; three cases inject a restrictive
capability into the private test pipeline while leaving the driver unchanged.
They also reject invalid draw, indexed draw and pipeline/bind-group transitions
before submission. The unrestricted NPOT/wrap case requires a positive device
query, and fails in strict mode if that prerequisite is missing.

### FL9_1 sampler translation and patterned sampling

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampler_translation -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 equal_isotropic_filters_normalize_lod_only_for_single_mip_views -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampler_state_ -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_single_mip_same_filter_finite_lod -- --nocapture --test-threads=1
```

The first two commands are CPU-only. The sampler-state test executes patterned texture readback
for minification, magnification, mixed filters, addressing and a sampler
transition. It also runs a native border-creation diagnostic; that diagnostic
is not a rendered-pixel test. Anisotropy 2 is sampled, but a single-mip image
does not establish visual anisotropic quality.

The same-filter single-mip LOD test passed on the RTX 3090 Ti at requested and
selected FL9_1. It checked 32 full patterned images for nearest and linear
min/mag filters, minification and magnification, comparing ranges `0..0`,
`0.5..0.5` and `1..4` with an unrestricted reference, using both nearest and
linear mipmap filtering. The controlled core fixture also matched all 64 pixels for a nearest
sampler with `0.5..0.5`. Mixed-filter and anisotropic ranges that cannot be
faithfully translated remain rejected. Mipmapped sampling checks the actual view
against the sampler translation; explicit-LOD sampling and historical FL9_1 drivers
remain unverified.

With strict hardware mode, the following filter runs CPU analysis and physical
RTX 3090 Ti FL9_1 tests. A bounded-bias mip test passed nine executions with
64 checked pixels each, covering literal biases -3, -1, -0.5, 0, 0.5, 1 and 3,
varying derivatives, a minimum-LOD clamp and a fractional linear-mipmap blend.
Negative linkage tests cover
dynamic, out-of-range, helper-argument and offset biases; draw validation
rejects incompatible anisotropic or mixed-filter sampling pairs for both
direct and indexed draws.

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 bias -- --nocapture --test-threads=1
```

The tested FXC emits an unclamped `texldb`; Microsoft's [instruction
reference](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/texldb---ps)
defines ps_2_0 bias values outside [-3, 3] as undefined. The backend currently
uses FL9_1 shader profiles even on higher-level devices. It accepts finite
literal biases in [-3, 3] without an offset, while requiring equal min/mag
filters and anisotropy 1 for every sampling pair used by the selected entry.
View and LOD compatibility checks still apply; general biased sampling and
historical-driver behavior remain unverified.

### FL9_1 texture transfers

With strict hardware mode enabled, `cargo test --offline -p wgpu-hal --lib
--no-default-features --features dx11 bgra -- --nocapture` checks BGRA raw
upload/copy/readback, sampling channel order, and render-to-partial-copy with
offsets, row padding and untouched initialized texels. A wrapped-backbuffer
unit test also matches this filter. The ordinary BGRA paths still require their
own positive driver format-support query.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_texture_transfer_chain -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_copy_then_sample -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_buffer_upload_then_sample -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 transfer_layout -- --nocapture
```

The first test checks zeroed resources, padded and offset uploads and readback,
partial and full texture copies, resource retention and ordered submissions.
The second clears a render target, copies it and samples the destination.
The third samples a padded upload with a nonzero source offset. FL9_x readback
rejects native diagnostic textures bound only as shader resources.
The layout test is CPU-only. Direct private HAL calls reject empty transfer
regions; wgpu-core filters validated empty copies before dispatching to HAL.

The private `private_device_creates_restricted_texture_views` test also passed
on the RTX FL9_1 device with a metadata-only copy texture view and rejected
sampling, attachment use and invalid descriptors. The controlled core fixture
verified 12 upload/copy/readback pixels each for RGBA8 and BGRA8 copy-only views
and rejected incompatible view usages. Neither result establishes historical
driver behavior or general public adapter support.

### Copy-only POT mip transfers

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_copy_only_mip_transfers_and_views -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_copy_only_mips_start_zeroed_and_have_metadata_views -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 metadata_texture_view_ranges_check_bounds_and_aspect -- --nocapture
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The first two tests execute GPU work; the third is CPU-only. Strict execution on
the RTX 3090 Ti at requested and actual FL9_1 verified RGBA8 and BGRA8 four-level
initialization, padded partial uploads, copies between mip levels of different
textures, readback, metadata views and resource retention. The core fixture
verified all 43 pixels in its RGBA8 mip chain. Each format requires a positive
`CheckFormatSupport` result including `D3D11_FORMAT_SUPPORT_MIP` and the other
required texture, sampling and render-target bits. Missing format support is a
SKIP in normal mode and a failure in strict mode for these GPU tests. The
initialization and transfer tests also skip failed format queries in normal mode
and fail in strict mode; the core fixture propagates query errors as failures in
either mode.

These tests do not exercise sampled or attachment mip views. Native FL9 shader-resource
views must include the smallest resource mip; NPOT mip chains, self-copies and
general mip sampling remain unsupported. Historical FL9 GPU behavior is
unverified.

### Sampled POT mip chains

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampled_mip -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict private test executed 68 GPU cases without skips on the RTX 3090 Ti
at requested and actual FL9_1. RGBA8 and BGRA8 readbacks cover selected mip
levels, nearest and linear mip filtering, fractional LOD and used versus unused
sampler relationships. For each format, a same-pass pipeline and bind-group
transition retained sampled textures, views, groups and samplers after the caller
destroyed them before submission, and readback matched 32 green and 32 blue
pixels. The controlled core fixture sampled the blue mip of a
2x1 texture and verified all 64 output pixels. Supported chains have one layer,
POT dimensions, the restricted sampled usage combinations and positive format
support. Native suffix SRVs include the resource's last mip. Previously
verified results include suffix views for both formats. The controlled core
`SampledMipSuffix` case produced 64 blue pixels, verifying view-relative LOD.
A separate pair of core cases used `TEXTURE_BINDING | COPY_DST` without
`COPY_SRC`: an uploaded mip produced 64 blue pixels and an untouched mip
produced 64 transparent-zero pixels. These results check sampling via a separate
render target; the SRV-only native texture cannot be directly read back.
A sampler whose minimum LOD was normalized requires
a single mip; when a finite maximum was raised, a used multi-mip view requires
the original maximum to reach the last mip and anisotropy 1. NPOT mip chains
remain unsupported;
historical FL9 GPU behavior is unverified.

### Non-suffix sampled mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_nonsuffix_sampled_views_track_source_contents -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti run at requested and actual FL9_1 passed 16 GPU cases,
eight each for RGBA8 and BGRA8, with 64 checked pixels per case and no skips.
The cases exercise view-relative levels, high-LOD clamping to the selected range, two
simultaneous clones, native suffix and clone transitions, source updates after
group creation and between submissions, and retained resources after caller
destruction. A recorded buffer-to-texture upload followed by an indexed draw in
the same submission produced 64 blue pixels per format after the upload and
index buffers were destroyed before submission. A feedback test rejects sampling
an overlapping mip of the attachment. The controlled core fixture checked 64 yellow pixels after a
source update. The source needs no logical `COPY_SRC` usage.

Non-suffix sampled views use a GPU clone refreshed by copying the selected
source mips on each draw; the per-draw copies and additional allocation have a
performance cost. The original resource is retained for alias checks. Native
suffix views do not need this clone. Format prerequisites are queried; missing
support skips the applicable case in normal mode and fails strict mode.
Historical drivers remain unverified and ordinary public enumeration remains
disabled.

### Individual color-attachment mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_to_individual_mips -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict private test passed for both RGBA8 and BGRA8 on the RTX 3090 Ti at
requested and selected FL9_1, checking the full mip chain. The controlled core
fixture checked all 43 pixels for each format and submitted a render-only mip
pass, which has no direct readback. This path requires POT dimensions and
`COLOR_TARGET` with optional `RESOURCE`, `COPY_SRC` and/or `COPY_DST`;
`TRANSIENT` is not supported in the same chain. Each attachment view selects one mip.
Historical hardware remains unverified and ordinary public enumeration stays
disabled.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_and_sample_mip_transitions -- --nocapture --test-threads=1
```

The strict RTX FL9_1 run passed six GPU cases across RGBA8 and BGRA8, including
render/sample state transitions. The controlled core fixture passed four cases
with 64 checked pixels each: suffix sampled views and last-mip dual-use views
for both formats. Native sampled suffix views include the resource's last mip;
non-suffix sampled views use clones. Attachment views and dual-use views select
one mip. Use in distinct passes is supported. Same-pass use is supported for
disjoint native suffix and clone-backed SRVs and attachment mips. Historical
devices and public adapter exposure remain unresolved.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_dual_nonsuffix_mip_render_and_sample -- --nocapture --test-threads=1
```

The strict RTX FL9_1 test passed eight GPU cases across RGBA8 and BGRA8. A
nonfinal dual-use view has an SRV backed by a per-draw clone and an RTV backed
by the original mip. The controlled core fixture also used a default dual-usage
view to render and then sample a nonfinal mip in separate passes, checking 64
pixels for each format. Same-pass clone-backed views of the attachment resource
are supported when their mip ranges are disjoint; historical driver behavior
remains unverified.

### Same-resource disjoint native mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_native_suffix_disjoint_attachment_mips -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

Six strict GPU cases passed on the RTX 3090 Ti at requested and selected FL9_1
across RGBA8 and BGRA8. Controlled core cases sampled a disjoint suffix while
rendering to mip 0 of the same texture and verified 64 pixels per format.
Overlapping full-range SRVs are rejected. Disjoint clone-backed views are
covered by the separate test below.
[D3D11.3 §5.3.1](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)
permits input and output views of disjoint subresources. The separate
[D3D9 sampler documentation](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-registers-sampler)
warns against simultaneously binding a texture for sampling and rendering;
it does not prove D3D11 behavior on historical FL9 drivers. Those drivers remain
unverified, and normal public D3D11 adapter enumeration remains disabled.

### Same-resource disjoint clone-backed mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_clone_disjoint_mips_preserve_depth -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti run at requested and selected FL9_1 passed two GPU cases,
one each for RGBA8 and BGRA8. Each checked 32 green and 32 blue pixels after
two same-pass draws with different clone-backed mip views and a depth test. The
controlled core fixture verified the same output for both formats. The tests
reject an overlapping mip and retain resources through submission.

Before each clone refresh, replay snapshots and unbinds the current RTV and DSV,
clears pixel shader resource slots, copies the selected mips, restores the
attachments and rebinds the pixel shader resources. This adds attachment
transitions to the existing per-draw copy cost. Historical FL9 drivers remain
unverified; ordinary public adapter enumeration remains disabled.

### FL9_1 uniform bindings

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sparse_uniform_allocation -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 level9_constant_budgets_and_multiple_buffers_compile -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_sparse_uniform_ranges -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_two_uniforms_with_sampled_textures -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_multiple_groups_keep_dynamic_ranges_and_layouts_independent -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_forked_uniform_bindings_keep_dynamic_ranges_independent -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 hal_uniform -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_short_uniform_dynamic_copy_and_odd_source_end -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 integer_uniform -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The first two are CPU/compiler tests. The hardware tests verify sparse static
ranges, GPU-upload ordering, vertex/fragment use, padded and nested matrix
structures, array stride, pipeline and group transitions, and coexistence with
sampled textures. The multi-group test binds one group at two indices with
different dynamic offsets, switches pipeline register plans, tests layout
prefix invalidation and checks all pixels after direct and indexed draws.
The scratch test reads back both distinct GPU constant-buffer snapshots.

On the RTX 3090 Ti at requested and selected FL9_1, the `hal_uniform` filter
passed four cases, including a hardware test of exact GPU-copied 4-, 5-, 12-
and 20-byte ranges, a range at the source-buffer end, and zero native padding.
The controlled core fixture passed short bindings of those sizes, including a
scalar with a dynamic offset, with 64 checked pixels per render. Its synthetic
downlevel flags enable `BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED`; the initial core
test without that flag rejected the shader as `UnalignedShader`. This is a
fixture setting, not public adapter exposure. Binding materialization uses CPU
scratch and a mapped staging buffer, which can stall. An odd-sized native copy
is not verified on historical FL9_1 drivers.
The additional strict `fl91_short_uniform_dynamic_copy_and_odd_source_end`
GPU test passed on the RTX 3090 Ti at requested and selected FL9_1. It verified
the exact bytes and zero padding for a 5-byte dynamic range after a GPU copy
and for a 5-byte range ending exactly at the end of a 65-byte native source.

The `integer_uniform` hardware filter now checks rejection of three original
integer entry points: vertex and fragment uniform bitcasts and a fragment
comparison around 2^24. Each test instead renders a float-only control and checks
64 pixels. Previous RTX output for the integer shaders is historical and
superseded. An FXC probe found that the embedded legacy pixel shader rounded
16777217 to 16777216, while a legacy vertex shader lowered a bitcast to numeric
float arithmetic. Forced FL9 execution on the RTX does not establish historical
driver integer behavior.

The selected FL9 shader profile guard applies to checked and unchecked modes.
It rejects runtime integer values, operations, conversions and bitcasts,
integer-containing uniforms or interfaces, switches, and non-folded global
initializers and overrides. Proven static literal indices and float-only shader
paths remain available; analysis limits fail closed. These restrictions do not
establish general integer shader support or enable public adapter exposure.

### FL9_1 depth attachments

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test -- --nocapture --test-threads=1
```

The test samples a texture while drawing overlapping geometry in both depth
orders, verifies initial zero depth, checks independent depth-only and
stencil-only clears, and checks an Always/no-write pipeline. Ten 8x8 readbacks
are compared in full. It also checks attachment extent and pipeline mismatch
rejection. The private HAL also supports restricted read-only attachment
passes and no-op stencil configurations; aspect views and nontrivial stencil
tests remain unavailable.
Additional passes cover all color write masks, face culling, sampled textures
and samplers in separate groups, sampler and pipeline transitions, alias
rejection, and coexistence with a uniform in another group.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_never_preserves_color_depth_and_validation -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict RTX FL9_1 test passed both Never depth-write variants, each checking
32 green and 32 blue pixels; two controlled core variants each checked 64
depth-dependent pixels. Never draws are suppressed after validation, within the
current inert stencil, read-only shader resource and no-query subset. Native
state uses Always with depth writes disabled defensively. Native FL9_1 guarantees
cover Always and LessEqual; other native comparisons remain unsupported here.
Future shader side effects, writing stencil or queries require reviewing this
emulation. Historical hardware remains unverified.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 stencil_noop -- --nocapture --test-threads=1
```

This filter passed two cases: one synthetic classifier test and one physical
RTX 3090 Ti test at requested and selected FL9_1. The GPU test compares an
Equal, zero-read-mask, Keep state with ignored stencil across two passes and
reference changes using full color readback. The controlled core fixture also
draws 32 red and 32 green pixels with Equal, `read_mask=0x100`, Replace,
zero write mask and reference `0xa5`; only the low eight stencil mask bits
are effective. Actual stencil bits cannot yet be read back, and nontrivial
masking and writes remain unsupported. These tests do not establish behavior on
historical FL9_1 hardware.

### FL9_1 default buffer bind flags

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 level9_default_buffer_bind_flags -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_default_copy_and_uniform_buffers -- --nocapture --test-threads=1
```

The first test is synthetic. The second checks native descriptors and a
copy/readback on a hardware device explicitly created at FL9_1.

### Controlled core integration

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture
```

The unsafe, nondefault fixture injects one explicitly selected hardware FL9_1
device into core, without enabling ordinary adapter enumeration. The test uses
core buffer upload/copy and a WGSL offscreen triangle, submission, fence wait,
mapped readback and explicit teardown. It checks all 64 copied bytes and all
64 rendered pixels, including the 32-red/32-green render. It also validates
unsupported transient formats and usages and creates transient textures and
views; it does not execute a transient render pass through core. Its restricted
numeric limits and format capabilities do not constitute a safe general adapter
contract.
