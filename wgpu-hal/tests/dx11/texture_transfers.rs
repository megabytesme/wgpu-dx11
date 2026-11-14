use crate::{
    dx11::{enumerate_native_adapters, Device, FeatureLevel, Queue},
    BufferCopy, BufferDescriptor, BufferTextureCopy, CommandEncoder as _, CommandEncoderDescriptor,
    CopyExtent, Device as _, FormatAspects, MemoryFlags, Queue as _, TextureCopy, TextureCopyBase,
    TextureDescriptor, TextureViewDescriptor,
};
use alloc::{sync::Arc, vec, vec::Vec};
use core::{num::NonZeroU64, time::Duration};

fn base(x: u32, y: u32) -> TextureCopyBase {
    TextureCopyBase {
        mip_level: 0,
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
fn fl91_texture_transfer_chain_readback_and_lifetime() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested FL9_1; actual {:?}",
        adapter.identity(),
        native.feature_level()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let shader_only = native.create_rgba_texture(1, 1, 1, 1)?;
    assert!(native.read_rgba_texture(&shader_only, 0, 0).is_err());
    let usage =
        wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE;
    let texture_desc = |width, height| TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Rgba8Unorm,
        usage,
        memory_flags: MemoryFlags::empty(),
        view_formats: vec![],
    };
    let (a, b, c) = unsafe {
        (
            device.create_texture(&texture_desc(4, 3)).unwrap(),
            device.create_texture(&texture_desc(6, 5)).unwrap(),
            device.create_texture(&texture_desc(6, 5)).unwrap(),
        )
    };
    assert_eq!(native.read_rgba_texture(&a, 0, 0)?, vec![0; 4 * 3 * 4]);
    assert_eq!(native.read_rgba_texture(&b, 0, 0)?, vec![0; 6 * 5 * 4]);
    let view = unsafe {
        device
            .create_texture_view(
                &a,
                &TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let bad_view = TextureViewDescriptor {
        label: None,
        format: wgt::TextureFormat::Bgra8Unorm,
        dimension: wgt::TextureViewDimension::D2,
        usage: wgt::TextureUses::RESOURCE,
        range: wgt::ImageSubresourceRange::default(),
    };
    assert!(unsafe { device.create_texture_view(&a, &bad_view) }.is_err());
    let bad_dimension = TextureViewDescriptor {
        dimension: wgt::TextureViewDimension::D2Array,
        format: wgt::TextureFormat::Rgba8Unorm,
        ..bad_view
    };
    assert!(unsafe { device.create_texture_view(&a, &bad_dimension) }.is_err());
    let buffer = || unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: 192,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    let input = buffer();
    let intermediate = buffer();
    let output = buffer();
    let source_output = buffer();
    let partial_output = buffer();
    let mut bytes = vec![0xa5; 192];
    for y in 0..3usize {
        for x in 0..4usize {
            let pos = 8 + y * 24 + x * 4;
            bytes[pos..pos + 4].copy_from_slice(&[
                10 + x as u8 * 40,
                20 + y as u8 * 60,
                30 + (x + y) as u8 * 20,
                255,
            ]);
        }
    }
    native.write_byte_buffer(&input, 0, &bytes)?;
    native.write_byte_buffer(&intermediate, 0, &[0; 192])?;
    native.write_byte_buffer(&output, 0, &[0xa5; 192])?;
    native.write_byte_buffer(&source_output, 0, &[0x3c; 192])?;
    native.write_byte_buffer(&partial_output, 0, &[0x5a; 192])?;
    let upload = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 8,
            bytes_per_row: Some(24),
            rows_per_image: Some(3),
        },
        texture_base: base(0, 0),
        size: extent(4, 3),
    };
    let patch = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 8,
            bytes_per_row: None,
            rows_per_image: None,
        },
        texture_base: base(0, 0),
        size: extent(1, 1),
    };
    let mut bad_upload = upload.clone();
    bad_upload.buffer_layout.offset = 160;
    assert!(native
        .prepare_buffer_to_texture(&input, &a, &bad_upload)
        .is_err());
    bad_upload = upload.clone();
    bad_upload.buffer_layout.bytes_per_row = Some(12);
    assert!(native
        .prepare_buffer_to_texture(&input, &a, &bad_upload)
        .is_err());
    let partial = TextureCopy {
        src_base: base(1, 1),
        dst_base: base(2, 2),
        size: extent(3, 2),
    };
    let full = TextureCopy {
        src_base: base(0, 0),
        dst_base: base(0, 0),
        size: extent(6, 5),
    };
    let read = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 12,
            bytes_per_row: Some(32),
            rows_per_image: Some(5),
        },
        texture_base: base(0, 0),
        size: extent(6, 5),
    };
    let source_read = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 4,
            bytes_per_row: Some(20),
            rows_per_image: Some(3),
        },
        texture_base: base(0, 0),
        size: extent(4, 3),
    };
    let partial_read = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 16,
            bytes_per_row: Some(20),
            rows_per_image: Some(2),
        },
        texture_base: base(2, 2),
        size: extent(3, 2),
    };
    let mut bad_read = read.clone();
    bad_read.buffer_layout.offset = 44;
    assert!(native
        .prepare_texture_to_buffer(&c, &output, &bad_read)
        .is_err());
    let mut unsupported = texture_desc(4, 3);
    unsupported.usage = wgt::TextureUses::RESOURCE;
    let no_copy = unsafe { device.create_texture(&unsupported).unwrap() };
    assert!(native
        .prepare_texture_to_texture(&no_copy, &b, &partial)
        .is_err());
    assert!(native
        .prepare_texture_to_texture(&a, &no_copy, &partial)
        .is_err());
    unsupported.sample_count = 4;
    assert!(unsafe { device.create_texture(&unsupported) }.is_err());
    unsupported.sample_count = 1;
    unsupported.mip_level_count = 2;
    let npot_mipmaps_supported = super::super::capabilities::Capabilities::query(&native)
        .npot_mipmapped_textures_supported();
    let npot_mip_result = unsafe { device.create_texture(&unsupported) };
    assert_eq!(npot_mip_result.is_ok(), npot_mipmaps_supported);
    if let Ok(texture) = npot_mip_result {
        unsafe { device.destroy_texture(texture) };
    }
    unsupported.mip_level_count = 1;
    unsupported.size.depth_or_array_layers = 2;
    assert!(unsafe { device.create_texture(&unsupported) }.is_err());
    unsafe {
        device.destroy_texture(no_copy);
    }
    assert!(native.prepare_texture_to_texture(&a, &a, &partial).is_err());
    let mut bad = partial.clone();
    bad.src_base.origin.x = 4;
    assert!(native.prepare_texture_to_texture(&a, &b, &bad).is_err());
    bad = partial.clone();
    bad.dst_base.origin.y = 4;
    assert!(native.prepare_texture_to_texture(&a, &b, &bad).is_err());
    bad = partial.clone();
    bad.src_base.mip_level = 1;
    assert!(native.prepare_texture_to_texture(&a, &b, &bad).is_err());
    bad = partial.clone();
    bad.dst_base.array_layer = 1;
    assert!(native.prepare_texture_to_texture(&a, &b, &bad).is_err());
    let mut encoder = unsafe {
        device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_texture_to_texture(
            &a,
            wgt::TextureUses::COPY_SRC,
            &a,
            [partial.clone()].into_iter(),
        );
        assert!(encoder.end_encoding().is_err());
    }
    let first = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_buffer(
            &input,
            &intermediate,
            [BufferCopy {
                src_offset: 0,
                dst_offset: 0,
                size: NonZeroU64::new(192).unwrap(),
            }]
            .into_iter(),
        );
        encoder.copy_buffer_to_texture(&intermediate, &a, [upload].into_iter());
        encoder.copy_buffer_to_texture(&intermediate, &b, [patch].into_iter());
        encoder.copy_texture_to_buffer(
            &a,
            wgt::TextureUses::COPY_SRC,
            &source_output,
            [source_read].into_iter(),
        );
        encoder.end_encoding().unwrap()
    };
    let second = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_texture_to_texture(&a, wgt::TextureUses::COPY_SRC, &b, [partial].into_iter());
        encoder.copy_texture_to_texture(&b, wgt::TextureUses::COPY_SRC, &c, [full].into_iter());
        encoder.copy_texture_to_buffer(&c, wgt::TextureUses::COPY_SRC, &output, [read].into_iter());
        encoder.copy_texture_to_buffer(
            &c,
            wgt::TextureUses::COPY_SRC,
            &partial_output,
            [partial_read].into_iter(),
        );
        encoder.end_encoding().unwrap()
    };
    unsafe {
        device.destroy_buffer(input);
        device.destroy_buffer(intermediate);
        device.destroy_texture(a);
    }
    assert_eq!(native.sampled_view(&view)?.1, [4, 3, 1]);
    let fence = native.create_native_fence();
    unsafe {
        queue.submit(&[&first], &[], (&fence, 1)).unwrap();
        queue.submit(&[&second], &[], (&fence, 2)).unwrap();
    }
    assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(10)))?);
    let source_got = native.read_byte_buffer(&source_output, 0, 192)?;
    let mut source_expected = vec![0x3c; 192];
    for y in 0..3usize {
        let source = 8 + y * 24;
        let destination = 4 + y * 20;
        source_expected[destination..destination + 16].copy_from_slice(&bytes[source..source + 16]);
    }
    assert_eq!(source_got, source_expected);
    let got = native.read_byte_buffer(&output, 0, 192)?;
    let mut expected: Vec<u8> = vec![0xa5; 192];
    for y in 0..5usize {
        for x in 0..6usize {
            let pixel = if x == 0 && y == 0 {
                bytes[8..12].to_vec()
            } else if (2..5).contains(&x) && (2..4).contains(&y) {
                let pos = 8 + (y - 1) * 24 + (x - 1) * 4;
                bytes[pos..pos + 4].to_vec()
            } else {
                vec![0, 0, 0, 0]
            };
            let pos = 12 + y * 32 + x * 4;
            expected[pos..pos + 4].copy_from_slice(&pixel);
        }
    }
    assert_eq!(got, expected);
    let partial_got = native.read_byte_buffer(&partial_output, 0, 192)?;
    let mut partial_expected = vec![0x5a; 192];
    for y in 0..2usize {
        let source = 8 + (y + 1) * 24 + 4;
        let destination = 16 + y * 20;
        partial_expected[destination..destination + 12]
            .copy_from_slice(&bytes[source..source + 12]);
    }
    assert_eq!(partial_got, partial_expected);
    std::println!("FL9_1 transfer readback: 12/12 uploaded pixels, 30/30 copied pixels, 6/6 partial pixels and padding match; zero initialization and two ordered submissions verified");
    unsafe {
        encoder.reset_all([first, second].into_iter());
        device.destroy_texture_view(view);
        device.destroy_texture(b);
        device.destroy_texture(c);
        device.destroy_buffer(output);
        device.destroy_buffer(source_output);
        device.destroy_buffer(partial_output);
    }
    Ok(())
}

