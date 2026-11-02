# Direct3D 11 validation plan

The backend is experimental and normal adapter enumeration is deliberately
empty. The existing HAL tests and local experiments do not establish WebGPU or
CTS conformance. Do not expose an adapter using placeholder capabilities to make
a test pass: many restrictions, including specific binding layouts, cannot be
expressed by `Limits` alone.

## 1. HAL tests and GPU readback

On a Windows machine with the DX11 feature enabled, run:

```console
cargo test -p wgpu-hal --lib --no-default-features --features dx11 dx11::
cargo clippy -p wgpu-hal --lib --no-default-features --features dx11 --no-deps -- -D warnings
```

The private HAL tests cover buffer and texture transfers, mapped shadows,
resource ownership and bounds rejection, fences, shaders, binding layouts,
rendering, and pixel readback. The original WGSL fixture in
`wgpu-hal/tests/dx11/fixtures/` exercises a checked two-texture overlay;
the opt-in FL9 texel-load lowering tests are in `wgpu-hal/tests/dx11/` and
are compiled as unit modules because they access private backend internals.
They do not enable the experimental lowering in the HAL.

Some tests return successfully when no hardware adapter exists. Record the
adapter name, actual device feature level, test count and readback result, and
verify that the hardware test paths ran; a successful test command alone is
not evidence of a GPU execution. Cross-compile the HAL and strict UWP host for
each target separately, and run the CoreWindow surface/readback probe on real
UWP devices. A compile or package is not a runtime result.
Before upstreaming, move the hidden diagnostic-only surface helpers out of the
public HAL surface API or replace them with a test-only seam; keep the normal
backend free of application-specific fixtures and window-host assumptions.

## 2. wgpu-core and public API integration

Add a reproducible, tightly scoped hardware integration test covering actual
wgpu-core device and queue initialization, empty and nonempty layouts, mapped
upload/copy/readback, checked shaders, rendering, errors and destruction. An
isolated local diagnostic has exercised device startup, mapped copying and an
indexed triangle via public wgpu with deliberately unsafe placeholder limits;
it is not an adapter and is not sufficient for release. The future test must
be safely contained and must not advertise those placeholders. Test device
loss, lifetimes, synchronization, initialization, and surface resize and
presentation, including diagnostics from the validation layers.

## 3. Upstream wgpu GPU tests

Once a DX11 adapter can truthfully describe and fulfill the required wgpu
contract, run the upstream suite:

```console
cargo xtask test --test wgpu-gpu
```

Use the GPU-test harness's backend selection (for example `WGPU_BACKEND=dx11`)
and verify that its adapter report actually selects this fork's DX11 adapter.
Record passed, failed, skipped and unrun tests separately. A zero-adapter run
or a skipped test is not a pass. Keep other backend builds and tests working.

## 4. WebGPU CTS

Only after the public adapter and test-runner integration are ready, start with
selected CTS cases and then run the pinned full suite:

```console
cargo xtask cts --backend dx11 'webgpu:api,operation,command_buffer,basic:*'
cargo xtask cts --backend dx11
```

`--backend` selects the CTS backend and expectation handling; it does not
make an unavailable adapter usable. At present `deno_webgpu/Cargo.toml` does
not enable the DX11 core feature on Windows, so wire and verify the local CTS
runner before using these commands for DX11. Confirm the adapter and backend
used in the run. Preserve the CTS revision, selected cases, outcomes, skips,
platform and driver details. Do not claim conformance from a selected subset.

## Hardware and validation matrix

For each applicable phase, exercise native D3D feature levels 9_1, 9_3 and
11_0 on suitable hardware or explicitly forced supported levels, including
x64 Windows/UWP, ARM64 UWP and ARM32 Windows 10 Mobile where available.
Record the native level actually created; an ANGLE-reported level does not
by itself establish the native device's maximum. Include texture-size and
NPOT restrictions, 16-bit indexed drawing and FL9 draw limits in level-9
coverage. Where a target cannot run a phase, record it as untested with the
reason rather than extrapolating from another machine.

Add an opt-in D3D11 debug-layer device mode and run it where the Windows
Graphics Tools/debug layer is installed. Capture and investigate all debug
messages, including binding hazards, descriptor errors, lifetime leaks and
synchronization problems. The current normal device-creation path does not
enable the debug layer. Installing missing system components may require
administrator access; do not silently substitute a non-debug run.
