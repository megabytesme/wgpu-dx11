use super::Sm5Stage;
use crate::dx11::{self, Device, FeatureLevel, Queue};
use alloc::{string::String, sync::Arc, vec::Vec};
use core::{mem::size_of, ptr, time::Duration};
use std::time::Instant;
use windows::{
    core::{Interface, Result},
    Win32::{
        Foundation::{E_FAIL, S_FALSE, S_OK},
        Graphics::{
            Direct3D::{
                Fxc::D3DDisassemble, D3D_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST,
                D3D_SRV_DIMENSION_BUFFER, D3D_SRV_DIMENSION_TEXTURE2D,
            },
            Direct3D11::*,
            Dxgi::{Common::*, DXGI_ERROR_WAS_STILL_DRAWING},
        },
    },
};

const HLSL: &str = include_str!("fl11_0_tests.hlsl");
const GRAPHICS_HLSL: &str = include_str!("fl11_0_graphics_tests.hlsl");

fn device() -> Option<(Device, Queue, dx11::NativeAdapter)> {
    let adapters = dx11::enumerate_native_adapters().unwrap();
    for adapter in adapters {
        let Ok(native) = adapter.open_exact_hardware_device(FeatureLevel::L11_0) else {
            continue;
        };
        assert!(!adapter.identity().software);
        assert_eq!(native.feature_level(), FeatureLevel::L11_0);
        let observed = unsafe { native.raw_device().GetFeatureLevel() }.0 as u32;
        assert_eq!(observed, FeatureLevel::L11_0.as_raw());
        let identity = adapter.identity();
        std::println!(
            "exact requested FL11_0, active {:?}, GetFeatureLevel {observed:#06x}: {:?}, PCI {:04x}:{:04x}, LUID {:08x}:{:08x}, driver {:?}, arch {}",
            native.feature_level(),
            identity.name,
            identity.vendor_id,
            identity.device_id,
            identity.luid.HighPart as u32,
            identity.luid.LowPart,
            adapter.driver_version(),
            std::env::consts::ARCH,
        );
        let native = Arc::new(native);
        return Some((Device::new(native.clone()), Queue::new(native), adapter));
    }
    assert_ne!(
        std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").as_deref(),
        Some(std::ffi::OsStr::new("1")),
        "exact physical FL11_0 is required"
    );
    std::println!("SKIP: no exact physical FL11_0 device");
    None
}

fn shader(stage: Sm5Stage, entry: &str, profile: &str) -> Vec<u8> {
    let source = if stage == Sm5Stage::Compute {
        HLSL
    } else {
        GRAPHICS_HLSL
    };
    let code = stage.compile(source, entry).unwrap();
    let disassembly = unsafe {
        D3DDisassemble(
            code.as_ptr().cast(),
            code.len(),
            0,
            windows::core::PCSTR::null(),
        )
        .unwrap()
    };
    let bytes = unsafe {
        core::slice::from_raw_parts(
            disassembly.GetBufferPointer().cast::<u8>(),
            disassembly.GetBufferSize(),
        )
    };
    let text = String::from_utf8_lossy(bytes);
    assert!(text.lines().any(|line| line.trim() == profile), "{text}");
    std::println!("FXC {entry}: {profile}; {} bytes DXBC", code.len());
    for line in text.lines().filter(|line| {
        let line = line.trim_start();
        line.starts_with("dcl_thread_group ")
            || line.starts_with("dcl_tgsm_")
            || line.starts_with("dcl_uav_")
    }) {
        std::println!("  {line}");
    }
    code
}

#[test]
fn tier_fl11_0_compiler_rejects_excess_threads_and_bad_entry() {
    let bad_z = "RWBuffer<uint> output_value; [numthreads(1, 1, 65)] void f(uint i : SV_GroupIndex) { output_value[i] = i; }";
    assert!(Sm5Stage::Compute.compile(bad_z, "f").is_err());
    assert!(Sm5Stage::Compute.compile(HLSL, "produce\0").is_err());
    assert!(Sm5Stage::Hull.compile(HLSL, "produce").is_err());
}

