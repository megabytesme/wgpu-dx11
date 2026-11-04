# FL9_1 handoff

**Outcome: PARTIALLY COMPLETE.** The mission is active. Ordinary wgpu D3D11
adapter enumeration remains disabled; controlled core-fixture tests do not
constitute a public adapter. The private HAL has tested D24S8 depth/stencil,
depth-only prepasses, stencil operations, read-only attachments, depth-bias
subsets, multiple texture formats and mip views, uniform layouts/ranges, and
indexed/nonindexed point, line and triangle topologies on a modern RTX 3090 Ti
explicitly forced to FL9_1. Historical FL9_1 compatibility has not been
verified. Public HWND presentation and the cross-layer `Present` reentrancy
deadlock remain unresolved; keep both ordinary adapter and public surface
exposure gated.

The indexed TriangleStrip checkpoint `f1b8c09a7` and strict core-fixture
coverage `c46c4a908` are followed by static boundary execution tests for vertex
c254 and pixel c31. At forced FL9_1 on the RTX 3090 Ti, the test verifies that
the vertex uniform shifts geometry and the pixel uniform changes color (17
green and 4 red pixels). Feature checkpoint `0c0ff00ba` follows the original
c254 checkpoint `6cac808c9`. Dynamic integer indexing remains rejected. The
full strict HAL suite passed 305 tests (two filtered); see FL9_1_PROGRESS.md
for exact commands and the limits of that evidence. Continue from
FL9_1_PLAN.md and update the progress journal after each verified checkpoint.

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
operations are safe. The latest strict private HAL suite passed 301 tests with
two filtered, and the current core suite passed 70/70 including strict FL9_1
GPU fixtures. The focused line-topology test and core fixture ran on the RTX
3090 Ti at explicitly requested and selected FL9_1. Older HAL tests may skip
internally, so use focused strict tests for hardware evidence.

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
| `2ff5a58e8` | dx11: emulate FL9_1 indexed line-strip restart |
| `f1b8c09a7` | dx11: emulate FL9_1 indexed triangle strips |
| `c46c4a908` | docs: record FL9_1 indexed triangle-strip checkpoint |
| `6cac808c9` | dx11: verify FL9_1 final vertex constant execution |
| `0c0ff00ba` | dx11: execute FL9_1 pixel constant boundary |
| `340147803` | dx11: support native FL9_1 depth comparisons |

## What is and is not usable

No operation currently executes through a public D3D11 wgpu adapter: normal
adapter enumeration returns zero and the ordinary HAL adapter is uninhabited. A
nondefault unsafe fixture injects a restricted device into public wgpu and core
for closed offscreen tests; it is not a general adapter or public example.
The private path supports restricted WGSL-to-Naga-to-HLSL-to-FXC shaders,
sampled RGBA8 and BGRA8 textures, single-mip transfers, multiple groups of static and dynamic uniforms,
triangle rendering with front/back culling, D24S8 native depth comparisons
(all except emulated Never) and channel masks.
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
read-only depth with sampled aliases, broader formats and mipmaps remain
incomplete. `Never` depth comparison remains emulated. FL9_1 does not guarantee
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
Nonindexed triangle strips use parity-preserving, rebased batches. Indexed
LineStrip and TriangleStrip do not rely on native FL9_x strip-cut support:
replay resolves the base vertex, splits fixed-index restart markers, validates
all non-restart fetches, and translates the runs to bounded native draws.
TriangleStrip expands each run in winding-preserving triangle-list order,
using compact Uint16 windows or per-stream gathers for wide triangles. Both
indexed strip formats are covered by private and controlled-core GPU tests.
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
The fixture change was independently reviewed, with no isolation or correctness
issue identified. Indexed LineList was subsequently implemented for Uint16/Uint32
and verified through both private HAL and the controlled core fixture. Indexed
strip support is recorded in the later checkpoints below.

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

## Previous checkpoint — indexed FL9_1 LineList

Indexed Uint16 and Uint32 LineList draws preserve pairs through compact
rebasing, gather wide pairs, ignore an incomplete tail and discard invalid
fetches before issuing native draws. The RTX 3090 Ti at requested/actual FL9_1
read back 22 red, 12 green, 24 blue and 14 white pixels. The strict private
suite passed 297 tests with two filtered; the strict core fixture suite passed
70/70 and its focused run confirmed eight red pixels for both index formats.
HAL build and Clippy passed (ten existing HAL warnings), core Clippy passed,
formatting and diff checks passed. Ordinary adapter enumeration remains empty.
Checkpoint `82c9b9228` (`dx11: add FL9_1 indexed line-list rendering`).

