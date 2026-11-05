# FL9_1 progress

## Current position — 2026-11-05

## Checkpoint — gate base-clamp sampling for 10Level9 — pending

Added `DownlevelLimits::texture_sample_base_clamp_to_edge` and wired it into
selected-entry core validation. Naga IR marks
`textureSampleBaseClampToEdge` as `ImageSample { clamp_to_edge: true }`; core
tracks that operation through helper calls and rejects it before HAL only for
the selected vertex or fragment entry point. The FL9_1 projection derives the
limit conservatively from both hardware and backend texture-query and explicit
LOD support. It remains unavailable in the current backend at every level.
The public strict test covers direct and helper-reached errors and a safe
sibling entry. An FXC test establishes X4532 under both 9_1 and 9_3 pixel
profiles and successful compilation under `ps_5_0`.

Verification (Rust 1.93.1, Visual Studio 2022 x64 developer prompt, Cargo
`-j16`): targeted wgpu-types (default and serde), selected-entry core, FXC,
strict public D3D11, strict core fixture and full private HAL suites passed;
the latter executed forced-FL9_1 GPU tests on the RTX 3090 Ti. Totals were
types 75/75 (serde 77/77), core fixture 87/87, DX12 core 83/83, public D3D11
13/13 and private HAL 344/344. The public and HAL suites selected the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
`software=false`) at requested and actual FL9_1. Targeted HAL and core
Clippy passed with nine and three pre-existing warnings respectively; wgpu
and serde-enabled types Clippy passed without warnings. HAL `dx11,gles` check,
changed-file rustfmt and `git diff --check` passed. No hardware test skipped.

The limit is additive to public `DownlevelLimits`; older serialized capability
snapshots default it to `true` to preserve existing behavior. See
`CAPABILITIES.md` for the compiler evidence and `TESTING.md` for reproduction.

## Checkpoint — indexed strips without vertex-rate inputs — `3b1d4f964`

Extended the no-vertex-rate indexed fast path to LineStrip and TriangleStrip.
At command replay the HAL copies the authoritative index buffer to its retained
staging buffer, maps the requested range, applies signed base-vertex restart
matching, splits runs, and issues bounded nonindexed strip draws. This avoids
relying on native FL9_1 strip-cut behavior. The path also covers instance-only
layouts by rebinding the instance stream for each emulated instance. Existing
vertex-rate indexed strip replay is unchanged. `vertex_index` and
`instance_index` remain unsupported.

Strict tests compare Uint16/Uint32 indexed draws with direct draws for both
strip topologies, instance-only and empty input layouts, nonzero first index,
and signed-base restart markers. The CPU planner test verifies exact restart
run lengths and overlap at bounded draw limits. Independent review found no
correctness blocker; because no vertex-rate input or vertex index can vary
vertex output, GPU image equality is necessarily degenerate, so exact run
planning is separately asserted on CPU. Additional coverage for nonzero index
binding offsets and in-submission index updates would strengthen the test.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 developer prompt
with Cargo `-j16`. The initial public test launch outside that prompt lacked
`link.exe`; rerunning from the documented MSVC environment succeeded. Results:

- Strict private D3D11 HAL suite: 343 passed, 0 failed, 0 ignored. Hardware
  cases selected the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
  driver `32.0.16.2101`, `software=false`) at requested and actual FL9_1.
- Strict public `wgpu` D3D11 integration suite: 13 passed, 0 failed,
  0 ignored on the same forced-FL9_1 device.
- Strict D3D11 core fixture suite: 86 passed, 0 failed, 0 ignored.
- DX12 core regression suite: 82 passed, 0 failed, 0 ignored.
- HAL Clippy passed with nine pre-existing warnings; no warning from this
  checkpoint. `wgpu` test Clippy passed without warnings.
- Changed-Rust-file rustfmt check and `git diff --check` passed. Workspace-wide
  formatting remains blocked only by the previously recorded untouched Naga
  HLSL writer formatting differences.

Reproduction commands (inside the Visual Studio x64 developer prompt):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
```

Continue the core/HAL contract audit. Public adapter access remains explicit
opt-in and offscreen-only; public presentation and historical FL9_1-driver
compatibility are not verified.

## Checkpoint — internal FL9_1 capability projection — `dfc93078a`

Added an FL9_1-only internal projection from the selected-device capability
model to wgpu limits, alignments, downlevel gates, features and per-format
reports. The controlled core fixture now consumes that projection instead of
injecting a separately fabricated capability set. It forces only the
conditional-NPOT branch needed for deterministic coverage and locally opts into
the aggregate BC feature only for its BC1-specific test. The production
projection withholds the BCn feature because only BC1 is implemented. It is
not connected to ordinary adapter enumeration; the core zero-adapter test
continues to verify that boundary.

The new `DownlevelLimits::texture_1d` and `texture_3d` gates let core reject
unsupported resource shapes even when their numeric dimension limits are
nonzero. Other backends retain their prior behavior through `true` defaults.
The public capability model remains provisional: numeric buffer/resource
limits, supported features and all core-valid operations still need a complete
contract audit before exposing an adapter.

Verification used Rust 1.93.1 (`+1.93`) and the Visual Studio MSVC prompt with
16 Cargo jobs. Its banner printed `Error: Unknown error`, but all compiler and
test commands completed:

- Strict controlled core suite: 78/78 passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and selected FL9_1. The fixture
  exercised uploads, copies, mip views, format gates, multiple rendering
  cases and CPU readback; unsupported 1D/3D shapes failed core validation.
- Strict private D3D11 HAL suite: 337/337 passed at forced FL9_1, with actual
  GPU rendering, transfer, fence and readback tests. Tests that intentionally
  rejected a shader before rendering logged local GPU `SKIP`s; these were not
  counted as executed draws.
- Focused capability projection tests: 4/4 passed, including the physical RTX
  query (`unrestricted_npot=Value(true)`, BC1 query positive) and the selected
  actual feature-level assertion.
- Core DX12 suite: 75/75; explicit ordinary D3D11 zero-adapter test: 1/1.
- `wgpu-types`: 65/65; serde-enabled `wgpu-types`: 66/66.
- HAL `dx11,gles`, `wgpu-info`, and `wgpu-test --tests` checks passed.
- Core fixture, wgpu-types, HAL `dx11,gles`, and wgpu-info Clippy passed.
  Existing warnings remain: 14 unused-serde imports in wgpu-types and 10
  unrelated HAL warnings. No warning was introduced by the projection.
- Targeted rustfmt and `git diff --check` passed.

One initial strict core run exposed that wgpu's color-attachment
bytes-per-sample limit assigns RGBA8 a cost of eight, despite its four-byte
storage size. The projection was corrected from 4 to 8 while retaining a
one-target count; the focused fixture and complete strict core suite then
passed. No failed test remains from that correction.

Reproduction commands (run inside the MSVC developer prompt):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --lib -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --features serde --lib -- --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11,gles
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-types --lib --tests
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 clippy --offline -j16 -p wgpu-info
rustfmt --edition 2021 --check tests/src/params.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/resource.rs wgpu-hal/src/dx11/adapter.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

## Checkpoint — query-gated R8/RG8/RGB565 copy and readback — `877c4e9bc`

Commit `877c4e9bc` adds the narrow-format copy-source paths and variable-width
texture readback. FL9_x GPU-to-CPU copies require an internal native
render-target binding even when the public texture is not a render target. The
HAL now admits `COPY_SRC` for R8_UNORM, R8G8_UNORM and B5G6R5 only when both
the backend path and selected device's `RENDER_TARGET` query are positive. HAL
texture creation adds the native RTV bind for this use. RGB5A1, RGBA4, the
SNORM formats and BC1 remain excluded. The controlled core fixture derives
these usage reports from the HAL projection.

The texture-to-buffer path now uses each format's block layout rather than a
hard-coded four bytes per texel. It checks block alignment and copies mapped
rows using the D3D11 staging texture's actual `RowPitch`. Existing checked
offset, pitch and destination-range handling is retained. A CPU test covers
R8, RG8 and RGB565 layouts with 256-byte rows and a nonzero offset.

The new strict public test explicitly selected the physical NVIDIA RTX 3090
Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested FL9_1,
and confirmed actual FL9_1. At that level, the selected-device masks were
R8=`0x0002d3e0`, RG8=`0x0002c320`, and B5G6R5=`0x0026f3a0`. For each format,
the test uploaded padded 5×3 source rows, performed a partial GPU texture copy
from a nonzero source origin into a differently sized 6×4 destination at a
nonzero destination origin, then read the complete destination into a buffer
at offset 16 with 256-byte rows. It verified all 24 texels (six copied and 18
zero-initialized/preserved) plus padding sentinels. All three formats passed.
The synthetic capability test rejects the copy-source path when the
render-target query is missing or the backend implementation switch is false.

Verification used Rust 1.93.1 in the Visual Studio 2022 x64 developer
environment, with `-j32` and strict hardware mode for GPU suites:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 check --offline -j32 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-hal --lib --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --lib --tests --no-default-features --features dx11-test-fixture,std,wgsl
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs wgpu/tests/dx11_public.rs
git diff --check
```

Results: strict HAL 359/359, public D3D11 26/26, controlled core fixture
1/1 (89 filtered), physical public adapter selection 1/1 (86 filtered), and
core DX12 86/86. The full public suite logged one optional Float16x2 input
format skip because the selected-device query does not report it; no
hardware-required test skipped for missing hardware. The HAL suite contains
expected negative shader cases that reject before GPU execution and log their
GPU-only subcase as skipped. HAL Clippy passed with nine existing unrelated
warnings, core Clippy with one existing warning, and public Clippy without
warnings. Changed-file rustfmt, HAL check and `git diff --check` passed. A
direct Cargo invocation outside the VS developer shell initially failed to
find `link.exe`; rerunning within `VsDevCmd.bat` passed. The first full HAL
run after implementation exposed two stale synthetic expectations; they were
updated to distinguish RGB565's implemented path and the suite then passed.

An independent read-only review found no concrete mismatch in the new path.
It confirmed the FL9_x render-target bind requirement, query/implementation
gating, checked transfer layout and use of native mapped row pitch. Historical
FL9_1 behavior and ARM32 UWP remain unverified. Continue the public core/HAL
format and operation audit.

Continue by auditing whether every projected numeric limit, per-format usage
and accepted core command is implemented by the D3D11 path. Public D3D11
enumeration and public HWND surface exposure remain gated; the forced RTX
result does not validate historical FL9_1 drivers.

## Checkpoint — vertex system-value input gates — `d85e51e7b`

Core now has `DownlevelLimits::vertex_index_input` and
`instance_index_input`. The controlled FL9_1 fixture reports both false and
rejects selected vertex entry points declaring either input before HAL; a
safe sibling entry remains usable. HAL already rejects both inputs. The
existing D3DCompile test confirms backend-generated `vertex_index` entry
interfaces fail under `vs_4_0_level_9_1`, whether the WGSL value is used or
unused. Its diagnostic is FXC X4555 for `SV_VertexID`; this establishes the
current generated-HLSL/profile boundary, not a universal hardware limitation.
The `instance_index` FXC boundary remains unverified and is documented only
as a backend restriction. Normal D3D11 enumeration remains disabled.

Verification used the pinned `1.93` toolchain (rustc 1.93.1) and MSVC. The
explicit `+1.93.1` alias is installed without a rustup manifest, so it failed
before compilation; `+1.93` is the repository-pinned alias. Visual Studio's
developer prompt printed `Error: Unknown error` in its banner, but all MSVC
commands below completed:

- wgpu-types full library suite: 64/64 passed. Clippy passed with 14 existing
  unused-serde-import warnings.
- Fixture-enabled core suite with `WGPU_DX11_REQUIRE_HARDWARE=1`: 78/78
  passed, including the real hardware fixture and zero-adapter isolation.
  The fixture selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
  software=false), requested FL9_1 and selected L9_1. The new gate logged its
  selected-entry rejection and safe sibling acceptance.
- Private D3D11 HAL suite with strict hardware mode: 333/333 passed. The
  focused FXC test separately passed and reported X4555 for direct-used,
  direct-unused, structure-used and structure-unused `vertex_index` inputs.
- HAL zero-adapter isolation test: 1/1 passed.
- Core DX12 regression suite: 75/75 passed with `dx12,std,wgsl`.
- HAL `dx11,gles` check and Clippy passed; fixture-enabled core Clippy passed;
  `wgpu-info` check and `wgpu-test --tests` check passed.
- Targeted rustfmt and `git diff --check` passed.

Cargo ran inside the Visual Studio MSVC developer prompt with
`CARGO_BUILD_JOBS=4` (the full strict fixture used `2`). Reproduction commands:

