//! [`Limits`] and downlevel-related types.

use core::cmp::Ordering;

#[cfg(any(feature = "serde", test))]
use serde::{Deserialize, Serialize};

#[cfg(doc)]
use crate::{Features, TextureFormat, VertexFormat};

/// Invoke a macro for each of the limits.
///
/// The supplied macro should take two arguments. The first is a limit name, as
/// an identifier, typically used to access a member of `struct Limits`. The
/// second is `Ordering::Less` if valid values are less than the limit (the
/// common case), or `Ordering::Greater` if valid values are more than the limit
/// (for limits like alignments, which are minima instead of maxima).
macro_rules! with_limits {
    ($macro_name:ident) => {
        $macro_name!(max_texture_dimension_1d, Ordering::Less);
        $macro_name!(max_texture_dimension_2d, Ordering::Less);
        $macro_name!(max_texture_dimension_3d, Ordering::Less);
        $macro_name!(max_texture_array_layers, Ordering::Less);
        $macro_name!(max_bind_groups, Ordering::Less);
        $macro_name!(max_bind_groups_plus_vertex_buffers, Ordering::Less);
        $macro_name!(max_bindings_per_bind_group, Ordering::Less);
        $macro_name!(
            max_dynamic_uniform_buffers_per_pipeline_layout,
            Ordering::Less
        );
        $macro_name!(
            max_dynamic_storage_buffers_per_pipeline_layout,
            Ordering::Less
        );
        $macro_name!(max_sampled_textures_per_shader_stage, Ordering::Less);
        $macro_name!(max_samplers_per_shader_stage, Ordering::Less);
        $macro_name!(max_storage_buffers_per_shader_stage, Ordering::Less);
        $macro_name!(max_storage_textures_per_shader_stage, Ordering::Less);
        $macro_name!(max_uniform_buffers_per_shader_stage, Ordering::Less);
        $macro_name!(max_binding_array_elements_per_shader_stage, Ordering::Less);
        $macro_name!(
            max_binding_array_acceleration_structure_elements_per_shader_stage,
            Ordering::Less
        );
        $macro_name!(
            max_binding_array_sampler_elements_per_shader_stage,
            Ordering::Less
        );

        $macro_name!(max_uniform_buffer_binding_size, Ordering::Less);
        $macro_name!(max_storage_buffer_binding_size, Ordering::Less);
        $macro_name!(max_vertex_buffers, Ordering::Less);
        $macro_name!(max_buffer_size, Ordering::Less);
        $macro_name!(max_vertex_attributes, Ordering::Less);
        $macro_name!(max_vertex_buffer_array_stride, Ordering::Less);
        $macro_name!(max_inter_stage_shader_variables, Ordering::Less);
        $macro_name!(min_uniform_buffer_offset_alignment, Ordering::Greater);
        $macro_name!(min_storage_buffer_offset_alignment, Ordering::Greater);
        $macro_name!(max_color_attachments, Ordering::Less);
        $macro_name!(max_color_attachment_bytes_per_sample, Ordering::Less);
        $macro_name!(max_compute_workgroup_storage_size, Ordering::Less);
        $macro_name!(max_compute_invocations_per_workgroup, Ordering::Less);
        $macro_name!(max_compute_workgroup_size_x, Ordering::Less);
        $macro_name!(max_compute_workgroup_size_y, Ordering::Less);
        $macro_name!(max_compute_workgroup_size_z, Ordering::Less);
        $macro_name!(max_compute_workgroups_per_dimension, Ordering::Less);

        $macro_name!(max_immediate_size, Ordering::Less);
        $macro_name!(max_non_sampler_bindings, Ordering::Less);

        $macro_name!(max_task_workgroup_total_count, Ordering::Less);
        $macro_name!(max_task_workgroups_per_dimension, Ordering::Less);
        $macro_name!(max_mesh_workgroup_total_count, Ordering::Less);
        $macro_name!(max_mesh_workgroups_per_dimension, Ordering::Less);
        $macro_name!(max_task_invocations_per_workgroup, Ordering::Less);
        $macro_name!(max_task_invocations_per_dimension, Ordering::Less);
        $macro_name!(max_mesh_invocations_per_workgroup, Ordering::Less);
        $macro_name!(max_mesh_invocations_per_dimension, Ordering::Less);

        $macro_name!(max_task_payload_size, Ordering::Less);
        $macro_name!(max_mesh_output_vertices, Ordering::Less);
        $macro_name!(max_mesh_output_primitives, Ordering::Less);
        $macro_name!(max_mesh_output_layers, Ordering::Less);
        $macro_name!(max_mesh_multiview_view_count, Ordering::Less);

        $macro_name!(max_blas_primitive_count, Ordering::Less);
        $macro_name!(max_blas_geometry_count, Ordering::Less);
        $macro_name!(max_tlas_instance_count, Ordering::Less);
        $macro_name!(max_acceleration_structures_per_shader_stage, Ordering::Less);
        $macro_name!(
            max_buffers_and_acceleration_structures_per_shader_stage,
            Ordering::Less
        );

        $macro_name!(max_multiview_view_count, Ordering::Less);

        $macro_name!(max_ray_dispatch_count, Ordering::Less);
        $macro_name!(max_ray_recursion_depth, Ordering::Less);
    };
}

/// Represents the sets of limits an adapter/device supports.
///
/// We provide three different defaults.
/// - [`Limits::downlevel_defaults()`]. This is a set of limits that is guaranteed to work on almost
///   all backends, including the "downlevel" OpenGL backend, but excluding WebGL2. For
///   most applications we recommend using these limits, assuming they are high enough for your
///   application, and you do not intend to support WebGL.
/// - [`Limits::downlevel_webgl2_defaults()`] This is a set of limits that is lower even than the
///   [`downlevel_defaults()`], configured to be low enough to support running in the browser using
///   WebGL2.
/// - [`Limits::default()`]. This is the set of limits that is guaranteed to work on all modern
///   backends and is guaranteed to be supported by WebGPU. Applications needing more modern
///   features can use this as a reasonable set of limits if they are targeting only desktop and
///   modern mobile devices.
///
/// We recommend starting with the most restrictive limits you can and manually increasing the
/// limits you need boosted. This will let you stay running on all hardware that supports the limits
/// you need.
///
/// Limits "better" than the default must be supported by the adapter and requested when requesting
/// a device. If limits "better" than the adapter supports are requested, requesting a device will
/// panic. Once a device is requested, you may only use resources up to the limits requested _even_
/// if the adapter supports "better" limits.
///
/// Requesting limits that are "better" than you need may cause performance to decrease because the
/// implementation needs to support more than is needed. You should ideally only request exactly
/// what you need.
///
/// Corresponds to [WebGPU `GPUSupportedLimits`](
/// https://gpuweb.github.io/gpuweb/#gpusupportedlimits).
///
/// [`downlevel_defaults()`]: Limits::downlevel_defaults
#[repr(C)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "camelCase", default))]
pub struct Limits {
    /// Maximum allowed value for the `size.width` of a texture created with `TextureDimension::D1`.
    /// Defaults to 8192. Higher is "better".
    #[cfg_attr(feature = "serde", serde(rename = "maxTextureDimension1D"))]
    pub max_texture_dimension_1d: u32,
    /// Maximum allowed value for the `size.width` and `size.height` of a texture created with `TextureDimension::D2`.
    /// Defaults to 8192. Higher is "better".
    #[cfg_attr(feature = "serde", serde(rename = "maxTextureDimension2D"))]
    pub max_texture_dimension_2d: u32,
    /// Maximum allowed value for the `size.width`, `size.height`, and `size.depth_or_array_layers`
    /// of a texture created with `TextureDimension::D3`.
    /// Defaults to 2048. Higher is "better".
    #[cfg_attr(feature = "serde", serde(rename = "maxTextureDimension3D"))]
    pub max_texture_dimension_3d: u32,
    /// Maximum allowed value for the `size.depth_or_array_layers` of a texture created with `TextureDimension::D2`.
    /// Defaults to 256. Higher is "better".
    pub max_texture_array_layers: u32,
    /// Amount of bind groups that can be attached to a pipeline at the same time. Defaults to 4. Higher is "better".
    pub max_bind_groups: u32,
    /// The maximum number of bind group and vertex buffer slots used simultaneously, counting any empty slots below the highest index.
    /// Defaults to 24. Higher is "better".
    pub max_bind_groups_plus_vertex_buffers: u32,
    /// Maximum binding index allowed in `create_bind_group_layout`. Defaults to 1000. Higher is "better".
    pub max_bindings_per_bind_group: u32,
    /// Amount of uniform buffer bindings that can be dynamic in a single pipeline. Defaults to 8. Higher is "better".
    pub max_dynamic_uniform_buffers_per_pipeline_layout: u32,
    /// Amount of storage buffer bindings that can be dynamic in a single pipeline. Defaults to 4. Higher is "better".
    pub max_dynamic_storage_buffers_per_pipeline_layout: u32,
    /// Amount of sampled textures visible in a single shader stage. Defaults to 16. Higher is "better".
    pub max_sampled_textures_per_shader_stage: u32,
    /// Amount of samplers visible in a single shader stage. Defaults to 16. Higher is "better".
    pub max_samplers_per_shader_stage: u32,
    /// Amount of storage buffers visible in a single shader stage. Defaults to 8. Higher is "better".
    pub max_storage_buffers_per_shader_stage: u32,
    /// Amount of storage textures visible in a single shader stage. Defaults to 4. Higher is "better".
    pub max_storage_textures_per_shader_stage: u32,
    /// Amount of uniform buffers visible in a single shader stage. Defaults to 12. Higher is "better".
    pub max_uniform_buffers_per_shader_stage: u32,
    /// Amount of individual resources within binding arrays that can be accessed in a single shader stage. Applies
    /// to all types of bindings except samplers.
    ///
    /// This "defaults" to 0. However if binding arrays are supported, all devices can support 500,000. Higher is "better".
    pub max_binding_array_elements_per_shader_stage: u32,
    /// Amount of individual acceleration structures within binding arrays that can be accessed in a single shader stage.
    ///
    /// This "defaults" to 0. Higher is "better".
    pub max_binding_array_acceleration_structure_elements_per_shader_stage: u32,
    /// Amount of individual samplers within binding arrays that can be accessed in a single shader stage.
    ///
    /// This "defaults" to 0. However if binding arrays are supported, all devices can support 1,000. Higher is "better".
    pub max_binding_array_sampler_elements_per_shader_stage: u32,
    /// Maximum size in bytes of a binding to a uniform buffer. Defaults to 64 KiB. Higher is "better".
    pub max_uniform_buffer_binding_size: u64,
    /// Maximum size in bytes of a binding to a storage buffer. Defaults to 128 MiB. Higher is "better".
    pub max_storage_buffer_binding_size: u64,
    /// Maximum length of `VertexState::buffers` when creating a `RenderPipeline`.
    /// Defaults to 8. Higher is "better".
    pub max_vertex_buffers: u32,
    /// A limit above which buffer allocations are guaranteed to fail.
    /// Defaults to 256 MiB. Higher is "better".
    ///
    /// Buffer allocations below the maximum buffer size may not succeed depending on available memory,
    /// fragmentation and other factors.
    pub max_buffer_size: u64,
    /// Maximum length of `VertexBufferLayout::attributes`, summed over all `VertexState::buffers`,
    /// when creating a `RenderPipeline`.
    /// Defaults to 16. Higher is "better".
    pub max_vertex_attributes: u32,
    /// Maximum value for `VertexBufferLayout::array_stride` when creating a `RenderPipeline`.
    /// Defaults to 2048. Higher is "better".
    pub max_vertex_buffer_array_stride: u32,
    /// Maximum value for the number of input or output variables for inter-stage communication
    /// (like vertex outputs or fragment inputs) `@location(…)`s (in WGSL parlance)
    /// when creating a `RenderPipeline`.
    /// Defaults to 16. Higher is "better".
    pub max_inter_stage_shader_variables: u32,
    /// Required `BufferBindingType::Uniform` alignment for `BufferBinding::offset`
    /// when creating a `BindGroup`, or for `set_bind_group` `dynamicOffsets`.
    /// Defaults to 256. Lower is "better".
    pub min_uniform_buffer_offset_alignment: u32,
    /// Required `BufferBindingType::Storage` alignment for `BufferBinding::offset`
    /// when creating a `BindGroup`, or for `set_bind_group` `dynamicOffsets`.
    /// Defaults to 256. Lower is "better".
    pub min_storage_buffer_offset_alignment: u32,
    /// The maximum allowed number of color attachments.
    pub max_color_attachments: u32,
    /// The maximum number of bytes necessary to hold one sample (pixel or subpixel) of render
    /// pipeline output data, across all color attachments as described by [`TextureFormat::target_pixel_byte_cost`]
    /// and [`TextureFormat::target_component_alignment`]. Defaults to 32. Higher is "better".
    ///
    /// ⚠️ `Rgba8Unorm`/`Rgba8Snorm`/`Bgra8Unorm`/`Bgra8Snorm` are deceptively 8 bytes per sample. ⚠️
    pub max_color_attachment_bytes_per_sample: u32,
    /// Maximum number of bytes used for workgroup memory in a compute entry point. Defaults to
    /// 16384. Higher is "better".
    pub max_compute_workgroup_storage_size: u32,
    /// Maximum value of the product of the `workgroup_size` dimensions for a compute entry-point.
    /// Defaults to 256. Higher is "better".
    pub max_compute_invocations_per_workgroup: u32,
    /// The maximum value of the `workgroup_size` X dimension for a compute stage `ShaderModule` entry-point.
    /// Defaults to 256. Higher is "better".
    pub max_compute_workgroup_size_x: u32,
    /// The maximum value of the `workgroup_size` Y dimension for a compute stage `ShaderModule` entry-point.
    /// Defaults to 256. Higher is "better".
    pub max_compute_workgroup_size_y: u32,
    /// The maximum value of the `workgroup_size` Z dimension for a compute stage `ShaderModule` entry-point.
    /// Defaults to 64. Higher is "better".
    pub max_compute_workgroup_size_z: u32,
    /// The maximum value for each dimension of a `ComputePass::dispatch_workgroups(x, y, z)` operation.
    /// Defaults to 65535. Higher is "better".
    pub max_compute_workgroups_per_dimension: u32,

