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
