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

Conditional NPOT textures at FL9 require one mip and restricted addressing. Full NPOT and
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
enable unrestricted mipmaps, multisampling, comparison samplers, or public exposure.
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
| 2D textures / formats / views | FL9_1 supports multiple native formats subject to individual format flags. Private RGBA8 and BGRA8 2D textures have one layer and sample; restricted copy-only and sampled POT textures can have multiple mips. D24S8 remains single-mip and attachment-only. | Transfer, view, clear and draw readback tests. Per-format capabilities and numeric limits must match creation, copies, initialization and view behavior. Other formats and dimensions need implementation or validation. |
| Mips, arrays, NPOT / sampling | POT RGBA8 and BGRA8 textures can have mip chains when format support is reported. Sampled chains require exact copy-destination and resource usage, optionally with copy-source, and a suffix view that includes the resource's last mip; attachment views remain single-mip. Arrays and NPOT mip chains remain unavailable. | Strict FL9_1 tests verify per-mip transfers and full-chain and suffix-view sampling on the RTX. Arbitrary non-suffix views need validation or emulation. Public exposure remains blocked. |
| Samplers | Independent point/linear min/mag, clamp/wrap/mirror and anisotropy up to 2 are implemented in the private HAL. Border addressing and comparison samplers are rejected; translated LOD ranges are checked against each used sampled view. | Patterned FL9_1 GPU readback covers filter and address modes, sampled mip levels and sampler switching. Anisotropy 2 creates and samples but visual anisotropic quality is unverified. Public feature/limit representation remains incomplete. |
| Bind groups / registers | Up to eight private groups may contain uniforms, sampled textures and samplers, with aggregate limits of thirteen uniforms, eight textures and sixteen samplers. Each draw checks only texture/sampler relationships used by the selected shader entry points. | Sparse multi-group, dynamic-offset, pipeline-transition and cross-group sampled-texture GPU tests cover selected configurations; FXC limits distinct sampled relationships and constants. Counts in `Limits` cannot represent all shader, visibility, compatibility or cross-group restrictions. Public exposure remains blocked. |
| Vertex / index formats | Native FL9 has wider IA support than the HAL. Private pipelines accept up to sixteen sparse vertex- or instance-rate streams, including zero-stride constant streams, and restricted float, unorm and `Snorm16x2`/`Snorm16x4` formats. FL9_1 requires input-assembler support for both SNORM formats; pipeline creation relies on that guarantee, while the hardware test queries the actual device. Attributes retain four-byte alignment. Multi-slot configurations support direct triangle lists, nonindexed strips and Uint16/Uint32 indexed triangle lists with translation when needed. On FL9_1, overlapping elements in a logical stream are assigned to distinct native slots bound to the same buffer. | Indexed and non-indexed readback, sparse-slot, overlap, zero-stride, SNORM vertex and rejected-index tests, plus controlled core renders. Limits and downlevel flags cover only part of the contract; other core-valid vertex formats and layouts need implementation or validation. Overlapping physical reads, stream offsets and maximum strides remain unverified on historical 9_1 drivers. SNORM vertex support does not imply SNORM texture support. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures, selected vertex entries declaring `vertex_index` or `instance_index`, and fragment entries declaring `primitive_index` are rejected by the private shader path. Checked texture loads reachable from the selected entry point remain unavailable. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests, including a sampled shader with an unused texture-load helper. FXC rejects fragment `position`, `front_facing` and `primitive_index` inputs in the tested profile. WGSL validation and feature bits do not represent arbitrary FXC instruction pressure or every translation restriction. Shader creation and pipeline validation need conservative diagnostics or emulation; successful Naga validation is insufficient. |
| Pipeline / raster / blending | Private graphics pipeline supports triangle lists and nonindexed triangle strips with configurable front-face winding and front/back culling, one color target, one sample and a small set of blend states. All RGBA channel write masks are supported for the single target. On FL9_1, alpha, inversion and vignette blending reject simultaneous RGB and alpha writes; masked-channel cases use coupled factors. Enabled blending requires a positive format query. Out-of-target viewports use a separately compiled vertex position transform when available. A D24S8 attachment supports Always/LessEqual depth comparison with optional depth writes and the restricted no-op stencil cases below; nontrivial stencil and depth bias remain unsupported. | Textured depth ordering, no-op stencil rendering, both windings and culling, viewport, all 16 channel masks and masked blend readback tests. Remapped shaders may exceed FL9 compiler limits even when the original compiles; such draws remain rejected. Full-channel separate-alpha modes remain unavailable by default; RTX-only diagnostic tests explicitly bypass this restriction and are not a portability guarantee. |
| Render passes / targets | One color target and an optional D24S8 depth/stencil attachment support load, clear or don't-care and store or discard. Discard retains native contents, which remain undefined to HAL callers. Restricted transient color and depth attachments use ordinary textures without memoryless savings; transient combinations with sampled or copy usage are rejected. Private read-only passes require LOAD and STORE on both aspects and pipelines without depth or stencil writes; depth sampling aliases remain unsupported. Depth-only clears and draws without a fragment stage are supported. No resolve, timestamps, occlusion, multiview or multiple color attachments. | Render/readback tests include a depth-only prepass, read-only depth preservation and transient color/depth attachment passes on the tested FL9_1 device. Aspect-specific depth views, public discard initialization and general pass combinations still need implementation or validation before public exposure. |
| Draw / instancing | Direct draws are split at per-level primitive limits; restricted instance-rate streams use repeated non-instanced native draws with stride-zero bindings adjusted by `first_instance` and instance number. FL9_1 does not use native per-instance IA classification. Triangle lists, nonindexed triangle strips and nonindexed zero-input point lists are supported; indexed draws with only instance-rate streams and shaders using `instance_index` remain unsupported. Strips use even-parity overlapping batches and rebased vertex bindings; indexed strips remain rejected even if the pipeline declares a strip index format. Indexed draws check base-adjusted indices via GPU staging. Native-safe u16 indices use native buffers; other valid u16 and u32 triangle lists use ordered rebased u16 windows or gather GPU vertex bytes for each stream into scratch. Out-of-bounds vertex fetches discard the entire draw. | Strict tests cover restricted instance-rate rendering, repeated draws without instance inputs, translated windows and gathered triangles, strip winding and a split boundary, signed base vertices, whole-draw discard and one interior point. General instancing and additional topology configurations still need implementation or validation. Historical point, high-offset and buffer-copy behavior remain unverified. |

