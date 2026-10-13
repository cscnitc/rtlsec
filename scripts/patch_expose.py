#!/usr/bin/env python3
"""patch_expose.py — expose internal signals of an RTL module to the C++ driver.

Usage:
  patch_expose.py <rtl.sv> <out.sv> <signal> [<signal> ...]

Adds a per-signal `/*verilator public_flat_rw*/` metacomment to each named
signal's declaration, producing a patched COPY of the file. The read-only RTL
tree is never modified.

Why metacomments rather than the `--public-flat-rw` command-line flag: on
Verilator 4.210 the global flag makes the generated __Syms.cpp reference
package variables that do not exist, so the model fails to compile. Per-signal
metacomments expose exactly what is needed and elide that code path.

Only declaration lines are touched. A line qualifies when it has no `=`,
`(`, `<`, `always`, `assign` or `initial`, so uses of the signal (assignments,
port connections, comparisons) are left alone. Signals are matched as whole
words, so `lock_q` never matches inside `lock_q2`.

Exits non-zero if any requested signal could not be exposed, so a caller can
never silently proceed with a harness that is missing a fault target.
"""
import re
import sys

MARK = "/*verilator public_flat_rw*/"


def is_decl_line(line: str) -> bool:
    s = line.strip()
    if not s.endswith(";"):
        return False
    for bad in ("=", "(", "<", "always", "assign", "initial", "generate", "endmodule"):
        if bad in s:
            return False
    return True


def main() -> int:
    if len(sys.argv) < 4:
        print(__doc__)
        return 2
    src, dst, signals = sys.argv[1], sys.argv[2], sys.argv[3:]
    with open(src) as fh:
        lines = fh.readlines()

    done = set()
    out = []
    for line in lines:
        if is_decl_line(line):
            for sig in signals:
                if sig in done:
                    continue
                # whole-word match only
                if re.search(r"\b%s\b" % re.escape(sig), line):
                    line = re.sub(
                        r"\b%s\b" % re.escape(sig),
                        "%s %s" % (sig, MARK),
                        line,
                        count=1,
                    )
                    done.add(sig)
        out.append(line)

    with open(dst, "w") as fh:
        fh.writelines(out)

    missing = [s for s in signals if s not in done]
    print("patch_expose: exposed %d/%d signals -> %s" % (len(done), len(signals), dst))
    for s in sorted(done):
        print("  exposed: %s" % s)
    if missing:
        print("  NOT FOUND (declaration not located): %s" % ", ".join(missing))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
