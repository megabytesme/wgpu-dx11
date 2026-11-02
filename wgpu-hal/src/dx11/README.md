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
An internal in-memory FXC compiler can produce DXBC for selected FL 9_1 and
shader-model 5 profiles. A narrow internal path translates validated Naga
entry points with explicitly mapped uniform buffers, then compiles them with
FXC. Texture, sampler, storage, and immediate bindings are rejected; general
WGSL support and device capability validation remain unimplemented.

The registered DX11 HAL instance deliberately exposes no adapters. It retains
validated UWP CoreWindow handles as agile COM references, but HAL surface
configuration, devices, shader pipelines and rendering are unavailable. Native
diagnostic primitives do not yet implement WebGPU semantics.
