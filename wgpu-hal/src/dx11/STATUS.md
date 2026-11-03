# Direct3D 11 backend status

The `dx11` backend is experimental. Normal adapter enumeration returns no
adapters: the current implementation does not meet wgpu's general device
contract, and its restrictions cannot all be expressed as wgpu limits.
Feature level 9_1 is a target for restricted compatibility work; it is not
evidence of full WebGPU conformance. See [CAPABILITIES.md](CAPABILITIES.md)
for the internal capability model, FL9_1 inventory and validation gaps.

## Implemented private HAL paths

- DXGI adapter discovery, D3D11 device/context ownership, and restricted
  BGRA8 FIFO surfaces for WinRT CoreWindow and borrowed Win32 HWND handles.
  The HWND path has hidden-window coverage; visible presentation and
  surface concurrency remain unverified.
- Buffer copies, initialization clears, fences, queue submission, and
  explicitly synchronized mapped CPU shadows, including transient staging.
- RGBA8 and BGRA8 sampled/color textures, texture views, bounded staging
  transfers, zero initialization and GPU texture-region copies, and point/linear,
  clamp/repeat/mirror samplers with anisotropy
  up to 2. On 9_1/9_2, equal min/mag filters with anisotropy 1 allow
  otherwise-unrepresentable LOD ranges to be normalized for single-mip views;
  other translated finite maximum LODs are checked against each used view's mip
  count. Nearest and linear mipmap filters have patterned GPU coverage;
  explicit-LOD sampling and
  historical driver behavior remain unverified. Reachable texture sample bias
  is rejected for the current FL9_1 shader profiles, including on higher-level
  devices, because the tested compiler emitted an unclamped legacy instruction
  with a narrower defined bias range.
- Copy-only color texture views can retain metadata and texture ownership without
  creating native sampled or attachment views. They cannot be used for sampling
  or rendering; core upload, copy and readback were verified for both formats.
- POT RGBA8 and BGRA8 copy-only textures support multiple initialized mips when
  the device reports the required format bits. Strict RTX FL9_1 tests exercised
  four levels, partial uploads, cross-resource copies, readback, metadata views
  and resource retention; a core fixture checked all 43 mip-chain pixels.
- Sampled POT mip chains support `COPY_DST | RESOURCE` with optional `COPY_SRC`,
  or `RESOURCE | COLOR_TARGET` with optional copy usages,
  a positively queried format and a suffix view including the resource's last
  mip. Selected shader pairs are checked against actual mip counts and sampler
  LOD translation. RGBA8 and BGRA8
  GPU readbacks and a controlled core render passed; same-pass transitions
  retained sampled resources after caller destruction before submission. Strict
  suffix-view tests passed on the RTX FL9_1 device, including 64 blue pixels
  through the controlled core fixture. Without `COPY_SRC`, core cases sampled
  an uploaded blue mip and an untouched zero mip, each across 64 pixels; direct
  readback from that shader-resource-only texture is unavailable. Non-suffix
  sampled views, NPOT mip chains and self-copies remain unsupported.
  FL9 sampled views must include the smallest resource mip, and
  historical GPU behavior is unverified.
- POT RGBA8 and BGRA8 color-attachment mip chains support an individual
  single-mip render-target view, with optional sampled, copy-source and
  copy-destination usage. A dual sampled/attachment view can select only the
  last mip; transient usage remains unsupported. Strict
  GPU readback passed for both formats; core checked all 43 pixels per format
  and submitted a render-only mip pass without direct readback. Combined
  render/sample transitions passed six private GPU cases across both formats and
  four core cases with 64 checked pixels each. Whole-resource feedback in the
  same pass remains rejected, even across disjoint mips.
- A restricted D24S8 attachment path provides zero initialization, independent
  depth/stencil clears, depth-only draws and Always/LessEqual depth tests with
  optional writes. Private read-only passes require load/store on both aspects,
  no depth writes and no sampled alias. Stencil configurations that always
  pass and cannot write are accepted as no-ops; nontrivial stencil tests or
  writes, aspect-specific views and depth copies remain unsupported. Textured
  depth ordering is verified through GPU readback.
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
- Short uniform ranges use a zero-padded native constant buffer and preserve the
  declared byte range, including non-multiples of 16. This stages through a CPU
  map and can stall; odd-sized GPU copies remain unverified on historical
  drivers. Selected integer uniform bitcasts and a precision-sensitive fragment
  comparison have GPU regression coverage, not general integer shader coverage.
- Triangle-list and nonindexed triangle-strip passes with up to sixteen sparse
  vertex- or instance-rate streams, winding and face
  culling, bounds-checked scissors, per-channel color write masks,
  restricted replace/alpha/crosshair/additive/vignette blending, and splitting
  at FL9 draw limits. Checked shaders allow proven value-vector dynamic reads;
  dynamic vector-pointer indexing remains rejected. Stores through aggregates
  containing two-row matrices are conservatively rejected in both shader modes.
- The private vertex-input subset includes `Snorm16x2` and `Snorm16x4`, with
  driver-checked input-assembler support and four-byte attribute alignment.
  This does not add signed-normalized texture formats.
- Zero-stride vertex streams repeat a constant attribute element in direct,
  nonindexed strip and indexed triangle-list paths, including mixed streams and
  window or gather translation. A bound range must contain the full attribute
  extent. Historical FL9_1 devices and the negative-effective-index semantics
  when every stream has zero stride remain unverified.
- FL9_1 rejects alpha, inversion and vignette blending when both RGB and alpha
  are written because separate alpha is not guaranteed. RGB-only and alpha-only
  cases use coupled factors; a test-only opt-in retains nonportable diagnostics.
- Nonindexed point-list draws are supported for zero-input pipelines; indexed
  points and point pipelines with vertex buffers remain unavailable.
- Nonindexed strips preserve winding through overlapping, rebased batches;
  indexed strips remain unsupported because native strip cuts are not guaranteed.
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
  each stream into scratch buffers for wide triangles. Indexed strips remain
  unsupported.
- Restricted direct, strip and indexed instancing repeats non-instanced native
  draws, binding each instance-rate stream with native stride zero at its
  `first_instance`-adjusted offset. Pipelines using `instance_index` and indexed
  draws with only instance-rate streams remain unsupported; general instancing
  is not advertised.
- Restricted indexed draws support signed base vertices and validate GPU index
  contents at replay. Uint32 triangle lists and Uint16 values beyond native
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