    /// Amount of storage available for immediates in bytes. Defaults to 0. Higher is "better".
    /// Requesting more than 0 during device creation requires [`Features::IMMEDIATES`] to be enabled.
    ///
    /// Expect the size to be:
    /// - Vulkan: 128-256 bytes
    /// - DX12: 128 bytes
    /// - Metal: 4096 bytes
    /// - OpenGL doesn't natively support immediates, and are emulated with uniforms,
    ///   so this number is less useful but likely 256.
    pub max_immediate_size: u32,
    /// Maximum number of live non-sampler bindings.
    ///
    /// <div class="warning">
    /// The default value is **1_000_000**, On systems with integrated GPUs (iGPUs)—particularly on Windows using the D3D12
    /// backend—this can lead to significant system RAM consumption since iGPUs share system memory directly with the CPU.
    /// </div>
    ///
    /// This limit only affects the d3d12 backend. Using a large number will allow the device
    /// to create many bind groups at the cost of a large up-front allocation at device creation.
    pub max_non_sampler_bindings: u32,

    /// The maximum total value for a `RenderPass::draw_mesh_tasks(x, y, z)` call on a mesh pipeline with a task shader.
    /// Higher is "better".
    pub max_task_workgroup_total_count: u32,
    /// The maximum value for each dimension of a `RenderPass::draw_mesh_tasks(x, y, z)` call on a mesh pipeline with a task shader.
    /// Higher is "better".
    pub max_task_workgroups_per_dimension: u32,
    /// The maximum product of arguments of a `RenderPass::draw_mesh_tasks(x, y, z)` operation on a mesh shader pipeline
    /// without task shaders.
    /// Also for task shader outputs. Higher is "better".
    pub max_mesh_workgroup_total_count: u32,
    /// The maximum value for each dimension of a `RenderPass::draw_mesh_tasks(x, y, z)` operation on a mesh shader pipeline
    /// without task shaders.
    /// Also for task shader outputs. Higher is "better".
    pub max_mesh_workgroups_per_dimension: u32,
    // These are fundamentally different. It is very common for limits on mesh shaders to be much lower.
    /// Maximum total number of invocations, or threads, per task shader workgroup. Higher is "better".
    pub max_task_invocations_per_workgroup: u32,
    /// The maximum value for each dimension of a task shader's workgroup size. Higher is "better".
    pub max_task_invocations_per_dimension: u32,
    /// Maximum total number of invocations, or threads, per mesh shader workgroup. Higher is "better".
    pub max_mesh_invocations_per_workgroup: u32,
    /// The maximum value for each dimension of a mesh shader's workgroup size. Higher is "better".
    pub max_mesh_invocations_per_dimension: u32,

    /// The maximum size of the payload passed from task to mesh shader. Higher is "better".
    pub max_task_payload_size: u32,
    /// The maximum number of vertices that a mesh shader may output. Higher is "better".
    pub max_mesh_output_vertices: u32,
    /// The maximum number of primitives that a mesh shader may output. Higher is "better".
    pub max_mesh_output_primitives: u32,
    /// The maximum number of layers that can be output from a mesh shader. Higher is "better".
    /// See [#8509](https://github.com/gfx-rs/wgpu/issues/8509).
    pub max_mesh_output_layers: u32,
    /// The maximum number of views that can be used by a mesh shader in multiview rendering.
    /// Higher is "better".
    pub max_mesh_multiview_view_count: u32,

    /// The maximum number of primitive (ex: triangles, aabbs) a BLAS is allowed to have. Requesting
    /// more than 0 during device creation only makes sense if [`Features::EXPERIMENTAL_RAY_QUERY`]
    /// is enabled.
    pub max_blas_primitive_count: u32,
    /// The maximum number of geometry descriptors a BLAS is allowed to have. Requesting
    /// more than 0 during device creation only makes sense if [`Features::EXPERIMENTAL_RAY_QUERY`]
    /// is enabled.
    pub max_blas_geometry_count: u32,
    /// The maximum number of instances a TLAS is allowed to have. Requesting more than 0 during
    /// device creation only makes sense if [`Features::EXPERIMENTAL_RAY_QUERY`]
    /// is enabled.
    pub max_tlas_instance_count: u32,
    /// The maximum number of acceleration structures allowed to be used in a shader stage.
    /// Requesting more than 0 during device creation only makes sense if [`Features::EXPERIMENTAL_RAY_QUERY`]
    /// is enabled.
    pub max_acceleration_structures_per_shader_stage: u32,
    /// The combined number of buffers (storage and uniform), vertex buffers, and acceleration
    /// structures that can be bound in a single shader stage.
    pub max_buffers_and_acceleration_structures_per_shader_stage: u32,

    /// The maximum number of views that can be used in multiview rendering
    pub max_multiview_view_count: u32,

