use super::*;
use windows::Win32::{
    Foundation::HMODULE,
    Graphics::{
        Direct3D::{D3D11_SRV_DIMENSION_TEXTURE2D, D3D_DRIVER_TYPE_WARP},
        Dxgi::IDXGIAdapter,
    },
};

const PACKED_HLSL: &str = r#"
Texture2D<float4> image : register(t0);
SamplerState image_sampler : register(s0);
RWBuffer<uint> result : register(u0);

uint4 read_texel(uint i) {
    const float2 center = float2(0.5, 0.5);
    uint mip = i & 1;
    float4 rgba = i < 2 ? image.SampleLevel(image_sampler, center, mip)
                        : image.Load(int3(0, 0, mip));
    return (uint4)round(rgba * 255);
}

[numthreads(4, 1, 1)]
void cs_main(uint3 index : SV_DispatchThreadID) {
    uint4 value = read_texel(index.x);
    [unroll] for (uint i = 0; i < 4; i++) {
        result[index.x * 4 + i] = value[i];
    }
}

uint4 ps_main(float4 position : SV_Position) : SV_Target {
    uint i = (uint)position.x;
    float4 implicit_sample = image.Sample(image_sampler, float2(0.5, 0.5));
    return i < 4 ? read_texel(i) : (uint4)round(implicit_sample * 255);
}
"#;

#[test]
#[ignore = "requires exclusive access to a physical FL11_1 adapter"]
fn tier_fl11_1_packed_sample_level_stages() {
    let (_, owner) = exact_hardware();
    unsafe {
        owner.with_immediate_context(|ctx| check_packed_sample_level_stages(owner.device(), ctx))
    };
}

#[test]
#[ignore = "optional WARP software control for the packed-format investigation"]
fn packed_sample_level_warp_control() {
    let (mut device, mut context, mut level) = (None, None, Default::default());
    unsafe {
        D3D11CreateDevice(
            None::<&IDXGIAdapter>,
            D3D_DRIVER_TYPE_WARP,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_FLAG(0),
            Some(&[D3D_FEATURE_LEVEL_11_1]),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut level),
            Some(&mut context),
        )
        .unwrap();
    }
    assert_eq!(level, D3D_FEATURE_LEVEL_11_1);
    std::println!("software control: WARP requested FL11_1, returned {level:?}");
    check_packed_sample_level_stages(
        &device.unwrap().cast().unwrap(),
        &context.unwrap().cast().unwrap(),
    );
}

