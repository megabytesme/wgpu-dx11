# FL9_1 completion plan

The objective is a usable explicit D3D11 backend through the public native wgpu
API, with truthful downlevel capabilities, tested textured depth rendering and
Windows presentation. The public adapter remains disabled until the contract
below is met. This plan does not claim WebGPU conformance.

## Ordered work and gates

1. Preserve the tested private backend; continue uniform layout and real
   constant-budget testing. FXC and the forced-FL9_1 RTX accept a dynamically
   indexed vertex constant array of 255 float4s and reject 256 at index 256;
   pixel FXC accepts 32 and rejects 33 at register 32. Forced-FL9_1 GPU
   execution now verifies that WGSL access to vertex constant c254 changes
   geometry and pixel constant c31 changes fragment color. Runtime integer
   indexing remains rejected, so dynamic near-limit indexing is still
   unavailable. Continue investigating matrix, array and nested-structure
   layouts and FXC instruction pressure with checked shader translation.
2. Implement depth/stencil textures, views, initialization, render-pass
   attachments and depth state incrementally. Verify overlapping geometry in
   both draw orders on forced FL9_1, and independent load/clear/store behavior.
   Validate format support and restrict unsupported native state.
3. Broaden pipeline and resource functionality: vertex layouts, topology,
   indexed semantics, render state, mips/formats and multiple groups. Dynamic
   uniform offsets have restricted private coverage. Each feature needs positive
   GPU and negative tests. Non-indexed line lists and line strips now have a
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
   Broader instance semantics and indirect draws remain unavailable. Microsoft
   marks native `DrawInstancedIndirect` and `DrawIndexedInstancedIndirect` as
   unsupported at 9_1; wgpu-core's `INDIRECT_EXECUTION` flag is also coupled to
   compute support and GPU argument validation, so the public validation path
   cannot currently expose a private CPU-staging emulation independently.
   R8_UNORM is implemented as a queried single-mip sampled/upload format. Its
   2D upload and nearest sampling pass on forced FL9_1; color-target,
   copy-source, readback and mip-chain uses remain unsupported. Continue adding
   narrowly verified required formats and usage combinations.
4. Resolve core/HAL contract mismatches before exposure: negative and
   out-of-target viewports work for shaders with a remapped variant, but the
   variant can exceed FL9 compiler limits and those draws still fail; viewport
   interpolation, clipping edges and historical-driver precision need more
   coverage. DXGI device-loss classification is centralized and covered for
   reset, removal, driver-internal and OOM codes; continue auditing mapping,
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
   render-state combinations and shader limitations.
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
   verifies that result. Conditional NPOT repeat use is also represented by a
   downlevel capability and checked against Naga's selected-entry sampling
   pairs using the currently bound views and samplers. A strict core fixture
   rejects a used NPOT/repeat pair before submission and reads 64 yellow pixels
   from POT/repeat plus NPOT/clamp pairs despite an unused incompatible
   cross-pair; the RTX's positive full-NPOT query is separately reported.
   Next continue auditing core-valid attachment, viewport and render-state
   combinations before building the truthful public capability snapshot.
5. Implement a truthful hardware adapter, device limits/features/downlevel and
   per-format reports. Audit every core-valid command against HAL behavior;
   restrict safely or implement/emulate it. No placeholder capabilities.
6. Complete HWND surface lifecycle while preserving CoreWindow; the private
   hidden-window path and retained-command-buffer resize are tested, but
   concurrent presentation requires work. Microsoft documents that DXGI may
   synchronously send window messages during `Present` and wait for the owning
   message-pump thread. The current core queue-index lock and HAL immediate-
   context mutex span `Present`; a callback that reenters wgpu can deadlock.
   Releasing these locks permits context/DXGI concurrency, which Microsoft
   documents as invalid. A safe cross-layer scheduling or thread-affinity
   design is unresolved; do not assume a cooperative window procedure or
   enable public surface support until it is resolved. Build public
   wgpu offscreen and desktop examples selecting real D3D11 and forcing 9_1.
   Verify an indexed textured depth-tested 3D scene, readback and presentation.
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
and does not represent the public adapter contract. Next audit the public
HAL/core adapter capability boundary and build a truthful, testable capability
snapshot without enabling ordinary DXGI enumeration. Keep the public adapter
and HWND surface gates closed until every core-legal operation in that snapshot
is implemented or rejected. Latest code checkpoints `983eea619` and
`834cd9856` add core validation and cross-backend reporting for multi-mip
depth/stencil attachments. The earlier `6f7f12978` added native FL9_1
slope-scaled triangle depth bias; checkpoint
`1a362ec6108de9accb52e2f1978a174d1f923106` modeled hardware bias guarantees
separately from backend support across all seven feature-level identities.
