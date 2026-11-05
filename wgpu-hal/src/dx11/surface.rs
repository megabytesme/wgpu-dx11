//! Private window surface with exclusive backbuffer ownership.
use super::{
    device::NativeDevice,
    swapchain::{CoreWindowFrame, CoreWindowSwapchain},
    texture::NativeTexture,
};
use alloc::{format, sync::Arc};
use core::{borrow::Borrow, ffi::c_void, num::NonZeroIsize, time::Duration};
use parking_lot::{Condvar, Mutex};
use windows::{
    core::{AgileReference, IUnknown, Interface},
    UI::Core::ICoreWindow,
};

#[derive(Clone, Debug)]
struct Configured {
    device: Arc<NativeDevice>,
    swapchain: CoreWindowSwapchain,
}

#[derive(Debug, Default)]
struct SurfaceState {
    current: Option<Configured>,
    busy: bool,
}

impl SurfaceState {
    fn as_ref(&self) -> Option<&Configured> {
        self.current.as_ref()
    }
}

struct SurfaceOperation<'a>(&'a Surface);

impl Drop for SurfaceOperation<'_> {
    fn drop(&mut self) {
        self.0.configured.lock().busy = false;
        self.0.operation_complete.notify_one();
    }
}

#[derive(Debug)]
enum Window {
    Core(AgileReference<ICoreWindow>),
    Hwnd(NonZeroIsize),
}

