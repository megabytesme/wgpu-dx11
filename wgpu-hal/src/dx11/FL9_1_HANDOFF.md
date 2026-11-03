# FL9_1 handoff

**Outcome: PARTIALLY COMPLETE.** The mission is active. The private D3D11 HAL
can render on a forced-FL9_1 RTX 3090 Ti, but ordinary wgpu adapter enumeration
is disabled. An unsafe, nondefault test fixture has exercised public wgpu
rendering, but no ordinary public adapter or HWND presentation has been
verified. A hidden HWND test has exercised the private surface, but visible
presentation and surface lifetime/concurrency still require verification.
A private D24S8 attachment path is implemented and its textured
depth/readback test has passed on forced FL9_1; stencil tests,
aspect-specific views and general render-pass operations remain incomplete.
Depth-only drawing without a fragment stage also passed a private GPU prepass
test; its trivial pixel-shader fallback has not been tested on historical GPUs.
See FL9_1_PLAN.md for completion gates,
FL9_1_PROGRESS.md for current commands and evidence, and CAPABILITIES.md for the
current implementation and validation matrix.

Resume on `uwp-dx11`; inspect `git status` and recent history before editing.
Viewport emulation was checkpointed as `ec645e63e` and retains the
compiler-budget limitation recorded in FL9_1_PROGRESS.md. Dynamic uniform
offsets were checkpointed as `edb1efa6b` after strict GPU and regression tests.
The private swapchain locking and selected-entry shader-load validation
changes have passed strict tests; check the working tree and recent commits
for their checkpoint status. Swapchain locking does not resolve DXGI window-callback
reentrancy or presentation completion in `wait_for_idle`. Do not infer full
FL9_1 or historical-device compatibility from
modern GPU tests. Keep public adapter enumeration gated until core-valid
operations are safe. The latest combined strict suite passed 209 tests; older
tests may skip internally, so use focused strict tests for hardware evidence.

## Verified checkpoints

The autonomous work began with `26739b52d1ae042a6499b578521eb3250f1d9c21`
(generalized static uniform buffers), then added:

| Commit | Scope |
| --- | --- |
| `19e82da409dd0ceec12eb9e9dca8934900b5ff11` | Nested uniform layouts and mission records |
| `424dfaa2f46e757e9d3a0dad9c50b9f110a18592` | Private textured D24S8 depth rendering |
| `e09825a90b32dc52eb83cd27f084c2803abf96c5` | FL9 default buffer bind flags |
| `8949812c55514e106e7803e00d7e25801e516d44` | Downlevel index usage validation |
| `7bdffefe244101bf7374fbc10044424f08eea8c1` | Adapter error expectations |
| `26825d96e8da4d9864f741527286d99a9494b53d` | Empty compute passes and unused blend constants |
| `4afbb21da9f1f319ff39fe73f2dceed58935a279` | Zero-sized viewports |
| `d13062b65c53a5a5fcac1f644bbec0d581d7e078` | Non-finite viewport rejection in core |
| `721469cfeabbb86995d3654b84eff1bbeae5d4a6` | Attachment discard flags |
| `bf56c17b99352a354cae3a4929504880fdc395ef` | Private hidden HWND surface |
| `7e045c4ed83e9506054161da24c4f13a4986af7b` | Surface-view lifetime and resize |
| `ec645e63e084fad3c377ccaf0f3b21ed49ee9e13` | Restricted clipped viewport emulation |
| `edb1efa6b3c07ac080a5a38f85de83282079b112` | Dynamic uniform offsets |
| `9464c4aef6bd02cb37dfd8aba5adaa0bb697d5ad` | Private swapchain/context serialization |
| `7a7f249f5bc54cfc67678fc00755b9d8429e7338` | Selected-entry texture-load validation |
| `4bba5f9c53e1ed70fa8569cc9ed7eeec30f06a0f` | Single-target color write masks |
| `f8bd5183a770dd7e81568516f6a3e3411a8de7d8` | Triangle face culling and winding |
| `f4df16d8b` | Pipeline-wide register planning |
| `a914c9a1f95f7806069051deef55630b9a8244f5` | Private multi-group uniform bindings |
| `47e8488ef19be593b8d4e899a567f2c302d73861` | Private cross-group sampled resources |
| `f6aa8f241415822eb2a58f8f992e2ff7e7123ba4` | Gated core buffer round trip |
| `86db48c45` | Gated core offscreen rendering |
| `6fe0282e7` | Zero inter-stage validation limits |
| `28c4a2f1a` | Gated core textured depth rendering |
| `4c2255de2` | Private checked indexed base vertices |

