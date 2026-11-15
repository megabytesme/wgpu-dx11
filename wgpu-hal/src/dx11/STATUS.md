## Fixed integer fragment LOD on multi-mip views — 2026-11-14

Added a distinct downlevel limit for direct or supported single-helper
fragment `textureSampleLevel` calls with a statically known, nonnegative
integer LOD. Core and HAL prove that every sample of the selected texture uses
the same fixed level. Core validates the view-relative level and sampler at
draw time. The HAL lazily creates one-mip scratch resources per selected view
mip under a per-view mutex and retains the selected SRV and refresh copy in the
recorded command. This avoids native level-9 `SampleLevel` while selecting the
requested subresource. The prior one-mip path continues to clamp to its only
level.

The sampler minimum must be zero, min/mag filters must match, anisotropy must
be one, and the original maximum must reach the selected level. FL9_1's native
maximum remains `FLT_MAX`; a finite descriptor maximum is accepted only when
the selected level is no greater than that original maximum. The capability is
projected separately from both general explicit-LOD support and the prior
single-mip subset. Strict public readback coverage for full and nonzero-base
views, cache refresh, one-mip fallback and negative range/MaxLOD cases is
written but has not yet been executed after this change.

## Query-gated BC1/BC2/BC3 mip chains — 2026-11-14

The private/public mip paths for BC1, BC2 and BC3 UNORM and sRGB now require
the selected format query to report `TEXTURE2D | SHADER_SAMPLE | MIP` and the
backend mip implementation gate. Cube chains additionally require
`TEXTURECUBE`. This is optional selected-device support, not a new FL9_1
guarantee. `SINGLE_MIP_ONLY` remains projected whenever one of those
requirements is absent. BC4–BC7, render-target use and UNORM/sRGB view
reinterpretation remain unsupported.

Strict public GPU execution on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) explicitly requested and confirmed FL9_1. For all six BC1/2/3
UNORM and sRGB formats, it uploaded four mip levels, required and issued
same-format texture copies for every mip, sampled each copied mip, and matched
64/64 output pixels per format. It also uploaded and sampled all six faces
across four cube mip levels for each format, matching 768/768 pixels per
format. Logical 2×2 and 1×1 mips use one physical 4×4 block.

FL9_x documents possible corruption when copying from mapped staging resources
to default resources with a null box and mismatched dimensions. For a full BC
upload, replay now copies the mapped bytes into owned CPU memory before the
null-box `UpdateSubresource` call; the staging resource is no longer the update
source. This adds one CPU copy for a full compressed upload. Whole-mip
texture-to-texture copies use the null-box `CopySubresourceRegion` form only
between equal-sized DEFAULT subresources at zero destination offsets. The strict
RTX run verifies current driver behavior, not historical FL9_1 drivers.

## Helper-based explicit LOD zero with a tint uniform — 2026-11-14

Core and HAL now permit one selected fragment entry point to forward its
location-0 `vec2f` unchanged to one supported straight-line explicit-LOD
sampling helper and return the helper result multiplied by one used
`vec4<f32>` uniform. The proof requires a direct componentwise product and
rejects scalar promotion, addition, additional used resources, extra calls,
and control flow. Naga's existing lowering changes only the helper's proven
literal-zero sample to implicit sampling; ordinary uniform mapping supplies
the tint. The shared Core/HAL corpus covers positive and rejected forms, and
FXC accepts the lowered shader for `ps_4_0_level_9_1`.

The strict public offscreen test used a sparse tint binding with a patterned
2x2 texture and checked all 64 readback pixels against `[100, 5, 15, 255]` and
`[5, 55, 20, 255]` on the physical RTX 3090 Ti at explicitly requested and
confirmed FL9_1. This extends only the existing explicit-LOD proof subset;
general explicit LOD remains unavailable.

## ARM32 UWP cross-compilation — 2026-11-14

The Rust 1.93.1 toolchain has no prebuilt `thumbv7a-uwp-windows-msvc`
standard library. A separate Rust 1.95.0-nightly source build with
`-Z build-std=std,panic_abort` compiled `wgpu` and the D3D11 dependency graph
for that target using the Visual Studio ARM environment. The first attempt
used the nearly full `E:` volume and stopped with `no space on device`; the
same command succeeded with a temporary target directory on `C:`. This is
compile evidence only. Lumia deployment, CoreWindow creation and presentation
remain unverified because Device Portal is unreachable.

## Multi-mip literal-zero sampling views — 2026-11-13

The restricted fragment `textureSampleLevel(..., 0.0)` path now supports
multi-mip 2D views. FL9_1 requires an SRV to include the resource's smallest
mip, so the HAL does not create a base-only SRV over a multi-mip resource. It
copies the view-relative first mip into a retained one-mip sampled texture
using an explicit `CopySubresourceRegion` source box and zero destination
offset, then refreshes that texture before each affected draw. Equal min/mag
filters, anisotropy one, and zero minimum LOD remain required. Strict RTX 3090
Ti FL9_1 readback verified a full view, a nonzero-base view, a source update
after view creation, and a pipeline transition to minified implicit sampling;
all checked pixels matched. This adds per-view scratch allocation and a copy
before each matching draw. General explicit LOD remains unavailable.

## Core queue-idle error retention — 2026-11-12

Queue destruction now tries a bounded idle wait before releasing resources.
D3D11 timeouts retain the queue without declaring device loss; `device.poll`
and `poll_all` retry retirement, including after the public Device has been
removed. A 10 ms reaper budget can delay a poll when an orphan exists. HAL
errors still invalidate the device, preserve the queue, and report device loss;
unresolved queues are intentionally leaked during Hub shutdown rather than
releasing possibly in-flight resources. Strict FL9_1 fixtures cover timeout,
later polling, device removal, OOM and callback-once behavior. The timeout
bounds polling and waitable-handle waits, but synchronous D3D calls and Windows
scheduling can exceed the nominal duration.

## Query-gated RGB565 multisample resolve — 2026-11-12

RGB565 color multisampling now shares the query gate used by RGBA8/BGRA8 for
texture creation, views, pipeline validation and resolve replay. The
multisampled texture initialization clear also accepts RGB565. Synthetic tests
require all format-support bits and positive per-count queries. Strict
forced-FL9_1 execution on the RTX 3090 Ti passed a HAL resolve/fence/readback
test and public 2x/4x/8x render/resolve/sample tests; all 64 pixels matched at
each count. A strict public render/resolve/readback test also verifies
RGB565 alpha blending at 2x, 4x and 8x; all 64 pixels fall within the
per-channel RGB565 quantization range. The device reports no 16x quality
levels. Alpha-to-coverage remains unverified.

## Separate-group samplers for 3D/cube query sampling — 2026-11-12

The narrow selected-entry 3D/cube `textureDimensions` and `textureNumLevels`
proofs may now use a compatible non-comparison sampler from any supported
group, including a group distinct from the query texture. They still permit
only the proven same-image implicit sample and retain all existing resource,
stage, operation and view restrictions. Strict forced-FL9_1 public tests and
the complete affected suites passed; this does not enable general multi-group
sampling or unrestricted queries. Exact hardware and command results are in
`FL9_1_PROGRESS.md`.

## Depth comparison CTS coverage — 2026-11-12

Strict CTS depth comparison selectors passed for D16, Depth24Plus and
Depth24PlusStencil8: all 24 compare-function/clear-value cases per format,
72 total, on the physical RTX 3090 Ti at requested and actual FL9_1. No cases
skipped or failed. The equal/not-equal cases logged Core's invariant-position
warning. The broader mixed-format depth selector still has unrelated
unsupported-usage failures; see `FL9_1_CTS.md`.

## Fragment computed LOD in a sampling helper — 2026-11-12

The restricted single-mip `textureSampleLevel` subset permits one
straight-line fragment helper to return one sample with a whitelisted LOD
expression. With literal-zero LOD, its `vec2f` coordinate may use the existing
whitelisted arithmetic, composition, splat or swizzle operations. A computed
LOD helper may use the same bounded coordinate expressions. Unsupported math,
nested calls, control flow, extra resources, additional samples, and
incompatible views or samplers remain rejected. A single-mip bound view,
matching min/mag filters and anisotropy 1 remain required. Core and HAL use
matching selected-entry proofs and lower only the proven sample. FXC compiled
the helper at `ps_4_0_level_9_1`; strict forced-FL9_1 public readback matched
all 64 transformed-coordinate pixels on the RTX 3090 Ti. General explicit-LOD
sampling across mipmapped views remains unavailable.

## Nonzero-group `textureSampleBaseClampToEdge` — 2026-11-12

The selected-entry fragment proof accepts its single 2D float image and
non-comparison sampler from any supported bind groups, including separate
groups. It preserves the one-sample, helper, single-mip, entry-point and
resource-shape restrictions. The strict RTX 3090 Ti FL9_1 test used the image
in group 1/binding 7 and sampler in group 2/binding 42; all 64 pixels matched
for out-of-range coordinates. This changes logical group placement only; it
does not enable general bind-group or sampling support.

## Nonzero-group `textureNumLevels` — 2026-11-12

The selected-entry fragment `textureNumLevels` proofs now accept the queried
non-array float image from any bind group supported by the existing resource
mapping. The optional implicit sample still requires its non-comparison
sampler in group 0. Direct and one-helper forms retain their existing
operation, entry-point and view restrictions. Strict RTX 3090 Ti FL9_1 tests
switched between one- and three-level views at group 2/binding 42; direct and
helper forms, each with and without the same-image sample, matched all 64
pixels. This does not enable general image queries or general multi-bind-group
support.

## Query-gated BGRA8 fragment `textureLoad` — 2026-11-11

The selected-entry fragment 2D `textureLoad` subset now supports query-gated
`Bgra8Unorm` and `Bgra8UnormSrgb` views. Core enforces single-mip resources and
keeps sRGB resource/view pairs format-identical; the HAL permits these formats
only for the existing 2D point-load emulation. Strict FL9_1 RTX readback
verified logical RGBA channel order for both BGRA formats across 128 output
pixels, including sRGB decoding of `[128, 64, 32, 255]` to `[55, 13, 4, 255]`.
Historical-device format support remains query-dependent.

## Query-gated 8-bit SNORM fragment `textureLoad` — 2026-11-11

The selected-entry fragment 2D `textureLoad` path now accepts
`Rg8Snorm` and `Rgba8Snorm` when the selected device reports sampled upload
support. The point-sample emulation preserves signed normalized channel
semantics and leaves the separate sample-bias path unchanged. Core and HAL
keep the support limited to the existing 2D load subset. The strict public
RTX 3090 Ti FL9_1 test verified all four input texels and out-of-bounds zero
for both formats, 256 pixels total. Historical FL9_1 format support remains
query-dependent and unverified.

## Safe winit surface smoke — 2026-11-11

The Windows-only `dx11_fl91_winit` example uses the safe public
`Instance::create_surface(Arc<Window>)` path with winit 0.30. A strict run on
the physical RTX 3090 Ti selected FL9_1, presented 60 indexed textured
depth-tested frames and observed/reconfigured a 400x300 to 640x480 physical
resize. The example bounds surface and depth extents to the adapter's
downlevel limit. It supplements the offscreen readback example with a
conventional window-toolkit integration proof; it does not verify historical
drivers or Lumia/CoreWindow.

## Nonzero-group 3D/cube `textureDimensions` queries — 2026-11-11

The selected-entry fragment 3D and cube `textureDimensions` direct and helper
proofs now accept the query texture from nonzero bind groups, using the same
logical group/binding identity as the compiler resource map. Existing narrow
shape, selected-entry, view and resource restrictions remain. An optional
sampler stays group-0-only. New public tests cover sparse group-2/binding-42
textures and view transitions for direct and helper queries. Strict
forced-FL9_1 public tests on the RTX 3090 Ti cover direct/helper 3D and cube
queries; cube mip-1 readback also passes. The full public target passed 143/143,
with three optional capability subcase skips. This is not general
texture-query support or historical-driver evidence.

## Query-gated R8/RG8 fragment `textureLoad` — 2026-11-11

The restricted fragment 2D `textureLoad` emulation now accepts `R8Unorm` in
addition to `Rgba8Unorm`, and now also accepts query-gated `Rg8Unorm` for 2D
loads. Core checks the selected bound view and requires a single-mip parent;
HAL independently keeps the format allowance limited to 2D `Load` and
`LoadDimensions`, leaving 3D load and sample-bias offset on their previous
format restrictions. R8/RG8 resources require the selected device's
`TEXTURE2D` query and sampled-upload backend path, but not `SHADER_SAMPLE`
because the emulation uses a point sampler. Strict forced-FL9_1 RTX readback
verified R8 normalized red/zero green/zero blue/one alpha and RG8 normalized
red/green/zero blue/one alpha for all 16 in-bounds texels; all 48
out-of-bounds results per format were zero. R8/RG8 sample-bias remains
rejected, and historical FL9_1 drivers remain unverified.

## RGB565 query-gated sampled mip chains — 2026-11-11

RGB565 now follows the existing packed-format mip path: Core capability
projection, HAL validation and native texture construction require both the
backend path and the selected device's positive `TEXTURE2D | MIP` query. The
RTX 3090 Ti reports `0x0026F3A0` at requested and actual FL9_1. Strict public
coverage samples mip 1 using implicit derivatives and verifies the raw packed
word and readback sentinels. This is query-selected evidence for this driver;
historical FL9_1 behavior remains unverified.

## RGB5A1/RGBA4 ordered texture-to-buffer readback — 2026-11-11

The selected-device format query and existing same-format GPU `COPY_SRC`
projection remain prerequisites for RGB5A1 and RGBA4. Their additional
texture-to-buffer path copies raw 16-bit texels from an execution-ordered CPU
shadow; it does not create a native staging resource. The shadow is allocated
only for `COPY_SRC` textures when the implementation gate is enabled and is
updated by the existing ordered upload and texture-copy paths. Strict
FL9_1 hardware coverage validates packed words, partial-copy borders, padded
rows, offsets and sentinels for both formats. The formats remain
non-renderable.

## RGB5A1/RGBA4 query-gated mip chains — 2026-11-11

The selected-device `TEXTURE2D | MIP` query now gates packed RGB5A1/RGBA4
sampled/upload mip chains in both the capability projection and native texture
constructor. The RTX 3090 Ti returned `0x000213A0` for each format at requested
and actual FL9_1. Strict public coverage uploaded mip 1, copied a partial
region, verified padded raw-shadow readback and sampled a single-mip view for
both formats. Mip generation, rendering and historical-device behavior remain
unsupported or unverified.

## R16G16_SNORM query-gated mip chains — 2026-11-11

`Rg16Snorm` multi-mip creation and sampled/upload capability now require both
the backend path and the selected device's positive `TEXTURE2D | MIP` query.
`SHADER_SAMPLE` is not required for point-only use and still controls
filterability independently. The RTX 3090 Ti forced to FL9_1 reports
`0x00021122`, including MIP but not `SHADER_SAMPLE`. Strict public tests upload
three levels and sample mip 1 through a single-mip view, then separately
perform a mip-1 partial GPU copy and verify raw readback, padded rows and
sentinels. Historical FL9_1 devices remain unverified.

## R16G16_SNORM GPU texture copies — 2026-11-11

The private format model exposes `COPY_SRC` for `Rg16Snorm` only when the
selected device reports `TEXTURE2D` and the backend same-format GPU copy path
is enabled. The separate texture-to-buffer mask additionally requires the
backend's ordered CPU-shadow implementation. A strict public forced-FL9_1 test
copies a nonzero-origin 4×4 region, reads back all 64 texels with a nonzero
offset and padded rows, and verifies raw values, zeroed borders and sentinels.
The copy remains native; readback uses extra CPU memory matching the texture
storage. Historical-driver behavior remains unverified.

## BC cube-format downlevel-mask correction — 2026-11-11

`DownlevelLimits::texture_cube_format_bit` now assigns separate bits to all
BC1/2/3 UNORM and sRGB cube formats. The D3D11 capability projection already
queried these formats independently, but absent shared mask bits silently
dropped BC2/3 UNORM and all sRGB variants before Core could expose the queried
result. The RTX 3090 Ti at requested/actual FL9_1 reports cube mask
`0x0003ff1f`; all six formats also report `COPY_SRC` and texture-to-buffer
support. A strict public test uploads distinct blocks to all six faces and
checks exact bytes through the ordered CPU-shadow readback, including padded
rows and sentinels. This proves the format projection and transfer path, not
cube sampling for every variant or behavior on historical drivers.

The focused `wgpu-types` cube-mask test, D3D11 capability-model projection
test, and strict public BC target passed. The public test logged the physical
adapter (`0x10de:0x2203`, driver `32.0.16.2101`, software=false) and completed
at requested/actual FL9_1. See `FL9_1_PROGRESS.md` for exact commands.

## RGBA8_SNORM shadow readback — 2026-11-11

The existing query-gated RGBA8_SNORM `COPY_SRC` path now also supports
texture-to-buffer readback through an execution-ordered raw-byte CPU shadow.
The selected device must report `TEXTURE2D`, and the backend implementation
gate must be enabled; native SRV-only staging readback remains unavailable.
Strict public FL9_1 hardware coverage verifies padded upload, partial GPU copy,
nonzero readback offset, padded rows and all 24 texels, including zeroed
untouched destination texels. The focused target passed 3/3 on the RTX 3090 Ti
at requested and actual FL9_1. Earlier entries below that say RGBA8_SNORM
readback is unsupported describe the state before this change.

## Selected fragment uniform-array helper — 2026-11-11

Core and the private HAL now accept one constrained helper form for the
existing direct fragment uniform-array lookup. The selected fragment entry
must make exactly one call to a straight-line helper, pass the direct `u32`
index member from the same canonical uniform, and return the helper's result
directly. The helper takes that `u32`, reads exactly one `vec4f` element from
the matching uniform array, and returns that load. Computed indices, extra
integer uses, other reads, nested/repeated calls, control flow and different
layouts remain rejected. The proof is selected-entry-specific and preserves
the existing bounds clamp.

