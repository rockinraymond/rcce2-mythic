"""Audit that every Items.dat entry is obtainable in normal play — i.e. some content
script grants it via GiveItem. Resolves BOTH literal GiveItem(..,"Name",..) and the
quest-reward pattern (RewardItem$ = "Name" ... GiveItem(.., RewardItem, ..)), so quest
rewards aren't false-flagged as stranded.

Debug-only sources (In-game Commands /itempack, Spawn_Test) are reported separately:
an item ONLY granted there is "debug-only" (reachable by an admin but not a normal player).

Read-only. Intended as a content-consistency check, not a hard gate (some items may be
intentionally debug-only / vestigial). Prints a summary; exits 0 always.
"""
import os, re, sys
import rcdata

HERE = os.path.dirname(__file__)
DATA = os.path.normpath(os.path.join(HERE, '..', '..', 'data'))
SD = os.path.join(DATA, 'Server Data')
SCRIPTS = os.path.join(SD, 'Scripts')

DEBUG_SOURCES = {'In-game Commands', 'Spawn_Test'}
# Items intentionally not in the normal-play loop (superseded basic gear); documented
# so the audit's expected state is explicit.
INTENTIONAL_DEBUG_ONLY = {'Sword', 'Shield'}

def grants_in(text):
    """Return the set of item names a script grants, resolving simple string vars."""
    # string-var assignments: Foo$ = "Bar"  /  Foo = "Bar"
    vars = {}
    for m in re.finditer(r'(\w+)\$?\s*=\s*"([^"]*)"', text):
        vars[m.group(1).lower()] = m.group(2)
    names = set()
    for m in re.finditer(r'[Gg]ive[Ii]tem\s*\(\s*\w+\s*,\s*([^,)]+)', text):
        arg = m.group(1).strip()
        if arg.startswith('"') and arg.endswith('"'):
            names.add(arg[1:-1])
        else:
            v = vars.get(arg.rstrip('$').lower())
            if v is not None:
                names.add(v)
    return names

def main():
    items = rcdata.read_items(open(os.path.join(SD, 'Items.dat'), 'rb').read())
    granted = {}   # item name (lower) -> set of source script basenames
    for f in os.listdir(SCRIPTS):
        if not f.endswith('.rsl'):
            continue
        base = f[:-4]
        for nm in grants_in(open(os.path.join(SCRIPTS, f), encoding='latin-1').read()):
            granted.setdefault(nm.lower(), set()).add(base)

    unexpected = []
    print("=== item obtainability ===")
    for it in items:
        nm = it['name']
        srcs = granted.get(nm.lower(), set())
        normal = sorted(srcs - DEBUG_SOURCES)
        debug = sorted(srcs & DEBUG_SOURCES)
        if normal:
            print(f"  OK    {nm:<22} via {', '.join(normal)}")
        elif debug:
            note = ' (intentional)' if nm in INTENTIONAL_DEBUG_ONLY else ''
            print(f"  DEBUG {nm:<22} only via {', '.join(debug)}{note}")
            if nm not in INTENTIONAL_DEBUG_ONLY:
                unexpected.append(f"{nm} is debug-only (only {', '.join(debug)})")
        else:
            note = ' (intentional)' if nm in INTENTIONAL_DEBUG_ONLY else ''
            print(f"  NONE  {nm:<22} not granted anywhere{note}")
            if nm not in INTENTIONAL_DEBUG_ONLY:
                unexpected.append(f"{nm} is unobtainable")

    if unexpected:
        print(f"\n=== {len(unexpected)} unexpected gap(s) (not in the intentional debug-only set) ===")
        for u in unexpected:
            print("  " + u)
    else:
        print("\nOK: every gameplay item is obtainable in normal play "
              "(only the intentional debug-only basics are not).")
    return 0

if __name__ == '__main__':
    sys.exit(main())
