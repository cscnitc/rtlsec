#!/usr/bin/env bash
# resolve_filelist.sh — build the RTL file list for an IP top by chasing
# Verilator's "cannot find module/package" errors around the tree.
#
#   ./resolve_filelist.sh <module> [tree] [out_filelist]
#
# <module> is a module name, e.g. `entropy_src` or `flash_ctrl_lcmgr` — it does
# not need to be a directory name. The seed is the module's own source file,
# found tree-wide by filename.
#
# Generic: it needs no build-system metadata (FuseSoC 2.4.7 rejects this tree's
# cores, and passing every file on the command line overruns the argument
# limit), only that the RTL lives somewhere under the tree root.
#
# Two ordering rules matter for Verilator 4.x:
#   * packages must be compiled before the modules that import them, so all
#     `*_pkg.sv` files are listed first;
#   * a signal's defining module must be found, so the tree's RTL directories
#     are offered as `-y` search paths.
set -uo pipefail

IP="${1:?ip}"
TREE="${2:-${RTL_TREE:-.}}"
OUT="${3:-filelist.f}"
VL="${VL:-verilator}"
WORK="$(mktemp -d)"
LIST="$WORK/files.f"
# Technology-specific prim implementations (ASIC standard cells, FPGA libs)
# must not shadow the generic simulation implementations.
EXCL=(-e '/dv/' -e '/pre_dv/' -e '/vendor/' -e '/tb/'
      -e '/prim_asap7/' -e '/prim_nangate45/' -e '/prim_sky130/'
      -e '/prim_tech/' -e '/prim_xilinx/' -e '/prim_artya7/' -e '/prim_pulp/' -e '/prim_egfab')

# --- seed: the module's own source file(s), found tree-wide by filename, plus
#     the parent IP's package files (they carry the local typedefs). Everything
#     else is pulled in on demand by the resolution loop. ---
find "$TREE/hw" -name "${IP}.sv" 2>/dev/null | grep -v "${EXCL[@]}" > "$LIST"
if [ ! -s "$LIST" ]; then
  echo "FATAL: no source file ${IP}.sv found under $TREE/hw" >&2
  exit 1
fi
# parent dir of the first hit, e.g. hw/ip/flash_ctrl/rtl -> seed sibling pkgs
SEEDDIR="$(dirname "$(head -1 "$LIST")")"
find "$SEEDDIR" -maxdepth 1 -name '*_pkg.sv' 2>/dev/null >> "$LIST"
sort -u -o "$LIST" "$LIST"
n_pkgs=$(grep -c '_pkg\.sv$' "$LIST" || true)

inc_args=()
while IFS= read -r d; do inc_args+=("-I$d"); done < <(
  find "$TREE/hw/ip" "$TREE/hw/top_earlgrey" -maxdepth 4 -type d -name rtl 2>/dev/null |
    grep -v "${EXCL[@]}" | sort
)

# Keep packages ahead of everything else, topologically ordered by their own
# imports (`entropy_src_pkg` imports `prim_util_pkg` but sorts before it).
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
order() {
  python3 "$HERE/order_filelist.py" "$1"
}

# Extract the names Verilator reports as unresolved. Wording varies across
# versions, so several patterns are tried.
missing_names() {
  printf '%s\n' "$1" | grep -oE "(Cannot find file containing (module|package|interface|primitive)|Package/class|missing package|module) '[^']+'" |
    grep -oE "'[^']+'" | tr -d "'" | sort -u
}

for i in $(seq 1 100); do
  order "$LIST" > "$WORK/ordered.f"
  err="$("$VL" --lint-only -Wno-fatal -Wno-lint -Wno-style \
    --top-module "$IP" "${inc_args[@]}" -f "$WORK/ordered.f" 2>&1)"
  rc=$?

  if [ "$rc" -eq 0 ]; then
    order "$LIST" > "$OUT"
    echo "resolved: $(wc -l < "$OUT") files after $i iterations ($n_pkgs packages seeded)"
    exit 0
  fi

  missing="$(missing_names "$err")"
  if [ -z "$missing" ]; then
    echo "no unresolved names, but verilator rc=$rc; first errors:"
    printf '%s\n' "$err" | grep -E '%Error' | head -12
    order "$LIST" > "$OUT"
    exit 1
  fi

  added=0
  while IFS= read -r name; do
    [ -z "$name" ] && continue
    # Prefer the generic simulation implementation of any prim_* module.
    f=""
    for cand in "$TREE/hw/ip/prim_generic/rtl/${name}.sv" "$TREE/hw/ip/prim/rtl/${name}.sv"; do
      if [ -f "$cand" ]; then f="$cand"; break; fi
    done
    if [ -z "$f" ]; then
      f="$(find "$TREE/hw" -name "${name}.sv" 2>/dev/null | grep -v "${EXCL[@]}" | head -1)"
    fi
    if [ -n "$f" ] && ! grep -qxF "$f" "$LIST"; then
      echo "$f" >> "$LIST"
      added=$((added + 1))
    fi
  done <<< "$missing"

  if [ "$added" -eq 0 ]; then
    echo "stalled at iteration $i; unresolved: $(printf '%s ' $missing)"
    printf '%s\n' "$err" | grep -E '%Error' | head -12
    order "$LIST" > "$OUT"
    exit 1
  fi
  sort -u -o "$LIST" "$LIST"
done

echo "iteration limit reached: $(wc -l < "$LIST") files"
order "$LIST" > "$OUT"
exit 1
