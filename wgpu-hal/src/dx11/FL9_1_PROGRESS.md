# FL9_1 progress

## Strict CTS routing and defaults — 2026-11-10

The xtask runner now has an explicit `--require-dx11-hardware` mode. It
requires `--backend dx11`, forwards the strict hardware variable to both CTS
runner paths, and also preserves an explicitly inherited strict variable.
Within this test mode only, Deno defaults omitted required limits to the
selected adapter's reported limits; explicit limit requests remain unchanged.
Normal Deno defaults and backend enumeration are unchanged.

The focused `basic:empty:*` selector passed 1/1 without skips and logged the
physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and
actual FL9_1. It validates runner/device setup but performs no render. The
broader `basic:*` run selected the same device and produced 1 pass, 2 failures,
0 skips. The failures request `Rgba8Uint` `COPY_SRC | COPY_DST`, which is
outside the current downlevel format contract, and then observe zero readback;
they are honest unsupported-path failures. They are not fixed by claiming
broader format support.

```text
cargo +1.93.1 test --offline -j4 -p deno_webgpu --features dx11 --lib required_limit_tests -- --nocapture
# 4 passed.

cargo +1.93.1 test --offline -j4 -p wgpu-xtask --bin wgpu-xtask cts::backend_feature_tests::strict_dx11_hardware_mode -- --nocapture
# 2 passed.

cargo +1.93.1 xtask cts --backend dx11 --require-dx11-hardware "webgpu:api,operation,command_buffer,basic:empty:*"
# 1 passed, 0 failed, 0 skipped; strict physical FL9_1 identity logged.

cargo +1.93.1 xtask cts --backend dx11 --require-dx11-hardware "webgpu:api,operation,command_buffer,basic:*"
# 1 passed, 2 failed, 0 skipped; failures are unsupported Rgba8Uint copy usages.

cargo +1.93.1 clippy --offline -j2 -p deno_webgpu --features dx11 --lib --tests
cargo +1.93.1 clippy --offline -j2 -p wgpu-xtask --bin wgpu-xtask
cargo +1.93.1 clippy --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --example dx11_fl91_offscreen --example dx11_fl91_windowed
# passed; the wgpu example build emitted five pre-existing D3D11 HAL warnings.

rustfmt +1.93.1 --edition 2021 --check deno_webgpu/adapter.rs xtask/src/cts.rs wgpu/examples/dx11_fl91_offscreen.rs wgpu/examples/dx11_fl91_windowed.rs
git diff --check
# passed (Git reports existing LF-to-CRLF working-copy notices).
```

The focused CTS selector is not a rendering test. The existing strict public
wgpu suite remains the evidence for actual rendering. The next work remains a
Core/HAL operation audit and targeted shader/resource additions only where the
FL9_1 contract allows them. Historical hardware and Lumia runtime remain
external verification gaps.

An initial Deno Clippy attempt at `-j12` exhausted the Windows paging file
(OS error 1455) while rebuilding proc-macro dependencies; rerunning at `-j2`
completed successfully. This was a resource failure, not a Rust or Clippy
diagnostic.

## Public example capability diagnostics — 2026-11-10

The offscreen and visible HWND examples now print selected-adapter wgpu
features and limits. They still request only `Features::empty()` and use the
reported limits, so this adds diagnostics without opting into optional device
features. Both examples were rebuilt and executed with strict hardware mode.
The RTX 3090 Ti reported 2D/3D limits of 2048/256, six array layers, eight bind
groups, 8 sampled textures and 16 samplers per shader stage, six uniform
buffers per shader stage, one color attachment and 22 combined shader buffers
per stage. Its reported features include the query-gated 16-bit formats,
BC1–BC3 subsets, timestamp queries, comparison border addressing and line
polygon mode.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93.1 run --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --example dx11_fl91_offscreen
# Physical RTX 3090 Ti, requested/actual FL9_1; all 1,024 pixels validated:
# 112 red, 277 green, 635 blue background.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93.1 run --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --example dx11_fl91_windowed -- --frames=60
# Physical RTX 3090 Ti, requested/actual FL9_1; 60 textured indexed
# depth-tested frames presented through HWND.
```

These runs verify this modern NVIDIA driver forced to FL9_1, not historical
FL9_1 devices. The offscreen run is the pixel-validation proof; the windowed
run verifies bounded presentation but does not read back the surface.

## Direct 2D `textureLoad` plus `textureDimensions` — 2026-11-10

The private/public FL9_1 path now accepts a deliberately narrow composition of
the existing direct fragment texture-load and dimensions emulations: one
single-sample group-0 `texture_2d<f32>`, exactly one in-bounds `textureLoad`
with constant `vec2i` coordinates and one level-less `textureDimensions`
query, directly converted from `vec2u` to `vec2f`. Both existing downlevel
limits must be enabled. Core, Naga and HAL reject mismatched images, extra
image operations/resources, helpers, control flow, nonconstant integer work
and other proof combinations. The texture itself must satisfy the existing
single-mip RGBA8 load path; host dimensions and point-sampling constants share
the existing `b13`/`s15` emulation path. General `texture_query` remains
disabled, and this composition does not change the reported capability model.

The focused Naga, Core, HAL proof and FXC tests passed. Strict public hardware
execution selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and confirmed FL9_1, and matched
`[100, 60, 80, 255]` for all 64 pixels in an 8x8 RGBA8 target.

```text
cargo +1.93.1 test --offline -j2 -p naga --features hlsl-out --lib fl9_combined_2d_texel_load_and_dimensions_share_proven_binding -- --nocapture
# 1 passed.

cargo +1.93.1 test --offline -j2 -p wgpu-core --no-default-features --features dx11,std,wgsl restricted_fl9_texture_load_and_dimensions_share_one_texture -- --nocapture
# 1 passed; the test distinguishes both-enabled, load-only, dimensions-only and mismatched-image cases.

cargo +1.93.1 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib texture_load_and_dimensions -- --nocapture
# 2 passed, including FXC compilation at ps_4_0_level_9_1.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_texture_load_and_dimensions_share_one_fl91_texture -- --exact --nocapture --test-threads=1
# 1 real GPU test passed; 64/64 pixels matched.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
# 117 passed, 0 failed; includes default adapter-enumeration isolation.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
# 467 passed, 0 failed.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93.1 test --offline -j2 -p wgpu-core --no-default-features --features dx11,std,wgsl --lib -- --test-threads=1
# 117 passed, 0 failed.

cargo +1.93.1 test --offline -j2 -p naga --features hlsl-out --lib
# 157 passed, 0 failed.

cargo +1.93.1 test --offline -j2 -p wgpu-core --no-default-features --features dx12,std,wgsl --lib -- --test-threads=1
# 116 passed, 0 failed.
```

The full public suite contains one optional RG8 mip subcase skipped because
the selected driver does not report that optional support; the combined-load
hardware test itself ran. Clippy and formatting passed after fixing a new
Core `collapsible_if` warning. Clippy still reports existing D3D11 HAL
warnings, and the DX12-only Core test build reports existing unused-variable
warnings in FL9 helper analysis.

Checkpoint: `8164843a9` (`dx11: combine FL9_1 texture load and dimensions`).

## Selected-entry texture-sample-bias sampler validation — 2026-11-10

The public FL9_1 adapter previously let `textureSampleBias` reach HAL with
mixed min/mag filters or anisotropy 2; HAL rejected those combinations during
command preparation. Core now carries Naga's selected-fragment-entry bias use
and sampled-pair metadata into render-pipeline state, retains each sampler's
compatibility after effective downlevel anisotropy adjustment, and rejects
incompatible used pairs at direct draw and render-bundle readiness. The new
`DownlevelLimits` field defaults false and is set only by DX11, leaving other
backends unchanged. A strict public negative test rejected mixed filters and
anisotropy 2 before HAL; the existing positive biased-mip test still matched
all 384 pixels at requested/actual FL9_1 on the RTX 3090 Ti. The full strict
public suite passed 115/115, the Core suite passed 125/125, and the strict
private HAL suite passed 464/464. HAL and Core Clippy completed with existing
warnings; new public test Clippy was clean. Render-bundle validation is wired
to the same check, but the Rust `RenderBundleEncoder::finish` wrapper uses a
fatal error path, so the public integration test covers direct draws only.

The verified change is ready for a focused checkpoint. It does not alter
normal adapter enumeration, selected shader support, or historical-device
verification.

## BC partial-initialization and matrix determinant execution — 2026-11-09

Two public strict-hardware regression targets now verify additional existing
paths without changing the advertised capability projection. The BC test
uploads one compressed block to each 8x4 BC1/BC2/BC3 texture, samples the
uploaded and untouched regions, and matched all 96 pixels including
format-correct zero initialization. The determinant test executes WGSL
`determinant` for mat2, mat3 and mat4 in both vertex and fragment stages; each
case changes the uniform matrix between draws and validates two complete 8x8
readbacks. All seven tests passed on the physical RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) at requested/actual FL9_1, software=false. The tests
confirm this device/driver/compiler combination only; historical FL9_1 remains
unverified. New-test Clippy is clean; five pre-existing D3D11 HAL Clippy
warnings remain.

Commands and result are recorded in `TESTING.md`.

## Strict CTS default-limit boundary and combined-limit correction — 2026-11-09

The pinned CTS checkout was made available and a strict DX11 run of
`webgpu:api,operation,command_buffer,basic:*` executed 3 cases: 0 passed, 3
failed, 0 skipped. The strict Deno preflight selected the opt-in DX11 route,
but each case failed before GPU operations because the CTS/Deno device request
uses Core WebGPU defaults. It requested 28 combined buffers per shader stage;
the adapter's prior projection was 17. This adapter intentionally does not
meet Core defaults, and other default limits also exceed its projection, so
this is an expected adapter-contract mismatch rather than a rendering failure.
Do not change Deno defaults or claim standard CTS success for this adapter.

The audit found that 17 was stale: this projection allows six uniform buffers
in the vertex stage and sixteen vertex-input buffers, and Core checks their
sum against `max_buffers_and_acceleration_structures_per_shader_stage`. The
projection now reports 22, and the synthetic projection test asserts that
value. This is a Core validation limit, not a native D3D11 register limit.

Validation:

```text
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_wgpu_projection_is_conservative_and_uses_only_implemented_paths -- --nocapture
# 1 passed; four pre-existing unnecessary-qualification warnings in hal_queue.rs.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
# 464 passed, 0 failed; strict hardware mode; includes CPU and GPU tests.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
# 115 passed, 0 failed; includes ordinary-enumeration isolation and strict FL9_1 GPU paths.

cargo +1.93 clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
# passed with 18 existing warnings; no warning in the changed capability projection.

rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs
git diff --check
# both passed.

$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:DENO_WEBGPU_BACKEND = 'dx11'
cargo +1.93 xtask cts --backend dx11 'webgpu:api,operation,command_buffer,basic:*'
# 0 passed, 0 skipped, 3 failed; request of 28 exceeded the corrected limit 22 before any GPU operation.
```

Deno's default request is correct for Core WebGPU semantics, so satisfying it
requires either a broader Core-level adapter contract or an explicit
compatibility/downlevel API path. Keep the standard CTS failure visible and use
strict native wgpu tests with explicitly requested supported limits for GPU
execution evidence.

Checkpoint: `bb34517a9ddb8a82c883f4a64c9a3154169e61f8` (`dx11: correct FL9_1 combined buffer limit`).

## Strict CTS hardware preflight and blend-mask boundary — 2026-11-09

Deno now has an explicit strict hardware mode for opt-in DX11 CTS runs. The
`WGPU_DX11_REQUIRE_HARDWARE=1` setting requires `DENO_WEBGPU_BACKEND` to select
only DX11; absent, mixed or mismatched backend masks reject the adapter
request. Adapter request failures also reject rather than returning `None`,
and the selected adapter must report `Backend::Dx11`, not be CPU/software, and
carry the HAL's `FL9_1; physical DXGI adapter;` marker. Normal requestAdapter
semantics are unchanged when strict mode is unset. The marker is a verified
driver-info string, not a structured feature-level field.

Rust 1.93.1 validation passed: the Deno backend-selection tests passed 6/6,
the `cts_runner` DX11 feature check passed, xtask routing passed 1/1, and
format/diff checks passed. The build reported three existing unused-variable
warnings in `wgpu-core/src/validation.rs`. No actual CTS tests ran: this
checkout lacks the CTS tree and helper. Strict CTS classification of a rejected
adapter request remains unverified until a selector is run against the CTS
runner.

An independent blend audit found no uncovered full-channel separate-alpha
descriptor: Core requires the absent `SEPARATE_ALPHA_BLEND` flag, and HAL
independently rejects it. RGB-only and alpha-only write masks are accepted
because the other equation is unobservable; HAL normalizes the unused native
equation. A strict Core fixture now covers alpha-only acceptance, passing 1/1
on the RTX 3090 Ti at requested/actual FL9_1.

Related commits: `ee480928e` (strict DX11 Deno adapter preflight) and
`d4227636e` (alpha-only blend validation coverage).

## Pipeline failure classification and opt-in CTS routing — 2026-11-09

Pipeline native-object creation now reports known `E_OUTOFMEMORY` as
`PipelineError::Device(OutOfMemory)` and DXGI device hung/removed/reset/driver
internal errors as `PipelineError::Device(Lost)`. `E_INVALIDARG` continues to
produce a linkage error. A production-mapper unit test covers these HRESULTs.
The focused HAL error tests passed 3/3. Existing `hal_queue.rs`
`unused_qualifications` warnings remain.

The Deno/xtask path now supports explicit experimental DX11 CTS selection:
`cargo xtask cts --backend dx11` enables `deno_webgpu/dx11`, which compiles
`wgpu-core/dx11`; the `d3d11` alias is accepted too. If
`DENO_WEBGPU_BACKEND` is unset, Deno uses `Backends::default()`, which excludes
DX11. Backend-mask tests passed 2/2, xtask feature-routing passed 1/1, and
`cts_runner` checked successfully with the feature enabled. Deno compilation
reported three existing Core unused-variable warnings. At this checkpoint CTS
execution was not attempted because this checkout had no local CTS checkout or
runner helper; the strict adapter preflight was added in the follow-up entry
above.

Commits: `f87abf0d3` (`dx11: classify pipeline device creation failures`),
`d2994cebd` (`xtask: enable opt-in D3D11 CTS routing`). Neither change alters
the D3D11 capability projection or normal adapter enumeration.

## Multi-mip RGBA8_SNORM partial copy — 2026-11-09

Added a strict public test combining the existing RGBA8_SNORM multi-mip and
GPU-copy paths. It uploads distinct signed patterns to four mip levels of
source and destination textures, copies a 2x2 region between nonzero origins
in mip 1, then samples a suffix view into an RGBA8 render target. All 16
sampled output pixels distinguish the copied region from untouched destination
texels. The strict capability report showed `COPY_SRC | COPY_DST |
TEXTURE_BINDING`, `FILTERABLE` and no `SINGLE_MIP_ONLY`; the public
texture-to-buffer-copy mask remained clear for RGBA8_SNORM. The only CPU
readback is from the RGBA8 output target.

The NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false) was selected with requested and actual FL9_1.
The new test did not skip. The full file passed both public GPU tests.

Verification used Rust 1.93.1, Visual Studio 2026 x64, offline Cargo and
bounded parallelism:

```powershell
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_snorm_copy -- --nocapture --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_snorm_copy'
cargo +1.93 fmt --all -- --check
git diff --check
```

Results: strict public SNORM target 2/2; formatting and diff checks passed;
Clippy passed with five existing HAL warnings (argument counts and one boolean
simplification), with no new test warning. No capability or backend code
changed. The test confirms a copied mip-1 subresource through sampled output;
it does not enable SNORM CPU readback or render attachment usage.

Checkpoint: `effae2add` (`dx11: verify multi-mip FL9_1 SNORM copies`).

## Combined narrow and RGBA/BGRA texture-usage execution — 2026-11-09

The strict public usage-operation test now covers the complete combined
`COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT` union on
R8Unorm, Rg8Unorm, Rgb5g6b5Unorm, Rgba8Unorm, Bgra8Unorm and the two sRGB
variants. Each path performs a padded queue upload, same-format GPU texture
copy, padded CPU copy-readback, sampled render into a same-format attachment,
and attachment readback. All 64 copied texels matched exactly; all 64 sampled
output pixels matched within one byte. The narrow formats exercise one-,
two-, and packed two-byte texels without requiring a filtering sampler. Both
readback images were prefilled with a sentinel; every row-padding byte remained
untouched. This is operation evidence only; no capabilities changed.

The NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false) reported the full usage union for all seven
formats, including `TEXTURE_FORMAT_16BIT_PACKED` for RGB565. The test requested
and selected FL9_1; no format skipped.

Verification used Rust 1.93.1, Visual Studio 2026 x64, offline Cargo and
bounded parallelism:

```powershell
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_executes_combined_texture_format_usages_at_fl91 -- --nocapture --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public'
cargo +1.93 fmt --all -- --check
git diff --check
```

Results: focused strict test 1/1; full strict public D3D11 target 117/117;
formatting and diff checks passed. Targeted Clippy passed with five existing
HAL warnings (argument counts and one boolean simplification); the test
introduced no warning.

The test proves this usage union for seven formats, not the full 21-format
operation matrix or every allowed usage combination. Several format paths are
still tested only at creation time. Extend operation coverage by format and
operation class, respecting each HAL path and selected-device query.

Checkpoint: `48cd0b95d` (`dx11: verify FL9_1 texture readback row padding`).

## Combined linear/sRGB RGBA8/BGRA8 texture-usage execution — 2026-11-09

Extended the strict public GPU test for the advertised combined usage set
`COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT` to Rgba8Unorm,
Bgra8Unorm and their sRGB variants. The test requires `FILTERABLE` as well as
the usage union. It uploads a patterned 8x8 source, copies it to a second
all-usage texture, reads that copy to a padded CPU buffer, samples the copy
into an all-usage same-format color attachment, then reads the attachment
back. It checked all 64 copied texels exactly and all 64 sampled output pixels
within one byte for each format, including sRGB decode/encode and BGRA byte
ordering.

Strict hardware was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), with requested and actual FL9_1. All four
formats reported the full usage set and filterability; no skip occurred.

Verification used Rust 1.93.1, Visual Studio 2026 x64, offline Cargo and
bounded parallelism:

```powershell
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_executes_combined_rgba8_bgra8_usages_at_fl91 -- --nocapture --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public'
cargo +1.93 fmt --all -- --check
git diff --check
```

Results: focused strict test 1/1; full strict public D3D11 target 117/117;
formatting and diff checks passed. Targeted Clippy passed with five existing
HAL warnings (argument counts and one boolean simplification); the test
introduced no warning.

This proves one combined usage union on four related formats, not the other
projected formats or all allowed usage combinations. The selected-device
table still checks many combinations only at creation time. Extend operation
coverage by format and operation class, respecting each HAL path and query
gate.

Checkpoint: `1967b0722` (`dx11: verify combined sRGB texture usage at FL9_1`).

## Combined RGBA8/BGRA8 texture-usage execution — 2026-11-09

Added a strict public GPU test for the advertised combined usage set
`COPY_SRC | COPY_DST | TEXTURE_BINDING | RENDER_ATTACHMENT` on Rgba8Unorm and
Bgra8Unorm. The test requires `FILTERABLE` as well as the usage union. It
uploads a patterned 8x8 source, copies it to a second all-usage texture, reads
that copy to a padded CPU buffer, samples the copy into an all-usage
same-format color attachment, then reads the attachment back. It checked all
64 copied texels and all 64 sampled output pixels for each format.

Strict hardware was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), with requested and actual FL9_1. Both formats
reported the full usage set and filterability; no skip occurred.

Verification used Rust 1.93.1, Visual Studio 2026 x64, offline Cargo and
bounded parallelism:

```powershell
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_executes_combined_rgba8_bgra8_usages_at_fl91 -- --nocapture --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --test-threads=1'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public'
cargo +1.93 fmt --all -- --check
git diff --check
```

Results: focused hardware test 1/1; full strict public target 117/117; both
formats had no skipped subcases. Formatting, Clippy and diff checks passed.
Clippy continues to report five pre-existing HAL warnings (argument count and
one boolean simplification); this test introduced none. The first compile
attempt caught a `u64::from` call in a const expression unsupported by the
pinned compiler; it was replaced by equivalent casts before the passing run.

The test proves one common combined usage union on two formats; it does not
close the full format operation matrix. The selected-device table still checks
many usage combinations only at creation time. Extend coverage by operation
class for other formats, respecting each backend path and query gate.

Checkpoint: `989ce81f2` (`dx11: execute combined FL9_1 texture usages`).

## Optional 2x/8x MSAA depth and stencil readback — 2026-11-09

Extended the public optional FL9_1 resolve test to run real depth ordering at
every selected-device-supported depth format for 2x and 8x RGBA8 MSAA. On the
RTX 3090 Ti all six pairs (2x/8x × Depth24PlusStencil8, Depth24Plus and
Depth16Unorm) were supported and executed. Each case rendered a near triangle
followed by a farther triangle with a distinct color, resolved and checked all
64 pixels; the near color survived, proving the later depth test rejected the
far draw. D24S8 also verified clear-to-zero, `Equal` with reference 0,
`Replace` with reference 1, successful `Equal` at reference 1, and rejection
at reference 0 after replacement. The test reports a color-only result rather
than claiming depth coverage if no query-positive depth format is available.

The strict public test selected NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
FL9_1 and asserted actual FL9_1. Queried RGBA8 sample counts were `[2, 8]`;
no depth/stencil subcases skipped. Focused run: 1 Rust test passed, 0 failed,
with six depth cases and two additional D24S8 stencil readbacks.

Verification:

- Focused strict run (from the Visual Studio x64 developer environment):

  ```powershell
  cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_optional_fl91_sample_counts -- --nocapture --test-threads=1'
  ```

- The full strict public target passed 116/116:

  ```powershell
  cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --test-threads=1'
  ```

- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and
  `git diff --check` passed.
- Targeted `cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public` passed. Existing HAL warnings remain for argument counts and one boolean expression; no new warning was reported for this test.

The independent Core/HAL projection audit confirmed that the 21-format table
currently verifies 189 advertised usage combinations, 136 rejected
combinations, 21 mip rules and 40 sample-count rules through creation only.
It did not find a confirmed execution mismatch in the inspected transfer,
attachment or resolve paths. The next task is a compact public operation matrix
for a few combined usages, beginning with RGBA8/BGRA8, so advertised
capabilities are exercised rather than only instantiated.

The panic reachability review found no valid public path to the current
compute, pipeline-cache, ray-tracing or acceleration-structure panic stubs;
Core/capability gates prevent successful object creation. This was a static
review, not a malformed unsafe-HAL audit. The shader review found no obvious
low-risk adjacent feature to expose: explicit LOD, derivatives, gather,
front-facing and fragment position remain profile restrictions. Avoid
expanding shader coverage before closing operation-proof gaps.

Checkpoint: `fa15bc6f8` (`dx11: verify FL9_1 multisample depth and stencil`).

## Selected-entry `textureNumLevels` helper plus sample — 2026-11-09

The narrow selected 2D fragment `textureNumLevels` helper may now coexist
with one direct implicit sample of the same texture. The helper remains a
single no-argument straight-line query called once; the selected entry uses
one group-zero non-comparison sampler, `vec2f` coordinates and the same
`texture_2d<f32>`. Core, Naga and HAL agree on selected-entry resource and
operation restrictions. The helper itself cannot sample; mismatched images,
multiple samples, explicit LOD, helper sampling and additional selected
resources remain rejected. FXC compiles the combined shader at
`ps_4_0_level_9_1`; general `texture_query` remains disabled.

Strict hardware execution used the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
FL9_1 and asserted actual FL9_1. The public test ran helper-only and
helper-plus-sample shaders over one- and three-level views. All 64 pixels
matched `[85,64,128,255]` for the one-level view and `[255,128,64,255]` for
the three-level view.

Verification used Rust 1.93.1, Visual Studio 2026 x64, offline Cargo and
bounded `-j8` parallelism:

- Focused Naga, Core, HAL relationship and FXC tests: 1/1 each.
- Strict private D3D11 HAL suite: 462/462.
- Strict Core fixture suite: 125/125.
- Full Naga library: 156/156.
- Strict public D3D11 target: 116/116; the focused GPU test executed without
  skipping and confirmed all 64 pixels in each view.
- Core DX12 regression: 116/116.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed. Targeted
  Clippy passed for Naga, Core, HAL and the public D3D11 test target. Existing
  warnings remain in Naga GLSL tests, Core fixture style, HAL style/tests and
  wgpu-types dependencies; no new warning remains in the increment.

An initial public readback assertion used an incorrect channel expression; it
was corrected to make the mip-count and sampled texel independently visible.
The full HAL regression initially found that an unused sibling-entry sampler
was rejected for the old query-helper path. Resource filtering is now limited
to the selected reachable entry, and both the new proof and the existing
negative test pass. No public adapter enumeration or general query capability
was enabled.

The next bounded gap is to make the existing optional 2x/8x multisample depth
test prove near/far rejection (and stencil where supported). A broader
Core-accepted-operation versus HAL-execution matrix also remains necessary.
Historical FL9_1 hardware, Lumia/CoreWindow execution and physical
device-removal recovery remain unverified.

Checkpoint: `8a8e6350f` (`dx11: sample through selected textureNumLevels helper`).

## Selected 2D dimensions helper plus implicit sample — 2026-11-09

The selected-entry 2D fragment `textureDimensions` helper path now permits
exactly one direct level-less implicit sample of the same image. The helper
remains one no-argument straight-line call, and its `vec2u` result must be
directly converted to `vec2f`. Core, Naga HLSL generation and HAL enforce
matching selected-entry resource and operation restrictions; FXC accepts the
generated pixel shader at `ps_4_0_level_9_1`. Helper sampling, explicit LOD,
extra bound resources, control flow, `select`, additional image operations and
unresolved identities remain rejected. General `texture_query` stays false.

Strict hardware execution selected the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
FL9_1 and verified actual FL9_1. The public WGSL shader used dimensions returned
from the helper and the sampled texel in its color result, then changed the
bound view from 4x2 to 8x4. Readback checked all 64 pixels against
`[50,40,80,255]` and `[40,60,160,255]` for the two halves. This is modern
NVIDIA evidence under a forced profile, not historical-driver evidence.

Verification used Rust 1.93.1, Visual Studio 2026 x64 and offline Cargo:

- New Naga, Core, HAL proof tests and FXC compilation: 1/1 each.
- Focused strict public GPU test: 1/1; full strict public D3D11 target: 116/116.
- Full strict private D3D11 HAL suite: 460/460.
- Full Naga library: 155/155; full Core library: 124/124.
- Core DX12 regression: 115/115; D3D11 HAL `cargo check` passed.
- Clippy passed for Naga, Core, HAL and the public D3D11 test target. Existing
  warnings remain in Naga test style, HAL style and fixture/test helpers; no
  warning was introduced in the new proofs.

The first HAL regression run caught an overbroad resource allowance that also
accepted an unused sampler in the existing `textureNumLevels` helper case.
The relaxation was scoped to helper-backed 2D `textureDimensions`; the
`textureNumLevels` negative test and full HAL suite then passed. Formatting and
`git diff --check` passed. Public default D3D11 adapter enumeration remains
disabled. Next, audit the remaining public Core/HAL operation matrix and choose
the next narrow, evidence-backed shader or rendering gap. Historical FL9_1
hardware, Lumia/CoreWindow runtime and physical device-removal recovery remain
unverified.

Checkpoint: `f8cd666aa` (`dx11: sample through selected textureDimensions helper`).

## Constant-mip cube query and RGBA8_SNORM copy — 2026-11-09

Checkpoint: `1e8407af6` (`dx11: extend FL9_1 cube queries and SNORM copies`).
The cube `textureDimensions` path now accepts a nonnegative literal or
module-constant mip only when the selected entry point does not also sample the
cube. Core, Naga, HAL and command replay share the same mip/view restrictions
and use the reserved `b13` dimensions value. The format extension permits
same-format RGBA8_SNORM GPU copy sources only after a positive selected-device
`TEXTURE2D` query and an enabled backend path. It does not enable color-target
use or texture-to-buffer readback; the CPU-readback format mask remains clear.

Strict hardware execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and confirmed FL9_1. The cube test queried mip 1 of an initialized four-level
8x8 RGBA8 cube and validated 64 pixels against `[128,128,0,255]` within one
byte. A first assertion required exact 128 and observed 127 for the 0.5 channel;
the one-byte conversion tolerance was applied and the strict rerun passed. The
SNORM support query returned `0x000213a0`; upload, GPU copy, sample and RGBA8
readback yielded `[255,0,255,0]` in all 64 pixels. No hardware skip was used
for either focused execution.

Verification used Rust 1.93.1 (`rustc +1.93 --version` reports
`1.93.1 (01f6ddf75 2026-02-11)`) and the Visual Studio 2026 x64 developer
environment. The explicitly named `+1.93.1` rustup alias has a missing
toolchain manifest, so commands used the working `+1.93` alias. Cargo ran
offline with `-j32`:

- Naga targeted cube proof: 1/1; complete Naga library: 152/152.
- Core targeted cube proof: 1/1; strict DX11 fixture library: 121/121.
- HAL targeted cube proof and synthetic SNORM gate: 1/1 each; strict private
  HAL suite: 451/451.
- Strict public cube test: 1/1; strict public `dx11_public` target: 110/110,
  including default-backend isolation; strict `dx11_snorm_copy`: 1/1.
- Core DX12 regression: 109/109; `wgpu-types`: 108/108.
- Clippy passed for Naga, Core tests, HAL tests and both public D3D11 targets.
  Existing warnings remain in HAL queue/index/shader/texture and fixture code;
  none point to these changes. `cargo +1.93 fmt --all -- --check` and
  `git diff --check` passed.

The first strict full HAL run caught two stale test expectations that still
rejected RGBA8_SNORM `COPY_SRC`: the format-projection matrix and generic
sample-upload test. Both now expect query-gated texture-to-texture support;
the strict readback test continues to reject CPU readback. The rerun passed
451/451. The full public suite logged optional format-mip skips; they were not
counted as execution evidence for either new feature. Historical FL9_1 drivers,
Lumia/CoreWindow runtime, and physical device-loss recovery remain unverified.

Next contained shader candidate: permit one same-image implicit sample in the
existing restricted `textureNumLevels` fragment emulation, after matching
Core/Naga/HAL identity proofs. Keep general texture queries disabled.

## Explicit timestamps inside FL9_1 passes — 2026-11-09

Checkpoint: `af1f5d7ac` (`dx11: support FL9_1 timestamps inside passes`).

The selected-device timestamp/disjoint probe now enables
`TIMESTAMP_QUERY_INSIDE_PASSES` along with `TIMESTAMP_QUERY` and
`TIMESTAMP_QUERY_INSIDE_ENCODERS`. HAL records explicit timestamp writes
during render or compute passes as the existing ordered D3D11 `End` command;
query ownership and index validation are unchanged. The public test writes
timestamps before and after a real render draw and twice inside an empty
compute pass, in addition to encoder and pass-boundary writes. Query slots are
not in execution order, so the test asserts ordering using indices
`0, 1, 2, 6, 7, 3, 4, 8, 9, 5`.

Strict hardware used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested/actual
FL9_1. The focused test passed 1/1, all ten values were nonzero and
nondecreasing in command order, and the observed period was 1 ns/tick. The
complete strict private HAL suite passed 450/450; the strict public D3D11
suite passed 113/113 and retained the default-adapter isolation check. Three
optional selected-device subcases reported skips; those are not counted as
hardware executions. HAL and public-target Clippy passed with existing HAL
style/qualification warnings. Formatting and `git diff --check` passed. The
first test draft failed to compile because `BufferViewMut` is intentionally
not indexable; the vertex bytes are now assembled separately and copied with
its write-only API.

Commands (Visual Studio 2026 x64 developer environment, Rust 1.93.1, offline,
Cargo `-j8`):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_timestamp_queries_resolve_on_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public
cargo +1.93 fmt --all -- --check
git diff --check
```

The sampled-image audit found no confirmed unguarded Core/HAL mismatch in its
reviewed paths. Remaining useful bounded shader/resource work includes
constant-mip 3D dimensions, selected-entry helper shapes, non-suffix 3D mip
views and additional format-specific copy paths. Native FL9_1 boundaries and
historical-device, Lumia/CoreWindow and physical device-loss verification
remain open.

## 3D and cube fragment `textureNumLevels` — 2026-11-09

Extended the existing direct fragment `textureNumLevels` emulation to
`texture_3d<f32>` and `texture_cube<f32>`. The public downlevel contract has
separate limits for 2D, 3D and cube. Core validates the selected entry point
against the matching dimension; Naga's existing proof path accepts only the
matching image type; the HAL proof retains the image dimension and checks the
bound view at draw time. Runtime emulation supplies the bound view's mip count
through the existing reserved constant. The 3D limit requires selected-device
RGBA8 volume sampling and the cube limit requires selected-device cube
sampling; neither requires multi-mip support because a single-mip view returns
one. Helpers, control flow, extra resources and general texture queries remain
rejected. Public default-backend selection remains unchanged.

Strict execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, `software=false`), explicitly
requested and confirmed FL9_1. The test initialized every mip and switched
between one-level and four-level 3D/cube views. For each dimension, both
view-count regions matched all 64 pixels; 256 pixels were checked in total.
The forced-FL9_1 RTX result does not establish behavior on historical FL9_1
hardware.

Verification used Rust 1.93.1, Visual Studio 2026 x64, offline Cargo `-j8`:

- Focused strict public GPU test: 1/1, real FL9_1 3D/cube query and readback.
- Strict private D3D11 HAL library: 450/450.
- Strict public D3D11 integration target: 113/113. Three optional device-query
  subcases reported `SKIP` within passing tests; those subcases are not GPU
  execution evidence.
- D3D11 Core library: 110/110.
- DX12 Core regression library: 109/109.
- Naga library: 152/152.
- wgpu-types library: 108/108.
- `cargo check -p wgpu-info`, Naga, wgpu-types, HAL, Core, public-test and
  wgpu-info Clippy commands passed. Existing warnings include feature-set-
  dependent unused `vector_size` variables in Core, default-feature serde and
  qualification warnings in wgpu-types, and existing HAL style/qualification
  warnings.
- Formatting and `git diff --check` passed after the final review.

Commands (run from a Visual Studio 2026 x64 Developer PowerShell):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_fragment_texture_num_levels_3d_and_cube_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib -- --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public
$env:WGPU_DX11_REQUIRE_HARDWARE = $null
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib -- --test-threads=1
cargo +1.93 test --offline -j8 -p naga --features hlsl-out,glsl-in,wgsl-in --lib
cargo +1.93 test --offline -j8 -p wgpu-types --lib
cargo +1.93 check --offline -j8 -p wgpu-info
cargo +1.93 clippy --offline -j8 -p naga --features hlsl-out,glsl-in,wgsl-in --lib
cargo +1.93 clippy --offline -j8 -p wgpu-types --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu-info
cargo +1.93 fmt --all -- --check
git diff --check
```

Checkpoint: `3ceba7844` (`dx11: support FL9_1 3D and cube textureNumLevels`).

Three read-only specialist audits found no confirmed unguarded Core-to-HAL
mismatch in the reviewed texture paths. The highest-ranked small rendering/API
extension is timestamp writes inside render/compute passes, gated by the
existing positive timestamp query. Additional bounded candidates are constant
mip-level 3D texture dimensions and selected-entry helper support for
texture queries. Do not pursue 3D GPU texture copies; the existing FL9_1
10Level9 audit records them as unavailable. Continue the broader public
capability audit; historical devices, Lumia/CoreWindow runtime and physical
device-removal recovery remain unverified.

## Public slope-scaled depth-bias execution — 2026-11-09

Extended `public_dx11_d16_depth_attachment_at_fl91` with a genuinely sloped
fullscreen triangle and public pipelines using positive and negative
`DepthBiasState::slope_scale`. The positive-bias draw failed the less-depth
test and preserved red across all 64 pixels; the negative-bias draw passed and
produced green across all 64 pixels. Existing constant-bias and Core rejection
of nonzero bias clamp remain covered.

The focused strict hardware test passed 1/1. The full strict public D3D11
target passed 112/112 on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) with requested/actual FL9_1 and
`software=false`. Three optional subcases reported device-specific skips:
sub-four-byte vertex input, Float16x2 vertex input, and Rg8Unorm mip support.
They are not counted as hardware execution results. Target Clippy passed with
four existing `too_many_arguments` warnings in `hal_index.rs`, `hal_shader.rs`
and `texture.rs`; `cargo +1.93 fmt --all -- --check` and `git diff --check`
passed. Test checkpoint: `e53561240`.

## 3D fragment `textureLoad` helpers — 2026-11-09

Extended the existing query-gated single-mip 3D float fragment `textureLoad`
subset through selected-entry reachable straight-line helper functions. Core,
HAL and Naga all require direct references to the same image global, reject
resource-handle parameters/results, branching, selects, other image operations
and additional bound resources. The native emulation remains point sampling
at texel centers with out-of-bounds and nonzero-mip loads returning zero.

Strict public execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and confirmed FL9_1. The public test verified two direct texel values, a nested
helper result, out-of-bounds zero and nonzero-mip zero across 320 output pixels.
It also confirmed Core rejects a branching helper before HAL. FXC compiled the
helper shader for `ps_4_0_level_9_1`.

Verification on Rust 1.93.1, Visual Studio 2026 x64 developer environment,
offline, Cargo `-j8`:

- Naga library: 152/152.
- D3D11 Core fixture library: 120/120 with hardware required.
- Strict private D3D11 HAL library: 448/448 with hardware required.
- Strict public D3D11 integration target: 112/112; the hardware helper test
  passed 1/1 and the default-adapter isolation test remained covered.
- DX12 Core regression library: 108/108.
- Naga, Core, HAL and public-target Clippy passed. HAL reported existing lint
  warnings; no new warnings point to the helper implementation.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

An initial Naga full-suite run found the old direct-3D test still expected all
helpers to fail; its assertion was corrected to require the now-supported
straight-line helper. An initial public shader attempted to pass integer
coordinates through helper arguments, which the existing FL9_1 integer gate
correctly rejects; the test now uses a zero-argument nested helper with a
literal coordinate. Historical FL9_1 hardware and physical device-loss
recovery remain unverified. Implementation checkpoint:
`ffafa8b2d` (`dx11: support FL9_1 3D textureLoad helpers`).

## Narrow f32 `mat2x2` inverse — 2026-11-09

Added a separate `DownlevelLimits::matrix_inverse_2x2` contract and a Naga
HLSL lowering for f32 2x2 matrices. The DX11 FL9_1 projection enables only
this narrow operation; general matrix inverse remains disabled. Core checks
the selected stage entry and all regular module functions because Naga's HLSL
writer emits those functions even when unused. Unsupported shapes such as
`mat3x3`, including an unused helper, are rejected before HAL. WGSL has no
matrix `inverse` builtin, so callers reach this subset through GLSL or Naga IR.

Strict public execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and confirmed FL9_1. The vertex and fragment shaders both executed the
non-symmetric inverse; all 64 RGBA8 pixels matched `[0,255,0,255]`. The same
test confirmed a `mat3` inverse fails as Core validation, with no internal HAL
error.

Verification on Rust 1.93.1, Visual Studio 2026 x64 developer environment,
offline, Cargo `-j8`:

- Naga library: 151/151.
- D3D11 Core fixture library: 120/120.
- wgpu-types library: 107/107.
- Strict private D3D11 HAL library: 447/447.
- Strict public D3D11 integration target: 112/112; public default adapter
  isolation remained covered.
- DX12 Core regression library: 108/108.
- The focused strict public mat2 inverse test passed 1/1 after extending the
  rendered vertex stage to execute inverse as well as the fragment stage.
- Targeted Clippy passed for Naga (`hlsl-out,glsl-in,wgsl-in`), wgpu-types,
  D3D11 Core fixture and the public D3D11 target. Existing HAL Clippy warnings
  remain for argument counts in `hal_index.rs`, `hal_shader.rs` and `texture.rs`.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

The selected RTX is a modern FL11-capable GPU restricted to FL9_1; this does
not replace testing on historical FL9_1 hardware. Public D3D11 exposure stays
explicit opt-in, and the overall completion mission remains open.

## Selected-entry 2D fragment `textureLoad` helpers — 2026-11-09 — `6e442d1d14a5873dd83344339de56b909dbe0306`

The narrow FL9_1 fragment `textureLoad` emulation now accepts nested,
straight-line helper calls when each load directly names the same global
single-mip `texture_2d<f32>`. Core, the HAL proof and Naga use the selected
entry point's reachable function closure. Resource handles cannot be passed
through helper parameters/results; reachable control flow, selects, sampling,
queries, additional image identities and other bound resources remain rejected.
Naga omits sibling-only helper functions and globals from the compiled shader.
The 3D emulation still rejects helper functions.

An initial regression sweep found stale Core, HAL and public tests that expected
all helper texture loads to fail. Those assertions were updated to accept the
proven helper subset while retaining control-flow rejection. An independent
read-only audit found no concrete selected-entry or resource-identity issue.

Final-tree results (Rust 1.93.1, Visual Studio x64 developer environment,
offline, Cargo `-j8`):

- Naga library: 149/149.
- D3D11 Core fixture library: 119/119, including selected-entry helper proof
  and hardware fixture coverage.
- Strict private D3D11 HAL library: 447/447 with
  `WGPU_DX11_REQUIRE_HARDWARE=1`.
- Strict public D3D11 integration target: 110/110 with hardware required.
- Focused strict public helper render/readback: 1/1 on NVIDIA GeForce RTX
  3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
  `software=false`. Four 8×8 outputs verified 64 direct-load green pixels,
  64 out-of-bounds zero pixels, 64 nonzero-mip zero pixels and 64 nested-helper
  green pixels (256 checked pixels total).
- DX12 Core regression library: 110/110.
- Naga, Core fixture, HAL and public-test Clippy commands passed. Naga had no
  warnings after cleanup. Core/HAL emitted existing warnings, including
  unused variables in DX12-only validation, argument counts, queue
  qualifications, range-array suggestions and test-only style warnings.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

The complete strict public suite also verifies that DX11 remains outside the
default backend set; explicitly requested `Backends::DX11` already exposes the
experimental FL9_1 adapter projection. RTX execution does not establish
historical FL9_1 driver behavior. Commit `6e442d1d` contains the implementation;
this journal update is pending its own documentation checkpoint.

## Query-gated RGB5A1 and RGBA4 GPU texture copies — 2026-11-09

Added same-format GPU texture-to-texture `COPY_SRC` for the FL9_1 packed
`Rgb5a1Unorm` and `Rgba4Unorm` formats. Each path requires its own backend
implementation gate and the selected-device `TEXTURE2D` query; it does not
require `RENDER_TARGET` and native texture creation remains SRV-only. Both
formats remain excluded from render-attachment and texture-to-buffer readback
support. A production capability test verifies positive, negative, failed and
unavailable query cases and confirms the readback mask remains clear.

The strict public test ran on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) with requested and actual FL9_1 and
`software=false`. For both packed formats it uploaded a padded 2×2 pattern,
copied the region to destination origin (1, 1) in a larger initialized texture,
sampled the destination, and read back 8×8 RGBA8. All 64 pixels matched per
format, including the copied red/green/blue/transparent texels and preserved
white border. The full strict public D3D11 suite passed 107/107, including the
default-enumeration isolation and prior packed-upload regression. The complete
strict private HAL suite passed 446/446.

Commands:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib packed_16bit_gpu_copies_need_only_the_selected_texture2d_query -- --nocapture --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "WGPU_DX11_REQUIRE_HARDWARE=1" && cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rgb5a1_rgba4_gpu_texture_copies_at_fl91 -- --nocapture --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "WGPU_DX11_REQUIRE_HARDWARE=1" && cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "WGPU_DX11_REQUIRE_HARDWARE=1" && cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib -- --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests'
cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public'
cargo +1.93 fmt --all -- --check
git diff --check
```

Results: capability unit test 1/1; focused hardware test 1/1; full strict
public D3D11 107/107; private HAL 446/446; DX12 Core 108/108; both Clippy
commands and formatting/diff checks passed. Clippy emitted pre-existing HAL
warnings. DX12 Core emitted three existing unused-variable warnings in
`wgpu-core/src/validation.rs`; no code in Core or Naga changed for this
increment. Implementation checkpoint: `317cb9a38` (`dx11: support packed
FL9_1 GPU texture copies`).

## Public depth-bias execution — 2026-11-09 — `27b6901bc97706617054f1ef2ff78af4eec630eb`

Extended the public D16 depth-attachment test to execute constant-bias
pipelines through Core and the D3D11 HAL. Two 4×8 regions first store the same
depth/color with zero bias; a positive-bias red draw fails the depth test on
the left, while a negative-bias red draw passes on the right. Full 8×8 GPU
readback matched the expected green/red split. A nonzero `DepthBiasState::clamp`
is rejected by Core through an error scope before HAL. This closes a public
execution-coverage gap for an already implemented FL9_1 capability.

The focused test passed 1/1 and the full strict public D3D11 suite passed
109/109 (`0 failed`, `0 ignored`, `0 filtered`) on NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false, requested/actual
FL9_1). One optional Float16x2 input-format subcase reported a selected-device
query skip; no test case skipped. Public-test Clippy passed with four existing
`too_many_arguments` warnings in `hal_index.rs`, `hal_shader.rs` and
`texture.rs`; formatting and `git diff --check` passed.

Commands:

```powershell
& cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "WGPU_DX11_REQUIRE_HARDWARE=1" && set "CARGO_INCREMENTAL=0" && set "CARGO_PROFILE_DEV_DEBUG=0" && set "CARGO_PROFILE_TEST_DEBUG=0" && cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1'
& cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "CARGO_INCREMENTAL=0" && set "CARGO_PROFILE_DEV_DEBUG=0" && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public'
cargo +1.93 fmt --all -- --check
git diff --check
```

## Indexed LineStrip restart in public render bundles — 2026-11-09 — `42cecc33690e20c789e1aa59bdb0dcf28283fa86`

Added `public_dx11_indexed_line_strip_restart_render_bundle_at_fl91`. It
submits consecutive render bundles using Uint16 and Uint32 indexed LineStrip
pipelines, each with two segments separated by the primitive-restart index.
The test drops the original vertex-buffer handle after bundle recording, then
reads the full 16×16 RGBA8 target and checks each expected colored segment,
both restart gaps, row placement, and absence of unexpected pixels. This
verifies render-bundle replay, both index widths, restart handling, and resource
retention together; it adds coverage rather than a new capability.

The focused strict test passed on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false, requested/actual
FL9_1), with per-segment readback counts `[2, 2, 2, 2]`. The complete strict
public D3D11 target passed 109/109 with no skipped test cases. HAL/test Clippy,
workspace formatting and `git diff --check` passed. Clippy emitted only the
existing `too_many_arguments` warnings in `hal_index.rs`, `hal_shader.rs` and
`texture.rs`.

Commands:

```powershell
& cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "WGPU_DX11_REQUIRE_HARDWARE=1" && set "CARGO_INCREMENTAL=0" && set "CARGO_PROFILE_DEV_DEBUG=0" && set "CARGO_PROFILE_TEST_DEBUG=0" && cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1'
& cmd.exe /d /s /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set "CARGO_INCREMENTAL=0" && set "CARGO_PROFILE_DEV_DEBUG=0" && cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public'
cargo +1.93 fmt --all -- --check
git diff --check
```

This is a targeted lifecycle/command coverage increment. The broader
operation-by-operation Core/HAL audit, historical FL9_1 drivers, Lumia ARM32/
CoreWindow execution and physical device-removal recovery remain unverified.
Next task: verify the already implemented constant/slope depth-bias path through
public Core pipeline creation and render/readback, including rejection of the
unsupported nonzero clamp descriptor.

## Depth-only multisample pipelines — 2026-11-09 — `dbfb26b41`

The HAL no longer rejects a multisample pipeline solely because it has no
color target. A depth-only pipeline still requires positive support for its
selected depth format and sample count; a present color target independently
requires the implemented RGBA8 render/resolve path. Core already allowed this
public request, so the old HAL check surfaced as a pipeline linkage failure.

The strict public regression extends
`public_dx11_resolves_four_sample_rgba8_at_fl91`: it performs a targetless 4x
depth prepass, then draws farther geometry in a color+depth pass and resolves
the RGBA8 target. On the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false, requested/actual FL9_1), D16, Depth24Plus and
D24S8 each blocked the later draw; all 64 resolved pixels remained blue. The
focused public test passed 1/1. The final strict public D3D11 target passed
108/108 on the same forced-FL9_1 adapter; the complete private HAL suite passed
443/443. Core's zero-adapter isolation test passed 1/1 and the DX12 Core suite
passed 106/106. HAL and public-test Clippy, workspace formatting and
`git diff --check` passed. Clippy reported existing HAL warnings; the Core
DX12 build reported three unrelated unused-variable warnings. The strict runs
used the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false, requested/actual FL9_1). No required strict
hardware case skipped.

The render audit confirmed this Core/HAL mismatch. The resource audit found no
other mismatch in its reviewed texture, transfer, view and mapping paths. A
surface-format concern was withdrawn after the Microsoft required-format table
confirmed BGRA8 UNORM and sRGB display/render-target support at FL9_1. The
broader operation-by-operation audit remains open; historical hardware,
Lumia/CoreWindow execution and physical device removal/recovery remain
unverified. Independent shader/pipeline, buffer/command and capability audits
found no further confirmed Core/HAL execution mismatch. They identified only
the clarity issue that `max_texture_dimension_1d` is nonzero while the
`texture_1d` capability flag is false; Core still rejects 1D texture creation.
An additional public panic-path audit found the HAL stubs and unwraps gated by
unadvertised features or Core mapping state; no legal public path to a panic
was confirmed. A separate unsafe-contract audit found no concrete public-path
memory-safety, ownership, mapping, or submission-lifetime issue. The
implementation and test changes are recorded in checkpoint `dbfb26b41`.

## Viewport-remap fail-fast and shader feature gates — 2026-11-09 — `dbfb26b41`

Render-pipeline creation now requires the FL9_1 viewport-remapped vertex
variant. A shader can no longer leave a usable-looking pipeline whose later
Core-valid fractional-viewport draw latches `DeviceError::Unexpected` and is
reported by Core as device loss. A variant compilation or nonfatal native
shader-creation failure is returned as a vertex linkage error at pipeline
creation; device and out-of-memory failures retain their device-error mapping.
The normal vertex variant remains compiled for integer viewport use.

The exact FXC boundary is covered by Naga/HLSL tests: the ordinary 254-vector
vertex shader compiles, while the viewport-remapped shader fails with FXC
X5615 because it requires 258 instruction slots against a 256-slot target.
The strict public regression observes the pipeline internal error and then
completes the existing render/readback on the same device (17 green and 4 red
pixels). A separate strict fractional-viewport rendering test passed.

The public FL9_1 feature-gate test also now verifies `CLIP_DISTANCES` and
`SHADER_F16` are unavailable, `SHADER_F16_IN_F32` is absent, and Core rejects
clip-distance output and `quantizeToF16`. The strict public D3D11 target passed
105/105 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false, requested/actual FL9_1). Three optional format
subcases visibly skipped from selected-device query results; no required strict
hardware test skipped. The strict private HAL suite passed 443/443, including
fractional viewports. Public and private HAL Clippy passed with existing
warnings only; formatting and `git diff --check` passed.

An initial public rebuild at `-j8` and then `-j2` with debug info enabled hit
Rustc LLVM out-of-memory failures. The successful full public run used
`CARGO_INCREMENTAL=0`, `CARGO_PROFILE_DEV_DEBUG=0`,
`CARGO_PROFILE_TEST_DEBUG=0`, and `-j2`.

The render audit found and fixed this conditional gap. The query/surface audit
found no new mismatch. A proposed combined depth-view mismatch was a stale
finding: commit `f3926cf9b` had already fixed that path. These focused audits
do not complete the operation-by-operation Core/HAL review. Historical FL9_1
drivers, Lumia ARM32/CoreWindow execution, and physical device removal/recovery
remain unverified. The viewport and shader-gate changes are part of the
current verified checkpoint.

## Public buffer lifetime before submission — 2026-11-09 — `67e8fcde45592a8a8ccc9c7759e9a0569313abd2`

A public Core↔HAL buffer-lifetime audit found no confirmed ordering or ownership
defect. Core retains submitted resources and pending queue writes; HAL copy
commands retain COM references. The audit identified missing public GPU
coverage for dropping source and destination buffer handles after command
recording but before submit. Added `public_dx11_buffer_handles_survive_until_submission_at_fl91`:
it initializes a 64-byte source, queues destination sentinels, records a
nonzero-offset partial copy followed by readback, finishes the command buffer,
drops both public handles, and submits. The strict readback verified all 64
bytes, including unchanged sentinels. Commit:
`67e8fcde45592a8a8ccc9c7759e9a0569313abd2`.

The focused strict test passed 1/1 and the complete strict public D3D11 target
passed 105/105 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, requested/actual FL9_1, software=false). Optional selected
feature branches still log visible skips; all strict-required hardware tests
executed. `cargo +1.93 fmt --all -- --check`, public-test Clippy and
`git diff --check` passed. Clippy retained four existing HAL
`too_many_arguments` warnings; the new test produced no warning.

A separate attachment lifecycle audit found Core initialization/discard
tracking aligned with the implemented HAL load/store, transient, depth/stencil
and resolve subset; it found no reproducible mismatch and made no changes.
Historical-device tests and actual device removal during map flush/invalidate
remain outstanding.

## `vertex_index` on TriangleStrip — 2026-11-09 — `5552a8fa57ca1c34c89dcec70524150d15b2674f`

The opt-in FL9_1 projection and defensive HAL validation now permit the
emulated `vertex_index` subset for TriangleStrip. Direct draws preserve source
IDs across native strip batching. Indexed Uint16/Uint32 strips resolve signed
base vertices and primitive-restart runs, then expand to winding-preserving
triangle-list identity values before the same Window/Gather execution paths.
The remap and hidden IA streams restore after replay. Core fixture validation
and the public topology projection include TriangleStrip; default backend
selection and ordinary D3D11 enumeration remain unchanged.

The strict public hardware test passed on NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), software=false, requested and actual
FL9_1. It rendered direct `first_vertex=100000`, Uint16 and Uint32 restart
strips, positive and negative base-vertex cases, and a wide-index Gather pair.
All six checked bands contained the expected red output; 2,942 pixels were red
and the remaining target pixels were clear.

Validation executed (MSVC x64 developer environment, Rust 1.93.1):

- Private HAL direct identity/overflow test: 1/1.
- Indexed strip expansion CPU test: 1/1.
- Full strict private HAL suite: 443/443.
- Focused strict public TriangleStrip test: 1/1.
- Full strict public D3D11 suite: 104/104, including default enumeration isolation.
- Strict controlled Core fixture: 1/1.
- DX12 Core regression: 106/106.

The hardware suites required `WGPU_DX11_REQUIRE_HARDWARE=1`; selected-device
optional-feature skips remain visible and are not counted as GPU execution.
The initial test failures were corrected in this increment: restart expansion
test expectations now match batch parity, the U16 buffer has a valid four-byte
allocation size, and readback checks the rendered bands without brittle
subpixel-edge assumptions. Existing HAL queue qualification warnings and the
three DX12 Core unused-variable warnings remain. Older FL9_1 drivers have not
been tested.

## Two-byte indexed vertex extents and padded 3D upload — 2026-11-09 — `90026ebf3`

Removed an unjustified four-byte copied-extent requirement from indexed
rebase, line/strip, point and triangle-strip planner paths. The planners still
enforce aligned buffer offsets and strides, nonzero extents, extent within
stride, bounds and checked arithmetic. CPU planner tests cover two-byte
vertices in native Uint16, Window, Gather, indexed line and indexed strip
paths. The RTX reports no optional sub-four-byte IA formats at forced FL9_1,
so this compatibility case has synthetic coverage but no positive GPU run.

Added a strict private HAL execution test for an offset, padded 2×2×2 RGBA8
upload into a 4×4×4 volume at a nonzero origin. It samples the volume into an
8×8 target, checking all eight uploaded voxels, 56 zero-initialized untouched
voxels, row/depth padding and output sentinels. The NVIDIA GeForce RTX 3090
Ti (`0x10de:0x2203`, driver `32.0.16.2101`) created the requested/actual FL9_1
device and passed. The full private HAL suite passed 441/441, the strict
public D3D11 target passed 103/103, formatting and `git diff --check` passed,
and HAL Clippy completed with the same existing backend warnings (no new
warning from these changes). The public suite logs optional feature skips;
they are not counted as GPU executions. Historical FL9_1 drivers remain
unverified.

## `vertex_index` on LineStrip — 2026-11-08 — `b93747fb4`

The selected FL9_1 projection and HAL now permit LineStrip in the existing
emulated `vertex_index` subset. Direct draw replay preserves `first_vertex`
through native strip batching; indexed Uint16/Uint32 strips split restart runs
after signed `base_vertex` resolution and attach matching IDs to compact
Window or wide-pair Gather segments. Temporary and hidden input-assembler
streams are restored after replay. TriangleStrip remains rejected because its
expansion must preserve strip winding and is a separate implementation task.

The strict public GPU test used the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) with requested and actual FL9_1 and
software=false. Direct batched `first_vertex=100000`, Uint16 and Uint32
restart-separated strips with positive and negative base vertices, and a wide
Gather pair produced seven independently checked red readback lines. The
private HAL suite passed 438/438; the strict public D3D11 target passed
103/103; the Core fixture suite passed 115/115; and the DX12 Core suite passed
106/106. The DX12 build emitted three unused-variable warnings in
`validation.rs`. Formatting and non-denying Clippy checks passed. Clippy
reported existing HAL warnings (including queue qualifications, test range
literals and argument counts), plus one single-element loop from this
increment; the loop was corrected. The public test target retains one existing
`std::collections::HashMap` lint. The full strict public run reports some
optional capability probes as SKIP; those do not count as hardware passes.
Historical FL9_1 hardware is not simulated by this modern adapter.

## `vertex_index` on LineList — 2026-11-08 — `0835c23cf`

Core's selected-topology projection and the HAL now permit LineList in the
existing emulated `vertex_index` subset; LineStrip and TriangleStrip remain
rejected. Direct replay applies `first_vertex` once to ordinary vertex streams
and generated IDs. Indexed Uint16/Uint32 replay keeps complete line pairs,
uses effective indices after `base_vertex`, and selects compact Window or
Gather remapping when needed. A scoped restore guard returns ordinary and
hidden IA vertex slots to their original buffer, stride and offset after
replay, including early errors.

Strict public GPU execution used the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested/selected
FL9_1, and software=false. The direct, Uint16 and Uint32 cases read back both
expected line rows. A second same-pass test exercised a nonzero `first_vertex`
stream and an indexed Uint32 Gather pair using vertex IDs 3 and 70,000; each
was followed by a triangle without an explicit vertex-buffer rebind, and
distinct readback regions/colors verified both outputs. The full strict public
D3D11 target passed 102/102. The full strict private HAL suite passed 437/437.
The Core fixture suite passed 115/115, including LineList acceptance and strip
rejection. A first test shader was rejected before GPU execution because it
used unsupported FL9_1 integer bitwise arithmetic; the shader was narrowed to
the supported conversion-only builtin use and the rerun passed. The Core
fixture suite also exposed an outdated expected feature mask; its expectation
now includes the query-gated timestamp feature pair when present.

## D16 CPU-readback staging boundary — 2026-11-08

A forced FL9_1 RTX 3090 Ti capability diagnostic confirmed that D16 depth
readback is not currently part of the public contract: Core's selected-device
copy-format mask deliberately excludes depth formats, so this is not a
Core-valid request that the HAL mishandles. An exploratory native probe found
that creating a CPU-readable staging texture in each tested D16-compatible
16-bit format (`R16_TYPELESS`, `R16_UNORM`, `R16_UINT`, `R16_SINT`,
`R16_SNORM`, `R16_FLOAT`) returns `E_INVALIDARG` at requested/actual FL9_1.
An earlier typed-D16 staging attempt also failed during map/copy and reported a
driver internal error. No D16 readback was validated and no capability was
exposed. A future implementation would require a separate shader conversion
path that samples depth and packs the exact 16-bit value into a CPU-readable
color texture; that is a materially larger emulation, not a staging-format
substitution. The experiment was removed from the working tree. No D16
readback capability was exposed.

## Query-gated R8G8_SNORM sampled mip chains — 2026-11-07

The R8G8_SNORM path now accepts multiple mips for the existing sampled/upload
usage subset when its FL9_1 hardware guarantee, backend implementation path
and selected-device `TEXTURE2D | MIP` query all permit the shape. The same
predicate removes `SINGLE_MIP_ONLY` from the public format projection; absent
or failed format queries remain conservative. `SHADER_SAMPLE` continues to
control filterability independently. NPOT mip chains still require positive
full-NPOT support. Render-target, copy-source and CPU readback uses remain
unsupported.

The strict public test ran on the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) at explicitly requested and selected
FL9_1. The device reported R8G8_SNORM support mask `0x00021320`. The test
uploaded three distinct signed RG mip levels and sampled level 1 into a 4×4
RGBA8 target. All 16 pixels matched `[192, 64, 0, 255]` within one code value.
This verifies the positive query path on a modern forced-FL9_1 driver, not
legacy driver compatibility.

The synthetic capability test passed for positive, missing, failed and
unavailable MIP queries, disabled backend/feature-level guarantees, and NPOT
positive/negative query behavior. After updating two pre-existing assertions
that still expected the old one-mip restriction, the strict private HAL suite
passed 402/402 and the strict public D3D11 suite passed 78/78 (one optional
Float16x2 input subcase explicitly skipped). The public adapter-isolation
regression passed 1/1 and DX12 core passed 94/94. Targeted Clippy passed for
HAL and the public test target; it reported the existing HAL argument-count,
single-range-vector, test-order and needless-borrow warnings. Rustfmt and
`git diff --check` passed.

Commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
~~~


## Query-gated NPOT mipmapped 2D textures — 2026-11-07

The capability model now separates FL9_x conditional NPOT behavior from a
positive `D3D11_FEATURE_DATA_D3D9_OPTIONS.FullNonPow2TextureSupport` result. The
internal `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` flag is projected only when the
backend path and effective feature-level capability allow it. D2 texture
creation, sampled-view validation and shader/resource draw compatibility now
use the same selected-device result; the per-format `MIP` query remains
required. Negative/unavailable 9_x queries and a disabled backend path remain
conservative in synthetic tests. NPOT cube shapes and arrays were not enabled.

The strict public test explicitly requested and selected FL9_1 on the NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false).
The runtime reported full-NPOT support. It created a 6×10 RGBA8 texture with
four mip levels, uploaded distinctive colors through padded buffer-to-texture
copies in one submission, then sampled mip 2 in a later render submission. All
64 output pixels matched `[0, 0, 255, 255]`. The test uses clamp addressing
because the public FL9_1 projection's finite repeat limit independently
prevents repeat sampler creation; it validates NPOT mip capability, not NPOT
wrap semantics. The RTX's positive optional query does not model restrictive
historical 9_x drivers.

Verification with pinned Rust 1.93.1 and the Visual Studio x64 environment:
the strict private HAL suite passed 401/401, the strict public D3D11 suite
passed 74/74, core's selected FL9_1 adapter test passed 1/1, and the DX12 core
regression passed 94/94. The full HAL rerun updated three earlier assertions
that still assumed all NPOT mip chains were forbidden; the corrected tests
check positive, negative and unavailable driver-query behavior. HAL build,
targeted Clippy, Rust formatting and `git diff --check` passed. Clippy reported
existing warnings in `texture.rs`, `hal_device.rs`, `swapchain.rs` and the
texel-load test; no new warning pointed to this increment.

Commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib dx11_scaffold_tests::requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib
cargo +1.93 build --offline -j8 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustup run 1.93 rustfmt --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/sampling.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/texture_transfers.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The wider FL9_1 completion mission remains partial; see the capability matrix
for outstanding resource, shader, presentation and historical-driver gaps.

## BC2/BC3 texture upload and copy support — 2026-11-07 — `324e45744`

Added the private one-mip BC2_UNORM and BC3_UNORM sampled-upload paths beside
the existing BC1 path. The implementation now uses queried per-format support,
16-byte blocks, 4×4 aligned extents and regions, and same-format GPU
texture-to-texture copies. BC1 retains its 8-byte block layout. Strict tests
uploaded padded source rows from aligned nonzero offsets, partially replaced a
block, copied the texture to a second resource and verified all 256 sampled
pixels for BC1, BC2 and BC3. The RTX 3090 Ti reported `0x000213a0` and
filterable support for all three formats at explicitly requested and selected
FL9_1.

The aggregate public `TEXTURE_COMPRESSION_BC` feature remains disabled because
the wgpu feature also covers BC4–BC7, which are not implemented by this FL9_1
backend. The per-format downlevel texture-to-buffer mask still excludes the BC
formats; readback from these shader-resource-only resources is not safe at
FL9_x. Historical FL9_1 adapter behavior remains unverified.

Verification with Rust 1.93.1 and the Visual Studio x64 environment:

- Strict private D3D11 HAL suite: 401/401 passed on the RTX 3090 Ti; the final
  BC helper refactor was followed by another 3/3 strict BC test run.
- Strict public D3D11 suite: 73/73 passed, with the optional Float16x2 input
  subcase skipped because the selected driver does not report its IA format.
- Strict core FL9_1 adapter selection: 1/1 passed.
- DX12 wgpu-core regression suite: 94/94 passed.
- D3D11 HAL Clippy passed with only existing warnings; formatting and
  `git diff --check` passed.

Commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib r8_texture_tests::fl91_bc -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib dx11_scaffold_tests::requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests
rustup run 1.93 rustfmt --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
~~~

The overall mission is still partially complete. Next continue the selected
core/HAL contract audit, focusing on remaining publicly accepted operations
and historical/runtime validation gaps rather than widening BC feature
advertisement.

## Public front-face and culling readback — 2026-11-07

Added a strict public wgpu test for rasterizer winding and back-face culling,
which previously had private HAL coverage but no public readback check. One
draw uses `FrontFace::Ccw` and one uses `FrontFace::Cw` over adjacent triangles
with opposite winding. On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), the selected device was explicitly requested and
confirmed at FL9_1. Readback found 10 red pixels from the CCW triangle, 10 green
pixels from the CW triangle and 44 clear pixels; neither winding appeared in
the opposite half. The focused strict hardware test passed 1/1.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_front_face_and_backface_culling_at_fl91 -- --nocapture --test-threads=1
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

This closes public execution evidence for the configured front-face and
back-face culling state; it does not close the wider pipeline/render-state or
historical-driver audit.

## Occlusion-query feature-level boundary — 2026-11-07

The 10Level9 feature-level table explicitly marks occlusion queries
unavailable at FL9_1 and available beginning at FL9_2. The private event
query used for queue fences is only a completion primitive and must not be
confused with public occlusion-query support. Keep the public downlevel flag
absent and preserve core rejection before the HAL. The strict public test
already checks that occlusion, timestamp and pipeline-statistics query-set
requests fail at the selected FL9_1 projection; the unsupported HAL query-set
path is therefore not reached. Timestamp and pipeline-statistics feature-level
guarantees were not established by this check and remain unimplemented.

Reference: [Microsoft's Direct3D feature-level table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro).
The focused strict public query-validation test passed 1/1 on the NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) with FL9_1
explicitly requested and selected; it reported `occlusion_queries=false` and
confirmed core rejected all three query-set categories before HAL. The
selected-entry loop-analysis unit test also covers an unbounded loop whose
false exit is expressed as the constant comparison `1u == 2u`; it passed
under `cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features
--features dx11,wgsl,std --lib
shader_loop_detection_follows_only_selected_entry_calls`. The initial
public-test filter used the wrong function name and selected zero tests; the
corrected command selected and passed the one hardware test.

## Selected-entry dynamic-loop validation — 2026-11-07

The previous loop checkpoint allowed statically bounded loops but reported
dynamic-loop support through the same `DownlevelLimits::shader_loops` field.
That allowed a runtime-dependent WGSL loop to pass core and fail later in
FXC with X3511. Added `shader_dynamic_loops` as a separate downlevel limit,
kept static loops enabled for the tested FL9_1 backend, and projected dynamic
loops only when backend implementation and both shader-stage guarantees are
present. Serde defaults the new field to true for older capability data, which
preserves the existing behavior of other backends.

Core now checks the selected entry points even when static loops are allowed.
The Naga IR analysis follows reachable helper calls, recognizes runtime
uniform/global values and local aliases, and fails closed for unknown
expressions. It also tracks caller-local variables passed to pointer
parameters so a helper cannot hide a runtime loop bound by mutating it through
a pointer. It also rejects a provably unbounded loop whose only exit condition
is the literal `false`. The analysis deliberately may reject some constant
helper-parameter cases conservatively; no dynamic-loop support is advertised.
A shader module's unselected entry points do not impose the restriction on
the selected one.

The strict public D3D11 loop test now renders static vertex and fragment loops
and validates all 64 pixels, then expects dynamic vertex and fragment
pipelines to return core validation errors before FXC. On the NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, `software=false`), the
requested and actual device level was FL9_1. The focused post-change run passed
1/1 and reported the adapter and exact result.

Verification with Rust 1.93.1 and the Visual Studio x64 environment:

- Strict public D3D11 suite: 70/70 passed. The optional Float16x2 input-format
  test includes a selected-device query skip branch.
- Strict private D3D11 HAL suite: 393/393 passed, including real GPU tests.
- Strict controlled core fixture: 103/103 passed, including hardware
  rendering and the zero-public-adapter isolation check.
- Core library tests: 95/95 passed.
- `wgpu-types --all-features`: 94/94 unit tests and 14/14 runnable doc tests
  passed; 21 doc tests were ignored.
- DX12 core tests: 94/94; DX12 HAL tests: 4/4.
- Core, HAL and `wgpu-types` Clippy passed. Existing warnings remain in
  `wgpu-types` qualification tests and D3D11 HAL test/helper code; no new
  warning was reported for the loop-analysis code.
- An initial core test command omitted `std` and WGSL features and failed to
  compile the test crate. The corrected commands above passed.

The separate-alpha FL9_1 restriction remains in the same working tree and is
still covered by its projection test. Historical FL9_1 hardware and Lumia
runtime remain unverified. This closes a narrow core/HAL validation mismatch;
the overall completion mission remains partial.

## Separate-alpha blend contract review — 2026-11-07

Rechecked the remaining separate-alpha blend concern against Microsoft's
10Level9 feature-level table. It explicitly marks Separate Alpha Blend as
unavailable at FL9_1, and the D3D9 options query does not provide a
selected-device override. Keep `HardwareGuarantees::separate_alpha_blend`
false, leave `DownlevelFlags::SEPARATE_ALPHA_BLEND` unset, and reject distinct
RGB/alpha equations when both channels are written. Existing same-equation
blending remains supported. A test-only RTX diagnostic is not public
capability evidence.

Added a CPU test that asserts the production FL9_1 projection keeps this flag
disabled. The strict public core fixture already checks validation rejection.
Microsoft reference: [Direct3D feature levels](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro).

After this contract correction, the full strict private HAL suite was rerun
with Rust 1.93.1 and the Visual Studio x64 environment. It passed 392/392
tests, including the new projection test and the existing forced-FL9_1 GPU
rendering, transfer, depth/stencil, sampler and surface tests. The run used
`WGPU_DX11_REQUIRE_HARDWARE=1`; it did not rely on hardware skips.

~~~powershell
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j2 -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1'
~~~

A read-only Lumia Device Portal availability check for
`/api/os/devicefamily` timed out during the helper's initial HTTPS request;
the device also did not answer a LAN ping. No package was deployed and no
device state was changed. Mobile runtime validation remains blocked by device
reachability in this session.

## 2D/cube copy compatibility — 2026-11-07

Extended the strict public cube transfer test to cover compatible native
`Texture2D` copies in both directions between ordinary single-layer textures
and cube subresources. A distinct 2D upload was copied into cube face 2 and
verified through the cube sampling render. Cube face 3 was copied into an
ordinary 2D texture, then read back; all 64 RGBA8 texels matched the source.
These checks exercise the same-format, one-mip, one-sample path on the actual
device. Cube-to-buffer readback remains rejected by the FL9_1 downlevel layer
limit and SRV-only staging restriction.

The focused strict public test passed 1/1 on the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, `software=false`), with FL9_1
requested and selected. The modified cube sampling test also checked all
960 rendered pixels, including the copied 2D face, across four mips.
Checkpoint: `7fb6cf3c9` (`dx11: expand FL9_1 public integration coverage`).

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_cube_texture_sampling_at_fl91 -- --nocapture --test-threads=1
```

## Query-gated cube-face GPU copies — 2026-11-07

The sampled cube path now permits `COPY_SRC` when the selected format supports
the usage. The HAL maps the requested face and mip to the native 2D subresource;
`CopySubresourceRegion` copies face 0 from a second cube into face 1 of the
sampled cube at all four mips. The strict public test then samples the
destination cube and validates 960/960 pixels, so the copy result is observed
through the GPU rather than inferred from command acceptance.

Cube resources remain shader-resource-only at FL9_1. Core advertises a
one-layer maximum for texture-to-buffer source textures and rejects cube
readback before HAL; the HAL independently rejects CPU staging from a resource
whose only bind flag is `D3D11_BIND_SHADER_RESOURCE`. This follows Microsoft's
[10Level9 context restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context).
The restriction does not block GPU-to-GPU copies between compatible cube
subresources. The cube format mask still requires selected-device
`TEXTURE2D|TEXTURECUBE` and sampled-format support.

Strict hardware execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with FL9_1 requested and selected and
`software=false`. The public D3D11 target passed 70/70 test functions (one
optional Float16x2 subcase was visibly skipped). The core fixture passed
103/103, the private DX11 HAL suite 391/391, `wgpu-types` 92/92 without serde
and 94/94 with serde, the DX12 core regression 94/94, and DX12 HAL 4/4. The
fixture's previous unconditional expectations for depth-texture and comparison
sampler rejection were corrected to follow the selected device's already
query-gated capabilities. Clippy passed with existing warnings: core's
nonminimal-boolean, single-element-loop and nested-if suggestions; HAL's seven
single-range-in-vec-initializer suggestions, one test-module ordering warning,
the existing `create_hal_view` argument-count warning and one needless borrow.
The multi-file rustfmt check still reports existing formatting drift in adjacent
3D-texture code; the edited public test file passes rustfmt. No historical
FL9_1 driver or Lumia runtime was exercised.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-types --lib
cargo +1.93 test --offline -j2 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j2 -p wgpu-hal --lib --no-default-features --features dx12 -- --test-threads=1
~~~

## Public R8 filtering and clear-buffer verification — 2026-11-07

The strict public sampled-format test now exercises both nearest and linear
filtering for R8_UNORM. Sampling a two-texel `[64, 192]` pattern across an
8×8 target produced the expected eight-column gradient within two code values;
all 64 pixels matched. A new public `clear_buffer` test uploads a distinct
64-byte pattern, clears only bytes 16–48, copies the result to a mapped
readback buffer, and verifies both the zeroed range and every preserved byte
outside it. Core rejected unaligned and out-of-bounds clear ranges before HAL
execution. The first negative check exposed that clear validation is finalized
when the encoder is finished; the test now finishes the invalid encoder inside
the validation scope.

Both operations executed on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with FL9_1 explicitly requested and
selected and `software=false`. The complete strict public D3D11 target passed
70/70 test functions; one optional Float16x2 vertex-input subcase was visibly
skipped because the selected-device query reports it unsupported. Public-test
Clippy passed with the existing HAL `create_hal_view` argument-count warning;
the changed Rust test file passed rustfmt and `git diff --check`. The public
offscreen example was also rebuilt and run through wgpu on that physical
adapter; it validated all 1,024 pixels (112 red, 277 green and 635 blue
background).

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_sampled_upload_renders_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_clear_buffer_zeros_only_the_requested_range_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 run --offline -j2 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std -- C:\Users\ben\AppData\Local\Temp\dx11-fl91-offscreen.ppm
cargo +1.93 clippy --offline -j2 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

### Follow-up verification — 2026-11-07

After the compiler-boundary regression was added, the complete strict private
D3D11 HAL suite passed 394/394 with `WGPU_DX11_REQUIRE_HARDWARE=1`. The first
public-suite compile at `-j16` failed in rustc while mapping `wgpu-types`
metadata and exited with `STATUS_STACK_BUFFER_OVERRUN`; rerunning the same
strict 70-test public suite at `-j2` passed 70/70 with zero test skips. The
focused strict sample-mask gate test passed 1/1. Commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_fragment_sample_mask_output_at_fl91 -- --nocapture --test-threads=1
~~~

The hardware was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) explicitly requested and selected at FL9_1. The optional
Float16x2 subcase remains capability-skipped because the selected adapter
does not report its vertex-input support. No historical-device claim follows.

## Follow-up — sample-mask emulation compiler limit — 2026-11-07

Checked whether the current single-sample renderer can safely lower fragment
`@builtin(sample_mask)` output to `discard`. The target sample observes only
bit 0, so the translation must distinguish odd masks from masks with only
higher bits set. A focused FXC test confirmed `ps_4_0_level_9_1` rejects
`mask & 1` with X3535 (`Bitwise operations not supported on this target`).
Treating all nonzero masks as covered would change valid WGSL semantics. Kept
the existing selected-entry core rejection and added a negative compiler
regression test. The focused test passed 1/1; this is compiler evidence, not a
hardware execution test. A future exact arithmetic lowering would need a proof
for the full `u32` domain, not just small mask values. The strict public
regression also passed 1/1 on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested and selected
at FL9_1; core rejected the mask before FXC and accepted the color-only sibling.

## Follow-up — constant sample bias through helper calls — 2026-11-07

The FL9_1 projection already supported finite literal texture-sample biases
from -3 through +3, but conservatively rejected a literal passed to a helper
parameter. Naga's HLSL for this form compiles under `ps_4_0_level_9_1` when
FXC sees the constant at the call site. Core and the private HAL now resolve
literal and unary-negated-literal arguments through nested helper calls and
validate each call site independently. Dynamic values, out-of-range literals,
offset sampling and mixed call sets containing an invalid literal remain
rejected. Analysis fails closed when its call-depth or work budget is exceeded.

CPU tests cover direct helper forwarding, nested calls, two calls with different
literal values, dynamic arguments, out-of-range values and selected-entry
isolation. The public strict mip test now renders both direct and helper-based
+1 bias sampling; the direct/helper/implicit mip cases matched all 384 output
pixels on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver
`32.0.16.2101`, explicitly requested and selected at FL9_1. The strict full
public suite passed 70/70 after a transient first-run adapter enumeration
failure in the unrelated RGB565 test; that test passed alone and in the
successful full rerun. The strict private HAL suite passed 395/395, core DX11
passed 95/95 and core DX12 passed 94/94.

Core, HAL and public-test Clippy passed. Existing warnings remain for
`create_hal_view` argument count and HAL test expressions/order. The new HAL
file changes were formatted with Rust 1.93 rustfmt. Workspace `cargo fmt --all
-- --check` still reports unrelated existing formatting differences in Naga's
HLSL writer, core resource/fixture code, D3D11 capabilities/transfer code and
`wgpu-types`; these were left unchanged. `git diff --check` passed. A public
suite build at `-j16` hit Windows paging/metadata allocation failure; `-j2`
completed the same strict suite successfully.

Commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sampled_mip_chain_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib -- --test-threads=1
cargo +1.93 test --offline -j1 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib -- --test-threads=1
cargo +1.93 clippy --offline -j1 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j1 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std
cargo +1.93 clippy --offline -j1 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 wgpu-hal/src/dx11/hal_shader.rs
git diff --check
~~~

## Public R8 linear-filter verification — 2026-11-07

The selected-device projection advertises R8_UNORM linear filtering when the
format query reports the required filtering support. The existing strict
public sampled-format test now distinguishes that behavior from nearest
filtering: a two-texel `[64, 192]` R8 texture is sampled across an 8×8 target,
and each output column is checked against the expected interpolated red value
with a two-code-value tolerance. All 64 pixels matched. This closes the R8
filtering evidence gap on the selected driver; it does not establish behavior
on historical FL9_1 hardware.

The full strict public D3D11 target passed 69/69 test functions on the NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly
requested and selected at FL9_1 with `software=false`. The optional Float16x2
input-format subcase remained visibly skipped after the selected-device query
returned false. Public-test Clippy passed with the existing HAL
`create_hal_view` argument-count warning; rustfmt on `wgpu/tests/dx11_public.rs`
and `git diff --check` passed. The stale introductory claim that the adapter
reports no optional features was corrected in `CAPABILITIES.md` and
`TESTING.md`; the actual projection may report query-gated 16-bit format and
comparison-border features.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_sampled_upload_renders_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

## Public wrong-thread present cleanup — 2026-11-07

Implementation commit: `8d6c8f93e12c766a5f8fb413f2abf781c4b55408`
(`dx11: release surface frames after rejected present`). An independent HWND
lifecycle audit found that public `Queue::present` consumes its
`SurfaceTexture` before core checks the HWND owner thread. Core returned
`SurfaceError::Invalid` without clearing `Presentation.acquired_texture`, so
the surface stayed unusable. A strict public test reproduced this on the RTX:
the default uncaptured-error handler panicked on the worker; with a capturing
handler, the next owner-thread acquisition returned `Lost`.

On wrong-thread rejection, core now calls its existing surface discard path
before returning the error. If the device is already lost, it uses the safe
release path to drop the acquired texture. The regression test now observes
the expected validation error, reacquires a frame on the owner thread and
presents it successfully. It also retains the existing successful render,
present, resize and wrong-thread acquisition checks.

Verification used Rust 1.93.1, Visual Studio 2022 x64 and strict hardware
mode on the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested/actual FL9_1:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_hwnd_surface_presents_on_its_window_thread -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/present.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

## Follow-up — query-gated FL9_1 comparison border addressing — 2026-11-07

Rechecked the D3D9 shadow exception and extended the optional FL9_1 shadow
query path through public core validation. When the selected device positively
reports `D3D11_FEATURE_D3D9_SHADOW_SUPPORT`, the adapter now exposes
`ADDRESS_MODE_CLAMP_TO_BORDER` and a new
`DownlevelLimits::border_addressing_requires_comparison_sampler` restriction.
Core rejects ordinary border samplers; the private HAL maps border mode only
for the supported query-gated `LessEqual` comparison sampler. Clamp-to-zero
remains unexposed, and an unavailable/failed query does not enable the feature.

The strict public RTX 3090 Ti tests explicitly requested and confirmed FL9_1
(`0x10de:0x2203`, NVIDIA driver `32.0.16.2101`, physical adapter). Sampling
outside a D16 view with opaque black returned failing comparisons at all
64 pixels; opaque white passed all 64. Core rejected ordinary border use and
`SamplerBorderColor::Zero`. This confirms the tested driver's selected-level
path, not historical FL9_1 behavior.

Synthetic tests cover the positive optional-query projection, unavailable
query behavior, comparison-only border compatibility, monotonic downlevel
limit matching, WebGPU-compliance rejection and old serialized limits
defaulting the new restriction to false. The focused strict border tests passed
2/2. With `WGPU_DX11_REQUIRE_HARDWARE=1`, the full private HAL suite passed
391/391 and the full public D3D11 suite passed 69/69; the public suite has one
visible optional Float16x2 vertex-input subcase skip because the device query
is negative. The full core DX11 unit target passed 95/95, adapter-isolation
and selected-device fixture checks passed 1/1 each, and DX12 core passed
94/94. The core fixture was updated to derive its expected optional feature
set from the selected-device projection after correctly detecting the newly
reported border feature.

`wgpu-types` tests with serde passed 93/93, and workspace checks for
`wgpu-test` and `wgpu-info` all-targets succeeded. HAL, core fixture, public
test and `wgpu-types` Clippy checks succeeded. Remaining warnings are existing:
HAL `create_hal_view` argument count, three unrelated core fixture lints and
six older unnecessary qualifications in `wgpu-types` tests. Public-test
Clippy has no warning from the new test. `cargo +1.93 fmt --all -- --check`
still reports previously existing formatting drift in Naga and earlier 3D
texture/capability fixture code; no newly added border code was listed.
`git diff --check` passed. This optional sampler path does not materially
change overall readiness: the opt-in adapter remains a constrained workload
subset, with broad API auditing, historical hardware, Lumia/CoreWindow runtime
and physical device-removal recovery still outstanding.

Verification used the pinned `+1.93` toolchain, Visual Studio 2022 x64
developer environment, offline dependencies and bounded `-j16` parallelism.
The full suites and checks were:

- `cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 -- --test-threads=1`
- `cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1`
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std -- --test-threads=1`
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,wgsl,std`
- `cargo +1.93 test --offline -j16 -p wgpu-types --features serde --lib`
- `cargo +1.93 check --offline -j16 -p wgpu-test -p wgpu-info --all-targets`
- `cargo +1.93 fmt --all -- --check`
- `git diff --check`

Results: focused public HWND lifecycle 1/1; full strict public D3D11 62/62;
controlled core fixture 101/101; DX12 core regression 92/92; both Clippy
commands passed. The full public run visibly skipped only the optional
Float16x2 IA branch after the selected device reported no support. Clippy
reported pre-existing HAL and core-fixture warnings. Rustfmt reported three
older formatting differences elsewhere in `dx11_public.rs`, not in this
change; the full-workspace format check also reports older unrelated
differences. `git diff --check` passed. The broader mission remains
**PARTIALLY COMPLETE**; historical FL9_1 driver and Lumia runtime behavior
remain unverified, and the core/HAL audit continues.

## Public unimplemented-feature rejection boundary — 2026-11-07

An independent core/HAL contract audit found that pipeline-cache and ray-query
handles are unavailable in the D3D11 HAL (`dx11::Api::Unavailable`), and these
features are not projected by the selected adapter. Added
`public_dx11_rejects_unimplemented_hal_features_at_fl91` to preserve that
boundary: on the selected physical FL9_1 adapter it verifies both features are
withheld and each `request_device` fails before opening the HAL. The focused
test passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), requested/actual FL9_1. This case tests feature rejection,
not rendering.

The full strict public test binary passed 62/62 on the same physical adapter;
one optional Float16x2 subcase visibly skipped after its capability query. The
new test itself also ran in the full suite. Clippy passed with the existing
HAL `too_many_arguments` warning. `cargo +1.93 fmt --all -- --check` continues
to report pre-existing formatting differences in other D3D11/core/Naga files;
the new test line was formatted to rustfmt's expected form. `git diff --check`
passed. The broader mission remains **PARTIALLY COMPLETE**; this closes a
specific feature-exposure regression gap only.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_unimplemented_hal_features_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
cargo +1.93 fmt --all -- --check
git diff --check
~~~

## Public zero-stride indexed-fetch verification — 2026-11-07

Implementation/test commit: `b0774e855de10e1d96c5345c23898d1f8585c099`
(`dx11: verify zero-stride indexed fetch through wgpu`).

Added `public_dx11_indexed_zero_stride_negative_base_vertex_at_fl91`, a
public wgpu → wgpu-core → wgpu-hal → D3D11 test. It creates a point-list
pipeline with a zero-stride vertex-rate position and an instance-rate offset,
then draws with base vertex `-1` using Uint16 and Uint32 indices. Each output
image is read back and all 64 pixels are checked; the expected point appears
over the blue clear in both cases.

The strict run used Rust 1.93.1 and Visual Studio 2022 x64:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_zero_stride_negative_base_vertex_at_fl91 -- --nocapture --test-threads=1
~~~

The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false)
was explicitly selected at requested/actual FL9_1. The focused test passed
1/1 and validated 128 pixels across the two index formats. The complete strict
public D3D11 test binary then passed 61/61; its optional Float16x2 input-format
subcase visibly skipped after a negative selected-device query. No test
silently substituted software rendering. Public-test Clippy passed with one
previously recorded HAL `too_many_arguments` warning. The targeted rustfmt
check still reports three older formatting differences elsewhere in the large
test file; it reported none in this addition. `git diff --check` passed. No
production behavior changed. This validates the zero-stride point case
through the public API, but not other shader features or historical FL9_1
drivers.

Additional validation commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

## Indexed zero-stride semantics audit — 2026-11-07

Resolved the previously documented open question for the backend's exposed
shader subset. The current WebGPU vertex-fetch algorithm resolves each index
with `baseVertex`, then computes an attribute address as the binding offset
plus `vertexIndex * arrayStride`. When every vertex-rate stream has zero
stride, a negative resolved index still references the same in-range attribute
element. The HAL avoids rebasing those index values for its indexed-point
emulation, while preserving the requested point count and instance behavior.

The existing `fl91_indexed_points_order_instances_and_lifetime` hardware test
already checks the observable result: Uint16 and Uint32 indexed point draws
use extreme indices and `i32::MIN` base vertex with zero-stride vertex-rate
streams, and verify the visible point pixels in all 64 target pixels. It was
rerun with strict hardware mode and passed on the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, software=false), explicitly requested and selected
at FL9_1. One hardware test executed and passed; no tests were skipped.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_indexed_points_order_instances_and_lifetime -- --nocapture --test-threads=1
~~~

This closes the implementation/test gap for the currently supported point
path; it does not establish behavior on historical FL9_1 drivers, or enable
`vertex_index`. The three old status notes describing this case as unresolved
were updated to reflect the specification algorithm and test evidence.

## D3D11 debug-layer checkpoint — 2026-11-07

The focused private FL9_1 indexed-render/readback test can now opt into the
D3D11 debug device using `WGPU_DX11_DEBUG_LAYER=1`. With Windows Graphics Tools
present, the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) created the
explicitly requested FL9_1 device and executed the existing indexed draw and
GPU-generated-index coverage. Readback matched the expected 64/64 red pixels
and all secondary index-window outputs. The info queue contained zero
error/corruption diagnostics. After clearing context state and issuing
`ReportLiveDeviceObjects`, the detailed report contained only the expected
device and immediate-context objects; no live D3D11 buffers, textures, views,
shaders or other resources remained.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 developer
environment:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:WGPU_DX11_DEBUG_LAYER = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 fl9_1_hardware_indexed_triangle -- --nocapture --test-threads=1
~~~

Result: one test executed, one passed, zero failed and zero skipped. This is a
focused private-HAL lifetime/diagnostic check, not a debug-layer run of the
complete public suite. Ordinary device creation remains unchanged; the debug
layer is used only by the explicit test path. `TESTING.md` documents the
repeatable command. A subsequent strict full private-HAL run with
`WGPU_DX11_REQUIRE_HARDWARE=1` passed 377/377; hardware-dependent tests used
the physical RTX without a WARP fallback. Production HAL compilation passed.
HAL Clippy passed with ten previously existing test/helper warnings; no new
warning was introduced. `device.rs` passed targeted rustfmt; the
`hal_encoder.rs` check still reports one pre-existing reflow at line 494,
outside this change. `git diff --check` passed.

## Current validation checkpoint — 2026-11-07

The current tested tree was `6be14b53f` (`docs: record FL9_1 volume copy
audit`). With `WGPU_DX11_REQUIRE_HARDWARE=1`, the complete private HAL suite
passed 377/377 and the public `wgpu` D3D11 suite passed 60/60 on the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), explicitly requested and selected at FL9_1. The public suite
reported its existing optional Float16x2 vertex-input subcase as a capability
skip; it is not counted as a GPU execution. The controlled core fixture passed
101/101, physical adapter selection passed 1/1, and the DX12 core regression
passed 92/92. The public offscreen example completed through wgpu and read back
1,024 pixels: 112 red, 277 green and 635 blue-background pixels.

An ARM32 UWP HAL check and builds of both public D3D11 examples passed for
`thumbv7a-uwp-windows-msvc` using installed nightly Cargo/Rust 1.95.0 with
`build-std`; Rust 1.93.1 stable Cargo rejected `-Z build-std` before starting.
This is compile/link coverage only. The Lumia Device Portal request to
`/api/os/devicefamily` timed out, so device execution remains unverified.

The selected-device FL9_1 format probe found no additional float-texture
candidate: R16_FLOAT, R16G16_FLOAT and R16G16B16A16_FLOAT returned `E_FAIL`;
R32_FLOAT and R32G32B32A32_FLOAT reported only `IA_VERTEX_BUFFER` (`0x2`).
These formats remain unexposed. HAL, core-fixture and public Clippy passed with
the existing warnings; `git diff --check` passed. The mission remains
**PARTIALLY COMPLETE**: the tested public subset is usable, but broad
core/HAL-contract closure, historical FL9_1 hardware and Lumia runtime are
outstanding.

Verification commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
cargo +1.93 clippy --offline -j32 -p wgpu --tests --no-default-features --features dx11,wgsl,std
cargo +nightly -Z build-std=std,panic_unwind,panic_abort check --offline -j32 -p wgpu-hal --target thumbv7a-uwp-windows-msvc --no-default-features --features dx11
cargo +nightly -Z build-std=std,panic_unwind,panic_abort build --offline -j32 -p wgpu --target thumbv7a-uwp-windows-msvc --no-default-features --features dx11,wgsl,std --examples
~~~

## Current position — 2026-11-06

Latest code checkpoint: `47a0bce53` (`core: preserve pass stencil reference
in render bundles`). The strict public D3D11 suite passed 60/60; the private
HAL suite passed 376/376, controlled core fixture 100/100, physical adapter
test 1/1 and DX12 core regression 92/92 at the preceding HAL checkpoint
`f04b4f93e`. The strict D3D11 tests selected the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false) at requested and
actual FL9_1. The public suite reported one optional Float16x2 input-format
subcase skipped because its format query was negative. Continue the public
core/HAL contract audit; the mission is not complete.

## Checkpoint — render-bundle pass state — `4960d50f3`

Added `public_dx11_render_bundle_preserves_pass_blend_and_scissor_state_at_fl91`.
It records one bundle, executes it twice in the same pass, and changes the
pass-level scissor and blend constant between executions. GPU readback
verified all 64 pixels: the left half was red and the right half green. The
bundle's vertex buffer was dropped before submission. This covers preservation
of pass-level state while bundle execution resets bundle-controlled state.

Verification used Rust 1.93.1, Visual Studio 2022 Build Tools x64, and strict
hardware mode:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_preserves_pass_blend_and_scissor_state_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

The focused test passed 1/1 on the physical RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested and
actual FL9_1. The full strict public suite passed 59/59. Its Float16x2 vertex
input subcase visibly skipped after the selected FL9_1 format query returned
false. Wgpu Clippy passed with the pre-existing HAL too-many-arguments
warning. Rustfmt still reports pre-existing formatting differences at other
locations in `dx11_public.rs`; the new test formatted cleanly. `git diff
--check` passed. This closes a bundle-state coverage gap, not the public
core/HAL contract audit.

## Core contract fix — stencil references in render bundles — `47a0bce53`

A strict public test exposed a core/HAL mismatch. Core forwarded
`set_stencil_reference` for direct draws, but `RenderBundle::execute` replayed
bundle pipeline changes directly through HAL without restoring the render
pass's current dynamic reference. Bundle draws therefore wrote stencil zero
even when the pass had set reference 1 or 2. Render-bundle replay now reapplies
the pass reference immediately after each bundle pipeline that uses stencil
reference state. The test executes one bundle twice with different references,
then compares stencil in a later pass; all 64 output pixels are red/green as
expected on forced FL9_1.

Verification used Rust 1.93.1, Visual Studio 2022 Build Tools x64, and strict
hardware mode:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_preserves_pass_stencil_reference_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/command/bundle.rs wgpu-core/src/command/render.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The focused hardware case passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, software=false), requested and actual FL9_1. The full
public suite passed 60/60; the optional Float16x2 IA subcase remained a
recognizable capability skip. Core fixture tests passed 100/100 and the DX12
core suite passed 92/92. Both Clippy commands passed with three existing
fixture warnings and the existing HAL too-many-arguments warning. Rustfmt
showed only the known older reflows elsewhere in `dx11_public.rs`; modified
Core files and the new test region are formatted. `git diff --check` passed.
This closes one reachable public contract mismatch; it does not close the
broader capability audit.

The read-only Lumia Device Portal probe for `/api/os/devicefamily` timed out
from this machine. No device state was changed. Windows 10 Mobile ARM32
runtime verification remains unavailable from this session; the x64 forced-
FL9_1 RTX results do not substitute for it.

## Checkpoint — enforce CoreWindow dispatcher-thread ownership — `f04b4f93e`

`Surface::from_core_window` now requires `CoreWindow.Dispatcher().HasThreadAccess`
and captures the creating thread. Later configure, acquire and present
operations must run on that owner thread, matching the existing HWND
thread-affinity boundary. The CoreWindow surface is still unavailable through
public surface capabilities; this change does not claim UWP runtime support.

Verification used Rust 1.93.1 and Visual Studio 2022 Build Tools:

~~~powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 core_window_owner_thread_check_rejects_other_threads -- --test-threads=1 --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
$env:RUSTC_BOOTSTRAP = '1'
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=arm -host_arch=x64 -winsdk=10.0.22621.0 && cargo +1.93 -Z build-std=std,panic_unwind,panic_abort check --offline -j16 -p wgpu-hal --target thumbv7a-uwp-windows-msvc --no-default-features --features dx11'
~~~

The focused thread-identity test passed 1/1. The strict HAL suite passed
376/376 and the strict public D3D11 suite passed 54/54; the hardware paths
reported the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested
and actual FL9_1, software=false. One optional Float16x2 input-format
subcase explicitly skipped due to a negative format query. HAL Clippy passed
with ten existing warnings in unrelated test/helper code. The HAL also
cross-compiled for `thumbv7a-uwp-windows-msvc` using Rust 1.93.1,
`RUSTC_BOOTSTRAP=1`, the ARM MSVC environment and Windows SDK 10.0.22621.0;
this is compile coverage, not Lumia execution. The strict core fixture passed
100/100, physical FL9_1 adapter enumeration passed 1/1, and DX12 core passed
92/92. The adapter test enumerated one physical FL9_1 device.
`git diff --check` passed.

## Checkpoint — multi-group render-bundle transitions — `7bc652409`

Added a strict public wgpu test that records two render bundles using the same
pipeline and separate bind groups. Group 0 contains a sparse uniform at binding
17; group 1 contains a sampled texture and sampler at bindings 7 and 42. Each
bundle uses different uniform and texture data, and the source groups, buffers,
textures, views, samplers and vertex buffer are dropped after bundle
recording. The bundles execute in order in one render pass. Readback verifies
32 left pixels as `[100, 60, 20, 255]` and 32 right pixels as
`[30, 90, 40, 255]`, so stale data or failure to rebind either group is
visible.

Verification used the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), software=false, requested and actual FL9_1:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_transitions_multiple_bind_groups_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

The focused hardware test passed 1/1 with all 64 pixels matching. The full
strict public suite passed 55/55 with one optional Float16x2 input-format
subcase skipped after the selected device reported no IA support. Wgpu Clippy
passed with one pre-existing `wgpu-hal` argument-count warning. Rustfmt reports
three pre-existing formatting differences elsewhere in `dx11_public.rs`; the
new test itself is formatted. `git diff --check` passed.

## Checkpoint — dynamic uniform offsets inside render bundles — `b934c891a`

Added a public strict GPU test that records two bundles with one pipeline and
one dynamic uniform binding at sparse group-0 binding 17. The bundles select
offsets 0 and 256 from one buffer, draw separate halves of the target, and then
execute after the source bind group, uniform buffer and vertex buffer are
dropped. The focused run passed on the physical RTX 3090 Ti at requested and
actual FL9_1; all 64 pixels matched exact red/green output. An initial
assertion used a 0.5 channel and exposed the driver's exact conversion as 127;
the test now uses endpoint colors so it verifies offset selection without
depending on float-to-unorm tie rounding.

Verification used Rust 1.93.1 and Visual Studio 2022 x64:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_dynamic_uniform_offsets_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

The focused test passed 1/1 and the full strict public test binary passed
56/56. One optional Float16x2 input-format subcase printed SKIP after a
negative format query. Clippy passed with one existing HAL argument-count
warning. Rustfmt reports three pre-existing differences elsewhere in
`dx11_public.rs`; the new test is formatted. `git diff --check` passed.

## Checkpoint — depth attachments in render bundles — `735195ef0`

Added a strict public GPU test that executes near-red and far-green draws from
two render bundles against one writable D16 depth attachment. The bundle
descriptor marks depth writable and the absent stencil aspect read-only,
matching the render pass. The near draw writes depth; the far draw fails the
Less comparison. Readback confirmed all 64 pixels remained exact red after the
source vertex buffer was dropped. This covers bundle/attachment compatibility
and depth state through the public core-to-HAL path.

Verification used Rust 1.93.1 and Visual Studio 2022 x64:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_depth_attachment_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

The focused hardware test passed 1/1 and the full strict public test binary
passed 58/58. The optional Float16x2 format subcase printed SKIP after the
device reported no IA support. Clippy passed with one existing HAL
argument-count warning. Rustfmt reports three pre-existing differences
elsewhere in `dx11_public.rs`; the new test is formatted. `git diff --check`
passed.

## Checkpoint — render-bundle pipeline/layout transition — `c4ae7bfac`

Added a strict public FL9_1 test that records consecutive bundles with
different inferred pipeline layouts: the first uses a sparse fragment uniform
at group 0/binding 17, and the second switches to a sampled texture and
sampler at group 0/bindings 7/42. Both draw separate halves of the same target.
The bind groups, buffer, texture, view, sampler and vertex buffer are dropped
after recording. The RTX 3090 Ti at requested/actual FL9_1 rendered 32 exact
red pixels and 32 exact green pixels.

Verification used Rust 1.93.1 and Visual Studio 2022 x64:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_pipeline_layout_transition_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

The focused test passed 1/1 and the full strict public test binary passed
57/57. One optional Float16x2 subcase visibly skipped after a negative input
format query. Clippy passed with one existing HAL argument-count warning.
Rustfmt reports three pre-existing differences elsewhere in
`dx11_public.rs`; the new test is formatted. `git diff --check` passed.

## Verified hardware boundary — ordinary 2D texture arrays

Microsoft's Direct3D feature-level capability table marks texture resource
arrays unavailable at FL9_1, FL9_2 and FL9_3. This is a hardware/API-level
restriction, not a missing FL9_1 backend path; the supported six-face cube
resource is a distinct shape. The opt-in public projection keeps
`TEXTURE_2D_ARRAYS` clear, and the strict cube test requests the physical FL9_1
device, checks the flag and verifies that a two-layer 2D texture is rejected
by core before HAL. The strict public suite separately rejects a `D2Array`
sampled binding layout before HAL. The cube test also rendered and read back
the four-mip cube pattern: 960/960 pixels matched on the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested/actual
FL9_1.

Verification used Rust 1.93.1 and the Visual Studio 18 x64 developer prompt:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_cube_texture_sampling_at_fl91 -- --nocapture --test-threads=1
~~~

The focused strict test passed 1/1. Microsoft reference: [feature-level
capability table](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro).

## Public backend documentation correction

The `Backend::Dx11` and `Backends::DX11` docs, backend-name parser docs, HAL
feature docs and native-probe output still claimed D3D11 had no usable adapter
or rendering. Updated them to describe the existing experimental, restricted
FL9_1 implementation and its required `dx11` Cargo feature. Core's diagnostic
now reflects that the public path includes a constrained owner-thread HWND
surface as well as offscreen rendering. The changelog's summary now reflects
the surface support, and the HAL README records the inspected frame capture.
These documentation and diagnostic changes do not alter backend behavior.

The affected wgpu-types, HAL and core crates compiled as dependencies of the
strict core adapter test. The test passed 1/1 and enumerated one physical
FL9_1 adapter. This example check passed:

~~~powershell
cargo +1.93 check --offline -j8 -p wgpu-hal --no-default-features --features dx11 --example dx11_probe
~~~

Changed Rust files passed the installed rustfmt check; the available pinned
Cargo executable reports Rust 1.93.1, while the direct `rustfmt +1.93`
invocation resolves to the installed stable rustfmt.

## Verified boundary — mutable format views unavailable at FL9_1 — uncommitted

Audited whether the public `VIEW_FORMATS` downlevel flag could be truthfully
enabled for compatible RGBA8/BGRA8 UNORM and sRGB view pairs. Microsoft's
10Level9 FL9_1 format table lists both typeless resource formats as
unavailable, although the corresponding typed formats are individually
supported. A native typeless `CreateTexture2D` probe on the physical RTX 3090
Ti forced to FL9_1 returned `E_INVALIDARG` (`0x80070057`) for both format
families. No HAL typeless-view implementation was retained. A strict public
test instead confirms `VIEW_FORMATS` stays absent and core rejects both
otherwise-compatible sRGB view lists before HAL. This validation test did not
submit rendering commands; existing standalone typed sRGB texture coverage is
unchanged. The result matches the documented FL9_1 format contract and avoids
claiming a capability that this feature level does not provide.

Exact strict command:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_mutable_format_views_at_fl91 -- --nocapture --test-threads=1
~~~

The focused test passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false) with requested/actual FL9_1. The complete
strict public D3D11 test binary then passed 52/52; its existing Float16x2
vertex-input subcase visibly skipped because the selected device did not
report `IA_VERTEX_BUFFER`. The mutable-view case itself performs public device
creation and core validation, but no render submission. Public-test Clippy and
`git diff --check` passed. `cargo +1.93 fmt --all -- --check` remains blocked by
pre-existing formatting differences in Naga's HLSL writer and an older section
of `wgpu/tests/dx11_public.rs`; the new test has no rustfmt diff. This boundary
is verified, documented and retained as an uncommitted working-tree change.

## Capability audit — blendability requires attachment support — uncommitted

The optional-format projection independently considered an implemented
attachment path and the selected `BLENDABLE` query. That could report
`COLOR_ATTACHMENT_BLEND` when the query omitted `RENDER_TARGET`, even though
the same projection correctly withheld `COLOR_ATTACHMENT`. Blendability now
requires the already projected `COLOR_ATTACHMENT` capability, so the selected
device query and backend implementation must agree. The synthetic RGB565 test
removes only `RENDER_TARGET` from a query that retains `BLENDABLE`, checks both
projected flags are absent, and retains the positive case where both bits are
present. R8 and RG8 tests also cover their queried attachment and blendable
paths.

Verification used Rust 1.93.1, Visual Studio x64 and
`CARGO_TARGET_DIR=E:\CodexTemp\wgpu-fl91-mission-target`:

- Capability tests filtered by `color_attachment`: 2/2 passed, covering the
  query-gated RG8 and RGB565 paths. RGB565 explicitly retains `BLENDABLE` while
  removing `RENDER_TARGET`; both projected flags remain absent. A separate
  synthetic case reports both query bits while disabling the backend
  attachment implementation, and also projects neither flag.
- Complete strict private D3D11 HAL suite: 370/370 passed. GPU cases forced
  the RTX 3090 Ti to FL9_1; expected unsupported-operation diagnostics and
  explicit nested skip branches were not counted as GPU execution.
- Complete strict public D3D11 test binary: 52/52 passed at requested/actual
  FL9_1. The existing Float16x2 vertex-input subcase visibly skipped because
  `IA_VERTEX_BUFFER` was unavailable for `R16G16_FLOAT`.
- Core `requesting_dx11_exposes_only_fl91_hardware_adapters`: 1/1 passed;
  two physical FL9_1 adapters were enumerated. No software/WARP adapter was
  exposed.
- HAL Clippy passed with the existing `create_hal_view` too-many-arguments
  warning. `rustfmt --check` passed for `capabilities.rs`; `git diff --check`
  passed. Workspace-wide fmt remains affected by the previously recorded
  Naga HLSL writer and legacy test formatting differences.

Exact commands (run from the Visual Studio x64 developer environment, with
the E: temporary Cargo target):

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib color_attachment -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib
rustfmt --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs
git diff --check
~~~

Continue cross-checking optional format projection and audit public-valid
core/HAL operation boundaries. The modern RTX does not establish behavior on
historical FL9_1 drivers or Windows Mobile ARM32.

## Queue and event-query failure handling — synthetic HRESULTs — uncommitted

Added a test-only native command that returns `DXGI_ERROR_DEVICE_REMOVED`
during command replay, then exercised it through the real core command-buffer
and queue-submit path on a selected physical adapter opened at requested and
actual FL9_1. Core returned `DeviceError::Lost`, invoked the device-lost
callback once with `Unknown`, rejected a subsequent submission, and reported
the queue empty. This validates classification and core invalidation after a
HAL submission error; it does not simulate physical GPU removal, driver
recovery, or continued device operation.

Verification (Rust 1.93.1, Visual Studio x64, `CARGO_TARGET_DIR` on E:):

- The focused strict test passed 1/1 on the RTX 3090 Ti
  (`0x10de:0x2203`, driver `32.0.16.2101`, software=false). The injected
  removal is synthetic; no physical device loss occurred.
- The full strict `wgpu-core` fixture suite passed 97/97. GPU fixture cases
  requested and confirmed FL9_1; the loss test itself validates core error
  propagation and does not count as GPU rendering work.
- The focused strict HAL fence test passed 1/1. It injected one `E_FAIL` from
  `GetData`, confirmed the pending event was retained with its signal value,
  then observed the same event complete through the real D3D11 context and
  fence wait.
- The complete strict private D3D11 HAL suite passed 371/371. GPU tests
  selected the physical RTX 3090 Ti at requested/actual FL9_1; expected
  unsupported-shader cases that print explicit skips were not GPU executions.
- Core Clippy and changed-file Rustfmt passed. Core Clippy has one pre-existing
  warning in an untouched fixture section. HAL `clippy --tests` passed with
  ten existing warnings in old test/fixture code (single-range collection,
  items after a test module, needless borrow) and the existing
  `create_hal_view` argument-count warning; no warning points at the new fence
  code.
- `git diff --check` passed. Workspace `cargo fmt --all -- --check` remains
  blocked by previously recorded formatting differences in Naga's HLSL writer
  and an older public-test block.

Exact strict commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_submission_device_loss_is_propagated_once -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib event_poll_error_retains_pending_query -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --tests
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --tests
rustfmt --edition 2021 --check wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-core/src/dx11_fixture_tests.rs
rustfmt --edition 2021 --check wgpu-hal/src/dx11/fence.rs
git diff --check
~~~

Continue the audit of public-valid resource, render-pass, mapping and device
error paths. The injected queue replay loss and event `GetData` failure do not
simulate physical device removal or driver recovery.

## Public primitive-index feature boundary — uncommitted

The FL9_1 HAL rejects fragment `primitive_index` shader input, but WGSL only
allows it when the optional public `PRIMITIVE_INDEX` feature is requested. The
selected D3D11 capability projection exposes no optional features. A strict
public regression now checks the physical adapter's feature set omits
`PRIMITIVE_INDEX` and that `request_device` rejects a request for it before
creating a device. This confirms the apparent shader mismatch is unreachable
through the current public contract; it adds no rendering or primitive-index
support.

The focused test passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), explicitly requested/actual FL9_1. It is a
public feature-validation test and performs no GPU rendering. The full strict
public D3D11 suite passed 53/53; its Float16x2 IA subcase remained an explicit
query-based skip. Public-test Clippy passed with the existing HAL
`create_hal_view` warning. Rustfmt check reports only the pre-existing legacy
indentation block at `dx11_public.rs:6633`; the new test block is formatted.
`git diff --check` passed. The exact command was:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_unavailable_primitive_index_feature -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
~~~

Continue the public capability and core/HAL audit; feature-gate verification
does not make primitive-index shader execution available.

## Initial checkpoint — gate base-clamp sampling for 10Level9 — superseded

The initial compiler investigation added `DownlevelLimits::texture_sample_base_clamp_to_edge`
and wired it into selected-entry core validation. Naga IR marks
`textureSampleBaseClampToEdge` as `ImageSample { clamp_to_edge: true }`; core
tracks that operation through helper calls and rejects it before HAL only for
the selected vertex or fragment entry point. The FL9_1 projection derives the
limit conservatively from both hardware and backend texture-query and explicit
LOD support. At that checkpoint it was unavailable in the backend. The later
verified implementation is recorded in the current base-clamp entry below.
The original public test covered direct/helper errors and a safe sibling;
that direct rejection expectation was updated when the narrow compiler
emulation landed.

Verification (Rust 1.93.1, Visual Studio 2022 x64 developer prompt, Cargo
`-j16`): targeted wgpu-types (default and serde), selected-entry core, FXC,
strict public D3D11, strict core fixture and full private HAL suites passed;
the latter executed forced-FL9_1 GPU tests on the RTX 3090 Ti. Totals were
types 75/75 (serde 77/77), core fixture 87/87, DX12 core 83/83, public D3D11
13/13 and private HAL 344/344. The public and HAL suites selected the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
`software=false`) at requested and actual FL9_1. Targeted HAL and core
Clippy passed with nine and three pre-existing warnings respectively; wgpu
and serde-enabled types Clippy passed without warnings. HAL `dx11,gles` check,
changed-file rustfmt and `git diff --check` passed. No hardware test skipped.

The limit was initially additive to public `DownlevelLimits`; the final
implementation reports only the narrower proof-specific field, leaving the
general limit false. See `CAPABILITIES.md` for the compiler evidence and
`TESTING.md` for reproduction.

## Checkpoint — indexed strips without vertex-rate inputs — `3b1d4f964`

Extended the no-vertex-rate indexed fast path to LineStrip and TriangleStrip.
At command replay the HAL copies the authoritative index buffer to its retained
staging buffer, maps the requested range, applies signed base-vertex restart
matching, splits runs, and issues bounded nonindexed strip draws. This avoids
relying on native FL9_1 strip-cut behavior. The path also covers instance-only
layouts by rebinding the instance stream for each emulated instance. Existing
vertex-rate indexed strip replay is unchanged. `vertex_index` and
`instance_index` remain unsupported.

Strict tests compare Uint16/Uint32 indexed draws with direct draws for both
strip topologies, instance-only and empty input layouts, nonzero first index,
and signed-base restart markers. The CPU planner test verifies exact restart
run lengths and overlap at bounded draw limits. Independent review found no
correctness blocker; because no vertex-rate input or vertex index can vary
vertex output, GPU image equality is necessarily degenerate, so exact run
planning is separately asserted on CPU. Additional coverage for nonzero index
binding offsets and in-submission index updates would strengthen the test.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 developer prompt
with Cargo `-j16`. The initial public test launch outside that prompt lacked
`link.exe`; rerunning from the documented MSVC environment succeeded. Results:

- Strict private D3D11 HAL suite: 343 passed, 0 failed, 0 ignored. Hardware
  cases selected the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
  driver `32.0.16.2101`, `software=false`) at requested and actual FL9_1.
- Strict public `wgpu` D3D11 integration suite: 13 passed, 0 failed,
  0 ignored on the same forced-FL9_1 device.
- Strict D3D11 core fixture suite: 86 passed, 0 failed, 0 ignored.
- DX12 core regression suite: 82 passed, 0 failed, 0 ignored.
- HAL Clippy passed with nine pre-existing warnings; no warning from this
  checkpoint. `wgpu` test Clippy passed without warnings.
- Changed-Rust-file rustfmt check and `git diff --check` passed. Workspace-wide
  formatting remains blocked only by the previously recorded untouched Naga
  HLSL writer formatting differences.

Reproduction commands (inside the Visual Studio x64 developer prompt):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
```

Continue the core/HAL contract audit. Public adapter access remains explicit
opt-in and offscreen-only; public presentation and historical FL9_1-driver
compatibility are not verified.

## Checkpoint — internal FL9_1 capability projection — `dfc93078a`

Added an FL9_1-only internal projection from the selected-device capability
model to wgpu limits, alignments, downlevel gates, features and per-format
reports. The controlled core fixture now consumes that projection instead of
injecting a separately fabricated capability set. It forces only the
conditional-NPOT branch needed for deterministic coverage and locally opts into
the aggregate BC feature only for its BC1-specific test. The production
projection withholds the BCn feature because only BC1 is implemented. It is
not connected to ordinary adapter enumeration; the core zero-adapter test
continues to verify that boundary.

The new `DownlevelLimits::texture_1d` and `texture_3d` gates let core reject
unsupported resource shapes even when their numeric dimension limits are
nonzero. Other backends retain their prior behavior through `true` defaults.
The public capability model remains provisional: numeric buffer/resource
limits, supported features and all core-valid operations still need a complete
contract audit before exposing an adapter.

Verification used Rust 1.93.1 (`+1.93`) and the Visual Studio MSVC prompt with
16 Cargo jobs. Its banner printed `Error: Unknown error`, but all compiler and
test commands completed:

- Strict controlled core suite: 78/78 passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and selected FL9_1. The fixture
  exercised uploads, copies, mip views, format gates, multiple rendering
  cases and CPU readback; unsupported 1D/3D shapes failed core validation.
- Strict private D3D11 HAL suite: 337/337 passed at forced FL9_1, with actual
  GPU rendering, transfer, fence and readback tests. Tests that intentionally
  rejected a shader before rendering logged local GPU `SKIP`s; these were not
  counted as executed draws.
- Focused capability projection tests: 4/4 passed, including the physical RTX
  query (`unrestricted_npot=Value(true)`, BC1 query positive) and the selected
  actual feature-level assertion.
- Core DX12 suite: 75/75; explicit ordinary D3D11 zero-adapter test: 1/1.
- `wgpu-types`: 65/65; serde-enabled `wgpu-types`: 66/66.
- HAL `dx11,gles`, `wgpu-info`, and `wgpu-test --tests` checks passed.
- Core fixture, wgpu-types, HAL `dx11,gles`, and wgpu-info Clippy passed.
  Existing warnings remain: 14 unused-serde imports in wgpu-types and 10
  unrelated HAL warnings. No warning was introduced by the projection.
- Targeted rustfmt and `git diff --check` passed.

One initial strict core run exposed that wgpu's color-attachment
bytes-per-sample limit assigns RGBA8 a cost of eight, despite its four-byte
storage size. The projection was corrected from 4 to 8 while retaining a
one-target count; the focused fixture and complete strict core suite then
passed. No failed test remains from that correction.

Reproduction commands (run inside the MSVC developer prompt):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --lib -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --features serde --lib -- --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11,gles
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-types --lib --tests
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 clippy --offline -j16 -p wgpu-info
rustfmt --edition 2021 --check tests/src/params.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/resource.rs wgpu-hal/src/dx11/adapter.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

## Checkpoint — query-gated R8/RG8/RGB565 copy and readback — `877c4e9bc`

Commit `877c4e9bc` adds the narrow-format copy-source paths and variable-width
texture readback. FL9_x GPU-to-CPU copies require an internal native
render-target binding even when the public texture is not a render target. The
HAL now admits `COPY_SRC` for R8_UNORM, R8G8_UNORM and B5G6R5 only when both
the backend path and selected device's `RENDER_TARGET` query are positive. HAL
texture creation adds the native RTV bind for this use. RGB5A1, RGBA4, the
SNORM formats and BC1 remain excluded. The controlled core fixture derives
these usage reports from the HAL projection.

The texture-to-buffer path now uses each format's block layout rather than a
hard-coded four bytes per texel. It checks block alignment and copies mapped
rows using the D3D11 staging texture's actual `RowPitch`. Existing checked
offset, pitch and destination-range handling is retained. A CPU test covers
R8, RG8 and RGB565 layouts with 256-byte rows and a nonzero offset.

The new strict public test explicitly selected the physical NVIDIA RTX 3090
Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested FL9_1,
and confirmed actual FL9_1. At that level, the selected-device masks were
R8=`0x0002d3e0`, RG8=`0x0002c320`, and B5G6R5=`0x0026f3a0`. For each format,
the test uploaded padded 5×3 source rows, performed a partial GPU texture copy
from a nonzero source origin into a differently sized 6×4 destination at a
nonzero destination origin, then read the complete destination into a buffer
at offset 16 with 256-byte rows. It verified all 24 texels (six copied and 18
zero-initialized/preserved) plus padding sentinels. All three formats passed.
The synthetic capability test rejects the copy-source path when the
render-target query is missing or the backend implementation switch is false.

Verification used Rust 1.93.1 in the Visual Studio 2022 x64 developer
environment, with `-j32` and strict hardware mode for GPU suites:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 check --offline -j32 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-hal --lib --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --lib --tests --no-default-features --features dx11-test-fixture,std,wgsl
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs wgpu/tests/dx11_public.rs
git diff --check
```

Results: strict HAL 359/359, public D3D11 26/26, controlled core fixture
1/1 (89 filtered), physical public adapter selection 1/1 (86 filtered), and
core DX12 86/86. The full public suite logged one optional Float16x2 input
format skip because the selected-device query does not report it; no
hardware-required test skipped for missing hardware. The HAL suite contains
expected negative shader cases that reject before GPU execution and log their
GPU-only subcase as skipped. HAL Clippy passed with nine existing unrelated
warnings, core Clippy with one existing warning, and public Clippy without
warnings. Changed-file rustfmt, HAL check and `git diff --check` passed. A
direct Cargo invocation outside the VS developer shell initially failed to
find `link.exe`; rerunning within `VsDevCmd.bat` passed. The first full HAL
run after implementation exposed two stale synthetic expectations; they were
updated to distinguish RGB565's implemented path and the suite then passed.

An independent read-only review found no concrete mismatch in the new path.
It confirmed the FL9_x render-target bind requirement, query/implementation
gating, checked transfer layout and use of native mapped row pitch. Historical
FL9_1 behavior and ARM32 UWP remain unverified. Continue the public core/HAL
format and operation audit.

Continue by auditing whether every projected numeric limit, per-format usage
and accepted core command is implemented by the D3D11 path. Public D3D11
enumeration and public HWND surface exposure remain gated; the forced RTX
result does not validate historical FL9_1 drivers.

## Checkpoint — vertex system-value input gates — `d85e51e7b`

Core now has `DownlevelLimits::vertex_index_input` and
`instance_index_input`. The controlled FL9_1 fixture reports both false and
rejects selected vertex entry points declaring either input before HAL; a
safe sibling entry remains usable. HAL already rejects both inputs. The
existing D3DCompile test confirms backend-generated `vertex_index` entry
interfaces fail under `vs_4_0_level_9_1`, whether the WGSL value is used or
unused. Its diagnostic is FXC X4555 for `SV_VertexID`; this establishes the
current generated-HLSL/profile boundary, not a universal hardware limitation.
The `instance_index` FXC boundary remains unverified and is documented only
as a backend restriction. Normal D3D11 enumeration remains disabled.

Verification used the pinned `1.93` toolchain (rustc 1.93.1) and MSVC. The
explicit `+1.93.1` alias is installed without a rustup manifest, so it failed
before compilation; `+1.93` is the repository-pinned alias. Visual Studio's
developer prompt printed `Error: Unknown error` in its banner, but all MSVC
commands below completed:

- wgpu-types full library suite: 64/64 passed. Clippy passed with 14 existing
  unused-serde-import warnings.
- Fixture-enabled core suite with `WGPU_DX11_REQUIRE_HARDWARE=1`: 78/78
  passed, including the real hardware fixture and zero-adapter isolation.
  The fixture selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
  software=false), requested FL9_1 and selected L9_1. The new gate logged its
  selected-entry rejection and safe sibling acceptance.
- Private D3D11 HAL suite with strict hardware mode: 333/333 passed. The
  focused FXC test separately passed and reported X4555 for direct-used,
  direct-unused, structure-used and structure-unused `vertex_index` inputs.
- HAL zero-adapter isolation test: 1/1 passed.
- Core DX12 regression suite: 75/75 passed with `dx12,std,wgsl`.
- HAL `dx11,gles` check and Clippy passed; fixture-enabled core Clippy passed;
  `wgpu-info` check and `wgpu-test --tests` check passed.
- Targeted rustfmt and `git diff --check` passed.

Cargo ran inside the Visual Studio MSVC developer prompt with
`CARGO_BUILD_JOBS=4` (the full strict fixture used `2`). Reproduction commands:

```powershell
cargo +1.93 test --offline -j 4 -p wgpu-types --lib vertex_builtins_are_downlevel_capabilities -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std vertex_builtin_detection_uses_selected_entry_point_declarations -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_index_entry_inputs_are_rejected_by_fxc -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-hal --lib --no-default-features --features dx11 incomplete_instance_exposes_no_adapters_and_rejects_unsupported_handles -- --nocapture --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j 4 -p wgpu-types --lib -- --test-threads=1
cargo +1.93 check --offline -j 4 -p wgpu-hal --no-default-features --features dx11,gles
cargo +1.93 check --offline -j 4 -p wgpu-info
cargo +1.93 check --offline -j 4 -p wgpu-test --tests
cargo +1.93 clippy --offline -j 4 -p wgpu-types --lib --tests
cargo +1.93 clippy --offline -j 4 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j 4 -p wgpu-hal --no-default-features --features dx11,gles
cargo +1.93 clippy --offline -j 4 -p wgpu-info
cargo +1.93 clippy --offline -j 4 -p wgpu-test --tests
rustfmt --edition 2021 --check tests/src/params.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/pipeline.rs wgpu-core/src/validation.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

`WGPU_DX11_REQUIRE_HARDWARE=1` was set for the full core fixture and HAL
suite. An initial DX12 command omitted `std,wgsl` and failed to compile the
test target; rerunning with those required features passed. No source change
was needed to correct that command issue.

## Documentation checkpoint — repeat-cap behavior reconciled — `e5fe9207c611234c1669da48892ec12b80ebcfab`

Updated `CAPABILITIES.md`, `TESTING.md` and `STATUS.md` to separate the private
HAL's conditional NPOT relationship checks from the core fixture's finite
FL9_1 repeat limit. The fixture rejects Repeat/MirrorRepeat sampler creation
at the documented limit of 128; synthetic core tests still cover selected-
entry NPOT relationship validation where those sampler modes are permitted.
This documentation-only checkpoint records the implementation in
`70e31a0f175803ced62134e10cd3f11cc8553886`.

## Checkpoint — selected fragment `front_facing` gate — `77ac0b78330c4ba330682c89526daa2833f2b1c6`

Core now represents whether fragment entry points may declare
`@builtin(front_facing)` in `DownlevelLimits::fragment_front_facing`. The
controlled FL9_1 profile sets this false because the tested
`ps_4_0_level_9_1` FXC profile rejects the input. Core inspects the selected
fragment entry point and rejects the incompatible pipeline before HAL, while a
safe sibling entry point in the same module remains usable. This is documented
as a compiler/backend limitation, not a fundamental hardware restriction.
Ordinary D3D11 adapter enumeration remains disabled.

Verification used Rust 1.93.1/MSVC and `CARGO_BUILD_JOBS=32`:

- `cargo test --offline -p wgpu-types --lib -- --test-threads=1`: 62/62 passed.
- CPU interface selection test passed; the strict fixture-enabled core suite
  passed 76/76 with `WGPU_DX11_REQUIRE_HARDWARE=1`. The focused fixture log
  identified NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false),
  requested/actual FL9_1, and logged the selected-entry rejection and safe
  sibling-pipeline success. Normal adapter isolation passed in the suite.
- Strict private HAL suite: 333/333 passed. The pre-existing FXC negative test
  confirmed the front-facing shader diagnostic; strict mode prevented hardware
  cases from silently skipping.
- Core DX12 regression: 73/73 passed.
- HAL `dx11,gles`, `wgpu-info --all-targets`, and `wgpu-test --all-targets`
  checks passed. Clippy passed for wgpu-types, fixture-enabled core, HAL
  `dx11,gles`, and wgpu-info.
- Targeted rustfmt and `git diff --check` passed. Workspace-wide
  `cargo fmt --all -- --check` reports pre-existing formatting differences in
  `naga/src/back/hlsl/writer.rs` and `wgpu-core/src/instance.rs`; neither file
  is part of this checkpoint and neither was reformatted.

The Visual Studio setup banner still prints `Error: Unknown error`, but all
MSVC-backed commands completed. This checkpoint narrows an observed shader
profile gap; it does not complete public adapter validation. Continue the
selected-entry shader and render contract audit, then build a capability
snapshot without enabling ordinary enumeration.

## Checkpoint — controlled core fixture honors FL9_1 repeat cap — `70e31a0f175803ced62134e10cd3f11cc8553886`

The controlled core fixtures now derive `max_texture_repeat_per_primitive`
from the selected D3D feature level instead of setting `u32::MAX`. Forced
FL9_1 reports the documented finite limit of 128, and the strict core fixture
verifies that both Repeat and MirrorRepeat samplers fail at creation with
`TextureRepeatLimitExceeded`. Its texture readback case now samples POT and
NPOT resources using clamp-to-edge. Private HAL tests retain bounded repeat
coverage; synthetic core tests retain coverage for conditional NPOT
relationship validation where repeat samplers are permitted. This fixture is
still a reduced test profile, not a public capability snapshot.

Verification used Rust 1.93.1/MSVC and `CARGO_BUILD_JOBS=32`:

- Strict focused core fixture with `--nocapture`: passed on NVIDIA GeForce RTX
  3090 Ti (`0x10de:0x2203`, software=false), requested and actual FL9_1. It
  logged `texture repeat limit: 128`, rejected both sampler modes, and matched
  all 64 pixels in each remaining render/readback case.
- Full strict fixture-enabled core suite: 75/75 passed, including normal
  D3D11 adapter isolation. The physical fixture ran with
  `WGPU_DX11_REQUIRE_HARDWARE=1`.
- HAL and fixture-enabled core Clippy passed. Targeted rustfmt and
  `git diff --check` passed.

The first compile attempt used an unqualified `resource` module in the new
fixture assertion; the path was corrected to `crate::resource` before the
passing runs. The RTX test does not demonstrate the historical repeat
overflow boundary. Public adapter enumeration remains disabled.

## Checkpoint — fractional FL9_1 viewport mapping — `95fb67e22`

The private HAL now keeps the logical fractional viewport values when building
the FL9_1 vertex remap. Its native D3D11 viewport uses integral bounds that
contain the clipped logical viewport, avoiding the 9_x runtime's DWORD-based
viewport conversion while preserving the requested screen transform. The
strict HAL readback test covers fractional x/y origins and extents. The
controlled core fixture also covers a fractional viewport with contained
geometry; it deliberately does not treat a viewport as a scissor, since an
oversized primitive can render outside the viewport rectangle. Core's
out-of-target viewport gate remains unchanged. Historical FL9_1 precision is
not established by the RTX test.

Verification used Rust 1.93.1/MSVC and `CARGO_BUILD_JOBS=32`:

- CPU viewport tests: `cargo test --offline -p wgpu-hal --lib
  --no-default-features --features dx11 fractional_viewport -- --nocapture
  --test-threads=1`: 3/3 passed.
- Strict private HAL test:
  `WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib
  --no-default-features --features dx11 fl91_viewport_fractional_coordinates
  -- --nocapture --test-threads=1`: passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1.
- Full strict private HAL suite: 333/333 passed. Hardware-dependent tests ran
  with strict mode enabled; shader-profile rejection tests validate compiler
  errors rather than GPU output.
- Strict core fixture: 75/75 passed. Its physical fixture test was separately
  run with `--nocapture`, reported the same RTX and requested/actual FL9_1,
  and matched all 64 pixels for `FractionalViewport` and the other render
  cases.
- Normal D3D11 zero-adapter isolation: 1/1 passed. Core DX12 regression suite:
  72/72 passed with `dx12,std,wgsl`.
- HAL and fixture-enabled core Clippy passed. Targeted rustfmt and
  `git diff --check` passed.

The first core viewport test draft incorrectly expected the viewport to clip
an additional oversized triangle; it failed at pixel `(1, 0)`. The fixture was
corrected to use only contained geometry, after which strict core readback
passed. An initial DX12 command omitted the required `wgsl` feature and failed
to compile; the correctly configured run above passed. Visual Studio's setup
script printed `Error: Unknown error` in its banner, but MSVC-backed Cargo
builds and all listed tests completed successfully. Public D3D11 enumeration
and surface exposure remain disabled.

## Checkpoint — core viewport clipping capability — `19ff6dff6`

Core now reports and enforces `DownlevelLimits::viewport_rectangles_outside_target`.
The default remains true for existing backends; the controlled FL9_1 fixture
sets it false. When false, core rejects a finite viewport crossing any active
render-target edge with a validation error before invoking HAL. This avoids
the previous late draw failure when D3D11's optional clipped-viewport shader
variant exceeds the FL9_1 FXC instruction budget. The private HAL remap path is
unchanged and remains available to restricted private callers.

Verification used Rust 1.93.1/MSVC with `CARGO_BUILD_JOBS=32`, one Cargo process
at a time:

- Baseline before the change: existing `wgpu-types` downlevel-limit test passed
  1/1.
- New CPU bounds test: `viewport_target_bounds_cover_all_edges` passed 1/1 and
  checked all four edges plus fully contained and edge-aligned rectangles.
- `cargo test --offline -p wgpu-types --lib -- --test-threads=1`: 60/60 passed.
- Strict fixture-enabled core suite: 75/75 passed. Its physical adapter was
  NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested and
  selected FL9_1. The new out-of-target case was validation-only and issued no
  draw; existing fixture operations performed rendering/readback and matched
  their expected pixels. The test also asserts that normal D3D11 HAL adapter
  enumeration is empty.
- Strict private HAL suite: 326/326 Rust tests passed. The run logged 144
  hardware initialization records on the RTX forced to FL9_1. Six shader
  probes printed `SKIP` after FXC rejected their unsupported shader before
  GPU rendering; those probes are not counted as successful GPU work.
- Core's explicit zero-D3D11-adapter test: 1/1 passed. Core DX12 regression:
  72/72 passed.
- HAL `dx11,gles`, core `dx11,std,wgsl`, `wgpu-info` and `wgpu-test` checks
  passed. Clippy passed for wgpu-types, fixture-enabled core, HAL `dx11,gles`,
  wgpu-info and wgpu-test. HAL emitted ten existing warnings; wgpu-types
  emitted fourteen existing unused-serde-import warnings.
- Targeted rustfmt and `git diff --check` passed. A first core build caught
  missing initializers in the no-op adapter; no-op and GLES now explicitly
  report the default-supported behavior. No test failures remain.

The strict core fixture did not print the driver version during this run; the
last recorded driver baseline is NVIDIA `32.0.16.2101`. This forced-level test
does not establish behavior on historical FL9_1 hardware. The public adapter
and surface gates remain closed.

## Checkpoint — finite texture-repeat limit in core — `d3e7db9a1`

Microsoft's 10Level9 `Draw`/`DrawIndexed` contract limits repeats over one
primitive to 128 at FL9_1 (2048 at 9_2, 8192 at 9_3/10_x, 16384 at 11_x).
The D3D11 feature-level model records those static guarantees. A new
`DownlevelLimits::max_texture_repeat_per_primitive` carries a reported bound;
core rejects `Repeat` and `MirrorRepeat` sampler creation for finite bounds,
because it cannot prove interpolated coordinates remain within the limit for
all possible draw data. This is intentionally stricter than the native bound:
core does not yet have a coordinate-range validator or semantics-preserving
shader emulation. The bound is not an optional driver query. The FL9_1 core
fixture kept its test-only unbounded setting so the prior NPOT draw test could
exercise selected-entry validation. That permissive setup was removed by
`70e31a0f1`; it was never a public capability projection.

The test harness now compares required `DownlevelLimits` when deciding whether
a conformance test can run. The minimum test profile requests no texture-repeat
capacity; a test needing unbounded repeat can request `u32::MAX`. Ordinary
D3D11 adapter enumeration remains disabled.

Validation on Rust 1.93.1 / Windows MSVC with `CARGO_BUILD_JOBS=32`:

- `cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampler_repeat_limit -- --nocapture --test-threads=1`: 1 passed, CPU-only.
- `cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 feature_level_guarantees_are_specific_and_do_not_promote_backend -- --nocapture --test-threads=1`: 1 passed, all seven feature-level identities checked.
- `cargo test --offline -p wgpu-types downlevel_limits_check_minimum_capabilities -- --nocapture`: 1 passed.
- Strict `fl91_core_buffer_and_offscreen_render_on_hardware`: 1 Rust test passed with many readback cases. NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested and actual FL9_1. The physical full-NPOT query was true; synthetic fixture validation forced the conditional path. NPOT/repeat rejection and valid sampled/readback output passed. This does not validate historical repeat overflow behavior.
- Core D3D11 zero-adapter isolation: 1 passed. Core DX12 regression suite: 71 passed.
- Strict private D3D11 HAL suite: 323 passed, 0 failed, including existing texture, sampler, draw, transfer and synchronization GPU tests. No dedicated over-limit GPU experiment was run because the modern driver does not reproduce legacy FL9_1 behavior.
- `cargo check -p wgpu-hal --no-default-features --features dx11,gles`, `cargo check -p wgpu-info`, `cargo check -p wgpu-test`, targeted `rustfmt --check`, and `git diff --check` passed.
- Core and wgpu-test Clippy passed. HAL Clippy passed with 10 pre-existing warnings; wgpu-types Clippy passed with 14 pre-existing unused-serde-import warnings. A first test-package Clippy call lacked the MSVC environment and failed to find `cl.exe`/`link.exe`; rerunning after `VsDevCmd.bat` passed. An early core assertion used `assert_eq!` on a non-`PartialEq` error and was corrected to `matches!` before the passing test.

The fixture did not print a driver version during this run; the prior hardware
baseline recorded NVIDIA driver `32.0.16.2101`. No historical FL9_1 hardware
was tested. See CAPABILITIES.md for the hardware/backend/public-contract split.

Branch: `uwp-dx11`. The autonomous FL9_1 completion mission remains active.
Recent verified code checkpoints include `834cd9856` (Vulkan reports the
shared multi-mip depth capability), `983eea619` (core gates multi-mip depth
attachments), `5fe21f035` (all native stencil
operation outcomes at forced FL9_1), `340147803` (native FL9_1 depth
comparisons), `f99cae113` (general single-target blend translation),
`8eff9020a` (single-sample output masks) and `1eb21620c` (core rejects NPOT
mip chains when the downlevel capability is absent). Ordinary
public D3D11 adapter enumeration and public surface exposure remain disabled.

The private renderer now has extensive forced-FL9_1 coverage for depth/stencil,
texture formats and mip views, static and dynamic uniform ranges, sampled
resources, texture transfers, and indexed/non-indexed point, line and triangle
topologies. The principal completion blockers remain a truthful public adapter
contract, unsupported core-valid descriptor/state combinations, a safe public
HWND presentation design, shader limits, and historical FL9_1 verification.
See CAPABILITIES.md and STATUS.md for the detailed implementation matrix.

## Latest validation

- Rust 1.93.1, MSVC, Windows 11, bounded Cargo jobs (8).
- At `5fe21f035`, the strict focused stencil GPU test passed 1/1 and the full
  strict private HAL suite passed 322/322. The RTX 3090 Ti was requested and
  selected at FL9_1. All eight operations in all three stencil outcomes matched
  1,536 checked output pixels. HAL check, core zero-adapter isolation, HAL
  Clippy, rustfmt and `git diff --check` passed.
- At `8eff9020a`, strict D3D11 HAL tests passed 318/318; the fixture-enabled
  core suite passed 70/70, the core DX12 suite passed 67/67, and the ordinary
  zero-adapter isolation test passed. The RTX 3090 Ti (`0x10de:0x2203`, driver
  `32.0.16.2101`) was requested and selected at FL9_1. A core fixture with a
  zero sample mask preserved all 64 blue clear pixels.
- At `1eb21620c`, the new core CPU dimension test passed 1/1, the strict
  fixture-enabled core suite passed 71/71 and printed the NPOT-mip missing-flag
  validation result, the core DX12 suite passed 68/68, and ordinary D3D11
  adapter isolation passed 1/1. Core Clippy, targeted rustfmt and
  `git diff --check` passed. The DX12-only test configuration emitted existing
  warnings because WGSL is disabled. No HAL code changed for this checkpoint.
- At `983eea619`, the strict controlled core fixture passed 1/1 on the RTX
  3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual
  FL9_1, including early rejection of a two-mip D24S8 render attachment. The
  full fixture-enabled core suite passed 71/71; the strict private HAL suite
  passed 322/322; the ordinary zero-adapter test passed 1/1; the core DX12
  suite passed 68/68; the focused wgpu-types flag test passed 1/1; combined
  D3D11+GLES and Vulkan HAL checks passed; core and Vulkan HAL Clippy passed;
  D3D11+GLES HAL Clippy passed with ten existing warnings; targeted rustfmt
  and `git diff --check` passed.
- No public D3D11 adapter, public HWND render, or historical FL9_1 hardware
  result is available.

## Checkpoint — FL9_1 stencil operation outcome coverage — `5fe21f035`

The existing D24S8 test now exercises each of the eight wgpu stencil
operations in all three native outcome slots: stencil/depth pass, stencil
comparison failure, and depth-test failure. Boundary values distinguish
clamped from wrapping increment/decrement; Replace uses different references
for successful and failed stencil comparisons. A subsequent Equal pipeline
draws blue only when the resulting stencil value matches the expected value.
All 24 cases matched all 64 pixels. The HAL does not expose direct stencil
storage readback, so this is an indirect validation through actual GPU tests.

The focused strict test passed 1/1 on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, software=false), with FL9_1 requested and selected. The full
strict D3D11 HAL suite passed 322/322, including the 24 stencil cases. The HAL
check passed, the ordinary core zero-D3D11-adapter isolation test passed 1/1,
and HAL Clippy completed with ten pre-existing warnings outside the changed
test. Rustfmt and `git diff --check` passed. The first attempt without
initializing the Visual Studio environment failed because `link.exe` was not
on PATH; rerunning after `VsDevCmd.bat` succeeded. Cargo used Rust 1.93.1 and
`CARGO_BUILD_JOBS=8`.

Microsoft defines the eight native operation behaviors in the
[`D3D11_STENCIL_OP` reference](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ne-d3d11-d3d11_stencil_op).
The 10Level9 device table documents the FL9_1 two-sided-stencil restriction;
this backend continues to reject asymmetric front/back states. Modern hardware
forced to FL9_1 does not establish behavior on historical drivers.

Commands, run from an initialized Visual Studio 2022 Build Tools environment
with Rust 1.93.1 and `CARGO_BUILD_JOBS=8`:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_stencil_operations_match_expected_values -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
rustfmt --edition 2021 --check wgpu-hal/tests/dx11/stencil_noop.rs
git diff --check
```

The six existing checked runtime-integer shader subcases still log `SKIP`
after the backend rejects their unsupported integer semantics; those skips are
not part of the stencil execution claim.

Code checkpoint: `5fe21f035` (`dx11: test FL9_1 stencil operation outcomes`).
Continue the core-valid descriptor/command audit before constructing a truthful
public capability snapshot. Keep normal adapter enumeration and public HWND
surface exposure disabled.

## Review findings and decisions

- Mapping creation must consider the selected shader entry point: an unused
  texture can share a logical identity with a layout uniform. A regression test
  now proves the unused declaration cannot cause a false rejection.
- A public adapter must not treat valid core calls as HAL failures: current
  examples include empty compute passes, several legal viewports and blend
  constants. Some mapping errors use
  `expect` and need a device-loss-safe contract audit.
- The FL9_1 device documentation says buffers require exactly one
  vertex/index/constant bind flag. The implementation now gives default
  copy-only and uniform-source buffers an internal vertex bind; logical usages
  remain unchanged. Staging buffers retain zero flags because the general D3D11
  staging/CPU-read contract prohibits pipeline binds. The level-9 table does
  not explicitly spell out the staging exception; legacy runtime behavior
  remains unverified.
- The private D24S8 path queries native support, retains depth views, initializes
  both aspects and supports depth-only clear passes. Depth-only drawing,
  read-only/aspect views and stencil testing remain public integration gates.
- Pixel FXC compilation accepts 32 referenced float4 constants and rejects 33
  in the tested profile. The vertex budget and near-limit GPU execution remain
  unverified; sparse high indices do not measure live constant pressure.

## Next task

The checked multi-uniform GPU case has subsequently been extended to a nested
structure, asymmetric mat3x2 and two-element vec4 array. On the same hardware
the strict `fl91_sparse_uniform_ranges` test passed with pixels
[143,207,16,255] and [80,143,16,255]. This verifies the tested offsets and
stride, not dynamic indexing or every uniform layout. The 126-test private
suite and targeted Clippy passed; the test and mission plan were checkpointed
as `19e82da409dd0ceec12eb9e9dca8934900b5ff11`.

The D24S8 attachment path has zero initialization,
depth-only clears, Always/LessEqual depth tests and optional writes. Strict
forced-FL9_1 RTX 3090 Ti rendering samples a texture and validates all 64 pixels
in each of ten passes, including both depth orders, independent stencil clear,
no-write behavior and retained resources. Stencil operations, aspect views,
read-only usage and broader depth formats remain unsupported. Independent review
identified and prompted rejection of non-ignored stencil faces even with zero
masks. The D3D11 build, strict 127-test private suite, core zero-adapter test
and targeted Clippy passed. Ten pre-existing Clippy warnings remain. The DX12
`wgpu_gpu::buffer::map_offset` hardware regression passed on the NVIDIA
adapter. It was checkpointed as
`424dfaa2f46e757e9d3a0dad9c50b9f110a18592`.

The default-buffer bind-flag portability fix passed its synthetic allocation
test and strict FL9_1 hardware copy/readback (16/16 bytes); the private suite
passed 129 tests, and the core zero-adapter and targeted Clippy checks passed.
The existing DX12 GPU test binary also passed `wgpu_gpu::buffer::map_offset`
on the NVIDIA adapter after generating its temporary GPU configuration. The
fix was checkpointed as `e09825a90b32dc52eb83cd27f084c2803abf96c5`.

Core's `UNRESTRICTED_INDEX_BUFFER` creation check was reproduced failing via a
Noop adapter with only that downlevel flag removed. It accepted `INDEX|VERTEX`
instead of rejecting it. The proposed core fix checks all known non-copy usages;
the targeted public-API test passes for restricted and unrestricted adapters,
including map/query usages, mapped-at-creation and an unrelated non-index
control. The complete 66-test core library suite and 129-test private D3D11
suite passed. Targeted core Clippy passed. This changes validation for all
adapters lacking the flag, including WebGL; the Noop restricted adapter covers
the shared rule but is not a WebGL runtime test. A rebuilt DX12 GPU buffer map
regression passed on the NVIDIA adapter.
The broader validation suite had 86 passes and three pre-existing adapter-error
message mismatches because the fork's DX11 backend is absent from the expected
strings. The suite passed with those three tests filtered. The core fix and its
tests were checkpointed as `8949812c55514e106e7803e00d7e25801e516d44`.
The stale expected strings now include the actual DX11 diagnostic. The full
unfiltered `cargo test --offline -p wgpu-test --test wgpu-validation --
--test-threads=1` suite passed 89/89 tests.
The diagnostic fix was checkpointed as `7bdffefe244101bf7374fbc10044424f08eea8c1`.

The encoder now accepts paired empty compute passes while rejecting dispatch,
timestamps, nesting and transfer commands inside a pass. A strict FL9_1 test
copied 16/16 expected bytes through two GPU copies around an empty compute
pass, retaining source resources until submission. The existing textured depth
test also exercises unused blend constants before and after pipeline selection;
all ten 64-pixel results still match. Blend factors using constants remain
unsupported, and any future implementation must retain the factor across
pipeline changes. The strict private suite passed 130 tests; older tests may
still skip internally. The zero-adapter core test and targeted Clippy passed,
with ten existing Clippy warnings. Independent review found no issue in this
restricted path. Public core execution has not been tested through D3D11.
The encoder change was checkpointed as `26825d96e`.

Viewport dimensions of zero are now accepted. Four strict forced-FL9_1 hardware
tests verified zero width and height preserve all 64 clear pixels, an indexed
draw also preserves them, and restoring the viewport with a half-width scissor
produces 32 clear and 32 drawn pixels. Negative coordinates and non-finite
values remain rejected. The full private suite passed 134 tests before adding
the additional negative viewport checks; all four focused tests passed again
afterward. Targeted Clippy passed with ten existing warnings and the core
zero-adapter test passed. Microsoft documents both zero dimensions and the
FL9_x negative-origin failure in
<https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_viewport>.
Negative origins and rectangles extending beyond the target remain unresolved
public integration blockers; historical FL9 drivers have not been tested.
Zero-sized viewport support was checkpointed as `4afbb21da`.

Core's viewport validation previously accepted NaN position and size values;
the new public Noop regression reproduced acceptance before the fix. Core now
rejects non-finite coordinates and dimensions before the HAL call, with
accurate diagnostics. The test covers NaN and both infinities in all six
fields and checks zero-sized, negative-origin and out-of-target finite controls.
The full public validation suite passed 90 tests before the final additional
controls and diagnostic wording; the focused test passed afterward. The 66
core library tests, targeted core Clippy and the rebuilt DX12
`wgpu_gpu::encoder::drop_encoder_after_error` hardware test passed. Vulkan
loader warnings about a missing OBS layer JSON are pre-existing. This shared
validation fix affects all native wgpu-core backends.
The shared validation fix was checkpointed as `d13062b65`.

Private render passes now accept exactly one load operation and one store
operation per attachment aspect, including don't-care and discard. Discard is
a native no-op, so no discarded contents are promised. Core's later zero
initialization still requires public-path integration testing. The strict
FL9_1 depth test executes independent stencil and depth discards, preserves a
loaded depth aspect, verifies a fully overwritten don't-care color target, and
checks all 64 pixels of ten subsequent results. Malformed operation flags are
rejected in unit and encoder tests. The full private suite passed 135 tests;
after adding the stronger don't-care and encoder-negative checks, the focused
GPU test passed again. The core isolation test and targeted Clippy passed, with
ten existing Clippy warnings.

Independent HWND research identified two presentation blockers: `Present`
thread synchronization and message-pump lock ordering need review, and retained
command-buffer RTV references may prevent `ResizeBuffers` even after a surface
texture is released. A future Win32 surface must address and test both; the
the public adapter gate remains unchanged.
Attachment discard support was checkpointed as `721469cfe`.

The private surface now accepts a borrowed Win32 HWND and creates a BGRA8
flip-sequential swapchain. It uses stretch scaling for HWND, preserving the
existing CoreWindow scaling. After unbinding and dropping a swapchain,
unconfigure flushes the immediate context before a replacement can be created.
The strict hidden-window test executed on the RTX 3090 Ti at forced FL9_1,
submitted a HAL clear, read back BGRA [0,0,255,255], presented with success,
resized and reacquired, then unconfigured and recreated the swapchain. This
does not establish visible desktop presentation or presentation concurrency.
The 136-test strict private suite passed before the final flush and scaling
changes; the focused hardware test passed after them. Targeted Clippy passed
with ten existing warnings and core's zero-adapter isolation test passed.
The test resets its recorded command before resize; live command-buffer RTV
references, concurrent Present/context access, message-pump locking and whether
queue idle covers pending presentation remain public-presentation blockers.
Windows 7 flip-swapchain support is not claimed.
The initial private HWND surface was checkpointed as `bf56c17b9`.

The retained-command-buffer resize concern was reproduced on the same RTX:
`ResizeBuffers` failed with the completed buffer still holding the backbuffer
RTV. Surface-backed render commands now keep a weak per-view reference. Queue
submission validates and temporarily retains all referenced surface views
before replay; ordinary offscreen render targets retain their previous strong
ownership. After GPU completion and view destruction, the hidden HWND test
resizes and recreates the swapchain while the old command buffer remains alive,
then records a new HAL clear and reads back green BGRA [0,255,0,255]. The old
buffer is reset only afterward. A destroyed surface view fails the submission
preflight check; resubmitting a command with destroyed resources is outside the
HAL contract. The strict 136-test private suite and zero-adapter core test
passed, and targeted Clippy passed with ten existing HAL warnings. At that
point an experimental Naga viewport transform was also in the working tree and
introduced three separate Clippy warnings; it was not used by the DX11 shader
path and is not part of the surface ownership fix. Visible presentation and
concurrent Present/context synchronization remain unverified.
The surface-view ownership fix was checkpointed as `7e045c4ed`.

Viewport remapping is implemented experimentally. The encoder rounds the
logical viewport consistently, intersects it with the target, and uses a
reserved vertex constant at `b13` with a second Naga-generated FL9 vertex
shader. The original shader is retained for in-target viewports. If the
remapped shader cannot be compiled or created, ordinary draws still work and
draws requiring remapping fail. Strict forced-FL9_1 tests cover all four
partially clipped edges, an entirely offscreen viewport, a one-pixel
intersection, fractional rounding, indexed and non-indexed draws, viewport
switching, variable clip-space w and depth testing with a nondefault depth
range. They check the complete 8x8 image. They do not establish arbitrary
perspective-varying interpolation, all clipping precision cases or historical
drivers. FXC rejects fragment-position input in `ps_4_0_level_9_1`; that is a
separate public-adapter blocker.

An FXC probe using 254 referenced uniform vectors compiled the original Naga
vertex shader, while its remapped variant failed with X5615 (258 instructions;
the profile limit is 256). The fallback and unavailable-variant draw rejection
are tested, but no general solution exists yet for such shaders. One diagnostic
using a 252-term nested WGSL expression overflowed the Windows test thread's
stack; the test was rewritten as sequential statements, and the lingering
crashed test process was terminated. The responsible parser/validator/writer
stage was not isolated; this remains an investigation item rather than a claim
about viewport math.

After integration, 153 private D3D11 tests passed in strict mode, and 135 Naga
library tests passed. Targeted Naga and DX11 Clippy passed with ten existing
DX11 test warnings; core zero-adapter isolation and DX12 HAL compilation
passed. The public HLSL `Options` struct gained a field, so downstream exhaustive
Rust literals need updating. GPU presentation remains hidden-only.
Following the error-classification and restored-pipeline checks, the focused
viewport suite passed 20 tests and the full strict private suite again passed
153 tests. A rebuilt DX12 GPU executable passed
`wgpu_gpu::encoder::drop_encoder_after_error` on the NVIDIA adapter; the DX12
HAL also compiled with the new Naga option disabled. During an earlier DX12
test build, other Cargo checks overlapped unintentionally; subsequent checks
were serialized. No memory-commit failure was observed.

Microsoft explicitly prohibits concurrent immediate-context use with most DXGI
calls. The current private surface does not yet serialize all swapchain calls
against device context operations. A common context mutex is necessary but
still needs a lock-order and responsive-message-pump design to avoid deadlocks.
HAL `wait_for_idle` also promises pending presentation completion, while the
current event-query wait has only established GPU command completion. These
are public-presentation blockers, not merely untested performance concerns.
See <https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-intro>
and <https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices#multithreading-and-dxgi>.
The restricted viewport implementation was checkpointed as `ec645e63e`.

Group-0 uniform bindings now support dynamic offsets. The layout retains the
dynamic bit; offsets are consumed in increasing logical binding order, checked
against the logical source-buffer size and captured in recorded commands.
Materialization still reads the GPU buffer at replay, preserving queued copies
and updates. The private alignment is four bytes; a future public device must
enforce its reported wgpu alignment. Strict FL9_1 GPU tests cover sparse mixed
uniforms, two dynamic offsets, preceding GPU copies, sampled textures, indexed
draws and an update across submissions. The mixed-uniform test read
[143,207,16,255] and [143,143,16,255]; the textured tests distinguished full,
half and quarter green. Synthetic tests reject bad counts, alignment, bounds,
overflow and offsets beyond an implicit-to-end range. The complete private
suite passed 157 tests; the core zero-adapter test and targeted Clippy passed,
with ten existing test warnings. Independent review found no source-range or
lifetime defect. Public adapter exposure and multiple bind groups remain open.
Dynamic uniform offsets were checkpointed as `edb1efa6b`.

The private swapchain synchronization change serializes DXGI calls with the
immediate context and uses short-lived surface and frame reservations instead
of holding their state locks through DXGI. A review identified a lost
unconfigure operation when a surface was busy; the draft now waits for that
operation to finish. The revised wait and teardown/recreation test passed on
the NVIDIA FL9_1 device, including a context-lock serialization check. The
hidden HWND clear returned BGRA [0,0,255,255], resize/readback succeeded, and
present returned `Ok(())`. This does not verify visible presentation.

This synchronization draft does not resolve synchronous window-message
reentrancy: a callback that reenters the context or waits for the same surface
operation may deadlock. It must not be treated as a safe public presentation
contract. Presentation completion in `wait_for_idle` also remains unproven.
The restricted private swapchain synchronization was checkpointed as
`9464c4aef`.

Separately, a shader-validation change narrows checked `textureLoad`
rejection to the selected entry point and transitively called functions.
Reachable loads remain rejected. A CPU call-graph test and a selected-entry
Naga/FXC sampled-rendering test passed; the latter verified the existing
alpha-blended readback. An independent review confirmed the traversal remains
conservative for calls in Naga's structured control flow.

The combined drafts passed `cargo test --offline -p wgpu-hal --lib
--no-default-features --features dx11 dx11:: -- --test-threads=1` with
`WGPU_DX11_REQUIRE_HARDWARE=1`: 162 passed, 0 failed, 2 filtered; some older
tests can still skip internally. Focused strict HWND and checked shader-render
tests independently passed. The core zero-adapter isolation test passed, and
`cargo clippy --offline -p wgpu-hal --lib --tests --no-default-features
--features dx11` passed with ten existing warnings. The first build exposed
three test-closure lifetime errors; these were corrected before the passing
run. No public D3D11 execution is claimed.

The selected-entry shader load validation was checkpointed as `7a7f249f5`.
Microsoft's required FL9_1 driver capabilities guarantee Always and LessEqual
depth comparisons, but do not establish the other six. The existence of the
D3D11 comparison enum or success on the RTX would not prove historical
support; a portable query or emulation remains to be investigated.

Private pipelines now map every combination of RGBA write channels, including
zero, to an explicit native state when necessary. FL9_1's required
`D3DPMISCCAPS_COLORWRITEENABLE` guarantees per-channel writes for its one
render target; this is distinct from independent masks across render targets.
Strict FL9_1 hardware tests verified all 16 masks over 64 pixels each, a
three-region masked pipeline transition, preserved depth writes with color
disabled, and masked additive blending. A synthetic test rejects unknown mask
bits. An independent review found no descriptor or lifetime defect. The full
strict 163-test suite passed before the final stronger transition assertion;
the focused hardware test passed again after that test-only improvement.

The color-mask work was checkpointed as `4bba5f9c5`. Triangle-list pipelines
now accept both front-face windings and none/front/back culling, using the same
native mapping as DX12. Opposite-winding textured triangles passed all six
combinations on the forced-FL9_1 RTX, with every pixel checked; a seventh case
also covered a negatively positioned viewport. An independent review found no
mapping or state-lifetime defect. The strict private suite passed 163 tests;
the core isolation test and targeted Clippy passed, with ten existing warnings.
The subsequently added Naga-to-FXC negative test for fragment `front_facing`
also passed: that built-in is rejected by the FL9_1 pixel profile even though
rasterizer face culling works. A direct FXC probe produced X4583 and X4568.
Face culling was checkpointed as `f8bd5183a`.

At the next checkpoint, a pipeline-wide register-plan
scaffold allocates disjoint compact slots across logical groups and feeds the
existing group-0 shader path; synthetic tests cover holes, visibility, sparse
bindings and aggregate register exhaustion. That checkpoint did not yet permit
a second group. An audit found that reusing one uniform materialization buffer
at two indices with different dynamic offsets would alias.
The 165-test strict private suite passed before the final allocator hardening;
afterward, the allocator test and a forced-FL9_1 dynamic-uniform GPU test
passed, with readback [143,207,16,255] and [143,143,16,255]. The allocator
rejects non-dense internal resource arrays and aggregate slot overflow; a
Naga/FXC test compiled distinct group-0 and group-2 uniforms using the same
logical binding number. Targeted Clippy passed with ten existing warnings.

Register planning was checkpointed as `f4df16d8b`. The current implementation
adds uniform-only groups after group 0, preserves layout-prefix invalidation,
and allocates independent constant-buffer materialization per group binding.
It rebinds using the active pipeline plan at each draw. Strict GPU tests cover
the same group at groups 0 and 2 with different dynamic offsets, a queued GPU
copy, direct and indexed draws, layout-prefix invalidation, pipeline switches
that move a native register, resource destruction before submit, and texture
sampling in group 0 with a group-1 uniform. The 64 checked pixels matched in
each case, including [64,128,0,255], [0,128,191,255] and [64,96,32,255].
The strict 167-test private suite passed; some older tests may skip internally.
An independent review found no concrete correctness defect. Dynamic offsets
here have private four-byte alignment, not a public wgpu limit. Each group bind
allocates scratch buffers and each draw stages and maps all bound uniforms;
this is correctness-first and can stall substantially. At that checkpoint,
sampled bindings remained restricted to group 0; public exposure was disabled.

The multiple-uniform-group implementation was checkpointed as `a914c9a1f`.
Current work extends sampled textures and samplers to separate logical groups,
using the same pipeline-wide native register plan and flattening current
per-group metadata for shader-relationship and conditional-NPOT validation.
Synthetic tests cover helper-derived cross-group pairs, unused invalid
cross-products and unavailable versus positive NPOT queries. Strict GPU tests
sample a patterned texture using samplers in different groups, switch the
sampler and pipeline slots during a pass, combine two sampled textures across
five groups, use a shared sampler and multiple samplers for one texture, retain
resources after destruction, and reject a later-group
render-target feedback alias, a missing group and an incompatible group.
The checked pixels distinguish [0,96,32,255], [64,0,0,255] and
[32,96,32,255]. These tests use a modern driver and do not establish legacy
conditional-NPOT hardware behavior. A test descriptor initially used the wrong
flags type; compilation caught it and the corrected focused GPU test passed.
The first full-suite run then had 167 passes and one failure: an old test still
expected a sampled layout at group 1 to be rejected. That layout is now
supported; its expectation was updated and the full suite passed 168 tests,
with two filtered. Afterward, added shared- and multiple-sampler GPU cases
passed the focused test; the latter checked [32,48,16,255] over 64 pixels.
The shared-sampler test was strengthened after review so that the second
patterned texture also distinguishes clamp from repeat; its 64 pixels matched
[16,56,48,255] on the forced-FL9_1 device.
Targeted Clippy passed with ten existing warnings after fixing a new style
warning, and the core zero-adapter test passed.
Cross-group sampled resources were checkpointed as `47e8488ef`.

A nondefault, unsafe `dx11-test-fixture` creates an owner-matched device and
queue for a closed core test without enabling ordinary adapter enumeration or
normal adapter opening. On the RTX 3090 Ti at requested and actual FL9_1,
`cargo test --offline -p wgpu-core --lib --no-default-features --features
dx11-test-fixture,std fl91_core_buffer_copy_readback_on_hardware -- --nocapture`
passed: core initialized the device, queued a buffer upload, submitted a GPU
copy, waited for completion and mapped 64/64 expected bytes. The fixture reports
no texture or surface capabilities and its intentionally restricted limits are
not a general adapter contract. Core's full fixture-enabled library suite
passed 67 tests; the feature-off zero-adapter test passed. Targeted fixture-on
core Clippy and feature-off HAL build passed. Independent review prompted
explicit command encoder and command-buffer teardown before device removal;
the focused hardware test passed again after that change.

The core buffer fixture was checkpointed as `f6aa8f241`. Its current extension
queries the actual RGBA8 driver support and tests a closed core offscreen
rendering sequence; forced-FL9_1 GPU readback matched 64/64 red pixels after a
blue clear. The fixture-enabled core suite passed 67 tests. Independent review
confirmed the observed readback path and requested negative format-capability
checks. The test-only format report is not safe for general public use:
core also treats color attachment support as transient attachment support,
which this HAL does not implement. Normal enumeration remains disabled.

The first rendering attempt exposed a separate core shader-validation panic
when a native device requests `max_inter_stage_shader_variables = 0`:
`wgpu-core/src/validation.rs` subtracts one from zero. Nearby deduction and
error-format arithmetic can also underflow. A limit of one was used for this
fixture because it is supported by the private backend; a separate core fix
and public Noop regression are still required. An independent audit confirmed
zero is a valid native limit request. The public Noop regression reproduced
the underflow before the fix. Signed intermediate budgets now preserve negative
post-deduction values and reject invalid pipelines without panicking; location
error formatting also handles zero. The public test covers limits zero and one,
point lists, front-facing and clip-distance deductions, and location errors.
The full public validation suite passed 91 tests with the expanded regression.
The fixture-enabled core suite passed 68 tests, including the physical FL9_1 rendering fixture and
the fragment error-format test. A first core test build caught missing alloc
imports in the new unit test; these were added and the suite then passed.
Fixture-enabled core Clippy passed. The public suite reported Vulkan loader
warnings about an absent OBS layer manifest; these did not fail tests.

The controlled core fixture now uploads a two-color texture and renders two
overlapping textured shapes with a D24S8 depth attachment through wgpu-core.
On the RTX 3090 Ti at requested and actual FL9_1, strict readback matched all
64 pixels: 32 red pixels where the near geometry occluded the far draw, and 32
green pixels where the far draw remained visible. The first focused run found
an obsolete assertion about the fixture's format flags; after updating that
assertion, the test passed. The fixture's queried format flags and narrow
test limits do not establish a safe general public adapter contract. The
fixture-enabled core suite passed 68 tests and the feature-off zero-adapter
test passed. Independent review confirmed the two draws and their readback
distinguish missing sampling, missing depth testing and missing depth writes.

The private u16 indexed path now accepts signed base vertices. At replay it
checks every raw and adjusted index and its vertex-binding range from the
GPU's current buffer contents before issuing that indexed draw. On forced
FL9_1, positive and negative bases, nonzero vertex/index binding offsets and
a prior GPU index upload rendered 64/64 expected pixels. A negative effective
index caused the expected replay error before the draw; prior copy and clear
commands had already executed. Synthetic tests cover limits and arithmetic.
The strict private suite passed 169 tests, with two filtered; some older tests
can skip internally. Targeted HAL Clippy passed with ten existing warnings.
This conservative path still rejects valid index data above its FL9 limits and
does not implement public robust-draw behavior or u32 indices.

The indexed path now discards a whole draw successfully when any vertex fetch
is outside its bound range, while otherwise-valid indices beyond the native
limit still fail replay. Synthetic tests cover both classifications and their
precedence. On forced FL9_1 a late invalid index suppressed an entire draw;
a later GPU index upload and draw produced the expected black-left, red-right
image and signaled the fence. Existing out-of-range textured tests also passed.
The strict private suite passed 169 tests, two filtered; some older tests may
skip internally. HAL Clippy passed with ten existing warnings. This does not
yet implement u32 indices or translation of otherwise-valid high indices.

With strict hardware mode, `cargo test --offline -p wgpu --no-default-features
--features std,dx11-test-fixture --test dx11_fixture -- --nocapture` passed on
the RTX 3090 Ti at requested and actual FL9_1. The nondefault test injects the
closed unsafe fixture through the public Rust API, verifies zero ordinary
D3D11 adapters before and after injection, and checks 32 red and 32 green pixels
from textured depth rendering. The first build exposed a missing unwrap of the
mapped-range result; after correction the test passed. Targeted fixture Clippy
and `cargo check --offline -p wgpu --no-default-features --features
std,dx12,wgsl` passed. This fixture does not provide a usable public adapter.

An index-translation audit found that rebasing changes the shader-visible
`vertex_index`. The backend's Naga-to-D3DCompile test rejects direct and
structure-member `vertex_index` entry inputs, used or unused, on the installed
FL9_1 compiler with X4555. An unrelated selected entry and the shader-model-5
control compile. A separate installed FXC executable produced the same
diagnostic. Future translation must explicitly preserve or reject this semantic
rather than relying on a compiler's current behavior.

The private pipeline now accepts a depth-only pass with no fragment stage,
using a trivial pixel shader with no render target. The installed FXC rejects
an entirely outputless pixel shader. The strict FL9_1 GPU test wrote depth on
the left half, then read back 32 black occluded and 32 green visible pixels
from a later color pass. The first run revealed that the new pass disturbed an
existing test's baseline; restoring the prior color and 0.75 depth baseline
made that test pass. An independent review found no state or lifetime issue and
prompted correct out-of-memory classification for the fallback compiler. Its
synthetic error test passed. The full private suite passed 170 tests before
that small error-path adjustment; the focused GPU and synthetic tests passed
afterward. Fixture rendering and targeted HAL Clippy passed. Historical FL9_1
driver behavior and fragment shaders without color targets remain unverified.

The private Uint32 index path stages authoritative GPU contents at replay and
converts effective indices up to 65534 into a fresh immutable native Uint16
buffer. It discards whole draws with out-of-range vertex fetches and reports a
replay error for otherwise-valid larger effective indices. Selected vertex
entry points declaring `vertex_index` are explicitly rejected. On the RTX at
forced FL9_1, GPU-written u32 data with both base directions rendered 64/64
red pixels; raw values 65535–65537 with base -3 also rendered correctly.
The 65534 boundary rendered, a valid-fetch 65535 was rejected, and a late OOB
draw was discarded before a subsequent GPU rewrite and draw. The private suite
passed 173 tests before the additional high-raw test was added; the expanded
focused GPU test passed, as did public-fixture rendering and targeted Clippy
(ten existing HAL warnings). Independent review found no concrete defect.
General u32 support, vertex-index semantics, instancing and public exposure
remain incomplete; staging and per-draw allocation incur synchronization cost.

The private u32 path now rebases a single effective-index window with a span
of at most 65534 to a checked vertex-buffer offset. On forced FL9_1, effective
indices 65535–65537 rendered correctly from a nonzero binding offset, followed
by direct and u16 draws that restored their original vertex bindings; all 64
pixels matched. A valid-fetch window spanning 65535 was rejected, and
synthetic checks cover bounds, alignment, overflow and OOB precedence. The
strict private suite passed 173 tests, two filtered; older tests may skip
internally. Independent review found no concrete defect. High stream offsets
remain unverified on historical FL9_1 drivers, and multiple windows or a
single triangle with a wider span remain unsupported.

The opt-in public wgpu fixture now records a GPU buffer copy to a Uint32 index
buffer and uses indexed drawing for the far textured triangle. Strict FL9_1
readback still matched 32 red and 32 green pixels, and ordinary adapter
enumeration remained empty. This remains a controlled unsafe fixture.

Restricted direct instancing now records one logical draw and replays all
triangle-aligned native batches for each instance in order. Selected shaders
declaring `instance_index`, indexed instancing and nonzero first instance are
rejected; general instancing remains unavailable. Strict FL9_1 GPU readback
matched zero/one/two-instance outputs and [40,120,0,0] within one unit for an
overlapping two-instance case across the 65535-primitive boundary. Its RGB-only
standard-alpha state couples native alpha factors because alpha is masked off;
alpha-writing separate-blend portability remains unresolved. The strict private
suite passed 176 tests, two filtered; older tests may skip internally. The
public fixture still rendered correctly and HAL Clippy passed with ten existing
warnings. An independent review found no concrete correctness defect.

Uint32 triangle lists now split into ordered rebased windows, with all indices
and scratch allocations checked before any native draw. A strict GPU test
verified low/high/low window order and a following direct draw. A second test
verified that a later unsupported wide triangle prevents an earlier valid
window from drawing. A synthetic test initially expected three windows for data
that fit in two; its input was corrected and the test passed. The first test
build also caught a missing `alloc::vec::Vec` import, which was added. The
strict private suite passed 177 tests, with two filtered; older tests can skip
internally. The public indexed fixture passed with 32 red and 32 green pixels.
An individual triangle spanning over 65534 effective indices still fails,
and high stream offsets remain unverified on historical FL9_1 hardware.

Restricted indexed instancing now preflights u16 or u32 indices once and
replays every chunk or window for each instance in order. Zero instances do
not stage or draw; selected instance-index inputs and nonzero first instance
remain rejected. Forced-FL9_1 GPU tests cover u16 chunk order, u32 window order,
zero/one/two instances and OOB discard. The strict private suite passed 178
tests, two filtered; older tests may skip internally. An independent review
identified that a saturating u16 case alone could not prove the second instance
executed, so an additional small translucent u16 case was added and passed.
HAL Clippy passed with ten existing warnings after removing a new argument-count
warning; the focused GPU test passed again after that change.

The private u32 path now gathers vertices for individual triangles spanning
more than 65534 effective indices. All draw bounds and scratch allocations are
preflighted, then GPU buffer copies gather only each validated attribute extent
into zero-initialized vertex scratch. Ordered window/gather segments replay in
instance order. On the forced-FL9_1 RTX, strict readback passed for GPU-written
indices and vertices, a negative base, nonzero binding offset, a final vertex
without trailing stride padding, 0/1/2 instances, mixed segment ordering and
late OOB discard. The private suite passed 178 tests, two filtered, and the
public indexed fixture passed. An independent review found no concrete defect;
historical-driver copy behavior remains unverified and each gathered triangle
currently requires a scratch allocation and three GPU copies.

To prevent draw splitting or gathering from changing a shader-visible ID, the
private pipeline now explicitly rejects `primitive_index` on the selected
fragment entry point, including struct members and unused declarations. Other
entry points remain unaffected. Validated Naga tests, actual pipeline creation
and Naga-to-FXC checks passed; the installed FL9_1 compiler reported X4583 and
X4568 for the builtin. The first test build caught a borrowed test shader being
destroyed too early; isolating the descriptor fixed it. The strict private suite
then passed 180 tests, with two filtered; older tests may skip internally.
Independent review found no concrete defect. The public indexed fixture and
HAL Clippy passed; Clippy retained ten existing warnings.

Within the restricted instancing subset, nonzero first-instance values are now
accepted without narrowing arithmetic because no selected shader or vertex
input can observe them. Strict GPU tests covered first-instance 0, 7 and
u32::MAX with direct and indexed draws. Uint16 draws that exceed FL9 native
index limits now use the same validated windows and GPU gather as Uint32 while
the native fast path remains for supported values. GPU readback verified a
65535 gather, negative- and positive-base windows, and restoration for a
subsequent native draw. The first full-suite run exposed five obsolete tests
that expected nonzero first instance to fail; their negative coverage was
changed to invalid ranges/alignment. The rerun passed 181 tests, two filtered;
older hardware tests can skip internally. Independent review found no concrete
defect in the translation or instance semantics. The public indexed fixture
passed again, and HAL Clippy passed with ten existing warnings.

Zero-input vertex pipelines and nonindexed zero-input point lists are now
supported privately. The baseline pipeline test failed before the change. On
the forced-FL9_1 RTX, an interior point produced one green and 63 blue pixels;
point-to-triangle-to-point transitions produced one green and 63 red pixels.
A 65,536-point GPU call was accepted, and CPU checks verified its 65,535/1
batch split and instance order. Zero-input native batches use start vertex zero
because shader-visible vertex IDs are rejected. Indexed points and point
pipelines with vertex buffers remain unsupported. The private suite passed 182
tests before the strengthened point tests; the focused GPU and CPU tests passed
afterward. The public indexed fixture and HAL Clippy passed, with ten existing
warnings. Historical point raster and driver behavior remain unverified.

FL9_1 now rejects alpha, inversion and vignette blend modes when both RGB and
alpha writes require unguaranteed separate-alpha support. Masked RGB-only and
alpha-only variants use equivalent coupled factors, and enabled blending
requires a positive format query. The focused strict test first demonstrated
the missing rejection, then passed on the RTX at forced FL9_1 with complete
8x8 readback for all three masked blend modes. Full-channel tests retain an
explicit test-only diagnostic opt-in and do not establish portability. The
strict private suite passed 184 tests, two filtered; older tests may skip
internally. The public indexed fixture passed with 32 red and 32 green pixels.
HAL Clippy passed with ten existing warnings. Independent review checked the
mask/factor combinations and device-error handling without finding a defect.
Checkpoint: `d01ace12a`.

Ordinary BGRA8 textures are the next resource increment. Before implementation,
`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_bgra_upload_copy_and_readback -- --nocapture` in strict hardware mode
selected the RTX at FL9_1 and failed at BGRA texture creation (`Unexpected`).
This baseline establishes missing functionality, not a GPU transfer failure.
After implementation, the strict BGRA filter passed four tests, including three
physical-GPU cases: asymmetric upload/copy/readback with padding; sampling raw
BGRA `[16,64,224,255]` as logical RGBA `[224,64,16,255]`; and rendering then
copying a partial region between different-sized textures with initialized
untouched pixels. Synthetic checks cover missing format support, incompatible
views and cross-format copies. The strict private suite passed 187 tests, two
filtered. HAL Clippy passed with ten existing warnings. A read-only independent
review found no concrete defect; historical GPU behavior is unverified.
Checkpoint: `86a5ddfb5`.

The core-validation baseline requested an occlusion query through the
controlled FL9_1 fixture and failed as `Device(Lost)`. FL9_1 does not guarantee
occlusion queries; a new downlevel availability flag now rejects that request
before HAL. The strict fixture then continued to GPU upload and textured depth
readback (64/64 bytes and 32 red/32 green pixels); all 68 fixture-enabled core
tests passed. A synthetic GLES version/extension test and a compliance test
passed; core and GLES Clippy passed. Custom HAL adapters and stored flag sets
must add the new flag for supported queries. GLES already uses conservative
query results, a separate pre-existing semantics issue this does not resolve.

The existing query GPU tests passed on DX12 (four RTX and four Microsoft Basic
Render Driver cases), desktop GL (four RTX cases), and Vulkan (four RTX cases).
DX12 query initialization reported 18 executed and four unsupported cases;
GL reported nine executed and two unsupported cases. A Noop creation test also
passed. The first parallel test-harness invocation aborted because isolation
requires `--test-threads=1`; rerunning serially passed. Vulkan initially failed
because the loader referenced a missing OBS layer manifest. Setting only the
test process's `VK_IMPLICIT_LAYER_PATH` to an empty temporary directory allowed
the Vulkan tests to pass, with a remaining registry lookup warning. This does
not repair the system's Vulkan configuration. The strict public wgpu fixture
also passed on the RTX at FL9_1 with 32 red and 32 green pixels.
The GPU report was generated at ignored `.gpuconfig`; the process-local Vulkan
layer override used the empty directory `target/codex-empty-vk-layers`.

The query regressions used `cargo test --offline -p wgpu-test --test wgpu-gpu
-- occlusion_query --nocapture --test-threads=1` with `WGPU_BACKEND` set in
turn to `dx12`, `gl`, and `vulkan`. The initialization runs used the same
command with `resolve_query_set_init` as the filter, on DX12 and GL. The
focused synthetic commands were `cargo test --offline -p wgpu-types --lib
occlusion_queries_are_required_for_webgpu_compliance` and `cargo test --offline
-p wgpu-hal --lib --no-default-features --features gles
conservative_occlusion_query_availability`. The core suite used
`cargo test --offline -p wgpu-core --lib --no-default-features --features
dx11-test-fixture,std`; strict hardware mode was enabled for the fixture.
Checkpoint: `17b840c1d`.

The next private increment is a read-only D24S8 attachment without sampled
aliasing. Its strict hardware baseline first failed to compile because a test
cloned a generic descriptor with an unsupported bound; after correcting the
test, the RTX at requested/actual FL9_1 failed read-only texture creation with
`Unexpected`. No public read-only-depth capability is implied by this work.
The private implementation now accepts READ-only textures/views and passes
only with LOAD|STORE for both aspects and a non-writing pipeline. A strict GPU
test on the RTX verified 32 green/32 blue pixels for depth preservation, then
32 red/32 blue pixels across READ→WRITE→READ submissions after releasing
caller resources. It rejects write-enabled pipeline transitions, clears,
discard, don't-care and depth sampling aliases. The full strict private suite
passed 188 tests, two filtered, before the added transition/lifetime assertions;
the expanded focused test passed afterward. HAL Clippy passed with ten existing
warnings. Independent review found no concrete defect. Historical support for
the zero depth-write mask remains unverified.
Checkpoint: `03cf563b1`.

The next shader baseline demonstrates that a selected checked shader with a
uniform and local-vector dynamic indexing is rejected by the existing blanket
guard. The new selected-entry, typed analysis accepts only proven local/value
vectors and leaves uniform pointers, arrays, matrices and unknown types rejected.
The focused strict GPU run executed three cases (in-range vector selection,
upper out-of-range selection and a uniform copied to a value), each with 64/64
pixels matching. Two further cases explicitly skipped GPU execution after the
tested FXC rejected potentially negative indices with X3548 and dynamic vector
stores with X3500. The complete private suite passed 194 tests, two filtered,
before final boundary-test refinements; those two GPU skips are included in the
libtest count and are not hardware passes. The tightened focused run passed six
libtest cases: three executed on GPU and three skipped GPU execution after X3548
or X3500, including an added out-of-bounds store case. The guard diagnostic is
asserted exactly and unexpected compiler failures fail the test. HAL Clippy
passed with ten existing warnings. Historical drivers remain unverified.
Checkpoint: `7c4fd052c`.

The next topology baseline, `fl91_nonindexed_triangle_strip_winding_and_first_vertex`,
selected the RTX at requested/actual FL9_1 and failed pipeline creation with
the existing unsupported-topology error. Nonindexed strips now work privately;
indexed strip restart remains excluded because not all 10level9 hardware
supports native strip cuts. The D3D9 65,535-unique-vertex caveat requires
conservative, even-parity overlapping batches rather than assuming the full
primitive count can be consumed by one strip.
The strict RTX test compared both winding and cull modes with triangle lists,
checked the second batch of a 65,536-vertex strip, rejected indexed strips and
verified a nonzero-offset strip-to-list transition with 32 red/32 green pixels.
The CPU batch test checked overlap and instance order. Naga→FXC reported X4568
for one flat-interpolation declaration; it did not execute on GPU. The combined
strict suite and Clippy results are recorded below. Rebased stream offsets and
strip behavior remain unverified on historical FL9_1 drivers.
Checkpoint: `ec8dbf12e`.

Independent shader review found that Naga's HLSL matrix-in-struct store
lowering can emit an unchecked dynamic component index and that some related
store helpers take destinations by value. The installed FL9 FXC rejects the
raw dynamic store with X3500 in both vertex and pixel profiles; this does not
establish safety on all compilers. Checked-shader fail-closed validation and
static-store correctness tests are in progress, including shaders without
uniforms and pointer-helper paths. Do not infer completion from the prior
local-vector GPU passes.
CPU regression baselines `checked_matrix_component_stores_require_safe_lowering`
and `matrix_struct_stores_require_correct_lowering_even_unchecked` both failed
before the fix, confirming the existing classifier accepts the problematic
shapes. The guard must cover stores and pointer call arguments through nested
struct/array paths in both checked and unchecked modes, while leaving safe
read-only matrix use available.
The implemented guard fails closed for affected aggregate stores and unknown
pointer provenance, tracks call-site provenance, and bounds the work and
nesting of its type and call-graph analysis. Five focused matrix tests passed,
including shared type DAGs, call-state limits and shared work exhaustion; the
checked/unchecked shader-module test passed. The combined strict private suite
passed 203 libtest cases with two filtered; three local-vector cases explicitly
skipped GPU execution, as noted above. Independent review found no concrete
provenance bypass. The guard conservatively rejects writes to other fields in
an affected aggregate and sufficiently complex valid shaders; it does not fix
the shared Naga lowering.
Checkpoint: `d77a14387`.

Vertex-input overlap audit: the FL9_1 minimum does not guarantee overlapping
elements in one input stream. The strict
`fl91_vertex_element_overlap_is_rejected` baseline created adjacent and
reverse-ordered disjoint controls on the RTX at requested/actual FL9_1, then
failed because the backend accepted a same-start overlap. A selected-level
fail-closed validation change now rejects same-start, partial and nested byte
ranges while accepting adjacent and disjoint ranges. The strict production
test and synthetic checks across seven feature-level identities passed; the
latter do not claim higher-level execution support. Success on this modern
driver is not evidence of historical stream-offset or stride support.
Checkpoint: `3cdbb28f6`.

Additional matrix-guard regressions passed for a bare matrix and matrix-pointer
helper, matrix arrays, safe/unsafe helper call order, an unselected unsafe
entry, checked and unchecked compilation, and the combined call-depth boundary.
The strict private suite passed 206 libtest cases, two filtered; the three
documented shader cases still skipped GPU execution. HAL Clippy passed with
ten existing warnings. Independent overlap review found no concrete defect.
Test checkpoint: `07b95c801`.

The direct triangle-list batching baseline failed a CPU boundary test: a
65,538-vertex draw was issued as one batch. D3D9 documents a 65,535-unique-
vertex caveat when MaxPrimitiveCount is 0xffff; whether 10level9 emulates that
restriction is unresolved. A conservative FL9_1-only direct-draw limit is in
progress, leaving indexed batching separate.
The new FL9_1 policy caps direct triangle-list batches at 65,535 vertices;
CPU tests passed for split points, tails, overflow and instance order, and a
strict RTX GPU test matched all 64 pixels with visible geometry on each side
of the split. The opaque GPU output does not independently prove instance
order. The complete strict private suite passed 208 libtest cases, two filtered,
with three shader GPU skips as documented; HAL Clippy retained ten existing
warnings. Independent review found no concrete defect. Large native first
vertex offsets remain unverified on historical devices.
Checkpoint: `894aa40c4`.

The next private increment targets sparse multiple vertex-rate streams for
direct triangle lists. Indexed window/gather and strip rebasing currently
assume one slot; their multi-slot variants must remain rejected until every
stream can be validated and translated together.
The strict `fl91_sparse_vertex_streams_render` baseline selected the RTX at
requested/actual FL9_1 and failed pipeline creation for position slot 0 plus
color slot 2, as expected from the existing single-slot restriction.
The direct-only implementation now supports sparse vertex-rate slots with
independent range checks and replay bindings. The first GPU run produced green
127 instead of the test's 128 due to interpolation/UNORM rounding; a 1-LSB
tolerance for that channel retained the test's discriminating output. The
expanded strict RTX test then passed, covering distinct and shared buffers,
slot 15, rejection at 16, transitions, a queued copy with source/destination
released before submission, and an exact last-attribute range plus one-byte-
short rejection. Physical overlap across slots and historical drivers remain
unverified. Indexed and strip multi-slot draws are still rejected.
The strict private suite passed 209 libtest cases, two filtered, with three
documented shader GPU skips. HAL Clippy passed with ten existing warnings.
The controlled core and public wgpu fixtures each passed on the RTX, including
their 32 red/32 green textured-depth readback. Independent review found no
concrete slot, bounds, stale-state or lifetime defect.
Checkpoint: `82a538433`.

Nonindexed strip batching now preflights and rebases every active vertex stream.
Indexed multi-stream draws remain rejected until their GPU index scan, windows
and gather handling cover all streams.
The strict `fl91_sparse_vertex_strips_render` baseline selected the RTX at
requested/actual FL9_1 and failed at end_encoding with Unexpected, confirming
the existing multi-slot strip restriction.
The strict RTX sparse-strip test then passed with slots 0/15, different strides
and offsets, a nonzero first vertex, a 65,536-vertex split, queued GPU copy and
resource destruction before submission. Every pixel matched. The opaque,
unculled output does not independently prove instance order or split parity;
existing CPU batching and single-stream culling tests cover those aspects.
Independent review found no concrete per-slot rebasing or lifetime defect.
The strict private suite passed 210 libtest cases, two filtered; the three
documented shader GPU skips remain skips. HAL Clippy passed with ten existing
warnings, and the controlled core fixture again matched 32 red/32 green pixels.
Checkpoint: `9a1efde6c`.

Indexed replay errors are not benign public validation: an Unexpected submit
error loses the core device and does not signal the requested fence. A future
partial indexed multi-stream path must avoid data-dependent unsupported replay
errors or remain explicitly private; earlier GPU work is not rolled back.
A private native-u16 multi-stream subset is accepted only when
binding sizes and base vertex prove at encoding that every in-range index fits
native FL9_1 limits. GPU index data still requires a complete replay-time OOB
scan; general u32 and high-index multi-stream translation remains future work.
The strict `fl91_sparse_vertex_indexed_render` baseline failed at end_encoding
on the RTX at requested/actual FL9_1, as expected from the existing restriction.

The focused strict RTX test passed after implementation, checking full-image output,
nonzero first index, positive and negative base vertices, two additive instances,
GPU index upload, resource release before submission, and whole-draw OOB discard.
Two CPU proof tests passed, including exhaustive checks over all 65,536 Uint16
values for 42 representative configurations. The complete strict private suite
passed 213 cases, two filtered, with three documented internal shader skips.
HAL Clippy passed with ten existing warnings. The core hardware fixture passed
with 64/64 buffer bytes and 32 red/32 green pixels; ordinary core enumeration
still returned zero D3D11 adapters. Independent review found no concrete issue.
The native subset stages and maps index data at replay and may stall; the general
multistream translation and public validation remain blockers.
Checkpoint: `9bee6efe5`.

Multi-stream indexed draws now scan all GPU-written Uint16/Uint32 indices against
every bound stream before drawing; any OOB fetch discards the entire draw.
Native-safe Uint16 keeps the direct path. Other data is translated to ordered
common index windows, with per-stream rebasing or per-stream GPU gather for
wide triangles. All segment resources are prepared before native draw commands.
The three strict `fl91_sparse_vertex_indexed` GPU cases passed on RTX at
requested/actual FL9_1: native u16, u16 gather to native transition with signed
base and instances, and u32 window/gather plus late OOB and retained GPU data.
Independent review found no concrete scan, ordering or lifetime defect.
The first combined suite found one obsolete test still expecting Uint32 rejection;
it was changed to test an index-buffer range that is actually out of bounds.
The corrected strict suite passed 216 cases, two filtered, including the new
transient tests; three previously documented shader cases still skip GPU work.
HAL Clippy passed with ten existing test-target warnings. The controlled core
fixture also passed its texture validation checks and 32 red/32 green readback.
Checkpoint: `954e07c28`.

Transient color and D24S8 attachments now use ordinary D3D resources and retain
transient usage on textures and views. Unsupported combinations are rejected;
the allocation hint does not imply memoryless storage. The strict transient
filter passed one CPU test and one RTX FL9_1 hardware test, with 64 matching
pixels each for transient depth and transient color observed through stored
depth. The core fixture verified color/depth creation and views, rejection of
invalid transient/unsupported format or storage usages, and subsequent ordinary
GPU rendering. It does not yet exercise a core-level transient pass. The full
strict suite passed 216 cases, two filtered, including these tests; HAL Clippy
passed with ten existing warnings, and normal core enumeration stayed empty.
An independent review found no concrete correctness issue. Historical drivers
and memoryless optimization are unverified.
Checkpoint: `672db3fb3`.

FL9_1 overlapping vertex elements now use deterministic native slot groups,
while logical buffer bindings and range checks remain unchanged. The strict
`overlapping_vertex` GPU test passed nine full-image executions on the RTX,
covering exact/partial overlap, lists, strips, pipeline transitions and u32
window/wide-gather draws. The slot allocator CPU test passed. The controlled
core fixture rendered both disjoint and partially overlapping layouts, each
with 32 red and 32 green pixels. The five strict sparse vertex regressions
passed. Independent review found no concrete mapping or lifetime defect;
historical FL9_1 drivers and remaining vertex formats/strides are unverified.
The complete strict D3D11 suite passed 218 cases, two filtered, with the three
previously documented internal shader GPU skips. HAL Clippy passed with ten
existing warnings; core ordinary D3D11 enumeration remained empty.
Checkpoint: `53abef8bc`.

A restricted stencil-state classifier accepts only states proven to always pass
on both faces and never change the eight-bit stencil value; these are implemented
with native stencil disabled. Reference updates inside passes are ignored for
this subset. Strict `stencil_noop` passed one CPU classifier test and one RTX
FL9_1 hardware test comparing IGNORE with masked Equal across two passes,
reference/pipeline changes and a read-only depth pass; invalid states and an
outside-pass reference were rejected. Actual stencil bits cannot yet be read.
The controlled core fixture also produced 32 red/32 green pixels using masked
Equal, Replace with zero write mask and a nonzero reference. The strict suite
passed 220 cases with two filtered before the final test-only boundary additions;
the updated focused test passed. Three historical shader cases still skip GPU
work. HAL Clippy passed with ten existing warnings and core enumeration stayed
empty. Independent review found no concrete classifier defect. Future stencil
support must retain and rebind reference state across pipeline switches.
Checkpoint: `c9a4105b2`.

Snorm16x2 and Snorm16x4 vertex inputs now map to the corresponding native
signed-normalized input formats. Microsoft lists these IA formats as required
on FL9_1. The strict RTX test queried flags 0x21122 and 0x2, and direct and
indexed rendering matched 64/64 pixels using negative endpoints, zero and
positive intermediate values. The controlled core fixture selected colors
using Snorm16x2 UV and produced 32 red/32 green pixels; its y component is not
independently observable. The CPU mapping test and independent review passed.
The complete strict private suite passed 222 cases, two filtered, with three
previously documented internal shader skips; HAL Clippy passed with ten
existing warnings and ordinary core D3D11 enumeration remained empty. The
private four-byte alignment remains; no SNORM texture or historical-device
support is inferred.
Checkpoint: `cca55a8f0`.

Zero-stride vertex streams now retain a constant bound record for direct and
strip draws and through indexed windows/gathers. The strict `zero_stride` filter
passed one CPU and one RTX FL9_1 GPU test: five full-image executions cover
mixed streams in slots 0/15, direct, strip, u16 and u32 window/gather paths,
updates, transitions and an undersized binding rejection. The all-zero-stream
triangle only establishes degenerate completion. The controlled core fixture
first hit its synthetic vertex-stage buffer limit of one; after raising that
test-only limit to two, it passed with 64 green pixels from constant UV data.
The complete strict suite passed 224 cases, two filtered, with the prior three
internal shader skips; HAL Clippy passed with ten existing warnings and core
ordinary enumeration stayed empty. Independent review found no concrete memory
or ordering defect. A negative effective indexed vertex with only zero-stride
streams is conservatively discarded even though its constant addresses may be
in bounds; its observable effect is unresolved beyond the current degenerate,
side-effect-free triangle subset. Historical hardware remains unverified.
Checkpoint: `cc8e1aa68`.

Restricted instance-rate vertex streams now bind each instance's source record
as a zero-stride native vertex stream before repeated ordinary draws; logical
bounds are preflighted using wide arithmetic, and indexed rebasing/gathers use
only vertex-rate streams. The strict `instance_stream` filter passed two CPU
and one RTX FL9_1 GPU test. Four full-image executions covered nonzero first
instance, sparse vertex/instance slots, zero-stride tint, GPU-written data,
retention, direct/strip/u16/u32 windows and gather. Overlap at pixel (4,7)
independently distinguishes correct instance-outer ordering. The controlled
core fixture produced 32 red/32 green pixels for two instance records; the
unsafe public-wgpu fixture still produced its 32 red/32 green indexed scene.
The first strict full suite found an obsolete rejection assertion; after updating
it, 228 cases passed with two filtered and three documented internal shader
skips. HAL Clippy passed with ten existing warnings and ordinary core D3D11
enumeration remained empty. Independent review found no concrete defect.
All-instance-only indexed draws and instance_index remain unsupported.
Checkpoint: `8653ba625`.

Corrected the internal maximum vertex-buffer count from the obsolete single-
stream value to the backend's 16-slot limit. The synthetic capability tests
passed (4 cases); slot 15 and sparse bindings were GPU-tested previously, but
simultaneous execution using all 16 slots has not been established. This does
not expose a public adapter.
Checkpoint: `fc5cfb75e`.

Uniform bindings now retain any nonzero logical size up to 64 KiB rather than
requiring a 16-byte multiple. The native constant/staging buffers are rounded
up, with only the exact GPU-copied bytes materialized into a pre-zeroed scratch
buffer. The strict HAL tests checked 4/5/12/20-byte ranges, a five-byte dynamic
range following GPU copy and a five-byte range at a 65-byte buffer end; logical
bytes and native padding matched readback. The controlled core fixture initially
failed UnalignedShader with its downlevel flags empty; after enabling only the
fixture's BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED flag, 4/5/12/20-byte cases rendered
64 expected pixels each, including dynamic offset. This is not public exposure.
Three strict integer-uniform shader cases (vertex/fragment bitcast and fragment
comparison at 2^24) passed 64 pixels each on the RTX; FXC disassembly raised a
possible mismatch that those cases did not reproduce. Integer vertex inputs and
general integer/historical semantics remain unverified and unadvertised.
The strict private suite passed 233 cases, two filtered, with three documented
internal shader skips. HAL Clippy passed with ten existing warnings; ordinary
core D3D11 enumeration stayed empty. Two independent reviews found no concrete
uniform materialization defect. Odd-byte GPU copy behavior remains unverified
on historical drivers, and staging maps may stall.
Checkpoint: `61d1e7a3e`.

For FL9_1 samplers with one mip, equal min/mag filters and anisotropy 1,
previously rejected finite LOD intervals normalize to native 0..FLT_MAX.
The sampler is marked single-mip-only; current bind-group validation also
requires sampled textures and views to have one mip. Strict `single_mip` tests
passed three cases, including 32 patterned RTX FL9_1 readbacks across nearest/
linear spatial and mip filters, magnification/minification and intervals
0..0, 0.5..0.5 and 1..4 versus reference. The controlled core fixture's
fractional sampler case matched all 64 pixels. Two independent reviews found
no concrete defect for the current one-mip subset. Explicit LOD/bias, mixed
filters with these clamps, anisotropic cases and historical drivers remain
unverified or rejected. The complete strict suite passed 235 cases with two
filtered before the final test-only mip-linear extension and model-field update;
the updated focused GPU and four capability CPU tests passed. HAL Clippy passed
with ten existing warnings and core ordinary enumeration remained empty.
Checkpoint: `62ef2660b`.

Copy-only texture views may now retain metadata without native SRV/RTV objects;
unsupported sampling or attachment use remains rejected. The strict private
view test passed on RTX FL9_1, including retained resource lifetime. The
controlled core fixture created RGBA8 and BGRA8 copy-only views, rejected
sampled/render view usages and matched 12 GPU-transferred pixels for each.
The combined strict suite passed 237 cases, two filtered, with three documented
internal shader skips. HAL Clippy passed with ten existing warnings; core
ordinary D3D11 enumeration remained empty. Independent review found no concrete
ownership or usage defect.
Checkpoint: `6a64195ad`.

FXC's current level-9 pixel profile emits unclamped legacy texldb for dynamic
SampleBias, whose documented +/-3 range is narrower than WGSL's allowed bias.
The selected-entry shader path now conservatively rejects reachable SampleBias
in checked and unchecked modes, including helper calls. Profile selection is
shared with compilation so the guard also applies on higher hardware while
this backend still targets FL9_1 profiles. Strict `bias` tests passed one CPU
case and one RTX FL9_1 hardware case: direct/helper bias pipelines rejected
with the specific linkage error and an ordinary sample from the same module
matched 64 pixels. Two reviews found no further concrete traversal or profile
defect after the initially identified device-level gating error was fixed.
The complete strict suite passed 237 cases, two filtered, with three documented
internal shader skips; HAL Clippy passed with ten existing warnings and ordinary
core enumeration remained empty. Conservatively rejected bounded bias and
historical-device behavior remain future work.
Checkpoint: `1f48812f7`.

A restricted POT RGBA8/BGRA8 copy-only mip chain is implemented, requiring
COPY_SRC|COPY_DST, one layer/sample and positive native format MIP support.
Every mip is zero-initialized; upload, readback and cross-resource copies carry
checked native subresource indices. Metadata-only views can select valid mip
ranges. Strict `mip` tests passed six cases, including RTX FL9_1 RGBA/BGRA
four-level initialization, padded partial uploads, cross-mip copies and full
readback: two format cases executed and passed, none skipped. The controlled
core fixture checked 43 pixels over four mip levels. Missing format support now
skips explicitly in normal mode and fails strict mode; query errors in the core
fixture remain failures. Three independent reviews found no concrete transfer,
initialization or ownership defect. The complete strict suite passed 241 cases,
two filtered, with three documented internal shader skips; two new Clippy test
warnings were corrected and the focused capability test passed afterward.
Clippy then had the existing ten warnings, and ordinary core enumeration stayed
empty. Sampling/rendering mips, NPOT mips, same-resource copies and historical
drivers remain unverified or unsupported; FL9 SRVs must include the smallest
mip, blocking arbitrary core-valid sampled views.
Checkpoint: `352c415eb`.

POT RGBA8/BGRA8 mip chains can now be sampled through full-chain D2 views
when texture usage is exactly COPY_SRC|COPY_DST|RESOURCE. Per-sampler LOD
translation metadata is checked against actual texture mip count only for
selected shader sampling pairs; a rewritten minimum remains one-mip-only, and
a raised maximum requires coverage of the last mip and non-anisotropic
filtering for multi-mip views. Strict `sampled_mip` passed 46 RTX FL9_1 GPU
executions with no format skips, including four selected levels, nearest/linear
mip filtering, fractional blending, used/unused pair validation and same-pass
transitions with resources destroyed before submission. The core fixture
sampled mip1 blue and checked all 64 pixels; the unsafe public-wgpu fixture's
existing indexed textured-depth scene still matched 32 red/32 green. The full
strict suite passed 244 cases with two filtered before the final test-only
lifetime addition; its updated targeted test passed. Three prior shader GPU
cases remain internally skipped. HAL Clippy passed with ten existing warnings,
and ordinary core D3D11 enumeration remained empty. Independent source and
GPU-test reviews found no concrete defect. Non-full sampled views, attachment
mips, NPOT chains, certain LOD/aniso pairs and historical drivers remain gaps.
Checkpoint: `e066d405c`.

Private POT RGBA8/BGRA8 sampled mip views may now start at any mip if the
view includes the resource's last mip, as required by FL9. Sampling-pair LOD
validation uses the view's dimensions and relative mip count. Strict RTX
FL9_1 testing passed 54/54 sampled-mip cases with no format skips, including
relative LOD, finite max LOD, single-level suffixes, lifetime and rejection of
non-suffix views. The controlled core suffix-view fixture read 64/64 blue
pixels. The full strict private suite passed 244 tests with two filtered; three
older shader cases skip hardware internally. HAL Clippy passed with ten
existing test warnings. Arbitrary non-suffix sampled views and mip attachment
views remain unsupported, and public adapter exposure remains disabled.
Checkpoint: `34794bb52b7389492ffcbd989c09d4de8121c95f`.

POT sampled mip textures now support COPY_DST|RESOURCE without requiring
COPY_SRC. The native texture has only shader-resource binding; exact usage
checks continue to reject copy-source and direct readback. Driver mip and
sample support remain required. Strict sampled-mip GPU tests passed 68/68
executions across RGBA8/BGRA8 with zero format skips, including seven new
uploads, suffix selections and untouched-mip zero checks per format. Controlled
core fixtures sampled 64/64 blue uploaded pixels and 64/64 transparent zero
untouched pixels. Synthetic model and native bind-flag/usage tests passed.
The full strict suite passed 244 tests with two filtered and three older shader
hardware cases skipped internally. HAL Clippy passed with ten existing test
warnings, and the core zero-adapter test passed. An independent review found no
concrete issue. Public enumeration stays off.
Checkpoint: `4a4b6eec90c12a8d3ba35629311ab1d58e052284`.

Next: individual FL9 2D render-target mip views. Core creates an internal
attachment view for each mip, including on render-only textures; native RTV
descriptors and render-pass extents must select the actual mip. Keep sampled
and attachment mip usage disjoint and preserve whole-resource alias checks.

Individual POT RGBA8/BGRA8 color mip attachments now use explicit 2D RTV mip
slices and mip-relative render extents, for COLOR_TARGET with optional copy
usages. RESOURCE and TRANSIENT combinations remain unavailable. Strict RTX
FL9_1 private execution passed both formats, verifying all chain pixels, padding,
selected clears/draws, untouched initialization, invalid views and lifetimes.
Controlled core execution checked 43/43 pixels for each format and separately
submitted render-only mip clears. Independent production review found no
concrete issue. The full strict private suite passed 245 tests, with two filtered
and three older shader hardware cases skipped internally. HAL Clippy passed
with ten existing test warnings, and ordinary core adapter enumeration stayed
empty. Historical devices and public adapter exposure remain unverified.
Checkpoint: `68c6a804d13cada3d1cb0feccef6bbb3d1d58d7f`.

Next: combined sampled and color-attachment mip textures in separate passes.
FL9 permits a combined SRV/RTV view only for the last mip, since an RTV selects
one level and an SRV must include the resource's last level. Preserve explicit
usage restrictions and reject whole-resource feedback; core-valid non-last
combined views remain a public integration gap.

POT RGBA8/BGRA8 textures now permit combined RESOURCE|COLOR_TARGET usage with
optional copy flags. RESOURCE views are suffixes, attachment views select one
mip, and a dual-use view selects only the last mip. Strict RTX FL9_1 private
testing passed six GPU transition cases across both formats, including feedback
rejection and resource lifetime; controlled core testing passed four cases
with 64/64 expected pixels each. Whole-resource alias rejection remains more
restrictive than core for disjoint mips. An independent review found no
concrete issue. Public adapter exposure remains disabled.
The first full-suite run had 245 passes and one failure because an older
negative test still expected a two-mip combined-usage texture to be rejected.
That assertion now checks a four-level chain on a 4x4 texture, exceeding the
valid mip count; its focused hardware test passed.
The rerun passed 246 tests with two filtered and three older shader cases
skipping GPU execution internally. HAL Clippy passed with ten existing test
warnings; ordinary core D3D11 enumeration remained empty.
Checkpoint: `2206731ded87a5cdc1c9bb3c557191027c10397b`.

Next investigation: finite literal textureSampleBias within the documented
legacy instruction range. Preserve selected-entry validation, reject dynamic
or out-of-range biases and account for anisotropic and LOD semantics before
allowing it; hardware execution and compiler evidence are required.

Selected FL9 shader entries now accept offset-free finite literal sample biases
in [-3,3]. Used pairs conservatively require equal min/mag filters and
anisotropy 1; dynamic values, helper arguments and offsets still reject. The
strict RTX test passed nine 64-pixel GPU renders, including fractional linear
mip filtering, with invalid shader and direct/indexed sampler checks. A core
fixture produced 64/64 blue pixels using a literal bias. The full strict suite
passed 249 tests before the final test-only linear/indexed additions; those
passed in the updated focused test. Three older shader cases skip hardware
internally. HAL Clippy retained ten pre-existing test warnings, the DX12 HAL
compiled, and ordinary core adapter enumeration remained empty. An independent
review found no concrete defect. Historical driver behavior remains unverified.
Checkpoint: `d62f52eaca76e4a8ff50eabbc9ee09910d94ffb5`.

Next investigation: emulate core-valid non-suffix sampled mip views without
stale contents, feedback bypass or unsupported native FL9 SRVs. A per-view
copy must reflect execution-time writes, preserve original resource identity,
and not unbind unrelated shader state. Keep public exposure disabled until all
core-visible view and presentation contracts are safe.

After the bias checkpoint, the gated public-wgpu regression passed again on
the RTX FL9_1 device: the indexed textured-depth fixture read 32 red and 32
green pixels. This nondefault unsafe fixture does not enable public enumeration.

Non-suffix sampled POT RGBA8/BGRA8 views now have a private GPU-copy emulation:
a view-specific truncated texture is refreshed from the retained original just
before each draw, then all active shader resources are rebound. The original
identity remains authoritative for whole-resource feedback checks. Strict RTX
FL9_1 tests passed 16/16 GPU cases across both formats, and the core fixture
read 64 yellow pixels after an update following view creation. The full strict
suite passed 250 tests with two filtered; three older shader cases skip GPU
execution internally. A build initially exposed non-Send retained IUnknown and
test closure lifetime errors; typed resource retention with transient canonical
identity checks and independently scoped test borrows corrected them. An
independent review found no concrete defect. The updated focused test includes
recorded upload followed by indexed sampling in one submission, with input
buffers destroyed before submit, and passed for both formats. HAL Clippy passed
with ten existing test warnings; the zero-adapter core test passed. The full
suite preceded those final test-only additions. Each use incurs GPU copies and
clone memory. The unsafe public-wgpu fixture again passed its indexed textured
depth check with 32 red and 32 green pixels. Historical drivers and public
adapter exposure remain unverified.
Checkpoint: `9e9c21392b0996c1ea2b6489c11c268c37379206`.

Next: use that clone path for a single non-final mip view with both RESOURCE
and COLOR_TARGET usages, with rendering and sampling in separate passes.
Keep the original-resource feedback guard and validate fresh copies after render.

Non-final single-mip dual RESOURCE|COLOR_TARGET views now use the original RTV
and a refreshed clone SRV. The strict RTX FL9_1 test passed eight GPU cases over
both color formats, including repeated render/sample transitions, early resource
destruction and feedback rejection. Controlled core default-view cases each
matched 64 blue pixels for RGBA8 and BGRA8. Independent review found no concrete
defect. The full strict suite passed 251 tests with two filtered and three older
shader cases skipping GPU execution internally. HAL Clippy passed with ten
existing test warnings; core's zero-adapter test passed. Same-pass whole-resource
feedback and public enumeration remain gated.
Checkpoint: `b431ff5d013dc9050f729b00a92973f961676ff1`.

Next investigation: whether FL9 and core permit simultaneous non-overlapping
mip sampling and attachment use, and whether clone refresh can be ordered
without reading a resource while its output view is bound. Preserve conservative
whole-resource rejection until the native and core contracts are established.

D3D11's documented disjoint-subresource contract supports a restricted private
case: a native suffix SRV may be sampled while a different mip of the same
texture is attached. The entire sampled view range is checked, not only shader
accesses; clone-backed same-resource views remain rejected. The first strict
test passed RGBA but failed BGRA because the test used an RGBA-only diagnostic
readback. Replacing that with recorded texture-to-buffer readback yielded six
of six GPU cases across both formats, with no skips. Controlled core fixtures
also matched 64 pixels per format. Historical FL9 hardware is unverified and
the public adapter remains disabled. The full strict private suite passed 253
tests with two filtered; three older shader cases skip GPU execution internally.
HAL Clippy passed with ten existing test warnings, and the core zero-adapter
isolation test passed. Independent review found no concrete alias-check bypass.
Checkpoint: `07eaee830f8600b1b56391089ab3df03a20bcddf`.

Next investigation: disjoint clone-backed views of the current attachment.
Copies must not read the source while its RTV is bound unless the native
contract guarantees it; unbinding and restoring output views must preserve
depth, surface lifetime and all other pipeline state without re-clearing.

Disjoint clone-backed sampled mips are now permitted. During clone refresh,
replay snapshots and unbinds the current single RTV and optional DSV, clears
PS SRVs, copies, restores the attachments without clearing, then rebinds active
sampling resources. Strict RTX FL9_1 tests passed both RGBA/BGRA depth cases,
each reading 32 green and 32 blue pixels after repeated refreshes; controlled
core cases matched the same depth-dependent results. A strict hidden-HWND test
sampled a clone, read BGRA [0,0,255,255], presented successfully and resized
with the completed command buffer retained. Visible presentation and the
synchronous window-callback reentrancy blocker remain unresolved. Independent
review found no concrete defect under the current one-RTV, PS-only restrictions.
The full strict private suite passed 255 tests with two filtered; three older
shader cases skip GPU execution internally. HAL Clippy passed with ten existing
test warnings, and core's ordinary zero-adapter test passed.
Checkpoint: `3e0fdcb9f9e8330e7cb900a021303d0e6d22c878`.

Next: a validated Never depth comparison can suppress draws only while shaders
have no observable storage side effects, stencil is inert and queries are
unavailable. FL9_1 guarantees native Always and LessEqual, not every comparison;
do not substitute LessEqual for strict comparisons.

Validated Never-depth draws are now suppressed under the current inert-stencil,
read-only-shader and no-query subset. Strict private and controlled core tests
passed depth-dependent output for both depth-write settings and direct/indexed
paths. The full strict suite passed 257 tests with two filtered; three older
shader tests skip GPU execution internally. HAL Clippy passed with ten existing
test warnings and core's zero-adapter isolation test passed. Independent review
found no concrete defect; future side effects or queries require re-audit.
Checkpoint: `8045637708e23b1d4de8c54f1ddde3d54bf0c6c8`.

Integer vertex input audit: although R8G8B8A8_UINT is a required FL9_1 IA
format, an FXC 10.1 `vs_4_0_level_9_1` probe of uint packing plus `asfloat`
accepted the shader but emitted embedded vs_2_0 float multiply/add instructions
instead of preserving bit reinterpretation. For bytes [63,128,0,0], the intended
bits 0x3f800000 represent 1.0f but the legacy listing writes the numeric
1065353216. The SM4 stream uses integer shifts/adds. This is bytecode evidence,
not historical-GPU execution. The format remains unsupported; audit currently
accepted uint uniforms and bitcasts for the same legacy-path hazard before
expanding integer shader or vertex support.

Follow-up shader audit found this risk is already reachable: FXC accepts an
FL9 pixel shader comparing a uint uniform to 16777217, but the embedded ps_2_0
listing converts the uniform to float and uses 16777216 as the comparison
constant. A float uniform converted to integer before that comparison has the
same issue. Modern RTX forced-FL9 readback cannot validate this legacy stream.
Prioritize a selected-entry fail-closed guard for unsafe runtime integer
operations and bitcasts before public exposure; do not treat earlier RTX
integer-uniform tests as proof of historical-device correctness.

The selected FL9 shader profile now rejects runtime integer values and
operations, conversions and bitcasts, integer-bearing used uniforms and
interfaces, switches, and unproven initializer/override forms in both checked
and unchecked modes. Literal/static indices and float-only shaders remain in
the supported subset. Two synthetic guard tests passed, including bounded
analysis and a defensive post-validation IR mutation. Three former integer
GPU tests now assert exact linkage rejection and each render 64/64 float-only
control pixels; six runtime-index rendering cases explicitly reject and skip
GPU execution. The full strict private suite passed 259 tests with two filtered.
The controlled core fixture and unsafe public-wgpu fixture still rendered
successfully, the latter producing 32 red/32 green pixels. HAL Clippy passed
with ten existing test warnings and ordinary core D3D11 enumeration stayed
empty. Independent review found no concrete bypass within the current subset;
the guard remains conservative and does not establish general integer support.
Checkpoint: `3de27b02b7659b028db3606e0e3047e07f9e75b8`.

Next: investigate nonindexed point-list pipelines with supported vertex streams.
Keep indexed points rejected at FL9_1, verify actual pixel placement and
first-vertex/stream bounds, and preserve point batching and instance ordering.

Nonindexed point pipelines now support the existing vertex and instance stream
subset. Vertex streams rebase for each native batch of at most 65,535 points;
indexed points remain rejected. The strict private test passed three full-image
RTX FL9_1 executions, including sparse slots, nonzero first vertex, a point to
triangle transition and 65,537 points across two instances with first_instance
1. The controlled core fixture matched all 64 pixels. The initial private
baseline failed at pipeline creation as expected; the first core attempt hit a
fixture-only inter-stage limit, corrected from 1 to 2 before the passing run.
The full strict suite passed 261 tests with two filtered; six dynamic-index
shaders are rejected and their GPU renders skipped. HAL Clippy passed with ten
existing test warnings and core's zero-adapter test passed. An independent
review found no concrete issue. Historical drivers are unverified.
Checkpoint: `4cba5bf6fb21ac23699fe1b012808d0852c6f0e9`.

Next: investigate indexed point-list emulation. Native FL9 indexed point draws
are forbidden; an ordered GPU-authoritative index readback and prevalidated
nonindexed runs may preserve point order, with a staging stall and potentially
many draws. Do not reuse triangle grouping or silently mishandle zero-stride
and negative effective indices.

Indexed point lists now use ordered, rebased nonindexed runs, with the GPU's
index contents staged and all stream ranges checked before any native draw.
The index-independent zero-stride and instance-only case avoids staging after
validating index-buffer and stream ranges. This relies on the existing ban on
index builtins, observable shader side effects and queries. A strict RTX FL9_1
test passed twelve full-image cases, including u16/u32 indices, signed bases,
repeated and reordered indices, GPU updates, instances and invalid late fetches.
The core fixture matched 64 pixels for each index format. Before implementation,
the new test failed on the expected unsupported draw. The first full suite run
found an old test expecting indexed points to fail; its negative case now checks
an out-of-range index binding. The strict suite then passed 266 tests, with two
filtered; six runtime-index shaders reject and skip their GPU renders. HAL
Clippy passed with ten pre-existing test warnings, and the core isolation test
passed. The unsafe public-wgpu fixture still rendered 32 red and 32 green pixels.
Historical drivers are unverified. The implementation can stall for GPU index
staging and use one native draw per scattered index.
Checkpoint: `8c2be917a31a0e124007b77989e2365a83fc376a`.

The private D24S8 triangle path now accepts signed constant depth bias when
slope and clamp are zero. Nonzero bias with selected `frag_depth`, nonzero or
nonfinite slope/clamp, and biased point pipelines remain rejected. Five strict
RTX FL9_1 full-image cases passed for positive, negative and zero bias, writes
and transitions. The core fixture matched all 64 pixels for each bias sign.
Pipeline tests rejected direct and structured fragment-depth outputs with bias;
zero-bias structured output compiled, while FXC rejected the direct-only output
with X4530 because the pixel shader did not write SV_Target0. A synthetic test
checked selected entry points and descriptor boundaries. The strict suite
passed 269 tests with two filtered; six dynamic-index GPU cases remain skipped.
Clippy passed with ten pre-existing test warnings, the core zero-adapter test
passed, and the targeted DX12 HAL library tests passed 2/2. Historical FL9 bias precision
is not established: Microsoft documents 32-bit bias arithmetic for FL10+.
No pristine failing GPU baseline was captured before implementation; the prior
code rejected all non-default depth bias, and the earlier full suite passed.
Checkpoint: `064d68dc2d98573b28730de5ba5e5f9c3aa6f83f`.

Next: allow and verify copies between different mip subresources of the same
texture. Core permits them but the private HAL currently rejects all self-copy.

Same-texture copies between different mip subresources now pass the existing
checked GPU copy path; identical canonical COM resources with the same native
subresource remain rejected. Before the change, the strict GPU test failed at
command encoding. Afterward, RGBA8 and BGRA8 passed partial copies in both
directions with complete padded readback, and each passed a render, same-texture
copy and clone-sampling sequence producing 64 blue pixels. The core fixture
verified every mip texel for both formats and rejected same-mip copies. The
private strict suite passed 271 tests, two filtered; six dynamic-index GPU cases
remain skipped. Clippy passed with ten pre-existing warnings, and the core
zero-adapter isolation test passed. An independent
review found no concrete ownership, identity or copy-contract defect. The
native source and destination are DEFAULT textures and use an explicit box;
historical FL9 drivers remain unverified.
Checkpoint: `ddb1b7cd9d559af88ee9d16c9d73a69b1101edc0`.

Next: investigate valid mipmapped COPY_DST-only texture creation, which core
permits but the current private usage matrix rejects.

Exact COPY_DST-only POT mip chains now use the existing copy-mip capability
gate without exposing source, sampled or attachment usage. The strict baseline
failed at texture creation before this change. Afterward, both RGBA8 and BGRA8
passed three uploaded levels and an untouched zero level on the forced-FL9_1
RTX. A test-only native DEFAULT-to-DEFAULT copy into a separate readable
texture verified all mip pixels and padding; this does not grant COPY_SRC to
the original. The controlled core fixture queued three writes, waited for
completion and checked view and readback usage rejection for both formats; it
does not claim core readback of the upload-only resource. Its first run exposed
a test assumption that core transfer validation happens at recording; the test
now asserts the exact missing-COPY_SRC error at command-buffer finish. The full
strict suite passed 273 tests with two filtered; six dynamic-index GPU cases
remain skipped. Clippy passed with ten existing test warnings, and core's
zero-adapter isolation test passed. An independent
review found no concrete usage leak or diagnostic copy defect. Historical FL9
drivers remain unverified.
Checkpoint: `735a9755645b54c06c9a1084ebf66ef49e4848e8`.

Next: investigate accepting a non-filtering sampler layout with a nearest
sampler, which core permits but the current private layout rejects.

NonFiltering sampler layouts now preserve their class in binding-plan identity
and accept only descriptors whose min, mag and mipmap filters are all nearest.
Filtering layouts continue accepting nearest or linear samplers. Sparse sampler
numbers retain the existing native register mapping; mixed sorted bindings and
reversed resource indices are checked. The strict baseline failed at layout
creation before the change. Two strict FL9_1 full-image executions passed,
including a filtering/non-filtering pipeline transition, and the controlled
core fixture matched 64 pixels and rejected a linear sampler with the expected
WrongSamplerFiltering error. Synthetic tests cover layout identity, descriptor
classification and mip-only linear filtering. The full private suite passed
277 tests, two filtered; six dynamic-index GPU cases remain skipped. Clippy
passed with ten existing warnings and core's zero-adapter test passed. The first
test compilation exposed a fixture lifetime error, which was fixed. Independent
review found no concrete mask, sorting or resource-matching defect.
Checkpoint: `7404886b4dd14da39c4859b5a33243c31f9ddd36`.

Next: support explicit unfilterable-float texture layouts only with selected
shader pairs that use NonFiltering sampler layouts. Preserve layout identity;
actual nearest sampler state alone cannot justify a Filtering layout.

Explicit Float{filterable:false} layouts now retain texture class in binding
identity. Selected Naga sampling pairs are checked per stage against logical
texture and sampler layouts, with unresolved or inconsistent slots failing
closed. A used unfilterable texture with a Filtering sampler layout is rejected
even if the actual sampler is nearest; unused incompatible pairs remain legal.
The strict baseline failed at layout creation. Two forced-FL9_1 full images
passed, including helper-function sampling and pipeline transitions; the bad
pair returned the exact fragment linkage error. The core fixture matched 64
pixels and rejected the corresponding invalid pair. CPU tests cover selected
entries, helpers, cross-group identities, layout masks and malformed mappings.
The initial CPU fixture used a reserved WGSL identifier and was fixed. The full
private suite passed 281 tests with two filtered; six dynamic-index GPU cases
remain skipped. Clippy passed with ten existing test warnings, and core's
zero-adapter isolation test passed. Independent
review found no concrete pair-validation bypass. No new texture formats or
public capability exposure were added.
Checkpoint: `4ef1fbdaafd138b5981b5765c6a33a887b54b594`.

Next: examine accepting combined vertex/fragment visibility for sampled
resources used only by the fragment shader, without enabling vertex sampling.

Sampled texture and sampler entries now accept VERTEX|FRAGMENT visibility as
well as FRAGMENT. Sorted per-binding visibility masks remain part of layout
identity; stage maps retain the original compact slots. Actual vertex-stage
sampled-resource use is still rejected. The pre-change strict baseline failed
at layout creation. Two strict FL9_1 full-image renders passed, including a
visibility/layout transition; the selected vertex sampler returned the expected
vertex linkage error. The core fixture matched all 64 pixels. Compiler tests
also reject selected vertex sampling, loads and dimension queries while
accepting unused declarations; their shared guard is mode-independent, but
the new test does not independently exercise both ShaderModule check modes.
The full private suite passed 284 tests, two filtered; six dynamic-index GPU
cases remain skipped. Clippy passed with ten existing test warnings and core's
zero-adapter isolation test passed. Independent review found no concrete
visibility, slot or shader-guard defect. No vertex sampling is advertised.
Checkpoint: `4f3f608c229fd6e5b7bc309db5ac975bfd790c7e`.

Exact premultiplied-alpha blending now uses coupled ONE/INV_SRC_ALPHA add
factors for color and alpha on supported linear RGBA8 and BGRA8 targets. The
strict baseline rejected the pipeline before implementation. Six strict FL9_1
RTX full-image cases passed, including opaque transitions; a controlled core
render matched 64 pixels with one-byte rounding tolerance. Its first attempt
found the unsafe fixture did not expose queried blendability; the fixture now
reports it only on a positive format query and explicitly remains restricted.
An observed core pixel was [128, 0, 63, 191]. A later full-suite run found
three stale tests expecting this blend to be rejected; they now check an
unsupported neighboring blend state. The full private suite passed 286 tests,
two filtered; six dynamic-index GPU cases remain skipped. The focused test
passed again after adding its missing format prerequisite. The fixture's
synthetic query test, core zero-adapter test, and Clippy passed; Clippy retained
ten existing test warnings. The initial test launch hit Windows paging-file
exhaustion before PowerShell started; retrying with two Cargo jobs succeeded.
Independent review found no concrete factor or capability isolation defect.
Full-channel straight-alpha and general separate-alpha remain unsupported;
historical FL9 execution and sRGB blending are unverified.
Checkpoint: `cac502305de4c2569bfc29da73eecdd9a59e92e8`.

Core shader-passthrough backend selection now returns NotCompiledForBackend for
Dx11 instead of reaching an unreachable panic. The feature gate still rejects
passthrough on the real controlled device; the fixture verified MissingFeatures
for empty and HLSL descriptors without fabricating unsupported device features.
A pure selection test verified the Dx11 and Noop errors and existing Vulkan,
DX12, Metal and GL source selection and precedence. The controlled strict
FL9_1 core fixture and targeted selection test passed, as did targeted core
Clippy after fixing a new test-only warning. The same selection test passed
with the DX12 core feature; that build emitted 15 warnings, primarily unused
items in the existing timestamp-normalization module. Ordinary adapter exposure remains
disabled. The public map-error path remains a substantive blocker: fallible
D3D11 staging/map work is behind a void HAL invalidate interface, so failures
currently panic. A correct remedy needs error propagation through HAL and core;
silently swallowing errors or marking a shadow coherent would be unsound.
Checkpoint: `ceaf4d603dc4ecbc6449b520f5a9ef546b5cfa64`.

HAL mapped-range invalidation now returns DeviceError across backends. Core
unmaps a failed read mapping before returning the error and does not expose the
pointer; BLAS compaction also unmaps and does not read its size on failure.
Vulkan now propagates its native invalidation error instead of unwrapping it.
A feature-gated one-shot fault on a real D3D11 buffer verified an OOM callback
and released shadow mapping in the controlled FL9_1 core fixture. The fixture
passed after a test-only label type correction. Strict private D3D11 tests
passed 286 tests with two filtered (six dynamic-index GPU cases still skip),
and the persistent shadow mapping test passed again after a reviewer removed a
test violating the unsafe HAL range precondition. Windows HAL and core checks
with dx11, dx12, vulkan and gles features passed; the core check emitted 15
warnings in the no-WGSL configuration. DX12 HAL tests passed 2/2, core adapter
isolation passed and targeted HAL/core Clippy passed with ten existing HAL test
warnings. Metal was not compiled on this Windows host. BLAS invalidation failure
was reviewed but not exercised on ray-tracing hardware. Flush and unmap failure
handling remain separate work, as does existing device-lost callback reentrancy
while core mapping locks may be held. Public adapter exposure remains disabled.
Checkpoint: `a690866b4a6cdc0981a1ffa448f2c28f0f41b756`.

Mapped-range flush now returns a HAL `DeviceError` across all backends. D3D11
shadow synchronization and Vulkan's native flush errors propagate; coherent or
implicitly coherent backends return success. Core mapped-at-creation writes,
direct writes, unmap staging, queue uploads, texture uploads, indirect draw
validation and acceleration-structure staging handle errors without enqueueing
failed staging data. Failed staging allocations are unmapped and destroyed;
failed direct mappings are unmapped and core map state returns to Idle. Device
loss callbacks are deferred until snatch/encoder locks are released, and the
strict D3D11 fixture verified callback reentrancy, resource destruction and
nested polling after an injected flush OOM. Queue write paths now release locks
in reverse acquisition order.

On the RTX 3090 Ti (vendor 4318, device 8707), strict mode requested and
asserted FL9_1. Both focused core hardware fixtures passed: the broader
render/transfer fixture also exercised injected invalidation failure and a
second pending map after device loss; the new mapped-write fixture verified
OOM propagation and shadow cleanup. The full core library suite passed 70/70,
including zero public D3D11 adapter exposure. The private D3D11 HAL suite
passed 286 tests with two filtered (six existing dynamic-index hardware cases
remain skipped). Cross-backend core checks for dx11/dx12/vulkan/gles and
dx11/replay passed; targeted DX12 HAL tests passed 2/2. Clippy passed with ten
existing HAL test warnings. Targeted rustfmt checks passed on all 20 changed
Rust files and `git diff --check` passed. Cross-backend checking without WGSL
emitted 15 existing unused/unreachable warnings.

`RUSTFLAGS=--cfg wgpu_validate_locks` remains unverified: after fixing the
queue-write LIFO release exposed by the first run, the full fixture reached an
existing `DEVICE_COMMAND_INDICES` to `COMMAND_BUFFER_DATA` lock-rank TODO. The
narrow flush fixture also encountered the ranked-lock checker while dropping
the fixture's `buffer_as_hal` guard. Metal was not compiled on this Windows
host. Public adapter exposure remains disabled.
Checkpoint: `13a4defb9` (`core: propagate mapped buffer flush failures`).

The next planned investigation was DXGI/window-procedure reentrancy during
`Present`; the outcome is recorded below. Keep ordinary D3D11 enumeration
disabled until this and the remaining core/HAL capability mismatches are
resolved.

The presentation audit confirmed the existing open gate. In
`wgpu-core/src/present.rs`, `Queue::present` holds `device.command_indices`
while calling HAL; `CoreWindowFrame::present_for_hal` holds
`NativeDevice::immediate` while calling DXGI. Microsoft documents that DXGI
may synchronously `SendMessage` to the window procedure and wait for the
message pump, and that immediate-context calls must be serialized with DXGI
operations on the same thread. Therefore dropping the HAL mutex alone permits
invalid concurrent context/DXGI use, while retaining it can block a callback
that reenters the device. The core queue-index lock creates another reentrancy
edge. `ID3D11Multithread` protects against calls from other threads but does
not remove the core lock ordering or make a cross-thread synchronous message
safe. `DXGI_PRESENT_DO_NOT_WAIT` is not established to avoid synchronous
window-message waits, so it is not treated as a fix. References:
[DXGI best practices](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices),
[D3D11 multithreading](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-intro),
[ID3D11Multithread::Enter](https://learn.microsoft.com/en-us/windows/win32/api/d3d11_4/nf-d3d11_4-id3d11multithread-enter),
[Present](https://learn.microsoft.com/en-us/windows/win32/api/dxgi/nf-dxgi-idxgiswapchain-present).
No safe backend-only change was identified; keep public surface exposure gated.

## One-sided FL9_1 stencil operations

The private D24S8 pipeline now maps stencil comparisons, operations, eight-bit
read/write masks and dynamic references to D3D11 state. Reference changes are
recorded, replayed against the selected depth-stencil state and reset at render
pass boundaries. Microsoft documents that FL9_1 does not support two-sided
stencil, so active pipeline states must use identical front and back faces;
asymmetric states reject at pipeline creation. The internal model records the
FL9_1 two-sided restriction and private stencil implementation without setting
the broad public depth-stencil capability.

Depth `Never` with active stencil is emulated using the documented FL9_1
Always-depth guarantee: the stencil pass operation becomes the requested
depth-fail operation, while color and depth writes are disabled. A strict GPU
test proved the write indirectly with a following Equal stencil test. The other
case still validates and suppresses draws when stencil is inert. The HAL cannot
read stencil storage directly. State translation maps all comparison and
operation enums, but physical evidence on the RTX covers Replace/Equal only;
historical hardware and all other active state combinations remain unverified.

Validation on the NVIDIA RTX 3090 Ti (vendor 0x10de/4318, device
0x2203/8707, driver 32.0.16.2101), with software adapters prohibited, requested
and selected FL9_1:

- Focused `stencil` filter: 3 passed (2 CPU-only mapping/classifier tests and
  one real-device test). Readback verified left 32 blue/right 32 black pixels
  after Replace/Equal/reference transitions, and 64 blue pixels after the
  Never-depth-fail stencil write.
- Complete strict D3D11 HAL module suite: 287 passed, 0 failed, 0 ignored,
  2 filtered. The focused filter passed again after the final `Copy` cleanup in
  the test.
- Core `requesting_dx11_does_not_expose_a_stub_adapter`: 1 passed.
- Targeted HAL Clippy passed with ten existing warnings; changed Rust files pass
  `rustfmt --edition 2021 --check`, and `git diff --check` passes.
- A targeted DX12 HAL test build and a one-job DX12 `cargo check` both stopped
  when rustc could not map standard-library metadata because Windows returned
  OS error 1455 (paging file too small). This is an environment failure; DX12
  regression is not verified by this increment. No public D3D11 adapter or
  historical FL9_1 device was enabled or tested.

Next: test this one-sided stencil subset through the unsafe, non-enumerated core
fixture, then continue the outstanding core/HAL contract audit. This does not
unblock ordinary public adapter exposure or the unresolved HWND presentation
reentrancy gate.
Checkpoint: `ae40c514a` (`dx11: support one-sided FL9_1 stencil operations`).

The same restricted stencil behavior now runs through `wgpu-core`'s unsafe,
feature-gated D3D11 fixture. A new case writes reference `0xa5` with a symmetric
Always/Replace pipeline whose color writes are masked, then switches to a
symmetric Equal pipeline and renders the image. The expected 64-pixel output
only appears if the core-recorded reference, stencil attachment, selected
pipeline state and HAL replay all agree. The fixture still asserts ordinary
HAL enumeration is empty; no normal adapter path was enabled.

On the physical NVIDIA RTX 3090 Ti (vendor 0x10de, device 0x2203), the fixture
requested and selected FL9_1 and the new ActiveStencil case matched all 64
pixels. The complete fixture-enabled core library suite passed 70/70 tests,
including its zero-adapter isolation test and both controlled hardware tests.
The focused fixture also passed independently with `WGPU_DX11_REQUIRE_HARDWARE=1`.
This verifies core-to-HAL stencil state on this modern GPU restricted to FL9_1;
historical FL9_1 behavior remains unverified. Rust formatting and
`git diff --check` passed.

Next: audit core-valid read-only depth/stencil attachment use and the remaining
private HAL render-pass contract. Keep public adapter and surface exposure
disabled while presentation reentrancy and truthful full capability reporting
remain unresolved.
Checkpoint: `cff2cc29b` (`core: test FL9_1 stencil through controlled fixture`).

The direct HAL read-only D24S8 contract now tracks effective stencil writes as
well as depth writes. Pipeline state records whether the final native stencil
operations can mutate bits after no-op canonicalization and the FL9_1 Never
remapping. Selecting such a pipeline in a read-only attachment pass rejects
before native replay. This closes a gap previously covered only by core's
`WRITES_STENCIL` validation. The read-only guard applies to the whole D24S8
attachment because the current private descriptor accepts READ usage only with
LOAD|STORE on both aspects; aspect-specific read-only usage remains unsupported.

Strict hardware validation on the RTX 3090 Ti at requested/selected FL9_1
passed the focused read-only test, preserving the 32 green/32 blue depth result
and rejecting both depth-writing and stencil-writing pipelines. The full
private D3D11 suite passed 287/287 tests (two filtered). The fixture-enabled
core suite passed 70/70, including the active-stencil core render and ordinary
zero-adapter regression. Targeted HAL Clippy passed with ten existing warnings;
rustfmt and `git diff --check` passed. This modern forced-FL9_1 device does not
verify historical drivers. DX12 was not rerun for this increment; its previous
targeted check was blocked by Windows paging-file error 1455.

Next: audit render-pass attachment usages and query/resolve paths against what
core can record, prioritizing a specific case that can be rejected or safely
implemented without broad capability claims. HWND public presentation and
ordinary D3D11 adapter exposure remain blocked by reentrancy and capability
gaps.
Checkpoint: `5ea3045f9` (`dx11: reject stencil writes on read-only attachments`).

The read-only regression now also covers a pipeline whose depth comparison is
Never and whose requested stencil depth-fail operation is Replace. The backend
remaps that operation to native stencil pass; the read-only guard rejects it
based on the effective descriptor. The strict forced-FL9_1 GPU test passed
again, while its preserved-depth image remained 32 green/32 blue. Clippy passed
with the same ten existing warnings; `rustfmt --edition 2021 --check` and
`git diff --check` passed. No production behavior changed in this follow-up.
Checkpoint: `f2e746d83` (`dx11: test remapped stencil writes on read-only depth`).

The render-pass audit found the unsupported query, resolve, multisample and
multiview paths are blocked at both relevant boundaries. The D3D11 HAL rejects
query commands, resolve targets, multisampled passes, multiview, multiple color
targets and pass query descriptors. The controlled core fixture exposes no
occlusion downlevel flag, no timestamp feature, no multisample format flags and
one array layer; it already asserts that query creation fails before HAL use.
A new 4x RGBA8 texture request asserts core returns
`InvalidMultisampledFormat`, which also prevents a legal resolve source from
being formed. No capability was enabled to make these tests pass.

With `WGPU_DX11_REQUIRE_HARDWARE=1`, the fixture-enabled core library suite
passed 70/70 tests on the physical RTX 3090 Ti forced to FL9_1; this includes
real offscreen GPU work, active stencil through core, query validation,
zero-adapter isolation and the new CPU-side multisample rejection. Core
Clippy passed without warnings. Ordinary HAL enumeration remained empty. The
4x case does not itself execute GPU work, and the HAL's negative resolve and
multiview branches are structurally guarded but not independently invoked from
core because the adapter contract does not expose their prerequisites.

Next: exercise the already-supported depth-only pass through the controlled
core fixture, then continue the broader render-pass and device-loss audit.
Public adapter/surface exposure remains gated by truthful full validation and
DXGI reentrancy.
Checkpoint: `8d8877e7a` (`core: reject unsupported FL9_1 multisample textures`).

The controlled core fixture now also performs a depth-only prepass with no
color attachments, then begins a color pass that LOADs the depth it wrote and
draws farther geometry. Strict RTX 3090 Ti execution at requested/actual
FL9_1 read back 32 blue pixels where the prepass depth rejected the farther
draw and 32 green pixels where depth remained clear. This proves the core
attachment transitions and HAL replay preserve depth across the two passes;
it is not general depth/stencil API support. An initial probe used depth Equal
and was rejected at pipeline creation, as intended by the restricted
comparison contract; the final test uses supported LessEqual.

The focused core fixture passed in strict hardware mode; the complete
fixture-enabled core suite passed 70/70, including ordinary zero-adapter
isolation. Core Clippy passed without warnings. Rust formatting and
`git diff --check` passed. Checkpoint: `a13699edd` (`core: test FL9_1 depth-only
render prepass`).

Next: continue auditing device-loss and command submission error paths around
render passes. Public adapter exposure remains gated; this core fixture is
unsafe, test-only and still confirms ordinary enumeration is empty.

The command/device-loss audit found that pipeline creation classified
`DXGI_ERROR_DRIVER_INTERNAL_ERROR` as `DeviceError::Lost`, but device, queue and
surface paths did not. Microsoft documents this HRESULT as a driver failure
that puts the device into the removed state. A shared HRESULT classifier now
handles it consistently with device hung/removed/reset and preserves
out-of-memory and unexpected-error distinctions. Optional viewport shader
creation still treats `E_INVALIDARG` as a nonfatal unsupported variant, and
surface errors still map ordinary failures to `SurfaceError::Other`.

CPU regressions passed for the shared classifier, queue mapping, surface
mapping and optional viewport shader error handling. The strict D3D11 suite
passed 289/289 tests with two filtered on the RTX 3090 Ti at requested/actual
FL9_1; several expected unsupported-shader cases report a deliberate rendering
skip after asserting rejection. The fixture-enabled core suite passed 70/70
with strict hardware required, and its focused public-wgpu/core fixture also
passed on the same hardware, including texture transfers and all depth/stencil
render cases. `cargo build --offline -p wgpu-hal --no-default-features
--features dx11` passed. HAL Clippy passed with the same ten pre-existing
warnings. Formatting and `git diff --check` passed. These tests validate
classification logic, not recovery from an induced physical device removal.

Checkpoint: `da035664154d080ecb80a3518cbfa6e4eecbe4d8` (`dx11: classify
driver-internal failures as device loss`). Device teardown, loss during live
presentation and debug-layer diagnostics remain unaudited; public adapter and
surface exposure remain gated.

The prior vertex constant probe referenced only one statically selected array
element and did not measure the profile range. It now compiles a dynamically
indexed `float4` constant array: FXC accepts length 255 and rejects lengths 256
and 257 with X4507 at constant register index 256. FXC accepts 32 referenced
pixel constants and rejects 33 with X5200 at register 32. With strict hardware
required, the RTX 3090 Ti opened at requested/actual FL9_1 and accepted native
`CreateVertexShader` for the 255-element bytecode. This test creates but does
not execute that shader; the private WGSL path rejects runtime integer indexing,
so near-limit GPU execution remains an explicit gap. The first two attempts at
static all-element reductions exceeded the vs_2_0 instruction budget and were
replaced with a dynamic-index compiler probe rather than weakening the profile.

The strict `constant` HAL filter passed 8 tests (284 filtered), including the
profile compiler boundary, native shader creation, constant depth-bias and
sample-bias renders, and uniform materialization. `fl91_vertex_constant_profile_boundary_creates_on_hardware`
reported the physical NVIDIA RTX 3090 Ti (vendor 0x10de, device 0x2203) and
requested/actual FL9_1. The complete strict private D3D11 HAL suite then passed
290 tests with two filtered in 73.84 seconds; it included the new shader-object
creation test. The focused strict shader-object test was rerun after adding its
explicit PASS line and passed. `cargo build -p wgpu-hal` passed; HAL Clippy
passed with the same ten existing warnings. Core's
`requesting_dx11_does_not_expose_a_stub_adapter` passed 1/1. Rust formatting
and `git diff --check` passed. The public adapter and HWND surface gates are
unchanged.
Checkpoint: `f775292a2` (`dx11: verify FL9_1 vertex constant limits`).

## 2026-11-03 — non-indexed FL9_1 line topologies

Starting from clean HEAD `f20a0a15f` (`docs: record FL9_1 constant boundary
checkpoint`), the private pipeline now accepts non-indexed line lists and line
strips. Replay batches line lists on even vertex counts and line strips with a
one-vertex overlap, rebasing bound streams for each batch. Indexed line draws
remain rejected. The internal capability model records the two implemented
topologies. Synthetic batch tests cover incomplete tails, instances, invalid
limits and FL9_1 boundaries.

Strict GPU validation passed on the physical NVIDIA RTX 3090 Ti
(`0x10de:0x2203`) requested and selected at FL9_1. A deterministic offscreen
render read back 22 red and 12 green pixels for line-list/line-strip draws; the
list includes a discarded trailing vertex. An indexed line-list command was
rejected before submission. CPU tests verified pair boundaries, one-vertex
strip overlap, instance ordering and maximum FL9_1 batch sizes.

The full strict private D3D11 suite passed 294/294 tests with two filtered.
Its first run exposed an obsolete test expectation that LineList pipeline
creation should reject; the test now verifies successful creation, and the
focused depth regression plus full suite pass. The `line_` filter passed 12/12,
including the real-device render. `cargo build --offline -p wgpu-hal --no-default-features
--features dx11` passed; core's `requesting_dx11_does_not_expose_a_stub_adapter`
passed 1/1. HAL Clippy passed with ten pre-existing warnings. Rustfmt checks
passed for every changed Rust file, and `git diff --check` passed. This feature
The feature is checkpointed as `3db26eed3` (`dx11: add FL9_1 nonindexed line
topologies`). Next: exercise these topologies through the controlled wgpu-core
fixture, then assess indexed line-list support. Public adapter and surface
exposure remain disabled.

The unsafe, feature-gated core fixture now creates LineList and LineStrip
pipelines through core, records `render_pass_draw`, submits on the injected
physical device and validates exact red sampled pixels against the blue clear.
Strict hardware readback produced 8 red pixels for LineList and 12 for
LineStrip, with all other target pixels unchanged. The fixture still asserts
ordinary HAL enumeration is empty. The complete strict fixture-enabled core
library suite passed 70/70 tests, including ordinary adapter isolation; core
Clippy and formatting checks passed. An independent review found no issue in
the fixture isolation or result assertions. Checkpoint `e79428c50` records this
core-to-HAL coverage. Next, implement and test indexed LineList semantics
without assuming indexed strip restart behavior; public D3D11 exposure remains
disabled.

The core-fixture change was independently reviewed and committed as
`e79428c50` (`dx11: verify FL9_1 line topologies through core fixture`). The
strict focused command and the complete fixture-enabled core library suite both
ran with `WGPU_DX11_REQUIRE_HARDWARE=1`; results were respectively 1/1 and
70/70. Core Clippy passed with no warnings. The fixture continues to check that
ordinary D3D11 adapter enumeration returns no adapters.

## 2026-11-03 — indexed FL9_1 LineList

The private HAL now records indexed LineList draws and replays GPU-authoritative
index data through a shared pair-preserving planner. Uint16 and Uint32 indices
are compacted into ordered Uint16 windows when their effective span is at most
65,534; a wider line pair is gathered into two-vertex scratch streams. Every
effective index is checked against every vertex-rate stream before any native
draw. An invalid fetch discards the whole draw, incomplete final pairs are
ignored, and native calls are capped at 131,070 indices / 65,535 lines. Indexed
LineStrip remains rejected.

Strict hardware execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`), explicitly requested and selected at FL9_1. The indexed
Uint16 case used a nonzero first index and incomplete tail; the Uint32 case
covered a compact window and a wide-pair gather. Readback contained 22 red,
12 green, 24 blue and 14 white pixels. A second submission with an
out-of-bounds fetch left every pixel at the clear value; indexed LineStrip was
rejected before submission. The focused strict test passed 1/1.

The full strict private suite passed 297 tests, with 0 failures and 2 filtered.
The strict fixture-enabled core suite passed 70/70 and retained the assertion
that ordinary D3D11 adapter enumeration is empty. Its focused hardware fixture
reported 8 red pixels for each indexed format and passed 1/1. HAL build passed;
HAL Clippy passed with ten existing warnings and core Clippy passed without
warnings. Rustfmt checks and `git diff --check` passed. The first attempted core
run lacked the Visual Studio linker environment and did not compile; rerunning
through `VsDevCmd.bat` passed. This code path maps the index staging buffer and
may stall; wide pairs also allocate small scratch buffers. Historical FL9_1
hardware remains untested.

Checkpoint `82c9b9228` (`dx11: add FL9_1 indexed line-list rendering`). Next,
analyze fixed-index restart semantics and implement indexed LineStrip only if
it can preserve core draw behavior under FL9_1. Keep ordinary adapter
enumeration and HWND exposure disabled.

## 2026-11-03 — indexed FL9_1 LineStrip emulation

The private HAL now accepts indexed LineStrip pipelines when their declared
`strip_index_format` matches the bound Uint16 or Uint32 index buffer. At replay,
it maps the GPU-authoritative selected index range, splits format-specific
restart markers into independent runs, discards runs shorter than two indices,
validates every effective vertex fetch across all vertex-rate streams, and
compacts each bounded run to Uint16. Long runs overlap one endpoint at the
FL9_1 native draw limit. An adjacent pair whose effective indices cannot fit a
single 65,534-span window is gathered into per-stream scratch buffers and
rendered as a two-vertex line strip. Native FL9 strip-cut support is not used.
The internal capability model records this implementation separately from the
hardware guarantee.

The implementation resolves `baseVertex + index` before testing for the
format-specific restart value and splitting primitive runs, following the
current [WebGPU index-resolution and primitive-assembly algorithm](https://www.w3.org/TR/webgpu/#index-resolution).
FL9_1 does not guarantee native strip-cut support, so this split occurs during
the replay-time CPU index scan before compact R16 runs are submitted. CPU tests
cover a non-marker that becomes a marker through a positive base and a raw
marker that becomes an ordinary, in-range vertex through a negative base. The
core fixture executes both index formats with base 1 and a marker formed by
resolution.

Strict private hardware testing used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`) with requested and selected FL9_1. The combined line test
read back 17 red, 12 green, 24 blue, 14 white, 10 cyan and 14 yellow pixels,
including marker-separated Uint16 and Uint32 strips. The Uint16 strip used a
positive base to create the resolved marker. A second submission verified a
Uint32 wide-edge gather followed by a compact window (15 yellow pixels) and
asserted a pixel on the gathered edge itself. Missing strip format and
pipeline/index-format mismatch were rejected before submission. An
out-of-bounds indexed fetch discarded the whole draw. The controlled core
fixture also passed indexed Uint16 and Uint32 LineStrip through core command
recording, with 12 red pixels each. Planner tests cover resolved-marker runs,
empty/short runs, signed-base rebasing, wide-edge gathering, Uint16 max as an
ordinary Uint32 index and the 65,536 index native strip boundary.

The first planner-test attempt exposed an undersized synthetic vertex range;
the corrected fixture now models the full effective index range. The first
strict GPU attempt had a test-only Uint32 fixture referencing vertex indices
that had not been appended; the backend correctly discarded it as an
out-of-bounds draw. The first core attempt then exposed an index-upload size of
14 bytes, which violates the core queue-write four-byte alignment; an unused
padding index now keeps the fixture upload valid. A separate review caught
that the first GPU wide-pair test used identical gathered endpoints. The test
now uses distinct positions and asserts an interior pixel on that specific
edge. After these fixture corrections, the focused strict HAL and core tests
passed; no product validation was weakened.

The focused strict HAL hardware test passed 1/1, and the focused strict core
fixture test passed 1/1. Final verification on the current implementation then
passed the complete strict private HAL suite (301 passed, 0 failed, 2 filtered)
and the strict fixture-enabled core suite (70 passed, 0 failed), which includes
the ordinary zero-D3D11-adapter isolation test. HAL compilation passed. HAL
Clippy completed with ten existing warnings and no errors; core Clippy passed
without warnings. The targeted DX12 pipeline descriptor regression passed
2/2. Rust formatting and `git diff --check` passed. The strict RTX tests report
the actual NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), request FL9_1 and
assert selected FL9_1. Historical FL9_1 devices remain untested.

Implementation checkpoint: `2ff5a58e8` (`dx11: emulate FL9_1 indexed
line-strip restart`). The checkpoint records the resolved-index restart rule
and checked wide-edge GPU output. The persistent handoff/journal update follows
in a documentation-only commit. Indexed triangle strips, public adapter
exposure and HWND surface exposure remain unavailable.

## 2026-11-03 — indexed FL9_1 TriangleStrip emulation

The private HAL now accepts indexed TriangleStrip pipelines for Uint16 and
Uint32 indices when the pipeline's declared `strip_index_format` matches the
bound index buffer. At replay, GPU-authoritative index bytes are copied to the
existing staging buffer and scanned after earlier commands. The scan resolves
`baseVertex + index` before recognizing the format-specific restart marker,
prevalidates every non-marker fetch against every vertex-rate stream, and
discards the whole draw if any fetch is invalid. Each run is expanded to
triangle-list indices with alternating winding; restart resets the run's
winding parity. Expanded indices are split at the FL9_1 maximum of 65,535
triangles per native draw and passed through the existing compact Uint16
window/gather path. Wide triangles gather the selected bytes from every
vertex-rate stream. Native FL9 strip-cut support is not assumed.

Strict private hardware testing ran on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`) with explicitly requested and selected FL9_1. Indexed Uint16
and Uint32 strips with restart were compared byte-for-byte against equivalent
triangle-list images for both front-face settings and both cull modes. A
separate wide Uint32 triangle requiring gathering read back 32 red pixels.
Mismatched and missing strip-index formats and invalid vertex fetches are
rejected before native draw execution. The controlled core fixture executed
Uint16 and Uint32 indexed TriangleStrip with a restart marker formed by the
signed base vertex; each produced 12 red pixels, and ordinary D3D11 enumeration
remained empty.

Deterministic planner tests passed 3/3 for winding/restart reset, base-resolved
restart values and the primitive-limit/whole-draw-discard boundary. The
complete strict private D3D11 suite passed 304/304 tests with two filtered;
the fixture-enabled core library suite passed 70/70, including the zero-adapter
isolation test. D3D11 HAL build and HAL/core Clippy passed; HAL Clippy reports
ten pre-existing warnings. Targeted DX12 pipeline descriptor tests passed
2/2. The changed Rust files pass targeted rustfmt. Workspace/package format
checks still report pre-existing formatting in the unrelated Naga HLSL writer
and core instance isolation test; those files were left unchanged. The first
test invocation lacked `link.exe`; rerunning through Visual Studio's
`VsDevCmd.bat` succeeded. Staging the index stream can stall the CPU, and
historical FL9_1 hardware remains untested.

Commands (Rust 1.93.1, MSVC environment; hardware commands used
`$env:WGPU_DX11_REQUIRE_HARDWARE = "1"`):

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 indexed_triangle_strip -- --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_nonindexed_triangle_strip_winding_and_first_vertex -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo clippy --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --all -- --check
cargo fmt --package wgpu-hal --package wgpu-core -- --check
rustfmt --check --edition 2021 wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_index.rs wgpu-hal/tests/dx11/triangle_strip.rs
git diff --check
```

The workspace format check reports the pre-existing `naga/src/back/hlsl/writer.rs`
format differences; the package check reports a pre-existing expression layout
in `wgpu-core/src/instance.rs`. Neither file is part of this increment. The
targeted rustfmt check for all changed Rust files and `git diff --check` pass.

Implementation checkpoint: `f1b8c09a7` (`dx11: emulate FL9_1 indexed triangle
strips`). Indexed TriangleStrip is now implemented; continue with the remaining
topology, resource and public-contract gates. Keep ordinary D3D11 adapter
enumeration and public HWND surface exposure disabled.

## Checkpoint — execute the final statically addressed vertex constant

The sparse-uniform suite now includes an end-to-end shader that statically reads
`constants[254]` from a 255-element vertex uniform array. It draws the same
triangle twice, first with a zero-filled uniform and then with a value only in
the final float4. On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), with FL9_1 explicitly requested and selected, readback
reported 17 green baseline pixels and 4 red shifted pixels. This verifies that
the final tested vertex constant affects GPU vertex processing. It does not
verify runtime indexing; runtime integer indexing remains deliberately rejected.
The test destroys its groups and buffers before queue submission and then
submits, waits on the native fence and reads back, exercising retained command
resource lifetime.

The focused strict test passed 1/1 with 306 tests filtered. The complete strict
private D3D11 library suite passed 305 tests, 0 failed, with 2 filtered. The
D3D11 HAL build and Clippy passed; Clippy reports ten existing warnings in
unrelated HAL code. The core zero-adapter isolation test passed 1/1 when run
with the required `std` feature. A first invocation without `std` failed to
compile the test target, then the corrected invocation passed. Targeted DX12
pipeline descriptor tests passed 2/2. The HAL package formatting check,
targeted rustfmt check and `git diff --check` passed.

Commands (Rust 1.93.1; Cargo invoked from the Visual Studio 2022 MSVC developer
environment; the hardware commands used strict mode):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_last_vertex_constant_changes_rendered_geometry -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/tests/dx11/uniform_bindings.rs
git diff --check
```

The feature is checkpointed as `6cac808c9` (`dx11: verify FL9_1 final vertex
constant execution`). Continue the uniform-layout audit and then work the next
safe render/resource gates in `FL9_1_PLAN.md`. The larger mission remains
PARTIALLY COMPLETE: this evidence is private-HAL execution on one modern GPU
forced to FL9_1, not historical-hardware or public-adapter validation.

## Follow-up — execute the pixel constant c31 boundary

The c254 geometry test now also uses a separate fragment-visible uniform array
and statically reads its final element, pixel c31. The two draws share one
pipeline and switch bind groups: vertex c254 shifts the second triangle, while
pixel c31 changes the fragment result from green to red. This verifies separate
vertex/pixel uniform bindings and their stage-specific execution through the
existing logical resource map. Readback again found 17 green baseline pixels
and 4 red shifted pixels.

Strict mode selected the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) at requested and actual FL9_1. The focused test passed
1/1 (306 filtered). The complete strict private D3D11 suite passed 305/305,
with two filtered. HAL Clippy passed with the same ten pre-existing warnings;
the changed Rust file and HAL package pass formatting, and `git diff --check`
passes. The targeted core isolation and DX12 checks from the preceding c254
checkpoint were not rerun for this private-HAL test-only extension.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_c254_and_pixel_c31_change_rendered_output -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/tests/dx11/uniform_bindings.rs
git diff --check
```

Checkpoint `0c0ff00ba` (`dx11: execute FL9_1 pixel constant boundary`). Static
near-limit vertex and pixel accesses now both have real forced-FL9_1 rendering
evidence. Runtime integer indexing remains rejected, and matrices/nested layouts
remain limited to the existing focused tests. Continue the private contract
audit and keep public adapter and HWND surface exposure disabled.

## Checkpoint — queried R8_UNORM sampled uploads

`268431ec3` adds R8_UNORM as a narrow one-mip, one-layer sampled/upload format.
The capability model separates the FL9_1 required-format guarantee, the
selected device's `TEXTURE2D | SHADER_SAMPLE` query, and the implemented
upload/sampling subset. Transfer layout now uses the format's texel width, so
the strict GPU test uploads a 5x4 image from byte offset 1 with 7-byte rows,
samples it through the private binding and shader paths, and reads a 10x8
RGBA8 target. The output matched 80/80 expected pixels. The physical adapter
was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), with requested and actual
FL9_1; its R8 support mask was `0x0002d3e0`. The driver version recorded by the
existing adapter probe is `32.0.16.2101`. The test exercised nearest sampling;
linear R8 filtering is query-gated but not visually tested. R8 render targets,
copy sources, readback, and mip chains remain unsupported.

Verification on Rust 1.93.1/MSVC:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_unorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_bind_group.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

The focused strict test passed 2/2 (one CPU capability test and one physical
GPU execution). The complete strict private suite passed 307 tests, with zero
failures and two filtered. HAL build, HAL Clippy, core's zero-adapter isolation
test (1/1), targeted DX12 tests (2/2), package formatting, targeted rustfmt,
and diff checks passed. HAL Clippy reported ten existing warnings in unrelated
code. Public D3D11 adapter enumeration remains disabled. Next, investigate and
implement the next FL9_1-required sampled format with the same query, usage,
layout and strict-readback discipline.

## Checkpoint — queried R8G8_SNORM sampled uploads

`881640796` extends the single-mip sampled/upload path to R8G8_SNORM (legacy
V8U8), a format listed as required at FL9_1 for 2D texture and shader sampling
by Microsoft's required-format table. The backend tracks its feature-level
guarantee, per-device query and implemented usage separately. Transfer layout
uses two bytes per texel, and view filterability follows the actual
`SHADER_SAMPLE` query. It remains non-renderable and cannot be copied from or
read back directly.

The strict hardware test uploaded a 5x4 signed pattern from byte offset 2 using
12-byte rows, sampled both signed channels, remapped to RGBA8 and validated all
80 pixels with a one-code allowance for UNORM rounding. The physical adapter
was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), requested and selected
FL9_1, R8G8_SNORM support mask `0x00021320`. The test used nearest filtering;
the format query also reported filtered-sampling support. The first strict run
found the zero-valued SNORM midpoint rendered as 127 instead of the model's
128; the test was corrected to allow one UNORM code, not to alter shader output.
The rerun passed. A Clippy pass flagged the initial 9-argument fixture helper;
it was refactored into a descriptor plus input/output parameters, leaving only
the ten pre-existing warnings.

Verification on Rust 1.93.1/MSVC:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_texture_tests -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

The two focused hardware tests passed 2/2. The full strict private D3D11 suite
passed 309 tests, with zero failures and two filtered. HAL build and Clippy,
core's zero-adapter test (1/1), DX12 tests (2/2), formatting and diff checks
passed. Ten pre-existing HAL Clippy warnings remain. The mission continues;
ordinary public adapter enumeration and HWND surface exposure remain gated.
Next, verify the required four-channel R8G8B8A8_SNORM sampled/upload subset,
then reassess the texture/depth and public-core contract gaps.

## Checkpoint — queried R8G8B8A8_SNORM sampled uploads

`312dba141` adds a one-mip, one-layer R8G8B8A8_SNORM sampled/upload path. The
required-format table identifies the FL9_1 2D shader-sample minimum through
legacy Q8W8V8U8. The internal model records the feature-level guarantee,
selected-device `CheckFormatSupport`, and implemented usage separately. The
transfer path accounts for four bytes per texel; render-target, copy-source,
readback and mip-chain uses remain rejected.

Strict hardware execution on the RTX 3090 Ti requested and selected FL9_1. The
device returned `R8G8B8A8_SNORM` support mask `0x000213a0`, including 2D and
shader-sample support. A padded 5x4 signed image was uploaded from byte offset
4 with 24-byte rows, sampled through the private shader/binding path and
remapped to RGBA8. All 80 pixels matched within one UNORM code value for
conversion rounding. The test also verifies the HAL rejects copy-source,
render-target and mipmapped descriptors.

Verification on Rust 1.93.1/MSVC:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 rgba8_snorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_texture_tests -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

All three focused sampled-format GPU cases passed 3/3. The complete strict
private D3D11 suite passed 311 tests, zero failed, with two filtered. HAL build
and Clippy, core zero-adapter isolation (1/1), targeted DX12 tests (2/2),
formatting and diff checks passed. HAL Clippy reports ten existing warnings;
none were introduced by this change. The current texture test helper also
asserts unsupported copy-source, render-target and mipmapped descriptors fail
at the HAL boundary. Public D3D11 adapter enumeration remains disabled.

Next investigate the required FL9_1 BC1 format and compressed buffer-to-texture
layout. Establish block row/height calculations and boundary rules with CPU
tests before attempting native upload and strict sampled-pixel verification.

## Checkpoint — BC1_UNORM sampled upload

Implementation checkpoint: `6c67e22be` (`dx11: add FL9_1 BC1 sampled texture uploads`).

BC1_UNORM is modeled as an FL9_1 required sampled format, but support remains
conditional on the selected device's `CheckFormatSupport` result. The private
HAL accepts one-mip, one-layer, 4-aligned sampled/upload textures only. Upload
layout uses 4x4 blocks of eight bytes; padded block rows, nonzero source
offsets, and block-aligned partial updates are validated with checked
arithmetic. Copy-source, render-target, readback, sRGB and mip-chain paths
remain rejected.

On the RTX 3090 Ti (vendor/device `0x10de:0x2203`, driver `32.0.16.2101`), the
strict test explicitly requested and selected FL9_1. `BC1_UNORM` support was
`0x000213a0`, including shader sampling. Four distinguishable 4x4 blocks were
uploaded into an 8x8 image from source offset 8 with 24-byte block-row pitch;
a second aligned partial upload replaced one block using offset 8 and 16-byte
pitch. Sampling and RGBA8 readback matched all 256 expected pixels.

Rust 1.93.1/MSVC verification:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 bc1 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 "dx11::" -- --test-threads=1
cargo build --offline -p wgpu-hal --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx12 pipeline_desc::tests -- --test-threads=1
cargo clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo fmt --package wgpu-hal -- --check
rustfmt --check --edition 2021 wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_transfer.rs wgpu-hal/src/dx11/texture.rs wgpu-hal/tests/dx11/r8_texture.rs
git diff --check
```

The BC1-filtered run passed 3/3 after formatting, including actual hardware
render/readback. The full strict private suite passed 315 tests with two
filtered before the final readability-only refactor of texture-format
validation. The HAL build passed; the focused core isolation test passed 1/1;
DX12 pipeline descriptor tests passed 2/2; formatting and diff checks passed.
Clippy passed with ten existing warnings, unchanged from the prior checkpoint.
Ordinary public D3D11 enumeration remains empty. The strict fixture-enabled
core library suite subsequently passed 70/70 with BC1 included, and the
focused core fixture matched all 64 pixels for `Bc1` on the forced-FL9_1 RTX.
It creates the BC1 texture and view, uploads
compressed blocks through `Global::queue_write_texture`, binds them, renders,
and maps readback. The fixture reports only `SAMPLED | COPY_DST` for BC1 and
rejects every other BC format at both HAL capability query and core texture
creation.

Checkpoint `1e59db97a` adds the test-only BC1 capability and core render case.
Checkpoint `6b86270ec` routes the main core fixture through
`Global::adapter_request_device` and the fixture-gated HAL `Adapter::open`
instead of creating a device directly from HAL. The strict core suite still
passed 70/70. The plain D3D11 HAL build and focused fixture pass; targeted
Clippy passes with ten existing HAL warnings. Ordinary DXGI/HAL adapter
enumeration remains empty. The fixture still injects manually constrained
limits and is not evidence that a truthful public adapter report exists.

Next audit the core/HAL adapter capability boundary and derive a small,
accurate, testable FL9_1 report from the existing capability model without
enabling ordinary enumeration. Identify every core-legal operation that such
a report would permit but HAL rejects; implement narrow validation or keep
that capability out of the report. Public adapter and HWND surface exposure
remain gated.

## Core capability-validation checkpoints — 2026-11-04

`96eacac49c26596a021672c5d405bb99fdbc56b1` adds
`DownlevelFlags::TWO_SIDED_STENCIL`. Core rejects differing front/back stencil
face state when the flag is absent, and the controlled FL9_1 fixture verifies
the missing-flag error before HAL. GLES, Vulkan and compliant/default flag sets
report support. Core format-feature projection for RGBA8, D24S8, BC1 and an
unsupported BC format is also checked against the fixture's manually limited
HAL report. This does not create a production adapter capability report.

`3c9a7fca156b45aad8fcfb4c99db39d36f5f8bed` honors the existing
`COMPARISON_SAMPLERS` flag during core sampler creation. The fixture verifies a
comparison sampler returns `MissingDownlevelFlags` before HAL. The strict
fixture-enabled core library suite passed 70/70; its GPU fixture ran on the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) with requested and actual
FL9_1, and all exercised image/readback checks passed. The regular core
zero-adapter isolation test passed 1/1. Core Clippy, targeted rustfmt and
`git diff --check` passed. The first shell attempt lacked `link.exe`; rerunning
inside Visual Studio 2022 Build Tools' `VsDevCmd.bat` succeeded. No HAL or
non-D3D11 code path failures were observed.

Next audit sampler/layout, blend, render-pass, and resource validation paths
for other core-legal requests that can reach an unsupported D3D11 HAL case.
Prioritize cases that can panic, issue invalid native commands, or fail only
after resource creation. Public enumeration and surface exposure remain
disabled.

`73ef1a6ada7a4db1fa8515de35745a7b7abd8720` adds
`DownlevelFlags::SEPARATE_ALPHA_BLEND` and core validation for render targets
that write both RGB and alpha with different blend equations. The controlled
FL9_1 core fixture confirms full-channel `ALPHA_BLENDING` fails with the
missing flag before HAL, while RGB-only alpha blending remains accepted by the
existing masked-channel implementation. GLES, Vulkan and default/compliant
capability sets report the flag; the FL9_1 D3D11 fixture leaves it absent.

Verification on Rust 1.93.1/MSVC: strict fixture-enabled core tests passed
70/70 (the controlled GPU fixture used the RTX 3090 Ti at requested/actual
FL9_1); core zero-adapter isolation passed 1/1; wgpu-types tests passed 55/55;
the combined DX11/DX12/Vulkan/GLES HAL check passed; core Clippy passed; DX12
pipeline descriptor regression passed 2/2; targeted rustfmt and
`git diff --check` passed. The wgpu-types test build emitted 14 existing
unused-serde-import warnings with serde disabled. Continue the core/HAL audit;
do not enable adapter enumeration based only on these targeted validations.

## Checkpoint — storage-buffer validation boundary — 2026-11-04

`58fbd32b4db44ce1e6432bfa941464899b695e7b` closes a core/HAL mismatch found
during the continuing adapter-contract audit. Core adds storage access bits to
some otherwise ordinary buffer allocations. The D3D11 HAL now accepts those
bits only alongside another supported buffer role; FL9_1 still receives its
single vertex or index native bind and no shader storage binding is implemented.
Core now requires `VERTEX_STORAGE`, `FRAGMENT_STORAGE` and `COMPUTE_SHADERS` for
storage-buffer layouts visible to those stages; fragment-writable bindings also
require `FRAGMENT_WRITABLE_STORAGE`. The strict controlled fixture verifies all
four missing-flag cases are rejected before HAL, then uploads, copies and reads
back 64/64 bytes from a `STORAGE | COPY_SRC | COPY_DST` buffer.

Verification used Rust 1.93.1/MSVC and `WGPU_DX11_REQUIRE_HARDWARE=1` on the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with requested and actual
FL9_1. The focused core fixture and full fixture-enabled core library suite
passed; the latter was 70/70, including the ordinary zero-adapter test. The
strict private D3D11 suite passed 315/315 (two filtered). The D3D11 HAL build,
combined DX11/DX12/Vulkan/GLES HAL check, core and HAL Clippy, standalone core
zero-adapter test (1/1), and DX12 pipeline descriptor test (2/2) passed.
`git diff --check` and targeted rustfmt passed. HAL Clippy reported ten
pre-existing warnings. Workspace `cargo fmt --check` remains blocked by a
pre-existing line-wrap discrepancy in `wgpu-core/src/instance.rs:1511`; that
unrelated file was not changed. The VS developer-shell banner still prints
`vError: Unknown error` even though MSVC builds and links succeed.

Next continue auditing core-legal operations that the private HAL rejects,
especially render-pass, texture/view and resource-lifetime boundaries. Public
D3D11 adapter enumeration and public surface exposure remain disabled.

## Checkpoint — texture-view dimensions rejected before HAL — `c844eab6e` — 2026-11-04

The audit found that the D3D11 HAL only creates `D2` views, while core could
forward `D2Array`, `Cube` and `CubeArray` layouts/views to it. A one-layer
`D2Array` view is legal as a descriptor shape even when the texture-array
limit is one, so the limit did not close this path. Added explicit
`TEXTURE_2D_ARRAYS` and `CUBE_TEXTURES` downlevel flags and require the
appropriate flag for sampled layout dimensions and resolved texture-view
dimensions. D3D11 leaves both flags absent. GLES, Vulkan and Metal now report
their existing array/cube support; public D3D11 enumeration remains closed.

The strict core fixture on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, requested and actual FL9_1) verified that 2D-array, cube and
cube-array layouts fail at core validation, and that a one-layer `D2Array`
view fails before HAL. The fixture-enabled core library suite passed 70/70,
including the GPU render fixture and zero-adapter isolation; the strict private
D3D11 suite passed 315/315 (two filtered). `wgpu-types` passed 55/55, the
combined DX11/DX12/Vulkan/GLES HAL check passed, core and HAL Clippy passed,
the standalone zero-adapter test passed 1/1 and the DX12 pipeline descriptor
regression passed 2/2. HAL Clippy retained ten existing warnings; workspace
format checking still reports the existing unrelated line-wrap in
`wgpu-core/src/instance.rs:1511`. Targeted rustfmt and `git diff --check`
passed. The VS developer-shell banner still prints `vError: Unknown error`
while builds succeed.

Next audit texture creation and render-pass attachment shapes, particularly
dimensions, format usages and core-valid resolve/load/store combinations.
Keep public adapter enumeration and surface exposure disabled.

## Checkpoint — texture-layout dimension validation — `8c072d4f7` — 2026-11-04

The continued layout audit found that the private HAL accepts only D2 sampled
views, while core could forward D1/D3 sampled binding layouts even when the
device's corresponding texture-dimension limit was zero. Core now rejects
those layouts with a validation error based on the requested device limits.
The audit also found that storage-texture `D2Array` layouts did not pass
through the array downlevel check applied to sampled textures; they now require
`TEXTURE_2D_ARRAYS` before format validation. The existing array/cube view
flags and texture creation checks remain unchanged.

The strict controlled core fixture passed on the RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, requested and actual FL9_1). It reported the expected
core errors for D1/D3 layouts, storage-texture D2Array, sampled array/cube
layouts and a one-layer D2Array view, then completed all existing GPU readback
cases. The full fixture-enabled core library suite passed 70/70, including
zero-adapter isolation. The core DX12 library suite passed 67/67. Core Clippy,
targeted rustfmt and `git diff --check` passed. The DX12-only feature build
emitted fifteen existing warnings because WGSL-dependent code was compiled
without the WGSL feature; no new failures were reported. The Visual Studio
developer-shell banner continues to print `vError: Unknown error` while Rust
builds and tests succeed.

Continue the texture descriptor/render-pass audit, then return to the broader
core-valid command inventory. Public D3D11 adapter enumeration and surface
exposure remain disabled.

## Checkpoint — FL9_1 single-sample output masks — `8eff9020a` — 2026-11-04

The core/HAL audit found that core permits a single-sample pipeline with a
zero sample mask, while the private HAL previously rejected every non-default
multisample descriptor and hard-coded the native mask to all ones. The pipeline
now retains the WebGPU mask and passes it to the output merger. For FL9_1 only,
a native zero mask is translated to `0xffff_fffe`: sample zero remains masked,
all other bits are irrelevant for the supported single-sample target, and the
native mask is nonzero as required by the downlevel context. Multisampling and
alpha-to-coverage remain rejected. A core-fixture render case verified that
all 64 pixels retain the blue clear color with `mask = 0`.

Rust 1.93.1/MSVC verification on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) requested and selected FL9_1. The focused strict HAL translation
test passed 1/1; the focused strict core fixture passed 1/1 and printed that all
64 pixels matched; the complete strict private D3D11 suite passed 318/318; the
complete fixture-enabled core library suite passed 70/70, including the
zero-adapter isolation test; and the core DX12 library suite passed 67/67.
The non-test D3D11 HAL `cargo check` passed. HAL and core Clippy passed, with
ten existing HAL warnings and no core warnings. Targeted rustfmt and
`git diff --check` passed. The Visual Studio developer-shell banner continues
to print `vError: Unknown error` despite successful compiler initialization.

Code checkpoint: `8eff9020a` (`dx11: honor single-sample masks at FL9_1`).
Public adapter enumeration and public surface exposure remain disabled. The
controlled core fixture still injects restricted capabilities and is not a
production-adapter report. Continue auditing texture creation and render-pass
validation; keep historical FL9_1 hardware and the public integration path
listed as unverified.

Commands for the focused checks were:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
```

The focused unit and fixture selectors, both Clippy commands, and targeted
format commands are recorded in the session checkpoint; the full commands
above include the corresponding suites and ordinary library check.

## Checkpoint — core rejects unavailable NPOT mip chains — `1eb21620c` — 2026-11-04

The texture descriptor audit found that core forwarded non-power-of-two mip
chains to HAL without checking the existing
`NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` downlevel flag. The D3D11 HAL only
implements power-of-two mip chains and rejects NPOT mip resources regardless
of the modern driver's optional full-NPOT query. Core now checks the flag for
multi-mip textures using the spatial dimensions for D1, D2 or D3; D2 array
layer counts are correctly excluded. The controlled FL9_1 fixture requests a
6×8 RGBA8 two-mip texture with the flag absent and observes the specific core
`MissingDownlevelFlags` error before HAL.

The CPU dimension test passed 1/1. Under strict hardware mode, the core fixture
ran on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested
and actual FL9_1, printed `PASS: NPOT mip chain rejected by core validation`,
and completed all render/readback cases; the complete fixture-enabled core
suite passed 71/71. The core DX12 suite passed 68/68, with the existing
no-WGSL-feature warnings; the ordinary zero-D3D11-adapter test passed 1/1.
Core Clippy, targeted rustfmt and `git diff --check` passed. Other backends
retain their existing flag reports: the default-compliant DX12/Metal flags and
the explicit Vulkan/GLES flags permit this path.

Code checkpoint: `1eb21620c` (`core: gate unsupported NPOT mip chains`). The
HAL was not changed; its last strict private suite passed 318 tests at
`8eff9020a`. Public D3D11 enumeration and surface exposure remain disabled.
Continue the core/HAL contract audit, especially blend-state legality and
resource/view combinations that cannot be represented by the current flags.

Commands:

```text
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std npot_mipmap_tests -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

## Checkpoint — general single-target blend translation — `f99cae113` — 2026-11-04

The custom blend audit found that the HAL accepted only a fixed set of named
blend presets even though core permits ordinary custom single-target blend
equations. The HAL now translates the ordinary non-dual-source blend factors
and all five operations, maps `Src`/`Dst` differently for RGB and alpha
components, and records/rebinds blend constants in command order. It rejects
`SrcAlphaSaturated`, all dual-source factors and differing full-channel RGB/
alpha equations unless separate-alpha support is established. That capability
remains unavailable in the production FL9_1 model. The native descriptor
mirrors target zero into the first four native slots so their `BlendEnable`
bits satisfy the Microsoft FL9_1 rule while only one target is bound.

An initial full-suite run exposed three stale assertions that expected the old
translator to reject a custom diagnostic equation or expected inactive blend
fields to retain preset values. After updating those tests to assert the new
contract, the full strict HAL suite passed 321/321. Its GPU blend test executed
constant and inverse-constant factors with changing blend constants, then Add,
Subtract, ReverseSubtract, Min and Max. All 64 pixels matched. The RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) was selected as hardware and the device
was explicitly requested and confirmed at FL9_1.

The controlled core fixture passed 71/71, including the custom constant-blend
render and 64-pixel readback through core command recording; the fixture still
injects restricted test capabilities. The core DX12 suite passed 68/68 with
fifteen existing warnings from the no-WGSL feature configuration. The ordinary
zero-D3D11-adapter isolation test passed 1/1. `cargo check` for the D3D11 HAL
passed. HAL Clippy passed with ten existing warnings and no new warnings; core
Clippy passed cleanly. Targeted rustfmt and `git diff --check` passed. The
Visual Studio developer-shell banner continues to print `vError: Unknown
error` even though the MSVC environment initializes and builds succeed.

The change does not establish historical FL9_1 behavior and does not enable a
public adapter. All factors are CPU-translation tested, but GPU readback uses
constant factors and `One` factors; other factor visual results remain
unverified. Full-channel separate-alpha equations remain gated. See Microsoft's
[10Level9 blend-state restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
and [`OMSetBlendState` blend-factor behavior](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-omsetblendstate).

Commands:

```text
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/tests/dx11/blend_state.rs wgpu-hal/tests/dx11/textured_render.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

Code checkpoint: `f99cae113` (`dx11: generalize FL9_1 blend-state translation`).
Next continue the core/HAL render-state audit; depth comparison functions are a
small candidate for verification and expansion. Keep adapter enumeration and
public surface exposure disabled, and preserve the separate-alpha restriction.

## Checkpoint — native FL9_1 raster depth comparisons — `340147803` — 2026-11-04

The pipeline previously rejected Less, Equal, Greater, NotEqual and
GreaterEqual depth comparisons despite the D3D11 depth-stencil descriptor
using the complete `D3D11_COMPARISON_FUNC` enum. The similarly worded FL9_1
LessEqual restriction in Microsoft's 10Level9 material concerns comparison
sampler filtering, not raster depth state. The HAL now maps all seven native
functions. Never retains the prior validated-draw suppression and active
stencil depth-fail remapping.

The new strict hardware test renders eight scissored regions with an expected
pass and fail depth for each comparison. It verifies all 128 RGBA pixels: 64
green pixels where depth passes, 64 blue clear pixels where it fails. Never's
two regions remain blue through the existing suppression path. Device creation
explicitly requested and asserted FL9_1 on the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, driver previously recorded as `32.0.16.2101`).
Historical FL9_1 implementations remain untested.

The pre-change strict `fl91_depth_test_preserves_near_geometry_and_loads_depth`
test passed 1/1. The first full post-change suite found one stale assertion that
expected Less pipeline creation to fail (321 passed, 1 failed); the assertion
was replaced with creation checks for the newly supported comparisons. The
rerun passed the complete strict D3D11 suite 322/322. The full controlled
fixture-enabled core suite passed 71/71, including its hardware fixture. Its
focused hardware fixture passed 1/1 and logged its texture, buffer, depth and
topology readbacks. Ordinary D3D11 adapter isolation passed 1/1.

`cargo check` and HAL Clippy passed. Clippy reported ten pre-existing warnings
in fence, HAL device tests, swapchain test ordering and the texel-load fixture;
none were in the changed files. Rustfmt check and `git diff --check` passed.
The Visual Studio developer-shell banner still prints `vError: Unknown error`
while initializing MSVC successfully. The core DX12 suite was not rerun because
this checkpoint changes only the private D3D11 pipeline/test path and docs.

Commands:

```text
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test_preserves_near_geometry_and_loads_depth -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_comparison_functions_match_expected_fragments -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
rustfmt --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/tests/dx11/depth_comparison.rs wgpu-hal/tests/dx11/depth_render.rs
git diff --check
```

## Checkpoint — FL9_1 slope-scaled triangle depth bias — `6f7f1297891ac3477ed7055ded00934da1c46c2a`

The native rasterizer path now carries both the signed constant bias and finite
`slope_scale` from `DepthBiasState` into `D3D11_RASTERIZER_DESC`. FL9_1's
required zero `DepthBiasClamp` remains enforced. Nonzero bias on points and
solid lines is rejected because Microsoft documents that bias has no effect on
those primitives (except wireframe lines, which this backend does not create).
Selected fragment entries writing `frag_depth` still reject either bias term.

The strict forced-FL9_1 RTX 3090 Ti test passed five complete constant-bias
images and two slope-scaled images. In the sloped tests, the first red triangle
writes a varying depth; positive slope bias prevents the second green triangle
from replacing it, and negative slope bias lets green replace it. All 64 pixels
matched in each case. Synthetic pipeline tests cover triangle acceptance,
point/line rejection, non-finite slope, clamp rejection and topology-specific
translation. The existing controlled core fixture still validates the positive
and negative constant-bias cases, but the new slope cases exercise the private
HAL path only.

The complete strict private D3D11 suite passed 322/322; six unsafe runtime
integer-index GPU subcases reported their expected rejection and skip. The
focused hardware test passed 1/1, the D3D11 HAL check passed, the ordinary core
zero-adapter test passed 1/1, and the strict controlled core fixture passed
1/1, including its depth-bias, topology and texture readbacks. HAL Clippy passed
with ten existing warnings outside the changed files. Targeted rustfmt and
`git diff --check` passed. The core DX12 suite was not rerun because only the
private D3D11 path and its test changed. MSVC initialization continued to print
the existing `vError: Unknown error` banner while compiling successfully.

The first new GPU assertion mistakenly expected the target clear color after a
depth-failing second draw; observed red correctly showed that the first draw's
color remained. The expectation was corrected and the strict test passed.
FXC's diagnostic for the existing direct-only `frag_depth` shader remains an
expected `X4530` because the profile requires all four `SV_Target0` components.
Microsoft documents that clamp is unsupported at FL9_x and describes the
slope-bias equation and primitive behavior in [D3D11 depth bias](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d10-graphics-programming-guide-output-merger-stage-depth-bias)
and [10Level9 rasterizer restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).
Modern hardware forced to FL9_1 does not establish historical bias precision.

Code checkpoint: `6f7f1297891ac3477ed7055ded00934da1c46c2a`
(`dx11: support FL9_1 slope depth bias`). Public adapter enumeration remains
disabled. Documentation checkpoint: `5bf9140034703754110639375168096218a2155d`
(`docs: record FL9_1 slope depth bias checkpoint`).

Commands:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_triangle_depth_bias_and_pipeline_transition -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11
rustfmt --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/tests/dx11/depth_bias.rs
git diff --check
```

Documentation checkpoint remains to be committed after this journal update.

## Checkpoint — core rejects unavailable FL9_1 depth-bias clamp — `a281a4c3f2c9e5efa91be992c5cbd401b9cec33c`

The controlled FL9_1 core fixture now sets a nonzero `DepthBiasState::clamp`
while its downlevel capabilities omit `DEPTH_BIAS_CLAMP`. It verifies that
core returns `CreateRenderPipelineError::MissingDownlevelFlags` for exactly
that flag before reaching the private HAL, then restores the descriptor and
continues the existing GPU tests. This confirms the HAL's native FL9_1 clamp
restriction has a corresponding safe core rejection path; normal adapter
enumeration remains disabled.

The full fixture-enabled core library suite passed 71/71 with strict hardware
required. A rerun of the focused hardware fixture passed 1/1 on the NVIDIA RTX
3090 Ti at explicitly requested and actual FL9_1 and printed the new rejection
result; all existing depth, texture, transfer, topology and readback checks
still passed. Core Clippy and targeted rustfmt passed. The fixture suite and
this focused case use the test-only adapter, not production adapter reports.

Code checkpoint: `a281a4c3f2c9e5efa91be992c5cbd401b9cec33c`
(`dx11: verify FL9_1 depth clamp core gate`). Documentation checkpoint:
`20f69046714e7f9ffb061df3825d0eaf52bdfabf` (`docs: record FL9_1 depth clamp
core validation`). Resume with the public HAL/core capability-boundary audit
in FL9_1_PLAN.md.

## Checkpoint — raster depth-bias capability facts — `1a362ec6108de9accb52e2f1978a174d1f923106`

`HardwareGuarantees` now records native constant and slope-scaled depth bias
for the recognized levels and records `DepthBiasClamp` only from FL10_0.
`BackendCapabilities` independently records the implemented D24S8 triangle
constant/slope paths and the absent clamp and biased-`frag_depth` paths. Tests
assert the feature-level distinction across all seven identities and ensure
the FL9_1 backend implementation is not promoted when a higher level is
modeled. The strict hardware diagnostic prints those facts from the selected
device level, separate from driver queries.

The targeted capability-model test passed 1/1. The complete strict private
D3D11 suite passed 322/322 on the RTX 3090 Ti explicitly requested and selected
at FL9_1. Its indexed-render diagnostic showed `depth_bias=true`,
`slope_scaled_depth_bias=true`, `depth_bias_clamp=false`, with the backend
reporting D24S8 triangle constant/slope support and clamp/biased-fragment-depth
support false. The D3D11 HAL check passed; HAL Clippy passed with ten existing
warnings outside the changed capability file; targeted rustfmt and
`git diff --check` passed. The six expected dynamic-index GPU skips described
in the earlier depth-bias checkpoint remain. Public enumeration remains empty.

Code checkpoint: `1a362ec6108de9accb52e2f1978a174d1f923106`
(`dx11: model raster depth-bias capabilities`). Documentation checkpoint:
`ae7a75b16a10dc5e98d3d4e7b38a9eace300d1a1`
(`docs: record FL9_1 raster bias capability model`). Continue with the
public HAL/core capability-boundary audit in FL9_1_PLAN.md.

## Checkpoint — core gates multi-mip depth attachments — `983eea619`

Microsoft's 10Level9 device-method table states that a resource created with
`D3D11_BIND_DEPTH_STENCIL` must have one mip at FL9_1, FL9_2 and FL9_3. Core
format features do not describe this mip-count/usage combination, while the
private D3D11 HAL creates D24S8 only as a single-mip depth attachment. Added
`DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP` and require it when a depth/stencil
texture with more than one mip is also a render attachment. The private D3D11
capability model records no implementation support at any modeled feature
level; hardware guarantees are separately modeled as absent through FL9_3 and
present from FL10_0. GLES and Vulkan explicitly report support; DX12 and Metal
retain the capability from their default downlevel reports.

The controlled core fixture omits the flag and verifies a two-mip D24S8
attachment returns `MissingDownlevelFlags(DEPTH_TEXTURE_MULTI_MIP)` before HAL.
This is core validation evidence, not an attempted native multi-mip DSV call.
Strict fixture execution completed on NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with FL9_1 requested and selected.
The existing fixture then completed its texture/depth/topology rendering and
readbacks. Normal D3D11 enumeration remains empty.

Validation used Rust 1.93.1, MSVC and `CARGO_BUILD_JOBS=8`. The fixture-enabled
core suite passed 71/71; strict private HAL suite passed 322/322; ordinary
zero-adapter isolation passed 1/1; core DX12 suite passed 68/68; the focused
wgpu-types capability test passed 1/1; combined D3D11+GLES HAL check and core
Clippy passed. HAL Clippy completed with ten existing warnings in unrelated
D3D11 fence/device/test-support code. A plain PowerShell test invocation first
failed because MSVC `link.exe` was not initialized; rerunning under
`VsDevCmd.bat` succeeded. A test build during implementation also caught a
missing initializer for the newly modeled backend field; it was fixed before
the successful runs.

Code checkpoints: `983eea619` (`core: gate multi-mip depth attachments by
capability`), followed by `834cd9856` (`hal: report multi-mip depth support on
Vulkan`). Cross-backend checks used:

```text
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-hal --no-default-features --features vulkan
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features vulkan
```

Documentation checkpoint is recorded in the following commit.
Continue the core/HAL audit before constructing the public capability
snapshot; do not enable enumeration or surface exposure.

## Checkpoint — selected-entry vertex texture limits in core — `68cccdf11`

Added `DownlevelLimits::max_sampled_textures_per_vertex_stage` so backends can report vertex sampled-texture capacity separately from the general per-stage limit. Core counts sampled and depth textures statically used by the selected vertex entry point, using Naga's validated per-entry resource usage (including helper calls and `textureLoad`), and rejects excess use before HAL pipeline creation. The D3D11 capability fixture reports zero at FL9_1; the test confirms a vertex texture pipeline fails with the specific core validation error before the private HAL. D3D11 hardware guarantees record vertex texture sampling as absent through FL9_3 and present from FL10_0, while backend implementation remains false. GLES queries its actual vertex texture unit count; Vulkan and noop report their current capacities. Ordinary D3D11 enumeration remains disabled.

Verification used Rust 1.93.1/MSVC with `CARGO_BUILD_JOBS=8`:

- Core selected-entry usage unit test: 1/1 passed.
- wgpu-types downlevel-compliance test: 1/1 passed.
- Strict forced-FL9_1 core fixture: 1/1 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`); the fixture confirmed `TooManySampledTexturesInVertexStage { given: 1, limit: 0 }` before HAL and then completed GPU rendering/readback.
- Full strict fixture-enabled core suite: 72/72 passed, including zero-D3D11-adapter isolation.
- Strict private D3D11 HAL suite: 322/322 passed with hardware required; expected negative shader tests still print `SKIP` for unsupported operations after confirming rejection.
- Core DX12 regression: 69/69 passed.
- HAL D3D11+GLES and Vulkan checks, wgpu-info check, and core Clippy passed. HAL Clippy passed with ten pre-existing warnings in D3D11 fence/device/test support. Targeted formatting and `git diff --check` passed.

The first DX12 invocation omitted `std,wgsl` and failed during test compilation; the corrected command passed. An initial attempt to construct a vertex-visible layout in the fixture reached a HAL device-lost path, so the test now reuses the supported fragment-visible layout: the core vertex-resource limit is checked before that later layout compatibility path. The actual unsupported vertex texture execution remains unattempted, by design.

Code checkpoint: `68cccdf111f36553e36f8328ec80ec3195786c80` (`core: gate vertex textures by downlevel limits`). Next continue the core/HAL capability-boundary audit; do not enable public adapter enumeration or surface exposure.

## Checkpoint — conditional FL9_x NPOT sampling in core — `bb22b4636`

Core now retains Naga's validated sampling pairs for the selected render-pipeline entries and checks those pairs against the current bind groups during draw readiness. On devices without unrestricted NPOT sampling, only a used NPOT texture with a `Repeat` sampler is rejected; an unused texture/sampler cross-pair does not cause a false rejection. D3D11 derives the capability from the selected feature level and a positive `D3D9_OPTIONS` query, treating negative and unavailable 9_x queries conservatively. The private HAL applies the same rule. Public D3D11 adapter enumeration remains disabled.

Verification used Rust/Cargo 1.93.1 and MSVC. The strict GPU suites ran with
`CARGO_BUILD_JOBS=16`; the final focused checks and Clippy runs used
`CARGO_BUILD_JOBS=32`:

- Strict core fixture suite: 73/73 passed; physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), software=false, requested/actual FL9_1. Its driver query reports full NPOT=true; the fixture synthetically sets the core capability false. The used NPOT/repeat pair was rejected at command encoding before submission. POT/repeat plus NPOT/clamp rendered 64/64 expected yellow pixels while an unused NPOT/repeat cross-pair remained bound.
- Strict private D3D11 HAL suite: 323/323 passed on forced FL9_1 hardware. The six legacy shader-operation cases that explicitly report `SKIP` remain unsupported negative probes, not GPU passes.
- Core zero-adapter isolation: 1/1 passed. Core DX12 regression: 70/70 passed.
- Core, HAL, wgpu-types and wgpu-info Clippy passed. HAL emitted ten existing warnings outside this change; wgpu-types emitted 14 existing serde-import warnings under default features. Rustfmt check on all changed Rust files and `git diff --check` passed. Workspace-wide `cargo fmt --all -- --check` still reports formatting differences in unchanged `naga/src/back/hlsl/writer.rs` and `wgpu-core/src/instance.rs`.

One attempted zero-adapter invocation omitted the `wgsl` feature and failed while compiling its WGSL-based unit test; the corrected invocation below passed. No code failure remained.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The core feature and test checkpoint is `bb22b4636` (`core: validate FL9_1 NPOT sampling pairs`). Documentation and this journal are recorded separately.

## Checkpoint — source-alpha-saturated blend validation — `e001cfcd0`

The core/HAL audit found that D3D11 10Level9 rejects `SRC_ALPHA_SAT` at FL9_1,
9_2 and 9_3, but core only gated separate-alpha and dual-source blending. A
blendable render-target format cannot represent this individual-factor
restriction. `DownlevelLimits::src_alpha_saturated_blend` now records the
capability, participates in minimum-capability checks and WebGPU-compliance
reporting, and is displayed by `wgpu-info`. Core rejects the factor in color or
alpha source/destination slots before HAL. The D3D11 model records 9_x as
unsupported and 10_0+ as guaranteed by hardware while keeping backend support
false at every level; no higher-level execution path was enabled. GLES and noop
report the baseline capability.

Verification used Rust 1.93.1, MSVC, `CARGO_BUILD_JOBS=32` and one Cargo process
at a time:

- `cargo test --offline -p wgpu-types --lib -- --test-threads=1`: 58/58 passed.
- Strict fixture-enabled core suite: 74/74 passed, including all four factor
  placements rejected by core and the existing GPU rendering/readback cases.
- Strict private D3D11 HAL suite: 323/323 passed with `WGPU_DX11_REQUIRE_HARDWARE=1`.
- Core zero-D3D11-adapter isolation: 1/1 passed; core DX12 suite: 71/71 passed.
- D3D11+GLES HAL, wgpu-info and wgpu-test checks passed. Core, wgpu-test and
  wgpu-info Clippy passed. HAL Clippy passed with ten existing warnings;
  wgpu-types Clippy passed with fourteen existing unused-serde-import warnings.
- Targeted rustfmt and `git diff --check` passed.

The commands ran in an initialized Visual Studio 2022 MSVC shell with
`CARGO_BUILD_JOBS=32`:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-types --lib -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-info
cargo check --offline -p wgpu-test
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo clippy --offline -p wgpu-types --tests
cargo clippy --offline -p wgpu-test --tests
cargo clippy --offline -p wgpu-info
rustfmt --edition 2021 --check wgpu-types/src/limits.rs wgpu-core/src/pipeline.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs tests/src/params.rs wgpu-info/src/human.rs
git diff --check
```

The strict tests selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
software=false), requested and selected FL9_1. The fixture does not print the
driver version in this run; prior baseline notes record `32.0.16.2101`. The
four unsupported blend descriptors themselves are validation-only and were
rejected before HAL; the rest of the fixture and private HAL suite performed
real GPU work. The first fixture attempt exposed that its test descriptor also
triggered the existing separate-alpha gate; RGB-only and alpha-only write masks
isolated the new check, after which the strict suite passed.

Next continue auditing core-valid vertex formats and attachment/viewport state.
The independent read-only audit found that core accepts baseline vertex
formats which `hal_pipeline.rs::vertex_format` does not translate; this needs a
precise format gate or implementation before public capability reporting.
Public D3D11 enumeration and surface exposure remain disabled.

## Checkpoint — FL9_1 vertex format reporting and input validation — `cd82e503d3954a86467c2d42f1ca39a6cbb3c66d`

Audited Microsoft's required DXGI format table and the existing mismatch
between core's accepted baseline vertex attributes and the private D3D11
input-layout mappings. Added `DownlevelLimits::vertex_format_mask`, with
baseline defaults preserved for existing adapters and the 64-bit formats still
controlled by their feature. Core now rejects attributes missing from that
per-format mask before HAL; the D3D11 capability fixture fills the mask from
the selected FL9_1 level, per-device `CheckFormatSupport(IA_VERTEX_BUFFER)`
results and the implemented HAL map. Ordinary D3D11 enumeration remains empty.

The private HAL now maps FL9_1-required `R8G8B8A8_UINT`, `R16G16_SINT` and
`R16G16B16A16_SINT` input formats in addition to the already supported float,
unorm and signed-normalized formats. The existing shader guard remains closed
to runtime integer math; it allows only direct integer vector vertex inputs
converted to float in the FL9_1 vertex shader. The Naga-to-FXC pipeline and
GPU test exercise the actual native layouts. Synthetic capability tests check
feature-level minima, driver/backend intersections and negative format-query
results. Microsoft's required-format table also identifies `R16G16_FLOAT` and
`R16G16B16A16_FLOAT` IA support beginning at FL9_3; this is recorded as a
hardware minimum only and does not enable those formats in the private HAL.

Verification used the pinned Rust 1.93.1 toolchain in the Visual Studio 2022
MSVC shell, with one Cargo process at a time and `CARGO_BUILD_JOBS=32`:

- Baseline strict `fl91_snorm16_vertex_formats_render`: 1/1 passed before the
  edit on the RTX 3090 Ti at requested/actual FL9_1.
- D3D11 HAL build: `cargo check --offline -p wgpu-hal --no-default-features --features dx11` passed.
- wgpu-types unit suite: 59/59 passed.
- Capability identity, mask projection, native mapping/query and integer
  shader-guard tests: each focused CPU test passed 1/1.
- Strict new `fl91_integer_vertex_formats_render`: 1/1 physical GPU test
  passed. Adapter NVIDIA GeForce RTX 3090 Ti, vendor/device `0x10de:0x2203`,
  software=false, driver `32.0.16.2101`; requested and actual feature level
  FL9_1. The driver reported `IA_VERTEX_BUFFER` flags `0x2` for each new
  format. The projected mask was `0x0000000078c30104`; all 64 RGBA8 pixels
  matched the expected yellow output.
- Full strict private D3D11 HAL suite: 326/326 Rust tests passed. This includes
  real GPU rendering/readback cases; a small set of negative shader probes
  intentionally reports unsupported execution as `SKIP` after checking safe
  rejection and is not counted as GPU execution.
- Full strict fixture-enabled core suite: 74/74 passed, including physical
  rendering/readback; the new `Unorm16x2` pipeline case was rejected by core
  before HAL. Core zero-D3D11-adapter isolation: 1/1 passed.
- Core DX12 regression suite: 71/71 passed. HAL D3D11+GLES check, wgpu-test
  check, wgpu-info Clippy, core Clippy and `rustfmt --check` on changed Rust
  files passed.
- HAL Clippy passed with ten existing warnings in unchanged D3D11 support and
  test code. wgpu-types Clippy and tests emitted fourteen existing unused
  serde-import warnings under the no-serde default feature configuration.
  `git diff --check` and the staged diff check passed.

An initial chained shell invocation of the non-D3D11 checks returned exit code
1 without diagnostics; each check was rerun separately and passed. No test or
build failure remains. Public D3D11 adapter and surface exposure remain gated.

Next audit the remaining core-valid render-pass attachment, vertex-layout and
pipeline descriptors against the actual HAL, then prioritize the public-core
integration blockers. Keep the historical-driver and Present reentrancy gaps
explicit; this modern-GPU result does not validate old FL9_1 hardware.

## Checkpoint — per-format single-mip validation — `53484db82`

Added an opt-in `SINGLE_MIP_ONLY` bit to HAL and wgpu-types format capability
flags. Core projects the bit and rejects a texture descriptor with more than
one mip before invoking HAL. The D3D11 fixture reports the bit for queried
single-mip sampled/upload formats (R8_UNORM, R8G8_SNORM, R8G8B8A8_SNORM and
BC1_UNORM) and D24S8. This closes a validation gap that device-wide NPOT mip
and depth-attachment flags did not express for POT sampled textures. Other HALs
leave the opt-in bit unset. The core fixture verifies one-mip creation and
pre-HAL rejection of POT 8×8 two-mip descriptors for every supported sampled
upload format; the existing BC1 render case still performs GPU upload,
sampling and readback.

Verification used Rust 1.93.1 and the Visual Studio 2022 MSVC environment with
`CARGO_BUILD_JOBS=32`, one Cargo process at a time:

- Baseline before editing: strict core FL9_1 fixture 1/1 and focused BC1 HAL
  tests 3/3. The physical adapter was NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1; BC1 sampling
  matched 256/256 pixels.
- wgpu-types suite: 59/59 passed.
- Strict controlled core fixture: 1/1 passed after the change. It reported
  `SINGLE_MIP_ONLY` for D24S8 and BC1, verified all queried single-mip sampled
  formats, accepted their one-mip descriptors, rejected their POT two-mip
  descriptors in core, and completed existing render/readback cases including
  BC1 at 64/64 pixels. Hardware remained the RTX 3090 Ti at requested/actual
  FL9_1. This run did not print the driver version.
- Focused BC1 HAL tests: 3/3 passed, including 256/256 GPU output pixels.
- Full strict private D3D11 HAL suite: 326/326 passed. Full strict
  fixture-enabled core suite: 74/74 passed. Zero-adapter isolation: 1/1 passed.
  Core DX12 regression suite: 71/71 passed.
- `cargo check` passed for HAL with `dx11,gles` and core with `dx11,std,wgsl`.
  Core, HAL and wgpu-types Clippy passed. HAL retained ten existing warnings
  in untouched code; wgpu-types retained fourteen existing serde-import
  warnings under the default no-serde test configuration.
- Targeted `rustfmt` checks on changed Rust files and `git diff --check`
  passed. The formatter's unrelated change to the existing zero-adapter test
  was restored before staging. The strict core fixture was rerun after the
  final assertion change.

Commands run (inside the initialized MSVC shell; `WGPU_DX11_REQUIRE_HARDWARE=1`
for both strict hardware suites):

```text
cargo test --offline -p wgpu-types --lib -- --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-core --no-default-features --features dx11,std,wgsl
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo clippy --offline -p wgpu-types --tests
rustfmt --edition 2021 --check wgpu-hal/src/lib.rs wgpu-types/src/texture/format.rs wgpu-core/src/resource.rs wgpu-core/src/device/resource.rs wgpu-hal/src/dx11/adapter.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

`wgpu-core/src/instance.rs` was included in the successful formatter run, then
its unrelated pre-existing formatting change was reverted. Its new mapping
block was inspected and `git diff --check` passed.

Code commit: `53484db82` (`dx11: validate single-mip texture format limits`).
Public D3D11 adapter enumeration and public HWND surface exposure remain
disabled. Next continue the core/HAL texture descriptor and render-pass audit;
do not treat this focused validation repair as completion of public adapter
readiness.

## Checkpoint — reusable per-format capability projection

Moved the fixture's duplicated format-capability mapping into
`Capabilities::texture_format_capabilities`. It derives D24S8, color-target,
copy, sampled and sampled-upload flags from the selected device's successful
format query and the backend's implemented paths. The fixture now delegates to
this production internal method. Missing and negative queries produce no
capabilities, and the method remains unused by ordinary D3D11 enumeration.
RGBA8/BGRA8 transfers are gated by their backend implementation switches.
Code commit: `7b3b5be08` (`dx11: centralize FL9_1 format capability projection`).

Baseline: the previous fixture-only blend-query test passed 1/1. An initial
attempt outside the MSVC developer shell failed at link because `link.exe` was
not on PATH; rerunning with `VsDevCmd.bat` resolved the environment issue.
Verification on Rust 1.93.1/MSVC (`CARGO_BUILD_JOBS=32`, one Cargo process):

- D3D11 `format_projection` tests passed 4/4, synthetic/CPU-only.
- `cargo check -p wgpu-hal --no-default-features --features dx11,gles` passed.
- Strict fixture-enabled core suite passed 75/75 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. It reported
  RGBA8, D24S8 and BC1 format projections and completed existing GPU
  render/readback cases. Its ordinary D3D11 zero-adapter isolation test passed.
- Strict private D3D11 HAL suite passed 329/329 on the RTX 3090 Ti at forced
  FL9_1; no tests were skipped.
- Core DX12 regression suite passed 72/72.
- HAL Clippy passed with ten existing warnings in untouched code.
- Targeted rustfmt and `git diff --check` passed.

The capability projection did not enable public enumeration and is not a full
adapter report. The public adapter and surface gates remain closed. The modern
RTX forced to FL9_1 does not substitute for historical hardware testing.

## Checkpoint — conditional RGBA8/BGRA8 mip projection — pending code commit

The reusable per-format projection now includes `SINGLE_MIP_ONLY` for RGBA8
and BGRA8 when the selected device lacks `D3D11_FORMAT_SUPPORT_MIP`, or when
any backend path needed for the supported POT mip combinations is unavailable.
The controlled core fixture compares both flags against the actual query and
includes both formats in the pre-HAL multi-mip rejection loop. Ordinary D3D11
enumeration remains empty.

Verification used Rust 1.93.1, the Visual Studio developer shell, and
`CARGO_BUILD_JOBS=32`, with one Cargo process at a time:

- Synthetic `color_format_projection_limits_mips_to_positive_driver_and_backend_support`: 1/1 passed for both RGBA8 and BGRA8, testing absent/present MIP bits and a disabled backend mip path.
- Strict fixture-enabled core suite: 75/75 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. RGBA8 and BGRA8 both reported driver MIP support and `single_mip_only=false`; existing mip-chain transfer/render/readback cases passed. The fixture also verified ordinary D3D11 enumeration remains empty.
- After adding the explicit query diagnostic, the focused strict hardware fixture passed 1/1 and printed the same query/projection values.
- Full strict private HAL suite: 330/330 passed, including GPU execution cases; core DX12 regression: 72/72 passed.
- HAL and core fixture Clippy passed; HAL retained ten pre-existing warnings in untouched code. Targeted rustfmt and `git diff --check` passed.

Exact commands:

```text
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 color_format_projection -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs
git diff --check
```

The negative no-MIP case is synthetic; the RTX cannot validate a driver that
does not expose that format bit. This modern forced-FL9_1 result does not
substitute for historical hardware. Next address the audited core/HAL mismatch
for nonzero depth bias with a selected fragment entry point that writes
`frag_depth`, using entry-point-specific validation and keeping adapter
enumeration closed.

## Checkpoint — selected-entry depth-bias validation — `8f81fdc8f`

Core now represents whether depth bias can be combined with a selected fragment
entry point writing `frag_depth` as
`DownlevelLimits::depth_bias_with_fragment_depth`. The controlled FL9_1 fixture
sets it false. A new `Interface` query reuses Naga-derived selected-entry
outputs; core rejects the unsupported combination before HAL and treats an
opaque shader interface conservatively. The private D3D11 capability model
already records this backend path as unavailable. Ordinary D3D11 adapter
enumeration remains empty.

The strict fixture created a WGSL module with both a color-only and a
color-plus-depth fragment entry. It verified: nonzero bias plus selected depth
output returns the specific core validation error; selecting the color entry
from that same module creates a biased pipeline; selecting the depth entry with
zero bias creates a pipeline. These compile through the actual D3D11 HAL on
physical hardware, but do not execute a depth-writing draw.

Verification used Rust 1.93.1 in the Visual Studio developer shell with
`CARGO_BUILD_JOBS=32`, one Cargo process at a time:

- `wgpu-types` full library tests: 61/61 passed; 14 existing unused-serde-import warnings.
- Strict fixture-enabled core suite: 75/75 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. Existing rendering/readback assertions passed; the new pipeline selection checks passed. The fixture also asserts ordinary D3D11 enumeration remains empty.
- Focused strict fixture rerun after correcting the case to use triangle topology: 1/1 passed. The initial test attempt ran inside the helper's point-list case and correctly failed the pre-existing depth-bias/topology validation; the test was scoped to the baseline triangle case and then passed.
- Core's zero-D3D11-adapter isolation test: 1/1 passed.
- Core DX12 regression: 72/72 passed.
- Strict private D3D11 HAL suite: 330/330 passed on the RTX 3090 Ti at requested/actual FL9_1, including actual GPU readback cases. A small set of shader tests intentionally logs `SKIP` for rendering after verifying expected safe shader rejection; these do not count as executed GPU draws.
- HAL `dx11,gles` check, `wgpu-info` check and `wgpu-test --tests` check passed.
- Core fixture Clippy, wgpu-types Clippy, wgpu-info Clippy and HAL `dx11,gles` Clippy passed. HAL retained ten existing warnings in untouched code; wgpu-types retained fourteen existing unused-serde-import warnings.
- Targeted rustfmt and `git diff --check` passed.

Exact commands:

```text
cargo test --offline -p wgpu-types --lib -- --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo check --offline -p wgpu-hal --no-default-features --features dx11,gles
cargo check --offline -p wgpu-info
cargo check --offline -p wgpu-test --tests
cargo clippy --offline -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo clippy --offline -p wgpu-types --tests
cargo clippy --offline -p wgpu-info
cargo clippy --offline -p wgpu-hal --tests --no-default-features --features dx11,gles
rustfmt --edition 2021 --check wgpu-types/src/limits.rs wgpu-core/src/validation.rs wgpu-core/src/device/resource.rs wgpu-core/src/pipeline.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs tests/src/params.rs
git diff --check
```

The RTX result is a compiler/validation check at forced FL9_1, not a
depth-writing GPU render or a historical-driver claim. Next continue the
public adapter contract audit; the public capability snapshot and HWND
presentation gates remain unresolved.

## Checkpoint — FL9_1 fragment position validation

Core now exposes `DownlevelLimits::fragment_position` and rejects a selected
fragment entry point that actually consumes `@builtin(position)` when the
capability is unavailable. The controlled FL9_1 fixture sets it to false.
Unlike `front_facing`, FXC strips an unused `SV_POSITION` input, so an unused
declaration and a safe sibling entry point remain valid. Naga validated
function expression refcounts are used to detect direct and structure-member
reads; helper arguments are traced recursively through the validated call
graph, including helpers that only forward a structure or vector to another
helper. Opaque shader interfaces are rejected conservatively. The capability
does not change normal D3D11 adapter enumeration.

Verification used Rust 1.93.1/MSVC and a serial Cargo build while a separate
Horizon build was consuming system memory:

- Fragment-position analysis CPU test: 1/1, including direct, structured,
  helper, forwarded helper, unused and sibling-entry cases.
- Strict controlled core suite: 77/77 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. Its
  offscreen fixture performed the existing FL9_1 rendering/readback cases and
  verified used-position rejection and unused-position acceptance.
- `wgpu-types` library tests: 63/63; 14 pre-existing unused serde-import
  warnings.
- Core zero-D3D11-adapter isolation: 1/1.
- Core DX12 regression: 74/74.
- `cargo check` for HAL `dx11,gles`, `wgpu-info`, and `wgpu-test --tests`
  passed.
- Core Clippy passed with two pre-existing `op_ref` warnings in
  `dx11_fixture_tests.rs`.
- Targeted Rust formatting and `git diff --check` passed.

The strict fixture is one hardware test containing multiple actual draws and
pixel checks; it is not historical FL9_1 validation. The private HAL suite was
not rerun for this core-only validation change. Code commit: `e8c7240da`
(`core: gate FL9_1 fragment position use`). This progress entry, handoff and
capability documentation record the verified checkpoint. Next continue the
production adapter capability audit; ordinary D3D11 enumeration and public
HWND surface exposure remain gated.

## Checkpoint — reject single-sample alpha-to-coverage

Core now returns a validation error when `alpha_to_coverage_enabled` is true
with sample count one. This is the WebGPU multisample-state rule and prevents
the descriptor from reaching the private HAL, which only supports one-sample
pipelines and otherwise returned an unexpected HAL error. The controlled
fixture asserts the specific core error before continuing its GPU rendering.

Rust 1.93.1/MSVC verification:

- Strict fixture-enabled core suite: 78/78 passed. Its FL9_1 render/readback
  integration test ran on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
  software=false), requested FL9_1 and actual L9_1. The test exercised the
  new validation assertion and retained its existing checked GPU output.
- Core DX12 suite: 75/75 passed.
- Core Clippy passed.
- Targeted rustfmt check and `git diff --check` passed.
- The Visual Studio developer prompt printed its existing `Error: Unknown
  error` banner, but each build/test command exited successfully.

Commands, run from the MSVC developer prompt (`WGPU_DX11_REQUIRE_HARDWARE=1`
for the first command):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/pipeline.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The code change is limited to core validation, its controlled fixture
assertion, and the Unreleased validation changelog. The HAL was unchanged, so
its most recent full strict suite result remains the 337/337 recorded in the
capability-projection checkpoint. This does not enable D3D11 enumeration or
multisampling. Continue auditing the public core/HAL contract.

## Checkpoint — gate compute-stage bind-group layouts

Core now requires `DownlevelFlags::COMPUTE_SHADERS` for every bind-group-layout
entry whose visibility includes the compute stage, regardless of resource
type. Before this, core gated compute-visible storage buffers but let uniform,
sampler and sampled-texture layouts reach the FL9_1 HAL, which only accepts
vertex/fragment bindings. The controlled fixture verifies all three new
resource classes fail with the missing-compute-capability validation error;
the existing storage tests continue covering storage buffers.

Rust 1.93.1/MSVC verification:

- Strict fixture-enabled core suite: 78/78 passed. The RTX 3090 Ti
  (`0x10de:0x2203`, software=false) was requested and selected at FL9_1. The
  controlled integration case exercised core validation plus the existing
  checked GPU render/readback paths.
- Focused strict fixture: 1/1 passed.
- Core DX12 suite: 75/75 passed.
- Core Clippy passed.
- Targeted rustfmt and `git diff --check` passed.
- The ordinary D3D11 zero-adapter isolation test remains part of the fixture
  suite and passed; normal enumeration remains disabled.

Commands, run from the MSVC developer prompt (`WGPU_DX11_REQUIRE_HARDWARE=1`
for the two fixture commands):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The core validation gate is shared and leaves adapters with compute support
unchanged. No HAL code changed, so the last full strict HAL suite remains
337/337. Continue auditing valid core requests against the private HAL before
considering public enumeration.

## Checkpoint — gate comparison-sampler layouts

Core now requires `DownlevelFlags::COMPARISON_SAMPLERS` when creating a bind
group layout containing a comparison sampler. The HAL supports only filtering
and non-filtering samplers; before this check, core allowed the layout and the
private HAL returned an unexpected error. The controlled fixture now verifies
that comparison-layout rejection occurs in core, in addition to the existing
comparison-sampler object creation rejection.

Rust 1.93.1/MSVC verification:

- Strict fixture-enabled core suite: 78/78 passed. The RTX 3090 Ti
  (`0x10de:0x2203`, software=false) was requested and selected at FL9_1; its
  core fixture performed the existing checked render/readback work.
- Focused strict fixture: 1/1 passed.
- Core DX12 suite: 75/75 passed.
- Core Clippy passed.
- Targeted rustfmt and `git diff --check` passed.
- Ordinary D3D11 zero-adapter isolation remains covered by the passing core
  suite; enumeration remains disabled.

Commands, run from the MSVC developer prompt (`WGPU_DX11_REQUIRE_HARDWARE=1`
for the fixture commands):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

No HAL source changed. Public D3D11 enumeration and surface exposure remain
gated; continue auditing core-valid layout and render operations.

## Checkpoint — gate empty bind-group visibility

The FL9_1 projection now reports `DownlevelLimits::empty_binding_visibility =
false`, and core rejects a bind-group-layout entry with an empty shader-stage
set before calling the HAL. This closes another legal-WebGPU-layout/HAL-rejection
mismatch. The limit defaults to true for existing backends, participates in
limit checks and the WebGPU-compliance predicate, and has serde defaulting for
older serialized capability values. GLES and noop explicitly report support;
the private DX11 projection reports unsupported. `wgpu-info` prints the new
limit. The changelog records the new validation behavior.

Rust 1.93.1/MSVC checks:

- `wgpu-types` library tests passed 66/66; with serde passed 67/67.
- Strict fixture-enabled core suite passed 78/78. The RTX 3090 Ti
  (`0x10de:0x2203`, software=false) executed at requested/actual FL9_1; the
  fixture specifically verified empty visibility is rejected by core before
  HAL and retained the existing render/readback checks.
- Full private HAL suite passed 337/337. Strict hardware mode produced 144
  physical-adapter reports across the suite, all selecting the RTX 3090 Ti at
  FL9_1. The suite reported no skipped or failed cases.
- Core DX12 suite passed 75/75. The ordinary D3D11 zero-adapter isolation test
  passed 1/1.
- HAL test compilation with `dx11,gles`, `wgpu-info` check, `wgpu-test` test
  check, and core fixture Clippy passed.
- Targeted rustfmt and `git diff --check` passed. Whole-workspace
  `cargo fmt --all -- --check` reports pre-existing differences in untouched
  `naga/src/back/hlsl/writer.rs` and `wgpu-core/src/instance.rs`.
- The Visual Studio developer prompt continues to print its existing
  `Error: Unknown error` banner while the invoked commands complete normally.

Commands (run from the Visual Studio developer prompt; strict commands set
`WGPU_DX11_REQUIRE_HARDWARE=1`):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib
cargo +1.93 test --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check tests/src/params.rs wgpu-core/src/binding_model.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

Public D3D11 adapter enumeration and HWND surface exposure remain disabled.
After this checkpoint, continue the core/HAL layout and rendering audit and
finish the broader capability projection; these individual gates do not make
the production adapter ready for enumeration.

## Checkpoint — gate unsupported sampled texture layout types and samples

The D3D11 projection now reports depth, signed-integer, unsigned-integer and
multisampled texture bindings as unavailable. Core rejects these BGL entries
before HAL, whose private layout mapper currently represents only single-sample
float sampled textures. Core also rejects vertex-only texture or sampler
layouts when the projected per-vertex sampled-texture limit is zero.
Fragment-visible resources with additional vertex visibility remain accepted
because the HAL can represent that layout shape; actual vertex-stage sampling
continues to fail at selected-entry pipeline validation.

Rust 1.93.1/MSVC verification:

- `wgpu-types` library tests passed 68/68; with serde passed 70/70. The serde
  test removes all newly added binding-capability fields from an old-shape
  serialized value and verifies their compatibility defaults.
- Strict fixture-enabled core suite passed 78/78 on the RTX 3090 Ti
  (`0x10de:0x2203`, software=false), forced and confirmed FL9_1. It exercised
  core rejection for depth, signed/unsigned integer, unfilterable multisampled
  and vertex-only sampled layouts, verified shared fragment/vertex visibility
  remains accepted, then completed existing textured depth renders and
  readback.
- Core DX12 suite passed 75/75.
- Strict physical D3D11 projection tests passed 4/4; the hardware case reported
  RTX 3090 Ti, vendor `0x10de`, device `0x2203`, software=false, requested
  FL9_1 and actual `L9_1`.
- Full private HAL suite passed 337/337. Strict mode logged 144 physical
  adapter identities and 101 GPU assertion lines. Six nested render checks
  explicitly skipped drawing because their unsupported dynamic-indexing shader
  was rejected at fragment linkage; they passed only the expected rejection
  checks and are not counted as GPU-rendered subcases.
- HAL test compilation with `dx11,gles`, `wgpu-info` check, `wgpu-test` test
  check, core fixture Clippy, targeted rustfmt and `git diff --check` passed.
- The Visual Studio developer prompt continues to print its existing
  `Error: Unknown error` banner while commands complete normally.

Commands run from the Visual Studio developer prompt. The core fixture, HAL
projection and full HAL commands set `WGPU_DX11_REQUIRE_HARDWARE=1`:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib
cargo +1.93 test --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check tests/src/params.rs wgpu-core/src/binding_model.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/gles/adapter.rs wgpu-hal/src/noop/mod.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs
git diff --check
```

Normal D3D11 adapter enumeration and HWND surface exposure remain disabled.
The next audit should continue with other core-valid layout and render shapes
that the HAL rejects, followed by the production capability projection.

## Checkpoint — pipeline override constants

The private D3D11 pipeline no longer rejects nonempty `ProgrammableStage`
constant maps. Shader compilation now resolves overrides with Naga's
`process_overrides` for the selected stage and entry point before entry-point
analysis, resource mapping, HLSL generation and FXC compilation. The ordinary
and viewport-remapped vertex shader paths share the same values. Invalid and
unknown overrides return the existing stage-specific pipeline-constant error.

The strict controlled core fixture rendered explicit fragment and vertex
overrides (`tint = 0.75`, readback red 191) and WGSL defaults (`tint = 0.25`,
readback red 64); each expected value appeared in all 64 checked pixels.
Unknown overrides were rejected with the corresponding fragment or vertex
pipeline-constant validation error. This also verifies that override
resolution precedes the private backend's selected-entry safety checks, rather
than treating `Expression::Override` as an unsafe runtime expression.

Rust 1.93.1/MSVC verification on 2026-11-04:

- Strict private D3D11 HAL suite: 337/337 passed with
  `WGPU_DX11_REQUIRE_HARDWARE=1`; real hardware cases explicitly selected the
  NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false) at requested and
  actual FL9_1.
- Strict controlled core suite: 78/78 passed. The core fixture used the same
  RTX 3090 Ti at requested and actual FL9_1 and performed the override
  readbacks above.
- Core DX12 regression suite: 75/75 passed.
- Ordinary D3D11 zero-adapter isolation test: 1/1 passed.
- HAL test compilation with `dx11,gles`, core Clippy, HAL Clippy, targeted
  rustfmt and `git diff --check` passed.
- Visual Studio's developer prompt printed its existing `Error: Unknown
  error` banner; all listed commands exited successfully.
- The vertex-override extension reran the strict controlled core suite at
  78/78 and core Clippy. Its first targeted rustfmt check found two formatting
  differences in the new assertions; formatting was applied to the changed
  fixture file and the targeted check then passed. No functional test failed.

Commands (run after entering the Visual Studio developer environment; strict
hardware commands set `WGPU_DX11_REQUIRE_HARDWARE=1`):

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/hal_shader.rs
git diff --check
```

Code changes are limited to `hal_shader.rs`, `hal_pipeline.rs` and the
controlled core fixture. Normal D3D11 adapter enumeration and public HWND
surface exposure remain disabled. Existing GPU tests already cover nested
uniform structures, an asymmetric matrix, stride-sensitive arrays and bounded
ranges. Runtime integer indexing remains unavailable. Continue the core/HAL
contract audit and production capability projection.

## Current checkpoint — strict shader audit and unchecked runtime indexing

The FL9_1 shader-module compiler test now fails instead of silently returning
success when `WGPU_DX11_REQUIRE_HARDWARE=1` and no physical adapter is found. It
prints the selected physical adapter and asserts the actual device level is
FL9_1. The existing dynamic uniform-indexing case now runs under both checked
and unchecked `ShaderRuntimeChecks`: literal indexing compiles in each mode,
while runtime integer indexing is rejected in each mode by the backend's
unconditional FL9_1 safety guard. This test creates native shader objects; it
does not issue a draw or count as rendering evidence.

On 2026-11-04, the focused strict test passed 1/1 using the NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, software=false), requested FL9_1 and actual FL9_1.
HAL Clippy, targeted rustfmt and `git diff --check` passed. The Visual Studio
developer prompt emitted its existing `Error: Unknown error` banner, but the
commands exited successfully. Exact test command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE='1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 hal_module_compiles_and_creates_level_9_shaders -- --nocapture --test-threads=1
```

The next reviewed mismatch is sampler anisotropy: core models the downlevel
flag as support up to 16, while the FL9_1 HAL accepts at most 2. A truthful
numeric downlevel limit is needed so core can clamp requests consistently
before HAL. Public D3D11 enumeration and HWND surface exposure remain gated.
Code checkpoint: `3e725d3f4` (`dx11: verify unchecked FL9_1 shader indexing rejection`).

## Checkpoint — numeric sampler-anisotropy capability

Core previously treated `DownlevelFlags::ANISOTROPIC_FILTERING` as permission to
pass every requested anisotropy value up to 16. The private FL9_1 HAL can create
only values through 2. `DownlevelLimits::max_sampler_anisotropy` now expresses
the numeric cap separately from the availability flag; core clamps to the
reported cap (and still sends 1 when the flag is absent). The cap is not a
WebGPU-required limit because WebGPU permits implementations to ignore
anisotropy. Old serde payloads default the new field to 16.

The FL9_1 wgpu projection reports cap 2 and retains the anisotropy flag. The
controlled core fixture created real D3D11 sampler states from requests 2, 3
and 16; the latter requests reached the HAL clamped to the legal maximum. The
single-mip test image cannot establish anisotropic quality on minified content.
Vulkan now reports its selected device maximum capped at 16, GLES reports its
queried maximum capped at 16, and the noop backend uses 16. `wgpu-info` prints
the limit. Normal D3D11 adapter enumeration remains empty.

Rust 1.93.1/MSVC verification on 2026-11-04:

- `wgpu-types` full library suite: 69/69; serde suite: 71/71, including the
  old-shape default for the added field.
- Core anisotropy clamp unit tests: 2/2.
- Strict controlled core suite: 80/80 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. The fixture
  reported cap 2 and successfully created states for requested clamps 2, 3 and
  16. Full readback and render cases in the fixture also passed.
- Strict private D3D11 HAL suite: 337/337. It reported the same physical GPU
  and forced feature level; sampler translation and rendered sampler cases
  remained green.
- Core DX12 regression suite: 77/77; D3D11 zero-adapter isolation: 1/1.
- HAL test compilation with `dx11,gles,vulkan`, `wgpu-info` check, `wgpu-test`
  test check and Clippy for wgpu-types, fixture-enabled wgpu-core, D3D11 HAL,
  and GLES/Vulkan HAL passed.
- Targeted rustfmt and `git diff --check` passed. The serde types build retains
  fourteen pre-existing unused-serde-import warnings. Visual Studio's developer
  prompt still prints its existing `Error: Unknown error` banner.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib
cargo +1.93 test --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampler_anisotropy_limit -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE='1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE='1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 check --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11,gles,vulkan
cargo +1.93 check --offline -j16 -p wgpu-info
cargo +1.93 check --offline -j16 -p wgpu-test --tests
cargo +1.93 clippy --offline -j16 -p wgpu-types --lib
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features gles,vulkan
```

A first compile exposed one missing test constant field and a Vulkan query typo;
both were corrected and the complete checks above then passed. An initial core
Clippy run pointed out a clamp idiom, which was corrected and Clippy rerun clean.
The next work remains the core/HAL capability audit and a truthful production
adapter contract. HWND `Present` reentrancy remains a separately documented
blocker; public D3D11 enumeration and surface exposure stay disabled.
Code checkpoint: `0b38756c1` (`core: honor downlevel sampler anisotropy limits`).

## Checkpoint — reject selected-entry texture loads in both runtime-check modes

The private FL9_1 shader compiler now rejects a reachable Naga `ImageLoad`
unconditionally. Previously this fail-closed restriction applied only when
shader runtime checks were enabled, despite the unsupported-operation comment
and FXC's existing rejection evidence. The native bounds proof remains an
isolated experiment and does not establish production texture-load semantics.
The hardware shader-module test verifies the selected load entry is rejected
and a safe sibling entry still compiles with both checked and unchecked runtime
checks.

Verification used the Rust 1.93.1 Visual Studio x64 environment. The focused
strict shader test passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
software=false), with requested and actual FL9_1. This checks shader compilation
and shader-object creation, not a texture-load draw. The complete strict
private D3D11 suite passed 337/337 with `WGPU_DX11_REQUIRE_HARDWARE=1`,
including its existing rendering/readback cases. The Visual Studio prompt
continues to print its known `Error: Unknown error` startup banner.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 hal_module_compiles_and_creates_level_9_shaders -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
```

This does not remove the experimental texel-load proof and does not enable
public D3D11 enumeration. The remaining mission gates are the core/HAL contract
audit, production capability projection, device loss/lifetime review and the
documented HWND Present reentrancy blocker.

## Checkpoint — complete the projected POT mip-usage matrix

The core/HAL audit found that format projection could permit POT RGBA8/BGRA8
mips for `RESOURCE`-only, `COPY_SRC`-only and `RESOURCE | COPY_SRC` textures,
while the private HAL rejected those usage shapes. Since format capabilities do
not carry usage-specific mip ceilings, the HAL now accepts every nonempty
combination of its four projected uses (`RESOURCE`, `COLOR_TARGET`, `COPY_SRC`,
`COPY_DST`). Backend mip-family switches and the actual MIP, shader-sample and
render-target format queries still gate the request.

CPU capability tests cover all fifteen nonempty usage combinations and retain
shape, backend-family, and driver-query rejection coverage. The strict FL9_1
sampled-mip test passed 74 GPU cases, zero skips, on NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, software=false). New hardware cases sampled zero-initialized
`RESOURCE` and `RESOURCE | COPY_SRC` mip chains into 64 transparent pixels per
format, and copied/read back a 256-byte zeroed `COPY_SRC`-only mip subresource
for RGBA8 and BGRA8. The strict controlled core fixture successfully created
all three formerly mismatched descriptors through the projected adapter path.
The complete strict private HAL suite passed 337/337. No historical FL9 GPU
claim is made.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 dx11::capabilities::tests:: -- --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_sampled_pot_mip_chains -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

Ordinary D3D11 enumeration remains disabled. The production adapter review,
remaining core/HAL audit, lifetime/device-loss review, and HWND Present
reentrancy blocker remain open.

## Checkpoint — request a core device from the unmodified FL9_1 projection

The controlled fixture now has a second route that uses the selected physical
device's exact D3D11 capability projection without test overrides. It builds a
test-only `ExposedAdapter` from the selected RTX 3090 Ti device, confirms the
normal HAL enumeration remains empty, asks core for a device using the exact
projected features and limits, and creates each of the 15 nonempty RGBA8 and
BGRA8 usage subsets advertised by the projection. It creates sampled and
render-target views where those usages apply, plus the projected R8_UNORM,
R8G8_SNORM, R8G8B8A8_SNORM and D24S8 resource/view shapes. This is a
test-injected, non-enumerated adapter: it exercises core request and resource
validation against the unmodified projection, but does not enable production
adapter exposure or HWND surfaces.

On 2026-11-04, strict execution used NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, software=false), explicitly requested and confirmed FL9_1.
The new exact-projection test passed, and the complete strict controlled core
fixture passed 81/81 with no hardware skips. The existing full core fixture
render/readback cases also ran in that suite. Targeted DX12 core regression
passed 77/77, and the zero-D3D11-adapter isolation test passed 1/1. Core
Clippy passed with three warnings in pre-existing fixture code (a nested `if`
and two needless references); none point into the new test. Targeted rustfmt
and `git diff --check` passed. The developer prompt continues to emit its
existing `Error: Unknown error` banner.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The production capability report still needs a full audit against every
core-valid operation. Device-loss/lifetime review remains open, and HWND
`Present` lock/reentrancy still blocks public presentation and normal D3D11
adapter exposure.

## Checkpoint — keep core buffer padding within D3D11 `ByteWidth`

The exact selected-device projection audit found that `max_buffer_size =
u32::MAX` was not safe for the current core allocation path. Core rounds every
buffer up to `COPY_BUFFER_ALIGNMENT` and adds one sentinel byte to vertex
buffers before rounding. A legal request at the old reported maximum could
therefore reach the HAL as a 4 GiB `ByteWidth`, which cannot fit the native
32-bit field. The FL9_1 projection now caps the logical request at
`u32::MAX - COPY_BUFFER_ALIGNMENT`, the largest value for which both the
vertex sentinel path and ordinary four-byte rounding fit in `ByteWidth`. The
synthetic projection test and exact-device core fixture assert this value.

On 2026-11-04, all four HAL projection tests passed; the selected-device case
reported NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, software=false), requested
FL9_1, actual FL9_1. The strict core fixture passed 81/81 with no skips,
including the unmodified projection request and resource/view creation test as
well as existing GPU render/readback cases. HAL Clippy passed. Core test Clippy
passed with the same three warnings in pre-existing fixture code (one
collapsible `if` and two needless references); the new test produced no
warning. Targeted rustfmt passed. An initial invocation outside the Visual
Studio environment could not find `link.exe`; rerunning through the installed
VS 2022 developer environment succeeded. That environment continues to print
its known `Error: Unknown error` banner.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The limit is derived from core's allocation padding rather than a historical
GPU guarantee. Large successful allocations are still subject to memory
availability. Continue auditing the remaining core/HAL descriptor boundary,
device loss/lifetime behavior and the unimplemented production adapter path.

On 2026-11-05 the exact-projection core test was extended to request a vertex
buffer one byte above the reported maximum and assert core returns
`CreateBufferError::MaxBufferSize` before HAL allocation. The strict fixture
was rerun at 81/81 with no skips on the same RTX 3090 Ti, requested and actual
FL9_1. The DX12 core regression passed 77/77 and the zero-D3D11-adapter
isolation test passed 1/1 after the projection change. Core Clippy passed with
the same three pre-existing fixture warnings and no warning in the new test;
targeted formatting and `git diff --check` passed.

Additional commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
```

## Checkpoint — drain partial command replay failures (uncommitted)

The lifetime audit found that native replay may fail after earlier commands in
the same submission have already reached the immediate context. Core tracks
completion only after HAL submission succeeds, so returning immediately could
release the borrowed command buffers before that partial work completes. HAL
now ends and flushes an event query on replay failure, then synchronously waits
for that query before returning the original replay error. The failed submission
does not advance or signal the queue fence. A test-only encoder case records a
real buffer copy followed by an injected failure; the strict hardware test
checks the copied bytes after the error and verifies the fence value remains
unsignaled. If the event query itself fails, the wait error is returned; actual
device-removal recovery and retention after an unqueryable device remain open.

The exact-projection core fixture additionally verifies that the selected
FL9_1 projection rejects D1/D3 texture creation and D2Array/Cube/CubeArray view
creation in core, including a one-layer array view. It also rejects
DepthOnly/StencilOnly D24S8 render views before HAL. `CAPABILITIES.md` and
`STATUS.md` now accurately state that numeric 1D/3D limits remain 2048/256 while
the downlevel dimension booleans are false; the limits alone do not enable
those shapes.

Rust 1.93.1 / Visual Studio x64 checks on 2026-11-05, run locally on
`Ben-Desktop` from `C:\dev\personal\wgpu` with 32 Cargo jobs:

- Strict partial-replay test: 1/1 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. A real GPU
  buffer copy executed before the injected replay error; its output was intact
  after return and the failed submission did not signal its fence.
- Strict private D3D11 HAL suite: 337/337 Rust tests, zero failures. The suite
  exercised hardware rendering, transfers, fences, readback, texture formats,
  and lifetime cases on the same physical GPU. Several unsupported shader
  experiments explicitly skipped their draw phase after expected compiler
  rejection; those skips are not GPU passes.
- Strict controlled core fixture: 81/81 tests, zero failures. The selected RTX
  3090 Ti ran at requested/actual FL9_1; core rendering/readback and projection
  validation executed, including the new pre-HAL shape rejections. Strict mode
  had no missing-hardware skips.
- DX12 core regression: 77/77. Ordinary D3D11 zero-adapter isolation: 1/1.
- HAL Clippy passed with nine existing warnings in untouched HAL tests; core
  Clippy passed with the same three pre-existing fixture warnings. The initial
  run identified two warnings in the new test code; both were fixed and HAL
  Clippy rerun clean of new warnings.
- Targeted rustfmt and `git diff --check` passed.

Commands (Visual Studio x64 developer environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 replay_failure_reaches_queue_and_does_not_signal_fence -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/fence.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The checkpoint was committed as `e32a29732e672cc96e9e2b9dbff3474425e9ae66`.
Continue the failure-path/device-loss audit and the truthful
production adapter work. Normal enumeration and surface capabilities remain
disabled; synchronous HWND `Present` reentrancy is still unresolved.

## Checkpoint — run multiple uniforms through the exact core projection

The private HAL already allocates thirteen uniform slots across all stages and
bind groups, but the internal FL9_1 projection exposed only one per shader
stage. The projection now exposes six per stage: the worst-case union of six
vertex-only and six fragment-only bindings is twelve, within the backend's
thirteen-slot cap. The two-stage count remains conservative and avoids
advertising a layout that the shared allocator would reject.

The controlled fixture adds two independent fragment uniform buffers at sparse
bindings 17 and 23. Each binds a nonzero 256-byte-aligned range with a distinct
native constant-buffer slot; both values multiply the sampled texture. Strict
RTX 3090 Ti readback at requested/actual FL9_1 matched all 64 pixels: red-side
pixels were `[64, 0, 0, 255]`, green-side pixels were `[0, 64, 0, 255]`.
The exact-projection core test also creates a seven-uniform fragment layout
and verifies core rejects it against the projected limit of six before HAL.
Ordinary D3D11 adapter enumeration remains empty.

Verification on 2026-11-05 used Rust 1.93.1 and the Visual Studio x64
environment on `Ben-Desktop` (`C:\dev\personal\wgpu`, 32 Cargo jobs):

- The strict core offscreen case passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested/actual FL9_1. Its two independent
  fragment uniforms changed the sampled output; all 64 pixels matched the
  red-side `[64, 0, 0, 255]` and green-side `[0, 64, 0, 255]` expectations.
- The exact selected-device projection test passed 1/1 and reported that core
  rejected seven fragment uniform bindings against the limit of six before
  HAL. This test requested a core device with the unmodified projection.
- The complete strict core fixture passed 81/81 with no missing-hardware skips;
  it included the two-uniform draw/readback. The exact-projection test above was
  run separately as well.
- HAL projection tests passed 4/4. The physical query reported the six-uniform
  stage cap on the forced FL9_1 RTX 3090 Ti; optional format and NPOT queries
  remained separate from implementation guarantees.
- The strict HAL suite passed 337/337. 148 test executions printed the
  selected physical adapter; six nested GPU draw experiments explicitly
  skipped rendering after expected FXC rejection, and are not GPU passes.
- DX12 core regression passed 77/77; ordinary D3D11 adapter isolation passed
  1/1.
- HAL and core Clippy passed. Existing warnings remain: nine HAL warnings in
  old tests/untouched code and three core fixture warnings; the new fixture
  introduced no lint warning. Targeted rustfmt and `git diff --check` passed.
- The first compile found a test-only `Option<u64>` versus `NonZeroU64`
  mismatch; the `BufferBinding` size was corrected. One test invocation
  outside the developer prompt could not find `link.exe`; rerunning through
  `VsDevCmd.bat` succeeded. The prompt still prints `Error: Unknown error`.

Commands (run inside the Visual Studio x64 developer environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

This capability increment was committed as
`0d86bcdd240fe98f61cfc085f250edcacbd41255`. Public adapter enumeration remains disabled.

## Checkpoint — exercise vertex and fragment uniforms through core

The controlled core renderer now includes a pipeline with sparse uniform
binding 17 visible only to the vertex stage and binding 23 visible only to the
fragment stage. Both use nonzero 256-byte-aligned ranges and distinct native
stage registers. The vertex value scales the sampled geometry horizontally;
the fragment value halves the sampled color. Readback checks all 64 pixels,
including the reduced coverage and tinted region. This case uses the selected
device's capability projection without overriding its uniform limits; the
fixture's existing conditional-NPOT override is unrelated.

Rust 1.93.1 / Visual Studio x64 checks on 2026-11-05, from
`C:\dev\personal\wgpu` on `Ben-Desktop`, with 16 Cargo jobs:

- The strict FL9_1 core renderer passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. The new
  vertex/fragment uniform case ran through core and D3D11 and verified all 64
  pixels. The 16 covered pixels matched red tinted by 0.5 (UNORM conversion
  accepted within one channel value); the remaining 48 pixels matched the
  blue clear color. The existing two-fragment-uniform test still passed.
- The complete controlled core fixture suite passed 81/81 with strict hardware
  mode enabled and no missing-hardware skip. Its single hardware fixture
  executed all contained transfer, validation and rendering cases.
- The exact selected-device projection test passed 1/1 and continued to reject
  seven fragment uniforms before HAL. Ordinary D3D11 adapter isolation passed
  1/1. DX12 core regression passed 77/77.
- Core Clippy passed with its three existing warnings at unrelated fixture
  lines. Targeted rustfmt and `git diff --check` passed.
- Initial assertions exposed a one-unit UNORM rounding difference and an
  incorrect assumption about the fixture's second triangle coverage. The
  final test uses a one-unit tolerance for the tinted channel and checks the
  reduced first-draw rectangle; no backend behavior was changed to mask either
  observation.

Commands (inside the Visual Studio x64 environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

Next continue auditing the exact core/HAL contract, with priority on resource
layout and render-pass combinations that remain core-legal but unsupported by
the private backend. The production adapter and public surface gates stay
closed; synchronous HWND `Present` reentrancy remains unresolved.

The stage-specific uniform fixture and its verification record were committed
as `2ae141279`.

## Checkpoint — verify two dynamic uniforms through core

The exact selected-device FL9_1 projection exposes two dynamic uniform
bindings per pipeline layout. The core fixture now binds logical binding 17
only to the vertex stage with base/dynamic offsets 256/256 and logical binding
23 only to the fragment stage with offsets 256/512. Each uniform references a
different aligned range. The vertex value scales geometry and the fragment
value tints a sampled texture, making both offset selections observable in the
same draw. The exact-projection test rejects three dynamic entries at
bind-group-layout creation with `DynamicUniformBuffers { limit: 2, count: 3 }`.
Normal D3D11 enumeration remains empty.

Verification on 2026-11-05 used Rust 1.93.1 and Visual Studio x64 on
`Ben-Desktop`, in `C:\dev\personal\wgpu`, with 16 Cargo jobs:

- Strict core rendering passed on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested and actual FL9_1. The new
  two-dynamic-uniform draw passed through core, private HAL and D3D11; all 64
  pixels matched. Sixteen covered pixels were red tinted by 0.5 (UNORM
  tolerance one); the other 48 matched blue clear. The existing static
  two-uniform and stage-specific uniform draws also passed.
- The complete controlled core fixture suite passed 81/81 with strict hardware
  mode and no missing-device skip. The exact selected-projection test passed
  1/1, including the three-entry rejection. DX12 core regression passed 77/77,
  and ordinary D3D11 zero-adapter isolation passed 1/1.
- Core Clippy passed with the same three pre-existing fixture warnings and no
  new warnings. Targeted rustfmt and `git diff --check` passed.
- An initial negative test expected rejection at pipeline-layout creation,
  but core rejects the over-limit dynamic entries at bind-group-layout
  creation. The final test asserts that earlier, more precise validation point.

Commands (inside Visual Studio x64; strict mode is required for the GPU run):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

The projection, core test and documentation changes were committed as
`8d19d14c9`. Next continue auditing core-legal pipeline/resource combinations
against the private HAL, and keep ordinary adapter and surface exposure
disabled.

## Checkpoint — execute a two-group uniform/texture pipeline through core

The controlled core fixture now has a `MultiGroupUniformBindings` case. It
creates a group-0 layout containing only a fragment-visible uniform at sparse
binding 17, and a group-1 layout containing a filtering sampler at binding 42
and sampled texture at binding 7. WGSL uses those exact logical identities;
the pipeline layout carries both group layouts, and the render pass binds the
two groups independently. The uniform halves the red channel of the texture.
Strict forced-FL9_1 readback checks the full 8x8 target: the left half is
`[128, 0, 0, 255]` within one UNORM unit, and the right half is
`[0, 255, 0, 255]`. This establishes one two-group core-to-HAL path, not every
combination or the full eight-group limit. Ordinary D3D11 enumeration remains
empty.

Verification on 2026-11-05 used Rust 1.93.1 and Visual Studio x64 on
`Ben-Desktop`, with `-j16` and strict hardware mode for D3D11 runs:

- Targeted core fixture passed 1/1 on NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, software=false), requested FL9_1 and actual L9_1. The new
  multi-group draw and all existing fixture cases rendered/read back as
  expected.
- Full controlled core fixture suite passed 81/81, including the real hardware
  render. The selected-device projection test passed 1/1 and rejected three
  dynamic uniforms against its projected limit of two. The zero-public-adapter
  isolation test passed 1/1.
- Full private D3D11 HAL suite passed 337/337 under strict mode. Some tests
  intentionally report nested shader-render skips after the expected FXC
  rejection; these are unsupported-shader negatives, not successful GPU
  executions. The suite's positive GPU cases reported the physical adapter and
  FL9_1.
- DX12 core regression passed 77/77. D3D11 HAL build passed. Core and HAL
  Clippy completed with existing warnings only: three core fixture warnings,
  seven `single_range_in_vec_init` test warnings in `hal_device.rs`, one
  `items_after_test_module` warning in `swapchain.rs`, and one `needless_borrow`
  warning in `fl9_texel_load.rs`. No warning points at the new fixture code.
- Rustfmt and `git diff --check` passed.

Commands (inside the Visual Studio x64 environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std requesting_dx11_does_not_expose_a_stub_adapter -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 build --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
```

## Checkpoint — opt-in public FL9_1 offscreen adapter

Commit `6d077bdee` exposes only physical DXGI adapters that successfully create
an explicitly requested FL9_1 device. Software adapters are filtered, no
higher-level fallback is attempted, and an adapter is returned only when the
selected-device capability projection exists. Adapter metadata reports the
name, PCI vendor/device IDs and the DXGI driver-package version when
`CheckInterfaceSupport(IDXGIDevice)` succeeds. The public feature set is empty,
the adapter reports a non-WebGPU-compliant downlevel contract, and
`surface_capabilities` remains `None`. No claim of a complete public adapter
contract or presentation support is made.

The strict public wgpu integration test selects the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), reports requested and actual FL9_1,
and excludes software adapters. Through ordinary public adapter enumeration it
requests two devices, uploads a two-color texture, binds sparse logical
bindings 7/42, draws indexed textured geometry with a D24S8 depth attachment,
and verifies the full 8x8 output: 32 red pixels and 32 green pixels. The second
device independently clears and reads back 64 green pixels. This verifies a
meaningful public path and sequential device opens over the adapter's shared
native D3D11 device/context; it is not the required complete 3D application
scene, a concurrent multi-device test, a full contract audit, or a historical
driver result.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio x64, bounded `-j16`,
strict hardware mode for DX11 runs:

- `cargo +1.93 build --offline -j16 -p wgpu-hal --no-default-features --features dx11` passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1; both public render/readback cases executed on the RTX at FL9_1.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1` passed 337/337. The suite includes CPU-only checks and strict physical-GPU tests; nested shader-render skips are expected negative FXC cases, not GPU-render passes.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1` passed 81/81, including physical forced-FL9_1 fixture rendering and ordinary public enumeration of the projected adapter.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl` passed 77/77.
- HAL, core and wgpu Clippy checks passed. Existing warnings remain: nine HAL test warnings (seven single-range-in-vector, one items-after-test-module, one needless-borrow) and three core fixture warnings (one collapsible-if, two op-ref); the new adapter/test code introduced none.
- Rustfmt `--check` on changed Rust files and `git diff --check` passed. No test skipped in the strict public test; ordinary runs print a recognizable SKIP when physical FL9_1 hardware is unavailable.

Exact commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 build --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/instance.rs wgpu-hal/src/dx11/adapter.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/device.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/instance.rs wgpu/tests/dx11_fixture.rs wgpu/tests/dx11_public.rs
git diff --check
```

The public projection and core/HAL operation audit remain incomplete. The
independent review found no confirmed core-valid route to the scanned unsupported
compute/query/cache/acceleration-structure panic paths, but this is not an
exhaustive audit. Continue auditing every advertised pipeline, resource,
mapping, pass and error path. Keep public surfaces disabled due to the
documented synchronous `Present` reentrancy deadlock. Historical FL9_1 drivers
remain untested.

## Checkpoint — public stage uniforms and two bind groups

Commit `a0f29c5ab` expands the ordinary public wgpu integration test. Group 0
contains a sparse vertex-visible transform uniform at binding 3 and a separate
fragment-visible tint uniform at binding 17. Group 1 contains a sampled
texture/sampler pair at bindings 7/42. Queue uploads initialize the uniforms
and texture; indexed textured geometry is transformed, depth-tested using
D24S8 and read back through public wgpu APIs. All 64 pixels matched: 32
tinted-red `[128, 0, 0, 255]` pixels (within one UNORM unit) and 32 green
`[0, 255, 0, 255]` pixels. The second device still independently cleared and
read back 64 green pixels.

Strict hardware execution on 2026-11-05 used Rust 1.93.1 and the Visual Studio
2022 x64 developer environment:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, requested and actual FL9_1, `software=false`.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed.

The first test invocation from a non-developer shell lacked `link.exe`; the
same test passed after invoking the installed Visual Studio x64 developer
environment. No GPU skip occurred. No HAL/core code changed in this increment,
so their last full verified counts remain the prior checkpoint (HAL 337/337,
core 81/81, DX12 core 77/77). Continue with the remaining exact core/HAL
capability contract audit; do not widen limits without coverage and keep public
surface support disabled until safe Present scheduling is established.

## Checkpoint — public transform update across submissions

The public integration now changes the vertex transform with
`Queue::write_buffer`, submits the same indexed textured/depth-tested pipeline
again, and verifies a second full readback. The initial image matched 32
tinted-red and 32 green pixels. The updated transform shifted the red quad by
two columns; the second image matched 16 blue clear pixels, 32 tinted-red
pixels and 16 green pixels. This confirms the selected public uniform binding
is updated between submissions and changes the rendered geometry while the
pipeline, texture/sampler bindings and target remain reused.

Verification on 2026-11-05 used Rust 1.93.1, Visual Studio 2022 x64 and
`WGPU_DX11_REQUIRE_HARDWARE=1`:

- `cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1, `software=false`.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- Targeted rustfmt and `git diff --check` passed.

The first attempt used an incorrect expected image for the translated triangle
and failed with a diagnostic showing the complete row values; the test's
expected image was corrected to the observed geometric coverage and then
passed. No backend code changed. Continue auditing selected-device projection
and public core/HAL combinations; retain offscreen-only exposure and the known
surface Present reentrancy blocker.

## Checkpoint — public dynamic uniform ranges

The public stage-uniform integration now declares both group-0 uniforms as
dynamic. The vertex transform binding references a 64-byte range in a 512-byte
buffer. After the first submitted render and readback, `Queue::write_buffer`
uploads a translated transform at byte offset 256; the next render selects
that aligned range using dynamic offsets `[256, 0]`. Tint remains in the
original range. This validates the adapter's projected two-dynamic-uniform
limit through public wgpu and confirms the updated subrange affects pixels.

Verification on 2026-11-05 used Rust 1.93.1, Visual Studio 2022 x64 and strict
hardware mode:

- Public D3D11 integration test passed 1/1 on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1, `software=false`. The first readback matched 32 tinted-red/32 green pixels; the dynamic-offset render matched 16 blue/32 tinted-red/16 green; a second device matched 64/64 green pixels.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std`, targeted rustfmt and `git diff --check` passed.

The focused core fixture already covered dynamic uniform offsets, and this
extends the positive evidence to ordinary public wgpu adapter/device/queue
calls. Invalid offset rejection and all dynamic range combinations remain
part of the wider contract audit. Public surfaces remain unavailable pending
safe Present scheduling.

## Checkpoint — public maximum bind-group position

The public integration now places sampled texture/sampler resources in group
7, the final slot of the adapter's reported eight-group limit, and reuses
logical binding numbers 3 and 17 already used by group-0 vertex/fragment
uniforms. The pipeline layout includes all eight positions with empty gaps.
This proves the selected D3D11 HAL/compiler mapping retains `(group, binding)`
identity and uses separate native uniform/texture/sampler namespaces for this
cross-group configuration.

Strict execution on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) passed 1/1 at requested and actual FL9_1, with
`software=false`. The initial readback matched 32 tinted-red/32 green pixels;
the queue-uploaded dynamic transform selected at offset 256 shifted the second
readback to 16 blue/32 tinted-red/16 green pixels; a second public device
matched 64/64 green pixels. Public wgpu Clippy, targeted rustfmt and diff
checks passed. The exact strict test command remains the public test command
in the preceding checkpoint.

This covers the maximum group index with one two-group resource configuration;
it does not prove every resource type or the eight-group aggregate resource
capacity. Continue the contract audit. The public adapter remains experimental
and offscreen-only pending broad operation coverage and safe Present scheduling.

## Checkpoint — concurrent public device submissions

Commit `e8f6fe5f2` replaces the sequential second-device smoke operation with
two concurrent public wgpu workloads. Each independently opened device records
a clear followed by texture-to-buffer copy, submits, maps and reads back its
own buffer from a scoped worker thread. The first device produced 64/64 red
pixels and the second produced 64/64 green pixels. This exercises the shared
native immediate-context mutex across separate public devices and confirms
submission, synchronization and readback remain ordered for both queues.

Strict test on 2026-11-05 passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), requested and actual FL9_1, `software=false`, with
`WGPU_DX11_REQUIRE_HARDWARE=1`. Exact command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

`cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std`, targeted rustfmt and `git diff --check` passed. No skip or newly introduced warning. This covers concurrent queue submissions on the same adapter/context; it does not establish public presentation concurrency or device-loss behavior. Continue the core/HAL contract audit and retain the offscreen surface gate.

## Checkpoint — public unsupported-resource validation

Commit `4efecaf6e` adds negative checks through the ordinary public adapter and
device. With a validation error scope around each operation, core rejects:

- D3 texture creation while the selected FL9_1 projection disables 3D textures.
- 4x RGBA8 render-target creation while the backend's sample-count support is one.
- `R32Float` sampled texture creation because that format is not in the selected implementation projection.
- A two-mip D24S8 render attachment under the single-mip FL9_x restriction.

Each captured error was `wgpu::Error::Validation`, demonstrating that the
public core boundary rejects these requests before they reach the private HAL.
The strict public FL9_1 test still passed 1/1 on the RTX 3090 Ti at requested
and actual FL9_1, `software=false`, including group-7 sampled resources,
dynamic uniform transform readback and concurrent clear/readback on two
devices. Clippy, targeted rustfmt and `git diff --check` passed. Exact command
and execution evidence are in the preceding concurrent-device checkpoint.

This is a focused negative set, not a complete per-format/usage audit. Keep
checking every core-valid resource and command against HAL before widening the
projection; public surfaces remain gated by Present scheduling.

## Checkpoint — public storage and array-layout rejection

Code checkpoint: `c5580577e` (`dx11: verify public FL9_1 storage and array rejection`).

The strict public integration test now also places a vertex-visible storage
buffer layout and a fragment-visible `D2Array` sampled-texture layout under
validation error scopes. Both return `wgpu::Error::Validation` through the
selected physical adapter, complementing the existing 3D, 4x-MSAA,
`R32Float`, and multi-mip D24S8 rejection cases. The valid indexed textured
depth-tested draw, sparse group-7 resource mapping, dynamic transform update,
and concurrent independent-device queue/readback workloads still execute.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64, strict
hardware mode:

- `$env:WGPU_DX11_REQUIRE_HARDWARE = '1'; cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`. All six unsupported requests were rejected by public core validation. Readback remained 32 tinted-red/32 green, then 16 blue/32 tinted-red/16 green after the dynamic transform; concurrent devices read 64/64 red and 64/64 green pixels.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed. Git printed only its existing LF-to-CRLF working-copy notice.

The Visual Studio developer-shell banner included `vError: Unknown error`;
the x64 environment was usable and Cargo completed successfully. This is
validation-boundary evidence, not a complete audit of all legal bind-group
entries. Continue checking public core operations that can pass current limits
and gates. The adapter is still offscreen-only, and window Present reentrancy
remains unresolved.

## Checkpoint — public depth-order rendering

Code checkpoint: `6390e4efe` (`dx11: verify public FL9_1 depth ordering`).

The public FL9_1 integration test now draws fully overlapping near and far
textured surfaces in both draw orders, using the existing public pipeline,
uniform groups, sampled texture, target and D24S8 attachment. The far surface
uses the sampled green texel and the near surface the sampled red texel. Each
of the 64 readback pixels remained tinted red for both orders, so the result
depends on depth comparison and writes rather than draw order. This upgrades
the public depth evidence beyond attachment creation and non-overlapping
geometry.

Verification on 2026-11-05 used strict hardware mode on the NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual
FL9_1, `software=false`:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 1/1, including both depth orders, all six public validation rejections, dynamic uniform image update and concurrent two-device readback.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed with no warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed.

This is still an offscreen diagnostic, not a representative desktop
application. Public HWND presentation remains gated by the unresolved
cross-layer Present reentrancy and lock-order design. The prior strict HAL,
core and DX12 counts are documented above and were not rerun for this
public-test-only change.

## Checkpoint — public R8Unorm sampling

Code checkpoint: `b965940ff` (`dx11: verify public FL9_1 R8 sampling`).

A second public API integration test now asserts the adapter's selected-device
R8Unorm format report, creates a `TEXTURE_BINDING | COPY_DST` texture, uploads
two byte values with a padded source row, samples it with a non-filtering
sampler at sparse bindings 7 and 42, and renders into RGBA8. The adapter
reported `TextureUsages(COPY_DST | TEXTURE_BINDING)` and
`TextureFormatFeatureFlags(SINGLE_MIP_ONLY)`, correctly withholding linear
filtering. Readback verified all 64 pixels: the left 32 were
`[64, 0, 0, 255]` and the right 32 `[192, 0, 0, 255]`.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64, strict
hardware mode:

- `$env:WGPU_DX11_REQUIRE_HARDWARE = '1'; cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 2/2 with no skips on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`. The first case includes public core rejection checks, depth ordering in both draw orders, dynamic-uniform rendering and concurrent two-device readback; the second executes the R8 upload/sample/readback.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- `rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs` and `git diff --check` passed.

The positive R8 path verifies public format projection through use. Other
sampled-only formats and their public shader/readback paths remain to be
audited; BC1 remains unavailable through public wgpu because its aggregate
compression feature is correctly withheld. HWND surface presentation remains
blocked on the separate Present reentrancy design.

## Checkpoint — public mapped-at-creation vertex upload

Code checkpoint: `8be26c25f` (`dx11: verify public mapped vertex initialization`).

The indexed textured public scene now creates its vertex buffer with
`mapped_at_creation`, fills the mapped range, and unmaps before command
recording. Its strict GPU render/readback verifies that core's initialization
staging copy reaches D3D11 vertex input correctly. The queue-based texture,
uniform, index and dynamic-range uploads remain in the same test, so this
extends public transfer coverage without replacing those paths.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64:

- Strict command from the R8Unorm checkpoint passed 2/2 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`, with no skips. The indexed depth-tested image and both explicit depth-order images matched; R8 upload/sample matched 32 `[64, 0, 0, 255]` and 32 `[192, 0, 0, 255]` pixels.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std` passed without warnings.
- Targeted rustfmt and `git diff --check` passed.

Mapped-at-creation is now verified through public vertex use. Arbitrary public
buffer mapping and GPU-write/read synchronization still require broader audit.

## Checkpoint — public textured 3D offscreen example

Commit `4c2b6237944c153963f91bf8d5ea590506a64a83`
(`dx11: add public FL9_1 offscreen cube example`) adds a
standalone public-API example. It enumerates the opt-in D3D11 backend's
physical adapter, verifies the explicitly selected FL9_1 identity, creates a
restricted public device, uploads a two-color texture and transform,
initializes indexed cube geometry through mapped-at-creation buffers, and
renders a depth-tested 3D scene to an offscreen RGBA8 target. It waits for
queue completion, maps the readback and validates every pixel against the two
texture colors or blue clear color. An optional argument saves the verified
image as PPM.

The first execution exposed a fixture error: generated indices were truncated
to 40 bytes although 36 Uint16 indices require 72 bytes. Core rejected the
draw before GPU execution. The index generator was corrected to append all
six indices for each of six faces, then strict execution passed.

Verification on 2026-11-05 used Rust 1.93.1 and the Visual Studio 2022 x64
developer environment. Exact command:

```powershell
cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
```

The NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
reported requested/actual FL9_1 and `software=false`. Readback validated 112
red pixels, 277 green pixels and 635 blue background pixels, 1,024 total.
Example-target `cargo +1.93 clippy --offline -j16 -p wgpu --examples
--no-default-features --features dx11,wgsl,std`, targeted rustfmt and
`git diff --check` passed. The Visual Studio shell printed its existing
`vError: Unknown error` banner, but the x64 build and execution completed.

This advances the public offscreen example requirement. It does not resolve
the safe HWND `Present` reentrancy/scheduling design, so public surface
capabilities remain unavailable.

## Checkpoint — accurate public-backend diagnostic

Commit `435a31c4d54fe4c565c95de722d3f21cc1aee051` updates the core instance
warning shown when D3D11 is explicitly enabled. The previous message claimed
there was no usable wgpu backend, which became inaccurate when the restricted
public FL9_1 offscreen adapter was added. The new warning describes its
experimental offscreen-only status without implying surface support.

Verification on 2026-11-05:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1` passed 1/1 and enumerated one physical RTX 3090 Ti FL9_1 adapter.
- `cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl`, targeted rustfmt and `git diff --check` passed.
- The strict public integration suite passed 2/2 on the same physical RTX at requested/actual FL9_1. It verified the indexed depth-tested public scene, six validation rejections, group-7 sampling, dynamic-uniform update, concurrent devices, and R8 sampled upload. The standalone example separately validated 1,024 pixels (112 red, 277 green, 635 blue clear).

An initial core test invocation omitted the crate's `std` and `wgsl` features and failed to compile test-only imports; rerunning with the feature set above succeeded. The MSVC developer prompt continues to print `vError: Unknown error` despite providing a working x64 toolchain.

The current core review remains focused on any core-valid operation that can
reach unsupported HAL behavior. The public adapter still reports no surface
capabilities; the known synchronous HWND `Present` reentrancy design remains
the presentation blocker.

## Checkpoint — selected FL9_1 texture-load validation

Commit `a3a6a177c` (`dx11: gate FL9_1 texture loads in core`) adds
`DownlevelLimits::texture_load`. Other backends keep the default enabled; the
D3D11 selected-device projection disables it. Core records Naga `ImageLoad`
usage per entry point, follows validated helper calls iteratively, and rejects
only the selected vertex or fragment entry that uses a texture load. The
pipeline error is a validation error and occurs before HAL compilation. A safe
sibling entry in the same shader module remains usable. The production HAL
restriction is unchanged; the optional native texel-load experiment remains
disabled. The public strict test/example future poller now waits using a
thread-unparking waker instead of panicking if an operation is pending.

Verification on 2026-11-05 used pinned Rust 1.93.1 and Visual Studio 2022 x64:

- `cargo +1.93 test --offline -j16 -p wgpu-types --lib texture_load_is_a_downlevel_capability` passed 1/1; the full wgpu-types library suite passed 70/70, and the old-shape serde compatibility test passed 1/1.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampled_texture_count_uses_the_selected_entry_point` passed 1/1. It covers direct and helper-reachable loads, unrelated sibling entries, stage selection and missing entry names.
- With `WGPU_DX11_REQUIRE_HARDWARE=1`, the full controlled core fixture suite passed 81/81 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1. The targeted rerun passed 1/1 and logged the selected-entry textureLoad rejection before HAL, then completed the fixture's rendering/readback cases.
- The strict public wgpu test passed 2/2 on the physical RTX at requested/actual FL9_1 with `software=false`. It printed the core rejection/safe-sibling pass; the public depth-tested render, dynamic uniform, transitions, concurrent-device reads and R8 sample checks passed.
- The standalone public cube example passed on the same device and validated 112 red, 277 green and 635 blue-clear pixels (1,024 total).
- The private D3D11 HAL suite passed 337/337 under strict hardware mode; the targeted DX12 core regression passed 77/77. The HAL/GLES/Vulkan check passed.
- Clippy passed for wgpu-types (with serde), wgpu-core fixture, HAL with DX11/GLES/Vulkan, and wgpu tests/examples. Pinned rustfmt on changed Rust files and `git diff --check` passed.

The first public-test invocation omitted the Visual Studio developer environment
and failed at link time (`link.exe` not found); rerunning through
`VsDevCmd.bat -arch=x64 -host_arch=x64` passed. The developer prompt continues
to print `vError: Unknown error` while providing the working x64 toolchain.
Git also reports its existing LF-to-CRLF working-copy warnings on edited Rust
files. Public presentation, a complete core/HAL contract audit and historical
FL9_1 hardware remain outstanding. Next, investigate the D3D11 HWND Present
path and core queue/context locking; do not expose surface capabilities unless
the reentrancy and surface lifecycle are safely resolved.

## Checkpoint — public FL9_1 stencil behavior

Commit `64894f9c8` (`dx11: test public FL9_1 stencil rendering`) adds a strict
public wgpu test for the implemented symmetric stencil subset. It clears the
D24S8 stencil plane, writes reference 1 with `Replace`, colors passing `Equal`
fragments green, then verifies an `Equal` draw with reference 0 cannot replace
that result. All 64 readback pixels were `[0, 255, 0, 255]`. The test also
confirms that an asymmetric front/back stencil pipeline is rejected through a
public validation error before HAL. `CAPABILITIES.md`, `TESTING.md` and
`STATUS.md` record this coverage.

Verification on 2026-11-05, Rust 1.93.1, Visual Studio 2022 x64:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1` passed 3/3 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`. This includes the public indexed textured/depth-tested draw, dynamic uniform update, two-device queue submissions, R8 sampled upload, and the new stencil readback/rejection checks.
- A first compile caught that `VertexState::buffers` requires `Option<VertexBufferLayout>`; the test fixture was corrected and the focused strict stencil case passed 1/1 before the complete 3/3 rerun.
- `cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std`, rustfmt on `wgpu/tests/dx11_public.rs`, and `git diff --check` passed.

Commit `ee05538a3` (`docs: refresh experimental D3D11 backend overview`)
replaces the obsolete README that said adapter enumeration was empty and the
renderer was not available through wgpu. It now accurately describes the
opt-in public offscreen adapter, strict commands, and still-gated surfaces.
No behavior changed in that documentation commit.

The public offscreen API now has verified depth and stencil use, but no HWND
surface capabilities. The next investigation revisits DXGI Present scheduling
and reentrancy; do not attempt to resolve it by dropping the immediate-context
or core queue-order locks without a correct serialization design.

## Checkpoint — public FL9_1 blending and Present audit

Commit `28d3bbddd` (`dx11: verify public FL9_1 blending and validation`) adds a
strict public render test for a premultiplied red overlay over opaque blue.
Using identical color and alpha equations, all 64 pixels read back as
`[128, 0, 128, 255]`. The test separately confirms core rejects
`SrcAlphaSaturated` and differing full-channel RGB/alpha equations before HAL.
Capability, status and test documents record the exact verified subset.

On 2026-11-05, the complete strict public suite passed 4/4 on the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with requested/actual FL9_1 and
`software=false`. It covers the indexed textured/depth-tested draw, dynamic
uniform update, public selected-entry texture-load validation, multi-device
submission/readback, R8 sampling, symmetric stencil readback, and blending.
The blend case's strict standalone filter passed 1/1. Clippy for wgpu tests,
pinned rustfmt and `git diff --check` passed.

Two independent read-only audits and the local source trace reconfirmed the
public HWND Present blocker. Core holds `device.command_indices.write()` across
HAL `present` (`wgpu-core/src/present.rs`); core submissions hold the same
serialization guard while invoking HAL submit. D3D11 surface presentation
retains a busy operation, and `CoreWindowFrame::present_for_hal` holds the
non-reentrant immediate-context mutex over synchronous `IDXGISwapChain::Present`
(`wgpu-hal/src/dx11/surface.rs`, `swapchain.rs`, and `device.rs`). A reentrant
window callback that submits can block first in core and then, even if that
lock is changed, on the HAL context. Dropping those guards without replacement
would allow unordered submissions or concurrent immediate-context use. A
reentrant mutex alone does not prove nested context calls safe while Present is
on the stack; surface unconfigure can also wait on the busy operation.

Microsoft [DXGI best practices](https://learn.microsoft.com/en-us/windows/win32/direct3darticles/dxgi-best-practices)
states that DXGI calls including Present may synchronously send window messages
and recommends not blocking the message pump. [D3D11 multithreading guidance](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-render-multi-thread-intro)
states that only one thread should use the immediate context and recommends
using that same thread for DXGI, especially Present. The likely safe direction
is a serialized asynchronous per-device executor: reentrant submissions must
enqueue and return rather than wait on a worker blocked in Present, while
command order, resources, mapping, surface lifecycle, and completion semantics
remain correct. This is a cross-layer redesign, not a lock-release patch. Keep
public surface capabilities unavailable while continuing independent FL9_1
work.

## Checkpoint — public FL9_1 scissor bounds

Commit `ee430b9b2` (`dx11: verify public FL9_1 scissor bounds`) adds a public
offscreen render test using an 8×8 RGBA8 target and a fullscreen triangle. A
scissor at `(2, 1)` with size `4×6` produced exactly 24 green pixels while the
remaining 40 stayed blue. The same selected-device test submits a viewport
extending one pixel past the left target edge and confirms core validation
rejects it before HAL. The capability, testing and status documents now state
this public evidence.

On 2026-11-05, with Rust 1.93.1, Visual Studio 2022 x64 and
`WGPU_DX11_REQUIRE_HARDWARE=1`, the full public suite passed 5/5 on the NVIDIA
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
`software=false`. The scissor test passed 1/1 in its focused run. Across the
five strict tests, rendering/readback covers the indexed textured depth scene,
dynamic uniform update, multi-device submission, R8 sampling, symmetric
stencil, premultiplied blending and scissor bounds; negative checks reject
texture loads, asymmetric stencil, unsupported blend states and out-of-target
viewports before HAL. `cargo +1.93 clippy --offline -j16 -p wgpu --tests
--no-default-features --features dx11,wgsl,std`, rustfmt and
`git diff --check` passed.

The safe public HWND presentation redesign remains the largest outstanding
cross-layer task. Independent audit evidence for the synchronous message-pump
and lock interaction is in the previous checkpoint above. Continue with other
bounded public FL9_1 functionality while surface capabilities remain disabled.

## Checkpoint — public sparse multi-texture sampling — `6280e36df`

Added a strict public wgpu integration test that declares two independently
uploaded fragment textures at logical bindings 7 and 42 and a shared sampler
at binding 100. Bind-group layout entries, WGSL declarations and runtime group
entries are deliberately ordered differently, exercising sparse logical
identities through core, HAL allocation, shader compilation and D3D11 runtime
binding. A shader adds both sampled colors. It also attempts to create an
incomplete group without binding 42 and verifies that public core returns a
validation error.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 environment on
2026-11-05. The focused strict test passed 1/1. The complete strict command

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

passed 6/6. Every test selected the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), with requested and actual FL9_1 and
`software=false`. The new case verified all 64 pixels as
`[104, 128, 24, 255]`. `cargo +1.93 clippy --offline -j16 -p wgpu --tests
--no-default-features --features dx11,wgsl,std`, targeted rustfmt and
`git diff --check` passed. Visual Studio printed its existing
`vError: Unknown error` banner while providing the working x64 toolchain.

The test does not widen the selected capability projection. Public HWND
surface capabilities remain unavailable because synchronous `Present` can
reenter while core queue-order and HAL immediate-context locks are held. The
contract audit and historical FL9_1 hardware verification remain outstanding.

## Checkpoint — public eight-texture boundary — `98c6879bd`

Added a strict public wgpu test for the selected projection's maximum of eight
sampled textures per fragment stage. It verifies the reported limit, binds and
samples eight independently uploaded textures at sparse logical binding
numbers through one sampler, and checks that every texture contributes to all
64 output pixels. It also confirms a nine-texture fragment layout fails with a
public core validation error before HAL.

The focused strict test passed 1/1. The complete `dx11_public` suite passed
7/7 on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with
requested and actual FL9_1 and `software=false`. The new test verified
`[180, 72, 36, 255]` on all 64 pixels; core rejected texture nine. Rust 1.93.1
wgpu test Clippy, targeted rustfmt and `git diff --check` passed. The Visual
Studio x64 prompt emitted its existing `vError: Unknown error` banner but
provided a working linker/toolchain. Exact command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

This tests the existing public count projection and does not enable any new
resource capability. Historical FL9_1 hardware, the full core/HAL contract
and public HWND presentation remain outstanding.

## Checkpoint — public texture-transfer roundtrip — `118918d40`

Added a public strict GPU test that starts with an 8×8 RGBA8 texture, updates
its 2×2 region from a mapped copy buffer at nonzero byte offset 16 and 256-byte
row pitch, copies that region from source origin `(2, 3)` to destination
origin `(3, 2)` between 8×8 and 6×6 textures, then reads the complete 6×6
destination using a 256-byte readback pitch. The source rows contain four
distinct colors; the destination starts green. All 36 pixels matched the
expected 2×2 patch while the other 32 pixels retained green. The mapped upload
buffer and textures remain alive through ordered submission and map completion.

The focused strict test passed 1/1. The complete public integration suite
passed 8/8 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with
requested and actual FL9_1 and `software=false`. The initial compile caught an
incorrect direct index on `BufferViewMut`; the test now writes two mapped
subranges through `get_mapped_range_mut` and no compiler warning remains.
Rust 1.93.1 wgpu test Clippy, targeted rustfmt and `git diff --check` passed.
Visual Studio's existing prompt banner was emitted, but its x64 compiler and
linker worked. Command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

This verifies the projected RGBA8 copy usages through public core and the HAL;
it does not establish all texture combinations or historical FL9_1 driver
behavior. Public HWND presentation and the exhaustive capability audit remain
outstanding.

## Checkpoint — public sampled RGBA8 mip chain — `f191d4f97`

Added a public strict test that creates an 8×8, four-level RGBA8 resource,
uploads a different solid color to every mip with padded rows, creates a view
of the full chain, and renders it with implicit derivatives. An oversized
fullscreen triangle produces a known 2:1 texel minification ratio and selects
level 1 through a nearest mip filter; all 64 output pixels matched green. The
test also verifies the public core rejects a three-mip R8 texture under the
selected device's `SINGLE_MIP_ONLY` format projection.

The focused strict test passed 1/1. The complete public suite passed 9/9 on the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), with requested and actual
FL9_1 and `software=false`. `Rgba8Unorm` reported sampled, upload, copy,
render-target and filtering support with no `SINGLE_MIP_ONLY` flag. The new
readback selected mip 1 in every pixel; R8 multi-mip creation produced a public
validation error. Rust 1.93.1 wgpu test Clippy, targeted rustfmt and
`git diff --check` passed. Early test runs selected mip 0 and then mip 2 while
calibrating the UV scale; the final 2:1 ratio selected mip 1 deterministically.
Visual Studio's existing x64 prompt banner did not affect the build or GPU
execution.

Exact strict command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

This proves the reported RGBA8 mip path on the tested RTX at forced FL9_1; it
does not generalize to NPOT textures, other formats or legacy drivers. Public
HWND presentation and exhaustive contract review remain outstanding.

## Checkpoint — public SNORM sampled formats — `f62e819db`

Added a public strict integration case that checks the selected adapter's
reports for `Rg8Snorm` and `Rgba8Snorm`, uploads each format, samples both
through one non-filtering sampler and maps RGBA8 output back to the CPU. Each
input uses signed values with nontrivial positive and negative channels; the
shader maps them into visible unorm output. Both format reports contain
`TEXTURE_BINDING | COPY_DST`, omit `FILTERABLE` and set `SINGLE_MIP_ONLY`.

The focused strict test passed 1/1 and the full public suite passed 10/10 on
the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual
FL9_1 and `software=false`. All 64 output pixels matched
`[192, 63, 160, 255]` within ±1. Rust 1.93.1 wgpu test Clippy, targeted
rustfmt and `git diff --check` passed. Visual Studio printed its recurring
developer-prompt banner while providing a working x64 build environment.

An independent read-only audit of public format, view, mip, upload, copy and
readback paths found no new undocumented mismatch. It reconfirmed the known
BC1 partial-edge upload restriction (block-aligned regions only), which remains
documented in `CAPABILITIES.md`; core allows edge-ending partial BC blocks that
the private HAL currently rejects. The strict command was:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The public offscreen contract and tested format subset are broader, but public
HWND presentation, the full reachable-operation audit and historical FL9_1
drivers remain outstanding.

## Checkpoint — public instanced vertex streams — `aeb41c79e`

Added a public strict GPU case using one per-vertex triangle stream plus a
second `VertexStepMode::Instance` stream containing per-instance offsets and
colors. `draw(0..3, 1..3)` selects records two and three; readback contains 20
green, 20 yellow and 472 blue clear pixels, with no pixels from the skipped
first red instance. This verifies both the emulated instance-rate binding and
nonzero `first_instance` behavior through public core/HAL submission. The same
test constructs a vertex entry that declares `instance_index` and confirms
core rejects the pipeline before HAL.

The focused strict test passed 1/1. The complete strict public wgpu suite
passed 11/11 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested and actual FL9_1, `software=false`. Rust 1.93.1 test Clippy,
targeted rustfmt and `git diff --check` passed. The Visual Studio x64 prompt
printed its existing `vError: Unknown error` banner, but build, link and GPU
execution succeeded. Exact command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

Public adapter presentation, the remaining core/HAL operation audit and
historical FL9_1 devices remain outstanding.

## Checkpoint — selected-entry sample-bias validation

Added three `DownlevelLimits` properties for the FL9_1 sample-bias subset:
maximum absolute literal bias 3, dynamic bias disabled, and texel offsets
disabled. Core records bias operations by entry point, follows helper calls,
and rejects unsupported forms as pipeline validation errors before HAL shader
compilation. This closes the case where a core-valid but unsupported bias
previously surfaced as a HAL linkage/internal error. Strict public readback
verified implicit sampling and literal biases -3, +1 and +3 against four mip
colors across 256 pixels. The public test also expects validation errors for
dynamic, helper-dynamic, out-of-range and offset forms. CPU-only tests cover
fractional literals, constant helper sampling, selected-entry isolation,
conservative dynamic rejection and unrestricted defaults.

Verification on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`):
requested/actual FL9_1, hardware adapter, no software fallback. Strict public
suite passed 11/11; core fixture suite passed 82/82, including real D3D11
readback cases; direct adapter test passed 1/1 and enumerated one FL9_1 device.
The wgpu-types suite passed 71/71; its serde compatibility case passed 1/1.
HAL `dx11,gles`, `wgpu-info`, and core `dx12` checks passed. Core and public
wgpu Clippy, wgpu-types serde Clippy, rustfmt checks and `git diff --check`
passed. The DX12 check emitted 15 existing warnings caused by building without
WGSL, while the no-serde wgpu-types test emitted existing serde-import
warnings. `wgpu-info` initially failed because its exhaustive downlevel
destructure had omitted the already-existing `texture_load` field; it now
prints that property alongside the new bias limits.

Exact strict commands:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -q -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1'
```

The Visual Studio prompt continues to print its pre-existing `vError: Unknown
error` banner despite correctly setting the x64 compiler and linker environment.
Public HWND presentation, a complete core/HAL operation audit and testing on
historical FL9_1 devices remain outstanding.

Code checkpoint commit: `f6e4125fe` (`core: validate FL9_1 texture sample bias limits`).

## Checkpoint — FL9_1 fragment derivative validation — `987f39cd3`

Added `DownlevelLimits::fragment_derivatives`, defaulting to true for backward
compatibility. The D3D11 hardware model marks the feature unavailable at FL9_1
and FL9_2 and available from FL9_3, while the current backend implementation
remains false. Core records derivative and explicit-gradient use per selected
entry point, follows helper calls, and rejects unsupported fragment pipelines
before HAL compilation. Naga expression structure is inspected directly; the
implementation does not parse WGSL or generated HLSL. The FXC test records the
actual `ps_4_0_level_9_1` diagnostics: `ddx` cannot map to the pixel
instruction set, and `SampleGrad` is unsupported by `ps_2_0`.

Verification used Rust 1.93.1 and the x64 Visual Studio 2022 developer
environment. With `WGPU_DX11_REQUIRE_HARDWARE=1`, the full HAL suite passed
338/338 test functions. GPU cases selected the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), software=false, and explicitly
requested/selected FL9_1. The selected-entry derivative/gradient negative
cases ran in the strict public mip test; the public suite passed 11/11 and
the full core D3D11 fixture suite passed 83/83. The fixture enumerated one
physical FL9_1 adapter. Some HAL test subcases intentionally report `SKIP`
after expected rejection of unsupported runtime integer indexing; these are
not missing-hardware skips, and no strict test silently substituted software.
The wgpu-types serde suite passed 73/73 plus 14 passing and 21 ignored doc
tests. The core DX12 unit suite passed 79/79. HAL `dx11,gles`, `wgpu-info`,
and core DX12 checks passed. Public wgpu, wgpu-types, and HAL Clippy passed;
core Clippy passed with three pre-existing warnings in the fixture test file,
and HAL Clippy passed with nine pre-existing warnings in unrelated D3D11
tests. Targeted formatting for changed crates and `git diff --check` passed.
Workspace formatting still reports pre-existing differences in committed
`naga/src/back/hlsl/writer.rs`. MSVC commands printed the existing Visual
Studio `vError: Unknown error` banner while compiling and linking successfully.

Exact test commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -q -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The repository-wide core/HAL audit remains unfinished; public presentation is
still gated by the existing synchronous DXGI `Present` reentrancy and lock
ordering problem. Historical FL9_1 hardware has not been tested. Continue
with shader/core contract gaps, then return to the surface scheduling design.

## Checkpoint — aggregate FL9_1 uniform-vector budgets — `76dd6656deec94d3832db6961a9516cdba08070e`

Added stage-specific `DownlevelLimits` for the FL9_1 vertex and fragment
constant budgets (255 and 32 float4 vectors). The hardware capability model
derives these limits only for FL9_x, while the active backend/compiler limits
remain a separate intersection. Core's validated Naga interface records the
complete layout size of uniform globals used by each selected entry point,
including helper calls, and rejects an over-budget vertex or fragment
pipeline before invoking HAL. Unused entry-point resources are not counted.
This size-based accounting is deliberately conservative when only part of a
declared structure is read. `wgpu-info` prints the values; non-D3D11 adapters
retain unlimited defaults. The downlevel limits remain non-WebGPU-compliant.

Microsoft's 10Level9 context reference specifies maximum totals of 255 vertex
and 32 pixel constants. FXC accepts 32 directly referenced pixel vectors in a
single buffer and across two 16-vector buffers; it rejects 33 and 17+16 at
register 32. A strict public FL9_1 pipeline reads all 32 vectors from two
sparse logical bindings, and GPU readback verifies 64/64 `[191, 0, 0, 255]`
pixels. The same public test asserts that a 33-vector fragment pipeline is
rejected by core validation before HAL. An initial shader test added an extra
`vec4` literal, which correctly consumed a 33rd FXC register; the final test
derives its alpha output from the uploaded uniforms instead.

Verification used `rustc 1.93.1 (01f6ddf75 2026-02-11)`, Visual Studio 2022 x64
tools and Cargo `-j16`. Strict GPU tests selected the physical NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested
FL9_1, observed actual FL9_1, and reported `software=false`:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 12/12 passed, no hardware skips. The new aggregate test rendered 64/64 expected pixels and rejected 33 vectors.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --test-threads=1`: 339/339 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1`: 84/84 passed, including physical adapter enumeration and core hardware cases.
- `cargo +1.93 test --offline -j16 -p wgpu-types --features serde`: 74/74 unit tests and 14 doc tests passed; 21 doc tests were ignored.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1`: 80/80 passed.
- HAL with `dx11,gles`, `wgpu-info`, and `wgpu-test` checks passed.
- Clippy passed for wgpu-types and wgpu. Core emitted three existing fixture-test warnings; HAL emitted nine existing test warnings. No warnings originated in the new implementation.
- `cargo +1.93 fmt --check` for wgpu-core, wgpu-hal, wgpu-types, wgpu-info, wgpu and wgpu-test, plus `git diff --check`, passed.

The first `wgpu-test` check was accidentally run outside the VS developer
prompt and could not find `cl.exe`; rerunning inside `VsDevCmd.bat` passed.
The known Visual Studio `vError: Unknown error` banner remains cosmetic. Public
HWND presentation, full public capability auditing, historical FL9_1 hardware
and remaining shader/resource gaps are still outstanding. Next continue the
reachable-operation audit and investigate the public surface scheduler.

## Checkpoint — explicit texture sample-level validation — `1dd629564`

The FL9_1 projection now includes `DownlevelLimits::texture_sample_level`.
Core records `ImageSample` operations with `SampleLevel::Exact` for each
selected entry point and its reachable helper functions, and rejects only the
selected pipeline entry before HAL. FXC diagnostics reject literal and dynamic
`textureSampleLevel` under both `ps_4_0_level_9_1` and `ps_4_0_level_9_3`
(`X4532: texlod not supported on this target`). The diagnostic profile exists
only to test compiler behavior; this adds no higher-level execution path.
Implicit sampling and safe sibling entries remain available. The capability
model separates the hardware threshold from the still-unimplemented backend
path, and serde defaults preserve older serialized `DownlevelLimits` values.

After the final core validation-order adjustment, the strict public wgpu suite
passed 12/12 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested/actual FL9_1, `software=false`; no hardware tests skipped. The public
test rejected direct and helper-forwarded explicit LOD through core while
creating the implicit-sampling sibling pipeline. Earlier in this same
implementation run, strict private HAL passed 340/340, the D3D11 core fixture
passed 85/85, `wgpu-types --features serde` passed 75/75 unit and 14/14 doc
tests (21 ignored), and the targeted DX12 core suite passed 81/81. Affected
Clippy and targeted formatting checks passed; only the repository's existing
HAL/core test warnings were reported. `git diff --check` passed.

Exact final public command:

```powershell
cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
```

## Checkpoint — selected-entry FL9_1 texture-query validation — `38b5a0077`

Added `DownlevelLimits::texture_query`, with a backward-compatible serde
default. The hardware model marks this feature from FL10_0, while the current
D3D11 implementation reports it unavailable at every level. Core records
`ImageQuery` use per entry point, including helper calls, and rejects only the
selected vertex or fragment pipeline entry before HAL. CPU validation tests
cover direct dimensions queries, helper forwarding, `textureNumLevels`, safe
sibling entries, vertex-stage use and missing entries. The public strict test
rejects direct, helper and level-count queries while compiling a safe sibling.

FXC confirms that the current Naga HLSL query lowering is not usable under
`ps_4_0_level_9_1` or `_9_3`: dimensions and level-count shaders fail with
X3548, and a direct float-output `GetDimensions` control fails with X4532.
Equivalent controls compile under `ps_5_0`. This is a selected compiler-path
restriction, not a claim that every FL9_1 driver behaves identically.

Verification used Rust 1.93.1, Visual Studio 2022 x64 tools and Cargo `-j16`.
The strict public suite passed 12/12, the full private HAL suite passed
341/341, and the core D3D11 fixture suite passed 86/86. GPU tests selected
the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), requested and selected FL9_1, `software=false`; no hardware
tests skipped. The targeted updated FXC diagnostic test passed after adding
the float-output control. The `wgpu-types --features serde` suite passed
76/76 unit tests and 14 doc tests (21 ignored); the targeted DX12 core suite
passed 82/82. HAL `dx11,gles`, `wgpu-info` and `wgpu-test` checks passed.
Affected Clippy and targeted Rust formatting checks passed; core/HAL emitted
only the already-recorded fixture/test warnings. `git diff --check` passed.

The relevant strict runs used these Cargo commands from the x64 Visual Studio
developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -q -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --features serde
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The D3D11 implementation and regression tests are committed in
`38b5a0077488f7ebe7131516cc2d3de5a0d9c0c2`. Exact commands are in
`TESTING.md` and this checkpoint. Continue the exact core/HAL contract audit.
Public HWND presentation remains gated by the unresolved synchronous
`Present` reentrancy/serialization design, and historical FL9_1 hardware is
untested.

## Checkpoint — indexed lists without vertex-rate inputs — `d96650d72`

Indexed LineList and TriangleList draws no longer fail solely because the
pipeline has only instance-rate or no vertex-rate inputs. Since FL9_1 shader
validation rejects `vertex_index`, index values and `base_vertex` cannot affect
vertex data in these cases. HAL still validates pipeline/resource state,
instance-stream bounds and the requested index-buffer range, then records a
nonindexed list draw with the original primitive count and instance bindings.
Layouts with vertex-rate inputs retain the existing indexed emulation path.
Indexed strips still require vertex-rate inputs so restart runs and strip
connectivity remain explicit. PointList's separately tested instance-only
path is unchanged.

The new private and public tests cover LineList and TriangleList with Uint16
and Uint32 index buffers, nonzero first-index, negative base-vertex, two
instances, instance-rate-only input and an empty vertex layout. Full readbacks
match the equivalent nonindexed draws; the degenerate triangle cases also
assert that the cleared image remains unchanged.

Strict execution used Rust 1.93.1 and Visual Studio 2022 x64 tools. The physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) was selected
as hardware (`software=false`) and explicitly requested and observed at FL9_1.
There were no missing-hardware skips. Results after the code commit:

- Private HAL suite: 342 passed, 0 failed, 0 ignored. The new test exercised
  eight indexed/direct comparisons across topology, layout and index format.
- Public `wgpu` integration suite: 13 passed, 0 failed, 0 ignored. The new
  public test exercised the same eight comparisons through core and HAL.
- Strict D3D11 core fixture suite: 86 passed, 0 failed, 0 ignored.
- DX12 core regression suite: 82 passed, 0 failed, 0 ignored.
- HAL Clippy completed with nine existing test/code warnings; no warning came
  from this patch. `wgpu` test Clippy completed without warnings.
- Rustfmt checks on the three changed Rust files and `git diff --check` passed.
  Workspace `cargo fmt --all -- --check` still reports formatting differences
  in the untouched, committed `naga/src/back/hlsl/writer.rs`; that unrelated
  file was not reformatted.

Exact strict commands (run from the Visual Studio x64 developer environment):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The code commit is `d96650d72`. Continue auditing operations that core
accepts but the private HAL cannot execute, then return to the public surface
scheduler design. Public adapter access remains opt-in/offscreen-only;
historical FL9_1 drivers remain untested.

## Checkpoint — selected fragment-depth output validation — `ed642d768`

Core now models the `ps_4_0_level_9_1` output restriction found during the
shader audit: a fragment entry point that writes `frag_depth` without
`@location(0)` fails pipeline validation before FXC. Naga interface metadata
checks the selected entry point, so a color-and-depth sibling remains valid
and an unrelated entry point does not affect selection. The D3D11 capability
projection reports this restriction; other backends retain the default
permissive value. The private compiler test records FXC X4530 for direct-only
depth output and accepts structured color plus depth. The public integration
test verifies the precise core rejection and compiles the supported output.

Strict execution used Rust 1.93.1, Visual Studio 2022 x64 tools, and the
physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
at requested and actual FL9_1 (`software=false`). Results:

- Private HAL suite: 344 passed, 0 failed, 0 ignored.
- Public `wgpu` D3D11 suite: 14 passed, 0 failed, 0 ignored.
- Strict D3D11 core fixture suite: 88 passed, 0 failed, 0 ignored.
- Core DX12 regression suite: 84 passed, 0 failed, 0 ignored.
- `wgpu-types` unit suite: 76 passed; serde-enabled suite: 78 passed.
- D3D11 HAL/GLES compilation, `wgpu-info`, and `wgpu-test --tests` checks
  passed. The latter two checks also caught and corrected exhaustive
  `DownlevelLimits` consumers that needed the newly added fields.
- Clippy passed for HAL, core, wgpu, types, info and test crates. HAL emitted
  nine pre-existing warnings in unrelated test/swapchain/texel-load code.
- Changed-file rustfmt and `git diff --check` passed. No hardware tests skipped.

The code checkpoint is `ed642d768`. Continue the selected-entry shader and
core/HAL contract audit, then revisit the public HWND presentation scheduler.
The adapter remains opt-in and offscreen-only; historical FL9_1 drivers and
ARM32 UWP hardware remain unverified.

## Checkpoint — fragment sample-mask output gate — `26321bf74`

The next selected-entry shader audit found that WGSL fragment
`@builtin(sample_mask)` output lowers to HLSL `SV_Coverage`, which the
`ps_4_0_level_9_1` FXC profile rejects with X4583. Core previously accepted
this operation and could fail only later in the private shader path. Added
`DownlevelLimits::fragment_sample_mask_output`, defaulting to available for
other backends and false in the D3D11 FL9_1 projection. Core now checks only
the selected fragment entry point, rejects before HAL, and permits a safe
color-only sibling. No shader rewrite is attempted; mapping its low coverage
bit to discard remains a possible single-sample emulation investigation.

Strict validation used Rust 1.93.1 and Visual Studio 2022 x64 tools. Hardware
was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
selected as hardware (`software=false`) with requested and actual FL9_1.
The private profile test isolated the Naga-to-FXC compiler path and observed:
`error X4583: semantic 'SV_Coverage' unsupported on ps_4_0_level_9_1`. The
public test confirmed the specific core validation error and acceptance of a
color-only sibling; it did not submit a draw for the negative pipeline case.

Results:

- Strict private HAL suite: 345 passed, 0 failed, 0 ignored; no hardware skips.
- Strict public D3D11 suite: 15 passed, 0 failed, 0 ignored; all selected the
  physical RTX at FL9_1.
- Strict D3D11 core fixture suite: 89 passed, 0 failed, 0 ignored.
- Core DX12 regression suite: 85 passed, 0 failed, 0 ignored.
- `wgpu-types`: 77 passed without serde and 79 passed with serde. The
  non-serde build reports 14 pre-existing unused serde-import warnings.
- HAL with D3D11/GLES, `wgpu-info`, and `wgpu-test --tests` checks passed.
- Clippy passed for HAL, core, wgpu, types, info and test crates. HAL reports
  nine pre-existing warnings in unrelated test/swapchain/texel-load code.
- Changed Rust files pass rustfmt; `git diff --check` passes.

During implementation, the first core and type test builds found missing values
in exhaustive FL9_1 fixture/test literals; these were fixed before the full
runs. The initial HAL-level compile attempt hit the existing conservative
runtime-integer guard first, so the compiler diagnostic test calls the same
production Naga-to-FXC function directly to isolate the profile restriction.
An optional check invocation using `--features noop` was invalid because
`noop` is not a `wgpu-hal` Cargo feature; the `noop` module is unconditionally
compiled in the successful HAL checks and test suite.

The code commit is `26321bf74`. Continue auditing other core-accepted shader
operations and HAL paths. Public HWND presentation still needs a cross-layer
scheduling design that avoids synchronous DXGI `Present` reentrancy; historical
FL9_1 drivers and ARM32 UWP hardware remain unverified.

## Checkpoint — FL9_1 sample-index validation — `7f48be505`

The next shader-contract audit confirmed that `@builtin(sample_index)` does
not require another FL9_1-specific pipeline limit: core passes the selected
adapter's `DownlevelFlags::MULTISAMPLED_SHADING` into Naga validation, and the
FL9_1 projection omits that flag. Added a strict public regression which opens
the physical device at forced FL9_1, verifies that a fragment entry using
`sample_index` fails through the expected Naga capability diagnostic, and
verifies a single-sample shader module remains accepted. This is a validation
test, not a GPU draw. The code-only checkpoint is `7f48be505`.

Verification used Rust 1.93.1 and the VS 2022 x64 environment:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_sample_index_without_multisampled_shading -- --nocapture --test-threads=1`: passed 1/1 on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, `software=false`.
- `rustfmt +1.93 --check wgpu/tests/dx11_public.rs` and `git diff --check`: passed.

The full HAL/public/core suites were not rerun for this isolated regression.
Continue the selected shader/core/HAL audit and the HWND presentation
scheduling investigation. Historical FL9_1 hardware and ARM32 UWP remain
unverified.

## Checkpoint — required FL9_1 sRGB color formats — `a9f46afe1`

The selected-format model now treats RGBA8_SRGB and BGRA8_SRGB as FL9_1
required formats while retaining independent positive `CheckFormatSupport`
gates. The backend routes their typed DXGI formats through the existing
single-mip texture upload, sampled SRV, color-target RTV, texture readback and
copy paths. Public format reporting exposes only usages and filter/blend flags
reported for the selected format and already implemented by the backend. A
stale HAL negative test was changed to keep testing rejection with Rgba16Float;
the two newly supported sRGB formats are no longer expected to fail creation.

Strict hardware validation used Rust 1.93.1 and Visual Studio 2022 x64. The
selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`)
was a physical adapter (`software=false`) and explicitly requested and
returned FL9_1. `CheckFormatSupport` returned `0x112ef3e0` for each sRGB format.
Public readback decoded uploaded sRGB texels to 128/128 expected linear pixels
`[55, 13, 134, 255]`; rendering linear `[0.5, 0.25, 1.0, 1.0]` into RGBA8_SRGB
and BGRA8_SRGB encoded 128/128 expected pixels, including the correct channel
order. No hardware test skipped. The dedicated public test checks single-mip
paths; no sRGB mip-chain execution case has run, and this modern device does
not establish historical-driver behavior.

Verification:

- `cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11`: passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1`: 346 passed, 0 failed, 0 ignored. The first run exposed the stale sRGB rejection assertion (345 passed, 1 failed); after correcting that test, the full run passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 17 passed, 0 failed, 0 ignored, all physical FL9_1 adapter cases.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1`: 89 passed, 0 failed, 0 ignored.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1`: 1 passed; core exposed the one physical FL9_1 adapter in the explicitly enabled DX11 configuration.
- `cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1`: 85 passed, 0 failed, 0 ignored.
- HAL Clippy passed with nine pre-existing warnings in unrelated test, swapchain and texel-load proof code. Public `wgpu` test Clippy passed without warnings. Changed-file rustfmt and `git diff --check` passed.

One early `rustfmt` invocation omitted the crate edition and could not parse
the repository's existing Rust 2021 C-string literal; the changed files pass
when checked with `--edition 2021`. One initial core test command omitted the
`wgsl` feature and failed to compile its WGSL-based unit tests; the corrected
command above passed. Neither issue required a product-code change. Historical
FL9_1 hardware, ARM32 UWP and the existing public HWND presentation scheduler
remain unverified or unresolved; continue the independent format and
core/HAL-contract audit.

## Checkpoint — public FL9_1 sRGB mip execution — `f2c813671`

The selected RTX projection reports mip support for both sRGB formats, so a
second public GPU test now creates four-level POT `Rgba8UnormSrgb` and
`Bgra8UnormSrgb` textures. It uploads different values to each level, samples
with implicit LOD into a linear RGBA8 target, and verifies that mip 1 is
selected and decoded to `[55, 13, 134, 255]` in all 128 checked pixels. This
closes the previously documented sRGB mip execution gap for the tested
implicit-LOD path; it does not test every LOD clamp or mip filter combination.

The first test attempt sampled mip 0 because its UV gradient produced a base
LOD. The expected mip-1 result was therefore wrong; the test gradient was
adjusted to match the calibrated public POT-mip test pattern. The corrected
strict test passed on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested and actual FL9_1,
`software=false`. The full strict public suite then passed 18/18 with no
skips. `cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public
--no-default-features --features dx11,wgsl,std`, changed-file rustfmt and
`git diff --check` passed. The suite does not establish historical-device
behavior; continue the capability and core/HAL audit.

## Checkpoint — public FL9_1 D16 depth-only attachment — `d0efe2753`

The private and selected-device capability models now expose D16_UNORM as a
single-mip depth-only attachment when the FL9_1 required-format guarantee, the
backend path and the selected device's `CheckFormatSupport` query all agree.
Texture initialization clears depth without requesting a stencil clear. HAL
views retain their actual depth format through render-pass and pipeline setup;
stencil operations on D16 and unsupported multi-mip requests are rejected.
Ordinary D24S8 behavior and normal D3D11 enumeration remain unchanged.

The strict public wgpu test draws near and far full-screen triangles in both
orders through a D16 attachment and validates 128/128 green pixels. It also
verifies the selected format projection and core rejection of multi-mip D16
and active stencil state. The NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) reported `CheckFormatSupport(D16_UNORM)=0x210020`; the
device was physical (`software=false`) and explicitly requested and returned
FL9_1. This validates the modern forced-level path, not historical drivers.

Verification (Rust 1.93.1, Visual Studio MSVC environment):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 -- --nocapture --test-threads=1`: 347 passed, 0 failed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 19 passed, 0 failed, 0 skipped.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 dx11::capabilities::tests -- --nocapture --test-threads=1`: 23 passed, including the selected-device D16 query/projection.
- D3D11 core fixture: 89 passed; core physical-adapter enumeration isolation: 1 passed; DX12 core regression: 85 passed.
- HAL and public-test Clippy passed; HAL emitted nine pre-existing warnings in unrelated code. Changed-file rustfmt and `git diff --check` passed.

The initial public test expected only `RENDER_ATTACHMENT`, but the HAL
correctly includes `TRANSIENT_ATTACHMENT`; the assertion was narrowed to
require render attachment and forbid sampled/copy usages. No implementation
failure remained. Continue depth/load/store and general core/HAL contract
coverage; safe public HWND presentation is still blocked on cross-layer DXGI
Present scheduling, and historical FL9_1 plus ARM32 UWP remain unverified.

## Checkpoint — D16 view and pass transitions — `0cba1afbb`

Extended the public strict D16 test to use an explicit `DepthOnly` view and
confirm that core rejects `StencilOnly`. Added two successive render-pass
cases: a second pass using LOAD retains the first pass's near depth and rejects
a farther fragment; a second pass using CLEAR resets the depth and accepts the
farther fragment. GPU readback matched all 128 transition pixels (64 per
case). The original draw-order cases still match all 128 pixels.

The first full-suite attempt failed because the view edit was accidentally
applied to an earlier D24 test. That change was restored, and the view checks
were scoped to the D16 test. The strict full public suite then passed 19/19 on
the physical RTX 3090 Ti, vendor/device `0x10de:0x2203`, driver
`32.0.16.2101`, requested/actual FL9_1, `software=false`, with zero skips.
`cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public
--no-default-features --features dx11,wgsl,std`, changed-file rustfmt and
`git diff --check` passed. The test does not exercise D16 discard semantics.
Next continue validated depth/render-pass combinations and the wider
core/HAL audit.

## Checkpoint — D16 discard followed by LOAD — `e3807a1b3`

Extended the public D16 render-pass test with a depth attachment stored using
Discard, followed by a second pass that loads depth and draws a nearer
primitive. The first pass writes depth 0.75 and red color; if that discarded
depth remained visible, the later 0.25 draw would replace the red output. The
strict readback remained red for all 64 pixels, showing that core's required
initialization after discard prevented stale depth from affecting subsequent
rendering. Together with the prior D16 cases, the test checked 128/128
draw-order pixels, 128/128 LOAD/CLEAR transition pixels and 64/64 discard-load
pixels.

The full strict public suite passed 19/19 on the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested and
returned FL9_1, `software=false`, with no skips. Targeted public-test Clippy,
rustfmt and `git diff --check` passed. The full private HAL, core fixture and
DX12 suites were not rerun for this test-only increment; their latest results
remain 347/347, 89/89, and 85/85 respectively at `d0efe2753`. Continue with
other validated render-pass combinations and the core/HAL audit.

## Checkpoint — query-gated R16G16_SNORM sampling — `95a7d50e8`

Microsoft's required DXGI format table distinguishes `R16G16_SNORM` 2D texture
support at FL9_1 from `SHADER_SAMPLE`, required beginning at FL9_2. The
capability model now records that distinction; an FL9_1 path is considered
only when the selected device's format query positively reports both
`TEXTURE2D` and `SHADER_SAMPLE`. It remains single-mip sampled/upload only.

The physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) was explicitly created at FL9_1 and returned
`R16G16_SNORM CheckFormatSupport=0x00021122`, which lacks `SHADER_SAMPLE`.
The capability projection reports no usages, private HAL texture creation
rejects the format, and the public test observes the same validation error.
The strict public SNORM test still sampled R8G8_SNORM and R8G8B8A8_SNORM and
matched all 64 output pixels. Synthetic capability tests cover a complete
positive query and reject incomplete or unavailable queries. A conditional
padded upload/sample path is implemented, but no GPU has exercised that
positive branch; no R16G16_SNORM sampled execution is claimed.

Verification (Rust 1.93.1, Visual Studio MSVC environment):

- The full strict private HAL suite passed 349/349. The R16G16_SNORM case was
  a GPU-query-driven rejection check on this adapter; other suite hardware
  cases executed normally with strict hardware selection.
- The full strict public D3D11 suite passed 19/19 with no skips. Its SNORM case
  verified R16G16_SNORM is not exposed and rejected by core, and validated the
  supported R8G8/RGBA8 SNORM pixel output.
- The capability-model tests passed 24/24; the final focused optional-query
  test passed 1/1. The private query-gated hardware test passed 1/1 and the
  public SNORM integration test passed 1/1 on the RTX.
- The D3D11 core fixture passed 89/89; physical-adapter enumeration passed
  1/1; the DX12 core regression passed 85/85.
- HAL and public D3D11 Clippy passed. HAL reported nine existing warnings in
  unrelated code; public Clippy had no warning. Changed-file rustfmt and
  `git diff --check` passed.

The `VsDevCmd.bat` banner included `vError: Unknown error` but MSVC compilation,
linking and all test processes completed successfully. The mission remains
active. Next continue FL9_1 depth/stencil render-pass transition coverage and
the public core/HAL contract audit; HWND presentation remains off until a safe
cross-layer scheduling design is established.

## Checkpoint — public D24S8 pass transitions — `24bbe6bce`

Commit `24bbe6bce` adds strict public wgpu coverage for D24S8 depth/stencil
LOAD, CLEAR and per-aspect DISCARD followed by LOAD. A LOAD pass preserved
depth 0.5 and stencil 1 together; separate stencil-clear and depth-clear
comparisons proved each aspect changed independently. Separate stencil and
depth discard cases then loaded both aspects and verified core initialized the
discarded subresource before rendering. The depth-discard probe chose an
incoming 0.25 value with `Less`: it would pass against stale depth 0.5 but
fails against the core's initialized zero. A sampled D24S8 descriptor was
rejected by public core; the existing D16 test covers rejection of the
unsupported stencil-only view. This does not assert native contents after
discard or historical-driver behavior.

The baseline D24S8 stencil test passed 1/1 on the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) at requested/actual FL9_1. The new
transition test passed 1/1 on the same physical adapter and verified 320
pixels: 64 combined LOAD, 64 stencil CLEAR, 64 depth CLEAR, 64 stencil discard
then LOAD, and 64 depth discard then LOAD. The full strict public D3D11 suite
passed 20/20, with no skips; every case selected physical hardware at FL9_1.

Exact verification (Rust 1.93.1, Visual Studio x64 developer environment,
Cargo `-j16`):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_stencil_replace_and_compare_at_fl91 -- --nocapture --test-threads=1`: 1/1 baseline test passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d24s8_load_clear_discard_transitions_at_fl91 -- --nocapture --test-threads=1`: 1/1 passed with the GPU readback above.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 20/20 passed, 0 failed, 0 ignored; no hardware skips.
- `cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std`, targeted rustfmt check and `git diff --check` passed.

The initial depth-discard assertion expected a later `Greater` test to fail,
but it passed because core initializes the discarded aspect to zero. The final
`Less` probe now distinguishes the stale 0.5 value from that initialization.
The private HAL, controlled core fixture, adapter-isolation and DX12 suites
were not rerun for this public-test-only increment; their recorded results at
the previous code checkpoint remain separate evidence. Continue the broader
core/HAL audit, then return to the cross-layer HWND presentation scheduler.
The public adapter remains opt-in and offscreen-only.

### Follow-up — public D24S8 capability/aspect checks — `cdd06e5ca`

The D24S8 public test now records the selected adapter's format projection and
asserts it is render-attachment/transient only and single-mip. It also verifies
public core rejects both `DepthOnly` and `StencilOnly` D24S8 views while the
combined attachment view succeeds. The focused strict test passed 1/1 and the
full strict public suite passed 20/20 on the RTX at requested/actual FL9_1;
public-test Clippy, rustfmt and `git diff --check` passed. No HAL/core code
changed in this follow-up. Commit: `cdd06e5ca`.

## Checkpoint — compatible linear/sRGB texture copies — `0de479c09`

The public core permits a texture copy between RGBA8 linear and sRGB formats,
and between their BGRA8 equivalents, because each pair belongs to one DXGI
format type group. The D3D11 HAL previously required exact DXGI format equality
and returned `E_INVALIDARG` for these public-valid copies. The HAL now accepts
only those four symmetric linear/sRGB pairings in addition to exact-format
copies; unrelated cross-family formats remain rejected. Native copies preserve
the stored texel bits and do not perform sRGB conversion.

A strict public wgpu test uploaded asymmetric 4×4 patterns into both RGBA8 and
BGRA8 linear textures, copied each to its sRGB partner and then back to linear,
and read back both intermediate and round-trip textures through 256-byte rows.
All 32 checked pixels matched their original bytes exactly on the NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and
actual FL9_1, `software=false`.

Verification (Rust 1.93.1, Visual Studio x64 developer environment, Cargo
`-j16`):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_srgb_texture_copies_preserve_bits_at_fl91 -- --nocapture --test-threads=1`: 1/1 physical GPU test passed.
- The complete strict public suite passed 21/21, with no skips; every test selected the RTX at forced FL9_1.
- The complete private HAL suite passed 350/350 under strict hardware mode, including the new format-family CPU test and existing GPU rendering/transfer tests.
- The controlled core D3D11 fixture passed 89/89, including selected physical adapter enumeration and core-to-HAL GPU execution.
- DX12 core regression passed 85/85.
- HAL and public wgpu-test Clippy, changed Rust file rustfmt check and `git diff --check` passed.

The initial invocation outside `VsDevCmd` lacked `link.exe`; rerunning in the
installed Visual Studio environment succeeded. `VsDevCmd` continues to print
`vError: Unknown error` in its banner, but all MSVC builds, links and test runs
completed successfully. Public HWND presentation remains gated by the
cross-layer `Present` reentrancy deadlock; the mission continues with the
remaining public core/HAL audit and scheduler design.

## Checkpoint — upstream Naga capability selector — `d93da4333`

The shared `wgpu-gpu` Naga capability test still treated `Backend::Dx11` as
unreachable. It now compares the adapter's projected Naga capabilities against
Naga's HLSL backend ceiling. The D3D11 API/module/device/queue/shader comments
and the testing guide were also corrected to describe the current opt-in,
offscreen adapter and core fixture rather than the pre-integration scaffold.

The first focused harness run selected the physical adapter but reported
`[Unsupported: Limits]`; its `ok` result represented a skip, not a test pass.
Commit `d93da4333` lowers this test's requested limits to the values required
by its no-resource capability check and the selected FL9_1 projection. The
rerun reported `[Executed]` and passed 1/1 on the RTX 3090 Ti at FL9_1. This
selector checks Naga capability projection and device setup; it does not draw
or read back pixels. The test crate also passed a normal `cargo check` with its
DX11 feature enabled.

The full `wgpu-gpu` harness command completed with Cargo's summary of 351
passed and zero failed. Its per-test names showed that only one test executed;
330 were unsupported by the advertised limits and 20 were skipped by test
expectations. The Cargo pass count therefore does not represent 351 executed
GPU tests. The one executed case was `naga_capabilities::validate_capabilities`.

Exact test commands:

- `WGPU_INFO_SAVE_GPUCONFIG_REPORT=1 cargo +1.93 test --offline -j16 -p wgpu-info --features wgpu/dx11 generate_gpuconfig_report -- --nocapture`
- `WGPU_BACKEND=dx11 WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-test --test wgpu-gpu --features wgpu/dx11 -- validate_capabilities --nocapture`
- `WGPU_BACKEND=dx11 WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-test --test wgpu-gpu --features wgpu/dx11 -- --nocapture`
- `cargo +1.93 check --offline -j16 -p wgpu-test --test wgpu-gpu --features wgpu/dx11`

For the runtime test only, the workspace's default `wgpu` feature list was
temporarily adjusted from `dx12`/`static-dxc` to `dx11` because this Build
Tools installation lacks `atls.lib`; the exact `Cargo.toml` bytes and existing
`.gpuconfig` bytes were restored in a `finally` block. The generated report
contained one DX11 adapter. The broader harness currently exercises almost
none of the public projection because its default requested limits exceed the
FL9_1 limits. The Deno CTS package still does not enable DX11, and this
non-WebGPU-compliant adapter must not be exposed to a conformance runner.

## Checkpoint — serialized public HWND presentation — `9ed1cc305`

The explicit `dx11` backend now reports a deliberately narrow public surface
contract for valid HWNDs: BGRA8 sRGB FIFO, opaque composition, render-attachment
usage and one frame of latency. Configure, acquire and present are restricted
to the HWND owner thread. A queue submission gate serializes queue submissions,
writes, mapping flushes, polling, surface configuration and selected
context-backed resource creation against `Present`. Same-thread reentrant work
that could mutate queue or mapping state is rejected. A HAL thread-affinity
query lets core reject the wrong thread before it waits on queue serialization.
The existing D3D11 default-backend selection is unchanged; callers must opt in
to `dx11`.

On the RTX 3090 Ti (`0x10de:0x2203`, NVIDIA driver `32.0.16.2101`), the device
was explicitly requested and confirmed at FL9_1 with `software=false`. The
public HWND test configured 64×64, rendered a clear, presented, dropped the old
backbuffer view, resized to 80×48, rendered and presented again, then verified
wrong-thread acquisition was rejected. The window was hidden: this proves
surface lifecycle and GPU presentation calls, not visible desktop output or a
continuously pumped message loop. Public D3D11 is still experimental and not
WebGPU-compliant.

Verification (Rust 1.93.1, Visual Studio 2022 x64 developer environment,
Cargo `-j32`):

- Strict private HAL suite: 351/351; strict D3D11 core fixture: 90/90; strict
  public D3D11 integration: 22/22, no failures or hardware skips. The public
  tests executed against the physical adapter at requested/actual FL9_1. The
  focused public HWND test passed 1/1 after the final HAL thread-affinity hook.
- Core DX12 regression: 86/86.
- Strict public adapter-selection test: 1/1; it explicitly enumerated the
  physical FL9_1 adapter through the opt-in D3D11 path. Normal backend
  priorities remain unchanged.
- Core and HAL Clippy passed with three and nine existing unrelated warnings;
  public-test Clippy was clean. Changed-file rustfmt and `git diff --check`
  passed. Workspace `cargo fmt --all -- --check` still reports the existing
  unrelated formatting differences in `naga/src/back/hlsl/writer.rs`.

The initial resize attempt failed because the test retained a view of the old
swapchain buffer; dropping that view before reconfiguration fixed the test.
Cross-thread acquisition is tested only after the final present because the
public core maps the HAL's wrong-thread surface error to `Lost` for that
surface. Exact commands (Rust 1.93.1):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1

& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1'
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1'
cargo +1.93 clippy --offline -j32 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-core/src/device/mod.rs wgpu-core/src/device/queue.rs wgpu-core/src/device/resource.rs wgpu-core/src/present.rs wgpu-core/src/resource.rs wgpu-hal/src/lib.rs wgpu-hal/src/dynamic/surface.rs wgpu-hal/src/dx11/adapter.rs wgpu-hal/src/dx11/surface.rs wgpu/tests/dx11_public.rs
git diff --check
```

The implementation checkpoint is commit `9ed1cc305` (`dx11: add serialized
public HWND presentation`).

## Follow-up — visible public HWND example — `ca0b026fd`

`wgpu/examples/dx11_fl91_windowed.rs` now creates and shows a native HWND,
selects a surface-compatible physical D3D11 adapter explicitly at FL9_1,
configures BGRA8 sRGB FIFO presentation, pumps the Windows message queue,
renders an animated indexed textured cube with D24S8 depth, and handles
client-area resize by reconfiguring the surface and recreating depth storage.
`--frames=N` makes the interactive example usable as a bounded smoke run.

The example compiled cleanly and ran for 60 frames on the NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
`software=false`. It printed `Presented 60 textured, indexed, depth-tested
frames.` and verified that the HWND was visible. This confirms the public
message pump and repeated surface render/present path. No screenshot or pixel
capture was taken; deterministic output remains covered by the public
offscreen readback test.

Code commit `ca0b026fd` (`dx11: add visible FL9_1 windowed example`) contains
the example and reproduction notes. Targeted Clippy, rustfmt and
`git diff --check` passed.

Exact command (Rust 1.93.1, Visual Studio x64 developer environment):

```powershell
cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std -- --frames=60'
```

The focused HWND lifecycle rerun also passed 1/1 after the surface checkpoint.
Next continue the public core/HAL contract audit, including additional
reentrant and resize edge cases, and independently inspect the visible output
when a visual-capture workflow is available. Historical FL9_1 behavior remains
unverified.

## Checkpoint — optional half-float vertex inputs (`867122688978524082ada4ba8614a3c3f4f72c3b`)

The required DXGI format table makes `R16G16_FLOAT` and
`R16G16B16A16_FLOAT` input-assembler support mandatory from FL9_3, not FL9_1.
The HAL now maps both formats but keeps them out of FL9_1 hardware guarantees.
The capability projection includes either only after the selected FL9_1
device's `CheckFormatSupport(IA_VERTEX_BUFFER)` query succeeds and the backend
mapping is present. Synthetic tests cover positive, absent-query and missing-
backend cases, including the resulting wgpu `DownlevelLimits` mask.

The RTX 3090 Ti (`0x10de:0x2203`, NVIDIA driver `32.0.16.2101`) was requested
and confirmed at FL9_1. Its public selected-device projection reports
`Float16x2=false`. The strict public test therefore logged
`SKIP: selected FL9_1 device does not report IA support for R16G16_FLOAT`; no
half-float draw is claimed as GPU-executed. `Float16x4` shares the same query
and mapping path and has no separate rendering test yet.

Verification after the code change: private HAL suite 351/351; public strict
D3D11 suite 23/23 test functions with one capability-based optional-path skip
(22 other public GPU tests executed); controlled core fixture 90/90; strict
physical adapter selection 1/1; core DX12 regression 86/86. HAL Clippy passed
with nine existing warnings outside this change; public-test Clippy passed
cleanly. Changed Rust files pass Rust 1.93 rustfmt and `git diff --check`.
The first core test invocation omitted `wgsl` and MSVC environment setup; it
failed at build configuration/link resolution and passed on the corrected
MSVC command below.

Commands run (Visual Studio 2022 x64 developer environment, bounded Cargo
parallelism `-j32`):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
```

Commit `867122688978524082ada4ba8614a3c3f4f72c3b` contains this implementation,
the public test and documentation. Continue auditing public core/HAL behavior
and optional vertex formats. Do not claim that the RTX executed Float16x2 at
FL9_1 or that a forced modern device validates historical FL9_1 drivers.

## Follow-up — visible FL9_1 frame capture

Re-ran the windowed example with strict hardware mode on the physical RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
`software=false`:

```powershell
cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std -- --frames=1800'
```

The run completed 1,800 frames. A second long bounded run was captured and
closed by posting `WM_CLOSE`; it presented 80,871 frames before the close was
processed. The client-area PNG was captured at the active monitor's physical
pixel coordinates after accounting for desktop DPI virtualization, then
visually inspected. It showed red and green cube faces over the expected dark
clear color. The temporary image is at
`%TEMP%\wgpu-dx11-fl91-windowed.png`; it is intentionally not checked into the
repository. This verifies visible output qualitatively, not deterministic
per-pixel presentation output; deterministic color/depth validation remains
covered by the public offscreen readback tests.

The initial capture attempt used DPI-virtualized coordinates and included
unrelated desktop regions. It was discarded and not used as evidence. The
final capture used per-monitor-aware coordinates and cropped to the HWND
client area. No code files changed during this verification.

## Checkpoint — selected-device optional vertex formats (`a4e9bc360`)

Code commit: `a4e9bc360d3355081292c87c9ce6ab1800252d8e`.

The HAL now maps all 41 wgpu vertex formats with direct DXGI IA equivalents,
excluding Float64 formats that have no faithful mapping. The FL9_1 static
guarantee set remains unchanged. Every mapped format outside that set enters
the selected-device projection only when `CheckFormatSupport` reports
`IA_VERTEX_BUFFER`; failed or missing queries stay unsupported. Allocation
tests verify the mapping table is unique and each stride matches
`VertexFormat::size()`. Synthetic projection tests cover the optional formats
as a whole, positive/negative device query results, and missing backend
mapping.

On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) explicitly forced
to FL9_1, the selected mask was `0x78c30104` and contained exactly the ten
guaranteed formats: `Uint8x4`, `Unorm8x4`, `Sint16x2`, `Snorm16x2`,
`Sint16x4`, `Snorm16x4` and `Float32` through `Float32x4`. No optional input
format was reported. The public Float16x2 test logged its capability skip;
this is not GPU execution evidence for optional formats.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 developer
environment, strict hardware mode, offline dependencies and Cargo `-j32`:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 vertex_format -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/capabilities.rs
git diff --check
```

Results: private HAL 351/351 before the final test-only invariant assertion;
the focused vertex-format test filter passed 5/5 afterward. Public D3D11
passed 23/23 test functions, with one capability-based optional Float16x2
skip and 22 other public GPU tests executed. The controlled core fixture
passed 90/90, physical adapter selection 1/1, and DX12 core 86/86. HAL Clippy
passed with nine existing warnings outside this change; public-test Clippy,
changed-file rustfmt and `git diff --check` passed. No optional-format GPU
execution or historical-driver behavior is claimed.

## Checkpoint — same-thread Present reentrancy coverage

Code commit: `f6b6d93a6a1746882c5384e341d37b95f01bcc6a`.

The controlled D3D11 core fixture now enters the production
`QueuePresentGuard` and invokes core operations on the same thread. It verifies
queue writes and submissions return `PresentationInProgress`, device polling
returns `Timeout`, and buffer/texture creation return the explicit
`DeviceError::PresentationInProgress`. Invalid resource IDs are dropped after
the creation checks. This exercises core behavior under the guard that wraps
native `Present`; it does not make DXGI deliver a window callback during the
test. The existing hidden-window public test still covers configure, acquire,
present, resize and owner-thread rejection.

The first test placement came after an existing injected OOM that deliberately
marks the fixture device lost, so resource creation correctly returned
`DeviceLost`. The reentrancy checks were moved before that failure injection;
the final strict test passed on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), requested/actual FL9_1, `software=false`:

```powershell
cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1'
```

Result: 1 targeted core fixture passed, 89 tests filtered out. Its full
readback workload also passed, including 64-pixel color checks across the
existing draw cases, plus the new guard assertions. The initial expected
failure during the device-lost phase was an ordering error in the new test,
not a backend failure. Rust 1.93 rustfmt and `git diff --check` pass. Core
Clippy passes with three pre-existing warnings in `dx11_fixture_tests.rs`
(one collapsible conditional and two unnecessary slice references); none are
in the new assertions. The full controlled fixture suite was not rerun; the
changed test passed individually, and the previous checkpoint recorded the
full fixture at 90/90.

## Checkpoint — optional R8G8_UNORM sampled upload at forced FL9_1

Code commit `672b6b250d2332eaae0d567f6d6af7f79f8ebbf0` adds a queried
`DXGI_FORMAT_R8G8_UNORM` path for one-mip, one-layer sampled/upload textures.
It is not treated as an FL9_1 guarantee: the selected device must positively
report both `D3D11_FORMAT_SUPPORT_TEXTURE2D` and
`D3D11_FORMAT_SUPPORT_SHADER_SAMPLE`, and unavailable or incomplete query
results remain unsupported. Backend transfer layout uses two bytes per texel.
The path does not expose render-target, copy-source, readback or mip-chain
uses, and the wgpu projection omits `FILTERABLE`.

The strict physical device was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`),
driver `32.0.16.2101`, software=false, requested and selected FL9_1. The native
query was `Value(181024)` (`0x0002c320`). The public test uploaded two
distinct RG texels, sampled the red channel and read back the 8×8 RGBA8 target;
all 64 pixels matched, with 32 left pixels `[64, 0, 0, 255]` and 32 right
pixels `[192, 0, 0, 255]`. The existing R8 baseline in the same test also
matched all 64 pixels. The different red values catch a mistaken one-byte
texel stride in the actual upload and sample path.

Verification used Rust 1.93.1 and Visual Studio 2022 x64 MSVC. Strict hardware
mode was enabled for hardware tests:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/texture.rs wgpu/tests/dx11_public.rs
git diff --check
```

Results: private HAL passed 353/353, including the synthetic optional-query
and two-byte layout tests plus forced-FL9_1 GPU rendering. The public D3D11
suite passed 23/23; the RG8 and R8 image paths both executed on hardware. The
separate optional Float16x2 vertex-format test logged its expected capability
skip. The physical D3D11 adapter-isolation test passed 1/1 and DX12 core passed
86/86. HAL Clippy passed with nine pre-existing warnings outside this change;
public-test Clippy, changed-file rustfmt and `git diff --check` passed. No
historical FL9_1 driver result is implied.

Next, continue the selected-device format and usage audit. Investigate another
bounded FL9_1 texture-format candidate against both the required-format table
and actual device query before implementing it. Keep optional formats
query-gated and the broader core/HAL contract audit active.

## Checkpoint — selected-device sampled-format filterability

Code commit: `4f1ac09a4` (`dx11: report query-backed sampled format filtering`).

For the implemented one-mip sampled/upload formats, a positive
`D3D11_FORMAT_SUPPORT_SHADER_SAMPLE` query now projects wgpu `FILTERABLE` /
`SAMPLED_LINEAR`. The same query already gates the private sampled-resource
path and `sampled_view` classifies the view as filterable from that bit. This
matches Microsoft's definition that the bit enables HLSL `Sample` with any
filter. Formats lacking the bit can be point-sampled in native D3D11 when the
resource format is supported, but this backend still rejects that subset; it
does not advertise point-only resources by inference.

The strict public R8/RG8 test still checks nearest sampling for both formats.
It now also renders RG8_UNORM using a linear sampler from two distinct texels;
all 64 output pixels matched the expected red gradient
`[72, 88, 104, 120, 136, 152, 168, 184]` across each row (tolerance 2). The
SNORM public test verifies `FILTERABLE` format flags from positive selected-
device queries and still uses nearest sampling. No linear SNORM output claim is
made.

The physical adapter was the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`),
driver `32.0.16.2101`, software=false, requested and actual FL9_1. It reported
`R8_UNORM=0x0002d3e0` and `R8G8_UNORM=0x0002c320`; both projected
`FILTERABLE`. `R16G16_SNORM=0x00021122` lacks `SHADER_SAMPLE`, and the current
path rejects it. Queries for `R16_FLOAT`, `R16G16_FLOAT` and
`R16G16B16A16_FLOAT` returned `E_FAIL` (`-2147467259`), which is an unavailable
observation, not proof of hardware absence.

Verification used the Rust 1.93.1 toolchain and Visual Studio 2022 x64 MSVC.
The strict hardware commands were run from a VS developer environment with
`WGPU_DX11_REQUIRE_HARDWARE=1`:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --config skip_children=true --check wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
```

Results: private HAL 353/353; public D3D11 23/23; physical adapter-selection
isolation 1/1; DX12 core 86/86. The public suite's Float16x2 vertex-input case
logged one expected capability skip. Six HAL test subcases rejected unsupported
runtime-integer shader constructs and consequently omitted only their follow-up
GPU render; these are intentional negative-coverage paths, not missing-hardware
skips. HAL Clippy passed with nine existing warnings in unrelated test code;
public-test Clippy, rustfmt and `git diff --check` passed. The relevant
capability unit test also passed 1/1. No historical FL9_1 behavior is implied.

The next bounded task is to investigate D3D11's point-only `Sample` behavior
for a resource format whose `TEXTURE2D` bit is present but `SHADER_SAMPLE` is
absent. Start with forced-FL9_1 R16G16_SNORM, whose RTX query is positive for
the resource but lacks the any-filter bit. Verify native texture/SRV creation,
point-sampler execution and public non-filterable projection before deciding
whether to expose the conditional subset.

## Checkpoint — query-backed point-only sampled textures

Code commit: `6c6b28d64` (`dx11: support query-backed point-only texture sampling`).

Microsoft's D3D11 format-support documentation permits point-filtered HLSL
`Sample` when `TEXTURE2D` is supported but `SHADER_SAMPLE` is absent. The
backend now treats those bits separately for its restricted one-mip sampled
upload formats: `TEXTURE2D` enables resource creation and sampling, while
`SHADER_SAMPLE` alone projects `FILTERABLE`. A failed query or missing
`TEXTURE2D` bit still rejects the format.

At forced FL9_1, the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false) reports `R16G16_SNORM=0x00021122`: the texture bit is present
and the any-filter bit is absent. The private HAL test uploaded padded
R16G16_SNORM data, created an SRV, point-sampled into RGBA8 and matched all
80 output pixels. The public adapter exposes the query-backed
`TEXTURE_FORMAT_16BIT_NORM` feature and reports sampled/upload usage without
`FILTERABLE`. Public core validation rejects a filtering-sampler bind for
this format; a non-filtering layout renders the signed data correctly and
all 64 checked output pixels match.

This surfaced and fixed a narrow core validation gap. For a filterable float
layout, core previously trusted the format's static sample type even when a
downlevel adapter's selected-device format flags omitted `FILTERABLE`. Core
now requires the actual view format capability in that case. The public test
exercises this check. The selected format feature remains query-gated, and
unsupported normalized formats retain empty format usages.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 environment.
The final complete strict HAL suite passed 355/355, including actual RTX
FL9_1 rendering. The strict public D3D11 integration suite passed 23/23; the
R16G16_SNORM point-only output and filter rejection both executed. The full
core D3D11 suite passed 87/87, the physical adapter isolation test passed
1/1, and DX12 core regression tests passed 86/86. HAL Clippy passed with nine
pre-existing test-code warnings; core and public-test Clippy passed without
warnings. Changed Rust files passed rustfmt and `git diff --check` passed.

Initial verification caught stale expectations: synthetic tests still
required `SHADER_SAMPLE` for point-only use, the adapter-isolation test assumed
the public feature set was always empty, and core allowed a filterable layout
for an unfilterable queried format. Those expectations and the core check were
updated; the full final suites above passed. The NVIDIA run establishes the
documented path only on this modern driver explicitly forced to FL9_1; it does
not establish historical driver behavior.

The packed-format audit found the three Windows 8 FL9_x texture formats to be
required at feature level 9_x and query-positive on this RTX. The separate
R16_UNORM, R16_SNORM, R16G16_UNORM and RGBA16_UNORM queries are unavailable;
R16G16_SNORM remains point-only and RGBA16_SNORM remains IA-only. The current
next task is the broader public core/HAL contract audit, with additional
formats gated on positive queries and actual resource/view execution.

## Checkpoint — query-backed packed 16-bit sampled texture family

Code commit: `552a4d019` (`dx11: add query-backed FL9_1 packed texture formats`).

The D3D11 format model now includes feature-gated `Rgb5g6b5Unorm`,
`Rgb5a1Unorm` and `Rgba4Unorm`. Each format has an independent
`CheckFormatSupport` result; the shared native-only
`TEXTURE_FORMAT_16BIT_PACKED` feature is projected only when all three
selected-device sampled paths are available. The backend exposes one-mip,
one-layer `TEXTURE_BINDING | COPY_DST` use and derives filterability from the
selected query. Other usage combinations remain unavailable.

On 2026-11-05, the RTX 3090 Ti (`0x10de:0x2203`, D3D11 driver
`32.0.16.2101`, software=false) was explicitly created at FL9_1. Format query
masks were `B5G6R5_UNORM=0x0026F3A0`, `B5G5R5A1_UNORM=0x000213A0` and
`B4G4R4A4_UNORM=0x000213A0`. The strict public wgpu test performed padded
uploads and separate sampling/readback renders for all three. Every one of
the 64 pixels per output matched; the RGB5A1 and RGBA4 cases also verified
transparent alpha. The selected-device reports were
`COPY_DST | TEXTURE_BINDING` and `FILTERABLE | SINGLE_MIP_ONLY` for all three.

Rust 1.93.1 checks passed for `wgpu-hal --features dx11`, the controlled
`wgpu-core` fixture, and the public `dx11_public` test target. The synthetic
packed-format capability test passed 1/1, packed-format metadata tests passed
2/2, the selected-device capability diagnostic passed 1/1, and the strict
public GPU test passed 1/1 (three format renders, 192 checked pixels). The
complete strict suites passed: private HAL 356/356, public wgpu 24/24, D3D11
core fixture 90/90, and DX12 core 86/86. The full `wgpu-types` suite passed
79 unit tests; its doctests passed 14 with 21 ignored. The packed public case
was run as part of the 24-test strict suite and passed with hardware required.

The strict suite commands were:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
```

The first three ran after setting `WGPU_DX11_REQUIRE_HARDWARE=1` inside the
Visual Studio 2022 x64 developer environment. The DX12 command was a CPU
regression suite and did not use the strict D3D11 variable.

Shared format conversion was kept backend-safe: DXGI and Vulkan receive the
matching packed format mapping; GLES has native upload descriptors but reports
the formats unsupported until its optional-feature projection is implemented;
Metal explicitly returns no format capability. `wgpu-hal` checked with
`dx11`, `dx12` (through the DX12 core test), `gles` and `vulkan` feature sets.
The Metal target cannot be compiled in this Windows environment.

The first DX12 regression compile found the newly added global format variants
missing from the common DXGI conversion match; that compile error was fixed by
adding their exact native mappings before rerunning the complete DX12 core
suite. The initial GLES check was run without MSVC setup and stopped because
`link.exe` was not on PATH; the developer-shell retry compiled successfully.

Clippy passed for HAL, core, the public test target and `wgpu-types`. HAL still
reports nine existing warnings in unrelated test code; core reports one
existing collapsible-if warning; `wgpu-types` reports fourteen pre-existing
unused serde-import warnings without its serde feature. The newly introduced
core test comparison warnings were fixed. Changed-file rustfmt and
`git diff --check` passed. Visual Studio's `VsDevCmd.bat` prints an existing
`Unknown error` banner in this environment, but the configured MSVC compiler
and linker worked for all successful builds.

The selected-device feature projection and the three sampled/upload paths
passed on the RTX 3090 Ti forced to actual FL9_1. Historical FL9_1 drivers and
ARM32 UWP remain unverified. Continue with the broader public core/HAL contract
audit; do not infer support for other formats or usages from these three.

## Follow-up — optional R32 and D32 format audit

Extended the physical-device capability diagnostic to query R32_FLOAT,
R32G32B32A32_FLOAT, D32_FLOAT and D32_FLOAT_S8X24_UINT, because these formats
are plausible next candidates for sampled or depth use. The strict diagnostic
passed 1/1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), forced
to actual FL9_1. Both R32 formats returned only `IA_VERTEX_BUFFER`
(`0x00000002`), without `TEXTURE2D`; both D32 queries returned `E_FAIL`.
No new resource path was added. The results and Microsoft minimum-level
context are in `CAPABILITIES.md`.

Command, run with `WGPU_DX11_REQUIRE_HARDWARE=1` in the Visual Studio 2022 x64
developer environment:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 fl91_wgpu_projection_matches_the_selected_physical_device -- --nocapture --test-threads=1
```

Continue the public core/HAL contract audit. The format query evidence remains
specific to the modern RTX forced to FL9_1; historical drivers and ARM32 UWP
remain unverified.

## Follow-up — outputless fragments with D24S8 stencil

Commit `967b1af89` (`dx11: verify outputless FL9_1 fragments with D24S8`)
adds a strict public test for a targetless outputless fragment in a D24S8 pass.
The shader discards the left half and writes depth plus stencil reference 7 on
the surviving half. A later stencil-equality pass renders red only where
discard left stencil at zero; a separate depth comparison renders green only
where discard left depth at its clear value. The other half remains blue in
both results. All 128 pixels across the two readbacks matched.

The complete strict public D3D11 suite passed 38/38 on the NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, requested/actual FL9_1, software=false).
The optional Float16x2 IA branch logged its expected capability skip. Exact
focused and suite commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_outputless_fragment_depth_stencil_pass_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

Rustfmt, public-test Clippy and `git diff --check` passed. Next resume the
broader core/HAL render-pass contract audit. Historical FL9_1 drivers and
ARM32 UWP remain unverified.

## Follow-up — unmatched targetless fragment color output

Commit `b5fb590ac` (`dx11: verify ignored targetless fragment outputs`) adds
coverage for a core-legal fragment stage that returns `@location(0)` while a
depth-only pipeline has no matching color target. The HAL already compiles
result-bearing fragment entries directly, and the D3D11 pass binds no RTV; the
output is ignored without changing the implementation. The test fills D16 in
that pass, then attempts a farther color draw. All 64 output pixels stayed
blue, proving the unmatched color did not reach an attachment and the depth
write did occur. The same test retains outputless-fragment discard coverage.

The focused test passed 1/1 and the complete strict public D3D11 suite passed
38/38 on the physical RTX 3090 Ti at requested/actual FL9_1. The optional
Float16x2 IA branch logged its capability skip. Public-test Clippy, rustfmt and
`git diff --check` passed. Independent render-pass review's suspected HAL
mismatch was disproved by the existing direct-compile path and this execution
test. The next concrete audit item is selected-entry core gating for the
runtime integer semantics and matrix-aggregate stores rejected by HAL.

## Follow-up — query-backed R8/RG8 color attachments

Code checkpoint: `a6d958664` (`dx11: enable query-gated R8/RG8 color attachments`).

Extended the one-mip R8_UNORM and R8G8_UNORM paths to support color-attachment
usage when the selected device reports `D3D11_FORMAT_SUPPORT_RENDER_TARGET`.
The hardware guarantee, driver query, backend implementation and public
projection remain separate: R8_UNORM is a required FL9_1 texture format but
render-target support is not guaranteed; R8G8_UNORM is itself an optional
format. Both are reported with `RENDER_ATTACHMENT` only after a positive
per-format query and remain single-mip, single-layer and single-sample. A later
increment gates blendability on the separate `BLENDABLE` query. Neither
exposes `COPY_SRC` or direct readback. The existing
sampled/upload path is preserved.

The strict public test creates a dual-bound source texture for each format,
clears it through an attachment-only view, samples its contents through a
separate texture-binding view and reads an RGBA8 output. On the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and actual FL9_1, `CheckFormatSupport` returned R8_UNORM=`0x0002d3e0` and
R8G8_UNORM=`0x0002c320`. Both include `RENDER_TARGET` (`0x00004000`), while
the original attachment checkpoint did not project its separate `BLENDABLE`
bit; the later follow-up does so only on a positive query. The clear value
`(0.25, 0.75, 0.5, 1.0)` sampled as `[64, 0, 0, 255]` for R8_UNORM and
`[64, 191, 0, 255]` for R8G8_UNORM; all 128 output pixels matched exactly.

The capability model tests positive, missing-query, unsupported-bit and
backend-disabled cases. Synthetic queries also verify that a point-only
format can retain attachment support without `SHADER_SAMPLE`, which controls
filterability separately. The shared private sampled-format test now expects
native target creation only for R8/RG8 when the actual query contains the
render-target bit. The controlled core fixture checks the new R8 projection;
the public GPU test also exercises both formats through core validation.

Verification used Rust 1.93.1 in the Visual Studio 2022 x64 environment. With
`WGPU_DX11_REQUIRE_HARDWARE=1`, the private HAL suite passed 356/356, including
the selected-device capability diagnostic; six shader-execution subcases
printed `SKIP` after separately confirming that unsupported integer-indexing
shaders are rejected. The complete public D3D11 suite passed 25/25, including
the new two-format GPU test; one optional Float16x2 input-format case skipped
because the selected device reports no IA support. The complete controlled
D3D11 core fixture passed 90/90, including adapter projection and public
enumeration, and DX12 core passed 86/86.

The relevant CPU capability subset passed 28/28. Clippy passed for HAL, core
and the public test target. HAL retains nine pre-existing warnings and core
one pre-existing collapsible-if warning; the public test target was warning
free. `git diff --check` passed. Workspace formatting check still reports five
pre-existing formatting differences in `naga/src/back/hlsl/writer.rs`; all
changed Rust files pass formatting checks. No Naga formatting changes were
made. The Visual Studio developer-shell setup continues to print its existing
`Unknown error` banner despite resolving the MSVC tools and running all
commands successfully.

The next task remains the public core/HAL contract audit. These query results
describe only the modern RTX 3090 Ti forced to FL9_1; historical FL9_1 drivers
and ARM32 UWP remain unverified.

## Follow-up — same-surface HWND operation re-entry

Code checkpoint: `be8e58729` (`dx11: test HWND surface operation reentrancy`).

Extended the strict hidden-HWND integration path to hold the surface operation
guard and call the actual HAL `configure` and `acquire_texture` methods again
on the owner thread. Both return the explicit `D3D11 surface operation
already in progress` error without disturbing the configured swap chain. The
same test then continues through acquire, render/readback, present, resize and
recreation.

With `WGPU_DX11_REQUIRE_HARDWARE=1`, the targeted strict run passed both hidden
surface tests on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), explicitly requested and actual FL9_1. Both clear and
clone-sampled paths presented, resized and completed their existing readback
checks. `rustfmt --check --edition 2021` passed for the changed test file, and
HAL Clippy passed with the same nine pre-existing warnings. `git diff --check`
passed.

Command run inside the Visual Studio 2022 x64 developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 fl91_hidden_hwnd -- --nocapture --test-threads=1
```

An attempted direct `IDXGISwapChain2::GetMaximumFrameLatency` diagnostic
returned `DXGI_ERROR_INVALID_CALL` for this swap chain, which is created
without `FRAME_LATENCY_WAITABLE_OBJECT`. The diagnostic was removed without
changing swap-chain behavior. Microsoft's API documentation states that this
getter's default is one; explicit waitable-object behavior remains outside
the current surface contract. The hidden test does not claim to verify the
latency through that unsupported query path.

Independent read-only audits found no confirmed route from a valid advertised
operation into a HAL panic and no confirmed public render-pass/surface contract
violation. Continue the broader public core/HAL audit; historical FL9_1 and
ARM32 remain unverified.

## Follow-up — query-gated B5G6R5 color attachment

Code checkpoint: `4b013795f` (`dx11: enable query-gated RGB565 render attachments`).

Extended the selected-device color-attachment subset to packed B5G6R5. The
capability predicate requires the existing `TEXTURE_FORMAT_16BIT_PACKED`
support, the format's `TEXTURE2D` query, the backend color-target path and a
positive `RENDER_TARGET` query. It adds no `COPY_SRC` or blendability, even
though the selected RTX query also reports the `BLENDABLE` bit. RGB5A1 and
RGBA4 remain sampled/upload-only.

The first strict execution exposed a native texture-helper whitelist that
still rejected RGB565 render targets after public/core validation had admitted
them. Adding B5G6R5 to that guarded native path fixed creation. A public test
now draws into R8_UNORM, R8G8_UNORM and RGB565 attachments, samples each into
RGBA8, and validates all 64 pixels within one UNORM code value. The
first observed pixels were `[66, 0, 0, 255]`, `[66, 129, 0, 255]` and
`[66, 130, 132, 255]`.

On the physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested
and actual FL9_1, the per-format masks were R8=`0x0002d3e0`,
RG8=`0x0002c320` and B5G6R5=`0x0026f3a0`; all report `RENDER_TARGET`. The
public strict test passed 1/1, the synthetic capability test passed 1/1, and
the controlled strict core fixture passed 1/1 (89 filtered tests). Full strict
regressions then passed: private HAL 357/357, public D3D11 25/25, controlled
core fixture 1/1 and DX12 core 86/86. The hardware and fixture commands used
Rust 1.93 in the Visual Studio 2022 x64 environment; the D3D11 runs set
`WGPU_DX11_REQUIRE_HARDWARE=1`. The HAL and core Clippy runs passed with nine
existing HAL warnings and one existing core warning; public Clippy was clean.
Changed-file rustfmt and `git diff --check` passed. `VsDevCmd.bat` continues to
print its existing `Unknown error` banner while successfully configuring
MSVC. Historical FL9_1 devices and ARM32 remain unverified.

The full public suite initially found the older packed-format upload test
still asserting that RGB565 could never be a render target. It now requires
render-target usage for RGB565, keeps RGB5A1/RGBA4 sampled-upload-only, and
continues to assert no copy-source use. The corrected full suite passed.

Exact verification commands, using rustc/cargo 1.93.1:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --lib --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib --tests
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/texture.rs wgpu/tests/dx11_public.rs
git diff --check
```

## Follow-up — rendered narrow-format copy/readback

Code checkpoint `52b573401` extends the selected-device `COPY_SRC` proof to
the combined `RESOURCE | COLOR_TARGET | COPY_SRC` usage for R8_UNORM,
R8G8_UNORM and B5G6R5. The public test renders a constant color into each
attachment, samples the result into RGBA8, and also copies the native attachment
to a nonzero-offset readback buffer with 256-byte rows. It checks all 64
sampled pixels, all 64 raw texels, the offset prefix and row padding. This
covers contents produced by rendering, in addition to the prior partial-copy
test that began from uploads.

R8's sampled and raw red values were `[66]`; RG8 sampled `[66, 130]` while its
native copied bytes were `[66, 129]`; RGB565 decoded to `[66, 130, 132]` and
its packed bytes matched the corresponding 5:6:5 quantization. The raw RG8
green check allows a one-code difference from the RGBA8 sampled result, so it
checks every texel without hard-coding the RTX driver's intermediate rounding.
Follow-up code checkpoint `2d3520c39` applies this tolerance and reruns the
strict suite.

Strict hardware mode passed the focused rendered-attachment test and the full
public suite, 26/26, on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, physical adapter, software=false), requested and actual
FL9_1. One optional Float16x2 input branch was visibly skipped because the
selected FL9_1 device did not report IA support; no hardware test was skipped.
The combined-usage synthetic capability case passed 1/1. The targeted
strict HAL suite had passed 359/359 immediately before this test-only
extension; HAL and public Clippy passed, with nine pre-existing HAL warnings
and no public warnings. Rustfmt and `git diff --check` passed.

Exact strict command, run with Rust 1.93.1 in the Visual Studio 2022 x64
environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 narrow_optional_copy_sources_require_render_target_support_and_backend_paths -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --lib --tests --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
```

The tested format masks remain R8=`0x0002d3e0`, RG8=`0x0002c320` and
B5G6R5=`0x0026f3a0`; each has `RENDER_TARGET`. These results apply to the
modern RTX forced to FL9_1 and do not verify historical FL9_1 drivers or ARM32
UWP. Next audit core-valid read-only depth/stencil render-pass combinations.

## Follow-up — public D24S8 read-only pass aspects

Code checkpoint `3a1b84bd0` expands the public D24S8 transition test to cover
all three core-valid read-only shapes without a sampled alias: both aspects
read-only, depth read-only with stencil writable, and stencil read-only with
depth writable. Each mixed case uses a later depth/stencil comparison to
verify the read-only aspect stayed unchanged while the writable aspect took
the expected update. Error scopes verify that a depth-writing pipeline is
rejected in a depth-read-only pass and a stencil-writing pipeline is rejected
in a stencil-read-only pass.

The first targeted run exposed only an incorrect expected-image classification
for an existing stencil-discard snapshot; that original image correctly stays
blue. After fixing the oracle, the focused strict test passed 1/1 and the full
strict public D3D11 suite passed 26/26. The expanded test checks 512 pixels
across eight snapshots on the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), explicitly requested and actual FL9_1. The
suite printed one capability skip for optional Float16x2 IA support; no
hardware execution was skipped. The D3D11 projection still omits
`READ_ONLY_DEPTH_STENCIL`, so simultaneous sampling of the DSV remains
unsupported. HAL code and capabilities were unchanged.

The public test crate passed Clippy without warnings. Rustfmt and
`git diff --check` passed.

Exact commands, run with Rust 1.93.1 in the Visual Studio 2022 x64 environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d24s8_load_clear_discard_transitions_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

## Follow-up — public colorless D24S8 depth prepass

Code checkpoint `78b52faf1` adds a public wgpu depth-only render pass with no
color attachments. It clears D24S8 depth/stencil, writes near depth without a
fragment shader, then starts a color pass and draws far geometry with a Less
comparison. All 64 pixels remain blue, proving the prepass affected later
depth testing. The existing D24S8 transitions now check 576 pixels across nine
snapshots, including read-only aspects and colorless rendering.

The exact-tree strict public run passed 26/26 tests using Rust 1.93.1 and the
Visual Studio 2022 x64 environment. The physical adapter was NVIDIA RTX 3090
Ti (`0x10de:0x2203`), driver `32.0.16.2101`, software=false, requested and
actual FL9_1. No hardware execution skipped; the optional Float16x2 IA branch
printed its explicit capability skip. The public test crate's Clippy,
rustfmt and diff checks also passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: exercise transient attachments through public wgpu/core/HAL with valid
clear/discard operations and verify the supported non-persistent semantics.
Continue the broader render-pass and core/HAL contract audit afterward.

## Follow-up — public transient attachment passes

Code checkpoint `d224dcd3e` adds strict public wgpu coverage for the existing
transient attachment implementation. It creates transient RGBA8 and D24S8
resources, rejects sampled transient usage, color LOAD/STORE, and depth LOAD,
then submits a transient color clear/draw/discard pass and a transient D24S8
clear/draw/discard pass. In the latter, a near green draw writes depth and a
far red draw fails; all 64 pixels in the ordinary stored color target are
green. Discarded transient data is not read back because public usage forbids
preserving it. These D3D11 textures use ordinary allocations; no memoryless
behavior or memory savings are claimed.

Follow-up commit `bbceec53d` adds public negative cases for read-only transient
depth and a render pass with no attachments. Both report validation errors on
encoder finish and do not reach the HAL. The test summary now states those
rejections along with invalid transient LOAD/STORE and sampled usage.

Invalid render-pass descriptor errors are deferred by core until the parent
encoder is finished. The negative checks therefore finish isolated encoders
before popping their error scopes. The first focused run initially appeared to
accept transient LOAD because the test popped the scope before finishing; after
following core's error path and correcting the test, the strict focused test
passed 1/1. After the negative validation extension, the full public suite
passed 27/27. Hardware was the physical
NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false),
requested and actual FL9_1. No GPU test skipped; the optional Float16x2 IA
branch printed its explicit unsupported-capability skip. Public Clippy,
rustfmt and diff checks passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_transient_attachments_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: exercise public `LoadOp::DontCare` on color and depth attachments with
full writes that make the resulting color deterministic. Continue the
remaining render-pass/core-HAL contract audit afterward.

## Follow-up — public color/depth `LoadOp::DontCare`

Code checkpoint `7538ada21` adds a strict public wgpu pass using
`LoadOp::DontCare` for both an RGBA8 color target and D16 depth. The test uses
the required unsafe token only with a full-screen first draw: depth compare
Always writes every depth sample and color blending is disabled, so every
color/depth pixel is overwritten before the subsequent Less test or any
readback. The later, farther draw fails the newly written depth test. Strict
FL9_1 readback found `[255, 0, 0, 255]` at all 64 pixels.

The focused strict test passed 1/1 on the physical NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested and actual
FL9_1. The full public test suite passed 28/28; it logged one optional
Float16x2 IA capability skip and no hardware-execution skips. Public Clippy,
rustfmt and diff checks passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_dont_care_load_overwrite_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: verify that a color target's discarded contents are initialized before
subsequent GPU readback, rather than exposing retained D3D11 texture contents.
Continue the render-pass and core/HAL contract audit afterward.

## Follow-up — discarded color reinitialized before copy

Checkpoint `fa305b35b` adds a strict public wgpu test that clears an RGBA8
render target red, stores it with `StoreOp::Discard`, then copies the texture
to a mapped readback buffer. Core's discard tracking initializes the resource
before the later read; all 64 pixels were transparent black, so the discarded
red contents were not exposed.

The focused test passed 1/1 and the complete strict public suite passed 29/29
on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), with requested and actual FL9_1. No hardware test skipped;
the optional Float16x2 vertex-input branch printed its explicit capability
skip. Public-test Clippy, rustfmt and `git diff --check` passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_discarded_color_is_initialized_before_read_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: bind the discarded texture as a sampled view in a later public render
pass and verify that core's initialization is visible through the SRV path.
Continue the contract audit after that focused test.

## Follow-up — discarded color reinitialized before sampling

Checkpoint `664a7626f` adds public wgpu coverage for a render-target-to-SRV
transition after `StoreOp::Discard`. The first pass clears an RGBA8 texture red
and discards it. A second pass samples it through sparse texture/sampler
bindings into a separate RGBA8 target, which is copied to a mapped buffer.
Core initializes the discarded resource before the sampled read; every one of
the 64 output pixels is transparent black.

The focused strict test passed 1/1 and the full strict public suite passed
30/30 on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), requested and actual FL9_1. The optional Float16x2 input
branch logged a capability skip; the new test and the other required hardware
paths executed. Public Clippy, rustfmt and `git diff --check` passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_discarded_color_is_initialized_before_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: check a later render-pass `LoadOp::Load` after color discard, which
exercises attachment read/modify/write initialization separately from copy and
sampled reads. Then continue the public core/HAL contract audit.

## Follow-up — initial color attachment load

Checkpoint `ab7b1ec37` adds a strict public test that creates an RGBA8
render-attachment texture, uses it first with `LoadOp::Load`, stores it, and
copies it to CPU readback. On the physical RTX 3090 Ti explicitly requested at
FL9_1, all 64 pixels were transparent black, confirming core initializes
previously unwritten attachment contents before a load.

The focused test passed 1/1. The complete strict public suite passed 32/32;
the optional Float16x2 vertex-input branch printed its capability skip, while
required FL9_1 paths executed. Public-test Clippy, rustfmt and `git diff
--check` passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_new_color_is_initialized_before_load_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Next: validate resource retention when a command buffer references a texture
and view whose public handles are dropped before queue submission.

## Follow-up — resource retention and unsupported-operation gates

Commit `93dd3b3e0` adds a strict public test that records an RGBA8 clear and
texture-to-buffer copy, drops the texture and view before submission, and
reads back blue from all 64 pixels. The selected NVIDIA RTX 3090 Ti reports
physical hardware and requested/actual FL9_1; this verifies command-buffer
ownership through execution.

Commits `7a409ae67`, `5729e4681` and `9a87d9a0f` add strict public negative
tests for indirect buffer, compute pipeline and occlusion-query creation. Core
rejects them using the missing `INDIRECT_EXECUTION`, `COMPUTE_SHADERS` and
`OCCLUSION_QUERY` capability gates before unsupported HAL entry points. These
are validation tests, not GPU execution tests.

The focused lifetime test and all three capability-gate tests passed. The full
strict public suite passed 36/36 on the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested/actual FL9_1. One optional
Float16x2 vertex-input branch was skipped because the selected device lacks
that format's IA support. Public Clippy, rustfmt and `git diff --check` passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_texture_references_survive_until_submission_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_indirect_buffers_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_compute_pipelines_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_occlusion_queries_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

## Verified checkpoint — outputless fragment in a depth-only pass

The D3D11 HAL now accepts a fragment stage with no color outputs when the
pipeline has a depth/stencil attachment and no color targets. Its direct Naga
HLSL form fails on the installed FL9_1 FXC with X4530 (`pixel shader must
minimally write all four components of SV_Target0`). The backend keeps the
original outputless fragment as a helper and adds an entry wrapper returning a
synthetic zero `SV_Target0`. The targetless render pass binds no RTV, so this
value cannot reach an application texture. Resource usage, inputs, helper
calls, `discard`, early-depth metadata and shader bindings are retained.

Strict hardware validation on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested/actual FL9_1:

- The focused public test passed 1/1. A conditional fragment discard in a
  D16-only pass prevented depth writes on the left half; a later farther draw
  rendered 32 red pixels there and failed depth on the other 32, which remained
  blue. All 64 readback pixels matched.
- The full public D3D11 suite passed 37/37; the optional Float16x2 IA branch
  printed its explicit capability skip because the selected device lacks the
  format query bit. Required hardware paths executed.
- The full private D3D11 HAL suite passed 360/360 under strict hardware mode.
- The core FL9_1 adapter selection test passed 1/1; it enumerated one physical
  FL9_1 adapter in the explicitly enabled DX11 configuration.
- The DX12 core regression suite passed 86/86.
- HAL Clippy passed with nine existing warnings; public D3D11 test Clippy,
  changed-file rustfmt and `git diff --check` passed.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu-hal --no-default-features --features dx11 --tests
cargo +1.93 clippy --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/hal_shader.rs wgpu-hal/src/dx11/shader.rs wgpu/tests/dx11_public.rs
git diff --check
```

Code commit `016f048447b45896757000f49d4e9fdc44484501`
(`dx11: support outputless FL9_1 depth-only fragments`) contains the
implementation, strict GPU test and capability documentation. Independent
HAL review found no concrete semantic or validation defect. The only remaining
coverage opportunity in this narrow path is a targetless shader that returns
only unmatched nonzero color locations; it is not known to fail. Next: verify
outputless targetless-fragment behavior with D24S8 stencil state, then continue
the broader core/HAL render-pass audit. Historical FL9_1 and ARM32 execution
remain unverified.

## Follow-up — outputless fragment depth and stencil effects

Commit `016f04844` added the outputless fragment wrapper after the installed
FL9_1 FXC rejected Naga's direct `void` pixel shader with X4530. Commit
`5ae420357` recorded its validation. This follow-up adds a public test for the
same pipeline shape with a D24S8 target and stencil `Replace` enabled. The
discarded half retains depth 1 and stencil 0; the surviving half receives
depth 0.25 and stencil 7. A later stencil-equality comparison renders red on
the discarded half only. A separate depth comparison renders green there only.
Both images were read back and all 128 pixels matched.

The focused test passed 1/1 and the complete strict public D3D11 suite passed
38/38 on the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested/actual FL9_1, hardware adapter. The optional Float16x2 IA input
branch printed SKIP because the selected device does not report that format's
IA support. `git diff --check` and changed-file rustfmt passed. The outputless
fragment implementation, existing 360-test HAL suite, core adapter selection
and DX12 regressions were already verified in commit `016f04844`; this
follow-up changed only public test coverage and documentation.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_outputless_fragment_depth_stencil_pass_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

Code/test commit `016f04844` is followed by documentation/test-journal commit
`5ae420357`, which recorded the D16 checkpoint. This D24S8 test and its
documentation are the current follow-up. Next: continue the core/HAL
render-pass and pipeline-state audit, resolving concrete public contract
mismatches with narrow tests. Historical FL9_1 devices and ARM32 UWP remain
unverified.

## Checkpoint — selected-entry integer and matrix-store validation — `320c21ab1`

Added `DownlevelLimits::runtime_integer_shader_operations` and
`DownlevelLimits::matrix_aggregate_stores`. D3D11 FL9_1 reports both false;
GLES, noop and default adapters report true. The limits participate in limit
requirements and WebGPU-compliance checks, deserialize as true for older
serialized snapshots, and appear in `wgpu-info` output. The capability names
describe backend/compiler behavior, not FL9_1 hardware guarantees.

Core summarizes validated Naga IR per entry point. The bounded integer scan
follows reachable helper functions and retains the existing direct
integer-vector-to-float vertex-input conversion. The matrix scan follows
pointer arguments into helpers and rejects stores through aggregates containing
two-row matrices, while allowing safe scalar siblings and matrix reads. Scan
budget exhaustion fails closed. Render-pipeline validation rejects these
selected entries before HAL. A dedicated fragment sample-mask rejection runs
before the broad integer gate so the existing precise validation error is
preserved.

Validation on 2026-11-06:

- The initial strict public suite found the sample-mask error-precedence issue
  described above (37 passed, one failed); after reordering the gates, the
  sample-mask test passed 1/1 and the complete suite passed 38/38. The only
  optional branch skip was Float16x2 IA support, which the selected device
  does not report.
- The strict full private D3D11 HAL suite passed 360/360 on the physical RTX
  3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, requested/actual FL9_1,
  software=false. Hardware-dependent cases ran with
  `WGPU_DX11_REQUIRE_HARDWARE=1`.
- Focused core selected-entry and DX11 adapter-isolation tests passed 3/3;
  the DX12 core regression suite passed 88/88.
- The wgpu-types downlevel-limit test passed 1/1. `wgpu-info` and GLES HAL
  checks compiled successfully.
- Core and public-test Clippy passed without warnings. HAL Clippy passed with
  nine pre-existing warnings in HAL tests; wgpu-types Clippy/test emitted 14
  existing unused serde-import warnings because that command did not enable
  serde. Changed Rust files passed rustfmt; `git diff --check` passed.

Commands included:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -p wgpu-types --lib unsupported_fl9_shader_semantics
cargo +1.93 check --offline -p wgpu-info
cargo +1.93 check --offline -p wgpu-hal --no-default-features --features gles
cargo +1.93 clippy --offline -p wgpu-hal --no-default-features --features dx11 --tests
cargo +1.93 clippy --offline -p wgpu-core --no-default-features --features dx11,std,wgsl --tests
cargo +1.93 clippy --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
cargo +1.93 clippy --offline -p wgpu-types --tests
rustfmt +1.93 --edition 2021 --check tests/src/params.rs wgpu-core/src/device/resource.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-core/src/pipeline.rs wgpu-core/src/validation.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/noop/mod.rs wgpu-hal/src/gles/adapter.rs wgpu-info/src/human.rs wgpu-types/src/limits.rs wgpu/tests/dx11_public.rs
git diff --check
```

Next: continue the core/HAL contract audit with shader entry-point operations
and render-pass combinations still accepted by core but rejected or unhandled
by this HAL. Preserve specific existing validation-error precedence when
adding broader capability gates. Optional formats, historical FL9_1 devices,
and ARM32 UWP remain separately unverified.

## Checkpoint — selected-entry `textureGather` validation — `25e9f55db`

Added public `DownlevelLimits::texture_gather` with backward-compatible serde
defaulting. The D3D11 hardware model treats shader model 4.1 gather as a static
feature-level guarantee from FL10_1; the current backend implementation flag
is false and the FL9_1 public projection therefore rejects gather. GLES reports
the capability only for ES 3.1+, desktop GL 4.0+, or
`GL_ARB_texture_gather`; noop retains its permissive default.

Core detects Naga `ImageSample::gather` in the selected vertex or fragment
entry point, including reachable helper functions, and reports a validation
error before HAL. Other entry points in the same module are unaffected. The
Naga-to-FXC test confirms the level-9_1 profile fails with X4532 while the
shader-model-5 control compiles. The strict public test rejects direct and
helper-reached WGSL gather and compiles an implicit-sampling sibling on the
physical adapter.

Strict hardware evidence on 2026-11-06: NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`), driver `32.0.16.2101`, hardware adapter, requested and actual
FL9_1. The focused public test passed 1/1 and reported the selected adapter;
the full strict public D3D11 suite passed 38/38; the full strict private D3D11
HAL suite passed 361/361. `textureGather` itself was intentionally not sent to
the GPU: public core rejects it based on the selected FL9_1 contract.

Other checks: the selected-entry CPU test and `wgpu-types` limit test passed
1/1 each; the serde compatibility test passed 1/1; DX12 core regression passed
89/89; `wgpu-info` and GLES checks passed. Clippy passed for core, wgpu public
tests, types with serde, GLES and wgpu-info. HAL Clippy passed with nine
pre-existing warnings in unrelated test code. Changed Rust files passed
rustfmt and `git diff --check` passed.

Useful commands (the test process was launched from the VS 2022 developer
environment with pinned Rust 1.93 and bounded `-j32`):

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-types --lib texture_gather_is_a_downlevel_capability
cargo +1.93 test --offline -j32 -p wgpu-types --lib --features serde previous_downlevel_limits_serde_shape_defaults_new_caps
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl texture_gather_limits_follow_selected_entry_and_helpers
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 --lib texture_gather_is_rejected_by_level_9_1_fxc_profile -- --nocapture
cargo +1.93 check --offline -j32 -p wgpu-info
cargo +1.93 check --offline -j32 -p wgpu-hal --no-default-features --features gles
```

Next: continue checking selected-entry shader operations and render-pipeline
states that core accepts against the D3D11 FL9_1 path. Do not enable a path
until it is implemented or rejected through core validation. Historical FL9_1
devices and ARM32 UWP remain unverified.

## Follow-up — compute-stage textureGather gate and profile boundary — `31ac3d2ef`

The selected-entry gather check now covers compute pipelines as well as render
pipelines. It runs after core validates the chosen compute entry's bindings and
before HAL pipeline creation, returning a validation error when the stage-
agnostic `DownlevelLimits::texture_gather` is false. Naga CPU analysis covers
direct and helper-reached compute gathers, and verifies that a positive limit
permits them. The FXC test also checks the exact `ps_4_1` boundary and compiles
a compute gather under `cs_5_0`.

The hardware model's single gather boolean applies to all WGSL stages, so its
static guarantee now begins at FL11_0: SM4.1 covers vertex/geometry/pixel
Gather, while compute gather4 requires SM5. FL9_1 remains false and the
backend still does not execute gather. This avoids implying compute support at
FL10_1 from the graphics-only SM4.1 guarantee.

Validation on 2026-11-06, using pinned Rust 1.93.1 and the Visual Studio
developer environment:

- Strict private D3D11 HAL suite: 361/361 passed. Hardware cases selected the
  physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
  `32.0.16.2101`), explicitly requested and confirmed FL9_1, software=false.
- Strict public D3D11 suite: 38/38 passed on the same physical adapter and
  selected level. Its optional Float16x2 IA branch explicitly skipped because
  the selected device lacks the queried format support. Gather itself was
  rejected before HAL and was not issued to the GPU.
- Core gather selected-entry/helper/compute analysis: 1/1; the adjacent
  explicit-LOD selected-entry test: 1/1; exact FXC gather profile test: 1/1.
  FXC rejected `ps_4_0_level_9_1` with X4532 (`cannot map expression to
  pixel shader instruction set`) and accepted `ps_4_1`, `ps_5_0`, and
  `cs_5_0` controls.
- DX12 core regression: 89/89; selected DX11 adapter enumeration test: 1/1;
  wgpu-types gather-limit test: 1/1.
- Core Clippy passed without warnings. HAL Clippy passed with nine pre-existing
  test-code warnings in unrelated files. The wgpu-types test emitted 14
  pre-existing unused serde-import warnings because serde was not enabled.
  Changed Rust files passed rustfmt and `git diff --check` passed.

The first compile of the compute gate found a moved `ShaderStageForValidation`
use after `check_stage`; the implementation now uses the fixed Naga and wgpu
compute-stage constants after that consuming call. No test failures remain.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl texture_gather_limits_follow_selected_entry_and_helpers -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl explicit_texture_sample_level_limits_follow_selected_entry_and_helpers -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib texture_gather -- --nocapture
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-types --lib texture_gather_is_a_downlevel_capability
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11,std,wgsl --tests
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --tests
rustfmt +1.93 --edition 2021 --check wgpu-core/src/device/resource.rs wgpu-core/src/pipeline.rs wgpu-core/src/validation.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/shader.rs wgpu-types/src/limits.rs
git diff --check
```

Independent review identified the next shader-contract probe: `@interpolate(sample)`
may emit a shader-model-4.1-only HLSL modifier, while the FL9_1 fixture omits
`DownlevelFlags::MULTISAMPLED_SHADING`. Confirm the exact FXC profile boundary,
then add a selected-entry core gate only if the profile evidence supports it.
Historical FL9_1 devices and ARM32 UWP remain unverified.

## Follow-up — fragment interpolation contract — `89a2dbbd8`

FXC accepts vertex `flat`, `linear` and `centroid` outputs under the tested
FL9_1 profile, but rejects fragment `flat` (`nointerpolation`) and `linear`
(`noperspective`) inputs with X4568. Fragment `centroid` and perspective
compile; a strict public draw/readback compares all 64 pixels. Core now has a
downlevel limit for this selected-fragment-entry restriction. It does not
reject vertex outputs or centroid sampling. Sample-qualified interpolation
continues to be rejected through Naga's existing multisampled-shading gate.

Verification on 2026-11-06 used pinned Rust 1.93.1 and the Visual Studio
developer environment. The strict public suite passed 39/39 and the private
HAL suite passed 363/363 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), hardware=true, requested/actual FL9_1. The optional
Float16x2 vertex-input branch visibly skipped because the selected device
does not report the required IA format support. FXC profile tests passed;
core selected-entry analysis, wgpu-types limit/serde compatibility tests and
the 90-test DX12 core regression passed. Clippy passed for core, HAL, public
wgpu tests, wgpu-types and wgpu-info; HAL reported nine existing warnings in
unrelated tests. Changed Rust files passed rustfmt and `git diff --check`.

## Follow-up — execute structured fragment depth — `fa9444968`

The previously compilation-only structured color-plus-`frag_depth` subset now
has a strict public execution test. The test draws a red triangle with vertex
depth 0.8 and fragment depth 0.2, followed by green with vertex depth 0.2 and
fragment depth 0.8 using `LessEqual` and depth writes. The RTX forced to FL9_1
produced red in all 64 pixels, proving `SV_Depth` controls depth testing and
the second draw fails. The test reports the selected physical adapter and
requested/actual level. Full strict public regression passed 40/40; the
optional Float16x2 branch was the single visible capability skip. Targeted
public Clippy, rustfmt and diff checks passed. Historical FL9_1 driver behavior
remains unverified.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:RUST_MIN_STACK = '16777216'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib interpolation_modifiers_require_shader_model_4 -- --nocapture
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl fragment_non_perspective_interpolation_follows_selected_entry
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_interpolation_profile_boundaries_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_depth_controls_depth_test_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-types --lib --features serde fragment_non_perspective_interpolation_is_a_downlevel_capability
cargo +1.93 check --offline -j8 -p wgpu-info
cargo +1.93 check --offline -j8 -p wgpu-hal --no-default-features --features gles
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11,std,wgsl --tests
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --tests
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
cargo +1.93 clippy --offline -j8 -p wgpu-types --features serde --tests
cargo +1.93 clippy --offline -j8 -p wgpu-info
rustfmt +1.93 --edition 2021 --check <changed Rust files>
git diff --check
```

The first full private HAL run showed intermittent DXGI unsupported errors and
a stack overflow in tests that passed when isolated. Retrying the full serial
suite with `RUST_MIN_STACK=16777216` passed 363/363. This was an environment or
test-isolation issue, not attributed to the interpolation change.

The latest contract audit found no confirmed unguarded shader-profile
mismatch. The next concrete verification gap was the accepted fragment-depth
path, now covered by the test above. Continue auditing core-legal D3D11
operations, optional format/usage reports and presentation lifecycle. The
backend remains opt-in and experimental, and historical FL9_1 hardware plus
ARM32 UWP remain unverified.

## Follow-up — public vertex system-value rejection — `f7579b68e`

Added a strict public wgpu validation test for `vertex_index` and
`instance_index`. On the selected physical FL9_1 adapter, core returns the
specific unsupported-input validation error for each selected pipeline before
the D3D11 HAL is reached, while a safe sibling entry compiles. This strengthens
the public contract test for already-gated backend limitations; it does not
implement either system value. The capability documentation now states this
public evidence and clarifies that the only exposed surface path is the
constrained HWND contract, correcting older checkpoint wording.

On 2026-11-06 the focused strict test passed 1/1, and the full strict public
suite passed 41/41 on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), hardware=true, requested/actual FL9_1. The optional
Float16x2 vertex input branch was the one visible skip. Public Clippy passed
with `-j1`, rustfmt and `git diff --check` passed. A first `-j8` rebuild hit
Windows paging-file error 1455 while archiving `wgpu-hal`; rerunning at `-j2`
completed and the strict test/full suite passed. No system memory or page-file
settings were changed.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_vertex_system_values_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

An independent read-only core/HAL audit found no confirmed advertised path that
is accepted by public core but would invoke an unimplemented D3D11 operation.
The HAL panic stubs inspected are paired with creation failures or public
feature/downlevel gates. The audit confirms `vertex_index` remains a real
ordinary-feature gap, but the current FL9_1 profile rejects its direct system
value; supporting it needs a deliberate emulation design, not a validation
change. Next, continue with the depth/stencil, view and selected-device
format/usage matrix, and preserve the current conservative projection.

## Follow-up — public render-bundle execution

Added a strict public wgpu test that records a pipeline, vertex buffer and
nonindexed triangle draw in a `RenderBundleEncoder`, executes it in a normal
color pass, submits and validates a complete 8×8 readback. The test is limited
to this basic bundle shape and does not establish indexed bundle draws or all
render-bundle state transitions.

The focused test passed 1/1 and printed the physical NVIDIA GeForce RTX 3090
Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1,
software=false. All 64 pixels were `[255, 64, 0, 255]`. The full strict public
suite passed 42/42; the optional Float16x2 IA case visibly skipped because the
selected FL9_1 device did not report the required format support. Public-test
Clippy, rustfmt and `git diff --check` passed.

Commands:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_executes_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

The first direct PowerShell rerun failed before linking because `link.exe` was
not on that shell's PATH. Re-running from the installed Visual Studio x64
developer environment passed. Code and test checkpoint: `d6b039960`
(`dx11: verify FL9_1 render bundle execution`). Continue the core-valid API
versus HAL audit; historical FL9_1 devices and ARM32 UWP remain unverified.

## Follow-up — indexed textured render-bundle retention

Added a second strict public bundle test with a Uint16 indexed triangle and a
sparse-bound texture/sampler pair. After finishing the bundle, the test drops
the original texture, view, sampler, bind group, vertex buffer and index
buffer handles. The submitted bundle still renders all 64 pixels as
`[51, 128, 204, 255]`, exercising actual core/HAL resource retention rather
than only bundle creation.

The first execution attempt failed at public validation because the test
uploaded a 6-byte index array with `Queue::write_buffer`, violating the 4-byte
copy alignment. Padding the backing array to 8 bytes while drawing only the
first three indices fixed the test. The focused strict run then passed 1/1 on
the NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested and
actual FL9_1, software=false. The full strict public suite passed 43/43; the
optional Float16x2 IA case remained the one visible hardware-capability skip.
Public-test Clippy with `-j16`, rustfmt and `git diff --check` passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_textured_render_bundle_retains_resources_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

## Follow-up — query-gated narrow-format blending and indexed bundles

Code checkpoint: `3774c7a2e` (`dx11: add query-gated FL9_1 narrow-format blending`).

Extended selected-device capability projection and native pipeline format
validation so R8_UNORM, R8G8_UNORM and B5G6R5 expose `BLENDABLE` only when the
actual format query reports it and the backend attachment path is implemented.
The public test now overlays half-alpha red with matching color and alpha
equations, retaining the FL9_1 separate-alpha restriction. It also keeps the
indexed textured bundle retention test from the preceding uncommitted change.

On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`),
requested and actual FL9_1, all three optional format queries were positive.
The strict public test read back all 64 pixels per format: R8 was
`[160, 0, 0, 255]`, RG8 `[160, 65, 0, 255]`, and RGB565 `[165, 65, 66, 255]`
after format quantization. The expected RGB565 code was `[164, 65, 66, 255]`
within the four-code packing tolerance. Synthetic capability cases verify
blendability is absent on unavailable/negative queries and present only on a
positive query. A device without this optional bit still executes the base
attachment test and reports the blending subcase as skipped.

Verification:

- Strict private HAL: 363/363 passed, including the synthetic format-query
  cases and physical GPU tests. The run used `RUST_MIN_STACK=16777216` to keep
  previously stack-sensitive full-suite cases isolated; selected hardware was
  the physical RTX 3090 Ti at requested/actual FL9_1.
- Focused strict public narrow-format test: 1/1 passed and executed actual
  blend draws/readback for all three formats.
- Full strict public D3D11 suite: 43/43 passed. The optional Float16x2 vertex
  input branch visibly skipped because this device does not report
  `IA_VERTEX_BUFFER` for R16G16_FLOAT; it was not counted as GPU capability
  coverage.
- HAL and public D3D11 test Clippy passed. HAL Clippy reports nine existing
  warnings in unrelated `hal_device.rs`, `swapchain.rs` and
  `tests/dx11/fl9_texel_load.rs` lines. Rustfmt and `git diff --check` passed.

Commands from an x64 Visual Studio developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:RUST_MIN_STACK = '16777216'
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_rg8_rgb565_render_attachments_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu/tests/dx11_public.rs
git diff --check
```

The selected-query capability is optional, not an FL9_1 guarantee, and the
RTX result does not establish support on older FL9_1 hardware. Continue the
public core/HAL operation audit; historical devices and ARM32 UWP remain
unverified.

## Follow-up — expand the FL9_1 dynamic uniform layout limit

The selected-device projection previously allowed only two dynamic uniform
buffers per pipeline layout even though the backend already allocates thirteen
constant-buffer slots and reports six uniform buffers per shader stage. The
projection now derives the per-stage limit once and allows twelve dynamic
uniform bindings per layout, the maximum six-vertex/six-fragment split. The
HAL's thirteen-resource bound and per-stage limits are unchanged.

The new public test uses sparse logical bindings, declared and supplied in
different orders, with six vertex-visible and six fragment-visible uniforms.
All ranges share one buffer but use distinct base offsets and nonzero dynamic
offsets. Two scissored draws switch between the selected and alternate ranges.
At forced FL9_1 on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), every left pixel matched `[60, 72, 84, 96]` and every right
pixel matched `[239, 203, 215, 227]` (64/64 pixels). Core rejects thirteen
dynamic entries against the new limit of twelve before HAL.

The first focused render run exposed an incorrect expected-channel rotation in
the alternate-offset oracle; the observed image was consistent with the data
selected by the shader, and the corrected strict test passed. The first full
core-fixture run also exposed an older format assertion that omitted the
already query-gated `COLOR_ATTACHMENT_BLEND` bit; the fixture now checks that
bit alongside the other projected format capabilities and the complete rerun
passed. Neither was an implementation failure.

The capability baseline diagnostic also queried RG8_SNORM (`0x00021320`) and
RGBA8_SNORM (`0x000213a0`) on the selected FL9_1 device. Both project as
sampled, filterable, copy-destination and single-mip only; neither query
contains `RENDER_TARGET`, so these formats provide no additional attachment
path on this adapter.

Verification, Rust 1.93.1 and Visual Studio x64 environment:

- Focused capability projection baseline passed 1/1 on the physical adapter.
- Strict public dynamic-uniform test passed 1/1 with the expected two-half
  image and a 13-entry validation rejection.
- Full strict private HAL suite passed 363/363. Six shader-boundary cases
  intentionally reported that their GPU subcase was skipped after the
  unsupported runtime-integer shader was rejected; those tests validate the
  rejection path, not rendering.
- Full strict public D3D11 suite passed 44/44. One optional Float16x2 vertex
  input branch skipped because the selected device lacks `IA_VERTEX_BUFFER`
  support for R16G16_FLOAT.
- Full strict core fixture suite passed 94/94, including selected-device
  projection checks and core-to-HAL rendering. A prior run before the fixture
  assertion update had 93 passes and the single stale-format assertion failure.
- Core DX12 regression suite passed 90/90.
- HAL Clippy passed with nine existing warnings in unrelated HAL/device,
  swapchain and texel-load test code. Core fixture Clippy passed with one
  existing `collapsible_if` warning. Public D3D11 Clippy passed without
  warnings.

Commands:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_uses_twelve_stage_disjoint_dynamic_uniforms_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j16 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib --tests
cargo +1.93 clippy --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-core/src/dx11_fixture_tests.rs wgpu/tests/dx11_public.rs
git diff --check
```

Code checkpoint `bd718635f` (`dx11: expose twelve FL9_1 dynamic uniform
bindings`) contains the verified limit expansion, public GPU test, fixture
correction, capability documentation and changelog entry. The tree was clean
after that commit. Continue the broader core/HAL contract audit; the public
backend remains experimental, and historical FL9_1 devices and ARM32 UWP
remain unverified.

## Follow-up — optional FL9_1 RGBA8 multisample resolve

The public capability projection and private HAL now allow one narrowly
scoped 4x RGBA8 render-target path when the selected device positively reports
both a nonzero `CheckMultisampleQualityLevels(RGBA8_UNORM, 4)` result and the
required texture, render-target, multisample-render-target and resolve format
bits. The 4x resource is single-mip, single-layer, color-target-only (it may
also be transient); it resolves to a same-sized, single-sample RGBA8 target.
FL9_1 does not guarantee this support, so other devices continue to see the
capability as unavailable. Multisampled sampling, depth, other counts/formats
and alpha-to-coverage remain unsupported.

On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), explicitly requested and actual FL9_1, the
quality-level count was 1 and RGBA8 `CheckFormatSupport` returned `0x312ef3e2`.
The strict public test rasterized a triangle into a transient 8x8 4x target,
resolved it, and checked all 64 pixels: 28 full red coverage, 8 partial
red/blue coverage and 28 blue clear pixels. A second zero-sample-mask draw
resolved all 64 exact blue clear pixels, verifying the FL9_1 nonzero native
sample-mask workaround for 4x targets.

Verification used Rust 1.93.1 through the installed `+1.93` toolchain alias,
Visual Studio x64 tools and bounded Cargo parallelism:

- D3D11 HAL `cargo check`: passed.
- Strict private D3D11 HAL suite: 364/364 passed on the selected physical
  device; unsupported shader subcases remain intentionally rejected.
- Strict public D3D11 suite: 45/45 passed on hardware, including the new
  4x render/resolve test. One optional Float16x2 vertex-input subcase was
  skipped because the selected format was not reported for vertex buffers.
- Strict core fixture suite: 94/94 passed, including public adapter creation,
  selected-device format projection and rendering.
- Core DX12 regression suite: 90/90 passed.
- Strict 4x public test: 1/1 passed with the pixel breakdown above.
- Targeted core fixture: passed after updating the expected unsupported
  sample-count error from 4x to 2x.

The initial test runs exposed stale tests that treated all multisampling as
unsupported and a core capability comparison that did not mask the optional
X4/resolve flags; the assertions now distinguish 2x rejection and optional
selected-device evidence. This modern RTX result does not establish behavior
on historical FL9_1 drivers. Continue the broader public core/HAL contract
audit; do not broaden the optional capability without matching runtime
queries and operation coverage.

## Follow-up — expose the FL9_1 vertex uniform range

The public uniform binding-size projection previously stopped at 512 bytes,
although the selected FL9_1 vertex profile and private HAL already supported
255 float4 constants (4,080 bytes). The limit now allows 4,080 bytes; selected
entry-point validation still limits fragment uniform footprints to 32 vectors
and vertex footprints to 255.

The strict public test binds a 255-vector vertex uniform at sparse binding 7
and a 32-vector fragment uniform at binding 42. On the physical NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), at
requested/actual FL9_1, c254 shifted geometry between two draws; readback
contained 17 green baseline pixels and 4 red shifted pixels. The same test
confirmed core rejects a 256-vector vertex pipeline before HAL. The existing
fragment-boundary test still rendered all 64 pixels from 32 vectors and
rejected 33 before HAL.

Verification with Rust 1.93.1 (`+1.93` alias), Visual Studio x64 tools and
bounded Cargo jobs:

- Strict new vertex-uniform test: 1/1, real GPU readback.
- Strict fragment aggregate test: 1/1, real GPU readback and 33-vector reject.
- Full strict public D3D11 suite: 46/46, physical RTX at requested/actual
  FL9_1. This includes all prior coverage and the 255-vector case.
- Strict core fixture suite: 94/94.
- HAL capability projection unit test: 1/1.
- Public D3D11 Clippy passed without warnings. HAL Clippy passed with its nine
  existing warnings in unrelated mapped-range tests, swapchain code and the
  isolated texel-load experiment.

The driver-specific result validates the current RTX path, not all historical
FL9_1 implementations. The full HAL suite and core DX12 regression were
already green at the prior multisample checkpoint; they are unaffected by
this selected-device uniform-limit update. Continue the broader public
core/HAL contract audit.

## Follow-up — fallible public mapped-buffer readback

The independent synchronization/lifetime audit found that `read_byte_buffer`
used `vec![0; len]` after mapping a D3D11 staging buffer. A large public
`MAP_READ` invalidation could therefore panic on allocation failure, and an
unwind could leave the staging resource mapped. The readback vector now uses
`try_reserve_exact` before staging creation or `Map`; failure becomes
`E_OUTOFMEMORY`, which the public HAL path classifies as
`DeviceError::OutOfMemory`. A CPU-only test supplies `usize::MAX` to exercise
the allocator capacity-overflow path deterministically.

Strict verification on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and actual FL9_1:

- Full private D3D11 HAL suite: 365/365 passed. It exercised real GPU upload,
  mapping, copy, rendering, synchronization and readback tests. Several
  shader-negative tests explicitly report that their unsupported rendering
  subcases were skipped after the intended validation rejection; they do not
  count as GPU feature coverage.
- Full strict public D3D11 test binary: 46/46 passed and executed on the
  physical adapter. One optional Float16x2 vertex-input subcase reported a
  capability skip because the selected device lacks `IA_VERTEX_BUFFER` for
  that format.
- HAL Clippy passed with nine existing warnings in unrelated mapped-range
  tests, swapchain code and the isolated texel-load experiment; no new warning
  was introduced. The changed file passed rustfmt and `git diff --check`.

Commands (Rust 1.93.1 via the `+1.93` alias and Visual Studio x64 tools):

```powershell
cmd.exe /d /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set WGPU_DX11_REQUIRE_HARDWARE=1 && set RUST_MIN_STACK=16777216 && cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1'
cmd.exe /d /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1'
cmd.exe /d /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 >nul && cargo +1.93 clippy --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib --tests'
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/buffer.rs
git diff --check
```

The parallel core/shader review found no unguarded selected-entry shader or
pipeline case in its audit. It confirmed a compiler boundary still missing a
core validation rule: FL9_1 FXC compiles up to 16 distinct sampled
texture/sampler relationships and rejects 17, while public limits count
textures and samplers independently. The public adapter therefore permits a
pipeline that fails later during backend compilation. The next increment will
add a narrow per-stage downlevel limit and selected-entry core test without
lowering either independent resource count.

The synchronization reviewer also identified a conditional HWND reentrancy
risk: `unconfigure` waits for an active surface operation, so a synchronous
window callback reentering it on the same thread could self-deadlock. Normal
resize/present and explicit operation-guard checks pass; callback delivery at
this point is not reproduced. This remains an open presentation lifecycle
item. Historical FL9_1 and ARM32 CoreWindow execution remain unverified.

## Follow-up — selected-entry sampled relationship limit

Code checkpoint: `0ad9a06f0` (`dx11: enforce FL9_1 sampled pair pipeline limit`).

The core/HAL contract audit confirmed that the private FL9_1 compiler path
supports sixteen distinct texture/sampler relationships in one shader stage,
while FXC rejects seventeen. The existing independent public limits of eight
textures and sixteen samplers did not express this cross-resource compiler
boundary, so a core-valid pipeline could fail later during D3D11 pipeline
creation. Added
`DownlevelLimits::max_sampled_texture_sampler_pairs_per_shader_stage`, with a
conservative default of `u32::MAX` for other backends, and projected the tested
FL9_1 value of 16. Render and compute pipeline creation count distinct pairs
from Naga's validated selected-entry `sampling_set`; over-limit stages return
a validation error before HAL. The existing texture and sampler binding limits
are unchanged. The public test compiles the 16-pair entry, accepts its one-pair
sibling and rejects the 17-pair entry in the same WGSL module.

The preceding mapped-readback allocation fix is committed as
`d4182610f4e63db3e3de2e65acea0103c27fb93c`.

Verification on Rust 1.93.1 with the Visual Studio x64 environment and Cargo
`-j16`:

- `cargo +1.93 test --offline -j16 -p wgpu-types`: 83 unit tests and 14
  doctests passed; the focused numeric-limit test passed 1/1 and the
  serde-compatibility test passed 1/1.
- Strict private D3D11 HAL suite: 365/365 passed. The existing
  `multi_resource_sixteen_sampler_relationships` test executed GPU rendering
  and readback at forced FL9_1.
- Strict public D3D11 suite: 47/47 passed on the physical RTX 3090 Ti at
  requested/actual FL9_1. The new test compiled the 16-pair shader and
  one-pair sibling, then confirmed core rejected 17 before HAL. One unrelated
  optional Float16x2 input subcase visibly skipped because the adapter did not
  report `IA_VERTEX_BUFFER` for that format.
- Strict controlled D3D11 core fixture suite: 95/95 passed, including physical
  adapter selection and core resource/command execution.
- DX12 core regression suite: 91/91 passed.
- `cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features
  --features dx11,gles` passed, covering the shared downlevel type and GLES
  adapter literal.
- Core Clippy passed with one existing `collapsible_if` warning in the
  fixture. HAL Clippy passed with nine existing unrelated warnings. Public
  D3D11-test and serde-enabled `wgpu-types` Clippy passed without warnings.
- Changed-file rustfmt and `git diff --check` passed. Git emitted only its
  existing LF-to-CRLF working-copy notices.

Hardware identity: NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver
`32.0.16.2101`, hardware adapter (`software=false`), explicitly requested and
selected FL9_1. The development driver reports unrestricted NPOT, so the
conditional-NPOT negative path remains covered by synthetic tests rather than
this hardware run.

Next continue the selected-entry shader/compiler and render-pass contract
audit. Public exposure remains experimental; historical FL9_1 drivers and
ARM32 UWP are still unverified.

## Follow-up — same-thread surface unconfigure reentry

Code checkpoint: `94741d7ef` (`dx11: avoid reentrant surface unconfigure deadlock`).

The surface reservation records its owning Rust thread. Cross-thread
`unconfigure` waits for an active operation as before; same-thread reentry now
logs and returns immediately, leaving the configured surface available for a
retry after the operation completes. CPU tests cover both the reentrant
fail-fast path and cross-thread wait/release behavior. This does not establish
that DXGI delivers a synchronous window callback during surface operations.

Verification used Rust 1.93.1 and the Visual Studio x64 environment:

- Strict private D3D11 HAL suite: 366/366 passed. The suite exercised real
  FL9_1 rendering and readback on the NVIDIA GeForce RTX 3090 Ti
  (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested and
  actual FL9_1. The added reservation unit case itself is CPU-only.
- Focused `unconfigure_` tests after the final source edit: 2/2 passed.
- Strict public HWND present/resize test: 1/1 passed on the same physical
  adapter at requested/actual FL9_1.
- HAL Clippy passed with nine existing warnings outside the changed surface
  code. Changed-file rustfmt and `git diff --check` passed.

Next close the independently identified public core/HAL mismatch for
alpha-to-coverage on query-supported 4x RGBA8 render targets, then continue the
selected-entry WGSL/FXC shader audit. Historical FL9_1 drivers, synchronous
callback delivery and CoreWindow/ARM32 execution remain unverified.

## Follow-up — explicit FL9_1 alpha-to-coverage limit

The optional query-backed four-sample RGBA8 render-and-resolve path did not
mean alpha-to-coverage was available. `DownlevelLimits::alpha_to_coverage`
now represents that separate contract; its full-capability default preserves
other backends, while the selected D3D11 FL9_1 projection reports `false`.
Core rejects an alpha-to-coverage pipeline after the required multisample-state
check and before HAL. The controlled fixture tests both single-sample and
multisampled rejection. No native alpha-to-coverage execution is claimed.

Verification on Rust 1.93.1 with the Visual Studio x64 environment used the
separate temporary Cargo target `E:\CodexTemp\wgpu-fl91-mission-target`:

- Strict `fl91_core_buffer_and_offscreen_render_on_hardware`: 1/1 passed on
  the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
  hardware adapter), requested and actual FL9_1. It logged the expected core
  rejection and completed the fixture's GPU draws, transfers, synchronization
  and readbacks.
- `requesting_dx11_exposes_only_fl91_hardware_adapters`: 1/1 passed and
  enumerated two physical FL9_1 D3D11 adapters through the explicit opt-in
  backend path.
- `cargo +1.93 test --offline -j8 -p wgpu-types`: 84 unit tests and 14
  doctests passed; 21 doctests were intentionally ignored. The no-serde build
  emitted the existing unused serde-import warnings.
- Earlier attempts using the repository-local target hit MSVC PDB limits and
  then exhausted C: during a debug-profile rebuild. Only outputs created by
  those failed commands were removed; all source changes were preserved. The
  successful test commands used the E: temporary target.

Next close the selected-entry dynamic-loop/FXC mismatch and the mapped
`BufferView` destruction lifetime issue found in the continuing core audit.
Historical FL9_1 behavior and ARM32 UWP remain unverified.

## Follow-up — preserve mapped views during buffer destruction

A safe public `BufferView` owns its API `Buffer` handle but its core backend
mapping stores a raw pointer into the mapped allocation. Core destruction
unmaps before destroying native resources, so calling `Buffer::destroy` while
a mapped view was alive could invalidate that pointer while the view remained
readable in safe Rust. `Buffer::destroy` now runs the existing `MapContext`
precondition before dispatching destruction. `MapContext::reset` checks for
live subranges before clearing `mapped_range`, preserving a usable mapping if
the panic is caught. The API docs now state the panic and require dropping
views before retrying.

The strict public test maps a 16-byte buffer at creation, opens an 8-byte view,
catches the expected destroy panic, then opens a disjoint aligned 8-byte view.
Both byte sequences remained intact; after dropping both views, destruction
succeeded. A first draft used a 4-byte second offset and was rejected by the
D3D11 public mapping alignment requirement; the final test uses aligned ranges.
This is real D3D11 mapping/lifetime coverage, not a GPU draw or submission.

Verification on Rust 1.93.1 and the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested/actual FL9_1:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_buffer_destroy_preserves_live_mapped_views -- --nocapture --test-threads=1`: 1/1 passed.
- Changed-file `rustfmt --check` passed. Final Clippy and diff checks are pending.

Next continue closing the selected-entry dynamic-loop/FXC compiler gap before
broader shader and render-state audits.
## Follow-up — selected-entry WGSL loop restriction — `62d12ecd4`

The core/HAL shader audit found that `wgpu-core` accepted WGSL loops that the
current D3D11 FL9_1 shader path could not compile. The HLSL writer uses
`force_loop_bounding`, emitting dynamic `while (true)` control with a
decrementing loop guard. Microsoft's shader-profile table lists static flow
control (depth 1) for `vs_4_0_level_9_1`, but does not list loop control for
`ps_4_0_level_9_1`; the current backend has no verified semantics-preserving
static-loop lowering. FXC confirmed rejection of the generated vertex and
fragment shaders, with X3552/X3511 for vertex and X3511 for fragment. This is
a current compiler-path limit, not a claim that all FL9_1 hardware lacks
static vertex loops.

Added `DownlevelLimits::shader_loops`, defaulting to true for existing
backends and old serialized capability reports. The D3D11 implementation
reports false. `HardwareGuarantees` now records static/dynamic flow-control
guarantees for vertex and fragment profiles across all seven recognized
feature levels, separately from backend implementation. Core walks Naga IR
from the selected vertex and fragment entries through nested blocks and
reachable helper calls, then rejects only a selected stage that reaches a
loop before HAL. Safe sibling entries remain valid. A controlled core fixture,
public D3D11 validation test, HAL/FXC diagnostic test, synthetic relationship
analysis test and serde-compatibility test cover the path.

Verification used Rust 1.93.1, Visual Studio x64 and
`CARGO_TARGET_DIR=E:\CodexTemp\wgpu-fl91-mission-target`:

- Strict private D3D11 HAL suite: 366/366 passed. The hardware/compiler test
  forced the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
  `32.0.16.2101`) to requested/actual FL9_1 (`software=false`); FXC emitted the
  expected loop rejection diagnostics and compiled the safe sibling.
- Strict D3D11 core fixture suite: 96/96 passed. The selected vertex and
  fragment loops were rejected before HAL; the remaining fixture executed its
  GPU operations and readbacks on the same forced-FL9_1 hardware.
- Strict public D3D11 suite: 49/49 passed on the same adapter and level. The
  selected-entry public rejection test passed. One optional Float16x2 input
  format branch was explicitly skipped because the device query did not
  advertise `IA_VERTEX_BUFFER` support.
- `wgpu-types --lib`: 85/85 passed; serde compatibility: 1/1 passed. The
  no-serde test build emitted 14 existing unused serde-import warnings.
- DX12 core regression: 92/92 passed.
- `cargo check -p wgpu-hal --no-default-features --features dx11,gles`,
  `cargo check -p wgpu-info` and `cargo check -p wgpu-test --tests` passed.
  The test-package check exposed a stale explicit `DownlevelLimits` literal
  missing the earlier sampled-pair limit; that lowest-capability fixture now
  sets the missing limit to zero and compiles.
- Core, HAL, public-test and serde-enabled `wgpu-types` Clippy passed. Core
  reports one existing `collapsible_if` warning in the fixture; HAL reports
  nine previously recorded warnings unrelated to this change. The other two
  Clippy runs were warning-free.
- Rustfmt 1.93.1 `--edition 2021 --check` for changed Rust files and
  `git diff --check` passed. Git reports its existing LF-to-CRLF working-copy
  notices.

Exact strict commands are in `TESTING.md`. The hardware evidence applies to a
modern RTX 3090 Ti running at FL9_1, not to historical 9_1 drivers. Public
D3D11 enumeration remains opt-in. Continue the core/HAL shader audit; this
checkpoint does not complete the mission.

## Follow-up — verify `instance_index` at the FL9_1 compiler boundary

The existing core gate and private HAL rejection for `instance_index` were
already correct, but the FXC profile behavior was undocumented. Added a
production compiler-path test covering direct and structure-member inputs,
used and unused declarations, a safe sibling, and a `vs_5_0` control. At
`vs_4_0_level_9_1`, FXC rejects all four generated `SV_InstanceID` inputs with
X4016: `SV_InstanceID semantic cannot be used with 10Level9 targets`. The
safe sibling and SM5 control compile. This justifies the FL9_1 selected-entry
core gate for the tested profile; it is not a universal hardware claim.

Verification used Rust 1.93.1 and the Visual Studio x64 environment:

- `cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features
  --features dx11 --lib
  fl91_instance_index_entry_inputs_are_rejected_by_fxc -- --nocapture
  --test-threads=1`: 1/1 passed. This is an FXC compiler test, not GPU draw
  execution.
- The first `-j8` invocation hit a Rust compiler process allocation failure
  before completing. Retrying with `-j2` succeeded; no source change was
  needed for the retry.

No runtime behavior or advertised capability changed. This CPU-only compiler
test does not require the strict hardware environment. Historical hardware
behavior remains unverified. Continue with the next selected-entry WGSL/FXC-
to-core mismatch.

## Follow-up — indexed strip restart inside a public render bundle (`3f0e6c41fe35ea46ad3074e9fab62b9e7d7af8cd`)

Added a strict public wgpu test that records Uint16 and Uint32 TriangleStrip
draws in separate `RenderBundleEncoder`s, each with a primitive-restart marker
between a left rectangle and a separate right triangle. The bundles execute
through color render passes on the physical RTX 3090 Ti explicitly requested
and confirmed at FL9_1 (`0x10de:0x2203`, driver `32.0.16.2101`, software=false).
Each 16×16 target readback matched the CPU reference at 256/256 pixels; 107
pixels were covered for each index format and the clear gap remained
unchanged. This exercises public core bundle recording, command replay,
restart splitting, submission and readback in both index-width paths.

Verification command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_triangle_strip_restart_render_bundle_at_fl91 -- --nocapture --test-threads=1
```

An initial CPU reference placed one pixel center exactly on the triangle edge;
the hardware's top-left rasterization rule covered that pixel, so the test
geometry was shifted slightly to keep sample centers off boundaries. The
focused strict run passed with 512/512 expected pixels across both index
formats, and the full strict public suite passed 50/50 with the same explicit
optional Float16x2 input-format capability skip. Continue with other
render-bundle state transitions and the broader core/HAL audit.

The full-suite command was:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The public-test Clippy command (`cargo +1.93 clippy --offline -j2 -p wgpu
--no-default-features --features dx11,wgsl,std --test dx11_public`), changed
file Rustfmt check and `git diff --check` also passed.

## Follow-up — same-pass render-bundle pipeline transition (`f98b7220089e3836ecaea883f25927ab5aa57835`)

Expanded the public restart test to execute a Uint16 green bundle followed by
a Uint32 red bundle in the same render pass. Both use a restart-separated
strip; their selected vertex shaders transform the shapes into disjoint target
regions. This checks that replay switches the index width, vertex and fragment
pipeline state and index resource without carrying stale state into the next
bundle. Readback matched all 256 pixels: 37 green, 37 red, and the expected
black background and gap. The test does not attempt to change viewport or
scissor from a bundle because those dynamic commands are not part of the
public `RenderBundleEncoder` API.

The focused strict test and full strict public D3D11 suite both passed on the
RTX 3090 Ti at requested/actual FL9_1; the suite result was 50/50 with one
optional `Float16x2` input-format capability subcase explicitly skipped.
Public-test Clippy, Rustfmt 1.93.1 and `git diff --check` passed. The exact
commands are the focused and full-suite commands recorded above.

## Follow-up — sampled resources across three public bind groups (`db3f95c7cd2dc8f8b692b423ee0e36d61d2143e7`)

Expanded the public FL9_1 textured integration to put the vertex transform and
fragment tint in group 0, the filtering sampler in group 4, and the sampled
texture at group 7. The shader uses the texture/sampler pair across two groups,
and logical binding numbers are reused across group identities. A later draw
replaces only the group-7 texture while leaving groups 0 and 4 bound. On the
RTX 3090 Ti requested/actual FL9_1, readback changed to 32 blue and 32
tinted-yellow pixels, confirming that the replacement texture and retained
sampler/uniform groups were used together.

The focused public test passed, and the complete strict public D3D11 test
binary passed 50/50; one optional Float16x2 input-format branch remains an
explicit capability skip. The public-test Clippy command, Rustfmt 1.93.1 and
`git diff --check` passed.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_api_renders_offscreen_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
```

## Checkpoint — independent sampled pairs across sparse public groups (`b89865ca5`)

Extended `public_dx11_sparse_multi_texture_sampling_at_fl91` from two textures
sharing one sampler in group 0 to two independently sampled texture/sampler
pairs across groups 1/4 and 6/7. The WGSL declarations use sparse bindings
7, 100, 42 and 17 in an order different from group and layout order. Patterned
RGBA8 inputs are sampled at a coordinate where nearest and linear filtering
produce different results. Forced-FL9_1 readback checked all 64 pixels against
`[40, 43±1, 0, 255]`, exercising each pair and its sampler register through
public wgpu/core/HAL.

A Repeat-addressing version of the probe was rejected during public sampler
creation because core cannot prove that interpolated coordinates remain under
the FL9_1 `max_texture_repeat` bound. This is expected capability validation;
no limit or validation rule was weakened. Repeat remains covered only by
bounded private-HAL tests.

Verification on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), requested/actual FL9_1:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sparse_multi_texture_sampling_at_fl91 -- --nocapture --test-threads=1`: 1/1 passed; all 64 pixels matched.
- The complete strict public D3D11 test binary passed 50/50; the optional Float16x2 input-format subcase explicitly skipped because the selected FL9_1 device did not report `IA_VERTEX_BUFFER` support for `R16G16_FLOAT`.
- Public-test Clippy and Rustfmt passed. `git diff --check` passed.

The group-7 sampler transition was implemented and verified in the follow-up
checkpoint below. Continue the full selected-entry/core/HAL audit; this
checkpoint does not complete the mission.

## Follow-up — FL9_1 constant texture-sampling offsets (9f1617eed)

Rechecked the WGSL/Naga sampling representation instead of adding a new
capability for a nonexistent textureSampleOffset builtin. WGSL 30.0.1
represents the constant texel offset as the final argument of
textureSampleBias. The existing texture_sample_bias_offset limit already
rejects the operation in core, and the private HAL independently rejects it
before shader compilation. The coverage now verifies direct and helper-reached
offset use at all three validation boundaries.

The new Naga-to-FXC regression compiles the biased-offset shader against both
ps_4_0_level_9_1 and ps_4_0_level_9_3. FXC rejects each with X4532;
the SM5 control compiles. This is compiler-profile evidence, not evidence about
historical driver execution. No offset-sampling functionality or public
capability was added.

Verification on the RTX 3090 Ti (0x10de:0x2203, driver
32.0.16.2101, software=false), with the public/fixture hardware tests
requesting and confirming FL9_1:

- The new HAL FXC test passed 1/1 and asserted X4532 for both level-9 profiles
  plus successful SM5 compilation.
- Core's selected-entry bias test passed, including a helper that performs
  biased sampling with a constant offset.
- The focused strict public sampled-mip test passed 1/1. Core rejected direct
  and helper-reached offset pipelines before HAL; the supported implicit and
  literal-bias paths matched 256 pixels.
- The full strict private HAL suite passed 368/368. Expected unsupported
  shader subcases emitted explicit GPU-rendering skips after preflight
  rejection; they are not counted as GPU executions.
- The full strict public D3D11 test binary passed 50/50. One optional
  Float16x2 vertex-input subcase remained query-skipped.
- The strict test-only core fixture suite passed 96/96; the selected-device
  projection test also passed independently.
- The DX12-feature core suite passed 92/92.
- Core Clippy passed with one existing warning in the untouched fixture test
  at dx11_fixture_tests.rs:4186; HAL Clippy passed with nine existing
  warnings in untouched files; public-test Clippy passed without warnings.
  Rustfmt 1.93.1 and git diff --check passed.

Exact commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib level_9_profiles_reject_texture_sample_bias_offsets -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib texture_sample_bias_limits_follow_selected_entry_and_helper_calls
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sampled_mip_chain_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
~~~

Continue auditing other WGSL/core-accepted operations and state combinations
against the selected FL9_1 compiler and HAL. This checkpoint does not complete
the mission; historical FL9_1 drivers and ARM32 UWP remain unverified.

## Checkpoint — sparse sampler-group transition (`d921da527`)

Extended `public_dx11_sparse_multi_texture_sampling_at_fl91` to render two
adjacent viewport regions in one pass. The first uses the linear green sampler;
the test then replaces only group 7 with a nearest sampler and draws the second
region. Both texture groups, the red sampler and the pipeline remain unchanged.
Forced-FL9_1 readback checked all 128 pixels: the linear side matched
`[40, 43±1, 0, 255]`, and the nearest side matched `[40, 40, 0, 255]`.

Verification on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), requested/actual FL9_1:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sparse_multi_texture_sampling_at_fl91 -- --nocapture --test-threads=1`: 1/1 passed.
- The complete strict public D3D11 test binary passed 50/50; the optional Float16x2 input-format subcase explicitly skipped because the selected device did not report `IA_VERTEX_BUFFER` support for `R16G16_FLOAT`.
- Public-test Clippy, Rustfmt 1.93.1 and `git diff --check` passed without new warnings.

Continue with the remaining core/HAL contract audit, prioritizing any operation
the public projection accepts but the D3D11 path cannot execute correctly.

## Verified cube-texture increment — `680b048ed` — 2026-11-06

The opt-in public D3D11 path now supports single-mip, single-sample RGBA8 cube
resources with six uploaded faces and sampled cube views. The selected-device
format mask requires positive `TEXTURE2D | TEXTURECUBE` support, separate from
2D sampling. Core rejects non-square, NPOT, oversized, mipmapped,
multisampled, render-target, unsupported-format and unsupported-view shapes
before HAL. Ordinary 2D arrays and cube arrays remain disabled. Cube
texture-to-texture copies and render-target views are not implemented.

On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and actual FL9_1, the selected cube
format mask was `0x1f1f`; all six uploaded face colors were sampled correctly
across 96/96 readback pixels. RG8_UNORM cube creation was rejected by core
because the selected device's per-format cube query was negative.

Verification used Rust 1.93.1, Visual Studio 18 x64 and bounded Cargo `-j8`:

- Strict public cube test: 1/1 passed on physical FL9_1 hardware.
- Full strict public D3D11 suite: 51/51 passed. The optional Float16x2 input
  subcase logged a query-based skip.
- Full private D3D11 HAL suite: 370/370 passed, including strict GPU tests.
- Strict core fixture suite: 96/96 passed, including cube-layout and invalid
  single-face-view validation.
- DX12 core regression suite: 92/92 passed.
- Cube capability unit tests: 2/2 passed; `wgpu-types` cube-limit test:
  1/1 passed with and without serde.
- `cargo check -p wgpu-hal --all-features` and `cargo check -p wgpu-info`
  passed. Targeted Clippy passed; HAL retained pre-existing warnings in
  unchanged code. Changed-file rustfmt and `git diff --check` passed.

The first broad runs found stale array-layer and subresource-test expectations
plus one stale capability-flag assertion; those were updated to model six cube
faces independently of ordinary 2D arrays and to keep box bounds separate
from subresource validation. Repeated full suites passed after the corrections.

Exact strict commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib cube_ -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_cube_texture_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
~~~

Continue auditing core-valid shader, render-state and resource combinations.
This checkpoint does not complete the mission; historical FL9_1 drivers and
ARM32 UWP remain unverified.

## Verified resource checkpoint — query-gated FL9_1 cube mip chains — `75222323e` — 2026-11-06

Extended the opt-in public cube texture path from one mip to caller-populated
POT mip chains. FL9_1 shape validation now permits at most the full chain for
square POT dimensions. The HAL additionally requires its cube-mip path, a
supported sampled usage, positive selected-device `D3D11_FORMAT_SUPPORT_MIP`,
and a format not marked single-mip-only. Ordinary arrays, cube arrays,
render-target cube use, cube copies/readback and individual 2D face views stay
unavailable. Core no longer rejects cube mips in its special shape gate; its
existing max-mip and per-format gates remain active.

The selected FL9_1 RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false) reports RGBA8 mip support. The strict public test requested and
asserted FL9_1, uploaded distinguishable data for four mips across six faces,
rendered each mip using an integer minimum-LOD clamp, then sampled a
non-suffix view of mip levels 1 and 2. This exercised the six-face GPU clone
refresh path required by the 10Level9 lowest-LOD SRV restriction. Readback
matched all 960 pixels. The cube query mask remained `0x1f1f`; unsupported
RG8_UNORM cube creation was rejected by core. The RTX is modern hardware
forced to FL9_1, not a proxy for every historical 9_1 driver.

Verification on Rust 1.93.1 and Visual Studio 18 x64:

- HAL capability validation test: 1/1 passed, including full-chain bounds,
  non-POT shape rejection, backend-disabled mips, absent `MIP` and unavailable
  query handling.
- D3D11 HAL build check passed; complete strict private HAL suite passed
  370/370. Expected unsupported-shader cases logged explicit skips; they were
  not counted as successful render operations.
- Focused strict public cube test passed 1/1; 960/960 expected readback pixels
  matched on the physical FL9_1 device.
- Complete strict public D3D11 test binary passed 51/51. One optional
  Float16x2 input-format subcase logged a query-based skip because the RTX's
  forced-FL9_1 device did not report `IA_VERTEX_BUFFER` for `R16G16_FLOAT`.
- Controlled core fixture passed 96/96, including cube-layout and shape checks;
  DX12 core regression passed 92/92.
- HAL, core-fixture and public-test Clippy completed. No new Clippy warning
  remains; the HAL reports the existing `too_many_arguments` warning on
  `create_hal_view`.
- `git diff --check` passed. Workspace `cargo fmt --all -- --check` still
  reports five pre-existing formatting differences in the unchanged
  `naga/src/back/hlsl/writer.rs`; no unrelated Naga formatting was applied.

Exact focused and regression commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib private_cube_texture_validation_is_shape_and_query_aware
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_cube_texture_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 check --offline -j8 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
git diff --check
~~~

Continue the selected-entry shader and texture-view contract audit. Historical
FL9_1 and Windows Mobile ARM32 behavior remain unverified; the mission is not
complete.

## Public view checkpoint — non-suffix FL9_1 2D mip range — `4d3ec6bb9` — 2026-11-06

Extended the existing public sampled RGBA8 mip-chain test to bind a non-suffix
2D view selecting source mip levels 1 and 2. A nearest sampler with minimum
LOD 1 selects the second level relative to that view, which is source mip 2.
The test renders through the normal public wgpu/core/HAL path, exercising the
HAL's clone-and-refresh SRV path for a non-suffix 2D view, then reads back the
target. All 64 pixels matched mip 2's distinctive blue value. Combined with
the existing implicit and literal-bias checks, the sampled-mip test verified
320 pixels.

Verification used Rust 1.93.1, Visual Studio x64, strict hardware mode and the
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), with
requested and actual FL9_1:

- Focused `public_dx11_sampled_mip_chain_at_fl91`: 1/1 passed; 320/320 output
  pixels matched, including all 64 from the non-suffix 2D view.
- Complete strict public D3D11 suite: 51/51 passed. The optional Float16x2
  vertex-input subcase visibly skipped because the selected FL9_1 device did
  not report `IA_VERTEX_BUFFER` for `R16G16_FLOAT`.
- Public-test Clippy passed with only the existing HAL `create_hal_view`
  too-many-arguments warning. Current rustfmt would reflow unrelated legacy
  sections throughout `dx11_public.rs`; that noise was not retained. The new
  block follows the adjacent file style, and `git diff --check` passed.

Exact commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sampled_mip_chain_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
git diff --check
~~~

Continue the core/HAL contract audit; the mission remains partially complete.
Historical FL9_1 and Windows Mobile ARM32 behavior remain unverified.

## Verified public resource increment — query-gated FL9_1 3D sampling — `226d6ebda`

The public projection now exposes only the selected device's positively queried
RGBA8 3D sampled/upload subset: single mip, single sample, POT dimensions up to
256, with `TEXTURE_BINDING | COPY_DST`. Buffer uploads, including partial
regions and source offsets, use a staging-buffer read followed by
`UpdateSubresource`; this is ordered and correct but may block. The backend
zero-initializes volume storage before exposing it. Texture-to-texture copies,
readback, render attachments, storage and mip chains remain rejected.

The strict public test requested and asserted FL9_1 on the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false). It uploaded a
distinctive 4×4×4 volume with padded rows, updated a texel from a nonzero source
offset, rendered all four z slices through sparse logical bindings, and checked
64/64 pixels. A second volume was partially initialized; readback through a
sampled render checked that the written texel changed and all 63 untouched
texels remained zero. In total 128/128 pixels matched.

The strict core fixture accepted supported 3D shape/view descriptors and
rejected NPOT, unsupported format, render attachment, mip-chain and 2D-to-3D
copy cases before HAL. A CPU-only capability test verifies that exposure
requires the texture-3D and shader-sampling format bits plus both backend paths;
unavailable and incomplete queries fail closed. Naga/FXC compilation for
FL9_1 3D sampling and the `wgpu-types` shape-contract test also passed.

Verification used Rust 1.93.1, Visual Studio 18 x64, strict hardware mode and
bounded Cargo `-j8`:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
~~~

All listed suites passed. HAL, core and public-test Clippy checks passed with
the existing `create_hal_view` too-many-arguments warning. Workspace rustfmt
check still reports formatting reflows in Naga's HLSL writer and accumulated
DX11 code, including older sections outside the new 3D test; no unrelated
workspace reformat was applied. `git diff --check` passed. This x64 forced-level
test does not establish behavior on historical FL9_1 drivers or Windows Mobile
ARM32.

## Query-gated FL9_1 3D mip sampling — `77f82fece` — 2026-11-06

Extended the narrow RGBA8 3D sampled/upload path to accept mip chains only when
the selected device's format support query positively reports
`D3D11_FORMAT_SUPPORT_MIP` and the backend path is enabled. Every mip is
zero-initialized and uploaded independently. The core projection exposes this
optional limit and rejects 3D shader-resource views that omit the resource's
lowest mip, matching the 10Level9 FL9_1 SRV rule. Texture copies, readback,
render-target use and storage remain disabled for 3D textures.

On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), the public test
requested and asserted FL9_1, observed the positive mip query, uploaded
distinctive 4³, 2³ and 1³ levels with padded rows, submitted the uploads,
created only suffix views, sampled three levels and verified all 64 output
pixels exactly. It also confirmed that a view omitting the lowest mip is
rejected. This is forced-level execution on a modern driver; it does not
validate historical FL9_1 devices.

The core fixture now follows the reported capability: it accepts a queried
3D mip chain, rejects a non-suffix view, and accepts a suffix view; it checks
that a multi-mip descriptor is rejected when the optional query path is absent.
The first full fixture run found an outdated single-mip expectation. The test
was updated to assert the new query-gated contract, and the full suite then
passed.

Verification used Rust 1.93.1 and Visual Studio 18 x64. Strict hardware mode
was enabled for D3D11 commands:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features gles
cargo +1.93 clippy --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib
cargo +1.93 clippy --offline -j16 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib
cargo +1.93 clippy --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
cargo +1.93 clippy --offline -j16 -p wgpu-types --lib --features serde
cargo +1.93 fmt --all -- --check
git diff --check
~~~

Results: private HAL 375/375; public D3D11 integration 54/54; core fixture
97/97; core adapter-isolation test 1/1, enumerating two FL9_1 physical
adapters; DX12 core regression 92/92. DX11 and GLES HAL checks passed.
Clippy passed for HAL, core, public integration and `wgpu-types`; the existing
HAL `create_hal_view` too-many-arguments warning remains. Workspace formatting
check reports previously accumulated reflows in Naga and older DX11 sections;
no unrelated formatting sweep was applied. `git diff --check` passed. Build
artifacts remain under `E:\CodexTemp\wgpu-fl91-mission-target`.

## Queue callback lock-safety checkpoint — 2026-11-06

The selected-entry/core lifetime audit found that HAL submission errors were
passed to `Device::handle_hal_error` while queue submission still held the
command-index write guard. Device loss invokes the user callback synchronously;
a callback that polls the device therefore attempted to take a read lock while
the same thread retained the write lock. Queue submission now classifies and
invalidates the device immediately but carries the one-shot callback invocation
out of the submission operation, firing it only after queue submission locks
have been released. The failure path covers queue submit, pending-write flushes,
and the surface-present preparation submission without using the encoder's
shared deferred-callback slot.

The controlled regression callback calls `device_poll(Poll)` synchronously.
The test performs the failing submit on a worker thread and has a timeout, so
the prior reentrant deadlock becomes an explicit failure. It verifies a lost
submission result, successful callback polling, one callback invocation and
subsequent queue rejection. The RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, hardware adapter) is opened at requested/actual FL9_1; DXGI
device removal is synthetic, not an actual hardware-removal test.

Verification used Rust 1.93.1, Visual Studio 2022 Build Tools x64 and strict
hardware mode for the D3D11 tests:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_submission_device_loss_is_propagated_once -- --test-threads=1 --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --test-threads=1 --nocapture
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1 --nocapture
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
rustfmt +1.93 --edition 2021 --check wgpu-core/src/device/queue.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
~~~

The focused reentrant callback regression passed 1/1; the complete core fixture
passed 97/97; the public D3D11 suite passed 54/54 with one existing optional
Float16x2 format subcase visibly skipped; the DX12 core suite passed 92/92; and
the physical-adapter fixture passed 1/1, enumerating one adapter. After removing
two needless `clone` calls found by Clippy, the focused regression passed again
and core Clippy completed. Existing warnings are the HAL
`create_hal_view` argument count and three older fixture lints (nonminimal bool,
single-element loop, nested if). Targeted rustfmt reports pre-existing layout
differences in older fixture sections; the changed callback block and queue
code are formatted, and `git diff --check` passes. The first DX12 invocation
failed because the shell lacked `link.exe`; rerunning inside Visual Studio 2022
Build Tools x64 passed. Physical removal/recovery and historical FL9_1 driver
behavior remain unverified.

## Fence-poll callback lock-safety checkpoint — 2026-11-06

The reentrant-poll regression for direct HAL submit failures passed, but a new
synthetic fence failure exposed a second callback path: `Device::maintain`
called `handle_hal_error` while it still held the device snatch read lock.
`Device::lose` invoked the user callback immediately, and a callback polling the
same device triggered a ranked-lock recursion panic. Core now converts HAL
errors from both fence `wait` and `get_fence_value` into the existing
`UserClosures` batch. Queue submit retains that batch on a maintenance error and
fires it only after releasing its outer submission gate. `Device::poll` and
surface-configuration callers already fire returned batches after their local
guards are dropped.

Two controlled tests inject `DXGI_ERROR_DEVICE_REMOVED` independently from
event-query `GetData` and fence wait. The GetData case enters core maintenance
after a real empty queue submission; the wait case submits first, then polls
with a wait request. Each callback synchronously polls the device; each test
verifies device-loss propagation and exactly one callback. The RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, hardware adapter) is opened with
requested/actual FL9_1. Both native failures are injected; no actual device
removal or recovery is claimed.

Verification used Rust 1.93.1, Visual Studio 2022 Build Tools x64 and strict
hardware mode:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fence_ -- --test-threads=1 --nocapture
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
git diff --check
~~~

## Follow-up — statically unrolled FL9_1 WGSL loops — 2026-11-07

FXC testing showed that the FL9_1 profiles accept statically bounded WGSL
loops when Naga's `force_loop_bounding` guard is disabled. The guard itself
adds dynamic loop control that FXC cannot compile for these profiles. The HAL
now disables that lowering for `vs_4_0_level_9_1` and
`ps_4_0_level_9_1`; other profiles keep the existing bounded lowering. The
capability model marks compiler-unrolled loops as implemented, so the
selected-device projection exposes `DownlevelLimits::shader_loops` for this
subset. FXC rejects the tested runtime-bounded and unbounded loops with X3511
at pipeline creation. They report as shader-compilation errors, not as
pre-HAL core validation errors. FXC instruction and unrolling limits still
apply, and this modern-GPU result does not establish behavior on historical
FL9_1 drivers.

The public strict hardware test renders a full-screen triangle. Its vertex
shader uses a four-iteration loop to produce the red channel; its fragment
shader uses a three-iteration loop to produce green. All 64 pixels matched
`[255, 191, 0, 255]`. Separate dynamic vertex and fragment pipelines were
rejected by FXC before submission. The deterministic HAL compiler test also
checks integer-loop compilation and an unbounded vertex-loop rejection. A
core fixture explicitly disables the loop limit to retain coverage of the
pre-HAL rejection path independently of the production projection.

Verification used Rust 1.93.1, Visual Studio 2022 x64, strict hardware mode
for D3D11 and the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and actual FL9_1:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_static_loops_compile_and_dynamic_loops_are_rejected_by_fxc -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_runs_static_but_rejects_dynamic_shader_loops_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/shader.rs wgpu-hal/src/dx11/capabilities.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-types/src/limits.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

Results: strict private HAL 377/377; strict public D3D11 60/60; strict core
fixture 101/101; physical adapter enumeration 1/1; DX12 core 92/92. The
public suite's optional Float16x2 vertex-input subcase visibly skipped because
the selected device reported no `R16G16_FLOAT` input support. Existing private
tests also printed expected skips for local-vector rendering after their
runtime-integer shaders were rejected. All three Clippy commands passed; the
HAL and core runs emitted the pre-existing warnings recorded in earlier
entries. `git diff --check` passed. Rustfmt still reports only the repository's
existing formatting discrepancies in older regions of the touched files; the
new loop code is formatted. An initial core-fixture run exposed that its
synthetic no-loop rejection case inherited the newly enabled production
projection; the fixture now explicitly sets `shader_loops = false`, and its
rerun passed.

This is a narrow compiler-supported expansion, not general dynamic loop
support or FL9_1 completion. Continue the remaining core/HAL contract audit.

## FL9_1 3D copy boundary audit — 2026-11-07

The next transfer audit checked whether the existing query-gated 3D sampled
and upload path could safely gain `COPY_SRC` or texture-to-texture copies.
Microsoft's 10Level9 `CopySubresourceRegion`/`CopyResource` contract says
FL9_1 permits only 2D textures and buffers to be copied within GPU-accessible
memory, does not permit copying a 3D texture from GPU-accessible to
CPU-accessible memory, and does not permit copying mipmapped volume textures.
The existing FL9_1 projection correctly keeps `texture_3d_texture_copies`
false and 3D `COPY_SRC` out of `texture_3d_usage_mask`; no unsafe emulation
was added. A capability-model assertion and strict public validation test now
cover that rejected use.

Reference: [Microsoft 10Level9 context restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context).

This closes one documentation and negative-validation gap, not the broader
resource/HAL audit. Continue with feasible public graphics operations and
keep hardware-impossible FL9_1 cases explicitly gated.

Rust 1.93.1 / Visual Studio 2022 x64 validation:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 texture_3d_sampling_requires_its_own_query_and_backend_paths -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_texture_3d_upload_and_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu --tests --no-default-features --features dx11,wgsl,std
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The capability test passed 1/1. The strict public test passed 1/1 on the
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), requested and actual FL9_1. Existing 3D mip sampling matched
64/64 pixels; the full 3D upload/partial-update/zero-init render matched
128/128 pixels. The new unsupported `COPY_SRC` request was captured as a
validation error before HAL. Both Clippy commands passed with existing HAL
warnings (10 in the HAL test build and one library warning in the public test
build). Rustfmt still reports older formatting differences in these files;
the newly added lines were aligned to rustfmt, and `git diff --check` passed.

## ARM32 UWP public example codegen — 2026-11-06

The public `wgpu` library and `dx11_fl91_offscreen` example both type-check
for `thumbv7a-uwp-windows-msvc`. The first attempt to link the example used
the x64 Windows SDK libraries and failed with ARM/x64 machine conflicts and
unresolved CRT symbols. Selecting the installed ARM cross environment and
Windows SDK 10.0.22621.0 corrected the toolchain; the example then linked to
an ARM executable (`dumpbin /headers`: `1C4 machine (ARM)`):

~~~powershell
$buildDir = Join-Path ([System.IO.Path]::GetTempPath()) 'wgpu-fl91-thumbv7a-check'
$env:CARGO_TARGET_DIR = $buildDir
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64_arm store 10.0.22621.0 && cargo +nightly -Z build-std=core,std,panic_abort build --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std --target thumbv7a-uwp-windows-msvc'
& 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\bin\HostX64\x64\dumpbin.exe' /headers (Join-Path $buildDir 'thumbv7a-uwp-windows-msvc\debug\examples\dx11_fl91_offscreen.exe')
~~~

This confirms Rust code generation and UWP ARM32 linking for the offscreen
consumer example, not package installation or GPU execution. The Lumia
Device Portal `GET /api/os/devicefamily` probe again timed out, so target
deployment and runtime validation could not be attempted. The public
CoreWindow presentation path remains withheld pending actual UWP runtime
verification.

## ARM32 UWP HAL cross-compilation — 2026-11-06

The pinned Rust 1.93.1 toolchain does not ship the low-tier
`thumbv7a-uwp-windows-msvc` target component. Using the installed Rust source
and Visual Studio 2022 x64 MSVC environment, nightly `build-std` checks
compiled both `wgpu-hal` and the public `wgpu` crate with D3D11 support for
that target:

~~~powershell
$buildDir = Join-Path ([System.IO.Path]::GetTempPath()) 'wgpu-fl91-thumbv7a-check'
$env:CARGO_TARGET_DIR = $buildDir
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && cargo +nightly -Z build-std=core,std,panic_abort check --offline -j16 -p wgpu-hal --no-default-features --features dx11 --target thumbv7a-uwp-windows-msvc'
& cmd.exe /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvars64.bat" && cargo +nightly -Z build-std=core,std,panic_abort check --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --target thumbv7a-uwp-windows-msvc'
~~~

Both checks completed successfully; the public check compiled `naga`,
`wgpu-hal`, `wgpu-core` and `wgpu`. This proves compilation only; package
deployment and execution on Lumia Windows 10 Mobile hardware remain
unverified. The ARM32 build used nightly because the pinned stable toolchain
has no prebuilt target component. Artifacts were isolated in the system temp
directory. The active branch remained clean at `2deeffc49` during the checks.

## HWND Present device-loss callback — 2026-11-06

Checkpoint commit: `cb2e06cecae0a2b6dab1622558deb3ef2195dbeb`.

Added a fixture-only native Present HRESULT injection and an end-to-end public
wgpu test. The test configured and acquired a real HWND swapchain on the RTX
3090 Ti at requested/actual FL9_1, then injected `DXGI_ERROR_DEVICE_REMOVED`
instead of calling native Present. Core delivered the public lost callback
once. A poll re-entered from that callback returned the expected
presentation-guard timeout instead of deadlocking. This is synthetic failure
coverage; it does not simulate hardware removal or recovery.

Verification used Rust 1.93.1 and the Visual Studio 2022 x64 environment:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11-test-fixture,std --test dx11_surface_error_fixture -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_surface_error_fixture --no-default-features --features dx11-test-fixture,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu-hal/src/dx11/surface.rs wgpu-hal/src/dx11/swapchain.rs wgpu/tests/dx11_surface_error_fixture.rs
git diff --check
~~~

Results: fixture public test 1/1; normal strict public D3D11 suite 60/60; and
strict private HAL suite 376/376. The public suite explicitly skipped only
the optional Float16x2 vertex-input subcase because the selected device query
did not report support. The HAL suite's integer-local-indexing cases explicitly
skipped their GPU subcases after the FL9_1 validator rejected those operations.
The ordinary non-fixture HAL check and fixture-test Clippy passed. Clippy
reports one pre-existing `too_many_arguments` warning in `texture.rs`; the
newly introduced `std_instead_of_core` warning was corrected. Formatting and
diff checks pass. No DX12 run was needed because this increment changes only
the D3D11 test fixture and its public integration test.

This closes public Present error propagation/reentrancy coverage only. Public
adapter exposure remains experimental; the capability audit, unsupported
shader/resource operations, historical FL9_1 devices, physical device loss,
CoreWindow runtime and Windows ARM32 verification remain outstanding.

Results: both focused fence error tests passed; the complete strict HAL suite
passed 375/375, core fixture 99/99, public D3D11 suite 54/54 and DX12 core
regression 92/92. The public suite reported the existing optional Float16x2
input-format skip; it was not counted as GPU execution. HAL Clippy passed with
ten existing warnings (range-array tests, test-module ordering, argument count
and a needless borrow). Core Clippy passed with three existing fixture warnings
(nonminimal bool, single-element loop and nested if). Changed queue and fence
files pass targeted rustfmt; the core resource and fixture files still report
pre-existing reflows outside these edits. `git diff --check` passes. The first
full-suite run took about 89 seconds for HAL; output was buffered while that
suite ran, not hung. Physical device-loss recovery and historical FL9_1 drivers
remain unverified.

## Removed-device-reason polling checkpoint — 2026-11-06

Extended the synthetic device-loss coverage to D3D11's ordinary
`GetDeviceRemovedReason` check. A test-only one-shot HRESULT injection in the
native device reaches the existing HAL `check_if_oom` path. The core fixture
opens the physical RTX 3090 Ti explicitly at FL9_1, installs a device-lost
callback that synchronously calls `device_poll` again, and polls on a worker
thread with a timeout. The callback returns once, the outer poll completes,
and subsequent queue submission is rejected. The injected result does not
simulate hardware removal or recovery. This verifies callback behavior on a
physical device context, not a physical GPU failure.

Verification used Rust 1.93.1, Visual Studio 2022 Build Tools x64, and strict
hardware mode for D3D11:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_removed_reason_callback_can_reenter_device_poll -- --test-threads=1 --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1 --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/device.rs wgpu-hal/src/dx11/hal_device.rs wgpu-core/src/dx11_fixture_tests.rs
git diff --check
~~~

The focused synthetic regression passed 1/1; the full strict core fixture
passed 100/100, the private HAL suite 375/375, the public D3D11 suite 54/54,
the explicit public adapter-enumeration test 1/1, and the DX12 core regression
92/92. The public suite reported the existing optional Float16x2 vertex-input
subcase skip. Both Clippy commands passed with the previously recorded HAL and
fixture warnings. Targeted rustfmt reports older formatting differences in
`hal_device.rs` and earlier regions of `dx11_fixture_tests.rs`; the new code
follows rustfmt, and `git diff --check` passes.

This closes one more error-notification path, not the broader public core/HAL
contract audit. The device-loss result remains synthetic, historical FL9_1
drivers and ARM32 remain unverified, and the mission is still in progress.

## Public consumer example run — 2026-11-06

Built both `dx11_fl91_offscreen` and `dx11_fl91_windowed` through the public
`wgpu` crate. Ran the offscreen example, which explicitly requests only the
DX11 backend and verifies the returned adapter is physical and reports FL9_1.
On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), its indexed
textured-cube scene passed full readback validation: 112 red, 277 green and
635 blue-background pixels across the 1,024-pixel image. This is a small
consumer-style integration proof, not broad rendering compatibility.

Commands and result:

~~~powershell
cargo +1.93 check --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
cargo +1.93 check --offline -j16 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std
cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
~~~

Both example checks passed. The run printed the selected physical adapter,
confirmed requested and actual FL9_1, and reported `PASS` with the pixel
counts above. A fresh read-only Lumia Device Portal `GET /api/os/devicefamily`
also timed out, as did the earlier probe; Windows Mobile ARM32 remains an
environmental verification gap.

## BC1 GPU copy-source boundary — 2026-11-06

The FL9_1 private capability projection now permits BC1 as a
texture-to-texture copy source when the selected device reports the required
Texture2D support, without requiring an RTV. Core has a new per-format
texture-to-buffer downlevel limit so BC1 GPU-copy capability cannot imply CPU
readback. A strict public-core fixture copied a partial BC1 block region into a
second BC1 texture, sampled the destination, and matched all 64 RGBA8 output
pixels. It also verified that a BC1 texture-to-buffer copy fails core
validation before HAL execution. The RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) returned BC1 format mask `0x000213a0` at requested/actual
FL9_1; this modern forced-level result does not establish behavior on
historical FL9_1 devices.

Rust 1.93.1 and Visual Studio 2022 Build Tools x64 verification:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-types --lib texture_to_buffer_copy_format_mask_is_a_downlevel_limit
cargo +1.93 check --offline -j16 -p wgpu-test --lib
cargo +1.93 clippy --offline -j16 -p wgpu-types --tests
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
git diff --check
~~~

Results: strict private HAL 376/376; strict core fixture 100/100; strict
public D3D11 60/60; the explicit adapter isolation test 1/1; DX12 core 92/92;
the format-mask unit test 1/1; and `wgpu-test` library check passed after
updating exhaustive downlevel-limit literals for the new field. The public
suite explicitly skipped its optional Float16x2 input-format subcase because
the selected adapter did not report R16G16_FLOAT IA support. Types, HAL and
core Clippy passed with existing unused-serde/qualification, HAL range/test
layout and argument-count warnings, and fixture style warnings. Workspace
`cargo fmt --all -- --check` remains nonzero because older Naga, D3D11 and
public-test sections do not match current rustfmt; the new BC1 code was
formatted locally and `git diff --check` passes. This is a focused copy-path
increment; public device-enumeration policy and the broader FL9_1 capability
audit remain unchanged.

## Queue-drop synchronization error path — 2026-11-06

Added a fixture-only wait-for-idle error injection and a controlled core test
for queue destruction. On the physical RTX 3090 Ti opened at requested/actual
FL9_1, the injected `DXGI_ERROR_DEVICE_REMOVED` was classified as device loss;
the lost callback re-entered `device_poll`, fired once, and core queue
destruction completed without panic. The error is synthetic and does not model
physical device removal. The focused strict test passed 1/1. The complete
strict private HAL suite passed 376/376, the strict D3D11 core fixture passed
101/101, and the DX12 core regression passed 92/92. HAL production checking
and HAL/core Clippy passed with existing warnings; changed HAL files pass
rustfmt, while the core fixture still has earlier formatting differences and
the repository-wide check remains nonzero for pre-existing formatting.
`git diff --check` passes. This closes a previously uncovered cleanup/error
path; it does not change production queue behavior or complete the wider
synchronization/lifetime audit.

Verification commands, run from the Visual Studio 2022 x64 environment with
Rust 1.93.1 and strict physical-hardware mode for the D3D11 suites:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_queue_drop_wait_device_loss_is_reported_without_panicking -- --test-threads=1 --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl -- --test-threads=1
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,std,wgsl
git diff --check
~~~
## Public `SurfaceTexture` clone lifecycle — 2026-11-07

The strict public HWND test exposed a safe-API lifetime bug: dropping one
`SurfaceTexture` clone immediately discarded the frame from core, so
presenting another clone returned `NothingToPresent`. The public wrapper now
stores its presentation detail and presented state in a shared lifecycle
object. Dropping a clone only discards when the final unpresented handle is
released; presenting any clone marks the shared frame as presented.

The extended HWND test covers three transitions on the physical NVIDIA RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual
FL9_1: drop one clone then present the other; present one clone then drop an
alias; and drop the final unpresented clone then reacquire. Each transition
completed without a validation error, and the existing wrong-thread present
recovery still reacquired and presented a frame.

The first strict test run before the implementation reproduced the failure:
public present reported `No surface image is currently acquired`. After the
fix, the focused HWND test passed 1/1 and the complete strict public D3D11
binary passed 62/62. The optional Float16x2 input-format branch remained an
explicit capability skip and was not counted as hardware execution.

Rust 1.93.1 checks:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_hwnd_surface_presents_on_its_window_thread -- --nocapture --test-threads=1
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
cargo +1.93 check --offline -p wgpu --no-default-features --features dx12,wgsl,std
cargo +1.93 check --offline -p wgpu --target wasm32-unknown-unknown --no-default-features --features webgpu,wgsl
rustup run 1.93 rustfmt --edition 2021 --check wgpu/src/api/surface_texture.rs wgpu/src/api/surface.rs wgpu/src/api/queue.rs
git diff --check
~~~

Both strict D3D11 test invocations used the Visual Studio 2022 x64 developer
environment. Public-test Clippy passed with the pre-existing HAL
`create_hal_view` argument-count warning. DX12 compilation and the wasm
WebGPU public-crate check passed. The source-file rustfmt check passed; the
large public D3D11 test file still reports three unrelated formatting deltas
that predate this change. The opt-in backend and wider FL9_1 contract audit
remain experimental and incomplete.

## Follow-up — cross-thread surface-frame alias drop — 2026-11-07

The strict HWND lifecycle regression now drops a non-final `SurfaceTexture`
clone on a worker thread while retaining another clone on the owner thread,
then presents the retained frame. This verifies shared lifecycle state across
the public type's `Send + Sync` boundary and avoids running final frame discard
on the worker in this case. The focused strict public test passed 1/1 on the
RTX 3090 Ti at requested/actual FL9_1. The complete public suite was not rerun
for this test-only extension; it passed 62/62 immediately before the change.

## Follow-up — public HWND default configuration — 2026-11-07

The public HWND smoke test now obtains its initial surface settings from
`Surface::get_default_config`. The method returns `Auto` color-space and alpha
values plus its generic two-frame latency hint; core resolves those values and
clamps the hint to the D3D11 surface's advertised one-frame range before HAL
configuration. The strict test then clears/presents, resizes and presents
again at requested/actual FL9_1 on the RTX 3090 Ti. The focused test passed
1/1. The full public suite was not rerun for this test-only change; its latest
run passed 62/62 immediately beforehand with the optional Float16x2 IA branch
explicitly skipped.

## Public frame cleanup after surface destruction — 2026-11-07

The HWND lifecycle test found a safe-API panic when a `Surface` was dropped
while an acquired, unpresented `SurfaceTexture` remained alive. Dropping the
frame afterward called `surface_texture_discard` through a removed
`SurfaceId`. `CoreSurface` and its output details now share a lifecycle mutex:
surface destruction marks the handle unavailable before removing it, and
discard/release serialize their core calls through the same guard. Output
cleanup becomes a no-op once the owning surface has removed and released the
frame.

The strict test reproduces surface destruction with a live frame and confirms
that dropping the frame causes no uncaptured error. The focused HWND test
passed 1/1 and the complete strict public suite passed 62/62 on the physical
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested/actual
FL9_1. The suite's optional Float16x2 IA branch explicitly skipped after a
negative selected-device query.

Cross-backend validation used Rust 1.93.1 and the Visual Studio 2022 x64
environment:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 check --offline -p wgpu --no-default-features --features dx12,wgsl,std
cargo +1.93 check --offline -p wgpu --target wasm32-unknown-unknown --no-default-features --features webgpu,wgsl
cargo +1.93 clippy --offline -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/src/backend/wgpu_core.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The DX12 and wasm public-crate checks and targeted Clippy passed; Clippy
reported the pre-existing D3D11 `create_hal_view` argument-count warning.
`wgpu_core.rs` passes rustfmt; the public test file reports three older
formatting differences in unrelated regions. This does not validate
CoreWindow or historical-driver behavior.

## Queue present after surface destruction — 2026-11-07

The lifecycle regression now also presents a retained public frame after its
owning `Surface` has been dropped. Before the fix, strict FL9_1 execution
reproduced a panic in core storage lookup (`Cannot get non-existent resource
SurfaceId(0,1)`). `CoreQueue::present` now locks the same per-surface lifecycle
guard used by frame discard/release; it calls core only while the surface is
live and otherwise reports `SurfaceError::Invalid` through the queue error
sink. The test verifies the validation report, then recreates the HWND surface
on a second hidden HWND while retaining an acquired frame from it. The stale
present reports validation without consuming the second surface's frame, which
then presents normally. A further frame verifies that dropping its final
unpresented output after surface destruction remains safe. Recreating on the
same HWND while retaining the original output was not a valid fixture because
the old native swapchain remained referenced, so the regression uses a second
HWND in the same wgpu instance.

On NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested and
actual FL9_1, the focused HWND test passed 1/1 and the final complete strict
public D3D11 suite passed 62/62 (42.69 seconds). The suite visibly skipped only its optional
Float16x2 vertex-input branch after the selected device reported no IA support.
Rust 1.93.1 targeted Clippy passed with the existing D3D11
`create_hal_view` argument-count warning. DX12 and wasm WebGPU public-crate
checks passed. Rustfmt passed for `wgpu_core.rs`; the public test file still
has two pre-existing formatting differences in unrelated sections.
`git diff --check` passed. This closes the stale surface-ID panic for public
present, without closing the broader surface/device-loss audit.

The checks ran after loading the Visual Studio 2022 x64 developer environment:

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_hwnd_surface_presents_on_its_window_thread -- --nocapture --test-threads=1
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
cargo +1.93 check --offline -p wgpu --no-default-features --features dx12,wgsl,std
cargo +1.93 check --offline -p wgpu --target wasm32-unknown-unknown --no-default-features --features webgpu,wgsl
rustup run 1.93 rustfmt --edition 2021 --check wgpu/src/backend/wgpu_core.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The Lumia Device Portal was retried with
`Invoke-LumiaPortal -Path '/api/os/devicefamily'`; the helper timed out in
`GetResponse()` again. This remains an external runtime-verification blocker;
no credentials or pairing state were changed.

## Public timestamp query rejection — 2026-11-07

The existing strict public query test covered occlusion query sets through the
missing `DownlevelFlags::OCCLUSION_QUERY` gate. It now also requests a timestamp
query set and verifies core rejects it through the unrequested
`Features::TIMESTAMP_QUERY` gate before the private HAL's unsupported query-set
creation method. The combined focused test passed 1/1 on the physical NVIDIA
RTX 3090 Ti at requested/actual FL9_1 (`0x10de:0x2203`, driver
`32.0.16.2101`); no query commands were issued to the GPU. After this change,
the complete strict public D3D11 suite passed 62/62; its optional Float16x2
vertex-input branch was explicitly skipped because the device query was
negative.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_occlusion_queries_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
git diff --check
~~~

## Public query-set validation boundary — 2026-11-07

The query-set regression now covers all three public query types. On the
physical RTX 3090 Ti at requested/actual FL9_1 (`0x10de:0x2203`, driver
`32.0.16.2101`), core rejected occlusion through the missing
`OCCLUSION_QUERY` downlevel flag and rejected timestamp and pipeline-statistics
sets through their absent features, before the unsupported HAL query creation
method. The focused test passed 1/1; it issued no query commands. The complete
strict public D3D11 suite passed 62/62 in 40.59 seconds. Its optional
Float16x2 vertex-input subcase visibly skipped because the selected device
reported no `R16G16_FLOAT` input-assembler support. Public-test Clippy passed
with the existing `create_hal_view` too-many-arguments warning. `git diff
--check` passed. Rustfmt reported three existing differences elsewhere in the
large public test file; the new test's changed lines require no formatting
change. The Lumia Device Portal request timed out again; CoreWindow runtime
verification remains outstanding.

Checkpoint: `6a171483b` (`dx11: validate all unavailable FL9_1 query types`).

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_unavailable_query_types_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

## Query-gated FL9_1 depth comparison sampling — 2026-11-07

Added a restricted private HAL path for sampled D16 depth with
`textureSampleCompare`. Comparison layouts and sampler creation now share an
internal capability predicate requiring all three conditions: the selected
level is FL9_1, the backend path is implemented, and
`D3D11_FEATURE_D3D9_SHADOW_SUPPORT` returns `true`. Unavailable or negative
queries and other feature levels reject the path. The private sampler supports
LessEqual only, matching min/mag filters, anisotropy 1 and the FL9_1 native LOD
requirements. The public wgpu projection still does not advertise comparison
samplers or sampled depth because its existing flags cannot express this
narrower contract.

The strict RTX 3090 Ti test (`0x10de:0x2203`, driver `32.0.16.2101`) requested
and confirmed FL9_1, reported shadow support true, wrote depth 0.25 to half of
an 8x8 D16 resource, then compared against 0.5. All 32 failing pixels were
black and all 32 passing pixels red. A second hardware test created typeless
D16/R16 and D24S8/R24 DSV/SRV views and LEQUAL samplers. Typed SRV
`CheckFormatSupport` returned `E_FAIL` for both view formats, while native
resource and view creation succeeded; only D16 comparison was rendered, so
D24S8 comparison remains unverified.

The full strict private HAL suite passed 385/385. The two new private hardware
tests passed; the capability predicate CPU test passed. The controlled core
fixture passed 1/1 and confirmed public comparison sampler requests still fail
the missing-downlevel gate; explicit core D3D11 adapter selection passed 1/1.
DX12 core regression passed 92/92. HAL `cargo check` and test Clippy passed;
Clippy reports existing warnings, including `create_hal_view` argument count,
single-range slice construction, items-after-test-module and an old needless
borrow. Rustfmt check found only four pre-existing formatting differences in
`capabilities.rs`; newly changed code in that file is formatted. `git diff
--check` passed.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib dx11:: -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_fixture_tests::fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_scaffold_tests::requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
cargo +1.93 check --offline -j8 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j8 -p wgpu-hal --tests --no-default-features --features dx11
git diff --check
~~~

## Public integration readiness check — 2026-11-07

Rebuilt and reran the public offscreen wgpu example from the current clean
checkpoint using Rust 1.93.1 and the Visual Studio 2022 x64 environment. It
selected the physical NVIDIA RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`), explicitly requested and received FL9_1, and reported
`software=false`. The public API rendered and read back the textured indexed
cube; the 32×32 image contained 112 red, 277 green and 635 blue-background
pixels, with all 1,024 pixels classified as expected.

This confirms that a constrained opt-in project trial is practical today; it
does not establish broad application compatibility. The README now matches the
current conditional `TEXTURE_FORMAT_16BIT_NORM` and
`TEXTURE_FORMAT_16BIT_PACKED` projection, distinguishes the qualitative HWND
capture from automated offscreen pixel checks, and calls out major unsupported
API paths. The larger shader/resource/core-HAL audit, historical-driver
coverage, physical device-loss behavior and Lumia CoreWindow runtime remain
open.

~~~powershell
& $env:ComSpec /d /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std'
git diff --check
~~~

## Public FL9_1 point and triangle draw boundaries — 2026-11-07

The HAL already split nonindexed point-list draws at the documented FL9_1
65,535 primitive limit, but the public GPU path lacked a boundary test. Added a
strict public wgpu test that submits 65,536 points with only the final vertex
inside the view. The draw is split into two native batches; forced-FL9_1
readback on the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) found the
one expected red pixel from the second batch and blue in all other pixels.
The same public draw also sends 65,538 triangle-list vertices: the first
65,535 form clipped triangles, and the final visible triangle is rendered in
the second native batch. Readback found 32 red and 32 blue pixels. This
verifies public core recording, HAL splitting, offset rebasing, command
submission and readback for both boundaries, not historical-driver behavior.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_splits_point_and_triangle_draws_at_fl91_limits -- --nocapture --test-threads=1
~~~

The focused test passed 1/1, and the full strict public D3D11 suite then passed
63/63 in 43.26 seconds. Public-test Clippy passed with the existing
`create_hal_view` too-many-arguments warning. `git diff --check` passed.
Rustfmt still reports four pre-existing formatting differences elsewhere in
the large public test file; the new test itself matches rustfmt. The optional
Float16x2 test branch reported a genuine selected-device capability skip.
Continue the broader public core/HAL contract audit after this boundary
checkpoint.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

## Verified resource boundary — 1D textures — 2026-11-07

The generic wgpu limits retain a nonzero `max_texture_dimension_1d` for FL9_1,
but Microsoft's 10Level9 device table explicitly states that
`ID3D11Device::CreateTexture1D` is not supported on any 9.x feature level.
This confirms the existing `DownlevelLimits::texture_1d = false` projection is
intentional; a D1 texture cannot be added as a backend implementation slice at
FL9_1. The strict public offscreen test now asserts the capability gate and
that a D1 RGBA8 sampled/upload descriptor is rejected by core before reaching
HAL. The remainder of the test rendered and read back 1,024 pixels on NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly
requested/actual FL9_1 and `software=false`; the D1 rejection itself issued no
GPU command.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_api_renders_offscreen_at_fl91 -- --nocapture --test-threads=1
~~~

The focused test passed 1/1. The complete strict public D3D11 suite then passed
63/63 in 44.69 seconds; the optional Float16x2 subcase visibly skipped because
the selected device lacks the queried IA format support. Core's strict
`requesting_dx11_exposes_only_fl91_hardware_adapters` test passed 1/1 and
enumerated one physical FL9_1 adapter. Public-test Clippy passed with the
existing `create_hal_view` too-many-arguments warning. Rustfmt check reports
four pre-existing formatting differences elsewhere in the large public test
file; `git diff --check` passed. Visual Studio's developer prompt emitted its
existing `Error: Unknown error` banner, but MSVC initialization and the Cargo
tests completed successfully. This records a feature-level API limit, not an
additional implementation capability.

## Query-gated 4x FL9_1 depth attachments — 2026-11-07

The existing 4x RGBA8 render/resolve path now accepts matching D16 or D24S8
depth attachments only when the selected device's `CheckFormatSupport` and
`CheckMultisampleQualityLevels(format, 4)` queries positively establish the
required support. The HAL creates explicit 2DMS DSVs and checks the depth
attachment sample count against the render-pass count. Format projection adds
`MULTISAMPLE_X4` for depth only when both the driver query and the backend path
are available; it never reports depth resolve or multisampled depth sampling.
Synthetic capability tests cover positive, zero-quality, unavailable-query,
backend-disabled and unsupported-count cases.

The strict public test now draws a near red triangle followed by a farther
green triangle into 4x RGBA8 plus query-selected 4x depth, resolves color and
checks all 64 pixels. On the physical NVIDIA RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`), explicitly requested and actual FL9_1, D16 and D24S8
both reported one 4x quality level. The D24S8 test produced 28 full red
pixels, 8 red/blue coverage-edge pixels and 28 blue clear pixels, with green
zero everywhere; the zero-sample-mask control retained blue at all 64 pixels.
This is a selected modern driver's result and not evidence about every
historical FL9_1 driver.

The full strict private HAL suite passed 377/377. The full strict public D3D11
suite passed 63/63; its Float16x2 input-assembler subcase visibly skipped after
the selected device query returned false. Core's strict DX11 adapter-isolation
test passed 1/1 and enumerated the physical FL9_1 adapter. The targeted DX12
core suite passed 92/92. HAL and public-test Clippy passed with the existing
`create_hal_view` argument-count warning. Rustfmt check still reports only
pre-existing formatting differences outside the new code, and `git diff
--check` passed.

The first public attempt exposed a too-narrow HAL usage check: core's internal
depth-clear view uses write-only depth usage while the attachment view uses
read/write usage. The accepted HAL usage set now covers those two attachment
shapes and their transient variants; unsupported copy/resource combinations
remain rejected. This closes the usage mismatch without expanding depth
resource functionality beyond render-pass attachments.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib dx11:: -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,wgsl,std
cargo +1.93 clippy --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11
cargo +1.93 clippy --offline -j16 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/texture.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The Visual Studio x64 developer-shell banner still prints `Error: Unknown
error`, but the compiler environment initialized and every listed Cargo command
completed. The change remains private to the opt-in D3D11 projection; public
default backend selection and normal adapter exposure are unchanged. Continue
the API-contract audit; this does not make the backend broadly feature-complete.

Follow-up target check: `wgpu-hal` also passed ARM32 UWP cross-compilation with
Rust 1.93.1 and the installed ARM MSVC/Windows SDK environment. Artifacts were
isolated under `%TEMP%\wgpu-fl91-ms8-arm32`; this verifies target compilation,
not Lumia runtime behavior.

~~~powershell
$env:RUSTC_BOOTSTRAP = '1'
$buildDir = Join-Path ([System.IO.Path]::GetTempPath()) 'wgpu-fl91-ms8-arm32'
$env:CARGO_TARGET_DIR = $buildDir
& cmd.exe /d /s /c 'call "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat" x64_arm store 10.0.22621.0 && cargo +1.93 -Z build-std=std,panic_unwind,panic_abort check --offline -j16 -p wgpu-hal --target thumbv7a-uwp-windows-msvc --no-default-features --features dx11'
~~~

## Core fixture depth-format projection regression — 2026-11-07

Running the complete controlled core fixture after adding depth
`MULTISAMPLE_X4` found a stale exact-flags assertion: core now correctly
projected the flag returned by the selected-device HAL query. Updated the
fixture to compare core's X4 flag against the HAL result and separately assert
that depth resolve remains absent. The strict fixture passed 8/8 afterward;
its main hardware case exercised the unmodified selected-device projection on
the physical RTX at requested/actual FL9_1, while six device-loss injections
remain synthetic. The fixture also rendered its existing color/depth, shader,
texture and resource cases successfully. Clippy passed with the existing HAL
`create_hal_view` argument-count warning. Rustfmt reports only pre-existing
formatting differences in earlier fixture code; the edited assertions are
formatted, and `git diff --check` passed.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_fixture_tests:: -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu-core/src/dx11_fixture_tests.rs
git diff --check
~~~

The first run had 7/8 pass and failed only at the stale assertion; after
updating it, the repeated strict run passed all 8. This confirms the optional
depth multisample capability is carried through HAL into core without
advertising depth resolve.

## Latest increment — representative public 3D scene and Lumia probe — 2026-11-07

Implementation and verification checkpoint: `d4458f457c8b117c76eebd7e20abfebf386b7778`
(`dx11: verify public FL9_1 textured 3D scene`).

Checkpoint `08702e95f9c930e8e14a59d53c18711db39bb2bb` expanded the public 4x
depth test to execute both selected-device-supported formats. The follow-up
adds `public_dx11_textured_depth_tested_3d_scene_at_fl91`, an indexed cube
using an uploaded RGBA8 face texture, a WGSL uniform transform, nearest
sampling and a D16 depth target. It rendered the 24-vertex/36-index cube twice
with opposite face order. On the RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) at explicitly requested and actual FL9_1, all 1,024 pixels
matched between images; the output contained 154 red, 21 blue, 70 yellow and
779 clear pixels. This verifies one representative public scene workload, not
the rest of the wgpu API or historical hardware.

The strict scene test passed 1/1 and test-target Clippy passed with the existing
HAL `create_hal_view` argument-count warning. The 4x depth focused test and
full public suite were also rerun in the preceding checkpoint: D16 and D24S8
each checked 64 expected resolved pixels, and the public suite passed 63/63
with the optional Float16x2 vertex format visibly skipped. The private HAL
suite passed 377/377. The first public rebuild at 16 jobs hit OS error 1455;
retry at `-j2` passed.

Platform probe: ping to `192.168.1.157:443` timed out, and the configured
read-only Lumia Device Portal `GET /api/os/devicefamily` timed out after about
16 seconds. Existing ARM32 UWP cross-compilation does not establish phone
runtime behavior. CoreWindow/Lumia verification remains blocked by device
reachability; no network or device settings were changed. Continue the
public-valid core/HAL audit and keep the mission **PARTIALLY COMPLETE**.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_fl91_scene -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu --test dx11_fl91_scene --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_fl91_scene.rs
git diff --check
~~~

## Follow-up — D16 and D24S8 4x depth execution — 2026-11-07

Expanded `public_dx11_resolves_four_sample_rgba8_at_fl91` to execute every
selected-device-query-supported 4x depth format, rather than only the first.
The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) positively queried
one 4x quality level for both D16 and D24S8 while the device was explicitly
requested and confirmed at FL9_1. Both actual render/resolve runs checked all
64 pixels: 28 near-red, 8 partial-coverage, and 28 clear; the farther green
draw was rejected. The zero-sample-mask control preserved blue at all 64
pixels. This is modern-driver evidence only, not historical FL9_1 coverage.

The strict private HAL suite passed 377/377. The complete strict public D3D11
test target passed 63/63; its optional Float16x2 vertex-input subcase was
explicitly skipped after the selected-device query returned false. Public
test Clippy passed with the pre-existing HAL `create_hal_view` argument-count
warning. Rustfmt still reports four existing formatting differences outside
the edited test, and `git diff --check` passed. The first public rebuild at 16
Cargo jobs hit Windows paging-file exhaustion (OS error 1455); the focused
test and full public target then built and ran successfully at `-j2`.

The nested entry-interface builtin experiment was removed: Naga 30.0.1
rejects the proposed nested IO structure during validation, so it cannot
reach this backend from valid WGSL. No HAL shader behavior changed. Continue
the core/HAL audit; public enumeration stays opt-in, and historical FL9_1,
Lumia/CoreWindow runtime and physical device-removal behavior remain
unverified.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib dx11:: -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_four_sample_rgba8_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
rustup run 1.93 rustfmt --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
~~~

## Follow-up — depth necessity control for the public 3D scene — 2026-11-07

Extended `public_dx11_textured_depth_tested_3d_scene_at_fl91` with a second
pipeline that omits depth. The same indexed cube is rendered in forward and
reverse face order both with D16 `Less` depth testing and with painter-order
overwrites. On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at
requested/actual FL9_1, the depth-tested images matched at all 1,024 pixels;
the painter-order outputs differed at 223 pixels. The depth image still
contained 154 red, 21 blue, 70 yellow and 779 clear pixels. This confirms that
overlapping faces are present and the stable result depends on depth testing.

Strict hardware test passed 1/1. The first run exposed expected implicit
pipeline-layout incompatibility between the two independently inferred
pipelines; the test now creates a matching resource bind group for each
pipeline and validates the ordinary public layout contract.

## Follow-up — D24S8 shadow comparison execution — 2026-11-07

Code/test checkpoint: `b90387efc` (`dx11: validate D24S8 FL9_1 shadow sampling`).

Generalized the existing private `SampleCmp` hardware case to execute against
both D16 and D24S8, retaining the same two-pass layout and pixel expectations.
The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false) was
explicitly requested and selected at FL9_1. `D3D11_FEATURE_D3D9_SHADOW_SUPPORT`
returned true. Both formats rendered an 8×8 RGBA8 target with exactly 32
failing black comparisons and 32 passing red comparisons. The format probe
also created D16/R16 and D24S8/R24 typeless resources and typed views. The
typed SRV `CheckFormatSupport` calls returned `E_FAIL` (`0x80004005`), while
the actual native creation calls and subsequent HAL D24S8 shader sampling
succeeded. This records a driver-query limitation separately from verified
resource and execution behavior.

The focused strict shadow tests passed 3/3; the view test covers D16 and D24S8
creation, and separate D16/D24S8 tests each rendered and read back the result.
The full
strict private HAL suite passed 386/386 with 2 unrelated tests filtered. The
controlled core fixture passed 1/1; its public comparison-sampler requests
remain rejected before HAL. No public capability changed. Existing compiler
diagnostics for intentionally rejected FL9_1 shader forms and synthetic
failure-injection tests appeared in the full suite; no test failed.
HAL test Clippy passed with ten warnings in existing HAL/test code outside the
new test; no new lint warning was reported. Changed-file rustfmt and
`git diff --check` passed.

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib dx11::hal_encoder::shadow_sampling_tests:: -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib dx11:: -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_fixture_tests::fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu-hal --tests --no-default-features --features dx11
rustup run 1.93 rustfmt --edition 2021 --check wgpu-hal/tests/dx11/shadow_sampling.rs
git diff --check
~~~

## Query-gated public depth comparison — 2026-11-07

Added the selected-device D3D9 shadow-support query to the public FL9_1
projection. A positive query now permits only `LessEqual` comparison sampling,
equal minification/magnification filters, comparison anisotropy 1 and sampled
single-sample D16/D24S8 views. Core validates these limits before the HAL.
Naga's selected-entry runtime-integer gate also now treats depth images as
floating-point sampling resources; a regression test covers a valid
`textureSampleCompare` entry point.

The HAL view metadata check previously rejected depth-only and stencil-only
D24S8 ranges, although core correctly resolves a sampled D24S8 view to its
DepthOnly aspect. It now accepts the two valid aspect ranges while retaining
the full-resource requirement for the restricted D24 path. Public core still
rejects aspect-specific D24S8 render-attachment views.

On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), explicit requested and actual FL9_1 matched.
`D3D11_FEATURE_D3D9_SHADOW_SUPPORT` was true. Separate public D16 and D24S8
comparison renders each read back 64 pixels: exactly 32 black failed
comparisons and 32 red passing comparisons. The D24S8 load/clear/discard
transition test also read back its expected 576 pixels.

Verification:

- Strict complete public D3D11 test target: 65/65 passed, zero skipped.
- Strict private D3D11 HAL suite: 389/389 passed, zero skipped.
- Focused core projection/adapter test: 1/1 passed on physical FL9_1.
- Selected-entry depth sampling validation regression: 1/1 passed.
- Shared core DX12 regression suite: 93/93 passed.
- GLES HAL check and `wgpu-types` Clippy passed.
- HAL, core-fixture and public-test Clippy passed; warnings were existing
  HAL/test warnings, including `create_hal_view` argument count and
  single-range test expressions.
- `git diff --check` passed. Rustfmt check still reports pre-existing
  formatting differences in unchanged sections of the fixture and capability
  test files; those unrelated sections were left untouched.

The strict public command was
`WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1`.
The strict private command was
`WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 -- --test-threads=1`.

This enables a narrow query-gated public capability only on devices reporting
the optional feature. The full core/HAL audit, historical FL9_1 behavior,
Lumia/CoreWindow runtime and physical device-removal recovery remain open.

### Follow-up — explicit-level shadow sampling coverage — 2026-11-07

Added public `textureSampleCompareLevel` execution coverage and a focused FXC
profile test. Naga lowers the WGSL builtin to `SampleCmpLevelZero`, which FXC
accepted for `ps_4_0_level_9_1`. On the physical RTX 3090 Ti at explicitly
requested and selected FL9_1, the public D16 test rendered/read back all 64
pixels: 32 failed comparisons were black and 32 passing comparisons were red.
The full strict public target passed 66/66 with zero skips; the full strict
private HAL suite passed 390/390 with zero skips. This is separate from the
implicit-derivative `textureSampleCompare` runs for D16 and D24S8, which remain
covered by the preceding entry. It does not widen the supported comparison
sampler contract.

Focused FXC command:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib dx11::shader::tests::level_9_1_pixel_profile_compiles_shadow_comparison_level_zero -- --nocapture
```

### Follow-up — public Depth24Plus shadow sampling — 2026-11-07

Extended the query-gated public depth path to `Depth24Plus`. The D3D11 HAL
backs this logical format with D24S8 storage, tracks the logical format through
texture views and pipelines, and does not expose the backing stencil aspect.
Its attachment and comparison-sampling projection uses the selected device's
D24S8 format and shadow-support queries. The controlled core fixture confirms
the `Depth24Plus` projection matches D24S8, creates the texture and view, and
keeps `DepthOnly` views of combined D24S8 non-renderable; `StencilOnly` D24S8
view creation remains rejected.

The physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false) created an explicitly requested and selected FL9_1 device.
Its positive `D3D11_FEATURE_D3D9_SHADOW_SUPPORT` query enabled only the
existing `LessEqual`, equal-filter, anisotropy-1 comparison subset. The public
Depth24Plus render/readback verified all 64 pixels: 32 failed comparisons were
black and 32 passing comparisons were red. This is selected-driver evidence;
it does not establish behavior on historical FL9_1 devices.

The existing public 4x color resolve/depth test was expanded to include
Depth24Plus rather than inferring its multisample attachment behavior from the
shared D24S8 backing. On the same forced-FL9_1 adapter, D16, Depth24Plus and
D24S8 each passed the near/far depth test: 28 full-red pixels, 8 partially
covered red/blue pixels and 28 blue clear pixels, with all green channels zero
after resolve. The focused test passed 1/1; after this addition the complete
strict public D3D11 target passed 67/67.

The independent read-only review found no concrete correctness issue in the
logical/native format mapping, view handling, capability gates or core stencil
validation. Historical-driver coverage remains the caveat.

With `WGPU_DX11_REQUIRE_HARDWARE=1`, the full private HAL suite passed 390/390
and the full public D3D11 integration target passed 67/67. The core adapter
isolation test and selected-device fixture test passed 1/1 each on hardware;
the core DX12 regression suite passed 93/93. All three crate checks for HAL,
core fixture and public wgpu passed before the final full test run. Clippy for
those targets passed; it reported existing warnings in HAL tests and the
`create_hal_view` argument count, with no warning on the new Depth24Plus path.

The initial core isolation invocation omitted `std` and `wgsl` while using
`--no-default-features`, so it failed to compile the test target. Re-running
with `--features dx11,wgsl,std` passed. `cargo +1.93 fmt --all -- --check`
still reports pre-existing formatting drift in Naga, HAL transfer imports,
and earlier fixture/capability code; the new Depth24Plus fixture assertions
were formatted without reformatting those unrelated regions. `git diff
--check` passed.

Commands (run from the Visual Studio 2022 x64 developer environment):

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_four_sample_rgba8_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std dx11_scaffold_tests::requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_fixture_tests::fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx12,wgsl,std
cargo +1.93 clippy --offline -j32 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 clippy --offline -j32 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std
cargo +1.93 clippy --offline -j32 -p wgpu --test dx11_public --no-default-features --features dx11,wgsl,std
cargo +1.93 fmt --all -- --check
git diff --check
~~~

### Follow-up — per-count query-gated FL9_1 multisampling — 2026-11-07

Replaced the fixed 4x capability field with independent selected-device
quality-level queries for 2x, 4x, 8x and 16x RGBA8, D16 and D24S8. Format
flags, HAL resource/view creation, pipeline validation, sample masks, render
passes and resolve now follow the queried count. Unsupported query results are
not projected. The RTX 3090 Ti (`0x10de:0x2203`, NVIDIA driver
`32.0.16.2101`, software=false), explicitly requested and selected at FL9_1,
reported RGBA8 quality levels 1, 1, 1 and 0 for 2x, 4x, 8x and 16x. Thus x16
remains rejected. D16 and D24S8 are queried separately.

The new strict public test rendered, resolved and read back x2 and x8 RGBA8
targets with matching Depth24Plus attachments; all 64 pixels were exactly
green at both counts. Existing x4 depth and zero-sample-mask coverage still
passed for D16, Depth24Plus and D24S8. Strict test results:

- Private D3D11 HAL suite: 397 passed, 0 failed, strict hardware required.
- Public D3D11 suite: 72 passed, 0 failed; one optional Float16x2 input
  subcase reported SKIP because the selected device lacks the IA format query.
- Selected-device controlled core fixture: 1 passed.
- Public FL9_1 adapter-selection test: 1 passed.
- DX12 core regression: 94 passed, 0 failed.
- HAL and public-test Clippy passed. HAL retains pre-existing warnings,
  including `create_hal_view` argument count and test-only range-vector lints.
- Rustfmt check reported pre-existing formatting differences in unrelated
  fixture code and capability assertions; other changed Rust files passed.
- `git diff --check` passed.

Commands (Visual Studio x64 developer environment, pinned Rust 1.93):

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j4 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_optional_fl91_sample_counts -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_four_sample_rgba8_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_fixture_tests::fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std dx11_scaffold_tests::requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx12,wgsl,std -- --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustup run 1.93 rustfmt --check --edition 2021 wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/hal_device.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/texture.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The mission remains partially complete. Historical FL9_1 drivers, Lumia
runtime, physical device removal and broader API-contract coverage remain
unverified or in progress.

### Follow-up — query-gated FL9_1 wireframe rasterization — 2026-11-07

The capability model now probes the selected D3D11 device by creating a
`D3D11_FILL_WIREFRAME` rasterizer state. `POLYGON_MODE_LINE` is advertised
only after that positive result. HAL pipeline creation maps triangle
`PolygonMode::Line` to wireframe and rejects polygon point mode; unavailable or
failed probes remain conservative. The RTX 3090 Ti forced to FL9_1 accepted
the probe. The strict public test rendered two triangles in wireframe and
compared all 256 pixels with the same five edges drawn as explicit line
primitives: images matched, with 17 green pixels.

Verification at requested/actual FL9_1 on the physical RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false):

- Synthetic wireframe query/projection test: 1 passed.
- Strict public wireframe render/readback: 1 passed, 17/256 green pixels,
  exactly equal to explicit line output.
- Complete strict private HAL suite: 398 passed, 0 failed.
- Complete strict public D3D11 suite: 73 passed, 0 failed, with one optional
  Float16x2 IA subcase skipped because the selected driver did not report it.
- HAL and public Clippy passed; only prior HAL warnings remain.
- Rustfmt passed for changed pipeline, feature documentation and public test.
  The full capability file check still reports earlier unrelated 3D-format
  formatting drift, left untouched.
- `git diff --check` passed.

Commands (Visual Studio x64 developer environment, pinned Rust 1.93):

~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib dx11::capabilities::tests::polygon_mode_line_requires_a_positive_rasterizer_probe -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_query_gated_wireframe_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustup run 1.93 rustfmt --check --edition 2021 wgpu-hal/src/dx11/hal_pipeline.rs wgpu-types/src/features.rs wgpu/tests/dx11_public.rs
git diff --check
~~~

The contract audit remains partial. Feature-gated shader operations, compute
and storage, unsupported resource shapes and query types, surface portability,
historical FL9_1 hardware, Lumia/CoreWindow runtime, and physical device-loss
recovery remain open or unavailable.

## Latest increment — query-gated R8_UNORM mip chains

R8_UNORM mip chains now require the selected device to report TEXTURE2D | MIP
and an enabled backend path. NPOT dimensions additionally require positive
unrestricted-NPOT support. Attachment and copy-source combinations additionally
require the selected RENDER_TARGET query and the matching implemented path;
SHADER_SAMPLE separately controls filterability. Strict public rendering on
the RTX 3090 Ti at requested/actual FL9_1 sampled mip 2 from a four-level
upload, cleared mip 0 as a render attachment and read mip 0 back. All 64 RGBA
pixels and all 64 R8 texels matched. Multi-mip R8 texture-to-texture copies
remain unverified.

The initial full HAL run found one obsolete negative assertion in the existing
R8 upload test: it expected every R8 multi-mip creation to fail. The assertion
now follows the positive MIP query for R8 and continues to reject other formats
without an implemented mip path. The focused test passed after the correction.

Verification:
- Strict public R8 mip upload/sample/render/readback: 1 passed, 64/64 RGBA
  pixels and 64/64 R8 texels matched.
- Complete strict private D3D11 HAL suite: 402 passed, 0 failed.
- Complete strict public D3D11 suite: 75 passed, 0 failed.
- Selected DX11 adapter isolation test: 1 passed.
- DX12 core regression suite: 94 passed, 0 failed.
- Hardware: NVIDIA GeForce RTX 3090 Ti, vendor 0x10de, device 0x2203,
  driver 32.0.16.2101, physical adapter, software=false, requested/actual
  FL9_1; R8_UNORM format support 0x0002d3e0.

Commands:
~~~powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j4 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std dx11_scaffold_tests::requesting_dx11_exposes_only_fl91_hardware_adapters -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx12,wgsl,std -- --test-threads=1
~~~

The mission remains partial. Multi-mip R8 texture copies, broader public API
contract coverage, historical FL9_1 drivers, Lumia/CoreWindow execution and
physical device-loss recovery remain open or unverified.

### Follow-up — R8 mip-subresource texture copy

The query-gated R8 path now has a strict public GPU test copying a partial 2x3
region at mip 1 between separate four-level textures, using source origin
(1,0) and destination origin (1,1). A padded readback of the destination mip
verified all 16 texels and the preserved cleared border. This establishes a
mipmapped R8 texture-to-texture subresource copy at matching mip indices;
copies between different R8 mip indices remain unverified. The physical RTX
3090 Ti executed at requested/actual FL9_1. The complete strict public D3D11 suite passed 76/76 after adding this test.

## Follow-up result — R8 mip-subresource copy

After adding the partial mip-1 R8 texture-to-texture copy, the complete strict
public D3D11 suite passed 76/76 on the RTX 3090 Ti at forced FL9_1. The test
used a nonzero source origin and destination origin, a padded R8 readback, and
verified all 16 mip texels including the cleared destination border. Clippy and
targeted rustfmt passed; only the pre-existing HAL create_hal_view argument
count warning remains in the wgpu test Clippy output.

### Follow-up — cross-level R8 mip copy

The same strict GPU test additionally copies a 2x2 region from source mip 1 to
the full destination mip 2, then reads back all four texels. The RTX 3090 Ti
forced to FL9_1 passed the cross-level copy. This verifies mip-1-to-mip-2 on
the tested device; it does not establish every mip pair or historical-driver
behavior.

### Follow-up — R8 NPOT mip-chain GPU evidence

The strict public R8 sampled-mip test now selects a 6x10 texture when the
selected adapter positively reports unrestricted NPOT mip sampling. On the
RTX 3090 Ti forced to FL9_1, the query was positive; mip 2 sampling matched
64/64 RGBA output pixels and mip-0 attachment readback matched all 60 R8
texels. The path falls back to the POT test shape if the optional capability is
absent; synthetic tests continue to verify that absent/negative queries reject
NPOT chains.

## Default backend selection boundary — 2026-11-07

The public integration audit found that adding DX11 to `Backends::all()` also
made it part of `Backends::default()`. A build with the experimental feature
could therefore initialize and prefer restricted FL9_1 D3D11 for an ordinary
default `wgpu::Instance`. `Backends::default()` now excludes DX11 while
`Backends::DX11` and explicitly requested `Backends::all()` retain the backend.
Core initializes DX11 after the pre-existing backend sequence, preserving
their relative priority when callers explicitly request all backends.

Verification on the NVIDIA GeForce RTX 3090 Ti (0x10de:0x2203, driver
32.0.16.2101) at explicitly requested/actual FL9_1:
- `wgpu-types`: 93/93 unit tests passed, including the default-set test.
- Public default-instance regression: 1/1 passed; it exposed no D3D11 adapter.
- Strict public D3D11 suite: 77/77 test functions passed, 0 failed. The optional `Float16x2` IA case reported its expected capability skip.
- Strict core adapter-selection test: 1/1 passed and enumerated one physical FL9_1 adapter when DX11 was explicitly requested.
- DX12 core regression suite: 94/94 passed.
- Clippy passed for `wgpu`, `wgpu-core` and `wgpu-types`; existing HAL `create_hal_view` argument-count and `wgpu-types` unused-import/qualification warnings remain.
- `git diff --check` passed. Workspace formatting checks report pre-existing differences in unrelated lines/files; the changed type and wgpu API files were clean under targeted rustfmt. `wgpu-core/src/instance.rs` has unrelated pre-existing formatting differences.

Commands (run from the Visual Studio 2026 x64 developer environment):

```powershell
cargo +1.93 test --offline -j2 -p wgpu-types dx11_is_not_enabled_by_default
cargo +1.93 test --offline -j2 -p wgpu-types --lib
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public experimental_dx11_is_not_enabled_by_default -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11,wgsl,std requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx12,wgsl,std -- --test-threads=1
cargo +1.93 clippy --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --tests
cargo +1.93 clippy --offline -j2 -p wgpu-core --no-default-features --features dx11,wgsl,std --tests
cargo +1.93 clippy --offline -j2 -p wgpu-types --tests
git diff --check
```

An initial test invocation used an overly specific `--exact` filter and ran
zero tests; the corrected filter above ran the intended unit test.

This restores runtime opt-in for the experimental backend and does not change
the exposed capability matrix or complete the broader core/HAL contract audit.

## Strict upstream GPU-test harness — 2026-11-07

Added `dx11` opt-in features to the shared `wgpu-test` and `wgpu-info`
packages. The native GPU harness now rejects strict D3D11 runs unless
`WGPU_BACKEND=dx11`, a physical FL9_1 adapter exists in `.gpuconfig`, and the
backend filter leaves at least one selected adapter. The Naga capability test
now accepts the backend's stricter border-sampler rule because it does not
exercise border sampling.

Using Rust 1.93.1 in the Visual Studio x64 environment, the all-features
`wgpu-gpu` test binary built successfully. With strict mode enabled, the RTX
3090 Ti was listed as `[Executed]`; `naga_capabilities::validate_capabilities`
passed (1 executed, 0 failed). Its `.gpuconfig` entry reports vendor
0x10de/device 0x2203, physical DXGI adapter, FL9_1 and driver 32.0.16.2101.
A strict run with `WGPU_BACKEND=dx12` failed as intended before test discovery.
The capability test initializes a public wgpu test context but performs no
draw, so this is not counted as a rendering test. Existing dedicated strict
public rendering tests remain the evidence for GPU execution.

Commands:

```powershell
$env:WGPU_BACKEND = 'dx11'
$env:WGPU_INFO_SAVE_GPUCONFIG_REPORT = '1'
cargo +1.93 test --offline -j4 -p wgpu-info --all-features generate_gpuconfig_report -- --nocapture
cargo +1.93 test --offline -j4 -p wgpu-test --all-features --test wgpu-gpu --no-run
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j4 -p wgpu-test --all-features --test wgpu-gpu validate_capabilities -- --nocapture
```

`cargo-nextest` is not installed in this environment, so this verification used
the `wgpu-gpu` test binary through `cargo test` rather than `cargo xtask test`.
Historical drivers, Lumia runtime execution and the broad core/HAL contract
audit remain open.


## Query-gated R8G8B8A8_SNORM mip chains — 2026-11-07

The required DXGI format table guarantees `TEXTURE2D | SHADER_SAMPLE | MIP`
for R8G8B8A8_SNORM beginning at FL9_1. The internal capability model now
tracks that hardware guarantee separately from the backend path and the
selected-device `CheckFormatSupport` result. The public format projection
omits `SINGLE_MIP_ONLY` only when the selected device reports
`TEXTURE2D | MIP` and the backend path is enabled. `SHADER_SAMPLE` continues to
control filterability independently. NPOT chains still require positive
unrestricted-NPOT support. The resource remains sampled/upload-only; render
target, copy-source, and CPU-readback uses remain unavailable.

A strict public test uploads three padded RGBA8_SNORM mip levels, samples mip
1 through a literal `textureSampleBias`, and checks all 16 RGBA8 output pixels
against `[192, 64, 160, 96]` within one UNORM code value. The RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) returned support mask `0x000213a0`
with requested and actual FL9_1. R8G8_SNORM's existing three-level test was
also rerun and matched all 16 expected pixels.

Verification with Rust 1.93.1 and the Visual Studio x64 environment:

- Synthetic mip capability test: 1/1.
- Focused strict public RG8/RGBA8 SNORM mip tests: 2/2; both executed on the
  physical adapter.
- Complete private HAL suite: 402/402 test functions passed under strict mode.
- Complete strict public D3D11 suite: 79/79 passed. The optional Float16x2
  vertex-input subcase reported its expected capability skip.
- Explicit D3D11 core adapter-selection test: 1/1, enumerating one physical
  FL9_1 adapter. The public test `experimental_dx11_is_not_enabled_by_default`
  passed, preserving default backend isolation.
- DX12 core regression: 94/94.
- D3D11 HAL build, HAL and public-test-target Clippy, rustfmt check and
  `git diff --check` passed. Clippy emitted existing warnings in HAL test and
  view code; none pointed to the new mip path.

Microsoft reference: [Required DXGI formats](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/required-dxgi-formats).
The forced modern RTX result verifies the selected driver path, not every
historical FL9_1 implementation. The broad API and platform audit remains
unfinished.

## Surface lifecycle regression — reentrant present after device loss — 2026-11-07

A strict public fixture reproduced a consumed-frame leak when a device-lost
callback reentrantly presents a second acquired surface frame. The reentrant
call exits through `Surface::present`'s invalid-device check before reaching
`Queue::present`; because the public frame is already marked presented, its
drop does not discard the core acquisition. `Surface::present` now releases
the retained core frame on that lost-device exit. A separate queue-level
reentrancy cleanup remains for calls rejected after entering `Queue::present`.

On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`) at requested/actual FL9_1, the fixture injected
`DXGI_ERROR_DEVICE_REMOVED`, verified the callback, then reconfigured the
second surface with a healthy device and reacquired its frame. The test is a
synthetic device-loss injection; physical removal/recovery is not established.

Strict fixture results: 2/2 passed (Present callback/reentrant poll and
reentrant consumed-frame cleanup). Targeted Clippy passed with the existing
HAL `create_hal_view` argument-count warning. Rustfmt was applied to the two
changed Rust files; `git diff --check` passed. The focused regression initially
failed with `PreviousOutputExists`, confirming the retained frame, and passed
after the early `Surface::present` cleanup.

Continue the public core/HAL contract audit. The usable surface, resource and
shader subset remains deliberately restricted; historical FL9_1 hardware,
Lumia/CoreWindow runtime and physical device-loss recovery remain unverified.

## Optional R8G8_UNORM sampled mip path — 2026-11-07

Added an optional sampled/upload mip path for `Rg8Unorm`. Multi-mip creation,
`SINGLE_MIP_ONLY` projection and native `R8G8_UNORM` resource creation now
require both the selected device's `TEXTURE2D | MIP` query and the backend path.
`SHADER_SAMPLE` remains an independent filterability check, and NPOT mip chains
still require positive unrestricted-NPOT support. The CPU capability test
exercises positive, missing, unavailable and backend-disabled cases.

The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at requested/actual
FL9_1 reports `R8G8_UNORM=0x0002c320`, which lacks `D3D11_FORMAT_SUPPORT_MIP`.
The strict public test and controlled core fixture logged an explicit optional
capability skip; no RG8 mip draw is claimed on this adapter. The positive
driver-query path remains unverified on hardware. The previously existing
single-mip RG8 upload, filtering, attachment, copy and readback tests still
execute and pass.

Updating the controlled core fixture exposed stale expectations unrelated to
the mip predicate: the shared BC compression feature now projects queried
BC2/BC3 formats too, and the restrictive NPOT regression had disabled its
limit but not the matching downlevel flag. The fixture now checks BC2/BC3
against their selected HAL capabilities and clears the flag for the synthetic
NPOT rejection case. A second existing assertion now allows only the two
query-gated features `ADDRESS_MODE_CLAMP_TO_BORDER` and `POLYGON_MODE_LINE`.

Verification with Rust 1.93.1, Visual Studio x64, strict hardware mode and
bounded Cargo jobs:

- New synthetic RG8 mip capability test: 1/1 passed.
- Complete private D3D11 HAL suite: 403/403 test functions passed.
- Complete strict public D3D11 suite: 80/80 passed. The optional RG8 mip path
  logged SKIP because the selected device lacks MIP; the existing Float16x2
  vertex-input subcase also logged its selected-format skip.
- All controlled D3D11 core fixtures: 8/8 passed. The main fixture rendered
  its full set of representative workloads on the physical adapter and logged
  the expected optional RG8 mip skip; device-loss cases use injected errors.
- Targeted strict public RG8 mip test: test function passed with the explicit
  unsupported-capability SKIP; a strict positive RG8 mip execution was not
  possible because the driver query lacks MIP.
- HAL, core-fixture and public-test Clippy completed. Warnings were existing:
  `create_hal_view` has too many arguments; the core fixture reports older
  boolean, single-element-loop and collapsible-if suggestions. No new lint
  warning points to this increment.
- Rustfmt check and `git diff --check` passed.

This increment remains a query-gated capability model and implementation
extension, not evidence that the development RTX executes RG8_UNORM mip chains.
Continue the core/HAL contract audit and prioritize functionality that can be
verified on the available FL9_1 device.

## Public harness coverage audit — 2026-11-07

The opt-in test harness and public capability projection were checked again from
commit `53928a52a`. Strict GPU configuration generation recorded the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at FL9_1.
The all-features `wgpu-gpu` target compiled with the installed VS2017 x64 ATL
library added to `LIB` for that command because VS2022 Build Tools lacks
`atls.lib`; no toolchain or repository configuration was changed. The full
strict target reported 351 test cases without failures, but only
`naga_capabilities::validate_capabilities` executed. The other 330 were marked
unsupported by the adapter's required limits/downlevel properties and 20 were
backend-skipped. Filtered generic transfer, texture-view and render-target
suites likewise performed no GPU operations. These counts are harness outcomes,
not 351 hardware passes; backend-specific `dx11_public` readback tests remain
the broad rendering evidence.

A read-only Lumia Device Portal request for `/api/os/devicefamily` timed out
from Ben-Desktop; TCP port 443 was unreachable. No device settings were
changed. Lumia ARM32 runtime verification remains unavailable in this session.

The transfer audit reconfirmed that the 10Level9 FL9_1 context contract
forbids 3D texture GPU copies and GPU-to-CPU 3D copies, and forbids copying
mipmapped volume textures. The projection's `texture_3d_texture_copies = false`
is a hardware-bound restriction, not an unimplemented native copy path. See
Microsoft's [CopySubresourceRegion FL9_x rules](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/nf-d3d11-id3d11devicecontext-copysubresourceregion).

The backend remains a limited prototype with substantial explicit restrictions;
continue the core/HAL contract audit and selected-device verification. The
historical FL9_1 driver matrix, Lumia runtime, physical device-removal recovery
and exhaustive public API audit remain open.

## Vertex system-value emulation boundary — 2026-11-07

The native FXC failures for `SV_VertexID` (X4555) and `SV_InstanceID` (X4016)
were already covered. I tested whether the current instance-stream machinery
could supply `vertex_index` through a synthetic float input instead. Exact
`u32` reconstruction from float halves fails under `vs_4_0_level_9_1` with
FXC X3548: dynamic `uint` use is restricted to known-positive values. The same
HLSL compiles under the `vs_5_0` control profile. Added a permanent compiler
regression for this boundary; the adapter continues to reject both system
values before HAL. This is a tested profile limitation, not proof that the
underlying FL9_1 hardware is incapable of an alternative encoding.

## Follow-up compiler probe — custom unsigned IA semantic — 2026-11-07

The float reconstruction result does not rule out every IA-based emulation.
Naga's HLSL writer now has an opt-in `fl9_instance_index_input` mapping that
emits `NAGA_INSTANCE_INDEX` for direct and struct-member inputs. A Naga writer
test checks the default remains `SV_InstanceID`; a HAL/FXC test compiles both
generated forms to DXBC under `vs_4_0_level_9_1`. The D3D11 call site does not
enable the mapping and has no host-value stream, slot reservation or per-draw
update yet. Both public limits remain false. The next step is to carry the
exact `first_instance + instance` value through all split/replay paths and
prove the result with GPU readback before changing capability reporting.

Naga's focused writer test passed 1/1, and the HAL test compiled both generated
WGSL forms through FXC to DXBC (1/1). `cargo clippy -p naga --tests` and the
DX11 HAL test Clippy run passed; HAL reported ten existing warnings outside
the new code. Rustfmt and `git diff --check` passed. No GPU operation or
capability promotion is claimed for this compiler-only step.

## Host-emulated `instance_index` — 2026-11-07

The private D3D11 pipeline now supports WGSL `@builtin(instance_index)` under
the FL9_1 shader profile. Since FXC rejects `SV_InstanceID` for 10Level9,
Naga's opt-in lowering reads the selected entry point's value from reserved
vertex constant register b13. The existing viewport transform occupies the
first 16-byte register in the same 32-byte constant buffer. Command replay
updates the second register to `first_instance + emulated_instance` before
each draw; direct indexed, rewritten-index and non-indexed paths carry the
required bindings. Core validation allows the builtin and supported
u32-to-f32 conversion while continuing to reject unrelated integer
operations. `vertex_index` remains unavailable.

With `WGPU_DX11_REQUIRE_HARDWARE=1`, the strict public test
`public_dx11_instance_rate_vertex_streams_at_fl91` passed on the NVIDIA GeForce
RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested and
selected at FL9_1 (software=false). Per-instance vertex-stream draws produced
their expected IDs in the blue channel. The new strict test
`public_dx11_instance_index_nonindexed_and_indexed_draws_at_fl91` also passed:
a non-indexed draw with `first_instance=2` and a direct indexed draw with
`first_instance=4` produced distinct left/right colors, and 128/128 pixels
matched within one code value. Naga writer tests and FXC compilation covered
direct, struct-only and mixed location/builtin signatures. Naga unit test,
HAL FXC test, core validation test and both focused hardware tests passed.

Verification with pinned Rust 1.93.1 and the Visual Studio x64 environment:
the private HAL suite passed 406/406, the strict public D3D11 suite passed
81/81, the core adapter-isolation test passed 1/1, the new core integer-gate
test passed 1/1, and the complete Naga library suite passed 136/136. The
DX12 core regression passed 94/94. The full private suite included the FXC
tests for direct, struct-only and mixed location/builtin signatures. The
focused hardware tests separately verified actual instance-index-dependent
readback on the RTX. Clippy and formatting checks are recorded after the final
diff review below.

The forced modern NVIDIA driver does not prove historical FL9_1 compatibility.

Reproduction commands from a Visual Studio x64 developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl91_integer_semantics_gate_follows_selected_entry_and_helper_calls -- --nocapture
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib requesting_dx11_exposes_only_fl91_hardware_adapters -- --nocapture
cargo +1.93 test --offline -j8 -p naga --lib
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx12 --lib
cargo +1.93 build --offline -j8 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 clippy --offline -j8 -p naga --tests
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustup run 1.93 rustfmt --edition 2021 --check naga/src/back/hlsl/mod.rs naga/src/back/hlsl/writer.rs wgpu-core/src/validation.rs wgpu-hal/src/dx11/capabilities.rs wgpu-hal/src/dx11/command.rs wgpu-hal/src/dx11/hal_encoder.rs wgpu-hal/src/dx11/hal_index.rs wgpu-hal/src/dx11/hal_pipeline.rs wgpu-hal/src/dx11/hal_shader.rs wgpu-hal/src/dx11/shader.rs wgpu-hal/src/dx11/viewport.rs wgpu-hal/src/dx12/device.rs wgpu/tests/dx11_public.rs
git diff --check
```

All commands passed. Clippy reported three new `too_many_arguments` warnings
on functions extended to carry instance-index state, plus the existing HAL
warnings (`create_hal_view`, test-only range construction, item ordering and
needless borrow). No Naga or core lint warning was reported. The public suite
also passed its regression that confirms default wgpu enumeration still does
not expose D3D11.

Checkpoint `225007ef5` contains the verified host-emulated `instance_index`
implementation. The earlier implementation status and testing notes that
reported this builtin as rejected have been marked as superseded; the current
strict evidence above is the source of truth. The next selected shader gap is
`vertex_index`, whose implementation must preserve source IDs through the
backend's rebased and gathered index paths.

## Host-emulated `vertex_index` for FL9_1 TriangleList — 2026-11-08

The private D3D11 path and opt-in wgpu projection now support the selected
vertex entry point's `@builtin(vertex_index)` through Naga's opt-in custom
float IA semantic. Core permits direct `u32`-to-`f32` conversion from this
input and rejects other integer runtime operations. Pipeline creation reserves
a native input slot and initializes an identity float stream through the
FL9_1 maximum direct vertex index (65,534). Nonindexed `first_vertex` ranges
above that limit are replayed with immutable per-draw float streams; user
vertex streams are rebound at the matching absolute byte offset. Existing
indexed translation also carries the resolved unsigned vertex IDs through
rebased Uint16 windows and wide-triangle gather paths. Negative resolved IDs
retain their u32 bit-pattern value for the supported conversion semantics.
The emulation is accepted only for TriangleList pipelines; at this checkpoint
other topologies failed at HAL pipeline creation because core limits could not
express the restriction. A later change adds a topology mask and core gate.
Direct integer arithmetic remains rejected. Temporary buffers
are created during command replay and retained through each draw; this adds
per-draw CPU allocation and native resource creation for remapped IDs.

A strict public GPU test requested and asserted FL9_1 on the physical NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`; software=false).
It verified complete 8x8 readbacks for a normal nonindexed draw, a degenerate
indexed draw, negative base vertex, reindexed Uint32 IDs 100000–100002,
nonindexed `first_vertex=100000`, and a normal draw after temporary remapping.
Observed red-pixel counts were 32/0/12/32/32/32. The high-ID test confirms the
custom stream and user-stream offset logic for this case; it is not evidence
for every large range or historical driver.

Validation after the implementation and argument-option cleanup:

- Strict public D3D11 suite: 82 passed, 0 failed, on the physical RTX at forced
  FL9_1. One optional Float16 vertex-format subcase logged a capability skip;
  no optional format execution is claimed.
- Private D3D11 HAL suite: 409 passed, 0 failed, including strict FL9_1 GPU
  execution cases.
- Naga library: 137 passed, including writer tests for the opt-in semantic.
- Core FL9 integer-semantics gate: 1 passed; core zero-public-DX11-adapter
  isolation: 1 passed.
- DX12 core regression: 94 passed.
- DX12 HAL regression: 4 passed.
- After the final shader-option refactor, the D3D11 HAL check, focused strict
  GPU readback and FXC semantic test passed; the complete public and HAL suites
  above were then rerun successfully.

Clippy passed for Naga, DX11 core, DX11 HAL and the public D3D11 integration
suite. The HAL reports existing `too_many_arguments`, test range-vector,
item-order and needless-borrow warnings; the public test reports its existing
`std::HashMap` warning. No warning was introduced by this increment. Rustfmt
and `git diff --check` passed. The forced modern RTX driver cannot establish
behavior on historical FL9_1 hardware. Public topology-specific core
validation remained a gap at this checkpoint and was closed by the later
topology-mask change recorded below.

Reproduction from Visual Studio 2022 x64 with Rust 1.93.1:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p naga --lib
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl91_integer_semantics_gate_follows_selected_entry_and_helper_calls
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib requesting_dx11_exposes_only_fl91_hardware_adapters
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib
```

## Direct vertex-stage uniform-array indexing — 2026-11-08

Added `DownlevelLimits::vertex_uniform_array_indexing`, defaulting to false and
enabled only by the opt-in FL9_1 D3D11 projection. Core and HAL analyze the
selected entry point and permit only direct `vertex_index` or `instance_index`
access to fixed-size arrays of `vec3` or `vec4` in vertex-stage uniform data.
Computed indices, helper-mediated indexing, fragment-stage accesses, nested
arrays, integer elements and unsupported pointer flows remain rejected. Core
and HAL retain the same narrow acceptance contract.

FXC's `vs_4_0_level_9_1` compiler rejected the default unsigned clamp with
X3548. Naga's HLSL `BindTarget` now has a serde-defaulted opt-in signed clamp;
the D3D11 FL9_1 binding map enables it only for uniform resources, while other
backends preserve their previous output. A raw signed-clamp control and all
generated `vec3`/`vec4` and `instance_index` shaders compile under FXC. Naga
writer tests verify signed output, unchanged default unsigned output and the
signed bound's representable range.

The strict public test requested and selected FL9_1 on the physical NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false).
It indexed four sparse-bound uniform values with `vertex_index`, checked the
distinct output for each, and confirmed index 4 selects the final array value.
This is positive execution on a modern driver constrained to FL9_1; it does not
establish historical hardware behavior.

Verification with Rust 1.93.1 from the Visual Studio x64 environment:

- Strict public D3D11 suite: 84 test functions passed, 0 failed. Optional
  Float16x2 IA and RG8_UNORM mip subcases printed capability skips.
- Strict private D3D11 HAL suite: 411 passed, 0 failed; physical FL9_1 GPU
  operations executed throughout the suite.
- Naga library: 139 passed, including both signed-clamp writer tests.
- Core selected-entry uniform-index tests: 2 passed; D3D11 adapter isolation:
  1 passed.
- DX12 core library: 97 passed; DX12 HAL library check passed.
- Focused strict uniform-array GPU test: 1 passed with all expected values.

The direct indexed-array subset is now implemented and tested. Helper-mediated
indices, computed index expressions, fragment-stage accesses, unsupported
array layouts, historical FL9_1 behavior and the broader core/HAL contract
audit remain open. The code changes are uncommitted in the current worktree.

## Surface queue identity and frame release — 2026-11-08

Core now releases an acquired frame when `Surface::discard` finds its
configured device invalid. The `Queue::present` early-error paths use a shared
cleanup helper that drops the presentation lock before attempting
discard/release. Public queue dispatch now passes the receiver queue ID to
core, so its existing device-identity check rejects a queue from another
device and consumes/releases the frame. The strict D3D11 HWND fixture verifies
the mismatch error, reconfiguration and successful reacquisition with the
matching device. It also keeps a second acquired frame alive while synthetic
`DXGI_ERROR_DEVICE_REMOVED` invalidates its device, drops that frame, then
reconfigures the surface to a healthy device and successfully acquires again.
Queue presentation checks device validity before entering the present guard,
so the existing reentrant lost-device path releases its frame without
reporting a spurious invalid-surface error. The injected HRESULT is not
physical device-loss/recovery evidence.

The strict fixture passed 2/2 on the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) at requested and actual FL9_1. It
covers wrong-device rejection, matching-queue reacquisition, the synthetic
Present-loss callback/reentrant-poll case, reentrant presentation after loss,
and dropping an unpresented frame after loss. The initial lifecycle cleanup is
checkpointed at `a7205baf35fe2799a4eb13b5fca9b129068d55b3`; queue identity is
checkpointed at `d890ee6bb15537eddca530c9fd5b25b26836c9fc`. The Deno surface
API retains configured-device presentation while the native wgpu `Queue`
entry point carries its receiver queue identity into core. `cargo +1.93 check
--offline -j8 -p deno_webgpu` passed under the Visual Studio x64 environment.
That workspace check exposed a missing GLES initializer for the added
`DownlevelLimits::vertex_index_topology_mask`; GLES now explicitly retains all
topologies in `b62b88464633122acb9134b92a340c21aa909a87`. Rustfmt,
`git diff --check`, and targeted public-fixture Clippy passed; Clippy reported
only the existing HAL `too_many_arguments` warnings.

## PointList `vertex_index` support — 2026-11-08

The earlier TriangleList-only implementation now supports PointList as well.
The backend emits the effective vertex ID through its existing hidden float
attribute path for direct and indexed point draws, preserving first vertex,
index values and base-vertex resolution. Core receives a topology capability
mask and rejects unsupported `vertex_index` topologies before HAL pipeline
creation; ordinary LineList pipelines remain valid. A private replay test
checks resolved point IDs, including the signed-base-vertex case.

On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), the public test requested and asserted FL9_1. It rendered three
direct points and one repeated indexed point; readback matched the exact
expected locations. The strict private HAL suite passed 409/409 and the
complete strict public D3D11 suite passed 83/83. The core hardware fixture
passed 1/1, core zero-D3D11-adapter isolation passed 1/1, and DX12 core
regression passed 95/95. Optional Float16x2 vertex input and RG8_UNORM mip
subcases were reported as capability skips. Historical FL9_1 devices remain
unverified.

Rustfmt and `git diff --check` passed. Public D3D11 Clippy passed at `-j2`;
the `-j8` attempt exhausted the Windows page file (error 1455), then the
bounded retry succeeded. Remaining warnings are pre-existing argument-count
warnings in the HAL and a pre-existing `std::HashMap` warning in the public
test. Exact reproduction commands are in `FL9_1_HANDOFF.md`.

## Core validation for TriangleList-only `vertex_index` — 2026-11-08

The public capability projection now carries a topology mask in
`DownlevelLimits`. D3D11 FL9_1 exposes `vertex_index` only for TriangleList,
matching the backend's custom float IA semantic and indexed replay paths. Core
uses the selected vertex entry point to detect the builtin, then rejects an
unsupported topology with a validation error before HAL pipeline creation. The
HAL's TriangleList check remains as a defensive boundary. Other backends retain
their full default topology mask; old serialized limits default to all known
topologies.

Verification on the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, software=false), with requested and actual FL9_1:

- `wgpu-types` library: 94 passed, including topology-mask subset and default
  compliance checks.
- Core topology-validation unit test: 1 passed.
- D3D11 capability-projection unit test: 1 passed.
- Strict core hardware fixture: 1 passed. It rejected a LineList pipeline using
  `vertex_index` at core validation, accepted TriangleList `vertex_index` and
  `instance_index` pipelines through HAL, and completed its existing textured,
  depth, buffer, mip and draw cases. One optional RG8_UNORM mip case was
  explicitly skipped because the selected device reported no MIP support.
- Existing public TriangleList readback remains the GPU evidence for
  vertex-index-dependent output; this fixture increment adds validation-boundary
  evidence, not a new draw result.

The initial compile identified exhaustive `DownlevelLimits` literals in tests
and the Noop backend that needed the new field; those were updated. The
`wgpu-types` test build continues to report its pre-existing unused serde-import
and qualification warnings. Historical FL9_1 topology behavior remains
unverified.

Additional verification for this increment:

- Strict private D3D11 HAL suite: 409 passed, 0 failed, with strict hardware
  mode enabled. The suite exercised the RTX 3090 Ti under explicitly requested
  FL9_1; optional format and shader-negative cases retained their existing
  explicit skip output.
- Complete strict public D3D11 suite: 82 passed, 0 failed on the same adapter
  at requested/actual FL9_1. Two optional subcases explicitly skipped: the
  Float16x2 IA format query and Rg8Unorm mip support. The public validation
  test accepted TriangleList vertex builtins, rejected LineList `vertex_index`
  through a wgpu validation error, and accepted ordinary LineList pipelines.
- DX12 core regression: 95 passed, 0 failed.
- Public D3D11 library feature check and strict TriangleList readback: passed;
  the readback checked six indexed/non-indexed and remapping cases on hardware.
- Core D3D11 zero-adapter isolation test: 1 passed.
- `wgpu-types` Clippy, DX11 core Clippy and DX11 HAL Clippy completed. Warnings
  are existing serde imports/qualifications, helper argument counts and test
  style warnings outside this change.
- Rustfmt check and `git diff --check` passed.

The strict public regression can be reproduced with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

Implementation checkpoint: `2e2bd70bb038ea09ca7e6eb5fe60dd5630c18867`
(`dx11: validate vertex index topology in core`); public regression checkpoint:
`732c629a1788a316512f20097ce450c99d4e9449`
(`dx11: test public vertex index topology validation`).

The new core behavior can be reproduced with:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-types --lib vertex_index_topology_masks_are_subset_capabilities
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib emulated_vertex_index_rejects_unsupported_topology_during_core_validation
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_wgpu_projection_is_conservative_and_uses_only_implemented_paths
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib
```

## Checked dynamic local-array validation — 2026-11-08

Core previously accepted a checked dynamic non-vector local-array access when
the selected shader entry point also referenced a uniform, then the D3D11 HAL
rejected it later during shader compilation. Core now retains each shader
module's `ShaderRuntimeChecks` and applies the same checked-mode predicate as
the HAL. The selected-entry analysis rejects those local-array accesses before
HAL only when checks are requested and that entry point uses a uniform; local
array-only shaders, unchecked modules and safe sibling entry points retain
their previous core behavior. Uniform-array access remains governed by the
separate `vertex_uniform_array_indexing` capability check.

CPU tests cover direct and helper-reached local array reads with a uniform,
local-only reads, unchecked access and selected-entry isolation. The core
focused tests passed 2/2; the strict public D3D11 suite passed 84/84 on the RTX
3090 Ti at requested/actual FL9_1, and DX12 core passed 97/97. This is a
validation-boundary correction, not an increase in supported dynamic indexing.

## FL9_1 emulated vertex input-slot preflight — 2026-11-08

The HAL emulates `vertex_index` through one hidden float IA stream, but core
previously checked only the logical vertex-buffer count and topology. FL9_1
also spills overlapping attributes into extra physical slots. Core now uses
`DownlevelLimits::vertex_index_input_requires_hidden_slot` and mirrors the
HAL's allocation order: reserve populated logical streams, assign attributes
by byte-range overlap, then require one free slot for the hidden stream. The
check runs only for a selected entry point that uses `vertex_index`; normal
pipelines retain their existing layout allowance. HAL keeps its final
allocation check as defense in depth.

The production allocator tests cover sixteen populated streams (reject),
fifteen streams plus one overlap spill (reject), fourteen streams plus one
spill (accept), sixteen nonoverlapping attributes in one stream (accept), and
sixteen overlapping attributes in one stream (reject). A strict public
validation test confirms core rejects the full-slot builtin pipeline with a
specific validation error, then accepts the identical layout without
`vertex_index`.

Verification with Rust 1.93.1 and the Visual Studio x64 environment:

- `cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib emulated_vertex_index_requires_a_free_physical_input_slot`: 1 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_accepts_emulated_vertex_and_instance_index_at_fl91 -- --nocapture --test-threads=1`: 1 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested/actual FL9_1, software=false. This test creates pipelines and validates the boundary; it does not submit a draw.

Initial compiles found missing exhaustive limit initializers in Noop and the
controlled core fixture, plus array-size annotations needed in the new public
test; these were corrected. The remaining work is broader selected-entry
shader coverage, resource/render-state audits and historical FL9_1/Lumia
verification. A read-only contract audit found no additional concrete
core-valid texture/view/transfer/render-pass mismatch in its scoped review.

Follow-up verification after integrating the capability and core gate:

- Strict public D3D11 suite: 84/84 passed on the same physical adapter; two
  optional subcases logged explicit skips (Float16x2 IA and RG8_UNORM mips).
- Strict private D3D11 HAL suite: 411/411 passed. It performed real GPU work
  throughout; shader-feature diagnostics and intentionally unsupported local
  indexing cases remained explicit, expected rejections.
- Core DX11 library: 99/99 passed. The public suite also confirmed that
  ordinary default backend selection still excludes D3D11.
- Core DX12 library: 98/98 passed.
- `wgpu-types` hidden-slot subset test: 1/1 passed; GLES HAL check passed.
- Focused HAL dynamic-uniform analyzer/FXC test: 1/1 passed after a Clippy
  cleanup; focused strict public uniform-array GPU test: 1/1 passed after that
  refactor and verified all four values on the RTX at requested/actual FL9_1.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.
- Core Clippy passed. HAL Clippy passed with existing range-literal,
  argument-count, test-order and needless-borrow warnings; no warning remains
  for the new shader-analysis helper.

The first HAL test link attempt returned LNK1114 because a temporary linker
file was momentarily locked; the bounded `-j2` retry passed. No test was
counted as a hardware skip-success in strict mode.

## Checkpoint — selected-entry vertex-index contracts — 2026-11-08

Commit `1fd14a6eac95d88204ef56106ae423d59ba3200f` records the direct
vertex-uniform-array indexing path, core/HAL parity for checked dynamic local
array accesses, and core preflight for the hidden vertex-input slot consumed
by emulated `vertex_index`.

The latest-tree focused rerun used the installed Visual Studio 2022 x64
environment and passed:

- Core `emulated_vertex_index_requires_a_free_physical_input_slot`: 1/1.
- Core `restricted_vertex_uniform_array_indexing_is_selected_entry_aware`:
  1/1.
- HAL `dynamic_uniform_array_reads_allow_direct_vertex_builtins`: 1/1,
  including the FL9_1 shader compilation checks.
- Strict public `public_dx11_vertex_uniform_array_indexing_at_fl91`: 1/1 on
  NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`,
  software=false, requested/actual FL9_1. Readback verified all four uniform
  entries and the out-of-range clamp to the last entry.
- `git diff --cached --check`: passed before commit.

Earlier in this increment, the complete strict private HAL, public D3D11,
core DX11, core DX12 and focused GLES checks passed as recorded above. A
read-only resource/format/view audit found no concrete core-valid request
reaching an unsupported HAL path; broader format-matrix coverage and
historical hardware remain open. The next shader candidate is one-hop direct
builtin forwarding through helper parameters, pending the independent shader
audit. This checkpoint does not close the public contract or platform audit.

## Follow-up — helper-index, surface lifetime and input-offset checkpoints — 2026-11-08

Three focused code checkpoints followed the earlier report:

- `e211639de` supports unchanged forwarding of `vertex_index` and
  `instance_index` through reachable helper parameters when each proven value
  is used only to index a fixed-size `vec3` or `vec4` vertex uniform array.
  Naga receives an explicit function/argument rewrite map; core and HAL derive
  matching selected-entry provenance. Computed indices, integer arithmetic,
  other array element types and fragment-stage indexing remain rejected.
- `bd024d74e` keeps surface backbuffers alive while derived views remain,
  rejects reconfiguration until those views are released, waits the previous
  device queue before same-adapter A→B surface handoff, and updates the public
  configuration cache according to whether the old configuration survived.
  Synthetic device loss is exercised; physical device removal/recovery is not.
- `473d14d9f` removes the HAL's extra four-byte attribute-offset rejection.
  Core continues enforcing its `min(format.size(), 4)` alignment; optional
  formats still require the selected device's `IA_VERTEX_BUFFER` query. The
  new public test rejects a one-byte-misaligned `Unorm8x2` attribute and draws
  from `Unorm8x2` at byte offset two when supported. The RTX forced to FL9_1
  reports no supported sub-four-byte vertex formats, so only the positive
  draw was skipped; no native offset-2 GPU success is claimed.

Latest-tree verification used Rust 1.93.1 with the Visual Studio 2022 x64
environment and bounded Cargo parallelism:

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -q -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 411 passed, 0 failed. The strict suite includes real FL9_1 GPU work as well as synthetic and CPU-only tests.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 85 passed, 0 failed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, software=false, requested/actual FL9_1. Three optional subcases logged skips: `Unorm8x2` IA support, `Float16x2` IA support and `Rg8Unorm` mip support. The final test readback validates helper-forwarded vertex/instance indices; the new offset test's core rejection passes but its positive offset render skips on the selected-device query.
- Strict HWND lifecycle fixture: 2/2 passed on the same adapter, including retained-view reconfigure blocking and queue handoff. Device-loss behavior in this fixture is injected/synthetic.
- Core DX11 library: 100/100; core DX12 library: 99/99; Naga library: 140/140.
- Focused Naga rewrite, HAL analyzer/FXC and core provenance tests each passed 1/1. The focused strict helper-index GPU readback passed 1/1 and validated direct plus two-helper paths.
- HAL and public-test Clippy passed. Existing warnings remain for HAL argument counts and the public test's `std::collections::HashMap` lint; no new warning was reported for this change. `rustfmt --check` passed for the complete HAL pipeline file and the new public test function snippet; `git diff --check` passed. The repository-wide format check was not rerun for this checkpoint.

Independent read-only audits found that most pipeline gates are deliberate and
core-checked; one cross-layer mismatch, the vertex-offset gate, is now fixed.
The shader audit recommends testing `array<vec2f, N>` vertex uniform indexing
only after FXC confirms its level-9 packing and clamp behavior. Remaining
mission blockers are the exhaustive public core/HAL acceptance audit, shader
subset gaps, optional format coverage, historical FL9_1 drivers, actual
device-loss recovery and Lumia/CoreWindow runtime verification. Ordinary
backend defaults still omit D3D11; explicit `Backends::DX11` is experimental
and opt-in, with a constrained HWND presentation path.

## FL9_1 validation and lifecycle corrections — 2026-11-08

Commit `3f3fd4fd0f9855ebbd8a4c6e47f77636307b092a` closes three reproduced
core/HAL contract gaps. Core now has a separate selected-entry capability gate
for fragment `@builtin(sample_mask)` input; the `ps_4_0_level_9_1` FXC test
reports `X4502: invalid ... input semantic 'SV_Coverage'`. The existing output
gate remains independent. Public HWND acquisition now returns validation for
duplicate and wrong-thread use, leaves an outstanding first frame usable, and
does not misreport these caller errors as surface loss. Storage-only buffers,
which core can validly allocate even though this FL9_1 backend has no storage
binding, now use the existing native dummy-bind path rather than failing
allocation.

Verification used Rust 1.93.1, the Visual Studio x64 environment and bounded
Cargo parallelism:

- `cargo +1.93 test --offline -j8 -p wgpu-types --lib`: 97 passed.
- `cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib`: 109 passed.
- `cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib`: 100 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 412 passed, with strict hardware required.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 85 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, software=false, requested/actual FL9_1. Three optional selected-device subcases logged skips: `Unorm8x2` IA, `Float16x2` IA and `Rg8Unorm` mip support.
- Focused `wgpu-types` sample-mask, core selected-entry, and strict HAL FXC tests passed 1/1 each; the public sample-mask input/output validation and HWND lifecycle tests passed within the full public run. The strict suite's rendering/readback cases exercised the physical GPU; the FXC failure case validates compilation diagnostics rather than a draw.
- HAL, core and public-test Clippy completed successfully with existing warnings documented in the preceding audit; changed Rust files passed `rustfmt --check`, and `git diff --check` passed.

This increment fixes specific mismatches; it does not close the broader public
contract audit. Storage shader access remains unavailable, and sample-mask
input remains unsupported at this shader profile. Historical 9_1 drivers,
physical device removal/recovery and Lumia/CoreWindow runtime remain
unverified. Continue with independently reproduced public-valid requests and
selected-entry shader/render-state coverage.

## Selected legacy shader built-in gates — 2026-11-08

The core/HAL audit found four legacy interface built-ins that optional GLSL or
SPIR-V frontends can express, but the selected FL9_1 HLSL writer cannot lower:
vertex `BaseVertex`, `BaseInstance`, `PointSize`, and fragment `PointCoord`.
Core previously accepted those selected pipelines and deferred failure to HAL.
`DownlevelLimits` now models each restriction; the D3D11 FL9_1 projection sets
them false, and core rejects only selected entry points whose validated
interfaces contain them. Ordinary sibling GLSL entries remain pipeline-valid.
No shader behavior is emulated or promoted.

Verification with Rust 1.93.1 and the Visual Studio x64 environment:

- `cargo +1.93 test --offline -j16 -p wgpu-types --lib`: 98 passed.
- `cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib`: 110 passed, including the selected-entry/direction test.
- `cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib`: 101 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1`: 86 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, physical adapter, requested/actual FL9_1. Three optional selected-device subcases skipped: `Unorm8x2` IA, `Float16x2` IA, and `Rg8Unorm` mip support. The new test performs validation and ordinary pipeline creation; other tests in this run perform real GPU rendering/readback.
- Focused strict GLSL test passed 1/1. The complete strict private HAL suite passed 412/412 with `WGPU_DX11_REQUIRE_HARDWARE=1`.
- Clippy passed for `wgpu-types`, `wgpu-core` tests, `wgpu-hal` tests, and the public test target. Existing warnings remain: unused serde imports and qualifications in `wgpu-types`, existing test-style warnings in core, D3D11 too-many-arguments/single-range warnings in HAL, and a disallowed standard HashMap in an older public test.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

The shader audit finding is closed for these four selected-entry inputs. It
does not increase the supported FL9_1 shader subset. Public D3D11 remains
opt-in. The broader API contract audit, historical-driver validation,
physical device-removal recovery, and Lumia/CoreWindow runtime verification
remain open; independent shader, resource, and adapter audits are continuing.

## Matrix-inverse validation and bounded buffer readback — 2026-11-08

Commit `41c161fd255b4062094c8ed986994abd76ba1e73` adds `DownlevelLimits::matrix_inverse` and selected shader
validation for an operation Naga accepts from GLSL/SPIR-V but its current HLSL
and MSL writers do not lower. Core checks the selected entry point and regular
module functions; the HLSL writer emits every regular function, so an otherwise
unused helper containing inverse must also be rejected. DX11, DX12 and Metal
project the operation as unsupported. The public regression rejects both a
selected inverse operation and an inverse-bearing unused helper before HAL,
while a separate ordinary shader module still creates a pipeline. CPU coverage
includes helper reachability, stages and a backend projection that permits the
operation.

Commit `98e1c2bbf27e2732ed0948e119c35f2fd658bbf8` changes D3D11 mapped-buffer readback to allocate a staging
buffer for the validated map/invalidation range, copy that non-null source box
from the default buffer into staging offset zero, and read back only those
bytes. The strict mapping test observes 16-, 8- and 16-byte staging allocations
for the tested mapped ranges and validates returned data. Microsoft documents
buffer copy-box coordinates in bytes and the FL9_x copy corruption warning for
staging-to-default copies under specific conditions; this path is default to
staging with a bounded explicit box. This reduces transient staging allocation
for small maps; no throughput benchmark was run.

Verification with Rust 1.93.1 and the Visual Studio x64 environment:

- `cargo +1.93 test --offline -j16 -p wgpu-types --lib`: 99 passed.
- `cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib`: 111 passed.
- `cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib`: 101 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 412 passed, including the bounded-range readback test.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1`: 87 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, physical adapter, requested/actual FL9_1. Three optional device-specific subcases logged skips; those are not counted as GPU feature passes. Public rendering/readback tests in the suite executed real hardware work; the matrix-inverse cases exercise core validation only.
- `cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx12`: passed.
- Clippy passed for `wgpu-types`, core tests, HAL tests and the public D3D11 target. Existing warnings include serde imports/qualifications in `wgpu-types`, core fixture style warnings, HAL argument-count/single-range/item-order warnings, and an older public-test `std::collections::HashMap` warning. No new matrix-inverse lint warning was reported.
- `cargo +1.93 fmt --all -- --check` and `git diff --check`: passed.

The two code commits are followed by a documentation checkpoint recording this
evidence. This closes only the two scoped findings, not the completion mission.
Public DX11 remains explicit opt-in; no Core WebGPU conformance, historical
driver compatibility, Lumia runtime or physical device-removal recovery is
claimed. Continue with separate hardware/shader/core audits and prioritize
unimplemented ordinary graphics functionality over expanding test counts.

## Follow-up — selected shader operation, empty unmap and resolve mip contracts — 2026-11-08

Three reproduced gaps were closed. Naga's HLSL writer does not lower
`MathFunction::Outer` (`outerProduct`), although WGSL/GLSL validation can
accept it. `DownlevelLimits::matrix_outer_product` now represents that writer
restriction; DX11 and DX12 disable it, and core rejects it in the selected
entry point plus regular module functions emitted by the HLSL writer. Public
GLSL regressions cover a selected operation, an otherwise unused helper and a
valid ordinary sibling pipeline. The shared HLSL writer limitation is also
projected by DX12 downlevel limits; Metal's MSL writer can lower the operation
and retains its positive projection.

Core empty buffer mappings use a zero-length dangling pointer and never
establish a native HAL mapping. `Buffer::unmap` now omits the HAL unmap call for
empty ranges. Strict core fixtures cover both a zero-sized mapped-at-creation
buffer and an empty asynchronous map of a nonempty buffer. The D3D11 render
resolve command now records both native subresource indices from the selected
views instead of assuming mip zero. A strict FL9_1 HAL test resolves a
multisampled clear into mip 1, reads back mip 0 and mip 1, and verifies that
the red mip-1 result is correct while the green mip-0 contents remain intact.

Verification used Rust 1.93.1, the Visual Studio x64 environment, offline
dependencies and bounded Cargo parallelism:

- `cargo +1.93 test --offline -j16 -p wgpu-types --lib matrix_outer_product_is_a_downlevel_shader_capability`: 1/1 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib -- --test-threads=1`: 112/112 passed, including selected-entry shader validation and strict physical-device mapping fixtures.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 413/413 passed, including actual FL9_1 resolve-to-mip-1 execution and readback.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1`: 87/87 passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`, physical hardware, requested/actual FL9_1. Three optional device-specific subcases logged skips (R8G8Unorm mip, R8G8Unorm vertex IA and Float16x2 vertex IA); those are not feature passes.
- `cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib`: 101/101 passed. `cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx12` and the corresponding `--features gles` check passed. `cargo +1.93 check --offline -j16 -p wgpu-test --no-default-features --features dx11 --lib` passed.
- Clippy passed for `wgpu-types`, affected core tests, D3D11 HAL tests, the public D3D11 target, and `wgpu-test` with `dx11`. Existing warning classes include HAL argument counts, older test-style lints and the public test's standard `HashMap`; no new warning was emitted for the operation gate or fixes.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

These fixes close specific implementation and core/HAL agreement defects; they
do not add broad FL9_1 functionality or complete the public contract audit.
The hardware results are from a modern GPU explicitly restricted to FL9_1,
not a historical FL9_1 device. Physical device-removal recovery, historical
driver behavior, Lumia/CoreWindow execution and full CTS integration remain
unverified.
The fixes are checkpointed in `04fa504a9434e6616d30a662dcc319725b4be004`
(`outerProduct` gate), `7979fd3d8f36a021283fa25c4fe8dfa85aa72eb5`
(empty mapping unmap) and `f306faae805b9d809e5a943b9e48524c784e15d6`
(selected-mip resolve). The follow-up documentation is being recorded in this
checkpoint.

## Independent contract follow-up — 2026-11-08

Three focused specialist passes ran against the current tree. The shader/HLSL
audit found no additional vertex or fragment math operation reaching an
unimplemented writer arm beyond the inverse and `outerProduct` gates now in
core. It found a separate GLSL/SPIR-V compute `WorkGroupSize` lowering gap,
but this is outside the FL9_1 graphics contract and D3D11 compute is disabled;
no change was made. A public texture/view/copy audit found no further confirmed
core-valid mismatch: 3D `COPY_SRC` is not projected, overlapping self-copies
are rejected by core and HAL, ordinary arrays are not exposed, and cube usages
match the sampled/copy-only HAL path. The previously found selected-mip resolve
was the only concrete defect in that pass and is covered above.

The opt-in `wgpu-test` harness is already strict-capable: its DX11 feature is
explicit, `WGPU_DX11_REQUIRE_HARDWARE=1` requires `WGPU_BACKEND=dx11` and a
physical FL9_1 device in `.gpuconfig`, and selected tests report `[Executed]`.
The Deno CTS runner is not wired to compile `wgpu-core/dx11`; a future explicit
DX11 CTS feature chain would also need a strict physical-hardware preflight.
Current `dx11_public` integration coverage remains the targeted public GPU
regression path, not CTS conformance.

Next bounded investigation is proving whether vertex uniform arrays of
explicitly padded structs containing `vec2<f32>` fields can use the existing
builtin-index emulation without changing WGSL/HLSL layout semantics. A direct
uniform `array<vec2f, N>` is invalid WGSL because its native 8-byte array stride
does not meet the required 16-byte uniform stride. The `StencilOnly` feasibility
review found that an aspect-only render attachment is invalid under core's
renderable-view contract.
Sampling a stencil-only D24S8 view would require a queried
`X24_TYPELESS_G8_UINT` SRV, uint texture bindings and `textureLoad`; the current
FL9_1 profile gates texture loads, and D24S8 SRV support is not a required
9_1 format guarantee. Defer that feature until an isolated FXC proof and
selected-device query establish a correct path. Intrinsic FL9_1 restrictions
and historical/Lumia verification gaps remain separately tracked.

## Padded `vec2<f32>` vertex-uniform arrays — 2026-11-08

Extended the selected vertex-uniform array indexing proof to accept the valid
WGSL wrapper shape `array<Vec2Slot, N>`, where `Vec2Slot` is a 16-byte struct
with one `vec2<f32>` member at offset zero. A plain `array<vec2<f32>, N>` is
still rejected by WGSL validation because its 8-byte element stride does not
meet the 16-byte uniform-address-space alignment. Core and HAL require the
same exact wrapper layout; the HAL verifies Naga's HLSL output and compiles it
with the FL9_1 FXC profile. The public strict readback test uses distinctive
padding bytes, checks all four uniform values and verifies the existing
out-of-range clamp selects the final element.

Verification on Rust 1.93.1, the Visual Studio x64 environment and the
physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false,
requested/actual FL9_1):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib -- --test-threads=1`: 112/112 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 414/414 passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1`: 87/87 passed. Three optional device subcases skipped: R8G8Unorm mip support, R8G8Unorm vertex IA and Float16x2 vertex IA. The new public readback case executed on the GPU and matched `[255,0,0,255]`, `[0,255,255,255]`, `[51,153,204,255]` and `[204,64,51,255]`; vertex index 4 matched the clamped final value `[204,64,51,255]`.
- `cargo +1.93 test --offline -j16 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib`: 101/101 passed.
- `cargo +1.93 fmt --all -- --check`, Clippy for the affected core tests, D3D11 HAL tests and public D3D11 test target, and `git diff --check`: passed. Clippy reports existing warning classes in D3D11 draw/view helpers, core fixture style, HAL tests and an older public test's `std::collections::HashMap`; no new warning was reported for this addition.

This adds one constrained shader-data shape. Other dynamic indexing forms,
plain `vec2` arrays, fragment-stage uniform-array indexing and general
WGSL/HLSL layout equivalence remain outside the verified subset. The selected
hardware is a modern RTX explicitly restricted to FL9_1, not historical
FL9_1 hardware. The implementation is checkpointed as
`4737d5f55e63411ddd647111ff1256d683fc94f2`.

## Present-time device-loss reservation regression — 2026-11-08

Extended the existing public HWND error fixture to cover the native swapchain
reservation after an injected `DXGI_ERROR_DEVICE_REMOVED` from `Present`. After
the public device-lost callback fires exactly once, the test accesses the
test-only HAL surface, reacquires from the same swapchain, and discards the
frame. This verifies that the failed public frame releases its acquisition
token. The error is synthetic; this does not emulate physical device removal
or recovery.

Strict verification used the Visual Studio 2026 x64 developer environment,
Rust 1.93.1, and the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false, requested/actual FL9_1):

- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11-test-fixture,std --test dx11_surface_error_fixture -- --nocapture --test-threads=1`: 2/2 passed, including the Present device-loss callback and reentrant Present cleanup cases.
- `cargo +1.93 clippy --offline -j16 -p wgpu --no-default-features --features dx11-test-fixture,std --test dx11_surface_error_fixture`: passed; three existing D3D11 HAL argument-count warnings were emitted.
- `cargo +1.93 fmt --all -- --check` and `git diff --check`: passed.

The native error is injected while the test still uses the physical D3D11
device; it is a lifecycle regression, not evidence of real removal/recovery.
The test is checkpointed as
`535ea4f098d046e0ed8d700dbde3cca544914025`.

## Public command-state and queue-write ordering — 2026-11-08

Two public regressions now cover command and resource state at the wgpu API
boundary. An empty render bundle invalidates the pass's pipeline, bind group
and vertex-buffer state; the strict test explicitly restores those bindings,
draws and verifies all 64 readback pixels. A separate uniform-boundary test
encodes draws, finishes the command buffer, then calls `Queue::write_buffer`
before submitting. Readback confirmed the execution-time update changed the
first draw from blue to green while the c254 vertex uniform still shifted the
second draw red; core continued to reject a 256-vector request before HAL.

Both ran on the physical RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), with FL9_1 explicitly requested and selected. The focused
strict tests each passed 1/1. The complete strict public D3D11 target passed
88/88, with three optional device-specific subcases reported as skips. The
full target included actual GPU readback for both new cases. Formatting,
Clippy for the public test target and `git diff --check` passed; Clippy emitted
only previously observed warning classes. The queue-ordering test and its
reproduction command are checkpointed in
`818fee943b117018cae30c813d17fa0e389c7669`; the empty-bundle state test is in
`9e07459a3eee540c349c9e28cc6a864221c3c01f`.

These cases add regression evidence, not new backend capabilities. The broad
public core/HAL contract audit, historical FL9_1 hardware, physical device
removal/recovery and Lumia/CoreWindow runtime execution remain open.

## Optional timestamp query support — 2026-11-08

The backend now probes timestamp/disjoint query creation and execution once
when opening a native D3D11 device. A positive, nonzero frequency gates
`TIMESTAMP_QUERY` and `TIMESTAMP_QUERY_INSIDE_ENCODERS`; timestamp query sets,
encoder writes and checked resolves are implemented. Resolve flushes and waits
on the CPU for `GetData`, then writes each result into the destination buffer.
Pass-level timestamp writes, occlusion queries and pipeline statistics remain
unavailable.

On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false),
requested and selected FL9_1, the strict public test resolved timestamps
`1794135670929055744` and `1794135670929060864`, confirmed their order and
reported `1 ns/tick`. The selected adapter advertised only the timestamp and
encoder-write features; public validation rejected occlusion and pipeline
statistics query sets.

Commands run with Rust 1.93.1 and the Visual Studio 2026 x64 environment:

```powershell
cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public --no-run
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_timestamp_queries_resolve_on_fl91 -- --nocapture --test-threads=1
```

The HAL check and public test-target compilation passed. Strict tests passed:
private D3D11 HAL 416/416, public D3D11 89/89, and DX12 core 101/101. The
public run reported three optional device-specific skips; the timestamp query
test itself executed and read back both results. HAL and public-target Clippy
passed with existing warning classes in unrelated D3D11 helpers and an older
public-test `HashMap`. Formatting and `git diff --check` passed. This increment
was committed as `ede5c299f`.

## Independent public-contract audits — 2026-11-08

A follow-up audit corrected the next candidate: FL9_1 does not permit 3D GPU
texture copies under Microsoft's 10Level9 context rules, so the existing
core/HAL rejection is correct and no copy implementation should be added. The
audit verified that the public 3D usage projection is
`TEXTURE_BINDING | COPY_DST`, with no `COPY_SRC`.

Independent render-bundle, selected-entry shader, texture format/usage, and
HAL unsupported-operation audits found no concrete public-valid D3D11 mismatch
in their scopes. Render bundles replay through the ordinary validated render
command path; format/readback masks are separately gated; shader operations
are checked for the selected entry point; and compute, pipeline-cache,
ray-tracing/acceleration-structure and occlusion-query operations are rejected
before unsupported HAL panic paths. These were source audits, not new test
runs. The broad audit of less-common public states and external verification
remain open. Historical FL9_1 driver behavior, Lumia/CoreWindow runtime, and
physical device-removal/recovery remain unverified.

The selected-device optional-format diagnostic was rerun under the Visual
Studio 2026 x64 developer environment after the initial shell lacked
`link.exe`. `fl91_wgpu_projection_matches_the_selected_physical_device`
passed 1/1 on the RTX 3090 Ti at requested/actual FL9_1. D32_FLOAT and
D32_FLOAT_S8X24 format queries returned `E_NOINTERFACE` rather than positive
support, so they do not provide a safe optional depth-format expansion on this
device. The selected device also reported no `TEXTURE2D` support for the
probed optional float formats; no new sampled format candidate was found.
Existing `hal_queue.rs` unnecessary-qualification compiler warnings remain.

## FL9_1 capability-boundary candidate sweep — 2026-11-08

A one-off native probe ran on the physical RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, software=false) with requested and selected feature
level L9_1. D16 read-only DSV creation with both depth-stencil and shader
resource binding failed at `CreateDepthStencilView` with `E_INVALIDARG`
(`0x80070057`). No read-only-depth capability is exposed. The supporting
experimental probe and view code were removed because the selected device
cannot execute the path, and Core's combined
`DEPTH_STENCIL_READ | RESOURCE` attachment use also needs a separate HAL
render-pass implementation plus positive GPU readback on a capable device.
This was a capability-boundary investigation, not a feature addition.

Independent audits also rejected two tempting expansions: BC1/2/3 readback
conflicts with 10Level9's restriction on copying shader-resource-only sources
to CPU-visible memory, and public wgpu gates the compressed format family as a
whole; 3D render-target views are not permitted by the FL9_1 `CreateRenderTargetView`
contract. Existing per-format and 3D usage gates remain unchanged. Neither
case was exercised as a new GPU operation.

Verification during the probe: the synthetic readonly-capability projection
test passed 1/1, and the strict selected-device capability diagnostic passed
1/1 on the RTX at requested/actual FL9_1 while reporting the DSV failure.
The diagnostic run used Rust 1.93.1 and the Visual Studio 2026 x64 developer
environment; it emitted the existing four `hal_queue.rs`
`unused_qualifications` warnings. No source change remains from the failed
feature experiment. Continue with the independent render-pass and buffer
mapping audits before selecting another implementation increment.

## Public FL9_1 buffer-map slice regression — 2026-11-08

Checkpoint `42414a6f9f3fd9382c620c0c0629894fd26d580b`
(`dx11: verify public FL9_1 buffer slice readback`) adds a public wgpu test
that submits an 8-byte buffer copy into a nonzero destination range, maps that
exact nonzero slice, then maps the complete buffer and verifies both sentinel
regions outside the copy were preserved. It passed on the NVIDIA RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false) with requested and
actual feature level FL9_1.

Commands run using Rust 1.93.1 and the Visual Studio 2026 x64 environment:

```powershell
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public --no-run
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_map_nonzero_slice_after_gpu_write_preserves_neighbors_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public experimental_dx11_is_not_enabled_by_default -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public
```

The target compiled, the focused strict test passed 1/1, the default-backend
isolation test passed 1/1, and the full strict public suite passed 90/90. Three
optional subcases explicitly logged skips: Rg8Unorm vertex-input support,
Float16x2 vertex-input support and Rg8Unorm mip support. The suite used the
physical RTX throughout and reported no hardware-test skips. Clippy passed
with three existing HAL `too_many_arguments` warnings and the existing
`std::collections::HashMap` warning in the public test file. The file-level
`cargo +1.93 fmt --check -- wgpu/tests/dx11_public.rs` reports numerous older
formatting differences elsewhere in that large test file; the newly added
function was not among the reported diffs. `git diff --check` passed. No HAL
or core code changed in this test-only increment.

## Combined default depth-view roles — 2026-11-08

An independent public resource audit found that a sampled depth texture could
be created with `RENDER_ATTACHMENT | TEXTURE_BINDING`, but a default texture
view inherited both usages and was rejected by HAL. The view creation path
already constructs an SRV and DSV when the corresponding use bits are present;
the supported-use mask incorrectly added `RESOURCE` only when it was the
view's sole usage. It now includes that bit whenever the requested view usage
contains it, while the texture's own usage and selected-device shadow-support
checks remain required.

The strict public test reuses one default view sequentially as both the
depth-write attachment and sampled comparison view for D16 and Depth24Plus.
For D24S8 it verifies creation and attachment use of the combined default
view, while sampling retains a separate `DepthOnly` view because core rejects
sampling an all-aspects D24S8 view. Each format produced 64/64 expected output
pixels (32 passing and 32 failing comparisons) on the physical NVIDIA RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), with FL9_1
requested and selected.

Verification with Rust 1.93.1 and Visual Studio 2026 x64:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_fl91_combined_default_depth_views_support_their_roles -- --nocapture --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu-hal --no-default-features --features dx11 --lib -- --quiet --test-threads=1
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/hal_device.rs wgpu/tests/dx11_public.rs
cargo +1.93 clippy --offline -j4 -p wgpu-hal --no-default-features --features dx11 --lib
cargo +1.93 clippy --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public
```

The focused public test passed 1/1, the full strict public suite passed
91/91, and the strict private D3D11 HAL suite passed 416/416. The public suite
logged three optional selected-device subcase skips: Rg8Unorm vertex input,
Float16x2 vertex input and Rg8Unorm mip support. HAL Clippy and public-test
Clippy passed with previously observed warnings (`too_many_arguments` in
unrelated HAL helpers and `std::collections::HashMap` in the existing public
test file). Rustfmt and `git diff --check` passed. One earlier full HAL run
reported several failures before its output session ended; a focused rerun of
the named indexed-triangle case and a complete quiet strict rerun both passed,
so no failure reproduced.

Implementation and test checkpoint:
`f3926cf9bf1b5ef61101b0efbc1166b45aeeb6e5`
(`dx11: support combined FL9_1 depth views`).

## Async write-map public regression — 2026-11-08

Added `public_dx11_async_write_map_updates_nonzero_slice_at_fl91`. It submits
a GPU copy using a `MAP_WRITE | COPY_SRC` buffer, requests an asynchronous
write map of bytes 8..16, updates that range, then submits full and partial
GPU copies and maps both readbacks. The test verifies that the new bytes are
visible to later GPU work and that source and destination sentinel bytes
outside the written/copied region remain unchanged.

The focused strict test passed 1/1 and the complete strict public D3D11 target
passed 92/92 on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), with requested and actual feature level
FL9_1. Selected-device optional subcases logged skips; the strict hardware
test itself executed and passed. The first invocation outside a Visual Studio
developer shell could not find `link.exe`; rerunning through Visual Studio
2026 `VsDevCmd.bat` built and ran successfully.

`cargo +1.93 clippy --offline -j4 -p wgpu --no-default-features --features
dx11,wgsl,glsl,std --test dx11_public`, `cargo +1.93 fmt --all -- --check`
and `git diff --check` passed. Clippy reported existing warnings in HAL
helpers and an existing `std::collections::HashMap` use in the public test.

Three concurrent source-only reviews found no confirmed public-valid mismatch
in the audited selected-entry shader gates, capability/format projection, or
buffer/queue ordering and lifetime paths. The buffer audit identified this
async write-map case as a missing public regression. These reviews were
targeted, not an exhaustive audit of every wgpu request. Historical FL9_1
drivers, actual device removal/recovery, and Lumia/CoreWindow runtime remain
unverified.

Implementation/test checkpoint: `fe0cc897a` (`dx11: test FL9_1 asynchronous write mapping`).

## Narrow fragment texture-load emulation and cube readback — 2026-11-08

The internal capability projection now exposes a distinct
`texture_load_2d_float_fragment` limit. It permits only a selected fragment
entry point with one direct `textureLoad` image: single-mip, non-arrayed
`texture_2d<f32>`, with no helper functions, control flow, additional bound
globals, sampling/query operations or extra image resources. Core stores the
proof per entry point and rejects unsupported variants before HAL. The private
shader path remaps that image to `t0`, then emulates point sampling at texel
centers using an internal `s15` sampler and a `b13` dimensions buffer; out of
bounds coordinates and nonzero mip requests return zero. General texture-load
support remains false. A strict public render/readback test passed all 64
in-bounds green pixels and 64 out-of-bounds zero pixels on the physical RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false), with FL9_1
explicitly requested and selected. It checked 64 in-bounds green pixels, 64
out-of-bounds zero pixels and 64 zero pixels for nonzero mip requests.

Cube texture-to-buffer readback now stages each requested face subresource.
The selected format must report the required render-target capability because
FL9_x disallows CPU copies from shader-resource-only textures; no RTV is
created. The downlevel layer-copy limit is six for the implemented cube path,
while ordinary texture arrays remain disabled. The strict HAL test checked
all 384 texels from six asymmetric faces and all buffer/row padding. The strict
public cube test also validated all six face readbacks and the existing cube
sampling/copy behavior on that same physical adapter and feature level.

Verification with Rust 1.93.1 and Visual Studio 2026 x64:

```powershell
cargo +1.93 check --offline -j8 -p wgpu-hal --no-default-features --features dx11
cargo +1.93 test --offline -j8 -p wgpu-types --lib restricted_texture_load_does_not_imply_general_texture_load
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib restricted_fl9_texture_load_proof_is_entry_point_specific
cargo +1.93 test --offline -j8 -p wgpu-types --lib
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib --quiet
cargo +1.93 check --offline -j8 -p wgpu-info --features dx11
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib --quiet -- --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_fragment_texture_load_emulation_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_cube_texture_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 clippy --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib
cargo +1.93 clippy --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib --tests
cargo +1.93 clippy --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public
cargo +1.93 fmt --all -- --check
git diff --check
```

The strict private HAL suite passed 419/419 and the strict public suite passed
93/93. Three optional selected-device subcases logged capability skips
(Rg8Unorm vertex input, Float16x2 vertex input, and Rg8Unorm mip support); no
mandatory hardware path skipped. The selected-entry Core fixture passed 1/1
and exercised its broader FL9_1 matrix, including acceptance of the restricted
texture-load contract. `wgpu-types`' limit-intersection test and the Core
entry-point proof test each passed 1/1. The full workspace format check and
`git diff --check` passed. The public test suite also verified default
backend isolation (`experimental_dx11_is_not_enabled_by_default`).

The RTX result validates this compiler/driver combination only; forced FL9_1
on modern hardware is not a substitute for historical FL9_1 devices. No
general Core exposure has been enabled: D3D11 remains explicit opt-in, and
the new capability describes only the tested shape.

The shared downlevel-limit addition initially exposed one exhaustive GLES
initializer at compile time. GLES now explicitly reports the narrow
texture-load flag as false while retaining its existing general
`texture_load=true`; its general capability satisfies the narrower requested
limit. `cargo +1.93 check --offline -j8 -p wgpu-info --features dx11` passed
after the fix, compiling the reporting path and enabled backends. The full
`wgpu-types` unit suite passed 101/101 and the DX12-configured Core unit suite
passed 102/102. HAL, Core-test and public-test Clippy completed successfully;
they emitted existing argument-count, fixture-style, and HashMap warnings. The
full workspace `cargo +1.93 fmt --all -- --check` and `git diff --check` both
passed.

Lumia runtime verification remains blocked for this attempt: the documented
Device Portal helper timed out while requesting `/api/os/devicefamily`. No
CoreWindow or ARM32 runtime result is inferred from the desktop test suite.

## Pass-boundary timestamps — 2026-11-08

The existing optional timestamp feature now supports `RenderPassDescriptor`
and `ComputePassDescriptor` boundary writes. HAL retains the timestamp query
objects and records begin markers before pass commands and end markers after
pass completion. Empty compute passes can be bracketed; compute pipelines and
dispatch remain rejected. `TIMESTAMP_QUERY_INSIDE_PASSES` remains unexposed,
so explicit timestamp-write methods during an open pass are still unavailable.

On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), the test explicitly requested and selected
FL9_1. Readback returned six ordered values for encoder, render-pass and
empty-compute-pass timestamps with a 1 ns/tick period. The focused strict
public case passed 1/1; the complete strict public D3D11 suite passed 93/93,
with three optional capability subcases logging skips and no required
hardware-test skip; the complete strict private HAL suite passed 419/419.
The HAL timestamp capability unit test, device timestamp/disjoint probe,
`cargo +1.93 check --offline -j16 -p wgpu-hal --no-default-features
--features dx11`, targeted rustfmt check and `git diff --check` passed. The
full strict public test command was rerun by the coordinator and also passed.

## GLSL outer-product lowering — 2026-11-08

The Naga HLSL writer previously returned `Error::Unimplemented` for
`MathFunction::Outer`, despite the backend's validated GLSL/SPIR-V path being
able to represent the operation. It now lowers f32 vector outer products to
an explicit square or rectangular HLSL matrix constructor. The GLSL overload
resolver also had the dimensions reversed for rectangular products; it now
produces `mat2x3` for `outerProduct(vec3, vec2)`, matching the GLSL
specification. Regression tests check the GLSL IR dimensions and generated
HLSL component ordering. The FL9_1 DX11 capability projection enables
`matrix_outer_product`; the selected-entry restriction remains in place for
backends that do not implement it. WGSL has no `outerProduct` builtin, and
DX12 continues to withhold this limit pending separate validation.

Strict execution on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false) explicitly requested and selected FL9_1. The
public test compiled and rendered square `mat2` and rectangular `mat2x3`
operations in the fragment shader. Readback matched `[16, 32, 48, 255]` for
all 64 pixels. The combined inverse-negative/outer-product pipeline test
passed 1/1; Naga library tests with `glsl-in` passed 142/142; the full strict
public D3D11 target passed 94/94. Three optional device-dependent subcases
logged skips (R8G8 vertex input, Float16x2 vertex input and R8G8 mip support);
the required hardware test did not skip.

Verification commands:

```powershell
cargo +1.93 test --offline -j16 -p naga --lib --features glsl-in
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_matrix_outer_product_renders_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public -- --nocapture --test-threads=1
```

These changes establish only this shader operation on the tested compiler and
driver; they do not establish historical FL9_1 compatibility or complete the
public contract audit.

Final verification was rerun on the integrated tree. The private HAL suite
passed 419/419 and the strict public D3D11 suite passed 94/94; three optional
selected-device format subcases logged SKIP while all required hardware tests
executed. The focused outer-product public test passed 1/1, Naga's GLSL-enabled
library suite passed 142/142, and DX12 Core passed 102/102. Naga, HAL and
public-test Clippy completed successfully. HAL Clippy retained existing
warnings in queue qualification, argument counts, single-element range
fixtures, test-module placement and a needless borrow; public-test Clippy
retained the existing `HashMap` warning. Workspace formatting and
`git diff --check` passed.

## Narrow FL9_1 fragment `textureDimensions` — 2026-11-08

Added a separate `DownlevelLimits::texture_dimensions_2d_float_fragment`
capability. Core allows only a direct fragment `textureDimensions` query on one
group-0, non-array `texture_2d<f32>`, converted directly to `vec2f` in a simple
entry point. Other queries, helper calls, control flow, additional bound
resources and unrelated runtime integer operations remain rejected. The HAL
reuses its private host-supplied dimensions constant and refreshes it when the
bound view changes; general `texture_query` remains disabled. `wgpu-info`
prints the new limit.

The strict physical-GPU test explicitly requested and selected FL9_1 on the
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false). It changed a bind group from a 16x8 to an 8x4 texture view and
verified all 64 output pixels: left `[255,255,0,255]`, right
`[128,128,0,255]` (±1). The first run exposed the Core and HAL runtime-integer
gates; these now exempt only the proven query and direct conversion, with
synthetic negative coverage for unrelated integer conversion.

Verification: type-limit tests 102/102; DX11 Core tests 104/104; DX12 Core
tests 103/103; private D3D11 HAL tests 421/421; strict public D3D11 tests
95/95; focused texture-dimensions hardware test 1/1; HAL proof/FXC tests 2/2;
the strict Core/HAL cleanup fixture passed 1/1; `wgpu-info` DX11 check passed.
The public suite logged three optional device
capability skips (R8G8 vertex input, Float16x2 vertex input and R8G8 mips); no
required test skipped. Core Clippy passed without warnings. HAL Clippy passed
with existing unrelated warnings. Workspace formatting and `git diff --check`
passed. Historical FL9_1 hardware remains untested.

## Narrow FL9_1 fragment `textureNumLevels` — 2026-11-08

Added an independent `texture_num_levels_2d_float_fragment` downlevel limit.
Core permits only one direct `textureNumLevels` query on a group-0, non-array
`texture_2d<f32>` in a simple fragment entry point, converted directly from
`u32` to `f32`. The selected entry point cannot use helpers, control flow,
other resources, other image operations or unrelated runtime integer
semantics. Naga now has a proof option separate from the existing texel-load
emulation, so enabling this query does not broaden the old texture-load
contract. HAL supplies the selected view's mip count through the reserved
`b13` constant and accepts a supported multi-mip view. General
`texture_query` remains disabled.

On the physical NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), the strict public test explicitly requested
and selected FL9_1. One texture was bound through views exposing one and three
mip levels. The shader returned red 85 and 255 to the two halves of an 8x8
target; all 64 output pixels matched within one UNORM value.

Verification on the integrated tree: private D3D11 HAL tests passed 424/424;
the strict public D3D11 suite passed 96/96, including the new GPU test and
three optional device-capability subcases that logged SKIP; Naga HLSL-enabled
library tests passed 142/142; the focused `wgpu-types` and DX11 Core tests each
passed 1/1. DX12 Core `cargo check` passed. The attempted full DX12 Core unit
test build stopped while compiling the generated Windows bindings after a
Windows memory-allocation failure, before tests ran. HAL, Core and types
Clippy passed; HAL retained existing warnings. Naga Clippy was also attempted
but rustc failed allocating 2 MiB under current Windows commit pressure.
Workspace formatting and `git diff --check` passed. These checks establish a
narrow compiler emulation on one modern GPU forced to FL9_1, not historical
FL9_1 driver behavior or general texture-query support.

## Narrow FL9_1 fragment `textureDimensions` with a constant mip — 2026-11-08

Extended the direct fragment `textureDimensions` proof to accept an optional
nonnegative literal or module-constant mip level. HAL computes the queried
dimensions relative to the selected view's base mip. Runtime levels, overrides,
negative signed levels and broader query expressions remain rejected. An
out-of-range level produces a safe 1x1 value where WGSL leaves the result
indeterminate. General `texture_query` remains disabled.

On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), strict mode explicitly requested and selected FL9_1. The
shader queried level `2u` on a 16x8 four-mip texture through two views with
different base mips; all 64 pixels matched the expected 4x2 and 2x1 dimensions.
The focused hardware test passed 1/1. The HAL out-of-range mip unit test passed
1/1. The surrounding strict suites previously passed: private HAL 427/427,
public D3D11 94/94, Naga HLSL tests 142/142 and Core 105/105. A focused rerun
after the last Naga writer cleanup again passed the hardware test. These runs
validate the emulation on one modern forced-FL9_1 device, not historical
drivers.

Next, audit whether the same query proof can safely cover supported 3D views.
Keep the shader/Core/HAL proof narrow and query-gated. Broad public contract
validation, historical FL9_1 hardware, Lumia/CoreWindow execution and physical
device-removal recovery remain mission-level open gates.

## Narrow FL9_1 fragment 3D `textureDimensions` — 2026-11-08

Added an independent `texture_dimensions_3d_float_fragment` limit. The first
supported form is one direct fragment query on one group-0, non-array
`texture_3d<f32>`, without an explicit mip argument, directly converted from
`vec3u` to `vec3f`. Core, Naga and HAL use a proof separate from the 2D path;
HAL supplies the bound view's XYZ dimensions through `b13`. The limit is
projected only when the selected FL9_1 device passes the existing RGBA8 3D
sampled-format query and the backend path is enabled. 3D query support does not
enable copies, attachments, readback or general `texture_query`.

The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false)
explicitly requested and selected FL9_1. A public draw queried a 4x2x8 volume
and all 64 readback pixels matched `[255, 128, 128, 255]`. Focused tests passed
for the independent wgpu-types limit (1/1), Core selected-entry proof (1/1),
HAL emulation binding (1/1), Naga HLSL writer (9/9) and full HLSL-enabled Naga
library suite (143/143). One initial GPU run exposed the missing HAL integer
conversion proof for vec3; that gate was extended narrowly and the rerun passed
1/1. Full D3D11 suites are pending for this increment. Historical hardware
remains unverified.

The public test was extended to switch from a 4x2x8 volume to a depth-one
4x2x1 view in the same render pass, verifying that a legal 3D view with depth
one passes the HAL query gate. All 64 output pixels matched the two expected
dimension colors. The complete strict public D3D11 suite then passed 95/95 on
the RTX 3090 Ti at explicitly requested and selected FL9_1; the ordinary
enumeration isolation test also passed. Three optional device-capability
subcases reported SKIP and were not counted as hardware execution passes. The
complete private HAL suite passed 429/429 in the integrated tree. The focused
Core proof passed 1/1, `cargo +1.93 fmt --all -- --check` passed, and
`git diff --check` passed (Git emitted only LF-to-CRLF worktree conversion
notices). These results do not establish historical FL9_1 driver behavior.

## Narrow FL9_1 `textureSampleBaseClampToEdge` — 2026-11-08

Added a separate narrow limit for one direct fragment base-clamp sample using
one group-0 non-array `texture_2d<f32>` and one group-0 non-comparison sampler.
Core's selected-entry proof rejects helper calls, other resources, additional
image operations and control flow for this path. Naga validates the resource
mapping and emits an implicit `Texture2D.Sample` after clamping coordinates to
half-texel centers using selected-view dimensions supplied in the existing
`b13` buffer. HAL binds the proven texture at the expected remapped register,
refreshes dimensions for the current bind group and requires a single-mip
view. Other sampled-resource and single-mip restrictions remain in force.

The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false)
explicitly requested and selected FL9_1. A 2x2 four-color texture was sampled
at out-of-range coordinates into an 8x8 target using sparse logical bindings 7
and 42; all 64 pixels matched their expected edge colors. The initial test
descriptor used Repeat addressing, which Core rejected before pipeline or GPU
execution because the public adapter has a finite repeat-per-primitive limit
of 128. The test now uses a legal ClampToEdge sampler. Therefore this run
verifies the emitted base-clamp path and readback, but does not physically
distinguish the builtin from native clamp addressing or establish behavior
with Repeat/MirrorRepeat samplers on FL9_1.

The Naga focused HLSL writer test passed 1/1 and full HLSL-enabled Naga library
tests passed 144/144; Windows SDK FXC accepted the generated pixel shader for
`ps_4_0_level_9_1`. Core selected-entry proof, HAL binding proof and the
`wgpu-types` capability test each passed 1/1. The strict public hardware test
passed 1/1 with all 64 pixels verified. A DX12 HAL check passed after its
HLSL options explicitly left both FL9-only proof modes disabled. The complete
private HAL suite then passed 430/430 and the complete strict public D3D11
suite passed 96/96 on the same forced FL9_1 hardware. The public suite includes
the zero-default-enumeration regression; optional per-device format subcases
may log SKIP and are not counted as GPU evidence. An earlier integrated run
exposed a stale assertion that expected the direct form to remain rejected;
the public regression now accepts the proven direct entry and still rejects
helper use before HAL. The entry-point proof is selected-entry-specific and
does not reject unrelated helper declarations in the same module. The general downlevel
`texture_sample_base_clamp_to_edge` remains false; only the narrow fragment
2D-float limit is exposed. Repeat/MirrorRepeat remain unverified through the
public API because Core rejects those sampler descriptors under the finite
FL9_1 repeat-per-primitive limit.

## 2D dimensions plus one implicit sample — 2026-11-08

Extended the narrow `texture_dimensions_2d_float_fragment` proof to allow one
direct level-less `textureSample` of the queried group-0 `texture_2d<f32>` with
one direct non-comparison sampler. Both operations must be in the selected
fragment entry point. The proof rejects explicit mip queries when sampling,
explicit-level sampling, helper calls, additional resources, other image
operations, control flow, `select` and unrelated runtime integer semantics.
General `texture_query` remains false; this does not authorize general texture
query support.

The strict public test explicitly requested and selected FL9_1 on the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false). One shader used both `textureDimensions(image)` and
`textureSample(image, sampler, uv)` to compute output. A bind-group transition
changed from a 4x2 texture filled with `[200,160,80,255]` to an 8x4 texture
filled with `[80,120,160,255]`; all 64 readback pixels matched `[50,40,80,255]`
on the first half and `[40,60,160,255]` on the second half (±1 UNORM value).
Thus the result depends on both the queried dimensions and the sampled bound
resource.

The Naga focused writer test and full HLSL-enabled Naga library tests passed
1/1 and 145/145. Core's selected-entry proof passed 1/1 and the full DX11 Core
library suite passed 106/106. The focused strict public GPU test passed 1/1.
The complete private HAL suite passed 430/430 with strict hardware mode on; the
run executed on the same RTX 3090 Ti at requested/actual FL9_1. That run caught
a Naga proof regression which had treated logical `@group` as a native
register namespace; the logical-group restriction was removed while keeping
the native `space` and register checks. After this correction, the complete
strict public D3D11 target passed 97/97 on the same physical RTX 3090 Ti at
requested/actual FL9_1. Optional capability-specific skips remained explicitly
reported by their tests and were not counted as hardware passes. This closes the
public-suite verification gap recorded during the first build attempt.

Commands run on Rust 1.93.1 in the VS 2026 x64 developer environment included:

```powershell
cargo +1.93 test --offline -j1 -p naga --lib --features hlsl-out fl9_texture_dimensions_can_share_texture_with_one_implicit_sample -- --nocapture
cargo +1.93 test --offline -j1 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_dimensions_accepts_only_direct_fragment_conversion -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_and_sample_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j1 -p naga --lib --features hlsl-out
cargo +1.93 test --offline -j1 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib
cargo +1.93 test --offline -j1 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

Workspace formatting and `git diff --check` passed. Four pre-existing
unnecessary-qualification warnings remain in `hal_queue.rs`. Historical
FL9_1 driver behavior, broad public-contract/CTS coverage, Lumia/CoreWindow
execution and physical device-removal recovery remain open.

The complete strict public target was rerun after the native binding-map proof
correction and passed 97/97 on the physical RTX 3090 Ti at requested/actual
FL9_1. Command:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The run included the combined dimensions-and-sample readback and the existing
public transition/rendering coverage. Optional per-device capability branches
continue to report their skips explicitly; they are not counted as hardware
execution evidence.

## Narrow FL9_1 3D `textureDimensions` plus implicit sample — 2026-11-08

Extended the existing independent `texture_dimensions_3d_float_fragment` limit
to permit one implicit, level-less `textureSample` from the same 3D float image
and one group-zero non-comparison sampler. Core's selected-entry proof requires
a direct query, matching image identity, direct `vec3f` conversion and
`vec3f` sample coordinates; helper functions, extra resources, explicit LOD,
extra image operations and runtime integer semantics remain rejected. Naga
accepts the 3D proof's one compact sampler mapping while keeping the texture at
the reserved `t0`; HAL uses the existing logical binding plan for sampler
registers and supplies the current bound volume's dimensions through `b13`.
The device remains gated by its positive RGBA8 3D sampled-format query, and
general texture query support remains false.

The strict public test forced/requested/selected FL9_1 on the physical NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false).
It uploaded constant but distinguishable 4x2x8 and 2x4x1 RGBA8 volumes and
switched bind groups during one render pass. The shader multiplied sampled
values `[200,160,80,255]` and `[80,120,160,255]` by queried-dimension ratios;
readback matched `[200,80,40,255]` and `[40,120,10,255]` for all 64 pixels.
Thus both dimension metadata and actual sample values affected the output.

Verification results:

- Naga focused writer test: 1/1; full HLSL-enabled library: 147/147.
- Core focused 3D proof and full DX11 library: 1/1, then 106/106.
- HAL focused emulation proof: 1/1; FXC test under `ps_4_0_level_9_1`: 1/1;
  complete strict private HAL suite: 434/434.
- Focused public strict readback: 1/1; complete strict public D3D11 target:
  98/98. The `experimental_dx11_is_not_enabled_by_default` isolation test
  passed. Optional capability branches that log SKIP are not GPU passes.
- `wgpu-types` full serde tests: 107 unit and 14 doc tests passed (21 doc tests
  ignored); DX12 HAL `cargo check` passed.
- Naga, Core, HAL and types Clippy commands returned success. Existing warnings
  remain in `wgpu-types` unnecessary qualifications and several established
  D3D11 HAL lint categories; no new Clippy error was introduced.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed. Git emitted
  only expected LF-to-CRLF worktree conversion notices.

An initial HAL proof test exposed expression-order sensitivity when the 3D
sample preceded the query. The proof now accepts the same selected 3D identity
regardless of Naga expression order and rejects helper functions itself. One
first public test compile used `FilterMode` for the separate mipmap-filter type;
the test was corrected to `MipmapFilterMode::Nearest`. Neither failure reached
GPU validation. No public default enumeration was enabled.

Still outstanding are a full audit of every Core-valid public request against
HAL behavior, broad WGSL and resource coverage, historical FL9_1 hardware,
Windows 10 Mobile ARM32/Lumia and public CoreWindow runtime verification, and
physical device-removal/recovery tests. The next shader slice should follow an
independent profile and contract review; do not treat this addition as broad
3D query or general texture-query support.

## Core/HAL shader-emulation validation alignment — 2026-11-08

A selected-entry audit found that the `textureNumLevels` and
`textureSampleBaseClampToEdge` Core proofs accepted an emulated entry point in
a module containing an otherwise unused regular helper, while the HAL's
emulation linker rejects any module with helper functions. Core now rejects
those emulated entries before HAL in that module shape. Regression tests retain
acceptance for the helper-free direct forms and acceptance for unrelated safe
sibling entry points; the synthetic Core fixture also initializes the newer
independent 3D texture-load limit explicitly.

The complete DX11 `wgpu-core` library suite passed 106/106 on Rust 1.93.1 in the
Visual Studio x64 environment. This was CPU-only validation; no GPU work was
needed for the rejection boundary. `git diff --check` passed. Historical
driver/runtime verification and the broader Core/HAL contract audit remain
open.

## Narrow FL9_1 fragment cube `textureDimensions` — 2026-11-08

Added an independent `texture_dimensions_cube_float_fragment` capability,
projected only when the selected FL9_1 device reports the implemented RGBA8
cube sampling path. Core's selected-entry proof, Naga's HLSL proof and HAL's
emulation proof agree on one direct no-level query of a group-0
`texture_cube<f32>`, converted from `vec2u` to `vec2f`. Explicit query levels,
helper calls, extra resources and unrelated image operations remain rejected.
The shared logical binding plan is unchanged; FXC compiled the actual shader
under `ps_4_0_level_9_1`, and HAL supplied the bound cube view's face dimensions
through the existing `b13` metadata constant.

The strict public test ran on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), with requested and
actual FL9_1. It uploaded deterministic contents to two RGBA8 cubes, switched
views from 8x8 to 4x4 during one render pass, and checked all 64 readback
pixels: `[255,255,0,255]` on the left and `[128,128,0,255]` on the right.
Both query sizes affected output. An initial strict run exposed that the test
must initialize cube contents before submission on this driver; adding the
uploads removed device loss, and the real query path then passed.

Verification commands and results:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-types --features serde
cargo +1.93 test --offline -j8 -p naga --lib --features hlsl-out
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

Results: `wgpu-types` 108/108 unit and 14/14 doc tests (21 ignored); Naga
148/148; Core 107/107; private HAL 436/436; full public strict D3D11 99/99.
The public default-enumeration isolation test passed. Public optional subcases
logged capability skips and are not counted as executed GPU cases. Physical
GPU results came from the strict HAL and public suites. The full public run
also caught a stale regression test after the preceding Core/HAL safety fix:
its direct base-clamp entry lived in a module with an unrelated helper. The
test now uses a helper-free direct shader and separately retains helper
rejection; the focused public test and full suite pass. Existing warnings are
six `wgpu-types` unnecessary qualifications and four `hal_queue.rs`
unnecessary qualifications. Clippy also reports existing HAL argument-count,
single-range test, item-order and one test-borrow warning; the wgpu public test
target reports an existing disallowed `std::collections::HashMap`; Naga has an
existing GLSL pattern-type-mismatch warning. The new Naga code adds no Clippy
warning. DX12 HAL `cargo check` passed and DX12 Core passed 106/106; DX11 Core
passed 107/107. All Rust formatting and `git diff --check` passed. Historical-
device/runtime, broad CTS, and physical device-removal coverage remain open.

Implementation checkpoint: `baacde33d` (`dx11: add narrow FL9_1 cube texture dimensions`).

## Public uniform update ordering regression — 2026-11-08

Added a public wgpu regression that records a GPU copy of a red uniform payload,
draws the left half of an offscreen target, copies a green payload into the same
uniform buffer, then draws the right half in a second pass in the same
submission. The shader binds the same buffer throughout. Readback verified all
32 red and 32 green pixels, so the second draw observes the execution-time
update rather than stale materialized data.

Strict verification ran with Rust 1.93.1 and the Visual Studio x64 environment
on the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false, requested/actual FL9_1):

- Focused strict test `public_dx11_uniform_gpu_updates_are_ordered_between_draws_at_fl91`: 1/1 passed.
- Complete strict public D3D11 integration target: 100/100 passed. Three optional selected-device subcases logged skips for R8G8_UNORM mip support, R8G8_UNORM vertex-input support and Float16x2 vertex input; required hardware tests did not skip.
- Public textured perspective cube with D16 depth: 1/1 passed; depth-tested opposite face orders matched all 1,024 pixels while painter-order controls differed at 223 pixels.
- Public-test Clippy, formatting of the changed Rust file and `git diff --check` passed. Clippy reported only existing D3D11 helper argument-count and public-test `HashMap` warnings.

The copy/draw sequence was tested through public wgpu and exercised real GPU submission and readback. Broader concurrent uniform updates, historical FL9_1 driver behavior and Lumia/CoreWindow execution remain open. Implementation checkpoint: `5951e7a873927bb95731d5ec26608eda4d715102`.

# Public BC1/BC2/BC3 unorm feature — 2026-11-09

Added the native-only `TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM` feature. DX11
advertises it only when the selected FL9_1 device and backend support sampled
uploads for BC1, BC2 and BC3 unorm. Core permits those three exact formats via
the feature while retaining the standard BC feature requirement for BC sRGB
and BC4–BC7. Adapter format usage queries remain authoritative, GPU BC copies
stay query-gated, and the FL9_x texture-to-buffer readback mask remains clear.

The strict public wgpu test explicitly requested/selected FL9_1 on the NVIDIA
GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`, software=false).
It uploaded BC1 red, BC2 green and BC3 blue blocks, copied the three resources,
sampled each in one fragment shader, and verified all 64 pixels were white.
The narrow feature was present, `TEXTURE_COMPRESSION_BC` was absent, and a BC4
texture request was rejected. A strict Core hardware fixture confirmed the
same projected feature and retained BC1 copy/readback boundaries; it passed
1/1, with the unrelated optional RG8 mip subcase skipped by the selected
device query. The HAL synthetic projection test passed 1/1, and the Core
synthetic exact-format feature test passed 1/1.

An initial Core fixture run failed because its legacy assertion expected
unsupported BC formats to fail only as invalid usages; Core now rejects them
earlier with the correct missing-feature validation error. The assertion was
updated to accept that precise earlier rejection, and the strict fixture then
passed. An initial public-test compile also exposed outdated pipeline-layout
and error-scope API field names; these were corrected before GPU execution.

The broad completion mission remains incomplete. Continue the full public
Core/HAL contract audit and practical shader/resource expansion. Historical
FL9_1 hardware, Lumia/CoreWindow runtime and real device-removal recovery remain
external verification gaps.

## BC feature regression verification — 2026-11-09

Final Rust 1.93.1 validation used the Visual Studio 2026 x64 developer
environment and bounded Cargo parallelism:

- `cargo +1.93 test --offline -j8 -p wgpu-types --lib`: 106 passed. Existing
  serde-import and unused-qualification warnings remain.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 444 passed,
  including selected-device D3D11 execution. The RTX 3090 Ti selected physical
  hardware at requested/actual FL9_1; BC1/2/3 format queries returned
  `0x000213a0`, BC4–7 queries were unavailable, the narrow feature was present
  and the broad BC feature was absent. Four existing HAL qualification
  warnings were reported.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib -- --test-threads=1`: 118 passed. This includes the production
  projection fixture and strict private-core hardware rendering.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_bc_subset -- --nocapture --test-threads=1`: 1 passed on the physical RTX. BC1 red, BC2 green and BC3 blue uploads, texture copies and sampling yielded 64 white pixels; BC4 and BC3 3D creation were rejected by Core.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1`: 106 passed. This includes the public zero-default-enumeration isolation test. Three optional subcases logged skips: R8G8_UNORM vertex input, Float16x2 vertex input and the R8G8_UNORM mip chain. They did not count as executed GPU cases.
- `cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib -- --test-threads=1`: 107 passed.
- D3D11 HAL `cargo check`, Clippy for `wgpu-types`, Core, HAL tests and the new public integration target, `cargo +1.93 fmt --all -- --check`, and `git diff --check` passed. Clippy emitted only existing backend/test warnings; no warning points to the BC changes.

The first full Core run exposed an outdated exact-feature assertion in the
selected-device fixture. It was updated to include the newly projected narrow
feature while continuing to assert that broad BC support is absent; the full
Core suite then passed. No public enumeration default or backend priority was
changed.

## Query-gated BC1/BC2/BC3 sRGB subset — 2026-11-09

Added a separate native-only `TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB` feature.
The FL9_1 capability projection now keeps sRGB guarantees, three independent
`CheckFormatSupport` results, and backend implementation paths distinct from
the unorm feature. Core permits only BC1/2/3 sRGB through that exact feature;
the standard BC feature remains absent. Typed sRGB views and same-format GPU
copies work on the selected device, but format reinterpretation, BC4–BC7, mip
chains, render-target use and texture-to-buffer readback remain unavailable.
The optional `SHADER_SAMPLE` query controls linear-filter reporting; when that
bit is absent the established point-sampling path remains valid.

Strict public execution selected the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), explicitly requested
and confirmed FL9_1. The selected device reported `0x000213a0` for each BC1,
BC2 and BC3 sRGB format (same as their unorm formats); BC4–BC7 queries returned
`E_FAIL` (`-2147467259`). The public test uploaded mid-gray sRGB blocks, copied
them GPU-to-GPU, sampled all three through the public pipeline and verified 64
pixels near `[59,57,59,255]`, well below the corresponding raw-unorm values.
The existing unorm path still verified 64 white pixels, and unsupported BC4
and BC3 3D creation remained rejected.

Verification after the final capability-contract correction:

- `cargo +1.93 test --offline -j 8 -p wgpu-hal --no-default-features --features dx11 --lib capabilities::tests -- --nocapture --test-threads=1`: 51/51 CPU/model tests passed, including selected-device diagnostics.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j 8 -p wgpu-hal --no-default-features --features dx11 --lib -- --test-threads=1`: 445/445 passed. This strict run included private HAL rendering/readback on the physical device.
- `cargo +1.93 test --offline -j 8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib texture_format_feature_tests -- --nocapture --test-threads=1`: 2/2 exact-format Core unit tests passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j 8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1`: 1/1 physical-device capability-projection fixture passed.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j 8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_bc_subset -- --nocapture --test-threads=1`: 1/1 real GPU test passed with the readback values above.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j 8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --test-threads=1`: 106/106 passed, including the public default-enumeration isolation test. Optional query-based subcases may skip within the suite; none are counted as evidence for this feature.
- `WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j 8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib -- --test-threads=1`: 119/119 passed.
- `cargo +1.93 test --offline -j 8 -p wgpu-core --no-default-features --features dx12,wgsl,std --lib -- --test-threads=1`: 108/108 passed.
- `cargo +1.93 test --offline -j 8 -p wgpu-types --lib features_names`: 1/1 passed. Existing serde-import and qualification warnings remain.
- HAL, Core and focused public-target Clippy passed. Existing warnings remain in `hal_queue.rs`, HAL helper/test sites and older Core fixture tests; a new Core `if_same_then_else` warning found during Clippy was corrected and is absent on the final run.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed. Git emitted only the repository's existing LF-to-CRLF working-copy notices.

The first full HAL run after implementation failed three synthetic BC query tests
because the new path incorrectly required the optional filterability bit for
resource creation. The code and synthetic expectation now retain the established
point-sampled contract while keeping `SAMPLED_LINEAR` query-gated; the complete
strict HAL suite then passed. An initial direct test invocation outside the VS
developer shell could not locate `link.exe`; using `VsDevCmd.bat` resolved the
environment issue. Normal public D3D11 enumeration remains disabled.

Independent audits recommend keeping shader/Core/HAL implementation centralized:
the next contained shader candidate is allowing selected helper functions in
the existing single-resource `textureLoad` emulation after all three layers
prove the reachable call graph. Naga's HLSL `matrixInverse` lowering is another
compiler gap, but requires numerical and instruction-budget coverage before its
limit changes. A suggestion to implement 3D texture copies conflicts with the
existing 10Level9 audit, which documents that path as unsupported at FL9_1;
do not pursue it. Public CoreWindow presentation still needs an actual UWP
runtime test, while historical FL9_1 devices and Lumia ARM32 remain unverified.
The overall mission is far from complete as a general-purpose backend: this
remains an opt-in, intentionally restricted FL9_1 implementation, with
unsupported intrinsic graphics features and substantial shader/API gaps.
The verified implementation and capability documentation are checkpointed at
`3a1cb5527` (`dx11: expose query-gated BC1-3 sRGB formats`).

## Constant-mip 3D `textureDimensions` — 2026-11-09

Extended the existing narrowly gated fragment 3D `textureDimensions` path to
accept a nonnegative literal or module-constant mip level. Core, Naga and HAL
require a direct group-zero non-array `texture_3d<f32>` identity and reject
dynamic or negative levels. An explicit-level query cannot be combined with
sampling in the same selected entry point; the previously verified level-less
query plus one compatible implicit sample remains supported. HAL computes
width, height and depth from the bound view's base dimensions at the requested
mip and supplies them through the existing `b13` emulation constant. The
selected device must independently report `texture_3d_mipmaps` before a
multi-mip 3D texture can be created. General texture queries remain disabled.

Strict hardware execution used the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), explicitly requested and selected
FL9_1, with `texture_3d_mipmaps=true`. An 8x4x8 RGBA8 volume with three mips
queried level 1; the 4x2x4 result was normalized and all output channels
matched `[128,128,128,255]` within one byte. The test also prints its strict
hardware identity and fails instead of silently skipping if hardware or the
required 3D-mip prerequisite is unavailable.

Verification (Rust 1.93.1, VS 2026 x64, offline, `-j8`):

- Naga targeted HLSL proof: 1/1 passed; full Naga library: 152/152 passed.
- Core targeted proof: 1/1 passed; strict DX11 fixture suite: 121/121 passed.
- HAL targeted proof: 1/1 passed; strict private DX11 HAL suite: 450/450 passed.
- Strict public forced-FL9_1 readback: 1/1 passed; full strict public D3D11
  target: 110/110 passed, including default adapter-isolation coverage.
- Core DX12 regression: 109/109 passed; wgpu-types: 108/108 passed.
- Clippy passed for Naga, HAL tests, Core tests and the public D3D11 test target.
  Existing warnings remain in HAL qualification/style sites, Core test helpers
  and default-feature `wgpu-types` serde imports/qualifications.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed after
  formatting.

The first focused Naga run found that HLSL emission still only accepted a
level-less 3D query; the output arm was extended. The negative dynamic-level
WGSL test initially omitted `@interpolate(flat)` on its integer fragment input;
the test shader was corrected. The first complete strict Core suite exposed a
stale hardware fixture mask that omitted the already-projected
`TIMESTAMP_QUERY_INSIDE_PASSES`; the mask now includes that query-gated feature
and the rerun passed. A full Naga integration test attempt could not complete:
its WGSL snapshot runner requires external `spirv-cross`, which is not
installed, and the unconstrained package run also terminated in an existing
recursive-template stack overflow. The Naga library and targeted HLSL proof
tests passed; snapshot coverage remains unverified in this environment.

The feature is checkpointed at `201f55b7b` (`dx11: support constant-mip 3D
texture dimensions at FL9_1`); the separately verified timestamp-fixture mask
correction is `8fe4e139d`. Next choose another isolated Core/HAL contract gap
after a fresh read-only audit, while retaining the experimental public-adapter
gate. Historical 9_1 hardware, Lumia/CoreWindow execution,
device-loss recovery on physical hardware, broad CTS/API coverage and
unsupported-operation closure remain incomplete.

## `textureNumLevels` with one implicit sample — 2026-11-09

The narrow fragment `textureNumLevels` emulation now allows exactly one
implicit `textureSample` from the same direct image. The selected entry must
use a dimension-matched float coordinate and one group-zero non-comparison
sampler. Core, Naga and HAL reject a mismatched image, additional samples,
explicit LOD, extra resources, helpers, control flow and other image operations.
FXC compiles 2D, 3D and cube variants at `ps_4_0_level_9_1`; the public GPU test
executes the 2D case. General texture queries remain unavailable.

Strict GPU testing selected the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested FL9_1 and confirmed actual
FL9_1 with `software=false`. A shader sampled the same RGBA8 texture it queried
while switching from a one-level to a three-level view. Its 8x8 readback
matched `[64,64,128,255]` on the left and `[191,128,64,255]` on the right, all
64 pixels within one byte.

The first strict hardware run compiled and submitted but returned black for the
sampled channels. Command replay was still clearing pixel SRV `t0` unconditionally
for the `textureNumLevels` emulation. The proof now records whether the selected
shader samples the queried texture, and replay retains the SRV only for that
case; the query-only path still clears the slot. The strict rerun passed. A
first Core build also caught the missing `alloc::format` import in the new
`no_std` test module; after adding it, the full fixture passed. An existing
negative Core case classified this now-supported same-image query/sample shape
as unsupported; it was removed, with mismatched images and other unsupported
shapes covered by the new negative test.

Verification used Rust 1.93.1, Visual Studio 2026 x64 and offline Cargo with
`-j32`:

- Naga targeted HLSL proof: 1/1; full Naga library: 153/153.
- Core targeted proof: 1/1; strict D3D11 fixture: 122/122.
- HAL relationship proof: 1/1; FL9_1 FXC compilation: 1/1; strict private
  D3D11 HAL suite: 453/453.
- Strict public GPU test: 1/1; full strict public D3D11 target: 110/110,
  including `experimental_dx11_is_not_enabled_by_default`.
- Core DX12 regression: 110/110; wgpu-types: 108/108.
- Clippy passed for Naga, Core fixture tests, HAL tests and the public D3D11
  target. Existing warnings remain in Naga GLSL test style, HAL qualifications
  and argument counts, Core fixture style, and wgpu-types serde imports and
  qualifications.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

The full public suite logged optional query-based subcase skips where the
selected RTX did not report the feature (including R16G16_FLOAT vertex input
and R8G8_UNORM mip support); those subcases are not hardware evidence. The
focused query-plus-sample test had no skip. The implementation was checkpointed
after final tests; historical FL9_1 behavior remains unverified.

## Selected-entry base-clamp helper — 2026-11-09

The existing narrow `textureSampleBaseClampToEdge` proof now permits one
reachable straight-line helper called once by the selected fragment entry.
The helper takes one `vec2f`, directly samples the same group-zero 2D float
texture with one non-comparison sampler, and returns `vec4f`. The entry may
compute the coordinate passed to the helper. Nested or repeated calls,
control-flow statements, resource-handle parameters/results, additional
selected resources and other image operations remain rejected. Naga emits only
the selected reachable helper and its globals; the same host dimensions,
half-texel clamp and implicit level-zero sample lowering are used as in the
direct path. The general downlevel capability remains false.

Strict public GPU execution selected the NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`), requested and confirmed FL9_1, and
reported `software=false`. The public WGSL entry computes transformed
coordinates and passes them to the helper. FXC compiled the helper at
`ps_4_0_level_9_1`; the public readback validated all 64 pixels. A HAL test
initially showed that two samples inside the helper were accepted by the
relationship proof; HAL now enforces its existing one-sample contract. A first
strict public attempt exposed the overly narrow raw-entry-argument requirement;
the validators now permit any type-checked `vec2f` call argument while retaining
the helper/resource/control-flow restrictions.

Verification used Rust 1.93.1, Visual Studio 2026 x64 and offline Cargo with
`-j8`:

- Naga helper lowering: 1/1; full Naga library: 153/153, including the
  transformed-coordinate helper case.
- Core selected-entry helper proof: 1/1; strict D3D11 fixture suite: 122/122,
  including the transformed-coordinate helper case.
- HAL helper proof and FXC compilation: 3/3; strict private D3D11 suite:
  454/454, including the transformed-coordinate helper case.
- Strict public helper GPU test: 1/1; the full strict public D3D11 target
  passed 110/110, including `experimental_dx11_is_not_enabled_by_default`.
  Optional capability-specific skips within the suite are not GPU evidence.
- Core DX12 regression: 110/110; `wgpu-types`: 108/108.
- D3D11 HAL `cargo check` passed.
- Clippy passed for Naga, Core fixture/tests, HAL and the public D3D11 test.
  Naga introduced no remaining Clippy warnings. Existing HAL warnings cover
  qualifications, argument counts, range test style and test-module ordering;
  existing Core test-style warnings remain.
- `cargo +1.93 fmt --all -- --check` and `git diff --check` passed.

Ordinary public D3D11 default enumeration remains disabled. Historical FL9_1
hardware, Lumia/CoreWindow execution, the wider unsupported-operation audit,
device-loss recovery on real hardware and broad CTS coverage remain open.

## Selected-entry helper `textureNumLevels` — 2026-11-09

The restricted 2D fragment `textureNumLevels` path now supports exactly one
reachable, no-argument, straight-line helper called once by the selected
entry. The helper performs one query on one group-zero, non-array
`texture_2d<f32>` and returns `u32`; the entry converts that result directly to
`f32`. Helpers with parameters, nested or repeated calls, control flow,
additional selected resources, extra queries, other image operations and
unrelated integer semantics remain rejected. The helper form is query-only;
the direct query-plus-one-sample form remains separately supported. 3D and
cube query forms remain direct-only. Core validation, Naga HLSL generation and
the private HAL proof use the selected entry point and agree on the one-helper
shape. The helper receives the existing host-known mip count through `b13`;
no native image-query instruction or new shader profile is used.

The strict public test explicitly selected the physical NVIDIA GeForce RTX
3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), requested FL9_1 and confirmed
actual FL9_1 with `software=false`. It switched between views exposing one
and three mip levels and verified all 64 pixels. The adjacent 3D/cube query
regression was rerun after the public test helper refactor and also passed at
requested/actual FL9_1; both dimension cases verified all 64 pixels.

Verification used Rust 1.93.1, Visual Studio x64 and offline Cargo:

- Full Naga library: 154/154.
- HAL selected-helper proof and FXC compile: 1/1 each.
- Strict private D3D11 HAL suite: 458/458.
- Strict D3D11 Core fixture suite: 123/123.
- Strict public D3D11 suite: 111/111; the focused helper test and the 3D/cube
  query test also passed after the test-helper signature cleanup.
- Core DX12 regression: 111/111; `wgpu-types`: 108/108.
- D3D11 build and Clippy checks passed. Existing HAL style warnings remain;
  no new helper-related Clippy warning was reported.

One optional Rg8Unorm mip subcase logged a skip in the full public suite; it is
not counted as hardware evidence. The focused helper and 3D/cube tests each
executed on hardware without skipping. `texture_query` remains generally
disabled. The result covers a modern NVIDIA driver forced to FL9_1, not
historical FL9_1 hardware.

Checkpoint: `8ba3f0f6418afedde998b3b615f0d30e1b21420c`
(`dx11: support selected-entry texture emulation helpers`).

## Public color-write-mask transitions — 2026-11-09

Added `public_dx11_color_write_masks_preserve_disabled_channels_at_fl91` in
`wgpu/tests/dx11_public.rs`. One render pass draws to six scissored columns
using `ALL`, `RED`, `GREEN`, `BLUE`, `ALPHA` and empty write masks; two more
columns remain untouched. The attachment begins with a distinct RGBA clear,
and all 64 output pixels are checked. The results prove that enabled channels
receive the fragment color, disabled channels preserve the clear value, and an
empty mask leaves the pixel unchanged. An independent read-only review found
the expectations and strict hardware gate consistent with the public API.

The focused test passed on the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), with requested and
actual FL9_1. The full strict public D3D11 target passed 114/114, including
`experimental_dx11_is_not_enabled_by_default`. No backend capability changed.

Rustup's pinned `1.93.1-x86_64-pc-windows-msvc` installation was incomplete
(missing manifest), so tests, formatting and Clippy used the installed Rust
1.93 MSVC toolchain. Exact test commands:

```text
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test -p wgpu --features dx11-test-fixture --test dx11_public public_dx11_color_write_masks_preserve_disabled_channels_at_fl91 -- --exact --nocapture'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test -p wgpu --features dx11-test-fixture --test dx11_public -j2 -- --nocapture'
cargo +1.93 fmt --all -- --check
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cargo +1.93 clippy -p wgpu --features dx11-test-fixture --test dx11_public -j8'
git diff --check
```

All commands passed. Clippy reported five pre-existing D3D11 HAL style warnings
and three pre-existing `wgpu-core` unused-variable warnings; the new test
introduced no warning. Visual Studio's developer prompt printed
`vError: Unknown error` in its banner, but initialized successfully and all
build/test commands completed. Formatting and diff checks passed. The result
adds public execution evidence only; it does not close the broad operation
matrix, historical FL9_1/Lumia verification or public exposure gates.

Checkpoint: `f71993b7c` (`dx11: verify FL9_1 color write masks`).

## Public fragment discard color/depth test — 2026-11-09

Added `public_dx11_fragment_discard_preserves_color_and_depth_at_fl91` in
`wgpu/tests/dx11_public.rs`. Its interpolated vertex alpha makes columns 0–3
discard and columns 4–7 write red. The first strict readback checked all 64
pixels against the initial clear color and red output. A second draw at depth
0.4 with `LessEqual` passes only where the first draw discarded; it turns
those pixels green while the red pixels retain their first-draw depth of 0.25.
The second readback checked another 64 pixels. This establishes that the
existing Naga HLSL/FXC path preserves color and depth for this conditional
fragment `discard` shape; no backend capability changed.

An independent review found the test's interpolated threshold, color
expectations, depth proof, strict gate and queue/readback ordering sound. Strict
execution selected the physical NVIDIA GeForce RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`, software=false), requested and
confirmed FL9_1. FXC accepted the generated shader under
`ps_4_0_level_9_1`. The focused test passed 1/1 and the complete strict public
D3D11 target passed 115/115, including default-enumeration isolation.

Rustup's pinned `1.93.1-x86_64-pc-windows-msvc` installation was incomplete
(missing manifest), so test, formatting and Clippy checks used installed Rust
1.93. Exact commands:

```text
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test -p wgpu --features dx11-test-fixture --test dx11_public public_dx11_fragment_discard_preserves_color_and_depth_at_fl91 -- --exact --nocapture'
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && set WGPU_DX11_REQUIRE_HARDWARE=1 && cargo +1.93 test -p wgpu --features dx11-test-fixture --test dx11_public -j2 -- --nocapture'
cargo +1.93 fmt --all -- --check
cmd.exe /c 'call "C:\Program Files\Microsoft Visual Studio\18\Community\Common7\Tools\VsDevCmd.bat" -arch=x64 -host_arch=x64 && cargo +1.93 clippy -p wgpu --features dx11-test-fixture --test dx11_public -j8'
git diff --check
```

All commands passed. Clippy reported the five existing D3D11 HAL style
warnings and three existing `wgpu-core` unused-variable warnings; the new test
introduced none. The developer prompt printed `vError: Unknown error` in its
banner but initialized successfully. One earlier test compile caught three
test-only `DepthStencilState` type mismatches; they were corrected before the
passing focused and full runs. The result adds execution evidence only and
does not imply broader WGSL, discard, or historical-driver conformance.

Checkpoint: `834944c4e` (`dx11: verify FL9_1 fragment discard semantics`).

## RG8_SNORM FL9_1 copy probe — 2026-11-09

The selected RTX 3090 Ti reports `TEXTURE2D` support for R8G8_SNORM, but a
positive format query did not predict the result of the copy call at forced
FL9_1. A diagnostic public test created and uploaded source/destination
textures successfully. Submitting a valid equal-size `CopySubresourceRegion`
caused device removal with `HRESULT(0x80070057)` (`E_INVALIDARG`). Replacing
the call diagnostically with `CopyResource` caused the same failure. These
results are specific to this GPU/driver combination, not proof that every
FL9_1 implementation forbids the operation.

With copies omitted, the same diagnostic rendered the uploaded signed texture
through the public sampling/render/readback path and all 16 RGBA8 pixels
matched. The backend now conservatively rejects RG8_SNORM `COPY_SRC` even when
the device reports `TEXTURE2D`; the format remains sampled/upload-only. The
public copy test was removed, and the generic Core readback-mask fixture now
uses RGBA8 with a synthetic cleared mask bit instead of depending on an
unsupported RG8_SNORM copy usage. The D3D11 public adapter remains gated from
ordinary enumeration.

Focused validation after the conservative gate:

```text
cargo +1.93.1 test --offline -j2 -p wgpu-hal --lib --no-default-features --features dx11 rg8_snorm_texture_copy_source_stays_disabled_without_verified_backend_support -- --nocapture
# 1 passed; query-positive Rg8Snorm still has no COPY_SRC implementation path.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93.1 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std instance::dx11_fixture_tests::fl91_core_format_mask_rejects_texture_readback_before_hal -- --nocapture --test-threads=1
# 1 passed on RTX 3090 Ti at requested/actual FL9_1; synthetic RGBA8 mask exclusion rejected before HAL.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93.1 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_snorm_sampled_uploads_at_fl91 -- --exact --nocapture --test-threads=1
# 1 passed on RTX 3090 Ti; Rg8Snorm exposed COPY_DST | TEXTURE_BINDING only and all 64 rendered pixels matched.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93.1 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_snorm_copy public_dx11_rgba8_snorm_ -- --nocapture --test-threads=1
# 2 passed; RGBA8_SNORM full and partial GPU copies verified 80 readback pixels total.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93.1 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
# 463 passed, 0 failed, 0 ignored; strict hardware mode. Four pre-existing hal_queue.rs unused-qualification warnings.

cargo +1.93.1 clippy --offline -j2 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93.1 clippy --offline -j2 -p wgpu-core --tests --no-default-features --features dx11-test-fixture,wgsl,std
# both passed with existing warnings; no warning was introduced by the new fixture or capability test.

rustfmt +1.93.1 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs wgpu-core/src/dx11_fixture_tests.rs wgpu-hal/src/dx11/hal_device.rs
git diff --check
# both passed.
```

The normal DX11 enumeration test also passed on the RTX and found one physical
FL9_1 adapter when explicitly requested. The Visual Studio developer prompt
continues to display its `vError: Unknown error` banner, but initializes and
runs the MSVC toolchain successfully. A DX12 regression was not rerun for this
D3D11-only gate and Core fixture.

## Viewport remap compiler-boundary correction — 2026-11-09

The optional viewport-shader change committed as `ac6968b53` was withdrawn
after the Core/HAL audit found that FL9_x uses DWORDs internally for
`RSSetViewports`. Fractional viewport coordinates therefore also need the
adjusted vertex shader, including when the fractional rectangle is entirely
inside the render target. Core's downlevel limit rejects out-of-target
rectangles but accepts fractional in-target rectangles. With no per-pipeline
Core/HAL validation signal for a missing remap variant, preserving the base
pipeline and failing a later draw would move a valid request's failure to
submission. The HAL again requires the adjusted variant during pipeline
creation and reports an FXC/linkage failure if it exceeds the FL9_1 budget.
No capability changed.

Strict validation on the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, requested/actual FL9_1) passed the complete private HAL suite
at 463/463. The opt-in debug-layer indexed rendering test passed 1/1 when the
test log confirmed `D3D11 debug layer requested: true`; the layer reported
zero error/corruption messages and the live-object report contained only the
expected device and immediate context. The first invocation had logged
`requested: false`, so it was not counted as a debug-layer run. Four
pre-existing `hal_queue.rs` qualification warnings remain.

Commands:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'; $env:WGPU_DX11_DEBUG_LAYER = '1'; cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib fl9_1_hardware_indexed_triangle -- --nocapture --test-threads=1
cargo +1.93 fmt --all -- --check
cargo +1.93 clippy --offline -j2 -p wgpu-hal --tests --no-default-features --features dx11
git diff --check
```

Microsoft reference: [D3D11_VIEWPORT feature-level behavior](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_viewport).

## Invariant-position depth prepass — 2026-11-10

A strict public test checks WGSL `@invariant @builtin(position)` through two
draws: a depth-only prepass and an Equal-depth color pass. This tests that the
shader preserves the position needed for depth equality; it does not add a
capability or widen the shader contract.

On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), explicitly requested and actual FL9_1, FXC
accepted the shader and all 64 pixels in the 8x8 RGBA8 target matched the
expected red output. The strict public test passed 1/1.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_invariant_position -- --nocapture --test-threads=1
# 1 passed on physical RTX 3090 Ti at requested/actual FL9_1; 64/64 pixels matched.

rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_invariant_position.rs
git diff --check
```

Checkpoint: `d509e98c58a342730df9c423ac2e0844653ec6ff`
(`dx11: verify FL9_1 invariant position depth prepass`).

This is an additional public execution proof. The broad Core/HAL contract
audit, unsupported-operation closure, historical FL9_1 hardware, Lumia ARM32/
CoreWindow runtime and physical device-removal recovery remain open.

## Independent color-format usage projection — 2026-11-10

The RGBA8/BGRA8 linear and sRGB format-capability projection no longer
requires sampled, copy-source, copy-destination and render-target checks to
succeed as one combined texture descriptor. It now reports independently
validated usages. A synthetic negative `CheckFormatSupport` test verifies that
sampled plus copy-destination support remains visible when render-target
support is absent, while the production checker still rejects attachment and
copy-source requests. The RTX reports all of these support bits, so its public
projection did not change.

Verification:

```text
cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib format_projection -- --nocapture
# 7 passed, including the new synthetic per-usage case.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
# 465 passed, 0 failed, 0 ignored; strict FL9_1 private HAL suite.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
# 115 passed, 0 failed, 0 ignored; includes default adapter-enumeration isolation.

cargo +1.93 clippy --offline -j2 -p wgpu-hal --tests --no-default-features --features dx11
cargo +1.93 check --offline -j2 -p wgpu-hal --no-default-features --features dx12
rustfmt +1.93 --edition 2021 --check wgpu-hal/src/dx11/capabilities.rs
git diff --check
# passed; Clippy reported only existing D3D11 HAL warnings.
```

The hardware suites selected NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`) at requested/actual FL9_1 with `software=false`. The
synthetic query-negative test is CPU-only and covers a driver case unavailable
on this development adapter.

Checkpoint: `359d508228fe1dfa6f5754b8d278fb3e20719967`
(`dx11: project color format usages independently`).

## Multisample blend/write-mask interaction — 2026-11-10

Added a strict public GPU test for the combination of query-supported RGBA8
multisampling, alpha blending, partial red/blue write mask, resolve and CPU
readback. The disabled green and alpha channels retain their clear values;
red and blue distinguish the expected blend from replacement or an incorrect
write mask. The test loops over every advertised 2x/4x/8x/16x resolve count.

On the NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested and actual FL9_1, the adapter
reported blendable RGBA8 with 2x, 4x and 8x resolve. Each count produced
`[112, 96, 143, 64]` in all 64 pixels. No capability flag changed.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_multisample_blend_write_mask_resolve_at_fl91 -- --exact --nocapture --test-threads=1
# 1 test passed; the test executed three real GPU sample counts (2x, 4x, 8x).

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
# 116 passed, 0 failed, 0 ignored; includes default-enumeration isolation.

cargo +1.93 clippy --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
# passed; Clippy reported only existing D3D11 HAL warnings.
```

The broader matrix still has useful untested combinations and the selected
modern driver does not substitute for historical FL9_1 hardware. See the
platform and Core/HAL audit notes in this plan.

Checkpoint: `a1287ff037358ab6d603e0f0b2ae02a1bbf0b6d2`
(`dx11: verify FL9_1 multisample blend write masks`).

## R8_UNORM mip-copy pair matrix — 2026-11-10

The strict public `public_dx11_r8_multimip_texture_copy_at_fl91` test now
executes all 16 source/destination mip-index pairs between two four-level
R8_UNORM textures. Each copy uses a valid nonzero source and destination
origin where possible, reads that copied region back before later commands can
overwrite it, and compares it with the source. The test then reads back every
complete destination mip to verify final contents and untouched texels. Both
readback buffers use 256-byte rows initialized with `0xCD`; all copied bytes,
all preserved texture texels and all row-padding sentinels matched.

On NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`,
software=false), the test requested and selected FL9_1. The focused test passed
1/1. The complete strict public D3D11 target passed 117/117; three optional
subcases logged SKIP because the selected device did not report R8G8 vertex
input, Float16 vertex input or Rg8Unorm mip support. This completes selected-
device evidence for every cross-level R8 mip pair; it does not establish
historical-driver behavior.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_multimip_texture_copy_at_fl91 -- --exact --nocapture --test-threads=1
# 1 passed; all 16 pairs and four complete destination mips matched.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
# 117 passed, 0 failed; three optional subcases logged SKIP.

rustfmt +1.93 --edition 2021 --check wgpu/tests/dx11_public.rs
git diff --check
```

## Core sampler LOD validation — 2026-11-10

The Core/HAL sampler LOD mismatch is now described by the additive
`DownlevelLimits::sampler_lod_clamp` capability. The DX11 capability projection
sets it false for FL9_1 and FL9_2 and true for FL9_3 and above; other backend
defaults preserve their existing support. Core rejects non-default LOD ranges
combined with distinct min/mag filters or anisotropy before HAL, and treats
non-finite LOD values as validation errors. Same-filter isotropic cases retain
the existing single-mip normalization path. HAL still validates selected-view
mip compatibility at draw preparation.

The strict Core fixture ran on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, software=false), explicitly requested and selected
FL9_1. Two unrepresentable mixed-filter LOD descriptors returned Core
validation errors; creating a default-range mixed-filter sampler afterward
succeeded on the same device. The fixture completed its real buffer, texture,
render, synchronization and readback workload: 1 passed, 0 failed. The strict
public D3D11 integration target passed 117/117; optional hardware format
subcases remained explicit skips. The public zero-DX11-adapter isolation test
passed 1/1. CPU-only capability/predicate tests passed (1 wgpu-types, 2 Core,
1 HAL).

Commands included:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib sampler_lod_clamp_tests -- --nocapture
cargo +1.93 test --offline -j8 -p wgpu-types sampler_lod_clamp_limit_is_checked_and_affects_compliance
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib feature_level_guarantees_are_specific_and_do_not_promote_backend -- --nocapture
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The first attempt to compile the Core adapter-scaffold test omitted the `std`
and `wgsl` test features; it failed at test compilation. Re-running with the
required features passed and enumerated one selected physical FL9_1 adapter.
The ordinary public API test still enumerated zero D3D11 adapters by default.
An initial `wgpu-test` package check also exposed an exhaustive lowest-
downlevel fixture that omitted a previously added sampler-bias limit; that
fixture was corrected to explicitly keep the limit disabled, and the package
check then passed.

Additional verification passed: private HAL strict tests 467/467; DX12 HAL and
Core `cargo check`; `wgpu-test` and `wgpu-info` checks; GLES HAL check; changed
Rust-file rustfmt checks; `git diff --check`; and Clippy for wgpu-types, Core
fixture and HAL. Clippy emitted existing warnings in serde-disabled wgpu-types,
Core fixture code and unrelated D3D11 HAL code; no new warning points to the
sampler LOD changes. Historical hardware, Lumia/CoreWindow runtime, physical
removal/recreation, full CTS and public adapter enablement remain open.

## Selected-view sampler LOD compatibility — 2026-11-10

The previous LOD slice rejected unrepresentable descriptor combinations at
sampler creation but left a view-dependent case to the D3D11 HAL draw path.
Added `hal::SamplerMipCompatibility` metadata with a default unrestricted
implementation for other backends; D3D11 returns the classification already
computed during native sampler translation. Core stores this metadata with
the sampler and records the selected mip count for each sampled texture view.
Render-pass and render-bundle draw validation checks only the selected
pipeline's sampling pairs against the currently bound bind-group resources.
The check runs again after pipeline or bind-group changes and does not use a
stale compatibility cache. Texture and sampler binding arrays are checked
conservatively across their possible element pairs.

CPU tests passed for unrestricted, single-mip and raised-maximum samplers,
zero/multiple mip counts, anisotropic raised maxima and missing/empty metadata.
The strict Core fixture passed on NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`,
driver `32.0.16.2101`, software=false), requesting and selecting FL9_1. Its
new negative case used a sampler whose native maximum was raised from 0.5 and
a selected two-mip view; Core returned
`DrawError::IncompatibleSamplerMipRange` during command validation before HAL
execution. The enclosing fixture continued through subsequent GPU rendering
and readback cases, confirming the validation error did not lose the device.

The first test attempt incorrectly expected an error directly from
`render_pass_draw`; Core records draw commands and validates them when the
encoder is finished. The test now asserts the command-finish validation error.

Final verification for this selected-view compatibility change:

- Full strict `wgpu-core` library tests: 129/129 passed. The forced-FL9_1
  hardware fixture ran on the RTX 3090 Ti; one optional RG8_UNORM mip subcase
  logged SKIP because the selected device did not report the required format
  query.
- Full strict private D3D11 HAL tests: 467/467 passed, including hardware
  rendering and transfer tests.
- Full strict public D3D11 integration target: 117/117 passed. Three optional
  selected-device subcases logged SKIP (R8G8 vertex input, Float16 vertex
  input and Rg8Unorm mip support).
- Public default-enumeration isolation: 1/1 passed; ordinary D3D11 adapter
  enumeration remains disabled.
- DX12 Core and GLES HAL checks passed.
- HAL and Core fixture Clippy passed. Existing warnings remain in unrelated
  code; no new warning points to this change.
- Rust formatting checks and `git diff --check` passed. Git emitted its
  existing LF-to-CRLF working-copy notices.

Commands:

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib -- --nocapture --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public experimental_dx11_is_not_enabled_by_default -- --exact --nocapture
cargo +1.93 check --offline -j12 -p wgpu-core --no-default-features --features dx12,wgsl,std
cargo +1.93 check --offline -j12 -p wgpu-hal --no-default-features --features gles
cargo +1.93 clippy --offline -j12 -p wgpu-hal --no-default-features --features dx11 --tests
cargo +1.93 clippy --offline -j12 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --tests
rustfmt +1.93 --edition 2021 --check [changed Rust files]
git diff --check
```

This closes one late-HAL validation path only. Sampler compatibility remains
a narrow private contract; broad Core/HAL validation, historical FL9_1
hardware and Lumia/CoreWindow verification remain open.

## Uncalled dynamic-loop helper compiler probe — 2026-11-10

The source audit found that Naga writes regular module functions even when
they are unreachable from the selected entry point, while Core's FL9_1 loop
gate is selected-entry aware. A strict fixture now selects safe vertex and
fragment entries in a module that also contains uncalled regular functions
with dynamic loops. On the physical RTX 3090 Ti (`0x10de:0x2203`, driver
`32.0.16.2101`, software=false), requested FL9_1 selected L9_1 and FXC accepted
the pipeline. The fixture passed 1/1 and continued through the existing GPU
render/readback cases. This disproves the suspected Core-to-FXC rejection for
this shape; no broader Core loop gate was added.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
# 1 passed; the fixture logged that FXC accepted the safe selected entries.
```

The first invocation from a plain PowerShell failed before linking because
`link.exe` was not in that shell's PATH. Re-running from the installed Visual
Studio 2022 Build Tools developer environment passed. The `VsDevCmd.bat`
banner printed a non-fatal `Unknown error`, but the MSVC linker and compiler
completed successfully.

## Optional fractional-viewport shader variant — 2026-11-10

FL9_1 viewport remapping needs an adjusted vertex shader because level-9
`RSSetViewports` does not preserve fractional coordinates. FXC can reject that
optional shader at its instruction limit even while accepting the base shader.
The D3D11 pipeline now retains the base pipeline when only the remap shader
fails with a linkage/invalid-argument result, and reports whether the variant
exists through the dynamic HAL pipeline interface. Core stores that bit and
rejects direct draws using a nonempty fractional viewport only when the
selected pipeline lacks the variant. Render bundles record whether their
draws need a remap and are checked against the viewport active at execution.
Zero-area viewports do not require a transform. Device errors still fail
pipeline creation. Other backends use the default `true` implementation, so
their behavior is unchanged.

The strict public regression builds the 255-vector boundary shader whose base
vertex shader compiles while the remapped variant exceeds the FL9_1 profile.
It renders and reads back the ordinary full-target viewport, then verifies
Core rejects the fractional-viewport draw before HAL submission. The hardware
was NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`), driver `32.0.16.2101`,
software=false; requested and actual feature level were FL9_1.

```text
WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_uses_255_vertex_uniform_vectors_with_post_encoding_queue_write_at_fl91 -- --exact --nocapture --test-threads=1
# 1 strict hardware test passed; normal viewport readback matched 17 green and 4 red pixels.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib -- --nocapture --test-threads=1
# 130 passed, 0 failed; includes the physical FL9_1 Core fixture.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu-hal --no-default-features --features dx11 --tests -- --test-threads=1
# 467 passed, 0 failed; includes viewport shader, remapping, texture, sampler, uniform and transfer regressions.

WGPU_DX11_REQUIRE_HARDWARE=1 cargo +1.93 test --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
# 117 passed, 0 failed; optional selected-device capability subcases reported SKIP.

cargo +1.93 check --offline -j12 -p wgpu-core --no-default-features --features dx12,wgsl,std
# passed; three pre-existing unused-variable warnings in FL9 helper analysis.

cargo +1.93 clippy --offline -j12 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --tests
cargo +1.93 clippy --offline -j12 -p wgpu-hal --no-default-features --features dx11 --tests
cargo +1.93 clippy --offline -j12 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public
# all completed successfully; existing warnings in unrelated HAL/Core tests remain.

rustfmt +1.93 --edition 2021 --check [changed Rust files]
git diff --check
# passed; Git printed existing LF-to-CRLF working-copy notices.
```

The full `dx11_public` target includes `experimental_dx11_is_not_enabled_by_default`, which passed and confirms default D3D11 enumeration remains disabled. This fixes one over-rejection without advertising another capability. The next infrastructure slice is a downlevel-aware strict CTS request path; historical FL9_1 devices, Lumia/CoreWindow runtime, physical device removal/recovery and the broader Core/HAL operation audit remain open.
