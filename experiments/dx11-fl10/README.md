# FXC compute boundary control

Run `powershell.exe -NoProfile -ExecutionPolicy Bypass -File experiments/dx11-fl10/fxc-compute-boundaries.ps1` from the repository root on Windows. The script requires the Microsoft `d3dcompiler_47.dll`; it compiles in memory and does not need a GPU or build the workspace.

The control checks that FXC accepts a structured UAV in `cs_4_0`, rejects a typed UAV in `cs_4_0` with X3666, and accepts the same typed UAV in `cs_5_0`. The positive `cs_5_0` case distinguishes the profile restriction from malformed HLSL. This is a compiler control only: it does not establish device support, execute a shader, validate all DirectCompute 4.x restrictions, or expose compute through wgpu. FL10_0 compute is optional and requires a device feature query and separate execution tests. See Microsoft's [DirectCompute downlevel restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/direct3d-11-advanced-stages-compute-shader).

On Ben-Desktop, Windows PowerShell 5.1 passed all three controls on 2026-11-21. This result is independent of the uncommitted FL10_0 candidate and is not a certification of it.
