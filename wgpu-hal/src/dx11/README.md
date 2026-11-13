# Experimental Direct3D 11 backend

The opt-in `dx11` feature exposes a restricted D3D11 backend on Windows. The
public adapter path enumerates physical DXGI adapters only when they create a
device explicitly requested at FL9_1 and pass the implemented capability
projection. Software adapters are excluded. D3D11 remains opt-in and does not
change default backend selection.

The adapter can be used for offscreen rendering and constrained native HWND
or validated WinRT CoreWindow surfaces through the public wgpu API. CoreWindow
creation requires its dispatcher thread; configure, acquire and present must
remain on the captured owner thread. CoreWindow runtime behavior on UWP/Lumia
is not yet verified. It exposes the native-only
`TEXTURE_FORMAT_16BIT_NORM` and `TEXTURE_FORMAT_16BIT_PACKED` features only
when the selected device reports the format support required by their
implemented paths. It is not WebGPU-compliant. HWND and CoreWindow surfaces
expose BGRA8 sRGB FIFO presentation, opaque composition, one frame of latency
and render-attachment usage. Configure, acquire and present must run on the
surface's creating thread.
Queue submissions, writes, mapped-buffer updates and context-backed resource
creation are serialized against Present; same-thread reentrant work that would
touch queue/context state is rejected. Drop views of the previous backbuffer
before reconfiguring a resized surface. The strict lifecycle test uses a
hidden HWND. A separate visible-window example pumps messages, renders and
resizes; its shown HWND and frame presents were verified, and a client-area
capture was visually inspected to confirm the distinct cube faces and clear
background.

A public offscreen example renders an indexed, textured, depth-tested cube and
checks full-image readback:

```powershell
cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
```

The visible-window example creates a native HWND, pumps messages, renders a
rotating indexed textured cube with depth testing, and handles resize. Close
the window to exit; `--frames=N` bounds a smoke run:

```powershell
cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std -- --frames=120
```

A 60-frame run passed on the RTX 3090 Ti at forced FL9_1. The visible client
area was captured and visually inspected for the distinct cube faces and clear
background. Unlike the offscreen example, the windowed example does not make
an automated pixel-level assertion about presented pixels.

This is ready for an opt-in project integration trial when the application's
rendering subset matches the [capability matrix](CAPABILITIES.md). Keep a
fallback backend available: compute/storage, indirect draws, query sets,
multiple color targets, ordinary texture arrays and unsupported shader forms
remain unavailable or rejected. The current coverage is not a claim that a
general wgpu application will run unchanged.

The strict public integration test requires physical FL9_1 hardware and rejects
missing hardware, software substitution, device-creation failures and wrong
readback. It also checks hidden HWND configure, render, present, resize and
owner-thread behavior:

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
requests FL9_1; it reports DXGI identity, selected-device support for several
optional float texture formats, and device-creation failures without claiming
that a native format query alone implies HAL or wgpu compatibility.

The HAL supports WinRT CoreWindow and borrowed HWND swapchain paths, and the
public capability model now exposes both. Hidden-HWND tests cover clear,
readback, present, resize and recreation; a strict public test covers HWND
configure, render, present, resize and owner-thread rejection. No CoreWindow
UWP/Lumia runtime test has been run.
