//! RCCE2 game client library — state model + login flow. The binary
//! (`main.rs`) wires these to the FFI transport; a future wgpu frontend will
//! reuse the same `world` + `login` over the same `Transport` seam.

pub mod assets;
pub mod login;
pub mod world;
