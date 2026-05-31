//! 2D screen-space overlay: alpha-blended coloured rectangles drawn over the 3D
//! scene (HUD, health bars, nameplate backings, target markers). Pixel
//! coordinates with the origin at the top-left; converted to NDC at draw time
//! from the current framebuffer size. No depth — drawn after the world pass with
//! `LoadOp::Load`. (A bitmap-font text layer builds on this next.)

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Project a world point to screen pixels (top-left origin) using a row-major
/// view-projection matrix. Returns `None` if behind the camera. `y_off` raises
/// the result in world units before projection (e.g. to a head/nameplate).
pub fn project(vp: &[f32; 16], world: [f32; 3], screen_w: f32, screen_h: f32) -> Option<(f32, f32)> {
    let (x, y, z) = (world[0], world[1], world[2]);
    let cx = vp[0] * x + vp[1] * y + vp[2] * z + vp[3];
    let cy = vp[4] * x + vp[5] * y + vp[6] * z + vp[7];
    let cw = vp[12] * x + vp[13] * y + vp[14] * z + vp[15];
    if cw <= 0.0001 {
        return None;
    }
    let ndc_x = cx / cw;
    let ndc_y = cy / cw;
    Some((
        (ndc_x * 0.5 + 0.5) * screen_w,
        (1.0 - (ndc_y * 0.5 + 0.5)) * screen_h,
    ))
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct V2 {
    pos: [f32; 2],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct TV {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

const SHADER: &str = r#"
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.clip = vec4<f32>(pos, 0.0, 1.0);
    o.color = color;
    return o;
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> { return in.color; }
"#;

const TEX_SHADER: &str = r#"
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) uv: vec2<f32>, @location(1) color: vec4<f32> };
@vertex fn vs(@location(0) pos: vec2<f32>, @location(1) uv: vec2<f32>, @location(2) color: vec4<f32>) -> VsOut {
    var o: VsOut;
    o.clip = vec4<f32>(pos, 0.0, 1.0);
    o.uv = uv;
    o.color = color;
    return o;
}
@group(0) @binding(0) var t: texture_2d<f32>;
@group(0) @binding(1) var s: sampler;
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(t, s, in.uv) * in.color;
}
"#;

/// One queued textured quad: pixel rect, uv rect (0..1), tint, texture key.
struct TexQuad {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    uv: [f32; 4], // u0, v0, u1, v1
    color: [f32; 4],
    key: String,
}

/// Accumulates rectangles, then draws them over a target view in one pass.
pub struct Overlay {
    pipeline: wgpu::RenderPipeline,
    rects: Vec<(f32, f32, f32, f32, [f32; 4])>,
    // Textured-quad layer (GUI icons, the XP bar, item icons).
    tex_pipeline: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: std::collections::HashMap<String, wgpu::BindGroup>,
    quads: Vec<TexQuad>,
}

