//! Real-time scene renderer for a window surface. Owns the pipeline, a depth
//! buffer, and a per-frame camera uniform; the scene drawables are uploaded
//! once (or whenever the scene changes) and re-drawn each frame with an updated
//! view-projection. Shares the textured pipeline with the offscreen PNG path
//! via [`crate::gpu`], so both look identical.

use std::collections::HashMap;
use std::rc::Rc;

use wgpu::util::DeviceExt;

use rcce_data::{B3dModel, Image};

use crate::gpu::{self, ActorSkin, Drawable, IndexCache, Pipeline, SkinPipeline, SkyPipeline, TexCache, Uniforms};
use crate::scene::SceneInstance;

/// A skinned actor instance for the GPU linear-blend-skinning path. The static
/// mesh is uploaded once (keyed by `key`); the pose comes from `frame`.
pub struct SkinnedInstance<'a> {
    /// Appearance key (e.g. `tmpl:gender`) — keys the cached static geometry.
    pub key: &'a str,
    /// The actor template model (with bones + skin weights).
    pub model: &'a B3dModel,
    /// Per-mesh textures (aligns to `model.meshes`).
    pub textures: &'a [Option<Image>],
    /// Animation frame (None = bind pose).
    pub frame: Option<f32>,
    /// Column-major instance model matrix (world transform).
    pub transform: [f32; 16],
    /// Tint colour (multiplied with the texture).
    pub color: [f32; 3],
}

/// One skinned mesh draw: shared static geometry + the per-actor uniform slot.
struct SkinnedDrawable {
    vbuf: Rc<wgpu::Buffer>,
    ibuf: Rc<wgpu::Buffer>,
    n_idx: u32,
    tex: Rc<wgpu::BindGroup>,
    actor: usize, // index into actor_pool
}

