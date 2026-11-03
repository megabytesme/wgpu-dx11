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
enable mipmaps, multisampling, comparison samplers, or public exposure.
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
| 2D textures / formats / views | FL9_1 supports multiple native formats subject to individual format flags. Private device creates single-mip, single-layer, single-sample RGBA8 and D24S8 2D textures; BGRA8 is limited to a wrapped swapchain backbuffer. | Transfer, view, clear and draw readback tests. The D24S8 subset is attachment-only; per-format capabilities and numeric limits must match creation, copies, initialization and view behavior. Other formats and dimensions need implementation or validation. |
| Mips, arrays, NPOT / sampling | FL9 conditional NPOT is more restrictive than unconditional support. Private textures have one mip and no arrays; sampled RGBA8 uses checked fragment bindings. | Sampling tests and optional full-NPOT query. `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` covers one restriction but not the NPOT/address-mode pairing; core needs a draw/bind-time texture-sampler compatibility rule or emulation. |
| Samplers | Independent point/linear min/mag, clamp/wrap/mirror and anisotropy up to 2 are implemented in the private HAL. Border addressing and comparison samplers are rejected; FL9_1 finite maximum LOD translation requires a single-mip view. | Patterned FL9_1 GPU readback covers filter and address modes and sampler switching. Anisotropy 2 creates and samples but visual anisotropic quality is unverified. Public feature/limit representation remains incomplete. |
| Bind groups / registers | Group 0 supports up to eight fragment sampled textures, sixteen filtering samplers, and up to thirteen static uniform bindings with compact independent registers. Each draw checks only texture/sampler relationships used by the selected shader entry points. | Sparse multi-uniform and sampled-texture GPU tests cover selected configurations; FXC limits distinct sampled relationships and constants. Counts in `Limits` cannot represent all shader, visibility, compatibility or cross-group restrictions. Public exposure remains blocked. |
| Vertex / index formats | Native FL9 has wider IA support than the HAL. Private pipelines accept a single interleaved vertex buffer and a restricted float/unorm format set. Indexed draws use u16, zero base vertex, replay-time index checks and FL9 index limits. | Indexed and non-indexed readback and rejected-index tests. Limits and downlevel flags cover only part of this; core currently permits other valid vertex formats, buffer slots, u32 index format and zero/nonzero base vertex semantics. Add emulation or explicit validation. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures are rejected by the private shader path. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests. WGSL validation and feature bits do not represent arbitrary FXC instruction pressure or every translation restriction. Shader creation and pipeline validation need conservative diagnostics or emulation; successful Naga validation is insufficient. |
| Pipeline / raster / blending | Private graphics pipeline requires default triangle-list primitive state, one buffer and color target, one sample and a small set of blend states. A D24S8 attachment supports Always/LessEqual depth comparison and optional depth writes; stencil tests and depth bias remain unsupported. | Textured depth ordering and load/clear tests; pipeline and readback tests. FL9_1 does not guarantee separate alpha blending; current private alpha/inversion configurations passed on RTX but need portable hardware validation or a fallback before being advertised. |
| Render passes / targets | One color target and an optional D24S8 depth/stencil attachment support load-or-clear and store. Depth-only clear passes are supported. No resolve, timestamps, occlusion, multiview or multiple color attachments. | Render/readback tests. Read-only and aspect-specific depth views, discard and general pass combinations still need implementation or validation before public exposure. |
| Draw / instancing | Direct draws are split at per-level primitive limits; u16 indexed draws check bounds via GPU staging. Private path only supports one instance, zero first instance and zero base vertex. FL9_1 has no guaranteed native instancing. | Strict indexed and other triangle tests. Existing `BASE_VERTEX` and indirect flags address only some restrictions; `instance_count > 1`, u32 indices, index semantics and alternate topologies need emulation or validation. |
| Queries / synchronization / readback | Native event queries back the private fence; this is not a public query-set implementation. Queue submission serializes the immediate context, and staging readback waits for completion. | Fence, sequencing, copy and GPU readback tests. Timestamp/occlusion query creation is rejected and some impossible destroy paths panic. Device loss, all lifetime/error paths and debug-layer diagnostics remain incomplete. |
| Compute / storage | No native compute shader or UAV support at FL9_1; private HAL rejects compute pipeline creation and storage usages. Other APIs may require CPU or graphics emulation. | Rejection tests; feature/limit gating alone is insufficient until core can safely create and validate a downlevel device without compute and all associated paths are audited. No compute/storage feature is exposed. |
| Presentation | FL9 hardware can support display through platform/format-specific paths. Private backend has a restricted WinRT CoreWindow BGRA8 FIFO path; no general window-system support. | Prior UWP surface diagnostic; format support and actual surface capabilities must be queried. CoreWindow ownership, resize, device loss, synchronization, compositing and non-WinRT platforms need coverage before any surface is advertised. |

## FL9_1 private texture transfers

