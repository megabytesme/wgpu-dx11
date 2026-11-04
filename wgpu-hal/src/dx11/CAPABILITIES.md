# D3D11 capability and public-adapter contract

This is an inventory of the private backend, not a wgpu capability report.
Ordinary D3D11 adapter enumeration remains empty. The internal model separates
feature-level minima, per-device query results, the restricted implementation,
and the public-adapter gate. `check_private_texture_2d` is a conservative
predicate for the private RGBA8, BGRA8, D24S8, R8_UNORM, R8G8_SNORM,
R8G8B8A8_SNORM and BC1_UNORM subsets; it is not a complete texture validation
rule.
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
Microsoft's required-format table maps legacy L8 to R8_UNORM, V8U8 to
R8G8_SNORM, and Q8W8V8U8 to R8G8B8A8_SNORM. Each requires 2D texture and
shader-sample support beginning at FL9_1. It also requires DXT1 / BC1_UNORM
for 2D texture sampling at FL9_1. The backend records these minima separately
from the selected device's queried format bits.

Conditional NPOT textures at FL9 require one mip and restricted addressing. Full NPOT and
comparison-shadow support are separately queried using `CheckFeatureSupport`;
a newer runtime or driver may return E_INVALIDARG for an unknown query, which
is retained as unavailable. `CheckFormatSupport` is queried only for RGBA8,
BGRA8, R8_UNORM, R8G8_SNORM, R8G8B8A8_SNORM, BC1_UNORM and D24S8, and
`CheckMultisampleQualityLevels` for RGBA8 at four samples.
A positive bit or quality count is a driver observation, not an implemented HAL
feature or a guarantee of a valid combination of descriptors. The RGBA8 2D
predicate intersects format bits with the implemented format, usage, shape and
single-sample restrictions. It uses the guaranteed dimension as a conservative
portable bound, not a measured hardware maximum.

