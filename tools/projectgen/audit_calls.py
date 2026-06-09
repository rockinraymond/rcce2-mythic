"""Runtime-correctness audit #2: flag function calls in our authored scripts that
are NOT known BVM commands, so each can be confirmed as a real RSL builtin vs a
silent typo. Cross-references the FULL BVM name set from bvm-reference.md plus the
set of calls used by SHIPPED scripts (known-good) plus RSL language builtins.
"""
import re, os, glob

HERE = os.path.dirname(__file__)
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
REF = os.path.join(ROOT, 'docs', 'bvm-reference.md')
SCRIPTS = os.path.join(ROOT, 'data', 'Server Data', 'Scripts')

# Scripts authored or edited during this project (the ones to scrutinize).
OURS = {
    'Spell_Heal', 'Spell_Regeneration', 'Spell_Meditation', 'Spell_FrostBolt',
    'Spell_Lightning', 'Item_HealthPotion', 'Item_ManaPotion', 'Quest_OrcRaiders',
    'Click_Merchant', 'Click_Trainer', 'MonsterLoot',
}
# RSL / Blitz language builtins + control keywords that look like calls.
BUILTINS = {
    'if', 'while', 'for', 'function', 'return', 'rand', 'doevents', 'int', 'float',
    'str', 'left', 'right', 'mid', 'len', 'upper', 'lower', 'chr', 'instr', 'abs',
    'using', 'each', 'select', 'case', 'repeat', 'until', 'wend', 'next', 'then',
}

def bvm_names():
    names = set()
    for line in open(REF, encoding='latin-1'):
        m = re.match(r'\|\s*`([A-Z0-9_]+)`\s*\|', line)
        if m:
            names.add(m.group(1).lower())
    return names

def calls_in(path):
    lines = [ln.split(';', 1)[0] for ln in open(path, encoding='latin-1').read().splitlines()]
    return set(c.lower() for c in re.findall(r'([A-Za-z_][A-Za-z0-9_]*)\s*\(', '\n'.join(lines)))

def main():
    bvm = bvm_names()
    print(f"{len(bvm)} BVM commands in reference")

    # known-good calls = everything the SHIPPED (not-ours) scripts use
    shipped_calls = set()
    for f in glob.glob(os.path.join(SCRIPTS, '*.rsl')):
        name = os.path.splitext(os.path.basename(f))[0]
        if name not in OURS:
            shipped_calls |= calls_in(f)

    known = bvm | BUILTINS | shipped_calls
    print("\n=== unknown calls in our scripts (verify each is a real builtin, not a typo) ===")
    any_unknown = False
    for f in sorted(glob.glob(os.path.join(SCRIPTS, '*.rsl'))):
        name = os.path.splitext(os.path.basename(f))[0]
        if name not in OURS:
            continue
        # local function defs in this file are fine
        local = set(m.lower() for m in re.findall(r'(?im)^\s*Function\s+([A-Za-z_][A-Za-z0-9_]*)', open(f, encoding='latin-1').read()))
        unknown = sorted(c for c in calls_in(f) if c not in known and c not in local)
        if unknown:
            any_unknown = True
            print(f"  {name}: {', '.join(unknown)}")
    if not any_unknown:
        print("  (none — every call resolves to a BVM, a shipped-script call, or a builtin)")
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
