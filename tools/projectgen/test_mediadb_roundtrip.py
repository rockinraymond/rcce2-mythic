"""Acceptance tests for the phase-2 media-database round-trip
(Game Data/Meshes.dat, Textures.dat, Sounds.dat, Music.dat).

Written from the acceptance criteria + the public behavior only:
  - rcdata.mediadb_to_obj(raw, kind) / rcdata.obj_to_mediadb(obj)
  - the JSON object shape {kind, entries, order, gaps}
  - the engine-side format authority src/Modules/Media.bb
    (Add*ToDatabase write order; CreateDatabase's 65535*4 zero index)

Plain python, no pytest. Run:  python test_mediadb_roundtrip.py
Exit code 0 = all checks pass, 1 = any failure.
"""
import json
import os
import struct
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import rcdata

DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
GAME_DATA = os.path.join(DATA, 'Game Data')

INDEX_SLOTS = 65535
INDEX_BYTES = INDEX_SLOTS * 4   # 262140 -- matches Media.bb CreateDatabase

# (filename, rcdata kind) for the four shipped databases
SHIPPED = [
    ('Meshes.dat',   rcdata.MESH),
    ('Textures.dat', rcdata.TEXTURE),
    ('Sounds.dat',   rcdata.SOUND),
    ('Music.dat',    rcdata.MUSIC),
]


def load(name):
    with open(os.path.join(GAME_DATA, name), 'rb') as f:
        return f.read()


def dumps_like_cli(obj):
    """The exact JSON-text form rcproject.py exports (its _dumps)."""
    return json.dumps(obj, sort_keys=True, indent=2, ensure_ascii=True) + '\n'


def roundtrip(raw, kind):
    """decode -> JSON text -> parse -> encode, the full export/build path."""
    obj = rcdata.mediadb_to_obj(raw, kind)
    text = dumps_like_cli(obj)
    return rcdata.obj_to_mediadb(json.loads(text))


def first_diff(a, b):
    n = min(len(a), len(b))
    for i in range(n):
        if a[i] != b[i]:
            return i
    return n if len(a) != len(b) else -1


def rec_bytes(kind, **fields):
    """Build one record's bytes with rcdata.Writer in the exact field order
    Media.bb's Add*ToDatabase writes (the engine-side format authority)."""
    w = rcdata.Writer()
    if kind == rcdata.TEXTURE:
        # AddTextureToDatabase: WriteShort Flags / WriteString Filename$
        w.short(fields['flags'])
        w.string(fields['name'])
    elif kind == rcdata.MESH:
        # AddMeshToDatabase: WriteByte IsAnim / WriteFloat 1.0 / WriteFloat 0
        # x3 / WriteShort 65535 / WriteString Filename$
        w.byte(fields['is_anim'])
        w.float(fields['scale'])
        w.float(fields['x'])
        w.float(fields['y'])
        w.float(fields['z'])
        # Blitz "WriteShort F, 65535" emits 0xFFFF; the codec models the
        # same bytes as signed -1, so accept either spelling here.
        sh = fields['shader']
        w.ushort(sh & 0xFFFF)
        w.string(fields['name'])
    elif kind == rcdata.SOUND:
        # AddSoundToDatabase: WriteByte Is3D / WriteString Filename$
        w.byte(fields['is_3d'])
        w.string(fields['name'])
    elif kind == rcdata.MUSIC:
        # AddMusicToDatabase: WriteString Filename$ only
        w.string(fields['name'])
    else:
        raise ValueError(kind)
    return w.getvalue()


def build_synthetic(kind, placements):
    """Construct a raw .dat: zeroed 65535*4 index + records appended in the
    given insertion order. placements = [(slot_id, record_bytes), ...].
    Returns (raw, {slot_id: offset})."""
    index = bytearray(INDEX_BYTES)
    blob = bytearray()
    offsets = {}
    for slot_id, rb in placements:
        off = INDEX_BYTES + len(blob)
        offsets[slot_id] = off
        struct.pack_into('<i', index, slot_id * 4, off)
        blob += rb
    return bytes(index) + bytes(blob), offsets


