# Direct3D 11 validation plan

The `dx11` feature enumerates only physical adapters that create the explicitly
requested FL9_1 device and pass the selected-device capability projection.
Only selected-device-gated features are exposed: the implemented 16-bit
normalized/packed texture paths and comparison-sampler border addressing when
the required query succeeds. The downlevel contract is not WebGPU-compliant.
A narrow public HWND surface is available with BGRA8 sRGB FIFO,
opaque composition, render-attachment usage and one frame of latency. Configure,
acquire and present are owner-thread-only. The strict public test uses a hidden
window. The bounded visible example pumps events, presents an animated cube
for 1,800 frames, and has a client-area capture showing distinct red and green
textured faces over the clear background. This remains experimental; the
HAL/core tests do not establish complete wgpu or CTS conformance.

Later sections include historical test checkpoints. Statements there that
public surface capabilities were unavailable or the adapter was offscreen-only
describe the state at those earlier checkpoints; the current HWND contract is
summarized above and in `CAPABILITIES.md`.

### Strict DX11 CTS selection

The xtask runner enables the experimental Deno DX11 feature only for an
explicit DX11 backend request. For a hardware-required run, set
`WGPU_DX11_REQUIRE_HARDWARE=1`; Deno then requires the backend mask to contain
only DX11 and rejects an absent or unverified physical FL9_1 adapter instead
of resolving `null`. The verified adapter identity is logged. Example:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo xtask cts --backend dx11 'webgpu:api,operation,command_buffer,basic:*'
```

The `basic:*` command was run in strict mode after obtaining the pinned CTS
checkout. All three cases failed during device creation (0 passed, 0 skipped,
3 failed): Deno supplies Core WebGPU default limits, including 28 combined
buffers per shader stage, while this experimental FL9_1 adapter reports 22. No
command-buffer operation ran, so this is not GPU execution evidence. The
rejection is expected for a deliberately non-Core-WebGPU adapter; other Core
defaults also exceed the FL9_1 projection. Do not raise projected limits or
lower Deno defaults to mask this contract mismatch. The strict preflight did
surface the request as failure rather than skip. The regular Deno backend mask
still excludes DX11 by default.

### Public fragment-discard color and depth test

`public_dx11_fragment_discard_preserves_color_and_depth_at_fl91` compiles an
interpolated conditional `discard` through Naga and FXC at FL9_1. It checks
that the discarded half preserves the cleared color and depth, then uses a
second depth-tested draw to show only discarded pixels pass. All 128 checked
pixels matched on the RTX 3090 Ti at requested/actual FL9_1.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test -p wgpu --features dx11-test-fixture --test dx11_public public_dx11_fragment_discard_preserves_color_and_depth_at_fl91 -- --exact --nocapture
```

### Public unimplemented-feature boundary test

`public_dx11_rejects_unimplemented_hal_features_at_fl91` verifies that the
physical FL9_1 adapter does not expose `PIPELINE_CACHE` or
`EXPERIMENTAL_RAY_QUERY`, and that requesting either feature fails before HAL
device open. This is a capability-boundary regression test, not a rendering
test.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_unimplemented_hal_features_at_fl91 -- --nocapture --test-threads=1
```

### Public buffer-clear transfer test

`public_dx11_clear_buffer_zeros_only_the_requested_range_at_fl91` uploads a
distinct 64-byte pattern, clears bytes 16–48, copies the buffer to a mapped
readback allocation and verifies that only the requested bytes changed. It
also submits unaligned and out-of-bounds clear ranges and checks that core
validation rejects them before HAL execution.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_clear_buffer_zeros_only_the_requested_range_at_fl91 -- --nocapture --test-threads=1
```

### Public offscreen integration test

The public integration test selects DX11 explicitly, asserts the physical
FL9_1 adapter identity and driver version, and requests a device within the
adapter's reported limits. It uploads a two-color texture at group 7 binding 3,
binds its sampler separately at group 4 binding 17, and binds a dynamic vertex
transform at group 0 binding 3 and dynamic fragment tint at group 0 binding 17.
This exercises sparse groups 0, 4 and 7 while reusing logical binding numbers
across group identities. An indexed textured scene uses those stage-visible
uniforms with a D24S8 depth attachment.
Readback checks all 64 pixels: 32 tinted-red pixels and 32 green pixels from
the depth-tested indexed draw. The test writes a translated transform into a
256-byte-aligned range and selects it with a dynamic offset between submissions;
the second frame contains 16 blue, 32 tinted-red and 16 green pixels. A later
draw keeps groups 0 and 4 bound while replacing only group 7's texture; it reads
back 32 blue and 32 tinted-yellow pixels. It then submits independent
clear/copy/readback workloads concurrently through the
first and a second public device from the same adapter. Strict mode turns
missing physical hardware into a test failure:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The same strict public test submits unsupported descriptors under validation
error scopes: unreported 16x multisampled color textures, `R32Float` sampled textures,
two-mip D24S8 attachments, vertex-visible storage-buffer layouts and
`D2Array` sampled-texture layouts. Each must return
`wgpu::Error::Validation`, confirming the public capability projection rejects
these paths before the private HAL.

### Query-gated 3D sampled texture test

`public_dx11_texture_3d_upload_and_sampling_at_fl91` requires a physical
adapter and requested/actual FL9_1 when `WGPU_DX11_REQUIRE_HARDWARE=1`. It
uploads a patterned 4×4×4 RGBA8 volume with 256-byte row pitch, applies a
single-voxel update from a nonzero source offset, samples all four depth slices
through sparse bindings 7 and 42, and verifies 64 pixels. A second volume is
partially written and rendered to verify zero initialization for all untouched
texels; another 64 pixels are checked. When the selected RGBA8 query reports
`D3D11_FORMAT_SUPPORT_MIP`, the test uploads three levels of a 4×4×4 volume,
submits the uploads, then samples mip 0, 1 and 2 through three suffix views and
checks all 64 output pixels. A view that omits the lowest mip and an overlong
mip chain are rejected through public validation scopes. Without the query,
the test instead verifies that a multi-mip request is rejected. NPOT
dimensions, a non-RGBA8 format and render-attachment usage are also rejected.
The RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) reports mip support
and passed at requested and actual FL9_1. The test proves this driver's
selected-level path, not behavior on historical 9_1 hardware.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_texture_3d_upload_and_sampling_at_fl91 -- --nocapture --test-threads=1
```

The strict public sampled-relationship boundary test compiles an entry point
with sixteen sparse texture/sampler relationships, accepts a one-pair sibling
entry point from the same WGSL module, and verifies that an entry point with
seventeen relationships is rejected by core validation before HAL. The private
HAL suite separately exercises sixteen relationships through GPU rendering.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_enforces_fl91_sampled_relationship_limit_per_entry_point -- --nocapture --test-threads=1
```

The development RTX 3090 Ti reported vendor `0x10de`, device `0x2203`, driver
`32.0.16.2101`, requested and actual FL9_1, and `software=false`. The main
image matched 32 tinted-red/32 green pixels; a queue write plus dynamic uniform
offset shifted the second image to 16 blue/32 tinted-red/16 green pixels. The
subsequent concurrent public-device workloads read back 64/64 red and 64/64
green pixels. The shader and bind-group layouts use groups 0, 4 and 7 with
binding numbers reused across groups. This is a representative public offscreen
integration case, not a complete application or adapter-contract audit.
All six unsupported resource requests were
rejected by public core validation.

### Query-gated FL9_1 comparison border test

`public_dx11_query_gated_fl91_comparison_border_sampling` and
`public_dx11_query_gated_fl91_comparison_border_white_sampling` require the
selected FL9_1 device to positively report D3D9 shadow support. They enable the
public border-addressing feature only for a `LessEqual` comparison sampler and
sample outside the depth view. Opaque black makes all 64 comparisons fail;
opaque white makes all 64 pass. Core validation rejects an ordinary border
sampler under the comparison-only downlevel limit and rejects
`SamplerBorderColor::Zero` because clamp-to-zero was not requested. The related
synthetic capability tests verify that an unavailable shadow query exposes
neither this feature nor depth sampling. Microsoft's [D3D9 shadow
contract](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_feature_data_d3d9_shadow_support)
explicitly permits border addressing for this comparison-sampler case at
FL9_1; it does not make general border samplers available.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_query_gated_fl91_comparison_border_sampling -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_query_gated_fl91_comparison_border_white_sampling -- --nocapture --test-threads=1
```

### Public render bundle execution

This strict test records a pipeline, vertex buffer and nonindexed triangle
draw in a public `RenderBundleEncoder`, executes the bundle in an ordinary
render pass, and validates all 64 pixels after GPU readback. It exercises the
basic bundle execution path; indexed draws and other bundle state combinations
are not covered by this case.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_executes_at_fl91 -- --nocapture --test-threads=1
```

The expected output is `[255, 64, 0, 255]` at every pixel. The test prints
adapter identity, driver, requested and actual feature level, and whether the
selected device is software-backed.

The indexed textured bundle test also executes a `Uint16` indexed triangle
sampling a sparse-bound texture/sampler pair. It drops the source texture,
view, sampler, bind group and vertex/index buffers after bundle recording; all
64 pixels must still match `[51, 128, 204, 255]` after submission. This checks
bundle resource retention for the tested draw shape.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_textured_render_bundle_retains_resources_at_fl91 -- --nocapture --test-threads=1
```

A second indexed bundle test records `Uint16` and `Uint32` TriangleStrips
containing primitive-restart markers. Both bundles execute in the same render
pass while switching index width, vertex transform and fragment color. The
left bundle renders green and the right bundle red; readback checks all 256
pixels of one 16×16 target, including the unchanged gap between them.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_triangle_strip_restart_render_bundle_at_fl91 -- --nocapture --test-threads=1
```

The multi-group bundle transition test records two bundles that use different
uniform and texture/sampler bind groups with one pipeline, drops the original
resources, then verifies that each half of the target has the expected color.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_transitions_multiple_bind_groups_at_fl91 -- --nocapture --test-threads=1
```

Dynamic offsets are captured independently in consecutive bundles. This test
uses offsets 0 and 256 from one uniform buffer and verifies that the two
bundles draw different exact colors after the source buffer and bind group are
dropped.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_dynamic_uniform_offsets_at_fl91 -- --nocapture --test-threads=1
```

The pipeline-layout transition test records a uniform-only bundle followed by
a texture/sampler-only bundle, then verifies each output after releasing the
source resources.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_pipeline_layout_transition_at_fl91 -- --nocapture --test-threads=1
```

The D16 bundle test executes a near red draw followed by a far green draw from
two bundles and checks that the writable depth attachment keeps all 64 pixels
red.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_depth_attachment_at_fl91 -- --nocapture --test-threads=1
```

The pass-state test executes one bundle twice while changing the pass-level
scissor rectangle and blend constant between executions. Readback verifies 32
red pixels on the left and 32 green pixels on the right; the bundle's vertex
buffer is released before submission.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_preserves_pass_blend_and_scissor_state_at_fl91 -- --nocapture --test-threads=1
```

The stencil-reference bundle test executes one bundle twice with different
pass-level references and scissor rectangles. Later comparison draws verify
the references were stored in the correct regions. This also covers Core's
reapplication of the pass reference after a bundle selects a stencil-using
pipeline.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_preserves_pass_stencil_reference_at_fl91 -- --nocapture --test-threads=1
```

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_render_bundle_dynamic_uniform_offsets_at_fl91 -- --nocapture --test-threads=1
```

### Public HWND surface smoke test

The public surface test creates a hidden top-level HWND, requests a compatible
physical FL9_1 adapter, configures a 64×64 BGRA8 sRGB FIFO surface, clears and
presents a frame, drops its view, resizes to 80×48, and presents a second
frame. It obtains its initial settings from `Surface::get_default_config`;
core resolves the returned `Auto` color/alpha values and clamps the generic
two-frame latency hint to the D3D11 surface's supported one-frame range. It
then verifies that acquisition from a different thread is rejected.
It also acquires a frame on the owner thread, attempts presentation from a
worker, checks the reported validation error, then confirms the owner thread
can acquire and present another frame. A duplicate acquisition returns a
validation error before HAL and leaves the first frame presentable; acquisition
from the wrong thread also returns validation rather than `Lost`. A rejected
`Queue::present` consumes its public `SurfaceTexture`; core must release the
acquired native image so the surface does not remain stuck.
Cloned public `SurfaceTexture` handles share the acquired frame's lifecycle:
dropping a non-final clone on a worker thread preserves the image, presenting
one clone prevents later aliases from discarding it, and dropping the final
unpresented clone discards it so the surface can acquire again.
The test also drops a `Surface` while its unpresented frame remains alive, then
drops the frame and verifies that cleanup does not access the removed core
surface ID. It also attempts to present a retained frame after surface
destruction and checks for the invalid-surface validation error rather than a
stale-ID panic while a second HWND surface has a frame acquired, then presents
that second frame normally. It also verifies final unpresented-frame discard
remains safe after destroying the second surface.
The D3D11 surface operations must run on the HWND's creating thread. A shared
queue gate serializes submissions, writes, mapping flushes, polling and
context-backed resource creation against `Present`; reentrant mutations return
an error, and same-thread reentrant `unconfigure` logs and returns without
waiting. The test is not a visible display test;
use the windowed example below to exercise the message loop and swapchain
presentation.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_hwnd_surface_presents_on_its_window_thread -- --nocapture --test-threads=1
```

The fixture-only public surface test injects `DXGI_ERROR_DEVICE_REMOVED` at
the native Present boundary. It verifies that the public device-lost callback
runs once and that polling re-entered from the callback returns the
presentation-guard timeout. The HRESULT is synthetic; the test does not
remove the physical device.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11-test-fixture,std --test dx11_surface_error_fixture -- --nocapture --test-threads=1
```

Surface reservation tests verify that `configure` and `acquire_texture` fail
fast during another operation, `unconfigure` waits when called by another
thread, and same-thread reentrant `unconfigure` returns without waiting. The
last case leaves the current configuration intact so the caller can retry
after the active operation ends. This tests the reservation logic directly;
it does not force DXGI to deliver a synchronous window callback.

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib unconfigure_ -- --nocapture --test-threads=1
```

### Visible public HWND example

`wgpu/examples/dx11_fl91_windowed.rs` opens a visible native HWND, uses the
public `dx11` adapter and surface APIs, pumps Windows messages, and renders a
rotating indexed textured cube with depth testing. It handles client-area
changes by reconfiguring the swapchain and recreating its depth target. Close
the window to stop it. For a bounded run, pass `--frames=N`:

```powershell
cargo +1.93 run --offline -j32 -p wgpu --example dx11_fl91_windowed --no-default-features --features dx11,wgsl,std -- --frames=120
```

A 60-frame run passed on the physical RTX 3090 Ti at requested/actual FL9_1.
The example checks that the HWND is visible and reports presented frame count.
It is not a pixel-readback test; use the offscreen example and strict
integration suite for deterministic pixel validation.

The public test also draws fully overlapping near and far textured surfaces
with the same depth-tested pipeline, in both far-then-near and near-then-far
orders. Every readback pixel remains the nearer surface's tinted-red value in
both cases, verifying depth comparison and writes through public wgpu rather
than only checking that a depth attachment can be created.

The same public render initializes its vertex input with
`mapped_at_creation`, fills the mapped range, and unmaps before encoding the
draw. Its later GPU readback confirms that the core staging copy committed the
mapped contents before vertex use.

### Public FL9_1 textured 3D example

`wgpu/examples/dx11_fl91_offscreen.rs` is a standalone public-API diagnostic.
It selects an enumerated physical D3D11 adapter only after explicit FL9_1
device creation, reports the adapter identifiers and driver, then renders an
indexed, textured, depth-tested cube into an offscreen target. The example
maps vertex and index data at creation, uploads a texture and transform,
submits through a public queue, waits for completion and checks all 1,024
pixels against the texture colors and clear color. Pass an optional output path
to save the verified image as PPM.

Run in a Visual Studio x64 developer shell. The example itself requires a
physical FL9_1 adapter and exits with an error when one is unavailable; it
does not substitute WARP.

```powershell
cargo +1.93 run --offline -j16 -p wgpu --example dx11_fl91_offscreen --no-default-features --features dx11,wgsl,std
```

On the RTX 3090 Ti the run reported vendor `0x10de`, device `0x2203`, driver
`32.0.16.2101`, requested/actual FL9_1 and `software=false`; readback contained
112 red, 277 green and 635 blue background pixels. This is public offscreen
execution evidence. It does not provide a window, presentation or surface
capabilities.

A second public test checks `R8Unorm`'s selected-device format report, uploads
two distinct values, samples them with a non-filtering sampler and renders to
RGBA8. The format reports `TEXTURE_BINDING | COPY_DST`; its `SINGLE_MIP_ONLY`
flag is omitted only when the selected format query and implemented backend
path permit R8 mip chains. This single-mip case checks all 64 pixels, with 32
red values of 64 and 32 red values of 192. It runs under the same strict
hardware command above.

The same public test checks the optional `Rg8Unorm` projection. It is exposed
after the selected device reports `TEXTURE2D`; the synthetic capability test
covers positive, unavailable and incomplete queries. A positive
`SHADER_SAMPLE` bit means `Sample` supports any filter; without it, the
backend limits use to point filtering. The strict test verifies nearest
sampling and, on this device, a linear-filter gradient.
When unavailable, the public case prints an explicit capability `SKIP` while
retaining the R8 baseline result. On 2026-11-05, the RTX 3090 Ti at requested
and actual FL9_1 returned `0x0002c320` for `R8G8_UNORM`. The nearest test
uploaded two distinct RG texels and sampled their red channel into an 8×8
RGBA8 target; all 64 pixels matched (32 with red 64, 32 with red 192), proving
the two-byte texel stride in the actual upload and sampling path. The linear
test produced the expected eight-column gradient in all 64 output pixels. The
selected format projection reports only `TEXTURE_BINDING | COPY_DST`; it
reports `SINGLE_MIP_ONLY` when the queried format or backend lacks mip-chain
support. A separate `RENDER_TARGET` query enables the narrow attachment path
below. The query-gated full mip chain is exercised by the sampled-mip test.

### R8/RG8/RGB565 color attachments

R8G8_UNORM and B5G6R5 remain single-mip, one-layer, one-sample resources.
R8_UNORM mip chains require `TEXTURE2D | MIP`; NPOT chains additionally require
unrestricted-NPOT support. Mip chains can combine with render-target and copy
source usage only when the selected device reports `RENDER_TARGET` and the
matching backend path is enabled. `SHADER_SAMPLE` separately controls
filterability. Color-attachment usage is reported only when the selected device's
`CheckFormatSupport` query positively includes
`D3D11_FORMAT_SUPPORT_RENDER_TARGET` (`0x00004000`). Each native texture has
both RTV and SRV bind flags; B5G6R5 additionally requires the native-only
`TEXTURE_FORMAT_16BIT_PACKED` feature. The public test creates separate
attachment-only and texture-binding-only views, draws into the former, samples
the latter, and checks RGBA8 readback. `COPY_SRC` and `BLENDABLE` are separately
gated by the selected query and backend paths.

On the RTX 3090 Ti at requested/actual FL9_1, the query masks were
R8_UNORM=`0x0002d3e0`, R8G8_UNORM=`0x0002c320` and B5G6R5=`0x0026f3a0`,
each including the render-target bit. The test draws a deterministic color
into each format, samples the result and checks all 64 pixels per format with
a one-code tolerance for UNORM conversion. The first observed pixels were
`[66, 0, 0, 255]`, `[66, 129, 0, 255]` and `[66, 130, 132, 255]`. The RTX
reported `BLENDABLE` for all three formats. A second pass overlays half-alpha
red with identical color and alpha equations, staying within FL9_1's
separate-alpha restriction; readback verified all pixels as
`[160, 0, 0, 255]` (R8),
`[160, 65, 0, 255]` (RG8) and quantized `[165, 65, 66, 255]` (RGB565).
On devices without the optional selected-query bit, this blend subcase is
reported as skipped while the attachment/sample test still executes.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_rg8_rgb565_render_attachments_at_fl91 -- --nocapture --test-threads=1
```

A third strict public test writes stencil reference value 1 over the target,
renders green where `Equal` succeeds, then attempts a blue draw with reference
0. All 64 pixels remain green. The same test verifies that asymmetric front/back
stencil state is rejected by public core validation before HAL. This covers the
implemented symmetric stencil subset through public wgpu, not two-sided stencil
support.

A fourth strict public test blends a premultiplied red overlay over opaque blue
with identical color and alpha equations. All 64 pixels read back as
`[128, 0, 128, 255]`. Separate color/alpha equations and source-alpha-saturated
factors are each rejected by core validation before HAL.

A fifth strict public test draws a fullscreen triangle through a 4×6 scissor
inside an 8×8 target. Readback confirms 24 green pixels inside and 40 unchanged
blue pixels outside. The same test submits an out-of-target viewport and
confirms core rejects it before HAL under the selected FL9_1 projection.

The strict public sparse multi-texture test declares two fragment textures and
two independent samplers across groups 1, 4, 6 and 7. Logical bindings are
sparse (7, 100, 42 and 17), and the WGSL declaration order differs from group
and bind-group layout order. The shader samples both textures at the same
coordinate using nearest and linear filtering respectively. The patterned
inputs make a sampler/register swap observable. In one render pass, the test
then replaces only group 7 with a nearest sampler and draws into the other half
of a 16×8 target, retaining the pipeline, both textures and the other sampler.
Readback verifies all 128 pixels: the linear half is `[40, 43±1, 0, 255]` and
the nearest half is `[40, 40, 0, 255]`. An incomplete texture group is
rejected by public core validation before rendering.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sparse_multi_texture_sampling_at_fl91 -- --nocapture --test-threads=1
```