pub struct WorldView {
    pipeline: Pipeline,
    skin: SkinPipeline,
    sky: SkyPipeline,
    /// Surface colour format (for offscreen screenshot captures).
    color_format: wgpu::TextureFormat,
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
    /// Cached constant index buffers (keyed like the textures) so per-frame
    /// rebuilds don't recreate the topology buffer each tick.
    idx_cache: IndexCache,
    /// Static skinned geometry per `key:mesh` (vbuf + ibuf + n_idx + texture),
    /// uploaded ONCE — the GPU skinning path poses it via the per-actor uniform.
    skin_static: HashMap<String, (Rc<wgpu::Buffer>, Rc<wgpu::Buffer>, u32, Rc<wgpu::BindGroup>)>,
    /// Reusable per-actor skinning uniform buffers + binds (grown as needed).
    actor_pool: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    /// This frame's skinned draws.
    skinned: Vec<SkinnedDrawable>,
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
        let skin = SkinPipeline::new(device, color_format, &pipeline);
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
            skin,
            sky,
            color_format,
            uniform_buf,
            bind0,
            depth: make_depth(device, w, h),
            statics: Vec::new(),
            dynamics: Vec::new(),
            tex_cache: TexCache::new(),
            idx_cache: IndexCache::new(),
            skin_static: HashMap::new(),
            actor_pool: Vec::new(),
            skinned: Vec::new(),
        }
    }

    /// Replace the per-frame SKINNED actors (GPU linear-blend skinning). The
    /// static mesh for each appearance is built once and cached; per frame only
    /// each actor's small bone-palette uniform is written — no CPU re-skin or
    /// vertex re-upload. Pair with [`set_dynamic`](Self::set_dynamic) for the
    /// unskinned attachments (hair/gear).
    pub fn set_skinned(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, instances: &[SkinnedInstance]) {
        self.skinned.clear();
        // Ensure a reusable uniform buffer/bind per actor instance.
        while self.actor_pool.len() < instances.len() {
            self.actor_pool.push(self.skin.make_actor(device));
        }
        for (ai, inst) in instances.iter().enumerate() {
            // Build + cache this appearance's static skinned meshes once.
            let first_key = format!("{}:0", inst.key);
            if !self.skin_static.contains_key(&first_key) {
                let (ids, wts) = inst.model.skin_attributes();
                for (mi, mesh) in inst.model.meshes.iter().enumerate() {
                    if mesh.positions.is_empty() || mesh.indices.is_empty() {
                        continue;
                    }
                    let vbuf = Rc::new(gpu::build_skinned_vbuf(device, mesh, &ids, &wts));
                    let ibuf = Rc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("skin-i"),
                        contents: bytemuck::cast_slice(&mesh.indices),
                        usage: wgpu::BufferUsages::INDEX,
                    }));
                    let tex = Rc::new(self.pipeline.texture_bind(device, queue, inst.textures.get(mi).and_then(|t| t.as_ref())));
                    self.skin_static.insert(format!("{}:{}", inst.key, mi), (vbuf, ibuf, mesh.indices.len() as u32, tex));
                }
            }
            // Update this actor's pose uniform (palette + model + colour).
            let skin = ActorSkin::new(&inst.model.bone_palette(inst.frame), inst.transform, inst.color);
            self.skin.update_actor(queue, &self.actor_pool[ai].0, &skin);
            // Queue a draw per mesh, sharing the actor's uniform slot.
            for mi in 0..inst.model.meshes.len() {
                if let Some((vbuf, ibuf, n_idx, tex)) = self.skin_static.get(&format!("{}:{}", inst.key, mi)) {
                    self.skinned.push(SkinnedDrawable {
                        vbuf: vbuf.clone(),
                        ibuf: ibuf.clone(),
                        n_idx: *n_idx,
                        tex: tex.clone(),
                        actor: ai,
                    });
                }
            }
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
            &mut self.idx_cache,
        );
    }

    pub fn drawable_count(&self) -> usize {
        self.statics.len() + self.dynamics.len() + self.skinned.len()
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
    pub fn set_cloud_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        self.sky.set_cloud_texture(device, queue, width, height, rgba);
    }
    pub fn clear_cloud_texture(&mut self) {
        self.sky.clear_clouds();
    }
    pub fn set_stars_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        self.sky.set_stars_texture(device, queue, width, height, rgba);
    }
    pub fn clear_stars_texture(&mut self) {
        self.sky.clear_stars();
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
        sky_time: f32,
        sky_night: f32,
    ) {
        let u = Uniforms::new(view_proj, eye, fog_color, fog_near, fog_far, ambient, light_dir);
        queue.write_buffer(&self.uniform_buf, 0, bytemuck::bytes_of(&u));
        // Sky gradient: horizon = fog colour (so the world fades into it),
        // zenith bluer/darker. Then the per-frame yaw pans the sky texture and
        // drives the cloud drift.
        self.sky.set_colors(queue, gpu::sky_zenith(fog_color), fog_color);
        self.sky.set_frame(queue, sky_yaw, sky_time, sky_night);
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
            // 1) Opaque pass: terrain base + props (everything except the splat
            //    overlays). Depth write on.
            rp.set_pipeline(&self.pipeline.pipeline);
            rp.set_bind_group(0, &self.bind0, &[]);
            for d in self.statics.iter().chain(self.dynamics.iter()) {
                if d.alpha {
                    continue;
                }
                rp.set_bind_group(1, &d.tex_bind, &[]);
                rp.set_vertex_buffer(0, d.vbuf.slice(..));
                rp.set_index_buffer(d.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..d.n_idx, 0, 0..1);
            }
            // 2) GPU-skinned actors: same camera uniform (group 0), per-actor pose
            //    uniform (group 2). The vertex shader skins the static mesh.
            if !self.skinned.is_empty() {
                rp.set_pipeline(&self.skin.pipeline);
                rp.set_bind_group(0, &self.bind0, &[]);
                for sd in &self.skinned {
                    rp.set_bind_group(1, &sd.tex, &[]);
                    rp.set_bind_group(2, &self.actor_pool[sd.actor].1, &[]);
                    rp.set_vertex_buffer(0, sd.vbuf.slice(..));
                    rp.set_index_buffer(sd.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                    rp.draw_indexed(0..sd.n_idx, 0, 0..1);
                }
            }
            // 3) Alpha pass: terrain splat overlays blended over the opaque base
            //    by vertex alpha (paths fade into grass). LessEqual + no depth
            //    write so they layer on the base but don't occlude the actors.
            rp.set_pipeline(&self.pipeline.alpha_pipeline);
            rp.set_bind_group(0, &self.bind0, &[]);
            for d in self.statics.iter().chain(self.dynamics.iter()) {
                if !d.alpha {
                    continue;
                }
                rp.set_bind_group(1, &d.tex_bind, &[]);
                rp.set_vertex_buffer(0, d.vbuf.slice(..));
                rp.set_index_buffer(d.ibuf.slice(..), wgpu::IndexFormat::Uint32);
                rp.draw_indexed(0..d.n_idx, 0, 0..1);
            }
        }
        queue.submit(Some(enc.finish()));
    }

    /// Render one frame of the live world to an offscreen texture and save it as
    /// a PNG — a headless screenshot of exactly what the window shows (same
    /// pipelines, same `set_scene`/`set_dynamic`/`set_skinned` state, same
    /// camera + atmosphere args as [`render`](Self::render)). For verifying the
    /// live render (actor placement, foliage cutout, camera framing) without a
    /// visible window. `w`/`h` should match the depth buffer (the window size).
    #[allow(clippy::too_many_arguments)]
    pub fn capture_png(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        w: u32,
        h: u32,
        view_proj: [f32; 16],
        eye: [f32; 3],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        ambient: [f32; 3],
        light_dir: [f32; 3],
        clear: wgpu::Color,
        sky_yaw: f32,
        sky_time: f32,
        sky_night: f32,
        path: &str,
    ) -> Result<(), String> {
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("screenshot"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.color_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        // Reuse the exact live render path, but target the offscreen view.
        self.render(
            device, queue, &view, view_proj, eye, fog_color, fog_near, fog_far, ambient,
            light_dir, clear, sky_yaw, sky_time, sky_night,
        );
        save_texture_png(device, queue, &tex, w, h, self.color_format, path)
    }
}

