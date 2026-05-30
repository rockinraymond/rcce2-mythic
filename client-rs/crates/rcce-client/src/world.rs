//! Live game-state model, fed by the server packet stream.
//!
//! Packet payload layouts decoded from the reference client's parse code
//! (`ClientNet.bb`) and the server serializer (`Actors.bb::ActorInstanceToString`).
//! All multi-byte fields are **big-endian** (handled by `MsgReader`).

use std::collections::HashMap;

use rcce_net::codec::MsgReader;
use rcce_net::{packet_id as pk, RecvMessage};

/// One actor instance in the current zone (player or NPC).
#[derive(Debug, Clone, Default)]
pub struct Actor {
    pub runtime_id: u16,
    pub template_id: u16,
    pub name: String,
    pub is_player: bool,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub dest_x: f32,
    pub dest_z: f32,
    pub is_running: bool,
    pub walk_back: bool,
    pub mount_id: u16,
    pub alive: bool,
    /// Attribute index → (value, maximum), as delivered by P_StatUpdate.
    /// Sparse — only attributes the server has sent. Health/Energy/etc. indices
    /// come from Fixed Attributes.dat (the caller maps them).
    pub attributes: HashMap<u8, (i16, i16)>,
}

/// Current zone metadata (from `P_ChangeArea`).
#[derive(Debug, Default, Clone)]
pub struct Zone {
    pub area_id: u32,
    pub name: String,
    pub pvp: bool,
    pub gravity_raw: u16,
    pub weather: u8,
}

/// Everything the client knows about the running game.
#[derive(Debug, Default)]
pub struct World {
    pub my_runtime_id: u16,
    pub me_x: f32,
    pub me_y: f32,
    pub me_z: f32,
    pub me_yaw: f32,
    pub zone: Zone,
    /// Other actors keyed by runtime id (excludes the local player).
    pub actors: HashMap<u16, Actor>,
    /// Recent chat lines (control-byte channel prefixes stripped).
    pub chat: Vec<String>,
    // Local player progression / stats.
    pub me_xp: i32,
    pub me_xp_bar: u8,
    pub me_gold: i32,
    pub me_attributes: HashMap<u8, (i16, i16)>,
}

impl World {
    /// Apply one received message, mutating state. Unknown types are ignored.
    pub fn apply(&mut self, m: &RecvMessage) {
        match m.msg_type {
            pk::CHANGE_AREA => self.on_change_area(&m.data),
            pk::NEW_ACTOR => self.on_new_actor(&m.data),
            pk::STANDARD_UPDATE => self.on_standard_update(&m.data),
            pk::ACTOR_GONE => self.on_actor_gone(&m.data),
            pk::CHAT_MESSAGE => self.on_chat(&m.data),
            pk::XP_UPDATE => self.on_xp_update(&m.data),
            pk::GOLD_CHANGE => self.on_gold_change(&m.data),
            pk::STAT_UPDATE => self.on_stat_update(&m.data),
            pk::ACTOR_DEAD => self.on_actor_dead(&m.data),
            _ => {}
        }
    }

    /// `P_ChangeArea` (ClientNet.bb:1627): X,Y,Z,Yaw f32 · PvP u8 · Gravity u16
    /// · AreaID u32 · Weather u8 · nameLen u8 · name.
    fn on_change_area(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        self.me_x = r.f32().unwrap_or(0.0);
        self.me_y = r.f32().unwrap_or(0.0);
        self.me_z = r.f32().unwrap_or(0.0);
        self.me_yaw = r.f32().unwrap_or(0.0);
        let pvp = r.u8().unwrap_or(0) != 0;
        let gravity_raw = r.u16().unwrap_or(200);
        let area_id = r.u32().unwrap_or(0);
        let weather = r.u8().unwrap_or(0);
        let name = r.str8().unwrap_or_default();
        // A zone change invalidates the old actor set (the server re-announces
        // everyone via P_NewActor for the new zone).
        self.actors.clear();
        self.zone = Zone {
            area_id,
            name,
            pvp,
            gravity_raw,
            weather,
        };
    }

    /// `P_NewActor` = `ActorInstanceToString` (Actors.bb:1076): ServerArea u32 ·
    /// RuntimeID u16 · Level u16 · XP u32 · TemplateID u16 · X,Y,Z,Yaw f32 ·
    /// isPlayer u8 · nameLen u8 · name · (more, ignored).
    fn on_new_actor(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        let _server_area = r.u32();
        let Some(runtime_id) = r.u16() else { return };
        let _level = r.u16();
        let _xp = r.u32();
        let template_id = r.u16().unwrap_or(0);
        let x = r.f32().unwrap_or(0.0);
        let y = r.f32().unwrap_or(0.0);
        let z = r.f32().unwrap_or(0.0);
        let yaw = r.f32().unwrap_or(0.0);
        let is_player = r.u8().unwrap_or(0) != 0;
        let name = r.str8().unwrap_or_default();
        if runtime_id == self.my_runtime_id {
            self.me_x = x;
            self.me_y = y;
            self.me_z = z;
            self.me_yaw = yaw;
            return; // don't list ourselves among "other actors"
        }
        self.actors.insert(
            runtime_id,
            Actor {
                runtime_id,
                template_id,
                name,
                is_player,
                x,
                y,
                z,
                yaw,
                dest_x: x,
                dest_z: z,
                alive: true,
                ..Default::default()
            },
        );
    }

