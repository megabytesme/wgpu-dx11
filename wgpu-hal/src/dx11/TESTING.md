# Direct3D 11 validation plan

The `dx11` feature now enumerates only physical adapters that create the
explicitly requested FL9_1 device, and uses the selected-device capability
projection for offscreen public wgpu use. The reported feature set is empty,
the downlevel contract is not WebGPU-compliant, and no surface capabilities are
reported. This remains experimental, and the existing HAL/core tests do not
establish complete wgpu or CTS conformance. Do not widen the projection until
core-valid operations are audited against the HAL; some restrictions cannot be
expressed by `Limits` alone.

### Public offscreen integration test

The public integration test selects DX11 explicitly, asserts the physical
FL9_1 adapter identity and driver version, and requests a device within the
adapter's reported limits. It uploads a two-color texture, binds it in group 7
at logical bindings 3 and 17, and binds a dynamic vertex transform at group 0
binding 3 and dynamic fragment tint at group 0 binding 17. This exercises all
eight bind-group positions while reusing binding numbers across groups. An indexed
textured scene uses those stage-visible uniforms with a D24S8 depth attachment.
Readback checks all 64 pixels: 32 tinted-red pixels and 32 green pixels from
the depth-tested indexed draw. The test writes a translated transform into a
256-byte-aligned range and selects it with a dynamic offset between submissions;
the second frame contains 16 blue, 32 tinted-red and 16 green pixels. It then
submits independent clear/copy/readback workloads concurrently through the
first and a second public device from the same adapter. Strict mode turns
missing physical hardware into a test failure:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j16 -p wgpu --no-default-features --features dx11,wgsl,std --test dx11_public -- --nocapture --test-threads=1
```

The same strict public test submits unsupported descriptors under validation
error scopes: 3D textures, 4x multisampled color textures, `R32Float` sampled
textures, two-mip D24S8 attachments, vertex-visible storage-buffer layouts and
`D2Array` sampled-texture layouts. Each must return
`wgpu::Error::Validation`, confirming the public capability projection rejects
these paths before the private HAL.

The development RTX 3090 Ti reported vendor `0x10de`, device `0x2203`, driver
`32.0.16.2101`, requested and actual FL9_1, and `software=false`. The main
image matched 32 tinted-red/32 green pixels; a queue write plus dynamic uniform
offset shifted the second image to 16 blue/32 tinted-red/16 green pixels. The
subsequent concurrent public-device workloads read back 64/64 red and 64/64
green pixels. The shader and bind-group layouts use group 7 with binding
numbers also used in group 0. This is a representative public offscreen
integration case, not a complete application or adapter-contract audit.
Presentation remains unavailable. All six unsupported resource requests were
rejected by public core validation.

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
RGBA8. The format reports only `TEXTURE_BINDING | COPY_DST` and
`SINGLE_MIP_ONLY`; all 64 pixels are checked, with 32 red values of 64 and 32
red values of 192. It runs under the same strict hardware command above.

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

A sixth strict public test declares two fragment textures at sparse logical
bindings 7 and 42, plus one shared sampler at binding 100. The bind-group
layout, WGSL declarations and runtime entries use different orders. Both
uploaded textures contribute to the output; readback verifies all 64 pixels
are `[104, 128, 24, 255]`. An incomplete group missing binding 42 is rejected
through public core validation before rendering.

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

A ninth strict public test uploads all four levels of an 8×8 RGBA8 mip chain
with different colors. Implicit derivatives and literal sample biases -3, +1
and +3 select their expected mip colors across 256 readback pixels. Dynamic,
helper-forwarded, out-of-range and offset biases are rejected by core validation
before HAL compilation. CPU-only core analysis tests cover fractional bias and
constant helper calls. The same public test confirms that a multi-mip R8
texture is rejected because the selected format projection marks R8 as
single-mip-only.

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

The separate `texture_sample_base_clamp_to_edge` limit covers the WGSL
`textureSampleBaseClampToEdge` builtin. Its Naga HLSL lowering uses both
`GetDimensions` and `SampleLevel`; FXC reports X4532 for
`ps_4_0_level_9_1` and `ps_4_0_level_9_3`, while the shader-model 5 control
compiles. CPU core tests cover direct use, helper reachability, safe sibling
entry points and the default-supported projection. The strict public FL9_1
test verifies that the selected-entry gate produces a validation error before
HAL for direct and helper-reached use.

A tenth strict public test uploads both `Rg8Snorm` and `Rgba8Snorm`, samples
them with one non-filtering sampler, converts signed values to visible
unorm output and checks all 64 pixels against `[192, 63, 160, 255]` within
one channel value. It confirms the selected format reports allow sampling and
uploads, but not filtering, and remain single-mip-only.

An eleventh strict public test draws per-instance offsets and colors from a
second vertex stream using `draw(0..3, 1..3)`. It verifies that the nonzero
first-instance range selects the second and third records, while the skipped
first record remains absent. It also confirms a vertex entry declaring
`instance_index` is rejected by public core validation.

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
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

### Internal FL9_1 capability projection

`Capabilities::wgpu_projection` is exercised against synthetic records and an
explicitly selected physical adapter. It returns a snapshot only for an
actually created FL9_1 device. Both ordinary opt-in D3D11 enumeration and the
core fixture consume the projected limits, alignments and downlevel gates. The
fixture locally opts into aggregate BC support only to test the private BC1
path; the production projection withholds `TEXTURE_COMPRESSION_BC` because the
backend does not implement the complete BCn format family.

```powershell
cargo +1.93 test --offline -j 16 -p wgpu-hal --lib --no-default-features --features dx11 wgpu_projection -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo +1.93 test --offline -j 16 -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The projection test records the selected adapter identity, requested and actual
feature level, driver format/optional-feature queries, and conservative limits
and flags. The RTX's forced FL9_1 result is not evidence for historical FL9_1
drivers.

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

