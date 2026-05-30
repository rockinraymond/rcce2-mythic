//! Headless verification of the 2D overlay: clear a target, draw some bars and
//! rectangles via rcce_render::Overlay, read back to a PNG. Confirms the
//! screen-space quad pipeline (pixel coords, alpha blend) without a window.
//!
//!   cargo run -p rcce-client --bin overlay_test --release -- [out.png]

use std::io::BufWriter;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "overlay_test.png".to_string());
    let (w, h) = (640u32, 360u32);

    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("overlay-test"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .expect("device");

    let format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("t"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());

    // Clear pass (sky), then overlay draws over it.
    let mut enc = device.create_command_encoder(&Default::default());
    {
        enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.45, g: 0.62, b: 0.82, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    queue.submit(Some(enc.finish()));

    let mut overlay = rcce_render::Overlay::new(&device, format);
    // A HUD panel, three health bars at different fills, and a target marker.
    overlay.rect(8.0, 8.0, 240.0, 56.0, [0.0, 0.0, 0.0, 0.5]);
    overlay.bar(120.0, 80.0, 80.0, 8.0, 1.0, [0.2, 0.85, 0.2, 1.0]);
    overlay.bar(300.0, 140.0, 80.0, 8.0, 0.55, [0.9, 0.8, 0.1, 1.0]);
    overlay.bar(420.0, 220.0, 80.0, 8.0, 0.2, [0.9, 0.2, 0.2, 1.0]);
    overlay.rect(317.0, 175.0, 6.0, 6.0, [1.0, 1.0, 1.0, 0.9]);
    // Font verification: render the printable charset.
    overlay.text_shadow(16.0, 16.0, 2.0, "RCCE2 Rust Client", [1.0, 1.0, 1.0, 1.0]);
    overlay.text(16.0, 250.0, 2.0, "ABCDEFGHIJKLMNOPQRSTUVWXYZ", [1.0, 1.0, 0.6, 1.0]);
    overlay.text(16.0, 274.0, 2.0, "abcdefghijklmnopqrstuvwxyz", [0.7, 1.0, 0.7, 1.0]);
    overlay.text(16.0, 298.0, 2.0, "0123456789 .,:;!?'-/()[]+=", [0.7, 0.8, 1.0, 1.0]);
    overlay.render(&device, &queue, &view, w as f32, h as f32);

    // Readback → PNG.
    let bpp = 4u32;
    let unpadded = w * bpp;
    let padded = unpadded.div_ceil(256) * 256;
    let rb = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &target,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &rb,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit(Some(enc.finish()));

    let slice = rb.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();
    let data = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let s = (row * padded) as usize;
        rgba.extend_from_slice(&data[s..s + unpadded as usize]);
    }
    drop(data);
    rb.unmap();

    let file = std::fs::File::create(&out).unwrap();
    let mut enc = png::Encoder::new(BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    println!("[overlay_test] wrote {out}");
}
