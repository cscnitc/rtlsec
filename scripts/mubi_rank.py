#!/usr/bin/env python3
"""mubi_rank.py — rank MuBi loose sites by the strength of the contradiction.

A loose call is not automatically a defect (the package documents legitimate
uses). The defect signature is a loose CONSUMER whose siblings -- same file, same
naming role such as _pfe / _pfa / _fo -- use the STRICT test on the same kind of
fanout. Same role, same interface, different rigour: that divergence is the bug.

Score:
  3  loose consumer with >=1 strict sibling of the SAME role suffix in the file
  2  loose consumer with strict siblings elsewhere in the file (role differs)
  1  loose site, no strict sibling in the file
"""
import os
import re
import sys
from collections import defaultdict

LOOSE = re.compile(r"mubi\d*_test_(true|false)_loose")
STRICT = re.compile(r"mubi\d*_test_(true|false)_strict")
ROLE = re.compile(r"\b([A-Za-z_0-9]+)_(pf[ea]|fo)\b")


def roles(text):
    """Role SUFFIX only (_pfe/_pfa/_fo), so a loose *_pfe intersects a strict
    *_pfe in the same file regardless of the signal's name."""
    return set(m.group(2) for m in ROLE.finditer(text))


def main():
    tree = sys.argv[1]
    files = defaultdict(lambda: {"loose": [], "strict": []})
    for root, _, names in os.walk(tree):
        if any(x in root for x in ("/dv/", "/pre_dv/", "/vendor/")):
            continue
        for fn in names:
            if not fn.endswith(".sv"):
                continue
            p = os.path.join(root, fn)
            try:
                lines = open(p, errors="replace").read().split("\n")
            except OSError:
                continue
            for i, ln in enumerate(lines, 1):
                if "function" in ln or ln.strip().startswith(("import", "export")):
                    continue
                if LOOSE.search(ln):
                    files[p]["loose"].append((i, ln.strip()))
                elif STRICT.search(ln):
                    files[p]["strict"].append((i, ln.strip()))

    rows = []
    for f, v in files.items():
        if not v["loose"]:
            continue
        strict_roles = set()
        for _, t in v["strict"]:
            strict_roles |= roles(t)
        for ln, t in v["loose"]:
            lroles = roles(t)
            if lroles & strict_roles:
                score = 3
            elif v["strict"]:
                score = 2
            else:
                score = 1
            rows.append((score, f.replace(tree + "/", ""), ln, t, sorted(lroles & strict_roles)))

    rows.sort(key=lambda r: (-r[0], r[1], r[2]))
    print(f"=== MuBi loose sites ranked by contradiction strength ({len(rows)}) ===")
    for score, f, ln, t, shared in rows:
        if score < 3 and len(sys.argv) < 3:
            continue
        tag = {3: "SAME-ROLE-STRICT-SIBLING", 2: "strict-elsewhere", 1: "no-strict-sibling"}[score]
        print(f"\n[{score}] {tag}")
        print(f"    {f}:{ln}")
        print(f"    {t[:120]}")
        if shared:
            print(f"    role(s) {shared} also tested STRICTLY in this file")


if __name__ == "__main__":
    main()
