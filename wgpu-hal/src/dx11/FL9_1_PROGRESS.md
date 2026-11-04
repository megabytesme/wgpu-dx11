# FL9_1 progress

## Current position — 2026-11-04

## Checkpoint — core viewport clipping capability — `19ff6dff6`

Core now reports and enforces `DownlevelLimits::viewport_rectangles_outside_target`.
The default remains true for existing backends; the controlled FL9_1 fixture
sets it false. When false, core rejects a finite viewport crossing any active
render-target edge with a validation error before invoking HAL. This avoids
the previous late draw failure when D3D11's optional clipped-viewport shader
variant exceeds the FL9_1 FXC instruction budget. The private HAL remap path is
unchanged and remains available to restricted private callers.

Verification used Rust 1.93.1/MSVC with `CARGO_BUILD_JOBS=32`, one Cargo process
at a time:

- Baseline before the change: existing `wgpu-types` downlevel-limit test passed
  1/1.
- New CPU bounds test: `viewport_target_bounds_cover_all_edges` passed 1/1 and
  checked all four edges plus fully contained and edge-aligned rectangles.
- `cargo test --offline -p wgpu-types --lib -- --test-threads=1`: 60/60 passed.
- Strict fixture-enabled core suite: 75/75 passed. Its physical adapter was
  NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested and
  selected FL9_1. The new out-of-target case was validation-only and issued no
  draw; existing fixture operations performed rendering/readback and matched
  their expected pixels. The test also asserts that normal D3D11 HAL adapter
  enumeration is empty.
- Strict private HAL suite: 326/326 Rust tests passed. The run logged 144
  hardware initialization records on the RTX forced to FL9_1. Six shader
  probes printed `SKIP` after FXC rejected their unsupported shader before
  GPU rendering; those probes are not counted as successful GPU work.
- Core's explicit zero-D3D11-adapter test: 1/1 passed. Core DX12 regression:
  72/72 passed.
- HAL `dx11,gles`, core `dx11,std,wgsl`, `wgpu-info` and `wgpu-test` checks
  passed. Clippy passed for wgpu-types, fixture-enabled core, HAL `dx11,gles`,
  wgpu-info and wgpu-test. HAL emitted ten existing warnings; wgpu-types
  emitted fourteen existing unused-serde-import warnings.
- Targeted rustfmt and `git diff --check` passed. A first core build caught
  missing initializers in the no-op adapter; no-op and GLES now explicitly
  report the default-supported behavior. No test failures remain.

The strict core fixture did not print the driver version during this run; the
last recorded driver baseline is NVIDIA `32.0.16.2101`. This forced-level test
does not establish behavior on historical FL9_1 hardware. The public adapter
and surface gates remain closed.

## Checkpoint — finite texture-repeat limit in core — `d3e7db9a1`

Microsoft's 10Level9 `Draw`/`DrawIndexed` contract limits repeats over one
primitive to 128 at FL9_1 (2048 at 9_2, 8192 at 9_3/10_x, 16384 at 11_x).
The D3D11 feature-level model records those static guarantees. A new
`DownlevelLimits::max_texture_repeat_per_primitive` carries a reported bound;
core rejects `Repeat` and `MirrorRepeat` sampler creation for finite bounds,
because it cannot prove interpolated coordinates remain within the limit for
all possible draw data. This is intentionally stricter than the native bound:
core does not yet have a coordinate-range validator or semantics-preserving
shader emulation. The bound is not an optional driver query. The FL9_1 core
fixture keeps its test-only unbounded setting so private repeat-render tests
continue exercising HAL behavior; this setting is not a public capability
projection.

The test harness now compares required `DownlevelLimits` when deciding whether
a conformance test can run. The minimum test profile requests no texture-repeat
capacity; a test needing unbounded repeat can request `u32::MAX`. Ordinary
D3D11 adapter enumeration remains disabled.

Validation on Rust 1.93.1 / Windows MSVC with `CARGO_BUILD_JOBS=32`:

- `cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampler_repeat_limit -- --nocapture --test-threads=1`: 1 passed, CPU-only.
- `cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 feature_level_guarantees_are_specific_and_do_not_promote_backend -- --nocapture --test-threads=1`: 1 passed, all seven feature-level identities checked.
- `cargo test --offline -p wgpu-types downlevel_limits_check_minimum_capabilities -- --nocapture`: 1 passed.
- Strict `fl91_core_buffer_and_offscreen_render_on_hardware`: 1 Rust test passed with many readback cases. NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested and actual FL9_1. The physical full-NPOT query was true; synthetic fixture validation forced the conditional path. NPOT/repeat rejection and valid sampled/readback output passed. This does not validate historical repeat overflow behavior.
- Core D3D11 zero-adapter isolation: 1 passed. Core DX12 regression suite: 71 passed.
- Strict private D3D11 HAL suite: 323 passed, 0 failed, including existing texture, sampler, draw, transfer and synchronization GPU tests. No dedicated over-limit GPU experiment was run because the modern driver does not reproduce legacy FL9_1 behavior.
- `cargo check -p wgpu-hal --no-default-features --features dx11,gles`, `cargo check -p wgpu-info`, `cargo check -p wgpu-test`, targeted `rustfmt --check`, and `git diff --check` passed.
- Core and wgpu-test Clippy passed. HAL Clippy passed with 10 pre-existing warnings; wgpu-types Clippy passed with 14 pre-existing unused-serde-import warnings. A first test-package Clippy call lacked the MSVC environment and failed to find `cl.exe`/`link.exe`; rerunning after `VsDevCmd.bat` passed. An early core assertion used `assert_eq!` on a non-`PartialEq` error and was corrected to `matches!` before the passing test.

The fixture did not print a driver version during this run; the prior hardware
baseline recorded NVIDIA driver `32.0.16.2101`. No historical FL9_1 hardware
was tested. See CAPABILITIES.md for the hardware/backend/public-contract split.

Branch: `uwp-dx11`. The autonomous FL9_1 completion mission remains active.
Recent verified code checkpoints include `834cd9856` (Vulkan reports the
shared multi-mip depth capability), `983eea619` (core gates multi-mip depth
attachments), `5fe21f035` (all native stencil
operation outcomes at forced FL9_1), `340147803` (native FL9_1 depth
comparisons), `f99cae113` (general single-target blend translation),
`8eff9020a` (single-sample output masks) and `1eb21620c` (core rejects NPOT
mip chains when the downlevel capability is absent). Ordinary
public D3D11 adapter enumeration and public surface exposure remain disabled.

The private renderer now has extensive forced-FL9_1 coverage for depth/stencil,
texture formats and mip views, static and dynamic uniform ranges, sampled
resources, texture transfers, and indexed/non-indexed point, line and triangle
topologies. The principal completion blockers remain a truthful public adapter
contract, unsupported core-valid descriptor/state combinations, a safe public
HWND presentation design, shader limits, and historical FL9_1 verification.
See CAPABILITIES.md and STATUS.md for the detailed implementation matrix.

## Latest validation

- Rust 1.93.1, MSVC, Windows 11, bounded Cargo jobs (8).
- At `5fe21f035`, the strict focused stencil GPU test passed 1/1 and the full
  strict private HAL suite passed 322/322. The RTX 3090 Ti was requested and
  selected at FL9_1. All eight operations in all three stencil outcomes matched
  1,536 checked output pixels. HAL check, core zero-adapter isolation, HAL
  Clippy, rustfmt and `git diff --check` passed.
- At `8eff9020a`, strict D3D11 HAL tests passed 318/318; the fixture-enabled
  core suite passed 70/70, the core DX12 suite passed 67/67, and the ordinary
  zero-adapter isolation test passed. The RTX 3090 Ti (`0x10de:0x2203`, driver
  `32.0.16.2101`) was requested and selected at FL9_1. A core fixture with a
  zero sample mask preserved all 64 blue clear pixels.
- At `1eb21620c`, the new core CPU dimension test passed 1/1, the strict
  fixture-enabled core suite passed 71/71 and printed the NPOT-mip missing-flag
  validation result, the core DX12 suite passed 68/68, and ordinary D3D11
  adapter isolation passed 1/1. Core Clippy, targeted rustfmt and
  `git diff --check` passed. The DX12-only test configuration emitted existing
  warnings because WGSL is disabled. No HAL code changed for this checkpoint.
- At `983eea619`, the strict controlled core fixture passed 1/1 on the RTX
  3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual
  FL9_1, including early rejection of a two-mip D24S8 render attachment. The
  full fixture-enabled core suite passed 71/71; the strict private HAL suite
  passed 322/322; the ordinary zero-adapter test passed 1/1; the core DX12
  suite passed 68/68; the focused wgpu-types flag test passed 1/1; combined
  D3D11+GLES and Vulkan HAL checks passed; core and Vulkan HAL Clippy passed;
  D3D11+GLES HAL Clippy passed with ten existing warnings; targeted rustfmt
  and `git diff --check` passed.
- No public D3D11 adapter, public HWND render, or historical FL9_1 hardware
  result is available.

## Checkpoint — FL9_1 stencil operation outcome coverage — `5fe21f035`

The existing D24S8 test now exercises each of the eight wgpu stencil
operations in all three native outcome slots: stencil/depth pass, stencil
comparison failure, and depth-test failure. Boundary values distinguish
clamped from wrapping increment/decrement; Replace uses different references
for successful and failed stencil comparisons. A subsequent Equal pipeline
draws blue only when the resulting stencil value matches the expected value.
All 24 cases matched all 64 pixels. The HAL does not expose direct stencil
storage readback, so this is an indirect validation through actual GPU tests.

The focused strict test passed 1/1 on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, software=false), with FL9_1 requested and selected. The full
strict D3D11 HAL suite passed 322/322, including the 24 stencil cases. The HAL
check passed, the ordinary core zero-D3D11-adapter isolation test passed 1/1,
and HAL Clippy completed with ten pre-existing warnings outside the changed
test. Rustfmt and `git diff --check` passed. The first attempt without
initializing the Visual Studio environment failed because `link.exe` was not
on PATH; rerunning after `VsDevCmd.bat` succeeded. Cargo used Rust 1.93.1 and
`CARGO_BUILD_JOBS=8`.

Microsoft defines the eight native operation behaviors in the
[`D3D11_STENCIL_OP` reference](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_stencil_op).
The 10Level9 device table documents the FL9_1 two-sided-stencil restriction;
this backend continues to reject asymmetric front/back states. Modern hardware
forced to FL9_1 does not establish behavior on historical drivers.