```powershell
cargo +1.93 test --offline -j 4 -p wgpu-types --lib vertex_builtins_are_downlevel_capabilities -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std vertex_builtin_detection_uses_selected_entry_point_declarations -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_index_entry_inputs_are_rejected_by_fxc -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-hal --lib --no-default-features --features dx11 incomplete_instance_exposes_no_adapters_and_rejects_unsupported_handles -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-types --lib -- --test-threads=1
cargo +1.93 check --offline -j 4 -p wgpu-hal --no-default-features --features dx11,gles
cargo +1.93 check --offline -j 4 -p wgpu-info
cargo +1.93 check --offline -j 4 -p wgpu-test --tests
cargo +1.93 clippy --offline -j 4 -p wgpu-types --lib --tests
cargo +1.93 clippy --offline -j 4 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j 4 -p wgpu-hal --no-default-features --features dx11,gles
cargo +1.93 clippy --offline -j 4 -p wgpu-info
cargo +1.93 clippy --offline -j 4 -p wgpu-test --tests
rustfmt --edition 2021 --check tests/src/params.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/pipeline.rs wgpu-core/src/validation.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

`WGPU_DX11_REQUIRE_HARDWARE=1` was set for the full core fixture and HAL
suite. An initial DX12 command omitted `std,wgsl` and failed to compile the
test target; rerunning with those required features passed. No source change
was needed to correct that command issue.

## Documentation checkpoint — repeat-cap behavior reconciled — `e5fe9207c611234c1669da48892ec12b80ebcfab`

Updated `CAPABILITIES.md`, `TESTING.md` and `STATUS.md` to separate the private
HAL's conditional NPOT relationship checks from the core fixture's finite
FL9_1 repeat limit. The fixture rejects Repeat/MirrorRepeat sampler creation
at the documented limit of 128; synthetic core tests still cover selected-
entry NPOT relationship validation where those sampler modes are permitted.
This documentation-only checkpoint records the implementation in
`70e31a0f175803ced62134e10cd3f11cc8553886`.

## Checkpoint — selected fragment `front_facing` gate — `77ac0b78330c4ba330682c89526daa2833f2b1c6`

Core now represents whether fragment entry points may declare
`@builtin(front_facing)` in `DownlevelLimits::fragment_front_facing`. The
controlled FL9_1 profile sets this false because the tested
`ps_4_0_level_9_1` FXC profile rejects the input. Core inspects the selected
fragment entry point and rejects the incompatible pipeline before HAL, while a
safe sibling entry point in the same module remains usable. This is documented
as a compiler/backend limitation, not a fundamental hardware restriction.
Ordinary D3D11 adapter enumeration remains disabled.

Verification used Rust 1.93.1/MSVC and `CARGO_BUILD_JOBS=32`:

- `cargo test --offline -p wgpu-types --lib -- --test-threads=1`: 62/62 passed.
- CPU interface selection test passed; the strict fixture-enabled core suite
  passed 76/76 with `WGPU_DX11_REQUIRE_HARDWARE=1`. The focused fixture log
  identified NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false),
  requested/actual FL9_1, and logged the selected-entry rejection and safe
  sibling-pipeline success. Normal adapter isolation passed in the suite.
- Strict private HAL suite: 333/333 passed. The pre-existing FXC negative test
  confirmed the front-facing shader diagnostic; strict mode prevented hardware
  cases from silently skipping.
- Core DX12 regression: 73/73 passed.
- HAL `dx11,gles`, `wgpu-info --all-targets`, and `wgpu-test --all-targets`
  checks passed. Clippy passed for wgpu-types, fixture-enabled core, HAL
  `dx11,gles`, and wgpu-info.
- Targeted rustfmt and `git diff --check` passed. Workspace-wide
  `cargo fmt --all -- --check` reports pre-existing formatting differences in
  `naga/src/back/hlsl/writer.rs` and `wgpu-core/src/instance.rs`; neither file
  is part of this checkpoint and neither was reformatted.

The Visual Studio setup banner still prints `Error: Unknown error`, but all
MSVC-backed commands completed. This checkpoint narrows an observed shader
profile gap; it does not complete public adapter validation. Continue the
selected-entry shader and render contract audit, then build a capability
snapshot without enabling ordinary enumeration.

## Checkpoint — controlled core fixture honors FL9_1 repeat cap — `70e31a0f175803ced62134e10cd3f11cc8553886`

The controlled core fixtures now derive `max_texture_repeat_per_primitive`
from the selected D3D feature level instead of setting `u32::MAX`. Forced
FL9_1 reports the documented finite limit of 128, and the strict core fixture
verifies that both Repeat and MirrorRepeat samplers fail at creation with
`TextureRepeatLimitExceeded`. Its texture readback case now samples POT and
NPOT resources using clamp-to-edge. Private HAL tests retain bounded repeat
coverage; synthetic core tests retain coverage for conditional NPOT
relationship validation where repeat samplers are permitted. This fixture is
still a reduced test profile, not a public capability snapshot.

Verification used Rust 1.93.1/MSVC and `CARGO_BUILD_JOBS=32`:

- Strict focused core fixture with `--nocapture`: passed on NVIDIA GeForce RTX
  3090 Ti (`0x10de:0x2203`, software=false), requested and actual FL9_1. It
  logged `texture repeat limit: 128`, rejected both sampler modes, and matched
  all 64 pixels in each remaining render/readback case.
- Full strict fixture-enabled core suite: 75/75 passed, including normal
  D3D11 adapter isolation. The physical fixture ran with
  `WGPU_DX11_REQUIRE_HARDWARE=1`.
- HAL and fixture-enabled core Clippy passed. Targeted rustfmt and
  `git diff --check` passed.

The first compile attempt used an unqualified `resource` module in the new
fixture assertion; the path was corrected to `crate::resource` before the
passing runs. The RTX test does not demonstrate the historical repeat
overflow boundary. Public adapter enumeration remains disabled.

## Checkpoint — fractional FL9_1 viewport mapping — `95fb67e22`

The private HAL now keeps the logical fractional viewport values when building
the FL9_1 vertex remap. Its native D3D11 viewport uses integral bounds that
contain the clipped logical viewport, avoiding the 9_x runtime's DWORD-based
viewport conversion while preserving the requested screen transform. The
strict HAL readback test covers fractional x/y origins and extents. The
controlled core fixture also covers a fractional viewport with contained
geometry; it deliberately does not treat a viewport as a scissor, since an
oversized primitive can render outside the viewport rectangle. Core's
out-of-target viewport gate remains unchanged. Historical FL9_1 precision is
not established by the RTX test.

Verification used Rust 1.93.1/MSVC and `CARGO_BUILD_JOBS=32`:

- CPU viewport tests: `cargo test --offline -p wgpu-hal --lib
  --no-default-features --features dx11 fractional_viewport -- --nocapture
  --test-threads=1`: 3/3 passed.
- Strict private HAL test:
  `WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib
  --no-default-features --features dx11 fl91_viewport_fractional_coordinates
  -- --nocapture --test-threads=1`: passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1.
- Full strict private HAL suite: 333/333 passed. Hardware-dependent tests ran
  with strict mode enabled; shader-profile rejection tests validate compiler
  errors rather than GPU output.
- Strict core fixture: 75/75 passed. Its physical fixture test was separately
  run with `--nocapture`, reported the same RTX and requested/actual FL9_1,
  and matched all 64 pixels for `FractionalViewport` and the other render
  cases.
- Normal D3D11 zero-adapter isolation: 1/1 passed. Core DX12 regression suite:
  72/72 passed with `dx12,std,wgsl`.
- HAL and fixture-enabled core Clippy passed. Targeted rustfmt and
  `git diff --check` passed.

The first core viewport test draft incorrectly expected the viewport to clip
an additional oversized triangle; it failed at pixel `(1, 0)`. The fixture was
corrected to use only contained geometry, after which strict core readback
passed. An initial DX12 command omitted the required `wgsl` feature and failed
to compile; the correctly configured run above passed. Visual Studio's setup
script printed `Error: Unknown error` in its banner, but MSVC-backed Cargo
builds and all listed tests completed successfully. Public D3D11 enumeration
and surface exposure remain disabled.

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
fixture kept its test-only unbounded setting so the prior NPOT draw test could
exercise selected-entry validation. That permissive setup was removed by
`70e31a0f1`; it was never a public capability projection.

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

## Checkpoint — conditional RGBA8/BGRA8 mip projection — pending code commit

The reusable per-format projection now includes `SINGLE_MIP_ONLY` for RGBA8
and BGRA8 when the selected device lacks `D3D11_FORMAT_SUPPORT_MIP`, or when
any backend path needed for the supported POT mip combinations is unavailable.
The controlled core fixture compares both flags against the actual query and
includes both formats in the pre-HAL multi-mip rejection loop. Ordinary D3D11
enumeration remains empty.

Verification used Rust 1.93.1, the Visual Studio developer shell, and
`CARGO_BUILD_JOBS=32`, with one Cargo process at a time:

- Synthetic `color_format_projection_limits_mips_to_positive_driver_and_backend_support`: 1/1 passed for both RGBA8 and BGRA8, testing absent/present MIP bits and a disabled backend mip path.
- Strict fixture-enabled core suite: 75/75 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. RGBA8 and BGRA8 both reported driver MIP support and `single_mip_only=false`; existing mip-chain transfer/render/readback cases passed. The fixture also verified ordinary D3D11 enumeration remains empty.
- After adding the explicit query diagnostic, the focused strict hardware fixture passed 1/1 and printed the same query/projection values.
- Full strict private HAL suite: 330/330 passed, including GPU execution cases; core DX12 regression: 72/72 passed.
- HAL and core fixture Clippy passed; HAL retained ten pre-existing warnings in untouched code. Targeted rustfmt and `git diff --check` passed.

Exact commands:

```text
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 color_format_projection -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs
git diff --check
```

The negative no-MIP case is synthetic; the RTX cannot validate a driver that
does not expose that format bit. This modern forced-FL9_1 result does not
substitute for historical hardware. Next address the audited core/HAL mismatch
for nonzero depth bias with a selected fragment entry point that writes
`frag_depth`, using entry-point-specific validation and keeping adapter
enumeration closed.

## Checkpoint — selected-entry depth-bias validation — `8f81fdc8f`

Core now represents whether depth bias can be combined with a selected fragment
entry point writing `frag_depth` as
`DownlevelLimits::depth_bias_with_fragment_depth`. The controlled FL9_1 fixture
sets it false. A new `Interface` query reuses Naga-derived selected-entry
outputs; core rejects the unsupported combination before HAL and treats an
opaque shader interface conservatively. The private D3D11 capability model
already records this backend path as unavailable. Ordinary D3D11 adapter
enumeration remains empty.

The strict fixture created a WGSL module with both a color-only and a
color-plus-depth fragment entry. It verified: nonzero bias plus selected depth
output returns the specific core validation error; selecting the color entry
from that same module creates a biased pipeline; selecting the depth entry with
zero bias creates a pipeline. These compile through the actual D3D11 HAL on
physical hardware, but do not execute a depth-writing draw.

Verification used Rust 1.93.1 in the Visual Studio developer shell with
`CARGO_BUILD_JOBS=32`, one Cargo process at a time:

- `wgpu-types` full library tests: 61/61 passed; 14 existing unused-serde-import warnings.
- Strict fixture-enabled core suite: 75/75 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. Existing rendering/readback assertions passed; the new pipeline selection checks passed. The fixture also asserts ordinary D3D11 enumeration remains empty.
- Focused strict fixture rerun after correcting the case to use triangle topology: 1/1 passed. The initial test attempt ran inside the helper's point-list case and correctly failed the pre-existing depth-bias/topology validation; the test was scoped to the baseline triangle case and then passed.
- Core's zero-D3D11-adapter isolation test: 1/1 passed.
- Core DX12 regression: 72/72 passed.
- Strict private D3D11 HAL suite: 330/330 passed on the RTX 3090 Ti at requested/actual FL9_1, including actual GPU readback cases. A small set of shader tests intentionally logs `SKIP` for rendering after verifying expected safe shader rejection; these do not count as executed GPU draws.
- HAL `dx11,gles` check, `wgpu-info` check and `wgpu-test --tests` check passed.
- Core fixture Clippy, wgpu-types Clippy, wgpu-info Clippy and HAL `dx11,gles` Clippy passed. HAL retained ten existing warnings in untouched code; wgpu-types retained fourteen existing unused-serde-import warnings.
- Targeted rustfmt and `git diff --check` passed.

Exact commands:

```text
cargo test --offline -p wgpu-types --lib -- --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-info
cargo check --offline -p wgpu-test --tests
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo clippy --offline -p wgpu-types --tests
cargo clippy --offline -p wgpu-info
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
rustfmt --edition 2021 --check wgpu-types/src/limits.rs wgpu-core/src/validation.rs wgpu-core/src/device/resource.rs wgpu-core/src/pipeline.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs tests/src/params.rs
git diff --check
```

The RTX result is a compiler/validation check at forced FL9_1, not a
depth-writing GPU render or a historical-driver claim. Next continue the
public adapter contract audit; the public capability snapshot and HWND
presentation gates remain unresolved.

## Checkpoint — FL9_1 fragment position validation

Core now exposes `DownlevelLimits::fragment_position` and rejects a selected
fragment entry point that actually consumes `@builtin(position)` when the
capability is unavailable. The controlled FL9_1 fixture sets it to false.
Unlike `front_facing`, FXC strips an unused `SV_POSITION` input, so an unused
declaration and a safe sibling entry point remain valid. Naga validated
function expression refcounts are used to detect direct and structure-member
reads; helper arguments are traced recursively through the validated call
graph, including helpers that only forward a structure or vector to another
helper. Opaque shader interfaces are rejected conservatively. The capability
does not change normal D3D11 adapter enumeration.

Verification used Rust 1.93.1/MSVC and a serial Cargo build while a separate
Horizon build was consuming system memory:

- Fragment-position analysis CPU test: 1/1, including direct, structured,
  helper, forwarded helper, unused and sibling-entry cases.
- Strict controlled core suite: 77/77 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. Its
  offscreen fixture performed the existing FL9_1 rendering/readback cases and
  verified used-position rejection and unused-position acceptance.
- `wgpu-types` library tests: 63/63; 14 pre-existing unused serde-import
  warnings.
- Core zero-D3D11-adapter isolation: 1/1.
- Core DX12 regression: 74/74.
- `cargo check` for HAL `dx11,gles`, `wgpu-info`, and `wgpu-test --tests`
  passed.
- Core Clippy passed with two pre-existing `op_ref` warnings in
  `dx11_fixture_tests.rs`.
- Targeted Rust formatting and `git diff --check` passed.

The strict fixture is one hardware test containing multiple actual draws and
pixel checks; it is not historical FL9_1 validation. The private HAL suite was
not rerun for this core-only validation change. Code commit: `e8c7240da`
(`core: gate FL9_1 fragment position use`). This progress entry, handoff and
capability documentation record the verified checkpoint. Next continue the
production adapter capability audit; ordinary D3D11 enumeration and public
HWND surface exposure remain gated.

## Checkpoint — reject single-sample alpha-to-coverage

Core now returns a validation error when `alpha_to_coverage_enabled` is true
with sample count one. This is the WebGPU multisample-state rule and prevents
the descriptor from reaching the private HAL, which only supports one-sample
pipelines and otherwise returned an unexpected HAL error. The controlled
fixture asserts the specific core error before continuing its GPU rendering.

Rust 1.93.1/MSVC verification:

- Strict fixture-enabled core suite: 78/78 passed. Its FL9_1 render/readback
  integration test ran on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
  software=false), requested FL9_1 and actual L9_1. The test exercised the
  new validation assertion and retained its existing checked GPU output.
- Core DX12 suite: 75/75 passed.
- Core Clippy passed.
- Targeted rustfmt check and `git diff --check` passed.
- The Visual Studio developer prompt printed its existing `Error: Unknown
  error` banner, but each build/test command exited successfully.

Commands, run from the MSVC developer prompt (`WGPU_DX11_REQUIRE_HARDWARE=1`
for the first command):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/pipeline.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The code change is limited to core validation, its controlled fixture
assertion, and the Unreleased validation changelog. The HAL was unchanged, so
its most recent full strict suite result remains the 337/337 recorded in the
capability-projection checkpoint. This does not enable D3D11 enumeration or
multisampling. Continue auditing the public core/HAL contract.

## Checkpoint — gate compute-stage bind-group layouts

Core now requires `DownlevelFlags::COMPUTE_SHADERS` for every bind-group-layout
entry whose visibility includes the compute stage, regardless of resource
type. Before this, core gated compute-visible storage buffers but let uniform,
sampler and sampled-texture layouts reach the FL9_1 HAL, which only accepts
vertex/fragment bindings. The controlled fixture verifies all three new
resource classes fail with the missing-compute-capability validation error;
the existing storage tests continue covering storage buffers.

Rust 1.93.1/MSVC verification:

- Strict fixture-enabled core suite: 78/78 passed. The RTX 3090 Ti
  (`0x10de:0x2203`, software=false) was requested and selected at FL9_1. The
  controlled integration case exercised core validation plus the existing
  checked GPU render/readback paths.
- Focused strict fixture: 1/1 passed.
- Core DX12 suite: 75/75 passed.
- Core Clippy passed.
- Targeted rustfmt and `git diff --check` passed.
- The ordinary D3D11 zero-adapter isolation test remains part of the fixture
  suite and passed; normal enumeration remains disabled.

Commands, run from the MSVC developer prompt (`WGPU_DX11_REQUIRE_HARDWARE=1`
for the two fixture commands):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The core validation gate is shared and leaves adapters with compute support
unchanged. No HAL code changed, so the last full strict HAL suite remains
337/337. Continue auditing valid core requests against the private HAL before
considering public enumeration.

## Checkpoint — gate comparison-sampler layouts

Core now requires `DownlevelFlags::COMPARISON_SAMPLERS` when creating a bind
group layout containing a comparison sampler. The HAL supports only filtering
and non-filtering samplers; before this check, core allowed the layout and the
private HAL returned an unexpected error. The controlled fixture now verifies
that comparison-layout rejection occurs in core, in addition to the existing
comparison-sampler object creation rejection.

Rust 1.93.1/MSVC verification:

- Strict fixture-enabled core suite: 78/78 passed. The RTX 3090 Ti
  (`0x10de:0x2203`, software=false) was requested and selected at FL9_1; its
  core fixture performed the existing checked render/readback work.
- Focused strict fixture: 1/1 passed.
- Core DX12 suite: 75/75 passed.
- Core Clippy passed.
- Targeted rustfmt and `git diff --check` passed.
- Ordinary D3D11 zero-adapter isolation remains covered by the passing core
  suite; enumeration remains disabled.

Commands, run from the MSVC developer prompt (`WGPU_DX11_REQUIRE_HARDWARE=1`
for the fixture commands):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

No HAL source changed. Public D3D11 enumeration and surface exposure remain
gated; continue auditing core-valid layout and render operations.

## Checkpoint — gate empty bind-group visibility

The FL9_1 projection now reports `DownlevelLimits::empty_binding_visibility =
false`, and core rejects a bind-group-layout entry with an empty shader-stage
set before calling the HAL. This closes another legal-WebGPU-layout/HAL-rejection
mismatch. The limit defaults to true for existing backends, participates in
limit checks and the WebGPU-compliance predicate, and has serde defaulting for
older serialized capability values. GLES and noop explicitly report support;
the private DX11 projection reports unsupported. `wgpu-info` prints the new
limit. The changelog records the new validation behavior.

Rust 1.93.1/MSVC checks:

- `wgpu-types` library tests passed 66/66; with serde passed 67/67.
- Strict fixture-enabled core suite passed 78/78. The RTX 3090 Ti
  (`0x10de:0x2203`, software=false) executed at requested/actual FL9_1; the
  fixture specifically verified empty visibility is rejected by core before
  HAL and retained the existing render/readback checks.
- Full private HAL suite passed 337/337. Strict hardware mode produced 144
  physical-adapter reports across the suite, all selecting the RTX 3090 Ti at
  FL9_1. The suite reported no skipped or failed cases.
- Core DX12 suite passed 75/75. The ordinary D3D11 zero-adapter isolation test
  passed 1/1.
- HAL test compilation with `dx11,gles`, `wgpu-info` check, `wgpu-test` test
  check, and core fixture Clippy passed.
- Targeted rustfmt and `git diff --check` passed. Whole-workspace
  `cargo fmt --all -- --check` reports pre-existing differences in untouched
  `naga/src/back/hlsl/writer.rs` and `wgpu-core/src/instance.rs`.
- The Visual Studio developer prompt continues to print its existing
  `Error: Unknown error` banner while the invoked commands complete normally.

Commands (run from the Visual Studio developer prompt; strict commands set
`WGPU_DX11_REQUIRE_HARDWARE=1`):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib
cargo +1.93 test --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check tests/src/params.rs wgpu-core/src/binding_model.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

Public D3D11 adapter enumeration and HWND surface exposure remain disabled.
After this checkpoint, continue the core/HAL layout and rendering audit and
finish the broader capability projection; these individual gates do not make
the production adapter ready for enumeration.

## Checkpoint — gate unsupported sampled texture layout types and samples

The D3D11 projection now reports depth, signed-integer, unsigned-integer and
multisampled texture bindings as unavailable. Core rejects these BGL entries
before HAL, whose private layout mapper currently represents only single-sample
float sampled textures. Core also rejects vertex-only texture or sampler
layouts when the projected per-vertex sampled-texture limit is zero.
Fragment-visible resources with additional vertex visibility remain accepted
because the HAL can represent that layout shape; actual vertex-stage sampling
continues to fail at selected-entry pipeline validation.

Rust 1.93.1/MSVC verification:

- `wgpu-types` library tests passed 68/68; with serde passed 70/70. The serde
  test removes all newly added binding-capability fields from an old-shape
  serialized value and verifies their compatibility defaults.
- Strict fixture-enabled core suite passed 78/78 on the RTX 3090 Ti
  (`0x10de:0x2203`, software=false), forced and confirmed FL9_1. It exercised
  core rejection for depth, signed/unsigned integer, unfilterable multisampled
  and vertex-only sampled layouts, verified shared fragment/vertex visibility
  remains accepted, then completed existing textured depth renders and
  readback.
- Core DX12 suite passed 75/75.
- Strict physical D3D11 projection tests passed 4/4; the hardware case reported
  RTX 3090 Ti, vendor `0x10de`, device `0x2203`, software=false, requested
  FL9_1 and actual `L9_1`.
- Full private HAL suite passed 337/337. Strict mode logged 144 physical
  adapter identities and 101 GPU assertion lines. Six nested render checks
  explicitly skipped drawing because their unsupported dynamic-indexing shader
  was rejected at fragment linkage; they passed only the expected rejection
  checks and are not counted as GPU-rendered subcases.
- HAL test compilation with `dx11,gles`, `wgpu-info` check, `wgpu-test` test
  check, core fixture Clippy, targeted rustfmt and `git diff --check` passed.
- The Visual Studio developer prompt continues to print its existing
  `Error: Unknown error` banner while commands complete normally.

Commands run from the Visual Studio developer prompt. The core fixture, HAL
projection and full HAL commands set `WGPU_DX11_REQUIRE_HARDWARE=1`:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib
cargo +1.93 test --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check tests/src/params.rs wgpu-core/src/binding_model.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

Normal D3D11 adapter enumeration and HWND surface exposure remain disabled.
The next audit should continue with other core-valid layout and render shapes
that the HAL rejects, followed by the production capability projection.

## Checkpoint — pipeline override constants

The private D3D11 pipeline no longer rejects nonempty `ProgrammableStage`
constant maps. Shader compilation now resolves overrides with Naga's
`process_overrides` for the selected stage and entry point before entry-point
analysis, resource mapping, HLSL generation and FXC compilation. The ordinary
and viewport-remapped vertex shader paths share the same values. Invalid and
unknown overrides return the existing stage-specific pipeline-constant error.

The strict controlled core fixture rendered explicit fragment and vertex
overrides (`tint = 0.75`, readback red 191) and WGSL defaults (`tint = 0.25`,
readback red 64); each expected value appeared in all 64 checked pixels.
Unknown overrides were rejected with the corresponding fragment or vertex
pipeline-constant validation error. This also verifies that override
resolution precedes the private backend's selected-entry safety checks, rather
than treating `Expression::Override` as an unsafe runtime expression.

Rust 1.93.1/MSVC verification on 2026-11-04:

- Strict private D3D11 HAL suite: 337/337 passed with
  `WGPU_DX11_REQUIRE_HARDWARE=1`; real hardware cases explicitly selected the
  NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false) at requested and
  actual FL9_1.
- Strict controlled core suite: 78/78 passed. The core fixture used the same
  RTX 3090 Ti at requested and actual FL9_1 and performed the override
  readbacks above.
- Core DX12 regression suite: 75/75 passed.
- Ordinary D3D11 zero-adapter isolation test: 1/1 passed.
- HAL test compilation with `dx11,gles`, core Clippy, HAL Clippy, targeted
  rustfmt and `git diff --check` passed.
- Visual Studio's developer prompt printed its existing `Error: Unknown
  error` banner; all listed commands exited successfully.
- The vertex-override extension reran the strict controlled core suite at
  78/78 and core Clippy. Its first targeted rustfmt check found two formatting
  differences in the new assertions; formatting was applied to the changed
  fixture file and the targeted check then passed. No functional test failed.

Commands (run after entering the Visual Studio developer environment; strict
hardware commands set `WGPU_DX11_REQUIRE_HARDWARE=1`):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/hal_shader.rs
git diff --check
```

Code changes are limited to `hal_shader.rs`, `hal_pipeline.rs` and the
controlled core fixture. Normal D3D11 adapter enumeration and public HWND
surface exposure remain disabled. Existing GPU tests already cover nested
uniform structures, an asymmetric matrix, stride-sensitive arrays and bounded
ranges. Runtime integer indexing remains unavailable. Continue the core/HAL
contract audit and production capability projection.

## Current checkpoint — strict shader audit and unchecked runtime indexing

The FL9_1 shader-module compiler test now fails instead of silently returning
success when `WGPU_DX11_REQUIRE_HARDWARE=1` and no physical adapter is found. It
prints the selected physical adapter and asserts the actual device level is
FL9_1. The existing dynamic uniform-indexing case now runs under both checked
and unchecked `ShaderRuntimeChecks`: literal indexing compiles in each mode,
while runtime integer indexing is rejected in each mode by the backend's
unconditional FL9_1 safety guard. This test creates native shader objects; it
does not issue a draw or count as rendering evidence.

On 2026-11-04, the focused strict test passed 1/1 using the NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, software=false), requested FL9_1 and actual FL9_1.
HAL Clippy, targeted rustfmt and `git diff --check` passed. The Visual Studio
developer prompt emitted its existing `Error: Unknown error` banner, but the
commands exited successfully. Exact test command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE='1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 hal_module_compiles_and_creates_level_9_shaders -- --nocapture --test-threads=1
```

The next reviewed mismatch is sampler anisotropy: core models the downlevel
flag as support up to 16, while the FL9_1 HAL accepts at most 2. A truthful
numeric downlevel limit is needed so core can clamp requests consistently
before HAL. Public D3D11 enumeration and HWND surface exposure remain gated.
Code checkpoint: `3e725d3f4` (`dx11: verify unchecked FL9_1 shader indexing rejection`).

## Checkpoint — numeric sampler-anisotropy capability

Core previously treated `DownlevelFlags::ANISOTROPIC_FILTERING` as permission to
pass every requested anisotropy value up to 16. The private FL9_1 HAL can create
only values through 2. `DownlevelLimits::max_sampler_anisotropy` now expresses
the numeric cap separately from the availability flag; core clamps to the
reported cap (and still sends 1 when the flag is absent). The cap is not a
WebGPU-required limit because WebGPU permits implementations to ignore
anisotropy. Old serde payloads default the new field to 16.

The FL9_1 wgpu projection reports cap 2 and retains the anisotropy flag. The
controlled core fixture created real D3D11 sampler states from requests 2, 3
and 16; the latter requests reached the HAL clamped to the legal maximum. The
single-mip test image cannot establish anisotropic quality on minified content.
Vulkan now reports its selected device maximum capped at 16, GLES reports its
queried maximum capped at 16, and the noop backend uses 16. `wgpu-info` prints
the limit. Normal D3D11 adapter enumeration remains empty.

Rust 1.93.1/MSVC verification on 2026-11-04:

- `wgpu-types` full library suite: 69/69; serde suite: 71/71, including the
  old-shape default for the added field.
- Core anisotropy clamp unit tests: 2/2.
- Strict controlled core suite: 80/80 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. The fixture
  reported cap 2 and successfully created states for requested clamps 2, 3 and
  16. Full readback and render cases in the fixture also passed.
- Strict private D3D11 HAL suite: 337/337. It reported the same physical GPU
  and forced feature level; sampler translation and rendered sampler cases
  remained green.
- Core DX12 regression suite: 77/77; D3D11 zero-adapter isolation: 1/1.
- HAL test compilation with `dx11,gles,vulkan`, `wgpu-info` check, `wgpu-test`
  test check and Clippy for wgpu-types, fixture-enabled wgpu-core, D3D11 HAL,
  and GLES/Vulkan HAL passed.
- Targeted rustfmt and `git diff --check` passed. The serde types build retains
  fourteen pre-existing unused-serde-import warnings. Visual Studio's developer
  prompt still prints its existing `Error: Unknown error` banner.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib
cargo +1.93 test --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampler_anisotropy_limit -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE='1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE='1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles,vulkan
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-types --lib
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features gles,vulkan
```

