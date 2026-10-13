# rtlsec — measured results

All numbers here come from runs whose **no-fault control was verified
`diverged=0`** on the same binary that produced them. Numbers measured on a
revision with a broken control are void and are not reproduced here.

Block identities, signal names and source locations are deliberately withheld
throughout: this repository describes the tool, and what the tool surfaces is
reported to the design owners rather than published here.

## What the tool surfaces

Every result below comes from a blind run: the tool is pointed at an RTL tree
and told nothing about what to look for. Findings are handed to a human for
validation — the tool reports what it flagged and why, and does not adjudicate
severity or exploitability.

Across the blocks swept so far, the detectors surfaced candidate patterns in
these categories:

| category | detector | what triggers a report |
|---|---|---|
| silent divergence | differential injection | forcing an internal signal changes a security-relevant observable while the error, alert and status paths stay inert |
| guarded-action contradiction | contract oracle | the design's own error signal asserts while the action that error is supposed to guard still advances |
| loose encoding consumption | encoding audit | a multi-bit encoding is read through the loose test at a site whose same-role siblings use the strict test |
| unguarded locked write | bus campaign | a locked, write-protected register accepts a software write and reads back the written value |

## Sweep totals (control validated on the same binary)

Each row is one block of the target design swept end to end; the block label is
arbitrary. "Inert" means the run produced no divergence at all. "Interventions
that took effect" is the count of runs where the injected change was
demonstrably applied — a run where it was not is not evidence of anything.

| block | runs | silent findings | detected | inert | interventions that took effect |
|---|---|---|---|---|---|
| A | 282 | 13 | 12 | 257 | 104 / 282 |
| B | 312 | 6 | 0 | 306 | 288 / 312 |
| C | 120 | 5 | 4 | 111 | 60 / 120 |

"Detected" counts runs where the design's own error, alert or status path did
report the change. Those are not findings — the design caught them. Only the
silent column is what the tool exists to surface.

Note on reproducing these: `scripts/sweep.sh` originally hardcoded one block and
ignored its arguments, so invoking it with a different block name silently
re-swept the first one and printed identical totals. It now takes the block as
its first argument and refuses to run if that block has no harness directory or
driver binary. Where a recorded figure came from a parameterised run rather than
the old script, that is stated alongside it.

## What each detector reaches

- **Fault injection** finds silent divergences in register state and in consumer
  logic. It does not find gating defects whose target cannot be forced at all.
- **Contract oracle** finds a missing guard when the harness can name both the
  error signal and the guarded action.
- **Encoding audit** reaches the loose/strict class that injection cannot, and
  ranks sites so the output is small enough to act on.

Each of the three is independently reachable from the CLI and each has produced
findings on real RTL.

## Measurement discipline

These rules are enforced in code, not in prose, and they are what makes the
numbers above meaningful:

1. **A control runs on the same binary as the result.** A finding without a
   clean no-fault control on the identical build is discarded.
2. **Non-interventions are reported as non-interventions.** If the injected
   change did not demonstrably take effect, the run is scored as inert. It is
   never counted as evidence that the design is clean.
3. **Only the silent column is a finding.** A change that the design's own
   reporting path catches is detection, not a defect.
4. **Verdicts are not taken from the tool's prose.** Every reported pattern
   carries the witness that produced it.

## Limits of what these numbers cover

Not every discovered surface has a harness, so coverage is a function of
harnesses, not of the detectors. The counts above describe the blocks that were
swept; they are not a claim about the tree as a whole. Where a block could not be
driven at all, that is recorded as a coverage gap rather than folded into a
clean-looking zero.
