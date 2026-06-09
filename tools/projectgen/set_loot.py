"""Iteration 7 content: attach MonsterLoot.rsl as the DeathScript on monster
spawns so kills drop gold (+ a chance of a potion). Surgical & idempotent."""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREAS = os.path.join(DATA, 'Server Data', 'Areas')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

LOOT_SCRIPT = 'MonsterLoot'
# area -> set of monster actor ids whose spawns should drop loot
PLAN = {'Test Zone': {3, 4}}  # Rat/Critter, Orc/Raider

def main():
    if not os.path.exists(os.path.join(SCRIPTS, LOOT_SCRIPT + '.rsl')):
        print(f"ERROR: {LOOT_SCRIPT}.rsl missing"); return 1

    for area_name, monster_ids in PLAN.items():
        path = os.path.join(AREAS, area_name + '.dat')
        raw = open(path, 'rb').read()
        area = rcdata.read_server_area(raw)
        orig = [dict(s) for s in area['spawns']]
        changed = []
        for i, s in enumerate(area['spawns']):
            if s['max'] > 0 and s['actor'] in monster_ids and s['death_script'] != LOOT_SCRIPT:
                s['death_script'] = LOOT_SCRIPT
                changed.append(i)
                print(f"  {area_name}: slot {i} (actor {s['actor']}) death_script <- {LOOT_SCRIPT}")
        if not changed:
            print(f"  {area_name}: nothing to do"); continue

        out = rcdata.write_server_area(area)
        chk = rcdata.read_server_area(out)
        for k in area:
            if k == 'spawns':
                continue
            assert chk[k] == area[k], f"non-spawn section '{k}' changed"
        for i in range(len(orig)):
            if i in changed:
                continue
            assert chk['spawns'][i] == orig[i], f"untouched slot {i} changed"
        with open(path + '.tmp', 'wb') as f:
            f.write(out)
        os.replace(path + '.tmp', path)
        print(f"  Wrote {area_name}.dat ({len(changed)} slot(s)).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
