//! Headless RCCE2 client: logs into the live server and maintains live game
//! state from the packet stream, printing it as it evolves. This is the
//! networking+state spine the wgpu renderer will draw from.
//!
//!   cargo run -p rcce-client --target i686-pc-windows-msvc \
//!       -- "C:\Users\dyanr\Desktop\rcce2\bin\RCEnet.dll" 127.0.0.1 25000 [seconds]

use std::thread::sleep;
use std::time::{Duration, Instant};

use enet_sys::EnetTransport;
use rcce_net::Transport;

use rcce_client::login::{login, Credentials};
use rcce_client::world::World;

fn main() {
    // Args: [host] [port] [seconds]. Transport is the compiled-in ENet fork
    // (enet-sys) — no DLL path needed; this binary is 64-bit.
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25000);
    let run_secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(12);

    let mut t = EnetTransport::new();
    let creds = Credentials {
        username: "rustbot".to_string(),
        password: "rustpass".to_string(),
        email: "rust@bot.com".to_string(),
    };

    println!("[client] logging in to {host}:{port} ...");
    let outcome = match login(&mut t, &host, port, &creds) {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[client] login failed: {e}");
            std::process::exit(1);
        }
    };
    println!("[client] ✓ in world, RuntimeID={}", outcome.runtime_id);

    let mut world = World {
        my_runtime_id: outcome.runtime_id,
        ..Default::default()
    };
    for m in &outcome.world_packets {
        world.apply(m);
    }

    // Live loop: apply packets, print evolving state on a cadence.
    let end = Instant::now() + Duration::from_secs(run_secs);
    let mut last_print = Instant::now() - Duration::from_secs(2);
    let mut chat_seen = 0usize;
    let mut updates = 0u64;

    while Instant::now() < end {
        for m in t.poll() {
            updates += 1;
            world.apply(&m);
        }
        if last_print.elapsed() >= Duration::from_millis(1500) {
            last_print = Instant::now();
            println!(
                "\n[client] zone='{}' (id {}) pvp={} weather={} | me=({:.1}, {:.1}, {:.1}) | {} other actor(s)",
                world.zone.name,
                world.zone.area_id,
                world.zone.pvp,
                world.zone.weather,
                world.me_x,
                world.me_y,
                world.me_z,
                world.actors.len(),
            );
            let mut listed: Vec<_> = world.actors.values().collect();
            listed.sort_by_key(|a| a.runtime_id);
            for a in listed.iter().take(8) {
                let kind = if a.is_player { "player" } else { "npc" };
                let moving = if a.is_running { " running" } else { "" };
                println!(
                    "           #{:<5} {:<14} tmpl={:<3} {:<6} pos=({:.1}, {:.1}){}",
                    a.runtime_id, a.name, a.template_id, kind, a.x, a.z, moving
                );
            }
            while chat_seen < world.chat.len() {
                println!("           chat> {}", world.chat[chat_seen]);
                chat_seen += 1;
            }
        }
        sleep(Duration::from_millis(30));
    }

    println!(
        "\n[client] done — applied {updates} packets. Final: zone '{}', {} actors.",
        world.zone.name,
        world.actors.len()
    );

    t.disconnect(outcome.peer);

    // ---- Render the live world as a real 3D scene (actors as their models) --
    let data_root = std::env::var("RCCE_DATA")
        .unwrap_or_else(|_| r"C:\Users\dyanr\Desktop\rcce2\data".to_string());
    let mut store = match rcce_client::assets::AssetStore::load(&data_root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[client] assets: {e}");
            return;
        }
    };

    // Resolve a model per actor (cached). The local player is the character we
    // created — actor template 0 (Human).
    let mut models: Vec<std::rc::Rc<rcce_data::B3dModel>> = Vec::new();
    let mut textures: Vec<Vec<Option<rcce_data::Image>>> = Vec::new();
    // (model idx, world pos, yaw degrees, color, render scale)
    let mut placements: Vec<(usize, [f32; 3], f32, [f32; 3], f32)> = Vec::new();

    let add_actor = |store: &mut rcce_client::assets::AssetStore,
                     models: &mut Vec<std::rc::Rc<rcce_data::B3dModel>>,
                     textures: &mut Vec<Vec<Option<rcce_data::Image>>>,
                         placements: &mut Vec<(usize, [f32; 3], f32, [f32; 3], f32)>,
                         tmpl: u16,
                         pos: [f32; 3],
                         yaw: f32,
                         color: [f32; 3]| {
        if let Some(m) = store.actor_model(tmpl, 0) {
            let scale = store.actor_render_scale(tmpl, 0).unwrap_or(0.05);
            let tex = store.actor_textures(tmpl, 0);
            let idx = models.len();
            models.push(m);
            textures.push(tex);
            placements.push((idx, pos, yaw, color, scale));
        }
    };

    add_actor(&mut store, &mut models, &mut textures, &mut placements, 0, [world.me_x, world.me_y, world.me_z], world.me_yaw, [0.85, 0.95, 0.85]);
    for a in world.actors.values() {
        let color = if a.is_player { [0.85, 0.9, 1.0] } else { [1.0, 1.0, 1.0] };
        add_actor(&mut store, &mut models, &mut textures, &mut placements, a.template_id, [a.x, a.y, a.z], a.yaw, color);
    }

    if placements.is_empty() {
        eprintln!("[client] no actor models resolved; skipping scene render");
        return;
    }

    // Build instances at the engine's real scale, feet seated on the ground.
    let ground_y = 0.0f32;
    let instances: Vec<rcce_render::SceneInstance> = placements
        .iter()
        .map(|&(idx, pos, yaw, color, scale)| {
            let model: &rcce_data::B3dModel = &models[idx];
            let (min, _max) = model.bounds();
            rcce_render::SceneInstance {
                model,
                textures: &textures[idx],
                translation: [pos[0], ground_y - min[1] * scale, pos[2]],
                yaw: yaw.to_radians(),
                scale,
                color,
            }
        })
        .collect();

    // Third-person camera framing the local player (placement 0).
    let (p_idx, p_pos, _, _, p_scale) = placements[0];
    let (pmin, pmax) = models[p_idx].bounds();
    let player_h = ((pmax[1] - pmin[1]) * p_scale).max(0.5);
    let d = player_h * 3.2;
    let eye = [p_pos[0] + d * 0.65, ground_y + player_h * 2.3, p_pos[2] + d * 0.9];
    let target = [p_pos[0], ground_y + player_h * 0.55, p_pos[2]];

    let out = "rcce_world3d.png";
    match rcce_render::render_scene_png(&instances, eye, target, ground_y, 1200, 900, out) {
        Ok(adapter) => println!(
            "[client] rendered 3D world ({} actors) via {adapter} -> {out}",
            instances.len()
        ),
        Err(e) => eprintln!("[client] scene render failed: {e}"),
    }
}
