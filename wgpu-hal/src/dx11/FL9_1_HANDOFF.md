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
operations are safe. The latest strict private HAL suite passed 286 tests with
two filtered, and the current core suite passed 70/70 including two strict
FL9_1 GPU fixtures. Older HAL tests may skip internally, so use focused strict
tests for hardware evidence.

The current public-surface gate is an unresolved cross-layer reentrancy issue:
DXGI may synchronously send messages to the HWND's window procedure during
`Present`; both core's queue-index lock and the HAL immediate-context lock span
that call. See the latest section of FL9_1_PROGRESS.md and the Microsoft
threading references there. Do not drop either lock without a replacement
serialization design. Continue independent private FL9_1 work while keeping
public D3D11 enumeration disabled; next investigate native stencil operations
and reference transitions.

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
| `3d716ab7b0ec497eb6e77c5f977969a630b258b7` | dx11: test public wgpu rendering with a gated fixture |
| `e7bf588fd2127ade3a346bf00eb9936d6b50fa5c` | dx11: discard out-of-bounds indexed draws |
| `cd2c653b9123e5d77cb1e39ed92aee1681a1d0ce` | dx11: test FL9_1 vertex index shader inputs |
| `8873489b092546367c1bac41b83f6a590644bb14` | dx11: support FL9_1 depth-only drawing |
| `555a77862e6ba00f95650893457fbab62e5b4adc` | dx11: translate bounded FL9_1 32-bit indices |
| `39b439cebe0aca0359f4fd08e252ef98968a9114` | dx11: rebase bounded 32-bit index windows |
| `ee5216e5e5b2412af2073ef985ac294ed78aa3d3` | dx11: emulate restricted FL9_1 direct instancing |
| `90238dedebc8739d2dd6eb1ef967d889b4121c02` | dx11: test indexed rendering through public wgpu |
| `730b4dfec684db09ebeec2bfd9d2ce7063f6df5f` | dx11: split 32-bit indices into FL9_1 windows |
| `1033fc9f623d7414cd3817c3683e701483c1da72` | dx11: emulate restricted indexed instancing |
| `e29a2e11b6cd6575b3edadbcba629cf27931ec29` | dx11: gather sparse FL9_1 triangle vertices |
| `d8c9f0fb9eb2773d5aee2ec56d4f458fc59a8d7f` | dx11: reject unsupported primitive index inputs |
| `1a1d7a14d0939da90c09ffac6204f8562536f083` | dx11: extend FL9_1 indexed draw compatibility |
| `26992d2957fe2ae04dbf1e7fdf157e83e3fe61cc` | dx11: support zero-input FL9_1 point rendering |
| `d01ace12a78f6a78e0ca4c8f12524f69bca718ed` | dx11: enforce FL9_1 blend capability limits |
| `86a5ddfb5ffce4dfeb3709d9d6cc0caf0a55f434` | dx11: support FL9_1 BGRA8 textures and transfers |
| `17b840c1d5e96ffda2c8a71efe9b5a160063559d` | core: validate downlevel occlusion query availability |
| `03cf563b1864e4dde958c8cb3d93963143d07cf7` | dx11: support restricted read-only depth attachments |
| `7c4fd052c5019b98b49855095bdabeeea5ef4369` | dx11: allow checked local vector indexing with uniforms |
| `d77a143874521e25052bf07c77367270d72b0d20` | dx11: reject unsafe matrix aggregate shader stores |
| `ec8dbf12e42a5217655cc9a819dc1247771bb77b` | dx11: support nonindexed FL9_1 triangle strips |
| `3cdbb28f6dc1b79ff464bdfa5649f6546bb8a589` | dx11: reject overlapping FL9_1 vertex elements |
| `07b95c80159b1ed535c6359cec5b65690e409653` | dx11: extend matrix guard regression coverage |
| `894aa40c442589b72ac07c9d72c8c55475cf1ea4` | dx11: bound FL9_1 direct triangle batches |
| `82a5384336c097aa310a0d5315e2472fde374333` | dx11: support sparse FL9_1 vertex streams |
| `9a1efde6ce5d97abeb0b4e8fb9d90168e7f3501e` | dx11: support sparse FL9_1 triangle strips |
| `9bee6efe5478fe09a6a960c3d7f6b0d14a4eb1d0` | dx11: support bounded u16 indexed vertex streams |
| `954e07c28173c667f109c367e49b82bec148a21c` | dx11: translate indexed draws across vertex streams |
| `672db3fb3b883a2f2119942cc98ea09206a44981` | dx11: support transient FL9_1 attachments |
| `53abef8bcda4600b61bce04dff3ec5d431ec3bf2` | dx11: support overlapping vertex attributes on FL9_1 |
| `c9a4105b222f531029fb470178d4ffe83383297d` | dx11: support inert FL9_1 stencil states |
| `cca55a8f043dac87b41c501d0c517bf46ee8a3fd` | dx11: support signed normalized vertex inputs |
| `cc8e1aa68d9b44592a584b515171293b42714136` | dx11: support zero-stride vertex streams |
| `8653ba625e6069996e5034f58374f4ac53fae381` | dx11: emulate FL9_1 instance-rate vertex streams |
| `fc5cfb75e085b5d9b90ea60d472a7b935ec426b6` | dx11: correct vertex stream capability count |
| `61d1e7a3e0f1079628087af2088a1f6e5bfc1e41` | dx11: materialize short uniform buffer ranges |
| `62ef2660b7d2ce7e8637da98694774fa4bf67b16` | dx11: preserve single-mip fractional LOD clamps |
| `6a64195adb09fed59984167368d41080e609894a` | dx11: retain copy-only texture views |
| `1f48812f7ced0cf906ae03ca776d17e3d4432bef` | dx11: reject unsafe level-9 sample bias |
| `352c415eb0b82bfb823112b992f7309be419f917` | dx11: support copy-only FL9_1 mip chains |
| `e066d405cf7b2eb110ffec9b562d0acdd26759fc` | dx11: sample full FL9_1 mip chains |
| `34794bb52b7389492ffcbd989c09d4de8121c95f` | dx11: support suffix FL9_1 mip views |
| `4a4b6eec90c12a8d3ba35629311ab1d58e052284` | dx11: support upload-only sampled mip textures |
| `68c6a804d13cada3d1cb0feccef6bbb3d1d58d7f` | dx11: support FL9_1 color attachment mip views |
| `2206731ded87a5cdc1c9bb3c557191027c10397b` | Render and sample FL9_1 mip textures |
| `d62f52eaca76e4a8ff50eabbc9ee09910d94ffb5` | Bounded FL9_1 texture sample bias |
| `9e9c21392b0996c1ea2b6489c11c268c37379206` | Emulate non-suffix sampled mip views |
| `b431ff5d013dc9050f729b00a92973f961676ff1` | Dual-use FL9_1 mip views |
| `07eaee830f8600b1b56391089ab3df03a20bcddf` | Disjoint native mip sampling |
| `3e0fdcb9f9e8330e7cb900a021303d0e6d22c878` | Disjoint clone-backed mip sampling |
| `8045637708e23b1d4de8c54f1ddde3d54bf0c6c8` | Emulate Never depth comparison |
| `3de27b02b7659b028db3606e0e3047e07f9e75b8` | Reject unsafe FL9 integer shader semantics |
| `4cba5bf6fb21ac23699fe1b012808d0852c6f0e9` | Point draws with vertex streams |
| `8c2be917a31a0e124007b77989e2365a83fc376a` | Ordered indexed point emulation |
| `064d68dc2d98573b28730de5ba5e5f9c3aa6f83f` | Restricted constant D24S8 depth bias |
| `ddb1b7cd9d559af88ee9d16c9d73a69b1101edc0` | Same-texture distinct-mip copies |
| `735a9755645b54c06c9a1084ebf66ef49e4848e8` | COPY_DST-only mip textures |
| `7404886b4dd14da39c4859b5a33243c31f9ddd36` | Non-filtering sampler layouts |
| `4ef1fbdaafd138b5981b5765c6a33a887b54b594` | Unfilterable float layout relationship validation |
| `4f3f608c229fd6e5b7bc309db5ac975bfd790c7e` | Combined sampled-resource visibility |
| `cac502305de4c2569bfc29da73eecdd9a59e92e8` | Premultiplied alpha blending |
| `ceaf4d603dc4ecbc6449b520f5a9ef546b5cfa64` | Safe D3D11 passthrough shader rejection |
| `a690866b4a6cdc0981a1ffa448f2c28f0f41b756` | Fallible mapped-range invalidation |
| `13a4defb9` | Fallible mapped-range flush and core error propagation |

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
Runtime integer-based vector indexing is now rejected by the conservative
integer guard in checked and unchecked shaders. Earlier dynamic-index GPU
results are historical; static literal indexing and float-only controls remain.
Nonindexed triangle strips use parity-preserving, rebased batches. Indexed strip
restart is not a guaranteed FL9_x native operation and remains rejected pending
a separate translation; accepting a strip index format in a private pipeline
does not make indexed strip draws supported.
FL9_1 overlapping vertex attributes are partitioned into separate native input
slots that bind the same logical buffer and preserve its original bounds. Private
and controlled-core GPU tests passed; historical-driver behavior remains unverified.
Private direct triangle lists and nonindexed strips support up to sixteen sparse
vertex-rate slots. The existing slot-0 indexed path remains available. Multi-slot and nonzero-slot
indexed triangle lists accept Uint16 and Uint32. GPU index data is scanned
at replay against every stream; any out-of-range fetch discards the entire draw.
Native-safe Uint16 uses a direct path; other draws use common translated windows
or per-stream GPU gathers for wide triangles. All segment resources are prepared
before drawing. Index staging can stall and historical drivers remain unverified.
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

