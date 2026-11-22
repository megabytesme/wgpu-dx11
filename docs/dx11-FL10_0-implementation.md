# Direct3D 11 FL10_0 implementation

**FL10_0 is incomplete.** The experimental native implementation is based on `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`. The FL9_1, FL9_2 and FL9_3 certification records are missing. Integrate on the exact cumulative candidate after those reviews. [Source and replayable patch](../deliverables/dx11-levels/FL10_0/source.json)

## Developer entry points

Set `WGPU_DX11_EXPERIMENTAL_FEATURE_LEVEL` to `10_0` to require that exact level on a hardware DXGI adapter. The selector also accepts `9_1` as the control. The backend validates the requested level against both D3D11 creation results. Unsupported values and conflicting legacy overrides fail closed. The selected FL10_0 portable path enables eight color attachments, separate alpha blending, independent write masks with a shared blend equation, multisample alpha-to-coverage, and occlusion queries. MRT fragment shaders use `ps_4_0`. An unset selector keeps automatic native negotiation, and `WGPU_DX11_FORCE_FL9_1=1` still requests the singleton FL9_1 level.

The render encoder retains color slots through sparse attachments, sampled-texture refresh, and per-attachment resolve. Public occlusion uses the existing ordered query resolve path, and Core clears unwritten slots. The public fixture checks all 128 color pixels and resolves one written and one unwritten query. Finite sampler maxima remain visible to the FL9 exact-mip lowering, which rejects substitutions outside the sampler's LOD range. The FL9 varying fix retries the level-9 compile after reflection proves FXC can remove an unread fragment position input.

The attachment recording and replay changes use selected hunks from FL9_3 commit `bd6078df6f77ec9a687edadaae9479e950e3bab5`, adapted for eight slots. The varying work uses the reflection and shader-writing portions of FL9_1 commit `dbae35d184974c22b08da2426804facc5550acec`. The alpha-to-coverage change adapts native blend-state selection from FL10_1 commit `8b8f5281acbcb0d755b1599f4a8cad4ed6c134c2` to FL10_0 and the eight-slot path. The candidate remains based on the pinned fork.

Use `wgpu_hal::dx11::fl10::Device::open` with a hardware adapter for native feature work. This owns a separate D3D11 device. Its context closure serializes immediate-context access, clears bindings when the callback returns and preserves command order across batches. Callers own synchronization to GPU completion and manage their own native resources. The interface provides explicitly targeted FXC compilation, validated Naga vertex and fragment translation, geometry shader creation, single-stream shader output declarations and the optional CS4 device query. [Microsoft shader profiles](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets) · [Native stream-output creation](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11device-creategeometryshaderwithstreamoutput)

Native FL10_0 guarantees geometry and stream output. CS4 requires the hardware option and has a restricted compute contract. Portable compute and indirect arguments remain disabled in this tranche. [Direct3D feature-level matrix](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro) · [Optional compute query](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_feature_data_d3d10_x_hardware_options)

## Reproduce desktop checks

Run from the isolated worktree on Ben-Desktop in developer PowerShell with the x64 MSVC toolchain initialized. Set `CARGO_TARGET_DIR` to an isolated scratch directory with sufficient space and use `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_INCREMENTAL=0` to reproduce the build configuration.

```powershell
cargo +stable test --offline --locked -p wgpu-hal --no-default-features --features dx11 --test dx11_fl10 --no-run
cargo +stable test --offline --locked --target i686-pc-windows-msvc -p wgpu-hal --no-default-features --features dx11 --test dx11_fl10 --no-run
cargo +stable test --offline --locked -p wgpu --no-default-features --features std,dx11,wgsl --test dx11_fl91_scene --no-run
```

Pass the Cargo-reported executables to `run-desktop.ps1`. Use the scene executable on x64 for the comparable native-level runs. The script takes the coordinator's desktop GPU lock and records the output, executable hash and observed peak process working set. It exercises two repetitions of a 32×32 textured D16 cube at forced FL9_1 and FL10_0 using the same portable rendering path. It includes CPU command recording and GPU completion wait. The timing and process memory measure this fixture. Native FL10_0 shader performance, VRAM and HorizonUWP gameplay require separate measurements. [Desktop runner](../deliverables/dx11-levels/FL10_0/run-desktop.ps1)

