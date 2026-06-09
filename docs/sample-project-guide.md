# RCCE2 Sample Project — Guide

This is a tour of the content shipped in the default project under [`data/`](../data),
written for someone discovering RCCE2 for the first time. It doubles as a worked
example: each section points at the exact files that make the feature work, so you
can learn the engine by reading — and editing — real content.

> Everything here is plain project data (binary `.dat` catalogs + text `.rsl`
> scripts). None of it requires touching engine source. The binary catalogs were
> authored with the small Python codec in [`tools/projectgen/`](../tools/projectgen)
> (see its `README.md` / `PLAN.md` if you want to script bulk content changes).
> New to writing `.rsl` scripts? The [RSL language guide](scripting/language.md)
> teaches the syntax, entry-point functions, and the privilege rules first.

## The 60-second loop a new player experiences

1. **Log in.** A welcome message greets you and a **starting kit** lands in your
   pack — a Long Sword, a Scarred Shield, 3 Potions of Healing, **50 gold**, and the
   *Heal* spell. (Press **I** to open your inventory and equip the sword & shield.)
2. **Spawn in the Plains.** A **Spell Trainer** stands nearby — right-click to learn
   any of seven spells. A **town Captain** offers the quest *Raiders at the Gate*, and a
   **General Store merchant** sells potions and gear for the gold you earn.
3. **Travel through the portal to the wilds (Test Zone).** Rats and aggressive Orc
   Raiders roam here. Fight them with melee or your elemental spells. Deeper in lurks
   **Grukk, the Raider-Chief** — a named mini-boss with a guaranteed reward.
4. **Loot.** Slain creatures drop gold and, sometimes, a Potion of Healing.
5. **Quests.** Kill rats for the Rat Catcher (an NPC in the wilds) and orc raiders for
   the Captain. Turn each in for gold, XP, and gear.
6. **Atmosphere.** Each zone has ambient sound and dynamic weather — forest birds and
   showers on the plains, fog in the wilds, flowing water and snow at the shrine — plus
   per-actor footsteps and creature attack/hit/death cries as you move and fight.

## Zones & how they connect

Portals live in the area files (gameplay = `data/Server Data/Areas/*.dat`,
visuals = `data/Areas/*.dat`).

| Zone | Role | Connects to |
|---|---|---|
| **Plains** | Starting town. Spawn point (`Begin` portal), Spell Trainer, quest Captain, General Store merchant, a Priest (player marriage), and a grazing Stag. | → Test Zone, → Northern Shrine |
| **Test Zone** | The wilds. Rats + Orc Raiders, the Rat Catcher quest NPC, and a pair of **Wounded Scouts** (orc-raid survivors who point you back to the Captain). | → Plains |
| **Northern Shrine** | A serene waterfall shrine (snowy). A **Shrine Keeper** restores your health & mana on right-click; a **Shrine Oracle** shares the realm's lore (the orc raids, Grukk, the old faith) — a peaceful lore hub and a safe place to recover between fights. | (entry portal) |

New characters start at `Plains` / `Begin` — set on the **Human/Fighter** actor
template's `StartArea`/`StartPortal` (`data/Server Data/Actors.dat`).

## Spells (`data/Server Data/Spells.dat` + `Scripts/Spell_*.rsl`)

Eight spells: four damage (three single-target schools + an AoE), three restoratives, and a
defensive buff. Learn them from the Spell Trainer; the spell's behaviour lives in its `.rsl` script.

| Spell | Type | Script |
|---|---|---|
| Fireball | Fire damage (projectile) | `Spell_Fireball.rsl` |
| Frost Bolt | Ice damage (projectile) | `Spell_FrostBolt.rsl` |
| Lightning Bolt | Electricity damage (fast projectile) | `Spell_Lightning.rsl` |
| Flame Nova | Fire damage to **all** nearby foes (AoE) | `Spell_FlameNova.rsl` |
| Heal | Instant self-heal | `Spell_Heal.rsl` |
| Regeneration | Heal-over-time | `Spell_Regeneration.rsl` |
| Meditation | Restore mana over time | `Spell_Meditation.rsl` |
| Faith Armor | Timed defensive buff (+Toughness/armor) | `Spell_FaithArmor.rsl` |

The three damage spells fire **projectiles** defined in `Projectiles.dat` (Fireball,
Frost Bolt, Lightning Bolt), each with its own emitter config (`data/Emitter Configs/*.rpc`)
and damage type. A spell script's anatomy (see `Spell_FrostBolt.rsl`): check/deduct
mana → play a cast animation + sound → `FireProjectile` → apply damage.

