# Rust Client Port — Plan

Goal: a **drop-in replacement for `bin/Client.exe`** written in Rust on a modern
graphics pipeline (**wgpu**), cross-platform (Windows/Linux/macOS), that connects
to and plays against the **unmodified** RCCE2 server and reads the **unchanged**
project files the GUE editor produces. No existing project file (`.bb`, `data/`)
is modified — the port is additive under `client-rs/`.

## Status (2026-05-31) — vertical slice through visual parity DONE

The port is now a playable, feature-rich client built to `bin/ClientRS.exe`
(`compile.bat -r` / `compile.sh -r`), alongside the Blitz `Client.exe`. It logs
into the unmodified server, loads real zones, and plays. Highlights:

- **Transport (Phase 0/5):** pure-Rust 64-bit ENet (`enet-sys`) — full login +
  world-state; FFI was skipped. WASD movement (server-authoritative) + mouse-look.
- **Data (Phase 1):** all parsers done + tested vs real `data/` — Meshes/Textures/
  Sounds/Music catalogs, B3D (mesh + skeleton + ANIM, quaternion-conjugate fix),
  Items/Actors/Spells, area `.dat` (incl. sky/cloud/storm/stars tex ids),
  Interface.dat, Attributes.dat. (~113 unit/round-trip tests workspace-wide, zero
  warnings.)
- **Vertical slice (Phase 2):** wgpu/winit window, real zone scenery + terrain,
  remote actors moving + animating, chat, multi-zone live reload on warp.
- **Gameplay (Phase 3):** combat + floating damage, full inventory (drag/drop/
  equip/drop/eat), dropped-item loot, spell action bar + cooldowns, NPC trade/
  examine/right-click, equipped gear + hair/beard attachments. (Quests/party UI
  minimal.)
- **Interface parity:** the HUD is placed at the **real `Interface.dat` fractional
  coordinates** (vitals, chat, minimap+blips, buffs, action bar + function-button
  row, 46-slot inventory grid, XP bar, compass) with the real GUI/item/spell `.bmp`
  artwork via a textured-quad overlay; clickable buttons + slots; hover tooltips;
  a full character panel (attributes + inventory + spells).
- **Visual parity (Phase 4):** CPU **and GPU** skeletal skinning (GPU opt-in via
  `RCCE_GPUSKIN`, faster + smoother — the perf fix), textured **sky + drifting
  clouds + storm-cloud swap + day/night stars**, rain/snow/storm particles +
  rain/storm audio, day-night fog/ambient, zone music + footstep SFX + volume/mute,
  minimap radar. (RTT radar + post-FX not ported — the 2D minimap + fog substitute.)

**Remaining / open:** confirm the GPU-skinning fps win in a live window
(`RCCE_GPUSKIN=1 RCCE_BENCH=300` vs `RCCE_BENCH=300`) and flip it to default;
`P_Sound` combat/cast SFX (the shipped `Sounds.dat` has no records, so nothing to
play); quests/party UI; Phase 6 true drop-in cutover (rename to `Client.exe` +
`Project Manager.exe` launch). See the `Rust client port` and `Interface.dat HUD
layout` agent memories for the running detail.

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

- ✅ **Phase 0 — Transport spike:** connect to a running server; `P_StartGame` +
  MD5 login; world-state packets. (Went straight to pure-Rust ENet, not FFI.)
- ✅ **Phase 1 — Data foundation (`rcce-data`):** parsers for all `.dat` catalogs
  + B3D + area + Interface/Attributes, tested vs real `data/`.
  - ✅ Blitz LE reader + indexed catalogs (Meshes/Textures/Sounds/Music).
  - ✅ B3D chunk parser → CPU mesh + skeleton + ANIM (+ GPU skin attrs).
  - ✅ Items/Actors/Spells; area `.dat`; Interface.dat; Attributes.dat.
- ✅ **Phase 2 — Vertical slice:** wgpu window → real zone meshes → walk → remote
  players (`P_StandardUpdate`) → chat → live multi-zone reload.
- ✅ **Phase 3 — Gameplay parity:** combat, inventory, spells, trading,
  examine/right-click wired to UI. (Quests/party UI minimal.)
- ✅ **Phase 4 — Visual parity:** skeletal anim (CPU + GPU skinning), particles/
  weather + audio, day-night lighting, textured sky/clouds/stars, minimap.
  (RTT radar + post-FX substituted by the 2D minimap + fog.)
- ✅ **Phase 5 — Cross-platform:** pure-Rust ENet (64-bit `enet-sys`); macOS/Linux
  build scripts (`compile.sh`/`test.sh`). (Path/case normalization as needed.)
- ⬜ **Phase 6 — Drop-in cutover:** currently ships as `bin/ClientRS.exe` ALONGSIDE
  `Client.exe` (opt-in `compile.bat -r`). True cutover (rename + `Project
  Manager.exe` launch) is deferred until parity is signed off.

## Acceptance mapping

- Viability: Phase 2 playable vertical slice on real server data.
- Parity: Phases 3–4.
- "Cross-platform + modern pipeline": Phase 5 (wgpu satisfies the pipeline from
  Phase 2; cross-platform completes at 5).
- "Drop-in `Client.exe`": Phase 6.

## Workspace

`client-rs/` Cargo workspace. Crates: `rcce-data` (parsers), `rcce-net` (ENet +
packet codecs), `rcce-render` (wgpu pipelines, overlay, sky/skin), `rcce-client`
(assets, world, audio, HUD, the `client-window` bin + headless render/probe bins).
Run `cd client-rs && cargo test`; build the client via `compile.bat -r`.