    /// The maximum total number (`x*y*z`) of rays able to be dispatched by a trace rays call in a ray
    /// tracing pass. Requesting more than 0 during device creation only makes sense if [`Features::EXPERIMENTAL_RAY_TRACING_PIPELINES`]
    /// is enabled.
    ///
    /// Currently only affects wgpu-hal
    pub max_ray_dispatch_count: u32,
    /// The maximum number that one can pass into a ray tracing pipeline creation to be the maximum ray
    /// recursion depth. (the maximum of the max ray recursion depth) Requesting more than 0 during device
    /// creation only makes sense if [`Features::EXPERIMENTAL_RAY_TRACING_PIPELINES`] is enabled.
    ///
    /// Currently only affects wgpu-hal
    pub max_ray_recursion_depth: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Limits {
    /// These default limits are guaranteed to work on all modern
    /// backends and guaranteed to be supported by WebGPU
    ///
    /// Those limits are as follows:
    /// ```rust
    /// # use wgpu_types::Limits;
    /// assert_eq!(Limits::defaults(), Limits {
    ///     max_texture_dimension_1d: 8192,
    ///     max_texture_dimension_2d: 8192,
    ///     max_texture_dimension_3d: 2048,
    ///     max_texture_array_layers: 256,
    ///     max_bind_groups: 4,
    ///     max_bind_groups_plus_vertex_buffers: 24,
    ///     max_bindings_per_bind_group: 1000,
    ///     max_dynamic_uniform_buffers_per_pipeline_layout: 8,
    ///     max_dynamic_storage_buffers_per_pipeline_layout: 4,
    ///     max_sampled_textures_per_shader_stage: 16,
    ///     max_samplers_per_shader_stage: 16,
    ///     max_storage_buffers_per_shader_stage: 8,
    ///     max_storage_textures_per_shader_stage: 4,
    ///     max_uniform_buffers_per_shader_stage: 12,
    ///     max_binding_array_elements_per_shader_stage: 0,
    ///     max_binding_array_acceleration_structure_elements_per_shader_stage: 0,
    ///     max_binding_array_sampler_elements_per_shader_stage: 0,
    ///     max_uniform_buffer_binding_size: 64 << 10, // (64 KiB)
    ///     max_storage_buffer_binding_size: 128 << 20, // (128 MiB)
    ///     max_vertex_buffers: 8,
    ///     max_buffer_size: 256 << 20, // (256 MiB)
    ///     max_vertex_attributes: 16,
    ///     max_vertex_buffer_array_stride: 2048,
    ///     max_inter_stage_shader_variables: 16,
    ///     min_uniform_buffer_offset_alignment: 256,
    ///     min_storage_buffer_offset_alignment: 256,
    ///     max_color_attachments: 8,
    ///     max_color_attachment_bytes_per_sample: 32,
    ///     max_compute_workgroup_storage_size: 16384,
    ///     max_compute_invocations_per_workgroup: 256,
    ///     max_compute_workgroup_size_x: 256,
    ///     max_compute_workgroup_size_y: 256,
    ///     max_compute_workgroup_size_z: 64,
    ///     max_compute_workgroups_per_dimension: 65535,
    ///     max_immediate_size: 0,
    ///     max_non_sampler_bindings: 1_000_000,
    ///     max_task_workgroup_total_count: 0,
    ///     max_task_workgroups_per_dimension: 0,
    ///     max_mesh_workgroup_total_count: 0,
    ///     max_mesh_workgroups_per_dimension: 0,
    ///     max_task_invocations_per_workgroup: 0,
    ///     max_task_invocations_per_dimension: 0,
    ///     max_mesh_invocations_per_workgroup: 0,
    ///     max_mesh_invocations_per_dimension: 0,
    ///     max_task_payload_size: 0,
    ///     max_mesh_output_vertices: 0,
    ///     max_mesh_output_primitives: 0,
    ///     max_mesh_output_layers: 0,
    ///     max_mesh_multiview_view_count: 0,
    ///     max_blas_primitive_count: 0,
    ///     max_blas_geometry_count: 0,
    ///     max_tlas_instance_count: 0,
    ///     max_acceleration_structures_per_shader_stage: 0,
    ///     max_buffers_and_acceleration_structures_per_shader_stage: 28, // sum of storage buffers, uniform buffers and vertex buffers limits
    ///     max_multiview_view_count: 0,
    ///     max_ray_dispatch_count: 0,
    ///     max_ray_recursion_depth: 0,
    /// });
    /// ```
    ///
    /// Rust doesn't allow const in trait implementations, so we break this out
    /// to allow reusing these defaults in const contexts
    #[must_use]
    pub const fn defaults() -> Self {
        Self {
            max_texture_dimension_1d: 8192,
            max_texture_dimension_2d: 8192,
            max_texture_dimension_3d: 2048,
            max_texture_array_layers: 256,
            max_bind_groups: 4,
            max_bind_groups_plus_vertex_buffers: 24,
            max_bindings_per_bind_group: 1000,
            max_dynamic_uniform_buffers_per_pipeline_layout: 8,
            max_dynamic_storage_buffers_per_pipeline_layout: 4,
            max_sampled_textures_per_shader_stage: 16,
            max_samplers_per_shader_stage: 16,
            max_storage_buffers_per_shader_stage: 8,
            max_storage_textures_per_shader_stage: 4,
            max_uniform_buffers_per_shader_stage: 12,
            max_binding_array_elements_per_shader_stage: 0,
            max_binding_array_acceleration_structure_elements_per_shader_stage: 0,
            max_binding_array_sampler_elements_per_shader_stage: 0,
            max_uniform_buffer_binding_size: 64 << 10, // (64 KiB)
            max_storage_buffer_binding_size: 128 << 20, // (128 MiB)
            max_vertex_buffers: 8,
            max_buffer_size: 256 << 20, // (256 MiB)
            max_vertex_attributes: 16,
            max_vertex_buffer_array_stride: 2048,
            max_inter_stage_shader_variables: 16,
            min_uniform_buffer_offset_alignment: 256,
            min_storage_buffer_offset_alignment: 256,
            max_color_attachments: 8,
            max_color_attachment_bytes_per_sample: 32,
            max_compute_workgroup_storage_size: 16384,
            max_compute_invocations_per_workgroup: 256,
            max_compute_workgroup_size_x: 256,
            max_compute_workgroup_size_y: 256,
            max_compute_workgroup_size_z: 64,
            max_compute_workgroups_per_dimension: 65535,
            max_immediate_size: 0,
            max_non_sampler_bindings: 1_000_000,

            max_task_workgroup_total_count: 0,
            max_task_workgroups_per_dimension: 0,
            max_mesh_workgroup_total_count: 0,
            max_mesh_workgroups_per_dimension: 0,
            max_task_invocations_per_workgroup: 0,
            max_task_invocations_per_dimension: 0,
            max_mesh_invocations_per_workgroup: 0,
            max_mesh_invocations_per_dimension: 0,
            max_task_payload_size: 0,
            max_mesh_output_vertices: 0,
            max_mesh_output_primitives: 0,
            max_mesh_output_layers: 0,
            max_mesh_multiview_view_count: 0,

            max_blas_primitive_count: 0,
            max_blas_geometry_count: 0,
            max_tlas_instance_count: 0,
            max_acceleration_structures_per_shader_stage: 0,
            max_buffers_and_acceleration_structures_per_shader_stage: 28,

            max_multiview_view_count: 0,

            max_ray_dispatch_count: 0,
            max_ray_recursion_depth: 0,
        }
    }

    /// These default limits are guaranteed to be compatible with GLES-3.1.
    ///
    /// Those limits are as follows (different from default are marked with *):
    /// ```rust
    /// # use wgpu_types::Limits;
    /// assert_eq!(Limits::downlevel_defaults(), Limits {
    ///     max_texture_dimension_1d: 2048, // *
    ///     max_texture_dimension_2d: 2048, // *
    ///     max_texture_dimension_3d: 256, // *
    ///     max_texture_array_layers: 256,
    ///     max_bind_groups: 4,
    ///     max_bind_groups_plus_vertex_buffers: 24,
    ///     max_bindings_per_bind_group: 1000,
    ///     max_dynamic_uniform_buffers_per_pipeline_layout: 8,
    ///     max_dynamic_storage_buffers_per_pipeline_layout: 4,
    ///     max_sampled_textures_per_shader_stage: 16,
    ///     max_samplers_per_shader_stage: 16,
    ///     max_storage_buffers_per_shader_stage: 4, // *
    ///     max_storage_textures_per_shader_stage: 4,
    ///     max_uniform_buffers_per_shader_stage: 12,
    ///     max_binding_array_elements_per_shader_stage: 0,
    ///     max_binding_array_acceleration_structure_elements_per_shader_stage: 0,
    ///     max_binding_array_sampler_elements_per_shader_stage: 0,
    ///     max_uniform_buffer_binding_size: 16 << 10, // * (16 KiB)
    ///     max_storage_buffer_binding_size: 128 << 20, // (128 MiB)
    ///     max_vertex_buffers: 8,
    ///     max_vertex_attributes: 16,
    ///     max_vertex_buffer_array_stride: 2048,
    ///     max_immediate_size: 0,
    ///     min_uniform_buffer_offset_alignment: 256,
    ///     min_storage_buffer_offset_alignment: 256,
    ///     max_inter_stage_shader_variables: 15,
    ///     max_color_attachments: 4,
    ///     max_color_attachment_bytes_per_sample: 32,
    ///     max_compute_workgroup_storage_size: 16352, // *
    ///     max_compute_invocations_per_workgroup: 256,
    ///     max_compute_workgroup_size_x: 256,
    ///     max_compute_workgroup_size_y: 256,
    ///     max_compute_workgroup_size_z: 64,
    ///     max_compute_workgroups_per_dimension: 65535,
    ///     max_buffer_size: 256 << 20, // (256 MiB)
    ///     max_non_sampler_bindings: 1_000_000,
    ///
    ///     max_task_workgroup_total_count: 0,
    ///     max_task_workgroups_per_dimension: 0,
    ///     max_mesh_workgroup_total_count: 0,
    ///     max_mesh_workgroups_per_dimension: 0,
    ///     max_task_invocations_per_workgroup: 0,
    ///     max_task_invocations_per_dimension: 0,
    ///     max_mesh_invocations_per_workgroup: 0,
    ///     max_mesh_invocations_per_dimension: 0,
    ///     max_task_payload_size: 0,
    ///     max_mesh_output_vertices: 0,
    ///     max_mesh_output_primitives: 0,
    ///     max_mesh_output_layers: 0,
    ///     max_mesh_multiview_view_count: 0,
    ///
    ///     max_blas_primitive_count: 0,
    ///     max_blas_geometry_count: 0,
    ///     max_tlas_instance_count: 0,
    ///     max_acceleration_structures_per_shader_stage: 0,
    ///     max_buffers_and_acceleration_structures_per_shader_stage: 24, // * sum of storage buffers, uniform buffers and vertex buffers limits
    ///
    ///     max_multiview_view_count: 0,
    ///
    ///     max_ray_dispatch_count: 0,
    ///     max_ray_recursion_depth: 0,
    /// });
    /// ```
    #[must_use]
    pub const fn downlevel_defaults() -> Self {
        Self {
            max_texture_dimension_1d: 2048,
            max_texture_dimension_2d: 2048,
            max_texture_dimension_3d: 256,
            max_storage_buffers_per_shader_stage: 4,
            max_uniform_buffer_binding_size: 16 << 10, // (16 KiB)
            max_inter_stage_shader_variables: 15,
            max_color_attachments: 4,
            // see: https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf#page=7
            max_compute_workgroup_storage_size: 16352,
            max_buffers_and_acceleration_structures_per_shader_stage: 24,
            ..Self::defaults()
        }
    }

    /// These default limits are guaranteed to be compatible with GLES-3.0 and WebGL2
    ///
    /// Those limits are as follows (different from `downlevel_defaults` are marked with +,
    /// *'s from `downlevel_defaults` shown as well.):
    /// ```rust
    /// # use wgpu_types::Limits;
    /// assert_eq!(Limits::downlevel_webgl2_defaults(), Limits {
    ///     max_texture_dimension_1d: 2048, // *
    ///     max_texture_dimension_2d: 2048, // *
    ///     max_texture_dimension_3d: 256, // *
    ///     max_texture_array_layers: 256,
    ///     max_bind_groups: 4,
    ///     max_bind_groups_plus_vertex_buffers: 24,
    ///     max_bindings_per_bind_group: 1000,
    ///     max_dynamic_uniform_buffers_per_pipeline_layout: 8,
    ///     max_dynamic_storage_buffers_per_pipeline_layout: 0, // +
    ///     max_sampled_textures_per_shader_stage: 16,
    ///     max_samplers_per_shader_stage: 16,
    ///     max_storage_buffers_per_shader_stage: 0, // * +
    ///     max_storage_textures_per_shader_stage: 0, // +
    ///     max_uniform_buffers_per_shader_stage: 11, // +
    ///     max_binding_array_elements_per_shader_stage: 0,
    ///     max_binding_array_acceleration_structure_elements_per_shader_stage: 0,
    ///     max_binding_array_sampler_elements_per_shader_stage: 0,
    ///     max_uniform_buffer_binding_size: 16 << 10, // * (16 KiB)
    ///     max_storage_buffer_binding_size: 0, // * +
    ///     max_vertex_buffers: 8,
    ///     max_vertex_attributes: 16,
    ///     max_vertex_buffer_array_stride: 255, // +
    ///     max_immediate_size: 0,
    ///     min_uniform_buffer_offset_alignment: 256,
    ///     min_storage_buffer_offset_alignment: 256,
    ///     max_inter_stage_shader_variables: 15,
    ///     max_color_attachments: 4,
    ///     max_color_attachment_bytes_per_sample: 32,
    ///     max_compute_workgroup_storage_size: 0, // +
    ///     max_compute_invocations_per_workgroup: 0, // +
    ///     max_compute_workgroup_size_x: 0, // +
    ///     max_compute_workgroup_size_y: 0, // +
    ///     max_compute_workgroup_size_z: 0, // +
    ///     max_compute_workgroups_per_dimension: 0, // +
    ///     max_buffer_size: 256 << 20, // (256 MiB),
    ///     max_non_sampler_bindings: 1_000_000,
    ///
    ///     max_task_workgroup_total_count: 0,
    ///     max_task_workgroups_per_dimension: 0,
    ///     max_mesh_workgroup_total_count: 0,
    ///     max_mesh_workgroups_per_dimension: 0,
    ///     max_task_invocations_per_workgroup: 0,
    ///     max_task_invocations_per_dimension: 0,
    ///     max_mesh_invocations_per_workgroup: 0,
    ///     max_mesh_invocations_per_dimension: 0,
    ///     max_task_payload_size: 0,
    ///     max_mesh_output_vertices: 0,
    ///     max_mesh_output_primitives: 0,
    ///     max_mesh_output_layers: 0,
    ///     max_mesh_multiview_view_count: 0,
    ///
    ///     max_blas_primitive_count: 0,
    ///     max_blas_geometry_count: 0,
    ///     max_tlas_instance_count: 0,
    ///     max_acceleration_structures_per_shader_stage: 0,
    ///     max_buffers_and_acceleration_structures_per_shader_stage: 19, // * sum of storage buffers, uniform buffers and vertex buffers limits
    ///
    ///     max_multiview_view_count: 0,
    ///
    ///     max_ray_dispatch_count: 0,
    ///     max_ray_recursion_depth: 0,
    /// });
    /// ```
    #[must_use]
    pub const fn downlevel_webgl2_defaults() -> Self {
        Self {
            max_uniform_buffers_per_shader_stage: 11,
            max_storage_buffers_per_shader_stage: 0,
            max_storage_textures_per_shader_stage: 0,
            max_dynamic_storage_buffers_per_pipeline_layout: 0,
            max_storage_buffer_binding_size: 0,
            max_vertex_buffer_array_stride: 255,
            max_compute_workgroup_storage_size: 0,
            max_compute_invocations_per_workgroup: 0,
            max_compute_workgroup_size_x: 0,
            max_compute_workgroup_size_y: 0,
            max_compute_workgroup_size_z: 0,
            max_compute_workgroups_per_dimension: 0,

            // Value supported by Intel Celeron B830 on Windows (OpenGL 3.1)
            max_inter_stage_shader_variables: 15,

            max_buffers_and_acceleration_structures_per_shader_stage: 19,

            // Most of the values should be the same as the downlevel defaults
            ..Self::downlevel_defaults()
        }
    }

    /// Sets each limit to `i32::MAX` (or 1, in the case of lower-is-better limits).
    ///
    /// These values do not reflect the capabilities of any actual device. They are
    /// used by the noop backend, and by the test that makes sure `with_limits!` is
    /// exhaustive.
    #[must_use]
    pub const fn unlimited() -> Self {
        /// Guaranteed to be no bigger than isize::MAX which is the maximum size of an allocation,
        /// except on 16-bit platforms which we certainly don’t fit in.
        const ALLOC_MAX_U32: u32 = i32::MAX as u32;
        /// Guaranteed to be no bigger than isize::MAX which is the maximum size of an allocation,
        /// except on 16-bit platforms which we certainly don’t fit in.
        const ALLOC_MAX_U64: u64 = i32::MAX as u64;

        Self {
            max_texture_dimension_1d: ALLOC_MAX_U32,
            max_texture_dimension_2d: ALLOC_MAX_U32,
            max_texture_dimension_3d: ALLOC_MAX_U32,
            max_texture_array_layers: ALLOC_MAX_U32,
            max_bind_groups: ALLOC_MAX_U32,
            max_bind_groups_plus_vertex_buffers: ALLOC_MAX_U32,
            max_bindings_per_bind_group: ALLOC_MAX_U32,
            max_dynamic_uniform_buffers_per_pipeline_layout: ALLOC_MAX_U32,
            max_dynamic_storage_buffers_per_pipeline_layout: ALLOC_MAX_U32,
            max_sampled_textures_per_shader_stage: ALLOC_MAX_U32,
            max_samplers_per_shader_stage: ALLOC_MAX_U32,
            max_storage_buffers_per_shader_stage: ALLOC_MAX_U32,
            max_storage_textures_per_shader_stage: ALLOC_MAX_U32,
            max_uniform_buffers_per_shader_stage: ALLOC_MAX_U32,
            max_binding_array_elements_per_shader_stage: ALLOC_MAX_U32,
            max_binding_array_sampler_elements_per_shader_stage: ALLOC_MAX_U32,
            max_binding_array_acceleration_structure_elements_per_shader_stage: ALLOC_MAX_U32,
            max_uniform_buffer_binding_size: ALLOC_MAX_U64,
            max_storage_buffer_binding_size: ALLOC_MAX_U64,
            max_vertex_buffers: ALLOC_MAX_U32,
            max_buffer_size: ALLOC_MAX_U64,
            max_vertex_attributes: ALLOC_MAX_U32,
            max_vertex_buffer_array_stride: ALLOC_MAX_U32,
            max_inter_stage_shader_variables: ALLOC_MAX_U32,
            min_uniform_buffer_offset_alignment: 1,
            min_storage_buffer_offset_alignment: 1,
            max_color_attachments: ALLOC_MAX_U32,
            max_color_attachment_bytes_per_sample: ALLOC_MAX_U32,
            max_compute_workgroup_storage_size: ALLOC_MAX_U32,
            max_compute_invocations_per_workgroup: ALLOC_MAX_U32,
            max_compute_workgroup_size_x: ALLOC_MAX_U32,
            max_compute_workgroup_size_y: ALLOC_MAX_U32,
            max_compute_workgroup_size_z: ALLOC_MAX_U32,
            max_compute_workgroups_per_dimension: ALLOC_MAX_U32,
            max_immediate_size: ALLOC_MAX_U32,
            max_non_sampler_bindings: ALLOC_MAX_U32,

            max_task_workgroup_total_count: ALLOC_MAX_U32,
            max_task_workgroups_per_dimension: ALLOC_MAX_U32,
            max_mesh_workgroup_total_count: ALLOC_MAX_U32,
            max_mesh_workgroups_per_dimension: ALLOC_MAX_U32,
            max_task_invocations_per_workgroup: ALLOC_MAX_U32,
            max_task_invocations_per_dimension: ALLOC_MAX_U32,
            max_mesh_invocations_per_workgroup: ALLOC_MAX_U32,
            max_mesh_invocations_per_dimension: ALLOC_MAX_U32,
            max_task_payload_size: ALLOC_MAX_U32,
            max_mesh_output_vertices: ALLOC_MAX_U32,
            max_mesh_output_primitives: ALLOC_MAX_U32,
            max_mesh_output_layers: ALLOC_MAX_U32,
            max_mesh_multiview_view_count: ALLOC_MAX_U32,

            max_blas_primitive_count: ALLOC_MAX_U32,
            max_blas_geometry_count: ALLOC_MAX_U32,
            max_tlas_instance_count: ALLOC_MAX_U32,
            max_acceleration_structures_per_shader_stage: ALLOC_MAX_U32,
            max_buffers_and_acceleration_structures_per_shader_stage: ALLOC_MAX_U32,

            max_multiview_view_count: ALLOC_MAX_U32,
            max_ray_dispatch_count: ALLOC_MAX_U32,
            max_ray_recursion_depth: ALLOC_MAX_U32,
        }
    }

    /// Modify the current limits to use the resolution limits of the other.
    ///
    /// This is useful because the swapchain might need to be larger than any other image in the application.
    ///
    /// If your application only needs 512x512, you might be running on a 4k display and need extremely high resolution limits.
    #[must_use]
    pub const fn using_resolution(self, other: Self) -> Self {
        Self {
            max_texture_dimension_1d: other.max_texture_dimension_1d,
            max_texture_dimension_2d: other.max_texture_dimension_2d,
            max_texture_dimension_3d: other.max_texture_dimension_3d,
            ..self
        }
    }

    /// Modify the current limits to use the buffer alignment limits of the adapter.
    ///
    /// This is useful for when you'd like to dynamically use the "best" supported buffer alignments.
    #[must_use]
    pub const fn using_alignment(self, other: Self) -> Self {
        Self {
            min_uniform_buffer_offset_alignment: other.min_uniform_buffer_offset_alignment,
            min_storage_buffer_offset_alignment: other.min_storage_buffer_offset_alignment,
            ..self
        }
    }

    /// The minimum guaranteed limits for acceleration structures if you enable [`Features::EXPERIMENTAL_RAY_QUERY`]
    #[must_use]
    pub const fn using_minimum_supported_acceleration_structure_values(self) -> Self {
        Self {
            max_blas_geometry_count: (1 << 24) - 1, // 2^24 - 1: Vulkan's minimum
            max_tlas_instance_count: (1 << 24) - 1, // 2^24 - 1: Vulkan's minimum
            max_blas_primitive_count: 1 << 28,      // 2^28: Metal's minimum
            // On metal acceleration structures are limited because they share buffer slots
            max_acceleration_structures_per_shader_stage: 1,
            max_buffers_and_acceleration_structures_per_shader_stage: 29,
            ..self
        }
    }

    /// Modify the current limits to use the acceleration structure limits of `other` (`other` could
    /// be the limits of the adapter).
    #[must_use]
    pub const fn using_acceleration_structure_values(self, other: Self) -> Self {
        Self {
            max_blas_geometry_count: other.max_blas_geometry_count,
            max_tlas_instance_count: other.max_tlas_instance_count,
            max_blas_primitive_count: other.max_blas_primitive_count,
            max_acceleration_structures_per_shader_stage: other
                .max_acceleration_structures_per_shader_stage,
            max_buffers_and_acceleration_structures_per_shader_stage: other
                .max_buffers_and_acceleration_structures_per_shader_stage,
            ..self
        }
    }

    /// The minimum guaranteed limits for acceleration structures if you enable [`Features::EXPERIMENTAL_RAY_TRACING_PIPELINES`]
    /// These may change in the future (including downwards).
    #[must_use]
    pub const fn using_minimum_supported_ray_tracing_pipeline_values(self) -> Self {
        Self {
            max_ray_dispatch_count: 1 << 30,
            max_ray_recursion_depth: 1,
            ..self
        }
    }

    /// The recommended minimum limits for mesh shaders if you enable [`Features::EXPERIMENTAL_MESH_SHADER`]
    ///
    /// These are chosen somewhat arbitrarily. They are small enough that they should cover all physical devices,
    /// but not necessarily all use cases.
    #[must_use]
    pub const fn using_recommended_minimum_mesh_shader_values(self) -> Self {
        Self {
            // These are DirectX limitations (both nvidia and AMD match these exactly on vulkan)
            // Note that Mac2 (newest intel macs) support up to 1024, but this is low enough,
            // to make use of mesh shaders nonviable in most cases.
            // We therefore, don't expose mesh shading on these devices.
            // In contrast, here is no limit for any A-series or M-series chip.
            max_task_workgroup_total_count: 2u32.pow(22),
            max_task_workgroups_per_dimension: 65535,
            // These are metal limitations
            // M3 ups both of these to 1M
            max_mesh_workgroup_total_count: 1024,
            max_mesh_workgroups_per_dimension: 1024,
            // Nvidia limit on vulkan
            max_task_invocations_per_workgroup: 128,
            max_task_invocations_per_dimension: 64,

            // DX12 limitation, revisit for vulkan
            max_mesh_invocations_per_workgroup: 128,
            max_mesh_invocations_per_dimension: 128,

            // Metal specifies this as its max
            max_task_payload_size: 16384 - 32,
            // DX12 limitation, revisit for vulkan
            max_mesh_output_vertices: 256,
            max_mesh_output_primitives: 256,
            // llvmpipe once again requires this to be 8. An RTX 3060 supports well over 1024.
            // Also DX12 vaguely suggests going over this is illegal in some cases.
            max_mesh_output_layers: 8,
            // llvmpipe reports 0 multiview count, which just means no multiview is allowed
            max_mesh_multiview_view_count: 0,
            ..self
        }
    }

    /// Compares every limits within self is within the limits given in `allowed`.
    ///
    /// If you need detailed information on failures, look at [`Limits::check_limits_with_fail_fn`].
    #[must_use]
    pub fn check_limits(&self, allowed: &Self) -> bool {
        let mut within = true;
        self.check_limits_with_fail_fn(allowed, true, |_, _, _| within = false);
        within
    }

    /// Compares every limits within self is within the limits given in `allowed`.
    /// For an easy to use binary choice, use [`Limits::check_limits`].
    ///
    /// If a value is not within the allowed limit, this function calls the `fail_fn`
    /// with the:
    ///  - limit name
    ///  - self's limit
    ///  - allowed's limit.
    ///
    /// If fatal is true, a single failure bails out the comparison after a single failure.
    pub fn check_limits_with_fail_fn(
        &self,
        allowed: &Self,
        fatal: bool,
        mut fail_fn: impl FnMut(&'static str, u64, u64),
    ) {
        macro_rules! check_with_fail_fn {
            ($name:ident, $ordering:expr) => {
                let invalid_ord = $ordering.reverse();
                if self.$name.cmp(&allowed.$name) == invalid_ord {
                    fail_fn(stringify!($name), self.$name as u64, allowed.$name as u64);
                    if fatal {
                        return;
                    }
                }
            };
        }

        with_limits!(check_with_fail_fn);
    }

    /// For each limit in `other` that is better than the value in `self`,
    /// replace the value in `self` with the value from `other`.
    ///
    /// A request for a limit value less than the WebGPU-specified default must
    /// be ignored. This function is used to clamp such requests to the default
    /// value.
    ///
    /// This function is not for clamping requests for values beyond the
    /// supported limits. For that purpose the desired function would be
    /// `or_worse_values_from`.
    #[must_use]
    pub fn or_better_values_from(mut self, other: &Self) -> Self {
        macro_rules! or_better_value_from {
            ($name:ident, $ordering:expr) => {
                match $ordering {
                    // Limits that are maximum values (most of them)
                    Ordering::Less => self.$name = self.$name.max(other.$name),
                    // Limits that are minimum values
                    Ordering::Greater => self.$name = self.$name.min(other.$name),
                    Ordering::Equal => unreachable!(),
                }
            };
        }

        with_limits!(or_better_value_from);

        self
    }

    /// For each limit in `other` that is worse than the value in `self`,
    /// replace the value in `self` with the value from `other`.
    ///
    /// This function is for clamping requests for values beyond the
    /// supported limits.
    #[must_use]
    pub fn or_worse_values_from(mut self, other: &Self) -> Self {
        macro_rules! or_worse_value_from {
            ($name:ident, $ordering:expr) => {
                match $ordering {
                    // Limits that are maximum values (most of them)
                    Ordering::Less => self.$name = self.$name.min(other.$name),
                    // Limits that are minimum values
                    Ordering::Greater => self.$name = self.$name.max(other.$name),
                    Ordering::Equal => unreachable!(),
                }
            };
        }

        with_limits!(or_worse_value_from);

        self
    }

    /// Sets all native-only limits to zero, except for `max_non_sampler_bindings`.
    pub fn zero_native_only(&mut self) {
        let Self {
            max_texture_dimension_1d: _,
            max_texture_dimension_2d: _,
            max_texture_dimension_3d: _,
            max_texture_array_layers: _,
            max_bind_groups: _,
            max_bind_groups_plus_vertex_buffers: _,
            max_bindings_per_bind_group: _,
            max_dynamic_uniform_buffers_per_pipeline_layout: _,
            max_dynamic_storage_buffers_per_pipeline_layout: _,
            max_sampled_textures_per_shader_stage: _,
            max_samplers_per_shader_stage: _,
            max_storage_buffers_per_shader_stage: _,
            max_storage_textures_per_shader_stage: _,
            max_uniform_buffers_per_shader_stage: _,
            max_uniform_buffer_binding_size: _,
            max_storage_buffer_binding_size: _,
            max_vertex_buffers: _,
            max_buffer_size: _,
            max_vertex_attributes: _,
            max_vertex_buffer_array_stride: _,
            max_inter_stage_shader_variables: _,
            min_uniform_buffer_offset_alignment: _,
            min_storage_buffer_offset_alignment: _,
            max_color_attachments: _,
            max_color_attachment_bytes_per_sample: _,
            max_compute_workgroup_storage_size: _,
            max_compute_invocations_per_workgroup: _,
            max_compute_workgroup_size_x: _,
            max_compute_workgroup_size_y: _,
            max_compute_workgroup_size_z: _,
            max_compute_workgroups_per_dimension: _,
            max_immediate_size: _,
            max_non_sampler_bindings: _, // This is more of an internal setting rather than a limit and it can't be 0.

            max_binding_array_elements_per_shader_stage,
            max_binding_array_acceleration_structure_elements_per_shader_stage,
            max_binding_array_sampler_elements_per_shader_stage,
            max_task_workgroup_total_count,
            max_task_workgroups_per_dimension,
            max_mesh_workgroup_total_count,
            max_mesh_workgroups_per_dimension,
            max_task_invocations_per_workgroup,
            max_task_invocations_per_dimension,
            max_mesh_invocations_per_workgroup,
            max_mesh_invocations_per_dimension,
            max_task_payload_size,
            max_mesh_output_vertices,
            max_mesh_output_primitives,
            max_mesh_output_layers,
            max_mesh_multiview_view_count,
            max_blas_primitive_count,
            max_blas_geometry_count,
            max_tlas_instance_count,
            max_acceleration_structures_per_shader_stage,
            max_buffers_and_acceleration_structures_per_shader_stage,
            max_multiview_view_count,
            max_ray_dispatch_count,
            max_ray_recursion_depth,
        } = self;
        *max_binding_array_elements_per_shader_stage = 0;
        *max_binding_array_acceleration_structure_elements_per_shader_stage = 0;
        *max_binding_array_sampler_elements_per_shader_stage = 0;
        *max_task_workgroup_total_count = 0;
        *max_task_workgroups_per_dimension = 0;
        *max_mesh_workgroup_total_count = 0;
        *max_mesh_workgroups_per_dimension = 0;
        *max_task_invocations_per_workgroup = 0;
        *max_task_invocations_per_dimension = 0;
        *max_mesh_invocations_per_workgroup = 0;
        *max_mesh_invocations_per_dimension = 0;
        *max_task_payload_size = 0;
        *max_mesh_output_vertices = 0;
        *max_mesh_output_primitives = 0;
        *max_mesh_output_layers = 0;
        *max_mesh_multiview_view_count = 0;
        *max_blas_primitive_count = 0;
        *max_blas_geometry_count = 0;
        *max_tlas_instance_count = 0;
        *max_acceleration_structures_per_shader_stage = 0;
        *max_buffers_and_acceleration_structures_per_shader_stage = 0;
        *max_multiview_view_count = 0;
        *max_ray_dispatch_count = 0;
        *max_ray_recursion_depth = 0;
    }
}

/// Represents the sets of additional limits on an adapter,
/// which take place when running on downlevel backends.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DownlevelLimits {
    /// Whether bind-group layout entries may have no shader-stage visibility.
    ///
    /// This can be unavailable when a backend's resource-layout representation requires
    /// every entry to be visible to at least one supported shader stage.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub empty_binding_visibility: bool,
    /// Whether textures with [`TextureDimension::D1`] can be created.
    ///
    /// This can be unavailable even when `max_texture_dimension_1d` is non-zero
    /// if the backend cannot implement the texture shape.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_1d: bool,
    /// Whether textures with [`TextureDimension::D3`] can be created.
    ///
    /// This can be unavailable even when `max_texture_dimension_3d` is non-zero
    /// if the backend cannot implement the texture shape.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_3d: bool,
    /// Texture usages allowed for 3D textures on this adapter.
    #[cfg_attr(feature = "serde", serde(default = "default_texture_3d_usage_mask"))]
    pub texture_3d_usage_mask: u32,
    /// Formats supported for 3D textures, represented by [`texture_3d_format_bit`].
    #[cfg_attr(feature = "serde", serde(default = "default_texture_3d_format_mask"))]
    pub texture_3d_format_mask: u32,
    /// Whether 3D texture mip chains are supported.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_3d_mipmaps: bool,
    /// Whether 3D texture views may omit the resource's lowest mip level.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_3d_non_suffix_mip_views: bool,
    /// Texture usages allowed for multisampled textures.
    #[cfg_attr(
        feature = "serde",
        serde(default = "default_multisampled_texture_usage_mask")
    )]
    pub multisampled_texture_usage_mask: u32,
    /// Formats that may be used as the source of a texture-to-buffer copy.
    ///
    /// Some downlevel backends can copy a texture between GPU resources but cannot safely stage
    /// that format for CPU-visible buffer readback. The bit assignments are defined by
    /// [`texture_to_buffer_copy_format_bit`].
    #[cfg_attr(
        feature = "serde",
        serde(default = "default_texture_to_buffer_copy_format_mask")
    )]
    pub texture_to_buffer_copy_format_mask: u32,
    /// Whether copies directly between 3D textures are supported.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_3d_texture_copies: bool,
    /// Whether non-power-of-two 3D textures are supported.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub npot_texture_3d: bool,
    /// Maximum width or height of a cube texture view.
    ///
    /// This can be lower than `Limits::max_texture_dimension_2d` when the
    /// backend has a separate hardware limit for cube textures.
    #[cfg_attr(
        feature = "serde",
        serde(default = "default_max_texture_cube_dimension")
    )]
    pub max_texture_cube_dimension: u32,
    /// Sampled formats for which cube textures are supported.
    ///
    /// The bit assignments are defined by [`texture_cube_format_bit`].
    #[cfg_attr(feature = "serde", serde(default = "default_texture_cube_format_mask"))]
    pub texture_cube_format_mask: u32,
    /// Whether non-power-of-two cube textures may be created and sampled.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub npot_cube_textures: bool,
    /// Whether depth sample types are supported by sampled texture bindings.
    ///
    /// This can be unavailable even when the backend supports depth attachments.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub sampled_depth_textures: bool,
    /// Whether signed-integer sample types are supported by sampled texture bindings.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub sampled_sint_textures: bool,
    /// Whether unsigned-integer sample types are supported by sampled texture bindings.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub sampled_uint_textures: bool,
    /// Whether multisampled textures may be declared in sampled texture bindings.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub multisampled_texture_bindings: bool,
    /// Whether render pipelines may enable alpha-to-coverage when multisampled.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub alpha_to_coverage: bool,
    /// Whether vertex and fragment shader entry points may contain loops.
    ///
    /// A backend may still reject loop forms that its selected shader profiles cannot preserve.
    /// In particular, this does not guarantee that dynamically bounded loops are supported.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub shader_loops: bool,
    /// Whether vertex entry points may declare `@builtin(vertex_index)`.
    ///
    /// This can be unavailable when the selected shader profile cannot compile the input.
    pub vertex_index_input: bool,
    /// Whether vertex entry points may declare `@builtin(instance_index)`.
    ///
    /// This can be unavailable when the backend cannot preserve the input's semantics.
    pub instance_index_input: bool,
    /// Maximum number of sampled texture bindings statically used by a vertex shader.
    ///
    /// This can be lower than [`Limits::max_sampled_textures_per_shader_stage`], which also
    /// applies to vertex shaders. WebGPU's baseline guarantee is 16.
    pub max_sampled_textures_per_vertex_stage: u32,
    /// Maximum number of distinct sampled texture/sampler pairs statically used by one shader
    /// entry point.
    ///
    /// This can be lower than either the sampled texture or sampler binding limit when a shader
    /// profile has a separate limit on texture sampling relationships.
    #[cfg_attr(
        feature = "serde",
        serde(default = "default_max_sampled_texture_sampler_pairs")
    )]
    pub max_sampled_texture_sampler_pairs_per_shader_stage: u32,
    /// Maximum aggregate number of 16-byte uniform vectors available to a vertex entry point.
    ///
    /// Downlevel adapters may conservatively count the complete layout of each statically used
    /// uniform binding. Bindings unused by the selected entry point do not consume this limit.
    #[cfg_attr(feature = "serde", serde(default = "default_max_uniform_vectors"))]
    pub max_vertex_shader_uniform_vectors: u32,
    /// Maximum aggregate number of 16-byte uniform vectors available to a fragment entry point.
    ///
    /// Downlevel adapters may conservatively count the complete layout of each statically used
    /// uniform binding. Bindings unused by the selected entry point do not consume this limit.
    #[cfg_attr(feature = "serde", serde(default = "default_max_uniform_vectors"))]
    pub max_fragment_shader_uniform_vectors: u32,
    /// Whether non-power-of-two textures may use repeat addressing when sampled.
    ///
    /// This is false on APIs that expose conditional NPOT support and whose driver does not
    /// report unrestricted NPOT sampling. The restriction applies only to texture/sampler pairs
    /// used by the selected shader entry point.
    pub unrestricted_npot_texture_sampling: bool,
    /// Maximum number of times texture coordinates may repeat over one primitive.
    ///
    /// `u32::MAX` means no finite limit is modeled. Core rejects repeating address modes when
    /// this value is finite because it cannot prove the coordinate range used by a draw.
    pub max_texture_repeat_per_primitive: u32,
    /// Maximum anisotropy value that may be passed to the backend.
    ///
    /// This is capped at 16, the maximum accepted by the wgpu HAL sampler
    /// interface. A value of 1 means that anisotropic filtering is unavailable;
    /// the [`DownlevelFlags::ANISOTROPIC_FILTERING`] flag must also be present
    /// for core to use values greater than 1.
    #[cfg_attr(feature = "serde", serde(default = "default_max_sampler_anisotropy"))]
    pub max_sampler_anisotropy: u16,
    /// Whether `BlendFactor::SrcAlphaSaturated` is supported.
    ///
    /// Direct3D feature levels 9_1 through 9_3 do not support this blend factor.
    pub src_alpha_saturated_blend: bool,
    /// Whether viewport rectangles may extend beyond the active render target.
    ///
    /// This is false on backends that cannot preserve clipped viewport semantics for every
    /// shader, such as when clipping requires an optional shader variant.
    pub viewport_rectangles_outside_target: bool,
    /// Whether depth bias may be combined with a fragment entry point that writes `frag_depth`.
    ///
    /// Adapters that cannot preserve this combination reject the pipeline during validation.
    pub depth_bias_with_fragment_depth: bool,
    /// Whether a fragment entry point writing `frag_depth` may omit `@location(0)` output.
    ///
    /// Some shader profiles require `SV_Target0` even when the only observable output is depth.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub fragment_depth_without_color_target_0: bool,
    /// Whether fragment entry points may write `@builtin(sample_mask)`.
    ///
    /// This depends on the pixel-shader profile and may be unavailable even when the API supports
    /// single-sample rendering.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub fragment_sample_mask_output: bool,
    /// Whether fragment inputs may use `@interpolate(flat)` or `@interpolate(linear)`.
    ///
    /// These interpolation modes may be unavailable in downlevel pixel-shader profiles even
    /// though the same qualifiers can appear on vertex outputs.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub fragment_non_perspective_interpolation: bool,
    /// Whether fragment entry points may declare `@builtin(front_facing)`.
    ///
    /// This can be unavailable when the shader backend cannot compile the corresponding input
    /// for its selected shader model.
    pub fragment_front_facing: bool,
    /// Whether fragment entry points may use `@builtin(position)` as an input.
    ///
    /// This can be unavailable when the shader backend cannot compile the corresponding input
    /// for its selected shader model.
    pub fragment_position: bool,
    /// Whether shader entry points may query texture dimensions or level counts.
    ///
    /// This can be unavailable when the selected shader profile does not support image-query
    /// instructions.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_query: bool,
    /// Whether shader entry points may use `textureSampleBaseClampToEdge`.
    ///
    /// This operation may lower to texture-dimension queries and explicit level sampling even
    /// though the WGSL builtin does not expose either operation directly.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_sample_base_clamp_to_edge: bool,
    /// Whether sampled texture operations may specify an explicit mip level.
    ///
    /// This models shader-profile support for `textureSampleLevel`; implicit
    /// sampling and `textureSampleBias` are reported separately.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_sample_level: bool,
    /// Whether shader entry points may use `textureGather` operations.
    ///
    /// This applies to every shader stage and can be unavailable when a selected shader profile
    /// does not support texture gather instructions.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_gather: bool,
    /// Whether shader entry points may use `textureLoad` operations.
    ///
    /// This can be unavailable when the backend cannot preserve texel-load semantics for its
    /// selected shader model or resource subset.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_load: bool,
    /// Whether shader entry points may use runtime integer operations.
    ///
    /// Some downlevel shader profiles have integer type declarations but do not preserve the
    /// semantics of runtime integer arithmetic, conversion, comparison or indexing.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub runtime_integer_shader_operations: bool,
    /// Whether stores through structs or arrays containing matrices are supported.
    ///
    /// This can be unavailable when a backend's shader translator cannot preserve aggregate
    /// matrix store semantics.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub matrix_aggregate_stores: bool,
    /// Maximum absolute literal bias accepted by `textureSampleBias`.
    ///
    /// `u32::MAX` means that no finite backend-specific bound is modeled. A
    /// finite value constrains only statically known bias values.
    #[cfg_attr(feature = "serde", serde(default = "default_max_texture_sample_bias"))]
    pub max_texture_sample_bias: u32,
    /// Whether `textureSampleBias` may use a dynamically computed bias.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_sample_bias_dynamic: bool,
    /// Whether `textureSampleBias` may specify a constant texel offset.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub texture_sample_bias_offset: bool,
    /// Whether fragment shaders may use derivatives or explicit texture gradients.
    #[cfg_attr(feature = "serde", serde(default = "default_true"))]
    pub fragment_derivatives: bool,
    /// Bit mask of vertex formats supported by the adapter.
    ///
    /// Bit `n` corresponds to the discriminant of [`VertexFormat`] with value `n`. This lets
    /// downlevel backends report format-specific vertex input support, which cannot be described
    /// by the vertex attribute count alone. The 64-bit vertex formats are still separately gated
    /// by [`Features::VERTEX_ATTRIBUTE_64BIT`].
    pub vertex_format_mask: u64,
}