A first compile exposed one missing test constant field and a Vulkan query typo;
both were corrected and the complete checks above then passed. An initial core
Clippy run pointed out a clamp idiom, which was corrected and Clippy rerun clean.
The next work remains the core/HAL capability audit and a truthful production
adapter contract. HWND `Present` reentrancy remains a separately documented
blocker; public D3D11 enumeration and surface exposure stay disabled.
Code checkpoint: `0b38756c1` (`core: honor downlevel sampler anisotropy limits`).

## Checkpoint — reject selected-entry texture loads in both runtime-check modes

The private FL9_1 shader compiler now rejects a reachable Naga `ImageLoad`
unconditionally. Previously this fail-closed restriction applied only when
shader runtime checks were enabled, despite the unsupported-operation comment
and FXC's existing rejection evidence. The native bounds proof remains an
isolated experiment and does not establish production texture-load semantics.
The hardware shader-module test verifies the selected load entry is rejected
and a safe sibling entry still compiles with both checked and unchecked runtime
checks.

Verification used the Rust 1.93.1 Visual Studio x64 environment. The focused
strict shader test passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
software=false), with requested and actual FL9_1. This checks shader compilation
and shader-object creation, not a texture-load draw. The complete strict
private D3D11 suite passed 337/337 with `WGPU_DX11_REQUIRE_HARDWARE=1`,
including its existing rendering/readback cases. The Visual Studio prompt
continues to print its known `Error: Unknown error` startup banner.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 hal_module_compiles_and_creates_level_9_shaders -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
```

This does not remove the experimental texel-load proof and does not enable
public D3D11 enumeration. The remaining mission gates are the core/HAL contract
audit, production capability projection, device loss/lifetime review and the
documented HWND Present reentrancy blocker.

## Checkpoint — complete the projected POT mip-usage matrix

The core/HAL audit found that format projection could permit POT RGBA8/BGRA8
mips for `RESOURCE`-only, `COPY_SRC`-only and `RESOURCE | COPY_SRC` textures,
while the private HAL rejected those usage shapes. Since format capabilities do
not carry usage-specific mip ceilings, the HAL now accepts every nonempty
combination of its four projected uses (`RESOURCE`, `COLOR_TARGET`, `COPY_SRC`,
`COPY_DST`). Backend mip-family switches and the actual MIP, shader-sample and
render-target format queries still gate the request.

CPU capability tests cover all fifteen nonempty usage combinations and retain
shape, backend-family, and driver-query rejection coverage. The strict FL9_1
sampled-mip test passed 74 GPU cases, zero skips, on NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, software=false). New hardware cases sampled zero-initialized
`RESOURCE` and `RESOURCE | COPY_SRC` mip chains into 64 transparent pixels per
format, and copied/read back a 256-byte zeroed `COPY_SRC`-only mip subresource
for RGBA8 and BGRA8. The strict controlled core fixture successfully created
all three formerly mismatched descriptors through the projected adapter path.
The complete strict private HAL suite passed 337/337. No historical FL9 GPU
claim is made.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 dx11::capabilities::tests:: -- --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_sampled_pot_mip_chains -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

Ordinary D3D11 enumeration remains disabled. The production adapter review,
remaining core/HAL audit, lifetime/device-loss review, and HWND Present
reentrancy blocker remain open.

## Checkpoint — request a core device from the unmodified FL9_1 projection

The controlled fixture now has a second route that uses the selected physical
device's exact D3D11 capability projection without test overrides. It builds a
test-only `ExposedAdapter` from the selected RTX 3090 Ti device, confirms the
normal HAL enumeration remains empty, asks core for a device using the exact
projected features and limits, and creates each of the 15 nonempty RGBA8 and
BGRA8 usage subsets advertised by the projection. It creates sampled and
render-target views where those usages apply, plus the projected R8_UNORM,
R8G8_SNORM, R8G8B8A8_SNORM and D24S8 resource/view shapes. This is a
test-injected, non-enumerated adapter: it exercises core request and resource
validation against the unmodified projection, but does not enable production
adapter exposure or HWND surfaces.

On 2026-11-04, strict execution used NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, software=false), explicitly requested and confirmed FL9_1.
The new exact-projection test passed, and the complete strict controlled core
fixture passed 81/81 with no hardware skips. The existing full core fixture
render/readback cases also ran in that suite. Targeted DX12 core regression
passed 77/77, and the zero-D3D11-adapter isolation test passed 1/1. Core
Clippy passed with three warnings in pre-existing fixture code (a nested `if`
and two needless references); none point into the new test. Targeted rustfmt
and `git diff --check` passed. The developer prompt continues to emit its
existing `Error: Unknown error` banner.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The production capability report still needs a full audit against every
core-valid operation. Device-loss/lifetime review remains open, and HWND
`Present` lock/reentrancy still blocks public presentation and normal D3D11
adapter exposure.

## Checkpoint — keep core buffer padding within D3D11 `ByteWidth`

The exact selected-device projection audit found that `max_buffer_size =
u32::MAX` was not safe for the current core allocation path. Core rounds every
buffer up to `COPY_BUFFER_ALIGNMENT` and adds one sentinel byte to vertex
buffers before rounding. A legal request at the old reported maximum could
therefore reach the HAL as a 4 GiB `ByteWidth`, which cannot fit the native
32-bit field. The FL9_1 projection now caps the logical request at
`u32::MAX - COPY_BUFFER_ALIGNMENT`, the largest value for which both the
vertex sentinel path and ordinary four-byte rounding fit in `ByteWidth`. The
synthetic projection test and exact-device core fixture assert this value.

On 2026-11-04, all four HAL projection tests passed; the selected-device case
reported NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested
FL9_1, actual FL9_1. The strict core fixture passed 81/81 with no skips,
including the unmodified projection request and resource/view creation test as
well as existing GPU render/readback cases. HAL Clippy passed. Core test Clippy
passed with the same three warnings in pre-existing fixture code (one
collapsible `if` and two needless references); the new test produced no
warning. Targeted rustfmt passed. An initial invocation outside the Visual
Studio environment could not find `link.exe`; rerunning through the installed
VS 2022 developer environment succeeded. That environment continues to print
its known `Error: Unknown error` banner.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The limit is derived from core's allocation padding rather than a historical
GPU guarantee. Large successful allocations are still subject to memory
availability. Continue auditing the remaining core/HAL descriptor boundary,
device loss/lifetime behavior and the unimplemented production adapter path.

On 2026-11-05 the exact-projection core test was extended to request a vertex
buffer one byte above the reported maximum and assert core returns
`CreateBufferError::MaxBufferSize` before HAL allocation. The strict fixture
was rerun at 81/81 with no skips on the same RTX 3090 Ti, requested and actual
FL9_1. The DX12 core regression passed 77/77 and the zero-D3D11-adapter
isolation test passed 1/1 after the projection change. Core Clippy passed with
the same three pre-existing fixture warnings and no warning in the new test;
targeted formatting and `git diff --check` passed.

Additional commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
```

## Checkpoint — drain partial command replay failures (uncommitted)

The lifetime audit found that native replay may fail after earlier commands in
the same submission have already reached the immediate context. Core tracks
completion only after HAL submission succeeds, so returning immediately could
release the borrowed command buffers before that partial work completes. HAL
now ends and flushes an event query on replay failure, then synchronously waits
for that query before returning the original replay error. The failed submission
does not advance or signal the queue fence. A test-only encoder case records a
real buffer copy followed by an injected failure; the strict hardware test
checks the copied bytes after the error and verifies the fence value remains
unsignaled. If the event query itself fails, the wait error is returned; actual
device-removal recovery and retention after an unqueryable device remain open.

The exact-projection core fixture additionally verifies that the selected
FL9_1 projection rejects D1/D3 texture creation and D2Array/Cube/CubeArray view
creation in core, including a one-layer array view. It also rejects
DepthOnly/StencilOnly D24S8 render views before HAL. `CAPABILITIES.md` and
`STATUS.md` now accurately state that numeric 1D/3D limits remain 2048/256 while
the downlevel dimension booleans are false; the limits alone do not enable
those shapes.

Rust 1.93.1 / Visual Studio x64 checks on 2026-11-05, run locally on
`Ben-Desktop` from `C:\dev\personal\wgpu` with 32 Cargo jobs:

- Strict partial-replay test: 1/1 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. A real GPU
  buffer copy executed before the injected replay error; its output was intact
  after return and the failed submission did not signal its fence.
- Strict private D3D11 HAL suite: 337/337 Rust tests, zero failures. The suite
  exercised hardware rendering, transfers, fences, readback, texture formats,
  and lifetime cases on the same physical GPU. Several unsupported shader
  experiments explicitly skipped their draw phase after expected compiler
  rejection; those skips are not GPU passes.
- Strict controlled core fixture: 81/81 tests, zero failures. The selected RTX
  3090 Ti ran at requested/actual FL9_1; core rendering/readback and projection
  validation executed, including the new pre-HAL shape rejections. Strict mode
  had no missing-hardware skips.
- DX12 core regression: 77/77. Ordinary D3D11 zero-adapter isolation: 1/1.
- HAL Clippy passed with nine existing warnings in untouched HAL tests; core
  Clippy passed with the same three pre-existing fixture warnings. The initial
  run identified two warnings in the new test code; both were fixed and HAL
  Clippy rerun clean of new warnings.
- Targeted rustfmt and `git diff --check` passed.

