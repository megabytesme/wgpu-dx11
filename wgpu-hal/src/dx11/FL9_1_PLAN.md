# FL9_1 completion plan

The objective is a usable explicit D3D11 backend through the public native wgpu
API, with truthful downlevel capabilities, tested textured depth rendering and
Windows presentation. The opt-in public adapter exposes physical FL9_1 devices
for offscreen rendering and a constrained HWND surface. Surface use is
owner-thread-only; hidden lifecycle tests and a bounded visible-window example
have run at forced FL9_1. A captured client-area frame was visually inspected:
the rotating cube showed distinct red and green textured faces over the clear
background. This plan does not claim WebGPU conformance.

## Ordered work and gates

### Verified hardware boundary — ordinary 2D texture arrays

Ordinary texture arrays are unavailable at FL9_1: Microsoft's feature-level
capability table lists texture resource arrays as unsupported for all 9.x
levels. The supported six-face cube resource is a distinct shape and does not
imply general array-view support. The public adapter reports
`TEXTURE_2D_ARRAYS` as absent, and strict public coverage verifies that a
two-layer 2D texture is rejected before HAL. No array implementation is
planned for the FL9_1 path.

[Microsoft feature-level capability table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro).

### Verified boundary — mutable format views unavailable at FL9_1

Compatible RGBA8/BGRA8 UNORM-to-sRGB view lists remain rejected by core and
`VIEW_FORMATS` stays absent. The 10Level9 FL9_1 format table lists the needed
typeless resource formats as unavailable, and a forced-FL9_1 native creation
probe returned `E_INVALIDARG` on the RTX 3090 Ti. A strict public validation
test covers both pairs. See `FL9_1_PROGRESS.md`; this does not block standalone
typed sRGB textures or ordinary same-format views.

Continue the core/HAL audit using independently reproduced public-valid
operations. Prioritize validation boundaries and lifetime/device-loss paths,
and keep optional format reports query-gated.

The current resource audit includes a narrow query-gated RGBA8 3D
sampled/upload path. Mip chains are enabled only when the selected device's
format query positively reports mip support; 3D views must include the
lowest mip as required by FL9_1. It does not enable 3D texture copies,
readback, render attachments or storage. Preserve those negative limits while
validating the public copy and shader paths.

The optional-format audit also found that a synthetic `BLENDABLE`-without-
`RENDER_TARGET` query could project blendability without attachment support.
The projection now requires the format's implemented color-attachment path, and
a synthetic RGB565 case covers the missing-render-target query. Continue
auditing cross-capability implications in optional format reporting.

The 4x RGBA8 format report also exposed a core/HAL usage mismatch: core accepted
multisampled sampled/copy descriptors from ordinary per-format usage flags,
while the D3D11 path creates only render-target resources. A dedicated
`DownlevelLimits::multisampled_texture_usage_mask` now lets core reject those
combinations while retaining query-gated render/resolve. Continue auditing
sample-count and usage intersections for other formats and backends.

Core queue submission also has a synthetic removed-device replay test. It
confirms HAL error classification, one-shot core device-lost notification,
subsequent submission rejection, queue invalidation and reentrant device polling
from the loss callback on a physical adapter opened at FL9_1. Queue submit now
defers the callback until its submission locks are released. Synthetic errors
from both fence `GetData` polling and the wait path also propagate through core;
the callback can reenter device polling without holding the snatch lock. Physical
device removal/recovery remains unverified; continue reviewing safe public error
and lifetime paths. Ordinary `device_poll` now also has a synthetic
`GetDeviceRemovedReason` failure test; its lost callback reenters polling and
later queue submissions are rejected. A separate strict HAL fence test injects
one `GetData` error, verifies the pending query is retained, then waits for that
query's real GPU completion. These tests cover error reporting and bookkeeping;
they do not simulate physical device loss or recovery.

### Public texture-view checkpoint — non-suffix 2D mip view (`4d3ec6bb9`)

The strict public sampled-mip test now creates a 2D view over source mips 1–2
of a four-level RGBA8 texture, binds it with a sampler clamped to the view's
second level and checks that the original mip-2 data is returned across all 64
pixels. This exercises the public core-to-HAL non-suffix clone-and-refresh
path; the complete strict public suite passed 51/51 on forced FL9_1 hardware.
The existing cube mip and private 2D view paths remain covered independently.

Continue the selected-entry shader and core/HAL contract audit, prioritizing
remaining observable failures reachable from valid public descriptors.

### Resource checkpoint — query-gated FL9_1 cube mip chains (`75222323e`)

The public offscreen path now accepts caller-populated mip chains for the
supported six-face, square POT RGBA8 cube subset when the selected device
positively reports format mip support. Four mips across all faces and a
non-suffix mip view were sampled on the forced-FL9_1 RTX 3090 Ti; readback
matched all 960 pixels. Unsupported formats and unavailable queries fail
closed. Cube copies, readback, render-target views, ordinary arrays and cube
arrays remain unavailable. The latest strict suites passed HAL 370/370, public
D3D11 51/51, core fixture 96/96 and DX12 core 92/92. See
`FL9_1_PROGRESS.md` for exact commands and limitations.

Next continue the selected-entry shader, texture-view and core/HAL contract
audit. Prioritize a reproducible mismatch that can be closed at the public
validation boundary, while keeping optional format/usage exposure query-gated.

### Compiler boundary — constant texture-sampling offset (9f1617eed)