## What is and is not usable

No operation currently executes through a public D3D11 wgpu adapter: normal
adapter enumeration returns zero and the ordinary HAL adapter is uninhabited. A
nondefault unsafe fixture injects a restricted device into public wgpu and core
for closed offscreen tests; it is not a general adapter or public example.
The private path supports restricted WGSL-to-Naga-to-HLSL-to-FXC shaders,
sampled RGBA8 and BGRA8 textures, single-mip transfers, multiple groups of static and dynamic uniforms,
triangle rendering with front/back culling, D24S8 Always/LessEqual depth
testing and channel masks.
This is not a general capability report or WebGPU conformance claim. The
capability matrix in CAPABILITIES.md specifies the tested subset and gaps.

The private hidden HWND path acquired, rendered, read back, presented and
resized, but visible desktop presentation has not been verified. CoreWindow
has prior private diagnostics; Windows 10 Mobile ARM32 and historical FL9_1
hardware have not been tested. `wait_for_idle` only has GPU event-query evidence,
not evidence that presentation completed. A synchronous HWND callback can
deadlock if it reenters context work or waits on the active surface operation.
This is a public presentation blocker even with context serialization.

Before exposing an adapter, core-valid states such as broader primitive and
vertex layouts, resource shapes and bindings need implementation or safe
validation. Private HAL errors can currently surface as device loss. Clipped
viewport emulation can exceed the FXC instruction budget, and the tested FXC
profile rejects fragment-position and front-facing inputs. Stencil operations,
read-only depth with sampled aliases, depth comparisons other than Always/LessEqual,
broader formats and mipmaps remain incomplete. FL9_1 does not guarantee
separate-alpha blending; the default path now rejects modes that require it,
while masked-channel equivalents remain supported. Native compute and UAV storage are not
FL9_1 guarantees. Do not replace those gaps with fabricated capabilities.

Core permits draw combinations, layouts and resource shapes outside the private
subset; the existing downlevel flags alone do
not make all of them unreachable. A safe public backend needs implementations,
sound fallbacks or targeted core validation for each such case. The unsafe,
owner-matched fixture proves buffer and textured indexed depth rendering through
core and public wgpu calls; it does not make general adapter exposure safe.
An occlusion-query downlevel flag now rejects unavailable query sets in core
before they can cause device loss. The D3D11 fixture omits the flag; ordinary
adapter exposure remains gated.
Private read-only D24S8 passes require load/store of both aspects, no sampled
aliases and no depth or stencil writes. Keep the public read-only flag off.
Checked shaders permit dynamic access to proven value vectors. Dynamic vector
pointers, uniform-pointer indexing and unknown provenance remain restricted.
The tested FXC rejected a negative-index case before GPU execution; successful
read cases do not prove that case or historical drivers.
Nonindexed triangle strips use parity-preserving, rebased batches. Indexed strip
restart is not a guaranteed FL9_x native operation and remains rejected pending
a separate translation; accepting a strip index format in a private pipeline
does not make indexed strip draws supported.
Private input-layout validation now rejects overlapping element byte ranges on
FL9_1. Core does not yet express that restriction; stream-offset and stride
behavior remains unverified on historical hardware.
Private direct triangle lists support up to sixteen sparse vertex-rate slots.
The existing slot-0 indexed and strip paths remain available; multi-slot and
nonzero-slot indexed/strip draws are rejected until all streams can be rebased
safely. Overlapping physical bytes read through separate slots remain unverified.
Naga's HLSL special handling of matrix stores in structures can omit an index
clamp or pass a destination by value. The private HAL now rejects stores through
matrix-containing aggregates in both shader modes with a bounded selected-entry
analysis. This intentionally rejects some safe writes and does not fix shared
Naga; historical compilers and drivers remain unverified.
The Dx11 unreachable in core's shader path is specifically in unsafe passthrough,
not normal validated WGSL; its reachability still needs an audit before exposure.

## Reproduction and latest evidence

On Windows 11, Rust 1.93.1 and the MSVC developer environment, use
`CARGO_HOME=C:\dev\personal\wgpu\target\codex-cargo-home`,
`CARGO_TARGET_DIR=target/codex-milestone1`, `CARGO_BUILD_JOBS=8`, and
`WGPU_DX11_REQUIRE_HARDWARE=1`. In an x64 Visual Studio developer shell:

