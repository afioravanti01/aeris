//! Effect classification for capability operations (§ 8.1).
//!
//! Used by:
//! - M2.T7 — V2 mandatory-`intent` rule on write calls
//! - M2.T8 — saga rule: write `do` with `undo noop` is rejected
//! - M2.T11 — capability-escape rules
//!
//! The tables below are the canonical realisation of the read /
//! write / diagnostic table in `language.md` § 8.1.

use crate::syntax::ast::{Block, ElseBranch, Expr, MatchArm, Pattern, Stmt};
use crate::syntax::token::Span;

/// (module, operation) pairs classified as **write** by § 8.1.
///
/// `clock.now` and `random.next` are read-classified per the spec
/// (always recorded, never write-effectful). `io.print*` are
/// diagnostic — they do *not* trigger V2 / saga rules either.
const WRITE_OPS: &[(&str, &str)] = &[
    ("fs", "write_file"),
    ("fs", "write_text"),
    ("fs", "write_bytes"),
    ("fs", "mkdir"),
    ("fs", "remove"),
    ("fs", "rename"),
    ("http", "post"),
    ("http", "put"),
    ("http", "patch"),
    ("http", "delete"),
    ("shell", "exec"),
    ("shell", "pipe"),
    ("ai", "complete"),
    ("ai", "chat"),
    ("ai", "embed"),
    ("ai", "tools"),
    ("ai", "session_ask"),
    ("ai", "decide"),
    ("kube", "apply"),
    ("kube", "delete"),
    ("docker", "run"),
    ("docker", "build"),
    ("docker", "push"),
    ("mongodb", "write"),
    ("minio", "put"),
    ("rabbitmq", "publish"),
    ("audit", "event"),
    ("env", "set"),
];

/// Whether `<module>.<op>` is a write-classified capability operation.
pub fn is_write_op(module: &str, op: &str) -> bool {
    WRITE_OPS.iter().any(|(m, o)| *m == module && *o == op)
}

/// All `<module>.<op>` pairs that name a real capability operation
/// (read | write | diagnostic) per `language.md` § 8.1 / § 22 / § 23.
/// `xs.map(f)` is *not* a cap call and is therefore absent from the
/// table; the body-resolution rule (§ 8.2) only fires when the surface
/// `<module>.<op>` appears in this list.
const ALL_CAPS: &[(&str, &str)] = &[
    // L1 stdlib (effectful)
    ("io", "print"),
    ("io", "println"),
    ("io", "eprint"),
    ("io", "eprintln"),
    ("io", "read_line"),
    ("fs", "read_file"),
    ("fs", "read_text"),
    ("fs", "read_bytes"),
    ("fs", "write_file"),
    ("fs", "write_text"),
    ("fs", "write_bytes"),
    ("fs", "walk"),
    ("fs", "stat"),
    ("fs", "exists"),
    ("fs", "mkdir"),
    ("fs", "remove"),
    ("fs", "rename"),
    ("http", "get"),
    ("http", "post"),
    ("http", "put"),
    ("http", "patch"),
    ("http", "delete"),
    ("shell", "exec"),
    ("shell", "pipe"),
    ("env", "read"),
    ("env", "must_read"),
    ("env", "set"),
    ("clock", "now"),
    ("clock", "sleep"),
    ("random", "next"),
    ("date", "now"),
    ("date", "today"),
    ("date", "timestamp"),
    ("date", "format"),
    ("yaml", "parse"),
    ("yaml", "parse_file"),
    ("net", "http"),
    ("net", "tcp"),
    ("net", "udp"),
    ("net", "resolve"),
    ("ai", "network"),
    // L2 native handlers
    ("ai", "complete"),
    ("ai", "chat"),
    ("ai", "embed"),
    ("ai", "tools"),
    // M19 v0.3 — extended AI surface (all gated on ai.* caps below)
    ("ai", "session"),
    ("ai", "session_ask"),
    ("ai", "decide"),
    ("ai", "usage"),
    ("kube", "apply"),
    ("kube", "delete"),
    ("kube", "get"),
    ("kube", "watch"),
    ("docker", "run"),
    ("docker", "build"),
    ("docker", "push"),
    ("docker", "pull"),
    ("docker", "inspect"),
    ("mongodb", "read"),
    ("mongodb", "write"),
    ("minio", "get"),
    ("minio", "put"),
    ("rabbitmq", "publish"),
    ("rabbitmq", "subscribe"),
    ("audit", "event"),
];