A seventh strict public test uses all eight sampled-texture slots advertised
for one FL9_1 shader stage. Eight independently uploaded textures at sparse
bindings each contribute a distinct value through a shared sampler, and all 64
pixels match `[180, 72, 36, 255]`. A nine-texture fragment layout is rejected
by core validation before HAL.

An eighth strict public test initializes two RGBA8 textures, copies a 2×2
region from a mapped buffer with a nonzero source offset and padded row pitch,
then copies that region between differently sized textures at nonzero source
and destination origins. It copies the final 6×6 destination to a padded
readback buffer and verifies every texel, including all unchanged green
pixels. The test exercises buffer-to-texture, texture-to-texture and
texture-to-buffer paths in one ordered submission.

A strict public R8_UNORM upload test additionally uploads all four levels of an
8×8 mip chain when the selected format query reports `TEXTURE2D | MIP`, then
samples mip 2 and validates all 64 output pixels. When the selected R8 query
also reports `RENDER_TARGET`, the same texture is rendered into at mip 0 and
copied to a padded readback buffer; all 64 R8 texels match white. A separate RGBA8 test covers
implicit derivatives and literal sample biases -3, +1 and +3, including +1
forwarded through a helper, across 384 readback pixels. Dynamic, out-of-range
and offset biases are rejected by core validation before HAL compilation.
CPU-only core analysis tests cover fractional bias, nested helper forwarding,
multiple call sites and out-of-range helper arguments; RGBA8 coverage also
includes query-gated NPOT dimensions.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_sampled_mip_chain_at_fl91 -- --nocapture --test-threads=1
```

The same strict test also expects core validation errors for direct and
helper-reached `dpdx` / `dpdy` and `textureSampleGrad` entry points. CPU-only
Naga interface tests verify selected-entry and helper-call tracking. An FXC
compiler test records that `ddx` cannot map to `ps_4_0_level_9_1` and
`SampleGrad` is unsupported by that profile.

Explicit `textureSampleLevel`, including level zero, is a separate operation
from implicit sampling. FXC rejects literal and dynamic explicit LOD with
`X4532: texlod not supported on this target` for both
`ps_4_0_level_9_1` and `ps_4_0_level_9_3`. Core's `texture_sample_level`
downlevel limit rejects selected entries that use `SampleLevel::Exact`
directly or through a helper, and permits an implicit-sampling sibling. The
strict public test exercises these validation outcomes; HAL compiler tests
capture the profile diagnostics. This capability is modeled as available at
FL10_0 and above, but the current D3D11 backend execution profile remains
FL9_1-only.

FXC also rejects `GetDimensions`-based HLSL emitted for `textureDimensions`,
level-specific dimensions and `textureNumLevels` with X3548 under both
`ps_4_0_level_9_1` and `ps_4_0_level_9_3`. The float-output HLSL overload does
not map to either 10Level9 pixel instruction set; the full shader-model 5
control compiles. Core's `texture_query` limit rejects these operations in the
selected entry point, including helper calls, before HAL, while safe sibling
entries remain accepted. The D3D11 projection keeps this implementation limit
false even though the hardware model records query support from feature level
10_0 onward.

The general `texture_sample_base_clamp_to_edge` limit remains false. The
`texture_sample_base_clamp_to_edge_2d_float_fragment` limit covers one narrow
FL9_1 form of `textureSampleBaseClampToEdge`. Core proves one fragment sample
using one group-0 `texture_2d<f32>` and one group-0 non-comparison sampler,
either directly or through one reachable straight-line helper called once with
a `vec2f` argument and returning `vec4f`. Resource handles cannot pass through
the helper; selected extra resources, other image operations and control flow
are rejected before HAL. HAL requires a single-mip view and updates the `b13`
width/height constants whenever the selected binding changes.
Naga's writer emits an implicit level-zero `Sample` with half-texel coordinate
clamping. The helper may receive a computed `vec2f` from the entry point; the
helper itself must directly sample the selected texture and sampler, with no
nested calls or control flow. FXC accepted the helper at
`ps_4_0_level_9_1`, and strict forced-FL9_1 public readback verified all 64
pixels on the RTX 3090 Ti. The public test uses a
ClampToEdge sampler because Core rejects Repeat/MirrorRepeat descriptors under
the adapter's finite repeat-per-primitive limit. It does not establish
sampler-independent edge behavior for those unavailable sampler modes.

```powershell
cargo +1.93 test --offline -j8 -p naga --lib --features hlsl-out fl9_base_clamp_uses_host_dimensions_and_implicit_sample
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib base_clamp_to_edge_sampling_follows_selected_entry_and_helper_calls
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib base_clamp -- --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_texture_sample_base_clamp_to_edge_at_fl91 -- --nocapture --test-threads=1
```

### Texture gather shader-model boundary

Microsoft documents HLSL `Gather` for graphics profiles at shader model 4.1
(FL10_1), while compute gather4 requires SM5 (FL11_0). Because the shared
stage-agnostic limit applies to every selected entry point, the all-stage
hardware guarantee starts at FL11_0. The current FL9_1 backend disables
`DownlevelLimits::texture_gather`. The selected-entry analysis checks Naga's
`ImageSample::gather` expression and follows reachable helper calls. This
prevents FXC failures from escaping as backend compilation errors while
allowing an unused gather helper or an implicit-sampling sibling entry.

```powershell
cargo +1.93 test --offline -j32 -p wgpu-types --lib texture_gather_is_a_downlevel_capability
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl texture_gather_limits_follow_selected_entry_and_helpers
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 --lib texture_gather_is_rejected_by_level_9_1_fxc_profile -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_api_renders_offscreen_at_fl91 -- --nocapture --test-threads=1
```

The compiler test records FXC's actual level-9_1 rejection and verifies that
the Naga output compiles under pixel shader model 4.1, pixel shader model 5.0
and compute shader model 5.0. The core analysis test covers selected fragment
entries plus direct and helper-reached compute entries. The strict public test
verifies that core rejects direct and helper-reached gather before HAL on the
selected FL9_1 hardware device and still creates a pipeline for its
implicit-sampling sibling. This does not claim that the current backend
implements gather or that the RTX simulates historical FL9_1 drivers.

A tenth strict public test uploads both `Rg8Snorm` and `Rgba8Snorm`, samples
them with one non-filtering sampler, converts signed values to visible
unorm output and checks all 64 pixels against `[192, 63, 160, 255]` within
one channel value. Both format projections permit filtering because their
selected-device `SHADER_SAMPLE` bits are set, though this rendering case uses
nearest sampling. They remain single-mip-only.

An eleventh strict public test draws per-instance offsets and colors from a
second vertex stream using `draw(0..3, 1..3)`. It verifies that the nonzero
first-instance range selects the second and third records, while the skipped
first record remains absent. A later strict test verifies `instance_index`
through the host-updated FL9_1 constant path; this earlier result predates that
implementation.

The controlled FL9_1 core fixture also checks that a two-mip D24S8 render
attachment is rejected by the missing `DEPTH_TEXTURE_MULTI_MIP` downlevel flag
before reaching HAL. This validates the core boundary; it does not execute a
multi-mip depth texture on the GPU.

The core sampler-repeat validator is CPU-tested against a synthetic finite
repeat limit, including both `Repeat` and `MirrorRepeat`, unaffected clamp
modes, and the unbounded sentinel. The D3D11 capability model separately checks
the static repeat guarantees for all seven recognized feature levels:

```powershell
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampler_repeat_limit -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 feature_level_guarantees_are_specific_and_do_not_promote_backend -- --nocapture --test-threads=1
cargo test --offline -p wgpu-types downlevel_limits_check_minimum_capabilities -- --nocapture
```

These tests are synthetic/CPU-only. They do not claim that the RTX validates
behavior at or beyond the FL9_1 repeat cap.

Sampler anisotropy uses both the `ANISOTROPIC_FILTERING` availability flag and
the numeric `DownlevelLimits::max_sampler_anisotropy` cap. Synthetic tests
cover capability matching and core's clamp to 1, 2 and 16; the strict core
fixture requests 2, 3 and 16 on a physical FL9_1 device whose projected cap is
2. The HAL receives a valid FL9_1 sampler state in each case. This validates
descriptor translation and clamping, not anisotropic image quality.

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib sampler_anisotropy_is_a_numeric_downlevel_limit
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampler_anisotropy_limit
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The source-alpha-saturated blend capability is false in the controlled FL9_1
core fixture. Its hardware test constructs pipelines with the factor in each
color/alpha source/destination slot and checks that core returns the specific
unsupported-factor validation error before HAL pipeline creation. The wgpu-types
test verifies downlevel requirement matching and that the missing capability
prevents WebGPU-compliance reporting:

```powershell
cargo test --offline -p wgpu-types source_alpha_saturated_blend_is_a_downlevel_capability -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib mip -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_query_gated_npot_mip_chain_at_fl91 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### FL9_1 shader-loop validation

The core CPU test verifies static loops remain allowed, runtime-dependent
exits are found through direct code and helper calls, local aliases are
tracked, and safe sibling entries remain unaffected. The public strict test
renders static vertex and fragment loops on forced-FL9_1 hardware and verifies
dynamic vertex and fragment pipelines are rejected by core before FXC. A
private HAL compiler test retains the underlying X3511 boundary evidence.

```powershell
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl validation::tests::shader_loop_detection_follows_only_selected_entry_calls -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --features dx11 --lib dx11::hal_shader::tests::hal_module_compiles_and_creates_level_9_shaders -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### Fallible mapped-buffer readback allocation

`read_byte_buffer` allocates its output with `try_reserve_exact` before it
creates and maps the staging buffer. Capacity/allocation failure maps to
`E_OUTOFMEMORY`; no D3D11 resource has been mapped at that point. The CPU-only
boundary test forces the allocator's capacity-overflow error using
`usize::MAX`. The strict private HAL suite and public D3D11 suite also exercise
normal mapped-buffer invalidation and readback on the selected hardware.

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 --lib readback_allocation_reports_out_of_memory_without_panicking
```

### FL9_1 packed 16-bit sampled formats

The strict public test checks the selected-device format reports and performs
three separate padded uploads and sampled renders for RGB565, RGB5A1 and RGBA4.
It verifies all 64 pixels in each output, including transparent alpha from the
1-bit and 4-bit formats. The native-only capability unit test synthesizes
unavailable, resource-only, sample-capable and negative queries for every
format and checks that absent backend support does not project the format.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --features dx11 packed_16bit_sampling_requires_each_selected_device_query_and_backend_path
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rgb565_sampled_upload_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rgb5a1_rgba4_gpu_texture_copies_at_fl91 -- --nocapture --test-threads=1
```

The packed sampled/upload path supports single-mip 2D
`TEXTURE_BINDING | COPY_DST` resources. RGB5A1 and RGBA4 additionally permit
same-format GPU texture-to-texture copies when their selected-device query
reports `TEXTURE2D`; the path does not query or bind `RENDER_TARGET` and remains
excluded from texture-to-buffer readback. The query-gated B5G6R5 target path
may combine `TEXTURE_BINDING`, `COPY_DST` and `RENDER_ATTACHMENT` as supported
by the private predicate; it still has no copy-source or direct-readback path.
None of these paths adds mip-chain support. The strict GPU test verifies a
partial 2×2 copy at destination origin (1, 1) for RGB5A1 and RGBA4, samples
each result into RGBA8, and checks copied colors plus unchanged white border
texels.

### Internal FL9_1 capability projection

`Capabilities::wgpu_projection` is exercised against synthetic records and an
explicitly selected physical adapter. It returns a snapshot only for an
actually created FL9_1 device. Both ordinary opt-in D3D11 enumeration and the
core fixture consume the projected limits, alignments and downlevel gates. The
fixture uses the production projection's narrow
`TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM` and
`TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB` features independently when all three
selected-device sampled-upload paths in each family are available. The broad
`TEXTURE_COMPRESSION_BC` feature remains withheld because BC4–BC7 are not
implemented.

```powershell
cargo +1.93 test --offline -j 16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j 16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The projection test records the selected adapter identity, requested and actual
feature level, driver format/optional-feature queries, and conservative limits
and flags. The RTX's forced FL9_1 result is not evidence for historical FL9_1
drivers.

For optional R8, RG8 and RGB565 attachment formats, synthetic capability tests
also separate `RENDER_TARGET` from `BLENDABLE`. A query with `BLENDABLE` but no
render-target bit must not project either attachment or blend support; a query
with both bits projects blendability only when the corresponding backend
attachment path is enabled.

```powershell
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib rgb565_color_attachment_requires_the_selected_device_query
```

Microsoft documents this as a static 10Level9 restriction, not a per-format
blendability query: [10Level9 device methods](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device).

### Fragment builtin input validation

The tested `ps_4_0_level_9_1` FXC profile rejects fragment
`@builtin(front_facing)`, including a declaration whose value is unused.
Core therefore rejects a selected entry point declaring that input. Microsoft
documents that 9_x pixel shaders cannot read fragment `@builtin(position)` /
HLSL `SV_POSITION`; for this builtin, core rejects actual selected-entry use
and permits unused declarations. The controlled FL9_1 profile reports
`fragment_front_facing = false` and `fragment_position = false`. Both gates
leave safe sibling entry points usable. CPU interface tests cover direct,
helper-function, structure-member, unused-declaration and selected-entry
position detection. The `front_facing`
gate reflects the tested compiler profile, while the position restriction is
documented for FL9_x.

```powershell
cargo test --offline -p wgpu-types --lib fragment_position_is_a_downlevel_capability -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fragment_position_detection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### FL9_1 texture-load validation

The D3D11 projection sets `DownlevelLimits::texture_load` to false. Core
analyzes selected vertex and fragment entry points, including called helper
functions, and rejects reachable `textureLoad` operations before private HAL
pipeline creation. A safe sibling entry point in the same module remains
usable. This is a backend/compiler subset restriction; the experimental native
texel-load limit probe is not enabled for application pipelines.

```powershell
cargo +1.93 test --offline -j16 -p wgpu-types --lib texture_load_is_a_downlevel_capability
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std sampled_texture_count_uses_the_selected_entry_point
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

CPU tests cover direct and helper-reachable loads, safe sibling entry points,
stage selection and missing names. The strict core fixture exercises the
selected-device projection before HAL, and the public integration test verifies
the same validation error through wgpu while a sibling pipeline is accepted.
The surrounding public render tests execute on the GPU; texture-load rejection
itself occurs before GPU work.

### FL9_1 vertex shader system-value emulation

The selected FL9_1 projection supports a bounded `vertex_index` subset for
TriangleList, PointList, LineList, LineStrip and TriangleStrip pipelines. The backend does not emit
`SV_VertexID`: FXC rejects that native semantic with X4555. Naga maps the
selected input to a custom float IA semantic, and command replay binds identity
or temporary remapping buffers. Core permits direct `u32`-to-`f32` use and
rejects integer arithmetic. The
`DownlevelLimits::vertex_index_topology_mask` reports TriangleList, PointList,
LineList, LineStrip and TriangleStrip as supported; the HAL retains a defensive
check. Indexed TriangleStrip resolves base vertex and restart runs before
winding-preserving expansion to triangle-list identity values.

The same private shader path also emulates `instance_index` from a host-updated
constant at b13. CPU tests cover selected-entry detection and restricted integer
semantics. FXC tests distinguish rejected native system semantics from the
custom Naga-generated input that compiles under `vs_4_0_level_9_1`.

Run the strict hardware test from a Visual Studio x64 developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_vertex_index_triangle_list_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_vertex_index_point_list_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_vertex_index_line_list_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_line_list_restores_user_vertex_stream_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_vertex_index_line_strip_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_vertex_index_triangle_strip_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The tests require a physical adapter and explicitly request and verify FL9_1.
On the RTX 3090 Ti, the existing list tests check complete 8x8 readbacks. The
red-pixel counts were 32 for a normal nonindexed draw, 0 for a degenerate
indexed draw, 12 for
a negative base-vertex draw, 32 for reindexed Uint32 indices, 32 for
`first_vertex=100000`, and 32 after temporary-stream use. LineList tests
additionally verify direct, Uint16 and Uint32 indexed output and a same-pass
indexed Gather transition with a nonempty user vertex stream. The tested driver
is `32.0.16.2101`; historical FL9_1 driver behavior remains unverified.

`public_dx11_vertex_index_line_strip_at_fl91` verifies a direct draw starting
at vertex 100,000 across the native strip-batch boundary; Uint16 and Uint32
restart-separated strips with positive base vertex; a Uint32 strip with
negative base vertex; and a wide-span pair that uses Gather remapping. Seven
distinct output rows depend on the selected `vertex_index` values, and all
other rows are checked clear. The shader uses no vertex buffers, isolating the
emulated system value and compact index mapping.

`public_dx11_vertex_index_triangle_strip_at_fl91` verifies direct
`first_vertex=100000`, Uint16 and Uint32 restart-separated strips with positive
and negative base vertices, and wide-span Gather remapping. Six output bands
depend on the selected IDs; the RTX 3090 Ti readback contained 2,942 red pixels
and clear pixels elsewhere. The focused strict test and full public target
passed at requested/actual FL9_1.
### Out-of-target viewport validation

The FL9_1 fixture reports
`DownlevelLimits::viewport_rectangles_outside_target = false`. Core rejects a
viewport extending past the active target before it reaches the private HAL;
this avoids a late command failure when the optional shader variant required
for viewport remapping exceeds the FL9_1 compiler profile. CPU tests cover all
four target edges, and the controlled fixture exercises the production command
validation path:

```powershell
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std viewport_target_bounds_cover_all_edges -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The first is CPU-only. The second uses a physical adapter forced to FL9_1 for
the fixture's rendering/readback cases; its out-of-target viewport case is a
validation-only negative case and does not issue a GPU draw.

The out-of-target limit does not reject fractional viewports contained by the
target. FL9_x `RSSetViewports` uses DWORDs internally, so the HAL requires its
adjusted vertex shader to preserve fractional coordinates too. Pipeline
creation therefore continues to report an FXC/linkage failure if only that
variant exceeds the level-9 instruction budget; deferring the missing-variant
failure until draw submission would accept a Core-valid request that HAL
cannot execute. `fl91_viewport_fractional_coordinates` verifies the supported
path, while `fl91_naga_viewport_transform_preserves_base_shader_budget`
captures the compiler budget boundary.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib fl91_viewport_fractional_coordinates -- --nocapture --test-threads=1
cargo test --offline -j2 -p wgpu-hal --no-default-features --features dx11 --lib fl91_naga_viewport_transform_preserves_base_shader_budget -- --nocapture --test-threads=1
```

The first test executes on strict physical FL9_1 hardware. The second is
CPU/FXC-only and verifies that the base shader compiles while the remapped
variant exceeds the profile budget.

Single-mip-only format metadata is checked through the strict controlled core
fixture. Queried sampled-upload formats can create one-mip textures; a POT
8×8, two-mip descriptor for each reported single-mip format must fail in core
with `InvalidMipLevelCountForFormat`, before HAL texture creation. The existing
BC1 core render case separately confirms its one-mip upload and sampling path
still works.

### Per-format capability projection

