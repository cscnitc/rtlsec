#!/bin/bash
# Run every single-probe stream from reset and collect one result row each.
#
# The alert cannot be cleared mid-run (verified: 367/368 probes stayed dirty
# after both a write-1-to-clear and the same preceded by a canonical enable
# re-assert), so a fresh simulation per probe is the only way to get a clean
# baseline. Each run starts from reset and carries the preamble itself.
cd "$HOME/h/harness/entropy_src" || exit 1
SPLIT="${1:-/tmp/split_es}"
BIN=./obj_dir/Vtb_entropy_src
OUT=/tmp/split_results.tsv
: > "$OUT"
n_done=0
for f in "$SPLIT"/cmds_*.hex; do
  [ -e "$f" ] || { echo "no streams in $SPLIT"; exit 1; }
  tag=$(basename "$f" .hex | sed 's/^cmds_//')
  cp "$f" cmds.hex
  cp "$SPLIT/map_${tag}.json" cmds.map 2>/dev/null || continue
  $BIN > /dev/null 2>&1
  python3 /home/player/split_extract.py cmds.map resp.txt "$tag" >> "$OUT"
  n_done=$((n_done+1))
done
echo "ran $n_done single-probe simulations"
echo "rows: $(wc -l < "$OUT")"
head -3 "$OUT"
