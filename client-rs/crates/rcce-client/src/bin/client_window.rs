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
use winit::event::{DeviceEvent, DeviceId, ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

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
    overlay: Option<rcce_render::Overlay>,
    store: Option<AssetStore>,
    net: Option<Net>,
    center: [f32; 3],
    span: f32,
    ground_y: f32,
    fog_color: [f32; 3],
    fog_near: f32,
    fog_far: f32,
    /// Zone lighting: ambient floor + toward-light unit vector (from the area
    /// header's DefaultLightPitch/Yaw).
    ambient: [f32; 3],
    light_dir: [f32; 3],
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
    /// Vertical look angle (radians, + tilts the camera up over the player).
    cam_pitch: f32,
    /// Mouse-look active: cursor grabbed/hidden, mouse motion drives yaw/pitch.
    /// Toggled with Tab; off by default so the headless/autowalk path and the
    /// arrow/Q-E discrete turn keep working unchanged.
    mouse_look: bool,
    last_move: Instant,
    was_moving: bool,
    /// `Some` while the chat line is open (the typed buffer); movement keys are
    /// suppressed. Enter sends + closes, Esc cancels.
    chat_input: Option<String>,
    /// Runtime id of the last-attacked actor (for the target highlight).
    target: Option<u16>,
    /// Floating combat-damage numbers (drained from world.combat_events).
    floaters: rcce_client::floaters::Floaters,
    /// Audio output (zone music). `None` when there's no audio device.
    audio: Option<rcce_client::audio::Audio>,
    /// Character sheet (gold/level/inventory/spells) from login's P_FetchCharacter.
    sheet: Option<rcce_client::fetch::CharacterSheet>,
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
            overlay: None,
            store: None,
            net: None,
            center: [0.0; 3],
            span: 100.0,
            ground_y: 0.0,
            fog_color: [0.45, 0.62, 0.82],
            fog_near: 1000.0,
            fog_far: 9000.0,
            ambient: [0.5, 0.5, 0.5],
            light_dir: [0.0, 0.5, -0.866],
            start: now,
            frames: 0,
            last_log: now,
            last_dyn_hash: u64::MAX,
            keys_wasd: [false; 4],
            run: false,
            cam_yaw: 0.0,
            cam_pitch: 0.25,
            mouse_look: false,
            last_move: now,
            was_moving: false,
            chat_input: None,
            target: None,
            floaters: rcce_client::floaters::Floaters::new(),
            audio: rcce_client::audio::Audio::new(),
            sheet: None,
        }
    }
}

/// Build animated actor instances (the local player + tracked actors) for the
/// current frame. Returns owned models/textures (the instances borrow them) and
/// placement tuples (idx, translation, rot, color, scale).
type Placement = (usize, [f32; 3], [f32; 3], [f32; 3], [f32; 3]);
/// Colour a damage number by its damage-type index (defaults to red). The
/// indices loosely follow the engine's Damage.dat ordering; exact names aren't
/// loaded yet, so this is a stable palette for visual variety.
fn damage_color(dtype: u8, alpha: f32) -> [f32; 4] {
    let [r, g, b] = match dtype {
        0 => [1.0, 0.85, 0.30], // physical — amber
        1 => [1.0, 0.45, 0.20], // fire — orange
        2 => [0.50, 0.80, 1.0], // cold — blue
        3 => [0.70, 1.0, 0.40], // nature/poison — green
        4 => [0.85, 0.50, 1.0], // magic — violet
        _ => [1.0, 0.40, 0.40], // default — red
    };
    [r, g, b, alpha]
}