#[test]
fn tier_fl11_0_native_queue_rejects_foreign_lists_and_reports_device_loss() {
    let Some((device, queue, adapter)) = device() else {
        return;
    };
    let native = device.native_fl11_0().unwrap();
    let foreign = Arc::new(
        adapter
            .open_exact_hardware_device(FeatureLevel::L11_0)
            .unwrap(),
    );
    let foreign_queue = Queue::new(foreign);
    let commands = unsafe { native.record(|_| Ok(())) }.unwrap();
    assert_eq!(
        foreign_queue.submit_native_fl11_0(commands),
        Err(crate::DeviceError::Unexpected)
    );

    let discarded = unsafe { native.record(|_| Err(windows::core::Error::from_hresult(E_FAIL))) };
    assert_eq!(discarded.unwrap_err().code(), E_FAIL);

    let commands = unsafe { native.record(|_| Ok(())) }.unwrap();
    device.test_fixture_remove_device_on_next_oom_check();
    assert_eq!(
        queue.submit_native_fl11_0(commands),
        Err(crate::DeviceError::Lost)
    );
    let commands = unsafe { native.record(|_| Ok(())) }.unwrap();
    queue.submit_native_fl11_0(commands).unwrap();
    unsafe { crate::Queue::wait_for_idle(&queue) }.unwrap();
    std::println!(
        "foreign command list rejected, recording error discarded, injected device loss propagated"
    );
}

fn buffer(device: &ID3D11Device, desc: D3D11_BUFFER_DESC, initial: Option<&[u32]>) -> ID3D11Buffer {
    let data = initial.map(|words| {
        assert_eq!(size_of_val(words), desc.ByteWidth as usize);
        D3D11_SUBRESOURCE_DATA {
            pSysMem: words.as_ptr().cast(),
            ..Default::default()
        }
    });
    let mut result = None;
    unsafe {
        device
            .CreateBuffer(&desc, data.as_ref().map(ptr::from_ref), Some(&mut result))
            .unwrap();
    }
    result.unwrap()
}

fn uav(
    device: &ID3D11Device,
    resource: &ID3D11Buffer,
    format: DXGI_FORMAT,
    size: u32,
    flags: u32,
) -> ID3D11UnorderedAccessView {
    let desc = D3D11_UNORDERED_ACCESS_VIEW_DESC {
        Format: format,
        ViewDimension: D3D11_UAV_DIMENSION_BUFFER,
        Anonymous: D3D11_UNORDERED_ACCESS_VIEW_DESC_0 {
            Buffer: D3D11_BUFFER_UAV {
                FirstElement: 0,
                NumElements: size,
                Flags: flags,
            },
        },
    };
    let mut view = None;
    unsafe {
        device
            .CreateUnorderedAccessView(resource, Some(&desc), Some(&mut view))
            .unwrap();
    }
    view.unwrap()
}

fn map_read<R: windows::core::Param<ID3D11Resource> + Copy>(
    context: &ID3D11DeviceContext,
    resource: R,
) -> Result<D3D11_MAPPED_SUBRESOURCE> {
    let mut map = D3D11_MAPPED_SUBRESOURCE::default();
    let timeout = Instant::now() + Duration::from_secs(5);
    unsafe { context.Flush() };
    loop {
        match unsafe {
            context.Map(
                resource,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut map),
            )
        } {
            Ok(()) => return Ok(map),
            Err(error)
                if error.code() == DXGI_ERROR_WAS_STILL_DRAWING && Instant::now() < timeout =>
            {
                std::thread::yield_now();
            }
            Err(error) => return Err(error),
        }
    }
}

