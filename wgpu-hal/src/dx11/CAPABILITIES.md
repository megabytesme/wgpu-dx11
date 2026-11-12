# D3D11 capability and public-adapter contract

## FL9_1 fragment textureGather emulation

The native FL9_1 shader profile does not provide HLSL Gather. The backend does
not advertise general `DownlevelLimits::texture_gather`; it reports only the
narrow `texture_gather_2d_float_fragment` capability. That limit permits one
direct fragment `textureGather` operation on a single-mip, single-sample
`Rgba8Unorm` or query-supported `Rgba8UnormSrgb` 2D float view with a regular
sampler using clamp-to-edge on both U and V. Core checks the actual bound view
format at draw time because the limit cannot encode a format list; unsupported
formats are rejected before HAL. The selected entry point must pass its location-0 `vec2f` argument
directly and return the gathered `vec4f` at location 0. Helper functions,
additional image operations and other shader shapes remain unavailable.

Naga lowers this proven operation to four point samples at texel centers and
assembles the requested channel in WGSL gather order. HAL binds the selected
image at reserved `t0`, supplies dimensions through reserved `b13`, and binds
an internal point/clamp sampler at reserved `s15`. Core checks the actual
currently bound view and sampler at render and bundle draw boundaries. The
physical RTX 3090 Ti at forced FL9_1 verified all 64 linear-view output pixels
against `[151, 166, 106, 91]` and all 64 sRGB-view pixels against the
linearized result `[79, 97, 37, 27]`. Core rejected an R8 view and a multi-mip
view before submission. This
is backend emulation rather than hardware gather support, does not enable the
general gather limit, and has not been verified on historical FL9_1 drivers.

## Additional strict FL9_1 evidence — 2026-11-11

Core and the private HAL now support a narrowly proved direct fragment
`textureSampleCompare` or `textureSampleCompareLevel` with one static `i32x2`
texel offset in `[-8, 7]`, using a 2D depth view that exposes exactly one mip
and one sample. Core rechecks the currently bound view before each draw. The
DX11 capability is exposed only when the FL9_1 device has a positive D3D9
shadow-comparison query; missing and negative results remain unsupported. The
HAL binds the selected view at the reserved texture slot and supplies its
dimensions through the reserved constants, while Naga rewrites the offset as
normalized-coordinate arithmetic because FXC rejects the native offset form
for `ps_4_0_level_9_1`. Strict RTX tests compiled and executed both WGSL
builtins and verified the one-texel shifted comparison boundary across all 64
pixels. This is a bounded emulation proof, not general depth sampling or
comparison-offset support on every FL9_1 driver.

The strict public BC1/BC2/BC3 subset now verifies ordered CPU-shadow
texture-to-buffer readback, padded rows and offsets, BC cube faces, and
UNORM/sRGB-compatible block copies. The shadow is an explicit CPU mirror of
ordered writes and copies; it is not a native GPU readback and requires every
future write path for these resources to keep the mirror current. The same
strict suite verifies partial BC initialization and sampling.

The backend also implements a narrowly proven fragment `textureSampleBias`
offset subset: one direct sample or one sample inside a single coordinate-
forwarding helper, from a 2D single-mip view, with a constant `i32x2` offset in
the FL9_1-supported range. The selected view may be mip 1 of a multi-mip
parent; the view itself must expose one mip. HLSL emulates the texel offset
using the selected view dimensions. Nested helpers, modified coordinates,
multiple samples, other texture shapes and arbitrary dynamic bias remain
unsupported.

Strict hardware verification used NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested and actual FL9_1,
`software=false`. A modern GPU restricted to FL9_1 does not establish
historical-device behavior.

Recent strict public execution checks covered partial initialization and
sampling of BC1/BC2/BC3 unorm textures, plus WGSL matrix `determinant` for
mat2/mat3/mat4 in vertex and fragment stages. All seven focused tests passed
on the physical RTX 3090 Ti at requested/actual FL9_1. This is device-specific
execution evidence for paths already present in the restricted projection;
it does not imply support on every historical FL9_1 driver or change the
capability contract. See `TESTING.md` and `FL9_1_PROGRESS.md`.

Core now tracks whether the selected fragment entry point uses
`textureSampleBias` and validates only that entry point's sampled sampler
bindings against the FL9_1 D3D11 requirement of equal min/mag filters and
effective anisotropy 1. The default-false downlevel limit is enabled only in
the DX11 projection; GLES, noop and other defaults retain their previous
behavior. Validation runs before direct draws and render-bundle recording can
reach the HAL. Strict RTX execution confirms the supported equal-filter path
still samples biased mip levels, while a public negative test confirms mixed
filters and anisotropy 2 are rejected as Core validation errors. The Rust
`RenderBundleEncoder::finish` wrapper treats Core finish errors as fatal, so the
bundle guard is not exercised through a public error scope. This is a narrow
validation contract, not broader bias or shader-profile support.

A separate strict public regression verifies that `@invariant
@builtin(position)` reproduces prepass depth: an Equal-depth color pass matched
all 64 pixels on the RTX 3090 Ti at forced FL9_1. This is execution evidence
for that shader/pipeline shape only, not a new capability claim.

The strict public render-state matrix also combines query-supported RGBA8
multisampling, alpha blending and a partial color write mask. On the RTX 3090
Ti at forced FL9_1, 2x/4x/8x rendering and resolve each matched all 64 expected
pixels. This verifies the combined state path on this driver; it does not
extend the selected-device capability flags.

The opt-in `dx11` feature now enumerates physical DXGI adapters that can create
the explicitly requested FL9_1 device and exposes the selected-device
capability projection for public wgpu rendering. Its constrained HWND and
validated WinRT CoreWindow surface contracts are BGRA8 sRGB FIFO, opaque
composition, render-attachment usage and one frame of latency. CoreWindow
construction verifies dispatcher-thread access; configure, acquire and
present must run on the captured owner thread. Only selected-device-gated
features are exposed: the implemented 16-bit normalized/packed texture paths
and comparison-sampler border addressing when the required query succeeds.
The downlevel contract is not WebGPU-compliant. This is a restricted FL9_1
adapter report, not a claim of general wgpu support. The
internal model separates feature-level minima, per-device query results, and
implemented backend paths. `check_private_texture_2d` is a conservative
predicate for the private RGBA8, BGRA8, D24S8, R8_UNORM, R8G8_UNORM, R8G8_SNORM,
R16G16_SNORM, R8G8B8A8_SNORM, B5G6R5 and BC1/BC2/BC3 unorm and sRGB subsets;
it is not a complete texture validation rule.

DX11 is excluded from `Backends::default()`, but adapter enumeration is not
globally disabled: when the Cargo `dx11` feature is enabled, applications that
explicitly request `Backends::DX11` (or `Backends::all()`) can enumerate the
physical adapters that pass this FL9_1 projection. The default-isolation test
only verifies that the default instance does not enable DX11.

For RGBA8/BGRA8 linear and sRGB formats, each reported usage is now projected
from its own selected-device query and production usage check. Missing render-
target support no longer hides independently valid sampled and copy-destination
use. This changes capability reporting only; unsupported combinations remain
rejected by the HAL checker.

This file also retains historical audit notes. Any older statement below that
adapter enumeration is offscreen-only or that surface reporting is disabled
describes an earlier checkpoint; the current public surfaces are limited to
the HWND and CoreWindow contracts summarized above. CoreWindow runtime use on
UWP/Lumia remains unverified.

## Storage limits

Compute, storage buffers and storage textures are not implemented for this
FL9_1 projection. The adapter reports zero compute limits, storage counts,
dynamic storage buffers and storage-buffer binding size instead of inheriting
the generic downlevel defaults. Core also omits the compute/storage downlevel
flags and rejects those pipelines and binding layouts before HAL. Creating a
buffer with the `STORAGE` usage bit remains distinct from binding it as a
shader resource and does not enable storage access.

## WGSL fragment discard

The ordinary WGSL fragment `discard` statement is translated by Naga's HLSL
writer and accepted by FXC for `ps_4_0_level_9_1`. A strict public GPU test
checks the selected conditional-discard path at requested/actual FL9_1: all
discarded pixels retain the attachment's prior color and depth, while covered
pixels write color and depth. A later depth-tested draw distinguishes both
halves. This records tested behavior for that shader shape; it does not remove
the other FL9_1 shader-profile restrictions or claim full WGSL coverage.

## FL9_1 mutable format views

The backend does not expose `DownlevelFlags::VIEW_FORMATS` at FL9_1. Although
RGBA8 and BGRA8 UNORM and sRGB are individually usable typed formats, a mutable
UNORM/sRGB view pair requires typeless native resource backing. Microsoft's
10Level9 FL9_1 format table marks both corresponding typeless formats as
unavailable. The forced-FL9_1 RTX 3090 Ti also rejects typeless RGBA8 and BGRA8
`CreateTexture2D` probes with `E_INVALIDARG` (`0x80070057`). Core therefore
rejects otherwise compatible sRGB `view_formats` before the HAL. Standalone
typed sRGB resources and views remain supported where their individual format
queries permit them; ordinary views selecting the resource's existing format
remain unaffected. This is not a driver query that can safely be generalized
to higher feature levels.

Microsoft reference: [10Level9 FL9_1 format support](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/format-support-for-direct3d-feature-level-9-1-hardware).
A failed query is recorded with its HRESULT and never treated as support.
Adapter metadata includes the DXGI driver-package version when
`IDXGIAdapter::CheckInterfaceSupport(IDXGIDevice)` succeeds; older or
unsupported runtimes leave it unavailable. See Microsoft's
[`CheckInterfaceSupport` contract](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiadapter-checkinterfacesupport).

## FL9_1 cube texture subset

The 10Level9 device contract allows a cube resource at FL9_1 when
`D3D11_RESOURCE_MISC_TEXTURECUBE` is set and `ArraySize` is exactly six; cube
dimensions must be square and power-of-two; multi-mip dimensions must also be
integral powers of two. The FL9_1 guaranteed maximum cube dimension is 512,
below the 2048 2D-texture limit. The backend implements single-sample, sampled
D2 cube resources with `TEXTURE_BINDING` and optional `COPY_DST` and `COPY_SRC`
usages. GPU copies work between compatible cube faces and ordinary 2D texture
subresources in either direction; strict FL9_1 readback sampled a cube face
copied to 2D, and sampled a 2D image copied into a cube face. Cube-to-buffer
readback uses a staging texture for each selected face. When a `COPY_SRC` cube
format has positive `D3D11_FORMAT_SUPPORT_RENDER_TARGET` support, the default
resource also receives `D3D11_BIND_RENDER_TARGET`; this bind flag is required
because FL9_x forbids GPU-to-CPU copying from a resource with only
`D3D11_BIND_SHADER_RESOURCE`. No RTV is created, and cube render-target views
remain unavailable. Cube-to-buffer format support remains query-gated. BC
formats with a positive texture-to-buffer capability use the ordered CPU
shadow for raw block readback. The cube-format and texture-to-buffer masks are
independent: the first requires per-format `TEXTURE2D | TEXTURECUBE` support
plus the sampled-upload backend path, while the second requires its own
`COPY_SRC` and readback-path gates.
Mip chains require the selected device's `D3D11_FORMAT_SUPPORT_MIP` query, the
implemented POT mip path and a format not projected as single-mip-only. Each
mip must be supplied by the caller; automatic mip generation is unavailable.
Ordinary 2D arrays, cube arrays, render-target cubes, and individual 2D face
views remain unavailable. Core gates cube creation and views on
`CUBE_TEXTURES`, a separate 512 cube limit, the NPOT policy, and a format mask
before the request reaches HAL.

Ordinary texture arrays are a feature-level restriction rather than a missing
backend implementation: Microsoft's downlevel feature table lists texture
resource arrays as unavailable at FL9_1, FL9_2 and FL9_3. The supported six
cube faces are the separate `TextureCube` resource shape, not a general array
view. Public tests confirm the array capability stays unset, reject a
two-layer 2D texture, and reject a `D2Array` sampled binding layout before HAL.

The mask's bits follow `DownlevelLimits::texture_cube_format_bit` for RGBA8,
BGRA8 and their sRGB variants, R8, RG8, RG8_SNORM, RG16_SNORM, RGBA8_SNORM,
BC1/2/3 UNORM and sRGB, RGB565, RGB5A1, and RGBA4. Each BC variant has a
distinct bit. D3D11 sets a bit only when the selected device reports both
`TEXTURE2D` and `TEXTURECUBE` and the format is also sampled by the implemented
backend. This avoids treating 2D sampling support as cube support. The RTX 3090
Ti at forced FL9_1 reports mask `0x0003ff1f`; all six BC variants passed the
selected-device checks and six-face upload/readback test. A strict public
render test sampled each face for all six formats and checked the decoded
face colors with a two-byte channel tolerance. RG8_UNORM,
RG8_SNORM, and RG16_SNORM are not projected as cube formats on this driver,
even though some remain usable as 2D textures.
Strict GPU readback sampled all six RGBA8 faces and four mip levels on the RTX
3090 Ti at forced FL9_1; all 960 pixels matched their face/mip colors. A
non-suffix view selecting levels 1 and 2 also passed through the six-face GPU
clone-refresh path required by the FL9_1 lowest-LOD SRV rule. This is
selected-driver evidence, not evidence for historical FL9_1 implementations.
`COPY_DST` permits ordered buffer-to-texture face/mip uploads, and `COPY_SRC`
permits GPU-to-GPU copies of cube-face subresources. A strict public test copies
one face across every mip and observes the changed sampled face. CPU readback
of RGBA8 and other query-supported copy formats is available for one face or
all six faces. The downlevel texture-to-buffer layer limit is six for this
cube shape; ordinary arrays remain disabled by the separate
`TEXTURE_2D_ARRAYS` flag. The strict HAL test verifies all 384 texels from six
asymmetric faces through a 256-byte-pitched buffer and checks all image, row,
and buffer padding. BC1/BC2/BC3 cube-face raw-byte readback uses the ordered CPU
shadow rather than a render-target bind. The strict RTX test verifies six faces
for each of BC1/2/3 UNORM and sRGB, after each format is independently admitted
by its cube bit, `COPY_SRC` usage and texture-to-buffer bit. This modern-driver
result does not establish the same optional support on historical FL9_1 GPUs.
Cube render-target views and single-face D2 views remain unsupported.

