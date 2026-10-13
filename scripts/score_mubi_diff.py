#!/usr/bin/env python3
"""Differential scoring for the MuBi encoding sweep.

The absolute test ("status must be all-zero") is invalid on this design: the
alert/status registers are nonzero even for CANONICAL writes, so it can never
fire and returns a false negative.

Correct test, per (register, field):
  baseline  = the status signature produced by writing this field's CANONICAL
              values.
  candidate = a NON-canonical value that was ACCEPTED (readback == written).
  A candidate whose status signature matches the field's canonical baseline was
  treated by the hardware exactly like a legal value -> undetected violation.

This needs no knowledge of the specific defect: the canonical set comes from the
encoding, and the baseline comes from the same field's own legal writes.
"""
import json
import sys
from collections import defaultdict

CANON_MUBI4 = {0b0101, 0b1010}


def load_responses(path):
    out = []
    for line in open(path, errors="replace"):
        p = line.split()
        if len(p) >= 3 and p[0] in ("W", "R"):
            out.append({"op": p[0], "addr": int(p[1], 16), "data": int(p[2], 16)})
    return out


def main():
    probes = [json.loads(l) for l in open(sys.argv[1]) if l.strip().startswith("{")]
    resp = load_responses(sys.argv[2])

    fields = defaultdict(lambda: {"canon": set(), "cand": []})
    rejected = accepted = 0
    unscorable = 0
    clean_canon = clean_noncanon = 0

    for p in probes:
        val = p.get("value", 0)
        ri, lsb = p.get("readback_idx"), p.get("lsb", 0)
        if ri is None or ri >= len(resp) or resp[ri]["op"] != "R":
            continue
        back = (resp[ri]["data"] >> lsb) & 0xF
        sig = tuple(resp[si]["data"] for _, si in p.get("status", []) if si < len(resp))
        # Scorable only if the alert read back 0 after the clear. Otherwise the
        # post-write status carries earlier probes' alerts and no verdict about
        # this write can be drawn from it.
        sb = p.get("status_before")
        baseline_clean = sb is not None and sb < len(resp) and resp[sb]["data"] == 0
        if not baseline_clean:
            unscorable += 1
            continue
        if val in CANON_MUBI4:
            clean_canon += 1
        elif back == (val & 0xF) and val != 0:
            clean_noncanon += 1
        key = (p.get("register"), p.get("field"))
        if val in CANON_MUBI4:
            fields[key]["canon"].add(sig)
            # A field is only testable if a CANONICAL write reads back its own
            # value; otherwise writes are being dropped and "readback == written"
            # proves nothing.
            if back == (val & 0xF):
                fields[key]["writable"] = True
        elif back == (val & 0xF) and val != 0:
            # val != 0: zero is the field's reset value, so writing 0 and reading
            # 0 is consistent with the write having been DROPPED. Counting that
            # as "accepted" manufactures a violation out of nothing.
            accepted += 1
            fields[key]["cand"].append((p.get("probe"), val, back, sig))
        else:
            rejected += 1

    print(f"probes={len(probes)}  UNSCORABLE (alert would not clear): {unscorable}")
    print(f"  scorable: {clean_canon} canonical, {clean_noncanon} non-canonical-accepted")
    print(f"probes={len(probes)}  non-canonical REJECTED (design said no): {rejected}")
    print(f"probes={len(probes)}  non-canonical ACCEPTED (design said yes): {accepted}")
    print()

    findings = []
    no_baseline = []
    for key, d in sorted(fields.items(), key=lambda kv: str(kv[0])):
        if not d["cand"]:
            continue
        if not d["canon"] or not d.get("writable"):
            no_baseline.append((key, len(d["cand"])))
            continue
        for probe, val, back, sig in d["cand"]:
            if sig in d["canon"]:
                findings.append((probe, key[0], key[1], val, back, sig))

    print("=== undetected encoding-contract violations (differential; field proven writable, value != 0) ===")
    if findings:
        seen = set()
        for probe, reg, field, val, back, sig in findings:
            k = (reg, field)
            if k in seen:
                continue
            seen.add(k)
            n = sum(1 for f in findings if (f[1], f[2]) == k)
            print(f"  {reg}.{field}: {n} non-canonical value(s) accepted with the "
                  f"same status as a CANONICAL write (e.g. wrote {val:#x}, read "
                  f"{back:#x}, status={tuple(hex(x) for x in sig)})")
    else:
        print("  none")

    if no_baseline:
        print()
        print("=== fields with accepted non-canonical values but NO canonical "
              "baseline (unscorable, not a finding) ===")
        for key, n in no_baseline:
            print(f"  {key[0]}.{key[1]}: {n} accepted writes, no canonical probe")


if __name__ == "__main__":
    main()