/// An experimental CoreWindow or borrowed HWND surface.
#[derive(Debug)]
pub struct Surface {
    window: Window,
    configured: Mutex<SurfaceState>,
    operation_complete: Condvar,
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
    log::error!("D3D11 surface error: {error}");
    match super::hal_device::classify_device_error(error.code()) {
        error @ (crate::DeviceError::OutOfMemory | crate::DeviceError::Lost) => {
            crate::SurfaceError::Device(error)
        }
        crate::DeviceError::Unexpected => {
            crate::SurfaceError::Other("D3D11 window swapchain operation failed")
        }
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
            window: Window::Core(window),
            configured: Mutex::new(SurfaceState::default()),
            operation_complete: Condvar::new(),
        })
    }
    pub(super) fn from_hwnd(hwnd: NonZeroIsize) -> Self {
        Self {
            window: Window::Hwnd(hwnd),
            configured: Mutex::new(SurfaceState::default()),
            operation_complete: Condvar::new(),
        }
    }
    pub(super) fn hwnd(&self) -> Option<windows::Win32::Foundation::HWND> {
        match self.window {
            Window::Hwnd(hwnd) => Some(windows::Win32::Foundation::HWND(hwnd.get() as *mut _)),
            Window::Core(_) => None,
        }
    }
    pub(super) fn supports_public_presentation(&self) -> bool {
        self.hwnd().is_some_and(|hwnd| unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, None) != 0
        })
    }
    fn is_window_thread(&self) -> bool {
        match self.hwnd() {
            Some(hwnd) => unsafe {
                windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId(hwnd, None)
                    == windows::Win32::System::Threading::GetCurrentThreadId()
            },
            None => true,
        }
    }
    pub(super) fn core_window(&self) -> windows::core::Result<ICoreWindow> {
        match &self.window {
            Window::Core(window) => window.resolve(),
            Window::Hwnd(_) => Err(windows::core::Error::from_hresult(
                windows::Win32::Foundation::E_INVALIDARG,
            )),
        }
    }

    fn begin_operation(&self) -> Result<SurfaceOperation<'_>, crate::SurfaceError> {
        let mut state = self.configured.lock();
        if state.busy {
            return Err(crate::SurfaceError::Other(
                "D3D11 surface operation already in progress",
            ));
        }
        state.busy = true;
        Ok(SurfaceOperation(self))
    }

    fn wait_for_operation(&self) -> SurfaceOperation<'_> {
        let mut state = self.configured.lock();
        while state.busy {
            self.operation_complete.wait(&mut state);
        }
        state.busy = true;
        SurfaceOperation(self)
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
        unsafe { self.diagnostic_cycle_inner(native, shader_input, false) }
    }

    /// Exercise a two-texture overlay pipeline through the private HAL surface.
    ///
    /// # Safety
    /// The surface must be unconfigured, have no acquired textures or derived
    /// views, and have no outstanding GPU work.
    #[doc(hidden)]
    pub unsafe fn diagnostic_overlay_cycle(
        &self,
        native: Arc<NativeDevice>,
        shader_input: crate::NagaShader,
    ) -> Result<[u8; 4], crate::SurfaceError> {
        unsafe { self.diagnostic_cycle_inner(native, shader_input, true) }
    }

    unsafe fn diagnostic_cycle_inner(
        &self,
        native: Arc<NativeDevice>,
        shader_input: crate::NagaShader,
        overlay: bool,
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
            let swapchain = self
                .configured
                .lock()
                .as_ref()
                .expect("configured")
                .swapchain
                .clone();
            swapchain.verify_texture_array().map_err(surface_error)?;
        }
        let first = unsafe { surface.acquire_texture(None, &fence)? };
        {
            let swapchain = self
                .configured
                .lock()
                .as_ref()
                .expect("configured")
                .swapchain
                .clone();
            if swapchain.acquire().is_ok() {
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
                    runtime_checks: if overlay {
                        wgt::ShaderRuntimeChecks::checked()
                    } else {
                        wgt::ShaderRuntimeChecks::unchecked()
                    },
                },
                crate::ShaderInput::Naga(shader_input),
            )
        }
        .map_err(|_| crate::SurfaceError::Other("shader module failed"))?;
        let mut group_entries = alloc::vec::Vec::new();
        if overlay {
            group_entries.push(wgt::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgt::ShaderStages::VERTEX | wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Buffer {
                    ty: wgt::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            });
        }
        let pair_count = if overlay { 2 } else { 1 };
        for index in 0..pair_count {
            let binding = u32::from(overlay) + index * 2;
            group_entries.push(wgt::BindGroupLayoutEntry {
                binding,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Texture {
                    sample_type: wgt::TextureSampleType::Float { filterable: true },
                    view_dimension: wgt::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            });
            group_entries.push(wgt::BindGroupLayoutEntry {
                binding: binding + 1,
                visibility: wgt::ShaderStages::FRAGMENT,
                ty: wgt::BindingType::Sampler(wgt::SamplerBindingType::Filtering),
                count: None,
            });
        }
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
        let mut attributes = alloc::vec![wgt::VertexAttribute {
            format: wgt::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        }];
        if overlay {
            attributes.extend([
                wgt::VertexAttribute {
                    format: wgt::VertexFormat::Float32x2,
                    offset: 8,
                    shader_location: 1,
                },
                wgt::VertexAttribute {
                    format: wgt::VertexFormat::Unorm8x4,
                    offset: 16,
                    shader_location: 2,
                },
                wgt::VertexAttribute {
                    format: wgt::VertexFormat::Float32,
                    offset: 20,
                    shader_location: 3,
                },
                wgt::VertexAttribute {
                    format: wgt::VertexFormat::Float32x2,
                    offset: 24,
                    shader_location: 4,
                },
            ]);
        }
        let buffers = [Some(crate::VertexBufferLayout {
            array_stride: if overlay { 32 } else { 8 },
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
        let mut target: wgt::ColorTargetState = wgt::TextureFormat::Bgra8Unorm.into();
        if overlay {
            target.blend = Some(wgt::BlendState::ALPHA_BLENDING);
        }
        let targets = [Some(target)];
        let pipeline = unsafe {
            crate::Device::create_render_pipeline(
                &device,
                &crate::RenderPipelineDescriptor {
                    label: None,
                    layout: &layout,
                    vertex_processor: crate::VertexProcessor::Standard {
                        vertex_buffers: &buffers,
                        vertex_stage: stage(if overlay { "vs_main" } else { "vs" }),
                    },
                    primitive: wgt::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgt::MultisampleState::default(),
                    fragment_stage: Some(stage(if overlay { "fs_main" } else { "fs" })),
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
        let uniform = if overlay {
            let buffer = unsafe {
                crate::Device::create_buffer(
                    &device,
                    &crate::BufferDescriptor {
                        label: None,
                        size: 64,
                        usage: wgt::BufferUses::UNIFORM | wgt::BufferUses::COPY_DST,
                        memory_flags: crate::MemoryFlags::empty(),
                    },
                )
            }
            .map_err(crate::SurfaceError::Device)?;
            let mut bytes = alloc::vec![0u8; 64];
            for (index, value) in [640f32, 480.0, 1.0, 0.0].iter().enumerate() {
                bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_ne_bytes());
            }
            native
                .write_byte_buffer(&buffer, 0, &bytes)
                .map_err(surface_error)?;
            Some(buffer)
        } else {
            None
        };
        let buffer_bindings: alloc::vec::Vec<_> = uniform
            .as_ref()
            .map(|buffer| crate::BufferBinding::new_unchecked(buffer, 0, None))
            .into_iter()
            .collect();
        let texture_bindings: alloc::vec::Vec<_> = (0..pair_count)
            .map(|_| crate::TextureBinding {
                view: &sampled_view,
                usage: wgt::TextureUses::RESOURCE,
            })
            .collect();
        let sampler_bindings: alloc::vec::Vec<_> = (0..pair_count).map(|_| &sampler).collect();
        let mut entries = alloc::vec::Vec::new();
        if overlay {
            entries.push(crate::BindGroupEntry {
                binding: 0,
                resource_index: 0,
                count: 1,
            });
        }
        for index in 0..pair_count {
            let binding = u32::from(overlay) + index * 2;
            entries.push(crate::BindGroupEntry {
                binding,
                resource_index: index,
                count: 1,
            });
            entries.push(crate::BindGroupEntry {
                binding: binding + 1,
                resource_index: index,
                count: 1,
            });
        }
        let group = unsafe {
            crate::Device::create_bind_group(
                &device,
                &crate::BindGroupDescriptor {
                    label: None,
                    layout: &group_layout,
                    buffers: &buffer_bindings,
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
            crate::Device::flush_mapped_ranges(&device, &upload, core::iter::once(0..264)).unwrap();
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
        let mut bytes = alloc::vec::Vec::new();
        let vertices = if overlay {
            [0f32, 480., 1280., 480., 0., -480.]
        } else {
            [-1f32, -1., 3., -1., -1., 3.]
        };
        for position in vertices.chunks_exact(2) {
            bytes.extend(position.iter().flat_map(|value| value.to_ne_bytes()));
            if overlay {
                bytes.extend([0.5f32, 0.5].iter().flat_map(|value| value.to_ne_bytes()));
                bytes.extend_from_slice(&[255, 255, 255, 255]);
                bytes.extend_from_slice(&0.5f32.to_ne_bytes());
                bytes.extend([0.5f32, 0.5].iter().flat_map(|value| value.to_ne_bytes()));
            }
        }
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
        if !self.is_window_thread() {
            return Err(crate::SurfaceError::Other(
                "D3D11 HWND presentation must run on the window thread",
            ));
        }
        let _operation = self.begin_operation()?;
        let configured = self
            .configured
            .lock()
            .current
            .clone()
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

    fn is_present_on_current_thread(&self) -> bool {
        self.is_window_thread()
    }

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
        if !self.is_window_thread() {
            return Err(crate::SurfaceError::Other(
                "D3D11 HWND configuration must run on the window thread",
            ));
        }
        let _operation = self.begin_operation()?;
        let configured = self.configured.lock().current.clone();
        if let Some(configured) = configured {
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
        self.configured.lock().current = Some(Configured {
            device: native,
            swapchain,
        });
        Ok(())
    }
    unsafe fn unconfigure(&self, device: &super::Device) {
        let _operation = self.wait_for_operation();
        let configured = self.configured.lock().current.clone();
        if let Some(config) = configured {
            if config.swapchain.belongs_to(device.native())
                && config.swapchain.release_bindings().is_ok()
            {
                let old = self.configured.lock().current.take();
                drop(old);
                drop(config);
                unsafe { device.native().immediate_context().Flush() };
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
        if !self.is_window_thread() {
            return Err(crate::SurfaceError::Other(
                "D3D11 HWND acquisition must run on the window thread",
            ));
        }
        let _operation = self.begin_operation()?;
        let configured = self
            .configured
            .lock()
            .current
            .clone()
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
#[path = "../../tests/dx11/hwnd_surface.rs"]
mod hwnd_surface_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Surface as _;

    #[test]
    fn hwnd_present_thread_check_is_owner_thread_only() {
        use windows::{
            core::w,
            Win32::{
                Foundation::HWND,
                UI::WindowsAndMessaging::{
                    CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
                },
            },
        };

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("wgpu D3D11 owner-thread test"),
                WS_OVERLAPPED,
                0,
                0,
                16,
                16,
                None,
                None,
                None,
                None,
            )
        }
        .expect("create hidden test HWND");
        struct WindowGuard(HWND);
        impl Drop for WindowGuard {
            fn drop(&mut self) {
                unsafe {
                    let _ = DestroyWindow(self.0);
                }
            }
        }
        let _window = WindowGuard(hwnd);
        let surface = Arc::new(Surface::from_hwnd(
            NonZeroIsize::new(hwnd.0 as isize).expect("non-null HWND"),
        ));
        assert!(surface.is_present_on_current_thread());
        let worker_surface = surface.clone();
        assert!(
            !std::thread::spawn(move || worker_surface.is_present_on_current_thread())
                .join()
                .expect("owner-thread check worker")
        );
    }

    #[test]
    fn driver_internal_error_marks_surface_device_lost() {
        let error = windows::core::Error::from_hresult(
            windows::Win32::Graphics::Dxgi::DXGI_ERROR_DRIVER_INTERNAL_ERROR,
        );
        assert!(matches!(
            surface_error(error),
            crate::SurfaceError::Device(crate::DeviceError::Lost)
        ));
    }

    #[test]
    fn surface_operations_fail_fast_and_release_on_unwind() {
        let surface = Surface::from_hwnd(NonZeroIsize::new(1).unwrap());
        let operation = surface.begin_operation().unwrap();
        assert!(surface.begin_operation().is_err());
        drop(operation);
        assert!(surface.begin_operation().is_ok());
        let result = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            let _operation = surface.begin_operation().unwrap();
            panic!("injected failure");
        }));
        assert!(result.is_err());
        assert!(surface.begin_operation().is_ok());
    }

    #[test]
    fn unconfigure_waits_for_an_active_operation() {
        let surface = Surface::from_hwnd(NonZeroIsize::new(1).unwrap());
        let operation = surface.begin_operation().unwrap();
        std::thread::scope(|scope| {
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (finished_tx, finished_rx) = std::sync::mpsc::channel();
            let surface = &surface;
            scope.spawn(move || {
                started_tx.send(()).unwrap();
                let _operation = surface.wait_for_operation();
                finished_tx.send(()).unwrap();
            });
            started_rx.recv().unwrap();
            assert!(finished_rx.recv_timeout(Duration::from_millis(50)).is_err());
            drop(operation);
            finished_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        assert!(surface.begin_operation().is_ok());
    }

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
