"""Place script-bearing NPC spawns (quest-givers, vendors, trainers) into an area.

Idempotent on actor_script (won't double-place). Surgical: asserts every non-spawn
section and every untouched spawn slot is byte-identical before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREAS = os.path.join(DATA, 'Server Data', 'Areas')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

# area -> list of NPC spawns (actor=0 Human/Fighter unless noted)
PLAN = {
    'Plains': [
        dict(actor=0, waypoint=5, actor_script='Quest_OrcRaiders', max=1,
             frequency=10, range=0.0, size=5.0),
        dict(actor=0, waypoint=7, actor_script='Click_Merchant', max=1,
             frequency=10, range=0.0, size=5.0),
    ],
}

def empty_slot(spawns):
    for i, s in enumerate(spawns):
        if s['max'] == 0 and s['actor'] == 0 and not s['actor_script'] \
           and not s['script'] and not s['death_script']:
            return i
    return -1

def main():
    actor_ids = {a['id'] for a in rcdata.read_actors(open(os.path.join(DATA, 'Server Data', 'Actors.dat'), 'rb').read())}

    for area_name, npcs in PLAN.items():
        path = os.path.join(AREAS, area_name + '.dat')
        raw = open(path, 'rb').read()
        area = rcdata.read_server_area(raw)
        orig = [dict(s) for s in area['spawns']]
        changed = []
        for npc in npcs:
            if npc['actor'] not in actor_ids:
                print(f"  ERROR: actor {npc['actor']} missing"); return 1
            if not os.path.exists(os.path.join(SCRIPTS, npc['actor_script'] + '.rsl')):
                print(f"  ERROR: script {npc['actor_script']}.rsl missing"); return 1
            if any(s['actor_script'] == npc['actor_script'] for s in area['spawns']):
                print(f"  skip ({area_name}): {npc['actor_script']} already placed"); continue
            slot = empty_slot(area['spawns'])
            if slot < 0:
                print(f"  ERROR: no empty slot in {area_name}"); return 1
            s = area['spawns'][slot]
            s['actor'] = npc['actor']; s['waypoint'] = npc['waypoint']
            s['actor_script'] = npc['actor_script']; s['max'] = npc['max']
            s['frequency'] = npc['frequency']; s['range'] = npc['range']; s['size'] = npc['size']
            changed.append(slot)
            print(f"  {area_name}: slot {slot} <- NPC actor {npc['actor']} "
                  f"script {npc['actor_script']} wp {npc['waypoint']}")
        if not changed:
            continue
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
