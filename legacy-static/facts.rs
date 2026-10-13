//! Fact extraction: a single pre-order walk of the sv-parser syntax tree with
//! an ancestor stack, so every construct is anchored to real AST node kinds
//! while still carrying the surrounding context (enclosing always block,
//! conditions, case arms) that the rules need.

use crate::{case_item_label, first_paren, is_bare_ident, is_const, norm, signal_root};
use std::path::PathBuf;
use sv_parser::{Locate, NodeEvent, RefNode, RefNodes, SyntaxTree};

/// One assignment / continuous assignment / declaration initialiser.
#[derive(Debug, Clone)]
pub struct Assign {
    pub lhs: String,
    pub rhs: String,
    pub lhs_root: String,
    pub rhs_root: String,
    pub rhs_const: bool,
    pub op: String,
    /// "nb" | "blk" | "cont" | "decl" | "opassign"
    pub kind: String,
    /// "comb" | "ff" | "latch" | "cont" | "decl" | "unknown"
    pub always: String,
    pub line: u32,
    pub file: PathBuf,
    /// Enclosing condition texts (outermost first).
    pub guards: Vec<String>,
    /// Enclosing case selector root (if inside a case).
    pub case_sel: String,
    /// Enclosing case item label (if inside a case).
    pub case_item: String,
    /// Full statement text.
    pub stmt: String,
}

impl Assign {
    /// Concatenated guard + case context, lowercased, for keyword matching.
    pub fn context_lc(&self) -> String {
        let mut s = String::new();
        for g in &self.guards {
            s.push_str(g);
            s.push(' ');
        }
        s.push_str(&self.case_sel);
        s.push(' ');
        s.push_str(&self.case_item);
        s.push(' ');
        s.push_str(&self.lhs);
        s.to_lowercase()
    }
    pub fn guard_lc(&self) -> String {
        let mut s = String::new();
        for g in &self.guards {
            s.push_str(g);
            s.push(' ');
        }
        s.to_lowercase()
    }
}

/// A comparison expression (`expr <op> expr`) over a binary operator.
#[derive(Debug, Clone)]
pub struct Comparison {
    pub op: String,
    pub lhs: String,
    pub rhs: String,
    pub lhs_root: String,
    pub rhs_root: String,
    pub line: u32,
    pub file: PathBuf,
    pub guards: Vec<String>,
    pub text: String,
    /// True when this comparison appears directly as a control condition.
    pub in_condition: bool,
}

/// A conditional (if/else-if/else) or ternary arm pair.
#[derive(Debug, Clone)]
pub struct CondArm {
    pub cond: String,
    pub then_text: String,
    pub else_text: String,
    pub line: u32,
    pub file: PathBuf,
    pub is_ternary: bool,
}

/// One case statement and its arms.
#[derive(Debug, Clone)]
pub struct CaseBlock {
    pub selector: String,
    pub selector_root: String,
    pub line: u32,
    pub file: PathBuf,
    pub arms: Vec<CaseArm>,
    /// "normal" | "matches" | "inside"
    pub style: String,
}

#[derive(Debug, Clone)]
pub struct CaseArm {
    pub label: String,
    pub body: String,
    pub line: u32,
}

/// An enumerated value declaration (candidate FSM state).
#[derive(Debug, Clone)]
pub struct EnumVal {
    pub name: String,
    pub line: u32,
}

