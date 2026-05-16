//! M2.T6 — allow-list intersection with `lockset.toml [caps]`.
//!
//! Realises `docs/language.md` § 8.3.2: every `cap[...]` entry that
//! carries an `@ <allow-list>` clause must be a strict subset of the
//! corresponding family ceiling declared in `lockset.toml [caps]`.
//! Out-of-ceiling entries are rejected with exit code 71 (§ 25.3).
//!
//! The check fires on every cap-typed parameter of every `fn` and
//! `saga` declaration. `cap[*]` is handled by M2.T5 (exit 65); a cap
//! entry with no `@` clause inherits the ceiling and is silently
//! accepted at this layer (§ 8.3.2 last bullet).

use super::error::{CheckError, CheckErrorKind};
use crate::lockset::CapsCeiling;
use crate::syntax::ast::{CapEntry, Item, Module, Param, Type};

pub fn check_module_against_lockset(m: &Module, caps: &CapsCeiling) -> Vec<CheckError> {
    let mut out: Vec<CheckError> = Vec::new();
    for item in &m.items {
        match item {
            Item::Fn(f) => check_params(&f.params, caps, &mut out),
            Item::Saga(s) => check_params(&s.params, caps, &mut out),
            _ => {}
        }
    }
    out
}

fn check_params(params: &[Param], caps: &CapsCeiling, out: &mut Vec<CheckError>) {
    for p in params {
        if let Type::Cap { entries, star, .. } = &p.ty {
            if *star {
                continue;
            }
            for entry in entries {
                check_entry(entry, caps, out);
            }
        }
    }
}

fn check_entry(entry: &CapEntry, caps: &CapsCeiling, out: &mut Vec<CheckError>) {
    let allow = match &entry.allow {
        Some(xs) => xs,
        None => return,
    };
    let segs = entry.path.segments.as_slice();
    let (op, ceiling, family) = match segs {
        [m_, op] => match family_for(m_, op, caps) {
            Some(f) => (format!("{m_}.{op}"), f.0, f.1),
            None => return,
        },
        _ => return,
    };
    for item in allow {
        if !ceiling.iter().any(|c| c == item) {
            out.push(CheckError::new(
                CheckErrorKind::AllowListOutsideLockset {
                    op: op.clone(),
                    entry: item.clone(),
                    family: family.into(),
                },
                entry.span,
            ));
        }
    }
}

/// Map a `(module, op)` pair to the lockset ceiling it must intersect
/// against. Modules without a ceiling dimension in `[caps]` (e.g.
/// `audit`, `clock`, `random`, `env`, `io`, `shell`, `mongodb`,
/// `minio`, `rabbitmq`, `docker`) return `None` here — those families
/// either carry no `@` allow-list at all (audit / clock / random / env
/// / io) or their ceiling extension is deferred to a later milestone.
fn family_for<'a>(
    module: &str,
    op: &str,
    caps: &'a CapsCeiling,
) -> Option<(&'a [String], &'static str)> {
    match (module, op) {
        ("http", _) => Some((&caps.http_allow, "http.allow")),
        ("kube", _) => Some((&caps.kube_contexts, "kube.contexts")),
        ("ai", _) => Some((&caps.ai_models, "ai.models")),
        ("fs", op) if is_fs_read(op) => Some((&caps.fs_allow_read, "fs.allow_read")),
        ("fs", op) if is_fs_write(op) => Some((&caps.fs_allow_write, "fs.allow_write")),
        _ => None,
    }
}

fn is_fs_read(op: &str) -> bool {
    matches!(
        op,
        "read_file" | "read_text" | "read_bytes" | "stat" | "exists" | "walk"
    )
}

fn is_fs_write(op: &str) -> bool {
    matches!(
        op,
        "write_file" | "write_text" | "write_bytes" | "mkdir" | "remove" | "rename"
    )
}