/// Copy a rendered texture back to the CPU and write it as a PNG (swizzling
/// BGRA surfaces to RGBA). The texture must have been created with `COPY_SRC`.
/// Shared by [`WorldView::capture_png`] and the client's menu screenshot.
pub fn save_texture_png(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    tex: &wgpu::Texture,
    w: u32,
    h: u32,
    format: wgpu::TextureFormat,
    path: &str,
) -> Result<(), String> {
    let bpp = 4u32;
    let unpadded = w * bpp;
    let padded = unpadded.div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shot-rb"),
        size: (padded * h) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut enc = device.create_command_encoder(&Default::default());
    enc.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &readback,
            layout: wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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
    let bgra = matches!(
        format,
        wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
    );
    let mut rgba = Vec::with_capacity((unpadded * h) as usize);
    for row in 0..h {
        let start = (row * padded) as usize;
        let line = &data[start..start + unpadded as usize];
        if bgra {
            for px in line.chunks_exact(4) {
                rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
            }
        } else {
            rgba.extend_from_slice(line);
        }
    }
    drop(data);
    readback.unmap();

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut wr = encoder.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(&rgba).map_err(|e| e.to_string())?;
    Ok(())
}

/// View-projection matrix for a camera looking from `eye` at `target`.
///
/// **Left-handed** to match the Blitz3D world the server streams (Blitz uses a
/// LH coordinate system: +Z is forward/into-screen). Using RH matrices on LH
/// world data reflects the image — a horizontal mirror — so the world rendered
/// backwards (path/NPCs on the wrong side). `perspective_lh` keeps wgpu's [0,1]
/// depth; `cull_mode: None` on every pipeline means the winding flip is moot.
pub fn view_proj(eye: [f32; 3], target: [f32; 3], aspect: f32) -> [f32; 16] {
    use glam::{Mat4, Vec3};
    let proj = Mat4::perspective_lh(50f32.to_radians(), aspect, 1.0, 100_000.0);
    let view = Mat4::look_at_lh(Vec3::from(eye), Vec3::from(target), Vec3::Y);
    (proj * view).to_cols_array()
}
