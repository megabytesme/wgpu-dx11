//! Recorded copy commands and a restricted FL9_1 render path.
use super::{
    buffer::{NativeBuffer, NativeBufferClear, NativeBufferCopy},
    device::NativeDevice,
    hal_index::IndexBinding,
    hal_transfer::{BufferToTexture, TextureToBuffer, TextureToTexture},
    hal_uniform::UniformBinding,
};
use alloc::{
    sync::{Arc, Weak},
    vec::Vec,
};
use windows::{
    core::{Error, Interface, Result},
    Win32::{
        Foundation::{E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Direct3D11::{
                ID3D11Buffer, ID3D11DepthStencilState, ID3D11DepthStencilView, ID3D11Device,
                ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState,
                ID3D11RenderTargetView, ID3D11SamplerState, ID3D11ShaderResourceView,
                ID3D11VertexShader, D3D11_CLEAR_DEPTH, D3D11_CLEAR_STENCIL, D3D11_VIEWPORT,
            },
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
    pub clear_depth: Option<f32>,
    pub clear_stencil: Option<u8>,
}

#[derive(Debug)]
pub(super) enum RenderTarget {
    Owned(ID3D11RenderTargetView),
    Surface(Weak<ID3D11RenderTargetView>),
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
    BufferToTexture(BufferToTexture),
    TextureToBuffer(TextureToBuffer),
    TextureToTexture(TextureToTexture),
    #[cfg(test)]
    InjectedFailure,
    BeginRender {
        target: Option<RenderTarget>,
        clear: Option<[f32; 4]>,
        depth: Option<DepthAttachment>,
        width: u32,
        height: u32,
    },
    SetPipeline {
        vertex: ID3D11VertexShader,
        fragment: ID3D11PixelShader,
        input: ID3D11InputLayout,
        rasterizer: ID3D11RasterizerState,
        blend: Option<windows::Win32::Graphics::Direct3D11::ID3D11BlendState>,
        depth_state: Option<ID3D11DepthStencilState>,
    },
    SetUniformGroup {
        uniform: Arc<UniformBinding>,
        slot: u32,
        visibility: wgt::ShaderStages,
    },
    SetTexture {
        slot: u32,
        texture: ID3D11ShaderResourceView,
    },
    SetSampler {
        slot: u32,
        sampler: ID3D11SamplerState,
    },
    SetVertex {
        buffer: ID3D11Buffer,
        offset: u32,
        stride: u32,
    },
    SetViewport(D3D11_VIEWPORT),
    SetScissor(windows::Win32::Foundation::RECT),
    Draw {
        first: u32,
        count: u32,
    },
    DrawIndexed {
        binding: Arc<IndexBinding>,
        first: u32,
        count: u32,
        vertex_bytes: u64,
        stride: u32,
        extent: u32,
        max_vertices: u32,
    },
    EndRender,
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
                            let region = windows::Win32::Graphics::Direct3D11::D3D11_BOX {
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

                    NativeCommand::BufferToTexture(copy) => copy.execute(context)?,
                    NativeCommand::TextureToBuffer(copy) => copy.execute(context)?,
                    NativeCommand::TextureToTexture(copy) => copy.execute(context),
                    #[cfg(test)]
                    NativeCommand::InjectedFailure => {
                        return Err(Error::from_hresult(windows::Win32::Foundation::E_FAIL));
                    }
                    NativeCommand::BeginRender {
                        target,
                        clear,
                        depth,
                        width,
                        height,
                    } => {
                        let target = target.as_ref().map(RenderTarget::resolve).transpose()?;
                        context.ClearState();
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
                        context.OMSetDepthStencilState(None, 0);
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
                        rasterizer,
                        blend,
                        depth_state,
                    } => {
                        context.RSSetState(Some(rasterizer));
                        context.OMSetBlendState(blend.as_ref(), None, u32::MAX);
                        context.OMSetDepthStencilState(depth_state.as_ref(), 0);
                        context.IASetInputLayout(Some(input));
                        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                        context.VSSetShader(Some(vertex), None);
                        context.PSSetShader(Some(fragment), None);
                    }
                    NativeCommand::SetUniformGroup {
                        uniform,
                        slot,
                        visibility,
                    } => uniform.execute(context, *slot, *visibility)?,
                    NativeCommand::SetTexture { slot, texture } => {
                        context.PSSetShaderResources(*slot, Some(&[Some(texture.clone())]));
                    }
                    NativeCommand::SetSampler { slot, sampler } => {
                        context.PSSetSamplers(*slot, Some(&[Some(sampler.clone())]));
                    }
                    NativeCommand::SetVertex {
                        buffer,
                        offset,
                        stride,
                    } => {
                        context.IASetVertexBuffers(
                            0,
                            1,
                            Some(&Some(buffer.clone())),
                            Some(stride),
                            Some(offset),
                        );
                    }
                    NativeCommand::SetViewport(viewport) => {
                        context.RSSetViewports(Some(core::slice::from_ref(viewport)))
                    }
                    NativeCommand::SetScissor(rect) => {
                        context.RSSetScissorRects(Some(core::slice::from_ref(rect)))
                    }
                    NativeCommand::Draw { first, count } => context.Draw(*count, *first),
                    NativeCommand::DrawIndexed {
                        binding,
                        first,
                        count,
                        vertex_bytes,
                        stride,
                        extent,
                        max_vertices,
                    } => {
                        binding.draw(
                            context,
                            *first,
                            *count,
                            (*vertex_bytes, *stride, *extent),
                            *max_vertices,
                        )?;
                    }
                    NativeCommand::EndRender => context.ClearState(),
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dx11::{enumerate_native_adapters, FeatureLevel};
    use alloc::vec;

    #[test]
    fn replay_failure_reaches_queue_and_does_not_signal_fence() -> Result<()> {
        use alloc::sync::Arc;
        let adapters = enumerate_native_adapters()?;
        let Some(adapter) = adapters.iter().find(|adapter| !adapter.identity().software) else {
            return Ok(());
        };
        let device = Arc::new(adapter.open_device(&[FeatureLevel::L9_1])?);
        let queue = crate::dx11::Queue::new(device.clone());
        let fence = device.create_native_fence();
        let commands =
            CommandBuffer::from_commands(&device, alloc::vec![NativeCommand::InjectedFailure]);
        let result = unsafe { crate::Queue::submit(&queue, &[&commands], &[], (&fence, 1)) };
        assert_eq!(result, Err(crate::DeviceError::Unexpected));
        assert!(device
            .wait_native_fence(&fence, 1, Some(core::time::Duration::ZERO))
            .is_err());
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