## Latest checkpoint — indexed FL9_1 LineStrip

`2ff5a58e8` emulates fixed-index LineStrip restart in the private HAL. It
resolves the signed base vertex before restart matching, splits runs, compacts
bounded windows, preserves connectivity across windows and gathers wide edges.
The strict RTX 3090 Ti tests requested and selected FL9_1. They verified both
index formats, marker creation/removal through base adjustment, format mismatch
rejection, invalid-fetch discard, and a checked pixel on a gathered wide edge.
The focused strict hardware test passed 1/1; the complete strict private suite
passed 301/301 with two filtered. The controlled core fixture passed 70/70,
including indexed Uint16/Uint32 LineList and LineStrip while ordinary adapter
enumeration remained empty. HAL build passed, HAL Clippy completed with ten
pre-existing warnings, core Clippy passed, targeted DX12 tests passed 2/2, and
format/diff checks passed. Historical FL9_1 hardware is unverified. Public
adapter exposure and HWND surface exposure remain open.

## Current implementation — indexed FL9_1 TriangleStrip

Indexed Uint16/Uint32 TriangleStrip restart is emulated at command replay.
Resolved indices are split into runs, validated against every vertex-rate
stream, expanded with alternating winding, and sent through compact native
triangle-list windows or wide-triangle gathers. Strict readback compared both
formats against equivalent triangle lists for every front-face/cull pairing;
the wide gather rendered 32 checked red pixels. The controlled core fixture
rendered both formats with 12 red pixels each while normal adapter enumeration
remained empty. The complete strict private suite passed 304 tests, the core
fixture suite passed 70, HAL build and Clippy passed (ten existing Clippy
warnings), core Clippy passed, and DX12 pipeline descriptor tests passed 2/2.
See `FL9_1_PROGRESS.md` for commands and format-check caveats. The feature
is checkpointed as `f1b8c09a7` (`dx11: emulate FL9_1 indexed triangle strips`).
Broader instancing, indirect draws, public adapter exposure and public HWND
surface exposure remain open.

## Latest checkpoint — required R8_UNORM sampled uploads

`268431ec3` adds single-mip R8_UNORM sampling and buffer-to-texture upload,
gated by actual device format support and an explicit FL9_1 capability model.
Strict execution on the RTX 3090 Ti (vendor/device `0x10de:0x2203`, driver
`32.0.16.2101`) requested and selected FL9_1, uploaded from byte offset 1 with
7-byte rows, and matched 80/80 readback pixels. The complete private HAL suite
passed 307 tests; core zero-adapter isolation and targeted DX12 tests passed.
R8 render targets, copy-source/readback, mip chains and linear-filter visual
validation remain unsupported or unverified. Full evidence and commands are
in `FL9_1_PROGRESS.md`.

The R8G8B8A8_SNORM follow-up noted below fulfilled the next-format task from
this checkpoint. Ordinary adapter enumeration remains disabled. Continue
auditing public-core/HAL contract gaps and presentation reentrancy; do not infer
historical-device support from the modern forced-level GPU.

`881640796` completes the required R8G8_SNORM sampled/upload increment. Strict
execution reported support mask `0x00021320`, uploaded padded signed data from
a nonzero source offset and matched 80/80 pixels within one UNORM code value.
The full strict private suite passed 309 tests (two filtered); core isolation,
DX12 regression, build, lint and formatting checks passed. See the latest
progress entry for the initial midpoint-rounding observation and its bounded
test tolerance.

## Latest checkpoint — required R8G8B8A8_SNORM sampled uploads

`312dba141` adds a queried single-mip R8G8B8A8_SNORM sampled/upload path.
Strict execution on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
requested and selected FL9_1, observed support mask `0x000213a0`, uploaded a
padded signed texture from a nonzero offset and matched all 80 rendered pixels
within one UNORM code value. The full strict private suite passed 311 tests
(two filtered); core zero-adapter isolation, DX12 regression, build, Clippy and
formatting checks passed. See FL9_1_PROGRESS.md for exact commands.

## Latest checkpoint — FL9_1 BC1 sampled uploads

`6c67e22be` adds queried BC1_UNORM sampled/upload textures, restricted to a
single mip/layer and 4x4-aligned dimensions. Block-row layout supports padded
pitch, source offsets and aligned partial updates. Copy-source, render-target,
readback, sRGB and mip chains remain rejected. The RTX 3090 Ti at requested and
actual FL9_1 reported support mask `0x000213a0`; strict sampled readback
matched 256/256 pixels. The complete strict private suite passed 315 tests
before a readability-only validation refactor, and the focused BC1 tests passed
3/3 after it. The HAL build, core zero-adapter test, DX12 pipeline tests,
Clippy (ten pre-existing warnings), formatting and diff checks passed.

