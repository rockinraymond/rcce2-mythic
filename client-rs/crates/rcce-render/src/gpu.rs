//! Shared GPU primitives for the textured-mesh renderers: the vertex format,
//! the WGSL shader, normal computation, the render pipeline, and per-instance
//! drawable building. Both the offscreen PNG path (`scene::render_scene_png`)
//! and the real-time window (`world_view::ScenePipeline`) build on these so the
//! two stay visually identical.

use std::collections::HashMap;
use std::rc::Rc;

use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec3};
use wgpu::util::DeviceExt;

use rcce_data::{B3dMesh, Image};

use crate::scene::SceneInstance;

/// Cache of uploaded texture bind groups, keyed by a caller-supplied string
/// (e.g. an actor's appearance). Lets per-frame actor rebuilds reuse the
/// already-uploaded skin instead of re-uploading every frame.
pub type TexCache = HashMap<String, Rc<wgpu::BindGroup>>;

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

/// Strength of the zone's directional light relative to its ambient floor.
pub const LIGHT_INTENSITY: f32 = 0.6;

/// Camera + atmosphere uniform. Field order matches the WGSL `U` block: each
/// vec3 packs a trailing scalar into its 16-byte slot (eye|fog_near,
/// fog_color|fog_far, ambient|light_intensity, light_dir|pad), so the struct is
/// a tight 128 bytes with no implicit padding.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Uniforms {
    pub mvp: [f32; 16],
    pub eye: [f32; 3],
    pub fog_near: f32,
    pub fog_color: [f32; 3],
    pub fog_far: f32,
    pub ambient: [f32; 3],
    pub light_intensity: f32,
    pub light_dir: [f32; 3],
    pub _pad: f32,
}

impl Uniforms {
    pub fn new(
        mvp: [f32; 16],
        eye: [f32; 3],
        fog_color: [f32; 3],
        fog_near: f32,
        fog_far: f32,
        ambient: [f32; 3],
        light_dir: [f32; 3],
    ) -> Uniforms {
        Uniforms {
            mvp,
            eye,
            fog_near,
            fog_color,
            fog_far,
            ambient,
            light_intensity: LIGHT_INTENSITY,
            light_dir,
            _pad: 0.0,
        }
    }
}

pub const SHADER: &str = r#"
struct U {
    mvp: mat4x4<f32>,
    eye: vec3<f32>,
    fog_near: f32,
    fog_color: vec3<f32>,
    fog_far: f32,
    ambient: vec3<f32>,
    light_intensity: f32,
    light_dir: vec3<f32>,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) world: vec3<f32>,
};
@vertex fn vs(@location(0) pos: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) uv: vec2<f32>, @location(3) color: vec3<f32>) -> VsOut {
    var o: VsOut;
    o.clip = u.mvp * vec4<f32>(pos, 1.0);
    o.normal = normal;
    o.uv = uv;
    o.color = color;
    o.world = pos;
    return o;
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    if (c.a < 0.5) { discard; }
    let N = normalize(in.normal);
    let L = normalize(u.light_dir);
    // Two-sided (cull is off): abs() keeps backfaces/interiors lit. The zone's
    // ambient is the floor; its directional light adds on top.
    let diff = abs(dot(N, L)) * u.light_intensity;
    let shade = u.ambient + vec3<f32>(diff);
    let lit = c.rgb * in.color * shade;
    // Distance fog toward the sky/fog colour.
    let dist = distance(in.world, u.eye);
    let f = clamp((dist - u.fog_near) / max(u.fog_far - u.fog_near, 1.0), 0.0, 1.0);
    return vec4<f32>(mix(lit, u.fog_color, f), 1.0);
}
"#;

/// Zenith (top-of-sky) colour derived from the horizon/fog colour: bluer and a
/// touch darker, so the gradient reads as sky. Clamped to [0,1].
pub fn sky_zenith(horizon: [f32; 3]) -> [f32; 3] {
    [
        (horizon[0] * 0.45).clamp(0.0, 1.0),
        (horizon[1] * 0.6 + 0.05).clamp(0.0, 1.0),
        (horizon[2] * 0.85 + 0.18).clamp(0.0, 1.0),
    ]
}

