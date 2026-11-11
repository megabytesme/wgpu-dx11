#![allow(unused_variables)]

use alloc::{string::String, sync::Arc, vec, vec::Vec};
use core::{ptr, sync::atomic::Ordering, time::Duration};

#[cfg(supports_64bit_atomics)]
use core::sync::atomic::AtomicU64;
#[cfg(not(supports_64bit_atomics))]
use portable_atomic::AtomicU64;

use crate::TlasInstance;

mod buffer;
pub use buffer::Buffer;
mod command;
pub use command::CommandBuffer;

#[derive(Clone, Debug)]
pub struct Api;

#[derive(Debug)]
pub struct Context {
    options: Arc<wgt::NoopBackendOptions>,
}
#[derive(Debug)]
pub struct Encoder;
#[derive(Debug)]
pub struct Resource;

#[derive(Debug)]
pub struct Fence {
    value: AtomicU64,
}

type DeviceResult<T> = Result<T, crate::DeviceError>;

impl crate::Api for Api {
    const VARIANT: wgt::Backend = wgt::Backend::Noop;

    type Instance = Context;
    type Surface = Context;
    type Adapter = Context;
    type Device = Context;

    type Queue = Context;
    type CommandEncoder = CommandBuffer;
    type CommandBuffer = CommandBuffer;

    type Buffer = Buffer;
    type Texture = Resource;
    type SurfaceTexture = Resource;
    type TextureView = Resource;
    type Sampler = Resource;
    type QuerySet = Resource;
    type Fence = Fence;
    type AccelerationStructure = Resource;
    type PipelineCache = Resource;

    type BindGroupLayout = Resource;
    type BindGroup = Resource;
    type PipelineLayout = Resource;
    type ShaderModule = Resource;
    type RenderPipeline = Resource;
    type RayTracingPipeline = Resource;
    type ComputePipeline = Resource;
}

crate::impl_dyn_resource!(Buffer, CommandBuffer, Context, Fence, Resource);

impl crate::DynAccelerationStructure for Resource {}
impl crate::DynBindGroup for Resource {}
impl crate::DynBindGroupLayout for Resource {}
impl crate::DynBuffer for Buffer {}
impl crate::DynCommandBuffer for CommandBuffer {}
impl crate::DynComputePipeline for Resource {}
impl crate::DynFence for Fence {}
impl crate::DynPipelineCache for Resource {}
impl crate::DynPipelineLayout for Resource {}
impl crate::DynQuerySet for Resource {}
impl crate::DynRenderPipeline for Resource {}
impl crate::DynRayTracingPipeline for Resource {}
impl crate::DynSampler for Resource {}
impl crate::DynShaderModule for Resource {}
impl crate::DynSurfaceTexture for Resource {}
impl crate::DynTexture for Resource {}
impl crate::DynTextureView for Resource {}

impl core::borrow::Borrow<dyn crate::DynTexture> for Resource {
    fn borrow(&self) -> &dyn crate::DynTexture {
        self
    }
}

impl crate::Instance for Context {
    type A = Api;

    unsafe fn init(desc: &crate::InstanceDescriptor<'_>) -> Result<Self, crate::InstanceError> {
        let options = Arc::new(desc.backend_options.noop.clone());
        if options.enable {
            Ok(Context { options })
        } else {
            Err(crate::InstanceError::new(String::from(
                "noop backend disabled because NoopBackendOptions::enable is false",
            )))
        }
    }
    unsafe fn create_surface(
        &self,
        _display_handle: raw_window_handle::RawDisplayHandle,
        _window_handle: raw_window_handle::RawWindowHandle,
    ) -> Result<Context, crate::InstanceError> {
        Ok(Context {
            options: Arc::clone(&self.options),
        })
    }
    unsafe fn enumerate_adapters(
        &self,
        _surface_hint: Option<&Context>,
    ) -> Vec<crate::ExposedAdapter<Api>> {
        let device_type = self.options.device_type.unwrap_or(wgt::DeviceType::Other);
        let subgroup_min_size = self
            .options
            .subgroup_min_size
            .unwrap_or(wgt::MINIMUM_SUBGROUP_MIN_SIZE);
        let subgroup_max_size = self
            .options
            .subgroup_max_size
            .unwrap_or(wgt::MAXIMUM_SUBGROUP_MAX_SIZE);
        let features = self.options.features.unwrap_or(wgt::Features::all());
        let limits = self
            .options
            .limits
            .clone()
            .unwrap_or(CAPABILITIES.limits.clone());

        let info = wgt::AdapterInfo {
            subgroup_min_size,
            subgroup_max_size,
            ..adapter_info()
        };

        let capabilities = crate::Capabilities {
            limits,
            ..CAPABILITIES
        };

        vec![crate::ExposedAdapter {
            adapter: Context {
                options: Arc::clone(&self.options),
            },
            info,
            features,
            capabilities,
        }]
    }
}