### Vertex system-value input validation

The controlled FL9_1 core fixture sets `vertex_index_input` and
`instance_index_input` false. It verifies that the selected vertex entry point
is rejected before HAL when it declares either input, and that another safe
entry point in the same module remains usable. CPU interface tests cover direct
and structure-member declarations, unused declarations, safe sibling entries
and missing entry names. A separate D3DCompile test verifies that compiling the
backend-generated `SV_VertexID` interface for `vs_4_0_level_9_1` fails with
FXC X4555, whether the WGSL value is used or unused. FXC acceptance of
`instance_index` has not been established; the backend currently rejects it.

```powershell
cargo test --offline -p wgpu-types --lib vertex_builtins_are_downlevel_capabilities -- --nocapture --test-threads=1
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std vertex_builtin_detection_uses_selected_entry_point_declarations -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 fl91_vertex_index_entry_inputs_are_rejected_by_fxc -- --nocapture --test-threads=1
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-core --lib --no-default-features --features dx11-test-fixture,std fl91_core_buffer_and_offscreen_render_on_hardware -- --nocapture --test-threads=1
```

The fixture exercises core validation on the forced-FL9_1 adapter; it does not
draw using either unsupported input. These restrictions are recorded as
profile/backend limitations, not hardware guarantees.

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
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 vertex_formats_map_the_fl91_required_input_set -- --nocapture --test-threads=1
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

### Public FL9_1 D16 depth-only attachment

The D16 format is a feature-level 9_1 required depth attachment format, but the
backend still intersects the guarantee with the selected device's
`CheckFormatSupport` result. The public test verifies that the adapter reports
only render-attachment/transient usage and `SINGLE_MIP_ONLY`, then creates a
D16 texture through wgpu. Near and far full-screen triangles are submitted in
both draw orders with depth writes enabled; all 128 pixels retain the near
triangle's green output. Core rejects a two-mip D16 texture and a stencil
pipeline before HAL.

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

With strict hardware mode enabled, run the zero-stride tests with:

```powershell
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 zero_stride -- --nocapture --test-threads=1
```

The filter passed one CPU case and one physical RTX 3090 Ti case at requested
and selected FL9_1. The GPU case checked five full-image executions with the
constant stream first or last, covering direct and strip draws, Uint16 and
Uint32 indexed windows and gather, a constant-resource update, and rejection of
a 19-byte range where the attribute needs 20 bytes. An all-zero-stream triangle
only established degenerate draw completion. The controlled core fixture used a
synthetic per-stage buffer limit of two and checked 64 green pixels. The correct
handling of a negative base-adjusted index when all streams have zero stride is
still an open specification question; there is no visible proof from the current
indexed triangle-only subset. Historical FL9_1 drivers remain unverified.

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
pixels. Pipelines using `instance_index` remain unsupported. Indexed lists and
strips with only instance-rate/no vertex-rate inputs are covered below. The
strip path reads the current index buffer during replay and emits a direct strip
per restart run so it does not merge primitive runs. These results do not
establish general instancing or historical FL9_1-driver behavior.

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

`@builtin(sample_mask)` output is another profile boundary. Naga emits HLSL
`SV_Coverage`, which Microsoft documents as writable only from `ps_4_1`;
`ps_4_0_level_9_1` rejects it. The HAL test captures the FXC failure and
compiles a color-only sibling. The strict public test confirms core rejects
only the selected entry using sample-mask output, before HAL. It does not
exercise a GPU draw; the device is nevertheless explicitly created at FL9_1.

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
unmodified-projection test separately verifies that core rejects three dynamic
uniform entries against the reported limit of two.

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
surface use. Synchronous window callbacks can still deadlock on reentry.

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
FL9_1 does not guarantee separate alpha blending: the production path rejects
those modes with both RGB and alpha writes. Some older full-channel blend tests
explicitly opt into a test-only RTX diagnostic and do not establish portability;
they can fail on hardware that lacks separate alpha support.

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

