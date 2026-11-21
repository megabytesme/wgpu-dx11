# D3D11 device compatibility

On 21 November 2026, Ben-Desktop and a signed Surface Pro X ARM64 AppContainer each created all seven requested D3D11 levels on physical hardware. Each exact-level device passed an 8×8 native render and pixel readback. The four FL10_0–FL11_1 devices on each machine also passed a four-word native dispatch and readback. The Lumia 950 XL ARM32 package built and signed. A repeated Device Portal connection timeout blocked deployment and measurement.

These results establish native offscreen smoke coverage. The roadmap marks every public wgpu tier **INCOMPLETE** while the accompanying conformance and hardware gates remain open. The native executable does not load the Rust backend. Its AppContainer launch opens a CoreWindow and runs the probe on a worker, with no DXGI swapchain or Present call. The results leave HWND, CoreWindow, SwapChainPanel, HDR, device recovery, and Horizon performance to their own tests.

## Tested devices

| Device and run | OS, driver, and identity | Package or binary |
| --- | --- | --- |
| Ben-Desktop, Win32 x64 | Windows 11 build 26300, driver `32.0.16.2101`, NVIDIA GeForce RTX 3090 Ti. DXGI reported vendor/device `10de:2203`, subsystem `161810de`, flags `0`, LUID `00000000:00014a80`. | Console EXE SHA-256 `3282D42E68951E593CF95B66A8EA7348D4C48AE2E1424E308DC161DBEAC9A372`. The native probe used explicit DXGI enumeration. |
| Ben-Tablet, Surface Pro X SQ1, signed ARM64 | Windows 11 build 26320, driver `27.20.2160.0`, Qualcomm Adreno 680. DXGI reported vendor/device `4d4f4351:41333430`, subsystem `30383138`, flags `0`, LUID `00000000:000118b0`. | `MCHorizonUWP.Dx11TierProbe_1.0.0.0_arm64__4pd4jeb6snrqw`, SHA-256 `73ED0652A84BCBF3DB905B50A9040BD221225AA7FC3350D255DC78591B8C15E3`. Signed install status was `Ok`. |
| Lumia 950 XL, signed ARM32 | Historical Adreno 430, driver `11.18.1078.53`. The November 21 tier package has no fresh Lumia DXGI or OS observations. | Development APPX signed and verified. SHA-256 `CB66A29128FC7EDD0E7F58CBC76F6C30DE3130FED0CA8ED05F02CAB7CEC71FEF`. ARMNT PE and AppContainer headers verified. Deployment blocked at Device Portal preflight. |

Separate DxDiag captures reported WDDM 3.2 on the RTX 3090 Ti and WDDM 2.7 on the Adreno 680. DxDiag reported DDI 12 for both adapters. The DDI is a driver-interface diagnostic. The exact D3D11 run tiers come from the creation results and `ID3D11Device::GetFeatureLevel()` below. LUIDs are local to these sessions. The Adreno IDs above are the values emitted by DXGI, including the wider fields.

[Desktop raw output](../deliverables/dx11-levels/evidence/desktop-raw.txt) · [Surface raw output](../deliverables/dx11-levels/evidence/surface-raw.txt) · [Surface signed activation receipt](../deliverables/dx11-levels/evidence/surface-deployment.txt)

## Exact feature-level results

For each PASS in the table, the `D3D11CreateDevice` HRESULT was `S_OK` (`0x00000000`), the API's returned level matched the request, `ID3D11Device::GetFeatureLevel()` separately returned the same value, and `GetDeviceRemovedReason()` was `S_OK` at the end of the case. FL10.x dispatch ran because both devices reported the optional Shader 4.x compute and raw/structured buffers bit at both levels. The FL9.x probe does not request a dispatch. Lumia remains unmeasured across the seven requests.