## Items (`data/Server Data/Items.dat`)

A starter catalog with real progression: **Long Sword** (slashing weapon), a 3-piece
**Imperial armour set** (Helmet/Armor + Scarred Shield, with a Toughness bonus on the
chest), an **Adventurine Ring** (+Magic while worn), three **potions** (Healing & Mana
are instant via `Item_*.rsl` scripts; the *Elixir of Strength* is a timed +5 Strength
buff driven purely by item data), and the *Rat Catcher Medalion* quest trophy. Item
icons live in `data/Textures/Items/`.

## Creatures (`data/Server Data/Actors.dat`)

Creature *types* are defined by Race + Class and reference a mesh + animation set.

| Race / Class | Role |
|---|---|
| Human / Fighter | The playable character (also reused for NPCs) |
| Stag | Passive animal |
| Rat / Critter | Weak enemy — the Rat Catcher quest target |
| Orc / Raider | Aggressive melee enemy — the Captain's quest target |
| Orc / Warlord | **Grukk, the Raider-Chief** — a named mini-boss deep in the wilds (300 HP, hits hard); drops a guaranteed Elixir of Strength + gold and a chance at an Imperial Helmet |

Creatures are placed into zones as **spawn points** in the gameplay area files
(`SpawnActor`, `SpawnMax`, `SpawnActorScript`, `SpawnDeathScript`). A spawn's
`SpawnActorScript` is the NPC's right-click behaviour (e.g. a quest); its
`SpawnDeathScript` runs on death (e.g. `MonsterLoot.rsl`).

**Factions & aggression** (`data/Server Data/Factions.dat`, the actors' `DefaultFaction`
+ `Aggressiveness`). Aggressiveness is `0 Passive / 1 Defensive / 2 Aggressive /
3 Non-combatant`. The player and town NPCs are faction **Traders**; rats and Orc Raiders
are faction **Wildkin**, which is hostile to Traders (rating 0) and allied to itself
(200). The Orc Raiders are **Aggressive** — they hunt the player on sight within range,
and call nearby Wildkin (the rats) into the fight, so the wilds attack in coordinated
packs. Because Wildkin are allied, orcs ignore the rats (and each other) as targets, which
keeps the Rat Catcher's rat population intact. This is the whole faction loop in miniature:
flip a `DefaultFaction` and a rating and a creature changes who it fights.

## Quests (`data/Server Data/Scripts/`)

- **Rat Catcher** (`Ratcatcher1.rsl`) — kill 2 rats for an NPC in the wilds.
- **Raiders at the Gate** (`Quest_OrcRaiders.rsl`) — the Plains Captain sends you to
  slay 3 Orc Raiders; rewards gold, XP, and an Imperial Helmet.
- **The Raider-Chief** (`Quest_RaiderChief.rsl`) — a Wounded Scout in the wilds asks you
  to avenge the slaughtered patrol by slaying **Grukk** (the Orc Warlord mini-boss deep in
  the zone); rewards gold, XP, and the Imperial Armor breastplate — completing the Imperial
  set with the Helmet above.

A quest script uses `NewQuest` / `WaitKill(player, actorID, n)` / `UpdateQuest` /
`CompleteQuest`, then grants rewards with `GiveItem` / `GiveXp` / `ChangeGold`.

> **Privileged scripts — important.** Many gameplay BVMs are privileged: `GiveItem`,
> `ChangeGold`, `GiveXp`, **`SetAttribute`** (used by every damage/heal effect), `Warp`,
> `SetActorTarget`, … A content script that calls them only takes effect if its name is
> listed in `data/Server Data/Privileged Scripts.dat`. This catches people out: a spell
> that does `SetAttribute(target, "Health", …)` will *silently do nothing* until the
> script is allowlisted. The shipped quest, loot, login, merchant, **spell, potion, and
> death** scripts are all listed there — if you add a new spell or reward script, add it
> too.

## Loot, economy & the starting kit

- **Loot:** each spawn's `SpawnDeathScript` runs on death. Rats use `MonsterLoot.rsl`
  (a little gold + a 25% Potion of Healing). Orc Raiders — the tougher quest target — use
  `OrcLoot.rsl`, a richer tiered table: more gold plus a roll for a Healing Potion (common),
  a Mana Potion (uncommon), or a rare Elixir of Strength. (A DeathScript receives only the
  *killer* — `Actor()` — not the corpse, so differentiated loot needs a per-enemy script
  rather than one shared script branching on the victim.)
