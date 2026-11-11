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

## Proposed filtered depth selectors

The exact selectors below are useful candidates for the supported depth
formats, but have **not** been recorded as separate invocations. In the broad
`webgpu:api,operation,rendering,depth:*` run, the CTS `depth_compare_func`
parameter matrix's 72 D16, Depth24Plus, and Depth24PlusStencil8 comparisons
passed. The three filters below isolate those format groups for a reproducible
follow-up; they must be run before being listed as individually executed.

```text
webgpu:api,operation,rendering,depth:depth_compare_func:format="depth16unorm"
webgpu:api,operation,rendering,depth:depth_compare_func:format="depth24plus"
webgpu:api,operation,rendering,depth:depth_compare_func:format="depth24plus-stencil8"
```

The source parameterizes `depth_compare_func` over the depth formats and
comparison functions in
[`depth.spec.ts`](../../../cts/src/webgpu/api/operation/rendering/depth.spec.ts).
The broad run's overall result was 130 subcases: 72 passed, 24 skipped and 34
failed. Its 8 D24S8 failures requested `COPY_DST`; 26 failed texture creation
for unsupported Depth32Float usages and then observed the unchanged clear
target. The 24 Depth32FloatStencil8 cases skipped. This broad result is not
proof that all depth states or formats pass. See
[`TESTING.md`](TESTING.md#4-webgpu-cts) and
[`FL9_1_PROGRESS.md`](FL9_1_PROGRESS.md#strict-depth-cts-probe-2026-11-11)
for the full triage and command.

## Known boundary results; do not count as passes

| Selector or selector family | Result and reason |
| --- | --- |
| `webgpu:api,operation,rendering,basic:*` | Recorded 1 pass, 2 failures. The failures request `Rgba8Uint` texture copy usage, which the selected FL9_1 device did not report and the projection does not expose. The RGBA8 selectors above are the supported transfer evidence. |
| `webgpu:api,operation,rendering,depth:depth_disabled:*` | Failed because its D24S8 fixture requests `COPY_DST`, which is outside this projection; this does not establish a depth-state execution failure. |
| `webgpu:api,operation,rendering,draw:default_arguments:*` | Failed when its shader used runtime integer operations rejected by the selected FL9_1 shader subset. |
| `webgpu:api,operation,rendering,depth:*` | Broad mixed result described above; do not classify the 34 fixture/format failures as supported-operation passes or backend depth failures without individual source triage. |
| `image_copy:origins_and_extents` | Not a valid isolated single-layer 2D transfer selector: it generates 1D/3D and array-layer cases. Filtering to RGBA8 and 2D still includes array layers, which FL9_1 does not expose. |
| `webgpu:api,operation,rendering,basic:large_draw:*` | `cts_runner/test.lst` contains a commented global failure expectation. No DX11-specific execution result is recorded here. |

The shader and array boundaries are documented in
[`CAPABILITIES.md`](CAPABILITIES.md#fl9_1-inventory-and-gaps). Ordinary 2D
texture arrays are a FL9_1 feature-level restriction; do not reinterpret their
rejection as a missing transfer implementation.

## Scope of this inventory

This list supports a practical regression subset for the current projection.
It does not cover every advertised format, API validation case, sampler,
shader operation, render state, surface path, or driver. The strict public
`wgpu` integration tests and private HAL tests provide additional evidence,
but they are not CTS executions. Desktop forced-FL9_1 results do not verify
historical FL9_1 devices or Lumia/CoreWindow runtime behavior.