Commands (Visual Studio x64 developer environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 replay_failure_reaches_queue_and_does_not_signal_fence -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/fence.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The checkpoint was committed as `e32a29732e672cc96e9e2b9dbff3474425e9ae66`.
Continue the failure-path/device-loss audit and the truthful
production adapter work. Normal enumeration and surface capabilities remain
disabled; synchronous HWND `Present` reentrancy is still unresolved.

## Checkpoint — run multiple uniforms through the exact core projection

The private HAL already allocates thirteen uniform slots across all stages and
bind groups, but the internal FL9_1 projection exposed only one per shader
stage. The projection now exposes six per stage: the worst-case union of six
vertex-only and six fragment-only bindings is twelve, within the backend's
thirteen-slot cap. The two-stage count remains conservative and avoids
advertising a layout that the shared allocator would reject.

The controlled fixture adds two independent fragment uniform buffers at sparse
bindings 17 and 23. Each binds a nonzero 256-byte-aligned range with a distinct
native constant-buffer slot; both values multiply the sampled texture. Strict
RTX 3090 Ti readback at requested/actual FL9_1 matched all 64 pixels: red-side
pixels were `[64, 0, 0, 255]`, green-side pixels were `[0, 64, 0, 255]`.
The exact-projection core test also creates a seven-uniform fragment layout
and verifies core rejects it against the projected limit of six before HAL.
Ordinary D3D11 adapter enumeration remains empty.

Verification on 2026-11-05 used Rust 1.93.1 and the Visual Studio x64
environment on `Ben-Desktop` (`C:\dev\personal\wgpu`, 32 Cargo jobs):

- The strict core offscreen case passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested/actual FL9_1. Its two independent
  fragment uniforms changed the sampled output; all 64 pixels matched the
  red-side `[64, 0, 0, 255]` and green-side `[0, 64, 0, 255]` expectations.
- The exact selected-device projection test passed 1/1 and reported that core
  rejected seven fragment uniform bindings against the limit of six before
  HAL. This test requested a core device with the unmodified projection.
- The complete strict core fixture passed 81/81 with no missing-hardware skips;
  it included the two-uniform draw/readback. The exact-projection test above was
  run separately as well.
- HAL projection tests passed 4/4. The physical query reported the six-uniform
  stage cap on the forced FL9_1 RTX 3090 Ti; optional format and NPOT queries
  remained separate from implementation guarantees.
- The strict HAL suite passed 337/337. 148 test executions printed the
  selected physical adapter; six nested GPU draw experiments explicitly
  skipped rendering after expected FXC rejection, and are not GPU passes.
- DX12 core regression passed 77/77; ordinary D3D11 adapter isolation passed
  1/1.
- HAL and core Clippy passed. Existing warnings remain: nine HAL warnings in
  old tests/untouched code and three core fixture warnings; the new fixture
  introduced no lint warning. Targeted rustfmt and `git diff --check` passed.
- The first compile found a test-only `Option<u64>` versus `NonZeroU64`
  mismatch; the `BufferBinding` size was corrected. One test invocation
  outside the developer prompt could not find `link.exe`; rerunning through
  `VsDevCmd.bat` succeeded. The prompt still prints `Error: Unknown error`.

Commands (run inside the Visual Studio x64 developer environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

This capability increment was committed as
`0d86bcdd240fe98f61cfc085f250edcacbd41255`. Public adapter enumeration remains disabled.

## Checkpoint — exercise vertex and fragment uniforms through core

The controlled core renderer now includes a pipeline with sparse uniform
binding 17 visible only to the vertex stage and binding 23 visible only to the
fragment stage. Both use nonzero 256-byte-aligned ranges and distinct native
stage registers. The vertex value scales the sampled geometry horizontally;
the fragment value halves the sampled color. Readback checks all 64 pixels,
including the reduced coverage and tinted region. This case uses the selected
device's capability projection without overriding its uniform limits; the
fixture's existing conditional-NPOT override is unrelated.

Rust 1.93.1 / Visual Studio x64 checks on 2026-11-05, from
`C:\dev\personal\wgpu` on `Ben-Desktop`, with 16 Cargo jobs:

- The strict FL9_1 core renderer passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. The new
  vertex/fragment uniform case ran through core and D3D11 and verified all 64
  pixels. The 16 covered pixels matched red tinted by 0.5 (UNORM conversion
  accepted within one channel value); the remaining 48 pixels matched the
  blue clear color. The existing two-fragment-uniform test still passed.
- The complete controlled core fixture suite passed 81/81 with strict hardware
  mode enabled and no missing-hardware skip. Its single hardware fixture
  executed all contained transfer, validation and rendering cases.
- The exact selected-device projection test passed 1/1 and continued to reject
  seven fragment uniforms before HAL. Ordinary D3D11 adapter isolation passed
  1/1. DX12 core regression passed 77/77.
- Core Clippy passed with its three existing warnings at unrelated fixture
  lines. Targeted rustfmt and `git diff --check` passed.
- Initial assertions exposed a one-unit UNORM rounding difference and an
  incorrect assumption about the fixture's second triangle coverage. The
  final test uses a one-unit tolerance for the tinted channel and checks the
  reduced first-draw rectangle; no backend behavior was changed to mask either
  observation.

Commands (inside the Visual Studio x64 environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

Next continue auditing the exact core/HAL contract, with priority on resource
layout and render-pass combinations that remain core-legal but unsupported by
the private backend. The production adapter and public surface gates stay
closed; synchronous HWND `Present` reentrancy remains unresolved.

The stage-specific uniform fixture and its verification record were committed
as `2ae141279`.

## Checkpoint — verify two dynamic uniforms through core

The exact selected-device FL9_1 projection exposes two dynamic uniform
bindings per pipeline layout. The core fixture now binds logical binding 17
only to the vertex stage with base/dynamic offsets 256/256 and logical binding
23 only to the fragment stage with offsets 256/512. Each uniform references a
different aligned range. The vertex value scales geometry and the fragment
value tints a sampled texture, making both offset selections observable in the
same draw. The exact-projection test rejects three dynamic entries at
bind-group-layout creation with `DynamicUniformBuffers { limit: 2, count: 3 }`.
Normal D3D11 enumeration remains empty.

Verification on 2026-11-05 used Rust 1.93.1 and Visual Studio x64 on
`Ben-Desktop`, in `C:\dev\personal\wgpu`, with 16 Cargo jobs:

- Strict core rendering passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. The new
  two-dynamic-uniform draw passed through core, private HAL and D3D11; all 64
  pixels matched. Sixteen covered pixels were red tinted by 0.5 (UNORM
  tolerance one); the other 48 matched blue clear. The existing static
  two-uniform and stage-specific uniform draws also passed.
- The complete controlled core fixture suite passed 81/81 with strict hardware
  mode and no missing-device skip. The exact selected-projection test passed
  1/1, including the three-entry rejection. DX12 core regression passed 77/77,
  and ordinary D3D11 zero-adapter isolation passed 1/1.
- Core Clippy passed with the same three pre-existing fixture warnings and no
  new warnings. Targeted rustfmt and `git diff --check` passed.
- An initial negative test expected rejection at pipeline-layout creation,
  but core rejects the over-limit dynamic entries at bind-group-layout
  creation. The final test asserts that earlier, more precise validation point.

Commands (inside Visual Studio x64; strict mode is required for the GPU run):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The projection, core test and documentation changes were committed as
`8d19d14c9`. Next continue auditing core-legal pipeline/resource combinations
against the private HAL, and keep ordinary adapter and surface exposure
disabled.

## Checkpoint — execute a two-group uniform/texture pipeline through core

The controlled core fixture now has a `MultiGroupUniformBindings` case. It
creates a group-0 layout containing only a fragment-visible uniform at sparse
binding 17, and a group-1 layout containing a filtering sampler at binding 42
and sampled texture at binding 7. WGSL uses those exact logical identities;
the pipeline layout carries both group layouts, and the render pass binds the
two groups independently. The uniform halves the red channel of the texture.
Strict forced-FL9_1 readback checks the full 8x8 target: the left half is
`[128, 0, 0, 255]` within one UNORM unit, and the right half is
`[0, 255, 0, 255]`. This establishes one two-group core-to-HAL path, not every
combination or the full eight-group limit. Ordinary D3D11 enumeration remains
empty.

Verification on 2026-11-05 used Rust 1.93.1 and Visual Studio x64 on
`Ben-Desktop`, with `-j16` and strict hardware mode for D3D11 runs:

- Targeted core fixture passed 1/1 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. The new
  multi-group draw and all existing fixture cases rendered/read back as
  expected.
- Full controlled core fixture suite passed 81/81, including the real hardware
  render. The selected-device projection test passed 1/1 and rejected three
  dynamic uniforms against its projected limit of two. The zero-public-adapter
  isolation test passed 1/1.
- Full private D3D11 HAL suite passed 337/337 under strict mode. Some tests
  intentionally report nested shader-render skips after the expected FXC
  rejection; these are unsupported-shader negatives, not successful GPU
  executions. The suite's positive GPU cases reported the physical adapter and
  FL9_1.
- DX12 core regression passed 77/77. D3D11 HAL build passed. Core and HAL
  Clippy completed with existing warnings only: three core fixture warnings,
  seven `single_range_in_vec_init` test warnings in `hal_device.rs`, one
  `items_after_test_module` warning in `swapchain.rs`, and one `needless_borrow`
  warning in `fl9_texel_load.rs`. No warning points at the new fixture code.
- Rustfmt and `git diff --check` passed.

Commands (inside the Visual Studio x64 environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 build --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

## Checkpoint — opt-in public FL9_1 offscreen adapter

Commit `6d077bdee` exposes only physical DXGI adapters that successfully create
an explicitly requested FL9_1 device. Software adapters are filtered, no
higher-level fallback is attempted, and an adapter is returned only when the
selected-device capability projection exists. Adapter metadata reports the
name, PCI vendor/device IDs and the DXGI driver-package version when
`CheckInterfaceSupport(IDXGIDevice)` succeeds. The public feature set is empty,
the adapter reports a non-WebGPU-compliant downlevel contract, and
`surface_capabilities` remains `None`. No claim of a complete public adapter
contract or presentation support is made.

The strict public wgpu integration test selects the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), reports requested and actual FL9_1,
and excludes software adapters. Through ordinary public adapter enumeration it
requests two devices, uploads a two-color texture, binds sparse logical
bindings 7/42, draws indexed textured geometry with a D24S8 depth attachment,
and verifies the full 8x8 output: 32 red pixels and 32 green pixels. The second
device independently clears and reads back 64 green pixels. This verifies a
meaningful public path and sequential device opens over the adapter's shared
native D3D11 device/context; it is not the required complete 3D application
scene, a concurrent multi-device test, a full contract audit, or a historical
driver result.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio x64, bounded `-j16`,
strict hardware mode for DX11 runs:

- `cargo +1.93 build --offline -j16 -p wgpu-hal --no-default-features --features dx11` passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1; both public render/readback cases executed on the RTX at FL9_1.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1` passed 337/337. The suite includes CPU-only checks and strict physical-GPU tests; nested shader-render skips are expected negative FXC cases, not GPU-render passes.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1` passed 81/81, including physical forced-FL9_1 fixture rendering and ordinary public enumeration of the projected adapter.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl` passed 77/77.
- HAL, core and wgpu Clippy checks passed. Existing warnings remain: nine HAL test warnings (seven single-range-in-vector, one items-after-test-module, one needless-borrow) and three core fixture warnings (one collapsible-if, two op-ref); the new adapter/test code introduced none.
- Rustfmt `--check` on changed Rust files and `git diff --check` passed. No test skipped in the strict public test; ordinary runs print a recognizable SKIP when physical FL9_1 hardware is unavailable.

Exact commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 build --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/instance.rs wgpu-hal/src/dx11/adapter.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/device.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/instance.rs wgpu/tests/dx11_fixture.rs wgpu/tests/dx11_public.rs
git diff --check
```

The public projection and core/HAL operation audit remain incomplete. The
independent review found no confirmed core-valid route to the scanned unsupported
compute/query/cache/acceleration-structure panic paths, but this is not an
exhaustive audit. Continue auditing every advertised pipeline, resource,
mapping, pass and error path. Keep public surfaces disabled due to the
documented synchronous `Present` reentrancy deadlock. Historical FL9_1 drivers
remain untested.

## Checkpoint — public stage uniforms and two bind groups

Commit `a0f29c5ab` expands the ordinary public wgpu integration test. Group 0
contains a sparse vertex-visible transform uniform at binding 3 and a separate
fragment-visible tint uniform at binding 17. Group 1 contains a sampled
texture/sampler pair at bindings 7/42. Queue uploads initialize the uniforms
and texture; indexed textured geometry is transformed, depth-tested using
D24S8 and read back through public wgpu APIs. All 64 pixels matched: 32
tinted-red `[128, 0, 0, 255]` pixels (within one UNORM unit) and 32 green
`[0, 255, 0, 255]` pixels. The second device still independently cleared and
read back 64 green pixels.

Strict hardware execution on 2026-11-05 used Rust 1.93.1 and the Visual Studio
2022 x64 developer environment:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, requested and actual FL9_1, `software=false`.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed.

The first test invocation from a non-developer shell lacked `link.exe`; the
same test passed after invoking the installed Visual Studio x64 developer
environment. No GPU skip occurred. No HAL/core code changed in this increment,
so their last full verified counts remain the prior checkpoint (HAL 337/337,
core 81/81, DX12 core 77/77). Continue with the remaining exact core/HAL
capability contract audit; do not widen limits without coverage and keep public
surface support disabled until safe Present scheduling is established.

## Checkpoint — public transform update across submissions

The public integration now changes the vertex transform with
`Queue::write_buffer`, submits the same indexed textured/depth-tested pipeline
again, and verifies a second full readback. The initial image matched 32
tinted-red and 32 green pixels. The updated transform shifted the red quad by
two columns; the second image matched 16 blue clear pixels, 32 tinted-red
pixels and 16 green pixels. This confirms the selected public uniform binding
is updated between submissions and changes the rendered geometry while the
pipeline, texture/sampler bindings and target remain reused.

Verification on 2026-11-05 used Rust 1.93.1, Visual Studio 2022 x64 and
`WGPU_DX11_REQUIRE_HARDWARE=1`:

- `cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1, `software=false`.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- Targeted rustfmt and `git diff --check` passed.

The first attempt used an incorrect expected image for the translated triangle
and failed with a diagnostic showing the complete row values; the test's
expected image was corrected to the observed geometric coverage and then
passed. No backend code changed. Continue auditing selected-device projection
and public core/HAL combinations; retain offscreen-only exposure and the known
surface Present reentrancy blocker.

## Checkpoint — public dynamic uniform ranges

The public stage-uniform integration now declares both group-0 uniforms as
dynamic. The vertex transform binding references a 64-byte range in a 512-byte
buffer. After the first submitted render and readback, `Queue::write_buffer`
uploads a translated transform at byte offset 256; the next render selects
that aligned range using dynamic offsets `[256, 0]`. Tint remains in the
original range. This validates the adapter's projected two-dynamic-uniform
limit through public wgpu and confirms the updated subrange affects pixels.

Verification on 2026-11-05 used Rust 1.93.1, Visual Studio 2022 x64 and strict
hardware mode:

- Public D3D11 integration test passed 1/1 on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1, `software=false`. The first readback matched 32 tinted-red/32 green pixels; the dynamic-offset render matched 16 blue/32 tinted-red/16 green; a second device matched 64/64 green pixels.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std`, targeted rustfmt and `git diff --check` passed.

The focused core fixture already covered dynamic uniform offsets, and this
extends the positive evidence to ordinary public wgpu adapter/device/queue
calls. Invalid offset rejection and all dynamic range combinations remain
part of the wider contract audit. Public surfaces remain unavailable pending
safe Present scheduling.

## Checkpoint — public maximum bind-group position

The public integration now places sampled texture/sampler resources in group
7, the final slot of the adapter's reported eight-group limit, and reuses
logical binding numbers 3 and 17 already used by group-0 vertex/fragment
uniforms. The pipeline layout includes all eight positions with empty gaps.
This proves the selected D3D11 HAL/compiler mapping retains `(group, binding)`
identity and uses separate native uniform/texture/sampler namespaces for this
cross-group configuration.

Strict execution on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) passed 1/1 at requested and actual FL9_1, with
`software=false`. The initial readback matched 32 tinted-red/32 green pixels;
the queue-uploaded dynamic transform selected at offset 256 shifted the second
readback to 16 blue/32 tinted-red/16 green pixels; a second public device
matched 64/64 green pixels. Public wgpu Clippy, targeted rustfmt and diff
checks passed. The exact strict test command remains the public test command
in the preceding checkpoint.

This covers the maximum group index with one two-group resource configuration;
it does not prove every resource type or the eight-group aggregate resource
capacity. Continue the contract audit. The public adapter remains experimental
and offscreen-only pending broad operation coverage and safe Present scheduling.

## Checkpoint — concurrent public device submissions

Commit `e8f6fe5f2` replaces the sequential second-device smoke operation with
two concurrent public wgpu workloads. Each independently opened device records
a clear followed by texture-to-buffer copy, submits, maps and reads back its
own buffer from a scoped worker thread. The first device produced 64/64 red
pixels and the second produced 64/64 green pixels. This exercises the shared
native immediate-context mutex across separate public devices and confirms
submission, synchronization and readback remain ordered for both queues.

Strict test on 2026-11-05 passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), requested and actual FL9_1, `software=false`, with
`WGPU_DX11_REQUIRE_HARDWARE=1`. Exact command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

`cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std`, targeted rustfmt and `git diff --check` passed. No skip or newly introduced warning. This covers concurrent queue submissions on the same adapter/context; it does not establish public presentation concurrency or device-loss behavior. Continue the core/HAL contract audit and retain the offscreen surface gate.

## Checkpoint — public unsupported-resource validation

Commit `4efecaf6e` adds negative checks through the ordinary public adapter and
device. With a validation error scope around each operation, core rejects:

- D3 texture creation while the selected FL9_1 projection disables 3D textures.
- 4x RGBA8 render-target creation while the backend's sample-count support is one.
- `R32Float` sampled texture creation because that format is not in the selected implementation projection.
- A two-mip D24S8 render attachment under the single-mip FL9_x restriction.

Each captured error was `wgpu::Error::Validation`, demonstrating that the
public core boundary rejects these requests before they reach the private HAL.
The strict public FL9_1 test still passed 1/1 on the RTX 3090 Ti at requested
and actual FL9_1, `software=false`, including group-7 sampled resources,
dynamic uniform transform readback and concurrent clear/readback on two
devices. Clippy, targeted rustfmt and `git diff --check` passed. Exact command
and execution evidence are in the preceding concurrent-device checkpoint.

This is a focused negative set, not a complete per-format/usage audit. Keep
checking every core-valid resource and command against HAL before widening the
projection; public surfaces remain gated by Present scheduling.

## Checkpoint — public storage and array-layout rejection

Code checkpoint: `c5580577e` (`dx11: verify public FL9_1 storage and array rejection`).

The strict public integration test now also places a vertex-visible storage
buffer layout and a fragment-visible `D2Array` sampled-texture layout under
validation error scopes. Both return `wgpu::Error::Validation` through the
selected physical adapter, complementing the existing 3D, 4x-MSAA,
`R32Float`, and multi-mip D24S8 rejection cases. The valid indexed textured
depth-tested draw, sparse group-7 resource mapping, dynamic transform update,
and concurrent independent-device queue/readback workloads still execute.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64, strict
hardware mode:

- `$env:WGPU_DX11_REQUIRE_HARDWARE = '1'; cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`. All six unsupported requests were rejected by public core validation. Readback remained 32 tinted-red/32 green, then 16 blue/32 tinted-red/16 green after the dynamic transform; concurrent devices read 64/64 red and 64/64 green pixels.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed. Git printed only its existing LF-to-CRLF working-copy notice.

The Visual Studio developer-shell banner included `vError: Unknown error`;
the x64 environment was usable and Cargo completed successfully. This is
validation-boundary evidence, not a complete audit of all legal bind-group
entries. Continue checking public core operations that can pass current limits
and gates. The adapter is still offscreen-only, and window Present reentrancy
remains unresolved.

## Checkpoint — public depth-order rendering

Code checkpoint: `6390e4efe` (`dx11: verify public FL9_1 depth ordering`).

The public FL9_1 integration test now draws fully overlapping near and far
textured surfaces in both draw orders, using the existing public pipeline,
uniform groups, sampled texture, target and D24S8 attachment. The far surface
uses the sampled green texel and the near surface the sampled red texel. Each
of the 64 readback pixels remained tinted red for both orders, so the result
depends on depth comparison and writes rather than draw order. This upgrades
the public depth evidence beyond attachment creation and non-overlapping
geometry.

Verification on 2026-11-05 used strict hardware mode on the NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual
FL9_1, `software=false`:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1, including both depth orders, all six public validation rejections, dynamic uniform image update and concurrent two-device readback.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed with no warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed.

This is still an offscreen diagnostic, not a representative desktop
application. Public HWND presentation remains gated by the unresolved
cross-layer Present reentrancy and lock-order design. The prior strict HAL,
core and DX12 counts are documented above and were not rerun for this
public-test-only change.

## Checkpoint — public R8Unorm sampling

Code checkpoint: `b965940ff` (`dx11: verify public FL9_1 R8 sampling`).

A second public API integration test now asserts the adapter's selected-device
R8Unorm format report, creates a `TEXTURE_BINDING | COPY_DST` texture, uploads
two byte values with a padded source row, samples it with a non-filtering
sampler at sparse bindings 7 and 42, and renders into RGBA8. The adapter
reported `TextureUsages(COPY_DST | TEXTURE_BINDING)` and
`TextureFormatFeatureFlags(SINGLE_MIP_ONLY)`, correctly withholding linear
filtering. Readback verified all 64 pixels: the left 32 were
`[64, 0, 0, 255]` and the right 32 `[192, 0, 0, 255]`.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64, strict
hardware mode:

- `$env:WGPU_DX11_REQUIRE_HARDWARE = '1'; cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 2/2 with no skips on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`. The first case includes public core rejection checks, depth ordering in both draw orders, dynamic-uniform rendering and concurrent two-device readback; the second executes the R8 upload/sample/readback.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed.

The positive R8 path verifies public format projection through use. Other
sampled-only formats and their public shader/readback paths remain to be
audited; BC1 remains unavailable through public wgpu because its aggregate
compression feature is correctly withheld. HWND surface presentation remains
blocked on the separate Present reentrancy design.

## Checkpoint — public mapped-at-creation vertex upload

Code checkpoint: `8be26c25f` (`dx11: verify public mapped vertex initialization`).

The indexed textured public scene now creates its vertex buffer with
`mapped_at_creation`, fills the mapped range, and unmaps before command
recording. Its strict GPU render/readback verifies that core's initialization
staging copy reaches D3D11 vertex input correctly. The queue-based texture,
uniform, index and dynamic-range uploads remain in the same test, so this
extends public transfer coverage without replacing those paths.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64:

- Strict command from the R8Unorm checkpoint passed 2/2 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`, with no skips. The indexed depth-tested image and both explicit depth-order images matched; R8 upload/sample matched 32 `[64, 0, 0, 255]` and 32 `[192, 0, 0, 255]` pixels.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- Targeted rustfmt and `git diff --check` passed.

Mapped-at-creation is now verified through public vertex use. Arbitrary public
buffer mapping and GPU-write/read synchronization still require broader audit.

## Checkpoint — public textured 3D offscreen example

Commit `4c2b6237944c153963f91bf8d5ea590506a64a83`
(`dx11: add public FL9_1 offscreen cube example`) adds a
standalone public-API example. It enumerates the opt-in D3D11 backend's
physical adapter, verifies the explicitly selected FL9_1 identity, creates a
restricted public device, uploads a two-color texture and transform,
initializes indexed cube geometry through mapped-at-creation buffers, and
renders a depth-tested 3D scene to an offscreen RGBA8 target. It waits for
queue completion, maps the readback and validates every pixel against the two
texture colors or blue clear color. An optional argument saves the verified
image as PPM.

The first execution exposed a fixture error: generated indices were truncated
to 40 bytes although 36 Uint16 indices require 72 bytes. Core rejected the
draw before GPU execution. The index generator was corrected to append all
six indices for each of six faces, then strict execution passed.

Verification on 2026-11-05 used Rust 1.93.1 and the Visual Studio 2022 x64
developer environment. Exact command:

```powershell
cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
```

The NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
reported requested/actual FL9_1 and `software=false`. Readback validated 112
red pixels, 277 green pixels and 635 blue background pixels, 1,024 total.
Example-target `cargo +1.93 clippy --offline -j16 -p wgpu --examples
--no-default-features --features dx11,wgsl,std`, targeted rustfmt and
`git diff --check` passed. The Visual Studio shell printed its existing
`vError: Unknown error` banner, but the x64 build and execution completed.

This advances the public offscreen example requirement. It does not resolve
the safe HWND `Present` reentrancy/scheduling design, so public surface
capabilities remain unavailable.

## Checkpoint — accurate public-backend diagnostic

Commit `435a31c4d54fe4c565c95de722d3f21cc1aee051` updates the core instance
warning shown when D3D11 is explicitly enabled. The previous message claimed
there was no usable wgpu backend, which became inaccurate when the restricted
public FL9_1 offscreen adapter was added. The new warning describes its
experimental offscreen-only status without implying surface support.

Verification on 2026-11-05:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1` passed 1/1 and enumerated one physical RTX 3090 Ti FL9_1 adapter.
- `cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl`, targeted rustfmt and `git diff --check` passed.
- The strict public integration suite passed 2/2 on the same physical RTX at requested/actual FL9_1. It verified the indexed depth-tested public scene, six validation rejections, group-7 sampling, dynamic-uniform update, concurrent devices, and R8 sampled upload. The standalone example separately validated 1,024 pixels (112 red, 277 green, 635 blue clear).

An initial core test invocation omitted the crate's `std` and `wgsl` features and failed to compile test-only imports; rerunning with the feature set above succeeded. The MSVC developer prompt continues to print `vError: Unknown error` despite providing a working x64 toolchain.

The current core review remains focused on any core-valid operation that can
reach unsupported HAL behavior. The public adapter still reports no surface
capabilities; the known synchronous HWND `Present` reentrancy design remains
the presentation blocker.

## Checkpoint — selected FL9_1 texture-load validation

Commit `a3a6a177c` (`dx11: gate FL9_1 texture loads in core`) adds
`DownlevelLimits::texture_load`. Other backends keep the default enabled; the
D3D11 selected-device projection disables it. Core records Naga `ImageLoad`
usage per entry point, follows validated helper calls iteratively, and rejects
only the selected vertex or fragment entry that uses a texture load. The
pipeline error is a validation error and occurs before HAL compilation. A safe
sibling entry in the same shader module remains usable. The production HAL
restriction is unchanged; the optional native texel-load experiment remains
disabled. The public strict test/example future poller now waits using a
thread-unparking waker instead of panicking if an operation is pending.

Verification on 2026-11-05 used pinned Rust 1.93.1 and Visual Studio 2022 x64:

- `cargo +1.93 test --offline -j16 -p wgpu-types --lib texture_load_is_a_downlevel_capability` passed 1/1; the full wgpu-types library suite passed 70/70, and the old-shape serde compatibility test passed 1/1.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampled_texture_count_uses_the_selected_entry_point` passed 1/1. It covers direct and helper-reachable loads, unrelated sibling entries, stage selection and missing entry names.
- With `WGPU_DX11_REQUIRE_HARDWARE=1`, the full controlled core fixture suite passed 81/81 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1. The targeted rerun passed 1/1 and logged the selected-entry textureLoad rejection before HAL, then completed the fixture's rendering/readback cases.
- The strict public wgpu test passed 2/2 on the physical RTX at requested/actual FL9_1 with `software=false`. It printed the core rejection/safe-sibling pass; the public depth-tested render, dynamic uniform, transitions, concurrent-device reads and R8 sample checks passed.
- The standalone public cube example passed on the same device and validated 112 red, 277 green and 635 blue-clear pixels (1,024 total).
- The private D3D11 HAL suite passed 337/337 under strict hardware mode; the targeted DX12 core regression passed 77/77. The HAL/GLES/Vulkan check passed.
- Clippy passed for wgpu-types (with serde), wgpu-core fixture, HAL with DX11/GLES/Vulkan, and wgpu tests/examples. Pinned rustfmt on changed Rust files and `git diff --check` passed.

The first public-test invocation omitted the Visual Studio developer environment
and failed at link time (`link.exe` not found); rerunning through
`VsDevCmd.bat -arch=x64 -host_arch=x64` passed. The developer prompt continues
to print `vError: Unknown error` while providing the working x64 toolchain.
Git also reports its existing LF-to-CRLF working-copy warnings on edited Rust
files. Public presentation, a complete core/HAL contract audit and historical
FL9_1 hardware remain outstanding. Next, investigate the D3D11 HWND Present
path and core queue/context locking; do not expose surface capabilities unless
the reentrancy and surface lifecycle are safely resolved.

## Checkpoint — public FL9_1 stencil behavior

Commit `64894f9c8` (`dx11: test public FL9_1 stencil rendering`) adds a strict
public wgpu test for the implemented symmetric stencil subset. It clears the
D24S8 stencil plane, writes reference 1 with `Replace`, colors passing `Equal`
fragments green, then verifies an `Equal` draw with reference 0 cannot replace
that result. All 64 readback pixels were `[0, 255, 0, 255]`. The test also
confirms that an asymmetric front/back stencil pipeline is rejected through a
public validation error before HAL. `CAPABILITIES.md`, `TESTING.md` and
`STATUS.md` record this coverage.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 3/3 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`. This includes the public indexed textured/depth-tested draw, dynamic uniform update, two-device queue submissions, R8 sampled upload, and the new stencil readback/rejection checks.
- A first compile caught that `VertexState::buffers` requires `Option<VertexBufferLayout>`; the test fixture was corrected and the focused strict stencil case passed 1/1 before the complete 3/3 rerun.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std`, rustfmt on `wgpu/tests/dx11_public.rs`, and `git diff --check` passed.

Commit `ee05538a3` (`docs: refresh experimental D3D11 backend overview`)
replaces the obsolete README that said adapter enumeration was empty and the
renderer was not available through wgpu. It now accurately describes the
opt-in public offscreen adapter, strict commands, and still-gated surfaces.
No behavior changed in that documentation commit.

The public offscreen API now has verified depth and stencil use, but no HWND
surface capabilities. The next investigation revisits DXGI Present scheduling
and reentrancy; do not attempt to resolve it by dropping the immediate-context
or core queue-order locks without a correct serialization design.

## Checkpoint — public FL9_1 blending and Present audit

Commit `28d3bbddd` (`dx11: verify public FL9_1 blending and validation`) adds a
strict public render test for a premultiplied red overlay over opaque blue.
Using identical color and alpha equations, all 64 pixels read back as
`[128, 0, 128, 255]`. The test separately confirms core rejects
`SrcAlphaSaturated` and differing full-channel RGB/alpha equations before HAL.
Capability, status and test documents record the exact verified subset.

On 2026-11-05, the complete strict public suite passed 4/4 on the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with requested/actual FL9_1 and
`software=false`. It covers the indexed textured/depth-tested draw, dynamic
uniform update, public selected-entry texture-load validation, multi-device
submission/readback, R8 sampling, symmetric stencil readback, and blending.
The blend case's strict standalone filter passed 1/1. Clippy for wgpu tests,
pinned rustfmt and `git diff --check` passed.

Two independent read-only audits and the local source trace reconfirmed the
public HWND Present blocker. Core holds `device.command_indices.write()` across
HAL `present` (`wgpu-core/src/present.rs`); core submissions hold the same
serialization guard while invoking HAL submit. D3D11 surface presentation
retains a busy operation, and `CoreWindowFrame::present_for_hal` holds the
non-reentrant immediate-context mutex over synchronous `IDXGISwapChain::Present`
(`wgpu-hal/src/dx11/surface.rs`, `swapchain.rs`, and `device.rs`). A reentrant
window callback that submits can block first in core and then, even if that
lock is changed, on the HAL context. Dropping those guards without replacement
would allow unordered submissions or concurrent immediate-context use. A
reentrant mutex alone does not prove nested context calls safe while Present is
on the stack; surface unconfigure can also wait on the busy operation.

Microsoft [DXGI best practices](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices)
states that DXGI calls including Present may synchronously send window messages
and recommends not blocking the message pump. [D3D11 multithreading guidance](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-intro)
states that only one thread should use the immediate context and recommends
using that same thread for DXGI, especially Present. The likely safe direction
is a serialized asynchronous per-device executor: reentrant submissions must
enqueue and return rather than wait on a worker blocked in Present, while
command order, resources, mapping, surface lifecycle, and completion semantics
remain correct. This is a cross-layer redesign, not a lock-release patch. Keep
public surface capabilities unavailable while continuing independent FL9_1
work.

## Checkpoint — public FL9_1 scissor bounds

Commit `ee430b9b2` (`dx11: verify public FL9_1 scissor bounds`) adds a public
offscreen render test using an 8×8 RGBA8 target and a fullscreen triangle. A
scissor at `(2, 1)` with size `4×6` produced exactly 24 green pixels while the
remaining 40 stayed blue. The same selected-device test submits a viewport
extending one pixel past the left target edge and confirms core validation
rejects it before HAL. The capability, testing and status documents now state
this public evidence.

On 2026-11-05, with Rust 1.93.1, Visual Studio 2022 x64 and
`WGPU_DX11_REQUIRE_HARDWARE=1`, the full public suite passed 5/5 on the NVIDIA
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
`software=false`. The scissor test passed 1/1 in its focused run. Across the
five strict tests, rendering/readback covers the indexed textured depth scene,
dynamic uniform update, multi-device submission, R8 sampling, symmetric
stencil, premultiplied blending and scissor bounds; negative checks reject
texture loads, asymmetric stencil, unsupported blend states and out-of-target
viewports before HAL. `cargo +1.93 clippy --offline -j16 -p wgpu --tests
--no-default-features --features dx11,wgsl,std`, rustfmt and
`git diff --check` passed.

The safe public HWND presentation redesign remains the largest outstanding
cross-layer task. Independent audit evidence for the synchronous message-pump
and lock interaction is in the previous checkpoint above. Continue with other
bounded public FL9_1 functionality while surface capabilities remain disabled.

## Checkpoint — public sparse multi-texture sampling — `6280e36df`

Added a strict public wgpu integration test that declares two independently
uploaded fragment textures at logical bindings 7 and 42 and a shared sampler
at binding 100. Bind-group layout entries, WGSL declarations and runtime group
entries are deliberately ordered differently, exercising sparse logical
identities through core, HAL allocation, shader compilation and D3D11 runtime
binding. A shader adds both sampled colors. It also attempts to create an
incomplete group without binding 42 and verifies that public core returns a
validation error.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 environment on
2026-11-05. The focused strict test passed 1/1. The complete strict command

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

passed 6/6. Every test selected the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with requested and actual FL9_1 and
`software=false`. The new case verified all 64 pixels as
`[104, 128, 24, 255]`. `cargo +1.93 clippy --offline -j16 -p wgpu --tests
--no-default-features --features dx11,wgsl,std`, targeted rustfmt and
`git diff --check` passed. Visual Studio printed its existing
`vError: Unknown error` banner while providing the working x64 toolchain.

The test does not widen the selected capability projection. Public HWND
surface capabilities remain unavailable because synchronous `Present` can
reenter while core queue-order and HAL immediate-context locks are held. The
contract audit and historical FL9_1 hardware verification remain outstanding.

## Checkpoint — public eight-texture boundary — `98c6879bd`

Added a strict public wgpu test for the selected projection's maximum of eight
sampled textures per fragment stage. It verifies the reported limit, binds and
samples eight independently uploaded textures at sparse logical binding
numbers through one sampler, and checks that every texture contributes to all
64 output pixels. It also confirms a nine-texture fragment layout fails with a
public core validation error before HAL.

The focused strict test passed 1/1. The complete `dx11_public` suite passed
7/7 on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with
requested and actual FL9_1 and `software=false`. The new test verified
`[180, 72, 36, 255]` on all 64 pixels; core rejected texture nine. Rust 1.93.1
wgpu test Clippy, targeted rustfmt and `git diff --check` passed. The Visual
Studio x64 prompt emitted its existing `vError: Unknown error` banner but
provided a working linker/toolchain. Exact command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

This tests the existing public count projection and does not enable any new
resource capability. Historical FL9_1 hardware, the full core/HAL contract
and public HWND presentation remain outstanding.

## Checkpoint — public texture-transfer roundtrip — `118918d40`

Added a public strict GPU test that starts with an 8×8 RGBA8 texture, updates
its 2×2 region from a mapped copy buffer at nonzero byte offset 16 and 256-byte
row pitch, copies that region from source origin `(2, 3)` to destination
origin `(3, 2)` between 8×8 and 6×6 textures, then reads the complete 6×6
destination using a 256-byte readback pitch. The source rows contain four
distinct colors; the destination starts green. All 36 pixels matched the
expected 2×2 patch while the other 32 pixels retained green. The mapped upload
buffer and textures remain alive through ordered submission and map completion.

The focused strict test passed 1/1. The complete public integration suite
passed 8/8 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with
requested and actual FL9_1 and `software=false`. The initial compile caught an
incorrect direct index on `BufferViewMut`; the test now writes two mapped
subranges through `get_mapped_range_mut` and no compiler warning remains.
Rust 1.93.1 wgpu test Clippy, targeted rustfmt and `git diff --check` passed.
Visual Studio's existing prompt banner was emitted, but its x64 compiler and
linker worked. Command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

This verifies the projected RGBA8 copy usages through public core and the HAL;
it does not establish all texture combinations or historical FL9_1 driver
behavior. Public HWND presentation and the exhaustive capability audit remain
outstanding.

## Checkpoint — public sampled RGBA8 mip chain — `f191d4f97`

Added a public strict test that creates an 8×8, four-level RGBA8 resource,
uploads a different solid color to every mip with padded rows, creates a view
of the full chain, and renders it with implicit derivatives. An oversized
fullscreen triangle produces a known 2:1 texel minification ratio and selects
level 1 through a nearest mip filter; all 64 output pixels matched green. The
test also verifies the public core rejects a three-mip R8 texture under the
selected device's `SINGLE_MIP_ONLY` format projection.

The focused strict test passed 1/1. The complete public suite passed 9/9 on the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with requested and actual
FL9_1 and `software=false`. `Rgba8Unorm` reported sampled, upload, copy,
render-target and filtering support with no `SINGLE_MIP_ONLY` flag. The new
readback selected mip 1 in every pixel; R8 multi-mip creation produced a public
validation error. Rust 1.93.1 wgpu test Clippy, targeted rustfmt and
`git diff --check` passed. Early test runs selected mip 0 and then mip 2 while
calibrating the UV scale; the final 2:1 ratio selected mip 1 deterministically.
Visual Studio's existing x64 prompt banner did not affect the build or GPU
execution.

Exact strict command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

This proves the reported RGBA8 mip path on the tested RTX at forced FL9_1; it
does not generalize to NPOT textures, other formats or legacy drivers. Public
HWND presentation and exhaustive contract review remain outstanding.

## Checkpoint — public SNORM sampled formats — `f62e819db`

Added a public strict integration case that checks the selected adapter's
reports for `Rg8Snorm` and `Rgba8Snorm`, uploads each format, samples both
through one non-filtering sampler and maps RGBA8 output back to the CPU. Each
input uses signed values with nontrivial positive and negative channels; the
shader maps them into visible unorm output. Both format reports contain
`TEXTURE_BINDING | COPY_DST`, omit `FILTERABLE` and set `SINGLE_MIP_ONLY`.

The focused strict test passed 1/1 and the full public suite passed 10/10 on
the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual
FL9_1 and `software=false`. All 64 output pixels matched
`[192, 63, 160, 255]` within ±1. Rust 1.93.1 wgpu test Clippy, targeted
rustfmt and `git diff --check` passed. Visual Studio printed its recurring
developer-prompt banner while providing a working x64 build environment.

An independent read-only audit of public format, view, mip, upload, copy and
readback paths found no new undocumented mismatch. It reconfirmed the known
BC1 partial-edge upload restriction (block-aligned regions only), which remains
documented in `CAPABILITIES.md`; core allows edge-ending partial BC blocks that
the private HAL currently rejects. The strict command was:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The public offscreen contract and tested format subset are broader, but public
HWND presentation, the full reachable-operation audit and historical FL9_1
drivers remain outstanding.

## Checkpoint — public instanced vertex streams — `aeb41c79e`

Added a public strict GPU case using one per-vertex triangle stream plus a
second `VertexStepMode::Instance` stream containing per-instance offsets and
colors. `draw(0..3, 1..3)` selects records two and three; readback contains 20
green, 20 yellow and 472 blue clear pixels, with no pixels from the skipped
first red instance. This verifies both the emulated instance-rate binding and
nonzero `first_instance` behavior through public core/HAL submission. The same
test constructs a vertex entry that declares `instance_index` and confirms
core rejects the pipeline before HAL.

The focused strict test passed 1/1. The complete strict public wgpu suite
passed 11/11 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested and actual FL9_1, `software=false`. Rust 1.93.1 test Clippy,
targeted rustfmt and `git diff --check` passed. The Visual Studio x64 prompt
printed its existing `vError: Unknown error` banner, but build, link and GPU
execution succeeded. Exact command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

Public adapter presentation, the remaining core/HAL operation audit and
historical FL9_1 devices remain outstanding.

## Checkpoint — selected-entry sample-bias validation

Added three `DownlevelLimits` properties for the FL9_1 sample-bias subset:
maximum absolute literal bias 3, dynamic bias disabled, and texel offsets
disabled. Core records bias operations by entry point, follows helper calls,
and rejects unsupported forms as pipeline validation errors before HAL shader
compilation. This closes the case where a core-valid but unsupported bias
previously surfaced as a HAL linkage/internal error. Strict public readback
verified implicit sampling and literal biases -3, +1 and +3 against four mip
colors across 256 pixels. The public test also expects validation errors for
dynamic, helper-dynamic, out-of-range and offset forms. CPU-only tests cover
fractional literals, constant helper sampling, selected-entry isolation,
conservative dynamic rejection and unrestricted defaults.

Verification on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`):
requested/actual FL9_1, hardware adapter, no software fallback. Strict public
suite passed 11/11; core fixture suite passed 82/82, including real D3D11
readback cases; direct adapter test passed 1/1 and enumerated one FL9_1 device.
The wgpu-types suite passed 71/71; its serde compatibility case passed 1/1.
HAL `dx11,gles`, `wgpu-info`, and core `dx12` checks passed. Core and public
wgpu Clippy, wgpu-types serde Clippy, rustfmt checks and `git diff --check`
passed. The DX12 check emitted 15 existing warnings caused by building without
WGSL, while the no-serde wgpu-types test emitted existing serde-import
warnings. `wgpu-info` initially failed because its exhaustive downlevel
destructure had omitted the already-existing `texture_load` field; it now
prints that property alongside the new bias limits.

