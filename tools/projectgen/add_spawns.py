"""Iteration 5 content: place creatures in the world.

The creature types added in iter 4 (Rat/Critter=3, Orc/Raider=4) existed but were
spawned nowhere. Test Zone already hosts the shipped Ratcatcher1 quest NPC (spawn
slot 0) but no rats — so the quest couldn't be completed. This activates spawns by
filling empty spawn slots (a spawn is live when SpawnMax>0; Server.bb:544):

  Test Zone:
    * Rat/Critter (actor 3) x3 at waypoint 0 (by the quest NPC) -> Ratcatcher works
    * Orc/Raider  (actor 4) x2 at waypoint 4 -> showcases the aggressive enemy

Idempotent: skips if a live spawn (max>0) for that actor already exists. Surgical:
asserts every non-spawn section and every untouched spawn slot is byte-for-byte
unchanged before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREAS = os.path.join(DATA, 'Server Data', 'Areas')
ACTORS = os.path.join(DATA, 'Server Data', 'Actors.dat')

# area filename (no .dat) -> list of spawns to ensure present
PLAN = {
    'Test Zone': [
        dict(actor=3, waypoint=0, max=3, frequency=8,  range=22.0, size=5.0),  # Rats
        dict(actor=4, waypoint=4, max=2, frequency=15, range=20.0, size=5.0),  # Orc Raiders
    ],
}

def empty_slot(spawns, start=0):
    for i in range(start, len(spawns)):
        s = spawns[i]
        if s['max'] == 0 and s['actor'] == 0 and not s['actor_script'] \
           and not s['script'] and not s['death_script']:
            return i
    return -1

def main():
    actor_ids = {a['id'] for a in rcdata.read_actors(open(ACTORS, 'rb').read())}

    for area_name, wants in PLAN.items():
        path = os.path.join(AREAS, area_name + '.dat')
        raw = open(path, 'rb').read()
        area = rcdata.read_server_area(raw)
        orig_spawns = [dict(s) for s in area['spawns']]
        changed = []

        for want in wants:
            if want['actor'] not in actor_ids:
                print(f"  ERROR: actor {want['actor']} not in Actors.dat"); return 1
            # already live?
            if any(s['actor'] == want['actor'] and s['max'] > 0 for s in area['spawns']):
                print(f"  skip ({area_name}): actor {want['actor']} already spawns")
                continue
            slot = empty_slot(area['spawns'])
            if slot < 0:
                print(f"  ERROR: no empty spawn slot in {area_name}"); return 1
            s = area['spawns'][slot]
            s['actor'] = want['actor']; s['waypoint'] = want['waypoint']
            s['max'] = want['max']; s['frequency'] = want['frequency']
            s['range'] = want['range']; s['size'] = want['size']
            changed.append(slot)
            print(f"  {area_name}: slot {slot} <- actor {want['actor']} "
                  f"max {want['max']} freq {want['frequency']}s wp {want['waypoint']}")

        if not changed:
            continue

        out = rcdata.write_server_area(area)
        # surgical safety: re-read and assert ONLY the intended spawn slots differ
        chk = rcdata.read_server_area(out)
        for k in area:
            if k == 'spawns':
                continue
            assert chk[k] == area[k], f"non-spawn section '{k}' changed in {area_name}"
        for i in range(len(orig_spawns)):
            if i in changed:
                continue
            assert chk['spawns'][i] == orig_spawns[i], \
                f"untouched spawn slot {i} changed in {area_name}"
        with open(path + '.tmp', 'wb') as f:
            f.write(out)
        os.replace(path + '.tmp', path)
        print(f"  Wrote {area_name}.dat ({len(out)} bytes; {len(changed)} slot(s) changed).")

    return 0

if __name__ == '__main__':
    sys.exit(main())