Synthetic Core and HAL tests cover helper selection, malformed neighboring
shapes, and a legal unused floating-point entry input. The HAL test also
generates level-9 HLSL and compiles it through FXC. Strict public execution on
the RTX 3090 Ti at requested/actual FL9_1 selected index 23 through the helper
and verified all 64 readback pixels were `[0, 255, 0, 255]`. This expands only
the fragment uniform-array subset; it does not enable general runtime integer
semantics or helper-mediated arbitrary resource access.

## FL9_1 selected-entry helper pruning — 2026-11-11

The HLSL writer emits regular functions reachable from the selected entry
point(s) only. Core's matrix inverse and outer-product restrictions use the
same entry-point call graph, so unsupported operations in unused helpers no
longer reject an otherwise supported pipeline. A reachable mat3 inverse still
fails as a Core validation error. The strict public mat2 inverse render passed
all 64 pixels with an unused mat3 helper in the fragment module.

The complete strict public D3D11 target passed 132/132 test functions and the
strict private HAL suite passed 490/490. Naga passed 163/163, Core passed
139/139, the DX12 HAL check passed, and affected Clippy and formatting checks
passed. See `FL9_1_PROGRESS.md` for exact commands and hardware evidence.

## Direct uniform-indexed vertex palette — 2026-11-14

Core and HAL accept one additional selected FL9_1 vertex-shader form:
`palette.values[palette.index]`, where the same bound uniform contains a `u32`
at offset zero and a fixed `array<vec4f, N>` at offset 16 (`2 <= N <= 253`,
exact span). FXC accepts length 253 and rejects length 254 with X4507 at
constant register index 256. The entry must be straight-line, use exactly one
direct dynamic array access, and have no helpers, control flow, `select`, computed index or
other dynamic indexing. The existing `vertex_uniform_array_indexing` limit is
unchanged; general runtime integer operations remain disabled.

Core and HAL synthetic tests reject computed, helper-mediated and wrong
element shapes. FXC compiles the level-9 vertex shader. Strict public execution
on the physical RTX 3090 Ti at requested/actual FL9_1 used four bind groups to
select four palette values and verified every pixel in each 8x8 readback.
Out-of-range uniform indices and historical FL9_1 drivers remain unverified.

## RG8_SNORM shadow copy and raw readback — 2026-11-11

RG8_SNORM `COPY_SRC` is now projected only when the selected device reports
`TEXTURE2D`. The backend does not attach an RTV or issue native texture-copy
commands for this format: an execution-ordered CPU shadow tracks initial
zeroing, queue uploads and buffer-to-texture updates, and a region copy writes
both the shadow and the native destination with `UpdateSubresource`. Raw
texture-to-buffer readback uses the same shadow and preserves the exact signed
byte representation. Self-copy within the same subresource remains rejected.
The strict public test checks padded and partial uploads, exact source and
destination bytes, untouched regions and sampling from the copied native
destination. The strict public RG8_SNORM test passed on the RTX 3090 Ti at
requested/actual FL9_1, and the complete strict private HAL and public D3D11
suites passed 491/491 and 129/129. One optional Float16 vertex-input case
printed a selected-device capability skip. The selected RTX driver's native
copy failure and historical FL9_1 behavior remain distinct; see `TESTING.md`
for the query gate and test results.

## Odd-sized index ranges — 2026-11-11

Commit `ab926004e` accepts in-bounds index ranges whose offset is aligned but
whose byte size ends with a partial index, matching Core's complete-index
counting. The strict public FL9_1 test rendered all 64 pixels from the three
complete Uint32 indices in a 13-byte range; a four-index draw was rejected
when the command encoder was finished.

## Zero-input draw range

For nonindexed draws with no per-vertex inputs and no `vertex_index` use, HAL
normalizes the effective first vertex to zero when `first_vertex + count`
exceeds `u32`. Strict Core/HAL readback covers TriangleList, LineList,
LineStrip and TriangleStrip; real fetch and `vertex_index` checks remain
intact.

## Timestamped depth-Never draws

Depth comparison `Never` draws execute with color and depth writes disabled,
including when the render pass contains pass-boundary or explicit timestamp
writes. Strict RTX FL9_1 readback verified unchanged color, unchanged depth
through a farther probe draw, and nondecreasing timestamps. This closes a
Core-valid command-recording failure without changing capability reporting.

## Combined visibility for depth/comparison bindings — 2026-11-10

Depth-texture and comparison-sampler bind-group entries can declare both
vertex and fragment visibility while remaining pixel-stage resources in the
HAL mapping. Core's zero vertex-sampled-texture limit still rejects a vertex
entry point that actually samples the depth texture. The strict RTX 3090 Ti
fixture verified successful layout creation and this pre-HAL pipeline
rejection at requested/actual FL9_1; no capability changed.

## FL9_1 sRGB blend semantics — 2026-11-10

The native format query's `BLENDABLE` bit only confirms that blending is
available. Microsoft's D3D11 contract requires FL9_1–FL9_3 hardware blending
for RGBA8/BGRA8 sRGB render targets to occur in sRGB space, while wgpu exposes
sRGB storage as encoded values with linear shader values. The FL9_1 public
projection therefore keeps sRGB attachment support but withholds
`COLOR_ATTACHMENT_BLEND`, even when the driver query reports `BLENDABLE`.
Linear RGBA8/BGRA8 blending remains selected-device-query gated. A synthetic
capability test checks all seven feature-level identities and both sRGB
formats; the strict public FL9_1 test checks the actual adapter projection and
Core rejection before HAL. The focused public test passed on the RTX 3090 Ti
at requested/actual FL9_1 and retained the linear RGBA8 64-pixel blend result.
The complete strict public target passed 132/132 test functions, and the
complete strict private HAL library suite passed 490/490. This corrects an
overclaim in capability reporting; it does not change native private-HAL
blend-state creation.

Latest shader-input status: `instance_index` and a restricted `vertex_index`
subset now execute on the opt-in FL9_1 path. `vertex_index` uses a custom float
IA semantic with direct identity IDs up to 65,534 and temporary remapped streams
for larger IDs, including nonindexed `first_vertex`. It is supported for
TriangleList, PointList, LineList, LineStrip and TriangleStrip, with direct `u32`-to-`f32`
use only. LineList preserves pairs and effective indexed IDs across
Window/Gather remapping and restores changed IA vertex slots. LineStrip keeps
`first_vertex` through direct batching and maps indexed restart runs and wide
Gather pairs to the same hidden ID stream. `DownlevelLimits` reports all five
topologies. Direct TriangleStrip draws preserve source IDs across batching;
indexed strips resolve signed base vertices and restart runs, then expand to
winding-preserving triangle-list IDs through Window/Gather paths. Strict RTX
3090 Ti readback covers direct and indexed strips, signed base vertex, primitive
restart and wide-span Gather. See `TESTING.md` for exact commands and results.

The public fragment-shader path also has a strict forced-FL9_1 `discard`
regression. Its readback proves that discarded fragments preserve both the
prior color and depth: a subsequent depth-tested draw updates only the pixels
that the first shader discarded. This exercises the existing Naga HLSL
translation and FXC path without adding a backend capability or broadening the
shader contract.
# Direct3D 11 backend status

## BC readback and fragment sample-bias offset — 2026-11-11

Core and HAL now also support direct fragment `textureSampleCompare` and
`textureSampleCompareLevel` calls with one constant `i32x2` texel offset in
`[-8, 7]`. The selected depth view must be 2D, single-mip and single-sample;
the actual bound view is checked at draw time. The DX11 projection enables the
subset only when D3D9 shadow-comparison support was positively queried. Naga
emits coordinate adjustment using the selected view's dimensions instead of
FXC's rejected native-offset call. Strict requested/actual FL9_1 tests on the
RTX 3090 Ti compiled and rendered both builtins, with the shifted comparison
edge matching all 64 readback pixels. Other comparison-offset shapes and
unsupported or unqueried devices remain rejected.

BC1/BC2/BC3 `COPY_SRC` now has a narrow texture-to-buffer path backed by an
execution-ordered CPU shadow. Direct writes, mapped buffer uploads and
texture-region copies update the shadow at their command replay position;
readback copies the raw compressed block bytes into the requested destination
layout. This avoids the FL9_x-prohibited GPU copy from shader-resource-only BC
textures, but duplicates compressed storage on the CPU and depends on every
future write path maintaining the mirror. Strict FL9_1 public tests cover
padded readback, offsets, BC1 unorm cube faces, all three BC sRGB source formats
and compatible UNORM/sRGB copies. The sRGB direct-readback case checks raw
blocks, row padding and sentinel bytes.

Core and the private HAL also support the bounded fragment
`textureSampleBias` texel-offset subset through either a direct sample or one
coordinate-forwarding helper. The view must be 2D and expose one mip, but may
select one mip from a multi-mip parent texture. HLSL applies the constant
offset using that selected view's dimensions. Strict public readback passed
for standalone 8x8 and selected 4x8 mip-1 views through both direct and helper
forms. Nested helpers, modified coordinates, multiple samples, arbitrary
offsets and other texture shapes remain rejected.

The sample-bias audit found and fixed a Core/HAL mismatch: Core's legal
single-mip view of a multi-mip parent was previously rejected because the HAL
checked the parent mip count instead of the view's mip range. The strict
multi-mip-parent case now exercises the corrected boundary. Hardware results
are limited to the RTX 3090 Ti at requested/actual FL9_1 and do not represent
legacy-device verification.

Recent public strict-hardware checks verified partial initialization of BC1,
BC2 and BC3 textures (all 96 sampled pixels) and WGSL `determinant` for mat2,
mat3 and mat4 in both vertex and fragment stages (768 readback pixels). All
seven tests passed on the RTX 3090 Ti at explicitly requested/actual FL9_1.
These checks add execution evidence for the current restricted projection;
they do not broaden it. See `TESTING.md`.

Core and HAL now permit a direct fragment `textureSampleBias` whose bias is an
inline `select` between two finite literals within the FL9_1 `[-3, 3]` range.
Out-of-range or computed arms and helper-contained selects remain rejected.
FXC accepted the `ps_4_0_level_9_1` shader, and strict public readback on the
RTX 3090 Ti selected mip 0 and mip 1 on opposite halves; all 64 pixels matched.
The capability is a separate downlevel limit and remains false by default for
other backends. Arbitrary dynamic bias is still unsupported.

The selected-entry `textureSampleBias` path now validates sampler compatibility
in Core before direct draw encoding. Only the DX11 projection requires matching
min/mag filters and effective anisotropy 1; other backends default to no such
restriction. Strict FL9_1 readback still passes the supported bias shader, and
a strict public negative test verifies mixed-filter and anisotropy-2 samplers
produce the expected Core validation error. The render-bundle Core guard is
present, but `RenderBundleEncoder::finish` reports failures through the Rust
wrapper's fatal path rather than an error scope.

RGBA8/BGRA8 linear and sRGB format capabilities are now projected per usage.
If a selected-device query lacks render-target support, independently valid
sampled and copy-destination usages remain visible to Core; the production HAL
checker still rejects the unsupported attachment and copy-source requests.

A strict public `@invariant @builtin(position)` regression also completed a
depth-only prepass followed by an Equal-depth color draw; all 64 readback
pixels matched. This verifies the tested shader and pipeline shape on the
development GPU without changing the capability projection.

Public GPU coverage now combines query-supported 2x/4x/8x RGBA8 multisampling,
alpha blending and red/blue-only writes. All 64 resolved pixels matched on
the RTX 3090 Ti at requested/actual FL9_1; no capability flags changed.

The opt-in `dx11` backend now enumerates physical adapters that can create an
explicit FL9_1 device and exposes the selected-device capability projection
for offscreen rendering and constrained Win32 HWND or WinRT CoreWindow surfaces
through the public wgpu API. Both surface paths report BGRA8 sRGB FIFO, opaque
composition, render-attachment usage and one frame of latency. `COPY_SRC` is
added only when the selected BGRA8 format projection supports texture readback;
a strict HWND test renders, copies the acquired frame to a padded buffer,
validates the pixels and presents it. CoreWindow creation verifies dispatcher
access, and surface operations are restricted to the creating thread.
CoreWindow runtime behavior remains unverified. The
backend exposes the native-only `TEXTURE_FORMAT_16BIT_NORM` feature only when
a selected-device query reports
the required 2D format support. The native-only `TEXTURE_FORMAT_16BIT_PACKED`
feature covers query-backed RGB565, RGB5A1 and RGBA4 sampled uploads and
sampled mip chains when each selected query also reports `MIP`. RGB565
additionally has queried render-target and copy/readback support; RGB5A1 and
RGBA4 additionally have query-gated same-format GPU
texture copies and ordered CPU-shadow texture-to-buffer readback, but no
render-target support.
R8_UNORM and R8G8_UNORM
also expose copy-source use only when their selected format query reports
render-target support. Their blendability is separately projected only when
the selected query reports `BLENDABLE`; strict FL9_1 public readback verifies
half-alpha red overlays for all three formats on the RTX 3090 Ti. A strict
public test renders into each of these three narrow formats, copies the
rendered attachment to a padded readback buffer and validates all native
texels. The backend does not
claim WebGPU compliance. The implementation remains experimental
and restrictions not representable through current core limits still need
auditing. Feature level 9_1 is a tested target, not evidence of
historical-driver compatibility. See [CAPABILITIES.md](CAPABILITIES.md) for
the capability matrix and validation gaps.

The current tree contains separate, narrowly gated fragment 2D and 3D
`textureLoad` paths. The 2D path supports up to eight selected single-mip,
non-arrayed float images, compactly mapped to `t0`–`t7`, and one optional user
uniform; the 3D path remains a single-image path and requires a positive
selected-device RGBA8 volume-sampling query. The 2D emulation uses per-image
sizes in `b13`, point/clamp sampling through `s15`, and masks out-of-bounds
coordinates to zero. Core and HAL reject a ninth 2D image. FXC compiles the
representative eight-image shader after redundant coordinate clamping was
removed; the pixel profile's 64 arithmetic instruction budget remains a
shader-dependent limit. Strict public readback covered two and eight distinct
2D resources, positive and negative out-of-bounds behavior, uniform tint and
sRGB decoding on the RTX 3090 Ti at requested/actual FL9_1. This does not
establish historical-driver behavior.

The public rasterizer path now has a strict forced-FL9_1 readback test for
opposite triangle windings under `FrontFace::Ccw` and `FrontFace::Cw` with
back-face culling. The test verified 10 red CCW pixels, 10 green CW pixels and
44 clear pixels on the RTX 3090 Ti. Other render-state combinations and
historical driver behavior remain under audit.

The opt-in public projection now exposes comparison sampling only when the
selected FL9_1 device positively reports D3D9 shadow support. Strict public
D16, Depth24Plus and D24S8 tests each verify 32 failed and 32 passing
comparisons. Depth24Plus uses D24S8 native storage and exposes no stencil
aspect. Core rejects unsupported functions, unequal filters and comparison
anisotropy above one. Historical drivers and depth-comparison behavior on
other hardware remain unverified.

When the same selected-device shadow query is positive, the opt-in projection
also enables `ADDRESS_MODE_CLAMP_TO_BORDER` for comparison samplers only.
`DownlevelLimits::border_addressing_requires_comparison_sampler` makes core
reject ordinary border samplers before HAL; clamp-to-zero remains unexposed.
The strict public FL9_1 tests used out-of-range depth coordinates and verified
that opaque black fails and opaque white passes all 64 comparison pixels,
while ordinary border and unrequested zero-border descriptors fail
validation. This selected NVIDIA result does not establish behavior on
historical FL9_1 drivers.

Public `SurfaceTexture` clones now share one presentation lifecycle. Dropping
an alias no longer discards a frame still held by another clone, and presenting
any clone prevents later aliases from trying to discard the already-presented
frame. The strict HWND test covers both clone-drop orders and reacquisition.
Dropping an unpresented frame after its `Surface` is destroyed is also safe;
the core output detail no longer calls through a removed surface ID. Calling
`Queue::present` on a retained frame after surface destruction reports an
invalid-surface validation error rather than panicking on the stale ID. The
strict test verifies this does not consume or disturb a frame acquired from a
second active HWND surface.

The optional RGBA8 and BGRA8 2x/4x/8x paths are restricted through
`DownlevelLimits::multisampled_texture_usage_mask` to color attachment and
transient attachment usages. Core rejects sampled and copy usages before the
HAL even when the selected format reports multisample render/resolve support.
Each format uses its own `CheckFormatSupport` and per-count
`CheckMultisampleQualityLevels` results, and resolve commands preserve the
selected native format. A strict RTX 3090 Ti FL9_1 BGRA8 clear/resolve test
checks 64 readback pixels; this optional result does not establish historical
device support.

