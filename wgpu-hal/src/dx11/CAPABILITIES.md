# D3D11 capability and public-adapter contract

The opt-in `dx11` feature now enumerates physical DXGI adapters that can create
the explicitly requested FL9_1 device and exposes the selected-device
capability projection for public wgpu rendering. Its constrained Win32 HWND
surface contract is BGRA8 sRGB FIFO, opaque composition, render-attachment
usage and one frame of latency; configure, acquire and present are
owner-thread-only. The reported feature set is empty and the downlevel contract
is not WebGPU-compliant. This is a restricted FL9_1 adapter report, not a claim
of general wgpu support. The
internal model separates feature-level minima, per-device query results, and
implemented backend paths. `check_private_texture_2d` is a conservative
predicate for the private RGBA8, BGRA8, D24S8, R8_UNORM, R8G8_UNORM, R8G8_SNORM,
R16G16_SNORM, R8G8B8A8_SNORM, B5G6R5 and BC1_UNORM subsets; it is not a complete
texture validation rule.
A failed query is recorded with its HRESULT and never treated as support.
Adapter metadata includes the DXGI driver-package version when
`IDXGIAdapter::CheckInterfaceSupport(IDXGIDevice)` succeeds; older or
unsupported runtimes leave it unavailable. See Microsoft's
[`CheckInterfaceSupport` contract](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiadapter-checkinterfacesupport).

## Feature-level and driver facts

The selected device level, not the adapter's maximum, determines the guaranteed
minimum. For FL9_1, the baseline includes 2048 2D dimensions, one render target,
65,535 primitives per draw, index values through 65,534, vertex and pixel
shaders, and conditional NPOT textures. These are minimum guarantees, not
assertions that larger sizes work on every device. Level 9_1 has no guaranteed
separate alpha blending, hardware instancing, geometry or compute shader stage,
or native UAV storage. The model also records constant and slope-scaled raster
depth bias as hardware guarantees, and `DepthBiasClamp` as unavailable at 9.x
but guaranteed from 10.0. The implementation facts remain separate: the
current private D24S8 path supports constant and slope-scaled bias on triangles,
but not clamp or bias with selected `frag_depth` output. Modeling higher-level
hardware guarantees does not enable higher-level execution paths. Other feature
levels have different documented minima; the hardware model records
`SrcAlphaSaturated` blending as absent at 9.x and present from 10.0, while the
current private backend implementation remains false at every level. Core's
`DownlevelLimits::src_alpha_saturated_blend` gate rejects the factor on the
current FL9_1 path. See the [10Level9 device constraints](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).
The 10Level9 [`Draw`/`DrawIndexed` contract](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context)
also caps texture repeats per primitive at 128 for FL9_1, 2048 for FL9_2,
8192 for FL9_3/10_x, and 16384 for FL11_x, matching the [resource limit
table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-resources-limits).
This is a static feature-level limit, not an optional driver query.
Microsoft's required-format table maps legacy L8 to R8_UNORM, V8U8 to
R8G8_SNORM, and Q8W8V8U8 to R8G8B8A8_SNORM. Each requires 2D texture and
shader-sample support beginning at FL9_1. It also requires DXT1 / BC1_UNORM
for 2D texture sampling at FL9_1. The backend records these minima separately
from the selected device's queried format bits.

The same table requires `R16G16_SNORM` 2D textures at FL9_1, but its
`SHADER_SAMPLE` requirement begins at FL9_2. FL9_1 therefore has no static
sampled-format guarantee for this format. D3D11's format-support contract
allows point-filtered `Sample` when `TEXTURE2D` is present but
`SHADER_SAMPLE` is absent. The HAL therefore permits the single-mip
sampled/upload path after a positive `TEXTURE2D` query and reports
`FILTERABLE` only when `SHADER_SAMPLE` is also present. Query failure or a
missing `TEXTURE2D` bit is rejected. The optional path does not add
copy-source, render-target or readback support.

`R8G8_UNORM` has no required FL9_1 sampled-texture guarantee. The backend
therefore queries the selected device and enables only its single-mip,
single-layer sampled/upload path when `CheckFormatSupport` reports
`TEXTURE2D`. `SHADER_SAMPLE` separately controls the `FILTERABLE` projection.
When the same query also reports `RENDER_TARGET`, the backend permits the
single-mip sampled color-attachment path. It remains non-blendable and has no
copy-source, direct-readback or mip-chain support.
The strict public test on the RTX 3090 Ti at forced FL9_1 verified the
optional path, RG channel stride and linear filtering.

The required-format table also specifies `D16_UNORM` with `TEXTURE2D` and
`DEPTH_STENCIL` support beginning at FL9_1. The backend represents that
feature-level guarantee separately from the selected device's
`CheckFormatSupport` result. Its implementation exposes a single-mip,
single-layer depth-only attachment; it does not add stencil aspects or depth
sampling. The public strict test verifies depth ordering in both draw orders.

The required-format table also includes `R8G8B8A8_UNORM_SRGB` and
`B8G8R8A8_UNORM_SRGB` from FL9_1. Both are modeled as feature-level
guarantees, then independently gated by their own device `CheckFormatSupport`
results and the implemented typed-texture path. The backend supports their
single-mip upload, sampled-view, render-target and readback uses. Strict public
tests verify sRGB decode while sampling and encode on render-target writes for
both channel orders, plus implicit-LOD selection from four-level POT chains.
The same positive per-format query and mip/backend checks used for linear
RGBA8/BGRA8 govern reported mip-chain support.

Conditional NPOT textures at FL9 require one mip and restricted addressing. Full NPOT and
comparison-shadow support are separately queried using `CheckFeatureSupport`;
a newer runtime or driver may return E_INVALIDARG for an unknown query, which
is retained as unavailable. `CheckFormatSupport` is queried only for RGBA8,
BGRA8, RGBA8_SRGB, BGRA8_SRGB, R8_UNORM, R8G8_UNORM, R8G8_SNORM, R16G16_SNORM,
R8G8B8A8_SNORM, BC1_UNORM, B5G6R5, B5G5R5A1, B4G4R4A4, D16_UNORM and D24S8, and
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

## Viewport clipping

The private HAL clamps a viewport rectangle to the attachment and, when
clipping changes its transform, selects a separately compiled FL9_1 vertex
shader variant. That variant can fail FXC's level-9 instruction limits even
when the original shader compiles. Core carries this implementation
restriction as `DownlevelLimits::viewport_rectangles_outside_target`; when it
is false, rectangles extending past any attachment edge are rejected during
render-command validation before the HAL is called. Rectangles contained by
the target continue through the normal shader path. The HAL's private clipped
viewport path remains available when its adjusted shader exists, but the
controlled FL9_1 core fixture reports the conservative false capability and
tests the rejection. The strict public wgpu test also verifies a 4×6 scissor
inside an 8×8 target (24 green and 40 unchanged blue pixels) and confirms an
out-of-target viewport is rejected before HAL. Other backends retain the
default true capability.

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
RGBA8 and matched all 80 output pixels. A later strict test also verified its
query-gated render-target, partial-copy and padded CPU-readback path. R8 linear
filtering is enabled only when the query indicates filtered sampling; public
GPU coverage verifies linear filtering for RG8_UNORM, while R8 linear filtering
has not been visually tested.

The same adapter returned R8G8_SNORM support mask `0x00021320` at requested
and selected FL9_1. A strict test uploaded padded signed RG pairs, sampled and
remapped both channels to RGBA8, and checked all 80 output pixels within one
UNORM code value for conversion rounding. Render-target, copy-source, readback
and mip-chain uses remain unsupported.

On 2026-11-05, the adapter returned `0x00021122` for `R16G16_SNORM` at
requested and selected FL9_1. The mask includes `TEXTURE2D` but not
`SHADER_SAMPLE`, matching Microsoft's FL9_1/FL9_2 minimum split. A strict
private HAL test uploaded a padded image, point-sampled it and matched 80/80
pixels. The public adapter now exposes the required
`TEXTURE_FORMAT_16BIT_NORM` feature and a non-filterable sampled/upload
capability; core rejects a filtering layout using the queried format flags.
The strict public point-sampling test also passed. This modern-driver result
does not establish historical-driver behavior.

