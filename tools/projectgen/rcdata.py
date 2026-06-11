"""rcdata.py — faithful codec for RCCE2 / RealmCrafter binary project files.

BlitzForge file I/O primitives (all little-endian):
  ReadByte  / WriteByte   : 1 byte unsigned
  ReadShort / WriteShort  : 2 bytes signed 16-bit
  ReadInt   / WriteInt    : 4 bytes signed 32-bit
  ReadFloat / WriteFloat  : 4 bytes IEEE-754 single
  ReadString/ WriteString : 4-byte LE length prefix + raw bytes (NOT null-terminated)

Verified against the loader/saver source in:
  src/Modules/Items.bb   (LoadItems/SaveItems)
  src/Modules/Spells.bb  (LoadSpells/SaveSpells)
  src/Modules/Media.bb   (CreateDatabase / Add*/Remove* — Meshes/Textures/Sounds index DBs)

Media .dat layout: 65535 int32 index slots (slot[ID] = byte offset of that ID's
record, or 0 if empty), then variable-length records appended after the index.
  texture record: flags(short) + name(string)
  mesh    record: isAnim(byte) + scale(float) + x(float) + y(float) + z(float) + shader(short) + name(string)
  sound   record: is3D(byte) + name(string)
"""

import struct
import io

# ---------- low-level stream ----------

class Reader:
    def __init__(self, data: bytes):
        self.b = data
        self.p = 0
    def eof(self):
        return self.p >= len(self.b)
    def byte(self):
        v = self.b[self.p]; self.p += 1; return v
    def short(self):
        v = struct.unpack_from('<h', self.b, self.p)[0]; self.p += 2; return v
    def ushort(self):
        v = struct.unpack_from('<H', self.b, self.p)[0]; self.p += 2; return v
    def int(self):
        v = struct.unpack_from('<i', self.b, self.p)[0]; self.p += 4; return v
    def float(self):
        v = struct.unpack_from('<f', self.b, self.p)[0]; self.p += 4; return v
    def string(self):
        n = self.int()
        if n < 0:
            raise ValueError(f"negative string length {n} at {self.p-4}")
        s = self.b[self.p:self.p+n]; self.p += n
        return s.decode('latin-1')
    def seek(self, pos):
        self.p = pos

class Writer:
    def __init__(self):
        self.buf = io.BytesIO()
    def byte(self, v):  self.buf.write(struct.pack('<B', v & 0xFF))
    def short(self, v): self.buf.write(struct.pack('<h', v))
    def ushort(self, v):self.buf.write(struct.pack('<H', v))
    def int(self, v):   self.buf.write(struct.pack('<i', v))
    def float(self, v): self.buf.write(struct.pack('<f', v))
    def string(self, s):
        raw = s.encode('latin-1')
        self.int(len(raw)); self.buf.write(raw)
    def getvalue(self):
        return self.buf.getvalue()

# ---------- Spells.dat ----------

SPELL_FIELDS = ['id','name','description','thumb_tex','exc_race','exc_class',
                'recharge','script','smethod']

def read_spells(data: bytes):
    r = Reader(data); out = []
    while not r.eof():
        sid = r.short()
        if sid < 0 or sid > 65534:
            break
        s = dict(id=sid, name=r.string(), description=r.string(),
                 thumb_tex=r.short(), exc_race=r.string(), exc_class=r.string(),
                 recharge=r.int(), script=r.string(), smethod=r.string())
        out.append(s)
    return out

def write_spells(spells):
    w = Writer()
    for s in spells:
        w.short(s['id']); w.string(s['name']); w.string(s['description'])
        w.short(s['thumb_tex']); w.string(s['exc_race']); w.string(s['exc_class'])
        w.int(s['recharge']); w.string(s['script']); w.string(s['smethod'])
    return w.getvalue()

# ---------- Items.dat ----------
# ItemType enum (from Items.bb): I_Misc=0, I_Weapon=1, I_Armour=2, I_Potion=3,
#   I_Image=4, I_Ingredient=5  (verified below at runtime if needed)
# Attributes: 40 shorts, stored as value+5000.

