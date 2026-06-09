"""Polish the Fireball spell description. It's the flagship first spell (id 0) but
kept the bland stock string "Fires a ball of fire at the target" while its siblings
(Frost Bolt "Hurl a bolt of ice that chills and wounds a foe.", etc.) got evocative
rewrites. Bring it up to the same voice. Description-only; surgical round-trip-asserted.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SP = os.path.join(DATA, 'Server Data', 'Spells.dat')

OLD = 'Fires a ball of fire at the target'
NEW = 'Hurl a roaring ball of flame that scorches a single foe.'

def main():
    spells = rcdata.read_spells(open(SP, 'rb').read())
    base = {s['id']: dict(s) for s in spells}
    target = None
    for s in spells:
        if s['name'] == 'Fireball':
            if s['description'] == NEW:
                print("  skip: Fireball description already polished"); return 0
            s['description'] = NEW
            target = s['id']
    if target is None:
        print("  ERROR: Fireball spell not found"); return 1

    out = rcdata.write_spells(spells)
    chk = {s['id']: s for s in rcdata.read_spells(out)}
    assert chk[target]['description'] == NEW
    for sid, b in base.items():
        if sid == target:
            for k in b:
                if k == 'description':
                    continue
                assert chk[sid][k] == b[k], f"Fireball field {k} changed"
        else:
            assert chk[sid] == b, f"untouched spell id{sid} changed"

    with open(SP + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(SP + '.tmp', SP)
    print(f"  Spells.dat: Fireball (id{target}) description -> {NEW!r}")
    return 0

if __name__ == '__main__':
    sys.exit(main())
