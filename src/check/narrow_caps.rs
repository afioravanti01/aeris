//! V1 narrow-caps linter (M12.T6).
//!
//! Realises `docs/language.md` § 8.5: for every `fn` that takes a
//! `cap` parameter, derive the actually-used `(module, op)` set
//! (and, where statically extractable, the host allow-list for
//! `http.*` calls) and compare it to the declared signature. The
//! tool is a **linter**, not an inferencer — it never rewrites the
//! file silently. Callers (`aeris fmt --narrow-caps`) print the
//! suggested diff so the developer can apply it.
//!
//! The intended authoring pattern is *generation loose, fmt tight*:
//! an LLM (or a human) writes a coarse signature like
//! `cap[http, kube, audit.event]`; this linter narrows it to the
//! body's actual usage.

use std::collections::BTreeSet;

use super::effects;
use crate::syntax::ast::{Block, CallArg, ElseBranch, Expr, FnDecl, Item, Module, Stmt, Type};
use crate::syntax::token::Span;

/// One suggested cap narrowing for a single function.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapNarrowing {
    pub fn_name: String,
    pub declared: String,
    pub narrowed: String,
    pub span: Span,
}

/// Walk `m` and produce one `CapNarrowing` per function whose
/// declared `cap` parameter is broader than its body actually uses.
pub fn narrow_caps(m: &Module) -> Vec<CapNarrowing> {
    let mut out: Vec<CapNarrowing> = Vec::new();
    for item in &m.items {
        if let Item::Fn(f) = item {
            if let Some(n) = narrow_fn(f) {
                out.push(n);
            }
        }
    }
    out
}

fn narrow_fn(f: &FnDecl) -> Option<CapNarrowing> {
    let cap_param = f.params.iter().find(|p| p.name == "cap")?;
    let (entries, star) = match &cap_param.ty {
        Type::Cap { entries, star, .. } => (entries.clone(), *star),
        _ => return None,
    };
    if star {
        // `cap[*]` is M2.T5's concern — never suggest narrowing here.
        return None;
    }
    // 1. What does the body actually use?
    let mut used_ops: BTreeSet<(String, String)> = BTreeSet::new();
    let mut http_hosts: BTreeSet<String> = BTreeSet::new();
    walk_block(&f.body, &mut used_ops, &mut http_hosts);
    // 2. Build the narrowed entry list. Each declared entry contributes
    //    the operations that actually fire under its umbrella; bare
    //    `cap[<module>]` expands to the leaves the body touched.
    let mut narrowed: Vec<NarrowedEntry> = Vec::new();
    for entry in &entries {
        match entry.path.segments.as_slice() {
            [m_] => {
                // Whole-module entry — narrow to the leaves used.
                for (used_m, used_op) in &used_ops {
                    if used_m == m_ {
                        narrowed.push(NarrowedEntry {
                            module: used_m.clone(),
                            op: used_op.clone(),
                            allow: narrow_allow_for(used_m, used_op, entry.allow.as_deref(), &http_hosts),
                        });
                    }
                }
            }
            [m_, op] => {
                if used_ops.contains(&(m_.clone(), op.clone())) {
                    narrowed.push(NarrowedEntry {
                        module: m_.clone(),
                        op: op.clone(),
                        allow: narrow_allow_for(m_, op, entry.allow.as_deref(), &http_hosts),
                    });
                }
            }
            _ => {
                // Defensive: malformed paths are dropped from the
                // narrowed form, prompting a visible diff.
            }
        }
    }
    narrowed.sort_by(|a, b| {
        a.module.cmp(&b.module).then_with(|| a.op.cmp(&b.op))
    });
    narrowed.dedup();
    let declared = render_declared(&entries);
    let narrowed_text = render_narrowed(&narrowed);
    if declared == narrowed_text {
        return None;
    }
    Some(CapNarrowing {
        fn_name: f.name.clone(),
        declared,
        narrowed: narrowed_text,
        span: f.span,
    })
}

