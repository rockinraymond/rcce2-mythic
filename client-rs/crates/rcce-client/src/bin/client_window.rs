//! Real-time client window: opens a winit window backed by a wgpu surface and
//! renders a real project zone (terrain + scenery) with an auto-orbiting camera,
//! at the display refresh rate. This is the live render spine; driving it from a
//! logged-in world + animated actors is the next step.
//!
//!   cargo run -p rcce-client --bin client-window --release -- [zone]
//!
//! NOTE: needs a display. In a headless/agent environment it still opens on the
//! host desktop and logs init + a periodic frame/scene summary to stdout, which
//! is how we sanity-check it without seeing pixels.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use rcce_data::{AreaScenery, B3dModel, Image};
use rcce_render::WorldView;

struct Gfx {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

impl Gfx {
    fn new(window: Arc<Window>) -> (Gfx, wgpu::TextureFormat) {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).expect("surface");
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .expect("adapter");
        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("window"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::downlevel_defaults(),
                memory_hints: wgpu::MemoryHints::Performance,
            },
            None,
        ))
        .expect("device");
        let caps = surface.get_capabilities(&adapter);
        // Use a non-srgb format so our Rgba8Unorm textures aren't double-gamma'd.
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| !f.is_srgb())
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        println!(
            "[client-window] {}x{} via {} ({:?})",
            config.width, config.height, adapter.get_info().name, format
        );
        (Gfx { surface, device, queue, config }, format)
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.config.width = w;
            self.config.height = h;
            self.surface.configure(&self.device, &self.config);
        }
    }
}

struct App {
    zone: String,
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    view: Option<WorldView>,
    center: [f32; 3],
    span: f32,
    ground_y: f32,
    start: Instant,
    frames: u64,
    last_log: Instant,
}

impl App {
    fn new(zone: String) -> App {
        let now = Instant::now();
        App {
            zone,
            window: None,
            gfx: None,
            view: None,
            center: [0.0; 3],
            span: 100.0,
            ground_y: 0.0,
            start: now,
            frames: 0,
            last_log: now,
        }
    }
}

/// Load a zone's scenery into SceneInstances (deduped models/textures kept alive
/// in the returned vecs, which the instances borrow). Returns (instances-source,
/// center, span, ground_y) — the caller builds SceneInstances from the source.
struct SceneData {
    models: Vec<Rc<B3dModel>>,
    textures: Vec<Vec<Option<Image>>>,
    // (model idx, pos, rot radians, scale)
    place: Vec<(usize, [f32; 3], [f32; 3], [f32; 3])>,
    center: [f32; 3],
    span: f32,
    ground_y: f32,
}

