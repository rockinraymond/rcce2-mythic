//! Client→server packet builders (the send side).

use rcce_net::codec::MsgWriter;

/// Build a `P_StandardUpdate` movement payload (the client send side,
/// `ClientNet.bb:1801`): `DestX, DestZ, Y, X, Z` (f32 LE) then `IsRunning,
/// WalkingBackward` (u8). The server (ServerNet.bb:1796) trusts the claimed
/// X/Z within a speed limit and moves the actor toward Dest. Sent unreliable
/// (`RCE_Send` defaults `ReliableFlag = 0`).
pub fn movement_packet(
    dest_x: f32,
    dest_z: f32,
    y: f32,
    x: f32,
    z: f32,
    running: bool,
    walking_backward: bool,
) -> Vec<u8> {
    let mut w = MsgWriter::new();
    w.f32(dest_x)
        .f32(dest_z)
        .f32(y)
        .f32(x)
        .f32(z)
        .u8(running as u8)
        .u8(walking_backward as u8);
    w.into_bytes()
}

/// Build a `P_SpellUpdate` cast request (`Interface3D.bb:1121-1128`): `"F"` +
/// spell id (u16) + an optional target RuntimeID (u16). The target bytes are
/// omitted when there's no target — the server tolerates a missing target
/// rather than the client sending a stale handle. Sent reliable.
pub fn cast_packet(spell_id: u16, target: Option<u16>) -> Vec<u8> {
    let mut w = MsgWriter::new();
    w.u8(b'F').u16(spell_id);
    if let Some(rid) = target {
        w.u16(rid);
    }
    w.into_bytes()
}

/// Build a `P_InventoryUpdate` pickup request (`ServerNet.bb:1611`): `"P"` +
/// DroppedItem handle (u32) + target inventory slot (u8). The server validates
/// same-area + distance + slot, then replies `"R"` to the picker and `"P"` to
/// everyone else. Sent reliable.
pub fn pickup_packet(handle: u32, slot: u8) -> Vec<u8> {
    let mut w = MsgWriter::new();
    w.u8(b'P').u32(handle).u8(slot);
    w.into_bytes()
}

/// `P_RightClick` (`Interface3D.bb:668`): the target actor's RuntimeID (u16).
/// Triggers the NPC's RightClick script server-side — a vendor replies with
/// `P_OpenTrading`, a dialog NPC with chat output. Sent reliable.
pub fn right_click_packet(runtime_id: u16) -> Vec<u8> {
    runtime_id.to_le_bytes().to_vec()
}

/// `P_Examine` (`Interface3D.bb:694`): the target actor's RuntimeID (u16).
/// Runs the NPC's Examine script; the reply comes back as chat/output text.
/// Sent reliable.
pub fn examine_packet(runtime_id: u16) -> Vec<u8> {
    runtime_id.to_le_bytes().to_vec()
}

/// Number of vendor/inventory slots in a trade basket (client `Dim(31)`).
const TRADE_SLOTS: usize = 32;

/// Build a `P_OpenTrading` buy/sell confirmation (`Interface3D.bb:2335-2350`):
/// 32 "his" slots of `ServerTradeID i32 + amount u16` (the items being bought;
/// unused slots are `-1, 0`), then 32 "mine" slots of `backpackSlot u8 +
/// amount u16` (items being sold; unused `0, 0`). Sent reliable.
pub fn trade_confirm_packet(buys: &[(u32, u16)], sells: &[(u8, u16)]) -> Vec<u8> {
    let mut w = MsgWriter::new();
    for i in 0..TRADE_SLOTS {
        match buys.get(i) {
            Some(&(id, amt)) => {
                w.u32(id).u16(amt);
            }
            None => {
                w.u32(0xFFFF_FFFF).u16(0); // -1 sentinel = empty slot
            }
        }
    }
    for i in 0..TRADE_SLOTS {
        match sells.get(i) {
            Some(&(slot, amt)) => {
                w.u8(slot).u16(amt);
            }
            None => {
                w.u8(0).u16(0);
            }
        }
    }
    w.into_bytes()
}

/// Build a `P_InventoryUpdate` drop request (`ServerNet.bb:1671`): `"D"` +
/// inventory slot u8 + amount u16. The server drops the item to the floor (a
/// world DroppedItem) and "T"-takes it from our inventory. Sent reliable.
pub fn inv_drop_packet(slot: u8, amount: u16) -> Vec<u8> {
    let mut w = MsgWriter::new();
    w.u8(b'D').u8(slot).u16(amount);
    w.into_bytes()
}

/// Build a `P_OpenTrading` close (`Interface3D.bb:2303`): an empty body tells
/// the server the trade window was dismissed. Sent reliable.
pub fn trade_close_packet() -> Vec<u8> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_confirm_one_buy_layout() {
        let p = trade_confirm_packet(&[(1001, 3)], &[]);
        // 32*(4+2) + 32*(1+2) = 192 + 96 = 288 bytes.
        assert_eq!(p.len(), 288);
        // Slot 0 buy: id 1001 (LE u32) + amount 3 (LE u16).
        assert_eq!(&p[0..6], &[0xE9, 0x03, 0, 0, 3, 0]);
        // Slot 1 buy: the -1 sentinel + 0.
        assert_eq!(&p[6..12], &[0xFF, 0xFF, 0xFF, 0xFF, 0, 0]);
        // First "mine" sell slot (offset 192): empty 0,0,0.
        assert_eq!(&p[192..195], &[0, 0, 0]);
    }

    #[test]
    fn trade_confirm_with_sell() {
        let p = trade_confirm_packet(&[], &[(14, 5)]);
        assert_eq!(p.len(), 288);
        // Sell slot 0 at byte 192: backpack slot 14 + amount 5 (LE u16).
        assert_eq!(&p[192..195], &[14, 5, 0]);
    }

    #[test]
    fn trade_close_is_empty() {
        assert!(trade_close_packet().is_empty());
    }

    #[test]
    fn inv_drop_layout() {
        // "D" + slot 14 + amount 1 (LE u16).
        assert_eq!(inv_drop_packet(14, 1), vec![b'D', 14, 1, 0]);
        assert_eq!(inv_drop_packet(3, 0x0102), vec![b'D', 3, 0x02, 0x01]);
    }

    #[test]
    fn interact_packets_are_le_runtime_id() {
        assert_eq!(right_click_packet(7), vec![7, 0]);
        assert_eq!(examine_packet(0x0102), vec![0x02, 0x01]);
    }

    #[test]
    fn cast_without_target_omits_rid() {
        // "F" + spell id 101 (LE), no target → 3 bytes.
        assert_eq!(cast_packet(101, None), vec![b'F', 101, 0]);
    }

    #[test]
    fn cast_with_target_appends_rid() {
        // "F" + spell 101 + target 7 (both LE u16) → 5 bytes.
        assert_eq!(cast_packet(101, Some(7)), vec![b'F', 101, 0, 7, 0]);
    }

    #[test]
    fn pickup_layout() {
        // "P" + handle 0x01020304 (LE) + slot 14.
        assert_eq!(pickup_packet(0x0102_0304, 14), vec![b'P', 0x04, 0x03, 0x02, 0x01, 14]);
    }
}