Commands, run from an initialized Visual Studio 2022 Build Tools environment
with Rust 1.93.1 and `CARGO_BUILD_JOBS=8`:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_stencil_operations_match_expected_values -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
rustfmt --edition 2021 --check wgpu-hal/tests/dx11/stencil_noop.rs
git diff --check
```

The six existing checked runtime-integer shader subcases still log `SKIP`
after the backend rejects their unsupported integer semantics; those skips are
not part of the stencil execution claim.

Code checkpoint: `5fe21f035` (`dx11: test FL9_1 stencil operation outcomes`).
Continue the core-valid descriptor/command audit before constructing a truthful
public capability snapshot. Keep normal adapter enumeration and public HWND
surface exposure disabled.

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
The surface-view ownership fix was checkpointed as `7e045c4ed`.

Viewport remapping is implemented experimentally. The encoder rounds the
logical viewport consistently, intersects it with the target, and uses a
reserved vertex constant at `b13` with a second Naga-generated FL9 vertex
shader. The original shader is retained for in-target viewports. If the
remapped shader cannot be compiled or created, ordinary draws still work and
draws requiring remapping fail. Strict forced-FL9_1 tests cover all four
partially clipped edges, an entirely offscreen viewport, a one-pixel
intersection, fractional rounding, indexed and non-indexed draws, viewport
switching, variable clip-space w and depth testing with a nondefault depth
range. They check the complete 8x8 image. They do not establish arbitrary
perspective-varying interpolation, all clipping precision cases or historical
drivers. FXC rejects fragment-position input in `ps_4_0_level_9_1`; that is a
separate public-adapter blocker.

An FXC probe using 254 referenced uniform vectors compiled the original Naga
vertex shader, while its remapped variant failed with X5615 (258 instructions;
the profile limit is 256). The fallback and unavailable-variant draw rejection
are tested, but no general solution exists yet for such shaders. One diagnostic
using a 252-term nested WGSL expression overflowed the Windows test thread's
stack; the test was rewritten as sequential statements, and the lingering
crashed test process was terminated. The responsible parser/validator/writer
stage was not isolated; this remains an investigation item rather than a claim
about viewport math.

After integration, 153 private D3D11 tests passed in strict mode, and 135 Naga
library tests passed. Targeted Naga and DX11 Clippy passed with ten existing
DX11 test warnings; core zero-adapter isolation and DX12 HAL compilation
passed. The public HLSL `Options` struct gained a field, so downstream exhaustive
Rust literals need updating. GPU presentation remains hidden-only.
Following the error-classification and restored-pipeline checks, the focused
viewport suite passed 20 tests and the full strict private suite again passed
153 tests. A rebuilt DX12 GPU executable passed
`wgpu_gpu::encoder::drop_encoder_after_error` on the NVIDIA adapter; the DX12
HAL also compiled with the new Naga option disabled. During an earlier DX12
test build, other Cargo checks overlapped unintentionally; subsequent checks
were serialized. No memory-commit failure was observed.

Microsoft explicitly prohibits concurrent immediate-context use with most DXGI
calls. The current private surface does not yet serialize all swapchain calls
against device context operations. A common context mutex is necessary but
still needs a lock-order and responsive-message-pump design to avoid deadlocks.
HAL `wait_for_idle` also promises pending presentation completion, while the
current event-query wait has only established GPU command completion. These
are public-presentation blockers, not merely untested performance concerns.
See <https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-intro>
and <https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices#multithreading-and-dxgi>.
The restricted viewport implementation was checkpointed as `ec645e63e`.

Group-0 uniform bindings now support dynamic offsets. The layout retains the
dynamic bit; offsets are consumed in increasing logical binding order, checked
against the logical source-buffer size and captured in recorded commands.
Materialization still reads the GPU buffer at replay, preserving queued copies
and updates. The private alignment is four bytes; a future public device must
enforce its reported wgpu alignment. Strict FL9_1 GPU tests cover sparse mixed
uniforms, two dynamic offsets, preceding GPU copies, sampled textures, indexed
draws and an update across submissions. The mixed-uniform test read
[143,207,16,255] and [143,143,16,255]; the textured tests distinguished full,
half and quarter green. Synthetic tests reject bad counts, alignment, bounds,
overflow and offsets beyond an implicit-to-end range. The complete private
suite passed 157 tests; the core zero-adapter test and targeted Clippy passed,
with ten existing test warnings. Independent review found no source-range or
lifetime defect. Public adapter exposure and multiple bind groups remain open.
Dynamic uniform offsets were checkpointed as `edb1efa6b`.

The private swapchain synchronization change serializes DXGI calls with the
immediate context and uses short-lived surface and frame reservations instead
of holding their state locks through DXGI. A review identified a lost
unconfigure operation when a surface was busy; the draft now waits for that
operation to finish. The revised wait and teardown/recreation test passed on
the NVIDIA FL9_1 device, including a context-lock serialization check. The
hidden HWND clear returned BGRA [0,0,255,255], resize/readback succeeded, and
present returned `Ok(())`. This does not verify visible presentation.

This synchronization draft does not resolve synchronous window-message
reentrancy: a callback that reenters the context or waits for the same surface
operation may deadlock. It must not be treated as a safe public presentation
contract. Presentation completion in `wait_for_idle` also remains unproven.
The restricted private swapchain synchronization was checkpointed as
`9464c4aef`.

Separately, a shader-validation change narrows checked `textureLoad`
rejection to the selected entry point and transitively called functions.
Reachable loads remain rejected. A CPU call-graph test and a selected-entry
Naga/FXC sampled-rendering test passed; the latter verified the existing
alpha-blended readback. An independent review confirmed the traversal remains
conservative for calls in Naga's structured control flow.

The combined drafts passed `cargo test --offline -p wgpu-hal --lib
--no-default-features --features dx11 dx11:: -- --test-threads=1` with
`WGPU_DX11_REQUIRE_HARDWARE=1`: 162 passed, 0 failed, 2 filtered; some older
tests can still skip internally. Focused strict HWND and checked shader-render
tests independently passed. The core zero-adapter isolation test passed, and
`cargo clippy --offline -p wgpu-hal --lib --tests --no-default-features
--features dx11` passed with ten existing warnings. The first build exposed
three test-closure lifetime errors; these were corrected before the passing
run. No public D3D11 execution is claimed.

The selected-entry shader load validation was checkpointed as `7a7f249f5`.
Microsoft's required FL9_1 driver capabilities guarantee Always and LessEqual
depth comparisons, but do not establish the other six. The existence of the
D3D11 comparison enum or success on the RTX would not prove historical
support; a portable query or emulation remains to be investigated.

Private pipelines now map every combination of RGBA write channels, including
zero, to an explicit native state when necessary. FL9_1's required
`D3DPMISCCAPS_COLORWRITEENABLE` guarantees per-channel writes for its one
render target; this is distinct from independent masks across render targets.
Strict FL9_1 hardware tests verified all 16 masks over 64 pixels each, a
three-region masked pipeline transition, preserved depth writes with color
disabled, and masked additive blending. A synthetic test rejects unknown mask
bits. An independent review found no descriptor or lifetime defect. The full
strict 163-test suite passed before the final stronger transition assertion;
the focused hardware test passed again after that test-only improvement.

The color-mask work was checkpointed as `4bba5f9c5`. Triangle-list pipelines
now accept both front-face windings and none/front/back culling, using the same
native mapping as DX12. Opposite-winding textured triangles passed all six
combinations on the forced-FL9_1 RTX, with every pixel checked; a seventh case
also covered a negatively positioned viewport. An independent review found no
mapping or state-lifetime defect. The strict private suite passed 163 tests;
the core isolation test and targeted Clippy passed, with ten existing warnings.
The subsequently added Naga-to-FXC negative test for fragment `front_facing`
also passed: that built-in is rejected by the FL9_1 pixel profile even though
rasterizer face culling works. A direct FXC probe produced X4583 and X4568.
Face culling was checkpointed as `f8bd5183a`.

At the next checkpoint, a pipeline-wide register-plan
scaffold allocates disjoint compact slots across logical groups and feeds the
existing group-0 shader path; synthetic tests cover holes, visibility, sparse
bindings and aggregate register exhaustion. That checkpoint did not yet permit
a second group. An audit found that reusing one uniform materialization buffer
at two indices with different dynamic offsets would alias.
The 165-test strict private suite passed before the final allocator hardening;
afterward, the allocator test and a forced-FL9_1 dynamic-uniform GPU test
passed, with readback [143,207,16,255] and [143,143,16,255]. The allocator
rejects non-dense internal resource arrays and aggregate slot overflow; a
Naga/FXC test compiled distinct group-0 and group-2 uniforms using the same
logical binding number. Targeted Clippy passed with ten existing warnings.

Register planning was checkpointed as `f4df16d8b`. The current implementation
adds uniform-only groups after group 0, preserves layout-prefix invalidation,
and allocates independent constant-buffer materialization per group binding.
It rebinds using the active pipeline plan at each draw. Strict GPU tests cover
the same group at groups 0 and 2 with different dynamic offsets, a queued GPU
copy, direct and indexed draws, layout-prefix invalidation, pipeline switches
that move a native register, resource destruction before submit, and texture
sampling in group 0 with a group-1 uniform. The 64 checked pixels matched in
each case, including [64,128,0,255], [0,128,191,255] and [64,96,32,255].
The strict 167-test private suite passed; some older tests may skip internally.
An independent review found no concrete correctness defect. Dynamic offsets
here have private four-byte alignment, not a public wgpu limit. Each group bind
allocates scratch buffers and each draw stages and maps all bound uniforms;
this is correctness-first and can stall substantially. At that checkpoint,
sampled bindings remained restricted to group 0; public exposure was disabled.

The multiple-uniform-group implementation was checkpointed as `a914c9a1f`.
Current work extends sampled textures and samplers to separate logical groups,
using the same pipeline-wide native register plan and flattening current
per-group metadata for shader-relationship and conditional-NPOT validation.
Synthetic tests cover helper-derived cross-group pairs, unused invalid
cross-products and unavailable versus positive NPOT queries. Strict GPU tests
sample a patterned texture using samplers in different groups, switch the
sampler and pipeline slots during a pass, combine two sampled textures across
five groups, use a shared sampler and multiple samplers for one texture, retain
resources after destruction, and reject a later-group
render-target feedback alias, a missing group and an incompatible group.
The checked pixels distinguish [0,96,32,255], [64,0,0,255] and
[32,96,32,255]. These tests use a modern driver and do not establish legacy
conditional-NPOT hardware behavior. A test descriptor initially used the wrong
flags type; compilation caught it and the corrected focused GPU test passed.
The first full-suite run then had 167 passes and one failure: an old test still
expected a sampled layout at group 1 to be rejected. That layout is now
supported; its expectation was updated and the full suite passed 168 tests,
with two filtered. Afterward, added shared- and multiple-sampler GPU cases
passed the focused test; the latter checked [32,48,16,255] over 64 pixels.
The shared-sampler test was strengthened after review so that the second
patterned texture also distinguishes clamp from repeat; its 64 pixels matched
[16,56,48,255] on the forced-FL9_1 device.
Targeted Clippy passed with ten existing warnings after fixing a new style
warning, and the core zero-adapter test passed.
Cross-group sampled resources were checkpointed as `47e8488ef`.

A nondefault, unsafe `dx11-test-fixture` creates an owner-matched device and
queue for a closed core test without enabling ordinary adapter enumeration or
normal adapter opening. On the RTX 3090 Ti at requested and actual FL9_1,
`cargo test --offline -p wgpu-core --lib --no-default-features --features
dx11-test-fixture,std fl91_core_buffer_copy_readback_on_hardware -- --nocapture`
passed: core initialized the device, queued a buffer upload, submitted a GPU
copy, waited for completion and mapped 64/64 expected bytes. The fixture reports
no texture or surface capabilities and its intentionally restricted limits are
not a general adapter contract. Core's full fixture-enabled library suite
passed 67 tests; the feature-off zero-adapter test passed. Targeted fixture-on
core Clippy and feature-off HAL build passed. Independent review prompted
explicit command encoder and command-buffer teardown before device removal;
the focused hardware test passed again after that change.

The core buffer fixture was checkpointed as `f6aa8f241`. Its current extension
queries the actual RGBA8 driver support and tests a closed core offscreen
rendering sequence; forced-FL9_1 GPU readback matched 64/64 red pixels after a
blue clear. The fixture-enabled core suite passed 67 tests. Independent review
confirmed the observed readback path and requested negative format-capability
checks. The test-only format report is not safe for general public use:
core also treats color attachment support as transient attachment support,
which this HAL does not implement. Normal enumeration remains disabled.

The first rendering attempt exposed a separate core shader-validation panic
when a native device requests `max_inter_stage_shader_variables = 0`:
`wgpu-core/src/validation.rs` subtracts one from zero. Nearby deduction and
error-format arithmetic can also underflow. A limit of one was used for this
fixture because it is supported by the private backend; a separate core fix
and public Noop regression are still required. An independent audit confirmed
zero is a valid native limit request. The public Noop regression reproduced
the underflow before the fix. Signed intermediate budgets now preserve negative
post-deduction values and reject invalid pipelines without panicking; location
error formatting also handles zero. The public test covers limits zero and one,
point lists, front-facing and clip-distance deductions, and location errors.
The full public validation suite passed 91 tests with the expanded regression.
The fixture-enabled core suite passed 68 tests, including the physical FL9_1 rendering fixture and
the fragment error-format test. A first core test build caught missing alloc
imports in the new unit test; these were added and the suite then passed.
Fixture-enabled core Clippy passed. The public suite reported Vulkan loader
warnings about an absent OBS layer manifest; these did not fail tests.

The controlled core fixture now uploads a two-color texture and renders two
overlapping textured shapes with a D24S8 depth attachment through wgpu-core.
On the RTX 3090 Ti at requested and actual FL9_1, strict readback matched all
64 pixels: 32 red pixels where the near geometry occluded the far draw, and 32
green pixels where the far draw remained visible. The first focused run found
an obsolete assertion about the fixture's format flags; after updating that
assertion, the test passed. The fixture's queried format flags and narrow
test limits do not establish a safe general public adapter contract. The
fixture-enabled core suite passed 68 tests and the feature-off zero-adapter
test passed. Independent review confirmed the two draws and their readback
distinguish missing sampling, missing depth testing and missing depth writes.

The private u16 indexed path now accepts signed base vertices. At replay it
checks every raw and adjusted index and its vertex-binding range from the
GPU's current buffer contents before issuing that indexed draw. On forced
FL9_1, positive and negative bases, nonzero vertex/index binding offsets and
a prior GPU index upload rendered 64/64 expected pixels. A negative effective
index caused the expected replay error before the draw; prior copy and clear
commands had already executed. Synthetic tests cover limits and arithmetic.
The strict private suite passed 169 tests, with two filtered; some older tests
can skip internally. Targeted HAL Clippy passed with ten existing warnings.
This conservative path still rejects valid index data above its FL9 limits and
does not implement public robust-draw behavior or u32 indices.

The indexed path now discards a whole draw successfully when any vertex fetch
is outside its bound range, while otherwise-valid indices beyond the native
limit still fail replay. Synthetic tests cover both classifications and their
precedence. On forced FL9_1 a late invalid index suppressed an entire draw;
a later GPU index upload and draw produced the expected black-left, red-right
image and signaled the fence. Existing out-of-range textured tests also passed.
The strict private suite passed 169 tests, two filtered; some older tests may
skip internally. HAL Clippy passed with ten existing warnings. This does not
yet implement u32 indices or translation of otherwise-valid high indices.

With strict hardware mode, `cargo test --offline -p wgpu --no-default-features
--features std,dx11-test-fixture --test dx11_fixture -- --nocapture` passed on
the RTX 3090 Ti at requested and actual FL9_1. The nondefault test injects the
closed unsafe fixture through the public Rust API, verifies zero ordinary
D3D11 adapters before and after injection, and checks 32 red and 32 green pixels
from textured depth rendering. The first build exposed a missing unwrap of the
mapped-range result; after correction the test passed. Targeted fixture Clippy
and `cargo check --offline -p wgpu --no-default-features --features
std,dx12,wgsl` passed. This fixture does not provide a usable public adapter.

An index-translation audit found that rebasing changes the shader-visible
`vertex_index`. The backend's Naga-to-D3DCompile test rejects direct and
structure-member `vertex_index` entry inputs, used or unused, on the installed
FL9_1 compiler with X4555. An unrelated selected entry and the shader-model-5
control compile. A separate installed FXC executable produced the same
diagnostic. Future translation must explicitly preserve or reject this semantic
rather than relying on a compiler's current behavior.

The private pipeline now accepts a depth-only pass with no fragment stage,
using a trivial pixel shader with no render target. The installed FXC rejects
an entirely outputless pixel shader. The strict FL9_1 GPU test wrote depth on
the left half, then read back 32 black occluded and 32 green visible pixels
from a later color pass. The first run revealed that the new pass disturbed an
existing test's baseline; restoring the prior color and 0.75 depth baseline
made that test pass. An independent review found no state or lifetime issue and
prompted correct out-of-memory classification for the fallback compiler. Its
synthetic error test passed. The full private suite passed 170 tests before
that small error-path adjustment; the focused GPU and synthetic tests passed
afterward. Fixture rendering and targeted HAL Clippy passed. Historical FL9_1
driver behavior and fragment shaders without color targets remain unverified.

The private Uint32 index path stages authoritative GPU contents at replay and
converts effective indices up to 65534 into a fresh immutable native Uint16
buffer. It discards whole draws with out-of-range vertex fetches and reports a
replay error for otherwise-valid larger effective indices. Selected vertex
entry points declaring `vertex_index` are explicitly rejected. On the RTX at
forced FL9_1, GPU-written u32 data with both base directions rendered 64/64
red pixels; raw values 65535–65537 with base -3 also rendered correctly.
The 65534 boundary rendered, a valid-fetch 65535 was rejected, and a late OOB
draw was discarded before a subsequent GPU rewrite and draw. The private suite
passed 173 tests before the additional high-raw test was added; the expanded
focused GPU test passed, as did public-fixture rendering and targeted Clippy
(ten existing HAL warnings). Independent review found no concrete defect.
General u32 support, vertex-index semantics, instancing and public exposure
remain incomplete; staging and per-draw allocation incur synchronization cost.

The private u32 path now rebases a single effective-index window with a span
of at most 65534 to a checked vertex-buffer offset. On forced FL9_1, effective
indices 65535–65537 rendered correctly from a nonzero binding offset, followed
by direct and u16 draws that restored their original vertex bindings; all 64
pixels matched. A valid-fetch window spanning 65535 was rejected, and
synthetic checks cover bounds, alignment, overflow and OOB precedence. The
strict private suite passed 173 tests, two filtered; older tests may skip
internally. Independent review found no concrete defect. High stream offsets
remain unverified on historical FL9_1 drivers, and multiple windows or a
single triangle with a wider span remain unsupported.

The opt-in public wgpu fixture now records a GPU buffer copy to a Uint32 index
buffer and uses indexed drawing for the far textured triangle. Strict FL9_1
readback still matched 32 red and 32 green pixels, and ordinary adapter
enumeration remained empty. This remains a controlled unsafe fixture.

Restricted direct instancing now records one logical draw and replays all
triangle-aligned native batches for each instance in order. Selected shaders
declaring `instance_index`, indexed instancing and nonzero first instance are
rejected; general instancing remains unavailable. Strict FL9_1 GPU readback
matched zero/one/two-instance outputs and [40,120,0,0] within one unit for an
overlapping two-instance case across the 65535-primitive boundary. Its RGB-only
standard-alpha state couples native alpha factors because alpha is masked off;
alpha-writing separate-blend portability remains unresolved. The strict private
suite passed 176 tests, two filtered; older tests may skip internally. The
public fixture still rendered correctly and HAL Clippy passed with ten existing
warnings. An independent review found no concrete correctness defect.

Uint32 triangle lists now split into ordered rebased windows, with all indices
and scratch allocations checked before any native draw. A strict GPU test
verified low/high/low window order and a following direct draw. A second test
verified that a later unsupported wide triangle prevents an earlier valid
window from drawing. A synthetic test initially expected three windows for data
that fit in two; its input was corrected and the test passed. The first test
build also caught a missing `alloc::vec::Vec` import, which was added. The
strict private suite passed 177 tests, with two filtered; older tests can skip
internally. The public indexed fixture passed with 32 red and 32 green pixels.
An individual triangle spanning over 65534 effective indices still fails,
and high stream offsets remain unverified on historical FL9_1 hardware.

Restricted indexed instancing now preflights u16 or u32 indices once and
replays every chunk or window for each instance in order. Zero instances do
not stage or draw; selected instance-index inputs and nonzero first instance
remain rejected. Forced-FL9_1 GPU tests cover u16 chunk order, u32 window order,
zero/one/two instances and OOB discard. The strict private suite passed 178
tests, two filtered; older tests may skip internally. An independent review
identified that a saturating u16 case alone could not prove the second instance
executed, so an additional small translucent u16 case was added and passed.
HAL Clippy passed with ten existing warnings after removing a new argument-count
warning; the focused GPU test passed again after that change.

The private u32 path now gathers vertices for individual triangles spanning
more than 65534 effective indices. All draw bounds and scratch allocations are
preflighted, then GPU buffer copies gather only each validated attribute extent
into zero-initialized vertex scratch. Ordered window/gather segments replay in
instance order. On the forced-FL9_1 RTX, strict readback passed for GPU-written
indices and vertices, a negative base, nonzero binding offset, a final vertex
without trailing stride padding, 0/1/2 instances, mixed segment ordering and
late OOB discard. The private suite passed 178 tests, two filtered, and the
public indexed fixture passed. An independent review found no concrete defect;
historical-driver copy behavior remains unverified and each gathered triangle
currently requires a scratch allocation and three GPU copies.

To prevent draw splitting or gathering from changing a shader-visible ID, the
private pipeline now explicitly rejects `primitive_index` on the selected
fragment entry point, including struct members and unused declarations. Other
entry points remain unaffected. Validated Naga tests, actual pipeline creation
and Naga-to-FXC checks passed; the installed FL9_1 compiler reported X4583 and
X4568 for the builtin. The first test build caught a borrowed test shader being
destroyed too early; isolating the descriptor fixed it. The strict private suite
then passed 180 tests, with two filtered; older tests may skip internally.
Independent review found no concrete defect. The public indexed fixture and
HAL Clippy passed; Clippy retained ten existing warnings.

Within the restricted instancing subset, nonzero first-instance values are now
accepted without narrowing arithmetic because no selected shader or vertex
input can observe them. Strict GPU tests covered first-instance 0, 7 and
u32::MAX with direct and indexed draws. Uint16 draws that exceed FL9 native
index limits now use the same validated windows and GPU gather as Uint32 while
the native fast path remains for supported values. GPU readback verified a
65535 gather, negative- and positive-base windows, and restoration for a
subsequent native draw. The first full-suite run exposed five obsolete tests
that expected nonzero first instance to fail; their negative coverage was
changed to invalid ranges/alignment. The rerun passed 181 tests, two filtered;
older hardware tests can skip internally. Independent review found no concrete
defect in the translation or instance semantics. The public indexed fixture
passed again, and HAL Clippy passed with ten existing warnings.

Zero-input vertex pipelines and nonindexed zero-input point lists are now
supported privately. The baseline pipeline test failed before the change. On
the forced-FL9_1 RTX, an interior point produced one green and 63 blue pixels;
point-to-triangle-to-point transitions produced one green and 63 red pixels.
A 65,536-point GPU call was accepted, and CPU checks verified its 65,535/1
batch split and instance order. Zero-input native batches use start vertex zero
because shader-visible vertex IDs are rejected. Indexed points and point
pipelines with vertex buffers remain unsupported. The private suite passed 182
tests before the strengthened point tests; the focused GPU and CPU tests passed
afterward. The public indexed fixture and HAL Clippy passed, with ten existing
warnings. Historical point raster and driver behavior remain unverified.

FL9_1 now rejects alpha, inversion and vignette blend modes when both RGB and
alpha writes require unguaranteed separate-alpha support. Masked RGB-only and
alpha-only variants use equivalent coupled factors, and enabled blending
requires a positive format query. The focused strict test first demonstrated
the missing rejection, then passed on the RTX at forced FL9_1 with complete
8x8 readback for all three masked blend modes. Full-channel tests retain an
explicit test-only diagnostic opt-in and do not establish portability. The
strict private suite passed 184 tests, two filtered; older tests may skip
internally. The public indexed fixture passed with 32 red and 32 green pixels.
HAL Clippy passed with ten existing warnings. Independent review checked the
mask/factor combinations and device-error handling without finding a defect.
Checkpoint: `d01ace12a`.

Ordinary BGRA8 textures are the next resource increment. Before implementation,
`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_bgra_upload_copy_and_readback -- --nocapture` in strict hardware mode
selected the RTX at FL9_1 and failed at BGRA texture creation (`Unexpected`).
This baseline establishes missing functionality, not a GPU transfer failure.
After implementation, the strict BGRA filter passed four tests, including three
physical-GPU cases: asymmetric upload/copy/readback with padding; sampling raw
BGRA `[16,64,224,255]` as logical RGBA `[224,64,16,255]`; and rendering then
copying a partial region between different-sized textures with initialized
untouched pixels. Synthetic checks cover missing format support, incompatible
views and cross-format copies. The strict private suite passed 187 tests, two
filtered. HAL Clippy passed with ten existing warnings. A read-only independent
review found no concrete defect; historical GPU behavior is unverified.
Checkpoint: `86a5ddfb5`.

The core-validation baseline requested an occlusion query through the
controlled FL9_1 fixture and failed as `Device(Lost)`. FL9_1 does not guarantee
occlusion queries; a new downlevel availability flag now rejects that request
before HAL. The strict fixture then continued to GPU upload and textured depth
readback (64/64 bytes and 32 red/32 green pixels); all 68 fixture-enabled core
tests passed. A synthetic GLES version/extension test and a compliance test
passed; core and GLES Clippy passed. Custom HAL adapters and stored flag sets
must add the new flag for supported queries. GLES already uses conservative
query results, a separate pre-existing semantics issue this does not resolve.

The existing query GPU tests passed on DX12 (four RTX and four Microsoft Basic
Render Driver cases), desktop GL (four RTX cases), and Vulkan (four RTX cases).
DX12 query initialization reported 18 executed and four unsupported cases;
GL reported nine executed and two unsupported cases. A Noop creation test also
passed. The first parallel test-harness invocation aborted because isolation
requires `--test-threads=1`; rerunning serially passed. Vulkan initially failed
because the loader referenced a missing OBS layer manifest. Setting only the
test process's `VK_IMPLICIT_LAYER_PATH` to an empty temporary directory allowed
the Vulkan tests to pass, with a remaining registry lookup warning. This does
not repair the system's Vulkan configuration. The strict public wgpu fixture
also passed on the RTX at FL9_1 with 32 red and 32 green pixels.
The GPU report was generated at ignored `.gpuconfig`; the process-local Vulkan
layer override used the empty directory `target/codex-empty-vk-layers`.

The query regressions used `cargo test --offline -p wgpu-test --test wgpu-gpu
-- occlusion_query --nocapture --test-threads=1` with `WGPU_BACKEND` set in
turn to `dx12`, `gl`, and `vulkan`. The initialization runs used the same
command with `resolve_query_set_init` as the filter, on DX12 and GL. The
focused synthetic commands were `cargo test --offline -p wgpu-types --lib
occlusion_queries_are_required_for_webgpu_compliance` and `cargo test --offline
-p wgpu-hal --lib --no-default-features --features gles
conservative_occlusion_query_availability`. The core suite used
`cargo test --offline -p wgpu-core --lib --no-default-features --features
dx11-test-fixture,std`; strict hardware mode was enabled for the fixture.
Checkpoint: `17b840c1d`.

The next private increment is a read-only D24S8 attachment without sampled
aliasing. Its strict hardware baseline first failed to compile because a test
cloned a generic descriptor with an unsupported bound; after correcting the
test, the RTX at requested/actual FL9_1 failed read-only texture creation with
`Unexpected`. No public read-only-depth capability is implied by this work.
The private implementation now accepts READ-only textures/views and passes
only with LOAD|STORE for both aspects and a non-writing pipeline. A strict GPU
test on the RTX verified 32 green/32 blue pixels for depth preservation, then
32 red/32 blue pixels across READ→WRITE→READ submissions after releasing
caller resources. It rejects write-enabled pipeline transitions, clears,
discard, don't-care and depth sampling aliases. The full strict private suite
passed 188 tests, two filtered, before the added transition/lifetime assertions;
the expanded focused test passed afterward. HAL Clippy passed with ten existing
warnings. Independent review found no concrete defect. Historical support for
the zero depth-write mask remains unverified.
Checkpoint: `03cf563b1`.

The next shader baseline demonstrates that a selected checked shader with a
uniform and local-vector dynamic indexing is rejected by the existing blanket
guard. The new selected-entry, typed analysis accepts only proven local/value
vectors and leaves uniform pointers, arrays, matrices and unknown types rejected.
The focused strict GPU run executed three cases (in-range vector selection,
upper out-of-range selection and a uniform copied to a value), each with 64/64
pixels matching. Two further cases explicitly skipped GPU execution after the
tested FXC rejected potentially negative indices with X3548 and dynamic vector
stores with X3500. The complete private suite passed 194 tests, two filtered,
before final boundary-test refinements; those two GPU skips are included in the
libtest count and are not hardware passes. The tightened focused run passed six
libtest cases: three executed on GPU and three skipped GPU execution after X3548
or X3500, including an added out-of-bounds store case. The guard diagnostic is
asserted exactly and unexpected compiler failures fail the test. HAL Clippy
passed with ten existing warnings. Historical drivers remain unverified.
Checkpoint: `7c4fd052c`.

The next topology baseline, `fl91_nonindexed_triangle_strip_winding_and_first_vertex`,
selected the RTX at requested/actual FL9_1 and failed pipeline creation with
the existing unsupported-topology error. Nonindexed strips now work privately;
indexed strip restart remains excluded because not all 10level9 hardware
supports native strip cuts. The D3D9 65,535-unique-vertex caveat requires
conservative, even-parity overlapping batches rather than assuming the full
primitive count can be consumed by one strip.
The strict RTX test compared both winding and cull modes with triangle lists,
checked the second batch of a 65,536-vertex strip, rejected indexed strips and
verified a nonzero-offset strip-to-list transition with 32 red/32 green pixels.
The CPU batch test checked overlap and instance order. Naga→FXC reported X4568
for one flat-interpolation declaration; it did not execute on GPU. The combined
strict suite and Clippy results are recorded below. Rebased stream offsets and
strip behavior remain unverified on historical FL9_1 drivers.
Checkpoint: `ec8dbf12e`.

Independent shader review found that Naga's HLSL matrix-in-struct store
lowering can emit an unchecked dynamic component index and that some related
store helpers take destinations by value. The installed FL9 FXC rejects the
raw dynamic store with X3500 in both vertex and pixel profiles; this does not
establish safety on all compilers. Checked-shader fail-closed validation and
static-store correctness tests are in progress, including shaders without
uniforms and pointer-helper paths. Do not infer completion from the prior
local-vector GPU passes.
CPU regression baselines `checked_matrix_component_stores_require_safe_lowering`
and `matrix_struct_stores_require_correct_lowering_even_unchecked` both failed
before the fix, confirming the existing classifier accepts the problematic
shapes. The guard must cover stores and pointer call arguments through nested
struct/array paths in both checked and unchecked modes, while leaving safe
read-only matrix use available.
The implemented guard fails closed for affected aggregate stores and unknown
pointer provenance, tracks call-site provenance, and bounds the work and
nesting of its type and call-graph analysis. Five focused matrix tests passed,
including shared type DAGs, call-state limits and shared work exhaustion; the
checked/unchecked shader-module test passed. The combined strict private suite
passed 203 libtest cases with two filtered; three local-vector cases explicitly
skipped GPU execution, as noted above. Independent review found no concrete
provenance bypass. The guard conservatively rejects writes to other fields in
an affected aggregate and sufficiently complex valid shaders; it does not fix
the shared Naga lowering.
Checkpoint: `d77a14387`.

Vertex-input overlap audit: the FL9_1 minimum does not guarantee overlapping
elements in one input stream. The strict
`fl91_vertex_element_overlap_is_rejected` baseline created adjacent and
reverse-ordered disjoint controls on the RTX at requested/actual FL9_1, then
failed because the backend accepted a same-start overlap. A selected-level
fail-closed validation change now rejects same-start, partial and nested byte
ranges while accepting adjacent and disjoint ranges. The strict production
test and synthetic checks across seven feature-level identities passed; the
latter do not claim higher-level execution support. Success on this modern
driver is not evidence of historical stream-offset or stride support.
Checkpoint: `3cdbb28f6`.

Additional matrix-guard regressions passed for a bare matrix and matrix-pointer
helper, matrix arrays, safe/unsafe helper call order, an unselected unsafe
entry, checked and unchecked compilation, and the combined call-depth boundary.
The strict private suite passed 206 libtest cases, two filtered; the three
documented shader cases still skipped GPU execution. HAL Clippy passed with
ten existing warnings. Independent overlap review found no concrete defect.
Test checkpoint: `07b95c801`.

The direct triangle-list batching baseline failed a CPU boundary test: a
65,538-vertex draw was issued as one batch. D3D9 documents a 65,535-unique-
vertex caveat when MaxPrimitiveCount is 0xffff; whether 10level9 emulates that
restriction is unresolved. A conservative FL9_1-only direct-draw limit is in
progress, leaving indexed batching separate.
The new FL9_1 policy caps direct triangle-list batches at 65,535 vertices;
CPU tests passed for split points, tails, overflow and instance order, and a
strict RTX GPU test matched all 64 pixels with visible geometry on each side
of the split. The opaque GPU output does not independently prove instance
order. The complete strict private suite passed 208 libtest cases, two filtered,
with three shader GPU skips as documented; HAL Clippy retained ten existing
warnings. Independent review found no concrete defect. Large native first
vertex offsets remain unverified on historical devices.
Checkpoint: `894aa40c4`.

The next private increment targets sparse multiple vertex-rate streams for
direct triangle lists. Indexed window/gather and strip rebasing currently
assume one slot; their multi-slot variants must remain rejected until every
stream can be validated and translated together.
The strict `fl91_sparse_vertex_streams_render` baseline selected the RTX at
requested/actual FL9_1 and failed pipeline creation for position slot 0 plus
color slot 2, as expected from the existing single-slot restriction.
The direct-only implementation now supports sparse vertex-rate slots with
independent range checks and replay bindings. The first GPU run produced green
127 instead of the test's 128 due to interpolation/UNORM rounding; a 1-LSB
tolerance for that channel retained the test's discriminating output. The
expanded strict RTX test then passed, covering distinct and shared buffers,
slot 15, rejection at 16, transitions, a queued copy with source/destination
released before submission, and an exact last-attribute range plus one-byte-
short rejection. Physical overlap across slots and historical drivers remain
unverified. Indexed and strip multi-slot draws are still rejected.
The strict private suite passed 209 libtest cases, two filtered, with three
documented shader GPU skips. HAL Clippy passed with ten existing warnings.
The controlled core and public wgpu fixtures each passed on the RTX, including
their 32 red/32 green textured-depth readback. Independent review found no
concrete slot, bounds, stale-state or lifetime defect.
Checkpoint: `82a538433`.

Nonindexed strip batching now preflights and rebases every active vertex stream.
Indexed multi-stream draws remain rejected until their GPU index scan, windows
and gather handling cover all streams.
The strict `fl91_sparse_vertex_strips_render` baseline selected the RTX at
requested/actual FL9_1 and failed at end_encoding with Unexpected, confirming
the existing multi-slot strip restriction.
The strict RTX sparse-strip test then passed with slots 0/15, different strides
and offsets, a nonzero first vertex, a 65,536-vertex split, queued GPU copy and
resource destruction before submission. Every pixel matched. The opaque,
unculled output does not independently prove instance order or split parity;
existing CPU batching and single-stream culling tests cover those aspects.
Independent review found no concrete per-slot rebasing or lifetime defect.
The strict private suite passed 210 libtest cases, two filtered; the three
documented shader GPU skips remain skips. HAL Clippy passed with ten existing
warnings, and the controlled core fixture again matched 32 red/32 green pixels.
Checkpoint: `9a1efde6c`.

Indexed replay errors are not benign public validation: an Unexpected submit
error loses the core device and does not signal the requested fence. A future
partial indexed multi-stream path must avoid data-dependent unsupported replay
errors or remain explicitly private; earlier GPU work is not rolled back.
A private native-u16 multi-stream subset is accepted only when
binding sizes and base vertex prove at encoding that every in-range index fits
native FL9_1 limits. GPU index data still requires a complete replay-time OOB
scan; general u32 and high-index multi-stream translation remains future work.
The strict `fl91_sparse_vertex_indexed_render` baseline failed at end_encoding
on the RTX at requested/actual FL9_1, as expected from the existing restriction.

The focused strict RTX test passed after implementation, checking full-image output,
nonzero first index, positive and negative base vertices, two additive instances,
GPU index upload, resource release before submission, and whole-draw OOB discard.
Two CPU proof tests passed, including exhaustive checks over all 65,536 Uint16
values for 42 representative configurations. The complete strict private suite
passed 213 cases, two filtered, with three documented internal shader skips.
HAL Clippy passed with ten existing warnings. The core hardware fixture passed
with 64/64 buffer bytes and 32 red/32 green pixels; ordinary core enumeration
still returned zero D3D11 adapters. Independent review found no concrete issue.
The native subset stages and maps index data at replay and may stall; the general
multistream translation and public validation remain blockers.
Checkpoint: `9bee6efe5`.

Multi-stream indexed draws now scan all GPU-written Uint16/Uint32 indices against
every bound stream before drawing; any OOB fetch discards the entire draw.
Native-safe Uint16 keeps the direct path. Other data is translated to ordered
common index windows, with per-stream rebasing or per-stream GPU gather for
wide triangles. All segment resources are prepared before native draw commands.
The three strict `fl91_sparse_vertex_indexed` GPU cases passed on RTX at
requested/actual FL9_1: native u16, u16 gather to native transition with signed
base and instances, and u32 window/gather plus late OOB and retained GPU data.
Independent review found no concrete scan, ordering or lifetime defect.
The first combined suite found one obsolete test still expecting Uint32 rejection;
it was changed to test an index-buffer range that is actually out of bounds.
The corrected strict suite passed 216 cases, two filtered, including the new
transient tests; three previously documented shader cases still skip GPU work.
HAL Clippy passed with ten existing test-target warnings. The controlled core
fixture also passed its texture validation checks and 32 red/32 green readback.
Checkpoint: `954e07c28`.

Transient color and D24S8 attachments now use ordinary D3D resources and retain
transient usage on textures and views. Unsupported combinations are rejected;
the allocation hint does not imply memoryless storage. The strict transient
filter passed one CPU test and one RTX FL9_1 hardware test, with 64 matching
pixels each for transient depth and transient color observed through stored
depth. The core fixture verified color/depth creation and views, rejection of
invalid transient/unsupported format or storage usages, and subsequent ordinary
GPU rendering. It does not yet exercise a core-level transient pass. The full
strict suite passed 216 cases, two filtered, including these tests; HAL Clippy
passed with ten existing warnings, and normal core enumeration stayed empty.
An independent review found no concrete correctness issue. Historical drivers
and memoryless optimization are unverified.
Checkpoint: `672db3fb3`.

FL9_1 overlapping vertex elements now use deterministic native slot groups,
while logical buffer bindings and range checks remain unchanged. The strict
`overlapping_vertex` GPU test passed nine full-image executions on the RTX,
covering exact/partial overlap, lists, strips, pipeline transitions and u32
window/wide-gather draws. The slot allocator CPU test passed. The controlled
core fixture rendered both disjoint and partially overlapping layouts, each
with 32 red and 32 green pixels. The five strict sparse vertex regressions
passed. Independent review found no concrete mapping or lifetime defect;
historical FL9_1 drivers and remaining vertex formats/strides are unverified.
The complete strict D3D11 suite passed 218 cases, two filtered, with the three
previously documented internal shader GPU skips. HAL Clippy passed with ten
existing warnings; core ordinary D3D11 enumeration remained empty.
Checkpoint: `53abef8bc`.

A restricted stencil-state classifier accepts only states proven to always pass
on both faces and never change the eight-bit stencil value; these are implemented
with native stencil disabled. Reference updates inside passes are ignored for
this subset. Strict `stencil_noop` passed one CPU classifier test and one RTX
FL9_1 hardware test comparing IGNORE with masked Equal across two passes,
reference/pipeline changes and a read-only depth pass; invalid states and an
outside-pass reference were rejected. Actual stencil bits cannot yet be read.
The controlled core fixture also produced 32 red/32 green pixels using masked
Equal, Replace with zero write mask and a nonzero reference. The strict suite
passed 220 cases with two filtered before the final test-only boundary additions;
the updated focused test passed. Three historical shader cases still skip GPU
work. HAL Clippy passed with ten existing warnings and core enumeration stayed
empty. Independent review found no concrete classifier defect. Future stencil
support must retain and rebind reference state across pipeline switches.
Checkpoint: `c9a4105b2`.

Snorm16x2 and Snorm16x4 vertex inputs now map to the corresponding native
signed-normalized input formats. Microsoft lists these IA formats as required
on FL9_1. The strict RTX test queried flags 0x21122 and 0x2, and direct and
indexed rendering matched 64/64 pixels using negative endpoints, zero and
positive intermediate values. The controlled core fixture selected colors
using Snorm16x2 UV and produced 32 red/32 green pixels; its y component is not
independently observable. The CPU mapping test and independent review passed.
The complete strict private suite passed 222 cases, two filtered, with three
previously documented internal shader skips; HAL Clippy passed with ten
existing warnings and ordinary core D3D11 enumeration remained empty. The
private four-byte alignment remains; no SNORM texture or historical-device
support is inferred.
Checkpoint: `cca55a8f0`.

Zero-stride vertex streams now retain a constant bound record for direct and
strip draws and through indexed windows/gathers. The strict `zero_stride` filter
passed one CPU and one RTX FL9_1 GPU test: five full-image executions cover
mixed streams in slots 0/15, direct, strip, u16 and u32 window/gather paths,
updates, transitions and an undersized binding rejection. The all-zero-stream
triangle only establishes degenerate completion. The controlled core fixture
first hit its synthetic vertex-stage buffer limit of one; after raising that
test-only limit to two, it passed with 64 green pixels from constant UV data.
The complete strict suite passed 224 cases, two filtered, with the prior three
internal shader skips; HAL Clippy passed with ten existing warnings and core
ordinary enumeration stayed empty. Independent review found no concrete memory
or ordering defect. A negative effective indexed vertex with only zero-stride
streams is conservatively discarded even though its constant addresses may be
in bounds; its observable effect is unresolved beyond the current degenerate,
side-effect-free triangle subset. Historical hardware remains unverified.
Checkpoint: `cc8e1aa68`.

Restricted instance-rate vertex streams now bind each instance's source record
as a zero-stride native vertex stream before repeated ordinary draws; logical
bounds are preflighted using wide arithmetic, and indexed rebasing/gathers use
only vertex-rate streams. The strict `instance_stream` filter passed two CPU
and one RTX FL9_1 GPU test. Four full-image executions covered nonzero first
instance, sparse vertex/instance slots, zero-stride tint, GPU-written data,
retention, direct/strip/u16/u32 windows and gather. Overlap at pixel (4,7)
independently distinguishes correct instance-outer ordering. The controlled
core fixture produced 32 red/32 green pixels for two instance records; the
unsafe public-wgpu fixture still produced its 32 red/32 green indexed scene.
The first strict full suite found an obsolete rejection assertion; after updating
it, 228 cases passed with two filtered and three documented internal shader
skips. HAL Clippy passed with ten existing warnings and ordinary core D3D11
enumeration remained empty. Independent review found no concrete defect.
All-instance-only indexed draws and instance_index remain unsupported.
Checkpoint: `8653ba625`.

Corrected the internal maximum vertex-buffer count from the obsolete single-
stream value to the backend's 16-slot limit. The synthetic capability tests
passed (4 cases); slot 15 and sparse bindings were GPU-tested previously, but
simultaneous execution using all 16 slots has not been established. This does
not expose a public adapter.
Checkpoint: `fc5cfb75e`.

Uniform bindings now retain any nonzero logical size up to 64 KiB rather than
requiring a 16-byte multiple. The native constant/staging buffers are rounded
up, with only the exact GPU-copied bytes materialized into a pre-zeroed scratch
buffer. The strict HAL tests checked 4/5/12/20-byte ranges, a five-byte dynamic
range following GPU copy and a five-byte range at a 65-byte buffer end; logical
bytes and native padding matched readback. The controlled core fixture initially
failed UnalignedShader with its downlevel flags empty; after enabling only the
fixture's BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED flag, 4/5/12/20-byte cases rendered
64 expected pixels each, including dynamic offset. This is not public exposure.
Three strict integer-uniform shader cases (vertex/fragment bitcast and fragment
comparison at 2^24) passed 64 pixels each on the RTX; FXC disassembly raised a
possible mismatch that those cases did not reproduce. Integer vertex inputs and
general integer/historical semantics remain unverified and unadvertised.
The strict private suite passed 233 cases, two filtered, with three documented
internal shader skips. HAL Clippy passed with ten existing warnings; ordinary
core D3D11 enumeration stayed empty. Two independent reviews found no concrete
uniform materialization defect. Odd-byte GPU copy behavior remains unverified
on historical drivers, and staging maps may stall.
Checkpoint: `61d1e7a3e`.

For FL9_1 samplers with one mip, equal min/mag filters and anisotropy 1,
previously rejected finite LOD intervals normalize to native 0..FLT_MAX.
The sampler is marked single-mip-only; current bind-group validation also
requires sampled textures and views to have one mip. Strict `single_mip` tests
passed three cases, including 32 patterned RTX FL9_1 readbacks across nearest/
linear spatial and mip filters, magnification/minification and intervals
0..0, 0.5..0.5 and 1..4 versus reference. The controlled core fixture's
fractional sampler case matched all 64 pixels. Two independent reviews found
no concrete defect for the current one-mip subset. Explicit LOD/bias, mixed
filters with these clamps, anisotropic cases and historical drivers remain
unverified or rejected. The complete strict suite passed 235 cases with two
filtered before the final test-only mip-linear extension and model-field update;
the updated focused GPU and four capability CPU tests passed. HAL Clippy passed
with ten existing warnings and core ordinary enumeration remained empty.
Checkpoint: `62ef2660b`.

Copy-only texture views may now retain metadata without native SRV/RTV objects;
unsupported sampling or attachment use remains rejected. The strict private
view test passed on RTX FL9_1, including retained resource lifetime. The
controlled core fixture created RGBA8 and BGRA8 copy-only views, rejected
sampled/render view usages and matched 12 GPU-transferred pixels for each.
The combined strict suite passed 237 cases, two filtered, with three documented
internal shader skips. HAL Clippy passed with ten existing warnings; core
ordinary D3D11 enumeration remained empty. Independent review found no concrete
ownership or usage defect.
Checkpoint: `6a64195ad`.

FXC's current level-9 pixel profile emits unclamped legacy texldb for dynamic
SampleBias, whose documented +/-3 range is narrower than WGSL's allowed bias.
The selected-entry shader path now conservatively rejects reachable SampleBias
in checked and unchecked modes, including helper calls. Profile selection is
shared with compilation so the guard also applies on higher hardware while
this backend still targets FL9_1 profiles. Strict `bias` tests passed one CPU
case and one RTX FL9_1 hardware case: direct/helper bias pipelines rejected
with the specific linkage error and an ordinary sample from the same module
matched 64 pixels. Two reviews found no further concrete traversal or profile
defect after the initially identified device-level gating error was fixed.
The complete strict suite passed 237 cases, two filtered, with three documented
internal shader skips; HAL Clippy passed with ten existing warnings and ordinary
core enumeration remained empty. Conservatively rejected bounded bias and
historical-device behavior remain future work.
Checkpoint: `1f48812f7`.

A restricted POT RGBA8/BGRA8 copy-only mip chain is implemented, requiring
COPY_SRC|COPY_DST, one layer/sample and positive native format MIP support.
Every mip is zero-initialized; upload, readback and cross-resource copies carry
checked native subresource indices. Metadata-only views can select valid mip
ranges. Strict `mip` tests passed six cases, including RTX FL9_1 RGBA/BGRA
four-level initialization, padded partial uploads, cross-mip copies and full
readback: two format cases executed and passed, none skipped. The controlled
core fixture checked 43 pixels over four mip levels. Missing format support now
skips explicitly in normal mode and fails strict mode; query errors in the core
fixture remain failures. Three independent reviews found no concrete transfer,
initialization or ownership defect. The complete strict suite passed 241 cases,
two filtered, with three documented internal shader skips; two new Clippy test
warnings were corrected and the focused capability test passed afterward.
Clippy then had the existing ten warnings, and ordinary core enumeration stayed
empty. Sampling/rendering mips, NPOT mips, same-resource copies and historical
drivers remain unverified or unsupported; FL9 SRVs must include the smallest
mip, blocking arbitrary core-valid sampled views.
Checkpoint: `352c415eb`.

POT RGBA8/BGRA8 mip chains can now be sampled through full-chain D2 views
when texture usage is exactly COPY_SRC|COPY_DST|RESOURCE. Per-sampler LOD
translation metadata is checked against actual texture mip count only for
selected shader sampling pairs; a rewritten minimum remains one-mip-only, and
a raised maximum requires coverage of the last mip and non-anisotropic
filtering for multi-mip views. Strict `sampled_mip` passed 46 RTX FL9_1 GPU
executions with no format skips, including four selected levels, nearest/linear
mip filtering, fractional blending, used/unused pair validation and same-pass
transitions with resources destroyed before submission. The core fixture
sampled mip1 blue and checked all 64 pixels; the unsafe public-wgpu fixture's
existing indexed textured-depth scene still matched 32 red/32 green. The full
strict suite passed 244 cases with two filtered before the final test-only
lifetime addition; its updated targeted test passed. Three prior shader GPU
cases remain internally skipped. HAL Clippy passed with ten existing warnings,
and ordinary core D3D11 enumeration remained empty. Independent source and
GPU-test reviews found no concrete defect. Non-full sampled views, attachment
mips, NPOT chains, certain LOD/aniso pairs and historical drivers remain gaps.
Checkpoint: `e066d405c`.

Private POT RGBA8/BGRA8 sampled mip views may now start at any mip if the
view includes the resource's last mip, as required by FL9. Sampling-pair LOD
validation uses the view's dimensions and relative mip count. Strict RTX
FL9_1 testing passed 54/54 sampled-mip cases with no format skips, including
relative LOD, finite max LOD, single-level suffixes, lifetime and rejection of
non-suffix views. The controlled core suffix-view fixture read 64/64 blue
pixels. The full strict private suite passed 244 tests with two filtered; three
older shader cases skip hardware internally. HAL Clippy passed with ten
existing test warnings. Arbitrary non-suffix sampled views and mip attachment
views remain unsupported, and public adapter exposure remains disabled.
Checkpoint: `34794bb52b7389492ffcbd989c09d4de8121c95f`.

POT sampled mip textures now support COPY_DST|RESOURCE without requiring
COPY_SRC. The native texture has only shader-resource binding; exact usage
checks continue to reject copy-source and direct readback. Driver mip and
sample support remain required. Strict sampled-mip GPU tests passed 68/68
executions across RGBA8/BGRA8 with zero format skips, including seven new
uploads, suffix selections and untouched-mip zero checks per format. Controlled
core fixtures sampled 64/64 blue uploaded pixels and 64/64 transparent zero
untouched pixels. Synthetic model and native bind-flag/usage tests passed.
The full strict suite passed 244 tests with two filtered and three older shader
hardware cases skipped internally. HAL Clippy passed with ten existing test
warnings, and the core zero-adapter test passed. An independent review found no
concrete issue. Public enumeration stays off.
Checkpoint: `4a4b6eec90c12a8d3ba35629311ab1d58e052284`.

Next: individual FL9 2D render-target mip views. Core creates an internal
attachment view for each mip, including on render-only textures; native RTV
descriptors and render-pass extents must select the actual mip. Keep sampled
and attachment mip usage disjoint and preserve whole-resource alias checks.

Individual POT RGBA8/BGRA8 color mip attachments now use explicit 2D RTV mip
slices and mip-relative render extents, for COLOR_TARGET with optional copy
usages. RESOURCE and TRANSIENT combinations remain unavailable. Strict RTX
FL9_1 private execution passed both formats, verifying all chain pixels, padding,
selected clears/draws, untouched initialization, invalid views and lifetimes.
Controlled core execution checked 43/43 pixels for each format and separately
submitted render-only mip clears. Independent production review found no
concrete issue. The full strict private suite passed 245 tests, with two filtered
and three older shader hardware cases skipped internally. HAL Clippy passed
with ten existing test warnings, and ordinary core adapter enumeration stayed
empty. Historical devices and public adapter exposure remain unverified.
Checkpoint: `68c6a804d13cada3d1cb0feccef6bbb3d1d58d7f`.

Next: combined sampled and color-attachment mip textures in separate passes.
FL9 permits a combined SRV/RTV view only for the last mip, since an RTV selects
one level and an SRV must include the resource's last level. Preserve explicit
usage restrictions and reject whole-resource feedback; core-valid non-last
combined views remain a public integration gap.

POT RGBA8/BGRA8 textures now permit combined RESOURCE|COLOR_TARGET usage with
optional copy flags. RESOURCE views are suffixes, attachment views select one
mip, and a dual-use view selects only the last mip. Strict RTX FL9_1 private
testing passed six GPU transition cases across both formats, including feedback
rejection and resource lifetime; controlled core testing passed four cases
with 64/64 expected pixels each. Whole-resource alias rejection remains more
restrictive than core for disjoint mips. An independent review found no
concrete issue. Public adapter exposure remains disabled.
The first full-suite run had 245 passes and one failure because an older
negative test still expected a two-mip combined-usage texture to be rejected.
That assertion now checks a four-level chain on a 4x4 texture, exceeding the
valid mip count; its focused hardware test passed.
The rerun passed 246 tests with two filtered and three older shader cases
skipping GPU execution internally. HAL Clippy passed with ten existing test
warnings; ordinary core D3D11 enumeration remained empty.
Checkpoint: `2206731ded87a5cdc1c9bb3c557191027c10397b`.

Next investigation: finite literal textureSampleBias within the documented
legacy instruction range. Preserve selected-entry validation, reject dynamic
or out-of-range biases and account for anisotropic and LOD semantics before
allowing it; hardware execution and compiler evidence are required.

Selected FL9 shader entries now accept offset-free finite literal sample biases
in [-3,3]. Used pairs conservatively require equal min/mag filters and
anisotropy 1; dynamic values, helper arguments and offsets still reject. The
strict RTX test passed nine 64-pixel GPU renders, including fractional linear
mip filtering, with invalid shader and direct/indexed sampler checks. A core
fixture produced 64/64 blue pixels using a literal bias. The full strict suite
passed 249 tests before the final test-only linear/indexed additions; those
passed in the updated focused test. Three older shader cases skip hardware
internally. HAL Clippy retained ten pre-existing test warnings, the DX12 HAL
compiled, and ordinary core adapter enumeration remained empty. An independent
review found no concrete defect. Historical driver behavior remains unverified.
Checkpoint: `d62f52eaca76e4a8ff50eabbc9ee09910d94ffb5`.

Next investigation: emulate core-valid non-suffix sampled mip views without
stale contents, feedback bypass or unsupported native FL9 SRVs. A per-view
copy must reflect execution-time writes, preserve original resource identity,
and not unbind unrelated shader state. Keep public exposure disabled until all
core-visible view and presentation contracts are safe.

After the bias checkpoint, the gated public-wgpu regression passed again on
the RTX FL9_1 device: the indexed textured-depth fixture read 32 red and 32
green pixels. This nondefault unsafe fixture does not enable public enumeration.

Non-suffix sampled POT RGBA8/BGRA8 views now have a private GPU-copy emulation:
a view-specific truncated texture is refreshed from the retained original just
before each draw, then all active shader resources are rebound. The original
identity remains authoritative for whole-resource feedback checks. Strict RTX
FL9_1 tests passed 16/16 GPU cases across both formats, and the core fixture
read 64 yellow pixels after an update following view creation. The full strict
suite passed 250 tests with two filtered; three older shader cases skip GPU
execution internally. A build initially exposed non-Send retained IUnknown and
test closure lifetime errors; typed resource retention with transient canonical
identity checks and independently scoped test borrows corrected them. An
independent review found no concrete defect. The updated focused test includes
recorded upload followed by indexed sampling in one submission, with input
buffers destroyed before submit, and passed for both formats. HAL Clippy passed
with ten existing test warnings; the zero-adapter core test passed. The full
suite preceded those final test-only additions. Each use incurs GPU copies and
clone memory. The unsafe public-wgpu fixture again passed its indexed textured
depth check with 32 red and 32 green pixels. Historical drivers and public
adapter exposure remain unverified.
Checkpoint: `9e9c21392b0996c1ea2b6489c11c268c37379206`.

Next: use that clone path for a single non-final mip view with both RESOURCE
and COLOR_TARGET usages, with rendering and sampling in separate passes.
Keep the original-resource feedback guard and validate fresh copies after render.

Non-final single-mip dual RESOURCE|COLOR_TARGET views now use the original RTV
and a refreshed clone SRV. The strict RTX FL9_1 test passed eight GPU cases over
both color formats, including repeated render/sample transitions, early resource
destruction and feedback rejection. Controlled core default-view cases each
matched 64 blue pixels for RGBA8 and BGRA8. Independent review found no concrete
defect. The full strict suite passed 251 tests with two filtered and three older
shader cases skipping GPU execution internally. HAL Clippy passed with ten
existing test warnings; core's zero-adapter test passed. Same-pass whole-resource
feedback and public enumeration remain gated.
Checkpoint: `b431ff5d013dc9050f729b00a92973f961676ff1`.

Next investigation: whether FL9 and core permit simultaneous non-overlapping
mip sampling and attachment use, and whether clone refresh can be ordered
without reading a resource while its output view is bound. Preserve conservative
whole-resource rejection until the native and core contracts are established.

D3D11's documented disjoint-subresource contract supports a restricted private
case: a native suffix SRV may be sampled while a different mip of the same
texture is attached. The entire sampled view range is checked, not only shader
accesses; clone-backed same-resource views remain rejected. The first strict
test passed RGBA but failed BGRA because the test used an RGBA-only diagnostic
readback. Replacing that with recorded texture-to-buffer readback yielded six
of six GPU cases across both formats, with no skips. Controlled core fixtures
also matched 64 pixels per format. Historical FL9 hardware is unverified and
the public adapter remains disabled. The full strict private suite passed 253
tests with two filtered; three older shader cases skip GPU execution internally.
HAL Clippy passed with ten existing test warnings, and the core zero-adapter
isolation test passed. Independent review found no concrete alias-check bypass.
Checkpoint: `07eaee830f8600b1b56391089ab3df03a20bcddf`.

Next investigation: disjoint clone-backed views of the current attachment.
Copies must not read the source while its RTV is bound unless the native
contract guarantees it; unbinding and restoring output views must preserve
depth, surface lifetime and all other pipeline state without re-clearing.

Disjoint clone-backed sampled mips are now permitted. During clone refresh,
replay snapshots and unbinds the current single RTV and optional DSV, clears
PS SRVs, copies, restores the attachments without clearing, then rebinds active
sampling resources. Strict RTX FL9_1 tests passed both RGBA/BGRA depth cases,
each reading 32 green and 32 blue pixels after repeated refreshes; controlled
core cases matched the same depth-dependent results. A strict hidden-HWND test
sampled a clone, read BGRA [0,0,255,255], presented successfully and resized
with the completed command buffer retained. Visible presentation and the
synchronous window-callback reentrancy blocker remain unresolved. Independent
review found no concrete defect under the current one-RTV, PS-only restrictions.
The full strict private suite passed 255 tests with two filtered; three older
shader cases skip GPU execution internally. HAL Clippy passed with ten existing
test warnings, and core's ordinary zero-adapter test passed.
Checkpoint: `3e0fdcb9f9e8330e7cb900a021303d0e6d22c878`.

Next: a validated Never depth comparison can suppress draws only while shaders
have no observable storage side effects, stencil is inert and queries are
unavailable. FL9_1 guarantees native Always and LessEqual, not every comparison;
do not substitute LessEqual for strict comparisons.

Validated Never-depth draws are now suppressed under the current inert-stencil,
read-only-shader and no-query subset. Strict private and controlled core tests
passed depth-dependent output for both depth-write settings and direct/indexed
paths. The full strict suite passed 257 tests with two filtered; three older
shader tests skip GPU execution internally. HAL Clippy passed with ten existing
test warnings and core's zero-adapter isolation test passed. Independent review
found no concrete defect; future side effects or queries require re-audit.
Checkpoint: `8045637708e23b1d4de8c54f1ddde3d54bf0c6c8`.

Integer vertex input audit: although R8G8B8A8_UINT is a required FL9_1 IA
format, an FXC 10.1 `vs_4_0_level_9_1` probe of uint packing plus `asfloat`
accepted the shader but emitted embedded vs_2_0 float multiply/add instructions
instead of preserving bit reinterpretation. For bytes [63,128,0,0], the intended
bits 0x3f800000 represent 1.0f but the legacy listing writes the numeric
1065353216. The SM4 stream uses integer shifts/adds. This is bytecode evidence,
not historical-GPU execution. The format remains unsupported; audit currently
accepted uint uniforms and bitcasts for the same legacy-path hazard before
expanding integer shader or vertex support.

Follow-up shader audit found this risk is already reachable: FXC accepts an
FL9 pixel shader comparing a uint uniform to 16777217, but the embedded ps_2_0
listing converts the uniform to float and uses 16777216 as the comparison
constant. A float uniform converted to integer before that comparison has the
same issue. Modern RTX forced-FL9 readback cannot validate this legacy stream.
Prioritize a selected-entry fail-closed guard for unsafe runtime integer
operations and bitcasts before public exposure; do not treat earlier RTX
integer-uniform tests as proof of historical-device correctness.

The selected FL9 shader profile now rejects runtime integer values and
operations, conversions and bitcasts, integer-bearing used uniforms and
interfaces, switches, and unproven initializer/override forms in both checked
and unchecked modes. Literal/static indices and float-only shaders remain in
the supported subset. Two synthetic guard tests passed, including bounded
analysis and a defensive post-validation IR mutation. Three former integer
GPU tests now assert exact linkage rejection and each render 64/64 float-only
control pixels; six runtime-index rendering cases explicitly reject and skip
GPU execution. The full strict private suite passed 259 tests with two filtered.
The controlled core fixture and unsafe public-wgpu fixture still rendered
successfully, the latter producing 32 red/32 green pixels. HAL Clippy passed
with ten existing test warnings and ordinary core D3D11 enumeration stayed
empty. Independent review found no concrete bypass within the current subset;
the guard remains conservative and does not establish general integer support.
Checkpoint: `3de27b02b7659b028db3606e0e3047e07f9e75b8`.

Next: investigate nonindexed point-list pipelines with supported vertex streams.
Keep indexed points rejected at FL9_1, verify actual pixel placement and
first-vertex/stream bounds, and preserve point batching and instance ordering.

Nonindexed point pipelines now support the existing vertex and instance stream
subset. Vertex streams rebase for each native batch of at most 65,535 points;
indexed points remain rejected. The strict private test passed three full-image
RTX FL9_1 executions, including sparse slots, nonzero first vertex, a point to
triangle transition and 65,537 points across two instances with first_instance
1. The controlled core fixture matched all 64 pixels. The initial private
baseline failed at pipeline creation as expected; the first core attempt hit a
fixture-only inter-stage limit, corrected from 1 to 2 before the passing run.
The full strict suite passed 261 tests with two filtered; six dynamic-index
shaders are rejected and their GPU renders skipped. HAL Clippy passed with ten
existing test warnings and core's zero-adapter test passed. An independent
review found no concrete issue. Historical drivers are unverified.
Checkpoint: `4cba5bf6fb21ac23699fe1b012808d0852c6f0e9`.

Next: investigate indexed point-list emulation. Native FL9 indexed point draws
are forbidden; an ordered GPU-authoritative index readback and prevalidated
nonindexed runs may preserve point order, with a staging stall and potentially
many draws. Do not reuse triangle grouping or silently mishandle zero-stride
and negative effective indices.

Indexed point lists now use ordered, rebased nonindexed runs, with the GPU's
index contents staged and all stream ranges checked before any native draw.
The index-independent zero-stride and instance-only case avoids staging after
validating index-buffer and stream ranges. This relies on the existing ban on
index builtins, observable shader side effects and queries. A strict RTX FL9_1
test passed twelve full-image cases, including u16/u32 indices, signed bases,
repeated and reordered indices, GPU updates, instances and invalid late fetches.
The core fixture matched 64 pixels for each index format. Before implementation,
the new test failed on the expected unsupported draw. The first full suite run
found an old test expecting indexed points to fail; its negative case now checks
an out-of-range index binding. The strict suite then passed 266 tests, with two
filtered; six runtime-index shaders reject and skip their GPU renders. HAL
Clippy passed with ten pre-existing test warnings, and the core isolation test
passed. The unsafe public-wgpu fixture still rendered 32 red and 32 green pixels.
Historical drivers are unverified. The implementation can stall for GPU index
staging and use one native draw per scattered index.
Checkpoint: `8c2be917a31a0e124007b77989e2365a83fc376a`.

The private D24S8 triangle path now accepts signed constant depth bias when
slope and clamp are zero. Nonzero bias with selected `frag_depth`, nonzero or
nonfinite slope/clamp, and biased point pipelines remain rejected. Five strict
RTX FL9_1 full-image cases passed for positive, negative and zero bias, writes
and transitions. The core fixture matched all 64 pixels for each bias sign.
Pipeline tests rejected direct and structured fragment-depth outputs with bias;
zero-bias structured output compiled, while FXC rejected the direct-only output
with X4530 because the pixel shader did not write SV_Target0. A synthetic test
checked selected entry points and descriptor boundaries. The strict suite
passed 269 tests with two filtered; six dynamic-index GPU cases remain skipped.
Clippy passed with ten pre-existing test warnings, the core zero-adapter test
passed, and the targeted DX12 HAL library tests passed 2/2. Historical FL9 bias precision
is not established: Microsoft documents 32-bit bias arithmetic for FL10+.
No pristine failing GPU baseline was captured before implementation; the prior
code rejected all non-default depth bias, and the earlier full suite passed.
Checkpoint: `064d68dc2d98573b28730de5ba5e5f9c3aa6f83f`.

Next: allow and verify copies between different mip subresources of the same
texture. Core permits them but the private HAL currently rejects all self-copy.

Same-texture copies between different mip subresources now pass the existing
checked GPU copy path; identical canonical COM resources with the same native
subresource remain rejected. Before the change, the strict GPU test failed at
command encoding. Afterward, RGBA8 and BGRA8 passed partial copies in both
directions with complete padded readback, and each passed a render, same-texture
copy and clone-sampling sequence producing 64 blue pixels. The core fixture
verified every mip texel for both formats and rejected same-mip copies. The
private strict suite passed 271 tests, two filtered; six dynamic-index GPU cases
remain skipped. Clippy passed with ten pre-existing warnings, and the core
zero-adapter isolation test passed. An independent
review found no concrete ownership, identity or copy-contract defect. The
native source and destination are DEFAULT textures and use an explicit box;
historical FL9 drivers remain unverified.
Checkpoint: `ddb1b7cd9d559af88ee9d16c9d73a69b1101edc0`.

Next: investigate valid mipmapped COPY_DST-only texture creation, which core
permits but the current private usage matrix rejects.

Exact COPY_DST-only POT mip chains now use the existing copy-mip capability
gate without exposing source, sampled or attachment usage. The strict baseline
failed at texture creation before this change. Afterward, both RGBA8 and BGRA8
passed three uploaded levels and an untouched zero level on the forced-FL9_1
RTX. A test-only native DEFAULT-to-DEFAULT copy into a separate readable
texture verified all mip pixels and padding; this does not grant COPY_SRC to
the original. The controlled core fixture queued three writes, waited for
completion and checked view and readback usage rejection for both formats; it
does not claim core readback of the upload-only resource. Its first run exposed
a test assumption that core transfer validation happens at recording; the test
now asserts the exact missing-COPY_SRC error at command-buffer finish. The full
strict suite passed 273 tests with two filtered; six dynamic-index GPU cases
remain skipped. Clippy passed with ten existing test warnings, and core's
zero-adapter isolation test passed. An independent
review found no concrete usage leak or diagnostic copy defect. Historical FL9
drivers remain unverified.
Checkpoint: `735a9755645b54c06c9a1084ebf66ef49e4848e8`.

Next: investigate accepting a non-filtering sampler layout with a nearest
sampler, which core permits but the current private layout rejects.

NonFiltering sampler layouts now preserve their class in binding-plan identity
and accept only descriptors whose min, mag and mipmap filters are all nearest.
Filtering layouts continue accepting nearest or linear samplers. Sparse sampler
numbers retain the existing native register mapping; mixed sorted bindings and
reversed resource indices are checked. The strict baseline failed at layout
creation before the change. Two strict FL9_1 full-image executions passed,
including a filtering/non-filtering pipeline transition, and the controlled
core fixture matched 64 pixels and rejected a linear sampler with the expected
WrongSamplerFiltering error. Synthetic tests cover layout identity, descriptor
classification and mip-only linear filtering. The full private suite passed
277 tests, two filtered; six dynamic-index GPU cases remain skipped. Clippy
passed with ten existing warnings and core's zero-adapter test passed. The first
test compilation exposed a fixture lifetime error, which was fixed. Independent
review found no concrete mask, sorting or resource-matching defect.
Checkpoint: `7404886b4dd14da39c4859b5a33243c31f9ddd36`.

Next: support explicit unfilterable-float texture layouts only with selected
shader pairs that use NonFiltering sampler layouts. Preserve layout identity;
actual nearest sampler state alone cannot justify a Filtering layout.

Explicit Float{filterable:false} layouts now retain texture class in binding
identity. Selected Naga sampling pairs are checked per stage against logical
texture and sampler layouts, with unresolved or inconsistent slots failing
closed. A used unfilterable texture with a Filtering sampler layout is rejected
even if the actual sampler is nearest; unused incompatible pairs remain legal.
The strict baseline failed at layout creation. Two forced-FL9_1 full images
passed, including helper-function sampling and pipeline transitions; the bad
pair returned the exact fragment linkage error. The core fixture matched 64
pixels and rejected the corresponding invalid pair. CPU tests cover selected
entries, helpers, cross-group identities, layout masks and malformed mappings.
The initial CPU fixture used a reserved WGSL identifier and was fixed. The full
private suite passed 281 tests with two filtered; six dynamic-index GPU cases
remain skipped. Clippy passed with ten existing test warnings, and core's
zero-adapter isolation test passed. Independent
review found no concrete pair-validation bypass. No new texture formats or
public capability exposure were added.
Checkpoint: `4ef1fbdaafd138b5981b5765c6a33a887b54b594`.

Next: examine accepting combined vertex/fragment visibility for sampled
resources used only by the fragment shader, without enabling vertex sampling.

Sampled texture and sampler entries now accept VERTEX|FRAGMENT visibility as
well as FRAGMENT. Sorted per-binding visibility masks remain part of layout
identity; stage maps retain the original compact slots. Actual vertex-stage
sampled-resource use is still rejected. The pre-change strict baseline failed
at layout creation. Two strict FL9_1 full-image renders passed, including a
visibility/layout transition; the selected vertex sampler returned the expected
vertex linkage error. The core fixture matched all 64 pixels. Compiler tests
also reject selected vertex sampling, loads and dimension queries while
accepting unused declarations; their shared guard is mode-independent, but
the new test does not independently exercise both ShaderModule check modes.
The full private suite passed 284 tests, two filtered; six dynamic-index GPU
cases remain skipped. Clippy passed with ten existing test warnings and core's
zero-adapter isolation test passed. Independent review found no concrete
visibility, slot or shader-guard defect. No vertex sampling is advertised.
Checkpoint: `4f3f608c229fd6e5b7bc309db5ac975bfd790c7e`.

Exact premultiplied-alpha blending now uses coupled ONE/INV_SRC_ALPHA add
factors for color and alpha on supported linear RGBA8 and BGRA8 targets. The
strict baseline rejected the pipeline before implementation. Six strict FL9_1
RTX full-image cases passed, including opaque transitions; a controlled core
render matched 64 pixels with one-byte rounding tolerance. Its first attempt
found the unsafe fixture did not expose queried blendability; the fixture now
reports it only on a positive format query and explicitly remains restricted.
An observed core pixel was [128, 0, 63, 191]. A later full-suite run found
three stale tests expecting this blend to be rejected; they now check an
unsupported neighboring blend state. The full private suite passed 286 tests,
two filtered; six dynamic-index GPU cases remain skipped. The focused test
passed again after adding its missing format prerequisite. The fixture's
synthetic query test, core zero-adapter test, and Clippy passed; Clippy retained
ten existing test warnings. The initial test launch hit Windows paging-file
exhaustion before PowerShell started; retrying with two Cargo jobs succeeded.
Independent review found no concrete factor or capability isolation defect.
Full-channel straight-alpha and general separate-alpha remain unsupported;
historical FL9 execution and sRGB blending are unverified.
Checkpoint: `cac502305de4c2569bfc29da73eecdd9a59e92e8`.

Core shader-passthrough backend selection now returns NotCompiledForBackend for
Dx11 instead of reaching an unreachable panic. The feature gate still rejects
passthrough on the real controlled device; the fixture verified MissingFeatures
for empty and HLSL descriptors without fabricating unsupported device features.
A pure selection test verified the Dx11 and Noop errors and existing Vulkan,
DX12, Metal and GL source selection and precedence. The controlled strict
FL9_1 core fixture and targeted selection test passed, as did targeted core
Clippy after fixing a new test-only warning. The same selection test passed
with the DX12 core feature; that build emitted 15 warnings, primarily unused
items in the existing timestamp-normalization module. Ordinary adapter exposure remains
disabled. The public map-error path remains a substantive blocker: fallible
D3D11 staging/map work is behind a void HAL invalidate interface, so failures
currently panic. A correct remedy needs error propagation through HAL and core;
silently swallowing errors or marking a shadow coherent would be unsound.
Checkpoint: `ceaf4d603dc4ecbc6449b520f5a9ef546b5cfa64`.

HAL mapped-range invalidation now returns DeviceError across backends. Core
unmaps a failed read mapping before returning the error and does not expose the
pointer; BLAS compaction also unmaps and does not read its size on failure.
Vulkan now propagates its native invalidation error instead of unwrapping it.
A feature-gated one-shot fault on a real D3D11 buffer verified an OOM callback
and released shadow mapping in the controlled FL9_1 core fixture. The fixture
passed after a test-only label type correction. Strict private D3D11 tests
passed 286 tests with two filtered (six dynamic-index GPU cases still skip),
and the persistent shadow mapping test passed again after a reviewer removed a
test violating the unsafe HAL range precondition. Windows HAL and core checks
with dx11, dx12, vulkan and gles features passed; the core check emitted 15
warnings in the no-WGSL configuration. DX12 HAL tests passed 2/2, core adapter
isolation passed and targeted HAL/core Clippy passed with ten existing HAL test
warnings. Metal was not compiled on this Windows host. BLAS invalidation failure
was reviewed but not exercised on ray-tracing hardware. Flush and unmap failure
handling remain separate work, as does existing device-lost callback reentrancy
while core mapping locks may be held. Public adapter exposure remains disabled.
Checkpoint: `a690866b4a6cdc0981a1ffa448f2c28f0f41b756`.

Mapped-range flush now returns a HAL `DeviceError` across all backends. D3D11
shadow synchronization and Vulkan's native flush errors propagate; coherent or
implicitly coherent backends return success. Core mapped-at-creation writes,
direct writes, unmap staging, queue uploads, texture uploads, indirect draw
validation and acceleration-structure staging handle errors without enqueueing
failed staging data. Failed staging allocations are unmapped and destroyed;
failed direct mappings are unmapped and core map state returns to Idle. Device
loss callbacks are deferred until snatch/encoder locks are released, and the
strict D3D11 fixture verified callback reentrancy, resource destruction and
nested polling after an injected flush OOM. Queue write paths now release locks
in reverse acquisition order.

On the RTX 3090 Ti (vendor 4318, device 8707), strict mode requested and
asserted FL9_1. Both focused core hardware fixtures passed: the broader
render/transfer fixture also exercised injected invalidation failure and a
second pending map after device loss; the new mapped-write fixture verified
OOM propagation and shadow cleanup. The full core library suite passed 70/70,
including zero public D3D11 adapter exposure. The private D3D11 HAL suite
passed 286 tests with two filtered (six existing dynamic-index hardware cases
remain skipped). Cross-backend core checks for dx11/dx12/vulkan/gles and
dx11/replay passed; targeted DX12 HAL tests passed 2/2. Clippy passed with ten
existing HAL test warnings. Targeted rustfmt checks passed on all 20 changed
Rust files and `git diff --check` passed. Cross-backend checking without WGSL
emitted 15 existing unused/unreachable warnings.

`RUSTFLAGS=--cfg wgpu_validate_locks` remains unverified: after fixing the
queue-write LIFO release exposed by the first run, the full fixture reached an
existing `DEVICE_COMMAND_INDICES` to `COMMAND_BUFFER_DATA` lock-rank TODO. The
narrow flush fixture also encountered the ranked-lock checker while dropping
the fixture's `buffer_as_hal` guard. Metal was not compiled on this Windows
host. Public adapter exposure remains disabled.
Checkpoint: `13a4defb9` (`core: propagate mapped buffer flush failures`).

The next planned investigation was DXGI/window-procedure reentrancy during
`Present`; the outcome is recorded below. Keep ordinary D3D11 enumeration
disabled until this and the remaining core/HAL capability mismatches are
resolved.

The presentation audit confirmed the existing open gate. In
`wgpu-core/src/present.rs`, `Queue::present` holds `device.command_indices`
while calling HAL; `CoreWindowFrame::present_for_hal` holds
`NativeDevice::immediate` while calling DXGI. Microsoft documents that DXGI
may synchronously `SendMessage` to the window procedure and wait for the
message pump, and that immediate-context calls must be serialized with DXGI
operations on the same thread. Therefore dropping the HAL mutex alone permits
invalid concurrent context/DXGI use, while retaining it can block a callback
that reenters the device. The core queue-index lock creates another reentrancy
edge. `ID3D11Multithread` protects against calls from other threads but does
not remove the core lock ordering or make a cross-thread synchronous message
safe. `DXGI_PRESENT_DO_NOT_WAIT` is not established to avoid synchronous
window-message waits, so it is not treated as a fix. References:
[DXGI best practices](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices),
[D3D11 multithreading](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-intro),
[ID3D11Multithread::Enter](https://learn.microsoft.com/en-us/windows/win32/api/d3d11_4/nf-d3d11_4-id3d11multithread-enter),
[Present](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiswapchain-present).
No safe backend-only change was identified; keep public surface exposure gated.

## One-sided FL9_1 stencil operations

The private D24S8 pipeline now maps stencil comparisons, operations, eight-bit
read/write masks and dynamic references to D3D11 state. Reference changes are
recorded, replayed against the selected depth-stencil state and reset at render
pass boundaries. Microsoft documents that FL9_1 does not support two-sided
stencil, so active pipeline states must use identical front and back faces;
asymmetric states reject at pipeline creation. The internal model records the
FL9_1 two-sided restriction and private stencil implementation without setting
the broad public depth-stencil capability.

Depth `Never` with active stencil is emulated using the documented FL9_1
Always-depth guarantee: the stencil pass operation becomes the requested
depth-fail operation, while color and depth writes are disabled. A strict GPU
test proved the write indirectly with a following Equal stencil test. The other
case still validates and suppresses draws when stencil is inert. The HAL cannot
read stencil storage directly. State translation maps all comparison and
operation enums, but physical evidence on the RTX covers Replace/Equal only;
historical hardware and all other active state combinations remain unverified.

Validation on the NVIDIA RTX 3090 Ti (vendor 0x10de/4318, device
0x2203/8707, driver 32.0.16.2101), with software adapters prohibited, requested
and selected FL9_1:

- Focused `stencil` filter: 3 passed (2 CPU-only mapping/classifier tests and
  one real-device test). Readback verified left 32 blue/right 32 black pixels
  after Replace/Equal/reference transitions, and 64 blue pixels after the
  Never-depth-fail stencil write.
- Complete strict D3D11 HAL module suite: 287 passed, 0 failed, 0 ignored,
  2 filtered. The focused filter passed again after the final `Copy` cleanup in
  the test.
- Core `requesting_dx11_does_not_expose_a_stub_adapter`: 1 passed.
- Targeted HAL Clippy passed with ten existing warnings; changed Rust files pass
  `rustfmt --edition 2021 --check`, and `git diff --check` passes.
- A targeted DX12 HAL test build and a one-job DX12 `cargo check` both stopped
  when rustc could not map standard-library metadata because Windows returned
  OS error 1455 (paging file too small). This is an environment failure; DX12
  regression is not verified by this increment. No public D3D11 adapter or
  historical FL9_1 device was enabled or tested.

Next: test this one-sided stencil subset through the unsafe, non-enumerated core
fixture, then continue the outstanding core/HAL contract audit. This does not
unblock ordinary public adapter exposure or the unresolved HWND presentation
reentrancy gate.
Checkpoint: `ae40c514a` (`dx11: support one-sided FL9_1 stencil operations`).

The same restricted stencil behavior now runs through `wgpu-core`'s unsafe,
feature-gated D3D11 fixture. A new case writes reference `0xa5` with a symmetric
Always/Replace pipeline whose color writes are masked, then switches to a
symmetric Equal pipeline and renders the image. The expected 64-pixel output
only appears if the core-recorded reference, stencil attachment, selected
pipeline state and HAL replay all agree. The fixture still asserts ordinary
HAL enumeration is empty; no normal adapter path was enabled.

On the physical NVIDIA RTX 3090 Ti (vendor 0x10de, device 0x2203), the fixture
requested and selected FL9_1 and the new ActiveStencil case matched all 64
pixels. The complete fixture-enabled core library suite passed 70/70 tests,
including its zero-adapter isolation test and both controlled hardware tests.
The focused fixture also passed independently with `WGPU_DX11_REQUIRE_HARDWARE=1`.
This verifies core-to-HAL stencil state on this modern GPU restricted to FL9_1;
historical FL9_1 behavior remains unverified. Rust formatting and
`git diff --check` passed.

Next: audit core-valid read-only depth/stencil attachment use and the remaining
private HAL render-pass contract. Keep public adapter and surface exposure
disabled while presentation reentrancy and truthful full capability reporting
remain unresolved.
Checkpoint: `cff2cc29b` (`core: test FL9_1 stencil through controlled fixture`).

The direct HAL read-only D24S8 contract now tracks effective stencil writes as
well as depth writes. Pipeline state records whether the final native stencil
operations can mutate bits after no-op canonicalization and the FL9_1 Never
remapping. Selecting such a pipeline in a read-only attachment pass rejects
before native replay. This closes a gap previously covered only by core's
`WRITES_STENCIL` validation. The read-only guard applies to the whole D24S8
attachment because the current private descriptor accepts READ usage only with
LOAD|STORE on both aspects; aspect-specific read-only usage remains unsupported.

Strict hardware validation on the RTX 3090 Ti at requested/selected FL9_1
passed the focused read-only test, preserving the 32 green/32 blue depth result
and rejecting both depth-writing and stencil-writing pipelines. The full
private D3D11 suite passed 287/287 tests (two filtered). The fixture-enabled
core suite passed 70/70, including the active-stencil core render and ordinary
zero-adapter regression. Targeted HAL Clippy passed with ten existing warnings;
rustfmt and `git diff --check` passed. This modern forced-FL9_1 device does not
verify historical drivers. DX12 was not rerun for this increment; its previous
targeted check was blocked by Windows paging-file error 1455.

Next: audit render-pass attachment usages and query/resolve paths against what
core can record, prioritizing a specific case that can be rejected or safely
implemented without broad capability claims. HWND public presentation and
ordinary D3D11 adapter exposure remain blocked by reentrancy and capability
gaps.
Checkpoint: `5ea3045f9` (`dx11: reject stencil writes on read-only attachments`).

The read-only regression now also covers a pipeline whose depth comparison is
Never and whose requested stencil depth-fail operation is Replace. The backend
remaps that operation to native stencil pass; the read-only guard rejects it
based on the effective descriptor. The strict forced-FL9_1 GPU test passed
again, while its preserved-depth image remained 32 green/32 blue. Clippy passed
with the same ten existing warnings; `rustfmt --edition 2021 --check` and
`git diff --check` passed. No production behavior changed in this follow-up.
Checkpoint: `f2e746d83` (`dx11: test remapped stencil writes on read-only depth`).

The render-pass audit found the unsupported query, resolve, multisample and
multiview paths are blocked at both relevant boundaries. The D3D11 HAL rejects
query commands, resolve targets, multisampled passes, multiview, multiple color
targets and pass query descriptors. The controlled core fixture exposes no
occlusion downlevel flag, no timestamp feature, no multisample format flags and
one array layer; it already asserts that query creation fails before HAL use.
A new 4x RGBA8 texture request asserts core returns
`InvalidMultisampledFormat`, which also prevents a legal resolve source from
being formed. No capability was enabled to make these tests pass.

With `WGPU_DX11_REQUIRE_HARDWARE=1`, the fixture-enabled core library suite
passed 70/70 tests on the physical RTX 3090 Ti forced to FL9_1; this includes
real offscreen GPU work, active stencil through core, query validation,
zero-adapter isolation and the new CPU-side multisample rejection. Core
Clippy passed without warnings. Ordinary HAL enumeration remained empty. The
4x case does not itself execute GPU work, and the HAL's negative resolve and
multiview branches are structurally guarded but not independently invoked from
core because the adapter contract does not expose their prerequisites.

Next: exercise the already-supported depth-only pass through the controlled
core fixture, then continue the broader render-pass and device-loss audit.
Public adapter/surface exposure remains gated by truthful full validation and
DXGI reentrancy.
Checkpoint: `8d8877e7a` (`core: reject unsupported FL9_1 multisample textures`).

The controlled core fixture now also performs a depth-only prepass with no
color attachments, then begins a color pass that LOADs the depth it wrote and
draws farther geometry. Strict RTX 3090 Ti execution at requested/actual
FL9_1 read back 32 blue pixels where the prepass depth rejected the farther
draw and 32 green pixels where depth remained clear. This proves the core
attachment transitions and HAL replay preserve depth across the two passes;
it is not general depth/stencil API support. An initial probe used depth Equal
and was rejected at pipeline creation, as intended by the restricted
comparison contract; the final test uses supported LessEqual.

The focused core fixture passed in strict hardware mode; the complete
fixture-enabled core suite passed 70/70, including ordinary zero-adapter
isolation. Core Clippy passed without warnings. Rust formatting and
`git diff --check` passed. Checkpoint: `a13699edd` (`core: test FL9_1 depth-only
render prepass`).

Next: continue auditing device-loss and command submission error paths around
render passes. Public adapter exposure remains gated; this core fixture is
unsafe, test-only and still confirms ordinary enumeration is empty.

The command/device-loss audit found that pipeline creation classified
`DXGI_ERROR_DRIVER_INTERNAL_ERROR` as `DeviceError::Lost`, but device, queue and
surface paths did not. Microsoft documents this HRESULT as a driver failure
that puts the device into the removed state. A shared HRESULT classifier now
handles it consistently with device hung/removed/reset and preserves
out-of-memory and unexpected-error distinctions. Optional viewport shader
creation still treats `E_INVALIDARG` as a nonfatal unsupported variant, and
surface errors still map ordinary failures to `SurfaceError::Other`.

CPU regressions passed for the shared classifier, queue mapping, surface
mapping and optional viewport shader error handling. The strict D3D11 suite
passed 289/289 tests with two filtered on the RTX 3090 Ti at requested/actual
FL9_1; several expected unsupported-shader cases report a deliberate rendering
skip after asserting rejection. The fixture-enabled core suite passed 70/70
with strict hardware required, and its focused public-wgpu/core fixture also
passed on the same hardware, including texture transfers and all depth/stencil
render cases. `cargo build --offline -p wgpu-hal --no-default-features
--features dx11` passed. HAL Clippy passed with the same ten pre-existing
warnings. Formatting and `git diff --check` passed. These tests validate
classification logic, not recovery from an induced physical device removal.

Checkpoint: `da035664154d080ecb80a3518cbfa6e4eecbe4d8` (`dx11: classify
driver-internal failures as device loss`). Device teardown, loss during live
presentation and debug-layer diagnostics remain unaudited; public adapter and
surface exposure remain gated.

The prior vertex constant probe referenced only one statically selected array
element and did not measure the profile range. It now compiles a dynamically
indexed `float4` constant array: FXC accepts length 255 and rejects lengths 256
and 257 with X4507 at constant register index 256. FXC accepts 32 referenced
pixel constants and rejects 33 with X5200 at register 32. With strict hardware
required, the RTX 3090 Ti opened at requested/actual FL9_1 and accepted native
`CreateVertexShader` for the 255-element bytecode. This test creates but does
not execute that shader; the private WGSL path rejects runtime integer indexing,
so near-limit GPU execution remains an explicit gap. The first two attempts at
static all-element reductions exceeded the vs_2_0 instruction budget and were
replaced with a dynamic-index compiler probe rather than weakening the profile.

The strict `constant` HAL filter passed 8 tests (284 filtered), including the
profile compiler boundary, native shader creation, constant depth-bias and
sample-bias renders, and uniform materialization. `fl91_vertex_constant_profile_boundary_creates_on_hardware`
reported the physical NVIDIA RTX 3090 Ti (vendor 0x10de, device 0x2203) and
requested/actual FL9_1. The complete strict private D3D11 HAL suite then passed
290 tests with two filtered in 73.84 seconds; it included the new shader-object
creation test. The focused strict shader-object test was rerun after adding its
explicit PASS line and passed. `cargo build -p wgpu-hal` passed; HAL Clippy
passed with the same ten existing warnings. Core's
`requesting_dx11_does_not_expose_a_stub_adapter` passed 1/1. Rust formatting
and `git diff --check` passed. The public adapter and HWND surface gates are
unchanged.
Checkpoint: `f775292a2` (`dx11: verify FL9_1 vertex constant limits`).

## 2026-11-03 — non-indexed FL9_1 line topologies

Starting from clean HEAD `f20a0a15f` (`docs: record FL9_1 constant boundary
checkpoint`), the private pipeline now accepts non-indexed line lists and line
strips. Replay batches line lists on even vertex counts and line strips with a
one-vertex overlap, rebasing bound streams for each batch. Indexed line draws
remain rejected. The internal capability model records the two implemented
topologies. Synthetic batch tests cover incomplete tails, instances, invalid
limits and FL9_1 boundaries.

Strict GPU validation passed on the physical NVIDIA RTX 3090 Ti
(`0x10de:0x2203`) requested and selected at FL9_1. A deterministic offscreen
render read back 22 red and 12 green pixels for line-list/line-strip draws; the
list includes a discarded trailing vertex. An indexed line-list command was
rejected before submission. CPU tests verified pair boundaries, one-vertex
strip overlap, instance ordering and maximum FL9_1 batch sizes.

The full strict private D3D11 suite passed 294/294 tests with two filtered.
Its first run exposed an obsolete test expectation that LineList pipeline
creation should reject; the test now verifies successful creation, and the
focused depth regression plus full suite pass. The `line_` filter passed 12/12,
including the real-device render. `cargo build --offline -p wgpu-hal --no-default-features
--features dx11` passed; core's `requesting_dx11_does_not_expose_a_stub_adapter`
passed 1/1. HAL Clippy passed with ten pre-existing warnings. Rustfmt checks
passed for every changed Rust file, and `git diff --check` passed. This feature
The feature is checkpointed as `3db26eed3` (`dx11: add FL9_1 nonindexed line
topologies`). Next: exercise these topologies through the controlled wgpu-core
fixture, then assess indexed line-list support. Public adapter and surface
exposure remain disabled.

The unsafe, feature-gated core fixture now creates LineList and LineStrip
pipelines through core, records `render_pass_draw`, submits on the injected
physical device and validates exact red sampled pixels against the blue clear.
Strict hardware readback produced 8 red pixels for LineList and 12 for
LineStrip, with all other target pixels unchanged. The fixture still asserts
ordinary HAL enumeration is empty. The complete strict fixture-enabled core
library suite passed 70/70 tests, including ordinary adapter isolation; core
Clippy and formatting checks passed. An independent review found no issue in
the fixture isolation or result assertions. Checkpoint `e79428c50` records this
core-to-HAL coverage. Next, implement and test indexed LineList semantics
without assuming indexed strip restart behavior; public D3D11 exposure remains
disabled.

The core-fixture change was independently reviewed and committed as
`e79428c50` (`dx11: verify FL9_1 line topologies through core fixture`). The
strict focused command and the complete fixture-enabled core library suite both
ran with `WGPU_DX11_REQUIRE_HARDWARE=1`; results were respectively 1/1 and
70/70. Core Clippy passed with no warnings. The fixture continues to check that
ordinary D3D11 adapter enumeration returns no adapters.

## 2026-11-03 — indexed FL9_1 LineList

The private HAL now records indexed LineList draws and replays GPU-authoritative
index data through a shared pair-preserving planner. Uint16 and Uint32 indices
are compacted into ordered Uint16 windows when their effective span is at most
65,534; a wider line pair is gathered into two-vertex scratch streams. Every
effective index is checked against every vertex-rate stream before any native
draw. An invalid fetch discards the whole draw, incomplete final pairs are
ignored, and native calls are capped at 131,070 indices / 65,535 lines. Indexed
LineStrip remains rejected.

Strict hardware execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`), explicitly requested and selected at FL9_1. The indexed
Uint16 case used a nonzero first index and incomplete tail; the Uint32 case
covered a compact window and a wide-pair gather. Readback contained 22 red,
12 green, 24 blue and 14 white pixels. A second submission with an
out-of-bounds fetch left every pixel at the clear value; indexed LineStrip was
rejected before submission. The focused strict test passed 1/1.

