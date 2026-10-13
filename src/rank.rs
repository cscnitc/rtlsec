//! rank.rs — the fault-injection checker oracle agent's signal triage.
//!
//! Reads Verilator `--xml-only` output, applies GENERIC filters (declared type
//! and name signatures only — never specific signal names), and emits the
//! ranked work queue the agent's workers pursue. Known FI-family signals are
//! used as VALIDATION only, never as filter inputs.
//!
//! No XML crate is used: a small tag-stack scanner extracts `<var>` elements
//! with their scope paths plus the dtype table. This keeps the crate
//! dependency-free and offline-buildable.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedSignal {
    pub rank: usize,
    pub module: String,
    pub signal: String,
    pub scope: String,
    pub vartype: String,
    pub width: Option<u32>,
    pub score: u32,
    pub why: Vec<String>,
    /// How many instance copies folded into this one pursued item.
    pub instances: usize,
    /// 0-based rank within the module's own queue.
    pub module_rank: usize,
}

fn re(pat: &str) -> Regex {
    Regex::new(pat).expect("rank regex")
}

fn has(pat: &str, s: &str) -> bool {
    re(pat).is_match(s)
}

/// Extract an attribute value from a tag string.
fn attr(tag: &str, name: &str) -> Option<String> {
    let r = re(&format!(r#"{name}\s*=\s*"([^"]*)""#));
    r.captures(tag).map(|c| c[1].to_string())
}

#[derive(Debug, Clone)]
struct Dtype {
    tag: String,
    name: Option<String>,
    left: Option<i64>,
    right: Option<i64>,
    sub: Option<String>,
}

fn num(s: &str) -> Option<i64> {
    s.parse::<i64>().ok()
}

fn width_of(dtypes: &BTreeMap<String, Dtype>, id: &str, depth: u8) -> Option<u32> {
    if id.is_empty() || depth > 4 {
        return None;
    }
    let d = dtypes.get(id)?;
    match d.tag.as_str() {
        "basicdtype" => match (d.left, d.right) {
            (Some(l), Some(r)) => Some((l - r).unsigned_abs() as u32 + 1),
            _ => Some(1),
        },
        // Same recursion set as the validated prototype: only these carry a
        // sub-type width. structdtype/memberdtype/paramtypedtype resolve to
        // None (unknown width), never guessed.
        "packarraydtype" | "unpackarraydtype" | "constdtype" | "refdtype"
        | "enumdtype" => width_of(dtypes, d.sub.as_deref().unwrap_or(""), depth + 1),
        _ => None,
    }
}

fn is_array_dtype(dtypes: &BTreeMap<String, Dtype>, id: &str) -> bool {
    matches!(
        dtypes.get(id).map(|d| d.tag.as_str()),
        Some("packarraydtype") | Some("unpackarraydtype")
    )
}

struct Scored {
    score: u32,
    module: String,
    signal: String,
    scope: String,
    vartype: String,
    width: Option<u32>,
    why: Vec<String>,
}

fn score_var(name: &str, vt: &str, w: Option<u32>, is_array: bool) -> (u32, Vec<String>) {
    let mut score = 0u32;
    let mut why = Vec::new();
    let mut add = |n: u32, tag: &str, cond: bool| {
        if cond {
            score += n;
            why.push(tag.to_string());
        }
    };
    let lower_vt = vt.to_ascii_lowercase();
    let is_mubi_type =
        has(r"^(mubi\d*_t|lc_tx_t)$", &lower_vt);
    let is_err_type = lower_vt == "err_t";
    let is_access_type = lower_vt.ends_with("_access_t");
    let mubi_name = has(r"mubi", name);
    let tier1 = has(
        r"(?i)(mubi|_pfe$|_pfe_|fips|oneway|_lock|read_lock|seed|^key|_key|secret|access)",
        name,
    );
    let tier2 = has(
        r"(?i)(_err|_fault|_alert|_intg|integ|_check|escalat|invalid|inv_state|disabled|denied|_en$|_enable|_valid|_sticky|_sts|_status|_fsm|state_d|state_q)",
        name,
    );
    // Encoded control: a nibble fault flips the whole decision.
    add(100, "mubi-type", is_mubi_type);
    // Error descriptor: faulting it blinds the detection path.
    add(100, "err-descriptor", is_err_type);
    // Access-control descriptor: gates reads (e.g. read locks).
    add(60, "access-descriptor", is_access_type);
    add(60, "mubi-name", mubi_name);
    add(50, "tier1", tier1);
    add(20, "tier2", tier2);
    // Sync-distribution fanout: where glitches land.
    add(
        40,
        "fanout-sync",
        has(r"(?i)(fanout|_sync$|^sync_)", name),
    );
    // Wide secret-carrying state: the asset itself.
    add(
        40,
        "asset-state",
        has(r"(?i)(seed|^key|_key|secret)", name)
            && (is_array || w.map(|x| x > 8).unwrap_or(false)),
    );
    // Single-bit security decision: one glitch flips it all.
    add(40, "decision-bit", w == Some(1) && (tier1 || tier2));
    // Event aggregation: the detection path's own output.
    add(30, "event-agg", has(r"(?i)^event_", name));
    // Narrow encoding surface: the canonical FI target width.
    add(
        30,
        &format!("enc-surface[{}b]", w.unwrap_or(0)),
        w.map(|x| x <= 8).unwrap_or(false) && (mubi_name || tier1),
    );
    (score, why)
}

/// Parse one Verilator XML file into scoped (scope_path, attr-map) vars plus
/// the dtype table.
fn parse_xml(
    text: &str,
) -> (Vec<(String, BTreeMap<String, String>)>, BTreeMap<String, Dtype>) {
    let mut dtypes: BTreeMap<String, Dtype> = BTreeMap::new();
    // dtype elements: <basicdtype ... id=".." .../> etc (self-closing or not).
    let dtype_re = re(r"<(\w*dtype)\b([^>]*)/?>");
    for c in dtype_re.captures_iter(text) {
        let tag = c[0].to_string();
        let (tname, attrs) = (c[1].to_string(), c[2].to_string());
        if let Some(id) = attr(&attrs, "id") {
            dtypes.insert(
                id,
                Dtype {
                    tag: tname,
                    name: attr(&attrs, "name").or_else(|| attr(&tag, "name")),
                    left: attr(&attrs, "left").as_deref().and_then(num),
                    right: attr(&attrs, "right").as_deref().and_then(num),
                    sub: attr(&attrs, "sub_dtype_id"),
                },
            );
        }
    }

    // Walk tags with a scope stack. Only the netlist subtree carries vars.
    let tag_re = re(r"<(/?)(\w+)\b([^>]*?)(/?)>");
    let mut stack: Vec<String> = Vec::new();
    let mut in_netlist = false;
    let mut vars: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    let scope_tags = ["module", "cell", "cells", "function", "task", "begin", "interface"];
    for c in tag_re.captures_iter(text) {
        let closing = &c[1] == "/";
        let tname = c[2].to_string();
        let attrs = c[3].to_string();
        let self_close = &c[4] == "/";
        if tname == "netlist" {
            in_netlist = !closing;
            continue;
        }
        if !in_netlist {
            continue;
        }
        if tname == "var" && !closing {
            let mut m = BTreeMap::new();
            let a_re = re(r#"(\w+)\s*=\s*"([^"]*)""#);
            for a in a_re.captures_iter(&attrs) {
                m.insert(a[1].to_string(), a[2].to_string());
            }
            vars.push((stack.join("."), m));
            continue;
        }
        if scope_tags.contains(&tname.as_str()) {
            if closing {
                stack.pop();
            } else if !self_close {
                let nm = attr(&attrs, "name").unwrap_or(tname.clone());
                stack.push(nm);
            }
        }
    }
    (vars, dtypes)
}

fn is_skipped(attrs: &BTreeMap<String, String>, name: &str, vt: &str) -> bool {
    if attrs.contains_key("param") || attrs.contains_key("localparam") {
        return true;
    }
    if vt == "int" || vt == "integer" {
        return true;
    }
    has(r"^(assert_|mubi\d+_test_(true|false|invalid)|prim_|F77_|V[A-Z])", name)
}

/// Rank the signals in the given XML files. Returns the pursued queue, ranked
/// per module, with each module contributing up to `per_module` signals.
///
/// Budgeting per module (rather than truncating one global list) matters:
/// scores are calibrated to be comparable only *within* a design, and a single
/// large IP would otherwise crowd every small one out of the queue entirely —
/// which is exactly how a real fault target gets silently dropped.
pub fn rank_files(paths: &[String], per_module: usize) -> Vec<RankedSignal> {
    let mut all: Vec<Scored> = Vec::new();
    for p in paths {
        let module = std::path::Path::new(p)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(p)
            .trim_start_matches('V')
            .to_string();
        let text = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let (vars, dtypes) = parse_xml(&text);
        for (scope, attrs) in &vars {
            let name = attrs.get("name").cloned().unwrap_or_default();
            if name.is_empty() {
                continue;
            }
            let vt = attrs.get("vartype").cloned().unwrap_or_default();
            if is_skipped(attrs, &name, &vt) {
                continue;
            }
            let w = attrs
                .get("dtype_id")
                .and_then(|id| width_of(&dtypes, id, 0));
            let is_array = attrs
                .get("dtype_id")
                .map(|id| is_array_dtype(&dtypes, id))
                .unwrap_or(false);
            let (score, why) = score_var(&name, &vt, w, is_array);
            if score > 0 {
                all.push(Scored {
                    score,
                    module: module.clone(),
                    signal: name,
                    scope: scope.clone(),
                    vartype: if vt.is_empty() {
                        "logic".to_string()
                    } else {
                        vt
                    },
                    width: w,
                    why,
                });
            }
        }
    }

    // One pursued item per net name: the agent faults the net, not each copy.
    let mut best: BTreeMap<String, Scored> = BTreeMap::new();
    let mut instances: BTreeMap<String, usize> = BTreeMap::new();
    for s in all {
        let key = format!("{}.{}", s.module, s.signal);
        *instances.entry(key.clone()).or_insert(0) += 1;
        match best.get(&key) {
            Some(old) if old.score >= s.score => {}
            _ => {
                best.insert(key, s);
            }
        }
    }

    // Per-module budgets.
    let mut by_mod: BTreeMap<String, Vec<Scored>> = BTreeMap::new();
    for (key, s) in best {
        let _ = key;
        by_mod.entry(s.module.clone()).or_default().push(s);
    }
    let mut selected: Vec<Scored> = Vec::new();
    for (_, mut v) in by_mod {
        v.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then(a.signal.cmp(&b.signal))
                .then(a.scope.cmp(&b.scope))
        });
        v.truncate(per_module);
        selected.extend(v);
    }
    // Global order for slicing: strongest first, then module name, then signal.
    selected.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.module.cmp(&b.module))
            .then(a.signal.cmp(&b.signal))
    });

    let mut mod_pos: BTreeMap<String, usize> = BTreeMap::new();
    let mut out = Vec::new();
    for (i, s) in selected.iter().enumerate() {
        let mr = *mod_pos.get(&s.module).unwrap_or(&0);
        mod_pos.insert(s.module.clone(), mr + 1);
        let key = format!("{}.{}", s.module, s.signal);
        out.push(RankedSignal {
            rank: i,
            module: s.module.clone(),
            signal: s.signal.clone(),
            scope: s.scope.clone(),
            vartype: s.vartype.clone(),
            width: s.width,
            score: s.score,
            why: s.why.clone(),
            instances: *instances.get(&key).unwrap_or(&1),
            module_rank: mr,
        });
    }
    out
}