WGSL 30.0.1 has no standalone textureSampleOffset builtin; its constant texel
offset form is the final argument to textureSampleBias. The existing
texture_sample_bias_offset limit and private HAL guard already reject it.
Coverage now follows direct and helper-reached uses through core, HAL and the
strict public path. Naga-generated HLSL fails FXC with X4532 under both
ps_4_0_level_9_1 and ps_4_0_level_9_3, while the SM5 control compiles. No new
shader capability or production validation path was added. The strict HAL
suite passed 368/368, public D3D11 passed 50/50, the core fixture passed 96/96
and DX12-feature core passed 92/92. See FL9_1_PROGRESS.md for the exact
commands and hardware details.

Continue auditing other core-accepted shader operations and render state
against the selected FL9_1 compiler and HAL. Keep unsupported operations gated
before HAL and do not claim historical-driver behavior from the modern RTX.

### Shader-contract checkpoint — WGSL loops (`62d12ecd4`)

Selected-entry loop detection now follows calls from the chosen render entry
point and records an explicit `DownlevelLimits::shader_loops` capability.
The D3D11 FL9_1 projection disables it because the current Naga HLSL writer
uses dynamically bounded loop lowering and FXC rejects the generated code for
both FL9_1 stages. Microsoft documents static flow control for the 9_1 vertex
profile but no flow-control support for the 9_1 pixel profile; the backend
does not yet have a tested static-loop lowering. Core rejects a selected
vertex or fragment entry that reaches a loop before HAL, while safe sibling
entries remain available. See the latest `FL9_1_PROGRESS.md` entry for strict
compiler and physical-device evidence.

Continue the selected-entry WGSL/FXC audit, prioritizing constructs that core
accepts but the current FL9_1 HLSL path cannot compile or preserve. Keep each
restriction selected-entry-aware and backed by a compiler or hardware test;
keep D3D11 explicitly opt-in and do not infer historical-driver behavior from
the modern RTX test system.

### Compiler-boundary check — `instance_index` (`70341b011`)

The previously documented unverified `instance_index` profile behavior is now
tested through Naga HLSL generation and FXC. The `vs_4_0_level_9_1` profile
rejects generated `SV_InstanceID` with X4016 for used and unused direct and
structure-member declarations; a safe sibling and a `vs_5_0` used-input
control compile. The existing selected-entry core gate remains the safe
public contract. This is profile/compiler evidence and does not establish
historical-driver behavior. Continue auditing other core-valid shader and
render-state combinations.

### Latest public binding checkpoint — independent pairs across sparse groups (`b89865ca5`)

The strict public wgpu integration binds stage-visible uniforms in group 0,
the sampled texture's sampler in group 4 and the texture in group 7. The
selected shader samples across the two resource groups. Replacing only group
7 while retaining groups 0 and 4 changes readback to 32 blue and 32
tinted-yellow pixels. The complete strict public D3D11 test binary passed
50/50 on the RTX 3090 Ti at requested/actual FL9_1; its optional Float16x2
subcase remains an explicit query-based skip. Aggregate layout edge cases and
additional group transitions still need audit. See `FL9_1_PROGRESS.md` for
the exact tests.

The sparse multi-texture test now also places two independent texture/sampler
pairs at groups 1/4 and 6/7 using sparse logical bindings. Patterned data and
nearest versus linear filtering make sampler-slot swaps observable; all 64
pixels match the checked output on forced FL9_1. A public Repeat sampler probe
was rejected by core because it cannot prove arbitrary interpolated
coordinates stay within the FL9_1 repeat bound. Keep that validation intact;
the follow-on `d921da527` checkpoint replaces only the group-7 sampler between
draws, retains the other resources and pipeline, and verifies both filtering
results across 128 pixels. Resume the broader selected-entry and core/HAL
contract audit.

### Render-bundle checkpoint — strip restart and pipeline transition (`f98b72200`)

The strict public D3D11 test records consecutive Uint16 and Uint32
TriangleStrip bundles with restart markers, each with different vertex and
fragment pipelines, then executes both in one pass at forced FL9_1. Readback
matched all 256 pixels, including the clear gap, with 37 green pixels from the
first bundle and 37 red from the second. The 50-test public suite passed on
the RTX 3090 Ti; one optional input-format subcase remains an explicit
capability skip. Broader bundle state combinations, restart topologies and
historical-driver behavior remain to be audited. See `FL9_1_PROGRESS.md` for
the commands and output.

Some status descriptions within the audit sections below preserve earlier
checkpoints. Later commits may supersede them; the opening summary and the
numbered current tasks are authoritative for the present state.

The public HWND path serializes submissions, queue writes, mapped-buffer
updates and context-backed resource creation against `Present`. Reentrant
same-thread operations that would mutate those resources are rejected, while
other threads wait for presentation to finish. Configure/acquire/present are
checked against the HWND owner thread. The public test verifies two presents
and resize on a hidden window. A visible example runs a Win32 message pump,
renders an animated indexed textured cube with depth testing, and handles
resize; a 60-frame forced-FL9_1 run succeeded. Captured-image verification
has confirmed the distinct face colors and clear background. The capture was
made from the live client area on a 150%-scaled desktop; the temporary PNG is
not a repository artifact.

