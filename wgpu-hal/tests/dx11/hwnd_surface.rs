use super::*;
use crate::{CommandEncoder as _, Device as _, Instance as _, Queue as _, Surface as _};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use windows::{
    core::w,
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED},
    },
};

struct WindowGuard(HWND);

impl Drop for WindowGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

unsafe fn record_clone_sample(
    device: &super::super::Device,
    native: &Arc<NativeDevice>,
    encoder: &mut super::super::CommandEncoder,
) -> windows::core::Result<()> {
    let texture = unsafe {
        device
            .create_texture(&crate::TextureDescriptor {
                label: None,
                size: wgt::Extent3d {
                    width: 4,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 3,
                sample_count: 1,
                dimension: wgt::TextureDimension::D2,
                format: wgt::TextureFormat::Rgba8Unorm,
                usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
                view_formats: alloc::vec![],
            })
            .unwrap()
    };
    native.write_rgba_texture(&texture, 0, 0, &[255, 0, 0, 255].repeat(16))?;
    let sampled = unsafe {
        device
            .create_texture_view(
                &texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange {
                        base_mip_level: 0,
                        mip_level_count: Some(1),
                        ..Default::default()
                    },
                },
            )
            .unwrap()
    };
    assert!(native.sampled_binding(&sampled)?.0.refresh.is_some());
    let sampler = unsafe {
        device
            .create_sampler(&crate::SamplerDescriptor {
                label: None,
                address_modes: [wgt::AddressMode::ClampToEdge; 3],
                mag_filter: wgt::FilterMode::Nearest,
                min_filter: wgt::FilterMode::Nearest,
                mipmap_filter: wgt::MipmapFilterMode::Nearest,
                lod_clamp: 0.0..32.0,
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
                sample_type: wgt::TextureSampleType::Float { filterable: true },
                view_dimension: wgt::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        },
        wgt::BindGroupLayoutEntry {
            binding: 42,
            visibility: wgt::ShaderStages::FRAGMENT,
            ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
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
    let layout = unsafe {
        device
            .create_pipeline_layout(&crate::PipelineLayoutDescriptor {
                label: None,
                flags: crate::PipelineLayoutFlags::empty(),
                bind_group_layouts: &[Some(&group_layout)],
                immediate_size: 0,
            })
            .unwrap()
    };
    let group = unsafe {
        device
            .create_bind_group(&crate::BindGroupDescriptor {
                label: None,
                layout: &group_layout,
                buffers: &[],
                textures: &[crate::TextureBinding {
                    view: &sampled,
                    usage: wgt::TextureUses::RESOURCE,
                }],
                samplers: &[&sampler],
                entries: &[
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
                ],
                acceleration_structures: &[],
                external_textures: &[],
            })
            .unwrap()
    };
    let module = naga::front::wgsl::parse_str(r#"
        @group(0) @binding(7) var image: texture_2d<f32>;
        @group(0) @binding(42) var samp: sampler;
        @vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); }
        @fragment fn fs() -> @location(0) vec4f { return textureSample(image, samp, vec2f(0.5)); }
    "#).unwrap();
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
                    module: alloc::borrow::Cow::Owned(module),
                    info,
                    debug_source: None,
                }),
            )
            .unwrap()
    };
    let attributes = [wgt::VertexAttribute {
        format: wgt::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    }];
    let vertex_buffers = [Some(crate::VertexBufferLayout {
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
                    vertex_buffers: &vertex_buffers,
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
    let data: alloc::vec::Vec<_> = [-1f32, -1., 3., -1., -1., 3.]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect();
    let vertex = unsafe {
        device
            .create_buffer(&crate::BufferDescriptor {
                label: None,
                size: 24,
                usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                memory_flags: crate::MemoryFlags::empty(),
            })
            .unwrap()
    };
    native.write_byte_buffer(&vertex, 0, &data)?;
    unsafe {
        encoder.set_render_pipeline(&pipeline);
        encoder.set_vertex_buffer(0, crate::BufferBinding::new_unchecked(&vertex, 0, None));
        encoder.set_bind_group(&layout, 0, &group, &[]);
        encoder.draw(0, 3, 0, 1);
        device.destroy_buffer(vertex);
        device.destroy_render_pipeline(pipeline);
        device.destroy_shader_module(shader);
        device.destroy_bind_group(group);
        device.destroy_pipeline_layout(layout);
        device.destroy_bind_group_layout(group_layout);
        device.destroy_sampler(sampler);
        device.destroy_texture_view(sampled);
        device.destroy_texture(texture);
    }
    Ok(())
}

#[test]
fn fl91_hidden_hwnd_surface_renders_and_resizes() -> windows::core::Result<()> {
    run_hidden_hwnd(false)
}

#[test]
fn fl91_hidden_hwnd_clone_sampling_presents_and_resizes() -> windows::core::Result<()> {
    run_hidden_hwnd(true)
}

fn run_hidden_hwnd(clone_sample: bool) -> windows::core::Result<()> {
    let adapters = super::super::enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[super::super::FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), super::super::FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested FL9_1; actual {:?}",
        adapter.identity(),
        native.feature_level()
    );
    if clone_sample {
        use windows::Win32::Graphics::{
            Direct3D11::{
                D3D11_FORMAT_SUPPORT_MIP, D3D11_FORMAT_SUPPORT_SHADER_SAMPLE,
                D3D11_FORMAT_SUPPORT_TEXTURE2D,
            },
            Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        };
        let flags = (D3D11_FORMAT_SUPPORT_MIP.0
            | D3D11_FORMAT_SUPPORT_SHADER_SAMPLE.0
            | D3D11_FORMAT_SUPPORT_TEXTURE2D.0) as u32;
        let support = unsafe {
            native
                .raw_device()
                .CheckFormatSupport(DXGI_FORMAT_R8G8B8A8_UNORM)
        };
        if !matches!(&support, Ok(bits) if bits & flags == flags) {
            assert!(
                !required,
                "strict mode requires RGBA8 sampled mips {flags:#x}: {support:?}"
            );
            std::println!("SKIP: RGBA8 sampled mip support {flags:#x}: {support:?}");
            return Ok(());
        }
    }
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("wgpu dx11 test"),
            WS_OVERLAPPED,
            0,
            0,
            64,
            64,
            None,
            None,
            None,
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) if !required => {
            std::println!("SKIP: could not create a test HWND: {error}");
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let _window = WindowGuard(hwnd);
    let handle = NonZeroIsize::new(hwnd.0 as isize).unwrap();
    let surface = unsafe {
        super::super::Instance
            .create_surface(
                RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                RawWindowHandle::Win32(Win32WindowHandle::new(handle)),
            )
            .unwrap()
    };
    let device = super::super::Device::new(native.clone());
    let queue = super::super::Queue::new(native.clone());
    let fence = native.create_native_fence();
    let mut config = crate::SurfaceConfiguration {
        maximum_frame_latency: 1,
        present_mode: wgt::PresentMode::Fifo,
        composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
        format: wgt::TextureFormat::Bgra8Unorm,
        color_space: wgt::SurfaceColorSpace::Srgb,
        extent: wgt::Extent3d {
            width: 32,
            height: 24,
            depth_or_array_layers: 1,
        },
        usage: wgt::TextureUses::COLOR_TARGET,
        view_formats: alloc::vec::Vec::new(),
    };
    unsafe {
        surface.configure(&device, &config).unwrap();
    }
    let active_surface_operation = surface.begin_operation().unwrap();
    assert!(matches!(
        unsafe { surface.acquire_texture(None, &fence) },
        Err(crate::SurfaceError::Other(
            "D3D11 surface operation already in progress"
        ))
    ));
    assert!(matches!(
        unsafe { surface.configure(&device, &config) },
        Err(crate::SurfaceError::Other(
            "D3D11 surface operation already in progress"
        ))
    ));
    drop(active_surface_operation);
    let chain = surface
        .configured
        .lock()
        .as_ref()
        .unwrap()
        .swapchain
        .clone();
    let guard = native.immediate_context();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(chain.size()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    drop(guard);
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap()?, [32, 24]);
    worker.join().unwrap();
    let acquired = unsafe { surface.acquire_texture(None, &fence).unwrap() };
    assert!(unsafe { surface.acquire_texture(None, &fence) }.is_err());
    let texture: &NativeTexture = acquired.texture.borrow();
    let view = unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Bgra8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: if clone_sample {
            wgt::Color::BLUE
        } else {
            wgt::Color::RED
        },
    })];
    let pass = crate::RenderPassDescriptor {
        label: None,
        extent: config.extent,
        sample_count: 1,
        color_attachments: &color,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        if clone_sample {
            record_clone_sample(&device, &native, &mut encoder)?;
        }
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        queue
            .submit(&[&command], &[&acquired.texture], (&fence, 1))
            .unwrap();
    }
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixel = acquired.texture.frame.as_ref().unwrap().readback()?;
    assert_eq!(pixel, [0, 0, 255, 255]);
    assert!(command
        .retain_surface_targets(&mut alloc::vec::Vec::new())
        .is_ok());
    unsafe {
        device.destroy_texture_view(view);
    }
    assert!(command
        .retain_surface_targets(&mut alloc::vec::Vec::new())
        .is_err());
    let present = unsafe { queue.present(&surface, acquired.texture) };
    assert!(
        present.is_ok() || matches!(present, Err(crate::SurfaceError::Occluded)),
        "{present:?}"
    );
    std::println!("Hidden HWND present: {present:?}; BGRA pixel {pixel:?}");
    unsafe {
        queue.wait_for_idle().unwrap();
    }
    config.extent.width = 24;
    config.extent.height = 16;
    unsafe {
        surface.configure(&device, &config).unwrap();
    }
    std::println!("Resize with completed command buffer retained: PASS");
    let resized = unsafe { surface.acquire_texture(None, &fence).unwrap() };
    let swapchain = surface
        .configured
        .lock()
        .as_ref()
        .unwrap()
        .swapchain
        .clone();
    let size = swapchain.size()?;
    assert_eq!(size, [24, 16]);
    drop(swapchain);
    let resized_view = unsafe {
        device
            .create_texture_view(
                resized.texture.borrow(),
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Bgra8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let resized_color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &resized_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::GREEN,
    })];
    let resized_pass = crate::RenderPassDescriptor {
        label: None,
        extent: config.extent,
        sample_count: 1,
        color_attachments: &resized_color,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let resized_command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&resized_pass).unwrap();
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        queue
            .submit(&[&resized_command], &[&resized.texture], (&fence, 2))
            .unwrap();
    }
    assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
    assert_eq!(
        resized.texture.frame.as_ref().unwrap().readback()?,
        [0, 255, 0, 255]
    );
    unsafe {
        device.destroy_texture_view(resized_view);
        surface.discard_texture(resized.texture);
        surface.unconfigure(&device);
        surface.configure(&device, &config).unwrap();
        let recreated = surface.acquire_texture(None, &fence).unwrap();
        surface.discard_texture(recreated.texture);
        queue.wait_for_idle().unwrap();
        encoder.reset_all([command, resized_command].into_iter());
    }
    std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::channel();
        let surface = &surface;
        scope.spawn(move || {
            let _operation = surface.begin_operation().unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        });
        rx.recv().unwrap();
        unsafe { surface.unconfigure(&device) };
    });
    unsafe {
        surface.configure(&device, &config).unwrap();
        let recreated = surface.acquire_texture(None, &fence).unwrap();
        surface.discard_texture(recreated.texture);
        surface.unconfigure(&device);
    }
    std::println!("Unconfigure after concurrent reservation and recreate: PASS");
    std::println!(
        "PASS: hidden HWND acquire, HAL {} readback, present and resize; visibility not verified",
        if clone_sample {
            "clone-sampled draw"
        } else {
            "clear"
        }
    );
    Ok(())
}
