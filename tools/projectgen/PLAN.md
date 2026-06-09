# RCCE2 default-project overhaul — plan

Goal: turn the bare smoke-test project under `data/` into a small but complete
world that *showcases* the engine to a new user — varied spells, items, actors,
a real starting area with NPCs and a quest, sound and music. Leverage the
**Heroes' Fate** asset library (`C:\Users\dyanr\Desktop\HeroesFate\Game\Data`,
permission granted) for art/audio/zone geometry. **No engine-source changes.**

## What we're starting from (audited iteration 1)

| Catalog | Count | Notes |
|---|---|---|
| Spells | 1 → **4** | only Fireball; 7 spell-icon textures sat unused |
| Items | 2 | Sword, Shield |
| Actors | 3 | Human/Fighter, Stag, Ork |
| Areas | ~6 | mostly "Test*" / "ha" placeholder zones |
| Textures | 88 | registered |
| Meshes | 89 | registered |
| **Sounds** | **0** | **project ships completely silent** |

Heroes' Fate is a **client distribution**: 49 finished visual zones + large
Meshes/Textures/Sounds/Music libraries, but **no** server gameplay catalogs
(Actors/Items/Spells/Factions). So HF gives us *art, audio, and zone geometry*;
gameplay we author ourselves.

## Iteration log

- **Iter 1 (done):** Reverse-engineered + documented the binary formats. Built
  `rcdata.py` codec, proven byte-faithful via `validate.py` round-trip on all 5
  live files. Added 3 restorative spells (Heal / Regeneration / Meditation) with
  working RSL scripts, reusing already-registered icons; upgraded Click_Trainer
  into a multi-spell teacher. Verified: round-trip, icon-existence, script-presence.
- **User bug report #2 (client hard-crash entering Test Zone):** Blitz Client.exe crashed
  ("actors pop then crash"); CLIENT LOG EMPTY = hard crash / access violation (not a logged
  RuntimeError). Server log healthy (spawned Human/Rat/Orc fine). ELIMINATION: Plains works
  with weather + ambient sound (wind covers whole zone) + player footsteps + Human/Stag
  meshes → none of those is the cause. Test-Zone-unique renderables = Rat (mesh 161) + Orc
  (mesh 81), both rendered for the FIRST time ever (original sample had them at SpawnMax=0).
  Wrote `b3dinspect.py` (b3d NODE/ANIM/bone parser): stag=28 bones+Head joint, rat=41 bones
  +Head joint+200 frames (NORMAL), **Orc.b3d = 165 bones, 2MB, 266 frames, Bip01 biped rig,
  no plain 'Head' joint (ANOMALOUS)** → prime crash suspect (runtime skinning overflow; no
  .bb-level bone Dim limit found, so it's in the compiled runtime — unfixable in data).
  ACTIONS: disable_test_monsters.py (set Rat+Orc SpawnMax=0, unblock) then enable_rat.py
  (re-enabled rat, orc still 0) as a bisect. AWAITING user retest: if Test Zone stable with
  rat-only → Orc mesh confirmed; then replace Orc/Raider's mesh with a working one or drop it.
  Added **/loc** command (In-game Commands.rsl) for coord capture.
  ORC REPLACEMENT OPTIONS (vetted HF enemy meshes via b3dinspect — all sane vs Orc.b3d's 165
  bones): NPCGoblin=11 bones, Troll=19, Gremlin=20, Wolf=20 (HF Actors/Monsters + Animals).
  These are HF's shipped enemies (render in-engine) → preferred fix: import one (e.g. Troll
  for a "Raider") = uses HF assets + orcish look + sane mesh. Needs: copy .b3d + its textures,
  register in Meshes.dat, PORT its anim set from HF Animations.dat (frame ranges differ from
  rcce2 sets 0-3), point Orc/Raider actor at it. Safe fallback = reskin Orc/Raider to rcce2
  Human mesh 3 + anim set 0 (100% proven render, but looks human). Pending user choice + rat
  confirm. NOTE: vet EVERY actor mesh with b3dinspect (bone count) before spawning — Orc.b3d
  taught this.
- **Orc fix APPLIED (import_troll.py):** replaced the crashing Orc.b3d with HF's **Troll**
  mesh (19 bones, vetted). Built `animsets.py` (AnimSet codec: id+name+150×[name,start,end,
  speed], round-trip proven). HF Troll anim set (id43) uses the SAME names as rcce2's Ork
  set except attacks ('Attack 1/2/3' vs 'Default/Right hand/Two hand attack') — ported it as
  rcce2 set id 4 with that remap (+ 'Sitdown'->'Sit down') so all engine triggers resolve.
  Copied Troll.b3d + Body.bmp (its texture, an absolute author path — also dropped next to
  the mesh as a load fallback + registered as actor body tex). Registered mesh id 83, tex id
  87. Orc/Raider actor now → mesh 83 / anim set 4 / body tex 87 / radius 40. Re-enabled rat
  + orc spawns. All files round-trip. CRASH-SAFE by bone count; cosmetic (texture/anim) is
  static-verified but needs in-game visual confirm. Fixed a codec bug: MediaDB.add_file mesh
  shader default 65535 overflowed signed-short pack → use -1 (== 0xFFFF, matches engine).
- **Preventive mesh audit + dormant-trap fix:** built `audit_meshes.py` — resolves every
  actor template's mesh and reports skinned-bone count, flagging >80 (Orc was 165, crashed;
  stag28/rat41/troll19 fine). It caught a LATENT trap: the original "Ork" actor (id 2) still
  pointed at the 165-bone Orc.b3d (spawned nowhere now, but a crash waiting for anyone who
  enables it). Repointed id 2 → Troll mesh 83 / anim 4 / tex 87 too. Now NO actor references
  Orc.b3d; audit passes clean (all ≤41 bones). Run audit_meshes.py before shipping any actor.
- **User retest #2 (Troll renders+walks+dies; 2 issues fixed):** (a) untextured — the Troll
  mesh's embedded 'Body.bmp' is a 2x2 dummy; real skin is Textures/Actors/Monsters/Troll_02.png
  applied via the actor body-texture system (Actors3D.bb:302 EntityTexture whole-entity when
  male_face unset). `fix_troll_texture.py` imported Troll_02.png (tex 88), pointed Orc/Raider +
  Ork male/female_body at it. LESSON: b3dtex.py basename-glob is unreliable (grabbed the wrong
  same-named Body.bmp) — match by the mesh's own directory. (b) STACK-OVERFLOW casting Fireball
  at a just-killed target: CreateProjectile derefs freed Target\CollisionEN (Projectiles3D.bb:21
  non-homing AND :74 homing); FreeProjectilesTargeting only covers in-flight, not a projectile
  BORN against an already-dead actor. Engine gap. Mitigation: `If Attribute(Target,"Health")<=0
  Then Return` guard in Spell_Fireball/FrostBolt/Lightning before FireProjectile (Attribute=0 for
  dead/freed) — stops firing at a corpse. Non-homing does NOT help (line 21 derefs at creation).
- **User retest #3 (Troll stack-overflows while WALKING, no combat):** client log empty (hard
  crash); Troll_02.png is a clean 512x512 POT RGB (not the cause); troll spawned+walked+textured
  THEN crashed → per-frame walk-update fault, Troll-mesh-specific (Orc.b3d 165-bone crashed too;
  the Human mesh + the rat render/walk flawlessly). CONCLUSION: HF monster meshes (Orc, Troll)
  are incompatible with this Blitz client build despite sane bone counts. RELIABILITY FIX
  (`fix_orc_to_human.py`): repointed BOTH orc actors (id4 Orc/Raider spawned + id2 Ork dormant;
  note 'Ork' vs 'Orc' spelling) to the Human mesh's exact render config (mesh 3/anim 0/human
  body+face/radius 25). No actor references the Troll mesh (83) now. Orc/Raider renders as a
  human warrior — crash-free (player pipeline), quest unchanged. Cosmetic tradeoff: looks human,
  not orcish. Troll/Orc meshes left registered but unused (revisit if client mesh handling fixed).
