#!/usr/bin/env python3
"""Print the TL-UL struct definitions and the FuseSoC core file list for an IP."""
import os
import re
import sys

pkg = "${RTL_TREE:-.}/hw/ip/tlul/rtl/tlul_pkg.sv"
t = open(pkg, errors="ignore").read()
print("=== tlul_pkg.sv typedefs of interest ===")
for name in ("tl_a_op_e", "tl_d_op_e", "tl_a_user_t", "tl_h2d_t", "tl_d2h_t"):
    m = re.search(r"typedef[^;]*?\b" + name + r"\b[^;]*;", t, re.S)
    if m:
        print(m.group(0).strip()[-600:])
        print("---")
    else:
        print(name, "NOT FOUND")
        print("---")

ip = sys.argv[1] if len(sys.argv) > 1 else "entropy_src"
print(f"=== {ip} core / filelist ===")
for base in (f"${RTL_TREE:-.}/hw/ip/{ip}", f"${RTL_TREE:-.}/hw/top_earlgrey/ip_autogen/{ip}"):
    for f in os.listdir(base) if os.path.isdir(base) else []:
        if f.endswith(".core"):
            print("core:", os.path.join(base, f))
    rtl = os.path.join(base, "rtl")
    if os.path.isdir(rtl):
        sv = [x for x in os.listdir(rtl) if x.endswith((".sv", ".svh"))]
        print("rtl file count:", len(sv), "sample:", sorted(sv)[:8])
print("=== top_pkg TL widths ===")
tp = open("${RTL_TREE:-.}/hw/top_earlgrey/rtl/autogen/top_earlgrey_pkg.sv", errors="ignore").read()
for m in re.finditer(r"parameter int (TL_[A-Z_]+)\s*=\s*([^;]+);", tp):
    print("  ", m.group(1), "=", m.group(2).strip())