The selected FL9_1 projection exposes sampled RGBA8 and R8_UNORM 3D textures
when each format has the required feature-level guarantee, the selected
device reports `TEXTURE3D | SHADER_SAMPLE`, and the shared sampled-upload path
is enabled. Volumes are POT, single-sample and up to 256³ with texture-binding
and upload usages. Forced-FL9_1 public readback on the RTX 3090 Ti verified
four distinct RGBA8 slices, a nonzero-offset partial voxel update and zero
initialization for untouched voxels across 128 pixels. A strict private R8
3D upload-and-sample test verifies all 64 output pixels, including the
format's one-byte texel layout and zero initialization. Buffer-to-volume upload currently stages
through a mapped buffer and `UpdateSubresource`. `COPY_SRC` uses an
execution-ordered CPU shadow because 10Level9 does not provide a safe native
3D texture-to-buffer copy path; buffer-to-volume uploads update both the GPU
resource and shadow from the staged source. Same-format 3D-to-3D copies are
now exposed through a CPU-shadow fallback when the selected format has a
projected volume path. The source requires `COPY_SRC` and the destination requires
`COPY_DST`; same-mip self-copy and mixed dimensions are rejected. Replay copies
the requested region from the source shadow, updates the destination shadow if
present, and uploads the packed region with `UpdateSubresource` and a non-null
destination box. This is not a native GPU copy and cannot consume GPU-written
volume data. Strict public FL9_1 coverage seeds a differently sized
destination with nonzero values, copies a partial 2×2×2 region at nonzero XYZ
origins, checks preserved outside voxels and readback padding, rejects an
out-of-bounds copy, then samples the destination; all voxels and 64 output
pixels matched on the RTX 3090 Ti. A query-supported mip-1 copy also preserved
untouched voxels. This path adds a full source shadow when `COPY_SRC` is
requested plus a temporary allocation and CPU copy per 3D copy. Render
attachments and storage remain unavailable.
The private HAL transfer regression additionally uploads a padded, nonzero-
offset 2×2×2 region at volume origin (1,1,1), samples every slice and verifies
all 64 target pixels, including 56 initialized untouched voxels. It passed on
the RTX at requested/actual FL9_1. Indexed rebase/gather planners now copy
byte-sized stream extents without imposing a four-byte extent multiple while
retaining their separate offset, stride and bounds validation; positive GPU
execution for optional sub-four-byte IA formats still requires another
format-positive FL9_1 device.

At FL9_1, mutable format view lists remain disabled. Compatible RGBA8/BGRA8
UNORM-to-sRGB reinterpretation requires typeless resources, which the 10Level9
format table does not provide; the RTX 3090 Ti's forced-FL9_1 native creation
probe also returns `E_INVALIDARG`. A strict public test verifies that the
adapter omits `VIEW_FORMATS` and core rejects both compatible view pairs before
the HAL. Standalone typed sRGB textures remain available through their own
queried format paths.

HAL maps device-hung, removed, reset and driver-internal-error HRESULTs to
`DeviceError::Lost`. A controlled core fixture now injects a synthetic
`DXGI_ERROR_DEVICE_REMOVED` during FL9_1 command replay and verifies submission
failure, device invalidation, exactly one lost callback and a reentrant device
poll from that callback. Fence `GetData` and wait errors also return their lost
callback through core's closure batch, after internal locks are released; both
reentrant-poll paths are tested. A third test injects the error from
`GetDeviceRemovedReason` during ordinary device polling; that callback also
reenters device polling and later queue submissions are rejected. Queue
destruction also has a synthetic `wait_for_idle` removal test: core reports
one lost callback, that callback reenters polling, and queue drop completes
without panicking. All paths use a physical adapter explicitly opened at
FL9_1, but the injected errors do not represent actual hardware removal or
recovery behavior.

The opt-in FL9_1 public path supports single-sample RGBA8 cube textures with
six independently uploaded faces. POT mip chains are enabled only when the
selected format query reports `D3D11_FORMAT_SUPPORT_MIP`; all mip contents must
be supplied by the caller. Core gates cube size, shape, mip count and format;
ordinary 2D arrays and cube arrays remain disabled. On the RTX 3090 Ti forced
to FL9_1, all six faces across four mips and a non-suffix mip view matched
960/960 output pixels. GPU copies are also tested between cube subresources and
ordinary 2D textures in both directions: the copied cube face is sampled, and
the copied 2D texture is read back. Cube texture-to-buffer readback remains
rejected by core's one-layer source limit because FL9_1 cannot stage an
SRV-only resource to CPU-visible memory. Render-target cube views remain
unsupported.

The selected-entry sampler contract also rejects the WGSL constant offset
variant of texture sampling through the existing bias-offset limit. Direct and
helper-reached uses are covered by core and private-HAL checks and by the
strict public sampled-mip test. FXC reports X4532 for the Naga-generated form
under both FL9_1 and FL9_3; the SM5 control compiles. This remains an
unsupported operation and does not add shader functionality to the backend.

Public buffer-map invalidation now reserves its temporary staging-readback
allocation fallibly before mapping the D3D11 staging resource. Allocation
failure is reported as device out-of-memory instead of panicking or leaving a
native resource mapped during unwinding.

The public `Buffer::destroy` path now checks for outstanding mapped views
before delegating to core unmap/destruction. The check leaves the mapped-range
state intact on panic; a strict FL9_1 public test verifies both disjoint views
remain readable and that destruction succeeds after they are dropped.

The strict public buffer-transfer test uploads a patterned 64-byte buffer,
clears a requested aligned 16..48 range, and verifies by copy/map readback that
the range is zero while bytes outside it are unchanged. Core rejects unaligned
and out-of-bounds clear ranges before HAL.

The selected FL9_1 public projection also reports a separate limit of 16
distinct sampled texture/sampler relationships per shader entry point. Core
counts Naga's validated relationships for each selected stage and rejects an
over-limit pipeline before HAL compilation; this does not reduce the independent
eight-texture and sixteen-sampler binding limits.

Strict public render-bundle tests now cover a nonindexed draw and an indexed
textured draw at forced FL9_1. The latter drops its source resource handles
after bundle recording and validates all 64 output pixels after execution. A
two-bundle transition also changes sparse uniform and sampled-resource groups
between recorded bundles and validates both outputs after the original handles
are dropped. Another test selects two dynamic uniform ranges from one buffer
inside consecutive bundles and verifies both outputs after source destruction.
Another transition switches between different uniform-only and
texture/sampler-only pipeline layouts and validates both outputs after source
destruction. A writable D16 bundle attachment test verifies that a later far
draw fails depth across all 64 pixels. One bundle can also execute twice while
the pass changes its scissor rectangle and blend constant; strict FL9_1
readback verifies both regions and confirms pass-level state survives bundle
execution. Colorless D16 bundles execute in depth-only passes, and a read-only
D16 bundle executes in a read-only depth pass while a later depth probe proves
the stored depth was preserved. Core rejects color-only execution of the
colorless bundle and rejects a writable bundle in the read-only pass. Strict
public GPU tests now also cover a read-only D24S8 stencil bundle and a
fractional-viewport render bundle; the stencil comparison and viewport remap
were both verified through readback at forced FL9_1.
The public stencil-reference bundle test also exposed a Core replay mismatch:
render-bundle pipeline changes did not restore the render pass's dynamic
stencil reference. Core now reapplies it after each bundle pipeline that uses
stencil reference state; forced-FL9_1 comparison draws verify separate red and
green regions.

Some detailed status bullets below preserve earlier checkpoints. Later
implementation checkpoints supersede their earlier surface and exposure
statements; the current contract is summarized above and in
[CAPABILITIES.md](CAPABILITIES.md).

The strict public integration test uses ordinary adapter enumeration and
validated wgpu APIs for uniforms at group 0, a sampler at group 4 and a
sampled texture at group 7, the highest position in an eight-group pipeline
layout. It intentionally reuses binding numbers 3 and 17 across groups. A
subsequent draw replaces only the group-7 texture while retaining the uniform
and sampler groups; readback changes from the original red/green pattern to
32 blue and 32 tinted-yellow pixels. The draw includes
vertex- and fragment-visible dynamic uniforms, indexed geometry, D24S8 depth
testing, submission and full 8×8 readback. On the RTX 3090 Ti forced to FL9_1, it
verified 32 tinted-red and 32 green pixels. A queue write updated an aligned
dynamic-uniform range and a second submission selected it with a dynamic
offset, shifting the image to 16 blue, 32 tinted-red and 16 green pixels. The
test then submitted independent clear/copy/readback workloads concurrently
through two public devices from the same adapter; readback verified all 64
pixels were red on one device and green on the other. This proves these paths
through the public API, not every operation within the projection. Error-scope
checks also confirmed that core rejects unreported 16x MSAA, `R32Float` sampling,
multi-mip D24S8, vertex-visible storage-buffer layouts and `D2Array`
sampled-texture layouts before the HAL. A separate public draw overlays
near/far surfaces in both draw orders; all 64 readback pixels select the nearer
surface, exercising depth comparison and writes through public wgpu. A second
strict public test uses the projected R8Unorm sampled/upload format and
checks 32 pixels at each of two channel values. The primary public draw also
initializes vertex data through a mapped-at-creation range and reads back the
result after unmapping. A third strict public test exercises symmetric stencil
replace/equality on D24S8 and verifies all 64 pixels are green; an asymmetric
front/back descriptor is rejected by core before the HAL. A fourth strict test
verifies premultiplied blending produces 64 `[128, 0, 128, 255]` pixels and
that separate-alpha and source-alpha-saturated blend descriptors are rejected
before HAL. A fifth strict test verifies a 4×6 scissor through public wgpu
(24 green and 40 blue pixels) and core rejection of an out-of-target viewport.
A strict public sparse multi-texture test places two independent
texture/sampler pairs at sparse bindings 7/100 and 42/17 in groups 1/4 and
6/7. Nearest and linear sampling of patterned textures both contribute to the
result. The test replaces only sampler group 7 between draws, retaining its
pipeline, textures and other sampler; all 128 readback pixels match the two
expected filtering results. An incomplete texture group is rejected by core
validation.
A seventh strict test samples eight independent textures at sparse bindings,
verifies every contribution in all 64 output pixels, and confirms a ninth
fragment texture is rejected at core validation.
An eighth strict test verifies a padded, offset buffer-to-texture upload,
partial copy between differently sized textures with nonzero origins, and
full texture readback; all 36 destination texels match the expected patch and
preserved background.
A strict test uploads and samples a four-level RGBA8 mip chain through implicit
derivatives and literal sample biases -3, +1 and +3, including a +1 bias
forwarded through a helper. The expected mip colors were verified across 384
pixels. Core rejects dynamic, out-of-range and offset bias forms before HAL,
and validates each helper call site independently. A separate R8_UNORM test
query-gates mip-chain creation and verifies the sampled mip through readback.
A strict test uploads and samples Rg8Snorm and Rgba8Snorm through public wgpu,
verifies signed channel conversion in all 64 pixels, and checks filterability
from the selected `SHADER_SAMPLE` query. Rg8Snorm and Rgba8Snorm mip chains are
separately exposed only after the corresponding selected `TEXTURE2D | MIP`
query and backend path are available.
An eleventh strict test verifies emulated per-instance vertex streams and a
nonzero first-instance range (20 green, 20 yellow and 472 blue pixels). A later
strict test verifies WGSL `instance_index` through the host-updated FL9_1
constant path; strict tests now verify TriangleList, PointList, LineList and
LineStrip `vertex_index` emulation, including signed-base and wide-span restart
handling.

A twelfth strict public test verifies the FL9_1 aggregate fragment uniform
budget across two sparse bindings: all 32 float4 vectors contribute to the
rendered result, and core rejects a 33-vector pipeline before HAL. The selected
device reports separate 255-vector vertex and 32-vector fragment limits; core
counts the complete layout footprint of each statically used uniform in the
selected entry point, which is conservative for partially read structures.
The public uniform binding-size limit now allows 4,080 bytes, matching the
255-vector vertex budget. A thirteenth strict test binds that full range, reads
c254 and c31, verifies geometry changes across two draws, and has core reject a
256-vector vertex pipeline. The per-stage fragment limit remains 32 vectors.
The strict sRGB test also exercises both public formats; full suite totals are
recorded in the current checkpoint below.

An additional strict public test now checks the selected adapter's usage and
filter reports for RGBA8_SRGB and BGRA8_SRGB, samples uploaded texels and
checks sRGB decoding across 128 pixels, then renders linear colors into both
sRGB targets and checks 128 encoded pixels and channel order. The complete
public D3D11 test binary passes 19/19 on the RTX at requested and actual
FL9_1.

The FL9_1 projection exposes `max_texture_sample_bias = 3` and disables dynamic
bias and texel offsets. Core's per-entry analysis follows helper calls, so
unsupported bias forms produce public validation errors instead of HAL
compiler errors. General fragment derivatives remain disabled; selected-entry
`dpdx`, `dpdy` and derivative-based sampling are rejected because FXC's level-
9_1 profile cannot express them. A separately gated direct `textureSampleGrad`
case is supported only for a single-mip 2D float view and a regular sampler
with matching min/mag filters and anisotropy one. Its explicit gradients
cannot affect the sole exposed mip or the shared spatial filter, so Core and HAL
validate this limited semantic equivalence against the live draw bindings. The
default downlevel limit remains false. `wgpu-info` prints the distinct limits;
synthetic tests cover helper/control-flow rejection and entry-point isolation.

General `textureSampleLevel` remains restricted. A separate narrow
`texture_sample_level_zero_2d_float_fragment` limit allows direct,
unconditional fragment `textureSampleLevel(texture_2d<f32>, sampler, uv, lod)`
with a finite literal or a side-effect-free scalar expression formed from
fragment inputs and whitelisted arithmetic. It also allows one straight-line
helper with a literal-zero LOD and validated coordinate arithmetic, or a
whitelisted computed LOD with the same bounded coordinate-expression subset.
Naga validates and lowers the proven operation to
implicit sampling; because the bound view contains one mip, minification and
magnification filters match, and anisotropy is one, the explicit LOD cannot
change the selected mip or spatial filter. Core checks the currently bound
view and sampler at draw time, including render-bundle draws. Nested helpers,
control flow, unsupported expression forms, other resource shapes and
incompatible bindings remain rejected. The generic
`texture_sample_level` limit stays false. FXC's X4532 diagnostic remains for
unlowered explicit LOD. Strict public readback checked 64/64 pixels for both
direct literal-zero, direct computed-LOD, literal-zero helper and computed-LOD
helper forms at requested/actual FL9_1; invalid helper shapes and incompatible
bindings are rejected before HAL.

WGSL's general `texture_gather` limit remains false at FL9_1. A separate
`texture_gather_2d_float_fragment` limit enables one fragment gather from a
single-mip `Rgba8Unorm` or query-supported `Rgba8UnormSrgb` 2D float view using
a regular sampler whose U and V modes are both clamp-to-edge. The selected
entry may gather directly with bounded side-effect-free `vec2f` arithmetic or
pass its unchanged location-0 coordinate to one straight-line helper whose
gather coordinate uses the same bounded expression grammar. Nested/repeated
helpers, transformed helper-call arguments, other image operations and
unsupported shader shapes remain rejected.
Because FXC's level-9_1 profile rejects HLSL Gather, Naga lowers the proven
operation to four point samples at texel centers and assembles the selected
WGSL channel in the specified component order. The emulation uses reserved
texture slot t0, constant buffer b13 and point sampler s15; the selected
texture dimensions are supplied by HAL. Core validates the currently bound
view and sampler before direct draws and render-bundle draws. The strict public
test on the physical RTX 3090 Ti at requested/actual FL9_1 verified all 64
linear-view pixels as `[151, 166, 106, 91]` within one UNORM step and all 64
sRGB-view pixels as `[79, 97, 37, 27]` within two UNORM steps. Core rejected
an R8 view and a multi-mip view before HAL submission. The direct and helper
forms each matched all 64 linear-view readback pixels; the sRGB direct form
matched all 64 decoded pixels. Core tracks the actual bound texture-view format because this narrow format restriction cannot be
represented by the downlevel limit itself. This is a
backend-emulated subset, not native gather support or historical-driver
evidence.

Texture image queries are also modeled separately. FXC rejects Naga's
`GetDimensions` output for `textureDimensions` and `textureNumLevels` with
X3548 in both tested 9_1 and 9_3 pixel profiles. The selected-entry
`texture_query` limit rejects direct and helper-reached queries before HAL;
unused query entries do not block safe siblings. The FL9_1 public projection
reports this limit unavailable, and `wgpu-info` displays it.

The projection now has separate narrow limits for fragment `textureDimensions`
and `textureNumLevels` on one non-array `texture_2d<f32>`. Direct dimensions
queries and their selected-helper form may use any supported bind group.
The queried `textureNumLevels` image may use any supported bind group and uses
the bound view's host-known mip count in private constant `b13`, including
supported multi-mip views. Its optional implicit sample still requires a
group-0 sampler. Neither limit enables general image queries; the proofs retain their
selected-entry and resource-shape restrictions. Strict public readback and
compiler validation are recorded in `TESTING.md`.

`textureSampleBaseClampToEdge` is implemented only through the narrow
`texture_sample_base_clamp_to_edge_2d_float_fragment` limit; the general
downlevel limit remains false. Naga supplies selected-view dimensions through host constant
`b13`, clamps coordinates to half-texel centers and emits implicit `Sample`,
which FXC accepts for `ps_4_0_level_9_1`. Core's additional narrow limit allows
one 2D float texture and one non-comparison sampler from any supported bind
groups, including separate groups, either directly or through exactly one
reachable straight-line helper called once with a `vec2f` and returning
`vec4f`. The helper cannot accept resource handles; selected extra resources,
other image operations and control flow remain unsupported. HAL revalidates
the selected single-mip view at draw preparation and refreshes the dimensions.
Strict RTX 3090 Ti readback verified all 64 pixels with the texture in group 1
and sampler in group 2 at requested/actual FL9_1. The public finite
repeat-per-primitive limit rejects Repeat and
MirrorRepeat sampler descriptors, so independence from those sampler modes is
not established by the public GPU test.

`wgpu/examples/dx11_fl91_offscreen.rs` is a runnable public-API example that
selects the physical FL9_1 adapter, renders an indexed textured 3D cube with
D24S8 depth testing and validates all 1,024 offscreen pixels. Strict execution
on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) reported 112 red,
277 green and 635 blue clear pixels. The public HWND surface and bounded
windowed example are also implemented; see the presentation checkpoint below.

