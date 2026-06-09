"""Give Northern Shrine a purpose: repoint spawn slot 0's actor_script from the
leftover 'Click_Test' demo NPC to 'Click_ShrineKeeper' (a right-click rest/restore
station). Surgical — only that one field changes; every other section and spawn
slot is asserted byte-identical before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREA = os.path.join(DATA, 'Server Data', 'Areas', 'Northern Shrine.dat')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

OLD, NEW = 'Click_Test', 'Click_ShrineKeeper'

def main():
    if not os.path.exists(os.path.join(SCRIPTS, NEW + '.rsl')):
        print(f"ERROR: {NEW}.rsl missing"); return 1
    raw = open(AREA, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = [dict(s) for s in area['spawns']]

    target = None
    for i, s in enumerate(area['spawns']):
        if s['actor_script'] == NEW:
            print(f"  skip: {NEW} already wired at slot {i}"); return 0
        if s['actor_script'] == OLD and target is None:
            target = i
    if target is None:
        print(f"  ERROR: no spawn with actor_script '{OLD}' found"); return 1

    area['spawns'][target]['actor_script'] = NEW
    print(f"  Northern Shrine: slot {target} actor_script {OLD!r} -> {NEW!r}")

    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"non-spawn section '{k}' changed"
    for i in range(len(orig)):
        if i == target:
            assert chk['spawns'][i]['actor_script'] == NEW
            # everything else in the slot must be unchanged
            for f in orig[i]:
                if f == 'actor_script':
                    continue
                assert chk['spawns'][i][f] == orig[i][f], f"slot {i} field {f} changed"
        else:
            assert chk['spawns'][i] == orig[i], f"untouched slot {i} changed"

    with open(AREA + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(AREA + '.tmp', AREA)
    print("  Wrote Northern Shrine.dat (1 field).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
