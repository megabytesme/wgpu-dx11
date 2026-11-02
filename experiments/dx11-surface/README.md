# UWP D3D11 surface probe

This standalone probe checks platform primitives for a Direct3D 11 backend.
By default it requests feature level 11_0 and creates a four-layer texture and
view. Set `DX11_PROBE_FEATURE_LEVEL=9_1` at build time to request 9_1 and skip
the texture-array check. Both modes create the backend's owned CoreWindow swap
chain through HAL Surface and Queue, compiles a resource-free WGSL shader,
records a triangle through the HAL render encoder, and checks its GPU readback
and presentation, plus discard and resize. The HAL instance exposes no
adapters; this native proof is not a wgpu render pipeline. The UWP host calls
it from the UI thread.

The UWP host writes its result to `LocalState/wgpu-dx11-surface-probe.txt`.
It also handles resize and releases the backbuffer before resizing.

For x64, compile the static library with a UWP-capable Rust standard library:

```powershell
cargo +stable build --manifest-path experiments/dx11-surface/Cargo.toml `
  --target x86_64-uwp-windows-msvc --release
```

The experimental Rust UWP target may require a locally patched standard
library and `-Z build-std=std,panic_abort`. The host's strict import audit
also requires a UWP-compatible `parking_lot_core` implementation: the default
Windows implementation may retain the desktop-only `GetModuleHandleA` import.
Horizon supplies such an override externally; it is not part of this fork.
In a Visual Studio UWP developer
shell using the Windows 10 SDK 15063, set `DX11_PROBE_LIB` to the resulting
`wgpu_dx11_surface_probe.lib` and `UWP_STRICT_LINK_PS1` to an import-auditing
link script, then run `experiments\dx11-surface\host\build.cmd`. The executable
requires a UWP package manifest and must be activated on the UI thread.

The strict-link script is external because the existing UWP Rust standard
library workaround belongs to the Horizon project. A successful build does
not establish wgpu or WebGPU conformance.
