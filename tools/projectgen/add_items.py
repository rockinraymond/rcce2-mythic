"""Iteration 3 content: a real starter item catalog.

The project shipped only a placeholder Sword + Shield (both using spell-icon
textures, no world mesh). This:
  1. registers the 5 real item icons sitting unregistered on disk
     (data/Textures/Items/*.bmp), and
  2. adds a starter set of weapons, armour, a ring, and three potions —
     two instant (script-driven) + one timed buff (attribute-effect driven).

Idempotent on name. Append-only; re-parses its own output and asserts the
existing catalog bytes are an untouched prefix before writing.
"""
import os, sys
import rcdata
from rcdata import (I_WEAPON, I_ARMOUR, I_RING, I_POTION, SLOT_WEAPON, SLOT_SHIELD,
                    SLOT_HAT, SLOT_CHEST, SLOT_RING, W_ONEHAND, DMG)

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
ITEMS = os.path.join(DATA, 'Server Data', 'Items.dat')
TEXTURES = os.path.join(DATA, 'Game Data', 'Textures.dat')
TEX_DIR = os.path.join(DATA, 'Textures')
SCRIPTS = os.path.join(DATA, 'Server Data', 'Scripts')

# item icons to register: name relative to data/Textures (flag 9 = colour+mipmap,
# matching the existing spell-icon registrations).
ICONS = [
    'Items\\Long Sword.bmp',
    'Items\\Scarred Shield.bmp',
    'Items\\Imperial Helmet.bmp',
    'Items\\Imperial Armor.bmp',
    'Items\\Adventurine.bmp',
]

def tex_disk(name):
    return os.path.join(TEX_DIR, name.replace('\\', os.sep))

