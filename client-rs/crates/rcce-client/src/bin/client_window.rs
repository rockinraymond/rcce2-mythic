//! Real-time client window: logs into a running server and renders the live
//! world — textured terrain/scenery (static) plus animated actors (dynamic),
//! at the display refresh rate, with a camera that orbits the local player.
//! Falls back to a zone-only spectator view if login fails.
//!
//!   cargo run -p rcce-client --bin client-window --release -- [host] [port] [zone]
//!
//! NOTE: needs a display + (for the live view) a running server. In a headless
//! agent environment it still opens on the host desktop; stdout logs init,
//! login, actor count, and fps so it can be sanity-checked without seeing
//! pixels.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use enet_sys::EnetTransport;
use rcce_client::assets::{clip_frame, AssetStore};
use rcce_client::login::{login, Credentials};
use rcce_client::world::World;
use rcce_data::{AreaScenery, B3dModel, Image};
use rcce_net::Transport;
use rcce_render::{SceneInstance, WorldView};

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

struct Net {
    transport: EnetTransport,
    world: World,
    peer: i32,
    updates: u64,
}

struct App {
    host: String,
    port: u16,
    zone: String,
    window: Option<Arc<Window>>,
    gfx: Option<Gfx>,
    view: Option<WorldView>,
    store: Option<AssetStore>,
    net: Option<Net>,
    center: [f32; 3],
    span: f32,
    ground_y: f32,
    start: Instant,
    frames: u64,
    last_log: Instant,
}

impl App {
    fn new(host: String, port: u16, zone: String) -> App {
        let now = Instant::now();
        App {
            host,
            port,
            zone,
            window: None,
            gfx: None,
            view: None,
            store: None,
            net: None,
            center: [0.0; 3],
            span: 100.0,
            ground_y: 0.0,
            start: now,
            frames: 0,
            last_log: now,
        }
    }
}

/// Build animated actor instances (the local player + tracked actors) for the
/// current frame. Returns owned models/textures (the instances borrow them) and
/// placement tuples (idx, translation, rot, color, scale).
type Placement = (usize, [f32; 3], [f32; 3], [f32; 3], [f32; 3]);
fn build_actors(
    store: &mut AssetStore,
    world: &World,
    elapsed: f32,
    ground_y: f32,
) -> (Vec<Rc<B3dModel>>, Vec<Vec<Option<Image>>>, Vec<Placement>) {
    let mut models = Vec::new();
    let mut textures = Vec::new();
    let mut place = Vec::new();

    let mut push = |store: &mut AssetStore,
                    models: &mut Vec<Rc<B3dModel>>,
                    textures: &mut Vec<Vec<Option<Image>>>,
                    place: &mut Vec<Placement>,
                    tmpl: u16,
                    gender: u8,
                    face: u8,
                    body: u8,
                    rid: u16,
                    moving: bool,
                    running: bool,
                    pos: [f32; 3],
                    yaw: f32,
                    color: [f32; 3]| {
        let Some(src) = store.actor_model(tmpl, gender) else { return };
        let names: &[&str] = if running {
            &["Run"]
        } else if moving {
            &["Walk"]
        } else {
            &["Idle", "Sit idle"]
        };
        let fps = src.anim.map(|a| a.fps).unwrap_or(15.0);
        let frame = store
            .actor_clip(tmpl, gender, names)
            .map(|c| clip_frame(c, fps, elapsed + rid as f32 * 0.13));
        let posed = Rc::new(B3dModel {
            meshes: src.posed_meshes(frame),
            textures: src.textures.clone(),
            brushes: src.brushes.clone(),
            bones: src.bones.clone(),
            anim: src.anim,
        });
        let scale = store.actor_render_scale(tmpl, gender).unwrap_or(0.05);
        let tex = store.actor_textures(tmpl, gender, face, body);
        let (min, _) = posed.bounds();
        let idx = models.len();
        models.push(posed);
        textures.push(tex);
        let trans = [pos[0], ground_y - min[1] * scale, pos[2]];
        place.push((idx, trans, [0.0, yaw.to_radians(), 0.0], color, [scale, scale, scale]));
    };

    push(store, &mut models, &mut textures, &mut place, 0, world.me_gender, world.me_face_tex, world.me_body_tex, world.my_runtime_id, false, false, [world.me_x, world.me_y, world.me_z], world.me_yaw, [0.85, 0.95, 0.85]);
    for a in world.actors.values() {
        let dx = a.dest_x - a.x;
        let dz = a.dest_z - a.z;
        let moving = (dx * dx + dz * dz) > 1.0;
        let color = if a.is_player { [0.85, 0.9, 1.0] } else { [1.0, 1.0, 1.0] };
        push(store, &mut models, &mut textures, &mut place, a.template_id, a.gender, a.face_tex, a.body_tex, a.runtime_id, moving, a.is_running, [a.x, a.y, a.z], a.yaw, color);
    }
    (models, textures, place)
}

