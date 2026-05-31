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
use winit::event::{DeviceEvent, DeviceId, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

use rcce_client::net::movement_packet;

use enet_sys::EnetTransport;
use rcce_client::assets::{attachment_placement, clip_frame, AssetStore};
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
    /// Benchmark mode (RCCE_BENCH=N): after a warmup, time N frames, print the
    /// average fps, and exit — so GPU/skinning perf is measurable. `bench_t0` /
    /// `bench_f0` mark the measurement window start.
    bench_target: Option<u64>,
    bench_t0: Option<Instant>,
    bench_f0: u64,
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
    /// Inventory/spellbook panel visible (toggled with I).
    show_inventory: bool,
    /// Footstep cadence + the resolved footstep .ogg files.
    footsteps: rcce_client::audio::FootstepTimer,
    footstep_paths: Vec<std::path::PathBuf>,
    /// Rain/snow particles + the previous frame's elapsed time (for dt).
    weather: rcce_client::weather::WeatherSystem,
    prev_elapsed: f32,
    /// Per-spell-id cooldown end time (elapsed seconds) for the action bar.
    spell_cooldowns: std::collections::HashMap<u16, f32>,
    /// Last cursor position in physical pixels (for HUD click hit-testing while
    /// mouse-look is off). Updated on CursorMoved.
    cursor: (f32, f32),
    /// Last frame's view-projection matrix (row-major), so a world click can
    /// project actors to screen and pick the nearest to the cursor.
    vp: [f32; 16],
    /// Time of the last world-pick click and the actor it hit, for double-click
    /// detection (a double-click or Shift+click interacts with the target).
    last_click: Instant,
    last_click_rid: Option<u16>,
    /// The inventory slot the cursor was last over (panel open) with an item, so
    /// the Drop / Eat buttons act on it even after the cursor moves onto them.
    last_inv_slot: Option<u8>,
    /// Storm thunder scheduling: next play time (elapsed secs) + a rotating index
    /// over Thunder1-3.ogg.
    next_thunder: f32,
    thunder_idx: usize,
    /// Cached cloud textures (regular + storm) for the current zone, so the
    /// cloud layer swaps to darker storm clouds when it's storming without a
    /// per-frame disk reload. `cloud_is_storm` tracks which is uploaded.
    cloud_regular_img: Option<rcce_data::texture::Image>,
    cloud_storm_img: Option<rcce_data::texture::Image>,
    cloud_is_storm: bool,
    /// Project data root + the zone whose geometry/sky is currently loaded, so a
    /// live area change (player warp) reloads the new zone's scenery + sky.
    data_root: String,
    loaded_zone: String,
    /// GPU linear-blend skinning for actor bodies (RCCE_GPUSKIN). Off by default
    /// (the CPU posed-meshes path); attachments stay CPU either way.
    gpu_skin: bool,
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
            bench_target: std::env::var("RCCE_BENCH").ok().and_then(|s| s.parse::<u64>().ok()).filter(|&n| n > 0),
            bench_t0: None,
            bench_f0: 0,
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
            show_inventory: false,
            footsteps: rcce_client::audio::FootstepTimer::new(),
            footstep_paths: Vec::new(),
            weather: rcce_client::weather::WeatherSystem::new(240),
            prev_elapsed: 0.0,
            spell_cooldowns: std::collections::HashMap::new(),
            cursor: (0.0, 0.0),
            vp: [0.0; 16],
            last_click: now,
            last_click_rid: None,
            last_inv_slot: None,
            next_thunder: 0.0,
            thunder_idx: 0,
            cloud_regular_img: None,
            cloud_storm_img: None,
            cloud_is_storm: false,
            data_root: String::new(),
            loaded_zone: String::new(),
            gpu_skin: std::env::var("RCCE_GPUSKIN").is_ok(),
        }
    }
}

/// One bottom function button: a HUD action, its real Client.exe x-fraction
/// (Interface3D.bb 4:3 branch), GUI-icon key, and text-label fallback.
#[derive(Clone, Copy, PartialEq)]
enum HudAction { Chat, Map, Inventory, Spells, Character, Quests, Party, Menu }

const FUNCTION_BUTTONS: [(HudAction, f32, &str, &str); 8] = [
    (HudAction::Chat, 0.631906250, "gui:Chat", "Cht"),
    (HudAction::Map, 0.669015625, "gui:Map", "Map"),
    (HudAction::Inventory, 0.705148437, "gui:Inventory", "Inv"),
    (HudAction::Spells, 0.742257812, "gui:Abilities", "Spl"),
    (HudAction::Character, 0.780343750, "gui:Character", "Chr"),
    (HudAction::Quests, 0.816476562, "gui:Quests", "Qst"),
    (HudAction::Party, 0.853585937, "gui:Party", "Pty"),
    (HudAction::Menu, 0.893000000, "gui:Menu", "Mnu"),
];
/// Function-button baseline + size (fractions of screen) — the real GY button
/// geometry from CreateActionBarButton (4:3 branch).
const FBTN_Y: f32 = 0.9415;
const FBTN_W: f32 = 0.033203125 - 0.006;
const FBTN_H: f32 = 0.044270833 - 0.008;

/// Spell action-bar slot grid (shared by the draw + hover-tooltip paths). The
/// 12 slots are left-anchored on the FBTN_Y baseline at FBTN_W×FBTN_H each.
const SPELLBAR_X0: f32 = 0.089867187;
const SPELLBAR_PITCH: f32 = 0.036132812;

/// Index of the inventory slot (0..=45) whose rect contains `(cx, cy)`, given the
/// InventoryWindow rect `iw` and the window-relative `buttons` (Interface.dat
/// inv_buttons), for an `(sw, sh)` viewport. Pure — shared by the click hit-test
/// and the hover tooltip.
fn inventory_slot_at(cx: f32, cy: f32, iw: rcce_data::IComp, buttons: &[rcce_data::IComp], sw: f32, sh: f32) -> Option<usize> {
    for (i, b) in buttons.iter().enumerate() {
        let bx = (iw.x + b.x * iw.w) * sw;
        let by = (iw.y + b.y * iw.h) * sh;
        let bw = (b.w * iw.w * sw).max(8.0);
        let bh = (b.h * iw.h * sh).max(8.0);
        if cx >= bx && cx < bx + bw && cy >= by && cy < by + bh {
            return Some(i);
        }
    }
    None
}

/// Runtime id of the actor nearest the cursor within `radius` px, projecting
/// each `(rid, [x,y,z])` through the view-projection `vp`. Pure — shared by the
/// world-click pick and the hover tooltip.
fn actor_at(cx: f32, cy: f32, actors: &[(u16, [f32; 3])], vp: &[f32; 16], sw: f32, sh: f32, radius: f32) -> Option<u16> {
    let mut best: Option<(u16, f32)> = None;
    for &(rid, pos) in actors {
        if let Some((px, py)) = rcce_render::project(vp, pos, sw, sh) {
            let d2 = (px - cx) * (px - cx) + (py - cy) * (py - cy);
            if d2 <= radius * radius && best.map(|(_, b)| d2 < b).unwrap_or(true) {
                best = Some((rid, d2));
            }
        }
    }
    best.map(|(rid, _)| rid)
}