    /// `P_StandardUpdate` (ClientNet.bb:1490): RuntimeID u16 · X f32 · Z f32 ·
    /// IsRunning u8 · WalkBack u8 · DestX f32 · DestZ f32 · Mount u16. (22 bytes
    /// for ground actors; flying actors append a Y f32.)
    fn on_standard_update(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        let Some(rid) = r.u16() else { return };
        let x = r.f32().unwrap_or(0.0);
        let z = r.f32().unwrap_or(0.0);
        let is_running = r.u8().unwrap_or(0) != 0;
        let walk_back = r.u8().unwrap_or(0) != 0;
        let dest_x = r.f32().unwrap_or(x);
        let dest_z = r.f32().unwrap_or(z);
        let mount_id = r.u16().unwrap_or(0);
        if rid == self.my_runtime_id {
            self.me_x = x;
            self.me_z = z;
        }
        if let Some(a) = self.actors.get_mut(&rid) {
            a.x = x;
            a.z = z;
            a.is_running = is_running;
            a.walk_back = walk_back;
            a.dest_x = dest_x;
            a.dest_z = dest_z;
            a.mount_id = mount_id;
        }
    }

    /// `P_ActorGone`: a runtime id that has left the zone.
    fn on_actor_gone(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        if let Some(rid) = r.u16() {
            self.actors.remove(&rid);
        }
    }

    /// `P_XPUpdate` (ClientNet.bb:689): `'B'`+barLevel(u8), or `'M'`+xpGain(i32).
    fn on_xp_update(&mut self, d: &[u8]) {
        match d.first() {
            Some(b'B') => {
                if let Some(&bar) = d.get(1) {
                    self.me_xp_bar = bar;
                }
            }
            Some(b'M') => {
                if let Some(gain) = MsgReader::new(&d[1..]).i32() {
                    self.me_xp = self.me_xp.saturating_add(gain);
                }
            }
            _ => {}
        }
    }

    /// `P_GoldChange` (ClientNet.bb:947): byte0 `'D'`=decrease (else increase),
    /// then amount(i32). Clamped at 0.
    fn on_gold_change(&mut self, d: &[u8]) {
        let decrease = d.first() == Some(&b'D');
        if let Some(amount) = MsgReader::new(&d[1.min(d.len())..]).i32() {
            let delta = if decrease { -amount } else { amount };
            self.me_gold = (self.me_gold + delta).max(0);
        }
    }

    /// `P_StatUpdate` (ClientNet.bb:996): byte0 `'A'`(value)/`'M'`(max) +
    /// RuntimeID(u16) + attrIndex(u8) + value(u16). (`'R'` resistances ignored.)
    fn on_stat_update(&mut self, d: &[u8]) {
        let kind = match d.first() {
            Some(&k) => k,
            None => return,
        };
        let mut r = MsgReader::new(&d[1..]);
        let (Some(rid), Some(attr), Some(val)) = (r.u16(), r.u8(), r.u16()) else {
            return;
        };
        if attr >= 40 {
            return;
        }
        let val = val as i16;
        let table = if rid == self.my_runtime_id {
            &mut self.me_attributes
        } else if let Some(a) = self.actors.get_mut(&rid) {
            &mut a.attributes
        } else {
            return;
        };
        let entry = table.entry(attr).or_default();
        match kind {
            b'A' => entry.0 = val, // current value
            b'M' => entry.1 = val, // maximum
            _ => {}
        }
    }

    /// `P_ActorDead` (ClientNet.bb:1071): RuntimeID(u16) of the actor that died.
    fn on_actor_dead(&mut self, d: &[u8]) {
        if let Some(rid) = MsgReader::new(d).u16() {
            if let Some(a) = self.actors.get_mut(&rid) {
                a.alive = false;
            }
        }
    }