#[test]
fn tier_fl11_0_compute_uav_atomic_barrier_and_indirect_readback() {
    let Some((device, queue, adapter)) = device() else {
        return;
    };
    let native = device.native_fl11_0().unwrap();
    let raw = native.device();
    let producer = shader(Sm5Stage::Compute, "produce", "cs_5_0");
    let consumer = shader(Sm5Stage::Compute, "consume", "cs_5_0");
    let prior = adapter
        .open_exact_hardware_device(FeatureLevel::L10_1)
        .unwrap();
    assert!(unsafe {
        prior
            .raw_device()
            .CreateComputeShader(&producer, None::<&ID3D11ClassLinkage>, None)
    }
    .is_err());
    let prior = Device::new(Arc::new(prior));
    assert!(prior.native_fl11_0().is_none());

    let mut producer_shader = None;
    let mut consumer_shader = None;
    unsafe {
        raw.CreateComputeShader(
            &producer,
            None::<&ID3D11ClassLinkage>,
            Some(&mut producer_shader),
        )
        .unwrap();
        raw.CreateComputeShader(
            &consumer,
            None::<&ID3D11ClassLinkage>,
            Some(&mut consumer_shader),
        )
        .unwrap();
    }
    let args = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 12,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            MiscFlags: (D3D11_RESOURCE_MISC_DRAWINDIRECT_ARGS
                | D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS)
                .0 as u32,
            ..Default::default()
        },
        Some(&[0, 0, 0]),
    );
    let sums = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 4,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_UNORDERED_ACCESS | D3D11_BIND_SHADER_RESOURCE).0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
            StructureByteStride: 4,
            ..Default::default()
        },
        Some(&[0]),
    );
    let typed = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 4,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            ..Default::default()
        },
        Some(&[9]),
    );
    let staging = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 4,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        },
        None,
    );
    let raw_uav = uav(
        raw,
        &args,
        DXGI_FORMAT_R32_TYPELESS,
        3,
        D3D11_BUFFER_UAV_FLAG_RAW.0 as u32,
    );
    let structured_uav = uav(raw, &sums, DXGI_FORMAT_UNKNOWN, 1, 0);
    let typed_uav = uav(raw, &typed, DXGI_FORMAT_R32_UINT, 1, 0);
    let mut structured_srv = None;
    let srv_desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_UNKNOWN,
        ViewDimension: D3D_SRV_DIMENSION_BUFFER,
        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
            Buffer: D3D11_BUFFER_SRV {
                Anonymous1: D3D11_BUFFER_SRV_0 { FirstElement: 0 },
                Anonymous2: D3D11_BUFFER_SRV_1 { NumElements: 1 },
            },
        },
    };
    unsafe {
        raw.CreateShaderResourceView(&sums, Some(&srv_desc), Some(&mut structured_srv))
            .unwrap();
    }

    let writes = unsafe {
        native.record(|context| {
            let mut slots = [const { None }; 8];
            slots[0] = Some(raw_uav);
            slots[7] = Some(structured_uav);
            context.CSSetUnorderedAccessViews(0, 8, Some(slots.as_ptr()), None);
            context.CSSetShader(producer_shader.as_ref(), None);
            context.Dispatch(1, 1, 1);
            Ok(())
        })
    }
    .unwrap();
    let reads = unsafe {
        native.record(|context| {
            context.CSSetShaderResources(0, Some(&[structured_srv]));
            context.CSSetUnorderedAccessViews(7, 1, Some([Some(typed_uav)].as_ptr()), None);
            context.CSSetShader(consumer_shader.as_ref(), None);
            context.DispatchIndirect(&args, 0);
            context.CSSetUnorderedAccessViews(7, 1, Some([None].as_ptr()), None);
            context.CopyResource(&staging, &typed);
            Ok(())
        })
    }
    .unwrap();
    queue.submit_native_fl11_0(writes).unwrap();
    queue.submit_native_fl11_0(reads).unwrap();
    let result = unsafe {
        native.readback(|context| {
            let map = map_read(context, &staging)?;
            let value = map.pData.cast::<u32>().read_unaligned();
            context.Unmap(&staging, 0);
            Ok(value)
        })
    }
    .unwrap();
    let expected = 8 * (1024 * 1023 / 2) + 28 * 1024 + 9;
    assert_eq!(result, expected);
    std::println!("indirect dispatch and typed/structured/raw UAV readback: {result}");
}

