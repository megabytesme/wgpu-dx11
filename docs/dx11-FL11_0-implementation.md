# Experimental D3D11 FL11_0

**Certification status is INCOMPLETE.** Develop on `codex/dx11-fl11_0-nov21`, based on `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`. The November 20 HorizonUWP product pin is `5c26a8060c1cf2dce20ee5707e84699ed0879e5c` and its upstream reference is `0a875da91dbb0fdcb7ba8c42c11341aa229af51a`. Predecessor certification runs from FL9_1 through FL10_1. Record the reviewed cumulative predecessor SHA and rerun its suites before requesting promotion.

## Selecting an exact hardware level

Enable the existing experimental DX11 backend explicitly and set `WGPU_DX11_EXACT_FEATURE_LEVEL=11_0` before enumerating adapters. The selector accepts `9_1`, `9_2`, `9_3`, `10_0`, `10_1`, `11_0` and `11_1`. Enumeration uses an explicit physical DXGI adapter and a singleton level list and verifies the returned level against `GetFeatureLevel`. Invalid or conflicting selectors produce no DX11 adapters. The legacy `WGPU_DX11_FORCE_FL9_1=1` selector continues to select FL9_1. Automatic enumeration uses its existing highest-first list and retains the retry for runtimes that reject the leading FL11_1 request. Exact selection includes the requested and active level and the adapter LUID in the report. Strict CTS selection verifies the live HAL adapter. [D3D11CreateDevice](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-d3d11createdevice)

The ordinary wgpu contract is still FL9_1-class. Its shader translation, portable storage, texture formats, limits and downlevel flags reflect the pre-existing implementation. The native feature-level result is independent of those capabilities.

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

## Remaining qualification

- The FL9_1 public regression and certification status are separate from the higher-tier native fixtures. See the exact-candidate results and inherited failures in [the test manifest](../deliverables/dx11-levels/FL11_0/tests.json).
- Portable SM5 pipeline creation, storage bindings and hazards, public indirect draws and dispatch, public query semantics, expanded dimensions and formats, shared RTV/UAV allocation and all inherited operations need implementation or qualification. Native-only tests still need stream output, remaining tessellation domains and boundaries, indirect argument rejection cases, sample positions, complete query and format-use matrices, and swapchain lifetime and resize.
- Signed exact-level Surface ARM64, Lumia ARM32 capability and deployment, and independent native-ceiling hardware results remain open. The roadmap's older results provide device controls.
- HorizonUWP shader packs, DRS, HUD, mod markers, presentation and matched performance need a signed build pinned to the qualifying fork SHA. The [benchmark manifest](../deliverables/dx11-levels/FL11_0/benchmarks.json) records the outstanding comparisons.

The [requirement ledger](../deliverables/dx11-levels/FL11_0/requirements.csv) imports the pinned roadmap's operations and DXGI/DDI format evidence and adds API methods from the SDK, HAL methods and public limits and features from the fork. `partial` identifies a recorded passing case whose coverage remains narrower than full qualification. `unknown` identifies rows without matching passing candidate evidence. The [hardware manifest](../deliverables/dx11-levels/FL11_0/hardware.json) identifies the physical target for each run. [D3D11 feature levels](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro) · [FL11_0 format support](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/format-support-for-direct3d-11-0-feature-level-hardware)
