# FL9_1 progress

## Current position — 2026-11-03

Branch: `uwp-dx11`. The previous private uniform work was independently
reviewed, corrected for unused-entry-point binding conflicts, tested and
checkpointed as `26739b52d1ae042a6499b578521eb3250f1d9c21` (generalized static
uniform bindings). No public D3D11 adapter is exposed.

The verified tree has a restricted private renderer with sampled textures,
static group-0 uniforms, RGBA8 transfers, indexed draws and event-query fences.
It lacks depth/stencil, HWND presentation, a usable public adapter and numerous
operations required by core. See CAPABILITIES.md for the detailed matrix.

## Latest validation

- Rust 1.93.1, MSVC, Windows 11, `CARGO_BUILD_JOBS=8`.
- `cargo build --offline -p wgpu-hal --no-default-features --features dx11`:
  passed.
- With `WGPU_DX11_REQUIRE_HARDWARE=1`, `cargo test --offline -p wgpu-hal
  --lib --no-default-features --features dx11 dx11:: -- --test-threads=1`:
  126 passed, 0 failed, 2 filtered. Some older tests can skip internally.
- Strict focused FL9_1 GPU tests ran on NVIDIA RTX 3090 Ti, vendor 0x10de,
  device 0x2203, driver 32.0.16.2101. The checked multi-uniform test rendered
  [127,191,0,255] and [64,127,0,255]; textured uniforms rendered
  [64,127,191,255]. Requested and actual feature level: 9_1.
- `cargo test --offline -p wgpu-core --lib --no-default-features --features
  dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture`:
  passed. Targeted Clippy passed with ten existing warnings.
- The preceding milestone's targeted DX12 buffer map regression passed on the
  NVIDIA adapter. No public D3D11 or historical-hardware validation exists.

## Review findings and decisions

- Mapping creation must consider the selected shader entry point: an unused
  texture can share a logical identity with a layout uniform. A regression test
  now proves the unused declaration cannot cause a false rejection.
- A public adapter must not treat valid core calls as HAL failures: current
  examples include empty compute passes, several legal viewports, blend
  constants and combined index/vertex usages. Some mapping errors use
  `expect` and need a device-loss-safe contract audit.
- The FL9_1 device documentation says default buffers require exactly one
  vertex/index/constant bind flag. The current copy-only and uniform-source
  default buffers use no bind flag; RTX accepts them, but historical-driver
  portability is unresolved. Staging buffers have separate no-bind rules.
- A private depth increment should query native support, retain depth views,
  initialize both D24S8 aspects, validate matching attachment format/extent,
  and reject unverified comparison or two-sided stencil configurations. Core can
  generate depth-only initialization passes, which remain a public gate.
- Pixel FXC compilation accepts 32 referenced float4 constants and rejects 33
  in the tested profile. The vertex budget and near-limit GPU execution remain
  unverified; sparse high indices do not measure live constant pressure.

## Next task

The checked multi-uniform GPU case has subsequently been extended to a nested
structure, asymmetric mat3x2 and two-element vec4 array. On the same hardware
the strict `fl91_sparse_uniform_ranges` test passed with pixels
[143,207,16,255] and [80,143,16,255]. This verifies the tested offsets and
stride, not dynamic indexing or every uniform layout. The 126-test private
suite and targeted Clippy passed; the test and mission plan were checkpointed
as `19e82da409dd0ceec12eb9e9dca8934900b5ff11`.

The current work adds a contained D24S8 attachment path with zero initialization,
depth-only clears, Always/LessEqual depth tests and optional writes. Strict
forced-FL9_1 RTX 3090 Ti rendering samples a texture and validates all 64 pixels
in each of ten passes, including both depth orders, independent stencil clear,
no-write behavior and retained resources. Stencil operations, aspect views,
read-only usage and broader depth formats remain unsupported. Independent review
identified and prompted rejection of non-ignored stencil faces even with zero
masks. The D3D11 build, strict 127-test private suite, core zero-adapter test
and targeted Clippy passed. Ten pre-existing Clippy warnings remain. The DX12
`wgpu_gpu::buffer::map_offset` hardware regression passed on the NVIDIA
adapter. The depth checkpoint is pending.

Next advance the core compatibility and presentation gates in FL9_1_PLAN.md.
