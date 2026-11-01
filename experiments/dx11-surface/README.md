# UWP D3D11 surface probe

This standalone probe checks the platform primitives needed by a potential
Direct3D 11 backend. It requests feature level 11_0, creates a four-layer
texture and view, creates a CoreWindow swap chain, and verifies a cleared
backbuffer through CPU readback before presentation. It is not a wgpu HAL
implementation. The Rust owner is deliberately bound to the UI thread.

The UWP host writes its result to `LocalState/wgpu-dx11-surface-probe.txt`.
It also handles resize and releases the backbuffer before resizing.

For x64, compile the static library with a UWP-capable Rust standard library:

```powershell
cargo +stable build --manifest-path experiments/dx11-surface/Cargo.toml `
  --target x86_64-uwp-windows-msvc --release
```

The experimental Rust UWP target may require a locally patched standard
library and `-Z build-std=std,panic_abort`. In a Visual Studio UWP developer
shell using the Windows 10 SDK 15063, set `DX11_PROBE_LIB` to the resulting
`wgpu_dx11_surface_probe.lib` and `UWP_STRICT_LINK_PS1` to an import-auditing
link script, then run `experiments\dx11-surface\host\build.cmd`. The executable
requires a UWP package manifest and must be activated on the UI thread.

The strict-link script is external because the existing UWP Rust standard
library workaround belongs to the Horizon project. A successful build does
not establish wgpu or WebGPU conformance.
