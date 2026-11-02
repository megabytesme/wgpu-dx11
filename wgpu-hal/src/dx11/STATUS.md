# Direct3D 11 backend status

The `dx11` backend is experimental. Normal adapter enumeration returns no
adapters: the current implementation does not meet wgpu's general device
contract, and its restrictions cannot all be expressed as wgpu limits.
Feature level 9_1 is a target for Horizon-specific compatibility work; it is
not evidence of full WebGPU conformance.

## Implemented private HAL paths

- DXGI adapter discovery, D3D11 device/context ownership, and a WinRT
  CoreWindow surface with restricted BGRA8 FIFO presentation.
- Buffer copies, initialization clears, fences, queue submission, and
  explicitly synchronized mapped CPU shadows, including transient staging.
- Single-mip RGBA8 sampled/color textures, texture views, bounded staging
  transfers, and restricted filtering samplers. On 9_1/9_2, translated
  finite maximum LODs require validated single-mip views.
- Empty, one-uniform, one-texture/sampler, and the Horizon HUD-shaped
  uniform/two-texture/two-sampler bind groups; restricted Naga/FXC shaders.
- Triangle-list passes with one interleaved vertex buffer, bounds-checked
  scissors, replace/alpha/crosshair/additive/vignette blending, and splitting
  at FL9 draw limits. Checked uniform shaders currently reject reachable
  dynamic indexing.
- Restricted 16-bit, zero-base indexed draws validate GPU index contents at
  replay and preserve draw order. The staging readback can stall the CPU.
- The real Horizon HUD and crosshair shaders have passed FL9_1 GPU readback;
  the HUD path has also passed an isolated x64 UWP surface diagnostic.

## Before a public adapter

- Exercise actual wgpu-core device creation and its late initialization; the
  matching private HAL startup operations have GPU tests but core integration
  has not yet been demonstrated.
- Implement or honestly gate the general binding layouts, shader inputs,
  texture formats/dimensions/mips/arrays, sampler modes, render states and
  buffer usages required by valid wgpu applications. Several restrictions,
  including specific bind-group shapes, cannot be described by `Limits`.
- Complete indexed and instanced rendering, depth/stencil paths and other
  operations needed by Horizon. On 9_1 the native index format is restricted
  to 16-bit and index values to 65,534; combined vertex/index native bindings
  on 9_1/9_2 need emulation. Other index formats, nonzero base vertices and
  out-of-range indices currently fail rather than providing full wgpu semantics.
- Replace or adapt Horizon compute/storage-dependent paths for 9_1, which has
  no native compute shaders. The opt-in Naga texel-load/dimensions experiments
  remain isolated proofs and are not enabled in the HAL.
- Validate real device format/features, target-specific UWP imports, shader
  semantics, device loss, synchronization and presentation on each target.
  Test-only success must not be presented as public adapter support.