    /// `P_ChatMessage`: a leading control byte (channel, e.g. 253/254) then text.
    fn on_chat(&mut self, d: &[u8]) {
        let text: String = d
            .iter()
            .filter(|&&b| b >= 32)
            .map(|&b| b as char)
            .collect();
        if !text.trim().is_empty() {
            self.chat.push(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcce_net::codec::MsgWriter;

    fn msg(t: u8, payload: Vec<u8>) -> RecvMessage {
        RecvMessage {
            msg_type: t,
            connection: 0,
            data: payload,
        }
    }

    #[test]
    fn change_area_then_new_actor_then_update() {
        let mut w = World {
            my_runtime_id: 1792,
            ..Default::default()
        };

        // P_ChangeArea
        let mut p = MsgWriter::new();
        p.f32(10.0).f32(0.0).f32(20.0).f32(1.5); // x y z yaw
        p.u8(0).u16(200).u32(7).u8(0).str8("Plains"); // pvp grav areaid weather name
        w.apply(&msg(pk::CHANGE_AREA, p.into_bytes()));
        assert_eq!(w.zone.name, "Plains");
        assert_eq!(w.zone.area_id, 7);
        assert!((w.me_x - 10.0).abs() < 0.01);

        // P_NewActor (an NPC, runtime id 50)
        let mut p = MsgWriter::new();
        p.u32(7).u16(50).u16(1).u32(0).u16(3); // area rid level xp tmpl
        p.f32(15.0).f32(0.0).f32(25.0).f32(0.0); // x y z yaw
        p.u8(0).str8("Stag"); // isPlayer name
        w.apply(&msg(pk::NEW_ACTOR, p.into_bytes()));
        assert_eq!(w.actors.len(), 1);
        assert_eq!(w.actors[&50].name, "Stag");
        assert!(!w.actors[&50].is_player);

        // P_StandardUpdate moves the NPC
        let mut p = MsgWriter::new();
        p.u16(50).f32(16.0).f32(26.0).u8(1).u8(0).f32(18.0).f32(28.0).u16(0);
        w.apply(&msg(pk::STANDARD_UPDATE, p.into_bytes()));
        assert!((w.actors[&50].x - 16.0).abs() < 0.01);
        assert!(w.actors[&50].is_running);

        // P_ActorGone removes it
        let mut p = MsgWriter::new();
        p.u16(50);
        w.apply(&msg(pk::ACTOR_GONE, p.into_bytes()));
        assert!(w.actors.is_empty());
    }

    /// Build a payload (the builders return `&mut Self`, so chaining
    /// `.into_bytes()` doesn't work — wrap in an owned writer).
    fn pkt(build: impl FnOnce(&mut MsgWriter)) -> Vec<u8> {
        let mut w = MsgWriter::new();
        build(&mut w);
        w.into_bytes()
    }

    #[test]
    fn xp_gold_stat_dead() {
        let mut w = World {
            my_runtime_id: 7,
            ..Default::default()
        };
        // Register an NPC (rnid 50).
        w.apply(&msg(
            pk::NEW_ACTOR,
            pkt(|p| {
                p.u32(0).u16(50).u16(1).u32(0).u16(3);
                p.f32(0.0).f32(0.0).f32(0.0).f32(0.0);
                p.u8(0).str8("Stag");
            }),
        ));
        assert!(w.actors[&50].alive);

        // XP: 'B' bar level, then 'M' xp gain.
        w.apply(&msg(pk::XP_UPDATE, pkt(|p| { p.u8(b'B').u8(4); })));
        assert_eq!(w.me_xp_bar, 4);
        w.apply(&msg(pk::XP_UPDATE, pkt(|p| { p.u8(b'M').i32(150); })));
        assert_eq!(w.me_xp, 150);

        // Gold: increase then decrease, clamped at 0.
        w.apply(&msg(pk::GOLD_CHANGE, pkt(|p| { p.u8(b'I').i32(100); })));
        assert_eq!(w.me_gold, 100);
        w.apply(&msg(pk::GOLD_CHANGE, pkt(|p| { p.u8(b'D').i32(250); })));
        assert_eq!(w.me_gold, 0);

        // StatUpdate: NPC health value + max ('A'/'M', attr index 5).
        w.apply(&msg(pk::STAT_UPDATE, pkt(|p| { p.u8(b'A').u16(50).u8(5).u16(80); })));
        w.apply(&msg(pk::STAT_UPDATE, pkt(|p| { p.u8(b'M').u16(50).u8(5).u16(100); })));
        assert_eq!(w.actors[&50].attributes[&5], (80, 100));
        // StatUpdate for self goes to me_attributes.
        w.apply(&msg(pk::STAT_UPDATE, pkt(|p| { p.u8(b'A').u16(7).u8(5).u16(42); })));
        assert_eq!(w.me_attributes[&5], (42, 0));

        // ActorDead marks the NPC dead.
        w.apply(&msg(pk::ACTOR_DEAD, pkt(|p| { p.u16(50); })));
        assert!(!w.actors[&50].alive);
    }
}
