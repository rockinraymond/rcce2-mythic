"""Iteration 10 content: give the combat zone ambient atmosphere.

Test Zone's CLIENT visual area (Data/Areas/Test Zone.dat) has no sound zones, so
it's silent. Add a large forest-ambient SoundZone covering the play area, using
the forestday sound registered in iter 2 (Sounds.dat id 2 = Forest\\forestday.ogg).

SoundZone semantics (Client.bb UpdateSoundZones/PlaySoundZone): plays when the
player is within Radius; RepeatTime=0 re-triggers on stop (continuous loop);
volume scaled by Volume/100; non-3D sounds play globally. Idempotent & surgical.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AREAS = os.path.join(DATA, 'Areas')          # client visual areas
SOUNDS_DAT = os.path.join(DATA, 'Game Data', 'Sounds.dat')

# area filename (no .dat) -> list of sound zones to ensure present
# Centered over the Test Zone waypoint span (x[-87,55], z[35,177], y~-74).
PLAN = {
    'Test Zone': [
        dict(x=-16.0, y=-74.0, z=106.0, radius=260.0, sound=2, music=-1,
             repeat_time=0, volume=60),  # forest day ambient, zone-wide
    ],
    # Northern Shrine has waterfall emitters + water; flowing-water ambient suits it.
    # Scenery spans x[-23,41] z[-16,40]; center the zone over the shrine.
    'Northern Shrine': [
        dict(x=9.0, y=5.0, z=11.0, radius=130.0, sound=5, music=-1,
             repeat_time=0, volume=55),  # Water\Riverplane.ogg, shrine-wide
    ],
    # Plains is a wide open grassland (x[-147,132] z[-131,121]); its only existing
    # zones are tiny localized spots. A gentle zone-wide wind suits open plains.
    'Plains': [
        dict(x=-7.0, y=5.0, z=-12.0, radius=320.0, sound=11, music=-1,
             repeat_time=0, volume=40),  # Weather\Wind.ogg, plains-wide
    ],
}

def main():
    sounds = rcdata.MediaDB(open(SOUNDS_DAT, 'rb').read(), rcdata.SOUND).entries()

    for area_name, zones in PLAN.items():
        path = os.path.join(AREAS, area_name + '.dat')
        raw = open(path, 'rb').read()
        area = rcdata.read_client_area(raw)
        before = len(area['sound_zones'])
        added = 0
        for z in zones:
            if z['sound'] != -1 and z['sound'] not in sounds:
                print(f"  ERROR: sound id {z['sound']} not registered"); return 1
            # idempotent: same sound id already present?
            if any(sz['sound'] == z['sound'] for sz in area['sound_zones']):
                print(f"  skip ({area_name}): sound {z['sound']} zone already present"); continue
            area['sound_zones'].append(dict(z))
            added += 1
            print(f"  {area_name}: + sound zone sound={z['sound']} "
                  f"({sounds.get(z['sound'],{}).get('name','?')}) radius={z['radius']} vol={z['volume']}")
        if added == 0:
            continue

        out = rcdata.write_client_area(area)
        chk = rcdata.read_client_area(out)
        # surgical: every section except sound_zones must be byte-identical
        for k in area:
            if k == 'sound_zones':
                continue
            assert chk[k] == area[k], f"section '{k}' changed in {area_name}"
        assert chk['sound_zones'][:before] == area['sound_zones'][:before], "existing zones changed"
        assert chk['sound_zones'] == area['sound_zones'], "sound zones re-parse mismatch"
        with open(path + '.tmp', 'wb') as f:
            f.write(out)
        os.replace(path + '.tmp', path)
        print(f"  Wrote {area_name}.dat ({len(out)} bytes, was {len(raw)}; +{added} sound zone).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
