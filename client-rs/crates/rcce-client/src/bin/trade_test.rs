//! Verifies the player↔player trade path end-to-end against a live server.
//! Two accounts (A=rustbot, B=rustbot2) in the same zone:
//!   1. A sends `/trade <Bname>` (chat) → server binds the trade, invites B.
//!   2. B sends `/trade` → both accepted → server sends both `P_OpenTrading "11P"`.
//!   3. Both clients now hold a `player_trade` board (the HARD assertion).
//!   4. A stages a backpack offer (`P_UpdateTrading`) → B's `player_trade.his`
//!      populates (verified when A's character actually has a backpack item).
//!
//! Relies on the gameplay-attribution fix (PR #462) and the Phase 1A/1B trade work.
//!
//!   cargo run -p rcce-client --bin trade-test --release -- [host] [port]

use std::thread::sleep;
use std::time::{Duration, Instant};

use enet_sys::EnetTransport;
use rcce_net::{packet_id as pk, Transport};

use rcce_client::login::{login, Credentials};
use rcce_client::net::{inv_move_packet, trade_offer_packet};
use rcce_client::world::World;

fn creds(user: &str) -> Credentials {
    Credentials { username: user.to_string(), password: "rustpass".to_string(), email: "rust@bot.com".to_string() }
}

fn settle(t: &mut EnetTransport, w: &mut World, ms: u64) {
    let end = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < end {
        for m in t.poll() {
            w.apply(&m);
        }
        sleep(Duration::from_millis(20));
    }
}

fn chat(t: &mut EnetTransport, w: &mut World, peer: i32, msg: &str) {
    t.send(peer, pk::CHAT_MESSAGE, msg.as_bytes(), true);
    // Service the connection to flush the send, applying anything that arrives
    // (e.g. the server's immediate P_OpenTrading "11P" reply to an accept) — do
    // NOT discard it, or the trade-open packet is lost.
    for _ in 0..3 {
        for m in t.poll() {
            w.apply(&m);
        }
        sleep(Duration::from_millis(60));
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let host = args.next().unwrap_or_else(|| "127.0.0.1".to_string());
    let port: u16 = args.next().and_then(|s| s.parse().ok()).unwrap_or(25000);

    // Log in both bots into the world.
    let mut ta = EnetTransport::new();
    let a = login(&mut ta, &host, port, &creds("rustbot")).expect("A login");
    let mut wa = World { my_runtime_id: a.runtime_id, ..Default::default() };
    for m in &a.world_packets { wa.apply(m); }
    settle(&mut ta, &mut wa, 600);

    let mut tb = EnetTransport::new();
    let b = login(&mut tb, &host, port, &creds("rustbot2")).expect("B login");
    let mut wb = World { my_runtime_id: b.runtime_id, ..Default::default() };
    for m in &b.world_packets { wb.apply(m); }
    settle(&mut tb, &mut wb, 800);
    // Let A receive B's spawn so A can resolve B's character name.
    settle(&mut ta, &mut wa, 800);
    println!("[trade] A rnid={} B rnid={}", a.runtime_id, b.runtime_id);

    let Some(bname) = wa.actors.get(&b.runtime_id).map(|act| act.name.clone()).filter(|n| !n.is_empty()) else {
        eprintln!("[trade] FAIL — A cannot see B's actor/name (actors A sees: {:?})",
            wa.actors.values().map(|x| (x.runtime_id, x.name.clone())).collect::<Vec<_>>());
        std::process::exit(1);
    };
    println!("[trade] A sees B as '{bname}'");

    // 1. A initiates the trade with B by character name.
    chat(&mut ta, &mut wa, a.peer, &format!("/trade {bname}"));
    settle(&mut tb, &mut wb, 800); // B receives the invite
    settle(&mut ta, &mut wa, 200);

    // 2. B accepts (a bare `/trade` while in the invited state).
    chat(&mut tb, &mut wb, b.peer, "/trade");
    settle(&mut ta, &mut wa, 900); // both receive P_OpenTrading "11P"
    settle(&mut tb, &mut wb, 600);

    let a_open = wa.player_trade.is_some();
    let b_open = wb.player_trade.is_some();
    println!("[trade] after accept: A player_trade={a_open} B player_trade={b_open}");
    if !a_open || !b_open {
        eprintln!("[trade] FAIL — the trade window did not open on both sides (handshake).");
        ta.disconnect(a.peer);
        tb.disconnect(b.peer);
        std::process::exit(1);
    }

    // 3. Give A something to offer. The wire offer slot is BACKPACK-RELATIVE
    //    (0..31 = absolute 14..45). If A has no backpack item, move an equipped
    //    one into backpack slot 14 first.
    let inv: Vec<(u8, u16)> = a.sheet.as_ref().map(|s| s.inventory.iter().map(|it| (it.slot, it.item_id)).collect()).unwrap_or_default();
    println!("[trade] A inventory (slot,item): {inv:?}");
    let has_backpack = inv.iter().any(|&(slot, _)| slot >= 14);
    let mut tried_offer = has_backpack;
    if !has_backpack {
        if let Some(&(eqslot, _)) = inv.iter().find(|&&(slot, _)| slot < 14) {
            println!("[trade] A has no backpack item; moving equipped slot {eqslot} -> backpack slot 14");
            ta.send(a.peer, pk::INVENTORY_UPDATE, &inv_move_packet(a.runtime_id, eqslot, 14, 0, false), true);
            settle(&mut ta, &mut wa, 700);
            tried_offer = true;
        }
    }

    // Offer backpack-RELATIVE slots 0..=7 (absolute 14..=21). The server forwards
    // each one A actually owns to B as an inbound P_UpdateTrading.
    for rel in 0u8..=7 {
        ta.send(a.peer, pk::UPDATE_TRADING, &trade_offer_packet(rel, 1), true);
        for _ in 0..2 { ta.poll(); sleep(Duration::from_millis(40)); }
    }
    settle(&mut tb, &mut wb, 1400);

    let his = wb.player_trade.as_ref().map(|pt| pt.his.len()).unwrap_or(0);
    println!("[trade] B sees {his} offered item(s) from A: {:?}",
        wb.player_trade.as_ref().map(|pt| pt.his.iter().map(|o| (o.item_id, o.amount)).collect::<Vec<_>>()));

    ta.disconnect(a.peer);
    tb.disconnect(b.peer);

    // The handshake (both boards open) is the hard pass. When A had an item to
    // offer, B must observe it — that proves the backpack-relative offer slot is
    // correct end-to-end (the bug where an absolute slot was sent showed his==0).
    if tried_offer && his == 0 {
        eprintln!("[trade] RESULT: FAIL — A offered an item but B saw none (offer slot space wrong?).");
        std::process::exit(1);
    }
    println!("[trade] RESULT: PASS — handshake opened both windows{}.",
        if his > 0 { format!("; B saw {his} offered item(s) — relative offer slot verified E2E") } else { " (A had no item to offer)".to_string() });
}
