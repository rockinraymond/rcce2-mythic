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
