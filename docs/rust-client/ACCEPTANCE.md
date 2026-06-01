# Rust Client — Feature-Parity Acceptance Criteria

The source of truth for "parity" is the **behavior of `bin/Client.exe`** as defined in `src/Client.bb` + `src/Modules/*.bb`, not guesswork. Every criterion below is grounded in a file:line citation into the Blitz source and is written as **input → expected observable result** so it can be verified.

## How to read this

- **Status tags** (audited 2026-06-01 against `client-rs/` HEAD `3de64b7`):
  - `DONE` — implemented and matches the reference (verified at least by static read; ✅ = also verified by a headless PNG / test / live run, with the evidence noted).
  - `PARTIAL` — present but incomplete or divergent from the reference.
  - `MISSING` — not implemented.
- **Evidence ladder** (weakest → strongest, per the Praxis evidence doctrine): assertion < static read < unit test < headless PNG < live run. A criterion is only marked `DONE ✅` when backed by PNG/test/live evidence; `DONE` without ✅ means static-read confidence only and still needs a runtime check.
- **Verification** column names the concrete check that flips the status (env-var headless capture, `cargo test`, probe bin, or live server run).
- IDs are stable handles (`MENU-3`, `MOVE-1`, …) that `PLAN.md` phases reference.

Headless verification primitives (confirmed in `client_window.rs`):
- `RCCE_SHOT=<path>` `RCCE_SHOT_FRAME=N` — capture to PNG and `exit(0)`. **Mode-dependent**: menu path default frame **45**, world path default frame **150**.
- `RCCE_AUTOLOGIN` (implied by `RCCE_BENCH`/`RCCE_AUTOWALK`), `RCCE_AUTOSUBMIT` (auto-submit login → char-select), `RCCE_AUTOENTER` (auto-enter first character → world).
- `RCCE_AUTOWALK` (headless movement self-test), `RCCE_BENCH=N` (`[bench] avg fps`), `RCCE_GPUSKIN`.
- Probe bins: `move_test`, `combat_test`, `chat_test`, `interact_test`, `actor_render`, `zone_render`, `anim_probe`, `tex_diag`, `appearance_probe`.
- Live data: `bin\Server.exe -UNLOCK` (UDP 25000, account `rustbot`; restart to clear an `L`/already-online state).

A live server reference run uses two human-driver accounts (the "other human" and the stag mob) in the starter zone; for headless checks, drive past the menu with the AUTO* vars and capture at a known frame.

---

## A. Menu / Login / Character-Select

The real client runs the entire menu inside its own `Graphics3D` context (`MainMenu.bb`, driven by `Client.bb:167 RunMenu()`), then tears it down before the game starts. The menu is **NOT** the gameplay zone — see MENU-SCENE below.

