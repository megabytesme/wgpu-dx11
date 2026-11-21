use super::*;
use windows::Win32::Graphics::Direct3D::{
    D3D_SRV_DIMENSION_TEXTURE2D, D3D_SRV_DIMENSION_TEXTURECUBEARRAY,
};

const LINEAR_PIXEL: &str = "
    Texture2D<float4> image : register(t0);
    SamplerState linear_sampler : register(s0);
    float4 main() : SV_Target { return image.SampleLevel(linear_sampler, float2(.5, .5), 0); }
";

fn view(device: &ID3D11Device, image: &ID3D11Texture2D) -> Result<ID3D11RenderTargetView> {
    let mut view = None;
    unsafe { device.CreateRenderTargetView(image, None, Some(&mut view))? };
    view.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn shader_view(device: &ID3D11Device, image: &ID3D11Texture2D) -> Result<ID3D11ShaderResourceView> {
    let mut view = None;
    unsafe { device.CreateShaderResourceView(image, None, Some(&mut view))? };
    view.ok_or_else(|| Error::from_hresult(E_FAIL))
}

fn sample_linear(
    device: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    vs: &[u8],
    ps: &[u8],
    image: ID3D11ShaderResourceView,
) -> Result<Vec<[u8; 4]>> {
    let vs = vertex_shader(device, vs)?;
    let ps = pixel_shader(device, ps)?;
    let mut sampler = None;
    unsafe {
        device.CreateSamplerState(
            &D3D11_SAMPLER_DESC {
                Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                ComparisonFunc: D3D11_COMPARISON_NEVER,
                MaxAnisotropy: 1,
                MaxLOD: f32::MAX,
                ..Default::default()
            },
            Some(&mut sampler),
        )?;
        let (target, rtv) = render_target(device, 1, 1);
        ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ctx.RSSetViewports(Some(&[viewport(1.0, 1.0)]));
        ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
        ctx.VSSetShader(&vs, None);
        ctx.PSSetShader(&ps, None);
        ctx.PSSetShaderResources(0, Some(&[Some(image)]));
        ctx.PSSetSamplers(0, Some(&[sampler]));
        ctx.Draw(3, 0);
        ctx.OMSetRenderTargets(None, None);
        read_pixels(device, ctx, &target, 1, 1)
    }
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_float32_format_linear_filtering() -> Result<()> {
    let exact = open(&hardware(), FeatureLevel::L10_1);
    let vs = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let ps = exact
        .compile_hlsl(LINEAR_PIXEL, "main", ShaderStage::Pixel)
        .unwrap();
    let cases = [
        (DXGI_FORMAT_R32_FLOAT, 1, "R32_FLOAT"),
        (DXGI_FORMAT_R32G32_FLOAT, 2, "R32G32_FLOAT"),
        (DXGI_FORMAT_R32G32B32A32_FLOAT, 4, "R32G32B32A32_FLOAT"),
    ];
    for (format, channels, name) in cases {
        let flags = exact.check_format_support(format)?;
        assert_ne!(flags & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32, 0);
        let source = [
            [0.125_f32, 0.0, 1.0, 0.25],
            [0.375, 0.125, 0.875, 0.5],
            [0.625, 0.375, 0.625, 0.75],
            [0.875, 0.5, 0.5, 1.0],
        ];
        let packed: Vec<_> = source
            .iter()
            .flat_map(|texel| &texel[..channels])
            .copied()
            .collect();
        let pixels = unsafe {
            exact.with_native(|device, ctx| {
                let image = texture(
                    device,
                    &D3D11_TEXTURE2D_DESC {
                        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                        ..texture_desc(format, 2, 2)
                    },
                    Some(&D3D11_SUBRESOURCE_DATA {
                        pSysMem: packed.as_ptr().cast(),
                        SysMemPitch: (channels * 2 * size_of::<f32>()) as u32,
                        SysMemSlicePitch: size_of_val(packed.as_slice()) as u32,
                    }),
                );
                sample_linear(device, ctx, &vs, &ps, shader_view(device, &image)?)
            })?
        };
        let mut expected = [0.0, 0.0, 0.0, 1.0];
        expected[..channels].copy_from_slice(&[0.5, 0.25, 0.75, 0.625][..channels]);
        std::println!("{name} CheckFormatSupport={flags:#010x} filtered center={pixels:?}");
        assert_unorm8(&pixels, &[expected]);
    }
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_depth_view_linear_filtering() -> Result<()> {
    let exact = open(&hardware(), FeatureLevel::L10_1);
    let vs = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let ps = exact
        .compile_hlsl(LINEAR_PIXEL, "main", ShaderStage::Pixel)
        .unwrap();
    let depths = [0.125_f32, 0.375, 0.625, 0.875];
    let d24 = depths.map(|depth| ((depth * 16_777_215.0).round() as u32) | (0x7f << 24));
    let d32 = depths.map(|depth| [depth.to_bits(), 0x7f]);
    let cases = [
        (
            DXGI_FORMAT_R24G8_TYPELESS,
            DXGI_FORMAT_R24_UNORM_X8_TYPELESS,
            d24.as_ptr().cast(),
            8,
            "R24_UNORM_X8_TYPELESS",
        ),
        (
            DXGI_FORMAT_R32G8X24_TYPELESS,
            DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS,
            d32.as_ptr().cast(),
            16,
            "R32_FLOAT_X8X24_TYPELESS",
        ),
    ];
    for (resource_format, view_format, data, pitch, name) in cases {
        let flags = exact.check_format_support(view_format)?;
        assert_ne!(
            flags & D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0 as u32,
            0,
            "{name}"
        );
        let pixels = unsafe {
            exact.with_native(|device, ctx| {
                let image = texture(
                    device,
                    &D3D11_TEXTURE2D_DESC {
                        BindFlags: (D3D11_BIND_DEPTH_STENCIL | D3D11_BIND_SHADER_RESOURCE).0 as u32,
                        ..texture_desc(resource_format, 2, 2)
                    },
                    Some(&D3D11_SUBRESOURCE_DATA {
                        pSysMem: data,
                        SysMemPitch: pitch,
                        SysMemSlicePitch: pitch * 2,
                    }),
                );
                let mut view = None;
                device.CreateShaderResourceView(
                    &image,
                    Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                        Format: view_format,
                        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
                        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                            Texture2D: D3D11_TEX2D_SRV {
                                MostDetailedMip: 0,
                                MipLevels: 1,
                            },
                        },
                    }),
                    Some(&mut view),
                )?;
                sample_linear(device, ctx, &vs, &ps, view.unwrap())
            })?
        };
        std::println!("{name} CheckFormatSupport={flags:#010x} filtered center={pixels:?}");
        assert_unorm8(&pixels, &[[0.5, 0.0, 0.0, 1.0]]);
    }
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_normalized_format_blending() -> Result<()> {
    let exact = open(&hardware(), FeatureLevel::L10_1);
    let vs = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let source_ps = exact
        .compile_hlsl(
            "float4 main() : SV_Target { return float4(.5, .5, .5, .5); }",
            "main",
            ShaderStage::Pixel,
        )
        .unwrap();
    let read_ps = exact
        .compile_hlsl(
            "Texture2D<float4> image : register(t0);
         float4 main() : SV_Target {
             float4 raw = image.Load(int3(0, 0, 0));
             return float4(raw.rgb * .5 + .5, raw.a);
         }",
            "main",
            ShaderStage::Pixel,
        )
        .unwrap();
    let cases = [
        (DXGI_FORMAT_R8_SNORM, 1, true, "R8_SNORM"),
        (DXGI_FORMAT_R8G8_SNORM, 2, true, "R8G8_SNORM"),
        (DXGI_FORMAT_R8G8B8A8_SNORM, 4, true, "R8G8B8A8_SNORM"),
        (DXGI_FORMAT_R16_SNORM, 1, true, "R16_SNORM"),
        (DXGI_FORMAT_R16G16_SNORM, 2, true, "R16G16_SNORM"),
        (
            DXGI_FORMAT_R16G16B16A16_SNORM,
            4,
            true,
            "R16G16B16A16_SNORM",
        ),
        (DXGI_FORMAT_R16_UNORM, 1, false, "R16_UNORM"),
        (DXGI_FORMAT_R16G16_UNORM, 2, false, "R16G16_UNORM"),
        (
            DXGI_FORMAT_R16G16B16A16_UNORM,
            4,
            false,
            "R16G16B16A16_UNORM",
        ),
    ];
    for (format, channels, signed, name) in cases {
        let flags = exact.check_format_support(format)?;
        assert_ne!(flags & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32, 0, "{name}");
        let clear = if signed { -1.0 } else { 0.25 };
        let pixels = unsafe {
            exact.with_native(|device, ctx| {
                let vs = vertex_shader(device, &vs)?;
                let source_ps = pixel_shader(device, &source_ps)?;
                let read_ps = pixel_shader(device, &read_ps)?;
                let image = texture(
                    device,
                    &D3D11_TEXTURE2D_DESC {
                        BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_RENDER_TARGET).0 as u32,
                        ..texture_desc(format, 1, 1)
                    },
                    None,
                );
                let rtv = view(device, &image)?;
                let mut blend = None;
                let channel_blend = D3D11_RENDER_TARGET_BLEND_DESC {
                    BlendEnable: true.into(),
                    SrcBlend: D3D11_BLEND_ONE,
                    DestBlend: D3D11_BLEND_ONE,
                    BlendOp: D3D11_BLEND_OP_ADD,
                    SrcBlendAlpha: D3D11_BLEND_ONE,
                    DestBlendAlpha: D3D11_BLEND_ONE,
                    BlendOpAlpha: D3D11_BLEND_OP_ADD,
                    RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
                };
                device.CreateBlendState(
                    &D3D11_BLEND_DESC {
                        RenderTarget: [channel_blend; 8],
                        ..Default::default()
                    },
                    Some(&mut blend),
                )?;
                ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                ctx.RSSetViewports(Some(&[viewport(1.0, 1.0)]));
                ctx.ClearRenderTargetView(&rtv, &[clear, clear, clear, 0.0]);
                ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
                ctx.OMSetBlendState(blend.as_ref(), None, u32::MAX);
                ctx.VSSetShader(&vs, None);
                ctx.PSSetShader(&source_ps, None);
                ctx.Draw(3, 0);
                ctx.OMSetRenderTargets(None, None);

                let (target, rtv) = render_target(device, 1, 1);
                ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
                ctx.OMSetBlendState(None, None, u32::MAX);
                ctx.PSSetShaderResources(0, Some(&[Some(shader_view(device, &image)?)]));
                ctx.PSSetShader(&read_ps, None);
                ctx.Draw(3, 0);
                ctx.OMSetRenderTargets(None, None);
                read_pixels(device, ctx, &target, 1, 1)
            })?
        };
        let mut expected = [128_u8, 128, 128, 255];
        expected[..channels.min(3)].fill(if signed { 64 } else { 223 });
        if channels == 4 {
            expected[3] = 128;
        }
        std::println!("{name} CheckFormatSupport={flags:#010x} blended readback={pixels:?}");
        for (actual, expected) in pixels[0].into_iter().zip(expected) {
            assert!(
                actual.abs_diff(expected) <= 2,
                "{name}: expected {expected}, got {actual}"
            );
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_cube_array_nonzero_view_base_and_copy() -> Result<()> {
    let adapter = hardware();
    let lower = open(&adapter, FeatureLevel::L10_0);
    let exact = open(&adapter, FeatureLevel::L10_1);
    let source = "
        TextureCubeArray<float4> cubes : register(t0);
        SamplerState sampler0 : register(s0);
        float4 main(float4 position : SV_Position) : SV_Target {
            const float3 directions[6] = {
                float3(1,0,0), float3(-1,0,0), float3(0,1,0),
                float3(0,-1,0), float3(0,0,1), float3(0,0,-1)
            };
            return cubes.SampleLevel(sampler0, float4(directions[(uint)position.x], 0), 0);
        }";
    assert!(matches!(
        lower.compile_hlsl(source, "main", ShaderStage::Pixel),
        Err(CompileError::Compiler { .. })
    ));
    let vs = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let ps = exact
        .compile_hlsl(source, "main", ShaderStage::Pixel)
        .unwrap();
    assert_profile(&ps, "ps_4_1");

    let desc = D3D11_TEXTURE2D_DESC {
        ArraySize: 12,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        MiscFlags: D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32,
        ..texture_desc(DXGI_FORMAT_R8G8B8A8_UNORM, 1, 1)
    };
    let pixels = unsafe {
        exact.with_native(|device, ctx| {
            let vertices = vertex_shader(device, &vs)?;
            let pixel = pixel_shader(device, &ps)?;
            let mut invalid = None;
            let seven_faces = device.CreateTexture2D(
                &D3D11_TEXTURE2D_DESC {
                    ArraySize: 7,
                    ..desc
                },
                None,
                Some(&mut invalid),
            );
            std::println!(
                "seven-face resource create HRESULT={:#010x}",
                seven_faces.err().map_or(0, |error| error.code().0)
            );

            let faces: [[u8; 4]; 12] =
                core::array::from_fn(|face| [10 + face as u8 * 16, 0, 0, 255]);
            let init: [_; 12] = core::array::from_fn(|face| D3D11_SUBRESOURCE_DATA {
                pSysMem: faces[face].as_ptr().cast(),
                SysMemPitch: 4,
                SysMemSlicePitch: 4,
            });
            let mut image = None;
            device.CreateTexture2D(&desc, Some(init.as_ptr()), Some(&mut image))?;
            let copy = texture(device, &desc, None);
            ctx.CopyResource(&copy, image.as_ref().unwrap());
            drop(image);
            let mut cube = None;
            let cube_view = |first, count| D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: desc.Format,
                ViewDimension: D3D_SRV_DIMENSION_TEXTURECUBEARRAY,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    TextureCubeArray: D3D11_TEXCUBE_ARRAY_SRV {
                        MostDetailedMip: 0,
                        MipLevels: 1,
                        First2DArrayFace: first,
                        NumCubes: count,
                    },
                },
            };
            let error = device
                .CreateShaderResourceView(&copy, Some(&cube_view(6, 2)), Some(&mut cube))
                .unwrap_err();
            std::println!("out-of-bounds cube SRV rejected: {:#010x}", error.code().0);
            device.CreateShaderResourceView(&copy, Some(&cube_view(6, 1)), Some(&mut cube))?;
            let mut sampler = None;
            device.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    MaxAnisotropy: 1,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
            let (target, rtv) = render_target(device, 6, 1);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.RSSetViewports(Some(&[viewport(6.0, 1.0)]));
            ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
            ctx.VSSetShader(&vertices, None);
            ctx.PSSetShader(&pixel, None);
            ctx.PSSetShaderResources(0, Some(&[cube]));
            ctx.PSSetSamplers(0, Some(&[sampler]));
            ctx.Draw(3, 0);
            ctx.OMSetRenderTargets(None, None);
            read_pixels(device, ctx, &target, 6, 1)
        })?
    };
    std::println!("cube array base-face=6 copied readback={pixels:?}");
    assert_eq!(
        pixels,
        [106, 122, 138, 154, 170, 186].map(|r| [r, 0, 0, 255])
    );
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_standard_4x_sample_frequency_copy_and_readback() -> Result<()> {
    let exact = open(&hardware(), FeatureLevel::L10_1);
    let rgba8 = DXGI_FORMAT_R8G8B8A8_UNORM;
    let flags = exact.check_format_support(rgba8)?;
    let levels = exact.check_multisample_quality_levels(rgba8, 4)?;
    std::println!("RGBA8 CheckFormatSupport={flags:#010x} quality levels for 4x={levels}");
    let required = (D3D11_FORMAT_SUPPORT_MULTISAMPLE_RENDERTARGET.0
        | D3D11_FORMAT_SUPPORT_MULTISAMPLE_LOAD.0) as u32;
    assert_eq!(flags & required, required);
    assert!(levels > 0);
    let vs_source = "
        struct Out {
            float4 pos : SV_Position;
            float2 uv : TEXCOORD;
        };
        Out main(uint i : SV_VertexID) {
            Out o;
            o.uv = float2((i << 1) & 2, i & 2);
            o.pos = float4(o.uv * float2(2, -2) + float2(-1, 1), 0, 1);
            return o;
        }";
    let ps_sample = "
        float4 main(float4 pos : SV_Position, sample float2 uv : TEXCOORD,
                    uint index : SV_SampleIndex) : SV_Target {
            return float4(uv * pos.w, (index + 1) * 0.125, 1);
        }";
    let ps_load = "
        Texture2DMS<float4, 4> image : register(t0);
        float4 main(float4 position : SV_Position) : SV_Target {
            return image.Load(int2(0,0), (int)position.x);
        }";
    let vs_sample = exact
        .compile_hlsl(vs_source, "main", ShaderStage::Vertex)
        .unwrap();
    let vs_load = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let ps_sample = exact
        .compile_hlsl(ps_sample, "main", ShaderStage::Pixel)
        .unwrap();
    let ps_load = exact
        .compile_hlsl(ps_load, "main", ShaderStage::Pixel)
        .unwrap();
    let disassembly = assert_profile(&ps_sample, "ps_4_1");
    assert!(disassembly.contains("linear sample"));
    let disassembly = assert_profile(&ps_load, "ps_4_1");
    assert!(
        disassembly.lines().any(|line| {
            let instruction = line.trim_start();
            instruction.starts_with("ld2dms") || instruction.starts_with("ldms")
        }),
        "{disassembly}"
    );

    let pixels = unsafe {
        exact.with_native(|device, ctx| {
            let vs_sample = vertex_shader(device, &vs_sample)?;
            let vs_load = vertex_shader(device, &vs_load)?;
            let ps_sample = pixel_shader(device, &ps_sample)?;
            let ps_load = pixel_shader(device, &ps_load)?;
            let desc = D3D11_TEXTURE2D_DESC {
                BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 4,
                    Quality: D3D11_STANDARD_MULTISAMPLE_PATTERN.0 as u32,
                },
                ..texture_desc(rgba8, 1, 1)
            };
            let image = texture(device, &desc, None);
            let copy = texture(device, &desc, None);
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.RSSetViewports(Some(&[viewport(1.0, 1.0)]));
            ctx.VSSetShader(&vs_sample, None);
            ctx.PSSetShader(&ps_sample, None);
            ctx.OMSetRenderTargets(Some(&[Some(view(device, &image)?)]), None);
            ctx.Draw(3, 0);
            ctx.OMSetRenderTargets(None, None);
            ctx.CopyResource(&copy, &image);

            let mut multisample = None;
            device.CreateShaderResourceView(&copy, None, Some(&mut multisample))?;
            let (target, rtv) = render_target(device, 4, 1);
            ctx.RSSetViewports(Some(&[viewport(4.0, 1.0)]));
            ctx.VSSetShader(&vs_load, None);
            ctx.PSSetShader(&ps_load, None);
            ctx.PSSetShaderResources(0, Some(&[multisample]));
            ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
            ctx.Draw(3, 0);
            ctx.OMSetRenderTargets(None, None);
            read_pixels(device, ctx, &target, 4, 1)
        })?
    };
    std::println!("RGBA8 4x standard sample readback={pixels:?}");
    assert_unorm8(
        &pixels,
        [
            [0.375, 0.125, 0.125, 1.0],
            [0.875, 0.375, 0.25, 1.0],
            [0.125, 0.625, 0.375, 1.0],
            [0.625, 0.875, 0.5, 1.0],
        ]
        .as_slice(),
    );
    Ok(())
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_independent_blend_eight_targets() -> Result<()> {
    let exact = open(&hardware(), FeatureLevel::L10_1);
    let vs = exact
        .compile_hlsl(FULL_SCREEN, "main", ShaderStage::Vertex)
        .unwrap();
    let source = "
        void main(out float4 a : SV_Target0, out float4 b : SV_Target1,
                  out float4 c : SV_Target2, out float4 d : SV_Target3,
                  out float4 e : SV_Target4, out float4 f : SV_Target5,
                  out float4 g : SV_Target6, out float4 h : SV_Target7) {
            a = b = c = d = e = f = g = h = float4(.5, .25, 0, 1);
        }";
    let ps = exact
        .compile_hlsl(source, "main", ShaderStage::Pixel)
        .unwrap();
    assert_profile(&ps, "ps_4_1");
    let pixels = unsafe {
        exact.with_native(|device, ctx| {
            let vs = vertex_shader(device, &vs)?;
            let ps = pixel_shader(device, &ps)?;
            let mut targets = Vec::new();
            let mut rtvs = Vec::new();
            for _ in 0..8 {
                let (target, rtv) = render_target(device, 1, 1);
                ctx.ClearRenderTargetView(&rtv, &[0.0, 0.0, 1.0, 1.0]);
                targets.push(target);
                rtvs.push(Some(rtv));
            }
            let mut blend = None;
            device.CreateBlendState(
                &D3D11_BLEND_DESC {
                    IndependentBlendEnable: true.into(),
                    RenderTarget: core::array::from_fn(|index| {
                        let writes_source = index % 2 == 0 || index == 7;
                        D3D11_RENDER_TARGET_BLEND_DESC {
                            BlendEnable: true.into(),
                            SrcBlend: if writes_source {
                                D3D11_BLEND_ONE
                            } else {
                                D3D11_BLEND_ZERO
                            },
                            DestBlend: if writes_source {
                                D3D11_BLEND_ZERO
                            } else {
                                D3D11_BLEND_ONE
                            },
                            BlendOp: D3D11_BLEND_OP_ADD,
                            SrcBlendAlpha: D3D11_BLEND_ONE,
                            DestBlendAlpha: D3D11_BLEND_ZERO,
                            BlendOpAlpha: D3D11_BLEND_OP_ADD,
                            RenderTargetWriteMask: if index == 7 {
                                D3D11_COLOR_WRITE_ENABLE_RED.0 as u8
                            } else {
                                D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8
                            },
                        }
                    }),
                    ..Default::default()
                },
                Some(&mut blend),
            )?;
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.RSSetViewports(Some(&[viewport(1.0, 1.0)]));
            ctx.OMSetRenderTargets(Some(&rtvs), None);
            ctx.OMSetBlendState(blend.as_ref(), None, u32::MAX);
            ctx.VSSetShader(&vs, None);
            ctx.PSSetShader(&ps, None);
            ctx.Draw(3, 0);
            ctx.OMSetRenderTargets(None, None);
            targets
                .iter()
                .map(|target| read_pixels(device, ctx, target, 1, 1).map(|pixels| pixels[0]))
                .collect::<Result<Vec<_>>>()
        })?
    };
    std::println!("independent blend RGBA8 readback={pixels:?}");
    assert_unorm8(
        &pixels,
        &[
            [0.5, 0.25, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.5, 0.25, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.5, 0.25, 0.0, 1.0],
            [0.0, 0.0, 1.0, 1.0],
            [0.5, 0.25, 0.0, 1.0],
            [0.5, 0.0, 1.0, 1.0],
        ],
    );
    Ok(())
}
