# Direct3D 11 backend status

The opt-in `dx11` backend now enumerates physical adapters that can create an
explicit FL9_1 device and exposes the selected-device capability projection
for offscreen rendering and a constrained Win32 HWND surface through the public
wgpu API. The HWND path reports BGRA8 sRGB FIFO, opaque composition,
render-attachment usage and one frame of latency; configure, acquire and
present are restricted to the creating thread. It exposes the native-only
`TEXTURE_FORMAT_16BIT_NORM` feature only when a selected-device query reports
the required 2D format support. The native-only `TEXTURE_FORMAT_16BIT_PACKED`
feature covers query-backed single-mip RGB565, RGB5A1 and RGBA4 sampled
uploads. RGB565 additionally has queried render-target and copy/readback
support; RGB5A1 and RGBA4 remain sampled-upload-only. R8_UNORM and R8G8_UNORM
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

The optional RGBA8 2x/4x/8x paths are restricted through
`DownlevelLimits::multisampled_texture_usage_mask` to color attachment and
transient attachment usages. Core rejects sampled and copy usages before the
HAL even when the selected format reports multisample render/resolve support.

The selected FL9_1 projection exposes sampled RGBA8 3D textures only when the
device reports `TEXTURE3D | SHADER_SAMPLE`. The tested subset is POT,
single-mip, single-sample volumes up to 256³ with texture-binding and upload
usages. Forced-FL9_1 public readback on the RTX 3090 Ti verified four distinct
slices, a nonzero-offset partial voxel update and zero initialization for
untouched voxels across 128 pixels. Buffer-to-volume upload currently stages
through a mapped buffer and `UpdateSubresource`; 3D texture copies, render
attachments, storage and readback remain unavailable.

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
execution. Other bundle state combinations remain part of the API audit.
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
A tenth strict test uploads and samples Rg8Snorm and Rgba8Snorm through public
wgpu, verifies signed channel conversion in all 64 pixels, and checks that the
projected formats are filterable and single-mip-only; this draw uses nearest
sampling, while the positive selected-device `SHADER_SAMPLE` query permits any
filter.
An eleventh strict test verifies emulated per-instance vertex streams and a
nonzero first-instance range (20 green, 20 yellow and 472 blue pixels); core
rejects the unsupported `instance_index` builtin.

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
compiler errors. The FL9_1 projection also disables fragment derivatives and
explicit texture gradients; core rejects selected-entry `dpdx`, `dpdy` and
`textureSampleGrad`, including helper calls. FXC diagnostics show `ddx` cannot
map to the level-9_1 instruction set and `SampleGrad` is unsupported by
`ps_2_0`. `wgpu-info` prints these downlevel limits. Synthetic tests cover
fractional bias, derivative helpers and entry-point isolation; this does not
claim full biased or explicit-gradient sampling support.

Explicit `textureSampleLevel` is independently restricted. The selected-entry
`texture_sample_level` limit rejects reachable `SampleLevel::Exact` operations
before HAL pipeline creation, including helper calls. FXC emits X4532 for both
literal and dynamic explicit LOD under the 9_1 and 9_3 pixel profiles; implicit
sampling is unaffected. The static feature-level model marks this capability
available starting at 10_0, while this backend's current implementation flag is
false.

WGSL `textureGather` has its own selected-entry downlevel limit. HLSL Gather
is available to graphics stages at SM4.1 (FL10_1), while compute gather4
requires SM5 (FL11_0). Because the shared downlevel limit applies to every
selected entry point, the static all-stage guarantee begins at FL11_0. The
FL9_1 public projection reports it unavailable. Core follows helper calls and
rejects direct or indirect gather use before HAL in render and compute
pipelines. FXC rejects Naga's Gather lowering under the level-9_1 profile
while the shader-model-5 compiler control accepts it. The backend has no
gather execution path yet.

Texture image queries are also modeled separately. FXC rejects Naga's
`GetDimensions` output for `textureDimensions` and `textureNumLevels` with
X3548 in both tested 9_1 and 9_3 pixel profiles. The selected-entry
`texture_query` limit rejects direct and helper-reached queries before HAL;
unused query entries do not block safe siblings. The FL9_1 public projection
reports this limit unavailable, and `wgpu-info` displays it.

