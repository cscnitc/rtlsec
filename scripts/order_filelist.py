#!/usr/bin/env python3
"""Topologically order a Verilator file list.

Verilator 4.x requires a package to be compiled before anything that imports
it, and the package graph is not alphabetical (`entropy_src_pkg` imports
`prim_util_pkg`, but sorts before it). This orders `*_pkg.sv` files by their
own `import`/`::` references, then appends every non-package file.
"""
import os
import re
import sys


def symbol(path: str) -> str:
    return os.path.basename(path)[:-3]


def main() -> int:
    src = sys.argv[1] if len(sys.argv) > 1 else "-"
    files = [l.strip() for l in (sys.stdin if src == "-" else open(src)) if l.strip()]
    pkgs = [f for f in files if f.endswith("_pkg.sv") or f.endswith("_pkg.svh")]
    mods = [f for f in files if f not in pkgs]

    by_sym = {symbol(f): f for f in pkgs}
    deps = {}
    for f in pkgs:
        try:
            text = open(f, errors="ignore").read()
        except OSError:
            deps[f] = set()
            continue
        refs = set(re.findall(r"\b([A-Za-z_]\w*)\s*::", text))
        refs |= set(re.findall(r"`?import\s+([A-Za-z_]\w*)", text))
        refs |= set(re.findall(r"include\s+\"([A-Za-z_]\w*_pkg)", text))
        deps[f] = {r for r in refs if r.endswith("_pkg")}

    out: list[str] = []
    state: dict[str, int] = {}  # 0 = visiting, 1 = done

    def visit(f: str) -> None:
        if state.get(f) == 1 or state.get(f) == 0:
            return
        state[f] = 0
        for r in sorted(deps.get(f, ())):
            g = by_sym.get(r)
            if g:
                visit(g)
        state[f] = 1
        out.append(f)

    for f in pkgs:
        visit(f)

    missing = [k for k, v in deps.items() if v - set(by_sym)]
    for line in out + mods:
        print(line)
    if missing:
        sys.stderr.write(f"order: {len(missing)} package files reference absent packages\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
