//! Offscreen 3D scene renderer: many model instances (each at a world
//! position/rotation/scale, with per-mesh textures and a fallback tint) plus a
//! ground plane, from an explicit camera, to a PNG. The in-world view — the
//! same data the client tracks, rendered as real textured B3D models.

use std::io::BufWriter;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};
use pollster::block_on;
use wgpu::util::DeviceExt;

use rcce_data::{B3dModel, Image};

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

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pos: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
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
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
};
@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) color: vec3<f32>) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.uv = uv;
    o.color = color;
    return o;
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let N = normalize(in.normal);
    let L = normalize(vec3<f32>(0.4, 0.85, 0.35));
    let d = max(dot(N, L), 0.0) * 0.7 + 0.4;
    let c = textureSample(tex, samp, in.uv);
    return vec4<f32>(c.rgb * in.color * d, 1.0);
}
"#;

/// Per-mesh normals (model space), computed from faces if absent.
fn mesh_normals(mesh: &rcce_data::B3dMesh) -> Vec<Vec3> {
    let has_n = mesh.normals.len() == mesh.positions.len();
    let mut normals: Vec<Vec3> = if has_n {
        mesh.normals.iter().map(|n| Vec3::from(*n)).collect()
    } else {
        vec![Vec3::ZERO; mesh.positions.len()]
    };
    if !has_n {
        for tri in mesh.indices.chunks_exact(3) {
            let (a, b, c) = (tri[0] as usize, tri[1] as usize, tri[2] as usize);
            let pa = Vec3::from(mesh.positions[a]);
            let fnv =
                (Vec3::from(mesh.positions[b]) - pa).cross(Vec3::from(mesh.positions[c]) - pa);
            normals[a] += fnv;
            normals[b] += fnv;
            normals[c] += fnv;
        }
        for n in &mut normals {
            *n = if n.length_squared() > 1e-12 {
                n.normalize()
            } else {
                Vec3::Y
            };
        }
    }
    normals
}

/// Render the scene to a PNG. `eye`/`target` define the camera.
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
    // Bounds (for the ground plane).
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for inst in instances {
        let t = Vec3::from(inst.translation);
        min = min.min(t);
        max = max.max(t);
    }
    if min.x > max.x {
        min = Vec3::splat(-50.0);
        max = Vec3::splat(50.0);
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
    let bgl0 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
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
    let bind0 = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &bgl0,
        entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }],
    });

    let bgl1 = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
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
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        ..Default::default()
    });
    let make_tex_bind = |img: Option<&Image>| -> wgpu::BindGroup {
        let (w, h, data): (u32, u32, Vec<u8>) = match img {
            Some(i) if i.width > 0 && i.height > 0 => (i.width, i.height, i.rgba.clone()),
            _ => (1, 1, vec![255, 255, 255, 255]),
        };
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("tex"),
            size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
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
            &data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        let view = tex.create_view(&Default::default());
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl1,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        })
    };

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("scene"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[&bgl0, &bgl1],
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
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Float32x3],
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

    struct Drawable {
        vbuf: wgpu::Buffer,
        ibuf: wgpu::Buffer,
        n_idx: u32,
        tex_bind: wgpu::BindGroup,
    }
    let mut drawables: Vec<Drawable> = Vec::new();

    // One drawable per (instance, mesh), baked to world space.
    for inst in instances {
        // Blitz `RotateEntity pitch,yaw,roll` → rotate about Y, then X, then Z.
        let rot = Mat4::from_rotation_y(inst.rot[1])
            * Mat4::from_rotation_x(inst.rot[0])
            * Mat4::from_rotation_z(inst.rot[2]);
        let nrot = Mat3::from_mat4(rot);
        let scale = Vec3::from(inst.scale);
        let trans = Vec3::from(inst.translation);
        for (mi, mesh) in inst.model.meshes.iter().enumerate() {
            if mesh.positions.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            let normals = mesh_normals(mesh);
            let has_uv = mesh.uvs.len() == mesh.positions.len();
            let verts: Vec<Vertex> = mesh
                .positions
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let world = trans + nrot * (Vec3::from(*p) * scale);
                    Vertex {
                        pos: world.into(),
                        normal: (nrot * normals[i]).into(),
                        uv: if has_uv { mesh.uvs[i] } else { [0.0, 0.0] },
                        color: inst.color,
                    }
                })
                .collect();
            let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("v"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("i"),
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let tex = inst.textures.get(mi).and_then(|t| t.as_ref());
            drawables.push(Drawable {
                vbuf,
                ibuf,
                n_idx: mesh.indices.len() as u32,
                tex_bind: make_tex_bind(tex),
            });
        }
    }

    // Ground plane (untextured, tinted).
    {
        let pad = (max - min).length().max(20.0) * 0.4;
        let (gx0, gx1) = (min.x - pad, max.x + pad);
        let (gz0, gz1) = (min.z - pad, max.z + pad);
        let gcol = [0.13, 0.18, 0.14];
        let n = [0.0, 1.0, 0.0];
        let v = |x: f32, z: f32| Vertex { pos: [x, ground_y, z], normal: n, uv: [0.0, 0.0], color: gcol };
        let verts = [v(gx0, gz0), v(gx1, gz0), v(gx1, gz1), v(gx0, gz1)];
        let idx: [u32; 6] = [0, 1, 2, 0, 2, 3];
        let vbuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gv"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ibuf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gi"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        drawables.push(Drawable { vbuf, ibuf, n_idx: 6, tex_bind: make_tex_bind(None) });
    }

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