The core/HAL audit also found and closed a selected-entry sampling-count gap:
FL9_1 now reports a separate limit of sixteen distinct texture/sampler pairs
per shader stage, derived from Naga's validated entry-point relationships.
Core rejects an over-limit pipeline before HAL while leaving the eight-texture
and sixteen-sampler binding capacities intact. The 16/17 boundary and sibling
entry-point selection are covered by a strict public test.

1. Preserve the tested private backend; continue uniform layout and real
   constant-budget testing. FXC and the forced-FL9_1 RTX accept a dynamically
   indexed vertex constant array of 255 float4s and reject 256 at index 256;
   pixel FXC accepts 32 and rejects 33 at register 32. Forced-FL9_1 GPU
   execution now verifies that WGSL access to vertex constant c254 changes
   geometry and pixel constant c31 changes fragment color. Runtime integer
   indexing remains rejected, so dynamic near-limit indexing is still
   unavailable. Core pipeline override constants are now resolved through
   Naga before selected-entry checks and FL9_1 shader compilation; strict
   hardware readback covers explicit/default values and invalid overrides for
   both vertex and fragment stages.
   Strict readback also covers nested structures, an asymmetric matrix,
   stride-sensitive arrays and padding. Runtime integer indexing remains
   unavailable. The selected-device core projection permits twelve dynamic
   uniform buffers per pipeline layout, the largest disjoint six-vertex plus
   six-fragment split within the native allocator. Strict public readback
   exercises all twelve sparse bindings and switches subranges between draws;
   core rejects thirteen before HAL. Continue checking shader instruction pressure and other
   untested layout shapes with checked translation. The selected FL9_1
   projection now reports 255 vertex and 32 fragment uniform float4 vectors;
   core conservatively counts complete statically used uniform layouts per
   selected entry point. Strict public readback used all 32 fragment vectors
   across two sparse bindings, while core rejected 33 before HAL. Per-stage
   aggregate validation is implemented. The public binding-size limit now
   allows the 4,080-byte vertex c0-c254 range; strict readback changes geometry
   from c254 and core rejects a 256-vector vertex shader. Fragment shaders
   remain limited to 32 vectors. Broader WGSL packing and instruction-budget
   cases still need coverage.
2. Extend depth/stencil coverage from the implemented D16 depth-only and D24S8
   attachment paths. D16 uses a required-format guarantee intersected with a
   selected-device query and is single-mip; strict public tests verify depth
   ordering in both draw orders, `DepthOnly` view use, rejection of stencil and
   multi-mip use, preservation across a second-pass LOAD and reset by CLEAR.
   A discard followed by LOAD also verifies that stale depth does not affect
   rendering. Public D24S8 tests now verify both aspects survive LOAD, each
   aspect's CLEAR independently changes later comparisons, and each aspect's
   DISCARD followed by LOAD is initialized before use. Sampled D24S8 is
   rejected through core; public core rejects depth-only/stencil-only D24S8
   views while retaining the combined attachment, and the D16 test rejects its
   unsupported stencil-only view. Continue auditing less common render-pass
   combinations and format aspects before widening public claims.