`Capabilities::texture_format_capabilities` is the production internal
projection used by the non-enumerated core fixture. CPU tests verify that
queried support, feature-level shape and backend implementation all intersect,
and that missing/negative queries report no format capability:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11,dx11-test-fixture format_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std -- --nocapture --test-threads=1
```

The hardware fixture requested and selected FL9_1 on the RTX 3090 Ti and
printed its RGBA8, D24S8 and BC1 core format projections. It now also checks
that normal HAL enumeration exposes a physical adapter using the same
projection. These checks cover the reported subset, not every wgpu operation.

The fixture also prints and asserts RGBA8/BGRA8 `SINGLE_MIP_ONLY` against the
selected device's `D3D11_FORMAT_SUPPORT_MIP` query. The RTX 3090 Ti reports MIP
for both formats at forced FL9_1, so the projected formats permit their
implemented POT mip paths. The synthetic `color_format_projection_limits_mips`
test removes MIP support or a backend mip path and verifies the real projection
reports single-mip-only. This synthetic case covers a restrictive driver that
the development GPU cannot represent.

## 1. HAL tests and GPU readback

### Focused FL9_1 hardware test

With strict hardware mode enabled, run the private transient-attachment checks:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 transient_attachment -- --nocapture --test-threads=1
```

The filter passed two cases on the RTX 3090 Ti at requested and selected
FL9_1: one CPU usage-validation test and one physical GPU test. The GPU test
checked all 64 pixels for transient depth followed by readable color, and for
transient color followed by stored depth and readable color. The backend uses
ordinary textures for this path, so the test does not establish memoryless
allocation or memory savings. Historical FL9_1 devices remain unverified.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_sparse_vertex_ -- --nocapture --test-threads=1` checks direct sparse streams,
slot 15 and rejection at 16, range boundaries, pipeline transitions, a queued
GPU copy and resource retention. It binds one buffer in two slots with distinct
byte regions. Separate overlap tests bind overlapping physical bytes through
distinct native slots on the tested RTX; historical FL9_1 devices remain
unverified. The strip test also checks per-slot rebasing across a 65,536-vertex
split, but its opaque, unculled output does not independently establish parity
or instance order.

With strict hardware mode enabled, the signed-normalized vertex checks are:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 snorm_vertex -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_formats_map_signed_normalized_words_without_expanding_other_formats -- --nocapture
```

The strict RTX 3090 Ti test requested and selected FL9_1, observed format
support masks `0x21122` and `0x2` for `Snorm16x2` and `Snorm16x4`, and checked
all 64 pixels from direct and indexed draws using both formats at a nonzero
buffer offset. The CPU test checks the native mappings. A controlled core
fixture also rendered 32 red and 32 green pixels using `Snorm16x2` UVs.
Microsoft lists both IA formats as required at FL9_1, but historical devices
have not been tested here. The private backend still requires four-byte
attribute alignment and does not thereby support SNORM texture formats.

The additional FL9_1-required integer IA mappings and their narrowly supported
shader conversion are checked with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_integer_vertex_formats_render -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_formats_map_required_and_optional_inputs -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 integer_guard_checks_selected_entry_and_level9_profile -- --nocapture --test-threads=1
```

The hardware test requests and asserts FL9_1, checks the driver's
`IA_VERTEX_BUFFER` bit for `R8G8B8A8_UINT`, `R16G16_SINT` and
`R16G16B16A16_SINT`, and verifies the capability projection includes them. A
single draw converts three input vectors directly to float, combines them into
yellow, and checks all 64 pixels. Integer arithmetic and other runtime integer
operations remain rejected by the shader guard. The static mapping and guard
tests are CPU-only.

The per-format mask's generic limit semantics and the D3D11 feature-level,
driver-query and backend intersections have CPU coverage:

```powershell
cargo test --offline -p wgpu-types --lib vertex_format_masks_are_subset_capabilities -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 feature_level_guarantees_are_specific_and_do_not_promote_backend -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_format_projection_intersects_level_driver_and_backend -- --nocapture --test-threads=1
```

The controlled core fixture obtains its mask from the selected FL9_1 device,
then replaces a baseline vertex format with unsupported `Unorm16x2` and checks
that core rejects pipeline creation before HAL. Run it in strict hardware mode
with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### Optional FL9_1 vertex input formats

The HAL maps the wgpu vertex formats with direct DXGI input equivalents except
`Float64` variants. Formats outside the FL9_1 required subset are admitted only
when the selected device's `CheckFormatSupport` query reports
`IA_VERTEX_BUFFER`; missing or failed queries are treated as unsupported. This
keeps the ten documented FL9_1 guarantees separate from optional driver
support. `Float16x2` and `Float16x4` become hardware guarantees only at FL9_3.
CPU tests cover every native mapping, descriptor size, and the synthetic
positive, absent-query and missing-backend cases. The public `Float16x2` test
renders a triangle and checks all 64 pixels if that format is reported.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_format_projection_intersects_level_driver_and_backend -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_formats_map_required_and_optional_inputs -- --nocapture --test-threads=1
cargo test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fl91_optional_float16_vertex_input -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested/actual FL9_1, the selected-device format mask
contains exactly the ten guaranteed formats and no optional inputs. The public
`Float16x2` test logs `SKIP`; this is not a successful GPU execution. The
private capability diagnostic prints the selected format mask and names. The
optional formats are not hardware-verified on this adapter; a driver-positive
format should receive a GPU readback case before broad claims are made about
legacy devices.

Vertex attribute offsets retain core's `min(format.size(), 4)` alignment and
the HAL passes the byte offset unchanged to the D3D11 input layout. The public
`public_dx11_byte_aligned_vertex_attribute_offset_at_fl91` test rejects a
misaligned offset of one and attempts a full readback render using
`Unorm8x2` at offset two. On the RTX 3090 Ti forced to FL9_1, no sub-four-byte
IA format is reported, so the positive case logs a capability skip after the
core rejection assertion. A suitable format-positive FL9_1 adapter is still
needed to verify native execution.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_byte_aligned_vertex_attribute_offset_at_fl91 -- --nocapture --test-threads=1
```

### Public FL9_1 sRGB texture formats

The public integration test checks the selected adapter's usage and filter
reports for `Rgba8UnormSrgb` and `Bgra8UnormSrgb`, uploads one sRGB texel to
each format, samples them into linear RGBA8 and verifies all 128 decoded
pixels. It then renders known linear values into each sRGB target and verifies
all 128 encoded readback pixels, including BGRA channel order and the transfer
curve. This exercises public texture creation, upload, sampling, render-target
view creation, GPU rendering and copy readback at explicitly selected FL9_1.
The companion mip test creates four-level POT textures in both formats and
uses implicit LOD sampling; mip 1 decoded to the expected linear value across
128 output pixels. Mip availability remains controlled by the device query
and the existing POT mip paths.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_srgb_texture_sampling_and_rendering -- --nocapture --test-threads=1
cargo test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_srgb_mip_chain_sampling_at_fl91 -- --nocapture --test-threads=1
```

The public copy test verifies DXGI typeless-family compatibility between
linear and sRGB RGBA8/BGRA8 formats. It copies a patterned texture from linear
to sRGB and back to linear, then reads both outputs through padded rows and
checks every texel byte. This establishes raw-bit preservation for these format
pairs; it does not claim color conversion during GPU copies.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_srgb_texture_copies_preserve_bits_at_fl91 -- --nocapture --test-threads=1
```

### Public FL9_1 D16 depth-only attachment

The D16 format is a feature-level 9_1 required depth attachment format, but the
backend still intersects the guarantee with the selected device's
`CheckFormatSupport` result. The public test verifies that the adapter reports
only render-attachment/transient usage and `SINGLE_MIP_ONLY`, then creates a
D16 texture through wgpu. Near and far full-screen triangles are submitted in
both draw orders with depth writes enabled; all 128 pixels retain the near
triangle's green output. The test uses a `DepthOnly` view and core rejects a
`StencilOnly` view, a two-mip D16 texture and a stencil pipeline before HAL.
Two additional two-pass cases verify that LOAD preserves the near depth value
and rejects a farther draw, while CLEAR resets depth and permits that draw;
the 128 pass-transition pixels are checked through readback. A third case
discards a written depth value and then loads it in a subsequent pass; the
core's initialization path prevents the stale value from changing the checked
output.

The RTX 3090 Ti at explicitly requested/actual FL9_1 reported D16 format mask
`0x210020`; the capability projection returned
`DEPTH_STENCIL_ATTACHMENT | SINGLE_MIP_ONLY`. This modern device confirms the
tested path, not every historical driver.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d16_depth_attachment_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu-hal --no-default-features --features dx11 d16_format_projection_requires_guarantee_driver_query_and_backend_path -- --nocapture
```

The first test executes public core → HAL → D3D11 → GPU rendering and readback.
The second is synthetic CPU-only coverage for unavailable or incomplete
queries and disabled guarantee/backend inputs.

### Public FL9_1 outputless fragment stage in a depth-only pass

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_outputless_fragment_depth_only_pass_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 fl91_outputless_discard_fragment_compiles_with_fxc -- --nocapture
```

The public test uses a physical adapter explicitly requested at FL9_1. A
fragment helper discards the left half of an 8×8 triangle in a DSV-only pass;
a later farther draw turns those pixels red while the depth-written right half
remains blue. A second targetless pass uses an ordinary location-0 color result
with no matching color target; the result is ignored, depth is written across
the target, and a later farther draw remains blue at all 64 pixels. All 128
pixels across both images are checked after GPU readback. The compiler test
records the direct FXC X4530 diagnostic for an outputless `void` shader, then
compiles the backend's synthetic-color wrapper while retaining a sparse
uniform binding and helper call. No RTV is bound for the targetless pass, so
the synthetic color cannot reach an application attachment.

The D24S8 variant verifies both depth and stencil side effects. The first pass
discards the left half and replaces stencil with reference 7 on the surviving
half. A later stencil-equality pass renders red only on the discarded half;
a separate depth comparison renders green there. The surviving half remains
blue in both results. All 128 readback pixels are validated on the strict
physical FL9_1 path.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_outputless_fragment_depth_stencil_pass_at_fl91 -- --nocapture --test-threads=1
```

### Public FL9_1 D24S8 depth/stencil pass transitions

The public transition test initializes both D24S8 aspects, then verifies a
second pass with LOAD preserves the depth and stencil values together. It
separately clears stencil while loading depth, and clears depth while loading
stencil, with comparisons making stale values produce the opposite color. Two
more cases discard only stencil or only depth and then LOAD both aspects:
stale values would draw green, while core initialization of the discarded
aspect leaves the blue clear. The readback checks all 320 pixels across the
LOAD and four CLEAR/DISCARD outcomes. A sampled D24S8 texture request is also
expected to fail public validation. The existing D16 test separately rejects
a `StencilOnly` view, which is not a supported D16 aspect.

On the RTX 3090 Ti forced to FL9_1, the public test passed with all pixels
matching. The depth-discard probe confirms the initialized value behaves as
zero: an incoming 0.25 fragment passes `Less` against the stale 0.5 value but
fails after discard initialization. This is evidence for the selected core
initialization path and driver, not a claim about native contents after discard.
The selected adapter reports D24S8 as render-attachment/transient only and
single-mip; core also rejects `DepthOnly` and `StencilOnly` D24S8 views because
the projection does not expose those aspect-specific formats.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d24s8_load_clear_discard_transitions_at_fl91 -- --nocapture --test-threads=1
```

The test executes public core → HAL → D3D11 → GPU rendering and readback.

### Exact FL9_1 capability projection through core

The strict fixture includes a test-only adapter constructed from the selected
physical D3D11 device and its unmodified FL9_1 capability projection. It
requests a core device, creates all projected RGBA8/BGRA8 usage combinations
and views, and checks the additional projected sampled and depth formats. The
test does not add the fixture adapter to ordinary HAL enumeration or enable a
public D3D11 adapter.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_unmodified_projection_creates_a_non_enumerated_core_device_on_hardware -- --nocapture --test-threads=1
```

The fixture also performs its existing GPU rendering and readbacks; the
unsupported-format case itself is CPU-side validation, not a GPU execution.

### FL9_1 core submission device-loss propagation

The controlled fixture can append a test-only native command that returns
`DXGI_ERROR_DEVICE_REMOVED` during command replay. Submission runs through the
HAL fence path and core queue handling on a physical adapter explicitly opened
at FL9_1. The test checks `DeviceError::Lost`, one `DeviceLostReason::Unknown`
callback, rejection of later submissions and queue cleanup. The error is
synthetic: it exercises classification and core propagation without removing
the GPU or establishing recovery behavior.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_submission_device_loss_is_propagated_once -- --nocapture --test-threads=1
```

The event-query retention test injects one `E_FAIL` from `GetData`, verifies
that the signal remains pending, and then waits for the real GPU event to
complete. It does not simulate device removal:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib event_poll_error_retains_pending_query -- --nocapture --test-threads=1
```

Core polling also checks `GetDeviceRemovedReason` after fence maintenance. The
test-only removed-reason injection verifies that a lost callback can reenter
`device_poll` while the original poll is unwinding, and that subsequent queue
submission is rejected. It runs on an explicitly selected physical FL9_1
device, but the removed-device result is synthetic:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_removed_reason_callback_can_reenter_device_poll -- --nocapture --test-threads=1
```

Queue destruction also waits for idle. The controlled fixture injects a
synthetic removed-device result from that wait and verifies the core loss
callback can poll reentrantly and that queue destruction does not panic. The
test uses a physical adapter opened at FL9_1; it does not simulate physical
removal:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_queue_drop_wait_device_loss_is_reported_without_panicking -- --nocapture --test-threads=1
```

`primitive_index` requires `Features::PRIMITIVE_INDEX`. The FL9_1 projection
does not expose optional features; this public test verifies the feature set
and that a device request requiring it is rejected before creation:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_unavailable_primitive_index_feature -- --nocapture --test-threads=1
```

With strict hardware mode enabled, run the zero-stride tests with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 zero_stride -- --nocapture --test-threads=1
```

The filter passed one CPU case and one physical RTX 3090 Ti case at requested
and selected FL9_1. The GPU case checked five full-image executions with the
constant stream first or last, covering direct and strip draws, Uint16 and
Uint32 indexed windows and gather, a constant-resource update, and rejection of
a 19-byte range where the attribute needs 20 bytes. An all-zero-stream triangle
only established degenerate draw completion. Negative effective-index behavior
is separately verified by `fl91_indexed_points_order_instances_and_lifetime`:
Uint16 and Uint32 indexed point draws use negative base-adjusted indices while
all vertex-rate streams have zero stride, and the test checks the visible point
pixels. This follows WebGPU's vertex-fetch calculation, where a zero array
stride leaves the attribute offset unchanged. The controlled core fixture used
a synthetic per-stage buffer limit of two and checked 64 green pixels.
Historical FL9_1 drivers remain unverified.

The public wgpu → core → HAL path has a focused readback test for the same
case:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_zero_stride_negative_base_vertex_at_fl91 -- --nocapture --test-threads=1
```

It requests FL9_1 and draws two indexed points with base vertex `-1`, once
with Uint16 indices and once with Uint32 indices. A zero-stride vertex stream
provides the constant point position while an instance-rate stream offsets it;
the test validates all 64 pixels for each format. The physical RTX 3090 Ti
run passed both images at requested/actual FL9_1.

With strict hardware mode enabled, run the instance-stream checks with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 instance_stream -- --nocapture --test-threads=1
```

The filter passed three cases: two CPU arithmetic/range tests and one physical
RTX 3090 Ti test at requested and selected FL9_1. The GPU case checked four
full-image executions across direct, strip, Uint16 and Uint32 window/gather
draws, with sparse mixed-rate streams, nonzero `first_instance`, zero-stride
tint, GPU copies and retained resources. It also checked short-range rejection.
In the Uint32 case, a red gather overlaps a green window at pixel (4, 7), while
the green gather does not cover it. The expected green pixel therefore
distinguishes instance-outer ordering from segment-outer ordering, which would
leave it red.
The controlled core fixture's instance-rate case produced 32 red and 32 green
pixels. This earlier result predates the host-emulated `instance_index`
implementation, which is verified by the current strict public test. Indexed
lists and strips with only instance-rate/no vertex-rate inputs are covered
below. The strip path reads the current index buffer during replay and emits a
direct strip per restart run so it does not merge primitive runs. These results
do not establish general instancing or historical FL9_1-driver behavior.

The no-vertex-rate indexed-list and strip cases are covered through both the
private HAL and public wgpu API:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_indexed_primitives_without_vertex_rate_inputs_match_nonindexed -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_indexed_primitives_without_vertex_rate_inputs_at_fl91 -- --nocapture --test-threads=1
```

The strict runs request and assert FL9_1 on the physical RTX 3090 Ti. They cover
LineList, TriangleList, LineStrip and TriangleStrip with Uint16 and Uint32
indices, using either instance-rate-only input or no vertex inputs. List draws
use nonzero first indices and negative base vertices. Strip draws use nonzero
first indices and signed base-vertex restart markers to create two runs. The
private CPU planner test checks run lengths and native-limit overlap; private
and public GPU readbacks compare each indexed result with its equivalent direct
draw. HAL/core still validate the bound index span before replay.

With strict hardware mode enabled, run the point-input execution test with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_point_list_vertex_inputs_and_pipeline_transition -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and selected FL9_1, the test checked three full
8×8 images. These cover sparse vertex slots, nonzero binding offsets and first
vertex, a point-to-triangle pipeline transition, and 65,537 points spanning
rebased batches with nonzero `first_instance` and an instance-rate stream. An
out-of-range draw is rejected during encoding. The controlled core `PointInput`
fixture checked all 64 pixels; its initial fixture buffer limit was adjusted from
one to two so it could create the intended layout. Before implementation the HAL
baseline failed because the pipeline rejected point inputs. Historical FL9_1
behavior has not been verified. Public surface capabilities remain unavailable.

With strict hardware mode enabled, run the indexed-point test and CPU planners:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 indexed_point -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti test requested and selected FL9_1 and checked 12 complete
8×8 images. Uint16 and Uint32 cases cover reordered and repeated indices, signed
base vertices, sparse vertex and instance streams, nonzero `first_instance`, a
recorded GPU index-buffer update, retained resources and whole-draw discard for
a late out-of-bounds fetch. Separate zero-stride and instance-only cases cover
extreme index values and negative effective indices without reading the index
contents; a short index range is rejected. The controlled core fixture checked
64 pixels each for Uint16 and Uint32. These cases exercise the private emulation,
not native indexed points, and do not establish historical-driver behavior.

With `$env:WGPU_DX11_REQUIRE_HARDWARE = "1"`, the focused sparse indexed test is:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_sparse_vertex_indexed -- --nocapture --test-threads=1
```

On the RTX 3090 Ti, all three focused tests passed with requested and selected
FL9_1. The high-Uint16 case also checks gather-to-native transition and additive
instances without rebinding vertex streams. The Uint16 test checks sparse slots 0 and 15, GPU-written indices, a
nonzero index binding offset, positive and negative base vertices, a nonzero
first index and two additive instances. A later out-of-bounds index discards
the complete draw; a subsequent valid draw and fence complete. The Uint32 test
uses GPU-written streams and indices to check ordered window and wide-triangle
gather rendering, then checks whole-draw discard after a late out-of-bounds
index. It verifies all 64 readback pixels in each target. These tests do not
establish historical-driver portability or general public indexed-draw support.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_direct_triangle_ -- --nocapture --test-threads=1` checks direct triangle
batch boundaries and order on CPU and validates pixels on both sides of a split
on physical hardware. Opaque output does not independently prove instance order.

