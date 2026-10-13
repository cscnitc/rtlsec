//! rtlsec-tool: per-file SystemVerilog static analyser for hardware-security
//! invariant violations. File-local (linter model): no cross-module elaboration.
//!
//! Rule classes implemented (see README.md):
//!   R1  Zeroization / wipe integrity (R1a unreachable, R1b no-op, R1c replicate, R1d impossible enable)
//!   R2  Select-arm vacuity
//!   R3  Comparator tautology
//!   R4  Gating parity
//!   R5  Constant / dead output
//!   R6  MuBi strict/loose divergence (sub-case of R7a)
//!   R7  Fault-injection / hardening-integrity gaps
//!       R7a loose-vs-strict consent test (generalises R6)
//!       R7b ungated integrity capture
//!       R7c non-sticky error accumulation
//!       R7d fault-escalation gap
//!       R7e error-attribution drop

pub mod engine;
pub mod facts;
pub mod frontend;
pub mod report;
pub mod rules;

use std::path::PathBuf;

/// A single candidate invariant violation. All findings are CANDIDATES
/// requiring human triage; the `confidence` field is an honest plausibility
/// estimate, not a verdict.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Rule id, e.g. "R1b" or "R7a".
    pub rule: String,
    /// Human name of the rule.
    pub rule_name: String,
    /// Sub-kind within the rule (e.g. "mubi" for R7a, "fsm-unreachable" for R1a).
    pub kind: String,
    /// Source file (may be an `\`include`d file if the construct came from one).
    pub file: PathBuf,
    /// 1-based line number in `file`.
    pub line: u32,
    /// Quoted source line(s).
    pub code: String,
    /// The syntactic construct that triggered the finding.
    pub construct: String,
    /// The sibling / reference construct it was compared against (mechanism evidence).
    pub compared_against: String,
    /// One-line statement of the violated invariant.
    pub invariant: String,
    /// Plausibility / confidence note.
    pub confidence: String,
    /// Free-form mechanism reasoning.
    pub detail: String,
    /// Enclosing module name (best effort).
    pub module: String,
}

// ---------------------------------------------------------------------------
// Shared text helpers
// ---------------------------------------------------------------------------

/// Collapse all whitespace out of an expression, for structural comparison.
pub fn norm(s: &str) -> String {
    s.split_whitespace().collect::<String>()
}

/// Normalise an expression more aggressively: drop all whitespace and strip a
/// single fully-enclosing paren pair, repeatedly.
pub fn norm_expr(s: &str) -> String {
    let mut t = norm(s);
    loop {
        let b = t.as_bytes();
        if b.len() >= 2 && b[0] == b'(' && b[b.len() - 1] == b')' && balanced_inner(&t) {
            t = t[1..t.len() - 1].to_string();
        } else {
            break;
        }
    }
    t
}