During development an exact native FL9_3 portable scene repeatedly failed while mapping the scene uniform buffer. The public test selector now rejects FL9_3 and the comparison stays open until a working FL9_3 candidate exists. [Diagnostic log](../deliverables/dx11-levels/FL10_0/logs/diagnostic-9_3-x64.log)

The initial tree `9e294f8de36a4557bf3f113f3c598efa0903b640` passed the exact FL10_0 native suite on the RTX 3090 Ti in x64 and x86 processes. Both runs checked the four FXC profiles, 128 MRT pixels, 16 texture pixels, three captured vertices, occlusion counts of 16 and 0, and 16 words from optional CS4. The x64 scene matched all 1,024 depth-tested pixels at FL9_1 and FL10_0. At 128 draws, the two process medians were 0.780–0.782 ms at FL9_1 and 0.772–0.802 ms at FL10_0, with 57.3–57.9 MiB peak process working set across the runs. [Baseline x64](../deliverables/dx11-levels/FL10_0/logs/baseline-9e294f8-native-x64.log) · [Baseline x86](../deliverables/dx11-levels/FL10_0/logs/baseline-9e294f8-native-x86.log)

The initial public fixture recorded one failure at exact FL9_1 and nine at exact FL10_0. The interim tree `2f9f98a9ee4b5e3cc39cb75f3895e4a026be9fbe` passed 177 of 177 at FL9_1 and 174 of 177 at FL10_0. The subsequent changes corrected the error expected for a finite sampler LOD maximum, set the oversized cube dimensions from the selected tier, and preserved rejection for repeat samplers under a finite precision limit. The revised fixture also reads back the result of sRGB blending on supported hardware and uses an exactly representable value in the byte-aligned vertex test. [Interim FL9_1](../deliverables/dx11-levels/FL10_0/logs/interim-2f9f98a-regression-9_1-x64.log) · [Interim FL10_0](../deliverables/dx11-levels/FL10_0/logs/interim-2f9f98a-regression-10_0-x64.log)

The pre-coverage tree `97e9a0a54d8a3c7ab98831f6d631bd9980c685a4` built on Ben-Desktop in `E:/temp/wgpu-dx11-fl10_0-nov21-target`. Its x64 and x86 CPU FXC checks and ARM64 and ARM32 UWP HAL type-checks passed. The x64 public regression required a test-crate recursion limit of 256 with the existing large fixture. The hardware rerun and CTS for the alpha-to-coverage candidate use the same build directory. The test manifest records each result against the exact source tree. [Prior source](../deliverables/dx11-levels/FL10_0/prior-97e9a0a-source.json) · [Test manifest](../deliverables/dx11-levels/FL10_0/tests.json)

The exact candidate `7475312c57126aaf5715c0f492c683f10e1b4cfb` passed all 177 public regression tests at each selected level, FL9_1 and FL10_0. Both native architectures passed on the RTX 3090 Ti. The portable eight-target test matched 128 pixels and resolved the query buffer to `[16, 0]`. The default selector and the two unsupported-level controls passed. ARM64 and ARM32 UWP HAL type-checks passed, and Clippy completed with warnings. [Exact FL9_1 regression](../deliverables/dx11-levels/FL10_0/logs/regression-9_1-x64.log) · [Exact FL10_0 regression](../deliverables/dx11-levels/FL10_0/logs/regression-10_0-x64.log) · [Portable readback](../deliverables/dx11-levels/FL10_0/logs/public-fl10-mrt-x64.log) · [Native and cross-build receipts](../deliverables/dx11-levels/FL10_0/tests.json)

The exact candidate's matched scene compared all 1,024 depth-tested pixels. At 128 draws, the process medians were 0.981–1.137 ms for FL9_1 and 1.072–1.229 ms for FL10_0. Observed peak process working set was 57.8–58.4 MiB across both levels. [Exact benchmark data](../deliverables/dx11-levels/FL10_0/performance.csv)

## Reproduce CTS runs

Build `cts_runner` from the candidate and provide a clean checkout of the CTS revision in `cts_runner/revision.txt` with its generated output. The strict runner verifies both the requested and actual native levels before giving the test a device. The script takes the desktop GPU lock in batches and records the runner digest, per-selector status and runtime case counts.