/// Whether `<module>.<op>` names a real capability operation. Anything
/// outside this list is a regular method-call (`x.f(a)` sugar per § 5.4)
/// and is exempt from body-resolution / V2 checks.
pub fn is_cap_op(module: &str, op: &str) -> bool {
    ALL_CAPS.iter().any(|(m, o)| *m == module && *o == op)
}

// ====================================================================
// AST walkers
// ====================================================================

/// Whether `b` contains any write-classified capability call. Recurses
/// through nested blocks, lambda / spawn / intent bodies, control-flow
/// arms and call arguments.
pub fn block_has_write_call(b: &Block) -> bool {
    b.stmts.iter().any(stmt_has_write) || b.tail.as_deref().is_some_and(expr_has_write)
}

fn stmt_has_write(s: &Stmt) -> bool {
    match s {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } => expr_has_write(value),
        Stmt::For { iter, body, .. } => expr_has_write(iter) || block_has_write_call(body),
        Stmt::While { cond, body, .. } => expr_has_write(cond) || block_has_write_call(body),
        // M17.T3 — the deferred body is treated as an inlined exit
        // statement for write-effect analysis.
        Stmt::Defer { body, .. } => expr_has_write(body),
        Stmt::Expr(e) => expr_has_write(e),
    }
}

pub fn expr_has_write(e: &Expr) -> bool {
    if let Some((m, op)) = call_of_capability(e) {
        if is_write_op(m, op) {
            return true;
        }
    }
    match e {
        Expr::Call { callee, args, .. } => {
            expr_has_write(callee) || args.iter().any(|a| expr_has_write(&a.value))
        }
        Expr::Try { expr, .. }
        | Expr::Await { expr, .. }
        | Expr::Raise { expr, .. }
        | Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::IsCheck { expr, .. } => expr_has_write(expr),
        Expr::Binary { lhs, rhs, .. } => expr_has_write(lhs) || expr_has_write(rhs),
        Expr::Field { base, .. } => expr_has_write(base),
        Expr::Index { base, index, .. } => expr_has_write(base) || expr_has_write(index),
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            expr_has_write(cond)
                || block_has_write_call(then_blk)
                || match else_ {
                    Some(ElseBranch::Else(b)) => block_has_write_call(b),
                    Some(ElseBranch::ElseIf(e)) => expr_has_write(e),
                    None => false,
                }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            expr_has_write(scrutinee)
                || arms.iter().any(|a| {
                    a.guard.as_ref().is_some_and(expr_has_write) || expr_has_write(&a.body)
                })
        }
        Expr::Block(b, _) => block_has_write_call(b),
        Expr::Lambda { body, .. } | Expr::Spawn { body, .. } | Expr::IntentBlock { body, .. } => {
            block_has_write_call(body)
        }
        Expr::Tuple(es, _) | Expr::List(es, _) => es.iter().any(expr_has_write),
        Expr::Record(rl, _) => {
            rl.fields.iter().any(|f| expr_has_write(&f.value))
                || rl.spread.as_deref().is_some_and(expr_has_write)
        }
        Expr::Range { start, end, .. } => {
            start.as_deref().is_some_and(expr_has_write)
                || end.as_deref().is_some_and(expr_has_write)
        }
        Expr::Return { expr, .. } | Expr::Break { expr, .. } => {
            expr.as_deref().is_some_and(expr_has_write)
        }
        Expr::Assign { value, .. } => expr_has_write(value),
        _ => false,
    }
}

