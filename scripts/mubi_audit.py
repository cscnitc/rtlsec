#!/usr/bin/env python3
"""mubi_audit.py — find MuBi strict/loose divergences across an RTL tree.

The encoding contract is the vendor's own, defined in prim_mubi_pkg:
  mubi4_test_true_strict(v) = (MuBi4True  == v)   -> only the canonical value is "true"
  mubi4_test_true_loose(v)  = (MuBi4False != v)   -> EVERY value except canonical false is "true"
  mubi4_test_false_loose(v) = (MuBi4True  != v)   -> EVERY value except canonical true is "false"

A "loose" test therefore treats any invalid encoding as a decisive value, i.e. it
fails open on corruption. The defect is not the existence of the function (it is
legitimate for some uses) but a LOOSE consumer sitting among STRICT siblings on
the same kind of interface -- the same fanout, the same role, different rigour.
That divergence is what the contract check reports.

Usage: mubi_audit.py <tree> [--only-flagged]
"""
import argparse
import os
import re
import sys
from collections import defaultdict

LOOSE = re.compile(r"mubi\d*_test_(true|false)_loose")
STRICT = re.compile(r"mubi\d*_test_(true|false)_strict")
# a consumer of a fanout, by OpenTitan convention
CONSUMER = re.compile(r"_pf[ea]\b|_fo\b|_pe\b")


def scan(tree):
    hits = []
    for root, _, files in os.walk(tree):
        if "/dv/" in root or "/pre_dv/" in root or "/vendor/" in root:
            continue
        for fn in files:
            if not fn.endswith(".sv"):
                continue
            p = os.path.join(root, fn)
            try:
                lines = open(p, errors="replace").read().split("\n")
            except OSError:
                continue
            for i, ln in enumerate(lines, 1):
                if LOOSE.search(ln) and "function" not in ln:
                    hits.append({
                        "file": p, "line": i, "text": ln.strip(),
                        "kind": "loose",
                        "consumer": bool(CONSUMER.search(ln)),
                    })
                elif STRICT.search(ln) and "function" not in ln:
                    hits.append({"file": p, "line": i, "text": ln.strip(),
                                 "kind": "strict", "consumer": False})
    return hits


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("tree")
    ap.add_argument("--only-flagged", action="store_true")
    a = ap.parse_args()

    hits = scan(a.tree)
    per_file = defaultdict(lambda: {"loose": [], "strict": []})
    for h in hits:
        per_file[h["file"]][h["kind"]].append(h)

    print(f"scanned {a.tree}")
    print(f"  loose call sites : {sum(len(v['loose']) for v in per_file.values())}")
    print(f"  strict call sites: {sum(len(v['strict']) for v in per_file.values())}")
    print()

    flagged = []
    for f, v in sorted(per_file.items()):
        if v["loose"] and v["strict"]:
            flagged.append((f, v))

    print(f"=== files mixing BOTH strict and loose on consumers: {len(flagged)} ===")
    for f, v in flagged:
        rel = f.replace(a.tree + "/", "")
        nl, ns = len(v["loose"]), len(v["strict"])
        print(f"\n  {rel}   ({nl} loose / {ns} strict)")
        for h in v["loose"]:
            print(f"    LOOSE  :{h['line']}  {h['text'][:110]}")
        if not a.only_flagged:
            for h in v["strict"][:3]:
                print(f"    strict :{h['line']}  {h['text'][:110]}")

    print()
    print("=== files with ONLY loose consumer tests (no strict sibling) ===")
    n_only = 0
    for f, v in sorted(per_file.items()):
        if v["loose"] and not v["strict"]:
            rel = f.replace(a.tree + "/", "")
            print(f"  {rel}: {len(v['loose'])} loose site(s)")
            for h in v["loose"]:
                print(f"      :{h['line']}  {h['text'][:100]}")
            n_only += 1
    if not n_only:
        print("  (none)")


if __name__ == "__main__":
    main()
