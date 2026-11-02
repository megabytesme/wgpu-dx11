# FL9 texel-load experiment

The opt-in Naga HLSL option `fl9_texel_load_proof` is a restricted experiment,
not a working wgpu backend. It replaces a supported WGSL `textureLoad` with
nearest sampling at the center of the selected texel and returns zero for
coordinates or mip levels outside the one-mip view. The option defaults to
`None`; existing HLSL generation remains unchanged.

The caller must bind the selected single-mip 2D float texture at `t0`, a point
clamp sampler at `s15`, and a 16-byte constant buffer at `b13` containing
`[width, height, 1 / width, 1 / height]`. Dimensions must be nonzero and valid
for feature level 9. The tested resource is RGBA8. A vertex shader supplying a
location-0 varying must link compatibly with the fragment shader.

The proof accepts only the selected fragment entry point, an optional single
location-0 `vec2<f32>` input, one directly referenced bound 2D texture, and
straight-line code. Other bound resources, arrays, helper functions, control
flow, explicit texel-load sample/array indices and non-fragment entry points
are rejected. The host is responsible for validating the view, dimensions,
sampler and reserved slots before opting in. It is not enabled in the DX11 HAL.

From the wgpu repository root, on a Windows machine with an FL9_1 D3D11 device:

```powershell
cargo +stable test -p wgpu-hal --lib --no-default-features --features dx11 dx11::fl9_texel_proof
```

The GPU test compares different texels, varying coordinates, invalid mip and
coordinate bounds, and a non-power-of-two texture against expected pixels.
