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
| `webgpu:api,operation,render_pipeline,overrides:basic:*` | 2/2 passed, no skips or failures | Vertex/fragment WGSL overrides through asynchronous and synchronous pipeline creation, rendering, and output checks. |
| `webgpu:api,operation,render_pipeline,overrides:shared_shader_module:*` | 2/2 passed, no skips or failures | Reuses one vertex/fragment shader-module pair across separately specialized pipelines; verifies both rendered outputs for async and sync creation. |
| `webgpu:api,operation,render_pipeline,overrides:multi_entry_points:*` | 2/2 passed, no skips or failures | Stage-specific override constants for separate entry points in one module; verifies rendered output for async and sync creation. |
| `webgpu:api,operation,render_pipeline,pipeline_output_targets:color,component_count:format="bgra8unorm"` | 1/1 passed, no skips or failures | Four-component BGRA8 fragment output. |
| `webgpu:api,operation,render_pipeline,pipeline_output_targets:color,component_count:format="bgra8unorm-srgb"` | 1/1 passed, no skips or failures | Four-component BGRA8 sRGB fragment output. |
| `webgpu:api,operation,render_pipeline,pipeline_output_targets:color,component_count:format="rgba8unorm-srgb"` | 1/1 passed, no skips or failures | Four-component RGBA8 sRGB fragment output. |
| `webgpu:api,operation,rendering,color_target_state:color_write_mask,channel_work:*` | 16 cases passed, no skips or failures | All channel-write-mask combinations. |
| `webgpu:api,operation,rendering,color_target_state:blend_constant,setting:*` | 3 cases passed, no skips or failures | Constant-factor blending with pixel checks; does not cover separate-alpha equations. |
| `webgpu:api,operation,rendering,color_target_state:blend_constant,initial:*` | 1/1 passed, no skips | Initial blend-constant behavior. |
| `webgpu:api,operation,rendering,color_target_state:blend_constant,not_inherited:*` | 1/1 passed, no skips | Pass-to-pass blend-constant reset. |
| `webgpu:api,operation,command_buffer,clearBuffer:clear:*` | 1/1 passed, 50 subcases, no skips or failures | GPU clear, offsets/ranges, and readback. |
| `webgpu:api,operation,buffers,map:mapAsync,read,*` | 18/18 passed, no skips or failures | `mapAsync` read ranges, including default/explicit/minimal range expansion and typed-array access. |
| `webgpu:api,operation,buffers,map:mapAsync,write,*` | 10/10 passed, no skips or failures | `mapAsync` write ranges and preservation of unchanged ranges across map/unmap. |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:single:*` | 2/2 passed, no skips or failures | Basic buffer copies. |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:copy_order:*` | 1/1 passed, no skips or failures | Ordered overlapping-range copies. |
| `webgpu:api,operation,command_buffer,copyBufferToBuffer:state_transitions:*` | 1/1 passed, no skips or failures | Ordered partial copies and readback. |
| `webgpu:api,operation,resource_init,buffer:copy_buffer_to_texture:*` | 1/1 passed, no skips or failures | Buffer-to-texture initialization, including source-offset subcases. |
| `webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rgba8unorm";dstFormat="rgba8unorm";dimension="2d"` | 1/1 passed, no skips or failures | Non-array RGBA8 2D copies, including generated mip/region cases. |
| `webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="r8unorm";dstFormat="r8unorm";dimension="2d"` | 1/1 passed, no skips or failures | Non-array R8 UNORM 2D texture copies, including generated zero-sized, partial, mip-level and unequal-resource-size cases. |
| `webgpu:api,operation,command_buffer,render,state_tracking:change_pipeline_before_and_after_vertex_buffer:*` | 1/1 passed, no skips or failures | Pipeline and vertex-buffer state transitions across point draws. |
| `webgpu:api,operation,command_buffer,render,state_tracking:set_index_buffer_before_non_indexed_draw:*` | 1/1 passed, no skips or failures | Indexed point draw followed by nonindexed draw in the same pass. |
| `webgpu:api,operation,command_buffer,render,state_tracking:set_index_buffer_without_changing_buffer:*` | 1/1 passed, no skips or failures | Index-buffer offset/range changes without rebinding the underlying buffer. |
| `webgpu:api,operation,command_buffer,render,state_tracking:set_vertex_buffer_without_changing_buffer:*` | 1/1 passed, no skips or failures | Vertex-buffer offset/size state changes without rebinding the underlying buffer. |
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

## Selectors skipped by the shared inventory

