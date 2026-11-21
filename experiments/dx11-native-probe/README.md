# D3D11 native feature-level probe

This standalone native fixture requests the seven D3D11 feature levels from FL9_1
through FL11_1 individually. It records the results of creation, `GetFeatureLevel`,
format and MSAA queries, offscreen GPU readbacks, shader-stage controls and device
status. It can run as a Win32 executable or a UWP AppContainer.

Run under Windows PowerShell 5.1 from this directory with VS 2022 Build Tools
(MSVC 14.44), Windows SDK 10.0.19041.0 and the corresponding native build tools.
ARM32 AppContainer linking uses the 10.0.15063.0 libraries. Override `-MsvcToolset`
and `-SdkRoot` if installed elsewhere.

```powershell
.\build.ps1 -Architecture x64 -OutputDirectory E:\Temp\wgpu-d3d11-x64
E:\Temp\wgpu-d3d11-x64\Dx11TierProbe.exe > E:\Temp\wgpu-d3d11-x64\native.log
python .\parse-probe.py E:\Temp\wgpu-d3d11-x64\native.log
```

For AppContainer, use `-AppContainer` and `-SigningThumbprint` for a trusted
development certificate in `CurrentUser\My`. Build an unsigned staging package
with `-Publisher 'CN=wgpu D3D11 Development'`. The script creates package artwork
and the `WgpuDx11.TierProbe` identity. The target needs the matching
`Microsoft.VCLibs.140.00` runtime. In the interactive user session on an ARM64
Windows device, run `run-surface.ps1 -Package <path> -Sha256 <digest>` to deploy
a signed ARM64 build and capture fresh LocalState output. On Windows Mobile,
use the paired Device Portal and verify the package state and
`dx11-tier-probe.txt` in LocalState. Acquire the lab's shared physical-device
lock before GPU testing or deployment.

The 4× RGBA8 resolve records all 64 pixel comparisons and the first observed
RGBA value. The 33-sample texture descriptor test separately reports the raw
creation HRESULT and resource presence. These controls run from FL10_1 through
FL11_1 regardless of the other control's result. An overall PASS covers the
fixture only. Keep native results distinct from wgpu HAL, public API, CTS, and
presentation and lifecycle receipts.
