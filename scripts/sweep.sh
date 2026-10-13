#!/bin/bash
# Sweep every faultable target across the seed window and classify each run.
#
# Only signals the design does not recompute are targets (the patcher drops
# nets). A SILENT finding needs two things to be meaningful: the fault changed a
# security-relevant observable (diverged=1) AND nothing on the error/alert/
# status path reported it (reported=0). A run with diverged=0 says nothing about
# the signal - the intervention may simply not have propagated - so it is
# counted as inert, never as evidence.
MODULE="${1:-flash_ctrl_lcmgr}"
cd "$HOME/h/fi/$MODULE" || { echo "no harness dir for module '$MODULE'"; exit 1; }
BIN="./obj_dir/Vtb_fi_$MODULE"
[ -x "$BIN" ] || { echo "no driver binary for module '$MODULE' ($BIN)"; exit 1; }
echo "module: $MODULE"

mapfile -t TARGETS < <(awk '/TARGETS\[\] *= *\{/{f=1} f{print} /};/{if(f)exit}' fi_main.cpp \
  | grep -oE '"[A-Za-z_][A-Za-z_0-9]*"' | tr -d '"')

echo "targets: ${#TARGETS[@]}"
total=0; silent=0; detected=0; nohit=0; effective=0
: > sweep_hits.txt
: > sweep_all.txt
for sig in "${TARGETS[@]}"; do
  for cyc in 30 60 90 120 150 180; do
    line=$($BIN +signal="$sig" +cycle=$cyc +for=1 2>/dev/null | grep WITNESS)
    d=$(echo "$line"  | grep -oE 'diverged=[01]'        | cut -d= -f2)
    r=$(echo "$line"  | grep -oE 'reported=[01]'        | cut -d= -f2)
    fe=$(echo "$line" | grep -oE 'force_effective=[01]' | cut -d= -f2)
    total=$((total+1))
    echo "$sig $cyc diverged=$d reported=$r force_effective=$fe" >> sweep_all.txt
    if [ "$d" = "1" ] && [ "$r" = "0" ]; then
      silent=$((silent+1)); echo "$sig @$cyc" >> sweep_hits.txt
    elif [ "$d" = "1" ]; then
      detected=$((detected+1))
    else
      nohit=$((nohit+1))
    fi
    if [ "$fe" = "1" ]; then effective=$((effective+1)); fi
  done
done
echo "SWEEP: $total runs -> $silent silent findings, $detected detected, $nohit inert"
echo "interventions that demonstrably took effect: $effective of $total"
echo "--- silent findings by target ---"
sort sweep_hits.txt | awk '{print $1}' | uniq -c | sort -rn | head -20
echo "--- diverged=1 runs (the only informative ones) ---"
grep -c 'diverged=1' sweep_all.txt