Microsoft references: [10Level9 device constraints](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device),
[feature-level resource limits](https://learn.microsoft.com/en-us/windows/desktop/direct3d11/overviews-direct3d-11-resources-limits),
[feature-level capability table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro), and
[format support queries](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11device-checkformatsupport).

## Legacy shader built-ins

The level-9 HLSL profiles used by this backend cannot compile the legacy
`BaseVertex`, `BaseInstance`, and `PointSize` vertex interface semantics or the
`PointCoord` fragment input. Some GLSL and SPIR-V inputs express these
built-ins even though the equivalent WGSL/downlevel path has no support. The
public FL9_1 projection keeps `vertex_base_vertex_input`,
`vertex_base_instance_input`, `vertex_point_size_output`, and
`fragment_point_coord_input` false. Core checks the selected entry point's
validated interface and rejects only a pipeline that uses one of them, before
HAL shader compilation. Ordinary entry points in the same module remain
eligible. This is a shader-profile implementation restriction; it does not
change the feature-level guarantees or enable additional shader operations.

## Selected FL9_1 floating-point built-ins

The public forced-FL9_1 shader path has strict readback coverage for WGSL
`modf`, `asinh`, `acosh`, and `atanh`, each driven by a runtime uniform and
checked over a complete 8x8 RGBA8 target. This records support for those tested
forms; it does not claim general WGSL builtin coverage. The backend also
permits only a direct scalar-f32 fragment expression `frexp(value).fract`.
Core and HAL prove that the `frexp` result is projected to its fraction in the
same Naga emission range; the exponent, whole result, local whole-result alias,
helper forwarding, vector form, vertex-stage use and other runtime integer
operations remain rejected. `DownlevelLimits::frexp_fraction_only_fragment`
defaults false and is set only in the selected FL9_1 D3D11 projection.

Microsoft's [HLSL `frexp` reference](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-frexp)
lists shader model 3 as its minimum, so the source intrinsic alone is not
treated as an FL9_1 guarantee. The installed FXC,
targeting `ps_4_0_level_9_1`, removes the unused exponent and emits the fraction
path in its `ps_2_0` Level9 section using floating-point instructions. A strict
public readback passed for positive, negative and zero runtime inputs (64
pixels each). Other FXC versions and historical drivers have not been tested;
shader compilation still fails the pipeline if their lowering cannot produce
the selected profile. This is a narrow compiler/backend subset, not general
runtime integer support.

## Feature-level and driver facts

The selected device level, not the adapter's maximum, determines the guaranteed
minimum. For FL9_1, the baseline includes 2048 2D dimensions, one render target,
65,535 primitives per draw, index values through 65,534, vertex and pixel
shaders, and conditional NPOT textures. These are minimum guarantees, not
assertions that larger sizes work on every device. Level 9_1 has no guaranteed
separate alpha blending, hardware instancing, geometry or compute shader stage,
or native UAV storage. Microsoft explicitly marks Separate Alpha Blend as
unavailable at FL9_1; there is no selected-device query that safely promotes
the capability. The public projection therefore keeps `SEPARATE_ALPHA_BLEND`
unset and core rejects differing RGB/alpha equations when both channels are
written. An RTX-only diagnostic that happens to render such states is not
evidence of the FL9_1 contract. The model also records constant and
slope-scaled raster depth bias as hardware guarantees, and `DepthBiasClamp` as unavailable at 9.x
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
shader-sample support beginning at FL9_1. The required-format table also
requires BC1, BC2 and BC3 for texture compression at FL9_1 ([Microsoft BC
support](https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11)).
The backend records these minima separately from the selected device's queried
format bits.

The same table requires `R16G16_SNORM` 2D textures at FL9_1, but its
`SHADER_SAMPLE` requirement begins at FL9_2. FL9_1 therefore has no static
sampled-format guarantee for this format. D3D11's format-support contract
allows point-filtered `Sample` when `TEXTURE2D` is present but
`SHADER_SAMPLE` is absent. The HAL permits the sampled/upload path after a
positive `TEXTURE2D` query and reports `FILTERABLE` only when
`SHADER_SAMPLE` is also present. A multi-mip path additionally requires a
positive `MIP` query and the implemented mip-chain path; it remains point-only
when `SHADER_SAMPLE` is absent. Query failure or a missing `TEXTURE2D` bit is
rejected. The format is not renderable. Same-format GPU copying and
texture-to-buffer readback are separate query/backend-gated paths; readback
currently uses an execution-ordered CPU shadow rather than native staging.

`R8G8_UNORM` has no required FL9_1 sampled-texture guarantee. The backend
therefore queries the selected device and enables only its single-mip,
single-layer sampled/upload path when `CheckFormatSupport` reports
`TEXTURE2D`. `SHADER_SAMPLE` separately controls the `FILTERABLE` projection.
When the same query also reports `RENDER_TARGET`, the backend permits the
single-mip sampled color-attachment path. Blending is separately exposed only
when that query also reports `BLENDABLE` and the native pipeline path accepts
the format. It has no copy-source, direct-readback or mip-chain support unless
the corresponding selected-device requirements are met.
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

Without the optional full-NPOT support, conditional NPOT textures at FL9 require
one mip and restricted addressing. A positive `FullNonPow2TextureSupport` query
allows the backend's query-gated 2D NPOT mip path when the selected format also
reports `MIP`. Full NPOT and comparison-shadow support are separately queried using `CheckFeatureSupport`;
a newer runtime or driver may return E_INVALIDARG for an unknown query, which
is retained as unavailable. `CheckFormatSupport` is queried only for RGBA8,
BGRA8, RGBA8_SRGB, BGRA8_SRGB, R8_UNORM, R8G8_UNORM, R8G8_SNORM, R16G16_SNORM,
R8G8B8A8_SNORM, BC1_UNORM, BC2_UNORM, BC3_UNORM, B5G6R5, B5G5R5A1, B4G4R4A4, D16_UNORM and D24S8, and
`CheckMultisampleQualityLevels` for RGBA8, D16_UNORM and D24S8 at 2, 4, 8 and
16 samples. Each positive count is projected independently; zero or failed
queries remain unsupported. The backend currently permits query-supported
2x, 4x and 8x RGBA8 render/resolve and matching D16/Depth24Plus/D24S8 depth
attachments. 16x is not implemented because the selected RTX query is zero.
Depth sample queries add attachment-only sample-count flags; they do not enable
depth resolve or multisampled shader bindings.
A separate FL9_1 shadow path is available only with a positive
`D3D11_FEATURE_D3D9_SHADOW_SUPPORT` query. Microsoft's [shadow-specific
contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_feature_data_d3d9_shadow_support)
permits border addressing on this comparison sampler even though
ordinary border samplers are unavailable at FL9_1. The public projection
therefore exposes `ADDRESS_MODE_CLAMP_TO_BORDER` together with a downlevel
limit requiring comparison samplers whenever this query succeeds. This does
not enable ordinary border samplers or the separate clamp-to-zero feature.
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

The private HAL clamps a viewport rectangle to the attachment and selects a
separately compiled FL9_1 vertex shader variant when clipping or fractional
coordinates change its transform. This is required because FL9_x
`RSSetViewports` uses DWORDs internally, so fractional coordinates are not
preserved by the native viewport. The adjusted shader can fail FXC's level-9
instruction limits even when the original shader compiles. Pipeline creation
retains the ordinary shader and records whether its optional adjusted variant
exists; Core rejects a direct draw or executed bundle draw using a fractional
viewport when that pipeline lacks the variant. Device errors from
adjusted-shader object creation still fail pipeline creation. Core carries the
outside-target restriction as
`DownlevelLimits::viewport_rectangles_outside_target`; when it is false,
rectangles extending past any attachment edge are rejected during
render-command validation before HAL. The strict public test verifies that a
shader at the remap instruction boundary creates and renders with a full-target
viewport, then rejects its fractional-viewport draw in Core. The controlled
FL9_1 core fixture tests the out-of-target rejection. Another strict public
test verifies a 4×6 scissor inside an 8×8 target (24 green and 40 unchanged
blue pixels). Other backends retain the default true capability. See Microsoft's
[D3D11_VIEWPORT feature-level behavior](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_viewport).

## Recorded forced-FL9_1 observation

On 2026-11-02, an RTX 3090 Ti (`0x10de:0x2203`), Windows 11 build 26300,
NVIDIA driver 32.0.16.2101, returned FL9_1 for an FL9_1-only request. Its
format-support masks were RGBA8 `0x312ef3e2`, BGRA8 `0x312ef3e0`, and D24S8
`0x00210020`. RGBA8 reports 2D texture, shader sampling, render-target and
blend support. RGBA8 and BGRA8 quality-level counts for 2x, 4x, 8x and 16x
were each 1, 1, 1 and 0; D16 and D24S8 sample counts are queried
independently. Full NPOT and
comparison-shadow queries returned true. The positive shadow query enables
only a narrow FL9_1 public depth-comparison subset: `LessEqual`, equal
minification/magnification filters and anisotropy 1. It does not enable other
comparison functions, unrestricted mipmaps or unrestricted comparison
sampling. The private HAL has the same query-gated path, documented below. A
later query-gated path uses each positive RGBA8 or BGRA8 sample-count result
only for same-format single-mip color rendering and resolve, and the D24S8 query for single-mip
Depth24Plus/D24S8 depth attachments in matching passes; D16 has its own
per-count query. It does not enable multisampled
texture bindings or depth resolve. The private indexed draw passed 64/64 readback pixels. Later strict
tests passed ordinary BGRA8 sampling, rendering and raw transfer readback on
the same GPU.
This forced level on a modern GPU does not emulate all historical FL9_1
hardware or establish portability of separate-alpha blending.

On 2026-11-03, the same adapter at requested and selected FL9_1 returned
R8_UNORM support mask `0x0002d3e0`, including 2D and shader-sample support.
The latter is the 10Level9 filtered-sampling indication. A strict test uploaded
a padded 5×4 image from byte offset 1, sampled it with nearest filtering into
RGBA8 and matched all 80 output pixels. A later strict test also verified its
query-gated render-target, partial-copy and padded CPU-readback path. R8 linear
filtering is enabled only when the query indicates filtered sampling; public
GPU coverage verifies linear filtering for both R8_UNORM and RG8_UNORM.

The same adapter returned R8G8_SNORM support mask `0x00021320` at requested
and selected FL9_1. A strict test uploaded padded signed RG pairs, sampled and
remapped both channels to RGBA8, and checked all 80 output pixels within one
UNORM code value for conversion rounding. The selected device also reports
`MIP`; a query-gated three-level sampled/upload path is implemented and tested.
Render-target, copy-source and CPU readback uses remain unsupported.

On 2026-11-05, the adapter returned `0x00021122` for `R16G16_SNORM` at
requested and selected FL9_1. The mask includes `TEXTURE2D` but not
`SHADER_SAMPLE`, matching Microsoft's FL9_1/FL9_2 minimum split. A strict
private HAL test uploaded a padded image, point-sampled it and matched 80/80
pixels. The public adapter now exposes the required
`TEXTURE_FORMAT_16BIT_NORM` feature and a non-filterable sampled/upload
capability; core rejects a filtering layout using the queried format flags.
The strict public point-sampling test also passed. The selected query includes
`MIP` without `SHADER_SAMPLE`; a query-gated three-level upload is tested by
selecting mip 1 through a single-mip view and point-sampling it. A separate
strict test validates mip-1 partial GPU copy and raw readback. This modern-
driver result does not establish historical-driver behavior.

The selected `TEXTURE2D` query and backend same-format copy implementation
also gate `Rg16Snorm` `COPY_SRC` for GPU texture-to-texture copies. The
texture-to-buffer downlevel mask is separately gated by an ordered raw-byte
CPU-shadow path, avoiding native staging readback for this SRV-only format.
Strict RTX coverage validates partial-copy sampling and exact raw readback
with padded rows and sentinels.

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
backend implements a narrow sampled-upload subset as `Rgb5g6b5Unorm`,
`Rgb5a1Unorm` and `Rgba4Unorm`, gated by `TEXTURE_FORMAT_16BIT_PACKED` and
separate selected-device `CheckFormatSupport` queries. RGB565, RGB5A1 and RGBA4
permit sampled/upload mip chains when their respective selected query reports
`TEXTURE2D | MIP`; the backend path is separately gated. At forced FL9_1 the
RTX returned `0x0026F3A0` for B5G6R5_UNORM and
`0x000213A0` for each of B5G5R5A1_UNORM and B4G4R4A4_UNORM. Those masks include
2D texture and shader-sample support. The strict public test uploaded padded
two-byte texels, sampled all three native SRVs and matched every RGBA8 output
pixel, including the 1-bit and 4-bit transparent alpha cases.

RGB5A1 and RGBA4 support `TEXTURE_BINDING | COPY_DST` and same-format GPU
texture-to-texture `COPY_SRC`, for one 2D layer, one sample and one mip. Their
copy path requires the selected format query's `TEXTURE2D` bit and backend
implementation gate; it does not require `RENDER_TARGET` and does not add a
native render-target bind flag. Texture-to-buffer readback is provided through
an execution-ordered raw CPU shadow, separately gated from GPU `COPY_SRC`;
the selected format query must still report `TEXTURE2D`. These formats remain
unavailable for render attachments. Packed-format mip-chain capability does
not imply mip generation or render-target support. B5G6R5 also permits
render-target, copy-source and CPU-readback use when its selected-device query
reports `RENDER_TARGET`; its draw/sample and partial-copy/readback paths have
strict GPU coverage. `SHADER_SAMPLE` controls filterability. The strict packed
format test uploads query-gated mip chains for RGB565, RGB5A1 and RGBA4.
RGB565 samples its uploaded mip 1 and verifies the packed word through
readback; RGB5A1 and RGBA4 upload four levels, copy within mip 1, read the
source region and full destination mip into padded buffers, then sample a
single-mip view. All checked words, borders, padding, sentinels and
sampled pixels matched. The modern RTX result verifies the query-selected
path, not behavior of historical FL9_1 drivers.

The same adapter returned R8G8B8A8_SNORM support mask `0x000213a0` at
requested and selected FL9_1. This includes `TEXTURE2D | MIP | SHADER_SAMPLE`.
The strict one-mip test uploaded padded four-channel signed data, sampled and
remapped all channels to RGBA8, then matched all 80 pixels within one UNORM
code value for conversion rounding. A separate query-gated three-level
sampled/upload test selected mip 1 with a literal bias and matched all 16
pixels. A separate single-mip, same-format GPU copy-source path is query-gated
by `TEXTURE2D`; render-target use remains unsupported. CPU texture-to-buffer
readback uses a query-gated raw-byte shadow, while native staging readback
remains prohibited for the SRV-only FL9_x resource.

The strict public operation test now combines the query-gated mip path with
same-format GPU copy: it uploads four-level RGBA8_SNORM textures, copies a
partial mip-1 region between nonzero origins, then samples a mip-1 suffix view
into RGBA8. All 16 copied and untouched pixels matched within one byte on the
RTX 3090 Ti forced to FL9_1. A separate strict test reads raw RGBA8_SNORM
bytes after upload and partial GPU copy, checking a nonzero buffer offset,
padded rows, zero-initialized untouched texels and copied bytes. The
downlevel readback bit requires both the format's COPY_SRC path and the
backend shadow implementation.

The adapter returned BC1_UNORM support mask `0x000213a0` at requested and
selected FL9_1. The initial strict test uploaded four solid-color blocks with a
nonzero buffer offset and padded rows, then replaced one block. The current
private subset supports one-mip, one-layer, 4-aligned BC1/BC2/BC3 unorm and
sRGB textures with RESOURCE and/or COPY_DST usage and queried GPU texture-copy
source usage. Upload and copy regions must cover complete aligned blocks.
Before the ordered shadow was added, BC texture-to-buffer readback was
unavailable. The current narrow path enables it only for queried BC1/2/3
`COPY_SRC` formats with the backend shadow path active. Render-target use, mip
chains, non-block-aligned dimensions and cross-format BC views remain
unavailable. Microsoft's
[10Level9 context restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context)
permit FL9_1 copies within GPU-accessible Texture2D resources and prohibit
GPU-to-CPU copies from resources carrying only the shader-resource bind flag.

The implementation also supports BC2_UNORM and BC3_UNORM sampled uploads and
same-format GPU texture copies. The public adapter exposes these implemented
unorm formats through the native-only `TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM`
feature when all three queried sampled-upload paths are available. This does
not expose the broader `TEXTURE_COMPRESSION_BC` feature. All three formats use
independently queried `CheckFormatSupport` results and share the 4×4, one-mip, one-layer,
block-aligned rules; block row pitches and source offsets must be multiples of
the format's 8-byte (BC1) or 16-byte (BC2/BC3) block size. Strict forced-FL9_1
tests uploaded a patterned image, replaced one block at a nonzero origin,
copied the result to a second resource and sampled all 256 output pixels. On
the RTX 3090 Ti, BC2 and BC3 each returned support mask `0x000213a0` with
linear-filter support. BC1/BC2/BC3 texture-to-buffer readback is available
through the ordered CPU shadow for queried `COPY_SRC` formats; it is not a
native staging read. Strict public tests check raw unorm bytes for 2D textures
and all six BC1 unorm cube faces. They also directly read back all three BC
sRGB source formats, checking raw blocks, offsets, row padding and sentinels.
Compatible sRGB-to-unorm copies verify preserved raw blocks through the
destination shadow. Render targets, mip chains,
non-block-aligned dimensions and UNORM/sRGB view reinterpretation remain
unavailable. Cross-family BC copies (such as BC1 to BC2) remain unavailable.
A separate native-only `TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB`
feature exposes the typed BC1/2/3 sRGB variants only when all three sRGB
sampled-upload paths pass their own selected-device queries and backend checks.
Their shader-resource views perform sRGB decoding; same-format GPU copies are
supported when the per-format usage query allows them. The strict public test
also sampled three compressed mid-gray sRGB textures after GPU copies and
verified all 64 pixels near linear `[59, 57, 59, 255]`, distinguishing sRGB
decode from unorm sampling. BC4 through BC7, mip chains and render-target use
remain unavailable. UNORM↔sRGB view reinterpretation and cross-family BC copies
remain unavailable. Core's texture-to-buffer format mask includes queried
BC1/BC2/BC3 copy-source formats only when the ordered shadow implementation is
enabled. The strict RTX test directly reads all three BC sRGB formats at
requested/actual FL9_1, in addition to checking a compatible unorm destination
after an sRGB-to-unorm copy.

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

