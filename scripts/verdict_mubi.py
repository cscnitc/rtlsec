#!/usr/bin/env python3
"""Per-field MuBi encoding-contract verdict, one probe per run.

Simple and sufficient: for each (register, field), compare the alert status
produced by its CANONICAL writes (MuBi4 true = 0b0101, false = 0b1010) against
the status produced by each ACCEPTED non-canonical write. Identical status means
the hardware treated an illegal encoding exactly like a legal one.

Each row came from its own simulation starting at reset, so the alert read
belongs to that write alone - which is what the sticky-alert confound made
impossible inside one long sweep.
"""
import sys
from collections import defaultdict

# Authoritative for this tree: prim_mubi_pkg defines MuBi4True = 4'h6,
# MuBi4False = 4'h9 (verified in hw/ip/prim/rtl/prim_mubi_pkg.sv:23-24). The
# older 4'b0101/4'b1010 pair is a DIFFERENT generation and inverting the two
# turns canonical writes into apparent violations.
CANON = {0b0110, 0b1001}


def main(path):
    rows = []
    for line in open(path):
        f = line.rstrip("\n").split("\t")
        if len(f) < 7:
            continue
        rows.append({
            "probe": f[0], "reg": f[1], "field": f[2], "lsb": int(f[3]),
            "val": int(f[4]), "back": f[5], "alert": int(f[6]),
        })
    print(f"rows={len(rows)}")

    fields = defaultdict(lambda: {"canon": set(), "cand": []})
    for r in rows:
        key = (r["reg"], r["field"])
        if r["val"] in CANON:
            fields[key]["canon"].add(r["alert"])
        else:
            try:
                back = int(r["back"])
            except ValueError:
                continue
            # accepted = readback equals what was written, and the value is not
            # the reset value (writing 0 and reading 0 is consistent with the
            # write having been dropped)
            if back == r["val"] and r["val"] != 0:
                fields[key]["cand"].append((r["probe"], r["val"], back, r["alert"]))

    print(f"fields with a canonical baseline: "
          f"{sum(1 for v in fields.values() if v['canon'])}")
    print()
    findings = []
    for (reg, fld), d in sorted(fields.items()):
        if not d["canon"] or not d["cand"]:
            continue
        for probe, val, back, alert in d["cand"]:
            if alert in d["canon"]:
                findings.append((reg, fld, probe, val, back, alert, sorted(d["canon"])))
    print("=== undetected encoding-contract violations ===")
    seen = set()
    for reg, fld, probe, val, back, alert, canon_alerts in findings:
        if (reg, fld) in seen:
            continue
        seen.add((reg, fld))
        n = sum(1 for f in findings if (f[0], f[1]) == (reg, fld))
        print(f"  {reg}.{fld}: {n} accepted non-canonical value(s) with the same "
              f"alert as canonical (e.g. wrote {val:#x} read {back:#x} "
              f"alert={alert:#x}, canonical alert={[hex(x) for x in canon_alerts]})")
    if not findings:
        print("  none")

    print()
    print("=== fields where a non-canonical write DID raise a new alert (correct) ===")
    ok = 0
    for (reg, fld), d in sorted(fields.items()):
        if not d["canon"] or not d["cand"]:
            continue
        if any(alert not in d["canon"] for _, _, _, alert in d["cand"]):
            ok += 1
    print(f"  {ok} field(s)")

    print()
    print("=== fields with accepted non-canonical values but no canonical baseline ===")
    nb = [(k, len(v["cand"])) for k, v in sorted(fields.items()) if v["cand"] and not v["canon"]]
    for k, n in nb:
        print(f"  {k[0]}.{k[1]}: {n} accepted")
    if not nb:
        print("  none")


if __name__ == "__main__":
    main(sys.argv[1])