`textureSampleBaseClampToEdge` is gated separately: Naga lowers it through
`GetDimensions` and `SampleLevel`, which FXC rejects with X4532 in both tested
10Level9 pixel profiles. The selected-entry limit follows helper calls and
rejects this operation before HAL; the FL9_1 projection reports it unavailable.

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
R8G8_UNORM, R16G16_SNORM, R8G8B8A8_SNORM, BC1_UNORM, BC2_UNORM,
BC3_UNORM, D16_UNORM and D24S8 are always single-mip in the private
implementation. R8G8_SNORM supports sampled/upload mip chains only when the
selected device reports `TEXTURE2D | MIP`; the independent `SHADER_SAMPLE`
query controls filterability. Render-target, copy-source and readback uses
remain unsupported. RGBA8 and BGRA8, including their
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

The controlled FL9_1 profile also reports
`DownlevelLimits::vertex_index_input = false` and
`DownlevelLimits::instance_index_input = false`. Core rejects selected vertex
entries declaring either input before HAL, while a safe sibling entry remains
usable. Compiling the backend-generated `SV_VertexID` interface for the tested
`vs_4_0_level_9_1` profile fails with FXC X4555 even when the declared value is
unused. The same profile rejects `SV_InstanceID` with X4016 for direct and
structure-member inputs whether used or unused; a `vs_5_0` used-input control
compiles. The private HAL separately rejects both inputs. These limits
describe the current compiler/backend contract, not a universal claim that all
feature-level 9_1 hardware makes the semantics impossible. A strict public
wgpu test on the forced-FL9_1 adapter confirms that core rejects each selected
entry before HAL and accepts a safe sibling pipeline.

Core also models the vertex-stage sampled-texture limit separately from the
shared per-stage texture limit. The controlled FL9_1 fixture reports zero and
checks the selected shader entry point before pipeline creation reaches HAL;
the strict fixture confirms the rejection is a core validation error. This
prevents unsupported D3D11 vertex texture access from becoming a HAL device
error if a future adapter reports the current private shader subset.

The selected-entry shader audit also covers `textureLoad`. The D3D11 public
projection sets `DownlevelLimits::texture_load` to false, and core follows
validated entry-point calls, including helper functions, to reject only the
selected stages that use texel loads before HAL pipeline creation. Safe sibling
entries remain usable. The opt-in native texel-load proof remains separate and
does not relax the production shader subset.

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

The public selected-FL9_1 projection also reports that runtime integer shader
operations and stores through aggregates containing two-row matrices are
unavailable. Core derives bounded per-entry summaries from validated Naga IR,
follows helper calls, and rejects only the selected unsupported entry before
HAL. It retains direct integer-vector-to-float vertex input conversion and
safe sibling entries. Strict public validation confirms unsupported fragment
integer arithmetic and vertex matrix aggregate stores fail through core, while
the CPU analysis tests cover helper pointer stores, safe matrix reads and entry
selection. These are backend/compiler restrictions, not hardware guarantees.

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
  capabilities remain withheld.
  A strict hidden HWND clone-sampling test on the RTX at FL9_1 read BGRA
  `[0, 0, 255, 255]`, returned `Ok` from present and resized with the completed
  command buffer retained. Public hidden-HWND presentation, resize and
  cross-thread waiting are tested. Surface operation reservations track their
  owner thread; same-thread reentrant `unconfigure` logs and returns without
  waiting, while cross-thread calls still wait. Native callback delivery and
  CoreWindow/ARM32 execution remain unverified; the HAL cross-compiles for
  ARM32 UWP, but the Lumia runtime was unavailable.
- Buffer copies, initialization clears, fences, queue submission, and
  explicitly synchronized mapped CPU shadows, including transient staging.
  Core-internal storage usage bits can accompany a supported transfer buffer,
  but no storage-buffer shader binding is implemented. Core requires the
  relevant stage downlevel flags and the controlled fixture confirms unsupported
  vertex, fragment and compute storage layouts are rejected before HAL.
