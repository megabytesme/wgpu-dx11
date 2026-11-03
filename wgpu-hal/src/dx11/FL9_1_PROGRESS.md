# FL9_1 progress

## Current position — 2026-11-03

Branch: `uwp-dx11`. The previous private uniform work was independently
reviewed, corrected for unused-entry-point binding conflicts, tested and
checkpointed as `26739b52d1ae042a6499b578521eb3250f1d9c21` (generalized static
uniform bindings). No public D3D11 adapter is exposed.

The verified tree has a restricted private renderer with sampled textures,
static group-0 uniforms, RGBA8 transfers, indexed draws, restricted D24S8
depth attachments and event-query fences. It lacks general depth/stencil,
HWND presentation, a usable public adapter and numerous operations required by
core. See CAPABILITIES.md for the detailed matrix.

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
  examples include empty compute passes, several legal viewports and blend
  constants. Some mapping errors use
  `expect` and need a device-loss-safe contract audit.
- The FL9_1 device documentation says buffers require exactly one
  vertex/index/constant bind flag. The implementation now gives default
  copy-only and uniform-source buffers an internal vertex bind; logical usages
  remain unchanged. Staging buffers retain zero flags because the general D3D11
  staging/CPU-read contract prohibits pipeline binds. The level-9 table does
  not explicitly spell out the staging exception; legacy runtime behavior
  remains unverified.
- The private D24S8 path queries native support, retains depth views, initializes
  both aspects and supports depth-only clear passes. Depth-only drawing,
  read-only/aspect views and stencil testing remain public integration gates.
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

The D24S8 attachment path has zero initialization,
depth-only clears, Always/LessEqual depth tests and optional writes. Strict
forced-FL9_1 RTX 3090 Ti rendering samples a texture and validates all 64 pixels
in each of ten passes, including both depth orders, independent stencil clear,
no-write behavior and retained resources. Stencil operations, aspect views,
read-only usage and broader depth formats remain unsupported. Independent review
identified and prompted rejection of non-ignored stencil faces even with zero
masks. The D3D11 build, strict 127-test private suite, core zero-adapter test
and targeted Clippy passed. Ten pre-existing Clippy warnings remain. The DX12
`wgpu_gpu::buffer::map_offset` hardware regression passed on the NVIDIA
adapter. It was checkpointed as
`424dfaa2f46e757e9d3a0dad9c50b9f110a18592`.

The default-buffer bind-flag portability fix passed its synthetic allocation
test and strict FL9_1 hardware copy/readback (16/16 bytes); the private suite
passed 129 tests, and the core zero-adapter and targeted Clippy checks passed.
The existing DX12 GPU test binary also passed `wgpu_gpu::buffer::map_offset`
on the NVIDIA adapter after generating its temporary GPU configuration. The
fix was checkpointed as `e09825a90b32dc52eb83cd27f084c2803abf96c5`.

Core's `UNRESTRICTED_INDEX_BUFFER` creation check was reproduced failing via a
Noop adapter with only that downlevel flag removed. It accepted `INDEX|VERTEX`
instead of rejecting it. The proposed core fix checks all known non-copy usages;
the targeted public-API test passes for restricted and unrestricted adapters,
including map/query usages, mapped-at-creation and an unrelated non-index
control. The complete 66-test core library suite and 129-test private D3D11
suite passed. Targeted core Clippy passed. This changes validation for all
adapters lacking the flag, including WebGL; the Noop restricted adapter covers
the shared rule but is not a WebGL runtime test. A rebuilt DX12 GPU buffer map
regression passed on the NVIDIA adapter.
The broader validation suite had 86 passes and three pre-existing adapter-error
message mismatches because the fork's DX11 backend is absent from the expected
strings. The suite passed with those three tests filtered. The core fix and its
tests were checkpointed as `8949812c55514e106e7803e00d7e25801e516d44`.
The stale expected strings now include the actual DX11 diagnostic. The full
unfiltered `cargo test --offline -p wgpu-test --test wgpu-validation --
--test-threads=1` suite passed 89/89 tests.
The diagnostic fix was checkpointed as `7bdffefe244101bf7374fbc10044424f08eea8c1`.

The encoder now accepts paired empty compute passes while rejecting dispatch,
timestamps, nesting and transfer commands inside a pass. A strict FL9_1 test
copied 16/16 expected bytes through two GPU copies around an empty compute
pass, retaining source resources until submission. The existing textured depth
test also exercises unused blend constants before and after pipeline selection;
all ten 64-pixel results still match. Blend factors using constants remain
unsupported, and any future implementation must retain the factor across
pipeline changes. The strict private suite passed 130 tests; older tests may
still skip internally. The zero-adapter core test and targeted Clippy passed,
with ten existing Clippy warnings. Independent review found no issue in this
restricted path. Public core execution has not been tested through D3D11.
The encoder change was checkpointed as `26825d96e`.