fn build_actors(
    store: &mut AssetStore,
    world: &World,
    elapsed: f32,
    ground_y: f32,
) -> (Vec<Rc<B3dModel>>, Vec<Rc<Vec<Option<Image>>>>, Vec<Placement>, Vec<String>) {
    let mut models = Vec::new();
    let mut textures: Vec<Rc<Vec<Option<Image>>>> = Vec::new();
    let mut place = Vec::new();
    let mut keys: Vec<String> = Vec::new();

    let mut push = |store: &mut AssetStore,
                    models: &mut Vec<Rc<B3dModel>>,
                    textures: &mut Vec<Rc<Vec<Option<Image>>>>,
                    place: &mut Vec<Placement>,
                    keys: &mut Vec<String>,
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
        let tex = store.actor_textures_rc(tmpl, gender, face, body);
        let (min, _) = posed.bounds();
        let idx = models.len();
        models.push(posed);
        textures.push(tex);
        keys.push(format!("{tmpl}:{gender}:{face}:{body}"));
        let trans = [pos[0], ground_y - min[1] * scale, pos[2]];
        place.push((idx, trans, [0.0, yaw.to_radians(), 0.0], color, [scale, scale, scale]));
    };

    push(store, &mut models, &mut textures, &mut place, &mut keys, 0, world.me_gender, world.me_face_tex, world.me_body_tex, world.my_runtime_id, false, false, [world.me_x, world.me_y, world.me_z], world.me_yaw, [0.85, 0.95, 0.85]);
    for a in world.actors.values() {
        let dx = a.dest_x - a.x;
        let dz = a.dest_z - a.z;
        let moving = (dx * dx + dz * dz) > 1.0;
        let color = if a.is_player { [0.85, 0.9, 1.0] } else { [1.0, 1.0, 1.0] };
        push(store, &mut models, &mut textures, &mut place, &mut keys, a.template_id, a.gender, a.face_tex, a.body_tex, a.runtime_id, moving, a.is_running, [a.x, a.y, a.z], a.yaw, color);
    }
    (models, textures, place, keys)
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
            self.ambient = env.ambient;
            self.light_dir = env.light_dir;
            // Zone music (looped), if this zone sets a LoadingMusicID and the
            // track resolves through Music.dat to a file on disk.
            if let Some(audio) = self.audio.as_mut() {
                audio.set_music(env.music_id, 0.4, |id| store.music_path(id));
            }
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
                if let Some(s) = &outcome.sheet {
                    println!(
                        "[client-window] sheet: gold={} level={} {} item(s) {} spell(s)",
                        s.gold, s.level, s.inventory.len(), s.spells.len()
                    );
                }
                self.sheet = outcome.sheet;
                self.net = Some(Net { transport, world, peer: outcome.peer, updates: 0 });
            }
            Err(e) => eprintln!("[client-window] login failed ({e}); zone-only spectator view"),
        }

        self.overlay = Some(rcce_render::Overlay::new(&gfx.device, format));
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
                // Chat typing mode: capture text, Enter sends, Esc cancels.
                if self.chat_input.is_some() {
                    if pressed {
                        match event.physical_key {
                            PhysicalKey::Code(KeyCode::Enter | KeyCode::NumpadEnter) => {
                                let msg = self.chat_input.take().unwrap_or_default();
                                if !msg.is_empty() {
                                    if let Some(net) = self.net.as_mut() {
                                        net.transport.send(
                                            net.peer,
                                            rcce_net::packet_id::CHAT_MESSAGE,
                                            msg.as_bytes(),
                                            true,
                                        );
                                    }
                                }
                            }
                            PhysicalKey::Code(KeyCode::Escape) => self.chat_input = None,
                            PhysicalKey::Code(KeyCode::Backspace) => {
                                if let Some(b) = self.chat_input.as_mut() {
                                    b.pop();
                                }
                            }
                            _ => {
                                if let (Some(t), Some(b)) =
                                    (event.text.as_ref(), self.chat_input.as_mut())
                                {
                                    for c in t.chars() {
                                        if !c.is_control() && b.chars().count() < 100 {
                                            b.push(c);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    return;
                }
                if let PhysicalKey::Code(code) = event.physical_key {
                    match code {
                        // Open the chat line.
                        KeyCode::Enter | KeyCode::KeyT if pressed => {
                            self.chat_input = Some(String::new());
                            self.keys_wasd = [false; 4]; // stop moving while typing
                        }
                        // Attack the nearest living actor.
                        KeyCode::KeyF | KeyCode::Space if pressed => {
                            if let Some(net) = self.net.as_mut() {
                                let (mx, mz) = (net.world.me_x, net.world.me_z);
                                let target = net
                                    .world
                                    .actors
                                    .values()
                                    .filter(|a| a.alive)
                                    .min_by(|a, b| {
                                        let da = (a.x - mx).powi(2) + (a.z - mz).powi(2);
                                        let db = (b.x - mx).powi(2) + (b.z - mz).powi(2);
                                        da.total_cmp(&db)
                                    })
                                    .map(|a| a.runtime_id);
                                if let Some(rid) = target {
                                    self.target = Some(rid);
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::ATTACK_ACTOR,
                                        &rid.to_le_bytes(),
                                        true,
                                    );
                                }
                            }
                        }
                        KeyCode::KeyW | KeyCode::ArrowUp => self.keys_wasd[0] = pressed,
                        KeyCode::KeyA => self.keys_wasd[1] = pressed,
                        KeyCode::KeyS | KeyCode::ArrowDown => self.keys_wasd[2] = pressed,
                        KeyCode::KeyD => self.keys_wasd[3] = pressed,
                        KeyCode::ShiftLeft | KeyCode::ShiftRight => self.run = pressed,
                        // Discrete camera turn (WASD move relative to it) —
                        // still available as a fallback when mouse-look is off.
                        KeyCode::ArrowLeft | KeyCode::KeyQ if pressed => self.cam_yaw -= 0.18,
                        KeyCode::ArrowRight | KeyCode::KeyE if pressed => self.cam_yaw += 0.18,
                        // Toggle mouse-look (grab/hide the cursor).
                        KeyCode::Tab if pressed => {
                            let on = !self.mouse_look;
                            self.set_mouse_look(on);
                        }
                        KeyCode::Escape => {
                            if self.mouse_look {
                                self.set_mouse_look(false);
                            } else {
                                event_loop.exit();
                            }
                        }
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

    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if !self.mouse_look {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            const SENS: f32 = 0.0032;
            self.cam_yaw += dx as f32 * SENS;
            // Up-drag looks up; clamp so the camera can't flip past the poles.
            self.cam_pitch = (self.cam_pitch - dy as f32 * SENS).clamp(-0.35, 1.30);
        }
    }
}

impl App {
    /// Enable/disable mouse-look: grab + hide the cursor (Locked, falling back to
    /// Confined) when on; release it when off.
    fn set_mouse_look(&mut self, on: bool) {
        self.mouse_look = on;
        let Some(w) = self.window.as_ref() else { return };
        if on {
            if w.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                let _ = w.set_cursor_grab(CursorGrabMode::Confined);
            }
            w.set_cursor_visible(false);
        } else {
            let _ = w.set_cursor_grab(CursorGrabMode::None);
            w.set_cursor_visible(true);
        }
    }

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
            // Spawn floating damage numbers for any new combat hits, expire old.
            self.floaters.ingest(&net.world.combat_events, elapsed);
            self.floaters.tick(elapsed);
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
                let (models, textures, place, keys) =
                    build_actors(store, &net.world, elapsed, self.ground_y);
                let instances: Vec<SceneInstance> = place
                    .iter()
                    .map(|&(idx, t, r, color, s)| SceneInstance {
                        model: &models[idx],
                        textures: &textures[idx][..],
                        translation: t,
                        rot: r,
                        scale: s,
                        color,
                    })
                    .collect();
                view.set_dynamic(&gfx.device, &gfx.queue, &instances, &keys);
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
            // Orbit behind the player: yaw places the camera on the -forward
            // side, pitch raises it. `dist` is the boom length.
            let dist = 13.0;
            let (sp, cp) = self.cam_pitch.sin_cos();
            let look = [cam_target[0], cam_target[1] + 3.5, cam_target[2]];
            let eye = [
                look[0] + sy * dist * cp,
                look[1] + dist * sp,
                look[2] + cy * dist * cp,
            ];
            (eye, look)
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
            self.ambient,
            self.light_dir,
            wgpu::Color {
                r: self.fog_color[0] as f64,
                g: self.fog_color[1] as f64,
                b: self.fog_color[2] as f64,
                a: 1.0,
            },
        );

        // 2D overlay: nameplates + health bars over actors, and a player HUD.
        let target_rid = self.target;
        if let Some(overlay) = self.overlay.as_mut() {
            let (sw, sh) = (gfx.config.width as f32, gfx.config.height as f32);
            let white = [1.0, 1.0, 1.0, 1.0];
            if let Some(net) = self.net.as_ref() {
                for a in net.world.actors.values() {
                    if !a.alive {
                        continue;
                    }
                    if let Some((px, py)) = rcce_render::project(&vp, [a.x, a.y + 5.5, a.z], sw, sh) {
                        let frac = if a.health_max > 0 {
                            a.health as f32 / a.health_max as f32
                        } else {
                            1.0
                        };
                        let is_target = target_rid == Some(a.runtime_id);
                        let col = if is_target {
                            [1.0, 0.85, 0.2, 1.0]
                        } else if a.is_player {
                            [0.4, 0.7, 1.0, 1.0]
                        } else {
                            [0.9, 0.3, 0.3, 1.0]
                        };
                        overlay.bar(px - 24.0, py - 14.0, 48.0, 5.0, frac, col);
                        if is_target {
                            // Target brackets around the bar.
                            overlay.rect(px - 28.0, py - 15.0, 2.0, 7.0, col);
                            overlay.rect(px + 26.0, py - 15.0, 2.0, 7.0, col);
                        }
                        if !a.name.is_empty() {
                            let tw = rcce_render::font::text_width(&a.name, 1.0);
                            let nc = if is_target { col } else { white };
                            overlay.text_shadow(px - tw * 0.5, py - 26.0, 1.0, &a.name, nc);
                        }
                    }
                }

                // Floating damage numbers, anchored over their target actor
                // (or me), rising and fading over their lifetime.
                for fl in self.floaters.iter() {
                    let pos = if fl.rid == net.world.my_runtime_id {
                        Some([net.world.me_x, net.world.me_y, net.world.me_z])
                    } else {
                        net.world.actors.get(&fl.rid).map(|a| [a.x, a.y, a.z])
                    };
                    let Some(p) = pos else { continue };
                    if let Some((px, py)) = rcce_render::project(&vp, [p[0], p[1] + 6.5, p[2]], sw, sh) {
                        let s = fl.damage.to_string();
                        let col = damage_color(fl.damage_type, fl.alpha(elapsed));
                        let tw = rcce_render::font::text_width(&s, 1.5);
                        overlay.text_shadow(px - tw * 0.5, py - 30.0 - fl.rise(elapsed), 1.5, &s, col);
                    }
                }

                // Player HUD: zone, HP bar + numbers, fps; chat log above it.
                let w = &net.world;
                let hpf = if w.me_health_max > 0 {
                    w.me_health as f32 / w.me_health_max as f32
                } else {
                    1.0
                };
                let fps = self.frames as f32 / elapsed.max(0.001);
                overlay.rect(10.0, sh - 56.0, 270.0, 48.0, [0.0, 0.0, 0.0, 0.45]);
                overlay.text_shadow(18.0, sh - 50.0, 2.0, &w.zone.name, white);
                overlay.bar(18.0, sh - 28.0, 200.0, 12.0, hpf, [0.2, 0.8, 0.25, 1.0]);
                let hp = format!("{}/{}", w.me_health.max(0), w.me_health_max.max(0));
                overlay.text(224.0, sh - 28.0, 1.0, &hp, white);
                overlay.text(sw - 84.0, 10.0, 1.0, &format!("{fps:.0} fps"), [0.8, 1.0, 0.8, 1.0]);
                // Character sheet readout (level + gold) from P_FetchCharacter.
                if let Some(sheet) = &self.sheet {
                    let line = format!("Lv {}   {}g", sheet.level, sheet.gold);
                    let tw = rcce_render::font::text_width(&line, 1.0);
                    overlay.text_shadow(sw - tw - 12.0, 24.0, 1.0, &line, [1.0, 0.88, 0.4, 1.0]);
                }

                // Chat log: the last few lines, just above the HUD.
                let chat_base = if self.chat_input.is_some() { 84.0 } else { 70.0 };
                for (i, line) in w.chat.iter().rev().take(5).enumerate() {
                    let y = sh - chat_base - i as f32 * 12.0;
                    let s: String = line.chars().take(60).collect();
                    overlay.text_shadow(14.0, y, 1.0, &s, [0.9, 0.9, 0.7, 1.0]);
                }
            }
            // Chat input line (with a blinking-ish caret).
            if let Some(buf) = self.chat_input.as_ref() {
                overlay.rect(10.0, sh - 80.0, sw - 20.0, 16.0, [0.0, 0.0, 0.0, 0.55]);
                let caret = if (elapsed * 2.0) as i64 % 2 == 0 { "_" } else { " " };
                overlay.text_shadow(14.0, sh - 78.0, 1.0, &format!("> {buf}{caret}"), [1.0, 1.0, 1.0, 1.0]);
            }
            overlay.render(&gfx.device, &gfx.queue, &tview, sw, sh);
        }

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