| Exact request | Native VS/PS target suffix | Ben-Desktop | Surface Pro X UWP ARM64 | Lumia UWP ARM32 |
| --- | --- | --- | --- | --- |
| FL9_1 `0x9100` | `4_0_level_9_1` | Create, 64 pixels PASS | Create, 64 pixels PASS | BLOCKED |
| FL9_2 `0x9200` | `4_0_level_9_1` | Create, 64 pixels PASS | Create, 64 pixels PASS | BLOCKED |
| FL9_3 `0x9300` | `4_0_level_9_3` | Create, 64 pixels PASS | Create, 64 pixels PASS | BLOCKED |
| FL10_0 `0xa000` | `4_0` | Create, 64 pixels, CS 4.0 / 4 words PASS | Create, 64 pixels, CS 4.0 / 4 words PASS | BLOCKED |
| FL10_1 `0xa100` | `4_1` | Create, 64 pixels, CS 4.1 / 4 words PASS | Create, 64 pixels, CS 4.1 / 4 words PASS | BLOCKED |
| FL11_0 `0xb000` | `5_0` | Create, 64 pixels, CS 5.0 / 4 words PASS | Create, 64 pixels, CS 5.0 / 4 words PASS | BLOCKED |
| FL11_1 `0xb100` | `5_0` | Create, 64 pixels, CS 5.0 / 4 words PASS | Create, 64 pixels, CS 5.0 / 4 words PASS | BLOCKED |

Historical product evidence on Adreno 430 includes eight signed offscreen scaling tests at exact FL9_1. A separate full-game run at native FL11_1 used FL9_1 compatibility shaders and presented through the game panel. The forced-FL9_1 full-game attempt cited there reported no new game frame or capture. Keep each result with its package and render path. This evidence does not fill any current ARM32 row. [Signed DRS offscreen report](../../MCHorizonUWP-nightly-2026-11-20/docs/nightly-drs-2026-11-20.md) · [Lumia saved-world report](../../MCHorizonUWP-nightly-2026-11-20/docs/nightly-lumia-drs-2026-11-21.md)

## Probe methodology and interpretation

- [Probe source](../deliverables/dx11-levels/probe/probe.cpp) makes each request with a one-element level array and `D3D11_CREATE_DEVICE_BGRA_SUPPORT`. Win32 enumerates DXGI adapters, excludes adapters flagged software, then passes the selected adapter with `D3D_DRIVER_TYPE_UNKNOWN`. AppContainer uses `D3D_DRIVER_TYPE_HARDWARE` with default adapter selection. Both paths read the live device's adapter through `IDXGIDevice::GetAdapter` and reject a software flag or level mismatch. Using the live adapter matters on FL9.x, where the adapter passed to creation can be a template.
- The draw compiles the exact tier's vertex/pixel targets offline with FXC, creates the stage objects and an input assembler layout, clears an RGBA8 target green and covers it with a magenta triangle. It copies to a staging texture, validates RGBA `255,0,255,255` at all 64 positions using the returned row pitch, and checks the removal reason. The compute shader dispatches one group into a structured UAV and verifies `42,43,44,45` from a staging buffer. These exercise actual GPU execution and data movement on the selected driver. They leave all other render, compute and copy configurations to dedicated cases.
- Raw reports preserve individual HRESULTs and bits from `CheckFormatSupport` and `D3D11_FEATURE_FORMAT_SUPPORT2` on the same 121 format IDs, as well as quality counts at 2×, 4×, 8×, 16× and 32× for queried render/depth formats. Field layout is `format|DXGI ID|support HRESULT|support mask|support2 HRESULT|support2 mask` and `msaa|DXGI ID|samples|HRESULT|quality count`. Option rows include Device1 QI, optional compute/doubles and selected D3D11 options. A query success, returned flag or quality count is an observation. Each advertised use still needs resource/view creation and an operation such as sampled pixels or a real resolve with readback.
- Read all `request` and `operation` rows before interpreting the aggregate `overall=PASS` line. Device-creation failures are recorded and skip operations, allowing the probe to answer which exact tiers a GPU exposes. The aggregate checks successful operations on at least one eligible hardware tier and detects live-adapter or exact-level mismatches. The all-seven claim comes from independent row verification on both logs. Final removal reasons and other diagnostic HRESULTs also need separate review.
- Request FL11_1 explicitly. On an older runtime a list containing `11_1` can immediately return `E_INVALIDARG`, including when lower levels appear later in the list. On a singleton FL11_1 `E_INVALIDARG`, the probe logs a diagnostic singleton FL11_0 call with null output pointers. Preserve the original FL11_1 failure, the lower diagnostic's exact HRESULT and the separate full FL11_0 request. A lower success leaves native FL11_1 unverified and does not uniquely establish why its request failed. Correlate OS build, runtime/interface support and known-good invocation. Neither measured machine hit the control branch. [D3D11CreateDevice](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-d3d11createdevice) · [How to get the feature level](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-get) · [Downlevel adapter and MSAA exceptions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-exceptions)

## Reproduce the smoke runs

