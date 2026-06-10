"""Make the now-Aggressive Orc Raiders actually threatening, so the restorative
content (Heal/Regeneration/Meditation spells, healing potions, the Shrine Keeper)
has a reason to exist.

Why this is the right lever (verified against GameServer.bb combat formula):
  weaponless-NPC melee  Damage = Strength/8 + Rand(-5,5) - AP   (AP clamps Damage>=1)
A new player's AP is small (shield only; no body armour in the starting kit; default
resistances), and the player starts at the 1000-HP cap. The orc's Strength was 18 ->
18/8 = 2, so post-AP damage clamped to ~1 = harmless. Strength is the orc's ONLY
damage source (weaponless), the formula is monotonic in Strength, and 1000 HP is a
huge buffer, so raising it can add tension without frustrating-death risk.

Conservative bump: Orc Raider Strength 18 -> 80  (=> ~10 +/-5 - AP ~ 1..11 per hit).
A 2-orc pack (plus faction-recruited rats) now chips meaningful HP over a sustained
fight, making heals/potions/Shrine useful, while a 1000-HP player is never one-shot
or swarm-killed. Rats stay weak (Strength 6). Strength is attribute index 2 (confirmed
by the Elixir of Strength: attributes[2]=+5, misc "+5 Strength").

FLAG: balance is runtime-sensitive (AP/hit-chance/level-growth). This is a static,
conservative estimate — playtest and adjust Strength up/down (or lower player starting
HP) to taste. Strictly safe in the "too hard" direction: more Strength only raises orc
damage from its current ~1; the min-1 clamp protects the floor.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
ACT = os.path.join(DATA, 'Server Data', 'Actors.dat')

STRENGTH = 2
NEW_STR = 80

def main():
    raw = open(ACT, 'rb').read()
    actors = rcdata.read_actors(raw)
    base = {a['id']: dict(a) for a in actors}
    changed = None
    for a in actors:
        if a['race'].upper() in ('ORC', 'ORK') and a['cls'].upper() == 'RAIDER':
            old = a['attr_value'][STRENGTH]
            if old == NEW_STR:
                print(f"  skip: Orc Raider Strength already {NEW_STR}"); return 0
            a['attr_value'][STRENGTH] = NEW_STR
            changed = (a['id'], old)
    if changed is None:
        print("  ERROR: no Orc/Raider actor found"); return 1

    out = rcdata.write_actors(actors)
    chk = {a['id']: a for a in rcdata.read_actors(out)}
    cid, old = changed
    assert chk[cid]['attr_value'][STRENGTH] == NEW_STR
    # everything else byte-identical-by-field
    for aid, b in base.items():
        if aid == cid:
            for k in b:
                if k == 'attr_value':
                    continue
                assert chk[aid][k] == b[k], f"orc field {k} changed unexpectedly"
            for i, v in enumerate(b['attr_value']):
                if i == STRENGTH:
                    continue
                assert chk[aid]['attr_value'][i] == v, f"orc attr {i} changed"
        else:
            assert chk[aid] == b, f"untouched actor id{aid} changed"

    with open(ACT + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(ACT + '.tmp', ACT)
    print(f"  Actors.dat: Orc/Raider id{cid} Strength {old} -> {NEW_STR}")
    return 0

if __name__ == '__main__':
    sys.exit(main())
