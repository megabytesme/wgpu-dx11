use crate::dx11::{enumerate_native_adapters, Device, FeatureLevel};
use alloc::{sync::Arc, vec::Vec};
use windows::{
    core::Result,
    Win32::Graphics::{Direct3D::D3D_SRV_DIMENSION_TEXTURECUBE, Direct3D11::*, Dxgi::Common::*},
};

fn exact_fl93(run: impl FnOnce(&ID3D11Device, &ID3D11DeviceContext) -> Result<()>) -> Result<()> {
    if std::env::var_os("WGPU_DX11_FEATURE_LEVEL").is_none_or(|value| value != "9_3") {
        return Ok(());
    }
    let adapters = enumerate_native_adapters()?;
    let adapter = adapters
        .iter()
        .find(|adapter| !adapter.identity().software)
        .expect("physical DXGI adapter");
    let mut native = adapter.open_device(&[FeatureLevel::L9_3])?;
    assert_eq!(native.feature_level(), FeatureLevel::L9_3);
    native.enable_experimental_fl93();
    std::println!(
        "Exact FL9_3 on {:?}, driver {:?}",
        adapter.identity(),
        adapter.driver_version()
    );
    unsafe { Device::new(Arc::new(native)).with_fl93_native_context(run) }
}

fn create_2d(device: &ID3D11Device, desc: &D3D11_TEXTURE2D_DESC) -> Result<ID3D11Texture2D> {
    let mut texture = None;
    unsafe { device.CreateTexture2D(desc, None, Some(&mut texture))? };
    Ok(texture.expect("native 2D texture"))
}

fn texture_desc(format: DXGI_FORMAT, width: u32, height: u32) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        Format: format,
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    }
}

fn staging_2d(
    device: &ID3D11Device,
    format: DXGI_FORMAT,
    width: u32,
    height: u32,
    mips: u32,
) -> Result<ID3D11Texture2D> {
    create_2d(
        device,
        &D3D11_TEXTURE2D_DESC {
            Usage: D3D11_USAGE_STAGING,
            MipLevels: mips,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..texture_desc(format, width, height)
        },
    )
}

fn cube_desc(format: DXGI_FORMAT, edge: u32, mips: u32) -> D3D11_TEXTURE2D_DESC {
    D3D11_TEXTURE2D_DESC {
        ArraySize: 6,
        MipLevels: mips,
        MiscFlags: D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32,
        ..texture_desc(format, edge, edge)
    }
}

fn create_cube(
    device: &ID3D11Device,
    format: DXGI_FORMAT,
    edge: u32,
    mips: u32,
) -> Result<ID3D11Texture2D> {
    let texture = create_2d(device, &cube_desc(format, edge, mips))?;
    let mut view = None;
    unsafe {
        device.CreateShaderResourceView(
            &texture,
            Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: format,
                ViewDimension: D3D_SRV_DIMENSION_TEXTURECUBE,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    TextureCube: D3D11_TEXCUBE_SRV {
                        MostDetailedMip: 0,
                        MipLevels: mips,
                    },
                },
            }),
            Some(&mut view),
        )?;
    }
    assert!(view.is_some());
    Ok(texture)
}

fn region(x: u32, y: u32, size: u32) -> D3D11_BOX {
    D3D11_BOX {
        left: x,
        right: x + size,
        top: y,
        bottom: y + size,
        front: 0,
        back: 1,
    }
}

fn read_staging(
    context: &ID3D11DeviceContext,
    resource: &ID3D11Resource,
    mip: u32,
    byte_count: usize,
) -> Result<Vec<u8>> {
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe {
        context.Map(resource, mip, D3D11_MAP_READ, 0, Some(&mut mapped))?;
        let data = (!mapped.pData.is_null() && mapped.RowPitch as usize >= byte_count)
            .then(|| core::slice::from_raw_parts(mapped.pData.cast::<u8>(), byte_count).to_vec());
        context.Unmap(resource, mip);
        Ok(data.expect("valid mapped staging row"))
    }
}

