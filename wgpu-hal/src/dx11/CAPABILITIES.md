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
The private indexed draw passed 64/64 readback pixels. Four-sample RGBA8
texture creation was rejected as expected. Later strict tests passed ordinary
BGRA8 sampling, rendering and raw transfer readback on the same GPU.
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
| 2D textures / formats / views | FL9_1 supports multiple native formats subject to individual format flags. Private device creates single-mip, single-layer, single-sample RGBA8, BGRA8 and D24S8 2D textures. | Transfer, view, clear and draw readback tests. The D24S8 subset is attachment-only; per-format capabilities and numeric limits must match creation, copies, initialization and view behavior. Other formats and dimensions need implementation or validation. |
| Mips, arrays, NPOT / sampling | FL9 conditional NPOT is more restrictive than unconditional support. Private textures have one mip and no arrays; sampled RGBA8 and BGRA8 use checked fragment bindings. | Sampling tests and optional full-NPOT query. `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` covers one restriction but not the NPOT/address-mode pairing; core needs a draw/bind-time texture-sampler compatibility rule or emulation. |
| Samplers | Independent point/linear min/mag, clamp/wrap/mirror and anisotropy up to 2 are implemented in the private HAL. Border addressing and comparison samplers are rejected; FL9_1 finite maximum LOD translation requires a single-mip view. | Patterned FL9_1 GPU readback covers filter and address modes and sampler switching. Anisotropy 2 creates and samples but visual anisotropic quality is unverified. Public feature/limit representation remains incomplete. |
| Bind groups / registers | Up to eight private groups may contain uniforms, sampled textures and samplers, with aggregate limits of thirteen uniforms, eight textures and sixteen samplers. Each draw checks only texture/sampler relationships used by the selected shader entry points. | Sparse multi-group, dynamic-offset, pipeline-transition and cross-group sampled-texture GPU tests cover selected configurations; FXC limits distinct sampled relationships and constants. Counts in `Limits` cannot represent all shader, visibility, compatibility or cross-group restrictions. Public exposure remains blocked. |
| Vertex / index formats | Native FL9 has wider IA support than the HAL. Private pipelines accept up to sixteen sparse vertex-rate streams and a restricted float/unorm format set; multi-slot configurations support direct triangle lists, nonindexed strips and a conservatively bounded native Uint16 indexed triangle-list path. FL9_1 rejects overlapping vertex-element byte ranges within each stream. The slot-0 path additionally supports u16 or u32 conversion. | Indexed and non-indexed readback, sparse-slot, overlap and rejected-index tests. Limits and downlevel flags cover only part of this; core permits overlapping elements and other valid vertex formats and layouts beyond this restricted path. Add emulation or explicit validation. Stream offsets and maximum strides remain unverified on historical 9_1 drivers. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures, selected vertex entries declaring `vertex_index` or `instance_index`, and fragment entries declaring `primitive_index` are rejected by the private shader path. Checked texture loads reachable from the selected entry point remain unavailable. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests, including a sampled shader with an unused texture-load helper. FXC rejects fragment `position`, `front_facing` and `primitive_index` inputs in the tested profile. WGSL validation and feature bits do not represent arbitrary FXC instruction pressure or every translation restriction. Shader creation and pipeline validation need conservative diagnostics or emulation; successful Naga validation is insufficient. |
| Pipeline / raster / blending | Private graphics pipeline supports triangle lists and nonindexed triangle strips with configurable front-face winding and front/back culling, one color target, one sample and a small set of blend states. All RGBA channel write masks are supported for the single target. On FL9_1, alpha, inversion and vignette blending reject simultaneous RGB and alpha writes; masked-channel cases use coupled factors. Enabled blending requires a positive format query. Out-of-target viewports use a separately compiled vertex position transform when available. A D24S8 attachment supports Always/LessEqual depth comparison and optional depth writes; stencil tests and depth bias remain unsupported. | Textured depth ordering, both windings and culling, viewport, all 16 channel masks and masked blend readback tests. Remapped shaders may exceed FL9 compiler limits even when the original compiles; such draws remain rejected. Full-channel separate-alpha modes remain unavailable by default; RTX-only diagnostic tests explicitly bypass this restriction and are not a portability guarantee. |
| Render passes / targets | One color target and an optional D24S8 depth/stencil attachment support load, clear or don't-care and store or discard. Discard retains native contents, which remain undefined to HAL callers. Private read-only passes require LOAD and STORE on both aspects and pipelines without depth or stencil writes; depth sampling aliases remain unsupported. Depth-only clears and draws without a fragment stage are supported. No resolve, timestamps, occlusion, multiview or multiple color attachments. | Render/readback tests, including a depth-only prepass and read-only depth preservation. Aspect-specific depth views, public discard initialization and general pass combinations still need implementation or validation before public exposure. |
| Draw / instancing | Direct draws are split at per-level primitive limits; repeated native draws emulate direct and indexed instances only when no instance-dependent inputs are present. Triangle lists, nonindexed triangle strips and nonindexed zero-input point lists are supported. Strips use even-parity overlapping batches and rebased vertex bindings; indexed strips remain rejected even if the pipeline declares a strip index format. Indexed draws check base-adjusted indices via GPU staging. The slot-0 path translates u32 and wide u16 indices into ordered windows or gathers vertex bytes into scratch. Multi-stream indexed triangle lists accept native Uint16 only under the conservative proof described below. Out-of-bounds vertex fetches discard the entire draw. FL9_1 has no guaranteed native instancing. | Strict tests cover repeated draws and instance order across native chunks, translated windows and gathered triangles, strip winding and a split boundary, signed base vertices, whole-draw discard and one interior point. Instance inputs and additional topology configurations still need emulation or validation. Historical point, high-offset and buffer-copy behavior remain unverified. |

