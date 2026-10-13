#!/bin/bash
cd "$HOME/h/fi/entropy_src" || exit 1
H=obj_dir/Vtb_fi_entropy_src_entropy_src.h
echo "=== how the surviving members are declared ==="
for s in mubi_module_en rng_fips_o core_rng_enable mubi_es_enable mubi_fips_en; do
  echo "--- $s"
  grep -nE "\b$s\b" "$H" 2>/dev/null | head -3
done
echo
echo "=== does any patched signal appear at all? sample grep ==="
for s in conf_rng_fips_qs threshold_oneway_qs mubi_rng_fips mubi_es_route; do
  n=$(grep -cE "\b$s\b" "$H" 2>/dev/null)
  p=$(grep -cE "\b$s\b" patched/entropy_src_core.sv patched/entropy_src_reg_top.sv 2>/dev/null | awk -F: '{t+=$2} END{print t}')
  echo "  $s: in DUT header=$n  in patched RTL=$p"
done
echo
echo "=== is the pragma actually in the patched text? ==="
grep -n "public_flat_rw" patched/entropy_src_core.sv | head -3
echo
echo "=== verilator bin option list for inline/opt ==="
/opt/verilator/bin/verilator_bin --help 2>/dev/null | grep -iE "inline|dce|--opt|public" | head -12
