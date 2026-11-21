# wgpu D3D11 development and conformance

## Scope and compatibility

Build the backend with the `dx11` Cargo feature. The protected starting point is
`aa7b65c43ef008d0a92d20fcdf9f10fa0883f157`. It exposes a restricted FL9_1-class
wgpu profile even when the D3D11 device can run a higher native feature level.
Use `wgpu-info` for the adapter's actual features, downlevel flags, limits and format capabilities.
No feature level is certified yet.

| Target | Native controls at project start | wgpu qualification still required |
| --- | --- | --- |
| Windows x64 and x86 | RTX 3090 Ti exact-level offscreen draw, formats, eligible compute and stage checks | Exact integrated candidate, signed lifecycle where applicable, inherited regression and CTS |
| Windows ARM64 | Surface Pro X Adreno 680 offscreen FL9_2 comparisons | Signed AppContainer activation, exact HAL/public path and recovery |
| Windows 10 Mobile ARM32 | Signed Lumia Adreno 430 exact native FL9_1 through FL11_1 | Split resolve rerun, HAL/public build and readback, CoreWindow presentation and recovery |
| Hardware capped at a target level | RTX and Adreno emulate selected requests within a higher native ceiling | Validate each level on actual ceiling hardware where available and track coverage gaps |

## Build and test

Use the pinned Rust toolchain and MSVC for desktop work. On Ben-Desktop use an
isolated `CARGO_TARGET_DIR` with sufficient space. Record the full SHA, dirty
state, command, target, toolchain and executable SHA-256 for each candidate.
The ARM32 Mobile HAL needs a toolchain with a compatible target, SDK and dependencies.
The standalone ARM32 native probe uses MSVC and its legacy Mobile link SDK.

```powershell
$env:WGPU_BACKEND = 'dx11'
$env:WGPU_DX11_FORCE_FL9_1 = '1'
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo run --locked -p wgpu-info --features dx11
cargo test --locked -p wgpu --no-default-features --features std,dx11,wgsl --test dx11_public -- --ignored --nocapture --test-threads=1
```

Inspect test prerequisites before running cases that create windows. Use
`--target i686-pc-windows-msvc` before the test executable separator for x86.
For tier branches, use that branch's exact selection entry point and record both
native feature-level values and the public projection. A requested enum by itself
is insufficient. [Native desktop and AppContainer probe](../experiments/dx11-native-probe/README.md)

## CTS

The repository's [CTS runner](../cts_runner/README.md) uses the revision in
`cts_runner/revision.txt`. With the same strict environment as above, run a
reviewed list or a targeted selector. Hold the shared desktop GPU lock for runtime.

```powershell
cargo xtask cts --backend dx11 --require-dx11-hardware 'webgpu:api,operation,command_buffer,basic:*'
cargo xtask cts --backend dx11 --require-dx11-hardware -f <reviewed-test-list>
```

Keep logs with individual results and skip reasons. Record the candidate, CTS
pin, test-list and runner hashes, selected device and exact feature level.
For a reproducible FL9_1 copy-layout batch, run
`python cts_runner/capture-dx11-fl91.py --help` and use
`cts_runner/dx11-fl91-copy-layout.lst`. It checks the pinned CTS revision,
runner digest and strict physical adapter marker. The batch keeps complete
selector logs and requires a pass without skips or warnings. Supply the
lab's canonical shared GPU lock file. Build the runner with
`cargo build --locked -p cts_runner --features deno_webgpu/dx11`.

The default list contains `fails-if(dx11)` exclusions. Audit excluded selectors
and runtime skips by case. A skipped optional operation does not settle mandatory
negative validation. Browser and canvas behavior requires a browser fixture.

## Branch and release process

Develop independent tier candidates in parallel. The promotion order is FL9_1,
FL9_2, FL9_3, FL10_0, FL10_1, FL11_0, FL11_1. Every candidate inherits the exact
accepted lower-tier changes. Review conflicts in adapter selection, capability
projection, validation, shaders, pipelines, resources and queries before
combining patches. Freeze and exercise an exact commit before considering
exposure or downstream adoption. Maintain previous reviewed SHA references.
Tier status is IMPLEMENTING, READY FOR INTEGRATION, INTEGRATED / UNCERTIFIED,
CERTIFIED, or BLOCKED.

A gate requires complete mandatory applicability and independent verification
for every requirement on the exact candidate. Record native, HAL, public API,
inherited and lifecycle evidence separately. Preserve mandatory native-only
operations as gaps until a safe reusable D3D11-specific interface is implemented
and tested. Capture optional hardware capabilities separately. Review native
guarantees, GPU readbacks, advertised limits and features, CTS details, and prior
backend regressions before publishing a release or changing default exposure.
[Direct3D functional specification](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)

For a downstream handoff, record the full candidate SHA and branch, supported
level and advertised API scope, per-tier certification status, dependencies and
API changes, compatibility risks, CTS and hardware receipts, and adoption tests.
The downstream application controls its own pin.