With strict hardware mode enabled, run the overlap execution test and allocator
unit test with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 overlapping_vertex -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_slots_reserve_logical_streams_and_partition_overlaps -- --nocapture
```

On the RTX 3090 Ti forced to FL9_1, the GPU test checked all pixels for nine
executions: exact and partial overlaps across triangle lists, nonindexed strips
and indexed lists, a pipeline transition, and u32 window and wide-gather draws. The CPU test checks slot
allocation, logical-slot reservations and the selected-level distinction. The
controlled core fixture also rendered both disjoint and overlapping layouts,
each producing 32 red and 32 green pixels. These results do not establish
older-driver behavior, unsupported vertex formats or maximum-stride support.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
triangle_strip -- --nocapture --test-threads=1` checks the private nonindexed
strip path. The strict GPU test compares culling and winding with triangle lists,
exercises a split boundary, and checks a nonzero-offset pipeline transition.
The separate FXC diagnostic does not establish flat interpolation execution.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_checked_ -- --nocapture --test-threads=1` includes six original
runtime local-vector-index shaders. Each now requires an explicit fragment
linkage rejection and reports that GPU rendering was skipped. The separate
no-index multi-group control still renders and checks all 64 pixels. Earlier
runtime-index GPU output is historical and superseded by the guard. Matrix-store analysis tests use
`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
matrix_ -- --nocapture` and cover checked/unchecked stores, helper provenance,
shared type graphs and fail-closed work limits; this does not fix shared Naga.

With strict hardware mode enabled, run the triangle depth-bias test with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_triangle_depth_bias_and_pipeline_transition -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and selected FL9_1, five full 8×8 D24S8
render/readback cases checked positive, negative and zero constant bias,
depth-write control and pipeline transitions. Two more sloped-triangle cases
checked positive slope bias preserving the earlier red depth and negative slope
bias allowing the later green depth, across all 64 pixels each. Point and solid
line bias, nonzero clamp and selected `frag_depth` outputs with nonzero bias
were rejected. Two controlled core constant-bias cases each checked 64 pixels.
With zero bias, FXC rejected a direct-only fragment-depth output with X4530; a
structured color-and-depth output compiled. Selecting a separate color entry
from the same module succeeded with nonzero bias. These results do not establish
bias arithmetic precision on historical FL9 hardware, nor a public adapter
contract.

The direct-only `@builtin(frag_depth)` entry in that shader is separately
rejected at zero bias through
`DownlevelLimits::fragment_depth_without_color_target_0`; FXC requires
`SV_Target0` and reports X4530 otherwise. The controlled core fixture and a
strict public wgpu test verify that this error is returned before HAL, while
the same module's structured color-plus-depth entry successfully compiles.

The strict public `public_dx11_fragment_depth_controls_depth_test_at_fl91`
test draws red geometry with vertex depth 0.8 and fragment depth 0.2, then
green geometry with vertex depth 0.2 and fragment depth 0.8 under `LessEqual`.
All 64 readback pixels remain red, proving the structured `SV_Depth` output,
rather than the vertex depth, controls the depth test at requested/actual
FL9_1.

`@builtin(sample_mask)` input and output are separate profile boundaries. Naga
emits HLSL `SV_Coverage`; Microsoft documents pixel-shader input as requiring
`ps_5_0` and output as requiring `ps_4_1`, while this backend compiles
`ps_4_0_level_9_1`. HAL compiler tests capture the FXC failure for each
direction and compile a color-only sibling. Core has separate input and output
downlevel limits and rejects only the selected fragment entry before HAL.
These checks validate the profile boundary, not a GPU draw.

The CPU-only selected-entry analysis test is:

```powershell
cargo +1.93 test --offline -j16 -p wgpu-core --lib fragment_sample_mask_input_follows_selected_entry -- --nocapture
```

The FL9_1 FXC profile diagnostic test uses a physical adapter and can be made
mandatory with `WGPU_DX11_REQUIRE_HARDWARE=1`:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_profile_rejects_fragment_sample_mask_input -- --nocapture --test-threads=1
```

The strict public test verifies both unsupported entry points fail as core
validation rather than reaching FXC, while a color-only sibling pipeline is
accepted:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_fragment_sample_mask_io_at_fl91 -- --nocapture --test-threads=1
```

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
interpolation_modifiers_require_shader_model_4 -- --nocapture` checks
`flat`, `linear`, and `centroid` on both level-9_1 stages, with higher-profile
controls. The vertex profile compiles all three. The pixel profile rejects
`flat` (`nointerpolation`) and `linear` (`noperspective`) with FXC X4568 but
accepts `centroid`. The strict public test rejects selected fragment entries
using `flat` or `linear` before HAL and renders both centroid and perspective
pipelines, comparing all 64 readback pixels. `@interpolate(sample)` is rejected
at shader-module validation by the existing `MULTISAMPLED_SHADING` capability
mapping; its strict public regression confirms that the perspective sibling is
accepted.

The controlled core fixture reports
`DownlevelLimits::depth_bias_with_fragment_depth = false`. It verifies that a
selected `frag_depth` entry plus nonzero bias returns
`UnsupportedDepthBiasWithFragmentDepth` before HAL, that another selected color
entry in the same module still creates with bias, and that the depth-writing
entry still creates with zero bias. The latter two cases also exercise D3D11
pipeline compilation at forced FL9_1. This is validation coverage, not a
depth-writing GPU draw.

Run the CPU-only capability mapping test across all seven recognized feature
levels with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 feature_level_guarantees_are_specific_and_do_not_promote_backend -- --nocapture --test-threads=1
```

It distinguishes the FL9.x lack of `DepthBiasClamp` from the constant and slope
bias hardware guarantees, then separately asserts the private backend's
D24S8-triangle implementation and its clamp/`frag_depth` restrictions. This
does not claim execution support for FL9_2 through FL11_1.

`cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11
fl91_read_only_depth_preserves_depth_and_rejects_writes -- --nocapture` checks
the private read-only D24S8 path. It validates depth preservation, rejects
depth-writing, stencil-writing and depth-Never-remapped stencil-write pipeline
transitions plus invalid pass operations, then exercises READ→WRITE→READ
across submissions after caller resource destruction. This
does not establish sampled-depth alias support or a public read-only flag.

The strict public D24S8 test also checks three core-valid read-only modes:
both aspects read-only, depth read-only with stencil writable, and stencil
read-only with depth writable. Subsequent Equal comparisons validate the
preserved or updated aspect through color output. Error scopes confirm core
rejects a depth-writing pipeline in a depth-read-only pass and a
stencil-writing pipeline in a stencil-read-only pass. This uses a normal DSV
without an SRV alias; the D3D11 projection does not report
`READ_ONLY_DEPTH_STENCIL`.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_d24s8_load_clear_discard_transitions_at_fl91 -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and actual FL9_1, all 576 pixels across the
nine pass snapshots matched. The test retains coverage for D24S8 LOAD, CLEAR,
DISCARD initialization and aspect-specific view rejection.

The same public test records a D24S8 prepass with `color_attachments: &[]`,
clears depth to 1, writes near depth without a fragment stage, then draws far
geometry in a color pass using `Less`. All 64 pixels remain blue because the
prepass changed the depth buffer; without that write, the far draw would pass
against the clear value. The strict focused and full public test runs passed at
requested and actual FL9_1.

For dynamic uniform offsets, run `cargo test -p wgpu-hal --lib
--no-default-features --features dx11 dynamic_uniform -- --nocapture` with
strict hardware mode enabled. These tests cover sampled and indexed draws,
offset changes, GPU-upload ordering and range rejection; they exercise the
private four-byte alignment, not a future public device's reported alignment.

The controlled core fixture also verifies two simultaneous dynamic uniforms
against the selected-device FL9_1 projection. Run
`cargo test -p wgpu-core --lib --no-default-features
--features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware
-- --nocapture --test-threads=1` with `WGPU_DX11_REQUIRE_HARDWARE=1`. Logical
binding 17 is vertex-visible and uses base/dynamic offsets 256/256; binding 23
is fragment-visible and uses 256/512. Both alter checked readback pixels. The
unmodified-projection test separately verifies that core rejects thirteen
dynamic uniform entries against the reported limit of twelve. The public test
`public_dx11_uses_twelve_stage_disjoint_dynamic_uniforms_at_fl91` executes the
full six-vertex/six-fragment split using sparse binding numbers and distinct
dynamic offsets. Two scissored draws select different ranges and validate the
complete 8×8 readback; it also checks that thirteen dynamic entries fail core
validation.

The same strict core fixture includes a two-group case: a fragment uniform at
group 0/binding 17 tints a sampled RGBA8 texture and filtering sampler at
group 1/bindings 7 and 42. The draw checks all 64 output pixels, proving that
the selected core pipeline layout, shader resource map and runtime HAL group
indices agree for this arrangement. This test does not claim all eight private
groups or all combinations of aggregate group limits are supported through a
public adapter.

The private HWND path can be checked with a hidden desktop window using
`cargo test -p wgpu-hal --lib --no-default-features --features dx11
fl91_hidden_hwnd_surface -- --nocapture`. With strict hardware mode enabled,
the test reports its physical adapter and actual feature level, reads back a
HAL clear, presents, resizes and recreates the swapchain. The additional strict
clone-sampling case can be selected with
`fl91_hidden_hwnd_clone_sampling_presents_and_resizes`. On the RTX 3090 Ti at
requested and actual FL9_1 it read BGRA `[0, 0, 255, 255]`, present returned
`Ok`, and resize succeeded while the completed command buffer remained retained.
A hidden-window test does not verify visible public presentation or concurrent
surface use. The controlled core fixture holds the same presentation guard and
verifies that queue writes and submissions return validation errors, polling
times out, and buffer/texture creation is rejected on same-thread reentry.
This simulates the active guard; it does not force DXGI to deliver a window
callback during `Present`.

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
The same test checks positive and negative base vertices with nonzero binding
offsets and a preceding GPU index upload; all 64 pixels must be red. A late
out-of-bounds index discards a whole draw; a later GPU index update and draw
still complete, producing black on the left and red on the right.
It also checks Uint32 conversion, including raw values 65535–65537 with base
vertex -3, high effective indices via rebased vertex offsets, ordered windows,
GPU gathering for a triangle spanning over 65534, and a GPU rewrite following
a discarded draw. A later out-of-bounds index discards the entire logical draw.

The strict depth-render test also checks zero, one and two direct instances and
an overlapping blend case across the native primitive-count boundary. The
expected 64-pixel outputs distinguish instance order from batch order.
The indexed test also covers u16 chunk and u32 window ordering across instances,
including nonzero first instance and whole-draw out-of-bounds handling. It
checks GPU-written u16 values that require gather or window translation and
verifies that a later native draw restores its vertex binding.

`empty_vertex_input_pipeline_is_accepted` checks zero-input triangle and
nonindexed point pipelines, including a single interior point and transitions
between point and triangle pipelines. CPU tests verify point batching at the
65,535-primitive boundary. Indexed point emulation is covered separately above.
It also checks masked alpha, inversion and vignette blends against readback.

The public strict path exercises batching beyond that limit on actual hardware:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_splits_point_and_triangle_draws_at_fl91_limits -- --nocapture --test-threads=1
```

The test draws 65,536 points and 65,538 triangle-list vertices at forced
FL9_1. For each topology, the only visible primitive is in the second native
batch, proving point rebasing and whole-triangle splitting through public core
submission and readback.
`fl91_premultiplied_blend_full_channels_and_transition` checked six complete
images at forced FL9_1: premultiplied blending and two opaque transitions for
each of linear RGBA8 and BGRA8. The exact supported premultiplied state uses
coupled `ONE` / `INV_SRC_ALPHA` add factors for both color and alpha, across all
write masks. A controlled core fixture checked 64 premultiplied-blend pixels
with a one-unit per-channel rounding tolerance; an observed left pixel was
`[128, 0, 63, 191]`. Its unsafe adapter fixture reports blendability only after
a positive driver format query, and cannot express the backend's restricted
blend-state subset. These tests do not establish behavior on historical FL9_1
drivers or enable public surface capabilities. The public sRGB format test
below verifies the distinct transfer function and channel order; it does not
validate every blend equation on sRGB targets.
Microsoft's 10Level9 capability table marks Separate Alpha Blend unavailable
at FL9_1. There is no selected-device query that promotes this guarantee, so
the production path rejects differing RGB/alpha equations when both are
written. Some older full-channel blend tests explicitly opt into a test-only
RTX diagnostic and do not establish portability or change public capability
reporting.

Custom single-target blend factors and operations are translated independently
of those preset tests. CPU tests verify every ordinary factor's RGB/alpha
mapping and rejection of `SrcAlphaSaturated` and dual-source factors. The strict
`fl91_custom_constant_blend_and_updates_match_readback` test uses constant and
inverse-constant factors with two blend-constant values, then exercises Add,
Subtract, ReverseSubtract, Min and Max in the same render pass; all 64 pixels
are checked. The controlled core fixture also renders a custom constant blend
and checks all 64 pixels through core command recording. These tests ran on an
RTX 3090 Ti explicitly requested and selected at FL9_1; historical drivers and
every nonconstant factor's visual output remain unverified.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_custom -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

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
does not by itself prove leak freedom; the opt-in debug-layer lifetime report
below checks the focused indexed-render path.

### Recorded FL9_1 run

On 2026-11-02, Windows 11 Pro build 26300, Rust/Cargo 1.93.1 and 1.99.1,
and NVIDIA driver 32.0.16.2101, the focused test selected the NVIDIA GeForce
RTX 3090 Ti (PCI vendor `0x10de`, device `0x2203`, software `false`). The only requested
and returned feature level was 9_1. The ordered buffer copies, indexed draws,
fence wait and readback executed: all 64 pixels were blue before submission
and red after completion. The focused result was 1 executed, 1 passed, 0
failed, 0 skipped. The Microsoft Basic Render Driver was enumerated as software
and was not selected. No D3D11 debug layer was enabled.

### Opt-in D3D11 debug-layer diagnostics

Run the same end-to-end indexed rendering and GPU readback test with the
Windows D3D11 debug layer enabled:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
$env:WGPU_DX11_DEBUG_LAYER = '1'
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 fl9_1_hardware_indexed_triangle -- --nocapture --test-threads=1
```

The debug flag is explicit and has no fallback: if Windows Graphics Tools is
missing, device creation fails with the native error. The test clears device
creation diagnostics, runs the indexed draw/readback sequence, fails on debug
layer error or corruption messages, clears context state, and emits a detailed
live-object report. The expected device and immediate context remain alive
while the report is collected; any other live D3D11 object fails the test.
This checks the focused private HAL path, not every public API operation.

Before the change, the DX11 unit-test command below reported 64 passed and 0
failed. Those tests did not report their hardware execution individually; the
native diagnostic separately confirmed that the RTX could create FL9_1. A
successful suite total therefore should not be interpreted as 64 proven GPU
tests. CPU-only cases include feature-level ordering, queue error mapping
(including `DXGI_ERROR_DRIVER_INTERNAL_ERROR`) and the no-adapter guard; other
cases exercise native GPU paths when an adapter is available. The same suite
reported 64 passed and 0 failed after the change.
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
UWP devices. CoreWindow creation now checks `Dispatcher.HasThreadAccess` and
surface operations enforce the thread captured at construction; current CPU
and ARM32-UWP compile checks do not verify UWP runtime behavior. A compile or
package is not a runtime result.
Before upstreaming, move the hidden diagnostic-only surface helpers out of the
public HAL surface API or replace them with a test-only seam; keep the normal
backend free of application-specific fixtures and window-host assumptions.

## 2. wgpu-core and public API integration

The opt-in `dx11` HAL instance now enumerates only physical adapters that
successfully create an explicitly requested FL9_1 device and satisfy the
implemented capability projection. The public integration tests use ordinary
wgpu adapter and device creation, while the controlled core fixture exercises
core device/queue initialization, layouts, uploads, rendering, readback and
validation against the selected physical device. Strict mode fails if hardware
is unavailable; it does not substitute WARP. These tests cover a substantial
subset but do not establish that every operation allowed by the projection is
safe or that the backend is WebGPU-compliant.

The public adapter now exposes a constrained HWND surface path. Queue work and
selected context-backed resource operations serialize against `Present`;
same-thread reentrant mutations are rejected, and configure/acquire/present
require the window's owner thread. A hidden-window lifecycle test and bounded
visible example have run at forced FL9_1. The visible example's client-area
capture has been inspected; deterministic pixel validation continues through
offscreen readback tests. Continue testing resize, error and reentrancy edges;
historical FL9_1 surface behavior remains outstanding. This does not make the
adapter WebGPU-compliant.

## 3. Upstream wgpu GPU tests

The opt-in adapter and direct strict integration tests exist, but the public
contract and test-runner coverage remain incomplete. Run focused upstream GPU
selectors that are supported by the projection, verify the selected backend
and adapter, and record skips separately:

```console
cargo xtask test --test wgpu-gpu
```

Use the GPU-test harness's backend selection (for example `WGPU_BACKEND=dx11`)
and verify that its adapter report actually selects this fork's DX11 adapter.
Record passed, failed, skipped and unrun tests separately. A zero-adapter run
or a skipped test is not a pass. Keep other backend builds and tests working.

## 4. WebGPU CTS

The Deno CTS runner still needs an explicit DX11 feature path and adapter
selection verification. After that wiring is complete, start with selected
CTS cases and then run the pinned full suite:

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

Core has a separate command-encoding gate because public wgpu validation must
not rely on the private HAL draw error. Run the controlled fixture with strict
hardware required:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

This test forces FL9_1 on the physical RTX 3090 Ti and logs the actual positive
full-NPOT driver query. Its test-only core adapter reports the feature-level
repeat limit of 128, so both Repeat and MirrorRepeat sampler creation must fail
with `TextureRepeatLimitExceeded`. The GPU case samples POT and NPOT textures
with clamp-to-edge and checks all 64 output pixels. Core's selected-entry
conditional-NPOT relationship logic is covered separately by the synthetic
`npot_repeat_validation_uses_the_bound_relationship` unit test, since a
faithful finite repeat limit rejects those samplers earlier. Private HAL
hardware tests exercise pair-aware NPOT/wrap behavior. The fixture does not
expose an ordinary D3D11 adapter.

### FL9_1 sampler translation and patterned sampling

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampler_translation -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 equal_isotropic_filters_normalize_lod_only_for_single_mip_views -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampler_state_ -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_single_mip_same_filter_finite_lod -- --nocapture --test-threads=1
```

The first two commands are CPU-only. The sampler-state test executes patterned texture readback
for minification, magnification, mixed filters, addressing and a sampler
transition. It also runs a native border-creation diagnostic; that diagnostic
is not a rendered-pixel test. Anisotropy 2 is sampled, but a single-mip image
does not establish visual anisotropic quality.

The same-filter single-mip LOD test passed on the RTX 3090 Ti at requested and
selected FL9_1. It checked 32 full patterned images for nearest and linear
min/mag filters, minification and magnification, comparing ranges `0..0`,
`0.5..0.5` and `1..4` with an unrestricted reference, using both nearest and
linear mipmap filtering. The controlled core fixture also matched all 64 pixels for a nearest
sampler with `0.5..0.5`. Mixed-filter and anisotropic ranges that cannot be
faithfully translated remain rejected. Mipmapped sampling checks the actual view
against the sampler translation; explicit-LOD sampling and historical FL9_1 drivers
remain unverified.

With strict hardware mode, the following filter runs CPU analysis and physical
RTX 3090 Ti FL9_1 tests. A bounded-bias mip test passed nine executions with
64 checked pixels each, covering literal biases -3, -1, -0.5, 0, 0.5, 1 and 3,
varying derivatives, a minimum-LOD clamp and a fractional linear-mipmap blend.
Negative linkage tests cover
dynamic, out-of-range, helper-argument and offset biases; draw validation
rejects incompatible anisotropic or mixed-filter sampling pairs for both
direct and indexed draws.

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 bias -- --nocapture --test-threads=1
```

The tested FXC emits an unclamped `texldb`; Microsoft's [instruction
reference](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/texldb---ps)
defines ps_2_0 bias values outside [-3, 3] as undefined. The backend currently
uses FL9_1 shader profiles even on higher-level devices. It accepts finite
literal biases in [-3, 3] without an offset, while requiring equal min/mag
filters and anisotropy 1 for every sampling pair used by the selected entry.
View and LOD compatibility checks still apply; general biased sampling and
historical-driver behavior remain unverified.

The constant offset accepted by WGSL is the final argument of
`textureSampleBias`. Its selected-entry core limit and private HAL guard reject
direct and helper-reached uses at FL9_1. The Naga-to-FXC profile test reproduces
X4532 under both FL9_1 and FL9_3 and compiles an SM5 control:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib level_9_profiles_reject_texture_sample_bias_offsets -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,std,wgsl --lib texture_sample_bias_limits_follow_selected_entry_and_helper_calls
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_sampled_mip_chain_at_fl91 -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested/actual FL9_1, the public test rejected both
direct and helper-reached offset pipelines before HAL; the sibling implicit
sample and supported literal-bias rendering still matched all 256 checked
pixels.

### FL9_1 texture transfers

With strict hardware mode enabled, `cargo test --offline -p wgpu-hal --lib
--no-default-features --features dx11 bgra -- --nocapture` checks BGRA raw
upload/copy/readback, sampling channel order, and render-to-partial-copy with
offsets, row padding and untouched initialized texels. A wrapped-backbuffer
unit test also matches this filter. The ordinary BGRA paths still require their
own positive driver format-support query.

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

The strict private `fl91_padded_partial_3d_upload_preserves_other_voxels`
case uploads a 2×2×2 RGBA8 region into a 4×4×4 volume at origin (1,1,1),
using a nonzero buffer offset, padded rows and padded image stride. It samples
each depth slice to a 2D target and checks all 64 pixels, including zeroed
voxels outside the update and output-buffer padding. It requires a physical
FL9_1 adapter and the selected device's optional 3D sampling and render-target
format support; unavailable format bits are logged as SKIP, not GPU PASS.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_padded_partial_3d_upload_preserves_other_voxels -- --nocapture --test-threads=1
```