def read_items(data: bytes):
    r = Reader(data); out = []
    while not r.eof():
        iid = r.short()
        if iid < 0 or iid > 65534:
            break
        it = dict(id=iid, name=r.string(), exc_race=r.string(), exc_class=r.string(),
                  script=r.string(), smethod=r.string(), item_type=r.byte(),
                  value=r.int(), mass=r.short(), takes_damage=r.byte(),
                  thumb_tex=r.short())
        it['gubbins'] = [r.short() for _ in range(6)]
        it['mmesh'] = r.short(); it['fmesh'] = r.short()
        it['slot_type'] = r.short(); it['stackable'] = r.byte()
        it['attributes'] = [r.short() - 5000 for _ in range(40)]
        # ItemType enum (Items.bb): I_Weapon=1 I_Armour=2 I_Ring=3 I_Potion=4
        #   I_Ingredient=5 I_Image=6 I_Other=7. Extra fields per the engine's
        #   Select Case: weapon / armour / (potion,ingredient) / image only.
        t = it['item_type']
        if t == 1:    # Weapon
            it['weapon_damage'] = r.short(); it['weapon_damage_type'] = r.short()
            it['weapon_type'] = r.short(); it['ranged_projectile'] = r.short()
            it['range'] = r.float(); it['ranged_animation'] = r.string()
        elif t == 2:  # Armour
            it['armour_level'] = r.short()
        elif t in (4, 5):  # Potion / Ingredient
            it['eat_effects_length'] = r.short()
        elif t == 6:  # Image
            it['image_id'] = r.short()
        it['misc_data'] = r.string()
        out.append(it)
    return out

def write_items(items):
    w = Writer()
    for it in items:
        w.short(it['id']); w.string(it['name']); w.string(it['exc_race'])
        w.string(it['exc_class']); w.string(it['script']); w.string(it['smethod'])
        w.byte(it['item_type']); w.int(it['value']); w.short(it['mass'])
        w.byte(it['takes_damage']); w.short(it['thumb_tex'])
        for g in it['gubbins']: w.short(g)
        w.short(it['mmesh']); w.short(it['fmesh']); w.short(it['slot_type'])
        w.byte(it['stackable'])
        for a in it['attributes']: w.short(a + 5000)
        t = it['item_type']
        if t == 1:
            w.short(it['weapon_damage']); w.short(it['weapon_damage_type'])
            w.short(it['weapon_type']); w.short(it['ranged_projectile'])
            w.float(it['range']); w.string(it['ranged_animation'])
        elif t == 2:
            w.short(it['armour_level'])
        elif t in (4, 5):
            w.short(it['eat_effects_length'])
        elif t == 6:
            w.short(it['image_id'])
        w.string(it['misc_data'])
    return w.getvalue()

# Item type / slot / weapon-type constants (from Items.bb / Inventories.bb)
I_WEAPON, I_ARMOUR, I_RING, I_POTION, I_INGREDIENT, I_IMAGE, I_OTHER = 1,2,3,4,5,6,7
SLOT_WEAPON, SLOT_SHIELD, SLOT_HAT, SLOT_CHEST, SLOT_HAND = 1,2,3,4,5
SLOT_BELT, SLOT_LEGS, SLOT_FEET, SLOT_RING, SLOT_AMULET, SLOT_BACKPACK = 6,7,8,9,10,11
W_ONEHAND, W_TWOHAND = 1, 2
# Damage.dat order
DMG = dict(Piercing=0, Slashing=1, Bashing=2, Fire=3, Ice=4, Poison=5,
           Electricity=6, Shadow=7, Divine=8, Wind=9, Magical=10)
# Attributes.dat order
ATTR = dict(Health=0, Mana=1, Strength=2, Dexterity=3, Speed=4, Magic=5,
            Toughness=6, Swimming=11)

def new_item(id, name, item_type, **kw):
    """Build a fully-formed item dict with sane defaults for every field the
    codec writes, including the 40-slot attribute array and the per-type extra
    fields. Pass overrides via kw; pass attrs={'Strength':5,...} for bonuses."""
    it = dict(id=id, name=name, item_type=item_type,
              exc_race=kw.get('exc_race',''), exc_class=kw.get('exc_class',''),
              script=kw.get('script',''), smethod=kw.get('smethod',''),
              value=kw.get('value',1), mass=kw.get('mass',1),
              takes_damage=kw.get('takes_damage',0),
              thumb_tex=kw.get('thumb_tex',-1),
              gubbins=list(kw.get('gubbins',[0,0,0,0,0,0])),
              mmesh=kw.get('mmesh',-1), fmesh=kw.get('fmesh',-1),
              slot_type=kw.get('slot_type',0), stackable=kw.get('stackable',0),
              misc_data=kw.get('misc_data',''))
    attrs = [0]*40
    for k, v in kw.get('attrs', {}).items():
        attrs[ATTR[k] if isinstance(k, str) else k] = v
    it['attributes'] = attrs
    if item_type == I_WEAPON:
        it['weapon_damage'] = kw.get('weapon_damage',1)
        it['weapon_damage_type'] = kw.get('weapon_damage_type',0)
        it['weapon_type'] = kw.get('weapon_type', W_ONEHAND)
        it['ranged_projectile'] = kw.get('ranged_projectile',0)
        it['range'] = kw.get('range',0.0)
        it['ranged_animation'] = kw.get('ranged_animation','')
    elif item_type == I_ARMOUR:
        it['armour_level'] = kw.get('armour_level',1)
    elif item_type in (I_POTION, I_INGREDIENT):
        it['eat_effects_length'] = kw.get('eat_effects_length',0)
    elif item_type == I_IMAGE:
        it['image_id'] = kw.get('image_id',0)
    return it


