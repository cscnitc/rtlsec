//! rtlsec — autonomous live-device security attacker for emulated OpenTitan.
//!
//! Milestone M1 (this binary): attack-surface discovery.
//!   rtlsec surface --tree /opt/opentitan [--ip aes] [--out surface.json]
//!   rtlsec list    --tree /opt/opentitan
//!
//! Later milestones add: harness generation, TL-UL campaign execution, oracles.

mod campaign;
mod fi;
mod harness;
mod mubi;
mod oracle;
mod rank;
mod surface;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Locate and read a module's source file anywhere under the tree, skipping the
/// UVM verification trees and vendored third-party code.
fn find_module_source(tree: &Path, module: &str) -> Option<String> {
    let out = std::process::Command::new("find")
        .arg(tree)
        .args(["-name", &format!("{module}.sv")])
        .output()
        .ok()?;
    let listing = String::from_utf8_lossy(&out.stdout);
    for line in listing.lines() {
        if line.contains("/dv/") || line.contains("/pre_dv/") || line.contains("/vendor/") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(line) {
            return Some(text);
        }
    }
    None
}

fn usage() -> ExitCode {
    eprintln!(
        "rtlsec — attack-surface discovery for OpenTitan RTL\n\n\
         USAGE:\n  \
           rtlsec list      --tree <RTL_TREE>\n  \
           rtlsec surface   --tree <RTL_TREE> [--ip <ip>] [--out <file.json>] [--quiet]\n  \
           rtlsec rank      --tree <RTL_TREE> --xml <Vtop.xml> [--xml ...] [--workers N] [--out <dir>]\n  \
           rtlsec attack    --tree <RTL_TREE> --ip <ip> [--only N]\n  \
           rtlsec campaign  --tree <RTL_TREE> --ip <ip> [--only N]\n  \
           rtlsec fi-tb     --tree <RTL_TREE> --ip <module> [--out <dir>] [--map <file>] [--resp <file>]\n  \
           rtlsec fi-cpp    --tree <RTL_TREE> --ip <module> [--out <dir>]\n  \
           rtlsec mubi-audit --tree <RTL_TREE> [--quiet]\n\n\
         The tree is read-only; nothing is ever written into it."
    );
    ExitCode::from(2)
}