Public culling coverage now complements the private rasterizer tests: strict
forced-FL9_1 readback checked opposite triangle windings through separate
CCW and CW back-face-culling pipelines, producing 10 red, 10 green and 44 clear
pixels. Other pipeline and render-state combinations remain under audit.

The selected D3D11 device is also probed by creating a wireframe rasterizer
state. When that creation succeeds, `POLYGON_MODE_LINE` is advertised and
triangle wireframe pipelines map to `D3D11_FILL_WIREFRAME`. The strict public
RTX test compared the full triangle wireframe readback with explicit edge-line
primitives; both images matched, with 17 green pixels. Polygon point mode and
historical-driver behavior remain unsupported or unverified.

| Category | Hardware and private implementation | Coverage; wgpu mapping and remaining work |
| --- | --- | --- |
| Device / adapter | Native DXGI discovery excludes software adapters and creates only an explicitly requested FL9_1 device. The public opt-in adapter exposes the selected-device projection and constrained HWND surface. A CoreWindow path implements and API-validates BGRA8 sRGB FIFO, opaque composition, render-attachment usage, one frame of latency and owner-thread lifecycle, but has not been exercised on Lumia/UWP hardware. | Strict public wgpu tests select the RTX 3090 Ti, render/read back with uniforms in group 0, a sampler in group 4 and textures in group 7, replace only the group-7 texture between draws, verify overlapping near/far draws in both orders, concurrently submit through two public devices sharing the selected native device/context, and verify core rejection of unsupported resource/layout requests. A hidden-window lifecycle test configures, presents, resizes and presents again; a visible-window example presents 60 animated textured, indexed, depth-tested frames. The adapter exposes `TEXTURE_FORMAT_16BIT_NORM`, `TEXTURE_FORMAT_16BIT_PACKED` and comparison-sampler `ADDRESS_MODE_CLAMP_TO_BORDER` only when their implemented paths and selected-device queries permit them. It is not WebGPU-compliant. The exhaustive contract audit remains outstanding; visible client-area output has been qualitatively captured and inspected. |
| Buffers, maps, copies | Native buffers and copies exist. Private HAL implements vertex, index, uniform, copy and explicit CPU-shadow mappings; noncoherent mappings are synchronized before CPU access. Vertex+index is rejected at 9_1/9_2. Index offsets remain aligned to index width, but an in-bounds index range may end with trailing bytes; Core exposes only complete indices to draws and the HAL now preserves that contract. The FL9_1 logical size limit leaves one 4-byte alignment unit below `u32::MAX`, so core's vertex sentinel byte and 4-byte allocation rounding still fit D3D11's `ByteWidth`. Storage usage bits and even storage-only allocation are accepted when otherwise core-valid; FL9_1 native allocation uses a dummy vertex bind for storage-only buffers, while shader storage binding remains unavailable. Indirect usage is unsupported. | Copy, clear, map, initialization and ownership tests. Strict public FL9_1 coverage remaps a `MAP_WRITE | COPY_SRC` range, verifies the prior contents, changes only two bytes and checks complete GPU copy/readback preserves all other bytes. A strict odd-sized Uint32 index-slice test renders three complete indices from a 13-byte range and verifies Core rejects a four-index draw. Core requires `VERTEX_STORAGE`, `FRAGMENT_STORAGE` or `COMPUTE_SHADERS` for storage-buffer layouts visible to those stages; writable fragment buffers also require `FRAGMENT_WRITABLE_STORAGE`. The controlled fixture verifies unsupported stage layouts fail before HAL and a storage/copy buffer still uploads, copies and reads back. A strict public test creates a storage-only buffer but verifies storage layouts remain rejected. Public `Buffer::destroy` rejects outstanding mapped views before core unmaps the D3D11 shadow allocation; strict public readback coverage verifies disjoint views remain valid after the rejected destroy. Strict public `clear_buffer` readback verifies an aligned 16..48 byte range is zeroed in queue order while the surrounding bytes retain their uploaded values; core rejects unaligned and out-of-bounds clear ranges. `UNRESTRICTED_INDEX_BUFFER` and limits describe only part of the buffer contract. |
| 2D textures / formats / views | FL9_1 texture use is projected per format from the static level guarantee, selected-device `CheckFormatSupport` and implemented usage path. RGBA8/BGRA8 support sampled/upload, render-target, copy and readback paths; mip support and optional attachment/copy uses require the relevant query. R8_UNORM, R8G8_SNORM and R8G8B8A8_SNORM support sampled/upload, with mip chains gated by `TEXTURE2D | MIP`; RGBA8_SNORM also permits same-format GPU copy-source use when the selected device reports `TEXTURE2D` and the backend path is enabled, plus raw-byte CPU-shadow readback behind its separate implementation gate. RG8_SNORM copy-source/readback uses an execution-ordered raw-byte CPU shadow and is exposed only when the selected device reports `TEXTURE2D`; it does not set an RTV bind flag or issue native texture-copy calls. R8G8_UNORM and packed 16-bit formats have narrower selected-query paths. RGB565/RGB5A1/RGBA4 mip chains each require that format's selected `TEXTURE2D | MIP` query and a separate implemented sampled/upload path. R16G16_SNORM permits query/backend-gated sampled/upload mip chains (`TEXTURE2D | MIP`), same-format GPU texture copies and separately backend-gated ordered CPU-shadow texture-to-buffer readback; `SHADER_SAMPLE` still independently controls filtering. It has no render-target or native staging readback path. BC1/2/3 unorm and sRGB each have a separate narrow sampled/upload feature and query-gated GPU-copy subset with CPU-shadow readback. D16/D24 formats support their restricted depth attachment and query-gated sampling paths. Other formats/usages remain unavailable. | Strict public tests verify uploads, sampled readback, optional per-format attachments/copies, mip selection and format-specific validation. The packed RGB565/RGB5A1/RGBA4 mip paths are selected per format query: strict public coverage samples RGB565 mip 1 and verifies its raw word and readback sentinels; RGB5A1/RGBA4 verify four-level creation, mip-1 upload, partial GPU copy, raw shadow readback and sampled output at requested/actual FL9_1. RGBA8_SNORM upload→GPU-copy→sample and raw-byte shadow readback after a partial copy are verified at requested/actual FL9_1; these paths do not add render-target support or native GPU-to-staging reads. The RTX 3090 Ti's native RG8_SNORM `CopySubresourceRegion` still fails with `E_INVALIDARG`; the execution-ordered shadow path now covers queued updates, raw readback and sample-visible destination updates without relying on that native copy. BC sRGB sampling verifies actual decoding. Core `TextureFormatFeatures` and downlevel limits must track the same query and backend predicates. Historical-driver format behavior remains unverified. |
| Texture view dimensions | The native resource path now includes RGBA8-family and selected query-backed cube SRVs: square POT, six faces, one sample; cube views are sampled-only, while cube resources may also use the documented optional copy usages. Mip chains are query-gated. The reported six array layers cover the cube backing only; `TEXTURE_2D_ARRAYS` remains absent. Ordinary arrays, cube arrays, render-target cubes and single-face D2 views remain unsupported. | Core gates cube resources/views on `CUBE_TEXTURES`, the selected cube dimension/NPOT limits and per-format cube mask; the no-array path rejects unsupported cube shapes/usages before HAL. Strict public forced-FL9_1 rendering samples all six RGBA8 faces across four mips and a non-suffix view (960/960 pixels). D1 and ordinary array paths remain gated; the separate 3D RGBA8 sampled-upload path is narrowly query-gated below. Historical drivers are unverified. |
| Mips, arrays, NPOT / sampling | POT RGBA8 and BGRA8 2D textures and sampled cube textures can have mip chains when format support is reported. Sampled suffix views are native; other supported mip ranges use a GPU clone. Color-attachment views select one mip. Sampled and color-attachment usages can coexist, with optional copy usages. NPOT 2D mip chains are capability-gated by positive `FullNonPow2TextureSupport` and per-format `MIP`; cube mip chains remain square POT, and ordinary arrays remain unavailable. | Strict FL9_1 tests verify per-mip transfers, attachment rendering, suffix and clone-backed non-suffix sampling on the RTX. A public 6×10 four-level RGBA8 texture uploaded with padded rows and sampled at mip 2 matched 64/64 output pixels. Cube mip chains require square POT dimensions, exactly six faces, one sample, supported sampled usage and a positive per-format `MIP` query. Contents are caller-uploaded; autogeneration is unavailable. Core requires `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`; D3D11 sets it only when its implementation path and the selected 9_x device's full-NPOT query are positive (10+ treats the hardware guarantee as sufficient). Negative or unavailable 9_x results leave the flag absent. Other NPOT formats/usages are gated by format and backend validation and remain less thoroughly tested. |
| Samplers | Independent point/linear min/mag, clamp/wrap/mirror and anisotropy up to 2 are implemented in the private HAL. When `D3D11_FEATURE_D3D9_SHADOW_SUPPORT` is positive, the public projection adds only `LessEqual` depth comparison with equal min/mag filters, anisotropy 1 and single-sample D16/Depth24Plus/D24S8 views. Other comparison functions and sampler combinations remain unavailable. Opaque black and opaque white border addressing is available only to those comparison samplers; ordinary border samplers, clamp-to-zero and other comparison combinations remain unavailable. Translated LOD ranges are checked against each used sampled view. | Patterned FL9_1 GPU readback covers filter and address modes, sampled mip levels and sampler switching. Strict private and public tests verify D16, Depth24Plus and D24S8 comparison. Public tests reject unsupported functions, filter mismatch, anisotropy, ordinary border and clamp-to-zero before HAL; black and white comparison-border samples each checked all 64 output pixels. Synthetic projection tests verify unavailable/negative queries expose no depth sampling; the controlled core fixture still verifies rejection when the flag is absent. Anisotropy 2 creates and samples but visual anisotropic quality is unverified. |
| Bind groups / registers | Up to eight groups may contain uniforms, sampled textures and samplers, with aggregate implementation limits of thirteen uniforms, eight textures and sixteen samplers. The internal projection exposes six uniform buffers per shader stage and twelve dynamic uniform buffers per pipeline layout; the latter is the maximum disjoint vertex/fragment split (six each) within the thirteen-slot allocator. Its combined buffers-and-acceleration-structures limit is 22: six vertex-stage uniforms plus sixteen vertex input buffers. This is a wgpu pipeline-validation limit, not a native D3D11 register namespace limit. | Strict public rendering binds twelve sparse dynamic ranges from one buffer, six vertex-visible and six fragment-visible, changes the selected subranges between two draws, and checks the complete readback. A separate public test samples two independent textures with nearest and linear samplers across groups 1, 4, 6 and 7; patterned data makes incorrect sampler assignment observable. It replaces only group 7 between draws and checks all 128 pixels for the two filter results. Core rejects thirteen dynamic uniform bindings. This does not cover every resource combination or historical driver. FXC limits distinct sampled relationships and constants. Counts in `Limits` cannot express all shader, visibility, compatibility or cross-group restrictions. The public adapter remains experimental and its full capability audit remains incomplete; its HWND and CoreWindow contracts are described in Device/adapter. |
| Vertex / index formats | The FL9_1 required IA subset is `Uint8x4`, `Unorm8x4`, `Sint16x2`, `Snorm16x2`, `Sint16x4`, `Snorm16x4` and `Float32` through `Float32x4`. These are guaranteed minima, intersected with `CheckFormatSupport(IA_VERTEX_BUFFER)` on the selected device and the HAL mapping table. The HAL maps every wgpu vertex format with a direct DXGI input equivalent except `Float64` variants. Formats outside the required FL9_1 subset are optional and enter the selected-device mask only when `CheckFormatSupport(IA_VERTEX_BUFFER)` succeeds; a missing or failed query omits them. `Float16x2` and `Float16x4` become hardware guarantees only from FL9_3. The RTX forced to FL9_1 reports no optional input format. Private pipelines accept up to sixteen sparse vertex- or instance-rate streams, including zero-stride constant streams. Integer IA inputs are limited to direct `i32`/`u32` vector-to-float conversion in the FL9_1 vertex profile; integer arithmetic and other runtime integer semantics remain rejected. Attribute offsets preserve core format-size alignment and are passed to D3D11 as byte offsets; the HAL adds no four-byte alignment restriction. Indexed rebase and gather planners likewise accept two-byte copied extents without imposing four-byte size alignment; their offset, stride, nonzero-size and bounds checks remain. Multi-slot configurations support triangle and point lists, nonindexed line and triangle strips, and Uint16/Uint32 indexed triangle, point and line lists with translation when needed. Indexed LineList and TriangleList with only instance-rate or no vertex-rate inputs validate the index range and replay equivalent nonindexed draws because index values and base vertex cannot affect vertex inputs when the selected shader does not consume `vertex_index`. Indexed LineStrip and TriangleStrip with no vertex-rate inputs read index contents at replay, split restart runs after signed base-vertex resolution, and replay one nonindexed strip per run; ordinary indexed strip draws retain the existing CPU restart-splitting path. These cases are verified on the RTX at forced FL9_1 for Uint16/Uint32, instance-only and empty layouts. On FL9_1, overlapping elements in a logical stream are assigned to distinct native slots bound to the same buffer. | `DownlevelLimits::vertex_format_mask` carries the format-specific support subset into core; unsupported attributes are rejected before HAL. Strict forced-FL9_1 readback exercises the required formats and SNORM conversion. The selected RTX exposes no optional formats, so its Float16x2 public test logs a capability skip; no optional-format GPU execution is claimed. Existing indexed and non-indexed readback, sparse-slot, overlap, zero-stride, SNORM vertex, strip and rejected-index tests remain applicable. Synthetic checks distinguish guaranteed formats, optional query results and backend mappings. Overlapping physical reads, stream offsets, maximum strides, optional-format execution and older-driver behavior remain unverified. SNORM vertex support does not imply SNORM texture support. |
| Shader stages / WGSL | FL9_1 uses `vs_4_0_level_9_1` and `ps_4_0_level_9_1`. These profiles restrict shader instructions, registers and flow control; vertex textures and fragment entries declaring primitive_index are rejected by the private shader path; vertex_index is emulated for TriangleList, PointList, LineList, LineStrip and TriangleStrip pipelines with a host-generated float input because FXC rejects SV_VertexID (X4555); instance_index is supplied through reserved vertex constant register b13 because FXC rejects SV_InstanceID (X4016). Core permits only the tested direct u32-to-f32 use of these builtins; integer arithmetic remains rejected. Core exposes `TriangleList`, `PointList`, `LineList`, `LineStrip` and `TriangleStrip` in `DownlevelLimits::vertex_index_topology_mask`; the HAL keeps a defensive check. Core checks the selected vertex entry point against `DownlevelLimits::max_sampled_textures_per_vertex_stage`, which the D3D11 fixture sets to zero, and `DownlevelLimits::fragment_front_facing` / `fragment_position`, which are false for the tested FL9_1 profile. A narrow fragment-only `textureLoad` subset is emulated for up to eight directly referenced single-mip `texture_2d<f32>` images, matching the FL9_1 pixel shader resource limit. The selected entry point and its reachable straight-line helpers may directly reference those global images; resource-handle parameters or results, other reachable bound resources, sampling/query operations and control flow remain rejected. Images map compactly to `t0`–`t7`, one dimension vector per image is stored at pixel constant buffer `b13`, and the internal point/clamp sampler `s15` samples texel centers. Out-of-bounds coordinates and nonzero mip requests produce zero. The eight-image shader compiles in FXC after removing redundant coordinate clamping; shaders remain subject to the profile 64-arithmetic-instruction limit. Unreachable helpers and helpers exclusive to another entry point do not constrain the selected shader. `DownlevelLimits::texture_load_2d_float_fragment` describes only this exact subset; general `texture_load` and `texture_query` remain false. Strict public GPU readback verifies in-bounds and out-of-bounds behavior. Fragment derivatives and explicit texture gradients are unavailable: selected-entry `fragment_derivatives` is false and core rejects reachable Naga `Derivative` and `ImageSample::Gradient` expressions, including helper calls. General `textureSampleLevel` remains unavailable below FL10_0. A separate `DownlevelLimits::texture_sample_level_zero_2d_float_fragment` permits one direct, unconditional fragment sample of a bound `texture_2d<f32>` with literal positive LOD zero, or that operation returned through one straight-line helper whose `vec2f` coordinate uses only the input, numeric literals, binary/unary arithmetic, composition, splat or swizzle. After a selected-entry Naga proof, HAL lowers only that sample to implicit sampling. Core requires the selected view to expose exactly one mip and the current regular sampler to have equal min/mag filters and anisotropy one before drawing. Dynamic/nonzero/fractional LOD, nested helpers, unsupported helper expressions or control flow, other texture shapes, comparison samplers and incompatible bindings remain rejected; general `texture_sample_level` stays false. FXC reports X4532 for unlowered exact LOD at both tested 9_1 and 9_3 pixel profiles. The pixel profile has 64 arithmetic and 32 texture instructions; the vertex profile has 256 instructions. | Naga/FXC and restricted shader GPU tests, including unused helper operations. The controlled core fixture verifies selected-entry validation before HAL; strict public readback verifies direct, forwarding-helper and bounded arithmetic-helper literal-zero forms, rejects dynamic LOD and incompatible bindings, and compiles an implicit-sampling sibling. The arithmetic helper shifted the sampled coordinate and all 64 readback pixels matched; Core render-bundle validation rejected a multi-mip view before HAL execution. Microsoft [documents the 9.1/9.2 and 9.3 shader-profile limits](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets). |
| Texture gather | FL9_1 has no native Gather instruction. General `DownlevelLimits::texture_gather` remains false. The backend implements only the separate `texture_gather_2d_float_fragment` subset: one direct fragment gather from a single-mip, single-sample `Rgba8Unorm` or query-supported `Rgba8UnormSrgb` 2D float view, regular sampler clamped on U and V, direct location-0 `vec2f` coordinate and direct location-0 `vec4f` result. | Naga emulates the selected channel with four point samples at texel centers in WGSL order. The selected texture uses reserved `t0`, dimensions use `b13`, and the internal point sampler uses `s15`. Core checks the selected entry and actual bound view format/mip count and sampler before render and bundle draws. Strict public FL9_1 readback on the RTX 3090 Ti matched all 64 linear pixels as `[151, 166, 106, 91]` and all 64 sRGB-decoded pixels as `[79, 97, 37, 27]`; Core rejects R8 and multi-mip views before HAL. Helper-reached gather remains rejected. Historical-driver behavior and other shader/resource forms remain unverified or unsupported. |
| Pipeline / raster / blending | Private graphics pipeline supports line lists, line strips, triangle lists, nonindexed triangle strips and point lists with configurable front-face winding and front/back culling for triangles, one color target and one sample by default. `POLYGON_MODE_LINE` is exposed only if selected-device creation of a D3D11 wireframe rasterizer state succeeds; it maps `PolygonMode::Line` to `D3D11_FILL_WIREFRAME`, while point polygon mode remains unavailable. Optional query-gated 2x/4x/8x RGBA8 and BGRA8 color render/resolve and matching D16/Depth24Plus/D24S8 depth attachments in matching passes. Blend translation supports the ordinary wgpu factors, including constant and inverse-constant factors, and all five blend operations; `SrcAlphaSaturated` and dual-source factors are rejected at FL9_1. Core reports the FL9_1 restriction through `DownlevelLimits::src_alpha_saturated_blend` and rejects use before HAL. Blend constants are recorded and rebound with pipeline state. A single-sample mask is passed to the output merger; a zero mask maps to `0xffff_fffe`, an equivalent no-coverage mask for the only active sample and the nonzero native value required at FL9_1. 16x on the tested device and multisampled sampling remain unsupported. Alpha-to-coverage is also unsupported even when 4x RGBA8 resolve is available; the selected FL9_1 projection reports `DownlevelLimits::alpha_to_coverage = false`, so core rejects it before HAL. All RGBA channel write masks are supported for the single target. A descriptor whose color and alpha equations differ is rejected when both RGB and alpha are written unless separate-alpha support is explicitly established; the production FL9_1 model does not establish it. The core `SEPARATE_ALPHA_BLEND` flag rejects these descriptors before HAL. Enabled blending requires a positive format query and must match wgpu's linear-space shader semantics. FL9_1–FL9_3 sRGB render-target blending is therefore not exposed even when the native format query reports `BLENDABLE`; the selected-level guarantee specifies sRGB-space blending. Out-of-target viewports use a separately compiled vertex position transform when available. A D24S8 attachment supports native Less, Equal, LessEqual, Greater, NotEqual, GreaterEqual and Always depth comparisons; Never retains its emulation; optional depth writes and one-sided stencil comparison/operations are implemented with identical front/back state. FL9_1 rejects differing front/back stencil face states; core now represents this restriction through `DownlevelFlags::TWO_SIDED_STENCIL` and rejects a differing descriptor before HAL. Never with active stencil maps the stencil pass operation to the requested depth-fail operation, forces Always depth comparison, and suppresses color/depth writes. Signed constant and finite slope-scaled depth bias are supported for triangles with selected-entry restrictions. Nonzero clamp is rejected because FL9_1 requires zero; nonzero bias on points and solid lines is rejected because native raster bias does not affect those primitives. | CPU tests cover factor translation and rejection of `SrcAlphaSaturated`/dual-source factors, and verify the sRGB-space blend guarantee across all recognized feature levels. A strict public FL9_1 test rejects sRGB blending before HAL while the selected-device projection is checked on the RTX 3090 Ti. A strict forced-FL9_1 HAL render checked all eight depth comparisons over 128 pixels (64 green, 64 blue), and a blend render checked constant-factor updates and all five operations over 64 pixels; the controlled core fixture checked a custom constant-factor render over 64 pixels. The private strict depth-bias render checks five constant-bias images and two sloped-triangle images (64 pixels each), including positive/negative slope signs, point/line rejection, nonzero clamp rejection and selected-entry `frag_depth` rejection. A strict public BGRA8 4x clear-and-resolve test validates 64 readback pixels after the queried format is enabled. Separate-alpha rendering is exercised only through an opt-in RTX diagnostic and is not a portability guarantee. The existing strict suite checks premultiplied blending and opaque transitions on RGBA8/BGRA8. Remapped shaders may exceed FL9 compiler limits even when the original compiles; such draws remain rejected. |
| Render passes / targets | One color target and an optional D16 or Depth24Plus depth-only attachment or D24S8 depth/stencil attachment support load, clear or don't-care and store or discard. Discard retains native contents, which remain undefined to HAL callers. Restricted transient color and depth attachments use ordinary textures without memoryless savings; transient combinations with sampled or copy usage are rejected. Private read-only passes require LOAD and STORE on each present aspect and pipelines without depth or stencil writes; depth sampling aliases remain unsupported. Depth-only clears and draws without a fragment stage are supported. Query-gated 2x/4x/8x RGBA8 and BGRA8 render targets can resolve to compatible single-sample targets, and query-gated D16/Depth24Plus/D24S8 depth attachments can join matching passes; depth is not resolved. Alpha-to-coverage, multisampled sampling, 16x on the tested device and other formats remain unavailable. Occlusion queries, multiview and multiple color attachments are unsupported. Timestamp descriptor writes are covered separately in the query capability row. | Public forced-FL9_1 tests cover D16 depth ordering and a `DepthOnly` view; D16 `StencilOnly` views and stencil pipeline state are rejected because D16 has no stencil aspect. D24S8 supports the tested symmetric stencil state and depth/stencil LOAD, CLEAR, and DISCARD-then-LOAD transitions; asymmetric front/back state and `StencilOnly` D24S8 views remain rejected. A second pass with LOAD preserves depth, and CLEAR resets it. A discarded D16 depth value followed by LOAD is initialized before use and does not influence the next draw. A public `LoadOp::DontCare` test overwrites all color and depth pixels before reading back all 64 red pixels. A new color texture first used with `LoadOp::Load` is initialized to transparent black before it is stored and read. Public color `StoreOp::Discard` followed by `LoadOp::Load`, copy and SRV sampling reinitializes the texture before the later read; all three paths produce transparent black at all 64 pixels rather than expose the discarded red clear. D24S8 tests cover a depth-only prepass, read-only depth preservation, and separate depth/stencil LOAD, CLEAR, and DISCARD-then-LOAD transitions. A public transient test executes color clear/draw/discard and uses transient D24S8 clear/draw/discard to control a stored color result; core rejects transient sampling, invalid LOAD/STORE, read-only transient depth and passes with no attachments. Query-gated D24S8 sampled usage uses the depth-only sampled view. A DepthOnly view of combined D24S8 can be created but core marks it non-renderable; query-backed Depth24Plus resources and D24S8 combined views are renderable. StencilOnly D24S8 views remain unavailable because Stencil8 is not exposed. Other combinations outside this stated contract remain unavailable unless specifically validated and projected. |