def zero_slot(raw, slot_id):
    """Zero an index slot, leaving the record bytes in place as a dead span.
    NOTE: this is NOT what the engine's Remove*FromDatabase does -- those
    compact via a full rewrite (Media.bb). A dead span models a
    crash-interrupted or hand-edited file, which the gap mechanism must
    preserve byte-faithfully rather than corrupt."""
    b = bytearray(raw)
    struct.pack_into('<i', b, slot_id * 4, 0)
    return bytes(b)


# ---------------------------------------------------------------- checks

def check_real_data_identity():
    """1. REAL-DATA BYTE IDENTITY: all four shipped Game Data files survive
    decode -> JSON text -> parse -> encode byte-for-byte."""
    for name, kind in SHIPPED:
        raw = load(name)
        out = roundtrip(raw, kind)
        assert out == raw, (
            "%s: round-trip not byte-identical (%d in, %d out, first diff @%d)"
            % (name, len(raw), len(out), first_diff(raw, out)))


def check_empty_db_music():
    """1b. EMPTY-DB CASE: shipped Music.dat is exactly the 262,140-byte
    all-zero index (no records), decodes to an empty object, and round-trips."""
    raw = load('Music.dat')
    assert len(raw) == INDEX_BYTES, \
        "Music.dat is %d bytes, expected %d" % (len(raw), INDEX_BYTES)
    assert raw == b'\x00' * INDEX_BYTES, "Music.dat is not all zero bytes"
    obj = rcdata.mediadb_to_obj(raw, rcdata.MUSIC)
    assert obj['entries'] == {}, "empty DB decoded non-empty entries"
    assert obj['order'] == [], "empty DB decoded non-empty order"
    assert roundtrip(raw, rcdata.MUSIC) == raw, "empty-DB round-trip differs"


def check_sparsity():
    """2. SPARSITY: Meshes.dat JSON must not serialize the 65,535-slot index
    densely. Compact JSON is under 10 KB for the 266 KB .dat; even the CLI's
    pretty-printed export stays far below a dense index's ~131 KB floor
    (65,535 list elements at >= 2 chars each)."""
    raw = load('Meshes.dat')
    assert len(raw) > 260000, "precondition: shipped Meshes.dat ~266 KB"
    obj = rcdata.mediadb_to_obj(raw, rcdata.MESH)
    compact = json.dumps(obj, sort_keys=True, separators=(',', ':'))
    pretty = dumps_like_cli(obj)
    assert len(compact) < 10240, \
        "compact JSON is %d bytes (>= 10 KB); index serialized densely?" % len(compact)
    assert len(pretty) < 65536, \
        "pretty JSON is %d bytes; index serialized densely?" % len(pretty)
    # the entries map must be sparse (only populated ids), not 65,535 keys
    assert len(obj['entries']) < 1000, \
        "entries map has %d keys; expected only populated slots" % len(obj['entries'])


def check_mid_gap_roundtrip():
    """3a. SYNTHETIC GAP: three records, the middle one's slot zeroed
    (a crash-interrupted / hand-edited state -- the engine's own Remove*
    compacts instead; see zero_slot note) leaving a dead span between
    live records.
    Round-trip must reproduce the dead bytes exactly."""
    placements = [
        (5,  rec_bytes(rcdata.SOUND, is_3d=1, name='Sounds\\step1.wav')),
        (17, rec_bytes(rcdata.SOUND, is_3d=0, name='Sounds\\ui_click.wav')),
        (9,  rec_bytes(rcdata.SOUND, is_3d=1, name='Sounds\\roar.wav')),
    ]
    raw, _ = build_synthetic(rcdata.SOUND, placements)
    dead = zero_slot(raw, 17)   # dead span between the id-5 and id-9 records
    obj = rcdata.mediadb_to_obj(dead, rcdata.SOUND)
    assert sorted(obj['entries'].keys()) == ['5', '9'], \
        "live entries wrong after mid-record deletion: %r" % sorted(obj['entries'])
    out = roundtrip(dead, rcdata.SOUND)
    assert out == dead, (
        "mid-gap round-trip differs (%d in, %d out, first diff @%d)"
        % (len(dead), len(out), first_diff(dead, out)))