struct Args {
    cmd: String,
    tree: PathBuf,
    ip: Option<String>,
    out: Option<PathBuf>,
    xml: Vec<PathBuf>,
    workers: usize,
    batch: usize,
    per_worker: usize,
    queue_file: Option<PathBuf>,
    start_cycle: u32,
    fault_cycles: u32,
    map: Option<PathBuf>,
    only: Option<usize>,
    split: Option<PathBuf>,
    resp: Option<PathBuf>,
    quiet: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut it = std::env::args().skip(1);
    let cmd = it.next().ok_or_else(|| "missing subcommand".to_string())?;
    let mut tree: Option<PathBuf> = None;
    let mut ip = None;
    let mut out = None;
    let mut only: Option<usize> = None;
    let mut split: Option<PathBuf> = None;
    let mut xml: Vec<PathBuf> = Vec::new();
    let mut workers: usize = 8;
    let mut queue_file: Option<PathBuf> = None;
    let mut start_cycle: u32 = 64;
    let mut fault_cycles: u32 = 64;
    let mut batch: usize = 0;          // 0 => workers * per_worker
    let mut per_worker: usize = 100;
    let mut map = None;
    let mut resp = None;
    let mut quiet = false;
    while let Some(a) = it.next() {
        match a.as_str() {
            "--tree" => tree = it.next().map(PathBuf::from),
            "--ip" => ip = it.next(),
            "--only" => only = it.next().and_then(|v| v.parse::<usize>().ok()),
            "--split" => split = it.next().map(PathBuf::from),
            "--out" => out = it.next().map(PathBuf::from),
            "--xml" => {
                if let Some(x) = it.next() {
                    xml.push(PathBuf::from(x));
                }
            }
            "--workers" => {
                workers = it.next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(8);
            }
            "--queue-file" => {
                queue_file = it.next().map(PathBuf::from);
            }
            "--start-cycle" => {
                start_cycle = it.next().and_then(|x| x.parse::<u32>().ok()).unwrap_or(64);
            }
            "--fault-cycles" => {
                fault_cycles = it.next().and_then(|x| x.parse::<u32>().ok()).unwrap_or(64);
            }
            "--batch" => {
                batch = it.next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
            }
            "--per-worker" => {
                per_worker = it.next().and_then(|x| x.parse::<usize>().ok()).unwrap_or(100);
            }
            "--map" => map = it.next().map(PathBuf::from),
            "--resp" => resp = it.next().map(PathBuf::from),
            "--quiet" => quiet = true,
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(Args {
        cmd,
        tree: tree.ok_or_else(|| "--tree is required".to_string())?,
        ip,
        out,
        xml,
        workers,
        batch,
        per_worker,
        queue_file,
        start_cycle,
        fault_cycles,
        map,
            only,
            split,
        resp,
        quiet,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return usage();
        }
    };
    if !args.tree.is_dir() {
        eprintln!("error: --tree {:?} is not a directory", args.tree);
        return ExitCode::from(2);
    }

    match args.cmd.as_str() {
        "mubi-audit" => return mubi::audit(&args.tree, !args.quiet),
        "surface" => {
            let s = match &args.ip {
                Some(ip) => match surface::discover(&args.tree, ip) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::FAILURE;
                    }
                },
                None => {
                    eprintln!("error: --ip is required for `surface` in this milestone");
                    return usage();
                }
            };
            let json = serde_json::to_string_pretty(&s).expect("serialize surface");
            if let Some(p) = &args.out {
                if let Err(e) = std::fs::write(p, &json) {
                    eprintln!("error: write {p:?}: {e}");
                    return ExitCode::FAILURE;
                }
                eprintln!("wrote {}", p.display());
            } else {
                println!("{json}");
            }
            if !args.quiet {
                let with_off = s.registers.iter().filter(|r| r.offset.is_some()).count();
                let mubi = s
                    .registers
                    .iter()
                    .flat_map(|r| r.fields.iter())
                    .filter(|f| f.is_mubi)
                    .count();
                eprintln!(
                    "{}: {} registers ({} with offset), {} encoding-class fields, {} lock registers",
                    s.ip,
                    s.registers.len(),
                    with_off,
                    mubi,
                    s.lock_registers.len()
                );
            }
            ExitCode::SUCCESS
        }
        "list" => {
            let all = surface::discover_all(&args.tree);
            println!("{:<20} {:>6} {:>8}", "IP", "REGS", "LOCKS");
            for s in &all {
                let with_off = s.registers.iter().filter(|r| r.offset.is_some()).count();
                println!(
                    "{:<20} {:>6} {:>8}",
                    s.ip,
                    with_off,
                    s.lock_registers.len()
                );
            }
            eprintln!("discovered {} IP register surfaces", all.len());
            ExitCode::SUCCESS
        }
        "fi-tb" => {
            let module = match &args.ip {
                Some(m) => m.clone(),
                None => {
                    eprintln!("error: --ip <module> is required for `fi-tb`");
                    return usage();
                }
            };
            let src = match find_module_source(&args.tree, &module) {
                Some(t) => t,
                None => {
                    eprintln!("error: cannot read source for {module}");
                    return ExitCode::FAILURE;
                }
            };
            let tb = match fi::generate_tb(&module, &src, 0x5eed_1234u32) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let dir = args
                .out
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("fi/{module}")));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: create {dir:?}: {e}");
                return ExitCode::FAILURE;
            }
            let tb_path = dir.join(format!("tb_fi_{module}.sv"));
            let meta = serde_json::json!({
                "module": module,
                "observables": tb.observables,
                "stim_ports": tb.stim_ports,
                "tb_top": format!("tb_fi_{module}"),
            });
            if std::fs::write(&tb_path, &tb.tb).is_err()
                || std::fs::write(dir.join("observables.json"),
                    serde_json::to_string_pretty(&meta).unwrap()).is_err()
            {
                eprintln!("error: write harness");
                return ExitCode::FAILURE;
            }
            eprintln!(
                "{module}: fi testbench -> {} ({} observables, {} stim ports)",
                tb_path.display(),
                tb.observables.len(),
                tb.stim_ports.len()
            );
            ExitCode::SUCCESS
        }
        "fi-cpp" => {
            let module = match &args.ip {
                Some(m) => m.clone(),
                None => {
                    eprintln!("error: --ip <module> is required for `fi-cpp`");
                    return usage();
                }
            };
            let dir = args
                .out
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("fi/{module}")));
            let header = match std::fs::read_to_string(
                dir.join("obj_dir")
                    .join(format!("Vtb_fi_{module}___024root.h")),
            ) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("error: read generated header: {e}");
                    return ExitCode::FAILURE;
                }
            };
            // Internal signals exposed by the per-signal public_flat_rw
            // metacomments are members of the DUT's own generated class, which
            // lives in the submodule header rather than the root header.
            let dut_header = std::fs::read_to_string(
                dir.join("obj_dir")
                    .join(format!("Vtb_fi_{module}_{module}.h")),
            )
            .unwrap_or_default();
            let observables: Vec<String> = std::fs::read_to_string(dir.join("observables.json"))
                .ok()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                .and_then(|v| {
                    v["observables"].as_array().map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                })
                .unwrap_or_default();
            let model_header = std::fs::read_to_string(
                dir.join("obj_dir").join(format!("Vtb_fi_{module}.h")),
            )
            .unwrap_or_default();
            // If the harness declares witness ports, they are the authority on
            // what the oracle compares. The module's own output list is not:
            // it filters out anything not "simple" (e.g. a 512-bit state bus),
            // so a signal a hand-written testbench deliberately exposes would
            // otherwise be invisible to the oracle, and any check keyed on it
            // would silently never fire.
            let tb_text = std::fs::read_to_string(dir.join(format!("tb_fi_{module}.sv")))
                .unwrap_or_default();
            let tb_obs = fi::tb_observables(&tb_text);
            let observables: Vec<String> = if !tb_obs.is_empty() {
                tb_obs.into_iter().map(|b| format!("PORT:{b}")).collect()
            } else {
                observables
                    .into_iter()
                    .filter_map(|o| fi::resolve_member(&dut_header, &o))
                    .map(|o| match fi::witness_port(&model_header, &o) {
                        Some(base) => format!("PORT:{base}"),
                        None => o,
                    })
                    .collect()
            };
            // An observation point must not also be a fault target: forcing a
            // signal that the oracle compares makes it differ by construction,
            // which yields a trivially "silent divergence" for every run. The
            // exclusion set uses plain names; the oracle keeps the PORT: form
            // so it reads the harness's witness ports.
            let obs_plain: std::collections::HashSet<String> = observables
                .iter()
                .map(|o| o.trim_start_matches("PORT:").to_string())
                .collect();
            let n_obs_total = observables.len();
            // Report the split honestly: the oracle prefers paired witness ports
            // and only falls back to DUT class members, which Verilator can leave
            // stale. Claiming "in the DUT class" would misdescribe the access
            // path actually used.
            let n_via_port = observables
                .iter()
                .filter(|o| o.starts_with("PORT:"))
                .count();
            eprintln!(
                "observables: {} of {} resolved ({} via witness ports, {} via DUT members)",
                observables.len(),
                n_obs_total,
                n_via_port,
                observables.len() - n_via_port
            );
            let mut names: Vec<String> = Vec::new();
            if let Some(qp) = &args.queue_file {
                if let Ok(txt) = std::fs::read_to_string(qp) {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) {
                        if let Some(arr) = v.as_array() {
                            for e in arr {
                                if e["module"].as_str() == Some(module.as_str()) {
                                    if let Some(sig) = e["signal"].as_str() {
                                        names.push(sig.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Never fault an observation point or a module input (see obs_plain
            // and input_ports above): the former differs by construction, the
            // latter is stimulus corruption rather than a design fault.
            let inputs: std::collections::HashSet<String> = fi::input_ports(&dut_header)
                .into_iter()
                .collect();
            let n_before = names.len();
            let names: Vec<String> = names
                .into_iter()
                .filter(|n| !obs_plain.contains(n) && !inputs.contains(n))
                .collect();
            eprintln!(
                "fault targets: {} candidates -> {} after dropping observation points and inputs",
                n_before,
                names.len()
            );
            // Force-mux targets first: a net that cannot be exposed as a member
            // is still forceable through the two public variables the patcher
            // injected around its driver. Direct members follow, for signals
            // (ports, registers) that Verilator does expose.
            let mut targets = fi::discover_mux_targets(&dut_header, &names);
            for (sig, member) in fi::discover_members(&dut_header, &names) {
                if !targets.iter().any(|(s, _)| *s == sig) {
                    targets.push((sig, member));
                }
            }
            if targets.is_empty() {
                // Fall back to the flattened root-class access path, which is
                // what --public-flat-rw produces.
                targets = fi::discover_flat_names(&header, &names, "u_fault");
            }
            let cpp = fi::generate_cpp(
                &format!("Vtb_fi_{module}"),
                &format!("tb_fi_{module}"),
                &targets,
                &observables,
                args.start_cycle,
                args.fault_cycles,
            );
            let cpp_path = dir.join("fi_main.cpp");
            if std::fs::write(&cpp_path, &cpp).is_err() {
                eprintln!("error: write driver");
                return ExitCode::FAILURE;
            }
            eprintln!(
                "{module}: driver -> {} ({} of {} queue signals addressable as internals)",
                cpp_path.display(),
                targets.len(),
                names.len()
            );
            ExitCode::SUCCESS
        }
        "rank" => {
            if args.xml.is_empty() {
                eprintln!("error: --xml <file> (repeatable) is required for `rank`");
                return usage();
            }
            let paths: Vec<String> = args.xml.iter().map(|p| p.display().to_string()).collect();
            // 0 => derive the batch from the worker count.
            let batch = if args.batch > 0 {
                args.batch
            } else {
                args.workers * args.per_worker
            };
            // Rank every candidate per module, then take the batch.
            let full = rank::rank_files(&paths, (batch.max(50)) / 5 + 50);
            // The batch: proportional per-module shares, so every module is
            // represented and the batch reaches beyond each module's head.
            let queue = rank::batch_by_quota(&full, batch);
            let dir = args.out.clone().unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: create {dir:?}: {e}");
                return ExitCode::FAILURE;
            }
            // The pursued queue: what the agent's workers fault, in order.
            let qpath = dir.join("pursued.json");
            let json = serde_json::to_string_pretty(&queue).expect("serialize queue");
            if let Err(e) = std::fs::write(&qpath, &json) {
                eprintln!("error: write {qpath:?}: {e}");
                return ExitCode::FAILURE;
            }
            // Worker slices: contiguous rank bands so each worker owns a
            // disjoint, equally-ranked share of the queue.
            let w = args.workers.max(1);
            let mut slices: Vec<Vec<&rank::RankedSignal>> = vec![Vec::new(); w];
            for (i, s) in queue.iter().enumerate() {
                slices[i % w].push(s);
            }
            let spath = dir.join("slices.json");
            let summary: Vec<serde_json::Value> = slices
                .iter()
                .enumerate()
                .map(|(i, sl)| {
                    serde_json::json!({
                        "worker": i,
                        "signals": sl.len(),
                        "top": sl.first().map(|s| format!("{}.{} [{}]", s.module, s.signal, s.score)),
                        "scores": sl.first().map(|s| s.score),
                    })
                })
                .collect();
            if let Err(e) = std::fs::write(&spath, serde_json::to_string_pretty(&summary).expect("slices")) {
                eprintln!("error: write {spath:?}: {e}");
                return ExitCode::FAILURE;
            }
            eprintln!(
                "batch of {} candidates across {} modules -> {} ({} workers)",
                queue.len(),
                {
                    let mut ms = std::collections::BTreeSet::new();
                    for s in &queue {
                        ms.insert(s.module.clone());
                    }
                    ms.len()
                },
                qpath.display(),
                w
            );
            for (i, sl) in slices.iter().enumerate().take(8) {
                if let Some(top) = sl.first() {
                    eprintln!("  worker {i}: {} signals, top {}.{} [{}]", sl.len(), top.module, top.signal, top.score);
                }
            }
            ExitCode::SUCCESS
        }
        "oracle" => {
            let ip = match &args.ip {
                Some(ip) => ip.clone(),
                None => { eprintln!("error: --ip is required for `oracle`"); return usage(); }
            };
            let s = match surface::discover(&args.tree, &ip) {
                Ok(s) => s,
                Err(e) => { eprintln!("error: {e}"); return ExitCode::FAILURE; }
            };
            let map_path = match &args.map {
                Some(m) => m.clone(),
                None => { eprintln!("error: --map is required for `oracle`"); return usage(); }
            };
            let resp_path = match &args.resp {
                Some(m) => m.clone(),
                None => { eprintln!("error: --resp is required for `oracle`"); return usage(); }
            };
            let map_txt = match std::fs::read_to_string(&map_path) {
                Ok(t) => t,
                Err(e) => { eprintln!("error: read {map_path:?}: {e}"); return ExitCode::FAILURE; }
            };
            let mut probes = Vec::new();
            for (n, line) in map_txt.lines().enumerate() {
                if line.trim().is_empty() { continue; }
                match serde_json::from_str::<campaign::Probe>(line) {
                    Ok(p) => probes.push(p),
                    Err(e) => { eprintln!("error: {map_path:?} line {}: {e}", n + 1); return ExitCode::FAILURE; }
                }
            }
            let resp_txt = match std::fs::read_to_string(&resp_path) {
                Ok(t) => t,
                Err(e) => { eprintln!("error: read {resp_path:?}: {e}"); return ExitCode::FAILURE; }
            };
            let lines: Vec<String> = resp_txt.lines().map(|l| l.to_string()).collect();
            let (findings, report) = oracle::analyse(&s, &probes, &lines);
            let bad = findings.iter().filter(|f| f.verdict == oracle::Verdict::UndetectedAccept).count();
            match &args.out {
                Some(p) => {
                    if let Err(e) = std::fs::write(p, &report) {
                        eprintln!("error: write {p:?}: {e}");
                        return ExitCode::FAILURE;
                    }
                    eprintln!("{}: {} probes, {} undetected acceptances -> {}", ip, findings.len(), bad, p.display());
                }
                None => {
                    print!("{report}");
                    eprintln!("{}: {} probes, {} undetected acceptances", ip, findings.len(), bad);
                }
            }
            ExitCode::SUCCESS
        }
        "attack" => {
            let ip = match &args.ip {
                Some(ip) => ip.clone(),
                None => {
                    eprintln!("error: --ip is required for `attack`");
                    return usage();
                }
            };
            let s = match surface::discover(&args.tree, &ip) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            if let Some(dir) = &args.split {
                // One probe per generated command stream: each simulation starts
                // from reset, so the alert baseline is clean. A mid-run clear was
                // tried and provably does not work (367/368 probes stayed dirty).
                if let Err(e) = std::fs::create_dir_all(dir) {
                    eprintln!("error: create {dir:?}: {e}");
                    return ExitCode::FAILURE;
                }
                let mut n = 0usize;
                loop {
                    let (cmds, map) = campaign::mubi_sweep(&s, Some(n));
                    if map.trim().is_empty() {
                        break;
                    }
                    let cp = dir.join(format!("cmds_{n:04}.hex"));
                    let mp = dir.join(format!("map_{n:04}.json"));
                    if let Err(e) = std::fs::write(&cp, &cmds).and_then(|_| std::fs::write(&mp, &map)) {
                        eprintln!("error: write probe {n}: {e}");
                        return ExitCode::FAILURE;
                    }
                    n += 1;
                }
                eprintln!("{ip}: split {n} single-probe streams -> {}", dir.display());
                return ExitCode::SUCCESS;
            }
            let (cmds, map) = campaign::mubi_sweep(&s, args.only);
            let dir = args.out.clone().unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: create {dir:?}: {e}");
                return ExitCode::FAILURE;
            }
            let cpath = dir.join("cmds.hex");
            let mpath = dir.join("cmds.map");
            if let Err(e) = std::fs::write(&cpath, &cmds).and_then(|_| std::fs::write(&mpath, &map)) {
                eprintln!("error: write campaign: {e}");
                return ExitCode::FAILURE;
            }
            let n = cmds.lines().filter(|l| *l == "0").count();
            let probes = map.lines().count().saturating_sub(1);
            eprintln!(
                "{}: encoding sweep with {} probes ({} writes) -> {}",
                ip,
                probes,
                n,
                cpath.display()
            );
            ExitCode::SUCCESS
        }
        "campaign" => {
            let ip = match &args.ip {
                Some(ip) => ip.clone(),
                None => {
                    eprintln!("error: --ip is required for `campaign`");
                    return usage();
                }
            };
            let s = match surface::discover(&args.tree, &ip) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let cmds = campaign::smoke(&s);
            let out = args
                .out
                .clone()
                .unwrap_or_else(|| PathBuf::from("cmds.hex"));
            if let Some(parent) = out.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Err(e) = std::fs::create_dir_all(parent) {
                        eprintln!("error: create {parent:?}: {e}");
                        return ExitCode::FAILURE;
                    }
                }
            }
            if let Err(e) = std::fs::write(&out, &cmds) {
                eprintln!("error: write {out:?}: {e}");
                return ExitCode::FAILURE;
            }
            let n = cmds.lines().count() / 3;
            eprintln!("{ip}: wrote {n} commands to {}", out.display());
            ExitCode::SUCCESS
        }
        "harness" => {
            let ip = match &args.ip {
                Some(ip) => ip.clone(),
                None => {
                    eprintln!("error: --ip is required for `harness`");
                    return usage();
                }
            };
            let s = match surface::discover(&args.tree, &ip) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            // Locate the IP top module source.
            let mut top: Option<(PathBuf, String)> = None;
            for base in [
                args.tree.join("hw/ip").join(&ip),
                args.tree.join("hw/top_earlgrey/ip_autogen").join(&ip),
                args.tree.join("hw/ip_templates").join(&ip),
            ] {
                let p = base.join("rtl").join(format!("{ip}.sv"));
                if p.is_file() {
                    match std::fs::read_to_string(&p) {
                        Ok(t) => {
                            top = Some((p, t));
                            break;
                        }
                        Err(e) => {
                            eprintln!("error: read {p:?}: {e}");
                            return ExitCode::FAILURE;
                        }
                    }
                }
            }
            let (top_path, top_src) = match top {
                Some(t) => t,
                None => {
                    eprintln!("error: no top module source found for {ip}");
                    return ExitCode::FAILURE;
                }
            };
            let h = match harness::generate(&s, &top_path, &top_src) {
                Ok(h) => h,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let dir = args
                .out
                .clone()
                .unwrap_or_else(|| PathBuf::from(format!("harness/{ip}")));
            if let Err(e) = std::fs::create_dir_all(&dir) {
                eprintln!("error: create {dir:?}: {e}");
                return ExitCode::FAILURE;
            }
            let tb_path = dir.join(format!("tb_{}.sv", h.ip));
            if let Err(e) = std::fs::write(&tb_path, &h.tb) {
                eprintln!("error: write {tb_path:?}: {e}");
                return ExitCode::FAILURE;
            }
            // Seed a command file holding only the sentinel, so the harness is
            // immediately runnable before any campaign is generated.
            let cmds = dir.join("cmds.hex");
            if !cmds.exists() {
                if let Err(e) = std::fs::write(&cmds, format!("{:#x}\n{:#x}\n{:#x}\n", 2u32, 0xdead_beefu32, 0u32)) {
                    eprintln!("error: write {cmds:?}: {e}");
                    return ExitCode::FAILURE;
                }
            }
            eprintln!(
                "{}: harness written to {} ({} registers discovered, tb + cmds.hex)",
                ip,
                dir.display(),
                s.registers.len()
            );
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("error: unknown subcommand `{other}`");
            usage()
        }
    }
}