fn load_zone(data_root: &str, zone: &str) -> Option<SceneData> {
    let mut store = rcce_client::assets::AssetStore::load(data_root)
        .map_err(|e| eprintln!("[client-window] assets: {e}"))
        .ok()?;
    let path = std::path::Path::new(data_root)
        .join("Areas")
        .join(format!("{zone}.dat"));
    let bytes = std::fs::read(&path)
        .map_err(|e| eprintln!("[client-window] {}: {e}", path.display()))
        .ok()?;
    let scenery = AreaScenery::parse(&bytes).ok()?;

    let mut models = Vec::new();
    let mut textures = Vec::new();
    let mut dedup = std::collections::HashMap::new();
    let mut place = Vec::new();
    let (mut min, mut max) = ([f32::MAX; 3], [f32::MIN; 3]);
    for s in &scenery.sceneries {
        let key = format!("{}:{}", s.mesh_id, s.texture_id);
        let idx = match dedup.get(&key) {
            Some(&i) => i,
            None => {
                let Some(m) = store.mesh_model(s.mesh_id) else { continue };
                let tex = store.scenery_textures(s.mesh_id, s.texture_id);
                let i = models.len();
                models.push(m);
                textures.push(tex);
                dedup.insert(key, i);
                i
            }
        };
        let rot = [s.rot[0].to_radians(), s.rot[1].to_radians(), s.rot[2].to_radians()];
        place.push((idx, s.pos, rot, s.scale));
        for k in 0..3 {
            min[k] = min[k].min(s.pos[k]);
            max[k] = max[k].max(s.pos[k]);
        }
    }
    if place.is_empty() {
        eprintln!("[client-window] zone '{zone}' has no resolved scenery");
        return None;
    }
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
    let span = ((max[0] - min[0]).powi(2) + (max[2] - min[2]).powi(2)).sqrt().max(50.0);
    println!("[client-window] zone '{zone}': {} objects, {} meshes, span {span:.0}", place.len(), models.len());
    Some(SceneData { models, textures, place, center, span, ground_y: min[1] })
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title(format!("RCCE2 — {}", self.zone))
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let (gfx, format) = Gfx::new(window.clone());

        let mut view = WorldView::new(&gfx.device, format, gfx.config.width, gfx.config.height);
        let data_root = std::env::var("RCCE_DATA")
            .unwrap_or_else(|_| r"C:\Users\dyanr\Desktop\rcce2\data".to_string());
        if let Some(sd) = load_zone(&data_root, &self.zone) {
            // Borrow models/textures to build instances, upload, then drop.
            let instances: Vec<rcce_render::SceneInstance> = sd
                .place
                .iter()
                .map(|&(idx, pos, rot, scale)| rcce_render::SceneInstance {
                    model: &sd.models[idx],
                    textures: &sd.textures[idx],
                    translation: pos,
                    rot,
                    scale,
                    color: [1.0, 1.0, 1.0],
                })
                .collect();
            view.set_scene(&gfx.device, &gfx.queue, &instances, sd.ground_y);
            self.center = sd.center;
            self.span = sd.span;
            self.ground_y = sd.ground_y;
            println!("[client-window] uploaded {} drawables", view.drawable_count());
        }

        self.gfx = Some(gfx);
        self.view = Some(view);
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.resize(size.width, size.height);
                }
                if let Some(view) = self.view.as_mut() {
                    if let Some(gfx) = self.gfx.as_ref() {
                        view.resize(&gfx.device, size.width, size.height);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.render();
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }
            _ => {}
        }
    }
}

impl App {
    fn render(&mut self) {
        let (Some(gfx), Some(view)) = (self.gfx.as_mut(), self.view.as_ref()) else { return };
        let frame = match gfx.surface.get_current_texture() {
            Ok(f) => f,
            Err(_) => {
                gfx.surface.configure(&gfx.device, &gfx.config);
                match gfx.surface.get_current_texture() {
                    Ok(f) => f,
                    Err(_) => return,
                }
            }
        };
        let tview = frame.texture.create_view(&Default::default());

        // Auto-orbit camera around the zone centre.
        let t = self.start.elapsed().as_secs_f32();
        let ang = t * 0.25;
        let r = self.span * 0.75;
        let h = self.span * 0.55;
        let eye = [
            self.center[0] + r * ang.cos(),
            self.ground_y + h,
            self.center[2] + r * ang.sin(),
        ];
        let target = [self.center[0], self.ground_y + self.span * 0.05, self.center[2]];
        let aspect = gfx.config.width as f32 / gfx.config.height.max(1) as f32;
        let vp = rcce_render::view_proj(eye, target, aspect);

        view.render(
            &gfx.device,
            &gfx.queue,
            &tview,
            vp,
            wgpu::Color { r: 0.45, g: 0.62, b: 0.82, a: 1.0 },
        );
        frame.present();

        self.frames += 1;
        if self.last_log.elapsed().as_secs_f32() >= 2.0 {
            let fps = self.frames as f32 / self.start.elapsed().as_secs_f32().max(0.001);
            println!("[client-window] frame {} (~{fps:.0} fps avg)", self.frames);
            self.last_log = Instant::now();
        }
    }
}

fn main() {
    let zone = std::env::args().nth(1).unwrap_or_else(|| "Plains".to_string());
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(zone);
    event_loop.run_app(&mut app).expect("run app");
}
