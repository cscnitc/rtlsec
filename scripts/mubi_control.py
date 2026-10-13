#!/usr/bin/env python3
"""Control for the MuBi sweep oracle: is the status baseline clean?

If the design's alert/status registers are nonzero even for CANONICAL writes,
then "status nonzero" does not mean the design noticed anything, and the
0-findings null is a false negative produced by a dirty baseline rather than a
design that enforces its contract.
"""
import json
import sys
from collections import Counter

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

    canon_status = Counter()
    noncanon_status = Counter()
    per_reg = {}

    for p in probes:
        val = p.get("value", 0)
        ri = p.get("readback_idx")
        if ri is None or ri >= len(resp) or resp[ri]["op"] != "R":
            continue
        lsb = p.get("lsb", 0)
        back = (resp[ri]["data"] >> lsb) & 0xF
        accepted = back == (val & 0xF)
        if not accepted:
            continue

        # status signature: all status-register values for this probe
        sig = tuple(resp[si]["data"] for _, si in p.get("status", []) if si < len(resp))
        is_canon = val in CANON_MUBI4
        (canon_status if is_canon else noncanon_status)[sig] += 1

        reg = p.get("register")
        d = per_reg.setdefault(reg, {"canon": Counter(), "noncanon": Counter()})
        d["canon" if is_canon else "noncanon"][sig] += 1

    print("=== ACCEPTED probes: status signature distribution ===")
    print(f"canonical accepted:     {sum(canon_status.values())} probes, "
          f"{len(canon_status)} distinct status signatures")
    for sig, n in canon_status.most_common(5):
        print(f"   {n:4d} x  status={tuple(hex(x) for x in sig)}")
    print(f"non-canonical accepted: {sum(noncanon_status.values())} probes, "
          f"{len(noncanon_status)} distinct status signatures")
    for sig, n in noncanon_status.most_common(5):
        print(f"   {n:4d} x  status={tuple(hex(x) for x in sig)}")

    print()
    print("=== is the baseline clean? (all-zero status = clean) ===")
    cz = canon_status.get((0, 0, 0), 0)
    nz = noncanon_status.get((0, 0, 0), 0)
    print(f"canonical probes with ALL-ZERO status:     {cz}/{sum(canon_status.values())}")
    print(f"non-canonical probes with ALL-ZERO status: {nz}/{sum(noncanon_status.values())}")
    if sum(canon_status.values()) and cz == 0:
        print("VERDICT: baseline is DIRTY - canonical writes also raise status, so")
        print("         'status nonzero' proves nothing. The 0-findings null is")
        print("         NOT trustworthy as 'the design enforces its contract'.")
    else:
        print("VERDICT: baseline clean - status reflects the write under test.")
        print("         A non-canonical value accepted with all-zero status would")
        print("         be a real undetected violation.")


if __name__ == "__main__":
    main()
