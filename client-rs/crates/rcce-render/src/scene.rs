//! Offscreen 3D scene renderer: draws many model instances (each at a world
//! position/rotation/scale, with a tint) plus a ground plane, from an explicit
//! camera, to a PNG. This is the in-world view — the same data the client tracks
//! (player + actors), rendered as their real B3D models. Textures + skeletal
//! animation layer on later; the scene plumbing is here.

use std::io::BufWriter;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use pollster::block_on;
use wgpu::util::DeviceExt;

use rcce_data::B3dModel;

/// One model placed in the world.
pub struct SceneInstance<'a> {
    pub model: &'a B3dModel,
    pub translation: [f32; 3],
    pub yaw: f32,
    pub scale: f32,
    pub color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    mvp: [f32; 16],
}

const SHADER: &str = r#"
struct U { mvp: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: U;
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec3<f32>,
};
@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec3<f32>) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.color = color;
    return o;
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let N = normalize(in.normal);
    let L = normalize(vec3<f32>(0.4, 0.85, 0.35));
    let d = max(dot(N, L), 0.0) * 0.75 + 0.25;
    return vec4<f32>(in.color * d, 1.0);
}
"#;

/// Per-vertex normals for a model (computed from faces if absent), in model space.
fn model_normals(model: &B3dModel) -> Vec<(Vec3, Vec3)> {
    // Returns (position, normal) for every vertex across all meshes, with a
    // shared index space implicit per mesh; we flatten meshes sequentially.
    let mut out = Vec::new();
    for mesh in &model.meshes {
        let base = out.len();
        let has_n = mesh.normals.len() == mesh.positions.len();
        for (i, p) in mesh.positions.iter().enumerate() {
            let n = if has_n { Vec3::from(mesh.normals[i]) } else { Vec3::ZERO };
            out.push((Vec3::from(*p), n));
        }
        if !has_n {
            for tri in mesh.indices.chunks_exact(3) {
                let (a, b, c) = (
                    base + tri[0] as usize,
                    base + tri[1] as usize,
                    base + tri[2] as usize,
                );
                let fnormal = (out[b].0 - out[a].0).cross(out[c].0 - out[a].0);
                out[a].1 += fnormal;
                out[b].1 += fnormal;
                out[c].1 += fnormal;
            }
        }
    }
    for (_, n) in out.iter_mut() {
        *n = if n.length_squared() > 1e-12 { n.normalize() } else { Vec3::Y };
    }
    out
}

/// Bake all instances into one world-space vertex/index buffer.
fn bake(instances: &[SceneInstance], ground_y: f32) -> (Vec<Vertex>, Vec<u32>, Vec3, Vec3) {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));

    for inst in instances {
        let rot = Mat4::from_rotation_y(inst.yaw);
        let nrot = glam::Mat3::from_mat4(rot);
        let pernorm = model_normals(inst.model);

        // Per-mesh index base offsets within this instance's flattened verts.
        let mut mesh_base = 0usize;
        let inst_vert_base = verts.len() as u32;
        for mesh in &inst.model.meshes {
            for (i, _p) in mesh.positions.iter().enumerate() {
                let (lp, ln) = pernorm[mesh_base + i];
                let world = Vec3::from(inst.translation) + nrot * (lp * inst.scale);
                min = min.min(world);
                max = max.max(world);
                verts.push(Vertex {
                    pos: world.into(),
                    normal: (nrot * ln).into(),
                    color: inst.color,
                });
            }
            let local_base = inst_vert_base + mesh_base as u32;
            for &idx in &mesh.indices {
                indices.push(local_base + idx);
            }
            mesh_base += mesh.positions.len();
        }
    }

    // Ground plane spanning the scene (with margin).
    if min.x > max.x {
        min = Vec3::splat(-50.0);
        max = Vec3::splat(50.0);
    }
    let pad = (max - min).length().max(20.0) * 0.4;
    let (gx0, gx1) = (min.x - pad, max.x + pad);
    let (gz0, gz1) = (min.z - pad, max.z + pad);
    let gcol = [0.13, 0.18, 0.14];
    let gn = [0.0, 1.0, 0.0];
    let gb = verts.len() as u32;
    for (x, z) in [(gx0, gz0), (gx1, gz0), (gx1, gz1), (gx0, gz1)] {
        verts.push(Vertex { pos: [x, ground_y, z], normal: gn, color: gcol });
    }
    indices.extend_from_slice(&[gb, gb + 1, gb + 2, gb, gb + 2, gb + 3]);

    (verts, indices, min, max)
}

/// Render the scene to a PNG. `eye`/`target` define the camera. Returns the
/// adapter name.
#[allow(clippy::too_many_arguments)]
pub fn render_scene_png(
    instances: &[SceneInstance],
    eye: [f32; 3],
    target: [f32; 3],
    ground_y: f32,
    width: u32,
    height: u32,
    path: &str,
) -> Result<String, String> {
    let (verts, indices, _min, _max) = bake(instances, ground_y);
    if indices.is_empty() {
        return Err("empty scene".into());
    }

    let aspect = width as f32 / height as f32;
    let proj = Mat4::perspective_rh(50f32.to_radians(), aspect, 1.0, 100_000.0);
    let view = Mat4::look_at_rh(Vec3::from(eye), Vec3::from(target), Vec3::Y);
    let uniforms = Uniforms {
        mvp: (proj * view).to_cols_array(),
    };

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

    let color_format = wgpu::TextureFormat::Rgba8Unorm;
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
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth.create_view(&Default::default());

    let ubuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("u"),
        contents: bytemuck::bytes_of(&uniforms),
        usage: wgpu::BufferUsages::UNIFORM,
    });
    let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scene"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("scene"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs",
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            compilation_options: Default::default(),
            targets: &[Some(color_format.into())],
        }),
        primitive: wgpu::PrimitiveState { cull_mode: None, ..Default::default() },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: true,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("v"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("i"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

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
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.45, g: 0.62, b: 0.82, a: 1.0 }),
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
        rp.set_pipeline(&pipeline);
        rp.set_bind_group(0, &bind_group, &[]);
        rp.set_vertex_buffer(0, vbuf.slice(..));
        rp.set_index_buffer(ibuf.slice(..), wgpu::IndexFormat::Uint32);
        rp.draw_indexed(0..indices.len() as u32, 0, 0..1);
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