fn check_packed_sample_level_stages(device: &ID3D11Device1, ctx: &ID3D11DeviceContext1) {
    let vertex_code = compile(Stage::Vertex, HLSL, "simple_vertex");
    let pixel_code = compile(Stage::Pixel, PACKED_HLSL, "ps_main");
    let compute_code = compile(Stage::Compute, PACKED_HLSL, "cs_main");

    let target_desc = D3D11_TEXTURE2D_DESC {
        Width: 5,
        Height: 1,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_R8G8B8A8_UINT,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
        ..Default::default()
    };
    let target = texture(device, target_desc);
    let storage = buffer(
        device,
        D3D11_BUFFER_DESC {
            ByteWidth: 64,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            ..Default::default()
        },
        None,
    );
    let uavs = [Some(buffer_uav(device, &storage, 16, false))];
    let (mut vertex, mut pixel, mut compute, mut target_view, mut rasterizer) =
        (None, None, None, None, None);
    unsafe {
        device
            .CreateVertexShader(&vertex_code, None, Some(&mut vertex))
            .unwrap();
        device
            .CreatePixelShader(&pixel_code, None, Some(&mut pixel))
            .unwrap();
        device
            .CreateComputeShader(&compute_code, None, Some(&mut compute))
            .unwrap();
        device
            .CreateRenderTargetView(&target, None, Some(&mut target_view))
            .unwrap();
        device
            .CreateRasterizerState1(
                &D3D11_RASTERIZER_DESC1 {
                    FillMode: D3D11_FILL_SOLID,
                    CullMode: D3D11_CULL_NONE,
                    DepthClipEnable: true.into(),
                    ..Default::default()
                },
                Some(&mut rasterizer),
            )
            .unwrap();
    }
    let rtvs = [target_view];
    let mut failed = Vec::new();
    unsafe {
        for (format, base, tail, base_rgba, tail_rgba) in [
            (
                DXGI_FORMAT_B5G6R5_UNORM,
                0xf800u16,
                0x07e0u16,
                [255, 0, 0, 255],
                [0, 255, 0, 255],
            ),
            (
                DXGI_FORMAT_B5G5R5A1_UNORM,
                0x801f,
                0xfc00,
                [0, 0, 255, 255],
                [255, 0, 0, 255],
            ),
            (
                DXGI_FORMAT_B4G4R4A4_UNORM,
                0xf0f0,
                0xf00f,
                [0, 255, 0, 255],
                [0, 0, 255, 255],
            ),
        ] {
            let resource = texture(
                device,
                D3D11_TEXTURE2D_DESC {
                    Width: 2,
                    Height: 2,
                    MipLevels: 2,
                    Format: format,
                    BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                    ..target_desc
                },
            );
            ctx.UpdateSubresource(&resource, 0, None, [base; 4].as_ptr().cast(), 4, 0);
            ctx.UpdateSubresource(&resource, 1, None, (&raw const tail).cast(), 2, 0);
            for first_mip in 0..=1 {
                let mut view = None;
                device
                    .CreateShaderResourceView(
                        &resource,
                        Some(&D3D11_SHADER_RESOURCE_VIEW_DESC {
                            Format: format,
                            ViewDimension: D3D11_SRV_DIMENSION_TEXTURE2D,
                            Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                                Texture2D: D3D11_TEX2D_SRV {
                                    MostDetailedMip: first_mip,
                                    MipLevels: 2 - first_mip,
                                },
                            },
                        }),
                        Some(&mut view),
                    )
                    .unwrap();
                let views = [view];
                let expected = if first_mip == 0 {
                    [base_rgba, tail_rgba, base_rgba, tail_rgba, base_rgba]
                } else {
                    let missing_alpha = if format == DXGI_FORMAT_B5G6R5_UNORM {
                        255
                    } else {
                        0
                    };
                    [
                        tail_rgba,
                        tail_rgba,
                        tail_rgba,
                        [0, 0, 0, missing_alpha],
                        tail_rgba,
                    ]
                };
                for (filter_name, filter) in [
                    ("point", D3D11_FILTER_MIN_MAG_MIP_POINT),
                    ("linear", D3D11_FILTER_MIN_MAG_MIP_LINEAR),
                ] {
                    let mut sampler = None;
                    device
                        .CreateSamplerState(
                            &D3D11_SAMPLER_DESC {
                                Filter: filter,
                                AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                                AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                                AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                                MaxAnisotropy: 1,
                                ComparisonFunc: D3D11_COMPARISON_ALWAYS,
                                MaxLOD: f32::MAX,
                                ..Default::default()
                            },
                            Some(&mut sampler),
                        )
                        .unwrap();
                    let samplers = [sampler];
                    ctx.CSSetShader(compute.as_ref(), None);
                    ctx.CSSetShaderResources(0, Some(&views));
                    ctx.CSSetSamplers(0, Some(&samplers));
                    ctx.CSSetUnorderedAccessViews(0, 1, Some(uavs.as_ptr()), None);
                    ctx.Dispatch(1, 1, 1);
                    ctx.ClearState();
                    let compute_values = read_buffer(device, ctx, &storage, 16);

                    ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                    ctx.VSSetShader(vertex.as_ref(), None);
                    ctx.PSSetShader(pixel.as_ref(), None);
                    ctx.PSSetShaderResources(0, Some(&views));
                    ctx.PSSetSamplers(0, Some(&samplers));
                    ctx.RSSetState(rasterizer.as_deref());
                    ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                        Width: 5.0,
                        Height: 1.0,
                        MaxDepth: 1.0,
                        ..Default::default()
                    }]));
                    ctx.OMSetRenderTargets(Some(&rtvs), None);
                    ctx.Draw(3, 0);
                    ctx.ClearState();
                    let pixel_values = read_texture_row(device, ctx, &target, target_desc);
                    let expected_compute = expected[..4].iter().flatten().copied().map(u32::from);
                    let compute_ok = compute_values.iter().copied().eq(expected_compute);
                    let pixel_ok = pixel_values == expected;
                    std::println!(
                            "{format:?} first_mip={first_mip} filter={filter_name} compute={compute_values:?} pixel={pixel_values:?} expected={expected:?}"
                        );
                    if !compute_ok || !pixel_ok {
                        failed.push((format, first_mip, filter_name, compute_ok, pixel_ok));
                    }
                }
            }
        }
    }
    assert!(
        failed.is_empty(),
        "packed stage sampling mismatches: {failed:?}"
    );
}
