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

/// Accumulates rectangles, then draws them over a target view in one pass.
pub struct Overlay {
    pipeline: wgpu::RenderPipeline,
    rects: Vec<(f32, f32, f32, f32, [f32; 4])>,
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
        Overlay {
            pipeline,
            rects: Vec::new(),
        }
    }

    pub fn clear(&mut self) {
        self.rects.clear();
    }

    /// Queue a filled rectangle (top-left origin, pixels, RGBA 0..1).
    pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        self.rects.push((x, y, w, h, color));
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
        if self.rects.is_empty() {
            return;
        }
        let (sw, sh) = (screen_w.max(1.0), screen_h.max(1.0));
        let to_ndc = |px: f32, py: f32| [px / sw * 2.0 - 1.0, 1.0 - py / sh * 2.0];
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
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("overlay-v"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let n = verts.len() as u32;
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
            rp.set_pipeline(&self.pipeline);
            rp.set_vertex_buffer(0, vbuf.slice(..));
            rp.draw(0..n, 0..1);
        }
        queue.submit(Some(enc.finish()));
        self.clear();
    }
}