# ---------- Actors.dat (Actor templates — race/class creature types) ----------
# NOT the huge per-character ActorInstance record; this is the compact Actor
# template list (Actors.bb LoadActors/SaveActors). Anim set ids (Animations.dat):
# 0=Player, 1=Stag, 2=Rat, 3=Ork (per Animations_debug.txt).

def read_actors(data: bytes):
    r = Reader(data); out = []
    while not r.eof():
        aid = r.short()
        if aid < 0 or aid > 65535:
            break
        a = dict(id=aid, race=r.string(), cls=r.string(), description=r.string(),
                 start_area=r.string(), start_portal=r.string(),
                 m_anim=r.short(), f_anim=r.short(),
                 scale=r.float(), radius=r.float())
        a['mesh_ids']        = [r.short() for _ in range(8)]
        a['beard_ids']       = [r.short() for _ in range(5)]
        a['male_hair_ids']   = [r.short() for _ in range(5)]
        a['female_hair_ids'] = [r.short() for _ in range(5)]
        a['male_face_ids']   = [r.short() for _ in range(5)]
        a['female_face_ids'] = [r.short() for _ in range(5)]
        a['male_body_ids']   = [r.short() for _ in range(5)]
        a['female_body_ids'] = [r.short() for _ in range(5)]
        a['m_speech_ids']    = [r.short() for _ in range(16)]
        a['f_speech_ids']    = [r.short() for _ in range(16)]
        a['blood_tex']       = r.short()
        a['attr_value']      = []
        a['attr_max']        = []
        for _ in range(40):
            a['attr_value'].append(r.short()); a['attr_max'].append(r.short())
        a['resistances']     = [r.short() for _ in range(20)]
        a['genders']         = r.byte()
        a['playable']        = r.byte()
        a['rideable']        = r.byte()
        a['aggressiveness']  = r.byte()
        a['aggressive_range']= r.int()
        a['trade_mode']      = r.byte()
        a['environment']     = r.byte()
        a['inventory_slots'] = r.int()
        a['default_damage_type'] = r.byte()
        a['default_faction'] = r.byte()
        a['xp_multiplier']   = r.int()
        a['poly_collision']  = r.byte()
        out.append(a)
    return out

def write_actors(actors):
    w = Writer()
    for a in actors:
        w.short(a['id']); w.string(a['race']); w.string(a['cls'])
        w.string(a['description']); w.string(a['start_area']); w.string(a['start_portal'])
        w.short(a['m_anim']); w.short(a['f_anim']); w.float(a['scale']); w.float(a['radius'])
        for k, n in [('mesh_ids',8),('beard_ids',5),('male_hair_ids',5),
                     ('female_hair_ids',5),('male_face_ids',5),('female_face_ids',5),
                     ('male_body_ids',5),('female_body_ids',5),('m_speech_ids',16),
                     ('f_speech_ids',16)]:
            for v in a[k]: w.short(v)
        w.short(a['blood_tex'])
        for i in range(40):
            w.short(a['attr_value'][i]); w.short(a['attr_max'][i])
        for v in a['resistances']: w.short(v)
        w.byte(a['genders']); w.byte(a['playable']); w.byte(a['rideable'])
        w.byte(a['aggressiveness']); w.int(a['aggressive_range'])
        w.byte(a['trade_mode']); w.byte(a['environment']); w.int(a['inventory_slots'])
        w.byte(a['default_damage_type']); w.byte(a['default_faction'])
        w.int(a['xp_multiplier']); w.byte(a['poly_collision'])
    return w.getvalue()

