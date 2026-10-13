#!/bin/bash
cd "$HOME/h/fi/entropy_src" || exit 1

echo "=== exposed signals in entropy_src_core (count + sample) ==="
echo "  count: $(grep -c public_flat_rw patched/entropy_src_core.sv)"
grep -oE "public_flat_rw\*/ +[A-Za-z_0-9]+" patched/entropy_src_core.sv | tail -6

echo
echo "=== generated model headers ==="
ls obj_dir/*.h 2>/dev/null | head

echo
echo "=== is mubi_rng_fips addressable in any header? ==="
grep -l "mubi_rng_fips" obj_dir/*.h 2>/dev/null | head
echo "  (end)"

echo
echo "=== all exposed names, and where each one actually lives ==="
grep -oE "public_flat_rw\*/ +[A-Za-z_0-9]+" patched/*.sv | sed 's/.*public_flat_rw\*\/ *//' | sort -u > /tmp/exposed.txt
echo "  distinct exposed names: $(wc -l < /tmp/exposed.txt)"
found=0
while read -r s; do
  h=$(grep -l "\b$s\b" obj_dir/*.h 2>/dev/null | head -1)
  if [ -n "$h" ]; then found=$((found+1)); else echo "  NOT-IN-ANY-HEADER: $s"; fi
done < /tmp/exposed.txt
echo "  exposed names present in some header: $found"

echo
echo "=== driver targets (the 5 addressable) ==="
grep -oE 'want == "[A-Za-z_0-9]+"' fi_main.cpp | sed 's/want == //' | tr -d '"' | sort -u | tr '\n' ' '
echo
