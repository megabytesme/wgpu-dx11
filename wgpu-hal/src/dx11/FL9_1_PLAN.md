# FL9_1 completion plan

The objective is a usable explicit D3D11 backend through the public native wgpu
API, with truthful downlevel capabilities, tested textured depth rendering and
Windows presentation. The opt-in public adapter now exposes physical FL9_1
devices for offscreen rendering; public surfaces remain disabled until their
contract is safe. This plan does not claim WebGPU conformance.

## Ordered work and gates

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
   unavailable. The selected-device core projection also permits two dynamic
   uniform buffers per pipeline layout; strict readback exercises distinct
   vertex- and fragment-visible offsets, and core rejects three at bind-group
   layout creation. Continue checking shader instruction pressure and other
   untested layout shapes with checked translation. The selected FL9_1
   projection now reports 255 vertex and 32 fragment uniform float4 vectors;
   core conservatively counts complete statically used uniform layouts per
   selected entry point. Strict public readback used all 32 fragment vectors
   across two sparse bindings, while core rejected 33 before HAL. Per-stage
   aggregate validation is implemented; broader WGSL packing and instruction
   budget cases still need coverage.
2. Extend depth/stencil coverage from the implemented D16 depth-only and D24S8
   attachment paths. D16 uses a required-format guarantee intersected with a
   selected-device query and is single-mip; strict public tests verify depth
   ordering in both draw orders, `DepthOnly` view use, rejection of stencil and
   multi-mip use, preservation across a second-pass LOAD and reset by CLEAR.
   A discard followed by LOAD also verifies that stale depth does not affect
   rendering. Continue testing independent store/discard behavior, unsupported
   views and remaining render-pass combinations before widening public claims.
3. Broaden pipeline and resource functionality: vertex layouts, topology,
   indexed semantics, render state, mips/formats and multiple groups. Dynamic
   uniform offsets have restricted private coverage. A controlled core test
   now exercises one two-group pipeline (uniform in group 0, sampled resources
   in group 1); broader aggregate group layouts and transitions remain to be
   audited. Each feature needs positive GPU and negative tests. Non-indexed
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
   2D upload and nearest sampling pass on forced FL9_1; color-target,
   copy-source, readback and mip-chain uses remain unsupported. The required
   FL9_1 IA formats now include `Uint8x4`, `Sint16x2` and `Sint16x4` alongside
   the previously mapped formats, with a core per-format downlevel mask and
   strict GPU verification. Required RGBA8_SRGB and BGRA8_SRGB formats now
   have independent device queries and public sampled/render-target paths;
   strict FL9_1 readback verifies decoding and encoding for both channel
   orders. The public suite also samples four-level POT chains of both formats
   and verifies implicit LOD selects mip 1. Continue adding narrowly verified
   required formats and usage combinations.
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
   Texture-view layout and creation validation now also requires explicit
   downlevel support for 2D arrays, cube views and cube-array views. The D3D11
   fixture leaves those flags absent and verifies rejection before HAL. D1/D3
   binding layouts are rejected when their texture dimension limits are zero,
   and storage-texture D2Array layouts now share the array downlevel gate.
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
   and acceptance of a single-sample shader.
   Single-sample output masks now reach D3D11; a zero WebGPU mask maps to an
   equivalent nonzero FL9_1 native mask. Multisampling remains gated.
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
   before building the truthful public capability snapshot.
5. Implement a truthful hardware adapter, device limits/features/downlevel and
   per-format reports. Audit every core-valid command against HAL behavior;
   restrict safely or implement/emulate it. No placeholder capabilities.
   An FL9_1-only wgpu-types projection now feeds the controlled core fixture
   and ordinary opt-in adapter enumeration, including selected-device format
   reports and explicit unsupported 1D/3D shape gates. A strict public wgpu
   integration test selects the physical RTX 3090 Ti, requests devices within
   the projection, uploads and samples a texture at sparse bindings, binds
   separate vertex transform and fragment tint uniforms in group 0, sampled
   resources in group 7 of an eight-position layout, and reuses binding indices
   across those groups. It performs indexed depth-tested draws, updates the
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
6. Complete HWND surface lifecycle while preserving CoreWindow; the private
   hidden-window path and retained-command-buffer resize are tested, but
   concurrent presentation requires work. Microsoft documents that DXGI may
   synchronously send window messages during `Present` and wait for the owning
   message-pump thread. The current core queue-index lock and HAL immediate-
   context mutex span `Present`; a callback that reenters wgpu can deadlock.
   Releasing these locks permits context/DXGI concurrency, which Microsoft
   documents as invalid. A safe cross-layer scheduling or thread-affinity
   design is unresolved; do not assume a cooperative window procedure or
   enable public surface support until it is resolved. A standalone public
   wgpu offscreen example now selects the physical FL9_1 adapter, renders an
   indexed textured depth-tested cube and verifies full-image readback. A
   desktop-window example and presentation remain outstanding until the
   cross-layer scheduling and lock-order design is resolved.
7. Audit unsupported operations, synchronization, lifetime and device loss;
   run strict GPU, core, Naga and non-D3D11 regressions and relevant downlevel
   conformance cases. Independently review the public contract.

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
FXC X4555 under the tested FL9_1 vertex profile. This records the
compiler/backend boundary, not a hardware guarantee. `instance_index` remains
a backend restriction whose FXC acceptance is unverified. The opt-in
offscreen-only adapter reports the selected FL9_1 projection; continue the
core/HAL audit before widening its contract. Public surface presentation is
still disabled.

The public offscreen integration now exercises sparse group-0 uniforms and
group-7 sampled bindings, indexed textured depth-tested output, dynamic
uniform updates and two concurrently submitting public devices. Its error
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