/// If `e` is a call whose callee is `<ident>.<ident>(...)`, return the
/// pair `(module, op)`. The body-resolution rule (§ 8.2) means such a
/// call resolves against the in-scope `cap` parameter; the type
/// checker (M2.T4) enforces the binding. Here we only classify by
/// surface name — this is sufficient for M2.T7 / M2.T8 / M2.T11.
pub fn call_of_capability(e: &Expr) -> Option<(&str, &str)> {
    if let Expr::Call { callee, .. } = e {
        if let Expr::Field { base, name, .. } = callee.as_ref() {
            if let Expr::Ident(m, _) = base.as_ref() {
                return Some((m.as_str(), name.as_str()));
            }
        }
    }
    None
}

// ====================================================================
//  M2.T7 — V2 mandatory `intent` walker
// ====================================================================

/// One V2 violation. `op` is the surface `<module>.<operation>` form;
/// `span` points at the offending call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V2Violation {
    pub op: String,
    pub span: Span,
}

/// Walk a block looking for write-classified capability calls that
/// have no enclosing `intent` block. `intent_active` is `true` if the
/// caller already established an intent context (e.g. saga-level
/// `intent "..."`); the caller is responsible for setting it.
pub fn collect_v2_violations(b: &Block, intent_active: bool) -> Vec<V2Violation> {
    let mut out = Vec::new();
    walk_block_v2(b, intent_active, &mut out);
    out
}

fn walk_block_v2(b: &Block, intent_active: bool, out: &mut Vec<V2Violation>) {
    for s in &b.stmts {
        walk_stmt_v2(s, intent_active, out);
    }
    if let Some(t) = &b.tail {
        walk_expr_v2(t, intent_active, out);
    }
}

fn walk_stmt_v2(s: &Stmt, intent_active: bool, out: &mut Vec<V2Violation>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } => {
            walk_expr_v2(value, intent_active, out)
        }
        Stmt::For { iter, body, .. } => {
            walk_expr_v2(iter, intent_active, out);
            walk_block_v2(body, intent_active, out);
        }
        Stmt::While { cond, body, .. } => {
            walk_expr_v2(cond, intent_active, out);
            walk_block_v2(body, intent_active, out);
        }
        // M17.T3 — V2 check sees `defer body` as if `body` were
        // inlined at every function exit point.
        Stmt::Defer { body, .. } => walk_expr_v2(body, intent_active, out),
        Stmt::Expr(e) => walk_expr_v2(e, intent_active, out),
    }
}

fn walk_expr_v2(e: &Expr, intent_active: bool, out: &mut Vec<V2Violation>) {
    // An `intent { ... }` block creates a fresh intent scope for its body.
    if let Expr::IntentBlock { body, .. } = e {
        walk_block_v2(body, true, out);
        return;
    }
    // Detect the offending call at this node *before* recursing into args.
    if !intent_active {
        if let Some((m, op)) = call_of_capability(e) {
            if is_write_op(m, op) {
                out.push(V2Violation {
                    op: format!("{m}.{op}"),
                    span: e.span(),
                });
            }
        }
    }
    match e {
        Expr::Call { callee, args, .. } => {
            walk_expr_v2(callee, intent_active, out);
            for a in args {
                walk_expr_v2(&a.value, intent_active, out);
            }
        }
        Expr::Try { expr, .. }
        | Expr::Await { expr, .. }
        | Expr::Raise { expr, .. }
        | Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::IsCheck { expr, .. } => walk_expr_v2(expr, intent_active, out),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr_v2(lhs, intent_active, out);
            walk_expr_v2(rhs, intent_active, out);
        }
        Expr::Field { base, .. } => walk_expr_v2(base, intent_active, out),
        Expr::Index { base, index, .. } => {
            walk_expr_v2(base, intent_active, out);
            walk_expr_v2(index, intent_active, out);
        }
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            walk_expr_v2(cond, intent_active, out);
            walk_block_v2(then_blk, intent_active, out);
            match else_ {
                Some(ElseBranch::Else(b)) => walk_block_v2(b, intent_active, out),
                Some(ElseBranch::ElseIf(e)) => walk_expr_v2(e, intent_active, out),
                None => {}
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr_v2(scrutinee, intent_active, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    walk_expr_v2(g, intent_active, out);
                }
                walk_expr_v2(&a.body, intent_active, out);
            }
        }
        Expr::Block(b, _) => walk_block_v2(b, intent_active, out),
        Expr::Lambda { body, .. } | Expr::Spawn { body, .. } => {
            // A lambda or spawn body inherits the *caller's* intent
            // scope: the lexical-ancestor rule (§ 10.1) does not chase
            // closure boundaries, but it does see the outer scope's
            // intent if the closure is built inside one.
            walk_block_v2(body, intent_active, out);
        }
        Expr::Tuple(es, _) | Expr::List(es, _) => {
            for x in es {
                walk_expr_v2(x, intent_active, out);
            }
        }
        Expr::Record(rl, _) => {
            for f in &rl.fields {
                walk_expr_v2(&f.value, intent_active, out);
            }
            if let Some(s) = &rl.spread {
                walk_expr_v2(s, intent_active, out);
            }
        }
        Expr::Range { start, end, .. } => {
            if let Some(s) = start {
                walk_expr_v2(s, intent_active, out);
            }
            if let Some(e2) = end {
                walk_expr_v2(e2, intent_active, out);
            }
        }
        Expr::Return { expr, .. } | Expr::Break { expr, .. } => {
            if let Some(e2) = expr {
                walk_expr_v2(e2, intent_active, out);
            }
        }
        Expr::Assign { value, .. } => walk_expr_v2(value, intent_active, out),
        // Atomic / no-op nodes
        Expr::IntentBlock { .. } => unreachable!("handled above"),
        _ => {}
    }
}

