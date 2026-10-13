#!/bin/bash
# Scan every patched RTL copy for the mis-typed-declaration signature:
# a declarator list that contains an injected force variable alongside a real
# signal, i.e.  <something>, rtlsec_fi_val_<sig> ;  which re-declares the
# earlier signal and breaks elaboration.
echo "=== patched dirs present ==="
ls -d "$HOME"/h/fi*/patched "$HOME"/h/fi*/patched 2>/dev/null | sort -u
echo
echo "=== 1. signature: a comma-list containing an injected var ==="
for d in $(ls -d "$HOME"/h/fi*/patched 2>/dev/null | sort -u); do
  hits=$(grep -rnE ',[[:space:]]*rtlsec_fi_(val|sel)_' "$d" 2>/dev/null | head -5)
  if [ -n "$hits" ]; then
    echo "  $d:"
    echo "$hits" | sed 's/^/    /'
  else
    echo "  $d: clean"
  fi
done
echo
echo "=== 2. every injected declaration, for inspection ==="
for d in $(ls -d "$HOME"/h/fi*/patched 2>/dev/null | sort -u); do
  echo "  --- $d"
  grep -rhoE '^[[:space:]]*[a-zA-Z_][^;]*rtlsec_fi_val_[A-Za-z_0-9]+[^;]*;' "$d" 2>/dev/null \
    | sed 's/^[[:space:]]*//' | sort -u | head -8 | sed 's/^/      /'
done
echo
echo "=== 3. count of injected vars per module ==="
for d in $(ls -d "$HOME"/h/fi*/patched 2>/dev/null | sort -u); do
  n=$(grep -rhoE 'rtlsec_fi_val_[A-Za-z_0-9]+' "$d" 2>/dev/null | sort -u | wc -l)
  echo "  $(basename $(dirname $d)): $n distinct injected vars"
done
