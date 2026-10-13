#!/bin/bash
# How many discovered IPs could the generator actually harness?
# The generator needs a synthesizable top named <ip>.sv exposing clk_i and a
# simple-port interface; count how many IPs satisfy that, and list the blockers.
cd ${RTL_TREE:-.}/hw/ip || exit 1
tot=0; ok=0
echo "=== per-IP: top present? clk_i? elab-ready? ==="
for d in */; do
  ip="${d%/}"
  [ -d "$ip/rtl" ] || continue
  top="$ip/rtl/$ip.sv"
  tot=$((tot+1))
  if [ ! -f "$top" ]; then
    printf "  %-20s NO_TOP\n" "$ip"
    continue
  fi
  if grep -qE '\bclk_i\b' "$top"; then
    ok=$((ok+1))
    printf "  %-20s top+clk_i\n" "$ip"
  else
    printf "  %-20s no_clk_i\n" "$ip"
  fi
done
echo
echo "IPs with a rtl/ dir : $tot"
echo "harnessable (top+clk_i): $ok"
