import sys
from collections import defaultdict

want = sys.argv[2:] if len(sys.argv) > 2 else None
rows = defaultdict(dict)
for line in open(sys.argv[1]):
    f = line.rstrip("\n").split("\t")
    if len(f) < 7:
        continue
    rows[(f[1], f[2])][int(f[4])] = (f[5], int(f[6]))

for (reg, fld), vals in sorted(rows.items()):
    if want and not any(w in f"{reg}.{fld}" for w in want):
        continue
    print(f"=== {reg}.{fld}  (canonical = 6 / 9) ===")
    print("   wrote  readback  alert   verdict")
    for v in sorted(vals):
        back, alert = vals[v]
        canon = "CANON " if v in (6, 9) else "      "
        if int(back) == v:
            verdict = "accepted"
        else:
            verdict = "REJECTED (readback != written)"
        print(f"   {v:#04x}   {back:>6}    {alert:#06x}  {canon} {verdict}")
    print()
