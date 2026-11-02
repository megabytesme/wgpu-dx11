//! CoreWindow HAL surface with exclusive backbuffer ownership.
use super::{
    device::NativeDevice,
    swapchain::{CoreWindowFrame, CoreWindowSwapchain},
    texture::NativeTexture,
};
use alloc::{format, sync::Arc};
use core::{borrow::Borrow, ffi::c_void, time::Duration};
use parking_lot::Mutex;
use windows::{
    UI::Core::ICoreWindow,
    Win32::{
        Foundation::E_OUTOFMEMORY,
        Graphics::Dxgi::{
            DXGI_ERROR_DEVICE_HUNG, DXGI_ERROR_DEVICE_REMOVED, DXGI_ERROR_DEVICE_RESET,
        },
    },
    core::{AgileReference, IUnknown, Interface},
};

#[derive(Debug)]
struct Configured {
    device: Arc<NativeDevice>,
    swapchain: CoreWindowSwapchain,
}

/// A CoreWindow retained through a COM agile reference.
#[derive(Debug)]
pub struct Surface {
    window: AgileReference<ICoreWindow>,
    configured: Mutex<Option<Configured>>,
}
crate::impl_dyn_resource!(Surface);

/// One acquired swapchain texture. The caller must destroy derived views before
/// presenting or discarding, as required by the HAL surface contract.
#[doc(hidden)]
#[derive(Debug)]
pub struct SurfaceTexture {
    texture: Option<NativeTexture>,
    frame: Option<CoreWindowFrame>,
}
crate::impl_dyn_resource!(SurfaceTexture);
impl crate::DynSurfaceTexture for SurfaceTexture {}
impl Borrow<NativeTexture> for SurfaceTexture {
    fn borrow(&self) -> &NativeTexture {
        self.texture.as_ref().expect("live surface texture")
    }
}
impl Borrow<dyn crate::DynTexture> for SurfaceTexture {
    fn borrow(&self) -> &(dyn crate::DynTexture + 'static) {
        self.texture.as_ref().expect("live surface texture")
    }
}
impl SurfaceTexture {
    pub(super) fn belongs_to(&self, device: &Arc<NativeDevice>) -> bool {
        self.frame
            .as_ref()
            .is_some_and(|frame| frame.belongs_to(device))
    }
    fn take_frame(&mut self) -> CoreWindowFrame {
        // Release our extra COM reference before dropping the frame's exclusive
        // acquire token, so resize cannot race a retained backbuffer.
        self.texture.take();
        self.frame.take().expect("live surface frame")
    }
}
impl Drop for SurfaceTexture {
    fn drop(&mut self) {
        self.texture.take();
        self.frame.take();
    }
}

fn surface_error(error: windows::core::Error) -> crate::SurfaceError {
    let code = error.code();
    log::error!("D3D11 surface error: {error}");
    if code == E_OUTOFMEMORY {
        crate::SurfaceError::Device(crate::DeviceError::OutOfMemory)
    } else if [
        DXGI_ERROR_DEVICE_HUNG,
        DXGI_ERROR_DEVICE_REMOVED,
        DXGI_ERROR_DEVICE_RESET,
    ]
    .contains(&code)
    {
        crate::SurfaceError::Device(crate::DeviceError::Lost)
    } else {
        crate::SurfaceError::Other("D3D11 CoreWindow swapchain operation failed")
    }
}

impl Surface {
    pub(super) unsafe fn from_core_window(raw: *mut c_void) -> Result<Self, crate::InstanceError> {
        let unknown = unsafe { IUnknown::from_raw_borrowed(&raw) }
            .ok_or_else(|| crate::InstanceError::new("null CoreWindow".into()))?;
        let window: ICoreWindow = unknown
            .cast()
            .map_err(|e| crate::InstanceError::new(format!("not an ICoreWindow: {e}")))?;
        let window = AgileReference::new(&window)
            .map_err(|e| crate::InstanceError::new(format!("could not retain CoreWindow: {e}")))?;
        Ok(Self {
            window,
            configured: Mutex::new(None),
        })
    }
    pub(super) fn core_window(&self) -> windows::core::Result<ICoreWindow> {
        self.window.resolve()
    }