The full strict private suite passed 297 tests, with 0 failures and 2 filtered.
The strict fixture-enabled core suite passed 70/70 and retained the assertion
that ordinary D3D11 adapter enumeration is empty. Its focused hardware fixture
reported 8 red pixels for each indexed format and passed 1/1. HAL build passed;
HAL Clippy passed with ten existing warnings and core Clippy passed without
warnings. Rustfmt checks and `git diff --check` passed. The first attempted core
run lacked the Visual Studio linker environment and did not compile; rerunning
through `VsDevCmd.bat` passed. This code path maps the index staging buffer and
may stall; wide pairs also allocate small scratch buffers. Historical FL9_1
hardware remains untested.

Checkpoint `82c9b9228` (`dx11: add FL9_1 indexed line-list rendering`). Next,
analyze fixed-index restart semantics and implement indexed LineStrip only if
it can preserve core draw behavior under FL9_1. Keep ordinary adapter
enumeration and HWND exposure disabled.

## 2026-11-03 — indexed FL9_1 LineStrip emulation

The private HAL now accepts indexed LineStrip pipelines when their declared
`strip_index_format` matches the bound Uint16 or Uint32 index buffer. At replay,
it maps the GPU-authoritative selected index range, splits format-specific
restart markers into independent runs, discards runs shorter than two indices,
validates every effective vertex fetch across all vertex-rate streams, and
compacts each bounded run to Uint16. Long runs overlap one endpoint at the
FL9_1 native draw limit. An adjacent pair whose effective indices cannot fit a
single 65,534-span window is gathered into per-stream scratch buffers and
rendered as a two-vertex line strip. Native FL9 strip-cut support is not used.
The internal capability model records this implementation separately from the
hardware guarantee.

