#!/usr/bin/env python3
"""Ensure the entropy_src queue contains the MuBi fanout signals that the loose-consumption pattern
depend on, then report whether they become addressable."""
import json
import os

Q = os.path.expanduser("~/h/batch/pursued.json")
q = json.load(open(Q))

WANT = [
    ("mubi_rng_fips", "entropy_src_core__pi1", "mubi4_t", 4),
    ("mubi_rng_fips_fanout", "entropy_src_core__pi1", "mubi4_t", 8),
    ("mubi_fips_flag_fanout", "entropy_src_core__pi1", "mubi4_t", 8),
    ("mubi_threshold_oneway_fanout", "entropy_src_core__pi1", "mubi4_t", 2),
    ("conf_rng_fips_qs", "entropy_src_reg_top__pi1", "logic", 4),
    ("threshold_oneway_qs", "entropy_src_reg_top__pi1", "logic", 4),
]

have = {(e.get("module"), e.get("signal")) for e in q}
added = []
for sig, scope, vt, w in WANT:
    if ("entropy_src", sig) in have:
        print(f"  already present: {sig}")
        continue
    q.append({"rank": 0, "module": "entropy_src", "signal": sig, "scope": scope,
              "vartype": vt, "width": w, "score": 90, "reasons": ["mubi-fanout"]})
    added.append(sig)

if added:
    json.dump(q, open(Q, "w"))
    print(f"  added {len(added)}: {', '.join(added)}")
print(f"queue now holds {len(q)} entries")