/// Build the batch the agent actually processes.
///
/// Taking the global top-N by score would only ever fault the highest-scoring
/// signals and would silently drop every module that is not the biggest one.
/// Instead each module contributes a share proportional to its ranked list, so
/// the batch spans every module and reaches beyond each module's head — which
/// is what a search tool should do. Signals re-ordered into a module's head by
/// the triage step are included; nothing is inserted that the ranking did not
/// already produce as a candidate.
pub fn batch_by_quota(queue: &[RankedSignal], batch: usize) -> Vec<RankedSignal> {
    let mut by_mod: BTreeMap<String, Vec<&RankedSignal>> = BTreeMap::new();
    for s in queue {
        by_mod.entry(s.module.clone()).or_default().push(s);
    }
    let total: usize = by_mod.values().map(|v| v.len()).sum();
    if total <= batch {
        return queue.to_vec();
    }
    let mut out: Vec<RankedSignal> = Vec::new();
    for (_, v) in by_mod.iter() {
        let n = v.len();
        let quota = (((batch as f64) * (n as f64) / (total as f64)).round() as usize).max(1);
        let quota = quota.min(n);
        for s in v.iter().take(quota) {
            out.push((*s).clone());
        }
    }
    out.truncate(batch);
    renumber(&mut out);
    out
}

