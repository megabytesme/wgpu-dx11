#![cfg(all(windows, feature = "dx11", feature = "wgsl"))]

use std::{
    future::Future,
    num::NonZeroIsize,
    sync::{mpsc, Arc},
    task::{Context, Poll, Wake, Waker},
    thread,
    time::Duration,
};
use wgpu::*;

struct ThreadWaker(thread::Thread);

impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}

fn poll_ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

#[test]
fn public_dx11_hwnd_surface_copy_src_reads_back_before_present_at_fl91() {
    use windows::{
        core::w,
        Win32::{
            Foundation::HWND,
            UI::WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED,
            },
        },
    };

    struct WindowGuard(HWND);

    impl Drop for WindowGuard {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }

    const WIDTH: u32 = 13;
    const HEIGHT: u32 = 7;
    const ROW_PITCH: u32 = 256;
    const BUFFER_OFFSET: u64 = 16;
    const SENTINEL: u8 = 0xA5;

    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("wgpu D3D11 FL9_1 surface copy test"),
            WS_OVERLAPPED,
            0,
            0,
            WIDTH as i32,
            HEIGHT as i32,
            None,
            None,
            None,
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            assert!(!required, "strict mode requires a test HWND: {error}");
            println!("SKIP: could not create a test HWND: {error}");
            return;
        }
    };
    let _window = WindowGuard(hwnd);

    let mut instance_descriptor = InstanceDescriptor::new_without_display_handle();
    instance_descriptor.backends = Backends::DX11;
    let instance = Instance::new(instance_descriptor);
    let surface = unsafe {
        instance.create_surface_unsafe(SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(rwh::WindowsDisplayHandle::new().into()),
            raw_window_handle: rwh::Win32WindowHandle::new(
                NonZeroIsize::new(hwnd.0 as isize).expect("non-null HWND"),
            )
            .into(),
        })
    }
    .expect("create public D3D11 HWND surface");
    let adapter = match poll_ready(instance.request_adapter(&RequestAdapterOptions {
        power_preference: PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: Some(&surface),
        ..Default::default()
    })) {
        Ok(adapter) => adapter,
        Err(error) => {
            assert!(
                !required,
                "strict mode requires a physical FL9_1 surface adapter: {error}"
            );
            println!("SKIP: no physical D3D11 adapter supports the HWND surface: {error}");
            return;
        }
    };
    let info = adapter.get_info();
    assert_eq!(info.backend, Backend::Dx11);
    assert!(info.driver_info.contains("FL9_1"), "{info:?}");
    assert!(info.driver_info.contains("physical"), "{info:?}");
    println!(
        "Hardware: name={:?} vendor={:#06x} device={:#06x} driver={:?} requested=FL9_1 actual=FL9_1 software=false; operation=HWND surface render/copy/readback/present",
        info.name, info.vendor, info.device, info.driver_info
    );

    let capabilities = surface.get_capabilities(&adapter);
    assert_eq!(capabilities.formats, [TextureFormat::Bgra8Unorm]);
    assert!(capabilities
        .usages
        .contains(TextureUsages::RENDER_ATTACHMENT));
    assert!(!capabilities.usages.contains(TextureUsages::COPY_DST));
    if !capabilities.usages.contains(TextureUsages::COPY_SRC) {
        assert!(
            !required,
            "strict mode requires BGRA8 surface COPY_SRC support"
        );
        println!("SKIP: selected D3D11 adapter does not expose surface COPY_SRC");
        return;
    }

    let (device, queue) = poll_ready(adapter.request_device(&DeviceDescriptor {
        label: Some("public D3D11 FL9_1 HWND surface copy"),
        required_features: Features::empty(),
        required_limits: adapter.limits(),
        ..Default::default()
    }))
    .expect("request a device within the FL9_1 projection");
    let mut config = surface
        .get_default_config(&adapter, WIDTH, HEIGHT)
        .expect("default D3D11 surface config");
    config.usage |= TextureUsages::COPY_SRC;
    surface.configure(&device, &config);

    let frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("public D3D11 HWND acquisition failed: {result:?}"),
    };
    let view = frame.texture.create_view(&TextureViewDescriptor {
        usage: Some(TextureUsages::RENDER_ATTACHMENT),
        ..Default::default()
    });

    let row_bytes = WIDTH * 4;
    let buffer_size = BUFFER_OFFSET + u64::from(ROW_PITCH) * u64::from(HEIGHT);
    let readback = device.create_buffer(&BufferDescriptor {
        label: Some("surface readback with row padding"),
        size: buffer_size,
        usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    queue.write_buffer(&readback, 0, &vec![SENTINEL; buffer_size as usize]);

    let clear = Color {
        r: 0.25,
        g: 0.5,
        b: 0.75,
        a: 1.0,
    };
    let mut encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("render and copy acquired HWND surface"),
    });
    {
        let _pass = encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("clear acquired HWND surface"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(clear),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    drop(view);
    encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &frame.texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: BUFFER_OFFSET,
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(HEIGHT),
            },
        },
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::channel();
    readback
        .slice(..)
        .map_async(MapMode::Read, move |result| sender.send(result).unwrap());
    device
        .poll(PollType::Wait {
            submission_index: Some(submission),
            timeout: Some(Duration::from_secs(10)),
        })
        .expect("wait for surface copy submission");
    receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("surface readback map callback")
        .expect("map surface readback buffer");

    let expected_bgra = [
        (clear.b * 255.0).round() as u8,
        (clear.g * 255.0).round() as u8,
        (clear.r * 255.0).round() as u8,
        (clear.a * 255.0).round() as u8,
    ];
    {
        let data = readback
            .slice(..)
            .get_mapped_range()
            .expect("mapped surface readback");
        assert!(data[..BUFFER_OFFSET as usize]
            .iter()
            .all(|value| *value == SENTINEL));
        for y in 0..HEIGHT as usize {
            let row_start = BUFFER_OFFSET as usize + y * ROW_PITCH as usize;
            for x in 0..WIDTH as usize {
                let pixel_start = row_start + x * 4;
                for (channel, expected) in expected_bgra.iter().enumerate() {
                    assert!(
                        data[pixel_start + channel].abs_diff(*expected) <= 1,
                        "pixel ({x}, {y}) channel {channel}: got {}, expected {expected}",
                        data[pixel_start + channel]
                    );
                }
            }
            assert!(
                data[row_start + row_bytes as usize..row_start + ROW_PITCH as usize]
                    .iter()
                    .all(|value| *value == SENTINEL)
            );
        }
    }
    readback.unmap();
    queue.present(frame);

    config.usage = TextureUsages::RENDER_ATTACHMENT;
    surface.configure(&device, &config);
    let render_only_frame = match surface.get_current_texture() {
        CurrentSurfaceTexture::Success(frame) | CurrentSurfaceTexture::Suboptimal(frame) => frame,
        result => panic!("public D3D11 render-only HWND acquisition failed: {result:?}"),
    };
    assert_eq!(
        render_only_frame.texture.usage(),
        TextureUsages::RENDER_ATTACHMENT,
        "reconfiguration must update the acquired texture descriptor"
    );
    let error_scope = device.push_error_scope(ErrorFilter::Validation);
    let mut invalid_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("reject copy from render-only acquired HWND surface"),
    });
    invalid_encoder.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &render_only_frame.texture,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &readback,
            layout: TexelCopyBufferLayout {
                offset: BUFFER_OFFSET,
                bytes_per_row: Some(ROW_PITCH),
                rows_per_image: Some(HEIGHT),
            },
        },
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    drop(invalid_encoder.finish());
    assert!(
        poll_ready(error_scope.pop()).is_some(),
        "render-only reconfiguration must remove COPY_SRC from acquired frames"
    );

    let render_only_view = render_only_frame
        .texture
        .create_view(&TextureViewDescriptor::default());
    let mut render_only_encoder = device.create_command_encoder(&CommandEncoderDescriptor {
        label: Some("render-only acquired HWND surface"),
    });
    {
        let _pass = render_only_encoder.begin_render_pass(&RenderPassDescriptor {
            label: Some("clear render-only acquired HWND surface"),
            color_attachments: &[Some(RenderPassColorAttachment {
                view: &render_only_view,
                depth_slice: None,
                resolve_target: None,
                ops: Operations {
                    load: LoadOp::Clear(Color::BLACK),
                    store: StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }
    drop(render_only_view);
    queue.submit([render_only_encoder.finish()]);
    queue.present(render_only_frame);
    println!(
        "PASS: copied and presented a COPY_SRC surface, then reconfigured the same surface to render-only; verified {} pixels, padding sentinels, copy rejection and second-frame presentation",
        WIDTH * HEIGHT
    );
}