def new_actor(id, race, cls, mesh_id, m_anim, **kw):
    """Build an Actor template with sane defaults. mesh_id goes in MeshIDs[0]."""
    a = dict(id=id, race=race, cls=cls,
             description=kw.get('description',''),
             start_area=kw.get('start_area',''), start_portal=kw.get('start_portal',''),
             m_anim=m_anim, f_anim=kw.get('f_anim', m_anim),
             scale=kw.get('scale',1.0), radius=kw.get('radius',5.0),
             mesh_ids=list(kw.get('mesh_ids',[mesh_id,-1,-1,-1,-1,-1,-1,-1])),
             beard_ids=[-1]*5, male_hair_ids=[-1]*5, female_hair_ids=[-1]*5,
             male_face_ids=[-1]*5, female_face_ids=[-1]*5,
             male_body_ids=[-1]*5, female_body_ids=[-1]*5,
             m_speech_ids=list(kw.get('m_speech_ids',[-1]*16)),
             f_speech_ids=list(kw.get('f_speech_ids',[-1]*16)),
             blood_tex=kw.get('blood_tex',12),
             resistances=list(kw.get('resistances',[0]*20)),
             genders=kw.get('genders',0), playable=kw.get('playable',0),
             rideable=kw.get('rideable',0), aggressiveness=kw.get('aggressiveness',0),
             aggressive_range=kw.get('aggressive_range',0),
             trade_mode=kw.get('trade_mode',0), environment=kw.get('environment',0),
             inventory_slots=kw.get('inventory_slots',0),
             default_damage_type=kw.get('default_damage_type',0),
             default_faction=kw.get('default_faction',0),
             xp_multiplier=kw.get('xp_multiplier',1),
             poly_collision=kw.get('poly_collision',0))
    av = [0]*40; am = [0]*40
    for k, v in kw.get('attrs', {}).items():
        idx = ATTR[k] if isinstance(k, str) else k
        av[idx] = v; am[idx] = v
    # allow separate max via attrs_max
    for k, v in kw.get('attrs_max', {}).items():
        idx = ATTR[k] if isinstance(k, str) else k
        am[idx] = v
    a['attr_value'] = av; a['attr_max'] = am
    return a


# ---------- Projectiles.dat ----------
def read_projectiles(data: bytes):
    r = Reader(data); out = []
    while not r.eof():
        pid = r.short()
        if pid < 0 or pid > 5000:
            break
        out.append(dict(id=pid, name=r.string(), mesh=r.short(),
                        emitter1=r.string(), emitter2=r.string(),
                        emitter1_tex=r.short(), emitter2_tex=r.short(),
                        homing=r.byte(), hit_chance=r.byte(),
                        damage=r.short(), damage_type=r.short(), speed=r.byte()))
    return out

def write_projectiles(projs):
    w = Writer()
    for p in projs:
        w.short(p['id']); w.string(p['name']); w.short(p['mesh'])
        w.string(p['emitter1']); w.string(p['emitter2'])
        w.short(p['emitter1_tex']); w.short(p['emitter2_tex'])
        w.byte(p['homing']); w.byte(p['hit_chance'])
        w.short(p['damage']); w.short(p['damage_type']); w.byte(p['speed'])
    return w.getvalue()


# ---------- Server Data/Factions.dat ----------
# Layout verified against Actors.bb LoadFactions / SaveFactions:
#   100 faction-name strings (4-byte LE len + latin-1), then a 100x100 byte grid
#   of FactionDefaultRatings(i, j) = faction i's default rating toward faction j.
# Rating scale (from GameServer/Interface3D usage): 0 = hostile, ~150 = neutral
# threshold (AILookForTargets attacks a target whose faction rating is < 150),
# 200+ = allied.

def read_factions(data: bytes):
    r = Reader(data)
    names = [r.string() for _ in range(100)]
    grid = [[r.byte() for _ in range(100)] for _ in range(100)]
    return dict(names=names, grid=grid)

def write_factions(f):
    w = Writer()
    for n in f['names']:
        w.string(n)
    for i in range(100):
        for j in range(100):
            w.byte(f['grid'][i][j])
    return w.getvalue()


# ---------- Server Data/Areas/*.dat (gameplay area: spawns, waypoints, portals) ----------
# Layout verified against ServerAreas.bb ServerLoadArea / ServerSaveArea.
# NOTE: the file does NOT store the area name (that comes from the filename).