When a selected FL9_1 device positively exposes depth sampling, a depth texture
created with both render-attachment and texture-binding usage may use a default
view that inherits both usages. The HAL creates both native views for that
logical view. A strict public test used one default D16 and Depth24Plus view
sequentially as a depth attachment and sampled comparison view; D24S8's
default all-aspects view was tested as an attachment and its sampled view
remained explicitly `DepthOnly`, as required by core. This does not permit
simultaneous depth attachment and sampling aliasing.

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
The public sampled-mip tests uploaded and rendered four-level RGBA8 and R8_UNORM
chains. RGBA8 implicit derivatives and R8_UNORM mip bias selected the expected
levels across all checked pixels. R8 mip support is exposed when the selected
device positively reports `TEXTURE2D | MIP` and the backend path is enabled;
NPOT R8 chains additionally require positive `FullNonPow2TextureSupport`.
Color-target and copy-source combinations also require the selected
`RENDER_TARGET` query and their implemented paths. `SHADER_SAMPLE` separately
controls filterability. Other sampled formats remain single-mip unless
separately documented.
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
retain the clear color. A later strict test verifies `instance_index` through
the host-updated FL9_1 constant path; `vertex_index` support uses host emulation for TriangleList, PointList, LineList, LineStrip and TriangleStrip, as described in the current section below.

A targetless pipeline may include a fragment entry point with no result when
it has a depth/stencil attachment. FXC's `ps_4_0_level_9_1` compiler rejects
Naga's native `void` pixel shader with X4530, so HAL wraps only result-less
fragment entry points in a helper and adds an `SV_Target0` value. The pass
binds no render-target view, making that synthetic color unobservable while
preserving fragment discard. Strict public D16 and D24S8 tests verify that a
discard suppresses later depth and stencil updates. A fragment stage with no
color target and no depth/stencil attachment remains unsupported. Fragment
results containing color, depth or sample-mask outputs are not rewritten; their
usual target and downlevel validation still applies. Core may ignore a color
output when its location has no target. A strict targetless D16 test verifies
that an unmatched `@location(0)` result does not prevent depth writes and does
not reach the later color pass.

POT RGBA8/BGRA8 mip chains accept every nonempty combination of `RESOURCE`,
`COLOR_TARGET`, `COPY_SRC` and `COPY_DST`. The selected format's MIP, shader-sample
and (when the usage includes `COLOR_TARGET` or `COPY_SRC`) render-target query
bits must all be present. Each implementation family remains separately gated:
copy-only, sampled, attachment-only, and sampled attachment chains. Core format
capabilities cannot express usage-dependent mip ceilings, so each reported
usage combination must pass the same private HAL path.

For FL9_1, general `textureLoad` remains unsupported. The opt-in public
projection separately permits one fragment-only subset through
`DownlevelLimits::texture_load_2d_float_fragment`: a selected entry point may
load one single-mip, non-arrayed `texture_2d<f32>`. Constant-coordinate loads
may occur through validated straight-line helpers that directly reference the
same image. For a dynamic coordinate, one `@location(0) vec2f` argument may
be converted directly to `vec2i` in the entry point, or passed unchanged to
one straight-line helper that directly returns the sole load. That helper may
apply literal-based f32 addition, subtraction, multiplication, division,
negation, vector composition/splat, component access or swizzle before the
`vec2i` conversion. Arithmetic in the entry-point call argument, nested or
computed calls, and other runtime integer operations remain rejected.
Core rejects control flow, additional bound globals, other image operations,
additional images and non-fragment stages before HAL. The private path maps the
resource to `t0` and emulates point sampling at texel centers with an internal
sampler at `s15` and dimensions at `b13`. The shader returns zero for
out-of-bounds coordinates and nonzero mip requests. The 2D path also accepts
query-gated `Rgba8UnormSrgb`, `Bgra8Unorm`, `Bgra8UnormSrgb`, `R8Unorm`, `Rg8Unorm`, `Rg8Snorm`,
`Rgba8Snorm` and `Rg16Snorm` views when
the selected device reports the format's sampled-resource support and the
sampled-upload backend path is enabled. `Rg16Snorm` additionally requires the
public 16-bit normalized format feature and an unfilterable-float layout. For
sRGB, the typed SRV performs the required RGB-to-linear conversion on shader
reads while alpha remains linear; this is limited to resources and views both
natively formatted as `Rgba8UnormSrgb` or `Bgra8UnormSrgb`. A BGRA typed view
returns channels in WGSL's logical RGBA order.
The internal point sampler does not require filtering behavior from the load
itself. Query-gated R8/RG8 and 8-bit SNORM sampled-upload paths require
`TEXTURE2D` but not `SHADER_SAMPLE`; sRGB remains behind the ordinary
sampled-resource projection and its selected format query. WGSL channel
semantics are preserved: R8 supplies normalized red, zero green/blue and alpha
one; RG8, Rg8Snorm and Rg16Snorm supply normalized red/green, zero blue and
alpha one; Rgba8Snorm supplies all four normalized channels. Core checks the
currently bound view and parent resource for the selected entry point before
drawing: it must be a single-mip, single-sample
2D view of a single-mip resource, with `Rgba8Unorm`, query-gated
`Rgba8UnormSrgb`, `Bgra8Unorm`, query-gated `Bgra8UnormSrgb`, `R8Unorm`,
`Rg8Unorm`, `Rg8Snorm`, `Rgba8Snorm` or
`Rg16Snorm` format. sRGB sampled
usage remains gated by the selected format query; the typed sRGB SRV preserves
the WGSL read conversion for RGB and linear alpha. Strict readback verifies
BGRA channel order for unorm and the linearized sRGB values. BGRA sRGB
resource/view formats must match. Rg16Snorm additionally
requires its query-gated sampled/upload path and `TEXTURE_FORMAT_16BIT_NORM`.
The separate sample-bias-offset emulation still requires linear RGBA8.
Unsupported formats
and mip ranges produce a validation error instead of a late HAL error; sibling
entry points without `textureLoad` do not inherit this restriction. Strict
public readback on the RTX 3090 Ti at requested/actual FL9_1 validated all 16
patterned texels for R8 and RG8, including four-channel conversion, plus 48
out-of-bounds zero results for each. The sRGB case read `[128, 128, 128, 128]`
through a typed sRGB view into a linear RGBA8 target and matched `[55, 55, 55,
128]` within one byte across 64 pixels. The strict 8-bit SNORM case also
verifies `Rg8Snorm` and `Rgba8Snorm` from four signed texels each plus
out-of-bounds zero, checking 256 output pixels. These formats remain
selected-query and sampled-upload gated; this narrow load path does not enable
their use by sample-bias emulation or establish compatibility with historical
9_1 hardware.

