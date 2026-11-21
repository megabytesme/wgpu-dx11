use super::*;
use std::time::Instant;
use windows::{
    core::BOOL,
    Win32::Graphics::Dxgi::{
        IDXGIAdapter3, IDXGIDevice, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
        DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, DXGI_QUERY_VIDEO_MEMORY_INFO,
    },
};

const WIDTH: u32 = 512;
const HEIGHT: u32 = 256;
const DRAW_COUNT: u32 = 64;
const VERTICES: [[f32; 4]; 3] = [
    [-1.0, 1.0, 0.5, 1.0],
    [3.0, 1.0, 0.5, 1.0],
    [-1.0, -3.0, 0.5, 1.0],
];

fn memory(device: &ID3D11Device) -> Result<[u64; 2]> {
    let adapter = unsafe { device.cast::<IDXGIDevice>()?.GetAdapter()? }.cast::<IDXGIAdapter3>()?;
    let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
    unsafe { adapter.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL, &mut info)? };
    let local = info.CurrentUsage;
    unsafe { adapter.QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL, &mut info)? };
    Ok([local, info.CurrentUsage])
}

#[test]
#[ignore = "requires an exclusively scheduled physical D3D11 adapter"]
fn exact_fl10_matched_native_render_benchmark() -> Result<()> {
    let adapter = hardware();
    let levels = [FeatureLevel::L9_1, FeatureLevel::L10_0, FeatureLevel::L10_1];
    for round in 0..levels.len() {
        for offset in 0..levels.len() {
            let level = levels[(round + offset) % levels.len()];
            let exact = open(&adapter, level);
            let vs = exact
                .compile_hlsl(
                    "float4 main(float4 p : POSITION) : SV_Position { return p; }",
                    "main",
                    ShaderStage::Vertex,
                )
                .unwrap();
            let ps = exact
                .compile_hlsl(
                    "float4 main() : SV_Target { return float4(.25, .75, 1, 1); }",
                    "main",
                    ShaderStage::Pixel,
                )
                .unwrap();
            unsafe {
                exact.with_native(|device, ctx| measure(device, ctx, level, round, &vs, &ps))?
            };
        }
    }
    Ok(())
}