/// Fullscreen sky drawn at the far plane (no depth write) so the world's
/// geometry renders over it. Falls back to a vertical fog gradient; when a sky
/// texture is set (the area's `SkyTexID`) it's sampled by screen UV, panned
/// horizontally with the camera yaw, and faded into the gradient at the horizon.
pub struct SkyPipeline {
    pipeline: wgpu::RenderPipeline,
    buf: wgpu::Buffer,
    bind: wgpu::BindGroup,
    tex_bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    tex_bind: wgpu::BindGroup,
    has_texture: bool,
    cloud_bind: wgpu::BindGroup,
    has_clouds: bool,
    stars_bind: wgpu::BindGroup,
    has_stars: bool,
}

const SKY_SHADER: &str = r#"
struct SkyU { top: vec4<f32>, bottom: vec4<f32>, params: vec4<f32>, params2: vec4<f32> };
@group(0) @binding(0) var<uniform> sky: SkyU;
@group(1) @binding(0) var skytex: texture_2d<f32>;
@group(1) @binding(1) var skysamp: sampler;
@group(2) @binding(0) var cloudtex: texture_2d<f32>;
@group(2) @binding(1) var cloudsamp: sampler;
@group(3) @binding(0) var starstex: texture_2d<f32>;
@group(3) @binding(1) var starssamp: sampler;
struct VO { @builtin(position) pos: vec4<f32>, @location(0) t: f32, @location(1) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) vi: u32) -> VO {
    var p = array<vec2<f32>, 3>(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    var o: VO;
    let xy = p[vi];
    o.pos = vec4<f32>(xy, 1.0, 1.0); // z = 1 → far plane
    o.t = (xy.y + 1.0) * 0.5;        // 0 at screen bottom, 1 at top
    o.uv = vec2<f32>((xy.x + 1.0) * 0.5, 1.0 - (xy.y + 1.0) * 0.5); // screen uv (y down)
    return o;
}
@fragment fn fs(i: VO) -> @location(0) vec4<f32> {
    let grad = mix(sky.bottom.rgb, sky.top.rgb, clamp(i.t, 0.0, 1.0));
    var col = grad;
    if (sky.params.x >= 0.5) {
        // Sky texture: pan horizontally with the camera yaw (Repeat sampler);
        // fade into the fog gradient at the horizon so it meets the terrain.
        let uv = vec2<f32>(i.uv.x + sky.params.y, i.uv.y);
        let tex = textureSample(skytex, skysamp, uv).rgb;
        let h = smoothstep(0.0, 0.30, i.t);
        col = mix(grad, tex, h);
    }
    if (sky.params2.x >= 0.5 && sky.params2.y > 0.01) {
        // Stars (behind clouds): additive so a black background adds nothing and
        // white stars add light. Gated by the night factor (params2.y).
        let suv = vec2<f32>(i.uv.x + sky.params2.z, i.uv.y);
        let s = textureSample(starstex, starssamp, suv).rgb;
        let sfade = smoothstep(0.05, 0.40, i.t);
        col = col + s * sky.params2.y * sfade;
    }
    if (sky.params.z >= 0.5) {
        // Cloud overlay: pans faster than the sky, alpha-composited, and only
        // well above the horizon (so it doesn't smear into the terrain).
        let cuv = vec2<f32>(i.uv.x + sky.params.w, i.uv.y);
        let c = textureSample(cloudtex, cloudsamp, cuv);
        let fade = smoothstep(0.12, 0.45, i.t);
        col = mix(col, c.rgb, c.a * fade);
    }
    return vec4<f32>(col, 1.0);
}
"#;

impl SkyPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> SkyPipeline {
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-u"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("sky"),
            size: 64, // four vec4 (top, bottom, params, params2)
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        // Texture+sampler bind group (group 1). Starts as a 1×1 white default so
        // the pipeline always has a valid binding; params.x gates its use.
        let tex_bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("sky-tex-bgl"),
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
            label: Some("sky-sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        // 1×1 defaults — never sampled (params gates each), so no upload needed.
        let tex_bind = make_sky_tex_bind(device, None, &tex_bgl, &sampler, 1, 1, None);
        let cloud_bind = make_sky_tex_bind(device, None, &tex_bgl, &sampler, 1, 1, None);
        let stars_bind = make_sky_tex_bind(device, None, &tex_bgl, &sampler, 1, 1, None);
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("sky"),
            source: wgpu::ShaderSource::Wgsl(SKY_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bgl, &tex_bgl, &tex_bgl, &tex_bgl], // groups 1-3 share the tex layout
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("sky"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs",
                compilation_options: Default::default(),
                targets: &[Some(color_format.into())],
            }),
            primitive: wgpu::PrimitiveState::default(),
            // Behind everything: never writes depth, always passes so it fills
            // the cleared frame; geometry (depth Less) then draws on top.
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });
        SkyPipeline {
            pipeline,
            buf,
            bind,
            tex_bgl,
            sampler,
            tex_bind,
            has_texture: false,
            cloud_bind,
            has_clouds: false,
            stars_bind,
            has_stars: false,
        }
    }

    pub fn set_colors(&self, queue: &wgpu::Queue, top: [f32; 3], bottom: [f32; 3]) {
        let data: [f32; 8] = [top[0], top[1], top[2], 1.0, bottom[0], bottom[1], bottom[2], 1.0];
        queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&data));
    }

    /// Per-frame params: `yaw` (radians) pans the sky texture; clouds pan faster
    /// (1.6×) plus a slow time drift so they move even when standing still. The
    /// has-sky / has-clouds flags come from whether those textures are set.
    pub fn set_frame(&self, queue: &wgpu::Queue, yaw: f32, time: f32, night: f32) {
        use std::f32::consts::PI;
        let has_sky = if self.has_texture { 1.0 } else { 0.0 };
        let has_clouds = if self.has_clouds { 1.0 } else { 0.0 };
        let has_stars = if self.has_stars { 1.0 } else { 0.0 };
        let off = yaw / (2.0 * PI);
        let cloud_off = off * 1.6 + time * 0.004;
        // params = [has_sky, sky_off, has_clouds, cloud_off];
        // params2 = [has_stars, night, star_off, _]. Stars pan with the sky.
        let params: [f32; 8] = [
            has_sky, off, has_clouds, cloud_off,
            has_stars, night.clamp(0.0, 1.0), off, 0.0,
        ];
        queue.write_buffer(&self.buf, 32, bytemuck::cast_slice(&params));
    }

    /// Upload the area's sky texture (RGBA8). Replaces any previous one and
    /// enables textured-sky rendering.
    pub fn set_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        self.tex_bind = make_sky_tex_bind(device, Some(queue), &self.tex_bgl, &self.sampler, width, height, Some(rgba));
        self.has_texture = true;
    }

    /// Clear the sky texture (revert to the plain gradient).
    pub fn clear_texture(&mut self) {
        self.has_texture = false;
    }

    /// Upload the area's cloud texture (RGBA8 with alpha) for the drifting cloud
    /// overlay. Replaces any previous one and enables clouds.
    pub fn set_cloud_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        self.cloud_bind = make_sky_tex_bind(device, Some(queue), &self.tex_bgl, &self.sampler, width, height, Some(rgba));
        self.has_clouds = true;
    }

    /// Clear the cloud overlay.
    pub fn clear_clouds(&mut self) {
        self.has_clouds = false;
    }

    /// Upload the area's night-stars texture (RGBA8). Shown additively, gated by
    /// the night factor passed to [`set_frame`].
    pub fn set_stars_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) {
        if width == 0 || height == 0 || rgba.len() < (width * height * 4) as usize {
            return;
        }
        self.stars_bind = make_sky_tex_bind(device, Some(queue), &self.tex_bgl, &self.sampler, width, height, Some(rgba));
        self.has_stars = true;
    }

    /// Clear the stars overlay.
    pub fn clear_stars(&mut self) {
        self.has_stars = false;
    }

    pub fn draw<'a>(&'a self, rp: &mut wgpu::RenderPass<'a>) {
        rp.set_pipeline(&self.pipeline);
        rp.set_bind_group(0, &self.bind, &[]);
        rp.set_bind_group(1, &self.tex_bind, &[]);
        rp.set_bind_group(2, &self.cloud_bind, &[]);
        rp.set_bind_group(3, &self.stars_bind, &[]);
        rp.draw(0..3, 0..1);
    }
}

