# projectgen — RCCE2 default-project content toolkit

A small, **verified** Python toolkit for reading and writing RCCE2 / RealmCrafter
binary project files, used to build out the shipped default/sample project under
`data/`. It does **not** touch engine source.

## Why this exists

The default project is intentionally minimal (it was a smoke-test world: 2 items,
1 spell, 3 actors, a few "Test" zones). To turn it into something that *showcases*
the engine for new users, we need to add a lot of content — spells, items, actors,
zones, scripts. Most of that content lives in length-prefixed **binary** `.dat`
files. Editing them by hand is error-prone; one wrong byte offset corrupts the
whole catalog. This toolkit models the formats faithfully so content can be
generated and round-trip-verified before it ever reaches the engine.

## Files

- `rcdata.py` — the codec. Low-level BlitzForge stream primitives (all little-endian:
  byte / short(signed16) / int(signed32) / float / string(4-byte-len prefix)), plus
  typed readers/writers for `Spells.dat`, `Items.dat`, and a `MediaDB` class for the
  `Meshes.dat` / `Textures.dat` / `Sounds.dat` index databases.
- `validate.py` — round-trip proof. Reads each real project file, re-encodes it, and
  asserts byte-for-byte equality. **Run this first, every iteration**, before trusting
  the codec to generate anything.
- `add_spells.py` — iteration 1 content: adds the restorative starter spells.
- `rcproject.py` — git-friendly project format ([Issue #32](https://github.com/RydeTec/rcce2/issues/32),
  phases 1+2). Round-trips the gameplay `.dat` files **and the four media
  databases** to/from JSON so they diff and merge in git; see below.
- `test_mediadb_roundtrip.py` — acceptance suite for the phase-2 media-DB
  codec (real-data byte identity, dead-span fixtures, JSON-mutation vs the
  engine's `Add*ToDatabase` write order, error cases, CLI gate).

## Git-friendly project format (`rcproject.py`)

Projects store content as opaque binary `.dat` files, so two people editing the
same catalog produce an **unmergeable binary conflict**. [Issue #32](https://github.com/RydeTec/rcce2/issues/32)
("Make projects more git friendly") proposes keeping an editable, diffable
representation in the tree and converting to `.dat` only at publish time. `rcproject.py`
is phase 1 of that: a round-trip `.dat ↔ JSON` CLI built on the same byte-faithful
`rcdata.py` codec `validate.py` proves.

```sh
python tools/projectgen/rcproject.py export data text/   # .dat -> .json   (decode for git)
# ...edit/merge the JSON, then before running the server / publishing:
python tools/projectgen/rcproject.py build  text/ data   # .json -> .dat   (the "obfuscation" form)
python tools/projectgen/rcproject.py verify data         # prove the round-trip is byte-exact
```

- **`export`** decodes every supported `.dat` into pretty-printed, key-sorted JSON
  (one `<name>.dat.json` mirroring the source path). The output is deterministic, so
  re-exporting an unchanged project yields byte-identical text — `git diff` shows
  exactly which records changed.
- **`build`** re-encodes the JSON back to `.dat` (atomic temp-file + replace).
- **`verify`** is the safety gate: for every supported file it decodes → serialises to
  JSON → parses back → re-encodes and asserts the bytes equal the original (and that the
  JSON is deterministic). Run it after editing the codec or before trusting a publish.

**Scope (phase 1):** the symmetric value-codec formats — `Spells`, `Items`, `Actors`,
`Projectiles`, `Factions`, server-side `Areas` (gameplay) and client-side `Areas`
(visual). `Server Data/Areas/Ownerships/*.dat` (a different format) and the legacy
`Areas/ha.dat` stub are skipped, matching `validate.py`'s known-good set.

**Scope (phase 2):** the four media databases — `Game Data/Meshes.dat`, `Textures.dat`,
`Sounds.dat`, `Music.dat`. These are index+blob structures, not value codecs
(a 65,535-slot offset index + records in insertion order), so their JSON form captures
three things: the sparse `entries` map (what humans diff),
the insertion `order` (ids sorted by blob offset — enough to re-derive every offset),
and any dead `gaps` (hex-encoded — note the engine itself never produces these:
`Remove*FromDatabase` compacts via a full rewrite, so gaps only appear in
crash-interrupted or hand-edited files, which the codec preserves faithfully
instead of corrupting). Rebuild is byte-identical; the ~262 KB zero-heavy
index serialises to a few KB of JSON. The actual assets are loose files and were always
git-friendly — these indexes were the merge-conflict magnets (every asset import by any
author rewrote the same opaque quarter-megabyte binary). Note `Music.dat` records are a
bare filename string (no flags byte — `AddMusicToDatabase` differs from its siblings),
and `Game Data/xMeshes.dat` is a legacy artifact with zero references in `src/`
(reported as unrecognised, untouched).

**Deferred (phase 3):** `Gubbins.dat` / `Animations.dat` / `Interface.dat` (different
formats); wiring `verify` into CI; optional compaction of the fixed-size area arrays
(150 triggers / 2000 waypoints / 1000 spawns are serialised in full today — faithful and
diffable, but verbose); a GUE/Loom export-on-save hook so authors never touch the CLI.

## The formats (verified against engine source)

### Catalog files (`Server Data/Items.dat`, `Server Data/Spells.dat`)
A flat concatenation of records, read until EOF. No header, no count. Each record
starts with a 2-byte signed ID. Strings are `int32 length + raw bytes` (latin-1,
not null-terminated). Item attributes are 40 shorts stored as `value + 5000`.
Source of truth: `src/Modules/Items.bb` (LoadItems/SaveItems), `src/Modules/Spells.bb`.

### Media index databases (`Game Data/{Meshes,Textures,Sounds}.dat`)
**Not** packed blobs — they are *indexes* to on-disk asset files:
- 65535 `int32` slots. `slot[ID]` = byte offset of that ID's record, or 0 if empty.
- Records appended after the index, in **insertion order** (with gaps left by editor
  deletes — so do not assume ID order == file order).
  - texture record: `flags(short) + name(string)`
  - mesh    record: `isAnim(byte) + scale(float) + x(float) + y(float) + z(float) + shader(short) + name(string)`
  - sound   record: `is3D(byte) + name(string)`
- `name` is the asset's path relative to `data/` (e.g. `Spell Icons\Spells\Heal.bmp`).

`MediaDB` preserves the original record blob verbatim and mutates only by *appending*
a record at EOF + patching one index slot — exactly mirroring `AddMeshToDatabase` /
`AddTextureToDatabase` / `AddSoundToDatabase` in `src/Modules/Media.bb`. This is why
`.save()` of an unmodified DB is byte-identical to the input.

## How to add content safely

1. `python validate.py` → confirm `ALL ROUND-TRIPS PASSED`.
2. Write a generator that loads the catalog, appends entries (referencing only
   assets confirmed present), re-encodes, **re-parses its own output**, and asserts
   the original bytes are an untouched prefix before writing.
3. Re-run `validate.py`.

## Proof level

- Codec faithfulness: **byte-exact round-trip** of all 5 live project files (strong).
- New-content correctness: format/round-trip verified; texture-ID and script-file
  existence checked. In-game behaviour is **not** runtime-verified here (needs a
  running Server.exe + client). RSL scripts are authored against patterns proven by
  the shipped `Spell_Fireball.rsl` / `Click_Test.rsl` / `quest.rsl`.
