use core::mem::size_of_val;
use windows::{
    core::{Error, Result, PCSTR},
    Win32::{
        Foundation::E_FAIL,
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Direct3D11::{
                ID3D11ClassLinkage, ID3D11DepthStencilView, ID3D11Device, ID3D11DeviceContext,
                D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER,
                D3D11_BUFFER_DESC, D3D11_COMPARISON_NEVER, D3D11_CPU_ACCESS_READ, D3D11_CULL_NONE,
                D3D11_FILL_SOLID, D3D11_FILTER_MIN_MAG_MIP_POINT, D3D11_INPUT_ELEMENT_DESC,
                D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ,
                D3D11_RASTERIZER_DESC, D3D11_SAMPLER_DESC, D3D11_SUBRESOURCE_DATA,
                D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE_ADDRESS_MIRROR,
                D3D11_TEXTURE_ADDRESS_WRAP, D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
                D3D11_VIEWPORT,
            },
            Dxgi::Common::{
                DXGI_FORMAT_R32G32_FLOAT, DXGI_FORMAT_R8G8B8A8_UNORM, DXGI_SAMPLE_DESC,
            },
        },
    },
};

use super::{
    capabilities::{query_full_npot, Query},
    enumerate_native_adapters, hardware_test,
    shader::{compile_hlsl, ShaderProfile},
    FeatureLevel,
};

const WIDTH: usize = 512;
const HEIGHT: usize = 4;
const SOURCE: &str = r#"
struct Output {
    // Keep the interpolator at the same signature register as the pixel input.
    float2 uv : TEXCOORD0;
    float4 pos : SV_Position;
};
Output vertex_main(float2 pos : POSITION, float2 uv : TEXCOORD0) {
    Output result;
    result.pos = float4(pos, 0.5, 1.0);
    result.uv = uv;
    return result;
}
Texture2D image : register(t0);
SamplerState address_sampler : register(s0);
float4 pixel_main(float2 uv : TEXCOORD0) : SV_Target {
    return image.Sample(address_sampler, uv);
}
"#;

#[test]
fn fl91_native_128_texture_repeats_and_conditional_npot() {
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let adapters = enumerate_native_adapters().unwrap();
    let Some(adapter) = hardware_test::select_adapter(&adapters, required).unwrap() else {
        return;
    };
    let device = match adapter.open_device(&[FeatureLevel::L9_1]) {
        Ok(device) => device,
        Err(error) if !required => {
            std::eprintln!("skipping native addressing: exact FL9_1 unavailable ({error})");
            return;
        }
        Err(error) => panic!("opening hardware at exact FL9_1 failed: {error}"),
    };
    assert_eq!(device.feature_level(), FeatureLevel::L9_1);
    let full_npot = query_full_npot(&device);
    std::eprintln!(
        "native addressing adapter={:?} driver={:?} requested=0x9100 returned={:#x} full_npot={full_npot:?}",
        adapter.identity(),
        adapter.driver_version(),
        device.feature_level().as_raw(),
    );

    let vs = compile_hlsl(SOURCE, "vertex_main", ShaderProfile::VertexLevel9_1).unwrap();
    let ps = compile_hlsl(SOURCE, "pixel_main", ShaderProfile::PixelLevel9_1).unwrap();
    // No resource or immediate context escapes the callback.
    unsafe {
        device.with_native_context(|raw, context| {
            check_addressing(
                raw,
                context,
                &vs,
                &ps,
                matches!(full_npot, Query::Value(false)),
            )
        })
    }
    .unwrap();
}

fn texel(x: usize, y: usize) -> [u8; 4] {
    [19 + 53 * x as u8, 31 + 109 * y as u8, 71, 255]
}

