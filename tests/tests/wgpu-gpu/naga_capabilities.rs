use wgpu_test::{
    gpu_test, GpuTestConfiguration, GpuTestInitializer, TestParameters, TestingContext,
};

pub fn all_tests(vec: &mut Vec<GpuTestInitializer>) {
    vec.push(VALIDATE_CAPABILITIES);
}

pub fn validate_capabilities(ctx: TestingContext) {
    use naga::valid::Capabilities as Caps;
    let device_caps = wgpu_core::device::features_to_naga_capabilities(
        ctx.adapter.features(),
        ctx.adapter.get_downlevel_capabilities().flags,
    );
    let max_caps = match ctx.adapter.get_info().backend {
        wgpu::Backend::Vulkan => naga::back::spv::supported_capabilities(),
        wgpu::Backend::Dx11 => naga::back::hlsl::supported_capabilities(),
        // TODO: when mesh shaders land, change this
        wgpu::Backend::Dx12 => naga::back::hlsl::supported_capabilities() | Caps::MESH_SHADER,
        wgpu::Backend::Metal => naga::back::msl::supported_capabilities() | Caps::MESH_SHADER,
        wgpu::Backend::Gl => naga::back::glsl::supported_capabilities(),
        wgpu::Backend::BrowserWebGpu => naga::back::wgsl::supported_capabilities(),
        wgpu::Backend::Noop => Caps::all(),
    };
    let diff = device_caps - max_caps;
    assert_eq!(diff, Caps::empty());
}

fn test_limits() -> wgpu::Limits {
    let mut limits = wgpu::Limits::downlevel_defaults();
    limits.max_texture_array_layers = 1;
    limits.max_sampled_textures_per_shader_stage = 8;
    limits.max_dynamic_uniform_buffers_per_pipeline_layout = 2;
    limits.max_uniform_buffers_per_shader_stage = 6;
    limits.max_uniform_buffer_binding_size = 512;
    limits.max_inter_stage_shader_variables = 8;
    limits.max_color_attachments = 1;
    limits.max_color_attachment_bytes_per_sample = 8;
    limits.max_buffers_and_acceleration_structures_per_shader_stage = 17;
    limits
}

fn test_parameters() -> TestParameters {
    let mut parameters = TestParameters::default()
        .downlevel_flags(wgpu::DownlevelFlags::empty())
        .limits(test_limits());
    parameters
        .required_downlevel_caps
        .limits
        .border_addressing_requires_comparison_sampler = true;
    parameters
}

#[gpu_test]
static VALIDATE_CAPABILITIES: GpuTestConfiguration = GpuTestConfiguration::new()
    .parameters(test_parameters())
    .run_sync(validate_capabilities);
