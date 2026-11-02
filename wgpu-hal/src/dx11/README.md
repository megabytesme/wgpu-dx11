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
entry points with explicitly mapped uniform buffers, plain 2D float textures,
and non-comparison samplers, then compiles them with FXC. Texture arrays,
storage, comparison samplers, and immediate bindings are rejected; general
WGSL support and device capability validation remain unimplemented. A desktop
FL 9_1 GPU test draws and reads back an actual sampled texture.

The registered DX11 HAL instance deliberately exposes no adapters. It retains
validated UWP CoreWindow handles as agile COM references. The private HAL
surface supports only opaque FIFO BGRA8 presentation and the restricted device,
encoder, and texture paths do not yet provide a usable wgpu renderer. Native
diagnostic primitives do not yet implement WebGPU semantics.

An internal CoreWindow swapchain owns its device and window, enforces one
outstanding backbuffer, and implements presentation and resize. A strict x64
UWP diagnostic checks GPU readback before and after resize at forced 9_1 and
11_0 on desktop hardware. The primitive backs a restricted HAL surface and
queue path, including HAL texture views
and fence ownership checks. Adapter enumeration remains empty.

A restricted FL 9_1 render path records one unblended triangle-list pass with
one Float32x2 vertex stream. A strict UWP diagnostic compiles resource-free WGSL,
uploads a texture through mapped-buffer and recorded-copy paths, records a
sampled triangle through the HAL encoder, checks a backbuffer pixel and presents. This sampled shader uses unchecked runtime behavior; the path does
not provide general shaders, render passes, or WebGPU.

The same restricted encoder can bind one validated fragment-stage 2D float
texture and sampler. On feature level 9, non-power-of-two textures reject
repeat/mirror addressing; unsupported layouts and texture/attachment aliasing
are rejected rather than silently rendered incorrectly.

Restricted RGBA8 buffer-to-texture and texture-to-buffer copies are recorded
and replayed in queue order. They use staging and synchronous CPU mapping on
D3D11, so these paths may stall; unsupported subresources and layouts fail.

A private uniform-range prototype stages an ordinary logical buffer range and
updates a separate constant buffer in full before binding. A single fixed
uniform group is supported with explicit minimum binding size and validated
shader layout, either alone or together with one 2D texture and sampler;
dynamic offsets and checked uniform shaders remain unsupported.
The staging path may stall and avoids D3D11's partial-update restriction.
