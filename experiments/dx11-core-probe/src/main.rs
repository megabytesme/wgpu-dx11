fn main() {
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::DX11;
    let instance = wgpu::Instance::new(desc);
    let hal = unsafe { instance.as_hal::<wgpu_hal::api::Dx11>() }.expect("DX11 instance");
    let (raw_adapter, raw_device) = unsafe { hal.open_internal_core_probe() }.expect("native device").expect("hardware adapter");
    let limits = raw_adapter.capabilities.limits.clone();
    let adapter = unsafe { instance.create_adapter_from_hal::<wgpu_hal::api::Dx11>(raw_adapter) };
    let desc = wgpu::DeviceDescriptor { required_limits: limits, ..Default::default() };
    let (device, queue) = unsafe { adapter.create_device_from_hal::<wgpu_hal::api::Dx11>(raw_device, &desc) }.expect("core device startup");
    device.on_uncaptured_error(std::sync::Arc::new(|error| eprintln!("wgpu error: {error:?}")));
    queue.submit([]);
    device.poll(wgpu::PollType::wait_indefinitely()).expect("device poll");
    let source = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: true,
    });
    source.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(&[17; 16]);
    source.unmap();
    let destination = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(&source, 0, &destination, 0, 16);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    destination.slice(..).map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).expect("copy poll");
    rx.recv().expect("map callback").expect("map read");
    assert_eq!(&*destination.slice(..).get_mapped_range().unwrap(), &[17; 16]);
    destination.unmap();
    println!("core device startup, empty submit, and mapped GPU copy succeeded");
    let preflight = device.create_buffer(&wgpu::BufferDescriptor { label: Some("preflight vertex"), size: 24, usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: true });
    println!("preflight vertex mapping: {:?}", preflight.slice(..).get_mapped_range_mut().map(|_| ()));
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl("@vertex fn vs(@location(0) p: vec2f) -> @builtin(position) vec4f { return vec4f(p, 0.5, 1.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(0.0, 1.0, 0.0, 1.0); }".into()),
    });
    let empty = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor { label: None, entries: &[] });
    let empty_group = device.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &empty, entries: &[] });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&empty)], immediate_size: 0 });
    let pipeline_scope = device.push_error_scope(wgpu::ErrorFilter::Internal);
    let pipeline_validation_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: None, layout: Some(&layout),
        vertex: wgpu::VertexState { module: &shader, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[Some(wgpu::VertexBufferLayout { array_stride: 8, step_mode: wgpu::VertexStepMode::Vertex, attributes: &[wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 }] })] },
        fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some("fs"), compilation_options: Default::default(), targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())] }),
        primitive: Default::default(), depth_stencil: None, multisample: Default::default(), multiview_mask: None, cache: None,
    });
    eprintln!("pipeline validation: {:?}; internal: {:?}", pollster::block_on(pipeline_validation_scope.pop()), pollster::block_on(pipeline_scope.pop()));
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let vertex = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 24, usage: wgpu::BufferUsages::VERTEX, mapped_at_creation: true });
    eprintln!("vertex internal error: {:?}", pollster::block_on(scope.pop()));
    let mut bytes = Vec::new();
    for f in [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0] { bytes.extend_from_slice(&f.to_le_bytes()); }
    vertex.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(&bytes);
    vertex.unmap();
    let index = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 8, usage: wgpu::BufferUsages::INDEX, mapped_at_creation: true });
    index.slice(..).get_mapped_range_mut().unwrap().copy_from_slice(&[0, 0, 1, 0, 2, 0, 0, 0]);
    index.unmap();
    let texture = device.create_texture(&wgpu::TextureDescriptor { label: None, size: wgpu::Extent3d { width: 8, height: 8, depth_or_array_layers: 1 }, mip_level_count: 1, sample_count: 1, dimension: wgpu::TextureDimension::D2, format: wgpu::TextureFormat::Rgba8Unorm, usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC, view_formats: &[] });
    let view = texture.create_view(&Default::default());
    let pixels = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: 2048, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor { label: None, color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view, depth_slice: None, resolve_target: None, ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store } })], depth_stencil_attachment: None, timestamp_writes: None, occlusion_query_set: None, multiview_mask: None });
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &empty_group, &[]);
        pass.set_vertex_buffer(0, vertex.slice(..));
        pass.set_index_buffer(index.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..3, 0, 0..1);
    }
    encoder.copy_texture_to_buffer(wgpu::TexelCopyTextureInfo { texture: &texture, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All }, wgpu::TexelCopyBufferInfo { buffer: &pixels, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(256), rows_per_image: Some(8) } }, texture.size());
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    pixels.slice(..).map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).expect("draw poll");
    rx.recv().unwrap().unwrap();
    let mapped = pixels.slice(..).get_mapped_range().unwrap();
    assert_eq!(&mapped[4 * 256 + 4 * 4..4 * 256 + 5 * 4], &[0, 255, 0, 255]);
    drop(mapped);
    pixels.unmap();
    println!("core indexed triangle produced expected pixel");
}