Viewport dimensions of zero are now accepted. Four strict forced-FL9_1 hardware
tests verified zero width and height preserve all 64 clear pixels, an indexed
draw also preserves them, and restoring the viewport with a half-width scissor
produces 32 clear and 32 drawn pixels. Negative coordinates and non-finite
values remain rejected. The full private suite passed 134 tests before adding
the additional negative viewport checks; all four focused tests passed again
afterward. Targeted Clippy passed with ten existing warnings and the core
zero-adapter test passed. Microsoft documents both zero dimensions and the
FL9_x negative-origin failure in
<https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_viewport>.
Negative origins and rectangles extending beyond the target remain unresolved
public integration blockers; historical FL9 drivers have not been tested.
Zero-sized viewport support was checkpointed as `4afbb21da`.

Core's viewport validation previously accepted NaN position and size values;
the new public Noop regression reproduced acceptance before the fix. Core now
rejects non-finite coordinates and dimensions before the HAL call, with
accurate diagnostics. The test covers NaN and both infinities in all six
fields and checks zero-sized, negative-origin and out-of-target finite controls.
The full public validation suite passed 90 tests before the final additional
controls and diagnostic wording; the focused test passed afterward. The 66
core library tests, targeted core Clippy and the rebuilt DX12
`wgpu_gpu::encoder::drop_encoder_after_error` hardware test passed. Vulkan
loader warnings about a missing OBS layer JSON are pre-existing. This shared
validation fix affects all native wgpu-core backends.
The shared validation fix was checkpointed as `d13062b65`.

Private render passes now accept exactly one load operation and one store
operation per attachment aspect, including don't-care and discard. Discard is
a native no-op, so no discarded contents are promised. Core's later zero
initialization still requires public-path integration testing. The strict
FL9_1 depth test executes independent stencil and depth discards, preserves a
loaded depth aspect, verifies a fully overwritten don't-care color target, and
checks all 64 pixels of ten subsequent results. Malformed operation flags are
rejected in unit and encoder tests. The full private suite passed 135 tests;
after adding the stronger don't-care and encoder-negative checks, the focused
GPU test passed again. The core isolation test and targeted Clippy passed, with
ten existing Clippy warnings.

Independent HWND research identified two presentation blockers: `Present`
thread synchronization and message-pump lock ordering need review, and retained
command-buffer RTV references may prevent `ResizeBuffers` even after a surface
texture is released. A future Win32 surface must address and test both; the
the public adapter gate remains unchanged.
Attachment discard support was checkpointed as `721469cfe`.

The private surface now accepts a borrowed Win32 HWND and creates a BGRA8
flip-sequential swapchain. It uses stretch scaling for HWND, preserving the
existing CoreWindow scaling. After unbinding and dropping a swapchain,
unconfigure flushes the immediate context before a replacement can be created.
The strict hidden-window test executed on the RTX 3090 Ti at forced FL9_1,
submitted a HAL clear, read back BGRA [0,0,255,255], presented with success,
resized and reacquired, then unconfigured and recreated the swapchain. This
does not establish visible desktop presentation or presentation concurrency.
The 136-test strict private suite passed before the final flush and scaling
changes; the focused hardware test passed after them. Targeted Clippy passed
with ten existing warnings and core's zero-adapter isolation test passed.
The test resets its recorded command before resize; live command-buffer RTV
references, concurrent Present/context access, message-pump locking and whether
queue idle covers pending presentation remain public-presentation blockers.
Windows 7 flip-swapchain support is not claimed.
The initial private HWND surface was checkpointed as `bf56c17b9`.

The retained-command-buffer resize concern was reproduced on the same RTX:
`ResizeBuffers` failed with the completed buffer still holding the backbuffer
RTV. Surface-backed render commands now keep a weak per-view reference. Queue
submission validates and temporarily retains all referenced surface views
before replay; ordinary offscreen render targets retain their previous strong
ownership. After GPU completion and view destruction, the hidden HWND test
resizes and recreates the swapchain while the old command buffer remains alive,
then records a new HAL clear and reads back green BGRA [0,255,0,255]. The old
buffer is reset only afterward. A destroyed surface view fails the submission
preflight check; resubmitting a command with destroyed resources is outside the
HAL contract. The strict 136-test private suite and zero-adapter core test
passed, and targeted Clippy passed with ten existing HAL warnings. At that
point an experimental Naga viewport transform was also in the working tree and
introduced three separate Clippy warnings; it was not used by the DX11 shader
path and is not part of the surface ownership fix. Visible presentation and
concurrent Present/context synchronization remain unverified.
