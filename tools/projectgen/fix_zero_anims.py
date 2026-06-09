"""Crash fix (mesh-independent, on death): the P_ActorDead handler plays a RANDOM
death animation (Rand(Death1..Death3)). Several anim sets have zero-length (0-0)
death ranges (Player 'Death 3'; Rat 'Death 2/3'; Ork all). A 0-length frame range
is an integer divide-by-zero in the client's animation code = "Stack overflow!".
Current engine source guards it (PlayAnimation returns early on AnimEnd=0), but a
stale client binary crashes. Fixing the DATA removes every 0-0 range so it can't
divide by zero on any client build.

For each set, every 0-0 named animation is repointed to a sensible non-zero range:
  death  -> a valid death pose in the same set (else idle)
  attack -> 'Default attack' in the same set (else idle)
  other  -> 'Idle'
Animations that already have a valid range are left untouched. Idempotent.
"""
import os, sys
import animsets

DATA = animsets.DATA
APATH = os.path.join(DATA, 'Game Data', 'Animations.dat')

def find_range(anims, name):
    for (n, a0, a1, sp) in anims:
        if n.lower() == name.lower() and not (a0 == 0 and a1 == 0):
            return (a0, a1)
    return None

def main():
    raw = open(APATH, 'rb').read()
    sets = animsets.read_sets(raw)
    assert animsets.write_sets(sets) == raw, "anim round-trip failed pre-edit — abort"

    total = 0
    for s in sets:
        anims = s['anims']
        idle = find_range(anims, 'Idle')
        if idle is None:
            print(f"  set {s['name']!r}: no valid Idle — skipping"); continue
        valid_death = (find_range(anims, 'Death 1') or find_range(anims, 'Death 2')
                       or find_range(anims, 'Death 3') or idle)
        valid_attack = find_range(anims, 'Default attack') or idle
        fixed = []
        for i, (n, a0, a1, sp) in enumerate(anims):
            if n and a0 == 0 and a1 == 0:
                nl = n.lower()
                if 'death' in nl:
                    tgt = valid_death
                elif 'attack' in nl:
                    tgt = valid_attack
                else:
                    tgt = idle
                anims[i] = (n, tgt[0], tgt[1], sp)
                fixed.append(n)
        if fixed:
            total += len(fixed)
            print(f"  set {s['id']} {s['name']!r}: fixed {len(fixed)} zero-range anims "
                  f"(idle={idle}, death->{valid_death})")

    if total == 0:
        print("No zero-range anims to fix."); return 0
    out = animsets.write_sets(sets)
    assert animsets.read_sets(out) == sets, "re-parse mismatch — abort"
    assert len(out) == len(raw), "size changed (should be fixed-layout) — abort"
    open(APATH, 'wb').write(out)
    print(f"Wrote Animations.dat ({total} zero-range anims repointed; size {len(out)} unchanged).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