The private `private_device_creates_restricted_texture_views` test also passed
on the RTX FL9_1 device with a metadata-only copy texture view and rejected
sampling, attachment use and invalid descriptors. The controlled core fixture
verified 12 upload/copy/readback pixels each for RGBA8 and BGRA8 copy-only views
and rejected incompatible view usages. Neither result establishes historical
driver behavior or general public adapter support.

### FL9_1-required sampled/upload formats

Microsoft lists R8_UNORM (legacy L8), R8G8_SNORM (legacy V8U8),
R8G8B8A8_SNORM (legacy Q8W8V8U8) and BC1/BC2/BC3_UNORM (legacy DXT1/DXT3/DXT5) as FL9_1 2D
shader-sample formats. `R16G16_SNORM` has a FL9_1 2D-texture guarantee, but
its required `SHADER_SAMPLE` support begins at FL9_2. D3D11 permits
point-filtered sampling when a selected device reports `TEXTURE2D` without
`SHADER_SAMPLE`; the backend reports that format as non-filterable. A failed
query or missing `TEXTURE2D` bit is rejected. R8_UNORM, R8G8_SNORM and
R8G8B8A8_SNORM permit sampled/upload mip chains only when the selected device
reports `TEXTURE2D | MIP` and the backend path is enabled. NPOT chains also
require positive unrestricted-NPOT support. `SHADER_SAMPLE` independently
controls filterability. The other listed sampled/upload formats remain
single-mip. BC1/BC2/BC3 dimensions and updates
must be aligned to complete 4×4 blocks, and source offsets/row pitches align to
the format block byte size.
Run the synthetic capability checks and strict hardware tests with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_unorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 rg8_snorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 rgba8_snorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_texture_tests::fl91_rg16_snorm_is_query_gated_and_samples_when_supported -- --nocapture --test-threads=1
cargo +1.93 test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_texture_tests::fl91_bc -- --nocapture --test-threads=1
```

The CPU tests cover legal and illegal usages, shape limits and missing or
insufficient driver-query results. The R8, R8G8 and RGBA8 SNORM hardware tests
request and assert FL9_1, upload padded 5×4 patterns from nonzero source offsets, sample into a
10×8 RGBA8 target and compare every pixel. R8_UNORM uses offset 1 and 7-byte
rows, R8G8_SNORM offset 2 and 12-byte rows, and R8G8B8A8_SNORM offset 4 and
24-byte rows. SNORM tests remap signed channels to UNORM and allow one code
value for conversion rounding. On the RTX 3090 Ti, R8_UNORM, R8G8_SNORM and
R8G8B8A8_SNORM returned masks `0x0002d3e0`, `0x00021320` and `0x000213a0`;
all reported filtered-sampling support, while the tests use nearest filtering.
At forced FL9_1, `R16G16_SNORM` returned `0x00021122`, which lacks
`SHADER_SAMPLE` but includes `TEXTURE2D`. The strict private test uploaded
padded R16G16_SNORM texels, point-sampled them into an RGBA8 target and matched
all 80 pixels. The public adapter reports `TEXTURE_FORMAT_16BIT_NORM` and
non-filterable sampled/upload usages; a filterable bind-group layout fails
core validation, while the non-filtering sampler path matched all 64 output
pixels. The test therefore validates both the optional native behavior and
the rejection boundary. This forced modern driver does not establish behavior
on historical FL9_1 hardware.
The BC1 test uploads an 8×8 block-compressed image from offset 8 with a 24-byte
block-row pitch. BC2/BC3 use offset 16 and a 48-byte row pitch because their
blocks are 16 bytes. Each test replaces the block at [4,0] using an aligned
32-byte pitch, copies the result to a second sampled texture, and checks all
256 output pixels. The forced-FL9_1 RTX reports `0x000213a0` and filtered
sampling for all three formats.
The native-only `TEXTURE_COMPRESSION_BC1_BC2_BC3_UNORM` and
`TEXTURE_COMPRESSION_BC1_BC2_BC3_SRGB` features are independently available
only when every format in the respective family passes its selected-device
sampled-upload query and backend check. A strict public wgpu test requested and
confirmed FL9_1, uploaded BC1 red, BC2 green and BC3 blue unorm blocks, copied
each texture on the RTX 3090 Ti, sampled all three in one fragment shader and
verified 64 white pixels. It then uploaded matching mid-gray sRGB blocks,
copied and sampled them, verifying 64 pixels near `[59, 57, 59, 255]`; raw
unorm sampling would be around `[132, 130, 132]`. The same device did not
advertise the full BC feature, and the test confirmed BC4 and BC3 3D texture
descriptors are rejected. BC texture-to-buffer readback remains unavailable.

Run the public strict execution test with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_bc_subset -- --nocapture --test-threads=1
```
BC1/BC2/BC3 `COPY_SRC` is limited to GPU texture-to-texture copying; a controlled
texture-to-buffer attempt fails core validation because FL9_x cannot safely
copy an SRV-only BC texture to CPU-visible memory. R16G16_SNORM still does not
support render-target, copy-source or readback operations in this private
subset. RG8_SNORM remains sampled/upload-only because valid FL9_1 GPU copies
failed on the development driver; RGBA8_SNORM supports same-format GPU texture
copies only when the selected device reports `TEXTURE2D`. Their readback masks
remain clear and texture-to-buffer readback is still rejected. RGB5A1 and RGBA4 remain
non-renderable and lack texture-to-buffer readback, but support query-gated
same-format GPU copies. R8_UNORM,
R8G8_UNORM and B5G6R5 have a narrower query-gated single-mip render-target,
copy and readback path described below. RG8_UNORM linear filtering has visual
execution coverage; R8_UNORM and the SNORM formats have query-backed
filterability but only nearest-sampling readback coverage.

### Query-gated RGBA8_SNORM GPU texture copies

The backend permits a single-mip RGBA8_SNORM texture to be a same-format
texture-copy source only when the selected device positively reports
`TEXTURE2D` and the backend copy path is enabled. A synthetic capability test
checks that a missing query or disabled implementation keeps `COPY_SRC`
unavailable. The strict public test explicitly requests FL9_1, uploads signed
texels, copies the texture to a second RGBA8_SNORM resource, samples the copy
and checks all 64 output pixels after RGBA8 readback. It also verifies that
`texture_to_buffer_copy_format_mask` does not advertise SNORM CPU readback.
On the RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`), the selected
format query returned `0x000213a0` and all 64 pixels matched
`[255, 0, 255, 0]`. This confirms the selected-device path only; it does not
verify historical FL9_1 drivers.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu-hal --no-default-features --features dx11 --lib rgba8_snorm_texture_copy_source_requires_implementation_and_positive_query -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_snorm_copy public_dx11_rgba8_snorm_texture_copy_then_sample_at_fl91 -- --nocapture --test-threads=1
```

### RG8_SNORM GPU texture copy boundary

`CheckFormatSupport(TEXTURE2D)` is not sufficient evidence that the backend's
copy operation is safe. A strict forced-FL9_1 probe selected the RTX 3090 Ti
(`0x10de:0x2203`, driver `32.0.16.2101`) and exercised RG8_SNORM upload,
sampling, RGBA8 render and readback. Those operations verified all 16 output
pixels when copies were omitted. Submitting a valid full
`CopySubresourceRegion` removed the device with `E_INVALIDARG`; replacing it
diagnostically with `CopyResource` produced the same result. RG8_SNORM
`COPY_SRC` is therefore disabled in both the private validation model and
public capability projection. The test does not establish a universal FL9_1
limitation; other device and driver behavior remains unverified. A separate
Core fixture checks that a synthetically cleared texture-to-buffer format-mask
bit rejects readback before HAL.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93.1 test --offline -j2 -p wgpu-hal --lib --no-default-features --features dx11 rg8_snorm_texture_copy_source_stays_disabled_without_verified_backend_support -- --exact --nocapture
cargo +1.93.1 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std instance::dx11_fixture_tests::fl91_core_format_mask_rejects_texture_readback_before_hal -- --exact --nocapture --test-threads=1
```

Run the controlled core copy/readback boundary test with strict physical
hardware required:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### Query-gated R8, RG8 and RGB565 copy/readback

The 10Level9 texture transfer restriction requires the native texture to have
render-target support for the tested GPU-to-CPU readback path. Accordingly,
R8_UNORM, R8G8_UNORM and B5G6R5 expose `COPY_SRC` only when the selected
device's `CheckFormatSupport` result includes `RENDER_TARGET`, and the backend
path is enabled. RGB5A1 and RGBA4 expose `COPY_SRC` for same-format GPU
texture copies when the selected format query reports `TEXTURE2D` and their
backend implementation paths are enabled. They do not gain render-target bind
flags or texture-to-buffer readback.
Texture-to-texture copies remain on the GPU. Readback copies into a staging
texture, then uses the mapped native row pitch to pack texels into the caller's
buffer. The format block size supplies the bytes-per-texel calculation (1 byte
for R8 and 2 bytes for RG8/RGB565).

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_rg8_rgb565_copy_and_readback_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_r8_rg8_rgb565_render_attachments_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu-hal --lib --no-default-features --features dx11 narrow_optional_copy_sources_require_render_target_support_and_backend_paths -- --nocapture --test-threads=1
```

The physical test explicitly requests FL9_1 and excludes software adapters.
It uploads padded source rows, partially copies a 3×2 region from a 5×3
texture into a differently sized 6×4 texture at a nonzero origin, and reads
the full result into a buffer with offset 16 and 256-byte row pitch. Each
format checks 24 texels (six copied and eighteen zero-initialized/preserved)
plus the row-padding sentinel. On the RTX 3090 Ti, R8, RG8 and RGB565 all
passed. The synthetic HAL test proves that a missing `RENDER_TARGET` query or
disabled backend path suppresses `COPY_SRC` and rejects use. This observation
does not establish compatibility with historical FL9_1 drivers.

The rendered-attachment case uses `RESOURCE | COLOR_TARGET | COPY_SRC` for
R8_UNORM, R8G8_UNORM and B5G6R5. It renders a constant color, samples the
attachment into RGBA8, and copies the native attachment directly to a padded
readback buffer. All 64 native texels per format, the nonzero buffer prefix and
row padding are checked. The RTX 3090 Ti at forced FL9_1 passed all three
formats. The stored R8 and RGB565 bytes match their expected quantized values;
the RG8 green byte is allowed a one-code rounding tolerance relative to the
RGBA8 sampled value. This checks the combined usage path and rendered source
contents, in addition to copies from uploaded textures.

### Non-filtering sampler layouts

With strict hardware mode enabled:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_nonfiltering_sampler_layout_render_and_transitions -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sparse_sampler_filtering_layouts_are_sorted_and_distinct -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 filtering_class_uses_all_original_descriptor_filters -- --nocapture
```

On the RTX 3090 Ti at requested and selected FL9_1, two 8×8 readback images
checked nearest sampling through a sparse NonFiltering layout and a transition
to a Filtering layout using a nearest sampler and a different texture. Separate
min-only, mag-only and mipmap-only linear samplers were rejected by the
NonFiltering layout, as were comparison samplers. The controlled core case
checked 64 pixels and returned `WrongSamplerFiltering` for a linear sampler.
CPU tests check sampler-class identity, the non-filtering mask, logical binding
order and the original descriptor's filter class. The initial strict baseline
rejected the NonFiltering layout. The controlled core fixture also verifies
comparison-sampler creation returns the missing `COMPARISON_SAMPLERS` downlevel
flag before calling the D3D11 HAL when the optional query is unavailable. The
public D3D11 path now exposes only the query-gated FL9_1 `LessEqual` subset;
its strict readback test checks both output classes and rejects a different
compare function, unequal min/mag filters and anisotropy above one before HAL.
General comparison sampler support remains unavailable. Historical drivers
remain unverified, and public surface capabilities remain unavailable.

### Sampled-resource visibility

With strict hardware mode enabled:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_sampled_vertex_fragment_visibility_and_transition -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampled_visibility_preserves_sparse_slots_and_layout_identity -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 level_9_1_vertex_texture_guards_follow_selected_entry -- --nocapture
```

The strict RTX FL9_1 test checked two full 8×8 images with fragment sampling and
a transition between `VERTEX | FRAGMENT` and `FRAGMENT` sampled-resource layouts.
Actual sampling by the selected vertex entry was rejected before drawing. A
controlled core case checked 64 pixels. CPU tests cover per-binding stage
mapping, layout identity and selected-entry guards. The initial strict baseline
rejected the combined-visibility layout. These results do not establish vertex
shader resource support or historical-driver behavior; the adapter reports no
surface capabilities.

### Unfilterable-float layout bindings

With strict hardware mode enabled:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_unfilterable_texture_sampling_relationships -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 unfilterable_texture_layouts_and_sampling_pairs_fail_closed -- --nocapture
```

The strict RTX FL9_1 test checked two complete images using a supported RGBA8
texture declared as unfilterable float and a NonFiltering sampler. It also
sampled a second pair through helper functions and switched selected entries;
an incompatible unused cross-pair was permitted. A selected entry using a
Filtering sampler layout with the unfilterable binding failed with the exact
fragment linkage error, even though the actual sampler used nearest filtering.
The controlled core test checked 64 pixels and rejected its invalid filtering
pair. CPU tests cover cross-group relationships, helper and entry-point selection,
and layout identity. The pre-change strict baseline rejected bind-group layout
creation. This layout support adds no texture formats, and historical hardware
and the broader public adapter contract remain unverified.

### COPY_DST-only POT mip uploads

With strict hardware mode enabled:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_copy_dst_only_mips_upload_and_usage_boundaries -- --nocapture --test-threads=1
```

For RGBA8 and BGRA8 on the RTX 3090 Ti at requested and selected FL9_1, the
test uploaded three mip levels and checked an untouched zero-initialized mip.
After the upload fence, it used an explicit test-only native GPU-to-GPU copy
into a separate render-target-capable texture for readback. The original
`COPY_DST`-only texture rejected logical copy-source, readback, sampled-view and
render-target-view use. A controlled core case submitted three writes, waited
for completion and verified forbidden view/readback validation; no core
readback of the upload-only texture is claimed. Historical hardware remains
unverified and public surface capabilities remain unavailable.

### NPOT mip capability gate

```powershell
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std npot_mipmap_tests -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

Core requires `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES` when any spatial dimension
of a texture with multiple mips is not a power of two. D2 array-layer counts do
not participate because they are not mip dimensions. The strict controlled
fixture requests a 6×8 RGBA8 texture with two mips while the flag is absent and
verifies a `MissingDownlevelFlags` validation error before HAL. The device's
optional full-NPOT query is combined with the backend path and per-format MIP
support before the D3D11 projection sets the flag. Synthetic capability tests
verify false and unavailable queries remain conservative. A strict public
forced-FL9_1 test on the RTX 3090 Ti uploaded all four levels of a 6×10 RGBA8
texture using padded rows and command-buffer copies, then sampled mip 2 into an
8×8 target. All 64 pixels matched the expected blue mip color. The adapter
reported `FullNonPow2TextureSupport=true`; this does not simulate a restrictive
historical driver. The CPU core test still covers D1/D2/D3 dimensions and the
D2 array-layer case.

### Copy-only POT mip transfers

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_copy_only_mip_transfers_and_views -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_copy_only_mips_start_zeroed_and_have_metadata_views -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 metadata_texture_view_ranges_check_bounds_and_aspect -- --nocapture
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The same strict controlled fixture checks that `D2Array`, `Cube` and
`CubeArray` bind-group layouts are rejected with their missing downlevel flags,
and that a one-layer `D2Array` view returns a core validation error before the
HAL is called. This covers a dimension restriction that
`max_texture_array_layers = 1` cannot express by itself.
It also verifies that D1/D3 layouts are rejected when their corresponding
texture dimension limits are zero, and that storage-texture `D2Array` layouts
use the same array capability gate before format validation.

The first two tests execute GPU work; the third is CPU-only. Strict execution on
the RTX 3090 Ti at requested and actual FL9_1 verified RGBA8 and BGRA8 four-level
initialization, padded partial uploads, copies between mip levels of different
textures, readback, metadata views and resource retention. The core fixture
verified all 43 pixels in its RGBA8 mip chain. Each format requires a positive
`CheckFormatSupport` result including `D3D11_FORMAT_SUPPORT_MIP` and the other
required texture, sampling and render-target bits. Missing format support is a
SKIP in normal mode and a failure in strict mode for these GPU tests. The
initialization and transfer tests also skip failed format queries in normal mode
and fail in strict mode; the core fixture propagates query errors as failures in
either mode.

These tests do not exercise sampled or attachment mip views. Native FL9 shader-resource
views must include the smallest resource mip; NPOT mip chains require a positive
full-NPOT query and a supported per-format path. Same-subresource copies remain
unsupported. Historical FL9 GPU behavior is unverified.

### Sampled POT mip chains

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 sampled_mip -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict private test executed 74 GPU cases without skips on the RTX 3090 Ti
at requested and actual FL9_1. RGBA8 and BGRA8 readbacks cover selected mip
levels, nearest and linear mip filtering, fractional LOD and used versus unused
sampler relationships. For each format, a same-pass pipeline and bind-group
transition retained sampled textures, views, groups and samplers after the caller
destroyed them before submission, and readback matched 32 green and 32 blue
pixels. The controlled core fixture sampled the blue mip of a
2x1 texture and verified all 64 output pixels. Supported chains have one layer,
POT dimensions, the restricted sampled usage combinations and positive format
support. Native suffix SRVs include the resource's last mip. Previously
verified results include suffix views for both formats. The controlled core
`SampledMipSuffix` case produced 64 blue pixels, verifying view-relative LOD.
A separate pair of core cases used `TEXTURE_BINDING | COPY_DST` without
`COPY_SRC`: an uploaded mip produced 64 blue pixels and an untouched mip
produced 64 transparent-zero pixels. These results check sampling via a separate
render target; the SRV-only native texture cannot be directly read back.
A sampler whose minimum LOD was normalized requires
a single mip; when a finite maximum was raised, a used multi-mip view requires
the original maximum to reach the last mip and anisotropy 1. NPOT mip chains
require positive full-NPOT and per-format MIP support;
historical FL9 GPU behavior is unverified.

The same strict test samples zero-initialized POT chains with `RESOURCE` alone
and `RESOURCE | COPY_SRC` into 64 transparent pixels for each format. It also
reads back all 256 zero bytes from mip 0 of a `COPY_SRC`-only chain for RGBA8
and BGRA8.
The core fixture creates all three usage shapes through the projected adapter
path. CPU capability tests cover every nonempty usage subset of `RESOURCE`,
`COLOR_TARGET`, `COPY_SRC` and `COPY_DST`; the physical tests exercise the
previously rejected resource-only and source-only shapes alongside existing
upload, sampled, attachment and copy combinations.

### Query-gated R8G8_SNORM mip chains

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j4 -p wgpu-hal --no-default-features --features dx11 --lib rg8_snorm_mip_chains_require_feature_level_backend_and_driver_support -- --nocapture --test-threads=1
cargo +1.93 test --offline -j4 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rg8_snorm_mip_chain_at_fl91 -- --nocapture --test-threads=1
```

The CPU capability test covers the FL9_1 guarantee, backend path and selected
device `TEXTURE2D | MIP` query independently, including unavailable and
negative query results. It also verifies that NPOT dimensions still require a
positive unrestricted-NPOT result. The strict public test ran on the RTX 3090
Ti at requested and actual FL9_1 (driver 32.0.16.2101); it uploaded three
distinct signed RG levels and rendered mip 1 into a 4×4 RGBA8 target. All 16
pixels matched `[192, 64, 0, 255]` within one code value. The device reported
format support mask `0x00021320`. This path does not add render-target,
copy-source or CPU-readback support for R8G8_SNORM, and the RTX result does not
establish behavior on historical FL9_1 drivers.

