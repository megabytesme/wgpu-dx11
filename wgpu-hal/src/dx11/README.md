# Experimental Direct3D 11 diagnostics

The `dx11` feature currently provides a Windows-only physical-adapter probe:

```console
cargo run -p wgpu-hal --example dx11_probe --no-default-features --features dx11
```

The probe enumerates DXGI adapters and asks D3D11 to create a device using a
descending feature-level list from 11_1 through 9_1. Its reported feature level
is the native D3D feature-level value, not a claim of WebGPU compatibility.
Device-creation failures are reported per adapter as HRESULTs. It also tries
an explicitly requested feature level 9_1 on each hardware adapter. The native
adapter/device handles are owned and the immediate context is mutex-protected;
these are experimental building blocks and are not wgpu HAL types. Internal
copy-only buffers support checked byte writes and blocking staging readback;
they are initially uninitialized and do not implement WebGPU buffer semantics.

This is diagnostic scaffolding only. There is no `wgpu-hal` DX11 `Api`, no
wgpu DX11 adapter, surface, shader implementation, command submission, or
rendering. UWP and ARM device execution have not yet been verified.