The private D24S8 subset is separate from the RGBA8 transfer matrix: it is a
single-mip, single-layer, single-sample 2D attachment, with driver-queried
texture and depth-stencil support. Creation clears depth and stencil to zero.
Depth and stencil clear/load flags are handled independently, including
depth-only passes used by core initialization patterns. The private pipeline
supports Always and LessEqual depth comparison with optional writes; non-ignored
stencil faces, depth bias, read-only attachments, aspect-specific views, copying
and sampling are rejected. The broad `depth_stencil` capability remains false.
Texture and view descriptors may declare READ|WRITE as a capability set, as core
does for render attachments, but an actual pass currently requires WRITE usage.
Strict GPU tests verify textured depth ordering, initialization, independent
clear/load behavior, no-write behavior and view lifetime. They do not prove
stencil operations or historical-driver compatibility.

| Transfer | Implementation and restrictions | Coverage |
| --- | --- | --- |
| Buffer to texture | Retained GPU buffer copied to a CPU-readable staging buffer at replay; mapped bytes feed `UpdateSubresource` with an explicit destination box and checked row pitch. This reads GPU contents at execution time, not a CPU shadow. | Offset and padded full/partial uploads, GPU-written source and ordered submissions. |
| Texture to texture | Native GPU `CopySubresourceRegion` between distinct compatible RGBA8 single-subresource textures, with an explicit checked source box. Self-copy is rejected. | Full and partial copies, different overall dimensions, zero-preserving destination and render-to-copy-to-sample. |
| Texture to buffer | A region is copied to a retained CPU-readable staging texture and mapped; each mapped row is written into the destination GPU buffer at the requested pitch and offset. | Full and partial source regions, nonzero buffer offsets, row padding and complete image comparison. |

HAL texture creation eagerly uploads zeroes before returning, including for
partial-write targets. The allocation and upload are proportional to the full
texture size; staging maps in buffer/texture transfers may stall the CPU. Native
diagnostic texture constructors remain separate and can create uninitialized
resources. The private HAL accepts only one-mip, one-layer, one-sample RGBA8
2D textures and enforces feature-level size limits. For `COPY_SRC`, it requests
an internal render-target bind in addition to the shader-resource bind: FL9
forbids GPU-to-CPU copying from resources bound only as shader resources. This
requires RGBA8 render-target support; it does not expose render-target usage to
callers that did not request it. The native D3D11 creation call can reject an
unsupported format/bind combination.

HAL transfer commands use explicit regions. No staging-to-default native copy with
a null source box is used, avoiding the documented 9_x corruption case for
mismatched dimensions or destination offsets. Core validates empty regions and
returns before calling HAL; direct private HAL empty-region transfers remain
rejected. Broad texture formats, multisampling, mip chains and arrays remain
unsupported. A modern GPU forced to 9_1 does not validate every historical
runtime or driver; public adapter exposure remains disabled.

## FL9_1 sampler contract

| Category | Native and driver contract | Private backend and coverage |
| --- | --- | --- |
| Address modes | Clamp, wrap and mirror are available; `MIRROR_ONCE` is not the same as mirrored repeat and is unsupported. Ordinary border addressing is not generally available at 9_1/9_2. | Clamp, repeat and mirrored repeat map independently on U/V/W. Patterned GPU tests cover repeat, mirror and mixed U/V. Border addressing is rejected before native creation; all wgpu border colors may be carried only when no border addressing is used. |
| Min/mag filtering | Point and linear filters can be specified independently. | All four min/mag combinations map to D3D11 filters. GPU readback exercises both magnification and minification with contrasting texels; filter outputs differ. |
| Mipmap filtering | Point and linear mip filters exist. | Both translate, but only one-mip textures are accepted. Mip selection always reaches that sole level; no general mipmap behavior is claimed. |
| LOD | FL9_1 native MinLOD must be integral and MaxLOD must be FLT_MAX. | Nonnegative integral MinLOD with native MaxLOD is accepted; a positive minimum additionally requires matching min/mag filters and anisotropy 1 because FL9 does not specify whether clamping precedes their selection. A finite maximum at least 1 with minimum 0 is translated to FLT_MAX only for validated single-mip views: it cannot cross the magnification/minification boundary at zero and all accesses select level zero. Other finite ranges and fractional minimums are rejected. FXC rejects `SampleLevel` for the tested pixel profile with X4532; explicit-LOD sampling is not claimed. |
| Anisotropy | FL9_1 native maximum is 2; an upper limit alone is not a visual-quality guarantee. | The private backend accepts 1 or 2, requiring linear min, mag and mip filters for 2. Native creation and a sampled GPU output passed; anisotropic quality on minified textures is unverified. Values above 2 are rejected even on a modern GPU forced to 9_1. |
| Comparison / border | Comparison filtering is not generally supported at 9_1; ordinary border addressing is not normally allowed. Optional 9_x shadow support is separate. | Comparison and border addressing remain rejected. On the RTX forced to 9_1, ordinary border sampler creation returned E_INVALIDARG. No optional wgpu border or comparison feature is advertised. |
| NPOT | Conditional NPOT requires one mip and restricts addressing; full NPOT is an optional driver query. | Shader-aware validation rejects non-clamp modes for used NPOT pairs unless full NPOT is positively reported. Mirror is treated conservatively as non-clamp, as are all U/V/W modes. Synthetic restrictive-capability tests supplement modern-driver execution. |