/// Returns the adapter info for the noop backend.
///
/// This is used in the test harness to construct info about
/// the noop backend adapter without actually initializing wgpu.
pub fn adapter_info() -> wgt::AdapterInfo {
    wgt::AdapterInfo {
        name: String::from("noop wgpu backend"),
        driver: String::from("wgpu"),
        ..wgt::AdapterInfo::new(wgt::DeviceType::Cpu, wgt::Backend::Noop)
    }
}

/// The capabilities of the noop backend.
///
/// This is used in the test harness to construct capabilities
/// of the noop backend without actually initializing wgpu.
pub const CAPABILITIES: crate::Capabilities = {
    crate::Capabilities {
        limits: wgt::Limits::unlimited(),
        alignments: crate::Alignments {
            // All maximally permissive
            buffer_copy_offset: wgt::BufferSize::MIN,
            buffer_copy_pitch: wgt::BufferSize::MIN,
            uniform_bounds_check_alignment: wgt::BufferSize::MIN,
            raw_tlas_instance_size: 0,
            ray_tracing_scratch_buffer_alignment: 1,
            ray_tracing_pipeline_group_data_size: 1,
            ray_tracing_pipeline_group_data_alignment: 1,
            ray_tracing_pipeline_data_offset_alignment: 1,
        },
        downlevel: wgt::DownlevelCapabilities {
            flags: wgt::DownlevelFlags::all(),
            limits: wgt::DownlevelLimits {
                empty_binding_visibility: true,
                texture_1d: true,
                texture_3d: true,
                texture_3d_usage_mask: wgt::TextureUsages::all().bits(),
                texture_3d_format_mask: u32::MAX,
                texture_3d_mipmaps: true,
                texture_3d_non_suffix_mip_views: true,
                multisampled_texture_usage_mask: wgt::TextureUsages::all().bits(),
                texture_to_buffer_copy_format_mask: u32::MAX,
                max_texture_to_buffer_copy_layers: u32::MAX,
                texture_3d_texture_copies: true,
                npot_texture_3d: true,
                max_texture_cube_dimension: u32::MAX,
                texture_cube_format_mask: u32::MAX,
                npot_cube_textures: true,
                sampled_depth_textures: true,
                comparison_sampler_function_mask: 0x00ff,
                comparison_sampler_independent_filters: true,
                sampler_lod_clamp: true,
                max_comparison_sampler_anisotropy: 16,
                border_addressing_requires_comparison_sampler: false,
                sampled_sint_textures: true,
                sampled_uint_textures: true,
                multisampled_texture_bindings: true,
                alpha_to_coverage: true,
                shader_loops: true,
                shader_dynamic_loops: true,
                frexp_fraction_only_fragment: false,
                vertex_index_input: true,
                vertex_base_vertex_input: true,
                vertex_base_instance_input: true,
                vertex_point_size_output: true,
                vertex_index_input_requires_hidden_slot: false,
                vertex_index_topology_mask: wgt::DownlevelLimits::VERTEX_INDEX_TOPOLOGY_MASK_ALL,
                instance_index_input: true,
                vertex_uniform_array_indexing: false,
                fragment_uniform_array_indexing: false,
                vertex_index_local_array_indexing: false,
                max_sampled_textures_per_vertex_stage: 16,
                max_sampled_texture_sampler_pairs_per_shader_stage: u32::MAX,
                max_vertex_shader_uniform_vectors: u32::MAX,
                max_fragment_shader_uniform_vectors: u32::MAX,
                unrestricted_npot_texture_sampling: true,
                max_texture_repeat_per_primitive: u32::MAX,
                max_sampler_anisotropy: 16,
                src_alpha_saturated_blend: true,
                viewport_rectangles_outside_target: true,
                depth_bias_with_fragment_depth: true,
                fragment_depth_without_color_target_0: true,
                fragment_sample_mask_input: true,
                fragment_sample_mask_output: true,
                fragment_point_coord_input: true,
                fragment_non_perspective_interpolation: true,
                fragment_front_facing: true,
                fragment_position: true,
                texture_query: true,
                texture_sample_base_clamp_to_edge: true,
                texture_sample_base_clamp_to_edge_2d_float_fragment: false,
                texture_sample_level: true,
                texture_gather: true,
                texture_load: true,
                texture_load_2d_float_fragment: true,
                texture_load_3d_float_fragment: false,
                texture_dimensions_2d_float_fragment: false,
                texture_dimensions_3d_float_fragment: false,
                texture_dimensions_cube_float_fragment: false,
                texture_num_levels_2d_float_fragment: false,
                texture_num_levels_3d_float_fragment: false,
                texture_num_levels_cube_float_fragment: false,
                runtime_integer_shader_operations: true,
                matrix_inverse: true,
                matrix_inverse_2x2: false,
                matrix_outer_product: true,
                matrix_aggregate_stores: true,
                max_texture_sample_bias: u32::MAX,
                texture_sample_bias_dynamic: true,
                fragment_texture_sample_bias_literal_select: false,
                texture_sample_bias_offset: true,
                texture_sample_compare_offset: true,
                fragment_texture_sample_bias_offset_2d_single_mip: false,
                fragment_texture_sample_compare_offset_2d_single_mip: false,
                texture_sample_bias_requires_compatible_sampler: false,
                fragment_derivatives: true,
                vertex_format_mask: wgt::DownlevelLimits::webgpu_vertex_format_mask(),
            },
            shader_model: wgt::ShaderModel::Sm5,
        },
        cooperative_matrix_properties: Vec::new(),
    }
};