The implementation resolves `baseVertex + index` before testing for the
format-specific restart value and splitting primitive runs, following the
current [WebGPU index-resolution and primitive-assembly algorithm](https://www.w3.org/TR/webgpu/#index-resolution).
FL9_1 does not guarantee native strip-cut support, so this split occurs during
the replay-time CPU index scan before compact R16 runs are submitted. CPU tests
cover a non-marker that becomes a marker through a positive base and a raw
marker that becomes an ordinary, in-range vertex through a negative base. The
core fixture executes both index formats with base 1 and a marker formed by
resolution.

Strict private hardware testing used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`) with requested and selected FL9_1. The combined line test
read back 17 red, 12 green, 24 blue, 14 white, 10 cyan and 14 yellow pixels,
including marker-separated Uint16 and Uint32 strips. The Uint16 strip used a
positive base to create the resolved marker. A second submission verified a
Uint32 wide-edge gather followed by a compact window (15 yellow pixels) and
asserted a pixel on the gathered edge itself. Missing strip format and
pipeline/index-format mismatch were rejected before submission. An
out-of-bounds indexed fetch discarded the whole draw. The controlled core
fixture also passed indexed Uint16 and Uint32 LineStrip through core command
recording, with 12 red pixels each. Planner tests cover resolved-marker runs,
empty/short runs, signed-base rebasing, wide-edge gathering, Uint16 max as an
ordinary Uint32 index and the 65,536 index native strip boundary.

The first planner-test attempt exposed an undersized synthetic vertex range;
the corrected fixture now models the full effective index range. The first
strict GPU attempt had a test-only Uint32 fixture referencing vertex indices
that had not been appended; the backend correctly discarded it as an
out-of-bounds draw. The first core attempt then exposed an index-upload size of
14 bytes, which violates the core queue-write four-byte alignment; an unused
padding index now keeps the fixture upload valid. A separate review caught
that the first GPU wide-pair test used identical gathered endpoints. The test
now uses distinct positions and asserts an interior pixel on that specific
edge. After these fixture corrections, the focused strict HAL and core tests
passed; no product validation was weakened.

The focused strict HAL hardware test passed 1/1, and the focused strict core
fixture test passed 1/1. Final verification on the current implementation then
passed the complete strict private HAL suite (301 passed, 0 failed, 2 filtered)
and the strict fixture-enabled core suite (70 passed, 0 failed), which includes
the ordinary zero-D3D11-adapter isolation test. HAL compilation passed. HAL
Clippy completed with ten existing warnings and no errors; core Clippy passed
without warnings. The targeted DX12 pipeline descriptor regression passed
2/2. Rust formatting and `git diff --check` passed. The strict RTX tests report
the actual NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), request FL9_1 and
assert selected FL9_1. Historical FL9_1 devices remain untested.

Implementation checkpoint: `2ff5a58e8` (`dx11: emulate FL9_1 indexed
line-strip restart`). The checkpoint records the resolved-index restart rule
and checked wide-edge GPU output. The persistent handoff/journal update follows
in a documentation-only commit. Indexed triangle strips, public adapter
exposure and HWND surface exposure remain unavailable.

## 2026-11-03 — indexed FL9_1 TriangleStrip emulation

The private HAL now accepts indexed TriangleStrip pipelines for Uint16 and
Uint32 indices when the pipeline's declared `strip_index_format` matches the
bound index buffer. At replay, GPU-authoritative index bytes are copied to the
existing staging buffer and scanned after earlier commands. The scan resolves
`baseVertex + index` before recognizing the format-specific restart marker,
prevalidates every non-marker fetch against every vertex-rate stream, and
discards the whole draw if any fetch is invalid. Each run is expanded to
triangle-list indices with alternating winding; restart resets the run's
winding parity. Expanded indices are split at the FL9_1 maximum of 65,535
triangles per native draw and passed through the existing compact Uint16
window/gather path. Wide triangles gather the selected bytes from every
vertex-rate stream. Native FL9 strip-cut support is not assumed.

Strict private hardware testing ran on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`) with explicitly requested and selected FL9_1. Indexed Uint16
and Uint32 strips with restart were compared byte-for-byte against equivalent
triangle-list images for both front-face settings and both cull modes. A
separate wide Uint32 triangle requiring gathering read back 32 red pixels.
Mismatched and missing strip-index formats and invalid vertex fetches are
rejected before native draw execution. The controlled core fixture executed
Uint16 and Uint32 indexed TriangleStrip with a restart marker formed by the
signed base vertex; each produced 12 red pixels, and ordinary D3D11 enumeration
remained empty.