For sparse multi-stream indexed triangle lists, actual index data is read from
GPU staging at replay, after prior commands. Every index is checked against each
bound range, stride and final attribute extent; any out-of-bounds fetch discards
the whole logical draw before native draws execute. Native-safe Uint16 indices
use the original buffers. Other valid Uint16 and Uint32 indices are partitioned
in primitive order into Uint16 windows with separately rebased stream offsets;
triangles wider than a window gather each stream into initialized scratch
buffers. Plans and scratch resources are prepared before drawing. The staging,
copies and allocations can stall or add substantial overhead. This private
subset does not remove the other validation and implementation gaps blocking
public adapter exposure.

Zero-stride streams use the same bound element for every vertex; rebased index
windows keep its offset unchanged and wide gathers retain the original constant
buffer. The full attribute extent must fit the bound range. The current indexed
path discards a draw when any base-adjusted index is negative. Whether that is
required when every stream has zero stride remains an open specification and
validation question; the current triangle-only shader subset produces degenerate
geometry from all-zero streams, so a completed draw provides no visible proof
of that case. Historical hardware has not been tested.

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
supports Always and LessEqual depth comparison with optional writes. Stencil
states are accepted only if both faces always pass and neither can write: each
comparison must be Always, or Equal/LessEqual/GreaterEqual with the low eight
read-mask bits zero; the low eight write-mask bits must be zero or all operations
on both faces must be Keep. These states are implemented with stencil disabled,
and reference updates are accepted as no-ops. Real stencil comparisons or
writes, depth bias, aspect-specific views, copying and sampling are rejected.
Future nontrivial stencil support must retain and rebind the reference on native
depth-stencil state changes. Private read-only passes require LOAD|STORE for both
aspects and a pipeline without depth writes. They use an ordinary DSV; sampled aliases remain
prohibited, so this does not establish the public `READ_ONLY_DEPTH_STENCIL` flag.
The broad `depth_stencil` capability remains false. Texture and view descriptors
may declare READ, WRITE or READ|WRITE, but an actual pass uses one usage.
Strict GPU tests verify textured depth ordering, initialization, independent
clear/load and discard behavior, no-write behavior and view lifetime. The no-op
stencil tests verify rendered color but cannot read back stencil bits. They do not
prove general stencil operations or historical-driver compatibility. The
documented FL9_1 minimum guarantees Always and LessEqual depth comparisons; whether 10Level9
guarantees or emulates the others without a per-comparison query is unresolved.
They remain rejected rather than being substituted with a different comparison.