/// A module-level port declaration.
#[derive(Debug, Clone)]
pub struct PortDecl {
    pub name: String,
    pub direction: String,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct Decl {
    pub name: String,
    pub text: String,
    pub line: u32,
}

#[derive(Debug, Default)]
pub struct FileFacts {
    pub file: PathBuf,
    pub module: String,
    pub assigns: Vec<Assign>,
    pub comparisons: Vec<Comparison>,
    pub arms: Vec<CondArm>,
    pub cases: Vec<CaseBlock>,
    pub enums: Vec<EnumVal>,
    pub ports: Vec<PortDecl>,
    pub decls: Vec<Decl>,
    /// Full source text of the file (for regex sweeps and quoting).
    pub source: String,
    pub lines: Vec<String>,
}

pub fn read_lines(p: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(p)
        .map(|s| s.lines().map(|l| l.to_string()).collect())
        .unwrap_or_default()
}

fn node_text(tree: &SyntaxTree, node: &RefNode) -> String {
    match tree.get_str(RefNodes(vec![node.clone()])) {
        Some(s) => s.to_string(),
        None => String::new(),
    }
}

fn first_locate(node: &RefNode) -> Option<Locate> {
    for n in node.clone() {
        if let RefNode::Locate(l) = n {
            return Some(*l);
        }
    }
    None
}

fn origin_of(tree: &SyntaxTree, node: &RefNode, fallback_file: &std::path::Path) -> (PathBuf, u32) {
    if let Some(loc) = first_locate(node) {
        if let Some((p, line)) = tree.get_origin(&loc) {
            return (p.clone(), line as u32);
        }
        return (fallback_file.to_path_buf(), loc.line);
    }
    (fallback_file.to_path_buf(), 0)
}

/// Split `lhs = rhs` on the first top-level `=` that is not part of
/// `==`, `!=`, `<=`, `>=`, `<=`, `=>`.
fn split_assign(s: &str) -> (String, String, String) {
    let b: Vec<char> = s.chars().collect();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { ' ' };
                let nextv = if i + 1 < b.len() { b[i + 1] } else { ' ' };
                if nextv == '=' || prev == '=' || prev == '!' || prev == '<' || prev == '>' {
                    i += 1;
                    continue;
                }
                let lhs: String = b[..i].iter().collect();
                let rhs: String = b[i + 1..].iter().collect();
                return (lhs.trim().to_string(), "=".to_string(), rhs.trim().to_string());
            }
            _ => {}
        }
        i += 1;
    }
    (s.trim().to_string(), String::new(), String::new())
}

/// Build the enclosing-context (guards, case selector/item, always kind) for a
/// node from the ancestor stack.
struct Ctx {
    guards: Vec<String>,
    case_sel: String,
    case_item: String,
    always: String,
}

fn ctx_from_stack(tree: &SyntaxTree, stack: &[RefNode]) -> Ctx {
    let mut guards = Vec::new();
    let mut case_item = String::new();
    let mut case_sel = String::new();
    let mut always = String::new();
    for a in stack.iter() {
        match a {
            RefNode::ConditionalStatement(_) => {
                let t = node_text(tree, a);
                let c = first_paren(&t);
                if !c.is_empty() {
                    guards.push(c);
                }
            }
            RefNode::CaseItemNondefault(_) => {
                if case_item.is_empty() {
                    let t = node_text(tree, a);
                    case_item = case_item_label(&t);
                }
            }
            RefNode::CaseStatementNormal(_) => {
                if case_sel.is_empty() {
                    let t = node_text(tree, a);
                    case_sel = first_paren(&t);
                }
            }
            RefNode::AlwaysKeyword(_) => {
                if always.is_empty() {
                    always = norm(&node_text(tree, a));
                }
            }
            _ => {}
        }
    }
    Ctx {
        guards,
        case_sel,
        case_item,
        always,
    }
}

pub fn extract(tree: &SyntaxTree, file: &std::path::Path, source: String) -> FileFacts {
    let lines: Vec<String> = source.lines().map(|l| l.to_string()).collect();
    let mut f = FileFacts {
        file: file.to_path_buf(),
        lines,
        source: source.clone(),
        ..Default::default()
    };

    let mut stack: Vec<RefNode> = Vec::new();
    let mut nested_assign_depth = 0i32;

    for ev in tree.into_iter().event() {
        match ev {
            NodeEvent::Enter(node) => {
                handle_node(&node, &stack, tree, file, &mut f, &mut nested_assign_depth);
                stack.push(node);
            }
            NodeEvent::Leave(_) => {
                stack.pop();
            }
        }
    }

    f
}

