# Experimental D3D11 FL11_0

**Certification status is INCOMPLETE.** Develop on `codex/dx11-fl11_0-nov21`, based on `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`. The November 20 HorizonUWP product pin is `5c26a8060c1cf2dce20ee5707e84699ed0879e5c` and its upstream reference is `0a875da91dbb0fdcb7ba8c42c11341aa229af51a`. Predecessor certification runs from FL9_1 through FL10_1. Record the reviewed cumulative predecessor SHA and rerun its suites before requesting promotion.

## Selecting an exact hardware level

Enable the existing experimental DX11 backend explicitly and set `WGPU_DX11_EXACT_FEATURE_LEVEL=11_0` before enumerating adapters. The selector accepts `9_1`, `9_2`, `9_3`, `10_0`, `10_1`, `11_0` and `11_1`. Enumeration uses an explicit physical DXGI adapter and a singleton level list and verifies the returned level against `GetFeatureLevel`. Invalid or conflicting selectors produce no DX11 adapters. The legacy `WGPU_DX11_FORCE_FL9_1=1` selector continues to select FL9_1. Automatic enumeration uses its existing highest-first list and retains the retry for runtimes that reject the leading FL11_1 request. Exact selection includes the requested and active level and the adapter LUID in the report. Strict CTS selection verifies the live HAL adapter. [D3D11CreateDevice](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-d3d11createdevice)

The ordinary wgpu contract is FL9_1-class by default. An experimental compute projection requires a separate runtime opt-in. The native feature-level result is independent of the public capabilities.

## Portable compute opt-in

Enable the `dx11-fl11_0` Cargo feature, select `WGPU_DX11_EXACT_FEATURE_LEVEL=11_0`, and set `WGPU_DX11_EXPERIMENTAL_FL11_0=1` before enumerating adapters. The opt-in exposes SM5 WGSL compute, read-only and writable storage buffers, dynamic storage ranges, workgroup memory, and direct and indirect compute dispatch. A GPU sanitizer checks the three indirect counts against the logical device limit and supplies `num_workgroups` to the shader. The portable indirect-draw flag stays disabled.

D3D11 binds raw storage ranges through SRV and UAV views. Dispatch replay clears conflicting bindings. Multiple ranges of one buffer use temporary GPU copies when any of those ranges is writable. Compute writes invalidate the CPU shadow used by later copies and mappings. [D3D11 raw buffer views](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_bufferex_srv) · [D3D11 functional specification](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)

The same opt-in adds typed `Rgba8Uint` 2D copies in both directions, including texture-to-texture copies. The adapter reports copy support after checking the driver's texture, mip and CPU-lockable format flags. Storage textures and indirect rendering remain separate work.

Use `run-desktop.ps1` to exercise both the opt-in and default projections. Rebuild the CTS runner with `--features deno_webgpu/dx11-fl11_0` and pass `-Portable` to `run-cts.ps1` for the compute projection.

## Native commands

Build wgpu or wgpu-hal with the experimental `dx11-fl11_0` Cargo feature. Acquire the D3D11 HAL device through `wgpu::Device::as_hal::<wgpu::hal::api::Dx11>()`. `native_fl11_0()` is available on an exact FL11_0 device. `Sm5Stage::compile` takes HLSL and an entry point and selects one of the six FXC profiles. Portable WGSL shaders continue through the existing translation and validation path. [FXC compiler targets](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets)

Create D3D11 resources on the supplied native device and record operations in the deferred-context callback. Resource initialization, hazard ordering, query scopes and capability checks are the native caller's responsibility. Submit the owned list through the originating HAL queue. The queue serializes execution against HAL commands, restores the immediate context state and checks device removal. `readback` locks the immediate context for native staging maps and query reads. Keep these resources independent from wgpu-tracked allocations, including swapchain images. The extension does not expose imported resource sharing. [D3D11 deferred rendering](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-render)

## Desktop verification

Run from this worktree on Ben-Desktop. Both desktop architectures use the same source, require a physical exact-level adapter, and compare copied data from GPU-native resources. The runner builds before acquiring the desktop GPU lock. Use a fresh evidence directory for each run. Add `-IncludeFl91Public` for the pinned public regression, `-IncludeBenchmarks` for the matched scene, and `-BuildOnly` to compile while the GPU is reserved.