The backend does not expose a public adapter. In particular, the wgpu downlevel
anisotropy flag does not describe this private maximum of 2 by itself; public
validation or emulation must be resolved before any such capability is exposed.

## Logical binding translation

Group 0 assigns textures and filtering samplers independently in ascending
logical binding order to compact fragment-stage `t` and `s` registers. An
Uniforms are independently sorted into `b0` through `b12`; `b13` is reserved
for the disabled texel-load proof. The same slot is used for a uniform in both
stages, and visibility is checked per stage. Layout identity and the same register map are
used by shader translation and command recording. Bind groups retain the native
views and samplers; recording rejects render-target feedback. Group resources
are rebound when changed, and incompatible pipeline layouts cannot draw with
an old group. Render-pass boundaries clear context state.

## Private uniform-buffer contract

The thirteen slots are a private layout allocation limit, not a claim that a
shader can use thirteen arbitrary buffers. The D3D11 API has fourteen application
constant-buffer slots per stage, while FL9_1 shader profiles have smaller legacy
constant budgets. A local FXC pixel-profile test compiles 32 referenced float4
constants and rejects 33 with X5200; instruction pressure and compiler packing
can impose earlier limits. FXC errors fail pipeline creation. A vertex probe
referencing only element 256 compiles; this does not establish a 257-constant
budget because the compiler can remap sparsely referenced constants. The full
vertex budget and near-limit GPU execution have not been measured here.

Static ranges use four-byte-aligned offsets, a nonzero size divisible by 16, and
at most 64 KiB. At bind replay each range is copied from the authoritative GPU
buffer to a CPU-readable staging buffer and uploaded to a separate native
constant buffer. This preserves subrange isolation and queued GPU writes but can
stall once per binding. D3D11.1 offset binding is not used; its range granularity
and runtime requirements differ. Group 0, vertex/fragment visibility, selected
entry-point minimum sizes and group/pipeline compatibility are checked. Dynamic
offsets, multiple groups, general checked uniform dynamic indexing, and broad
public adapter exposure remain unsupported. GPU coverage exercises multiple
range bindings, pipeline transitions, a matrix and padded structure, and sampled
texture coexistence. A checked forced-FL9_1 GPU test also verifies nested
structures, an asymmetric `mat3x2<f32>` and a `vec4<f32>` array with distinct
stride-sensitive values. This does not establish every WGSL layout or operation.

The private layout accepts up to eight sampled textures and sixteen samplers.
These are binding capacities, not a promise that every shader using them
compiles: `ps_4_0_level_9_1` has instruction limits and FXC allocates a legacy
sampler for each distinct texture/sampler relationship. FXC accepted sixteen
relationships in a focused experiment and rejected seventeen with X4510. It
can remap multiple textures sharing a sampler and one texture using multiple
samplers. The forced FL9_1 hardware tests execute three independent textures,
shared and distinct samplers, one texture with two sampler states, and eight
textures with either a shared sampler or eight samplers, and sixteen distinct
texture/sampler relationships. They do not prove all combinations or historical
driver behavior. FXC compilation errors remain pipeline creation errors.

Pipeline creation obtains sampling relationships from Naga's validated
entry-point `sampling_set`, including helper-function calls. The relationships
are resolved through the same logical-to-native binding map as the compiled
shader. Bind groups retain each texture's dimensions and each sampler's
addressing mode. Before indexed or non-indexed draws, the encoder checks the
current pipeline's pairs against the current bind group; changing either state
changes the validation inputs. Missing identities fail closed, and an invalid
draw causes command encoding to fail before submission.

For FL9, a used NPOT texture with a wrapping sampler is accepted only when the
`D3D9_OPTIONS` full-NPOT query returned true. A false or unavailable query uses
the conditional restriction. POT/wrap and NPOT/clamp pairs work even when an
unused cross-product would be invalid. Textures remain single-mip and a sampler
is checked against all address axes. Synthetic tests exercise restrictive
capabilities; the RTX driver reports full NPOT support even when forced to FL9_1.
The HAL returns its existing generic `DeviceError::Unexpected` on draw rejection.
Binding arrays, comparison samplers, storage, vertex sampling and separate bind
groups remain unavailable; normal adapter enumeration remains disabled.

The disabled texel-load proof reserves `t0`, `s15` and `b13`; it cannot simply
be switched on with this allocator. Enabling it requires coordinated register
reservations and collision tests. The active sampled path does not reserve
those slots or enable the proof.

`wgpu-core/src/device/resource.rs` still treats `Backend::Dx11` as unreachable
for shader module construction; `tests/tests/wgpu-gpu/naga_capabilities.rs`
contains a matching test-only assertion. These must be replaced deliberately
when an adapter exists. Tests should cover rejected legal-looking requests at
core validation boundaries and ensure HAL is never called for unsupported
combinations. Merely changing an unreachable arm or choosing generous limits
would expose unsupported paths, including methods that currently panic.