3. Broaden pipeline and resource functionality: vertex layouts, topology,
   indexed semantics, render state, mips/formats and multiple groups. Dynamic
   uniform offsets have restricted private coverage. A controlled core test
   still exercises a two-group fixture pipeline (uniform in group 0, sampled
   resources in group 1). The strict public integration now also binds
   uniforms in group 0, a sampler in group 4 and a texture in group 7, then
   replaces group 7 alone and verifies the new sample output. Broader
   aggregate group layouts remain to be audited. All wgpu vertex formats with
   direct DXGI input equivalents are
   now mapped except Float64 variants. Outside the ten FL9_1-required IA
   formats, core exposes a format only when the selected device positively
   reports `IA_VERTEX_BUFFER`; the forced-FL9_1 RTX reports no optional input
   formats, so optional-format GPU execution remains unverified. Each feature
   needs positive GPU and negative tests. Non-indexed
   line lists and line strips now have a
   bounded private FL9_1 implementation with strict GPU readback and the
   controlled core fixture verifies both through core command recording and
   readback. Indexed Uint16/Uint32 line lists now preserve line pairs through
   compact rebasing and wide-pair gathering, with strict HAL GPU output and a
   controlled core fixture. Indexed LineStrip now splits format-specific
   restart markers into independent runs, preserves connectivity at native
   draw limits, rebases bounded windows and gathers wide edges. Restart
   markers are matched after signed base-vertex resolution, following the
   current WebGPU index-resolution and primitive-assembly algorithms. Indexed
   TriangleStrip now splits the resolved markers, expands each run with
   alternating winding into bounded triangle-list draws, and gathers triangles
   whose index span exceeds the native FL9_1 window. Strict private readback
   compares both index formats against equivalent triangle lists across front
   face and cull settings; the controlled core fixture verifies both formats.
   Indexed LineList and TriangleList with only instance-rate or no vertex-rate
   inputs now validate the index range and replay a matching nonindexed draw
   sequence; strict private and public readbacks compare Uint16/Uint32 cases to
   direct draws. Indexed LineStrip and TriangleStrip now read the execution-time
   index contents, split effective restart runs, and replay matching per-run
   nonindexed strips when no vertex-rate inputs exist. Strict tests cover both
   index widths, layouts and strip topologies. PointList retains its separately
   tested instance-only path. `vertex_index` and `instance_index` remain
   unsupported.
   Broader instance semantics and indirect draws remain unavailable. Microsoft
   marks native `DrawInstancedIndirect` and `DrawIndexedInstancedIndirect` as
   unsupported at 9_1; wgpu-core's `INDIRECT_EXECUTION` flag is also coupled to
   compute support and GPU argument validation, so the public validation path
   cannot currently expose a private CPU-staging emulation independently.
   R8_UNORM is implemented as a queried single-mip sampled/upload format. Its
   2D upload and nearest sampling pass on forced FL9_1. R8G8_UNORM, which is
   not a required FL9_1 sampled format, is exposed only after a positive
   selected-device `TEXTURE2D` query; `SHADER_SAMPLE` separately controls
   filterability. The RTX returned `0x0002c320`, and strict public
   upload/sample readback passed. R8_UNORM (`0x0002d3e0`) and R8G8_UNORM now
   additionally expose single-mip render-attachment use after their selected
   format query reports `RENDER_TARGET`; strict public draw-then-sample
   readback now covers R8_UNORM, R8G8_UNORM and B5G6R5 with all 64 pixels per
   format checked. B5G6R5 requires `TEXTURE_FORMAT_16BIT_PACKED`. R8_UNORM,
   R8G8_UNORM and B5G6R5 now also expose query-gated GPU copies and CPU
   readback when `RENDER_TARGET` is reported. Their blend capability is
   separately gated by `BLENDABLE`; strict public FL9_1 readback verifies
   identical-equation alpha blending on the RTX for all three. These remain
   single-mip. Other narrow formats remain without copy-source/readback paths.
   The required
   FL9_1 IA formats now include `Uint8x4`, `Sint16x2` and `Sint16x4` alongside
   the previously mapped formats, with a core per-format downlevel mask and
   strict GPU verification. Required RGBA8_SRGB and BGRA8_SRGB formats now
   have independent device queries and public sampled/render-target paths;
   strict FL9_1 readback verifies decoding and encoding for both channel
   orders. The public suite also samples four-level POT chains of both formats
   and verifies implicit LOD selects mip 1. The required FL9_1 table
   guarantees R16G16_SNORM as a 2D texture but requires `SHADER_SAMPLE` only
   from FL9_2. The RTX forced to FL9_1 returns `0x00021122`: it supports the
   2D resource but lacks any-filter support. Private and public tests verified
   point-filtered upload/sample/readback while core rejects a filtering bind.
   The public adapter exposes `TEXTURE_FORMAT_16BIT_NORM` only after the
   selected query establishes 2D support. The Windows 8 FL9_x packed formats
   B5G6R5, B5G5R5A1 and B4G4R4A4 now have individual selected-device queries
   and single-mip sampled/upload support behind `TEXTURE_FORMAT_16BIT_PACKED`;
   forced-FL9_1 public readback covers all three channel layouts and alpha.
   B5G6R5 also has query-gated single-mip render-target support; a public
   shader draw, sample and readback checks every pixel. The RTX reports
   `0x0026f3a0`, including BLENDABLE. R8_UNORM and R8G8_UNORM likewise report
   the optional bit. The public projection now exposes it only when the
   selected query and implemented pipeline path agree; a strict overlay test
   validates all three. Continue the format and usage audit from the actual
   query table.
