# Independent candidate evidence review

Both FL9_1 and FL9_2 remain **INCOMPLETE**. This review checked Git provenance, local file hashes and test behavior. It did not run a GPU or deploy a package. The higher-tier worktrees still had the research pin as HEAD when checked.

## FL9_1 corrected fixture rerun

The evidence commit is `6adb6422278c52794c8f6e8040659b44a50dfb0d`. Recomputed SHA-256 for 82 path-and-digest records in its test manifest and matched each. The full x64 suite receipt attributes 178 passing functions and four logged subcase skips to `dbdc67913fcc858924ca553f68764fd8b432bd70` and records binary SHA-256 `70c5004985aa49d332e9db56c05a94b9fd3f7b9a85c63ac1715cc4e6e2a53b77`. The executable currently present at that receipt's path has a different hash that matches the later two-test readback recheck. The current x86 executable also matches its later recheck. The recheck SHA is `9bf6b497bef0c03dcfdc71e5e2bf942752714f34` for both architectures. Attribute newly captured results to that fixture revision. [Committed receipt](<C:/dev/personal/wgpu-dx11-FL91-cert-Nov21/deliverables/dx11-levels/FL9_1/tests.json>)

Under the desktop GPU lock, verify and run the current binaries with the exact fixture filter. Capture stdout and assert two tests ran, the expected physical adapter was used and the reported feature level is FL9_1. A full suite on either present binary would create fresh evidence for the later revision.

```powershell
$fl91 = Join-Path $env:TEMP 'wgpu-fl91-cert-Nov21'
$fl91Binaries = [ordered]@{
    'candidate-target/debug/deps/dx11_public-0b8a24bd95fa9fc2.exe' =
        '6B59EC06D6995D47347F65AF2F01F7381A9C9A4E13C4A44279EFACE695C4483A'
    'cross-target/i686-pc-windows-msvc/debug/deps/dx11_public-eb5e2333a8413fbc.exe' =
        '958D535A9FADF7E9E839731A37DE873CEFDDB2F0DB8142F93CEC945074A7F2B6'
}
$env:WGPU_DX11_FORCE_FL9_1 = '1'
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
foreach ($fl91Binary in $fl91Binaries.GetEnumerator()) {
    $fl91Exe = Join-Path $fl91 $fl91Binary.Key
    if ((Get-FileHash -LiteralPath $fl91Exe).Hash -ne $fl91Binary.Value) {
        throw "Unexpected binary hash for $fl91Exe"
    }
    & $fl91Exe --exact --test-threads=1 --nocapture public_dx11_r8_rg8_rgb565_render_attachments_at_fl91 public_dx11_fragment_3d_texture_load_at_fl91
    if ($LASTEXITCODE -ne 0) { throw "FL9_1 recheck failed for $fl91Exe" }
}
```

## FL9_2 stencil and provenance

- The committed manifest at `32177ff3e202566f798201ccb1d4db2aaf246ad1` labels its focused desktop runs pre-commit snapshots. Local x64 and x86 executables exist, including multiple x64 versions. The committed desktop receipts lack a hash linking those binaries to implementation `912fc25d497f872e744185a2564555647e4e2b46`. Treat the local binaries as historical controls. The active FL9_2 worktree also has uncommitted code, so preserve it and reproduce from an exact archive or fresh detached checkout. [Committed receipt](<C:/dev/personal/wgpu-dx11-FL92-Nov21/deliverables/dx11-levels/FL9_2/tests.json>)
- The Surface ARM64 candidate log exists at `%TEMP%/fl92-nov21-arm64-candidate.log` with SHA-256 `E3D42DC0304F8B9955EF9CB15F148B5138D5A0B2B2DDD7FB0AF989F825B5D202`. The raw log carries the ARM64 `dx11_fl92` binary digest `AC8F1669C0BFEA3C5DC76E546FA3A382F04A63B4E3DAAD219D610F258DA07711`. The committed manifest records this Surface source archive digest `f5a42a34fa61ad72c108b1a8ef1f93ead65715c8326e686e6e04c39f93cdc8f8` for the candidate build. The Surface binary and source archive were recorded on Ben-Tablet. This review did not retrieve or rehash them. The log shows real public occlusion, indexed draw pixels and sampled `R16_UNORM` at exact physical FL9_2, plus explicit asymmetric stencil **rejection** at both selected levels on Adreno 680.
- The diagnostic log at `%TEMP%/fl92-nov21-arm64-stencil-diagnostic.log` hashes to `982C1EC73668D8E62B93B62410164CA5700421D845202B5A8728C126C769F347` and reports diagnostic binary digest `BE3B007C0FE38EBF8AAD6D80D2864D9148ACD870DDF4336E43EA4C60379FAF0B`. Physical Adreno 680 driver `27.20.2160.0` at exact FL9_2 produced red `[255, 0, 0, 255]` at back-face pixel `(4, 0)` where the fixture expected black `[0, 0, 0, 255]`. The log lacks a source revision for this diagnostic build. The committed candidate source explicitly withholds `TWO_SIDED_STENCIL` on this adapter, verifies the Core validation message, and returns before the asymmetric draw. Its result establishes safe rejection, so correct asymmetric stencil drawing and the distinction between driver behavior and backend state lowering remain open. [Candidate fixture source](<C:/dev/personal/wgpu-dx11-FL92-Nov21/wgpu/tests/dx11_fl92.rs:656>)
- The signed Lumia ARM32 package's local SHA-256 matches the reported `90B8B94CB29172A3D83A89FAE85982D6C598D78EFCA984E22298088483BDF596`. The package is a native negotiation probe. The reported attempts reached Portal HTTP 503 and later timed out before installation. [Hardware receipt](<C:/dev/personal/wgpu-dx11-FL92-Nov21/deliverables/dx11-levels/FL9_2/hardware.json>)

