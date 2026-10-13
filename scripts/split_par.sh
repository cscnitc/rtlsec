#!/bin/bash
# Run the single-probe streams in PARALLEL.
#
# The harness reads cmds.hex / writes resp.txt relative to its CWD, so each
# worker just needs its own small directory; the compiled model is invoked by
# absolute path and is not copied. Runs are independent, so they parallelise
# perfectly - the sequential version was the only reason this was slow.
set -u
SPLIT="${1:-/tmp/split_es}"
BIN="$HOME/h/harness/entropy_src/obj_dir/Vtb_entropy_src"
W="${2:-32}"
ROOT=/tmp/swork
OUT=/tmp/split_results.tsv
rm -rf "$ROOT"; mkdir -p "$ROOT"
: > "$OUT"

run_one() {
  n="$1"
  d="$ROOT/$n"
  mkdir -p "$d"
  cp "$SPLIT/cmds_$n.hex" "$d/cmds.hex" 2>/dev/null || return 0
  cp "$SPLIT/map_$n.json" "$d/cmds.map" 2>/dev/null || return 0
  ( cd "$d" && "$BIN" >/dev/null 2>&1 )
  python3 /home/player/split_extract.py "$d/cmds.map" "$d/resp.txt" "$n" >> "$OUT.$n"
}
export -f run_one
export SPLIT ROOT BIN OUT

ls "$SPLIT"/cmds_*.hex | sed 's/.*cmds_//; s/\.hex//' | \
  xargs -P "$W" -I{} bash -c 'run_one {}'

cat "$OUT".* > "$OUT" 2>/dev/null
rm -f "$OUT".*
echo "rows: $(wc -l < "$OUT")"