def check_trailing_gap_roundtrip():
    """3b. SYNTHETIC TRAILING GAP: the LAST record's slot zeroed -- dead span
    at EOF with no live record after it. Round-trip must keep the trailing
    bytes (file length unchanged)."""
    placements = [
        (0, rec_bytes(rcdata.TEXTURE, flags=1, name='Textures\\grass.bmp')),
        (1, rec_bytes(rcdata.TEXTURE, flags=0, name='Textures\\rock.bmp')),
        (2, rec_bytes(rcdata.TEXTURE, flags=4, name='Textures\\water.png')),
    ]
    raw, _ = build_synthetic(rcdata.TEXTURE, placements)
    dead = zero_slot(raw, 2)    # trailing dead span
    obj = rcdata.mediadb_to_obj(dead, rcdata.TEXTURE)
    assert sorted(obj['entries'].keys()) == ['0', '1'], \
        "live entries wrong after trailing deletion: %r" % sorted(obj['entries'])
    out = roundtrip(dead, rcdata.TEXTURE)
    assert len(out) == len(dead), \
        "trailing dead span dropped: %d in, %d out" % (len(dead), len(out))
    assert out == dead, (
        "trailing-gap round-trip differs (first diff @%d)"
        % first_diff(dead, out))


def check_mutation_mesh():
    """4a. MUTATION VIA JSON (mesh): append an id to order + entries[str(id)]
    with the engine-default fields AddMeshToDatabase writes. The rebuilt .dat
    must equal the original with exactly one index slot patched to old-EOF
    and the Media.bb-ordered record bytes appended there."""
    raw = load('Meshes.dat')
    obj = rcdata.mediadb_to_obj(raw, rcdata.MESH)
    # first free id, the way AddMeshToDatabase scans for it
    used = set(int(k) for k in obj['entries'])
    nid = 0
    while nid in used:
        nid += 1
    # engine defaults: IsAnim arg, scale 1.0, offsets 0, shader 65535
    # (WriteShort -> the codec's signed spelling is -1), then the filename
    entry = dict(is_anim=1, scale=1.0, x=0.0, y=0.0, z=0.0, shader=-1,
                 name='Monsters\\TestWyrm\\wyrm.b3d')
    obj['order'].append(nid)
    obj['entries'][str(nid)] = entry
    out = rcdata.obj_to_mediadb(json.loads(dumps_like_cli(obj)))

    expected_rec = rec_bytes(rcdata.MESH, **entry)
    assert len(out) == len(raw) + len(expected_rec), \
        "size %d, expected %d" % (len(out), len(raw) + len(expected_rec))
    # the new record lands at old EOF (what FileSize() returns in Media.bb)
    slot_off = struct.unpack_from('<i', out, nid * 4)[0]
    assert slot_off == len(raw), \
        "index slot %d points at %d, expected old EOF %d" % (nid, slot_off, len(raw))
    assert out[slot_off:] == expected_rec, \
        "appended record bytes do not match Media.bb write order"
    # every pre-existing byte unchanged except the one patched index slot
    assert out[:nid * 4] == raw[:nid * 4], "bytes before patched slot changed"
    assert out[nid * 4 + 4:len(raw)] == raw[nid * 4 + 4:], \
        "pre-existing bytes after patched slot changed"


def check_mutation_music():
    """4b. MUTATION VIA JSON (music, from the empty DB): the record is the
    bare WriteString filename, landing at offset 262140 in slot 0."""
    raw = load('Music.dat')
    obj = rcdata.mediadb_to_obj(raw, rcdata.MUSIC)
    obj['order'].append(0)
    obj['entries']['0'] = dict(name='Music\\theme.ogg')
    out = rcdata.obj_to_mediadb(json.loads(dumps_like_cli(obj)))

    expected_rec = rec_bytes(rcdata.MUSIC, name='Music\\theme.ogg')
    assert struct.unpack_from('<i', out, 0)[0] == INDEX_BYTES, \
        "slot 0 should point at %d" % INDEX_BYTES
    assert out[INDEX_BYTES:] == expected_rec, \
        "music record is not the bare WriteString filename"
    assert out[4:INDEX_BYTES] == raw[4:INDEX_BYTES], "other index slots changed"
    assert len(out) == INDEX_BYTES + len(expected_rec)