/// Build a sky texture bind group; uploads `rgba` when a queue + pixels are
/// given (the 1×1 default passes `None`/`None` and is never sampled). The bind
/// group retains the texture via its view, so the handle can be dropped here.
fn make_sky_tex_bind(
    device: &wgpu::Device,
    queue: Option<&wgpu::Queue>,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    width: u32,
    height: u32,
    rgba: Option<&[u8]>,
) -> wgpu::BindGroup {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sky-tex"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    if let (Some(queue), Some(rgba)) = (queue, rgba) {
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
    }
    let view = tex.create_view(&Default::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("sky-tex-bind"),
        layout: bgl,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}

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
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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

// ── GPU skinning ───────────────────────────────────────────────────────────
// Upload each actor template mesh's STATIC vbuf once (bind-pose verts + per-
// vertex bone ids/weights); per frame write only the actor uniform (the bone
// matrix palette + model transform + colour). The vertex shader does linear-
// blend skinning, so no CPU re-skin / vertex re-upload per frame.

/// Max bones in the per-actor uniform palette. 64 mat4x4 = 4 KB (well under the
/// 64 KB uniform limit); actor skeletons are far smaller.
pub const MAX_BONES: usize = 64;

/// A skinned mesh vertex: bind-pose position/normal/uv + up to 4 bone indices
/// and weights (from [`rcce_data::B3dModel::skin_attributes`]).
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct SkinnedVertex {
    pub pos: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub bones: [u32; 4],
    pub weights: [f32; 4],
}

