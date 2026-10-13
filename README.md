# rtlsec — autonomous security attacker for emulated RTL designs

> **Note for the competition organizers:** entries **B-016b, B-017, B-021,
> B-024, B-025 and B-026** were found with this tool.

A tool that attacks a *running* hardware design rather than reading its source.
It discovers the attack surface from the design's own machine-readable specs,
drives the live device through its real bus interface, and reports security
invariant violations it finds on its own.

Written in Rust. The RTL tree is read-only throughout; nothing is ever written
into it.

## Why this shape

Register maps are rarely complete in one place. Older IP blocks define their
registers in one machine-readable format, newer ones are migrating to another,
and the generated RTL is the only artifact that holds the complete, implemented
address map. rtlsec reads both specification sources and merges them, so an IP
block whose registers are split across the old and new formats still ends up
with its full address map.

The second reason is the bus. Peripheral IP tops in this class of SoC expose a
standard peripheral bus port. That means one generic bus driver can attack every
one of them over the interface they actually present to software, with no
per-IP adapter — the register map comes from the specs, the protocol is fixed.

## Status

**M1 — attack-surface discovery: done and verified.** A whole tree is enumerated
into per-IP register surfaces with authoritative byte offsets read from the
generated register packages, and lock relationships resolved from the
specifications. A full tree resolves to several dozen IP register surfaces. Unit
tests cover bit-range parsing, offset parsing, and offset lookup.

**M2 — harness generation: done for the blocks exercised so far.** Given an IP
top and a resolved filelist, the tool emits a Verilator harness that
instantiates the real IP and drives its bus port from a software host model, and
the campaign runs over that bus.

**M3 — campaign scale-up: partial.** Coverage is a function of harnesses, not of
the detectors: the detectors are IP-agnostic but each block needs a harness
before it can be swept. Measured results for the blocks covered so far are in
RESULTS.md.

**M4 — validation: ongoing.** Pointed blind at a tree, the tool flags patterns on
its own across three detectors (differential injection, contract oracle,
encoding audit) and separately surfaces divergences the sweeps were not looking
for. Flagged sites are handed to a human for validation; the tool does not
adjudicate severity.

## Usage

```
rtlsec list      --tree <RTL_TREE>
rtlsec surface   --tree <RTL_TREE> [--ip <ip>] [--out <file.json>] [--quiet]
rtlsec rank      --tree <RTL_TREE> --xml <Vtop.xml> [--workers N] [--out <dir>]
rtlsec attack    --tree <RTL_TREE> --ip <ip> [--only N]
rtlsec campaign  --tree <RTL_TREE> --ip <ip> [--only N]
rtlsec fi-tb     --tree <RTL_TREE> --ip <module> [--out <dir>] [--map <file>] [--resp <file>]
rtlsec fi-cpp    --tree <RTL_TREE> --ip <module> [--out <dir>]
rtlsec mubi-audit --tree <RTL_TREE> [--quiet]
```

- `list` / `surface` — discover what is there.
- `rank` — order candidate signals by security relevance, so the sweep spends
  its runs on plausible targets first.
- `attack` / `campaign` — drive the block over its bus and look for patterns.
- `fi-tb` / `fi-cpp` — generate and drive the differential fault-injection
  harness (two instances of the same module, one faulted).
- `mubi-audit` — the encoding audit: rank multi-bit encodings that are read
  through a loose (permissive) test at a site whose same-role siblings use the
  strict test.

## Data sources

- Register specifications in both the older and newer machine-readable formats;
  the tool reads both and merges them.
- Generated register packages, for authoritative byte offsets.
- The elaborated design (via Verilator XML), for signal reachability and
  hierarchy.

## Tell-tale patterns as an algorithm

The detectors do not recognise defect names; they test *predicates*. Each
pattern is a decision procedure over the design and its behaviour, so the same
code applies to any block that has a harness:

| id | pattern | decision procedure | why it is a tell |
|---|---|---|---|
| T1 | unguarded locked write | write the field while its write-protection lock reads *locked*; read back | a lock advertised as protection that does not protect |
| T2 | loose encoding consumption | at each permissive-encoding read site, is the value reachable in a non-canonical state, and do same-role siblings use the strict read? | a permissive read accepts every encoding except one, so an illegal value reads as ENABLED |
| T3 | silent divergence | force an internal signal, compare two instances of the same module cycle by cycle, and watch the error/alert path | the design's own reporting says nothing happened |
| T4 | guarded-action contradiction | is the error output asserted while the action that error is supposed to guard still advances? | the guard is documented in code and absent in behaviour |
| T5 | dropped gating term | does a gating expression omit a term its same-role siblings carry? | an omission is invisible at the definition site |
| T6 | dead write / unreachable clear | is a write or clear path structurally present but absent from every reachable transition? | intent is documented, effect cannot occur |