The strict private suite most recently passed 244 tests with two filtered;
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
Mapped-range invalidation now reports failures through HAL and core, and an
injected D3D11 OOM reached the map callback after releasing the shadow. Flush
and unmap still use infallible HAL methods; device-lost callback reentrancy
during mapping remains unaudited. Metal compilation and BLAS error execution
were not available on the Windows test host.

Resume by checking `git status`, `git log` and the latest progress entry.
Keep Cargo jobs bounded and do not overlap heavyweight builds. Prioritize a
sound presentation synchronization design and core/HAL contract coverage while
continuing verified FL9_1 increments.
Point lists with supported vertex and instance streams are supported privately,
including Uint16 and Uint32 indexed draws translated to ordered nonindexed runs.
Strict GPU readback covers sparse streams, transitions, rebased 65,537-point
batches, GPU-updated indices, signed bases, instances and whole-draw discard.
Indexed point replay maps GPU-authoritative indices and can stall or issue many
draws. It avoids mapping when only zero-stride or instance-rate inputs exist;
this depends on the current shader builtin and side-effect restrictions.
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

Transient color and D24S8 attachments use ordinary D3D allocations; memoryless
savings are not claimed. Private GPU rendering and core texture/view creation
are tested; a core transient render pass and historical hardware remain unverified.

