"""Add an Orc Warlord mini-boss to the wilds — the leader of the raiders the Wounded
Scouts warn about. CLONES the working Orc/Raider actor record (inheriting its proven
Troll mesh, anim set, Wildkin faction, and aggression) so the only differences are the
boss-defining fields: 300 HP (cap raised), Strength 100, bigger (scale 1.2), more XP.

Then places ONE spawn in Test Zone at the unused wp 1 (deep in the zone, away from the
entry rats/orcs), with a spawn-init that names it (Init_OrcWarlord) and a boss drop table
(BossLoot). Allowlists both scripts (SetName / ChangeGold / GiveItem are privileged).

Surgical: actor append asserts the existing actors are an untouched prefix; spawn edit
asserts every other section/slot byte-identical.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
ACT = os.path.join(SD, 'Actors.dat')
AREA = os.path.join(SD, 'Areas', 'Test Zone.dat')
SCRIPTS = os.path.join(SD, 'Scripts')
PRIV = os.path.join(SD, 'Privileged Scripts.dat')

INIT, LOOT = 'Init_OrcWarlord', 'BossLoot'
BOSS_WP = 1

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

def add_actor():
    raw = open(ACT, 'rb').read()
    actors = rcdata.read_actors(raw)
    if any(a['cls'].upper() == 'WARLORD' for a in actors):
        print("  skip: Orc Warlord actor already present")
        return next(a['id'] for a in actors if a['cls'].upper() == 'WARLORD')
    raider = next(a for a in actors if a['race'].upper() in ('ORC', 'ORK') and a['cls'].upper() == 'RAIDER')
    new_id = max(a['id'] for a in actors) + 1
    boss = {k: (list(v) if isinstance(v, list) else v) for k, v in raider.items()}
    boss['id'] = new_id
    boss['cls'] = 'Warlord'
    boss['attr_value'] = list(raider['attr_value'])
    boss['attr_max'] = list(raider['attr_max'])
    boss['attr_value'][0] = 300; boss['attr_max'][0] = 300      # Health
    boss['attr_value'][2] = 100; boss['attr_max'][2] = 100      # Strength (at cap)
    boss['xp_multiplier'] = 25
    boss['scale'] = 1.2
    actors.append(boss)
    out = rcdata.write_actors(actors)
    chk = rcdata.read_actors(out)
    # existing actors unchanged (prefix), boss appended with the boss fields
    base = {a['id']: a for a in actors[:-1]}
    cm = {a['id']: a for a in chk}
    for aid, a in base.items():
        assert cm[aid] == a, f"existing actor {aid} changed"
    b = cm[new_id]
    assert b['cls'] == 'Warlord' and b['attr_max'][0] == 300 and b['attr_value'][2] == 100
    assert b['mesh_ids'] == raider['mesh_ids'] and b['default_faction'] == raider['default_faction']
    assert b['aggressiveness'] == raider['aggressiveness']
    with open(ACT + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(ACT + '.tmp', ACT)
    print(f"  Actors.dat: + id{new_id} Orc/Warlord (clone of Raider; HP 300, Str 100, scale 1.2, xp x25)")
    return new_id

def empty_slot(spawns):
    for i, s in enumerate(spawns):
        if (s['max'] == 0 and s['actor'] == 0 and not s['actor_script']
                and not s['script'] and not s['death_script']):
            return i
    return -1

def wire_spawn(boss_id):
    raw = open(AREA, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = [dict(s) for s in area['spawns']]
    if any(s['actor'] == boss_id and s['max'] > 0 for s in area['spawns']):
        print("  skip: boss spawn already placed"); return
    slot = empty_slot(area['spawns'])
    if slot < 0:
        print("  ERROR: no empty spawn slot in Test Zone"); sys.exit(1)
    s = area['spawns'][slot]
    s['actor'] = boss_id; s['waypoint'] = BOSS_WP; s['max'] = 1
    s['frequency'] = 60; s['range'] = 0.0; s['size'] = 5.0
    s['script'] = INIT; s['death_script'] = LOOT
    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    for k in area:
        if k == 'spawns':
            continue
        assert chk[k] == area[k], f"non-spawn section '{k}' changed"
    for i in range(len(orig)):
        if i == slot:
            continue
        assert chk['spawns'][i] == orig[i], f"untouched slot {i} changed"
    with open(AREA + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(AREA + '.tmp', AREA)
    print(f"  Test Zone: slot {slot} <- Orc Warlord (actor {boss_id}) @ wp {BOSS_WP}, init {INIT}, loot {LOOT}")

def main():
    for nm in (INIT, LOOT):
        if not os.path.exists(os.path.join(SCRIPTS, nm + '.rsl')):
            print(f"ERROR: {nm}.rsl missing"); return 1
    boss_id = add_actor()
    wire_spawn(boss_id)
    allowlist(INIT)
    allowlist(LOOT)
    return 0

if __name__ == '__main__':
    sys.exit(main())