```powershell
$scratch = 'E:/CodexScratch/wgpu-fl11_0-Nov21'
pwsh -NoProfile -File deliverables/dx11-levels/FL11_0/run-desktop.ps1 -Architecture x64 -ScratchDirectory $scratch -OutputDirectory "$scratch/evidence-x64" -IncludeFl91Public -IncludeBenchmarks
pwsh -NoProfile -File deliverables/dx11-levels/FL11_0/run-desktop.ps1 -Architecture x86 -ScratchDirectory $scratch -OutputDirectory "$scratch/evidence-x86"
```

The targeted fixtures cover SM5 profile disassembly, triangle tessellation and geometry with pixel and pipeline-statistics readback, raw and structured UAVs, atomics, the 1,024-thread and 32 KiB group-shared workload, GPU-written draw and dispatch arguments, R32_UINT typed loads and stores, BC6H/BC7 typeless views and compressed 2D mip copies. The prior-level control rejects SM5 compute creation at exact FL10_1. Native ownership tests cover foreign lists, discarded recordings and injected device-loss reporting. Actual physical device removal requires a separate run.

The desktop benchmark uses the same 32×32 textured cube, RGBA8 color target and D16 depth target at exact FL9_1, FL10_1 and FL11_0. Separate processes measure command recording through completion wait at one and 128 draws per submission, then report peak working set and peak committed bytes. These runs compare the current portable path. GPU-only timestamps and a signed HorizonUWP comparison need separate evidence.

## Candidate results

Source commit `ad37b24ca1fd9152629d3b17f7dbb52e41e5a83f` passed all five targeted native fixtures in both x64 and x86 processes on the RTX 3090 Ti. Both runs verified the public adapter's exact FL11_0, FL10_1 and FL9_1 selections and rejected the invalid FL12_0 request. Indirect tessellated draw readback matched 64 pixels with two input patches. Compute readback returned 4,218,889, and BC7 sRGB readback matched the expected linear conversion.

The x64 forced FL9_1 public run reported 176 passing test functions, including four conditional skip messages, and one failing test. `public_dx11_inter_stage_varying_limit_at_fl91` reports seven variables and expects eight. Strict CTS verified live physical FL11_0 and passed five of seven selected cases. Two command-buffer copy cases failed because the conservative projection rejects `COPY_SRC | COPY_DST` on `Rgba8Uint`. The limits cases validated the reported projection.

| Requested and active level | Median, 1 draw | Median, 128 draws | Peak working set | Peak committed bytes |
| --- | ---: | ---: | ---: | ---: |
| FL9_1 | 0.2575 ms | 0.8516 ms | 59,846,656 B | 64,696,320 B |
| FL10_1 | 0.2567 ms | 0.8697 ms | 59,682,816 B | 64,573,440 B |
| FL11_0 | 0.2629 ms | 0.9065 ms | 59,854,848 B | 65,720,320 B |

Each level produced the same readback histogram. The raw benchmark captures 25 samples after five warmups for each draw count. [Benchmark manifest](../deliverables/dx11-levels/FL11_0/benchmarks.json)

Build the strict CTS runner with the pinned toolchain and a configured MSVC environment. The x64 build used the cached Rusty V8 137.3.0 release library from the identical lockfile in the FL9_2 lane. The linker option disables PDB generation for this disposable runner.

```powershell
cargo rustc -p cts_runner --bin cts_runner --features deno_webgpu/dx11 --locked --offline -- -C link-arg=/DEBUG:NONE
pwsh -NoProfile -File deliverables/dx11-levels/FL11_0/run-cts.ps1 -Runner "$env:CARGO_TARGET_DIR/debug/cts_runner.exe" -CtsCheckout $pinnedCtsCheckout -OutputDirectory "$scratch/cts-evidence"
```

The pinned CTS checkout is `3fdd32363410f2bd4e3e2f046980967e6abc27e6`. The archived runner receipt counts repeated failure-log headers. The test manifest uses the authoritative CTS summaries and reports two failures.

