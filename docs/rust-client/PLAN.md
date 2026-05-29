# Rust Client Port — Plan

Goal: a **drop-in replacement for `bin/Client.exe`** written in Rust on a modern
graphics pipeline (**wgpu**), cross-platform (Windows/Linux/macOS), that connects
to and plays against the **unmodified** RCCE2 server and reads the **unchanged**
project files the GUE editor produces. No existing project file (`.bb`, `data/`)
is modified — the port is additive under `client-rs/`.

## Locked decisions (2026-05-29)

| Decision | Choice | Rationale |
|---|---|---|
| Graphics/windowing | **wgpu + winit** | Maps 1:1 onto Blitz3D's explicit entity/camera/RenderWorld model; true cross-platform; WGSL. |
| v1 sequencing | **Vertical slice first** | Login → load one real zone → walk → see other players move → chat. Proves protocol + renderer + data path end-to-end before broadening. |
| Network transport | **FFI `RCEnet.dll` first, swap to pure-Rust ENet later** | Proves all 38 packet codecs against the real server immediately (Windows); cross-platform transport becomes a contained swap behind one trait (Phase 5). |

## Discovery: reusable vs. ground-up

**Reusable as *spec* (reproduce exactly, no code reuse):**
- **Wire protocol** — 38 live packet types (57 defined). Big-endian Bank encoding,
  1-byte-length-prefixed strings, MD5-hashed password. Ref: `docs/protocol/`,
  `src/Modules/{RCEnet,Packets,ClientNet}.bb`.
- **On-disk formats** — B3D meshes (BB3D/NODE/MESH/BONE/ANIM), indexed `.dat`
  catalogs (Meshes/Textures/Sounds/Music — 65535-slot i32 index + records),
  Items/Actors/Spells records, area `.dat`, `Accounts.dat` saves. Ref:
  `src/Modules/{Media,Items,Actors,Spells,ClientAreas,b3dfile}.bb`.
- **Asset codecs** — textures PNG/BMP/JPG (`image`), audio OGG Vorbis
  (`rodio`/`kira`), bzip2 (`bzip2`).

**Ground-up (replace the Blitz3D + DirectX 7 stack):**
- 3D renderer (~8.2k lines) → wgpu.
- 2D UI / Gooey, 15+ windows (~9.6k lines) → custom-draw on wgpu (egui for menus optional).
- Networking (~2k lines) → `rcce-net`.
- World/zone + animation + environment (~3.5k) → `rcce-world`.
- Data loaders (~3.1k) → `rcce-data` (**Phase 1, in progress**).
- Audio → `rodio`.

**Confirmed scope reducers:**
- **Client links NO scripting VM.** `Client.bb` includes none of `briskvm` /
  `ScriptingCommands` / `RC_Standard_Invoker`. BVM is server-only; the client
  merely renders packet-driven dialog/input/progress UI. The Rust client needs
  zero scripting engine.
- `briskvm.dll`, MySQL DLLs, DX7 stack: not client-portable and/or not client
  deps — irrelevant to the port.

**The one real risk:** what `RCEnet.dll` writes on the UDP wire. `.decls` shows a
12-function message-queue API (reliable-flag + channels ⇒ almost certainly ENet
underneath). FFI-first (Phase 0) removes it from the critical path; Phase 5
either reimplements over a Rust ENet crate or, if available, ports RCEnet source
from the BlitzForge submodule.

## Critical correctness note — byte order

- **File I/O** (`ReadInt`/`ReadShort`/`ReadFloat`, all `.dat`/save files) is
  **little-endian** native. → `rcce-data::BlitzReader`.
- **Wire** (`RCE_StrFromInt$`) is **big-endian**. → `rcce-net` codec.
- `.dat` strings: **4-byte LE length** + bytes (`MediaReadFilename$`, Media.bb:23).
  Wire strings: **1-byte length** + bytes. Do not conflate.

## Phases

- **Phase 0 — Transport spike:** FFI-load `RCEnet.dll`; connect to a running
  server; complete `P_StartGame` + MD5 login; log received packet types.
- **Phase 1 — Data foundation (`rcce-data`) [in progress]:** parsers for all
  `.dat` catalogs + B3D + area + `Accounts.dat`, round-trip-tested vs real `data/`.
  - ✅ Blitz LE reader + indexed catalog (`Meshes.dat`: 89 entries parsed clean).
  - ⬜ Textures/Sounds/Music catalogs (same index shape, different records).
  - ⬜ B3D chunk parser → CPU mesh + skeleton + anim.
  - ⬜ Items/Actors/Spells catalogs; area `.dat`; `Accounts.dat`.
- **Phase 2 — Vertical slice:** wgpu window → one real zone's meshes → walk →
  remote players (`P_StandardUpdate`) → chat (`P_ChatMessage`). **Viability proof.**
- **Phase 3 — Gameplay parity:** combat, inventory, spells, trading, quests,
  party — remaining ~30 packet handlers wired to UI.
- **Phase 4 — Visual parity:** skeletal anim blending, particles/weather,
  lighting/day-night, radar RTT, post-FX (or tasteful modern equivalents).
- **Phase 5 — Cross-platform:** swap FFI transport → pure-Rust ENet; path/case
  normalization; Linux + macOS builds.
- **Phase 6 — Drop-in cutover:** binary named `Client.exe`, same config files,
  launches from `Project Manager.exe` unchanged.

## Acceptance mapping

- Viability: Phase 2 playable vertical slice on real server data.
- Parity: Phases 3–4.
- "Cross-platform + modern pipeline": Phase 5 (wgpu satisfies the pipeline from
  Phase 2; cross-platform completes at 5).
- "Drop-in `Client.exe`": Phase 6.

## Workspace

`client-rs/` Cargo workspace. Crates added to `members` as each phase opens so it
always builds. Current: `rcce-data`. Run `cd client-rs && cargo test`.