fn load_zone_static(store: &mut AssetStore, view: &mut WorldView, gfx: &Gfx, data_root: &str, zone: &str) -> Option<([f32; 3], f32, f32)> {
    let path = std::path::Path::new(data_root).join("Areas").join(format!("{zone}.dat"));
    let bytes = std::fs::read(&path).map_err(|e| eprintln!("[client-window] {}: {e}", path.display())).ok()?;
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
        return None;
    }
    let instances: Vec<SceneInstance> = place
        .iter()
        .map(|&(idx, pos, rot, scale)| SceneInstance {
            model: &models[idx],
            textures: &textures[idx],
            translation: pos,
            rot,
            scale,
            color: [1.0, 1.0, 1.0],
        })
        .collect();
    view.set_scene(&gfx.device, &gfx.queue, &instances, min[1]);
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
    let span = ((max[0] - min[0]).powi(2) + (max[2] - min[2]).powi(2)).sqrt().max(50.0);
    println!("[client-window] zone '{zone}': {} objects, {} meshes, span {span:.0}", place.len(), models.len());
    Some((center, span, min[1]))
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("RCCE2 — Rust client")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 800));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let (gfx, format) = Gfx::new(window.clone());
        let mut view = WorldView::new(&gfx.device, format, gfx.config.width, gfx.config.height);

        let data_root = std::env::var("RCCE_DATA")
            .unwrap_or_else(|_| r"C:\Users\dyanr\Desktop\rcce2\data".to_string());
        let mut store = match AssetStore::load(&data_root) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[client-window] assets: {e}");
                event_loop.exit();
                return;
            }
        };

        // Static scenery (always — also the fallback view).
        if let Some((center, span, gy)) = load_zone_static(&mut store, &mut view, &gfx, &data_root, &self.zone) {
            self.center = center;
            self.span = span;
            self.ground_y = gy;
        }

        // Try to log into the live server.
        println!("[client-window] logging in to {}:{} ...", self.host, self.port);
        let mut transport = EnetTransport::new();
        // Use a pre-existing account (with a character) so login is fast — a
        // brand-new account would enter the slow CreateCharacter loop and block
        // window creation. Overridable via RCCE_USER. (A non-clean prior exit
        // leaves the account "online" → 'L'; restart the server to clear it.)
        let user = std::env::var("RCCE_USER").unwrap_or_else(|_| "rustbot".to_string());
        let creds = Credentials {
            username: user,
            password: "rustpass".to_string(),
            email: "rust@bot.com".to_string(),
        };
        match login(&mut transport, &self.host, self.port, &creds) {
            Ok(outcome) => {
                let mut world = World {
                    my_runtime_id: outcome.runtime_id,
                    template_genders: store.template_genders(),
                    ..Default::default()
                };
                for m in &outcome.world_packets {
                    world.apply(m);
                }
                println!("[client-window] ✓ in world '{}', RuntimeID={}", world.zone.name, outcome.runtime_id);
                self.net = Some(Net { transport, world, peer: outcome.peer, updates: 0 });
            }
            Err(e) => eprintln!("[client-window] login failed ({e}); zone-only spectator view"),
        }

        self.store = Some(store);
        self.gfx = Some(gfx);
        self.view = Some(view);
        window.request_redraw();
        self.window = Some(window);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                if let Some(net) = self.net.as_mut() {
                    net.transport.disconnect(net.peer);
                }
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                if let Some(gfx) = self.gfx.as_mut() {
                    gfx.resize(size.width, size.height);
                }
                if let (Some(view), Some(gfx)) = (self.view.as_mut(), self.gfx.as_ref()) {
                    view.resize(&gfx.device, size.width, size.height);
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
        let (Some(gfx), Some(view), Some(store)) =
            (self.gfx.as_mut(), self.view.as_mut(), self.store.as_mut())
        else {
            return;
        };
        let elapsed = self.start.elapsed().as_secs_f32();

        // Pump the network and rebuild animated actors.
        let mut cam_target = self.center;
        let mut following = false;
        if let Some(net) = self.net.as_mut() {
            for m in net.transport.poll() {
                net.updates += 1;
                net.world.apply(&m);
            }
            let (models, textures, place) = build_actors(store, &net.world, elapsed, self.ground_y);
            let instances: Vec<SceneInstance> = place
                .iter()
                .map(|&(idx, t, r, color, s)| SceneInstance {
                    model: &models[idx],
                    textures: &textures[idx],
                    translation: t,
                    rot: r,
                    scale: s,
                    color,
                })
                .collect();
            view.set_dynamic(&gfx.device, &gfx.queue, &instances);
            cam_target = [net.world.me_x, net.world.me_y, net.world.me_z];
            following = true;
        }

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

        // Camera: orbit the player (live) or the zone centre (spectator).
        let ang = elapsed * 0.3;
        let (eye, target) = if following {
            let r = 9.0;
            let eye = [cam_target[0] + r * ang.cos(), cam_target[1] + 6.0, cam_target[2] + r * ang.sin()];
            (eye, [cam_target[0], cam_target[1] + 3.0, cam_target[2]])
        } else {
            let r = self.span * 0.75;
            let eye = [self.center[0] + r * ang.cos(), self.ground_y + self.span * 0.55, self.center[2] + r * ang.sin()];
            (eye, [self.center[0], self.ground_y + self.span * 0.05, self.center[2]])
        };
        let aspect = gfx.config.width as f32 / gfx.config.height.max(1) as f32;
        let vp = rcce_render::view_proj(eye, target, aspect);
        view.render(&gfx.device, &gfx.queue, &tview, vp, wgpu::Color { r: 0.45, g: 0.62, b: 0.82, a: 1.0 });
        frame.present();

        self.frames += 1;
        if self.last_log.elapsed().as_secs_f32() >= 2.0 {
            let fps = self.frames as f32 / elapsed.max(0.001);
            let (actors, ups) = self
                .net
                .as_ref()
                .map(|n| (n.world.actors.len(), n.updates))
                .unwrap_or((0, 0));
            println!("[client-window] frame {} (~{fps:.0} fps), {actors} actor(s), {ups} packets", self.frames);
            self.last_log = Instant::now();
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25000);
    let zone = args.next().unwrap_or_else(|| "Plains".to_string());
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new(host, port, zone);
    event_loop.run_app(&mut app).expect("run app");
}
