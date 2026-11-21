# Direct3D 11 FL9_1 certification

**INCOMPLETE**

## Follow-up candidate

Candidate `dbae35d184974c22b08da2426804facc5550acec` adds guarded native BGRX8 textures and native mip generation. A public device reaches these operations through `Device::as_hal::<wgpu::hal::api::Dx11>()`. The extension validates device ownership and uses the immediate-context lock shared with queue submission. Keep native textures within the extension and initialize their subresources by upload or clear before use.

| Area | Follow-up result on physical RTX 3090 Ti at requested and returned FL9_1 |
| --- | --- |
| BGRX8 | The UNORM and sRGB textures passed the typed 2D, cube and volume sampling checks, including both depth slices, all six cube faces and opaque sampled alpha with X set to zero. The 2D and cube paths passed cross-encoding bit copies, clears, staged reads and mip generation. Volume rendering and direct volume copies observe the 10Level9 restrictions. The driver reported `SHADER_LOAD` for both typed formats. The level-9 shader fixture exercises sampling. |
| Native mip generation | Native `GenerateMips` populated three lower levels for each of the seven documented formats in both 2D and cube textures. The test checks each cube face, legal suffix views, initialized source mips and cross-device rejection. A public-device guard also reached BGRX mip generation and read back `[9, 17, 35]`. |
| Varyings | Eight distinct weighted varyings rendered `[102, 60, 18, 143]` in 64 pixels. Core rejected location 8. After an FXC input-limit diagnostic, the backend uses reflection to prove that a declared fragment position is unused before regenerating just the level-9 signature. The resulting shader still uses the FL9_1 FXC profile. CPU checks cover direct, conditional, aliased and derivative position reads. |
| Clipping and repeats | The six-plane native test matched all 256 pixels with clipping disabled, in both sign directions for each slot, and with all six active. Native address-mode controls matched 49,152 pixels across 24 cases, including positive and negative 128-period spans. Desktop reported unrestricted NPOT support, so the conditional NPOT allocation rejection remains unexercised there. Core still rejects portable Repeat and MirrorRepeat. |
| sRGB blending | The default `HARDWARE` and explicit-adapter `UNKNOWN` controls returned the same adapter LUID at FL9_1. Both produced BGR `[137, 188, 187]`, matching the linear-space model and the same-adapter FL10_0 control. The documented encoded-space model predicts `[94, 128, 128]`. This discrepancy remains open. [D3D11 blend rules](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_blend_desc) |
| Event queries | The native fence followed a real GPU copy and the staged bytes matched. The bounded-wait fixture used an injected timeout and then waited successfully on the same hardware event. |

The x64 and x86 public suites each passed 178 test functions, with four optional subcases skipped on each architecture. All seven new native test functions and both focused query tests passed on each architecture. x64 Core passed 156 and wgpu-types passed 120. The x64 HAL suite passed 562 and failed both hidden-HWND queued-Present checks. The same checks failed in a pre-varying build under the same disconnected Windows session. Interactive presentation still needs verification.

The ARM64 public suite cross-built. The ARM32 offscreen consumer linked as an ARM AppContainer against the Windows 10 SDK after recovery from a disk-space failure. Its imports include `D3D11CreateDevice` and undecorated `LoadLibraryExW`. The build uses inert panel-host stubs. Signed runtime testing remains pending. [Test receipts](../deliverables/dx11-levels/FL9_1/tests.json)

The 364-row matrix and CTS audit retain the original run under its earlier remediation SHA. The audit enumerates all 4,720 case skips with reasons and 1,197 list exclusions. Three unmodified copy selectors failed on the follow-up candidate. Each combines four eligible single-layer cases with three cases requiring a non-cube 2D array. A temporary copy of the CTS test changed only that case selection. All 12 eligible pixel assertions then passed. The complete unmodified failures and exact selection delta are recorded. The audit still has 1,130 unresolved exclusions. [Contract matrix](../deliverables/dx11-levels/FL9_1/contract.csv) · [Runtime skips](../deliverables/dx11-levels/FL9_1/cts-runtime-skips.csv) · [Excluded selectors](../deliverables/dx11-levels/FL9_1/cts-excluded-selectors.csv) · [Audit summary](../deliverables/dx11-levels/FL9_1/cts-exclusion-audit.json)

Surface SSH timed out before the device rerun was deferred. The Lumia was unavailable. Signed presentation, physical removal, low-memory and exact-candidate product checks remain open. [Blocker register](../deliverables/dx11-levels/FL9_1/blockers.md)