/// Per-actor skinning uniform: the column-major bone palette
/// ([`rcce_data::B3dModel::bone_palette`]), the instance model matrix
/// (column-major), and the actor tint. Matches the WGSL `Actor` block.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct ActorSkin {
    pub bones: [[f32; 16]; MAX_BONES],
    pub model: [f32; 16],
    pub color: [f32; 4],
}

impl ActorSkin {
    /// Build from a column-major bone palette (truncated/padded to MAX_BONES),
    /// a column-major model matrix, and a colour.
    pub fn new(palette: &[[f32; 16]], model: [f32; 16], color: [f32; 3]) -> ActorSkin {
        let mut bones = [IDENTITY16; MAX_BONES];
        for (i, m) in palette.iter().take(MAX_BONES).enumerate() {
            bones[i] = *m;
        }
        ActorSkin { bones, model, color: [color[0], color[1], color[2], 1.0] }
    }
}

/// Column-major 4×4 identity (for the default bone palette).
const IDENTITY16: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, //
    0.0, 1.0, 0.0, 0.0, //
    0.0, 0.0, 1.0, 0.0, //
    0.0, 0.0, 0.0, 1.0,
];

const SKIN_SHADER: &str = r#"
struct U {
    mvp: mat4x4<f32>,
    eye: vec3<f32>,
    fog_near: f32,
    fog_color: vec3<f32>,
    fog_far: f32,
    ambient: vec3<f32>,
    light_intensity: f32,
    light_dir: vec3<f32>,
    _pad: f32,
};
@group(0) @binding(0) var<uniform> u: U;
@group(1) @binding(0) var tex: texture_2d<f32>;
@group(1) @binding(1) var samp: sampler;
struct Actor {
    bones: array<mat4x4<f32>, 64>,
    model: mat4x4<f32>,
    color: vec4<f32>,
};
@group(2) @binding(0) var<uniform> a: Actor;
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec3<f32>,
    @location(3) world: vec3<f32>,
};
@vertex fn vs(
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) bones: vec4<u32>,
    @location(4) weights: vec4<f32>,
) -> VsOut {
    let wsum = weights.x + weights.y + weights.z + weights.w;
    var sp = vec3<f32>(0.0);
    var sn = vec3<f32>(0.0);
    if (wsum > 0.0001) {
        for (var i = 0u; i < 4u; i = i + 1u) {
            let w = weights[i];
            if (w > 0.0) {
                let m = a.bones[bones[i]];
                sp = sp + w * (m * vec4<f32>(pos, 1.0)).xyz;
                sn = sn + w * (m * vec4<f32>(normal, 0.0)).xyz;
            }
        }
    } else {
        sp = pos;
        sn = normal;
    }
    let world = a.model * vec4<f32>(sp, 1.0);
    var o: VsOut;
    o.clip = u.mvp * world;
    o.normal = (a.model * vec4<f32>(sn, 0.0)).xyz;
    o.uv = uv;
    o.color = a.color.rgb;
    o.world = world.xyz;
    return o;
}
@fragment fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let c = textureSample(tex, samp, in.uv);
    if (c.a < 0.5) { discard; }
    let N = normalize(in.normal);
    let L = normalize(u.light_dir);
    let diff = abs(dot(N, L)) * u.light_intensity;
    let shade = u.ambient + vec3<f32>(diff);
    let lit = c.rgb * in.color * shade;
    let dist = distance(in.world, u.eye);
    let f = clamp((dist - u.fog_near) / max(u.fog_far - u.fog_near, 1.0), 0.0, 1.0);
    return vec4<f32>(mix(lit, u.fog_color, f), 1.0);
}
"#;