| ID | Criterion (input → observable) | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| MENU-1 | Client boots to a **login screen** (Username / Password[masked] / Email fields + Login button) over the menu backdrop, not straight into the world | DONE | `login.rs`, `client_window.rs:1442-1637`; ref `MainMenu.bb:430-472` | `RCCE_SHOT` menu capture |
| MENU-2 | Typing in a field echoes; password renders masked; Tab cycles Name→Pass→Email; Enter in password = Login | PARTIAL | ref `MainMenu.bb:466,771-783`; Rust has fields but Tab-cycle / Enter-in-pass not confirmed | manual + `RCCE_SHOT` |
| MENU-3 | Login sends `P_VerifyAccount` `[1:nameLen][name][1:md5Len][MD5(password)]`; reply byte `Y`=ok(+charlist), `N/P`=bad creds, `B`=banned, `L`=already-online | DONE | ref `MainMenu.bb:805,828-836`; `auth.rs`/`net.rs` | live login to `Server.exe` |
| MENU-4 | On success the client streams the full dataset via `P_FetchActors` (attributes/damage/environment/factions/items/actors) before char-select | DONE | ref `MainMenu.bb:872-1143`; `fetch.rs` | live |
| MENU-5 | **Character-select** shows the roster (name + race/gender), with Create / Delete / Enter-World actions | DONE | `client_window.rs:1442-1637`; ref `MainMenu.bb:1553-2011` | `RCCE_AUTOSUBMIT` + `RCCE_SHOT` |
| MENU-6 | Create-character flow: race picker (from `Playable` actors), gender/class/hair/face/beard/clothes pickers, attribute-point spend, name field; sends `P_CreateCharacter` | PARTIAL | ref `MainMenu.bb:2137-2477`; Rust `create_char` exists — pickers/point-spend coverage unverified | live create |
| MENU-7 | Delete-character: confirm box → `P_DeleteCharacter` `[name][pw]+slot`; reply replaces the roster | DONE | ref `MainMenu.bb:1765-1785`; `delete_char` | live delete |
| MENU-8 | Enter-world: `P_FetchCharacter` streams C1/C3/S/Q blocks; then `RCE_Disconnect` menu socket, reconnect game socket, `P_StartGame` `[name][pw]+slot` | DONE | ref `MainMenu.bb:1852-2006`, `ClientNet.bb:33-112`; `enter_world` | live enter |
| MENU-9 | **Two-phase connect**: menu uses a non-game socket (`"X"`, port 11001+); game uses the player-named socket (port 11002+). Reproduce both | PARTIAL | ref `MainMenu.bb:749`, `ClientNet.bb:33-42`; confirm Rust does the disconnect+reconnect | live |
| MENU-SCENE | **The dedicated 3D menu scene** (THE known gap): the **selected character's actor mesh** posed at world `(30, ground, 100)` playing `Anim_Idle`, camera framing the torso — **NOT** a spectator orbit of the gameplay zone | DONE ✅ | `render_menu` now clears the startup zone geometry (`set_scene(&[])`, forces a fresh world reload on enter), builds the highlighted `CharInfo` via a menu `World` + `build_actors` (template param added so the char's real `actor_id` poses), and circles a turntable camera around the `(30,0,100)` anchor. ref `MainMenu.bb:80-92,1706-1727,2020-2086` | `RCCE_AUTOSUBMIT` + `RCCE_SHOT` frame 150: PNG shows the posed "Ructaros/Human(M)" model against a backdrop, no terrain — read & confirmed 2026-06-01 |
| MENU-SCENE-b | Exact backdrop fidelity: dark-blue fogged void (`0,51,102`) + full-screen backdrop quad per screen (`Login.png`/`Character Selection.png`/`EULA.png`) + optional `Set.b3d` diorama at `(-210,-35,-145)` scale 30 + dolly-to-head on selection (currently the zone sky shows as the backdrop and the camera is a turntable, not a dolly) | PARTIAL | ref `MainMenu.bb:441-453,1563-1576,2037-2086` | follow-up — `RCCE_SHOT` showing the backdrop PNG + void |
| MENU-10 | Menu music: `Data\Music\Menu.ogg` looped while in menu, stopped on enter-world | PARTIAL | ref `MainMenu.bb:99-103,136`; Rust audio plays zone music — menu music unconfirmed | live audio |
| MENU-11 | Resolution-aware layout: 16:9 (`ResolutionType=1`) vs 4:3 gadget positions both reproduced | DEFERRED | ref `MainMenu.bb:44-46`; Rust uses its own HUD coords — low priority vs. scene parity | n/a alpha |
| MENU-12 | Last-username persistence: pre-fill account from `Data\Last Username.dat` (obfuscated password line) | PARTIAL | ref `MainMenu.bb:728-733,861-868`; `RCCE_USER` pre-fills instead | manual |
| MENU-13 | EULA gate (skipped if `EULA.txt` empty) + Server-Selector screen (skipped if no `Server Selector.dat`) | DEFERRED | ref `MainMenu.bb:354-427,2908-2997`; optional, both gates default-off in starter project | n/a alpha |

---

## B. Local Player Movement & Input

