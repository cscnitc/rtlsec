#!/bin/bash
# Independently verify the type-extraction fix WITHOUT touching the agent's build
# dirs: run only the patcher (prepare_fi.py) into a scratch workdir, then grep the
# patched RTL for the corruption signature.
set -u
FL="$HOME/h/fi/flash_ctrl_lcmgr"
LIST=""
for cand in "$FL/files_fi.f" "$FL/files.f" "$FL/filelist.f"; do
  [ -f "$cand" ] && LIST="$cand" && break
done
if [ -z "$LIST" ]; then
  echo "no file list found under $FL; listing what is there:"
  ls "$FL" 2>/dev/null | head -20
  exit 0
fi
echo "using file list: $LIST  ($(wc -l < "$LIST") files)"

W=/tmp/fipatchcheck
rm -rf "$W"; mkdir -p "$W"
python3 "$HOME/rtlsec-tool/scripts/prepare_fi.py" \
  --module flash_ctrl_lcmgr \
  --tree ${RTL_TREE:-.} \
  --list "$LIST" \
  --queue "$HOME/h/batch/pursued.json" \
  --workdir "$W" 2>&1 | tail -6

echo
echo "=== corruption signature (comma-list containing an injected var) ==="
if grep -rnE ',[[:space:]]*rtlsec_fi_(val|sel)_' "$W/patched" 2>/dev/null; then
  echo "  ^^ STILL BROKEN"
else
  echo "  clean: 0 occurrences"
fi
echo
echo "=== the err_sts_q declaration + its force lines ==="
grep -nE 'err_sts_(d|q)|rtlsec_fi_(val|sel)_err_sts_q' "$W/patched/flash_ctrl_lcmgr.sv" 2>/dev/null | head -8
echo
echo "=== injected var count ==="
grep -rhoE 'rtlsec_fi_val_[A-Za-z_0-9]+' "$W/patched" 2>/dev/null | sort -u | wc -l
