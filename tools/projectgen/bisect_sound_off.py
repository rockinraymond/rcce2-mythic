"""BISECT: silence the project to test whether SOUND playback is the crash cause.
Sounds.dat was empty before this project's work, so the client never played a sound
here until now; the user reports a hard crash with actors 'making sounds'. This:
  - clears every actor's MSpeechIDs/FSpeechIDs (no footsteps / combat / death sounds)
  - removes every SoundZone from the visual areas (no ambient forest/wind/water/music)
Weather and everything else are left intact (separate variable). Fully reversible:
re-run set_actor_sounds.py + add_soundzones.py to restore.
"""
import os, sys, glob
import rcdata

DATA = os.path.normpath(os.path.join(os.path.dirname(__file__), '..', '..', 'data'))

def main():
    # 1. clear actor speech arrays
    ap = os.path.join(DATA, 'Server Data', 'Actors.dat')
    actors = rcdata.read_actors(open(ap, 'rb').read())
    cleared = 0
    for a in actors:
        had = any(v != -1 for v in a['m_speech_ids'] + a['f_speech_ids'])
        a['m_speech_ids'] = [-1]*16
        a['f_speech_ids'] = [-1]*16
        if had:
            cleared += 1
    open(ap, 'wb').write(rcdata.write_actors(actors))
    print(f"  cleared speech on {cleared} actor templates")

    # 2. strip sound zones from every visual area
    for p in sorted(glob.glob(os.path.join(DATA, 'Areas', '*.dat'))):
        if os.path.basename(p).lower() == 'ha.dat':
            continue
        raw = open(p, 'rb').read()
        try:
            area = rcdata.read_client_area(raw)
        except Exception:
            continue
        n = len(area['sound_zones'])
        if n == 0:
            continue
        area['sound_zones'] = []
        out = rcdata.write_client_area(area)
        # surgical: every non-soundzone section identical
        chk = rcdata.read_client_area(out)
        for k in area:
            if k == 'sound_zones':
                continue
            assert chk[k] == area[k], f"section '{k}' changed in {os.path.basename(p)}"
        open(p, 'wb').write(out)
        print(f"  {os.path.basename(p)}: removed {n} sound zone(s)")
    return 0

if __name__ == '__main__':
    sys.exit(main())
