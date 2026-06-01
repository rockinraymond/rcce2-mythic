//! Live game-state model, fed by the server packet stream.
//!
//! Packet payload layouts decoded from the reference client's parse code
//! (`ClientNet.bb`) and the server serializer (`Actors.bb::ActorInstanceToString`).
//! All multi-byte fields are **little-endian** (handled by `MsgReader`).

use std::collections::HashMap;

use rcce_net::codec::{MsgReader, MsgWriter};
use rcce_net::{packet_id as pk, RecvMessage};

/// An open NPC dialog window (`P_Dialog` "N"/"T"/"O"/"C"). Server-driven: the
/// NPC's `Main` script pushes a title, text lines, and option lines; the client
/// echoes "N"/"T" acks (via `pending_sends`) so the script advances. One active
/// dialog at a time (matches typical play). ref ClientNet.bb:1027-1068.
#[derive(Debug, Clone, Default)]
pub struct Dialog {
    pub script_handle: u32,
    pub runtime_id: u16,
    pub title: String,
    pub lines: Vec<(String, [f32; 4])>,
    pub options: Vec<String>,
}

/// An in-flight projectile (`P_Projectile`, id 37). Spawns at the source actor,
/// flies toward the target (homing → the target actor's live position, else a
/// snapshot taken at spawn) and is removed on impact (within 2 units). Rendered
/// as a billboard at its projected screen position. ref ClientNet.bb:217-238.
#[derive(Debug, Clone)]
pub struct Projectile {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Homing target's runtime id (only when `homing`); 0 otherwise.
    pub target_rid: u16,
    pub tx: f32,
    pub ty: f32,
    pub tz: f32,
    pub homing: bool,
    /// World units per second.
    pub speed: f32,
}

/// One actor instance in the current zone (player or NPC).
/// A combat hit reported by `P_AttackActor`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombatEvent {
    pub target: u16,
    pub damage: u16,
    /// Damage-type index (maps to a name via Damage.dat).
    pub damage_type: u8,
}

#[derive(Debug, Clone, Default)]
pub struct Actor {
    pub runtime_id: u16,
    pub template_id: u16,
    pub name: String,
    pub tag: String,
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
    /// Appearance from P_NewActor: gender (0 male / 1 female) and the 0..4
    /// face/body/hair/beard selection indices into the actor template's id
    /// arrays. Drive which skin + hair/beard mesh this actor draws.
    pub gender: u8,
    pub face_tex: u8,
    pub body_tex: u8,
    pub hair: u8,
    pub beard: u8,
    /// Health value/maximum from P_NewActor (spawn HP; the bar fraction).
    pub health: i16,
    pub health_max: i16,
    /// Attribute index → (value, maximum), as delivered by P_StatUpdate.
    /// Sparse — only attributes the server has sent. Health/Energy/etc. indices
    /// come from Fixed Attributes.dat (the caller maps them).
    pub attributes: HashMap<u8, (i16, i16)>,
    /// Equipped gear item ids from P_InventoryUpdate "O": [weapon, shield,
    /// chest, hat]. 65535 = nothing in that slot. The foundation for attaching
    /// gear meshes; for now the weapon name shows on the nameplate.
    pub equipped: [u16; 4],
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
    /// Local player's appearance (from our own P_NewActor).
    pub me_gender: u8,
    pub me_face_tex: u8,
    pub me_body_tex: u8,
    pub me_hair: u8,
    pub me_beard: u8,
    pub me_health: i16,
    pub me_health_max: i16,
    /// Template gender mode (`Actors.dat` `Genders`) keyed by template id.
    /// Populated by the host before applying packets so `on_new_actor` knows
    /// whether the wire carries a gender byte (only when mode == 0). Empty map
    /// ⇒ assume 0 (byte present), the players-and-most-NPCs default.
    pub template_genders: HashMap<u16, u8>,
    pub zone: Zone,
    /// Other actors keyed by runtime id (excludes the local player).
    pub actors: HashMap<u16, Actor>,
    /// Recent chat lines (control-byte channel prefixes stripped).
    /// Recent chat lines with their colour (from the `P_ChatMessage` sentinel).
    pub chat: Vec<(String, [f32; 4])>,
    // Local player progression / stats.
    pub me_xp: i32,
    pub me_xp_bar: u8,
    pub me_gold: i32,
    pub me_attributes: HashMap<u8, (i16, i16)>,
    /// Recent combat hits (from P_AttackActor).
    pub combat_events: Vec<CombatEvent>,
    /// Items dropped in the world (P_InventoryUpdate "D"), keyed by the
    /// server's DroppedItem handle. Removed on pickup ("P"/"R").
    pub dropped_items: HashMap<u32, DroppedItem>,
    /// The open vendor/trade window, if any (P_OpenTrading).
    pub current_trade: Option<crate::trade::TradeWindow>,
    /// The open NPC dialog window, if any (P_Dialog). See [`Dialog`].
    pub dialog: Option<Dialog>,
    /// In-flight projectiles (P_Projectile). See [`Projectile`].
    pub projectiles: Vec<Projectile>,
    /// The local player's inventory, keyed by slot (0..13 equipment, 14..45
    /// backpack). Seeded from the P_FetchCharacter sheet, then kept live by
    /// P_InventoryUpdate G/T/H/R. BTreeMap so the panel iterates in slot order.
    pub me_inventory: std::collections::BTreeMap<u8, crate::fetch::InvItem>,
    /// Outbound packets the apply() logic needs to send (e.g. the "GY" accept
    /// for a given item). The host drains this after each poll.
    pub pending_sends: Vec<(u8, Vec<u8>)>,
    /// Active status effects (buffs/debuffs) on the local player, from
    /// P_ActorEffect. Shown as a HUD icon row.
    pub active_effects: Vec<ActiveEffect>,
}

