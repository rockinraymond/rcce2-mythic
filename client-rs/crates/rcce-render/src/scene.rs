//! Offscreen 3D scene renderer: many model instances (each at a world
//! position/rotation/scale, with per-mesh textures and a fallback tint) plus a
//! ground plane, from an explicit camera, to a PNG. Shares the textured,
//! fog-aware pipeline with the real-time window via [`crate::gpu`] — so the
//! headless PNG and the live window render identically.

use std::io::BufWriter;

use glam::{Mat4, Vec3};
use pollster::block_on;

use rcce_data::{B3dModel, Image};

use crate::gpu::{self, Pipeline, Uniforms};

/// One model placed in the world. `textures` aligns to `model.meshes`.
pub struct SceneInstance<'a> {
    pub model: &'a B3dModel,
    pub textures: &'a [Option<Image>],
    pub translation: [f32; 3],
    /// Pitch, yaw, roll in radians (X, Y, Z). Actors use `[0, yaw, 0]`;
    /// scenery carries all three from the area file.
    pub rot: [f32; 3],
    /// Per-axis world scale. Actors pass `[s, s, s]`; scenery is non-uniform.
    pub scale: [f32; 3],
    /// Fallback/tint colour (multiplied with the texture; shows through where a
    /// mesh has no texture).
    pub color: [f32; 3],
}

/// Render the scene to a PNG with distance fog. `eye`/`target` define the
/// camera; `fog_color` is also the sky/clear colour.
#[allow(clippy::too_many_arguments)]
pub fn render_scene_png(
    instances: &[SceneInstance],
    eye: [f32; 3],
    target: [f32; 3],
    ground_y: f32,
    fog_color: [f32; 3],
    fog_near: f32,
    fog_far: f32,
    ambient: [f32; 3],
    light_dir: [f32; 3],
    width: u32,
    height: u32,
    path: &str,
    // Optional sky texture (w, h, RGBA8) for the textured skydome; None keeps
    // the plain gradient.
    sky_tex: Option<(u32, u32, Vec<u8>)>,
) -> Result<String, String> {
    let aspect = width as f32 / height as f32;
    let proj = Mat4::perspective_rh(50f32.to_radians(), aspect, 1.0, 100_000.0);
    let view = Mat4::look_at_rh(Vec3::from(eye), Vec3::from(target), Vec3::Y);
    let uniforms = Uniforms::new(
        (proj * view).to_cols_array(),
        eye,
        fog_color,
        fog_near,
        fog_far,
        ambient,
        light_dir,
    );

    let instance = wgpu::Instance::default();
    let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .ok_or("no GPU adapter")?;
    let adapter_name = adapter.get_info().name;
    let (device, queue) = block_on(adapter.request_device(
        &wgpu::DeviceDescriptor {
            label: Some("scene"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .map_err(|e| format!("request_device: {e}"))?;

    let color_format = gpu::COLOR_FORMAT;
    let target_tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: color_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = target_tex.create_view(&Default::default());
    let depth = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("depth"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: gpu::DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&Default::default());

    // Shared pipeline + per-instance drawables (with ground plane).
    let pipeline = Pipeline::new(&device, color_format);
    let mut sky = gpu::SkyPipeline::new(&device, color_format);
    sky.set_colors(&queue, gpu::sky_zenith(fog_color), fog_color);
    if let Some((w, h, rgba)) = &sky_tex {
        sky.set_texture(&device, &queue, *w, *h, rgba);
    }
    sky.set_frame(&queue, 0.0); // still image → no yaw pan
    let ubuf = device.create_buffer_init_uniform(&uniforms);
    let bind0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.bgl_uniform,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
    });
    let drawables = gpu::build_drawables(&device, &queue, &pipeline, instances, ground_y);
    if drawables.is_empty() {
        return Err("empty scene".into());
    }

    let bpp = 4u32;
    let unpadded = width * bpp;
    let padded = unpadded.div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("rb"),
        size: (padded * height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    {
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &color_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: fog_color[0] as f64,
                        g: fog_color[1] as f64,
                        b: fog_color[2] as f64,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &depth_view,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        sky.draw(&mut rp); // gradient sky behind the world
        rp.set_pipeline(&pipeline.pipeline);
        rp.set_bind_group(0, &bind0, &[]);
        for d in &drawables {
            rp.set_bind_group(1, &d.tex_bind, &[]);
            rp.set_vertex_buffer(0, d.vbuf.slice(..));
            rp.set_index_buffer(d.ibuf.slice(..), wgpu::IndexFormat::Uint32);
            rp.draw_indexed(0..d.n_idx, 0, 0..1);
        }
    }
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &target_tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(enc.finish()));

    let slice = readback.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| format!("map: {e:?}"))?;
    let data = slice.get_mapped_range();
    let mut rgba = Vec::with_capacity((unpadded * height) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        rgba.extend_from_slice(&data[start..start + unpadded as usize]);
    }
    drop(data);
    readback.unmap();

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut w = encoder.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(&rgba).map_err(|e| e.to_string())?;

    Ok(adapter_name)
}

/// Small helper: a uniform buffer initialised with `u`.
trait DeviceUniformExt {
    fn create_buffer_init_uniform(&self, u: &Uniforms) -> wgpu::Buffer;
}
impl DeviceUniformExt for wgpu::Device {
    fn create_buffer_init_uniform(&self, u: &Uniforms) -> wgpu::Buffer {
        use wgpu::util::DeviceExt;
        self.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("u"),
            contents: bytemuck::bytes_of(u),
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }
}