def read_server_area(data: bytes):
    r = Reader(data)
    a = {}
    a['weather_chance'] = [r.byte() for _ in range(5)]
    a['entry_script'] = r.string()
    a['exit_script'] = r.string()
    a['pvp'] = r.byte()
    a['gravity'] = r.short()
    a['outdoors'] = r.byte()
    a['weather_link'] = r.string()
    a['triggers'] = [dict(x=r.float(), y=r.float(), z=r.float(), size=r.float(),
                          script=r.string(), method=r.string()) for _ in range(150)]
    a['waypoints'] = [dict(x=r.float(), y=r.float(), z=r.float(),
                           next_a=r.short(), next_b=r.short(), prev=r.short(),
                           pause=r.int()) for _ in range(2000)]
    a['portals'] = [dict(name=r.string(), link_area=r.string(), link_name=r.string(),
                         x=r.float(), y=r.float(), z=r.float(), size=r.float(),
                         yaw=r.float()) for _ in range(100)]
    a['spawns'] = [dict(actor=r.short(), waypoint=r.short(), size=r.float(),
                        script=r.string(), actor_script=r.string(),
                        death_script=r.string(), max=r.short(), frequency=r.short(),
                        range=r.float()) for _ in range(1000)]
    nwater = r.short()
    a['waters'] = [dict(x=r.float(), y=r.float(), z=r.float(), width=r.float(),
                        depth=r.float(), damage=r.short(), damage_type=r.short())
                   for _ in range(nwater)]
    return a

def write_server_area(a):
    w = Writer()
    for v in a['weather_chance']: w.byte(v)
    w.string(a['entry_script']); w.string(a['exit_script'])
    w.byte(a['pvp']); w.short(a['gravity']); w.byte(a['outdoors'])
    w.string(a['weather_link'])
    for t in a['triggers']:
        w.float(t['x']); w.float(t['y']); w.float(t['z']); w.float(t['size'])
        w.string(t['script']); w.string(t['method'])
    for p in a['waypoints']:
        w.float(p['x']); w.float(p['y']); w.float(p['z'])
        w.short(p['next_a']); w.short(p['next_b']); w.short(p['prev']); w.int(p['pause'])
    for p in a['portals']:
        w.string(p['name']); w.string(p['link_area']); w.string(p['link_name'])
        w.float(p['x']); w.float(p['y']); w.float(p['z']); w.float(p['size']); w.float(p['yaw'])
    for s in a['spawns']:
        w.short(s['actor']); w.short(s['waypoint']); w.float(s['size'])
        w.string(s['script']); w.string(s['actor_script']); w.string(s['death_script'])
        w.short(s['max']); w.short(s['frequency']); w.float(s['range'])
    w.short(len(a['waters']))
    for wt in a['waters']:
        w.float(wt['x']); w.float(wt['y']); w.float(wt['z'])
        w.float(wt['width']); w.float(wt['depth'])
        w.short(wt['damage']); w.short(wt['damage_type'])
    return w.getvalue()


# ---------- Data/Areas/*.dat (CLIENT visual area: terrain, scenery, water, emitters,
#            sound zones). Layout verified against ClientAreas.bb SaveArea (the writer). ----------

