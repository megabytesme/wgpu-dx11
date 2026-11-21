# D3D11 FL9_2 development path

FL9_2 remains **INCOMPLETE**. Development began at wgpu
`aa7b65c43ef008d0a92d20fcdf9f10fa0883f157` and Horizon Nightly product
`5c26a8060c1cf2dce20ee5707e84699ed0879e5c`. The product's upstream Nightly pin
is `0a875da91dbb0fdcb7ba8c42c11341aa229af51a`. The FL9_1 prerequisite gate
and the combined-candidate review are pending.

## Adapter selection

Set `WGPU_DX11_FEATURE_LEVEL=9_2` before enumerating D3D11 adapters to opt in.
Each eligible hardware adapter receives a one-element feature-level list.
Device creation must return the requested level or the adapter is skipped.
The D3D11 adapter reports `FL9_2` in `driver_info` and the HAL exposes the
actual native feature level independently. Software adapters are skipped.

`WGPU_DX11_FEATURE_LEVEL=9_1` and the existing `WGPU_DX11_FORCE_FL9_1=1`
select exact FL9_1. Invalid values and conflicts reject D3D11 enumeration.
With both overrides unset, the existing hardware preference order and FL9_1
capability projection apply, including on hardware selected at FL9_2 or
higher. FL9_2 projection requires explicit opt-in. An actual FL9_2 device
reports a maximum 3D texture dimension of 256 in either mode. The desktop
runtime accepted 256 and rejected 512 at that level.

## Capabilities

- The FL9_2 projection adds separate alpha blending and anisotropy up to 16.
  Ordinary sampler LOD and texture/view checks continue to apply.
- Distinct front and back stencil state is exposed on the RTX 3090 Ti.
  The Surface Pro X Adreno 680 (DXGI `4d4f4351:41333430`) produced an incorrect
  back-face result, so the adapter and HAL reject asymmetric stencil on that
  device. The restriction applies across driver versions until another
  version is verified.
- With individual positive format queries, `R16Unorm`, `Rg16Unorm`, and
  `Rgba16Unorm` support filterable sampling and uploads for single-sample,
  single-mip 2D textures. Each format uses its own texel width in zeroing,
  binding and transfer.
- An empty native occlusion query must resolve to zero before the adapter
  publishes `DownlevelFlags::OCCLUSION_QUERY`. The encoder records native
  begin/end operations. Resolve produces the zero/nonzero result permitted
  by WebGPU and handles unavailable slots through Core's query initialization.
  A bounded native read can stall the CPU during query resolve.
- Both tiers use FXC `vs_4_0_level_9_1` and `ps_4_0_level_9_1`.
  The existing shader guards and emulation continue to apply. Native 10Level9
  format support flags can describe operations that these FXC targets reject.
  [Microsoft compiler target reference](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets)
- Native primitive, vertex-index and repeat ceilings remain level-specific.
  A finite repeat ceiling still limits public sampling address modes.
  [Microsoft Direct3D feature-level table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro)

## Native DX11 interop

DX11 HAL callers can create a device-owned sampler through
`Device::create_mirror_once_sampler` and select Mirror Once per axis.
The sampler applies reflection around zero and clamps at one. It binds
through the typed HAL. The portable sampler address modes are unchanged.
[Microsoft D3D11 address modes](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_texture_address_mode)

`Device::create_native_occlusion_query` creates an owned count query.
Record a begin and end in the same native command buffer with
`CommandEncoder::{begin,end}_native_occlusion_query` and submit it once.
`Device::native_occlusion_result` returns the 64-bit sample count after
submission. Reset returns a fresh generation, while command buffers and
readers can retain the previous generation. Each operation checks device
ownership. Record consecutive native and public queries in separate
intervals. A wgpu encoder used through `as_hal_mut` must use HAL commands
for the entire encoding sequence. Public wgpu occlusion resolves remain
Boolean.

See the [contract inventory](../deliverables/dx11-levels/FL9_2/contract.csv),
[test results](../deliverables/dx11-levels/FL9_2/tests.json),
[hardware results](../deliverables/dx11-levels/FL9_2/hardware.json),
[performance results](../deliverables/dx11-levels/FL9_2/performance.csv), and
[open requirements](../deliverables/dx11-levels/FL9_2/blockers.md).

## Reproduce desktop validation

Use a Visual Studio Developer PowerShell for the intended target architecture
and a lane-specific Cargo target directory. Run test binaries on a physical
adapter under the shared desktop GPU lock.

