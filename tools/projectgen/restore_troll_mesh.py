"""Restore the proper Troll monster mesh on the orc actors (undo the human-reskin
hedge — the crashes were sound-source exhaustion, not the mesh). The Troll rendered,
walked, animated and died fine in testing. Keeps actor Speech arrays OFF (per-step
footstep/combat sounds are the exhaustion driver and stay disabled for stability).
"""
import os, sys
import rcdata

DATA = os.path.normpath(os.path.join(os.path.dirname(__file__), '..', '..', 'data'))
AP = os.path.join(DATA, 'Server Data', 'Actors.dat')

TROLL_MESH, TROLL_ANIM, TROLL_TEX, TROLL_RADIUS = 83, 4, 88, 40.0

def main():
    actors = rcdata.read_actors(open(AP, 'rb').read())
    changed = []
    for a in actors:
        if a['race'].upper() in ('ORC', 'ORK'):
            a['mesh_ids'] = [TROLL_MESH, -1, -1, -1, -1, -1, -1, -1]
            a['m_anim'] = TROLL_ANIM
            a['f_anim'] = TROLL_ANIM
            a['male_body_ids'] = [TROLL_TEX, -1, -1, -1, -1]
            a['female_body_ids'] = [TROLL_TEX, -1, -1, -1, -1]
            a['male_face_ids'] = [-1, -1, -1, -1, -1]
            a['female_face_ids'] = [-1, -1, -1, -1, -1]
            a['radius'] = TROLL_RADIUS
            a['genders'] = 3
            a['blood_tex'] = 12
            # speech intentionally left as-is (cleared/off)
            changed.append(f"{a['race']}/{a['cls']} id{a['id']}")
    if not changed:
        print("No orc actors found."); return 0
    open(AP, 'wb').write(rcdata.write_actors(actors))
    print("Restored Troll mesh/anim/texture on:", ', '.join(changed))
    return 0

if __name__ == '__main__':
    sys.exit(main())