```text
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 dx11:: -- --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test_preserves_near_geometry_and_loads_depth -- --nocapture
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo clippy --offline -p wgpu-hal --lib --tests --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture
```

The strict private suite most recently passed 209 tests with two filtered;
some older tests can skip internally. The focused physical-GPU color-mask test
reported NVIDIA RTX 3090 Ti, vendor 0x10de, device 0x2203, driver 32.0.16.2101,
requested and actual FL9_1. It checked all 64 pixels for each of 16 masks,
three transition regions, zero color writes with depth writes and masked
additive blending. Multiple uniform groups, dynamic offsets, changed register
plans, and group-0 texture sampling with a group-1 uniform also passed strict
GPU readback. Cross-group textures and samplers, shared and multiple samplers,
transitions and feedback rejection have also been exercised; see the progress
log and `git status` for checkpoint status. The focused hidden HWND test read
BGRA [0,0,255,255] and
reported successful present and recreation. Core's zero-adapter test passed;
the fixture-only core GPU upload/copy/map test matched 64/64 bytes, its
offscreen draw matched 64/64 red pixels, and the
fixture-enabled core library suite passed 67 tests. Ordinary enumeration and
normal adapter opening remain disabled. The fixture-on core Clippy check passed.
targeted Clippy completed with ten existing warnings. Earlier work also ran
Naga, core, public Noop validation and targeted DX12 regressions as recorded
in FL9_1_PROGRESS.md. No public D3D11 example or visual display test ran.

The opt-in `wgpu` integration test used the public Rust API with an unsafe
injected fixture. On the forced-FL9_1 RTX, GPU-uploaded Uint32 indexed textured
depth rendering produced 32 red and 32 green pixels. Ordinary D3D11 enumeration
remained empty before and after injection. This is a controlled test, not
general public exposure.

Ordinary BGRA8 textures and transfers are supported in the private subset.
Native copies require identical formats; upload/readback preserve raw BGRA
bytes, while shader sampling and render targets use logical RGBA channels.

Core's zero inter-stage limit panic was reproduced in a public Noop test and
fixed with signed deduction arithmetic. The expanded focused test and the
fixture-enabled core suite (68 tests) passed; see the progress log for details.
The controlled core fixture has also rendered a sampled texture with depth on
the forced-FL9_1 RTX: readback matched 32 red and 32 green pixels. This is an
unsafe, feature-gated fixture rather than ordinary public adapter enumeration.
The private indexed path also passed signed base-vertex GPU tests using
execution-time index data. Out-of-bounds vertex fetches discard the whole draw;
bounded u32 data converts to u16 scratch, while valid public draws beyond its
conservative native limits still require emulation.

Known costs include per-group scratch allocation, per-draw staging and mapping
of uniform ranges, and buffer-texture
uploads, and GPU readback for checked indexed draws. They can stall the CPU;
no representative frame-time measurement has been made. Device removal,
mapping error paths and surface thread reentrancy require additional audit.

Resume by checking `git status`, `git log` and the latest progress entry.
Keep Cargo jobs bounded and do not overlap heavyweight builds. Prioritize a
sound presentation synchronization design and core/HAL contract coverage while
continuing verified FL9_1 increments.
Zero-input pipelines and nonindexed point lists are supported privately; only
the interior-point case has GPU readback coverage, and indexed points remain
unavailable.
Full-channel separate-alpha diagnostics require an explicit test-only opt-in
and are not evidence of historical-driver support.
Private Uint32 conversion uses ordered effective-index windows spanning at most
65534 and explicitly rejects selected vertex-index shader inputs. It rebases
the vertex buffer, gathers wider triangles with GPU copies, allocates scratch at
replay, and may stall for index staging. Historical-driver high-offset and
buffer-copy behavior remains unverified.
Restricted direct and indexed instancing repeat ordered native draws for
shaders without instance-dependent inputs. Nonzero first instance is
unobservable in this subset; instance-index shaders remain unsupported.
DXGI may synchronously call a window procedure during presentation. A private
application can avoid the deadlock only by keeping its UI callback nonblocking
and queueing graphics work; that cooperation cannot be assumed for arbitrary
public callers. A worker or recursive mutex alone does not establish safety.
The documented FL9_1 minimum D3D9 caps do not establish depth write-mask support
on every historical device; the existing no-write path is GPU-tested on the
RTX but its historical portability remains unverified. A zero-write depth state
also does not make simultaneous depth sampling safe without a read-only DSV.
Only begin higher feature levels after the requested FL9_1 contract is met;
retain the shared capability-driven architecture when doing so.