def read_client_area(data: bytes):
    r = Reader(data)
    a = dict(
        loading_tex=r.short(), loading_music=r.short(),
        sky_tex=r.short(), cloud_tex=r.short(), storm_cloud_tex=r.short(), stars_tex=r.short(),
        fog_r=r.byte(), fog_g=r.byte(), fog_b=r.byte(), fog_near=r.float(), fog_far=r.float(),
        map_tex=r.short(), outdoors=r.byte(),
        ambient_r=r.byte(), ambient_g=r.byte(), ambient_b=r.byte(),
        light_pitch=r.float(), light_yaw=r.float(), slope_restrict=r.float())
    n = r.short()
    a['scenery'] = [dict(mesh=r.short(), x=r.float(), y=r.float(), z=r.float(),
                         pitch=r.float(), yaw=r.float(), roll=r.float(),
                         sx=r.float(), sy=r.float(), sz=r.float(),
                         anim_mode=r.byte(), scenery_id=r.byte(), texture=r.short(),
                         catch_rain=r.byte(), entity_type=r.byte(),
                         lightmap=r.string(), rcte=r.string(),
                         cast_shadow=r.byte(), receive_shadow=r.byte(), render_range=r.byte())
                    for _ in range(n)]
    n = r.short()
    a['water'] = [dict(tex=r.short(), tex_scale=r.float(), x=r.float(), y=r.float(), z=r.float(),
                       sx=r.float(), sz=r.float(), red=r.byte(), green=r.byte(),
                       blue=r.byte(), opacity=r.byte()) for _ in range(n)]
    n = r.short()
    a['colboxes'] = [dict(x=r.float(), y=r.float(), z=r.float(), pitch=r.float(),
                          yaw=r.float(), roll=r.float(), sx=r.float(), sy=r.float(),
                          sz=r.float()) for _ in range(n)]
    n = r.short()
    a['emitters'] = [dict(config=r.string(), tex=r.short(), x=r.float(), y=r.float(),
                          z=r.float(), pitch=r.float(), yaw=r.float(), roll=r.float())
                     for _ in range(n)]
    n = r.short()
    terrains = []
    for _ in range(n):
        t = dict(base_tex=r.short(), detail_tex=r.short())
        size = r.int(); t['size'] = size
        t['heights'] = [r.float() for _ in range((size + 1) * (size + 1))]
        t.update(x=r.float(), y=r.float(), z=r.float(), pitch=r.float(), yaw=r.float(),
                 roll=r.float(), sx=r.float(), sy=r.float(), sz=r.float(),
                 detail_tex_scale=r.float(), detail=r.int(), morph=r.byte(), shading=r.byte())
        terrains.append(t)
    a['terrains'] = terrains
    n = r.short()
    a['sound_zones'] = [dict(x=r.float(), y=r.float(), z=r.float(), radius=r.float(),
                             sound=r.short(), music=r.short(), repeat_time=r.int(),
                             volume=r.byte()) for _ in range(n)]
    return a

def write_client_area(a):
    w = Writer()
    for k in ['loading_tex','loading_music','sky_tex','cloud_tex','storm_cloud_tex','stars_tex']:
        w.short(a[k])
    w.byte(a['fog_r']); w.byte(a['fog_g']); w.byte(a['fog_b'])
    w.float(a['fog_near']); w.float(a['fog_far'])
    w.short(a['map_tex']); w.byte(a['outdoors'])
    w.byte(a['ambient_r']); w.byte(a['ambient_g']); w.byte(a['ambient_b'])
    w.float(a['light_pitch']); w.float(a['light_yaw']); w.float(a['slope_restrict'])
    w.short(len(a['scenery']))
    for s in a['scenery']:
        w.short(s['mesh']); w.float(s['x']); w.float(s['y']); w.float(s['z'])
        w.float(s['pitch']); w.float(s['yaw']); w.float(s['roll'])
        w.float(s['sx']); w.float(s['sy']); w.float(s['sz'])
        w.byte(s['anim_mode']); w.byte(s['scenery_id']); w.short(s['texture'])
        w.byte(s['catch_rain']); w.byte(s['entity_type'])
        w.string(s['lightmap']); w.string(s['rcte'])
        w.byte(s['cast_shadow']); w.byte(s['receive_shadow']); w.byte(s['render_range'])
    w.short(len(a['water']))
    for wt in a['water']:
        w.short(wt['tex']); w.float(wt['tex_scale']); w.float(wt['x']); w.float(wt['y'])
        w.float(wt['z']); w.float(wt['sx']); w.float(wt['sz'])
        w.byte(wt['red']); w.byte(wt['green']); w.byte(wt['blue']); w.byte(wt['opacity'])
    w.short(len(a['colboxes']))
    for c in a['colboxes']:
        w.float(c['x']); w.float(c['y']); w.float(c['z'])
        w.float(c['pitch']); w.float(c['yaw']); w.float(c['roll'])
        w.float(c['sx']); w.float(c['sy']); w.float(c['sz'])
    w.short(len(a['emitters']))
    for e in a['emitters']:
        w.string(e['config']); w.short(e['tex']); w.float(e['x']); w.float(e['y'])
        w.float(e['z']); w.float(e['pitch']); w.float(e['yaw']); w.float(e['roll'])
    w.short(len(a['terrains']))
    for t in a['terrains']:
        w.short(t['base_tex']); w.short(t['detail_tex']); w.int(t['size'])
        for h in t['heights']: w.float(h)
        w.float(t['x']); w.float(t['y']); w.float(t['z'])
        w.float(t['pitch']); w.float(t['yaw']); w.float(t['roll'])
        w.float(t['sx']); w.float(t['sy']); w.float(t['sz'])
        w.float(t['detail_tex_scale']); w.int(t['detail']); w.byte(t['morph']); w.byte(t['shading'])
    w.short(len(a['sound_zones']))
    for sz in a['sound_zones']:
        w.float(sz['x']); w.float(sz['y']); w.float(sz['z']); w.float(sz['radius'])
        w.short(sz['sound']); w.short(sz['music']); w.int(sz['repeat_time']); w.byte(sz['volume'])
    return w.getvalue()


