#!/usr/bin/env python3
"""Enumerate the otp_ctrl CSR fields that belong to an encoding class."""
import hjson, re, sys, json

P = "${RTL_TREE:-.}/hw/top_earlgrey/ip_autogen/otp_ctrl/data/otp_ctrl.hjson"
d = hjson.load(open(P))
regs = d["registers"]["core"]


def unwrap(r):
    if "multireg" in r:
        return r["multireg"]
    if "window" in r:
        return r["window"]
    return r


def width(b):
    b = str(b).strip()
    m = re.fullmatch(r"(\d+):(\d+)", b)
    if m:
        return int(m.group(1)) - int(m.group(2)) + 1
    if b.isdigit():
        return 1
    return None


print("=== all registers ===")
for r in regs:
    r = unwrap(r)
    if "name" not in r:
        print("  skip:", list(r.keys()), str(r)[:80])
        continue
    fl = r.get("fields", [])
    print("  %-36s sw=%-8s resval=%-6s fields=%s" % (
        r["name"], r.get("swaccess"), r.get("resval"),
        [(f["name"], f.get("bits"), f.get("swaccess"), f.get("resval"), f.get("mubi")) for f in fl]))

print()
print("=== encoding-class candidates ===")
for r in regs:
    r = unwrap(r)
    if "name" not in r:
        continue
    for f in r.get("fields", []):
        b = str(f.get("bits", ""))
        w = width(b)
        nm = f["name"].lower()
        mu = bool(f.get("mubi")) or "lock" in nm or "en" == nm[-2:] or "fips" in nm or "oneway" in nm
        if w in (4, 8) or mu:
            print("  %-36s %-28s bits=%-10s w=%s sw=%-6s resval=%-8s mubi=%s" % (
                r["name"], f["name"], b, w, f.get("swaccess"), f.get("resval"), f.get("mubi")))