/// Recompute rank / module_rank as positions in the given order.
fn renumber(queue: &mut [RankedSignal]) {
    let mut mod_pos: BTreeMap<String, usize> = BTreeMap::new();
    for (i, s) in queue.iter_mut().enumerate() {
        s.rank = i;
        let c = mod_pos.entry(s.module.clone()).or_insert(0);
        s.module_rank = *c;
        *c += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI_XML: &str = r#"
<verilator_xml>
<netlist>
<module name="demo" origName="demo">
<var name="mubi_mode" dtype_id="10" vartype="mubi4_t" origName="mubi_mode"/>
<var name="mubi_mode" dtype_id="10" vartype="mubi4_t" origName="mubi_mode"/>
<var name="AlertSkewCycles" dtype_id="2" vartype="int" origName="AlertSkewCycles" param="true"/>
<var name="seed_err_q" dtype_id="6" vartype="logic" origName="seed_err_q"/>
<var name="clk_i" dtype_id="6" vartype="logic" origName="clk_i"/>
<basicdtype id="10" name="logic" left="3" right="0"/>
<basicdtype id="6" name="logic"/>
<basicdtype id="2" name="logic" left="31" right="0"/>
</module>
</netlist>
</verilator_xml>"#;

    #[test]
    fn filters_and_scores() {
        let (vars, dtypes) = parse_xml(MINI_XML);
        assert_eq!(vars.len(), 5);
        let w = width_of(&dtypes, "10", 0);
        assert_eq!(w, Some(4));
        assert_eq!(width_of(&dtypes, "6", 0), Some(1));
    }

    #[test]
    fn mubi_outranks_plain_seed_err() {
        let (s1, _) = score_var("mubi_mode", "mubi4_t", Some(4), false);
        let (s2, _) = score_var("seed_err_q", "logic", Some(1), false);
        assert!(s1 > s2, "{s1} vs {s2}");
        // seed_err: tier1(50)+tier2(20)+decision(40)+enc(30) = 140
        assert_eq!(s2, 140);
    }

    #[test]
    fn skips_params_and_ints() {
        let mut m = BTreeMap::new();
        m.insert("param".to_string(), "true".to_string());
        assert!(is_skipped(&m, "X", "logic"));
        let m2 = BTreeMap::new();
        assert!(is_skipped(&m2, "i", "int"));
        assert!(!is_skipped(&m2, "seed_err_q", "logic"));
    }
}