// ====================================================================
//  M2.T4 — body-resolution walker
// ====================================================================

/// One body-resolution violation: a `<module>.<op>(...)` call inside a
/// function body whose in-scope `cap` parameter does not list that
/// operation, or a function with no `cap` parameter at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapResolutionError {
    pub op: String,
    pub kind: CapResolutionKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapResolutionKind {
    /// No `cap` parameter is in lexical scope (pure function calling
    /// an effectful operation).
    NoCapInScope,
    /// `cap` exists but its effect signature does not include the
    /// requested `<module>.<op>` pair.
    OpNotInCap,
}

/// Collect body-resolution violations for a function body. `cap` is
/// the set of `(module, op)` pairs declared in the function's `cap`
/// parameter; pass `None` if the function has no `cap` parameter.
/// A bare-module entry `(m, "*")` matches every op of module `m`
/// per § 8.3 ("a capability tree node implies its leaves").
pub fn collect_cap_resolution_errors(
    body: &Block,
    cap: Option<&std::collections::HashSet<(String, String)>>,
) -> Vec<CapResolutionError> {
    let mut out = Vec::new();
    walk_block_cap(body, cap, &mut out);
    out
}

fn cap_set_covers(set: &std::collections::HashSet<(String, String)>, m: &str, op: &str) -> bool {
    // `("*", "*")` is the wildcard sentinel used by `fn main(cap)`
    // (§ 8.4): the synthesised cap is composed at runtime from the
    // manifest, so the body-resolution layer accepts any op.
    set.contains(&("*".to_string(), "*".to_string()))
        || set.contains(&(m.to_string(), op.to_string()))
        || set.contains(&(m.to_string(), "*".to_string()))
}

fn walk_block_cap(
    b: &Block,
    cap: Option<&std::collections::HashSet<(String, String)>>,
    out: &mut Vec<CapResolutionError>,
) {
    for s in &b.stmts {
        walk_stmt_cap(s, cap, out);
    }
    if let Some(t) = &b.tail {
        walk_expr_cap(t, cap, out);
    }
}

fn walk_stmt_cap(
    s: &Stmt,
    cap: Option<&std::collections::HashSet<(String, String)>>,
    out: &mut Vec<CapResolutionError>,
) {
    match s {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } => walk_expr_cap(value, cap, out),
        Stmt::For { iter, body, .. } => {
            walk_expr_cap(iter, cap, out);
            walk_block_cap(body, cap, out);
        }
        Stmt::While { cond, body, .. } => {
            walk_expr_cap(cond, cap, out);
            walk_block_cap(body, cap, out);
        }
        Stmt::Defer { body, .. } => walk_expr_cap(body, cap, out),
        Stmt::Expr(e) => walk_expr_cap(e, cap, out),
    }
}