/// A buff/debuff on the local player (P_ActorEffect "A").
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveEffect {
    pub id: u32,
    pub texture_id: u16,
    pub name: String,
}

/// An item lying on the ground, from `P_InventoryUpdate "D"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DroppedItem {
    pub handle: u32,
    pub item_id: u16,
    pub amount: u16,
    pub health: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
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
            pk::ATTACK_ACTOR => self.on_attack_actor(&m.data),
            pk::NAME_CHANGE => self.on_name_change(&m.data),
            pk::INVENTORY_UPDATE => self.on_inventory_update(&m.data),
            pk::ACTOR_EFFECT => self.on_actor_effect(&m.data),
            pk::WEATHER_CHANGE => self.on_weather_change(&m.data),
            pk::OPEN_TRADING => self.current_trade = crate::trade::TradeWindow::parse(&m.data),
            pk::DIALOG => self.on_dialog(&m.data),
            pk::PROJECTILE => self.on_projectile(&m.data),
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

    /// `P_NewActor` = `ActorInstanceToString` (Actors.bb:1057): ServerArea u32 ·
    /// RuntimeID u16 · Level u16 · XP u32 · TemplateID u16 · X,Y,Z,Yaw f32 ·
    /// isPlayer u8 · nameLen u8 · name · tagLen u8 · tag · **gender u8 (only if
    /// the template's Genders mode == 0)** · Reputation i16 · FaceTex u16 ·
    /// Hair u16 · BodyTex u16 · Beard u16 · (stats/equipment/factions, ignored).
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
        let tag = r.str8().unwrap_or_default();

        // Gender byte is present only when the template is player-selectable
        // (mode 0). For mode 1 it's male (0); mode 2 it's female (1).
        let mode = self.template_genders.get(&template_id).copied().unwrap_or(0);
        let gender = if mode == 0 {
            (r.u8().unwrap_or(0)).min(1)
        } else if mode == 2 {
            1
        } else {
            0
        };
        let _reputation = r.u16(); // skip 2 bytes (value unused here)
        let clamp4 = |v: Option<u16>| v.unwrap_or(0).min(4) as u8;
        let face_tex = clamp4(r.u16());
        let hair = clamp4(r.u16());
        let body_tex = clamp4(r.u16());
        let beard = clamp4(r.u16());
        // Speed (value, max) then Health (value, max).
        let _speed = (r.u16(), r.u16());
        let health = r.u16().unwrap_or(0) as i16;
        let health_max = r.u16().unwrap_or(0) as i16;

        if runtime_id == self.my_runtime_id {
            self.me_x = x;
            self.me_y = y;
            self.me_z = z;
            self.me_yaw = yaw;
            self.me_gender = gender;
            self.me_face_tex = face_tex;
            self.me_body_tex = body_tex;
            self.me_hair = hair;
            self.me_beard = beard;
            self.me_health = health;
            self.me_health_max = health_max;
            return; // don't list ourselves among "other actors"
        }
        self.actors.insert(
            runtime_id,
            Actor {
                runtime_id,
                template_id,
                name,
                tag,
                is_player,
                x,
                y,
                z,
                yaw,
                dest_x: x,
                dest_z: z,
                alive: true,
                gender,
                face_tex,
                body_tex,
                hair,
                beard,
                health,
                health_max,
                equipped: [0xFFFF; 4], // nothing equipped until an "O" update
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
        // Health is attribute 0 (Server.bb reads HealthStat from
        // Fixed Attributes.dat → 0); mirror it onto the actor's health field so
        // the HP bars reflect live combat damage.
        const HEALTH_STAT: u8 = 0;
        if rid == self.my_runtime_id {
            let e = self.me_attributes.entry(attr).or_default();
            match kind {
                b'A' => e.0 = val,
                b'M' => e.1 = val,
                _ => {}
            }
            if attr == HEALTH_STAT {
                match kind {
                    b'A' => self.me_health = val,
                    b'M' => self.me_health_max = val,
                    _ => {}
                }
            }
        } else if let Some(a) = self.actors.get_mut(&rid) {
            let e = a.attributes.entry(attr).or_default();
            match kind {
                b'A' => e.0 = val,
                b'M' => e.1 = val,
                _ => {}
            }
            if attr == HEALTH_STAT {
                match kind {
                    b'A' => a.health = val,
                    b'M' => a.health_max = val,
                    _ => {}
                }
            }
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

    /// `P_AttackActor` (ClientNet.bb:1115): byte0 `'H'`(hit) + targetRID(u16) +
    /// rawDamage(u16, −1) + damageType(u8). HP itself arrives via P_StatUpdate;
    /// this records the hit for feedback.
    fn on_attack_actor(&mut self, d: &[u8]) {
        if d.first() != Some(&b'H') {
            return;
        }
        let mut r = MsgReader::new(&d[1..]);
        let (Some(target), Some(raw_dmg), Some(dtype)) = (r.u16(), r.u16(), r.u8()) else {
            return;
        };
        let damage = raw_dmg.saturating_sub(1);
        self.combat_events.push(CombatEvent {
            target,
            damage,
            damage_type: dtype,
        });
    }

    /// `P_NameChange` (ClientNet.bb:936): RID(u16) + nameLen(u8) + name + tag.
    fn on_name_change(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        let (Some(rid), Some(name_len)) = (r.u16(), r.u8()) else {
            return;
        };
        let rest = r.rest();
        let n = (name_len as usize).min(rest.len());
        let name = String::from_utf8_lossy(&rest[..n]).into_owned();
        let tag = String::from_utf8_lossy(&rest[n..]).into_owned();
        if let Some(a) = self.actors.get_mut(&rid) {
            a.name = name;
            a.tag = tag;
        }
    }

    /// `P_InventoryUpdate` (ClientNet.bb:1277): a sub-typed family covering both
    /// world loot ("D"/"P") and the local player's own inventory ("R" received,
    /// "G" given, "T" taken, "H" health), keeping `me_inventory` live.
    fn on_inventory_update(&mut self, d: &[u8]) {
        match d.first() {
            // Item dropped in the world: amount u16, x/y/z f32, handle u32, then
            // the 83-byte ItemInstance (id = first u16, health = last byte).
            Some(b'D') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(amount), Some(x), Some(y), Some(z), Some(handle)) =
                    (r.u16(), r.f32(), r.f32(), r.f32(), r.u32())
                else {
                    return;
                };
                let Some(item) = r.bytes(83) else { return };
                let item_id = u16::from_le_bytes([item[0], item[1]]);
                if item_id == 0xFFFF {
                    return; // no-item sentinel
                }
                let health = item[82];
                self.dropped_items
                    .insert(handle, DroppedItem { handle, item_id, amount, health, x, y, z });
            }
            // Someone else picked up a dropped item (handle u32) — remove it.
            Some(b'P') => {
                if let Some(h) = MsgReader::new(&d[1..]).u32() {
                    self.dropped_items.remove(&h);
                }
            }
            // I received a dropped item: handle u32 + slot u8. Move it from the
            // world into my inventory.
            Some(b'R') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(handle), Some(slot)) = (r.u32(), r.u8()) else { return };
                if let Some(di) = self.dropped_items.remove(&handle) {
                    self.inv_add(slot, di.item_id, di.amount, di.health);
                }
            }
            // Given an item: handle u32 + ItemID u16 + Amount u16. Place it in a
            // free/stackable slot and ACK with "GY" + handle + slot (or "GN").
            Some(b'G') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(handle), Some(item_id), Some(amount)) = (r.u32(), r.u16(), r.u16()) else {
                    return;
                };
                if item_id == 0xFFFF {
                    return;
                }
                match self.inv_free_slot(item_id) {
                    Some(slot) => {
                        self.inv_add(slot, item_id, amount, 100);
                        let mut reply = b"GY".to_vec();
                        reply.extend_from_slice(&handle.to_le_bytes());
                        reply.push(slot);
                        self.pending_sends.push((pk::INVENTORY_UPDATE, reply));
                    }
                    None => {
                        let mut reply = b"GN".to_vec();
                        reply.extend_from_slice(&handle.to_le_bytes());
                        self.pending_sends.push((pk::INVENTORY_UPDATE, reply));
                    }
                }
            }
            // An item was taken from my inventory: slot u8 + amount u16.
            Some(b'T') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(slot), Some(amount)) = (r.u8(), r.u16()) else { return };
                if let Some(it) = self.me_inventory.get_mut(&slot) {
                    it.amount = it.amount.saturating_sub(amount);
                    if it.amount == 0 {
                        self.me_inventory.remove(&slot);
                    }
                }
            }
            // Equipped-gear update for an actor: rid u16 + weapon/shield/chest/
            // hat item ids (u16 each, 65535 = none) + 6 gubbin bytes (ignored).
            Some(b'O') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(rid), Some(weapon), Some(shield), Some(chest), Some(hat)) =
                    (r.u16(), r.u16(), r.u16(), r.u16(), r.u16())
                else {
                    return;
                };
                if let Some(a) = self.actors.get_mut(&rid) {
                    a.equipped = [weapon, shield, chest, hat];
                }
            }
            // An item's health (durability) changed: slot u8 + health u8.
            Some(b'H') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(slot), Some(health)) = (r.u8(), r.u8()) else { return };
                if let Some(it) = self.me_inventory.get_mut(&slot) {
                    it.health = health;
                }
            }
            _ => {}
        }
    }

    /// Add `amount` of `item_id` to inventory slot `slot`, stacking if the slot
    /// already holds the same item.
    fn inv_add(&mut self, slot: u8, item_id: u16, amount: u16, health: u8) {
        let e = self
            .me_inventory
            .entry(slot)
            .or_insert(crate::fetch::InvItem { slot, item_id, amount: 0, health });
        if e.item_id == item_id {
            e.amount = e.amount.saturating_add(amount);
            e.health = health;
        } else {
            *e = crate::fetch::InvItem { slot, item_id, amount, health };
        }
    }

    /// Pick a slot for an incoming item: an existing backpack slot holding the
    /// same item (to stack), else the first empty backpack slot (14..=45).
    fn inv_free_slot(&self, item_id: u16) -> Option<u8> {
        if let Some((&slot, _)) = self
            .me_inventory
            .iter()
            .find(|(&s, it)| s >= 14 && it.item_id == item_id)
        {
            return Some(slot);
        }
        (14u8..=45).find(|s| !self.me_inventory.contains_key(s))
    }

    /// `P_WeatherChange` (ClientNet.bb:1272): areaId u32 + weather u8. Applies
    /// only when it targets the area we're standing in.
    fn on_weather_change(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        let (Some(area), Some(weather)) = (r.u32(), r.u8()) else {
            return;
        };
        if area == self.zone.area_id {
            self.zone.weather = weather;
        }
    }

    /// `P_ActorEffect` (ClientNet.bb:493): the local player's status effects.
    /// "A" adds an effect (id u32, texture u16, name), "E" applies an attribute
    /// delta (att u8, amount i32), "R" removes an effect by id and undoes its
    /// 40×i32 attribute deltas.
    fn on_actor_effect(&mut self, d: &[u8]) {
        match d.first() {
            Some(b'A') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(id), Some(texture_id)) = (r.u32(), r.u16()) else { return };
                let name = String::from_utf8_lossy(r.rest()).into_owned();
                self.active_effects.retain(|e| e.id != id);
                self.active_effects.push(ActiveEffect { id, texture_id, name });
            }
            Some(b'E') => {
                let mut r = MsgReader::new(&d[1..]);
                let (Some(att), Some(amount)) = (r.u8(), r.i32()) else { return };
                if att < 40 {
                    let e = self.me_attributes.entry(att).or_default();
                    e.0 = e.0.saturating_add(amount as i16);
                }
            }
            Some(b'R') => {
                let mut r = MsgReader::new(&d[1..]);
                let Some(id) = r.u32() else { return };
                self.active_effects.retain(|e| e.id != id);
                // Optional 40×i32 attribute-restore block (subtract the deltas).
                if d.len() >= 1 + 4 + 40 * 4 {
                    for i in 0..40u8 {
                        if let Some(amount) = r.i32() {
                            let e = self.me_attributes.entry(i).or_default();
                            e.0 = e.0.saturating_sub(amount as i16);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// `P_ChatMessage`: an optional leading colour sentinel then text (CHAT-2,
    /// ClientNet.bb:1219). 254=yellow, 253=red, 252=purple, 251=green, 250 = the
    /// next 3 bytes as explicit RGB; otherwise white. A `<<…>>`-prefixed line
    /// (the local player's own) renders blue.
    fn on_chat(&mut self, d: &[u8]) {
        if d.is_empty() {
            return;
        }
        let (mut color, body): ([f32; 4], &[u8]) = match d[0] {
            254 => ([1.0, 1.0, 0.0, 1.0], &d[1..]),
            253 => ([1.0, 0.2, 0.2, 1.0], &d[1..]),
            252 => ([0.78, 0.04, 0.78, 1.0], &d[1..]),
            251 => ([0.08, 0.86, 0.2, 1.0], &d[1..]),
            250 if d.len() >= 4 => (
                [d[1] as f32 / 255.0, d[2] as f32 / 255.0, d[3] as f32 / 255.0, 1.0],
                &d[4..],
            ),
            _ => ([0.92, 0.92, 0.78, 1.0], d),
        };
        let text: String = body.iter().filter(|&&b| b >= 32).map(|&b| b as char).collect();
        let text = text.trim().to_string();
        if text.starts_with("<<") {
            color = [0.0, 0.5, 1.0, 1.0]; // local player's own line
        }
        if !text.is_empty() {
            self.chat.push((text, color));
        }
    }

    /// Handle an inbound `P_Dialog` (NPC dialog). Builds/updates `self.dialog`
    /// and queues the "N"/"T" acks the NPC `Main` script waits on. Soft-fails on
    /// a short/garbage payload. ref ClientNet.bb:1027-1068.
    fn on_dialog(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        match r.u8() {
            // New: [4]scriptHandle [2]runtimeID [2]bgTexID [n]title.
            Some(b'N') => {
                let (Some(script), Some(rid), Some(_bg)) = (r.u32(), r.u16(), r.u16()) else {
                    return;
                };
                let title = String::from_utf8_lossy(r.rest()).into_owned();
                self.dialog = Some(Dialog {
                    script_handle: script,
                    runtime_id: rid,
                    title,
                    lines: Vec::new(),
                    options: Vec::new(),
                });
                // Reply "N" + scriptHandle + our dialog handle (we reuse the
                // scriptHandle as the handle) so the server maps its script here.
                let mut w = MsgWriter::new();
                w.u8(b'N').u32(script).u32(script);
                self.pending_sends.push((pk::DIALOG, w.into_bytes()));
            }
            // Text: [1]R [1]G [1]B [4]dialogHandle [n]text.
            Some(b'T') => {
                let (Some(red), Some(green), Some(blue), Some(_dh)) =
                    (r.u8(), r.u8(), r.u8(), r.u32())
                else {
                    return;
                };
                let text = String::from_utf8_lossy(r.rest()).into_owned();
                if let Some(dl) = self.dialog.as_mut() {
                    let col = [red as f32 / 255.0, green as f32 / 255.0, blue as f32 / 255.0, 1.0];
                    dl.lines.push((text, col));
                    let mut w = MsgWriter::new();
                    w.u8(b'T').u32(dl.script_handle);
                    self.pending_sends.push((pk::DIALOG, w.into_bytes()));
                }
            }
            // Options: [4]dialogHandle then repeated [1]len [len]optionText.
            Some(b'O') => {
                if r.u32().is_none() {
                    return;
                }
                if let Some(dl) = self.dialog.as_mut() {
                    dl.options.clear();
                    loop {
                        let Some(n) = r.u8() else { break };
                        let Some(b) = r.bytes(n as usize) else { break };
                        dl.options.push(String::from_utf8_lossy(b).into_owned());
                    }
                }
            }
            // Close: [4]dialogHandle.
            Some(b'C') => self.dialog = None,
            _ => {}
        }
    }

    /// Resolve a runtime id to a world position (self or a tracked actor).
    fn actor_pos(&self, rid: u16) -> Option<[f32; 3]> {
        if rid == self.my_runtime_id {
            Some([self.me_x, self.me_y, self.me_z])
        } else {
            self.actors.get(&rid).map(|a| [a.x, a.y, a.z])
        }
    }

    /// Handle an inbound `P_Projectile`: spawn a projectile at the source actor
    /// flying toward the target. Soft-fails if either actor is unknown.
    /// ref ClientNet.bb:217-238.
    fn on_projectile(&mut self, d: &[u8]) {
        let mut r = MsgReader::new(d);
        let (Some(src), Some(tgt), Some(_mesh), Some(_t1), Some(_t2), Some(homing), Some(spd)) =
            (r.u16(), r.u16(), r.u16(), r.u16(), r.u16(), r.u8(), r.u8())
        else {
            return;
        };
        let (Some(sp), Some(tp)) = (self.actor_pos(src), self.actor_pos(tgt)) else {
            return;
        };
        // Blitz: Speed# = (serverSpeed/50)·2.0 units/frame@30fps → ·30 for /sec.
        let speed = (spd as f32 / 50.0) * 2.0 * 30.0;
        self.projectiles.push(Projectile {
            x: sp[0],
            y: sp[1] + 3.0,
            z: sp[2],
            target_rid: if homing != 0 { tgt } else { 0 },
            tx: tp[0],
            ty: tp[1] + 3.0,
            tz: tp[2],
            homing: homing != 0,
            speed,
        });
    }

    /// Advance every projectile toward its target and drop those that impact
    /// (within 2 units). Homing projectiles re-acquire the live target position.
    pub fn tick_projectiles(&mut self, dt: f32) {
        let my = self.my_runtime_id;
        let me = [self.me_x, self.me_y, self.me_z];
        for p in &mut self.projectiles {
            if p.homing {
                let tp = if p.target_rid == my {
                    Some(me)
                } else {
                    self.actors.get(&p.target_rid).map(|a| [a.x, a.y, a.z])
                };
                if let Some(tp) = tp {
                    p.tx = tp[0];
                    p.ty = tp[1] + 3.0;
                    p.tz = tp[2];
                }
            }
            let (dx, dy, dz) = (p.tx - p.x, p.ty - p.y, p.tz - p.z);
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            if dist > 0.001 {
                let step = (p.speed * dt).min(dist);
                p.x += dx / dist * step;
                p.y += dy / dist * step;
                p.z += dz / dist * step;
            }
        }
        self.projectiles.retain(|p| {
            let (dx, dy, dz) = (p.tx - p.x, p.ty - p.y, p.tz - p.z);
            (dx * dx + dy * dy + dz * dz).sqrt() > 2.0
        });
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
    fn dialog_new_text_options_close() {
        let mut w = World::default();
        // "N": scriptHandle, runtimeID, bgTexID, title.
        let mut p = MsgWriter::new();
        p.u8(b'N').u32(0x1122_3344).u16(5).u16(0).raw(b"Hail");
        w.apply(&msg(pk::DIALOG, p.into_bytes()));
        let dl = w.dialog.as_ref().expect("dialog created");
        assert_eq!((dl.script_handle, dl.runtime_id, dl.title.as_str()), (0x1122_3344, 5, "Hail"));
        // Reply "N" + scriptHandle + dialogHandle (== scriptHandle).
        let mut exp = MsgWriter::new();
        exp.u8(b'N').u32(0x1122_3344).u32(0x1122_3344);
        assert_eq!(w.pending_sends, vec![(pk::DIALOG, exp.into_bytes())]);
        w.pending_sends.clear();

        // "T": green text line + a "T" ack.
        let mut t = MsgWriter::new();
        t.u8(b'T').u8(0).u8(255).u8(0).u32(0x1122_3344).raw(b"Hello there");
        w.apply(&msg(pk::DIALOG, t.into_bytes()));
        assert_eq!(w.dialog.as_ref().unwrap().lines[0].0, "Hello there");
        let mut expt = MsgWriter::new();
        expt.u8(b'T').u32(0x1122_3344);
        assert_eq!(w.pending_sends.last().unwrap(), &(pk::DIALOG, expt.into_bytes()));

        // "O": two options.
        let mut o = MsgWriter::new();
        o.u8(b'O').u32(0x1122_3344).u8(3).raw(b"Yes").u8(2).raw(b"No");
        w.apply(&msg(pk::DIALOG, o.into_bytes()));
        assert_eq!(w.dialog.as_ref().unwrap().options, vec!["Yes".to_string(), "No".to_string()]);

        // "C": close.
        let mut c = MsgWriter::new();
        c.u8(b'C').u32(0x1122_3344);
        w.apply(&msg(pk::DIALOG, c.into_bytes()));
        assert!(w.dialog.is_none());
    }

    #[test]
    fn projectile_spawn_move_impact() {
        let mut w = World { my_runtime_id: 1, ..Default::default() };
        w.actors.insert(
            2,
            Actor { runtime_id: 2, x: 10.0, alive: true, ..Default::default() },
        );
        // P_Projectile: src=1(me) tgt=2 mesh/tex=0 homing=0 speed=50 emit1len=0.
        let mut p = MsgWriter::new();
        p.u16(1).u16(2).u16(0).u16(0).u16(0).u8(0).u8(50).u8(0);
        w.apply(&msg(pk::PROJECTILE, p.into_bytes()));
        assert_eq!(w.projectiles.len(), 1);
        assert!(w.projectiles[0].x.abs() < 0.01); // spawned at me (x=0)
        // speed = 50/50·2·30 = 60 u/s; 0.1s → ~6 units toward x=10.
        w.tick_projectiles(0.1);
        let x = w.projectiles[0].x;
        assert!((5.0..7.0).contains(&x), "moved to {x}");
        // Keep ticking until it impacts (within 2 of x=10) and is removed.
        for _ in 0..10 {
            w.tick_projectiles(0.1);
        }
        assert!(w.projectiles.is_empty(), "projectile impacted + removed");
    }

    #[test]
    fn chat_colour_sentinels() {
        let mut w = World::default();
        w.apply(&msg(pk::CHAT_MESSAGE, vec![254, b'h', b'i'])); // yellow
        w.apply(&msg(pk::CHAT_MESSAGE, vec![253, b'r', b'e', b'd'])); // red
        w.apply(&msg(pk::CHAT_MESSAGE, vec![250, 10, 20, 30, b'x'])); // explicit RGB
        w.apply(&msg(pk::CHAT_MESSAGE, b"plain".to_vec())); // white (no sentinel)
        w.apply(&msg(pk::CHAT_MESSAGE, b"<<Me>> hi".to_vec())); // own line -> blue
        assert_eq!(w.chat.len(), 5);
        assert_eq!(w.chat[0], ("hi".to_string(), [1.0, 1.0, 0.0, 1.0]));
        assert_eq!(w.chat[1].1, [1.0, 0.2, 0.2, 1.0]);
        assert_eq!(w.chat[2], ("x".to_string(), [10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0, 1.0]));
        assert_eq!(w.chat[3].0, "plain");
        assert!(w.chat[4].0.starts_with("<<Me>>") && w.chat[4].1 == [0.0, 0.5, 1.0, 1.0]);
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

        // Health is attr 0 → mirrored onto actor.health / me_health for the bars.
        w.apply(&msg(pk::STAT_UPDATE, pkt(|p| { p.u8(b'M').u16(50).u8(0).u16(120); })));
        w.apply(&msg(pk::STAT_UPDATE, pkt(|p| { p.u8(b'A').u16(50).u8(0).u16(75); })));
        assert_eq!((w.actors[&50].health, w.actors[&50].health_max), (75, 120));
        w.apply(&msg(pk::STAT_UPDATE, pkt(|p| { p.u8(b'A').u16(7).u8(0).u16(33); })));
        assert_eq!(w.me_health, 33);

        // ActorDead marks the NPC dead.
        w.apply(&msg(pk::ACTOR_DEAD, pkt(|p| { p.u16(50); })));
        assert!(!w.actors[&50].alive);
    }

    #[test]
    fn new_actor_appearance_both_gender_modes() {
        let mut w = World::default();
        // Template 3 = male-only (mode 1, NO gender byte); template 9 =
        // player-selectable (mode 0, gender byte present).
        w.template_genders.insert(3, 1);
        w.template_genders.insert(9, 0);

        // Mode-1 NPC: after name+tag, Reputation, then Face/Hair/Body/Beard.
        w.apply(&msg(
            pk::NEW_ACTOR,
            pkt(|p| {
                p.u32(0).u16(50).u16(1).u32(0).u16(3);
                p.f32(0.0).f32(0.0).f32(0.0).f32(0.0);
                p.u8(1).str8("Knight"); // isPlayer name
                p.str8("[Boss]"); // tag (no gender byte for mode 1)
                p.u16(0); // reputation
                p.u16(2).u16(1).u16(3).u16(4); // face hair body beard
            }),
        ));
        let a = &w.actors[&50];
        assert_eq!(a.tag, "[Boss]");
        assert_eq!(a.gender, 0); // mode 1 -> male
        assert_eq!((a.face_tex, a.hair, a.body_tex, a.beard), (2, 1, 3, 4));

        // Mode-0 player: gender byte = 1 (female) sits between tag and reputation.
        w.apply(&msg(
            pk::NEW_ACTOR,
            pkt(|p| {
                p.u32(0).u16(51).u16(1).u32(0).u16(9);
                p.f32(0.0).f32(0.0).f32(0.0).f32(0.0);
                p.u8(1).str8("Heroine");
                p.str8(""); // empty tag
                p.u8(1); // gender byte (female)
                p.u16(0); // reputation
                p.u16(4).u16(0).u16(1).u16(0); // face hair body beard
            }),
        ));
        let b = &w.actors[&51];
        assert_eq!(b.gender, 1);
        assert_eq!((b.face_tex, b.body_tex), (4, 1));
    }

    #[test]
    fn attack_and_rename() {
        let mut w = World::default();
        // Register an actor (rnid 50).
        w.apply(&msg(
            pk::NEW_ACTOR,
            pkt(|p| {
                p.u32(0).u16(50).u16(1).u32(0).u16(3);
                p.f32(0.0).f32(0.0).f32(0.0).f32(0.0);
                p.u8(0).str8("Stag");
            }),
        ));

        // P_AttackActor: 'H', target 50, raw damage 11 (-> 10), type 2.
        w.apply(&msg(pk::ATTACK_ACTOR, pkt(|p| { p.u8(b'H').u16(50).u16(11).u8(2); })));
        assert_eq!(
            w.combat_events.last().copied(),
            Some(CombatEvent { target: 50, damage: 10, damage_type: 2 })
        );

        // P_NameChange: rid 50, name "Boss", tag "[Elite]".
        w.apply(&msg(
            pk::NAME_CHANGE,
            pkt(|p| {
                p.u16(50).u8(4).raw(b"Boss").raw(b"[Elite]");
            }),
        ));
        assert_eq!(w.actors[&50].name, "Boss");
        assert_eq!(w.actors[&50].tag, "[Elite]");
    }

    #[test]
    fn dropped_item_spawn_and_pickup() {
        let mut w = World::default();
        // P_InventoryUpdate "D": amount u16, x/y/z f32, handle u32, then the
        // 83-byte ItemInstance (id = first u16).
        let drop = pkt(|p| {
            p.u8(b'D').u16(3).f32(12.0).f32(0.0).f32(34.0).u32(777);
            p.u16(42); // ItemInstance id
            p.raw(&[0u8; 81]); // rest of the 83-byte ItemInstance
        });
        w.apply(&msg(pk::INVENTORY_UPDATE, drop));
        assert_eq!(w.dropped_items.len(), 1);
        let di = w.dropped_items[&777];
        assert_eq!((di.item_id, di.amount), (42, 3));
        assert!((di.x - 12.0).abs() < 0.01 && (di.z - 34.0).abs() < 0.01);

        // "P" (someone else grabbed it) removes it by handle.
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'P').u32(777); })));
        assert!(w.dropped_items.is_empty());

        // A no-item-sentinel drop is ignored.
        let bad = pkt(|p| {
            p.u8(b'D').u16(1).f32(0.0).f32(0.0).f32(0.0).u32(9);
            p.u16(0xFFFF).raw(&[0u8; 81]);
        });
        w.apply(&msg(pk::INVENTORY_UPDATE, bad));
        assert!(w.dropped_items.is_empty());
    }

    #[test]
    fn inventory_give_take_health_sync() {
        let mut w = World::default();
        // "G" give: handle u32, item u16, amount u16 → free backpack slot + GY.
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'G').u32(99).u16(10).u16(2); })));
        assert_eq!(w.me_inventory.len(), 1);
        let (&slot, it) = w.me_inventory.iter().next().unwrap();
        assert_eq!(slot, 14); // first free backpack slot
        assert_eq!((it.item_id, it.amount), (10, 2));
        // Acked with "GY" + handle(LE) + slot.
        assert_eq!(w.pending_sends.len(), 1);
        assert_eq!(w.pending_sends[0].1, vec![b'G', b'Y', 99, 0, 0, 0, 14]);

        // Another give of the same item stacks into the same slot.
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'G').u32(1).u16(10).u16(3); })));
        assert_eq!(w.me_inventory.len(), 1);
        assert_eq!(w.me_inventory[&14].amount, 5);

        // "H" durability change on slot 14.
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'H').u8(14).u8(60); })));
        assert_eq!(w.me_inventory[&14].health, 60);

        // "T" take 2 → amount 3; take 3 more → slot removed.
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'T').u8(14).u16(2); })));
        assert_eq!(w.me_inventory[&14].amount, 3);
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'T').u8(14).u16(3); })));
        assert!(w.me_inventory.is_empty());
    }

    #[test]
    fn inventory_receive_from_dropped() {
        let mut w = World::default();
        // Drop an item in the world, then receive it into a slot.
        let drop = pkt(|p| {
            p.u8(b'D').u16(4).f32(0.0).f32(0.0).f32(0.0).u32(55);
            p.u16(7); // item id
            p.raw(&[0u8; 80]);
            p.u8(90); // ItemInstance health byte (offset 82)
        });
        w.apply(&msg(pk::INVENTORY_UPDATE, drop));
        assert_eq!(w.dropped_items.len(), 1);
        assert_eq!(w.dropped_items[&55].health, 90);

        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| { p.u8(b'R').u32(55).u8(20); })));
        assert!(w.dropped_items.is_empty());
        assert_eq!((w.me_inventory[&20].item_id, w.me_inventory[&20].amount, w.me_inventory[&20].health), (7, 4, 90));
    }

    #[test]
    fn equipped_update_sets_actor_gear() {
        let mut w = World::default();
        // Spawn an actor (rid 50).
        let mut p = MsgWriter::new();
        p.u32(7).u16(50).u16(1).u32(0).u16(3);
        p.f32(0.0).f32(0.0).f32(0.0).f32(0.0);
        p.u8(0).str8("Guard");
        w.apply(&msg(pk::NEW_ACTOR, p.into_bytes()));
        assert_eq!(w.actors[&50].equipped, [0xFFFF; 4]); // nothing yet

        // "O": rid 50, weapon 42, shield 65535, chest 7, hat 65535, + 6 gubbins.
        w.apply(&msg(pk::INVENTORY_UPDATE, pkt(|p| {
            p.u8(b'O').u16(50).u16(42).u16(0xFFFF).u16(7).u16(0xFFFF);
            p.raw(&[0u8; 6]);
        })));
        assert_eq!(w.actors[&50].equipped, [42, 0xFFFF, 7, 0xFFFF]);
    }

    #[test]
    fn actor_effect_add_modify_remove() {
        let mut w = World::default();
        // "A" add: id u32, texture u16, name.
        w.apply(&msg(pk::ACTOR_EFFECT, pkt(|p| { p.u8(b'A').u32(5).u16(10).raw(b"Poison"); })));
        assert_eq!(w.active_effects.len(), 1);
        assert_eq!(w.active_effects[0].name, "Poison");
        assert_eq!((w.active_effects[0].id, w.active_effects[0].texture_id), (5, 10));

        // "E" attribute delta: att 0, amount -30.
        w.apply(&msg(pk::ACTOR_EFFECT, pkt(|p| { p.u8(b'E').u8(0).i32(-30); })));
        assert_eq!(w.me_attributes[&0].0, -30);

        // Re-adding the same id replaces, not duplicates.
        w.apply(&msg(pk::ACTOR_EFFECT, pkt(|p| { p.u8(b'A').u32(5).u16(11).raw(b"Poison II"); })));
        assert_eq!(w.active_effects.len(), 1);
        assert_eq!(w.active_effects[0].name, "Poison II");

        // "R" remove by id (no restore block).
        w.apply(&msg(pk::ACTOR_EFFECT, pkt(|p| { p.u8(b'R').u32(5); })));
        assert!(w.active_effects.is_empty());
    }

    #[test]
    fn weather_change_only_for_current_area() {
        let mut w = World::default();
        w.zone.area_id = 7;
        w.zone.weather = 0;
        // A change for our area applies.
        w.apply(&msg(pk::WEATHER_CHANGE, pkt(|p| { p.u32(7).u8(1); })));
        assert_eq!(w.zone.weather, 1);
        // A change for a different area is ignored.
        w.apply(&msg(pk::WEATHER_CHANGE, pkt(|p| { p.u32(99).u8(2); })));
        assert_eq!(w.zone.weather, 1);
    }
}