The private FL9_1 path now accepts identical front/back stencil states and
retains/rebinds the reference. Asymmetric active state is rejected because
Microsoft's 10Level9 restrictions exclude two-sided stencil at 9_1. Strict RTX
readback indirectly verifies Replace, Equal, changed references and a Never
depth-fail stencil update; direct stencil readback and historical-device
validation remain unavailable.

Snorm16x2/x4 vertex inputs passed strict private FL9_1 readback, and Snorm16x2
UV passed the controlled core fixture. Four-byte private vertex alignment and
other unsupported formats remain restrictions; SNORM texture support is not implied.

Zero-stride vertex streams work through direct, strip and indexed windows/gathers
in the tested subset. Negative effective indexed vertices with only zero-stride
streams remain a specification caveat; current supported triangles are degenerate.

Restricted instance-rate streams are emulated by rebinding a zero-stride native
stream for each repeated draw. Core and strict GPU fixtures pass; indexed draws
with only instance-rate streams and shader instance_index remain unsupported.

Short uniform ranges are staged with exact GPU source bytes and zero-padded
native buffers. The unsafe core fixture sets BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED
and passes 4/5/12/20-byte rendering; no public flag is currently exposed.
Earlier RTX integer-uniform results are superseded. FXC's embedded legacy shader
can round a runtime integer comparison against 16777217 to 16777216, so selected
FL9 shaders now reject runtime integer values and operations, integer-bearing
uniforms and interfaces, and unproven initializers. Three affected tests assert
linkage rejection and still render float-only controls; six runtime-index GPU
renders are skipped after their expected rejection. General integer semantics
and historical-driver behavior remain unresolved.