4. Resolve core/HAL contract mismatches before exposure: core now models
   `viewport_rectangles_outside_target` and rejects clipped rectangles for the
   conservative FL9_1 fixture before HAL, because the optional remapped vertex
   shader can exceed FL9 compiler limits. The private HAL's fractional
   viewport remap is now readback-tested, including the core path with
   contained geometry; the native FL9_1 viewport stays integer-bounded and
   the shader transform preserves logical fractional coordinates.
   Historical-driver precision remains unverified. DXGI device-loss classification is
   centralized and covered for reset, removal, driver-internal and OOM codes;
   continue auditing mapping,
   teardown and remaining device-loss panic paths. Empty compute
   passes and unused blend constants are handled; compute dispatch remains
   unavailable. The downlevel combined-index usage rule has been corrected,
   and unavailable occlusion query sets are rejected by a downlevel flag.
   Depth-only clears and restricted draws work privately; handle read-only
   depth hazards and audit the remaining general render-pass combinations.
   Active asymmetric stencil state is now rejected by core unless the adapter
   reports `DownlevelFlags::TWO_SIDED_STENCIL`; the D3D11 fixture omits it at
   FL9_1. Comparison-sampler creation now similarly requires
   `DownlevelFlags::COMPARISON_SAMPLERS`, and the controlled fixture verifies
   rejection before HAL. Full-channel differing RGB/alpha blend equations now
   require `SEPARATE_ALPHA_BLEND`; the FL9_1 fixture checks this while
   preserving the masked-channel emulation path. Continue mapping core-valid
   render state and format combinations to explicit downlevel or format
   capabilities before considering production enumeration. Storage-buffer
   layouts now require the matching vertex, fragment or compute downlevel flag.
   The HAL accepts core-internal storage usage bits only alongside another
   supported buffer role; it still does not implement shader storage access.
    Texture-view layout and creation validation now requires explicit
    downlevel support for 2D arrays and cube-array views. The D3D11 projection
    leaves those flags absent and verifies rejection before HAL. The supported
    FL9_1 cube subset is query-gated by `CUBE_TEXTURES`, a separate cube limit
    and per-format queries; strict public rendering samples all six RGBA8
    faces. D1/D3 binding layouts are rejected when their texture dimension
    limits are zero, and storage-texture D2Array layouts share the array gate.
   The selected-entry audit also models `textureLoad` as a downlevel limit and
   rejects only entries that reach a load through their call graph; safe sibling
   entries remain usable. Continue auditing other WGSL operations accepted by
   core but rejected by the private FL9_1 compiler. The public FL9_1 projection
   now models the FXC bias subset (`abs(literal) <= 3`, no dynamic value or
   offset), and selected-entry core validation follows helper calls so
   unsupported bias forms fail before HAL. Strict public readback verifies
   implicit sampling and -3/+1/+3 literal bias mip selection. Continue the
   shader audit for additional core-accepted compiler failures. The
   `ps_4_0_level_9_1` projection also disables pixel derivatives and explicit
   texture gradients; selected-entry core validation follows helper calls and
   rejects `dpdx`, `dpdy` and `textureSampleGrad` before HAL.
   Reachable `textureSampleLevel` calls are separately gated by the selected
   entry point: FXC rejects both literal and dynamic exact LOD under the 9_1
   and 9_3 pixel profiles, including when forwarded through a helper. Implicit
   sampling remains allowed. Selected-entry `textureDimensions` and
   `textureNumLevels` use is now gated by a downlevel limit; the current FL9_1
   HLSL compiler path rejects image-query lowering before HAL, while safe
   sibling entries remain usable. `textureSampleBaseClampToEdge` has its own
   selected-entry gate because its HLSL lowering uses image queries and
   explicit-level sampling; both 9_1 and 9_3 pixel profiles reject it with
   FXC X4532, including through a helper. Continue checking other WGSL
   expressions against the FL9_1 compiler path. Fragment
   `@builtin(sample_mask)` output is separately gated: Naga lowers it to
   `SV_Coverage`, which Microsoft documents as writable only from `ps_4_1`,
   while FL9_1 compiles `ps_4_0_level_9_1`. FXC rejection and the selected-entry
   core error are tested; a possible single-sample discard emulation remains
   unimplemented. Fragment `@builtin(sample_index)` is already rejected at
   WGSL/Naga validation because the FL9_1 projection omits
   `MULTISAMPLED_SHADING`; a strict public regression now confirms this path
   and acceptance of a single-sample shader. FXC rejects fragment
   `@interpolate(flat)` and `@interpolate(linear)` because their HLSL forms
   `nointerpolation` and `noperspective` fail with X4568 under
   `ps_4_0_level_9_1`; core now gates only the selected fragment entry point.
   Vertex outputs with these modes and fragment centroid interpolation compile.
   A strict public render confirms centroid and perspective produce identical
   single-sample readback. The previously compilation-only structured
   color-plus-`frag_depth` case now has strict public depth-test readback:
   fragment depth overrides contradictory vertex depth in both draws as
   expected. WGSL `textureGather` is now a
   stage-agnostic downlevel limit. Microsoft documents HLSL Gather for
   graphics at shader model 4.1 (FL10_1+) and compute gather4 at SM5 (FL11_0);
   the all-stage guarantee therefore begins at FL11_0. FXC rejects the FL9_1
   lowering with X4532, while `ps_4_1`, `ps_5_0`, and `cs_5_0` controls compile.
   Core rejects direct/helper use in selected render and compute entries while
   compiling an implicit-sampling sibling. Continue auditing other core-valid
   operations against the selected FL9_1 compiler.
   Single-sample output masks now reach D3D11; zero WebGPU masks map to an
   equivalent nonzero FL9_1 native mask. RGBA8 4x render-and-resolve is
   available only when selected-device format and sample-count queries
   positively establish support; multisampled sampling and depth remain gated.
   Four-sample resolve does not imply alpha-to-coverage: a dedicated
   `DownlevelLimits::alpha_to_coverage` capability is false for FL9_1, and the
   controlled core fixture verifies rejection before HAL. Next close the
   selected-entry dynamic-loop/FXC mismatch and mapped `BufferView`
   destruction lifetime issue found in the continuing audit.
   Core texture creation now requires `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`
   for NPOT spatial dimensions when more than one mip is requested; the D3D11
   implementation remains POT-only even when the runtime reports full NPOT.
   The private single-target blend path now translates ordinary wgpu factors
   and all five operations, including blend constants; it rejects FL9_1
   `SRC_ALPHA_SAT`, dual-source factors and full-channel distinct RGB/alpha
   equations unless the latter are explicitly established. Strict forced-9_1
   readback covered custom constant factors, while CPU translation tests cover
   every supported factor/operation pair. Continue auditing other core-valid
   render-state combinations and shader limitations. Core now models the
   10Level9 `SRC_ALPHA_SAT` restriction as a downlevel capability and rejects
   all factor positions before HAL; continue the vertex-format and attachment
   audit before preparing the public capability snapshot.
   Raster depth comparison now maps all seven native D3D11 comparison functions;
   `Never` keeps its validated-draw and stencil-depth-fail emulation. A strict
   forced-FL9_1 render verifies pass and fail pixels for all eight wgpu compare
   functions. The 10Level9 `LessEqual` restriction is for comparison samplers,
   not raster depth state. FL9_1 triangle pipelines also pass finite slope-scale
   bias to the native rasterizer with the required zero clamp; strict readback
   verifies positive/negative slope effects. Solid points and lines remain
   rejected for nonzero bias because bias has no effect on them. Historical
   FL9_1 drivers remain unverified. All eight stencil operations are now
   verified on pass, stencil-fail and depth-fail outcomes with strict FL9_1
   readback; asymmetric front/back states remain rejected. Core now gates
   multi-mip depth/stencil attachments through a dedicated downlevel flag,
   matching the FL9_x D3D11 requirement that depth/stencil-bound resources use
   one mip. Vertex-stage sampled texture access now has a separate downlevel
   limit. Core counts resources used by the selected vertex entry point and
   rejects them before HAL; the controlled FL9_1 fixture reports zero and
   verifies that result. Conditional NPOT repeat use is represented by a
   downlevel capability and checked against Naga's selected-entry sampling
   pairs in core when the reported repeat limit allows such samplers. FL9_1's
   fixed repeat cap is 128, so the controlled core fixture now derives that
   value from the selected level and rejects Repeat and MirrorRepeat sampler
   creation: core cannot prove interpolated coordinate ranges for every draw.
   The fixture separately renders POT and NPOT clamp-to-edge resources. CPU
   tests cover the conditional NPOT relationship path under synthetic
   unrestricted-repeat settings; private HAL tests exercise bounded repeat.
   The RTX forced to 9_1 does not validate legacy overflow behavior. Test selection
   now checks downlevel limit requirements as well as flags. Next continue
   auditing core-valid attachment, viewport and render-state combinations
   before building the truthful public capability snapshot. The audit also
   found that core permits RGBA8/BGRA8 linear-to-sRGB texture copies while HAL
   required exact DXGI format equality. The D3D11 copy path now accepts only
   the corresponding same-family linear/sRGB pairs; strict FL9_1 readback
   verifies raw-bit preservation for both channel families.
