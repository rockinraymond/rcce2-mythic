//! Verifies the movement SEND path: client A walks (sends P_StandardUpdate),
//! client B observes. Two accounts, one process.
//!
//! KNOWN BLOCKER — root-caused as far as source allows (re-confirmed live):
//! the server drops A's movement at `FindActorInstanceFromRNID(M\FromID)`
//! (handler ServerNet.bb:1797; lookup Actors.bb:263-266 returns Null when
//! `RNID<1 Or RNID>MaxRNID`, MaxRNID=5000).
//!
//! Exact mechanism (ServerNet.bb P_StartGame 2149-2179): the server keeps TWO
//! ids — `\RNID = M\FromID` (the connection id; the gameplay-attribution key,
//! indexed into `ActorByRNID` ONLY `If M\FromID>0 And <=5000`) and a SEPARATE
//! `\RuntimeID = AssignRuntimeID(...)` (a small counter, the network id that's
//! BROADCAST and which our client reads back as "7"). So our "7" is NOT the
//! attribution key. `M\FromID = RCE_GetMessageConnection() = iSender`, and the
//! vendored RCEnet wrapper sets `iSender = (int)Event.peer` (a heap POINTER,
//! always >5000) — which would make the `<=5000` guard fail and `ActorByRNID`
//! never get set, breaking movement for EVERY client. Since real RealmCrafter
//! games work, bin/Server.exe's actual RCEnet.dll must NOT return the raw
//! pointer (likely `incomingPeerID`, 0..4999) — i.e. it differs from the
//! vendored source. Payload FORMAT is correct (ServerNet.bb:1808-1815).
//!
//! To crack it (needs data this autonomous env can't get): capture the real
//! Client.exe's connect+StartGame+move on UDP 25000 and compare the connection
//! identity, OR temporarily instrument the server to log M\FromID at StartGame
//! vs at P_StandardUpdate. Until then the Rust client is a live SPECTATOR.
//!
//!   cargo run --release -p rcce-client --bin move-test -- [host] [port]

use std::thread::sleep;
use std::time::{Duration, Instant};

use enet_sys::EnetTransport;
use rcce_net::{packet_id as pk, Transport};

use rcce_client::login::{login, Credentials};
use rcce_client::net::movement_packet;
use rcce_client::world::World;

fn creds(user: &str) -> Credentials {
    Credentials {
        username: user.to_string(),
        password: "rustpass".to_string(),
        email: "rust@bot.com".to_string(),
    }
}

/// Pump a transport into its world for `ms`.
fn settle(t: &mut EnetTransport, w: &mut World, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        for m in t.poll() {
            w.apply(&m);
        }
        sleep(Duration::from_millis(20));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25000);

    // Client A.
    let mut ta = EnetTransport::new();
    let a = login(&mut ta, &host, port, &creds("rustbot")).expect("A login");
    let mut wa = World { my_runtime_id: a.runtime_id, ..Default::default() };
    for m in &a.world_packets {
        wa.apply(m);
    }
    settle(&mut ta, &mut wa, 800);
    println!(
        "[move] A rnid={} pos=({:.1}, {:.1}, {:.1})",
        a.runtime_id, wa.me_x, wa.me_y, wa.me_z
    );

    // Client B (separate account) — observer.
    let mut tb = EnetTransport::new();
    let b = login(&mut tb, &host, port, &creds("rustbot2")).expect("B login");
    let mut wb = World { my_runtime_id: b.runtime_id, ..Default::default() };
    for m in &b.world_packets {
        wb.apply(m);
    }
    settle(&mut tb, &mut wb, 1200); // let B receive A's P_NewActor
    let a_rnid = a.runtime_id;
    let b_start = wb.actors.get(&a_rnid).map(|x| (x.x, x.z));
    let dump = |w: &World, label: &str| {
        let mut v: Vec<_> = w.actors.values().collect();
        v.sort_by_key(|x| x.runtime_id);
        for act in v {
            println!(
                "[move]   {label}: rnid {} '{}' pos=({:.1},{:.1})",
                act.runtime_id, act.name, act.x, act.z
            );
        }
    };
    println!(
        "[move] B rnid={} sees {} actor(s); A(rnid {a_rnid}) start = {:?}",
        b.runtime_id,
        wb.actors.len(),
        b_start
    );
    dump(&wb, "B start");

    // A walks in +X, sending movement each tick; B keeps observing.
    let (mut ax, az, ay) = (wa.me_x, wa.me_z, wa.me_y);
    let start_ax = ax;
    // Steps under the server's 2.0-unit per-packet clamp floor are always
    // accepted; larger steps get rejected (held) when packets bunch up.
    for _ in 0..40 {
        ax += 1.6;
        let p = movement_packet(ax + 15.0, az, ay, ax, az, true, false);
        // Reliable + immediate service to flush. Do NOT locally set wa.me_x:
        // the server echoes the player's own authoritative position back, so
        // wa.me_x reflects what the server accepted.
        ta.send(a.peer, pk::STANDARD_UPDATE, &p, true);
        for m in ta.poll() {
            wa.apply(&m);
        }
        sleep(Duration::from_millis(140));
        for m in ta.poll() {
            wa.apply(&m);
        }
        for m in tb.poll() {
            wb.apply(&m);
        }
    }
    settle(&mut ta, &mut wa, 400);
    settle(&mut tb, &mut wb, 600);
    println!(
        "[move] server-authoritative A position (from A's own echo): me_x={:.1} (claimed up to {:.1})",
        wa.me_x, ax
    );

    let b_end = wb.actors.get(&a_rnid).map(|x| (x.x, x.z));
    println!("[move] A claimed-walk X {:.1} -> {:.1}", start_ax, ax);
    dump(&wb, "B end  ");
    println!("[move] B sees A end = {:?}", b_end);

    ta.disconnect(a.peer);
    tb.disconnect(b.peer);

    match (b_start, b_end) {
        (Some((sx, _)), Some((ex, _))) => {
            let dx = ex - sx;
            println!("[move] B observed A move dX = {:.1}", dx);
            if dx.abs() > 5.0 {
                println!("[move] RESULT: PASS — the server accepted A's movement and broadcast it to B.");
            } else {
                eprintln!("[move] RESULT: FAIL — A did not visibly move to B.");
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("[move] RESULT: FAIL — B never saw actor A (rnid {a_rnid}).");
            std::process::exit(1);
        }
    }
}