- **Merchant:** `Click_Merchant.rsl` is a gold *sink* — a scripted shop (dialog menu, no
  trade window) that checks `Gold()` and sells potions/gear via `ChangeGold` + `GiveItem`,
  completing the earn-and-spend loop. Stock: Potions of Healing/Mana (25g), a Long Sword
  (85g), an **Adventurine Ring** (120g, +Magic while worn) and an **Elixir of Strength**
  (60g — the data-driven timed buff above). The ring and elixir are *only* available here,
  giving gold a real purpose.
- **Starting kit + welcome:** `Login.rsl` greets the player and, on the first-ever
  login, grants the starting gear and the Heal spell.

## Ambient sound & weather

- **Ambient sound:** each gameplay zone has a `SoundZone` in its visual area file —
  wind across the open plains, forest in the wilds, flowing water at the shrine. These are
  *channel-managed* (one reused channel per zone). Sounds are registered in
  `data/Game Data/Sounds.dat` (files under `data/Sounds/`).
- **Spell sounds:** casting plays a one-shot sound (see `PlaySound` in the `Spell_*.rsl`
  scripts).
- **Per-actor footstep/combat sounds.** Wired through each actor template's `Actors.dat`
  Speech arrays (`MSpeechIDs`/`FSpeechIDs`, indexed by event — `Attack1`, `Hit1`, `Death`,
  `FootstepDry`/`Wet`). The engine plays the indexed sound positionally on that actor when
  the event fires. Currently wired: the Human/Fighter (player) footsteps, the Rat/Critter
  squeaks, and the Orc/Raider + Orc/Warlord (Grukk) troll vocalizations + heavy footstep.
  These were previously **OFF** because the old fire-and-forget `EmitSound` — fired every
  step of every moving actor — exhausted the client's audio sources over a session and
  hard-crashed it ([rcce2#489](https://github.com/RydeTec/rcce2/issues/489)). That engine
  bug is fixed: actor sounds now route through a bounded recycling channel pool
  (`EmitActorSound`, [rcce2#540](https://github.com/RydeTec/rcce2/pull/540)) that caps the
  live channel count, so they are safe to ship and are now ON. The wiring is reproducible
  via `tools/projectgen/set_actor_sounds.py` (idempotent; only the speech arrays change).
- **Weather:** the `WeatherChance[5]` weights at the head of each gameplay area file
  drive dynamic weather — showers/storms on the plains, rain/fog in the wilds, snow at
  the Northern Shrine.

## Running it

Build the engine + tools (see the repo [`ReadMe.md`](../ReadMe.md) / `compile.bat`),
then launch the authoritative server and connect a client:

```
bin\Server.exe -UNLOCK      # starts the game server (UDP 25000)
bin\Client.exe              # or bin\ClientRS.exe — create an account, make a
                            # character, and enter the world
```

## Want to extend it?

Start small and copy the patterns above:

- **A new spell:** add a record to `Spells.dat`, write a `Spell_X.rsl` (copy an
  existing one), and teach it from `Click_Trainer.rsl`.
- **A new enemy:** add an `Actors.dat` template (reuse an existing mesh + anim set),
  then add a spawn point to a zone's gameplay area file.
- **A new quest:** copy `Quest_OrcRaiders.rsl`, change the target/reward, spawn an NPC
  with it as `SpawnActorScript`, and add the script name to `Privileged Scripts.dat`.

The Python codec in `tools/projectgen/` can read/write every catalog and area file
byte-faithfully if you'd rather script these edits than use the editor.

## Credits & asset attribution

Several of the art and audio assets in this sample project — creature meshes (e.g. the
Troll used for the Orc Raiders and Warlord), their textures, the magic/ambient/creature
**sounds**, and the creature **animation** sets — originate from the **Heroes' Fate**
project and are included here **with permission** for use as RCCE2 sample content. They
remain the property of their original authors; if you reuse this sample project as the
basis for your own game, check the Heroes' Fate licensing terms before redistributing
those specific assets. The RCCE2-authored content (the `.rsl` scripts, the catalog/area
data wiring, and the `tools/projectgen/` codec) follows the repo's licensing — see the
[`ReadMe.md`](../ReadMe.md) License section.