Exact strict commands:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -q -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1'
```

The Visual Studio prompt continues to print its pre-existing `vError: Unknown
error` banner despite correctly setting the x64 compiler and linker environment.
Public HWND presentation, a complete core/HAL operation audit and testing on
historical FL9_1 devices remain outstanding.

Code checkpoint commit: `f6e4125fe` (`core: validate FL9_1 texture sample bias limits`).

## Checkpoint — FL9_1 fragment derivative validation — `987f39cd3`

Added `DownlevelLimits::fragment_derivatives`, defaulting to true for backward
compatibility. The D3D11 hardware model marks the feature unavailable at FL9_1
and FL9_2 and available from FL9_3, while the current backend implementation
remains false. Core records derivative and explicit-gradient use per selected
entry point, follows helper calls, and rejects unsupported fragment pipelines
before HAL compilation. Naga expression structure is inspected directly; the
implementation does not parse WGSL or generated HLSL. The FXC test records the
actual `ps_4_0_level_9_1` diagnostics: `ddx` cannot map to the pixel
instruction set, and `SampleGrad` is unsupported by `ps_2_0`.

Verification used Rust 1.93.1 and the x64 Visual Studio 2022 developer
environment. With `WGPU_DX11_REQUIRE_HARDWARE=1`, the full HAL suite passed
338/338 test functions. GPU cases selected the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), software=false, and explicitly
requested/selected FL9_1. The selected-entry derivative/gradient negative
cases ran in the strict public mip test; the public suite passed 11/11 and
the full core D3D11 fixture suite passed 83/83. The fixture enumerated one
physical FL9_1 adapter. Some HAL test subcases intentionally report `SKIP`
after expected rejection of unsupported runtime integer indexing; these are
not missing-hardware skips, and no strict test silently substituted software.
The wgpu-types serde suite passed 73/73 plus 14 passing and 21 ignored doc
tests. The core DX12 unit suite passed 79/79. HAL `dx11,gles`, `wgpu-info`,
and core DX12 checks passed. Public wgpu, wgpu-types, and HAL Clippy passed;
core Clippy passed with three pre-existing warnings in the fixture test file,
and HAL Clippy passed with nine pre-existing warnings in unrelated D3D11
tests. Targeted formatting for changed crates and `git diff --check` passed.
Workspace formatting still reports pre-existing differences in committed
`naga/src/back/hlsl/writer.rs`. MSVC commands printed the existing Visual
Studio `vError: Unknown error` banner while compiling and linking successfully.

Exact test commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -q -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The repository-wide core/HAL audit remains unfinished; public presentation is
still gated by the existing synchronous DXGI `Present` reentrancy and lock
ordering problem. Historical FL9_1 hardware has not been tested. Continue
with shader/core contract gaps, then return to the surface scheduling design.

## Checkpoint — aggregate FL9_1 uniform-vector budgets — `76dd6656deec94d3832db6961a9516cdba08070e`

Added stage-specific `DownlevelLimits` for the FL9_1 vertex and fragment
constant budgets (255 and 32 float4 vectors). The hardware capability model
derives these limits only for FL9_x, while the active backend/compiler limits
remain a separate intersection. Core's validated Naga interface records the
complete layout size of uniform globals used by each selected entry point,
including helper calls, and rejects an over-budget vertex or fragment
pipeline before invoking HAL. Unused entry-point resources are not counted.
This size-based accounting is deliberately conservative when only part of a
declared structure is read. `wgpu-info` prints the values; non-D3D11 adapters
retain unlimited defaults. The downlevel limits remain non-WebGPU-compliant.

Microsoft's 10Level9 context reference specifies maximum totals of 255 vertex
and 32 pixel constants. FXC accepts 32 directly referenced pixel vectors in a
single buffer and across two 16-vector buffers; it rejects 33 and 17+16 at
register 32. A strict public FL9_1 pipeline reads all 32 vectors from two
sparse logical bindings, and GPU readback verifies 64/64 `[191, 0, 0, 255]`
pixels. The same public test asserts that a 33-vector fragment pipeline is
rejected by core validation before HAL. An initial shader test added an extra
`vec4` literal, which correctly consumed a 33rd FXC register; the final test
derives its alpha output from the uploaded uniforms instead.

Verification used `rustc 1.93.1 (01f6ddf75 2026-02-11)`, Visual Studio 2022 x64
tools and Cargo `-j16`. Strict GPU tests selected the physical NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested
FL9_1, observed actual FL9_1, and reported `software=false`:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 12/12 passed, no hardware skips. The new aggregate test rendered 64/64 expected pixels and rejected 33 vectors.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1`: 339/339 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1`: 84/84 passed, including physical adapter enumeration and core hardware cases.
- `cargo +1.93 test --offline -j16 -p wgpu-types --features serde`: 74/74 unit tests and 14 doc tests passed; 21 doc tests were ignored.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1`: 80/80 passed.
- HAL with `dx11,gles`, `wgpu-info`, and `wgpu-test` checks passed.
- Clippy passed for wgpu-types and wgpu. Core emitted three existing fixture-test warnings; HAL emitted nine existing test warnings. No warnings originated in the new implementation.
- `cargo +1.93 fmt --check` for wgpu-core, wgpu-hal, wgpu-types, wgpu-info, wgpu and wgpu-test, plus `git diff --check`, passed.

The first `wgpu-test` check was accidentally run outside the VS developer
prompt and could not find `cl.exe`; rerunning inside `VsDevCmd.bat` passed.
The known Visual Studio `vError: Unknown error` banner remains cosmetic. Public
HWND presentation, full public capability auditing, historical FL9_1 hardware
and remaining shader/resource gaps are still outstanding. Next continue the
reachable-operation audit and investigate the public surface scheduler.

## Checkpoint — explicit texture sample-level validation — `1dd629564`

The FL9_1 projection now includes `DownlevelLimits::texture_sample_level`.
Core records `ImageSample` operations with `SampleLevel::Exact` for each
selected entry point and its reachable helper functions, and rejects only the
selected pipeline entry before HAL. FXC diagnostics reject literal and dynamic
`textureSampleLevel` under both `ps_4_0_level_9_1` and `ps_4_0_level_9_3`
(`X4532: texlod not supported on this target`). The diagnostic profile exists
only to test compiler behavior; this adds no higher-level execution path.
Implicit sampling and safe sibling entries remain available. The capability
model separates the hardware threshold from the still-unimplemented backend
path, and serde defaults preserve older serialized `DownlevelLimits` values.

After the final core validation-order adjustment, the strict public wgpu suite
passed 12/12 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested/actual FL9_1, `software=false`; no hardware tests skipped. The public
test rejected direct and helper-forwarded explicit LOD through core while
creating the implicit-sampling sibling pipeline. Earlier in this same
implementation run, strict private HAL passed 340/340, the D3D11 core fixture
passed 85/85, `wgpu-types --features serde` passed 75/75 unit and 14/14 doc
tests (21 ignored), and the targeted DX12 core suite passed 81/81. Affected
Clippy and targeted formatting checks passed; only the repository's existing
HAL/core test warnings were reported. `git diff --check` passed.