impl crate::Surface for Context {
    type A = Api;

    unsafe fn configure(
        &self,
        device: &Context,
        config: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {
        Ok(())
    }

    unsafe fn unconfigure(&self, device: &Context) {}

    unsafe fn acquire_texture(
        &self,
        _timeout: Option<Duration>,
        _fence: &Fence,
    ) -> Result<crate::AcquiredSurfaceTexture<Api>, crate::SurfaceError> {
        Err(crate::SurfaceError::Timeout)
    }
    unsafe fn discard_texture(&self, texture: Resource) {}
}

impl crate::Adapter for Context {
    type A = Api;

    unsafe fn open(
        &self,
        features: wgt::Features,
        _limits: &wgt::Limits,
        _memory_hints: &wgt::MemoryHints,
    ) -> DeviceResult<crate::OpenDevice<Api>> {
        Ok(crate::OpenDevice {
            device: Context {
                options: Arc::clone(&self.options),
            },
            queue: Context {
                options: Arc::clone(&self.options),
            },
        })
    }
    unsafe fn texture_format_capabilities(
        &self,
        format: wgt::TextureFormat,
    ) -> crate::TextureFormatCapabilities {
        crate::TextureFormatCapabilities::empty()
    }

    unsafe fn surface_capabilities(&self, surface: &Context) -> Option<crate::SurfaceCapabilities> {
        None
    }

    unsafe fn get_presentation_timestamp(&self) -> wgt::PresentationTimestamp {
        wgt::PresentationTimestamp::INVALID_TIMESTAMP
    }

    fn get_ordered_buffer_usages(&self) -> wgt::BufferUses {
        wgt::BufferUses::INCLUSIVE | wgt::BufferUses::MAP_WRITE
    }

