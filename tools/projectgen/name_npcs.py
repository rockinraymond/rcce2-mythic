"""Give every named-role NPC a proper nameplate. NPCs default to their Race ("Human")
without a spawn-init SetName; the Spell Trainer was worse — its init was the leftover
Spawn_Test, naming it "Test Human". Repoints each spawn's init `script` slot to a small
name-init. Matches spawns by their actor_script (their role), so it's position-independent.

Surgical: only the matched spawns' `script` field changes; everything else asserted
byte-identical. Allowlists the inits (SetName is privileged).
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
AREAS = os.path.join(SD, 'Areas')
SCRIPTS = os.path.join(SD, 'Scripts')
PRIV = os.path.join(SD, 'Privileged Scripts.dat')

# area -> { actor_script (role) : init script to set }
PLAN = {
    'Plains': {
        'Click_Trainer':      'Init_SpellTrainer',   # was Spawn_Test ("Test Human")
        'Quest_OrcRaiders':   'Init_TownCaptain',
        'Click_Merchant':     'Init_Shopkeeper',
        'marriage':           'Init_Priest',
    },
    'Test Zone': {
        'Ratcatcher1':        'Init_RatCatcher',
    },
}

def allowlist(name):
    b = open(PRIV, 'rb').read()
    if any(l.strip() == name.encode('latin-1') for l in b.split(b'\n')):
        return
    eol = b'\r\n' if b'\r\n' in b else b'\n'
    if not b.endswith(eol):
        b += eol
    b += name.encode('latin-1') + eol
    open(PRIV, 'wb').write(b)
    print(f"  allowlist: + {name}")

def main():
    inits = {v for m in PLAN.values() for v in m.values()}
    for nm in inits:
        if not os.path.exists(os.path.join(SCRIPTS, nm + '.rsl')):
            print(f"ERROR: {nm}.rsl missing"); return 1

    for area_name, roles in PLAN.items():
        path = os.path.join(AREAS, area_name + '.dat')
        raw = open(path, 'rb').read()
        area = rcdata.read_server_area(raw)
        orig = [dict(s) for s in area['spawns']]
        changed = {}
        for i, s in enumerate(area['spawns']):
            init = roles.get(s['actor_script'])
            if init and s['script'] != init:
                old = s['script']
                s['script'] = init
                changed[i] = (s['actor_script'], old, init)
        if not changed:
            print(f"  {area_name}: nothing to change (already named)")
            continue
        out = rcdata.write_server_area(area)
        chk = rcdata.read_server_area(out)
        for k in area:
            if k == 'spawns':
                continue
            assert chk[k] == area[k], f"{area_name}: non-spawn section '{k}' changed"
        for i in range(len(orig)):
            if i in changed:
                for f in orig[i]:
                    if f == 'script':
                        continue
                    assert chk['spawns'][i][f] == orig[i][f], f"{area_name} slot {i} field {f} changed"
            else:
                assert chk['spawns'][i] == orig[i], f"{area_name} untouched slot {i} changed"
        with open(path + '.tmp', 'wb') as f:
            f.write(out)
        os.replace(path + '.tmp', path)
        for i, (role, old, init) in sorted(changed.items()):
            print(f"  {area_name}: slot {i} ({role}) init {old!r} -> {init!r}")

    for nm in sorted(inits):
        allowlist(nm)
    return 0

if __name__ == '__main__':
    sys.exit(main())