### Query-gated R8G8B8A8_SNORM mip chains

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib rgba8_snorm_mip_chains_require_feature_level_backend_and_driver_support -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public snorm_mip_chain_at_fl91 -- --nocapture --test-threads=1
```

Microsoft's required-format table lists `TEXTURE2D`, `SHADER_SAMPLE` and
`MIP` for `R8G8B8A8_SNORM` beginning at FL9_1. The backend still requires the
selected device to report `TEXTURE2D | MIP` and its mip path to be enabled;
the format's `SHADER_SAMPLE` query independently determines filterability.
The strict public test uploads three padded signed levels, samples mip 1 with
a literal bias and compares all 16 pixels against `[192, 64, 160, 96]` within
one code value. The RTX 3090 Ti reported `0x000213a0` at requested and actual
FL9_1. Mip support does not add render-target, copy-source or CPU-readback
support, and the modern driver does not represent historical FL9_1 devices.

### Non-suffix sampled mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_nonsuffix_sampled_views_track_source_contents -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti run at requested and actual FL9_1 passed 16 GPU cases,
eight each for RGBA8 and BGRA8, with 64 checked pixels per case and no skips.
The cases exercise view-relative levels, high-LOD clamping to the selected range, two
simultaneous clones, native suffix and clone transitions, source updates after
group creation and between submissions, and retained resources after caller
destruction. A recorded buffer-to-texture upload followed by an indexed draw in
the same submission produced 64 blue pixels per format after the upload and
index buffers were destroyed before submission. A feedback test rejects sampling
an overlapping mip of the attachment. The controlled core fixture checked 64 yellow pixels after a
source update. The source needs no logical `COPY_SRC` usage.

Non-suffix sampled views use a GPU clone refreshed by copying the selected
source mips on each draw; the per-draw copies and additional allocation have a
performance cost. The original resource is retained for alias checks. Native
suffix views do not need this clone. Format prerequisites are queried; missing
support skips the applicable case in normal mode and fails strict mode.
Historical drivers remain unverified and public surface capabilities remain
unavailable.

The public wgpu test `public_dx11_sampled_mip_chain_at_fl91` additionally binds
a non-suffix RGBA8 2D view selecting source mips 1–2, clamps sampling to the
view's second mip, and verifies the original mip-2 color in all 64 output
pixels. Together with the four full-chain/bias draws, the strict public test
checks 320 pixels and exercises the public core-to-HAL clone refresh path.

### Individual color-attachment mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_to_individual_mips -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict private test passed for both RGBA8 and BGRA8 on the RTX 3090 Ti at
requested and selected FL9_1, checking the full mip chain. The controlled core
fixture checked all 43 pixels for each format and submitted a render-only mip
pass, which has no direct readback. This path requires POT dimensions and
`COLOR_TARGET` with optional `RESOURCE`, `COPY_SRC` and/or `COPY_DST`;
`TRANSIENT` is not supported in the same chain. Each attachment view selects one mip.
Historical hardware remains unverified and public surface capabilities remain
unavailable.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_and_sample_mip_transitions -- --nocapture --test-threads=1
```

The strict RTX FL9_1 run passed six GPU cases across RGBA8 and BGRA8, including
render/sample state transitions. The controlled core fixture passed four cases
with 64 checked pixels each: suffix sampled views and last-mip dual-use views
for both formats. Native sampled suffix views include the resource's last mip;
non-suffix sampled views use clones. Attachment views and dual-use views select
one mip. Use in distinct passes is supported. Same-pass use is supported for
disjoint native suffix and clone-backed SRVs and attachment mips. Historical
historical devices and the broader public adapter contract remain unresolved.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_dual_nonsuffix_mip_render_and_sample -- --nocapture --test-threads=1
```

The strict RTX FL9_1 test passed eight GPU cases across RGBA8 and BGRA8. A
nonfinal dual-use view has an SRV backed by a per-draw clone and an RTV backed
by the original mip. The controlled core fixture also used a default dual-usage
view to render and then sample a nonfinal mip in separate passes, checking 64
pixels for each format. Same-pass clone-backed views of the attachment resource
are supported when their mip ranges are disjoint; historical driver behavior
remains unverified.

### Same-texture distinct-mip copies

With strict hardware mode enabled:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_same_texture_disjoint_mip_copies -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_render_copy_same_texture_mip_then_sample_clone -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and selected FL9_1, RGBA8 and BGRA8 partial
copies between two mips passed in both directions with full padded readback,
including untouched texels. The separate render, same-texture copy and
clone-backed sample case produced 64 blue pixels for each format after the
source resources were destroyed by the caller before submission. The controlled
core fixture checked all mip texels for both formats and rejected a same-mip
copy. The pre-change strict baseline rejected a valid distinct-mip copy. Native
copies use an explicit source box between DEFAULT resources. Same-subresource
copies remain rejected; historical FL9 drivers remain unverified and public
public D3D11 surface capabilities remain unavailable.

### Same-resource disjoint native mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_native_suffix_disjoint_attachment_mips -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

Six strict GPU cases passed on the RTX 3090 Ti at requested and selected FL9_1
across RGBA8 and BGRA8. Controlled core cases sampled a disjoint suffix while
rendering to mip 0 of the same texture and verified 64 pixels per format.
Overlapping full-range SRVs are rejected. Disjoint clone-backed views are
covered by the separate test below.
[D3D11.3 §5.3.1](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)
permits input and output views of disjoint subresources. The separate
[D3D9 sampler documentation](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-registers-sampler)
warns against simultaneously binding a texture for sampling and rendering;
it does not prove D3D11 behavior on historical FL9 drivers. Those drivers remain
unverified, and public D3D11 surface capabilities remain unavailable.

### Same-resource disjoint clone-backed mip views

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_clone_disjoint_mips_preserve_depth -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti run at requested and selected FL9_1 passed two GPU cases,
one each for RGBA8 and BGRA8. Each checked 32 green and 32 blue pixels after
two same-pass draws with different clone-backed mip views and a depth test. The
controlled core fixture verified the same output for both formats. The tests
reject an overlapping mip and retain resources through submission.

Before each clone refresh, replay snapshots and unbinds the current RTV and DSV,
clears pixel shader resource slots, copies the selected mips, restores the
attachments and rebinds the pixel shader resources. This adds attachment
transitions to the existing per-draw copy cost. Historical FL9 drivers remain
unverified; public surface capabilities remain unavailable.

### FL9_1 uniform bindings

```powershell
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 sparse_uniform_allocation -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 level9_constant_budgets_and_multiple_buffers_compile -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std uniform_constant_vectors_use_selected_entry_point_resources -- --nocapture
cargo +1.93 test --offline -j16 -p wgpu-types --features serde shader_uniform_vector_limits_are_numeric_downlevel_limits -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_enforces_fl91_aggregate_uniform_constants -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_uses_255_vertex_uniform_vectors_with_post_encoding_queue_write_at_fl91 -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_constant_profile_boundary_creates_on_hardware -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_c254_and_pixel_c31_change_rendered_output -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_sparse_uniform_ranges -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_two_uniforms_with_sampled_textures -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_multiple_groups_keep_dynamic_ranges_and_layouts_independent -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_forked_uniform_bindings_keep_dynamic_ranges_independent -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 hal_uniform -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_short_uniform_dynamic_copy_and_odd_source_end -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 integer_uniform -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The allocation, Naga and FXC checks above are CPU/compiler tests. The strict
public aggregate-budget case reads all 32 fragment vectors from two sparse
uniform bindings and verifies 64 pixels; core validation rejects a 33-vector
pipeline before HAL. The profile-boundary hardware diagnostic asks FXC to
compile a vertex shader with a dynamically indexed 255-element
constant array, then creates that shader on the explicitly forced-FL9_1 device.
Its companion compiler test confirms 256 and 257 elements fail at register
index 256. The diagnostic does not draw the shader. A separate c254/c31 test
compiles WGSL arrays through Naga and FXC, then draws with zero and nonzero
values in the final vertex and pixel constants. On the forced-FL9_1 RTX 3090 Ti,
readback distinguished 17 baseline green pixels from 4 shifted red pixels.
This verifies vertex c254 geometry and pixel c31 color changes; runtime
integer-indexed arrays remain rejected. The public projection allows a
4,080-byte (255-vector) uniform binding so the vertex stage can use its full
FL9_1 constant budget; its selected-entry limit continues to reject fragment
shaders above 32 vectors. A strict public test reads c254 and c31, draws twice
with different groups, verifies that c254 shifts the geometry (17 green, 4 red
pixels), and confirms core rejects a 256-vector vertex pipeline before HAL.
The remaining hardware tests verify
sparse static ranges,
GPU-upload ordering, vertex/fragment use, padded and nested matrix
structures, array stride, pipeline and group transitions, and coexistence with
sampled textures. The multi-group test binds one group at two indices with
different dynamic offsets, switches pipeline register plans, tests layout
prefix invalidation and checks all pixels after direct and indexed draws.
The scratch test reads back both distinct GPU constant-buffer snapshots.

On the RTX 3090 Ti at requested and selected FL9_1, the `hal_uniform` filter
passed four cases, including a hardware test of exact GPU-copied 4-, 5-, 12-
and 20-byte ranges, a range at the source-buffer end, and zero native padding.
The controlled core fixture passed short bindings of those sizes, including a
scalar with a dynamic offset, with 64 checked pixels per render. Its synthetic
downlevel flags enable `BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED`; the initial core
test without that flag rejected the shader as `UnalignedShader`. This is a
fixture setting, not a capability in the public projection. Binding materialization uses CPU
scratch and a mapped staging buffer, which can stall. An odd-sized native copy
is not verified on historical FL9_1 drivers.
The additional strict `fl91_short_uniform_dynamic_copy_and_odd_source_end`
GPU test passed on the RTX 3090 Ti at requested and selected FL9_1. It verified
the exact bytes and zero padding for a 5-byte dynamic range after a GPU copy
and for a 5-byte range ending exactly at the end of a 65-byte native source.

The `integer_uniform` hardware filter now checks rejection of three original
integer entry points: vertex and fragment uniform bitcasts and a fragment
comparison around 2^24. Each test instead renders a float-only control and checks
64 pixels. Previous RTX output for the integer shaders is historical and
superseded. An FXC probe found that the embedded legacy pixel shader rounded
16777217 to 16777216, while a legacy vertex shader lowered a bitcast to numeric
float arithmetic. Forced FL9 execution on the RTX does not establish historical
driver integer behavior.

The selected FL9 shader profile guard applies to checked and unchecked modes.
It rejects runtime integer values, operations, conversions and bitcasts,
integer-containing uniforms or interfaces, switches, and non-folded global
initializers and overrides. Proven static literal indices and float-only shader
paths remain available; analysis limits fail closed. These restrictions do not
establish general integer shader support or broaden the public shader contract.

Core exposes the backend restrictions through
`DownlevelLimits::runtime_integer_shader_operations` and
`DownlevelLimits::matrix_aggregate_stores`. Selected-entry analysis follows
reachable helper calls and runs before HAL pipeline creation. The CPU tests
cover helper-passed integer values, safe direct integer vertex-input conversion,
matrix aggregate stores through a pointer helper, safe matrix reads and safe
sibling entry points:

```powershell
cargo +1.93 test --offline -j32 -p wgpu-core --lib --no-default-features --features dx11,std,wgsl fl91_ -- --nocapture
cargo +1.93 test --offline -j32 -p wgpu-types --lib unsupported_fl9_shader_semantics
```

The strict public API regression runs in the offscreen test and verifies that
fragment runtime integer arithmetic and a vertex store through a matrix
aggregate fail with core validation errors, while the unused safe sibling
entries compile:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_api_renders_offscreen_at_fl91 -- --nocapture --test-threads=1
```

The physical validation run used the RTX 3090 Ti at requested and actual
FL9_1. It does not establish historical-driver behavior or broader integer
shader support.

### FL9_1 depth attachments

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_test -- --nocapture --test-threads=1
```

The test samples a texture while drawing overlapping geometry in both depth
orders, verifies initial zero depth, checks independent depth-only and
stencil-only clears, and checks an Always/no-write pipeline. Ten 8x8 readbacks
are compared in full. It also checks attachment extent and pipeline mismatch
rejection. The private HAL also supports restricted read-only attachment
passes and the limited one-sided stencil path described below; aspect views and
asymmetric front/back state remain unavailable.
Additional passes cover all color write masks, face culling, sampled textures
and samplers in separate groups, sampler and pipeline transitions, alias
rejection, and coexistence with a uniform in another group.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_never_preserves_color_depth_and_validation -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_depth_comparison_functions_match_expected_fragments -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The strict RTX FL9_1 comparison test passed all eight wgpu depth functions.
Seven use native D3D11 depth comparisons, each checked with passing and failing
depths; all 128 pixels matched (64 green, 64 blue). Never retains its two
existing depth-write variants, each checking 32 green and 32 blue pixels; two
controlled core variants each checked 64 depth-dependent pixels. Inert-stencil
Never draws are suppressed after validation within the current side-effect-free,
read-only-resource and no-query subset. Active stencil uses the separate
Always-based depth-fail remapping documented above. The documented FL9_1
LessEqual restriction applies to comparison samplers, not raster depth state.
Future shader side effects or query support require reviewing draw suppression.
Historical hardware remains unverified.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 stencil -- --nocapture --test-threads=1
```

This filter contains two CPU-only mapping/classifier tests and one GPU test. The
physical RTX 3090 Ti test explicitly requests and confirms FL9_1. It verifies
one-sided reference and Equal behavior, masked no-op behavior, and all eight
stencil operations in each of the native pass, stencil-fail and depth-fail
branches. Boundary inputs cover saturating and wrapping arithmetic. Each of 24
cases reads back an 8x8 color target after a later Equal test; the HAL cannot
directly read stencil storage. The test also rejects active asymmetric
front/back state. Microsoft documents the FL9_1 two-sided-stencil restriction
in the linked 10Level9 device table in `CAPABILITIES.md`. These results do not
establish behavior on historical FL9_1 hardware.

### FL9_1 default buffer bind flags

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 level9_default_buffer_bind_flags -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_default_copy_and_uniform_buffers -- --nocapture --test-threads=1
```

The first test is synthetic. The second checks native descriptors and a
copy/readback on a hardware device explicitly created at FL9_1.

### Controlled core integration

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture
```

### Public transient attachment pass

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_transient_attachments_at_fl91 -- --nocapture --test-threads=1
```

On the RTX 3090 Ti at requested and actual FL9_1, the test passed. It checks
transient color clear/draw/discard execution and transient D24S8 clear/draw/
discard use in a depth test; the ordinary RGBA8 output was green at all 64
pixels. Core rejected transient sampling, invalid color/depth load/store
operations, read-only transient depth and render passes with no attachments.
Invalid render-pass descriptors are checked after finishing their isolated
encoder because core reports those errors at finish.

### Public `LoadOp::DontCare`

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_dont_care_load_overwrite_at_fl91 -- --nocapture --test-threads=1
```

The test constructs `LoadOp::DontCare` with its required unsafe token only
because a full-screen `Always` draw writes every color and depth pixel before
the later `Less` depth test. On the RTX 3090 Ti at requested and actual FL9_1,
all 64 pixels read back as `[255, 0, 0, 255]`. No undefined pre-pass contents
are read.

### Public discarded color initialization

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_discarded_color_is_initialized_before_read_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_discarded_color_is_initialized_before_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_discarded_color_is_initialized_before_load_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_new_color_is_initialized_before_load_at_fl91 -- --nocapture --test-threads=1
```

The public test clears an RGBA8 attachment red, discards it, then copies it to
a CPU buffer. Core's discard tracking initializes the texture before the later
read. The strict RTX 3090 Ti FL9_1 run observed transparent black in all 64
pixels, not the retained red D3D11 contents.

The companion `public_dx11_discarded_color_is_initialized_before_sampling_at_fl91`
test samples the same discarded texture through a sparse logical texture and
sampler binding in a second render pass. The output pass read back transparent
black at all 64 pixels, confirming initialization before the SRV use and the
render-target-to-shader-resource transition on the forced-FL9_1 device.

The companion `public_dx11_discarded_color_is_initialized_before_load_at_fl91`
test begins a second render pass using `LoadOp::Load` on that discarded
attachment, stores it and copies it to readback. All 64 pixels are transparent
black instead of the prior red contents. Both focused tests passed, and the
complete strict public suite passed 31/31. The optional Float16x2 input branch
logged its capability skip; required FL9_1 operations ran on the physical
adapter.

The companion `public_dx11_new_color_is_initialized_before_load_at_fl91` test
uses `LoadOp::Load` on a newly created color attachment before any write, then
stores and reads it back. All 64 pixels are transparent black. The focused
test passed 1/1; after adding it, the full strict public suite passed 32/32.

### Public command-buffer resource retention

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_texture_references_survive_until_submission_at_fl91 -- --nocapture --test-threads=1
```

The test records an RGBA8 attachment clear and texture-to-buffer copy, finishes
the command buffer, then drops the public texture and view handles before
submitting. The strict RTX 3090 Ti FL9_1 run read back blue in all 64 pixels,
confirming the recorded commands retain the referenced resources through
execution. The focused test passed 1/1; the full strict public suite passed
33/33, with the optional Float16x2 input branch visibly capability-skipped.

### Public mapped-view destruction lifetime

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_buffer_destroy_preserves_live_mapped_views -- --nocapture --test-threads=1
```

Destroying a buffer with a live mapped view now panics before core can unmap
and release the mapped allocation. The test catches that panic, opens a second
nonoverlapping 8-byte view to prove the mapping state was preserved, checks
both views, drops them and then successfully destroys the buffer. The strict
run selected the RTX 3090 Ti at requested/actual FL9_1. This test exercises
native mapped-buffer lifetime, not a GPU draw or submission.

The public `public_dx11_buffer_handles_survive_until_submission_at_fl91` test
complements that mapped-view case. It maps and initializes a 64-byte COPY_SRC
buffer, queues sentinel bytes into a destination, records a partial source to
destination copy and a destination to MAP_READ copy, finishes the command
buffer, then drops the public source and destination handles before submission.
The strict RTX FL9_1 run verified all 64 output bytes, including the copied
28-byte region and untouched sentinels. The focused test passed 1/1; the full
strict public target passed 105/105. Reproduce it with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_buffer_handles_survive_until_submission_at_fl91 -- --nocapture --test-threads=1
```

The unsafe, nondefault fixture injects one explicitly selected hardware FL9_1
device into core separately from ordinary adapter enumeration. The test uses
core buffer upload/copy and a WGSL offscreen triangle, submission, fence wait,
mapped readback and explicit teardown. It checks all 64 copied bytes and all
64 rendered pixels, including the 32-red/32-green render. It also validates
unsupported transient formats and usages and creates transient textures and
views. Its restricted numeric limits and format capabilities do not constitute
a safe general adapter contract. The fixture verifies that core's format-feature
query preserves the HAL's RGBA8, D24S8, narrow BC1/2/3 and unsupported-BC results. It
also verifies that core rejects asymmetric stencil state with the missing
`TWO_SIDED_STENCIL` downlevel flag before invoking the FL9_1 HAL. Full-channel
straight-alpha blending is rejected with `SEPARATE_ALPHA_BLEND`, while the
same blend with alpha writes masked creates through the existing HAL path.

### Public indirect-draw rejection

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_indirect_buffers_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j32 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_compute_pipelines_at_fl91 -- --nocapture --test-threads=1
```

The physical FL9_1 adapter reports no `DownlevelFlags::INDIRECT_EXECUTION`.
Core rejects creation of a `BufferUsages::INDIRECT` argument buffer with a
validation error naming that gate, before any indirect draw method can reach
the HAL. The focused strict test passed 1/1.

The companion `public_dx11_rejects_compute_pipelines_at_fl91` test asserts the
selected adapter omits `DownlevelFlags::COMPUTE_SHADERS` and verifies that core
rejects compute-pipeline creation with that named validation gate before HAL.
It also passed 1/1 on the forced-FL9_1 physical device.

The `public_dx11_enforces_query_type_feature_gates_at_fl91` test confirms that
timestamps still require the caller to request the advertised feature, while
occlusion and pipeline-statistics queries remain unsupported. The focused
timestamp execution test and current command are recorded below.

### FL9_1 line topologies

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_line_lists_and_strip_render -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_line_batch_limits_preserve_complete_primitives -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 line_list_batches_preserve_pairs_and_instances -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 line_strip_batches_overlap_one_vertex_across_batches -- --nocapture
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 indexed_line -- --nocapture
```

The GPU test requests and asserts FL9_1, renders direct and indexed line lists
and strips, and verifies readback colors. Indexed coverage includes Uint16 and
Uint32 buffers, nonzero first indices, restart-separated runs, bounded
compaction, a wide pair/edge that requires vertex gathering, and a mismatched
strip format rejected before submission. The Uint16 path uses `baseVertex = 1`
to turn `0xFFFE` into the resolved restart marker; the Uint32 path uses the raw
marker at base zero. A wide Uint32 strip asserts a specific pixel on the
gathered edge as well as the compact continuation, so the gathered pair must
be visible. A separate indexed draw whose effective fetch is out of range
leaves the target clear. CPU tests verify pair-preserving line batches, strip
endpoint overlap, resolved-index restart splitting, signed-base rebasing,
wide-edge gathering, whole-draw discard and the 131,070 LineList / 65,536
LineStrip index limits. On the RTX 3090 Ti at requested and actual FL9_1, the
combined readback contained 17 red, 12 green, 24 blue, 14 white, 10 cyan and 14
yellow pixels. A separate wide Uint32 strip readback contained 15 yellow
pixels, including the asserted gathered-edge pixel.

The controlled core fixture also creates indexed LineList pipelines and submits
Uint16 and Uint32 draws through its separately injected test adapter:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture
```

