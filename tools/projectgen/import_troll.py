"""Replace the crashing Orc.b3d (165 bones) with HF's Troll mesh (19 bones, vetted
crash-safe) as the Orc/Raider enemy — a proper monster, using HF assets.

Steps (all statically verified; no runtime check possible here):
 1. copy Troll.b3d + its Body.bmp texture into rcce2 data/
 2. register the mesh in Meshes.dat and the body texture in Textures.dat
 3. port HF's Troll anim set (id 43) into rcce2 Animations.dat as a new set,
    remapping HF's 'Attack 1/2/3' to rcce2's 'Default/Right hand/Two hand attack'
    (and 'Sitdown'->'Sit down') so the engine's animation triggers all resolve
 4. point the Orc/Raider actor at the new mesh / anim set / body texture
 5. re-enable the Orc/Raider spawn in Test Zone

Idempotent: re-running detects already-imported pieces and skips them.
"""
import os, sys, shutil
import rcdata, animsets

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
HF = 'C:/Users/dyanr/Desktop/HeroesFate/Game/Data'

MESH_SRC = os.path.join(HF, 'Meshes', 'Actors', 'Monsters', 'Troll.b3d')
TEX_SRC  = os.path.join(HF, 'Meshes', 'Actors', 'Animals', 'Body.bmp')  # Troll's body tex
MESH_DST_REL = 'Actors\\Monsters\\Troll.b3d'        # under data/Meshes
TEX_DST_REL  = 'Actors\\TrollBody.bmp'              # under data/Textures
MESH_DST = os.path.join(DATA, 'Meshes', 'Actors', 'Monsters', 'Troll.b3d')
TEX_DST  = os.path.join(DATA, 'Textures', 'Actors', 'TrollBody.bmp')
TEX_FALLBACK = os.path.join(DATA, 'Meshes', 'Actors', 'Monsters', 'Body.bmp')  # next-to-mesh

ATTACK_REMAP = {'Attack 1': 'Default attack', 'Attack 2': 'Right hand attack',
                'Attack 3': 'Two hand attack', 'Sitdown': 'Sit down'}

def main():
    for p in (MESH_SRC, TEX_SRC):
        if not os.path.exists(p):
            print(f"  ERROR: HF source missing: {p}"); return 1

    # 1. copy files
    for src, dst in [(MESH_SRC, MESH_DST), (TEX_SRC, TEX_DST), (TEX_SRC, TEX_FALLBACK)]:
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        if not os.path.exists(dst):
            shutil.copy2(src, dst); print(f"  copied -> {os.path.relpath(dst, DATA)}")
        else:
            print(f"  exists -> {os.path.relpath(dst, DATA)}")

    # 2a. register mesh
    mraw = open(os.path.join(DATA, 'Game Data', 'Meshes.dat'), 'rb').read()
    mdb = rcdata.MediaDB(mraw, rcdata.MESH)
    mesh_id = mdb.find_by_name(MESH_DST_REL)
    if mesh_id is None:
        mesh_id = mdb.add_file(MESH_DST_REL, is_anim=1)
        out = mdb.save()
        orig = rcdata.MediaDB(mraw, rcdata.MESH)
        for i, off in enumerate(orig.index):
            if off != 0: assert mdb.index[i] == off
        assert bytes(mdb.blob[:len(orig.blob)]) == bytes(orig.blob)
        open(os.path.join(DATA, 'Game Data', 'Meshes.dat'), 'wb').write(out)
        print(f"  registered mesh id {mesh_id} ({MESH_DST_REL})")
    else:
        print(f"  mesh already registered id {mesh_id}")

    # 2b. register texture
    traw = open(os.path.join(DATA, 'Game Data', 'Textures.dat'), 'rb').read()
    tdb = rcdata.MediaDB(traw, rcdata.TEXTURE)
    tex_id = tdb.find_by_name(TEX_DST_REL)
    if tex_id is None:
        tex_id = tdb.add_file(TEX_DST_REL, flags=9)
        out = tdb.save()
        orig = rcdata.MediaDB(traw, rcdata.TEXTURE)
        for i, off in enumerate(orig.index):
            if off != 0: assert tdb.index[i] == off
        assert bytes(tdb.blob[:len(orig.blob)]) == bytes(orig.blob)
        open(os.path.join(DATA, 'Game Data', 'Textures.dat'), 'wb').write(out)
        print(f"  registered texture id {tex_id} ({TEX_DST_REL})")
    else:
        print(f"  texture already registered id {tex_id}")

    # 3. port anim set
    apath = os.path.join(DATA, 'Game Data', 'Animations.dat')
    araw = open(apath, 'rb').read()
    rc = animsets.read_sets(araw)
    assert animsets.write_sets(rc) == araw, "anim round-trip failed — abort"
    anim_id = next((s['id'] for s in rc if s['name'] == 'Troll'), None)
    if anim_id is None:
        hf = animsets.read_sets(open(os.path.join(HF, 'Game Data', 'Animations.dat'), 'rb').read())
        troll = next(s for s in hf if s['name'] == 'Troll')
        used = {s['id'] for s in rc}
        anim_id = 0
        while anim_id in used: anim_id += 1
        new_anims = [(ATTACK_REMAP.get(n, n), a0, a1, sp) for (n, a0, a1, sp) in troll['anims']]
        rc.append({'id': anim_id, 'name': 'Troll', 'anims': new_anims})
        out = animsets.write_sets(rc)
        assert out[:len(araw)] == araw, "existing anim sets changed — abort"
        chk = animsets.read_sets(out)
        assert chk[-1]['name'] == 'Troll' and chk[-1]['id'] == anim_id
        open(apath, 'wb').write(out)
        print(f"  ported Troll anim set as id {anim_id} (attack names remapped)")
    else:
        print(f"  Troll anim set already present id {anim_id}")

    # 4. point Orc/Raider actor at the troll mesh/anim/texture
    aparth = os.path.join(DATA, 'Server Data', 'Actors.dat')
    actors = rcdata.read_actors(open(aparth, 'rb').read())
    for a in actors:
        if (a['race'], a['cls']) == ('Orc', 'Raider'):
            a['mesh_ids'][0] = mesh_id
            a['m_anim'] = anim_id; a['f_anim'] = anim_id
            a['male_body_ids'][0] = tex_id
            a['radius'] = 40.0   # troll-sized, was the 165-bone Orc's 67
            print(f"  Orc/Raider -> mesh {mesh_id}, anim set {anim_id}, body tex {tex_id}")
    open(aparth, 'wb').write(rcdata.write_actors(actors))

    # 5. re-enable the Orc spawn in Test Zone
    tz = os.path.join(DATA, 'Server Data', 'Areas', 'Test Zone.dat')
    area = rcdata.read_server_area(open(tz, 'rb').read())
    for s in area['spawns']:
        if s['actor'] == 4 and s['max'] == 0:
            s['max'] = 2; print("  re-enabled Orc/Raider spawn (max 2)")
    open(tz, 'wb').write(rcdata.write_server_area(area))
    return 0

if __name__ == '__main__':
    sys.exit(main())
