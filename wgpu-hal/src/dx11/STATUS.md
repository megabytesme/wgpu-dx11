# Direct3D 11 backend status

The `dx11` backend is experimental. Normal adapter enumeration returns no
adapters: the current implementation does not meet wgpu's general device
contract, and its restrictions cannot all be expressed as wgpu limits.
Feature level 9_1 is a target for restricted compatibility work; it is not
evidence of full WebGPU conformance. See [CAPABILITIES.md](CAPABILITIES.md)
for the internal capability model, FL9_1 inventory and validation gaps.

Core gates multi-mip depth/stencil attachment textures on
`DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP`. Microsoft's [10Level9 device
rules](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-device)
require a single mip for depth/stencil-bound resources at FL9_1 through FL9_3;
the controlled FL9_1 core fixture verifies the descriptor is rejected before
the private HAL. D24S8 remains single-mip in the private implementation, and
normal adapter enumeration remains disabled.

Core also models the vertex-stage sampled-texture limit separately from the
shared per-stage texture limit. The controlled FL9_1 fixture reports zero and
checks the selected shader entry point before pipeline creation reaches HAL;
the strict fixture confirms the rejection is a core validation error. This
prevents unsupported D3D11 vertex texture access from becoming a HAL device
error if a future adapter reports the current private shader subset.

## Implemented private HAL paths

- DXGI adapter discovery, D3D11 device/context ownership, and restricted
  BGRA8 FIFO surfaces for WinRT CoreWindow and borrowed Win32 HWND handles.
  A strict hidden HWND clone-sampling test on the RTX at FL9_1 read BGRA
  `[0, 0, 255, 255]`, returned `Ok` from present and resized with the completed
  command buffer retained. Visible public presentation and surface concurrency
  remain unverified; synchronous window callbacks can deadlock on reentry.
- Buffer copies, initialization clears, fences, queue submission, and
  explicitly synchronized mapped CPU shadows, including transient staging.
  Core-internal storage usage bits can accompany a supported transfer buffer,
  but no storage-buffer shader binding is implemented. Core requires the
  relevant stage downlevel flags and the controlled fixture confirms unsupported
  vertex, fragment and compute storage layouts are rejected before HAL.
- Texture resources and views remain 2D-only. Core rejects D1/D3 binding
  layouts when the corresponding limits are zero, and requires explicit
  downlevel flags for 2D-array, cube and cube-array binding layouts and views;
  storage-texture array layouts use the same array gate. A one-layer 2D-array
  view is rejected before reaching the HAL. Core also gates NPOT mip chains
  with `NON_POWER_OF_TWO_MIPMAPPED_TEXTURES`; D3D11 leaves it absent because
  this HAL implements POT-only mip chains regardless of optional full-NPOT
  driver support.
- RGBA8 and BGRA8 sampled/color textures, texture views, bounded staging
  transfers, zero initialization and GPU texture-region copies, and point/linear,
  clamp/repeat/mirror samplers with anisotropy
  up to 2. On 9_1/9_2, equal min/mag filters with anisotropy 1 allow
  otherwise-unrepresentable LOD ranges to be normalized for single-mip views;
  other translated finite maximum LODs are checked against each used view's mip
  count. Nearest and linear mipmap filters have patterned GPU coverage;
  explicit-LOD sampling and
  historical driver behavior remain unverified. The current FL9_1 shader
  profiles, including on higher-level devices, accept finite literal texture
  biases in [-3, 3] without an offset. All sampling pairs used by a selected
  entry containing bias require equal min/mag filters and anisotropy 1; dynamic,
  out-of-range and helper-argument biases remain rejected. Nine strict GPU
  executions checked 64 pixels each, including linear mip blending; direct and
  indexed draws rejected incompatible samplers.
- R8_UNORM is supported for single-mip, single-layer 2D sampled textures with
  RESOURCE and/or COPY_DST usage, subject to a positive device format query.
  A strict forced-FL9_1 test verifies a padded upload from a nonzero buffer
  offset and all 80 sampled output pixels. Render-target, copy-source, readback
  and mip-chain uses remain unsupported.
- R8G8_SNORM has the same queried one-mip sampled/upload subset. A strict
  forced-FL9_1 test verifies padded signed two-channel data through GPU
  sampling and 80-pixel RGBA8 readback (one-code UNORM conversion tolerance).
  Render-target, copy-source, readback and mip-chain uses remain unsupported.
