#!/bin/bash
# Add the MuBi source flops to the entropy_src queue so the FI pipeline exposes
# and targets them. They were absent from the ranked shortlist, which is why
# none of the MuBi signals were addressable as fault targets.
#
# These are REGISTER flops holding the CSR value; forcing one bypasses the CSR
# write path (whose writes are otherwise rejected) and lets the corrupted
# encoding propagate to the fanout consumer.
set -eu
cd "$HOME/h/batch"

echo "=== queue entry shape (first entropy_src entry) ==="
python3 -c "
import json
d=json.load(open('pursued.json'))
for e in d:
    if e['module']=='entropy_src':
        print(json.dumps(e)[:300]); break
print('total entries:', len(d))
"

cp pursued.json pursued.json.bak
python3 - <<'PY'
import json
d = json.load(open('pursued.json'))
tmpl = next(e for e in d if e['module'] == 'entropy_src')
new = []
for sig in ['conf_rng_fips_qs', 'threshold_oneway_qs', 'conf_threshold_scope_qs']:
    if any(e['module'] == 'entropy_src' and e['signal'] == sig for e in d):
        print('  already present:', sig); continue
    e = dict(tmpl)
    e['signal'] = sig
    new.append(e)
d.extend(new)
json.dump(d, open('pursued.json', 'w'))
print('added', len(new), 'signal(s):', [e['signal'] for e in new])
print('total now:', len(d))
PY
