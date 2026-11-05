# Experimental Direct3D 11 backend

The opt-in `dx11` feature exposes a restricted D3D11 backend on Windows. The
public adapter path enumerates physical DXGI adapters only when they create a
device explicitly requested at FL9_1 and pass the implemented capability
projection. Software adapters are excluded. D3D11 remains opt-in and does not
change default backend selection.

The adapter can be used for offscreen rendering through the public wgpu API,
but it reports no wgpu features, is not WebGPU-compliant, and does not expose
surface capabilities. A public offscreen example renders an indexed,
textured, depth-tested cube and checks full-image readback:

```powershell
cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
```

The strict public integration test requires physical FL9_1 hardware and rejects
missing hardware, software substitution, device-creation failures and wrong
readback:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The current development evidence is from a Windows 11 RTX 3090 Ti explicitly
restricted to FL9_1. It does not establish compatibility with historical
FL9_1 drivers or hardware. The private HAL covers a wider operation subset
than the public API tests; see the capability matrix, test guide and status
report before relying on a particular operation:

- [Capability matrix](CAPABILITIES.md)
- [Testing guide](TESTING.md)
- [Backend status and gaps](STATUS.md)
- [Autonomous FL9_1 plan](FL9_1_PLAN.md)

`cargo run -p wgpu-hal --example dx11_probe --no-default-features --features dx11`
is a native diagnostic. It tries a descending feature-level list and separately
requests FL9_1; it reports DXGI identity and device-creation failures without
claiming wgpu compatibility.

The HAL retains private CoreWindow and borrowed HWND swapchain paths, with
hidden-window tests for clear, readback, present, resize and recreation. They
do not establish visible desktop presentation or safe concurrent/reentrant
use. Public `surface_capabilities` remains unavailable until queue ordering,
immediate-context serialization and DXGI message-pump reentrancy have a safe
cross-layer design.
