#!/bin/bash
cd "$HOME/h/fi/flash_ctrl_lcmgr" || exit 1
B=./obj_dir/Vtb_fi_flash_ctrl_lcmgr

echo "=== CONTROL: good stimulus, same ports -> contradiction must be 0 ==="
printf "  %s\n" "$($B +contract=obs_intg_err_o_g,obs_seeds_g 2>/dev/null | grep -oE 'error_asserted=[01] action_advanced=[01] contradiction=[01]')"

echo
echo "=== VIOLATION: read fails integrity ==="
printf "  %s\n" "$($B +contract=obs_intg_err_o_g,obs_seeds_g +bad_intg=1 2>/dev/null | grep -oE 'error_asserted=[01] action_advanced=[01] contradiction=[01]')"

echo
echo "=== why do obs_intg_err_g and obs_intg_err_o_g disagree? ==="
grep -nE "obs_intg_err(_o)?_g" tb_fi_flash_ctrl_lcmgr.sv
echo
echo "=== is one of them undriven (declared but never assigned)? ==="
for p in obs_intg_err_g obs_intg_err_o_g; do
  n=$(grep -cE "assign +$p" tb_fi_flash_ctrl_lcmgr.sv)
  printf "  %-20s assign statements: %s\n" "$p" "$n"
done
