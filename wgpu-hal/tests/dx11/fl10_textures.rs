use wgpu_hal::dx11::fl10;
use windows::Win32::Graphics::{
    Direct3D::{D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D11_SRV_DIMENSION_TEXTURE2DARRAY},
    Direct3D11::*,
    Dxgi::Common::*,
};

const WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) id: u32) -> @builtin(position) vec4f {
    let x = f32((id & 2u) << 1u) - 1.0;
    let y = f32((id & 1u) << 2u) - 1.0;
    return vec4f(x, y, 0.0, 1.0);
}

@group(0) @binding(0) var layers: texture_2d_array<f32>;
@group(0) @binding(1) var volume: texture_3d<f32>;

@fragment fn ps() -> @location(0) u32 {
    let mip = textureLoad(layers, vec2i(0, 0), 0, 2);
    let voxel = textureLoad(volume, vec3i(0, 0, 1), 0);
    return u32(round(255.0 * mip.r))
         | (u32(round(255.0 * mip.g)) << 8)
         | (u32(round(255.0 * voxel.b)) << 16);
}
";

pub(super) fn exercise(native: &fl10::Device) {
    let module = naga::front::wgsl::parse_str(WGSL).unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();

    let vs_code = fl10::compile_graphics_entry(
        &module,
        &info,
        naga::ShaderStage::Vertex,
        "vs",
        Default::default(),
    )
    .unwrap();
    assert!(
        fl10::compile_graphics_entry(
            &module,
            &info,
            naga::ShaderStage::Fragment,
            "ps",
            Default::default(),
        )
        .is_err()
    );
    let registers = (0..2)
        .map(|binding| {
            (
                naga::ResourceBinding { group: 0, binding },
                naga::back::hlsl::BindTarget {
                    register: binding,
                    ..Default::default()
                },
            )
        })
        .collect();
    let ps_code =
        fl10::compile_graphics_entry(&module, &info, naga::ShaderStage::Fragment, "ps", registers)
            .unwrap();

    unsafe {
        let (array, volume) = native
            .with_context(|device, context| (array_view(device, context), volume_view(device)));
        native.with_context(|device, context| {
            draw_and_read(device, context, &vs_code, &ps_code, array, volume);
        });
    }
}

unsafe fn array_view(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
) -> ID3D11ShaderResourceView {
    unsafe {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: 5,
            Height: 3,
            MipLevels: 3,
            ArraySize: 2,
            Format: DXGI_FORMAT_R8G8B8A8_TYPELESS,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_GENERATE_MIPS.0 as u32,
            ..Default::default()
        };
        let mut source = None;
        let mut copy = None;
        device
            .CreateTexture2D(&desc, None, Some(&mut source))
            .unwrap();
        device
            .CreateTexture2D(&desc, None, Some(&mut copy))
            .unwrap();
        let (source, copy) = (source.unwrap(), copy.unwrap());

        for (subresource, value) in [(0, [0_u8, 0, 255, 255]), (3, [128_u8, 255, 0, 255])] {
            let data = [value; 15];
            context.UpdateSubresource(&source, subresource, None, data.as_ptr().cast(), 20, 0);
        }
        let mut srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2DARRAY,
            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                Texture2DArray: D3D11_TEX2D_ARRAY_SRV {
                    MostDetailedMip: 0,
                    MipLevels: 3,
                    FirstArraySlice: 0,
                    ArraySize: 2,
                },
            },
        };
        let mut mip_view = None;
        device
            .CreateShaderResourceView(&source, Some(&srv_desc), Some(&mut mip_view))
            .unwrap();
        context.GenerateMips(mip_view.as_ref().unwrap());
        context.CopyResource(&copy, &source);

        srv_desc.Anonymous.Texture2DArray.FirstArraySlice = 1;
        srv_desc.Anonymous.Texture2DArray.ArraySize = 1;
        let mut view = None;
        device
            .CreateShaderResourceView(&copy, Some(&srv_desc), Some(&mut view))
            .unwrap();
        srv_desc.Format = DXGI_FORMAT_R16G16_FLOAT;
        assert!(
            device
                .CreateShaderResourceView(&copy, Some(&srv_desc), None)
                .is_err()
        );
        view.unwrap()
    }
}

unsafe fn volume_view(device: &ID3D11Device) -> ID3D11ShaderResourceView {
    unsafe {
        let mut texture = None;
        let data = [[[0_u8, 0, 0, 255]; 4], [[0_u8, 0, 173, 255]; 4]];
        let init = D3D11_SUBRESOURCE_DATA {
            pSysMem: data.as_ptr().cast(),
            SysMemPitch: 8,
            SysMemSlicePitch: 16,
        };
        device
            .CreateTexture3D(
                &D3D11_TEXTURE3D_DESC {
                    Width: 2,
                    Height: 2,
                    Depth: 2,
                    MipLevels: 1,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    Usage: D3D11_USAGE_IMMUTABLE,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..Default::default()
                },
                Some(&init),
                Some(&mut texture),
            )
            .unwrap();
        let mut view = None;
        device
            .CreateShaderResourceView(texture.as_ref().unwrap(), None, Some(&mut view))
            .unwrap();
        view.unwrap()
    }
}

unsafe fn draw_and_read(
    device: &ID3D11Device,
    context: &ID3D11DeviceContext,
    vs_code: &[u8],
    ps_code: &[u8],
    array: ID3D11ShaderResourceView,
    volume: ID3D11ShaderResourceView,
) {
    unsafe {
        let color = super::target(device, false);
        let staging = super::target(device, true);
        let mut rtv = None;
        device
            .CreateRenderTargetView(&color, None, Some(&mut rtv))
            .unwrap();
        context.ClearRenderTargetView(rtv.as_ref().unwrap(), &[0.0; 4]);
        let mut vs = None;
        device
            .CreateVertexShader(vs_code, None, Some(&mut vs))
            .unwrap();
        let mut ps = None;
        device
            .CreatePixelShader(ps_code, None, Some(&mut ps))
            .unwrap();
        context.VSSetShader(vs.as_ref(), None);
        context.PSSetShader(ps.as_ref(), None);
        context.PSSetShaderResources(0, Some(&[Some(array), Some(volume)]));
        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: 4.0,
            Height: 4.0,
            MaxDepth: 1.0,
            ..Default::default()
        }]));
        context.OMSetRenderTargets(Some(&[rtv]), None::<&ID3D11DepthStencilView>);
        context.Draw(3, 0);
        context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
        context.CopyResource(&staging, &color);
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        context
            .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .unwrap();
        let pixels = core::array::from_fn::<_, 4, _>(|y| {
            *mapped
                .pData
                .cast::<u8>()
                .add(y * mapped.RowPitch as usize)
                .cast::<[u32; 4]>()
        });
        context.Unmap(&staging, 0);
        assert_eq!(pixels, [[0x00adff37; 4]; 4]);
    }
}
