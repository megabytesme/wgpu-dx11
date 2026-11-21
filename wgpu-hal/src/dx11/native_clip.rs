use core::mem::size_of_val;
use windows::{
    core::{Error, Result, PCSTR},
    Win32::{
        Foundation::E_FAIL,
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Direct3D11::{
                ID3D11ClassLinkage, ID3D11DepthStencilView, ID3D11Device, ID3D11DeviceContext,
                D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BIND_VERTEX_BUFFER,
                D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ, D3D11_CULL_NONE, D3D11_FILL_SOLID,
                D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAPPED_SUBRESOURCE,
                D3D11_MAP_READ, D3D11_RASTERIZER_DESC, D3D11_SUBRESOURCE_DATA,
                D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE,
                D3D11_USAGE_STAGING, D3D11_VIEWPORT,
            },
            Dxgi::Common::{
                DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
            },
        },
    },
};

use super::{
    enumerate_native_adapters, hardware_test,
    shader::{compile_hlsl, ShaderProfile},
    FeatureLevel,
};

const SIZE: usize = 16;
const OFF: [f32; 4] = [0.0; 4];
const CLEAR_PIXEL: [u8; 4] = [0, 0, 0, 255];
const DRAWN_PIXEL: [u8; 4] = [255, 0, 255, 255];
const SOURCE: &str = r#"
cbuffer Clip : register(b0) { float4 planes[6]; };
[clipplanes(planes[0], planes[1], planes[2], planes[3], planes[4], planes[5])]
float4 vertex_main(float2 position : POSITION) : SV_Position {
    return float4(position, 0.5, 1.0);
}
float4 pixel_main() : SV_Target {
    return float4(1.0, 0.0, 1.0, 1.0);
}
"#;

#[test]
fn fl91_native_six_user_clip_planes_readback() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = enumerate_native_adapters().unwrap();
    let Some(adapter) = hardware_test::select_adapter(&adapters, required).unwrap() else {
        return;
    };
    let device = match adapter.open_device(&[FeatureLevel::L9_1]) {
        Ok(device) => device,
        Err(error) if !required => {
            std::eprintln!("skipping clip-plane readback: exact FL9_1 unavailable ({error})");
            return;
        }
        Err(error) => panic!("opening hardware at exact FL9_1 failed: {error}"),
    };
    assert_eq!(device.feature_level(), FeatureLevel::L9_1);
    std::eprintln!(
        "native clip planes adapter={:?} driver={:?} requested=0x9100 returned={:#x}",
        adapter.identity(),
        adapter.driver_version(),
        device.feature_level().as_raw(),
    );

    let vs = compile_hlsl(SOURCE, "vertex_main", ShaderProfile::VertexLevel9_1)
        .expect("compile six user clip planes for vs_4_0_level_9_1");
    let ps = compile_hlsl(SOURCE, "pixel_main", ShaderProfile::PixelLevel9_1)
        .expect("compile fixture pixel shader for ps_4_0_level_9_1");
    // The callback owns all resources and does not retain or reenter the context.
    unsafe { device.with_native_context(|raw, context| check_render(raw, context, &vs, &ps)) }
        .unwrap();
}