def check_error_entry_not_in_order():
    """5a. ERROR: an id present in entries but missing from order must raise
    (its record could never be placed in the blob)."""
    raw, _ = build_synthetic(
        rcdata.SOUND, [(0, rec_bytes(rcdata.SOUND, is_3d=0, name='a.wav'))])
    obj = rcdata.mediadb_to_obj(raw, rcdata.SOUND)
    obj['entries']['7'] = dict(is_3d=0, name='orphan.wav')   # not in order
    try:
        rcdata.obj_to_mediadb(json.loads(dumps_like_cli(obj)))
    except Exception:
        return
    raise AssertionError("entry missing from order did not raise")


def check_error_duplicate_in_order():
    """5b. ERROR: a duplicate id in order must raise (one slot cannot point
    at two records)."""
    raw, _ = build_synthetic(
        rcdata.SOUND, [(0, rec_bytes(rcdata.SOUND, is_3d=0, name='a.wav')),
                       (1, rec_bytes(rcdata.SOUND, is_3d=1, name='b.wav'))])
    obj = rcdata.mediadb_to_obj(raw, rcdata.SOUND)
    obj['order'] = obj['order'] + [obj['order'][0]]          # duplicate id
    try:
        rcdata.obj_to_mediadb(json.loads(dumps_like_cli(obj)))
    except Exception:
        return
    raise AssertionError("duplicate id in order did not raise")


def check_cli_verify():
    """6. CLI INTEGRATION: `rcproject.py verify <data>` exits 0, reports all
    18 supported files passing, and covers the four Game Data databases."""
    proc = subprocess.run(
        [sys.executable, os.path.join(HERE, 'rcproject.py'), 'verify', DATA],
        capture_output=True, text=True, cwd=HERE)
    out = proc.stdout + proc.stderr
    assert proc.returncode == 0, \
        "verify exited %d:\n%s" % (proc.returncode, out)
    assert '[FAIL]' not in out, "verify printed a FAIL line:\n%s" % out
    assert 'ALL round-trips passed' in out, "missing pass summary:\n%s" % out
    assert '18 file(s)' in out, "expected 18 file(s) in summary:\n%s" % out
    for name in ('Game Data/Meshes.dat', 'Game Data/Textures.dat',
                 'Game Data/Sounds.dat', 'Game Data/Music.dat'):
        assert '[PASS] %s' % name in out, "%s not verified:\n%s" % (name, out)


CHECKS = [
    ('real-data byte identity (all 4 Game Data files)', check_real_data_identity),
    ('empty-DB case: Music.dat = 262,140 zero bytes',   check_empty_db_music),
    ('sparsity: Meshes.dat JSON is small + sparse',     check_sparsity),
    ('synthetic mid-record dead span round-trips',      check_mid_gap_roundtrip),
    ('synthetic trailing dead span round-trips',        check_trailing_gap_roundtrip),
    ('JSON mutation: mesh append matches Media.bb',     check_mutation_mesh),
    ('JSON mutation: music append (bare string rec)',   check_mutation_music),
    ('error: entry missing from order raises',          check_error_entry_not_in_order),
    ('error: duplicate id in order raises',             check_error_duplicate_in_order),
    ('CLI: rcproject verify passes 18 files incl. 4 media DBs', check_cli_verify),
]


def main():
    failures = 0
    for i, (label, fn) in enumerate(CHECKS, 1):
        try:
            fn()
        except AssertionError as e:
            print("[FAIL] %2d. %s" % (i, label))
            print("        %s" % e)
            failures += 1
        except Exception as e:  # unexpected blow-up is also a failure
            print("[FAIL] %2d. %s" % (i, label))
            print("        unexpected %s: %s" % (type(e).__name__, e))
            failures += 1
        else:
            print("[PASS] %2d. %s" % (i, label))
    print()
    total = len(CHECKS)
    if failures:
        print("%d/%d checks FAILED." % (failures, total))
        return 1
    print("All %d checks passed." % total)
    return 0


if __name__ == '__main__':
    sys.exit(main())
