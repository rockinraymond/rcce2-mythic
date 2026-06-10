"""Iteration 1 content: add restorative spells to the catalog.

Re-uses spell-icon textures already registered in Textures.dat (Heal=76,
Meditation=77, Regeneration=78) and the new Spell_*.rsl scripts. Idempotent:
re-running won't duplicate spells that already exist by name.
"""
import os, sys
import rcdata

DATA = os.path.normpath(os.path.join(os.path.dirname(__file__), '..', '..', 'data'))
SPELLS = os.path.join(DATA, 'Server Data', 'Spells.dat')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')
TEXTURES = os.path.join(DATA, 'Game Data', 'Textures.dat')

# name -> (description, thumb_tex, recharge_ms, script)
NEW_SPELLS = [
    ('Heal',          'Channel divine energy to mend your wounds.',         76,  6000, 'Spell_Heal'),
    ('Regeneration',  'A soothing warmth restores your health over time.',  78, 30000, 'Spell_Regeneration'),
    ('Meditation',    'Sit and focus to recover your mana.',                77, 20000, 'Spell_Meditation'),
    ('Frost Bolt',    'Hurl a bolt of ice that chills and wounds a foe.',   10,  4000, 'Spell_FrostBolt'),
    ('Lightning Bolt','A fast, powerful arc of lightning. High mana cost.', 79,  7000, 'Spell_Lightning'),
    ('Flame Nova',    'Burst fire outward, scorching all nearby foes (AoE).', 67, 10000, 'Spell_FlameNova'),
]

def main():
    raw = open(SPELLS, 'rb').read()
    spells = rcdata.read_spells(raw)
    existing = {s['name'] for s in spells}
    used_ids = {s['id'] for s in spells}

    # validate texture references exist
    tex = rcdata.MediaDB(open(TEXTURES, 'rb').read(), rcdata.TEXTURE).entries()

    def next_id():
        i = 0
        while i in used_ids:
            i += 1
        return i

    added = []
    for name, desc, tx, recharge, script in NEW_SPELLS:
        if name in existing:
            print(f"  skip (exists): {name}")
            continue
        if tx not in tex:
            print(f"  ERROR: texture id {tx} for {name} not registered — aborting")
            return 1
        scriptfile = os.path.join(SCRIPTS, script + '.rsl')
        if not os.path.exists(scriptfile):
            print(f"  ERROR: script {script}.rsl missing for {name} — aborting")
            return 1
        sid = next_id()
        used_ids.add(sid)
        spells.append(dict(id=sid, name=name, description=desc, thumb_tex=tx,
                           exc_race='', exc_class='', recharge=recharge,
                           script=script, smethod='Main'))
        added.append((sid, name))
        print(f"  add: id {sid} '{name}' icon {tx} ({tex[tx]['name']}) -> {script}.rsl recharge {recharge}ms")

    if not added:
        print("Nothing to add; catalog already current.")
        return 0

    out = rcdata.write_spells(spells)
    # safety: re-read what we just encoded and confirm it parses back identically
    check = rcdata.read_spells(out)
    assert len(check) == len(spells), "re-parse count mismatch"
    for a, b in zip(spells, check):
        assert a == b, f"re-parse mismatch on {a['name']}"
    # confirm the original Fireball record bytes are untouched at the front
    assert out[:len(raw)] == raw, "existing catalog bytes changed — refusing to write"

    tmp = SPELLS + '.tmp'
    with open(tmp, 'wb') as f:
        f.write(out)
    os.replace(tmp, SPELLS)
    print(f"Wrote {SPELLS}: {len(spells)} spells, {len(out)} bytes (was {len(raw)}).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