    /// Exercise the private HAL surface and queue in the UWP diagnostic.
    /// No adapter exposes this incomplete path.
    ///
    /// # Safety
    /// The surface must be unconfigured, have no acquired textures or derived
    /// views, and have no outstanding GPU work.
    #[doc(hidden)]
    pub unsafe fn diagnostic_cycle(
        &self,
        native: Arc<NativeDevice>,
        shader_input: crate::NagaShader,
    ) -> Result<[u8; 4], crate::SurfaceError> {
        use crate::{DynDevice, DynQueue, DynSurface};
        let device = super::Device::new(native.clone());
        let native_queue = super::Queue::new(native.clone());
        let fence = native.create_native_fence();
        let surface: &dyn DynSurface = self;
        let queue: &dyn DynQueue = &native_queue;
        let mut config = crate::SurfaceConfiguration {
            maximum_frame_latency: 1,
            present_mode: wgt::PresentMode::Fifo,
            composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
            format: wgt::TextureFormat::Bgra8Unorm,
            color_space: wgt::SurfaceColorSpace::Srgb,
            extent: wgt::Extent3d {
                width: 640,
                height: 480,
                depth_or_array_layers: 1,
            },
            usage: wgt::TextureUses::COLOR_TARGET,
            view_formats: alloc::vec::Vec::new(),
        };
        unsafe {
            surface.configure(&device, &config)?;
        }
        if native.feature_level() == super::FeatureLevel::L11_0 {
            let state = self.configured.lock();
            state
                .as_ref()
                .expect("configured")
                .swapchain
                .verify_texture_array()
                .map_err(surface_error)?;
        }
        let first = unsafe { surface.acquire_texture(None, &fence)? };
        {
            let state = self.configured.lock();
            if state
                .as_ref()
                .expect("configured")
                .swapchain
                .acquire()
                .is_ok()
            {
                return Err(crate::SurfaceError::Other(
                    "duplicate acquire unexpectedly succeeded",
                ));
            }
        }
        let texture = first
            .texture
            .as_any()
            .downcast_ref::<SurfaceTexture>()
            .ok_or(crate::SurfaceError::Other("unexpected surface texture"))?;
        let view_desc = crate::TextureViewDescriptor {
            label: None,
            format: wgt::TextureFormat::Bgra8Unorm,
            dimension: wgt::TextureViewDimension::D2,
            usage: wgt::TextureUses::COLOR_TARGET,
            range: wgt::ImageSubresourceRange::default(),
        };
        let view = unsafe {
            DynDevice::create_texture_view(
                &device,
                Borrow::<dyn crate::DynTexture>::borrow(texture),
                &view_desc,
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let native_view = view
            .as_any()
            .downcast_ref::<super::texture::NativeTextureView>()
            .ok_or(crate::SurfaceError::Other("missing HAL render-target view"))?;
        let frame = texture
            .frame
            .as_ref()
            .ok_or(crate::SurfaceError::Other("missing frame"))?;
        frame.clear([0., 0., 1., 1.]).map_err(surface_error)?;
        let before = frame.readback().map_err(surface_error)?;
        if before != [255, 0, 0, 255] {
            return Err(crate::SurfaceError::Other("unexpected initial frame pixel"));
        }
        let shader = unsafe {
            crate::Device::create_shader_module(
                &device,
                &crate::ShaderModuleDescriptor {
                    label: None,
                    runtime_checks: wgt::ShaderRuntimeChecks::unchecked(),
                },
                crate::ShaderInput::Naga(shader_input),
            )
        }
        .map_err(|_| crate::SurfaceError::Other("shader module failed"))?;
        let group_entries = [
            wgt::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgt::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            },
        ];
        let group_layout = unsafe {
            crate::Device::create_bind_group_layout(
                &device,
                &crate::BindGroupLayoutDescriptor {
                    label: None,
                    flags: crate::BindGroupLayoutFlags::empty(),
                    entries: &group_entries,
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let layout = unsafe {
            crate::Device::create_pipeline_layout(
                &device,
                &crate::PipelineLayoutDescriptor {
                    label: None,
                    flags: crate::PipelineLayoutFlags::empty(),
                    bind_group_layouts: &[Some(&group_layout)],
                    immediate_size: 0,
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
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
            crate::Device::create_render_pipeline(
                &device,
                &crate::RenderPipelineDescriptor {
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
                },
            )
        }
        .map_err(|_| crate::SurfaceError::Other("render pipeline failed"))?;
        let sampled = unsafe {
            crate::Device::create_texture(
                &device,
                &crate::TextureDescriptor {
                    label: None,
                    size: wgt::Extent3d {
                        width: 2,
                        height: 2,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgt::TextureDimension::D2,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    usage: wgt::TextureUses::RESOURCE | wgt::TextureUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                    view_formats: alloc::vec![],
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let sampled_view = unsafe {
            crate::Device::create_texture_view(
                &device,
                &sampled,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Rgba8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::RESOURCE,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let sampler = unsafe {
            crate::Device::create_sampler(
                &device,
                &crate::SamplerDescriptor {
                    label: None,
                    address_modes: [wgt::AddressMode::ClampToEdge; 3],
                    mag_filter: wgt::FilterMode::Nearest,
                    min_filter: wgt::FilterMode::Nearest,
                    mipmap_filter: wgt::MipmapFilterMode::Nearest,
                    lod_clamp: 0.0..f32::MAX,
                    compare: None,
                    anisotropy_clamp: 1,
                    border_color: None,
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let texture_bindings = [crate::TextureBinding {
            view: &sampled_view,
            usage: wgt::TextureUses::RESOURCE,
        }];
        let sampler_bindings = [&sampler];
        let entries = [
            crate::BindGroupEntry {
                binding: 0,
                resource_index: 0,
                count: 1,
            },
            crate::BindGroupEntry {
                binding: 1,
                resource_index: 0,
                count: 1,
            },
        ];
        let group = unsafe {
            crate::Device::create_bind_group(
                &device,
                &crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &[],
                    samplers: &sampler_bindings,
                    textures: &texture_bindings,
                    entries: &entries,
                    acceleration_structures: &[],
                    external_textures: &[],
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let upload = unsafe {
            crate::Device::create_buffer(
                &device,
                &crate::BufferDescriptor {
                    label: None,
                    size: 264,
                    usage: wgt::BufferUses::MAP_WRITE | wgt::BufferUses::COPY_SRC,
                    memory_flags: crate::MemoryFlags::empty(),
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let mapping = unsafe { crate::Device::map_buffer(&device, &upload, 0..264) }
            .map_err(crate::SurfaceError::Device)?;
        if mapping.is_coherent {
            return Err(crate::SurfaceError::Other("unexpected coherent mapping"));
        }
        unsafe {
            core::ptr::write_bytes(mapping.ptr.as_ptr(), 0, 264);
            core::ptr::copy_nonoverlapping(
                [0u8, 255, 0, 255, 0, 255, 0, 255].as_ptr(),
                mapping.ptr.as_ptr(),
                8,
            );
            core::ptr::copy_nonoverlapping(
                [0u8, 255, 0, 255, 0, 255, 0, 255].as_ptr(),
                mapping.ptr.as_ptr().add(256),
                8,
            );
            crate::Device::flush_mapped_ranges(&device, &upload, core::iter::once(0..264));
        }
        let copy = crate::BufferTextureCopy {
            buffer_layout: wgt::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
            texture_base: crate::TextureCopyBase {
                mip_level: 0,
                array_layer: 0,
                origin: wgt::Origin3d::ZERO,
                aspect: crate::FormatAspects::COLOR,
            },
            size: crate::CopyExtent {
                width: 2,
                height: 2,
                depth: 1,
            },
        };
        let vertices: [f32; 6] = [-1., -1., 3., -1., -1., 3.];
        let bytes: alloc::vec::Vec<u8> = vertices.iter().flat_map(|v| v.to_ne_bytes()).collect();
        let vertex = unsafe {
            crate::Device::create_buffer(
                &device,
                &crate::BufferDescriptor {
                    label: None,
                    size: bytes.len() as u64,
                    usage: wgt::BufferUses::VERTEX | wgt::BufferUses::COPY_DST,
                    memory_flags: crate::MemoryFlags::empty(),
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        native
            .write_byte_buffer(&vertex, 0, &bytes)
            .map_err(surface_error)?;
        let color = [Some(crate::ColorAttachment {
            target: crate::Attachment {
                view: native_view,
                usage: wgt::TextureUses::COLOR_TARGET,
            },
            depth_slice: None,
            resolve_target: None,
            ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
            clear_value: wgt::Color {
                r: 0.,
                g: 0.,
                b: 0.,
                a: 1.,
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
        let mut encoder = unsafe {
            crate::Device::create_command_encoder(
                &device,
                &crate::CommandEncoderDescriptor {
                    label: None,
                    queue: &native_queue,
                },
            )
        }
        .map_err(crate::SurfaceError::Device)?;
        let commands = unsafe {
            crate::CommandEncoder::begin_encoding(&mut encoder, None)
                .map_err(crate::SurfaceError::Device)?;
            crate::CommandEncoder::copy_buffer_to_texture(
                &mut encoder,
                &upload,
                &sampled,
                core::iter::once(copy),
            );
            crate::CommandEncoder::begin_render_pass(&mut encoder, &pass)
                .map_err(crate::SurfaceError::Device)?;
            crate::CommandEncoder::set_bind_group(&mut encoder, &layout, 0, &group, &[]);
            crate::CommandEncoder::set_render_pipeline(&mut encoder, &pipeline);
            crate::CommandEncoder::set_vertex_buffer(
                &mut encoder,
                0,
                crate::BufferBinding::new_unchecked(&vertex, 0, None),
            );
            crate::CommandEncoder::draw(&mut encoder, 0, 3, 0, 1);
            crate::CommandEncoder::end_render_pass(&mut encoder);
            crate::CommandEncoder::end_encoding(&mut encoder)
                .map_err(crate::SurfaceError::Device)?
        };
        unsafe {
            queue
                .submit(&[&commands], &[first.texture.as_ref()], (&fence, 1))
                .map_err(crate::SurfaceError::Device)?;
            queue.wait_for_idle().map_err(crate::SurfaceError::Device)?;
        }
        let pixel = frame.readback().map_err(surface_error)?;
        unsafe {
            crate::CommandEncoder::reset_all(&mut encoder, core::iter::once(commands));
            crate::Device::unmap_buffer(&device, &upload);
            DynDevice::destroy_texture_view(&device, view);
            queue.present(surface, first.texture)?;
            queue.wait_for_idle().map_err(crate::SurfaceError::Device)?;
        }
        let discarded = unsafe { surface.acquire_texture(None, &fence)? };
        unsafe {
            surface.discard_texture(discarded.texture);
        }
        config.extent.width = 320;
        config.extent.height = 240;
        unsafe {
            surface.configure(&device, &config)?;
        }
        let resized = unsafe { surface.acquire_texture(None, &fence)? };
        let frame = resized
            .texture
            .as_any()
            .downcast_ref::<SurfaceTexture>()
            .and_then(|texture| texture.frame.as_ref())
            .ok_or(crate::SurfaceError::Other(
                "unexpected resized surface texture",
            ))?;
        let resized_pixel = frame
            .clear_and_readback([0.0, 1.0, 0.0, 1.0])
            .map_err(surface_error)?;
        if pixel != resized_pixel {
            return Err(crate::SurfaceError::Other(
                "surface pixels changed across resize",
            ));
        }
        unsafe {
            queue
                .submit(&[], &[resized.texture.as_ref()], (&fence, 2))
                .map_err(crate::SurfaceError::Device)?;
        }
        unsafe {
            queue.present(surface, resized.texture)?;
            queue.wait_for_idle().map_err(crate::SurfaceError::Device)?;
        }
        unsafe {
            surface.unconfigure(&device);
        }
        Ok(pixel)
    }

    pub(super) fn present(
        &self,
        device: &Arc<NativeDevice>,
        mut texture: SurfaceTexture,
    ) -> Result<(), crate::SurfaceError> {
        let state = self.configured.lock();
        let configured = state
            .as_ref()
            .ok_or(crate::SurfaceError::Other("surface is not configured"))?;
        let frame = texture
            .frame
            .as_ref()
            .ok_or(crate::SurfaceError::Other("surface frame is not acquired"))?;
        if !Arc::ptr_eq(&configured.device, device) || !configured.swapchain.owns(frame) {
            return Err(crate::SurfaceError::Other(
                "surface frame or device does not match",
            ));
        }
        if texture
            .take_frame()
            .present_for_hal(1)
            .map_err(surface_error)?
        {
            Ok(())
        } else {
            Err(crate::SurfaceError::Occluded)
        }
    }
}

fn supported_config(config: &crate::SurfaceConfiguration) -> bool {
    config.format == wgt::TextureFormat::Bgra8Unorm
        && config.color_space == wgt::SurfaceColorSpace::Srgb
        && config.present_mode == wgt::PresentMode::Fifo
        && config.composite_alpha_mode == wgt::CompositeAlphaMode::Opaque
        && config.extent.width > 0
        && config.extent.height > 0
        && config.extent.depth_or_array_layers == 1
        && config.usage == wgt::TextureUses::COLOR_TARGET
        && config
            .view_formats
            .iter()
            .all(|format| *format == config.format)
        && config.maximum_frame_latency == 1
}

impl crate::Surface for Surface {
    type A = super::Api;
    unsafe fn configure(
        &self,
        device: &super::Device,
        config: &crate::SurfaceConfiguration,
    ) -> Result<(), crate::SurfaceError> {
        if !supported_config(config) {
            return Err(crate::SurfaceError::Other(
                "unsupported D3D11 surface configuration",
            ));
        }
        let mut state = self.configured.lock();
        if let Some(configured) = state.as_ref() {
            if !configured.swapchain.belongs_to(device.native()) {
                return Err(crate::SurfaceError::Other("surface device does not match"));
            }
            return configured
                .swapchain
                .resize(config.extent.width, config.extent.height)
                .map_err(surface_error);
        }
        let native = device.native().clone();
        let swapchain = CoreWindowSwapchain::new(
            self,
            native.clone(),
            config.extent.width,
            config.extent.height,
        )
        .map_err(surface_error)?;
        *state = Some(Configured {
            device: native,
            swapchain,
        });
        Ok(())
    }
    unsafe fn unconfigure(&self, device: &super::Device) {
        let mut state = self.configured.lock();
        if let Some(config) = state.as_ref() {
            if config.swapchain.belongs_to(device.native())
                && config.swapchain.release_bindings().is_ok()
            {
                *state = None;
                return;
            }
        }
        log::error!(
            "D3D11 surface cannot unconfigure with a mismatched device or outstanding frame"
        );
    }
    unsafe fn acquire_texture(
        &self,
        _timeout: Option<Duration>,
        fence: &super::fence::NativeFence,
    ) -> Result<crate::AcquiredSurfaceTexture<Self::A>, crate::SurfaceError> {
        let state = self.configured.lock();
        let configured = state
            .as_ref()
            .ok_or(crate::SurfaceError::Other("surface is not configured"))?;
        configured
            .device
            .validate_fence(fence)
            .map_err(surface_error)?;
        let frame = configured.swapchain.acquire().map_err(surface_error)?;
        let raw = frame
            .texture()
            .ok_or(crate::SurfaceError::Other("missing backbuffer"))?
            .clone();
        let texture = NativeTexture::from_core_window_backbuffer(&configured.device, raw)
            .map_err(surface_error)?;
        Ok(crate::AcquiredSurfaceTexture {
            texture: SurfaceTexture {
                texture: Some(texture),
                frame: Some(frame),
            },
            suboptimal: false,
        })
    }
    unsafe fn discard_texture(&self, texture: SurfaceTexture) {
        drop(texture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_config_rejects_unimplemented_modes() {
        let valid = crate::SurfaceConfiguration {
            maximum_frame_latency: 1,
            present_mode: wgt::PresentMode::Fifo,
            composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
            format: wgt::TextureFormat::Bgra8Unorm,
            color_space: wgt::SurfaceColorSpace::Srgb,
            extent: wgt::Extent3d {
                width: 320,
                height: 240,
                depth_or_array_layers: 1,
            },
            usage: wgt::TextureUses::COLOR_TARGET,
            view_formats: alloc::vec::Vec::new(),
        };
        assert!(supported_config(&valid));
        let mut bad = valid.clone();
        bad.format = wgt::TextureFormat::Rgba8Unorm;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.present_mode = wgt::PresentMode::Immediate;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.composite_alpha_mode = wgt::CompositeAlphaMode::PreMultiplied;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.usage |= wgt::TextureUses::COPY_SRC;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.extent.width = 0;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.maximum_frame_latency = 2;
        assert!(!supported_config(&bad));
        bad = valid.clone();
        bad.view_formats.push(wgt::TextureFormat::Bgra8UnormSrgb);
        assert!(!supported_config(&bad));
    }
}
