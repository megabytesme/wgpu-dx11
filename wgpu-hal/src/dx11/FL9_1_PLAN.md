# FL9_1 completion plan

The objective is a usable explicit D3D11 backend through the public native wgpu
API, with truthful downlevel capabilities, tested textured depth rendering and
Windows presentation. The public adapter remains disabled until the contract
below is met. This plan does not claim WebGPU conformance.

## Ordered work and gates

1. Preserve the tested private backend; continue uniform layout and real
   constant-budget testing. FXC and the forced-FL9_1 RTX accept a dynamically
   indexed vertex constant array of 255 float4s and reject 256 at index 256;
   pixel FXC accepts 32 and rejects 33 at register 32. Near-limit vertex
   execution remains unverified because private WGSL rejects runtime integer
   indexing. Investigate asymmetric matrices, arrays, nested structures and
   FXC instruction pressure with checked shader translation.
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
   readback. Indexed line lists and indexed line-strip semantics remain open;
   implement and verify indexed line-list rebasing while preserving line pairs.
   Indexed strips need separate primitive-restart and batch-connectivity
   analysis.
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