/// GPU linear-blend-skinning pipeline. Reuses the camera (group 0) and texture
/// (group 1) layouts from the base [`Pipeline`]; adds the per-actor skinning
/// uniform (group 2).
pub struct SkinPipeline {
    pub pipeline: wgpu::RenderPipeline,
    pub bgl_actor: wgpu::BindGroupLayout,
}

impl SkinPipeline {
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat, base: &Pipeline) -> SkinPipeline {
        let bgl_actor = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("actor-skin"),
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
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skin"),
            source: wgpu::ShaderSource::Wgsl(SKIN_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skin"),
            bind_group_layouts: &[&base.bgl_uniform, &base.bgl_texture, &bgl_actor],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skin"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs",
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<SkinnedVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x2, 3 => Uint32x4, 4 => Float32x4],
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
        SkinPipeline { pipeline, bgl_actor }
    }

    /// Create the per-actor uniform buffer + bind group (group 2). Update it each
    /// frame with [`update_actor`](Self::update_actor).
    pub fn make_actor(&self, device: &wgpu::Device) -> (wgpu::Buffer, wgpu::BindGroup) {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("actor-skin"),
            size: std::mem::size_of::<ActorSkin>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("actor-skin"),
            layout: &self.bgl_actor,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() }],
        });
        (buf, bind)
    }

    pub fn update_actor(&self, queue: &wgpu::Queue, buf: &wgpu::Buffer, skin: &ActorSkin) {
        queue.write_buffer(buf, 0, bytemuck::bytes_of(skin));
    }
}

/// Build the static skinned vertex buffer for `mesh` (bind-pose pos/normal/uv +
/// the per-vertex bone ids/weights). Uploaded once; the pose comes from the
/// per-frame bone palette in the shader.
pub fn build_skinned_vbuf(
    device: &wgpu::Device,
    mesh: &B3dMesh,
    bone_ids: &[[u32; 4]],
    weights: &[[f32; 4]],
) -> wgpu::Buffer {
    let normals = mesh_normals(mesh);
    let has_uv = mesh.uvs.len() == mesh.positions.len();
    let verts: Vec<SkinnedVertex> = mesh
        .positions
        .iter()
        .enumerate()
        .map(|(i, p)| SkinnedVertex {
            pos: *p,
            normal: normals[i].into(),
            uv: if has_uv { mesh.uvs[i] } else { [0.0, 0.0] },
            bones: bone_ids.get(i).copied().unwrap_or([0; 4]),
            weights: weights.get(i).copied().unwrap_or([0.0; 4]),
        })
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("skin-v"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

/// A single uploaded mesh ready to draw. `tex_bind` and `ibuf` are
/// reference-counted so dynamic rebuilds can share cached uploads — the index
/// (topology) buffer is constant across animation frames, so it's cached and
/// reused instead of recreated every rebuild.
pub struct Drawable {
    pub vbuf: wgpu::Buffer,
    pub ibuf: Rc<wgpu::Buffer>,
    pub n_idx: u32,
    pub tex_bind: Rc<wgpu::BindGroup>,
}

/// Cache of constant index (topology) buffers, keyed like [`TexCache`].
pub type IndexCache = HashMap<String, Rc<wgpu::Buffer>>;

/// Bake one mesh into world-space vertex/index buffers (no texture). The
/// instance's rotation/scale/translation are applied per vertex.
fn bake_mesh(
    device: &wgpu::Device,
    nrot: Mat3,
    scale: Vec3,
    trans: Vec3,
    color: [f32; 3],
    mesh: &B3dMesh,
) -> (wgpu::Buffer, Rc<wgpu::Buffer>, u32) {
    let vbuf = bake_verts(device, nrot, scale, trans, color, mesh);
    let ibuf = Rc::new(bake_indices(device, mesh));
    (vbuf, ibuf, mesh.indices.len() as u32)
}

/// World-space vertex buffer for `mesh` (positions transformed by the instance).
/// Rebuilt every frame for animated meshes (positions change).
fn bake_verts(device: &wgpu::Device, nrot: Mat3, scale: Vec3, trans: Vec3, color: [f32; 3], mesh: &B3dMesh) -> wgpu::Buffer {
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
                color,
            }
        })
        .collect();
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("v"),
        contents: bytemuck::cast_slice(&verts),
        usage: wgpu::BufferUsages::VERTEX,
    })
}

/// Index (topology) buffer for `mesh` — constant across animation frames.
fn bake_indices(device: &wgpu::Device, mesh: &B3dMesh) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("i"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    })
}