#[test]
fn tier_fl9_3_native_rg32_float_color_and_volume() -> Result<()> {
    use windows::core::Interface as _;
    exact_fl93(|device, context| {
        let format = DXGI_FORMAT_R32G32_FLOAT;
        let bits = unsafe { device.CheckFormatSupport(format)? };
        std::println!("RG32F native format flags: {bits:#x}");
        let shape_bits = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_TEXTURE3D.0
            | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32;
        assert_eq!(bits & shape_bits, shape_bits, "RG32F support was {bits:#x}");
        let color = create_2d(
            device,
            &D3D11_TEXTURE2D_DESC {
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..texture_desc(format, 1, 1)
            },
        )?;
        let staging = staging_2d(device, format, 1, 1, 1)?;
        let mut view = None;
        unsafe {
            device.CreateRenderTargetView(&color, None, Some(&mut view))?;
            context
                .ClearRenderTargetView(view.as_ref().expect("RG32F RTV"), &[0.25, 0.5, 0.0, 1.0]);
            context.CopyResource(&staging, &color);
        }
        let expected = [0.25f32, 0.5].map(f32::to_le_bytes);
        assert_eq!(
            read_staging(context, &staging.cast()?, 0, 8)?,
            expected.as_flattened()
        );

        std::println!("RG32F 2D render target clear and readback matched");
        for (width, height, depth) in [(256, 1, 1), (1, 256, 1), (1, 1, 256)] {
            let desc = D3D11_TEXTURE3D_DESC {
                Width: width,
                Height: height,
                Depth: depth,
                MipLevels: 1,
                Format: format,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                ..Default::default()
            };
            let mut volume = None;
            let mut srv = None;
            unsafe {
                device.CreateTexture3D(&desc, None, Some(&mut volume))?;
                let volume = volume.expect("RG32F volume");
                device.CreateShaderResourceView(&volume, None, Some(&mut srv))?;
            }
            assert!(srv.is_some());
            std::println!("RG32F {width}x{height}x{depth} volume and SRV created");
        }
        std::println!("Creating RG32F cube and SRV");
        let cube = create_cube(device, format, 4, 1)?;
        let edge = region(3, 3, 1);
        unsafe {
            context.UpdateSubresource(&cube, 5, Some(&edge), expected.as_ptr().cast(), 8, 8);
            context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &cube, 5, Some(&edge));
        }
        assert_eq!(
            read_staging(context, &staging.cast()?, 0, 8)?,
            expected.as_flattened()
        );
        std::println!("RG32F support={bits:#x}; RTV clear, 256 on all axes with SRVs, and cube-face SRV and copy matched");
        Ok(())
    })
}

#[test]
fn tier_fl9_3_native_2d_and_cube_extent() -> Result<()> {
    use windows::core::Interface as _;
    exact_fl93(|device, context| {
        let format = DXGI_FORMAT_R8G8B8A8_UNORM;
        let staging = staging_2d(device, format, 1, 1, 1)?;
        let data = [71u8, 113, 197, 255];
        for (width, height) in [(4096, 1), (1, 4096)] {
            let texture = create_2d(device, &texture_desc(format, width, height))?;
            let edge = region(width - 1, height - 1, 1);
            unsafe {
                context.UpdateSubresource(&texture, 0, Some(&edge), data.as_ptr().cast(), 4, 4);
                context.CopySubresourceRegion(&staging, 0, 0, 0, 0, &texture, 0, Some(&edge));
            }
            assert_eq!(read_staging(context, &staging.cast()?, 0, 4)?, data);
        }
        std::println!("RGBA8 4096x1 and 1x4096 edge uploads and readback matched");

        let format = DXGI_FORMAT_BC1_UNORM;
        let mips = 13;
        let cube = create_cube(device, format, 4096, mips)?;
        // Small compressed mips still occupy one block. Match their dimensions
        // with a mip of a block-aligned staging texture.
        let staging = staging_2d(device, format, 4, 4, 3)?;
        let edge = region(4092, 4092, 4);
        let block = |face, mip| [face as u8, 0xf8, 0xe0, 0x07, mip as u8, 0, 0, 0];
        for face in 0..6 {
            for (mip, source_box) in [(0, Some(&raw const edge)), (mips - 1, None)] {
                unsafe {
                    context.UpdateSubresource(
                        &cube,
                        face * mips + mip,
                        source_box,
                        block(face, mip).as_ptr().cast(),
                        8,
                        8,
                    )
                };
            }
        }
        for face in (0..6).rev() {
            for (mip, source_box, staging_mip) in
                [(0, Some(&raw const edge), 0), (mips - 1, None, 2)]
            {
                unsafe {
                    context.CopySubresourceRegion(
                        &staging,
                        staging_mip,
                        0,
                        0,
                        0,
                        &cube,
                        face * mips + mip,
                        source_box,
                    )
                };
                assert_eq!(
                    read_staging(context, &staging.cast()?, staging_mip, 8)?,
                    block(face, mip)
                );
            }
        }
        std::println!(
            "BC1 4096-edge cube SRV and all six distinct base-edge and final-mip blocks verified"
        );
        let desc = cube_desc(format, 4096, 1);
        for invalid in [
            D3D11_TEXTURE2D_DESC {
                ArraySize: 5,
                ..desc
            },
            D3D11_TEXTURE2D_DESC {
                Height: 2048,
                ..desc
            },
            cube_desc(format, 4097, 1),
        ] {
            assert_eq!(
                unsafe { device.CreateTexture2D(&invalid, None, None) }
                    .expect_err("invalid cube descriptor")
                    .code(),
                windows::Win32::Foundation::E_INVALIDARG
            );
        }
        std::println!("Invalid face count, rectangular cube and edge 4097 rejected");
        Ok(())
    })
}

