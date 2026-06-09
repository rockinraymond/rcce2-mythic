"""Iteration 6 content (part 1): elemental projectiles for new damage spells.

The project shipped Fireball + Arrow only. This adds Frost Bolt (Ice) and
Lightning Bolt (Electricity), reusing emitter configs already on disk
(Data/Emitter Configs/*.rpc) and registered particle textures. Idempotent on name.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
PROJ = os.path.join(DATA, 'Server Data', 'Projectiles.dat')
EMIT = os.path.join(DATA, 'Emitter Configs')
TEXTURES = os.path.join(DATA, 'Game Data', 'Textures.dat')

# DamageType: Ice=4, Electricity=6
NEW = [
    dict(name='Frost Bolt', mesh=-1, emitter1='Default', emitter2='Snow',
         emitter1_tex=10, emitter2_tex=10, homing=1, hit_chance=90,
         damage=8, damage_type=4, speed=68),
    dict(name='Lightning Bolt', mesh=-1, emitter1='Default', emitter2='Flame',
         emitter1_tex=79, emitter2_tex=79, homing=1, hit_chance=95,
         damage=12, damage_type=6, speed=95),
]

def main():
    raw = open(PROJ, 'rb').read()
    projs = rcdata.read_projectiles(raw)
    names = {p['name'].upper() for p in projs}
    used = {p['id'] for p in projs}
    tex = rcdata.MediaDB(open(TEXTURES, 'rb').read(), rcdata.TEXTURE).entries()

    def nid():
        i = 0
        while i in used:
            i += 1
        return i

    added = []
    for spec in NEW:
        if spec['name'].upper() in names:
            print(f"  skip (exists): {spec['name']}"); continue
        for em in (spec['emitter1'], spec['emitter2']):
            if em and not os.path.exists(os.path.join(EMIT, em + '.rpc')):
                print(f"  ERROR: emitter config {em}.rpc missing"); return 1
        for t in (spec['emitter1_tex'], spec['emitter2_tex']):
            if t not in tex:
                print(f"  ERROR: emitter texture {t} not registered"); return 1
        i = nid(); used.add(i)
        p = dict(id=i, **spec)
        projs.append(p); added.append((i, spec['name']))
        print(f"  add projectile: id {i} '{spec['name']}' dmg {spec['damage']} "
              f"type {spec['damage_type']} speed {spec['speed']} emit {spec['emitter1']}+{spec['emitter2']}")

    if not added:
        print("Nothing to add."); return 0

    out = rcdata.write_projectiles(projs)
    chk = rcdata.read_projectiles(out)
    assert len(chk) == len(projs) and all(a == b for a, b in zip(projs, chk)), "re-parse mismatch"
    assert out[:len(raw)] == raw, "existing projectile bytes changed — refusing"
    with open(PROJ + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(PROJ + '.tmp', PROJ)
    print(f"Wrote {PROJ}: {len(projs)} projectiles, {len(out)} bytes (was {len(raw)}).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