For sparse multi-stream indexed triangle lists, the encoder calculates the
intersection of all stream capacities, accounting for each bound range, stride
and final attribute extent. It accepts a nonempty Uint16 draw only if every raw
index that could be in bounds, and its base-adjusted index, fit the selected
level's native index limit. If no raw index could be in bounds, replay safely
discards the draw. Actual index data is read from GPU staging at replay, after
prior commands; any out-of-bounds index discards the entire logical draw before
native draws execute. Otherwise the original streams and Uint16 index buffer are
bound for native draws. Uint32 or a potentially in-bounds index above the native
limit is rejected during command encoding rather than translated. This is more
restrictive than valid wgpu indexed draws and cannot safely underpin public
adapter exposure without emulation or an adequate validation contract.

FL9_1 direct triangle-list batches are conservatively limited to 65,535
vertices, preserving whole triangles and instance order. This accounts for a
documented D3D9 unique-vertex caveat; whether 10level9 independently guarantees
or emulates that restriction is unresolved. Indexed batches use their separate
index limits. Large native start offsets still need historical-driver testing.
| Queries / synchronization / readback | Native event queries back the private fence; this is not a public query-set implementation. Queue submission serializes the immediate context, and staging readback waits for completion. | Fence, sequencing, copy and GPU readback tests. Core rejects occlusion query sets when `OCCLUSION_QUERY` is absent; timestamp queries require a feature. The private HAL rejects both, and some impossible destroy paths panic. Device loss, all lifetime/error paths and debug-layer diagnostics remain incomplete. |
| Compute / storage | No native compute shader or UAV support at FL9_1; private HAL accepts paired empty compute passes but rejects compute pipeline creation, dispatch and storage usages. Other APIs may require CPU or graphics emulation. | Strict hardware test verifies ordered copies around an empty pass and rejects dispatch and invalid pass sequences; feature/limit gating alone is insufficient until core can safely create and validate a downlevel device without compute and all associated paths are audited. No compute/storage feature is exposed. |
| Presentation | FL9 hardware can support display through platform/format-specific paths. Private backend has restricted WinRT CoreWindow and borrowed Win32 HWND BGRA8 FIFO paths. | Prior UWP diagnostic and a hidden HWND HAL clear, readback, present, resize and recreation test, including a retained completed command buffer. Swapchain calls are serialized against the immediate context, but synchronous window callbacks can still deadlock on reentry; pending presentation completion, visible output, device loss and surface capability reporting remain unverified. No public surface is advertised. |

For FL9_1/9_2, default buffers have exactly one native vertex or index bind:
copy-only, mapped and uniform-source buffers use an internal vertex bind, while
index buffers use an index bind. This does not add a logical wgpu usage. Staging
buffers retain zero bind flags because D3D11 staging and CPU-read resources
cannot be pipeline-bound; Microsoft's 10Level9 table does not explicitly state
that exception. A modern forced-FL9_1 device passed copy/readback tests; legacy
runtime behavior remains unverified.

## FL9_1 private texture transfers

The private D24S8 subset is separate from the color transfer matrix: it is a
single-mip, single-layer, single-sample 2D attachment, with driver-queried
texture and depth-stencil support. Creation clears depth and stencil to zero.
Depth and stencil clear/load flags are handled independently, including
depth-only passes used by core initialization patterns. Depth-only drawing
uses a trivial pixel shader with no render target because the tested FL9_1 FXC
profile rejects a pixel shader that writes no target. Historical-driver
behavior of this fallback is unverified. The private pipeline
supports Always and LessEqual depth comparison with optional writes; non-ignored
stencil faces, depth bias, aspect-specific views, copying and sampling are
rejected. Private read-only passes require LOAD|STORE for both aspects and a
pipeline without depth writes. They use an ordinary DSV; sampled aliases remain
prohibited, so this does not establish the public `READ_ONLY_DEPTH_STENCIL` flag.
The broad `depth_stencil` capability remains false. Texture and view descriptors
may declare READ, WRITE or READ|WRITE, but an actual pass uses one usage.
Strict GPU tests verify textured depth ordering, initialization, independent
clear/load and discard behavior, no-write behavior and view lifetime. They do not prove
stencil operations or historical-driver compatibility. The documented FL9_1
minimum guarantees Always and LessEqual depth comparisons; whether 10Level9
guarantees or emulates the others without a per-comparison query is unresolved.
They remain rejected rather than being substituted with a different comparison.

