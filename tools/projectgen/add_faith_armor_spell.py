"""Add the Faith Armor spell (id 7) — a divine defensive buff (temporary +Toughness).
New spell *category* beyond the existing damage/heal roster. Uses the purpose-built
'Spell Icons\\Spells\\FaithArmor2.bmp' texture (id 73, registered) for its icon, the
Spell_FaithArmor.rsl script, and gets allowlisted (SetAttribute is privileged).

Appends to Spells.dat (existing spells asserted an untouched prefix). The trainer menu
+ teach branch were wired separately in Click_Trainer.rsl.
"""
import os, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
SP = os.path.join(SD, 'Spells.dat')
SCRIPTS = os.path.join(SD, 'Scripts')
PRIV = os.path.join(SD, 'Privileged Scripts.dat')

NAME = 'Faith Armor'
SCRIPT = 'Spell_FaithArmor'
THUMB = 73          # FaithArmor2.bmp (registered)
RECHARGE = 15000

def allowlist(name):
    b = open(PRIV, 'rb').read()
    if any(l.strip() == name.encode('latin-1') for l in b.split(b'\n')):
        print(f"  allowlist: {name} already present"); return
    eol = b'\r\n' if b'\r\n' in b else b'\n'
    if not b.endswith(eol):
        b += eol
    b += name.encode('latin-1') + eol
    open(PRIV, 'wb').write(b)
    print(f"  allowlist: + {name}")

def main():
    if not os.path.exists(os.path.join(SCRIPTS, SCRIPT + '.rsl')):
        print(f"ERROR: {SCRIPT}.rsl missing"); return 1
    tex = rcdata.read_textures(open(os.path.join(DATA, 'Game Data', 'Textures.dat'), 'rb').read())
    if THUMB not in tex:
        print(f"ERROR: thumb texture {THUMB} not registered"); return 1

    spells = rcdata.read_spells(open(SP, 'rb').read())
    if any(s['name'] == NAME for s in spells):
        print(f"  skip: spell {NAME!r} already present")
        allowlist(SCRIPT)
        return 0
    new_id = max(s['id'] for s in spells) + 1
    spell = dict(id=new_id, name=NAME,
                 description='A shield of faith hardens your skin, turning aside blows for a time.',
                 thumb_tex=THUMB, exc_race='', exc_class='',
                 recharge=RECHARGE, script=SCRIPT, smethod='Main')
    spells.append(spell)
    out = rcdata.write_spells(spells)
    chk = rcdata.read_spells(out)
    # existing spells unchanged (prefix)
    base = {s['id']: s for s in spells[:-1]}
    cm = {s['id']: s for s in chk}
    for sid, s in base.items():
        assert cm[sid] == s, f"existing spell {sid} changed"
    n = cm[new_id]
    assert n['name'] == NAME and n['thumb_tex'] == THUMB and n['script'] == SCRIPT
    with open(SP + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(SP + '.tmp', SP)
    print(f"  Spells.dat: + id{new_id} {NAME!r} (thumb {THUMB}, recharge {RECHARGE}, script {SCRIPT})")
    allowlist(SCRIPT)
    return 0

if __name__ == '__main__':
    sys.exit(main())