/// Render a unified diff describing every narrowing in `m`. The output
/// is empty when the module is already minimal — `aeris fmt
/// --narrow-caps` exits 0 in that case.
pub fn render_narrowing_diff(m: &Module) -> String {
    let suggestions = narrow_caps(m);
    if suggestions.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for s in &suggestions {
        out.push_str(&format!(
            "--- fn {} (declared)\n+++ fn {} (narrowed)\n-{}\n+{}\n\n",
            s.fn_name, s.fn_name, s.declared, s.narrowed
        ));
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NarrowedEntry {
    module: String,
    op: String,
    allow: Option<Vec<String>>,
}

fn narrow_allow_for(
    module: &str,
    op: &str,
    declared_allow: Option<&[String]>,
    http_hosts: &BTreeSet<String>,
) -> Option<Vec<String>> {
    // Allow-list narrowing surface for the linter. Each family that
    // has a meaningful allow-list dimension (§ 8.3.1) contributes one
    // arm here. We deliberately keep this conservative — when we
    // can't statically extract the runtime value we leave the
    // declared allow-list intact.
    if module == "http" && (op == "get" || op == "post" || op == "put" || op == "patch" || op == "delete") {
        let used: BTreeSet<String> = http_hosts.clone();
        if used.is_empty() {
            return declared_allow.map(<[String]>::to_vec);
        }
        return Some(match declared_allow {
            Some(declared) => declared
                .iter()
                .filter(|h| used.contains(h.as_str()))
                .cloned()
                .collect(),
            None => used.into_iter().collect(),
        });
    }
    declared_allow.map(<[String]>::to_vec)
}

// ---- AST walking ----

fn walk_block(
    b: &Block,
    used: &mut BTreeSet<(String, String)>,
    http_hosts: &mut BTreeSet<String>,
) {
    for s in &b.stmts {
        walk_stmt(s, used, http_hosts);
    }
    if let Some(t) = &b.tail {
        walk_expr(t, used, http_hosts);
    }
}

fn walk_stmt(
    s: &Stmt,
    used: &mut BTreeSet<(String, String)>,
    http_hosts: &mut BTreeSet<String>,
) {
    match s {
        Stmt::Let { value, .. } | Stmt::Var { value, .. } => walk_expr(value, used, http_hosts),
        Stmt::For { iter, body, .. } => {
            walk_expr(iter, used, http_hosts);
            walk_block(body, used, http_hosts);
        }
        Stmt::While { cond, body, .. } => {
            walk_expr(cond, used, http_hosts);
            walk_block(body, used, http_hosts);
        }
        Stmt::Defer { body, .. } => walk_expr(body, used, http_hosts),
        Stmt::Expr(e) => walk_expr(e, used, http_hosts),
    }
}

fn walk_expr(
    e: &Expr,
    used: &mut BTreeSet<(String, String)>,
    http_hosts: &mut BTreeSet<String>,
) {
    if let Some((m, op)) = effects::call_of_capability(e) {
        if effects::is_cap_op(m, op) {
            used.insert((m.to_string(), op.to_string()));
            // Extract the host from a leading string-literal URL on
            // `http.<verb>(...)` calls.
            if m == "http" {
                if let Expr::Call { args, .. } = e {
                    if let Some(arg) = args.first() {
                        if let Expr::Str(url, _) = &arg.value {
                            if let Some(host) = host_from_url(url) {
                                http_hosts.insert(host);
                            }
                        }
                    }
                }
            }
        }
    }
    match e {
        Expr::Call { callee, args, .. } => {
            walk_expr(callee, used, http_hosts);
            for a in args {
                walk_call_arg(a, used, http_hosts);
            }
        }
        Expr::Try { expr, .. }
        | Expr::Await { expr, .. }
        | Expr::Raise { expr, .. }
        | Expr::Unary { expr, .. }
        | Expr::Cast { expr, .. }
        | Expr::IsCheck { expr, .. } => walk_expr(expr, used, http_hosts),
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, used, http_hosts);
            walk_expr(rhs, used, http_hosts);
        }
        Expr::Field { base, .. } => walk_expr(base, used, http_hosts),
        Expr::Index { base, index, .. } => {
            walk_expr(base, used, http_hosts);
            walk_expr(index, used, http_hosts);
        }
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            walk_expr(cond, used, http_hosts);
            walk_block(then_blk, used, http_hosts);
            match else_ {
                Some(ElseBranch::Else(b)) => walk_block(b, used, http_hosts),
                Some(ElseBranch::ElseIf(e)) => walk_expr(e, used, http_hosts),
                None => {}
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            walk_expr(scrutinee, used, http_hosts);
            for a in arms {
                if let Some(g) = &a.guard {
                    walk_expr(g, used, http_hosts);
                }
                walk_expr(&a.body, used, http_hosts);
            }
        }
        Expr::Block(b, _)
        | Expr::IntentBlock { body: b, .. }
        | Expr::Lambda { body: b, .. }
        | Expr::Spawn { body: b, .. } => walk_block(b, used, http_hosts),
        Expr::Tuple(es, _) | Expr::List(es, _) => {
            for x in es {
                walk_expr(x, used, http_hosts);
            }
        }
        Expr::Record(rl, _) => {
            for f in &rl.fields {
                walk_expr(&f.value, used, http_hosts);
            }
            if let Some(s) = &rl.spread {
                walk_expr(s, used, http_hosts);
            }
        }
        Expr::Range { start, end, .. } => {
            if let Some(s) = start {
                walk_expr(s, used, http_hosts);
            }
            if let Some(e2) = end {
                walk_expr(e2, used, http_hosts);
            }
        }
        Expr::Return { expr, .. } | Expr::Break { expr, .. } => {
            if let Some(e2) = expr {
                walk_expr(e2, used, http_hosts);
            }
        }
        Expr::Assign { value, .. } => walk_expr(value, used, http_hosts),
        _ => {}
    }
}