/// Instance rotation matrix (Y·X·Z) from `rot` radians.
fn inst_nrot(rot: [f32; 3]) -> Mat3 {
    Mat3::from_mat4(
        Mat4::from_rotation_y(rot[1]) * Mat4::from_rotation_x(rot[0]) * Mat4::from_rotation_z(rot[2]),
    )
}

/// Build dynamic actor drawables, reusing cached texture binds keyed by
/// `keys[i]` (one per instance). Geometry (posed verts) is rebuilt every call;
/// only the texture upload is cached. No ground plane.
pub fn build_actor_drawables_cached(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &Pipeline,
    instances: &[SceneInstance],
    keys: &[String],
    cache: &mut TexCache,
    idx_cache: &mut IndexCache,
) -> Vec<Drawable> {
    let mut drawables = Vec::new();
    for (ii, inst) in instances.iter().enumerate() {
        let nrot = inst_nrot(inst.rot);
        let scale = Vec3::from(inst.scale);
        let trans = Vec3::from(inst.translation);
        let key = keys.get(ii).map(String::as_str).unwrap_or("");
        for (mi, mesh) in inst.model.meshes.iter().enumerate() {
            if mesh.positions.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            // Posed vertices change every frame; the index topology is constant,
            // so cache + reuse the index buffer instead of recreating it.
            let vbuf = bake_verts(device, nrot, scale, trans, inst.color, mesh);
            let ckey = format!("{key}:{mi}");
            let ibuf = idx_cache
                .entry(ckey.clone())
                .or_insert_with(|| Rc::new(bake_indices(device, mesh)))
                .clone();
            let n_idx = mesh.indices.len() as u32;
            let tex = inst.textures.get(mi).and_then(|t| t.as_ref());
            let tex_bind = cache
                .entry(ckey)
                .or_insert_with(|| Rc::new(pipeline.texture_bind(device, queue, tex)))
                .clone();
            drawables.push(Drawable { vbuf, ibuf, n_idx, tex_bind });
        }
    }
    drawables
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
    let (mut drawables, min, max) = build_instance_drawables(device, queue, pipeline, instances);

    // Ground plane spanning the instances.
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
        let ibuf = Rc::new(device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("gi"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        }));
        drawables.push(Drawable {
            vbuf,
            ibuf,
            n_idx: 6,
            tex_bind: Rc::new(pipeline.texture_bind(device, queue, None)),
        });
    }
    drawables
}

/// Core: bake every (instance, mesh) into a world-space drawable; also returns
/// the bounding box (min,max) of the instance translations.
fn build_instance_drawables(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    pipeline: &Pipeline,
    instances: &[SceneInstance],
) -> (Vec<Drawable>, Vec3, Vec3) {
    let mut drawables = Vec::new();
    let (mut min, mut max) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));

    for inst in instances {
        let nrot = inst_nrot(inst.rot);
        let scale = Vec3::from(inst.scale);
        let trans = Vec3::from(inst.translation);
        min = min.min(trans);
        max = max.max(trans);
        for (mi, mesh) in inst.model.meshes.iter().enumerate() {
            if mesh.positions.is_empty() || mesh.indices.is_empty() {
                continue;
            }
            let (vbuf, ibuf, n_idx) = bake_mesh(device, nrot, scale, trans, inst.color, mesh);
            let tex = inst.textures.get(mi).and_then(|t| t.as_ref());
            drawables.push(Drawable {
                vbuf,
                ibuf,
                n_idx,
                tex_bind: Rc::new(pipeline.texture_bind(device, queue, tex)),
            });
        }
    }
    (drawables, min, max)
}

#[cfg(test)]
mod tests {
    use super::sky_zenith;

    #[test]
    fn zenith_is_bluer_and_darker_than_horizon() {
        // A typical pale-blue horizon/fog colour.
        let horizon = [0.45, 0.62, 0.82];
        let z = sky_zenith(horizon);
        // Bluer: blue channel dominates more than at the horizon.
        assert!(z[2] > z[0] && z[2] > z[1], "zenith should be blue-dominant: {z:?}");
        // Darker in red/green than the horizon (gradient brightens toward it).
        assert!(z[0] < horizon[0] && z[1] < horizon[1], "{z:?} vs {horizon:?}");
        // Stays in range.
        assert!(z.iter().all(|c| (0.0..=1.0).contains(c)));
    }

    #[test]
    fn zenith_clamps() {
        let z = sky_zenith([2.0, 2.0, 2.0]);
        assert!(z.iter().all(|c| *c <= 1.0));
    }
}