References: [feature-level table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro),
[10Level9 device constraints](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device),
[10Level9 draw constraints](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context),
[D3D9 primitive-count compatibility limits](https://learn.microsoft.com/en-us/windows/win32/api/d3d9caps/ns-d3d9caps-d3dcaps9),
[primitive topologies](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-primitive-topologies),
[format support queries](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11device-checkformatsupport),
[required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats),
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

On 2026-11-03, the same adapter at requested and selected FL9_1 returned
R8_UNORM support mask `0x0002d3e0`, including 2D and shader-sample support.
The latter is the 10Level9 filtered-sampling indication. A strict test uploaded
a padded 5×4 image from byte offset 1, sampled it with nearest filtering into
RGBA8 and matched all 80 output pixels. R8 linear filtering is enabled only
when the query indicates filtered sampling, but has not been GPU-tested here;
copy-source, readback and render-target operations remain unimplemented as
noted in the transfer matrix.

The same adapter returned R8G8_SNORM support mask `0x00021320` at requested
and selected FL9_1. A strict test uploaded padded signed RG pairs, sampled and
remapped both channels to RGBA8, and checked all 80 output pixels within one
UNORM code value for conversion rounding. Render-target, copy-source, readback
and mip-chain uses remain unsupported.

The same adapter returned R8G8B8A8_SNORM support mask `0x000213a0` at
requested and selected FL9_1. The strict test uploaded padded four-channel
signed data, sampled and remapped all channels to RGBA8, then matched all 80
pixels within one UNORM code value for conversion rounding. Render-target,
copy-source, readback and mip-chain uses remain unsupported.

The adapter returned BC1_UNORM support mask `0x000213a0` at requested and
selected FL9_1. The strict test uploaded four solid-color blocks through a
buffer with a nonzero offset and padded block-row pitch, then replaced one
block through a second staged partial upload. Sampling into RGBA8 matched all
256 output pixels. The private subset accepts only one-mip, one-layer,
4-aligned BC1_UNORM textures with RESOURCE and/or COPY_DST usage. Updates must
cover complete aligned blocks. BC1 copy-source, readback, render-target,
mip-chain, non-block-aligned dimensions and BC1 sRGB views remain unavailable.

## FL9_1 inventory and gaps

All rows block public exposure until their gaps are addressed or safe validation
is established. Hardware availability below means the FL9_1 contract, not a
claim about a particular driver's optional capabilities. Test names refer to
private HAL tests; success on one modern adapter is not proof for all FL9 GPUs.

| Category | Hardware and private implementation | Coverage; wgpu mapping and remaining work |
| --- | --- | --- |
| Device / adapter | Native DXGI hardware discovery and explicitly selected feature levels exist. HAL adapter is uninhabited and enumeration is empty. | Hardware probe and zero-adapter tests. No `Limits`, `Features`, `DownlevelCapabilities`, format capability set, or wgpu device is advertised. A new adapter must be truthful and satisfy every valid operation it permits. |
| Buffers, maps, copies | Native buffers and copies exist. Private HAL implements vertex, index, uniform, copy and explicit CPU-shadow mappings; vertex+index is rejected at 9_1/9_2. Storage and indirect usages are absent. | Copy, map, initialization and ownership tests. `UNRESTRICTED_INDEX_BUFFER` and limits can describe some restrictions, but mapping coherency, initialization, complete usage combinations and lifetime validation need an audit. |
| 2D textures / formats / views | FL9_1 supports multiple native formats subject to individual format flags. Private RGBA8 and BGRA8 2D textures have one layer and sample; restricted copy-only, sampled and color-attachment POT textures can have multiple mips. R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM are limited to one-mip, one-layer sampled/upload resources and use positive per-device 2D/shader-sample queries. BC1_UNORM additionally requires dimensions aligned to 4×4 blocks and supports block-aligned uploads only. D24S8 remains single-mip and attachment-only. | Transfer, view, clear and draw readback tests. Padded-offset uploads and sampling for the three sampled-upload-only uncompressed formats passed on the RTX at FL9_1; a padded BC1 upload plus partial block replacement matched all 256 output pixels. These formats remain non-renderable and cannot be copied from or read back in this private subset. Per-format capabilities and numeric limits must match creation, copies, initialization and view behavior. Other formats and dimensions need implementation or validation. |
| Mips, arrays, NPOT / sampling | POT RGBA8 and BGRA8 textures can have mip chains when format support is reported. Sampled suffix views are native; other supported mip ranges use a GPU clone. Color-attachment views select one mip. Sampled and color-attachment usages can coexist, with optional copy usages. Arrays and NPOT mip chains remain unavailable. | Strict FL9_1 tests verify per-mip transfers, attachment rendering, suffix and clone-backed non-suffix sampling on the RTX. Public exposure remains blocked. |
| Samplers | Independent point/linear min/mag, clamp/wrap/mirror and anisotropy up to 2 are implemented in the private HAL. Border addressing and comparison samplers are rejected; translated LOD ranges are checked against each used sampled view. | Patterned FL9_1 GPU readback covers filter and address modes, sampled mip levels and sampler switching. Anisotropy 2 creates and samples but visual anisotropic quality is unverified. Public feature/limit representation remains incomplete. |
| Bind groups / registers | Up to eight private groups may contain uniforms, sampled textures and samplers, with aggregate limits of thirteen uniforms, eight textures and sixteen samplers. Each draw checks only texture/sampler relationships used by the selected shader entry points. | Sparse multi-group, dynamic-offset, pipeline-transition and cross-group sampled-texture GPU tests cover selected configurations; FXC limits distinct sampled relationships and constants. Counts in `Limits` cannot represent all shader, visibility, compatibility or cross-group restrictions. Public exposure remains blocked. |
| Vertex / index formats | Native FL9 has wider IA support than the HAL. Private pipelines accept up to sixteen sparse vertex- or instance-rate streams, including zero-stride constant streams, and restricted float, unorm and `Snorm16x2`/`Snorm16x4` formats. FL9_1 requires input-assembler support for both SNORM formats; pipeline creation relies on that guarantee, while the hardware test queries the actual device. Attributes retain four-byte alignment. Multi-slot configurations support triangle and point lists, nonindexed line and triangle strips, and Uint16/Uint32 indexed triangle, point and line lists with translation when needed. Indexed LineStrip and TriangleStrip use CPU restart splitting; native FL9 strip-cut support is not assumed. On FL9_1, overlapping elements in a logical stream are assigned to distinct native slots bound to the same buffer. | Indexed and non-indexed readback, sparse-slot, overlap, zero-stride, SNORM vertex and rejected-index tests, plus controlled core renders. Strict FL9_1 line and triangle strip rendering checks both index formats, restart-separated runs, winding, wide primitive gathering, format mismatch rejection and out-of-range discard. Limits and downlevel flags cover only part of the contract; other core-valid vertex formats and layouts need implementation or validation. Overlapping physical reads, stream offsets and maximum strides remain unverified on historical 9_1 drivers. SNORM vertex support does not imply SNORM texture support. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures, selected vertex entries declaring `vertex_index` or `instance_index`, and fragment entries declaring `primitive_index` are rejected by the private shader path. Checked texture loads reachable from the selected entry point remain unavailable. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests, including a sampled shader with an unused texture-load helper. FXC rejects fragment `position`, `front_facing` and `primitive_index` inputs in the tested profile. WGSL validation and feature bits do not represent arbitrary FXC instruction pressure or every translation restriction. Shader creation and pipeline validation need conservative diagnostics or emulation; successful Naga validation is insufficient. |
| Pipeline / raster / blending | Private graphics pipeline supports line lists, line strips, triangle lists, nonindexed triangle strips and point lists with configurable front-face winding and front/back culling for triangles, one color target, one sample and a small set of blend states. All RGBA channel write masks are supported for the single target. Exact premultiplied-alpha blending uses coupled `ONE` / `INV_SRC_ALPHA` add factors for both color and alpha on linear RGBA8 and BGRA8. On FL9_1, straight alpha, inversion and vignette blending reject simultaneous RGB and alpha writes; masked-channel cases use coupled factors. Enabled blending requires a positive format query. Out-of-target viewports use a separately compiled vertex position transform when available. A D24S8 attachment supports native Always/LessEqual depth comparisons and emulated Never; optional depth writes and one-sided stencil comparison/operations are implemented with identical front/back state. FL9_1 rejects differing front/back stencil face states; core now represents this restriction through `DownlevelFlags::TWO_SIDED_STENCIL` and rejects a differing descriptor before HAL. Never with active stencil maps the stencil pass operation to the requested depth-fail operation, forces guaranteed Always depth comparison, and suppresses color/depth writes. Signed constant triangle depth bias is supported with selected-entry restrictions; slope and clamp bias remain unsupported. | Textured depth ordering, stencil Replace/Equal/reference updates including depth-fail emulation, both windings and culling, viewport, all 16 channel masks and masked blend readback tests. Stencil bits are validated indirectly through subsequent stencil comparisons because this HAL has no stencil readback path. Strict FL9_1 premultiplied tests checked six full images across RGBA8 and BGRA8; the controlled core fixture checked 64 pixels with a one-unit rounding tolerance. Remapped shaders may exceed FL9 compiler limits even when the original compiles; such draws remain rejected. Full-channel separate-alpha modes remain unavailable by default; RTX-only diagnostic tests explicitly bypass this restriction and are not a portability guarantee. |
| Render passes / targets | One color target and an optional D24S8 depth/stencil attachment support load, clear or don't-care and store or discard. Discard retains native contents, which remain undefined to HAL callers. Restricted transient color and depth attachments use ordinary textures without memoryless savings; transient combinations with sampled or copy usage are rejected. Private read-only passes require LOAD and STORE on both aspects and pipelines without depth or stencil writes; depth sampling aliases remain unsupported. Depth-only clears and draws without a fragment stage are supported. No resolve, timestamps, occlusion, multiview or multiple color attachments. | Render/readback tests include a depth-only prepass, read-only depth preservation and transient color/depth attachment passes on the tested FL9_1 device. Aspect-specific depth views, public discard initialization and general pass combinations still need implementation or validation before public exposure. |
| Draw / instancing | Direct draws are split without splitting line-list pairs or strip connectivity; restricted instance-rate streams use repeated non-instanced native draws with stride-zero bindings adjusted by `first_instance` and instance number. FL9_1 does not use native per-instance IA classification. Triangle lists, line lists, line strips, nonindexed triangle strips and point lists are supported with the restricted vertex and instance streams. Line-list batches contain at most 65,534 vertices (32,767 lines) because the 9_1 compatibility cap also limits unique vertices when the primitive maximum is 65,535; line strips contain at most 65,535 vertices and overlap one endpoint when split. Point batches rebase their vertex bindings and contain at most 65,535 points. Indexed point draws accept Uint16 and Uint32. Indexed LineList accepts both formats, preserves pairs through compact windows, gathers wide pairs and ignores an incomplete tail; each native call is limited to 131,070 indices and effective indices to 65,534. Indexed LineStrip accepts Uint16 and Uint32, splits runs when the resolved `baseVertex + index` equals the format marker, overlaps one vertex at native-call boundaries, compacts effective indices to R16 windows and gathers edges whose endpoint span exceeds 65,534. Each native line-strip call is limited to 65,536 indices. Indexed TriangleStrip accepts Uint16 and Uint32 and resolves restart markers before splitting runs. It preserves alternating winding by expanding each run to triangle-list order, then submits bounded compacted Uint16 windows or gathers wide triangles for every vertex stream. Expanded native calls are limited to 65,535 triangles. Every non-marker fetch is prevalidated against every vertex-rate stream and any invalid fetch discards the entire draw. Instance-only indexed triangle draws and shaders using `instance_index` remain unsupported. Index-dependent indexed draws check effective indices via GPU staging. Native-safe u16 triangle-list indices use native buffers; other valid triangle-list indices use ordered rebased u16 windows or per-stream GPU gathers. Out-of-bounds vertex fetches discard the entire draw. | Strict tests cover direct and indexed line and triangle strips, both index formats, restart-separated runs, markers created or removed by signed base vertices, all triangle-strip front-face/cull combinations, whole-image equality against equivalent triangle lists, a Uint32 wide-triangle gather, format mismatch rejection, bounded compaction and whole-draw invalid-fetch discard; restricted instance-rate rendering, translated windows and point draws with a 65,537-point split. The controlled core fixture checks indexed Uint16/Uint32 LineList, LineStrip and TriangleStrip output; ordinary enumeration remains empty. GPU index staging and wide-primitive gathers may stall/allocate scratch; historical device behavior is unverified. General instancing remains unsupported. |

FL9_1 native indexed point lists are unavailable. The private HAL stages the
GPU-authoritative index buffer at replay and plans ordered nonindexed point
runs with rebased vertex streams. It validates every effective index and stream
range before issuing a draw, and discards the whole draw on an invalid vertex
fetch. Mapping can stall the CPU, and scattered or repeated indices can require
many native draws. The index-independent fast path avoids staging when all
vertex-rate streams have zero stride or there are no vertex-rate streams; it
still checks index-buffer and constant-stream ranges. Shader `vertex_index`
remains rejected. These private paths do not establish a public adapter contract.

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
validation question; the current indexed triangle-only subset produces degenerate
geometry from all-zero streams, so a completed draw provides no visible proof
of that case. Historical hardware has not been tested.

FL9_1 direct triangle-list batches are conservatively limited to 65,535
vertices, preserving whole triangles and instance order. This accounts for a
documented D3D9 unique-vertex caveat; whether 10level9 independently guarantees
or emulates that restriction is unresolved. Indexed batches use their separate
index limits. Large native start offsets still need historical-driver testing.
| Queries / synchronization / readback | Native event queries back the private fence; this is not a public query-set implementation. Queue submission serializes the immediate context, and staging readback waits for completion. Shared HRESULT classification recognizes device hung, removed, reset and driver-internal-error results as device loss. | Fence, sequencing, copy and GPU readback tests; CPU tests cover device-loss HRESULT classification. Core rejects occlusion query sets when `OCCLUSION_QUERY` is absent; timestamp queries require a feature. The private HAL rejects both, and some destroy paths for resources that cannot be created still panic. Actual device removal/recovery, all lifetime/error paths and debug-layer diagnostics remain unverified. |
| Compute / storage | No native compute shader or UAV support at FL9_1; private HAL accepts paired empty compute passes but rejects compute pipeline creation, dispatch and storage usages. Other APIs may require CPU or graphics emulation. | Strict hardware test verifies ordered copies around an empty pass and rejects dispatch and invalid pass sequences; feature/limit gating alone is insufficient until core can safely create and validate a downlevel device without compute and all associated paths are audited. No compute/storage feature is exposed. |
| Presentation | FL9 hardware can support display through platform/format-specific paths. Private backend has restricted WinRT CoreWindow and borrowed Win32 HWND BGRA8 FIFO paths. | Prior UWP diagnostic and hidden HWND HAL tests cover clear and clone-sampled rendering, readback, present, resize and recreation with a completed command buffer retained. On the RTX at FL9_1 the clone-sampled test read BGRA `[0, 0, 255, 255]` and present returned `Ok`. Swapchain calls are serialized against the immediate context, but synchronous window callbacks can still deadlock on reentry; pending presentation completion, visible output, device loss and surface capability reporting remain unverified. No public surface is advertised. |

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
Microsoft's [10Level9 device restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
specify that FL9_1 does not support two-sided stencil. The private FL9_1 path
therefore accepts only equivalent front and back face states; asymmetric active
states fail at pipeline creation.
Depth and stencil clear/load flags are handled independently, including
depth-only passes used by core initialization patterns. Depth-only drawing
uses a trivial pixel shader with no render target because the tested FL9_1 FXC
profile rejects a pixel shader that writes no target. Historical-driver
behavior of this fallback is unverified. The private pipeline
supports native Always and LessEqual depth comparison with optional writes.
Never is emulated by suppressing validated draws when stencil is inert; when
active stencil must observe the depth-fail branch, a separate Always-based
state remapping is used below. Draw suppression remains restricted to the
current side-effect-free, read-only-resource and no-query subset. Future shader
side effects or query support require revisiting it.
Stencil state maps all eight WGSL operations and comparison functions to the
native D3D11 depth-stencil descriptor, with the low eight bits of the masks.
This describes translation, not evidence that every mapped state works on all
FL9_1 drivers; the strict hardware test executes Replace and Equal. Only
identical active front/back states are supported; the general public
`depth_stencil` capability remains false. The reference is retained in command
replay, rebound when either the reference or pipeline changes, and reset at each
render pass. Strict readback verifies Replace writes indirectly by using a later
Equal comparison, including a wrong-reference rejection and state transitions.
The DSV itself still cannot be read back through this HAL, so tests do not claim
direct inspection of stencil storage. Always-passing, non-writing stencil
states continue to canonicalize to disabled stencil.

Depth comparison Never remains draw suppression only when stencil is inert. If
active stencil must observe the depth-fail branch, the backend uses the
guaranteed Always depth comparison, remaps StencilPassOp to the requested
StencilDepthFailOp, and disables color and depth writes. This avoids depending
on a depth comparison outside the documented FL9_1 Always/LessEqual minimum.
The test confirms that the following Equal comparison observes the write.
Private read-only passes require LOAD|STORE for both aspects and a pipeline
without depth or stencil writes. They use an ordinary DSV; sampled aliases
remain prohibited, so this does not establish the public
`READ_ONLY_DEPTH_STENCIL` flag.
The broad `depth_stencil` capability remains false. Texture and view descriptors
may declare READ, WRITE or READ|WRITE, but an actual pass uses one usage.
Strict GPU tests verify textured depth ordering, initialization, independent
clear/load and discard behavior, no-write behavior and view lifetime. The no-op
stencil tests verify rendered color but cannot read back stencil bits. They do not
prove general stencil operations or historical-driver compatibility. The
documented FL9_1 minimum guarantees Always and LessEqual depth comparisons;
other native comparison guarantees remain unresolved. Never uses the restricted
draw-suppression emulation; the other comparisons remain rejected.

Triangle pipelines support signed constant D24S8 depth bias with zero slope
scale and zero clamp. Nonzero bias is rejected for point lists and for a selected
fragment entry writing `frag_depth`; unused depth-writing entries do not cause
rejection. Native slope-scale and nonzero-clamp bias remain unavailable. The
strict FL9_1 RTX test checked five complete images, and two controlled core
cases checked 64 pixels each. FXC rejected the zero-bias direct-only
`frag_depth` entry with X4530, while a structured color-and-depth entry
compiled. Microsoft specifies 32-bit bias arithmetic explicitly for FL10 and
higher; precision on historical FL9 hardware remains unverified. See
[D3D11 depth bias](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-output-merger-stage-depth-bias)
and [10Level9 rasterizer restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).

| Transfer | Implementation and restrictions | Coverage |
| --- | --- | --- |
| Buffer to texture | Retained GPU buffer copied to a CPU-readable staging buffer at replay; mapped bytes feed `UpdateSubresource` with an explicit destination box and checked row pitch. This reads GPU contents at execution time, not a CPU shadow. R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM accept one-mip, one-layer sampled/upload textures only. BC1_UNORM uses 4×4 texel blocks, 8-byte block rows, 4-aligned dimensions and block-aligned update regions. | RGBA8/BGRA8 offset and padded full/partial uploads, GPU-written source and ordered submissions. All three sampled-upload-only uncompressed formats passed padded-row, nonzero-source-offset upload and full rendered readback tests on forced FL9_1. BC1 passed an offset padded full upload, a partial block replacement and 256-pixel rendered readback. |
| Texture to texture | Native GPU `CopySubresourceRegion` between compatible RGBA8 or BGRA8 textures, including different mips of one texture, with an explicit checked source box and DEFAULT source and destination resources. Copies within the same subresource and cross-format copies are rejected. R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM copies remain unsupported. | Full and partial copies, different overall dimensions, cross-resource and same-resource mip copies, zero-preserving destination and render-to-copy-to-sample. |
| Texture to buffer | A region is copied to a retained CPU-readable staging texture and mapped; each mapped row is written into the destination GPU buffer at the requested pitch and offset. R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM readback are unsupported. | Full and partial RGBA8/BGRA8 source regions, nonzero buffer offsets, row padding and complete image comparison. |

The strict FL9_1 RTX test checked partial same-texture copies between distinct
mips in both directions for RGBA8 and BGRA8, including all affected and
untouched texels and padded readback bytes. Two more GPU cases rendered into a
mip, copied to another mip of that texture and sampled its clone-backed view;
each checked 64 blue pixels. Controlled core tests checked all mip texels for
both formats and rejected a same-mip copy. Before the change, the strict
baseline rejected a valid distinct-mip copy. Historical FL9 hardware remains
unverified, and this does not enable public adapter enumeration.

HAL texture creation eagerly uploads zeroes before returning, including for
partial-write targets and each mip of a supported chain. Allocation and uploads
are proportional to texture size; staging maps in buffer/texture transfers may
stall the CPU. Native
diagnostic texture constructors remain separate and can create uninitialized
resources. The private color HAL accepts one-layer, one-sample RGBA8 or BGRA8
2D textures and enforces feature-level size limits. R8_UNORM, R8G8_SNORM and
R8G8B8A8_SNORM support only a single-mip, single-layer 2D texture with RESOURCE
and/or COPY_DST usage; their actual `TEXTURE2D | SHADER_SAMPLE` bits are
checked at creation. They are not render targets, copy sources or readback
sources. `SHADER_SAMPLE` also determines whether these views are treated as
filterable. The strict RTX R8_UNORM test observed support mask `0x0002d3e0` and
verified nearest sampling only. Multiple mips require POT
dimensions, one of the supported copy-only, sampled or color-attachment usage
combinations and positive per-format
`D3D11_FORMAT_SUPPORT_MIP`, texture and shader-sample bits; copy-source usage
or color-attachment usage also requires render-target support.
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

POT RGBA8 and BGRA8 mip chains also support logical `COPY_DST` alone when the
required format queries succeed. The strict FL9_1 RTX test uploaded three mip
levels and checked an untouched zero-initialized fourth level. To inspect the
upload-only texture, the test used an explicit native GPU-to-GPU diagnostic
copy into a separate render-target-capable texture, then read that texture;
the original texture still rejects logical copy-source, readback, sampled-view
and render-target-view use. The controlled core test submitted three writes and
checked completion and forbidden view/readback validation; it did not read
back the upload-only texture. Historical drivers remain unverified and ordinary
public adapter enumeration remains disabled.

Sampled multi-mip textures use `COPY_DST | RESOURCE` with optional `COPY_SRC`,
or `RESOURCE | COLOR_TARGET` with optional `COPY_SRC` and/or `COPY_DST`.
Native shader-resource views span a suffix through the resource's last mip.
A supported non-suffix sampled view uses a separate native texture
with the selected mip range; its source mips are copied to the clone at each
draw before sampling. This GPU work has a per-draw copy and storage cost, but
does not require logical `COPY_SRC` usage. The original texture is retained and
used for feedback checks, so cloning cannot hide a same-pass alias. A sampled
texture without `COPY_SRC` cannot be directly read back; a texture with only
`COPY_DST | RESOURCE` uses only a native shader-resource bind. Color-attachment
mip chains use `COLOR_TARGET` with optional `RESOURCE`, `COPY_SRC` and/or
`COPY_DST`, excluding `TRANSIENT`; each attachment view selects one mip. A view
with both sampled and attachment usages can select any single mip: a nonfinal
mip uses a clone SRV and an original-resource RTV.
In the same pass, native suffix and clone-backed SRVs may sample the same
texture as an RTV only when their mip ranges are disjoint; overlapping ranges
are rejected. For clone refresh, replay snapshots the active RTV and DSV,
unbinds both, clears pixel shader resource slots, performs the copies and
restores the attachments; the current pixel shader resources are rebound
before drawing. Sampled view metadata and the selected shader relationships drive
LOD compatibility validation at draw time; no cross-product check rejects an
unused texture/sampler pair. On the tested RTX FL9_1 device, a same-pass
pipeline and bind-group transition retained caller-destroyed sampled resources
through submission and produced 32 green and 32 blue pixels for each format.

Strict FL9_1 tests on the RTX 3090 Ti verified four initialized mip levels for
RGBA8 and BGRA8, padded partial uploads, copies between different textures and
mip levels, readback, metadata views and retained resources. The controlled core
fixture checked all 43 pixels of a four-level chain. Native FL9 shader-resource
views must include the smallest resource mip; clone-backed non-suffix views
emulate the restricted supported ranges. Sixteen strict GPU cases passed on
the RTX across both formats, including a recorded buffer-to-texture upload
followed by an indexed draw in the same submission. The controlled core fixture
checked 64 yellow pixels after updating the source. Historical GPU behavior is
unverified.

Individual-mip color attachment tests passed for RGBA8 and BGRA8 on the RTX
at FL9_1, including full-chain readback checks. The controlled core fixture
checked all 43 pixels for each format and submitted a render-only mip pass;
the latter has no direct pixel readback.

Combined sampled-and-attachment mip transitions passed six strict GPU cases for
both formats on the RTX FL9_1 device, and the controlled core fixture checked
64 output pixels in each of four cases. These cover separate usage-specific
views and last-mip dual-use views across passes, not same-pass feedback.
An additional eight strict GPU cases covered nonfinal dual-use views for both
formats; the core fixture's default dual-usage view produced 64 matching pixels
for each format after render-to-sample transitions.

Same-resource disjoint native suffix SRV and RTV use passed six strict GPU cases
across both formats on the RTX at FL9_1; controlled core cases checked 64 pixels
per format. [D3D11.3 functional specification §5.3.1](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)
permits simultaneously binding disjoint subresources for input and output.
Two additional strict clone-backed cases, one per format, produced 32 green and
32 blue pixels each across a same-pass depth-tested transition. Controlled core
cases verified the same depth-preserving output for both formats.
[D3D9 sampler documentation](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-registers-sampler)
warns against binding a texture as both render target and texture at draw time;
that API statement does not establish historical FL9 D3D11 driver behavior.
Historical hardware remains unverified, and ordinary public enumeration is disabled.

HAL transfer commands use explicit regions. No staging-to-default native copy with
a null source box is used, avoiding the documented 9_x corruption case for
mismatched dimensions or destination offsets. Core validates empty regions and
returns before calling HAL; direct private HAL empty-region transfers remain
rejected. Broad texture formats, multisampling, NPOT mip chains and arrays
remain unsupported. A modern GPU forced to 9_1 does not
validate every historical runtime or driver; public adapter exposure remains
disabled.

## FL9_1 sampler contract

| Category | Native and driver contract | Private backend and coverage |
| --- | --- | --- |
| Address modes | Clamp, wrap and mirror are available; `MIRROR_ONCE` is not the same as mirrored repeat and is unsupported. Ordinary border addressing is not generally available at 9_1/9_2. | Clamp, repeat and mirrored repeat map independently on U/V/W. Patterned GPU tests cover repeat, mirror and mixed U/V. Border addressing is rejected before native creation; all wgpu border colors may be carried only when no border addressing is used. |
| Min/mag filtering | Point and linear filters can be specified independently. | All four min/mag combinations map to D3D11 filters. GPU readback exercises both magnification and minification with contrasting texels; filter outputs differ. |
| Mipmap filtering | Point and linear mip filters exist. | Both are tested with sampled full-chain POT textures; suffix and restricted clone-backed non-suffix views passed on the RTX. General mipmap behavior remains unavailable. |
| LOD | FL9_1 native MinLOD must be integral and MaxLOD must be FLT_MAX. | A normalized minimum requires a one-mip view. When only a finite maximum is raised and the minimum is preserved, a used multi-mip view requires the original maximum to reach its last mip and anisotropy 1. Anisotropic sampling with a raised finite maximum remains one-mip-only. Other unrepresentable mixed-filter and anisotropic ranges are rejected. FL9 does not specify whether clamping precedes min/mag selection. FXC rejects `SampleLevel` for the tested pixel profile with X4532; explicit-LOD sampling is not claimed. |
| Anisotropy | FL9_1 native maximum is 2; an upper limit alone is not a visual-quality guarantee. | The private backend accepts 1 or 2, requiring linear min, mag and mip filters for 2. Native creation and a sampled GPU output passed; anisotropic quality on minified textures is unverified. Values above 2 are rejected even on a modern GPU forced to 9_1. |
| Comparison / border | Comparison filtering is not generally supported at 9_1; ordinary border addressing is not normally allowed. Optional 9_x shadow support is separate. | Comparison and border addressing remain rejected. On the RTX forced to 9_1, ordinary border sampler creation returned E_INVALIDARG. No optional wgpu border or comparison feature is advertised. |
| NPOT | Conditional NPOT requires one mip and restricts addressing; full NPOT is an optional driver query. | Shader-aware validation rejects non-clamp modes for used NPOT pairs unless full NPOT is positively reported. Mirror is treated conservatively as non-clamp, as are all U/V/W modes. Synthetic restrictive-capability tests supplement modern-driver execution. |

For normalized LOD ranges, patterned single-mip GPU tests cover both nearest
and linear mipmap filters on the RTX 3090 Ti at FL9_1. Explicit-LOD sampling
and historical FL9_1 driver behavior remain unverified. The current compiler
uses FL9_1 profiles even on higher-level devices; a selected entry point that
can reach `textureSampleBias` is accepted only for finite literal biases in
[-3, 3] without a sampling offset. Dynamic biases, out-of-range values and
biases passed through helper arguments are rejected, even when the argument is
a literal. All sampling pairs used by a selected entry containing bias must use
equal min/mag filters and anisotropy 1; normal view and LOD checks still apply.
Unrelated entries remain usable. The tested FXC emitted an unclamped legacy
`texldb`, for which Microsoft documents undefined behavior outside [-3, 3] at
ps_2_0. Nine strict FL9_1 GPU executions checked 64 pixels each, including a
fractional linear-mipmap blend; direct and indexed draws rejected incompatible
samplers. This restricted implementation does not cover all valid biased sampling.

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

The pipeline assigns textures and filtering or non-filtering samplers independently in ascending
group and logical binding order to compact fragment-stage `t` and `s` registers.
Uniforms from all groups are sorted by group and binding into `b0` through
`b12`; vertex `b13` is
reserved for viewport remapping and the disabled texel-load proof also reserves
`b13`. The same slot is used for a uniform in both
stages, and visibility is checked per stage. Layout identity and the same register map are
used by shader translation and command recording. Bind groups retain the native
views and samplers; recording rejects overlapping render-target feedback, while
supported disjoint native and clone-backed views can be used. Each draw binds
resources using the active pipeline's plan, and changing the layout passed to
`set_bind_group` invalidates groups at and after the first unequal prefix.

NonFiltering sampler layouts require nearest minification, magnification and
mipmap filters. Filtering layouts also accept nearest samplers. The sampler
class remains part of layout identity, including with sparse logical bindings;
switching layouts requires the corresponding group. Strict FL9_1 RTX tests
checked two full images across a layout transition. A controlled core test
checked 64 pixels and rejected a linear sampler with `WrongSamplerFiltering`.
CPU tests cover binding order, the non-filtering mask and original sampler
filter class. The pre-change strict baseline rejected the NonFiltering layout.
The layout can also declare an unfilterable-float binding for the already
supported RGBA8 and BGRA8 textures. The selected shader entry's actual sampling
pairs must use a NonFiltering sampler for such a binding; a Filtering sampler
layout is rejected even when the bound sampler happens to use nearest filters.
Unused incompatible cross-pairs do not cause rejection. Two strict RTX images
covered helper-function sampling and a selected-entry transition; a core case
checked 64 pixels and rejected an invalid filtering pair. CPU tests cover
cross-group pairs, helper and entry-point selection, and layout identity. The
pre-change strict baseline rejected the texture layout. This adds no new
unfilterable formats or comparison samplers. Historical drivers remain unverified
and public adapter enumeration remains disabled.
Render-pass boundaries clear context state.

Sampled texture and sampler entries may declare `FRAGMENT` or
`VERTEX | FRAGMENT` visibility. Visibility is retained per binding in layout
identity and stage mappings; combined visibility does not enable vertex-stage
texture or sampler use. A selected vertex shader that samples a texture is
rejected before drawing. Two strict FL9_1 RTX images checked fragment sampling
and a transition between these layouts; a controlled core case checked 64
pixels. CPU mapping and compiler guards cover stage identity and selected
vertex entries. The original strict baseline rejected the combined layout.
Historical FL9 drivers remain unverified, and public enumeration remains empty.

## Private uniform-buffer contract

The thirteen slots are a private layout allocation limit, not a claim that a
shader can use thirteen arbitrary buffers. The D3D11 API has fourteen application
constant-buffer slots per stage, while FL9_1 shader profiles have smaller legacy
constant budgets. FXC compiles 32 referenced pixel float4 constants and rejects
33 with X5200. A vertex shader with a dynamically indexed `float4[255]` compiles;
arrays of 256 and 257 fail with X4507 at constant register index 256. The
forced-FL9_1 RTX 3090 Ti also accepts `CreateVertexShader` for the 255-element
profile. A strict forced-FL9_1 WGSL render now statically reads vertex c254 and
pixel c31 from separate uniform arrays. Changing the vertex value moves the
triangle and changing the pixel value changes its output color in readback.
This establishes execution of those final tested registers on the RTX, not
dynamic indexing or historical-driver behavior; the private WGSL path still
rejects runtime integer indexing. Instruction pressure and compiler packing
can impose earlier limits on ordinary shaders. FXC errors fail pipeline
creation.

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
public adapter must enforce its reported wgpu alignment through core. For the
selected FL9 shader entry, checked and unchecked compilation rejects runtime
integer values, operations, conversions and bitcasts, integer-containing
uniforms or interfaces, switches, and non-folded global initializers and
overrides. This also rejects runtime integer-based vector indexing. Proven
static literal indices and float-only paths remain available. Matrix-containing aggregate stores are rejected
in checked and unchecked shaders because Naga's current HLSL special lowering
can lose writes or omit an index clamp. The conservative guard also rejects
unrelated stores to such aggregates and shaders exceeding its analysis budget.
General dynamic uniform indexing and broad public adapter exposure remain
unsupported. GPU coverage exercises multiple
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
disabled. Three earlier RTX GPU results for integer-uniform bitcasts and a
comparison around 2^24 are superseded: the original integer entry points now
fail linkage, and the updated tests render float-only controls. An FXC probe
showed the embedded legacy pixel shader rounding 16777217 to 16777216 and a
legacy vertex shader lowering a bitcast to numeric float arithmetic. Modern
forced-FL9 execution does not establish historical-driver integer semantics.

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
unused cross-product would be invalid. Multi-mip sampled views are POT-only;
native suffix views include the resource's last mip and other supported ranges
use per-draw GPU clones. Sampler checks include all address axes and use the
view-relative mip count and LOD. Strict suffix and non-suffix sampling passed
for both formats on the RTX FL9_1 device; the controlled core suffix fixture
produced 64 blue pixels and its non-suffix fixture produced 64 yellow pixels
after a source update. Synthetic tests exercise restrictive
capabilities; the RTX driver reports full NPOT support even when forced to FL9_1.
The HAL returns its existing generic `DeviceError::Unexpected` on draw rejection.
Binding arrays, comparison samplers, storage and vertex sampling remain
unavailable; normal adapter enumeration remains disabled.

The disabled texel-load proof reserves `t0`, `s15` and `b13`; it cannot simply
be switched on with this allocator. Enabling it requires coordinated register
reservations and collision tests. The active sampled path does not reserve
those slots or enable the proof.

`wgpu-core/src/device/resource.rs` rejects D3D11 passthrough shader modules
with `NotCompiledForBackend`; it does not use an unreachable arm. The matching
test in `tests/tests/wgpu-gpu/naga_capabilities.rs` records that behavior. This
is separate from the normal WGSL-to-HLSL path. Active differing front/back
stencil states now require `DownlevelFlags::TWO_SIDED_STENCIL`, allowing an
adapter without FL9_1 two-sided stencil to reject the pipeline before HAL.
Tests should continue to cover rejected legal-looking requests at core
validation boundaries and ensure HAL is never called for unsupported
combinations. Merely choosing generous limits would still expose unsupported
paths, including methods that currently panic.