The remaining queried 16-bit normalized formats did not provide another
sampled implementation candidate on this device: R16_UNORM, R16_SNORM,
R16G16_UNORM and R16G16B16A16_UNORM returned `E_FAIL`, while
R16G16B16A16_SNORM returned only `IA_VERTEX_BUFFER` (`0x00000002`). These are
unavailable/partial query observations, not claims that every FL9_1 device
lacks them. Microsoft's required-format table places the normalized texture
support for R16_UNORM and R16G16_UNORM at FL9_2 and lists R16G16B16A16_SNORM
as an FL9_1 vertex-input format only.

The selected-device diagnostic also checked R32_FLOAT and
R32G32B32A32_FLOAT. Both returned only `IA_VERTEX_BUFFER` (`0x00000002`) at
forced FL9_1, without `TEXTURE2D`; D32_FLOAT and D32_FLOAT_S8X24_UINT queries
returned `E_FAIL`. These results provide no additional sampled or depth
attachment path on this RTX. Microsoft's required-format table places R32_FLOAT
texture support at FL9_2 and does not list D32 as an FL9_1 depth attachment.
This is a bounded observation for the selected driver, not a claim about every
FL9_1 implementation. See [Microsoft's required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats).

Microsoft requires B5G6R5, B5G5R5A1 and B4G4R4A4 2D textures, filtered shader
sampling, mip support and CPU lockability for Windows 8 FL9_x drivers. The
backend implements the narrower single-mip sampled-upload subset as
`Rgb5g6b5Unorm`, `Rgb5a1Unorm` and `Rgba4Unorm`, gated by
`TEXTURE_FORMAT_16BIT_PACKED` and separate selected-device `CheckFormatSupport`
queries. At forced FL9_1 the RTX returned `0x0026F3A0` for B5G6R5_UNORM and
`0x000213A0` for each of B5G5R5A1_UNORM and B4G4R4A4_UNORM. Those masks include
2D texture and shader-sample support. The strict public test uploaded padded
two-byte texels, sampled all three native SRVs and matched every RGBA8 output
pixel, including the 1-bit and 4-bit transparent alpha cases.

RGB5A1 and RGBA4 are exposed only for `TEXTURE_BINDING | COPY_DST`, one 2D
layer, one sample and one mip. B5G6R5 also permits render-target, copy-source
and CPU-readback use when its selected-device query reports `RENDER_TARGET`;
its draw/sample and partial-copy/readback paths have strict GPU coverage.
`SHADER_SAMPLE` controls filterability. Mip chains remain unsupported. The
modern RTX result verifies the query-selected path, not behavior of historical
FL9_1 drivers.

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

On 2026-11-05, the same adapter returned `CheckFormatSupport` mask
`0x112ef3e0` for both `R8G8B8A8_UNORM_SRGB` and
`B8G8R8A8_UNORM_SRGB` at requested and selected FL9_1. The mask includes the
required 2D texture, shader sampling, mip, render-target and blendable bits.
The public strict test uploaded one texel in each format and verified that
sampling decoded all 128 RGBA8 output pixels to `[55, 13, 134, 255]`. It then
rendered linear `[0.5, 0.25, 1.0, 1.0]` into both sRGB targets and verified 128
encoded pixels, including RGBA/BGRA byte order. A separate strict test created
four-level POT chains of both formats and verified that implicit LOD selected
mip 1, decoding 128/128 pixels to `[55, 13, 134, 255]`. These observations
verify the tested paths on this driver; the modern adapter does not establish
historical-driver behavior or every possible sRGB mip use.

## FL9_1 inventory and gaps

The public adapter exposes only the capability projection and operations
covered by core validation; remaining row-specific gaps block expanding that
contract. Hardware availability below means the FL9_1 contract, not a claim
about a particular driver's optional capabilities. Test names refer to private
HAL tests unless stated otherwise; success on one modern adapter is not proof
for all FL9 GPUs.

| Category | Hardware and private implementation | Coverage; wgpu mapping and remaining work |
| --- | --- | --- |
| Device / adapter | Native DXGI discovery excludes software adapters and creates only an explicitly requested FL9_1 device. The public opt-in adapter exposes the selected-device projection and a constrained HWND surface: BGRA8 sRGB FIFO, opaque composition, render-attachment usage, one frame of latency and owner-thread lifecycle. | Strict public wgpu tests select the RTX 3090 Ti, render/read back through group 7 and dynamic uniforms, verify overlapping near/far draws in both orders, concurrently submit through two public devices sharing the selected native device/context, and verify core rejection of unsupported resource/layout requests. A hidden-window lifecycle test configures, presents, resizes and presents again; a visible-window example presents 60 animated textured, indexed, depth-tested frames. The adapter exposes `TEXTURE_FORMAT_16BIT_NORM` only when a queried R16G16_SNORM format is a 2D texture; other public feature support remains empty. It is not WebGPU-compliant. The exhaustive contract audit remains outstanding; visible client-area output has been qualitatively captured and inspected. |
| Buffers, maps, copies | Native buffers and copies exist. Private HAL implements vertex, index, uniform, copy and explicit CPU-shadow mappings; vertex+index is rejected at 9_1/9_2. The FL9_1 logical size limit leaves one 4-byte alignment unit below `u32::MAX`, so core's vertex sentinel byte and 4-byte allocation rounding still fit D3D11's `ByteWidth`. Core may carry storage usage bits on an otherwise supported buffer allocation, but FL9_1 native buffers retain a single vertex/index bind and no shader storage binding is implemented. Indirect usage is unsupported. | Copy, map, initialization and ownership tests. Core now requires `VERTEX_STORAGE`, `FRAGMENT_STORAGE` or `COMPUTE_SHADERS` for storage-buffer layouts visible to those stages; writable fragment buffers also require `FRAGMENT_WRITABLE_STORAGE`. The controlled fixture verifies unsupported stage layouts fail before HAL and a storage/copy buffer still uploads, copies and reads back. This does not expose storage access. `UNRESTRICTED_INDEX_BUFFER` and limits describe only part of the buffer contract. |
| 2D textures / formats / views | FL9_1 supports multiple native formats subject to individual format flags. Private RGBA8 and BGRA8 2D textures have one layer and sample; restricted copy-only, sampled and color-attachment POT textures can have multiple mips. R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM are limited to one-mip, one-layer sampled/upload resources and use positive per-device `TEXTURE2D` queries. R8G8_UNORM and R16G16_SNORM have no FL9_1 sampled guarantee; their one-mip sampled/upload paths are conditionally available after a positive `TEXTURE2D` query. R8_UNORM, R8G8_UNORM and packed B5G6R5 additionally expose single-mip color-attachment use only after the selected device reports `RENDER_TARGET`; that same query gates their implemented GPU copy and CPU-readback path. They are not exposed as blendable. B5G6R5 also requires `TEXTURE_FORMAT_16BIT_PACKED`. `SHADER_SAMPLE` independently controls filterability. BC1_UNORM additionally requires dimensions aligned to 4×4 blocks and supports block-aligned uploads only. D16_UNORM is a queried single-mip depth-only attachment; D24S8 supports depth/stencil attachments. Microsoft documents that FL9_x D3D11 depth/stencil-bound resources require one mip. Core now represents multi-mip depth/stencil attachment support with `DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP`; the FL9_1 D3D11 capability projection must leave it absent so invalid descriptors fail before HAL. | Transfer, view, clear and draw readback tests. Padded-offset uploads and sampling for R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM passed on the RTX at FL9_1; the optional R8G8_UNORM path also passed a sparse-bound upload/sample test with distinct channel data. A strict public test draws into R8_UNORM, R8G8_UNORM and B5G6R5 attachments, samples the resulting contents and validates all 192 output pixels within one UNORM code value. The follow-up copies each rendered attachment directly into a padded staging readback buffer and checks all 64 native texels per format. Another strict public test copies a partial rectangle for each of those formats between differently sized textures and reads back all 24 texels with a padded row layout. A padded BC1 upload plus partial block replacement matched all 256 output pixels. The RTX's R16G16_SNORM query lacks `SHADER_SAMPLE`; strict private/public tests prove point sampling succeeds and filtering is rejected through core validation. The controlled core fixture verifies multi-mip D24S8 attachment creation is rejected by the missing downlevel flag before HAL. Other sampled formats remain non-renderable and cannot be copied from or read back in this private subset. Per-format capabilities and numeric limits must match creation, copies, initialization and view behavior. Other formats and dimensions need implementation or validation. |
| Texture view dimensions | The D3D11 resource path is limited to 2D, one-layer resources and the private HAL creates only `D2` views. | Core rejects D1/D3 texture creation and binding layouts through explicit downlevel limits, and gates `D2Array`, `Cube` and `CubeArray` layouts/views with explicit downlevel flags before HAL. The array flag also gates storage-texture array layouts. A one-layer texture view can still have array semantics even when `max_texture_array_layers` is one; that limit alone cannot represent the missing dimension. The projection retains inherited nonzero native 1D/3D numeric limits (2048/256) while setting the dimension booleans false; the controlled fixture verifies texture-creation and view rejection before HAL. The public strict test verifies D3 creation is a public validation error. |
| Mips, arrays, NPOT / sampling | POT RGBA8 and BGRA8 textures can have mip chains when format support is reported. Sampled suffix views are native; other supported mip ranges use a GPU clone. Color-attachment views select one mip. Sampled and color-attachment usages can coexist, with optional copy usages. Arrays and NPOT mip chains remain unavailable. | Strict FL9_1 tests verify per-mip transfers, attachment rendering, suffix and clone-backed non-suffix sampling on the RTX. Core now requires `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` before forwarding any NPOT mip chain; D3D11 does not set this implementation flag, even when the runtime reports full NPOT support. The public projection rejects unsupported shapes and mip chains. |
| Samplers | Independent point/linear min/mag, clamp/wrap/mirror and anisotropy up to 2 are implemented in the private HAL. Border addressing and comparison samplers are rejected; core now requires `COMPARISON_SAMPLERS` before forwarding a comparison descriptor. Translated LOD ranges are checked against each used sampled view. | Patterned FL9_1 GPU readback covers filter and address modes, sampled mip levels and sampler switching. The controlled core fixture verifies comparison samplers fail the missing-downlevel check before HAL. Anisotropy 2 creates and samples but visual anisotropic quality is unverified. Public feature/limit representation remains incomplete. |
| Bind groups / registers | Up to eight groups may contain uniforms, sampled textures and samplers, with aggregate implementation limits of thirteen uniforms, eight textures and sixteen samplers. The internal projection exposes at most six uniform buffers per shader stage, keeping the union of disjoint vertex and fragment bindings within the thirteen-slot allocator. | Sparse multi-group, dynamic-offset, pipeline-transition and cross-group core GPU tests cover selected configurations. The strict public wgpu test now binds group-0 dynamic uniforms and group-7 texture/sampler resources while reusing logical binding numbers 3 and 17 across groups; indexed rendering and readback pass. This verifies the maximum group index and logical identity translation on the tested RTX, not every resource combination or historical driver. FXC limits distinct sampled relationships and constants. Counts in `Limits` cannot express all shader, visibility, compatibility or cross-group restrictions. The public adapter remains experimental and its full capability audit remains incomplete; its narrow HWND contract is described in Device/adapter. |
| Vertex / index formats | The FL9_1 required IA subset is `Uint8x4`, `Unorm8x4`, `Sint16x2`, `Snorm16x2`, `Sint16x4`, `Snorm16x4` and `Float32` through `Float32x4`. These are guaranteed minima, intersected with `CheckFormatSupport(IA_VERTEX_BUFFER)` on the selected device and the HAL mapping table. The HAL maps every wgpu vertex format with a direct DXGI input equivalent except `Float64` variants. Formats outside the required FL9_1 subset are optional and enter the selected-device mask only when `CheckFormatSupport(IA_VERTEX_BUFFER)` succeeds; a missing or failed query omits them. `Float16x2` and `Float16x4` become hardware guarantees only from FL9_3. The RTX forced to FL9_1 reports no optional input format. Private pipelines accept up to sixteen sparse vertex- or instance-rate streams, including zero-stride constant streams. Integer IA inputs are limited to direct `i32`/`u32` vector-to-float conversion in the FL9_1 vertex profile; integer arithmetic and other runtime integer semantics remain rejected. Attributes retain four-byte alignment. Multi-slot configurations support triangle and point lists, nonindexed line and triangle strips, and Uint16/Uint32 indexed triangle, point and line lists with translation when needed. Indexed LineList and TriangleList with only instance-rate or no vertex-rate inputs validate the index range and replay equivalent nonindexed draws because index values and base vertex cannot affect vertex inputs when `vertex_index` is unavailable. Indexed LineStrip and TriangleStrip with no vertex-rate inputs read index contents at replay, split restart runs after signed base-vertex resolution, and replay one nonindexed strip per run; ordinary indexed strip draws retain the existing CPU restart-splitting path. These cases are verified on the RTX at forced FL9_1 for Uint16/Uint32, instance-only and empty layouts. On FL9_1, overlapping elements in a logical stream are assigned to distinct native slots bound to the same buffer. | `DownlevelLimits::vertex_format_mask` carries the format-specific support subset into core; unsupported attributes are rejected before HAL. Strict forced-FL9_1 readback exercises the required formats and SNORM conversion. The selected RTX exposes no optional formats, so its Float16x2 public test logs a capability skip; no optional-format GPU execution is claimed. Existing indexed and non-indexed readback, sparse-slot, overlap, zero-stride, SNORM vertex, strip and rejected-index tests remain applicable. Synthetic checks distinguish guaranteed formats, optional query results and backend mappings. Overlapping physical reads, stream offsets, maximum strides, optional-format execution and older-driver behavior remain unverified. SNORM vertex support does not imply SNORM texture support. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures, selected vertex entries declaring `vertex_index` or `instance_index`, and fragment entries declaring `primitive_index` are rejected by the private shader path. Core checks the selected vertex entry point against `DownlevelLimits::max_sampled_textures_per_vertex_stage`, which the D3D11 fixture sets to zero, and `DownlevelLimits::fragment_front_facing` / `fragment_position`, which are false for the tested FL9_1 profile. Texture loads reachable from the selected entry point are unconditionally rejected by the private HAL. Fragment derivatives and explicit texture gradients are unavailable: selected-entry `fragment_derivatives` is false and core rejects reachable Naga `Derivative` and `ImageSample::Gradient` expressions, including helper calls. Explicit `textureSampleLevel` is separately unavailable below FL10_0: core's `texture_sample_level` downlevel limit rejects reachable `SampleLevel::Exact` operations, while implicit sampling remains supported. FXC reports X4532 (`texlod not supported on this target`) for literal and dynamic explicit LOD at both `ps_4_0_level_9_1` and `ps_4_0_level_9_3`. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests, including unused helper operations. The controlled core fixture verifies selected-entry validation before HAL; the strict public test rejects direct and helper-reached explicit LOD while compiling an implicit-sampling sibling. Microsoft [documents the 9.1/9.2 and 9.3 shader-profile limits](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets). |
| Pipeline / raster / blending | Private graphics pipeline supports line lists, line strips, triangle lists, nonindexed triangle strips and point lists with configurable front-face winding and front/back culling for triangles, one color target and one sample. Blend translation supports the ordinary wgpu factors, including constant and inverse-constant factors, and all five blend operations; `SrcAlphaSaturated` and dual-source factors are rejected at FL9_1. Core reports the FL9_1 restriction through `DownlevelLimits::src_alpha_saturated_blend` and rejects use before HAL. Blend constants are recorded and rebound with pipeline state. A single-sample mask is passed to the output merger; a zero mask maps to `0xffff_fffe`, an equivalent no-coverage mask for the only active sample and the nonzero native value required at FL9_1. Multisampling and alpha-to-coverage remain unsupported. All RGBA channel write masks are supported for the single target. A descriptor whose color and alpha equations differ is rejected when both RGB and alpha are written unless separate-alpha support is explicitly established; the production FL9_1 model does not establish it. The core `SEPARATE_ALPHA_BLEND` flag rejects these descriptors before HAL. Enabled blending requires a positive format query. Out-of-target viewports use a separately compiled vertex position transform when available. A D24S8 attachment supports native Less, Equal, LessEqual, Greater, NotEqual, GreaterEqual and Always depth comparisons; Never retains its emulation; optional depth writes and one-sided stencil comparison/operations are implemented with identical front/back state. FL9_1 rejects differing front/back stencil face states; core now represents this restriction through `DownlevelFlags::TWO_SIDED_STENCIL` and rejects a differing descriptor before HAL. Never with active stencil maps the stencil pass operation to the requested depth-fail operation, forces Always depth comparison, and suppresses color/depth writes. Signed constant and finite slope-scaled depth bias are supported for triangles with selected-entry restrictions. Nonzero clamp is rejected because FL9_1 requires zero; nonzero bias on points and solid lines is rejected because native raster bias does not affect those primitives. | CPU tests cover factor translation and rejection of `SrcAlphaSaturated`/dual-source factors. A controlled FL9_1 core fixture rejects all four source-alpha-saturated source/destination factor positions before HAL. A strict forced-FL9_1 HAL render checked all eight depth comparisons over 128 pixels (64 green, 64 blue), and a blend render checked constant-factor updates and all five operations over 64 pixels; the controlled core fixture checked a custom constant-factor render over 64 pixels. The private strict depth-bias render checks five constant-bias images and two sloped-triangle images (64 pixels each), including positive/negative slope signs, point/line rejection, nonzero clamp rejection and selected-entry `frag_depth` rejection. Separate-alpha rendering is exercised only through an opt-in RTX diagnostic and is not a portability guarantee. The existing strict suite checks premultiplied blending and opaque transitions on RGBA8/BGRA8. Remapped shaders may exceed FL9 compiler limits even when the original compiles; such draws remain rejected. |
| Render passes / targets | One color target and an optional D16 depth-only or D24S8 depth/stencil attachment support load, clear or don't-care and store or discard. Discard retains native contents, which remain undefined to HAL callers. Restricted transient color and depth attachments use ordinary textures without memoryless savings; transient combinations with sampled or copy usage are rejected. Private read-only passes require LOAD and STORE on each present aspect and pipelines without depth or stencil writes; depth sampling aliases remain unsupported. Depth-only clears and draws without a fragment stage are supported. No resolve, timestamps, occlusion, multiview or multiple color attachments. | Public forced-FL9_1 tests cover D16 depth ordering and a `DepthOnly` view; `StencilOnly` and active stencil use are rejected, a second pass with LOAD preserves depth, and CLEAR resets it. A discarded D16 depth value followed by LOAD is initialized before use and does not influence the next draw. A public `LoadOp::DontCare` test overwrites all color and depth pixels before reading back all 64 red pixels. Public color `StoreOp::Discard` followed by copy and by SRV sampling reinitializes the texture before the later read; both paths produce transparent black at all 64 pixels rather than expose the discarded red clear. D24S8 tests cover a depth-only prepass, read-only depth preservation, and separate depth/stencil LOAD, CLEAR, and DISCARD-then-LOAD transitions. A public transient test executes color clear/draw/discard and uses transient D24S8 clear/draw/discard to control a stored color result; core rejects transient sampling, invalid LOAD/STORE, read-only transient depth and passes with no attachments. Sampled D24S8 usage and public depth-only/stencil-only D24S8 views are rejected by core; the combined attachment view remains supported. The controlled core fixture verifies the aspect-view errors before HAL because the projection does not expose the aspect formats. Other pass combinations still need validation before public exposure. |

Strict public wgpu tests verify symmetric D24S8 stencil replace/equality with
all 64 output pixels green, plus premultiplied blending with all pixels
`[128, 0, 128, 255]`. Asymmetric stencil faces, separate-alpha equations and
source-alpha-saturated factors are rejected before HAL. This confirms the
implemented symmetric stencil and same-equation blend subsets, not two-sided
stencil or separate-alpha support. A strict public shader also samples two
independent textures through sparse logical bindings 7 and 42 and one shared
sampler at 100; all 64 readback pixels include both sampled contributions.
Public core rejects the incomplete bind group before command submission.
Another strict public shader uses the full projected eight sampled textures
per stage through sparse logical bindings and verifies each contribution in
the output. Core rejects a ninth texture layout before HAL, matching the
projected limit rather than advertising a broader resource count.
The public transfer test also verifies a mapped-buffer upload with offset 16
and 256-byte row pitch, a 2×2 copy between 8×8 and 6×6 textures at nonzero
origins, and a 6×6 readback with padded rows. All 36 output texels matched,
including the preserved destination background.
The public sampled-mip test uploaded and rendered an RGBA8 four-level POT chain;
implicit derivatives selected mip 1 across all 64 pixels. The same test
confirmed core rejects multi-mip R8 because that queried format is projected
with `SINGLE_MIP_ONLY`. This does not imply multi-mip support for other formats
or NPOT chains.
Public `Rg8Snorm` and `Rgba8Snorm` tests also verify the selected-device
`TEXTURE_BINDING | COPY_DST`, filterable, single-mip projection. Their strict
render test uses nearest sampling and verifies signed-normalized conversion in
all 64 RGBA8 output pixels; the selected-device `SHADER_SAMPLE` query also
permits any filter for these formats. A separate RG8_UNORM test verifies linear
filtering with an eight-value readback gradient. Other formats and usages are
not implied.
Public rendering also verifies the implemented per-instance vertex-stream
emulation with a nonzero first-instance range. The skipped first record does
not render, the next two records produce distinct colors, and all other pixels
retain the clear color. Core rejects `instance_index` entry points because the
selected FL9_1 contract does not expose that system value.

POT RGBA8/BGRA8 mip chains accept every nonempty combination of `RESOURCE`,
`COLOR_TARGET`, `COPY_SRC` and `COPY_DST`. The selected format's MIP, shader-sample
and (when the usage includes `COLOR_TARGET` or `COPY_SRC`) render-target query
bits must all be present. Each implementation family remains separately gated:
copy-only, sampled, attachment-only, and sampled attachment chains. Core format
capabilities cannot express usage-dependent mip ceilings, so each reported
usage combination must pass the same private HAL path.

For FL9_1, the private shader compiler rejects `textureLoad` / Naga
`ImageLoad` reachable from the selected entry point unconditionally, including
when shader runtime checks are disabled. `DownlevelLimits::texture_load` now
projects this restriction into core, which follows validated entry-point call
graphs and rejects only selected vertex or fragment entries that use the
operation. Other entry points in the same module are analyzed independently.
The native limits proof remains an opt-in experiment and is not used to expose
texel loads through the production HAL.

Per-format `SINGLE_MIP_ONLY` metadata reports the current one-mip ceiling for
R8_UNORM, R8G8_SNORM, R16G16_SNORM, R8G8B8A8_SNORM, BC1_UNORM, D16_UNORM and
D24S8. RGBA8 and BGRA8
also receive this flag when the selected device's successful format query
lacks `D3D11_FORMAT_SUPPORT_MIP`, or when any currently required backend mip
path is disabled. Core checks this metadata before HAL creation and rejects
larger mip counts. This is separate from device-wide NPOT-mipmap and
depth-attachment flags: the format metadata expresses a per-format
implementation ceiling, while those flags express broader downlevel behavior.
The controlled fixture compares the RGBA8/BGRA8 projection against the real
device query; the RTX 3090 Ti reports MIP support for both at forced FL9_1.
Synthetic capability tests cover missing MIP support and disabled backend
paths, since this modern driver cannot reproduce those negative query results.

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
| Queries / synchronization / readback | Native event queries back the private fence; this is not a public query-set implementation. Queue submission serializes the immediate context, and staging readback waits for completion. If replay fails after issuing earlier commands, HAL ends and flushes an event query and waits for prior GPU work before returning the error, keeping borrowed command buffers and resources alive until that partial submission drains. Shared HRESULT classification recognizes device hung, removed, reset and driver-internal-error results as device loss. | Fence, sequencing, copy, partial-replay-failure and GPU readback tests; CPU tests cover device-loss HRESULT classification. Core rejects occlusion query sets when `OCCLUSION_QUERY` is absent; timestamp queries require a feature. The private HAL rejects both, and some destroy paths for resources that cannot be created still panic. Actual device removal/recovery, synchronization after a failed event query, all lifetime/error paths and debug-layer diagnostics remain unverified. |
| Compute / storage | No native compute shader or UAV support at FL9_1; private HAL accepts paired empty compute passes but rejects compute pipeline creation, dispatch and shader storage bindings. Core may retain storage bits for transfer-capable buffers but does not bind them as storage. Other APIs may require CPU or graphics emulation. | Strict hardware test verifies ordered copies around an empty pass and rejects dispatch and invalid pass sequences. The controlled core fixture checks stage-specific storage-buffer layout rejection before HAL. No compute/storage feature is exposed; feature/limit gating alone remains insufficient for public device exposure. |
| Presentation | FL9 hardware can support display through platform/format-specific paths. The private backend retains the WinRT CoreWindow path and exposes a constrained Win32 HWND surface through opt-in public wgpu: BGRA8 sRGB FIFO, opaque composition, render-attachment usage and one frame of latency. Configure, acquire and present are owner-thread-only. | Queue and selected context-backed operations serialize against Present; same-thread reentrant mutations are rejected. Strict RTX tests cover hidden-window configure/present/resize, wrong-thread acquisition rejection and a visible 60-frame textured indexed scene. A client-area capture was visually inspected and showed distinct red and green faces over the clear color. Deterministic presentation pixels remain unavailable, while equivalent color/depth behavior is covered by offscreen readback. Further lifecycle/error edges, device loss and historical driver behavior remain unverified. This is not general surface or WebGPU support. |

For FL9_1/9_2, default buffers have exactly one native vertex or index bind:
copy-only, mapped and uniform-source buffers use an internal vertex bind, while
index buffers use an index bind. This does not add a logical wgpu usage. Staging
buffers retain zero bind flags because D3D11 staging and CPU-read resources
cannot be pipeline-bound; Microsoft's 10Level9 table does not explicitly state
that exception. A modern forced-FL9_1 device passed copy/readback tests; legacy
runtime behavior remains unverified.

Core can add logical storage-use bits to a buffer's internal usage when the
descriptor also requests storage. The D3D11 allocator accepts those bits only
alongside another supported logical role and still chooses the single native
vertex or index bind required by FL9_1. This lets copy and initialization paths
operate on the resource; it does not provide shader storage access. Core rejects
storage-buffer layouts unless the visible shader stage's downlevel flag is
present, before the unsupported layout reaches this HAL.

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
behavior of this fallback is unverified. The private pipeline translates Less,
Equal, LessEqual, Greater, NotEqual, GreaterEqual and Always to native D3D11
depth comparisons, with optional writes. Strict readback covers each native
function. Never is emulated by suppressing validated draws when stencil is inert; when
active stencil must observe the depth-fail branch, a separate Always-based
state remapping is used below. Draw suppression remains restricted to the
current side-effect-free, read-only-resource and no-query subset. Future shader
side effects or query support require revisiting it.
Stencil state maps all eight WGSL operations and comparison functions to the
native D3D11 depth-stencil descriptor, with the low eight bits of the masks.
The strict forced-FL9_1 hardware test executes every operation in each native
pass, stencil-fail and depth-fail slot with boundary values for saturating and
wrapping arithmetic. Subsequent Equal tests verify all 24 resulting states via
full 8x8 color readback. The DSV itself cannot be read back through this HAL,
and this RTX result does not establish behavior on historical FL9_1 drivers.
Only identical active front/back states are supported; the general public
`depth_stencil` capability remains false. The reference is retained in command
replay, rebound when either the reference or pipeline changes, and reset at each
render pass. Strict readback verifies Replace writes indirectly by using a later
Equal comparison, including a wrong-reference rejection and state transitions.
These tests do not claim direct inspection of stencil storage. Always-passing,
non-writing stencil states continue to canonicalize to disabled stencil.

Depth comparison Never remains draw suppression only when stencil is inert. If
active stencil must observe the depth-fail branch, the backend uses Always,
remaps StencilPassOp to the requested StencilDepthFailOp, and disables color
and depth writes. The 10Level9 [`CreateSamplerState` restriction](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
and [comparison-sampler example](https://learn.microsoft.com/en-us/windows/uwp/gaming/create-depth-buffer-resource--view--and-sampler-state)
refer to comparison filtering;
`D3D11_DEPTH_STENCIL_DESC::DepthFunc` uses the full
[`D3D11_COMPARISON_FUNC`](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_comparison_func)
enum ([depth-stencil descriptor](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_depth_stencil_desc)).
The strict forced-FL9_1 test verifies all seven native depth functions and the
Never emulation. Historical 9_1 drivers remain unverified. The stencil test
confirms that a following Equal comparison observes the remapped write.
Private read-only passes require LOAD|STORE for both aspects and a pipeline
without depth or stencil writes. A strict public test now verifies D24S8
passes with both aspects read-only, read-only depth plus writable stencil, and
writable depth plus read-only stencil. Core rejects pipelines that write a
read-only aspect; all three valid combinations pass on the forced-FL9_1 RTX.
The public projection still omits `READ_ONLY_DEPTH_STENCIL`: without that flag,
core uses the ordinary writable DSV usage and does not permit sampling the
attachment during the pass. This test proves aspect rules for non-aliasing
passes, not a read-only DSV/SRV alias. Private read-only passes use an ordinary
DSV as well. The public D24S8 path also supports a colorless depth prepass:
the later color pass fails its depth comparison only after the prepass writes
the near depth value. This verifies the no-color-target pass across core, HAL
and GPU execution.
The broad `depth_stencil` capability remains false. Texture and view descriptors
may declare READ, WRITE or READ|WRITE, but an actual pass uses one usage.
Strict GPU tests verify textured depth ordering, initialization, independent
clear/load and discard behavior, no-write behavior and view lifetime. The no-op
stencil tests verify rendered color but cannot read back stencil bits. They do not
prove general stencil operations or historical-driver compatibility. The
10Level9 device reference lists a LessEqual restriction for comparison
samplers, not raster depth-stencil state. The D3D11 depth-state descriptor uses
the complete comparison-function enum. All seven non-Never raster comparisons
now map natively and passed forced-FL9_1 RTX readback; historical device
behavior remains unverified. Never uses the restricted draw-suppression and
stencil-remapping emulation.

Triangle pipelines support signed constant and finite slope-scaled D24S8 depth
bias with zero clamp. Nonzero bias is rejected for point lists and lines, which
are unaffected in the solid-fill state, and for a selected fragment entry
writing `frag_depth`. Core represents this implementation boundary with
`DownlevelLimits::depth_bias_with_fragment_depth` and checks only the selected
fragment entry point; an entry point with no available core interface is
rejected conservatively when the adapter lacks the capability. Unselected
depth-writing entries do not cause rejection.
Nonzero clamp is rejected because FL9_1 requires `DepthBiasClamp = 0`; core's
`DEPTH_BIAS_CLAMP` downlevel gate now has a controlled-fixture regression proving
this descriptor is rejected before HAL. A strict
FL9_1 RTX test checked five constant-bias and two sloped-triangle complete
images; the two slope cases verified that positive bias preserved the nearer
red depth and negative bias let the later green depth pass across all 64
pixels. The controlled core fixture still checks positive/negative constant
bias only. FXC rejected the zero-bias direct-only `frag_depth` entry with
X4530, while a structured color-and-depth entry compiled. Microsoft specifies
32-bit bias arithmetic explicitly for FL10 and higher; precision on historical
FL9 hardware remains unverified. See
[D3D11 depth bias](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-output-merger-stage-depth-bias)
and [10Level9 rasterizer restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).

The FL9_1 projection also disables
`DownlevelLimits::fragment_depth_without_color_target_0`. FXC rejects a
fragment shader that writes `SV_Depth` without `SV_Target0` with X4530, even at
zero depth bias, while a shader returning both `@location(0)` color and
`frag_depth` compiles. Core follows the selected entry point and rejects only
the missing-target-0 shape before HAL. A color output at another location does
not satisfy this profile requirement. This restriction reflects the current
10Level9 compiler path; it is not a general FL9_1 hardware claim.

The FL9_1 projection also disables
`DownlevelLimits::fragment_sample_mask_output`. Naga lowers a WGSL fragment
`@builtin(sample_mask)` output to HLSL `SV_Coverage`, whose writable pixel
shader form Microsoft documents as requiring `ps_4_1` or higher. The backend
uses `ps_4_0_level_9_1`; a strict-device FXC test confirms that the mask output
fails while a color-only sibling compiles. Core checks the selected fragment
entry point before HAL. The private renderer is single-sample, so a future
semantics-preserving implementation could investigate lowering the low bit
to discard, but no such rewrite is currently performed.

| Transfer | Implementation and restrictions | Coverage |
| --- | --- | --- |
| Buffer to texture | Retained GPU buffer copied to a CPU-readable staging buffer at replay; mapped bytes feed `UpdateSubresource` with an explicit destination box and checked row pitch. This reads GPU contents at execution time, not a CPU shadow. RGBA8/BGRA8 linear and sRGB formats share this transfer implementation. R8_UNORM, R8G8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM accept one-mip, one-layer sampled/upload use; R8_UNORM and R8G8_UNORM may combine upload and color-target use only when the selected query reports `RENDER_TARGET`. R8G8_UNORM is optional and requires a positive `TEXTURE2D` query. R16G16_SNORM has the same restricted path only if the selected device query reports `TEXTURE2D`; `SHADER_SAMPLE` controls filterability. BC1_UNORM uses 4×4 texel blocks, 8-byte block rows, 4-aligned dimensions and block-aligned update regions. | RGBA8/BGRA8 offset and padded full/partial uploads, GPU-written source and ordered submissions. A strict public test uploaded both sRGB formats and verified their sampled decoding. The guaranteed sampled-upload-only uncompressed formats passed padded-row, nonzero-source-offset upload and full rendered readback tests on forced FL9_1. The optional R8G8_UNORM path passed strict public upload and sampled readback on the RTX; R16G16_SNORM point sampling and filtering rejection both passed at forced FL9_1. BC1 passed an offset padded full upload, a partial block replacement and 256-pixel rendered readback. |
| Texture to texture | Native GPU `CopySubresourceRegion` between RGBA8 or BGRA8 linear/sRGB textures, including compatible linear↔sRGB pairs, different mips of one texture, and checked source boxes with DEFAULT resources. The format pairs must belong to the same DXGI type group; copies preserve stored bits without color conversion. Single-mip R8_UNORM, R8G8_UNORM and B5G6R5 copies are also supported when the selected format query reports `RENDER_TARGET`. Other cross-family formats and copies within the same subresource are rejected. R8G8_SNORM, R16G16_SNORM and R8G8B8A8_SNORM copies remain unsupported. | Full and partial copies, different overall dimensions, cross-resource and same-resource mip copies, zero-preserving destination and render-to-copy-to-sample. Strict public tests copy RGBA8 and BGRA8 linear↔sRGB and verify every intermediate and round-trip byte; a separate query-gated test verifies partial copies for R8, RG8 and RGB565. |
| Texture to buffer | A region is copied to a retained CPU-readable staging texture and mapped; each mapped row is written into the destination GPU buffer at the requested pitch and offset using the format's block width. Single-mip R8_UNORM, R8G8_UNORM and B5G6R5 readback is supported only when the selected format query reports `RENDER_TARGET`, as required by the FL9_x internal render-target binding restriction. R8G8_SNORM, R16G16_SNORM and R8G8B8A8_SNORM readback remain unsupported. | Full and partial RGBA8/BGRA8 linear/sRGB source regions, nonzero buffer offsets, row padding and complete image comparison. Strict sRGB render-target readback matched encoded bytes for both channel orders; query-gated R8/RG8/RGB565 readback checked all texels and padding for partial copies and directly from rendered color attachments. |

The strict FL9_1 RTX test checked partial same-texture copies between distinct
mips in both directions for RGBA8 and BGRA8, including all affected and
untouched texels and padded readback bytes. Two more GPU cases rendered into a
mip, copied to another mip of that texture and sampled its clone-backed view;
each checked 64 blue pixels. Controlled core tests checked all mip texels for
both formats and rejected a same-mip copy. Before the change, the strict
baseline rejected a valid distinct-mip copy. Historical FL9 hardware remains
unverified; the public adapter also has the constrained HWND surface path
described in the device/adapter capability row above.

HAL texture creation eagerly uploads zeroes before returning, including for
partial-write targets and each mip of a supported chain. Allocation and uploads
are proportional to texture size; staging maps in buffer/texture transfers may
stall the CPU. Native diagnostic texture constructors remain separate and can
create uninitialized resources. The private color HAL accepts one-layer,
one-sample RGBA8 or BGRA8 2D textures and enforces feature-level size limits.
R8_UNORM, R8G8_SNORM, R16G16_SNORM and R8G8B8A8_SNORM support only a
single-mip, single-layer 2D texture with RESOURCE and/or COPY_DST usage; their
actual `TEXTURE2D` bits are checked at creation. R8_UNORM, R8G8_UNORM and
B5G6R5 also support RESOURCE and/or COPY_DST combined with COLOR_TARGET, plus
a target-only use, when the selected format query contains `RENDER_TARGET`.
Those three formats also support `COPY_SRC` only after that same positive
render-target query, because FL9_x requires an internal render-target bind for
GPU-to-CPU copies. Their render-target paths remain non-blendable. Microsoft's required-format table
lists R8_UNORM as a FL9_1 texture format without guaranteeing render-target
support; the implementation therefore requires an individual query. The
`D3D11_FORMAT_SUPPORT_RENDER_TARGET` bit (`0x4000`) and `BLENDABLE` bit
(`0x8000`) are distinct. See [required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats)
and [D3D11 format support flags](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_format_support).
R8G8_SNORM, R16G16_SNORM and
R8G8B8A8_SNORM remain non-renderable. A positive `SHADER_SAMPLE` bit
additionally reports `FILTERABLE`. D3D11 permits point-only sampling without
that bit, and the backend now supports that case for query-backed resources.
On FL9_1, public R16G16_SNORM use also requires `TEXTURE_FORMAT_16BIT_NORM`;
core uses the selected device's format flags to reject filtering layouts.
R8G8_UNORM is not a required FL9_1 sampled format and is exposed only after
its selected-device query reports `TEXTURE2D`. The forced-FL9_1 RTX reported
`0x0002c320`; strict public readback verified nearest sampling and the two-byte
texel stride, and a linear sampler produced the expected eight-column
gradient. The strict RTX R8_UNORM test observed support mask `0x0002d3e0` and
the optional R8G8_UNORM test observed `0x0002c320`; both include the
`RENDER_TARGET` bit (`0x00004000`). Their public path draws into each
attachment, then samples a view of the written texture into RGBA8. The
The first sampled pixels were `[66, 0, 0, 255]` and `[66, 129, 0, 255]`; all
64 pixels per format matched within one code value. B5G6R5 also requires the native-only
`TEXTURE_FORMAT_16BIT_PACKED` feature and a positive per-device query. On the
RTX at forced FL9_1 it returned `0x0026f3a0`, including both `RENDER_TARGET`
and `BLENDABLE`; blending remains unexposed until the format-specific blend
path is implemented and tested. A public draw into B5G6R5 followed by
sampling matched 64 pixels within one UNORM code of `[66, 130, 132, 255]`.
The strict public transfer test additionally uploads each of the three queried
formats, copies a partial rectangle between different texture dimensions and
reads the full destination into a nonzero-offset, 256-byte-pitched buffer. All
24 texels and untouched row padding are checked for R8, RG8 and RGB565. A
separate strict case uses each format with `RESOURCE | COLOR_TARGET | COPY_SRC`,
renders into it, then copies the rendered attachment to a nonzero-offset,
padded readback buffer. It checks all 64 source texels per format and confirms
the offset prefix and row padding retain their sentinel values. For RG8, the
native green byte is checked within one code of the RGBA8 sample because
render-target conversion can quantize the intermediate channel differently.
A missing or failed render-target query omits
`COLOR_ATTACHMENT` from the public projection and rejects attachment creation.
Multiple mips require POT
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
back the upload-only texture. Historical driver support remains unverified;
public adapter enumeration is available under the explicit `dx11` backend,
with the narrow HWND contract described in the Presentation row.

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
Historical hardware remains unverified; ordinary opt-in enumeration currently
exposes only the conservative FL9_1 offscreen projection.

HAL transfer commands use explicit regions. No staging-to-default native copy with
a null source box is used, avoiding the documented 9_x corruption case for
mismatched dimensions or destination offsets. Core validates empty regions and
returns before calling HAL; direct private HAL empty-region transfers remain
rejected. Broad texture formats, multisampling, NPOT mip chains and arrays
remain unsupported. A modern GPU forced to 9_1 does not
validate every historical runtime or driver; the experimental adapter's HWND
surface contract remains limited to the operations described above.

## FL9_1 sampler contract

| Category | Native and driver contract | Private backend and coverage |
| --- | --- | --- |
| Address modes | Clamp, wrap and mirror are available; `MIRROR_ONCE` is not the same as mirrored repeat and is unsupported. Ordinary border addressing is not generally available at 9_1/9_2. | Clamp, repeat and mirrored repeat map independently on U/V/W. Patterned GPU tests cover repeat, mirror and mixed U/V. Border addressing is rejected before native creation; all wgpu border colors may be carried only when no border addressing is used. |
| Repeat range | FL9_1 limits texture repeats to 128 over one primitive (`Draw`/`DrawIndexed`). No runtime query raises this feature-level guarantee. | Private HAL repeat sampling is exercised within controlled shaders. Core now rejects Repeat and MirrorRepeat sampler descriptors whenever the reported limit is finite, because it cannot prove interpolated coordinates stay within the native range for every draw. The modern RTX forced to FL9_1 does not validate historical behavior beyond the documented limit. |
| Min/mag filtering | Point and linear filters can be specified independently. | All four min/mag combinations map to D3D11 filters. GPU readback exercises both magnification and minification with contrasting texels; filter outputs differ. |
| Mipmap filtering | Point and linear mip filters exist. | Both are tested with sampled full-chain POT textures; suffix and restricted clone-backed non-suffix views passed on the RTX. General mipmap behavior remains unavailable. |
| LOD | FL9_1 native MinLOD must be integral and MaxLOD must be FLT_MAX. | A normalized minimum requires a one-mip view. When only a finite maximum is raised and the minimum is preserved, a used multi-mip view requires the original maximum to reach its last mip and anisotropy 1. Anisotropic sampling with a raised finite maximum remains one-mip-only. Other unrepresentable mixed-filter and anisotropic ranges are rejected. FL9 does not specify whether clamping precedes min/mag selection. FXC rejects `SampleLevel` for the tested pixel profile with X4532; explicit-LOD sampling is not claimed. |
| Anisotropy | FL9_1 native maximum is 2; an upper limit alone is not a visual-quality guarantee. | The private backend accepts 1 or 2, requiring linear min, mag and mip filters for 2. `DownlevelLimits::max_sampler_anisotropy` reports 2, and core clamps requested values to this cap before HAL creation. Native creation and a sampled GPU output passed; anisotropic quality on minified textures is unverified. |
| Comparison / border | Comparison filtering is not generally supported at 9_1; ordinary border addressing is not normally allowed. Optional 9_x shadow support is separate. | Comparison and border addressing remain rejected. On the RTX forced to 9_1, ordinary border sampler creation returned E_INVALIDARG. No optional wgpu border or comparison feature is advertised. |
| NPOT | FL9_x permits a single-mip 2D NPOT texture only when `AddressU/V/W` are not `D3D11_TEXTURE_ADDRESS_WRAP`; full NPOT is an optional driver query. `MIRROR` is distinct from `WRAP`. | The private HAL checks only used NPOT/repeat pairs unless full NPOT is positively reported. Core also has selected-entry pair validation, but its FL9_1 fixture reports the finite per-primitive repeat limit of 128 and consequently rejects Repeat and MirrorRepeat sampler creation before pair validation is reachable. Clamp and mirrored-repeat do not trigger the conditional-NPOT `WRAP` rule; MirrorRepeat remains subject to the separate finite repeat limit. Synthetic core tests exercise selected-entry pair validation under a repeat-permitting limit, while the strict physical fixture verifies the 128 limit and renders POT/NPOT clamp sampling. |

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
samplers. The public projection reports a maximum absolute literal bias of 3,
no dynamic bias and no sampling offsets. Core's selected-entry analysis follows
helper calls and rejects those unsupported forms as validation errors before
HAL pipeline compilation. Public readback verifies implicit sampling and
literal biases -3, +1 and +3 selected the expected mip colors across 256
pixels; synthetic interface tests cover fractional and helper cases. This
restricted implementation does not cover all valid biased sampling.

The public offscreen adapter reports anisotropy availability through the
`ANISOTROPIC_FILTERING` flag and the cap through
`DownlevelLimits::max_sampler_anisotropy`; the selected FL9_1 projection
reports 2. Core clamps the requested sampler value to this maximum, and strict
core fixture tests create native samplers for requests 2, 3 and 16. This does
not establish visual anisotropic quality on minified textures or validate
historical FL9_1 drivers.

The nondefault `dx11-test-fixture` feature still provides an unsafe adapter for
controlled core integration tests and synthetic capability overrides. Normal
adapter opening now uses the same selected-device projection, while the
fixture-specific constructor remains outside enumeration. Format projection
is centralized in `Capabilities::texture_format_capabilities`: it intersects
positive `CheckFormatSupport` results with feature-level shape limits and
backend implementation switches. Both enumerated adapters and fixtures use
this method for RGBA8, BGRA8, sampled-upload formats and D24S8. Public HWND
surface support is constrained to the BGRA8 sRGB FIFO contract above. The
controlled fixture tests transient descriptors and validation cases. A strict
public wgpu test now exercises transient color and D24S8 render passes through
core and HAL, checking core rejection of sampled usage, invalid load/store
operations, read-only transient depth and a pass with no attachments. Discarded transient contents are not read back because the public
usage contract forbids preserving or sampling them.

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
unfilterable formats or comparison samplers. Historical drivers remain
unverified; public offscreen adapter support does not widen this resource
contract.
Render-pass boundaries clear context state.

Sampled texture and sampler entries may declare `FRAGMENT` or
`VERTEX | FRAGMENT` visibility. Visibility is retained per binding in layout
identity and stage mappings; combined visibility does not enable vertex-stage
texture or sampler use. A selected vertex shader that samples a texture is
rejected before drawing. Two strict FL9_1 RTX images checked fragment sampling
and a transition between these layouts; a controlled core case checked 64
pixels. CPU mapping and compiler guards cover stage identity and selected
vertex entries. The original strict baseline rejected the combined layout.
Historical FL9 drivers remain unverified; no public surface is reported.

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
creation. FL9_1's usable profile budget is 255 vertex float4 constants and 32
pixel constants; Microsoft documents the selected-feature-level limits for
`VSSetConstantBuffers` and `PSSetConstantBuffers` in the
[10Level9 context reference](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context).
The selected FL9_1 capability projection reports 255 vertex vectors and 32
fragment vectors, intersected with the backend limits and confirmed against
FXC. Core counts the full 16-byte layout footprint of each statically used
uniform binding for the selected entry point, including helper calls, and
rejects a pipeline before HAL if the aggregate exceeds that stage's budget.
Unused uniforms do not count. This is deliberately conservative when a shader
reads only part of a declared uniform structure. FXC compiles 16+16 referenced
pixel vectors across two constant buffers and rejects 17+16 at register 32.
A strict public FL9_1 draw reads all 32 vectors from two sparse uniform
bindings and verifies the expected output; core rejects a 33-vector pipeline.
The current public projection additionally limits each binding to 512 bytes
and each stage to six bindings, so vertex aggregate usage cannot reach FXC's
255-vector ceiling through public layouts. Higher feature levels use a
different constant-buffer model and are intentionally not projected by this
FL9_1-only backend.

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
fixture flag is not a public adapter capability; the public projection uses the
selected device's native capability query without this override. The
selected-device FL9_1 core projection permits two dynamic
uniform buffers per pipeline layout. Strict core readback uses logical
bindings 17 (vertex) and 23 (fragment), with a base offset of 256 bytes and
dynamic offsets of 256 and 512 bytes respectively; geometry and sampled color
both change as expected. Core rejects three dynamic uniform entries against
the projected limit of two. This verifies the restricted core path, not a
publicly enumerated adapter. Three earlier RTX GPU results for integer-uniform bitcasts and a
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

For the private HAL, a used NPOT texture with a wrapping sampler is accepted
only when the `D3D9_OPTIONS` full-NPOT query returned true. A false or
unavailable query uses the conditional restriction. Microsoft specifies that
the restricted mode forbids `D3D11_TEXTURE_ADDRESS_WRAP` on U, V or W; mirrored
repeat maps to the distinct `D3D11_TEXTURE_ADDRESS_MIRROR` mode and is not
treated as wrap. The core also has selected-entry pair validation, but a
faithful FL9_1 core capability report sets the fixed per-primitive repeat limit
to 128. Since core cannot prove interpolated coordinates remain within that
limit for every draw, Repeat and MirrorRepeat sampler creation is rejected;
core pair-validation tests therefore use synthetic repeat-permitting limits.
The physical core fixture verifies both sampler rejections and renders POT and
NPOT clamp sampling. The RTX 3090 Ti reports full NPOT support under forced
FL9_1; restrictive optional-query behavior is tested synthetically. Multi-mip sampled views are POT-only;
native suffix views include the resource's last mip and other supported ranges
use per-draw GPU clones. Sampler checks include all address axes and use the
view-relative mip count and LOD. Strict suffix and non-suffix sampling passed
for both formats on the RTX FL9_1 device; the controlled core suffix fixture
produced 64 blue pixels and its non-suffix fixture produced 64 yellow pixels
after a source update. Synthetic tests exercise restrictive
capabilities; the RTX driver reports full NPOT support even when forced to FL9_1.
The HAL returns its existing generic `DeviceError::Unexpected` on draw rejection.
Binding arrays, comparison samplers, storage and vertex sampling remain
unavailable; the public projection remains FL9_1-only and reports no surface
capabilities.

The disabled texel-load proof reserves `t0`, `s15` and `b13`; it cannot simply
be switched on with this allocator. Enabling it requires coordinated register
reservations and collision tests. The active sampled path does not reserve
those slots or enable the proof.

The selected-entry `texture_query` downlevel limit covers WGSL image queries
such as `textureDimensions` and `textureNumLevels`. D3D11's 10Level9 pixel
profiles reject Naga's `GetDimensions` lowering with FXC X3548 for FL9_1 and
FL9_3; the documented float-out overload also cannot be mapped by those
profiles. Core follows helper calls and rejects only the selected entry point
before HAL. The model treats queries as a feature-level shader guarantee from
10_0 onward, but the current backend implementation remains false for every
level. This gate is profile/compiler-driven; it is not a driver format query.

`textureSampleBaseClampToEdge` has a separate selected-entry limit. Naga lowers
it to a clamped image sample at level zero, and the current HLSL writer emits
`GetDimensions` plus `SampleLevel`. FXC reports X4532 for this lowering under
`ps_4_0_level_9_1` and `ps_4_0_level_9_3`; the shader-model 5 control compiles.
The D3D11 projection enables the limit only when both queried hardware and
backend support for texture queries and explicit-level sampling are present.
The current implementation therefore reports it unavailable at every level.
Core rejects only selected entries that reach the builtin, including through
helpers; an unused declaration or safe sibling entry is unaffected.

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

### Vertex shader system-value inputs

The current private HAL does not implement vertex `vertex_index` or
`instance_index` inputs. `BackendCapabilities` records both as unavailable, and
the controlled FL9_1 core profile reports
`DownlevelLimits::vertex_index_input = false` and
`DownlevelLimits::instance_index_input = false`; core rejects the selected
entry-point declaration before HAL. The `vs_4_0_level_9_1` FXC test rejects
the backend-generated `SV_VertexID` interface with FXC X4555 whether its
WGSL value is used or unused. This is observed compiler output; it does not
prove an inherent hardware restriction. `instance_index` is currently a
backend implementation restriction; its profile-specific FXC behavior has not
been verified. Microsoft describes SV system-value semantics as Direct3D 10
additions, while 10Level9 exposes D3D9-era shader behavior; this mapping does
not claim that these semantics are impossible on every implementation.

`primitive_index` is separately gated by optional
`Features::PRIMITIVE_INDEX`; the public D3D11 projection does not advertise
that feature while the HAL rejects it. [Microsoft HLSL semantics](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-semantics)
and [10Level9 device rules](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
are references for the hardware model, not substitutes for the tested FXC
profile.

## Internal FL9_1 wgpu projection

`Capabilities::wgpu_projection` builds a conservative wgpu-types snapshot only
when the selected device actually runs at FL9_1. It intersects selected-level
facts, driver queries and backend paths. Ordinary opt-in D3D11 enumeration uses
this same snapshot for physical adapters, and the controlled core fixture also
consumes it with additional synthetic restrictions where needed.

The projection preserves the native 2048×2048 2D minimum and logical binding
indices through 999. The binding-index limit is distinct from implementation
capacities: the private pipeline allocator still caps aggregate resources at
eight textures, sixteen samplers and thirteen uniform slots. The projection
limits each shader stage to six uniforms so disjoint vertex and fragment
bindings together cannot exceed that shared allocator. Core receives
explicit downlevel gates for 1D and 3D textures because inherited nonzero
hardware size limits do not mean these resource shapes are implemented by this
backend. The numeric projection remains 2048/256 while the dimension booleans
are false; core rejects creation before reaching this 2D-only HAL.
Other unsupported shader, storage, query and multisample paths remain absent
from projected flags or format usages.

The projection reports `empty_binding_visibility = false`. WebGPU permits a
bind-group layout entry whose visibility bitset is empty, but the current HAL
layout representation requires every resource entry to target a supported
stage. Core rejects this layout shape before HAL for the FL9_1 projection.

The projection also reports `sampled_depth_textures`, `sampled_sint_textures`,
`sampled_uint_textures` and `multisampled_texture_bindings` as false. The
private layout mapper supports only float sampled textures; attachment-only
D24S8 does not provide a depth-sampling path, and the implemented sampled
formats provide no signed- or unsigned-integer sample class. The renderer is
single-sample-only. Core rejects those layout types before HAL. A vertex-only
texture or sampler layout is also rejected when the separate vertex
sampled-texture limit is zero; fragment-visible layouts remain valid, and
actual vertex shader sampling remains rejected at pipeline validation.

The projection withholds `Features::TEXTURE_COMPRESSION_BC`: wgpu's BC feature
guarantees the BCn family, while this backend currently implements only BC1.
The private core fixture adds the aggregate feature locally when it specifically
tests its BC1 path; this does not establish that the production projection can
advertise BC support. Enabling a truthful public report requires implementing
the required BC formats or adding a narrower core capability contract.
