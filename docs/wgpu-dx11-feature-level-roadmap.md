# wgpu Direct3D 11 feature-level roadmap

**FL9_1 through FL11_1 remain INCOMPLETE.**

The experimental backend negotiates all seven native levels. The pinned public adapter exposes an FL9_1-class contract, including when D3D11 selects FL11_1. New native probes opened all seven exact levels on an RTX 3090 Ti and in a signed Surface Pro X ARM64 AppContainer. Both GPUs drew and read back 64 expected pixels per tier and executed compute writes at FL10_0, FL10_1, FL11_0 and FL11_1. The ARM32 package is signed and verified. The Lumia Device Portal timed out before deployment. [Device compatibility](wgpu-dx11-device-compatibility.md) · [Hardware receipts](../deliverables/dx11-levels/hardware-probes.json)

## Source baseline

| Component | Revision |
| --- | --- |
| wgpu fork with depthless pipeline fix | `aa7b65c43ef008d0a92d20fcdf9f10fa0883f157` |
| HorizonUWP November 20 product | `5c26a8060c1cf2dce20ee5707e84699ed0879e5c` |
| Horizon upstream Nightly | `0a875da91dbb0fdcb7ba8c42c11341aa229af51a` |
| Fork WebGPU CTS | `3fdd32363410f2bd4e3e2f046980967e6abc27e6` |

The fork defines native device levels in [device.rs](../wgpu-hal/src/dx11/device.rs). [instance.rs](../wgpu-hal/src/dx11/instance.rs) negotiates and can force FL9_1. [capabilities.rs](../wgpu-hal/src/dx11/capabilities.rs) contains native guarantees, queries and the restricted wgpu projection. [adapter.rs](../wgpu-hal/src/dx11/adapter.rs) reports the compatibility path and selected native device level. [hal_shader.rs](../wgpu-hal/src/dx11/hal_shader.rs) selects FL9_1 graphics profiles in the production Naga-to-FXC path. [hal_device.rs](../wgpu-hal/src/dx11/hal_device.rs) has no working compute pipeline and [hal_encoder.rs](../wgpu-hal/src/dx11/hal_encoder.rs) rejects dispatch.

Record the selected native level, public projection, runtime and driver feature queries, output/window model and actual shader profile in each receipt. FL12_x belongs to the Direct3D 12 program.

## Ordered tiers

| Tier | Native boundary in addition to inherited obligations | Next implementation and test |
| --- | --- | --- |
| FL9_1 | Level-9_1 VS/PS, 16-bit indexes, one color target, 2048 2D, 512 cube, 256 volume and conditional NPOT | Finish format and native API inventory, exact hardware and historical driver checks, and signed CoreWindow evidence. [FL9 source review](../deliverables/dx11-levels/research-fl9.md) |
| FL9_2 | Occlusion, separate alpha, two-sided stencil, native 32-bit IA, 1,048,575 index and primitive limits, 16× anisotropy, 2,048 repeat and required DDI additions | Reuse the baseline shader profile. Implement query, sampler and native IA behavior. Route Mirror Once through a native interface. [FL9_2 instance](../deliverables/dx11-levels/instances/fl9_2.md) |
| FL9_3 | Level-9_3 VS/PS, restricted indexed instancing, MRT4 with compatible bit depth and independent write masks, 4,096 2D/cube | Add a distinct shader profile and multi-target path. Resolve volume and format discrepancies on exact hardware. [FL9_3 instance](../deliverables/dx11-levels/instances/fl9_3.md) |
| FL10_0 | SM4, geometry and stream output, eight targets, general NPOT/arrays, BC4/BC5 and driver-gated CS4 | Add reachable shader/resource/render operations and a native geometry path. Validate the smaller CS4 contract before exposing compute. [FL10_0 instance](../deliverables/dx11-levels/instances/fl10_0.md) |
| FL10_1 | SM4.1, 32 IA slots, cube arrays, independent blend and per-format 4× MSAA/sample-frequency guarantees | Change the shared HAL 16-slot capacity through the spine. Verify actual per-format sample, load, resolve, gather and filter behavior. [FL10_1 instance](../deliverables/dx11-levels/instances/fl10_1.md) |
| FL11_0 | SM5, hull/domain and patch tessellation, mandatory compute/UAV, indirect execution and BC6H/BC7 | Implement replay, storage hazards, bounds and indirect args. Tessellation needs reachable native support and executed output. [FL11_0 instance](../deliverables/dx11-levels/instances/fl11_0.md) |
| FL11_1 | Explicit 11.1 runtime, CS 64 UAVs and separate shared graphics OM 64 slots, logic ops and forced sampling | Account for RTV/UAV slot overlap and runtime/interface requirements. Probe optional SAD4, extended doubles and later runtime features separately. [FL11_1 instance](../deliverables/dx11-levels/instances/fl11_1.md) |