Two guards make the output trustworthy, and both are enforced in code rather
than in prose: a pattern is reported only when the intervention was *shown to
take effect*, and every result is accepted only alongside a no-fault control on
the same binary that stays clean. A no-op force is therefore reported as a
no-op, never as evidence of a clean design.

### Why this scales further than current practice

The claim this design supports is not that a hand-written detector found
hand-picked sites. It is that the patterns are IP-agnostic and the per-block
cost is one harness:

- **Deterministic and reproducible.** No model in the loop, no prompt drift; a
  run is bit-for-bit repeatable and its control is part of the record.
- **Sound by construction.** Findings require an effective intervention *and* a
  clean control, so the false-positive rate is bounded by the harness, not by
  heuristics.
- **Oracles over signatures.** Nothing here enumerates a defect name, so the
  same binary applies to a block it has never seen.
- **The bottleneck is mechanical, not intellectual.** Ranked output is already
  small enough to act on — a whole-tree population of several hundred candidate
  encoding sites collapses to a handful of ranked findings — which is what makes
  fuzzer-scale operation plausible: the remaining work is harness co-generation
  and parallel campaign execution across workers, not new detection theory.

That combination — deterministic, soundly guarded, signature-free, and already
reducing a whole-tree site population to a handful of ranked findings — is where
we expect this to outperform dynamic RTL security tooling that depends on either
hand-written properties or unguarded statistical ranking.

## Invariant oracles (no knowledge of specific defects)

Each is a property any correct control must satisfy; none names a specific
defect.

- **Detection completeness** — a change to a security-relevant observable that
  is *not* reported by the design's own error path is a finding. Implemented as
  the differential injection oracle.
- **Lock integrity** — write-protected controls reject writes.
- **Clear integrity** — cleared secrets do not survive a clear.
- **Consent integrity** — controls do not consent on non-canonical encodings.
  Implemented both on the bus and statically.
- **Sibling agreement** — structurally identical sites that disagree about their
  guarding are flagged, odd-one-out first. This ranking is what makes the output
  small enough to act on.
- **Privilege monotonicity** — access rights never widen.
- **Guarded-action contradiction** — the design's own error signal asserting
  while the guarded action still advances is a self-contradiction. Implemented as
  the contract oracle.

## Injection mechanism: nets are rewritten, not exposed

Fault injection needs a way to drive an internal signal from outside without
modifying the source of truth. The generated net cannot be assigned from C++
directly, so the tool rewrites each single-driver net into a selector multiplexer
over two variables that are made public at build time, and drives both halves.
The RTL tree is never touched; the rewrite happens on copies.

Practical consequences, both measured rather than assumed:

- The public mark is applied per-signal as a source metacomment, not with a
  global compiler flag; the global flag is broken in the toolchain version used
  here.
- Only *variables* become addressable this way, not nets. That is why single-driver
  nets are rewritten into the selector form first.
- Not every marked signal survives elaboration: inlining can eliminate a signal
  that is declared but never read, so the tool reports how many targets are
  actually addressable rather than how many were marked.

## Known limitations (measured, not assumed)

1. **Coverage is harness-bound.** Every block needs a harness before the
   detectors can run on it. Discovered surfaces without a harness are coverage
   gaps, reported as such.
2. **Signals eliminated by elaboration cannot be targeted.** Where a module is
   inlined into its parent, its internals may have no addressable member at all,
   and the driver for that block will not compile until the harness declares the
   witness differently.
3. **Some defects are not reachable by injection at all** — gating defects whose
   target cannot be forced, and encodings that a register interface rejects
   before they reach the consumer. Those are reached by the static audit, and
   the tool says which detector produced which finding.
4. **Ranking is a heuristic.** It orders work; it does not decide truth. Nothing
   is reported on ranking alone.

## Building

```
cargo build --release
cargo test --release
```

Requires a Rust toolchain and Verilator for the harness and simulation paths.
The analysis paths (`list`, `surface`, `rank`, `mubi-audit`) need no simulator.

## Layout

```
src/            Rust core: surface discovery, harness generation, campaign
                planner, oracles, ranking, injection, encoding audit
scripts/        harness build and sweep drivers
harnesses/      testbenches that instantiate real IP and drive its bus
legacy-static/  earlier static-fact front end, kept for reference
```