## Previous candidate and baseline evidence

This pass started from wgpu `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157` and produced remediation commit `dbdc67913fcc858924ca553f68764fd8b432bd70` and a test-only refinement in `9bf6b497bef0c03dcfdc71e5e2bf942752714f34`. The November 20 HorizonUWP checkout was `5c26a8060c1cf2dce20ee5707e84699ed0879e5c` with Horizon `0a875da91dbb0fdcb7ba8c42c11341aa229af51a`. See the [test receipts](../deliverables/dx11-levels/FL9_1/tests.json).

- The 360-row matrix records native driver requirements, 10Level9 API restrictions, SDK limits, shader profiles and projected wgpu operations. It has separate evidence fields for driver queries, native GPU results and wgpu results. Each DXGI format row refers to one format-support bit. [Contract matrix](../deliverables/dx11-levels/FL9_1/contract.csv)
- The hardware records identify each forced-level execution, desktop presentation, ARM64 offscreen execution and historical product execution. The format query comparison resolves numeric feature masks against the SDK header. [Hardware records](../deliverables/dx11-levels/FL9_1/hardware.json) · [Format queries](../deliverables/dx11-levels/FL9_1/format-queries.csv)
- Mission Control's gate remains pending. [Open requirements](../deliverables/dx11-levels/FL9_1/blockers.md)

## Observed results

| Scope | Observation |
| --- | --- |
| Desktop x64 public wgpu | All 178 functions in `dx11_public` returned success. Four logged optional subcases were skipped. The corrected varying test rendered all 64 pixels with seven user variables and Core rejected the eighth. For RGBA8 and BGRA8, same-format and cross-encoding volume copies preserved eight voxels per copy. Each cross-encoding destination matched all 64 sampled pixels. |
| Desktop x64 resource limits | A 512-square cube sampled the corner on all six faces. Separate volumes sampled the edge at 256 texels on each axis. Core rejected the selected above-limit descriptors. |
| Desktop x64 HAL and Core | DX11 HAL ran 556 tests with seven logged subcase skips. The Core suite passed 156 tests after the fixture was aligned to the existing 128 MiB minus 4 byte buffer ceiling. All 120 `wgpu-types` tests passed. Test function totals include CPU and validation-only tests. |
| Desktop x64 presentation | The safe winit surface reconfigured from 400 × 300 to 640 × 480 physical pixels and presented 90 indexed, textured depth-tested frames. Surface-injection and copy tests also ran. |
| Desktop x86 | Physical native device creation, native mip readback and two public texture-limit tests passed. The first single-process public invocation stalled after logging the RGB565 copy/readback case and was stopped. Four isolated retries passed. The final public suite passed all 178 functions in eight fresh processes, with the same four optional subcase skips as x64. |
| Surface Pro X ARM64 | The native runtime returned `0x9100` on Adreno 680. The offscreen example matched 1,024 pixels. The public suite reported 177 passes, four optional subcase skips and one HWND surface configuration failure in the locked SSH session. Both volume-limit cases and both linear/sRGB volume-copy cases passed. Signed interactive presentation remains open. |
| Native formats | In each of the x64, x86 and ARM64 runs, all 139 format-bit rows classified as mandatory in the current driver table reported the required bit. All seven tested auto-mip formats populated both lower levels. The cases distinguish two subregions at 2 × 2 and verify a mixed value at 1 × 1. The format probe also logged 4× MSAA quality and confirmed that the event query eventually returned `TRUE`. |
| D3D9 legacy caps | The native D3D9Ex probe queried `GetDeviceCaps(D3DDEVTYPE_HAL)` on the RTX 3090 Ti. It recorded the modern driver's legacy caps, including pixel shader model 3, 16× anisotropy and a 16,384 texture extent. The separate D3D11 device reported exact FL9_1 while running the native and public readbacks. |
| FXC | SDK `10.0.26100.0` passed 16 compiler checks per x64 and x86 compiler. The cases cover instruction limits, dependent fetches, static loop depth, clip-plane count and pixel input signatures. FXC counted a declared unused `SV_Position` against the eight-input budget and rejected an active pixel position input. |
| CTS | All 1,686 runnable curated selectors completed on the rebuilt physical x64 FL9_1 candidate. The run logged 17,962 passing case occurrences, 4,720 runtime skips and zero failures. The list excludes 1,197 selectors for DX11. The initial run stopped before texture creation on an adapter `NotFound` result. The exact retry and the rebuilt prefix passed. |
| ARM32 UWP | The full offscreen consumer initially linked against five missing Horizon host entry points. With inert offscreen-only host stubs it linked as ARMNT AppContainer. The import table recorded `D3D11CreateDevice` and undecorated `LoadLibraryExW`. DXGI and FXC remained dynamically loaded. CoreWindow activation and mobile runtime import compatibility still need an on-device run. |