Core gates multi-mip depth/stencil attachment textures on
`DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP`. Microsoft's [10Level9 device
rules](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
require a single mip for depth/stencil-bound resources at FL9_1 through FL9_3;
the controlled FL9_1 core fixture verifies the descriptor is rejected before
the private HAL. D16_UNORM and D24S8 remain single-mip in the private implementation; the
public projection reports that format restriction.

Per-format `SINGLE_MIP_ONLY` metadata is also projected through
`TextureFormatFeatures`, and core rejects larger mip chains before HAL. The
R8G8_UNORM, D16_UNORM and D24S8 remain single-mip in the private
implementation. BC1/2/3 variants additionally remain single-mip when their
selected format query lacks `TEXTURE2D | SHADER_SAMPLE | MIP` or the backend
mip path is disabled. R16G16_SNORM, R8G8_SNORM and R8G8B8A8_SNORM support
sampled/upload mip chains only when the selected device
  reports `TEXTURE2D | MIP`; the independent `SHADER_SAMPLE` query controls
  filterability. Same-format GPU texture copies are separately available for
  RGBA8_SNORM when its selected `TEXTURE2D` query and backend path are positive.
  Render-target use remains unsupported; texture-to-buffer readback uses a
  separately gated ordered raw-byte shadow. RGBA8 and BGRA8, including their
sRGB variants, are also reported
single-mip if the selected driver's format query lacks `MIP` or any required
backend mip path is unavailable. The controlled fixture asserts this
projection against the actual format-query result. This closes the POT
multi-mip gap that device-wide NPOT and depth-attachment flags cannot express.
R8_UNORM also has a selected-query-gated sampled/upload mip path; it requires
the hardware/backend path and `TEXTURE2D | MIP`, with positive unrestricted
NPOT support for non-power-of-two chains.

RGBA8_SRGB and BGRA8_SRGB now follow the linear RGBA8/BGRA8 upload, sampled
view, render-target and readback paths, with independent FL9_1 guarantees and
per-device format queries. A strict public test verifies sRGB decoding from
both upload formats and sRGB encoding to both render-target formats across
128 sampled and 128 rendered pixels. A dedicated sRGB mip-chain execution test
also uploads four-level POT chains for both formats and verifies that implicit
LOD sampling selects mip 1 in all 128 readback pixels. The D16_UNORM public
test reports only render-attachment/transient usage and SINGLE_MIP_ONLY. It
verifies near geometry wins over far geometry in both draw orders, a
`DepthOnly` view is accepted while `StencilOnly` is rejected, LOAD preserves
depth between passes and CLEAR resets it. After a depth store discard, a later
LOAD also does not expose stale depth to rendering. Readback checks 128 pixels
for draw order and another 192 for pass transitions. Mip reporting remains
conditional on the selected format query and the existing POT mip
implementation.

The public D24S8 transition test verifies that a second pass loading both
aspects preserves the initialized depth and stencil values. Separate
depth-only and stencil-only CLEAR cases prove each aspect changes the later
comparison independently. It also discards each aspect separately, loads it
again, and verifies core initializes it before use; all 320 checked pixels
matched the expected results. The selected RTX's FL9_1 D24S8 path passed these
cases. The D24S8 format report now also exposes sampled usage after a positive
shadow query, and the public comparison test verifies the depth-only sampled
view. Core continues to reject aspect-specific D24S8 render-attachment views
while retaining the combined attachment view. These results do
not establish behavior on historical FL9_1 drivers or native post-discard
contents.

The complete strict private HAL suite passes 347/347, the controlled core
fixture suite passes 89/89, core's explicit physical-adapter enumeration test
passes 1/1, and the DX12 core regression suite passes 85/85. The expanded
public suite passes 19/19. `cargo check` for
the D3D11 HAL, targeted Clippy, changed-file rustfmt and `git diff --check`
also pass. HAL Clippy reports nine existing warnings in unrelated mapped
range tests, swapchain ordering and texel-load proof code.

The internal per-format projection now resides in `Capabilities` and combines
the actual device's format query with the selected feature level and implemented
backend paths. Both enumerated adapters and the test fixture call this
projection. The public adapter is restricted to offscreen use plus the tested
BGRA8 sRGB HWND surface path described below.

Core's controlled FL9_1 capability snapshot now also marks
`DownlevelLimits::depth_bias_with_fragment_depth` unavailable. A pipeline with
nonzero depth bias and a selected fragment entry point that writes
`frag_depth` is rejected before HAL; selecting a different entry point from the
same module remains allowed. The capability is represented separately from
the native `DEPTH_BIAS_CLAMP` flag because this is a backend implementation
restriction, not a feature-level guarantee.

An independent selected-entry limit now rejects zero-bias `frag_depth` output
without `@location(0)`, matching FXC X4530. The structured color-plus-depth
entry remains accepted and compiles. The limit checks specifically for target
0; unrelated color locations do not satisfy the 10Level9 profile requirement.
A strict public FL9_1 rendering test now proves the structured output affects
depth testing: the first draw writes fragment depth 0.2 despite vertex depth
0.8, then the second draw's fragment depth 0.8 fails despite vertex depth 0.2.
All 64 pixels retain the first draw's red output.

The controlled FL9_1 core fixture reports
`DownlevelLimits::fragment_front_facing = false` and
`DownlevelLimits::fragment_position = false`. The tested `ps_4_0_level_9_1`
FXC profile rejects `@builtin(front_facing)`, even when its declared value is
unused, so core rejects selected entries that declare it. Microsoft documents
that 9_x pixel shaders cannot read `SV_POSITION`; core rejects selected
entries that use fragment `@builtin(position)` and permits unused declarations.
Both gates leave safe sibling entries in the same module usable. Fragment
position is unavailable in the 9_x shader model; front-facing remains a tested
compiler/backend limitation, not a separate claim about hardware
impossibility.

The selected FL9_1 projection enables `DownlevelLimits::vertex_index_input` for
a constrained host-stream emulation. FXC rejects native `SV_VertexID` with
X4555, so Naga emits a custom float IA semantic and command replay supplies
identity or temporary remapping streams. This path supports TriangleList,
PointList, LineList, LineStrip and TriangleStrip, and
`DownlevelLimits::vertex_index_topology_mask` limits core validation to those
topologies; the HAL retains its own defensive check. Core permits direct
`u32`-to-`f32` use; integer arithmetic remains rejected. `instance_index` is
emulated through reserved vertex constant b13, updated to
`first_instance + instance` before each draw. Neither emulation is proof of
compatibility with historical FL9_1 drivers.

Strict RTX tests at requested/actual FL9_1 cover both builtins. The latest
vertex-index public readback checks normal and degenerate draws, negative base
vertex, reindexed wide indices, `first_vertex=100000`, and state after the
remapping draw. A PointList regression verifies direct IDs and repeated
indexed IDs through full-target readback. See `TESTING.md` for the exact command
and results.
Core also models the vertex-stage sampled-texture limit separately from the
shared per-stage texture limit. The controlled FL9_1 fixture reports zero and
checks the selected shader entry point before pipeline creation reaches HAL;
the strict fixture confirms the rejection is a core validation error. This
prevents unsupported D3D11 vertex texture access from becoming a HAL device
error if a future adapter reports the current private shader subset.

The selected-entry shader audit keeps general `DownlevelLimits::texture_load`
false while exposing the narrow `texture_load_2d_float_fragment` projection
only through the experimental D3D11 backend, which remains excluded from the
default backend set. Core accepts up to eight directly referenced selected
fragment 2D float load identities from single-mip views; the selected entry
and reachable straight-line helpers cannot pass resource handles, use control
flow or access other bound resource classes. One user uniform may coexist. A
direct `@location(0) vec2f` to `vec2i` coordinate is accepted only through the
separately proved forms; other runtime integer operations remain rejected. A
single-image 3D subset remains separate. Naga and HAL use compact `t0`–`t7`,
internal `s15`, and one size vector per image in `b13`. The hardware pixel
profile still limits each compiled shader to 64 arithmetic instructions.
Unreachable helpers and sibling entry points do not constrain the selected
shader. Strict readback covers in-bounds, positive/negative out-of-bounds and
nonzero-mip behavior, distinct two- and eight-image output, uniform coexistence
and safe sibling entry points. This remains an experimental subset, not
general `textureLoad` support.

Core now also applies the FL9_x conditional-NPOT addressing rule to the actual
texture/sampler pairs used by the selected render-pipeline entry points. Naga's
validated sampling relationships are retained on the pipeline, while bind
groups retain the current view dimensions and sampler address modes. Repeat
addressing of an NPOT texture is rejected during command encoding unless the
adapter positively reports unrestricted NPOT support; clamp and mirrored
repeat do not trigger this D3D11 `WRAP` restriction. The strict RTX 3090 Ti
core fixture reports FL9_1's finite repeat limit of 128 and verifies that
Repeat and MirrorRepeat sampler creation are rejected; it renders POT and
NPOT clamp sampling. Private HAL tests and a synthetic core unit test retain
coverage of selected-pair conditional-NPOT behavior where repeat samplers are
permitted. The driver itself reports unrestricted NPOT support at forced
FL9_1, so the restrictive optional-capability behavior is verified
synthetically.

Core also models the fixed 10Level9 maximum texture-repeat count per primitive:
128 at FL9_1, 2048 at FL9_2, 8192 at FL9_3/10_x and 16384 at FL11_x. Since
wgpu-core cannot prove the interpolated coordinate range for every draw, sampler
creation rejects `Repeat` and `MirrorRepeat` whenever this limit is finite.
This is independent of conditional-NPOT validation: clamp and mirror avoid the
NPOT `WRAP` rule, but mirror still repeats and is constrained by the draw limit.
The RTX forced to FL9_1 does not simulate the historical driver behavior; this
contract is derived from Microsoft's static 10Level9 requirements.

Vertex-format support is now reported as a per-format `DownlevelLimits` mask.
For the FL9_1 private fixture, the mask intersects the feature-level minimum,
the selected driver's `IA_VERTEX_BUFFER` query and the formats actually mapped
by the HAL. Core rejects unreported attributes before HAL. The private input
layout now maps `Uint8x4`, `Sint16x2` and `Sint16x4` in addition to its previous
formats; direct integer-vector-to-float conversion is the only newly accepted
runtime integer shader operation. The forced-FL9_1 RTX test exercises all three
formats in one draw and validates every pixel. The public selected-device
projection reports this mask; other layout and draw restrictions remain.

The public selected-FL9_1 projection continues to report general stores through
aggregates containing two-row matrices as unavailable. The bounded Core and HAL
proofs allow one narrow exception: direct, statically indexed column or scalar
component writes to an ordinary `matCx2<f32>` member of a function-local
struct. Naga lowers this exact shape to direct writes of its split HLSL column
members. Whole-matrix stores, dynamic indices, helper-mediated pointer writes,
nested or array-contained matrices, and private globals remain rejected. Core
follows helper calls and evaluates only the selected entry point. Strict public
validation and readback verify the allowed case; CPU analysis tests retain the
negative cases and safe sibling-entry behavior. These are compiler/backend
restrictions, not hardware guarantees.

Core now also models whether viewport rectangles may extend beyond the active
render target with `DownlevelLimits::viewport_rectangles_outside_target`. The
controlled FL9_1 fixture reports false and verifies that left-overflowing
rectangles fail command validation before HAL. The private HAL still supports
clipping when its optional remapped vertex shader compiles, but that variant
can exceed FL9_1 instruction limits, so the core path conservatively rejects
all out-of-target rectangles. Other backends keep the default true capability.

## Implemented private HAL paths

- DXGI adapter discovery, D3D11 device/context ownership, and restricted
  BGRA8 FIFO surfaces for WinRT CoreWindow and borrowed Win32 HWND handles.
  CoreWindow construction verifies `Dispatcher.HasThreadAccess` and later
  surface operations are restricted to the captured owner thread; this is a
  safety check, not CoreWindow runtime verification. Public CoreWindow surface
  capabilities are exposed for successfully constructed CoreWindow objects;
  real UWP/Lumia presentation remains unverified.
  A strict hidden HWND clone-sampling test on the RTX at FL9_1 read BGRA
  `[0, 0, 255, 255]`, returned `Ok` from present and resized with the completed
  command buffer retained. Public hidden-HWND presentation, resize and
  cross-thread waiting are tested. Surface operation reservations track their
  owner thread; same-thread reentrant `unconfigure` logs and returns without
  waiting, while cross-thread calls still wait. Native callback delivery and
  CoreWindow/ARM32 execution remain unverified. Both the HAL and public wgpu
  DX11 feature set cross-check for `thumbv7a-uwp-windows-msvc` using nightly
  `build-std`; the Lumia runtime remains unavailable.
- Buffer copies, initialization clears, fences, queue submission, and
  explicitly synchronized mapped CPU shadows, including transient staging.
  Core-internal storage usage bits can accompany a supported transfer buffer,
  but no storage-buffer shader binding is implemented. Core requires the
  relevant stage downlevel flags and the controlled fixture confirms unsupported
  vertex, fragment and compute storage layouts are rejected before HAL.
- Texture resources and views include queried, sampled-only RGBA8 3D textures,
  but 1D textures remain unavailable. The internal projection retains
  inherited numeric 1D/3D limits of 2048/256; core rejects 1D dimensions and
  gates the narrow 3D subset before HAL.
  The 1D gate is an FL9_1 API limitation, not an unfinished view path:
  Microsoft's [10Level9 `CreateTexture1D` reference](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device) says the method is not
  supported at any 9.x feature level. The generic 2048-wide limit is not
  evidence that a 1D resource can be created.
  It also requires explicit downlevel flags for 2D-array, cube and cube-array
  binding layouts and views;
  storage-texture array layouts use the same array gate. A one-layer 2D-array
  view is rejected before reaching the HAL. Core also gates NPOT mip chains
  with `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`; D3D11 projects the flag only when
  its NPOT mip path is implemented and the selected FL9_x device positively
  reports full-NPOT support. Texture creation and sampled views apply the same
  query, with per-format MIP support checked independently. A strict public
  forced-FL9_1 test uploads a 6×10 RGBA8 four-mip chain and verifies mip-2
  sampling across 64 output pixels. Negative and unavailable queries remain
  covered synthetically; the RTX reports full NPOT support, so it cannot
  simulate restrictive historical hardware.
- RGBA8 and BGRA8 sampled/color textures, texture views, bounded staging
  transfers, zero initialization and GPU texture-region copies, and point/linear,
  clamp/repeat/mirror samplers with anisotropy
  up to 2. On 9_1/9_2, equal min/mag filters with anisotropy 1 allow
  otherwise-unrepresentable LOD ranges to be normalized for single-mip views;
  other translated finite maximum LODs are checked against each used view's mip
  count. Nearest and linear mipmap filters have patterned GPU coverage;
  explicit-LOD sampling and
  historical driver behavior remain unverified. The current FL9_1 shader
  profiles, including on higher-level devices, accept finite literal texture
  biases in [-3, 3] without an offset. All sampling pairs used by a selected
  entry containing bias require equal min/mag filters and anisotropy 1; dynamic,
  out-of-range and helper-argument biases remain rejected. Nine strict GPU
  executions checked 64 pixels each, including linear mip blending; direct and
  indexed draws rejected incompatible samplers.
- `DownlevelLimits::max_sampler_anisotropy` carries the private FL9_1 maximum of
  2 separately from the anisotropic-filtering availability flag. Core clamps
  descriptor requests to the reported cap before HAL creation; strict core
  hardware coverage creates native samplers for requests 2, 3 and 16. A strict
  private minification test now measures alternating-column contrast of 1 at
  anisotropy 2 versus 128 at anisotropy 1 on the RTX 3090 Ti forced to FL9_1.
  Historical-driver behavior remains unverified.
- R8_UNORM is supported for single-layer 2D sampled textures with RESOURCE
  and/or COPY_DST usage. Full mip chains require a positive selected-device
  `TEXTURE2D | MIP` query and the implemented backend path; NPOT chains also
  require positive unrestricted-NPOT support.
  A strict forced-FL9_1 test verifies a padded upload from a nonzero buffer
  offset and all 80 sampled output pixels. A positive `SHADER_SAMPLE` query
  reports `FILTERABLE`; when the bit is absent but `TEXTURE2D` is present, the
  backend supports point-only sampling. The selected-device
  `RENDER_TARGET` query also enables a single-mip color-attachment path; it is
  blendable only when the selected query also reports `BLENDABLE`; copy-source
  and direct readback are controlled separately.
- R8G8_UNORM is not a required FL9_1 sampled format. Its single-layer
  sampled/upload path is exposed only when the selected device reports
  `TEXTURE2D`; sampled mip chains additionally require `MIP` and the backend
  mip path. The forced-FL9_1 RTX returned
  `0x0002c320`; a strict public upload/sample test verified 64 pixels and the
  two-byte texel stride. A second pass verifies linear filtering with an
  eight-column gradient. The selected-device `RENDER_TARGET` query also
  gates single-mip attachment use. Blendability, copy-source and direct
  readback are separately selected-query-gated.
  The RTX query mask lacks `MIP`, so its strict public case skips RG8_UNORM
  mip execution; capability tests cover the positive and negative query paths.
- R8G8_SNORM's one-mip sampled/upload subset remains available when the
  selected device reports `TEXTURE2D`. A separate sampled/upload mip-chain
  path requires positive `TEXTURE2D | MIP` support. Strict
  forced-FL9_1 tests verify padded signed two-channel data and a three-level
  chain through GPU sampling. Same-format texture copies remain disabled: on
  the forced-FL9_1 RTX 3090 Ti, a full-resource `CopySubresourceRegion` with a valid source box and a diagnostic `CopyResource` both caused device removal with `E_INVALIDARG`. This query result alone is not
  enough to advertise copy support. Render-target use and CPU readback also
  remain unsupported, and the readback mask remains clear.
- R16G16_SNORM is not guaranteed to support shader sampling at FL9_1; the
  required-format table raises `SHADER_SAMPLE` to FL9_2. The selected device
  can still support point-only sampling when it reports `TEXTURE2D`. The RTX
  forced to FL9_1 reports `0x00021122`, without `SHADER_SAMPLE`; strict HAL
  and public tests upload, point-sample and validate readback, while core
  rejects a filtering layout from the queried format flags. Public use also
  requires `TEXTURE_FORMAT_16BIT_NORM`. Multi-mip sampled/upload use requires
  the selected query's `TEXTURE2D | MIP` bits plus the backend path; the
  format remains non-filterable without `SHADER_SAMPLE`. It is not renderable.
  Same-format GPU copies and ordered CPU-shadow readback are separately gated.
- R8G8B8A8_SNORM's base sampled/upload subset remains selected-query-gated.
  Its required FL9_1 format guarantees `TEXTURE2D | MIP | SHADER_SAMPLE`, but
  the implementation also requires the selected device's `TEXTURE2D | MIP`
  query and enabled backend path before exposing mip chains. Strict public
  tests verify padded signed four-channel sampling and three-level biased
  mip sampling. Same-format texture-to-texture copies are enabled only when
  the selected `TEXTURE2D` query and backend path are positive; a strict public
  FL9_1 test uploads, copies, samples and validates the destination. A separate
  gated ordered raw-byte shadow supports CPU readback; render-target use remains
  unsupported.
- BC1/BC2/BC3_UNORM and sRGB have query-gated sampled/upload mip chains when
  `TEXTURE2D | SHADER_SAMPLE | MIP` and the backend path are enabled; cube mip
  chains additionally require `TEXTURECUBE`. Base dimensions are block aligned,
  and partial origins/regions follow block alignment with logical-edge tails.
  Strict forced-FL9_1 public tests uploaded, GPU-copied and sampled all four
  mips for all six variants, including 2×2 and 1×1 logical tails, and verified
  all 64 output pixels per format. A second strict path sampled four mips on
  all six cube faces for each variant and verified all 768 pixels per format.
  Full compressed uploads detach bytes from mapped staging memory before the
  null-box `UpdateSubresource` call; same-sized full mip GPU copies use a null
  source box. Query-gated texture-to-buffer readback remains an ordered CPU
  shadow because native FL9_x staging reads are unavailable. Render-target,
  BC4–BC7 and cross-family/view reinterpretation remain unsupported.
- Copy-only color texture views can retain metadata and texture ownership without
  creating native sampled or attachment views. They cannot be used for sampling
  or rendering; core upload, copy and readback were verified for both formats.
- POT RGBA8 and BGRA8 copy-only textures support multiple initialized mips when
  the device reports the required format bits. Strict RTX FL9_1 tests exercised
  four levels, partial uploads, cross-resource copies, readback, metadata views
  and resource retention; a core fixture checked all 43 mip-chain pixels.
- POT RGBA8 and BGRA8 mip chains also accept logical `COPY_DST` alone. Strict
  RTX tests uploaded three mips and verified an untouched zero mip through a
  test-only native GPU-to-GPU copy to a separate readable render-target texture.
  Core submitted three writes, waited for completion and rejected forbidden
  view/readback uses; it did not read back the upload-only texture. Historical
  FL9 drivers remain unverified; public surfaces remain unavailable.
- Copies between distinct mips of one supported texture use native DEFAULT to
  DEFAULT `CopySubresourceRegion` with an explicit source box. Strict RTX tests
  checked partial copies in both directions with full padded readback for RGBA8
  and BGRA8; render-to-copy-to-clone-sample cases each produced 64 blue pixels.
  Core checked all mip texels for both formats and rejected a same-mip copy.
  The earlier strict baseline rejected a valid distinct-mip copy. Copies within
  one subresource remain unsupported; historical drivers remain unverified.
- Sampled POT mip chains support `COPY_DST | RESOURCE` with optional `COPY_SRC`,
  or `RESOURCE | COLOR_TARGET` with optional copy usages,
  and a positively queried format. Suffix views including the resource's last
  mip use native SRVs; supported non-suffix sampled views use per-draw GPU
  copies into a clone and retain the original for feedback validation. Selected
  shader pairs are checked against actual mip counts and sampler
  LOD translation. RGBA8 and BGRA8
  GPU readbacks and a controlled core render passed; same-pass transitions
  retained sampled resources after caller destruction before submission. Strict
  suffix-view tests passed on the RTX FL9_1 device, including 64 blue pixels
  through the controlled core fixture. Without `COPY_SRC`, core cases sampled
  an uploaded blue mip and an untouched zero mip, each across 64 pixels; direct
  readback from that shader-resource-only texture is unavailable. Sixteen
  strict clone-backed GPU cases passed across both formats, including a recorded
  upload followed by an indexed draw in the same submission; the controlled core
  fixture checked 64 yellow pixels after a source update. The clone incurs
  per-draw copies and storage. NPOT mip chains require a positive full-NPOT
  query and a supported format path; same-subresource copies remain unsupported.
  Native FL9 sampled views must include the smallest resource mip;
  historical GPU behavior is unverified.
- POT RGBA8 and BGRA8 color-attachment mip chains support an individual
  single-mip render-target view, with optional sampled, copy-source and
  copy-destination usage. A dual sampled/attachment view can select any single
  mip; a nonfinal mip uses a clone SRV and original-resource RTV. Transient
  usage remains unsupported. Strict
  GPU readback passed for both formats; core checked all 43 pixels per format
  and submitted a render-only mip pass without direct readback. Combined
  render/sample transitions passed six private GPU cases across both formats and
  four core cases with 64 checked pixels each. An additional eight private GPU
  cases covered nonfinal dual views, and default-view core cases checked 64
  pixels per format. Native suffix and clone-backed SRVs may sample disjoint
  mips of the same texture in the attachment pass; overlapping ranges remain
  rejected. Clone refresh snapshots and unbinds the RTV/DSV, copies, restores
  the attachments and rebinds pixel shader resources. Six strict RTX FL9_1
  native cases and two clone-backed cases passed across both formats; the latter
  and matching controlled core cases each produced 32 green and 32 blue pixels,
  demonstrating depth preservation. [D3D11.3 §5.3.1](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)
  permits disjoint subresource input/output; the [D3D9 sampler documentation](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-registers-sampler)
  warns against simultaneous texture and render-target binding. Historical FL9
  drivers remain unverified, and public surfaces remain unavailable.
- POT RGBA8/BGRA8 mip chains now accept every nonempty combination of
  `RESOURCE`, `COLOR_TARGET`, `COPY_SRC` and `COPY_DST`, gated by the matching
  backend path and queried format bits. Strict FL9_1 tests sample zero-initialized
  `RESOURCE` and `RESOURCE|COPY_SRC` chains into 64 checked pixels each, and copy
  the `COPY_SRC`-only mip-0 subresource through staging for both formats. The
  controlled core fixture creates the three combinations previously rejected by
  HAL. Historical driver behavior remains unverified.
- A restricted D24S8 attachment path provides zero initialization, independent
  depth/stencil clears, depth-only draws and all seven native depth comparisons
  other than Never, with optional writes. A strict FL9_1 test checked both
  passing and failing depths for every comparison across 128 pixels. Never
  retains its existing validated-draw/stencil remapping. Equal front/back
  stencil states map native comparisons,
  operations and references; asymmetric states are rejected because FL9_1 does
  not support two-sided stencil. Core represents this restriction with
  `DownlevelFlags::TWO_SIDED_STENCIL`; the controlled fixture verifies an
  asymmetric pipeline is rejected before the HAL call. Never with active
  stencil uses Always plus remapped depth-fail operations and disabled
  color/depth writes; inert-stencil
  Never still suppresses validated draws. Replace/Equal writes and reference
  transitions are indirectly verified through strict GPU readback. All eight
  stencil operations now pass boundary-value checks when used on native pass,
  stencil-fail and depth-fail branches (24 full-image cases); the DSV is not
  directly readable through the HAL. Private and public read-only passes are
  tested without sampled aliases. The public D24S8 test covers both aspects
  read-only and each mixed read/write aspect combination; core rejects writes
  to whichever aspect is read-only. `READ_ONLY_DEPTH_STENCIL` remains
  unreported, so simultaneous sampled-depth aliasing is unavailable.
  A public colorless D24S8 prepass followed by a color pass also verifies that
  depth written without a color target controls the later comparison.
  Aspect-specific views and depth copies remain unsupported; broad public
  depth-stencil capability is still false.
- A public D16 depth-only pipeline with a result-less fragment stage now
  compiles and runs at forced FL9_1. FXC rejects the native `void` pixel shader
  with X4530, so HAL moves the validated fragment body into a helper and adds
  a synthetic `SV_Target0` return. The DSV-only pass binds no RTV, preserving
  discard and depth behavior while making the color output unobservable. A
  conditional-discard test checked 32 red pixels where depth was not written
  and 32 blue pixels where the later draw failed depth testing.
- D24S8 triangle pipelines accept signed constant and finite slope-scaled depth
  bias with zero clamp. Core's controlled fixture verifies nonzero clamp is
  rejected by `DEPTH_BIAS_CLAMP` before HAL. Nonzero bias on points or solid
  lines, nonzero clamp and bias with a selected fragment-depth output are
  rejected. Five strict FL9_1 RTX
  constant-bias images and two slope-bias images each checked all 64 pixels;
  the controlled core fixture checks positive and negative constant bias. FXC
  rejected a direct-only zero-bias `frag_depth` entry with X4530, while a
  structured color-and-depth zero-bias entry compiled. FL9 historical bias
  precision remains unverified; public surfaces remain unavailable.
- Single-sample pipeline masks reach the D3D11 output merger. Because FL9_1
  rejects a zero native `SampleMask`, a zero wgpu mask is translated to
  `0xffff_fffe`, which masks sample zero while satisfying the native nonzero
  requirement. A strict controlled core render preserved all 64 clear-color
  pixels. Optional 2x/4x/8x RGBA8 render/resolve and matching
  D16/Depth24Plus/D24S8 depth attachments are exposed only after their
  selected-device format and per-count quality queries pass. Depth must match
  the color and pipeline sample counts; depth resolve and multisampled
  sampling remain unsupported. 16x is not available on the tested device.
  Alpha-to-coverage is separately gated by
  `DownlevelLimits::alpha_to_coverage` and rejected by core before HAL.
- Transient color and D24S8 attachments use ordinary D3D11 textures; no
  memoryless allocation or memory saving is claimed. Transient color requires
  color-attachment usage, and transient depth requires depth-write usage; sampled
  and copy usages are rejected for these textures and views. Strict public
  FL9_1 execution covers transient color clear/draw/discard and transient D24S8
  depth/stencil clear/draw/discard controlling a readable color result. Core
  validation rejects transient sampling, invalid LOAD/STORE operations,
  read-only transient depth and render passes with no attachments.
- A strict public `LoadOp::DontCare` test overwrites every color and D16 depth
  pixel before later reads. The near draw writes every depth value, the farther
  draw fails, and all 64 stored color pixels match.
- A strict public RGBA8 pass clears red and uses `StoreOp::Discard`, then copies
  the same texture to a CPU buffer. Core initializes the texture before that
  read, and all 64 pixels are transparent black; discarded red contents are
  not exposed.
- A second strict public pass samples a red-cleared, discarded RGBA8 attachment
  through an SRV after switching it from render-target use. Core initialization
  is visible through sampling: all 64 output pixels are transparent black.
- A strict public follow-up render pass uses `LoadOp::Load` after color discard;
  copying afterward returns transparent black at all 64 pixels, not the old
  red clear.
- A new public RGBA8 attachment used first with `LoadOp::Load`, then stored and
  read back, also returns transparent black at all 64 pixels.
- A strict public command-buffer test drops an attachment texture and view
  after recording but before submission; the queued clear/copy still returns
  blue for all 64 pixels.
- Public core rejects an `INDIRECT` buffer at creation because the selected
  FL9_1 projection omits `INDIRECT_EXECUTION`; unsupported native indirect draw
  methods are unreachable through this public capability contract.
- Public core rejects compute-pipeline creation through its missing
  `COMPUTE_SHADERS` downlevel gate before invoking the unsupported HAL path.
- Public core rejects occlusion and pipeline-statistics query sets. Native
  timestamp/disjoint commands remain available for diagnostics, but public
  timestamp features and query-set creation are rejected because D3D11 only
  guarantees frequency within a disjoint interval while wgpu permits queries
  across submissions. The public queue timestamp period is zero.
- Negative and fractional viewports use a separately compiled vertex shader
  and a reserved constant buffer. FL9_x viewport calls do not preserve
  fractional values. If FXC cannot compile the adjusted variant, pipeline
  creation fails with a linkage error; this also avoids accepting a pipeline
  whose in-target fractional viewport would later fail during submission.
  Core rejects out-of-target rectangles, but fractional rectangles within the
  target remain valid and require the adjusted shader.
- Up to eight groups with twenty-six static uniform bindings total across the
  vertex and fragment stages, eight sampled textures and sixteen samplers in
  total. D3D11 has independent constant-buffer registers for vertex and pixel
  shaders; the allocator supports thirteen uniforms per stage in `b0`–`b12`
  and reserves `b13` per stage for internal emulation. One bind group may hold
  13+13 uniforms when stage visibility is disjoint. Core's separate dynamic
  uniform pipeline-layout limit remains thirteen. The public combined
  buffers-per-stage limit is 29
  (thirteen uniforms plus sixteen vertex input buffers); it is a Core pipeline
  limit rather than a native D3D11 register limit. The standard Core WebGPU
  default of 28 is met for this combined count, but that does not imply other
  default limits or general shader support. Strict FL9_1 public readback
  exercises thirteen sparse fragment-visible dynamic ranges from one source
  buffer and switches their selected offsets between draws; Core rejects
  fourteen dynamic bindings before HAL. Strict FL9_1 rendering also binds
  thirteen sparse vertex and thirteen fragment uniforms in different groups;
  each stage uses `b0`–`b12`, and both sets contribute to all 64 readback pixels.
  Core and HAL accept 13+13 layouts across two groups and in one group when
  visibility is disjoint. A controlled core draw
  also places a fragment uniform in group 0 and sampled resources in group 1;
  this verifies one supported two-group combination, not the entire eight-group
  aggregate limit.
  Shader-aware pair validation and restricted
  Naga/FXC shaders.
- NonFiltering sampler layouts accept samplers with nearest minification,
  magnification and mipmap filtering; Filtering layouts also accept nearest.
  Sparse bindings and sampler class remain part of layout identity. Strict RTX
  tests checked two complete images including a layout transition, and a core
  fixture checked 64 pixels and rejected a linear sampler with
  `WrongSamplerFiltering`. The original strict baseline rejected the layout.
  Existing RGBA8/BGRA8 textures can use an unfilterable-float layout binding when
  each sampling pair used by the selected entry has a NonFiltering sampler.
  Two strict RTX images and a 64-pixel core case covered this subset; invalid
  filtering pairs were rejected, while unused incompatible pairs were allowed.
  No additional unfilterable formats or general comparison samplers are
  supported through core. Core rejects comparison sampler creation when the
  adapter omits `DownlevelFlags::COMPARISON_SAMPLERS`; the controlled FL9_1
  fixture verifies rejection before HAL. When the selected device positively
  reports `D3D11_FEATURE_D3D9_SHADOW_SUPPORT`, the opt-in public projection
  exposes only `LessEqual`, equal min/mag filters, anisotropy 1 and single-
  sample D16/Depth24Plus/D24S8 sampled usage. Strict public renders for all
  three formats each validate 64 pixels. Core rejects unsupported functions,
  filter mismatch and anisotropy
  are rejected before HAL. Historical drivers remain unverified; public
  surface capabilities remain unavailable.
  Default views of textures created with both depth-attachment and sampled
  usage now retain both native view roles. The same D16 and Depth24Plus default
  view was used sequentially for attachment and sampled comparison; D24S8
  sampling still uses a `DepthOnly` view. Strict public readback covers all
  three formats.
- Sampled texture and sampler bindings accept `FRAGMENT` and
  `VERTEX | FRAGMENT` visibility, retained per binding in layout identity.
  Actual vertex-stage sampling remains rejected. Two strict RTX images covered
  fragment sampling and a layout transition; core checked 64 pixels. CPU tests
  cover stage mapping and selected-entry compiler guards. The baseline rejected
  the combined layout; historical FL9 drivers remain unverified.
- Short uniform ranges use a zero-padded native constant buffer and preserve the
  declared byte range, including non-multiples of 16. A strict public test now
  verifies a 4-byte range at offset 256 through the ordinary wgpu projection,
  rendering 64 expected pixels at FL9_1. This stages through a CPU map and can
  stall; odd-sized GPU copies remain unverified on historical drivers. Runtime
  integer expressions and integer-containing uniforms or
  interfaces are conservatively rejected for selected FL9 entries in both
  checked and unchecked modes, except the separately proved vertex built-ins
  and narrow direct fragment uniform-array indexing documented below. Earlier
  RTX integer-shader results are superseded by explicit rejection tests with
  float-only GPU controls.
- The forced-FL9_1 private shader path executes static WGSL reads of vertex
  constant c254 and pixel constant c31. Changing the vertex value moves the
  triangle and changing the pixel value changes its color; readback distinguishes
  17 baseline green pixels and 4 shifted red pixels. General runtime
  integer-indexed uniform arrays remain rejected outside the narrowly proved
  vertex and fragment subsets described below.
- Triangle-list and nonindexed triangle-strip passes with up to sixteen sparse
  vertex- or instance-rate streams, winding and face
  culling, bounds-checked scissors, per-channel color write masks, translated
  single-target blend factors/operations and splitting at FL9 draw limits.
  Runtime integer-based vector indexing is rejected in both
  shader modes; proven static literal indices remain supported. Stores through aggregates
  containing two-row matrices are conservatively rejected in both shader modes.
- The private vertex-input subset includes `Snorm16x2` and `Snorm16x4`, with
  driver-checked input-assembler support and four-byte attribute alignment.
  This does not add signed-normalized texture formats.
- Zero-stride vertex streams repeat a constant attribute element in direct,
  nonindexed strip and indexed triangle-list paths, including mixed streams and
  window or gather translation. A bound range must contain the full attribute
  extent. In the tested indexed-point path, negative base-adjusted indices
  remain valid when every vertex-rate stream has zero stride: the WebGPU
  vertex-fetch formula adds `vertexIndex * arrayStride`, so the bound element
  remains at the same in-range offset. The strict Uint16/Uint32 point test
  verifies visible output for this case. Historical FL9_1 devices remain
  unverified.
- Exact premultiplied-alpha blending uses coupled `ONE` / `INV_SRC_ALPHA` add
  factors for RGB and alpha, including all color-write masks. Strict FL9_1
  tests checked six full images on linear RGBA8/BGRA8, including transitions;
  the controlled core fixture checked 64 pixels with a one-unit rounding
  tolerance. The unsafe fixture reports blendability only on a positive driver
  format query and cannot represent the restricted blend-state subset.
  sRGB formats and broader public use remain unsupported. Historical
  FL9_1 driver behavior remains unverified.
- The Microsoft 10Level9 feature table marks Separate Alpha Blend unavailable
  at FL9_1, with no selected-device query to promote it. FL9_1 rejects straight
  alpha, inversion and vignette blending when both RGB and alpha are written.
  Core requires `DownlevelFlags::SEPARATE_ALPHA_BLEND` only when the effective
  alpha-channel equations differ and both RGB and alpha are written. Equivalent
  factor spellings such as `Src` and `SrcAlpha` are accepted and normalized by
  HAL; RGB-only and alpha-only cases remain accepted. True separate-alpha
  equations still require the absent flag. A strict public FL9_1 render checks
  an accepted equivalent form across all 64 pixels. A test-only opt-in retains
  nonportable separate-alpha diagnostics.
- Standard blend factors and all five blend operations now translate through
  one path, including per-channel constant blend factors. `SRC_ALPHA_SAT` and
  dual-source factors remain rejected for FL9_1. Command replay retains blend
  factor state across pipeline binds and updates it when `set_blend_constants`
  is recorded. The D3D11 blend descriptor mirrors its first target into the
  first four native slots to satisfy the FL9_1 `BlendEnable` rule while the
  backend binds only one render target. A forced-FL9_1 hardware draw exercised
  constant and inverse-constant factors with two factor values plus Add,
  Subtract, ReverseSubtract, Min and Max; all 64 readback pixels matched. The
  controlled core fixture matched the custom constant-add output across 64
  pixels. Other factor mappings are CPU-tested, not each independently
  rendered. This still does not establish historical-device behavior or public
  adapter safety.
- The 10Level9 device contract excludes `D3D11_BLEND_SRC_ALPHA_SAT` at FL9_1,
  9_2 and 9_3. `DownlevelLimits::src_alpha_saturated_blend` records this
  capability separately from format blendability and separate-alpha equations.
  Core rejects the factor in either color or alpha source/destination slots
  before the HAL is called. The FL9_1 fixture checks all four slots on the
  RTX 3090 Ti. The HAL still does not implement this factor at higher levels.
- Nonindexed point-list draws support the restricted vertex- and instance-rate
  streams, including sparse slots, nonzero offsets and `first_instance`. FL9_1
  point batches rebase vertex bindings and contain at most 65,535 points.
  Strict RTX tests checked three full images, including 65,537 points across a
  batch boundary. A public strict draw checks 65,536 points and a separate
  65,538-vertex triangle list, placing the only visible primitive in each
  second batch; readback verifies both. A controlled core fixture checked 64
  pixels. Indexed Uint16 and Uint32 points use ordered
  nonindexed runs; GPU index staging and CPU
  mapping can stall, and scattered indices can require many draws. When all
  vertex-rate streams have zero stride, or only instance-rate streams are used,
  index values do not affect the output and staging is avoided after range
  validation. Twelve strict RTX images and two 64-pixel core renders covered
  indexed points. Historical FL9_1 drivers remain unverified.
- LineList and LineStrip use explicit D3D11 topologies. Direct list batches
  preserve pairs; direct strip batches overlap their endpoint. Indexed LineList
  accepts Uint16 and Uint32, maps GPU-authoritative indices at replay, preserves
  pairs while compacting bounded windows, and gathers a wide pair into two
  vertices when needed. It limits native calls to 131,070 indices (65,535
  lines), ignores an incomplete tail and discards the draw on any invalid fetch.
  Strict RTX readback checked direct and indexed line topology output in both
  index formats, restart-separated strips, an indexed wide-edge gather and
  out-of-range draw discard. The controlled core fixture checked indexed
  Uint16 and Uint32 LineList and LineStrip output, including markers formed by
  base-vertex adjustment. Indexed LineStrip emulates restart by splitting
  CPU-mapped resolved indices because native FL9 strip cuts are unavailable.
- Nonindexed strips preserve winding through overlapping, rebased batches.
  Indexed LineStrip splits resolved restart markers into compact line strips;
  indexed TriangleStrip expands resolved runs into winding-preserving triangle
  lists and uses bounded Uint16 windows or wide-triangle gathers. Native FL9
  strip-cut support is not assumed. Strict GPU and controlled-core tests cover
  both index formats; historical FL9_1 behavior remains unverified.
- Direct FL9_1 triangle-list batches are conservatively limited to 65,535
  vertices; indexed draw batching retains its separate limit.
- FL9_1 overlapping vertex attributes within one logical stream are assigned
  to separate native input slots that bind the same underlying buffer. This
  emulation is limited to the existing vertex formats and available native
  slots; historical FL9_1 devices remain unverified.
- Sparse multi-stream indexed triangle lists accept Uint16 and Uint32 indices.
  Replay checks GPU-written indices against every stream and discards a whole
  draw on an out-of-bounds fetch. Native-safe Uint16 indices use the original
  buffers; other valid indices use ordered rebased Uint16 windows or gather
  each stream into scratch buffers for wide triangles. Indexed LineStrip splits
  resolved restart markers and preserves connectivity through compact windows
  and wide-edge gathers. Indexed TriangleStrip splits the same resolved markers,
  expands runs to alternating-winding triangle-list indices, then uses bounded
  Uint16 windows or gathers wide triangles across every vertex stream. The
  private and controlled core paths test both index formats; FL9_1 historical
  hardware remains unverified.
- Restricted direct, strip and indexed instancing repeats non-instanced native
  draws, binding each instance-rate stream with native stride zero at its
  `first_instance`-adjusted offset. Indexed LineList and TriangleList draws
  with only instance-rate or no vertex-rate streams validate the index range,
  then replay the same primitive count with nonindexed draws. Indexed LineStrip
  and TriangleStrip draws without vertex-rate inputs read index data at replay,
  split effective restart runs and issue one direct strip draw for each
  bounded run. Strict private HAL and public wgpu readback cover Uint16 and
  Uint32, instance-only and empty layouts. The existing PointList instance-only
  path remains covered separately. Direct supported `instance_index` input is
  emulated through a host-updated constant and has strict public GPU coverage;
  native instancing remains restricted to the tested instance-rate streams
  and entry-point subset.
- Restricted index-dependent draws support signed base vertices and validate GPU
  index contents at replay. Uint32 triangle lists and Uint16 values beyond native
  limits convert to ordered native 16-bit windows, with wide triangles gathered
  through GPU vertex-buffer copies. Out-of-bounds vertex fetches discard the
  whole draw. Staging and scratch
  allocation can stall the CPU; this is not a full public draw contract.
- An original two-texture overlay shader passed RTX diagnostics with alpha and
  inversion blend states; full-channel separate alpha is not a guaranteed
  FL9_1 production configuration.
- A local, deliberately unsafe core bootstrap experiment with placeholder
  capabilities created a wgpu device, copied mapped buffers, and rendered an
  indexed triangle on FL9_1 with pixel readback. It is not an adapter or a
  capability-conformance test.

The selected-device projection now supplies conservative limits, alignments,
downlevel gates, features and per-format reports to both ordinary adapter
enumeration and the controlled core fixture. It exposes the narrow BC1/2/3
unorm feature when all three queried sampled-upload paths are available, while
withholding the aggregate BC feature because BC4 through BC7 remain
unimplemented.

## Remaining public contract and completion gates

- The public adapter is FL9_1-only. The public integration
test verifies adapter selection, device/queue creation, sparse multi-group
texture/sampler and stage-visible dynamic uniform bindings, indexed textured
depth-tested rendering, transform updates, concurrent device submissions and
pixel readback. The public test is still a bounded diagnostic scene, not a
representative application or a complete contract audit.
- The public HWND and validated CoreWindow surface paths report BGRA8 sRGB
  FIFO, opaque composition, render-attachment usage and one frame of latency.
  CoreWindow construction verifies dispatcher access; configure, acquire and
  present are restricted to the creating thread. A queue
  gate serializes submission, writes, mapping flushes, polling and
  context-backed resource creation against Present; same-thread reentrant work
  is rejected before queue or mapping state changes. The strict public test
  configures, clears, presents, resizes and presents again, then confirms
  wrong-thread and duplicate acquisitions return validation errors without
  losing the surface or consuming the first outstanding frame. It uses a
  hidden window; the separate visible example exercises the message pump and
  desktop presentation. Lumia/UWP CoreWindow presentation has not been run.
- `wgpu/examples/dx11_fl91_windowed.rs` is a visible public-API example. It
  pumps Win32 messages, handles resize, and presents an animated indexed,
  textured, depth-tested cube. It uses a transform uniform in group 0, an
  independent material tint uniform in group 2, and texture/sampler bindings
  in group 7. Its surface validation accepts additional reported usages when
  render-attachment usage is present. A 60-frame run and a five-frame
  two-uniform run passed on the RTX 3090 Ti at forced FL9_1. A live client-area
  capture was visually inspected; strict offscreen tests provide deterministic
  readback.
- The public surface error fixture injects device removal from `Present`,
  checks one public device-lost callback, and reacquires/discards the same HAL
  swapchain to verify the failed frame released its acquisition reservation.
  This is synthetic HRESULT coverage on real FL9_1 hardware, not physical
  removal or recovery testing.
- The public capability projection and core/HAL audit still need independent
review across every valid pipeline, texture, buffer, mapping, render-pass and
error path. Current strict tests cover a broad subset but do not prove every
request permitted by the limits.
- The FL9_1 public limits explicitly report zero compute limits, storage
  bindings, dynamic storage buffers and storage-buffer binding size. Compute
  pipelines and storage binding layouts remain rejected by Core; a buffer's
  `STORAGE` allocation usage alone does not expose shader storage.
- Compute, storage access, queries, indirect draws, general multisampling (except optional query-gated
  2x/4x/8x RGBA8 render/resolve and matching D16/Depth24Plus/D24S8 depth attachments), arrays,
general comparison samplers, several shader builtins and unrestricted WGSL remain
unavailable and are gated or rejected. The texel-load/dimensions experiments
remain isolated proofs.
- The opt-in D3D11 debug-layer indexed-render test reported zero error or
  corruption messages and no live D3D11 resources after cleanup. This is a
  focused private-HAL check, not a debug-layer audit of the whole public suite.
- Physical device removal/recovery, historical FL9_1 drivers, CoreWindow on
  target UWP devices and Lumia ARM32 runtime remain unverified. Synthetic
  queue, fence, polling, and Present error paths have separate coverage.

Event-query failure retention now has a strict HAL regression: an injected
one-shot `E_FAIL` from `GetData` leaves the pending event and timeline value
intact, after which the real GPU event completes normally. A separate core
fixture injects `DXGI_ERROR_DEVICE_REMOVED` during queue replay and verifies
lost-device classification, one callback, rejected later submission and core
queue invalidation. Both are synthetic error injections; neither simulates
physical device removal or recovery.

The public texture-copy path now accepts RGBA8↔RGBA8-sRGB and BGRA8↔BGRA8-sRGB
copies that core already considers format-compatible. The D3D11 path retains
the native same-type-group requirement; it copies stored bits and performs no
sRGB conversion. Strict FL9_1 readback checks the sRGB intermediate and the
linear round-trip for both channel families. Other differing formats remain
rejected.

Do not describe this restricted experimental adapter as complete wgpu or
WebGPU support. See [TESTING.md](TESTING.md) for exact public and private test
commands.

See [TESTING.md](TESTING.md) for the staged validation plan and hardware matrix.

Outputless fragment shaders now execute in targetless D16 and D24S8 passes.
Strict FL9_1 readback verifies conditional discard prevents both depth and
stencil writes; targetless rendering still requires a depth/stencil attachment.

Fragment `@builtin(sample_mask)` input and output have separate downlevel
limits. Input lowers to HLSL `SV_Coverage`, which Microsoft documents as
requiring `ps_5_0`; output requires `ps_4_1`. The selected FL9_1 profile is
`ps_4_0_level_9_1`. Physical-device-gated FXC tests reject both directions
while color-only siblings compile. Core rejects only the selected fragment
entry using the unsupported direction, before HAL compilation. These are
shader-profile restrictions, not a general FL9_1 hardware guarantee.

The FL9_1 projection also sets
`DownlevelLimits::fragment_non_perspective_interpolation` to false. FXC rejects
fragment `flat` and `linear` interpolation with X4568, while vertex outputs
with those qualifiers and fragment `centroid` compile. Core rejects only the
selected fragment entry before HAL. A strict public test renders centroid and
perspective pipelines and compares all 64 readback pixels. Sample-qualified
interpolation remains covered by Naga's existing
`MULTISAMPLED_SHADING` validation capability.

The HAL maps all wgpu vertex formats with direct DXGI IA equivalents except
Float64 variants. At FL9_1, formats outside the required set enter the
selected adapter's vertex-format mask only when `CheckFormatSupport`
(`IA_VERTEX_BUFFER`) succeeds; the feature-level guarantee set remains
unchanged. The RTX 3090 Ti forced to FL9_1 reports exactly the ten required
formats and no optional format. The strict public Float16x2 readback test logs
that case as a skip. Synthetic capability tests cover positive optional
queries, unavailable queries and missing backend mappings.

Core accepts vertex attribute offsets aligned to `min(format.size(), 4)`.
The HAL now preserves those validated byte offsets in
`D3D11_INPUT_ELEMENT_DESC::AlignedByteOffset` instead of imposing a blanket
four-byte alignment. The public regression rejects an offset of one for
`Unorm8x2`; it renders all 64 pixels from `Unorm8x2` at offset two when the
selected device reports `R8G8_UNORM` input support. The RTX 3090 Ti forced to
FL9_1 reports no IA format smaller than four bytes, so the positive render
case explicitly skips on this machine; no hardware execution is claimed for
the byte-offset path yet.

The required FL9_x packed B5G6R5, B5G5R5A1 and B4G4R4A4 formats are now
implemented as `Rgb5g6b5Unorm`, `Rgb5a1Unorm` and `Rgba4Unorm` with a shared
native-only feature and individual selected-device format queries. All three
support single-mip sampled uploads with channel/alpha readback coverage. The
alpha-bearing formats also support same-format GPU texture-to-texture copies
when the selected format query reports `TEXTURE2D`; the copy path does not add
render-target bind flags or texture-to-buffer readback. A strict public test
uploads each format, partially copies at a nonzero destination origin, samples
the result and verifies copied and preserved pixels. The
query-gated B5G6R5 path also supports single-mip color attachments when the
selected device reports `RENDER_TARGET`; blending is separately projected
only when both attachment support and `BLENDABLE` are reported, and is covered
by strict public readback. It has no
direct readback or mip-chain use outside the existing query-gated copy path.
The RTX 3090 Ti forced to FL9_1 reports `0x0026F3A0` for B5G6R5 and
`0x000213A0` for both alpha-bearing formats. A strict public test draws to a
B5G6R5 attachment, samples it and checks all 64 output pixels within one
UNORM code value.

## WGSL loop control

The D3D11 FL9_1 path disables Naga's dynamic loop-bounding guard so FXC can
unroll statically bounded loops. `shader_loops` reports that subset, while
`shader_dynamic_loops` is false. Core analyzes the selected entry point and
helper calls and rejects runtime-dependent exits and provably unbounded
literal-false exits before HAL compilation. It also treats caller-local values
passed to pointer parameters as runtime-tainted. A strict public test rendered
correctly with static loops in both stages and checked every pixel; core also
rejected the dynamic vertex and fragment cases. FXC instruction and unrolling
limits remain. Details and exact commands are in `FL9_1_PROGRESS.md`.

## Query-gated 3D mip sampling

POT RGBA8 3D textures may use mip chains when the selected device reports
`D3D11_FORMAT_SUPPORT_MIP` and the backend path is enabled. Uploads target each
mip separately. FL9_1 views must include the lowest mip, so core rejects
non-suffix 3D views through `DownlevelLimits::texture_3d_non_suffix_mip_views`.
The strict RTX 3090 Ti test sampled three uploaded mip levels at requested and
actual FL9_1; all 64 output pixels matched. GPU copies and readback for 3D
textures remain unsupported.

The fixture-only public HWND error test injects `DXGI_ERROR_DEVICE_REMOVED` at
Present. On the physical RTX 3090 Ti opened at requested/actual FL9_1, core
delivered the public lost callback exactly once; callback-reentrant polling
returned the expected presentation-guard timeout. Injection bypasses the
native Present call and does not simulate physical device removal.

The public integration suite now includes an indexed perspective cube rendered
with an uploaded RGBA8 face texture, a uniform transform, a nearest sampler
and D16 depth. At forced FL9_1 on the RTX 3090 Ti, rendering the faces in
forward and reverse order produced identical complete 32×32 readbacks; red,
blue and yellow faces plus the clear background were all observed. With depth
omitted, reversing the order changed 223 pixels, confirming that the scene
contains meaningful overlap resolved by the D16 test. This is a representative
small scene, not evidence that the remaining public API surface is implemented.

The pipeline subset now includes query-gated `POLYGON_MODE_LINE` where
`CreateRasterizerState(D3D11_FILL_WIREFRAME)` succeeds. The feature maps only
to triangle wireframe; polygon point remains unavailable. The strict public
RTX test matched the full wireframe image to explicit line primitives, with 17
green pixels on the 16×16 target.

## Direct vertex-stage uniform-array indexing

Core and HAL permit a selected vertex entry point to index a fixed-size uniform
array of `vec3` or `vec4` with `vertex_index` or `instance_index`, including
unchanged forwarding of that builtin through helper parameters when the value
is used exclusively for the supported array access. Computed indices, nested
arrays and other element types remain rejected. Naga emits a signed bounds clamp for the FL9_1 profiles because FXC
rejects its default unsigned clamp form. The strict public RTX 3090 Ti test at
requested/actual FL9_1 read distinct entries through direct and two-helper
paths and confirmed an index equal to the array length uses the final
in-range value. Historical FL9_1 driver behavior remains unverified.

The same restricted indexing path also accepts explicitly padded uniform
arrays of one-member, 16-byte structs containing `vec2<f32>` at offset zero.
Core and HAL reject other `vec2` array layouts; the public strict readback test
verified four distinct values and the clamped final index on the RTX 3090 Ti.
The unpadded `array<vec2<f32>, N>` form is invalid WGSL in uniform address
space and is not a supported workaround.

The same narrow selected-vertex proof now permits direct `instance_index`
access to a fixed array of up to 63 `mat4x4<f32>` values. Computed indices,
non-square matrices and longer arrays remain rejected. The bound keeps one
matrix array to 252 float4 constants; FXC still rejects shaders whose aggregate
referenced constants exceed the FL9_1 shader-profile budget. The HAL proof and
FXC compile test pass, and a strict public draw uses two matrix entries to
translate a triangle into distinct left and right regions. All 64 pixels were
read back on the physical RTX 3090 Ti at requested/actual FL9_1; the two
instance-selected samples were magenta and the gap remained clear. Historical
FL9_1 hardware remains unverified.

## Direct fragment uniform-array indexing

Core and HAL permit up to two direct fragment-stage reads from separate
`array<vec4f, N>` uniform bindings for `2 <= N <= 24`. Each uniform uses the
same structure layout with its scalar `u32` index at offset zero and array at
offset 16; the bindings may be in different groups. The selected entry must
use each index only for its corresponding read. Helper calls, computed or
cross-binding indices, other integer uses/accesses, a third read, and
different array shapes remain rejected. The
selected FL9_1 projection reports
the dedicated `DownlevelLimits::fragment_uniform_array_indexing` gate rather
than reusing a vertex-only capability. Naga-generated HLSL compiles under FXC
for lengths 2 and 24; FXC rejects length 25 with X5589 at constant register
32 (maximum 31). The 24-element Level9 shader uses 40 pixel-shader instruction
slots. The strict public RTX 3090 Ti test at
requested/actual FL9_1 rendered indices zero, one and 23 and verified all 64
pixels per value. A second strict pipeline performs two direct lookups from
separate sparse bindings and verifies 64 pixels for each of three index
pairs. This does not enable general fragment integer operations or establish
historical-driver behavior.

## Hidden `vertex_index` input-slot validation

The emulated builtin consumes a physical vertex-input slot. Core now mirrors
the FL9_1 HAL allocator, including spills caused by overlapping attributes,
and rejects selected `vertex_index` layouts that cannot reserve the hidden
stream before HAL pipeline creation. The same layout remains available to
entry points that do not use the builtin. Synthetic allocator boundaries and
a strict public validation test passed; the validation test issues no draw.

## Legacy GLSL/SPIR-V shader built-in validation

Core now represents the FL9_1 restrictions on `BaseVertex`, `BaseInstance`,
`PointSize`, and `PointCoord` as downlevel limits. It checks the selected
vertex or fragment entry point before asking HAL to compile a shader, while
ordinary sibling entry points remain available. A strict public test on the
RTX 3090 Ti at requested and actual FL9_1 verifies the four validation errors
and successful pipeline creation for ordinary GLSL shaders. This closes a
core/HAL validation mismatch; it does not add shader functionality.

## Matrix inverse shader validation

Naga's HLSL writer lowers f32 `mat2x2` and `mat3x3` inverse using explicit
cofactors. DX11 exposes them through separate `matrix_inverse_2x2` and
`matrix_inverse_3x3` downlevel limits, while general `matrix_inverse` remains
false. Core checks the selected entry point and reachable helper calls;
mat4 and other unsupported shapes fail as validation errors before HAL. The
strict public FL9_1 GLSL test executes both mat2 and mat3 inverse in vertex and
fragment stages. All 64 mat2 pixels matched `[0, 255, 0, 255]`; the
non-symmetric mat3 output matched `[64, 64, 64, 255]`, with independent vertex
and fragment results in separate channels. A mat4 negative test confirmed
Core rejection without an internal HAL error. A new Naga-to-FXC compiler probe
uses runtime matrix inputs: its `ps_5_0` control compiles, while
`ps_4_0_level_9_1` is rejected with X5608 because generated code uses 108
arithmetic slots against the 64-slot pixel limit and 108 total slots against
the 96-slot limit. This measures the compiler/profile boundary for the
currently generated inverse; Core keeps fragment mat4 inverse rejected. The
probe is compiler-only. WGSL has no inverse builtin, so this applies to
GLSL/SPIR-V or Naga IR inputs. Other shapes and DX12/Metal projection remain
unavailable. This is a bounded translator capability, not additional FL9_1
hardware support.

The HLSL writer now lowers `outerProduct` for validated f32 vectors and square
or rectangular matrices. DX11 exposes the corresponding downlevel limit, and
selected-entry validation still rejects occurrences in regular module helper
functions if a backend does not support them. Strict FL9_1 GLSL rendering
verified square and rectangular results across all 64 pixels. The GLSL front
end has no WGSL `outerProduct` builtin; DX12 keeps its limit disabled pending
separate validation.

Core unmap now skips the HAL call for zero-length mappings. Core represents
empty mapped ranges with a dangling zero-length pointer and the D3D11 HAL does
not own a native mapping for them. Strict core fixtures cover zero-size
mapped-at-creation and empty asynchronous mappings before unmap. The D3D11
resolve command now retains the source and destination view mip indices; a
strict forced-FL9_1 HAL test resolves into mip 1 and verifies mip 0 remains
unchanged.

## Query-gated R8_UNORM mip chains

R8_UNORM now supports mip chains only when the selected device reports
TEXTURE2D | MIP and the backend path is enabled. NPOT chains also require
positive unrestricted-NPOT support. Render-target and copy-source combinations
require the selected RENDER_TARGET query and matching implemented path;
SHADER_SAMPLE separately controls filterability. On the RTX 3090 Ti forced to
FL9_1, a strict public test uploaded four mips, sampled mip 2, cleared mip 0
as an attachment and read mip 0 back; all 64 RGBA pixels and all 64 R8 texels
matched. A strict public copy test now covers all 16 source/destination mip
index pairs for two four-level R8 textures. Every pair's offset region is read
back before later writes can replace it, then complete destination mips are
checked for copied texels, preserved texels and row-padding sentinels.

## R8 NPOT mip-chain evidence

R8_UNORM mip chains now have positive physical-device coverage for the
unrestricted-NPOT branch. The RTX 3090 Ti reported unrestricted NPOT support
while explicitly running at FL9_1. A strict public 6x10, four-level R8 chain
was uploaded and sampled at mip 2; 64/64 RGBA pixels matched, and clearing and
reading mip 0 verified all 60 R8 texels. Negative/unavailable query behavior
remains covered synthetically; historical restrictive drivers are unverified.

## Optional RG8_UNORM sampled mip chains

RG8_UNORM mip chains are now an optional sampled/upload path, gated by the
selected device's `TEXTURE2D | MIP` query and the backend path. Filterability
continues to follow `SHADER_SAMPLE`; NPOT chains require the positive
unrestricted-NPOT capability. The RTX 3090 Ti query (`0x0002c320`) omits MIP,
so its strict test explicitly skips mip execution while confirming the
single-mip projection. Synthetic tests cover the positive and negative query
branches. No physical execution is claimed for the mip path.

## Narrow FL9_1 fragment texture dimensions

The opt-in public projection exposes a separate
`texture_dimensions_2d_float_fragment` limit while keeping general
`texture_query` unavailable. Core proves one selected fragment query on a
non-array `texture_2d<f32>`, directly converted to `vec2f`. The direct form may
bind the image in any valid group. The query may alternatively be returned by
exactly one reachable, argument-free, straight-line helper called once; that
helper form remains group-0-only. The helper cannot query a mip, use other
resources or image operations, or call another function. Unreachable sibling
helpers and resources are pruned from emitted HLSL. Direct query behavior,
including its existing optional constant-mip and query-plus-one-sample forms,
retains the one-image restriction; an accompanying sampler remains group-0-only.
The helper may now coexist with exactly one level-less implicit sample of the
same 2D float texture in the selected entry point, using one group-0
non-comparison sampler and a `vec2f` coordinate. The helper itself remains
query-only; helper sampling, explicit LOD, extra resources, control flow,
`select` and other image operations are rejected. HAL reuses the host-supplied
dimension constant and updates it when the bound view changes. The strict RTX
3090 Ti test exercises the helper and sample after switching 4x2 and 8x4 views;
all 64 pixels matched expected output. General texture queries remain
unavailable.

The same narrow 2D path now also allows exactly one level-less implicit
`textureSample` of the queried texture with one group-0 non-comparison sampler.
Both the query and sample must be used directly in the selected fragment entry;
extra resources, image operations, helpers, control flow, `select` and runtime
integer work remain rejected. A strict public FL9_1 readback test switched
between 4x2 and 8x4 views with different texel colors; all 64 pixels matched
the output that depends on both queried dimensions and sampled contents.
General `texture_query` remains unavailable.

The separate 2D fragment `textureLoad` and `textureDimensions` emulations may
also be combined when both existing downlevel limits are enabled. Core, Naga
and HAL require one direct level-less query and one load from the same
group-0, single-sample `texture_2d<f32>`, with a constant integer coordinate,
no helper/control flow, no sampling and at most one used `vec4<f32>` uniform.
Other used resources and additional or differently shaped uniforms remain
rejected. HAL reuses the RGBA8 single-mip point-sampling load path and supplies
dimensions through the same reserved `b13` constants; the user uniform uses
the ordinary `b0` register in the tested layout. Strict public forced-FL9_1
rendering verified 64/64 output pixels as `[100, 60, 80, 255]` without a
uniform and `[50, 30, 20, 255]` with one. This composition adds no new
capability flag and general `texture_query` remains disabled.

## Narrow FL9_1 fragment `textureNumLevels`

The opt-in projection exposes separate
`texture_num_levels_2d_float_fragment`,
`texture_num_levels_3d_float_fragment` and
`texture_num_levels_cube_float_fragment` limits for one fragment
`textureNumLevels` query on a matching group-0, non-array float texture,
directly converted to `f32`. The 2D query may be direct or returned by exactly
one reachable, argument-free straight-line helper called once by the selected
entry point. That helper itself cannot sample, but the selected entry may also
contain one direct implicit sample of the same image with one group-0
non-comparison sampler and a coordinate type matching its dimension. The
selected image may be 2D, 3D or cube, each gated by its corresponding limit.
The 3D/cube paths are gated by the selected device's sampled RGBA8 volume/cube
support; they do not require mip support because a single-mip view validly
reports one level. Core, Naga and HAL require a direct query or one selected
parameterless straight-line helper that returns the query, with no control
flow, additional resources, other image operations beyond at most one
same-image implicit sample, mismatched image/sampler shapes or unrelated
integer semantics. HAL supplies
the selected bound view's mip count through `b13`; texture and sampler retain
their ordinary register assignments. Strict GPU execution covers helper-only
3D and cube queries across one- and three-level views. The helper-plus-sample
combination is covered by CPU proof and FXC compilation tests, while the 2D
helper-plus-sample form has full GPU readback coverage. The 2D composed output
matched `[85,64,128,255]` and
`[255,128,64,255]` for all 64 pixels on the RTX 3090 Ti at requested/actual
FL9_1. The separate direct query-plus-sample test also remains covered. General
`texture_query` remains unavailable.

## Constant-mip FL9_1 fragment `textureDimensions`

The existing `texture_dimensions_2d_float_fragment` limit also accepts one
literal or module-constant mip level on the same narrow 2D float fragment path.
The level is relative to the current view. Core and HAL reject dynamic,
override-based and negative levels. The host supplies dimensions computed from
the current view base size and selected level through `b13`; out-of-range levels
use a safe value because WGSL leaves the result indeterminate. General
`texture_query` remains disabled. See `TESTING.md` for the strict readback test.

## Narrow FL9_1 fragment 3D `textureDimensions`

The opt-in projection adds `texture_dimensions_3d_float_fragment` separately
from the 2D query limit. It is enabled only when the selected FL9_1 device's
RGBA8 3D sampled-format query succeeds and the backend path is enabled. The
selected fragment entry must contain one direct `textureDimensions` query on
one group-0, non-array `texture_3d<f32>`, followed by a direct `vec3f`
conversion. An explicit mip argument must be a nonnegative literal or module
constant and cannot coexist with sampling in that entry point. Multi-mip
resources remain separately gated by the selected device's `MIP` query and
`texture_3d_mipmaps` limit. A level-less query may also perform one implicit
sample of the same volume through one group-0 non-comparison sampler, using
`vec3f` coordinates. HAL supplies the selected 3D view's dimensions at the
requested mip through reserved `b13`; texture and sampler registers remain
allocated through the shared logical binding map. The existing strict public
test on the RTX 3090 Ti explicitly selected FL9_1 and transitioned between
4x2x8 and 2x4x1 volumes containing different colors. The new mip test queries
level 1 of an 8x4x8, three-level volume and checks 4x2x4 through GPU readback.
One parameterless, straight-line helper may directly return a level-less
`vec3u` query when the selected entry directly converts that result to
`vec3f`; helper mip arguments, extra helper operations or nested calls remain
rejected. The strict helper test transitions the same two volumes and verifies
dimensions and sampled colors across all 64 readback pixels. Extra resources,
other image operations, explicit LOD and additional samples remain rejected;
this does not enable general texture queries, 3D copies, render attachments or
readback.

## Narrow FL9_1 fragment 3D `textureLoad`

A separate `texture_load_3d_float_fragment` downlevel limit covers a
single-mip RGBA8 volume load in a restricted fragment entry point and its
reachable straight-line helpers. Each load must directly reference the same
image global; helpers cannot pass resource handles, branch, select, use other
image operations or add other bound resources. Naga lowers the load to
internal point sampling at texel centers; HAL supplies the selected D3 view
dimensions in `b13` and binds its internal `s15` sampler. Core rejects an
unsupported shape unless the 3D-specific limit is true, independently of the
existing 2D load limit. The strict RTX 3090 Ti test at requested/actual FL9_1
checks two in-bounds colors, zero for an out-of-bounds coordinate and a
nonzero mip, plus a nested helper result, across 320 pixels. FXC accepts the
generated direct and helper shaders under `ps_4_0_level_9_1`. This does not
enable general `texture_load` or 3D copies, attachments or readback; device
support remains gated by the selected RGBA8 volume-sampling query. Core also
validates the selected entry point's currently bound 3D view before HAL draw
execution; a strict test rejects a multi-mip view as Validation rather than
allowing a late HAL error.

## Native timestamp diagnostics; public feature deferred

The HAL retains native timestamp and disjoint-query code for diagnostics. A
strict FL9_1 hardware test executes two timestamp writes inside one disjoint
interval and checks the reported frequency and ordering. This does not justify
public `TIMESTAMP_QUERY` support: Core permits query writes and resolves across
queue submissions, while D3D11 only guarantees timestamp reliability and
frequency within one disjoint interval. The public projection therefore omits
`TIMESTAMP_QUERY` and both inside-encoder/pass features, and the public queue
period is zero. Public tests verify feature requests and query-set creation are
rejected before HAL timestamp commands can be recorded.
## Narrow FL9_1 fragment cube `textureDimensions`

The opt-in projection adds `texture_dimensions_cube_float_fragment`, gated by
the selected device's cube-format sampling query and the backend path. Core,
Naga and HAL permit a direct cube face-size query or one parameterless,
query-only helper converted from `vec2u` to `vec2f`, with either no mip
argument or a nonnegative literal/module-constant level. An explicit-level
query cannot also sample in the same selected entry point; the level-less
query may share one implicit sample of the same cube with `vec3f` coordinates.
The HAL supplies the selected view's face dimensions at that mip through
`b13`; nonzero levels additionally require the existing selected-format mip
support query. Ordinary cube resource binding continues through the existing
mapping. FXC's level-9 profile does not receive `GetDimensions`; the size comes
from the shared emulation constant. Strict public execution on the RTX 3090 Ti
at requested/actual FL9_1 verified a direct query and one helper call across
the 8x8/4x4 view transition and a mip-1 query on a populated four-level cube.
Helper-side sampling, extra calls/resources, dynamic or negative levels,
explicit level plus sampling, unrelated image operations and general texture
queries remain unsupported.

## Narrow public BC1/BC2/BC3 unorm feature — 2026-11-09

The D3D11 projection now reports the native-only
`TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM` feature only when the selected device
and backend implement sampled upload for all three formats. Core accepts these
unorm formats through that feature while keeping the standard BC1–BC7 feature
requirement for sRGB BC, BC4, BC5, BC6 and BC7. Per-format queries continue to
report copy-source capability independently, and the FL9_x texture-to-buffer
readback mask remains clear.

A strict public wgpu test on the RTX 3090 Ti at requested/actual FL9_1 uploads
BC1 red, BC2 green and BC3 blue data, copies each to a second texture, samples
all three in one fragment shader and verifies 64 white pixels. A BC4 texture
request is rejected. The Core fixture uses the same projected feature; the
broad aggregate BC feature remains absent. The feature does not add BC sRGB,
BC4–BC7, mip, render-target or CPU-readback support.

## Narrow public BC1/BC2/BC3 sRGB feature — 2026-11-09

Added the independent native-only
`TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB` feature. The selected-device projection
requires separate `CheckFormatSupport` results for all three typed sRGB formats,
their FL9_1 guarantees and the existing sampled-upload implementation. Core
permits only BC1/2/3 sRGB through this feature; it does not imply the unorm
feature or the aggregate BC feature. Per-format usages gate GPU copies, and
texture-to-buffer readback remains unavailable.

Strict forced-FL9_1 public execution on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) uploaded and copied mid-gray BC1/2/3 sRGB blocks, sampled them
through the public wgpu pipeline and verified all 64 readback pixels near
linear `[59, 57, 59, 255]`. This differs decisively from raw unorm values near
`[132, 130, 132]`, proving sRGB decoding as well as format creation and copy.
The existing unorm workload still produced 64 white pixels; BC4 and BC3 3D
texture creation were rejected. This modern-driver result does not establish
behavior on historical FL9_1 hardware.