#[test]
fn tier_fl9_3_native_rgba16_float_blend() -> Result<()> {
    use crate::dx11::shader::{compile_hlsl, ShaderProfile};
    use windows::{
        core::{Interface as _, PCSTR},
        Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
    };
    exact_fl93(|device, context| {
        let format = DXGI_FORMAT_R16G16B16A16_FLOAT;
        let support = unsafe { device.CheckFormatSupport(format)? };
        assert_ne!(support & D3D11_FORMAT_SUPPORT_BLENDABLE.0 as u32, 0);
        let target = create_2d(
            device,
            &D3D11_TEXTURE2D_DESC {
                BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
                ..texture_desc(format, 2, 2)
            },
        )?;
        let staging = staging_2d(device, format, 2, 2, 1)?;
        let vs = compile_hlsl(
            "float4 main(float2 position : POSITION) : SV_Position { return float4(position, 0, 1); }",
            "main", ShaderProfile::VertexLevel9_3,
        ).expect("FL9_3 vertex shader");
        let ps = compile_hlsl(
            "float4 main() : SV_Target { return float4(0.25, 0.125, 0.5, 0.25); }",
            "main",
            ShaderProfile::PixelLevel9_3,
        )
        .expect("FL9_3 pixel shader");
        let positions = [[-1.0f32, -1.0], [-1.0, 3.0], [3.0, -1.0]];
        let mut vertex_buffer = None;
        let mut rtv = None;
        let mut input = None;
        let mut vertex = None;
        let mut pixel = None;
        let mut rasterizer = None;
        let mut blend = None;
        let mut depth = None;
        let mut blend_desc = D3D11_BLEND_DESC::default();
        blend_desc.RenderTarget[..4].fill(D3D11_RENDER_TARGET_BLEND_DESC {
            BlendEnable: true.into(),
            SrcBlend: D3D11_BLEND_ONE,
            DestBlend: D3D11_BLEND_ONE,
            BlendOp: D3D11_BLEND_OP_ADD,
            SrcBlendAlpha: D3D11_BLEND_ONE,
            DestBlendAlpha: D3D11_BLEND_ONE,
            BlendOpAlpha: D3D11_BLEND_OP_ADD,
            RenderTargetWriteMask: 0xf,
        });
        unsafe {
            device.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: size_of_val(&positions) as u32,
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                    ..Default::default()
                },
                Some(&D3D11_SUBRESOURCE_DATA {
                    pSysMem: positions.as_ptr().cast(),
                    ..Default::default()
                }),
                Some(&mut vertex_buffer),
            )?;
            device.CreateRenderTargetView(&target, None, Some(&mut rtv))?;
            device.CreateVertexShader(&vs, None, Some(&mut vertex))?;
            device.CreatePixelShader(&ps, None, Some(&mut pixel))?;
            device.CreateInputLayout(
                &[D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    ..Default::default()
                }],
                &vs,
                Some(&mut input),
            )?;
            device.CreateRasterizerState(
                &D3D11_RASTERIZER_DESC {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut rasterizer),
            )?;
            device.CreateBlendState(&blend_desc, Some(&mut blend))?;
            device.CreateDepthStencilState(
                &D3D11_DEPTH_STENCIL_DESC {
                    DepthFunc: D3D11_COMPARISON_ALWAYS,
                    ..Default::default()
                },
                Some(&mut depth),
            )?;
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            context.IASetInputLayout(input.as_ref());
            context.IASetVertexBuffers(0, 1, Some(&vertex_buffer), Some(&8), Some(&0));
            context.VSSetShader(vertex.as_ref(), None);
            context.PSSetShader(pixel.as_ref(), None);
            context.RSSetState(rasterizer.as_ref());
            context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                Width: 2.0,
                Height: 2.0,
                MaxDepth: 1.0,
                ..Default::default()
            }]));
            context.ClearRenderTargetView(
                rtv.as_ref().expect("RGBA16F RTV"),
                &[0.25, 0.375, 0.125, 0.25],
            );
            context.OMSetRenderTargets(Some(&[rtv]), None);
            context.OMSetBlendState(blend.as_ref(), None, u32::MAX);
            context.OMSetDepthStencilState(depth.as_ref(), 0);
            context.Draw(3, 0);
            context.OMSetRenderTargets(None, None);
            context.CopyResource(&staging, &target);
        }
        let expected = [0x3800u16, 0x3800, 0x3900, 0x3800].map(u16::to_le_bytes);
        assert_eq!(
            read_staging(context, &staging.cast()?, 0, 8)?,
            expected.as_flattened()
        );
        std::println!(
            "RGBA16F additive blending matched half-float readback; support={support:#x}"
        );
        Ok(())
    })
}