#[cfg(feature = "serde")]
fn default_true() -> bool {
    true
}

#[cfg(feature = "serde")]
fn default_max_sampler_anisotropy() -> u16 {
    16
}

#[cfg(feature = "serde")]
fn default_max_texture_sample_bias() -> u32 {
    u32::MAX
}

#[cfg(feature = "serde")]
fn default_max_uniform_vectors() -> u32 {
    u32::MAX
}

#[cfg(feature = "serde")]
fn default_max_texture_cube_dimension() -> u32 {
    u32::MAX
}

#[cfg(feature = "serde")]
fn default_texture_cube_format_mask() -> u32 {
    u32::MAX
}

#[cfg(feature = "serde")]
fn default_texture_3d_format_mask() -> u32 {
    u32::MAX
}

#[cfg(feature = "serde")]
fn default_texture_3d_usage_mask() -> u32 {
    crate::TextureUsages::all().bits()
}

#[cfg(feature = "serde")]
fn default_multisampled_texture_usage_mask() -> u32 {
    crate::TextureUsages::all().bits()
}

#[cfg(feature = "serde")]
fn default_texture_to_buffer_copy_format_mask() -> u32 {
    u32::MAX
}

#[cfg(feature = "serde")]
fn default_max_sampled_texture_sampler_pairs() -> u32 {
    u32::MAX
}

