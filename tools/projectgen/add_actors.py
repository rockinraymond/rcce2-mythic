"""Iteration 4 content: add creature types (Actor templates).

Actors.dat defines creature *types* by Race+Class; quests/spawns reference them
via ActorID(Race,Class). The project shipped 3 (Human/Fighter player, Stag, Ork).
This adds two combat-ready enemies that reuse already-registered meshes/anims:

  * Rat / Critter  — rat mesh 161, anim set 2. Race/Class deliberately match the
    shipped Ratcatcher1.rsl quest (WaitRace="Rat", WaitClass="Critter") so that
    quest becomes functional once a Rat is spawned in a zone.
  * Orc / Raider   — Ork mesh 81, anim set 3, but AGGRESSIVE (the base Ork type
    ships non-aggressive), so it actually engages players — a real melee enemy.

Idempotent on (race,cls). Append-only; re-parses output and asserts the existing
catalog is an untouched prefix before writing.

NOTE: stat balance and scale/radius are reasoned guesses (no headless server in
this loop to runtime-verify). Meshes 161/81 and body textures 66/81 are confirmed
registered.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
ACTORS = os.path.join(DATA, 'Server Data', 'Actors.dat')
MESHES = os.path.join(DATA, 'Game Data', 'Meshes.dat')
TEXTURES = os.path.join(DATA, 'Game Data', 'Textures.dat')

def main():
    raw = open(ACTORS, 'rb').read()
    actors = rcdata.read_actors(raw)
    have = {(a['race'].upper(), a['cls'].upper()) for a in actors}
    used = {a['id'] for a in actors}

    meshes = rcdata.MediaDB(open(MESHES, 'rb').read(), rcdata.MESH).entries()
    textures = rcdata.MediaDB(open(TEXTURES, 'rb').read(), rcdata.TEXTURE).entries()

    def nid():
        i = 0
        while i in used:
            i += 1
        return i

    # (race, cls, mesh, anim, body_tex, kwargs)
    NEW = [
        ('Rat', 'Critter', 161, 2, 66, dict(
            description='A mangy, oversized rat. A nuisance to new adventurers.',
            scale=1.0, radius=14.0, genders=3, playable=0,
            aggressiveness=1, aggressive_range=45, default_damage_type=0,
            xp_multiplier=1, blood_tex=12,
            attrs={'Health': 30, 'Strength': 6, 'Speed': 22, 'Toughness': 1},
            attrs_max={'Health': 30, 'Strength': 100, 'Speed': 100, 'Toughness': 100,
                       'Mana': 100, 'Dexterity': 100, 'Magic': 100})),
        ('Orc', 'Raider', 81, 3, 81, dict(
            description='A brutish orc raider. Hits hard; hunts on sight.',
            scale=1.0, radius=67.0, genders=3, playable=0,
            aggressiveness=1, aggressive_range=70, default_damage_type=2,  # Bashing
            xp_multiplier=2, blood_tex=12,
            attrs={'Health': 120, 'Strength': 18, 'Speed': 24, 'Toughness': 5},
            attrs_max={'Health': 120, 'Strength': 100, 'Speed': 100, 'Toughness': 100,
                       'Mana': 100, 'Dexterity': 100, 'Magic': 100})),
    ]

    added = []
    for race, cls, mesh, anim, body_tex, kw in NEW:
        if (race.upper(), cls.upper()) in have:
            print(f"  skip (exists): {race}/{cls}")
            continue
        if mesh not in meshes:
            print(f"  ERROR: mesh {mesh} for {race}/{cls} not registered"); return 1
        if body_tex not in textures:
            print(f"  ERROR: body texture {body_tex} for {race}/{cls} not registered"); return 1
        i = nid(); used.add(i)
        a = rcdata.new_actor(i, race, cls, mesh, anim, f_anim=anim, **kw)
        a['male_body_ids'] = [body_tex, -1, -1, -1, -1]
        actors.append(a)
        added.append((i, race, cls))
        print(f"  add actor: id {i} {race}/{cls} mesh {mesh} anim {anim} "
              f"hp {kw['attrs']['Health']} aggr {kw['aggressiveness']}")

    if not added:
        print("Nothing to add; actor catalog already current.")
        return 0

    out = rcdata.write_actors(actors)
    chk = rcdata.read_actors(out)
    assert len(chk) == len(actors), "re-parse count mismatch"
    for a, b in zip(actors, chk):
        assert a == b, f"re-parse mismatch on {a['race']}/{a['cls']}"
    assert out[:len(raw)] == raw, "existing actor bytes changed — refusing"
    with open(ACTORS + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(ACTORS + '.tmp', ACTORS)
    print(f"Wrote {ACTORS}: {len(actors)} actors, {len(out)} bytes (was {len(raw)}).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