`1e59db97a` extends the controlled core fixture through BC1 texture creation,
queue upload, binding, sampling and mapped RGBA8 readback; the tested red/green
regions matched all 64 pixels. The fixture reports only BC1 `SAMPLED | COPY_DST`
and rejects all other BC formats at both HAL capability query and core texture
creation. The strict fixture-enabled core library suite passed 70/70.

`6b86270ec` changes the main fixture device creation to
`Global::adapter_request_device`, which calls the fixture-gated HAL
`Adapter::open`. The strict forced-FL9_1 core render fixture passed, including
BC1. The regular HAL build also passed with the test-fixture feature disabled.
The test still injects hand-chosen limits/features and therefore does not
validate a production adapter capability report. Next audit and model that
core/HAL boundary without enabling ordinary adapter enumeration or surface
exposure.

## Current core validation fixes

`96eacac49c26596a021672c5d405bb99fdbc56b1` adds a core-visible
`TWO_SIDED_STENCIL` downlevel flag. The FL9_1 controlled fixture omits the flag
and confirms differing front/back stencil descriptors fail before HAL. GLES
and Vulkan report their existing two-sided stencil support.

`3c9a7fca156b45aad8fcfb4c99db39d36f5f8bed` implements the existing
`COMPARISON_SAMPLERS` downlevel check in core. On the forced-FL9_1 fixture, a
comparison sampler now returns the missing-flag validation error before the
private HAL rejects it. The full fixture-enabled core suite passed 70/70; the
GPU fixture ran on the RTX 3090 Ti at requested/actual FL9_1. The core
zero-adapter test passed 1/1 and core Clippy and formatting passed. Public
enumeration remains empty. Continue the core/HAL contract audit; next check
blend, sampler/layout and resource restrictions that lack a sufficiently
narrow core validation path.

`73ef1a6ada7a4db1fa8515de35745a7b7abd8720` adds a
`SEPARATE_ALPHA_BLEND` downlevel flag and requires it when a pipeline writes
both RGB and alpha using different blend equations. The forced-FL9_1 core
fixture verifies full-channel straight-alpha blending is rejected before HAL
and RGB-only masked blending remains accepted. Strict fixture-enabled core
tests passed 70/70 on the RTX 3090 Ti at requested/actual FL9_1; wgpu-types
tests passed 55/55, combined backend HAL check passed, core Clippy passed,
zero-adapter isolation passed 1/1 and DX12 pipeline tests passed 2/2. Continue
auditing public core/HAL contract mismatches and preserve the closed public
adapter gate.

## Current checkpoint — `58fbd32b4`

Core now rejects storage-buffer layouts when the corresponding vertex,
fragment or compute downlevel flag is missing. The private HAL accepts core
storage-use bits only when another supported buffer usage is present and does
not implement shader storage binding. Strict RTX 3090 Ti FL9_1 tests verified
the stage-specific rejections and a 64-byte storage/copy buffer upload, GPU
copy, fence and readback. Full fixture-enabled core tests passed 70/70, and the
strict private D3D11 suite passed 315/315. Exact checks and the known formatting
and Clippy warnings are recorded in `FL9_1_PROGRESS.md`.

Resume the core/HAL contract audit at render passes, texture/view usages and
resource lifetime/error paths. Keep both public D3D11 enumeration and public
surface exposure closed; this test fixture still injects limits and is not a
production adapter capability report.

## Latest checkpoint — texture-view dimension validation — `c844eab6e`

Core now gates `D2Array`, `Cube` and `CubeArray` sampled layouts and texture
views with explicit downlevel flags. This prevents the private D3D11 HAL's
unsupported view shapes from reaching HAL, including a one-layer `D2Array`
view that `max_texture_array_layers = 1` alone cannot reject. The RTX 3090 Ti
strict FL9_1 fixture observed the expected core errors; the full fixture core
suite passed 70/70, the strict private HAL suite passed 315/315, wgpu-types
passed 55/55, combined backend HAL checks, core/HAL Clippy, zero-adapter
isolation and a DX12 regression passed. Exact commands and existing warning
details are in `FL9_1_PROGRESS.md`.

Resume at texture descriptor and render-pass validation. Public adapter
enumeration and public surface exposure remain disabled; the controlled core
fixture still injects capabilities and is not production adapter reporting.

## Latest checkpoint — texture layout dimension validation — `8c072d4f7`