A separate `texture_load_3d_float_fragment` limit covers one fragment
`textureLoad` from a single-mip, non-arrayed `texture_3d<f32>`. The selected
entry point and its reachable helpers may perform the load when every helper
is straight-line, directly references that same image global, and passes no
resource handles. Control flow, selects, other image operations, additional
resources and unsupported shader forms remain rejected before HAL. The limit
is enabled only at FL9_1 when the selected device positively reports RGBA8
`TEXTURE3D | SHADER_SAMPLE` support and the 3D sampled-upload path is enabled.
The backend maps the volume to `t0`, uses its internal point sampler at `s15`,
and supplies `(width, height, depth, 0)` through `b13`; it samples at texel
centers and returns zero for out-of-bounds coordinates or nonzero mip levels.
Strict public FL9_1 readback on the RTX 3090 Ti verifies direct and nested
helper loads, two distinct volume texels, both zero cases, and Core rejection
of a branching helper. Core also checks the selected 3D textureLoad entry
point's current view and rejects a multi-mip view before HAL execution. The
2D and 3D narrow limits are independent, while the general `texture_load`
limit remains false. This selected modern-driver result does not establish
historical FL9_1 behavior.

Per-format `SINGLE_MIP_ONLY` metadata reports the current one-mip ceiling for
R8G8_UNORM, BC1_UNORM, BC2_UNORM, BC3_UNORM, D16_UNORM and
D24S8. RGBA8 and BGRA8
also receive this flag when the selected device's successful format query
lacks `D3D11_FORMAT_SUPPORT_MIP`, or when any currently required backend mip
path is disabled. Core checks this metadata before HAL creation and rejects
larger mip counts. This is separate from device-wide NPOT-mipmap and
depth-attachment flags: the format metadata expresses a per-format
implementation ceiling, while those flags express broader downlevel behavior.
R8_UNORM, R8G8_SNORM, R16G16_SNORM and R8G8B8A8_SNORM have narrow sampled/upload mip-chain paths when the
selected device positively reports `TEXTURE2D | MIP` and the matching backend
path is enabled; R8_UNORM additionally requires its sampled-texture guarantee.
Their format projections omit `SINGLE_MIP_ONLY` only under those predicates.
NPOT chains still require positive unrestricted-NPOT support. These narrow
mip paths do not imply general render-target or readback support; format copy
paths are independently gated by their selected-device query and backend
implementation.
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
Nonindexed point lists are split at the FL9_1 limit of 65,535 points with the
vertex-buffer offset rebased for each batch. Triangle-list batches preserve
whole triangles and split at the 65,535-vertex input window. A strict public
GPU test submits 65,536 points and 65,538 triangle vertices; the visible point
and triangle are only in the final batch, and readback confirms both. The
triangle readback contained 32 red and 32 blue pixels. Historical driver
behavior at large draw boundaries remains unverified.
| Queries / synchronization / readback | The 10Level9 feature table marks occlusion queries unavailable at FL9_1 and available from FL9_2; they remain rejected. Native timestamp/disjoint query execution is retained for HAL diagnostics, but the public projection does not advertise timestamp features. WebGPU permits timestamp pairs to cross queue submissions, while D3D11 only guarantees their reliability and frequency within one disjoint interval; this HAL cannot report interval-specific frequency or invalid results. HAL implements `PassTimestampWrites` boundary markers for render passes and empty compute passes; this descriptor requires `TIMESTAMP_QUERY`. The native HAL can record these ordered `End` commands for diagnostics, but Core rejects public timestamp use because the timestamp features are absent. Compute pipeline creation and dispatch remain unavailable. Pipeline-statistics queries remain unavailable. Query resolve flushes and waits for D3D11 `GetData`, then writes results to a checked `QUERY_RESOLVE` buffer; this preserves order but can block the CPU. The public queue timestamp period is zero; a startup frequency sample is not treated as a lifetime guarantee. Queue submission serializes the immediate context, and staging readback waits for completion. If replay fails after issuing earlier commands, HAL ends and flushes an event query and waits for prior GPU work before returning the error, keeping borrowed command buffers and resources alive until that partial submission drains. Shared HRESULT classification recognizes device hung, removed, reset and driver-internal-error results as device loss. | A strict private HAL diagnostic brackets two timestamp writes in one disjoint interval and verifies a positive frequency and monotonic values. A strict public test verifies the feature request and query-set creation are rejected and the queue period is zero; see `TESTING.md`. Public tests reject occlusion and pipeline-statistics sets. Fence, sequencing, copy, partial-replay-failure and GPU readback tests; CPU tests cover HRESULT classification. A controlled core fixture injects `DXGI_ERROR_DEVICE_REMOVED` during replay on a forced-FL9_1 device and verifies `DeviceError::Lost`, core invalidation and exactly one lost callback; this does not physically remove the selected GPU. Core tests inject removal from fence wait, GetData, `GetDeviceRemovedReason` and queue-drop `wait_for_idle` paths; callbacks can re-enter polling, and queue destruction completes without panicking. Physical device removal/recovery, remaining lifetime/error paths and debug-layer diagnostics remain unverified. A synthetic GetData failure test confirms the pending query remains retained until a later real completion. |

Microsoft's [Direct3D feature-level table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro)
lists occlusion queries as unavailable at FL9_1 and available at FL9_2 and
above. This settles that specific query gap as a hardware boundary for this
mission. A positive startup timestamp/disjoint probe is diagnostic only: it does not guarantee reliable timestamp pairs across later submissions. The public timestamp features remain disabled until the backend can preserve disjoint validity and interval-specific frequency. Pipeline-statistics support remains unavailable.
| Compute / storage | No native compute shader or UAV support at FL9_1; private HAL accepts paired empty compute passes but rejects compute pipeline creation, dispatch and shader storage bindings. Core may retain storage bits for transfer-capable buffers but does not bind them as storage. Other APIs may require CPU or graphics emulation. | Strict hardware test verifies ordered copies around an empty pass and rejects dispatch and invalid pass sequences. The controlled core fixture checks stage-specific storage-buffer layout rejection before HAL. Public strict tests verify core rejects compute-pipeline creation and `INDIRECT` buffer creation before reaching the corresponding unsupported HAL paths. No compute/storage feature is exposed; feature/limit gating alone remains insufficient for public device exposure. |
| Presentation | FL9 hardware can support display through platform/format-specific paths. The opt-in public backend exposes constrained HWND and CoreWindow surfaces: BGRA8 sRGB FIFO, opaque composition, render-attachment usage and one frame of latency. HWND uses a native DXGI frame-latency waitable swap chain set to one outstanding frame; acquisition waits on its owned handle. CoreWindow construction requires dispatcher-thread access; configure, acquire and present use the captured owner thread. | Queue and selected context-backed operations serialize against Present; queue idle and unconfigure drain a pending present. Strict RTX tests cover hidden-HWND configure/present/resize, waitable acquisition, pending-present completion and destruction, wrong-thread acquisition rejection and a visible 60-frame textured indexed scene. A client-area capture was visually inspected and showed distinct red and green faces over the clear color. A fixture-only public test injects `DXGI_ERROR_DEVICE_REMOVED` at Present and verifies exactly one public lost callback plus prompt reentrant-poll rejection; this is synthetic and does not remove the GPU. Deterministic presentation pixels remain unavailable, while equivalent color/depth behavior is covered by offscreen readback. CoreWindow public capability exposure is not runtime-tested; UWP/Lumia execution and historical driver behavior remain unverified. This is not general surface or WebGPU support. |

The controlled core fixture now also injects a removed-device result from
`GetDeviceRemovedReason` during `device_poll`. Its lost callback reenters
`device_poll`, and later queue submissions are rejected. The RTX is opened at
FL9_1, but the HRESULT is synthetic; actual device removal and recovery remain
unverified.

## Direct FL9_1 vertex-index float-vector reads

`DownlevelLimits::vertex_index_vector_indexing` is false by default and is
enabled by the selected FL9_1 D3D11 projection after its backend proof. Noop
reports it as well because it does not compile or execute native shaders. Core
and HAL independently prove the selected vertex entry point contains only
direct, read-only `@builtin(vertex_index)` accesses to `vec2f`, `vec3f` or
`vec4f` values in a straight-line body, with no more than sixteen accesses.
Naga lowers the indices to static HLSL swizzle comparisons; it does not emit
dynamic vector indexing that the Level 9 FXC profile rejects. Restrict
behavior maps invalid
indices to an in-bounds component, using the final component as the fallback.
Helpers, computed indices, stores, pointer escapes, control flow and other
dynamic indexing remain rejected. The public strict FL9_1 test rendered the
three-vertex vector-indexed triangle and verified all 64 pixels on an RTX 3090
Ti. An FXC probe compiled vec2/vec3/vec4 lowering as `vs_4_0_level_9_1` and
asserted the final-component fallbacks. This verifies the selected compiler
and driver path, not historical FL9_1 hardware.

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
function. Never is emulated with an Always depth comparison and disabled color
and depth writes when stencil is inert; the draw still executes so timestamp
queries retain command ordering. When active stencil must observe the
depth-fail branch, a separate Always-based state remapping is used below.
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