fn walk_expr_cap(
    e: &Expr,
    cap: Option<&std::collections::HashSet<(String, String)>>,
    out: &mut Vec<CapResolutionError>,
) {
    if let Some((m, op)) = call_of_capability(e) {
        if is_cap_op(m, op) {
            match cap {
                None => out.push(CapResolutionError {
                    op: format!("{m}.{op}"),
                    kind: CapResolutionKind::NoCapInScope,
                    span: e.span(),
                }),
                Some(set) if !cap_set_covers(set, m, op) => out.push(CapResolutionError {
                    op: format!("{m}.{op}"),
                    kind: CapResolutionKind::OpNotInCap,
                    span: e.span(),
                }),
                _ => {}
            }
        }
    }
    match e {
        Expr::Call { callee, args, .. } => {
            walk_expr_cap(callee, cap, out);
            for a in args {
                walk_expr_cap(&a.value, cap, out);
            }
        }
        Expr::Try { expr, .. }
        | Expr::Await { expr, .. }
        | Expr::Raise { expr, .. }
        | Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::IsCheck { expr, .. } => walk_expr_cap(expr, cap, out),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr_cap(lhs, cap, out);
            walk_expr_cap(rhs, cap, out);
        }
        Expr::Field { base, .. } => walk_expr_cap(base, cap, out),
        Expr::Index { base, index, .. } => {
            walk_expr_cap(base, cap, out);
            walk_expr_cap(index, cap, out);
        }
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            walk_expr_cap(cond, cap, out);
            walk_block_cap(then_blk, cap, out);
            match else_ {
                Some(ElseBranch::Else(b)) => walk_block_cap(b, cap, out),
                Some(ElseBranch::ElseIf(e)) => walk_expr_cap(e, cap, out),
                None => {}
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr_cap(scrutinee, cap, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    walk_expr_cap(g, cap, out);
                }
                walk_expr_cap(&a.body, cap, out);
            }
        }
        Expr::Block(b, _) => walk_block_cap(b, cap, out),
        Expr::IntentBlock { body, .. } => walk_block_cap(body, cap, out),
        // Lambda closures inherit the enclosing `cap` (§ 7.3); a
        // `spawn { ... }` body does *not* — sharing cap across
        // threads is forbidden (§ 8.7), so the body sees `None`.
        Expr::Lambda { body, .. } => walk_block_cap(body, cap, out),
        Expr::Spawn { body, .. } => walk_block_cap(body, None, out),
        Expr::Tuple(es, _) | Expr::List(es, _) => {
            for x in es {
                walk_expr_cap(x, cap, out);
            }
        }
        Expr::Record(rl, _) => {
            for f in &rl.fields {
                walk_expr_cap(&f.value, cap, out);
            }
            if let Some(s) = &rl.spread {
                walk_expr_cap(s, cap, out);
            }
        }
        Expr::Range { start, end, .. } => {
            if let Some(s) = start {
                walk_expr_cap(s, cap, out);
            }
            if let Some(e2) = end {
                walk_expr_cap(e2, cap, out);
            }
        }
        Expr::Return { expr, .. } | Expr::Break { expr, .. } => {
            if let Some(e2) = expr {
                walk_expr_cap(e2, cap, out);
            }
        }
        Expr::Assign { value, .. } => walk_expr_cap(value, cap, out),
        _ => {}
    }
}

// ====================================================================
//  M2.T2 — match exhaustiveness walker (structural)
// ====================================================================

/// One match-exhaustiveness violation discovered in a function body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchViolation {
    pub kind: MatchViolationKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchViolationKind {
    EmptyMatch,
    AllGuardedNoCatchAll,
}

/// Walk a block collecting every `match` expression that fails the
/// structural exhaustiveness rule of § 17.2. Full type-aware
/// exhaustiveness (enum-variant coverage, list patterns) is layered
/// on later when scrutinee types are known.
pub fn collect_match_violations(b: &Block) -> Vec<MatchViolation> {
    let mut out = Vec::new();
    walk_block_match(b, &mut out);
    out
}

