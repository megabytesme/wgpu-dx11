use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferTextureCopy, CommandEncoder as _, CopyExtent, Device as _, FormatAspects, Queue as _,
    TextureCopy, TextureCopyBase,
};
use alloc::{sync::Arc, vec, vec::Vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_BOX, D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
};

#[test]
fn fl91_copy_dst_only_mips_upload_and_usage_boundaries() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no physical D3D11 adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested and actual FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let mut executed = 0;
    let mut skipped = 0;
    for (format, native_format) in [
        (wgt::TextureFormat::Rgba8Unorm, DXGI_FORMAT_R8G8B8A8_UNORM),
        (wgt::TextureFormat::Bgra8Unorm, DXGI_FORMAT_B8G8R8A8_UNORM),
    ] {
        let flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_MIP.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0) as u32;
        let support = unsafe { native.raw_device().CheckFormatSupport(native_format) };
        if !matches!(&support, Ok(bits) if bits & flags == flags) {
            assert!(
                !required,
                "strict mode requires {format:?} mip support {flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} mip support {flags:#x}: {support:?}");
            skipped += 1;
            continue;
        }
        let mut desc = crate::TextureDescriptor {
            label: None,
            size: wgt::Extent3d {
                width: 8,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 4,
            sample_count: 1,
            dimension: wgt::TextureDimension::D2,
            format,
            usage: wgt::TextureUses::COPY_DST,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: vec![],
        };
        let upload_only = unsafe { device.create_texture(&desc) }
            .expect("POT mip chain with logical COPY_DST only");
        desc.usage |= wgt::TextureUses::COPY_SRC;
        let control = unsafe { device.create_texture(&desc) }.expect("copyable mip control");
        desc.usage |= wgt::TextureUses::COLOR_TARGET;
        let diagnostic =
            unsafe { device.create_texture(&desc) }.expect("renderable diagnostic mip chain");
        let (native_upload, _, _) = native.texture_copy_destination(&upload_only, 0)?;
        let (native_diagnostic, _, _) = native.texture_copy_destination(&diagnostic, 0)?;
        for usage in [wgt::TextureUses::RESOURCE, wgt::TextureUses::COLOR_TARGET] {
            let unsupported = crate::TextureViewDescriptor {
                label: None,
                format,
                dimension: wgt::TextureViewDimension::D2,
                usage,
                range: wgt::ImageSubresourceRange::default(),
            };
            assert!(
                unsafe { device.create_texture_view(&upload_only, &unsupported) }.is_err(),
                "{format:?} COPY_DST-only texture cannot create a {usage:?} view"
            );
        }
        let create_buffer = |size, usage| unsafe {
            device
                .create_buffer(&crate::BufferDescriptor {
                    label: None,
                    size,
                    usage,
                    memory_flags: crate::MemoryFlags::empty(),
                })
                .unwrap()
        };
        let input = create_buffer(256, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let output = create_buffer(256, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let dims = [(8u32, 4u32, 0u64), (4, 2, 128), (2, 1, 160), (1, 1, 168)];
        let mut data = vec![0xa5u8; 256];
        let mut regions = Vec::new();
        for (mip, (width, height, offset)) in dims.into_iter().enumerate() {
            for y in 0..height {
                for x in 0..width {
                    let pos = offset as usize + (y * width * 4 + x * 4) as usize;
                    data[pos..pos + 4].copy_from_slice(&[
                        11 + mip as u8 * 37 + x as u8 * 3,
                        21 + y as u8 * 23,
                        31 + mip as u8 * 17,
                        255,
                    ]);
                }
            }
            regions.push(BufferTextureCopy {
                buffer_layout: wgt::TexelCopyBufferLayout {
                    offset,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                texture_base: TextureCopyBase {
                    mip_level: mip as u32,
                    array_layer: 0,
                    origin: wgt::Origin3d { x: 0, y: 0, z: 0 },
                    aspect: FormatAspects::COLOR,
                },
                size: CopyExtent {
                    width,
                    height,
                    depth: 1,
                },
            });
        }
        let mut expected = data.clone();
        expected[160..168].fill(0);
        native.write_byte_buffer(&input, 0, &data)?;
        native.write_byte_buffer(&output, 0, &vec![0xa5; 256])?;
        assert!(
            native
                .prepare_texture_to_buffer(&upload_only, &output, &regions[0])
                .is_err(),
            "COPY_DST-only texture cannot be read back"
        );
        assert!(
            native
                .prepare_texture_to_texture(
                    &upload_only,
                    &control,
                    &TextureCopy {
                        src_base: regions[0].texture_base.clone(),
                        dst_base: regions[0].texture_base.clone(),
                        size: regions[0].size,
                    }
                )
                .is_err(),
            "COPY_DST-only texture cannot be a copy source"
        );
        let mut encoder = unsafe {
            device
                .create_command_encoder(&crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &queue,
                })
                .unwrap()
        };
        let commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            let uploads = [regions[0].clone(), regions[1].clone(), regions[3].clone()];
            encoder.copy_buffer_to_texture(&input, &upload_only, uploads.clone().into_iter());
            encoder.copy_buffer_to_texture(&input, &control, uploads.into_iter());
            for region in &regions {
                encoder.copy_texture_to_buffer(
                    &control,
                    wgt::TextureUses::COPY_SRC,
                    &output,
                    [region.clone()].into_iter(),
                );
            }
            let commands = encoder.end_encoding().expect("COPY_DST-only mip uploads");
            device.destroy_buffer(input);
            device.destroy_texture(upload_only);
            device.destroy_texture(control);
            commands
        };
        let fence = native.create_native_fence();
        unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);
        assert_eq!(
            native.read_byte_buffer(&output, 0, 256)?,
            expected,
            "{format:?} copy-capable control mip bytes and untouched padding"
        );
        {
            let context = native.immediate_context();
            for (mip, (width, height, _)) in dims.into_iter().enumerate() {
                let source_box = D3D11_BOX {
                    left: 0,
                    top: 0,
                    front: 0,
                    right: width,
                    bottom: height,
                    back: 1,
                };
                unsafe {
                    context.CopySubresourceRegion(
                        &native_diagnostic,
                        mip as u32,
                        0,
                        0,
                        0,
                        &native_upload,
                        mip as u32,
                        Some(&source_box),
                    );
                }
            }
        }
        native.write_byte_buffer(&output, 0, &vec![0xa5; 256])?;
        let diagnostic_commands = unsafe {
            encoder.begin_encoding(None).unwrap();
            for region in &regions {
                encoder.copy_texture_to_buffer(
                    &diagnostic,
                    wgt::TextureUses::COPY_SRC,
                    &output,
                    [region.clone()].into_iter(),
                );
            }
            encoder.end_encoding().expect("diagnostic mip readback")
        };
        unsafe {
            queue
                .submit(&[&diagnostic_commands], &[], (&fence, 2))
                .unwrap()
        };
        assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(10)))?);
        assert_eq!(
            native.read_byte_buffer(&output, 0, 256)?,
            expected,
            "{format:?} test-only native copy of COPY_DST-only mips and untouched padding"
        );
        unsafe {
            encoder.reset_all([commands, diagnostic_commands].into_iter());
            device.destroy_buffer(output);
            device.destroy_texture(diagnostic);
        }
        executed += 1;
        std::println!("PASS: {format:?} COPY_DST-only three-mip uploads and untouched zero mip; control and native diagnostic readbacks matched");
    }
    std::println!("COPY_DST-only mip cases: executed={executed}, passed={executed}, failed=0, skipped_formats={skipped}");
    Ok(())
}