Depth comparison Never uses an Always depth comparison with color and depth
writes disabled when stencil is inert; draws are still issued, including in
timestamped passes. If active stencil must observe the depth-fail branch, the
backend remaps StencilPassOp to the requested StencilDepthFailOp and disables
color and depth writes. The 10Level9 [`CreateSamplerState` restriction](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
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
behavior remains unverified. Never uses the restricted no-write and
stencil-remapping emulation; timestamped draws retain their command ordering.

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
10Level9 compiler path; it is not a general FL9_1 hardware claim. A strict
public forced-FL9_1 draw/readback verifies the supported structured
color-plus-depth path: fragment-written depth changed which overlapping draw
passed depth testing across all 64 output pixels.

The FL9_1 projection disables both
`DownlevelLimits::fragment_sample_mask_input` and
`DownlevelLimits::fragment_sample_mask_output`. Naga lowers either WGSL
`@builtin(sample_mask)` direction to HLSL `SV_Coverage`. Microsoft documents
pixel-shader input as requiring `ps_5_0`, and output as requiring `ps_4_1` or
higher; the backend compiles `ps_4_0_level_9_1`. Core checks the selected
fragment entry point for input and output separately, and HAL defensively
rejects the input before compilation. Physical-device-gated FXC tests verify that the
input and output fail while a color-only sibling compiles. These are profile
restrictions, not claims that sample masking is intrinsically impossible on
all FL9_1 devices. The private renderer is single-sample, so a future
semantics-preserving implementation could investigate lowering the output's
low bit to discard, but no such rewrite is currently performed. A direct FXC
probe shows why a generic discard rewrite is not currently safe: the
level-9_1 profile rejects the bitwise `mask & 1` operation (X3535), which is
needed to preserve the single-sample low-bit semantics for every valid `u32`
mask. Treating every nonzero mask as covered would be observably wrong when
only higher bits are set, so core continues to reject this output before HAL.

The FL9_1 projection also disables
`DownlevelLimits::fragment_non_perspective_interpolation`. Naga emits
`@interpolate(flat)` as HLSL `nointerpolation` and `@interpolate(linear)` as
`noperspective`; FXC rejects both modifiers on `ps_4_0_level_9_1` with
X4568. The vertex profile accepts them on outputs, so core checks only the
selected fragment entry point's inputs. Perspective and centroid interpolation
compile and execute. `@interpolate(sample)` remains rejected earlier by Naga's
existing `MULTISAMPLED_SHADING` capability mapping. This is the tested
FL9_1 compiler/runtime path, not a separate hardware guarantee.

| Transfer | Implementation and restrictions | Coverage |
| --- | --- | --- |
| Buffer to texture | Retained GPU buffer copied to a CPU-readable staging buffer at replay; mapped bytes feed `UpdateSubresource` with an explicit destination box and checked row pitch. This reads GPU contents at execution time, not a CPU shadow. RGBA8/BGRA8 linear and sRGB formats share this transfer implementation. R8_UNORM supports query-gated mip chains when `TEXTURE2D | MIP` is reported; copy-source and render-target combinations additionally require the positive `RENDER_TARGET` query and matching backend paths; R8G8_SNORM also permits sampled/upload mip chains only after the selected query confirms `TEXTURE2D | MIP`. R8G8_UNORM sampled/upload mip chains additionally require selected-device `TEXTURE2D | MIP` and the backend mip path; otherwise it remains one-mip and one-layer; R8G8B8A8_SNORM also permits sampled/upload mip chains when the selected query reports `TEXTURE2D | MIP` and the backend path is enabled; R8_UNORM and R8G8_UNORM may combine upload and color-target use only when the selected query reports `RENDER_TARGET`. R8G8_UNORM is optional and requires a positive `TEXTURE2D` query. R16G16_SNORM has the same restricted path only if the selected device query reports `TEXTURE2D`; `SHADER_SAMPLE` controls filterability. BC1 uses 4×4 texel blocks with 8-byte blocks; BC2 and BC3 use 16-byte blocks. All require 4-aligned dimensions, row pitches/offsets aligned to block size, and complete block-aligned update regions. | RGBA8/BGRA8 offset and padded full/partial uploads, GPU-written source and ordered submissions. A strict public test uploaded both sRGB formats and verified their sampled decoding. The guaranteed sampled-upload-only uncompressed formats passed padded-row, nonzero-source-offset upload and full rendered readback tests on forced FL9_1. The optional R8G8_UNORM path passed strict public upload and sampled readback on the RTX; R16G16_SNORM point sampling and filtering rejection both passed at forced FL9_1. BC1/BC2/BC3 passed offset padded uploads, partial block replacement, GPU copy and 256-pixel rendered readback. |
| Texture to texture | Native GPU `CopySubresourceRegion` between RGBA8 or BGRA8 linear/sRGB textures, including compatible linear↔sRGB pairs, different mips of one texture, and checked source boxes with DEFAULT resources. The format pairs must belong to the same DXGI type group; copies preserve stored bits without color conversion. R8_UNORM subresource copies require `RENDER_TARGET`; a strict public test copies a partial region at mip 1 between two mipmapped R8 resources and verifies the preserved border plus all 16 texels; the same test copies source mip 1 to destination mip 2 and verifies all four texels; strict forced-FL9_1 coverage now verifies all 16 source/destination mip-index pairs for four-level R8 textures, including offset regions, final mip contents, untouched texels and padded readback bytes. Single-mip R8G8_UNORM and B5G6R5 copies also require the selected `RENDER_TARGET` query. BC1/BC2/BC3_UNORM are query-gated, block-aligned GPU copy sources; they do not require an RTV. RGBA8_SNORM same-format copies are enabled only by a positive selected-device `TEXTURE2D` query and backend path; CPU readback uses a separately gated raw-byte shadow, not native staging. RG8_SNORM copies use the execution-ordered shadow to read a source region and `UpdateSubresource` to write the native destination; this path requires positive `TEXTURE2D`, never binds an RTV, and does not call `CopySubresourceRegion` for that format. R16G16_SNORM supports query/backend-gated same-format GPU copies without an RTV and a separately backend-gated texture-to-buffer path backed by an execution-ordered CPU shadow; the shadow duplicates raw texture storage and is not native staging readback. Other cross-family formats and copies within the same subresource are rejected. | Full and partial copies, different overall dimensions, cross-resource and same-resource mip copies, zero-preserving destination and render-to-copy-to-sample. RGBA8_SNORM's strict test validates all 64 output pixels. RG8_SNORM strict public coverage checks exact raw signed bytes, padded rows, untouched destination regions and sampled output after the emulated copy. RGBA8 and BGRA8 linear↔sRGB cases verify every intermediate and round-trip byte; a separate query-gated test verifies partial copies for R8, RG8 and RGB565. Strict private HAL tests copy BC1/BC2/BC3 between resources, sample each destination and validate all 256 output pixels. R16G16_SNORM strict public coverage verifies copied and untouched texels through point-sampled RGBA8 output. |
| Texture to buffer | Most formats copy the selected region to a retained CPU-readable staging texture and map it; each mapped row is written into the destination GPU buffer at the requested pitch and offset using the format's block width. BC1/BC2/BC3, RG8_SNORM and RGBA8_SNORM use execution-ordered CPU shadows because FL9_x does not permit direct GPU-to-CPU copies from their shader-resource-only native textures. Shadows begin zeroed and track direct writes, mapped buffer-to-texture uploads, and texture-to-texture region copies at their queue replay positions. Copy-to-buffer emits raw stored bytes (including signed-normalized encodings) into the requested offset and row pitch; it does not claim a native staging-texture copy. The per-format `DownlevelLimits::texture_to_buffer_copy_format_mask` gate includes RG8_SNORM only when the selected `TEXTURE2D` query and shadow path are available, includes R16G16_SNORM when its query-gated COPY_SRC projection and separate shadow implementation are available, includes RGBA8_SNORM when its COPY_SRC path and separate readback shadow are available, and includes BC formats when their query-gated copy path and shadow implementation are available. This duplicates source storage on the CPU and performs a CPU copy for each emulated copy/readback. R8_UNORM readback, including a mip subresource of a mipmapped texture, requires the selected `RENDER_TARGET` query, as required by the FL9_x internal render-target binding restriction. R8G8_UNORM and B5G6R5 readback remains single-mip and query-gated. Cube readback selects each requested face subresource and requires a positive `RENDER_TARGET` format query for non-BC formats; the native cube receives that bind flag without creating an RTV. The projected layer limit is six, while ordinary arrays remain unavailable. Each SNORM readback format remains separately query- and backend-gated; formats without a synchronized raw-byte shadow remain excluded from the copy-to-buffer mask. | Full and partial RGBA8/BGRA8 linear/sRGB source regions, nonzero buffer offsets, row padding and complete image comparison. Strict sRGB render-target readback matched encoded bytes for both channel orders; query-gated R8/RG8/RGB565 readback checked all texels and padding for partial copies and directly from rendered color attachments. Strict RG8_SNORM coverage checks exact source and destination raw bytes with padded rows after ordered updates and copies. Strict compressed tests compare raw BC1/BC2/BC3 block bytes before and after an ordered partial buffer upload and texture-to-texture copy, including nonzero offsets and padded rows; the existing rendering test independently samples the native GPU copy destination. |

RGB5A1 and RGBA4 texture-to-buffer copies use the same ordered raw CPU-shadow
path. Their copy-to-buffer mask bit is enabled only when the selected query,
GPU `COPY_SRC` path and packed-shadow implementation are all available. Strict
public coverage checks both original words and partially copied destination
words, preserved zero-initialized borders, a nonzero destination offset,
padded rows and sentinels. This is not native staging readback and duplicates
each texture's packed storage in CPU memory.

The strict FL9_1 RTX test checked partial same-texture copies between distinct
mips in both directions for RGBA8 and BGRA8, including all affected and
untouched texels and padded readback bytes. Two more GPU cases rendered into a
mip, copied to another mip of that texture and sampled its clone-backed view;
each checked 64 blue pixels. Controlled core tests checked all mip texels for
both formats and rejected a same-mip copy. Before the change, the strict
baseline rejected a valid distinct-mip copy. Historical FL9 hardware remains
unverified; the public adapter also has constrained HWND and CoreWindow
surface paths described in the device/adapter capability row above.

HAL texture creation eagerly uploads zeroes before returning, including for
partial-write targets and each mip of a supported chain. Allocation and uploads
are proportional to texture size; staging maps in buffer/texture transfers may
stall the CPU. Native diagnostic texture constructors remain separate and can
create uninitialized resources. The private color HAL accepts one-layer,
one-sample RGBA8 or BGRA8 2D textures and enforces feature-level size limits.
R8_UNORM and R16G16_SNORM support single-layer 2D textures with RESOURCE
and/or COPY_DST usage; their actual `TEXTURE2D` bits are checked at creation.
R8_UNORM, R8G8_SNORM, R16G16_SNORM and R8G8B8A8_SNORM also support sampled/
upload mip chains when their selected device reports `TEXTURE2D | MIP` and
the corresponding backend path is enabled; `SHADER_SAMPLE` independently
controls filterability. R16G16_SNORM remains non-filterable on the RTX forced
to FL9_1. This does not generally enable render-target, copy-source or
readback use; those paths remain separately gated. R8_UNORM, R8G8_UNORM and
B5G6R5 also support RESOURCE and/or COPY_DST combined with COLOR_TARGET, plus
a target-only use, when the selected format query contains `RENDER_TARGET`.
Those three formats also support `COPY_SRC` only after that same positive
render-target query, because FL9_x requires an internal render-target bind for
GPU-to-CPU copies. The selected-device `BLENDABLE` query independently controls
their format capability projection. Microsoft's required-format table
lists R8_UNORM as a FL9_1 texture format without guaranteeing render-target
support; the implementation therefore requires an individual query. The
`D3D11_FORMAT_SUPPORT_RENDER_TARGET` bit (`0x4000`) and `BLENDABLE` bit
(`0x8000`) are distinct. The public blendable flag is projected only when
both the selected-device query and the backend's render-attachment path support
the format; `BLENDABLE` alone never implies render-target support. See [required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats)
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
R8G8_UNORM sampled/upload mip path additionally requires `TEXTURE2D | MIP` and
the backend mip path. The RTX mask lacks MIP, so the selected device remains
single-mip for this format; a positive mip path is synthetic-only so far.
The first sampled pixels were `[66, 0, 0, 255]` and `[66, 129, 0, 255]`; all
64 pixels per format matched within one code value. B5G6R5 also requires the native-only
`TEXTURE_FORMAT_16BIT_PACKED` feature and a positive per-device query. On the
RTX at forced FL9_1 it returned `0x0026f3a0`, including both `RENDER_TARGET`
and `BLENDABLE`. The public path now supports blending only when the selected
query reports that bit. Strict readback verifies a half-alpha red overlay for
all three narrow formats using identical RGB and alpha equations. A public draw
into B5G6R5 followed by sampling matched 64 pixels within one UNORM code of
`[66, 130, 132, 255]`.
The strict public transfer test additionally uploads each of the three queried
formats, copies a partial rectangle between different texture dimensions and
reads the full destination into a nonzero-offset, 256-byte-pitched buffer. All
24 texels and untouched row padding are checked for R8, RG8 and RGB565. The
public operation-matrix test also executes the complete
`COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT` union for R8, RG8,
RGB565, linear/sRGB RGBA8 and linear/sRGB BGRA8: padded upload, same-format GPU
copy, padded copy readback, sampled rendering to a same-format attachment and
attachment readback. All 64 copied texels match exactly, sampled outputs match
within one byte and sentinel-filled row padding remains untouched on the RTX
3090 Ti forced to FL9_1. This is query-gated
execution evidence for those seven formats and does not widen the existing
projection. A separate strict case uses each narrow format with
`RESOURCE | COLOR_TARGET | COPY_SRC`,
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
with the constrained HWND and CoreWindow contracts described in the
Presentation row.

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
Historical hardware remains unverified. The opt-in adapter uses the
conservative FL9_1 device projection; its public surface paths are limited to
the constrained HWND and CoreWindow contracts summarized above.

HAL transfer commands use explicit regions. No staging-to-default native copy with
a null source box is used, avoiding the documented 9_x corruption case for
mismatched dimensions or destination offsets. Core validates empty regions and
returns before calling HAL; direct private HAL empty-region transfers remain
rejected. Broad texture formats, general multisampling (except query-gated 2x/4x/8x RGBA8 render/resolve and matching D16/Depth24Plus/D24S8 depth attachments), and ordinary arrays
remain unsupported. 2D NPOT mip chains are enabled only by a positive full-NPOT
query and positive per-format mip support. A modern GPU forced to 9_1 does not
validate every historical runtime or driver; the experimental adapter's HWND
surface contract remains limited to the operations described above.

## FL9_1 sampler contract

| Category | Native and driver contract | Private backend and coverage |
| --- | --- | --- |
| Address modes | Clamp, wrap and mirror are available; `MIRROR_ONCE` is not the same as mirrored repeat and is unsupported. Ordinary border addressing is not generally available at 9_1/9_2; D3D9 shadow support is a query-gated comparison-sampler exception. | Clamp, repeat and mirrored repeat map independently on U/V/W. Patterned GPU tests cover repeat, mirror and mixed U/V. When the shadow query succeeds, comparison samplers may use border addressing with representable 0/1 border components. Ordinary border and clamp-to-zero samplers remain rejected. |
| Repeat range | FL9_1 limits texture repeats to 128 over one primitive (`Draw`/`DrawIndexed`). No runtime query raises this feature-level guarantee. | Private HAL repeat sampling is exercised within controlled shaders. Core now rejects Repeat and MirrorRepeat sampler descriptors whenever the reported limit is finite, because it cannot prove interpolated coordinates stay within the native range for every draw. The modern RTX forced to FL9_1 does not validate historical behavior beyond the documented limit. |
| Min/mag filtering | Point and linear filters can be specified independently. | All four min/mag combinations map to D3D11 filters. GPU readback exercises both magnification and minification with contrasting texels; filter outputs differ. |
| Mipmap filtering | Point and linear mip filters exist. | Both are tested with sampled full-chain POT textures, including suffix and restricted clone-backed non-suffix views; query-gated 2D NPOT mip uploads and sampling are also verified. Mipmap generation remains unavailable. The restricted direct explicit-LOD path accepts finite literals or whitelisted side-effect-free scalar expressions only with a one-mip view, matching min/mag filters and anisotropy one; a single-mip helper still requires literal zero. General `textureSampleLevel` remains unavailable. |
| LOD | FL9_1 native MinLOD must be integral and MaxLOD must be FLT_MAX. | A normalized minimum requires a one-mip view. When only a finite maximum is raised and the minimum is preserved, a used multi-mip view requires the original maximum to reach its last mip and anisotropy 1. Anisotropic sampling with a raised finite maximum remains one-mip-only. Other unrepresentable mixed-filter and anisotropic ranges are rejected. FL9 does not specify whether clamping precedes min/mag selection. FXC rejects unlowered `SampleLevel` for the tested pixel profile with X4532. The separately gated direct subset lowers to implicit sampling only when one bound mip, equal min/mag filters and anisotropy one make the selected level and spatial filter independent of the explicit LOD. |
| Anisotropy | FL9_1 native maximum is 2; an upper limit alone is not a visual-quality guarantee. | The private backend accepts 1 or 2, requiring linear min, mag and mip filters for 2. `DownlevelLimits::max_sampler_anisotropy` reports 2, and core clamps requested values to this cap before HAL creation. Native creation and a sampled GPU output passed; anisotropic quality on minified textures is unverified. |
| Comparison / border | General comparison filtering is not guaranteed at 9_1; optional 9_x shadow support is separately queried. The supported native subset is `LessEqual` comparison with equal point or linear min/mag, one mip, no anisotropy and the FL9_1-required sampler LOD values. The shadow contract permits border addressing only for this depth comparison path, with border components 0 or 1. | The public and private D3D11 paths enable depth comparison only when `D3D11_FEATURE_D3D9_SHADOW_SUPPORT` positively reports support and the selected level is FL9_1. The public projection reports only the `LessEqual` function bit, disallows independent min/mag filters and limits comparison anisotropy to one. D16 and D24S8 use typeless depth resources with typed DSV/SRV views; private and public tests rendered 32 failing and 32 passing pixels for each format. Typed SRV `CheckFormatSupport` queries returned `E_FAIL`, but resource/view creation and sampled output succeeded. Strict public black/white border tests checked that all 64 out-of-range D16 comparisons failed/passed respectively. Core uses a downlevel limit to reject non-comparison border samplers; clamp-to-zero remains unavailable. Comparison on other levels and other functions/combinations remain unavailable. |
| NPOT | FL9_x permits a single-mip 2D NPOT texture only when `AddressU/V/W` are not `D3D11_TEXTURE_ADDRESS_WRAP`; full NPOT is an optional driver query. `MIRROR` is distinct from `WRAP`. | The private HAL checks only used NPOT/repeat pairs unless full NPOT is positively reported. Core also has selected-entry pair validation, but its FL9_1 fixture reports the finite per-primitive repeat limit of 128 and consequently rejects Repeat and MirrorRepeat sampler creation before pair validation is reachable. Clamp and mirrored-repeat do not trigger the conditional-NPOT `WRAP` rule; MirrorRepeat remains subject to the separate finite repeat limit. Synthetic core tests exercise selected-entry pair validation under a repeat-permitting limit, while the strict physical fixture verifies the 128 limit and renders POT/NPOT clamp sampling. |

Core reports `DownlevelLimits::sampler_lod_clamp = false` for FL9_1/9_2.
Before HAL sampler creation, it returns a validation error for a non-default
LOD range combined with distinct min/mag filters or anisotropy. This prevents
an unsupported descriptor from being translated to `DeviceError::Unexpected`
and incorrectly marking the device lost. Same-filter isotropic normalization
remains available with the existing one-mip view restriction. Non-finite LOD
values are rejected by Core.

HAL now returns the sampler translation's mip compatibility classification to
Core. Bind groups retain each sampled view's selected mip count and each
sampler's classification. At render-pass and render-bundle draw validation,
Core checks only the `(texture, sampler)` pairs used by the selected pipeline
against the currently bound groups. Rebinding a group therefore validates its
actual view ranges without a stale cache. The strict Core fixture rejects a
two-mip view paired with a sampler whose translated maximum is raised from
0.5, before the HAL draw is issued; single-mip cases remain permitted.

For normalized LOD ranges, patterned single-mip GPU tests cover both nearest
and linear mipmap filters on the RTX 3090 Ti at FL9_1. Explicit-LOD sampling
and historical FL9_1 driver behavior remain unverified. The current compiler
uses FL9_1 profiles even on higher-level devices; a selected entry point that
can reach `textureSampleBias` is accepted only for finite literal biases in
[-3, 3] for the no-offset form. WGSL has no separate `textureSampleOffset`
builtin; the optional constant texel offset belongs to `textureSampleBias`.
FXC returns X4532 for the native biased-offset form under both
`ps_4_0_level_9_1` and `ps_4_0_level_9_3`, while the SM5 control compiles. A
separate, narrowly proved path emulates one constant `i32x2` offset in
`[-8, 7]` for a direct sample or one coordinate-forwarding helper, using a
single-mip 2D view and the selected view dimensions. Other direct or
helper-reached offset forms are rejected before shader execution. Dynamic
biases, out-of-range values and bias expressions that cannot be proven from
literals are rejected. Core and HAL
resolve finite literal and unary-negated-literal arguments through nested
helper calls per call site, so a valid literal forwarded to a helper is not
mistaken for a dynamic bias; mixed call sites are checked independently. All
sampling pairs used by a selected entry containing bias must use equal min/mag
filters and anisotropy 1; normal view and LOD checks still apply.
One additional proven form is available to direct fragment-entry samples: the
bias may be an inline `select` whose accept and reject values are finite
literals within the reported maximum. This preserves the FL9_1 `texldb` range
for either condition result without clamping WGSL values. A `select` stored in
a local, used in a helper, containing a computed arm, or exceeding the bound is
rejected. The new downlevel limit is backend-implementation gated and defaults
to false for other backends. Strict forced-FL9_1 readback selected mip 0 for
the negative arm and mip 1 for the positive arm, matching 64/64 pixels. This
does not establish behavior on historical FL9_1 drivers.
Unrelated entries remain usable. The tested FXC emitted an unclamped legacy
`texldb`, for which Microsoft documents undefined behavior outside [-3, 3] at
ps_2_0. Nine strict FL9_1 GPU executions checked 64 pixels each, including a
fractional linear-mipmap blend; direct and indexed draws rejected incompatible
samplers. The public projection reports a maximum absolute literal bias of 3,
no dynamic bias and no unrestricted offset support. The separate
`fragment_texture_sample_bias_offset_2d_single_mip` limit exposes only the
tested constant-offset subset. Core's selected-entry analysis follows helper
calls and rejects unsupported forms as validation errors before HAL pipeline
compilation. Public readback verifies implicit sampling and
literal biases -3, +1 and +3 selected the expected mip colors across 384
pixels, including a +1 bias forwarded to a helper; synthetic interface tests
cover fractional bias, nested helper forwarding, dynamic arguments, mixed
call sites and out-of-range values. This
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
unfilterable formats or general comparison samplers. The public D3D11
projection now enables the narrow query-gated `LessEqual` depth-comparison
subset described above; historical drivers remain unverified, and this does
not widen the texture/sampler contract beyond that subset.
Render-pass boundaries clear context state.

Sampled texture and sampler entries may declare `FRAGMENT` or
`VERTEX | FRAGMENT` visibility. Visibility is retained per binding in layout
identity and stage mappings; combined visibility does not enable vertex-stage
texture or sampler use. A selected vertex shader that samples a texture is
rejected before drawing. Two strict FL9_1 RTX images checked fragment sampling
and a transition between these layouts; a controlled core case checked 64
pixels. CPU mapping and compiler guards cover stage identity and selected
vertex entries. The original strict baseline rejected the combined layout.
Historical FL9 drivers remain unverified; the surface contracts above do not
expand the sampled-resource contract.

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
historical-driver behavior. A narrow public exception permits direct
vertex-stage indexing of fixed-size `vec3` and `vec4` uniform arrays using
`vertex_index` or `instance_index`, plus fixed `mat4x4<f32>` arrays of at most
63 elements indexed directly by `instance_index`. Computed matrix indices,
non-square matrices and larger arrays remain rejected. The matrix bound is
252 float4 slots for that array, and FXC remains responsible for rejecting a
shader whose aggregate referenced constants exceed the profile budget. Other
runtime integer indexing remains rejected. A separate
`DownlevelLimits::fragment_uniform_array_indexing`
exception permits one or two direct fragment-stage reads from separate
`array<vec4<f32>, N>` uniform bindings for `2 <= N <= 24`. Each uniform uses
the existing structure shape with one scalar `u32` index at offset zero and
the array at offset 16; bindings may be in different groups. The array has
stride 16 and the structure span must be `16 + 16*N` bytes. Each index may be
used only for its corresponding read. In addition, one selected fragment entry
may forward its direct index through exactly one straight-line helper call.
The helper must take that `u32`, return the selected `vec4f` array element
directly, and contain no other operations, reads or calls. The entry must
return the helper result directly. Computed or cross-binding indices, extra
integer uses/accesses, additional reads, nested or repeated helper calls, and
other shapes remain rejected. Core and HAL apply the proof to the selected
entry point and clamp the helper index to the fixed array bounds. Naga emits a signed clamp for
`ps_4_0_level_9_1`. FXC accepts the 24-element case and rejects 25 elements;
the 24-element shader uses 40 Level9 pixel-shader instruction slots. Strict
public forced-FL9_1 readback checked all 64 pixels for direct indices zero,
one and 23, and all 64 pixels green for helper-forwarded index 23. CPU tests
cover entry-point selection, the forwarded float index and malformed helper
shapes. This compiler/backend boundary is measured on the RTX 3090 Ti and does
not establish historical-driver behavior or general fragment integer support.
The vertex path also accepts the
WGSL-valid padded wrapper form where each array element is a 16-byte struct
containing one `vec2<f32>` at offset zero. A plain uniform
`array<vec2<f32>, N>` remains invalid WGSL due to its 8-byte stride. Core and
HAL enforce the same wrapper shape, and strict FL9_1 GPU readback verified all
four indexed values plus out-of-range clamping. The level-9 HLSL path emits a
signed in-range clamp because FXC
rejects the unsigned clamp form for these profiles. Instruction pressure and
compiler packing can impose earlier limits on ordinary shaders. FXC errors
fail pipeline creation. FL9_1's usable profile budget is 255 vertex float4 constants and 32
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
The public projection allows a uniform binding up to 4,080 bytes (255 float4
vectors), matching the FL9_1 vertex profile's tested c254 boundary. The
selected-entry core validation separately keeps fragment use to 32 vectors;
a 33-vector fragment pipeline is rejected before HAL. Each stage remains
limited to six uniform bindings. A strict public draw now reads vertex c254
and fragment c31, changes the bound values between draws, and verifies that
the 4,080-byte vertex range shifts the rendered geometry. A 256-vector vertex
pipeline is rejected by core before HAL. Higher feature levels use a different
constant-buffer model and are intentionally not projected by this FL9_1-only
backend.

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
uniforms or interfaces, switches, non-folded global initializers and overrides,
and non-uniform dynamic array, matrix and vector indexing. One separate
`DownlevelLimits::vertex_index_local_array_indexing` exception permits a
single direct read from an initialized function-local `array<vec2<f32>, 3>`
in a straight-line vertex entry, indexed directly by `vertex_index`. It does
not permit helper forwarding, computed indices, writes, multiple dynamic
accesses or vector indexing. The level-9 HLSL writer uses signed clamping for
this exact fixed array because FXC rejects its unsigned clamp form. Strict
public readback verified a fullscreen triangle with all 64 pixels white;
Core, Naga and HAL keep the exception synchronized. FXC rejects the tested
dynamic `vertex_index` vector-index form with X3548. Proven static literal
indices and the separately validated uniform-array subset remain available.
General matrix-containing aggregate stores remain rejected in checked and
unchecked shaders because Naga's HLSL special lowering can lose writes or omit
an index clamp. Core and HAL allow only direct, statically indexed column or
scalar component writes to an ordinary `matCx2<f32>` member of a
function-local struct. Naga emits direct assignments to the split HLSL column
members for this exact access shape. Whole-matrix stores, dynamic column or row
indices, helper-mediated pointer writes, nested or array-contained matrices,
private globals and analysis-budget exhaustion remain rejected. The public
FL9_1 projection keeps `DownlevelLimits::matrix_aggregate_stores` false; the
exception is a bounded validation rule, not a general capability claim. Core
repeats the analysis for selected vertex and fragment entries and follows
helper calls. Float-only matrix reads and safe sibling entries remain usable.
Strict forced-FL9_1 public rendering verified the narrow exception through
Core, HAL, D3D11 and readback; CPU tests cover its pointer and index boundaries.
These are backend/compiler restrictions, not feature-level hardware
guarantees.
Naga's HLSL writer lowers f32 `mat2x2`, `mat3x3`, and `mat4x4` inverse using
explicit cofactors. DX11 exposes `mat2x2` and `mat3x3` through independent
`DownlevelLimits::matrix_inverse_2x2` and `matrix_inverse_3x3` limits. The
tested Level 9 vertex profile also supports the separate
`matrix_inverse_4x4_vertex` limit; Core rejects `mat4x4` inverse in fragment
shaders and keeps general `matrix_inverse` false. Core checks the selected
entry point and reachable helpers before HAL. Strict forced-FL9_1 GLSL tests
execute `mat2x2` and `mat3x3` in both stages and a runtime-data, non-symmetric
`mat4x4` inverse in the vertex stage. The mat4 case matched all 64 pixels at
`[64, 128, 128, 255]` within two UNORM steps; a fragment mat4 inverse is still
rejected before HAL. WGSL has no `inverse` builtin, so this applies to GLSL,
SPIR-V, or Naga IR inputs. It is a measured compiler/backend subset, not a
guarantee for every historical FL9_1 driver; DX12 and Metal keep the narrow
limit disabled.
Naga's HLSL writer now lowers `outerProduct` for validated f32 vectors and
square or rectangular matrices. The DX11 FL9_1 projection enables
`DownlevelLimits::matrix_outer_product`; selected-entry validation still
checks the operation and regular module functions before translation. Strict
GPU rendering at FL9_1 verified square and rectangular GLSL cases. The GLSL
front end currently has no WGSL `outerProduct` builtin to expose. DX12 keeps
the limit disabled pending its own capability decision and regression
coverage. This is compiler/backend support, not a broader FL9_1 hardware
guarantee.
Computed indices, helper-mediated indexing, fragment-stage indexing outside
the exact 2–24 element vec4 subset above, integer or nested array elements and other
general dynamic uniform indexing remain unsupported.
`DownlevelLimits::vertex_uniform_array_indexing` enables only the direct vertex
builtin subset described above. The RTX 3090 Ti at
requested/actual FL9_1 rendered all four array elements and verified that an
index equal to the array length selects the last element. This is selected
device/compiler evidence, not a historical-driver guarantee. Broad public
adapter exposure remains unsupported. GPU coverage exercises multiple range
bindings, pipeline transitions, a matrix and padded structure, and sampled
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

The selected FL9_1 public projection reports a separate
`DownlevelLimits::max_sampled_texture_sampler_pairs_per_shader_stage` of 16.
This models the observed `ps_4_0_level_9_1` FXC boundary independently of the
eight-texture and sixteen-sampler binding limits. Core counts Naga's validated
sampling relationships for the selected entry point; sixteen pairs compile,
while seventeen are rejected before HAL compilation. Sibling entry points in
one module have independent counts. This is a compiler/backend constraint,
not a general D3D11 binding-slot guarantee.

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
unavailable; the public projection remains FL9_1-only and exposes only the
constrained HWND and CoreWindow surface contracts summarized above.

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

The D3D11 projection separately enables the narrow
`texture_dimensions_2d_float_fragment` downlevel limit. Its direct form permits
one selected fragment entry point to query a non-array `texture_2d<f32>` in
any bind group and directly convert the `vec2u` result to `vec2f`. The query
may also appear in exactly one reachable helper with no arguments that returns
the query result; a 2D helper may use any texture bind group. The helper must
be straight-line, contain no other resource or image operation, and be called
once. Unreachable sibling helpers and their resources are pruned for HLSL
emission. The optional direct-query mip level must be a non-negative literal
or direct module `const`; the helper form is level-less only. Dimensions are
relative to the bound view. A direct
level-less query may share the entry point with exactly one implicit
`textureSample` of that same texture and one compatible non-comparison sampler.
For D2, the queried texture and sampler may each use any supported bind group,
including different groups. The helper itself remains query-only, but it may
coexist with exactly one level-less implicit sample of the same image in the
selected entry point, using one compatible non-comparison sampler and a
`vec2f` coordinate. D2, D3 and cube texture/sampler bindings may use any
supported groups, including different groups. This permits the proven
operations without permitting unrelated selected resources, other image
operations, control flow, `select`, or runtime integer work. A direct query
with an explicit mip level remains query-only. The HAL supplies
the selected view mip's dimensions through its existing private `b13`
emulation path and follows bind-group changes. For a level outside the view's
mip range, WGSL permits an indeterminate result; the host supplies a safe 1x1
value. General `texture_query` remains false, and other query forms continue to
fail Core validation. Strict RTX 3090 Ti readback verified direct and helper
query-plus-sample co-use across bind-group transitions for all 64 pixels at
requested/actual FL9_1. Naga, Core and HAL reject helper sampling, explicit LOD,
extra resources and additional image operations. This narrow limit does not
imply general WebGPU texture-query support.

A separate `texture_dimensions_3d_float_fragment` limit covers one direct
fragment `textureDimensions` query on a non-array `texture_3d<f32>` in any
supported bind group, directly converted to `vec3f`. Its optional mip argument
must be a nonnegative literal or module constant. A query with an explicit
mip cannot be combined with sampling in the same entry point. The feature is
exposed only when the selected FL9_1 device passes the existing RGBA8 3D
sampled-texture format query and the backend path is enabled; multi-mip
resources additionally require the selected device's `MIP` format query and
the separate `texture_3d_mipmaps` limit. Core, Naga and HAL keep this proof
distinct from the 2D path and from 2D texel-load emulation. It permits one
parameterless, straight-line helper that directly returns the level-less
`vec3u` query, with the entry point directly converting that result to
`vec3f`; a helper cannot take a mip argument, perform other operations or call
another function. Since FXC rejects the helper's unsigned vector return in
`ps_4_0_level_9_1` with X3548, Naga emits a signed `int3` helper return and
call result only for this proof; the host-supplied dimensions are known
nonnegative and fit the signed range. Additional bound resources and other
image operations are rejected. A level-less query may also share the entry
point with one implicit sample of that same volume through one non-comparison
sampler in any supported group using `vec3f` coordinates. HAL
supplies the selected view's width, height and depth at the requested mip via
`b13`, while the shared binding map assigns the ordinary texture and sampler
registers. Direct and helper query textures and samplers may use any supported
group. Strict RTX 3090 Ti tests switched
between 4x2x8 and 2x4x1 volumes
with different texel values; both dimensions and sampled data affected the
expected 64 readback pixels. A second strict test queried mip 1 of an 8x4x8,
three-level volume and verified 4x2x4 through GPU readback. Other helpers,
explicit LOD, extra resources and other image operations remain rejected. 3D
texture copies, attachments, readback and general texture queries remain
unavailable.

The opt-in FL9_1 projection exposes separate
`texture_num_levels_2d_float_fragment`,
`texture_num_levels_3d_float_fragment` and
`texture_num_levels_cube_float_fragment` limits. Each permits one
`textureNumLevels` query on one non-array float image of the matching
dimension from any bind group supported by the resource mapping, in a simple
fragment entry point, directly converted from `u32` to `f32`. Each dimension permits the query to be returned through exactly one
reachable, argument-free straight-line helper called once by the selected
entry point. The helper must directly return the query and remains query-only.
The selected entry may also sample the same image once directly through one
non-comparison sampler using implicit LOD and coordinates matching the image
dimension. For 2D images, texture and sampler may use different supported bind
groups; 3D and cube texture/sampler groups may also differ. The 3D and cube limits are enabled only when the selected
device reports the corresponding RGBA8 sampled-texture support; mipmap support is not required
to query a single-mip view. The HAL supplies the currently bound view's mip
count through the same `b13` emulation resource, including the view's suffix
mip range. The query remains a single direct `u32` to `f32` conversion. Other
helper shapes, control flow,
extra bound resources, other image operations and unrelated integer values
remain rejected. General `texture_query` remains false. This capability is
compiler emulation based and does not imply native FL9_1 image-query
instructions or historical-driver coverage. The strict forced-FL9_1 readback
test checks that the queried mip count and sampled texel contents both affect
output across bound views with different visible mip counts. The strict 2D
test exercises direct and helper queries with a texture at group 2 and sampler
at group 3. Strict 3D/cube coverage now checks direct and helper queries with
textures at group 2 and samplers at group 3, with all readback pixels
validated.

The general `texture_sample_base_clamp_to_edge` limit remains false. The
FL9_1 projection exposes only the separate
`texture_sample_base_clamp_to_edge_2d_float_fragment` restriction.
`textureSampleBaseClampToEdge` is lowered by Naga using
the one supported operation to an implicit `Texture2D.Sample` after clamping
coordinates to half-texel centers using the selected view dimensions supplied
through `b13`; this avoids FL9_1-incompatible `GetDimensions` and
`SampleLevel`. Core permits one fragment sample of a non-array
`texture_2d<f32>` and one non-comparison sampler from any supported bind
groups, including separate groups, either directly in the selected entry or
through exactly one reachable straight-line helper that is called once, takes
one `vec2f`, and returns `vec4f`. Resource handles cannot be passed through
the helper. Additional selected resources, other image
operations and control flow remain unsupported; unreachable sibling helpers
do not affect the selected entry. HAL
requires the currently selected view to expose exactly one mip and refreshes
the host dimensions for each selected bind group before drawing. The entry may
compute the `vec2f` argument passed to its helper. An unused sibling entry
remains available when it does not use the builtin. Strict RTX 3090 Ti FL9_1
readback verified all 64 output pixels for the helper path with transformed
out-of-range coordinates.
The public repeat-per-primitive limit rejects Repeat and MirrorRepeat samplers
at creation, so the GPU test uses ClampToEdge; sampler-independent wrapping
semantics have not been physically distinguished on this adapter.

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

The selected FL9_1 projection enables `vertex_index_input` for a constrained
emulation and sets `DownlevelLimits::vertex_index_topology_mask` to TriangleList,
PointList, LineList, LineStrip and TriangleStrip. Core rejects other topologies before HAL pipeline
creation, and the HAL retains its own defensive check. Naga lowers the selected builtin to a custom float vertex semantic;
pipeline creation reserves an IA slot and the HAL supplies a host-generated
identity stream. Direct IDs through 65,534 use that stream. Larger nonindexed
IDs and indexed IDs that cannot use the identity stream are replayed through
temporary float streams with corresponding vertex-stream offsets. The path
supports TriangleList, PointList, LineList, LineStrip and TriangleStrip. Indexed
point draws preserve resolved IDs through consecutive runs and the matching
identity or temporary float stream. LineList preserves `first_vertex`, complete
pairs and effective indices after `base_vertex`; Uint16/Uint32 paths use
Window/Gather remapping when the native FL9_1 index window is exceeded, then
restore ordinary and hidden IA vertex bindings. LineStrip preserves direct IDs
through batching. TriangleStrip expands indexed restart-separated runs into
winding-preserving triangle-list order before assigning IDs, including direct,
Window and Gather paths. Strict GPU tests cover direct and indexed formats,
signed base vertices, restart, wide Gather and user-stream restoration. Core
permits only direct `u32`-to-`f32` use, so
the float representation preserves the value consumed by the allowed shader
expression. Other integer operations remain rejected. FXC native `SV_VertexID`
still fails with X4555; the custom semantic compiles for
`vs_4_0_level_9_1`.

`instance_index` is lowered separately to a host-updated value in reserved
vertex constant register b13. The viewport transform shares that 32-byte
constant buffer, and replay writes `first_instance + instance` for each draw.

The strict public test at explicitly requested and selected FL9_1 checks
nonindexed and indexed TriangleList draws, negative base vertex, reindexed
Uint32 indices, a nonindexed `first_vertex` of 100,000 and a subsequent draw
after temporary-stream use. Readback red-pixel counts were 32/0/12/32/32/32.
The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) is the tested
physical device; historical FL9_1 driver behavior remains unverified.
`DownlevelLimits::vertex_index_input_requires_hidden_slot` also tells core that
this emulation consumes a native input slot. Core mirrors the FL9_1 slot
allocator, including overlapping-attribute spills, and rejects selected
`vertex_index` layouts that leave no slot for the hidden stream before HAL
pipeline creation. The same full layout remains valid for a shader that does
not use `vertex_index`. Synthetic allocator boundaries and a strict public
validation regression cover this rule; the HAL retains its allocation check.
`primitive_index` is separately gated by optional
`Features::PRIMITIVE_INDEX`; the public D3D11 projection does not advertise
that feature while the HAL rejects it. A strict public test confirms the bit
is absent and that `request_device` rejects it before native device creation.
[Microsoft HLSL semantics](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-semantics)
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
bindings together cannot exceed that shared allocator. One-dimensional
textures remain disabled: although the generic resource-limit table lists a
2048-texel FL9_1 1D dimension, Microsoft's [10Level9 `CreateTexture1D`
table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
explicitly marks the method unsupported on every 9.x feature level. The
numeric `Limits::max_texture_dimension_1d` value therefore does not imply that
the D3D11 FL9_1 API can create such a resource. Three-dimensional resources
are exposed only when
the selected device's RGBA8 query includes `TEXTURE3D | SHADER_SAMPLE` and the
sampled-upload backend path is present. This narrow path supports POT RGBA8,
single-sample volumes up to the FL9_1 dimension limit of 256, with
`TEXTURE_BINDING | COPY_DST` usages. `COPY_SRC` is also accepted when requested
for an execution-ordered CPU shadow used for texture-to-buffer readback and
the 3D-to-3D copy fallback;
it does not enable native 3D GPU copies. Mip chains are exposed only when the
selected RGBA8 query also includes `D3D11_FORMAT_SUPPORT_MIP` and the backend
mip path is enabled. Each mip is initialized and uploaded separately; shader
resource views must include the resource's lowest mip, as required by the
10Level9 FL9_1 view contract. A new downlevel limit lets core reject
non-suffix 3D views before HAL. Buffer-to-volume uploads use a source buffer
copy to a CPU-readable staging buffer followed by `Map` and `UpdateSubresource`,
preserving execution ordering at the cost of a blocking transfer. 3D
`COPY_SRC` and texture-to-buffer readback are implemented narrowly through
the ordered CPU shadow, not through native GPU-to-CPU copies. Same-format 3D
texture-to-texture region copies are also exposed through a replay-time
CPU-shadow fallback when `texture_3d_texture_copies` is true. Source textures
need `COPY_SRC`; destinations need `COPY_DST`. Core and HAL validate the
selected mip, bounds and format, reject mixed 2D/3D copies, and reject copies
within one 3D subresource. The fallback copies the requested voxels from the
execution-ordered source shadow, updates the destination shadow when one
exists, then calls `UpdateSubresource` with a non-null 3D destination box.
This avoids native 3D `CopyResource`/`CopySubresourceRegion` operations that
the 10Level9 FL9_1 context does not support. It does not support GPU-written
3D sources, 3D render attachments or storage. `COPY_SRC` retains one full
RGBA8 volume shadow; each copy also allocates a packed region buffer and costs
a CPU copy before the native upload. Buffer-to-volume upload still stages
through a mapped buffer followed by `UpdateSubresource`. 3D texture-to-buffer
readback copies the requested region from the shadow into the destination
buffer, preserving row padding and submission ordering. If the selected
format query lacks MIP support, the public descriptor remains single-mip.
A strict private test also uploads a padded, nonzero-offset 2×2×2 region to a
4×4×4 volume at (1,1,1), samples every slice, and checks all 64 pixels,
including initialized untouched voxels. The RTX 3090 Ti passed at
requested/actual FL9_1. A separate strict public test checks ordered upload
followed by full-volume and partial-region CPU-shadow readback, including
nonzero buffer offsets and padded row/depth layouts; all 64 volume texels and
all bytes in the partial destination buffer were validated. The strict public
copy test seeds the destination with nonzero voxels, copies a partial 2×2×2
region between differently sized volumes at nonzero XYZ origins, verifies all
destination voxels and readback padding, samples the copied resource, and
checks an out-of-bounds request is rejected. On the RTX 3090 Ti at requested
and actual FL9_1, all destination values matched and all 64 sampled pixels
matched. This is an ordered CPU-shadow/native `UpdateSubresource` fallback,
not native GPU-to-GPU copying; historical FL9_1 driver behavior remains
unverified.