Deterministic planner tests passed 3/3 for winding/restart reset, base-resolved
restart values and the primitive-limit/whole-draw-discard boundary. The
complete strict private D3D11 suite passed 304/304 tests with two filtered;
the fixture-enabled core library suite passed 70/70, including the zero-adapter
isolation test. D3D11 HAL build and HAL/core Clippy passed; HAL Clippy reports
ten pre-existing warnings. Targeted DX12 pipeline descriptor tests passed
2/2. The changed Rust files pass targeted rustfmt. Workspace/package format
checks still report pre-existing formatting in the unrelated Naga HLSL writer
and core instance isolation test; those files were left unchanged. The first
test invocation lacked `link.exe`; rerunning through Visual Studio's
`VsDevCmd.bat` succeeded. Staging the index stream can stall the CPU, and
historical FL9_1 hardware remains untested.

Commands (Rust 1.93.1, MSVC environment; hardware commands used
`$env:WGPU_DX11_REQUIRE_HARDWARE = "1"`):

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 indexed_triangle_strip -- --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_nonindexed_triangle_strip_winding_and_first_vertex -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo clippy --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --all -- --check
cargo fmt --package wgpu-hal --package wgpu-core -- --check
rustfmt --check --edition 2021 wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_index.rs wgpu-hal/tests/dx11/triangle_strip.rs
git diff --check
```

The workspace format check reports the pre-existing `naga/src/back/hlsl/writer.rs`
format differences; the package check reports a pre-existing expression layout
in `wgpu-core/src/instance.rs`. Neither file is part of this increment. The
targeted rustfmt check for all changed Rust files and `git diff --check` pass.

Implementation checkpoint: `f1b8c09a7` (`dx11: emulate FL9_1 indexed triangle
strips`). Indexed TriangleStrip is now implemented; continue with the remaining
topology, resource and public-contract gates. Keep ordinary D3D11 adapter
enumeration and public HWND surface exposure disabled.

## Checkpoint — execute the final statically addressed vertex constant

The sparse-uniform suite now includes an end-to-end shader that statically reads
`constants[254]` from a 255-element vertex uniform array. It draws the same
triangle twice, first with a zero-filled uniform and then with a value only in
the final float4. On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), with FL9_1 explicitly requested and selected, readback
reported 17 green baseline pixels and 4 red shifted pixels. This verifies that
the final tested vertex constant affects GPU vertex processing. It does not
verify runtime indexing; runtime integer indexing remains deliberately rejected.
The test destroys its groups and buffers before queue submission and then
submits, waits on the native fence and reads back, exercising retained command
resource lifetime.

The focused strict test passed 1/1 with 306 tests filtered. The complete strict
private D3D11 library suite passed 305 tests, 0 failed, with 2 filtered. The
D3D11 HAL build and Clippy passed; Clippy reports ten existing warnings in
unrelated HAL code. The core zero-adapter isolation test passed 1/1 when run
with the required `std` feature. A first invocation without `std` failed to
compile the test target, then the corrected invocation passed. Targeted DX12
pipeline descriptor tests passed 2/2. The HAL package formatting check,
targeted rustfmt check and `git diff --check` passed.

Commands (Rust 1.93.1; Cargo invoked from the Visual Studio 2022 MSVC developer
environment; the hardware commands used strict mode):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_last_vertex_constant_changes_rendered_geometry -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/tests/dx11/uniform_bindings.rs
git diff --check
```

