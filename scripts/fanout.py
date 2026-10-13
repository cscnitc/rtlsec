#!/usr/bin/env python3
"""fanout.py — run one fault campaign per ranked signal, in parallel shards.

Each campaign is an independent process: build the module's simulator once,
then run it once per candidate signal with that signal faulted. There is no
communication between campaigns, so the parallelism is plain process fan-out.

Why not MPI: MPI exists to move messages between ranks of one tightly-coupled
job. These campaigns share nothing, so MPI would add a dependency (it is not
installed on this host) and buy no speed. Process sharding reaches the same
throughput with no install and no launcher daemon. The real limits here are
simulator build time (one per module) and core count.

Usage:
  fanout.py --queue pursued.json --workdir /home/player/h/fi \
            --shards 10 --parallel 10 [--per-signal-seconds 60] [--dry-run]

Writes <workdir>/results/shard_<i>.jsonl and prints a timing summary.
"""
import argparse
import concurrent.futures as cf
import json
import os
import subprocess
import time

MOON = ["ssh", "-o", "BatchMode=yes", "moon"]
CTR = "rustdev"


def sh(cmd, timeout=None):
    return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)


def build_module(module, workdir, tree="${RTL_TREE:-.}", log=print):
    """Build the Verilator model for one module. Runs once, reused by every
    campaign against that module."""
    inner = (
        f"cd {workdir}/{module} && "
        f"bash {workdir}/rtlsec-tool/scripts/build_harness.sh "
        f"{module} {workdir}/{module} {tree}"
    )
    t0 = time.time()
    r = sh(MOON + [f"docker exec {CTR} bash -lc {json.dumps(inner)}"], timeout=3600)
    dt = time.time() - t0
    ok = "HARNESS_OK" in r.stdout
    log(f"  build {module}: {'ok' if ok else 'FAILED'} in {dt:.1f}s")
    return ok, dt


def run_signal(workdir, module, signal, timeout):
    """One campaign: run the module's simulator with this signal faulted."""
    inner = (
        f"cd {workdir}/{module} && "
        f"./obj_dir/Vtb_{module} +fault_signal={signal}"
    )
    t0 = time.time()
    try:
        r = sh(MOON + [f"docker exec {CTR} bash -lc {json.dumps(inner)}"],
               timeout=timeout)
        return {"signal": signal, "rc": r.returncode, "secs": time.time() - t0,
                "tail": (r.stdout + r.stderr)[-400:]}
    except subprocess.TimeoutExpired:
        return {"signal": signal, "rc": -1, "secs": time.time() - t0,
                "tail": "TIMEOUT"}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--queue", required=True)
    ap.add_argument("--workdir", required=True)
    ap.add_argument("--shards", type=int, default=10)
    ap.add_argument("--parallel", type=int, default=10,
                    help="concurrent shards (moon caps at 128 threads)")
    ap.add_argument("--per-signal-seconds", type=int, default=120)
    ap.add_argument("--dry-run", action="store_true")
    args = ap.parse_args()

    queue = json.load(open(args.queue))
    by_mod = {}
    for s in queue:
        by_mod.setdefault(s["module"], []).append(s["signal"])

    print(f"queue: {len(queue)} signals across {len(by_mod)} modules")
    total_est = 0.0
    for m, sigs in sorted(by_mod.items()):
        print(f"  {m}: {len(sigs)} signals")

    if args.dry_run:
        # Shard plan only: who would fault what, and the wall-clock estimate.
        shards = [[] for _ in range(args.shards)]
        for i, s in enumerate(queue):
            shards[i % args.shards].append(f"{s['module']}.{s['signal']}")
        print(f"\nshard plan ({args.shards} shards, {args.parallel} concurrent):")
        for i, sl in enumerate(shards):
            print(f"  shard {i}: {len(sl)} signals")
        print(f"\nbuilds: {len(by_mod)} modules (one each, can run in parallel)")
        return 0

    os.makedirs(f"{args.workdir}/results", exist_ok=True)

    # 1. Build every module's simulator once, in parallel.
    t_start = time.time()
    print("\n[1/2] building simulators")
    with cf.ThreadPoolExecutor(max_workers=args.parallel) as ex:
        futs = {ex.submit(build_module, m, args.workdir): m for m in by_mod}
        for f in cf.as_completed(futs):
            pass
    t_build = time.time() - t_start
    print(f"  builds done in {t_build:.1f}s")

    # 2. Run campaigns across shards.
    print(f"\n[2/2] running {len(queue)} campaigns across {args.parallel} shards")
    shards = [[] for _ in range(args.shards)]
    for i, s in enumerate(queue):
        shards[i % args.shards].append(s)

    def run_shard(idx):
        out = []
        for s in shards[idx]:
            r = run_signal(args.workdir, s["module"], s["signal"],
                           args.per_signal_seconds)
            r["module"] = s["module"]
            out.append(r)
        with open(f"{args.workdir}/results/shard_{idx}.jsonl", "w") as fh:
            for r in out:
                fh.write(json.dumps(r) + "\n")
        return idx, out

    t_camp0 = time.time()
    done = []
    with cf.ThreadPoolExecutor(max_workers=args.parallel) as ex:
        for idx, out in ex.map(run_shard, range(args.shards)):
            done.extend(out)
            print(f"  shard {idx} done ({len(out)} campaigns)")
    t_camp = time.time() - t_camp0

    secs = [d["secs"] for d in done]
    print(f"\ncampaigns: {len(done)} in {t_camp:.1f}s "
          f"({sum(secs)/max(1,len(secs)):.2f}s avg per campaign)")
    print(f"total wall clock: {time.time()-t_start:.1f}s")
    bad = [d for d in done if d["rc"] != 0]
    if bad:
        print(f"non-zero exits: {len(bad)} (see results/*.jsonl)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
