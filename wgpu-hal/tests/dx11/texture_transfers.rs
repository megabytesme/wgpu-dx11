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
    assert!(unsafe { device.create_texture(&unsupported) }.is_err());
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
    assert_eq!(native.sampled_view(&view)?.1, [4, 3]);
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