- R8G8B8A8_SNORM has the same queried one-mip sampled/upload subset. A strict
  forced-FL9_1 test verifies padded signed four-channel data through GPU
  sampling and 80-pixel RGBA8 readback (one-code UNORM conversion tolerance).
  Render-target, copy-source, readback and mip-chain uses remain unsupported.
- BC1_UNORM has a queried one-mip, one-layer sampled/upload subset. Texture
  dimensions and upload regions must align to 4×4 blocks; rows are measured in
  eight-byte blocks. A strict forced-FL9_1 test uploads a padded 8×8 pattern,
  replaces one block through a second upload, then validates all 256 rendered
  pixels. Copy-source, readback, render-target, mip-chain and sRGB view uses
  remain unsupported.
- Copy-only color texture views can retain metadata and texture ownership without
  creating native sampled or attachment views. They cannot be used for sampling
  or rendering; core upload, copy and readback were verified for both formats.
- POT RGBA8 and BGRA8 copy-only textures support multiple initialized mips when
  the device reports the required format bits. Strict RTX FL9_1 tests exercised
  four levels, partial uploads, cross-resource copies, readback, metadata views
  and resource retention; a core fixture checked all 43 mip-chain pixels.
- POT RGBA8 and BGRA8 mip chains also accept logical `COPY_DST` alone. Strict
  RTX tests uploaded three mips and verified an untouched zero mip through a
  test-only native GPU-to-GPU copy to a separate readable render-target texture.
  Core submitted three writes, waited for completion and rejected forbidden
  view/readback uses; it did not read back the upload-only texture. Historical
  FL9 drivers remain unverified, and public enumeration remains disabled.
- Copies between distinct mips of one supported texture use native DEFAULT to
  DEFAULT `CopySubresourceRegion` with an explicit source box. Strict RTX tests
  checked partial copies in both directions with full padded readback for RGBA8
  and BGRA8; render-to-copy-to-clone-sample cases each produced 64 blue pixels.
  Core checked all mip texels for both formats and rejected a same-mip copy.
  The earlier strict baseline rejected a valid distinct-mip copy. Copies within
  one subresource remain unsupported; historical drivers remain unverified.
- Sampled POT mip chains support `COPY_DST | RESOURCE` with optional `COPY_SRC`,
  or `RESOURCE | COLOR_TARGET` with optional copy usages,
  and a positively queried format. Suffix views including the resource's last
  mip use native SRVs; supported non-suffix sampled views use per-draw GPU
  copies into a clone and retain the original for feedback validation. Selected
  shader pairs are checked against actual mip counts and sampler
  LOD translation. RGBA8 and BGRA8
  GPU readbacks and a controlled core render passed; same-pass transitions
  retained sampled resources after caller destruction before submission. Strict
  suffix-view tests passed on the RTX FL9_1 device, including 64 blue pixels
  through the controlled core fixture. Without `COPY_SRC`, core cases sampled
  an uploaded blue mip and an untouched zero mip, each across 64 pixels; direct
  readback from that shader-resource-only texture is unavailable. Sixteen
  strict clone-backed GPU cases passed across both formats, including a recorded
  upload followed by an indexed draw in the same submission; the controlled core
  fixture checked 64 yellow pixels after a source update. The clone incurs
  per-draw copies and storage. NPOT mip chains and same-subresource copies remain
  unsupported. Native FL9 sampled views must include the smallest resource mip;
  historical GPU behavior is unverified.
- POT RGBA8 and BGRA8 color-attachment mip chains support an individual
  single-mip render-target view, with optional sampled, copy-source and
  copy-destination usage. A dual sampled/attachment view can select any single
  mip; a nonfinal mip uses a clone SRV and original-resource RTV. Transient
  usage remains unsupported. Strict
  GPU readback passed for both formats; core checked all 43 pixels per format
  and submitted a render-only mip pass without direct readback. Combined
  render/sample transitions passed six private GPU cases across both formats and
  four core cases with 64 checked pixels each. An additional eight private GPU
  cases covered nonfinal dual views, and default-view core cases checked 64
  pixels per format. Native suffix and clone-backed SRVs may sample disjoint
  mips of the same texture in the attachment pass; overlapping ranges remain
  rejected. Clone refresh snapshots and unbinds the RTV/DSV, copies, restores
  the attachments and rebinds pixel shader resources. Six strict RTX FL9_1
  native cases and two clone-backed cases passed across both formats; the latter
  and matching controlled core cases each produced 32 green and 32 blue pixels,
  demonstrating depth preservation. [D3D11.3 §5.3.1](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm)
  permits disjoint subresource input/output; the [D3D9 sampler documentation](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/dx9-graphics-reference-asm-ps-registers-sampler)
  warns against simultaneous texture and render-target binding. Historical FL9
  drivers remain unverified, and public adapter enumeration remains disabled.
