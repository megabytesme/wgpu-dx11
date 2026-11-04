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
    let [x, y, width, height] = [x, y, width, height].map(f64::from);
    let left = x.max(0.0).min(f64::from(extent[0]));
    let top = y.max(0.0).min(f64::from(extent[1]));
    let clipped_right = (x + width).max(0.0).min(f64::from(extent[0]));
    let clipped_bottom = (y + height).max(0.0).min(f64::from(extent[1]));
    let native_left = left.floor();
    let native_top = top.floor();
    let native_right = if width > 0.0 && clipped_right > left {
        clipped_right.ceil()
    } else {
        native_left
    };
    let native_bottom = if height > 0.0 && clipped_bottom > top {
        clipped_bottom.ceil()
    } else {
        native_top
    };
    let clipped_width = native_right - native_left;
    let clipped_height = native_bottom - native_top;
    let transform = if clipped_width == 0.0 || clipped_height == 0.0 {
        [1.0, 1.0, 0.0, 0.0]
    } else {
        [
            (width / clipped_width) as f32,
            (height / clipped_height) as f32,
            ((2.0 * (x - native_left) + width - clipped_width) / clipped_width) as f32,
            ((clipped_height - 2.0 * (y - native_top) - height) / clipped_height) as f32,
        ]
    };
    Some(Viewport {
        native: D3D11_VIEWPORT {
            TopLeftX: native_left as f32,
            TopLeftY: native_top as f32,
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

    #[test]
    fn fractional_viewport_keeps_logical_coordinates_in_the_remap() {
        let viewport = translate(
            &crate::Rect {
                x: 1.75,
                y: 1.3,
                w: 2.25,
                h: 2.2,
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
                viewport.native.Height,
            ],
            [1.0, 1.0, 3.0, 3.0]
        );
        for (actual, expected) in viewport
            .transform
            .iter()
            .zip([0.75, 2.2 / 3.0, 0.25, 0.2 / 3.0])
        {
            assert!((*actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn fractional_viewport_intersects_the_target_with_integer_bounds() {
        let viewport = translate(
            &crate::Rect {
                x: -0.25,
                y: 0.25,
                w: 4.5,
                h: 3.5,
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
                viewport.native.Height,
            ],
            [0.0, 0.0, 5.0, 4.0]
        );
        assert_eq!(viewport.transform, [0.9, 0.875, -0.2, 0.0]);
    }

    #[test]
    fn zero_sized_fractional_viewport_stays_empty() {
        let viewport = translate(
            &crate::Rect {
                x: 1.25,
                y: 2.75,
                w: 0.0,
                h: 3.5,
            },
            0.0..1.0,
            [8, 8],
            2048,
        )
        .unwrap();
        assert_eq!(viewport.native.Width, 0.0);
        assert_eq!(viewport.transform, [1.0, 1.0, 0.0, 0.0]);
    }
}