- Texture resources and views remain 2D-only. The internal projection retains
  inherited numeric 1D/3D limits of 2048/256 but marks both dimension
  capabilities unavailable; core rejects those texture dimensions before HAL.
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
  hardware coverage creates native samplers for requests 2, 3 and 16. The
  single-mip resource model still cannot establish anisotropic visual quality.
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
- R8G8_UNORM is not a required FL9_1 sampled format. Its one-mip,
  single-layer sampled/upload path is exposed only when the selected device
  reports `TEXTURE2D`; `SHADER_SAMPLE` controls filterability. The forced-FL9_1 RTX returned
  `0x0002c320`; a strict public upload/sample test verified 64 pixels and the
  two-byte texel stride. A second pass verifies linear filtering with an
  eight-column gradient. The selected-device `RENDER_TARGET` query also
  gates single-mip attachment use. Blendability, copy-source and direct
  readback are separately selected-query-gated.
- R8G8_SNORM's one-mip sampled/upload subset remains available when the
  selected device reports `TEXTURE2D`. A separate sampled/upload mip-chain
  path requires positive `TEXTURE2D | MIP` support. Strict
  forced-FL9_1 tests verify padded signed two-channel data and a three-level
  chain through GPU sampling. Render-target, copy-source and readback uses
  remain unsupported.
- R16G16_SNORM is not guaranteed to support shader sampling at FL9_1; the
  required-format table raises `SHADER_SAMPLE` to FL9_2. The selected device
  can still support point-only sampling when it reports `TEXTURE2D`. The RTX
  forced to FL9_1 reports `0x00021122`, without `SHADER_SAMPLE`; strict HAL
  and public tests upload, point-sample and validate readback, while core
  rejects a filtering layout from the queried format flags. Public use also
  requires `TEXTURE_FORMAT_16BIT_NORM`. The format remains single-mip,
  sampled/upload-only and is not renderable or readable back through a buffer.
- R8G8B8A8_SNORM has the same queried one-mip sampled/upload subset. A strict
  forced-FL9_1 test verifies padded signed four-channel data through GPU
  sampling and 80-pixel RGBA8 readback (one-code UNORM conversion tolerance).
  Render-target, copy-source, readback and mip-chain uses remain unsupported.
- BC1/BC2/BC3_UNORM have queried one-mip, one-layer sampled/upload subsets and
  block-aligned GPU texture-copy source paths. Dimensions, row pitches, offsets,
  updates and copy boxes align to 4×4 blocks and the format block size (8 bytes
  for BC1; 16 for BC2/BC3). Strict forced-FL9_1 tests upload, partially replace,
  copy to a second texture and sample all 256 output pixels per format. Core's
  per-format texture-to-buffer limit rejects CPU readback before HAL.
  Render-target, mip-chain and sRGB view uses remain unsupported.
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
- Public core rejects occlusion, timestamp and pipeline-statistics
  query-set creation through their missing downlevel flag or features before
  invoking unsupported HAL query creation.
- Negative and out-of-target viewports use a separately compiled vertex shader
  and a reserved constant buffer. If FXC cannot compile that variant, draws
  needing remapping are rejected; the original shader remains usable.
- Up to eight groups with thirteen static or dynamic uniform slots, eight sampled
  textures and sixteen samplers in total. The internal wgpu capability
  projection exposes six uniforms per shader stage, ensuring disjoint vertex
  and fragment bindings fit the shared thirteen-slot allocator. The public
  dynamic uniform limit is twelve per pipeline layout, the maximum disjoint
  six-vertex/six-fragment split. Strict FL9_1 public readback exercises all
  twelve sparse ranges from one source buffer and switches their selected
  offsets between draws; core rejects thirteen before HAL. A controlled core draw
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
- Sampled texture and sampler bindings accept `FRAGMENT` and
  `VERTEX | FRAGMENT` visibility, retained per binding in layout identity.
  Actual vertex-stage sampling remains rejected. Two strict RTX images covered
  fragment sampling and a layout transition; core checked 64 pixels. CPU tests
  cover stage mapping and selected-entry compiler guards. The baseline rejected
  the combined layout; historical FL9 drivers remain unverified.
- Short uniform ranges use a zero-padded native constant buffer and preserve the
  declared byte range, including non-multiples of 16. This stages through a CPU
  map and can stall; odd-sized GPU copies remain unverified on historical
  drivers. Runtime integer expressions and integer-containing uniforms or
  interfaces are conservatively rejected for selected FL9 entries in both
  checked and unchecked modes. Earlier RTX integer-shader results are
  superseded by explicit rejection tests with float-only GPU controls.
