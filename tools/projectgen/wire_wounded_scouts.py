"""Turn the two silent placeholder Human NPCs in Test Zone (wp 2 and wp 3) into
Wounded Scout flavour NPCs: set each spawn's init `script` -> Init_WoundedScout
(names them) and `actor_script` -> Click_WoundedScout (right-click dialog).

Surgical: only matches the blank actor-0 placeholder spawns at wp 2/3 (max 1, all
three script slots empty), only touches script/actor_script, and asserts every
other section and untouched slot is byte-identical before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREA = os.path.join(DATA, 'Server Data', 'Areas', 'Test Zone.dat')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

INIT, CLICK = 'Init_WoundedScout', 'Click_WoundedScout'
TARGET_WPS = {2, 3}

def is_blank_placeholder(s):
    return (s['actor'] == 0 and s['max'] == 1 and s['waypoint'] in TARGET_WPS
            and not s['script'] and not s['actor_script'] and not s['death_script'])

def main():
    for nm in (INIT, CLICK):
        if not os.path.exists(os.path.join(SCRIPTS, nm + '.rsl')):
            print(f"ERROR: {nm}.rsl missing"); return 1

    raw = open(AREA, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = [dict(s) for s in area['spawns']]

    if any(s['actor_script'] == CLICK for s in area['spawns']):
        print("  skip: scouts already wired"); return 0

    changed = []
    for i, s in enumerate(area['spawns']):
        if is_blank_placeholder(s):
            s['script'] = INIT
            s['actor_script'] = CLICK
            changed.append((i, s['waypoint']))
    if not changed:
        print("  ERROR: no blank placeholder spawns at wp 2/3 found"); return 1
    for i, wp in changed:
        print(f"  Test Zone: slot {i} (wp {wp}) -> init {INIT}, click {CLICK}")

    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"non-spawn section '{k}' changed"
    ci = {i for i, _ in changed}
    for i in range(len(orig)):
        if i in ci:
            for f in orig[i]:
                if f in ('script', 'actor_script'):
                    continue
                assert chk['spawns'][i][f] == orig[i][f], f"slot {i} field {f} changed"
            assert chk['spawns'][i]['script'] == INIT
            assert chk['spawns'][i]['actor_script'] == CLICK
        else:
            assert chk['spawns'][i] == orig[i], f"untouched slot {i} changed"

    with open(AREA + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(AREA + '.tmp', AREA)
    print(f"  Wrote Test Zone.dat ({len(changed)} spawn(s)).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
