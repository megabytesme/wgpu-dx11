# DX11 higher-tier execution contract

This contract applies to FL9_3, FL10_0, FL10_1, FL11_0 and FL11_1. The tier-specific fixtures prioritize integration risks. Certification covers every source row, every applicable path and the full inherited regression set. The [feature dependency graph](../feature-dependencies.json) assigns modules to isolated tier owners and serializes edits to common interfaces.

## Build the obligation list

Use [native guarantees](../native-guarantees.csv), [DXGI and DDI format/use requirements](../dxgi-format-support.csv), [operation deltas](../feature-level-deltas.csv), [format deltas](../dxgi-format-deltas.csv) and the [conformance recipes](../conformance-plan.json). Read the specific linked source and qualifier while preparing each case. Keep DXGI application and DDI driver obligations under their original IDs. The delta lists prioritize work and the full tables define the current indexed scope. Use the highest applicable source row for the current-tier behavior and run the lower-tier requirements with the original expectations and exact lower-tier selections. A lower tier's unavailable row can coexist with a newly required function at the active tier. Preserve optional and conditional rows, including the query and the conditional execution result, when the option is present.

From the roadmap worktree, run the following with the tier string from the instance spec. Keep the emitted CSVs alongside the run evidence. The source tables have tier identifiers without the `FL` prefix.

```powershell
$dx11Tier = '10_0'
$dx11Tiers = @('9_1','9_2','9_3','10_0','10_1','11_0','11_1')
$dx11TierIndex = [Array]::IndexOf($dx11Tiers, $dx11Tier)
if ($dx11TierIndex -lt 0) { throw "Unknown DX11 tier $dx11Tier" }
$dx11Scope = $dx11Tiers[0..$dx11TierIndex]
$dx11Tables = Join-Path (Get-Location) 'deliverables/dx11-levels'
$dx11PlanDir = Join-Path $env:TEMP ("dx11-$dx11Tier-plan-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $dx11PlanDir | Out-Null
$dx11Manifest = foreach ($dx11Table in @('native-guarantees.csv','dxgi-format-support.csv')) {
    $dx11Source = Join-Path $dx11Tables $dx11Table
    $dx11Rows = @(Import-Csv $dx11Source | Where-Object { $_.tier -in $dx11Scope })
    if (($dx11Rows.id | Sort-Object -Unique).Count -ne $dx11Rows.Count) {
        throw "Duplicate requirement ID in $dx11Table"
    }
    foreach ($dx11Level in $dx11Scope) {
        if ($dx11Level -notin $dx11Rows.tier) { throw "Missing $dx11Level in $dx11Table" }
    }
    $dx11Rows | Export-Csv (Join-Path $dx11PlanDir $dx11Table) -NoTypeInformation -Encoding UTF8
    [pscustomobject]@{
        Table = $dx11Table
        SourceSha256 = (Get-FileHash $dx11Source -Algorithm SHA256).Hash
        TotalRows = $dx11Rows.Count
        ActiveTierRows = @($dx11Rows | Where-Object tier -eq $dx11Tier).Count
    }
}
$dx11Manifest | Export-Csv (Join-Path $dx11PlanDir 'inventory.csv') -NoTypeInformation -Encoding UTF8
$dx11PlanDir
```

Each obligation can require several cases. Give each case a stable suffix identifying the method, stage, resource and view dimension, aspect/mip/array range, binding/use, sample count/quality and test polarity as applicable. Expand broad IDs into method, stage and limit subcases. Include ordinary values, boundary and one beyond, plus independent isolation of combined constraints. Reconcile the application references in the tier research and Functional Specification with the CSVs before review. New or ambiguous methods, API flags, numeric precision/ordering guarantees, cross-stage limits, resource combinations, runtime/platform restrictions and HAL interface methods need source-backed subrows even where the current inventory has no direct ID. Record the unresolved source question, owner and missing proof in the handoff. The current row counts alone leave this method-level reconciliation open.

## Route each operation to its executing layer

The CSVs name owning HAL entry points. In this table DX11 files are relative to `wgpu-hal/src/dx11/`, public files are relative to `wgpu/src/api/`, and Core files are relative to `wgpu-core/src/`. The exact positive and negative operation recipes come from the referenced test family and each source row's native value and qualifiers. A family label is a fixture template. Instantiate it for each obligation and report any missing mapping.

