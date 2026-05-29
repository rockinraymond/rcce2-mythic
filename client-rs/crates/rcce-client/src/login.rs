//! Reusable login flow (generic over [`Transport`]): account create/verify,
//! character create, then a fresh connection for `P_StartGame`. Mirrors the
//! reference client's two-connection menu→game handshake. World packets that
//! arrive during the StartGame wait are buffered and returned for the caller to
//! apply to its [`World`](crate::world::World).

use std::thread::sleep;
use std::time::{Duration, Instant};

use rcce_net::auth::{encrypt_email, md5_hex};
use rcce_net::codec::{MsgReader, MsgWriter};
use rcce_net::{packet_id as pk, RecvMessage, Transport};

pub struct Credentials {
    pub username: String,
    pub password: String,
    pub email: String,
}

pub struct LoginOutcome {
    /// Server-assigned runtime id for the local player.
    pub runtime_id: u16,
    /// The live (game) connection handle — keep it for sending.
    pub peer: i32,
    /// World packets received during the StartGame wait (apply these first).
    pub world_packets: Vec<RecvMessage>,
}

fn pump<T: Transport>(t: &mut T, ms: u64) -> Vec<RecvMessage> {
    let mut all = Vec::new();
    let deadline = Instant::now() + Duration::from_millis(ms);
    while Instant::now() < deadline {
        all.extend(t.poll());
        sleep(Duration::from_millis(15));
    }
    all
}

fn sentinel(m: &RecvMessage) -> char {
    m.data.first().map(|&b| b as char).unwrap_or('\0')
}

/// Run the full login flow against `host:port`. On success the player is in the
/// world with a runtime id; the returned `peer` is the live connection.
pub fn login<T: Transport>(
    t: &mut T,
    host: &str,
    port: u16,
    c: &Credentials,
) -> Result<LoginOutcome, String> {
    let md5 = md5_hex(&c.password);

    // ---- Connection #1: account + character ----
    let peer = t.connect(host, port).map_err(|e| format!("conn#1: {e}"))?;

    // CreateAccount (idempotent: "N" just means it already exists).
    let mut w = MsgWriter::new();
    w.str8(&c.username)
        .str8(&md5)
        .u8(c.email.len() as u8)
        .raw(&encrypt_email(&c.email));
    t.send(peer, pk::CREATE_ACCOUNT, w.as_slice(), true);
    let _ = pump(t, 1200);

    // VerifyAccount → 'Y' + packed character records.
    let mut w = MsgWriter::new();
    w.str8(&c.username).str8(&md5);
    t.send(peer, pk::VERIFY_ACCOUNT, w.as_slice(), true);
    let resp = pump(t, 1500);
    let m = resp
        .into_iter()
        .find(|m| matches!(sentinel(m), 'Y' | 'P' | 'N' | 'B' | 'L'))
        .ok_or("no VerifyAccount response")?;
    let char_count = match sentinel(&m) {
        'Y' => {
            let mut r = MsgReader::new(&m.data[1..]);
            let mut n = 0;
            while r.remaining() > 0 {
                if r.str8().is_none() {
                    break;
                }
                let (actor, gender) = (r.u16(), r.u8());
                let _appearance = (r.u8(), r.u8(), r.u8(), r.u8());
                if actor.is_none() || gender.is_none() {
                    break;
                }
                n += 1;
            }
            n
        }
        c => return Err(format!("VerifyAccount failed (sentinel '{c}')")),
    };

    // CreateCharacter if the account has none. Always send the 40-byte attribute
    // block — the server reads the name at a hardcoded Offset+47.
    if char_count == 0 {
        let mut created = false;
        'actors: for actor_id in 0u16..=20 {
            for suffix in 0..3 {
                let name = if suffix == 0 {
                    "Rustaroo".to_string()
                } else {
                    format!("Rustaroo{suffix}")
                };
                let mut w = MsgWriter::new();
                w.str8(&c.username).str8(&md5);
                w.u16(actor_id).u8(0).u8(0).u8(0).u8(0).u8(0);
                w.raw(&[0u8; 40]);
                w.raw(name.as_bytes());
                t.send(peer, pk::CREATE_CHARACTER, w.as_slice(), true);
                match pump(t, 1500)
                    .iter()
                    .find(|m| matches!(sentinel(m), 'Y' | 'I' | 'N'))
                    .map(sentinel)
                {
                    Some('Y') => {
                        created = true;
                        break 'actors;
                    }
                    Some('I') => continue,
                    Some('N') => break,
                    _ => {}
                }
            }
        }
        if !created {
            return Err("could not create a character".into());
        }
    }

    t.disconnect(peer);
    sleep(Duration::from_millis(300));

    // ---- Connection #2: enter the world ----
    let peer2 = t.connect(host, port).map_err(|e| format!("conn#2: {e}"))?;
    let mut w = MsgWriter::new();
    w.str8(&c.username).str8(&md5).u8(0); // character index 0
    t.send(peer2, pk::START_GAME, w.as_slice(), true);

    let mut runtime_id = 0u16;
    let mut replies = 0;
    let mut world_packets = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && replies < 13 {
        for m in t.poll() {
            if m.msg_type == pk::START_GAME {
                if sentinel(&m) == 'N' && m.data.len() == 1 {
                    return Err("StartGame rejected ('N')".into());
                }
                if m.data.len() <= 2 {
                    runtime_id = MsgReader::new(&m.data).u16().unwrap_or(0);
                }
                replies += 1;
            } else {
                world_packets.push(m);
            }
        }
        sleep(Duration::from_millis(20));
    }
    if runtime_id == 0 {
        return Err("no RuntimeID assigned".into());
    }
    Ok(LoginOutcome {
        runtime_id,
        peer: peer2,
        world_packets,
    })
}