fn balanced_inner(s: &str) -> bool {
    let mut d = 0i32;
    for (i, c) in s.chars().enumerate() {
        match c {
            '(' => d += 1,
            ')' => {
                d -= 1;
                if d == 0 && i != s.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    d == 0
}

/// Root signal path of an expression: the leading identifier chain
/// (`a.b.c`), ignoring any indexing/select or surrounding operators.
/// Returns "" when the expression does not start with an identifier
/// (constants, unary operators, parenthesised subexpressions...).
pub fn signal_root(s: &str) -> String {
    let b: Vec<char> = s.trim().chars().collect();
    let is_start = |c: char| c.is_ascii_alphabetic() || c == '_' || c == '$';
    let is_cont = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    let mut i = 0usize;
    if b.is_empty() || !is_start(b[0]) {
        return String::new();
    }
    loop {
        while i < b.len() && is_cont(b[i]) {
            i += 1;
        }
        if i + 1 < b.len() && b[i] == '.' && is_start(b[i + 1]) {
            i += 1;
            continue;
        }
        break;
    }
    b[..i].iter().collect()
}

/// True when the expression is a plain literal constant.
pub fn is_const(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return false;
    }
    if let Some(pos) = t.find('\'') {
        let after = &t[pos + 1..];
        if !after.is_empty() {
            let c = after.chars().next().unwrap().to_ascii_lowercase();
            if "bodh".contains(c) || c == '0' || c == '1' {
                return after.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            }
        }
        return false;
    }
    t.chars().all(|c| c.is_ascii_digit() || c == '_')
}

/// True when the expression is a bare (non-indexed) identifier reference.
pub fn is_bare_ident(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty()
        && t.chars().next().map(|c| c.is_ascii_alphabetic() || c == '_').unwrap_or(false)
        && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Content of the first parenthesised group in `s` (used to pull the condition
/// out of an `if (...)` / `case (...)` text without touching generic AST types).
pub fn first_paren(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut start = None;
    let mut depth = 0i32;
    for (i, c) in b.iter().enumerate() {
        if *c == '(' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if *c == ')' {
            depth -= 1;
            if depth == 0 {
                if let Some(s0) = start {
                    return b[s0 + 1..i].iter().collect();
                }
            }
        }
    }
    String::new()
}

/// Everything after the first top-level `(` ... `)` group (the "body").
pub fn after_paren(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut start = None;
    let mut depth = 0i32;
    for (i, c) in b.iter().enumerate() {
        if *c == '(' {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if *c == ')' {
            depth -= 1;
            if depth == 0 && start.is_some() {
                return b[i + 1..].iter().collect();
            }
        }
    }
    String::new()
}

/// Case-item label: text before the first top-level ':'.
pub fn case_item_label(s: &str) -> String {
    let b: Vec<char> = s.chars().collect();
    let mut depth = 0i32;
    for (i, c) in b.iter().enumerate() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return b[..i].iter().collect::<String>().trim().to_string(),
            _ => {}
        }
    }
    s.trim().to_string()
}

/// Split an expression on a top-level binary separator (`&&` by default).
pub fn split_top(s: &str, sep: &str) -> Vec<String> {
    let sb: Vec<char> = sep.chars().collect();
    let b: Vec<char> = s.chars().collect();
    let mut depth = 0i32;
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            '(' | '[' | '{' => {
                depth += 1;
                cur.push(c);
                i += 1;
                continue;
            }
            ')' | ']' | '}' => {
                depth -= 1;
                cur.push(c);
                i += 1;
                continue;
            }
            _ => {}
        }
        if depth == 0 && i + sb.len() <= b.len() && b[i..i + sb.len()] == sb[..] {
            out.push(cur.trim().to_string());
            cur.clear();
            i += sb.len();
            continue;
        }
        cur.push(c);
        i += 1;
    }
    if !cur.trim().is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

/// Normalised atom for a gate term: strips leading unary negation and outer
/// parens so `!(a)` and `a` compare, `~x` and `x`.
pub fn gate_atom(t: &str) -> String {
    let mut s = norm_expr(t);
    loop {
        if s.starts_with('!') || s.starts_with('~') {
            s = s[1..].to_string();
            s = norm_expr(&s);
        } else {
            break;
        }
    }
    s
}

/// Trailing "role token" of a signal name: last `_`-separated word, digits and
/// trailing q/d/n/o/b suffixes stripped. Used to compare gating roles.
pub fn role_token(root: &str) -> String {
    let last = root.rsplit('.').next().unwrap_or(root);
    let mut t = last.to_string();
    // strip trailing _q/_d/_n/_o/_b/_i single-letter style suffixes repeatedly
    loop {
        let mut changed = false;
        for suf in ["_q", "_d", "_n", "_o", "_b", "_i"] {
            if t.ends_with(suf) && t.len() > suf.len() {
                t.truncate(t.len() - suf.len());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // strip trailing digits
    while t.len() > 1 && t.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        t.pop();
    }
    t
}

/// True when the text looks like a plain unsigned/size literal zero.
pub fn is_zero_literal(s: &str) -> bool {
    let t = norm(s);
    if t.is_empty() {
        return false;
    }
    if t == "0" || t == "1'b0" || t == "1'h0" || t == "1'd0" {
        return true;
    }
    if let Some(pos) = t.find('\'') {
        let after = &t[pos + 1..];
        if let Some(rest) = after.strip_prefix('b').or_else(|| after.strip_prefix('h')).or_else(|| after.strip_prefix('d')).or_else(|| after.strip_prefix('o')) {
            return !rest.is_empty() && rest.chars().all(|c| c == '0' || c == '_');
        }
        if after == "0" {
            return true;
        }
    }
    false
}
