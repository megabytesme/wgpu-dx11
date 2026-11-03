use core::ops::Range;
use windows::{
    core::Result,
    Win32::{
        Foundation::E_FAIL,
        Graphics::Direct3D11::{
            ID3D11Buffer, D3D11_BIND_CONSTANT_BUFFER, D3D11_BUFFER_DESC, D3D11_USAGE_DEFAULT,
            D3D11_VIEWPORT,
        },
    },
};

pub(super) fn create_buffer(device: &super::device::NativeDevice) -> Result<ID3D11Buffer> {
    let desc = D3D11_BUFFER_DESC {
        ByteWidth: 16,
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
        ..Default::default()
    };
    let mut buffer = None;
    unsafe {
        device
            .raw_device()
            .CreateBuffer(&desc, None, Some(&mut buffer))?;
    }
    buffer.ok_or_else(|| windows::core::Error::from_hresult(E_FAIL))
}

pub(super) struct Viewport {
    pub native: D3D11_VIEWPORT,
    pub transform: [f32; 4],
}

pub(super) fn translate(
    rect: &crate::Rect<f32>,
    depth: Range<f32>,
    extent: [u32; 2],
    max_dimension: u32,
) -> Option<Viewport> {
    let [x, y, width, height] = [rect.x, rect.y, rect.w, rect.h];
    let limit = max_dimension as f32;
    if ![x, y, width, height, depth.start, depth.end]
        .iter()
        .all(|v| v.is_finite())
        || width < 0.0
        || height < 0.0
        || width > limit
        || height > limit
        || x < -2.0 * limit
        || y < -2.0 * limit
        || x + width > 2.0 * limit - 1.0
        || y + height > 2.0 * limit - 1.0
        || !(0.0..=1.0).contains(&depth.start)
        || !(0.0..=1.0).contains(&depth.end)
        || depth.start > depth.end
    {
        return None;
    }
    let [x, y, width, height] = [x, y, width, height].map(|value| f64::from(value.round()));
    // The native viewport must not extend past the logical clip volume.
    let left = x.max(0.0).min(f64::from(extent[0]));
    let top = y.max(0.0).min(f64::from(extent[1]));
    let right = (x + width).max(0.0).min(f64::from(extent[0]));
    let bottom = (y + height).max(0.0).min(f64::from(extent[1]));
    let clipped_width = (right - left).max(0.0);
    let clipped_height = (bottom - top).max(0.0);
    let transform = if clipped_width == 0.0 || clipped_height == 0.0 {
        [1.0, 1.0, 0.0, 0.0]
    } else {
        [
            (width / clipped_width) as f32,
            (height / clipped_height) as f32,
            ((2.0 * (x - left) + width - clipped_width) / clipped_width) as f32,
            ((clipped_height - 2.0 * (y - top) - height) / clipped_height) as f32,
        ]
    };
    Some(Viewport {
        native: D3D11_VIEWPORT {
            TopLeftX: left as f32,
            TopLeftY: top as f32,
            Width: clipped_width as f32,
            Height: clipped_height as f32,
            MinDepth: depth.start,
            MaxDepth: depth.end,
        },
        transform,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_maps_viewport_without_expanding_clip_bounds() {
        let viewport = translate(
            &crate::Rect {
                x: -4.,
                y: -2.,
                w: 8.,
                h: 8.,
            },
            0.0..1.0,
            [8, 8],
            2048,
        )
        .unwrap();
        assert_eq!(
            [
                viewport.native.TopLeftX,
                viewport.native.TopLeftY,
                viewport.native.Width,
                viewport.native.Height
            ],
            [0., 0., 4., 6.]
        );
        assert_eq!(viewport.transform, [2., 8. / 6., -1., 2. / 6.]);
        let empty = translate(
            &crate::Rect {
                x: -20.,
                y: 0.,
                w: 4.,
                h: 8.,
            },
            0.0..1.0,
            [8, 8],
            2048,
        )
        .unwrap();
        assert_eq!(empty.native.Width, 0.);
        assert_eq!(empty.transform, [1., 1., 0., 0.]);
        assert!(translate(
            &crate::Rect {
                x: f32::NAN,
                y: 0.,
                w: 4.,
                h: 8.
            },
            0.0..1.0,
            [8, 8],
            2048
        )
        .is_none());
    }
}
