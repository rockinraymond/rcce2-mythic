"""UNBLOCK: the Blitz client hard-crashes (empty log = access violation) when
rendering the Rat (mesh 161) / Orc (mesh 81) in Test Zone — untested shipped meshes
that never actually spawned in the original sample. By elimination (Plains works
with weather + ambient sound + footsteps + Human/Stag meshes), these two meshes are
the only Test-Zone-unique renderable content. Temporarily set their SpawnMax=0 so
the zone is safe to enter while the mesh issue is investigated. Surgical & reversible.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
TZ = os.path.join(DATA, 'Server Data', 'Areas', 'Test Zone.dat')
DISABLE_ACTORS = {3, 4}  # Rat/Critter, Orc/Raider

def main():
    raw = open(TZ, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = rcdata.read_server_area(raw)
    changed = []
    for i, s in enumerate(area['spawns']):
        if s['actor'] in DISABLE_ACTORS and s['max'] > 0:
            print(f"  slot {i}: actor {s['actor']} max {s['max']} -> 0 (disabled)")
            s['max'] = 0
            changed.append(i)
    if not changed:
        print("Nothing to disable."); return 0
    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"section '{k}' changed"
    for i in range(len(orig['spawns'])):
        if i in changed:
            continue
        assert chk['spawns'][i] == orig['spawns'][i], f"untouched slot {i} changed"
    with open(TZ + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(TZ + '.tmp', TZ)
    print(f"Wrote Test Zone.dat — Rat/Orc spawns disabled (reversible).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
