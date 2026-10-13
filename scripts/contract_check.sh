#!/bin/bash
# Contract check: does the design raise its OWN error signal while the guarded
# action still advances? Control first - with good stimulus there must be no
# contradiction, otherwise the check is broken rather than the design.
cd "$HOME/h/fi/flash_ctrl_lcmgr" || exit 1
B=./obj_dir/Vtb_fi_flash_ctrl_lcmgr

echo "=== how the TB reads its stimulus-violation switches ==="
grep -n "plusargs" tb_fi_flash_ctrl_lcmgr.sv | head -8
echo

echo "=== oracle port set now (harness-declared) ==="
grep -c 'if (contract' fi_main.cpp
grep -oE 'obs_[a-z_]+_g' tb_fi_flash_ctrl_lcmgr.sv | sort -u | tr '\n' ' '
echo; echo

# obs_intg_err_g is a stale duplicate of obs_intg_err_o_g (both assign from
# intg_err_o_g, lines 370/404) and reads a frozen value; the oracle rejects it
# with status=unknown-port. Use the paired witness port.
echo "=== CONTROL: good stimulus -> contradiction must be 0 ==="
$B +contract=obs_intg_err_o_g,obs_seeds_g 2>/dev/null | grep -E 'CONTRACT|ACTIVITY'

echo
echo "=== VIOLATION: read fails integrity ==="
$B +contract=obs_intg_err_o_g,obs_seeds_g +bad_intg=1 2>/dev/null | grep -E 'CONTRACT'

echo
echo "=== VIOLATION: bad integrity + corrupted data ==="
$B +contract=obs_intg_err_o_g,obs_seeds_g +bad_intg=1 +bad_data=1 2>/dev/null | grep -E 'CONTRACT'