def main():
    # ---- 1. register icons ----
    tdb = rcdata.MediaDB(open(TEXTURES, 'rb').read(), rcdata.TEXTURE)
    icon_id = {}
    new_icons = []
    for name in ICONS:
        existing = tdb.find_by_name(name)
        if existing is not None:
            icon_id[name] = existing
            print(f"  icon registered already: {existing} {name}")
            continue
        if not os.path.exists(tex_disk(name)):
            print(f"  ERROR: icon not on disk: {name}")
            return 1
        nid = tdb.add_file(name, flags=9)
        icon_id[name] = nid
        new_icons.append(nid)
        print(f"  register icon: id {nid} {name}")

    if new_icons:
        raw_t = open(TEXTURES, 'rb').read()
        out_t = tdb.save()
        orig_t = rcdata.MediaDB(raw_t, rcdata.TEXTURE)
        for i, off in enumerate(orig_t.index):
            if off != 0:
                assert tdb.index[i] == off, f"texture index slot {i} moved"
        assert bytes(tdb.blob[:len(orig_t.blob)]) == bytes(orig_t.blob), "texture blob changed"
        with open(TEXTURES + '.tmp', 'wb') as f:
            f.write(out_t)
        os.replace(TEXTURES + '.tmp', TEXTURES)
        print(f"  Textures.dat updated: +{len(new_icons)} icons")

    LS = icon_id['Items\\Long Sword.bmp']
    SS = icon_id['Items\\Scarred Shield.bmp']
    IH = icon_id['Items\\Imperial Helmet.bmp']
    IA = icon_id['Items\\Imperial Armor.bmp']
    AV = icon_id['Items\\Adventurine.bmp']

    # ---- 2. build item catalog ----
    raw = open(ITEMS, 'rb').read()
    items = rcdata.read_items(raw)
    existing = {i['name'] for i in items}
    used = {i['id'] for i in items}

    def nid():
        i = 0
        while i in used:
            i += 1
        return i

    NEW = [
        # weapons
        ('Long Sword',     I_WEAPON, dict(thumb_tex=LS, value=85, mass=4, slot_type=SLOT_WEAPON,
                                          weapon_damage=7, weapon_damage_type=DMG['Slashing'],
                                          weapon_type=W_ONEHAND, takes_damage=1,
                                          misc_data='A well-balanced steel longsword.')),
        # armour
        ('Scarred Shield', I_ARMOUR, dict(thumb_tex=SS, value=60, mass=5, slot_type=SLOT_SHIELD,
                                          armour_level=3, takes_damage=1,
                                          misc_data='A battered but sturdy shield.')),
        ('Imperial Helmet',I_ARMOUR, dict(thumb_tex=IH, value=70, mass=2, slot_type=SLOT_HAT,
                                          armour_level=2, takes_damage=1,
                                          misc_data='Polished helm of the Imperial guard.')),
        ('Imperial Armor', I_ARMOUR, dict(thumb_tex=IA, value=140, mass=12, slot_type=SLOT_CHEST,
                                          armour_level=5, takes_damage=1,
                                          attrs={'Toughness': 2},
                                          misc_data='Heavy breastplate of the Imperial guard.')),
        # ring with a small bonus while worn
        ('Adventurine Ring',I_RING, dict(thumb_tex=AV, value=120, mass=0, slot_type=SLOT_RING,
                                          attrs={'Magic': 2},
                                          misc_data='A green aventurine set in a silver band.')),
        # potions: two instant (script), one timed buff (attribute effect)
        ('Potion of Healing', I_POTION, dict(thumb_tex=76, value=25, mass=1, stackable=1,
                                          eat_effects_length=0, script='Item_HealthPotion',
                                          smethod='Main', misc_data='Restores 50 health.')),
        ('Potion of Mana',    I_POTION, dict(thumb_tex=77, value=25, mass=1, stackable=1,
                                          eat_effects_length=0, script='Item_ManaPotion',
                                          smethod='Main', misc_data='Restores 40 mana.')),
        ('Elixir of Strength',I_POTION, dict(thumb_tex=72, value=90, mass=1, stackable=1,
                                          eat_effects_length=60, attrs={'Strength': 5},
                                          misc_data='+5 Strength for 60 seconds.')),
        # Trophy item: the shipped Ratcatcher1.rsl quest rewards this by name but
        # it was never in the catalog, so GiveItem silently dropped it. Name MUST
        # match the script's RewardItem$ string exactly ("Medalion" sic).
        ('Rat Catcher Medalion', rcdata.I_OTHER, dict(thumb_tex=86, value=50, mass=0,
                                          stackable=0,
                                          misc_data='A guild token, proof you cleared the rats.')),
    ]

    added = []
    for name, itype, kw in NEW:
        if name in existing:
            print(f"  skip (exists): {name}")
            continue
        # verify references resolve
        if kw.get('thumb_tex', -1) not in tdb.entries():
            print(f"  ERROR: thumb_tex {kw.get('thumb_tex')} for {name} not registered")
            return 1
        scr = kw.get('script', '')
        if scr and not os.path.exists(os.path.join(SCRIPTS, scr + '.rsl')):
            print(f"  ERROR: script {scr}.rsl missing for {name}")
            return 1
        i = nid(); used.add(i)
        items.append(rcdata.new_item(i, name, itype, **kw))
        added.append((i, name))
        print(f"  add item: id {i} '{name}' type {itype} icon {kw.get('thumb_tex')}")

    if not added:
        print("Nothing to add; item catalog already current.")
        return 0

    out = rcdata.write_items(items)
    chk = rcdata.read_items(out)
    assert len(chk) == len(items), "re-parse count mismatch"
    for a, b in zip(items, chk):
        assert a == b, f"re-parse mismatch on {a['name']}"
    assert out[:len(raw)] == raw, "existing item bytes changed — refusing"
    with open(ITEMS + '.tmp', 'wb') as f:
        f.write(out)
    os.replace(ITEMS + '.tmp', ITEMS)
    print(f"Wrote {ITEMS}: {len(items)} items, {len(out)} bytes (was {len(raw)}).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