# ---------- Media databases (Meshes / Textures / Sounds) ----------

INDEX_SLOTS = 65535
INDEX_BYTES = INDEX_SLOTS * 4

TEXTURE = 'texture'
MESH = 'mesh'
SOUND = 'sound'
# Music.dat shares the index+blob shape but its record is the bare filename
# string -- AddMusicToDatabase (Media.bb:498) writes only WriteString, no
# leading flags/is3D byte like the other three databases.
MUSIC = 'music'

def _read_record(r, kind):
    if kind == TEXTURE:
        return dict(flags=r.short(), name=r.string())
    if kind == MESH:
        return dict(is_anim=r.byte(), scale=r.float(), x=r.float(), y=r.float(),
                    z=r.float(), shader=r.short(), name=r.string())
    if kind == SOUND:
        return dict(is_3d=r.byte(), name=r.string())
    if kind == MUSIC:
        return dict(name=r.string())
    raise ValueError(kind)

def _write_record(kind, e):
    w = Writer()
    if kind == TEXTURE:
        w.short(e['flags']); w.string(e['name'])
    elif kind == MESH:
        w.byte(e['is_anim']); w.float(e['scale']); w.float(e['x']); w.float(e['y'])
        w.float(e['z']); w.short(e['shader']); w.string(e['name'])
    elif kind == SOUND:
        w.byte(e['is_3d']); w.string(e['name'])
    elif kind == MUSIC:
        w.string(e['name'])
    else:
        raise ValueError(kind)
    return w.getvalue()


class MediaDB:
    """Index-and-blob model of a Meshes/Textures/Sounds .dat.

    Faithful to the engine: a 65535-slot int index (slot[ID] = byte offset of
    that ID's record, 0 = empty) followed by variable-length records appended
    in INSERTION order. Mutation = append a record at EOF + patch one index
    slot, exactly mirroring Add*ToDatabase. Existing bytes are never moved, so
    .save() of an unmodified DB is byte-identical to what was loaded."""

    def __init__(self, raw: bytes, kind: str):
        self.kind = kind
        r = Reader(raw)
        self.index = [r.int() for _ in range(INDEX_SLOTS)]
        # records blob is everything after the index, preserved verbatim
        self.blob = bytearray(raw[INDEX_BYTES:])

    def entries(self):
        """Decode all populated slots into {id: dict}."""
        out = {}
        for idx, off in enumerate(self.index):
            if off > 0:
                r = Reader(bytes(self._index_view()) )
                r.seek(off)
                out[idx] = _read_record(r, self.kind)
        return out

    def _index_view(self):
        # reconstruct a full-file view (index + blob) for offset-based reads
        head = b''.join(struct.pack('<i', o) for o in self.index)
        return head + bytes(self.blob)

    def get(self, idx):
        off = self.index[idx]
        if off <= 0:
            return None
        r = Reader(self._index_view()); r.seek(off)
        return _read_record(r, self.kind)

    def find_by_name(self, name, ci=True):
        """Return the ID whose record name matches, or None."""
        target = name.upper() if ci else name
        for idx, e in self.entries().items():
            n = e['name'].upper() if ci else e['name']
            if n == target:
                return idx
        return None

    def first_free_id(self):
        used = {i for i, o in enumerate(self.index) if o > 0}
        nid = 0
        while nid in used:
            nid += 1
        return nid

    def add(self, entry):
        """Append a record at EOF, assign the lowest free ID. Returns the ID."""
        nid = self.first_free_id()
        off = INDEX_BYTES + len(self.blob)
        self.blob += _write_record(self.kind, entry)
        self.index[nid] = off
        return nid

    def add_file(self, name, **kw):
        """Convenience: register an asset filename with type-appropriate
        defaults. kw overrides (flags, is_anim, is_3d, scale, x, y, z, shader)."""
        if self.kind == TEXTURE:
            e = dict(flags=kw.get('flags', 0), name=name)
        elif self.kind == MESH:
            # shader sentinel is 0xFFFF; the codec reads/writes it signed, so -1
            # (== 0xFFFF unsigned) matches the bytes the engine's WriteShort emits.
            e = dict(is_anim=kw.get('is_anim', 0), scale=kw.get('scale', 1.0),
                     x=kw.get('x', 0.0), y=kw.get('y', 0.0), z=kw.get('z', 0.0),
                     shader=kw.get('shader', -1), name=name)
        elif self.kind == SOUND:
            e = dict(is_3d=kw.get('is_3d', 0), name=name)
        return self.add(e)

    def save(self):
        head = b''.join(struct.pack('<i', o) for o in self.index)
        return head + bytes(self.blob)