The feature is checkpointed as `6cac808c9` (`dx11: verify FL9_1 final vertex
constant execution`). Continue the uniform-layout audit and then work the next
safe render/resource gates in `FL9_1_PLAN.md`. The larger mission remains
PARTIALLY COMPLETE: this evidence is private-HAL execution on one modern GPU
forced to FL9_1, not historical-hardware or public-adapter validation.

## Follow-up — execute the pixel constant c31 boundary

The c254 geometry test now also uses a separate fragment-visible uniform array
and statically reads its final element, pixel c31. The two draws share one
pipeline and switch bind groups: vertex c254 shifts the second triangle, while
pixel c31 changes the fragment result from green to red. This verifies separate
vertex/pixel uniform bindings and their stage-specific execution through the
existing logical resource map. Readback again found 17 green baseline pixels
and 4 red shifted pixels.

Strict mode selected the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) at requested and actual FL9_1. The focused test passed
1/1 (306 filtered). The complete strict private D3D11 suite passed 305/305,
with two filtered. HAL Clippy passed with the same ten pre-existing warnings;
the changed Rust file and HAL package pass formatting, and `git diff --check`
passes. The targeted core isolation and DX12 checks from the preceding c254
checkpoint were not rerun for this private-HAL test-only extension.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_c254_and_pixel_c31_change_rendered_output -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/tests/dx11/uniform_bindings.rs
git diff --check
```

Checkpoint `0c0ff00ba` (`dx11: execute FL9_1 pixel constant boundary`). Static
near-limit vertex and pixel accesses now both have real forced-FL9_1 rendering
evidence. Runtime integer indexing remains rejected, and matrices/nested layouts
remain limited to the existing focused tests. Continue the private contract
audit and keep public adapter and HWND surface exposure disabled.

## Checkpoint — queried R8_UNORM sampled uploads

`268431ec3` adds R8_UNORM as a narrow one-mip, one-layer sampled/upload format.
The capability model separates the FL9_1 required-format guarantee, the
selected device's `TEXTURE2D | SHADER_SAMPLE` query, and the implemented
upload/sampling subset. Transfer layout now uses the format's texel width, so
the strict GPU test uploads a 5x4 image from byte offset 1 with 7-byte rows,
samples it through the private binding and shader paths, and reads a 10x8
RGBA8 target. The output matched 80/80 expected pixels. The physical adapter
was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), with requested and actual
FL9_1; its R8 support mask was `0x0002d3e0`. The driver version recorded by the
existing adapter probe is `32.0.16.2101`. The test exercised nearest sampling;
linear R8 filtering is query-gated but not visually tested. R8 render targets,
copy sources, readback, and mip chains remain unsupported.

Verification on Rust 1.93.1/MSVC:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_unorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_bind_group.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

The focused strict test passed 2/2 (one CPU capability test and one physical
GPU execution). The complete strict private suite passed 307 tests, with zero
failures and two filtered. HAL build, HAL Clippy, core's zero-adapter isolation
test (1/1), targeted DX12 tests (2/2), package formatting, targeted rustfmt,
and diff checks passed. HAL Clippy reported ten existing warnings in unrelated
code. Public D3D11 adapter enumeration remains disabled. Next, investigate and
implement the next FL9_1-required sampled format with the same query, usage,
layout and strict-readback discipline.

## Checkpoint — queried R8G8_SNORM sampled uploads

`881640796` extends the single-mip sampled/upload path to R8G8_SNORM (legacy
V8U8), a format listed as required at FL9_1 for 2D texture and shader sampling
by Microsoft's required-format table. The backend tracks its feature-level
guarantee, per-device query and implemented usage separately. Transfer layout
uses two bytes per texel, and view filterability follows the actual
`SHADER_SAMPLE` query. It remains non-renderable and cannot be copied from or
read back directly.

The strict hardware test uploaded a 5x4 signed pattern from byte offset 2 using
12-byte rows, sampled both signed channels, remapped to RGBA8 and validated all
80 pixels with a one-code allowance for UNORM rounding. The physical adapter
was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), requested and selected
FL9_1, R8G8_SNORM support mask `0x00021320`. The test used nearest filtering;
the format query also reported filtered-sampling support. The first strict run
found the zero-valued SNORM midpoint rendered as 127 instead of the model's
128; the test was corrected to allow one UNORM code, not to alter shader output.
The rerun passed. A Clippy pass flagged the initial 9-argument fixture helper;
it was refactored into a descriptor plus input/output parameters, leaving only
the ten pre-existing warnings.

Verification on Rust 1.93.1/MSVC:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_texture_tests -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

The two focused hardware tests passed 2/2. The full strict private D3D11 suite
passed 309 tests, with zero failures and two filtered. HAL build and Clippy,
core's zero-adapter test (1/1), DX12 tests (2/2), formatting and diff checks
passed. Ten pre-existing HAL Clippy warnings remain. The mission continues;
ordinary public adapter enumeration and HWND surface exposure remain gated.
Next, verify the required four-channel R8G8B8A8_SNORM sampled/upload subset,
then reassess the texture/depth and public-core contract gaps.

## Checkpoint — queried R8G8B8A8_SNORM sampled uploads

`312dba141` adds a one-mip, one-layer R8G8B8A8_SNORM sampled/upload path. The
required-format table identifies the FL9_1 2D shader-sample minimum through
legacy Q8W8V8U8. The internal model records the feature-level guarantee,
selected-device `CheckFormatSupport`, and implemented usage separately. The
transfer path accounts for four bytes per texel; render-target, copy-source,
readback and mip-chain uses remain rejected.

Strict hardware execution on the RTX 3090 Ti requested and selected FL9_1. The
device returned `R8G8B8A8_SNORM` support mask `0x000213a0`, including 2D and
shader-sample support. A padded 5x4 signed image was uploaded from byte offset
4 with 24-byte rows, sampled through the private shader/binding path and
remapped to RGBA8. All 80 pixels matched within one UNORM code value for
conversion rounding. The test also verifies the HAL rejects copy-source,
render-target and mipmapped descriptors.

Verification on Rust 1.93.1/MSVC:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 rgba8_snorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_texture_tests -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

All three focused sampled-format GPU cases passed 3/3. The complete strict
private D3D11 suite passed 311 tests, zero failed, with two filtered. HAL build
and Clippy, core zero-adapter isolation (1/1), targeted DX12 tests (2/2),
formatting and diff checks passed. HAL Clippy reports ten existing warnings;
none were introduced by this change. The current texture test helper also
asserts unsupported copy-source, render-target and mipmapped descriptors fail
at the HAL boundary. Public D3D11 adapter enumeration remains disabled.

Next investigate the required FL9_1 BC1 format and compressed buffer-to-texture
layout. Establish block row/height calculations and boundary rules with CPU
tests before attempting native upload and strict sampled-pixel verification.

## Checkpoint — BC1_UNORM sampled upload

Implementation checkpoint: `6c67e22be` (`dx11: add FL9_1 BC1 sampled texture uploads`).

BC1_UNORM is modeled as an FL9_1 required sampled format, but support remains
conditional on the selected device's `CheckFormatSupport` result. The private
HAL accepts one-mip, one-layer, 4-aligned sampled/upload textures only. Upload
layout uses 4x4 blocks of eight bytes; padded block rows, nonzero source
offsets, and block-aligned partial updates are validated with checked
arithmetic. Copy-source, render-target, readback, sRGB and mip-chain paths
remain rejected.

On the RTX 3090 Ti (vendor/device `0x10de:0x2203`, driver `32.0.16.2101`), the
strict test explicitly requested and selected FL9_1. `BC1_UNORM` support was
`0x000213a0`, including shader sampling. Four distinguishable 4x4 blocks were
uploaded into an 8x8 image from source offset 8 with 24-byte block-row pitch;
a second aligned partial upload replaced one block using offset 8 and 16-byte
pitch. Sampling and RGBA8 readback matched all 256 expected pixels.

Rust 1.93.1/MSVC verification:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 bc1 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

The BC1-filtered run passed 3/3 after formatting, including actual hardware
render/readback. The full strict private suite passed 315 tests with two
filtered before the final readability-only refactor of texture-format
validation. The HAL build passed; the focused core isolation test passed 1/1;
DX12 pipeline descriptor tests passed 2/2; formatting and diff checks passed.
Clippy passed with ten existing warnings, unchanged from the prior checkpoint.
Ordinary public D3D11 enumeration remains empty. The strict fixture-enabled
core library suite subsequently passed 70/70 with BC1 included, and the
focused core fixture matched all 64 pixels for `Bc1` on the forced-FL9_1 RTX.
It creates the BC1 texture and view, uploads
compressed blocks through `Global::queue_write_texture`, binds them, renders,
and maps readback. The fixture reports only `SAMPLED | COPY_DST` for BC1 and
rejects every other BC format at both HAL capability query and core texture
creation.

Checkpoint `1e59db97a` adds the test-only BC1 capability and core render case.
Checkpoint `6b86270ec` routes the main core fixture through
`Global::adapter_request_device` and the fixture-gated HAL `Adapter::open`
instead of creating a device directly from HAL. The strict core suite still
passed 70/70. The plain D3D11 HAL build and focused fixture pass; targeted
Clippy passes with ten existing HAL warnings. Ordinary DXGI/HAL adapter
enumeration remains empty. The fixture still injects manually constrained
limits and is not evidence that a truthful public adapter report exists.

Next audit the core/HAL adapter capability boundary and derive a small,
accurate, testable FL9_1 report from the existing capability model without
enabling ordinary enumeration. Identify every core-legal operation that such
a report would permit but HAL rejects; implement narrow validation or keep
that capability out of the report. Public adapter and HWND surface exposure
remain gated.

## Core capability-validation checkpoints — 2026-11-04

`96eacac49c26596a021672c5d405bb99fdbc56b1` adds
`DownlevelFlags::TWO_SIDED_STENCIL`. Core rejects differing front/back stencil
face state when the flag is absent, and the controlled FL9_1 fixture verifies
the missing-flag error before HAL. GLES, Vulkan and compliant/default flag sets
report support. Core format-feature projection for RGBA8, D24S8, BC1 and an
unsupported BC format is also checked against the fixture's manually limited
HAL report. This does not create a production adapter capability report.

`3c9a7fca156b45aad8fcfb4c99db39d36f5f8bed` honors the existing
`COMPARISON_SAMPLERS` flag during core sampler creation. The fixture verifies a
comparison sampler returns `MissingDownlevelFlags` before HAL. The strict
fixture-enabled core library suite passed 70/70; its GPU fixture ran on the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) with requested and actual
FL9_1, and all exercised image/readback checks passed. The regular core
zero-adapter isolation test passed 1/1. Core Clippy, targeted rustfmt and
`git diff --check` passed. The first shell attempt lacked `link.exe`; rerunning
inside Visual Studio 2022 Build Tools' `VsDevCmd.bat` succeeded. No HAL or
non-D3D11 code path failures were observed.

Next audit sampler/layout, blend, render-pass, and resource validation paths
for other core-legal requests that can reach an unsupported D3D11 HAL case.
Prioritize cases that can panic, issue invalid native commands, or fail only
after resource creation. Public enumeration and surface exposure remain
disabled.

`73ef1a6ada7a4db1fa8515de35745a7b7abd8720` adds
`DownlevelFlags::SEPARATE_ALPHA_BLEND` and core validation for render targets
that write both RGB and alpha with different blend equations. The controlled
FL9_1 core fixture confirms full-channel `ALPHA_BLENDING` fails with the
missing flag before HAL, while RGB-only alpha blending remains accepted by the
existing masked-channel implementation. GLES, Vulkan and default/compliant
capability sets report the flag; the FL9_1 D3D11 fixture leaves it absent.

Verification on Rust 1.93.1/MSVC: strict fixture-enabled core tests passed
70/70 (the controlled GPU fixture used the RTX 3090 Ti at requested/actual
FL9_1); core zero-adapter isolation passed 1/1; wgpu-types tests passed 55/55;
the combined DX11/DX12/Vulkan/GLES HAL check passed; core Clippy passed; DX12
pipeline descriptor regression passed 2/2; targeted rustfmt and
`git diff --check` passed. The wgpu-types test build emitted 14 existing
unused-serde-import warnings with serde disabled. Continue the core/HAL audit;
do not enable adapter enumeration based only on these targeted validations.

## Checkpoint — storage-buffer validation boundary — 2026-11-04

`58fbd32b4db44ce1e6432bfa941464899b695e7b` closes a core/HAL mismatch found
during the continuing adapter-contract audit. Core adds storage access bits to
some otherwise ordinary buffer allocations. The D3D11 HAL now accepts those
bits only alongside another supported buffer role; FL9_1 still receives its
single vertex or index native bind and no shader storage binding is implemented.
Core now requires `VERTEX_STORAGE`, `FRAGMENT_STORAGE` and `COMPUTE_SHADERS` for
storage-buffer layouts visible to those stages; fragment-writable bindings also
require `FRAGMENT_WRITABLE_STORAGE`. The strict controlled fixture verifies all
four missing-flag cases are rejected before HAL, then uploads, copies and reads
back 64/64 bytes from a `STORAGE | COPY_SRC | COPY_DST` buffer.

Verification used Rust 1.93.1/MSVC and `WGPU_DX11_REQUIRE_HARDWARE=1` on the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with requested and actual
FL9_1. The focused core fixture and full fixture-enabled core library suite
passed; the latter was 70/70, including the ordinary zero-adapter test. The
strict private D3D11 suite passed 315/315 (two filtered). The D3D11 HAL build,
combined DX11/DX12/Vulkan/GLES HAL check, core and HAL Clippy, standalone core
zero-adapter test (1/1), and DX12 pipeline descriptor test (2/2) passed.
`git diff --check` and targeted rustfmt passed. HAL Clippy reported ten
pre-existing warnings. Workspace `cargo fmt --check` remains blocked by a
pre-existing line-wrap discrepancy in `wgpu-core/src/instance.rs:1511`; that
unrelated file was not changed. The VS developer-shell banner still prints
`vError: Unknown error` even though MSVC builds and links succeed.

Next continue auditing core-legal operations that the private HAL rejects,
especially render-pass, texture/view and resource-lifetime boundaries. Public
D3D11 adapter enumeration and public surface exposure remain disabled.

## Checkpoint — texture-view dimensions rejected before HAL — `c844eab6e` — 2026-11-04

The audit found that the D3D11 HAL only creates `D2` views, while core could
forward `D2Array`, `Cube` and `CubeArray` layouts/views to it. A one-layer
`D2Array` view is legal as a descriptor shape even when the texture-array
limit is one, so the limit did not close this path. Added explicit
`TEXTURE_2D_ARRAYS` and `CUBE_TEXTURES` downlevel flags and require the
appropriate flag for sampled layout dimensions and resolved texture-view
dimensions. D3D11 leaves both flags absent. GLES, Vulkan and Metal now report
their existing array/cube support; public D3D11 enumeration remains closed.

The strict core fixture on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, requested and actual FL9_1) verified that 2D-array, cube and
cube-array layouts fail at core validation, and that a one-layer `D2Array`
view fails before HAL. The fixture-enabled core library suite passed 70/70,
including the GPU render fixture and zero-adapter isolation; the strict private
D3D11 suite passed 315/315 (two filtered). `wgpu-types` passed 55/55, the
combined DX11/DX12/Vulkan/GLES HAL check passed, core and HAL Clippy passed,
the standalone zero-adapter test passed 1/1 and the DX12 pipeline descriptor
regression passed 2/2. HAL Clippy retained ten existing warnings; workspace
format checking still reports the existing unrelated line-wrap in
`wgpu-core/src/instance.rs:1511`. Targeted rustfmt and `git diff --check`
passed. The VS developer-shell banner still prints `vError: Unknown error`
while builds succeed.

Next audit texture creation and render-pass attachment shapes, particularly
dimensions, format usages and core-valid resolve/load/store combinations.
Keep public adapter enumeration and surface exposure disabled.

## Checkpoint — texture-layout dimension validation — `8c072d4f7` — 2026-11-04

The continued layout audit found that the private HAL accepts only D2 sampled
views, while core could forward D1/D3 sampled binding layouts even when the
device's corresponding texture-dimension limit was zero. Core now rejects
those layouts with a validation error based on the requested device limits.
The audit also found that storage-texture `D2Array` layouts did not pass
through the array downlevel check applied to sampled textures; they now require
`TEXTURE_2D_ARRAYS` before format validation. The existing array/cube view
flags and texture creation checks remain unchanged.

The strict controlled core fixture passed on the RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, requested and actual FL9_1). It reported the expected
core errors for D1/D3 layouts, storage-texture D2Array, sampled array/cube
layouts and a one-layer D2Array view, then completed all existing GPU readback
cases. The full fixture-enabled core library suite passed 70/70, including
zero-adapter isolation. The core DX12 library suite passed 67/67. Core Clippy,
targeted rustfmt and `git diff --check` passed. The DX12-only feature build
emitted fifteen existing warnings because WGSL-dependent code was compiled
without the WGSL feature; no new failures were reported. The Visual Studio
developer-shell banner continues to print `vError: Unknown error` while Rust
builds and tests succeed.

Continue the texture descriptor/render-pass audit, then return to the broader
core-valid command inventory. Public D3D11 adapter enumeration and surface
exposure remain disabled.

## Checkpoint — FL9_1 single-sample output masks — `8eff9020a` — 2026-11-04

The core/HAL audit found that core permits a single-sample pipeline with a
zero sample mask, while the private HAL previously rejected every non-default
multisample descriptor and hard-coded the native mask to all ones. The pipeline
now retains the WebGPU mask and passes it to the output merger. For FL9_1 only,
a native zero mask is translated to `0xffff_fffe`: sample zero remains masked,
all other bits are irrelevant for the supported single-sample target, and the
native mask is nonzero as required by the downlevel context. Multisampling and
alpha-to-coverage remain rejected. A core-fixture render case verified that
all 64 pixels retain the blue clear color with `mask = 0`.

Rust 1.93.1/MSVC verification on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) requested and selected FL9_1. The focused strict HAL translation
test passed 1/1; the focused strict core fixture passed 1/1 and printed that all
64 pixels matched; the complete strict private D3D11 suite passed 318/318; the
complete fixture-enabled core library suite passed 70/70, including the
zero-adapter isolation test; and the core DX12 library suite passed 67/67.
The non-test D3D11 HAL `cargo check` passed. HAL and core Clippy passed, with
ten existing HAL warnings and no core warnings. Targeted rustfmt and
`git diff --check` passed. The Visual Studio developer-shell banner continues
to print `vError: Unknown error` despite successful compiler initialization.

Code checkpoint: `8eff9020a` (`dx11: honor single-sample masks at FL9_1`).
Public adapter enumeration and public surface exposure remain disabled. The
controlled core fixture still injects restricted capabilities and is not a
production-adapter report. Continue auditing texture creation and render-pass
validation; keep historical FL9_1 hardware and the public integration path
listed as unverified.

Commands for the focused checks were:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
```