impl Default for DownlevelLimits {
    fn default() -> Self {
        Self {
            empty_binding_visibility: true,
            texture_1d: true,
            texture_3d: true,
            texture_3d_usage_mask: crate::TextureUsages::all().bits(),
            texture_3d_format_mask: u32::MAX,
            texture_3d_mipmaps: true,
            texture_3d_non_suffix_mip_views: true,
            multisampled_texture_usage_mask: crate::TextureUsages::all().bits(),
            texture_to_buffer_copy_format_mask: u32::MAX,
            texture_3d_texture_copies: true,
            npot_texture_3d: true,
            max_texture_cube_dimension: u32::MAX,
            texture_cube_format_mask: u32::MAX,
            npot_cube_textures: true,
            sampled_depth_textures: true,
            sampled_sint_textures: true,
            sampled_uint_textures: true,
            multisampled_texture_bindings: true,
            alpha_to_coverage: true,
            shader_loops: true,
            vertex_index_input: true,
            instance_index_input: true,
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
            fragment_sample_mask_output: true,
            fragment_non_perspective_interpolation: true,
            fragment_front_facing: true,
            fragment_position: true,
            texture_query: true,
            texture_sample_base_clamp_to_edge: true,
            texture_sample_level: true,
            texture_gather: true,
            texture_load: true,
            runtime_integer_shader_operations: true,
            matrix_aggregate_stores: true,
            max_texture_sample_bias: u32::MAX,
            texture_sample_bias_dynamic: true,
            texture_sample_bias_offset: true,
            fragment_derivatives: true,
            vertex_format_mask: Self::webgpu_vertex_format_mask(),
        }
    }
}

impl DownlevelLimits {
    /// Returns the bit assigned to a format in the cube-texture capability mask.
    #[must_use]
    pub const fn texture_cube_format_bit(format: crate::TextureFormat) -> u32 {
        match format {
            crate::TextureFormat::Rgba8Unorm => 1 << 0,
            crate::TextureFormat::Bgra8Unorm => 1 << 1,
            crate::TextureFormat::Rgba8UnormSrgb => 1 << 2,
            crate::TextureFormat::Bgra8UnormSrgb => 1 << 3,
            crate::TextureFormat::R8Unorm => 1 << 4,
            crate::TextureFormat::Rg8Unorm => 1 << 5,
            crate::TextureFormat::Rg8Snorm => 1 << 6,
            crate::TextureFormat::Rg16Snorm => 1 << 7,
            crate::TextureFormat::Rgba8Snorm => 1 << 8,
            crate::TextureFormat::Bc1RgbaUnorm => 1 << 9,
            crate::TextureFormat::Rgb5g6b5Unorm => 1 << 10,
            crate::TextureFormat::Rgb5a1Unorm => 1 << 11,
            crate::TextureFormat::Rgba4Unorm => 1 << 12,
            _ => 0,
        }
    }

    /// Returns the bit assigned to a format in the 3D-texture capability mask.
    #[must_use]
    pub const fn texture_3d_format_bit(format: crate::TextureFormat) -> u32 {
        match format {
            crate::TextureFormat::Rgba8Unorm => 1 << 0,
            _ => 0,
        }
    }

    /// Returns whether the selected format is supported for 3D textures.
    #[must_use]
    pub const fn supports_texture_3d_format(&self, format: crate::TextureFormat) -> bool {
        let bit = Self::texture_3d_format_bit(format);
        if bit == 0 {
            self.texture_3d_format_mask == u32::MAX
        } else {
            self.texture_3d_format_mask & bit != 0
        }
    }

    /// Returns whether the 3D-texture usage mask includes all requested usages.
    #[must_use]
    pub const fn supports_texture_3d_usage(&self, usage: crate::TextureUsages) -> bool {
        crate::TextureUsages::from_bits_retain(self.texture_3d_usage_mask).contains(usage)
    }

    /// Returns whether `usage` is allowed for multisampled textures.
    #[must_use]
    pub const fn supports_multisampled_texture_usage(&self, usage: crate::TextureUsages) -> bool {
        crate::TextureUsages::from_bits_retain(self.multisampled_texture_usage_mask).contains(usage)
    }

    /// Returns the bit assigned to a format in the texture-to-buffer copy mask.
    #[must_use]
    pub const fn texture_to_buffer_copy_format_bit(format: crate::TextureFormat) -> u32 {
        match format {
            crate::TextureFormat::Rgba8Unorm => 1 << 0,
            crate::TextureFormat::Bgra8Unorm => 1 << 1,
            crate::TextureFormat::Rgba8UnormSrgb => 1 << 2,
            crate::TextureFormat::Bgra8UnormSrgb => 1 << 3,
            crate::TextureFormat::R8Unorm => 1 << 4,
            crate::TextureFormat::Rg8Unorm => 1 << 5,
            crate::TextureFormat::Rg8Snorm => 1 << 6,
            crate::TextureFormat::Rg16Snorm => 1 << 7,
            crate::TextureFormat::Rgba8Snorm => 1 << 8,
            crate::TextureFormat::Bc1RgbaUnorm => 1 << 9,
            crate::TextureFormat::Rgb5g6b5Unorm => 1 << 10,
            crate::TextureFormat::Rgb5a1Unorm => 1 << 11,
            crate::TextureFormat::Rgba4Unorm => 1 << 12,
            crate::TextureFormat::Depth16Unorm => 1 << 13,
            crate::TextureFormat::Depth24PlusStencil8 => 1 << 14,
            _ => 0,
        }
    }

    /// Returns whether a format may be copied to a buffer on this adapter.
    #[must_use]
    pub const fn supports_texture_to_buffer_copy(&self, format: crate::TextureFormat) -> bool {
        let bit = Self::texture_to_buffer_copy_format_bit(format);
        if bit == 0 {
            self.texture_to_buffer_copy_format_mask == u32::MAX
        } else {
            self.texture_to_buffer_copy_format_mask & bit != 0
        }
    }