5. Implement a truthful hardware adapter, device limits/features/downlevel and
   per-format reports. Audit every core-valid command against HAL behavior;
   restrict safely or implement/emulate it. No placeholder capabilities.
   An FL9_1-only wgpu-types projection now feeds the controlled core fixture
   and ordinary opt-in adapter enumeration, including selected-device format
   reports and explicit unsupported 1D/3D shape gates. A strict public wgpu
   integration test selects the physical RTX 3090 Ti, requests devices within
   the projection, uploads and samples a texture at sparse bindings, binds
   separate vertex transform and fragment tint uniforms in group 0, a sampler
   in group 4 and a sampled texture in group 7 of an eight-position layout,
   and reuses binding indices across those groups. Replacing only the texture
   group changes readback while the sampler and uniform groups remain bound.
   It performs indexed depth-tested draws, updates the
   transform between submissions using a dynamic uniform offset, and verifies
   both images by readback. Continue auditing the full core/HAL contract before
   widening the projection.
   Logical binding indices remain capped by the wgpu index limit while actual
   resource counts use separate backend limits. The projection exposes six
   uniform buffers per shader stage so disjoint vertex/fragment bindings fit
   the shared thirteen-slot HAL allocator; strict core GPU coverage exercises
   two independent fragment uniforms, a vertex uniform alongside a fragment
   uniform through the same selected-device projection, and rejects seven in
   one stage. The aggregate BC feature stays
   withheld because only BC1 is implemented. Continue the descriptor and
   feature/format audit before treating the exposed subset as complete.
6. Keep the narrow HWND surface contract safe and tested alongside CoreWindow.
   Public operations serialize queue submission, writes, mapping flushes,
   polling and context-backed creation against `Present`; same-thread
   reentrant mutations fail safely, while other threads wait. Configure,
   acquire and present enforce window-thread affinity. Strict hardware tests
   cover hidden-window configure/present/resize and wrong-thread rejection; a
   bounded visible example presents an animated textured indexed cube for 60
   frames at forced FL9_1. A captured live client area has been visually
   inspected, and the controlled core fixture holds the presentation guard
   while verifying same-thread queue write/submit, poll, buffer creation and
   texture creation behavior. Continue testing lifecycle/error and remaining
   reentrancy edges. `unconfigure` detects same-thread reentry and returns
   without waiting, while other threads continue to wait for the active
   operation; CPU tests cover both reservation behaviors. Synchronous native
   callback delivery remains unverified. Preserve the CoreWindow path;
   Windows 10 Mobile ARM32 remains unverified. Strict hidden-HWND HAL tests
   also verify that same-surface `configure` and `acquire_texture` fail
   promptly while another surface operation owns its operation guard; normal
   present/resize remains functional afterward.
7. Audit unsupported operations, synchronization, lifetime and device loss;
   run strict GPU, core, Naga and non-D3D11 regressions and relevant downlevel
   conformance cases. Independently review the public contract. The upstream
   `wgpu-gpu` Naga capability selector now uses HLSL's capability ceiling for
   the D3D11 backend instead of assuming no DX11 adapter can be selected; the
   focused selector ran against the physical FL9_1 adapter.

## Completion evidence

The public example must select a physical D3D11 adapter, report requested and
actual FL9_1, execute and read back representative depth-tested rendering, and
present to an HWND where an interactive desktop is available. Publicly
advertised operations must be implemented or rejected by valid core rules.
Document unavailable hardware features separately from missing implementation,
and historical hardware separately from modern GPUs forced to FL9_1.

Run only one heavyweight Cargo process at a time with bounded build jobs.
Record exact commands, hardware results, skips, failures and commit hashes in
FL9_1_PROGRESS.md. Maintain FL9_1_HANDOFF.md for interruption and final audit.

