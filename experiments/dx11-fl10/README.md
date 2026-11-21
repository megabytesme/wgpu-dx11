# FXC compute boundary control

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File experiments/dx11-fl10/fxc-compute-boundaries.ps1` from the repository root on Windows. The script requires the Microsoft `d3dcompiler_47.dll`; it compiles in memory and does not need a GPU or build the workspace.

The control checks that FXC accepts structured and raw UAVs in `cs_4_0`, rejects a typed UAV in `cs_4_0` with X3666, and accepts the same typed UAV in `cs_5_0`. The positive `cs_5_0` case distinguishes the profile restriction from malformed HLSL. This is a compiler control only: it does not establish device support, execute a shader, validate all DirectCompute 4.x restrictions, or expose compute through wgpu. FL10_0 compute is optional and requires a device feature query and separate execution tests. See Microsoft's [DirectCompute downlevel restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/direct3d-11-advanced-stages-compute-shader).

On Ben-Desktop, Windows PowerShell 5.1 passed all three controls on 2026-11-21. This result is independent of the uncommitted FL10_0 candidate and is not a certification of it.

## Native execution control

`compute-probe.cpp` creates a separate D3D11 device at exactly FL10_0 and queries `D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS`. When the optional compute bit is false, it reports `UNSUPPORTED` and exits 2 without dispatching. When true, it runs separate `cs_4_0` structured and raw byte-address buffer shaders. Each dispatches 16 groups, copies the UAV to a staging buffer and checks all 16 words. Errors and mismatches exit 1; successful readbacks exit 0. This tests native D3D11 independently of the wgpu backend and does not enable compute in wgpu.

From an x64 MSVC developer command prompt at the repository root, build into a disposable directory and run either `compute-probe.exe warp` or `compute-probe.exe hardware`:

```cmd
cl /nologo /EHsc /W4 /WX /std:c++17 /Fe:<scratch>\compute-probe.exe /Fo:<scratch>\compute-probe.obj experiments\dx11-fl10\compute-probe.cpp /link d3d11.lib d3dcompiler.lib
```

Coordinate runtime access using the shared desktop GPU lock even for WARP. On Ben-Desktop on 2026-11-21, the x64 build above succeeded in `%TEMP%\wgpu-dx11-fl10-compute-probe`. Under that lock, both WARP and the default NVIDIA RTX 3090 Ti hardware adapter reported exact FL10_0 and `compute4=1`; all 16 structured and all 16 raw words matched on each. The [receipt](compute-receipt.txt) records the OS, adapter identities and source and executable hashes. The absent-feature branch remains untested.