    /// Returns whether the selected format can be used by a cube texture.
    #[must_use]
    pub const fn supports_texture_cube_format(&self, format: crate::TextureFormat) -> bool {
        let bit = Self::texture_cube_format_bit(format);
        if bit == 0 {
            self.texture_cube_format_mask == u32::MAX
        } else {
            self.texture_cube_format_mask & bit != 0
        }
    }

    /// Returns the mask of vertex formats required by the WebGPU baseline.
    #[must_use]
    pub const fn webgpu_vertex_format_mask() -> u64 {
        let format_count = crate::VertexFormat::Unorm8x4Bgra as u32 + 1;
        let all_formats = (1u64 << format_count) - 1;
        let float64_start = crate::VertexFormat::Float64 as u32;
        let float64_count = crate::VertexFormat::Float64x4 as u32 - float64_start + 1;
        let float64_formats = ((1u64 << float64_count) - 1) << float64_start;
        all_formats & !float64_formats
    }

    /// Returns whether a particular vertex format is supported by the adapter.
    #[must_use]
    pub const fn supports_vertex_format(&self, format: crate::VertexFormat) -> bool {
        let bit = format as u32;
        bit < u64::BITS && self.vertex_format_mask & (1u64 << bit) != 0
    }

    /// Returns whether `self` satisfies the minimum downlevel limits in `required`.
    #[must_use]
    pub fn check_limits(&self, required: &Self) -> bool {
        (!required.texture_1d || self.texture_1d)
            && (!required.empty_binding_visibility || self.empty_binding_visibility)
            && (!required.texture_3d
                || (self.texture_3d
                    && self.texture_3d_usage_mask & required.texture_3d_usage_mask
                        == required.texture_3d_usage_mask
                    && self.texture_3d_format_mask & required.texture_3d_format_mask
                        == required.texture_3d_format_mask
                    && (!required.texture_3d_mipmaps || self.texture_3d_mipmaps)
                    && (!required.texture_3d_non_suffix_mip_views
                        || self.texture_3d_non_suffix_mip_views)
                    && (!required.texture_3d_texture_copies || self.texture_3d_texture_copies)
                    && (!required.npot_texture_3d || self.npot_texture_3d)))
            && self.multisampled_texture_usage_mask & required.multisampled_texture_usage_mask
                == required.multisampled_texture_usage_mask
            && self.texture_to_buffer_copy_format_mask & required.texture_to_buffer_copy_format_mask
                == required.texture_to_buffer_copy_format_mask
            && self.max_texture_cube_dimension >= required.max_texture_cube_dimension
            && self.texture_cube_format_mask & required.texture_cube_format_mask
                == required.texture_cube_format_mask
            && (!required.npot_cube_textures || self.npot_cube_textures)
            && (!required.sampled_depth_textures || self.sampled_depth_textures)
            && (!required.sampled_sint_textures || self.sampled_sint_textures)
            && (!required.sampled_uint_textures || self.sampled_uint_textures)
            && (!required.multisampled_texture_bindings || self.multisampled_texture_bindings)
            && (!required.alpha_to_coverage || self.alpha_to_coverage)
            && (!required.shader_loops || self.shader_loops)
            && (!required.vertex_index_input || self.vertex_index_input)
            && (!required.instance_index_input || self.instance_index_input)
            && self.max_sampled_textures_per_vertex_stage
                >= required.max_sampled_textures_per_vertex_stage
            && self.max_sampled_texture_sampler_pairs_per_shader_stage
                >= required.max_sampled_texture_sampler_pairs_per_shader_stage
            && self.max_vertex_shader_uniform_vectors >= required.max_vertex_shader_uniform_vectors
            && self.max_fragment_shader_uniform_vectors
                >= required.max_fragment_shader_uniform_vectors
            && (!required.unrestricted_npot_texture_sampling
                || self.unrestricted_npot_texture_sampling)
            && self.max_texture_repeat_per_primitive >= required.max_texture_repeat_per_primitive
            && self.max_sampler_anisotropy >= required.max_sampler_anisotropy
            && (!required.src_alpha_saturated_blend || self.src_alpha_saturated_blend)
            && (!required.viewport_rectangles_outside_target
                || self.viewport_rectangles_outside_target)
            && (!required.depth_bias_with_fragment_depth || self.depth_bias_with_fragment_depth)
            && (!required.fragment_depth_without_color_target_0
                || self.fragment_depth_without_color_target_0)
            && (!required.fragment_sample_mask_output || self.fragment_sample_mask_output)
            && (!required.fragment_non_perspective_interpolation
                || self.fragment_non_perspective_interpolation)
            && (!required.fragment_front_facing || self.fragment_front_facing)
            && (!required.fragment_position || self.fragment_position)
            && (!required.texture_query || self.texture_query)
            && (!required.texture_sample_base_clamp_to_edge
                || self.texture_sample_base_clamp_to_edge)
            && (!required.texture_sample_level || self.texture_sample_level)
            && (!required.texture_gather || self.texture_gather)
            && (!required.texture_load || self.texture_load)
            && (!required.runtime_integer_shader_operations
                || self.runtime_integer_shader_operations)
            && (!required.matrix_aggregate_stores || self.matrix_aggregate_stores)
            && self.max_texture_sample_bias >= required.max_texture_sample_bias
            && (!required.texture_sample_bias_dynamic || self.texture_sample_bias_dynamic)
            && (!required.texture_sample_bias_offset || self.texture_sample_bias_offset)
            && (!required.fragment_derivatives || self.fragment_derivatives)
            && self.vertex_format_mask & required.vertex_format_mask == required.vertex_format_mask
    }
}

/// Lists various ways the underlying platform does not conform to the WebGPU standard.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DownlevelCapabilities {
    /// Combined boolean flags.
    pub flags: DownlevelFlags,
    /// Additional limits and individual downlevel capabilities.
    pub limits: DownlevelLimits,
    /// Which collections of features shaders support. Defined in terms of D3D's shader models.
    pub shader_model: ShaderModel,
}

impl Default for DownlevelCapabilities {
    fn default() -> Self {
        Self {
            flags: DownlevelFlags::all(),
            limits: DownlevelLimits::default(),
            shader_model: ShaderModel::Sm5,
        }
    }
}

impl DownlevelCapabilities {
    /// Returns true if the underlying platform offers complete support of the baseline WebGPU standard.
    ///
    /// If this returns false, some parts of the API will result in validation errors where they would not normally.
    /// These parts can be determined by the values in this structure.
    #[must_use]
    pub fn is_webgpu_compliant(&self) -> bool {
        self.flags.contains(DownlevelFlags::compliant())
            && self.limits.empty_binding_visibility
            && self.limits.texture_1d
            && self.limits.texture_3d
            && self.limits.texture_3d_usage_mask == crate::TextureUsages::all().bits()
            && self.limits.texture_3d_format_mask == u32::MAX
            && self.limits.texture_3d_mipmaps
            && self.limits.texture_3d_non_suffix_mip_views
            && self.limits.multisampled_texture_usage_mask == crate::TextureUsages::all().bits()
            && self.limits.texture_to_buffer_copy_format_mask == u32::MAX
            && self.limits.texture_3d_texture_copies
            && self.limits.npot_texture_3d
            && self.limits.max_texture_cube_dimension
                >= DownlevelLimits::default().max_texture_cube_dimension
            && self.limits.texture_cube_format_mask
                & DownlevelLimits::default().texture_cube_format_mask
                == DownlevelLimits::default().texture_cube_format_mask
            && self.limits.npot_cube_textures
            && self.limits.sampled_depth_textures
            && self.limits.sampled_sint_textures
            && self.limits.sampled_uint_textures
            && self.limits.multisampled_texture_bindings
            && self.limits.alpha_to_coverage
            && self.limits.shader_loops
            && self.limits.vertex_index_input
            && self.limits.instance_index_input
            && self.limits.max_sampled_textures_per_vertex_stage
                >= DownlevelLimits::default().max_sampled_textures_per_vertex_stage
            && self
                .limits
                .max_sampled_texture_sampler_pairs_per_shader_stage
                >= DownlevelLimits::default().max_sampled_texture_sampler_pairs_per_shader_stage
            && self.limits.max_vertex_shader_uniform_vectors
                >= DownlevelLimits::default().max_vertex_shader_uniform_vectors
            && self.limits.max_fragment_shader_uniform_vectors
                >= DownlevelLimits::default().max_fragment_shader_uniform_vectors
            && self.limits.unrestricted_npot_texture_sampling
            && self.limits.max_texture_repeat_per_primitive == u32::MAX
            && self.limits.src_alpha_saturated_blend
            && self.limits.viewport_rectangles_outside_target
            && self.limits.depth_bias_with_fragment_depth
            && self.limits.fragment_depth_without_color_target_0
            && self.limits.fragment_sample_mask_output
            && self.limits.fragment_non_perspective_interpolation
            && self.limits.fragment_front_facing
            && self.limits.fragment_position
            && self.limits.texture_query
            && self.limits.texture_sample_base_clamp_to_edge
            && self.limits.texture_sample_level
            && self.limits.texture_gather
            && self.limits.texture_load
            && self.limits.runtime_integer_shader_operations
            && self.limits.matrix_aggregate_stores
            && self.limits.max_texture_sample_bias
                >= DownlevelLimits::default().max_texture_sample_bias
            && self.limits.texture_sample_bias_dynamic
            && self.limits.texture_sample_bias_offset
            && self.limits.fragment_derivatives
            && self.limits.vertex_format_mask & DownlevelLimits::webgpu_vertex_format_mask()
                == DownlevelLimits::webgpu_vertex_format_mask()
            && self.shader_model >= ShaderModel::Sm5
    }
}

bitflags::bitflags! {
    /// Binary flags listing features that may or may not be present on downlevel adapters.
    ///
    /// A downlevel adapter is a GPU adapter that wgpu supports, but with potentially limited
    /// features, due to the lack of hardware feature support.
    ///
    /// Flags that are **not** present for a downlevel adapter or device usually indicates
    /// non-compliance with the WebGPU specification, but not always.
    ///
    /// You can check whether a set of flags is compliant through the
    /// [`DownlevelCapabilities::is_webgpu_compliant()`] function.
    #[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
    #[cfg_attr(feature = "serde", serde(transparent))]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct DownlevelFlags: u32 {
        /// The device supports compiling and using compute shaders.
        ///
        /// WebGL2, and GLES3.0 devices do not support compute.
        const COMPUTE_SHADERS = 1 << 0;
        /// Supports binding storage buffers and textures to fragment shaders.
        const FRAGMENT_WRITABLE_STORAGE = 1 << 1;
        /// Supports indirect drawing and dispatching.
        ///
        /// [`Self::COMPUTE_SHADERS`] must be present for this flag.
        ///
        /// WebGL2, GLES 3.0, and Metal on Apple1/Apple2 GPUs do not support indirect.
        const INDIRECT_EXECUTION = 1 << 2;
        /// Supports non-zero `base_vertex` parameter to direct indexed draw calls.
        ///
        /// Indirect calls, if supported, always support non-zero `base_vertex`.
        ///
        /// Supported by:
        /// - Vulkan
        /// - DX12
        /// - Metal on Apple3+ or Mac1+
        /// - OpenGL 3.2+
        /// - OpenGL ES 3.2
        const BASE_VERTEX = 1 << 3;
        /// Supports reading from a depth/stencil texture while using it as a read-only
        /// depth/stencil attachment.
        ///
        /// The WebGL2 and GLES backends do not support RODS.
        const READ_ONLY_DEPTH_STENCIL = 1 << 4;
        /// Supports textures with mipmaps which have a non power of two size.
        const NON_POWER_OF_TWO_MIPMAPPED_TEXTURES = 1 << 5;
        /// Supports textures that are cube arrays.
        const CUBE_ARRAY_TEXTURES = 1 << 6;
        /// Supports comparison samplers.
        const COMPARISON_SAMPLERS = 1 << 7;
        /// Supports different blend operations per color attachment.
        const INDEPENDENT_BLEND = 1 << 8;
        /// Supports storage buffers in vertex shaders.
        const VERTEX_STORAGE = 1 << 9;

        /// Supports samplers with anisotropic filtering. Note this isn't actually required by
        /// WebGPU, the implementation is allowed to completely ignore aniso clamp. This flag is
        /// here for native backends so they can communicate to the user of aniso is enabled.
        ///
        /// All backends and all devices support anisotropic filtering.
        const ANISOTROPIC_FILTERING = 1 << 10;

        /// Supports storage buffers in fragment shaders.
        const FRAGMENT_STORAGE = 1 << 11;

        /// Supports sample-rate shading.
        const MULTISAMPLED_SHADING = 1 << 12;

        /// Supports copies between depth textures and buffers.
        ///
        /// GLES/WebGL don't support this.
        const DEPTH_TEXTURE_AND_BUFFER_COPIES = 1 << 13;

        /// Supports all the texture usages described in WebGPU. If this isn't supported, you
        /// should call `get_texture_format_features` to get how you can use textures of a given format
        const WEBGPU_TEXTURE_FORMAT_SUPPORT = 1 << 14;

        /// Supports buffer bindings with sizes that aren't a multiple of 16.
        ///
        /// WebGL doesn't support this.
        const BUFFER_BINDINGS_NOT_16_BYTE_ALIGNED = 1 << 15;

        /// Supports buffers to combine [`BufferUsages::INDEX`] with usages other than [`BufferUsages::COPY_DST`] and [`BufferUsages::COPY_SRC`].
        /// Furthermore, in absence of this feature it is not allowed to copy index buffers from/to buffers with a set of usage flags containing
        /// [`BufferUsages::VERTEX`]/[`BufferUsages::UNIFORM`]/[`BufferUsages::STORAGE`] or [`BufferUsages::INDIRECT`].
        ///
        /// WebGL doesn't support this.
        const UNRESTRICTED_INDEX_BUFFER = 1 << 16;

        /// Supports full 32-bit range indices (2^32-1 as opposed to 2^24-1 without this flag)
        ///
        /// Corresponds to Vulkan's `VkPhysicalDeviceFeatures.fullDrawIndexUint32`
        const FULL_DRAW_INDEX_UINT32 = 1 << 17;

        /// Supports depth bias clamping
        ///
        /// Corresponds to Vulkan's `VkPhysicalDeviceFeatures.depthBiasClamp`
        const DEPTH_BIAS_CLAMP = 1 << 18;

        /// Supports specifying which view format values are allowed when create_view() is called on a texture.
        ///
        /// The WebGL and GLES backends doesn't support this.
        const VIEW_FORMATS = 1 << 19;

        /// With this feature not present, there are the following restrictions on `Queue::copy_external_image_to_texture`:
        /// - The source must not be [`web_sys::OffscreenCanvas`]
        /// - [`CopyExternalImageSourceInfo::origin`] must be zero.
        /// - [`CopyExternalImageDestInfo::color_space`] must be srgb.
        /// - If the source is an [`web_sys::ImageBitmap`]:
        ///   - [`CopyExternalImageSourceInfo::flip_y`] must be false.
        ///   - [`CopyExternalImageDestInfo::premultiplied_alpha`] must be false.
        ///
        /// WebGL doesn't support this. WebGPU does.
        const UNRESTRICTED_EXTERNAL_TEXTURE_COPIES = 1 << 20;

        /// Supports specifying which view formats are allowed when calling create_view on the texture returned by
        /// `Surface::get_current_texture`.
        ///
        /// The GLES/WebGL and Vulkan on Android doesn't support this.
        const SURFACE_VIEW_FORMATS = 1 << 21;

        /// If this is true, calls to `CommandEncoder::resolve_query_set` will be performed on the queue timeline.
        ///
        /// If this is false, calls to `CommandEncoder::resolve_query_set` will be performed on the device (i.e. cpu) timeline
        /// and will block that timeline until the query has data. You may work around this limitation by waiting until the submit
        /// whose queries you are resolving is fully finished (through use of `queue.on_submitted_work_done`) and only
        /// then submitting the resolve_query_set command. The queries will be guaranteed finished, so will not block.
        ///
        /// Supported by:
        /// - Vulkan,
        /// - DX12
        /// - Metal
        /// - OpenGL 4.4+
        ///
        /// Not Supported by:
        /// - GL ES / WebGL
        const NONBLOCKING_QUERY_RESOLVE = 1 << 22;

        /// Allows shaders to use `quantizeToF16`, `pack2x16float`, and `unpack2x16float`, which
        /// operate on `f16`-precision values stored in `f32`s.
        ///
        /// Not supported by Vulkan on Mesa when [`Features::SHADER_F16`] is absent.
        const SHADER_F16_IN_F32 = 1 << 23;

        /// Supports features introduced in MSL 2.1.
        const MSL2_1 = 1 << 24;

        /// The adapter supports the WebGPU texture compression requirement:
        /// BC || (ETC2 && ASTC).
        ///
        /// See <https://www.w3.org/TR/webgpu/#adapter-capability-guarantees>.
        const TEXTURE_COMPRESSION = 1 << 25;

        /// Supports creating and using occlusion query sets.
        ///
        /// Custom HAL adapters must report this flag to permit occlusion queries.
        const OCCLUSION_QUERY = 1 << 26;

        /// Supports distinct stencil state for front-facing and back-facing primitives.
        ///
        /// Without this flag, render pipelines with different front and back stencil
        /// face states are rejected during validation.
        const TWO_SIDED_STENCIL = 1 << 27;

        /// Supports different blend equations for RGB and alpha in one render target.
        ///
        /// Without this flag, pipelines that write both RGB and alpha and specify
        /// different blend components are rejected during validation.
        const SEPARATE_ALPHA_BLEND = 1 << 28;

        /// Supports 2D texture array views and sampling.
        const TEXTURE_2D_ARRAYS = 1 << 29;

        /// Supports cube texture views and sampling.
        const CUBE_TEXTURES = 1 << 30;

        /// Supports depth/stencil attachment textures with multiple mip levels.
        ///
        /// D3D11 feature levels 9_1 through 9_3 require resources bound as
        /// depth/stencil attachments to have exactly one mip level.
        const DEPTH_TEXTURE_MULTI_MIP = 1 << 31;
    }
}