fn walk_call_arg(
    a: &CallArg,
    used: &mut BTreeSet<(String, String)>,
    http_hosts: &mut BTreeSet<String>,
) {
    walk_expr(&a.value, used, http_hosts);
}

// ---- helpers ----

fn host_from_url(url: &str) -> Option<String> {
    // Strip the scheme — everything before `://` if present.
    let after_scheme = match url.find("://") {
        Some(i) => &url[i + 3..],
        None => url,
    };
    if after_scheme.is_empty() {
        return None;
    }
    // Cut at the first `/` (path), `?` (query), or `#` (fragment).
    let host_port_end = after_scheme
        .find(['/', '?', '#'])
        .unwrap_or(after_scheme.len());
    let host_port = &after_scheme[..host_port_end];
    if host_port.is_empty() {
        return None;
    }
    // Drop user-info: `user@host`.
    let host_port = match host_port.rfind('@') {
        Some(i) => &host_port[i + 1..],
        None => host_port,
    };
    // Drop the port: split on the last `:` if it's followed by digits
    // (avoids cutting an IPv6 zone-id).
    let host = match host_port.rfind(':') {
        Some(i) if host_port[i + 1..].chars().all(|c| c.is_ascii_digit()) => &host_port[..i],
        _ => host_port,
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn render_declared(entries: &[crate::syntax::ast::CapEntry]) -> String {
    let mut out = String::from("cap[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&e.path.segments.join("."));
        if let Some(allow) = &e.allow {
            out.push_str(" @ ");
            out.push_str(&render_allow(allow));
        }
    }
    out.push(']');
    out
}

fn render_narrowed(entries: &[NarrowedEntry]) -> String {
    let mut out = String::from("cap[");
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&e.module);
        out.push('.');
        out.push_str(&e.op);
        if let Some(allow) = &e.allow {
            out.push_str(" @ ");
            out.push_str(&render_allow(allow));
        }
    }
    out.push(']');
    out
}

fn render_allow(allow: &[String]) -> String {
    if allow.len() == 1 {
        return format!("\"{}\"", allow[0]);
    }
    let items: Vec<String> = allow.iter().map(|s| format!("\"{s}\"")).collect();
    format!("[{}]", items.join(", "))
}

