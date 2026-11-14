# D3D11 FL9_1 CTS inventory

This is a small, curated inventory for the opt-in D3D11 FL9_1 projection. It
records selected CTS evidence and safe selectors to run next. It is not a full
CTS pass list, a WebGPU conformance claim, or evidence for historical FL9_1
drivers. The shared `cts_runner/test.lst`, `fail.lst`, and `skip.lst` are
backend-neutral expectations and do not provide a DX11-specific result
inventory.

## Strict hardware command

Run from a Visual Studio developer shell with the repository-pinned Rust
toolchain. Strict mode requires physical hardware and explicitly requests and
checks FL9_1; it must not silently substitute WARP or count a hardware skip as
a pass.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware '<selector>'
```

Keep the runner's per-selector counts and printed adapter details. Record
passed, failed, skipped and unrun subcases separately. An optional capability
skip is not GPU execution evidence.

## Executed selectors

These exact selectors have recorded strict results on the physical NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, software=false,
with requested and actual feature level FL9_1. They cover selected operations
only.

| Selector | Recorded result | Coverage |
| --- | --- | --- |
| `webgpu:api,operation,rendering,basic:clear:*` | 1/1 passed, no skips or failures | RGBA8 clear, copy to buffer, and readback. |
| `webgpu:api,operation,rendering,basic:fullscreen_quad:*` | 1/1 passed, no skips or failures | Bufferless triangle, selected `vertex_index` lowering, and green pixel readback. |
| `webgpu:api,operation,rendering,color_target_state:color_write_mask,channel_work:*` | 16 cases passed, no skips or failures | All channel-write-mask combinations. |
| `webgpu:api,operation,rendering,color_target_state:blend_constant,setting:*` | 3 cases passed, no skips or failures | Constant-factor blending with pixel checks; does not cover separate-alpha equations. |
| `webgpu:api,operation,rendering,color_target_state:blend_constant,initial:*` | 1/1 passed, no skips | Initial blend-constant behavior. |
| `webgpu:api,operation,rendering,color_target_state:blend_constant,not_inherited:*` | 1/1 passed, no skips | Pass-to-pass blend-constant reset. |
| `webgpu:api,operation,command_buffer,clearBuffer:clear:*` | 1/1 passed, 50 subcases, no skips or failures | GPU clear, offsets/ranges, and readback. |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:single:*` | 2/2 passed, no skips or failures | Basic buffer copies. |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:copy_order:*` | 1/1 passed, no skips or failures | Ordered overlapping-range copies. |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:state_transitions:*` | 1/1 passed, no skips or failures | Ordered partial copies and readback. |
| `webgpu:api,operation,resource_init,buffer:copy_buffer_to_texture:*` | 1/1 passed, no skips or failures | Buffer-to-texture initialization, including source-offset subcases. |
| `webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rgba8unorm";dstFormat="rgba8unorm";dimension="2d"` | 1/1 passed, no skips or failures | Non-array RGBA8 2D copies, including generated mip/region cases. |
| `webgpu:api,operation,rendering,color_target_state:blending,formats:*` | 4 passed, 29 skipped, 6 failed | RGBA8/RG8/R8 linear blend paths passed. Failures are listed below: two sRGB targets hit the documented FL9_1 blend-semantics rejection; four other formats were not renderable in the selected-device projection, but the CTS preflight only uses its generic format table. |