#[test]
fn fl91_cube_texture_to_buffer_readback_uses_padded_faces() -> windows::core::Result<()> {
    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let identity = adapter.identity();
    let driver = adapter.driver_version();
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);
    let support = unsafe {
        native.raw_device().CheckFormatSupport(
            windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        )?
    };
    assert_ne!(
        support & windows::Win32::Graphics::Direct3D11::D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32,
        0,
        "cube readback requires queried RGBA8 render-target support"
    );
    std::println!(
        "Hardware: {:?} ({:#06x}:{:#06x}), driver={driver:?}, software={}; requested/actual FL9_1; RGBA8 format support={:#010x}",
        identity.name,
        identity.vendor_id,
        identity.device_id,
        identity.software,
        support,
    );

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let cube = unsafe {
        device
            .create_texture(&TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 6,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage: wgt::TextureUses::RESOURCE
                    | wgt::TextureUses::COPY_DST
                    | wgt::TextureUses::COPY_SRC,
                memory_flags: MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let input_offset = 20usize;
    let input_row_pitch = 256usize;
    let input_rows_per_image = 9usize;
    let input_image_pitch = input_row_pitch * input_rows_per_image;
    let mut upload_data = vec![0x6d; input_offset + 6 * input_image_pitch];
    for face in 0..6usize {
        for y in 0..8usize {
            for x in 0..8usize {
                let pixel = [
                    10 + face as u8 * 30 + x as u8,
                    20 + y as u8 * 17,
                    5 + face as u8 * 7 + y as u8 * 3 + x as u8,
                    255,
                ];
                let offset = input_offset + face * input_image_pitch + y * input_row_pitch + x * 4;
                upload_data[offset..offset + 4].copy_from_slice(&pixel);
            }
        }
    }
    let input = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: upload_data.len() as u64,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&input, 0, &upload_data)?;

    let output_offset = 16usize;
    let output_row_pitch = 256usize;
    let output_rows_per_image = 9usize;
    let output_image_pitch = output_row_pitch * output_rows_per_image;
    let output_size = output_offset + 6 * output_image_pitch;
    let output = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: output_size as u64,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&output, 0, &vec![0xa5; output_size])?;

    let layer_base = |layer| TextureCopyBase {
        mip_level: 0,
        array_layer: layer,
        origin: wgt::Origin3d { x: 0, y: 0, z: 0 },
        aspect: FormatAspects::COLOR,
    };
    let uploads = (0..6).map(|layer| BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: (input_offset + layer as usize * input_image_pitch) as u64,
            bytes_per_row: Some(input_row_pitch as u32),
            rows_per_image: Some(input_rows_per_image as u32),
        },
        texture_base: layer_base(layer),
        size: extent(8, 8),
    });
    let readbacks = (0..6).map(|layer| BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: (output_offset + layer as usize * output_image_pitch) as u64,
            bytes_per_row: Some(output_row_pitch as u32),
            rows_per_image: Some(output_rows_per_image as u32),
        },
        texture_base: layer_base(layer),
        size: extent(8, 8),
    });
    let mut encoder = unsafe {
        device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_texture(&input, &cube, uploads);
        encoder.copy_texture_to_buffer(&cube, wgt::TextureUses::COPY_SRC, &output, readbacks);
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);

    let actual = native.read_byte_buffer(&output, 0, output_size)?;
    let mut expected = vec![0xa5; output_size];
    for face in 0..6usize {
        for y in 0..8usize {
            for x in 0..8usize {
                let pixel = [
                    10 + face as u8 * 30 + x as u8,
                    20 + y as u8 * 17,
                    5 + face as u8 * 7 + y as u8 * 3 + x as u8,
                    255,
                ];
                let offset =
                    output_offset + face * output_image_pitch + y * output_row_pitch + x * 4;
                expected[offset..offset + 4].copy_from_slice(&pixel);
            }
        }
    }
    assert_eq!(
        actual, expected,
        "cube face bytes or readback padding differed"
    );
    std::println!(
        "PASS: six cube faces copied to padded CPU buffer; 384 texels and every row/image padding byte matched"
    );
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(input);
        device.destroy_buffer(output);
        device.destroy_texture(cube);
    }
    Ok(())
}

