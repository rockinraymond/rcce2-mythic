//! `P_FetchCharacter` response parsing — the character sheet (stats, inventory,
//! spells) the menu requests after VerifyAccount.
//!
//! The server streams several blocks, each a separate `P_FetchCharacter`
//! packet, distinguished by a 1- or 2-byte prefix (reference parser:
//! `MainMenu.bb:1878-1990`):
//!
//! - `"C1"` — stats: gold u32, reputation i16, level u16, xp u32, faction u8,
//!   then up to 40 × (value u16, max u16) attribute pairs.
//! - `"C3"` — inventory: repeated `slot u8`; if `slot < 46` (Slots_Inventory+1)
//!   it's a filled slot followed by an 83-byte ItemInstance + amount u16, else
//!   (sentinel 99) the slot is empty. May span multiple packets.
//! - `"S"` — known spells: repeated level u16, id u16, thumbTex u16, recharge
//!   u16, name str16, description str16, memorised u8.
//! - `"Q"` — quest log: repeated `nameLen u8 · name · statusLen u16 ·
//!   statusBlob` (the status blob is raw RGB + optional completed marker +
//!   text, parsed by `world::parse_quest_status` at seed time — not a UTF-8
//!   string, so it is read as raw bytes).
//! - `"F"` — terminator: questCount u16, spellCount u16.
//!
//! All multi-byte fields are little-endian (matches `MsgReader` and the live
//! stat/combat path). Each block is fed to [`CharacterSheet::apply_packet`];
//! callers loop until [`CharacterSheet::done`] (the `"F"` block) or a timeout.

use rcce_net::codec::MsgReader;

/// Inventory slot count test threshold (`Slots_Inventory + 1`,
/// `Inventories.bb:31`). A slot byte `>= 46` (e.g. the sentinel 99) is empty.
pub const SLOT_VALID_BELOW: u8 = 46;
/// Serialized ItemInstance length (`Items.bb:66`): id u16 + 40×attr u16 +
/// health u8 = 83 bytes.
pub const ITEM_INSTANCE_LEN: usize = 83;
/// The "no item" id sentinel (`WriteItemInstance` emits 65535 for Null).
pub const NO_ITEM: u16 = 65535;

/// One occupied inventory slot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InvItem {
    /// Slot index (0..13 equipment, 14..45 backpack).
    pub slot: u8,
    pub item_id: u16,
    pub amount: u16,
    pub health: u8,
}

/// One known spell (name/description come over the wire, so no Spells.dat
/// lookup is needed for display).
#[derive(Debug, Clone, PartialEq)]
pub struct SpellInfo {
    pub id: u16,
    pub level: u16,
    pub thumb_tex: u16,
    pub recharge: u16,
    pub name: String,
    pub description: String,
    pub memorised: bool,
}

/// Accumulated character data from the `P_FetchCharacter` stream.
#[derive(Debug, Clone, Default)]
pub struct CharacterSheet {
    pub gold: u32,
    pub reputation: i16,
    pub level: u16,
    pub xp: u32,
    pub home_faction: u8,
    /// (value, max) per attribute, in id order (up to 40).
    pub attributes: Vec<(i16, i16)>,
    pub inventory: Vec<InvItem>,
    pub spells: Vec<SpellInfo>,
    /// Quest log entries from the `"Q"` blocks: `(name, raw status blob)`. The
    /// blob is parsed into a coloured status line + completed flag at seed time
    /// via `world::parse_quest_status` (it carries non-text RGB bytes, so it is
    /// kept raw here rather than lossily decoded to a `String`).
    pub quests: Vec<(String, Vec<u8>)>,
    /// Set once the `"F"` terminator block is seen.
    pub done: bool,
}

impl CharacterSheet {
    /// Feed one `P_FetchCharacter` packet body. Unknown/short blocks are
    /// ignored (the server can split a block across packets; partial trailing
    /// data is simply skipped, matching the reference client's bounded walk).
    pub fn apply_packet(&mut self, data: &[u8]) {
        match data.first() {
            Some(b'C') if data.get(1) == Some(&b'1') => self.parse_stats(&data[2..]),
            Some(b'C') if data.get(1) == Some(&b'3') => self.parse_inventory(&data[2..]),
            Some(b'S') => self.parse_spells(&data[1..]),
            Some(b'Q') => self.parse_quests(&data[1..]),
            Some(b'F') => self.done = true,
            _ => {} // anything else: ignored
        }
    }

    fn parse_stats(&mut self, body: &[u8]) {
        let mut r = MsgReader::new(body);
        let (Some(gold), Some(rep), Some(level), Some(xp), Some(faction)) =
            (r.u32(), r.u16(), r.u16(), r.u32(), r.u8())
        else {
            return;
        };
        self.gold = gold;
        self.reputation = rep as i16;
        self.level = level;
        self.xp = xp;
        self.home_faction = faction;
        self.attributes.clear();
        while self.attributes.len() < 40 {
            let (Some(v), Some(m)) = (r.u16(), r.u16()) else { break };
            self.attributes.push((v as i16, m as i16));
        }
    }

