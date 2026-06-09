"""Round-trip validation: read each real project file, re-encode, and assert
the bytes are identical. This is the proof the codec faithfully models the
on-disk format before we use it to generate content."""
import os, sys
import rcdata

DATA = os.path.join(os.path.dirname(__file__), '..', '..', 'data')

def load(path):
    with open(path, 'rb') as f:
        return f.read()

def check(label, raw, decoded, reencode):
    out = reencode(decoded)
    ok = (out == raw)
    status = 'PASS' if ok else 'FAIL'
    print(f"[{status}] {label}: {len(raw)} bytes in, {len(out)} bytes out")
    if not ok:
        # find first divergence
        n = min(len(raw), len(out))
        for i in range(n):
            if raw[i] != out[i]:
                print(f"        first diff at byte {i}: {raw[i]:#04x} != {out[i]:#04x}")
                lo = max(0, i-8)
                print(f"        orig {raw[lo:i+8].hex()}")
                print(f"        ours {out[lo:i+8].hex()}")
                break
        if len(raw) != len(out):
            print(f"        length mismatch: {len(raw)} vs {len(out)}")
    return ok

def main():
    results = []

    p = os.path.join(DATA, 'Server Data', 'Spells.dat')
    raw = load(p)
    sp = rcdata.read_spells(raw)
    print(f"  spells: {[s['name'] for s in sp]}")
    results.append(check('Spells.dat', raw, sp, rcdata.write_spells))

    p = os.path.join(DATA, 'Server Data', 'Items.dat')
    raw = load(p)
    it = rcdata.read_items(raw)
    print(f"  items: {[(i['id'], i['name'], i['item_type']) for i in it]}")
    results.append(check('Items.dat', raw, it, rcdata.write_items))

    p = os.path.join(DATA, 'Server Data', 'Actors.dat')
    raw = load(p)
    ac = rcdata.read_actors(raw)
    print(f"  actors: {[(a['id'], a['race'], a['cls']) for a in ac]}")
    results.append(check('Actors.dat', raw, ac, rcdata.write_actors))

    for label, kind in [('Textures.dat', rcdata.TEXTURE),
                        ('Meshes.dat', rcdata.MESH),
                        ('Sounds.dat', rcdata.SOUND)]:
        p = os.path.join(DATA, 'Game Data', label)
        raw = load(p)
        db = rcdata.MediaDB(raw, kind)
        ents = db.entries()
        mx = max(ents) if ents else -1
        print(f"  {label}: {len(ents)} registered (max id {mx})")
        results.append(check(label, raw, db, lambda d: d.save()))

    import glob
    for p in sorted(glob.glob(os.path.join(DATA, 'Server Data', 'Areas', '*.dat'))):
        raw = load(p)
        area = rcdata.read_server_area(raw)
        results.append(check('Area:' + os.path.basename(p), raw, area,
                             rcdata.write_server_area))

    for p in sorted(glob.glob(os.path.join(DATA, 'Areas', '*.dat'))):
        if os.path.basename(p).lower() == 'ha.dat':
            continue  # legacy pre-shadow-fields test stub; not current format
        raw = load(p)
        try:
            area = rcdata.read_client_area(raw)
        except Exception as e:
            print(f"[FAIL] Visual:{os.path.basename(p)}: {e!r}")
            results.append(False); continue
        results.append(check('Visual:' + os.path.basename(p), raw, area,
                             rcdata.write_client_area))

    print()
    if all(results):
        print("ALL ROUND-TRIPS PASSED — codec is byte-faithful.")
        return 0
    print("SOME ROUND-TRIPS FAILED — do NOT generate content until fixed.")
    return 1

if __name__ == '__main__':
    sys.exit(main())