Exact final public command:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
```

## Checkpoint — selected-entry FL9_1 texture-query validation — `38b5a0077`

Added `DownlevelLimits::texture_query`, with a backward-compatible serde
default. The hardware model marks this feature from FL10_0, while the current
D3D11 implementation reports it unavailable at every level. Core records
`ImageQuery` use per entry point, including helper calls, and rejects only the
selected vertex or fragment pipeline entry before HAL. CPU validation tests
cover direct dimensions queries, helper forwarding, `textureNumLevels`, safe
sibling entries, vertex-stage use and missing entries. The public strict test
rejects direct, helper and level-count queries while compiling a safe sibling.

FXC confirms that the current Naga HLSL query lowering is not usable under
`ps_4_0_level_9_1` or `_9_3`: dimensions and level-count shaders fail with
X3548, and a direct float-output `GetDimensions` control fails with X4532.
Equivalent controls compile under `ps_5_0`. This is a selected compiler-path
restriction, not a claim that every FL9_1 driver behaves identically.

Verification used Rust 1.93.1, Visual Studio 2022 x64 tools and Cargo `-j16`.
The strict public suite passed 12/12, the full private HAL suite passed
341/341, and the core D3D11 fixture suite passed 86/86. GPU tests selected
the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), requested and selected FL9_1, `software=false`; no hardware
tests skipped. The targeted updated FXC diagnostic test passed after adding
the float-output control. The `wgpu-types --features serde` suite passed
76/76 unit tests and 14 doc tests (21 ignored); the targeted DX12 core suite
passed 82/82. HAL `dx11,gles`, `wgpu-info` and `wgpu-test` checks passed.
Affected Clippy and targeted Rust formatting checks passed; core/HAL emitted
only the already-recorded fixture/test warnings. `git diff --check` passed.

The relevant strict runs used these Cargo commands from the x64 Visual Studio
developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -q -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The D3D11 implementation and regression tests are committed in
`38b5a0077488f7ebe7131516cc2d3de5a0d9c0c2`. Exact commands are in
`TESTING.md` and this checkpoint. Continue the exact core/HAL contract audit.
Public HWND presentation remains gated by the unresolved synchronous
`Present` reentrancy/serialization design, and historical FL9_1 hardware is
untested.

## Checkpoint — indexed lists without vertex-rate inputs — `d96650d72`

Indexed LineList and TriangleList draws no longer fail solely because the
pipeline has only instance-rate or no vertex-rate inputs. Since FL9_1 shader
validation rejects `vertex_index`, index values and `base_vertex` cannot affect
vertex data in these cases. HAL still validates pipeline/resource state,
instance-stream bounds and the requested index-buffer range, then records a
nonindexed list draw with the original primitive count and instance bindings.
Layouts with vertex-rate inputs retain the existing indexed emulation path.
Indexed strips still require vertex-rate inputs so restart runs and strip
connectivity remain explicit. PointList's separately tested instance-only
path is unchanged.

The new private and public tests cover LineList and TriangleList with Uint16
and Uint32 index buffers, nonzero first-index, negative base-vertex, two
instances, instance-rate-only input and an empty vertex layout. Full readbacks
match the equivalent nonindexed draws; the degenerate triangle cases also
assert that the cleared image remains unchanged.

Strict execution used Rust 1.93.1 and Visual Studio 2022 x64 tools. The physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) was selected
as hardware (`software=false`) and explicitly requested and observed at FL9_1.
There were no missing-hardware skips. Results after the code commit:

- Private HAL suite: 342 passed, 0 failed, 0 ignored. The new test exercised
  eight indexed/direct comparisons across topology, layout and index format.
- Public `wgpu` integration suite: 13 passed, 0 failed, 0 ignored. The new
  public test exercised the same eight comparisons through core and HAL.
- Strict D3D11 core fixture suite: 86 passed, 0 failed, 0 ignored.
- DX12 core regression suite: 82 passed, 0 failed, 0 ignored.
- HAL Clippy completed with nine existing test/code warnings; no warning came
  from this patch. `wgpu` test Clippy completed without warnings.
- Rustfmt checks on the three changed Rust files and `git diff --check` passed.
  Workspace `cargo fmt --all -- --check` still reports formatting differences
  in the untouched, committed `naga/src/back/hlsl/writer.rs`; that unrelated
  file was not reformatted.

Exact strict commands (run from the Visual Studio x64 developer environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The code commit is `d96650d72`. Continue auditing operations that core
accepts but the private HAL cannot execute, then return to the public surface
scheduler design. Public adapter access remains opt-in/offscreen-only;
historical FL9_1 drivers remain untested.

## Checkpoint — selected fragment-depth output validation — `ed642d768`

Core now models the `ps_4_0_level_9_1` output restriction found during the
shader audit: a fragment entry point that writes `frag_depth` without
`@location(0)` fails pipeline validation before FXC. Naga interface metadata
checks the selected entry point, so a color-and-depth sibling remains valid
and an unrelated entry point does not affect selection. The D3D11 capability
projection reports this restriction; other backends retain the default
permissive value. The private compiler test records FXC X4530 for direct-only
depth output and accepts structured color plus depth. The public integration
test verifies the precise core rejection and compiles the supported output.

Strict execution used Rust 1.93.1, Visual Studio 2022 x64 tools, and the
physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
at requested and actual FL9_1 (`software=false`). Results:

- Private HAL suite: 344 passed, 0 failed, 0 ignored.
- Public `wgpu` D3D11 suite: 14 passed, 0 failed, 0 ignored.
- Strict D3D11 core fixture suite: 88 passed, 0 failed, 0 ignored.
- Core DX12 regression suite: 84 passed, 0 failed, 0 ignored.
- `wgpu-types` unit suite: 76 passed; serde-enabled suite: 78 passed.
- D3D11 HAL/GLES compilation, `wgpu-info`, and `wgpu-test --tests` checks
  passed. The latter two checks also caught and corrected exhaustive
  `DownlevelLimits` consumers that needed the newly added fields.
- Clippy passed for HAL, core, wgpu, types, info and test crates. HAL emitted
  nine pre-existing warnings in unrelated test/swapchain/texel-load code.
- Changed-file rustfmt and `git diff --check` passed. No hardware tests skipped.

The code checkpoint is `ed642d768`. Continue the selected-entry shader and
core/HAL contract audit, then revisit the public HWND presentation scheduler.
The adapter remains opt-in and offscreen-only; historical FL9_1 drivers and
ARM32 UWP hardware remain unverified.

## Checkpoint — fragment sample-mask output gate — `26321bf74`

The next selected-entry shader audit found that WGSL fragment
`@builtin(sample_mask)` output lowers to HLSL `SV_Coverage`, which the
`ps_4_0_level_9_1` FXC profile rejects with X4583. Core previously accepted
this operation and could fail only later in the private shader path. Added
`DownlevelLimits::fragment_sample_mask_output`, defaulting to available for
other backends and false in the D3D11 FL9_1 projection. Core now checks only
the selected fragment entry point, rejects before HAL, and permits a safe
color-only sibling. No shader rewrite is attempted; mapping its low coverage
bit to discard remains a possible single-sample emulation investigation.

Strict validation used Rust 1.93.1 and Visual Studio 2022 x64 tools. Hardware
was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
selected as hardware (`software=false`) with requested and actual FL9_1.
The private profile test isolated the Naga-to-FXC compiler path and observed:
`error X4583: semantic 'SV_Coverage' unsupported on ps_4_0_level_9_1`. The
public test confirmed the specific core validation error and acceptance of a
color-only sibling; it did not submit a draw for the negative pipeline case.

Results:

- Strict private HAL suite: 345 passed, 0 failed, 0 ignored; no hardware skips.
- Strict public D3D11 suite: 15 passed, 0 failed, 0 ignored; all selected the
  physical RTX at FL9_1.
- Strict D3D11 core fixture suite: 89 passed, 0 failed, 0 ignored.
- Core DX12 regression suite: 85 passed, 0 failed, 0 ignored.
- `wgpu-types`: 77 passed without serde and 79 passed with serde. The
  non-serde build reports 14 pre-existing unused serde-import warnings.
- HAL with D3D11/GLES, `wgpu-info`, and `wgpu-test --tests` checks passed.
- Clippy passed for HAL, core, wgpu, types, info and test crates. HAL reports
  nine pre-existing warnings in unrelated test/swapchain/texel-load code.
- Changed Rust files pass rustfmt; `git diff --check` passes.

During implementation, the first core and type test builds found missing values
in exhaustive FL9_1 fixture/test literals; these were fixed before the full
runs. The initial HAL-level compile attempt hit the existing conservative
runtime-integer guard first, so the compiler diagnostic test calls the same
production Naga-to-FXC function directly to isolate the profile restriction.
An optional check invocation using `--features noop` was invalid because
`noop` is not a `wgpu-hal` Cargo feature; the `noop` module is unconditionally
compiled in the successful HAL checks and test suite.

The code commit is `26321bf74`. Continue auditing other core-accepted shader
operations and HAL paths. Public HWND presentation still needs a cross-layer
scheduling design that avoids synchronous DXGI `Present` reentrancy; historical
FL9_1 drivers and ARM32 UWP hardware remain unverified.

## Checkpoint — FL9_1 sample-index validation — `7f48be505`

The next shader-contract audit confirmed that `@builtin(sample_index)` does
not require another FL9_1-specific pipeline limit: core passes the selected
adapter's `DownlevelFlags::MULTISAMPLED_SHADING` into Naga validation, and the
FL9_1 projection omits that flag. Added a strict public regression which opens
the physical device at forced FL9_1, verifies that a fragment entry using
`sample_index` fails through the expected Naga capability diagnostic, and
verifies a single-sample shader module remains accepted. This is a validation
test, not a GPU draw. The code-only checkpoint is `7f48be505`.

Verification used Rust 1.93.1 and the VS 2022 x64 environment:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_sample_index_without_multisampled_shading -- --nocapture --test-threads=1`: passed 1/1 on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`.
- `rustfmt +1.93 --check wgpu/tests/dx11_public.rs` and `git diff --check`: passed.

The full HAL/public/core suites were not rerun for this isolated regression.
Continue the selected shader/core/HAL audit and the HWND presentation
scheduling investigation. Historical FL9_1 hardware and ARM32 UWP remain
unverified.

## Checkpoint — required FL9_1 sRGB color formats — `a9f46afe1`

The selected-format model now treats RGBA8_SRGB and BGRA8_SRGB as FL9_1
required formats while retaining independent positive `CheckFormatSupport`
gates. The backend routes their typed DXGI formats through the existing
single-mip texture upload, sampled SRV, color-target RTV, texture readback and
copy paths. Public format reporting exposes only usages and filter/blend flags
reported for the selected format and already implemented by the backend. A
stale HAL negative test was changed to keep testing rejection with Rgba16Float;
the two newly supported sRGB formats are no longer expected to fail creation.

Strict hardware validation used Rust 1.93.1 and Visual Studio 2022 x64. The
selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
was a physical adapter (`software=false`) and explicitly requested and
returned FL9_1. `CheckFormatSupport` returned `0x112ef3e0` for each sRGB format.
Public readback decoded uploaded sRGB texels to 128/128 expected linear pixels
`[55, 13, 134, 255]`; rendering linear `[0.5, 0.25, 1.0, 1.0]` into RGBA8_SRGB
and BGRA8_SRGB encoded 128/128 expected pixels, including the correct channel
order. No hardware test skipped. The dedicated public test checks single-mip
paths; no sRGB mip-chain execution case has run, and this modern device does
not establish historical-driver behavior.

Verification:

- `cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11`: passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1`: 346 passed, 0 failed, 0 ignored. The first run exposed the stale sRGB rejection assertion (345 passed, 1 failed); after correcting that test, the full run passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 17 passed, 0 failed, 0 ignored, all physical FL9_1 adapter cases.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1`: 89 passed, 0 failed, 0 ignored.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1`: 1 passed; core exposed the one physical FL9_1 adapter in the explicitly enabled DX11 configuration.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1`: 85 passed, 0 failed, 0 ignored.
- HAL Clippy passed with nine pre-existing warnings in unrelated test, swapchain and texel-load proof code. Public `wgpu` test Clippy passed without warnings. Changed-file rustfmt and `git diff --check` passed.

One early `rustfmt` invocation omitted the crate edition and could not parse
the repository's existing Rust 2021 C-string literal; the changed files pass
when checked with `--edition 2021`. One initial core test command omitted the
`wgsl` feature and failed to compile its WGSL-based unit tests; the corrected
command above passed. Neither issue required a product-code change. Historical
FL9_1 hardware, ARM32 UWP and the existing public HWND presentation scheduler
remain unverified or unresolved; continue the independent format and
core/HAL-contract audit.

## Checkpoint — public FL9_1 sRGB mip execution — `f2c813671`

The selected RTX projection reports mip support for both sRGB formats, so a
second public GPU test now creates four-level POT `Rgba8UnormSrgb` and
`Bgra8UnormSrgb` textures. It uploads different values to each level, samples
with implicit LOD into a linear RGBA8 target, and verifies that mip 1 is
selected and decoded to `[55, 13, 134, 255]` in all 128 checked pixels. This
closes the previously documented sRGB mip execution gap for the tested
implicit-LOD path; it does not test every LOD clamp or mip filter combination.

The first test attempt sampled mip 0 because its UV gradient produced a base
LOD. The expected mip-1 result was therefore wrong; the test gradient was
adjusted to match the calibrated public POT-mip test pattern. The corrected
strict test passed on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested and actual FL9_1,
`software=false`. The full strict public suite then passed 18/18 with no
skips. `cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public
--no-default-features --features dx11,wgsl,std`, changed-file rustfmt and
`git diff --check` passed. The suite does not establish historical-device
behavior; continue the capability and core/HAL audit.

## Checkpoint — public FL9_1 D16 depth-only attachment — `d0efe2753`

The private and selected-device capability models now expose D16_UNORM as a
single-mip depth-only attachment when the FL9_1 required-format guarantee, the
backend path and the selected device's `CheckFormatSupport` query all agree.
Texture initialization clears depth without requesting a stencil clear. HAL
views retain their actual depth format through render-pass and pipeline setup;
stencil operations on D16 and unsupported multi-mip requests are rejected.
Ordinary D24S8 behavior and normal D3D11 enumeration remain unchanged.

The strict public wgpu test draws near and far full-screen triangles in both
orders through a D16 attachment and validates 128/128 green pixels. It also
verifies the selected format projection and core rejection of multi-mip D16
and active stencil state. The NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) reported `CheckFormatSupport(D16_UNORM)=0x210020`; the
device was physical (`software=false`) and explicitly requested and returned
FL9_1. This validates the modern forced-level path, not historical drivers.

Verification (Rust 1.93.1, Visual Studio MSVC environment):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 -- --nocapture --test-threads=1`: 347 passed, 0 failed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 19 passed, 0 failed, 0 skipped.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 dx11::capabilities::tests -- --nocapture --test-threads=1`: 23 passed, including the selected-device D16 query/projection.
- D3D11 core fixture: 89 passed; core physical-adapter enumeration isolation: 1 passed; DX12 core regression: 85 passed.
- HAL and public-test Clippy passed; HAL emitted nine pre-existing warnings in unrelated code. Changed-file rustfmt and `git diff --check` passed.

The initial public test expected only `RENDER_ATTACHMENT`, but the HAL
correctly includes `TRANSIENT_ATTACHMENT`; the assertion was narrowed to
require render attachment and forbid sampled/copy usages. No implementation
failure remained. Continue depth/load/store and general core/HAL contract
coverage; safe public HWND presentation is still blocked on cross-layer DXGI
Present scheduling, and historical FL9_1 plus ARM32 UWP remain unverified.

## Checkpoint — D16 view and pass transitions — `0cba1afbb`

Extended the public strict D16 test to use an explicit `DepthOnly` view and
confirm that core rejects `StencilOnly`. Added two successive render-pass
cases: a second pass using LOAD retains the first pass's near depth and rejects
a farther fragment; a second pass using CLEAR resets the depth and accepts the
farther fragment. GPU readback matched all 128 transition pixels (64 per
case). The original draw-order cases still match all 128 pixels.

The first full-suite attempt failed because the view edit was accidentally
applied to an earlier D24 test. That change was restored, and the view checks
were scoped to the D16 test. The strict full public suite then passed 19/19 on
the physical RTX 3090 Ti, vendor/device `0x10de:0x2203`, driver
`32.0.16.2101`, requested/actual FL9_1, `software=false`, with zero skips.
`cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public
--no-default-features --features dx11,wgsl,std`, changed-file rustfmt and
`git diff --check` passed. The test does not exercise D16 discard semantics.
Next continue validated depth/render-pass combinations and the wider
core/HAL audit.

## Checkpoint — D16 discard followed by LOAD — `e3807a1b3`

Extended the public D16 render-pass test with a depth attachment stored using
Discard, followed by a second pass that loads depth and draws a nearer
primitive. The first pass writes depth 0.75 and red color; if that discarded
depth remained visible, the later 0.25 draw would replace the red output. The
strict readback remained red for all 64 pixels, showing that core's required
initialization after discard prevented stale depth from affecting subsequent
rendering. Together with the prior D16 cases, the test checked 128/128
draw-order pixels, 128/128 LOAD/CLEAR transition pixels and 64/64 discard-load
pixels.

The full strict public suite passed 19/19 on the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested and
returned FL9_1, `software=false`, with no skips. Targeted public-test Clippy,
rustfmt and `git diff --check` passed. The full private HAL, core fixture and
DX12 suites were not rerun for this test-only increment; their latest results
remain 347/347, 89/89, and 85/85 respectively at `d0efe2753`. Continue with
other validated render-pass combinations and the core/HAL audit.

## Checkpoint — query-gated R16G16_SNORM sampling — `95a7d50e8`

Microsoft's required DXGI format table distinguishes `R16G16_SNORM` 2D texture
support at FL9_1 from `SHADER_SAMPLE`, required beginning at FL9_2. The
capability model now records that distinction; an FL9_1 path is considered
only when the selected device's format query positively reports both
`TEXTURE2D` and `SHADER_SAMPLE`. It remains single-mip sampled/upload only.

The physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) was explicitly created at FL9_1 and returned
`R16G16_SNORM CheckFormatSupport=0x00021122`, which lacks `SHADER_SAMPLE`.
The capability projection reports no usages, private HAL texture creation
rejects the format, and the public test observes the same validation error.
The strict public SNORM test still sampled R8G8_SNORM and R8G8B8A8_SNORM and
matched all 64 output pixels. Synthetic capability tests cover a complete
positive query and reject incomplete or unavailable queries. A conditional
padded upload/sample path is implemented, but no GPU has exercised that
positive branch; no R16G16_SNORM sampled execution is claimed.

Verification (Rust 1.93.1, Visual Studio MSVC environment):

- The full strict private HAL suite passed 349/349. The R16G16_SNORM case was
  a GPU-query-driven rejection check on this adapter; other suite hardware
  cases executed normally with strict hardware selection.
- The full strict public D3D11 suite passed 19/19 with no skips. Its SNORM case
  verified R16G16_SNORM is not exposed and rejected by core, and validated the
  supported R8G8/RGBA8 SNORM pixel output.
- The capability-model tests passed 24/24; the final focused optional-query
  test passed 1/1. The private query-gated hardware test passed 1/1 and the
  public SNORM integration test passed 1/1 on the RTX.
- The D3D11 core fixture passed 89/89; physical-adapter enumeration passed
  1/1; the DX12 core regression passed 85/85.
- HAL and public D3D11 Clippy passed. HAL reported nine existing warnings in
  unrelated code; public Clippy had no warning. Changed-file rustfmt and
  `git diff --check` passed.

The `VsDevCmd.bat` banner included `vError: Unknown error` but MSVC compilation,
linking and all test processes completed successfully. The mission remains
active. Next continue FL9_1 depth/stencil render-pass transition coverage and
the public core/HAL contract audit; HWND presentation remains off until a safe
cross-layer scheduling design is established.

## Checkpoint — public D24S8 pass transitions — `24bbe6bce`

Commit `24bbe6bce` adds strict public wgpu coverage for D24S8 depth/stencil
LOAD, CLEAR and per-aspect DISCARD followed by LOAD. A LOAD pass preserved
depth 0.5 and stencil 1 together; separate stencil-clear and depth-clear
comparisons proved each aspect changed independently. Separate stencil and
depth discard cases then loaded both aspects and verified core initialized the
discarded subresource before rendering. The depth-discard probe chose an
incoming 0.25 value with `Less`: it would pass against stale depth 0.5 but
fails against the core's initialized zero. A sampled D24S8 descriptor was
rejected by public core; the existing D16 test covers rejection of the
unsupported stencil-only view. This does not assert native contents after
discard or historical-driver behavior.

The baseline D24S8 stencil test passed 1/1 on the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) at requested/actual FL9_1. The new
transition test passed 1/1 on the same physical adapter and verified 320
pixels: 64 combined LOAD, 64 stencil CLEAR, 64 depth CLEAR, 64 stencil discard
then LOAD, and 64 depth discard then LOAD. The full strict public D3D11 suite
passed 20/20, with no skips; every case selected physical hardware at FL9_1.

Exact verification (Rust 1.93.1, Visual Studio x64 developer environment,
Cargo `-j16`):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_stencil_replace_and_compare_at_fl91 -- --nocapture --test-threads=1`: 1/1 baseline test passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d24s8_load_clear_discard_transitions_at_fl91 -- --nocapture --test-threads=1`: 1/1 passed with the GPU readback above.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 20/20 passed, 0 failed, 0 ignored; no hardware skips.
- `cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std`, targeted rustfmt check and `git diff --check` passed.

The initial depth-discard assertion expected a later `Greater` test to fail,
but it passed because core initializes the discarded aspect to zero. The final
`Less` probe now distinguishes the stale 0.5 value from that initialization.
The private HAL, controlled core fixture, adapter-isolation and DX12 suites
were not rerun for this public-test-only increment; their recorded results at
the previous code checkpoint remain separate evidence. Continue the broader
core/HAL audit, then return to the cross-layer HWND presentation scheduler.
The public adapter remains opt-in and offscreen-only.

### Follow-up — public D24S8 capability/aspect checks — `cdd06e5ca`

The D24S8 public test now records the selected adapter's format projection and
asserts it is render-attachment/transient only and single-mip. It also verifies
public core rejects both `DepthOnly` and `StencilOnly` D24S8 views while the
combined attachment view succeeds. The focused strict test passed 1/1 and the
full strict public suite passed 20/20 on the RTX at requested/actual FL9_1;
public-test Clippy, rustfmt and `git diff --check` passed. No HAL/core code
changed in this follow-up. Commit: `cdd06e5ca`.

## Checkpoint — compatible linear/sRGB texture copies — `0de479c09`

The public core permits a texture copy between RGBA8 linear and sRGB formats,
and between their BGRA8 equivalents, because each pair belongs to one DXGI
format type group. The D3D11 HAL previously required exact DXGI format equality
and returned `E_INVALIDARG` for these public-valid copies. The HAL now accepts
only those four symmetric linear/sRGB pairings in addition to exact-format
copies; unrelated cross-family formats remain rejected. Native copies preserve
the stored texel bits and do not perform sRGB conversion.

A strict public wgpu test uploaded asymmetric 4×4 patterns into both RGBA8 and
BGRA8 linear textures, copied each to its sRGB partner and then back to linear,
and read back both intermediate and round-trip textures through 256-byte rows.
All 32 checked pixels matched their original bytes exactly on the NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and
actual FL9_1, `software=false`.

Verification (Rust 1.93.1, Visual Studio x64 developer environment, Cargo
`-j16`):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_srgb_texture_copies_preserve_bits_at_fl91 -- --nocapture --test-threads=1`: 1/1 physical GPU test passed.
- The complete strict public suite passed 21/21, with no skips; every test selected the RTX at forced FL9_1.
- The complete private HAL suite passed 350/350 under strict hardware mode, including the new format-family CPU test and existing GPU rendering/transfer tests.
- The controlled core D3D11 fixture passed 89/89, including selected physical adapter enumeration and core-to-HAL GPU execution.
- DX12 core regression passed 85/85.
- HAL and public wgpu-test Clippy, changed Rust file rustfmt check and `git diff --check` passed.

