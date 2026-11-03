//! Tests of [`wgpu::RenderPipeline`] and related.

use wgpu_test::fail;

#[test]
fn zero_and_small_inter_stage_limits_validate_without_panicking() {
    fn check(
        device: &wgpu::Device,
        source: &str,
        topology: wgpu::PrimitiveTopology,
    ) -> Option<wgpu::Error> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let _pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::TextureFormat::Rgba8Unorm.into())],
            }),
            multiview_mask: None,
            cache: None,
        });
        pollster::block_on(scope.pop())
    }
    let plain = "@vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }";
    let location = "struct Out { @builtin(position) pos: vec4f, @location(0) value: vec4f } @vertex fn vs() -> Out { return Out(vec4f(0.0), vec4f(1.0)); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }";
    let builtin = "@vertex fn vs() -> @builtin(position) vec4f { return vec4f(0.0); } @fragment fn fs(@builtin(front_facing) f: bool) -> @location(0) vec4f { return select(vec4f(0.0), vec4f(1.0), f); }";
    let clip = "enable clip_distances; struct Out { @builtin(position) pos: vec4f, @builtin(clip_distances) clip: array<f32, 8> } @vertex fn vs() -> Out { return Out(vec4f(0.0), array<f32, 8>()); } @fragment fn fs() -> @location(0) vec4f { return vec4f(1.0); }";
    for limit in [0, 1] {
        let (device, _) = wgpu::Device::noop(&wgpu::DeviceDescriptor {
            required_limits: wgpu::Limits {
                max_inter_stage_shader_variables: limit,
                ..Default::default()
            },
            required_features: wgpu::Features::CLIP_DISTANCES,
            ..Default::default()
        });
        assert!(check(&device, plain, wgpu::PrimitiveTopology::TriangleList).is_none());
        let point = check(&device, plain, wgpu::PrimitiveTopology::PointList);
        assert_eq!(point.is_some(), limit == 0);
        let facing = check(&device, builtin, wgpu::PrimitiveTopology::TriangleList);
        assert_eq!(facing.is_some(), limit == 0);
        let output = check(&device, location, wgpu::PrimitiveTopology::TriangleList);
        assert_eq!(output.is_some(), limit == 0);
        assert!(check(&device, location, wgpu::PrimitiveTopology::PointList).is_some());
        assert!(check(&device, clip, wgpu::PrimitiveTopology::TriangleList).is_some());
        if let Some(error) = output {
            assert!(error
                .to_string()
                .contains("max_inter_stage_shader_variables"));
        }
    }
}

#[test]
fn reject_fragment_shader_output_over_max_color_attachments() {
    let (device, _queue) = wgpu::Device::noop(&Default::default());

    // NOTE: Vertex shader is a boring quad. The fragment shader is the interesting part.
    let source = format!(
        "\
@vertex
fn vert(@builtin(vertex_index) vertex_index : u32) -> @builtin(position) vec4f {{
    var pos = array<vec2f, 3>(
        vec2(0.0, 0.5),
        vec2(-0.5, -0.5),
        vec2(0.5, -0.5)
    );
    return vec4f(pos[vertex_index], 0.0, 1.0);
}}

@fragment
fn frag() -> @location({}) vec4f {{
    return vec4(1.0, 0.0, 0.0, 1.0);
}}
",
        device.limits().max_color_attachments
    );

    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let module = &module;

    fail(
        &device,
        || {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                layout: None,
                label: None,
                vertex: wgpu::VertexState {
                    module,
                    entry_point: None,
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                fragment: Some(wgpu::FragmentState {
                    module,
                    entry_point: None,
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        blend: None,
                        write_mask: Default::default(),
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        },
        Some(concat!(
            "Location[8] Float32x4 interpolated as Some(Perspective) ",
            "with sampling Some(Center)'s index exceeds the `max_color_attachments` limit (8)"
        )),
    );
}