#[test]
fn fl91_bgra_upload_copy_and_readback() -> windows::core::Result<()> {
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
        "Hardware: {:?}; requested and selected FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let desc = TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width: 3,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Bgra8Unorm,
        usage: wgt::TextureUses::COPY_SRC | wgt::TextureUses::COPY_DST | wgt::TextureUses::RESOURCE,
        memory_flags: MemoryFlags::empty(),
        view_formats: vec![],
    };
    let source = unsafe { device.create_texture(&desc) }.expect("BGRA8 texture creation");
    let destination = unsafe { device.create_texture(&desc) }.unwrap();
    let rgba_desc = TextureDescriptor {
        format: wgt::TextureFormat::Rgba8Unorm,
        ..desc.clone()
    };
    let rgba = unsafe { device.create_texture(&rgba_desc) }.unwrap();
    let view_desc = TextureViewDescriptor {
        label: None,
        format: wgt::TextureFormat::Bgra8Unorm,
        dimension: wgt::TextureViewDimension::D2,
        usage: wgt::TextureUses::RESOURCE,
        range: wgt::ImageSubresourceRange::default(),
    };
    let view = unsafe { device.create_texture_view(&source, &view_desc) }.unwrap();
    assert_eq!(native.sampled_view(&view)?.1, [3, 2, 1]);
    let wrong_view = TextureViewDescriptor {
        format: wgt::TextureFormat::Rgba8Unorm,
        ..view_desc
    };
    assert!(unsafe { device.create_texture_view(&source, &wrong_view) }.is_err());
    assert!(native.read_rgba_texture(&source, 0, 0).is_err());
    let buffer = || unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: 48,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    let input = buffer();
    let output = buffer();
    let mut data = vec![0xa5; 48];
    for y in 0..2usize {
        for x in 0..3usize {
            let offset = 4 + y * 16 + x * 4;
            data[offset..offset + 4].copy_from_slice(&[
                10 + x as u8 * 30,
                40 + y as u8 * 60,
                190 - x as u8 * 20,
                255,
            ]);
        }
    }
    native.write_byte_buffer(&input, 0, &data)?;
    native.write_byte_buffer(&output, 0, &[0x5a; 48])?;
    let layout = wgt::TexelCopyBufferLayout {
        offset: 4,
        bytes_per_row: Some(16),
        rows_per_image: Some(2),
    };
    let region = BufferTextureCopy {
        buffer_layout: layout,
        texture_base: base(0, 0),
        size: extent(3, 2),
    };
    let copy = TextureCopy {
        src_base: base(0, 0),
        dst_base: base(0, 0),
        size: extent(3, 2),
    };
    assert!(native
        .prepare_texture_to_texture(&source, &rgba, &copy)
        .is_err());
    assert!(native
        .prepare_texture_to_texture(&rgba, &destination, &copy)
        .is_err());
    let mut encoder = unsafe {
        device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_texture(&input, &source, [region.clone()].into_iter());
        encoder.copy_texture_to_texture(
            &source,
            wgt::TextureUses::COPY_SRC,
            &destination,
            [copy].into_iter(),
        );
        encoder.copy_texture_to_buffer(
            &destination,
            wgt::TextureUses::COPY_SRC,
            &output,
            [region].into_iter(),
        );
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);
    let got = native.read_byte_buffer(&output, 0, 48)?;
    let mut expected = vec![0x5a; 48];
    for y in 0..2usize {
        let start = 4 + y * 16;
        expected[start..start + 12].copy_from_slice(&data[start..start + 12]);
    }
    assert_eq!(got, expected, "BGRA bytes and padding");
    std::println!("PASS: FL9_1 BGRA upload/copy/readback, 6 pixels and row padding");
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_texture_view(view);
        device.destroy_texture(source);
        device.destroy_texture(destination);
        device.destroy_texture(rgba);
        device.destroy_buffer(input);
        device.destroy_buffer(output);
    }
    Ok(())
}