fn measure(
    device: &ID3D11Device,
    ctx: &ID3D11DeviceContext,
    level: FeatureLevel,
    round: usize,
    vs: &[u8],
    ps: &[u8],
) -> Result<()> {
    let initial_memory = memory(device).ok();
    let (image, rtv) = render_target(device, WIDTH, HEIGHT);
    let depth = unsafe {
        texture(
            device,
            &D3D11_TEXTURE2D_DESC {
                BindFlags: D3D11_BIND_DEPTH_STENCIL.0 as u32,
                ..texture_desc(DXGI_FORMAT_D24_UNORM_S8_UINT, WIDTH, HEIGHT)
            },
            None,
        )
    };
    let mut dsv = None;
    let mut layout = None;
    let mut buffer = None;
    let mut depth_state = None;
    let layout_desc = D3D11_INPUT_ELEMENT_DESC {
        SemanticName: PCSTR(c"POSITION".as_ptr().cast()),
        Format: DXGI_FORMAT_R32G32B32A32_FLOAT,
        ..Default::default()
    };
    let vs_shader = vertex_shader(device, vs)?;
    let ps = pixel_shader(device, ps)?;
    let fence = query(device, D3D11_QUERY_EVENT)?;
    let timestamps = (
        query(device, D3D11_QUERY_TIMESTAMP_DISJOINT),
        query(device, D3D11_QUERY_TIMESTAMP),
        query(device, D3D11_QUERY_TIMESTAMP),
    );
    unsafe {
        device.CreateDepthStencilView(&depth, None, Some(&mut dsv))?;
        device.CreateInputLayout(&[layout_desc], vs, Some(&mut layout))?;
        device.CreateBuffer(
            &D3D11_BUFFER_DESC {
                ByteWidth: size_of_val(&VERTICES) as u32,
                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                ..Default::default()
            },
            Some(&D3D11_SUBRESOURCE_DATA {
                pSysMem: VERTICES.as_ptr().cast(),
                ..Default::default()
            }),
            Some(&mut buffer),
        )?;
        let face = D3D11_DEPTH_STENCILOP_DESC {
            StencilFailOp: D3D11_STENCIL_OP_KEEP,
            StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
            StencilPassOp: D3D11_STENCIL_OP_KEEP,
            StencilFunc: D3D11_COMPARISON_ALWAYS,
        };
        device.CreateDepthStencilState(
            &D3D11_DEPTH_STENCIL_DESC {
                DepthEnable: true.into(),
                DepthWriteMask: D3D11_DEPTH_WRITE_MASK_ZERO,
                DepthFunc: D3D11_COMPARISON_LESS,
                FrontFace: face,
                BackFace: face,
                ..Default::default()
            },
            Some(&mut depth_state),
        )?;
        ctx.ClearDepthStencilView(dsv.as_ref(), D3D11_CLEAR_DEPTH.0, 1.0, 0);
        ctx.IASetInputLayout(layout.as_ref());
        ctx.IASetVertexBuffers(0, 1, Some(&buffer), Some(&16), Some(&0));
        ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        ctx.RSSetViewports(Some(&[viewport(WIDTH as f32, HEIGHT as f32)]));
        ctx.OMSetDepthStencilState(depth_state.as_ref(), 0);
        ctx.OMSetRenderTargets(Some(&[Some(rtv)]), dsv.as_ref());
        ctx.VSSetShader(&vs_shader, None);
        ctx.PSSetShader(&ps, None);
        for _ in 0..DRAW_COUNT {
            ctx.Draw(3, 0);
        }
        ctx.End(&fence);
        ctx.Flush();
    }
    let mut completed = BOOL::default();
    wait_for_query_data(ctx, &fence, &mut completed)?;
    assert!(completed.as_bool());
    let ready_memory = memory(device).ok();
    let start = Instant::now();
    unsafe {
        if let (Ok(disjoint), Ok(before), Ok(_)) = &timestamps {
            ctx.Begin(disjoint);
            ctx.End(before);
        }
        for _ in 0..DRAW_COUNT {
            ctx.Draw(3, 0);
        }
        if let (Ok(disjoint), Ok(_), Ok(after)) = &timestamps {
            ctx.End(after);
            ctx.End(disjoint);
        }
        ctx.End(&fence);
        ctx.Flush();
    }
    let submit_us = start.elapsed().as_secs_f64() * 1e6;
    let queued_memory = memory(device).ok();
    wait_for_query_data(ctx, &fence, &mut completed)?;
    let complete_us = start.elapsed().as_secs_f64() * 1e6;
    assert!(completed.as_bool());
    let finished_memory = memory(device).ok();
    let gpu_us = if let (Ok(disjoint), Ok(before), Ok(after)) = timestamps {
        let mut duration = D3D11_QUERY_DATA_TIMESTAMP_DISJOINT::default();
        wait_for_query_data(ctx, &disjoint, &mut duration)?;
        let mut first = 0u64;
        let mut last = 0u64;
        wait_for_query_data(ctx, &before, &mut first)?;
        wait_for_query_data(ctx, &after, &mut last)?;
        last.checked_sub(first)
            .filter(|_| !duration.Disjoint.as_bool() && duration.Frequency != 0)
            .map(|ticks| ticks as f64 * 1e6 / duration.Frequency as f64)
    } else {
        None
    };
    unsafe { ctx.OMSetRenderTargets(None, None) };
    let pixels = read_pixels(device, ctx, &image, WIDTH, HEIGHT)?;
    assert!(pixels.iter().all(|pixel| *pixel == [64, 191, 255, 255]));
    std::println!(
        "BENCHMARK level={level:?} round={} submit_us={submit_us:.3} complete_us={complete_us:.3} gpu_us={gpu_us:?} memory_bytes={:?}",
        round + 1, [initial_memory, ready_memory, queued_memory, finished_memory]
    );
    Ok(())
}
