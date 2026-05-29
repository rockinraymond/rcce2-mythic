//! Headless RCCE2 client: logs into the live server and maintains live game
//! state from the packet stream, printing it as it evolves. This is the
//! networking+state spine the wgpu renderer will draw from.
//!
//!   cargo run -p rcce-client --target i686-pc-windows-msvc \
//!       -- "C:\Users\dyanr\Desktop\rcce2\bin\RCEnet.dll" 127.0.0.1 25000 [seconds]

use std::thread::sleep;
use std::time::{Duration, Instant};

use rcce_net::Transport;
use rcenet_ffi::FfiTransport;

use rcce_client::login::{login, Credentials};
use rcce_client::world::World;

fn main() {
    let mut args = std::env::args().skip(1);
    let dll = args
        .next()
        .unwrap_or_else(|| r"C:\Users\dyanr\Desktop\rcce2\bin\RCEnet.dll".to_string());
    let host = args.next().unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25000);
    let run_secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(12);

    let mut t = FfiTransport::load(&dll).expect("load RCEnet.dll");
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
}
