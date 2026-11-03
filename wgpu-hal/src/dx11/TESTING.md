# Direct3D 11 validation plan

The backend is experimental and normal adapter enumeration is deliberately
empty. The existing HAL tests and local experiments do not establish WebGPU or
CTS conformance. Do not expose an adapter using placeholder capabilities to make
a test pass: many restrictions, including specific binding layouts, cannot be
expressed by `Limits` alone.

## 1. HAL tests and GPU readback

### Focused FL9_1 hardware test

From an x64 Visual Studio developer shell, require physical hardware with
`$env:WGPU_DX11_REQUIRE_HARDWARE = "1"` in PowerShell, then run:

```console
cargo test -p wgpu-hal --lib --no-default-features --features dx11 dx11::hal_encoder::tests::fl9_1_hardware_indexed_triangle -- --exact --nocapture
```

The test prints every DXGI adapter and selects the first non-software adapter.
It requests only FL9_1 and asserts the returned level. It writes vertex and
16-bit index data to an upload buffer, records copies into initially zeroed
vertex and index buffers, then submits the copies before an indexed draw through
the private HAL queue. The test checks an 8×8 target before submission, waits
for a fence with a five-second timeout, and checks every pixel after readback.
Resources remain alive until completion and are explicitly destroyed afterwards.
The shader goes through the existing Naga and D3D11 shader pipeline.

Read the printed result, not just libtest's final line:

- `PASS`: the hardware device and all operations completed with the expected pixels.
- `SKIP`: no non-software adapter was enumerated; no GPU test was executed.
  Libtest still reports this case as `ok`, so retain `--nocapture` and inspect
  the result when collecting evidence. With `WGPU_DX11_REQUIRE_HARDWARE=1`,
  absence of a hardware adapter is a failing test instead.
- `FAIL`: enumeration, device creation, execution, synchronization, or
  validation failed. The test exits unsuccessfully. An enumeration failure is
  reported as a failure because its cause cannot safely be inferred to be a
  missing prerequisite. A process or loader failure before the test starts
  also cannot produce a test summary and must be investigated separately.

The test reports the adapter name, PCI IDs, software flag, requested and actual
feature levels, feature-level guarantees, targeted driver queries, backend
implementation status, readback and one-case counts. It does not query driver versions;
record them separately on Windows, for example:

```powershell
Get-CimInstance Win32_VideoController | Select-Object Name,PNPDeviceID,DriverVersion,DriverDate
```

If several physical adapters are present, the first DXGI hardware adapter is
used; check its printed identity before treating a result as evidence for a
particular GPU. No WARP or higher-feature-level fallback is attempted. This
is a private HAL test, not a public wgpu or WebGPU conformance test. It does
not prove leak freedom; a debug-layer lifetime check remains future work.

### Recorded FL9_1 run

On 2026-11-02, Windows 11 Pro build 26300, Rust/Cargo 1.93.1 and 1.99.1,
and NVIDIA driver 32.0.16.2101, the focused test selected the NVIDIA GeForce
RTX 3090 Ti (PCI vendor `0x10de`, device `0x2203`, software `false`). The only requested
and returned feature level was 9_1. The ordered buffer copies, indexed draws,
fence wait and readback executed: all 64 pixels were blue before submission
and red after completion. The focused result was 1 executed, 1 passed, 0
failed, 0 skipped. The Microsoft Basic Render Driver was enumerated as software
and was not selected. No D3D11 debug layer was enabled.

Before the change, the DX11 unit-test command below reported 64 passed and 0
failed. Those tests did not report their hardware execution individually; the
native diagnostic separately confirmed that the RTX could create FL9_1. A
successful suite total therefore should not be interpreted as 64 proven GPU
tests. CPU-only cases include feature-level ordering, queue error mapping and
the no-adapter guard; other cases exercise native GPU paths when an adapter is
available. The same suite reported 64 passed and 0 failed after the change.
The DX11-disabled DX12 GPU tests for mapped-buffer copy/readback and capability
validation each passed on the RTX 3090 Ti. Repeat the focused command to
establish fresh hardware evidence.

### Sparse texture and sampler bindings

With `WGPU_DX11_REQUIRE_HARDWARE=1`, run:

```console
cargo test -p wgpu-hal --lib --no-default-features --features dx11 dx11::hal_encoder::textured_tests -- --test-threads=1 --nocapture
```

This includes sparse bindings 7 and 42 in either numerical order, reversed WGSL
declarations and layout entries, uniform coexistence, and a red-to-green texture
binding transition.
Each checks GPU readback at FL9_1. No-adapter runs fail in strict mode.

### Broader suite

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

A safe core integration test is deferred: the HAL `Adapter` is uninhabited,
its normal instance intentionally exposes no adapters, and core's shader module
path treats the DX11 backend as unreachable. A core device also needs truthful
capabilities and validation for its accepted operations; restrictions such as
specific binding layouts cannot be represented by limits alone. A test-only
bootstrap would require a separately contained adapter and validation contract,
not just an enumeration switch. Do not use placeholder limits to bypass it.

A future tightly scoped hardware integration test should cover actual
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

### Multiple sampled resources

Set `WGPU_DX11_REQUIRE_HARDWARE=1` and run:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 multi_resource -- --nocapture --test-threads=1
```

The cases check three sparse textures, distinct and shared samplers, a texture
using two sampler states, changing one texture, a pipeline transition and eight
textures with a shared sampler or eight samplers, and sixteen distinct sampler
relationships. Each reads back GPU output. The `level9_fxc`
unit test separately checks the FXC boundary of sixteen distinct sampled
texture/sampler relationships versus seventeen; it is not a hardware test.

### Shader-aware NPOT compatibility

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampling_relationships -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 shader_pairs_ -- --nocapture --test-threads=1
```

The first command uses synthetic capabilities and validated WGSL entry-point
analysis. The second executes FL9_1 GPU readback; three cases inject a restrictive
capability into the private test pipeline while leaving the driver unchanged.
They also reject invalid draw, indexed draw and pipeline/bind-group transitions
before submission. The unrestricted NPOT/wrap case requires a positive device
query, and fails in strict mode if that prerequisite is missing.

### FL9_1 sampler translation and patterned sampling

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampler_translation -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampler_state_ -- --nocapture --test-threads=1
```

The first command is CPU-only. The second executes patterned texture readback
for minification, magnification, mixed filters, addressing and a sampler
transition. It also runs a native border-creation diagnostic; that diagnostic
is not a rendered-pixel test. Anisotropy 2 is sampled, but a single-mip image
does not establish visual anisotropic quality.

### FL9_1 texture transfers

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_texture_transfer_chain -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_copy_then_sample -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_buffer_upload_then_sample -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 transfer_layout -- --nocapture
```

The first test checks zeroed resources, padded and offset uploads and readback,
partial and full texture copies, resource retention and ordered submissions.
The second clears a render target, copies it and samples the destination.
The third samples a padded upload with a nonzero source offset. FL9_x readback
rejects native diagnostic textures bound only as shader resources.
The layout test is CPU-only. Direct private HAL calls reject empty transfer
regions; wgpu-core filters validated empty copies before dispatching to HAL.

### FL9_1 uniform bindings

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sparse_uniform_allocation -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 level9_constant_budgets_and_multiple_buffers_compile -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_sparse_uniform_ranges -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_two_uniforms_with_sampled_textures -- --nocapture --test-threads=1
```

The first two are CPU/compiler tests. The hardware tests verify sparse static
ranges, GPU-upload ordering, vertex/fragment use, padded and nested matrix
structures, array stride, pipeline and group transitions, and coexistence with
sampled textures.

### FL9_1 depth attachments

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test -- --nocapture --test-threads=1
```

The test samples a texture while drawing overlapping geometry in both depth
orders, verifies initial zero depth, checks independent depth-only and
stencil-only clears, and checks an Always/no-write pipeline. Ten 8x8 readbacks
are compared in full. It also checks attachment extent and pipeline mismatch
rejection. Stencil tests and read-only/aspect views are not implemented.

### FL9_1 default buffer bind flags

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 level9_default_buffer_bind_flags -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_default_copy_and_uniform_buffers -- --nocapture --test-threads=1
```

The first test is synthetic. The second checks native descriptors and a
copy/readback on a hardware device explicitly created at FL9_1.