- A restricted D24S8 attachment path provides zero initialization, independent
  depth/stencil clears, depth-only draws and all seven native depth comparisons
  other than Never, with optional writes. A strict FL9_1 test checked both
  passing and failing depths for every comparison across 128 pixels. Never
  retains its existing validated-draw/stencil remapping. Equal front/back
  stencil states map native comparisons,
  operations and references; asymmetric states are rejected because FL9_1 does
  not support two-sided stencil. Core represents this restriction with
  `DownlevelFlags::TWO_SIDED_STENCIL`; the controlled fixture verifies an
  asymmetric pipeline is rejected before the HAL call. Never with active
  stencil uses Always plus remapped depth-fail operations and disabled
  color/depth writes; inert-stencil
  Never still suppresses validated draws. Replace/Equal writes and reference
  transitions are indirectly verified through strict GPU readback. All eight
  stencil operations now pass boundary-value checks when used on native pass,
  stencil-fail and depth-fail branches (24 full-image cases); the DSV is not
  directly readable through the HAL. Private read-only passes require
  load/store on both aspects, no depth or stencil writes and no sampled alias.
  Aspect-specific views and depth copies remain unsupported; broad public
  depth-stencil capability is still false.
- D24S8 triangle pipelines accept signed constant and finite slope-scaled depth
  bias with zero clamp. Core's controlled fixture verifies nonzero clamp is
  rejected by `DEPTH_BIAS_CLAMP` before HAL. Nonzero bias on points or solid
  lines, nonzero clamp and bias with a selected fragment-depth output are
  rejected. Five strict FL9_1 RTX
  constant-bias images and two slope-bias images each checked all 64 pixels;
  the controlled core fixture checks positive and negative constant bias. FXC
  rejected a direct-only zero-bias `frag_depth` entry with X4530, while a
  structured color-and-depth zero-bias entry compiled. FL9 historical bias
  precision remains unverified; public adapter enumeration remains disabled.
- Single-sample pipeline masks reach the D3D11 output merger. Because FL9_1
  rejects a zero native `SampleMask`, a zero wgpu mask is translated to
  `0xffff_fffe`, which masks sample zero while satisfying the native nonzero
  requirement. A strict controlled core render preserved all 64 clear-color
  pixels; multisampling and alpha-to-coverage remain unsupported.
- Transient color and D24S8 attachments use ordinary D3D11 textures; no
  memoryless allocation or memory saving is claimed. Transient color requires
  color-attachment usage, and transient depth requires depth-write usage; sampled
  and copy usages are rejected for these private textures and views.
- Negative and out-of-target viewports use a separately compiled vertex shader
  and a reserved constant buffer. If FXC cannot compile that variant, draws
  needing remapping are rejected; the original shader remains usable.
- Up to eight groups with thirteen static or dynamic uniform slots, eight sampled
  textures and sixteen samplers in total.
  Shader-aware pair validation and restricted
  Naga/FXC shaders.
- NonFiltering sampler layouts accept samplers with nearest minification,
  magnification and mipmap filtering; Filtering layouts also accept nearest.
  Sparse bindings and sampler class remain part of layout identity. Strict RTX
  tests checked two complete images including a layout transition, and a core
  fixture checked 64 pixels and rejected a linear sampler with
  `WrongSamplerFiltering`. The original strict baseline rejected the layout.
  Existing RGBA8/BGRA8 textures can use an unfilterable-float layout binding when
  each sampling pair used by the selected entry has a NonFiltering sampler.
  Two strict RTX images and a 64-pixel core case covered this subset; invalid
  filtering pairs were rejected, while unused incompatible pairs were allowed.
  No additional unfilterable formats or comparison samplers are supported.
  Core now rejects comparison sampler creation when the adapter omits
  `DownlevelFlags::COMPARISON_SAMPLERS`; the controlled FL9_1 fixture verifies
  rejection before HAL. Historical drivers remain unverified and public
  enumeration stays disabled.
- Sampled texture and sampler bindings accept `FRAGMENT` and
  `VERTEX | FRAGMENT` visibility, retained per binding in layout identity.
  Actual vertex-stage sampling remains rejected. Two strict RTX images covered
  fragment sampling and a layout transition; core checked 64 pixels. CPU tests
  cover stage mapping and selected-entry compiler guards. The baseline rejected
  the combined layout; historical FL9 drivers remain unverified.
