"""Iteration 19 content: make kill XP on the intended enemies meaningful.

Kill XP (GameServer.bb:164) = max(1, killedLvl-killerLvl) * Actor.XPMultiplier + Rand(0,20).
The two combat creatures shipped with low multipliers (Rat=1, Orc/Raider=2), so kills
barely contributed to progression. Bump them so farming the intended enemies rewards XP
that, with the gentler LevelUp curve (Lvl*250), reaches the early levels in a demo session.
Surgical: only xp_multiplier changes. Idempotent.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
ACTORS = os.path.join(DATA, 'Server Data', 'Actors.dat')

PLAN = {('Rat', 'Critter'): 5, ('Orc', 'Raider'): 12}

def main():
    raw = open(ACTORS, 'rb').read()
    actors = rcdata.read_actors(raw)
    orig = rcdata.read_actors(raw)
    changed = False
    for a in actors:
        key = (a['race'], a['cls'])
        if key in PLAN and a['xp_multiplier'] != PLAN[key]:
            print(f"  {key[0]}/{key[1]}: xp_multiplier {a['xp_multiplier']} -> {PLAN[key]}")
            a['xp_multiplier'] = PLAN[key]
            changed = True
    if not changed:
        print("Nothing to change."); return 0
    out = rcdata.write_actors(actors)
    chk = rcdata.read_actors(out)
    for new, old in zip(chk, orig):
        for k in new:
            if k == 'xp_multiplier':
                continue
            assert new[k] == old[k], f"non-xp field '{k}' changed on {old['race']}/{old['cls']}"
    with open(ACTORS + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(ACTORS + '.tmp', ACTORS)
    print(f"Wrote {ACTORS} ({len(out)} bytes).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
