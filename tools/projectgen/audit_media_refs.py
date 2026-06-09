"""Reference-integrity audit: every media ID referenced by the catalogs (item/spell
thumbnails, projectile meshes, actor meshes + body/face textures, blood textures)
must resolve to a registered entry in the corresponding media DB, or the player sees
a missing icon / the renderer drops the asset. -1 / 0xFFFF means 'none' and is skipped.

Read-only. Exit 1 if any dangling reference is found.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
GD = os.path.join(DATA, 'Game Data')

NONE = (-1, 65535, 0xFFFF)

def reg_ids(entries):
    # MediaDB.entries() -> dict keyed by media ID
    return set(entries.keys())

def main():
    tex = reg_ids(rcdata.read_textures(open(os.path.join(GD, 'Textures.dat'), 'rb').read()))
    mesh = reg_ids(rcdata.read_meshes(open(os.path.join(GD, 'Meshes.dat'), 'rb').read()))
    print(f"registered: {len(tex)} textures, {len(mesh)} meshes")

    problems = []

    def chk_tex(ctx, tid):
        if tid in NONE or tid == 0:
            return
        if tid not in tex:
            problems.append(f"{ctx}: texture id {tid} not registered")

    def chk_mesh(ctx, mid):
        if mid in NONE:
            return
        if mid not in mesh:
            problems.append(f"{ctx}: mesh id {mid} not registered")

    spells = rcdata.read_spells(open(os.path.join(SD, 'Spells.dat'), 'rb').read())
    for s in spells:
        chk_tex(f"Spell '{s['name']}' thumb", s['thumb_tex'])

    items = rcdata.read_items(open(os.path.join(SD, 'Items.dat'), 'rb').read())
    for it in items:
        chk_tex(f"Item '{it['name']}' thumb", it['thumb_tex'])
        chk_mesh(f"Item '{it['name']}' mmesh", it['mmesh'])
        chk_mesh(f"Item '{it['name']}' fmesh", it['fmesh'])

    projs = rcdata.read_projectiles(open(os.path.join(SD, 'Projectiles.dat'), 'rb').read())
    for p in projs:
        chk_mesh(f"Projectile '{p['name']}' mesh", p['mesh'])
        chk_tex(f"Projectile '{p['name']}' emitter1 tex", p['emitter1_tex'])
        chk_tex(f"Projectile '{p['name']}' emitter2 tex", p['emitter2_tex'])

    actors = rcdata.read_actors(open(os.path.join(SD, 'Actors.dat'), 'rb').read())
    for a in actors:
        tag = f"Actor {a['id']} {a['race']}/{a['cls']}"
        for m in a['mesh_ids']:
            chk_mesh(f"{tag} mesh", m)
        for t in a['male_body_ids'] + a['female_body_ids']:
            chk_tex(f"{tag} body tex", t)
        for t in a['male_face_ids'] + a['female_face_ids']:
            chk_tex(f"{tag} face tex", t)
        chk_tex(f"{tag} blood tex", a['blood_tex'])

    if problems:
        print(f"\n=== {len(problems)} dangling media reference(s) ===")
        for p in problems:
            print("  " + p)
        return 1
    print("OK: all referenced media IDs resolve to registered entries.")
    return 0

if __name__ == '__main__':
    sys.exit(main())