| Transfer | Implementation and restrictions | Coverage |
| --- | --- | --- |
| Buffer to texture | Retained GPU buffer copied to a CPU-readable staging buffer at replay; mapped bytes feed `UpdateSubresource` with an explicit destination box and checked row pitch. This reads GPU contents at execution time, not a CPU shadow. | Offset and padded full/partial uploads, GPU-written source and ordered submissions. |
| Texture to texture | Native GPU `CopySubresourceRegion` between distinct, identically formatted RGBA8 or BGRA8 textures, including selected supported mip subresources, with an explicit checked source box. Self-copy and cross-format copies are rejected. | Full and partial copies, different overall dimensions, cross-resource mip copies, zero-preserving destination and render-to-copy-to-sample. |
| Texture to buffer | A region is copied to a retained CPU-readable staging texture and mapped; each mapped row is written into the destination GPU buffer at the requested pitch and offset. | Full and partial source regions, nonzero buffer offsets, row padding and complete image comparison. |

HAL texture creation eagerly uploads zeroes before returning, including for
partial-write targets and each mip of a supported chain. Allocation and uploads
are proportional to texture size; staging maps in buffer/texture transfers may
stall the CPU. Native
diagnostic texture constructors remain separate and can create uninitialized
resources. The private color HAL accepts one-layer, one-sample RGBA8 or BGRA8
2D textures and enforces feature-level size limits. Multiple mips require POT
dimensions, exact `COPY_SRC | COPY_DST`, `COPY_DST | RESOURCE` or
`COPY_SRC | COPY_DST | RESOURCE` usage and positive per-format
`D3D11_FORMAT_SUPPORT_MIP`, texture and shader-sample bits; copy-source usage
also requires render-target support.
The `check_private_texture_2d` predicate represents this restricted subset,
not general mip capability. Raw BGRA8
uploads and readback retain B,G,R,A byte order; shader sampling and render
target writes use logical RGBA channels. For `COPY_SRC`, it requests
an internal render-target bind in addition to the shader-resource bind: FL9
forbids GPU-to-CPU copying from resources bound only as shader resources. This
requires render-target support for the selected format; it does not expose render-target usage to
callers that did not request it. The native D3D11 creation call can reject an
unsupported format/bind combination.

A copy-only color texture may have a metadata-only view with no native shader
resource or render-target view. It retains the texture and owning device but
cannot be sampled or attached. The controlled core fixture checked RGBA8 and
BGRA8 copy-only views, rejected incompatible view usages and verified 12 pixels
per format through GPU upload, copy and readback. Multi-mip copy-only textures
also support checked metadata-only views of selected mip ranges. They do not
provide native sampled or render-target mip views.

