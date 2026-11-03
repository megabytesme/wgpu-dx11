//! Checked, correctness-first indexed draws for the private D3D11 HAL.
//!
//! GPU index data is copied to staging and checked at replay time, after all
//! earlier commands. This preserves command ordering but may stall the CPU.
use super::{buffer::NativeBuffer, device::NativeDevice};
use alloc::vec::Vec;
use core::num::NonZeroU64;
use windows::{
    core::{Error, Result},
    Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, E_OUTOFMEMORY},
        Graphics::{
            Direct3D11::{
                ID3D11Buffer, ID3D11DeviceContext, D3D11_BIND_INDEX_BUFFER,
                D3D11_BIND_VERTEX_BUFFER, D3D11_BOX, D3D11_BUFFER_DESC, D3D11_CPU_ACCESS_READ,
                D3D11_MAPPED_SUBRESOURCE, D3D11_MAP_READ, D3D11_SUBRESOURCE_DATA,
                D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE, D3D11_USAGE_STAGING,
            },
            Dxgi::Common::DXGI_FORMAT_R16_UINT,
        },
    },
};

#[derive(Debug)]
pub(super) struct IndexBinding {
    buffer: ID3D11Buffer,
    staging: ID3D11Buffer,
    offset: u32,
    length: u64,
    max_index: u16,
    format: wgt::IndexFormat,
}

impl NativeDevice {
    pub(super) fn prepare_index_binding(
        &self,
        buffer: &NativeBuffer,
        offset: u64,
        size: Option<NonZeroU64>,
        format: wgt::IndexFormat,
    ) -> Result<IndexBinding> {
        let (raw, length) = self.index_buffer(buffer, offset, size, format)?;
        let mut source_desc = D3D11_BUFFER_DESC::default();
        unsafe { raw.GetDesc(&mut source_desc) };
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: source_desc.ByteWidth,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.raw_device()
                .CreateBuffer(&desc, None, Some(&mut staging))?;
        }
        Ok(IndexBinding {
            buffer: raw,
            staging: staging.ok_or_else(|| Error::from_hresult(E_FAIL))?,
            offset: u32::try_from(offset).map_err(|_| Error::from_hresult(E_INVALIDARG))?,
            length,
            max_index: super::capabilities::HardwareGuarantees::for_level(self.feature_level())
                .max_vertex_index
                .min(u32::from(u16::MAX)) as u16,
            format,
        })
    }
}

impl IndexBinding {
    pub(super) fn contains_range(&self, first: u32, count: u32) -> bool {
        let end = u64::from(first) + u64::from(count);
        let size = match self.format {
            wgt::IndexFormat::Uint16 => 2,
            wgt::IndexFormat::Uint32 => 4,
        };
        end.checked_mul(size)
            .is_some_and(|bytes| bytes <= self.length)
            && u64::from(self.offset)
                .checked_add(end * size)
                .is_some_and(|bytes| bytes <= isize::MAX as u64)
    }