Exact commands and result descriptions are also recorded in
[`TESTING.md`](TESTING.md#strict-public-cts-copy-subset) and
[`FL9_1_PROGRESS.md`](FL9_1_PROGRESS.md#strict-downlevel-cts-copy-coverage),
with the clear-buffer and fullscreen-quad runs under
[`strict CTS clear-buffer`](FL9_1_PROGRESS.md#strict-cts-clear-buffer-operation-and-boundary-results)
and [`fullscreen quad`](FL9_1_PROGRESS.md#strict-fullscreen-quad-cts-recheck-2026-11-11).
The `basic:clear` and buffer-copy state-transition commands are in
[`TESTING.md`](TESTING.md#strict-dx11-cts-selection). The color-mask and
blend-constant results are recorded under [`render-state CTS`](FL9_1_PROGRESS.md#public-render-state-cts-slice-2026-11-10)
and [`blend-constant CTS`](TESTING.md#strict-dynamic-blend-constant-cts-selector).

## Executed filtered depth selectors

The exact commands below ran on the physical RTX 3090 Ti at requested and
actual FL9_1. Each format's 24 compare-function/clear-value cases passed with
zero skips or failures. The test's Equal/NotEqual pipeline validation emitted
the existing invariant-position warning; it did not affect the CTS results.

```text
webgpu:api,operation,rendering,depth:depth_compare_func:format="depth16unorm";*
webgpu:api,operation,rendering,depth:depth_compare_func:format="depth24plus";*
webgpu:api,operation,rendering,depth:depth_compare_func:format="depth24plus-stencil8";*
```

The `;*` retains all remaining `depthCompare` and `depthClearValue` cases for
the selected format. The source parameterizes `depth_compare_func` over the
depth formats and comparison functions in
[`depth.spec.ts`](../../../cts/src/webgpu/api/operation/rendering/depth.spec.ts).
These filtered runs do not replace the broad depth-suite triage: the broad
selector still has unrelated unsupported-usage failures for D24S8 `COPY_DST`
and Depth32Float, plus skipped Depth32FloatStencil8 cases. See
[`TESTING.md`](TESTING.md#4-webgpu-cts) and
[`FL9_1_PROGRESS.md`](FL9_1_PROGRESS.md#strict-depth-cts-probe-2026-11-11)
for related coverage and the broad-suite triage.

## Known boundary results; do not count as passes

| Selector or selector family | Result and reason |
| --- | --- |
| `webgpu:api,operation,rendering,basic:*` | Recorded 1 pass, 2 failures. The failures request `Rgba8Uint` texture copy usage, which the selected FL9_1 device did not report and the projection does not expose. The RGBA8 selectors above are the supported transfer evidence. |
| `webgpu:api,operation,rendering,depth:depth_disabled:*` | Failed because its D24S8 fixture requests `COPY_DST`, which is outside this projection; this does not establish a depth-state execution failure. |
| `webgpu:api,operation,rendering,depth_bias:depth_bias_24bit_format:format="depth24plus";*` and `format="depth24plus-stencil8";*` | Each selector failed 3/3 because the CTS fixture requests `COPY_SRC` on the depth texture, which this downlevel projection rejects. The resulting black color and teardown validation errors do not establish a depth-bias failure. A separate strict public test uses render-attachment-only `Depth24Plus`, and verified zero, positive and negative constant bias over 64 readback pixels. |
| `webgpu:api,operation,rendering,draw:default_arguments:*` | Failed when its shader used runtime integer operations rejected by the selected FL9_1 shader subset. |
| `webgpu:api,operation,rendering,depth:*` | 130 cases: 72 passed, 24 skipped, 34 failed. Source triage accounts for every failure: eight cases create D24S8 with `COPY_DST` even though the fixture never copies to it; 26 request `Depth32Float`, unavailable in this FL9_1 projection. These are texture-creation rejections, not failures of supported depth rendering. Keep D24S8 `COPY_DST` gated: D3D11 9_x permits depth-stencil resources only as `CopySubresourceRegion` sources, and Core lacks an equivalent downlevel guard on every texture-to-texture destination path. |
| `webgpu:api,operation,rendering,color_target_state:blending,formats:*` | 4 passed, 29 skipped, 6 failed. The failures were `rgba8unorm-srgb` and `bgra8unorm-srgb` (Core rejects blending because FL9_1 sRGB blend semantics do not match wgpu), plus `r16float`, `rg16float`, `rgba16float`, and `rgb10a2unorm` (Core rejects them as non-renderable on the selected device). The CTS helper `skipIfTextureFormatNotUsableAsRenderAttachment` uses its generic format table and does not consult this backend's selected-device `getTextureFormatFeatures` projection, so those four unsupported device formats reach pipeline creation. They are not evidence of a valid Core request reaching HAL. The 29 skips include non-blendable integer formats and feature-gated formats. |
| `image_copy:origins_and_extents` | Not a valid isolated single-layer 2D transfer selector: it generates 1D/3D and array-layer cases. Filtering to RGBA8 and 2D still includes array layers, which FL9_1 does not expose. |
| `webgpu:api,operation,rendering,basic:large_draw:*` | The strict DX11 run selected the physical RTX 3090 Ti at FL9_1, but all 4 parameterizations were invalid for this adapter: the fixture creates a `GPUBufferUsage.INDIRECT` buffer unconditionally, including `indirect=false`, and Core correctly rejects that usage without `INDIRECT_EXECUTION`. The zero-pixel expectations in those poisoned runs are inconclusive for direct drawing. A separate public strict test covers the FL9_1 direct point/triangle batching boundary. |

The shader and array boundaries are documented in
[`CAPABILITIES.md`](CAPABILITIES.md#fl9_1-inventory-and-gaps). Ordinary 2D
texture arrays are a FL9_1 feature-level restriction; do not reinterpret their
rejection as a missing transfer implementation.

The `large_draw` attempt used `WGPU_DX11_REQUIRE_HARDWARE=1` and reached the
physical RTX 3090 Ti (`0x10de:0x2203`), requested and verified at FL9_1. Its
summary was 0 passed, 0 skipped and 4 failed. CTS source creates the indirect
buffer at `cts/src/webgpu/api/operation/rendering/basic.spec.ts` before the
`indirect` branch, while Core rejects `INDIRECT` usage for this downlevel
device in `wgpu-core/src/device/resource.rs`. The run therefore establishes
the intended capability rejection, not a direct-draw regression. The direct
boundary is exercised separately by
`public_dx11_splits_point_and_triangle_draws_at_fl91_limits`.

The exact commands were:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:CARGO_BUILD_JOBS = '1'
& 'E:\Temp\wgpu_mission_cargo.bat' xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,rendering,basic:large_draw:*'
& 'E:\Temp\wgpu_mission_cargo.bat' test --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_splits_point_and_triangle_draws_at_fl91_limits -- --nocapture --test-threads=1
```

## Scope of this inventory

This list supports a practical regression subset for the current projection.
It does not cover every advertised format, API validation case, sampler,
shader operation, render state, surface path, or driver. The strict public
`wgpu` integration tests and private HAL tests provide additional evidence,
but they are not CTS executions. Desktop forced-FL9_1 results do not verify
historical FL9_1 devices or Lumia/CoreWindow runtime behavior.

## Strict render-pass selector boundary — 2026-11-12

On the current worktree, the raw `webgpu:api,operation,render_pass,*`
selector ran in strict required-hardware mode and reported 3 passes, 41 skips
and 139 failed cases. The failures group as follows:

| Failing test family | Cases | Source-backed boundary |
| --- | ---: | --- |
| `storeOp,color_attachment_only` | 64 | Includes two array layers and two mip levels. The RGBA8 mip-0/layer-0 subcase ran; sibling layer/mip cases hit the Core `TEXTURE_2D_ARRAYS` or `DEPTH_TEXTURE_MULTI_MIP` gate. Other formats also request copy-source/render-target usage not exposed by the selected-device projection. |
| `resolve,render_pass_resolve` | 40 | Every fixture requests two or four color attachments (`resolve.spec.ts`); the adapter limit is one. This does not test the supported single-target resolve path. |
| `clear_value,stencil_clear_value` | 20 | Fixtures request `COPY_SRC` on `Stencil8` or `Depth24PlusStencil8`; the selected projection rejects those copy usages. |
| `storeOp,depth_stencil_attachment_only` | 6 | Mixes unsupported formats/usages with array-layer and depth-mip cases. |
| `storeOp,color_attachment_with_depth_stencil_attachment` | 4 | Uses `Depth32Float` with `COPY_SRC`, which the projection does not expose. |
| `storeOp,multiple_color_attachments` | 4 | Requests two through four targets; the adapter limit is one. |
| `transient_attachment,overlapping_transient_attachments` | 1 | Exercises multiple simultaneous attachments beyond the one-target limit. |

The aggregate failure count is not a supported-subset regression count: CTS
continues after scoped Core validation rejects unsupported subcases, then
reports their error-resource readbacks as mismatches. Core's D2-array gate is
in `wgpu-core/src/device/resource.rs`; the relevant fixture shapes are in
`cts/src/webgpu/api/operation/render_pass/storeOp.spec.ts`,
`resolve.spec.ts` and `clear_value.spec.ts`. No confirmed failure of a
projection-supported render-pass operation was found.

The following strict CTS selectors passed on the current worktree with no
skips or failures:

| Selector | Result |
| --- | ---: |
| `webgpu:api,operation,rendering,basic:clear:*` | 1/1 |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:state_transitions:*` | 1/1 |
| `webgpu:api,operation,rendering,color_target_state:color_write_mask,channel_work:*` | 16/16 |
| `webgpu:api,operation,resource_init,buffer:copy_buffer_to_texture:*` | 1/1 |
| `webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rgba8unorm";dstFormat="rgba8unorm";dimension="2d"` | 1/1 |

The color-only store-operation cases cannot be narrowed to their supported
one-mip, one-layer subcase with CTS query parameters: mip and layer are nested
`beginSubcases()` inside each format/store-operation test case. These exact
strict selectors were attempted on the RTX 3090 Ti at requested/actual FL9_1:

| Selector suffix | Result |
| --- | --- |
| `colorFormat="rgba8unorm";storeOperation="store"` | 0/1 CTS cases passed. The mip-0/layer-0 subcase ran and matched. The same case then exercised array layer 1 and mip 1; unsupported array/mip validation produced mismatched readback and a final `TEXTURE_2D_ARRAYS` validation exception. |
| `colorFormat="rgba8unorm";storeOperation="discard"` | 0/1 CTS cases passed. All four nested subcases ran without a reported texel mismatch, then the case failed at finalization on the unsupported `TEXTURE_2D_ARRAYS` validation error. |

These are not passing CTS selectors and must not be added to `test.lst`.
Existing strict public tests separately verify supported one-mip color
store/discard initialization behavior. Closing this CTS gap cleanly requires
upstream CTS support for selecting nested subcases or a test whose parameters
expose mip and layer at query level; widening the DX11 texture-array contract
to make the selector pass would be incorrect.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,render_pass,*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,rendering,basic:clear:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyBufferToBuffer:state_transitions:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,rendering,color_target_state:color_write_mask,channel_work:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,resource_init,buffer:copy_buffer_to_texture:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat=\"rgba8unorm\";dstFormat=\"rgba8unorm\";dimension=\"2d\"'
```

## Additional strict color-target selectors — 2026-11-13

The following focused selectors passed on the physical RTX 3090 Ti at
requested/actual FL9_1 with no skips, failures or warnings:

| Selector | Result |
| --- | ---: |
| `webgpu:api,operation,rendering,color_target_state:color_write_mask,blending_disabled:*` | 2/2 |
| `webgpu:api,operation,rendering,color_target_state:blending,clamping:format="rgba8unorm";*` | 8/8 |

The first covers both values of the `blending_disabled` parameter. The second
checks RGBA8 UNORM clamping; color and alpha use the same blend component, so
it does not cover the known separate-alpha portability issue.

## Additional strict texture-copy selector — 2026-11-14

The BGRA8 UNORM 2D-to-2D copy selector passed without skips or failures on the
physical RTX 3090 Ti with requested and actual FL9_1. The corresponding
`Rg16Snorm` selector was also attempted, but all subcases skipped because
`texture-formats-tier1` is not in this adapter's conservative projection; it
is not a passing selector and is not in the curated pass inventory.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="bgra8unorm";dstFormat="bgra8unorm";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rg16snorm";dstFormat="rg16snorm";dimension="2d"'
```

The strict BGRA8 run selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), requested and confirmed FL9_1, and reported 1 pass,
0 skips and 0 failures. The Rg16Snorm run selected the same adapter and
reported 0 passes, 1 skipped test case and 0 failures; each subcase required
the unexposed `texture-formats-tier1` feature.

An attempted narrowly filtered RGBA8 texture-zero-initialization selector
matched no cases. Its requested aspect, mip, sample-count, layer and related
parameters are nested subcases rather than query parameters; the coarser
selector would also execute unsupported shapes. No GPU work was run for that
attempt. This is a CTS selection limitation, not a passing test or an observed
backend failure. The CTS runner was built in a separate system-temp target
after the existing E: target ran low on free space; no existing artifacts or
repository files were removed or changed.

Exact selectors:

```text
webgpu:api,operation,rendering,color_target_state:color_write_mask,blending_disabled:*
webgpu:api,operation,rendering,color_target_state:blending,clamping:format="rgba8unorm";*
```

Additional strict selectors in the same RTX 3090 Ti session passed with zero
skips or failures:

| Selector | Result |
| --- | ---: |
| `webgpu:api,operation,render_pass,storeop2:storeOp_controls_whether_1x1_drawn_quad_is_stored:storeOp="store"` | 1/1 |
| `webgpu:api,operation,render_pass,storeop2:storeOp_controls_whether_1x1_drawn_quad_is_stored:storeOp="discard"` | 1/1 |
| `webgpu:api,operation,resource_init,buffer:partial_write_buffer:*` | 1/1 (3 offset subcases) |

The first two draw into a 1x1 R8Unorm render target and verify the stored or
discarded result through copy and CPU readback. The buffer case writes twelve
bytes into a 32-byte COPY_SRC|COPY_DST buffer at each of the three CTS offsets,
then copies and checks the surrounding zero-initialized bytes. Invocation:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:CARGO_BUILD_JOBS = '4'
$env:CARGO_TARGET_DIR = 'C:\Users\ben\AppData\Local\Temp\wgpu-dx11-cts-probe-target'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,render_pass,storeop2:storeOp_controls_whether_1x1_drawn_quad_is_stored:storeOp="store"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,render_pass,storeop2:storeOp_controls_whether_1x1_drawn_quad_is_stored:storeOp="discard"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,resource_init,buffer:partial_write_buffer:*'
```

All three used the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) at requested/actual FL9_1. The compiler emitted the current
Core `validation.rs:4012` unused `level` warning and the test runner emitted
its expected experimental-DX11 warning.

The actual commands used the Visual Studio wrapper at
`E:\Temp\wgpu_mission_cargo.bat`, which supplies the pinned Rust 1.93.1
toolchain and external target directory. A direct PowerShell attempt outside
that developer environment tried to relink `cts_runner` and failed because
`link.exe` was not on `PATH`; rerunning within the Visual Studio environment
passed. The parameterized copy selector also required escaped JSON quotes in
Windows PowerShell so the CTS query parser received the format strings.

## Focused primitive topology and vertex-state fixture — 2026-11-13

The direct-draw basic-topology selectors passed on the physical RTX 3090 Ti at
requested/actual FL9_1 with no skips or failures (5/5):

```text
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="point-list";indirect=false;primitiveRestart=false
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="line-list";indirect=false;primitiveRestart=false
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="line-strip";indirect=false;primitiveRestart=false
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="triangle-list";indirect=false;primitiveRestart=false
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="triangle-strip";indirect=false;primitiveRestart=false
```

The incomplete-list direct-draw cases also passed with no skips or failures
(3/3): line-list draw count 5 and triangle-list draw counts 4 and 5.

```text
webgpu:api,operation,render_pipeline,primitive_topology:unaligned_vertex_count:topology="line-list";indirect=false;*
webgpu:api,operation,render_pipeline,primitive_topology:unaligned_vertex_count:topology="triangle-list";indirect=false;*
```

Strip primitive restart also passed for the direct line-strip and
triangle-strip cases (2/2):

```text
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="line-strip";indirect=false;primitiveRestart=true
webgpu:api,operation,render_pipeline,primitive_topology:basic:topology="triangle-strip";indirect=false;primitiveRestart=true
```

The `vertex_state,correctness:setVertexBuffer_offset_and_attribute_offset`
selector was also probed with `format="float32x2"`, but is not usable as a
DX11 vertex-input result: the CTS helper hard-codes an `r32sint` color target
and readback texture. That format is not renderable in the selected adapter's
projection, so the run failed at finalization with `Format R32Sint is not
renderable` and the zero-filled output mismatched every generated check. This
does not establish a vertex buffer offset failure or pass. The full strict
public DX11 suite includes passing float vertex offset/stride GPU tests; use
those until CTS exposes a render-target format parameter for this fixture.

## Single-target pipeline output — 2026-11-13

The RGBA8 component-count subcase passed on the physical RTX 3090 Ti at
requested and actual FL9_1 with no skips or failures. It creates one RGBA8
color attachment, renders a triangle with a `vec4f` fragment output, and
checks the rendered pixels. This supplements the color-mask and blend tests
with ordinary public render-pipeline output validation; it does not test
multiple attachments or unsupported output formats.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:CARGO_BUILD_JOBS = '4'
$env:CARGO_TARGET_DIR = 'C:\Users\ben\AppData\Local\Temp\wgpu-dx11-cts-probe-target'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,render_pipeline,pipeline_output_targets:color,component_count:format="rgba8unorm"'
```

The runner reported NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver
`32.0.16.2101`, physical adapter, requested/actual FL9_1 and a verified
FL9_1 marker: 1/1 passed. The build emitted the existing unused `level`
warning at `wgpu-core/src/validation.rs:4012`; the runner also printed its
expected experimental-DX11 warning.

## Partial buffer map and render state tracking — 2026-11-13

Four additional narrow selectors passed in strict DX11 mode on the physical
RTX 3090 Ti at requested and verified FL9_1. They cover both map modes across
three buffer offsets, pipeline changes around vertex-buffer state, vertex
buffer rebinding, and index-buffer rebinding. There were five CTS cases total,
with no skips or failures.

```text
webgpu:api,operation,resource_init,buffer:map_partial_buffer:*
webgpu:api,operation,command_buffer,render,state_tracking:change_pipeline_before_and_after_vertex_buffer:*
webgpu:api,operation,command_buffer,render,state_tracking:set_vertex_buffer_without_changing_buffer:*
webgpu:api,operation,command_buffer,render,state_tracking:set_index_buffer_without_changing_buffer:*
```

The test runner identified NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`),
driver `32.0.16.2101`, and the physical adapter. Invocation used the pinned
Rust 1.93.1 toolchain, `WGPU_DX11_REQUIRE_HARDWARE=1`, four Cargo jobs, and the
system-temp target directory:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:CARGO_BUILD_JOBS = '4'
$env:CARGO_TARGET_DIR = 'C:\Users\ben\AppData\Local\Temp\wgpu-dx11-cts-probe-target'
$env:CARGO_NET_OFFLINE = 'true'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,resource_init,buffer:map_partial_buffer:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,render,state_tracking:change_pipeline_before_and_after_vertex_buffer:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,render,state_tracking:set_vertex_buffer_without_changing_buffer:*'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,render,state_tracking:set_index_buffer_without_changing_buffer:*'
```

The build reported the existing unused `level` warning in
`wgpu-core/src/validation.rs`; Visual Studio also printed its existing
`vError: Unknown error` startup banner, but all four invocations completed.