#[test]
fn fl91_bgra_render_and_partial_copy() -> windows::core::Result<()> {
    use alloc::borrow::Cow;
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
        "Hardware: {:?}; requested and selected FL9_1",
        adapter.identity()
    );
    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let texture_desc = |width, height, usage| TextureDescriptor {
        label: None,
        size: wgt::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgt::TextureDimension::D2,
        format: wgt::TextureFormat::Bgra8Unorm,
        usage,
        memory_flags: MemoryFlags::empty(),
        view_formats: vec![],
    };
    let source = unsafe {
        device
            .create_texture(&texture_desc(
                3,
                2,
                wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
            ))
            .unwrap()
    };
    let destination = unsafe {
        device
            .create_texture(&texture_desc(
                5,
                4,
                wgt::TextureUses::COPY_DST | wgt::TextureUses::COPY_SRC,
            ))
            .unwrap()
    };
    let view = unsafe {
        device
            .create_texture_view(
                &source,
                &TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Bgra8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let module = naga::front::wgsl::parse_str("@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(0.8, 0.4, 0.2, 1.0); }").unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let shader = unsafe {
        device
            .create_shader_module(
                &crate::ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgt::ShaderRuntimeChecks::checked(),
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[],
                immediate_size: 0,
            })
            .unwrap()
    };
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 8,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Bgra8Unorm.into())];
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };
    let vertices = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: 24,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    let data: Vec<u8> = [-1f32, -1., 3., -1., -1., 3.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    native.write_byte_buffer(&vertices, 0, &data)?;
    let output = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: 112,
                usage: wgt::BufferUses::COPY_DST | wgt::BufferUses::COPY_SRC,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&output, 0, &[0x5a; 112])?;
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.0,
        },
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent: wgt::Extent3d {
            width: 3,
            height: 2,
            depth_or_array_layers: 1,
        },
        sample_count: 1,
        color_attachments: &colors,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let copy = TextureCopy {
        src_base: base(1, 0),
        dst_base: base(2, 1),
        size: extent(2, 2),
    };
    let read = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: 4,
            bytes_per_row: Some(24),
            rows_per_image: Some(4),
        },
        texture_base: base(0, 0),
        size: extent(5, 4),
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertices, 0, None));
        encoder.set_scissor_rect(&crate::Rect {
            x: 1,
            y: 0,
            w: 1,
            h: 1,
        });
        encoder.draw(0, 3, 0, 1);
        encoder.end_render_pass();
        encoder.copy_texture_to_texture(
            &source,
            wgt::TextureUses::COPY_SRC,
            &destination,
            [copy].into_iter(),
        );
        encoder.copy_texture_to_buffer(
            &destination,
            wgt::TextureUses::COPY_SRC,
            &output,
            [read].into_iter(),
        );
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe {
        queue.submit(&[&commands], &[], (&fence, 1)).unwrap();
    }
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);
    let got = native.read_byte_buffer(&output, 0, 112)?;
    let mut expected = vec![0x5a; 112];
    for y in 0..4usize {
        for x in 0..5usize {
            let pixel = if (2..4).contains(&x) && (1..3).contains(&y) {
                if x == 2 && y == 1 {
                    [51, 102, 204, 255]
                } else {
                    [77, 51, 26, 255]
                }
            } else {
                [0, 0, 0, 0]
            };
            let offset = 4 + y * 24 + x * 4;
            expected[offset..offset + 4].copy_from_slice(&pixel);
        }
    }
    for (index, (&actual, &wanted)) in got.iter().zip(&expected).enumerate() {
        let in_pixel = index >= 4 && (index - 4) / 24 < 4 && (index - 4) % 24 < 20;
        assert!(
            actual.abs_diff(wanted) <= u8::from(in_pixel),
            "BGRA rendered copy byte {index}: {actual} expected {wanted}"
        );
    }
    std::println!("PASS: rendered BGRA partial copy with nonzero origins, differing dimensions and untouched pixels");
    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(vertices);
        device.destroy_buffer(output);
        device.destroy_texture_view(view);
        device.destroy_texture(source);
        device.destroy_texture(destination);
        device.destroy_render_pipeline(pipeline);
        device.destroy_pipeline_layout(layout);
        device.destroy_shader_module(shader);
    }
    Ok(())
}