The private `private_device_creates_restricted_texture_views` test also passed
on the RTX FL9_1 device with a metadata-only copy texture view and rejected
sampling, attachment use and invalid descriptors. The controlled core fixture
verified 12 upload/copy/readback pixels each for RGBA8 and BGRA8 copy-only views
and rejected incompatible view usages. Neither result establishes historical
driver behavior or general public adapter support.

### FL9_1-required sampled/upload formats

Microsoft lists R8_UNORM (legacy L8), R8G8_SNORM (legacy V8U8),
R8G8B8A8_SNORM (legacy Q8W8V8U8) and BC1_UNORM (legacy DXT1) as FL9_1 2D
shader-sample formats. The backend checks the selected device's actual
`TEXTURE2D | SHADER_SAMPLE` bits and supports only single-mip sampled/upload
textures. BC1 dimensions and updates must be aligned to complete 4×4 blocks.
Run the synthetic capability checks and strict hardware tests with:

```powershell
$env:WGPU_DX11_REQUIRE_HARDWARE = "1"
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 r8_unorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 rg8_snorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 rgba8_snorm -- --nocapture --test-threads=1
cargo test --offline -p wgpu-hal --lib --no-default-features --features dx11 bc1 -- --nocapture --test-threads=1
```

The CPU tests cover legal and illegal usages, shape limits and missing or
insufficient driver-query results. All three hardware tests request and assert
FL9_1, upload padded 5×4 patterns from nonzero source offsets, sample into a
10×8 RGBA8 target and compare every pixel. R8_UNORM uses offset 1 and 7-byte
rows, R8G8_SNORM offset 2 and 12-byte rows, and R8G8B8A8_SNORM offset 4 and
24-byte rows. SNORM tests remap signed channels to UNORM and allow one code
value for conversion rounding. On the RTX 3090 Ti, R8_UNORM, R8G8_SNORM and
R8G8B8A8_SNORM returned masks `0x0002d3e0`, `0x00021320` and `0x000213a0`;
all reported filtered-sampling support, while the tests use nearest filtering.
The BC1 test uploads an 8×8 block-compressed image from offset 8 with a 24-byte
block-row pitch, replaces the block at [4,0] with a second padded upload, and
checks all 256 pixels after sampling. Its format mask was `0x000213a0` and
filtered sampling was reported. The uncompressed sampled/upload-only formats
and BC1 do not support render-target, copy-source, readback or mip-chain
operations in this private subset. R8_UNORM linear filtering lacks visual
execution coverage.

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
flag before calling the D3D11 HAL. Comparison samplers remain unsupported;
historical drivers remain unverified, and public surface capabilities remain
unavailable.

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
optional full-NPOT query does not imply that this backend implements NPOT mip
chains. The CPU test covers D1/D2/D3 dimensions and the D2 array-layer case.

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
views must include the smallest resource mip; NPOT mip chains, same-subresource copies and
general mip sampling remain unsupported. Historical FL9 GPU behavior is
unverified.

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
remain unsupported;
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
integer-indexed arrays remain rejected. The remaining hardware tests verify
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

The unsafe, nondefault fixture injects one explicitly selected hardware FL9_1
device into core separately from ordinary adapter enumeration. The test uses
core buffer upload/copy and a WGSL offscreen triangle, submission, fence wait,
mapped readback and explicit teardown. It checks all 64 copied bytes and all
64 rendered pixels, including the 32-red/32-green render. It also validates
unsupported transient formats and usages and creates transient textures and
views; it does not execute a transient render pass through core. Its restricted
numeric limits and format capabilities do not constitute a safe general adapter
contract. The fixture verifies that core's format-feature query preserves the
HAL's RGBA8, D24S8, BC1-only and unsupported-BC results. It also verifies core
rejects asymmetric stencil state with the missing
`TWO_SIDED_STENCIL` downlevel flag before invoking the FL9_1 HAL. Full-channel
straight-alpha blending is rejected with `SEPARATE_ALPHA_BLEND`, while the
same blend with alpha writes masked creates through the existing HAL path.

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
zero native `SampleMask`, so the backend passes `0xffff_fffe`; with one active
sample this retains the zero-mask result. Multisampling and alpha-to-coverage
remain unsupported. See [OMSetBlendState FL9_1 restrictions](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context)
and [WebGPU multisample validation](https://www.w3.org/TR/webgpu/#validating-gpumultisamplestate).

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