Latest sampled-format increments: R8_UNORM (`268431ec3`), R8G8_SNORM
(`881640796`), R8G8B8A8_SNORM (`312dba141`) and BC1_UNORM (`6c67e22be`)
use queried, single-mip sampled/upload paths and strict FL9_1 GPU readback.
BC1 upload uses 4x4 blocks and 8-byte block rows, including padded pitches,
nonzero source offsets and aligned partial updates. BC1 copy-source, render
target, readback, sRGB and mip-chain uses remain unsupported. The controlled
core fixture now validates BC1 through texture creation, queue upload, binding,
rendering and readback, and creates its device via `adapter_request_device`
and the test-only HAL `Adapter::open`. It still injects hand-selected limits
and does not by itself represent the public adapter contract. The opt-in
production adapter now uses that same selected-device projection for
offscreen-only public wgpu access. Continue auditing the public HAL/core
capability boundary and every core-legal operation; do not widen the current
projection until those operations are implemented or rejected. Keep HWND
surface capabilities unavailable. Latest code checkpoints `983eea619` and
`834cd9856` add core validation and cross-backend reporting for multi-mip
depth/stencil attachments. The earlier `6f7f12978` added native FL9_1
slope-scaled triangle depth bias; checkpoint
`1a362ec6108de9accb52e2f1978a174d1f923106` modeled hardware bias guarantees
separately from backend support across all seven feature-level identities.
The controlled core fixture now validates the per-format `SINGLE_MIP_ONLY`
capability for R8_UNORM, R8G8_SNORM, R8G8B8A8_SNORM, BC1_UNORM and D24S8;
POT multi-mip descriptors are rejected before HAL creation while one-mip
sampled-upload cases remain available. This closes the format-specific gap
left by the separate NPOT-mipmap and depth-attachment gates.

Per-format projection is centralized in
`Capabilities::texture_format_capabilities`, which intersects positive
selected-device format queries with feature-level shape checks and implemented
backend paths. Both the opt-in production adapter and the test-only core
fixture delegate to this internal method. Projection tests cover missing/
negative queries and unsupported backend paths. Continue the descriptor and
render-pass audit for core-valid requests the private HAL cannot execute; the
public offscreen route remains experimental and does not complete this audit.

The latest audit found a core/HAL pipeline mismatch: core permits nonzero depth
bias when the selected fragment entry point writes `frag_depth`, while the
private HAL rejects it. A narrow selected-entry validation gate now rejects
that case before HAL pipeline creation, while unused depth-writing entries do
not trigger it. The per-format RGBA8/BGRA8 mip projection honors missing driver
MIP support and disabled backend mip paths; the strict RTX fixture reports MIP
support for both formats at FL9_1. Public HWND surface exposure remains gated;
the offscreen adapter uses this projection.

The selected-entry depth-bias mismatch is now represented in core's
`DownlevelLimits` and covered by the controlled fixture. Next audit remaining
core-valid depth/stencil and render-pass combinations. The tested FL9_1 FXC
profile also rejects `SV_Depth` without `SV_Target0` at zero bias with X4530;
the new `fragment_depth_without_color_target_0` limit makes core reject only
that selected output shape before HAL, while structured color-plus-depth
remains compilable. Continue checking core-valid shader output shapes against
FXC before widening the projection. The tested FL9_1 FXC profile also rejects
fragment `@builtin(front_facing)`, and Microsoft documents
that 9_x pixel shaders cannot read fragment `@builtin(position)`. Core now
represents these with selected-entry `DownlevelLimits` gates, so safe sibling
entries remain usable. Continue auditing shader and render-state combinations
against the currently exposed projection; do not widen it until every permitted
operation is implemented or safely rejected. Public surface capabilities
remain unavailable.

The selected-entry audit now also covers vertex system-value inputs. The
controlled FL9_1 profile reports `vertex_index_input` and `instance_index_input`
as unavailable; core rejects declarations before HAL and permits safe sibling
entries. The backend-generated `SV_VertexID` interface fails to compile with
FXC X4555 under the tested FL9_1 vertex profile; `SV_InstanceID` fails with
X4016 for direct and structure-member inputs, used or unused. The safe
instance-index sibling compiles at FL9_1 and a used input compiles with the
`vs_5_0` control. These are compiler/backend boundaries, not universal
hardware guarantees. The opt-in adapter reports the selected FL9_1
projection; a strict public test confirms both built-ins are rejected before
HAL while a safe sibling pipeline compiles.
The current public surface remains limited to the HWND contract summarized at
the top of this plan. Continue the core/HAL audit before widening the public
capability contract.

The public offscreen integration now exercises sparse group-0 uniforms, a
group-4 sampler and group-7 sampled texture, group replacement, indexed
textured depth-tested output, dynamic uniform updates and two concurrently
submitting public devices. Its error
scopes additionally verify that core rejects vertex-visible storage-buffer
layouts and D2Array sampled-texture layouts, alongside unsupported 3D,
multisampled, format and mip requests. These checks expand the tested public
validation boundary but do not complete the core/HAL audit or the required
desktop presentation path.

The same public test now overlays near and far textured geometry in both draw
orders and verifies that all pixels show the nearer surface. This establishes
depth-test ordering through public wgpu; a representative desktop application
and presentation are still outstanding.

The public adapter projection's one-mip R8Unorm sampled/upload path is also
render-tested: a non-filtering sample from two uploaded values produced the
expected 64-pixel split. Continue validating other projected format/usage
combinations at the public boundary.