The initial invocation outside `VsDevCmd` lacked `link.exe`; rerunning in the
installed Visual Studio environment succeeded. `VsDevCmd` continues to print
`vError: Unknown error` in its banner, but all MSVC builds, links and test runs
completed successfully. Public HWND presentation remains gated by the
cross-layer `Present` reentrancy deadlock; the mission continues with the
remaining public core/HAL audit and scheduler design.

## Checkpoint — upstream Naga capability selector — `d93da4333`

The shared `wgpu-gpu` Naga capability test still treated `Backend::Dx11` as
unreachable. It now compares the adapter's projected Naga capabilities against
Naga's HLSL backend ceiling. The D3D11 API/module/device/queue/shader comments
and the testing guide were also corrected to describe the current opt-in,
offscreen adapter and core fixture rather than the pre-integration scaffold.

The first focused harness run selected the physical adapter but reported
`[Unsupported: Limits]`; its `ok` result represented a skip, not a test pass.
Commit `d93da4333` lowers this test's requested limits to the values required
by its no-resource capability check and the selected FL9_1 projection. The
rerun reported `[Executed]` and passed 1/1 on the RTX 3090 Ti at FL9_1. This
selector checks Naga capability projection and device setup; it does not draw
or read back pixels. The test crate also passed a normal `cargo check` with its
DX11 feature enabled.

The full `wgpu-gpu` harness command completed with Cargo's summary of 351
passed and zero failed. Its per-test names showed that only one test executed;
330 were unsupported by the advertised limits and 20 were skipped by test
expectations. The Cargo pass count therefore does not represent 351 executed
GPU tests. The one executed case was `naga_capabilities::validate_capabilities`.

Exact test commands:

- `WGPU_INFO_SAVE_GPUCONFIG_REPORT=1 cargo +1.93 test --offline -j16 -p wgpu-info --features wgpu/dx11 generate_gpuconfig_report -- --nocapture`
- `WGPU_BACKEND=dx11 WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-test --test wgpu-gpu --features wgpu/dx11 -- validate_capabilities --nocapture`
- `WGPU_BACKEND=dx11 WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-test --test wgpu-gpu --features wgpu/dx11 -- --nocapture`
- `cargo +1.93 check --offline -j16 -p wgpu-test --test wgpu-gpu --features wgpu/dx11`

For the runtime test only, the workspace's default `wgpu` feature list was
temporarily adjusted from `dx12`/`static-dxc` to `dx11` because this Build
Tools installation lacks `atls.lib`; the exact `Cargo.toml` bytes and existing
`.gpuconfig` bytes were restored in a `finally` block. The generated report
contained one DX11 adapter. The broader harness currently exercises almost
none of the public projection because its default requested limits exceed the
FL9_1 limits. The Deno CTS package still does not enable DX11, and this
non-WebGPU-compliant adapter must not be exposed to a conformance runner.

## Checkpoint — serialized public HWND presentation — `9ed1cc305`

The explicit `dx11` backend now reports a deliberately narrow public surface
contract for valid HWNDs: BGRA8 sRGB FIFO, opaque composition, render-attachment
usage and one frame of latency. Configure, acquire and present are restricted
to the HWND owner thread. A queue submission gate serializes queue submissions,
writes, mapping flushes, polling, surface configuration and selected
context-backed resource creation against `Present`. Same-thread reentrant work
that could mutate queue or mapping state is rejected. A HAL thread-affinity
query lets core reject the wrong thread before it waits on queue serialization.
The existing D3D11 default-backend selection is unchanged; callers must opt in
to `dx11`.

On the RTX 3090 Ti (`0x10de:0x2203`, NVIDIA driver `32.0.16.2101`), the device
was explicitly requested and confirmed at FL9_1 with `software=false`. The
public HWND test configured 64×64, rendered a clear, presented, dropped the old
backbuffer view, resized to 80×48, rendered and presented again, then verified
wrong-thread acquisition was rejected. The window was hidden: this proves
surface lifecycle and GPU presentation calls, not visible desktop output or a
continuously pumped message loop. Public D3D11 is still experimental and not
WebGPU-compliant.

Verification (Rust 1.93.1, Visual Studio 2022 x64 developer environment,
Cargo `-j32`):

- Strict private HAL suite: 351/351; strict D3D11 core fixture: 90/90; strict
  public D3D11 integration: 22/22, no failures or hardware skips. The public
  tests executed against the physical adapter at requested/actual FL9_1. The
  focused public HWND test passed 1/1 after the final HAL thread-affinity hook.
- Core DX12 regression: 86/86.
- Strict public adapter-selection test: 1/1; it explicitly enumerated the
  physical FL9_1 adapter through the opt-in D3D11 path. Normal backend
  priorities remain unchanged.
- Core and HAL Clippy passed with three and nine existing unrelated warnings;
  public-test Clippy was clean. Changed-file rustfmt and `git diff --check`
  passed. Workspace `cargo fmt --all -- --check` still reports the existing
  unrelated formatting differences in `naga/src/back/hlsl/writer.rs`.

The initial resize attempt failed because the test retained a view of the old
swapchain buffer; dropping that view before reconfiguration fixed the test.
Cross-thread acquisition is tested only after the final present because the
public core maps the HAL's wrong-thread surface error to `Lost` for that
surface. Exact commands (Rust 1.93.1):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1

& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1'
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1'
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-core/src/device/mod.rs wgpu-core/src/device/queue.rs wgpu-core/src/device/resource.rs wgpu-core/src/present.rs wgpu-core/src/resource.rs wgpu-hal/src/lib.rs wgpu-hal/src/dynamic/surface.rs wgpu-hal/src/dx11/adapter.rs wgpu-hal/src/dx11/surface.rs wgpu/tests/dx11_public.rs
git diff --check
```

The implementation checkpoint is commit `9ed1cc305` (`dx11: add serialized
public HWND presentation`).

## Follow-up — visible public HWND example — `ca0b026fd`

`wgpu/examples/dx11_fl91_windowed.rs` now creates and shows a native HWND,
selects a surface-compatible physical D3D11 adapter explicitly at FL9_1,
configures BGRA8 sRGB FIFO presentation, pumps the Windows message queue,
renders an animated indexed textured cube with D24S8 depth, and handles
client-area resize by reconfiguring the surface and recreating depth storage.
`--frames=N` makes the interactive example usable as a bounded smoke run.

The example compiled cleanly and ran for 60 frames on the NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
`software=false`. It printed `Presented 60 textured, indexed, depth-tested
frames.` and verified that the HWND was visible. This confirms the public
message pump and repeated surface render/present path. No screenshot or pixel
capture was taken; deterministic output remains covered by the public
offscreen readback test.

Code commit `ca0b026fd` (`dx11: add visible FL9_1 windowed example`) contains
the example and reproduction notes. Targeted Clippy, rustfmt and
`git diff --check` passed.

Exact command (Rust 1.93.1, Visual Studio x64 developer environment):

```powershell
cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std -- --frames=60'
```

The focused HWND lifecycle rerun also passed 1/1 after the surface checkpoint.
Next continue the public core/HAL contract audit, including additional
reentrant and resize edge cases, and independently inspect the visible output
when a visual-capture workflow is available. Historical FL9_1 behavior remains
unverified.

## Checkpoint — optional half-float vertex inputs (`867122688978524082ada4ba8614a3c3f4f72c3b`)

The required DXGI format table makes `R16G16_FLOAT` and
`R16G16B16A16_FLOAT` input-assembler support mandatory from FL9_3, not FL9_1.
The HAL now maps both formats but keeps them out of FL9_1 hardware guarantees.
The capability projection includes either only after the selected FL9_1
device's `CheckFormatSupport(IA_VERTEX_BUFFER)` query succeeds and the backend
mapping is present. Synthetic tests cover positive, absent-query and missing-
backend cases, including the resulting wgpu `DownlevelLimits` mask.

The RTX 3090 Ti (`0x10de:0x2203`, NVIDIA driver `32.0.16.2101`) was requested
and confirmed at FL9_1. Its public selected-device projection reports
`Float16x2=false`. The strict public test therefore logged
`SKIP: selected FL9_1 device does not report IA support for R16G16_FLOAT`; no
half-float draw is claimed as GPU-executed. `Float16x4` shares the same query
and mapping path and has no separate rendering test yet.

Verification after the code change: private HAL suite 351/351; public strict
D3D11 suite 23/23 test functions with one capability-based optional-path skip
(22 other public GPU tests executed); controlled core fixture 90/90; strict
physical adapter selection 1/1; core DX12 regression 86/86. HAL Clippy passed
with nine existing warnings outside this change; public-test Clippy passed
cleanly. Changed Rust files pass Rust 1.93 rustfmt and `git diff --check`.
The first core test invocation omitted `wgsl` and MSVC environment setup; it
failed at build configuration/link resolution and passed on the corrected
MSVC command below.

Commands run (Visual Studio 2022 x64 developer environment, bounded Cargo
parallelism `-j32`):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
```

Commit `867122688978524082ada4ba8614a3c3f4f72c3b` contains this implementation,
the public test and documentation. Continue auditing public core/HAL behavior
and optional vertex formats. Do not claim that the RTX executed Float16x2 at
FL9_1 or that a forced modern device validates historical FL9_1 drivers.

## Follow-up — visible FL9_1 frame capture

Re-ran the windowed example with strict hardware mode on the physical RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
`software=false`:

```powershell
cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std -- --frames=1800'
```

The run completed 1,800 frames. A second long bounded run was captured and
closed by posting `WM_CLOSE`; it presented 80,871 frames before the close was
processed. The client-area PNG was captured at the active monitor's physical
pixel coordinates after accounting for desktop DPI virtualization, then
visually inspected. It showed red and green cube faces over the expected dark
clear color. The temporary image is at
`%TEMP%\wgpu-dx11-fl91-windowed.png`; it is intentionally not checked into the
repository. This verifies visible output qualitatively, not deterministic
per-pixel presentation output; deterministic color/depth validation remains
covered by the public offscreen readback tests.

The initial capture attempt used DPI-virtualized coordinates and included
unrelated desktop regions. It was discarded and not used as evidence. The
final capture used per-monitor-aware coordinates and cropped to the HWND
client area. No code files changed during this verification.

## Checkpoint — selected-device optional vertex formats (`a4e9bc360`)

Code commit: `a4e9bc360d3355081292c87c9ce6ab1800252d8e`.

The HAL now maps all 41 wgpu vertex formats with direct DXGI IA equivalents,
excluding Float64 formats that have no faithful mapping. The FL9_1 static
guarantee set remains unchanged. Every mapped format outside that set enters
the selected-device projection only when `CheckFormatSupport` reports
`IA_VERTEX_BUFFER`; failed or missing queries stay unsupported. Allocation
tests verify the mapping table is unique and each stride matches
`VertexFormat::size()`. Synthetic projection tests cover the optional formats
as a whole, positive/negative device query results, and missing backend
mapping.

On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) explicitly forced
to FL9_1, the selected mask was `0x78c30104` and contained exactly the ten
guaranteed formats: `Uint8x4`, `Unorm8x4`, `Sint16x2`, `Snorm16x2`,
`Sint16x4`, `Snorm16x4` and `Float32` through `Float32x4`. No optional input
format was reported. The public Float16x2 test logged its capability skip;
this is not GPU execution evidence for optional formats.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 developer
environment, strict hardware mode, offline dependencies and Cargo `-j32`:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 vertex_format -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/capabilities.rs
git diff --check
```

Results: private HAL 351/351 before the final test-only invariant assertion;
the focused vertex-format test filter passed 5/5 afterward. Public D3D11
passed 23/23 test functions, with one capability-based optional Float16x2
skip and 22 other public GPU tests executed. The controlled core fixture
passed 90/90, physical adapter selection 1/1, and DX12 core 86/86. HAL Clippy
passed with nine existing warnings outside this change; public-test Clippy,
changed-file rustfmt and `git diff --check` passed. No optional-format GPU
execution or historical-driver behavior is claimed.

## Checkpoint — same-thread Present reentrancy coverage

Code commit: `f6b6d93a6a1746882c5384e341d37b95f01bcc6a`.

The controlled D3D11 core fixture now enters the production
`QueuePresentGuard` and invokes core operations on the same thread. It verifies
queue writes and submissions return `PresentationInProgress`, device polling
returns `Timeout`, and buffer/texture creation return the explicit
`DeviceError::PresentationInProgress`. Invalid resource IDs are dropped after
the creation checks. This exercises core behavior under the guard that wraps
native `Present`; it does not make DXGI deliver a window callback during the
test. The existing hidden-window public test still covers configure, acquire,
present, resize and owner-thread rejection.

The first test placement came after an existing injected OOM that deliberately
marks the fixture device lost, so resource creation correctly returned
`DeviceLost`. The reentrancy checks were moved before that failure injection;
the final strict test passed on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), requested/actual FL9_1, `software=false`:

```powershell
cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1'
```

Result: 1 targeted core fixture passed, 89 tests filtered out. Its full
readback workload also passed, including 64-pixel color checks across the
existing draw cases, plus the new guard assertions. The initial expected
failure during the device-lost phase was an ordering error in the new test,
not a backend failure. Rust 1.93 rustfmt and `git diff --check` pass. Core
Clippy passes with three pre-existing warnings in `dx11_fixture_tests.rs`
(one collapsible conditional and two unnecessary slice references); none are
in the new assertions. The full controlled fixture suite was not rerun; the
changed test passed individually, and the previous checkpoint recorded the
full fixture at 90/90.

## Checkpoint — optional R8G8_UNORM sampled upload at forced FL9_1

Code commit `672b6b250d2332eaae0d567f6d6af7f79f8ebbf0` adds a queried
`DXGI_FORMAT_R8G8_UNORM` path for one-mip, one-layer sampled/upload textures.
It is not treated as an FL9_1 guarantee: the selected device must positively
report both `D3D11_FORMAT_SUPPORT_TEXTURE2D` and
`D3D11_FORMAT_SUPPORT_SHADER_SAMPLE`, and unavailable or incomplete query
results remain unsupported. Backend transfer layout uses two bytes per texel.
The path does not expose render-target, copy-source, readback or mip-chain
uses, and the wgpu projection omits `FILTERABLE`.

The strict physical device was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`),
driver `32.0.16.2101`, software=false, requested and selected FL9_1. The native
query was `Value(181024)` (`0x0002c320`). The public test uploaded two
distinct RG texels, sampled the red channel and read back the 8×8 RGBA8 target;
all 64 pixels matched, with 32 left pixels `[64, 0, 0, 255]` and 32 right
pixels `[192, 0, 0, 255]`. The existing R8 baseline in the same test also
matched all 64 pixels. The different red values catch a mistaken one-byte
texel stride in the actual upload and sample path.

Verification used Rust 1.93.1 and Visual Studio 2022 x64 MSVC. Strict hardware
mode was enabled for hardware tests:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/texture.rs wgpu/tests/dx11_public.rs
git diff --check
```

Results: private HAL passed 353/353, including the synthetic optional-query
and two-byte layout tests plus forced-FL9_1 GPU rendering. The public D3D11
suite passed 23/23; the RG8 and R8 image paths both executed on hardware. The
separate optional Float16x2 vertex-format test logged its expected capability
skip. The physical D3D11 adapter-isolation test passed 1/1 and DX12 core passed
86/86. HAL Clippy passed with nine pre-existing warnings outside this change;
public-test Clippy, changed-file rustfmt and `git diff --check` passed. No
historical FL9_1 driver result is implied.

Next, continue the selected-device format and usage audit. Investigate another
bounded FL9_1 texture-format candidate against both the required-format table
and actual device query before implementing it. Keep optional formats
query-gated and the broader core/HAL contract audit active.

## Checkpoint — selected-device sampled-format filterability

Code commit: `4f1ac09a4` (`dx11: report query-backed sampled format filtering`).

For the implemented one-mip sampled/upload formats, a positive
`D3D11_FORMAT_SUPPORT_SHADER_SAMPLE` query now projects wgpu `FILTERABLE` /
`SAMPLED_LINEAR`. The same query already gates the private sampled-resource
path and `sampled_view` classifies the view as filterable from that bit. This
matches Microsoft's definition that the bit enables HLSL `Sample` with any
filter. Formats lacking the bit can be point-sampled in native D3D11 when the
resource format is supported, but this backend still rejects that subset; it
does not advertise point-only resources by inference.

The strict public R8/RG8 test still checks nearest sampling for both formats.
It now also renders RG8_UNORM using a linear sampler from two distinct texels;
all 64 output pixels matched the expected red gradient
`[72, 88, 104, 120, 136, 152, 168, 184]` across each row (tolerance 2). The
SNORM public test verifies `FILTERABLE` format flags from positive selected-
device queries and still uses nearest sampling. No linear SNORM output claim is
made.

The physical adapter was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`),
driver `32.0.16.2101`, software=false, requested and actual FL9_1. It reported
`R8_UNORM=0x0002d3e0` and `R8G8_UNORM=0x0002c320`; both projected
`FILTERABLE`. `R16G16_SNORM=0x00021122` lacks `SHADER_SAMPLE`, and the current
path rejects it. Queries for `R16_FLOAT`, `R16G16_FLOAT` and
`R16G16B16A16_FLOAT` returned `E_FAIL` (`-2147467259`), which is an unavailable
observation, not proof of hardware absence.

