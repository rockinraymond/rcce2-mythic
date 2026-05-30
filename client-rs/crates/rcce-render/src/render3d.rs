//! Offscreen 3D renderer: draws a [`B3dModel`] with a perspective camera and
//! simple directional lighting, to a PNG. Verifiable headlessly (look at the
//! image). This is the real-geometry step toward the in-world 3D view; the same
//! pipeline (depth buffer, MVP uniform, per-vertex normals) scales up to the
//! full scene with textures + skeletal animation.

use std::io::BufWriter;

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use pollster::block_on;
use wgpu::util::DeviceExt;

use rcce_data::B3dModel;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    mvp: [f32; 16],
    model: [f32; 16],
}

const SHADER: &str = r#"
struct Uniforms { mvp: mat4x4<f32>, model: mat4x4<f32> };
@group(0) @binding(0) var<uniform> u: Uniforms;
struct VsOut { @builtin(position) clip: vec4<f32>, @location(0) normal: vec3<f32> };

@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(pos, 1.0);
    o.normal = (u.model * vec4<f32>(normal, 0.0)).xyz;
    return o;
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let N = normalize(in.normal);
    let L = normalize(vec3<f32>(0.4, 0.85, 0.35));
    let diff = max(dot(N, L), 0.0) * 0.8 + 0.2;
    let base = vec3<f32>(0.72, 0.76, 0.82);
    return vec4<f32>(base * diff, 1.0);
}
"#;

/// Flatten a model's meshes into one vertex/index buffer, ensuring per-vertex
/// normals (computed from faces when the mesh lacks them).
fn build_geometry(model: &B3dModel) -> (Vec<Vertex>, Vec<u32>, [f32; 3], [f32; 3]) {
    let mut verts: Vec<Vertex> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);

    for mesh in &model.meshes {
        let base = verts.len() as u32;
        let has_normals = mesh.normals.len() == mesh.positions.len();

        for (i, p) in mesh.positions.iter().enumerate() {
            for k in 0..3 {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
            let n = if has_normals {
                mesh.normals[i]
            } else {
                [0.0, 0.0, 0.0] // filled below from faces
            };
            verts.push(Vertex { pos: *p, normal: n });
        }

        // Face-normal accumulation when normals are missing.
        if !has_normals {
            for tri in mesh.indices.chunks_exact(3) {
                let (a, b, c) = (
                    base as usize + tri[0] as usize,
                    base as usize + tri[1] as usize,
                    base as usize + tri[2] as usize,
                );
                let pa = Vec3::from(verts[a].pos);
                let pb = Vec3::from(verts[b].pos);
                let pc = Vec3::from(verts[c].pos);
                let fn_ = (pb - pa).cross(pc - pa);
                for &v in &[a, b, c] {
                    let n = Vec3::from(verts[v].normal) + fn_;
                    verts[v].normal = n.into();
                }
            }
        }

        for &idx in &mesh.indices {
            indices.push(base + idx);
        }
    }

    // Normalize any accumulated normals.
    for v in &mut verts {
        let n = Vec3::from(v.normal);
        if n.length_squared() > 1e-12 {
            v.normal = n.normalize().into();
        } else {
            v.normal = [0.0, 1.0, 0.0];
        }
    }

    if verts.is_empty() {
        min = [0.0; 3];
        max = [0.0; 3];
    }
    (verts, indices, min, max)
}

/// Render `model` to a `width`x`height` PNG at `path`. `yaw` rotates the model
/// (radians) for a chosen viewing angle. Returns the adapter name.
pub fn render_model_png(
    model: &B3dModel,
    yaw: f32,
    width: u32,
    height: u32,
    path: &str,
) -> Result<String, String> {
    let (verts, indices, min, max) = build_geometry(model);
    if verts.is_empty() || indices.is_empty() {
        return Err("model has no geometry".into());
    }

    let center = Vec3::new(
        (min[0] + max[0]) * 0.5,
        (min[1] + max[1]) * 0.5,
        (min[2] + max[2]) * 0.5,
    );
    let extent = Vec3::new(max[0] - min[0], max[1] - min[1], max[2] - min[2]);
    let radius = extent.max_element().max(0.001) * 0.5;

    // Camera: a 3/4 view, framed to the model's bounds.
    let dir = Vec3::new(0.55, 0.45, 1.0).normalize();
    let eye = center + dir * (radius * 3.2);
    let aspect = width as f32 / height as f32;
    let proj = Mat4::perspective_rh(45f32.to_radians(), aspect, radius * 0.05, radius * 20.0);
    let view = Mat4::look_at_rh(eye, center, Vec3::Y);
    let model_mat = Mat4::from_rotation_y(yaw);
    let mvp = proj * view * model_mat;

    let uniforms = Uniforms {
        mvp: mvp.to_cols_array(),
        model: model_mat.to_cols_array(),
    };

    // ---- wgpu setup ----
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
            label: Some("rcce-render3d"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_defaults(),
            memory_hints: wgpu::MemoryHints::Performance,
        },
        None,
    ))
    .map_err(|e| format!("request_device: {e}"))?;

    let color_format = wgpu::TextureFormat::Rgba8Unorm;
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: color_format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let color_view = target.create_view(&Default::default());

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
        label: Some("uniforms"),
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
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: ubuf.as_entire_binding(),
        }],
    });

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("model"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bgl],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("model"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs",
            compilation_options: Default::default(),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs",
            compilation_options: Default::default(),
            targets: &[Some(color_format.into())],
        }),
        primitive: wgpu::PrimitiveState {
            cull_mode: None, // B3D winding varies; draw both sides
            ..Default::default()
        },
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
        label: Some("verts"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("indices"),
        contents: bytemuck::cast_slice(&indices),
        usage: wgpu::BufferUsages::INDEX,
    });

    let bpp = 4u32;
    let unpadded = width * bpp;
    let padded = unpadded.div_ceil(256) * 256;
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
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
                    load: wgpu::LoadOp::Clear(wgpu::Color { r: 0.07, g: 0.09, b: 0.13, a: 1.0 }),
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
            texture: &target,
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