// ====================================================================
//  Tests — M2.T6 acceptance fixtures (§ 8.3.2)
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lockset::parse_lockset;
    use crate::syntax::parse;

    fn ceiling(toml_src: &str) -> CapsCeiling {
        parse_lockset(toml_src)
            .expect("lockset parses")
            .caps
            .clone()
    }

    fn errs(toml_src: &str, aeris_src: &str) -> Vec<CheckError> {
        let m = parse(aeris_src).unwrap_or_else(|e| panic!("parse on {aeris_src:?}: {e:?}"));
        check_module_against_lockset(&m, &ceiling(toml_src))
    }

    fn project_with_http(allow: &[&str]) -> String {
        let xs: Vec<String> = allow.iter().map(|s| format!("\"{s}\"")).collect();
        format!(
            r#"
                [project]
                name  = "x"
                aeris = "0.2.0"

                [caps]
                http.allow = [{joined}]
            "#,
            joined = xs.join(", ")
        )
    }

    // ---- positive ----

    #[test]
    fn http_post_inside_ceiling_is_accepted() {
        let toml = project_with_http(&["api.acme.com", "api.stripe.com"]);
        let aer = r#"
            fn pay(cap: cap[http.post @ ["api.acme.com"]]) {}
        "#;
        assert!(errs(&toml, aer).is_empty());
    }

    #[test]
    fn http_post_with_two_subset_entries_is_accepted() {
        let toml = project_with_http(&["api.acme.com", "api.stripe.com"]);
        let aer = r#"
            fn pay(cap: cap[http.post @ ["api.acme.com", "api.stripe.com"]]) {}
        "#;
        assert!(errs(&toml, aer).is_empty());
    }

    #[test]
    fn http_post_without_allow_clause_inherits_ceiling() {
        // § 8.3.2 last bullet: a function asking for `http.post` (no
        // `@`) is accepted; M2.T6 has nothing to check.
        let toml = project_with_http(&["api.acme.com"]);
        let aer = r#"
            fn pay(cap: cap[http.post]) {}
        "#;
        assert!(errs(&toml, aer).is_empty());
    }

    #[test]
    fn fs_read_inside_ceiling_is_accepted() {
        let toml = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            fs.allow_read  = ["/etc/aeris/**", "./data/**"]
            fs.allow_write = ["./out/**"]
        "#;
        let aer = r#"
            fn rd(cap: cap[fs.read_file @ ["./data/**"]]) {}
        "#;
        assert!(errs(toml, aer).is_empty());
    }

    #[test]
    fn ai_complete_inside_models_ceiling_is_accepted() {
        let toml = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            ai.models = ["claude-opus-4-7", "claude-haiku-4-5"]
        "#;
        let aer = r#"
            fn ask(cap: cap[ai.complete @ ["claude-opus-4-7"]]) {}
        "#;
        assert!(errs(toml, aer).is_empty());
    }

    // ---- negative — exit code 71 ----

    #[test]
    fn http_post_outside_ceiling_rejected_with_71() {
        // The headline acceptance from `docs/plan.md § 5.2 M2.T6`.
        let toml = project_with_http(&["api.acme.com"]);
        let aer = r#"
            fn evil(cap: cap[http.post @ ["evil.com"]]) {}
        "#;
        let es = errs(&toml, aer);
        assert_eq!(es.len(), 1, "expected exactly one error, got {es:#?}");
        assert_eq!(es[0].exit_code(), 71);
        match &es[0].kind {
            CheckErrorKind::AllowListOutsideLockset {
                op,
                entry,
                family,
            } => {
                assert_eq!(op, "http.post");
                assert_eq!(entry, "evil.com");
                assert_eq!(family, "http.allow");
            }
            other => panic!("wrong kind: {other:?}"),
        }
    }

    #[test]
    fn http_post_partially_outside_ceiling_rejected_per_offending_entry() {
        let toml = project_with_http(&["api.acme.com"]);
        let aer = r#"
            fn pay(cap: cap[http.post @ ["api.acme.com", "evil.com", "also-evil.com"]]) {}
        "#;
        let es = errs(&toml, aer);
        // Two violations — `evil.com` and `also-evil.com`.
        assert_eq!(es.len(), 2);
        let mut entries: Vec<&str> = es
            .iter()
            .filter_map(|e| match &e.kind {
                CheckErrorKind::AllowListOutsideLockset { entry, .. } => Some(entry.as_str()),
                _ => None,
            })
            .collect();
        entries.sort();
        assert_eq!(entries, vec!["also-evil.com", "evil.com"]);
        for e in &es {
            assert_eq!(e.exit_code(), 71);
        }
    }

    #[test]
    fn fs_write_outside_ceiling_rejected() {
        let toml = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            fs.allow_write = ["./out/**"]
        "#;
        let aer = r#"
            fn w(cap: cap[fs.write_file @ ["/etc/passwd"]]) {}
        "#;
        let es = errs(toml, aer);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].exit_code(), 71);
        if let CheckErrorKind::AllowListOutsideLockset { family, .. } = &es[0].kind {
            assert_eq!(family, "fs.allow_write");
        }
    }

    #[test]
    fn kube_context_outside_ceiling_rejected() {
        let toml = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            kube.contexts = ["prod-eu-1"]
        "#;
        let aer = r#"
            fn k(cap: cap[kube.apply @ ["staging-us-1"]]) {}
        "#;
        let es = errs(toml, aer);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].exit_code(), 71);
        if let CheckErrorKind::AllowListOutsideLockset { entry, family, .. } = &es[0].kind {
            assert_eq!(entry, "staging-us-1");
            assert_eq!(family, "kube.contexts");
        }
    }

    #[test]
    fn ai_model_outside_ceiling_rejected() {
        let toml = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            ai.models = ["claude-opus-4-7"]
        "#;
        let aer = r#"
            fn ask(cap: cap[ai.complete @ ["gpt-4"]]) {}
        "#;
        let es = errs(toml, aer);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].exit_code(), 71);
    }

    #[test]
    fn empty_ceiling_means_any_listed_entry_is_outside() {
        // No `[caps] http.allow` → empty ceiling → any explicit
        // `http.post @ ["..."]` is rejected.
        let toml = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"
        "#;
        let aer = r#"
            fn evil(cap: cap[http.post @ ["api.acme.com"]]) {}
        "#;
        let es = errs(toml, aer);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].exit_code(), 71);
    }

    #[test]
    fn saga_param_signature_is_also_checked() {
        let toml = project_with_http(&["api.acme.com"]);
        let aer = r#"
            saga charge(cap: cap[http.post @ ["evil.com"]]) {
                intent "x"
                step pay {
                    do   { http.post("https://evil.com/c", "{}")? }
                    undo { http.post("https://evil.com/r", "{}")? }
                }
            }
        "#;
        let es = errs(&toml, aer);
        assert!(es
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::AllowListOutsideLockset { .. })));
        assert!(es.iter().all(|e| e.exit_code() == 71));
    }

    #[test]
    fn cap_star_is_skipped_here_handled_by_m2_t5() {
        // `cap[*]` is the M2.T5 concern. M2.T6 does not also fire on it.
        let toml = project_with_http(&["api.acme.com"]);
        let aer = "fn f(cap: cap[*]) {}";
        assert!(errs(&toml, aer).is_empty());
    }

    // ---- M15 — prototype-mode (caps.required = false) ----

    fn check_with(toml_src: &str, aeris_src: &str) -> Vec<crate::check::CheckError> {
        let m = crate::syntax::parse(aeris_src)
            .unwrap_or_else(|e| panic!("parse: {e:?}"));
        let lockset = crate::lockset::parse_lockset(toml_src).expect("lockset");
        crate::check::check_module_with_lockset(&m, &lockset.caps)
    }

    fn fixture_strict() -> &'static str {
        r#"
            [project]
            name = "x"
            aeris = "0.2.0"
            [caps]
            required = true
        "#
    }

    fn fixture_prototype() -> &'static str {
        r#"
            [project]
            name = "x"
            aeris = "0.2.0"
            [caps]
            required = false
        "#
    }

    #[test]
    fn prototype_mode_suppresses_no_cap_in_scope() {
        // Function without `cap` parameter calls `io.println`. Strict
        // mode rejects with E65; prototype mode accepts.
        let aer = r#"fn say() { io.println("hi") }"#;
        let strict = check_with(fixture_strict(), aer);
        assert!(strict
            .iter()
            .any(|e| matches!(e.kind, crate::check::CheckErrorKind::NoCapInScope { .. })));
        let proto = check_with(fixture_prototype(), aer);
        assert!(proto
            .iter()
            .all(|e| !matches!(e.kind, crate::check::CheckErrorKind::NoCapInScope { .. })));
    }

    #[test]
    fn prototype_mode_does_not_relax_op_not_in_cap() {
        // Function declares `cap[fs.read_file]` but writes — that's
        // an explicit opt-in to discipline. Even prototype mode keeps
        // OpNotInCapSignature active.
        let aer = r#"
            fn f(cap: cap[fs.read_file]) {
                intent "x" { fs.write_file("/x", "y") }
            }
        "#;
        let proto = check_with(fixture_prototype(), aer);
        assert!(proto
            .iter()
            .any(|e| matches!(e.kind, crate::check::CheckErrorKind::OpNotInCapSignature { .. })));
    }

    #[test]
    fn prototype_mode_keeps_intent_rule_active() {
        // E66 is about program structure, not authority — must fire
        // in both modes.
        let aer = r#"
            fn pay() {
                http.post("https://x/y", "{}")
            }
        "#;
        let proto = check_with(fixture_prototype(), aer);
        assert!(proto
            .iter()
            .any(|e| matches!(e.kind, crate::check::CheckErrorKind::MissingIntentForWriteCall { .. })));
    }

    #[test]
    fn prototype_mode_keeps_saga_undo_rule_active() {
        let aer = r#"
            saga s(cap: cap[http.post]) {
                intent "x"
                step a { do { http.post("u", "{}")? } undo noop }
            }
        "#;
        let proto = check_with(fixture_prototype(), aer);
        assert!(proto
            .iter()
            .any(|e| matches!(e.kind, crate::check::CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. })));
    }

    #[test]
    fn prototype_mode_keeps_cap_star_ban_active() {
        let aer = "fn f(cap: cap[*]) {}";
        let proto = check_with(fixture_prototype(), aer);
        assert!(proto
            .iter()
            .any(|e| matches!(e.kind, crate::check::CheckErrorKind::CapStarInUserCode)));
    }

    #[test]
    fn prototype_mode_keeps_allow_list_intersection_active() {
        // E71 still fires: prototype mode is about the *signature*
        // requirement, not about the lockset ceiling enforcement.
        let toml = r#"
            [project]
            name = "x"
            aeris = "0.2.0"
            [caps]
            required = false
            http.allow = ["api.acme.com"]
        "#;
        let aer = r#"fn pay(cap: cap[http.post @ "evil.com"]) {}"#;
        let proto = check_with(toml, aer);
        assert!(proto
            .iter()
            .any(|e| matches!(e.kind, crate::check::CheckErrorKind::AllowListOutsideLockset { .. })));
    }

    #[test]
    fn lockset_required_default_is_true() {
        // Absence of `required` defaults to strict mode, preserving
        // M0–M14 behaviour for legacy locksets.
        let toml = r#"
            [project]
            name = "x"
            aeris = "0.2.0"
            [caps]
            http.allow = ["x"]
        "#;
        let lockset = crate::lockset::parse_lockset(toml).unwrap();
        assert!(lockset.caps.required);
    }

    #[test]
    fn lockset_required_explicit_false_parses() {
        let toml = r#"
            [project]
            name = "x"
            aeris = "0.2.0"
            [caps]
            required = false
        "#;
        let lockset = crate::lockset::parse_lockset(toml).unwrap();
        assert!(!lockset.caps.required);
    }

    #[test]
    fn lockset_required_non_bool_is_rejected() {
        let toml = r#"
            [project]
            name = "x"
            aeris = "0.2.0"
            [caps]
            required = "yes"
        "#;
        let err = crate::lockset::parse_lockset(toml).unwrap_err();
        assert!(err.message.contains("caps.required"));
    }

    #[test]
    fn unrelated_modules_are_not_intersected() {
        // `audit.event`, `clock.now`, `env.read`, `io.println` etc.
        // carry no `@` allow-list dimension; M2.T6 must not fire.
        // `mongodb.*`, `minio.*`, `rabbitmq.*`, `shell.exec` lack a
        // ceiling field in `CapsCeiling` today — for now M2.T6 leaves
        // those unconstrained. The wider ceiling is deferred to a
        // future milestone.
        let toml = project_with_http(&[]);
        let aer = r#"
            fn f(cap: cap[
                audit.event,
                io.println,
                shell.exec @ ["kubectl"],
                mongodb.write @ ["db.col"],
                minio.put @ ["bucket"]
            ]) {}
        "#;
        assert!(errs(&toml, aer).is_empty());
    }
}