The focused unit and fixture selectors, both Clippy commands, and targeted
format commands are recorded in the session checkpoint; the full commands
above include the corresponding suites and ordinary library check.

## Checkpoint — core rejects unavailable NPOT mip chains — `1eb21620c` — 2026-11-04

The texture descriptor audit found that core forwarded non-power-of-two mip
chains to HAL without checking the existing
`NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` downlevel flag. The D3D11 HAL only
implements power-of-two mip chains and rejects NPOT mip resources regardless
of the modern driver's optional full-NPOT query. Core now checks the flag for
multi-mip textures using the spatial dimensions for D1, D2 or D3; D2 array
layer counts are correctly excluded. The controlled FL9_1 fixture requests a
6×8 RGBA8 two-mip texture with the flag absent and observes the specific core
`MissingDownlevelFlags` error before HAL.

The CPU dimension test passed 1/1. Under strict hardware mode, the core fixture
ran on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested
and actual FL9_1, printed `PASS: NPOT mip chain rejected by core validation`,
and completed all render/readback cases; the complete fixture-enabled core
suite passed 71/71. The core DX12 suite passed 68/68, with the existing
no-WGSL-feature warnings; the ordinary zero-D3D11-adapter test passed 1/1.
Core Clippy, targeted rustfmt and `git diff --check` passed. Other backends
retain their existing flag reports: the default-compliant DX12/Metal flags and
the explicit Vulkan/GLES flags permit this path.

Code checkpoint: `1eb21620c` (`core: gate unsupported NPOT mip chains`). The
HAL was not changed; its last strict private suite passed 318 tests at
`8eff9020a`. Public D3D11 enumeration and surface exposure remain disabled.
Continue the core/HAL contract audit, especially blend-state legality and
resource/view combinations that cannot be represented by the current flags.

Commands:

```text
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std npot_mipmap_tests -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

## Checkpoint — general single-target blend translation — `f99cae113` — 2026-11-04

The custom blend audit found that the HAL accepted only a fixed set of named
blend presets even though core permits ordinary custom single-target blend
equations. The HAL now translates the ordinary non-dual-source blend factors
and all five operations, maps `Src`/`Dst` differently for RGB and alpha
components, and records/rebinds blend constants in command order. It rejects
`SrcAlphaSaturated`, all dual-source factors and differing full-channel RGB/
alpha equations unless separate-alpha support is established. That capability
remains unavailable in the production FL9_1 model. The native descriptor
mirrors target zero into the first four native slots so their `BlendEnable`
bits satisfy the Microsoft FL9_1 rule while only one target is bound.

An initial full-suite run exposed three stale assertions that expected the old
translator to reject a custom diagnostic equation or expected inactive blend
fields to retain preset values. After updating those tests to assert the new
contract, the full strict HAL suite passed 321/321. Its GPU blend test executed
constant and inverse-constant factors with changing blend constants, then Add,
Subtract, ReverseSubtract, Min and Max. All 64 pixels matched. The RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) was selected as hardware and the device
was explicitly requested and confirmed at FL9_1.

The controlled core fixture passed 71/71, including the custom constant-blend
render and 64-pixel readback through core command recording; the fixture still
injects restricted test capabilities. The core DX12 suite passed 68/68 with
fifteen existing warnings from the no-WGSL feature configuration. The ordinary
zero-D3D11-adapter isolation test passed 1/1. `cargo check` for the D3D11 HAL
passed. HAL Clippy passed with ten existing warnings and no new warnings; core
Clippy passed cleanly. Targeted rustfmt and `git diff --check` passed. The
Visual Studio developer-shell banner continues to print `vError: Unknown
error` even though the MSVC environment initializes and builds succeed.

The change does not establish historical FL9_1 behavior and does not enable a
public adapter. All factors are CPU-translation tested, but GPU readback uses
constant factors and `One` factors; other factor visual results remain
unverified. Full-channel separate-alpha equations remain gated. See Microsoft's
[10Level9 blend-state restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
and [`OMSetBlendState` blend-factor behavior](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-omsetblendstate).

Commands:

```text
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/tests/dx11/blend_state.rs wgpu-hal/tests/dx11/textured_render.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

Code checkpoint: `f99cae113` (`dx11: generalize FL9_1 blend-state translation`).
Next continue the core/HAL render-state audit; depth comparison functions are a
small candidate for verification and expansion. Keep adapter enumeration and
public surface exposure disabled, and preserve the separate-alpha restriction.

## Checkpoint — native FL9_1 raster depth comparisons — `340147803` — 2026-11-04

The pipeline previously rejected Less, Equal, Greater, NotEqual and
GreaterEqual depth comparisons despite the D3D11 depth-stencil descriptor
using the complete `D3D11_COMPARISON_FUNC` enum. The similarly worded FL9_1
LessEqual restriction in Microsoft's 10Level9 material concerns comparison
sampler filtering, not raster depth state. The HAL now maps all seven native
functions. Never retains the prior validated-draw suppression and active
stencil depth-fail remapping.

The new strict hardware test renders eight scissored regions with an expected
pass and fail depth for each comparison. It verifies all 128 RGBA pixels: 64
green pixels where depth passes, 64 blue clear pixels where it fails. Never's
two regions remain blue through the existing suppression path. Device creation
explicitly requested and asserted FL9_1 on the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, driver previously recorded as `32.0.16.2101`).
Historical FL9_1 implementations remain untested.

The pre-change strict `fl91_depth_test_preserves_near_geometry_and_loads_depth`
test passed 1/1. The first full post-change suite found one stale assertion that
expected Less pipeline creation to fail (321 passed, 1 failed); the assertion
was replaced with creation checks for the newly supported comparisons. The
rerun passed the complete strict D3D11 suite 322/322. The full controlled
fixture-enabled core suite passed 71/71, including its hardware fixture. Its
focused hardware fixture passed 1/1 and logged its texture, buffer, depth and
topology readbacks. Ordinary D3D11 adapter isolation passed 1/1.

`cargo check` and HAL Clippy passed. Clippy reported ten pre-existing warnings
in fence, HAL device tests, swapchain test ordering and the texel-load fixture;
none were in the changed files. Rustfmt check and `git diff --check` passed.
The Visual Studio developer-shell banner still prints `vError: Unknown error`
while initializing MSVC successfully. The core DX12 suite was not rerun because
this checkpoint changes only the private D3D11 pipeline/test path and docs.

Commands:

```text
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test_preserves_near_geometry_and_loads_depth -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_comparison_functions_match_expected_fragments -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
rustfmt --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/tests/dx11/depth_comparison.rs wgpu-hal/tests/dx11/depth_render.rs
git diff --check
```

## Checkpoint — FL9_1 slope-scaled triangle depth bias — `6f7f1297891ac3477ed7055ded00934da1c46c2a`

The native rasterizer path now carries both the signed constant bias and finite
`slope_scale` from `DepthBiasState` into `D3D11_RASTERIZER_DESC`. FL9_1's
required zero `DepthBiasClamp` remains enforced. Nonzero bias on points and
solid lines is rejected because Microsoft documents that bias has no effect on
those primitives (except wireframe lines, which this backend does not create).
Selected fragment entries writing `frag_depth` still reject either bias term.

The strict forced-FL9_1 RTX 3090 Ti test passed five complete constant-bias
images and two slope-scaled images. In the sloped tests, the first red triangle
writes a varying depth; positive slope bias prevents the second green triangle
from replacing it, and negative slope bias lets green replace it. All 64 pixels
matched in each case. Synthetic pipeline tests cover triangle acceptance,
point/line rejection, non-finite slope, clamp rejection and topology-specific
translation. The existing controlled core fixture still validates the positive
and negative constant-bias cases, but the new slope cases exercise the private
HAL path only.

The complete strict private D3D11 suite passed 322/322; six unsafe runtime
integer-index GPU subcases reported their expected rejection and skip. The
focused hardware test passed 1/1, the D3D11 HAL check passed, the ordinary core
zero-adapter test passed 1/1, and the strict controlled core fixture passed
1/1, including its depth-bias, topology and texture readbacks. HAL Clippy passed
with ten existing warnings outside the changed files. Targeted rustfmt and
`git diff --check` passed. The core DX12 suite was not rerun because only the
private D3D11 path and its test changed. MSVC initialization continued to print
the existing `vError: Unknown error` banner while compiling successfully.

The first new GPU assertion mistakenly expected the target clear color after a
depth-failing second draw; observed red correctly showed that the first draw's
color remained. The expectation was corrected and the strict test passed.
FXC's diagnostic for the existing direct-only `frag_depth` shader remains an
expected `X4530` because the profile requires all four `SV_Target0` components.
Microsoft documents that clamp is unsupported at FL9_x and describes the
slope-bias equation and primitive behavior in [D3D11 depth bias](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-output-merger-stage-depth-bias)
and [10Level9 rasterizer restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).
Modern hardware forced to FL9_1 does not establish historical bias precision.

Code checkpoint: `6f7f1297891ac3477ed7055ded00934da1c46c2a`
(`dx11: support FL9_1 slope depth bias`). Public adapter enumeration remains
disabled. Documentation checkpoint: `5bf9140034703754110639375168096218a2155d`
(`docs: record FL9_1 slope depth bias checkpoint`).

