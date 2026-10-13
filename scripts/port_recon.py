#!/usr/bin/env python3
"""Report the port shape of an OpenTitan IP top module."""
import re
import sys

ip = sys.argv[1] if len(sys.argv) > 1 else "entropy_src"
paths = [
    f"${RTL_TREE:-.}/hw/ip/{ip}/rtl/{ip}.sv",
    f"${RTL_TREE:-.}/hw/top_earlgrey/ip_autogen/{ip}/rtl/{ip}.sv",
]
t = None
for p in paths:
    try:
        t = open(p, errors="ignore").read()
        print("file:", p, "len:", len(t))
        break
    except OSError:
        continue
if t is None:
    sys.exit(f"no top module found for {ip}")

i = t.find(f"module {ip}")
if i < 0:
    sys.exit("module header not found")
seg = t[i : i + 20000]
j = seg.find(");")
decl = seg[:j]
ports = re.findall(
    r"(input|output|inout)\s+(?:wire\s+|logic\s+)?(\[[^\]]*\])?\s*(\w+)", decl
)
ins = [x for x in ports if x[0] == "input"]
outs = [x for x in ports if x[0] == "output"]
print("total ports:", len(ports), "inputs:", len(ins), "outputs:", len(outs))
print("tl ports:", [x[2] for x in ports if "tl_" in x[2]])
print("clk/rst:", [x[2] for x in ports if x[2] in ("clk_i", "rst_ni", "rst_shadowed_ni")])
print("first 30 inputs:", [x[2] for x in ins[:30]])
print("first 15 outputs:", [x[2] for x in outs[:15]])
# how many inputs are simple one-bit vs buses
buses = [x for x in ins if x[1]]
print("bus inputs:", len(buses), "scalar inputs:", len(ins) - len(buses))