Core now exposes the `sampler_lod_clamp` downlevel limit. The opt-in FL9_1/9_2
projection sets it false so Core rejects mixed min/mag or anisotropic samplers
with non-default LOD clamps before the HAL can classify them as unexpected
device errors. Same-filter isotropic single-mip normalization remains
available. Strict Core fixture validation and rendering passed on the RTX
3090 Ti forced to FL9_1. HAL reports translated sampler mip restrictions to
Core, which checks the selected pipeline's sampled pairs against the currently
bound view ranges during render-pass and bundle command validation. A strict
Core case rejects a two-mip view for a single-mip-restricted sampler before
HAL draw execution. DX11 remains excluded from default backend selection, but
applications that explicitly enable the `dx11` feature and request the backend
can enumerate the restricted public adapter.

## Direct FL9_1 vertex-index local-array read

Core, Naga and HAL permit one direct `vertex_index` read from an initialized
function-local `array<vec2<f32>, 3>` in a straight-line vertex entry. The
selected public projection reports the narrow
`DownlevelLimits::vertex_index_local_array_indexing` limit. Helper forwarding,
computed indices, writes, multiple dynamic accesses, and dynamic vector or
uniform indexing remain rejected. A strict public fullscreen-triangle test
verified all 64 pixels on the RTX 3090 Ti at requested/actual FL9_1. This is a
compiler/backend subset, not general runtime integer support, and historical
FL9_1 hardware remains unverified.