Commands:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_triangle_depth_bias_and_pipeline_transition -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
rustfmt --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/tests/dx11/depth_bias.rs
git diff --check
```

Documentation checkpoint remains to be committed after this journal update.

## Checkpoint — core rejects unavailable FL9_1 depth-bias clamp — `a281a4c3f2c9e5efa91be992c5cbd401b9cec33c`

The controlled FL9_1 core fixture now sets a nonzero `DepthBiasState::clamp`
while its downlevel capabilities omit `DEPTH_BIAS_CLAMP`. It verifies that
core returns `CreateRenderPipelineError::MissingDownlevelFlags` for exactly
that flag before reaching the private HAL, then restores the descriptor and
continues the existing GPU tests. This confirms the HAL's native FL9_1 clamp
restriction has a corresponding safe core rejection path; normal adapter
enumeration remains disabled.

The full fixture-enabled core library suite passed 71/71 with strict hardware
required. A rerun of the focused hardware fixture passed 1/1 on the NVIDIA RTX
3090 Ti at explicitly requested and actual FL9_1 and printed the new rejection
result; all existing depth, texture, transfer, topology and readback checks
still passed. Core Clippy and targeted rustfmt passed. The fixture suite and
this focused case use the test-only adapter, not production adapter reports.

Code checkpoint: `a281a4c3f2c9e5efa91be992c5cbd401b9cec33c`
(`dx11: verify FL9_1 depth clamp core gate`). Documentation checkpoint:
`20f69046714e7f9ffb061df3825d0eaf52bdfabf` (`docs: record FL9_1 depth clamp
core validation`). Resume with the public HAL/core capability-boundary audit
in FL9_1_PLAN.md.

## Checkpoint — raster depth-bias capability facts — `1a362ec6108de9accb52e2f1978a174d1f923106`

`HardwareGuarantees` now records native constant and slope-scaled depth bias
for the recognized levels and records `DepthBiasClamp` only from FL10_0.
`BackendCapabilities` independently records the implemented D24S8 triangle
constant/slope paths and the absent clamp and biased-`frag_depth` paths. Tests
assert the feature-level distinction across all seven identities and ensure
the FL9_1 backend implementation is not promoted when a higher level is
modeled. The strict hardware diagnostic prints those facts from the selected
device level, separate from driver queries.

The targeted capability-model test passed 1/1. The complete strict private
D3D11 suite passed 322/322 on the RTX 3090 Ti explicitly requested and selected
at FL9_1. Its indexed-render diagnostic showed `depth_bias=true`,
`slope_scaled_depth_bias=true`, `depth_bias_clamp=false`, with the backend
reporting D24S8 triangle constant/slope support and clamp/biased-fragment-depth
support false. The D3D11 HAL check passed; HAL Clippy passed with ten existing
warnings outside the changed capability file; targeted rustfmt and
`git diff --check` passed. The six expected dynamic-index GPU skips described
in the earlier depth-bias checkpoint remain. Public enumeration remains empty.

Code checkpoint: `1a362ec6108de9accb52e2f1978a174d1f923106`
(`dx11: model raster depth-bias capabilities`). Documentation checkpoint:
`ae7a75b16a10dc5e98d3d4e7b38a9eace300d1a1`
(`docs: record FL9_1 raster bias capability model`). Continue with the
public HAL/core capability-boundary audit in FL9_1_PLAN.md.

## Checkpoint — core gates multi-mip depth attachments — `983eea619`

Microsoft's 10Level9 device-method table states that a resource created with
`D3D11_BIND_DEPTH_STENCIL` must have one mip at FL9_1, FL9_2 and FL9_3. Core
format features do not describe this mip-count/usage combination, while the
private D3D11 HAL creates D24S8 only as a single-mip depth attachment. Added
`DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP` and require it when a depth/stencil
texture with more than one mip is also a render attachment. The private D3D11
capability model records no implementation support at any modeled feature
level; hardware guarantees are separately modeled as absent through FL9_3 and
present from FL10_0. GLES and Vulkan explicitly report support; DX12 and Metal
retain the capability from their default downlevel reports.

The controlled core fixture omits the flag and verifies a two-mip D24S8
attachment returns `MissingDownlevelFlags(DEPTH_TEXTURE_MULTI_MIP)` before HAL.
This is core validation evidence, not an attempted native multi-mip DSV call.
Strict fixture execution completed on NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with FL9_1 requested and selected.
The existing fixture then completed its texture/depth/topology rendering and
readbacks. Normal D3D11 enumeration remains empty.

Validation used Rust 1.93.1, MSVC and `CARGO_BUILD_JOBS=8`. The fixture-enabled
core suite passed 71/71; strict private HAL suite passed 322/322; ordinary
zero-adapter isolation passed 1/1; core DX12 suite passed 68/68; the focused
wgpu-types capability test passed 1/1; combined D3D11+GLES HAL check and core
Clippy passed. HAL Clippy completed with ten existing warnings in unrelated
D3D11 fence/device/test-support code. A plain PowerShell test invocation first
failed because MSVC `link.exe` was not initialized; rerunning under
`VsDevCmd.bat` succeeded. A test build during implementation also caught a
missing initializer for the newly modeled backend field; it was fixed before
the successful runs.

Code checkpoints: `983eea619` (`core: gate multi-mip depth attachments by
capability`), followed by `834cd9856` (`hal: report multi-mip depth support on
Vulkan`). Cross-backend checks used:

```text
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-hal --no-default-features --features vulkan
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features vulkan
```

Documentation checkpoint is recorded in the following commit.
Continue the core/HAL audit before constructing the public capability
snapshot; do not enable enumeration or surface exposure.

## Checkpoint — selected-entry vertex texture limits in core — `68cccdf11`

Added `DownlevelLimits::max_sampled_textures_per_vertex_stage` so backends can report vertex sampled-texture capacity separately from the general per-stage limit. Core counts sampled and depth textures statically used by the selected vertex entry point, using Naga's validated per-entry resource usage (including helper calls and `textureLoad`), and rejects excess use before HAL pipeline creation. The D3D11 capability fixture reports zero at FL9_1; the test confirms a vertex texture pipeline fails with the specific core validation error before the private HAL. D3D11 hardware guarantees record vertex texture sampling as absent through FL9_3 and present from FL10_0, while backend implementation remains false. GLES queries its actual vertex texture unit count; Vulkan and noop report their current capacities. Ordinary D3D11 enumeration remains disabled.

Verification used Rust 1.93.1/MSVC with `CARGO_BUILD_JOBS=8`:

- Core selected-entry usage unit test: 1/1 passed.
- wgpu-types downlevel-compliance test: 1/1 passed.
- Strict forced-FL9_1 core fixture: 1/1 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`); the fixture confirmed `TooManySampledTexturesInVertexStage { given: 1, limit: 0 }` before HAL and then completed GPU rendering/readback.
- Full strict fixture-enabled core suite: 72/72 passed, including zero-D3D11-adapter isolation.
- Strict private D3D11 HAL suite: 322/322 passed with hardware required; expected negative shader tests still print `SKIP` for unsupported operations after confirming rejection.
- Core DX12 regression: 69/69 passed.
- HAL D3D11+GLES and Vulkan checks, wgpu-info check, and core Clippy passed. HAL Clippy passed with ten pre-existing warnings in D3D11 fence/device/test support. Targeted formatting and `git diff --check` passed.

The first DX12 invocation omitted `std,wgsl` and failed during test compilation; the corrected command passed. An initial attempt to construct a vertex-visible layout in the fixture reached a HAL device-lost path, so the test now reuses the supported fragment-visible layout: the core vertex-resource limit is checked before that later layout compatibility path. The actual unsupported vertex texture execution remains unattempted, by design.

Code checkpoint: `68cccdf111f36553e36f8328ec80ec3195786c80` (`core: gate vertex textures by downlevel limits`). Next continue the core/HAL capability-boundary audit; do not enable public adapter enumeration or surface exposure.

## Checkpoint — conditional FL9_x NPOT sampling in core — `bb22b4636`

Core now retains Naga's validated sampling pairs for the selected render-pipeline entries and checks those pairs against the current bind groups during draw readiness. On devices without unrestricted NPOT sampling, only a used NPOT texture with a `Repeat` sampler is rejected; an unused texture/sampler cross-pair does not cause a false rejection. D3D11 derives the capability from the selected feature level and a positive `D3D9_OPTIONS` query, treating negative and unavailable 9_x queries conservatively. The private HAL applies the same rule. Public D3D11 adapter enumeration remains disabled.

Verification used Rust/Cargo 1.93.1 and MSVC. The strict GPU suites ran with
`CARGO_BUILD_JOBS=16`; the final focused checks and Clippy runs used
`CARGO_BUILD_JOBS=32`:

- Strict core fixture suite: 73/73 passed; physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), software=false, requested/actual FL9_1. Its driver query reports full NPOT=true; the fixture synthetically sets the core capability false. The used NPOT/repeat pair was rejected at command encoding before submission. POT/repeat plus NPOT/clamp rendered 64/64 expected yellow pixels while an unused NPOT/repeat cross-pair remained bound.
- Strict private D3D11 HAL suite: 323/323 passed on forced FL9_1 hardware. The six legacy shader-operation cases that explicitly report `SKIP` remain unsupported negative probes, not GPU passes.
- Core zero-adapter isolation: 1/1 passed. Core DX12 regression: 70/70 passed.
- Core, HAL, wgpu-types and wgpu-info Clippy passed. HAL emitted ten existing warnings outside this change; wgpu-types emitted 14 existing serde-import warnings under default features. Rustfmt check on all changed Rust files and `git diff --check` passed. Workspace-wide `cargo fmt --all -- --check` still reports formatting differences in unchanged `naga/src/back/hlsl/writer.rs` and `wgpu-core/src/instance.rs`.

One attempted zero-adapter invocation omitted the `wgsl` feature and failed while compiling its WGSL-based unit test; the corrected invocation below passed. No code failure remained.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The core feature and test checkpoint is `bb22b4636` (`core: validate FL9_1 NPOT sampling pairs`). Documentation and this journal are recorded separately.

## Checkpoint — source-alpha-saturated blend validation — `e001cfcd0`

The core/HAL audit found that D3D11 10Level9 rejects `SRC_ALPHA_SAT` at FL9_1,
9_2 and 9_3, but core only gated separate-alpha and dual-source blending. A
blendable render-target format cannot represent this individual-factor
restriction. `DownlevelLimits::src_alpha_saturated_blend` now records the
capability, participates in minimum-capability checks and WebGPU-compliance
reporting, and is displayed by `wgpu-info`. Core rejects the factor in color or
alpha source/destination slots before HAL. The D3D11 model records 9_x as
unsupported and 10_0+ as guaranteed by hardware while keeping backend support
false at every level; no higher-level execution path was enabled. GLES and noop
report the baseline capability.

Verification used Rust 1.93.1, MSVC, `CARGO_BUILD_JOBS=32` and one Cargo process
at a time:

- `cargo test --offline -p wgpu-types --lib -- --test-threads=1`: 58/58 passed.
- Strict fixture-enabled core suite: 74/74 passed, including all four factor
  placements rejected by core and the existing GPU rendering/readback cases.
- Strict private D3D11 HAL suite: 323/323 passed with `WGPU_DX11_REQUIRE_HARDWARE=1`.
- Core zero-D3D11-adapter isolation: 1/1 passed; core DX12 suite: 71/71 passed.
- D3D11+GLES HAL, wgpu-info and wgpu-test checks passed. Core, wgpu-test and
  wgpu-info Clippy passed. HAL Clippy passed with ten existing warnings;
  wgpu-types Clippy passed with fourteen existing unused-serde-import warnings.
- Targeted rustfmt and `git diff --check` passed.

The commands ran in an initialized Visual Studio 2022 MSVC shell with
`CARGO_BUILD_JOBS=32`:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-types --lib -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-info
cargo check --offline -p wgpu-test
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo clippy --offline -p wgpu-types --tests
cargo clippy --offline -p wgpu-test --tests
cargo clippy --offline -p wgpu-info
rustfmt --edition 2021 --check wgpu-types/src/limits.rs wgpu-core/src/pipeline.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs tests/src/params.rs wgpu-info/src/human.rs
git diff --check
```

The strict tests selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
software=false), requested and selected FL9_1. The fixture does not print the
driver version in this run; prior baseline notes record `32.0.16.2101`. The
four unsupported blend descriptors themselves are validation-only and were
rejected before HAL; the rest of the fixture and private HAL suite performed
real GPU work. The first fixture attempt exposed that its test descriptor also
triggered the existing separate-alpha gate; RGB-only and alpha-only write masks
isolated the new check, after which the strict suite passed.

Next continue auditing core-valid vertex formats and attachment/viewport state.
The independent read-only audit found that core accepts baseline vertex
formats which `hal_pipeline.rs::vertex_format` does not translate; this needs a
precise format gate or implementation before public capability reporting.
Public D3D11 enumeration and surface exposure remain disabled.

## Checkpoint — FL9_1 vertex format reporting and input validation — `cd82e503d3954a86467c2d42f1ca39a6cbb3c66d`

Audited Microsoft's required DXGI format table and the existing mismatch
between core's accepted baseline vertex attributes and the private D3D11
input-layout mappings. Added `DownlevelLimits::vertex_format_mask`, with
baseline defaults preserved for existing adapters and the 64-bit formats still
controlled by their feature. Core now rejects attributes missing from that
per-format mask before HAL; the D3D11 capability fixture fills the mask from
the selected FL9_1 level, per-device `CheckFormatSupport(IA_VERTEX_BUFFER)`
results and the implemented HAL map. Ordinary D3D11 enumeration remains empty.

The private HAL now maps FL9_1-required `R8G8B8A8_UINT`, `R16G16_SINT` and
`R16G16B16A16_SINT` input formats in addition to the already supported float,
unorm and signed-normalized formats. The existing shader guard remains closed
to runtime integer math; it allows only direct integer vector vertex inputs
converted to float in the FL9_1 vertex shader. The Naga-to-FXC pipeline and
GPU test exercise the actual native layouts. Synthetic capability tests check
feature-level minima, driver/backend intersections and negative format-query
results. Microsoft's required-format table also identifies `R16G16_FLOAT` and
`R16G16B16A16_FLOAT` IA support beginning at FL9_3; this is recorded as a
hardware minimum only and does not enable those formats in the private HAL.

Verification used the pinned Rust 1.93.1 toolchain in the Visual Studio 2022
MSVC shell, with one Cargo process at a time and `CARGO_BUILD_JOBS=32`:

- Baseline strict `fl91_snorm16_vertex_formats_render`: 1/1 passed before the
  edit on the RTX 3090 Ti at requested/actual FL9_1.
- D3D11 HAL build: `cargo check --offline -p wgpu-hal --no-default-features --features dx11` passed.
- wgpu-types unit suite: 59/59 passed.
- Capability identity, mask projection, native mapping/query and integer
  shader-guard tests: each focused CPU test passed 1/1.
- Strict new `fl91_integer_vertex_formats_render`: 1/1 physical GPU test
  passed. Adapter NVIDIA GeForce RTX 3090 Ti, vendor/device `0x10de:0x2203`,
  software=false, driver `32.0.16.2101`; requested and actual feature level
  FL9_1. The driver reported `IA_VERTEX_BUFFER` flags `0x2` for each new
  format. The projected mask was `0x0000000078c30104`; all 64 RGBA8 pixels
  matched the expected yellow output.
- Full strict private D3D11 HAL suite: 326/326 Rust tests passed. This includes
  real GPU rendering/readback cases; a small set of negative shader probes
  intentionally reports unsupported execution as `SKIP` after checking safe
  rejection and is not counted as GPU execution.
- Full strict fixture-enabled core suite: 74/74 passed, including physical
  rendering/readback; the new `Unorm16x2` pipeline case was rejected by core
  before HAL. Core zero-D3D11-adapter isolation: 1/1 passed.
- Core DX12 regression suite: 71/71 passed. HAL D3D11+GLES check, wgpu-test
  check, wgpu-info Clippy, core Clippy and `rustfmt --check` on changed Rust
  files passed.
- HAL Clippy passed with ten existing warnings in unchanged D3D11 support and
  test code. wgpu-types Clippy and tests emitted fourteen existing unused
  serde-import warnings under the no-serde default feature configuration.
  `git diff --check` and the staged diff check passed.

An initial chained shell invocation of the non-D3D11 checks returned exit code
1 without diagnostics; each check was rerun separately and passed. No test or
build failure remains. Public D3D11 adapter and surface exposure remain gated.

Next audit the remaining core-valid render-pass attachment, vertex-layout and
pipeline descriptors against the actual HAL, then prioritize the public-core
integration blockers. Keep the historical-driver and Present reentrancy gaps
explicit; this modern-GPU result does not validate old FL9_1 hardware.

## Checkpoint — per-format single-mip validation — `53484db82`

Added an opt-in `SINGLE_MIP_ONLY` bit to HAL and wgpu-types format capability
flags. Core projects the bit and rejects a texture descriptor with more than
one mip before invoking HAL. The D3D11 fixture reports the bit for queried
single-mip sampled/upload formats (R8_UNORM, R8G8_SNORM, R8G8B8A8_SNORM and
BC1_UNORM) and D24S8. This closes a validation gap that device-wide NPOT mip
and depth-attachment flags did not express for POT sampled textures. Other HALs
leave the opt-in bit unset. The core fixture verifies one-mip creation and
pre-HAL rejection of POT 8×8 two-mip descriptors for every supported sampled
upload format; the existing BC1 render case still performs GPU upload,
sampling and readback.

Verification used Rust 1.93.1 and the Visual Studio 2022 MSVC environment with
`CARGO_BUILD_JOBS=32`, one Cargo process at a time:

- Baseline before editing: strict core FL9_1 fixture 1/1 and focused BC1 HAL
  tests 3/3. The physical adapter was NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1; BC1 sampling
  matched 256/256 pixels.
- wgpu-types suite: 59/59 passed.
- Strict controlled core fixture: 1/1 passed after the change. It reported
  `SINGLE_MIP_ONLY` for D24S8 and BC1, verified all queried single-mip sampled
  formats, accepted their one-mip descriptors, rejected their POT two-mip
  descriptors in core, and completed existing render/readback cases including
  BC1 at 64/64 pixels. Hardware remained the RTX 3090 Ti at requested/actual
  FL9_1. This run did not print the driver version.
- Focused BC1 HAL tests: 3/3 passed, including 256/256 GPU output pixels.
- Full strict private D3D11 HAL suite: 326/326 passed. Full strict
  fixture-enabled core suite: 74/74 passed. Zero-adapter isolation: 1/1 passed.
  Core DX12 regression suite: 71/71 passed.
- `cargo check` passed for HAL with `dx11,gles` and core with `dx11,std,wgsl`.
  Core, HAL and wgpu-types Clippy passed. HAL retained ten existing warnings
  in untouched code; wgpu-types retained fourteen existing serde-import
  warnings under the default no-serde test configuration.
- Targeted `rustfmt` checks on changed Rust files and `git diff --check`
  passed. The formatter's unrelated change to the existing zero-adapter test
  was restored before staging. The strict core fixture was rerun after the
  final assertion change.

Commands run (inside the initialized MSVC shell; `WGPU_DX11_REQUIRE_HARDWARE=1`
for both strict hardware suites):

```text
cargo test --offline -p wgpu-types --lib -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-core --no-default-features --features dx11,std,wgsl
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo clippy --offline -p wgpu-types --tests
rustfmt --edition 2021 --check wgpu-hal/src/lib.rs wgpu-types/src/texture/format.rs wgpu-core/src/resource.rs wgpu-core/src/device/resource.rs wgpu-hal/src/dx11/adapter.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

`wgpu-core/src/instance.rs` was included in the successful formatter run, then
its unrelated pre-existing formatting change was reverted. Its new mapping
block was inspected and `git diff --check` passed.

Code commit: `53484db82` (`dx11: validate single-mip texture format limits`).
Public D3D11 adapter enumeration and public HWND surface exposure remain
disabled. Next continue the core/HAL texture descriptor and render-pass audit;
do not treat this focused validation repair as completion of public adapter
readiness.

## Checkpoint — reusable per-format capability projection

Moved the fixture's duplicated format-capability mapping into
`Capabilities::texture_format_capabilities`. It derives D24S8, color-target,
copy, sampled and sampled-upload flags from the selected device's successful
format query and the backend's implemented paths. The fixture now delegates to
this production internal method. Missing and negative queries produce no
capabilities, and the method remains unused by ordinary D3D11 enumeration.
RGBA8/BGRA8 transfers are gated by their backend implementation switches.
Code commit: `7b3b5be08` (`dx11: centralize FL9_1 format capability projection`).

Baseline: the previous fixture-only blend-query test passed 1/1. An initial
attempt outside the MSVC developer shell failed at link because `link.exe` was
not on PATH; rerunning with `VsDevCmd.bat` resolved the environment issue.
Verification on Rust 1.93.1/MSVC (`CARGO_BUILD_JOBS=32`, one Cargo process):

- D3D11 `format_projection` tests passed 4/4, synthetic/CPU-only.
- `cargo check -p wgpu-hal --no-default-features --features dx11,gles` passed.
- Strict fixture-enabled core suite passed 75/75 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. It reported
  RGBA8, D24S8 and BC1 format projections and completed existing GPU
  render/readback cases. Its ordinary D3D11 zero-adapter isolation test passed.
- Strict private D3D11 HAL suite passed 329/329 on the RTX 3090 Ti at forced
  FL9_1; no tests were skipped.
- Core DX12 regression suite passed 72/72.
- HAL Clippy passed with ten existing warnings in untouched code.
- Targeted rustfmt and `git diff --check` passed.

The capability projection did not enable public enumeration and is not a full
adapter report. The public adapter and surface gates remain closed. The modern
RTX forced to FL9_1 does not substitute for historical hardware testing.