`image_copy:offsets_and_sizes:*` is skipped on DX11 by `cts_runner/test.lst`.
Its inner subcases include unsupported 1D textures and multi-layer 2D array
textures, and the CTS selector cannot isolate its generated copy-depth
subcases. [Microsoft's 10Level9 device reference](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
specifies a non-cube 2D texture array size of one at FL9_1; the backend separately exposes six-layer cube resources
only when their format is query-supported. A prior strict aggregate probe
selected the physical RTX 3090 Ti at requested/actual FL9_1, but failed on the
unsupported multi-layer cases. It did not report a content mismatch for the
one-layer upload cases, but it cannot count as a one-layer CTS pass. Focused
public and HAL transfer tests remain the evidence for supported offsets and
row layouts. This skip does not imply those focused cases failed or that array
textures are supported.

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
| `webgpu:api,operation,resource_init,texture_zero:uninitialized_texture_is_zero:dimension="2d";readMethod="CopyToBuffer";format="rgba8unorm"` | 0/1 CTS cases passed. The case expands internally to unsupported seven-layer shapes even with the 2D/RGBA8 query; Core rejects `depthOrArrayLayers=7` against the projected maximum of 6. All reported content mismatches were for those seven-layer cases. The supported one-layer, one-mip, POT, canary-free subcase had no mismatch, but the aggregate is not a CTS pass. A separate strict public test validates zero initialization after discard. |
| `webgpu:api,operation,command_buffer,image_copy:rowsPerImage_and_bytesPerRow:initMethod="CopyB2T";checkMethod="FullCopyT2B";format="rgba8unorm";dimension="2d"` | 0/1 CTS cases passed on the strict FL9_1 adapter. Its nested copy-size cases all create six-layer textures for D2; Core permits that shape only as a valid cube because `TEXTURE_2D_ARRAYS` is unavailable. Non-square and, where unsupported, NPOT cube cases are rejected. The nested copy-size parameters cannot be selected independently by CTS query, so the aggregate is not evidence of a transfer mismatch or pass. |
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

Six selected 2D-to-2D copy selectors passed without skips or failures on the
physical RTX 3090 Ti at requested and actual FL9_1: same-format BGRA8 UNORM,
plus linear-to-sRGB and sRGB-to-linear pairs for both RGBA8 and BGRA8. The
same-format R8 UNORM copy also passed. The copies preserve stored bits; they do
not perform color-space conversion. The
corresponding `Rg16Snorm` selector was also attempted, but all subcases skipped
because `texture-formats-tier1` is not in this adapter's conservative
projection; it is not a passing selector and is not in the curated pass
inventory.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="bgra8unorm";dstFormat="bgra8unorm";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="bgra8unorm";dstFormat="bgra8unorm-srgb";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="bgra8unorm-srgb";dstFormat="bgra8unorm";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="r8unorm";dstFormat="r8unorm";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rgba8unorm";dstFormat="rgba8unorm-srgb";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rgba8unorm-srgb";dstFormat="rgba8unorm";dimension="2d"'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rg16snorm";dstFormat="rg16snorm";dimension="2d"'
```

The strict runs selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), requested and confirmed FL9_1. Each of the six
supported selectors reported 1 pass, 0 skips and 0 failures (6/6 total).
The Rg16Snorm run selected the same adapter and reported 0 passes, 1 skipped
test case and 0 failures; each subcase required the unexposed
`texture-formats-tier1` feature.

The same six supported selectors passed 6/6 on both DX12 and Vulkan. Vulkan
logged that an optional OBS layer manifest was unavailable, but all selected
test cases completed without skips or failures.

The analogous 2D `Rg8Unorm` selector failed 0/1 with no skips on forced
FL9_1. The CTS case includes nested four-mip copies, while this adapter reports
`SINGLE_MIP_ONLY` for Rg8Unorm because its format query lacks MIP support. The
resulting mip-3 validation errors produce secondary readback mismatches; they
are outside the public projection and do not test the supported single-mip
copy path. The selector cannot isolate the no-mip nested subcase, so it was
not added to `cts_runner/test.lst`. Dedicated public tests do verify strict
single-mip RG8 upload, partial copy and readback.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rg8unorm";dstFormat="rg8unorm";dimension="2d"'
```

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

## Indexed to nonindexed draw state transition — 2026-11-14

The strict selector below passed 1/1 with no skips or failures on the physical
RTX 3090 Ti at explicitly requested and verified FL9_1. It performs an indexed
point draw followed by a nonindexed point draw in one render pass and validates
all four RGBA8 pixels. This covers the index-buffer state transition; it does
not broaden any advertised backend capability. The selector is recorded in the
shared passing CTS inventory.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,render,state_tracking:set_index_buffer_before_non_indexed_draw:*'
```

The runner reported NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver
`32.0.16.2101`, physical adapter, requested/actual FL9_1 and the verified
FL9_1 marker. The command used the repository wrapper's Visual Studio x64
environment and cached Deno runner; the existing `vError: Unknown error`
startup banner did not affect execution.

## Same-format sRGB texture copies — 2026-11-14

The two same-format sRGB 2D copy selectors passed 2/2 with zero skips or
failures on the physical NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) at requested and verified FL9_1. These complement the recorded
linear/sRGB cross-format copies and add no format or usage capability claims.
The selector list was passed via a temporary file to preserve embedded JSON
quotes through Windows argument parsing; that file was removed after the run.

```text
webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="rgba8unorm-srgb";dstFormat="rgba8unorm-srgb";dimension="2d"
webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,non_compressed,non_array:srcFormat="bgra8unorm-srgb";dstFormat="bgra8unorm-srgb";dimension="2d"
```

Compact strict invocation (`--print-output-when test-fails`) reported 1/1
passed, 0 skipped and 0 failed for each selector. The runner printed the
physical-adapter FL9_1 marker twice, once per selector. The existing Visual
Studio `vError: Unknown error` banner did not affect the run.

## Compressed BC copy selector is gated by the aggregate WebGPU feature — 2026-11-14

A strict attempt of the same-format BC1 copy selector selected the physical
NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and
actual FL9_1. The CTS reported 0 passed, 1 skipped and 0 failed; every nested
copy subcase was skipped because it requires the aggregate
`texture-compression-bc` feature. This adapter projection deliberately
withholds `Features::TEXTURE_COMPRESSION_BC`, which covers BC1 through BC7,
because the backend currently implements only BC1/2/3. D3D11-specific
BC1/2/3 UNORM and sRGB subset features have separate strict public GPU tests,
but this standard WebGPU CTS selector cannot exercise those partial features.

The first direct Windows invocation lost the embedded JSON string quotes and
failed in CTS query parsing before running a test; the corrected invocation
passed the selector through a temporary `-f` list file. That file was removed
after the run. BC2 and BC3 CTS selectors were not run because they use the same
aggregate feature gate. Do not count this probe as GPU execution or a passing
CTS case. The existing public wgpu tests remain the evidence for the private
BC1/2/3 subset.

For a reproducible attempt, put the exact selector below in a one-line ASCII
file and pass it with `-f`:

```text
webgpu:api,operation,command_buffer,copyTextureToTexture:color_textures,compressed,non_array:srcFormat="bc1-rgba-unorm";dstFormat="bc1-rgba-unorm";dimension="2d"
```

## 3D undefined-parameter copy selector requires unsupported NPOT volume — 2026-11-15

The strict selector
`image_copy:undefined_params:initMethod="WriteTexture";checkMethod="FullCopyT2B";dimension="3d"`
ran 0/1 passed, 0 skipped and 1 failed. Its CTS fixture hardcodes a 100-texel
3D width, then continues after texture-creation validation errors and reports
misleading readback mismatches. FL9_1 requires power-of-two dimensions for
`CreateTexture3D`; this backend truthfully projects `npot_texture_3d=false` and
Core rejects the fixture before HAL. Microsoft documents that 10Level9 rule in
the [CreateTexture3D restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).

The three 3D `undefined_params` selectors in `cts_runner/test.lst` are now
annotated `fails-if(dx11)` (the buffer-to-texture entry retains its existing
other-backend exclusions). This excludes only this invalid NPOT fixture from
DX11 CTS selection; it is not a pass and does not relax the implementation.
Dedicated strict public and HAL tests verify supported POT 2x2x2 upload, copy,
readback and sampling.

## Adapter request CTS selector requires compute — 2026-11-15

The shared `requestAdapter:*` selector's `requestAdapter` test creates a
storage-buffer compute pipeline and verifies its output for every adapter
request. Strict execution selected the physical RTX 3090 Ti at FL9_1, then
failed all 18 option combinations because Core correctly rejected the
unavailable `COMPUTE_SHADERS` capability; fallback requests also correctly
returned no software adapter. The selector therefore cannot validate adapter
selection independently of compute support and is marked `fails-if(dx11)`.
Separate strict backend tests continue to assert explicit DX11 adapter/device
selection at FL9_1, no WARP substitution, and unchanged default backend
selection.

## Strict shared-list CTS continuation — 2026-11-15

The strict default-list run passed `unittests:*` (1031/1031), the five
invalid-feature-level adapter cases (5/5), buffer map reads (18/18), buffer
map writes (10/10), empty command submission (1/1), buffer copies (4/4), and
nine selected RGBA8/BGRA8/R8 color-copy selectors (9/9). These ran on the
physical RTX 3090 Ti at explicitly requested and selected FL9_1.

The generic `createBindGroup:buffer_binding_resource` CTS case is a storage
buffer compute shader, so it is marked `fails-if(dx11)`. `command_buffer,basic`
is narrowed to its empty-submit case because the other two cases require
`rgba8uint` texture copy usage, which Core rejects for this projection. The
depth/stencil copy selectors request copy destinations, array layers, or
mipmapped depth resources unavailable in this FL9_1 path and are marked
`fails-if(dx11)`. The `image_copy:undefined_params` selectors cannot isolate
their internally generated subcases: the 1D case uses an unsupported
dimension, the 2D case includes multi-layer textures, and the 3D case uses a
non-power-of-two width. Those aggregate selectors are marked `fails-if(dx11)`;
focused strict transfer tests remain the evidence for supported single-layer
operations.

An exploratory suffix run over 102 later shared-list selectors reported 87
failures and 15 skips. Its failures include unsupported compute operations and
the newer `IMMEDIATES` feature; that count is not a backend regression score.
The remaining selectors need case-by-case classification before the shared
list can serve as a DX11 run plan. No full-suite CTS pass is claimed.

## Read-only depth/stencil selector requires fragment position — 2026-11-15

The focused `Depth16Unorm` `sampling_while_testing` selector executed both
read-only variants on the physical RTX 3090 Ti at requested/actual FL9_1 and
failed 0/2. Both test shaders use fragment `@builtin(position)` to derive the
depth/stencil texel coordinates. The selected backend projection sets
`DownlevelLimits::fragment_position` to false, and Core rejects that input
before a valid test draw can execute. The expected-buffer mismatches and the
test harness's follow-on validation-scope error are consequences of running a
test outside the advertised downlevel contract; they are not evidence against
the separately verified D16 comparison-sampling path.

All listed `readonly_depth_stencil:sampling_while_testing` selectors use the
same unsupported fragment input in their test/check entry points, including
stencil-only cases. They are now marked `fails-if(dx11)` in the shared test
list. This is an explicit unsupported-test classification, not a CTS pass.
Focused public tests continue to verify supported depth comparison sampling,
depth testing/writes and stencil operations independently.

## Broad render-pass selector mixes unsupported attachment shapes — 2026-11-15

The curated `render_pass,*` selector failed inside its `clear_value` and other
aggregate cases. Its stencil-clear test requires copy-source use of `Stencil8`
and depth/stencil formats; this adapter intentionally does not expose those
copy paths. Its resolve test unconditionally constructs 2- and 4-target
pipelines, above FL9_1's single-target limit. Store-op cases also combine
multiple color targets, multi-layer arrays, or multi-mip depth attachments,
which the adapter rejects through its downlevel contract. Those aggregate
cases cannot be narrowed to individual internal subcases through CTS selectors,
so the broad selector is marked `fails-if(dx11)` rather than treating its
partial execution as a pass.

The independent transient-attachment count case is still applicable: it uses
the adapter's single color target. Its strict RTX 3090 Ti FL9_1 run passed
1/1. That narrow selector remains in the curated list as positive operation
coverage.

The broad `rendering,depth:*` selector also mixes valid depth comparisons with
unsupported depth formats and setup operations. Its strict run reported
72/130 passing, 24 skipped and 34 failed; failures included Depth24PlusStencil8
copy initialization and Depth32Float render-target cases. The individual
`depth_compare_func` selectors restricted to `depth16unorm` and `depth24plus`
both passed all 24 cases on the RTX 3090 Ti at forced FL9_1, with zero skips or
failures. The curated list now retains those supported comparisons while
excluding the aggregate selector for DX11. These runs verify depth comparison
state on the selected modern driver, not historical FL9_1 devices.

The broad `rendering,draw:*` selector is also outside the exposed contract.
Its `arguments` and `default_arguments` shaders use runtime integer operations
in the vertex stage; `vertex_attributes,basic` writes to a storage buffer, and
the arguments matrix includes indirect draws. Core rejects these shader and
resource requirements under the FL9_1 projection. The aggregate selector
reported 0/788 passed, 182 skipped and 606 failed; this is not a direct or
indexed draw regression. The selector is now gated for DX11. Existing strict
public and private tests continue to exercise supported direct, indexed and
bounded large draws with actual GPU readback.

The `uncapturederror:iff_uncaptured` aggregate also mixes positive validation
events with an OOM probe that allocates `rgba32float` at the device's maximum
texture dimensions. That format is intentionally unavailable to this FL9_1
projection, so texture creation is rejected as a validation error before an
allocation can produce the requested OOM event. The two validation-event
variants passed; the aggregate is now gated and those exact variants remain
selected.

The shared `vertex_state,correctness` selectors are not valid format coverage
for this adapter: their test shaders write results to `R32Sint`, which is not
an exposed FL9_1 render target, and their parameter sets include vertex formats
outside the projected format mask. Core rejects those combinations before
they can test fetch/stride behavior. The three aggregate selectors are gated
for DX11; focused HAL and public tests cover the supported format mask and
buffer offsets.

The aggregate `validation,buffer,create:usage:*` selector also combines
otherwise-valid WebGPU usage masks with `INDIRECT` and index-buffer
combinations gated by `INDIRECT_EXECUTION` and `UNRESTRICTED_INDEX_BUFFER`.
Strict execution ran 78 cases: 65 passed and 13 were rejected by the selected
downlevel checks before HAL allocation. Since the selector does not isolate
only the supported combinations compactly, it is gated for DX11; focused
buffer tests continue to cover supported vertex, index, uniform, copy and map
usage combinations.

The `validation,buffer,destroy:all_usages` selector likewise includes an
indirect buffer, which requires the unadvertised `INDIRECT_EXECUTION`
downlevel flag. The error-buffer, repeated-destroy and mapped-buffer destroy
selectors remain selected independently so resource-destruction coverage is
preserved without treating unsupported indirect usage as a backend failure.

The `capability_checks,limits,maxBindGroups:setBindGroup` CTS selector is not
applicable to the strict DX11 test harness as written. It assumes core-default
limits (including four bind groups), while the strict downlevel Deno adapter
uses the adapter's reported limits as its device defaults; with this adapter,
`maxBindGroups` is eight and a lower requested value does not lower that
default. The selector also builds compute encoders, rejected by the expected
`COMPUTE_SHADERS` downlevel gate. The strict run reported 10/30 pass and 20/30
failures for these mixed assumptions. It is gated for DX11 pending a CTS
variant parameterized by the selected default feature level and render-only
execution. This does not gate ordinary bind-group count validation: the
focused public and Core tests cover the backend's advertised group limit.

The aggregate `capability_checks,limits,maxBufferSize` selector assumes a
Core-defaulting adapter whose supported maximum is at least the 256 MiB WebGPU
default. This experimental FL9_1 adapter deliberately reports a 128 MiB minus
four-byte backend ceiling after the physical driver failed an in-range
near-4-GiB allocation. Its exact `buffer,create:limit:*` selector passed below,
at and above the advertised boundary (3/3) on strict hardware. The core-default
aggregate is gated for DX11; it does not replace that selected-adapter boundary
test.

For `maxInterStageShaderVariables:createRenderPipeline`, the `atDefault`
selector passed its 104 supported cases with 272 cases skipped and no failures;
the skipped cases request multisampled shader built-ins that FL9_1 does not
expose. The `atMaximum` selector failed 104/376 because this adapter's
downlevel maximum is eight variables while the CTS assumes a core-defaulting
adapter's default of sixteen. That adapter/default mismatch is specific to this
experimental backend; `atMaximum` is gated for DX11, while `atDefault` and the
backend's targeted shader-interface tests remain selected.

All `validation,compute_pipeline:*` CTS cases construct a compute pipeline,
including control cases that require successful pipeline creation and
descriptor-negative cases whose shader-specific validation is preempted by
the intentionally absent `COMPUTE_SHADERS` downlevel flag. The family is
therefore gated for DX11. A dedicated Core fixture asserts compute-pipeline
creation is rejected at that exact flag boundary; the CTS family cannot
exercise shader-level compute validation on this adapter.

The `createBindGroup:bind_group_layout,device_mismatch` selector uses a
compute-visible uniform layout. The mismatched and control cases are internal
subcases of one CTS selector and cannot be split by the selector list. Core's
missing compute-stage flag preempts the whole test, so this selector is gated
for DX11. Cross-device resource validation remains covered by backend/Core
tests whose valid controls use FL9_1-supported stages.

`createBindGroup:binding_count_mismatch` builds compute-visible storage-buffer
layouts for all subcases. Core correctly rejects those layouts under the
missing compute-shader capability before it can validate the entry-count
matrix, so the selector is gated for DX11. Supported buffer bind-group
validation remains covered by private HAL and public tests.

`createBindGroup:binding_must_be_present_in_layout` likewise builds only a
compute-visible storage-buffer layout. Core rejects it before the intended
binding-number checks, so this selector is gated for DX11.

`createBindGroup:binding_must_contain_resource_defined_in_layout` uses
compute-visible entries for every resource type and includes storage textures
unsupported by this adapter. Its 143 cases all fail before the intended
resource-identity check, either at the compute-stage or storage-texture
capability gate, so the selector is gated for DX11.

`createBindGroup:binding_resources,device_mismatch` mixes storage buffers,
storage textures and compute-only visibility subcases with sampled resources.
Core rejects these unsupported layouts before reaching the intended device
identity checks; the strict CTS selector failed all six selected resource
types. It is gated for DX11, with supported-stage cross-device checks retained
in focused Core/public tests.

`createBindGroup:buffer_offset_and_size_for_bind_groups_match` uses only a
compute-visible storage-buffer layout. The Core compute gate fires for both
the valid and invalid range cases, so this selector cannot measure the buffer
range checks for the FL9_1 path. It is gated for DX11; uniform range/offset
validation remains covered in the public tests.

## Render-pipeline override and blend selectors — 2026-11-15

The broad render-pipeline override selector produced 6/8 passing cases; only
the two `precision` variants failed because the CTS renders to `rgba32float`,
which this FL9_1 projection does not advertise as a render target. The list now
keeps the passing `basic`, `shared_shader_module` and `multi_entry_points`
selectors and excludes only `precision` for DX11.

The `pipeline_output_targets:color,component_count,blend` CTS selector
deliberately assigns distinct color and alpha blend factors. Core correctly
rejects those pipelines because `SEPARATE_ALPHA_BLEND` is not exposed at
FL9_1; targeted public tests verify supported coupled blending and reject
distinct equations. The CTS selector is therefore marked `fails-if(dx11)`.

The broad `rendering,color_target_state:blending,formats` selector also mixed
four verified blendable linear formats with formats unavailable to this
adapter and sRGB blending whose required linear-space semantics are not
supported by FL9_1. The strict run passed `r8unorm`, `rg8unorm`, `rgba8unorm`
and `bgra8unorm`; it failed for `rgba8unorm-srgb`, `bgra8unorm-srgb`, the
16-bit float formats and `rgb10a2unorm`, which are either not render targets
or are not exposed as blendable under the selected FL9_1 contract. The broad
selector is now gated for DX11 and those four passing formats are retained as
individual selectors.

The CTS `blending,GPUBlendComponent` test hardcodes `rgba16float` so it can
avoid unorm clamping while checking every factor pair. This FL9_1 projection
does not expose that format as a render target, so every case is outside the
selected adapter's usable blend contract; the selector is marked
`fails-if(dx11)`. The separate supported-format blend selector above remains
the positive CTS coverage for linear FL9_1 targets.

The broad `createTexture:texture_usage` selector assumes core-guaranteed
format usages and does not derive its expected result from the selected
downlevel adapter's `TextureFormatFeatures`. It combines all formats,
dimensions and usage pairs, including storage bindings and 3D usages not
advertised at FL9_1. On the strict RTX 3090 Ti FL9_1 run, 146/330 outer
selectors failed on these projection differences and 172 were skipped for
unsupported formats. The selector is gated for DX11; supported format-usage
creation and rejection remain covered by the focused public projection test
and private capability tests. The separate `createTexture:usage` selector
continues to test invalid usage-bit combinations.

The `createTexture:transient_viewFormats` selector requires alternate view
formats, but this backend does not advertise `DownlevelFlags::VIEW_FORMATS`.
Both selected RGBA8 cases were rejected at that Core gate before HAL; the
selector is gated for DX11. Same-format views and the backend's explicit
rejection of reinterpretation remain covered by public tests.

The general `createTexture:usage` selector constructs an RGBA8 texture and
assumes every syntactically valid usage mask can be created. Its 36 cases
include six valid WebGPU masks containing `STORAGE_BINDING`, which FL9_1
does not expose. The broad selector ran 30/36 successfully and rejected those
six at the intended downlevel format-usage check. The list now keeps the 30
other exact usage-mask cases and marks only the six storage masks inapplicable
for DX11.

The broad `createTexture:viewFormats` family combines compatible-format
views with format pairs outside this adapter's projection. The current
FL9_1 contract does not expose alternate view-format reinterpretation, and
the strict run's sole non-skipped case requested unsupported R8Snorm
sampling (0/36 passed, 35 skipped, 1 failed). The family is gated for DX11;
focused public tests cover same-format views and predictable rejection of
reinterpretation.

The aggregate `createTexture:zero_size_and_usage` mixes the valid zero-size
validation checks with 1D textures and formats/usages not exposed by this
adapter. Its strict run passed five supported outer cases, skipped the three
BC1 cases because the public adapter does not request the compression feature,
and failed five cases preempted by unsupported 1D or RGB10A2 support. The list
keeps only the five passing RGBA8/D24S8 cases and three recognized BC1 skips.

The broad `createView:array_layers` selector also mixes unsupported texture
creation shapes with view-range validation: D1 textures are unavailable, D2
uses nested layer counts 1, 6 and 18 (ordinary arrays are not exposed; six
layers are supported only through the separately tested cube path), and the
test does not isolate those nested cases. Its 3D default and explicit-3D
selectors passed on strict FL9_1 hardware and remain selected; the other seven
outer selectors are gated for DX11. This is a CTS granularity limitation, not
evidence for enabling 1D or ordinary arrays. Public tests continue to verify
cube-view success and ordinary-array rejection. The narrowed strict command
`cargo xtask cts --skip-checkout --backend dx11 --require-dx11-hardware --filter 'createView:array_layers' -f cts_runner/test.lst`
passed both retained 3D selectors with zero skips and failures on the physical
RTX 3090 Ti at requested and selected FL9_1.

The broad `createView:aspect` matrix requests `TEXTURE_BINDING` for every
texture format, although the selected FL9_1 projection does not permit that
usage for most formats. In the strict run, 33 aspect cases passed, 93 failed
before reaching their aspect check, and 177 were skipped by the CTS format
feature gate. The list now retains the exact 33 cases across R8/RG8/RGBA8/
BGRA8 and supported depth formats and marks only the 93 inapplicable format
cases for DX11. The narrowed physical-device run passed 33/33 with no skips
or failures; this checks validation behavior, not rendered GPU output.

The `createView:cube_faces_square` family is inapplicable as a whole: its 2D
control cases use six-layer resources and assume ordinary 2D-array view
support, while cube cases mix non-square and NPOT descriptors rejected during
texture creation before the intended view check. The CTS cases do not isolate
those creation errors. The selector is gated for DX11; strict public tests
separately verify supported cube views and NPOT rejection.

The broad `createView:dimension` matrix uses D1 resources (not exposed), a
six-layer D2 texture (ordinary arrays are not exposed, and its 2D/cube cases
share one selector family), and a 3D texture with depth 6 (rejected by the
FL9_1 power-of-two volume rule). The CTS reference does not isolate those
resource-creation prerequisites from view-dimension validation. Its full 21
outer cases failed on setup or left error scopes after the setup errors; the
selector is gated for DX11. Public tests separately cover the supported cube
view and 3D view paths and reject unavailable dimensions.

The broad `createView:format` matrix requires testing all format-pair
reinterpretations, including `viewFormats` lists. FL9_1 does not advertise
`VIEW_FORMATS`, and the CTS subcases combine supported same-format views with
unsupported reinterpretations instead of exposing a selector for the pair.
The strict run skipped 30/36 feature combinations and failed the remaining six
mixed groups. The family is gated for DX11; focused public checks retain
same-format view success and predictable alternate-format rejection.

The broad `createView:mip_levels` matrix is also inapplicable: D1 is
unavailable, D2 creates an 18-layer texture despite the six-layer cube-only
limit, and its 3D cases expect arbitrary mip subranges although the selected
FL9_1 projection requires 3D views to include the lowest mip. The strict run
failed 9/9 outer selectors at these documented prerequisites. The selector is
gated for DX11; public tests cover supported full-chain cube/3D views and the
3D non-suffix-mip rejection.

The `createView:texture_view_usage_of_multiple_usages` matrix combines
otherwise-supported RGBA8 view-usage cases with `STORAGE_BINDING`. Five
selectors failed because the texture creation usage includes storage, which
FL9_1 does not expose; the other eleven passed. The list retains those eleven
exact cases and gates only the five storage combinations. This checks view
usage validation, not storage-resource support.

The subsequent `createView:texture_view_usage_with_view_format` family is
gated as a whole. Its 505 outer cases yielded 20 passes, 159 failures, and
326 skips. Failures were expected FL9_1 downlevel rejections: unsupported
format/usages, or `VIEW_FORMATS` which this adapter does not expose. The CTS
helper checks format-specific render/storage support but does not account for
the selected downlevel usage and view-format restrictions. Existing public
tests cover the supported same-format view and rejection of alternate formats.

The following `createView:texture_view_usage` matrix yielded 36 passes, 143
failures, and 273 feature skips. Its helper checks renderability/storage
format support but not all downlevel usage rules, so texture creation fails
for copy, storage, and attachment combinations outside this adapter's
projection. The curated list keeps the exact 36 passing cases, gates the 143
unsupported combinations, and omits the 273 skipped cases.

The next full run reached `encoding:beginComputePass`. The only failing
subcase requested an occlusion query in compute-pass timestamp writes, which
requires the unavailable `OCCLUSION_QUERY` downlevel flag; the timestamp query
case and other timestamp-query cases skipped because `timestamp-query` is not
enabled. The broad file selector is replaced by its five exact selectors,
with only the occlusion case marked inapplicable.

The following `encoding:beginRenderPass` file selector had one passing depth
attachment device-mismatch case, one feature-skipped timestamp query case,
and failures in the color-attachment and occlusion-query cases. The former
constructs two color attachments above the adapter's one-target limit; the
latter requires the unexposed `OCCLUSION_QUERY` flag. The list now keeps the
passing depth selector and gates only those two unsupported cases.

The next failure was `encoding:cmds,clearBuffer:buffer_usage`. Its 10 usage
subcases had nine passes and one rejection because buffer usage `256`
(`INDIRECT`) requires the unavailable `INDIRECT_EXECUTION` flag. The CTS
`paramsSubcasesOnly` query cannot select one inner usage value, so the
`buffer_usage` test is gated as a unit. The other seven clearBuffer test
families remain selected, and public adapter tests cover indirect usage
rejection directly.

The subsequent full strict run passed the retained clearBuffer tests and then
failed all 17 selected `encoding:cmds,compute_pass` cases at the expected
`COMPUTE_SHADERS` downlevel gate. FL9_1 exposes no compute pipeline path, so
the compute-pass CTS file selector is now gated as a unit; focused public Core
tests verify compute-pipeline/pass capability rejection.

The next full run passed the narrowed compute and clearBuffer groups and failed
in `copyBufferToBuffer:buffer_usage`: 15 of 16 usage subcases passed; the
remaining combination attempted an `INDIRECT` buffer and hit the expected
`INDIRECT_EXECUTION` gate. CTS cannot select that inner subcase, so only the
buffer-usage test is gated. The other seven copyBufferToBuffer test families
remain selected.

The next strict curated run reached `copyTextureToTexture:copy_aspects`. The
RGBA8 case is supported, but the D24S8 case tries to create depth textures with
`COPY_SRC`, which Core rejects because `DEPTH_TEXTURE_AND_BUFFER_COPIES` is
not advertised on this adapter. The broad copyTextureToTexture selector is
split into its test families; the RGBA8 aspect case remains selected and only
the D24S8 aspect case is gated. Other depth-copy restrictions already have
dedicated DX11 gates and focused negative coverage.

The retained `copyTextureToTexture:copy_ranges` matrix is inapplicable as a whole. Its D1 case requests an unavailable texture dimension; its 2D cases create three-layer textures and therefore require ordinary 2D-array support; its 3D cases create a `16x8x3` volume rejected by FL9_1's NPOT 3D restriction. The strict focused run confirmed setup errors before the intended copy-range validation (235/235 outer cases failed). The family is gated for DX11. Supported 2D full texture copies remain exercised by strict operation CTS cases, and bounds behavior is covered by focused public/private tests.
The `copyTextureToTexture:copy_within_same_texture` family is likewise not applicable: the CTS fixture requires a seven-layer 2D texture, while this adapter exposes at most six layers for cube textures and not ordinary 2D arrays. All subcases failed during texture creation before self-copy overlap validation. The family is gated for DX11; bounded self-copy behavior remains covered by the backend's supported-resource tests.
`copyTextureToTexture:depth_stencil_copy_restrictions` is gated as a family because its fixtures require three mip levels for depth/stencil textures, while this FL9_1 projection only supports one mip level and does not advertise depth texture copies. Five format selectors failed texture creation at the unsupported mip count; one was feature-skipped. The family never reached the intended copy validation. Existing focused public/Core tests retain explicit rejection coverage for unsupported depth copies.
The retained `copyTextureToTexture:mipmap_level` family can be split by dimension. Its 2D and 3D selectors passed (1/1 each); the D1 selector failed at texture creation because D1 is not exposed. The curated list now retains 2D/3D and gates only D1. These are validation results for the tested mip-level error cases, not a claim that every mipmapped resource/usage is supported.
The `copyTextureToTexture:multisampled_copy_restrictions` CTS family is gated for DX11. The fixture creates multisampled textures with `COPY_SRC` and `COPY_DST`, which the selected FL9_1 adapter correctly rejects at resource creation before the test's copy-region assertions; it does not establish a supported regression. The backend's currently projected multisample path does not expose multisampled copy usages.
The `copyTextureToTexture:sample_count` family also cannot reach its intended copy-compatibility assertions: its multisampled source/destination fixtures request COPY usages that this adapter does not expose for multisampled textures. Core rejected `COPY_DST` during fixture creation. It is gated for DX11; this is consistent with the separately gated multisampled-copy-region family.
`copyTextureToTexture:texture_format_compatibility` is a large format-pair matrix (2,843 selectors). In the strict hardware run, 64 exact source/destination format pairs passed, 1,283 were feature-skipped, and 1,496 failed because one or both formats have no projected `COPY_SRC`/`COPY_DST` support (the fixture creation failure occurs before compatibility validation). The curated list retains the 64 exact pairs that reached and passed the intended validation and omits unsupported/skipped format pairs. This gives direct coverage of same-format and compatible/incompatible format-pair rules only within the exposed copy-capable formats.
The `copyTextureToTexture:texture_usage` selector is a single `paramsSubcasesOnly` cross-product. All usage pairs were attempted, but its matrix includes `STORAGE_BINDING` on the RGBA8 texture; this is rejected at texture creation because storage textures are outside the FL9_1 projection. The selector cannot isolate just that usage axis as an outer CTS selector, so the mixed family is gated for DX11. Copy source/destination usage validation remains covered by the retained copy operation and focused API tests.
The curated full strict run next reached `render/draw:buffer_binding_overlap`. The initial direct `draw` and `drawIndexed` runs failed pipeline creation: the fixture uses an outputless fragment shader with color `writeMask: 0`, and FL9_1 FXC still requires a pixel shader to write all four `SV_Target0` components. The HAL now synthesizes a zero color output for result-less fragment entry points, while preserving their body and side effects. This matches the WebGPU fragment-state rule, which requires a location output only when the corresponding color write mask is nonzero.

After that fix, each exact selector was rerun separately in strict hardware mode. Both passed pipeline creation and then failed at the Core downlevel validation gate because the fixture creates one buffer with `VERTEX | INDEX` usage. FL9_1 requires exactly one of vertex, index or constant-buffer binding, and Core correctly rejects this combination because `UNRESTRICTED_INDEX_BUFFER` is absent. The CTS fixture therefore cannot exercise the otherwise-supported direct/indexed draw operations on this adapter. Only these two `buffer_binding_overlap` selectors are marked `fails-if(dx11)`; other direct and indexed rendering coverage remains selected. `drawIndirect` and `drawIndexedIndirect` remain individually gated because `INDIRECT_EXECUTION` is not exposed. The strict selector logs and resumed curated-run result are recorded in the current handoff/progress entry.

The resumed full strict run then reached `draw:index_buffer_OOB`. Its four direct `drawIndexed` parameter selectors passed, while all four `drawIndexedIndirect` selectors failed when the CTS attempted to create/use an `INDIRECT` buffer; the adapter correctly reports that `INDIRECT_EXECUTION` is unavailable. The entry is split by `drawType`: direct index-buffer bounds validation remains selected, and only `drawIndexedIndirect` is marked `fails-if(dx11)`. This is a fixture capability limitation, not a failure of direct indexed draw validation.

The following `draw:unused_buffer_bound` selector failed in all eight outer cases before its intended assertions. Its CTS source unconditionally creates the supposedly unused `smallBuffer` with `GPUBufferUsage.INDEX | GPUBufferUsage.VERTEX`; Core correctly rejects that combined usage because FL9_1 lacks `UNRESTRICTED_INDEX_BUFFER` and the native 9_1 contract requires a buffer to have a single binding role. Since the same invalid fixture is used across every draw type and size combination, the aggregate selector is skipped for DX11. This does not gate otherwise-supported draw validation.

The next `draw:vertex_buffer_OOB` family failed all 60 outer selectors on its nested `attributeFormat="snorm8x2"` subcase with `Vertex format Snorm8x2 is not supported by this downlevel adapter`. The CTS source places `attributeFormat` after `beginSubcases`, so test-list selectors cannot isolate the other two formats (`float32` and `float16x4`) from the unsupported one. Microsoft’s FL9_1 required-format table lists `R8G8_SNORM` for texture use but not `IA_VERTEX_BUFFER`, confirming that this format is outside the 9_1 contract ([Microsoft required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats)). The mixed `vertex_buffer_OOB` family is therefore skipped for DX11. This avoids treating the fixture’s unsupported format as a supported-path failure; vertex formats within the projected mask remain covered by other tests.

The next full strict run passed the curated draw checks and stopped on two `dynamic_state:setViewport` tests: `exceeds_attachment_size` and `xy_rect_contained_in_bounds`. Both include cases expected to succeed with a viewport that extends beyond the active render target, but Core rejects those cases under the explicit FL9_1 downlevel limit `viewport_rectangles_outside_target = false`. The WebGPU `setViewport` validation checks viewport dimensions and maximum viewport bounds, without requiring the rectangle to fit inside the current attachment ([WebGPU specification](https://www.w3.org/TR/webgpu/)); the rejection is the adapter's documented downlevel limitation. The test list now enumerates the dynamic-state file's individual tests, skipping only those two viewport families while retaining the other six viewport, scissor, blend and stencil tests.

After splitting the dynamic-state tests, the run reached `indirect_draw:indirect_buffer_state` and failed because Core requires the unadvertised `INDIRECT_EXECUTION` flag for `drawIndirect` and `drawIndexedIndirect`. All three selectors currently curated from `indirect_draw.spec.ts` (`indirect_buffer_state`, `indirect_buffer_usage`, and `indirect_buffer,device_mismatch`) invoke those indirect methods in their subcases. They are now individually marked `fails-if(dx11)`; no direct draw selectors are affected.

## Strict curated suite completion — 2026-11-17

The strict command `xtask cts --skip-checkout --backend dx11 --require-dx11-hardware --print-output-when test-fails -f cts_runner/test.lst`, with `WGPU_DX11_REQUIRE_HARDWARE=1`, completed successfully using the isolated target directory `E:\Temp\wgpu-fl91-interstage-target-20261116`. The CTS runner unit tests passed 1,031/1,031. The full list produced 1,686 summaries: 17,962 passed, 4,720 skipped, 0 failed, and 0 warnings. Full log: `E:\Temp\wgpu_dx11_curated_cts_after_texture_ok_gate_20261117.log`.

Additional strict focused results:

- `shader_io,group_and_binding`: 439 passed, 0 skipped, 0 failed. Valid declarations are gated only for unsupported cube-array, depth-cube-array, and external texture resource kinds; binding validation remains selected for their invalid group/binding attributes. Log: `E:\Temp\wgpu_dx11_shader_io_group_and_binding_curated_20261117.log`.
- `shader_io,interpolate`: 599 passed, 49 skipped, 0 failed. Only valid linear and perspective sample-frequency interpolation cases are gated because FL9_1 does not expose `MULTISAMPLED_SHADING`. Log: `E:\Temp\wgpu_dx11_shader_io_interpolate_narrow_curated_20261117.log`.
- `util,texture,texture_ok`: 2 passed, 4 feature skips, 0 failed among retained selectors. Unsupported r8snorm copy-usage and rg32float/rgba32float copy-format selectors are individually gated. Log: `E:\Temp\wgpu_dx11_util_texture_ok_curated_20261117.log`.
- `cts_runner::lst_files_are_sorted`: 1 passed, 0 failed.

The latest shared-target attempt stopped at linker errors before running tests; the isolated target run above succeeded, and no target directory was cleaned. The suite remains curated around the adapter's exposed FL9_1 capabilities and fixture applicability. It does not establish full FL9_1 compatibility or WebGPU conformance.