## Direct FL9_1 vertex-index float-vector read

The selected FL9_1 projection now also enables
`DownlevelLimits::vertex_index_vector_indexing`. Core and HAL accept only
direct read-only `vertex_index` access to float vectors in a straight-line
vertex entry; helpers, computed indices, stores and pointer escapes remain
rejected. Naga emits static swizzle comparisons instead of dynamic vector
indexing. The lowering preserves Naga's restricted-index behavior by selecting
the last vector component for indices outside the in-range component cases.
An FXC probe accepted vec2/vec3/vec4 lowering at `vs_4_0_level_9_1`, and a
strict public RTX 3090 Ti test verified all 64 pixels of the vec3-indexed
triangle. The capability defaults false; noop reports it because it does not
compile or execute native shaders, and DX12 leaves the HLSL lowering disabled.
This is not general runtime integer support or historical-hardware evidence.

## Direct FL9_1 fragment `frexp` fraction

The selected FL9_1 fragment profile permits a direct scalar `frexp(value).fract`
projection only. Core and HAL prove the math-result and fraction-access
expressions are emitted together in the selected entry point; they reject a
whole-result local, exponent access, helper call, vector result, vertex-stage
use and any additional runtime integer semantics. The new downlevel limit is
false by default and enabled only in the selected FL9_1 D3D11 capability
projection.

