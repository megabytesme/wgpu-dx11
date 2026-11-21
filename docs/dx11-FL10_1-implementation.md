# D3D11 feature level 10.1 development

**Certification status: INCOMPLETE.** This worktree was started from
`aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`. Promotion requires the reviewed
FL10.0 candidate and all earlier certification records. The HorizonUWP
November 20 product remains pinned separately.

## Native interface

Enable the `wgpu-hal` Cargo feature `dx11-experimental-fl10` to use
`wgpu_hal::dx11::exact::ExactDevice`. Enumerate the physical DXGI adapters,
choose an adapter, and pass a single `FeatureLevel` to `ExactDevice::open`.
The open operation checks the reported device level and the software flag on
the live adapter. The caller can record the requested level, actual level,
adapter name, PCI identifiers, and LUID. Trying another level is a separate
open on the same adapter.

Native callbacks run under the immediate-context mutex on the separately
owned device. Bindings are cleared on return and on unwind. The callback
finishes CPU mappings and paired queries before exit. Native device children
can be used again on the same device. An operation that reads GPU output
uses a completion query or staging map. Wgpu resource tracking, portable
validation, and wgpu queue ordering are outside this interface.

The FXC selector supports exact-level FL9.1 graphics controls and FL10.0/10.1
vertex, geometry and pixel profiles. The compute profile requires a positive
`D3D11_FEATURE_D3D10_X_HARDWARE_OPTIONS` result. A failed optional query and an
unsupported optional feature are distinct outcomes. The native interface
shares the fork's existing desktop and UWP FXC loading path.
[Direct3D 10.1 changes](https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-10-1) ·
[Optional FL10.x compute](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_feature_data_d3d10_x_hardware_options)

## Desktop verification

From the dedicated wgpu worktree, use the Visual Studio developer shell with
the Windows 10 SDK 10.0.19041.0. The builds use Rust from
`rust-toolchain.toml`. Set `CARGO_TARGET_DIR` to a lane-specific scratch
directory with enough space for both architectures. The evidence names the
source commit and tree tested, as well as the SHA-256 of each executable.

```powershell
$env:CARGO_TARGET_DIR = 'E:/temp/wgpu-fl10_1/target'
$env:CARGO_PROFILE_TEST_DEBUG = '0'
$env:CARGO_INCREMENTAL = '0'
cargo test --offline --target x86_64-pc-windows-msvc -p wgpu-hal --no-default-features --features dx11-experimental-fl10 --lib --no-run
cargo test --offline --target i686-pc-windows-msvc -p wgpu-hal --no-default-features --features dx11-experimental-fl10 --lib --no-run
```

For each executable printed by Cargo, pass its path as `-Executable` to
`deliverables/dx11-levels/FL10_1/run-desktop.ps1` and pass a fresh file path
as `-Log`. The runner acquires the shared desktop GPU lock for execution.
The disassembly assertions and GPU oracles run in the same executable.
The suite exercises native and Naga-generated gather, cube-array
sampling and copies, eight independent blend outputs, 4× multisample
interpolation and loads, geometry stream output and DrawAuto, and
optional structured-buffer compute. FXC results and native operation
results are recorded in the lane's
[`tests.json`](../deliverables/dx11-levels/FL10_1/tests.json) and
[`hardware.json`](../deliverables/dx11-levels/FL10_1/hardware.json).
On Ben-Desktop, all seven cases passed with x64 and x86 binaries on the
RTX 3090 Ti. The focused forced-FL9.1 adapter regression passed in each
binary. The baseline DX11-only check and the opt-in test lint completed
with existing warnings.

The stream-output fixture uses the inferred stride of one buffer
and stream zero. It checks the raw vertex bytes, stream-output
statistics, and the subsequent DrawAuto render.
[Stream-output setup](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-output-stream-stage-getting-started) ·
[DrawAuto contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-drawauto)

The native microbenchmark uses the same adapter, 512 × 256 RGBA8 color and
D24S8 depth targets, geometry, draw count, and warm-up for each exact tier.
It rotates the order of FL9.1, FL10.0 and FL10.1. It records submission time,
completion time, a disjoint-checked GPU interval where available, and four
samples of current process video memory per run. See
[`benchmarks.json`](../deliverables/dx11-levels/FL10_1/benchmarks.json).
The median GPU interval was 47.104 µs for all three tiers in both
architectures. Maximum sampled local video memory was 9,908,224 bytes
at each tier.
Horizon gameplay performance and peak video memory over a gameplay trace
require a product build on the reviewed integration candidate.

## Portable renderer opt-in

Build `wgpu` with `dx11-experimental-fl10` and `wgsl`. Select
`Backends::DX11` explicitly and set `WGPU_DX11_EXPERIMENTAL_FL10_1=1`
before enumerating adapters. The DX11 backend requests only FL10.1 and
uses the native hardware-identity checks. The experimental adapter is
identified by `FL10_1 (experimental portable subset)` in its driver info.
An explicit `WGPU_DX11_FORCE_FL9_1=1` takes precedence.