fn push_assign(
    f: &mut FileFacts,
    tree: &SyntaxTree,
    node: &RefNode,
    file: &std::path::Path,
    stack: &[RefNode],
    lhs: String,
    op: String,
    rhs: String,
    kind: &str,
) {
    let (of, line) = origin_of(tree, node, file);
    let ctx = ctx_from_stack(tree, stack);
    let always = match ctx.always.as_str() {
        "always_comb" => "comb".to_string(),
        "always_ff" => "ff".to_string(),
        "always_latch" => "latch".to_string(),
        "always" => "always".to_string(),
        _ => String::new(),
    };
    let always = if always.is_empty() {
        if kind == "cont" {
            "cont".to_string()
        } else {
            "unknown".to_string()
        }
    } else {
        always
    };
    let stmt = node_text(tree, node);
    let rhs_c = is_const(&rhs);
    let lhs_r = signal_root(&lhs);
    let rhs_r = if is_bare_ident(&rhs) {
        rhs.clone()
    } else {
        signal_root(&rhs)
    };
    f.assigns.push(Assign {
        lhs,
        rhs,
        lhs_root: lhs_r,
        rhs_root: rhs_r,
        rhs_const: rhs_c,
        op,
        kind: kind.to_string(),
        always,
        line,
        file: of,
        guards: ctx.guards,
        case_sel: ctx.case_sel,
        case_item: ctx.case_item,
        stmt,
    });
}