// ====================================================================
//  Tests — M12.T6 acceptance
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn one(src: &str) -> Vec<CapNarrowing> {
        let m = parse(src).unwrap_or_else(|e| panic!("parse: {e:?}"));
        narrow_caps(&m)
    }

    // ---- host extraction ----

    #[test]
    fn host_from_https_url() {
        assert_eq!(host_from_url("https://api.acme.com/x"), Some("api.acme.com".into()));
    }

    #[test]
    fn host_from_http_url_with_port() {
        assert_eq!(host_from_url("http://x.example:8080/y"), Some("x.example".into()));
    }

    #[test]
    fn host_from_url_with_query() {
        assert_eq!(host_from_url("https://api.x.com/p?a=b"), Some("api.x.com".into()));
    }

    #[test]
    fn host_from_url_with_user_info() {
        assert_eq!(host_from_url("https://u:p@host/x"), Some("host".into()));
    }

    #[test]
    fn host_from_invalid_url_is_none() {
        assert_eq!(host_from_url(""), None);
        assert_eq!(host_from_url("://"), None);
    }

    // ---- function narrowing ----

    #[test]
    fn bare_module_cap_narrows_to_used_ops() {
        // The headline acceptance: a coarse signature is narrowed to
        // the operations the body actually uses.
        let src = r#"
            fn pay(cap: cap[http]) {
                intent "pay" { http.post("https://api.acme.com/c", "\{\}") }
            }
        "#;
        let xs = one(src);
        assert_eq!(xs.len(), 1);
        assert_eq!(xs[0].fn_name, "pay");
        assert_eq!(xs[0].declared, "cap[http]");
        assert_eq!(xs[0].narrowed, "cap[http.post @ \"api.acme.com\"]");
    }

    #[test]
    fn unused_op_in_explicit_list_is_dropped() {
        let src = r#"
            fn pay(cap: cap[http.get, http.post]) {
                intent "p" { http.post("https://x/y", "\{\}") }
            }
        "#;
        let xs = one(src);
        assert_eq!(xs.len(), 1);
        assert!(xs[0].declared.contains("http.get"));
        assert!(xs[0].declared.contains("http.post"));
        assert!(!xs[0].narrowed.contains("http.get"));
        assert!(xs[0].narrowed.contains("http.post"));
    }

    #[test]
    fn allow_list_narrows_to_actually_used_hosts() {
        let src = r#"
            fn pay(cap: cap[http.post @ ["api.acme.com", "api.stripe.com"]]) {
                intent "p" { http.post("https://api.acme.com/x", "\{\}") }
            }
        "#;
        let xs = one(src);
        assert_eq!(xs.len(), 1);
        assert!(xs[0].declared.contains("api.stripe.com"));
        assert!(!xs[0].narrowed.contains("api.stripe.com"));
        assert!(xs[0].narrowed.contains("api.acme.com"));
    }

    #[test]
    fn fn_already_minimal_emits_no_suggestion() {
        let src = r#"
            fn pay(cap: cap[http.post @ "api.acme.com"]) {
                intent "p" { http.post("https://api.acme.com/x", "\{\}") }
            }
        "#;
        assert!(one(src).is_empty());
    }

    #[test]
    fn cap_star_is_not_narrowed_here() {
        // `cap[*]` is M2.T5's concern; the linter leaves it alone so a
        // single `cap[*]` violation surfaces only once.
        let src = r#"fn f(cap: cap[*]) {}"#;
        assert!(one(src).is_empty());
    }

    #[test]
    fn fn_without_cap_param_yields_nothing() {
        let src = r#"fn pure(x: int) -> int { x + 1 }"#;
        assert!(one(src).is_empty());
    }

    #[test]
    fn body_using_multiple_modules_narrows_each() {
        let src = r#"
            fn run(cap: cap[fs, http, audit.event]) {
                intent "x" {
                    fs.read_file("/x")
                    http.get("https://api.x.com/y")
                    audit.event("e", { x: 1 })
                }
            }
        "#;
        let xs = one(src);
        assert_eq!(xs.len(), 1);
        let n = &xs[0].narrowed;
        assert!(n.contains("fs.read_file"));
        assert!(n.contains("http.get"));
        assert!(n.contains("audit.event"));
        // No write-fs leakage even though the declared `cap[fs]` was
        // wide-open.
        assert!(!n.contains("fs.write_file"));
    }

    #[test]
    fn render_narrowing_diff_is_empty_when_all_minimal() {
        let src = r#"
            fn pay(cap: cap[http.post @ "x"]) {
                intent "p" { http.post("https://x/y", "\{\}") }
            }
        "#;
        let m = parse(src).unwrap();
        assert!(render_narrowing_diff(&m).is_empty());
    }

    #[test]
    fn render_narrowing_diff_contains_minus_and_plus_lines() {
        let src = r#"
            fn pay(cap: cap[http]) {
                intent "p" { http.post("https://x/y", "\{\}") }
            }
        "#;
        let m = parse(src).unwrap();
        let d = render_narrowing_diff(&m);
        assert!(d.contains("--- fn pay (declared)"));
        assert!(d.contains("+++ fn pay (narrowed)"));
        assert!(d.contains("-cap[http]"));
        assert!(d.contains("+cap[http.post @ \"x\"]"));
    }

    #[test]
    fn module_without_caps_emits_nothing() {
        let src = r#"
            record R { x: int }
            fn pure() -> int { 1 }
        "#;
        let m = parse(src).unwrap();
        assert!(narrow_caps(&m).is_empty());
        assert!(render_narrowing_diff(&m).is_empty());
    }

    #[test]
    fn allow_list_narrowing_only_keeps_subset_of_declared() {
        // The body uses a host that isn't in the declared allow-list:
        // the narrowing still produces the *intersection* (= empty)
        // rather than expanding the declared list. The linter never
        // broadens authority.
        let src = r#"
            fn pay(cap: cap[http.post @ "api.acme.com"]) {
                intent "p" { http.post("https://api.stripe.com/x", "\{\}") }
            }
        "#;
        let xs = one(src);
        assert_eq!(xs.len(), 1);
        assert!(!xs[0].narrowed.contains("api.stripe.com"));
        assert!(!xs[0].narrowed.contains("api.acme.com"));
    }
}
