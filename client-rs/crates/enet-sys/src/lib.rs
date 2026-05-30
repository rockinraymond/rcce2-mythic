//! Raw FFI to the vendored RCEnet ENet fork (see `vendor/`). Compiled from the
//! exact C source the server's `RCEnet.dll` was built from, so it is
//! wire-compatible by construction — including the fork's 8-byte header
//! (`[sessionID u32 LE][peerID u16 BE][sentTime u16 BE]`, protocol.c) that
//! stock `rusty_enet` couldn't speak. Builds 64-bit and cross-platform
//! (`vendor/unix.c` covers Linux/macOS), removing the 32-bit FFI constraint.
//!
//! Only the entry points needed to smoke-test the build are declared here; the
//! full `enet_host_*` / `enet_peer_*` surface plus the RCEnet `[type][payload]`
//! framing wrapper land next. (Note: this fork has no `enet_linked_version`.)

use std::os::raw::c_int;

extern "C" {
    /// `enet_initialize` — returns 0 on success.
    pub fn enet_initialize() -> c_int;
    pub fn enet_deinitialize();
}

/// Initialize then deinitialize ENet. Confirms the vendored C compiled, linked,
/// and runs. Returns `enet_initialize`'s result (0 = success).
pub fn smoke() -> i32 {
    unsafe {
        let r = enet_initialize();
        enet_deinitialize();
        r
    }
}
