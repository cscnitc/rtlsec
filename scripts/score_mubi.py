#!/usr/bin/env python3
"""Score the MuBi encoding sweep against its own contract.

The contract: a 4-bit encoding-class field is only supposed to accept its
canonical values (MuBi4True=0b0101, MuBi4False=0b1010). Any OTHER value is
non-canonical. A non-canonical value that the hardware ACCEPTS (readback returns
what was written) while its alert/status path stays CLEAR is an undetected
violation - the design did not enforce its own encoding contract.

No knowledge of any specific defect is needed: the canonical set comes from the
encoding, not from the bug.

Usage: score_mubi.py <cmds.map> <resp.txt>
"""
import json
import sys

CANON_MUBI4 = {0b0101, 0b1010}      # MuBi4True, MuBi4False
CANON_MUBI8 = {0b0110_1001, 0b1001_0110}


def load_responses(path):
    """Index only the bus responses (W/R lines); OBS lines are not responses."""
    out = []
    for line in open(path, errors="replace"):
        p = line.split()
        if len(p) >= 3 and p[0] in ("W", "R"):
            err = int(p[3].split("=")[1]) if len(p) > 3 and "=" in p[3] else 0
            out.append({"op": p[0], "addr": int(p[1], 16), "data": int(p[2], 16), "err": err})
    return out


def main():
    map_path, resp_path = sys.argv[1], sys.argv[2]
    probes = [json.loads(l) for l in open(map_path) if l.strip().startswith("{")]
    resp = load_responses(resp_path)
    print(f"probes={len(probes)} bus_responses={len(resp)}")

    accepted_noncanon = 0
    flagged = []
    for p in probes:
        val = p.get("value", 0)
        lsb = p.get("lsb", 0)
        canon = CANON_MUBI4 if val <= 0xF else CANON_MUBI8
        if val in canon:
            continue                      # canonical: nothing to test
        ri = p.get("readback_idx")
        wi = p.get("write_idx")
        if ri is None or ri >= len(resp) or wi is None or wi >= len(resp):
            continue
        rd = resp[ri]
        if rd["op"] != "R":
            continue
        back = (rd["data"] >> lsb) & 0xF

        # Did the design accept the non-canonical value?
        if back != (val & 0xF):
            continue
        accepted_noncanon += 1

        # Did its own alert/status path notice anything?
        noticed = False
        detail = []
        for name, si in p.get("status", []):
            if si < len(resp):
                sv = resp[si]["data"]
                detail.append(f"{name}={sv:#010x}")
                if sv != 0:
                    noticed = True
        if not noticed:
            flagged.append((p.get("probe"), p.get("register"), p.get("field"), val, back, detail))

    print(f"non-canonical values ACCEPTED by the design: {accepted_noncanon}")
    print(f"of those, with a CLEAR alert/status path:    {len(flagged)}")
    print()
    if flagged:
        print("=== undetected encoding-contract violations ===")
        seen = set()
        for probe, reg, field, val, back, detail in flagged:
            key = (reg, field)
            if key in seen:
                continue
            seen.add(key)
            print(f"  probe {probe}: {reg}.{field} wrote {val:#x}, read back {back:#x}, "
                  f"status clear ({', '.join(detail)})")
    else:
        print("no undetected violations in this sweep")


if __name__ == "__main__":
    main()
