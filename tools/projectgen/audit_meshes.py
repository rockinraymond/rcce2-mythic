"""Preventive audit: for every actor template, resolve its mesh and report the
skinned-bone count. Flags meshes in the danger zone that can overflow the runtime
skinner and HARD-CRASH the Blitz client (this is exactly what Orc.b3d's 165-bone
rig did — see PLAN.md 'User bug report #2'). Run before shipping any new actor.

Heuristic threshold: stag=28, rat=41, troll=19 render fine; the crashing Orc was
165. Flag > 80 as risky (verify in-client before spawning).
"""
import os, sys
import rcdata, b3dinspect

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
MESH_DIR = os.path.join(DATA, 'Meshes')
RISKY_BONES = 80

def bone_count(b3d_path):
    """Parse just the bone count via b3dinspect's chunk walker."""
    import struct
    data = open(b3d_path, 'rb').read()
    if data[:4] != b'BB3D':
        return None
    size = struct.unpack_from('<i', data, 4)[0]
    bones = [0]
    def walk(off, end):
        while off + 8 <= end:
            tag = data[off:off+4]; csize = struct.unpack_from('<i', data, off+4)[0]
            body = off + 8; bend = body + csize
            if tag == b'NODE':
                p = body
                while data[p] != 0: p += 1
                walk(p + 1 + 10*4, bend)
            elif tag == b'BONE':
                bones[0] += 1
            elif tag == b'MESH':
                walk(body + 4, bend)
            off = bend
    walk(12, 12 + size)
    return bones[0]

def main():
    actors = rcdata.read_actors(open(os.path.join(DATA, 'Server Data', 'Actors.dat'), 'rb').read())
    meshes = rcdata.MediaDB(open(os.path.join(DATA, 'Game Data', 'Meshes.dat'), 'rb').read(), rcdata.MESH).entries()
    risky = 0
    print(f"{'actor':22} {'mesh':40} bones")
    for a in actors:
        mid = a['mesh_ids'][0]
        rec = meshes.get(mid)
        if not rec:
            print(f"  {a['race']+'/'+a['cls']:20} mesh id {mid}: NOT REGISTERED  <-- !!")
            risky += 1; continue
        path = os.path.join(MESH_DIR, rec['name'].replace('\\', os.sep))
        if not os.path.exists(path):
            print(f"  {a['race']+'/'+a['cls']:20} {rec['name']:40} FILE MISSING  <-- !!")
            risky += 1; continue
        bc = bone_count(path)
        flag = '  <-- RISKY (verify in client!)' if (bc or 0) > RISKY_BONES else ''
        if flag: risky += 1
        print(f"  {a['race']+'/'+a['cls']:20} {rec['name']:40} {bc}{flag}")
    print()
    if risky:
        print(f"{risky} actor mesh(es) need attention (>{RISKY_BONES} bones or missing).")
        return 1
    print(f"All actor meshes OK (<= {RISKY_BONES} bones, files present).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