- Short uniform ranges use a zero-padded native constant buffer and preserve the
  declared byte range, including non-multiples of 16. This stages through a CPU
  map and can stall; odd-sized GPU copies remain unverified on historical
  drivers. Runtime integer expressions and integer-containing uniforms or
  interfaces are conservatively rejected for selected FL9 entries in both
  checked and unchecked modes. Earlier RTX integer-shader results are
  superseded by explicit rejection tests with float-only GPU controls.
- The forced-FL9_1 private shader path executes static WGSL reads of vertex
  constant c254 and pixel constant c31. Changing the vertex value moves the
  triangle and changing the pixel value changes its color; readback distinguishes
  17 baseline green pixels and 4 shifted red pixels. Runtime integer-indexed
  uniform arrays remain rejected.
- Triangle-list and nonindexed triangle-strip passes with up to sixteen sparse
  vertex- or instance-rate streams, winding and face
  culling, bounds-checked scissors, per-channel color write masks, translated
  single-target blend factors/operations and splitting at FL9 draw limits.
  Runtime integer-based vector indexing is rejected in both
  shader modes; proven static literal indices remain supported. Stores through aggregates
  containing two-row matrices are conservatively rejected in both shader modes.
- The private vertex-input subset includes `Snorm16x2` and `Snorm16x4`, with
  driver-checked input-assembler support and four-byte attribute alignment.
  This does not add signed-normalized texture formats.
- Zero-stride vertex streams repeat a constant attribute element in direct,
  nonindexed strip and indexed triangle-list paths, including mixed streams and
  window or gather translation. A bound range must contain the full attribute
  extent. Historical FL9_1 devices and the negative-effective-index semantics
  when every stream has zero stride remain unverified.
- Exact premultiplied-alpha blending uses coupled `ONE` / `INV_SRC_ALPHA` add
  factors for RGB and alpha, including all color-write masks. Strict FL9_1
  tests checked six full images on linear RGBA8/BGRA8, including transitions;
  the controlled core fixture checked 64 pixels with a one-unit rounding
  tolerance. The unsafe fixture reports blendability only on a positive driver
  format query and cannot represent the restricted blend-state subset.
  sRGB formats and normal public adapter exposure remain unsupported. Historical
  FL9_1 driver behavior remains unverified.
- FL9_1 rejects straight alpha, inversion and vignette blending when both RGB and alpha
  are written because separate alpha is not guaranteed. Core now requires
  `DownlevelFlags::SEPARATE_ALPHA_BLEND` for distinct color/alpha equations when
  both RGB and alpha are written; RGB-only and alpha-only cases use coupled
  factors and remain accepted. A test-only opt-in retains nonportable
  diagnostics.
- Standard blend factors and all five blend operations now translate through
  one path, including per-channel constant blend factors. `SRC_ALPHA_SAT` and
  dual-source factors remain rejected for FL9_1. Command replay retains blend
  factor state across pipeline binds and updates it when `set_blend_constants`
  is recorded. The D3D11 blend descriptor mirrors its first target into the
  first four native slots to satisfy the FL9_1 `BlendEnable` rule while the
  backend binds only one render target. A forced-FL9_1 hardware draw exercised
  constant and inverse-constant factors with two factor values plus Add,
  Subtract, ReverseSubtract, Min and Max; all 64 readback pixels matched. The
  controlled core fixture matched the custom constant-add output across 64
  pixels. Other factor mappings are CPU-tested, not each independently
  rendered. This still does not establish historical-device behavior or public
  adapter safety.
- Nonindexed point-list draws support the restricted vertex- and instance-rate
  streams, including sparse slots, nonzero offsets and `first_instance`. FL9_1
  point batches rebase vertex bindings and contain at most 65,535 points.
  Strict RTX tests checked three full images, including 65,537 points across a
  batch boundary; a controlled core fixture checked 64 pixels. Indexed Uint16
  and Uint32 points use ordered nonindexed runs; GPU index staging and CPU
  mapping can stall, and scattered indices can require many draws. When all
  vertex-rate streams have zero stride, or only instance-rate streams are used,
  index values do not affect the output and staging is avoided after range
  validation. Twelve strict RTX images and two 64-pixel core renders covered
  indexed points. Historical FL9_1 drivers remain unverified.
