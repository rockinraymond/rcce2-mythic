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
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use rcce_client::net::movement_packet;

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
    fog_color: [f32; 3],
    fog_near: f32,
    fog_far: f32,
    start: Instant,
    frames: u64,
    last_log: Instant,
    /// Hash of the last dynamic-actor build (anim tick + actor states); the
    /// expensive re-skin/re-upload is skipped while it's unchanged.
    last_dyn_hash: u64,
    /// Movement input: W/A/S/D held + Shift (run). Camera yaw (radians) rotated
    /// by Left/Right arrows; WASD move relative to it. `last_move` throttles the
    /// P_StandardUpdate send; `was_moving` lets us send one stop packet on idle.
    keys_wasd: [bool; 4],
    run: bool,
    cam_yaw: f32,
    last_move: Instant,
    was_moving: bool,
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
            fog_color: [0.45, 0.62, 0.82],
            fog_near: 1000.0,
            fog_far: 9000.0,
            start: now,
            frames: 0,
            last_log: now,
            last_dyn_hash: u64::MAX,
            keys_wasd: [false; 4],
            run: false,
            cam_yaw: 0.0,
            last_move: now,
            was_moving: false,
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

/// Cheap fingerprint of everything that affects the actor drawables: a ~12 Hz
/// animation tick plus each actor's quantised position/yaw/run state. When it's
/// unchanged the dynamic geometry is reused (no re-skin/re-upload).
fn dyn_hash(world: &World, elapsed: f32) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ((elapsed * 12.0) as u64).hash(&mut h);
    world.my_runtime_id.hash(&mut h);
    ((world.me_x * 2.0) as i32).hash(&mut h);
    ((world.me_z * 2.0) as i32).hash(&mut h);
    (world.me_yaw as i32).hash(&mut h);
    let mut rids: Vec<u16> = world.actors.keys().copied().collect();
    rids.sort_unstable();
    for rid in rids {
        let a = &world.actors[&rid];
        rid.hash(&mut h);
        ((a.x * 2.0) as i32).hash(&mut h);
        ((a.z * 2.0) as i32).hash(&mut h);
        (a.yaw as i32).hash(&mut h);
        a.is_running.hash(&mut h);
    }
    h.finish()
}