/// Greedy word-wrap into lines of at most `max_chars` (for tooltip bodies).
fn wrap_text(s: &str, max_chars: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if !cur.is_empty() && cur.len() + 1 + word.len() > max_chars {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The inventory action button (Drop / Eat) under `(cx, cy)`, given the
/// InventoryWindow rect and the two window-relative button rects. Pure — shared
/// by the draw and the click hit-test.
#[derive(Clone, Copy, PartialEq, Debug)]
enum InvAction {
    Drop,
    Eat,
}
fn inv_action_button_at(
    cx: f32,
    cy: f32,
    iw: rcce_data::IComp,
    drop: rcce_data::IComp,
    eat: rcce_data::IComp,
    sw: f32,
    sh: f32,
) -> Option<InvAction> {
    let hit = |c: rcce_data::IComp| {
        let (x, y, w, h) = ((iw.x + c.x * iw.w) * sw, (iw.y + c.y * iw.h) * sh, c.w * iw.w * sw, c.h * iw.h * sh);
        w > 1.0 && h > 1.0 && cx >= x && cx < x + w && cy >= y && cy < y + h
    };
    if hit(drop) {
        Some(InvAction::Drop)
    } else if hit(eat) {
        Some(InvAction::Eat)
    } else {
        None
    }
}

/// Index of the action-bar spell slot (0..=11) whose rect contains `(cx, cy)`.
/// Pure — uses the same geometry the spell-bar draw loop does.
fn spell_slot_at(cx: f32, cy: f32, sw: f32, sh: f32) -> Option<usize> {
    let by = FBTN_Y * sh;
    let (sw_, sh_) = (FBTN_W * sw, FBTN_H * sh);
    for i in 0..12usize {
        let x = (SPELLBAR_X0 + i as f32 * SPELLBAR_PITCH) * sw;
        if cx >= x && cx < x + sw_ && cy >= by && cy < by + sh_ {
            return Some(i);
        }
    }
    None
}

/// Compass strip marks: for a `heading` (radians, the way the player faces) and
/// a horizontal field-of-view `fov` (radians), return the visible compass points
/// as `(offset, label)` where offset ∈ [-0.5, 0.5] is the position across the
/// band (0 = centre = dead ahead) and an empty label is an intercardinal tick.
/// Pure so the live HUD and a unit test share one definition.
fn compass_marks(heading: f32, fov: f32) -> Vec<(f32, &'static str)> {
    use std::f32::consts::PI;
    let pts: [(f32, &str); 8] = [
        (0.0, "N"), (PI * 0.25, ""), (PI * 0.5, "E"), (PI * 0.75, ""),
        (PI, "S"), (PI * 1.25, ""), (PI * 1.5, "W"), (PI * 1.75, ""),
    ];
    let mut out = Vec::new();
    for (a, label) in pts {
        // Shortest signed angular difference into [-PI, PI].
        let mut d = a - heading;
        while d > PI {
            d -= 2.0 * PI;
        }
        while d < -PI {
            d += 2.0 * PI;
        }
        let off = d / fov;
        // Small epsilon so a mark sitting exactly on the edge (off = ±0.5, e.g. E
        // and W at fov = PI) isn't dropped by float rounding.
        if off.abs() <= 0.5 + 1e-3 {
            out.push((off, label));
        }
    }
    out
}

/// Which function button (if any) contains screen-pixel point `(cx, cy)` for a
/// `(sw, sh)` viewport. Pure geometry shared by the draw + click paths.
fn function_button_at(cx: f32, cy: f32, sw: f32, sh: f32) -> Option<HudAction> {
    let by = FBTN_Y * sh;
    let (bw, bh) = (FBTN_W * sw, FBTN_H * sh);
    for (action, fx, _, _) in FUNCTION_BUTTONS {
        let bx = fx * sw;
        if cx >= bx && cx < bx + bw && cy >= by && cy < by + bh {
            return Some(action);
        }
    }
    None
}

/// Build animated actor instances (the local player + tracked actors) for the
/// current frame. Returns owned models/textures (the instances borrow them) and
/// placement tuples (idx, translation, rot, color, scale).
type Placement = (usize, [f32; 3], [f32; 3], [f32; 3], [f32; 3]);
/// RuntimeID of the nearest living actor to (mx, mz), if any.
fn nearest_living_actor(world: &rcce_client::world::World, mx: f32, mz: f32) -> Option<u16> {
    world
        .actors
        .values()
        .filter(|a| a.alive)
        .min_by(|a, b| {
            let da = (a.x - mx).powi(2) + (a.z - mz).powi(2);
            let db = (b.x - mx).powi(2) + (b.z - mz).powi(2);
            da.total_cmp(&db)
        })
        .map(|a| a.runtime_id)
}

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

/// A GPU-skinned actor body: the source model (with bones), its textures, the
/// animation frame, the column-major instance transform, and tint. The body's
/// static mesh is uploaded once by [`WorldView::set_skinned`]; only the pose
/// uniform updates per frame.
struct SkinnedActor {
    key: String,
    model: Rc<B3dModel>,
    textures: Rc<Vec<Option<Image>>>,
    frame: Option<f32>,
    transform: [f32; 16],
    color: [f32; 3],
}

fn build_actors(
    store: &mut AssetStore,
    world: &World,
    elapsed: f32,
    ground_y: f32,
    gpu_skin: bool,
) -> (
    Vec<Rc<B3dModel>>,
    Vec<Rc<Vec<Option<Image>>>>,
    Vec<Placement>,
    Vec<String>,
    Vec<SkinnedActor>,
) {
    let mut models = Vec::new();
    let mut textures: Vec<Rc<Vec<Option<Image>>>> = Vec::new();
    let mut place = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut skinned: Vec<SkinnedActor> = Vec::new();

    let push = |store: &mut AssetStore,
                    models: &mut Vec<Rc<B3dModel>>,
                    textures: &mut Vec<Rc<Vec<Option<Image>>>>,
                    place: &mut Vec<Placement>,
                    keys: &mut Vec<String>,
                    skinned: &mut Vec<SkinnedActor>,
                    tmpl: u16,
                    gender: u8,
                    face: u8,
                    body: u8,
                    hair: u8,
                    beard: u8,
                    weapon_item: u16,
                    shield_item: u16,
                    weapon_override: Option<u16>,
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
        let scale = store.actor_render_scale(tmpl, gender).unwrap_or(0.05);
        let tex = store.actor_textures_rc(tmpl, gender, face, body);
        // Joint positions + bounds come from the bind-pose source model
        // (joint_pos returns the bind-pose joint, so attachments don't need the
        // posed body — letting the body go to the GPU skinning path).
        let (min, _) = src.bounds();
        let head = src.joint_pos("Head").unwrap_or([0.0, 0.0, 0.0]);
        let hand = src.joint_pos("R_Hand");
        let l_hand = src.joint_pos("L_Hand");
        let trans = [pos[0], ground_y - min[1] * scale, pos[2]];
        let yaw_rad = yaw.to_radians();
        let key_body = format!("{tmpl}:{gender}:{face}:{body}");
        let can_skin = !src.bones.is_empty() && src.bones.len() <= rcce_render::gpu::MAX_BONES;
        if gpu_skin && can_skin {
            // GPU-skinned body: the static mesh is posed in the vertex shader.
            let m = glam::Mat4::from_translation(glam::Vec3::from(trans))
                * glam::Mat4::from_rotation_y(yaw_rad)
                * glam::Mat4::from_scale(glam::Vec3::splat(scale));
            skinned.push(SkinnedActor {
                key: key_body,
                model: src.clone(),
                textures: tex,
                frame,
                transform: m.to_cols_array(),
                color,
            });
        } else {
            // CPU-skinned body (default / fallback): pose on the CPU.
            let posed = Rc::new(B3dModel {
                meshes: src.posed_meshes(frame),
                textures: src.textures.clone(),
                brushes: src.brushes.clone(),
                bones: src.bones.clone(),
                anim: src.anim,
            });
            let idx = models.len();
            models.push(posed);
            textures.push(tex);
            keys.push(key_body);
            place.push((idx, trans, [0.0, yaw_rad, 0.0], color, [scale, scale, scale]));
        }

        // Head attachments (hair, and beard for males) at the head joint.
        for att in store.actor_attachments(tmpl, gender, hair, beard) {
            let (t, r, s) = attachment_placement(trans, yaw_rad, scale, head, &att);
            let aidx = models.len();
            keys.push(format!("att:{tmpl}:{gender}:{}", att.mesh_id));
            models.push(att.model);
            textures.push(Rc::new(att.textures));
            place.push((aidx, t, r, color, s));
        }

        // Equipped weapon at the R_Hand joint (same mechanism). The override
        // forces a mesh for verification, since shipped items have no world
        // mesh (mmesh = 65535).
        let weapon_att = match weapon_override {
            Some(mesh) => store.gear_attachment_mesh(mesh),
            None if weapon_item != 0xFFFF => store.gear_attachment(weapon_item),
            None => None,
        };
        if let (Some(att), Some(hand)) = (weapon_att, hand) {
            let (t, r, s) = attachment_placement(trans, yaw_rad, scale, hand, &att);
            let widx = models.len();
            keys.push(format!("wpn:{}", att.mesh_id));
            models.push(att.model);
            textures.push(Rc::new(att.textures));
            place.push((widx, t, r, color, s));
        }

        // Equipped shield at the L_Hand joint (same mechanism). The override
        // also forces a mesh here for verification.
        let shield_att = match weapon_override {
            Some(mesh) => store.gear_attachment_mesh(mesh),
            None if shield_item != 0xFFFF => store.gear_attachment(shield_item),
            None => None,
        };
        if let (Some(att), Some(lh)) = (shield_att, l_hand) {
            let (t, r, s) = attachment_placement(trans, yaw_rad, scale, lh, &att);
            let sidx = models.len();
            keys.push(format!("shd:{}", att.mesh_id));
            models.push(att.model);
            textures.push(Rc::new(att.textures));
            place.push((sidx, t, r, color, s));
        }
    };

    // Debug override: force a weapon mesh on every actor (verifies the R_Hand
    // attach path; shipped items carry no world mesh).
    let weapon_override = std::env::var("RCCE_WEAPON_MESH").ok().and_then(|s| s.parse::<u16>().ok());
    // The local player's equipped weapon/shield are inventory slots 0/1.
    let me_weapon = world.me_inventory.get(&0).map(|it| it.item_id).unwrap_or(0xFFFF);
    let me_shield = world.me_inventory.get(&1).map(|it| it.item_id).unwrap_or(0xFFFF);
    push(store, &mut models, &mut textures, &mut place, &mut keys, &mut skinned, 0, world.me_gender, world.me_face_tex, world.me_body_tex, world.me_hair, world.me_beard, me_weapon, me_shield, weapon_override, world.my_runtime_id, false, false, [world.me_x, world.me_y, world.me_z], world.me_yaw, [0.85, 0.95, 0.85]);
    for a in world.actors.values() {
        let dx = a.dest_x - a.x;
        let dz = a.dest_z - a.z;
        let moving = (dx * dx + dz * dz) > 1.0;
        let color = if a.is_player { [0.85, 0.9, 1.0] } else { [1.0, 1.0, 1.0] };
        push(store, &mut models, &mut textures, &mut place, &mut keys, &mut skinned, a.template_id, a.gender, a.face_tex, a.body_tex, a.hair, a.beard, a.equipped[0], a.equipped[1], weapon_override, a.runtime_id, moving, a.is_running, [a.x, a.y, a.z], a.yaw, color);
    }
    (models, textures, place, keys, skinned)
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

/// Locate the project `data/` directory so the bin/-placed exe finds its
/// assets like the Blitz client does. Priority: `RCCE_DATA` env → a `data/`
/// next to or above the current dir → a `data/` next to or above the exe
/// (so `bin/ClientRS.exe` resolves `bin/../data`). Falls back to `"data"`.
fn resolve_data_root() -> String {
    if let Ok(p) = std::env::var("RCCE_DATA") {
        if !p.is_empty() {
            return p;
        }
    }
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd.clone());
        roots.push(cwd.join(".."));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf()); // e.g. bin/
            roots.push(dir.join("..")); // bin/.. = project root
            if let Some(up) = dir.parent() {
                roots.push(up.join("..")); // target/release/.. ladders
            }
        }
    }
    for r in &roots {
        let cand = r.join("data");
        if cand.is_dir() {
            return cand.to_string_lossy().into_owned();
        }
    }
    "data".to_string()
}

/// Load the GUI .bmp textures the HUD draws (function-button icons, the XP bar,
/// the empty action-bar slot) and register them with the overlay under stable
/// `gui:<Name>` keys. Missing files are skipped — the HUD falls back to text
/// labels / coloured rects when a key isn't registered.
fn register_gui_textures(overlay: &mut rcce_render::Overlay, device: &wgpu::Device, queue: &wgpu::Queue, data_root: &str) {
    let gui = std::path::Path::new(data_root).join("Textures").join("GUI");
    let files = [
        ("gui:Chat", "Chat.bmp"),
        ("gui:Map", "Map.bmp"),
        ("gui:Inventory", "Inventory.bmp"),
        ("gui:Abilities", "Abilities.bmp"),
        ("gui:Character", "Character.bmp"),
        ("gui:Quests", "Quests.bmp"),
        ("gui:Party", "Party.bmp"),
        ("gui:Menu", "Menu.bmp"),
        ("gui:EmptySlot", "EmptySlot.bmp"),
        ("gui:XP", "Action Bar XP.bmp"),
    ];
    let mut ok = 0;
    for (key, name) in files {
        if let Some(img) = rcce_data::texture::load(&gui.join(name)) {
            overlay.register_texture(device, queue, key, img.width, img.height, &img.rgba);
            ok += 1;
        }
    }
    println!("[client-window] registered {ok}/{} GUI textures from {}", files.len(), gui.display());
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
    // Real sky: resolve the area's SkyTexID through the texture catalog and
    // upload it for the textured skydome (else keep the gradient).
    let sky = scenery.env.sky_tex_id;
    if sky != 65535 {
        if let Some(img) = store.texture_path(sky).and_then(|p| rcce_data::texture::load(&p)) {
            view.set_sky_texture(&gfx.device, &gfx.queue, img.width, img.height, &img.rgba);
            println!("[client-window] zone '{zone}': sky texture {}x{} (id {sky})", img.width, img.height);
        } else {
            view.clear_sky_texture();
        }
    } else {
        view.clear_sky_texture();
    }
    // Cloud overlay (CloudTexID → drifting alpha-blended clouds).
    let cloud = scenery.env.cloud_tex_id;
    if cloud != 65535 {
        if let Some(img) = store.texture_path(cloud).and_then(|p| rcce_data::texture::load(&p)) {
            view.set_cloud_texture(&gfx.device, &gfx.queue, img.width, img.height, &img.rgba);
        } else {
            view.clear_cloud_texture();
        }
    } else {
        view.clear_cloud_texture();
    }
    // Night stars overlay (StarsTexID → additive stars, gated by night).
    let stars = scenery.env.stars_tex_id;
    if stars != 65535 {
        if let Some(img) = store.texture_path(stars).and_then(|p| rcce_data::texture::load(&p)) {
            view.set_stars_texture(&gfx.device, &gfx.queue, img.width, img.height, &img.rgba);
        } else {
            view.clear_stars_texture();
        }
    } else {
        view.clear_stars_texture();
    }
    let center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5, (min[2] + max[2]) * 0.5];
    let span = ((max[0] - min[0]).powi(2) + (max[2] - min[2]).powi(2)).sqrt().max(50.0);
    println!("[client-window] zone '{zone}': {} objects, {} meshes, span {span:.0}", place.len(), models.len());
    Some((center, span, min[1], scenery.env.clone()))
}

/// Result of loading a zone: camera framing + env + the decoded cloud textures
/// (regular + storm) for the storm-cloud swap.
struct ZoneLoad {
    center: [f32; 3],
    span: f32,
    ground_y: f32,
    env: rcce_data::AreaEnv,
    cloud_regular: Option<rcce_data::texture::Image>,
    cloud_storm: Option<rcce_data::texture::Image>,
}

