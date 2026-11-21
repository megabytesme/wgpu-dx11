# Native FL9_1 sRGB blend diagnostic

This standalone Win32 control renders 8×8 pixels for each native
`R8G8B8A8_UNORM_SRGB`, `B8G8R8A8_UNORM_SRGB`, and `B8G8R8X8_UNORM_SRGB`
render target. A green destination with alpha 0.75 and a non-gray source with
alpha 0.5 use `SRC_ALPHA` / `INV_SRC_ALPHA` for both RGB and alpha. Every raw
four-byte pixel is logged; row pitch, RGB channel order, and all alpha bytes are
checked. The BGRX X byte is recorded but ignored. Ideal RGB models are encoded
`[128,128,94]` and linear `[188,188,137]`; RGB and alpha use ±2 byte tolerance.
The models are reported as RGB, while raw pixels are in native memory order.

The literal default-adapter `D3D_DRIVER_TYPE_HARDWARE` creation mode is reported
against Microsoft's [feature-level 9.x blend-space rule](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_blend_desc):
an all-linear observation is a rule discrepancy, not a passing encoded-space
result. Explicit adapter creation uses `D3D_DRIVER_TYPE_UNKNOWN` and is reported
separately as outside that conditional rule. A mixed or unrecognized result is
unclassified. All three are diagnostic outcomes: `smoke=PASS` means setup,
readback and alpha checks completed, not that the native rule or public WebGPU
behavior passed. This fixture changes no backend capability or smoke report.

Build from an x64 MSVC developer shell, substituting a fresh scratch directory:

```cmd
cl /nologo /EHsc /W4 /WX /O2 /std:c++17 /Fe:<scratch>\srgb-blend.exe /Fo:<scratch>\srgb-blend.obj experiments\dx11-native-probe\srgb-blend.cpp /link d3d11.lib dxgi.lib d3dcompiler.lib
```

Acquire the shared GPU lock before running `srgb-blend.exe hardware` and
`srgb-blend.exe adapter 0`. The latter selects a physical DXGI adapter by index;
compare reported LUIDs before attributing differences to creation mode.

The [RTX 3090 Ti receipt](srgb-blend-receipt.txt) records both creation modes
at exact FL9_1 on the same LUID. All 64 pixels in each of the three formats
matched the linear model, with all applicable alpha bytes within tolerance.
Thus the literal HARDWARE run is recorded as a discrepancy with the cited
rule; the UNKNOWN run is a separate observation. This is one adapter and one
blend equation, not a general hardware or wgpu certification.