Movement is **destination-based**, not velocity-based: input sets `Me\DestX#/DestZ#`, and the shared `UpdateActorInstances` loop walks the actor toward it. Key constants: `KeyboardMoveDistance#=3.0`, base speed `1.5 units/frame@30fps` (×2 run, ÷2 back), move/stop threshold **dist-to-dest = 2.0**, turn rate **3°/frame@30fps**, `NetworkMS=200ms` send cadence.

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| MOVE-1 | Forward key projects a point `KeyboardMoveDistance#*(1+IsRunning)` ahead of facing and `SetDestination`s it; running doubles the projected distance | PARTIAL | ref `Interface3D.bb:475-486`; Rust WASD sets a camera-relative intent (`client_window.rs:2049-2061`) — not the dest-projection model | `move_test`, live |
| MOVE-2 | Backward key: `IsRunning=False` (cannot run backward), projects `−dist`, sets `WalkingBackward=True` | PARTIAL | ref `Interface3D.bb:493-507` | live |
| MOVE-3 | Turn-left/right rotate the character 3°/frame@30fps in place, preserving destination relative to new facing | PARTIAL | ref `Interface3D.bb:514-534`; Rust has Q/E discrete turn (`:1283-1288`) | live |
| MOVE-4 | Run modifier: `IsRunning = Run-key-down OR AlwaysRun`; AlwaysRun toggles on its key | PARTIAL | ref `Interface3D.bb:483,416` | live |
| MOVE-5 | **Click-to-move** (THE known gap): left-click on terrain (no actor under cursor) unprojects the ground point and walks the player there, stopping within the 2.0-unit dist-to-dest threshold; manual WASD overrides | DONE ✅ | Added `rcce_render::unproject_ground` (screen ray ∩ player ground plane, inverts the GPU's `clip = vp*world`) + a `move_target` the per-frame movement steers toward (`overlay.rs`, `client_window.rs` world_pick + movement block). ref `Interface3D.bb:949-1033` | unproject unit tests green; live (`RCCE_CLICKMOVE=160`): player walked Z 88.4→83.9 onto the clicked target 83.6 and stopped, X unchanged — logs + PNG read 2026-06-01 |
| MOVE-5b | Click marker entity at the hit point; hold-to-move (repeat each frame while held) | PARTIAL | core walk-there done; the visual click-marker and press-hold-repeat are deferred (cosmetic / refinement) | follow-up |
| MOVE-6 | Double-click an actor → run to it; double-click ground → set `IsRunning=True` to existing dest | MISSING | ref `Interface3D.bb:830-844,1022` | live |
| MOVE-7 | Jump: jump-key when grounded sets `Me\Y# = JumpStrength#*Gravity# (=0.1)`, plays `Anim_Jump`, sends `P_Jump` immediately | MISSING | ref `Interface3D.bb:457-463` | live |
| MOVE-8 | `SetDestination` blocks walking-character destinations that fall inside a Water volume below its surface | MISSING | ref `Client.bb:998-1011` | live (needs water) |
| MOVE-9 | Mouse-look: hold RMB → third-person rotates `CamYaw#`, first-person rotates the character; pitch clamped [−70,85] | PARTIAL | ref `Interface3D.bb:602-641`; Rust RMB mouse-look exists (`:1393-1403`) | live |
| MOVE-10 | Outbound `P_StandardUpdate` (22 bytes: DestX,DestZ,Y,X,Z floats + IsRunning,WalkingBackward) sent at most every 200ms **only when position changed** | PARTIAL | ref `ClientNet.bb:1798-1805`; Rust `movement_packet` throttled (`:2114-2118`) — confirm field layout + change-gate | `wire_probe`, live |

---

## C. Animation State Machine

Anim constants are slot indices into a per-AnimSet table (`Anim_Idle=125, Anim_Walk=149, Anim_Run=148`, swim 145-147, ride 142-144, jump 126, death 127-129, hit 130-132, attack 138-141). `PlayAnimation(AI,Mode,Speed#,Seq)` scales `Speed#` by `(AnimEnd−AnimStart)*AnimSpeed#[seq]`; Mode 1 = loop, Mode 3 = once; negative speed plays in reverse.

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| ANIM-1 | **Local player plays Walk/Run while moving** (THE known gap): when dist-to-dest > 2.0, play `Anim_Run` (running) / `Anim_Walk` fwd @0.04 / `Anim_Walk` back @−0.02; return to `Anim_Idle` @0.003 when stopped | DONE ✅ | Fixed: the local-player push now threads this frame's `moving`/`run` (was hardcoded `false,false`) through `build_actors` + `dyn_hash` (`client_window.rs:682,703,2139,2142`), routing Me through the same Walk/Run/Idle clip selector as remote actors. ref `Client.bb:594-728` | `RCCE_AUTOWALK` + `RCCE_SHOT` frames 300 & 313 vs idle: legs in a walk stride and advancing between frames (player also translates) — PNGs read & confirmed 2026-06-01 |
| ANIM-2 | Remote actors animate walk/run/idle from their replicated `IsRunning`/dest delta | DONE | `client_window.rs:684-688,572-582`; ref `Client.bb:639-672` | `RCCE_SHOT` of a moving remote actor |
| ANIM-3 | Anim selection is gated by `CurrentSeq`/`Animating()` so a playing clip isn't restarted every frame | PARTIAL | ref `Client.bb:594-595,639`; confirm Rust clip-switch hysteresis | static + live |
| ANIM-4 | Swim anims underwater: `Anim_SwimFast`(run)/`Anim_SwimSlow`(walk)/`Anim_SwimIdle`(stop) | MISSING | ref `Client.bb:625-637,720-727` | live (needs water) |
| ANIM-5 | Mounted rider anims: `Anim_RideRun/RideWalk/RideIdle` mirror the mount's gait | MISSING | ref `Client.bb:599-610,716-718` | live (needs mount) |
| ANIM-6 | Idle fidget: ~1/1000 frames while idle, play a random `Anim_LookRound..Anim_Yawn` once | MISSING | ref `Client.bb:706-708` | live observation |
| ANIM-7 | Jump anim (`Anim_Jump`, mode 3 @0.05) on local jump and on remote `P_Jump` | MISSING | ref `Interface3D.bb:460`, `ClientNet.bb:241-246` | live |
| ANIM-8 | Combat anims: attack (138-141 by weapon), hit-react (130-132), parry (133-137), death (127-129) play on `P_AttackActor`/`P_ActorDead` | MISSING | ref `ClientNet.bb:1131-1203,1096` | live combat |
| ANIM-9 | `PlayAnimation` speed scaling reads `AnimStart/AnimEnd/AnimSpeed` from `Animations.dat`; reproduce the per-clip speed normalization | PARTIAL | ref `Animations.bb:41-66`; `anim.rs` parses ANIM — confirm speed scaling | `anim_probe`, `cargo test` |

---

## D. Camera

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| CAM-1 | Third-person boom: pivot at player + `CamHeight#`, rotate `(CamPitch, CamYaw+180)`, push back `CamDist#`, smooth-follow curve 6.0·Delta | DONE | ref `Client.bb:846-877`; `render` camera path | `RCCE_SHOT` world |
| CAM-2 | Camera collision: `LinePick` from player to desired cam point; on hit snap to pick point (keep player visible); flip 180° if shoved within 2.0 of Head | PARTIAL | ref `Client.bb:864-887`; Rust has per-zone occluder spheres (PLAN note) | live near a wall |
| CAM-3 | Zoom: wheel `CamDist# ∓= MouseZSpeed*1.5` clamp [5,50]; keyboard zoom clamp [3,50] | PARTIAL | ref `Interface3D.bb:643-657` | live |
| CAM-4 | First-person mode toggle (`Key_ChangeViewMode` when `ViewMode=2`): cam at Head joint, pitch eased, yaw follows character | MISSING | ref `Client.bb:889-893` | live |
| CAM-5 | MMB snaps camera behind character (`CamYaw=EntityYaw(Me)`, `CamPitch=0`) | MISSING | ref `Interface3D.bb:444-447` | live |
| CAM-6 | Underwater: cam below a water plane tints cls/fog to water color, near/far 1/50, hides sky/stars/clouds; restores on surfacing | MISSING | ref `Client.bb:895-922` | live (needs water) |

---

## E. Targeting / Interaction / Dialog

`PlayerTarget` = `Handle(ActorInstance)` of the selection (0=none), re-resolved every frame via `Object.ActorInstance(PlayerTarget)`. `InteractRange#=20.0`. Controls are rebindable; shipped defaults: Select/MoveTo=LMB, TalkTo=RMB.

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| TGT-1 | Left-click an actor selects it (`PlayerTarget` set via the entity's name→handle), shows `ActorSelectEN` ground decal under its feet, and opens the Char-Interaction window (target HP/faction/level/reputation) | DONE ✅ | ref `Interface3D.bb:792-882,1056-1074,3229-3293`; Rust selects nearest projected actor (`world_pick`). **Phase 4a: fixed `overlay::project` — it was transposed (column-major matrix indexed as row-major), misplacing every pick AND nameplate/floater/loot label; now `clip = vp*world`, unit-tested (centre-maps-to-centre + round-trips with the live-verified `unproject_ground`).** **Phase 4b: target now gets a corner-bracket selection reticle (feet→head, the on-screen analogue of `ActorSelectEN`) + a top-centre Char-Interaction panel (name + HP bar).** Omitted: faction/level/reputation (not yet parsed into the `Actor` struct) | DONE ✅ — `RCCE_SELECT=160` + HUD-inclusive `RCCE_SHOT`: panel shows "TEST HUMAN" + HP, read 2026-06-01 |
| TGT-2 | Selection highlight follows the target each frame; clears + hides when target stale/dies | DONE ✅ | the reticle + panel re-project/redraw from live actor data every frame; `on_actor_dead` clears `self.target` so both vanish on death. ref `Interface3D.bb:1056-1074`, `ClientNet.bb:1105` | same `RCCE_SELECT` PNG |
| TGT-3 | **Right-click context menu** (THE known gap): single left-click on an actor pops an "Actions" menu at the cursor with Interact, Attack (non-players), Examine, Trade (non-players) | DONE ✅ | `ContextMenu` type + `exec_menu_action`; single-click opens it (`world_pick`), `hud_click` gives it click priority, drawn over the HUD. Attack/Trade gated on `!is_player` (Actor lacks Aggressiveness/TradeMode). Unit-tested (per-actor items, hit-test rows, screen clamp). ref `Interface3D.bb:845-880,660-717` | `RCCE_SELECT=160` PNG: gold-bordered Interact/Attack/Examine/Trade menu over "Test Human", read 2026-06-01 |
| TGT-4 | Context **Interact** sends `P_RightClick [2]RuntimeID` → server runs the NPC `Main` script | DONE ✅ | menu Interact (+ double-click) → `exec_menu_action` sends RIGHT_CLICK. ref `Interface3D.bb:668,748,782` | server-side dialog reply verified in TGT-5 |
| TGT-5 | **NPC dialog window** (THE known gap): server `P_Dialog` sub-protocol `N`(new)/`T`(text)/`O`(options)/`C`(close) opens a window with wrapped text + green clickable options; selecting an option sends `P_Dialog "O" [4]scriptHandle [1]opt` | DONE ✅ | `World::on_dialog` parses N/T/O/C into a `Dialog` + queues the "N"/"T" acks (via `pending_sends`); `client_window` draws the left-side window (title + wrapped text + green numbered options) and hit-tests option clicks (`dialog_option_packet`, new `DIALOG` const 21). ref `ClientNet.bb:1027-1068`, `Interface3D.bb:45-162,1561-1586` | unit test `dialog_new_text_options_close` (parse + acks) green; `RCCE_DIALOGTEST` PNG shows the window "Greetings, traveler" + 3 green options, read 2026-06-01. (Live scripted dialog needs an NPC with a dialog `Main` script — the starter "Test Human" has none; `RCCE_INTERACT` fires `RIGHT_CLICK` for that path.) |
| TGT-6 | Context **Examine** sends `P_Examine [2]RuntimeID`; **Trade** sends `P_Trade [2]RuntimeID` | DONE ✅ | menu Examine→`EXAMINE`, Trade→`TRADE` (new packet const 62) via `exec_menu_action`. ref `Interface3D.bb:694,703` | menu rows present in the `RCCE_SELECT` PNG |
| TGT-7 | Cycle-target key walks to the next NPC combatant (`Aggressiveness<3`, `RNID=0`) | MISSING | ref `Interface3D.bb:419-438` | live |
| TGT-8 | Free-text input dialog (`TextInput`) and timed `P_ProgressBar` prompts render and reply | MISSING | ref `Interface3D.bb:1587-1599`, `ClientNet.bb:151-167` | live (scripted) |
| TGT-9 | Pick-up a dropped item in range (`<25`): `P_InventoryUpdate "P" [4]serverHandle [1]slot`; "No inventory space" otherwise | DONE | ref `Interface3D.bb:911-914`; world loot handling | live |

---

## F. Combat

Auto-attack on a flagged target: `AttackTarget=True` + `PlayerTarget` drives `UpdateCombat`. No per-swing click.

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| CBT-1 | **Attack a mob** (THE known gap, e.g. the stag): set `attacking` (Attack menu button / Attack key), then auto-swing on `CombatDelay` cooldown via `P_AttackActor [2]RuntimeID` while in range | DONE ✅ | per-frame combat loop (`attacking` flag + `last_attack` + pure `combat_step`): chase out of range, stop + swing in range every `COMBAT_DELAY_MS` (1500). Cleared on target death/vanish or manual move. ref `ClientCombat.bb:16-79` | unit test `combat_step_decisions` green; **live `RCCE_ATTACK=150`: the player chased Z 88→21 to the target then logged 7 `[combat] swing` at dist 4.4 paced ~1.5s, stationary in range — confirmed 2026-06-01** |
| CBT-2 | Range gate: melee `MaxRange#≈4.5`; out of range → chase via the move-to system; in range → stop | DONE ✅ | `combat_step` (Chase if dist>MELEE_RANGE else Swing/Wait) + the loop sets/clears `move_target`. ref `ClientCombat.bb:37-64` | same live run: chased then stopped at dist 4.4. (Ranged weapon `Range−0.5` deferred — no weapon-range data parsed yet.) |
| CBT-3 | Render `P_AttackActor` broadcast: `H`(I hit)/`Y`(hit me)/else — attacker attack-anim, target hit-anim, HP subtract, blood-spurt emitter; miss → parry anim | PARTIAL | ref `ClientNet.bb:1115-1206`; Rust records hits → floating numbers (`world.rs:396-410`) but no anims/blood | live |
| CBT-4 | Floating damage numbers (`DamageInfoStyle=3`) rise over the actor's head (red=taken, green=dealt) and expire | DONE | ref `ClientCombat.bb:147-229`; `floaters.rs`, drawn `client_window.rs:2438-2450` | `combat_test` / live |
| CBT-5 | Chat-line damage style (`DamageInfoStyle=2`): "You hit X for N type damage!" colored | PARTIAL | ref `ClientCombat.bb:150-168` | live |
| CBT-6 | Death (`P_ActorDead`): play random death anim, dismount, "You killed X!", set HP 0 + fade, clear `PlayerTarget`/`AttackTarget`, free dialogs | PARTIAL | ref `ClientNet.bb:1071-1112`; Rust `on_actor_dead` clears state, no death anim | live |

---

## G. Inventory & Equipment

46 slots: 0-13 equipment, 14-45 backpack. Stack ceiling 32767. Mouse-slot carrier for drag. Packets `P_InventoryUpdate "D"/"A"/"S"/"P"`.

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| INV-1 | 46-slot grid at real `Interface.dat` fractional coords with real item thumbnails + amount labels | DONE ✅ | `client_window.rs:2730-2799`; ref `Interface3D.bb:3735-3763`; HUD-layout memory | prior `RCCE_SHOT` evidence |
| INV-2 | Live inventory model updated by `P_InventoryUpdate` G/T/H/R/D/P/O subtypes | DONE | `world.rs:431-525`; ref `ClientNet.bb:1277-1450` | live |
| INV-3 | Pick-up rules: amount 1 or Ctrl = whole stack; Shift = one; else Amount dialog | PARTIAL | ref `Interface3D.bb:2484-2621`; confirm modifier coverage | live |
| INV-4 | Drag/drop: same-slot put-back, identical-stack merge (`InventoryAdd`), different-slot swap (`InventorySwap`); drop-to-ground (`InventoryDrop`→`"D"`) | DONE | `client_window.rs:1726-1775`; ref `Inventories.bb:99-200` | live |
| INV-5 | Use/eat: `UseItem` → potion `P_EatItem`, image item opens `WItemWindow`, other `P_ItemScript`; backpack weapon/armour auto-equips to a free matching slot | PARTIAL | ref `Interface3D.bb:4132-4261`; confirm image-item + auto-equip | live |
| INV-6 | Equipped gear attaches as hand/body meshes on the actor + hair/beard | DONE | `client_window.rs:645-673`; appearance memory | `appearance_probe` |
| INV-7 | Item tooltip on hover >1s: name/type/damage/value/mass/stackable/restrictions/description | PARTIAL | ref `Interface3D.bb:1918-1984` | live hover |
| INV-8 | Stack amounts clamp to 32767 (16-bit wire/save) | DONE | ref `Inventories.bb:40-47` + inventory memory | `cargo test` |

---

## H. Spells & Action Bar

`SpellCharge[]` is keyed by **spell ID 0-999** (must match server decrement). Action bar = 36 logical slots / 3 pages / 12 visible; slot value `>0`=item, `0`/`65535`=empty, `<0`=spell.

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| SPL-1 | Spellbook window (`K`): memorised-page (10 slots) + paged known-spells (alphabetical), icons + name/rank/description | PARTIAL | ref `Interface3D.bb:3578-3598,3081`; Rust has action bar, spellbook window coverage unclear | live |
| SPL-2 | Cast: `P_SpellUpdate "F" [2]spellID [+2 targetRid]` when `SpellChargeReady`; else "not recharged" | DONE | `net.rs:34-41`, `client_window.rs:1253-1274`; ref `Interface3D.bb:1543` | live |
| SPL-3 | Cooldown/charge keyed by spell ID; predictive decrement 100/100ms; display shading | DONE | `client_window.rs:2869-2927`; ref `Interface3D.bb:386-395`, SpellCharge memory | live |
| SPL-4 | Memorise (`P_SpellUpdate "M"`) with a 60-tick progress bar when `RequireMemorise`; un-memorise (`"U"`) | MISSING | ref `Interface3D.bb:1466-1502,1368-1370` | live |
| SPL-5 | Action bar: assign item (`"I"`), assign spell (`"S"`), clear (`"N"`); 12 slots fire on F1-F12 / click; 3-page swap | PARTIAL | ref `Interface3D.bb:1077-1280,1169-1212`; Rust fires 1-9, paging/assign unclear | live |
| SPL-6 | Action bar loaded from the `P_StartGame` payload (3 slot-groups) | PARTIAL | ref `ClientNet.bb:62-106` | live |
| SPL-7 | Incoming `P_KnownSpellUpdate` A/D/L (add/remove/level) updates known spells + resort | PARTIAL | ref `ClientNet.bb:823-923` | live |
| SPL-8 | Render own cast effects / projectiles (currently send-only) | MISSING | audit §5c: `SPELL_UPDATE` send-only | live |

---

## I. Chat

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| CHAT-1 | Open chat (Enter / `/`), type, send `P_ChatMessage` (raw text; `/commands` parsed server-side) | DONE | `client_window.rs:1080-1123`; ref `Interface3D.bb:2190-2213` | `chat_test` / live |
| CHAT-2 | Incoming `P_ChatMessage` color sentinels (254=yellow,253=red,252=purple,251=green,250=RGB) + `<<self>>`=blue; render in chat log | PARTIAL | ref `ClientNet.bb:1219-1252`; Rust strips control bytes, no colors (`world.rs:607-616`) | live |
| CHAT-3 | Chat scrollback ring buffer (2000) + history mode + up/down scroll | MISSING | ref `Interface3D.bb:3012-3057` | live |
| CHAT-4 | Chat bubbles over actors (`P_BubbleMessage`, or `<`-prefixed when `UseBubbles>1`) | MISSING | ref `ClientNet.bb:1209-1252`, `Interface3D.bb:219` | live |

---

## J. Trade

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| TRD-1 | `P_OpenTrading` opens a modal trade window (mine 32 / theirs 32), partner type `N`(NPC)/`S`(scenery)/`P`(player) | PARTIAL | ref `ClientNet.bb:582-668`; `trade.rs`, `client_window.rs:2828-2862` (buy-only) | live vendor |
| TRD-2 | Select items (amount 1/Shift=1/Ctrl=max/else Amount dialog), running cost, Accept sends packed `P_OpenTrading` | PARTIAL | ref `Interface3D.bb:2297-2372`; Rust buys 1-9 | live |
| TRD-3 | Player↔player live sync (`P_UpdateTrading`), cost up/down, mirror partner offer | MISSING | ref `Interface3D.bb:2404-2418`, `ClientNet.bb:533` | live (2 players) |
| TRD-4 | Cancel/close (`P_OpenTrading ""`) + forced `P_CloseTrading` | PARTIAL | ref `Interface3D.bb:2298-2304`, `ClientNet.bb:573` | live |

---

## K. Quests & Party

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| QST-1 | Quest log window (`L`): per-quest name+status, completed-filter, paging; colored by status RGB; "Completed" gold | MISSING | ref `Interface3D.bb:3634,3979`; Rust prints "not yet implemented" (`:1661-1663`) | live |
| QST-2 | Incoming `P_QuestLog` N/U/D (new/update/delete) updates the log + "Quest log updated" | MISSING | ref `ClientNet.bb:955` | live |
| PTY-1 | Party window (`P`): up to 7 member names, click → `/p <name> ` whisper, Leave → `/leave` | MISSING | ref `Interface3D.bb:3567,1647-1666`; Rust no-op | live (2 players) |
| PTY-2 | Incoming `P_PartyUpdate` name list updates the roster | MISSING | ref `ClientNet.bb:483` | live |

---

## L. HUD (always-on)

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| HUD-1 | Vitals bars (Health/Energy + any attribute) at real `Interface.dat` coords, value/max numbers, hover tooltip | DONE ✅ | `client_window.rs:2541-2562`; HUD-layout memory | prior PNG |
| HUD-2 | XP bar scaled by `XPBarLevel/255`, driven by `P_XPUpdate` | DONE | `client_window.rs:2945-2961`; ref `Interface3D.bb:3166` | live |
| HUD-3 | Money via multi-denomination `Money$` (shown in Inventory/Trade only) | PARTIAL | ref `Client.bb:1160-1177`; Rust level/gold readout (`:2570`) | live |
| HUD-4 | Function-button row (Chat/Map/Inventory/Spells/Character/Quests/Party/Menu) toggles panels | PARTIAL | ref `Interface3D.bb:3499-3519`; Rust HUD buttons partly stubbed | live |
| HUD-5 | Compass strip driven by player yaw | DONE | `client_window.rs:2376-2395`; ref `Interface3D.bb:3068` | PNG |
| HUD-6 | Buff/debuff icons from `P_ActorEffect` A/R, hover name | DONE | `client_window.rs:2491-2504`; ref `Interface3D.bb:3207` | live |
| HUD-7 | Nameplates + HP bars over actors | DONE | `client_window.rs:2398-2434` | PNG |
| HUD-8 | Character sheet (`C`): name, reputation, level, XP, paged attributes | PARTIAL | ref `Interface3D.bb:3644-3665,1721-1797` | live |

---

## M. Environment

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| ENV-1 | Day/night cycle advances local clock (`60000/TimeFactor` ms/min, `TimeFactor=10` → 2.4h/day); sky/stars crossfade at dusk/dawn; light/fog/ambient shift | DONE | `daynight.rs`; ref `Environment.bb:203-233`, `Environment3D.bb:381-426`; `RCCE_PHASE`/`RCCE_DAYNIGHT_SECS` | `RCCE_PHASE` + `RCCE_SHOT` day vs night |
| ENV-2 | Weather (`P_WeatherChange` + `P_ChangeArea` byte): Sun/Rain/Snow/Fog/Storm/Wind — particles, fog target, cloud swap, audio | DONE | `weather.rs`; ref `Environment3D.bb:157-235` | `RCCE_SHOT` rain/snow |
| ENV-3 | Sky/clouds/stars: textured skydome, clouds drift (`TurnEntity 0.05·Delta`), storm-cloud swap, night stars; per-zone tex IDs from area .dat | DONE | `world_view.rs:215-284`; ref `ClientAreas_FE.bb:349-399` | PNG |
| ENV-4 | Water: per-zone translucent plane with scrolling UV (+ optional bump/foam); collision box for walkers | MISSING | ref `ClientAreas_FE.bb:704-785`, `Environment3D.bb:266-295`; audit: no water in render | live (water zone) |
| ENV-5 | Lightning during Storm: random `ScreenFlash` white + thunder SFX on final flash | MISSING | ref `Environment3D.bb:316-330` | live storm |
| ENV-6 | Screen flash (`P_ScreenFlash` R/G/B/alpha/length/texID) full-screen quad, linear decay | MISSING | ref `Client.bb:1112-1157`, `ClientNet.bb:679-686` | live (scripted) |

---

## N. Audio

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| AUD-1 | Zone music (`P_Music`) loops, frees previous channel; loading-screen music during zone load | DONE | `audio.rs`; ref `ClientNet.bb:758-769` | live |
| AUD-2 | Sound zones (radius-triggered, 3D/2D by filename last-byte flag, repeat timer, fade-out on exit) | PARTIAL | ref `Client.bb:938-993`; confirm Rust sound-zone parsing | live |
| AUD-3 | Footstep SFX at gait extremes, wet/dry × gender, 3D positional, suppressed underwater | DONE | `audio.rs`; ref `Client.bb:677-701` | live (footsteps fire only when moving+following) |
| AUD-4 | Combat/cast/speech SFX (`P_Sound`, `P_Speech`) 3D from actor; note shipped `Sounds.dat` may be empty | PARTIAL | ref `ClientNet.bb:733-769`, `Actors3D.bb:790-803` | live |
| AUD-5 | Weather audio: rain/wind looped, thunder one-shots; volume/mute | DONE | `weather.rs`; ref `Environment3D.bb:129-154` | live |

---

## O. Radar / Projectiles / Zone

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| RAD-1 | Minimap/radar showing actors + loot, oriented to player | DONE | `radar.rs`, `client_window.rs:2453-2488` | PNG |
| RAD-2 | (Reference is an RTT top-down fog-of-war map with persistence; Rust substitutes a blip radar) | DEFERRED | ref `Radar.bb:21-370`; documented substitution | n/a alpha |
| PRJ-1 | Projectiles (`P_Projectile`): spawn from source, homing tracks target / non-homing snapshots target pos, RP emitters, impact at dist<2 | MISSING | ref `ClientNet.bb:217-238`, `Projectiles3D.bb:11-90`; audit: no projectile code | live (spell/ranged) |
| ZON-1 | Zone warp (`P_ChangeArea`): teardown remote actors/projectiles/loot, save radar fog, reload scenery/water/emitters/sound/terrain, reposition Me, set weather | DONE | `world.rs` multi-zone reload; ref `ClientNet.bb:1633-1777` | live warp |
| ZON-2 | Same-`AreaName` re-warp skips full reload but still teardowns if numeric AreaID differs | PARTIAL | ref `ClientNet.bb:1676,1717` | live |

---

## P. Transport / Protocol (foundation)

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| NET-1 | 64-bit pure-Rust ENet transport (reliable flag + channels) connects to the unmodified server | DONE ✅ | `enet-sys`, `rcce-net/transport.rs`; Rust-port memory (PR #462 merged) | live |
| NET-2 | Wire codec: big-endian int fields, 1-byte-length strings, 4-byte LE floats; file I/O little-endian, 4-byte-length strings | DONE ✅ | `codec.rs`, `reader.rs`; ~113 tests | `cargo test` |
| NET-3 | MD5-hashed password auth | DONE | `auth.rs`; ref `MainMenu.bb:804` | live |
| NET-4 | All live packet codecs (≥38 types) round-trip vs the real server | PARTIAL | `codec.rs`; some inbound handlers missing (dialog, quest, party, projectile, spell-effect) | `cargo test` + live |

---

## Q. Tooling / Drop-in (meta)

| ID | Criterion | Status | Evidence / Reference | Verification |
|---|---|---|---|---|
| TOOL-1 | Headless PNG capture (`RCCE_SHOT`/`_FRAME`) for menu (frame 45) and world (frame 150). **Phase 4b: the world shot now renders the 2D overlay too** (was 3D-only via `capture_png`), so HUD / nameplates / target panel are headlessly verifiable. New hooks: `RCCE_SELECT=<frame>` (select nearest actor). | DONE ✅ | `client_window.rs` world shot now world+overlay→offscreen | self-evident |
| TOOL-2 | AUTO* headless drivers (LOGIN/SUBMIT/ENTER/WALK) | DONE | audit §8 | self |
| TOOL-3 | `cargo build --release` zero warnings + `cargo test` green | DONE | workspace state | `cargo build/test` |
| TOOL-4 | Phase 6 true drop-in cutover (rename → `Client.exe`, `Project Manager.exe` launch) | DEFERRED | PLAN Phase 6 | post-parity |

---

## Parity scorecard (2026-06-01 baseline)

Counting concrete criteria (excluding DEFERRED): **DONE ≈ 41, PARTIAL ≈ 28, MISSING ≈ 16** (ANIM-1, MOVE-5, MENU-SCENE, project-fix, TGT-1..6, CBT-1/2 closed 2026-06-01, Phases 1-5). **All four headline play-test gaps are now closed.**

1. ~~**MENU-SCENE** — dedicated 3D menu scene with posed character~~ **DONE ✅** (Phase 3; backdrop-art polish = MENU-SCENE-b).
2. ~~**ANIM-1** — local-player walk/run animation~~ **DONE ✅** (Phase 1).
3. ~~**MOVE-5** — click-to-move~~ **DONE ✅** (Phase 2).
4. ~~Targeting + combat: TGT-1..6 (select / highlight / panel / context menu / NPC dialog) + CBT-1/2 (attack-the-mob auto-loop)~~ **DONE ✅** (Phases 4-5).

**Remaining work is non-headline parity breadth** (PARTIAL/MISSING): combat anims (ANIM-8), spell effects/projectiles (SPL-8/PRJ-1), quests/party (QST/PTY), water (ENV-4), chat colors/scrollback (CHAT-2..4), trade completeness (TRD-3), camera modes (CAM-4..6), swim/ride anims (ANIM-4/5), and the MENU-SCENE-b backdrop polish.

These drive the Phase ordering in `PLAN.md`.
