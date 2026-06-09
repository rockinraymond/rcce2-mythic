"""Turn the wp-2 Wounded Scout into the quest-giver for "The Raider-Chief" (avenge the
patrol by slaying Grukk, the Orc/Warlord boss). Repoints ONLY that one scout's
actor_script Click_WoundedScout -> Quest_RaiderChief; the wp-3 scout stays ambient
flavour. The spawn keeps its Init_WoundedScout name init. Allowlists the quest script.

Surgical: asserts every other section/slot byte-identical before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
AREA = os.path.join(SD, 'Areas', 'Test Zone.dat')
SCRIPTS = os.path.join(SD, 'Scripts')
PRIV = os.path.join(SD, 'Privileged Scripts.dat')

OLD, NEW = 'Click_WoundedScout', 'Quest_RaiderChief'
QUEST_WP = 2

def allowlist(name):
    b = open(PRIV, 'rb').read()
    if any(l.strip() == name.encode('latin-1') for l in b.split(b'\n')):
        print(f"  allowlist: {name} already present"); return
    eol = b'\r\n' if b'\r\n' in b else b'\n'
    if not b.endswith(eol):
        b += eol
    b += name.encode('latin-1') + eol
    open(PRIV, 'wb').write(b)
    print(f"  allowlist: + {name}")

def main():
    if not os.path.exists(os.path.join(SCRIPTS, NEW + '.rsl')):
        print(f"ERROR: {NEW}.rsl missing"); return 1

    raw = open(AREA, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = [dict(s) for s in area['spawns']]

    if any(s['actor_script'] == NEW for s in area['spawns']):
        print("  skip: Raider-Chief quest already wired")
        allowlist(NEW)
        return 0

    target = None
    for i, s in enumerate(area['spawns']):
        if s['actor_script'] == OLD and s['waypoint'] == QUEST_WP and s['max'] > 0:
            target = i; break
    if target is None:
        print(f"  ERROR: no {OLD} spawn at wp {QUEST_WP} found"); return 1

    area['spawns'][target]['actor_script'] = NEW
    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"non-spawn section '{k}' changed"
    for i in range(len(orig)):
        if i == target:
            for f in orig[i]:
                if f == 'actor_script':
                    continue
                assert chk['spawns'][i][f] == orig[i][f], f"slot {i} field {f} changed"
            assert chk['spawns'][i]['actor_script'] == NEW
        else:
            assert chk['spawns'][i] == orig[i], f"untouched slot {i} changed"

    with open(AREA + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(AREA + '.tmp', AREA)
    print(f"  Test Zone: slot {target} (wp {QUEST_WP} scout) actor_script {OLD!r} -> {NEW!r}")
    allowlist(NEW)
    return 0

if __name__ == '__main__':
    sys.exit(main())
