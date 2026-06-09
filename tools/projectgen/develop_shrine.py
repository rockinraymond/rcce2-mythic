"""Develop the Northern Shrine into a peaceful lore hub:
  1. FIX: the Shrine Keeper spawn's init `script` is the leftover Spawn_Test, which
     names it "Test Human". Repoint -> Init_ShrineKeeper (names it "Shrine Keeper").
  2. ADD: a Shrine Oracle lore NPC. The zone has only one valid waypoint (wp 0, the
     Keeper), so add wp 1 at a small offset on the same Y-platform (~8 units away,
     known-walkable adjacent ground) and place the Oracle there with a name init +
     lore dialog.
  3. Allowlist the two name-init scripts (SetName is privileged); the Oracle's dialog
     script is non-privileged.

Surgical: asserts every other section/slot/waypoint byte-identical before writing.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
AREA = os.path.join(SD, 'Areas', 'Northern Shrine.dat')
SCRIPTS = os.path.join(SD, 'Scripts')
PRIV = os.path.join(SD, 'Privileged Scripts.dat')

KEEPER_INIT = 'Init_ShrineKeeper'
ORACLE_INIT = 'Init_ShrineOracle'
ORACLE_CLICK = 'Click_ShrineOracle'
OLD_KEEPER_INIT = 'Spawn_Test'
ORACLE_WP = 1
OFFSET = (6.0, 0.0, 6.0)

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

def empty_slot(spawns):
    for i, s in enumerate(spawns):
        if (s['max'] == 0 and s['actor'] == 0 and not s['actor_script']
                and not s['script'] and not s['death_script']):
            return i
    return -1

def main():
    for nm in (KEEPER_INIT, ORACLE_INIT, ORACLE_CLICK):
        if not os.path.exists(os.path.join(SCRIPTS, nm + '.rsl')):
            print(f"ERROR: {nm}.rsl missing"); return 1

    raw = open(AREA, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig_spawns = [dict(s) for s in area['spawns']]
    orig_wps = [dict(w) for w in area['waypoints']]

    if any(s['actor_script'] == ORACLE_CLICK for s in area['spawns']):
        print("  skip: Shrine Oracle already placed")
        allowlist(KEEPER_INIT); allowlist(ORACLE_INIT)
        return 0

    # 1. fix keeper init
    keeper_slot = None
    for i, s in enumerate(area['spawns']):
        if s['actor_script'] == 'Click_ShrineKeeper':
            assert s['script'] == OLD_KEEPER_INIT, f"unexpected keeper init {s['script']!r}"
            s['script'] = KEEPER_INIT
            keeper_slot = i
    if keeper_slot is None:
        print("  ERROR: Shrine Keeper spawn not found"); return 1
    print(f"  Northern Shrine: slot {keeper_slot} keeper init {OLD_KEEPER_INIT!r} -> {KEEPER_INIT!r}")

    # 2. add wp 1 at offset from wp 0
    w0 = area['waypoints'][0]
    w1 = area['waypoints'][ORACLE_WP]
    assert w1['x'] == 0.0 and w1['y'] == 0.0 and w1['z'] == 0.0, "wp1 not empty, aborting"
    w1['x'] = w0['x'] + OFFSET[0]
    w1['y'] = w0['y'] + OFFSET[1]
    w1['z'] = w0['z'] + OFFSET[2]
    w1['next_a'] = w0['next_a']; w1['next_b'] = w0['next_b']
    w1['prev'] = w0['prev']; w1['pause'] = w0['pause']
    print(f"  Northern Shrine: wp {ORACLE_WP} <- ({w1['x']:.1f},{w1['y']:.1f},{w1['z']:.1f}) (offset from keeper)")

    # 3. place oracle
    slot = empty_slot(area['spawns'])
    if slot < 0:
        print("  ERROR: no empty spawn slot"); return 1
    s = area['spawns'][slot]
    s['actor'] = 0; s['waypoint'] = ORACLE_WP; s['max'] = 1
    s['frequency'] = 10; s['range'] = 0.0; s['size'] = 5.0
    s['script'] = ORACLE_INIT; s['actor_script'] = ORACLE_CLICK
    print(f"  Northern Shrine: slot {slot} <- Shrine Oracle @ wp {ORACLE_WP} (init {ORACLE_INIT}, click {ORACLE_CLICK})")

    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k in ('spawns', 'waypoints'):
            continue
        assert chk[k] == area[k], f"non-spawn section '{k}' changed"
    # waypoints: only wp ORACLE_WP changed
    for i in range(len(orig_wps)):
        if i == ORACLE_WP:
            continue
        assert chk['waypoints'][i] == orig_wps[i], f"untouched waypoint {i} changed"
    # spawns: only keeper_slot (script) and oracle slot changed
    for i in range(len(orig_spawns)):
        if i == keeper_slot:
            for f in orig_spawns[i]:
                if f == 'script':
                    continue
                assert chk['spawns'][i][f] == orig_spawns[i][f], f"keeper slot field {f} changed"
        elif i == slot:
            continue
        else:
            assert chk['spawns'][i] == orig_spawns[i], f"untouched slot {i} changed"

    with open(AREA + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(AREA + '.tmp', AREA)
    print("  Wrote Northern Shrine.dat.")
    allowlist(KEEPER_INIT)
    allowlist(ORACLE_INIT)
    return 0

if __name__ == '__main__':
    sys.exit(main())