| Operation | Standalone D3D11 control | HAL and public route |
| --- | --- | --- |
| Exact selection and support | Singleton `D3D11CreateDevice`, `GetFeatureLevel`, DXGI adapter identity, `CheckFeatureSupport`; test failed level and separately logged fallback | `instance.rs`, `device.rs`, `adapter.rs`, `capabilities.rs`; request via public `instance.rs` and `adapter.rs`, assert public `Adapter::features`, `limits`, `get_downlevel_capabilities` and `get_texture_format_features` before use and after resetting opt-in |
| Shader, IA, draw and output merger | `Create*Shader`, `CreateInputLayout`, IA and OM setters, native draw; compare pixels and guards, then vary one stage, layout, state or draw limit | `shader.rs`, `hal_shader.rs`, `hal_pipeline.rs`, `hal_index.rs`, `hal_encoder.rs`, `command.rs`; Core `device/resource.rs`, `pipeline.rs`, `command/render.rs` and `command/bundle.rs`; public `device.rs`, `render_pipeline.rs`, `render_pass.rs`, `render_bundle_encoder.rs` |
| Resource, format, view and sampling | `CheckFormatSupport`/`CheckFeatureSupport`, `CreateBuffer`/`CreateTexture*`, `Create*View`, `CreateSamplerState`, readback for the exact operation; reject an incompatible format/dimension/view or sampler separately | `capabilities.rs`, `texture.rs`, `buffer.rs`, `hal_device.rs`, `hal_sampler.rs`, `hal_bind_group.rs`, `hal_shader.rs`; Core `device/resource.rs` and `command/bind.rs`; public `adapter.rs`, `device.rs` and `texture.rs` |
| Transfer, MSAA, and readback | `CheckMultisampleQualityLevels`, `UpdateSubresource`, `CopyResource`/`CopySubresourceRegion`, `ResolveSubresource`, `Map`/`Unmap`; validate invalid flags, bounds, pitched rows, aspects, sample-count and quality mismatches | `hal_transfer.rs`, `hal_queue.rs`, `hal_encoder.rs`, `command.rs`; Core `command/transfer.rs` and `device/queue.rs`; public `command_encoder.rs`, `queue.rs`, and `buffer.rs` |
| Queries and completion | `CreateQuery`, `Begin` where needed, `End`, bounded `GetData` with ready/pending distinction; compare results after submission, invalid sequence and preserved guard bytes | `query.rs`, `fence.rs`, `hal_queue.rs`, `hal_device.rs`, `hal_encoder.rs`; Core `command/query.rs`, `command/render.rs`, `command/timestamp_writes.rs`, `device/life.rs`; public `query_set.rs`, `command_encoder.rs`, `render_pass.rs`, `queue.rs`, `device.rs` |
| Storage, dispatch and indirect | Compile, create, bind and execute the stage and resource combination; prove data/counters and argument bytes; exercise over-limit dimensions, aliasing and illegal binding overlap | `hal_bind_group.rs`, `hal_device.rs`, `hal_uniform.rs`, `hal_encoder.rs`, `command.rs`; Core `device/bgl.rs`, `device/resource.rs`, `command/compute.rs`, `command/render.rs`, `command/draw.rs`; public `device.rs`, `compute_pass.rs`, `render_pass.rs` |
| Presentation and device loss | Swap chain on the relevant real surface, Present/resize/recreate and loss diagnostics; verify recovery and old-resource failure | `surface.rs`, `swapchain.rs`, `instance.rs`, `hal_queue.rs`; Core `device/surface_config.rs` and `device/life.rs`; public `surface.rs`, `surface_texture.rs`, `device.rs` and signed product |
| Native-only requirements | Standalone fixture executes the real API and records behavior, including errors or debug messages for invalid commands | Deliver a guarded DX11 HAL extension invoked through public `Device::as_hal::<wgpu_hal::api::Dx11>()` or a reviewed alternative reachable from the fork. Put execution and validation tests on that consumer path. See the contract below. |

