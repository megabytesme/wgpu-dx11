use super::*;
use crate::{CommandEncoder as _, Device as _, Instance as _, Queue as _, Surface as _};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, Win32WindowHandle, WindowsDisplayHandle,
};
use windows::{
    core::w,
    Win32::{
        Foundation::HWND,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_OVERLAPPED},
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

#[test]
fn fl91_hidden_hwnd_surface_renders_and_resizes() -> windows::core::Result<()> {
    let adapters = super::super::enumerate_native_adapters()?;
    let required = std::env::var_os("WGPU_DX11_REQUIRE_HARDWARE").is_some_and(|v| v == "1");
    let Some(adapter) = super::super::hardware_test::select_adapter(&adapters, required)
        .unwrap_or_else(|e| panic!("{e}"))
    else {
        std::println!("SKIP: no hardware adapter");
        return Ok(());
    };
    let native = Arc::new(adapter.open_device(&[super::super::FeatureLevel::L9_1])?);
    assert_eq!(native.feature_level(), super::super::FeatureLevel::L9_1);
    std::println!(
        "Hardware: {:?}; requested FL9_1; actual {:?}",
        adapter.identity(),
        native.feature_level()
    );
    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("STATIC"),
            w!("wgpu dx11 test"),
            WS_OVERLAPPED,
            0,
            0,
            64,
            64,
            None,
            None,
            None,
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) if !required => {
            std::println!("SKIP: could not create a test HWND: {error}");
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let _window = WindowGuard(hwnd);
    let handle = NonZeroIsize::new(hwnd.0 as isize).unwrap();
    let surface = unsafe {
        super::super::Instance
            .create_surface(
                RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
                RawWindowHandle::Win32(Win32WindowHandle::new(handle)),
            )
            .unwrap()
    };
    let device = super::super::Device::new(native.clone());
    let queue = super::super::Queue::new(native.clone());
    let fence = native.create_native_fence();
    let mut config = crate::SurfaceConfiguration {
        maximum_frame_latency: 1,
        present_mode: wgt::PresentMode::Fifo,
        composite_alpha_mode: wgt::CompositeAlphaMode::Opaque,
        format: wgt::TextureFormat::Bgra8Unorm,
        color_space: wgt::SurfaceColorSpace::Srgb,
        extent: wgt::Extent3d {
            width: 32,
            height: 24,
            depth_or_array_layers: 1,
        },
        usage: wgt::TextureUses::COLOR_TARGET,
        view_formats: alloc::vec::Vec::new(),
    };
    unsafe {
        surface.configure(&device, &config).unwrap();
    }
    let chain = surface
        .configured
        .lock()
        .as_ref()
        .unwrap()
        .swapchain
        .clone();
    let guard = native.immediate_context();
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        tx.send(chain.size()).unwrap();
    });
    assert!(rx.recv_timeout(Duration::from_millis(50)).is_err());
    drop(guard);
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap()?, [32, 24]);
    worker.join().unwrap();
    let acquired = unsafe { surface.acquire_texture(None, &fence).unwrap() };
    assert!(unsafe { surface.acquire_texture(None, &fence) }.is_err());
    let texture: &NativeTexture = acquired.texture.borrow();
    let view = unsafe {
        device
            .create_texture_view(
                texture,
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Bgra8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let mut encoder = unsafe {
        device
            .create_command_encoder(&crate::CommandEncoderDescriptor {
                label: None,
                queue: &queue,
            })
            .unwrap()
    };
    let color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::RED,
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
    let command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&pass).unwrap();
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        queue
            .submit(&[&command], &[&acquired.texture], (&fence, 1))
            .unwrap();
    }
    assert!(native.wait_native_fence(&fence, 1, Some(Duration::from_secs(5)))?);
    let pixel = acquired.texture.frame.as_ref().unwrap().readback()?;
    assert_eq!(pixel, [0, 0, 255, 255]);
    assert!(command
        .retain_surface_targets(&mut alloc::vec::Vec::new())
        .is_ok());
    unsafe {
        device.destroy_texture_view(view);
    }
    assert!(command
        .retain_surface_targets(&mut alloc::vec::Vec::new())
        .is_err());
    let present = unsafe { queue.present(&surface, acquired.texture) };
    assert!(
        present.is_ok() || matches!(present, Err(crate::SurfaceError::Occluded)),
        "{present:?}"
    );
    std::println!("Hidden HWND present: {present:?}; BGRA pixel {pixel:?}");
    unsafe {
        queue.wait_for_idle().unwrap();
    }
    config.extent.width = 24;
    config.extent.height = 16;
    unsafe {
        surface.configure(&device, &config).unwrap();
    }
    std::println!("Resize with completed command buffer retained: PASS");
    let resized = unsafe { surface.acquire_texture(None, &fence).unwrap() };
    let swapchain = surface
        .configured
        .lock()
        .as_ref()
        .unwrap()
        .swapchain
        .clone();
    let size = swapchain.size()?;
    assert_eq!(size, [24, 16]);
    drop(swapchain);
    let resized_view = unsafe {
        device
            .create_texture_view(
                resized.texture.borrow(),
                &crate::TextureViewDescriptor {
                    label: None,
                    format: wgt::TextureFormat::Bgra8Unorm,
                    dimension: wgt::TextureViewDimension::D2,
                    usage: wgt::TextureUses::COLOR_TARGET,
                    range: wgt::ImageSubresourceRange::default(),
                },
            )
            .unwrap()
    };
    let resized_color = [Some(crate::ColorAttachment {
        target: crate::Attachment {
            view: &resized_view,
            usage: wgt::TextureUses::COLOR_TARGET,
        },
        depth_slice: None,
        resolve_target: None,
        ops: crate::AttachmentOps::LOAD_CLEAR | crate::AttachmentOps::STORE,
        clear_value: wgt::Color::GREEN,
    })];
    let resized_pass = crate::RenderPassDescriptor {
        label: None,
        extent: config.extent,
        sample_count: 1,
        color_attachments: &resized_color,
        depth_stencil_attachment: None,
        multiview_mask: None,
        timestamp_writes: None,
        occlusion_query_set: None,
    };
    let resized_command = unsafe {
        encoder.begin_encoding(None).unwrap();
        encoder.begin_render_pass(&resized_pass).unwrap();
        encoder.end_render_pass();
        encoder.end_encoding().unwrap()
    };
    unsafe {
        queue
            .submit(&[&resized_command], &[&resized.texture], (&fence, 2))
            .unwrap();
    }
    assert!(native.wait_native_fence(&fence, 2, Some(Duration::from_secs(5)))?);
    assert_eq!(
        resized.texture.frame.as_ref().unwrap().readback()?,
        [0, 255, 0, 255]
    );
    unsafe {
        device.destroy_texture_view(resized_view);
        surface.discard_texture(resized.texture);
        surface.unconfigure(&device);
        surface.configure(&device, &config).unwrap();
        let recreated = surface.acquire_texture(None, &fence).unwrap();
        surface.discard_texture(recreated.texture);
        queue.wait_for_idle().unwrap();
        encoder.reset_all([command, resized_command].into_iter());
    }
    std::thread::scope(|scope| {
        let (tx, rx) = std::sync::mpsc::channel();
        let surface = &surface;
        scope.spawn(move || {
            let _operation = surface.begin_operation().unwrap();
            tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
        });
        rx.recv().unwrap();
        unsafe { surface.unconfigure(&device) };
    });
    unsafe {
        surface.configure(&device, &config).unwrap();
        let recreated = surface.acquire_texture(None, &fence).unwrap();
        surface.discard_texture(recreated.texture);
        surface.unconfigure(&device);
    }
    std::println!("Unconfigure after concurrent reservation and recreate: PASS");
    std::println!("PASS: hidden HWND acquire, HAL clear/readback, present and resize; visibility not verified");
    Ok(())
}