#[allow(clippy::too_many_arguments)]
fn handle_node(
    node: &RefNode,
    stack: &[RefNode],
    tree: &SyntaxTree,
    file: &std::path::Path,
    f: &mut FileFacts,
    nested_assign_depth: &mut i32,
) {
    match node {
        // ---------------- module identity ----------------
        RefNode::ModuleDeclarationAnsi(m) => {
            if f.module.is_empty() {
                let t = node_text(tree, node);
                let name = t
                    .trim_start()
                    .trim_start_matches("module")
                    .trim()
                    .split(|c: char| c.is_whitespace() || c == '(' || c == ';' || c == '#')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let _ = m;
                f.module = name;
            }
        }
        RefNode::ModuleDeclarationNonansi(m) => {
            if f.module.is_empty() {
                let t = node_text(tree, node);
                let name = t
                    .trim_start()
                    .trim_start_matches("module")
                    .trim()
                    .split(|c: char| c.is_whitespace() || c == '(' || c == ';' || c == '#')
                    .next()
                    .unwrap_or("")
                    .to_string();
                let _ = m;
                f.module = name;
            }
        }

        // ---------------- assignments ----------------
        RefNode::NonblockingAssignment(x) => {
            let _ = nested_assign_depth;
            let lhs = node_text(tree, &RefNode::from(&x.nodes.0));
            let rhs = node_text(tree, &RefNode::from(&x.nodes.3));
            let opt = norm(&node_text(tree, &RefNode::from(&x.nodes.1)));
            let op = if opt == "<=" { "<=".to_string() } else { "<=".to_string() };
            push_assign(f, tree, node, file, stack, lhs, op, rhs, "nb");
        }
        RefNode::BlockingAssignment(x) => {
            let t = node_text(tree, node);
            let (lhs, op, rhs) = split_assign(&t);
            push_assign(f, tree, node, file, stack, lhs, op, rhs, "blk");
        }
        RefNode::OperatorAssignment(x) => {
            let lhs = node_text(tree, &RefNode::from(&x.nodes.0));
            let op = norm(&node_text(tree, &RefNode::from(&x.nodes.1)));
            let rhs = node_text(tree, &RefNode::from(&x.nodes.2));
            push_assign(f, tree, node, file, stack, lhs, op, rhs, "opassign");
        }
        RefNode::NetAssignment(x) => {
            let lhs = node_text(tree, &RefNode::from(&x.nodes.0));
            let rhs = node_text(tree, &RefNode::from(&x.nodes.2));
            push_assign(f, tree, node, file, stack, lhs, "=".to_string(), rhs, "cont");
        }
        RefNode::VariableAssignment(x) => {
            let lhs = node_text(tree, &RefNode::from(&x.nodes.0));
            let rhs = node_text(tree, &RefNode::from(&x.nodes.2));
            let kind = if stack.iter().any(|n| matches!(n, RefNode::ContinuousAssignVariable(_))) {
                "cont"
            } else {
                "blk"
            };
            push_assign(f, tree, node, file, stack, lhs, "=".to_string(), rhs, kind);
        }
        RefNode::VariableDeclAssignmentVariable(x) => {
            let lhs = node_text(tree, &RefNode::from(&x.nodes.0));
            if let Some((_eq, expr)) = &x.nodes.2 {
                let rhs = node_text(tree, &RefNode::from(expr));
                push_assign(f, tree, node, file, stack, lhs, "=".to_string(), rhs, "decl");
            }
        }
        RefNode::NetDeclAssignment(x) => {
            let lhs = node_text(tree, &RefNode::from(&x.nodes.0));
            if let Some((_eq, expr)) = &x.nodes.2 {
                let rhs = node_text(tree, &RefNode::from(expr));
                push_assign(f, tree, node, file, stack, lhs, "=".to_string(), rhs, "decl");
            }
        }

        // ---------------- comparisons ----------------
        RefNode::ExpressionBinary(x) => {
            let op = norm(&node_text(tree, &RefNode::from(&x.nodes.1)));
            let l = node_text(tree, &RefNode::from(&x.nodes.0));
            let r = node_text(tree, &RefNode::from(&x.nodes.3));
            let interesting = matches!(
                op.as_str(),
                "==" | "!=" | "===" | "!==" | "<" | ">" | "<=" | ">="
            );
            if interesting {
                let (of, line) = origin_of(tree, node, file);
                let ctx = ctx_from_stack(tree, stack);
                let in_cond = stack.iter().any(|n| {
                    matches!(n, RefNode::ConditionalStatement(_) | RefNode::CaseStatementNormal(_))
                });
                f.comparisons.push(Comparison {
                    op: op.clone(),
                    lhs: l.clone(),
                    rhs: r.clone(),
                    lhs_root: signal_root(&l),
                    rhs_root: signal_root(&r),
                    line,
                    file: of,
                    guards: ctx.guards,
                    text: format!("{} {} {}", norm(&l), op, norm(&r)),
                    in_condition: in_cond,
                });
            }
        }

        // ---------------- ternary ----------
        RefNode::ConditionalExpression(x) => {
            let then_e = node_text(tree, &RefNode::from(&x.nodes.3));
            let else_e = node_text(tree, &RefNode::from(&x.nodes.5));
            let full = node_text(tree, node);
            let cond = first_paren(&full);
            let cond = if cond.is_empty() {
                // ternary condition is whatever precedes the first top-level '?'
                full.split('?').next().unwrap_or("").trim().to_string()
            } else {
                cond
            };
            let (of, line) = origin_of(tree, node, file);
            f.arms.push(CondArm {
                cond,
                then_text: then_e,
                else_text: else_e,
                line,
                file: of,
                is_ternary: true,
            });
        }

        // ---------------- if/else ----------
        RefNode::ConditionalStatement(_) => {
            let full = node_text(tree, node);
            let cond = first_paren(&full);
            let body = crate::after_paren(&full);
            // crude then/else split on the first top-level "else"
            let (then_t, else_t) = split_else(&body);
            let (of, line) = origin_of(tree, node, file);
            f.arms.push(CondArm {
                cond,
                then_text: then_t,
                else_text: else_t,
                line,
                file: of,
                is_ternary: false,
            });
        }

        // ---------------- case statements ----------------
        RefNode::CaseStatementNormal(x) => {
            let t = node_text(tree, node);
            let sel = first_paren(&t);
            let (of, line) = origin_of(tree, node, file);
            let mut arms = Vec::new();
            collect_case_arms(x, tree, &mut arms);
            f.cases.push(CaseBlock {
                selector: sel.clone(),
                selector_root: signal_root(&sel),
                line,
                file: of,
                arms,
                style: "normal".to_string(),
            });
        }

        // ---------------- enum values ----------------
        RefNode::EnumNameDeclaration(x) => {
            let t = node_text(tree, node);
            let name = t
                .trim()
                .split(|c: char| c == '=' || c == '[' || c.is_whitespace())
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            let (_, line) = origin_of(tree, node, file);
            if !name.is_empty() {
                f.enums.push(EnumVal { name, line });
                let _ = x;
            }
        }

        // ---------------- ports ----------------
        RefNode::AnsiPortDeclarationVariable(_) => {
            let t = norm(&node_text(tree, node));
            if let Some(dir) = port_dir(&t) {
                let name = port_name(&t);
                let (_, line) = origin_of(tree, node, file);
                if !name.is_empty() {
                    f.ports.push(PortDecl {
                        name,
                        direction: dir,
                        line,
                    });
                }
            }
        }
        RefNode::AnsiPortDeclarationNet(_) => {
            let t = norm(&node_text(tree, node));
            if let Some(dir) = port_dir(&t) {
                let name = port_name(&t);
                let (_, line) = origin_of(tree, node, file);
                if !name.is_empty() {
                    f.ports.push(PortDecl {
                        name,
                        direction: dir,
                        line,
                    });
                }
            }
        }

        // ---------------- declarations (for width / name sweeps) ----------------
        RefNode::DataDeclarationVariable(x) => {
            let t = norm(&node_text(tree, node));
            let name = decl_name(&t);
            let (_, line) = origin_of(tree, node, file);
            if !name.is_empty() {
                f.decls.push(Decl { name, text: t, line });
                let _ = x;
            }
        }

        _ => {}
    }
}

