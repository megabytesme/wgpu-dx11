# D3D11 capability and public-adapter contract

This is an inventory of the private backend, not a wgpu capability report.
Ordinary D3D11 adapter enumeration remains empty. The internal model separates
feature-level minima, per-device query results, the restricted implementation,
and the public-adapter gate. `check_private_texture_2d` is a conservative
predicate for one private subset; it is not a complete texture validation rule.
A failed query is recorded with its HRESULT and never treated as support.

## Feature-level and driver facts

The selected device level, not the adapter's maximum, determines the guaranteed
minimum. For FL9_1, the baseline includes 2048 2D dimensions, one render target,
65,535 primitives per draw, index values through 65,534, vertex and pixel
shaders, and conditional NPOT textures. These are minimum guarantees, not
assertions that larger sizes work on every device. Level 9_1 has no guaranteed
separate alpha blending, hardware instancing, geometry or compute shader stage,
or native UAV storage. Other feature levels have different documented minima;
this model does not enable their additional execution paths.

NPOT textures at FL9 require one mip and restricted addressing. Full NPOT and
comparison-shadow support are separately queried using `CheckFeatureSupport`;
a newer runtime or driver may return E_INVALIDARG for an unknown query, which
is retained as unavailable. `CheckFormatSupport` is queried only for RGBA8,
BGRA8 and D24S8, and `CheckMultisampleQualityLevels` for RGBA8 at four samples.
A positive bit or quality count is a driver observation, not an implemented HAL
feature or a guarantee of a valid combination of descriptors. The RGBA8 2D
predicate intersects format bits with the implemented format, usage, shape and
single-sample restrictions. It uses the guaranteed dimension as a conservative
portable bound, not a measured hardware maximum.

