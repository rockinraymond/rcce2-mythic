//! Adversarial / malformed-input acceptance suite for the `rcce-client` WIRE
//! parsers — the packet handlers and the login/character/trade decoders that
//! consume bytes straight off the server connection.
//!
//! Core invariant (repo `CLAUDE.md`, "Soft-fail on server-controlled data"):
//! **parsing a malformed or hostile packet must NEVER panic.** A panic in a
//! packet handler is a full client crash — and the server (or a man-in-the-
//! middle) can send any bytes. Every handler / decoder must, for garbage /
//! truncated / length-bomb input, soft-fail (drop the packet, return
//! `None`/`Err`/a partial value) instead of panicking, indexing out of bounds,
//! `unwrap()`-ing a short read, or allocating on an attacker-chosen count.
//!
//! The companion suites in `rcce-data` / `rcce-net` (PR #466) cover the on-disk
//! asset parsers and the wire codec; this closes the remaining gap — the
//! client's own consumers: `World::apply` (the whole `P_*` dispatch),
//! `CharacterSheet::apply_packet`, `parse_char_list`, and the player-trade
//! parsers. All inputs are synthetic; the suite needs no `data/` files.

use std::panic::catch_unwind;

use rcce_client::fetch::CharacterSheet;
use rcce_client::login::parse_char_list;
use rcce_client::trade::{PlayerTrade, TradeWindow};
use rcce_client::world::World;
use rcce_net::{packet_id as pk, RecvMessage};

/// A bundle of generically-hostile byte slices every consumer must survive,
/// including length-prefix "bombs" (a `u16`/`u32` count claiming far more bytes
/// than follow) — the classic shape that crashes a parser which trusts the
/// length and allocates / indexes on it.
fn garbage_corpus() -> Vec<(&'static str, Vec<u8>)> {
    let mut v = vec![
        ("empty", vec![]),
        ("one_byte", vec![0x00]),
        ("one_byte_hi", vec![0xFF]),
        ("two_bytes", vec![0xFF, 0x00]),
        ("ff_8", vec![0xFF; 8]),
        ("zero_8", vec![0x00; 8]),
        ("ff_64", vec![0xFF; 64]),
        ("zero_64", vec![0x00; 64]),
        ("ascii_64", vec![b'A'; 64]),
        ("ff_512", vec![0xFF; 512]),
        ("alt_256", (0..256).map(|i| (i & 0xFF) as u8).collect()),
        // str16 length-bomb: claims a 0xFFFF-byte string, supplies 2 bytes.
        ("str16_bomb", vec![0xFF, 0xFF, b'x', b'y']),
        // all empty-slot sentinels (99) — parse_inventory `continue`s on each;
        // must terminate, not spin.
        ("sentinel_run", vec![0x63; 256]),
    ];
    // A leading control byte ('C','S','F','N','A','M','R','U','D','P', …) over a
    // garbage tail exercises the sub-typed handlers (stat/inventory/spell/trade).
    for lead in [b'C', b'S', b'F', b'N', b'A', b'M', b'R', b'U', b'D', b'P', b'1', b'3'] {
        let mut b = vec![lead];
        b.extend_from_slice(&[0xFF; 48]);
        v.push(("lead_garbage", b));
        let mut b2 = vec![lead, b'1'];
        b2.extend_from_slice(&[0xFF; 48]);
        v.push(("lead2_garbage", b2));
    }
    v
}

#[test]
fn world_apply_never_panics_on_any_packet() {
    // Fuzz EVERY packet type (0..=255, covering all P_* ids + the unknown-id
    // `_ => {}` path) with every hostile body, on a fresh world AND on a world
    // with the local-player id set (so handlers that branch on `my_runtime_id`
    // and look up actors/self take more of their code paths).
    for msg_type in 0u8..=255 {
        for (name, body) in garbage_corpus() {
            for seeded in [false, true] {
                let result = catch_unwind(|| {
                    let mut w = if seeded {
                        World { my_runtime_id: 7, ..Default::default() }
                    } else {
                        World::default()
                    };
                    // Apply twice — some handlers mutate state the next apply reads.
                    let m = RecvMessage { msg_type, connection: 0, data: body.clone() };
                    w.apply(&m);
                    w.apply(&m);
                });
                assert!(
                    result.is_ok(),
                    "World::apply panicked on packet type {msg_type} body '{name}' (seeded={seeded})"
                );
            }
        }
    }
}

#[test]
fn world_apply_open_trade_then_garbage() {
    // Open a player trade, then feed garbage UPDATE/CLOSE/OPEN trading packets —
    // the trade handlers deref `player_trade`/`current_trade`, so exercise them
    // with a board present.
    for (name, body) in garbage_corpus() {
        let result = catch_unwind(|| {
            let mut w = World { my_runtime_id: 7, ..Default::default() };
            w.apply(&RecvMessage { msg_type: pk::OPEN_TRADING, connection: 0, data: vec![b'P'] });
            for t in [pk::UPDATE_TRADING, pk::OPEN_TRADING, pk::CLOSE_TRADING, pk::INVENTORY_UPDATE, pk::STAT_UPDATE] {
                w.apply(&RecvMessage { msg_type: t, connection: 0, data: body.clone() });
            }
        });
        assert!(result.is_ok(), "trade-handler path panicked on body '{name}'");
    }
}

#[test]
fn character_sheet_apply_packet_never_panics() {
    // The login character fetch (P_FetchCharacter): 'C1' stats, 'C3' inventory,
    // 'S' spells (str16 name/description — length-bomb territory). Try the raw
    // corpus AND each sub-type prefix over a garbage tail.
    for (name, body) in garbage_corpus() {
        for prefix in [vec![], vec![b'C', b'1'], vec![b'C', b'3'], vec![b'S'], vec![b'F'], vec![b'Q']] {
            let mut data = prefix.clone();
            data.extend_from_slice(&body);
            let result = catch_unwind(|| {
                let mut s = CharacterSheet::default();
                s.apply_packet(&data);
            });
            assert!(
                result.is_ok(),
                "CharacterSheet::apply_packet panicked on prefix {prefix:?} + '{name}'"
            );
        }
    }
}

#[test]
fn standalone_parsers_never_panic() {
    for (name, body) in garbage_corpus() {
        // Login char-list (P_FetchAccount): variable-length char records.
        let r = catch_unwind(|| {
            let _ = parse_char_list(&body);
        });
        assert!(r.is_ok(), "parse_char_list panicked on '{name}'");

        // Trade window open (P_OpenTrading): N/S/P kind + offer records.
        let r = catch_unwind(|| {
            let _ = TradeWindow::parse(&body);
        });
        assert!(r.is_ok(), "TradeWindow::parse panicked on '{name}'");

        // Player-trade inbound offer (P_UpdateTrading): slot + amount + item.
        let r = catch_unwind(|| {
            let mut pt = PlayerTrade::default();
            pt.apply_his_update(&body);
        });
        assert!(r.is_ok(), "PlayerTrade::apply_his_update panicked on '{name}'");
    }
}