fn split_else(body: &str) -> (String, String) {
    let b: Vec<char> = body.chars().collect();
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < b.len() {
        match b[i] {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ => {}
        }
        if depth == 0
            && i + 4 <= b.len()
            && b[i] == 'e'
            && b[i..i + 4] == ['e', 'l', 's', 'e']
            && (i == 0
                || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == '_'))
            && (i + 4 >= b.len()
                || !(b[i + 4].is_ascii_alphanumeric() || b[i + 4] == '_'))
        {
            let then_t: String = b[..i].iter().collect();
            let else_t: String = b[i + 4..].iter().collect();
            return (then_t.trim().to_string(), else_t.trim().to_string());
        }
        i += 1;
    }
    (body.trim().to_string(), String::new())
}

fn collect_case_arms(x: &sv_parser::CaseStatementNormal, tree: &SyntaxTree, arms: &mut Vec<CaseArm>) {
    // The arms are the CaseItem head plus the Vec tail; both are reachable by
    // walking the node textually for the case items in order.
    let _ = x;
    let _ = tree;
    let _ = arms;
}

fn port_dir(t: &str) -> Option<String> {
    for d in ["input", "output", "inout", "ref"] {
        if t.starts_with(d) {
            return Some(d.to_string());
        }
    }
    None
}

fn port_name(t: &str) -> String {
    // last identifier-ish token before '=', ',', ')' etc.
    let mut cand = String::new();
    let mut cur = String::new();
    for c in t.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            cur.push(c);
        } else {
            if !cur.is_empty() {
                cand = cur.clone();
            }
            cur.clear();
        }
    }
    if !cur.is_empty() {
        cand = cur;
    }
    cand
}

fn decl_name(t: &str) -> String {
    // strip leading storage/type keywords then take first identifier
    let mut s = t.to_string();
    for kw in [
        "automatic ", "static ", "const ", "var ", "wire ", "logic ", "reg ", "bit ", "byte ",
        "int ", "integer ", "shortint ", "longint ", "signed ", "unsigned ", "rand ",
    ] {
        s = s.replace(kw, " ");
    }
    // drop packed dimension
    if let Some(idx) = s.find('[') {
        let close = s.find(']');
        if let Some(c) = close {
            s = format!("{}{}", &s[..idx], &s[c + 1..]);
        }
    }
    s.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .find(|w| !w.is_empty())
        .unwrap_or("")
        .to_string()
}