Expand every format/use row independently. Resource dimensions, mips and binding use creation plus a terminal mip/layer/face/slice sentinel. Each sample/load/gather/filter/comparison mode needs a distinguishable shader output using that exact mode. Input assembly and stream-output rows require converted or captured buffer values. Render, depth, blend, logic and MSAA rows need actual GPU writes and observable per-sample or resolve effects. UAV loads, stores and each atomic category need a fixture with expected values and contention where applicable. For typeless/cast rows use exact resource/view format pairs and invalid pairs, including sRGB decoding/encoding where relevant. For transfer/lock rows validate pitch, compressed-block edges and guard bytes. Sharing requires use from the second device or consumer with synchronization. Scan-out, present, video and tiling require the corresponding platform API and an observable result when applicable. A metadata row uses the specified bit-layout/metadata oracle. If the public API exposes a narrower view/copy/binding subset, test that subset and cover the required remainder through the reachable extension.

For each unavailable operation, establish a related legal control and assert the documented rejection or limit. Void context calls need the documented debug diagnostic or observable defined failure behavior. Avoid submitting a known undefined command merely to inspect its pixels. Public rejections should identify validation before submission and preserve device health and unaffected output. A documented portable lowering may succeed on an otherwise unavailable native operation. Prove the lowering's semantics separately.

## Reachable DX11 extension

Use an isolated, default-disabled lane for native-only resource and views, commands, pipeline state/stages and query results. The public HAL guard is a starting point. Register exact entry points and required ownership rules before integration, then exercise them on the same DX11 device as a portable submission. A staged branch may use local fixtures and immutable contracts while the common owner reviews API changes. Complete the independent implementation and compiler controls while predecessor certification is pending.

Own COM objects and reject cross-device handles. Validate requested/returned feature level, runtime interfaces, required option bits, format/view pairs, subresource bounds and incompatible binds. Serialize immediate-context access with queue submission and preserve resource hazards, initialization, COM lifetimes and completed-work ordering. Start with extension-owned resources. Shared imports need an explicit owner, initialization and transition protocol before use. Test portable→extension→portable ordering, repeated context use, resource drop/reset, and error handling with guard data. Standalone Direct3D tests and prototypes cannot close the production extension path.

## Admit evidence and promote

For each case attach the requirement ID and source, test case name, positive and negative expectations, native/HAL/public revision and entry point, executable and package SHA-256, raw command/environment, exact clean source SHA, compiler/profile/flags, adapter/LUID/driver/runtime/OS/arch, requested and returned enum, hardware flag, stdout/diagnostics, timeout state and oracle readback. Include exact build/run directory, commands and result. Keep failed and unrun outcomes explicit. Link artifacts and retain raw bytes, or their hashes plus decoded expected/observed values, for disputed results. Require a paired known-good control on the same adapter/build and one changed factor in each discriminating negative. Native reference, production HAL, public validation, extension and signed product records each answer different questions.

The five higher-tier worktrees were on the research pin when checked against coordination. The roadmap's RTX 3090 Ti x64 and signed Surface ARM64 logs cover singleton native creation, verified raster pixels, and supported native compute writes. Those logs establish only the fixtures they run. There was no higher-tier implementation sample or gate registered for adversarial rerun at the time of review. On receipt, first compare the reported source and binary hashes, then rerun the exact fixture and an independent discriminating control. Compile-only tests, interface acquisition and cap bits are partial results. A GPU run at a newer adapter's forced feature level does not supply legacy/native-ceiling evidence. [Native reference receipts](../hardware-probes.json)

Run elevated and inherited suites at each exact selected/observed tier on the same proposed candidate SHA. Exercise the opt-in disabled branch with public reporting assertions, and reject mandatory skips or a zero-test run. If the physical device tops out below a tier, preserve the maximum-level receipt and gate result for that device and record the higher-tier coverage requirement against suitable hardware. Retain signed architecture-specific execution and real presentation records where required. Mission Control reviews completed predecessor gates before integration promotion, default enablement, or advancing the product pin. Missing mandatory cases, source reconciliation, unexplained CTS exclusions, and unavailable physical evidence keep the tier **INCOMPLETE**.