Sampled multi-mip textures use exactly `COPY_DST | RESOURCE` or
`COPY_SRC | COPY_DST | RESOURCE`, and shader-resource views spanning a suffix
through the resource's last mip. The former has no copy-source usage and cannot
be directly read back; its native resource uses only a shader-resource bind. No
multi-mip attachment view is implemented. Sampled view metadata and the selected shader relationships drive
LOD compatibility validation at draw time; no cross-product check rejects an
unused texture/sampler pair. On the tested RTX FL9_1 device, a same-pass
pipeline and bind-group transition retained caller-destroyed sampled resources
through submission and produced 32 green and 32 blue pixels for each format.

Strict FL9_1 tests on the RTX 3090 Ti verified four initialized mip levels for
RGBA8 and BGRA8, padded partial uploads, copies between different textures and
mip levels, readback, metadata views and retained resources. The controlled core
fixture checked all 43 pixels of a four-level chain. FL9 shader-resource views
must include the smallest resource mip; arbitrary core-valid views would require
validation or emulation before public exposure. Historical GPU behavior is
unverified.

HAL transfer commands use explicit regions. No staging-to-default native copy with
a null source box is used, avoiding the documented 9_x corruption case for
mismatched dimensions or destination offsets. Core validates empty regions and
returns before calling HAL; direct private HAL empty-region transfers remain
rejected. Broad texture formats, multisampling, NPOT mip chains, sampled
non-suffix views and arrays remain unsupported. A modern GPU forced to 9_1 does not
validate every historical runtime or driver; public adapter exposure remains
disabled.

## FL9_1 sampler contract

| Category | Native and driver contract | Private backend and coverage |
| --- | --- | --- |
| Address modes | Clamp, wrap and mirror are available; `MIRROR_ONCE` is not the same as mirrored repeat and is unsupported. Ordinary border addressing is not generally available at 9_1/9_2. | Clamp, repeat and mirrored repeat map independently on U/V/W. Patterned GPU tests cover repeat, mirror and mixed U/V. Border addressing is rejected before native creation; all wgpu border colors may be carried only when no border addressing is used. |
| Min/mag filtering | Point and linear filters can be specified independently. | All four min/mag combinations map to D3D11 filters. GPU readback exercises both magnification and minification with contrasting texels; filter outputs differ. |
| Mipmap filtering | Point and linear mip filters exist. | Both are tested with sampled full-chain POT textures, and suffix-view sampling passed on the RTX. Arbitrary non-suffix views and general mipmap behavior remain unavailable. |
| LOD | FL9_1 native MinLOD must be integral and MaxLOD must be FLT_MAX. | A normalized minimum requires a one-mip view. When only a finite maximum is raised and the minimum is preserved, a used multi-mip view requires the original maximum to reach its last mip and anisotropy 1. Anisotropic sampling with a raised finite maximum remains one-mip-only. Other unrepresentable mixed-filter and anisotropic ranges are rejected. FL9 does not specify whether clamping precedes min/mag selection. FXC rejects `SampleLevel` for the tested pixel profile with X4532; explicit-LOD sampling is not claimed. |
| Anisotropy | FL9_1 native maximum is 2; an upper limit alone is not a visual-quality guarantee. | The private backend accepts 1 or 2, requiring linear min, mag and mip filters for 2. Native creation and a sampled GPU output passed; anisotropic quality on minified textures is unverified. Values above 2 are rejected even on a modern GPU forced to 9_1. |
| Comparison / border | Comparison filtering is not generally supported at 9_1; ordinary border addressing is not normally allowed. Optional 9_x shadow support is separate. | Comparison and border addressing remain rejected. On the RTX forced to 9_1, ordinary border sampler creation returned E_INVALIDARG. No optional wgpu border or comparison feature is advertised. |
| NPOT | Conditional NPOT requires one mip and restricts addressing; full NPOT is an optional driver query. | Shader-aware validation rejects non-clamp modes for used NPOT pairs unless full NPOT is positively reported. Mirror is treated conservatively as non-clamp, as are all U/V/W modes. Synthetic restrictive-capability tests supplement modern-driver execution. |

