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

## Optional Win32 GPU copy benchmark

The separate `benchmark.cpp` executable does not change the tier probe or its
report format. From an x64 MSVC developer shell, build into a fresh scratch
directory, then run an exact tier under the shared GPU lock:

```powershell
.\build-benchmark.ps1 -OutputDirectory C:\Temp\dx11-copy-bench
C:\Temp\dx11-copy-bench\Dx11CopyBenchmark.exe 10_0
```

The argument accepts `9_1`, `9_2`, `9_3`, `10_0`, `10_1`, `11_0`, or `11_1`.
The default hardware adapter and actual tier are printed. One untimed warmup
precedes five samples, each containing 32 copies of a 4 MiB buffer. D3D11
timestamp and timestamp-disjoint queries measure GPU time, with a five-second
timeout per query. A staging readback compares all 4 MiB after the timed work.
The reported GiB/s is logical bytes copied divided by timestamp duration; it is
not measured memory-bus traffic. This repeated-copy microbenchmark does not
measure wgpu, draws, presentation, gameplay, or end-to-end latency.

Exit 0 means at least one valid sample and a matching readback; disjoint samples
are reported and omitted from the median. Exit 2 means timestamp query creation
reported unsupported, exit 3 means every sample was disjoint, and exit 1 means
an error, timeout, or mismatch. Unsupported, all-disjoint, and error paths have
not been induced in the recorded run. The [receipt](benchmark-receipt.txt)
contains the exact source and executable hashes, adapter, OS and per-sample
results for a short RTX 3090 Ti FL10_0/FL11_0 run. These measurements are not
an architectural comparison between feature levels.