Microsoft's [HLSL `frexp` reference](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx-graphics-hlsl-frexp)
documents the source intrinsic as shader-model-3 functionality, but the
installed compiler accepts the restricted HLSL at
`ps_4_0_level_9_1` and emits the fraction path in the Level9 `ps_2_0` section
with float instructions only. The strict public FL9_1 math test verified all
64 pixels for positive six, negative six (sign-sensitive encoding) and zero;
the complete runtime `frexp` result remains an expected Core validation error.
This proves the installed compiler and modern NVIDIA driver path, not every
FXC release or historical FL9_1 driver. Other WGSL `frexp` forms remain
unavailable.

## Query-gated RGBA8 sRGB fragment `textureLoad` — 2026-11-11

The existing 2D fragment-load emulation now accepts an `Rgba8UnormSrgb`
resource/view when the selected adapter's normal format projection permits
sampled use. It uses the proven texel-center point sample through the typed
sRGB SRV, which converts RGB to linear values and leaves alpha linear. Core
continues to validate the actual bound view; HAL keeps sRGB out of 3D load and
sample-bias-offset paths. Strict forced-FL9_1 RTX readback verified encoded
`[128, 128, 128, 128]` as linear `[55, 55, 55, 128]` across 64 pixels. This
does not establish behavior on historical FL9_1 drivers.