[Microsoft feature levels](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro) · [10Level9 API reference](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9) · [D3D11 functional specification](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)

## Requirements and verification

- [Native requirements](../deliverables/dx11-levels/native-guarantees.csv) use stable `FL<tier>.OP.<requirement>` IDs, classifications, numeric values and qualifiers, owners, public mappings, test families and current results. [Tier deltas](../deliverables/dx11-levels/feature-level-deltas.csv) compare predecessor values. [HAL capability matrix](../deliverables/dx11-levels/hal-capability-matrix.json) carries the same operation IDs and links to the format inventory.
- [Format/use inventory](../deliverables/dx11-levels/dxgi-format-support.csv) contains all cells from the six published application tables, plus 10Level9 DDI minima by tier. Keep application `DXGI` and driver `DDI` requirements distinct. FL9_2 uses the driver minima and 10Level9 method rules. Microsoft publishes no standalone FL9_2 application table in this series. [Format deltas](../deliverables/dx11-levels/dxgi-format-deltas.csv) compare adjacent tables with matching source scope.
- The source packets for [FL9](../deliverables/dx11-levels/research-fl9.md), [FL10](../deliverables/dx11-levels/research-fl10.md) and [FL11](../deliverables/dx11-levels/research-fl11.md) give format caveats, precise stage restrictions, source disagreements and finer test oracles. The [native API gap analysis](wgpu-dx11-native-api-gap-analysis.md) identifies mandatory behavior that needs a reachable native-facing interface. Geometry, stream output, tessellation, native context state, logic operations and several format uses have no exact portable path.
- Expand the [conformance plan](../deliverables/dx11-levels/conformance-plan.json) into per-row fixtures. Each fixture needs a positive result at the specified stage and format, a defined negative or boundary oracle, and provenance for actual native, HAL, public or signed product execution. Physical modern forced-tier execution, native-ceiling execution and runtime-conditional hardware each have their own evidence requirements.
- The [implementation plan](../deliverables/dx11-levels/implementation-plan.json), [dependency graph](wgpu-dx11-tier-dependencies.md) and [file ownership](../deliverables/dx11-levels/agent-ownership.md) specify the proposed integration spine and one path owner at a time. Retain every preceding gate and regression check. A missing mandatory operation, unverified advertised path or unrun required hardware test keeps a tier incomplete.

The format tables are enumerated cell by cell. The operation ledger still needs an independent method-level and shader-accuracy audit against the full functional specification before it can define the exhaustive certification denominator. In particular, preserve the source differences on 10Level9 volume and format uses and the per-format/runtime qualifiers for FL10.1 MSAA and FL11.1 logic and sample support. [Required DDI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats) · [DXGI application tables](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/dx-graphics-dxgi-overviews)

The fork journal records 17,962 passes, 4,720 skips and zero failures across 1,686 curated CTS summaries. Capture full case outcomes for each exact candidate, including the disposition of applicable skips. Extend the strict runner beyond its current FL9_1 assertion before counting higher-tier CTS receipts. [Fork CTS journal](../wgpu-hal/src/dx11/FL9_1_CTS.md) · [Runner wiring](../xtask/src/cts.rs) · [WebGPU specification](https://www.w3.org/TR/webgpu/)

## HorizonUWP activation criteria

| Product path | Integration gate |
| --- | --- |
| Shader API 0.2 and packs in `shaders/` | Exercise Shader API WGSL passes and Iris-compatible GLSL packs separately. Verify entry points, formats, bindings, pass graph, expected pixels and fallback. |
| GPU culling | Distinguish CPU visibility, occlusion and GPU culling. The count-driven path requires compute, storage capacity and `MULTI_DRAW_INDIRECT_COUNT`. An ordinary D3D11 indirect draw supplies a different execution contract. |
| HDR and surfaces | Verify actual formats, color-space negotiation and physical output for SwapChainPanel, HWND and CoreWindow as applicable. Record SDR fallback, mode changes and display behavior. |
| DRS and timing | Before timing-driven DRS, verify timestamp ordering, disjoint and conversion. Test the existing CPU/frame-pressure fallback and full-resolution HUD independently. |

Benchmark the same pinned scene, camera, settings, resolution, VSync, pack and thermal state. Compare exact FL9_1 first with FL9_1 compatibility on a higher native level, then with the genuine higher HAL/public path when available. The new native smoke supplies no Horizon performance comparison. [Horizon and presentation research](../deliverables/dx11-levels/research-integration.md)

For native reference controls, exact-candidate reruns, CTS triage and the per-feature graph, see [Implementation support](wgpu-dx11-implementation-support.md).