On the exact `912fc25d497f872e744185a2564555647e4e2b46` source, build with an isolated `CARGO_TARGET_DIR`. `--no-run` can prepare the artifacts before taking the GPU lock. Hash the built executables before running `dx11_fl92` and `dx11_fl92_r16unorm` with the same settings under the lock. Run strict FL9_1 as the paired regression and append `--target i686-pc-windows-msvc` before the harness argument delimiter for the x86 comparison.

```powershell
Remove-Item Env:WGPU_DX11_FORCE_FL9_1 -ErrorAction SilentlyContinue
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
foreach ($fl92Level in '9_2','9_1') {
    $env:WGPU_DX11_FEATURE_LEVEL = $fl92Level
    cargo test --locked --offline -p wgpu --no-default-features --features std,dx11,wgsl --test dx11_fl92 --test dx11_fl92_r16unorm -- --nocapture --test-threads=1
    if ($LASTEXITCODE -ne 0) { throw "Focused DX11 suite failed at $fl92Level" }
}
```

For an independent Adreno reproduction, pair a standalone exact FL9_2 stencil draw with the production fixture on the same driver. Use opposite winding, explicit front-only replace and back-face keep, known initial stencil, a second equality-tested color pass and a winding-swap control. Compare the full rendered mask or independent stencil reads. Repeat with symmetric stencil and require identical legal output. A passing public rejection run on that driver leaves the positive path open. A desktop positive control covers the desktop driver.

## Higher-tier receipts

Coordination had FL9_3, FL10_1 and FL11_0 status files for isolated preparation. Actual FL9_3 through FL11_1 worktree HEADs still matched `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`. There was no higher-tier candidate executable path, content hash, execution log or certification gate registered to rerun. The previous forced-level native graphics and compute receipts cover the reference harness workloads. [Reference receipts](hardware-probes.json)

## Independent RTX rerun

The two selected FL9_1 x64 fixtures passed with the verified `9bf6b497bef0c03dcfdc71e5e2bf942752714f34` binary. Neither was ignored. [FL9_1 log](evidence/desktop-x64-fl91-reviewed.txt)

The five opt-in FL9_2 x64 tests passed from an immutable archive of `912fc25d497f872e744185a2564555647e4e2b46`. I built `dx11_fl92` and `dx11_fl92_r16unorm` offline and used `WGPU_DX11_FEATURE_LEVEL=9_2`, `WGPU_DX11_REQUIRE_HARDWARE=1`, and `--ignored --nocapture --test-threads=1`. The four main tests executed occlusion resolve, overlapping vertex attributes and indices above 65,534, blend and scissor, and asymmetric stencil. The fifth exercised R16_UNORM initialization, upload, copy, and filtered readback. The blend and limits receipt rejects width 257 at the projected 3D limit of 256. A native 512-axis volume boundary fixture is still needed to reconcile that conservative projection with the 10Level9 application table. [FL9_2 log](evidence/desktop-x64-fl92-candidate.txt) · [R16_UNORM log](evidence/desktop-x64-fl92-r16unorm-candidate.txt)
