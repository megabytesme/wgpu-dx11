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
            if self.is_u32() {
                let read = |index: usize| {
                    core::ptr::read_unaligned(
                        mapped
                            .pData
                            .cast::<u8>()
                            .add(start + index * 4)
                            .cast::<u32>(),
                    )
                };
                let plan = plan_u32_windows(
                    read,
                    count as usize,
                    base_vertex,
                    self.max_index,
                    vertex_offset,
                    vertex_layout,
                    max_vertices,
                );
                context.Unmap(&self.staging, 0);
                let Some(plan) = plan? else {
                    return Ok(());
                };
                let mut windows = Vec::new();
                windows
                    .try_reserve_exact(plan.len())
                    .map_err(|_| Error::from_hresult(E_OUTOFMEMORY))?;
                let device = context.GetDevice()?;
                let scratch_size = stride
                    .checked_mul(3)
                    .ok_or_else(|| Error::from_hresult(E_INVALIDARG))?;
                let mut zeroes = Vec::<u8>::new();
                if plan
                    .iter()
                    .any(|segment| matches!(segment, U32Segment::Gather(_)))
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
                        U32Segment::Gather(sources) => {
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
                });
            } else {
                let indices = (0..count as usize).map(|index| {
                    core::ptr::read_unaligned(
                        mapped
                            .pData
                            .cast::<u8>()
                            .add(start + index * 2)
                            .cast::<u16>(),
                    )
                });
                let decision =
                    classify_indices(indices, base_vertex, self.max_index, vertex_layout);
                context.Unmap(&self.staging, 0);
                match decision {
                    IndexDecision::Draw => {}
                    IndexDecision::Discard => return Ok(()),
                    IndexDecision::Unsupported => return Err(Error::from_hresult(E_INVALIDARG)),
                }
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
        }
        Ok(())
    }
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
    count: u32,
    data: Vec<u8>,
}

#[derive(Debug)]
enum U32Segment {
    Window(U32Window),
    Gather([D3D11_BOX; 3]),
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
        || stride == 0
        || !stride.is_multiple_of(4)
        || extent == 0
        || !extent.is_multiple_of(4)
        || extent > stride
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
            windows.push(U32Segment::Gather(sources));
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
        classify_indices, classify_u32_indices, for_each_window, plan_u32_windows,
        rebased_vertex_offset, IndexDecision, U32Decision, U32Segment,
    };

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
        let U32Segment::Gather(boxes) = &mixed[1] else {
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
        let U32Segment::Gather(boxes) = &short[0] else {
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