References: [feature-level table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro),
[10Level9 device constraints](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device),
[10Level9 draw constraints](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context),
[format support queries](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11device-checkformatsupport),
[shader compiler targets](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets),
and the [WebGPU specification](https://www.w3.org/TR/webgpu/).

## Recorded forced-FL9_1 observation

On 2026-11-02, an RTX 3090 Ti (`0x10de:0x2203`), Windows 11 build 26300,
NVIDIA driver 32.0.16.2101, returned FL9_1 for an FL9_1-only request. Its
format-support masks were RGBA8 `0x312ef3e2`, BGRA8 `0x312ef3e0`, and D24S8
`0x00210020`. RGBA8 reports 2D texture, shader sampling, render-target and
blend support. The RGBA8 four-sample quality-level count was 1; full NPOT and
comparison-shadow queries returned true. These optional observations do not
enable mipmaps, multisampling, depth, comparison samplers, or public exposure.
The private indexed draw passed 64/64 readback pixels, and private BGRA8 texture
creation and four-sample RGBA8 texture creation were rejected as expected.
This forced level on a modern GPU does not emulate all historical FL9_1
hardware or establish portability of separate-alpha blending.

## FL9_1 inventory and gaps

All rows block public exposure until their gaps are addressed or safe validation
is established. Hardware availability below means the FL9_1 contract, not a
claim about a particular driver's optional capabilities. Test names refer to
private HAL tests; success on one modern adapter is not proof for all FL9 GPUs.

| Category | Hardware and private implementation | Coverage; wgpu mapping and remaining work |
| --- | --- | --- |
| Device / adapter | Native DXGI hardware discovery and explicitly selected feature levels exist. HAL adapter is uninhabited and enumeration is empty. | Hardware probe and zero-adapter tests. No `Limits`, `Features`, `DownlevelCapabilities`, format capability set, or wgpu device is advertised. A new adapter must be truthful and satisfy every valid operation it permits. |
| Buffers, maps, copies | Native buffers and copies exist. Private HAL implements vertex, index, uniform, copy and explicit CPU-shadow mappings; vertex+index is rejected at 9_1/9_2. Storage and indirect usages are absent. | Copy, map, initialization and ownership tests. `UNRESTRICTED_INDEX_BUFFER` and limits can describe some restrictions, but mapping coherency, initialization, complete usage combinations and lifetime validation need an audit. |
| 2D textures / formats / views | FL9_1 supports multiple native formats subject to individual format flags. Private device creates single-mip, single-layer, single-sample RGBA8 2D textures; BGRA8 is limited to a wrapped swapchain backbuffer. | Transfer, view, clear and draw readback tests. Per-format capabilities and numeric limits can restrict usage and dimensions, but must match creation, copies, initialization and view behavior. All other formats and dimensions need implementation or validation. |
| Mips, arrays, NPOT / sampling | FL9 conditional NPOT is more restrictive than unconditional support. Private textures have one mip and no arrays; sampled RGBA8 uses checked fragment bindings. | Sampling tests and optional full-NPOT query. `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` covers one restriction but not the NPOT/address-mode pairing; core needs a draw/bind-time texture-sampler compatibility rule or emulation. |
| Samplers | Point/linear and restricted clamp/wrap/mirror modes exist. No anisotropy, comparison or border mode in the private HAL; FL9_1 LOD translation requires a single-mip view. | Sampler and texture binding tests. Omitting `COMPARISON_SAMPLERS` describes one gap; sampler LOD, filter/address and texture combinations need validation or emulation. Optional driver comparison support does not enable backend comparison sampling. |
| Bind groups / registers | Only empty, uniform, one texture+sampler, uniform+texture+sampler and a fixed uniform+two-texture/two-sampler shape are accepted, with fixed binding numbers and visibility. | Layout/ownership and textured GPU tests. Counts in `Limits` cannot represent legal binding permutations, gaps, visibility, dynamic offsets, cross-group register allocation or aliasing. A future design needs general register allocation or specific core layout and bind-time validation. |
| Vertex / index formats | Native FL9 has wider IA support than the HAL. Private pipelines accept a single interleaved vertex buffer and a restricted float/unorm format set. Indexed draws use u16, zero base vertex, replay-time index checks and FL9 index limits. | Indexed and non-indexed readback and rejected-index tests. Limits and downlevel flags cover only part of this; core currently permits other valid vertex formats, buffer slots, u32 index format and zero/nonzero base vertex semantics. Add emulation or explicit validation. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures are rejected by the private shader path. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests. WGSL validation and feature bits do not represent arbitrary FXC instruction pressure or every translation restriction. Shader creation and pipeline validation need conservative diagnostics or emulation; successful Naga validation is insufficient. |
| Pipeline / raster / blending | Private graphics pipeline requires default triangle-list primitive state, one buffer and target, no depth, one sample and a small set of blend states. Hardware supports more configurations, format permitting. | Pipeline and readback tests. Single-target/sample limits and format flags are expressible, but allowed blend operations, vertex combinations, shader restrictions and all-write-mask requirement are not fully captured by existing limits. FL9_1 does not guarantee separate alpha blending; current private alpha/inversion configurations passed on RTX but need portable hardware validation or a fallback before being advertised. |
| Render passes / targets | One color target with load-or-clear and store is implemented. No depth/stencil, resolve, timestamps, occlusion, multiview or multiple attachments. RGBA8 renderability is queried; BGRA8 is surface-specific. | Render/readback tests. Attachment count, format flags and sample counts help, but core's generally valid pass and load/store combinations require implementation or explicit gating. |
| Draw / instancing | Direct draws are split at per-level primitive limits; u16 indexed draws check bounds via GPU staging. Private path only supports one instance, zero first instance and zero base vertex. FL9_1 has no guaranteed native instancing. | Strict indexed and other triangle tests. Existing `BASE_VERTEX` and indirect flags address only some restrictions; `instance_count > 1`, u32 indices, index semantics and alternate topologies need emulation or validation. |
| Queries / synchronization / readback | Native event queries back the private fence; this is not a public query-set implementation. Queue submission serializes the immediate context, and staging readback waits for completion. | Fence, sequencing, copy and GPU readback tests. Timestamp/occlusion query creation is rejected and some impossible destroy paths panic. Device loss, all lifetime/error paths and debug-layer diagnostics remain incomplete. |
| Compute / storage | No native compute shader or UAV support at FL9_1; private HAL rejects compute pipeline creation and storage usages. Other APIs may require CPU or graphics emulation. | Rejection tests; feature/limit gating alone is insufficient until core can safely create and validate a downlevel device without compute and all associated paths are audited. No compute/storage feature is exposed. |
| Presentation | FL9 hardware can support display through platform/format-specific paths. Private backend has a restricted WinRT CoreWindow BGRA8 FIFO path; no general window-system support. | Prior UWP surface diagnostic; format support and actual surface capabilities must be queried. CoreWindow ownership, resize, device loss, synchronization, compositing and non-WinRT platforms need coverage before any surface is advertised. |

`wgpu-core/src/device/resource.rs` still treats `Backend::Dx11` as unreachable
for shader module construction; `tests/tests/wgpu-gpu/naga_capabilities.rs`
contains a matching test-only assertion. These must be replaced deliberately
when an adapter exists. Tests should cover rejected legal-looking requests at
core validation boundaries and ensure HAL is never called for unsupported
combinations. Merely changing an unreachable arm or choosing generous limits
would expose unsupported paths, including methods that currently panic.
