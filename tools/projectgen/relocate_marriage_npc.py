"""Relocate the marriage 'Priest' NPC out of the monster wilds (Test Zone) and into
town (Plains), where a registrar belongs. The marriage feature is a player<->player
system (10,000GP + a second player to target) — solo-unusable but a legitimate
multiplayer demo, so we keep it, just place it sensibly.

  - Plains: fill empty spawn slot with actor 0 (Human) @ wp 1 (a walkable town
    waypoint near the spawn point), actor_script 'marriage', matching the other
    town NPCs' spawn params (max 1, freq 10, range 0, size 5).
  - Test Zone: clear the 'marriage' spawn (max 0 + blank actor_script) so it no
    longer spawns among the rats/orcs.

Surgical: asserts every other section/slot in BOTH files is byte-identical.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AR = os.path.join(DATA, 'Server Data', 'Areas')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

PLAINS = os.path.join(AR, 'Plains.dat')
WILDS = os.path.join(AR, 'Test Zone.dat')
SCRIPT = 'marriage'
PLAINS_WP = 1


def empty_slot(spawns):
    for i, s in enumerate(spawns):
        if (s['max'] == 0 and s['actor'] == 0 and not s['actor_script']
                and not s['script'] and not s['death_script']):
            return i
    return -1


def edit_area(path, mutate):
    raw = open(path, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = [dict(s) for s in area['spawns']]
    touched = mutate(area)
    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"{path}: non-spawn section '{k}' changed"
    for i in range(len(orig)):
        if i in touched:
            continue
        assert chk['spawns'][i] == orig[i], f"{path}: untouched slot {i} changed"
    with open(path + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(path + '.tmp', path)
    return touched


def main():
    if not os.path.exists(os.path.join(SCRIPTS, SCRIPT + '.rsl')):
        print(f"ERROR: {SCRIPT}.rsl missing"); return 1

    # Idempotency: bail if marriage already in Plains.
    parea = rcdata.read_server_area(open(PLAINS, 'rb').read())
    if any(s['actor_script'] == SCRIPT and s['max'] > 0 for s in parea['spawns']):
        print("  skip: marriage already placed in Plains"); return 0

    def add_to_plains(area):
        slot = empty_slot(area['spawns'])
        if slot < 0:
            raise RuntimeError("no empty Plains spawn slot")
        s = area['spawns'][slot]
        s['actor'] = 0; s['waypoint'] = PLAINS_WP; s['actor_script'] = SCRIPT
        s['max'] = 1; s['frequency'] = 10; s['range'] = 0.0; s['size'] = 5.0
        print(f"  Plains: slot {slot} <- Priest (marriage) @ wp {PLAINS_WP}")
        return {slot}

    def clear_in_wilds(area):
        touched = set()
        for i, s in enumerate(area['spawns']):
            if s['actor_script'] == SCRIPT:
                s['actor_script'] = ''; s['max'] = 0
                touched.add(i)
                print(f"  Test Zone: slot {i} marriage spawn cleared (max 0, script blank)")
        if not touched:
            raise RuntimeError("no marriage spawn found in Test Zone")
        return touched

    edit_area(PLAINS, add_to_plains)
    edit_area(WILDS, clear_in_wilds)
    print("  Wrote Plains.dat + Test Zone.dat.")
    return 0


if __name__ == '__main__':
    sys.exit(main())