Both indexed LineList draws and both indexed LineStrip draws read back red
pixels. This exercises core command recording through the explicitly injected
test device; the public offscreen adapter path is covered separately by
`wgpu/tests/dx11_public.rs`.

Indexed TriangleStrip uses the same controlled core fixture. Its Uint16 and
Uint32 cases select a nonzero first index and signed base vertex that resolves
an ordinary value to the strip-restart marker. Each run renders 12 red pixels;
the fixture asserts the complete expected color result through the explicitly
injected test device.

The private `fl91_nonindexed_triangle_strip_winding_and_first_vertex` GPU test
also exercises indexed TriangleStrip. It compares Uint16 and Uint32 restart
strips against equivalent triangle-list rendering for both front-face values
and both cull modes. Readback images must match exactly. A separate Uint32
triangle whose index span exceeds the FL9_1 16-bit window verifies the GPU
gather path with 32 checked red pixels. Format mismatch and missing declared
strip-index format are rejected before submission. CPU planner tests cover
restart splitting after base-vertex resolution, winding reset per run, the
65,535-triangle FL9_1 native call limit and whole-draw discard on invalid
vertex fetches. Replay scans GPU-authoritative index bytes through staging,
which preserves command ordering but can stall the CPU; native strip-cut
support is not used.

### Single-sample output mask

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_zero_sample_mask_uses_equivalent_nonzero_native_mask -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The first test checks production mask translation, including high-only `u64`
masks. The strict core fixture requested and selected FL9_1 on the RTX 3090 Ti;
its `mask = 0` pipeline preserved all 64 blue clear pixels. FL9_1 forbids a
zero native `SampleMask`, so the backend passes `0xffff_fffe` for single-sample
rendering. For multisampled rendering it complements the active sample bits;
the 2x, 4x and 8x masks therefore leave all active samples disabled while
using nonzero native masks. The strict public zero-mask test still verifies
the 4x result. Query-supported 2x, 4x and 8x RGBA8 render/resolve and matching
D16/Depth24Plus/D24S8 depth attachments are supported; Depth24Plus uses D24S8
native storage without stencil access. The strict public sample-count test
rendered and resolved x2 and x8 targets, and the existing test exercises x4
with depth. Sixteen-sample targets are not exposed on the tested device;
multisampled sampling, other formats and alpha-to-coverage remain unsupported. See
[OMSetBlendState FL9_1 restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context)
and [WebGPU multisample validation](https://www.w3.org/TR/webgpu/#validating-gpumultisamplestate).

### Query-gated RGBA8 multisample render and resolve

The FL9_1 feature level does not guarantee multisampling. For each supported
count (2, 4, 8 or 16), the selected device must report a positive
`CheckMultisampleQualityLevels(RGBA8_UNORM, count)` result and
`CheckFormatSupport` must include `TEXTURE2D`, `RENDER_TARGET`,
`MULTISAMPLE_RENDERTARGET` and `MULTISAMPLE_RESOLVE`. A failed or zero query
omits only the corresponding sample-count flag, and core rejects that count.
The HAL supports query-backed single-mip, single-layer RGBA8 color attachments
and same-sized single-sample resolve destinations. D16_UNORM and
D24_UNORM_S8_UINT depth attachments are enabled independently for each
positively queried count; depth must match the color target and pipeline sample
count. Depth resolve, multisampled sampling, other formats and alpha-to-coverage
remain unsupported.

On the RTX 3090 Ti forced to FL9_1, RGBA8 quality-level queries for 2x, 4x, 8x
and 16x returned 1, 1, 1 and 0; D16 and D24S8 are queried independently. The
RGBA8 format mask was `0x312ef3e2`. Strict public fullscreen draws at 2x and 8x
resolved to green and matched all 64 pixels at each count. The existing strict
4x triangle resolved to a
single-sample texture, and readback checked all 64 pixels: 28 full red triangle,
8 partial-coverage red/blue resolve values and 28 clear blue pixels.
This is optional selected-device evidence from a modern GPU, not a FL9_1
hardware guarantee.

The same strict test selected D24PlusStencil8 after a positive 4x query,
created the matching multisampled depth view and drew near red geometry before
far green geometry over the same triangle. It then repeated the test with
Depth24Plus, whose native backing is D24S8 but whose stencil aspect is hidden.
Both resolved 8×8 targets contained 28 full-red interior pixels, 8 red/blue
edge pixels and 28 blue clear pixels; all 64 green channels were zero,
confirming the farther draw failed depth. This verifies the selected RTX
execution path, not every FL9_1 driver.

The same strict run then cleared the multisampled target blue, rendered with a
zero WebGPU sample mask and resolved again; all 64 pixels remained exactly
`[0, 0, 255, 255]`.

### FL9_1 alpha-to-coverage validation

Four-sample RGBA8 render-and-resolve support does not imply alpha-to-coverage
support. The FL9_1 projection reports
`DownlevelLimits::alpha_to_coverage = false`, and core rejects a multisampled pipeline enabling it before the HAL.
The controlled fixture checks this error while retaining the existing separate
single-sample validation for alpha-to-coverage.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

On the RTX 3090 Ti, requested and actual FL9_1, the fixture's unsupported
multisampled request was rejected by core before HAL; the rest of the strict
fixture also completed its real D3D11 operations and readbacks. This does not
test native alpha-to-coverage rendering.

### Multisampled texture usage boundary

The strict public resolve test also checks the exact usage subset exposed by
`DownlevelLimits::multisampled_texture_usage_mask`. On devices that positively
report a supported RGBA8 sample count, a render attachment remains creatable,
while adding sampled, copy-source or copy-destination usage is rejected by core
before native allocation. The HAL creates render-target-only RGBA8 color
resources and query-supported D16/D24S8 depth attachments, optionally marked
transient.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_four_sample_rgba8_at_fl91 -- --nocapture --test-threads=1
```

On the RTX 3090 Ti, requested and actual FL9_1, this test created and resolved
the supported RGBA8 target with a query-supported D24S8 depth attachment,
rejected `RENDER_ATTACHMENT | TEXTURE_BINDING`,
`RENDER_ATTACHMENT | COPY_SRC` and `RENDER_ATTACHMENT | COPY_DST` before HAL,
and verified all 64 resolve pixels.

`public_dx11_resolves_optional_fl91_sample_counts` separately renders and
resolves full-screen triangles for every queried x2 and x8 count. On the RTX
3090 Ti both counts passed, and all 64 readback pixels per count were exactly
green. Unsupported optional counts are visibly skipped; zero and failed
queries do not enter the test's supported-count list.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_optional_fl91_sample_counts -- --nocapture --test-threads=1
```

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_resolves_four_sample_rgba8_at_fl91 -- --nocapture --test-threads=1
```

### FL9_1 mutable-format view rejection

`public_dx11_rejects_mutable_format_views_at_fl91` runs through ordinary
public adapter enumeration in strict hardware mode. It asserts that the
selected physical adapter requested and returned FL9_1 without `VIEW_FORMATS`,
then checks that core rejects compatible RGBA8/BGRA8 UNORM-to-sRGB view lists
before HAL. This is a capability and validation test; it does not submit
rendering work. Native typeless-resource creation also returned `E_INVALIDARG`,
and Microsoft's 10Level9 format table marks these typeless formats unavailable
at FL9_1.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rejects_mutable_format_views_at_fl91 -- --nocapture --test-threads=1
```

### FL9_1 cube texture sampling

`public_dx11_cube_texture_sampling_at_fl91` asserts the selected physical
adapter reports `CUBE_TEXTURES` and a 512 cube limit while leaving ordinary
2D arrays unavailable. It conditionally rejects RG8_UNORM cube creation when
the selected format mask lacks cube support. When `Rgba8Unorm` is not marked
`SINGLE_MIP_ONLY`, the test uploads four independently colored mip levels for
all six faces and renders each mip through an integer minimum-LOD sampler. It
also samples a non-suffix view of mip levels 1 and 2, exercising the backend's
six-face GPU clone refresh. Strict mode requires the positive mip query;
non-strict mode visibly skips mip-specific coverage if the device lacks it.
Nonsquare, NPOT, oversized, unsupported-mip, multisampled and render-target
cube descriptors, an ordinary two-layer array and a single-face D2 view are
checked through public core validation. The test also copies face 0 of a
second cube into face 1 of the sampled cube at every mip, then checks that
face's changed color in the render readback. It also copies a regular 2D image
into cube face 2 and cube face 3 back to an ordinary 2D texture. The
2D-to-cube copy is observed through sampling; the cube-to-2D copy is verified
by reading back all 64 texels. Cube-to-buffer readback is now allowed for the
selected device's query-supported copy formats. Core projects six copy layers
while ordinary 2D arrays remain disabled by `TEXTURE_2D_ARRAYS`.

The RTX 3090 Ti at requested/actual FL9_1 reported format mask `0x1f1f`,
vendor/device `0x10de:0x2203`, driver `32.0.16.2101`, and software=false.
Its RGBA8 format query reported mip support. Cube sampling and GPU face copies
passed with all 960 pixels matching expected face and mip colors, including
the cross-shape copied face and non-suffix view. Cube-to-2D readback matched
all 64 texels. Cube texture-to-buffer readback is separately exercised by
`fl91_cube_texture_to_buffer_readback_uses_padded_faces` below; it requires a
positive RGBA8 render-target query so the native cube resource is not
shader-resource-only. No cube RTV is created.
The RG8_UNORM cube request was rejected by core from the selected format mask.
These per-format `CheckFormatSupport` results do not imply historical FL9_1
drivers support the same optional formats.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib cube_ -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_cube_texture_to_buffer_readback_uses_padded_faces -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_cube_texture_sampling_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std,wgsl -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-core --lib --no-default-features --features dx12,std,wgsl
```
### Storage buffer validation boundary

The controlled core fixture verifies that a buffer with `STORAGE | COPY_SRC |
COPY_DST` can use the ordinary ordered upload/copy/readback path, with all 64
bytes checked. The same test verifies that storage-buffer layouts visible to
vertex, fragment or compute shaders fail at core validation with the missing
`VERTEX_STORAGE`, `FRAGMENT_STORAGE`, `FRAGMENT_WRITABLE_STORAGE` or
`COMPUTE_SHADERS` flags before the private HAL is called. This is not shader
storage support.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### FL9_1 WGSL loops

`public_dx11_runs_static_but_rejects_dynamic_shader_loops_at_fl91` compiles
statically bounded vertex and fragment loops through the normal public
pipeline path, renders their values, and checks all 64 pixels. Dynamic vertex
and fragment loops are rejected by core validation before FXC or GPU
submission. The synthetic core test also covers helper pointer writes, local
aliases, a safe sibling entry, and a provably unbounded loop with a literal
false exit. The public limits distinguish the statically unrolled subset from
dynamic-loop support.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu-hal --lib --no-default-features --features dx11 fl91_static_loops_compile_and_dynamic_loops_are_rejected_by_fxc -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_runs_static_but_rejects_dynamic_shader_loops_at_fl91 -- --nocapture --test-threads=1
```

### FL9_1 1D texture boundary

The strict public offscreen test asserts `DownlevelLimits::texture_1d` is
false and that core rejects a D1 texture before HAL. This is required by the
10Level9 device contract: `CreateTexture1D` is unavailable on 9.x, even though
the generic `max_texture_dimension_1d` value is nonzero.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_api_renders_offscreen_at_fl91 -- --nocapture --test-threads=1
```

### Public textured, depth-tested 3D scene

`public_dx11_textured_depth_tested_3d_scene_at_fl91` draws an indexed,
perspective-transformed cube through public wgpu using a sampled RGBA8 face
texture, a uniform transform, a nearest sampler and a D16 depth attachment.
It submits the same faces in forward and reverse order, then compares every
pixel in both 32×32 readbacks. The depth-tested images must match; a second
pair of runs omits depth and must differ under reversed face order, proving
that overlapping faces exercise depth rather than coincidentally avoiding
overdraw. The test also checks the clear background and at least three
distinct sampled face colors. This is a small integration scene, not a broader
capability or performance claim.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_fl91_scene -- --nocapture --test-threads=1
```

### Query-gated FL9_1 depth comparison sampling

The private HAL accepts a sampled D16, Depth24Plus or D24S8 depth view with a comparison
sampler only when `D3D11_FEATURE_D3D9_SHADOW_SUPPORT` positively reports the
FL9_1 LessEqual path. Each test writes 0.25 to the left half of a depth target,
leaves the right half at 1.0, samples with reference 0.5 and checks the full
8×8 RGBA8 result: 32 failed comparisons are black and 32 passing comparisons
are red. The public projection exposes this subset only when the selected
device query is positive: the `LessEqual` function bit, equal min/mag filters,
comparison anisotropy 1, and single-sample D16/Depth24Plus/D24S8 sampled usage. Core checks
those restrictions before HAL. The public test uses separate depth-attachment
and sampled views, renders the same two comparison outcomes, and validates all
64 output pixels. Public Depth24Plus coverage uses a D24S8-backed resource but
keeps the stencil aspect inaccessible; the controlled core fixture confirms
Depth24Plus and D24S8 project the same queried depth capabilities and that a
DepthOnly view of a combined D24S8 texture is still non-renderable. A separate
private probe checks native DSV/SRV creation for the typeless D16 and D24
resources. The typed SRV support query returns `E_FAIL` on the tested driver,
but native resource/view creation and actual sampled output succeed for all
three public depth formats.

The Depth24Plus path maps to a native D24S8 texture while preserving the
logical Depth24Plus format through texture and view metadata. It uses the same
selected-device D24S8 format and shadow-support queries as D24S8, but core does
not expose the backing stencil aspect. On the RTX 3090 Ti, its public
comparison test rendered 32 failing black pixels and 32 passing red pixels.
This verifies the selected driver path, not compatibility with every FL9_1
driver.

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib dx11::hal_encoder::shadow_sampling_tests:: -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib dx11::capabilities::tests::private_fl91_shadow_sampling_requires_level_backend_and_positive_query -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_query_gated_fl91_ -- --nocapture --test-threads=1
```

### Public front-face and culling

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_front_face_and_backface_culling_at_fl91 -- --nocapture --test-threads=1
```

The test draws adjacent triangles with opposite winding through public wgpu
pipelines using `FrontFace::Ccw` and `FrontFace::Cw`, both with back-face
culling. The forced-FL9_1 RTX 3090 Ti readback contained 10 red CCW pixels,
10 green CW pixels and 44 blue clear pixels, with no cross-winding output.

### Query-gated FL9_1 wireframe rasterization

`POLYGON_MODE_LINE` is added to the selected adapter feature set only if a
probe call to `ID3D11Device::CreateRasterizerState` accepts `D3D11_FILL_WIREFRAME`.
The backend maps triangle `PolygonMode::Line` to that native fill mode; point
polygon mode remains rejected. The public strict test compares the complete
wireframe triangle image against the same triangle's five explicit line
segments. This is selected-device evidence, not a historical-driver guarantee.

Reference: [D3D11_RASTERIZER_DESC](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_rasterizer_desc).

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_query_gated_wireframe_at_fl91 -- --nocapture --test-threads=1
```

### R8 mip-subresource copy follow-up

The strict public forced-FL9_1 test now copies a partial 2x3 rectangle at mip 1
between two four-level R8_UNORM textures, with a nonzero destination origin. It
reads back the full 4x4 destination mip using padded rows and verifies all 16
texels, including the unchanged cleared border. Copies between different R8
mip indices remain unverified.

The test also copies the top-left 2x2 region from source mip 1 into the full
2x2 destination mip 2, then reads back and checks all four texels. Thus both a
partial same-level copy and a full cross-level copy are covered; other R8 mip
source/destination combinations are not exhaustively tested.

### R8_UNORM NPOT mip-chain verification

When the selected device reports unrestricted NPOT mip sampling, the strict R8
mip render test now uses a 6x10 four-level texture. The RTX 3090 Ti reported
the capability at requested/actual FL9_1; mip 2 sampling matched all 64 RGBA
pixels, while mip-0 attachment readback matched all 60 R8 texels. Devices
without the optional query use the POT R8 chain and do not claim NPOT support.

### Upstream `wgpu-test` capability check

The shared GPU-test package and `wgpu-info` expose an opt-in `dx11` Cargo
feature. When generating `.gpuconfig` for a strict D3D11 run, select the backend
explicitly so the default-backend policy does not omit it:

```powershell
$env:WGPU_BACKEND = "dx11"
$env:WGPU_INFO_SAVE_GPUCONFIG_REPORT = "1"
cargo +1.93 test --offline -j4 -p wgpu-info --all-features generate_gpuconfig_report -- --nocapture
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j4 -p wgpu-test --all-features --test wgpu-gpu validate_capabilities -- --nocapture
```

Run these commands from a Visual Studio x64 developer environment. Strict mode
fails before test discovery unless `WGPU_BACKEND=dx11` and `.gpuconfig` contains
a physical adapter whose report identifies FL9_1. The capability test's
`[Executed]` status confirms it was not skipped for unsupported adapter
requirements. The `dx11` test-package feature is included by `--all-features`;
ordinary test runs without `WGPU_BACKEND=dx11` continue to use the default
backend set, which excludes D3D11.

### Optional R8G8_UNORM sampled mip chains

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j2 -p wgpu-hal --lib --features dx11 rg8_unorm_mip_chains_require_selected_query_and_backend
cargo +1.93 test --offline -j2 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_rg8_unorm_mip_chain_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j2 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,wgsl,std dx11_fixture_tests::fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

RG8_UNORM has no static FL9_1 sampled-mip guarantee. The backend exposes its
sampled/upload mip path only when the selected device positively reports
`TEXTURE2D | MIP` and the implementation path is enabled; `SHADER_SAMPLE`
continues to determine filtering independently. The synthetic test covers
missing and unavailable query bits, backend disablement and NPOT gating. The
RTX 3090 Ti at requested/actual FL9_1 reports `R8G8_UNORM=0x0002c320`, which
does not contain MIP. The strict public test therefore reports an explicit
optional-capability skip and performs no RG8 mip draw on this machine. The
controlled core fixture confirms the single-mip projection and conditional
creation rule. Execution on an FL9_1 device whose RG8_UNORM query includes MIP
remains unverified.

### Host-emulated `instance_index`

The selected FL9_1 vertex entry point may consume `@builtin(instance_index)`
through a host-updated constant at b13. This avoids the `SV_InstanceID` semantic
rejected by the 10Level9 FXC profile. Run both strict public tests from a
Visual Studio x64 developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_instance_rate_vertex_streams_at_fl91 -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_instance_index_nonindexed_and_indexed_draws_at_fl91 -- --nocapture --test-threads=1
```

The second test checks all 128 pixels after a non-indexed draw with
`first_instance=2` and an indexed draw with `first_instance=4`. TriangleList
and PointList `vertex_index` support is covered by the corresponding tests in
the public D3D11 suite; other topologies remain rejected. The RTX 3090 Ti at
forced FL9_1 verifies this compiler and driver path only; it does not represent
every historical FL9_1 driver.

### Direct vertex uniform-array indexing

The public FL9_1 path accepts direct `vertex_index` or `instance_index`
indexing of fixed-size `vec3` and `vec4` uniform arrays. The strict test checks
all four distinct array elements and the clamped out-of-range boundary on a
physical hardware adapter:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_vertex_uniform_array_indexing_at_fl91 -- --nocapture --test-threads=1
```

The Naga writer uses signed clamping only when the FL9_1 D3D11 binding map
opts in; other HLSL backends retain the existing unsigned output. Tests cover
both emitted forms, FXC compilation for `vs_4_0_level_9_1`, and rejection of
unsupported shapes in core and HAL.

The hidden input also consumes one of the sixteen physical D3D input slots.
Core mirrors HAL's FL9_1 overlap-allocation rules and rejects an exhausted
layout before HAL, only when the selected entry point uses `vertex_index`.
The strict public test includes a 16-stream rejection and confirms the same
layout remains valid without the builtin:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib emulated_vertex_index_requires_a_free_physical_input_slot
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_accepts_emulated_vertex_and_instance_index_at_fl91 -- --nocapture --test-threads=1
```

The core test covers full streams, overlapping and nonoverlapping attributes,
and the exact boundary for reserving the hidden slot. The public case performs
validation on the RTX 3090 Ti at requested/actual FL9_1; it does not issue a
draw. Existing public readback tests remain the execution evidence for the
emulated builtin itself.

### Legacy GLSL built-in validation

Run the selected-entry validation test with GLSL enabled in a Visual Studio x64
developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib legacy_shader_builtins_follow_selected_entry_and_direction
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_rejects_unimplemented_legacy_shader_builtins_at_fl91 -- --nocapture --test-threads=1
```

The CPU test covers stage, direction, missing names, and selected-entry
isolation. The public test rejects `BaseVertex`, `BaseInstance`, `PointSize`,
and `PointCoord` before HAL and confirms ordinary GLSL vertex/fragment
pipelines remain valid. On the RTX 3090 Ti the selected adapter is reported
as physical hardware at requested and actual FL9_1; rejected-pipeline cases
exercise validation and do not submit draws.

### Matrix inverse compiler gate

Run the selected-entry core test and strict public regression with GLSL enabled:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib unsupported_matrix_inverse_covers_compiled_entry_and_module_functions
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_rejects_unimplemented_matrix_inverse_at_fl91 -- --nocapture --test-threads=1
```

The core test covers selected entry points, regular module functions, vertex
and fragment stages, and a backend that permits inverse. The public test
verifies that the FL9_1 projection rejects selected inverse operations and
inverse-bearing helper functions before the HLSL writer while an ordinary
separate shader module remains valid; pipeline validation does not issue a
draw.

### Optional FL9_1 timestamp queries

Run the capability projection test, native probe and public resolve test from
an x64 Visual Studio developer environment:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib timestamp_query_features_require_a_positive_frequency_probe_and_backend_path
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_timestamp_and_disjoint_queries_execute_on_hardware -- --nocapture --test-threads=1
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_timestamp_queries_resolve_on_fl91 -- --nocapture --test-threads=1
```

The native probe creates timestamp and disjoint queries on the selected FL9_1
device, flushes and reads the disjoint frequency. A positive result enables
timestamp query sets, encoder writes and explicit writes inside render and
compute passes. The public test resolves timestamps written outside a pass, at
the beginning and end of a render pass, explicitly before and after a draw,
around an empty compute pass, and explicitly inside that compute pass. It
copies all ten values to mapped memory and checks every value is nonzero and
the command sequence is nondecreasing in execution order (query indices
`0, 1, 2, 6, 7, 3, 4, 8, 9, 5`). Pass descriptor timestamps
require only `TIMESTAMP_QUERY`; explicit pass writes additionally require
`TIMESTAMP_QUERY_INSIDE_PASSES`, which is exposed only with the positive
selected-device probe. Compute pipeline creation and dispatch remain
unsupported. The test also confirms occlusion and pipeline-statistics sets
remain rejected. D3D11 has no native query-result copy command here, so resolve
waits for `GetData` on the CPU. Strict execution passed 1/1 on the physical
NVIDIA GeForce RTX 3090 Ti (`0x10de:0x2203`, driver `32.0.16.2101`) at
requested/actual FL9_1 (`software=false`). The ten query-index values were
`[1794227433154996224, 1794227433155000320, 1794227433155001344,
1794227433155020800, 1794227433155020800, 1794227433155020800,
1794227433155009536, 1794227433155020800, 1794227433155020800,
1794227433155020800]`; in command order
`0, 1, 2, 6, 7, 3, 4, 8, 9, 5`, all ten were nonzero and nondecreasing.
The observed queue period was 1 ns/tick. The complete strict public target
passed 113/113 and the complete strict private HAL suite passed 450/450.

### Narrow FL9_1 fragment `textureLoad`

Run the selected-entry proof and limit-projection tests, then execute the strict
public GPU case:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib restricted_fl9_texture_load_proof_is_entry_point_specific
cargo +1.93 test --offline -j8 -p naga --lib fl9_2d_texel_load_emits_only_proven_reachable_helpers
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_load_emulation_accepts_direct_global_nested_helpers
cargo +1.93 test --offline -j8 -p wgpu-types --lib restricted_texture_load_does_not_imply_general_texture_load
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_fragment_texture_load_emulation_at_fl91 -- --nocapture --test-threads=1
```

This is not general `textureLoad` support. It covers one single-mip, non-arrayed `texture_2d<f32>` in a fragment entry point. The selected entry point and its reachable straight-line helpers may directly reference the same global image; helpers cannot receive or return resource handles. Other reachable bound globals, additional image identities, sampling/query operations and control flow are rejected. Unreachable helper functions and helpers used only by sibling entry points do not constrain this shader. Naga emits only the selected entry point reachable helpers and used globals for this proof. The selected-entry proof maps the texture to `t0`; private
shader emulation point-samples at texel centers using `s15`, reads dimensions
from `b13`, and yields zero for out-of-bounds coordinates and nonzero mip
requests. The public test on the physical RTX 3090 Ti at requested/actual
FL9_1 checked four 8×8 outputs: 64 direct-helper in-bounds green pixels, 64 out-of-bounds zero pixels, 64 zero pixels for a nonzero mip request and 64 nested-helper in-bounds green pixels. The broader
strict public suite also includes this test. This modern driver result does
not establish behavior on historical FL9_1 hardware.

### FL9_1 GLSL `outerProduct`

Naga's HLSL writer lowers validated f32 outer products to an explicit matrix
constructor. Run the Naga dimension/lowering regression and strict public
readback test from a Visual Studio x64 developer environment:

```powershell
cargo +1.93 test --offline -j16 -p naga --lib --features glsl-in outer_product
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_matrix_outer_product_renders_at_fl91 -- --nocapture --test-threads=1
```

The GLSL frontend regression checks that `outerProduct(vec3, vec2)` has the
specified `mat2x3` dimensions; the HLSL writer regression checks square and
rectangular component order. The strict public test explicitly requests and
asserts FL9_1 on the physical RTX 3090 Ti, then renders and reads back all 64
pixels for square and rectangular operations. The observed output was
`[16, 32, 48, 255]` per pixel. The capability applies to Naga IR accepted by
the backend; WGSL currently has no `outerProduct` builtin. DX12 exposure remains
disabled pending a separate projection decision.

### Narrow FL9_1 fragment `textureDimensions`

Run the synthetic capability/proof checks and strict readback test:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-types restricted_texture_dimensions_do_not_imply_general_texture_queries
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_dimensions_accepts_only_direct_fragment_conversion
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib texture_dimensions
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_emulation_at_fl91 -- --nocapture --test-threads=1
```

On the RTX 3090 Ti, the test explicitly requested and selected FL9_1. One
pipeline sampled the dimensions of two differently sized views across a bind
group switch; all 64 output pixels matched the expected yellow and half-yellow
values. The general `texture_query` capability remained false. This validates
the narrow emulation on a modern GPU restricted to FL9_1, not behavior on
historical hardware.

The same strict public test now calls `textureDimensions` through one selected
argument-free helper and switches the bound texture view between the 16x8 and
8x4 sources. The helper proof does not permit a mip argument, sampling, nested
calls, other selected resources or control flow. Direct queries retain their
existing proof. Focused CPU/FXC checks are:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_dimensions_accepts_only_direct_fragment_conversion
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_dimensions_accepts_one_selected_helper
cargo +1.93 test --offline -j8 -p naga --no-default-features --features hlsl-out,wgsl-in --lib fl9_texture_dimensions_emits_only_the_selected_query_helper
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_dimensions_through_selected_helper_compiles_with_fxc -- --nocapture
```

With `WGPU_DX11_REQUIRE_HARDWARE=1`, run the public forced-FL9_1 render and
readback test:

```powershell
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_emulation_at_fl91 -- --nocapture --test-threads=1
```

### Narrow FL9_1 fragment `textureNumLevels`

Run the selected-entry proof, type-limit and HLSL compilation checks, then the
strict public readback test:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-types restricted_texture_num_levels_is_independent_of_other_texture_queries
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_num_levels_accepts_only_direct_fragment_conversion
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_num_levels_allows_one_compatible_implicit_sample
cargo +1.93 test --offline -j8 -p naga --no-default-features --features hlsl-out,wgsl-in --lib fl9_texture_num_levels_can_share_texture_with_one_implicit_sample
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib texture_num_levels
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_num_levels_can_share_one_implicit_sample
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_num_levels_and_implicit_sample_compile -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_num_levels_emulation_at_fl91 -- --nocapture --test-threads=1
```

The 2D query also supports one selected, no-argument straight-line helper that
returns the queried `u32` for direct conversion to `f32`. The helper itself is
query-only, but the selected entry may also contain exactly one direct implicit
sample of the same image through one group-zero non-comparison sampler. Nested
or repeated calls, helper sampling, control flow, mismatched images, additional
selected resources, multiple samples and explicit LOD remain rejected. 3D/cube
query paths remain direct-only. The strict helper readback uses one- and
three-level views and checks both helper-only and helper-plus-sample variants.
Run its production proofs and hardware test with:

```powershell
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,std --lib fl9_texture_num_levels_accepts_one_selected_query_helper
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11-test-fixture,wgsl,glsl,std --lib fl9_texture_num_levels_helper_allows_one_matching_implicit_sample
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_num_levels_accepts_one_selected_helper
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_num_levels_helper_accepts_one_same_texture_sample
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_num_levels_through_selected_helper_compiles_with_fxc -- --nocapture --test-threads=1
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_num_levels_helper_with_implicit_sample_compiles_with_fxc -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_num_levels_selected_helper_at_fl91 -- --nocapture --test-threads=1
```

The query may share the selected entry with one implicit sample of the same
texture using one group-zero non-comparison sampler. For helper-backed 2D
queries, the helper remains query-only and the sample must be a direct entry
operation. The strict public test switches between one- and three-level views
over an initialized RGBA8 mip chain; the first output channel depends on the
view mip count and the next two channels depend on the sampled texel. Core,
Naga and private HAL tests accept the helper-plus-sample shape only for 2D and
reject a different image, multiple samples, explicit LOD, helper sampling,
comparison sampling or extra resources. FXC compiles the helper composition at
`ps_4_0_level_9_1`; the host mip-count query remains the existing `b13`
constant emulation and the sample remains an ordinary implicit `.Sample`.

The strict RTX 3090 Ti helper-plus-sample readback explicitly requested and
selected FL9_1. It verified `[85,64,128,255]` for the one-level view and
`[255,128,64,255]` for the three-level view across all 64 pixels. The same test
also reruns the helper-only form, retaining its previous readback coverage.

The public test uses one four-mip texture through two views exposing one and
three levels. A bind-group transition changes the returned count while the
same shader samples the selected view. The 8x8 readback expects
`[64,64,128,255]` on the left and `[191,128,64,255]` on the right; all 64
pixels were verified on the RTX 3090 Ti at explicitly requested and selected
FL9_1. General `texture_query` stays disabled. This is a narrow host metadata
emulation and does not establish historical FL9_1 driver behavior.

The `public_dx11_fragment_texture_num_levels_3d_and_cube_at_fl91` test also
queries sampled RGBA8 3D and cube views through the independent
`texture_num_levels_3d_float_fragment` and
`texture_num_levels_cube_float_fragment` limits. It switches between a
one-level view at the resource's lowest mip and a full-chain view, checking the
full 8x8 readback for each dimension. Both views include the lowest LOD as
required by FL9_1 shader-resource views. If the selected device lacks optional
mip support, the test still executes the single-mip query and reports that the
differing-count subcase was unavailable. The shader remains direct
fragment-only; helpers, extra resources and unrelated queries remain rejected.

```powershell
cargo +1.93 test --offline -j8 -p wgpu-types restricted_3d_and_cube_texture_num_levels_are_independent
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_num_levels_3d_and_cube_limits_are_independent
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_num_levels_accepts_3d_and_cube_images
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_num_levels_compiles_for_3d_and_cube_images
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_fragment_texture_num_levels_3d_and_cube_at_fl91 -- --nocapture --test-threads=1
```

### Constant-mip FL9_1 fragment `textureDimensions`

```powershell
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_dimensions
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_dimensions
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_constant_mip_at_fl91 -- --nocapture --test-threads=1
```

The constant-level test uses a four-mip 16x8 texture and two views with
different base mips. It queries level `2u` relative to each view and expects
yellow output for 4x2 dimensions on the left and half-yellow for 2x1 on the
right. Runtime, override and negative signed levels are rejected by synthetic
Core/HAL tests. General `texture_query` remains disabled.

### FL9_1 `textureDimensions` with one implicit sample

```powershell
cargo +1.93 test --offline -j1 -p naga --lib --features hlsl-out fl9_texture_dimensions_can_share_texture_with_one_implicit_sample
cargo +1.93 test --offline -j1 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_texture_dimensions_accepts_only_direct_fragment_conversion
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_and_sample_at_fl91 -- --nocapture --test-threads=1
```

The shader may combine one level-less `textureDimensions(image)` with one
implicit `textureSample(image, sampler, uv)` of the same group-0 2D float
texture. The direct selected-entry form rejects explicit sample/query mip
levels, helper calls, `select`, extra bound resources and other image
operations. A separate helper form permits one no-argument, straight-line
helper returning the dimensions for direct `vec2f` conversion, alongside one
direct implicit sample of that same image with one group-0 non-comparison
sampler. The helper cannot sample, call other functions or use control flow;
extra resources and other image operations remain rejected.

Run the Naga, Core, HAL and FXC proofs, then require the physical GPU test:

```powershell
cargo +1.93 test --offline -j8 -p naga --no-default-features --features hlsl-out,wgsl-in --lib fl9_texture_dimensions_helper_can_sample_same_texture
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,glsl,std --lib fl9_texture_dimensions_helper_allows_one_matching_implicit_sample
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_dimensions_helper_accepts_one_same_texture_sample
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_texture_dimensions_helper_with_implicit_sample_compiles_with_fxc -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_fragment_texture_dimensions_helper_and_sample_at_fl91 -- --nocapture --test-threads=1
```

The strict RTX 3090 Ti run requested and selected FL9_1, switched between 4x2
and 8x4 views with different texel colors, and matched all 64 readback pixels:
`[50,40,80,255]` and `[40,60,160,255]`. The complete strict public target
passed 116/116; its optional query-gated subcases are not counted as hardware
evidence. General texture queries remain disabled.

### Narrow FL9_1 fragment 3D `textureDimensions`

```powershell
cargo +1.93 test --offline -j1 -p naga --lib --features hlsl-out fl9_3d_texture_dimensions_can_share_texture_with_one_implicit_sample
cargo +1.93 test --offline -j1 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_3d_texture_dimensions_has_an_independent_narrow_proof
cargo +1.93 test --offline -j1 -p wgpu-hal --no-default-features --features dx11 --lib fl9_3d_texture_dimensions_binding_is_distinct_from_2d_emulation
cargo +1.93 test --offline -j1 -p wgpu-hal --no-default-features --features dx11 --lib fl91_3d_texture_dimensions_and_implicit_sample_compile
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j1 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_3d_at_fl91 -- --nocapture --test-threads=1
```

The selected fragment entry may combine one direct no-level
`textureDimensions(volume)` converted to `vec3f` with one level-less implicit
sample of that same non-array `texture_3d<f32>` through one group-0
non-comparison sampler. Helpers, extra resources, explicit LOD, explicit query
levels and unrelated image operations remain rejected. The strict test uploads
two different volumes, transitions bind groups from 4x2x8 to 2x4x1, and
multiplies the sampled color by the queried dimensions. On the physical RTX
3090 Ti explicitly requested and selected at FL9_1, all 64 output pixels
matched `[200, 80, 40, 255]` on the first half and `[40, 120, 10, 255]` on the
second half. The full private HAL suite passed 434/434, the full strict public
D3D11 target passed 98/98, Naga passed 147/147 and DX11 Core passed 106/106.
Optional capability subcases may report SKIP and are not GPU execution passes.
This result does not establish historical FL9_1 driver behavior.

### Narrow FL9_1 fragment 3D `textureLoad`

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p naga --no-default-features --features hlsl-out,wgsl-in --lib fl9_3d_texel_load_emits_only_proven_reachable_helpers
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib restricted_fl9_texture_load_proof_is_entry_point_specific
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_texture_load_emulation_accepts_direct_global_nested_helpers
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_3d_texel_load_helpers_compile_with_the_level_9_1_profile
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_3d_texture_load_selects_a_distinct_single_mip_emulation
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_3d_texel_load_compiles_with_the_level_9_1_profile
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_3d_texture_load_at_fl91 -- --nocapture --test-threads=1
```

The Naga and HAL proof tests accept nested straight-line helpers that directly
load the same volume and reject branching or mixed-resource helpers. The
FXC test compiles the helper shader using the actual level-9_1 pixel profile.
The public test uploads an asymmetric 2x2x2 RGBA8 volume and checks two direct
loads, a nested helper load, an out-of-bounds depth coordinate, and a nonzero
mip request through GPU readback; it also confirms Core rejects a branching
helper before HAL. The `texture_load_3d_float_fragment` limit remains separate
from the 2D limit and is projected only for positively queried, implemented
RGBA8 volume sampling. General `texture_load` stays disabled; historical
FL9_1 drivers are not represented by the RTX result.

### Narrow FL9_1 fragment cube `textureDimensions`

```powershell
cargo +1.93 test --offline -j8 -p naga --lib --features hlsl-out fl9_cube_texture_dimensions_uses_face_size_metadata
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_cube_texture_dimensions_has_an_independent_narrow_proof
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_cube_texture_dimensions_selects_face_metadata_emulation
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl91_cube_texture_dimensions_and_implicit_sample_compile
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_cube_at_fl91 -- --nocapture --test-threads=1
```

The previously verified strict public test explicitly selected the physical
RTX 3090 Ti at requested/actual FL9_1, initialized two RGBA8 cube resources,
and rendered through one 8x8 then one 4x4 view. Its padded readback matched
yellow `[255,255,0,255]` and half-yellow `[128,128,0,255]` across all 64 pixels.
The test now also includes a constant-level query of mip 1 from an initialized
four-level cube when the selected-device RGBA8 format features permit mip
chains. Strict execution on the RTX 3090 Ti at requested/actual FL9_1 matched
all 64 pixels to `[128,128,0,255]` within one byte. Strict mode fails if the
mip prerequisite is absent, while normal mode reports the mip-specific case
as skipped.
Dynamic or negative levels and an explicit-level query combined with sampling
remain rejected. This does not establish general texture-query support or
historical-driver behavior.

### Narrow FL9_1 fragment 3D `textureDimensions` with a constant mip

```powershell
cargo +1.93 test --offline -j8 -p naga --no-default-features --features hlsl-out,wgsl-in --lib fl9_3d_texture_dimensions_uses_host_supplied_int3
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features dx11,wgsl,std --lib fl9_3d_texture_dimensions_has_an_independent_narrow_proof
cargo +1.93 test --offline -j8 -p wgpu-hal --no-default-features --features dx11 --lib fl9_3d_texture_dimensions_binding_is_distinct_from_2d_emulation
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public public_dx11_fragment_texture_dimensions_3d_constant_mip_at_fl91 -- --nocapture --test-threads=1
```

The selected fragment entry may pass only a nonnegative literal or module-
constant mip level to the direct 3D `textureDimensions` query. Dynamic and
negative levels, helper calls, extra resources and combining an explicit-level
query with sampling remain rejected. The public readback uses an 8x4x8 RGBA8
volume with three levels; querying level 1 must produce 4x2x4, normalized to
`[128,128,128,255]`. Multi-mip volume creation is separately query-gated by
`texture_3d_mipmaps`; a machine lacking that prerequisite cannot provide GPU
evidence for this case. A strict run must be reported as a skip or failure,
never as a rendering pass, if the prerequisite is unavailable.

### Narrow FL9_1 `mat2x2<f32>` matrix inverse

```powershell
cargo +1.93 test --offline -j8 -p naga --features glsl-in --lib matrix_inverse_
cargo +1.93 test --offline -j8 -p wgpu-core --no-default-features --features "wgsl glsl spirv" --lib matrix_inverse
cargo +1.93 test --offline -j8 -p wgpu-types matrix_inverse
$env:WGPU_DX11_REQUIRE_HARDWARE = '1'
cargo +1.93 test --offline -j8 -p wgpu --no-default-features --features dx11,wgsl,glsl,std --test dx11_public public_dx11_mat2_matrix_inverse_at_fl91 -- --nocapture --test-threads=1
```

The selected D3D11 projection permits only f32 `mat2x2` inverse; general
`matrix_inverse` remains false. The public GLSL test executes inverse in both
vertex and fragment stages at requested/actual FL9_1. Its asymmetric
`mat2(1, 0, 1, 1)` case yields `[0, 255, 0, 255]` in all 64 pixels. It then
confirms `mat3` inverse, including an unused regular helper that Naga still
emits, is rejected as a Core validation error before HAL. WGSL does not define
an inverse builtin; this path covers GLSL and other Naga IR sources that
contain the operation. The RTX result does not establish historical driver
behavior.