Equal min/mag, anisotropy-1 samplers can normalize finite LOD clamps on the
one-mip path; pair validation checks the actual sampled view mip count.

Current FL9_1 shader profiles accept offset-free literal textureSampleBias in
[-3, 3] with equal min/mag filtering and anisotropy 1 for every used pair in
the selected entry. Dynamic or out-of-range biases and helper arguments remain
rejected. Wider bias support needs semantics-preserving lowering, and older
drivers remain unverified.

POT RGBA8/BGRA8 mip chains support initialization, transfers and sampled mip
views. Native suffix views include the smallest resource mip; non-suffix views
use a GPU clone refreshed before each draw. Strict private and controlled core
tests verify view-relative sampling and updates. A single-mip dual-use view
can render to the original and sample through a refreshed clone in later passes.
The same texture can be sampled through a disjoint native or clone-backed view
while another mip is attached. Clone refresh temporarily unbinds and restores
the active attachments. Overlapping ranges remain rejected.

Sampled mip chains also accept COPY_DST|RESOURCE without COPY_SRC. They use
shader-resource-only native textures; copy-source use and direct readback stay
unavailable. Strict GPU tests sampled uploaded and untouched levels, and core
verified 64 blue and 64 zero pixels in separate cases.

Sampler LOD policy is validated against actual selected texture/sampler pairs
and mip counts; finite max normalization and anisotropy remain restricted.

## Current stencil checkpoint

The private FL9_1 path accepts identical active front/back stencil states,
retains and rebinds the dynamic reference, and rejects asymmetric active state.
The strict RTX test indirectly verifies Replace/Equal, changed references and
an active-stencil Never-depth emulation through color readback. The full strict
HAL suite passed 287 tests, the core zero-adapter test passed, and Clippy passed
with ten existing warnings. A DX12 HAL regression build/check hit Windows paging
file error 1455 while mapping Rust crate metadata; it remains unverified for
this increment.

The one-sided stencil subset now also runs through the unsafe, feature-gated
core fixture. `ActiveStencil` records a symmetric Always/Replace pipeline with
color writes masked, then switches to an Equal pipeline and renders. On the
forced-FL9_1 RTX 3090 Ti, all 64 readback pixels matched. The full fixture-
enabled core library suite passed 70/70 tests, including ordinary adapter
isolation; the focused core hardware fixture also passed in strict mode. The
fixture continues to assert that ordinary HAL enumeration is empty. This is
controlled core/HAL coverage, not public adapter exposure or historical-device
validation.

Core fixture checkpoint: `cff2cc29b` (`core: test FL9_1 stencil through
controlled fixture`). The HAL now also classifies effective native stencil
writes and rejects depth- or stencil-writing pipelines on a read-only D24S8
attachment. This is whole-attachment read-only only: both aspects must use
LOAD|STORE, and sampled aliasing is not enabled. Strict RTX read-only testing
preserved 32 green/32 blue pixels and rejected both write cases. The complete
strict private suite passed 287 tests with two filtered; the fixture-enabled
core suite passed all 70 tests, including its zero-adapter regression. Clippy
passed with ten existing warnings. Windows paging-file error 1455 prevented a
DX12 regression check; historical FL9 hardware and public D3D11 exposure remain
unverified.

The read-only regression now also rejects a depth-Never pipeline whose requested
stencil depth-fail Replace is remapped to a native pass write. The focused strict
GPU test passed with the preserved 32 green/32 blue result. This validates the
guard against the effective native operations, not only the input descriptor.