```powershell
cargo +stable build --offline --locked -p cts_runner --features deno_webgpu/dx11
python deliverables/dx11-levels/FL10_0/run-cts.py <cts_runner.exe> <cts_checkout>
python deliverables/dx11-levels/FL10_0/run-cts.py <cts_runner.exe> <cts_checkout> --curated --start 0 --count 96
```

The development build reused the local x64 V8 137.3.0 library from the FL9_1 lane through `RUSTY_V8_ARCHIVE`. The CTS executable is built from the corresponding source tree. All 11 Deno WebGPU unit tests passed on the exact current candidate. [CTS runner script](../deliverables/dx11-levels/FL10_0/run-cts.py) · [Deno unit tests](../deliverables/dx11-levels/FL10_0/logs/cts-strict-unit.log)

The first focused run used revision `3fdd32363410f2bd4e3e2f046980967e6abc27e6` and verified physical FL10_0 in all six selectors on the pre-coverage tree. Clear, fullscreen quad, RGBA8 sRGB blending and occlusion begin/end validation completed successfully. The component-count blend selector failed the R8 and RG8 cases and passed its two RGBA cases. The broader occlusion selector passed basic counts, query reuse, scissor and depth cases. It reported unsupported Stencil8 attachment and alpha-to-coverage configurations before the Deno runner aborted on a reentrant `createBuffer` invocation. Its 147 passes and 96 failures are partial case reports. [Prior focused CTS receipt](../deliverables/dx11-levels/FL10_0/logs/prior-97e9a0a-cts-focused.jsonl) · [Prior focused CTS log](../deliverables/dx11-levels/FL10_0/logs/prior-97e9a0a-cts-focused.log)

The current candidate verified physical FL10_0 in all seven focused selectors. Five passed, including all 80 alpha-to-coverage cases. The R8 and RG8 blend mismatches persisted. The broad occlusion selector recorded 227 passes and 16 Stencil8 failures before the same Deno reentrancy abort. The other six selectors reported 88 passes and two failures. [Exact focused CTS receipt](../deliverables/dx11-levels/FL10_0/logs/cts-focused.jsonl) · [Exact focused CTS log](../deliverables/dx11-levels/FL10_0/logs/cts-focused.log)

The first 96 curated selectors recorded 51 executions and 45 entries excluded by the checked-in DX11 conditions. Of the executed selectors, 50 exited successfully and one failed. The runner reported 1,176 cases passed, 368 skipped, and two failed. Both failures were D3D11 out-of-memory results when creating a buffer at or one byte below the advertised limit. Forty-five selectors emitted strict physical FL10_0 adapter receipts. [Exact curated CTS receipt](../deliverables/dx11-levels/FL10_0/logs/cts-curated-0.jsonl) · [Exact curated CTS log](../deliverables/dx11-levels/FL10_0/logs/cts-curated-0.log)

## Promotion blockers

- Implement and validate the remaining public HAL and wgpu operations, including 1D and array texture views, Stencil8 attachments, SM4 vertex and general single-target shader lowering, and expanded binding and format support. Diagnose the R8 and RG8 blend mismatches with an exact SM4 control, and establish an allocatable buffer boundary. Track each Microsoft format/use requirement and its negative boundary. [FL10_0 format table](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/format-support-for-direct3d-feature-level-10-0-hardware) · [Requirement ledger](../deliverables/dx11-levels/FL10_0/requirements.csv)
- Cover native stream-output slots and overflow, vertex and sampling limits, depth/stencil and multisample behavior, and optional CS4 boundaries. The single-device tests cover concrete executions and leave the remaining rows unknown.
- Test signed AppContainer binaries on the Surface Pro X and, if supported, the Lumia 950 XL. The Surface SSH port did not respond from Ben-Desktop. Lumia TCP 443 later responded, and the paired Device Portal read returned a WebException. Signed presentation, resize, suspend/resume and physical device removal remain pending.
- Rerun the inherited tiers and CTS on a reviewed integration candidate. Then verify pinned HorizonUWP shaders, DRS, HUD and mod markers and record matched gameplay performance and memory. Independent QA review and the FL10_0 gate request follow this evidence.

[Test manifest](../deliverables/dx11-levels/FL10_0/tests.json) · [Hardware receipts](../deliverables/dx11-levels/FL10_0/hardware.json) · [Benchmark results](../deliverables/dx11-levels/FL10_0/performance.csv)