- LineList and LineStrip use explicit D3D11 topologies. Direct list batches
  preserve pairs; direct strip batches overlap their endpoint. Indexed LineList
  accepts Uint16 and Uint32, maps GPU-authoritative indices at replay, preserves
  pairs while compacting bounded windows, and gathers a wide pair into two
  vertices when needed. It limits native calls to 131,070 indices (65,535
  lines), ignores an incomplete tail and discards the draw on any invalid fetch.
  Strict RTX readback checked direct and indexed line topology output in both
  index formats, restart-separated strips, an indexed wide-edge gather and
  out-of-range draw discard. The controlled core fixture checked indexed
  Uint16 and Uint32 LineList and LineStrip output, including markers formed by
  base-vertex adjustment. Indexed LineStrip emulates restart by splitting
  CPU-mapped resolved indices because native FL9 strip cuts are unavailable.
- Nonindexed strips preserve winding through overlapping, rebased batches.
  Indexed LineStrip splits resolved restart markers into compact line strips;
  indexed TriangleStrip expands resolved runs into winding-preserving triangle
  lists and uses bounded Uint16 windows or wide-triangle gathers. Native FL9
  strip-cut support is not assumed. Strict GPU and controlled-core tests cover
  both index formats; historical FL9_1 behavior remains unverified.
- Direct FL9_1 triangle-list batches are conservatively limited to 65,535
  vertices; indexed draw batching retains its separate limit.
- FL9_1 overlapping vertex attributes within one logical stream are assigned
  to separate native input slots that bind the same underlying buffer. This
  emulation is limited to the existing vertex formats and available native
  slots; historical FL9_1 devices remain unverified.
- Sparse multi-stream indexed triangle lists accept Uint16 and Uint32 indices.
  Replay checks GPU-written indices against every stream and discards a whole
  draw on an out-of-bounds fetch. Native-safe Uint16 indices use the original
  buffers; other valid indices use ordered rebased Uint16 windows or gather
  each stream into scratch buffers for wide triangles. Indexed LineStrip splits
  resolved restart markers and preserves connectivity through compact windows
  and wide-edge gathers. Indexed TriangleStrip splits the same resolved markers,
  expands runs to alternating-winding triangle-list indices, then uses bounded
  Uint16 windows or gathers wide triangles across every vertex stream. The
  private and controlled core paths test both index formats; FL9_1 historical
  hardware remains unverified.
- Restricted direct, strip and indexed instancing repeats non-instanced native
  draws, binding each instance-rate stream with native stride zero at its
  `first_instance`-adjusted offset. Pipelines using `instance_index` and indexed
  triangle draws with only instance-rate streams remain unsupported; general
  instancing is not advertised.
- Restricted index-dependent draws support signed base vertices and validate GPU
  index contents at replay. Uint32 triangle lists and Uint16 values beyond native
  limits convert to ordered native 16-bit windows, with wide triangles gathered
  through GPU vertex-buffer copies. Out-of-bounds vertex fetches discard the
  whole draw. Staging and scratch
  allocation can stall the CPU; this is not a full public draw contract.
- An original two-texture overlay shader passed RTX diagnostics with alpha and
  inversion blend states; full-channel separate alpha is not a guaranteed
  FL9_1 production configuration.
- A local, deliberately unsafe core bootstrap experiment with placeholder
  capabilities created a wgpu device, copied mapped buffers, and rendered an
  indexed triangle on FL9_1 with pixel readback. It is not an adapter or a
  capability-conformance test.

## Before a public adapter

- Build a reproducible, tightly scoped core-integration test without
  presenting placeholder capabilities as a usable adapter.
- Implement or honestly gate the general binding layouts, shader inputs,
  texture formats/dimensions/mips/arrays, sampler modes, render states and
  buffer usages required by valid wgpu applications. Several restrictions,
  including specific bind-group shapes, cannot be described by `Limits`.
- Complete indexed and instanced rendering, remaining depth/stencil paths and other
  operations needed by many applications. On 9_1 the native index format is
  restricted to 16-bit and index values to 65,534; combined vertex/index bindings
  on 9_1/9_2 need emulation. Additional topologies and shader-visible IDs still
  need implementation or validation for full wgpu semantics.
- Feature level 9_1 has no native compute shaders; applications relying on
  compute or storage need alternatives. The opt-in Naga texel-load/dimensions
  experiments remain isolated proofs and are not enabled in the HAL.
- Validate real device format/features, target-specific UWP imports, shader
  semantics, device loss, synchronization and presentation on each target.
  Test-only success must not be presented as public adapter support.

See [TESTING.md](TESTING.md) for the staged validation plan and hardware matrix.
