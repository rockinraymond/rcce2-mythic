"""Bisect step: re-enable the Rat spawn (actor 3) in Test Zone, leave the Orc
(actor 4) disabled. b3d inspection shows rat.b3d is structurally normal (Head joint,
41 bones, valid anim) while Orc.b3d is anomalous (165 bones) — the likely crash. If
Test Zone is stable with only the rat live, the Orc mesh is confirmed as the culprit.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
TZ = os.path.join(DATA, 'Server Data', 'Areas', 'Test Zone.dat')
RAT_MAX = 3  # original rat spawn count

def main():
    raw = open(TZ, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = rcdata.read_server_area(raw)
    changed = []
    for i, s in enumerate(area['spawns']):
        if s['actor'] == 3 and s['max'] == 0:  # Rat/Critter, currently disabled
            s['max'] = RAT_MAX
            changed.append(i)
            print(f"  slot {i}: Rat spawn re-enabled (max {RAT_MAX})")
        elif s['actor'] == 4:
            print(f"  slot {i}: Orc spawn left at max {s['max']} (disabled)")
    if not changed:
        print("Rat already enabled / not found."); return 0
    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k]
    for i in range(len(orig['spawns'])):
        if i in changed:
            continue
        assert chk['spawns'][i] == orig['spawns'][i]
    with open(TZ + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(TZ + '.tmp', TZ)
    print("Wrote Test Zone.dat — rat live, orc disabled.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
