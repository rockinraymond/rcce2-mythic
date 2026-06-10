"""RELIABILITY FIX: HF monster meshes crash this Blitz client build during the
walking actor's per-frame update — Orc.b3d (165 bones) hard-crashed, and the Troll
(19 bones, valid anim/texture) ALSO intermittently stack-overflows while walking
past the camera. The Human mesh (Male_02), by contrast, renders + walks flawlessly
(it's the player and every NPC). So point the Orc/Raider (id 4) and the dormant Ork
(id 2) at the Human mesh's exact rendering config — guaranteed crash-free, identical
pipeline to the player. Gameplay (race/class/stats/quest/spawn) is preserved, so the
'Raiders at the Gate' quest still works; the enemy just renders as a human raider.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
AP = os.path.join(DATA, 'Server Data', 'Actors.dat')

# fields that control rendering — copied from the proven Human/Fighter template
RENDER_FIELDS = ['mesh_ids', 'm_anim', 'f_anim', 'scale', 'radius', 'genders',
                 'beard_ids', 'male_hair_ids', 'female_hair_ids',
                 'male_face_ids', 'female_face_ids', 'male_body_ids',
                 'female_body_ids', 'blood_tex']

def main():
    actors = rcdata.read_actors(open(AP, 'rb').read())
    human = next((a for a in actors if (a['race'], a['cls']) == ('Human', 'Fighter')), None)
    if human is None:
        print("ERROR: Human/Fighter template not found"); return 1
    changed = []
    for a in actors:
        if a['race'] == 'Orc' or (a['race'], a['cls']) == ('Orc', 'Raider'):
            for f in RENDER_FIELDS:
                a[f] = list(human[f]) if isinstance(human[f], list) else human[f]
            changed.append(f"{a['race']}/{a['cls']} (id {a['id']})")
    if not changed:
        print("No Orc actors found."); return 0
    open(AP, 'wb').write(rcdata.write_actors(actors))
    print("Repointed to Human mesh/anim/appearance:", ', '.join(changed))
    return 0

if __name__ == '__main__':
    sys.exit(main())