The dynamic-uniform pipeline limit is twelve, matching the largest valid
six-vertex/six-fragment visibility split; one shader stage remains limited to
six. Other unsupported shader, storage and query paths remain absent from
projected flags or format usages. General multisampling remains unavailable except for per-count query-gated 2x/4x/8x RGBA8 render/resolve and matching D16/Depth24Plus/D24S8 depth attachments.

`DownlevelLimits::multisampled_texture_usage_mask` narrows that path to
`RENDER_ATTACHMENT` and optional `TRANSIENT_ATTACHMENT` usages. Core rejects
multisampled descriptors that also request `TEXTURE_BINDING`, `COPY_SRC` or `COPY_DST`
before HAL creation. Positive format-specific sample queries establish the supported attachment path only; they do not imply those other usages, depth resolve or multisampled shader bindings.

The projection reports `empty_binding_visibility = false`. WebGPU permits a
bind-group layout entry whose visibility bitset is empty, but the current HAL
layout representation requires every resource entry to target a supported
stage. Core rejects this layout shape before HAL for the FL9_1 projection.

The projection reports `sampled_depth_textures` only when the selected FL9_1
device positively reports `D3D11_FEATURE_D3D9_SHADOW_SUPPORT`. In that case,
single-sample D16, Depth24Plus and D24S8 depth views support the projected
LessEqual comparison path; Depth24Plus uses D24S8 storage while hiding
stencil. Core validates the comparison function, filter equality and
anisotropy limit. Signed- and unsigned-integer sampling and multisampled shader
bindings remain unavailable. A vertex-only texture or sampler layout is
rejected when the separate vertex sampled-texture limit is zero;
fragment-visible layouts remain valid. Depth-texture and comparison-sampler
entries may also declare combined vertex/fragment visibility; HAL maps those
resources only to the fragment stage. Core still rejects a vertex shader that
actually samples them because the vertex sampled-texture limit is zero. A
strict Core fixture verifies both layout acceptance and pre-HAL pipeline
rejection.