    fn get_ordered_texture_usages(&self) -> wgt::TextureUses {
        wgt::TextureUses::INCLUSIVE
            | wgt::TextureUses::COLOR_TARGET
            | wgt::TextureUses::DEPTH_STENCIL_WRITE
    }
}

impl crate::Queue for Context {
    type A = Api;

    unsafe fn submit(
        &self,
        command_buffers: &[&CommandBuffer],
        surface_textures: &[&Resource],
        (fence, fence_value): (&Fence, crate::FenceValue),
    ) -> DeviceResult<()> {
        // All commands are executed synchronously.
        for cb in command_buffers {
            // SAFETY: Caller is responsible for ensuring synchronization between commands and
            // other mutations.
            unsafe {
                cb.execute();
            }
        }
        fence.value.store(fence_value, Ordering::Release);
        Ok(())
    }
    unsafe fn present(
        &self,
        surface: &Context,
        texture: Resource,
    ) -> Result<(), crate::SurfaceError> {
        Ok(())
    }

    unsafe fn get_timestamp_period(&self) -> f32 {
        1.0
    }

    unsafe fn wait_for_idle(&self) -> Result<(), crate::DeviceError> {
        Ok(())
    }
}

impl crate::Device for Context {
    type A = Api;

    unsafe fn create_buffer(&self, desc: &crate::BufferDescriptor) -> DeviceResult<Buffer> {
        Buffer::new(desc)
    }

    unsafe fn destroy_buffer(&self, buffer: Buffer) {}
    unsafe fn add_raw_buffer(&self, _buffer: &Buffer) {}

    unsafe fn map_buffer(
        &self,
        buffer: &Buffer,
        range: crate::MemoryRange,
    ) -> DeviceResult<crate::BufferMapping> {
        // Safety: the `wgpu-core` validation layer will prevent any user-accessible aliasing
        // mappings from being created, so we don’t need to perform any checks here, except for
        // bounds checks on the range which are built into `get_slice_ptr()`.
        Ok(crate::BufferMapping {
            ptr: ptr::NonNull::new(buffer.get_slice_ptr(range).cast::<u8>()).unwrap(),
            is_coherent: true,
        })
    }
    unsafe fn unmap_buffer(&self, buffer: &Buffer) {}
    unsafe fn flush_mapped_ranges<I>(
        &self,
        buffer: &Buffer,
        ranges: I,
    ) -> Result<(), crate::DeviceError> {
        Ok(())
    }
    unsafe fn invalidate_mapped_ranges<I>(
        &self,
        buffer: &Buffer,
        ranges: I,
    ) -> Result<(), crate::DeviceError> {
        Ok(())
    }

    unsafe fn create_texture(&self, desc: &crate::TextureDescriptor) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_texture(&self, texture: Resource) {}
    unsafe fn add_raw_texture(&self, _texture: &Resource) {}

