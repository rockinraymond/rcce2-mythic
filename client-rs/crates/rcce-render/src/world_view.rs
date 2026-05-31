//! Real-time scene renderer for a window surface. Owns the pipeline, a depth
//! buffer, and a per-frame camera uniform; the scene drawables are uploaded
//! once (or whenever the scene changes) and re-drawn each frame with an updated
//! view-projection. Shares the textured pipeline with the offscreen PNG path
//! via [`crate::gpu`], so both look identical.

use wgpu::util::DeviceExt;

use crate::gpu::{self, Drawable, Pipeline, SkyPipeline, TexCache, Uniforms};
use crate::scene::SceneInstance;

pub struct WorldView {
    pipeline: Pipeline,
    sky: SkyPipeline,
    uniform_buf: wgpu::Buffer,
    bind0: wgpu::BindGroup,
    depth: wgpu::TextureView,
    /// Static geometry (terrain/scenery), uploaded once.
    statics: Vec<Drawable>,
    /// Per-frame geometry (actors), rebuilt as they move/animate.
    dynamics: Vec<Drawable>,
    /// Cached actor texture binds (keyed by appearance) so per-frame rebuilds
    /// reuse the upload instead of re-sending skins to the GPU every frame.
    tex_cache: TexCache,
}

fn make_depth(device: &wgpu::Device, w: u32, h: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: gpu::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

impl WorldView {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat, w: u32, h: u32) -> WorldView {
        let pipeline = Pipeline::new(device, color_format);
        let sky = SkyPipeline::new(device, color_format);
        let uniform_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("u"),
            contents: bytemuck::bytes_of(&Uniforms::new(
                [0.0; 16], [0.0; 3], [0.0; 3], 1.0, 2.0, [0.5; 3], [0.0, 1.0, 0.0],
            )),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.bgl_uniform,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buf.as_entire_binding(),
            }],
        });
        WorldView {
            pipeline,
            sky,
            uniform_buf,
            bind0,
            depth: make_depth(device, w, h),
            statics: Vec::new(),
            dynamics: Vec::new(),
            tex_cache: TexCache::new(),
        }
    }

    /// Replace the static scene geometry (terrain/scenery + ground plane).
    pub fn set_scene(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[SceneInstance],
        ground_y: f32,
    ) {
        self.statics = gpu::build_drawables(device, queue, &self.pipeline, instances, ground_y);
    }

    /// Replace the dynamic (per-frame) geometry — actors. `keys[i]` identifies
    /// instance `i`'s appearance so its texture upload is cached across frames.
    /// No ground plane.
    pub fn set_dynamic(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        instances: &[SceneInstance],
        keys: &[String],
    ) {
        self.dynamics = gpu::build_actor_drawables_cached(
            device,
            queue,
            &self.pipeline,
            instances,
            keys,
            &mut self.tex_cache,
        );
    }

    pub fn drawable_count(&self) -> usize {
        self.statics.len() + self.dynamics.len()
    }

    pub fn resize(&mut self, device: &wgpu::Device, w: u32, h: u32) {
        self.depth = make_depth(device, w, h);
    }

    /// Draw the scene to `view` with the camera + atmosphere. `eye` is the
    /// camera position (for fog distance); `fog_*` define the distance fog.
    #[allow(clippy::too_many_arguments)]
    /// Upload the area's sky texture (RGBA8) for the textured skydome; pass it on
    /// zone load. Clearing reverts to the plain gradient.
    pub fn set_sky_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        self.sky.set_texture(device, queue, width, height, rgba);
    }
    pub fn clear_sky_texture(&mut self) {
        self.sky.clear_texture();
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        view_proj: [f32; 16],
        eye: [f32; 3],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        ambient: [f32; 3],
        light_dir: [f32; 3],
        clear: wgpu::Color,
        sky_yaw: f32,
    ) {
        let u = Uniforms::new(view_proj, eye, fog_color, fog_near, fog_far, ambient, light_dir);
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
        // Sky gradient: horizon = fog colour (so the world fades into it),
        // zenith bluer/darker. Then the per-frame yaw pans any sky texture.
        self.sky.set_colors(queue, gpu::sky_zenith(fog_color), fog_color);
        self.sky.set_frame(queue, sky_yaw);
        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("world"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            self.sky.draw(&mut rp); // behind the world (far plane, no depth write)
            rp.set_pipeline(&self.pipeline.pipeline);
            rp.set_bind_group(0, &self.bind0, &[]);
            for d in self.statics.iter().chain(self.dynamics.iter()) {
                rp.set_bind_group(1, &d.tex_bind, &[]);
                rp.set_vertex_buffer(0, d.vbuf.slice(..));
                rp.set_index_buffer(d.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..d.n_idx, 0, 0..1);
            }
        }
        queue.submit(Some(enc.finish()));
    }
}

/// View-projection matrix for a camera looking from `eye` at `target`.
pub fn view_proj(eye: [f32; 3], target: [f32; 3], aspect: f32) -> [f32; 16] {
    use glam::{Mat4, Vec3};
    let proj = Mat4::perspective_rh(50f32.to_radians(), aspect, 1.0, 100_000.0);
    let view = Mat4::look_at_rh(Vec3::from(eye), Vec3::from(target), Vec3::Y);
    (proj * view).to_cols_array()
}
