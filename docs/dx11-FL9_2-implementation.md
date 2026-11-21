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
- With positive format queries, `R16Unorm` supports filterable sampling and
  uploads for single-sample, single-mip 2D textures.
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

The desktop focused x64 and x86 runs used pre-commit snapshots. The ARM64
focused rerun used the exact implementation commit and verified direct query
resolve, linear interpolation, and the Adreno stencil rejection. The final
desktop focused rerun, full HAL regression, hidden HWND resize, and synthetic
device-loss runs need a desktop GPU slot. The pinned CTS runner built and
its selected hardware cases also await that slot.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:WGPU_DX11_FEATURE_LEVEL = '9_1'
cargo test --locked --offline -p wgpu-hal --no-default-features --features dx11 --lib dx11:: -- --nocapture --test-threads=1
$env:WGPU_DX11_FEATURE_LEVEL = '9_2'
cargo test --locked --offline -p wgpu-hal --no-default-features --features dx11 --lib fl92_hidden_hwnd_clone_sampling_presents_and_resizes -- --ignored --nocapture --test-threads=1
cargo test --locked --offline -p wgpu --no-default-features --features std,dx11-test-fixture --test dx11_surface_error_fixture -- --nocapture --test-threads=1
cargo xtask cts --skip-checkout --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,basic:empty:*' 'webgpu:api,operation,rendering,basic:clear:*'
```

Use the matching checkout at CTS revision
`3fdd32363410f2bd4e3e2f046980967e6abc27e6`. Repeat the synthetic surface
fixture and selected CTS cases with `WGPU_DX11_FEATURE_LEVEL=9_1`.

## Integration

The implementation commit is `912fc25d497f872e744185a2564555647e4e2b46` on
`codex/dx11-fl92-nov21`. Review its diff with
`git diff aa7b65c43ef008d0a92d20fcdf9f10fa0883f157 912fc25d497f872e744185a2564555647e4e2b46`.

Keep the Horizon Nightly fork pin unchanged until Mission Control publishes
the exact FL9_1 certification SHA. Then review the experimental commits
relative to `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`, replay the reviewed
changes on the certified SHA in a separate integration worktree, and submit
the combined candidate's full SHA and diff to Mission Control. Rerun forced
FL9_1, legacy fallback and automatic selection on that candidate before
considering promotion. Signed UWP builds must use isolated package identities.
Check exact device level, real resize and presentation, physical loss and
recovery, gameplay output, and matched performance on each required device.

The signed ARM32 native negotiation probe is
`MCHorizonUWP.Fl92NativeProbe_1.0.0.0_arm__4pd4jeb6snrqw`. The paired
Device Portal returned HTTP 503 and later timed out at device-family
preflight. The package and deployment helper are listed in
[hardware.json](../deliverables/dx11-levels/FL9_2/hardware.json). Once the
paired Portal responds, acquire the Lumia lock and run the helper on
Ben-Desktop in Windows PowerShell 5.1. It verifies the package hash,
waits for registration, activates the app, then reads the fresh
`LocalState/fl92-native-probe.txt` report. The FL9_2 entry needs a
successful HRESULT and returned level `0x00009200`. Record the FL9_1
control and the app's live DXGI identity with the result.
