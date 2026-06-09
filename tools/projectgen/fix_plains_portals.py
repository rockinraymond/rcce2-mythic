"""Fix: the Plains travel portals sit on scenery (rocks) the player can't reach
(e.g. 'to testing' was at Y=13.4, above every waypoint). Reposition each DEPARTURE
portal onto walkable ground.

Ground truth available: Plains has no heightmap terrain to sample, so the only
known-walkable references are the spawn point and the waypoint network (the AI
navigates between waypoints, so the segment between two waypoints is walkable).
Place each travel portal at the MIDPOINT of two waypoints — on a walk path, at
ground height, and clear of the stationary NPC spawn points (wp5/7/8) so it won't
teleport the player while they approach an NPC.

Leaves 'Begin' (spawn) and 'return' (arrival point) untouched. Surgical: only the
named portals' x/y/z/size change; everything else stays byte-identical.

NOTE: best estimate without a 3D view — verify in-game and adjust coords if needed.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
PLAINS = os.path.join(DATA, 'Server Data', 'Areas', 'Plains.dat')

# portal name -> (x, y, z, size). Midpoints of walkable waypoint pairs:
#   Test:        mid(wp8 -41.1,5.6,16.9 ; wp6 -33.7,7.4,-51.5)
#   to testing:  mid(wp5  6.4,9.2,-15.6 ; wp4 29.1,9.0,-47.0)
#   to timelase: mid(wp7 91.7,7.0,21.8 ; wp3 78.2,3.3,-70.8)
NEW_POS = {
    'Test':        (-37.4, 6.5, -17.3, 4.0),
    'to testing':  (17.8, 9.1, -31.3, 4.0),
    'to timelase': (84.9, 5.2, -24.5, 4.0),
}

def main():
    raw = open(PLAINS, 'rb').read()
    area = rcdata.read_server_area(raw)
    orig = rcdata.read_server_area(raw)
    changed = []
    for i, p in enumerate(area['portals']):
        if p['name'] in NEW_POS:
            x, y, z, sz = NEW_POS[p['name']]
            old = (round(p['x'], 1), round(p['y'], 1), round(p['z'], 1), round(p['size'], 1))
            p['x'], p['y'], p['z'], p['size'] = x, y, z, sz
            changed.append((i, p['name'], old, (x, y, z, sz)))
    if not changed:
        print("No matching portals found."); return 1

    out = rcdata.write_server_area(area)
    chk = rcdata.read_server_area(out)
    # surgical: only the changed portal entries may differ
    for k in area:
        if k == 'portals':
            continue
        assert chk[k] == area[k], f"section '{k}' changed"
    changed_idx = {i for i, *_ in changed}
    for i in range(len(orig['portals'])):
        if i in changed_idx:
            continue
        assert chk['portals'][i] == orig['portals'][i], f"untouched portal {i} changed"
    with open(PLAINS + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(PLAINS + '.tmp', PLAINS)
    for i, name, old, new in changed:
        print(f"  portal[{i}] {name!r}: {old} -> {new}")
    print(f"Wrote {PLAINS}.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
