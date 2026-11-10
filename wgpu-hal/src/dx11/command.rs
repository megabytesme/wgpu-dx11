//! Recorded copy commands and a restricted FL9_1 render path.
use super::{
    buffer::{NativeBuffer, NativeBufferClear, NativeBufferCopy},
    device::NativeDevice,
    hal_index::IndexBinding,
    hal_transfer::{BufferToTexture, BufferToTexture3D, TextureToBuffer, TextureToTexture},
    hal_uniform::UniformBinding,
};
use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::{
            Direct3D::{
                D3D_PRIMITIVE_TOPOLOGY_LINELIST, D3D_PRIMITIVE_TOPOLOGY_LINESTRIP,
                D3D_PRIMITIVE_TOPOLOGY_POINTLIST, D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
                D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP,
            },
            Direct3D11::{
                ID3D11Asynchronous, ID3D11Buffer, ID3D11DepthStencilState, ID3D11DepthStencilView,
                ID3D11Device, ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader,
                ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11Resource, ID3D11SamplerState,
                ID3D11ShaderResourceView, ID3D11VertexShader, D3D11_BIND_VERTEX_BUFFER, D3D11_BOX,
                D3D11_BUFFER_DESC, D3D11_CLEAR_DEPTH, D3D11_CLEAR_STENCIL, D3D11_SUBRESOURCE_DATA,
                D3D11_USAGE_IMMUTABLE, D3D11_VIEWPORT,
            },
            Dxgi::Common::DXGI_FORMAT,
        },
    },
};

/// Records validated copy commands without submitting GPU work.
#[allow(dead_code)]
pub(crate) struct CopyEncoder<'a> {
    device: &'a NativeDevice,
    copies: Vec<NativeBufferCopy>,
}

#[derive(Debug)]
pub(super) struct DepthAttachment {
    pub view: ID3D11DepthStencilView,
    pub format: wgt::TextureFormat,
    pub clear_depth: Option<f32>,
    pub clear_stencil: Option<u8>,
}

#[derive(Debug)]
pub(super) struct ResolveOperation {
    pub source: ID3D11Resource,
    pub destination: ID3D11Resource,
    pub source_subresource: u32,
    pub destination_subresource: u32,
    pub format: DXGI_FORMAT,
}

#[derive(Debug)]
pub(super) enum RenderTarget {
    Owned(ID3D11RenderTargetView),
    Surface(Weak<ID3D11RenderTargetView>),
}

#[derive(Debug)]
pub(super) struct InstanceBindings {
    pub first: u32,
    pub streams: Vec<(u32, ID3D11Buffer, u32, u32)>,
    pub instance_index: Option<(ID3D11Buffer, [f32; 4])>,
}

impl InstanceBindings {
    pub(super) fn validate(&self, instances: u32) -> Result<()> {
        if let Some(last) = instances.checked_sub(1) {
            if self.instance_index.is_some() && self.first.checked_add(last).is_none() {
                return Err(Error::from_hresult(E_INVALIDARG));
            }
            for (_, _, offset, stride) in &self.streams {
                instance_vertex_offset(*offset, *stride, self.first, last)
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            }
        }
        Ok(())
    }

    pub(super) fn bind(&self, context: &ID3D11DeviceContext, instance: u32) -> Result<()> {
        for (slot, buffer, offset, stride) in &self.streams {
            let absolute = instance_vertex_offset(*offset, *stride, self.first, instance)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            unsafe {
                context.IASetVertexBuffers(
                    *slot,
                    1,
                    Some(&Some(buffer.clone())),
                    Some(&0),
                    Some(&absolute),
                );
            }
        }
        if let Some((buffer, transform)) = &self.instance_index {
            let index = self
                .first
                .checked_add(instance)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            let mut constants = [0u32; 8];
            for (destination, value) in constants[..4].iter_mut().zip(transform) {
                *destination = value.to_bits();
            }
            constants[4] = index;
            unsafe {
                context.UpdateSubresource(buffer, 0, None, constants.as_ptr().cast(), 0, 0);
                context.VSSetConstantBuffers(13, Some(&[Some(buffer.clone())]));
            }
        }
        Ok(())
    }
}

pub(super) struct VertexBufferRestore<'a> {
    context: &'a ID3D11DeviceContext,
    bindings: Vec<(u32, ID3D11Buffer, u32, u32)>,
}

impl<'a> VertexBufferRestore<'a> {
    pub(super) fn new(context: &'a ID3D11DeviceContext) -> Self {
        Self {
            context,
            bindings: Vec::new(),
        }
    }

    pub(super) fn add(
        &mut self,
        slot: u32,
        buffer: ID3D11Buffer,
        stride: u32,
        offset: u32,
    ) -> Result<()> {
        if self
            .bindings
            .iter()
            .any(|(bound_slot, ..)| *bound_slot == slot)
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        self.bindings
            .try_reserve(1)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        self.bindings.push((slot, buffer, stride, offset));
        Ok(())
    }
}

impl Drop for VertexBufferRestore<'_> {
    fn drop(&mut self) {
        unsafe {
            for (slot, buffer, stride, offset) in &self.bindings {
                self.context.IASetVertexBuffers(
                    *slot,
                    1,
                    Some(&Some(buffer.clone())),
                    Some(stride),
                    Some(offset),
                );
            }
        }
    }
}

pub(super) fn instance_vertex_offset(
    offset: u32,
    stride: u32,
    first: u32,
    instance: u32,
) -> Option<u32> {
    let index = u64::from(first) + u64::from(instance);
    let offset = u64::from(stride)
        .checked_mul(index)?
        .checked_add(u64::from(offset))?;
    u32::try_from(offset).ok()
}

impl RenderTarget {
    fn resolve(&self) -> Result<ID3D11RenderTargetView> {
        match self {
            Self::Owned(target) => Ok(target.clone()),
            Self::Surface(target) => target
                .upgrade()
                .map(|target| (*target).clone())
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG)),
        }
    }
}