- **ROOT CAUSE FOUND (user retest #4): zero-length anim ranges → integer /0 = "Stack overflow!".**
  Human-meshed orc ALSO crashed, on DEATH, after anim+sound played fine → NOT mesh, NOT spell.
  Server fine (scripts ran). P_ActorDead (ClientNet.bb:1096) plays Rand(Death1..Death3); Player
  set 'Death 3' = 0-0, Rat 'Death 2/3' = 0-0, Ork all 0-0. A 0-length range divides by zero in
  the client anim code = "Stack overflow!". Current PlayAnimation (Animations.bb:58) guards it
  (`If AnimEnd[Seq]=0 Then Return`) so a CURRENT client is fine — user's bin/Client.exe is STALE
  (predates the guard; this worktree's bin is May 30, HEAD Jun 8). DATA FIX (version-independent):
  `fix_zero_anims.py` repointed ALL 93 zero-range anims across all 5 sets (death->valid death pose,
  attack->Default attack, other->Idle); NO 0-0 ranges remain. This ALSO explains the earlier Troll
  "walking" crash (Troll set had 0-0 Stand up/Look around/Yawn the idle AI plays). MESH WAS LIKELY
  INNOCENT — the anim /0 was the real bug all along; the human-mesh reskin can be reverted to the
  Troll now that anims are fixed (deferred to user buy-in given crash-fatigue). Animations.dat is
  fixed-layout so value edits don't change size (round-trip safe).
- **User retest #5: HARD crash (NO dialog) on login, actors moving+making sounds.** Access
  violation (not "Stack overflow!"). KEY: both bin/Client.exe (worktree + root) are the SAME
  May-30 build and the anim guard predates it — client is NOT badly stale; these are real issues
  my new content exercises. SOUND is the strongest suspect (never played before; original shipped
  .ogg but registered NONE — maybe deliberate; user emphasized 'making sounds') — possible OGG
  codec/channel hard crash. BISECT: `bisect_sound_off.py` cleared all actor speech + removed all
  sound zones (project SILENT now; reversible via set_actor_sounds.py + add_soundzones.py).
  Awaiting retest: silent+no-crash => sound is it (convert OGG->WAV or fix); still-crash => rule
  out sound, bisect weather then actors.
- **CONFIRMED: sound playback = the crash (silent retest = NO crash).** Backend is OpenAL
  (bin/OpenAL32.dll) which only plays PCM; NO ogg/vorbis decoder DLL in bin/ → OGG almost
  certainly unsupported (explains why the original registered 0 of its shipped .ogg files).
  No ffmpeg/converter available. WAV-vs-OGG test: `make_wav_test.py` generates a pure-Python
  PCM WAV (22050/16/mono), registers it (id 22), wires it as a Plains login-ambient sound zone;
  OGG sounds stay disabled. Awaiting retest: WAV plays + no crash => OGG was it, move ALL sounds
  to WAV (synthesize/source WAV since no converter); still-crash => sound unusable in this client,
  leave disabled (project fully playable silent).
- **CORRECTION (user): OGG sounds PLAY FINE (audible: ambient+footsteps+magic) — NOT a format
  issue; WAV theory dropped.** Crash correlates with sustained sound => likely audio-source
  EXHAUSTION from fire-and-forget EmitSound (footsteps fire ~every step of every moving actor;
  Client.bb:695 PlayActorSound/footstep EmitSound have NO channel tracking, unlike the managed
  ambient zones which reuse SZ\Channel via ChannelPlaying/StopChannel). `restore_safe_sound.py`:
  restored the 3 managed ambient zones (Plains wind / Test Zone forest / N.Shrine water) + removed
  WAV test; actor Speech (footsteps+combat) LEFT OFF; magic cast PlaySound still on. Awaiting
  sustained retest: stable => per-step actor sounds were it (then re-add OCCASIONAL combat sounds,
  keep per-step footsteps off); still-crash => narrow ambient/magic.
- **RESOLVED: sustained retest STABLE.** Per-step footstep/combat EmitSound exhaustion was the
  crash; managed ambient zones + magic cast sounds are fine. Meshes were NEVER the problem (the
  165-bone Orc.b3d "crash" and the Troll "walk crash" were both this sound issue). User asked to
  restore the meshes -> `restore_troll_mesh.py` put both orc actors back on the Troll monster
  (mesh 83/anim 4/tex 88/radius 40/genders 3); actor Speech LEFT OFF (per-step sounds stay
  disabled for stability). FINAL STABLE STATE: proper monsters (Troll orcs, rat, stag) + managed
  ambient audio (wind/forest/water) + magic cast sounds; NO per-step actor footstep/combat sounds.
  OPTION: occasional combat sounds (attack/hit/death — far rarer than footsteps) could be re-added
  + tested without footsteps, if the user wants troll/rat combat vocals back.
  Filed engine bug: RydeTec/rcce2#489 (unmanaged per-step EmitSound exhausts OpenAL sources).
- **Polish:** fixed Human/Fighter starting stats — Health was 10000/1000 (current 10x max, a
  leftover typo -> broken-looking HUD + invincible player) and Mana 50/100. Set both current =
  max (Health 1000/1000, Mana 100/100) so a fresh character spawns with full bars and combat /
  healing actually matters. Affects NEW characters only (template); existing saves keep their
  values. Surgical edit (only the two attr_value entries).
- **Docs accuracy:** updated docs/sample-project-guide.md audio section to match reality —
  managed ambient zones (wind/forest/water) + spell cast sounds ON; per-actor footstep/combat
  sounds documented as intentionally OFF with the #489 explanation + how to re-enable. The
  guide previously claimed actor sounds that were disabled.
- **User bug report (Plains portals on rocks):** the 3 travel portals were on scenery the
  player couldn't reach ('to testing' was at Y=13.4, above every waypoint). Plains has NO
  heightmap terrain to sample walkable Y; only the spawn + waypoint network are known-
  walkable. `fix_plains_portals.py` moved Test/'to testing'/'to timelase' to waypoint-pair
  MIDPOINTS (on the AI walk path, ground height, clear of NPC spawns). Surgical, verified.
  BEST ESTIMATE without a 3D view — awaiting user in-game verification. Added a **/loc**
  command to In-game Commands.rsl (Output ActorX/Y/Z) so the user can stand on walkable
  ground and read exact coords for a precise placement pass.
- **Iter 20 (done):** Two clean runtime-correctness audits + an AOE spell. Built
  `audit_calls.py` (flags script calls that aren't a known BVM/builtin/shipped-call — catches
  typos): our scripts came back CLEAN. Verified the trainer's now-8-option dialog is safe —
  the dialog is a 14-line scrolling window (Interface3D.bb), no option cap. Added **Flame
  Nova** (spell id 6, AoE fire) — the one distinct combat mechanic missing. CRASH-SAFE
  design: damage floored at 1 HP so no target hits 0 → no KillActor→FreeActorInstance fires
  inside the NextActorInZone walk (avoids the documented For-Each-during-Delete hazard).
  Allowlisted Spell_FlameNova; taught from the trainer. 7 spells now (4 damage incl AoE + 3
  restoratives).
- **Iter 19 (done):** Tuned progression so leveling is actually visible in a demo.
  Kill XP (GameServer.bb:164) = max(1, killedLvl-killerLvl)*XPMultiplier + Rand(0,20); the
  combat creatures had tiny multipliers and LevelUp required XP>Lvl*1000 — a new player
  would need ~100 kills to see level 2, so the (now-working) level system never triggered.
  Changed LevelUp.rsl to XP>Lvl*250 with a While loop (applies every level a big XP gain
  crosses; terminates since Lvl strictly increases). Bumped XP multipliers (`tune_xp.py`,
  surgical): Rat 1->5, Orc/Raider 2->12. Now ~2 quests (220xp) + clearing spawns reaches
  level 2-3 in a session. Reasoned tuning (can't playtest) but the direction is unambiguous.
- **Iter 18 (done):** Finished the privilege-correctness sweep + built an audit tool.
  `audit_privileges.py` parses the privileged-BVM set from bvm-reference.md and flags every
  script that calls one but isn't allowlisted (the silently-broken class). Found 6; the
  real bug was **LevelUp** — the engine's default XP-gain script (ThreadScript("LevelUp",…)
  non-priv, GameServer.bb:105) calls privileged `SetActorLevel`, so **players never leveled
  up** despite earning XP from quests/kills. Allowlisted LevelUp. The other 5 (BlackSmithing
  Skill Template, Poison Potion, SendMail, UpdateMail, Warp Script) are dormant example
  scripts with NO engine/content invocation — correctly left un-allowlisted (don't grant
  privilege to unused scripts). Progression now works end-to-end: kill/quest → GiveXp →
  LevelUp → SetActorLevel. All WIRED privileged-BVM callers are now allowlisted.
- **Iter 17 (done):** CRITICAL bug-fix — made the spell/potion/death systems actually work.
  Discovery: `BVM_SETATTRIBUTE` requires FULL privilege (ScriptingCommands.bb:2346,
  RequirePrivileged not self-or-priv), and spells/potions run NON-privileged
  (ServerNet.bb:1276 ThreadScript(Sp\Script$,...,Handle(caster),Handle(target),level), no
  priv flag) and weren't allowlisted → every spell's damage/heal + every instant potion
  SILENTLY DID NOTHING (incl. shipped Fireball — broke when SETATTRIBUTE was hardened).
  Allowlisted all 8 (Spell_Fireball/FrostBolt/Lightning/Heal/Regeneration/Meditation +
  Item_HealthPotion/ManaPotion). ALSO: Death.rsl (ThreadScript("Death",...) non-priv, not
  allowlisted) → its SetAttribute(Health,50)+Warp respawn failed; AND it warped to the
  CURRENT zone's "Begin" portal which only Plains has. Allowlisted Death + changed warp to
  fixed "Plains"/"Begin" so respawn works from any zone. LESSON: format-verified ≠
  runtime-correct — the privilege-gating layer hid broken core systems behind clean files.
- **Iter 16 (done):** Vendor — closed the economy loop (earn from loot/quests -> spend).
  No gold sink existed. Wrote Click_Merchant.rsl: a scripted shop (dialog menu, NOT
  OpenTrading) using the non-priv `Gold(actor)` getter to check affordability then
  ChangeGold(-price)+GiveItem (privileged -> allowlisted). Sells Potion of Healing/Mana
  (25g) + Long Sword (85g). Placed in Plains wp7 via place_npc.py; allowlisted. ChangeGold
  accepts negative to deduct. Guide updated.
- **Iter 15 (done):** Gave actors voices + footsteps (uses HF assets — the stated goal).
  Speech array (Actors.dat MSpeechIDs/FSpeechIDs[16]) index->event: 4=Attack1 6=Hit1
  9=Death 10=FootstepDry 11=FootstepWet (65535/-1 silent); engine plays MSpeechIDs[event]
  positionally (Actors3D.bb:794). Imported 6 HF sounds (Rat_01/04/07 squeaks ids 16-18,
  Troll Attack/Hit/Death ids 19-21 for the Orc Raider) via add_sounds.py HF_IMPORT. Wired
  via `set_actor_sounds.py` (surgical — only speech arrays change): player footsteps
  (Carefulstep2/dampstep ids 0/1), rat attack/hit/death (16/17/18), orc attack/hit/death
  (19/20/21) + heavy footstep. Combat now has audio.
- **Iter 14 (done):** Wrote `docs/sample-project-guide.md` (140 lines) — a player
  walkthrough of the loop AND a "how it's wired" learning reference mapping every feature
  to its files/scripts (spells/items/creatures/quests/spawns/loot/weather/ambient + a
  privileged-scripts note + "how to extend"). Accurate to the shipped content (verified via
  a codec snapshot: 6 spells, 11 items, 5 creatures, 4 projectiles, 16 sounds, 37 scripts).
  Highest-value remaining VERIFIABLE artifact for new-user discovery; no data changed.
- **Iter 13 (done):** Showcased the dynamic weather system (previously undemonstrated).
  All gameplay zones shipped WeatherChance [0,0,0,0,0] = permanently clear. WeatherChance[i]
  = percent weight for weather type i+1 (1=Rain 2=Snow 3=Fog 4=Storm 5=Wind, Environment.bb;
  remainder=clear); engine rolls every Rand(2500,10000) ticks (ServerAreas UpdateWeather).
  `add_weather.py` set thematic weather: Plains [20,0,0,5,0] (showers+rare storm), Test Zone
  [22,0,8,0,0] (rain+fog), Northern Shrine [0,30,0,0,0] (snow). Surgical (only the 5 weather
  bytes change; rest byte-identical). Pairs with the Rain/Thunder/Snow sounds+particles
  registered in iter 2.
- **Iter 12 (done):** New-player starting kit + HF-port blocker found.
  **BLOCKER (important):** HF area .dat files are an OLDER visual-area format — they fail
  to decode at the scenery section (HF scenery records lack the Lightmap$/RCTE$/CastShadow/
  ReceiveShadow/RenderRange fields added later). `LoadArea` has NO version detection, so the
  current engine CANNOT load HF areas as-is; porting one needs either editor re-save (GUE/
  Loom) to upgrade the format, or reverse-engineering the old scenery layout to read-old/
  write-new — PLUS copying+registering every referenced mesh/texture, PLUS no visual verify.
  High cost, low marginal value over the complete loop already built → deferred to a human
  with the editor + visual check. **Content:** new chars get C\Gold=StartGold but NO items
  (ServerNet P_CreateCharacter). Added a first-ever-login starting kit to Login.rsl (gated
  on the existing FirstLogin detection): Long Sword + Scarred Shield + 3 Potion of Healing +
  teaches Heal. GiveItem is privileged so allowlisted "Login" (bonus: also un-breaks the
  league-scoreboard WriteFile that was silently failing). AddAbility is NOT privileged
  (Click_Trainer uses it un-allowlisted). Login invoked non-priv via
  ThreadScript("Login","Main",Handle(char),0) (ServerNet.bb:2168).
- **Iter 11 (done):** Atmosphere pass — every gameplay zone now has character-matched
  ambient sound (additive sound zones via the iter-10 codec; non-destructive, doesn't
  touch visuals). Northern Shrine (waterfalls+water) -> Water\\Riverplane (id 5, r130);
  Plains (open grassland) -> zone-wide Weather\\Wind (id 11, r320, vol40) layered over its
  existing localized fountain+music spots; Test Zone already had forest (iter 10).
  `add_soundzones.py` PLAN extended; idempotent on sound id. Test Terrain/Test1 left
  silent (unused stubs). All visual areas still round-trip.
- **Iter 10 (done):** CLIENT visual-area codec (last + most complex format) + ambient
  sound. Derived the full layout from ClientAreas.bb SaveArea@820: header (loading tex/
  music, sky/cloud/stormcloud/stars tex, fog rgb+near+far, map tex, outdoors, ambient rgb,
  light pitch/yaw, slope) then short-counted sections: scenery (mesh,xyz,pitch/yaw/roll,
  scale xyz, anim_mode, scenery_id, texture, catch_rain, entity_type, lightmap str, rcte
  str, cast/receive shadow, render_range), water, colboxes, emitters (config str), terrains
  (base/detail tex, size int, (size+1)^2 height floats, transform, detail, morph, shading),
  sound_zones (xyz, radius, sound, music, repeat_time, volume). Codec read_client_area/
  write_client_area, round-trip PROVEN on all 5 real areas + baked into validate.py
  (ha.dat excluded — legacy pre-shadow-fields stub, unreferenced). **Discovery:** Plains
  already had 2 sound zones referencing sound id 2 = the forestday I registered in iter 2,
  so iter-2 retroactively gave Plains ambient audio. **Content:** add_soundzones.py added a
  zone-wide forest-ambient SoundZone to silent Test Zone (sound 2, radius 260, vol 60).
  SoundZone semantics: plays within Radius, RepeatTime=0 loops via re-trigger (Client.bb
  UpdateSoundZones). This unlocks future visual work (scenery/fog/terrain edits, zone port).
- **Iter 9 (done):** Polish + groundwork. (a) Added "Rat Catcher Medalion" item (I_Other,
  id 10) — the shipped Ratcatcher1.rsl rewards it by exact name but it was never in the
  catalog, so GiveItem silently dropped it; now its reward works (parallel to iter-7's
  allowlist fix). (b) Added a new-player welcome to Login.rsl (3 Output lines after
  `Player = Actor()`, before the league logic — minimal, low-risk insertion) orienting
  players to the trainer, the Captain's quest, and the wilds. (c) Began decoding the
  CLIENT visual-area format (ClientAreas.bb LoadArea/SaveArea, Data\Areas\*.dat) as
  groundwork for a future zone port — it's variable-length & complex (loading screen
  refs, terrain, scenery, water, lights), NOT fixed-array like the server area. Header
  so far: LoadingTexID short, LoadingMusicID short, SkyTexID/CloudTexID/StormCloudTexID/
  StarsTexID shorts, FogR/G/B bytes, FogNear float, FogFar float, then terrain/scenery...
  Full decode + zone port deferred to a dedicated iteration (SaveArea @ line 820 is the
  cleaner reference to derive the format).
- **Iter 8 (done):** A second quest, cross-zone. Wrote Quest_OrcRaiders.rsl ("Raiders at
  the Gate") modeled on the Ratcatcher state-machine (resumes via QuestStatus; WaitKill x3
  on ActorID("Orc","Raider")) but rewarding items that EXIST (Imperial Helmet + 150 gold +
  120 xp) — the shipped Ratcatcher rewards a "Rat Catcher Medalion" that isn't in the
  catalog. Placed the quest-giver (Human NPC) in Plains at waypoint 5 via `place_npc.py`
  (surgical NPC-spawn placer, supports actor_script). Allowlisted Quest_OrcRaiders. Ties
  together iter4/5 enemy + iter6 spells + iter7 rewards into a town→wilds→town loop.
  Cross-checked: reward item exists, Orc/Raider exists, spawn placed, script allowlisted.
- **Iter 7 (done):** Economy / rewards. Found a latent gap: all reward BVMs
  (GiveItem/ChangeGold/ChangeMoney/GiveXp/GiveKillXp/SetGold) are Privileged, but the
  shipped quest scripts Ratcatcher1.rsl + quest.rsl were NOT on the privileged allowlist
  -> their rewards silently refused (the allowlist's own comment notes shipped scripts
  "were ALREADY broken"). Audited both (standard quest logic, no dangerous BVMs) and added
  them + a new MonsterLoot to `Privileged Scripts.dat` (CRLF, no BOM — append in latin-1).
  Added MonsterLoot.rsl (a spawn DeathScript: GameServer.bb:209 invokes it as
  ThreadScript(script,"Main",Handle(Killer),0) so Actor()=the KILLER) — drops 3-14 gold +
  25% Potion of Healing. Wired it as SpawnDeathScript on the Rat & Orc Test Zone spawns
  (`set_loot.py`, surgical). Quest rewards now pay out; kills drop loot. (Inferred-broken
  reward path; allowlisting is elevation-only so safe regardless. Not runtime-verified.)
- **Iter 6 (done):** Damage-spell variety. Decoded Projectiles.dat (id, name, mesh,
  emitter1/2 = .rpc config names in Data/Emitter Configs, emitter1/2 tex, homing, hit%,
  damage, damage_type, speed) — codec `read_projectiles`/`write_projectiles`, round-trip
  proven. Added Frost Bolt (Ice, emitters Default+Snow, tex blue1=10) and Lightning Bolt
  (Electricity, Default+Flame, tex lightbeacon=79) projectiles (`add_projectiles.py`).
  Added matching spells (Spells.dat ids 4/5) + scripts Spell_FrostBolt.rsl /
  Spell_Lightning.rsl (modeled on shipped Spell_Fireball: fire homing projectile + apply
  damage + PlaySound + CreateFloatingNumber). Taught both via Click_Trainer (now 6-option
  menu). Catalog now spans 3 damage schools (Fire/Ice/Electricity) + 3 restorative.
  Cross-checked: every spell script exists, every FireProjectile name resolves.
- **Iter 5 (done):** Placed creatures in the world (the keystone). Decoded the full
  server-Area format (ServerAreas.bb ServerLoadArea/SaveArea: weather header, Entry/
  ExitScript, PvP/Gravity/Outdoors, 150 triggers, 2000 waypoints, 100 portals, 1000
  spawn slots, then N water records — area NAME comes from the filename, not the file).
  Codec `read_server_area`/`write_server_area`, round-trip proven on all 4 shipped areas
  + baked into validate.py. **Discovery:** spawn slots were pre-authored — Plains already
  spawns the Click_Trainer Human NPC (so iter-1's multi-spell trainer is LIVE in the start
  zone) and the Stag; Test Zone hosts the Ratcatcher1 + marriage NPCs. A spawn is live when
  SpawnMax>0 (Server.bb:544); engine safely skips slots whose actor doesn't exist.
  **Content:** `add_spawns.py` activated 2 empty Test Zone slots — Rat/Critter x3 at wp 0
  (the Ratcatcher quest is now COMPLETABLE) and Orc/Raider x2 at wp 4 (aggressive-enemy
  showcase). Surgical: asserts every non-spawn section + every untouched slot is unchanged.
  Spawn fields: actor(id), waypoint, size, script, actor_script(NPC right-click/AI),
  death_script, max, frequency(s), range(scatter).
- **Iter 4 (done):** Creature types. Decoded the compact Actor-template format
  (Actors.bb LoadActors/SaveActors — NOT the huge per-character ActorInstance) and
  added it to the codec (`read_actors`/`write_actors`/`new_actor`), round-trip proven
  on the shipped 3 (Human/Fighter, Stag, Ork). Added 2 combat-ready enemies reusing
  registered meshes/anims: **Rat/Critter** (mesh 161, anim 2 — race/class match the
  shipped Ratcatcher1.rsl quest so it's now wireable) and **Orc/Raider** (mesh 81,
  anim 3, AGGRESSIVE; base Ork ships passive). `add_actors.py`. Anim sets: 0=Player
  1=Stag 2=Rat 3=Ork. Template layout notes: mesh_ids[0]=male mesh, [1]=female (0/-1
  if none); male_body_ids[0]/male_face_ids[0] = body/face *texture* IDs; genders 0=male
  only, 3=genderless (monsters use the male_* arrays). CAVEAT: stat balance + scale/
  radius are reasoned guesses (no runtime verify); shipped templates have odd
  inventory_slots negatives — new actors use 0. Creatures EXIST but aren't spawned in
  any zone yet, and NPC right-click scripts are per-instance — both need the Area
  format (next iteration).
- **Iter 3 (done):** Starter item catalog (was just placeholder Sword+Shield). Fixed a
  codec bug first — the per-type extra-field map was wrong for ItemType 3/4/6 (only
  round-tripped because test items were types 1-2); corrected to match the engine's
  Select Case (weapon=1, armour=2, potion/ingredient=4/5, image=6; ring=3/other=7 have
  none). Registered the 5 real item icons sitting unregistered on disk
  (data/Textures/Items/*.bmp → tex IDs 82-86). Added 8 items: Long Sword, Scarred
  Shield, Imperial Helmet/Armor (3-piece set, +2 Toughness on chest), Adventurine Ring
  (+2 Magic), and 3 potions — Healing/Mana (instant, script Item_HealthPotion/
  Item_ManaPotion) + Elixir of Strength (timed buff: attrs[Strength]=+5,
  eat_effects_length=60). `add_items.py`, `rcdata.new_item()` factory. Verified: all 5
  files round-trip, icon/script refs resolve, existing bytes preserved. Consts:
  slots Weapon1/Shield2/Hat3/Chest4/Hand5/Belt6/Legs7/Feet8/Ring9/Amulet10/Backpack11;
  attrs Health0/Mana1/Str2/Dex3/Speed4/Magic5/Tough6/Swim11; dmg Pierce0/Slash1/Bash2/
  Fire3/Ice4/Poison5/Elec6/Shadow7/Divine8/Wind9/Magical10. Eating a potion = timed
  ActorEffect applying the item's attr array for eat_effects_length sec + runs Script$.
- **Iter 2 (done):** Gave the project sound. The 12 .ogg files shipped under
  data/Sounds/ were registered in NONE of Sounds.dat (it had 0 entries) — registered
  all 12 (footsteps/forest/ice/water/weather, IDs 0-11). Imported 4 HF magic-cast
  sounds into data/Sounds/Magic/ (IDs 12-15) and wired `PlaySound(Player, id, 1)`
  into all 4 spell scripts so casting is audible. `add_sounds.py`, manifest in
  `sound_ids.txt`. Verified: round-trip, on-disk file existence, script↔ID match.
  NOT runtime-verified. Footstep (per-actor Speech array, IDs 10/11) + zone-ambient
  wiring still pending (need Actors.dat / Area-format decode).
- **Shrine Keeper (Northern Shrine gets a purpose):** Northern Shrine's only NPC was
  a leftover `Click_Test` demo spawn. Authored `Click_ShrineKeeper.rsl` (right-click
  rest station: SetAttribute Health/Mana to MaxAttribute = full restore, "Pray" anim,
  blessing Output; Examine flavour). `wire_shrine_keeper.py` surgically repointed
  spawn slot 0's actor_script `Click_Test`→`Click_ShrineKeeper` (only that field
  changed — asserted every other section/slot byte-identical). Allowlisted in
  Privileged Scripts.dat (SetAttribute is privileged). Verified: validate.py round-trip
  PASS, audit_privileges clean (not in the unlisted-privileged set), audit_calls clean,
  "Pray" anim = valid 1425-1508 range (no 0-0 /0 crash). NOT runtime-verified.
- **Rat Catcher quest polish (first-impression cleanup):** the shipped `Ratcatcher1.rsl`
  (the earliest quest a new player accepts) spammed dev debug text on accept — a blank
  line from `Output(Player, QuestResult$)` (typo: `QuestResult$` undeclared → non-Strict
  reads as empty), plus `"In quest Waitloop"` and `"Waiting to kill Rat Critter 2"`.
  Reads as unfinished-demo. Removed all three lines surgically; left every quest-state
  string, the `QuestTemp` log notifications, and the reward confirmation untouched (resume
  state-machine intact). Verified: audit_calls clean, no stray debug Output remains. RSL is
  engine-parsed (no compile); not runtime-verified. Spawn audit also surfaced backlog: 2
  silent placeholder Human NPCs in Test Zone (wp 2/3), the out-of-place stock `marriage`
  NPC in the wilds, and 3 junk Plains portals (`to testing`/`return`/`to timelase`).
- **Wounded Scouts (wilds NPCs get purpose + narrative thread):** the two silent
  placeholder Human spawns in Test Zone (wp 2/3) became **Wounded Scouts** — orc-raid
  survivors. Two files, reused by both spawns: `Init_WoundedScout.rsl` (spawn `script`
  slot → `SetName` "Wounded Scout"; privileged → allowlisted) and `Click_WoundedScout.rsl`
  (right-click → short OpenDialog warning + a signpost to the Plains Captain's "Raiders at
  the Gate" quest; Examine → flavour line; non-privileged, dialog/Output only).
  `wire_wounded_scouts.py` surgically set script+actor_script on exactly the two blank
  actor-0 wp-2/3 placeholders (asserted every other section/slot byte-identical). Ties the
  wilds to the Plains quest narratively. Verified: validate.py PASS (Test Zone 78938→79008,
  string growth expected), audit_privileges clean, audit_calls clean. NOT runtime-verified.
  Remaining backlog: out-of-place `marriage` NPC in the wilds; 3 junk Plains portals.
- **Portal-name investigation (negative result, logged so we don't redo it):** confirmed
  portal `name` fields are NOT player-visible — 0 references in any client module. They're
  internal IDs only: incoming-link matching (a dest portal's name == a source portal's
  linkname) + `StartPortal` spawn match (ServerNet.bb:2873). `PortalLinkName` is consumed
  only by the GUE editor + load/save. So renaming the gibberish `to timelase` (the sole
  Plains→Northern Shrine portal) gains nothing player-facing — skipped. Also: user already
  said "leave the duplicate Test Zone portals alone." Portals are a dead end for player-
  facing polish absent a runtime-verifiable walkability/connectivity bug.
- **Orphaned items surfaced through the merchant:** item-acquisition audit (every GiveItem
  across all scripts vs the 11-item catalog) found 2 items UNOBTAINABLE in normal play:
  Adventurine Ring (id 6, +Magic wearable) and Elixir of Strength (id 9) — the latter is
  the flagship "timed +5 STR buff driven purely by item data" the guide advertises but a
  player could never get. Verified both have real effects in item data (Ring attributes[5]=
  +2 while worn; Elixir item_type=4 consumable, attributes[2]=+5 STR, eat_effects_length=60
  — engine applies it on consume, no script). Added both to Click_Merchant.rsl (Ring 120g =
  its value field; Elixir 60g) as menu options 4/5, "Just browsing" → 6. Verified: audit_calls
  clean, merchant allowlisted, both names exact-match catalog. NOT runtime-verified.
- **Differentiated enemy loot (rats vs orcs):** both enemies shared MonsterLoot (gold +
  25% healing potion) → identical feel. KEY CONSTRAINT discovered: a DeathScript is invoked
  `ThreadScript(script, "Main", Handle(Killer), 0)` (GameServer.bb:209) — `Actor()` = killer,
  param2 = 0, so there is NO corpse handle; one shared script CANNOT branch on the victim.
  Fix = per-enemy death scripts. Authored `OrcLoot.rsl` (Orc Raider, the quest target):
  more gold (10-30 vs 3-14) + tiered roll — Elixir of Strength 5% / Mana Potion 20% /
  Healing Potion 30%. Also gives the merchant-only Elixir/Mana a drop path. `wire_orc_loot.py`
  repointed only Orc/Raider spawns' death_script MonsterLoot→OrcLoot (rats keep MonsterLoot);
  surgical, asserted byte-identical elsewhere. Allowlisted OrcLoot (ChangeGold/GiveItem priv).
  Verified: validate.py PASS (79008→79004, MonsterLoot→OrcLoot is -4 bytes), audit_privileges
  clean, audit_calls clean, drop item names exact-match catalog. NOT runtime-verified.
- **Marriage 'Priest' relocated wilds→town:** the stock `marriage` NPC sat at Test Zone
  wp 1 among the rats/orcs. Analysis: it's a player<->player system — polls ActorTarget
  for a SECOND player, gates on 10,000GP — so solo-unusable for a new user, and a "Priest"
  in a monster zone is incongruous. But it's a legit multiplayer feature demo, so KEEP +
  relocate rather than delete. `relocate_marriage_npc.py` added it to Plains (town) empty
  slot 6 @ wp 1 (walkable town waypoint near the spawn point; NPC spawns anchor to the AI
  waypoint network so this is placement-safe, unlike free-positioned portals) and cleared
  the Test Zone spawn (max 0 + blank actor_script). Both files round-trip byte-faithfully;
  every other section/slot asserted identical. Final layout is coherent: Plains = Stag/
  Trainer/Captain/Merchant/Priest; wilds = RatCatcher/2 Scouts/rats/orcs. NOTE: marriage.rsl
  and Click_marriage.rsl are byte-identical duplicates (spawn uses 'marriage'); Click_marriage
  is now an unreferenced dup — left on disk, candidate for a future dedupe. NOT runtime-verified.
- **Faction system activated + Orc Raiders made truly aggressive:** the guide called the
  orcs "aggressive" but they were `aggressiveness=1` (Defensive — retaliate only). Traced the
  engine: `AILookForTargets` (GameServer.bb:1466) only proactively targets at `aggressiveness=2`,
  gated by `FactionRatings[target] < 150`. ROOT CAUSE: Factions.dat had only Traders(0)/
  Voyageur(1) at rating 100, and EVERY actor was default_faction=0 — so mobs were the player's
  own faction; the faction system was dormant. Fix (new faction codec read_factions/write_factions
  in rcdata.py, byte-faithful-proven): added faction 2 **Wildkin** (Wildkin↔Traders=0 hostile,
  Wildkin→Wildkin=200 allied); set Rat + Orc Raider default_faction=2; Orc Raider aggressiveness
  =2. Now orcs hunt the player on sight AND `AICallForHelp` (rating≥190) pulls allied rats into
  the fight = coordinated packs, while orcs ignore rats/each other (protects the Rat Catcher
  population). Town/scout NPCs (Traders) won't aid orcs. `setup_wildkin_faction.py` (surgical;
  asserts codec round-trip + only-targeted-fields). Verified: validate.py PASS (Actors 2207→2207,
  byte fields; Factions round-trips), final values confirmed. NOTE: wilds Human NPCs (scouts/rat
  catcher, faction 0) may be aggro'd by orcs but are 1000-HP and survive — acceptable. NOT
  runtime-verified. Bug caught mid-impl: over-broad post-write assertion expected the unused
  id2 'Ork' (cls='') to be Wildkin; fixed to assert only changed IDs.
- **Combat-balance static analysis + conservative orc-threat tune.** Verified the combat
  formula (GameServer.bb): weaponless-NPC melee `Damage = Strength/8 + Rand(-5,5) - AP`,
  clamp `>=1`; weapon melee uses item `weapon_damage` (+Str adjust). Kill XP `= max(1,
  victimLvl-killerLvl) * XPMultiplier + Rand(0,20)` → orc ~12-32, rat ~5-25; both starter
  quests + their required kills ~316 XP > the `Lvl*250` lvl-2 gate, so PROGRESSION IS
  REACHABLE. FINDING: combat is far too forgiving — player starts at the 1000-HP cap
  (Long Sword weapon_damage=7 → ~12-15/hit; orc HP 120) while orcs were Strength 18 →
  18/8=2 minus a starting player's small AP (shield only, no body armour in kit, default
  resists, Toughness/8~1) = clamped ~1/hit = harmless. So last tick's "aggressive wilds"
  weren't threatening AND the restorative content (Heal/Regen/Meditation, potions, Shrine)
  was dead weight. FIX (`tune_orc_threat.py`): Orc Raider Strength 18→80 (~1-11/hit after
  AP) — a 2-orc pack + faction-recruited rats now chips meaningful HP over a sustained
  fight, activating heals/potions, while 1000 HP keeps it non-lethal (no frustrating
  deaths). Strength = attr index 2 (Elixir attributes[2]=+5 "+5 Strength" confirms). Strictly
  safe in the too-hard direction (Str↑ only raises damage from ~1; min-1 clamp floors it).
  Validate.py PASS (2207→2207, byte field). **FLAG FOR PLAYTEST:** balance is runtime-
  sensitive (AP/hit-chance/level-stat-growth); adjust orc Strength or lower player starting
  HP (currently value[0]=max[0]=1000) to taste. Rats left weak (Str 6). NOT runtime-verified.
- **Description-quality pass (spells + items):** audited every spell description + item
  misc_data. All were good (the restorative/elemental spell rewrites + item flavour from
  earlier iters) EXCEPT Fireball (id 0, the flagship first spell) which kept the bland stock
  "Fires a ball of fire at the target" while its siblings got evocative rewrites. Fixed via
  `polish_fireball_desc.py` -> "Hurl a roaring ball of flame that scorches a single foe."
  (surgical, round-trip-asserted only-Fireball-changed; validate.py PASS). Items 7/8/10
  (potions, medallion) descriptions all fine. NOTE: the debug-only Sword(0)/Shield(1)
  ("A sharp blade"/"a shield") are weak but not in the player path (only /itempack +
  Spawn_Test) — left as-is. Project is now at micro-polish; content loop is complete.
- **Reference-integrity audit + full health-check sweep.** Added `audit_media_refs.py`
  (reusable, read-only): checks every catalog-referenced media ID — item/spell thumbnails,
  projectile meshes, actor meshes + body/face/blood textures — resolves to a registered
  entry in Textures/Meshes.dat (skips -1/0/0xFFFF = none). Result: ALL refs resolve, no
  missing-icon/dropped-mesh bugs slipped in over the iterations. Ran the FULL audit suite
  as a project health check — all green: validate.py round-trips byte-faithful; media refs
  resolve; audit_calls finds zero typo'd/unknown calls; audit_privileges shows only the 5
  stock templates unlisted (BlackSmithing/Poison Potion/SendMail/UpdateMail/Warp Script —
  none spawn-wired, unreachable in normal play); audit_meshes confirms every actor mesh
  <=80 bones (165-bone Orc.b3d gone; both orc templates use 19-bone Troll). PROJECT IS
  COMPLETE + VERIFIED CLEAN. Remaining work is playtest-gated balance (user's call) or
  net-new content (new zone/quest) — not bug-fixing.
- **NET-NEW: Orc Warlord mini-boss ("Grukk, the Raider-Chief").** First content *expansion*
  past the complete baseline. `add_orc_warlord.py` CLONES the working Orc/Raider actor record
  (id4 -> new id5 Orc/Warlord) so it inherits the proven Troll mesh(83)/anim(4)/Wildkin
  faction(2)/aggressiveness(2) — only boss fields differ: Health 300 (cap raised from 120),
  Strength 100, scale 1.2, xp_multiplier 25. Cloning (vs new_actor from scratch) = zero
  appearance-reconstruction risk + reuses already-registered media. Placed ONE spawn in Test
  Zone at the unused wp 1 (deep, away from entry rats/orcs) with spawn-init `Init_OrcWarlord`
  (SetName "Grukk, the Raider-Chief"; NPCs default to Race$ name, Actors.bb:628) + death script
  `BossLoot` (gold 80-150 + guaranteed Elixir of Strength + Healing Potion + 50% Imperial
  Helmet). Both scripts allowlisted (SetName/ChangeGold/GiveItem privileged). Boss is race Orc
  class Warlord so it does NOT count toward the Orc/Raider quest (intentional bonus content).
  Verified: validate.py PASS (Actors 2207->2672 = +1 record; Test Zone 79019), audit_media_refs
  OK, audit_meshes 19 bones, audit_calls clean, audit_privileges clean. NOT runtime-verified.
- **NET-NEW: "The Raider-Chief" boss quest (3rd quest) + surfaces Imperial Armor.** Gave the
  Grukk mini-boss a quest hook: `Quest_RaiderChief.rsl` (single-target clone of the proven
  Quest_OrcRaiders state machine — WaitKill(ActorID("Orc","Warlord"),1)), given by the wp-2
  Wounded Scout (narratively perfect: a survivor wanting vengeance). Reward = 200 gold + 150 XP
  + **Imperial Armor** — which the item-acquisition audit had flagged as orphaned (only via
  debug /itempack), so this surfaces it AND completes the Imperial set with the Helmet from
  Quest_OrcRaiders. `wire_raider_chief_quest.py` repointed ONLY the wp-2 scout's actor_script
  Click_WoundedScout->Quest_RaiderChief (wp-3 scout stays ambient flavour); spawn keeps its
  Init_WoundedScout name. Allowlisted (NewQuest/GiveItem/GiveXp/ChangeGold privileged). Verified:
  validate.py PASS (Test Zone 79019->79018, script-name -1 byte), audit_calls clean, audit_priv
  clean, reward item + ActorID race/class exact-match. NOT runtime-verified. Now 3 quests; both
  Imperial pieces obtainable. Wilds arc: scouts (lore+quest) -> raiders (quest) -> Grukk (boss+quest).
- **NET-NEW: Faith Armor spell (8th spell, new BUFF category).** Roster was 4 damage + 3
  restorative; no buff. Added `Spell_FaithArmor.rsl` — a divine defensive buff: raises Toughness
  +40 (feeds AP/damage-reduction) for 20s then subtracts exactly the bonus (drift-safe), via the
  proven own-thread DoEvents timing pattern (like Regeneration's HoT). Synergizes with the
  orc-threat bump (gives the player a defensive option). Reused the purpose-built HF icon tex 73
  'Spell Icons\\Spells\\FaithArmor2.bmp' (already registered) — name + icon + the existing
  divine/faith spell flavour all align. `add_faith_armor_spell.py` appended Spells.dat id7
  (existing spells asserted untouched prefix); wired into Click_Trainer.rsl (menu option 8 +
  ElseIf Result=8 AddAbility branch; "Nothing for now" -> option 9; Examine updated to
  "attack, restorative, and protective"). Allowlisted (SetAttribute privileged). Verified:
  validate.py PASS (Spells 755->886), audit_media_refs OK (thumb 73 resolves), audit_calls clean,
  audit_privileges clean. NOT runtime-verified. Trainer now teaches 8 spells across 3 categories.
- **NET-NEW + BUGFIX: Northern Shrine developed into a lore hub.** BUG found: the Shrine
  Keeper spawn's init `script` was the leftover `Spawn_Test` -> the rest NPC displayed as
  "Test Human" (and got tagged/given test items). Fixed: new `Init_ShrineKeeper.rsl` (SetName
  "Shrine Keeper"), repointed slot-0 init. ADD: a Shrine Oracle lore NPC (`Init_ShrineOracle.rsl`
  name + `Click_ShrineOracle.rsl` dialog) that ties the world's threads together (borderlands,
  orc raids, Grukk, the old faith behind the restorative magic) — gives a new player CONTEXT for
  the scouts/raiders/boss, turning the under-used shrine into a peaceful lore hub without combat.
  PLACEMENT: the zone had only ONE valid waypoint (wp 0, the Keeper; others are (0,0,0)), so added
  wp 1 at wp0 + (6,0,6) = same Y-platform, ~8u adjacent (low walkability risk vs free-positioned
  portals). `develop_shrine.py` (surgical: only wp1 + keeper-init + oracle-slot changed; all else
  asserted byte-identical). Allowlisted both name inits (SetName priv); Oracle dialog is non-priv.
  Verified: validate.py PASS (Northern Shrine round-trips), audit_calls clean, audit_privileges
  clean. NOT runtime-verified (esp. wp1 walkability — flag for playtest, easily nudged if off).
- **BUGFIX + polish: named every role NPC (was "Human"/"Test Human").** Swept all spawn init
  slots for leftover test refs (after the shrine "Test Human" find). Caught: Plains slot 3 =
  the SPELL TRAINER also ran the leftover Spawn_Test init -> displayed "Test Human" (the central
  onboarding NPC!). Broader gap: Captain/Merchant/Priest/Rat-Catcher had NO init -> defaulted to
  Race name "Human" (NPC name = Actor\Race$, Actors.bb:628). Added 5 tiny SetName inits
  (Init_SpellTrainer/TownCaptain/Shopkeeper/Priest/RatCatcher) + `name_npcs.py` which matches
  spawns by actor_script (role) — position-independent — and repoints only the init `script`
  field. Now every town/quest NPC has a proper nameplate. Allowlisted all 5 (SetName priv).
  Verified: validate.py PASS (Plains 79031, Test Zone 79033), audit_calls clean, audit_priv clean.
  NOT runtime-verified. Sweep otherwise clean — no missing/leftover scripts elsewhere.
- **Polish: full Examine() coverage on NPCs.** 5 of 9 live NPC scripts had Examine(); the 4
  quest/marriage scripts (Quest_OrcRaiders/Quest_RaiderChief/Ratcatcher1/marriage) did not, so
  examining them did nothing. Appended a one-line flavour Examine() to each (Captain/scout/
  ratcatcher/priest). Non-privileged Output only. NOTE: used ASCII punctuation, not em-dashes —
  em-dash (U+2014) is not latin-1-encodable and safer to avoid in RSL Output strings. Verified:
  every live NPC script now has Examine() (full coverage), audit_calls clean. NOT runtime-verified.
- **Full health sweep + new audit_item_obtainability.py + DATA-LOSS INCIDENT (fixed).** Ran the
  whole audit suite after the boss/quest/spell/NPC expansions — all green (round-trips, media
  refs, calls, meshes; only the 5 unused stock templates unlisted). Added `audit_item_obtainability.py`:
  confirms every Items.dat entry is granted by some script, resolving BOTH literal GiveItem("X")
  AND the quest `RewardItem$ = "X" ... GiveItem(..,RewardItem,..)` variable pattern. Result: every
  gameplay item obtainable; only the vestigial basic Sword/Shield are debug-only (intentional,
  superseded by the starting Long Sword/Scarred Shield — flagged in the tool's INTENTIONAL set).
  **INCIDENT:** the prior tick's Examine-append heredoc opened files in `'w'` (TRUNCATES on open)
  then `.write()` failed encoding an em-dash (U+2014) to latin-1 — Python truncates BEFORE the
  encode error, so `Quest_RaiderChief.rsl` was left empty; the ASCII retry then wrote only the
  Examine block onto the empty file, destroying the quest's Main(). Blast radius = that one file
  (others ASCII / written before the throw). RESTORED full quest (Write tool, ASCII punctuation).
  Verified: Main+reward+Examine present, zero non-ASCII in all 4 scripts (no mojibake), obtainability
  shows Imperial Armor OK via Quest_RaiderChief, audit_calls clean, validate.py PASS. LESSON: never
  truncate-open before an operation that can throw; write to a temp + atomic-replace, or encode the
  string before opening. (Same SafeWrite discipline the engine uses for .dat files.)
- **Diff-review + Heroes' Fate asset attribution.** Doc-vs-data check: guide accurate (all 8
  spells named + "Eight spells"; all 3 quests listed) — no drift. Added a "Credits & asset
  attribution" section to the guide crediting the HF-origin meshes/textures/sounds/animations
  (used with permission) and distinguishing them from RCCE2-authored scripts/data/tooling — the
  right thing for a reusable sample project. Flagged (didn't touch) tracked runtime artifacts
  from the user's playtesting that clutter the diff: `data/Last Username.dat`, `data/Areas/Radar/*.rdr`.
- **New-player onboarding fixes (Login.rsl).** (1) In-game welcome said "Spell Trainer teaches
  6 spells" — stale; now 8 (+ mentions the merchant). Player-visible drift fixed. (2) New
  characters spawned with 0 gold, locking them out of the merchant/economy showcase until they
  grind loot — added a 50-gold first-login purse (ChangeGold; Login allowlisted) so they can
  sample the shop immediately. Guide starting-kit line updated. Verified: audit_calls + audit_priv
  clean. NOT runtime-verified.
- **Event-script drift scan + respawn-HP fix (Death.rsl).** Reviewed Death + all edited event
  scripts; drift scan found no other stale spell-counts (the "6 spells" was isolated to Login).
  Death.rsl respawned the player at a hardcoded `SetAttribute("Health", 50)` — clearly written
  for the old ~100 max HP (~half), but ~5% once max HP became 1000 (same drift class as "6 spells":
  a magic number stale vs a changed parameter). Fixed to `MaxAttribute(.,"Health") / 2` — robust
  to whatever max-HP the user settles on, restoring the apparent half-health intent. audit_calls +
  audit_priv clean (Death allowlisted). NOT runtime-verified. Death.rsl otherwise coherent (valid
  Death 1/2/3 anims, respawn at Plains/Begin, no-gold-loss — forgiving).
- **Health-restoration made scale-invariant (Heal/Regeneration/Healing Potion).** Same drift
  class, systemic: all script HEALTH heals were flat values calibrated for the old ~100 max HP
  — Heal Rand(25,45) (~5% of 1000), Regen Rand(8,16)/tick, Potion 50 — so the whole restorative
  showcase was trivial at the shipped 1000 max. Converted each to `(maxhp * pct) / 100`: Heal
  25-45%, Regen 8-16%/tick, Potion 50%. PROVABLY scale-invariant — resolves to the EXACT original
  numbers at max=100 and scales to 250-450/400-800/500 at max=1000 (sanity-checked both scales).
  Not a balance guess: correct at ANY max-HP the user picks. MANA heals (Meditation, Mana Potion)
  left flat — mana max is still 100, so no drift. Updated the Healing Potion misc_data ("Restores
  50 health" -> "...restores half your health"). Verified: validate.py PASS (Items round-trip),
  audit_calls clean. NOT runtime-verified. (Combat DAMAGE side is separate — orc Str already bumped;
  the root player-max-HP=1000 anomaly remains the user's balance call, now flagged repeatedly.)

- **Damage-spell verification (no change) + crafting scoped & DECLINED.** Confirmed damage spells
  are well-calibrated vs enemy HP (Fireball 30-60, FrostBolt 18-34, Lightning 35-60, FlameNova
  12-28 AoE vs rat 30 / orc 120 / boss 300) — no HP-scale drift (enemies aren't at 1000). Noted
  but left a minor ambiguous projectile-vs-script damage-layering (~10, may not even apply; not
  blind-tweaking functional balance). Evaluated the `BlackSmithing Skill Template` as net-new
  crafting content and DECLINED: needs 7 new items (Forge Hammer + Iron/Steel/Mythril Bar+Sword,
  none exist) + icons (uncertain textures), a material-source economy, AND a "Blacksmithing"
  attribute that DOESN'T EXIST (project attrs = Health/Mana/Strength/Dexterity/Speed/Magic/
  Toughness/Swimming; adding one = engine/config change, off-limits) — plus it stores XP in
  ActorGlobal 8, colliding with Login's League system. A half-built crafting system would lower
  showcase quality; correct call is to decline within the content-only/no-engine constraints.

## LOOP STATUS: default-project overhaul COMPLETE (paused)

The "build a better default project (content-only, no engine)" goal is done. Delivered: 3 zones
(Plains town / Test Zone wilds / Northern Shrine lore hub), 8 spells (4 damage + 3 restorative +
1 buff), 3 quests + a named mini-boss (Grukk), an activated faction system (Wildkin packs),
tiered enemy loot + a gold economy, named NPCs with examine text across all zones, weather +
ambient sound, HF asset attribution, and a byte-faithful Python codec + 6 audit tools. All audits
green; HP-scale drift swept (heals/respawn now scale-invariant). Loop PAUSED after exhausting
high-value low-risk content-only work. REMAINING (needs user / opt-in): (1) BALANCE — player max
HP=1000 anomaly drives all the flat-value mismatches; lower to ~100-150 to auto-calibrate, or
keep and accept the scale; needs playtest. (2) Net-new systems (crafting/mounts/more zones) need
new assets + possibly engine/attribute changes. Re-fire /loop with direction to resume.

## Backlog (roughly highest-leverage first)

1. **Sound the world.** Copy a curated set of HF sounds into `data/Sounds/`,
   register them in `Sounds.dat` via `MediaDB.add_file`. Then wire footsteps,
   ambient forest, combat hits. Biggest perceived-quality jump for least risk.
2. **Items catalog.** Add a real starter set: a few weapons (dagger/mace/bow with
   the existing `Arrow` projectile), armour pieces (reuse shield meshes + add a
   couple from HF Equipment), and consumable potions (Health/Mana) wired to
   `I_Potion` eat-effects. Verify mesh/icon IDs exist or register them first.
3. **Spell variety.** A damage line beyond Fireball (Frost Bolt / Lightning) —
   needs a projectile + emitter; register the projectile and any new particle
   texture first. A buff (Faith Armor, icon 72) once duration handling is settled.
4. **Actors / NPCs.** Decode `Server Data/Actors.dat` fully (the big record — see
   ReadActorInstance in `src/Modules/Actors.bb`) and add: the spell trainer NPC,
   a vendor, a quest-giver, and 2-3 monster types reusing existing/HF meshes.
5. **A real starting zone.** Port one HF zone (visual `Data/Areas/*.dat` +
   referenced meshes/textures) and author the matching gameplay
   `Server Data/Areas/*.dat`. Decode the Area format from `ServerAreas.bb` /
   `ClientAreas.bb` first. Replace the "Test*"/"ha" zones as the default spawn.
6. **A quest chain.** Build on `quest.rsl` / `Ratcatcher1.rsl` patterns — a short
   intro quest that ties the trainer, a monster, and a reward together.
7. **Factions / Money / progression polish.** Sensible starting factions, money
   denominations, level curve.

## Hard rules / gotchas (carry forward)

- **Always `validate.py` first**, and have every generator re-parse its own output
  and assert the existing catalog is an untouched prefix before writing.
- Media `.dat` are index+blob; **append only** (lowest free ID), never repack.
- Strings are latin-1, 4-byte length-prefixed. IDs are signed 16-bit.
- Reference only assets that are registered (or register them in the same change).
- RSL content scripts ship as `.rsl`; the engine loads them by the `Script$` name
  in the catalog record, entry method = `SMethod$` (usually `Main`).
- Runtime behaviour can't be verified here (no headless server+client in this loop);
  cap claims at format-correct + asset-references-resolve. Note what's unverified.