```powershell
$env:CARGO_TARGET_DIR = Join-Path $env:TEMP 'horizon-wgpu-fl92-cargo'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
Remove-Item Env:WGPU_DX11_FORCE_FL9_1 -ErrorAction SilentlyContinue
foreach ($tier in '9_2', '9_1') {
    $env:WGPU_DX11_FEATURE_LEVEL = $tier
    cargo test --locked --offline -p wgpu --no-default-features --features std,dx11,wgsl --test dx11_fl92 --test dx11_fl92_r16unorm -- --ignored --nocapture --test-threads=1
    if ($LASTEXITCODE) { throw "Strict D3D11 test failed for $tier" }
}
```

Add `--target i686-pc-windows-msvc` for a 32-bit process. Clear both overrides
for the default-selection check. Build the
[standalone native probe](../deliverables/dx11-levels/FL9_2/native_probe.cpp)
with `cl /W4 /WX /EHsc /MT native_probe.cpp /link d3d11.lib dxgi.lib` in a
temporary directory. Pass `9_2` or `9_1` to the executable. Its individual
HRESULTs and format support masks describe native descriptor and allocation
checks. The wgpu fixture verifies draw and readback.

The `dx11_fl91_scene` fixture runs the same textured, D16 depth-tested
32×32 cube at either tier. Set `WGPU_DX11_SCENE_BENCH=1` to time one and 128
indexed draws per submission. Each run warms up five submissions and measures
25. The timer includes command recording, submit, and the completion wait.
Display presentation, game throughput, and peak memory remain open.

The RTX 3090 Ti x64 and x86 focused runs at
`6224078a56ea049d7f98f62d237b84db7d1b32ff` verified the native samplers,
numeric queries, three UNORM16 formats, and the indexed and primitive count
boundaries. The boundary fixture checks index 1,048,575, rebasing at
1,048,576, safe rejection of an out-of-bounds index, and preserved output
after batching 1,048,576 point primitives. The x64 hidden HWND resize and
synthetic surface loss fixtures passed. The x86 FL9_1 regressions surfaced
two strict hidden-window presentation failures. They reproduced in an
isolated rerun when Present returned the allowed occluded status and the
strict fixture required a queued frame. The x64 FL9_1 suite passed all 557
selected tests. See the per-run records in [tests.json](../deliverables/dx11-levels/FL9_2/tests.json).

The x86 visible HWND control at exact FL9_1 presented all 30 requested
textured, indexed, depth-tested frames. The earlier x86 failures occurred
when strict mode required DXGI to queue frames on a hidden HWND. The old
fixture passed the follow-up retry, confirming that the failure is
intermittent. The revised fixture checks the actual hidden Present result
and pending-frame state independently, then shows the same HWND without
activating it. Strict mode requires three queued Presents on the visible
window, with the existing draw and resize readbacks. Both FL9_1 cases and
the FL9_2 case passed on x64 and x86 at the exact requested tiers.
The current x86 FL9_1 HAL run passed all 555 selected tests, including
both revised HWND cases.

The anisotropic minification fixture in
`353d9dda4510a184bbeef559e36671453f1d6eea` creates samplers at 2, 4, 8
and 16 and rejects 17 at exact FL9_2. At eight texels per pixel along the
major axis, mean absolute deviation from mid-gray was 1 at anisotropy 16
and 128 at anisotropy 2. Both x64 and x86 produced the same readback. The
existing matched FL9_1 control at two texels per pixel gave 1 at anisotropy
2 and 128 at anisotropy 1 on both architectures.

Selected CTS cases use the exact-tier verification in
`383f40a8c8c936dc715cd7f8773c488b04910b67`. This runner checks the
requested FL9_1 or FL9_2 marker together with the physical adapter identity.
The earlier runner rejected a correctly selected FL9_2 device before
executing cases. After the repair, all four selected cases passed at each
level with zero skips.

