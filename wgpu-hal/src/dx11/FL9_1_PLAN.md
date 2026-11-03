# FL9_1 completion plan

The objective is a usable explicit D3D11 backend through the public native wgpu
API, with truthful downlevel capabilities, tested textured depth rendering and
Windows presentation. The public adapter remains disabled until the contract
below is met. This plan does not claim WebGPU conformance.

## Ordered work and gates

1. Preserve the tested private backend; continue uniform layout and real
   constant-budget testing. Investigate asymmetric matrices, arrays, nested
   structures and FXC instruction pressure with checked shader translation.
2. Implement depth/stencil textures, views, initialization, render-pass
   attachments and depth state incrementally. Verify overlapping geometry in
   both draw orders on forced FL9_1, and independent load/clear/store behavior.
   Validate format support and restrict unsupported native state.
3. Broaden pipeline and resource functionality: vertex layouts, topology,
   indexed semantics, render state, mips/formats, multiple groups and dynamic
   uniform offsets. Each feature needs positive GPU and negative tests.
4. Resolve core/HAL contract mismatches before exposure: negative and
   out-of-target viewport cases can reach rejecting HAL paths; zero-sized
   viewports are tested. Audit mapping/device-loss panics. Empty compute
   passes and unused blend constants are handled; compute dispatch remains
   unavailable. The downlevel combined-index usage rule has been corrected.
   Handle depth-only initialization passes and read-only depth hazards.
5. Implement a truthful hardware adapter, device limits/features/downlevel and
   per-format reports. Audit every core-valid command against HAL behavior;
   restrict safely or implement/emulate it. No placeholder capabilities.
6. Implement HWND surface lifecycle while preserving CoreWindow. Build public
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
