//! Shared GPU primitives for the textured-mesh renderers: the vertex format,
//! the WGSL shader, normal computation, the render pipeline, and per-instance
//! drawable building. Both the offscreen PNG path (`scene::render_scene_png`)
//! and the real-time window (`world_view::ScenePipeline`) build on these so the
//! two stay visually identical.

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};
use wgpu::util::DeviceExt;

use rcce_data::{B3dMesh, Image};

use crate::scene::SceneInstance;

pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    pub mvp: [f32; 16],
}

pub const SHADER: &str = r#"
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
    let c = textureSample(tex, samp, in.uv);
    if (c.a < 0.5) { discard; }
    let N = normalize(in.normal);
    let L = normalize(vec3<f32>(0.4, 0.85, 0.35));
    let d = max(abs(dot(N, L)), 0.0) * 0.7 + 0.4;
    return vec4<f32>(c.rgb * in.color * d, 1.0);
}
"#;

/// Per-mesh normals (model space), computed from faces if absent.
pub fn mesh_normals(mesh: &B3dMesh) -> Vec<Vec3> {
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

/// Bind-group layouts + sampler + pipeline for the textured shader, targeting
/// `color_format` with a `Depth32Float` depth buffer.
pub struct Pipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bgl_uniform: wgpu::BindGroupLayout,
    pub bgl_texture: wgpu::BindGroupLayout,
    pub sampler: wgpu::Sampler,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Pipeline {
        let bgl_uniform = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("u"),
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
        let bgl_texture = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tex"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("scene"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl_uniform, &bgl_texture],
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
                format: DEPTH_FORMAT,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        Pipeline {
            pipeline,
            bgl_uniform,
            bgl_texture,
            sampler,
        }
    }

    /// Upload an `Image` (or a 1x1 white fallback) as a texture bind group.
    pub fn texture_bind(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        img: Option<&Image>,
    ) -> wgpu::BindGroup {
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
            layout: &self.bgl_texture,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        })
    }
}

/// A single uploaded mesh ready to draw.
pub struct Drawable {
    pub vbuf: wgpu::Buffer,
    pub ibuf: wgpu::Buffer,
    pub n_idx: u32,
    pub tex_bind: wgpu::BindGroup,
}

/// Bake every (instance, mesh) into a world-space [`Drawable`] (positions
/// transformed by the instance's rot/scale/translation). A ground plane spanning
/// the instances is appended (untextured, tinted).
pub fn build_drawables(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &Pipeline,
    instances: &[SceneInstance],
    ground_y: f32,
) -> Vec<Drawable> {
    let mut drawables = Vec::new();
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));

    for inst in instances {
        let rot = Mat4::from_rotation_y(inst.rot[1])
            * Mat4::from_rotation_x(inst.rot[0])
            * Mat4::from_rotation_z(inst.rot[2]);
        let nrot = Mat3::from_mat4(rot);
        let scale = Vec3::from(inst.scale);
        let trans = Vec3::from(inst.translation);
        min = min.min(trans);
        max = max.max(trans);
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
                tex_bind: pipeline.texture_bind(device, queue, tex),
            });
        }
    }

    // Ground plane.
    if min.x <= max.x {
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
        drawables.push(Drawable {
            vbuf,
            ibuf,
            n_idx: 6,
            tex_bind: pipeline.texture_bind(device, queue, None),
        });
    }
    drawables
}
