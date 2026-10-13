#!/usr/bin/env python3
"""prepare_fi.py — point the fault-injection elaboration at patched RTL copies.

Usage:
  prepare_fi.py --module MOD --tree TREE --list LIST --queue QUEUE --workdir DIR

For every signal of MOD in the ranked queue, locate the RTL file that declares
it, copy that file, and add a per-signal `/*verilator public_flat_rw*/
metacomment to the copy. Then emit a new file list (DIR/files_fi.f) in which
each patched copy stands in for its original, so Verilator elaborates the
patched source. The read-only tree is never written to.

Why not the `--public-flat-rw` flag: on Verilator 4.210 that flag makes the
generated __Syms.cpp reference package variables that do not exist, so the
model does not compile. Metacomments expose only what is needed and avoid it.

That the patched copy replaces the original matters: if both were listed,
Verilator would see two definitions of the module.
"""
import argparse
import json
import os
import re
import sys

MARK = "/*verilator public_flat_rw*/"


# A declaration is `<type> <name>[, <name>]* ;` where the type is an identifier
# (optionally package-qualified) with optional packed/unpacked dimensions.
# Requiring that shape matters because the loose conditions below are not enough:
# a CONTINUATION LINE of a multi-line assignment ends in ';' and contains no '=',
# '(' or '<', so it passed as a declaration and the metacomment was injected into
# the middle of an expression, e.g.
#   init_o & en_i & root_key_i /*verilator public_flat_rw*/.valid ? SlotRoot : ...
# which fails elaboration with a syntax error at the injected comment.
DECL_RE = re.compile(
    r"^[A-Za-z_][\w:]*(\s*\[[^\]]*\])*"          # type, optional packed dims
    r"\s+[A-Za-z_]\w*(\s*\[[^\]]*\])*"           # first name, optional unpacked dims
    r"(\s*,\s*[A-Za-z_]\w*(\s*\[[^\]]*\])*)*"    # further names
    r"\s*;$"
)


def is_decl_line(line: str) -> bool:
    s = line.strip()
    if not s.endswith(";"):
        return False
    # `import`/`export` lines end in ';' and contain no excluded token, but they
    # are not declarations: a metacomment placed on one is a syntax error
    # (observed as `unexpected /*verilator public_flat_rw*/` on
    # `import prim_mubi_pkg::mubi4_and_hi;`). Skip them.
    if s.startswith(("import ", "export ")):
        return False
    # shape check on the line with metacomments removed, so a previously injected
    # comment cannot disguise a statement fragment as a declaration
    clean = re.sub(r"/\*.*?\*/", " ", s).strip()
    if not DECL_RE.match(clean):
        return False
    for bad in ("=", "(", "<", "always", "assign", "initial", "generate", "endmodule"):
        if bad in s:
            return False
    return True


def whole_word(line: str, sig: str) -> bool:
    return re.search(r"\b%s\b" % re.escape(sig), line) is not None


def expose(text: str, sig: str):
    """Return (new_text, hit) adding the metacomment to sig's declaration.

    Declarations inside function/task bodies are skipped: a metacomment on a
    function-local variable is a syntax error, and such a variable is not
    design state that can be faulted anyway. Without this, a ranked name that
    happens to match a local (e.g. `malformed_err` in tlul_pkg) corrupts the
    file and breaks elaboration.
    """
    out, done = [], False
    depth = 0
    for line in text.split("\n"):
        s = line.strip()
        if re.match(r"^(function|task)\b", s):
            depth += 1
        if depth == 0 and not done and is_decl_line(line) and whole_word(line, sig):
            line = re.sub(r"\b%s\b" % re.escape(sig), "%s %s" % (sig, MARK), line, count=1)
            done = True
        if re.match(r"^(endfunction|endtask)\b", s):
            depth = max(0, depth - 1)
        out.append(line)
    return "\n".join(out), done


def decl_type_of(text: str, sig: str):
    """Return the declared type text for `sig` (e.g. 'mubi4_t'), or None.

    A single declaration may declare several signals at once, with metacomments
    interleaved:

        lc_tx_t err_sts_d /*verilator public_flat_rw*/, err_sts_q /*verilator public_flat_rw*/;

    A naive prefix match captures everything before the target name -- including
    the OTHER signal's name and the comment -- and emitting that as the type of
    the force variable re-declares that signal (observed at elaboration as
    `Duplicate declaration of signal: 'err_sts_d'`).

    So: strip metacomments, split the declarator list on commas, and take the
    type from the first declarator by dropping its final identifier (the
    declared name). Everything before it is the type. Returns None when the
    line does not declare `sig`, so a mis-parse can never invent a type.
    """
    for line in text.split("\n"):
        s = line.strip()
        if not is_decl_line(line) or not whole_word(line, sig):
            continue
        clean = re.sub(r"/\*.*?\*/", " ", s).strip()
        if not clean.endswith(";"):
            continue
        clean = clean[:-1].strip()
        parts = [p.strip() for p in clean.split(",")]
        if not parts:
            continue
        # only accept if one of the declarators actually declares `sig`
        names = []
        for i, p in enumerate(parts):
            if i == 0:
                m = re.match(r"^(.*?)\s+([A-Za-z_]\w*)$", p)
                names.append(m.group(2) if m else None)
            else:
                names.append(p if re.match(r"^[A-Za-z_]\w*$", p) else None)
        if sig not in names:
            continue
        m = re.match(r"^(.*?)\s+([A-Za-z_]\w*)$", parts[0])
        if not m:
            continue
        typ = m.group(1).strip()
        if typ:
            return typ
    return None


