"""Iteration 15 content: give actors voices + footsteps.

Wire each creature's Speech sound array (Actors.dat MSpeechIDs/FSpeechIDs[16]) to
registered sounds. Speech index -> event (Actors.bb): 4=Attack1 6=Hit1 9=Death
10=FootstepDry 11=FootstepWet (65535/-1 = silent). The engine plays MSpeechIDs[event]
positionally on that actor when the event fires (Actors3D.bb:794).

Player footsteps reuse the footstep sounds registered in iter 2 (ids 0,1); creature
attack/hit/death use the HF vocalizations imported this iteration (ids 16-21).

Re-enabled in the showcase project once the engine channel-pool fix (#540) made
per-actor EmitSound safe; the PLAN was extended to cover the Orc/Warlord mini-boss
(Grukk) -- a clone of the Orc/Raider on the same Troll mesh -- so it reuses the
troll vocalizations (ids 19-21) rather than staying mute.

Surgical: only the speech arrays change; everything else stays byte-identical.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
ACTORS = os.path.join(DATA, 'Server Data', 'Actors.dat')
SOUNDS_DAT = os.path.join(DATA, 'Game Data', 'Sounds.dat')

# speech index constants
ATTACK1, HIT1, DEATH, FOOT_DRY, FOOT_WET = 4, 6, 9, 10, 11

# (race, cls) -> {speech_index: sound_id} applied to BOTH male & female arrays
PLAN = {
    ('Human', 'Fighter'): {FOOT_DRY: 0, FOOT_WET: 1},                 # Carefulstep2 / dampstep
    ('Rat', 'Critter'):   {ATTACK1: 16, HIT1: 17, DEATH: 18},        # rat squeaks
    ('Orc', 'Raider'):    {ATTACK1: 19, HIT1: 20, DEATH: 21,
                           FOOT_DRY: 1},                              # troll grunts + heavy step
    ('Orc', 'Warlord'):   {ATTACK1: 19, HIT1: 20, DEATH: 21,
                           FOOT_DRY: 1},                              # Grukk mini-boss: clone of
                                                                     # Raider (same Troll mesh), so
                                                                     # the same troll vocalizations
}

def main():
    sounds = rcdata.MediaDB(open(SOUNDS_DAT, 'rb').read(), rcdata.SOUND).entries()
    raw = open(ACTORS, 'rb').read()
    actors = rcdata.read_actors(raw)
    orig = rcdata.read_actors(raw)  # pristine copy for comparison
    changed = False

    for a in actors:
        key = (a['race'], a['cls'])
        if key not in PLAN:
            continue
        for idx, sid in PLAN[key].items():
            if sid not in sounds:
                print(f"  ERROR: sound id {sid} not registered"); return 1
            if a['m_speech_ids'][idx] != sid or a['f_speech_ids'][idx] != sid:
                a['m_speech_ids'][idx] = sid
                a['f_speech_ids'][idx] = sid
                changed = True
        print(f"  {key[0]}/{key[1]}: speech set "
              f"{ {i: a['m_speech_ids'][i] for i in PLAN[key]} }")

    if not changed:
        print("Nothing to change; actor sounds already set."); return 0

    out = rcdata.write_actors(actors)
    chk = rcdata.read_actors(out)
    # surgical: only m_speech_ids / f_speech_ids may differ, per actor
    assert len(chk) == len(orig)
    for new, old in zip(chk, orig):
        for k in new:
            if k in ('m_speech_ids', 'f_speech_ids'):
                continue
            assert new[k] == old[k], f"non-speech field '{k}' changed on {old['race']}/{old['cls']}"
    with open(ACTORS + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(ACTORS + '.tmp', ACTORS)
    print(f"Wrote {ACTORS} ({len(out)} bytes, was {len(raw)}).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
