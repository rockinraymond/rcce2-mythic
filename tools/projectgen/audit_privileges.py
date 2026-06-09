"""Runtime-correctness audit: find content scripts that call privileged BVMs but
are NOT on the privileged allowlist -> those calls silently no-op at runtime.

Privileged BVM set is parsed from docs/bvm-reference.md (gate column == Privileged).
This is heuristic (regex over `Func(` calls; ignores comments only loosely) but good
enough to flag the silently-broken-script class found in iter 17.
"""
import re, os, glob

HERE = os.path.dirname(__file__)
ROOT = os.path.normpath(os.path.join(HERE, '..', '..'))
REF = os.path.join(ROOT, 'docs', 'bvm-reference.md')
ALLOW = os.path.join(ROOT, 'data', 'Server Data', 'Privileged Scripts.dat')
SCRIPTS = os.path.join(ROOT, 'data', 'Server Data', 'Scripts')

def main():
    priv = set()
    for line in open(REF, encoding='latin-1'):
        m = re.match(r'\|\s*`([A-Z0-9_]+)`\s*\|.*\|\s*Privileged\s*\|', line)
        if m:
            priv.add(m.group(1).lower())
    print(f"{len(priv)} privileged BVMs in reference")

    allow = set()
    for ln in open(ALLOW, encoding='latin-1').read().splitlines():
        s = ln.strip()
        if s and not s.startswith(';'):
            allow.add(s.lower())

    issues = []
    for f in sorted(glob.glob(os.path.join(SCRIPTS, '*.rsl'))):
        name = os.path.splitext(os.path.basename(f))[0]
        # strip ';' line comments before scanning for calls
        lines = []
        for ln in open(f, encoding='latin-1').read().splitlines():
            lines.append(ln.split(';', 1)[0])
        txt = '\n'.join(lines)
        calls = set(c.lower() for c in re.findall(r'([A-Za-z_][A-Za-z0-9_]*)\s*\(', txt))
        used = sorted(c for c in calls if c in priv)
        if used and name.lower() not in allow:
            issues.append((name, used))

    print(f"\n=== scripts calling privileged BVMs but NOT allowlisted ({len(issues)}) ===")
    for n, p in issues:
        print(f"  {n}: {', '.join(p)}")
    if not issues:
        print("  (none — every privileged-BVM caller is allowlisted)")
    return 0

if __name__ == '__main__':
    raise SystemExit(main())