Build from `C:\dev\personal\wgpu-dx11-roadmap-Nov21` on Ben-Desktop with [probe/build.ps1](../deliverables/dx11-levels/probe/build.ps1). The script uses MSVC 14.44.35207 and Windows SDK 10.0.19041 FXC and headers. ARM32 links against Windows SDK 10.0.15063 libraries and sets minimum OS 15063. ARM64 sets minimum OS 16299. Pass the approved development signer's existing local certificate thumbprint for both packages and the project assets directory. The script signs and verifies each APPX and returns its SHA-256. Use fresh paths under the system temp directory.

```powershell
$probe = '.\deliverables\dx11-levels\probe'
$buildRoot = Join-Path $env:TEMP ('dx11-tier-probe-' + [guid]::NewGuid().ToString('N'))
$desktop = Join-Path $buildRoot 'x64'
$surface = Join-Path $buildRoot 'arm64'
$lumia = Join-Path $buildRoot 'arm'
$assets = 'C:\dev\personal\MCHorizonUWP-nightly-2026-11-20\uwp\host\Assets'

& "$probe\build.ps1" -Architecture x64 -OutputDirectory $desktop
& "$probe\build.ps1" -Architecture arm64 -OutputDirectory $surface -LogoDirectory $assets -SigningThumbprint $projectDevelopmentSignerThumbprint
& "$probe\build.ps1" -Architecture arm -OutputDirectory $lumia -LogoDirectory $assets -SigningThumbprint $projectDevelopmentSignerThumbprint

& (Join-Path $desktop 'Dx11TierProbe.exe') |
    Set-Content -LiteralPath (Join-Path $desktop 'desktop-raw.txt') -Encoding Ascii
if ($LASTEXITCODE -ne 0) { throw "Native desktop probe failed: $LASTEXITCODE" }
```

The builds for the recorded runs completed in `%TEMP%\dx11-roadmap-nov21-x64`, `%TEMP%\dx11-roadmap-nov21-uwp-arm64` and `%TEMP%\dx11-roadmap-nov21-uwp-arm`. The first produced the x64 executable and raw smoke log. The ARM64 build produced the signed APPX and the transferred package was verified by SHA-256, installed and activated. The ARM32 build produced the signed APPX and PE import/header report.

- **Surface Pro X**. Acquire the Surface lock. Transfer the isolated signed ARM64 APPX and [run-surface.ps1](../deliverables/dx11-levels/probe/run-surface.ps1) to the tablet with its verified hash. Run the script under the tablet's interactive account using the local paths. It checks the transfer, adds the APPX, validates the installed architecture and status, activates `!App`, and waits for a fresh `LocalState\dx11-tier-probe.txt` from that run and process completion. Preserve `surface-deployment.txt` and `surface-raw.txt` with OS/driver capture.
- **Lumia 950 XL**. Acquire the Lumia lock. Wake and unlock the phone, connect it to the home LAN, and ensure the paired Device Portal responds. In Windows PowerShell 5.1, dot-source `$env:USERPROFILE\.codex\lumia-device-portal\portal.ps1` and use `Invoke-LumiaPortal -Path '/api/os/devicefamily'` for the authenticated preflight. It timed out during this work. Once reachable, verify the local ARM APPX hash above, use Device Portal's app package upload, poll `/api/app/packagemanager/state`, confirm installed package identity and activate that specific app. Download the fresh `LocalState\dx11-tier-probe.txt` from that isolated package's storage. Keep registration, activation and raw per-tier rows together. [Windows Device Portal app and files APIs](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/device-portal-api-core)

Record exact package and source identities, OS build, D3D runtime/interface probes, driver and WDDM where the diagnostic is available, returned adapter identifiers and operational rows. Maintain distinct evidence for native enumeration and smoke, full native operation coverage, HAL/public execution, and signed physical presentation and lifecycle. Missing hardware, absent mandatory runtime APIs and unverified paths remain open certification blockers under the [roadmap](wgpu-dx11-feature-level-roadmap.md).

## Baseline public build

Ben-Desktop built the existing pinned Rust public test target from `C:\dev\personal\wgpu-dx11-roadmap-Nov21` with the x64 MSVC 14.44.35207 and SDK 10.0.19041.0 linker paths and `CARGO_TARGET_DIR=%TEMP%\dx11-roadmap-nov21-cargo`. `cargo test --locked --offline -p wgpu --no-default-features --features dx11,wgsl --test dx11_public --no-run` completed. The shared desktop GPU was reserved for other active lanes during the verification windows. The physical strict `dx11_public` suite remains unrun in this lane. This build covers compilation of the existing public tests.
