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
- Single-mip RGBA8 and BGRA8 sampled/color textures, texture views, bounded staging
  transfers, zero initialization and GPU texture-region copies, and point/linear,
  clamp/repeat/mirror samplers with anisotropy
  up to 2. On 9_1/9_2, translated finite maximum LODs require single-mip views.
- A restricted D24S8 attachment path provides zero initialization, independent
  depth/stencil clears, depth-only draws and Always/LessEqual depth tests with
  optional writes. Private read-only passes require load/store on both aspects,
  no depth writes and no sampled alias. Stencil tests, aspect-specific views
  and depth copies remain unsupported. Textured depth ordering is verified
  through GPU readback.
- Negative and out-of-target viewports use a separately compiled vertex shader
  and a reserved constant buffer. If FXC cannot compile that variant, draws
  needing remapping are rejected; the original shader remains usable.
- Up to eight groups with thirteen static or dynamic uniform slots, eight sampled
  textures and sixteen samplers in total.
  Shader-aware pair validation and restricted
  Naga/FXC shaders.
- Triangle-list and nonindexed triangle-strip passes with zero inputs or one interleaved vertex buffer, winding and face
  culling, bounds-checked scissors, per-channel color write masks,
  restricted replace/alpha/crosshair/additive/vignette blending, and splitting
  at FL9 draw limits. Checked shaders allow proven value-vector dynamic reads;
  dynamic vector-pointer indexing remains rejected. Stores through aggregates
  containing two-row matrices are conservatively rejected in both shader modes.
- FL9_1 rejects alpha, inversion and vignette blending when both RGB and alpha
  are written because separate alpha is not guaranteed. RGB-only and alpha-only
  cases use coupled factors; a test-only opt-in retains nonportable diagnostics.
- Nonindexed point-list draws are supported for zero-input pipelines; indexed
  points and point pipelines with vertex buffers remain unavailable.
- Nonindexed strips preserve winding through overlapping, rebased batches;
  indexed strips remain unsupported because native strip cuts are not guaranteed.
- Direct FL9_1 triangle-list batches are conservatively limited to 65,535
  vertices; indexed draw batching retains its separate limit.
- FL9_1 input layouts reject overlapping element byte ranges within a stream;
  ordinary core validation does not yet express this restriction.
- Direct and indexed instancing repeat native draws in instance order when the
  shader has no instance-dependent inputs. Nonzero first instance is ignored
  within this subset; general instancing is not advertised.
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