    pub(super) fn is_u32(&self) -> bool {
        self.format == wgt::IndexFormat::Uint32
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Indexed point replay needs stream and instance state"
    )]
    pub(super) fn draw_points(
        &self,
        context: &ID3D11DeviceContext,
        first: u32,
        count: u32,
        base: i32,
        vertices: &[(u32, ID3D11Buffer, u32, u64, u32, u32)],
        draw_limits: (u32, u32),
        instance_bindings: &super::command::InstanceBindings,
    ) -> Result<()> {
        let (max_points, instances) = draw_limits;
        instance_bindings.validate(instances)?;
        if vertices.is_empty()
            || vertices.iter().any(|(_, _, offset, _, stride, extent)| {
                !offset.is_multiple_of(4)
                    || !stride.is_multiple_of(4)
                    || *extent == 0
                    || (*stride != 0 && *extent > *stride)
                    || !extent.is_multiple_of(4)
            })
            || !self.contains_range(first, count)
            || max_points == 0
            || (count != 0 && first.checked_add(count - 1).is_none())
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        if count == 0 || instances == 0 {
            return Ok(());
        }
        let mut layouts = Vec::new();
        layouts
            .try_reserve_exact(vertices.len())
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        layouts.extend(vertices.iter().map(|v| (v.2, v.3, v.4, v.5)));
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&self.staging, &self.buffer);
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let size = if self.is_u32() { 4 } else { 2 };
            let start = self.offset as usize + first as usize * size;
            let read = |i: usize| {
                let ptr = mapped.pData.cast::<u8>().add(start + i * size);
                if self.is_u32() {
                    core::ptr::read_unaligned(ptr.cast::<u32>())
                } else {
                    u32::from(core::ptr::read_unaligned(ptr.cast::<u16>()))
                }
            };
            let plan = plan_point_runs(read, count as usize, base, max_points, &layouts);
            context.Unmap(&self.staging, 0);
            let Some(plan) = plan? else { return Ok(()) };
            for instance in 0..instances {
                instance_bindings.bind(context, instance)?;
                for run in &plan {
                    for ((slot, buffer, _, _, stride, _), offset) in
                        vertices.iter().zip(&run.offsets)
                    {
                        context.IASetVertexBuffers(
                            *slot,
                            1,
                            Some(&Some(buffer.clone())),
                            Some(stride),
                            Some(offset),
                        );
                    }
                    context.Draw(run.count, 0);
                }
            }
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Indexed line replay needs stream and instance state"
    )]
    pub(super) fn draw_lines(
        &self,
        context: &ID3D11DeviceContext,
        first: u32,
        count: u32,
        base: i32,
        vertices: &[(u32, ID3D11Buffer, u32, u64, u32, u32)],
        draw_limits: (u32, u32),
        instance_bindings: &super::command::InstanceBindings,
    ) -> Result<()> {
        let (max_indices, instances) = draw_limits;
        instance_bindings.validate(instances)?;
        if vertices.is_empty()
            || vertices.iter().any(|(_, _, offset, _, stride, extent)| {
                !offset.is_multiple_of(4)
                    || !stride.is_multiple_of(4)
                    || *extent == 0
                    || (*stride != 0 && *extent > *stride)
                    || !extent.is_multiple_of(4)
            })
            || !self.contains_range(first, count)
            || max_indices < 2
            || !max_indices.is_multiple_of(2)
            || (count != 0 && first.checked_add(count - 1).is_none())
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let complete_count = count & !1;
        if complete_count == 0 || instances == 0 {
            return Ok(());
        }

        let mut layouts = Vec::new();
        layouts
            .try_reserve_exact(vertices.len())
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        layouts.extend(vertices.iter().map(|v| (v.3, v.4, v.5)));

        enum Prepared {
            Window {
                index: ID3D11Buffer,
                offsets: Vec<u32>,
                count: u32,
            },
            Gather {
                streams: Vec<Option<(ID3D11Buffer, [D3D11_BOX; 2])>>,
            },
        }

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&self.staging, &self.buffer);
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let index_size = if self.is_u32() { 4 } else { 2 };
            let start = self.offset as usize + first as usize * index_size;
            let read = |index: usize| {
                let ptr = mapped.pData.cast::<u8>().add(start + index * index_size);
                if self.is_u32() {
                    core::ptr::read_unaligned(ptr.cast::<u32>())
                } else {
                    u32::from(core::ptr::read_unaligned(ptr.cast::<u16>()))
                }
            };
            let plan = plan_indexed_lines(
                read,
                complete_count as usize,
                base,
                self.max_index,
                max_indices,
                &layouts,
            );
            context.Unmap(&self.staging, 0);
            let Some(plan) = plan? else { return Ok(()) };
            if plan.is_empty() {
                return Ok(());
            }

            let device = context.GetDevice()?;
            let mut prepared = Vec::new();
            prepared
                .try_reserve_exact(plan.len())
                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
            for segment in plan {
                match segment {
                    IndexedLineSegment::Window {
                        minimum,
                        count,
                        indices,
                    } => {
                        let mut offsets = Vec::new();
                        offsets
                            .try_reserve_exact(vertices.len())
                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                        for (_, _, offset, length, stride, extent) in vertices {
                            offsets.push(
                                rebased_vertex_offset(
                                    minimum,
                                    *offset,
                                    (*length, *stride, *extent),
                                )
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
                            );
                        }
                        let byte_width = u32::try_from(indices.len())
                            .map_err(|_| Error::from_hresult(E_INVALIDARG))?;
                        let desc = D3D11_BUFFER_DESC {
                            ByteWidth: byte_width,
                            Usage: D3D11_USAGE_IMMUTABLE,
                            BindFlags: D3D11_BIND_INDEX_BUFFER.0 as u32,
                            ..Default::default()
                        };
                        let data = D3D11_SUBRESOURCE_DATA {
                            pSysMem: indices.as_ptr().cast(),
                            ..Default::default()
                        };
                        let mut index = None;
                        device.CreateBuffer(&desc, Some(&data), Some(&mut index))?;
                        prepared.push(Prepared::Window {
                            index: index.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                            offsets,
                            count,
                        });
                    }
                    IndexedLineSegment::Gather { indices } => {
                        let mut streams = Vec::new();
                        streams
                            .try_reserve_exact(vertices.len())
                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                        for (_, _, offset, length, stride, extent) in vertices {
                            if *stride == 0 {
                                streams.push(None);
                                continue;
                            }
                            let byte_width = stride
                                .checked_mul(2)
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            let mut zeroes = Vec::new();
                            zeroes
                                .try_reserve_exact(byte_width as usize)
                                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                            zeroes.resize(byte_width as usize, 0u8);
                            let desc = D3D11_BUFFER_DESC {
                                ByteWidth: byte_width,
                                Usage: D3D11_USAGE_DEFAULT,
                                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                                ..Default::default()
                            };
                            let initial = D3D11_SUBRESOURCE_DATA {
                                pSysMem: zeroes.as_ptr().cast(),
                                ..Default::default()
                            };
                            let mut buffer = None;
                            device.CreateBuffer(&desc, Some(&initial), Some(&mut buffer))?;
                            let source_box = |index| -> Result<D3D11_BOX> {
                                let left = rebased_vertex_offset(
                                    index,
                                    *offset,
                                    (*length, *stride, *extent),
                                )
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                let right = left
                                    .checked_add(*extent)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                Ok(D3D11_BOX {
                                    left,
                                    right,
                                    top: 0,
                                    bottom: 1,
                                    front: 0,
                                    back: 1,
                                })
                            };
                            let sources = [source_box(indices[0])?, source_box(indices[1])?];
                            streams.push(Some((
                                buffer.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                                sources,
                            )));
                        }
                        prepared.push(Prepared::Gather { streams });
                    }
                }
            }

            for segment in &prepared {
                if let Prepared::Gather { streams } = segment {
                    for ((_, source_buffer, _, _, stride, _), stream) in
                        vertices.iter().zip(streams)
                    {
                        let Some((destination, sources)) = stream else {
                            continue;
                        };
                        for (index, source_box) in sources.iter().enumerate() {
                            context.CopySubresourceRegion(
                                destination,
                                0,
                                index as u32 * stride,
                                0,
                                0,
                                source_buffer,
                                0,
                                Some(source_box),
                            );
                        }
                    }
                }
            }

            for instance in 0..instances {
                instance_bindings.bind(context, instance)?;
                for segment in &prepared {
                    match segment {
                        Prepared::Window {
                            index,
                            offsets,
                            count,
                        } => {
                            for ((slot, buffer, _, _, stride, _), offset) in
                                vertices.iter().zip(offsets)
                            {
                                context.IASetVertexBuffers(
                                    *slot,
                                    1,
                                    Some(&Some(buffer.clone())),
                                    Some(stride),
                                    Some(offset),
                                );
                            }
                            context.IASetIndexBuffer(index, DXGI_FORMAT_R16_UINT, 0);
                            context.DrawIndexed(*count, 0, 0);
                        }
                        Prepared::Gather { streams } => {
                            for ((slot, buffer, offset, _, stride, _), stream) in
                                vertices.iter().zip(streams)
                            {
                                let (buffer, offset) = match stream {
                                    Some((buffer, _)) => (buffer, &0),
                                    None => (buffer, offset),
                                };
                                context.IASetVertexBuffers(
                                    *slot,
                                    1,
                                    Some(&Some(buffer.clone())),
                                    Some(stride),
                                    Some(offset),
                                );
                            }
                            context.Draw(2, 0);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "Indexed replay needs stream and instance state"
    )]
    pub(super) fn draw_multi(
        &self,
        context: &ID3D11DeviceContext,
        first: u32,
        count: u32,
        base: i32,
        vertices: &[(u32, ID3D11Buffer, u32, u64, u32, u32)],
        draw_limits: (u32, u32),
        instance_bindings: &super::command::InstanceBindings,
    ) -> Result<()> {
        let (max_vertices, instances) = draw_limits;
        instance_bindings.validate(instances)?;
        if vertices.is_empty()
            || vertices.iter().any(|(_, _, offset, _, stride, extent)| {
                !offset.is_multiple_of(4)
                    || !stride.is_multiple_of(4)
                    || *extent == 0
                    || (*stride != 0 && *extent > *stride)
                    || !extent.is_multiple_of(4)
            })
            || !self.contains_range(first, count)
            || max_vertices == 0
            || !max_vertices.is_multiple_of(3)
            || (count != 0 && first.checked_add(count - 1).is_none())
        {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        if count == 0 || instances == 0 {
            return Ok(());
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&self.staging, &self.buffer);
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let size = if self.is_u32() { 4 } else { 2 };
            let start = self.offset as usize + first as usize * size;
            let read = |i: usize| {
                let ptr = mapped.pData.cast::<u8>().add(start + i * size);
                if self.is_u32() {
                    core::ptr::read_unaligned(ptr.cast::<u32>())
                } else {
                    u32::from(core::ptr::read_unaligned(ptr.cast::<u16>()))
                }
            };
            let mut native = !self.is_u32();
            let mut valid = true;
            for i in 0..count as usize {
                let value = read(i);
                let effective = i64::from(value) + i64::from(base);
                valid &= multi_u32_index_in_bounds(
                    value,
                    base,
                    vertices.iter().map(|v| (v.3, v.4, v.5)),
                );
                native &=
                    value <= u32::from(self.max_index) && effective <= i64::from(self.max_index);
            }
            let plan = if valid && !native {
                let first_vertex = &vertices[0];
                Some(plan_u32_windows(
                    read,
                    count as usize,
                    base,
                    self.max_index,
                    first_vertex.2,
                    (first_vertex.3, first_vertex.4, first_vertex.5),
                    max_vertices,
                ))
            } else {
                None
            };
            context.Unmap(&self.staging, 0);
            if !valid {
                return Ok(());
            }
            if native {
                for (slot, buffer, offset, _, stride, _) in vertices {
                    context.IASetVertexBuffers(
                        *slot,
                        1,
                        Some(&Some(buffer.clone())),
                        Some(stride),
                        Some(offset),
                    );
                }
                context.IASetIndexBuffer(&self.buffer, DXGI_FORMAT_R16_UINT, self.offset);
                for instance in 0..instances {
                    instance_bindings.bind(context, instance)?;
                    super::command::draw_batches(first, count, 1, max_vertices, |first, count| {
                        context.DrawIndexed(count, first, base);
                    })?;
                }
                return Ok(());
            }
            let Some(plan) = plan else {
                return Err(Error::from_hresult(E_FAIL));
            };
            let Some(plan) = plan? else { return Ok(()) };
            enum Segment {
                Window {
                    buffer: ID3D11Buffer,
                    offsets: Vec<u32>,
                    count: u32,
                },
                Gather {
                    streams: Vec<Option<(ID3D11Buffer, [D3D11_BOX; 3])>>,
                },
            }
            let device = context.GetDevice()?;
            let mut prepared = Vec::new();
            prepared
                .try_reserve_exact(plan.len())
                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
            for segment in plan {
                match segment {
                    U32Segment::Window(window) => {
                        let minimum = window.minimum;
                        let mut offsets = Vec::new();
                        offsets
                            .try_reserve_exact(vertices.len())
                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                        for (_, _, offset, length, stride, extent) in vertices {
                            offsets.push(
                                rebased_vertex_offset(
                                    minimum,
                                    *offset,
                                    (*length, *stride, *extent),
                                )
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?,
                            );
                        }
                        let desc = D3D11_BUFFER_DESC {
                            ByteWidth: window.data.len() as u32,
                            Usage: D3D11_USAGE_IMMUTABLE,
                            BindFlags: D3D11_BIND_INDEX_BUFFER.0 as u32,
                            ..Default::default()
                        };
                        let data = D3D11_SUBRESOURCE_DATA {
                            pSysMem: window.data.as_ptr().cast(),
                            ..Default::default()
                        };
                        let mut buffer = None;
                        device.CreateBuffer(&desc, Some(&data), Some(&mut buffer))?;
                        prepared.push(Segment::Window {
                            buffer: buffer.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                            offsets,
                            count: window.count,
                        });
                    }
                    U32Segment::Gather { indices, .. } => {
                        let mut streams = Vec::new();
                        streams
                            .try_reserve_exact(vertices.len())
                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                        for (_, _, offset, length, stride, extent) in vertices {
                            if *stride == 0 {
                                streams.push(None);
                                continue;
                            }
                            let mut sources = [D3D11_BOX::default(); 3];
                            for (source, &index) in sources.iter_mut().zip(&indices) {
                                let left = rebased_vertex_offset(
                                    index,
                                    *offset,
                                    (*length, *stride, *extent),
                                )
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                let right = left
                                    .checked_add(*extent)
                                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                                *source = D3D11_BOX {
                                    left,
                                    right,
                                    top: 0,
                                    bottom: 1,
                                    front: 0,
                                    back: 1,
                                };
                            }
                            let bytes = stride
                                .checked_mul(3)
                                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                            let mut zeroes = Vec::new();
                            zeroes
                                .try_reserve_exact(bytes as usize)
                                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                            zeroes.resize(bytes as usize, 0u8);
                            let desc = D3D11_BUFFER_DESC {
                                ByteWidth: bytes,
                                Usage: D3D11_USAGE_DEFAULT,
                                BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                                ..Default::default()
                            };
                            let data = D3D11_SUBRESOURCE_DATA {
                                pSysMem: zeroes.as_ptr().cast(),
                                ..Default::default()
                            };
                            let mut buffer = None;
                            device.CreateBuffer(&desc, Some(&data), Some(&mut buffer))?;
                            streams.push(Some((
                                buffer.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                                sources,
                            )));
                        }
                        prepared.push(Segment::Gather { streams });
                    }
                }
            }
            for segment in &prepared {
                if let Segment::Gather { streams } = segment {
                    for ((_, source_buffer, _, _, stride, _), stream) in
                        vertices.iter().zip(streams)
                    {
                        let Some((buffer, sources)) = stream else {
                            continue;
                        };
                        for (index, source) in sources.iter().enumerate() {
                            context.CopySubresourceRegion(
                                buffer,
                                0,
                                index as u32 * stride,
                                0,
                                0,
                                source_buffer,
                                0,
                                Some(source),
                            );
                        }
                    }
                }
            }
            for instance in 0..instances {
                instance_bindings.bind(context, instance)?;
                for segment in &prepared {
                    match segment {
                        Segment::Window {
                            buffer,
                            offsets,
                            count,
                        } => {
                            for ((slot, vertex, _, _, stride, _), offset) in
                                vertices.iter().zip(offsets)
                            {
                                context.IASetVertexBuffers(
                                    *slot,
                                    1,
                                    Some(&Some(vertex.clone())),
                                    Some(stride),
                                    Some(offset),
                                );
                            }
                            context.IASetIndexBuffer(buffer, DXGI_FORMAT_R16_UINT, 0);
                            context.DrawIndexed(*count, 0, 0);
                        }
                        Segment::Gather { streams } => {
                            for ((slot, vertex, offset, _, stride, _), stream) in
                                vertices.iter().zip(streams)
                            {
                                let (buffer, offset) = match stream {
                                    Some((buffer, _)) => (buffer, &0),
                                    None => (vertex, offset),
                                };
                                context.IASetVertexBuffers(
                                    *slot,
                                    1,
                                    Some(&Some(buffer.clone())),
                                    Some(stride),
                                    Some(offset),
                                );
                            }
                            context.Draw(3, 0);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// The caller holds the owning device's immediate-context lock.
    pub(super) fn draw(
        &self,
        context: &ID3D11DeviceContext,
        first: u32,
        count: u32,
        base_vertex: i32,
        vertex_binding: (&ID3D11Buffer, u32, u64, u32, u32),
        draw_limits: (u32, u32),
    ) -> Result<()> {
        let (vertex, vertex_offset, vertex_bytes, stride, extent) = vertex_binding;
        let (max_vertices, instances) = draw_limits;
        let vertex_layout = (vertex_bytes, stride, extent);
        if !self.contains_range(first, count) || max_vertices == 0 {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        if count == 0 || instances == 0 {
            return Ok(());
        }
        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        unsafe {
            context.CopyResource(&self.staging, &self.buffer);
            context.Map(&self.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            if mapped.pData.is_null() {
                context.Unmap(&self.staging, 0);
                return Err(Error::from_hresult(E_FAIL));
            }
            let size = if self.is_u32() { 4 } else { 2 };
            let start = self.offset as usize + first as usize * size;
            let plan = if self.is_u32() {
                let read = |index: usize| {
                    core::ptr::read_unaligned(
                        mapped
                            .pData
                            .cast::<u8>()
                            .add(start + index * 4)
                            .cast::<u32>(),
                    )
                };
                plan_u32_windows(
                    read,
                    count as usize,
                    base_vertex,
                    self.max_index,
                    vertex_offset,
                    vertex_layout,
                    max_vertices,
                )
                .map(|plan| plan.map(IndexPlan::Rewritten))
            } else {
                let read = |index: usize| {
                    core::ptr::read_unaligned(
                        mapped
                            .pData
                            .cast::<u8>()
                            .add(start + index * 2)
                            .cast::<u16>(),
                    )
                };
                plan_u16_draw(
                    read,
                    count as usize,
                    base_vertex,
                    self.max_index,
                    vertex_offset,
                    vertex_layout,
                    max_vertices,
                )
            };
            context.Unmap(&self.staging, 0);
            let Some(plan) = plan? else { return Ok(()) };
            match plan {
                IndexPlan::NativeU16 => {
                    context.IASetIndexBuffer(&self.buffer, DXGI_FORMAT_R16_UINT, self.offset);
                    super::command::draw_batches(
                        first,
                        count,
                        instances,
                        max_vertices,
                        |first, count| {
                            context.DrawIndexed(count, first, base_vertex);
                        },
                    )?;
                }
                IndexPlan::Rewritten(plan) => {
                    let mut windows = Vec::new();
                    windows
                        .try_reserve_exact(plan.len())
                        .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                    let device = context.GetDevice()?;
                    let scratch_size = stride
                        .checked_mul(3)
                        .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                    let mut zeroes = Vec::<u8>::new();
                    if stride != 0
                        && plan
                            .iter()
                            .any(|segment| matches!(segment, U32Segment::Gather { .. }))
                    {
                        zeroes
                            .try_reserve_exact(scratch_size as usize)
                            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                        zeroes.resize(scratch_size as usize, 0);
                    }
                    for segment in plan {
                        let mut buffer = None;
                        match segment {
                            U32Segment::Window(window) => {
                                let desc = D3D11_BUFFER_DESC {
                                    ByteWidth: window.data.len() as u32,
                                    Usage: D3D11_USAGE_IMMUTABLE,
                                    BindFlags: D3D11_BIND_INDEX_BUFFER.0 as u32,
                                    ..Default::default()
                                };
                                let data = D3D11_SUBRESOURCE_DATA {
                                    pSysMem: window.data.as_ptr().cast(),
                                    ..Default::default()
                                };
                                device.CreateBuffer(&desc, Some(&data), Some(&mut buffer))?;
                                windows.push(PreparedU32Segment::Window {
                                    buffer: buffer.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                                    vertex_offset: window.vertex_offset,
                                    count: window.count,
                                });
                            }
                            U32Segment::Gather { .. } if stride == 0 => {
                                windows.push(PreparedU32Segment::Constant);
                            }
                            U32Segment::Gather { sources, .. } => {
                                let desc = D3D11_BUFFER_DESC {
                                    ByteWidth: scratch_size,
                                    Usage: D3D11_USAGE_DEFAULT,
                                    BindFlags: D3D11_BIND_VERTEX_BUFFER.0 as u32,
                                    ..Default::default()
                                };
                                let data = D3D11_SUBRESOURCE_DATA {
                                    pSysMem: zeroes.as_ptr().cast(),
                                    ..Default::default()
                                };
                                device.CreateBuffer(&desc, Some(&data), Some(&mut buffer))?;
                                windows.push(PreparedU32Segment::Gather {
                                    buffer: buffer.ok_or_else(|| Error::from_hresult(E_FAIL))?,
                                    sources,
                                });
                            }
                        }
                    }
                    for segment in &windows {
                        if let PreparedU32Segment::Gather { buffer, sources } = segment {
                            for (index, source) in sources.iter().enumerate() {
                                context.CopySubresourceRegion(
                                    buffer,
                                    0,
                                    index as u32 * stride,
                                    0,
                                    0,
                                    vertex,
                                    0,
                                    Some(source),
                                );
                            }
                        }
                    }
                    for_each_window(instances, windows.len(), |index| match &windows[index] {
                        PreparedU32Segment::Window {
                            buffer,
                            vertex_offset,
                            count,
                        } => {
                            context.IASetVertexBuffers(
                                0,
                                1,
                                Some(&Some(vertex.clone())),
                                Some(&stride),
                                Some(vertex_offset),
                            );
                            context.IASetIndexBuffer(buffer, DXGI_FORMAT_R16_UINT, 0);
                            context.DrawIndexed(*count, 0, 0);
                        }
                        PreparedU32Segment::Gather { buffer, .. } => {
                            context.IASetVertexBuffers(
                                0,
                                1,
                                Some(&Some(buffer.clone())),
                                Some(&stride),
                                Some(&0),
                            );
                            context.Draw(3, 0);
                        }
                        PreparedU32Segment::Constant => {
                            context.IASetVertexBuffers(
                                0,
                                1,
                                Some(&Some(vertex.clone())),
                                Some(&stride),
                                Some(&vertex_offset),
                            );
                            context.Draw(3, 0);
                        }
                    });
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
struct PointRun {
    count: u32,
    offsets: Vec<u32>,
}

fn plan_point_runs(
    read: impl Fn(usize) -> u32,
    count: usize,
    base: i32,
    max_points: u32,
    layouts: &[(u32, u64, u32, u32)],
) -> Result<Option<Vec<PointRun>>> {
    if max_points == 0 || layouts.is_empty() {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    for i in 0..count {
        if !multi_u32_index_in_bounds(
            read(i),
            base,
            layouts
                .iter()
                .map(|&(_, length, stride, extent)| (length, stride, extent)),
        ) {
            return Ok(None);
        }
    }
    let mut runs = Vec::new();
    let mut start = 0;
    while start < count {
        let first = (i64::from(read(start)) + i64::from(base)) as u64;
        let mut end = start + 1;
        while end < count && end - start < max_points as usize {
            let current = (i64::from(read(end)) + i64::from(base)) as u64;
            if first.checked_add((end - start) as u64) != Some(current) {
                break;
            }
            end += 1;
        }
        let last = first
            .checked_add((end - start - 1) as u64)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let mut offsets = Vec::new();
        offsets
            .try_reserve_exact(layouts.len())
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        for &(offset, length, stride, extent) in layouts {
            let layout = (length, stride, extent);
            let first_offset = rebased_vertex_offset(first, offset, layout)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            rebased_vertex_offset(last, offset, layout)
                .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
            offsets.push(first_offset);
        }
        runs.try_reserve(1)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        runs.push(PointRun {
            count: (end - start) as u32,
            offsets,
        });
        start = end;
    }
    Ok(Some(runs))
}

fn multi_u32_index_in_bounds(
    value: u32,
    base: i32,
    layouts: impl Iterator<Item = (u64, u32, u32)>,
) -> bool {
    let Ok(index) = u64::try_from(i64::from(value) + i64::from(base)) else {
        return false;
    };
    layouts.into_iter().all(|(length, stride, extent)| {
        index
            .checked_mul(u64::from(stride))
            .and_then(|end| end.checked_add(u64::from(extent)))
            .is_some_and(|end| end <= length)
    })
}

#[derive(Debug)]
enum IndexedLineSegment {
    Window {
        minimum: u64,
        count: u32,
        indices: Vec<u8>,
    },
    Gather {
        indices: [u64; 2],
    },
}

fn plan_indexed_lines(
    read: impl Fn(usize) -> u32,
    count: usize,
    base: i32,
    max_index: u16,
    max_indices: u32,
    layouts: &[(u64, u32, u32)],
) -> Result<Option<Vec<IndexedLineSegment>>> {
    if layouts.is_empty()
        || max_indices < 2
        || !max_indices.is_multiple_of(2)
        || layouts.iter().any(|&(length, stride, extent)| {
            length < u64::from(extent)
                || !stride.is_multiple_of(4)
                || extent == 0
                || !extent.is_multiple_of(4)
                || (stride != 0 && extent > stride)
        })
    {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    let complete = count & !1;
    if complete == 0 {
        return Ok(Some(Vec::new()));
    }
    if (0..complete).any(|i| !multi_u32_index_in_bounds(read(i), base, layouts.iter().copied())) {
        return Ok(None);
    }
    let effective = |i| (i64::from(read(i)) + i64::from(base)) as u64;
    let mut segments = Vec::new();
    let mut start = 0;
    while start < complete {
        let pair = [effective(start), effective(start + 1)];
        let mut minimum = pair[0].min(pair[1]);
        let mut maximum = pair[0].max(pair[1]);
        if maximum - minimum > u64::from(max_index) {
            segments
                .try_reserve(1)
                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
            segments.push(IndexedLineSegment::Gather { indices: pair });
            start += 2;
            continue;
        }
        let mut end = start + 2;
        while end < complete && end - start < max_indices as usize {
            let next = [effective(end), effective(end + 1)];
            let pair_minimum = next[0].min(next[1]);
            let pair_maximum = next[0].max(next[1]);
            if pair_maximum - pair_minimum > u64::from(max_index) {
                break;
            }
            let next_minimum = minimum.min(pair_minimum);
            let next_maximum = maximum.max(pair_maximum);
            if next_maximum - next_minimum > u64::from(max_index) {
                break;
            }
            minimum = next_minimum;
            maximum = next_maximum;
            end += 2;
        }
        let count = u32::try_from(end - start).map_err(|_| Error::from_hresult(E_INVALIDARG))?;
        let size = (end - start)
            .checked_mul(2)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let mut indices = Vec::new();
        indices
            .try_reserve_exact(size)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        indices.resize(size, 0);
        for i in start..end {
            let index = u16::try_from(effective(i) - minimum)
                .map_err(|_| Error::from_hresult(E_INVALIDARG))?;
            let offset = (i - start) * 2;
            indices[offset..offset + 2].copy_from_slice(&index.to_ne_bytes());
        }
        segments
            .try_reserve(1)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        segments.push(IndexedLineSegment::Window {
            minimum,
            count,
            indices,
        });
        start = end;
    }
    Ok(Some(segments))
}

fn for_each_window(instances: u32, windows: usize, mut draw: impl FnMut(usize)) {
    for _ in 0..instances {
        for window in 0..windows {
            draw(window);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IndexDecision {
    Draw,
    Discard,
    Unsupported,
}

#[derive(Debug)]
enum IndexPlan {
    NativeU16,
    Rewritten(Vec<U32Segment>),
}

fn plan_u16_draw(
    read: impl Fn(usize) -> u16,
    count: usize,
    base_vertex: i32,
    max_index: u16,
    vertex_offset: u32,
    vertex_layout: (u64, u32, u32),
    max_vertices: u32,
) -> Result<Option<IndexPlan>> {
    match classify_indices((0..count).map(&read), base_vertex, max_index, vertex_layout) {
        IndexDecision::Draw => Ok(Some(IndexPlan::NativeU16)),
        IndexDecision::Discard => Ok(None),
        IndexDecision::Unsupported => plan_u32_windows(
            |index| u32::from(read(index)),
            count,
            base_vertex,
            max_index,
            vertex_offset,
            vertex_layout,
            max_vertices,
        )
        .map(|plan| plan.map(IndexPlan::Rewritten)),
    }
}

fn classify_indices(
    indices: impl Iterator<Item = u16>,
    base_vertex: i32,
    max_index: u16,
    vertex_layout: (u64, u32, u32),
) -> IndexDecision {
    let (vertex_bytes, stride, extent) = vertex_layout;
    let mut decision = IndexDecision::Draw;
    for value in indices {
        let effective = i64::from(value) + i64::from(base_vertex);
        let end = u64::try_from(effective).ok().and_then(|index| {
            index
                .checked_mul(u64::from(stride))
                .and_then(|bytes| bytes.checked_add(u64::from(extent)))
        });
        if end.is_none_or(|end| end > vertex_bytes) {
            decision = IndexDecision::Discard;
        } else if decision == IndexDecision::Draw
            && (value > max_index || effective > i64::from(max_index))
        {
            decision = IndexDecision::Unsupported;
        }
    }
    decision
}

fn classify_u32_indices(
    indices: impl Iterator<Item = u32>,
    base_vertex: i32,
    max_index: u16,
    vertex_layout: (u64, u32, u32),
) -> U32Decision {
    let (vertex_bytes, stride, extent) = vertex_layout;
    let mut out_of_bounds = false;
    let mut minimum = u64::MAX;
    let mut maximum = 0u64;
    for value in indices {
        let effective = i64::from(value) + i64::from(base_vertex);
        let end = u64::try_from(effective).ok().and_then(|index| {
            index
                .checked_mul(u64::from(stride))
                .and_then(|bytes| bytes.checked_add(u64::from(extent)))
        });
        if end.is_none_or(|end| end > vertex_bytes) {
            out_of_bounds = true;
        } else {
            let effective = effective as u64;
            minimum = minimum.min(effective);
            maximum = maximum.max(effective);
        }
    }
    if out_of_bounds {
        U32Decision::Discard
    } else if minimum == u64::MAX {
        U32Decision::Draw(0)
    } else if maximum - minimum > u64::from(max_index) {
        U32Decision::Unsupported
    } else {
        U32Decision::Draw(minimum)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum U32Decision {
    Draw(u64),
    Discard,
    Unsupported,
}

#[derive(Debug)]
struct U32Window {
    vertex_offset: u32,
    minimum: u64,
    count: u32,
    data: Vec<u8>,
}

#[derive(Debug)]
enum U32Segment {
    Window(U32Window),
    Gather {
        sources: [D3D11_BOX; 3],
        indices: [u64; 3],
    },
}

#[derive(Debug)]
enum PreparedU32Segment {
    Window {
        buffer: ID3D11Buffer,
        vertex_offset: u32,
        count: u32,
    },
    Gather {
        buffer: ID3D11Buffer,
        sources: [D3D11_BOX; 3],
    },
    Constant,
}

fn plan_u32_windows(
    read: impl Fn(usize) -> u32,
    count: usize,
    base_vertex: i32,
    max_index: u16,
    vertex_offset: u32,
    vertex_layout: (u64, u32, u32),
    max_vertices: u32,
) -> Result<Option<Vec<U32Segment>>> {
    let (_, stride, extent) = vertex_layout;
    if max_vertices == 0
        || !max_vertices.is_multiple_of(3)
        || !stride.is_multiple_of(4)
        || extent == 0
        || !extent.is_multiple_of(4)
        || (stride != 0 && extent > stride)
    {
        return Err(Error::from_hresult(E_INVALIDARG));
    }
    if classify_u32_indices((0..count).map(&read), base_vertex, max_index, vertex_layout)
        == U32Decision::Discard
    {
        return Ok(None);
    }
    let complete = count - count % 3;
    let mut windows = Vec::new();
    let mut start = 0;
    while start < complete {
        let mut triangle = [0u64; 3];
        for (index, value) in triangle.iter_mut().enumerate() {
            *value = (i64::from(read(start + index)) + i64::from(base_vertex)) as u64;
        }
        let triangle_min = *triangle.iter().min().unwrap();
        let triangle_max = *triangle.iter().max().unwrap();
        if triangle_max - triangle_min > u64::from(max_index) {
            let mut sources = [D3D11_BOX::default(); 3];
            for (source, effective) in sources.iter_mut().zip(triangle) {
                let left = rebased_vertex_offset(effective, vertex_offset, vertex_layout)
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                let right = left
                    .checked_add(extent)
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                *source = D3D11_BOX {
                    left,
                    right,
                    top: 0,
                    bottom: 1,
                    front: 0,
                    back: 1,
                };
            }
            windows
                .try_reserve(1)
                .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
            windows.push(U32Segment::Gather {
                sources,
                indices: triangle,
            });
            start += 3;
            continue;
        }
        let mut end = start;
        let mut minimum = u64::MAX;
        let mut maximum = 0;
        while end < complete && end - start < max_vertices as usize {
            let mut triangle_min = u64::MAX;
            let mut triangle_max = 0;
            for index in end..end + 3 {
                let value = (i64::from(read(index)) + i64::from(base_vertex)) as u64;
                triangle_min = triangle_min.min(value);
                triangle_max = triangle_max.max(value);
            }
            if triangle_max - triangle_min > u64::from(max_index) {
                break;
            }
            let next_min = minimum.min(triangle_min);
            let next_max = maximum.max(triangle_max);
            if end != start && next_max - next_min > u64::from(max_index) {
                break;
            }
            minimum = next_min;
            maximum = next_max;
            end += 3;
        }
        let offset = rebased_vertex_offset(minimum, vertex_offset, vertex_layout)
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let size = (end - start)
            .checked_mul(2)
            .and_then(|n| n.checked_add(3))
            .map(|n| n & !3)
            .filter(|&n| u32::try_from(n).is_ok())
            .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
        let mut data = Vec::new();
        data.try_reserve_exact(size)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        data.resize(size, 0);
        for index in start..end {
            let effective = (i64::from(read(index)) + i64::from(base_vertex)) as u64;
            let value = (effective - minimum) as u16;
            let pos = (index - start) * 2;
            data[pos..pos + 2].copy_from_slice(&value.to_ne_bytes());
        }
        windows
            .try_reserve(1)
            .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
        windows.push(U32Segment::Window(U32Window {
            vertex_offset: offset,
            minimum,
            count: (end - start) as u32,
            data,
        }));
        start = end;
    }
    Ok(Some(windows))
}

fn rebased_vertex_offset(minimum: u64, offset: u32, vertex_layout: (u64, u32, u32)) -> Option<u32> {
    let (vertex_bytes, stride, extent) = vertex_layout;
    let relative = minimum.checked_mul(u64::from(stride))?;
    if relative.checked_add(u64::from(extent))? > vertex_bytes {
        return None;
    }
    let result = u64::from(offset)
        .checked_add(relative)
        .and_then(|value| u32::try_from(value).ok())?;
    result.is_multiple_of(4).then_some(result)
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::{
        classify_indices, classify_u32_indices, for_each_window, multi_u32_index_in_bounds,
        plan_indexed_lines, plan_u16_draw, plan_u32_windows, rebased_vertex_offset, IndexDecision,
        IndexPlan, IndexedLineSegment, U32Decision, U32Segment,
    };

    #[test]
    fn indexed_line_windows_preserve_pairs_rebase_and_ignore_an_incomplete_tail() {
        let indices = [70_000, 70_001, 70_002, 70_003, 17];
        let plan = plan_indexed_lines(
            |i| indices[i],
            indices.len(),
            0,
            65_534,
            131_070,
            &[(70_004 * 8, 8, 8)],
        )
        .unwrap()
        .unwrap();
        assert_eq!(plan.len(), 1);
        let IndexedLineSegment::Window {
            minimum,
            count,
            indices,
        } = &plan[0]
        else {
            panic!("expected a rebased line-list window")
        };
        assert_eq!(*minimum, 70_000);
        assert_eq!(*count, 4);
        assert_eq!(indices, &[0, 0, 1, 0, 2, 0, 3, 0]);
    }

    #[test]
    fn indexed_line_plan_gathers_wide_pairs_without_reordering() {
        let indices = [0, 65_535, 65_536, 65_537];
        let plan = plan_indexed_lines(
            |i| indices[i],
            indices.len(),
            0,
            65_534,
            131_070,
            &[(65_538 * 8, 8, 8)],
        )
        .unwrap()
        .unwrap();
        assert!(matches!(
            &plan[0],
            IndexedLineSegment::Gather {
                indices: [0, 65_535]
            }
        ));
        assert!(
            matches!(
                &plan[1],
                IndexedLineSegment::Window {
                    minimum: 65_536,
                    count: 2,
                    indices,
                } if indices == &[0, 0, 1, 0]
            ),
            "{plan:?}"
        );
    }

    #[test]
    fn indexed_line_plans_discard_bad_fetches_and_split_at_primitive_limit() {
        let invalid = [0, 1, 3, 2];
        assert!(plan_indexed_lines(
            |i| invalid[i],
            invalid.len(),
            0,
            65_534,
            131_070,
            &[(3 * 8, 8, 8)],
        )
        .unwrap()
        .is_none());

        let indices: Vec<_> = (0..131_072).map(|i| i & 1).collect();
        let plan = plan_indexed_lines(
            |i| indices[i],
            indices.len(),
            0,
            65_534,
            131_070,
            &[(16, 8, 8)],
        )
        .unwrap()
        .unwrap();
        let counts: Vec<_> = plan
            .iter()
            .map(|segment| match segment {
                IndexedLineSegment::Window { count, .. } => *count,
                IndexedLineSegment::Gather { .. } => panic!("narrow lines do not gather"),
            })
            .collect();
        assert_eq!(counts, [131_070, 2]);
    }

    #[test]
    fn indexed_point_runs_preserve_order_and_rebase_every_stream() {
        let indices = [7, 8, 8, 3, 100_000, 100_001];
        let layouts = [
            (16, 2_000_000, 16, 8),
            (28, 4_000_000, 32, 16),
            (8, 4, 0, 4),
        ];
        let runs = super::plan_point_runs(|i| indices[i], indices.len(), -3, 65_535, &layouts)
            .unwrap()
            .unwrap();
        assert_eq!(
            runs.iter().map(|run| run.count).collect::<Vec<_>>(),
            [2, 1, 1, 2]
        );
        assert_eq!(runs[0].offsets, [80, 156, 8]);
        assert_eq!(runs[1].offsets, [96, 188, 8]);
        assert_eq!(runs[2].offsets, [16, 28, 8]);
        assert_eq!(runs[3].offsets, [1_599_968, 3_199_932, 8]);
    }

    #[test]
    fn indexed_point_runs_split_at_native_primitive_limit() {
        let layouts = [(0, 65_537 * 4, 4, 4)];
        let runs = super::plan_point_runs(|i| i as u32, 65_537, 0, 65_535, &layouts)
            .unwrap()
            .unwrap();
        assert_eq!(
            runs.iter().map(|run| run.count).collect::<Vec<_>>(),
            [65_535, 2]
        );
        assert_eq!(runs[0].offsets, [0]);
        assert_eq!(runs[1].offsets, [262_140]);
    }

    #[test]
    fn indexed_point_runs_discard_full_draw_for_any_invalid_index() {
        let layouts = [(0, 16, 4, 4), (8, 4, 0, 4)];
        for (indices, base) in [([0, 1, 4], 0), ([1, 2, 0], -1)] {
            assert!(
                super::plan_point_runs(|i| indices[i], indices.len(), base, 65_535, &layouts)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(super::plan_point_runs(|_| 0, 1, 0, 0, &layouts).is_err());
        assert!(super::plan_point_runs(|_| 0, 1, 0, 1, &[]).is_err());
        assert!(super::plan_point_runs(|_| 1, 1, 0, 1, &[(u32::MAX - 3, 8, 4, 4)]).is_err());
    }

    #[test]
    fn multi_stream_index_bounds_check_each_range_and_signed_base() {
        let layout = |upper: u64, stride: u32, extent: u32| {
            (
                upper * u64::from(stride) + u64::from(extent),
                stride,
                extent,
            )
        };
        let layouts = [layout(70000, 12, 8), layout(65535, 20, 16)];
        for (index, base, expected) in [
            (0, 0, true),
            (65534, 0, true),
            (65535, 0, true),
            (65536, 0, false),
            (65536, -1, true),
            (0, -1, false),
            (0, i32::MIN, false),
            (u32::MAX, i32::MAX, false),
            (u32::MAX, i32::MIN, false),
        ] {
            assert_eq!(
                multi_u32_index_in_bounds(index, base, layouts.into_iter()),
                expected,
                "index={index}, base={base}"
            );
        }
        assert!(![0, 1, 2, 65536]
            .into_iter()
            .all(|index| { multi_u32_index_in_bounds(index, 0, layouts.into_iter()) }));
        assert!(!multi_u32_index_in_bounds(0, 0, [(3, 4, 4)].into_iter()));
        assert!(!multi_u32_index_in_bounds(
            u32::MAX,
            i32::MAX,
            [(u64::MAX, u32::MAX, u32::MAX)].into_iter()
        ));
    }

    #[test]
    fn zero_stride_index_windows_preserve_constant_source() {
        let layout = (8, 0, 8);
        assert!(multi_u32_index_in_bounds(u32::MAX, 0, [layout].into_iter()));
        assert!(!multi_u32_index_in_bounds(0, -1, [layout].into_iter()));
        assert!(!multi_u32_index_in_bounds(0, 0, [(7, 0, 8)].into_iter()));
        let advancing = (65536 * 12, 12, 12);
        assert!(multi_u32_index_in_bounds(
            65535,
            0,
            [layout, advancing].into_iter()
        ));
        assert!(multi_u32_index_in_bounds(
            65535,
            0,
            [advancing, layout].into_iter()
        ));
        assert!(!multi_u32_index_in_bounds(
            65536,
            0,
            [layout, advancing].into_iter()
        ));
        assert_eq!(rebased_vertex_offset(65535, 8, layout), Some(8));
        let window_indices = [65535u32, 65536, 65537];
        let window = plan_u32_windows(|i| window_indices[i], 3, 0, 65534, 8, layout, 196605)
            .unwrap()
            .unwrap();
        let U32Segment::Window(window) = &window[0] else {
            panic!("expected window")
        };
        assert_eq!((window.minimum, window.vertex_offset), (65535, 8));
        assert_eq!(window.data, [0, 0, 1, 0, 2, 0, 0, 0]);
        let wide_indices = [0u32, 1, 65535];
        let wide = plan_u32_windows(|i| wide_indices[i], 3, 0, 65534, 8, layout, 196605)
            .unwrap()
            .unwrap();
        let U32Segment::Gather { sources, indices } = &wide[0] else {
            panic!("expected gather")
        };
        assert_eq!(*indices, [0, 1, 65535]);
        assert!(sources
            .iter()
            .all(|source| (source.left, source.right) == (8, 16)));
        assert!(
            plan_u32_windows(|i| wide_indices[i], 3, -1, 65534, 8, layout, 196605)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn u16_rewrites_only_when_native_indices_exceed_fl9_limits() {
        let layout = (70000 * 8, 8, 8);
        let plan = |indices: &[u16], base, offset, layout| {
            plan_u16_draw(
                |i| indices[i],
                indices.len(),
                base,
                65534,
                offset,
                layout,
                196605,
            )
        };
        assert!(matches!(
            plan(&[0, 1, 2], 0, 8, layout).unwrap(),
            Some(IndexPlan::NativeU16)
        ));
        let Some(IndexPlan::Rewritten(segments)) =
            plan(&[65533, 65534, 65535], -65533, 8, layout).unwrap()
        else {
            panic!("expected rebased u16")
        };
        let U32Segment::Window(window) = &segments[0] else {
            panic!("expected index window")
        };
        assert_eq!(window.vertex_offset, 8);
        assert_eq!(window.data, [0, 0, 1, 0, 2, 0, 0, 0]);
        let Some(IndexPlan::Rewritten(segments)) = plan(&[0, 1, 2], 65535, 8, layout).unwrap()
        else {
            panic!("expected shifted u16")
        };
        let U32Segment::Window(window) = &segments[0] else {
            panic!("expected index window")
        };
        assert_eq!(window.vertex_offset, 8 + 65535 * 8);
        let Some(IndexPlan::Rewritten(segments)) = plan(&[0, 1, 65535], 0, 8, layout).unwrap()
        else {
            panic!("expected gathered u16")
        };
        assert!(matches!(segments[0], U32Segment::Gather { .. }));
        assert!(plan(&[65535, 0, 1], -1, 8, layout).unwrap().is_none());
        assert!(plan(&[0, 1, 2, 0, 1, 65535], 0, 8, (100 * 8, 8, 8))
            .unwrap()
            .is_none());
        assert!(plan(&[0, 1, 2], 65535, u32::MAX - 3, layout).is_err());
    }

    #[test]
    fn indexed_instance_windows_remain_in_primitive_order() {
        let mut drawn = Vec::new();
        for_each_window(2, 3, |window| drawn.push(window));
        assert_eq!(drawn, [0, 1, 2, 0, 1, 2]);
        drawn.clear();
        for_each_window(0, 3, |window| drawn.push(window));
        for_each_window(2, 0, |window| drawn.push(window));
        assert!(drawn.is_empty());
    }

    #[test]
    fn u32_windows_preflight_whole_draw_and_preserve_triangle_order() {
        let plan = |indices: &[u32], max_vertices| {
            plan_u32_windows(
                |i| indices[i],
                indices.len(),
                0,
                65534,
                8,
                (70000 * 8, 8, 8),
                max_vertices,
            )
        };
        let indices = [0, 1, 2, 65538, 65539, 65540, 3, 4, 5];
        let windows = plan(&indices, 196605).unwrap().unwrap();
        assert_eq!(windows.len(), 3);
        let windows: Vec<_> = windows
            .iter()
            .map(|segment| match segment {
                U32Segment::Window(window) => window,
                _ => panic!("expected index window"),
            })
            .collect();
        assert_eq!(
            windows.iter().map(|w| w.vertex_offset).collect::<Vec<_>>(),
            [8, 8 + 65538 * 8, 8 + 3 * 8]
        );
        for window in windows {
            assert_eq!(window.count, 3);
            assert_eq!(window.data, [0, 0, 1, 0, 2, 0, 0, 0]);
        }
        assert_eq!(plan(&[0, 1, 65534], 196605).unwrap().unwrap().len(), 1);
        let mixed = plan(&[0, 1, 2, 0, 1, 65535, 3, 4, 5], 196605)
            .unwrap()
            .unwrap();
        assert!(matches!(mixed[0], U32Segment::Window(_)));
        let U32Segment::Gather { sources: boxes, .. } = &mixed[1] else {
            panic!("expected gather")
        };
        assert_eq!(
            boxes.iter().map(|b| (b.left, b.right)).collect::<Vec<_>>(),
            [(8, 16), (16, 24), (8 + 65535 * 8, 16 + 65535 * 8)]
        );
        assert!(matches!(mixed[2], U32Segment::Window(_)));
        assert!(plan(&[0, 1, 65535, 70000, 1, 2], 196605).unwrap().is_none());
        assert_eq!(plan(&[0, 1, 2, 65535], 196605).unwrap().unwrap().len(), 1);
        assert!(plan(&[0, 1, 2, 70000], 196605).unwrap().is_none());
        assert!(plan(&[], 196605).unwrap().unwrap().is_empty());
        assert!(plan(&[0, 1], 196605).unwrap().unwrap().is_empty());
        assert_eq!(plan(&[0, 1, 2, 3, 4, 5], 3).unwrap().unwrap().len(), 2);
        assert!(plan(&indices, 4).is_err());
        assert!(plan_u32_windows(
            |i| indices[i],
            indices.len(),
            0,
            65534,
            u32::MAX - 3,
            (70000 * 8, 8, 8),
            196605
        )
        .is_err());
        let short = plan_u32_windows(
            |i| [1u32, 2, 65536][i],
            3,
            -1,
            65534,
            8,
            (65535 * 16 + 8, 16, 8),
            196605,
        )
        .unwrap()
        .unwrap();
        let U32Segment::Gather { sources: boxes, .. } = &short[0] else {
            panic!("expected gather")
        };
        assert_eq!(
            (boxes[2].left, boxes[2].right),
            (8 + 65535 * 16, 16 + 65535 * 16)
        );
        assert!(plan_u32_windows(
            |i| [0u32, 1, 65535][i],
            3,
            0,
            65534,
            u32::MAX - 3,
            (65536 * 8, 8, 8),
            196605
        )
        .is_err());
        assert!(plan_u32_windows(|_| 0, 3, 0, 65534, 0, (64, 8, 12), 196605).is_err());
    }

    #[test]
    fn u32_conversion_checks_effective_indices_and_whole_draw() {
        let large = (u64::MAX, 1, 1);
        assert_eq!(
            classify_u32_indices([0, 65534].into_iter(), 0, 65534, large),
            U32Decision::Draw(0)
        );
        assert_eq!(
            classify_u32_indices([65535].into_iter(), 0, 65534, large),
            U32Decision::Draw(65535)
        );
        assert_eq!(
            classify_u32_indices([65535].into_iter(), -1, 65534, large),
            U32Decision::Draw(65534)
        );
        assert_eq!(
            classify_u32_indices([u32::MAX].into_iter(), i32::MIN, 65534, large),
            U32Decision::Draw(2147483647)
        );
        assert_eq!(
            classify_u32_indices([0].into_iter(), i32::MIN, 65534, large),
            U32Decision::Discard
        );
        assert_eq!(
            classify_u32_indices([65535, 0].into_iter(), -1, 65534, large),
            U32Decision::Discard
        );
        assert_eq!(
            classify_u32_indices([0, 65535].into_iter(), -1, 65534, large),
            U32Decision::Discard
        );
        assert_eq!(
            classify_u32_indices([3].into_iter(), 0, 65534, (24, 8, 8)),
            U32Decision::Discard
        );
        assert_eq!(
            classify_u32_indices([2].into_iter(), 0, 65534, (24, 8, 8)),
            U32Decision::Draw(2)
        );
        assert_eq!(
            classify_u32_indices([100000, 165534].into_iter(), 0, 65534, large),
            U32Decision::Draw(100000)
        );
        assert_eq!(
            classify_u32_indices([100000, 165535].into_iter(), 0, 65534, large),
            U32Decision::Unsupported
        );
        assert_eq!(
            classify_u32_indices([0, 65535, u32::MAX].into_iter(), 0, 65534, (65536, 1, 1)),
            U32Decision::Discard
        );
        assert_eq!(
            rebased_vertex_offset(65535, 8, (65540 * 8, 8, 8)),
            Some(8 + 65535 * 8)
        );
        assert_eq!(rebased_vertex_offset(9, 8, (72, 8, 8)), None);
        assert_eq!(rebased_vertex_offset(u64::MAX, 8, (u64::MAX, 8, 8)), None);
        assert_eq!(rebased_vertex_offset(1, u32::MAX - 3, (16, 8, 8)), None);
        assert_eq!(rebased_vertex_offset(0, 2, (16, 8, 8)), None);
    }

    #[test]
    fn base_vertex_checks_effective_index_and_binding_range() {
        let layout = (72, 8, 8);
        for (index, base) in [(0, 3), (2, 3), (6, -3), (8, -3), (0, 0)] {
            assert_eq!(
                classify_indices([index].into_iter(), base, 65534, layout),
                IndexDecision::Draw
            );
        }
        for (index, base) in [(0, -1), (0, i32::MIN), (0, i32::MAX), (9, 0)] {
            assert_eq!(
                classify_indices([index].into_iter(), base, 65534, layout),
                IndexDecision::Discard
            );
        }
        assert_eq!(
            classify_indices([65535].into_iter(), -65535, 65534, layout),
            IndexDecision::Unsupported
        );
        assert_eq!(
            classify_indices([65534].into_iter(), 1, 65534, (u64::MAX, 1, 1)),
            IndexDecision::Unsupported
        );
        assert_eq!(
            classify_indices([65534].into_iter(), 0, 65534, (65535, 1, 1)),
            IndexDecision::Draw
        );
        assert_eq!(
            classify_indices([2].into_iter(), 3, 65534, (47, 8, 8)),
            IndexDecision::Discard
        );
        assert_eq!(
            classify_indices([2].into_iter(), 3, 65534, (48, 8, 8)),
            IndexDecision::Draw
        );
        assert_eq!(
            classify_indices([65535, 0].into_iter(), -1, 65534, (u64::MAX, 1, 1)),
            IndexDecision::Discard
        );
        assert_eq!(
            classify_indices([0, 65535].into_iter(), -1, 65534, (u64::MAX, 1, 1)),
            IndexDecision::Discard
        );
        assert_eq!(
            classify_indices(core::iter::empty(), i32::MIN, 65534, layout),
            IndexDecision::Draw
        );
    }
}