| Transfer | Implementation and restrictions | Coverage |
| --- | --- | --- |
| Buffer to texture | Retained GPU buffer copied to a CPU-readable staging buffer at replay; mapped bytes feed `UpdateSubresource` with an explicit destination box and checked row pitch. This reads GPU contents at execution time, not a CPU shadow. | Offset and padded full/partial uploads, GPU-written source and ordered submissions. |
| Texture to texture | Native GPU `CopySubresourceRegion` between distinct, identically formatted RGBA8 or BGRA8 single-subresource textures, with an explicit checked source box. Self-copy and cross-format copies are rejected. | Full and partial copies, different overall dimensions, zero-preserving destination and render-to-copy-to-sample. |
| Texture to buffer | A region is copied to a retained CPU-readable staging texture and mapped; each mapped row is written into the destination GPU buffer at the requested pitch and offset. | Full and partial source regions, nonzero buffer offsets, row padding and complete image comparison. |

HAL texture creation eagerly uploads zeroes before returning, including for
partial-write targets. The allocation and upload are proportional to the full
texture size; staging maps in buffer/texture transfers may stall the CPU. Native
diagnostic texture constructors remain separate and can create uninitialized
resources. The private color HAL accepts only one-mip, one-layer, one-sample
RGBA8 or BGRA8 2D textures and enforces feature-level size limits. Raw BGRA8
uploads and readback retain B,G,R,A byte order; shader sampling and render
target writes use logical RGBA channels. For `COPY_SRC`, it requests
an internal render-target bind in addition to the shader-resource bind: FL9
forbids GPU-to-CPU copying from resources bound only as shader resources. This
requires render-target support for the selected format; it does not expose render-target usage to
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

The nondefault `dx11-test-fixture` feature provides an unsafe, non-enumerated
adapter solely for a controlled core integration test. Normal `Adapter::open`
still fails. The fixture reports driver-queried RGBA8 and BGRA8 color and copy
capabilities and D24S8 attachment capability, and no surface capabilities. Core's mapping also
permits transient attachments from the color flag, which this HAL does not
support; its test-only limits and format report are not a general adapter
contract. The fixture verifies core buffer operations and an offscreen triangle,
not ordinary public adapter selection or presentation.

## Logical binding translation

The pipeline assigns textures and filtering samplers independently in ascending
group and logical binding order to compact fragment-stage `t` and `s` registers.
Uniforms from all groups are sorted by group and binding into `b0` through
`b12`; vertex `b13` is
reserved for viewport remapping and the disabled texel-load proof also reserves
`b13`. The same slot is used for a uniform in both
stages, and visibility is checked per stage. Layout identity and the same register map are
used by shader translation and command recording. Bind groups retain the native
views and samplers; recording rejects render-target feedback. Each draw binds
resources using the active pipeline's plan, and changing the layout passed to
`set_bind_group` invalidates groups at and after the first unequal prefix.
Render-pass boundaries clear context state.

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

Ranges use four-byte-aligned offsets, a nonzero size divisible by 16, and
at most 64 KiB. At bind replay each range is copied from the authoritative GPU
buffer to a CPU-readable staging buffer and uploaded to a separate native
constant buffer. This preserves subrange isolation and queued GPU writes but can
stall once per binding per draw. Each group bind also allocates an independent
staging/constant pair per uniform so that the same group can be used at
different dynamic offsets simultaneously. D3D11.1 offset binding is not used;
its range granularity and runtime requirements differ. Group identity,
vertex/fragment visibility, selected
entry-point minimum sizes and group/pipeline compatibility are checked. Dynamic
offsets are consumed in ascending logical binding order and checked against the
logical source-buffer size before recording. A range extending to the buffer end
allows only offset zero. Four-byte alignment is the private HAL minimum; a future
public adapter must enforce its reported wgpu alignment through core. Checked
shaders using uniforms permit dynamic access to proven value vectors;
checked dynamic vector-pointer indexing is rejected regardless of uniform use.
Uniform-pointer indexing, dynamic arrays with uniforms, matrices and unknown
provenance remain restricted. Matrix-containing aggregate stores are rejected
in checked and unchecked shaders because Naga's current HLSL special lowering
can lose writes or omit an index clamp. The conservative guard also rejects
unrelated stores to such aggregates and shaders exceeding its analysis budget.
The tested FXC rejects a potentially negative value index in the pixel profile;
other compiler versions may differ. General checked
uniform indexing and broad public adapter exposure remain unsupported. GPU coverage exercises multiple
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
current pipeline's pairs against the current bound groups; changing either state
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