The render-pass audit confirmed the fixture's reported capabilities prevent
core from recording the HAL's unsupported query, resolve, multisample and
multiview commands. Core rejects occlusion/timestamp query creation and now
has a 4x RGBA8 negative case; the D3D11 HAL independently rejects unsupported
render-pass descriptors. Fixture-enabled core tests passed 70/70 in strict
hardware mode, and core Clippy passed without warnings. Ordinary D3D11 adapter
enumeration remains empty.

Latest production checkpoint: `5ea3045f9` (read-only stencil guard); latest
core validation checkpoint: `8d8877e7a` (reject FL9_1 multisample textures).
The remapped stencil-write regression is checkpointed as `f2e746d83`. The core
fixture now also records a colorless D24S8 prepass, then LOADs depth in a color
pass; strict FL9_1 readback matched 32 blue and 32 green pixels. The complete
fixture-enabled core suite passed 70/70 and core Clippy passed without
warnings. Latest checkpoint: `a13699edd` (`core: test FL9_1 depth-only render
prepass`). Continue the device-loss/render-pass audit. Keep public adapter and
HWND surface exposure disabled until capability truthfulness and presentation
reentrancy are resolved.

The latest verified increment centralizes DXGI HRESULT classification and
recognizes `DXGI_ERROR_DRIVER_INTERNAL_ERROR` as device loss in queue, device,
surface and optional pipeline-variant paths. CPU error-mapping tests passed;
the strict private HAL suite passed 289 tests and the strict fixture-enabled
core suite passed 70 tests on the RTX 3090 Ti forced to FL9_1. This does not
simulate actual device removal. The follow-up audit must cover teardown and
failure recovery while preserving the existing public adapter and presentation
gates. Checkpoint: `da035664154d080ecb80a3518cbfa6e4eecbe4d8`.

The current uniform-limit follow-up replaces a sparse vertex constant probe
with a dynamically indexed 255-element `float4` array. FXC accepts 255 and
rejects 256/257 at constant index 256; 32 pixel constants compile and 33 fail
at register 32. The forced-FL9_1 RTX accepted native shader-object creation for
the 255-element vertex profile. This shader was not drawn because private WGSL
rejects runtime integer indexing. The near-limit execution gate remains open.
The focused strict `constant` filter passed 8 tests (284 filtered). The complete
strict HAL suite passed 290 tests with two filtered. The targeted HAL build and
Clippy passed; Clippy retained ten existing warnings. Core's zero-D3D11-adapter
test passed 1/1. Formatting and `git diff --check` passed. Commit this bounded
finding, then continue the loss/teardown and public-contract audit without
enabling ordinary adapter or HWND surface exposure.
Checkpoint: `f775292a2` (`dx11: verify FL9_1 vertex constant limits`).

## Latest checkpoint — line topologies through the controlled core fixture

`e79428c50` adds LineList and LineStrip to the unsafe, feature-gated core
fixture. Core creates the actual pipeline and records nonindexed draw commands;
the injected HAL device remains unavailable through ordinary enumeration. The
strict RTX 3090 Ti FL9_1 readback contained 8 sampled red pixels for LineList
and 12 for LineStrip, with the rest of the target remaining clear blue. The
fixture-enabled core library suite passed 70/70 tests and core Clippy passed.
The previous private HAL line-topology implementation remains checkpointed as
`3db26eed3` and had passed the strict private suite and targeted hardware tests.
Next assess indexed LineList conversion and batching. Indexed strips remain
separate pending primitive-restart and connectivity analysis.

## Previous checkpoint — non-indexed line topologies

`3db26eed3` adds private non-indexed LineList and LineStrip pipelines and draw
replay. Line-list batches preserve vertex pairs; line-strip batches overlap one
endpoint. Indexed line topologies remain rejected. Strict FL9_1 readback on the
RTX 3090 Ti verified 22 red and 12 green pixels, and an indexed line-list
command was rejected before submission. The full strict private HAL suite
passed 294 tests with two filtered; the targeted core zero-adapter isolation
test, HAL build and Clippy passed. The working tree is clean after this
checkpoint. Next validate line topology through the unsafe controlled core
fixture, then continue toward indexed semantics and the broader public contract.