    fn parse_inventory(&mut self, body: &[u8]) {
        let mut r = MsgReader::new(body);
        while let Some(slot) = r.u8() {
            if slot >= SLOT_VALID_BELOW {
                continue; // empty-slot sentinel (99)
            }
            let Some(item) = r.bytes(ITEM_INSTANCE_LEN) else { break };
            let Some(amount) = r.u16() else { break };
            let item_id = u16::from_le_bytes([item[0], item[1]]);
            let health = item[ITEM_INSTANCE_LEN - 1];
            if item_id != NO_ITEM {
                self.inventory.push(InvItem { slot, item_id, amount, health });
            }
        }
    }

    fn parse_spells(&mut self, body: &[u8]) {
        let mut r = MsgReader::new(body);
        while let (Some(level), Some(id), Some(thumb), Some(recharge)) =
            (r.u16(), r.u16(), r.u16(), r.u16())
        {
            let (Some(name), Some(description), Some(mem)) = (r.str16(), r.str16(), r.u8()) else {
                break;
            };
            if self.spells.len() >= 1000 {
                break;
            }
            self.spells.push(SpellInfo {
                id,
                level,
                thumb_tex: thumb,
                recharge,
                name,
                description,
                memorised: mem != 0,
            });
        }
    }

    /// Parse a `"Q"` quest-log block: repeated `nameLen u8 · name · statusLen
    /// u16 · statusBlob`. The status blob is kept raw (it holds RGB bytes the
    /// `world::parse_quest_status` reader needs); empty-name entries are
    /// skipped. Bounded at 500 (the server caps the log at 500 — ServerNet.bb).
    /// Mirrors the live `on_quest_log` "N" wire shape (world.rs).
    fn parse_quests(&mut self, body: &[u8]) {
        let mut r = MsgReader::new(body);
        while let Some(name) = r.str8() {
            let Some(n) = r.u16() else { break };
            let Some(blob) = r.bytes(n as usize) else { break };
            if self.quests.len() >= 500 {
                break;
            }
            if !name.is_empty() {
                self.quests.push((name, blob.to_vec()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcce_net::codec::MsgWriter;

    fn item_instance(id: u16, health: u8) -> Vec<u8> {
        let mut w = MsgWriter::new();
        w.u16(id);
        for _ in 0..40 {
            w.u16(5000); // value+5000 == 0
        }
        w.u8(health);
        w.into_bytes()
    }

    #[test]
    fn parse_c1_stats() {
        let mut w = MsgWriter::new();
        w.raw(b"C1").u32(1234).u16((-7i16) as u16).u16(12).u32(99999).u8(3);
        for i in 0..40 {
            w.u16(i as u16).u16((i as u16) + 100);
        }
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert_eq!(s.gold, 1234);
        assert_eq!(s.reputation, -7);
        assert_eq!(s.level, 12);
        assert_eq!(s.xp, 99999);
        assert_eq!(s.home_faction, 3);
        assert_eq!(s.attributes.len(), 40);
        assert_eq!(s.attributes[5], (5, 105));
    }

    #[test]
    fn parse_c3_inventory_mixed() {
        let mut w = MsgWriter::new();
        w.raw(b"C3");
        w.u8(0).raw(&item_instance(42, 200)).u16(1); // slot 0: sword
        w.u8(99); // empty slot sentinel
        w.u8(14).raw(&item_instance(7, 255)).u16(50); // backpack slot 14: 50 potions
        w.u8(99);
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert_eq!(s.inventory.len(), 2);
        assert_eq!(s.inventory[0], InvItem { slot: 0, item_id: 42, amount: 1, health: 200 });
        assert_eq!(s.inventory[1], InvItem { slot: 14, item_id: 7, amount: 50, health: 255 });
    }

    #[test]
    fn parse_c3_skips_no_item_sentinel() {
        let mut w = MsgWriter::new();
        w.raw(b"C3");
        w.u8(3).raw(&item_instance(NO_ITEM, 0)).u16(0); // filled slot but id=65535
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert!(s.inventory.is_empty());
    }

    #[test]
    fn parse_s_spells() {
        let mut w = MsgWriter::new();
        w.raw(b"S");
        // spell 1: Fireball, memorised
        w.u16(3).u16(101).u16(9).u16(2000);
        w.u16("Fireball".len() as u16).raw(b"Fireball");
        w.u16("Burns".len() as u16).raw(b"Burns");
        w.u8(1);
        // spell 2: Heal, not memorised
        w.u16(1).u16(102).u16(10).u16(1500);
        w.u16("Heal".len() as u16).raw(b"Heal");
        w.u16(0); // empty description
        w.u8(0);
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert_eq!(s.spells.len(), 2);
        assert_eq!(s.spells[0].name, "Fireball");
        assert_eq!(s.spells[0].level, 3);
        assert!(s.spells[0].memorised);
        assert_eq!(s.spells[1].name, "Heal");
        assert_eq!(s.spells[1].description, "");
        assert!(!s.spells[1].memorised);
    }

    /// Build one Q-block entry: `nameLen u8 · name · statusLen u16 · blob`.
    fn quest_entry(w: &mut MsgWriter, name: &str, blob: &[u8]) {
        w.u8(name.len() as u8).raw(name.as_bytes());
        w.u16(blob.len() as u16).raw(blob);
    }

    #[test]
    fn parse_q_quests() {
        // Two quests: an in-progress one and a completed one. The status blob is
        // RGB(3) + optional 254 completed-marker + text, exactly as the server
        // packs it and as world::parse_quest_status decodes it.
        let mut w = MsgWriter::new();
        w.raw(b"Q");
        quest_entry(&mut w, "Find the Sword", &{
            let mut b = vec![255u8, 255, 64];
            b.extend_from_slice(b"Search the ruins.");
            b
        });
        quest_entry(&mut w, "Greet the Mayor", &{
            let mut b = vec![128u8, 255, 128, 254];
            b.extend_from_slice(b"Done.");
            b
        });
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert_eq!(s.quests.len(), 2);
        assert_eq!(s.quests[0].0, "Find the Sword");
        assert_eq!(s.quests[1].0, "Greet the Mayor");
        // The raw blob round-trips through world::parse_quest_status correctly.
        let (text0, color0, done0) = crate::world::parse_quest_status(&s.quests[0].1);
        assert_eq!(text0, "Search the ruins.");
        assert_eq!(color0, [1.0, 1.0, 64.0 / 255.0, 1.0]);
        assert!(!done0);
        let (text1, _c1, done1) = crate::world::parse_quest_status(&s.quests[1].1);
        assert_eq!(text1, "Done.");
        assert!(done1);
    }

    #[test]
    fn parse_q_multi_packet_accumulates() {
        let mut s = CharacterSheet::default();
        let mut a = MsgWriter::new();
        a.raw(b"Q");
        quest_entry(&mut a, "Quest A", b"\x80\x80\x80status a");
        s.apply_packet(a.as_slice());
        let mut b = MsgWriter::new();
        b.raw(b"Q");
        quest_entry(&mut b, "Quest B", b"\x80\x80\x80status b");
        s.apply_packet(b.as_slice());
        assert_eq!(s.quests.len(), 2);
    }

    #[test]
    fn parse_q_truncated_does_not_panic_and_keeps_prefix() {
        // First entry complete; second entry's status is truncated (claims 50
        // bytes, supplies 3). The parser keeps the good prefix and stops without
        // panicking — the bounded-walk soft-fail discipline.
        let mut w = MsgWriter::new();
        w.raw(b"Q");
        quest_entry(&mut w, "Good", b"\xff\xff\xfftext");
        w.u8("Bad".len() as u8).raw(b"Bad");
        w.u16(50).raw(b"abc"); // statusLen says 50, only 3 present
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert_eq!(s.quests.len(), 1);
        assert_eq!(s.quests[0].0, "Good");
    }

    #[test]
    fn parse_q_empty_name_skipped() {
        // A zero-length name entry is consumed (cursor stays aligned) but not
        // stored, matching on_quest_log's `!name.is_empty()` guard.
        let mut w = MsgWriter::new();
        w.raw(b"Q");
        quest_entry(&mut w, "", b"\xff\xff\xffignored");
        quest_entry(&mut w, "Real", b"\xff\xff\xffkept");
        let mut s = CharacterSheet::default();
        s.apply_packet(w.as_slice());
        assert_eq!(s.quests.len(), 1);
        assert_eq!(s.quests[0].0, "Real");
    }

    #[test]
    fn terminator_sets_done() {
        let mut s = CharacterSheet::default();
        assert!(!s.done);
        let mut w = MsgWriter::new();
        w.raw(b"F").u16(0).u16(2);
        s.apply_packet(w.as_slice());
        assert!(s.done);
    }

    #[test]
    fn multi_packet_accumulates() {
        let mut s = CharacterSheet::default();
        let mut c3a = MsgWriter::new();
        c3a.raw(b"C3").u8(0).raw(&item_instance(1, 100)).u16(1);
        s.apply_packet(c3a.as_slice());
        let mut c3b = MsgWriter::new();
        c3b.raw(b"C3").u8(1).raw(&item_instance(2, 100)).u16(1);
        s.apply_packet(c3b.as_slice());
        assert_eq!(s.inventory.len(), 2);
    }
}