Core now rejects D1/D3 texture binding layouts when the requested device limit
for that dimension is zero, and storage-texture D2Array layouts require the
same `TEXTURE_2D_ARRAYS` flag as sampled array layouts. The strict forced-FL9_1
core fixture verified these validation errors on the RTX 3090 Ti and completed
its GPU readback cases. The fixture-enabled core suite passed 70/70; the core
DX12 suite passed 67/67; Clippy and targeted formatting checks passed. The
DX12-only suite had fifteen existing no-WGSL feature warnings. Exact commands
are recorded in `FL9_1_PROGRESS.md`.

Continue auditing texture descriptors and render-pass validation. The D3D11
fixture still injects restricted capabilities; normal adapter enumeration and
public surface exposure remain disabled.

## Latest checkpoint — single-sample output masks — `8eff9020a`

The private D3D11 pipeline now forwards `MultisampleState::mask` to
`OMSetBlendState` while still rejecting multisampling and alpha-to-coverage.
At FL9_1, a zero WebGPU mask is translated to `0xffff_fffe`, which masks the
only supported sample while meeting the nonzero native argument requirement.
The controlled core fixture rendered with a zero mask and verified that all
64 pixels remained the blue clear color.

The strict private suite passed 318 tests; the fixture-enabled core suite passed
70 and includes the zero-adapter isolation regression; the core DX12 suite
passed 67. The focused strict fixture and mask translation tests each passed
1/1. HAL build/check and Clippy passed (ten existing HAL Clippy warnings); core
Clippy, targeted formatting and `git diff --check` passed. Hardware was an RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1.
See `FL9_1_PROGRESS.md` for exact commands. Next continue the core/HAL audit at
texture descriptor and render-pass validation. The fixture's injected
capabilities do not validate a production adapter, and historical FL9_1
hardware is untested.

## Core texture capability checkpoint — `1eb21620c`

Core texture creation now requires `DownlevelFlags::NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`
for a multi-mip resource with any non-power-of-two spatial dimension. D2 array
layer counts are excluded. This closes a descriptor path that reached the
private D3D11 HAL even though its mip implementation is POT-only. A strict
controlled FL9_1 core fixture verified a 6×8 RGBA8/two-mip descriptor fails
with the specific missing-flag validation error before HAL. The CPU dimension
test, full fixture-enabled core suite (71), core DX12 suite (68), explicit
zero-adapter regression (1), core Clippy, targeted formatting and diff check
passed. The core DX12 test build emitted existing warnings with WGSL disabled.
The HAL was unchanged; its last strict private suite passed 318 tests under
`8eff9020a`. See `FL9_1_PROGRESS.md` for exact commands. Continue auditing
custom blend-state validation and public adapter gaps; keep both public gates
closed.

## Checkpoint — general single-target blending — `f99cae113`

The HAL now translates standard single-target wgpu blend factors and all five
blend operations, including blend constants recorded and rebound in command
order. It rejects `SrcAlphaSaturated`, dual-source factors and differing
full-channel RGB/alpha equations unless separate-alpha support is established;
the production FL9_1 model still does not establish it. The strict D3D11 HAL
suite passed 321/321 on an RTX 3090 Ti at requested and actual FL9_1. The new
GPU case checked constant-factor changes and Add/Subtract/ReverseSubtract/Min/
Max, matching all 64 pixels. The controlled core suite passed 71/71 with a
64-pixel custom blend render, the core DX12 suite passed 68/68, and the ordinary
zero-adapter test passed. Clippy and targeted formatting/diff checks passed;
ten existing HAL Clippy warnings remain. See `FL9_1_PROGRESS.md` for commands,
driver identity and failure history.

## Latest checkpoint — native FL9_1 depth comparisons — `340147803`

All seven non-Never wgpu comparisons now map to their D3D11 depth-stencil
comparison functions. Never keeps its validated-draw and stencil-depth-fail
emulation. A strict RTX 3090 Ti test explicitly created an FL9_1 device and
verified both pass/fail samples for every comparison in 128 output pixels.
The full private HAL suite passed 322/322; the controlled core fixture passed
71/71 and the ordinary zero-adapter test passed. Ten pre-existing HAL Clippy
warnings remain. Exact commands and results are in `FL9_1_PROGRESS.md`.

Next continue the core/HAL render-state audit and build a truthful capability
snapshot without enabling ordinary adapter enumeration. Public adapter
enumeration and public surface exposure remain disabled. The HWND `Present`
reentrancy issue, historical FL9_1 hardware and production capability-reporting
gaps remain unresolved.