#[test]
fn tier_fl11_0_geometry_and_triangle_tessellation_readback() {
    let Some((device, queue, _)) = device() else {
        return;
    };
    let native = device.native_fl11_0().unwrap();
    let raw = native.device();
    let mut vs = None;
    let mut hs = None;
    let mut ds = None;
    let mut gs = None;
    let mut ps = None;
    let mut cs = None;
    let mut layout = None;
    let linkage = None::<&ID3D11ClassLinkage>;
    let vertex = shader(Sm5Stage::Vertex, "vertex", "vs_5_0");
    unsafe {
        raw.CreateVertexShader(&vertex, linkage, Some(&mut vs))
            .unwrap();
        raw.CreateInputLayout(
            &[
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: windows::core::PCSTR(c"POSITION".as_ptr().cast()),
                    Format: DXGI_FORMAT_R32G32_FLOAT,
                    InputSlotClass: D3D11_INPUT_PER_VERTEX_DATA,
                    ..Default::default()
                },
                D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: windows::core::PCSTR(c"SHIFT".as_ptr().cast()),
                    Format: DXGI_FORMAT_R32_FLOAT,
                    InputSlot: 1,
                    InputSlotClass: D3D11_INPUT_PER_INSTANCE_DATA,
                    InstanceDataStepRate: 1,
                    ..Default::default()
                },
            ],
            &vertex,
            Some(&mut layout),
        )
        .unwrap();
        raw.CreateComputeShader(
            &shader(Sm5Stage::Compute, "produce_draws", "cs_5_0"),
            linkage,
            Some(&mut cs),
        )
        .unwrap();
        raw.CreateHullShader(
            &shader(Sm5Stage::Hull, "hull", "hs_5_0"),
            linkage,
            Some(&mut hs),
        )
        .unwrap();
        raw.CreateDomainShader(
            &shader(Sm5Stage::Domain, "domain", "ds_5_0"),
            linkage,
            Some(&mut ds),
        )
        .unwrap();
        raw.CreateGeometryShader(
            &shader(Sm5Stage::Geometry, "geometry", "gs_5_0"),
            linkage,
            Some(&mut gs),
        )
        .unwrap();
        raw.CreatePixelShader(
            &shader(Sm5Stage::Pixel, "pixel", "ps_5_0"),
            linkage,
            Some(&mut ps),
        )
        .unwrap();
    }

    let positions = [-1.0f32, 1.0, -1.0, 1.0, -1.0, -3.0, 3.0, 1.0, 3.0, 1.0];
    let vertices = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: size_of_val(&positions) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            ..Default::default()
        },
        Some(&positions.map(f32::to_bits)),
    );
    let instances = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 8,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
            ..Default::default()
        },
        Some(&[8.0f32.to_bits(), 0]),
    );
    let indices = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 16,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_INDEX_BUFFER.0 as u32,
            ..Default::default()
        },
        Some(&[1, 2, 3, 4]),
    );
    let args = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 56,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            MiscFlags: (D3D11_RESOURCE_MISC_DRAWINDIRECT_ARGS
                | D3D11_RESOURCE_MISC_BUFFER_ALLOW_RAW_VIEWS)
                .0 as u32,
            ..Default::default()
        },
        Some(&[0; 14]),
    );
    let args_view = uav(
        raw,
        &args,
        DXGI_FORMAT_R32_TYPELESS,
        14,
        D3D11_BUFFER_UAV_FLAG_RAW.0 as u32,
    );

    let desc = D3D11_TEXTURE2D_DESC {
        Width: 8,
        Height: 8,
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
    let mut color = None;
    let mut staging = None;
    let mut view = None;
    let mut query = None;
    unsafe {
        raw.CreateTexture2D(&desc, None, Some(&mut color)).unwrap();
        raw.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                ..desc
            },
            None,
            Some(&mut staging),
        )
        .unwrap();
        raw.CreateRenderTargetView(color.as_ref().unwrap(), None, Some(&mut view))
            .unwrap();
        raw.CreateQuery(
            &D3D11_QUERY_DESC {
                Query: D3D11_QUERY_PIPELINE_STATISTICS,
                MiscFlags: 0,
            },
            Some(&mut query),
        )
        .unwrap();
    }
    let query: ID3D11Asynchronous = query.unwrap().cast().unwrap();
    let mut rasterizer = None;
    unsafe {
        raw.CreateRasterizerState(
            &D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_NONE,
                DepthClipEnable: true.into(),
                ..Default::default()
            },
            Some(&mut rasterizer),
        )
        .unwrap();
    }
    let color = color.unwrap();
    let staging = staging.unwrap();
    let prepare_args = unsafe {
        native.record(|context| {
            context.CSSetUnorderedAccessViews(0, 1, Some([Some(args_view)].as_ptr()), None);
            context.CSSetShader(cs.as_ref(), None);
            context.Dispatch(1, 1, 1);
            Ok(())
        })
    }
    .unwrap();
    let commands = unsafe {
        native.record(|context| {
            context.ClearRenderTargetView(view.as_ref(), &[1.0, 0.0, 0.0, 1.0]);
            context.OMSetRenderTargets(Some(&[view]), None::<&ID3D11DepthStencilView>);
            let left = D3D11_VIEWPORT {
                Width: 4.0,
                Height: 8.0,
                MaxDepth: 1.0,
                ..Default::default()
            };
            let right = D3D11_VIEWPORT {
                TopLeftX: 4.0,
                ..left
            };
            context.RSSetState(rasterizer.as_ref());
            context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_3_CONTROL_POINT_PATCHLIST);
            context.IASetInputLayout(layout.as_ref());
            context.IASetVertexBuffers(
                0,
                2,
                Some([Some(vertices), Some(instances)].as_ptr()),
                Some([8, 4].as_ptr()),
                Some([0, 0].as_ptr()),
            );
            context.IASetIndexBuffer(&indices, DXGI_FORMAT_R32_UINT, 0);
            context.VSSetShader(vs.as_ref(), None);
            context.HSSetShader(hs.as_ref(), None);
            context.DSSetShader(ds.as_ref(), None);
            context.GSSetShader(gs.as_ref(), None);
            context.PSSetShader(ps.as_ref(), None);
            context.Begin(&query);
            context.RSSetViewports(Some(&[left]));
            context.DrawInstancedIndirect(&args, 4);
            context.RSSetViewports(Some(&[right]));
            context.DrawIndexedInstancedIndirect(&args, 20);
            context.DrawInstancedIndirect(&args, 40);
            context.End(&query);
            context.OMSetRenderTargets(None, None::<&ID3D11DepthStencilView>);
            context.CopyResource(&staging, &color);
            Ok(())
        })
    }
    .unwrap();
    queue.submit_native_fl11_0(prepare_args).unwrap();
    queue.submit_native_fl11_0(commands).unwrap();
    let (pixels, stats) = unsafe {
        native.readback(|context| {
            let map = map_read(context, &staging)?;
            let mut pixels = [[0u8; 4]; 64];
            for row in 0..8 {
                ptr::copy_nonoverlapping(
                    map.pData.cast::<u8>().add(row * map.RowPitch as usize),
                    pixels[row * 8..].as_mut_ptr().cast(),
                    32,
                );
            }
            context.Unmap(&staging, 0);
            let mut stats = D3D11_QUERY_DATA_PIPELINE_STATISTICS::default();
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let hr = (context.vtable().GetData)(
                    context.as_raw(),
                    query.as_raw(),
                    ptr::from_mut(&mut stats).cast(),
                    size_of::<D3D11_QUERY_DATA_PIPELINE_STATISTICS>() as u32,
                    D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32,
                );
                match hr {
                    S_OK => break,
                    S_FALSE if Instant::now() < deadline => std::thread::yield_now(),
                    S_FALSE => return Err(windows::core::Error::from_hresult(E_FAIL)),
                    _ => hr.ok()?,
                }
            }
            Ok((pixels, stats))
        })
    }
    .unwrap();
    assert_eq!(pixels, [[0, 255, 0, 255]; 64]);
    assert_eq!(stats.IAPrimitives, 2);
    assert!(stats.HSInvocations > 0);
    assert!(stats.DSInvocations > 3);
    assert!(stats.GSInvocations > 1);
    assert_eq!(stats.GSInvocations, stats.GSPrimitives);
    std::println!(
        "GPU-written indirect draw, indexed draw with base vertex -1 and first instance 1, and zero-count draw: triangle tessellation 64/64 pixels; HS {}, DS {}, GS {}",
        stats.HSInvocations,
        stats.DSInvocations,
        stats.GSInvocations
    );
}