The portable path creates an alpha-to-coverage blend state even when
color blending is disabled and the pipeline writes all color channels.
Its GPU fixture draws three alpha values into a four-sample attachment,
resolves into a single-sampled texture, and checks GPU readback. It also
checks rejection of one-sample alpha-to-coverage and of the feature on
the legacy path. Resource-free WGSL stages with no emulated vertex
identifiers use SM4.1 FXC profiles. The remaining stages retain the legacy
compiler path while their HAL contracts are expanded.

```powershell
cargo test --offline --target x86_64-pc-windows-msvc -p wgpu --no-default-features --features std,wgsl,dx11-experimental-fl10 --test dx11_fl10_1 --no-run
```

Run the test executable using `run-desktop.ps1` with
`-Filter public_dx11_alpha_to_coverage_requires_opt_in_and_resolves`.
Set the environment before running the script. For Surface, build the
wgpu integration test and wgpu-hal unit tests for
`aarch64-pc-windows-msvc`, and pass both executables to
`run-surface.ps1`. The runner verifies the transfer hashes over SSH
and keeps the Surface lock through completion. These are Win32 tests.
The ARM64 and ARM32 UWP library checks are separate build evidence.

The portable test passed on RTX 3090 Ti using x64 and x86, and on
Surface Pro X using native ARM64 Win32. The alpha-0.5 draw resolved to
average red 127 on RTX and 135.875 on Adreno 680. Both lower-path controls
passed on desktop. Six native format fixtures passed on each of the three
targets. The evidence includes executable digests and the Surface transfer
paths. [Follow-up test record](../deliverables/dx11-levels/FL10_1/portable-and-formats.json)

The follow-up native format cases exercise all five newly mandatory
filtering entries from the Direct3D FL10.1 format table, including
typed views of D24S8 and D32S8 resources, and nine normalized color
formats with native additive blending. Both suites check input support
bits and verify pixel values from real render targets. The P010 and P016
entries require separate video-plane coverage.
[Direct3D FL10.1 format table](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/format-support-for-direct3d-feature-level-10-1-hardware)

The installed signed roadmap reference on Lumia reported exact FL10.1
creation on Adreno 430 and successful native draw and dispatch readbacks.
Its combined RGBA8 4× resolve and invalid-33-sample control failed.
The complete report was already present when retrieved. Those observations
apply to the roadmap reference package.
[Signed Lumia reference output](../deliverables/dx11-levels/FL10_1/evidence/lumia-signed-reference.log)

The signed branch probe passed on Lumia ARM32 at the same tested source
revision. The portable 4× draw and resolve produced average red values
of 0, 135, and 255. Both ARM packages signed and passed strict import
checks. Their build used the November 20 UWP Rust standard-library
patches and an isolated `parking_lot_core` override that links UWP
address-wait functions through WindowsApp. The isolated package
receipts identify the runtime result for each target.
[AppContainer build and run receipts](../deliverables/dx11-levels/FL10_1/appcontainer-results.json)

The signed ARM64 package is ready for the Surface. Its deployment
attempts timed out at SSH before transfer. The recorded ARM64 Win32
results cover the portable fixture and the native format suite.

The RGBA8 render oracles use the Direct3D conversion tolerance of 0.6
integer ULP. This accounts for the NVIDIA driver converting 0.5 to 127.
The driver also returned success for a seven-face cube resource, though
the resource descriptor documentation calls for a multiple of six.
The cube fixture records that observation and requires rejection of a
view that would extend outside its underlying texture. Portable view
validation still needs to enforce its own cube shape constraints.
[Direct3D conversion rules](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm) ·
[Texture2D descriptor](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_texture2d_desc)

## Remaining certification

- Route FL10.1 through the graphics HAL and Naga, then expose each
  supported format, feature, and limit based on a tested implementation.
  This includes array/depth views, copies and mipmaps, register and
  input-assembler limits, resource bindings, per-format render paths,
  native query and predication behavior, and remaining shader and
  rasterizer semantics.
- Finish method-level and per-format native operation coverage, including
  optional prerequisites and negative boundaries. The
  [requirement ledger](../deliverables/dx11-levels/FL10_1/requirements.csv)
  expands the pinned FL10.1 roadmap rows and records native, HAL, and
  public results separately. Regenerate the ledger with
  `python deliverables/dx11-levels/FL10_1/update-requirements.py C:/dev/personal/wgpu-dx11-roadmap-Nov21`.
- Run the inherited suite on the candidate based on certified FL10.0.
  Validate D3D11 public API coverage, CTS, lifetime, real device removal,
  resize and presentation on the relevant desktop and signed AppContainer
  targets. Run ARM64 on Surface and probe the Lumia driver before using
  ARM32 results for this tier.
- Verify the signed HorizonUWP renderer with the same fork SHA, including
  shaders, dynamic resolution, HUD, mod markers, and paired gameplay
  timing and memory. Submit a gate request only after independent review.
