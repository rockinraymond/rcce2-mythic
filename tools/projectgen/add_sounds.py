"""Iteration 2 content: give the project a voice.

The default project ships 12 .ogg files under data/Sounds/ but registers NONE
of them in Sounds.dat (GetSound resolves Data\\Sounds\\<Name>). This:
  1. registers those shipped-but-dead sounds, and
  2. copies a few Heroes' Fate magic-cast sounds into data/Sounds/Magic/ and
     registers them, so the starter spells can be made audible.

Idempotent: skips any (name, is3d) already present. Append-only via MediaDB,
so existing index bytes are never disturbed. Emits sound_ids.txt manifest.

Sounds.dat starts empty, so registration order fixes IDs deterministically.
"""
import os, sys, shutil
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SOUNDS_DAT = os.path.join(DATA, 'Game Data', 'Sounds.dat')
SOUNDS_DIR = os.path.join(DATA, 'Sounds')
HF_SOUNDS = r'C:/Users/dyanr/Desktop/HeroesFate/Game/Data/Sounds'
MANIFEST = os.path.join(HERE, 'sound_ids.txt')

# (name relative to data/Sounds, is_3d) — registered in this order.
EXISTING = [
    ('Footsteps\\Carefulstep2.ogg', 1),
    ('Footsteps\\dampstep.ogg',     1),
    ('Forest\\forestday.ogg',       0),
    ('Forest\\forestnight.ogg',     0),
    ('Ice\\Ice cracking.ogg',       0),
    ('Water\\Riverplane.ogg',       0),
    ('Water\\water surface.ogg',    0),
    ('Weather\\Rain.ogg',           0),
    ('Weather\\Thunder1.ogg',       0),
    ('Weather\\Thunder2.ogg',       0),
    ('Weather\\Thunder3.ogg',       0),
    ('Weather\\Wind.ogg',           0),
]

# HF sounds to import: (HF source rel path, dest name under data/Sounds, is_3d).
# Order matters — Sounds.dat fills lowest free id, so new entries get sequential
# ids after the 16 already registered (magic casts = 12-15). Creature vocalizations
# below land at 16-21 (see set_actor_sounds.py which wires them to Speech arrays).
HF_IMPORT = [
    ('Magic/Cast_01.ogg', 'Magic\\Cast_01.ogg', 1),
    ('Magic/Cast_05.ogg', 'Magic\\Cast_05.ogg', 1),
    ('Magic/Cast_08.ogg', 'Magic\\Cast_08.ogg', 1),
    ('Magic/Cast_12.ogg', 'Magic\\Cast_12.ogg', 1),
    # creature vocalizations -> ids 16..21
    ('Animals/Rat/Rat_01.ogg',     'Animals\\Rat\\Rat_01.ogg',     1),  # 16 rat attack
    ('Animals/Rat/Rat_04.ogg',     'Animals\\Rat\\Rat_04.ogg',     1),  # 17 rat hit
    ('Animals/Rat/Rat_07.ogg',     'Animals\\Rat\\Rat_07.ogg',     1),  # 18 rat death
    ('Monsters/TrollAttack_01.ogg','Monsters\\TrollAttack_01.ogg', 1),  # 19 orc attack
    ('Monsters/TrollHit_01.ogg',   'Monsters\\TrollHit_01.ogg',    1),  # 20 orc hit
    ('Monsters/TrollDeath_01.ogg', 'Monsters\\TrollDeath_01.ogg',  1),  # 21 orc death
]

def disk_path(name):
    return os.path.join(SOUNDS_DIR, name.replace('\\', os.sep))

def main():
    raw = open(SOUNDS_DAT, 'rb').read()
    db = rcdata.MediaDB(raw, rcdata.SOUND)
    before = {(e['name'].upper(), e['is_3d']) for e in db.entries().values()}

    # 1. copy HF magic sounds onto disk (skip if already there)
    for src_rel, dest_name, _is3d in HF_IMPORT:
        src = os.path.join(HF_SOUNDS, src_rel)
        dst = disk_path(dest_name)
        if not os.path.exists(src):
            print(f"  ERROR: HF source missing: {src}")
            return 1
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        if not os.path.exists(dst):
            shutil.copy2(src, dst)
            print(f"  copied: {src_rel} -> data/Sounds/{dest_name}")
        else:
            print(f"  on disk already: data/Sounds/{dest_name}")

    # 2. register everything (existing + imported), verifying the file is on disk
    to_register = EXISTING + [(d, i) for _s, d, i in HF_IMPORT]
    added = []
    for name, is3d in to_register:
        if (name.upper(), is3d) in before:
            print(f"  skip (registered): {name}")
            continue
        if not os.path.exists(disk_path(name)):
            print(f"  ERROR: file not on disk, refusing to register: {name}")
            return 1
        nid = db.add_file(name, is_3d=is3d)
        added.append((nid, name, is3d))
        print(f"  register: id {nid:3} 3d{is3d} {name}")

    if added:
        out = db.save()
        # Safety for an index+blob DB: every index slot that was non-zero in the
        # original must still point to the same offset, and the original record
        # blob (everything after the index) must survive as a prefix of the new
        # blob — i.e. we only filled empty slots and appended records.
        orig = rcdata.MediaDB(raw, rcdata.SOUND)
        new = rcdata.MediaDB(out, rcdata.SOUND)
        for i, off in enumerate(orig.index):
            if off != 0:
                assert new.index[i] == off, f"index slot {i} moved — refusing"
        assert bytes(new.blob[:len(orig.blob)]) == bytes(orig.blob), \
            "original record blob changed — refusing"
        rt = new.entries()
        tmp = SOUNDS_DAT + '.tmp'
        with open(tmp, 'wb') as f:
            f.write(out)
        os.replace(tmp, SOUNDS_DAT)
        print(f"Wrote {SOUNDS_DAT}: {len(rt)} sounds, {len(out)} bytes (was {len(raw)}).")
    else:
        print("Nothing to register.")

    # 3. emit manifest of current registrations
    ents = rcdata.MediaDB(open(SOUNDS_DAT, 'rb').read(), rcdata.SOUND).entries()
    with open(MANIFEST, 'w') as f:
        f.write("# RCCE2 Sounds.dat registration manifest (id -> name)\n")
        for i in sorted(ents):
            f.write(f"{i}\t3d{ents[i]['is_3d']}\t{ents[i]['name']}\n")
    print(f"Manifest -> {MANIFEST}")
    return 0

if __name__ == '__main__':
    sys.exit(main())