def instrument(text: str, sig: str, typ: str):
    """Rewrite `assign <sig> = <expr>;` into a force-mux over PUBLIC VARIABLES.

    Marking a net with the metacomment is not enough: Verilator folds nets into
    their consumers, so the member never materialises (measured: 142 signals
    marked, 11 addressable). A declared *variable* does become a member. So the
    driver is rewritten as

        <typ>  rtlsec_fi_val_<sig>;   // public_flat_rw
        logic  rtlsec_fi_sel_<sig>;   // public_flat_rw
        assign <sig> = rtlsec_fi_sel_<sig> ? rtlsec_fi_val_<sig> : (<expr>);

    which leaves the golden model untouched (sel defaults to 0) while making the
    net's value settable from the testbench.
    """
    if ("rtlsec_fi_val_%s" % sig) in text:
        return text, False
    lines = text.split("\n")
    for i, line in enumerate(lines):
        s = line.strip()
        m = re.match(r"^assign\s+%s\s*=\s*(.+);\s*$" % re.escape(sig), s)
        if not m:
            continue
        expr = m.group(1)
        indent = line[:len(line) - len(line.lstrip())]
        vsig, ssig = "rtlsec_fi_val_%s" % sig, "rtlsec_fi_sel_%s" % sig
        lines[i:i + 1] = [
            "%s%s %s /*verilator public_flat_rw*/;" % (indent, typ, vsig),
            "%slogic %s /*verilator public_flat_rw*/;" % (indent, ssig),
            "%sassign %s = %s ? %s : (%s);" % (indent, sig, ssig, vsig, expr),
        ]
        return "\n".join(lines), True
    return text, False


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--module", required=True)
    ap.add_argument("--tree", required=True)
    ap.add_argument("--list", required=True)
    ap.add_argument("--queue", required=True)
    ap.add_argument("--workdir", required=True)
    a = ap.parse_args()

    # signals of interest for this module, best-ranked first
    with open(a.queue) as fh:
        q = json.load(fh)
    entries = [e for e in q if e.get("module") == a.module]
    names, seen = [], set()
    for e in entries:
        s = e.get("signal")
        if s and s not in seen:
            seen.add(s)
            names.append(s)
    print("queue: %d signal names for %s" % (len(names), a.module))
    if not names:
        print("prepare_fi: no queue entries for %s (nothing to expose)" % a.module)
        return 0

    with open(a.list) as fh:
        files = [l.strip() for l in fh if l.strip()]

    # index the candidate .sv files
    contents = {}
    for f in files:
        if not f.endswith(".sv"):
            continue
        try:
            contents[f] = open(f).read()
        except OSError:
            continue

    # module name a signal's scope implies, e.g. "entropy_src_core__pi1" ->
    # "entropy_src_core", used to prefer the right declaring file
    scope_of = {e["signal"]: (e.get("scope") or "").split("__")[0]
                for e in entries if e.get("signal")}


    patched = {}      # original path -> patched copy path
    placed = {}       # original path -> [signals]
    missing = []
    for sig in names:
        want_mod = scope_of.get(sig) or a.module
        # prefer a file whose basename matches the scope module, then any file
        # Packages hold types, functions and constants, not faultable design
        # state; patching them corrupts the file (see expose()).
        cands = sorted([f for f in contents if not os.path.basename(f).endswith("_pkg.sv")],
                       key=lambda f: (0 if want_mod and want_mod in os.path.basename(f) else 1,
                                      len(contents[f])))
        hit = False
        for f in cands:
            txt = patched.get(f, contents[f])
            new, ok = expose(txt, sig)
            if ok:
                # already patched this file: re-expose from the current text
                # A net that is merely marked is still folded away, so also
                # rewrite its driver into a force-mux gated by public variables,
                # which do become addressable members.
                typ = decl_type_of(new, sig)
                if typ:
                    new, _ = instrument(new, sig, typ)
                patched[f] = new
                placed.setdefault(f, []).append(sig)
                hit = True
                break
        if not hit:
            missing.append(sig)

    if not patched:
        print("prepare_fi: NO signals could be exposed — refusing to continue")
        print("  missing: %s" % ", ".join(missing[:12]))
        return 1

    pdir = os.path.join(a.workdir, "patched")
    # Clear previously patched copies. A stale copy from an earlier run is the
    # same hazard as a stale elaborated model: a generated intermediate that
    # persists can be silently reused. If any patched file were listed without
    # being regenerated, the run would quietly proceed against old RTL.
    if os.path.isdir(pdir):
        for f in os.listdir(pdir):
            if f.endswith(".sv"):
                os.remove(os.path.join(pdir, f))
    os.makedirs(pdir, exist_ok=True)
    out_lines, replaced = [], set()
    for f in files:
        if f in patched:
            dst = os.path.join(pdir, os.path.basename(f))
            with open(dst, "w") as fh:
                fh.write(patched[f])
            out_lines.append(dst)
            replaced.add(f)
        else:
            out_lines.append(f)

    flist = os.path.join(a.workdir, "files_fi.f")
    with open(flist, "w") as fh:
        fh.write("\n".join(out_lines) + "\n")

    total = sum(len(v) for v in placed.values())
    print("prepare_fi: exposed %d/%d signals across %d patched file(s)"
          % (total, len(names), len(patched)))
    for f, sigs in sorted(placed.items()):
        print("  %s: %d signals" % (os.path.basename(f), len(sigs)))
    if missing:
        print("  not located (%d): %s" % (len(missing), ", ".join(missing[:10])))
    print("file list written: %s" % flist)
    # A signal we cannot expose cannot be faulted; say so rather than silently
    # running a sweep that is missing targets.
    return 0


if __name__ == "__main__":
    sys.exit(main())
