#!/usr/bin/env bash
# build_harness.sh — build a generated rtlsec harness with Verilator.
#
#   ./build_harness.sh <ip> <harness_dir> [tree]
#
# Runs INSIDE the build container.
#
# Verilator 4.x notes that this script works around:
#   * no `--main`, and the top-level clock must be driven from C++, so the
#     harness takes `clk` as an input and a simulation main is generated;
#   * packages must be compiled before their importers, so the file list is
#     topologically ordered by resolve_filelist.sh;
#   * dependency closure has to be discovered, since FuseSoC 2.4.7 rejects this
#     tree's cores.
set -euo pipefail

IP="${1:?ip}"
DIR="${2:?harness dir}"
TREE="${3:-${RTL_TREE:-.}}"
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIST="${LIST:-$HOME/h/${IP}_files.f}"

# The Verilator install keeps its runtime headers under
# share/verilator/include, while the makefile it generates looks for
# $VERILATOR_ROOT/include. Detect the real location and hand it to make.
VROOT="$(dirname "$(dirname "$(find /opt/verilator -name verilated.mk 2>/dev/null | head -1)")")"
if [ -z "$VROOT" ] || [ ! -f "$VROOT/include/verilated.mk" ]; then
  echo "FATAL: cannot locate verilated.mk under /opt/verilator" >&2
  exit 1
fi
VL="${VL:-/opt/verilator/bin/verilator}"

cd "$DIR"

MODEL="Vtb_${IP}"

# --- 1. dependency closure (cached) ---
if [ ! -s "$LIST" ]; then
  echo "resolving RTL file list for $IP ..."
  VL="$VL" bash "$HERE/resolve_filelist.sh" "$IP" "$TREE" "$LIST" 2>&1 |
    grep -vE '^order: [0-9]+ package' || true
fi
if [ ! -s "$LIST" ]; then
  echo "FATAL: no file list resolved for $IP" >&2
  exit 1
fi
echo "file list: $(wc -l < "$LIST") files"

# --- 2. simulation main ---
cat > sim_main.cpp <<EOF
#define TB_HEADER "${MODEL}.h"
#define VMODEL ${MODEL}
$(cat "$HERE/../src/sim_main.cpp.tmpl")
EOF

# --- 3. include paths for the RTL tree (technology-specific prim dirs are
#        excluded so ASIC/FPGA cells cannot shadow the generic simulation
#        implementations) ---
inc_args=()
while IFS= read -r d; do inc_args+=("-I$d"); done < <(
  find "$TREE/hw/ip" "$TREE/hw/top_earlgrey" -maxdepth 4 -type d -name rtl 2>/dev/null |
    grep -v -e '/dv/' -e '/pre_dv/' -e '/vendor/' \
      -e '/prim_asap7/' -e '/prim_nangate45/' -e '/prim_sky130/' \
      -e '/prim_tech/' -e '/prim_xilinx/' -e '/prim_artya7/' \
      -e '/prim_pulp/' -e '/prim_egfab' | sort
)

# --- 4. generate the C++ model, then build it with the correct
#        VERILATOR_ROOT (see the note above) ---
"$VL" --cc --exe -Wno-fatal -Wno-lint -Wno-style \
  --top-module "tb_${IP}" \
  "${inc_args[@]}" \
  --Mdir obj_dir --prefix "$MODEL" \
  -f "$LIST" "tb_${IP}.sv" sim_main.cpp > "$DIR/verilator_gen.log" 2>&1
gen_rc=$?
if [ "$gen_rc" -ne 0 ]; then
  echo "FATAL: Verilator elaboration failed:" >&2
  grep -E '%Error' "$DIR/verilator_gen.log" | grep -v 'Command Failed' | head -10 >&2
  exit 1
fi

# The generated makefile hard-assigns VERILATOR_ROOT itself, which overrides an
# environment variable. A command-line make variable takes precedence over the
# makefile's own assignment, so pass it as an argument.
make -C obj_dir -f "$MODEL.mk" VERILATOR_ROOT="$VROOT" -j 8 > "$DIR/make.log" 2>&1
make_rc=$?
if [ "$make_rc" -ne 0 ]; then
  echo "FATAL: C++ build failed:" >&2
  grep -E 'error:|Error' "$DIR/make.log" | head -10 >&2
  exit 1
fi

BIN="obj_dir/$MODEL"
if [ -x "$BIN" ]; then
  echo "HARNESS_OK $DIR/$BIN"
else
  echo "HARNESS_FAIL no binary at $BIN" >&2
  exit 1
fi
