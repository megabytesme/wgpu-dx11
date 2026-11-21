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

## Native indexed instancing and resource limits

Exact opt-in FL9_3 can call DrawIndexedInstanced for complete, in-bounds indexed triangle
lists with at least two instances. The shader must consume an instance-rate attribute
and native slot zero must hold vertex-rate data. A second input layout gives the native
instance streams a step rate of one. Replay switches back to the emulated layout after
each native draw. Shaders using emulated vertex or instance indices, viewport remapping,
strip restart, and draws outside the native limits retain ordered replay.
[10Level9 DrawIndexedInstanced restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context#id3d11devicecontextdrawindexedinstanced)

The exact FL9_3 device probes creation of typed 3D textures and shader resource views
on each axis for the public volume formats. It bounds the advertised limit and the HAL
creation path by the observed common extent. A separate raw D3D11 probe on the
RTX 3090 Ti created RGBA8 and RG32F textures and views at 256 on each axis. At 512,
CreateTexture3D returned E_INVALIDARG on all three axes for both formats. The method
table specifies 512, while the feature-level overview lists 256. The 512 result remains
a scoped failure in the ledger.
[10Level9 CreateTexture3D method](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device#id3d11devicecreatetexture3d) � [Feature-level overview](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro#direct3d-11-feature-support-feature-levels-12_1-through-9_1)

The x64 and x86 readbacks cover both index widths, nonzero first instance, the raw
index ceiling of 1,048,575, and fallback replay. The FL9_1 and FL9_2 controls execute
the same fixtures with zero native instanced draws. Both architectures passed public
half-float vertex input, native mip generation and RGBA16F blending. The native BC1
cube fixture checks all six 4096-edge faces at the base and final mip, and rejects
a rectangular cube, a five-face cube and a 4097 edge.

The native RG32F test covers a 2D render target, a cube face, and volume and shader
resource view creation on all three axes at 256. Separate RG32F volume transfer
sequences reported DXGI_ERROR_DEVICE_REMOVED on both architectures. One sequence
used a region update and copy into a 1x1x1 staging texture. The other initialized
the volume at creation and used a whole-resource copy into a staging texture of
the same extent. The receipts preserve both failures.
[Instancing and resource receipts](../deliverables/dx11-levels/FL9_3/instancing-resources.json)

## Remaining certification work

- Integrate onto the reviewed FL9_2 revision after FL9_1 and FL9_2 are certified.
  Replay all inherited cases on that cumulative source revision.
- Expand format and transfer coverage, including shader sampling of sRGB BGRX8 and
  the remaining RGBA16F, RG32F, RGBA32F and BGRA requirements. Check
  shader budgets and sampler coverage. Exercise non-unit native instance step rates
  and the remaining method restrictions as dedicated cases.
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
