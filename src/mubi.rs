//! mubi — encoding-contract audit for MuBi (multi-bit, redundant-encoding) signals.
//!
//! The contract is the vendor's own, defined in `prim_mubi_pkg`:
//!   mubi4_test_true_strict(v)  = (MuBi4True  == v)  -> only the canonical value is "true"
//!   mubi4_test_true_loose(v)   = (MuBi4False != v)  -> EVERY value except canonical false
//!                                                     is "true", i.e. it fails open
//!   mubi4_test_false_loose(v)  = (MuBi4True  != v)  -> every value except canonical true
//!                                                     is "false"
//!
//! A loose test is not automatically a defect -- the package documents uses for it.
//! The defect signature is a loose CONSUMER whose siblings in the same file play the
//! same role (`_pfe`, `_pfa`, `_fo`) with the STRICT test on the same kind of fanout.
//! Same role, same interface, different rigour: the loose arm fails open on a faulted
//! encoding where its siblings refuse. That divergence is what this audit reports.
//!
//! This is a COMPLEMENT to fault injection, not a replacement: injection can only
//! reach storage (Verilator's `public_flat_rw` drops nets), so consumer-logic defects
//! like this class are invisible to it.

use std::fs;
use std::path::{Path, PathBuf};

/// One call site of a MuBi test.
pub struct Site {
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
}

fn skipped(dir: &Path) -> bool {
    let s = dir.to_string_lossy();
    s.contains("/dv/") || s.contains("/pre_dv/") || s.contains("/vendor/")
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if !skipped(&p) {
                walk(&p, out);
            }
        } else if p.extension().map(|x| x == "sv").unwrap_or(false) {
            out.push(p);
        }
    }
}

/// True if the line calls a loose MuBi test (and is not the function definition).
fn is_loose(line: &str) -> bool {
    (line.contains("_test_true_loose") || line.contains("_test_false_loose"))
        && !line.contains("function")
}

/// True if the line calls a strict MuBi test (and is not the function definition).
fn is_strict(line: &str) -> bool {
    (line.contains("_test_true_strict") || line.contains("_test_false_strict"))
        && !line.contains("function")
}

/// Role suffixes in a line: `foo_pfe`, `bar_pfa`, `baz_fo` -> "pfe"/"pfa"/"fo".
/// The role is what makes two sites siblings -- same interface, same duty.
pub fn role_suffixes(line: &str) -> Vec<String> {
    let mut v = Vec::new();
    for w in line.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
        for s in ["_pfe", "_pfa", "_fo"] {
            if w.ends_with(s) && w.len() > s.len() {
                v.push(s.trim_start_matches('_').to_string());
            }
        }
    }
    v
}

/// Scan the tree for MuBi test call sites.
pub fn scan(tree: &Path) -> (Vec<Site>, Vec<Site>) {
    let mut files = Vec::new();
    walk(tree, &mut files);
    let mut loose = Vec::new();
    let mut strict = Vec::new();
    for f in files {
        let text = match fs::read_to_string(&f) {
            Ok(t) => t,
            Err(_) => continue,
        };
        for (i, ln) in text.lines().enumerate() {
            let t = ln.trim();
            if t.starts_with("import") || t.starts_with("export") {
                continue;
            }
            if is_loose(ln) {
                loose.push(Site { file: f.clone(), line: i + 1, text: t.to_string() });
            } else if is_strict(ln) {
                strict.push(Site { file: f.clone(), line: i + 1, text: t.to_string() });
            }
        }
    }
    (loose, strict)
}

/// A ranked finding: a loose site plus its contradiction evidence.
pub struct Finding {
    pub score: u8,
    pub file: PathBuf,
    pub line: usize,
    pub text: String,
    pub shared_roles: Vec<String>,
    pub strict_siblings: usize,
}