# back-compat thin wrappers for introspection
def read_textures(data): return MediaDB(data, TEXTURE).entries()
def read_meshes(data):   return MediaDB(data, MESH).entries()
def read_sounds(data):   return MediaDB(data, SOUND).entries()


# ---------- media DB <-> JSON-able object (rcproject phase 2) ----------
#
# Byte-faithful round-trip form for the index+blob databases. The .dat is NOT
# a value codec: record bytes live at the offsets the index points to, in
# insertion order. The JSON form therefore captures three things: the decoded
# records (sparse id -> fields map, the part humans diff and merge), the
# insertion ORDER (ids sorted by blob offset -- appends are strictly
# increasing, so order reconstructs every offset), and any GAP spans
# (hex-encoded, keyed by the id whose record follows them; trailing gaps use
# before=None). Rebuilding replays order/gaps and re-derives each index slot.
#
# Engine fact (verified against Media.bb:120/186/242/298): Remove*FromDatabase
# does NOT leave dead spans -- every Remove* rereads the survivors and
# rewrites the file compacted in ascending-ID order via CreateDatabase, and
# the Add* duplicate-scan walks the blob sequentially, so a gapped file is
# not a state the engine itself produces or can safely append to. The gap
# mechanism here is a strict fidelity superset: it preserves the bytes of
# crash-interrupted, legacy, or hand-edited files instead of corrupting them.
# Corollary for diff readers: an engine-side DELETE rewrites the whole blob
# (order + every offset change), so it shows as a large JSON diff, not a
# one-slot change.

def mediadb_to_obj(raw, kind):
    db = MediaDB(raw, kind)
    full = db._index_view()
    # Decode every populated slot and measure each record's byte length via
    # the reader's position delta (needed for gap detection).
    spans = []  # (offset, end, id)
    entries = {}
    for idx, off in enumerate(db.index):
        if off > 0:
            r = Reader(full)
            r.seek(off)
            entries[str(idx)] = _read_record(r, kind)
            spans.append((off, r.p, idx))
    spans.sort()
    order = [idx for (_, _, idx) in spans]
    # Sanity: the engine appends records back-to-back; overlapping spans mean
    # a misdecoded record and would silently corrupt the rebuild.
    gaps = []
    pos = INDEX_BYTES
    for off, end, idx in spans:
        if off < pos:
            raise ValueError(f"overlapping record for id {idx} at {off} (expected >= {pos})")
        if off > pos:
            gaps.append(dict(before=idx, hex=full[pos:off].hex()))
        pos = end
    if pos < len(full):
        gaps.append(dict(before=None, hex=full[pos:].hex()))
    return dict(kind=kind, entries=entries, order=order, gaps=gaps)


def obj_to_mediadb(obj):
    kind = obj['kind']
    entries = obj['entries']
    order = list(obj['order'])
    gaps = list(obj.get('gaps', []))
    # Index ids may arrive as JSON string keys; order entries as ints.
    index = [0] * INDEX_SLOTS
    blob = bytearray()
    def emit_gaps(marker):
        nonlocal blob
        for g in gaps:
            if g['before'] == marker:
                blob += bytes.fromhex(g['hex'])
    seen = set()
    for idx in order:
        idx = int(idx)
        if idx in seen:
            raise ValueError(f"duplicate id {idx} in order")
        seen.add(idx)
        emit_gaps(idx)
        rec = entries[str(idx)]
        index[idx] = INDEX_BYTES + len(blob)
        blob += _write_record(kind, rec)
    emit_gaps(None)
    missing = set(entries.keys()) - {str(i) for i in seen}
    if missing:
        raise ValueError(f"entries not present in order: {sorted(missing)}")
    head = b''.join(struct.pack('<i', o) for o in index)
    return head + bytes(blob)

# ---------- simple string-list files (Damage.dat) ----------

def read_string_list(data, count):
    r = Reader(data)
    return [r.string() for _ in range(count)]
