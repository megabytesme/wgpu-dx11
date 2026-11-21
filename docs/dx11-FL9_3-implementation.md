# Experimental D3D11 feature level 9_3

Status **INCOMPLETE**. The branch starts at the known-good fork revision
aa7b65c43ef008d0a92d20fcdf9f10fa0883f157. The FL9_1 and FL9_2 certification gate files
are absent. The November 20 Horizon product remains at
5c26a8060c1cf2dce20ee5707e84699ed0879e5c.

## Opt-in and backend behavior

Set WGPU_DX11_FEATURE_LEVEL to 9_3 before creating a D3D11 instance. The backend opens
a physical DXGI adapter with a singleton feature-level request, checks the returned
level against ID3D11Device::GetFeatureLevel, and reports the requested and actual levels,
PCI identity, driver version and DXGI LUID. The exact selector also accepts 9_1 and 9_2.
An invalid request or a conflicting WGPU_DX11_FORCE_FL9_1 value yields no D3D11 adapters.
The existing automatic enumeration remains the default. Exact 9_2 in this worktree
uses the pinned baseline renderer and serves as a control for the FL9_3 delta.

The opt-in renderer selects the 10Level9 vertex and pixel profiles for FL9_3 and keeps
the existing downlevel safety checks for shader translation and binding. Ordinary fragment
derivatives and explicit gradients use native FXC instructions. The public downlevel
contract restricts derivatives to the default precision. FXC requires contiguous pixel
output semantics, so sparse shaders receive padding outputs in the native translation
while retaining their original public locations. [FXC targets](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets) · [WGSL derivatives](https://www.w3.org/TR/WGSL/#derivative-builtin-functions)

The public adapter exposes four color attachments and a 32-byte per-sample limit. Pipeline,
render pass and render bundle validation apply the native same-bit-depth rule. The encoder
preserves slot indices and records individual clears and resolves. The blend state supports
per-target write masks with a common blend equation and enable bit. General independent
blending and alpha-to-coverage remain unavailable. [Device restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device#id3d11devicecreateblendstate) · [Output-merger restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context#id3d11devicecontextomsetrendertargets) · [WebGPU byte accounting](https://www.w3.org/TR/webgpu/#abstract-opdef-calculating-color-attachment-bytes-per-sample)

FL9_3 probes native sampler LOD clamp support when opening the device. On the tested RTX
runtime, the probe selects the inherited, view-checked normalization for supported clamp
descriptors. That fallback prevented the device loss seen when the first candidate
sent finite MaxLOD values to the FL9_3 device. Full LOD and anisotropic coverage remain open.

The experimental HAL device exposes with_fl93_native_context through Device::as_hal.
Native operations must use independently owned resources. The callback shares the HAL's
immediate-context lock, clears D3D11 state at entry and exit, and checks device removal
before and after the operation. The caller owns external synchronization and GPU completion.
The interface allows tests to create native formats and views outside wgpu's portable API,
including BGRX8 textures and GenerateMips.

## Reproduce the desktop evidence

Run in this worktree on Ben-Desktop from the Visual Studio Developer Command Prompt
for the chosen target. Set CARGO_TARGET_DIR to an isolated directory with sufficient
space. The focused tests use the offline locked dependency cache.

    cargo test --no-run --offline --locked -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_fl93 --test dx11_index_ranges --test dx11_fl91_scene
    cargo test --no-run --offline --locked -p wgpu-hal --no-default-features --features dx11 --lib
    cargo clippy --offline --locked -p wgpu-hal --no-default-features --features dx11 --tests

Build the public tests with --target i686-pc-windows-msvc from the matching developer
prompt for x86. Hold the coordinator desktop GPU lock only during test execution.
Set WGPU_DX11_REQUIRE_HARDWARE=1 and WGPU_DX11_FEATURE_LEVEL=9_3. Run the built
dx11_fl93 executable with --nocapture --test-threads=1. Run the HAL binary with the
tier_fl9_3 filter to exercise native mip readback and the exact FL9_2 shader-object
negative control separately. Invoke exact_selector with each requested level and an
unsupported level. Repeat the predecessor Uint32 index and scene controls at their
recorded settings. The table in tests.json gives the exact commands and digests.

## Desktop results

The focused public tests passed on x64 and x86 using exact physical FL9_3. They compared
every pixel of four 8x8 RGBA8 targets, including a D24S8 attachment, independent masks,
common additive blending, sparse output location 2, an explicit-gradient mip selection,
and the four/one/four attachment transition. Controls rejected five targets, mismatched
color bit depths, independent blend equations and explicit fine derivatives. The FL9_1
Uint32 index fixture and the 32x32 textured cube passed at their recorded settings.

The x64 native callback created both FL9_3 shader stages and generated and read back mip
2 on BGRX8 and RGBA32F textures. The compiler rejected FL9_3 derivatives under the
FL9_1 pixel profile. The separate native negative check **failed** when the RTX driver
accepted the FL9_3 bytecode on its exact FL9_2 device. Keep the expected rejection and
observed HRESULT separate. [10Level9 pixel shader profiles](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device#id3d11devicecreatepixelshader)

| Architecture | FL9_1 median | FL9_2 median | FL9_3 median | FL9_3 observed peak process working set |
| --- | ---: | ---: | ---: | ---: |
| x64 | 4.63 ms | 4.89 ms | 4.85 ms | 58.1 MiB |
| x86 | 9.03 ms | 9.14 ms | 7.83 ms | 48.0 MiB |

Five samples per tier used the same 32x32 scene and binary on each architecture. Timing
covers command recording, submission and mapped readback. Process working set includes
CPU-side allocations. GPU peak memory remains unmeasured. See
[raw benchmark rows](../deliverables/dx11-levels/FL9_3/benchmarks.csv) and
[desktop test receipts](../deliverables/dx11-levels/FL9_3/tests.json).

## Remaining certification work

- Integrate onto the reviewed FL9_2 revision after FL9_1 and FL9_2 are certified.
  Replay all inherited cases on that cumulative source revision.
- Verify full format, transfer, cube/volume boundary, shader-budget and sampler coverage.
  Implement and verify native indexed instancing in the HAL and test first instance,
  both index widths, slot rules and ordered fallback. The current public replay still
  uses the inherited instancing path.
- Verify dedicated native controls for arbitrary border color, step rates, annotated
  optional runtime capabilities, query results, context loss and resource lifetime.
- Run CTS with an assertion of the observed native tier and audit every skip. Qualify
  presentation and signed AppContainer on Surface ARM64 and Lumia ARM32 when supported,
  plus the pinned Horizon shader, DRS, HUD and mod checks. Surface SSH timed out in this
  session at 192.168.1.227:22. Lumia Device Portal timed out at 192.168.1.157:443.
- Resolve the observed RTX FL9_2 shader-profile negative result. Keep the expected native
  rejection visible alongside the observed driver behavior. Validate on independent
  native-ceiling hardware before generalizing the result.

The requirement ledger includes inherited operations and format/use rows from roadmap
revision 126114a9fa069194e2bd63ac8e8d46c327a35d83, with 10Level9 method-level entries
from the Microsoft references. Unverified rows remain UNKNOWN. A scoped pass in the
ledger applies only to its recorded device, operation and artifact.