fn load_zone_static(store: &mut AssetStore, view: &mut WorldView, gfx: &Gfx, data_root: &str, zone: &str) -> Option<([f32; 3], f32, f32, rcce_data::AreaEnv)> {
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
    Some((center, span, min[1], scenery.env.clone()))
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
        if let Some((center, span, gy, env)) = load_zone_static(&mut store, &mut view, &gfx, &data_root, &self.zone) {
            self.center = center;
            self.span = span;
            self.ground_y = gy;
            self.fog_color = env.fog_color;
            self.fog_near = env.fog_near;
            self.fog_far = env.fog_far;
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
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        KeyCode::KeyW | KeyCode::ArrowUp => self.keys_wasd[0] = pressed,
                        KeyCode::KeyA => self.keys_wasd[1] = pressed,
                        KeyCode::KeyS | KeyCode::ArrowDown => self.keys_wasd[2] = pressed,
                        KeyCode::KeyD => self.keys_wasd[3] = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.run = pressed,
                        // Discrete camera turn (WASD move relative to it).
                        KeyCode::ArrowLeft | KeyCode::KeyQ if pressed => self.cam_yaw -= 0.18,
                        KeyCode::ArrowRight | KeyCode::KeyE if pressed => self.cam_yaw += 0.18,
                        KeyCode::Escape => event_loop.exit(),
                        _ => {}
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
        let (Some(gfx), Some(view), Some(store)) =
            (self.gfx.as_mut(), self.view.as_mut(), self.store.as_mut())
        else {
            return;
        };
        let elapsed = self.start.elapsed().as_secs_f32();

        // Movement intent from WASD relative to the camera yaw (computed before
        // borrowing `net`). Camera-space basis: forward = into-screen, right.
        let (sy, cy) = self.cam_yaw.sin_cos();
        let fwd = [-sy, -cy];
        let right = [cy, -sy];
        let mut dir = [0.0f32, 0.0];
        // RCCE_AUTOWALK forces forward movement (for headless verification of
        // the movement-send path without a keyboard).
        let auto = std::env::var_os("RCCE_AUTOWALK").is_some();
        if self.keys_wasd[0] || auto { dir[0] += fwd[0]; dir[1] += fwd[1]; }
        if self.keys_wasd[2] { dir[0] -= fwd[0]; dir[1] -= fwd[1]; }
        if self.keys_wasd[3] { dir[0] += right[0]; dir[1] += right[1]; }
        if self.keys_wasd[1] { dir[0] -= right[0]; dir[1] -= right[1]; }
        let mag = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let moving = mag > 0.01;
        let want_send = self.last_move.elapsed().as_millis() >= 110;
        let run = self.run;

        // Pump the network, send movement, and rebuild animated actors.
        let mut cam_target = self.center;
        let mut following = false;
        let mut did_send = false;
        if let Some(net) = self.net.as_mut() {
            for m in net.transport.poll() {
                net.updates += 1;
                net.world.apply(&m);
            }
            // Send a P_StandardUpdate toward the input direction (unreliable,
            // like ClientNet.bb): the server walks the actor toward Dest and
            // echoes its authoritative position, which on_standard_update
            // applies back to me_x/z. A single stop packet on key-release.
            let (mx, my, mz) = (net.world.me_x, net.world.me_y, net.world.me_z);
            if moving && want_send {
                let (nx, nz) = (dir[0] / mag, dir[1] / mag);
                let p = movement_packet(mx + nx * 16.0, mz + nz * 16.0, my, mx, mz, run, false);
                net.transport.send(net.peer, rcce_net::packet_id::STANDARD_UPDATE, &p, false);
                did_send = true;
            } else if !moving && self.was_moving {
                let p = movement_packet(mx, mz, my, mx, mz, false, false);
                net.transport.send(net.peer, rcce_net::packet_id::STANDARD_UPDATE, &p, false);
            }

            let hash = dyn_hash(&net.world, elapsed);
            if hash != self.last_dyn_hash {
                let (models, textures, place) =
                    build_actors(store, &net.world, elapsed, self.ground_y);
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
                self.last_dyn_hash = hash;
            }
            cam_target = [net.world.me_x, net.world.me_y, net.world.me_z];
            following = true;
        }
        if did_send {
            self.last_move = Instant::now();
        }
        self.was_moving = moving;

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

        // Camera: third-person follow behind the player along cam_yaw (live),
        // or a slow orbit of the zone centre (spectator). `behind = -forward`.
        let (eye, target) = if following {
            let behind = [sy, cy];
            let (dist, height) = (12.0, 6.5);
            let eye = [
                cam_target[0] + behind[0] * dist,
                cam_target[1] + height,
                cam_target[2] + behind[1] * dist,
            ];
            (eye, [cam_target[0], cam_target[1] + 3.5, cam_target[2]])
        } else {
            let ang = elapsed * 0.3;
            let r = self.span * 0.75;
            let eye = [self.center[0] + r * ang.cos(), self.ground_y + self.span * 0.55, self.center[2] + r * ang.sin()];
            (eye, [self.center[0], self.ground_y + self.span * 0.05, self.center[2]])
        };
        let aspect = gfx.config.width as f32 / gfx.config.height.max(1) as f32;
        let vp = rcce_render::view_proj(eye, target, aspect);
        view.render(
            &gfx.device,
            &gfx.queue,
            &tview,
            vp,
            eye,
            self.fog_color,
            self.fog_near,
            self.fog_far,
            wgpu::Color {
                r: self.fog_color[0] as f64,
                g: self.fog_color[1] as f64,
                b: self.fog_color[2] as f64,
                a: 1.0,
            },
        );
        frame.present();

        self.frames += 1;
        if self.last_log.elapsed().as_secs_f32() >= 2.0 {
            let fps = self.frames as f32 / elapsed.max(0.001);
            let (actors, ups, pos) = self
                .net
                .as_ref()
                .map(|n| (n.world.actors.len(), n.updates, (n.world.me_x, n.world.me_z)))
                .unwrap_or((0, 0, (0.0, 0.0)));
            println!(
                "[client-window] frame {} (~{fps:.0} fps), {actors} actor(s), {ups} packets, me=({:.1},{:.1})",
                self.frames, pos.0, pos.1
            );
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
