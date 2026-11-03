use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferTextureCopy, CommandEncoder as _, CopyExtent, Device as _, FormatAspects, Queue as _,
    TextureCopy, TextureCopyBase,
};
use alloc::{sync::Arc, vec};
use core::time::Duration;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_RENDER_TARGET,
        D3D11_FORMAT_SUPPORT_SHADER_SAMPLE, D3D11_FORMAT_SUPPORT_TEXTURE2D,
    },
    Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_R8G8B8A8_UNORM},
};

fn base(mip: u32, x: u32, y: u32) -> TextureCopyBase {
    TextureCopyBase {
        mip_level: mip,
        array_layer: 0,
        origin: wgt::Origin3d { x, y, z: 0 },
        aspect: FormatAspects::COLOR,
    }
}

fn extent(width: u32, height: u32) -> CopyExtent {
    CopyExtent {
        width,
        height,
        depth: 1,
    }
}

#[test]
fn fl91_copy_only_mip_transfers_and_views() -> windows::core::Result<()> {
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
    let dims = [(8u32, 4u32), (4, 2), (2, 1), (1, 1)];
    let mut executed = 0;
    let mut skipped = 0;
    for (run, (format, native_format)) in [
        (wgt::TextureFormat::Rgba8Unorm, DXGI_FORMAT_R8G8B8A8_UNORM),
        (wgt::TextureFormat::Bgra8Unorm, DXGI_FORMAT_B8G8R8A8_UNORM),
    ]
    .into_iter()
    .enumerate()
    {
        let required_flags = (D3D11_FORMAT_SUPPORT_TEXTURE2D.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_RENDER_TARGET.0
            | D3D11_FORMAT_SUPPORT_MIP.0) as u32;
        let support = unsafe { native.raw_device().CheckFormatSupport(native_format) };
        if !matches!(&support, Ok(flags) if flags & required_flags == required_flags) {
            assert!(
                !required,
                "strict mode requires {format:?} mip texture support {required_flags:#x}: {support:?}"
            );
            std::println!("SKIP: {format:?} mip texture support {required_flags:#x}: {support:?}");
            skipped += 1;
            continue;
        }
        let desc = crate::TextureDescriptor {
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
            usage: wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST,
            memory_flags: crate::MemoryFlags::empty(),
            view_formats: vec![],
        };
        let source = unsafe { device.create_texture(&desc) }.expect("POT copy-only mip source");
        let destination =
            unsafe { device.create_texture(&desc) }.expect("POT copy-only mip destination");
        let full_view_desc = crate::TextureViewDescriptor {
            label: None,
            format,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::empty(),
            range: wgt::ImageSubresourceRange::default(),
        };
        let full_view = unsafe { device.create_texture_view(&source, &full_view_desc) }
            .expect("metadata full view");
        let sub_view_desc = crate::TextureViewDescriptor {
            range: wgt::ImageSubresourceRange {
                base_mip_level: 1,
                mip_level_count: Some(2),
                ..Default::default()
            },
            ..full_view_desc.clone()
        };
        let sub_view = unsafe { device.create_texture_view(&source, &sub_view_desc) }
            .expect("metadata subrange view");
        let mut invalid_view = sub_view_desc.clone();
        invalid_view.usage = wgt::TextureUses::RESOURCE;
        assert!(unsafe { device.create_texture_view(&source, &invalid_view) }.is_err());
        invalid_view = sub_view_desc.clone();
        invalid_view.range.base_mip_level = 4;
        assert!(unsafe { device.create_texture_view(&source, &invalid_view) }.is_err());
        invalid_view = sub_view_desc.clone();
        invalid_view.range.mip_level_count = Some(0);
        assert!(unsafe { device.create_texture_view(&source, &invalid_view) }.is_err());
        invalid_view = sub_view_desc.clone();
        invalid_view.range.mip_level_count = Some(4);
        assert!(unsafe { device.create_texture_view(&source, &invalid_view) }.is_err());
        let mut unsupported = desc.clone();
        unsupported.mip_level_count = 5;
        assert!(unsafe { device.create_texture(&unsupported) }.is_err());
        unsupported = desc.clone();
        unsupported.size.width = 7;
        assert!(unsafe { device.create_texture(&unsupported) }.is_err());
        unsupported = desc.clone();
        unsupported.usage |= wgt::TextureUses::RESOURCE
            | wgt::TextureUses::COLOR_TARGET
            | wgt::TextureUses::TRANSIENT;
        assert!(unsafe { device.create_texture(&unsupported) }.is_err());
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
        let input = create_buffer(96, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let before = create_buffer(2048, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let after = create_buffer(2048, wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST);
        let mut data = vec![0x9au8; 96];
        for y in 0..2usize {
            for x in 0..4usize {
                let offset = 8 + y * 24 + x * 4;
                data[offset..offset + 4].copy_from_slice(&[
                    20 + x as u8 * 31,
                    40 + y as u8 * 47,
                    60 + (x + y) as u8 * 19,
                    255,
                ]);
            }
        }
        data[64..68].copy_from_slice(&[7, 119, 203, 251]);
        native.write_byte_buffer(&input, 0, &data)?;
        native.write_byte_buffer(&before, 0, &vec![0xa5; 2048])?;
        native.write_byte_buffer(&after, 0, &vec![0xa5; 2048])?;
        let upload = BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 8,
                bytes_per_row: Some(24),
                rows_per_image: Some(2),
            },
            texture_base: base(1, 0, 0),
            size: extent(4, 2),
        };
        let patch = BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 64,
                bytes_per_row: None,
                rows_per_image: None,
            },
            texture_base: base(2, 1, 0),
            size: extent(1, 1),
        };
        let copy_large = TextureCopy {
            src_base: base(1, 0, 0),
            dst_base: base(0, 2, 1),
            size: extent(4, 2),
        };
        let copy_small = TextureCopy {
            src_base: base(2, 0, 0),
            dst_base: base(1, 1, 1),
            size: extent(2, 1),
        };
        let mut bad = upload.clone();
        bad.texture_base.mip_level = 4;
        assert!(native
            .prepare_buffer_to_texture(&input, &source, &bad)
            .is_err());
        bad = upload.clone();
        bad.texture_base.origin.x = 1;
        assert!(native
            .prepare_buffer_to_texture(&input, &source, &bad)
            .is_err());
        bad = upload.clone();
        bad.buffer_layout.bytes_per_row = Some(12);
        assert!(native
            .prepare_buffer_to_texture(&input, &source, &bad)
            .is_err());
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
            for (phase, output) in [(0, &before), (1, &after)] {
                if phase == 1 {
                    encoder.copy_buffer_to_texture(
                        &input,
                        &source,
                        [upload.clone(), patch.clone()].into_iter(),
                    );
                    encoder.copy_texture_to_texture(
                        &source,
                        wgt::TextureUses::COPY_SRC,
                        &destination,
                        [copy_large.clone(), copy_small.clone()].into_iter(),
                    );
                }
                for (texture_index, texture) in [&source, &destination].into_iter().enumerate() {
                    for (mip, &(width, height)) in dims.iter().enumerate() {
                        let read = BufferTextureCopy {
                            buffer_layout: wgt::TexelCopyBufferLayout {
                                offset: 8 + (texture_index as u64 * 4 + mip as u64) * 256,
                                bytes_per_row: Some(width * 4 + 8),
                                rows_per_image: Some(height),
                            },
                            texture_base: base(mip as u32, 0, 0),
                            size: extent(width, height),
                        };
                        encoder.copy_texture_to_buffer(
                            texture,
                            wgt::TextureUses::COPY_SRC,
                            output,
                            [read].into_iter(),
                        );
                    }
                }
            }
            let commands = encoder.end_encoding().expect("mip transfer commands");
            device.destroy_buffer(input);
            device.destroy_texture(source);
            device.destroy_texture(destination);
            commands
        };
        let fence = native.create_native_fence();
        unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
        assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);
        for (phase, buffer) in [(0, &before), (1, &after)] {
            let actual = native.read_byte_buffer(buffer, 0, 2048)?;
            let mut expected = vec![0xa5; 2048];
            for texture_index in 0..2usize {
                for (mip, &(width, height)) in dims.iter().enumerate() {
                    let offset = 8 + (texture_index * 4 + mip) * 256;
                    let pitch = width as usize * 4 + 8;
                    for y in 0..height as usize {
                        for x in 0..width as usize {
                            let pixel: &[u8] = if phase == 1 && texture_index == 0 && mip == 1 {
                                let pos = 8 + y * 24 + x * 4;
                                &data[pos..pos + 4]
                            } else if phase == 1 && texture_index == 0 && mip == 2 && x == 1 {
                                &data[64..68]
                            } else if phase == 1
                                && texture_index == 1
                                && mip == 0
                                && (2..6).contains(&x)
                                && (1..3).contains(&y)
                            {
                                let pos = 8 + (y - 1) * 24 + (x - 2) * 4;
                                &data[pos..pos + 4]
                            } else if phase == 1
                                && texture_index == 1
                                && mip == 1
                                && x == 2
                                && y == 1
                            {
                                &data[64..68]
                            } else {
                                &[0, 0, 0, 0]
                            };
                            let pos = offset + y * pitch + x * 4;
                            expected[pos..pos + 4].copy_from_slice(pixel);
                        }
                    }
                }
            }
            assert_eq!(
                actual, expected,
                "{format:?} phase {phase}: image, mip, or row padding mismatch"
            );
        }
        unsafe {
            encoder.reset_all([commands].into_iter());
            device.destroy_texture_view(sub_view);
            device.destroy_texture_view(full_view);
            device.destroy_buffer(before);
            device.destroy_buffer(after);
        }
        std::println!("PASS: FL9_1 {format:?} mip initialization, uploads, copies and full readback (run {run})");
        executed += 1;
    }
    std::println!(
        "Mip format cases: executed={executed} passed={executed} failed=0 skipped={skipped}"
    );
    Ok(())
}