For normalized LOD ranges, patterned single-mip GPU tests cover both nearest
and linear mipmap filters on the RTX 3090 Ti at FL9_1. Explicit-LOD sampling
and historical FL9_1 driver behavior remain unverified. The current compiler
uses FL9_1 profiles even on higher-level devices; a selected entry point that
can reach `textureSampleBias` is conservatively rejected, including through
helper functions. Unrelated entries remain usable.
The tested FXC emitted an unclamped legacy `texldb`, for which Microsoft
documents undefined behavior outside [-3, 3] at ps_2_0. This restriction is
not a general solution for valid biased sampling.

The backend does not expose a public adapter. In particular, the wgpu downlevel
anisotropy flag does not describe this private maximum of 2 by itself; public
validation or emulation must be resolved before any such capability is exposed.

The nondefault `dx11-test-fixture` feature provides an unsafe, non-enumerated
adapter solely for a controlled core integration test. Normal `Adapter::open`
still fails. The fixture reports driver-queried RGBA8 and BGRA8 color and copy
capabilities and D24S8 attachment capability, and no surface capabilities. The
fixture checks transient texture and view creation and invalid usage or format
rejection, but does not exercise a transient render pass through core. Its
test-only limits and format report are not a general adapter contract. The
fixture verifies core buffer operations and an offscreen triangle, not ordinary
public adapter selection or presentation.

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

Ranges use four-byte-aligned offsets and nonzero sizes of at most 64 KiB;
the logical size need not be a multiple of 16. At bind replay the exact range
is copied from the authoritative GPU buffer to a CPU-readable staging buffer,
then placed in zero-padded CPU scratch and uploaded to a separately allocated,
16-byte-aligned native constant buffer. This preserves subrange isolation and
queued GPU writes, but mapping can stall once per binding per draw. Each group
bind also allocates an independent
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
GPU tests verified 4-, 5-, 12- and 20-byte ranges and zeroed native padding.
An additional test verified a 5-byte dynamic range following a GPU copy and a
5-byte range ending exactly at the end of a 65-byte native source buffer;
historical-driver behavior for an
odd-sized buffer copy remains unverified. The controlled core fixture enables
`BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED` to test these ranges and rendered with a
dynamic offset. Without this flag core rejected its short uniform shader. The
fixture flag is not a public adapter capability; normal enumeration remains
disabled. Separate GPU regressions verify selected integer-uniform bitcasts in
vertex and fragment shaders and a fragment comparison of 2^24 and 2^24+1. They
do not establish general integer shader semantics or historical GPU support.

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
shader. Bind groups retain each texture's dimensions and mip count and each
sampler's addressing and translated LOD restrictions. Before indexed or non-indexed draws, the encoder checks the
current pipeline's pairs against the current bound groups; changing either state
changes the validation inputs. Missing identities fail closed, and an invalid
draw causes command encoding to fail before submission.

For FL9, a used NPOT texture with a wrapping sampler is accepted only when the
`D3D9_OPTIONS` full-NPOT query returned true. A false or unavailable query uses
the conditional restriction. POT/wrap and NPOT/clamp pairs work even when an
unused cross-product would be invalid. Multi-mip sampled views are POT-only and
span a suffix including the resource's last mip; sampler checks include all
address axes and use the view-relative mip count and LOD. Strict suffix sampling
passed for both formats on the RTX FL9_1 device, and the controlled core suffix
fixture produced 64 blue pixels. Synthetic tests exercise restrictive
capabilities; the RTX driver reports full NPOT support even when forced to FL9_1.
The HAL returns its existing generic `DeviceError::Unexpected` on draw rejection.
Binding arrays, comparison samplers, storage and vertex sampling remain
unavailable; normal adapter enumeration remains disabled.

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
