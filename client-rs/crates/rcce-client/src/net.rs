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

#[cfg(test)]
mod tests {
    use super::*;

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
