"""Test the WAV-vs-OGG hypothesis: the OpenAL backend has no OGG decoder DLL, and
the project crashed playing OGG sounds. Generate a simple PCM WAV (which Blitz3D
reliably plays), register it, and wire it as a single Plains ambient sound zone.
If login plays it without crashing => WAV works, OGG was the crash => we move all
sounds to WAV. If it still crashes => sound is broken in this client regardless.

Generates a gentle 2s loopable low 'ambience' (quiet) so it isn't annoying.
"""
import os, sys, wave, struct, math
import rcdata

DATA = os.path.normpath(os.path.join(os.path.dirname(__file__), '..', '..', 'data'))
WAV_REL = 'Ambient\\wav_test.wav'                 # under data/Sounds
WAV_DST = os.path.join(DATA, 'Sounds', 'Ambient', 'wav_test.wav')

def make_wav(path):
    rate = 22050
    dur = 2.0
    n = int(rate * dur)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    w = wave.open(path, 'wb')
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(rate)
    frames = bytearray()
    for i in range(n):
        t = i / rate
        # quiet low blended tone (110Hz + 165Hz), gentle amplitude
        v = 0.18 * (math.sin(2*math.pi*110*t) + 0.6*math.sin(2*math.pi*165*t))
        # fade in/out a touch so the loop seam is soft
        env = min(1.0, t/0.1, (dur-t)/0.1)
        s = int(max(-1.0, min(1.0, v*env)) * 22000)
        frames += struct.pack('<h', s)
    w.writeframes(bytes(frames)); w.close()

def main():
    make_wav(WAV_DST)
    print(f"wrote {os.path.relpath(WAV_DST, DATA)} ({os.path.getsize(WAV_DST)} bytes)")

    # register the WAV (is_3d=0 -> 2D ambient via PlaySound, simplest path)
    sp = os.path.join(DATA, 'Game Data', 'Sounds.dat')
    raw = open(sp, 'rb').read()
    db = rcdata.MediaDB(raw, rcdata.SOUND)
    sid = db.find_by_name(WAV_REL)
    if sid is None:
        sid = db.add_file(WAV_REL, is_3d=0)
        out = db.save()
        orig = rcdata.MediaDB(raw, rcdata.SOUND)
        for i, off in enumerate(orig.index):
            if off != 0: assert db.index[i] == off
        assert bytes(db.blob[:len(orig.blob)]) == bytes(orig.blob)
        open(sp, 'wb').write(out)
    print(f"registered WAV sound id {sid}")

    # add ONE Plains sound zone using it (auto-plays on login), centered on spawn
    pz = os.path.join(DATA, 'Areas', 'Plains.dat')
    raw = open(pz, 'rb').read()
    area = rcdata.read_client_area(raw)
    # remove any prior wav_test zone (idempotent), then add
    area['sound_zones'] = [z for z in area['sound_zones'] if z['sound'] != sid]
    area['sound_zones'].append(dict(x=-53.0, y=3.0, z=80.0, radius=400.0,
                                    sound=sid, music=-1, repeat_time=0, volume=70))
    out = rcdata.write_client_area(area)
    chk = rcdata.read_client_area(out)
    for k in area:
        if k == 'sound_zones':
            continue
        assert chk[k] == area[k], f"section '{k}' changed"
    open(pz, 'wb').write(out)
    print(f"added Plains ambient sound zone using WAV id {sid} (radius 400 covers spawn)")
    return 0

if __name__ == '__main__':
    sys.exit(main())