impl Overlay {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Overlay {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<V2>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        // Textured-quad pipeline (GUI .bmp icons, XP bar, item icons).
        let tex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay-tex"),
            source: wgpu::ShaderSource::Wgsl(TEX_SHADER.into()),
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("overlay-tex-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("overlay-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let tex_pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&tex_layout],
            push_constant_ranges: &[],
        });
        let tex_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay-tex"),
            layout: Some(&tex_pl),
            vertex: wgpu::VertexState {
                module: &tex_shader,
                entry_point: "vs",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TV>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                }],
            },
            fragment: Some(wgpu::FragmentState {
                module: &tex_shader,
                entry_point: "fs",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Overlay {
            pipeline,
            rects: Vec::new(),
            tex_pipeline,
            tex_layout,
            sampler,
            textures: std::collections::HashMap::new(),
            quads: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.rects.clear();
        self.quads.clear();
    }

    /// Upload a decoded GUI image once under `key`, building its bind group.
    /// Re-registering the same key replaces it. Skips empty images.
    pub fn register_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key: &str,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("overlay-gui-tex"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &tex,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(width * 4), rows_per_image: Some(height) },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&Default::default());
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("overlay-gui-bg"),
            layout: &self.tex_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.textures.insert(key.to_string(), bg);
    }

    /// Whether a texture is registered under `key`.
    pub fn has_texture(&self, key: &str) -> bool {
        self.textures.contains_key(key)
    }

    /// Queue a textured quad (full texture). Tint multiplies the sample
    /// (`[1,1,1,1]` = unmodified). No-op if `key` isn't registered (the caller
    /// can fall back to a text label / coloured rect).
    pub fn image(&mut self, x: f32, y: f32, w: f32, h: f32, key: &str, tint: [f32; 4]) {
        if self.textures.contains_key(key) {
            self.quads.push(TexQuad { x, y, w, h, uv: [0.0, 0.0, 1.0, 1.0], color: tint, key: key.to_string() });
        }
    }

    /// Queue a textured quad sampling a sub-rect (uv in 0..1) of the texture.
    pub fn image_uv(&mut self, x: f32, y: f32, w: f32, h: f32, key: &str, uv: [f32; 4], tint: [f32; 4]) {
        if self.textures.contains_key(key) {
            self.quads.push(TexQuad { x, y, w, h, uv, color: tint, key: key.to_string() });
        }
    }

    /// Queue a filled rectangle (top-left origin, pixels, RGBA 0..1).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.rects.push((x, y, w, h, color));
    }

    /// Draw `s` as 8×8 bitmap glyphs (one quad per lit pixel) at `scale` px per
    /// font pixel, top-left at `(x, y)`. Newlines advance a line.
    pub fn text(&mut self, x: f32, y: f32, scale: f32, s: &str, color: [f32; 4]) {
        let (mut cx, mut cy) = (x, y);
        for ch in s.chars() {
            if ch == '\n' {
                cx = x;
                cy += 9.0 * scale;
                continue;
            }
            let g = crate::font::glyph(ch as u8);
            for (row, bits) in g.iter().enumerate() {
                for col in 0..8u8 {
                    if bits & (1 << col) != 0 {
                        self.rect(cx + col as f32 * scale, cy + row as f32 * scale, scale, scale, color);
                    }
                }
            }
            cx += 9.0 * scale;
        }
    }

    /// `text` with a 1px dark drop-shadow for legibility over the 3D scene.
    pub fn text_shadow(&mut self, x: f32, y: f32, scale: f32, s: &str, color: [f32; 4]) {
        self.text(x + scale, y + scale, scale, s, [0.0, 0.0, 0.0, 0.7]);
        self.text(x, y, scale, s, color);
    }

    /// A `frac`-filled bar: dark background + a coloured foreground (e.g. HP).
    pub fn bar(&mut self, x: f32, y: f32, w: f32, h: f32, frac: f32, color: [f32; 4]) {
        self.rect(x - 1.0, y - 1.0, w + 2.0, h + 2.0, [0.0, 0.0, 0.0, 0.6]);
        let f = frac.clamp(0.0, 1.0);
        if f > 0.0 {
            self.rect(x, y, w * f, h, color);
        }
    }

    /// Draw all queued rects over `view` (loads existing contents). Clears the
    /// queue. No-op if nothing queued.
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        screen_w: f32,
        screen_h: f32,
    ) {
        if self.rects.is_empty() && self.quads.is_empty() {
            return;
        }
        let (sw, sh) = (screen_w.max(1.0), screen_h.max(1.0));
        let to_ndc = |px: f32, py: f32| [px / sw * 2.0 - 1.0, 1.0 - py / sh * 2.0];

        // Coloured rects.
        let mut verts: Vec<V2> = Vec::with_capacity(self.rects.len() * 6);
        for &(x, y, w, h, c) in &self.rects {
            let tl = to_ndc(x, y);
            let tr = to_ndc(x + w, y);
            let br = to_ndc(x + w, y + h);
            let bl = to_ndc(x, y + h);
            for p in [tl, tr, br, tl, br, bl] {
                verts.push(V2 { pos: p, color: c });
            }
        }
        let n = verts.len() as u32;
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("overlay-v"),
            // Never hand wgpu a zero-length buffer.
            contents: if verts.is_empty() { &[0u8; 4] } else { bytemuck::cast_slice(&verts) },
            usage: wgpu::BufferUsages::VERTEX,
        });

        // Textured quads, grouped into contiguous per-texture draw ranges so we
        // bind each GUI texture once. Order preserves first-seen-key grouping.
        let mut tverts: Vec<TV> = Vec::with_capacity(self.quads.len() * 6);
        let mut ranges: Vec<(String, u32, u32)> = Vec::new(); // key, start, count
        {
            use std::collections::HashMap;
            let mut buckets: HashMap<&str, Vec<&TexQuad>> = HashMap::new();
            let mut order: Vec<&str> = Vec::new();
            for q in &self.quads {
                if !buckets.contains_key(q.key.as_str()) {
                    order.push(q.key.as_str());
                }
                buckets.entry(q.key.as_str()).or_default().push(q);
            }
            for key in order {
                let start = tverts.len() as u32;
                for q in &buckets[key] {
                    let [u0, v0, u1, v1] = q.uv;
                    let tl = (to_ndc(q.x, q.y), [u0, v0]);
                    let tr = (to_ndc(q.x + q.w, q.y), [u1, v0]);
                    let br = (to_ndc(q.x + q.w, q.y + q.h), [u1, v1]);
                    let bl = (to_ndc(q.x, q.y + q.h), [u0, v1]);
                    for (p, uv) in [tl, tr, br, tl, br, bl] {
                        tverts.push(TV { pos: p, uv, color: q.color });
                    }
                }
                let count = tverts.len() as u32 - start;
                ranges.push((key.to_string(), start, count));
            }
        }
        let tvbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("overlay-tv"),
            contents: if tverts.is_empty() { &[0u8; 4] } else { bytemuck::cast_slice(&tverts) },
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut enc = device.create_command_encoder(&Default::default());
        {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("overlay"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if n > 0 {
                rp.set_pipeline(&self.pipeline);
                rp.set_vertex_buffer(0, vbuf.slice(..));
                rp.draw(0..n, 0..1);
            }
            // Textured quads on top (icons over their backing rects).
            if !ranges.is_empty() {
                rp.set_pipeline(&self.tex_pipeline);
                rp.set_vertex_buffer(0, tvbuf.slice(..));
                for (key, start, count) in &ranges {
                    if let Some(bg) = self.textures.get(key) {
                        rp.set_bind_group(0, bg, &[]);
                        rp.draw(*start..(*start + *count), 0..1);
                    }
                }
            }
        }
        queue.submit(Some(enc.finish()));
        self.clear();
    }
}
