#!/usr/bin/env python3
"""Extract one probe's result from a single-probe run.

Each split run is a fresh simulation starting from reset, so the alert read
after the write belongs to this write alone - which is what the sticky-alert
confound made impossible inside one long sweep.

Prints one TSV row: probe  register  field  lsb  wrote  readback  alert  err_code  err_code_test
"""
import json
import sys


def load_responses(path):
    out = []
    for line in open(path, errors="replace"):
        p = line.split()
        if len(p) >= 3 and p[0] in ("W", "R"):
            out.append({"op": p[0], "addr": int(p[1], 16), "data": int(p[2], 16)})
    return out


def main():
    map_path, resp_path, tag = sys.argv[1], sys.argv[2], sys.argv[3]
    rows = [json.loads(l) for l in open(map_path) if l.strip().startswith("{")]
    if not rows:
        return
    p = rows[0]
    resp = load_responses(resp_path)
    ri, wi = p.get("readback_idx"), p.get("write_idx")
    if ri is None or ri >= len(resp) or wi is None or wi >= len(resp):
        print("\t".join([tag, p.get("register", "?"), p.get("field", "?"),
                         str(p.get("lsb", 0)), str(p.get("value", 0)),
                         "NA", "NA", "NA", "NA"]))
        return
    lsb = p.get("lsb", 0)
    back = (resp[ri]["data"] >> lsb) & 0xF
    vals = []
    for _, si in p.get("status", []):
        vals.append(resp[si]["data"] if si < len(resp) else -1)
    while len(vals) < 3:
        vals.append(0)
    print("\t".join([tag, p.get("register", "?"), p.get("field", "?"), str(lsb),
                     str(p.get("value", 0)), str(back),
                     str(vals[0]), str(vals[1]), str(vals[2])]))


if __name__ == "__main__":
    main()
