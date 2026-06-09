"""Give the Orc Raider spawns a richer death script (OrcLoot) than the rats'
shared MonsterLoot. Repoints the death_script of every spawn whose actor is the
Orc/Raider template; rats keep MonsterLoot.

Surgical: only touches death_script on Orc/Raider spawns, asserts every other
section/slot byte-identical before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREA = os.path.join(DATA, 'Server Data', 'Areas', 'Test Zone.dat')
ACTORS = os.path.join(DATA, 'Server Data', 'Actors.dat')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

OLD, NEW = 'MonsterLoot', 'OrcLoot'

def main():
    if not os.path.exists(os.path.join(SCRIPTS, NEW + '.rsl')):
        print(f"ERROR: {NEW}.rsl missing"); return 1

    actors = rcdata.read_actors(open(ACTORS, 'rb').read())
    orc_ids = {a['id'] for a in actors if a['race'].upper() in ('ORC', 'ORK')
               and a['cls'].upper() == 'RAIDER'}
    if not orc_ids:
        print("ERROR: no Orc/Raider actor template found"); return 1

    raw = open(AREA, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = [dict(s) for s in area['spawns']]

    if any(s['death_script'] == NEW for s in area['spawns']):
        print("  skip: OrcLoot already wired"); return 0

    changed = []
    for i, s in enumerate(area['spawns']):
        if s['actor'] in orc_ids and s['max'] > 0 and s['death_script'] == OLD:
            s['death_script'] = NEW
            changed.append(i)
            print(f"  Test Zone: slot {i} (orc actor {s['actor']}) death_script {OLD!r} -> {NEW!r}")
    if not changed:
        print("  ERROR: no Orc/Raider spawn with death_script MonsterLoot found"); return 1

    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"non-spawn section '{k}' changed"
    ci = set(changed)
    for i in range(len(orig)):
        if i in ci:
            for f in orig[i]:
                if f == 'death_script':
                    continue
                assert chk['spawns'][i][f] == orig[i][f], f"slot {i} field {f} changed"
            assert chk['spawns'][i]['death_script'] == NEW
        else:
            assert chk['spawns'][i] == orig[i], f"untouched slot {i} changed"

    with open(AREA + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(AREA + '.tmp', AREA)
    print(f"  Wrote Test Zone.dat ({len(changed)} spawn(s)).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
