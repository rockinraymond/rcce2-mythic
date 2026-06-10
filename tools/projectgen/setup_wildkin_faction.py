"""Bring the dormant faction system to life and make the Orc Raiders actually
aggressive (the guide calls them aggressive; they were only Defensive).

Before: only factions 0 'Traders' (the player) and 1 'Voyageur' exist, all
cross-ratings 100, and EVERY actor is default_faction 0 — so mobs are the player's
own faction and an aggressive mob would treat the player as kin.

After:
  Factions.dat — add faction 2 'Wildkin' (the wild monsters). Ratings:
    Wildkin -> Traders = 0   (hostile: aggressive Wildkin will hunt the player)
    Traders -> Wildkin = 0   (player faction sees Wildkin as hostile)
    Wildkin -> Wildkin = 200 (allied: orcs don't fight rats or each other)
  Actors.dat —
    Orc Raider (Orc/Raider): default_faction 2, aggressiveness 2 (proactive hunt)
    Rat (Rat/Critter):       default_faction 2 (so orcs ignore them — protects the
                             Rat Catcher quest population), aggressiveness stays 1.

Town NPCs (Human, faction 0) and the Stag stay as-is. The wilds' Human NPCs (Rat
Catcher, Wounded Scouts) are faction 0 too, so an orc may aggro them — but they're
1000-HP Human/Fighter templates and win/survive, acceptable emergent behaviour.

Surgical + reversible: asserts the faction codec round-trips, only flips the named
fields, and re-reads to confirm.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
FAC = os.path.join(DATA, 'Server Data', 'Factions.dat')
ACT = os.path.join(DATA, 'Server Data', 'Actors.dat')

TRADERS, WILDKIN = 0, 2
WILDKIN_NAME = 'Wildkin'

def do_factions():
    raw = open(FAC, 'rb').read()
    assert rcdata.write_factions(rcdata.read_factions(raw)) == raw, "faction codec not byte-faithful"
    f = rcdata.read_factions(raw)
    if f['names'][WILDKIN] == WILDKIN_NAME:
        print("  skip: Wildkin faction already present"); return False
    if f['names'][WILDKIN]:
        print(f"  ERROR: faction slot {WILDKIN} already named {f['names'][WILDKIN]!r}"); sys.exit(1)
    f['names'][WILDKIN] = WILDKIN_NAME
    f['grid'][WILDKIN][TRADERS] = 0
    f['grid'][TRADERS][WILDKIN] = 0
    f['grid'][WILDKIN][WILDKIN] = 200
    out = rcdata.write_factions(f)
    chk = rcdata.read_factions(out)
    assert chk['names'][WILDKIN] == WILDKIN_NAME
    assert chk['grid'][WILDKIN][TRADERS] == 0
    assert chk['grid'][TRADERS][WILDKIN] == 0
    assert chk['grid'][WILDKIN][WILDKIN] == 200
    # nothing else changed
    base = rcdata.read_factions(raw)
    for i in range(100):
        for j in range(100):
            if (i, j) in ((WILDKIN, TRADERS), (TRADERS, WILDKIN), (WILDKIN, WILDKIN)):
                continue
            assert chk['grid'][i][j] == base['grid'][i][j], f"grid[{i}][{j}] changed"
    for i in range(100):
        if i == WILDKIN:
            continue
        assert chk['names'][i] == base['names'][i], f"name {i} changed"
    with open(FAC + '.tmp', 'wb') as fh:
        fh.write(out)
    os.replace(FAC + '.tmp', FAC)
    print(f"  Factions.dat: added faction {WILDKIN} {WILDKIN_NAME!r}; Wildkin<->Traders=0, Wildkin->Wildkin=200")
    return True

def do_actors():
    raw = open(ACT, 'rb').read()
    actors = rcdata.read_actors(raw)
    base = {a['id']: dict(a) for a in actors}
    changed_ids = set()
    changed = []
    for a in actors:
        race, cls = a['race'].upper(), a['cls'].upper()
        if race in ('ORC', 'ORK') and cls == 'RAIDER':
            a['default_faction'] = WILDKIN; a['aggressiveness'] = 2
            changed_ids.add(a['id'])
            changed.append(f"{a['race']}/{a['cls']} id{a['id']}: faction->{WILDKIN}, aggressiveness->2")
        elif race == 'RAT':
            a['default_faction'] = WILDKIN
            changed_ids.add(a['id'])
            changed.append(f"{a['race']}/{a['cls']} id{a['id']}: faction->{WILDKIN}")
    if not changed:
        print("  ERROR: no orc/rat actors found"); sys.exit(1)
    out = rcdata.write_actors(actors)
    chk = {a['id']: a for a in rcdata.read_actors(out)}
    # only the targeted actors changed; everything else byte-identical-by-field
    for aid, b in base.items():
        if aid in changed_ids:
            assert chk[aid]['default_faction'] == WILDKIN
        else:
            assert chk[aid] == b, f"untouched actor id{aid} changed"
    with open(ACT + '.tmp', 'wb') as fh:
        fh.write(out)
    os.replace(ACT + '.tmp', ACT)
    for c in changed:
        print("  Actors.dat:", c)

def main():
    do_factions()
    do_actors()
    return 0

if __name__ == '__main__':
    sys.exit(main())