The indexed public scene now fills its vertex buffer through a mapped-at-
creation range before unmapping and drawing; successful FL9_1 readback
verifies the staged mapping contents reached GPU vertex input.

The public offscreen integration now also samples two independently uploaded
textures at sparse bindings 7 and 42 through one sampler at binding 100. The
layout, WGSL declaration and bind-group entry orders differ; the strict GPU
test verifies all 64 pixels contain both contributions and public core rejects
an incomplete group. Continue the exact core/HAL audit and broaden public
render coverage. HWND presentation remains gated on safe cross-layer
serialization and reentrancy handling.

The public path now exercises the entire projected eight-texture fragment
capacity in one shader. Each sparse-bound uploaded texture affects every
checked pixel; a ninth fragment texture is rejected by core before HAL. This
verifies the current count boundary on the RTX forced to FL9_1, not historical
driver behavior or every shader's instruction budget.

A public transfer test now sends a nonzero-offset, padded-row mapped buffer
through a partial buffer-to-texture update, copies a 2×2 source box between
different texture dimensions at nonzero origins, and reads all 36 destination
texels back through a padded buffer layout. The test also verifies untouched
destination texels retain their prior values.

The public RGBA8 sampled path now has a four-level POT mip-chain execution
test. Distinct colors were uploaded to each level and implicit derivatives
selected level 1 across the full 8×8 output. Core also rejects a multi-mip R8
descriptor according to that format's `SINGLE_MIP_ONLY` report.

Public Rg8Snorm and Rgba8Snorm capabilities now have upload, sample and
conversion readback coverage through ordinary wgpu. The exact selected-device
reports remain non-filterable and single-mip-only; all 64 output pixels matched
the signed-to-unorm shader conversion.

Public vertex input coverage now includes emulated per-instance streams with a
nonzero `first_instance`; readback verifies the selected instance records and
the skipped record. The public selected-entry gate rejects `instance_index`,
which the FL9_1 shader path does not implement.

## Current checkpoint — outputless fragment with depth and stencil

The public FL9_1 projection already allowed an outputless fragment stage with
no color targets and a depth attachment, but the D3D11 HAL rejected that
pipeline shape. Direct Naga HLSL output failed with FXC X4530 because the
`ps_4_0_level_9_1` profile requires `SV_Target0`. The HAL now retains the
validated fragment body as a helper and adds a synthetic zero color result to
the entry point. The pass binds no RTV, so the added color has no observable
destination; discard and depth behavior remain active. A strict public D16
test uses conditional discard, then draws farther geometry in a color pass.
A D24S8 test verifies that the discard also suppresses stencil replacement.
Separate later stencil and depth comparisons validate 32 pixels on each half
of both readback images. The compiler test covers a location input, a helper
call and a sparse uniform mapping. A color-returning fragment with an
unmatched location-0 output is also accepted in a targetless D16 pass; a later
depth comparison verifies the output is ignored while depth writes remain.
The full strict public suite passed 38/38,
including the D24S8 case, on the physical RTX 3090 Ti at requested/actual
FL9_1. Historical FL9_1 profile behavior remains unverified.

The public FL9_1 projection now has separate downlevel limits for runtime
integer shader operations and stores through aggregates containing two-row
matrices. Core analyzes validated Naga IR per selected entry, follows helper
calls and pointer arguments, and rejects unsupported paths before HAL while
preserving direct integer-vector vertex-input conversion and safe matrix
reads. Public error-scope checks and CPU analysis tests cover both gates. The
first full public-suite run exposed an error-precedence regression for fragment
sample-mask output; the dedicated sample-mask gate now runs before the broad
integer gate, and the full suite passes. Next continue auditing core-valid
shader and render-state combinations against the actual HAL. Keep optional
formats query-gated and historical FL9_1 and ARM32 behavior explicitly
unverified.

The public command path also has strict render-bundle execution tests: core
records a nonindexed triangle, an indexed textured triangle and Uint16/Uint32
indexed TriangleStrips with primitive-restart markers. Each restart bundle
renders two separated shapes and checks all 256 pixels, including the clear
gap. Other bundle state combinations and restart topologies still need
targeted coverage.

## Follow-up — mapped-view destruction safety

The safe public `Buffer::destroy` API now checks the shared `MapContext` before
core can unmap and release the backend allocation. `MapContext::reset` checks
for active subranges before clearing its mapped range, so a caught panic keeps
the map state usable. A strict FL9_1 test verifies two disjoint views remain
readable after a rejected destruction and that destruction succeeds once both
views are dropped. The selected-entry dynamic-loop/FXC mismatch is now gated
and tested. Continue with a public sampler-group transition test, then resume
the selected-entry and core/HAL contract audit.

## Current increment — query-gated FL9_1 cube sampling

The opt-in public adapter exposes the 10Level9 six-face cube-resource subset
through a separate 512-pixel cube limit and selected-device per-format
`TEXTURECUBE` queries. Ordinary 2D arrays and cube arrays remain disabled.
Core rejects unsupported cube shapes, formats and views before HAL. A strict
public test uploaded a different RGBA8 color to each face and sampled all six
axis directions at explicitly requested/actual FL9_1; all 96 readback pixels
matched. The RTX 3090 Ti reported cube format mask `0x1f1f`, and core rejected
RG8 cube creation because the selected format query did not include cube
support. Cube copies and render-target views remain unavailable. Continue the
core/HAL audit; do not infer historical FL9_1 support from this modern driver.
