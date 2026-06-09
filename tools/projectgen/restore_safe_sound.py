"""Restore audio conservatively after confirming OGG plays fine but sustained sound
crashes (likely fire-and-forget EmitSound exhaustion from per-step footsteps/combat).

Keep the SAFE/managed sounds, drop the high-churn ones:
  - remove the temporary WAV test zone from Plains (no longer needed)
  - restore the managed ambient SoundZones (UpdateSoundZones tracks + stops channels,
    fixed small count -> no leak)
  - LEAVE actor Speech arrays cleared (no per-step footstep/combat EmitSound churn)
Magic cast sounds (PlaySound in the spell scripts) are 2D + occasional and were never
disabled. If this is stable, actor combat/footstep sounds can be re-added carefully later.
"""
import os, sys, glob
import rcdata

DATA = os.path.normpath(os.path.join(os.path.dirname(__file__), '..', '..', 'data'))
SOUNDS_DAT = os.path.join(DATA, 'Game Data', 'Sounds.dat')

def sid_of(name):
    db = rcdata.MediaDB(open(SOUNDS_DAT, 'rb').read(), rcdata.SOUND)
    return db.find_by_name(name)

# managed ambient zones to restore (sound id resolved by name)
PLAN = {
    'Test Zone':       [dict(name='Forest\\forestday.ogg',  x=-16.0, y=-74.0, z=106.0, radius=260.0, vol=60)],
    'Northern Shrine': [dict(name='Water\\Riverplane.ogg',  x=9.0,  y=5.0,  z=11.0,  radius=130.0, vol=55)],
    'Plains':          [dict(name='Weather\\Wind.ogg',      x=-7.0, y=5.0,  z=-12.0, radius=320.0, vol=40)],
}

def main():
    sounds = rcdata.MediaDB(open(SOUNDS_DAT, 'rb').read(), rcdata.SOUND).entries()
    wav_id = sid_of('Ambient\\wav_test.wav')

    for area_name, zones in PLAN.items():
        p = os.path.join(DATA, 'Areas', area_name + '.dat')
        raw = open(p, 'rb').read()
        area = rcdata.read_client_area(raw)
        # drop the WAV test zone if present
        before = len(area['sound_zones'])
        area['sound_zones'] = [z for z in area['sound_zones'] if z['sound'] != wav_id]
        for z in zones:
            sid = sid_of(z['name'])
            if sid is None:
                print(f"  ERROR: sound {z['name']} not registered"); return 1
            if any(sz['sound'] == sid for sz in area['sound_zones']):
                continue
            area['sound_zones'].append(dict(x=z['x'], y=z['y'], z=z['z'], radius=z['radius'],
                                            sound=sid, music=-1, repeat_time=0, volume=z['vol']))
        out = rcdata.write_client_area(area)
        chk = rcdata.read_client_area(out)
        for k in area:
            if k == 'sound_zones':
                continue
            assert chk[k] == area[k], f"section '{k}' changed in {area_name}"
        open(p, 'wb').write(out)
        print(f"  {area_name}: sound zones {before} -> {len(area['sound_zones'])} "
              f"({[ (sz['sound']) for sz in area['sound_zones']]})")
    print("Restored managed ambient zones; actor footstep/combat sounds remain OFF.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
