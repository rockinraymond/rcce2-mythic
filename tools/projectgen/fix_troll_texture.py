"""Texture fix: the Troll mesh's embedded 'Body.bmp' is a 2x2 dummy (why the troll
looked untextured). HF applies the real skin via the actor body-texture system
(Actors3D.bb EntityTexture). The real skin is Textures/Actors/Monsters/Troll_02.png.
Import it, register it, and point the troll-based actors (Orc/Raider id4 + the
dormant Ork id2, both on the Troll mesh) at it via male_body/female_body.
"""
import os, sys, shutil
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SRC = 'C:/Users/dyanr/Desktop/HeroesFate/Game/Data/Textures/Actors/Monsters/Troll_02.png'
DST_REL = 'Actors\\Troll_02.png'
DST = os.path.join(DATA, 'Textures', 'Actors', 'Troll_02.png')

def main():
    if not os.path.exists(SRC):
        print(f"ERROR: source missing {SRC}"); return 1
    os.makedirs(os.path.dirname(DST), exist_ok=True)
    if not os.path.exists(DST):
        shutil.copy2(SRC, DST); print(f"copied -> Textures/{DST_REL}")
    else:
        print(f"exists -> Textures/{DST_REL}")

    # register texture (flag 9 = colour+mipmap, matching other actor skins)
    traw = open(os.path.join(DATA, 'Game Data', 'Textures.dat'), 'rb').read()
    tdb = rcdata.MediaDB(traw, rcdata.TEXTURE)
    tex_id = tdb.find_by_name(DST_REL)
    if tex_id is None:
        tex_id = tdb.add_file(DST_REL, flags=9)
        out = tdb.save()
        orig = rcdata.MediaDB(traw, rcdata.TEXTURE)
        for i, off in enumerate(orig.index):
            if off != 0: assert tdb.index[i] == off
        assert bytes(tdb.blob[:len(orig.blob)]) == bytes(orig.blob)
        open(os.path.join(DATA, 'Game Data', 'Textures.dat'), 'wb').write(out)
        print(f"registered texture id {tex_id} ({DST_REL})")
    else:
        print(f"texture already registered id {tex_id}")

    # point troll-based actors at the real skin (both male & female slot 0)
    ap = os.path.join(DATA, 'Server Data', 'Actors.dat')
    actors = rcdata.read_actors(open(ap, 'rb').read())
    troll_mesh = rcdata.MediaDB(open(os.path.join(DATA, 'Game Data', 'Meshes.dat'), 'rb').read(),
                                rcdata.MESH).find_by_name('Actors\\Monsters\\Troll.b3d')
    for a in actors:
        if a['mesh_ids'][0] == troll_mesh:
            a['male_body_ids'][0] = tex_id
            a['female_body_ids'][0] = tex_id
            print(f"  {a['race']}/{a['cls']} (id {a['id']}): body texture -> {tex_id}")
    open(ap, 'wb').write(rcdata.write_actors(actors))
    return 0

if __name__ == '__main__':
    sys.exit(main())