impl DownlevelFlags {
    /// All flags that indicate if the backend is WebGPU compliant
    #[must_use]
    pub const fn compliant() -> Self {
        // We use manual bit twiddling to make this a const fn as `Sub` and `.remove` aren't const

        // WebGPU doesn't actually require aniso
        Self::from_bits_truncate(Self::all().bits() & !Self::ANISOTROPIC_FILTERING.bits())
    }
}

/// Collections of shader features a device supports if they support less than WebGPU normally allows.
// TODO: Fill out the differences between shader models more completely
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ShaderModel {
    /// Extremely limited shaders, including a total instruction limit.
    Sm2,
    /// Missing minor features and storage images.
    Sm4,
    /// WebGPU supports shader module 5.
    Sm5,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextureFormat;
    use alloc::{format, string::String, vec::Vec};

    #[test]
    fn occlusion_queries_are_required_for_webgpu_compliance() {
        let mut capabilities = DownlevelCapabilities::default();
        assert!(capabilities.flags.contains(DownlevelFlags::OCCLUSION_QUERY));
        assert!(capabilities.is_webgpu_compliant());
        capabilities.flags.remove(DownlevelFlags::OCCLUSION_QUERY);
        assert!(!capabilities.is_webgpu_compliant());
    }

    #[test]
    fn compliant_flags_include_two_sided_stencil_and_separate_alpha_blend() {
        assert!(DownlevelCapabilities::default()
            .flags
            .contains(DownlevelFlags::TWO_SIDED_STENCIL));
        assert!(DownlevelFlags::compliant().contains(DownlevelFlags::TWO_SIDED_STENCIL));
        assert!(DownlevelCapabilities::default()
            .flags
            .contains(DownlevelFlags::SEPARATE_ALPHA_BLEND));
        assert!(DownlevelFlags::compliant().contains(DownlevelFlags::SEPARATE_ALPHA_BLEND));
        assert!(DownlevelCapabilities::default()
            .flags
            .contains(DownlevelFlags::TEXTURE_2D_ARRAYS | DownlevelFlags::CUBE_TEXTURES));
        assert!(DownlevelFlags::compliant()
            .contains(DownlevelFlags::TEXTURE_2D_ARRAYS | DownlevelFlags::CUBE_TEXTURES));
        assert!(DownlevelCapabilities::default()
            .flags
            .contains(DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP));
        assert!(DownlevelFlags::compliant().contains(DownlevelFlags::DEPTH_TEXTURE_MULTI_MIP));
    }

    #[test]
    fn extra_vertex_texture_capacity_does_not_break_downlevel_compliance() {
        let mut capabilities = DownlevelCapabilities::default();
        capabilities.limits.max_sampled_textures_per_vertex_stage = 32;
        assert!(capabilities.is_webgpu_compliant());

        capabilities.limits.max_sampled_textures_per_vertex_stage = 15;
        assert!(!capabilities.is_webgpu_compliant());

        capabilities.limits.max_sampled_textures_per_vertex_stage = 16;
        capabilities.limits.unrestricted_npot_texture_sampling = false;
        assert!(!capabilities.is_webgpu_compliant());

        capabilities.limits.unrestricted_npot_texture_sampling = true;
        capabilities.limits.max_texture_repeat_per_primitive = 128;
        assert!(!capabilities.is_webgpu_compliant());
    }

    #[test]
    fn alpha_to_coverage_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            alpha_to_coverage: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            alpha_to_coverage: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            alpha_to_coverage: true,
            ..DownlevelLimits::default()
        }));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn shader_loops_are_a_downlevel_capability() {
        let available = DownlevelLimits {
            shader_loops: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            shader_loops: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn cube_texture_limits_are_checked_separately_from_2d_limits() {
        let rgba8 = DownlevelLimits::texture_cube_format_bit(crate::TextureFormat::Rgba8Unorm);
        let r8 = DownlevelLimits::texture_cube_format_bit(crate::TextureFormat::R8Unorm);
        let available = DownlevelLimits {
            max_texture_cube_dimension: 512,
            texture_cube_format_mask: rgba8,
            npot_cube_textures: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            max_texture_cube_dimension: 256,
            texture_cube_format_mask: rgba8,
            npot_cube_textures: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_texture_cube_dimension: 512,
            texture_cube_format_mask: r8,
            npot_cube_textures: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.supports_texture_cube_format(crate::TextureFormat::Rgba16Float));
        assert!(DownlevelLimits::default()
            .supports_texture_cube_format(crate::TextureFormat::Rgba16Float));
        assert!(!available.check_limits(&DownlevelLimits {
            max_texture_cube_dimension: 513,
            npot_cube_textures: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_texture_cube_dimension: 256,
            npot_cube_textures: true,
            ..DownlevelLimits::default()
        }));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn three_dimensional_texture_limits_are_checked_as_a_shape_contract() {
        let rgba8 = DownlevelLimits::texture_3d_format_bit(crate::TextureFormat::Rgba8Unorm);
        let supported_usage = (crate::TextureUsages::TEXTURE_BINDING
            | crate::TextureUsages::COPY_DST)
            .bits();
        let available = DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: false,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        };
        let required = DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: false,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&required));
        assert!(!available.check_limits(&DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: crate::TextureUsages::STORAGE_BINDING.bits(),
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: false,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: 1 << 1,
            texture_3d_mipmaps: false,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.supports_texture_3d_format(crate::TextureFormat::R8Unorm));
        assert!(!available.check_limits(&DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: true,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: false,
            texture_3d_texture_copies: true,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        }));
        let suffix_mips = DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: true,
            texture_3d_non_suffix_mip_views: false,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        };
        assert!(suffix_mips.check_limits(&DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: true,
            texture_3d_non_suffix_mip_views: false,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        }));
        assert!(!suffix_mips.check_limits(&DownlevelLimits {
            texture_3d: true,
            texture_3d_usage_mask: supported_usage,
            texture_3d_format_mask: rgba8,
            texture_3d_mipmaps: true,
            texture_3d_non_suffix_mip_views: true,
            texture_3d_texture_copies: false,
            npot_texture_3d: false,
            ..DownlevelLimits::default()
        }));
        assert!(available.check_limits(&DownlevelLimits {
            texture_3d: false,
            ..DownlevelLimits::default()
        }));
    }

    #[test]
    fn downlevel_limits_check_minimum_capabilities() {
        let available = DownlevelLimits {
            empty_binding_visibility: true,
            texture_1d: true,
            texture_3d: true,
            texture_3d_usage_mask: crate::TextureUsages::all().bits(),
            texture_3d_format_mask: u32::MAX,
            texture_3d_mipmaps: true,
            texture_3d_non_suffix_mip_views: true,
            multisampled_texture_usage_mask: crate::TextureUsages::all().bits(),
            texture_to_buffer_copy_format_mask: u32::MAX,
            texture_3d_texture_copies: true,
            npot_texture_3d: true,
            sampled_depth_textures: true,
            sampled_sint_textures: true,
            sampled_uint_textures: true,
            multisampled_texture_bindings: true,
            alpha_to_coverage: true,
            shader_loops: false,
            vertex_index_input: false,
            instance_index_input: false,
            max_sampled_textures_per_vertex_stage: 4,
            max_sampled_texture_sampler_pairs_per_shader_stage: 16,
            max_vertex_shader_uniform_vectors: 4,
            max_fragment_shader_uniform_vectors: 8,
            max_texture_cube_dimension: 512,
            texture_cube_format_mask: u32::MAX,
            npot_cube_textures: false,
            unrestricted_npot_texture_sampling: false,
            max_texture_repeat_per_primitive: 128,
            max_sampler_anisotropy: 2,
            src_alpha_saturated_blend: false,
            viewport_rectangles_outside_target: false,
            depth_bias_with_fragment_depth: false,
            fragment_depth_without_color_target_0: false,
            fragment_sample_mask_output: false,
            fragment_non_perspective_interpolation: false,
            fragment_front_facing: false,
            fragment_position: false,
            texture_query: false,
            texture_sample_base_clamp_to_edge: false,
            texture_sample_level: false,
            texture_gather: false,
            texture_load: true,
            runtime_integer_shader_operations: false,
            matrix_aggregate_stores: false,
            max_texture_sample_bias: 3,
            texture_sample_bias_dynamic: false,
            texture_sample_bias_offset: false,
            fragment_derivatives: false,
            vertex_format_mask: DownlevelLimits::webgpu_vertex_format_mask(),
        };
        let required = DownlevelLimits {
            empty_binding_visibility: true,
            texture_1d: true,
            texture_3d: true,
            texture_3d_usage_mask: crate::TextureUsages::all().bits(),
            texture_3d_format_mask: u32::MAX,
            texture_3d_mipmaps: true,
            texture_3d_non_suffix_mip_views: true,
            multisampled_texture_usage_mask: crate::TextureUsages::all().bits(),
            texture_to_buffer_copy_format_mask: u32::MAX,
            texture_3d_texture_copies: true,
            npot_texture_3d: true,
            sampled_depth_textures: true,
            sampled_sint_textures: true,
            sampled_uint_textures: true,
            multisampled_texture_bindings: true,
            alpha_to_coverage: true,
            shader_loops: false,
            vertex_index_input: false,
            instance_index_input: false,
            max_sampled_textures_per_vertex_stage: 3,
            max_sampled_texture_sampler_pairs_per_shader_stage: 16,
            max_vertex_shader_uniform_vectors: 3,
            max_fragment_shader_uniform_vectors: 8,
            max_texture_cube_dimension: 256,
            texture_cube_format_mask: u32::MAX,
            npot_cube_textures: false,
            unrestricted_npot_texture_sampling: false,
            max_texture_repeat_per_primitive: 128,
            max_sampler_anisotropy: 2,
            src_alpha_saturated_blend: false,
            viewport_rectangles_outside_target: false,
            depth_bias_with_fragment_depth: false,
            fragment_depth_without_color_target_0: false,
            fragment_sample_mask_output: false,
            fragment_non_perspective_interpolation: false,
            fragment_front_facing: false,
            fragment_position: false,
            texture_query: false,
            texture_sample_base_clamp_to_edge: false,
            texture_sample_level: false,
            texture_gather: false,
            texture_load: true,
            runtime_integer_shader_operations: false,
            matrix_aggregate_stores: false,
            max_texture_sample_bias: 3,
            texture_sample_bias_dynamic: false,
            texture_sample_bias_offset: false,
            fragment_derivatives: false,
            vertex_format_mask: DownlevelLimits::webgpu_vertex_format_mask(),
        };
        assert!(available.check_limits(&required));
        assert!(!available.check_limits(&DownlevelLimits {
            runtime_integer_shader_operations: true,
            ..required.clone()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            matrix_aggregate_stores: true,
            ..required.clone()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_sampled_texture_sampler_pairs_per_shader_stage: 17,
            ..required.clone()
        }));

        assert!(!available.check_limits(&DownlevelLimits {
            max_texture_repeat_per_primitive: 129,
            ..required.clone()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            unrestricted_npot_texture_sampling: true,
            ..required
        }));
    }

    #[test]
    fn sampler_anisotropy_is_a_numeric_downlevel_limit() {
        let available = DownlevelLimits {
            max_sampler_anisotropy: 2,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            max_sampler_anisotropy: 2,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_sampler_anisotropy: 3,
            ..DownlevelLimits::default()
        }));
        assert!(DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
        assert!(DownlevelCapabilities::default().is_webgpu_compliant());
    }

    #[test]
    fn shader_uniform_vector_limits_are_numeric_downlevel_limits() {
        let available = DownlevelLimits {
            max_vertex_shader_uniform_vectors: 255,
            max_fragment_shader_uniform_vectors: 32,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            max_vertex_shader_uniform_vectors: 255,
            max_fragment_shader_uniform_vectors: 32,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_vertex_shader_uniform_vectors: 256,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_fragment_shader_uniform_vectors: 33,
            ..DownlevelLimits::default()
        }));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn sampled_texture_sampler_pairs_are_a_numeric_downlevel_limit() {
        let available = DownlevelLimits {
            max_sampled_texture_sampler_pairs_per_shader_stage: 16,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            max_sampled_texture_sampler_pairs_per_shader_stage: 16,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            max_sampled_texture_sampler_pairs_per_shader_stage: 17,
            ..DownlevelLimits::default()
        }));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn empty_binding_visibility_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            empty_binding_visibility: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            empty_binding_visibility: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn sampled_texture_types_are_downlevel_capabilities() {
        for available in [
            DownlevelLimits {
                sampled_depth_textures: false,
                ..DownlevelLimits::default()
            },
            DownlevelLimits {
                sampled_sint_textures: false,
                ..DownlevelLimits::default()
            },
            DownlevelLimits {
                sampled_uint_textures: false,
                ..DownlevelLimits::default()
            },
        ] {
            assert!(!available.check_limits(&DownlevelLimits::default()));
            assert!(!DownlevelCapabilities {
                limits: available,
                ..DownlevelCapabilities::default()
            }
            .is_webgpu_compliant());
        }
    }

    #[test]
    fn multisampled_texture_bindings_are_a_downlevel_capability() {
        let available = DownlevelLimits {
            multisampled_texture_bindings: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            multisampled_texture_bindings: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn multisampled_texture_usage_mask_is_a_downlevel_limit() {
        let allowed =
            crate::TextureUsages::RENDER_ATTACHMENT | crate::TextureUsages::TRANSIENT_ATTACHMENT;
        let available = DownlevelLimits {
            multisampled_texture_usage_mask: allowed.bits(),
            ..DownlevelLimits::default()
        };
        assert!(available.supports_multisampled_texture_usage(allowed));
        assert!(!available.supports_multisampled_texture_usage(
            crate::TextureUsages::RENDER_ATTACHMENT | crate::TextureUsages::COPY_SRC
        ));
        assert!(available.check_limits(&DownlevelLimits {
            multisampled_texture_usage_mask: allowed.bits(),
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn texture_to_buffer_copy_format_mask_is_a_downlevel_limit() {
        let rgba8 = DownlevelLimits::texture_to_buffer_copy_format_bit(TextureFormat::Rgba8Unorm);
        let bc1 = DownlevelLimits::texture_to_buffer_copy_format_bit(TextureFormat::Bc1RgbaUnorm);
        let available = DownlevelLimits {
            texture_to_buffer_copy_format_mask: rgba8,
            ..DownlevelLimits::default()
        };

        assert!(available.supports_texture_to_buffer_copy(TextureFormat::Rgba8Unorm));
        assert!(!available.supports_texture_to_buffer_copy(TextureFormat::Bc1RgbaUnorm));
        assert!(!available.supports_texture_to_buffer_copy(TextureFormat::Rgba16Float));
        assert!(available.check_limits(&DownlevelLimits {
            texture_to_buffer_copy_format_mask: rgba8,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            texture_to_buffer_copy_format_mask: bc1,
            ..DownlevelLimits::default()
        }));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
        assert!(
            DownlevelLimits::default().supports_texture_to_buffer_copy(TextureFormat::Bc1RgbaUnorm)
        );
        assert!(
            DownlevelLimits::default().supports_texture_to_buffer_copy(TextureFormat::Rgba16Float)
        );
    }

    #[test]
    fn explicit_texture_sample_level_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            texture_sample_level: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            texture_sample_level: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn texture_gather_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            texture_gather: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            texture_gather: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn texture_query_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            texture_query: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            texture_query: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn texture_sample_base_clamp_to_edge_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            texture_sample_base_clamp_to_edge: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            texture_sample_base_clamp_to_edge: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn previous_downlevel_limits_serde_shape_defaults_new_caps() {
        let mut serialized = serde_json::to_value(DownlevelLimits::default()).unwrap();
        let object = serialized.as_object_mut().unwrap();
        for field in [
            "empty_binding_visibility",
            "sampled_depth_textures",
            "sampled_sint_textures",
            "sampled_uint_textures",
            "multisampled_texture_bindings",
            "alpha_to_coverage",
            "shader_loops",
            "max_sampled_texture_sampler_pairs_per_shader_stage",
            "max_vertex_shader_uniform_vectors",
            "max_fragment_shader_uniform_vectors",
            "max_sampler_anisotropy",
            "fragment_depth_without_color_target_0",
            "fragment_sample_mask_output",
            "fragment_non_perspective_interpolation",
            "texture_query",
            "texture_sample_base_clamp_to_edge",
            "texture_gather",
            "texture_load",
            "runtime_integer_shader_operations",
            "matrix_aggregate_stores",
            "texture_sample_level",
            "max_texture_sample_bias",
            "texture_sample_bias_dynamic",
            "texture_sample_bias_offset",
            "fragment_derivatives",
        ] {
            object.remove(field);
        }

        let limits: DownlevelLimits = serde_json::from_value(serialized).unwrap();
        assert!(limits.empty_binding_visibility);
        assert!(limits.sampled_depth_textures);
        assert!(limits.sampled_sint_textures);
        assert!(limits.sampled_uint_textures);
        assert!(limits.multisampled_texture_bindings);
        assert!(limits.alpha_to_coverage);
        assert!(limits.shader_loops);
        assert_eq!(
            limits.max_sampled_texture_sampler_pairs_per_shader_stage,
            u32::MAX
        );
        assert_eq!(limits.max_vertex_shader_uniform_vectors, u32::MAX);
        assert_eq!(limits.max_fragment_shader_uniform_vectors, u32::MAX);
        assert_eq!(limits.max_sampler_anisotropy, 16);
        assert!(limits.fragment_depth_without_color_target_0);
        assert!(limits.fragment_sample_mask_output);
        assert!(limits.fragment_non_perspective_interpolation);
        assert!(limits.texture_query);
        assert!(limits.texture_sample_base_clamp_to_edge);
        assert!(limits.texture_gather);
        assert!(limits.texture_load);
        assert!(limits.runtime_integer_shader_operations);
        assert!(limits.matrix_aggregate_stores);
        assert!(limits.texture_sample_level);
        assert_eq!(limits.max_texture_sample_bias, u32::MAX);
        assert!(limits.texture_sample_bias_dynamic);
        assert!(limits.texture_sample_bias_offset);
        assert!(limits.fragment_derivatives);
    }

    #[test]
    fn viewport_rectangles_outside_target_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            viewport_rectangles_outside_target: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            viewport_rectangles_outside_target: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn depth_bias_with_fragment_depth_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            depth_bias_with_fragment_depth: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            depth_bias_with_fragment_depth: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn fragment_depth_without_color_target_0_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            fragment_depth_without_color_target_0: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            fragment_depth_without_color_target_0: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn fragment_sample_mask_output_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            fragment_sample_mask_output: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            fragment_sample_mask_output: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn fragment_non_perspective_interpolation_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            fragment_non_perspective_interpolation: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            fragment_non_perspective_interpolation: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn fragment_front_facing_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            fragment_front_facing: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            fragment_front_facing: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn fragment_position_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            fragment_position: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            fragment_position: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn texture_load_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            texture_load: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            texture_load: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn unsupported_fl9_shader_semantics_are_downlevel_capabilities() {
        for available in [
            DownlevelLimits {
                runtime_integer_shader_operations: false,
                ..DownlevelLimits::default()
            },
            DownlevelLimits {
                matrix_aggregate_stores: false,
                ..DownlevelLimits::default()
            },
        ] {
            assert!(!available.check_limits(&DownlevelLimits::default()));
            assert!(!DownlevelCapabilities {
                limits: available,
                ..DownlevelCapabilities::default()
            }
            .is_webgpu_compliant());
        }
    }

    #[test]
    fn texture_sample_bias_restrictions_are_downlevel_limits() {
        let available = DownlevelLimits {
            max_texture_sample_bias: 3,
            texture_sample_bias_dynamic: false,
            texture_sample_bias_offset: false,
            fragment_derivatives: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            max_texture_sample_bias: 3,
            texture_sample_bias_dynamic: false,
            texture_sample_bias_offset: false,
            fragment_derivatives: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!available.check_limits(&DownlevelLimits {
            max_texture_sample_bias: 4,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits {
            fragment_derivatives: true,
            ..DownlevelLimits::default()
        }));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn vertex_builtins_are_downlevel_capabilities() {
        let available = DownlevelLimits {
            vertex_index_input: false,
            instance_index_input: false,
            ..DownlevelLimits::default()
        };
        let required = DownlevelLimits {
            vertex_index_input: false,
            instance_index_input: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&required));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn texture_dimensions_are_downlevel_capabilities() {
        let available = DownlevelLimits {
            texture_1d: false,
            texture_3d: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            texture_1d: false,
            texture_3d: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));
        assert!(!DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        }
        .is_webgpu_compliant());
    }

    #[test]
    fn vertex_format_masks_are_subset_capabilities() {
        let position = crate::VertexFormat::Float32x3;
        let color = crate::VertexFormat::Unorm8x4;
        let available = DownlevelLimits {
            vertex_format_mask: (1 << position as u32) | (1 << color as u32),
            ..DownlevelLimits::default()
        };
        let required = DownlevelLimits {
            vertex_format_mask: 1 << color as u32,
            ..DownlevelLimits::default()
        };
        assert!(available.supports_vertex_format(position));
        assert!(available.check_limits(&required));
        assert!(!available.check_limits(&DownlevelLimits::default()));

        let mut compliant = DownlevelCapabilities::default();
        assert!(compliant.is_webgpu_compliant());
        compliant.limits.vertex_format_mask &= !(1 << crate::VertexFormat::Unorm8x4 as u32);
        assert!(!compliant.is_webgpu_compliant());
        assert!(!compliant
            .limits
            .supports_vertex_format(crate::VertexFormat::Unorm8x4));
    }

    #[test]
    fn source_alpha_saturated_blend_is_a_downlevel_capability() {
        let available = DownlevelLimits {
            src_alpha_saturated_blend: false,
            ..DownlevelLimits::default()
        };
        assert!(available.check_limits(&DownlevelLimits {
            src_alpha_saturated_blend: false,
            ..DownlevelLimits::default()
        }));
        assert!(!available.check_limits(&DownlevelLimits::default()));

        let capabilities = DownlevelCapabilities {
            limits: available,
            ..DownlevelCapabilities::default()
        };
        assert!(!capabilities.is_webgpu_compliant());
    }

    fn side_by_side(left: &str, right: &str) -> String {
        let left_lines: Vec<&str> = left.lines().map(str::trim).collect();
        let right_lines: Vec<&str> = right.lines().map(str::trim).collect();
        let max_lines = left_lines.len().max(right_lines.len());
        let diffs: Vec<(&str, &str)> = (0..max_lines)
            .map(|i| {
                let l = *left_lines.get(i).unwrap_or(&"");
                let r = *right_lines.get(i).unwrap_or(&"");
                (l, r)
            })
            .filter(|(l, r)| l != r)
            .collect();
        let left_width = diffs.iter().map(|(l, _)| l.len()).max().unwrap_or(0);
        let mut out = String::new();
        for (l, r) in &diffs {
            out += &format!("{:<width$}  |  {}\n", l, r, width = left_width);
        }
        out
    }

    #[test]
    fn with_limits_exhaustive() {
        // Check that all limits are included in `with_limits!`, by using it to
        // replicate `Limits::unlimited()`.
        let mut limits = Limits::default();

        macro_rules! set_to_max {
            ($name:ident, $ordering:expr) => {
                if $ordering == Ordering::Less {
                    limits.$name = i32::MAX as _;
                } else {
                    limits.$name = 1;
                }
            };
        }

        with_limits!(set_to_max);

        assert_eq!(
            limits,
            Limits::unlimited(),
            "with_limits! did not replicate Limits::unlimited():\n{}",
            side_by_side(
                &format!("with_limits!\n------------\n{:#?}", limits),
                &format!(
                    "Limits::unlimited()\n-------------------\n{:#?}",
                    Limits::unlimited()
                ),
            )
        );
    }
}
