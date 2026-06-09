#!/usr/bin/env python3
"""rcproject -- git-friendly project format (Issue #32, phase 1).

Projects store content as opaque binary `.dat` files, so two authors editing the
same catalog produce an unmergeable binary conflict. This tool round-trips the
gameplay `.dat` files to/from pretty-printed, key-sorted JSON so they diff and
merge cleanly in git; the `.dat` stays the runtime / publish ("obfuscation")
form, exactly as Issue #32 proposes.

  rcproject export <project_dir> <text_dir>   # .dat -> .json   (decode for git)
  rcproject build  <text_dir> <project_dir>   # .json -> .dat   (publish)
  rcproject verify <project_dir>              # in-memory round-trip byte-check

Built on the byte-faithful codec in `rcdata.py` (the same one `validate.py`
proves). `verify` is the safety gate: it decodes every supported file, serialises
it to JSON text, parses it back, re-encodes, and asserts the bytes are identical
to the original -- so `export` then `build` is provably lossless before anyone
trusts it.

Scope (phase 1): the symmetric value-codec formats only -- Spells, Items, Actors,
Projectiles, Factions, server-side Areas (gameplay) and client-side Areas
(visual). The Meshes/Textures/Sounds media databases are append-only index+blob
structures (not value codecs -- a rebuild from their decoded entries would lose
insertion order / gap layout and change the bytes), so a git-friendly form for
them is deferred to phase 2. Unrecognised `.dat` files are reported and left
untouched, never silently mangled.
"""
import os
import sys
import json
import glob

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import rcdata

# codec key -> (decode bytes->obj, encode obj->bytes)
CODECS = {
    'spells':      (rcdata.read_spells,      rcdata.write_spells),
    'items':       (rcdata.read_items,       rcdata.write_items),
    'actors':      (rcdata.read_actors,      rcdata.write_actors),
    'projectiles': (rcdata.read_projectiles, rcdata.write_projectiles),
    'factions':    (rcdata.read_factions,    rcdata.write_factions),
    'server_area': (rcdata.read_server_area, rcdata.write_server_area),
    'client_area': (rcdata.read_client_area, rcdata.write_client_area),
}

# Exact project-relative paths (forward slashes) for the single-file catalogs.
_EXACT = {
    'Server Data/Spells.dat':      'spells',
    'Server Data/Items.dat':       'items',
    'Server Data/Actors.dat':      'actors',
    'Server Data/Projectiles.dat': 'projectiles',
    'Server Data/Factions.dat':    'factions',
}


def classify(rel):
    """Map a project-relative `.dat` path (forward slashes) to a codec key, or
    None if it is not a supported format. Mirrors validate.py's known-good set:
    gameplay areas live DIRECTLY under "Server Data/Areas" (not the Ownerships
    subfolder, a different format); visual areas live directly under "Areas",
    where `ha.dat` is a legacy pre-shadow-fields stub the codec can't read."""
    if rel in _EXACT:
        return _EXACT[rel]
    parts = rel.split('/')
    base = parts[-1].lower()
    parent = '/'.join(parts[:-1])
    if not base.endswith('.dat'):
        return None
    if parent == 'Server Data/Areas':
        return 'server_area'
    if parent == 'Areas' and base != 'ha.dat':
        return 'client_area'
    return None


def discover(project_dir):
    """Yield (relpath, codec_key) for every supported `.dat` under project_dir,
    sorted for deterministic output."""
    found = []
    for p in glob.glob(os.path.join(project_dir, '**', '*.dat'), recursive=True):
        rel = os.path.relpath(p, project_dir).replace(os.sep, '/')
        key = classify(rel)
        if key:
            found.append((rel, key))
    return sorted(found)


def _dumps(obj):
    """Deterministic JSON: sorted keys + stable indent + ascii-escaped, so the
    latin-1 high bytes the codec uses serialise reproducibly and survive the
    round-trip through `write_*` (which re-encodes via latin-1)."""
    return json.dumps(obj, sort_keys=True, indent=2, ensure_ascii=True) + '\n'


def _atomic_write(path, data):
    """Temp-file + os.replace, per the projectgen atomic-write discipline -- a
    failed/partial encode never clobbers an existing file."""
    os.makedirs(os.path.dirname(path) or '.', exist_ok=True)
    tmp = path + '.tmp'
    with open(tmp, 'wb') as f:
        f.write(data)
    os.replace(tmp, path)


def cmd_export(project_dir, text_dir):
    files = discover(project_dir)
    for rel, key in files:
        read_fn, _ = CODECS[key]
        with open(os.path.join(project_dir, rel), 'rb') as f:
            raw = f.read()
        obj = read_fn(raw)
        out = os.path.join(text_dir, rel + '.json')
        _atomic_write(out, _dumps(obj).encode('utf-8'))
        print(f"  export {rel}  ({key})")
    print(f"Exported {len(files)} file(s) to {text_dir}.")
    return 0


def cmd_build(text_dir, project_dir):
    n = 0
    for p in sorted(glob.glob(os.path.join(text_dir, '**', '*.dat.json'), recursive=True)):
        rel_json = os.path.relpath(p, text_dir).replace(os.sep, '/')
        rel = rel_json[:-len('.json')]
        key = classify(rel)
        if not key:
            print(f"  skip   {rel_json} (unrecognised)")
            continue
        _, write_fn = CODECS[key]
        with open(p, 'r', encoding='utf-8') as f:
            obj = json.load(f)
        _atomic_write(os.path.join(project_dir, rel), write_fn(obj))
        print(f"  build  {rel}  ({key})")
        n += 1
    print(f"Built {n} .dat file(s) into {project_dir}.")
    return 0


def cmd_verify(project_dir):
    files = discover(project_dir)
    ok = True
    for rel, key in files:
        read_fn, write_fn = CODECS[key]
        with open(os.path.join(project_dir, rel), 'rb') as f:
            raw = f.read()
        try:
            obj = read_fn(raw)
            text = _dumps(obj)                      # decode -> JSON text
            rebuilt = write_fn(json.loads(text))    # parse -> encode
            text2 = _dumps(read_fn(raw))            # determinism check
        except Exception as e:                      # noqa: BLE001 -- report, don't crash the sweep
            print(f"[FAIL] {rel} ({key}): {e!r}")
            ok = False
            continue
        match = rebuilt == raw
        det = text == text2
        if not (match and det):
            ok = False
        notes = ''
        if not match:
            notes += f' [bytes differ: {len(raw)} in, {len(rebuilt)} out]'
        if not det:
            notes += ' [non-deterministic JSON]'
        print(f"[{'PASS' if (match and det) else 'FAIL'}] {rel} ({key}){notes}")
    print()
    print(f"{'ALL' if ok else 'SOME'} round-trips {'passed' if ok else 'FAILED'} "
          f"({len(files)} file(s)).")
    return 0 if ok else 1


USAGE = ("usage:\n"
         "  rcproject export <project_dir> <text_dir>   # .dat -> .json\n"
         "  rcproject build  <text_dir> <project_dir>   # .json -> .dat\n"
         "  rcproject verify <project_dir>              # round-trip byte-check")


def main(argv):
    if len(argv) >= 2 and argv[1] == 'export' and len(argv) == 4:
        return cmd_export(argv[2], argv[3])
    if len(argv) >= 2 and argv[1] == 'build' and len(argv) == 4:
        return cmd_build(argv[2], argv[3])
    if len(argv) >= 2 and argv[1] == 'verify' and len(argv) == 3:
        return cmd_verify(argv[2])
    print(USAGE)
    return 2


if __name__ == '__main__':
    sys.exit(main(sys.argv))