/// Load a zone's scenery + sky/cloud/stars (via `load_zone_static`) and decode
/// its cloud textures. The single primitive used by both the initial load and a
/// live area-change reload.
fn load_zone_full(store: &mut AssetStore, view: &mut WorldView, gfx: &Gfx, data_root: &str, zone: &str) -> Option<ZoneLoad> {
    let (center, span, ground_y, env) = load_zone_static(store, view, gfx, data_root, zone)?;
    let load_img = |id: u16| -> Option<rcce_data::texture::Image> {
        (id != 65535).then(|| store.texture_path(id).and_then(|p| rcce_data::texture::load(&p))).flatten()
    };
    let cloud_regular = load_img(env.cloud_tex_id);
    let cloud_storm = load_img(env.storm_cloud_tex_id);
    Some(ZoneLoad { center, span, ground_y, env, cloud_regular, cloud_storm })
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

        let data_root = resolve_data_root();
        println!("[client-window] data root: {data_root}");
        self.data_root = data_root.clone();
        let mut store = match AssetStore::load(&data_root) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[client-window] assets: {e}");
                event_loop.exit();
                return;
            }
        };

        // Static scenery (always — also the fallback view).
        if let Some(z) = load_zone_full(&mut store, &mut view, &gfx, &data_root, &self.zone) {
            self.center = z.center;
            self.span = z.span;
            self.ground_y = z.ground_y;
            self.fog_color = z.env.fog_color;
            self.fog_near = z.env.fog_near;
            self.fog_far = z.env.fog_far;
            self.ambient = z.env.ambient;
            self.light_dir = z.env.light_dir;
            self.cloud_regular_img = z.cloud_regular;
            self.cloud_storm_img = z.cloud_storm;
            self.cloud_is_storm = false;
            if let Some(audio) = self.audio.as_mut() {
                audio.set_music(z.env.music_id, 0.4, |id| store.music_path(id));
            }
            self.loaded_zone = self.zone.clone();
        }
        // Resolve footstep sounds once (played as one-shots while moving).
        self.footstep_paths = store.footstep_sounds();

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
                    // Seed the live inventory from the fetched sheet; from here
                    // P_InventoryUpdate G/T/H/R keeps world.me_inventory current.
                    for it in &s.inventory {
                        world.me_inventory.insert(it.slot, *it);
                    }
                }
                self.sheet = outcome.sheet;
                self.net = Some(Net { transport, world, peer: outcome.peer, updates: 0 });
            }
            Err(e) => eprintln!("[client-window] login failed ({e}); zone-only spectator view"),
        }

        let mut overlay = rcce_render::Overlay::new(&gfx.device, format);
        register_gui_textures(&mut overlay, &gfx.device, &gfx.queue, &data_root);
        self.overlay = Some(overlay);
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
                        // Pick up the nearest dropped item within range.
                        KeyCode::KeyE if pressed => {
                            let occupied: std::collections::HashSet<u8> = self
                                .sheet
                                .as_ref()
                                .map(|s| s.inventory.iter().map(|i| i.slot).collect())
                                .unwrap_or_default();
                            let slot = (14u8..=45).find(|s| !occupied.contains(s)).unwrap_or(14);
                            if let Some(net) = self.net.as_mut() {
                                let (mx, mz) = (net.world.me_x, net.world.me_z);
                                let nearest = net
                                    .world
                                    .dropped_items
                                    .values()
                                    .map(|d| (d.handle, (d.x - mx).powi(2) + (d.z - mz).powi(2)))
                                    .filter(|(_, d2)| *d2 < 60.0 * 60.0)
                                    .min_by(|a, b| a.1.total_cmp(&b.1))
                                    .map(|(h, _)| h);
                                if let Some(h) = nearest {
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::INVENTORY_UPDATE,
                                        &rcce_client::net::pickup_packet(h, slot),
                                        true,
                                    );
                                }
                            }
                        }
                        // Action bar: cast the Nth memorised spell (1-9).
                        KeyCode::Digit1
                        | KeyCode::Digit2
                        | KeyCode::Digit3
                        | KeyCode::Digit4
                        | KeyCode::Digit5
                        | KeyCode::Digit6
                        | KeyCode::Digit7
                        | KeyCode::Digit8
                        | KeyCode::Digit9
                            if pressed =>
                        {
                            let idx = match code {
                                KeyCode::Digit1 => 0,
                                KeyCode::Digit2 => 1,
                                KeyCode::Digit3 => 2,
                                KeyCode::Digit4 => 3,
                                KeyCode::Digit5 => 4,
                                KeyCode::Digit6 => 5,
                                KeyCode::Digit7 => 6,
                                KeyCode::Digit8 => 7,
                                _ => 8,
                            };
                            // Inventory panel open: number keys act on the Nth
                            // item — Shift = equip (move to its gear slot), plain
                            // = drop one.
                            if self.show_inventory {
                                let item = self
                                    .net
                                    .as_ref()
                                    .and_then(|n| n.world.me_inventory.values().filter(|it| it.slot >= 14).nth(idx))
                                    .map(|it| (it.slot, it.item_id));
                                if let Some((slot, item_id)) = item {
                                    if self.run {
                                        // Equip: swap into the matching gear slot.
                                        let dest = self.store.as_ref().and_then(|s| s.item_equip_slot(item_id));
                                        if let (Some(dest), Some(net)) = (dest, self.net.as_mut()) {
                                            let rid = net.world.my_runtime_id;
                                            net.transport.send(
                                                net.peer,
                                                rcce_net::packet_id::INVENTORY_UPDATE,
                                                &rcce_client::net::inv_move_packet(rid, slot, dest, 0, true),
                                                true,
                                            );
                                        }
                                    } else if let Some(net) = self.net.as_mut() {
                                        net.transport.send(
                                            net.peer,
                                            rcce_net::packet_id::INVENTORY_UPDATE,
                                            &rcce_client::net::inv_drop_packet(slot, 1),
                                            true,
                                        );
                                    }
                                }
                                return;
                            }
                            // If a vendor window is open, the number keys buy
                            // the Nth offer; otherwise they cast the Nth spell.
                            let buy = self
                                .net
                                .as_ref()
                                .and_then(|n| n.world.current_trade.as_ref())
                                .and_then(|t| t.offers.get(idx))
                                .map(|o| (o.server_trade_id, o.amount.max(1)));
                            if let Some((trade_id, amount)) = buy {
                                if let Some(net) = self.net.as_mut() {
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::OPEN_TRADING,
                                        &rcce_client::net::trade_confirm_packet(&[(trade_id, amount)], &[]),
                                        true,
                                    );
                                }
                                return;
                            }
                            let cast = self
                                .sheet
                                .as_ref()
                                .and_then(|s| s.spells.iter().filter(|x| x.memorised).nth(idx))
                                .map(|sp| (sp.id, sp.recharge));
                            if let Some((spell_id, recharge)) = cast {
                                let now = self.start.elapsed().as_secs_f32();
                                let ready = self.spell_cooldowns.get(&spell_id).copied().unwrap_or(0.0);
                                if now >= ready {
                                    let target = self.target;
                                    if let Some(net) = self.net.as_mut() {
                                        net.transport.send(
                                            net.peer,
                                            rcce_net::packet_id::SPELL_UPDATE,
                                            &rcce_client::net::cast_packet(spell_id, target),
                                            true,
                                        );
                                    }
                                    self.spell_cooldowns
                                        .insert(spell_id, now + recharge as f32 / 1000.0);
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
                        // Toggle the inventory / spellbook panel.
                        KeyCode::KeyI if pressed => self.show_inventory = !self.show_inventory,
                        // Interact (right-click) the target/nearest NPC — a
                        // vendor replies with P_OpenTrading → the vendor panel.
                        KeyCode::KeyR if pressed => {
                            if let Some(net) = self.net.as_mut() {
                                let rid = self
                                    .target
                                    .or_else(|| nearest_living_actor(&net.world, net.world.me_x, net.world.me_z));
                                if let Some(rid) = rid {
                                    self.target = Some(rid);
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::RIGHT_CLICK,
                                        &rcce_client::net::right_click_packet(rid),
                                        true,
                                    );
                                }
                            }
                        }
                        // Examine the target/nearest NPC — reply arrives as chat.
                        KeyCode::KeyX if pressed => {
                            if let Some(net) = self.net.as_mut() {
                                let rid = self
                                    .target
                                    .or_else(|| nearest_living_actor(&net.world, net.world.me_x, net.world.me_z));
                                if let Some(rid) = rid {
                                    self.target = Some(rid);
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::EXAMINE,
                                        &rcce_client::net::examine_packet(rid),
                                        true,
                                    );
                                }
                            }
                        }
                        // Audio: M mutes, [ / ] lower / raise master volume.
                        KeyCode::KeyM if pressed => {
                            if let Some(a) = self.audio.as_mut() {
                                let m = a.toggle_mute();
                                println!("[audio] muted = {m}");
                            }
                        }
                        KeyCode::BracketLeft if pressed => {
                            if let Some(a) = self.audio.as_mut() {
                                a.adjust_master_volume(-0.1);
                            }
                        }
                        KeyCode::BracketRight if pressed => {
                            if let Some(a) = self.audio.as_mut() {
                                a.adjust_master_volume(0.1);
                            }
                        }
                        KeyCode::Escape => {
                            let trade_open = self
                                .net
                                .as_ref()
                                .map(|n| n.world.current_trade.is_some())
                                .unwrap_or(false);
                            if self.mouse_look {
                                self.set_mouse_look(false);
                            } else if trade_open {
                                if let Some(net) = self.net.as_mut() {
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::OPEN_TRADING,
                                        &rcce_client::net::trade_close_packet(),
                                        true,
                                    );
                                    net.world.current_trade = None;
                                }
                            } else {
                                event_loop.exit();
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = (position.x as f32, position.y as f32);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Left-click on the HUD acts only while mouse-look is off (cursor
                // free). Right-button toggles mouse-look on press for quick camera
                // grab, off on release.
                if button == MouseButton::Left && state == ElementState::Pressed && !self.mouse_look {
                    self.hud_click();
                } else if button == MouseButton::Right {
                    self.set_mouse_look(state == ElementState::Pressed);
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
    /// Handle a left-click on the HUD (mouse-look off). Hit-tests the bottom
    /// function-button row, then the inventory slot grid when the panel is open.
    /// Positions mirror the draw code exactly (shared FUNCTION_BUTTONS / the
    /// Interface.dat inv_buttons).
    fn hud_click(&mut self) {
        let Some(gfx) = self.gfx.as_ref() else { return };
        let (sw, sh) = (gfx.config.width as f32, gfx.config.height as f32);
        let (cx, cy) = self.cursor;

        // Function-button row.
        if let Some(action) = function_button_at(cx, cy, sw, sh) {
            match action {
                HudAction::Chat => {
                    if self.chat_input.is_none() {
                        self.chat_input = Some(String::new());
                    }
                }
                // The character panel shows gear + backpack + spells, so the
                // Inventory / Character / Spells icons all toggle it.
                HudAction::Inventory | HudAction::Character | HudAction::Spells => {
                    self.show_inventory = !self.show_inventory;
                }
                HudAction::Map | HudAction::Quests | HudAction::Party | HudAction::Menu => {
                    println!("[client-window] HUD button not yet implemented");
                }
            }
            return;
        }

        // Inventory slot grid (only when the panel is open and we have positions).
        // When the panel is closed, a non-HUD click is a world click → select the
        // nearest actor under the cursor as the target.
        if !self.show_inventory {
            self.world_pick(sw, sh, cx, cy);
            return;
        }

        // Drop / Eat buttons act on the last-hovered inventory slot.
        let action_btn = self
            .store
            .as_ref()
            .and_then(|s| s.interface())
            .and_then(|i| inv_action_button_at(cx, cy, i.inventory_window, i.inventory_drop, i.inventory_eat, sw, sh));
        if let Some(action) = action_btn {
            if let Some(slot) = self.last_inv_slot {
                let item = self
                    .net
                    .as_ref()
                    .and_then(|n| n.world.me_inventory.values().find(|it| it.slot == slot))
                    .map(|it| it.item_id);
                if let Some(item_id) = item {
                    match action {
                        InvAction::Drop => {
                            if let Some(net) = self.net.as_mut() {
                                net.transport.send(
                                    net.peer,
                                    rcce_net::packet_id::INVENTORY_UPDATE,
                                    &rcce_client::net::inv_drop_packet(slot, 1),
                                    true,
                                );
                            }
                        }
                        InvAction::Eat => {
                            // Only Potion (4) / Ingredient (5) are edible.
                            let edible = self
                                .store
                                .as_ref()
                                .and_then(|s| s.item_def(item_id))
                                .map(|d| d.item_type == 4 || d.item_type == 5)
                                .unwrap_or(false);
                            if edible {
                                if let Some(net) = self.net.as_mut() {
                                    net.transport.send(
                                        net.peer,
                                        rcce_net::packet_id::EAT_ITEM,
                                        &rcce_client::net::eat_item_packet(slot, 1),
                                        true,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            return;
        }

        let Some(iface) = self.store.as_ref().and_then(|s| s.interface()) else { return };
        let clicked_slot = inventory_slot_at(cx, cy, iface.inventory_window, &iface.inventory_buttons, sw, sh)
            .map(|i| i as u8);
        let Some(slot) = clicked_slot else { return };
        // Resolve the item in the clicked slot from the live inventory.
        let item = self
            .net
            .as_ref()
            .and_then(|n| n.world.me_inventory.values().find(|it| it.slot == slot))
            .map(|it| (it.slot, it.item_id));
        let Some((slot, item_id)) = item else { return };
        let shift = self.run;
        if slot < 14 {
            // Equipment slot click → unequip to the first free backpack slot.
            let occupied: std::collections::HashSet<u8> = self
                .net
                .as_ref()
                .map(|n| n.world.me_inventory.values().map(|it| it.slot).collect())
                .unwrap_or_default();
            let dest = (14u8..=45).find(|s| !occupied.contains(s));
            if let (Some(dest), Some(net)) = (dest, self.net.as_mut()) {
                let rid = net.world.my_runtime_id;
                net.transport.send(
                    net.peer,
                    rcce_net::packet_id::INVENTORY_UPDATE,
                    &rcce_client::net::inv_move_packet(rid, slot, dest, 0, true),
                    true,
                );
            }
        } else if shift {
            // Shift-click a backpack item → equip into its gear slot.
            let dest = self.store.as_ref().and_then(|s| s.item_equip_slot(item_id));
            if let (Some(dest), Some(net)) = (dest, self.net.as_mut()) {
                let rid = net.world.my_runtime_id;
                net.transport.send(
                    net.peer,
                    rcce_net::packet_id::INVENTORY_UPDATE,
                    &rcce_client::net::inv_move_packet(rid, slot, dest, 0, true),
                    true,
                );
            }
        } else if let Some(net) = self.net.as_mut() {
            // Plain click a backpack item → drop one.
            net.transport.send(
                net.peer,
                rcce_net::packet_id::INVENTORY_UPDATE,
                &rcce_client::net::inv_drop_packet(slot, 1),
                true,
            );
        }
    }

    /// World click: select the living actor whose projected position is nearest
    /// the cursor (within a pixel radius) as the target highlight. Uses the
    /// cached view-projection from the last rendered frame. No-op without a
    /// network world. The 'R'/'X' keys then interact with / examine the target.
    fn world_pick(&mut self, sw: f32, sh: f32, cx: f32, cy: f32) {
        const PICK_RADIUS: f32 = 48.0;
        let pick = self.net.as_ref().and_then(|net| {
            // Aim at roughly chest height so the pick matches the body.
            let actors: Vec<(u16, [f32; 3])> = net
                .world
                .actors
                .values()
                .filter(|a| a.alive)
                .map(|a| (a.runtime_id, [a.x, a.y + 3.0, a.z]))
                .collect();
            actor_at(cx, cy, &actors, &self.vp, sw, sh, PICK_RADIUS)
        });
        if let Some(rid) = pick {
            // A double-click on the same actor (or Shift+click) interacts; a
            // plain single click only selects the target.
            let now = Instant::now();
            let double = self.last_click_rid == Some(rid)
                && now.duration_since(self.last_click).as_millis() < 400;
            self.last_click = now;
            self.last_click_rid = Some(rid);
            self.target = Some(rid);
            if double || self.run {
                if let Some(net) = self.net.as_mut() {
                    net.transport.send(
                        net.peer,
                        rcce_net::packet_id::RIGHT_CLICK,
                        &rcce_client::net::right_click_packet(rid),
                        true,
                    );
                }
            }
        }
    }

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
            // Live area change (player warp): reload the new zone's scenery +
            // sky/clouds/stars + music. Gated by the zone name so it only fires
            // on an actual change, not every frame.
            if !net.world.zone.name.is_empty() && net.world.zone.name != self.loaded_zone {
                let zone = net.world.zone.name.clone();
                if let Some(z) = load_zone_full(store, view, gfx, &self.data_root, &zone) {
                    self.center = z.center;
                    self.span = z.span;
                    self.ground_y = z.ground_y;
                    self.fog_color = z.env.fog_color;
                    self.fog_near = z.env.fog_near;
                    self.fog_far = z.env.fog_far;
                    self.ambient = z.env.ambient;
                    self.light_dir = z.env.light_dir;
                    self.cloud_regular_img = z.cloud_regular;
                    self.cloud_storm_img = z.cloud_storm;
                    self.cloud_is_storm = false;
                    if let Some(audio) = self.audio.as_mut() {
                        audio.set_music(z.env.music_id, 0.4, |id| store.music_path(id));
                    }
                    println!("[client-window] reloaded zone '{zone}'");
                }
                // Mark loaded even if the area file was missing, so we don't
                // retry the reload (and its disk I/O) every frame.
                self.loaded_zone = zone;
            }
            // Flush any replies the apply() logic queued (e.g. the "GY" accept
            // when the server gives us an item).
            for (ptype, data) in net.world.pending_sends.drain(..) {
                net.transport.send(net.peer, ptype, &data, true);
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

            // GPU skinning makes the per-actor pose update cheap (just the
            // bone-palette uniform; the static body mesh is cached), so rebuild
            // every frame for smooth animation. The CPU path stays throttled to
            // ~12 Hz by dyn_hash (each rebuild re-skins + re-uploads vertices).
            let hash = dyn_hash(&net.world, elapsed);
            if self.gpu_skin || hash != self.last_dyn_hash {
                let (models, textures, place, keys, skinned) =
                    build_actors(store, &net.world, elapsed, self.ground_y, self.gpu_skin);
                // CPU drawables: attachments (+ bodies when GPU skinning is off).
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
                // GPU-skinned bodies (when enabled) — static mesh + pose uniform.
                if self.gpu_skin {
                    let sinst: Vec<rcce_render::SkinnedInstance> = skinned
                        .iter()
                        .map(|a| rcce_render::SkinnedInstance {
                            key: &a.key,
                            model: &a.model,
                            textures: &a.textures[..],
                            frame: a.frame,
                            transform: a.transform,
                            color: a.color,
                        })
                        .collect();
                    view.set_skinned(&gfx.device, &gfx.queue, &sinst);
                }
                self.last_dyn_hash = hash;
            }
            cam_target = [net.world.me_x, net.world.me_y, net.world.me_z];
            following = true;
        }
        if did_send {
            self.last_move = Instant::now();
        }
        self.was_moving = moving;

        // Footstep one-shots while the local player moves (faster when running).
        if let Some(idx) = self.footsteps.tick(elapsed, moving && following, self.run) {
            if let Some(audio) = self.audio.as_ref() {
                if !self.footstep_paths.is_empty() {
                    let p = &self.footstep_paths[idx % self.footstep_paths.len()];
                    audio.play_oneshot(p, 0.55);
                }
            }
        }

        // Weather ambient: rain loops while it's raining; a storm adds a wind
        // loop and periodic thunder one-shots — mirrors Environment3D.bb
        // SetWeather (W_Rain plays Snd_Rain; W_Storm adds Snd_Wind + thunder).
        {
            let wx = self
                .net
                .as_ref()
                .map(|n| rcce_client::weather::weather_from_byte(n.world.zone.weather))
                .unwrap_or(rcce_client::weather::Weather::Clear);
            let storm = wx == rcce_client::weather::Weather::Storm;
            // Storm-cloud swap: upload the darker storm clouds while storming,
            // the regular clouds otherwise — only on a change (no per-frame
            // reload). StormCloudTexID absent → keep the regular clouds.
            let want_storm_clouds = storm && self.cloud_storm_img.is_some();
            if want_storm_clouds != self.cloud_is_storm {
                let img = if want_storm_clouds {
                    self.cloud_storm_img.as_ref()
                } else {
                    self.cloud_regular_img.as_ref()
                };
                if let Some(img) = img {
                    view.set_cloud_texture(&gfx.device, &gfx.queue, img.width, img.height, &img.rgba);
                    self.cloud_is_storm = want_storm_clouds;
                }
            }
            let rain_p = wx.is_rainy().then(|| store.sound_path("Weather/Rain.ogg")).flatten();
            let wind_p = storm.then(|| store.sound_path("Weather/Wind.ogg")).flatten();
            let thunder_p = if storm && elapsed >= self.next_thunder {
                store.sound_path(&format!("Weather/Thunder{}.ogg", self.thunder_idx % 3 + 1))
            } else {
                None
            };
            if let Some(audio) = self.audio.as_mut() {
                let mut keep: Vec<&'static str> = Vec::new();
                if let Some(p) = &rain_p {
                    audio.set_ambient_loop("rain", p, 0.5);
                    keep.push("rain");
                }
                if let Some(p) = &wind_p {
                    audio.set_ambient_loop("wind", p, 0.4);
                    keep.push("wind");
                }
                audio.retain_ambient(&keep);
                if let Some(p) = &thunder_p {
                    audio.play_oneshot(p, 0.7);
                }
            }
            // Advance thunder scheduling (independent of whether audio exists).
            if storm {
                if elapsed >= self.next_thunder {
                    self.thunder_idx = self.thunder_idx.wrapping_add(1);
                    // Deterministic 8–15s gap from the counter (no RNG).
                    self.next_thunder = elapsed + 8.0 + (self.thunder_idx as f32 * 2.6) % 7.0;
                }
            } else {
                self.next_thunder = elapsed + 6.0;
            }
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
        self.vp = vp; // cache for world-click picking
        // Day/night: a slow local cycle modulates fog/sky + ambient. Cycle
        // length is RCCE_DAYNIGHT_SECS (default 600s); RCCE_PHASE pins a fixed
        // phase for screenshots.
        let phase = std::env::var("RCCE_PHASE")
            .ok()
            .and_then(|s| s.parse::<f32>().ok())
            .unwrap_or_else(|| {
                let cycle = std::env::var("RCCE_DAYNIGHT_SECS")
                    .ok()
                    .and_then(|s| s.parse::<f32>().ok())
                    .unwrap_or(600.0);
                rcce_client::daynight::phase_at(elapsed, cycle)
            });
        let sky = rcce_client::daynight::daynight(phase);
        let fog_dn = rcce_client::daynight::modulate(self.fog_color, &sky);
        let ambient_dn = rcce_client::daynight::modulate(self.ambient, &sky);
        view.render(
            &gfx.device,
            &gfx.queue,
            &tview,
            vp,
            eye,
            fog_dn,
            self.fog_near,
            self.fog_far,
            ambient_dn,
            self.light_dir,
            wgpu::Color {
                r: fog_dn[0] as f64,
                g: fog_dn[1] as f64,
                b: fog_dn[2] as f64,
                a: 1.0,
            },
            self.cam_yaw,
            elapsed,
            rcce_client::daynight::night_factor(phase),
        );

        // 2D overlay: nameplates + health bars over actors, and a player HUD.
        let target_rid = self.target;
        if let Some(overlay) = self.overlay.as_mut() {
            let (sw, sh) = (gfx.config.width as f32, gfx.config.height as f32);
            let white = [1.0, 1.0, 1.0, 1.0];

            // Weather particles (rain/snow) — drawn first so they sit behind the
            // HUD/nameplates. Driven by the zone's weather byte.
            let wkind = self
                .net
                .as_ref()
                .map(|n| rcce_client::weather::weather_from_byte(n.world.zone.weather))
                .unwrap_or(rcce_client::weather::Weather::Clear);
            let dt = (elapsed - self.prev_elapsed).clamp(0.0, 0.1);
            self.prev_elapsed = elapsed;
            self.weather.update(dt, sw, sh, wkind);
            match wkind {
                // Storm rains too, with a slightly heavier/greyer streak.
                rcce_client::weather::Weather::Rain | rcce_client::weather::Weather::Storm => {
                    let streak = if wkind == rcce_client::weather::Weather::Storm {
                        (2.0, 11.0, [0.55, 0.6, 0.75, 0.6])
                    } else {
                        (1.5, 9.0, [0.6, 0.7, 0.9, 0.5])
                    };
                    for p in self.weather.particles() {
                        overlay.rect(p.x, p.y, streak.0, streak.1, streak.2);
                    }
                }
                rcce_client::weather::Weather::Snow => {
                    for p in self.weather.particles() {
                        overlay.rect(p.x, p.y, 3.0, 3.0, [1.0, 1.0, 1.0, 0.8]);
                    }
                }
                rcce_client::weather::Weather::Clear => {}
            }

            // Compass strip (top-centre) at the real Interface.dat `compass` rect,
            // scrolling with the camera heading. Always on (uses cam_yaw), like
            // Client.exe's compass.
            if let Some(comp) = store.interface().map(|i| i.compass) {
                if comp.w > 0.0 && comp.h > 0.0 {
                    let (cx0, cy0, cw, chh) = comp.px(sw, sh);
                    let center = cx0 + cw * 0.5;
                    overlay.rect(cx0, cy0, cw, chh, [0.0, 0.25, 0.0, 0.4]);
                    // Centre reference tick (dead ahead).
                    overlay.rect(center - 0.5, cy0, 1.5, chh, [0.7, 1.0, 0.7, 0.9]);
                    use std::f32::consts::PI;
                    for (off, label) in compass_marks(self.cam_yaw, PI) {
                        let mx = center + off * cw;
                        if label.is_empty() {
                            // Intercardinal tick.
                            overlay.rect(mx - 0.5, cy0 + chh * 0.5, 1.0, chh * 0.5, [0.4, 0.8, 0.4, 0.8]);
                        } else {
                            let tw = rcce_render::font::text_width(label, 1.0);
                            overlay.text_shadow(mx - tw * 0.5, cy0 + 1.0, 1.0, label, [0.7, 1.0, 0.7, 1.0]);
                        }
                    }
                }
            }

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
                        // Equipped weapon (from P_InventoryUpdate "O") under the name.
                        if a.equipped[0] != 0xFFFF {
                            let wname = store.item_name(a.equipped[0]);
                            let tw = rcce_render::font::text_width(&wname, 1.0);
                            overlay.text_shadow(px - tw * 0.5, py - 38.0, 1.0, &wname, [0.85, 0.85, 0.7, 1.0]);
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

                // Minimap/radar at the real Interface.dat Radar rect (right
                // side), forward-up radar of nearby actors + loot.
                {
                    let (cx, cy, r) = match store.interface() {
                        Some(iface) => {
                            let rd = iface.radar;
                            let r = (rd.w * sw).min(rd.h * sh) * 0.5;
                            ((rd.x + rd.w * 0.5) * sw, (rd.y + rd.h * 0.5) * sh, r)
                        }
                        None => (74.0, 74.0, 64.0),
                    };
                    let yaw = self.cam_yaw;
                    let range = 140.0;
                    let (mx, mz) = (net.world.me_x, net.world.me_z);
                    overlay.rect(cx - r - 4.0, cy - r - 4.0, (r + 4.0) * 2.0, (r + 4.0) * 2.0, [0.0, 0.0, 0.0, 0.5]);
                    // Heading line (forward = up) then the player pip at centre.
                    overlay.rect(cx - 1.0, cy - r * 0.5, 2.0, r * 0.5, [0.4, 0.8, 0.4, 0.7]);
                    overlay.rect(cx - 2.0, cy - 2.0, 4.0, 4.0, [0.6, 1.0, 0.6, 1.0]);
                    for a in net.world.actors.values() {
                        if let Some((ox, oy)) = rcce_client::radar::world_to_radar(a.x - mx, a.z - mz, yaw, range, r) {
                            let col = if Some(a.runtime_id) == target_rid {
                                [1.0, 0.85, 0.2, 1.0]
                            } else if a.is_player {
                                [0.4, 0.7, 1.0, 1.0]
                            } else {
                                [0.95, 0.35, 0.35, 1.0]
                            };
                            overlay.rect(cx + ox - 2.0, cy + oy - 2.0, 4.0, 4.0, col);
                        }
                    }
                    for d in net.world.dropped_items.values() {
                        if let Some((ox, oy)) = rcce_client::radar::world_to_radar(d.x - mx, d.z - mz, yaw, range, r) {
                            overlay.rect(cx + ox - 1.5, cy + oy - 1.5, 3.0, 3.0, [1.0, 0.85, 0.3, 1.0]);
                        }
                    }
                }

                // Status-effect pills at the real Buffs rect (top-right).
                if !net.world.active_effects.is_empty() {
                    let (mut ex, ey) = match store.interface() {
                        Some(iface) => (iface.buffs.x * sw, iface.buffs.y * sh),
                        None => (10.0, 152.0),
                    };
                    for eff in &net.world.active_effects {
                        let label: String = eff.name.chars().take(12).collect();
                        let tw = rcce_render::font::text_width(&label, 1.0);
                        let pillw = tw + 10.0;
                        overlay.rect(ex, ey, pillw, 14.0, [0.32, 0.16, 0.36, 0.82]);
                        overlay.text_shadow(ex + 5.0, ey + 2.0, 1.0, &label, [1.0, 0.85, 1.0, 1.0]);
                        ex += pillw + 4.0;
                    }
                }

                // Dropped-item loot markers: a gold pip + name/amount at the
                // item's world position. "[E]" hint on the nearest in range.
                if !net.world.dropped_items.is_empty() {
                    let (mx, mz) = (net.world.me_x, net.world.me_z);
                    let nearest = net
                        .world
                        .dropped_items
                        .values()
                        .map(|d| (d.handle, (d.x - mx).powi(2) + (d.z - mz).powi(2)))
                        .min_by(|a, b| a.1.total_cmp(&b.1));
                    let gold = [1.0, 0.85, 0.3, 1.0];
                    for d in net.world.dropped_items.values() {
                        if let Some((px, py)) = rcce_render::project(&vp, [d.x, d.y + 1.2, d.z], sw, sh) {
                            overlay.rect(px - 3.0, py - 3.0, 6.0, 6.0, gold);
                            let name = store.item_name(d.item_id);
                            let label = if d.amount > 1 { format!("{name} x{}", d.amount) } else { name };
                            let in_range = nearest.map(|(h, d2)| h == d.handle && d2 < 60.0 * 60.0).unwrap_or(false);
                            let label = if in_range { format!("{label}  [E]") } else { label };
                            let tw = rcce_render::font::text_width(&label, 1.0);
                            overlay.text_shadow(px - tw * 0.5, py - 16.0, 1.0, &label, gold);
                        }
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
                // Vitals bars at the real Interface.dat fractional positions
                // (Health top-left red, Energy below it blue, …), matching
                // Client.exe instead of an invented bottom HUD.
                if let Some(iface) = store.interface() {
                    for (i, a) in iface.attributes.iter().enumerate() {
                        if a.w <= 0.001 || a.h <= 0.001 {
                            continue;
                        }
                        let (val, max) = if i == 0 {
                            (w.me_health.max(0) as f32, w.me_health_max.max(1) as f32)
                        } else if let Some(&(v, m)) = w.me_attributes.get(&(i as u8)) {
                            (v.max(0) as f32, m.max(1) as f32)
                        } else {
                            continue;
                        };
                        let (vx, vy, vw, vh) = a.px(sw, sh);
                        let frac = (val / max).clamp(0.0, 1.0);
                        let col = [a.rgb[0] as f32 / 255.0, a.rgb[1] as f32 / 255.0, a.rgb[2] as f32 / 255.0, 1.0];
                        overlay.rect(vx - 1.0, vy - 1.0, vw + 2.0, vh + 2.0, [0.0, 0.0, 0.0, 0.6]);
                        overlay.bar(vx, vy, vw, vh, frac, col);
                        if i == 0 {
                            let s = format!("{}/{}", val as i32, max as i32);
                            overlay.text_shadow(vx + 3.0, vy + vh * 0.5 - 4.0, 1.0, &s, white);
                        }
                    }
                } else {
                    overlay.rect(10.0, sh - 56.0, 270.0, 48.0, [0.0, 0.0, 0.0, 0.45]);
                    overlay.bar(18.0, sh - 28.0, 200.0, 12.0, hpf, [0.2, 0.8, 0.25, 1.0]);
                }
                overlay.text_shadow(8.0, sh - 16.0, 1.0, &w.zone.name, [0.8, 0.85, 0.9, 1.0]);
                overlay.text(sw - 84.0, 10.0, 1.0, &format!("{fps:.0} fps"), [0.8, 1.0, 0.8, 1.0]);
                // Character sheet readout (level + gold) from P_FetchCharacter.
                if let Some(sheet) = &self.sheet {
                    let line = format!("Lv {}   {}g", sheet.level, sheet.gold);
                    let tw = rcce_render::font::text_width(&line, 1.0);
                    overlay.text_shadow(sw - tw - 12.0, 24.0, 1.0, &line, [1.0, 0.88, 0.4, 1.0]);
                }
                // Audio readout (M mute, [ / ] volume).
                if let Some(a) = self.audio.as_ref() {
                    let s = if a.is_muted() {
                        "Audio: muted".to_string()
                    } else {
                        format!("Vol {}%", (a.master_volume() * 100.0).round() as i32)
                    };
                    let tw = rcce_render::font::text_width(&s, 1.0);
                    let col = if a.is_muted() { [1.0, 0.6, 0.6, 1.0] } else { [0.7, 0.85, 1.0, 1.0] };
                    overlay.text_shadow(sw - tw - 12.0, 38.0, 1.0, &s, col);
                }

                // Chat log at the real Chat rect (bottom-left), newest at the
                // bottom of the box.
                let (cx0, cy0, cw, chh) = match store.interface() {
                    Some(iface) => iface.chat.px(sw, sh),
                    None => (14.0, sh - 160.0, 388.0, 152.0),
                };
                overlay.rect(cx0, cy0, cw, chh, [0.0, 0.0, 0.0, 0.28]);
                let max_lines = ((chh / 12.0) as usize).max(1);
                let bottom = cy0 + chh - 13.0;
                for (i, line) in w.chat.iter().rev().take(max_lines).enumerate() {
                    let y = bottom - i as f32 * 12.0;
                    let s: String = line.chars().take(60).collect();
                    overlay.text_shadow(cx0 + 4.0, y, 1.0, &s, [0.9, 0.9, 0.7, 1.0]);
                }
            }
            // Chat input line just under the chat box (real Chat rect bottom).
            if let Some(buf) = self.chat_input.as_ref() {
                let (cx0, cy0, cw, chh) = match store.interface() {
                    Some(iface) => iface.chat.px(sw, sh),
                    None => (14.0, sh - 160.0, 388.0, 152.0),
                };
                overlay.rect(cx0, cy0 + chh, cw, 16.0, [0.0, 0.0, 0.0, 0.6]);
                let caret = if (elapsed * 2.0) as i64 % 2 == 0 { "_" } else { " " };
                overlay.text_shadow(cx0 + 4.0, cy0 + chh + 2.0, 1.0, &format!("> {buf}{caret}"), [1.0, 1.0, 1.0, 1.0]);
            }

            // Inventory / spellbook panel (toggle with I). Item names resolve
            // through Items.dat; spell names arrive over the wire.
            if self.show_inventory {
                // Match the real InventoryWindow rect (centred ~0.25,0.2,0.5,0.55)
                // and draw the 46-slot grid at the real window-relative button
                // positions (Interface.dat inv_buttons): rows 0-1 are the 14
                // equipment slots, rows 2-5 the 32 backpack slots.
                let dim = [0.6, 0.6, 0.6, 1.0];
                let iface = store.interface();
                let (px, py, pw, ph) = match iface {
                    Some(i) => {
                        let r = i.inventory_window.px(sw, sh);
                        (r.0.round(), r.1.round(), r.2.round(), r.3.round())
                    }
                    None => {
                        let (pw, ph) = (340.0f32, 384.0f32);
                        (((sw - pw) * 0.5).round(), ((sh - ph) * 0.5).round(), pw, ph)
                    }
                };
                overlay.rect(px, py, pw, ph, [0.05, 0.06, 0.10, 0.92]);
                overlay.rect(px, py, pw, 22.0, [0.15, 0.18, 0.28, 0.96]);
                overlay.text_shadow(px + 10.0, py + 6.0, 1.5, "Character", white);
                overlay.text(px + pw - 78.0, py + 7.0, 1.0, "[I] close", dim);

                // Attributes column to the left of the inventory window: the
                // named, non-hidden attribute slots (Attributes.dat) with live
                // values from the character sheet.
                if let Some(sheet) = &self.sheet {
                    let mut rows: Vec<(String, [f32; 4])> = Vec::new();
                    for i in 0..sheet.attributes.len().min(rcce_data::AttributeNames::COUNT) {
                        if let Some(name) = store.attribute_name(i) {
                            let (val, mx) = sheet.attributes[i];
                            let line = if i <= 1 && mx > 0 {
                                format!("{name}: {val}/{mx}")
                            } else {
                                format!("{name}: {val}")
                            };
                            let col = match i {
                                0 => [1.0, 0.55, 0.5, 1.0],
                                1 => [0.55, 0.7, 1.0, 1.0],
                                _ => white,
                            };
                            rows.push((line, col));
                        }
                    }
                    if !rows.is_empty() {
                        let aw = 152.0f32;
                        let ax = (px - aw - 6.0).max(4.0);
                        let boxh = 24.0 + rows.len() as f32 * 13.0 + 6.0;
                        overlay.rect(ax, py, aw, boxh, [0.05, 0.06, 0.10, 0.92]);
                        overlay.rect(ax, py, aw, 20.0, [0.15, 0.18, 0.28, 0.96]);
                        overlay.text_shadow(ax + 8.0, py + 5.0, 1.0, "Attributes", white);
                        let mut ay = py + 24.0;
                        for (line, col) in &rows {
                            overlay.text(ax + 8.0, ay, 1.0, line, *col);
                            ay += 13.0;
                        }
                    }
                }

                // Spells column to the right of the inventory window: all known
                // spells (icon + name + level), memorised ones highlighted.
                if let Some(sheet) = &self.sheet {
                    if !sheet.spells.is_empty() {
                        let cw2 = 174.0f32;
                        let sx = (px + pw + 6.0).min((sw - cw2 - 4.0).max(0.0));
                        let rowh = 16.0f32;
                        let cap = (((ph - 24.0) / rowh) as usize).max(1);
                        let total = sheet.spells.len();
                        let shown = if total > cap { cap - 1 } else { total };
                        let rows_drawn = shown + if total > cap { 1 } else { 0 };
                        let boxh = 24.0 + rows_drawn as f32 * rowh + 4.0;
                        overlay.rect(sx, py, cw2, boxh, [0.05, 0.06, 0.10, 0.92]);
                        overlay.rect(sx, py, cw2, 20.0, [0.15, 0.18, 0.28, 0.96]);
                        overlay.text_shadow(sx + 8.0, py + 5.0, 1.0, &format!("Spells ({total})"), white);
                        let mut sy = py + 24.0;
                        for sp in sheet.spells.iter().take(shown) {
                            let key = format!("spell:{}", sp.id);
                            if !overlay.has_texture(&key) {
                                if let Some(img) =
                                    store.texture_path(sp.thumb_tex).and_then(|p| rcce_data::texture::load(&p))
                                {
                                    overlay.register_texture(&gfx.device, &gfx.queue, &key, img.width, img.height, &img.rgba);
                                }
                            }
                            if overlay.has_texture(&key) {
                                overlay.image(sx + 4.0, sy, 13.0, 13.0, &key, [1.0, 1.0, 1.0, 1.0]);
                            } else {
                                overlay.rect(sx + 4.0, sy, 13.0, 13.0, [0.1, 0.1, 0.16, 0.9]);
                            }
                            let col = if sp.memorised { [1.0, 0.9, 0.5, 1.0] } else { [0.85, 0.85, 0.9, 1.0] };
                            let star = if sp.memorised { "*" } else { "" };
                            let nm: String = format!("{}{} (L{})", star, sp.name, sp.level).chars().take(24).collect();
                            overlay.text(sx + 20.0, sy + 3.0, 1.0, &nm, col);
                            sy += rowh;
                        }
                        if total > cap {
                            overlay.text(sx + 8.0, sy + 3.0, 1.0, &format!("+{} more", total - shown), dim);
                        }
                    }
                }

                // Slot index -> item, from the live inventory.
                let me_inv = self.net.as_ref().map(|n| &n.world.me_inventory);
                let mut by_slot: std::collections::HashMap<u8, (u16, u16)> = std::collections::HashMap::new();
                if let Some(m) = me_inv {
                    for it in m.values() {
                        by_slot.insert(it.slot, (it.item_id, it.amount));
                    }
                }

                if let Some(iface) = iface {
                    let iw = &iface.inventory_window;
                    // Header line: level / gold / xp.
                    if let Some(s) = &self.sheet {
                        overlay.text_shadow(px + 10.0, py + 26.0, 1.0, &format!("Lv {}   {} gold   {} xp", s.level, s.gold, s.xp), [1.0, 0.88, 0.4, 1.0]);
                    }
                    for (i, b) in iface.inventory_buttons.iter().enumerate() {
                        // Window-relative fraction -> screen pixels.
                        let bx = (iw.x + b.x * iw.w) * sw;
                        let bgy = (iw.y + b.y * iw.h) * sh;
                        let bw = (b.w * iw.w * sw).max(8.0);
                        let bh = (b.h * iw.h * sh).max(8.0);
                        let equip = i < 14;
                        let occupied = by_slot.contains_key(&(i as u8));
                        // Real EmptySlot.bmp frame, with a translucent state tint
                        // layered on top (interleaved draw list); the rect is the
                        // opaque fallback when the texture is missing.
                        if overlay.has_texture("gui:EmptySlot") {
                            overlay.image(bx, bgy, bw, bh, "gui:EmptySlot", [1.0, 1.0, 1.0, 1.0]);
                            let tint = match (equip, occupied) {
                                (true, true) => [0.30, 0.45, 0.25, 0.35],
                                (false, true) => [0.30, 0.35, 0.55, 0.35],
                                _ => [0.0, 0.0, 0.0, 0.0],
                            };
                            if tint[3] > 0.0 {
                                overlay.rect(bx, bgy, bw, bh, tint);
                            }
                        } else {
                            let bg = match (equip, occupied) {
                                (true, true) => [0.20, 0.26, 0.18, 0.95],
                                (true, false) => [0.12, 0.14, 0.12, 0.85],
                                (false, true) => [0.16, 0.18, 0.26, 0.95],
                                (false, false) => [0.09, 0.10, 0.14, 0.82],
                            };
                            overlay.rect(bx, bgy, bw, bh, bg);
                        }
                        // Equipment slots show their slot-name when empty.
                        if equip && !occupied {
                            if let Some(name) = rcce_data::equip_slot_name(i as u8) {
                                let abbr: String = name.chars().take(((bw / 6.0) as usize).max(2)).collect();
                                overlay.text(bx + 2.0, bgy + bh * 0.5 - 4.0, 1.0, &abbr, [0.45, 0.45, 0.5, 1.0]);
                            }
                        }
                        if let Some(&(item_id, amount)) = by_slot.get(&(i as u8)) {
                            // Draw the real item thumbnail (lazily registered from
                            // the item's ThumbnailTexID on first sight) over the
                            // slot frame; fall back to the name abbreviation.
                            let key = format!("item:{item_id}");
                            if !overlay.has_texture(&key) {
                                if let Some(img) =
                                    store.item_icon_path(item_id).and_then(|p| rcce_data::texture::load(&p))
                                {
                                    overlay.register_texture(&gfx.device, &gfx.queue, &key, img.width, img.height, &img.rgba);
                                }
                            }
                            if overlay.has_texture(&key) {
                                let pad = (bw * 0.1).min(3.0);
                                overlay.image(bx + pad, bgy + pad, bw - pad * 2.0, bh - pad * 2.0, &key, [1.0, 1.0, 1.0, 1.0]);
                            } else {
                                let name = store.item_name(item_id);
                                let maxc = ((bw / 6.0) as usize).max(2);
                                let abbr: String = name.chars().take(maxc).collect();
                                overlay.text_shadow(bx + 2.0, bgy + 2.0, 1.0, &abbr, white);
                            }
                            if amount > 1 {
                                overlay.text(bx + 2.0, bgy + bh - 9.0, 1.0, &format!("x{amount}"), [0.8, 1.0, 0.8, 1.0]);
                            }
                        }
                        // Backpack 1-9 keybind hint in the slot corner.
                        if !equip {
                            let bp_idx = i - 14;
                            if bp_idx < 9 {
                                overlay.text(bx + bw - 7.0, bgy + 1.0, 1.0, &format!("{}", bp_idx + 1), [1.0, 1.0, 0.5, 0.9]);
                            }
                        }
                    }
                    // Real inv_gold display + Drop / Eat buttons at their
                    // window-relative Interface.dat positions.
                    let iw = iface.inventory_window;
                    let to_scr = |c: rcce_data::IComp| -> (f32, f32, f32, f32) {
                        ((iw.x + c.x * iw.w) * sw, (iw.y + c.y * iw.h) * sh, c.w * iw.w * sw, c.h * iw.h * sh)
                    };
                    let gold = self.sheet.as_ref().map(|s| s.gold).unwrap_or(0);
                    let (gx, gy, _, _) = to_scr(iface.inventory_gold);
                    overlay.text_shadow(gx, gy, 1.0, &format!("Gold: {gold}"), [1.0, 0.88, 0.4, 1.0]);
                    for (comp, label) in [(iface.inventory_drop, "Drop"), (iface.inventory_eat, "Eat")] {
                        let (dx, dy, dw, dh) = to_scr(comp);
                        if dw > 1.0 && dh > 1.0 {
                            overlay.rect(dx, dy, dw, dh, [0.20, 0.16, 0.12, 0.9]);
                            let tw = rcce_render::font::text_width(label, 1.0);
                            overlay.text_shadow(dx + (dw - tw) * 0.5, dy + dh * 0.5 - 4.0, 1.0, label, white);
                        }
                    }
                    overlay.text(px + 10.0, py + ph - 13.0, 1.0, "1-9 drop  ·  Shift+1-9 equip", dim);
                } else if let Some(s) = &self.sheet {
                    // Fallback text list when Interface.dat is absent.
                    overlay.text_shadow(px + 10.0, py + 30.0, 1.0, &format!("Lv {}   {} gold", s.level, s.gold), [1.0, 0.88, 0.4, 1.0]);
                } else {
                    overlay.text(px + 10.0, py + 30.0, 1.0, "(no character data)", dim);
                }
            }

            // Vendor / trade window (P_OpenTrading) — lists what the NPC offers,
            // with names from Items.dat and prices from each item's value.
            if let Some(trade) = self.net.as_ref().and_then(|n| n.world.current_trade.as_ref()) {
                use rcce_client::trade::TradeKind;
                let dimc = [0.6, 0.6, 0.6, 1.0];
                let (pw, ph) = (320.0, 300.0);
                let (px, py) = ((sw - pw - 40.0).round(), ((sh - ph) * 0.5).round());
                overlay.rect(px, py, pw, ph, [0.07, 0.06, 0.05, 0.92]);
                overlay.rect(px, py, pw, 22.0, [0.28, 0.22, 0.12, 0.96]);
                let title = match trade.kind {
                    TradeKind::Npc => "Vendor",
                    TradeKind::Scenery => "Container",
                    TradeKind::Player => "Trade",
                };
                overlay.text_shadow(px + 10.0, py + 6.0, 1.5, title, white);
                overlay.text(px + pw - 80.0, py + 7.0, 1.0, "[Esc] close", dimc);
                let mut y = py + 30.0;
                if trade.offers.is_empty() {
                    overlay.text(px + 10.0, y, 1.0, "(nothing for sale)", dimc);
                } else {
                    overlay.text(px + 10.0, y, 1.0, "Press 1-9 to buy:", dimc);
                    y += 14.0;
                    for (i, off) in trade.offers.iter().enumerate() {
                        if y > py + ph - 16.0 { break; }
                        let name = store.item_name(off.item_id);
                        let qty = if off.amount > 1 { format!(" x{}", off.amount) } else { String::new() };
                        let num = if i < 9 { format!("{}. ", i + 1) } else { "   ".to_string() };
                        let line: String = format!("{num}{name}{qty}").chars().take(30).collect();
                        overlay.text(px + 12.0, y, 1.0, &line, white);
                        // Price = the item's base value (Items.dat), right-aligned.
                        let price = format!("{}g", store.item_value(off.item_id).max(0));
                        let pw2 = rcce_render::font::text_width(&price, 1.0);
                        overlay.text(px + pw - pw2 - 12.0, y, 1.0, &price, [1.0, 0.88, 0.4, 1.0]);
                        y += 14.0;
                    }
                }
            }

            // Bottom action bar + function buttons, placed at the real
            // Client.exe fractional coordinates (Interface3D.bb:3511-3534 /
            // CreateActionBarButton). The row sits on the Y=0.9415 baseline.
            // 4:3 layout: 12 spell slots left-anchored at 0.089867187 + i*pitch,
            // function buttons right-anchored at fixed X positions.
            {
                // Bound to the shared module consts so the draw geometry and the
                // hover/click hit-tests (spell_slot_at) can't drift apart.
                const BAR_Y: f32 = FBTN_Y;
                const SLOT_W: f32 = FBTN_W;
                const SLOT_H: f32 = FBTN_H;
                const SLOT_PITCH: f32 = SPELLBAR_PITCH;
                const SLOT_X0: f32 = SPELLBAR_X0;
                let (sw_, sh_, bw, bh) = (SLOT_W * sw, SLOT_H * sh, SLOT_W * sw, SLOT_H * sh);
                let by = BAR_Y * sh;
                // 12 spell slots; memorised spells fill the first N in order.
                let mem: Vec<_> = self
                    .sheet
                    .as_ref()
                    .map(|s| s.spells.iter().filter(|sp| sp.memorised).take(12).collect::<Vec<_>>())
                    .unwrap_or_default();
                for i in 0..12usize {
                    let x = (SLOT_X0 + i as f32 * SLOT_PITCH) * sw;
                    // Real EmptySlot.bmp frame under each slot (interleaved draw
                    // list lets the shading + number layer on top); coloured-rect
                    // fallback when the texture is missing.
                    if overlay.has_texture("gui:EmptySlot") {
                        overlay.image(x, by, sw_, sh_, "gui:EmptySlot", [1.0, 1.0, 1.0, 1.0]);
                    } else {
                        overlay.rect(x, by, sw_, sh_, [0.08, 0.08, 0.13, 0.78]);
                    }
                    if let Some(sp) = mem.get(i) {
                        // Real spell icon (Spell.ThumbnailTexID → Textures.dat),
                        // lazily registered, drawn over the slot frame; cooldown
                        // shade + number + name layer on top.
                        let key = format!("spell:{}", sp.id);
                        if !overlay.has_texture(&key) {
                            if let Some(img) =
                                store.texture_path(sp.thumb_tex).and_then(|p| rcce_data::texture::load(&p))
                            {
                                overlay.register_texture(&gfx.device, &gfx.queue, &key, img.width, img.height, &img.rgba);
                            }
                        }
                        let has_icon = overlay.has_texture(&key);
                        if has_icon {
                            let pad = (sw_ * 0.08).min(2.0);
                            overlay.image(x + pad, by + pad, sw_ - pad * 2.0, sh_ - pad * 2.0, &key, [1.0, 1.0, 1.0, 1.0]);
                        }
                        let ready = self.spell_cooldowns.get(&sp.id).copied().unwrap_or(0.0);
                        let remaining = (ready - elapsed).max(0.0);
                        if remaining > 0.0 {
                            let span = (sp.recharge as f32 / 1000.0).max(0.1);
                            let frac = (remaining / span).clamp(0.0, 1.0);
                            overlay.rect(x, by, sw_, sh_ * frac, [0.0, 0.0, 0.0, 0.6]);
                        }
                        if i < 9 {
                            overlay.text_shadow(x + 2.0, by + 1.0, 1.0, &format!("{}", i + 1), [1.0, 1.0, 0.6, 1.0]);
                        }
                        if !has_icon {
                            let abbr: String = sp.name.chars().take(4).collect();
                            overlay.text(x + 2.0, by + sh_ - 9.0, 1.0, &abbr, white);
                        }
                    }
                }
                // Function buttons (right cluster), drawn with the real GUI .bmp
                // icons when registered; text labels are the fallback. The active
                // panel (inventory) is highlighted. Positions come from the shared
                // FUNCTION_BUTTONS table so hit-testing matches exactly.
                for (action, fx, key, label) in FUNCTION_BUTTONS {
                    let x = fx * sw;
                    let active = action == HudAction::Inventory && self.show_inventory;
                    if overlay.has_texture(key) {
                        let tint = if active { [1.0, 1.0, 0.6, 1.0] } else { [1.0, 1.0, 1.0, 1.0] };
                        overlay.image(x, by, bw, bh, key, tint);
                    } else {
                        let bg = if active { [0.32, 0.30, 0.12, 0.9] } else { [0.12, 0.12, 0.18, 0.82] };
                        overlay.rect(x, by, bw, bh, bg);
                        overlay.text_shadow(x + 3.0, by + bh * 0.5 - 4.0, 1.0, label, [0.85, 0.85, 0.7, 1.0]);
                    }
                }

                // XP bar along the very bottom. The server sends a 0..255 fill
                // (P_XPUpdate "B"); Client.exe clips the Action Bar XP texture to
                // that fraction (UpdateXPBar: ScaleEntity + VertexTexCoords), which
                // maps to drawing image_uv over a dark backing.
                let fill = self.net.as_ref().map(|n| n.world.me_xp_bar as f32 / 255.0).unwrap_or(0.0);
                let xb_x = SLOT_X0 * sw;
                let xb_w = (0.92 - SLOT_X0) * sw;
                let xb_h = (sh * 0.012).max(5.0);
                let xb_y = sh - xb_h - 2.0;
                overlay.rect(xb_x, xb_y, xb_w, xb_h, [0.0, 0.0, 0.0, 0.7]);
                if fill > 0.0 {
                    if overlay.has_texture("gui:XP") {
                        overlay.image_uv(xb_x, xb_y, xb_w * fill, xb_h, "gui:XP", [0.0, 0.0, fill, 1.0], [1.0, 1.0, 1.0, 1.0]);
                    } else {
                        overlay.rect(xb_x, xb_y, xb_w * fill, xb_h, [0.7, 0.55, 0.15, 0.95]);
                    }
                }
            }

            // Hover tooltip (topmost): an inventory slot while the panel is open,
            // else a spell-bar slot. Shows the item/spell name + stats near the
            // cursor, clamped to stay on screen.
            {
                let (cx, cy) = self.cursor;
                let white = [1.0, 1.0, 1.0, 1.0];
                let gold = [1.0, 0.88, 0.4, 1.0];
                let accent = [0.8, 0.85, 1.0, 1.0];
                let mut lines: Vec<(String, [f32; 4])> = Vec::new();
                if self.show_inventory {
                    if let Some(iface) = store.interface() {
                        if let Some(slot) =
                            inventory_slot_at(cx, cy, iface.inventory_window, &iface.inventory_buttons, sw, sh)
                        {
                            if let Some(it) = self
                                .net
                                .as_ref()
                                .and_then(|n| n.world.me_inventory.values().find(|it| it.slot == slot as u8))
                            {
                                // Remember this slot so the Drop / Eat buttons can
                                // act on it after the cursor moves onto them.
                                self.last_inv_slot = Some(slot as u8);
                                lines.push((store.item_name(it.item_id), white));
                                if let Some(def) = store.item_def(it.item_id) {
                                    if let Some(sname) = rcce_data::equip_slot_name(slot as u8) {
                                        lines.push((sname.to_string(), [0.7, 1.0, 0.8, 1.0]));
                                    }
                                    if def.weapon_damage > 0 {
                                        lines.push((format!("Damage: {}", def.weapon_damage), [1.0, 0.7, 0.6, 1.0]));
                                    }
                                    if def.armour_level > 0 {
                                        lines.push((format!("Armour: {}", def.armour_level), [0.7, 0.85, 1.0, 1.0]));
                                    }
                                    if def.mass > 0 {
                                        lines.push((format!("Mass: {}", def.mass), [0.7, 0.7, 0.7, 1.0]));
                                    }
                                }
                                lines.push((format!("Value: {}g", store.item_value(it.item_id)), gold));
                                if it.amount > 1 {
                                    lines.push((format!("Quantity: {}", it.amount), accent));
                                }
                            }
                        }
                    }
                }
                if lines.is_empty() {
                    if let Some(slot) = spell_slot_at(cx, cy, sw, sh) {
                        let mem: Vec<_> = self
                            .sheet
                            .as_ref()
                            .map(|s| s.spells.iter().filter(|sp| sp.memorised).take(12).collect::<Vec<_>>())
                            .unwrap_or_default();
                        if let Some(sp) = mem.get(slot) {
                            lines.push((sp.name.clone(), white));
                            lines.push((format!("Level {} · Recharge {:.1}s", sp.level, sp.recharge as f32 / 1000.0), accent));
                            for chunk in wrap_text(&sp.description, 44).into_iter().take(6) {
                                lines.push((chunk, [0.78, 0.78, 0.78, 1.0]));
                            }
                        }
                    }
                }
                // Hovered-actor world tooltip — only with the panel closed and
                // clear of the function-button row, so it doesn't fight the HUD.
                if lines.is_empty()
                    && !self.show_inventory
                    && function_button_at(cx, cy, sw, sh).is_none()
                {
                    if let Some(net) = self.net.as_ref() {
                        let actors: Vec<(u16, [f32; 3])> = net
                            .world
                            .actors
                            .values()
                            .filter(|a| a.alive)
                            .map(|a| (a.runtime_id, [a.x, a.y + 3.0, a.z]))
                            .collect();
                        if let Some(rid) = actor_at(cx, cy, &actors, &self.vp, sw, sh, 32.0) {
                            if let Some(a) = net.world.actors.get(&rid) {
                                let nm = if a.name.is_empty() { format!("Actor {rid}") } else { a.name.clone() };
                                let col = if a.is_player { [0.5, 0.8, 1.0, 1.0] } else { [1.0, 0.7, 0.6, 1.0] };
                                lines.push((nm, col));
                                if a.health_max > 0 {
                                    lines.push((format!("HP {} / {}", a.health.max(0), a.health_max), accent));
                                }
                            }
                        }
                    }
                }
                if !lines.is_empty() {
                    let (pad, lh) = (5.0f32, 12.0f32);
                    let tw = lines
                        .iter()
                        .map(|(s, _)| rcce_render::font::text_width(s, 1.0))
                        .fold(0.0f32, f32::max);
                    let boxw = tw + pad * 2.0;
                    let boxh = lines.len() as f32 * lh + pad * 2.0;
                    let mut tx = cx + 14.0;
                    let mut ty = cy + 14.0;
                    if tx + boxw > sw {
                        tx = (cx - boxw - 6.0).max(0.0);
                    }
                    if ty + boxh > sh {
                        ty = (sh - boxh).max(0.0);
                    }
                    overlay.rect(tx, ty, boxw, boxh, [0.04, 0.05, 0.09, 0.95]);
                    overlay.rect(tx, ty, boxw, 1.5, [0.4, 0.45, 0.6, 0.9]);
                    for (i, (s, c)) in lines.iter().enumerate() {
                        overlay.text_shadow(tx + pad, ty + pad + i as f32 * lh, 1.0, s, *c);
                    }
                }
            }

            overlay.render(&gfx.device, &gfx.queue, &tview, sw, sh);
        }

        frame.present();

        self.frames += 1;

        // Benchmark mode: after a short warmup, time the next N frames and report
        // the average fps, then exit. Lets perf changes be measured headlessly-ish.
        if let Some(n) = self.bench_target {
            const WARMUP: u64 = 120;
            if self.bench_t0.is_none() && self.frames >= WARMUP {
                self.bench_t0 = Some(Instant::now());
                self.bench_f0 = self.frames;
            }
            if let Some(t0) = self.bench_t0 {
                let measured = self.frames - self.bench_f0;
                if measured >= n {
                    let secs = t0.elapsed().as_secs_f32().max(1e-4);
                    let draws = self.view.as_ref().map(|v| v.drawable_count()).unwrap_or(0);
                    let actors = self.net.as_ref().map(|nn| nn.world.actors.len()).unwrap_or(0);
                    println!(
                        "[bench] avg fps over {measured} frames: {:.1} ({secs:.2}s, {actors} actors, {draws} drawables)",
                        measured as f32 / secs
                    );
                    std::process::exit(0);
                }
            }
        }

        if self.last_log.elapsed().as_secs_f32() >= 2.0 {
            let fps = self.frames as f32 / elapsed.max(0.001);
            let (actors, ups, pos) = self
                .net
                .as_ref()
                .map(|n| (n.world.actors.len(), n.updates, (n.world.me_x, n.world.me_z)))
                .unwrap_or((0, 0, (0.0, 0.0)));
            let draws = self.view.as_ref().map(|v| v.drawable_count()).unwrap_or(0);
            println!(
                "[client-window] frame {} (~{fps:.0} fps), {actors} actor(s), {draws} drawables, {ups} packets, me=({:.1},{:.1})",
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

#[cfg(test)]
mod tests {
    use super::*;

    // The function-button row hit-test must agree with the draw geometry: a
    // click at each button's centre returns that button's action, and the gaps
    // / outside the row return None.
    #[test]
    fn function_button_hit_test() {
        let (sw, sh) = (1280.0f32, 800.0f32);
        let by = FBTN_Y * sh;
        let (bw, bh) = (FBTN_W * sw, FBTN_H * sh);
        let cy = by + bh * 0.5;
        // Centre of each button maps back to its own action, in order.
        let expect = [
            HudAction::Chat, HudAction::Map, HudAction::Inventory, HudAction::Spells,
            HudAction::Character, HudAction::Quests, HudAction::Party, HudAction::Menu,
        ];
        for (idx, (action, fx, _, _)) in FUNCTION_BUTTONS.iter().enumerate() {
            let cx = fx * sw + bw * 0.5;
            let got = function_button_at(cx, cy, sw, sh).expect("button hit");
            assert!(got == *action && got == expect[idx], "button {idx} mismatch");
        }
        // Above the row (well clear of the baseline) → nothing.
        assert!(function_button_at(0.7 * sw, by - bh, sw, sh).is_none());
        // Far left of the cluster (the spell-slot area) → no function button.
        assert!(function_button_at(0.1 * sw, cy, sw, sh).is_none());
        // Just past the last button's right edge → nothing.
        let last_x = FUNCTION_BUTTONS[7].1 * sw + bw + 2.0;
        assert!(function_button_at(last_x, cy, sw, sh).is_none());
    }

    #[test]
    fn compass_strip_marks() {
        use std::f32::consts::PI;
        // Facing N (heading 0): N is dead-centre; E to the right edge, W to the
        // left edge (fov = PI), S is just out of view.
        let m = compass_marks(0.0, PI);
        let n = m.iter().find(|(_, l)| *l == "N").expect("N visible");
        assert!(n.0.abs() < 1e-4, "N should be centred, got {}", n.0);
        let e = m.iter().find(|(_, l)| *l == "E").expect("E visible");
        assert!((e.0 - 0.5).abs() < 1e-4, "E at right edge, got {}", e.0);
        let w = m.iter().find(|(_, l)| *l == "W").expect("W visible");
        assert!((w.0 + 0.5).abs() < 1e-4, "W at left edge, got {}", w.0);
        assert!(m.iter().all(|(_, l)| *l != "S"), "S should be hidden facing N");

        // Turning right (heading +PI/2 = facing E): E becomes centred, N moves to
        // the left edge — the strip scrolls left as you turn right.
        let m = compass_marks(PI * 0.5, PI);
        let e = m.iter().find(|(_, l)| *l == "E").expect("E visible");
        assert!(e.0.abs() < 1e-4, "E centred facing E, got {}", e.0);
        let n = m.iter().find(|(_, l)| *l == "N").expect("N visible");
        assert!((n.0 + 0.5).abs() < 1e-4, "N at left edge, got {}", n.0);
    }

    #[test]
    fn spell_slot_hit_test() {
        let (sw, sh) = (1280.0f32, 800.0f32);
        let by = FBTN_Y * sh;
        let (sw_, sh_) = (FBTN_W * sw, FBTN_H * sh);
        let cy = by + sh_ * 0.5;
        // Centre of each of the 12 slots maps back to its index.
        for i in 0..12usize {
            let cx = (SPELLBAR_X0 + i as f32 * SPELLBAR_PITCH) * sw + sw_ * 0.5;
            assert_eq!(spell_slot_at(cx, cy, sw, sh), Some(i), "slot {i}");
        }
        // Above the bar → none; far right (past slot 11) → none.
        assert_eq!(spell_slot_at(0.2 * sw, by - sh_, sw, sh), None);
        let past = (SPELLBAR_X0 + 12.0 * SPELLBAR_PITCH) * sw + 4.0;
        assert_eq!(spell_slot_at(past, cy, sw, sh), None);
    }

    #[test]
    fn inventory_slot_hit_test() {
        use rcce_data::IComp;
        let (sw, sh) = (1280.0f32, 800.0f32);
        // Mirror the real InventoryWindow + a 2-slot row at the documented
        // window-relative positions (button 0 at 0.035, button 1 at 0.155).
        let iw = IComp { x: 0.25, y: 0.2, w: 0.5, h: 0.55, alpha: 1.0, rgb: [0; 3] };
        let mk = |x: f32, y: f32| IComp { x, y, w: 0.09, h: 0.11, alpha: 1.0, rgb: [0; 3] };
        let buttons = [mk(0.035, 0.02), mk(0.155, 0.02)];
        // Centre of button 1 resolves to index 1.
        let b = &buttons[1];
        let cx = (iw.x + (b.x + b.w * 0.5) * iw.w) * sw;
        let cy = (iw.y + (b.y + b.h * 0.5) * iw.h) * sh;
        assert_eq!(inventory_slot_at(cx, cy, iw, &buttons, sw, sh), Some(1));
        // A point left of the whole window → no slot.
        assert_eq!(inventory_slot_at(0.0, cy, iw, &buttons, sw, sh), None);
    }

    #[test]
    fn inv_action_button_hit_test() {
        use rcce_data::IComp;
        let (sw, sh) = (1280.0f32, 800.0f32);
        let iw = IComp { x: 0.25, y: 0.2, w: 0.5, h: 0.55, alpha: 1.0, rgb: [0; 3] };
        // Real inv_drop / inv_eat window-relative rects from Interface.dat.
        let drop = IComp { x: 0.76, y: 0.93, w: 0.2, h: 0.045, alpha: 1.0, rgb: [0; 3] };
        let eat = IComp { x: 0.5, y: 0.93, w: 0.2, h: 0.045, alpha: 1.0, rgb: [0; 3] };
        let centre = |c: IComp| {
            (
                (iw.x + (c.x + c.w * 0.5) * iw.w) * sw,
                (iw.y + (c.y + c.h * 0.5) * iw.h) * sh,
            )
        };
        let (dx, dy) = centre(drop);
        assert_eq!(inv_action_button_at(dx, dy, iw, drop, eat, sw, sh), Some(InvAction::Drop));
        let (ex, ey) = centre(eat);
        assert_eq!(inv_action_button_at(ex, ey, iw, drop, eat, sw, sh), Some(InvAction::Eat));
        // Centre of the window (the slot grid) hits neither button.
        let cx = (iw.x + 0.5 * iw.w) * sw;
        let cy = (iw.y + 0.4 * iw.h) * sh;
        assert_eq!(inv_action_button_at(cx, cy, iw, drop, eat, sw, sh), None);
    }

    #[test]
    fn actor_pick_nearest() {
        // Identity view-proj: project maps world (x,y,_) → screen
        // ((x*0.5+0.5)*sw, (1-(y*0.5+0.5))*sh), so points are placeable by hand.
        let vp = [
            1.0, 0.0, 0.0, 0.0,
            0.0, 1.0, 0.0, 0.0,
            0.0, 0.0, 1.0, 0.0,
            0.0, 0.0, 0.0, 1.0,
        ];
        let (sw, sh) = (1000.0f32, 1000.0f32);
        // A at world 0,0,0 → screen centre (500,500); B at 0.5,0,0 → x=750.
        let actors = [(10u16, [0.0, 0.0, 0.0]), (20u16, [0.5, 0.0, 0.0])];
        assert_eq!(actor_at(505.0, 500.0, &actors, &vp, sw, sh, 32.0), Some(10));
        assert_eq!(actor_at(745.0, 500.0, &actors, &vp, sw, sh, 32.0), Some(20));
        // Equidistant-ish but closer to B; and a far point → None.
        assert_eq!(actor_at(700.0, 500.0, &actors, &vp, sw, sh, 60.0), Some(20));
        assert_eq!(actor_at(500.0, 100.0, &actors, &vp, sw, sh, 32.0), None);
    }
}