fn run_fl91_padded_partial_3d_upload(
    format: wgt::TextureFormat,
    native_format: windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT,
    bytes_per_texel: usize,
    label: &str,
) -> windows::core::Result<()> {
    use alloc::borrow::Cow;
    use windows::Win32::Graphics::{
        Direct3D11::{
            D3D11_FORMAT_SUPPORT_RENDER_TARGET, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
            D3D11_FORMAT_SUPPORT_TEXTURE3D,
        },
        Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
    };

    let adapters = enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|error| panic!("{error}"))
    else {
        std::println!("SKIP: no physical D3D11 adapter");
        return Ok(());
    };
    let identity = adapter.identity();
    let driver = adapter.driver_version();
    let native = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), FeatureLevel::L9_1);

    let support = unsafe { native.raw_device().CheckFormatSupport(native_format)? };
    let texture_requirements =
        (D3D11_FORMAT_SUPPORT_TEXTURE3D.0 | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0) as u32;
    if support & texture_requirements != texture_requirements {
        std::println!(
            "selected physical FL9_1 device lacks {format:?} 3D sampling support ({support:#010x})"
        );
        return Err(windows::core::Error::from_hresult(
            windows::Win32::Foundation::E_FAIL,
        ));
    }
    let target_support = unsafe {
        native
            .raw_device()
            .CheckFormatSupport(DXGI_FORMAT_R8G8B8A8_UNORM)?
    };
    if target_support & D3D11_FORMAT_SUPPORT_RENDER_TARGET.0 as u32 == 0 {
        std::println!(
            "selected physical FL9_1 device lacks RGBA8 render-target support ({target_support:#010x})"
        );
        return Err(windows::core::Error::from_hresult(
            windows::Win32::Foundation::E_FAIL,
        ));
    }
    std::println!(
        "Hardware: {:?} ({:#06x}:{:#06x}), driver={driver:?}, software={}; requested/actual FL9_1; {format:?} format support={support:#010x}",
        identity.name, identity.vendor_id, identity.device_id, identity.software,
    );

    let device = Device::new(native.clone());
    let queue = Queue::new(native.clone());
    let volume = unsafe {
        device
            .create_texture(&TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 4,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D3,
                format,
                usage: wgt::TextureUses::RESOURCE
                    | wgt::TextureUses::COPY_DST
                    | if bytes_per_texel == 1 {
                        wgt::TextureUses::COPY_SRC
                    } else {
                        wgt::TextureUses::empty()
                    },
                memory_flags: MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let copy_target = if bytes_per_texel == 1 {
        Some(unsafe {
            device
                .create_texture(&TextureDescriptor {
                    label: None,
                    size: wgt::Extent3d {
                        width: 4,
                        height: 4,
                        depth_or_array_layers: 4,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D3,
                    format,
                    usage: wgt::TextureUses::RESOURCE
                        | wgt::TextureUses::COPY_SRC
                        | wgt::TextureUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                    view_formats: vec![],
                })
                .unwrap()
        })
    } else {
        None
    };

    let source_offset = 16usize;
    let source_row_pitch = 32usize;
    let source_rows_per_image = 3usize;
    let source_image_pitch = source_row_pitch * source_rows_per_image;
    let upload_region = extent(2, 2);
    let upload = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: source_offset as u64,
            bytes_per_row: Some(source_row_pitch as u32),
            rows_per_image: Some(source_rows_per_image as u32),
        },
        texture_base: TextureCopyBase {
            mip_level: 0,
            array_layer: 0,
            origin: wgt::Origin3d { x: 1, y: 1, z: 1 },
            aspect: FormatAspects::COLOR,
        },
        size: CopyExtent {
            width: upload_region.width,
            height: upload_region.height,
            depth: 2,
        },
    };
    let mut upload_data = vec![0xa5; source_offset + 2 * source_image_pitch];
    for z in 0..2usize {
        for y in 0..2usize {
            for x in 0..2usize {
                let offset = source_offset
                    + z * source_image_pitch
                    + y * source_row_pitch
                    + x * bytes_per_texel;
                if bytes_per_texel == 1 {
                    upload_data[offset] = 40 + x as u8 * 80 + y as u8 * 40 + z as u8 * 20;
                } else {
                    upload_data[offset..offset + 4].copy_from_slice(&[
                        40 + x as u8 * 80,
                        20 + y as u8 * 90,
                        30 + z as u8 * 70,
                        255,
                    ]);
                }
            }
        }
    }
    let input = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: upload_data.len() as u64,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&input, 0, &upload_data)?;

    let volume_readback = if bytes_per_texel == 1 {
        let buffer = unsafe {
            device
                .create_buffer(&BufferDescriptor {
                    label: None,
                    size: 3860,
                    usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                    memory_flags: MemoryFlags::empty(),
                })
                .unwrap()
        };
        native.write_byte_buffer(&buffer, 0, &vec![0x5a; 3860])?;
        Some(buffer)
    } else {
        None
    };

    let sampled_volume = copy_target.as_ref().unwrap_or(&volume);
    let volume_view = unsafe {
        device
            .create_texture_view(
                sampled_volume,
                &TextureViewDescriptor {
                    label: None,
                    format,
                    dimension: wgt::TextureViewDimension::D3,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange {
                        aspect: wgt::TextureAspect::All,
                        base_mip_level: 0,
                        mip_level_count: Some(1),
                        base_array_layer: 0,
                        array_layer_count: Some(1),
                    },
                },
            )
            .unwrap()
    };
    let sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [wgt::AddressMode::ClampToEdge; 3],
                mag_filter: wgt::FilterMode::Nearest,
                min_filter: wgt::FilterMode::Nearest,
                mipmap_filter: wgt::MipmapFilterMode::Nearest,
                lod_clamp: 0.0..f32::MAX,
                compare: None,
                anisotropy_clamp: 1,
                border_color: None,
            })
            .unwrap()
    };
    let entries = [
        wgt::BindGroupLayoutEntry {
            binding: 7,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Texture {
                sample_type: wgt::TextureSampleType::Float { filterable: false },
                view_dimension: wgt::TextureViewDimension::D3,
                multisampled: false,
            },
            count: None,
        },
        wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::NonFiltering),
            count: None,
        },
    ];
    let group_layout = unsafe {
        device
            .create_bind_group_layout(&crate::BindGroupLayoutDescriptor {
                label: None,
                flags: crate::BindGroupLayoutFlags::empty(),
                entries: &entries,
            })
            .unwrap()
    };
    let texture_bindings = [crate::TextureBinding {
        view: &volume_view,
        usage: wgt::TextureUses::RESOURCE,
    }];
    let sampler_bindings = [&sampler];
    let group_entries = [
        crate::BindGroupEntry {
            binding: 7,
            resource_index: 0,
            count: 1,
        },
        crate::BindGroupEntry {
            binding: 42,
            resource_index: 0,
            count: 1,
        },
    ];
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &[],
                samplers: &sampler_bindings,
                textures: &texture_bindings,
                entries: &group_entries,
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let pipeline_layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let module = naga::front::wgsl::parse_str(
        "struct VertexOut { @builtin(position) position: vec4f, @location(0) uvw: vec3f }; \
         @group(0) @binding(7) var volume: texture_3d<f32>; \
         @group(0) @binding(42) var volume_sampler: sampler; \
         @vertex fn vs(@location(0) position: vec2f, @location(1) uvw: vec3f) -> VertexOut { return VertexOut(vec4f(position, 0.5, 1.0), uvw); } \
         @fragment fn fs(input: VertexOut) -> @location(0) vec4f { return textureSample(volume, volume_sampler, input.uvw); }",
    )
    .unwrap();
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .unwrap();
    let shader = unsafe {
        device
            .create_shader_module(
                &crate::ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
                },
                crate::ShaderInput::Naga(crate::NagaShader {
                    module: Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let attributes = [
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x3,
            offset: 8,
            shader_location: 1,
        },
    ];
    let buffers = [Some(crate::VertexBufferLayout {
        array_stride: 20,
        step_mode: wgt::VertexStepMode::Vertex,
        attributes: &attributes,
    })];
    let constants = naga::back::PipelineConstants::default();
    let stage = |entry_point| crate::ProgrammableStage {
        module: &shader,
        entry_point,
        constants: &constants,
        zero_initialize_workgroup_memory: false,
    };
    let targets = [Some(wgt::TextureFormat::Rgba8Unorm.into())];
    let pipeline = unsafe {
        device
            .create_render_pipeline(&crate::RenderPipelineDescriptor {
                label: None,
                layout: &pipeline_layout,
                vertex_processor: crate::VertexProcessor::Standard {
                    vertex_buffers: &buffers,
                    vertex_stage: stage("vs"),
                },
                primitive: wgt::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: None,
                multisample: wgt::MultisampleState::default(),
                fragment_stage: Some(stage("fs")),
                color_targets: &targets,
                multiview_mask: None,
                cache: None,
            })
            .unwrap()
    };

    let target = unsafe {
        device
            .create_texture(&TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 8,
                    height: 8,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage: wgt::TextureUses::COLOR_TARGET | wgt::TextureUses::COPY_SRC,
                memory_flags: MemoryFlags::empty(),
                view_formats: vec![],
            })
            .unwrap()
    };
    let target_view = unsafe {
        device
            .create_texture_view(
                &target,
                &TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };

    let mut vertices = Vec::with_capacity(24 * 5);
    for quad in 0..4 {
        let qx = quad % 2;
        let qy = quad / 2;
        let left = -1.0 + qx as f32;
        let right = left + 1.0;
        let top = 1.0 - qy as f32;
        let bottom = top - 1.0;
        let z = (quad as f32 + 0.5) / 4.0;
        for [x, y, u, v] in [
            [left, top, 0.0, 0.0],
            [left, bottom, 0.0, 1.0],
            [right, top, 1.0, 0.0],
            [right, top, 1.0, 0.0],
            [left, bottom, 0.0, 1.0],
            [right, bottom, 1.0, 1.0],
        ] {
            vertices.extend_from_slice(&[x, y, u, v, z]);
        }
    }
    let vertex_data: Vec<u8> = vertices.into_iter().flat_map(f32::to_ne_bytes).collect();
    let vertex_buffer = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: vertex_data.len() as u64,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex_buffer, 0, &vertex_data)?;

    let output_offset = 12usize;
    let output_row_pitch = 256usize;
    let output_size = output_offset + output_row_pitch * 8;
    let output = unsafe {
        device
            .create_buffer(&BufferDescriptor {
                label: None,
                size: output_size as u64,
                usage: wgt::BufferUses::COPY_SRC | wgt::BufferUses::COPY_DST,
                memory_flags: MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&output, 0, &vec![0x5a; output_size])?;
    let readback = BufferTextureCopy {
        buffer_layout: wgt::TexelCopyBufferLayout {
            offset: output_offset as u64,
            bytes_per_row: Some(output_row_pitch as u32),
            rows_per_image: Some(8),
        },
        texture_base: base(0, 0),
        size: extent(8, 8),
    };
    let colors = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &target_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::TRANSPARENT,
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent: wgt::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        sample_count: 1,
        color_attachments: &colors,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let commands = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.copy_buffer_to_texture(&input, &volume, [upload].into_iter());
        if let (Some(copy_target), Some(readback)) =
            (copy_target.as_ref(), volume_readback.as_ref())
        {
            let copy_region = TextureCopy {
                src_base: TextureCopyBase {
                    mip_level: 0,
                    array_layer: 0,
                    origin: wgt::Origin3d { x: 1, y: 1, z: 1 },
                    aspect: FormatAspects::COLOR,
                },
                dst_base: TextureCopyBase {
                    mip_level: 0,
                    array_layer: 0,
                    origin: wgt::Origin3d { x: 1, y: 1, z: 1 },
                    aspect: FormatAspects::COLOR,
                },
                size: CopyExtent {
                    width: 2,
                    height: 2,
                    depth: 2,
                },
            };
            let readback_region = BufferTextureCopy {
                buffer_layout: wgt::TexelCopyBufferLayout {
                    offset: 16,
                    bytes_per_row: Some(256),
                    rows_per_image: Some(4),
                },
                texture_base: TextureCopyBase {
                    mip_level: 0,
                    array_layer: 0,
                    origin: wgt::Origin3d { x: 0, y: 0, z: 0 },
                    aspect: FormatAspects::COLOR,
                },
                size: CopyExtent {
                    width: 4,
                    height: 4,
                    depth: 4,
                },
            };
            encoder.copy_texture_to_texture(
                &volume,
                wgt::TextureUses::COPY_SRC,
                copy_target,
                [copy_region].into_iter(),
            );
            encoder.copy_texture_to_buffer(
                copy_target,
                wgt::TextureUses::COPY_SRC,
                readback,
                [readback_region].into_iter(),
            );
        }
        encoder.begin_render_pass(&pass).unwrap();
        encoder.set_render_pipeline(&pipeline);
        encoder.set_bind_group(&pipeline_layout, 0, &group, &[]);
        encoder.set_vertex_buffer(
            0,
            crate::BufferBinding::new_unchecked(&vertex_buffer, 0, None),
        );
        encoder.draw(0, 24, 0, 1);
        encoder.end_render_pass();
        encoder.copy_texture_to_buffer(
            &target,
            wgt::TextureUses::COPY_SRC,
            &output,
            [readback].into_iter(),
        );
        encoder.end_encoding().unwrap()
    };
    let fence = native.create_native_fence();
    unsafe { queue.submit(&[&commands], &[], (&fence, 1)).unwrap() };
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(10)))?);

    let actual = native.read_byte_buffer(&output, 0, output_size)?;
    if let Some(readback) = volume_readback.as_ref() {
        let actual_volume = native.read_byte_buffer(readback, 0, 3860)?;
        let mut expected_volume = vec![0x5a; 3860];
        for z in 0..4usize {
            for y in 0..4usize {
                for x in 0..4usize {
                    let value =
                        if (1..=2).contains(&x) && (1..=2).contains(&y) && (1..=2).contains(&z) {
                            40 + (x - 1) as u8 * 80 + (y - 1) as u8 * 40 + (z - 1) as u8 * 20
                        } else {
                            0
                        };
                    expected_volume[16 + z * 1024 + y * 256 + x] = value;
                }
            }
        }
        assert_eq!(
            actual_volume, expected_volume,
            "R8 3D copy or padded texture-to-buffer readback differed"
        );
        std::println!(
            "PASS: R8 3D texture copy and readback; 64 texels, partial-copy zero preservation, offset, padded rows/depth and sentinel bytes matched"
        );
    }
    let mut expected = vec![0x5a; output_size];
    for y in 0..8usize {
        for x in 0..8usize {
            let z = (y / 4) * 2 + x / 4;
            let voxel_x = x % 4;
            let voxel_y = y % 4;
            let pixel =
                if (1..=2).contains(&voxel_x) && (1..=2).contains(&voxel_y) && (1..=2).contains(&z)
                {
                    if bytes_per_texel == 1 {
                        [
                            40 + (voxel_x - 1) as u8 * 80
                                + (voxel_y - 1) as u8 * 40
                                + (z - 1) as u8 * 20,
                            0,
                            0,
                            255,
                        ]
                    } else {
                        [
                            40 + (voxel_x - 1) as u8 * 80,
                            20 + (voxel_y - 1) as u8 * 90,
                            30 + (z - 1) as u8 * 70,
                            255,
                        ]
                    }
                } else {
                    [0, 0, 0, if bytes_per_texel == 1 { 255 } else { 0 }]
                };
            let offset = output_offset + y * output_row_pitch + x * 4;
            expected[offset..offset + 4].copy_from_slice(&pixel);
        }
    }
    assert_eq!(
        actual, expected,
        "padded 3D upload sample or untouched voxel differed"
    );
    std::println!(
        "PASS: FL9_1 padded partial {label} 3D upload; 8 uploaded voxels and 56 initialized untouched voxels sampled correctly, with row/depth padding preserved"
    );

    unsafe {
        encoder.reset_all([commands].into_iter());
        device.destroy_buffer(input);
        if let Some(readback) = volume_readback {
            device.destroy_buffer(readback);
        }
        device.destroy_buffer(vertex_buffer);
        device.destroy_buffer(output);
        device.destroy_bind_group(group);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_pipeline_layout(pipeline_layout);
        device.destroy_sampler(sampler);
        device.destroy_texture_view(volume_view);
        device.destroy_texture_view(target_view);
        device.destroy_texture(volume);
        if let Some(copy_target) = copy_target {
            device.destroy_texture(copy_target);
        }
        device.destroy_texture(target);
        device.destroy_render_pipeline(pipeline);
        device.destroy_shader_module(shader);
    }
    Ok(())
}

#[test]
fn fl91_padded_partial_3d_upload_preserves_other_voxels() -> windows::core::Result<()> {
    run_fl91_padded_partial_3d_upload(
        wgt::TextureFormat::Rgba8Unorm,
        windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        4,
        "RGBA8",
    )
}

#[test]
fn fl91_r8_3d_upload_and_sampling_preserve_other_voxels() -> windows::core::Result<()> {
    run_fl91_padded_partial_3d_upload(
        wgt::TextureFormat::R8Unorm,
        windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8_UNORM,
        1,
        "R8",
    )
}
