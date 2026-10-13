#!/usr/bin/env bash
# run_fi.sh — build and run a differential fault-injection campaign for one module.
#
#   ./run_fi.sh <module> [tree] [queue.json]
#
# Runs INSIDE the build container.
#
# Single elaboration: the driver source exists as a stub before Verilator runs,
# so the generated makefile references it; the resolved driver then overwrites
# that same file before `make`. That avoids elaborating the design twice, which
# matters because dual-instance models are expensive to elaborate.
set -euo pipefail

MODULE="${1:?module}"
TREE="${2:-${RTL_TREE:-.}}"
QUEUE="${3:-$HOME/h/batch/pursued.json}"
DIR="${DIR:-$HOME/h/fi/$MODULE}"
LIST="${LIST:-$HOME/h/${MODULE}_files.f}"
VL="${VL:-/opt/verilator/bin/verilator}"
VROOT=/opt/verilator/share/verilator
MODEL="Vtb_fi_${MODULE}"

mkdir -p "$DIR"
cd "$DIR"

# --- 1. dependency closure -------------------------------------------------
if [ ! -s "$LIST" ]; then
  echo "resolving RTL file list for $MODULE ..."
  VL="$VL" bash "$HOME/rtlsec-tool/scripts/resolve_filelist.sh" "$MODULE" "$TREE" "$LIST" \
    2>&1 | grep -vE '^order: [0-9]+ package' || true
fi
[ -s "$LIST" ] || { echo "FATAL: no file list for $MODULE" >&2; exit 1; }
echo "file list: $(wc -l < "$LIST") files"

# --- 2. harness ------------------------------------------------------------
"$HOME/rtlsec-tool/target/release/rtlsec" fi-tb --tree "$TREE" --ip "$MODULE" --out "$DIR"

# If a hand-authored testbench exists for this module it takes precedence. The
# generated stimulus drives free data inputs from an LFSR, which cannot produce
# a protocol handshake: a module that waits on a request/acknowledge exchange
# parks in idle, no consumer is live when a signal is forced, and every
# injection becomes a false negative. Harnesses under harnesses/<module>/ add a
# real responder so the design is actually exercised.
AUTHORED="$HOME/rtlsec-tool/harnesses/$MODULE/tb_fi_$MODULE.sv"
if [ -s "$AUTHORED" ]; then
  cp "$AUTHORED" "$DIR/tb_fi_$MODULE.sv"
  echo "using authored harness: $AUTHORED"
fi

# --- 3. stub driver, then elaborate ---------------------------------------
printf '#include "verilated.h"\nint main(){return 0;}\n' > fi_main.cpp

inc_args=()
while IFS= read -r d; do inc_args+=("-I$d"); done < <(
  find "$TREE/hw/ip" "$TREE/hw/top_earlgrey" -maxdepth 4 -type d -name rtl 2>/dev/null |
    grep -v -e '/dv/' -e '/pre_dv/' -e '/vendor/' \
      -e '/prim_asap7/' -e '/prim_nangate45/' -e '/prim_sky130/' \
      -e '/prim_tech/' -e '/prim_xilinx/' -e '/prim_artya7/' \
      -e '/prim_pulp/' -e '/prim_egfab' | sort
)

# --- 4. expose the ranked signals on patched copies, then elaborate -------
# Per-signal metacomments on copies of the declaring RTL files. The global
# --public-flat-rw flag is deliberately NOT used: on Verilator 4.210 it makes
# the generated __Syms.cpp reference package variables that do not exist, so
# the model cannot compile.
python3 "$HOME/rtlsec-tool/scripts/prepare_fi.py" \
  --module "$MODULE" --tree "$TREE" --list "$LIST" --queue "$QUEUE" \
  --workdir "$DIR" || { echo "FATAL: could not expose fault targets" >&2; exit 1; }
FLIST="$DIR/files_fi.f"
[ -s "$FLIST" ] || { echo "FATAL: no patched file list" >&2; exit 1; }

echo "elaborating dual-instance model ..."
# obj_dir MUST be cleared first: the success test below looks for a generated
# file, and that file survives from any earlier run. Leaving it in place makes a
# failed elaboration indistinguishable from a successful one, so the pipeline
# silently reuses a stale model built from an older testbench.
rm -rf obj_dir
# --x-assign 0 --x-initial 0 : Verilator's default maps each unknown to a
# UNIQUE value, so the two identical instances receive different garbage for
# undefined state and diverge immediately — with no fault injected at all. That
# makes the differential meaningless. Forcing all unknowns to a fixed 0 keeps
# the two instances bit-identical, which is what a differential requires.
"$VL" --cc --exe -Wno-fatal -Wno-lint -Wno-style \
  --x-assign 0 --x-initial 0 \
  --top-module "tb_fi_${MODULE}" "${inc_args[@]}" \
  --Mdir obj_dir --prefix "$MODEL" \
  -f "$FLIST" "tb_fi_${MODULE}.sv" fi_main.cpp > verilator_gen.log 2>&1 || true
if grep -qE '^%Error' verilator_gen.log || [ ! -f "obj_dir/${MODEL}___024root.h" ]; then
  echo "FATAL: elaboration failed:" >&2
  grep -E '%Error' verilator_gen.log | grep -v 'Command Failed' | head -10 >&2
  exit 1
fi
echo "elaborated: obj_dir/${MODEL}___024root.h ($(grep -cE '^%Error' verilator_gen.log || true) errors)"

# --- 4. resolve fault targets against the real netlist --------------------
"$HOME/rtlsec-tool/target/release/rtlsec" fi-cpp --tree "$TREE" --ip "$MODULE" \
  --queue-file "$QUEUE" --out "$DIR"

# --- 5. compile ------------------------------------------------------------
make -C obj_dir -f "${MODEL}.mk" VERILATOR_ROOT="$VROOT" -j 8 > make.log 2>&1 || {
  echo "FATAL: compile failed:" >&2
  grep -E 'error:' make.log | head -10 >&2
  exit 1
}

if [ -x "obj_dir/${MODEL}" ]; then
  echo "FI_HARNESS_OK $DIR/obj_dir/${MODEL}"
else
  echo "FI_HARNESS_FAIL" >&2
  exit 1
fi