Verification used the Rust 1.93.1 toolchain and Visual Studio 2022 x64 MSVC.
The strict hardware commands were run from a VS developer environment with
`WGPU_DX11_REQUIRE_HARDWARE=1`:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
```

Results: private HAL 353/353; public D3D11 23/23; physical adapter-selection
isolation 1/1; DX12 core 86/86. The public suite's Float16x2 vertex-input case
logged one expected capability skip. Six HAL test subcases rejected unsupported
runtime-integer shader constructs and consequently omitted only their follow-up
GPU render; these are intentional negative-coverage paths, not missing-hardware
skips. HAL Clippy passed with nine existing warnings in unrelated test code;
public-test Clippy, rustfmt and `git diff --check` passed. The relevant
capability unit test also passed 1/1. No historical FL9_1 behavior is implied.

The next bounded task is to investigate D3D11's point-only `Sample` behavior
for a resource format whose `TEXTURE2D` bit is present but `SHADER_SAMPLE` is
absent. Start with forced-FL9_1 R16G16_SNORM, whose RTX query is positive for
the resource but lacks the any-filter bit. Verify native texture/SRV creation,
point-sampler execution and public non-filterable projection before deciding
whether to expose the conditional subset.

## Checkpoint — query-backed point-only sampled textures

Code commit: `6c6b28d64` (`dx11: support query-backed point-only texture sampling`).

Microsoft's D3D11 format-support documentation permits point-filtered HLSL
`Sample` when `TEXTURE2D` is supported but `SHADER_SAMPLE` is absent. The
backend now treats those bits separately for its restricted one-mip sampled
upload formats: `TEXTURE2D` enables resource creation and sampling, while
`SHADER_SAMPLE` alone projects `FILTERABLE`. A failed query or missing
`TEXTURE2D` bit still rejects the format.

At forced FL9_1, the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false) reports `R16G16_SNORM=0x00021122`: the texture bit is present
and the any-filter bit is absent. The private HAL test uploaded padded
R16G16_SNORM data, created an SRV, point-sampled into RGBA8 and matched all
80 output pixels. The public adapter exposes the query-backed
`TEXTURE_FORMAT_16BIT_NORM` feature and reports sampled/upload usage without
`FILTERABLE`. Public core validation rejects a filtering-sampler bind for
this format; a non-filtering layout renders the signed data correctly and
all 64 checked output pixels match.

This surfaced and fixed a narrow core validation gap. For a filterable float
layout, core previously trusted the format's static sample type even when a
downlevel adapter's selected-device format flags omitted `FILTERABLE`. Core
now requires the actual view format capability in that case. The public test
exercises this check. The selected format feature remains query-gated, and
unsupported normalized formats retain empty format usages.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 environment.
The final complete strict HAL suite passed 355/355, including actual RTX
FL9_1 rendering. The strict public D3D11 integration suite passed 23/23; the
R16G16_SNORM point-only output and filter rejection both executed. The full
core D3D11 suite passed 87/87, the physical adapter isolation test passed
1/1, and DX12 core regression tests passed 86/86. HAL Clippy passed with nine
pre-existing test-code warnings; core and public-test Clippy passed without
warnings. Changed Rust files passed rustfmt and `git diff --check` passed.

Initial verification caught stale expectations: synthetic tests still
required `SHADER_SAMPLE` for point-only use, the adapter-isolation test assumed
the public feature set was always empty, and core allowed a filterable layout
for an unfilterable queried format. Those expectations and the core check were
updated; the full final suites above passed. The NVIDIA run establishes the
documented path only on this modern driver explicitly forced to FL9_1; it does
not establish historical driver behavior.

The packed-format audit found the three Windows 8 FL9_x texture formats to be
required at feature level 9_x and query-positive on this RTX. The separate
R16_UNORM, R16_SNORM, R16G16_UNORM and RGBA16_UNORM queries are unavailable;
R16G16_SNORM remains point-only and RGBA16_SNORM remains IA-only. The current
next task is the broader public core/HAL contract audit, with additional
formats gated on positive queries and actual resource/view execution.

## Checkpoint — query-backed packed 16-bit sampled texture family

Code commit: `552a4d019` (`dx11: add query-backed FL9_1 packed texture formats`).

The D3D11 format model now includes feature-gated `Rgb5g6b5Unorm`,
`Rgb5a1Unorm` and `Rgba4Unorm`. Each format has an independent
`CheckFormatSupport` result; the shared native-only
`TEXTURE_FORMAT_16BIT_PACKED` feature is projected only when all three
selected-device sampled paths are available. The backend exposes one-mip,
one-layer `TEXTURE_BINDING | COPY_DST` use and derives filterability from the
selected query. Other usage combinations remain unavailable.

On 2026-11-05, the RTX 3090 Ti (`0x10de:0x2203`, D3D11 driver
`32.0.16.2101`, software=false) was explicitly created at FL9_1. Format query
masks were `B5G6R5_UNORM=0x0026F3A0`, `B5G5R5A1_UNORM=0x000213A0` and
`B4G4R4A4_UNORM=0x000213A0`. The strict public wgpu test performed padded
uploads and separate sampling/readback renders for all three. Every one of
the 64 pixels per output matched; the RGB5A1 and RGBA4 cases also verified
transparent alpha. The selected-device reports were
`COPY_DST | TEXTURE_BINDING` and `FILTERABLE | SINGLE_MIP_ONLY` for all three.

Rust 1.93.1 checks passed for `wgpu-hal --features dx11`, the controlled
`wgpu-core` fixture, and the public `dx11_public` test target. The synthetic
packed-format capability test passed 1/1, packed-format metadata tests passed
2/2, the selected-device capability diagnostic passed 1/1, and the strict
public GPU test passed 1/1 (three format renders, 192 checked pixels). The
complete strict suites passed: private HAL 356/356, public wgpu 24/24, D3D11
core fixture 90/90, and DX12 core 86/86. The full `wgpu-types` suite passed
79 unit tests; its doctests passed 14 with 21 ignored. The packed public case
was run as part of the 24-test strict suite and passed with hardware required.

The strict suite commands were:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The first three ran after setting `WGPU_DX11_REQUIRE_HARDWARE=1` inside the
Visual Studio 2022 x64 developer environment. The DX12 command was a CPU
regression suite and did not use the strict D3D11 variable.

Shared format conversion was kept backend-safe: DXGI and Vulkan receive the
matching packed format mapping; GLES has native upload descriptors but reports
the formats unsupported until its optional-feature projection is implemented;
Metal explicitly returns no format capability. `wgpu-hal` checked with
`dx11`, `dx12` (through the DX12 core test), `gles` and `vulkan` feature sets.
The Metal target cannot be compiled in this Windows environment.

The first DX12 regression compile found the newly added global format variants
missing from the common DXGI conversion match; that compile error was fixed by
adding their exact native mappings before rerunning the complete DX12 core
suite. The initial GLES check was run without MSVC setup and stopped because
`link.exe` was not on PATH; the developer-shell retry compiled successfully.

Clippy passed for HAL, core, the public test target and `wgpu-types`. HAL still
reports nine existing warnings in unrelated test code; core reports one
existing collapsible-if warning; `wgpu-types` reports fourteen pre-existing
unused serde-import warnings without its serde feature. The newly introduced
core test comparison warnings were fixed. Changed-file rustfmt and
`git diff --check` passed. Visual Studio's `VsDevCmd.bat` prints an existing
`Unknown error` banner in this environment, but the configured MSVC compiler
and linker worked for all successful builds.

The selected-device feature projection and the three sampled/upload paths
passed on the RTX 3090 Ti forced to actual FL9_1. Historical FL9_1 drivers and
ARM32 UWP remain unverified. Continue with the broader public core/HAL contract
audit; do not infer support for other formats or usages from these three.

## Follow-up — optional R32 and D32 format audit

Extended the physical-device capability diagnostic to query R32_FLOAT,
R32G32B32A32_FLOAT, D32_FLOAT and D32_FLOAT_S8X24_UINT, because these formats
are plausible next candidates for sampled or depth use. The strict diagnostic
passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), forced
to actual FL9_1. Both R32 formats returned only `IA_VERTEX_BUFFER`
(`0x00000002`), without `TEXTURE2D`; both D32 queries returned `E_FAIL`.
No new resource path was added. The results and Microsoft minimum-level
context are in `CAPABILITIES.md`.

Command, run with `WGPU_DX11_REQUIRE_HARDWARE=1` in the Visual Studio 2022 x64
developer environment:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 fl91_wgpu_projection_matches_the_selected_physical_device -- --nocapture --test-threads=1
```

Continue the public core/HAL contract audit. The format query evidence remains
specific to the modern RTX forced to FL9_1; historical drivers and ARM32 UWP
remain unverified.

## Follow-up — query-backed R8/RG8 color attachments

Code checkpoint: `a6d958664` (`dx11: enable query-gated R8/RG8 color attachments`).

Extended the one-mip R8_UNORM and R8G8_UNORM paths to support color-attachment
usage when the selected device reports `D3D11_FORMAT_SUPPORT_RENDER_TARGET`.
The hardware guarantee, driver query, backend implementation and public
projection remain separate: R8_UNORM is a required FL9_1 texture format but
render-target support is not guaranteed; R8G8_UNORM is itself an optional
format. Both are reported with `RENDER_ATTACHMENT` only after a positive
per-format query and remain single-mip, single-layer, single-sample and
non-blendable. Neither exposes `COPY_SRC` or direct readback. The existing
sampled/upload path is preserved.

The strict public test creates a dual-bound source texture for each format,
clears it through an attachment-only view, samples its contents through a
separate texture-binding view and reads an RGBA8 output. On the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and actual FL9_1, `CheckFormatSupport` returned R8_UNORM=`0x0002d3e0` and
R8G8_UNORM=`0x0002c320`. Both include `RENDER_TARGET` (`0x00004000`), while
the separate `BLENDABLE` bit is not projected. The clear value
`(0.25, 0.75, 0.5, 1.0)` sampled as `[64, 0, 0, 255]` for R8_UNORM and
`[64, 191, 0, 255]` for R8G8_UNORM; all 128 output pixels matched exactly.

The capability model tests positive, missing-query, unsupported-bit and
backend-disabled cases. Synthetic queries also verify that a point-only
format can retain attachment support without `SHADER_SAMPLE`, which controls
filterability separately. The shared private sampled-format test now expects
native target creation only for R8/RG8 when the actual query contains the
render-target bit. The controlled core fixture checks the new R8 projection;
the public GPU test also exercises both formats through core validation.

Verification used Rust 1.93.1 in the Visual Studio 2022 x64 environment. With
`WGPU_DX11_REQUIRE_HARDWARE=1`, the private HAL suite passed 356/356, including
the selected-device capability diagnostic; six shader-execution subcases
printed `SKIP` after separately confirming that unsupported integer-indexing
shaders are rejected. The complete public D3D11 suite passed 25/25, including
the new two-format GPU test; one optional Float16x2 input-format case skipped
because the selected device reports no IA support. The complete controlled
D3D11 core fixture passed 90/90, including adapter projection and public
enumeration, and DX12 core passed 86/86.

The relevant CPU capability subset passed 28/28. Clippy passed for HAL, core
and the public test target. HAL retains nine pre-existing warnings and core
one pre-existing collapsible-if warning; the public test target was warning
free. `git diff --check` passed. Workspace formatting check still reports five
pre-existing formatting differences in `naga/src/back/hlsl/writer.rs`; all
changed Rust files pass formatting checks. No Naga formatting changes were
made. The Visual Studio developer-shell setup continues to print its existing
`Unknown error` banner despite resolving the MSVC tools and running all
commands successfully.

The next task remains the public core/HAL contract audit. These query results
describe only the modern RTX 3090 Ti forced to FL9_1; historical FL9_1 drivers
and ARM32 UWP remain unverified.

## Follow-up — same-surface HWND operation re-entry

Code checkpoint: `be8e58729` (`dx11: test HWND surface operation reentrancy`).

Extended the strict hidden-HWND integration path to hold the surface operation
guard and call the actual HAL `configure` and `acquire_texture` methods again
on the owner thread. Both return the explicit `D3D11 surface operation
already in progress` error without disturbing the configured swap chain. The
same test then continues through acquire, render/readback, present, resize and
recreation.

With `WGPU_DX11_REQUIRE_HARDWARE=1`, the targeted strict run passed both hidden
surface tests on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), explicitly requested and actual FL9_1. Both clear and
clone-sampled paths presented, resized and completed their existing readback
checks. `rustfmt --check --edition 2021` passed for the changed test file, and
HAL Clippy passed with the same nine pre-existing warnings. `git diff --check`
passed.

Command run inside the Visual Studio 2022 x64 developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 fl91_hidden_hwnd -- --nocapture --test-threads=1
```

An attempted direct `IDXGISwapChain2::GetMaximumFrameLatency` diagnostic
returned `DXGI_ERROR_INVALID_CALL` for this swap chain, which is created
without `FRAME_LATENCY_WAITABLE_OBJECT`. The diagnostic was removed without
changing swap-chain behavior. Microsoft's API documentation states that this
getter's default is one; explicit waitable-object behavior remains outside
the current surface contract. The hidden test does not claim to verify the
latency through that unsupported query path.

Independent read-only audits found no confirmed route from a valid advertised
operation into a HAL panic and no confirmed public render-pass/surface contract
violation. Continue the broader public core/HAL audit; historical FL9_1 and
ARM32 remain unverified.

## Follow-up — query-gated B5G6R5 color attachment

Code checkpoint: `4b013795f` (`dx11: enable query-gated RGB565 render attachments`).

Extended the selected-device color-attachment subset to packed B5G6R5. The
capability predicate requires the existing `TEXTURE_FORMAT_16BIT_PACKED`
support, the format's `TEXTURE2D` query, the backend color-target path and a
positive `RENDER_TARGET` query. It adds no `COPY_SRC` or blendability, even
though the selected RTX query also reports the `BLENDABLE` bit. RGB5A1 and
RGBA4 remain sampled/upload-only.

The first strict execution exposed a native texture-helper whitelist that
still rejected RGB565 render targets after public/core validation had admitted
them. Adding B5G6R5 to that guarded native path fixed creation. A public test
now draws into R8_UNORM, R8G8_UNORM and RGB565 attachments, samples each into
RGBA8, and validates all 64 pixels within one UNORM code value. The
first observed pixels were `[66, 0, 0, 255]`, `[66, 129, 0, 255]` and
`[66, 130, 132, 255]`.

On the physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested
and actual FL9_1, the per-format masks were R8=`0x0002d3e0`,
RG8=`0x0002c320` and B5G6R5=`0x0026f3a0`; all report `RENDER_TARGET`. The
public strict test passed 1/1, the synthetic capability test passed 1/1, and
the controlled strict core fixture passed 1/1 (89 filtered tests). Full strict
regressions then passed: private HAL 357/357, public D3D11 25/25, controlled
core fixture 1/1 and DX12 core 86/86. The hardware and fixture commands used
Rust 1.93 in the Visual Studio 2022 x64 environment; the D3D11 runs set
`WGPU_DX11_REQUIRE_HARDWARE=1`. The HAL and core Clippy runs passed with nine
existing HAL warnings and one existing core warning; public Clippy was clean.
Changed-file rustfmt and `git diff --check` passed. `VsDevCmd.bat` continues to
print its existing `Unknown error` banner while successfully configuring
MSVC. Historical FL9_1 devices and ARM32 remain unverified.

The full public suite initially found the older packed-format upload test
still asserting that RGB565 could never be a render target. It now requires
render-target usage for RGB565, keeps RGB5A1/RGBA4 sampled-upload-only, and
continues to assert no copy-source use. The corrected full suite passed.

Exact verification commands, using rustc/cargo 1.93.1:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --lib --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib --tests
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/texture.rs wgpu/tests/dx11_public.rs
git diff --check
```

## Follow-up — rendered narrow-format copy/readback

Code checkpoint `52b573401` extends the selected-device `COPY_SRC` proof to
the combined `RESOURCE | COLOR_TARGET | COPY_SRC` usage for R8_UNORM,
R8G8_UNORM and B5G6R5. The public test renders a constant color into each
attachment, samples the result into RGBA8, and also copies the native attachment
to a nonzero-offset readback buffer with 256-byte rows. It checks all 64
sampled pixels, all 64 raw texels, the offset prefix and row padding. This
covers contents produced by rendering, in addition to the prior partial-copy
test that began from uploads.

R8's sampled and raw red values were `[66]`; RG8 sampled `[66, 130]` while its
native copied bytes were `[66, 129]`; RGB565 decoded to `[66, 130, 132]` and
its packed bytes matched the corresponding 5:6:5 quantization. The raw RG8
green check allows a one-code difference from the RGBA8 sampled result, so it
checks every texel without hard-coding the RTX driver's intermediate rounding.
Follow-up code checkpoint `2d3520c39` applies this tolerance and reruns the
strict suite.

Strict hardware mode passed the focused rendered-attachment test and the full
public suite, 26/26, on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, physical adapter, software=false), requested and actual
FL9_1. One optional Float16x2 input branch was visibly skipped because the
selected FL9_1 device did not report IA support; no hardware test was skipped.
The combined-usage synthetic capability case passed 1/1. The targeted
strict HAL suite had passed 359/359 immediately before this test-only
extension; HAL and public Clippy passed, with nine pre-existing HAL warnings
and no public warnings. Rustfmt and `git diff --check` passed.

Exact strict command, run with Rust 1.93.1 in the Visual Studio 2022 x64
environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 narrow_optional_copy_sources_require_render_target_support_and_backend_paths -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --lib --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
```

The tested format masks remain R8=`0x0002d3e0`, RG8=`0x0002c320` and
B5G6R5=`0x0026f3a0`; each has `RENDER_TARGET`. These results apply to the
modern RTX forced to FL9_1 and do not verify historical FL9_1 drivers or ARM32
UWP. Next audit core-valid read-only depth/stencil render-pass combinations.

## Follow-up — public D24S8 read-only pass aspects

Code checkpoint `3a1b84bd0` expands the public D24S8 transition test to cover
all three core-valid read-only shapes without a sampled alias: both aspects
read-only, depth read-only with stencil writable, and stencil read-only with
depth writable. Each mixed case uses a later depth/stencil comparison to
verify the read-only aspect stayed unchanged while the writable aspect took
the expected update. Error scopes verify that a depth-writing pipeline is
rejected in a depth-read-only pass and a stencil-writing pipeline is rejected
in a stencil-read-only pass.

The first targeted run exposed only an incorrect expected-image classification
for an existing stencil-discard snapshot; that original image correctly stays
blue. After fixing the oracle, the focused strict test passed 1/1 and the full
strict public D3D11 suite passed 26/26. The expanded test checks 512 pixels
across eight snapshots on the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), explicitly requested and actual FL9_1. The
suite printed one capability skip for optional Float16x2 IA support; no
hardware execution was skipped. The D3D11 projection still omits
`READ_ONLY_DEPTH_STENCIL`, so simultaneous sampling of the DSV remains
unsupported. HAL code and capabilities were unchanged.

The public test crate passed Clippy without warnings. Rustfmt and
`git diff --check` passed.

Exact commands, run with Rust 1.93.1 in the Visual Studio 2022 x64 environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d24s8_load_clear_discard_transitions_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

## Follow-up — public colorless D24S8 depth prepass

Code checkpoint `78b52faf1` adds a public wgpu depth-only render pass with no
color attachments. It clears D24S8 depth/stencil, writes near depth without a
fragment shader, then starts a color pass and draws far geometry with a Less
comparison. All 64 pixels remain blue, proving the prepass affected later
depth testing. The existing D24S8 transitions now check 576 pixels across nine
snapshots, including read-only aspects and colorless rendering.

The exact-tree strict public run passed 26/26 tests using Rust 1.93.1 and the
Visual Studio 2022 x64 environment. The physical adapter was NVIDIA RTX 3090
Ti (`0x10de:0x2203`), driver `32.0.16.2101`, software=false, requested and
actual FL9_1. No hardware execution skipped; the optional Float16x2 IA branch
printed its explicit capability skip. The public test crate's Clippy,
rustfmt and diff checks also passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: exercise transient attachments through public wgpu/core/HAL with valid
clear/discard operations and verify the supported non-persistent semantics.
Continue the broader render-pass and core/HAL contract audit afterward.

## Follow-up — public transient attachment passes

Code checkpoint `d224dcd3e` adds strict public wgpu coverage for the existing
transient attachment implementation. It creates transient RGBA8 and D24S8
resources, rejects sampled transient usage, color LOAD/STORE, and depth LOAD,
then submits a transient color clear/draw/discard pass and a transient D24S8
clear/draw/discard pass. In the latter, a near green draw writes depth and a
far red draw fails; all 64 pixels in the ordinary stored color target are
green. Discarded transient data is not read back because public usage forbids
preserving it. These D3D11 textures use ordinary allocations; no memoryless
behavior or memory savings are claimed.

Follow-up commit `bbceec53d` adds public negative cases for read-only transient
depth and a render pass with no attachments. Both report validation errors on
encoder finish and do not reach the HAL. The test summary now states those
rejections along with invalid transient LOAD/STORE and sampled usage.

Invalid render-pass descriptor errors are deferred by core until the parent
encoder is finished. The negative checks therefore finish isolated encoders
before popping their error scopes. The first focused run initially appeared to
accept transient LOAD because the test popped the scope before finishing; after
following core's error path and correcting the test, the strict focused test
passed 1/1. After the negative validation extension, the full public suite
passed 27/27. Hardware was the physical
NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false),
requested and actual FL9_1. No GPU test skipped; the optional Float16x2 IA
branch printed its explicit unsupported-capability skip. Public Clippy,
rustfmt and diff checks passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_transient_attachments_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: exercise public `LoadOp::DontCare` on color and depth attachments with
full writes that make the resulting color deterministic. Continue the
remaining render-pass/core-HAL contract audit afterward.