#[test]
fn tier_fl11_0_bc6h_bc7_typeless_views_mips_copy_and_decode() {
    let Some((device, queue, _)) = device() else {
        return;
    };
    let native = device.native_fl11_0().unwrap();
    let raw = native.device();
    let code = shader(Sm5Stage::Compute, "decode_blocks", "cs_5_0");
    let mut cs = None;
    unsafe { raw.CreateComputeShader(&code, None::<&ID3D11ClassLinkage>, Some(&mut cs)) }.unwrap();
    let formats = [
        DXGI_FORMAT_BC7_UNORM,
        DXGI_FORMAT_BC7_UNORM_SRGB,
        DXGI_FORMAT_BC6H_UF16,
        DXGI_FORMAT_BC6H_SF16,
    ];
    let expected_support = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
        | D3D11_FORMAT_SUPPORT_TEXTURE3D.0
        | D3D11_FORMAT_SUPPORT_TEXTURECUBE.0
        | D3D11_FORMAT_SUPPORT_SHADER_LOAD.0
        | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
        | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
    for format in formats {
        let actual = unsafe { raw.CheckFormatSupport(format) }.unwrap();
        std::println!("DXGI format {} support flags {actual:#010x}", format.0);
        assert_eq!(actual & expected_support, expected_support, "{format:?}");
    }

    fn bc7_gray(odd_value: u8) -> [u8; 16] {
        // Mode 6, identical RGBA endpoints with p-bits set, and zero indices.
        let mut bits = 1u128 << 6;
        for (channel, value) in [odd_value, odd_value, odd_value, 255]
            .into_iter()
            .enumerate()
        {
            assert_eq!(value & 1, 1);
            let endpoint = u128::from(value >> 1);
            bits |= endpoint << (7 + 14 * channel);
            bits |= endpoint << (14 + 14 * channel);
        }
        bits |= 3 << 63;
        bits.to_le_bytes()
    }

    fn subresource(data: &[u8], pitch: u32) -> D3D11_SUBRESOURCE_DATA {
        D3D11_SUBRESOURCE_DATA {
            pSysMem: data.as_ptr().cast(),
            SysMemPitch: pitch,
            SysMemSlicePitch: 0,
        }
    }

    let mut base = [0xcd; 96];
    for (offset, gray) in [(0, 1), (16, 3), (48, 5), (64, 65)] {
        base[offset..offset + 16].copy_from_slice(&bc7_gray(gray));
    }
    let mip1 = bc7_gray(33);
    let mip2 = bc7_gray(61);
    let mip3 = bc7_gray(129);
    let bc7_data = [
        subresource(&base, 48),
        subresource(&mip1, 16),
        subresource(&mip2, 16),
        subresource(&mip3, 16),
    ];
    let bc6_black = [0u8; 16];
    let bc6_data = subresource(&bc6_black, 16);
    let desc = D3D11_TEXTURE2D_DESC {
        Width: 8,
        Height: 8,
        MipLevels: 4,
        ArraySize: 1,
        Format: DXGI_FORMAT_BC7_TYPELESS,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
        ..Default::default()
    };
    let mut bc7_source = None;
    let mut bc7_copy = None;
    let mut bc6 = None;
    unsafe {
        raw.CreateTexture2D(&desc, Some(bc7_data.as_ptr()), Some(&mut bc7_source))
            .unwrap();
        raw.CreateTexture2D(&desc, None, Some(&mut bc7_copy))
            .unwrap();
        raw.CreateTexture2D(
            &D3D11_TEXTURE2D_DESC {
                Width: 4,
                Height: 4,
                MipLevels: 1,
                Format: DXGI_FORMAT_BC6H_TYPELESS,
                ..desc
            },
            Some(&bc6_data),
            Some(&mut bc6),
        )
        .unwrap();
        let invalid = D3D11_TEXTURE2D_DESC {
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            Format: DXGI_FORMAT_BC7_UNORM,
            ..desc
        };
        assert!(raw.CreateTexture2D(&invalid, None, None).is_err());
    }

    let bc7_copy = bc7_copy.unwrap();
    let bc6 = bc6.unwrap();
    let srv_desc = |format, mips| D3D11_SHADER_RESOURCE_VIEW_DESC {
        Format: format,
        ViewDimension: D3D_SRV_DIMENSION_TEXTURE2D,
        Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D11_TEX2D_SRV {
                MostDetailedMip: 0,
                MipLevels: mips,
            },
        },
    };
    let mut views = [const { None }; 4];
    unsafe {
        for (index, format) in formats.into_iter().enumerate() {
            let (texture, mips) = if index < 2 { (&bc7_copy, 4) } else { (&bc6, 1) };
            raw.CreateShaderResourceView(
                texture,
                Some(&srv_desc(format, mips)),
                Some(&mut views[index]),
            )
            .unwrap();
        }
        assert!(raw
            .CreateShaderResourceView(&bc7_copy, Some(&srv_desc(DXGI_FORMAT_BC6H_UF16, 4)), None)
            .is_err());
    }
    let output = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 80,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_UNORDERED_ACCESS.0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
            StructureByteStride: 16,
            ..Default::default()
        },
        None,
    );
    let staging = buffer(
        raw,
        D3D11_BUFFER_DESC {
            ByteWidth: 80,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        },
        None,
    );
    let output_view = uav(raw, &output, DXGI_FORMAT_UNKNOWN, 5, 0);
    let commands = unsafe {
        native.record(|context| {
            context.CopyResource(&bc7_copy, bc7_source.as_ref().unwrap());
            context.CSSetShaderResources(0, Some(&views));
            context.CSSetUnorderedAccessViews(0, 1, Some([Some(output_view)].as_ptr()), None);
            context.CSSetShader(cs.as_ref(), None);
            context.Dispatch(1, 1, 1);
            context.CSSetUnorderedAccessViews(0, 1, Some([None].as_ptr()), None);
            context.CopyResource(&staging, &output);
            Ok(())
        })
    }
    .unwrap();
    queue.submit_native_fl11_0(commands).unwrap();
    let pixels = unsafe {
        native.readback(|context| {
            let map = map_read(context, &staging)?;
            let pixels = map.pData.cast::<[[f32; 4]; 5]>().read_unaligned();
            context.Unmap(&staging, 0);
            Ok(pixels)
        })
    }
    .unwrap();
    let gray = 129.0f32 / 255.0;
    let srgb = ((gray + 0.055) / 1.055).powf(2.4);
    let expected = [65.0 / 255.0, gray, srgb, 0.0, 0.0];
    for (pixel, expected) in pixels.iter().zip(expected) {
        for &channel in &pixel[..3] {
            assert!((channel - expected).abs() < 0.001, "{pixels:?}");
        }
        assert_eq!(pixel[3], 1.0);
    }
    std::println!("compressed copy and shader readback: {pixels:?}");
}