Scoped rustfmt and HAL Clippy completed. The whole-workspace rustfmt check still finds formatting differences in the pinned baseline files. Clippy reported 30 warnings in existing code.

## Portable candidate results

Source commit `8a58a3a57320d6278bdd97485211809a03467629` passed three portable fixtures, five native fixtures and all exact-selection controls in both x64 and x86 processes on the physical RTX 3090 Ti. Portable readback verified `num_workgroups`, dynamic storage ranges, sampled data and rejection of an over-limit GPU-written dispatch. The 1,024-thread, 32 KiB workgroup produced atomic results of 7 and 13 in two ranges of one buffer. The typed `Rgba8Uint` copy chain reproduced both pitched rows byte for byte.

The pinned x64 CTS selection passed all seven cases with zero warnings or skips. Both `b2t2b` and `b2t2t2b` passed after enabling the native integer format in both transfer allowlists. CTS also verified the storage binding size alignment and compute workgroup limits.

The forced FL9_1 public suite returned 176 passing functions and one failure on x64. The single-process x86 run returned 127 passing functions and 50 failures after adapter selection started failing partway through. The same adapter-selection failure occurred on the pre-change revision, which returned 123 passing functions and 54 failures. Splitting the exact candidate into four fresh x86 processes returned 176 passing functions and one failure. Both the x64 and split x86 runs include four conditional skip messages and retain the existing varying-limit failure at seven versus eight. [Portable test manifest](../deliverables/dx11-levels/FL11_0/portable-tests.json)

```powershell
$scratch = Join-Path $env:TEMP 'wgpu-fl11-portable'
pwsh -NoProfile -File deliverables/dx11-levels/FL11_0/run-desktop.ps1 -Architecture x64 -ScratchDirectory $scratch -OutputDirectory "$scratch/desktop-x64" -IncludeFl91Public
pwsh -NoProfile -File deliverables/dx11-levels/FL11_0/run-desktop.ps1 -Architecture x86 -ScratchDirectory $scratch -OutputDirectory "$scratch/desktop-x86" -IncludeFl91Public
$env:CARGO_TARGET_DIR = "$scratch/cts-target"
cargo rustc -p cts_runner --bin cts_runner --features deno_webgpu/dx11-fl11_0 --locked --offline -- -C link-arg=/DEBUG:NONE
pwsh -NoProfile -File deliverables/dx11-levels/FL11_0/run-cts.ps1 -Portable -Runner "$env:CARGO_TARGET_DIR/debug/cts_runner.exe" -CtsCheckout $pinnedCtsCheckout -OutputDirectory "$scratch/cts"
```

## Remaining qualification

- The FL9_1 public regression and certification status are separate from the higher-tier native fixtures. See the exact-candidate results and inherited failures in [the test manifest](../deliverables/dx11-levels/FL11_0/tests.json).
- Portable storage textures, indirect draws, public query semantics, expanded dimensions and formats, shared RTV/UAV allocation and all inherited operations need implementation or qualification. Native-only tests still need stream output, remaining tessellation domains and boundaries, indirect argument rejection cases, sample positions, complete query and format-use matrices, and swapchain lifetime and resize.
- Signed exact-level Surface ARM64, Lumia ARM32 capability and deployment, and independent native-ceiling hardware results remain open. The roadmap's older results provide device controls.
- HorizonUWP shader packs, DRS, HUD, mod markers, presentation and matched performance need a signed build pinned to the qualifying fork SHA. The [benchmark manifest](../deliverables/dx11-levels/FL11_0/benchmarks.json) records the outstanding comparisons.

The [requirement ledger](../deliverables/dx11-levels/FL11_0/requirements.csv) imports the pinned roadmap's operations and DXGI/DDI format evidence and adds API methods from the SDK, HAL methods and public limits and features from the fork. `partial` identifies a recorded passing case whose coverage remains narrower than full qualification. `unknown` identifies rows without matching passing candidate evidence. The [hardware manifest](../deliverables/dx11-levels/FL11_0/hardware.json) identifies the physical target for each run. [D3D11 feature levels](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro) · [FL11_0 format support](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/format-support-for-direct3d-11-0-feature-level-hardware)
