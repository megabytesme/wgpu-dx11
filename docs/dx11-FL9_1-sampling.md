# FL9_1 Repeat and MirrorRepeat

Reviewed baseline `2865e800840ea956684ce682a50ed16d46271b91`. Core currently accepts a reflected fragment sampling pair when it proves bounded coordinates for direct, non-arrayed 2D float sampling. The proof covers literal coordinates in `[-63, 63]` and rejects gradients, gathers, offsets and comparisons. The binder checks every referenced periodic sampler at draw and render-bundle validation. It independently checks every addressing axis for WRAP when a sampled 2D texture is conditional NPOT. The sampler and original coordinate go to the native sampling instruction, preserving its taps and implicit LOD. [Coordinate proof](../wgpu-core/src/validation.rs) · [Draw checks](../wgpu-core/src/command/bind.rs) · [Prior device evidence](dx11-FL9_1-full-certification.md)

Microsoft specifies 128 texture repeats per primitive at FL9_1. Conditional NPOT 2D allows one mip and prohibits WRAP on any sampler axis. MIRROR remains a distinct native mode. An affirmative `FullNonPow2TextureSupport` query lifts the conditional NPOT constraint on the selected device. The repeat cap still applies. [Native draw limits](https://learn.microsoft.com/en-us/windows/win32/direct3d11/d3d11-graphics-reference-10level9-context) · [Feature table footnote 3](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-devices-downlevel-intro) · [D3D9_OPTIONS](https://learn.microsoft.com/en-us/windows/win32/api/d3d11/ns-d3d11-d3d11_feature_data_d3d9_options)

## Discriminator

A fold before implicit sampling changes the inputs to LOD calculation. WRAP applies to individual texel addresses during filtering, so `frac` followed by CLAMP also changes linear edge taps. The triangle wave for MIRROR is continuous, but discrete derivatives cancel in a quad that straddles the reversal symmetrically. WebGPU supports independent address modes, min/mag filters, mip filter and LOD clamp, including implicit LOD from texture-coordinate derivatives. [WebGPU samplers](https://www.w3.org/TR/webgpu/#samplers) · [D3D texture address processing](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm#TextureAddressProcessing) · [D3D LOD calculation](https://microsoft.github.io/DirectX-Specs/d3d/archive/D3D11_3_FunctionalSpec.htm#LODCalculation)

The [fixture](../deliverables/dx11-levels/FL9_1/SamplingDiscriminator.hlsl) and [runner](../deliverables/dx11-levels/FL9_1/Test-SamplingDiscriminator.ps1) reproduce the compiler result and the analytical reference. Run on Ben-Desktop from the wgpu worktree with Windows PowerShell 5.1. The runner cleans up compiler outputs after reading the legacy disassembly.

```powershell
$fxc = 'C:/Program Files (x86)/Windows Kits/10/bin/10.0.26100.0/x64/fxc.exe'
powershell.exe -NoProfile -File deliverables/dx11-levels/FL9_1/Test-SamplingDiscriminator.ps1 -FxcPath $fxc
```

Both x64 and x86 FXC `10.0.26100.9457` matched all 18 expected compiler outcomes and the CPU reference. The x64 binary hash is `3c1d8f3aa0d1c970732f14d61d3a4b5dfa8ca4b7c81d567643f39f6cef412010`. The x86 binary hash is `eba158a54af629d29f124c91e8c1fd8d9082cc7dec1701c00188bd41b3c20511`. Both runs used HLSL with LF line endings, SHA-256 `9189aec21ca5f16471f84a749c558f63018f4a425cfbf0ca97c4b56fa67d3ba4`.

| Pixel entry point | `ps_4_0_level_9_1` result | `ps_5_0` control |
| --- | --- | --- |
| `native_sample` | Compiles, 1 texture and 1 arithmetic instruction | Compiles |
| `uniform_rebase` | Compiles, 1 texture and 2 arithmetic instructions | Compiles |
| `repeat_folded` | Compiles, 1 texture and 2 arithmetic instructions | Compiles |
| `mirror_folded` | Compiles, 1 texture and 6 arithmetic instructions | Compiles |
| `explicit_derivatives`, `gradients_after_repeat_fold` | X4532 | Compiles |
| `explicit_gradients` | X5190 even with constant gradients | Compiles |
| `explicit_lod`, `explicit_texel` | X4532 | Compiles |

The readback inputs below use a 2×2 RGBA8Unorm target, one fullscreen triangle, no blending, and a 16×16 RGBA8Unorm source. Use clip-space vertices `(-1,1)`, `(3,1)`, `(-1,-3)` with `z=0.5`, `w=1`, and pass the UV through the vertex shader. For intended left and right pixel-center U values `L` and `R`, assign the three vertex U values `L-(R-L)/2`, `L+3.5*(R-L)`, `L-(R-L)/2`. Use V=0.5 and CLAMP on V and W. Each draw covers both rows, and both pixels in a row are in the same interior 2×2 derivative quad.

| Case | Source and sampler | U at pixel centers | Native reference | Folded result predicted by the CPU oracle |
| --- | --- | --- | --- | --- |
| Repeat linear seam | One mip, red column 0, blue column 15, equal colors down each column. Linear min/mag, WRAP U. | `1`, `1` | RGB approximately `[128, 0, 128]` | `repeat_folded` plus CLAMP gives `[255, 0, 0]`. |
| Repeat derivative | Five mips, each constant and a different color. Point min/mag/mip, WRAP U, LOD clamp 0 through 4. | `63/64`, `65/64` | Footprint 0.5, mip 0 | `repeat_folded` plus WRAP gives footprint 15.5, mip 4. |
| Mirror derivative | Same five-mip setup with MIRROR U. | `15/16`, `17/16` | Footprint 2, mip 1 | `mirror_folded` plus CLAMP gives footprint 0, mip 0. |

For a hardware run, assign mip colors red, green, blue, yellow and magenta in order. Read back all four pixels. Use exact colors for point-sampled mips and a tolerance of two UNORM units for the linear seam. Repeat the three native-reference draws with U shifted by `+256` and `-256`, `uniform_rebase`, and the corresponding even offset in `b0`. The candidate in that control rebases before the native sample. Request and assert exact feature level `0x9100`, record adapter identity and the NPOT query, and keep forced-tier and true-ceiling observations separate. The compiler and CPU oracle run supplies no GPU readback.

## Bounded extension

Primitive- or draw-uniform translation by two periods at a time preserves either periodic mode. It is an option when an entry point owns the input or can prove the actual sample coordinates. Preserve every nonperiodic coordinate, leave native sampling intact, and require the translated coordinate interval to fit a conservative subset of the native cap. Where possible, perform the translation before interpolation so a large unneeded coordinate offset cannot consume interpolator precision. Verify bounds for the actual rasterization and sample paths, including clipped primitives, helper lanes and floating-point interpolation error. For a first typed extension, use immutable owned vertices, a fixed affine UV varying, constant positive clip W, bounded screen-space slopes, and a guard band for derivative helpers. Reject inputs outside that contract, unsupported view/sampler combinations and unknown conditional-NPOT WRAP support before issuing the draw. Splitting an ordinary draw or changing vertex/index streams can change primitive IDs, clipping and ordering, so keep those transformations out of the transparent path.

For ordinary wgpu, any expanded static proof should accept the user-authored value only when every lane, including helper lanes, remains in the interval. Use the current pipeline and binding validation paths, and retain the source-view and selected-sampler checks for accepted pairs in direct and bundle draws. Shader reflection cannot derive a conservative input interval from arbitrary vertex buffers, uniforms, sampled data or runtime math. Binding a periodic sampler also cannot justify changing clamp samples through the same pipeline and layout.

Single-mip uncompressed 2D float sampling with equal min/mag filters and anisotropy 1 leaves room for separate emulation. Mask the periodic axes, resolve taps with the selected address mode, and keep texture-view dimensions and filtering checks bound to the actual pair. Nearest and periodic bilinear taps require distinct controls, including odd NPOT extents, negative coordinates, both axes and seam ties. Even at one mip, differing min/mag filters can expose derivative changes. Anisotropy can expose tap changes independently of mip selection. The existing single-mip Grad compatibility path uses draw-time checks on both view and sampler and does not establish general explicit gradient support. [Existing single-mip compatibility checks](../wgpu-core/src/command/bind.rs)

Replicated textures or per-mip auxiliary views require a budget for allocations, updates, views and shader bindings. A periodic 2×2 duplicate of a legal 2048×2048 texture exceeds the physical extent and quadruples pixel storage. Per-level padding also changes mip geometry and normalized scale unless the transformed view preserves the per-level mapping. Manual taps or shader duplication compete with FL9_1's 32 texture instructions, 64 arithmetic instructions, 12 temporary registers and four levels of dependent reads. A path must reserve its instruction and binding costs or reject before execution. Existing observable texture writes, clears, copies, sRGB views, rebinding and command ordering must be reflected in any derived resources. [Native resource limits](https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-resources-limits) · [FL9_1 profile limits](https://learn.microsoft.com/en-us/windows/win32/direct3dhlsl/specifying-compiler-targets)

General arbitrary-coordinate Repeat and MirrorRepeat remain open on FL9_1. The discriminator establishes why the tested fold and direct shader-instruction strategies cannot provide them. Keep the finite native limit and the proof check while evaluating narrower, independently verified paths.
