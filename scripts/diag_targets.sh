#!/bin/bash
# Why are only 5 of 244 queue signals addressable? Check, for each entropy_src
# queue signal, whether it actually exists as a member of the DUT class header.
cd "$HOME/h/fi/entropy_src" || exit 1
H=obj_dir/Vtb_fi_entropy_src_entropy_src.h
R=obj_dir/Vtb_fi_entropy_src___024root.h

echo "=== header sizes ==="
wc -l "$H" "$R" 2>/dev/null

python3 - <<'PY'
import json, re, os
q = json.load(open('/home/player/h/batch/pursued.json'))
names = [e['signal'] for e in q if e['module'] == 'entropy_src']
print(f"entropy_src queue names: {len(names)}")

def load(p):
    try: return open(p, errors='replace').read()
    except OSError: return ""

dut = load('obj_dir/Vtb_fi_entropy_src_entropy_src.h')
root = load('obj_dir/Vtb_fi_entropy_src___024root.h')
patched = ""
for f in os.listdir('patched'):
    patched += load(os.path.join('patched', f))

in_dut = [n for n in names if re.search(r'\b%s\b' % re.escape(n), dut)]
in_root = [n for n in names if re.search(r'\b%s\b' % re.escape(n), root)]
in_patched = [n for n in names if re.search(r'\b%s\b' % re.escape(n), patched)]

print(f"  present in DUT class header : {len(in_dut)}")
print(f"  present in root header      : {len(in_root)}")
print(f"  present in patched RTL      : {len(in_patched)}")
print()
print("  sample in DUT header:", in_dut[:12])
print()
print("  MuBi ones in DUT header:", [n for n in in_dut if re.search(r'mubi|fips|oneway', n, re.I)][:15])
print()
print("  MuBi ones in patched RTL:", [n for n in in_patched if re.search(r'mubi|fips|oneway', n, re.I)][:15])
print()
for probe in ('conf_rng_fips_qs', 'threshold_oneway_qs', 'mubi_rng_fips', 'mubi_module_en'):
    print(f"  {probe:24s} dut={bool(re.search(r'\\b%s\\b' % probe, dut))} "
          f"patched={bool(re.search(r'\\b%s\\b' % probe, patched))}")
PY