fn walk_block_match(b: &Block, out: &mut Vec<MatchViolation>) {
    for s in &b.stmts {
        walk_stmt_match(s, out);
    }
    if let Some(t) = &b.tail {
        walk_expr_match(t, out);
    }
}

fn walk_stmt_match(s: &Stmt, out: &mut Vec<MatchViolation>) {
    match s {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } => walk_expr_match(value, out),
        Stmt::For { iter, body, .. } => {
            walk_expr_match(iter, out);
            walk_block_match(body, out);
        }
        Stmt::While { cond, body, .. } => {
            walk_expr_match(cond, out);
            walk_block_match(body, out);
        }
        Stmt::Defer { body, .. } => walk_expr_match(body, out),
        Stmt::Expr(e) => walk_expr_match(e, out),
    }
}

fn walk_expr_match(e: &Expr, out: &mut Vec<MatchViolation>) {
    if let Expr::Match { arms, span, .. } = e {
        if let Some(kind) = classify_match(arms) {
            out.push(MatchViolation { kind, span: *span });
        }
    }
    match e {
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr_match(scrutinee, out);
            for a in arms {
                if let Some(g) = &a.guard {
                    walk_expr_match(g, out);
                }
                walk_expr_match(&a.body, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            walk_expr_match(callee, out);
            for a in args {
                walk_expr_match(&a.value, out);
            }
        }
        Expr::Try { expr, .. }
        | Expr::Await { expr, .. }
        | Expr::Raise { expr, .. }
        | Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::IsCheck { expr, .. } => walk_expr_match(expr, out),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr_match(lhs, out);
            walk_expr_match(rhs, out);
        }
        Expr::Field { base, .. } => walk_expr_match(base, out),
        Expr::Index { base, index, .. } => {
            walk_expr_match(base, out);
            walk_expr_match(index, out);
        }
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            walk_expr_match(cond, out);
            walk_block_match(then_blk, out);
            match else_ {
                Some(ElseBranch::Else(b)) => walk_block_match(b, out),
                Some(ElseBranch::ElseIf(e)) => walk_expr_match(e, out),
                None => {}
            }
        }
        Expr::Block(b, _)
        | Expr::IntentBlock { body: b, .. }
        | Expr::Lambda { body: b, .. }
        | Expr::Spawn { body: b, .. } => walk_block_match(b, out),
        Expr::Tuple(es, _) | Expr::List(es, _) => {
            for x in es {
                walk_expr_match(x, out);
            }
        }
        Expr::Record(rl, _) => {
            for f in &rl.fields {
                walk_expr_match(&f.value, out);
            }
            if let Some(s) = &rl.spread {
                walk_expr_match(s, out);
            }
        }
        Expr::Range { start, end, .. } => {
            if let Some(s) = start {
                walk_expr_match(s, out);
            }
            if let Some(e2) = end {
                walk_expr_match(e2, out);
            }
        }
        Expr::Return { expr, .. } | Expr::Break { expr, .. } => {
            if let Some(e2) = expr {
                walk_expr_match(e2, out);
            }
        }
        Expr::Assign { value, .. } => walk_expr_match(value, out),
        _ => {}
    }
}

/// Two structural cases produce diagnostics without type information:
/// empty matches, and matches whose arms are *all* guarded with no
/// unguarded catch-all (§ 17.2). All other shapes are accepted at
/// this layer; a richer type-aware pass will refine them later.
fn classify_match(arms: &[MatchArm]) -> Option<MatchViolationKind> {
    if arms.is_empty() {
        return Some(MatchViolationKind::EmptyMatch);
    }
    let has_unguarded_catch_all = arms.iter().any(|a| {
        a.guard.is_none() && matches!(a.pattern, Pattern::Wildcard(_) | Pattern::Bind(_, _))
    });
    let all_guarded = arms.iter().all(|a| a.guard.is_some());
    if all_guarded && !has_unguarded_catch_all {
        return Some(MatchViolationKind::AllGuardedNoCatchAll);
    }
    None
}