fn sample_column(
    mode: windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE_ADDRESS_MODE,
    u: f64,
    width: usize,
) -> usize {
    let normalized = match mode {
        D3D11_TEXTURE_ADDRESS_WRAP => u.rem_euclid(1.0),
        D3D11_TEXTURE_ADDRESS_MIRROR => 1.0 - (u.rem_euclid(2.0) - 1.0).abs(),
        _ => u.clamp(0.0, 1.0),
    };
    ((normalized * width as f64) as usize).min(width - 1)
}

fn check_addressing(
    raw: &ID3D11Device,
    context: &ID3D11DeviceContext,
    vs_code: &[u8],
    ps_code: &[u8],
    conditional_npot: bool,
) -> Result<()> {
    let mut vs = None;
    let mut ps = None;
    let mut layout = None;
    let mut vertices = None;
    let mut rasterizer = None;
    let mut target = None;
    let mut view = None;
    let mut staging = None;

    let buffer_desc = D3D11_BUFFER_DESC {
        ByteWidth: size_of_val(&[[0.0f32; 4]; 6]) as u32,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
        ..Default::default()
    };
    let layout_desc = [
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
            Format: DXGI_FORMAT_R32G32_FLOAT,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            ..Default::default()
        },
        D3D11_INPUT_ELEMENT_DESC {
            SemanticName: PCSTR(c"TEXCOORD".as_ptr().cast()),
            Format: DXGI_FORMAT_R32G32_FLOAT,
            AlignedByteOffset: 8,
            InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
            ..Default::default()
        },
    ];
    let target_desc = D3D11_TEXTURE2D_DESC {
        Width: WIDTH as u32,
        Height: HEIGHT as u32,
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
        raw.CreateBuffer(&buffer_desc, None, Some(&mut vertices))?;
        raw.CreateRasterizerState(&rasterizer_desc, Some(&mut rasterizer))?;
        raw.CreateTexture2D(&target_desc, None, Some(&mut target))?;
        raw.CreateTexture2D(&staging_desc, None, Some(&mut staging))?;
        let target = target.expect("D3D11 render target");
        let staging = staging.expect("D3D11 staging texture");
        let vertices = vertices.expect("D3D11 vertex buffer");
        raw.CreateRenderTargetView(&target, None, Some(&mut view))?;
        let view = view.expect("D3D11 render-target view");
        let targets = [Some(view.clone())];
        context.RSSetState(rasterizer.as_ref());
        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
            Width: WIDTH as f32,
            Height: HEIGHT as f32,
            MaxDepth: 1.0,
            ..Default::default()
        }]));
        context.IASetInputLayout(layout.as_ref());
        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        context.IASetVertexBuffers(0, 1, Some(&Some(vertices.clone())), Some(&16), Some(&0));
        context.VSSetShader(vs.as_ref(), None);
        context.PSSetShader(ps.as_ref(), None);

        let mut failed_cases = 0;
        for source_width in [4, 3] {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: source_width,
                Height: 2,
                BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
                ..target_desc
            };
            let source_bytes: alloc::vec::Vec<_> = (0..2)
                .flat_map(|row| (0..source_width as usize).map(move |x| texel(x, row)))
                .collect();
            let init = D3D11_SUBRESOURCE_DATA {
                pSysMem: source_bytes.as_ptr().cast(),
                SysMemPitch: source_width * 4,
                ..Default::default()
            };
            let mut source = None;
            let mut srv = None;
            raw.CreateTexture2D(&desc, Some(&init), Some(&mut source))?;
            raw.CreateShaderResourceView(
                &source.expect("D3D11 sampled texture"),
                None,
                Some(&mut srv),
            )?;
            context.PSSetShaderResources(0, Some(&[srv]));
            if source_width == 3 && conditional_npot {
                let invalid_desc = D3D11_TEXTURE2D_DESC {
                    MipLevels: 2,
                    ..desc
                };
                assert!(raw
                    .CreateTexture2D(&invalid_desc, None, Some(&mut None))
                    .is_err());
                std::eprintln!("native conditional NPOT rejected a 3x2 two-mip texture");
            }
            let modes = if source_width == 4 {
                &[
                    D3D11_TEXTURE_ADDRESS_WRAP,
                    D3D11_TEXTURE_ADDRESS_MIRROR,
                    D3D11_TEXTURE_ADDRESS_CLAMP,
                ][..]
            } else {
                &[D3D11_TEXTURE_ADDRESS_CLAMP][..]
            };
            for &mode_u in modes {
                // V=1.25 selects the first row with Wrap, and the last row with Clamp.
                let mode_v = if mode_u == D3D11_TEXTURE_ADDRESS_MIRROR {
                    D3D11_TEXTURE_ADDRESS_WRAP
                } else {
                    D3D11_TEXTURE_ADDRESS_CLAMP
                };
                let sampler_desc = D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_POINT,
                    AddressU: mode_u,
                    AddressV: mode_v,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    MaxAnisotropy: 1,
                    ComparisonFunc: D3D11_COMPARISON_NEVER,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                };
                let mut sampler = None;
                raw.CreateSamplerState(&sampler_desc, Some(&mut sampler))?;
                context.PSSetSamplers(0, Some(&[sampler]));
                for (left, right, v) in [
                    (0.0f32, 2.0f32, 1.25f32),
                    (0.0, 2.0, 0.75),
                    (0.0, 128.0, 1.25),
                    (-128.0, 0.0, 1.25),
                    (0.0, 128.0, 0.75),
                    (-128.0, 0.0, 0.75),
                ] {
                    let data = [
                        [-1.0, -1.0, left, v],
                        [-1.0, 1.0, left, v],
                        [1.0, -1.0, right, v],
                        [1.0, -1.0, right, v],
                        [-1.0, 1.0, left, v],
                        [1.0, 1.0, right, v],
                    ];
                    context.UpdateSubresource(&vertices, 0, None, data.as_ptr().cast(), 0, 0);
                    context.ClearRenderTargetView(&view, &[0.0; 4]);
                    context.OMSetRenderTargets(Some(&targets), None::<&ID3D11DepthStencilView>);
                    context.Draw(6, 0);
                    context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
                    context.CopyResource(&staging, &target);
                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
                    if mapped.pData.is_null() || mapped.RowPitch < (WIDTH * 4) as u32 {
                        context.Unmap(&staging, 0);
                        return Err(Error::from_hresult(E_FAIL));
                    }
                    let mut pixels = [[[0u8; 4]; WIDTH]; HEIGHT];
                    for (row, destination) in pixels.iter_mut().enumerate() {
                        core::ptr::copy_nonoverlapping(
                            mapped
                                .pData
                                .cast::<u8>()
                                .add(row * mapped.RowPitch as usize),
                            destination.as_mut_ptr().cast(),
                            WIDTH * 4,
                        );
                    }
                    context.Unmap(&staging, 0);
                    let row_index = sample_column(mode_v, f64::from(v), 2);
                    let mut mismatch_count = 0;
                    let mut first_mismatch = None;
                    for (y, row) in pixels.iter().enumerate() {
                        for (x, &actual) in row.iter().enumerate() {
                            let u = f64::from(left)
                                + (x as f64 + 0.5) * f64::from(right - left) / WIDTH as f64;
                            let column = sample_column(mode_u, u, source_width as usize);
                            let expected = texel(column, row_index);
                            if actual != expected {
                                mismatch_count += 1;
                                first_mismatch.get_or_insert((x, y, actual, expected));
                            }
                        }
                    }
                    failed_cases += usize::from(mismatch_count != 0);
                    std::eprintln!(
                        "native addressing width={source_width} U={mode_u:?} V={mode_v:?} span={left}..{right} v={v} mismatches={mismatch_count}/{} first_mismatch={first_mismatch:?} first={:?} last={:?}",
                        WIDTH * HEIGHT, pixels[0][0], pixels[HEIGHT - 1][WIDTH - 1],
                    );
                }
            }
        }
        assert_eq!(failed_cases, 0, "native addressing pixel checks failed");
    }
    Ok(())
}
