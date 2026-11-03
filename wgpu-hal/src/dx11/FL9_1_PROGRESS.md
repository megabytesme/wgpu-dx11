# FL9_1 progress

## Current position — 2026-11-03

Branch: `uwp-dx11`. The previous private uniform work was independently
reviewed, corrected for unused-entry-point binding conflicts, tested and
checkpointed as `26739b52d1ae042a6499b578521eb3250f1d9c21` (generalized static
uniform bindings). No public D3D11 adapter is exposed.

The verified tree has a restricted private renderer with sampled textures,
group-0 uniforms including dynamic offsets, RGBA8 transfers, indexed draws,
restricted D24S8 depth attachments, event-query fences and a hidden-HWND
presentation test. It lacks general depth/stencil, verified visible HWND
presentation, a usable public adapter and numerous operations required by core.
See CAPABILITIES.md for the detailed matrix.

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