    unsafe fn create_texture_view(
        &self,
        texture: &Resource,
        desc: &crate::TextureViewDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_texture_view(&self, view: Resource) {}
    unsafe fn create_sampler(&self, desc: &crate::SamplerDescriptor) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_sampler(&self, sampler: Resource) {}

    unsafe fn create_command_encoder(
        &self,
        desc: &crate::CommandEncoderDescriptor<Context>,
    ) -> DeviceResult<CommandBuffer> {
        Ok(CommandBuffer::new())
    }

    unsafe fn create_bind_group_layout(
        &self,
        desc: &crate::BindGroupLayoutDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_bind_group_layout(&self, bg_layout: Resource) {}
    unsafe fn create_pipeline_layout(
        &self,
        desc: &crate::PipelineLayoutDescriptor<Resource>,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_pipeline_layout(&self, pipeline_layout: Resource) {}
    unsafe fn create_bind_group(
        &self,
        desc: &crate::BindGroupDescriptor<Resource, Buffer, Resource, Resource, Resource>,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_bind_group(&self, group: Resource) {}

    unsafe fn create_shader_module(
        &self,
        desc: &crate::ShaderModuleDescriptor,
        shader: crate::ShaderInput,
    ) -> Result<Resource, crate::ShaderError> {
        Ok(Resource)
    }
    unsafe fn destroy_shader_module(&self, module: Resource) {}
    unsafe fn create_render_pipeline(
        &self,
        desc: &crate::RenderPipelineDescriptor<Resource, Resource, Resource>,
    ) -> Result<Resource, crate::PipelineError> {
        Ok(Resource)
    }
    unsafe fn destroy_render_pipeline(&self, pipeline: Resource) {}
    unsafe fn create_compute_pipeline(
        &self,
        desc: &crate::ComputePipelineDescriptor<Resource, Resource, Resource>,
    ) -> Result<Resource, crate::PipelineError> {
        Ok(Resource)
    }
    unsafe fn destroy_compute_pipeline(&self, pipeline: Resource) {}
    unsafe fn create_ray_tracing_pipeline(
        &self,
        desc: &crate::RayTracingPipelineDescriptor<Resource, Resource, Resource>,
    ) -> Result<Resource, crate::PipelineError> {
        Ok(Resource)
    }
    unsafe fn destroy_ray_tracing_pipeline(&self, pipeline: Resource) {}
    unsafe fn get_raytracing_pipeline_group_data(
        &self,
        pipeline: &Resource,
        groups: core::ops::Range<u32>,
    ) -> Result<Vec<u8>, crate::DeviceError> {
        Ok(vec![0; groups.count()])
    }
    unsafe fn create_pipeline_cache(
        &self,
        desc: &crate::PipelineCacheDescriptor<'_>,
    ) -> Result<Resource, crate::PipelineCacheError> {
        Ok(Resource)
    }
    unsafe fn destroy_pipeline_cache(&self, cache: Resource) {}

    unsafe fn create_query_set(
        &self,
        desc: &wgt::QuerySetDescriptor<crate::Label>,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn destroy_query_set(&self, set: Resource) {}
    unsafe fn create_fence(&self) -> DeviceResult<Fence> {
        Ok(Fence {
            value: AtomicU64::new(0),
        })
    }
    unsafe fn destroy_fence(&self, fence: Fence) {}
    unsafe fn get_fence_value(&self, fence: &Fence) -> DeviceResult<crate::FenceValue> {
        Ok(fence.value.load(Ordering::Acquire))
    }
    unsafe fn wait(
        &self,
        fence: &Fence,
        value: crate::FenceValue,
        timeout: Option<Duration>,
    ) -> DeviceResult<bool> {
        // The relevant commands must have already been submitted, and noop-backend commands are
        // executed synchronously, so there is no waiting — either it is already done,
        // or this method was called incorrectly.
        assert!(
            fence.value.load(Ordering::Acquire) >= value,
            "submission must have already been done"
        );
        Ok(true)
    }

    unsafe fn start_graphics_debugger_capture(&self) -> bool {
        false
    }
    unsafe fn stop_graphics_debugger_capture(&self) {}
    unsafe fn create_acceleration_structure(
        &self,
        desc: &crate::AccelerationStructureDescriptor,
    ) -> DeviceResult<Resource> {
        Ok(Resource)
    }
    unsafe fn get_acceleration_structure_build_sizes<'a>(
        &self,
        _desc: &crate::GetAccelerationStructureBuildSizesDescriptor<'a, Buffer>,
    ) -> crate::AccelerationStructureBuildSizes {
        Default::default()
    }
    unsafe fn get_acceleration_structure_device_address(
        &self,
        _acceleration_structure: &Resource,
    ) -> wgt::BufferAddress {
        Default::default()
    }
    unsafe fn destroy_acceleration_structure(&self, _acceleration_structure: Resource) {}

    fn tlas_instance_to_bytes(&self, instance: TlasInstance) -> Vec<u8> {
        vec![]
    }

    fn get_internal_counters(&self) -> wgt::HalCounters {
        Default::default()
    }

    fn check_if_oom(&self) -> DeviceResult<()> {
        Ok(())
    }
}