The projection withholds `Features::TEXTURE_COMPRESSION_BC`: that feature
covers BC1 through BC7, while this backend implements only BC1, BC2 and BC3
unorm and sRGB. Separate native-only unorm and sRGB subset features each unlock
only their own three formats, and only when every selected-device sampled-upload
query and backend path is available. Format usages remain query-specific; BC4–BC7
and CPU readback are not exposed. Strict public GPU coverage verifies the
unorm and sRGB paths, including same-format GPU copies, on the RTX 3090 Ti.

## Public render bundles

Strict public FL9_1 tests record both a nonindexed triangle and an indexed
textured triangle in `RenderBundleEncoder`s, execute them in color passes and
validate all 64 readback pixels for each. The indexed test drops the source
texture, sampler, bind group and vertex/index buffers after bundle recording,
then confirms the bundle retains what execution needs. Consecutive indexed
Uint16 and Uint32 TriangleStrip bundles include primitive-restart markers
between a left rectangle and right triangle, then execute in one pass while
switching vertex and fragment pipelines. Strict FL9_1 readback checks all 256
pixels: 37 green pixels from the first bundle, 37 red pixels from the second
and the unchanged clear gap. A separate two-bundle test switches both a sparse
uniform at group 0/binding 17 and a texture/sampler at group 1/bindings 7/42;
after dropping the recorded source handles, readback checks 32 pixels of each
distinct expected color. Two bundles also select distinct dynamic uniform
offsets from one buffer and verify red/green output after dropping the source
buffer and bind group. Another pair switches between a uniform-only pipeline
layout and a texture/sampler-only pipeline layout, verifying both outputs
after their source resources are dropped. Strict public coverage also includes
empty-bundle state reset, indexed triangle- and line-strip restart, inherited
scissor and blend constants, inherited stencil reference, and writable D16
depth across consecutive bundles. A colorless D16 bundle writes depth in a
depth-only pass; later depth probes verify its contents and Core rejects using
that bundle in a color-only pass. Read-only D24S8 stencil bundle aspects and
fractional-viewport bundle execution are verified by strict forced-FL9_1
public readback tests. A separate read-only D16 depth-bundle test executes in a
read-only depth pass, verifies all 64 color
pixels, proves with a later depth probe that the bundle preserved depth, and
confirms Core rejects a writable bundle in the same pass. A final test in this
group binds a writable D16 attachment to two consecutive bundles; the far draw
fails depth and readback checks all 64 near-red pixels after dropping the
source vertex buffer.

## Shader loop control

Microsoft's 10Level9 shader-profile table gives `vs_4_0_level_9_1` static flow
control to depth 1, but lists no flow-control capability for
`ps_4_0_level_9_1`. The 9.3 profiles add static control to depth 4 and the
pixel profile adds dynamic control to depth 24. The backend now disables
Naga's `force_loop_bounding` lowering for the two FL9_1 profiles. FXC compiles
statically bounded float loops in both stages, including the public GPU test's
four-iteration vertex and three-iteration fragment loops. That test checks all
64 output pixels on forced-FL9_1 hardware. FXC rejects the tested dynamic and
unbounded loop forms with X3511 before any draw is submitted.

The FL9_1 projection sets `DownlevelLimits::shader_loops` to true for the
statically bounded subset and `shader_dynamic_loops` to false. Core analyzes
the selected entry point and called helpers, then rejects loops whose exit
conditions depend on runtime data or whose exit is provably never taken before
HAL compilation. The analysis conservatively tracks local writes through
pointer parameters and unknown expressions. This avoids turning FXC's X3511
diagnostic into the only failure path. Static loops still depend on FXC's
instruction and unroll limits; the RTX 3090 Ti result does not establish
behavior on historical FL9_1 drivers.

## Representative public 3D scene

The strict public `dx11_fl91_scene` integration test renders a perspective-
transformed indexed cube with a sampled 2D RGBA8 face texture, uniform matrix,
nearest sampler and D16 depth attachment. It executes two command sequences
with opposite face order and compares every pixel in both 32×32 readbacks;
the RTX 3090 Ti at explicitly requested/actual FL9_1 produced identical
images with 154 red, 21 blue, 70 yellow and 779 clear pixels. A painter-order
control omitting depth differs at 223 pixels when face order is reversed. The
test proves this specific shader, binding, indexed draw, depth and readback
combination through public wgpu. It does not establish broader shader or
format coverage, historical FL9_1 compatibility, or application-level
performance.

## Narrow FL9_1 fragment cube `textureDimensions`

The separate `texture_dimensions_cube_float_fragment` limit permits one direct
`textureDimensions` query on one non-array `texture_cube<f32>` in any supported
bind group in the selected fragment entry point, followed by direct conversion
of its `vec2u` result to `vec2f`. The query may omit the mip level or use a nonnegative literal or
module constant. An explicit-level query cannot coexist with sampling in that
entry point; a level-less query may retain the existing one implicit sample.
The result is the selected cube view's face dimensions, not its layer count.
Core, Naga and HAL accept either a direct query or one parameterless,
query-only helper called exactly once by the selected entry point. A helper
must query the same bound cube directly, return `vec2u`, and contain no
sampling, extra calls, control flow or explicit mip. Any entry-point sample
must use that same cube with a `vec3f` coordinate. Dynamic or negative levels,
unrelated image operations, extra resources and runtime integer operations
remain rejected. Naga emits the cube SRV with the shared resource mapping; FXC
consumes the host-provided dimensions rather than emitting `GetDimensions`,
which the level-9 profile rejects. HAL supplies the current view's dimensions
at the selected mip through the existing `b13` emulation constant. Nonzero
levels still require the selected RGBA8 format to support mip chains; that
format query remains independent of this shader-query limit. Direct and
helper query images and optional non-comparison samplers may use any supported
group, including different groups. General
`texture_query` remains false. Strict forced-FL9_1 readback on the RTX 3090 Ti
verified the 8x8-to-4x4 transition across a direct query and a helper call and
the constant mip-1 query in an initialized mipmapped cube; all 64 pixels
matched within one byte. Historical FL9_1 driver behavior is not established
by the modern forced-level result.

## Combined FL9_1 2D `textureLoad` and `textureDimensions`

The existing `texture_load_2d_float_fragment` and
`texture_dimensions_2d_float_fragment` limits can be used together for one
selected fragment entry point that directly loads one texel and queries the
level-less dimensions of the same group-zero `texture_2d<f32>`. The load
coordinate must be a constant `vec2i`; the query must be directly converted
from `vec2u` to `vec2f`. Helpers, control flow, sampling, extra used resources,
other image operations and nonconstant integer operations remain unavailable.
The bound view must satisfy the existing single-mip RGBA8 texture-load path.
Core, Naga and HAL share the selected-entry and resource-identity proof. The
query value and load dimensions reuse the existing `b13` host constant and
`s15` point-sampling emulation; FXC compiles the combined pixel shader at
`ps_4_0_level_9_1`. Strict public GPU readback on the RTX 3090 Ti matched all
64 pixels. This is an intersection of two existing narrow emulations, not a
new general texture-query capability, and historical FL9_1 driver behavior is
unverified.

## Zero-input draw ranges

Core may accept an API draw whose `first_vertex + vertex_count - 1` exceeds
`u32` when the pipeline has no per-vertex inputs and the selected shader does
not consume `vertex_index`. In this case the start vertex is not observable
through vertex fetch or shader input. HAL normalizes the effective first
vertex to zero for generic and topology-specific replay only for this path.
Real vertex-fetch bounds and the separate `vertex_index` emulation range
remain checked. A strict Core/HAL FL9_1 fixture verified count-2 TriangleList,
LineList and LineStrip draws plus a count-3 TriangleStrip with
`first_vertex = u32::MAX`; each completed and left all 64 readback pixels
clear. This does not change the capability projection.

## Uniform buffers with FL9_1 fragment 2D `textureLoad`

The selected-entry fragment 2D float `textureLoad` proof now supports up to
eight selected image identities and up to six used uniform buffers. Images map
compactly to `t0`–`t7`; their dimensions share the reserved pixel constant
buffer `b13`, and the point/clamp emulation sampler uses `s15`. The normal
constant-buffer allocator maps the uniforms independently. Other used resource
classes and a seventh used uniform remain rejected. Multi-image loads
remain limited to single-mip, non-arrayed 2D float views; the separate 3D load
path and combined load-and-dimensions emulation remain single-image. Strict
forced-FL9_1 public readback verified all eight sampled inputs, positive and
negative out-of-bounds zero results, and the one-, two- and six-uniform paths.
The eight-image arithmetic test compiles under FXC's 64 arithmetic-slot limit.
Historical hardware behavior remains unverified.

The distinct selected-entry fragment 3D float `textureLoad` emulation also
allows up to six used uniform buffers, using the same compact constant-buffer
allocator while reserving `b13` for volume dimensions. A sparse group-1,
binding-42 uniform visibly tints a loaded voxel on the forced-FL9_1 RTX 3090 Ti.
Seven uniforms, another used resource class, multiple images and combined
load/dimensions remain rejected. This does not add general uniform or 3D
texture support.
