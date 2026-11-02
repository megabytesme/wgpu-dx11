//! Recorded copy commands and a restricted FL9_1 render path.
use super::{
    buffer::{NativeBuffer, NativeBufferCopy},
    device::NativeDevice,
};
use alloc::vec::Vec;
use windows::{
    Win32::{
        Foundation::E_INVALIDARG,
        Graphics::{
            Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            Direct3D11::{
                D3D11_VIEWPORT, ID3D11Buffer, ID3D11DepthStencilView, ID3D11Device,
                ID3D11DeviceContext, ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState,
                ID3D11RenderTargetView, ID3D11VertexShader,
            },
        },
    },
    core::{Error, Interface, Result},
};

/// Records validated copy commands without submitting GPU work.
#[allow(dead_code)]
pub(crate) struct CopyEncoder<'a> {
    device: &'a NativeDevice,
    copies: Vec<NativeBufferCopy>,
}

#[derive(Debug)]
pub(super) enum NativeCommand {
    Copy(NativeBufferCopy),
    BeginRender {
        target: ID3D11RenderTargetView,
        clear: Option<[f32; 4]>,
        width: u32,
        height: u32,
    },
    SetPipeline {
        vertex: ID3D11VertexShader,
        fragment: ID3D11PixelShader,
        input: ID3D11InputLayout,
        rasterizer: ID3D11RasterizerState,
    },
    SetVertex {
        buffer: ID3D11Buffer,
        offset: u32,
    },
    SetViewport(D3D11_VIEWPORT),
    Draw {
        first: u32,
        count: u32,
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

    /// The caller must hold the owning device's immediate-context mutex.
    pub(super) fn replay(&self, context: &ID3D11DeviceContext) {
        for command in &self.commands {
            unsafe {
                match command {
                    NativeCommand::Copy(copy) => copy.execute(context),
                    NativeCommand::BeginRender {
                        target,
                        clear,
                        width,
                        height,
                    } => {
                        context.ClearState();
                        context.OMSetRenderTargets(
                            Some(&[Some(target.clone())]),
                            None::<&ID3D11DepthStencilView>,
                        );
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
                        if let Some(color) = clear {
                            context.ClearRenderTargetView(target, color);
                        }
                    }
                    NativeCommand::SetPipeline {
                        vertex,
                        fragment,
                        input,
                        rasterizer,
                    } => {
                        context.RSSetState(Some(rasterizer));
                        context.IASetInputLayout(Some(input));
                        context.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
                        context.VSSetShader(Some(vertex), None);
                        context.PSSetShader(Some(fragment), None);
                    }
                    NativeCommand::SetVertex { buffer, offset } => {
                        context.IASetVertexBuffers(
                            0,
                            1,
                            Some(&Some(buffer.clone())),
                            Some(&8),
                            Some(offset),
                        );
                    }
                    NativeCommand::SetViewport(viewport) => {
                        context.RSSetViewports(Some(core::slice::from_ref(viewport)))
                    }
                    NativeCommand::Draw { first, count } => context.Draw(*count, *first),
                    NativeCommand::EndRender => context.ClearState(),
                }
            }
        }
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
        let context = self.immediate_context();
        command_buffer.replay(&context);
        Ok(())
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
    use crate::dx11::{FeatureLevel, enumerate_native_adapters};
    use alloc::vec;

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