The latest pinned product reports cover compatibility-mode ARM gameplay, HUD/atlas, resolution scaling, screenshots and native/Wasm hooks at their reported levels. The historical forced-Lumia FL9_1 attempt produced audio without a game frame. This lane exercised the targeted upload-shadow and depthless pipeline regressions through the desktop public suite. Product, low-memory and lifecycle evidence remains tied to its original package, runtime and negotiated feature level. [Lumia diagnostic report](../../MCHorizonUWP-nightly-2026-11-20/docs/nightly-lumia-drs-2026-11-21.md) · [November 20 runtime follow-up](../../MCHorizonUWP-nightly-2026-11-20/docs/nightly-runtime-completion-2026-11-21.md)

The runtime returned `E_FAIL` for the unresolved `R32_UINT` format query, and the BC1 query lacked the legacy volume bit. These rows identify the source discrepancies individually. [Current driver format table](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats) · [10Level9 context behavior](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context)

The original ARM64 3D texture-load tint and R8 blend fixtures observed adjacent legal bytes at half-step UNORM conversion values. The refinement moves the shader inputs away from those half steps while keeping exact readbacks and sentinel checks. Both cases passed on x64, x86 and ARM64 after the refinement. The original failures and all reruns are in the test receipts. [Direct3D FLOAT to UNORM rules](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm#FLOATtoUNORM)

Clippy completed with repository warnings. The Rust formatter reported the same 21 existing hunks on the pinned baseline and the remediation commit. The documented Cargo commands use debug binaries.

## Reproduce from the candidate

Run from the lane worktree in the matching Visual Studio Developer PowerShell. Use a disposable Cargo target under the system temp directory. Set both strict flags and take the shared GPU lock before starting a hardware process.

```powershell
$env:WGPU_DX11_FORCE_FL9_1 = '1'
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 test --locked -p wgpu --features dx11-test-fixture --tests -- --test-threads=1 --nocapture
cargo +1.93.1 test --locked -p wgpu-hal -p wgpu-core -p wgpu-types --features wgpu-core/dx11-test-fixture,wgpu-hal/dx11-test-fixture --lib -- --test-threads=1 --nocapture
cargo +1.93.1 run --locked -p wgpu --features dx11 --example dx11_fl91_winit -- --resize-once --frames=90
```

For the public x86 binaries, use the x86 MSVC environment with `--target i686-pc-windows-msvc --no-default-features --features dx11,wgsl,std`. Select `dx11_public` and `dx11_fl91_texture_limits`. Run `dx11_fl91_offscreen` natively on Ben-Tablet with the same strict flags after building for `aarch64-pc-windows-msvc`.

From an architecture-matched Developer PowerShell and a system-temp working directory, build the native control with the source from the worktree.

```powershell
cl /nologo /W4 /WX /O2 /MT /std:c++17 /EHsc $probeSource /Fe:native-probe.exe /link d3d11.lib dxgi.lib
.\native-probe.exe
```

[The native format comparison script](../deliverables/dx11-levels/FL9_1/Compare-NativeFormatQueries.ps1) accepts the native output logs and the matching SDK's `d3d11.h`. [The FXC harness](../deliverables/dx11-levels/FL9_1/Test-FxcProfiles.ps1) accepts the full architecture-specific `fxc.exe` path. CTS uses upstream revision `3fdd32363410f2bd4e3e2f046980967e6abc27e6` and the fork's `cts_runner/test.lst`.

```powershell
cargo +1.93.1 xtask cts --skip-checkout --backend dx11 --require-dx11-hardware --print-output-when test-fails -f cts_runner/test.lst
```

The ARM32 compile and link evidence uses MSVC 14.44, SDK 10.0.22621.0, and Rust 1.93.1 with `RUSTC_BOOTSTRAP=1` and `-Z build-std=core,alloc,std,panic_abort`. The temporary consumer uses `dx11,wgsl,std`, compiles the checked-in offscreen example, and supplies five inert Horizon panel host symbols. Its `dumpbin /headers /imports` transcript is attached to `tests.json`. The configured product host must supply its real panel functions for presentation.