/// Rank loose sites by how strongly their own file contradicts them.
pub fn rank(r: &Path, loose: &[Site], strict: &[Site]) -> Vec<Finding> {
    let rel = |p: &Path| p.strip_prefix(r).unwrap_or(p).to_string_lossy().to_string();

    // role suffixes that this file's strict sites use
    let mut strict_roles_by_file: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    let mut strict_count_by_file: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for s in strict {
        let k = rel(&s.file);
        for role in role_suffixes(&s.text) {
            strict_roles_by_file.entry(k.clone()).or_default().insert(role);
        }
        *strict_count_by_file.entry(k).or_insert(0) += 1;
    }

    let mut out = Vec::new();
    for s in loose {
        let k = rel(&s.file);
        let my_roles = role_suffixes(&s.text);
        let sroles = strict_roles_by_file.get(&k);
        let shared: Vec<String> = match sroles {
            Some(set) => {
                let mut v: Vec<String> =
                    my_roles.iter().filter(|x| set.contains(*x)).cloned().collect();
                v.sort();
                v.dedup();
                v
            }
            None => Vec::new(),
        };
        let nstrict = *strict_count_by_file.get(&k).unwrap_or(&0);
        let score = if !shared.is_empty() {
            3
        } else if nstrict > 0 {
            2
        } else {
            1
        };
        out.push(Finding {
            score,
            file: s.file.clone(),
            line: s.line,
            text: s.text.clone(),
            shared_roles: shared,
            strict_siblings: nstrict,
        });
    }
    out.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.file.cmp(&b.file))
            .then_with(|| a.line.cmp(&b.line))
    });
    out
}

/// Entry point for `rtlsec mubi-audit`.
pub fn audit(tree: &Path, verbose: bool) -> std::process::ExitCode {
    let (loose, strict) = scan(tree);
    println!("rtlsec mubi-audit --tree {}", tree.display());
    println!("  loose call sites : {}", loose.len());
    println!("  strict call sites: {}", strict.len());

    let findings = rank(tree, &loose, &strict);
    let strong = findings.iter().filter(|f| f.score == 3).count();
    let mid = findings.iter().filter(|f| f.score == 2).count();
    println!("  ranked findings  : score3={strong} (same-role strict sibling) score2={mid} score1={}",
             findings.len() - strong - mid);

    let shown: Vec<&Finding> = findings.iter().filter(|f| f.score >= 3 || verbose).collect();
    println!("\n=== loose sites ranked by contradiction strength ({}) ===", shown.len());
    for f in shown {
        let tag = match f.score {
            3 => "SAME-ROLE-STRICT-SIBLING",
            2 => "strict-elsewhere-in-file",
            _ => "no-strict-sibling",
        };
        println!("\n[{}] {}", f.score, tag);
        println!("    {}:{}", f.file.display(), f.line);
        println!("    {}", f.text);
        if !f.shared_roles.is_empty() {
            println!(
                "    role(s) {:?} also tested STRICTLY in this file ({} strict site(s) in file)",
                f.shared_roles, f.strict_siblings
            );
        }
    }
    println!();
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_suffix_extraction() {
        // a consumer line yields its role suffix, which is what makes two sites siblings
        let r = role_suffixes("assign rng_fips_pfe = mubi4_test_true_loose(x);");
        assert!(r.contains(&"pfe".to_string()));
        let r = role_suffixes("assign es_enable_fo = prim_mubi_pkg::mubi4_test_true_strict(x);");
        assert!(r.contains(&"fo".to_string()));
        assert!(role_suffixes("assign plain_signal = 1'b0;").is_empty());
    }

    #[test]
    fn loose_and_strict_detection() {
        assert!(is_loose("assign a = mubi4_test_true_loose(v);"));
        assert!(is_loose("assign a = prim_mubi_pkg::mubi4_test_false_loose(v);"));
        assert!(!is_loose("assign a = mubi4_test_true_strict(v);"));
        assert!(is_strict("assign a = mubi4_test_true_strict(v);"));
        // the package's own definition must not be reported as a call site
        assert!(!is_loose("  function automatic logic mubi4_test_true_loose(mubi4_t val);"));
        assert!(!is_strict("  function automatic logic mubi4_test_true_strict(mubi4_t val);"));
    }
}