#[derive(Debug)]
pub(super) enum NativeCommand {
    Copy(NativeBufferCopy),
    ClearBuffer(NativeBufferClear),
    WriteTimestamp(ID3D11Asynchronous),
    CopyQueryResults {
        queries: Vec<ID3D11Asynchronous>,
        buffer: ID3D11Buffer,
        offset: u32,
    },
    BufferToTexture(BufferToTexture),
    BufferToTexture3D(BufferToTexture3D),
    TextureToBuffer(TextureToBuffer),
    TextureToTexture(TextureToTexture),
    #[cfg(test)]
    InjectedFailure,
    #[cfg(feature = "dx11-test-fixture")]
    InjectedDeviceRemoval,
    BeginRender {
        target: Option<RenderTarget>,
        clear: Option<[f32; 4]>,
        depth: Option<DepthAttachment>,
        viewport_buffer: ID3D11Buffer,
        width: u32,
        height: u32,
    },
    SetPipeline {
        vertex: ID3D11VertexShader,
        fragment: ID3D11PixelShader,
        input: Option<ID3D11InputLayout>,
        topology: wgt::PrimitiveTopology,
        rasterizer: ID3D11RasterizerState,
        blend: Option<windows::Win32::Graphics::Direct3D11::ID3D11BlendState>,
        sample_mask: u32,
        depth_state: Option<ID3D11DepthStencilState>,
        vertex_index: Option<(u32, ID3D11Buffer)>,
    },
    SetBlendConstant([f32; 4]),
    SetVertexShader(ID3D11VertexShader),
    SetUniformGroup {
        uniform: Arc<UniformBinding>,
        slot: u32,
        visibility: wgt::ShaderStages,
        source_box: D3D11_BOX,
    },
    RefreshSampledTextures {
        textures: Vec<Arc<super::texture::SampledRefresh>>,
    },
    SetTexture {
        slot: u32,
        texture: ID3D11ShaderResourceView,
    },
    SetSampler {
        slot: u32,
        sampler: ID3D11SamplerState,
    },
    SetTexelLoad {
        texture: ID3D11ShaderResourceView,
        sampler: ID3D11SamplerState,
        dimensions: ID3D11Buffer,
        kind: super::hal_shader::Fl9TextureEmulationKind,
        has_implicit_sample: bool,
        width: u32,
        height: u32,
        depth: u32,
        mip_levels: u32,
    },
    SetVertex {
        slot: u32,
        buffer: ID3D11Buffer,
        offset: u32,
        stride: u32,
    },
    SetViewport {
        viewport: D3D11_VIEWPORT,
        buffer: ID3D11Buffer,
        transform: [f32; 4],
    },
    SetScissor(windows::Win32::Foundation::RECT),
    SetStencilReference(u32),
    Draw {
        first: u32,
        count: u32,
        instances: u32,
        max_vertices: u32,
        zero_input: bool,
        vertex_index: Option<VertexIndexDraw>,
        instance_bindings: InstanceBindings,
    },
    DrawPoints {
        vertices: Vec<(u32, ID3D11Buffer, u32, u32)>,
        first_vertex: u32,
        count: u32,
        instances: u32,
        max_points: u32,
        max_vertex_index: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instance_bindings: InstanceBindings,
    },
    DrawLines {
        vertices: Vec<(u32, ID3D11Buffer, u32, u32)>,
        first_vertex: u32,
        count: u32,
        max_vertices: u32,
        max_vertex_index: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    DrawLineStrip {
        vertices: Vec<(u32, ID3D11Buffer, u32, u32)>,
        first_vertex: u32,
        count: u32,
        max_vertices: u32,
        max_vertex_index: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    DrawStrip {
        vertices: Vec<(u32, ID3D11Buffer, u32, u32)>,
        first_vertex: u32,
        count: u32,
        max_vertex_index: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    DrawIndexed {
        binding: Arc<IndexBinding>,
        vertex: ID3D11Buffer,
        vertex_offset: u32,
        first: u32,
        count: u32,
        base_vertex: i32,
        vertex_bytes: u64,
        stride: u32,
        extent: u32,
        max_vertices: u32,
        instances: u32,
        instance_bindings: InstanceBindings,
        vertex_index: Option<(u32, ID3D11Buffer)>,
    },
    DrawIndexedMulti {
        binding: Arc<IndexBinding>,
        vertices: Vec<(u32, ID3D11Buffer, u32, u64, u32, u32)>,
        first: u32,
        count: u32,
        base_vertex: i32,
        max_vertices: u32,
        instances: u32,
        instance_bindings: InstanceBindings,
        vertex_index: Option<(u32, ID3D11Buffer)>,
    },
    DrawIndexedPoints {
        binding: Arc<IndexBinding>,
        vertices: Vec<(u32, ID3D11Buffer, u32, u64, u32, u32)>,
        first: u32,
        count: u32,
        base_vertex: i32,
        max_points: u32,
        max_vertex_index: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    DrawIndexedLines {
        binding: Arc<IndexBinding>,
        vertices: Vec<(u32, ID3D11Buffer, u32, u64, u32, u32)>,
        first: u32,
        count: u32,
        base_vertex: i32,
        max_indices: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    DrawIndexedLineStrips {
        binding: Arc<IndexBinding>,
        vertices: Vec<(u32, ID3D11Buffer, u32, u64, u32, u32)>,
        first: u32,
        count: u32,
        base_vertex: i32,
        max_indices: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    DrawIndexedTriangleStrips {
        binding: Arc<IndexBinding>,
        vertices: Vec<(u32, ID3D11Buffer, u32, u64, u32, u32)>,
        first: u32,
        count: u32,
        base_vertex: i32,
        max_vertices: u32,
        vertex_index: Option<(u32, ID3D11Buffer)>,
        instances: u32,
        instance_bindings: InstanceBindings,
    },
    EndRender {
        resolve: Option<ResolveOperation>,
    },
}

#[derive(Debug)]
pub(super) struct VertexIndexDraw {
    pub slot: u32,
    pub identity: ID3D11Buffer,
    pub max_vertex_index: u32,
    pub vertices: Vec<(u32, ID3D11Buffer, u32, u32)>,
}

pub(super) fn draw_batches(
    first: u32,
    count: u32,
    instances: u32,
    max_vertices: u32,
    mut draw: impl FnMut(u32, u32),
) -> Result<()> {
    if max_vertices == 0
        || !max_vertices.is_multiple_of(3)
        || (count != 0 && first.checked_add(count - 1).is_none())
    {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    for _ in 0..instances {
        let mut next = first;
        let mut remaining = count;
        while remaining != 0 {
            let batch = remaining.min(max_vertices);
            draw(next, batch);
            remaining -= batch;
            if remaining != 0 {
                next = next
                    .checked_add(batch)
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            }
        }
    }
    Ok(())
}

fn draw_start_vertex(first: u32, zero_input: bool) -> u32 {
    if zero_input {
        0
    } else {
        first
    }
}

fn point_batches(
    count: u32,
    max_points: u32,
    mut draw: impl FnMut(u32, u32) -> Result<()>,
) -> Result<()> {
    if max_points == 0 {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let mut first = 0;
    let mut remaining = count;
    while remaining != 0 {
        let batch = remaining.min(max_points);
        draw(first, batch)?;
        remaining -= batch;
        if remaining != 0 {
            first = first
                .checked_add(batch)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        }
    }
    Ok(())
}

fn line_list_batches(
    count: u32,
    max_vertices: u32,
    instances: u32,
    mut draw: impl FnMut(u32, u32) -> Result<()>,
) -> Result<()> {
    if max_vertices < 2 || !max_vertices.is_multiple_of(2) {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    for _ in 0..instances {
        let mut first = 0;
        let mut remaining = count & !1;
        while remaining != 0 {
            let batch = remaining.min(max_vertices);
            draw(first, batch)?;
            remaining -= batch;
            if remaining != 0 {
                first = first
                    .checked_add(batch)
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            }
        }
    }
    Ok(())
}

fn line_strip_batches(
    count: u32,
    max_vertices: u32,
    instances: u32,
    mut draw: impl FnMut(u32, u32) -> Result<()>,
) -> Result<()> {
    if max_vertices < 2 {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    for _ in 0..instances {
        let mut first = 0;
        let mut remaining = count;
        while remaining >= 2 {
            let batch = remaining.min(max_vertices);
            draw(first, batch)?;
            if batch == remaining {
                break;
            }
            let step = batch - 1;
            first = first
                .checked_add(step)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            remaining -= step;
        }
    }
    Ok(())
}

pub(super) fn strip_vertex_offset(offset: u32, stride: u32, first: u32) -> Result<u32> {
    first
        .checked_mul(stride)
        .and_then(|value| value.checked_add(offset))
        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))
}

fn strip_batches(
    count: u32,
    instances: u32,
    mut draw: impl FnMut(u32, u32) -> Result<()>,
) -> Result<()> {
    const MAX_VERTICES: u32 = 65_534;
    for _ in 0..instances {
        let mut first = 0;
        let mut remaining = count;
        while remaining >= 3 {
            let batch = remaining.min(MAX_VERTICES);
            draw(first, batch)?;
            if batch == remaining {
                break;
            }
            let step = batch - 2;
            first = first
                .checked_add(step)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            remaining -= step;
        }
    }
    Ok(())
}

fn direct_strip_vertex_indices(first: u32, count: u32) -> Result<Vec<u64>> {
    let mut indices = Vec::new();
    indices
        .try_reserve_exact(count as usize)
        .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
    for relative in 0..count {
        indices.push(u64::from(
            first
                .checked_add(relative)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
        ));
    }
    Ok(indices)
}

/// Owned commands retain their D3D11 resources until the command buffer drops.
#[allow(dead_code)]
#[derive(Debug)]
#[doc(hidden)]
pub struct CommandBuffer {
    owner: ID3D11Device,
    commands: Vec<NativeCommand>,
}

impl CommandBuffer {
    pub(super) fn from_commands(device: &NativeDevice, commands: Vec<NativeCommand>) -> Self {
        Self {
            owner: device.raw_device().clone(),
            commands,
        }
    }

    pub(super) fn validate_device(&self, device: &NativeDevice) -> Result<()> {
        if device.raw_device().as_raw() != self.owner.as_raw() {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        Ok(())
    }

    pub(super) fn retain_surface_targets(
        &self,
        retained: &mut Vec<Arc<ID3D11RenderTargetView>>,
    ) -> Result<()> {
        for command in &self.commands {
            if let NativeCommand::BeginRender {
                target: Some(RenderTarget::Surface(target)),
                ..
            } = command
            {
                let target = target
                    .upgrade()
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                retained
                    .try_reserve(1)
                    .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                retained.push(target);
            }
        }
        Ok(())
    }

    /// The caller must hold the owning device's immediate-context mutex.
    pub(super) fn replay(&self, context: &ID3D11DeviceContext) -> Result<()> {
        let mut active_depth_state = None;
        let mut active_blend_state = None;
        let mut active_sample_mask = u32::MAX;
        let mut blend_constant = [1.0; 4];
        let mut stencil_reference = 0;
        let mut active_vertex_index_slot = None;
        for command in &self.commands {
            unsafe {
                match command {
                    NativeCommand::Copy(copy) => copy.execute(context),
                    NativeCommand::ClearBuffer(clear) => {
                        // A fixed static chunk avoids allocating proportional to buffer size.
                        static ZEROES: [u8; 4096] = [0; 4096];
                        let mut start = clear.start;
                        while start < clear.end {
                            let end = clear.end.min(start.saturating_add(ZEROES.len() as u32));
                            let region = D3D11_BOX {
                                left: start,
                                right: end,
                                top: 0,
                                bottom: 1,
                                front: 0,
                                back: 1,
                            };
                            context.UpdateSubresource(
                                &clear.buffer,
                                0,
                                Some(&region),
                                ZEROES.as_ptr().cast(),
                                0,
                                0,
                            );
                            start = end;
                        }
                    }
                    NativeCommand::WriteTimestamp(query) => context.End(query),
                    NativeCommand::CopyQueryResults {
                        queries,
                        buffer,
                        offset,
                    } => {
                        context.Flush();
                        for (index, query) in queries.iter().enumerate() {
                            let mut value = 0u64;
                            super::query::wait_for_query_data(context, query, &mut value)?;
                            let byte_offset = offset
                                .checked_add(
                                    u32::try_from(index)
                                        .ok()
                                        .and_then(|index| index.checked_mul(wgt::QUERY_SIZE))
                                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
                                )
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            let region = D3D11_BOX {
                                left: byte_offset,
                                right: byte_offset
                                    .checked_add(wgt::QUERY_SIZE)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
                                top: 0,
                                bottom: 1,
                                front: 0,
                                back: 1,
                            };
                            context.UpdateSubresource(
                                buffer,
                                0,
                                Some(&region),
                                core::ptr::from_ref(&value).cast(),
                                0,
                                0,
                            );
                        }
                    }

                    NativeCommand::BufferToTexture(copy) => copy.execute(context)?,
                    NativeCommand::BufferToTexture3D(copy) => copy.execute(context)?,
                    NativeCommand::TextureToBuffer(copy) => copy.execute(context)?,
                    NativeCommand::TextureToTexture(copy) => copy.execute(context),
                    #[cfg(test)]
                    NativeCommand::InjectedFailure => {
                        return Err(Error::from_hresult(E_FAIL));
                    }
                    #[cfg(feature = "dx11-test-fixture")]
                    NativeCommand::InjectedDeviceRemoval => {
                        return Err(Error::from_hresult(
                            windows::Win32::Graphics::Dxgi::DXGI_ERROR_DEVICE_REMOVED,
                        ));
                    }
                    NativeCommand::BeginRender {
                        target,
                        clear,
                        depth,
                        viewport_buffer,
                        width,
                        height,
                    } => {
                        let target = target.as_ref().map(RenderTarget::resolve).transpose()?;
                        active_depth_state = None;
                        active_blend_state = None;
                        active_sample_mask = u32::MAX;
                        blend_constant = [1.0; 4];
                        stencil_reference = 0;
                        context.ClearState();
                        active_vertex_index_slot = None;
                        if let Some(target) = &target {
                            context.OMSetRenderTargets(
                                Some(&[Some(target.clone())]),
                                depth.as_ref().map(|attachment| &attachment.view),
                            );
                        } else {
                            context.OMSetRenderTargets(
                                None,
                                depth.as_ref().map(|attachment| &attachment.view),
                            );
                        }
                        context.OMSetBlendState(None, None, u32::MAX);
                        context.OMSetDepthStencilState(None, stencil_reference);
                        let identity: [f32; 4] = [1., 1., 0., 0.];
                        let mut constants = [0u32; 8];
                        for (destination, value) in constants[..4].iter_mut().zip(identity) {
                            *destination = value.to_bits();
                        }
                        context.UpdateSubresource(
                            viewport_buffer,
                            0,
                            None,
                            constants.as_ptr().cast(),
                            0,
                            0,
                        );
                        context.VSSetConstantBuffers(13, Some(&[Some(viewport_buffer.clone())]));
                        context.RSSetViewports(Some(&[D3D11_VIEWPORT {
                            TopLeftX: 0.,
                            TopLeftY: 0.,
                            Width: *width as f32,
                            Height: *height as f32,
                            MinDepth: 0.,
                            MaxDepth: 1.,
                        }]));
                        context.RSSetScissorRects(Some(&[windows::Win32::Foundation::RECT {
                            left: 0,
                            top: 0,
                            right: *width as i32,
                            bottom: *height as i32,
                        }]));
                        if let (Some(target), Some(color)) = (&target, clear) {
                            context.ClearRenderTargetView(target, color);
                        }
                        if let Some(attachment) = depth {
                            let mut flags = 0;
                            if attachment.clear_depth.is_some() {
                                flags |= D3D11_CLEAR_DEPTH.0;
                            }
                            if attachment.clear_stencil.is_some() {
                                flags |= D3D11_CLEAR_STENCIL.0;
                            }
                            if flags != 0 {
                                context.ClearDepthStencilView(
                                    &attachment.view,
                                    flags,
                                    attachment.clear_depth.unwrap_or(0.0),
                                    attachment.clear_stencil.unwrap_or(0),
                                );
                            }
                        }
                    }
                    NativeCommand::SetPipeline {
                        vertex,
                        fragment,
                        input,
                        topology,
                        rasterizer,
                        blend,
                        sample_mask,
                        depth_state,
                        vertex_index,
                    } => {
                        active_depth_state = depth_state.clone();
                        active_blend_state = blend.clone();
                        active_sample_mask = *sample_mask;
                        context.RSSetState(Some(rasterizer));
                        context.OMSetBlendState(
                            active_blend_state.as_ref(),
                            Some(&blend_constant),
                            active_sample_mask,
                        );
                        context
                            .OMSetDepthStencilState(active_depth_state.as_ref(), stencil_reference);
                        context.IASetInputLayout(input.as_ref());
                        if let Some((slot, buffer)) = vertex_index {
                            context.IASetVertexBuffers(
                                *slot,
                                1,
                                Some(&Some(buffer.clone())),
                                Some(&4),
                                Some(&0),
                            );
                        } else if let Some(slot) = active_vertex_index_slot {
                            context.IASetVertexBuffers(slot, 1, Some(&None), Some(&0), Some(&0));
                        }
                        active_vertex_index_slot = vertex_index.as_ref().map(|(slot, _)| *slot);
                        context.IASetPrimitiveTopology(match topology {
                            wgt::PrimitiveTopology::PointList => D3D_PRIMITIVE_TOPOLOGY_POINTLIST,
                            wgt::PrimitiveTopology::LineList => D3D_PRIMITIVE_TOPOLOGY_LINELIST,
                            wgt::PrimitiveTopology::LineStrip => D3D_PRIMITIVE_TOPOLOGY_LINESTRIP,
                            wgt::PrimitiveTopology::TriangleList => {
                                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST
                            }
                            wgt::PrimitiveTopology::TriangleStrip => {
                                D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP
                            }
                        });
                        context.VSSetShader(Some(vertex), None);
                        context.PSSetShader(Some(fragment), None);
                    }
                    NativeCommand::SetBlendConstant(value) => {
                        blend_constant = *value;
                        context.OMSetBlendState(
                            active_blend_state.as_ref(),
                            Some(&blend_constant),
                            active_sample_mask,
                        );
                    }
                    NativeCommand::SetVertexShader(vertex) => {
                        context.VSSetShader(Some(vertex), None)
                    }
                    NativeCommand::SetUniformGroup {
                        uniform,
                        slot,
                        visibility,
                        source_box,
                    } => uniform.execute(context, *slot, *visibility, source_box)?,
                    NativeCommand::RefreshSampledTextures { textures } => {
                        let mut targets = [None];
                        let mut depth = None;
                        context.OMGetRenderTargets(Some(&mut targets), Some(&mut depth));
                        context.OMSetRenderTargets(None, None);
                        context.PSSetShaderResources(
                            0,
                            Some(&[None, None, None, None, None, None, None, None]),
                        );
                        for texture in textures {
                            texture.execute(context);
                        }
                        context.OMSetRenderTargets(
                            if targets[0].is_some() {
                                Some(&targets)
                            } else {
                                None
                            },
                            depth.as_ref(),
                        );
                    }
                    NativeCommand::SetTexture { slot, texture } => {
                        context.PSSetShaderResources(*slot, Some(&[Some(texture.clone())]));
                    }
                    NativeCommand::SetSampler { slot, sampler } => {
                        context.PSSetSamplers(*slot, Some(&[Some(sampler.clone())]));
                    }
                    NativeCommand::SetTexelLoad {
                        texture,
                        sampler,
                        dimensions,
                        kind,
                        has_implicit_sample,
                        width,
                        height,
                        depth,
                        mip_levels,
                    } => {
                        let constants = match kind {
                            super::hal_shader::Fl9TextureEmulationKind::Load
                            | super::hal_shader::Fl9TextureEmulationKind::LoadDimensions => [
                                *width as f32,
                                *height as f32,
                                1.0 / *width as f32,
                                1.0 / *height as f32,
                            ],
                            super::hal_shader::Fl9TextureEmulationKind::Load3D => {
                                [*width as f32, *height as f32, *depth as f32, 0.0]
                            }
                            super::hal_shader::Fl9TextureEmulationKind::Dimensions => {
                                [*width as f32, *height as f32, 0.0, 0.0]
                            }
                            super::hal_shader::Fl9TextureEmulationKind::DimensionsCube => {
                                [*width as f32, *height as f32, 0.0, 0.0]
                            }
                            super::hal_shader::Fl9TextureEmulationKind::Dimensions3D => {
                                [*width as f32, *height as f32, *depth as f32, 0.0]
                            }
                            super::hal_shader::Fl9TextureEmulationKind::NumLevels => {
                                [*mip_levels as f32, 0.0, 0.0, 0.0]
                            }
                            super::hal_shader::Fl9TextureEmulationKind::BaseClamp => {
                                [*width as f32, *height as f32, 0.0, 0.0]
                            }
                        };
                        context.UpdateSubresource(
                            dimensions,
                            0,
                            None,
                            constants.as_ptr().cast(),
                            0,
                            0,
                        );
                        let sampled_resource = if matches!(
                            kind,
                            super::hal_shader::Fl9TextureEmulationKind::NumLevels
                        ) && !has_implicit_sample
                        {
                            None
                        } else {
                            Some(texture.clone())
                        };
                        context.PSSetShaderResources(0, Some(&[sampled_resource]));
                        if matches!(
                            kind,
                            super::hal_shader::Fl9TextureEmulationKind::Load
                                | super::hal_shader::Fl9TextureEmulationKind::Load3D
                                | super::hal_shader::Fl9TextureEmulationKind::LoadDimensions
                        ) {
                            context.PSSetSamplers(15, Some(&[Some(sampler.clone())]));
                        }
                        context.PSSetConstantBuffers(13, Some(&[Some(dimensions.clone())]));
                    }
                    NativeCommand::SetVertex {
                        slot,
                        buffer,
                        offset,
                        stride,
                    } => {
                        context.IASetVertexBuffers(
                            *slot,
                            1,
                            Some(&Some(buffer.clone())),
                            Some(stride),
                            Some(offset),
                        );
                    }
                    NativeCommand::SetViewport {
                        viewport,
                        buffer,
                        transform,
                    } => {
                        let mut constants = [0u32; 8];
                        for (destination, value) in constants[..4].iter_mut().zip(transform) {
                            *destination = value.to_bits();
                        }
                        context.UpdateSubresource(buffer, 0, None, constants.as_ptr().cast(), 0, 0);
                        context.RSSetViewports(Some(core::slice::from_ref(viewport)));
                    }
                    NativeCommand::SetScissor(rect) => {
                        context.RSSetScissorRects(Some(core::slice::from_ref(rect)))
                    }
                    NativeCommand::SetStencilReference(reference) => {
                        stencil_reference = *reference;
                        context
                            .OMSetDepthStencilState(active_depth_state.as_ref(), stencil_reference);
                    }
                    NativeCommand::Draw {
                        first,
                        count,
                        instances,
                        max_vertices,
                        zero_input,
                        vertex_index,
                        instance_bindings,
                    } => {
                        instance_bindings.validate(*instances)?;
                        if let Some(vertex_index) = vertex_index {
                            let batch_limit = (*max_vertices)
                                .min(vertex_index.max_vertex_index.saturating_add(1));
                            let batch_limit = batch_limit - batch_limit % 3;
                            if batch_limit == 0 {
                                return Err(Error::from_hresult(E_INVALIDARG));
                            }
                            draw_batches(0, *count, 0, batch_limit, |_, _| {})?;
                            let device = context.GetDevice()?;
                            let mut chunks = Vec::new();
                            let chunk_count = count.div_ceil(batch_limit) as usize;
                            chunks
                                .try_reserve_exact(chunk_count)
                                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                            let mut relative = 0;
                            while relative < *count {
                                let count = (*count - relative).min(batch_limit);
                                let first = first
                                    .checked_add(relative)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                first
                                    .checked_add(count - 1)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                let byte_width = count
                                    .checked_mul(4)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                let mut values = Vec::new();
                                values
                                    .try_reserve_exact(count as usize)
                                    .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                                for index in 0..count {
                                    values.push((first + index) as f32);
                                }
                                let desc = D3D11_BUFFER_DESC {
                                    ByteWidth: byte_width,
                                    Usage: D3D11_USAGE_IMMUTABLE,
                                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                                    ..Default::default()
                                };
                                let data = D3D11_SUBRESOURCE_DATA {
                                    pSysMem: values.as_ptr().cast(),
                                    ..Default::default()
                                };
                                let mut buffer = None;
                                device.CreateBuffer(&desc, Some(&data), Some(&mut buffer))?;
                                chunks.push((
                                    first,
                                    count,
                                    buffer.ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
                                ));
                                relative = relative
                                    .checked_add(count)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            }
                            for instance in 0..*instances {
                                instance_bindings.bind(context, instance)?;
                                for (first, count, index_buffer) in &chunks {
                                    for (slot, buffer, offset, stride) in &vertex_index.vertices {
                                        let offset = u64::from(*offset)
                                            .checked_add(u64::from(*first) * u64::from(*stride))
                                            .and_then(|offset| u32::try_from(offset).ok())
                                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                        context.IASetVertexBuffers(
                                            *slot,
                                            1,
                                            Some(&Some(buffer.clone())),
                                            Some(stride),
                                            Some(&offset),
                                        );
                                    }
                                    context.IASetVertexBuffers(
                                        vertex_index.slot,
                                        1,
                                        Some(&Some(index_buffer.clone())),
                                        Some(&4),
                                        Some(&0),
                                    );
                                    context.Draw(*count, 0);
                                }
                            }
                            for (slot, buffer, offset, stride) in &vertex_index.vertices {
                                context.IASetVertexBuffers(
                                    *slot,
                                    1,
                                    Some(&Some(buffer.clone())),
                                    Some(stride),
                                    Some(offset),
                                );
                            }
                            context.IASetVertexBuffers(
                                vertex_index.slot,
                                1,
                                Some(&Some(vertex_index.identity.clone())),
                                Some(&4),
                                Some(&0),
                            );
                        } else {
                            draw_batches(*first, *count, 0, *max_vertices, |_, _| {})?;
                            for instance in 0..*instances {
                                instance_bindings.bind(context, instance)?;
                                draw_batches(*first, *count, 1, *max_vertices, |first, count| {
                                    context.Draw(count, draw_start_vertex(first, *zero_input));
                                })?;
                            }
                        }
                    }
                    NativeCommand::DrawPoints {
                        vertices,
                        first_vertex,
                        count,
                        instances,
                        max_points,
                        max_vertex_index,
                        vertex_index,
                        instance_bindings,
                    } => {
                        instance_bindings.validate(*instances)?;
                        first_vertex
                            .checked_add(count.saturating_sub(1))
                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                        point_batches(*count, *max_points, |first, _| {
                            for (_, _, offset, stride) in vertices {
                                strip_vertex_offset(*offset, *stride, first)?;
                            }
                            Ok(())
                        })?;
                        for instance in 0..*instances {
                            instance_bindings.bind(context, instance)?;
                            point_batches(*count, *max_points, |first, count| {
                                for (slot, buffer, base, stride) in vertices {
                                    let offset = strip_vertex_offset(*base, *stride, first)?;
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer.clone())),
                                        Some(stride),
                                        Some(&offset),
                                    );
                                }
                                if let Some((slot, identity)) = vertex_index {
                                    let absolute_first = first_vertex
                                        .checked_add(first)
                                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                    let last = absolute_first
                                        .checked_add(count - 1)
                                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                    let (buffer, offset) = if last <= *max_vertex_index {
                                        let offset = absolute_first
                                            .checked_mul(4)
                                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                        (identity.clone(), offset)
                                    } else {
                                        let mut ids = Vec::new();
                                        ids.try_reserve_exact(count as usize)
                                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                                        ids.extend(
                                            (0..count).map(|i| u64::from(absolute_first + i)),
                                        );
                                        let device = context.GetDevice()?;
                                        (
                                            super::hal_index::create_vertex_index_buffer(
                                                &device, &ids,
                                            )?,
                                            0,
                                        )
                                    };
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer)),
                                        Some(&4),
                                        Some(&offset),
                                    );
                                }
                                context.Draw(count, 0);
                                Ok(())
                            })?;
                        }
                        if let Some((slot, identity)) = vertex_index {
                            context.IASetVertexBuffers(
                                *slot,
                                1,
                                Some(&Some(identity.clone())),
                                Some(&4),
                                Some(&0),
                            );
                        }
                    }
                    NativeCommand::DrawLines {
                        vertices,
                        first_vertex,
                        count,
                        max_vertices,
                        max_vertex_index,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => {
                        instance_bindings.validate(*instances)?;
                        first_vertex
                            .checked_add(count.saturating_sub(1))
                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                        let mut restore = VertexBufferRestore::new(context);
                        for (slot, buffer, offset, stride) in vertices {
                            restore.add(*slot, buffer.clone(), *stride, *offset)?;
                        }
                        if let Some((slot, identity)) = vertex_index {
                            restore.add(*slot, identity.clone(), 4, 0)?;
                        }
                        line_list_batches(*count, *max_vertices, 1, |first, batch_count| {
                            let absolute_first = first_vertex
                                .checked_add(first)
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            absolute_first
                                .checked_add(batch_count.saturating_sub(1))
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            for (_, _, offset, stride) in vertices {
                                strip_vertex_offset(*offset, *stride, absolute_first)?;
                            }
                            Ok(())
                        })?;
                        for instance in 0..*instances {
                            instance_bindings.bind(context, instance)?;
                            line_list_batches(*count, *max_vertices, 1, |first, count| {
                                let absolute_first = first_vertex
                                    .checked_add(first)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                for (slot, buffer, base, stride) in vertices {
                                    let offset =
                                        strip_vertex_offset(*base, *stride, absolute_first)?;
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer.clone())),
                                        Some(stride),
                                        Some(&offset),
                                    );
                                }
                                if let Some((slot, identity)) = vertex_index {
                                    let last = absolute_first
                                        .checked_add(count - 1)
                                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                    let (buffer, offset) = if last <= *max_vertex_index {
                                        let offset = absolute_first
                                            .checked_mul(4)
                                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                        (identity.clone(), offset)
                                    } else {
                                        let mut ids = Vec::new();
                                        ids.try_reserve_exact(count as usize)
                                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                                        ids.extend(
                                            (0..count).map(|i| u64::from(absolute_first + i)),
                                        );
                                        let device = context.GetDevice()?;
                                        (
                                            super::hal_index::create_vertex_index_buffer(
                                                &device, &ids,
                                            )?,
                                            0,
                                        )
                                    };
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer)),
                                        Some(&4),
                                        Some(&offset),
                                    );
                                }
                                context.Draw(count, 0);
                                Ok(())
                            })?;
                        }
                    }
                    NativeCommand::DrawLineStrip {
                        vertices,
                        first_vertex,
                        count,
                        max_vertices,
                        max_vertex_index,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => {
                        instance_bindings.validate(*instances)?;
                        first_vertex
                            .checked_add(count.saturating_sub(1))
                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                        let mut restore = VertexBufferRestore::new(context);
                        for (slot, buffer, offset, stride) in vertices {
                            restore.add(*slot, buffer.clone(), *stride, *offset)?;
                        }
                        if let Some((slot, identity)) = vertex_index {
                            restore.add(*slot, identity.clone(), 4, 0)?;
                        }
                        line_strip_batches(*count, *max_vertices, 1, |first, count| {
                            let absolute_first = first_vertex
                                .checked_add(first)
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            absolute_first
                                .checked_add(count - 1)
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            for (_, _, offset, stride) in vertices {
                                strip_vertex_offset(*offset, *stride, absolute_first)?;
                            }
                            Ok(())
                        })?;
                        for instance in 0..*instances {
                            instance_bindings.bind(context, instance)?;
                            line_strip_batches(*count, *max_vertices, 1, |first, count| {
                                let absolute_first = first_vertex
                                    .checked_add(first)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                let last = absolute_first
                                    .checked_add(count - 1)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                for (slot, buffer, base, stride) in vertices {
                                    let offset =
                                        strip_vertex_offset(*base, *stride, absolute_first)?;
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer.clone())),
                                        Some(stride),
                                        Some(&offset),
                                    );
                                }
                                if let Some((slot, identity)) = vertex_index {
                                    let (buffer, offset) = if last <= *max_vertex_index {
                                        let offset = absolute_first
                                            .checked_mul(4)
                                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                        (identity.clone(), offset)
                                    } else {
                                        let mut ids = Vec::new();
                                        ids.try_reserve_exact(count as usize)
                                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                                        ids.extend(
                                            (0..count).map(|i| u64::from(absolute_first + i)),
                                        );
                                        (
                                            super::hal_index::create_vertex_index_buffer(
                                                &context.GetDevice()?,
                                                &ids,
                                            )?,
                                            0,
                                        )
                                    };
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer)),
                                        Some(&4),
                                        Some(&offset),
                                    );
                                }
                                context.Draw(count, 0);
                                Ok(())
                            })?;
                        }
                    }
                    NativeCommand::DrawStrip {
                        vertices,
                        first_vertex,
                        count,
                        max_vertex_index,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => {
                        instance_bindings.validate(*instances)?;
                        first_vertex
                            .checked_add(count.saturating_sub(1))
                            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                        let mut restore = VertexBufferRestore::new(context);
                        for (slot, buffer, offset, stride) in vertices {
                            restore.add(*slot, buffer.clone(), *stride, *offset)?;
                        }
                        if let Some((slot, identity)) = vertex_index {
                            restore.add(*slot, identity.clone(), 4, 0)?;
                        }
                        let device = if vertex_index.is_some() {
                            Some(context.GetDevice()?)
                        } else {
                            None
                        };
                        let mut batches = Vec::new();
                        strip_batches(*count, 1, |first, batch_count| {
                            let absolute_first = first_vertex
                                .checked_add(first)
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            let last = absolute_first
                                .checked_add(batch_count.saturating_sub(1))
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            for (_, _, offset, stride) in vertices {
                                strip_vertex_offset(*offset, *stride, absolute_first)?;
                            }
                            let index_binding = match vertex_index {
                                Some((_, identity)) if last <= *max_vertex_index => Some((
                                    identity.clone(),
                                    absolute_first
                                        .checked_mul(4)
                                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
                                )),
                                Some(_) => {
                                    let indices =
                                        direct_strip_vertex_indices(absolute_first, batch_count)?;
                                    Some((
                                        super::hal_index::create_vertex_index_buffer(
                                            device
                                                .as_ref()
                                                .ok_or_else(|| Error::from_hresult(E_FAIL))?,
                                            &indices,
                                        )?,
                                        0,
                                    ))
                                }
                                None => None,
                            };
                            batches
                                .try_reserve(1)
                                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                            batches.push((absolute_first, batch_count, index_binding));
                            Ok(())
                        })?;
                        for instance in 0..*instances {
                            instance_bindings.bind(context, instance)?;
                            for (first, count, index_binding) in &batches {
                                for (slot, buffer, base_offset, stride) in vertices {
                                    let offset =
                                        strip_vertex_offset(*base_offset, *stride, *first)?;
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer.clone())),
                                        Some(stride),
                                        Some(&offset),
                                    );
                                }
                                if let (Some((slot, _)), Some((buffer, offset))) =
                                    (vertex_index, index_binding)
                                {
                                    context.IASetVertexBuffers(
                                        *slot,
                                        1,
                                        Some(&Some(buffer.clone())),
                                        Some(&4),
                                        Some(offset),
                                    );
                                }
                                context.Draw(*count, 0);
                            }
                        }
                    }
                    NativeCommand::DrawIndexed {
                        binding,
                        vertex,
                        vertex_offset,
                        first,
                        count,
                        base_vertex,
                        vertex_bytes,
                        stride,
                        extent,
                        max_vertices,
                        instances,
                        instance_bindings,
                        vertex_index,
                    } => {
                        binding.draw(
                            context,
                            *first,
                            *count,
                            *base_vertex,
                            (vertex, *vertex_offset, *vertex_bytes, *stride, *extent),
                            (*max_vertices, *instances),
                            instance_bindings,
                            vertex_index.as_ref(),
                        )?;
                    }
                    NativeCommand::DrawIndexedMulti {
                        binding,
                        vertices,
                        first,
                        count,
                        base_vertex,
                        max_vertices,
                        instances,
                        instance_bindings,
                        vertex_index,
                    } => binding.draw_multi(
                        context,
                        *first,
                        *count,
                        *base_vertex,
                        vertices,
                        (*max_vertices, *instances),
                        instance_bindings,
                        vertex_index.as_ref(),
                    )?,
                    NativeCommand::DrawIndexedPoints {
                        binding,
                        vertices,
                        first,
                        count,
                        base_vertex,
                        max_points,
                        max_vertex_index,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => binding.draw_points(
                        context,
                        *first,
                        *count,
                        *base_vertex,
                        vertices,
                        (*max_points, *instances),
                        instance_bindings,
                        *max_vertex_index,
                        vertex_index.as_ref(),
                    )?,
                    NativeCommand::DrawIndexedLines {
                        binding,
                        vertices,
                        first,
                        count,
                        base_vertex,
                        max_indices,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => binding.draw_lines(
                        context,
                        *first,
                        *count,
                        *base_vertex,
                        vertices,
                        (*max_indices, *instances),
                        instance_bindings,
                        vertex_index.as_ref(),
                    )?,
                    NativeCommand::DrawIndexedLineStrips {
                        binding,
                        vertices,
                        first,
                        count,
                        base_vertex,
                        max_indices,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => binding.draw_line_strips(
                        context,
                        *first,
                        *count,
                        *base_vertex,
                        vertices,
                        (*max_indices, *instances),
                        instance_bindings,
                        vertex_index.as_ref(),
                    )?,
                    NativeCommand::DrawIndexedTriangleStrips {
                        binding,
                        vertices,
                        first,
                        count,
                        base_vertex,
                        max_vertices,
                        vertex_index,
                        instances,
                        instance_bindings,
                    } => {
                        context.IASetPrimitiveTopology(
                            if vertices.is_empty() && vertex_index.is_none() {
                                D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP
                            } else {
                                D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST
                            },
                        );
                        let result = binding.draw_triangle_strips(
                            context,
                            *first,
                            *count,
                            *base_vertex,
                            vertices,
                            (*max_vertices, *instances),
                            instance_bindings,
                            vertex_index.as_ref(),
                        );
                        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLESTRIP);
                        result?;
                    }
                    NativeCommand::EndRender { resolve } => {
                        context.ClearState();
                        if let Some(resolve) = resolve {
                            context.ResolveSubresource(
                                &resolve.destination,
                                resolve.destination_subresource,
                                &resolve.source,
                                resolve.source_subresource,
                                resolve.format,
                            );
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

crate::impl_dyn_resource!(CommandBuffer);
impl crate::DynCommandBuffer for CommandBuffer {}

impl NativeDevice {
    /// Begin an internal copy-only recording.
    #[allow(dead_code)]
    pub(crate) fn create_copy_encoder(&self) -> CopyEncoder<'_> {
        CopyEncoder {
            device: self,
            copies: Vec::new(),
        }
    }

    /// Replay a recorded command buffer in order under one lock.
    /// This internal helper does not signal a fence.
    #[allow(dead_code)]
    pub(crate) fn submit_copy_commands(&self, command_buffer: &CommandBuffer) -> Result<()> {
        command_buffer.validate_device(self)?;
        let mut retained = Vec::new();
        command_buffer.retain_surface_targets(&mut retained)?;
        let context = self.immediate_context();
        command_buffer.replay(&context)
    }
}

impl CopyEncoder<'_> {
    /// Validate and retain one buffer-copy region without submitting it.
    #[allow(dead_code)]
    pub(crate) fn copy_buffer(
        &mut self,
        source: &NativeBuffer,
        destination: &NativeBuffer,
        region: &crate::BufferCopy,
    ) -> Result<()> {
        self.copies.push(
            self.device
                .prepare_buffer_copy(source, destination, region)?,
        );
        Ok(())
    }

    /// Finish recording, retaining all resources for later submission.
    #[allow(dead_code)]
    pub(crate) fn finish(self) -> CommandBuffer {
        CommandBuffer::from_commands(
            self.device,
            self.copies.into_iter().map(NativeCommand::Copy).collect(),
        )
    }

    #[cfg(test)]
    pub(crate) fn finish_with_injected_failure(self) -> CommandBuffer {
        let mut commands: Vec<_> = self.copies.into_iter().map(NativeCommand::Copy).collect();
        commands.push(NativeCommand::InjectedFailure);
        CommandBuffer::from_commands(self.device, commands)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, FeatureLevel};
    use alloc::vec;

    #[test]
    fn point_batches_rebase_streams_and_keep_instance_order() {
        let mut batches = Vec::new();
        for instance in 0..2 {
            point_batches(65_536, 65_535, |first, count| {
                let position = strip_vertex_offset(16, 12, first)?;
                let constant = strip_vertex_offset(24, 0, first)?;
                batches.push((instance, first, count, position, constant));
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(
            batches,
            [
                (0, 0, 65_535, 16, 24),
                (0, 65_535, 1, 786_436, 24),
                (1, 0, 65_535, 16, 24),
                (1, 65_535, 1, 786_436, 24),
            ]
        );
        assert!(point_batches(1, 0, |_, _| Ok(())).is_err());
        assert!(point_batches(2, 1, |first, _| {
            strip_vertex_offset(u32::MAX, 4, first).map(|_| ())
        })
        .is_err());
        let mut calls = 0;
        point_batches(0, 65_535, |_, _| {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 0);
    }
    #[test]
    fn instance_stream_offsets_use_wide_index_arithmetic() {
        assert_eq!(instance_vertex_offset(8, 4, 1, 2), Some(20));
        assert_eq!(instance_vertex_offset(8, 0, u32::MAX, u32::MAX), Some(8));
        assert_eq!(instance_vertex_offset(u32::MAX - 3, 4, 1, 0), None);
        assert_eq!(
            instance_vertex_offset(0, u32::MAX, u32::MAX, u32::MAX),
            None
        );
    }

    #[test]
    fn fl91_direct_triangle_batches_limit_unique_vertices() {
        let hardware =
            super::super::capabilities::HardwareGuarantees::for_level(FeatureLevel::L9_1);
        let mut batches = Vec::new();
        draw_batches(
            7,
            65_538,
            2,
            hardware.max_direct_triangle_vertices_per_draw(),
            |first, count| batches.push((first, count)),
        )
        .unwrap();
        assert_eq!(
            batches,
            [(7, 65_535), (65_542, 3), (7, 65_535), (65_542, 3)]
        );
        assert_eq!(hardware.max_triangle_vertices_per_draw(), 196_605);
        for level in FeatureLevel::ALL_DESCENDING {
            if level != FeatureLevel::L9_1 {
                let hardware = super::super::capabilities::HardwareGuarantees::for_level(level);
                assert_eq!(
                    hardware.max_direct_triangle_vertices_per_draw(),
                    hardware.max_triangle_vertices_per_draw()
                );
            }
        }
        for (count, expected) in [
            (0, vec![]),
            (1, vec![(7, 1)]),
            (2, vec![(7, 2)]),
            (65_535, vec![(7, 65_535)]),
            (65_536, vec![(7, 65_535), (65_542, 1)]),
            (65_537, vec![(7, 65_535), (65_542, 2)]),
        ] {
            batches.clear();
            draw_batches(
                7,
                count,
                1,
                hardware.max_direct_triangle_vertices_per_draw(),
                |first, count| batches.push((first, count)),
            )
            .unwrap();
            assert_eq!(batches, expected);
        }
        batches.clear();
        draw_batches(
            7,
            65_538,
            0,
            hardware.max_direct_triangle_vertices_per_draw(),
            |first, count| batches.push((first, count)),
        )
        .unwrap();
        assert!(batches.is_empty());
        assert!(draw_batches(
            u32::MAX,
            2,
            1,
            hardware.max_direct_triangle_vertices_per_draw(),
            |_, _| panic!("overflow must be rejected before drawing"),
        )
        .is_err());
    }

    #[test]
    fn strip_batches_preserve_winding_overlap_and_instances() {
        let mut batches = Vec::new();
        for count in 0..3 {
            strip_batches(count, 1, |first, count| {
                batches.push((first, count));
                Ok(())
            })
            .unwrap();
        }
        assert!(batches.is_empty());
        strip_batches(65_536, 2, |first, count| {
            batches.push((first, count));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            batches,
            [(0, 65_534), (65_532, 4), (0, 65_534), (65_532, 4)]
        );
        assert!(batches
            .iter()
            .all(|(first, count)| first % 2 == 0 && *count <= 65_534));
        batches.clear();
        strip_batches(u32::MAX, 0, |first, count| {
            batches.push((first, count));
            Ok(())
        })
        .unwrap();
        assert!(batches.is_empty());
        assert!(strip_batches(4, 1, |_, _| Err(Error::from_hresult(E_INVALIDARG))).is_err());
        assert_eq!(strip_vertex_offset(8, 12, 65_532).unwrap(), 786_392);
        assert!(strip_vertex_offset(8, 12, u32::MAX).is_err());
        assert!(strip_vertex_offset(u32::MAX, 4, 1).is_err());
    }

    #[test]
    fn direct_strip_vertex_indices_preserve_first_vertex_and_check_overflow() {
        assert_eq!(
            direct_strip_vertex_indices(23, 4).unwrap(),
            [23, 24, 25, 26]
        );
        assert_eq!(
            direct_strip_vertex_indices(65_532, 4).unwrap(),
            [65_532, 65_533, 65_534, 65_535]
        );
        assert!(direct_strip_vertex_indices(u32::MAX, 2).is_err());
    }

    #[test]
    fn line_list_batches_preserve_pairs_and_instances() {
        let mut batches = Vec::new();
        line_list_batches(0, 6, 1, |first, count| {
            batches.push((first, count));
            Ok(())
        })
        .unwrap();
        line_list_batches(1, 6, 1, |first, count| {
            batches.push((first, count));
            Ok(())
        })
        .unwrap();
        assert!(batches.is_empty());

        line_list_batches(15, 6, 2, |first, count| {
            batches.push((first, count));
            Ok(())
        })
        .unwrap();
        assert_eq!(batches, [(0, 6), (6, 6), (12, 2), (0, 6), (6, 6), (12, 2)]);
        assert!(batches
            .iter()
            .all(|(first, count)| first % 2 == 0 && count % 2 == 0));
        assert!(line_list_batches(4, 1, 1, |_, _| Ok(())).is_err());
        assert!(line_list_batches(4, 3, 1, |_, _| Ok(())).is_err());
        assert!(line_list_batches(4, 4, 1, |_, _| Err(Error::from_hresult(E_INVALIDARG))).is_err());
    }

    #[test]
    fn line_strip_batches_overlap_one_vertex_across_batches() {
        let mut batches = Vec::new();
        for count in 0..2 {
            line_strip_batches(count, 6, 1, |first, count| {
                batches.push((first, count));
                Ok(())
            })
            .unwrap();
        }
        assert!(batches.is_empty());
        line_strip_batches(13, 6, 2, |first, count| {
            batches.push((first, count));
            Ok(())
        })
        .unwrap();
        assert_eq!(batches, [(0, 6), (5, 6), (10, 3), (0, 6), (5, 6), (10, 3)]);
        for windows in batches.chunks_exact(3) {
            assert_eq!(windows[1].0, windows[0].0 + windows[0].1 - 1);
            assert_eq!(windows[2].0, windows[1].0 + windows[1].1 - 1);
        }
        assert!(line_strip_batches(2, 1, 1, |_, _| Ok(())).is_err());
        assert!(
            line_strip_batches(4, 2, 1, |_, _| Err(Error::from_hresult(E_INVALIDARG))).is_err()
        );
    }

    #[test]
    fn fl91_line_batch_limits_preserve_complete_primitives() {
        let hardware =
            super::super::capabilities::HardwareGuarantees::for_level(FeatureLevel::L9_1);
        let mut batches = Vec::new();
        line_list_batches(
            131_070,
            hardware.max_direct_line_list_vertices_per_draw(),
            1,
            |first, count| {
                batches.push((first, count));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(batches, [(0, 65_534), (65_534, 65_534), (131_068, 2)]);

        batches.clear();
        line_strip_batches(
            65_536,
            hardware.max_direct_line_strip_vertices_per_draw(),
            1,
            |first, count| {
                batches.push((first, count));
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(batches, [(0, 65_535), (65_534, 2)]);
    }

    #[test]
    fn draw_batches_keep_instance_order_and_check_boundaries() {
        let mut batches = Vec::new();
        draw_batches(5, 9, 2, 6, |first, count| batches.push((first, count))).unwrap();
        assert_eq!(batches, [(5, 6), (11, 3), (5, 6), (11, 3)]);
        batches.clear();
        draw_batches(0, 9, 0, 6, |first, count| batches.push((first, count))).unwrap();
        draw_batches(u32::MAX, 0, 2, 6, |first, count| {
            batches.push((first, count))
        })
        .unwrap();
        assert!(batches.is_empty());
        draw_batches(u32::MAX, 1, 1, 6, |first, count| {
            batches.push((first, count))
        })
        .unwrap();
        assert_eq!(batches, [(u32::MAX, 1)]);
        assert!(draw_batches(u32::MAX, 2, 1, 6, |_, _| {}).is_err());
        assert!(draw_batches(0, 3, 1, 0, |_, _| {}).is_err());
        assert!(draw_batches(0, 3, 1, 4, |_, _| {}).is_err());
    }

    #[test]
    fn point_batches_preserve_instances_and_zero_input_starts() {
        let mut batches = Vec::new();
        draw_batches(0, 65_536, 1, 65_535, |first, count| {
            batches.push((first, count))
        })
        .unwrap();
        assert_eq!(batches, [(0, 65_535), (65_535, 1)]);
        batches.clear();
        draw_batches(0, 65_536, 2, 65_535, |first, count| {
            batches.push((first, count))
        })
        .unwrap();
        assert_eq!(
            batches,
            [(0, 65_535), (65_535, 1), (0, 65_535), (65_535, 1)]
        );
        let zero_starts: Vec<_> = batches
            .iter()
            .map(|&(first, _)| draw_start_vertex(first, true))
            .collect();
        assert_eq!(zero_starts, [0, 0, 0, 0]);
        let attribute_starts: Vec<_> = batches
            .iter()
            .map(|&(first, _)| draw_start_vertex(first, false))
            .collect();
        assert_eq!(attribute_starts, [0, 65_535, 0, 65_535]);
        assert_eq!(draw_start_vertex(37, true), 0);
        assert_eq!(draw_start_vertex(37, false), 37);
    }

    #[test]
    fn replay_failure_reaches_queue_and_does_not_signal_fence() -> Result<()> {
        use alloc::sync::Arc;
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            assert!(std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_none_or(|v| v != "1"));
            std::println!("SKIP: no physical D3D11 adapter");
            return Ok(());
        };
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        assert_eq!(device.feature_level(), FeatureLevel::L9_1);
        std::println!(
            "Hardware: {:?}; requested FL9_1; actual FL9_1",
            adapter.identity()
        );
        let queue = crate::dx11::Queue::new(device.clone());
        let fence = device.create_native_fence();
        let source = device.create_byte_buffer(16)?;
        let destination = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&source, 0, &[0x6d; 16])?;
        device.write_byte_buffer(&destination, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: core::num::NonZeroU64::new(16).unwrap(),
        };
        let mut encoder = device.create_copy_encoder();
        encoder.copy_buffer(&source, &destination, &region)?;
        let commands = encoder.finish_with_injected_failure();
        let result = unsafe { crate::Queue::submit(&queue, &[&commands], &[], (&fence, 1)) };
        assert_eq!(result, Err(crate::DeviceError::Unexpected));
        assert!(device
            .wait_native_fence(&fence, 1, Some(core::time::Duration::ZERO))
            .is_err());
        assert_eq!(
            device.read_byte_buffer(&destination, 0, 16)?,
            vec![0x6d; 16]
        );
        Ok(())
    }

    #[test]
    fn recorded_copy_submits_in_order_and_retains_resources() -> Result<()> {
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = adapter.open_device(&[FeatureLevel::L9_1])?;
        let middle = device.create_byte_buffer(16)?;
        let output = device.create_byte_buffer(16)?;
        device.write_byte_buffer(&middle, 0, &[0; 16])?;
        device.write_byte_buffer(&output, 0, &[0; 16])?;
        let region = crate::BufferCopy {
            src_offset: 0,
            dst_offset: 0,
            size: core::num::NonZeroU64::new(16).unwrap(),
        };
        let command_buffer = {
            let input = device.create_byte_buffer(16)?;
            device.write_byte_buffer(&input, 0, &[0x5a; 16])?;
            let mut encoder = device.create_copy_encoder();
            assert!(encoder.copy_buffer(&input, &input, &region).is_err());
            encoder.copy_buffer(&input, &middle, &region)?;
            encoder.copy_buffer(&middle, &output, &region)?;
            encoder.finish()
        }; // `input` has been dropped; the command buffer owns its COM reference.
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0; 16]);
        let dynamic: &dyn crate::DynCommandBuffer = &command_buffer;
        assert!(dynamic.as_any().is::<CommandBuffer>());
        let other = adapter.open_device(&[FeatureLevel::L9_1])?;
        assert!(other.submit_copy_commands(&command_buffer).is_err());
        device.submit_copy_commands(&command_buffer)?;
        assert_eq!(device.read_byte_buffer(&output, 0, 16)?, vec![0x5a; 16]);
        Ok(())
    }
}