## Query-gated Rg16Snorm fragment 2D `textureLoad` — 2026-11-11

Core and HAL now allow `Rg16Snorm` in the existing selected-entry fragment
2D texture-load emulation, while preserving the separate 3D and sample-bias
format restrictions. The selected adapter must positively report
`TEXTURE_FORMAT_16BIT_NORM` and the existing sampled/upload usages; a
nonfiltering layout is sufficient. Strict RTX 3090 Ti execution at requested
and actual FL9_1 verified signed RG normalization for four texels across all
64 output pixels and zero for 64 out-of-bounds pixels. This is optional
format-query evidence from a modern driver, not historical FL9_1 validation.

## Vertex-only FL9_1 f32 `mat4x4` inverse

The Naga HLSL writer lowers f32 4x4 inverse with explicit cofactors. DX11
projects this through `DownlevelLimits::matrix_inverse_4x4_vertex`; selected
entry-point validation permits it only in the vertex stage and only for f32
square 4x4 matrices. General matrix inverse remains disabled, as does fragment
mat4 inverse. A strict forced-FL9_1 public test feeds a non-symmetric matrix
through runtime vertex attributes and verifies all 64 readback pixels as
`[64, 128, 128, 255]` within two UNORM steps. This measures the installed FXC
and RTX 3090 Ti path; historical FL9_1 compiler/driver behavior is not
established. WGSL has no matrix inverse builtin.

The selected-entry FL9_1 fragment 2D float `textureLoad` proof now permits one
used uniform buffer alongside its selected texture. It uses the existing
uniform register mapping and keeps the texture-load emulation's reserved
registers. Multiple used uniforms, uniform-backed 3D loads, and combined
load/dimensions with a uniform remain rejected. The strict RTX 3090 Ti test
verified the tint output at group 0/binding 7 and group 1/binding 42; exact
commands and regression counts are in `TESTING.md` and `FL9_1_PROGRESS.md`.

## Eight-image FL9_1 fragment 2D `textureLoad` — 2026-11-12

The shared Naga HLSL writer no longer clamps integer load coordinates before
sampling. The reserved internal sampler is already point-filtered and
clamp-to-edge, while the generated bounds mask returns zero for invalid
coordinates; the explicit coordinate clamp duplicated that address behavior.
This reduced the arithmetic cost enough for the representative eight-image
shader to compile under FXC's `ps_4_0_level_9_1` 64-slot limit. Core/HAL keep
the eight-image bound and reject a ninth resource. The strict public test
verified all 64 pixels at forced FL9_1 for eight-image output, distinct
2-image output, positive/negative out-of-bounds zero, one- and two-uniform
outputs and the
existing nested-helper, dynamic-coordinate, invalid-mip and sRGB cases. The
3D load helper retains its explicit coordinate clamp.

## Two uniforms with FL9_1 fragment 2D `textureLoad` — 2026-11-12

The selected-entry fragment 2D float texture-load proof now permits two used
uniform buffers alongside up to eight images. Core, HAL and Naga reject a third
used uniform; at this checkpoint the 3D uniform combination was not yet
supported. Sparse uniform bindings map to distinct compact native constant
registers while `b13` remains reserved for internal image dimensions. Strict
RTX readback verified a sampled texel multiplied by a tint and combined with a
bias from the second uniform as `[64, 64, 32, 255]` across all 64 pixels, for
both group-0 and group-1 sparse layouts. See `FL9_1_PROGRESS.md` for exact
commands and regression results.

## Six uniforms with selected fragment 2D `textureLoad` — 2026-11-12

Core, Naga and HAL now permit up to six used uniform buffers alongside the
existing eight-image selected fragment 2D float texture-load path. Sparse
logical bindings use the existing compact uniform allocator; the internal
`b13` image-dimension buffer remains reserved. Seven used uniforms and other
used resource classes remain rejected. At this checkpoint uniform-backed 3D
loads were not yet supported; a later increment is recorded below. Strict RTX
3090 Ti readback at requested/actual FL9_1 verified
the six-uniform result `[64, 96, 32, 223]` for 64/64 pixels in each of group 0
and group 1. This does not claim general FL9_1 shader-resource support.

## Uniform coexistence with selected fragment 3D `textureLoad` — 2026-11-12

The existing single-image, single-mip fragment 3D `textureLoad` emulation now
allows up to six used uniform buffers. Core, Naga and HAL retain selected-entry
validation, reserve `b13` for volume dimensions and use the existing compact
uniform registers. Strict public execution on the RTX 3090 Ti at requested and
actual FL9_1 used a sparse group-1/binding-42 tint uniform; the loaded voxel
rendered `[127, 0, 64, 255]` across all 64 pixels. Synthetic tests reject seven
uniforms, and Core still rejects this path when its 3D load limit is disabled.
Multiple images, unrelated used resources, 3D load/dimensions with uniforms,
and multi-uniform or wrong-type uniforms in the combined 2D path remain
unsupported.
## Computed coordinates in one fragment 2D `textureLoad` helper — 2026-11-12

The narrow selected-entry fragment 2D float `textureLoad` path accepts one
straight-line helper that forwards the entry-point `vec2f` argument unchanged,
then applies literal-based f32 arithmetic, composition, splat, component
access or swizzle before the existing `vec2i` cast. Core and HAL validate this
same shape; the HAL compiles it with the FL9_1 pixel shader profile. Strict
RTX 3090 Ti rendering at requested/actual FL9_1 checked all 64 pixels for
group 0/binding 7 and group 1/binding 42, including the shifted source column
and out-of-bounds zero. Entry-point call-site arithmetic, nested helpers and
other runtime integer semantics remain rejected. Strict RTX 3090 Ti testing
at requested/actual FL9_1 verified both group configurations. Full regressions
passed Naga 171, Core DX11 136, Core DX12 135, private HAL 517 and public
D3D11 153 tests. The separate-alpha blend portability warning and historical
FL9_1 driver gap are unchanged.

## Cross-device surface reconfiguration after a pending-present timeout

The HAL surface interface now has an additive fallible unconfigure operation;
other backends retain the old infallible behavior by default. D3D11 reports a
timeout or teardown error without releasing the configured swapchain. Core keeps
the previous Presentation association on that error, so a later configure can
retry unconfiguration with the original device. A strict public regression
performs an actual hidden-HWND present, injects a one-shot pending-present
completion timeout during cross-device reconfiguration, verifies that the
previous configuration remains selected, completes the present, and then
successfully configures a second FL9_1 device. The injected timeout is
synthetic; physical presentation timeout behavior is not claimed.

## Device loss during cross-device surface reconfiguration — 2026-11-12

If D3D11 reports `Device(Lost)` while unconfiguring, it retires the lost
swapchain and reports the loss instead of claiming success. Core removes the
old presentation, attributes the loss to the old device, and defers its
callback until the surface and queue locks are released before configuring the
replacement device. Device loss during the replacement configure path is also
deferred until queue submission locks are released. Strict public fixtures
inject both HRESULT failures and reenter `Device::poll` from each callback.
These tests establish error routing and lock ordering, not physical GPU
removal or TDR recovery.

## Vertex-indexed local matrix column writes

A straight-line vertex entry point can now assign a direct local
`matCx2<f32>` column using `@builtin(vertex_index)`. Naga emits a signed
conditional split-matrix setter accepted by FL9_1 FXC. Core and HAL use matching
shape proofs; dynamic row writes, nested or array matrices, and helper-mediated
stores remain blocked. Strict public GPU readback on the RTX 3090 Ti at
requested and actual FL9_1 matched all 64 pixels within two LSB per color
channel. This does not enable general runtime integer operations or the
downlevel matrix aggregate-store flag.

## One fragment uniform with 2D dimensions and sampling — 2026-11-14

The selected direct FL9_1 fragment entry may combine one `vec4f` uniform
with a level-less `textureDimensions` query and one implicit sample of the
same non-array 2D float image. Core, HAL and Naga enforce the selected-entry
shape and reject multiple or differently shaped uniforms, helpers, mismatched
images, other selected resources and extra image operations. User and
emulation constants use separate registers (`b0` and `b13`). A strict public
test on the RTX 3090 Ti explicitly forced FL9_1 and verified all 64 pixels
after switching between 4x2 and 8x4 textures; the tint, dimensions and texel
values all affected the result. Uniform coexistence remains unsupported for
helper, 3D/cube and explicit-mip query paths.

## Query-supported multisample-mask execution — 2026-11-14

The strict public 4x RGBA8 resolve test now exercises masks `0`, `1`, `3`,
`0x10` and `1 << 32`. On the RTX 3090 Ti at requested/actual FL9_1, masks 1
and 3 changed all 64 pixels to quarter- and half-coverage white over blue;
zero and bits outside the four-sample range preserved all 64 blue pixels.
The test exercises sample-mask state changes, FL9_1's zero-mask translation,
64-bit-to-native mask truncation, resolve and readback. This is selected modern
driver evidence only.

## Query-gated R8 3D textures

The selected FL9_1 projection now includes R8_UNORM 3D textures only when its
format guarantee, device `TEXTURE3D | SHADER_SAMPLE` query and backend path all
agree. Format-sized shadow storage supports initialization, padded upload,
3D-to-3D CPU-shadow copy, texture-to-buffer readback, views and sampling.
Because the public mip limit is global, multi-mip 3D support requires every
projected 3D format to report `MIP`. The 3D `textureLoad` emulation remains
RGBA8-only.

On the RTX 3090 Ti explicitly forced to FL9_1, strict private HAL execution
verified 64 R8 voxels after upload/copy/readback and sampled every slice into
64 matching RGBA8 pixels. A strict public wgpu test verified upload, partial
3D copy and padded readback for 64 voxels; the Core fixture verified the R8
format projection and D3 view. R8 volumes still cannot be render attachments
or GPU-written sources for the CPU-shadow copy fallback. The selected RTX
does not establish behavior on historical FL9_1 drivers.

## Optional RG8_UNORM 3D textures

RG8_UNORM is not a format guaranteed by the 10Level9 FL9_1 table. The
selected-device projection now adds its 3D format bit only after a positive
`CheckFormatSupport` result containing both `TEXTURE3D` and `SHADER_SAMPLE`;
2D support and the modern GPU's maximum feature level are not substitutes for
that query. The existing format-sized volume upload, CPU shadow, partial copy,
readback, view and sampled-output paths accept its two-byte texels once that
predicate is satisfied. It remains excluded from the RGBA8-only 3D
`textureLoad` emulation.

Synthetic capability cases cover a positive optional query, missing or
incomplete query bits, a 3D-only projection, and the global mip limit when
RG8 is combined with formats that do or do not report MIP. The RTX 3090 Ti
forced to FL9_1 returned `0x0002c320` for RG8 format support, without the
required `TEXTURE3D` bit. The strict private HAL and public tests both emitted
an explicit `SKIP`; neither is a GPU execution pass. Their code paths remain
unverified on hardware that positively reports RG8 3D sampling support.

## Public textured depth-tested FL9_1 draw — 2026-11-14

The strict public test binds a sampled texture and sampler at sparse logical
bindings 7 and 42, then draws green, red and blue full-screen textures at
increasing depth. The nearer red draw passes the `Depth24Plus` `Less` test and
the later farther blue draw is rejected. CPU readback matched opaque red for
all 64 pixels on the RTX 3090 Ti at requested and actual FL9_1. This proves
the combined sampled-resource and depth path for this exact pipeline shape.

## Restricted FL9_1 fragment `textureSampleGrad`

One direct, unconditional fragment `textureSampleGrad` or one narrow
helper-wrapped sample is lowered to implicit sampling only when the selected
image is a non-array 2D float texture, its currently bound view exposes exactly
one mip, and the regular sampler has matching min/mag filters with anisotropy
one. The helper must be called exactly once with its unchanged `vec2f` input
and return one direct-global sample. Direct and helper samples may use literal
gradients or exact matching `dpdx(uv)` / `dpdy(uv)` values used only by that
sample. Core and HAL prove the
selected entry-point shape and recheck live bindings before drawing. Other
helper/control-flow forms and general derivative semantics remain unsupported.
Strict FL9_1 execution on the RTX 3090 Ti rendered the existing four-level
patterned mip chain through a single-mip view; direct and helper samples with
literal gradients or matching coordinate derivatives matched all 64 readback
pixels. A draw using the direct-gradient pipeline and
a multi-mip view was rejected by Core before HAL. The full public target
previously passed 169/169 in strict mode. This is selected modern-driver
evidence, not validation on historical FL9_1 hardware.