fn check_render(
    raw: &ID3D11Device,
    context: &ID3D11DeviceContext,
    vs_code: &[u8],
    ps_code: &[u8],
) -> Result<()> {
    let mut vs = None;
    let mut ps = None;
    let mut layout = None;
    let mut vertices = None;
    let mut constants = None;
    let mut rasterizer = None;
    let mut target = None;
    let mut view = None;
    let mut staging = None;

    let vertex_data = [-1.0f32, -1.0, -1.0, 3.0, 3.0, -1.0];
    let vertex_desc = D3D11_BUFFER_DESC {
        ByteWidth: size_of_val(&vertex_data) as u32,
        Usage: D3D11_USAGE_IMMUTABLE,
        BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
        ..Default::default()
    };
    let vertex_init = D3D11_SUBRESOURCE_DATA {
        pSysMem: vertex_data.as_ptr().cast(),
        ..Default::default()
    };
    let constants_desc = D3D11_BUFFER_DESC {
        ByteWidth: size_of_val(&[OFF; 6]) as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
        ..Default::default()
    };
    let layout_desc = [D3D11_INPUT_ELEMENT_DESC {
        SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
        Format: DXGI_FORMAT_R32G32_FLOAT,
        InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
        ..Default::default()
    }];
    let target_desc = D3D11_TEXTURE2D_DESC {
        Width: SIZE as u32,
        Height: SIZE as u32,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    };
    let staging_desc = D3D11_TEXTURE2D_DESC {
        Usage: D3D11_USAGE_STAGING,
        BindFlags: 0,
        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
        ..target_desc
    };
    let rasterizer_desc = D3D11_RASTERIZER_DESC {
        FillMode: D3D11_FILL_SOLID,
        CullMode: D3D11_CULL_NONE,
        DepthClipEnable: true.into(),
        ..Default::default()
    };

    unsafe {
        raw.CreateVertexShader(vs_code, None::<&ID3D11ClassLinkage>, Some(&mut vs))?;
        raw.CreatePixelShader(ps_code, None::<&ID3D11ClassLinkage>, Some(&mut ps))?;
        raw.CreateInputLayout(&layout_desc, vs_code, Some(&mut layout))?;
        raw.CreateBuffer(&vertex_desc, Some(&vertex_init), Some(&mut vertices))?;
        raw.CreateBuffer(&constants_desc, None, Some(&mut constants))?;
        raw.CreateRasterizerState(&rasterizer_desc, Some(&mut rasterizer))?;
        raw.CreateTexture2D(&target_desc, None, Some(&mut target))?;
        raw.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        let target = target.expect("D3D11 render target");
        let staging = staging.expect("D3D11 staging texture");
        let constants = constants.expect("D3D11 clip-plane constants");
        raw.CreateRenderTargetView(&target, None, Some(&mut view))?;
        let view = view.expect("D3D11 render target view");
        let targets = [Some(view.clone())];
        context.RSSetState(rasterizer.as_ref());
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: SIZE as f32,
            Height: SIZE as f32,
            MaxDepth: 1.0,
            ..Default::default()
        }]));
        context.IASetInputLayout(layout.as_ref());
        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context.IASetVertexBuffers(
            0,
            1,
            Some(&vertices),
            Some(&(2 * size_of_val(&0.0f32) as u32)),
            Some(&0),
        );
        context.VSSetShader(vs.as_ref(), None);
        context.PSSetShader(ps.as_ref(), None);
        context.VSSetConstantBuffers(0, Some(&[Some(constants.clone())]));

        let draw = |planes: [[f32; 4]; 6]| -> Result<[[[u8; 4]; SIZE]; SIZE]> {
            context.UpdateSubresource(&constants, 0, None, planes.as_ptr().cast(), 0, 0);
            context.ClearRenderTargetView(&view, &[0.0, 0.0, 0.0, 1.0]);
            context.OMSetRenderTargets(Some(&targets), None::<&ID3D11DepthStencilView>);
            context.Draw(3, 0);
            context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            context.CopyResource(&staging, &target);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() || mapped.RowPitch < (SIZE * 4) as u32 {
                context.Unmap(&staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let mut pixels = [[[0; 4]; SIZE]; SIZE];
            for (row, destination) in pixels.iter_mut().enumerate() {
                core::ptr::copy_nonoverlapping(
                    mapped
                        .pData
                        .cast::<u8>()
                        .add(row * mapped.RowPitch as usize),
                    destination.as_mut_ptr().cast(),
                    SIZE * 4,
                );
            }
            context.Unmap(&staging, 0);
            Ok(pixels)
        };

        assert_coverage("disabled", draw([OFF; 6])?, |_, _| true);
        for slot in 0..6 {
            for sign in [1.0, -1.0] {
                let mut planes = [OFF; 6];
                planes[slot] = [sign, 0.0, 0.0, 0.0];
                let case = alloc::format!("slot {slot} x sign {sign}");
                assert_coverage(&case, draw(planes)?, |x, _| (x >= SIZE / 2) == (sign > 0.0));
            }
        }

        // Four planes bound the rectangle. Two more cut off opposite corners.
        let planes = [
            [1.0, 0.0, 0.0, 0.5],
            [-1.0, 0.0, 0.0, 0.5],
            [0.0, 1.0, 0.0, 0.5],
            [0.0, -1.0, 0.0, 0.5],
            [1.0, 1.0, 0.0, 0.5625],
            [-1.0, -1.0, 0.0, 0.5625],
        ];
        assert_coverage("six-plane intersection", draw(planes)?, |x, y| {
            (4..12).contains(&x) && (4..12).contains(&y) && x.abs_diff(y) <= 4
        });
    }
    Ok(())
}

fn assert_coverage(
    label: &str,
    pixels: [[[u8; 4]; SIZE]; SIZE],
    covered: impl Fn(usize, usize) -> bool,
) {
    let mut drawn = 0;
    for (y, row) in pixels.iter().enumerate() {
        for (x, &actual) in row.iter().enumerate() {
            let expected = if covered(x, y) {
                drawn += 1;
                DRAWN_PIXEL
            } else {
                CLEAR_PIXEL
            };
            assert_eq!(actual, expected, "clip planes {label}, pixel ({x}, {y})");
        }
    }
    std::eprintln!(
        "native clip planes {label}: covered={drawn} clear={} upper-left={:?} upper-right={:?} center={:?}",
        SIZE * SIZE - drawn, pixels[0][0], pixels[0][SIZE - 1], pixels[SIZE / 2][SIZE / 2],
    );
}