Three same-adapter desktop benchmark pairs exercised both forced tiers
with identical texture, camera, render extent, draw count, and completion
pacing. Median-of-run-median submission times for one draw were 0.353 ms
at FL9_1 and 0.392 ms at FL9_2. At 128 draws, they were 1.926 ms and
1.719 ms respectively. Timing varied across pairs. These are debug
offscreen results.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:WGPU_DX11_FEATURE_LEVEL = '9_1'
cargo test --locked --offline -p wgpu-hal --no-default-features --features dx11 --lib dx11:: -- --nocapture --test-threads=1
$env:WGPU_DX11_FEATURE_LEVEL = '9_2'
cargo test --locked --offline -p wgpu-hal --no-default-features --features dx11 --lib fl92_anisotropic_minification -- --nocapture --test-threads=1
cargo test --locked --offline -p wgpu-hal --no-default-features --features dx11 --lib fl92_hidden_hwnd_clone_sampling_presents_and_resizes -- --ignored --nocapture --test-threads=1
cargo test --locked --offline -p wgpu --no-default-features --features std,dx11-test-fixture --test dx11_surface_error_fixture -- --nocapture --test-threads=1
cargo xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,basic:empty:*' 'webgpu:api,operation,rendering,basic:clear:*'
```

Use the matching checkout at CTS revision
`3fdd32363410f2bd4e3e2f046980967e6abc27e6`. Repeat the synthetic surface
fixture and selected CTS cases with `WGPU_DX11_FEATURE_LEVEL=9_1`.

## Integration

The runtime commits on `codex/dx11-fl92-nov21` are
`912fc25d497f872e744185a2564555647e4e2b46` and
`6224078a56ea049d7f98f62d237b84db7d1b32ff`. The separate CTS harness
commit is `383f40a8c8c936dc715cd7f8773c488b04910b67` and the anisotropy
fixture commit is `353d9dda4510a184bbeef559e36671453f1d6eea`.

Review the runtime diff with
`git diff aa7b65c43ef008d0a92d20fcdf9f10fa0883f157 6224078a56ea049d7f98f62d237b84db7d1b32ff`.
The existing FL9_1 remediation proposal overlaps in capabilities, textures,
the surface fault fixture, and the changelog. Retain the accepted lower-tier
format and copy changes while applying the gated FL9_2 paths and tests.

Keep the Horizon Nightly fork pin unchanged until Mission Control publishes
the exact FL9_1 certification SHA. Then review the experimental commits
relative to `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`, replay the reviewed
changes on the certified SHA in a separate integration worktree, and submit
the combined candidate's full SHA and diff to Mission Control. Rerun forced
FL9_1, legacy fallback and automatic selection on that candidate before
considering promotion. Signed UWP builds must use isolated package identities.
Check exact device level, real resize and presentation, physical loss and
recovery, gameplay output, and matched performance on each required device.

For the Adreno stencil investigation, compile
[`stencil_native_probe.cpp`](../deliverables/dx11-levels/FL9_2/stencil_native_probe.cpp)
with `cl /nologo /W4 /WX /O2 stencil_native_probe.cpp /link d3d11.lib dxgi.lib d3dcompiler.lib`
in an ARM64 Visual Studio Developer Command Prompt. Under the Surface lock,
run it against the physical Adreno adapter. It records the exact level,
both winding conventions, symmetric and culling controls, and depth-disabled
and color-enabled cases. All ten original cases passed on the desktop RTX
at native FL9_2. On the Adreno 680 with driver `27.20.2160.0`, all six
symmetric controls passed and all four asymmetric cases incorrectly lit
128 extra pixels at exact native FL9_2. The failure reproduced with both
front winding conventions and with depth disabled or color writes enabled.
It occurs in a direct D3D11 program before wgpu state translation.

The extended probe verifies per-primitive complementary culling with
symmetric stencil state on that driver. Alternating front and back
operations over overlapping geometry leaves stencil value two. Batching
by face leaves value four. Any HAL workaround must preserve primitive and
instance order and cover indexed topology, clipping, and pipeline state.
The Adreno capability remains disabled while full runtime coverage is open.

The signed ARM32 native negotiation probe
`MCHorizonUWP.Fl92NativeProbe_1.0.0.0_arm__4pd4jeb6snrqw` ran on the Lumia
through its paired Device Portal. The helper verified the signed package
hash, activated the installed isolated probe, and read the fresh
`LocalState/fl92-native-probe.txt` report. Both singleton requests returned
`S_OK` with the requested device levels `0x9100` and `0x9200`. The
physical adapter was `4d4f4351:46363432` with DXGI flags zero. The package
verifies device negotiation. ARM32 HAL rendering and CoreWindow
presentation still require coverage.

The [standalone volume probe](../deliverables/dx11-levels/FL9_2/volume_native_probe.cpp)
uses independent width, height, and depth tests. Build with
`cl /nologo /W4 /WX /EHsc /MT /O2 volume_native_probe.cpp /link d3d11.lib dxgi.lib d3dcompiler.lib`
and pass the exact tier and adapter index. On Adreno at exact FL9_2,
RGBA8 and R16 UNORM passed padded writes, final-texel updates, and shader
readback with 256 texels on each axis. Every corresponding 512 and 1024
allocation returned `E_INVALIDARG`. Keep the projection at 256. The
API-specific 512 entry and the resource table remain inconsistent.

Commit `6b358a4359d695bd613efd017d0f23928ecc7927` extends the experimental
sampled-upload contract to single-mip R16 UNORM volumes when the selected
driver reports both 2D and 3D sampling support. The public fixture verified
queue and encoder upload to the back slice, zero-initialized front slice,
and linear interpolation between slices at exact FL9_2 on x64, x86, and
ARM64. The exact FL9_1 controls rejected the capability on all three
architectures. Volume mips, copy-source use, and other UNORM16 volume
formats remain outside this projection.