- The forced-FL9_1 private shader path executes static WGSL reads of vertex
  constant c254 and pixel constant c31. Changing the vertex value moves the
  triangle and changing the pixel value changes its color; readback distinguishes
  17 baseline green pixels and 4 shifted red pixels. Runtime integer-indexed
  uniform arrays remain rejected.
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
  Core now requires
  `DownlevelFlags::SEPARATE_ALPHA_BLEND` for distinct color/alpha equations when
  both RGB and alpha are written; RGB-only and alpha-only cases use coupled
  factors and remain accepted. A test-only opt-in retains nonportable
  diagnostics.
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
  path remains covered separately. Pipelines using `instance_index` remain
  unsupported; general instancing is not advertised.
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
enumeration and the controlled core fixture. It withholds the aggregate BC
compression feature because BC4 through BC7 remain unimplemented; isolated
private core BC tests opt into the aggregate feature only for coverage.

## Remaining public contract and completion gates

- The public adapter is FL9_1-only. The public integration
test verifies adapter selection, device/queue creation, sparse multi-group
texture/sampler and stage-visible dynamic uniform bindings, indexed textured
depth-tested rendering, transform updates, concurrent device submissions and
pixel readback. The public test is still a bounded diagnostic scene, not a
representative application or a complete contract audit.
- The narrow public HWND surface path reports BGRA8 sRGB FIFO, opaque
  composition, render-attachment usage and one frame of latency. Configure,
  acquire and present are restricted to the HWND's creating thread. A queue
  gate serializes submission, writes, mapping flushes, polling and
  context-backed resource creation against Present; same-thread reentrant work
  is rejected before queue or mapping state changes. The strict public test
  configures, clears, presents, resizes and presents again, then confirms
  wrong-thread acquisition is rejected. It uses a hidden window; the separate
  visible example exercises the message pump and desktop presentation.
- `wgpu/examples/dx11_fl91_windowed.rs` is a visible public-API example. It
  pumps Win32 messages, handles resize, and presents an animated indexed,
  textured, depth-tested cube. A 60-frame run passed on the RTX 3090 Ti at
  forced FL9_1. A live client-area capture was visually inspected; strict
  offscreen tests provide deterministic readback.
- The public capability projection and core/HAL audit still need independent
review across every valid pipeline, texture, buffer, mapping, render-pass and
error path. Current strict tests cover a broad subset but do not prove every
request permitted by the limits.
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

Fragment `@builtin(sample_mask)` output is now represented by
`DownlevelLimits::fragment_sample_mask_output`. Microsoft documents HLSL
`SV_Coverage` output as requiring `ps_4_1`; the selected FL9_1 profile is
`ps_4_0_level_9_1`. FXC rejects the output, while the color-only sibling
compiles. Core rejects only selected fragment entries that write the mask.

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

The required FL9_x packed B5G6R5, B5G5R5A1 and B4G4R4A4 formats are now
implemented as `Rgb5g6b5Unorm`, `Rgb5a1Unorm` and `Rgba4Unorm` with a shared
native-only feature and individual selected-device format queries. All three
support single-mip sampled uploads with channel/alpha readback coverage. The
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

## Query-gated R8_UNORM mip chains

R8_UNORM now supports mip chains only when the selected device reports
TEXTURE2D | MIP and the backend path is enabled. NPOT chains also require
positive unrestricted-NPOT support. Render-target and copy-source combinations
require the selected RENDER_TARGET query and matching implemented path;
SHADER_SAMPLE separately controls filterability. On the RTX 3090 Ti forced to
FL9_1, a strict public test uploaded four mips, sampled mip 2, cleared mip 0
as an attachment and read mip 0 back; all 64 RGBA pixels and all 64 R8 texels
matched. A strict public test also copied a partial rectangle at mip 1 between two mipmapped R8 textures and verified all 16 destination texels plus the preserved border; the same strict test also copied mip 1 to mip 2 and verified all four destination texels; other cross-level combinations remain unverified.

## R8 NPOT mip-chain evidence

R8_UNORM mip chains now have positive physical-device coverage for the
unrestricted-NPOT branch. The RTX 3090 Ti reported unrestricted NPOT support
while explicitly running at FL9_1. A strict public 6x10, four-level R8 chain
was uploaded and sampled at mip 2; 64/64 RGBA pixels matched, and clearing and
reading mip 0 verified all 60 R8 texels. Negative/unavailable query behavior
remains covered synthetically; historical restrictive drivers are unverified.
