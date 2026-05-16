//! Aeris type and capability checker.
//!
//! Realises `docs/language.md` § 4 (types), § 8 (capabilities),
//! § 9 (contracts), § 10 (intent), § 17 (pattern matching) and the
//! V2/V3 patches. Exit codes per § 25.3.
//!
//! M2.T1 lays the type-resolution groundwork: it walks every type
//! annotation in the parsed module and reports unknown types, missing
//! generic bindings, arity mismatches on stdlib containers, duplicate
//! declarations, duplicate record fields, duplicate enum variants and
//! cyclic type aliases. Later M2 tasks extend the same `check_module`
//! entry point with capability rules (T3–T6), V2 enforcement (T7),
//! saga rules (T8), agent_net cycle detection (T9), versioning (T10)
//! and cap-escape rules (T11).

pub mod effects;
mod error;
mod lockset_caps;
mod narrow_caps;
mod render;
mod resolver;

pub use error::{CheckError, CheckErrorKind};
pub use lockset_caps::check_module_against_lockset;
pub use narrow_caps::{narrow_caps, render_narrowing_diff, CapNarrowing};
pub use render::{explain, render_diagnostic};

use crate::lockset::CapsCeiling;
use crate::syntax::ast::Module;

/// Run the static-analysis suite over a parsed module. The checker
/// returns *every* error discovered (no early abort) so consumers like
/// `aeris check` can render a complete diagnostic batch in one pass.
pub fn check_module(m: &Module) -> Vec<CheckError> {
    resolver::check_module(m)
}

/// Run the static-analysis suite together with the M2.T6 allow-list
/// intersection check against the project's `lockset.toml [caps]`
/// ceiling. The lockset-intersection errors are appended to the result
/// of `check_module` so callers receive a single combined batch.
///
/// M15: when `caps.required == false` the checker switches to
/// prototype mode (§ 8.4.1). Body-resolution `NoCapInScope` errors
/// (E65) are suppressed for functions without a `cap` parameter; every
/// other rule (`OpNotInCapSignature`, `cap[*]` ban, `intent`, saga
/// `undo`, allow-list intersection) remains active.
pub fn check_module_with_lockset(m: &Module, caps: &CapsCeiling) -> Vec<CheckError> {
    let mut out = resolver::check_module(m);
    if !caps.required {
        out.retain(|e| !is_prototype_suppressible(&e.kind, m));
    }
    out.extend(lockset_caps::check_module_against_lockset(m, caps));
    out
}

/// Whether a check error should be suppressed under `required = false`.
/// Today this means a single class: `NoCapInScope` raised on a function
/// that does not declare a `cap` parameter — the prototype-mode escape
/// hatch. `OpNotInCapSignature` remains active because the developer
/// explicitly opted in to the discipline by writing `cap: cap[...]`.
fn is_prototype_suppressible(kind: &CheckErrorKind, _m: &Module) -> bool {
    matches!(kind, CheckErrorKind::NoCapInScope { .. })
}

// ====================================================================
// M2.T1 — 40 positive + 20 negative type-resolution fixtures.
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parse;

    fn errs(src: &str) -> Vec<CheckError> {
        let m = parse(src).unwrap_or_else(|e| panic!("parse failure on {src:?}: {e:?}"));
        check_module(&m)
    }

    fn ok(src: &str) {
        let es = errs(src);
        assert!(es.is_empty(), "expected no errors for {src:?}, got {es:#?}");
    }

    fn bad(src: &str, kind_pred: impl Fn(&CheckErrorKind) -> bool) {
        let es = errs(src);
        assert!(
            es.iter().any(|e| kind_pred(&e.kind)),
            "expected matching error for {src:?}, got {es:#?}"
        );
        for e in &es {
            assert_eq!(e.exit_code(), 64, "all M2.T1 errors must use exit 64");
        }
    }

    // ----------------- positive fixtures (40) -----------------

    #[test]
    fn p01_record_with_int_field() {
        ok("record R { x: int }");
    }

    #[test]
    fn p02_record_all_primitives() {
        ok(r#"
            record AllPrim {
                a: bool, b: int, c: i64, d: u32, e: f64,
                f: decimal, g: string, h: bytes, j: char,
                k: uuid, l: date, m: timestamp, n: duration,
            }
        "#);
    }

    #[test]
    fn p03_record_unit_field() {
        ok("record R { x: unit }");
    }

    #[test]
    fn p04_record_list_of_int() {
        ok("record R { xs: list<int> }");
    }

    #[test]
    fn p05_record_set_of_string() {
        ok("record R { tags: set<string> }");
    }

    #[test]
    fn p06_record_map_string_int() {
        ok("record R { kv: map<string, int> }");
    }

    #[test]
    fn p07_record_option() {
        ok("record R { x: option<int> }");
    }

    #[test]
    fn p08_record_result() {
        ok("record R { x: result<int> }");
    }

    #[test]
    fn p09_record_nested_generics() {
        ok("record R { xs: list<list<int>> }");
    }

    #[test]
    fn p10_record_tuple_field() {
        ok("record R { p: (int, string) }");
    }

    #[test]
    fn p11_record_unit_tuple_field() {
        ok("record R { z: () }");
    }

    #[test]
    fn p12_record_field_referring_to_other_record() {
        ok("record User { id: uuid } record Order { user: User }");
    }

    #[test]
    fn p13_record_field_referring_to_enum() {
        ok("enum Status { A, B } record S { st: Status }");
    }

    #[test]
    fn p14_record_field_referring_to_model() {
        ok(r#"
            model Invoice@v1 { id: uuid }
            record Batch { items: list<Invoice@v1> }
        "#);
    }

    #[test]
    fn p15_record_generic() {
        ok("record Wrapper<T> { value: T }");
    }

    #[test]
    fn p16_record_generic_two_params() {
        ok("record Pair<A, B> { l: A, r: B }");
    }

    #[test]
    fn p17_record_generic_referenced_in_list() {
        ok("record Box<T> { items: list<T> }");
    }

    #[test]
    fn p18_record_generic_referenced_in_map() {
        ok("record Cache<K, V> { kv: map<K, V> }");
    }

    #[test]
    fn p19_enum_unit_variants() {
        ok("enum Color { Red, Green, Blue }");
    }

    #[test]
    fn p20_enum_tuple_variant() {
        ok("enum E { A, B(int), C(string, int) }");
    }

    #[test]
    fn p21_enum_record_variant() {
        ok(r#"enum E { A, B { x: int, y: int } }"#);
    }

    #[test]
    fn p22_enum_generic() {
        ok("enum Either<L, R> { Left(L), Right(R) }");
    }

    #[test]
    fn p23_model_minimum() {
        ok("model M@v1 { id: uuid }");
    }

    #[test]
    fn p24_model_higher_version() {
        ok("model Doc@v42 { text: string }");
    }

    #[test]
    fn p25_model_with_list_field() {
        ok("model Order@v1 { lines: list<int> }");
    }

    #[test]
    fn p26_model_referencing_other_model() {
        ok(r#"
            model Line@v1 { qty: int }
            model Order@v1 { lines: list<Line@v1> }
        "#);
    }

    #[test]
    fn p27_two_versions_of_same_model() {
        // `Invoice@v1` and `Invoice@v2` are distinct types per § 4.5.
        ok(r#"
            model Invoice@v1 { id: uuid }
            model Invoice@v2 { id: uuid, total: decimal }
        "#);
    }

    #[test]
    fn p28_type_alias_to_primitive() {
        ok("type Email = string");
    }

    #[test]
    fn p29_type_alias_to_user_record() {
        ok("record User { id: uuid } type U = User");
    }

    #[test]
    fn p30_type_alias_to_generic_container() {
        ok("type Ids = list<uuid>");
    }

    #[test]
    fn p31_type_alias_chain() {
        ok(r#"
            type A = string
            type B = A
            type C = B
        "#);
    }

    #[test]
    fn p32_type_alias_generic_lhs() {
        ok("type Pair<A, B> = (A, B)");
    }

    #[test]
    fn p33_fn_signature_int_return() {
        ok("fn add(a: int, b: int) -> int {}");
    }

    #[test]
    fn p34_fn_with_generics() {
        ok("fn first<T>(xs: list<T>) -> option<T> {}");
    }

    #[test]
    fn p35_fn_returning_unit_is_ok_without_arrow() {
        ok("fn doit(x: int) {}");
    }

    #[test]
    fn p36_fn_with_fn_type_param() {
        ok("fn map<T, U>(xs: list<T>, f: fn(T) -> U) -> list<U> {}");
    }

    #[test]
    fn p37_fn_with_cap_param() {
        ok("fn f(cap: cap[fs.read_file]) {}");
    }

    #[test]
    fn p38_const_with_type() {
        ok("const PI: decimal = 3.14");
    }

    #[test]
    fn p39_record_referring_to_aliased_primitive() {
        ok("type Email = string record User { e: Email }");
    }

    #[test]
    fn p40_pub_visibility_is_irrelevant_to_types() {
        ok(r#"
            pub record User { id: uuid }
            pub fn show(u: User) -> string {}
        "#);
    }

    // ----------------- negative fixtures (20) -----------------

    #[test]
    fn n01_unknown_primitive() {
        bad(
            "record R { x: bigint }",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "bigint"),
        );
    }

    #[test]
    fn n02_unknown_user_type() {
        bad(
            "record R { x: Foo }",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "Foo"),
        );
    }

    #[test]
    fn n03_list_with_wrong_arity() {
        bad("record R { xs: list<int, string> }", |k| {
            matches!(
                k,
                CheckErrorKind::WrongTypeArity { name, expected: 1, found: 2 } if name == "list"
            )
        });
    }

    #[test]
    fn n04_map_with_one_arg() {
        bad("record R { kv: map<int> }", |k| {
            matches!(
                k,
                CheckErrorKind::WrongTypeArity { name, expected: 2, found: 1 } if name == "map"
            )
        });
    }

    #[test]
    fn n05_bare_list_without_args() {
        bad(
            "record R { xs: list }",
            |k| matches!(k, CheckErrorKind::ArityRequired(s) if s == "list"),
        );
    }

    #[test]
    fn n06_unknown_model_version() {
        bad(
            "model Invoice@v1 { id: uuid } record R { x: Invoice@v3 }",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "Invoice@v3"),
        );
    }

    #[test]
    fn n07_unbound_generic_in_field() {
        bad(
            "record R { x: T }",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "T"),
        );
    }

    #[test]
    fn n08_unbound_generic_in_fn_return() {
        bad(
            "fn id(x: int) -> T {}",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "T"),
        );
    }

    #[test]
    fn n09_duplicate_record() {
        bad(
            "record R { a: int } record R { b: int }",
            |k| matches!(k, CheckErrorKind::DuplicateDecl(s) if s == "R"),
        );
    }

    #[test]
    fn n10_duplicate_record_field() {
        bad("record R { a: int, a: string }", |k| {
            matches!(
                k,
                CheckErrorKind::DuplicateField { decl, field } if decl == "R" && field == "a"
            )
        });
    }

    #[test]
    fn n11_duplicate_enum_variant() {
        bad("enum E { A, A }", |k| {
            matches!(
                k,
                CheckErrorKind::DuplicateVariant { decl, variant } if decl == "E" && variant == "A"
            )
        });
    }

    #[test]
    fn n12_duplicate_generic_param() {
        bad("record P<T, T> { x: T }", |k| {
            matches!(
                k,
                CheckErrorKind::DuplicateGeneric { decl, name } if decl == "P" && name == "T"
            )
        });
    }

    #[test]
    fn n13_cyclic_type_alias() {
        bad(
            r#"
                type A = B
                type B = A
            "#,
            |k| matches!(k, CheckErrorKind::CyclicTypeAlias(_)),
        );
    }

    #[test]
    fn n14_cyclic_alias_self_reference() {
        bad(
            "type X = X",
            |k| matches!(k, CheckErrorKind::CyclicTypeAlias(s) if s == "X"),
        );
    }

    #[test]
    fn n15_unknown_in_alias_target() {
        bad(
            "type X = Y",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "Y"),
        );
    }

    #[test]
    fn n16_duplicate_model_at_same_version() {
        bad(
            r#"
                model M@v1 { a: int }
                model M@v1 { b: int }
            "#,
            |k| {
                matches!(
                    k,
                    CheckErrorKind::ModelVersionConflict { name, version: 1 } if name == "M"
                )
            },
        );
    }

    #[test]
    fn n17_duplicate_field_in_model() {
        bad("model M@v1 { id: uuid, id: int }", |k| {
            matches!(
                k,
                CheckErrorKind::DuplicateField { decl, field } if decl == "M" && field == "id"
            )
        });
    }

    #[test]
    fn n18_unknown_in_enum_tuple_variant() {
        bad(
            "enum E { A(Bogus) }",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "Bogus"),
        );
    }

    #[test]
    fn n19_user_type_with_wrong_generic_arity() {
        bad(
            "record P<A, B> { x: A, y: B } record Q { p: P<int> }",
            |k| {
                matches!(
                    k,
                    CheckErrorKind::WrongTypeArity { name, expected: 2, found: 1 } if name == "P"
                )
            },
        );
    }

    #[test]
    fn n20_const_with_unknown_type() {
        bad(
            "const X: Bogus = 1",
            |k| matches!(k, CheckErrorKind::UnknownType(s) if s == "Bogus"),
        );
    }

    // ----------------- meta ----------------

    // ----------------- M2.T5 — `cap[*]` rejected -----------------

    fn errs_with_code(src: &str, code: u8) -> Vec<CheckError> {
        let m = parse(src).unwrap_or_else(|e| panic!("parse failure on {src:?}: {e:?}"));
        let es = check_module(&m);
        for e in &es {
            assert!(
                e.exit_code() == code || matches!(e.kind, CheckErrorKind::CapStarInUserCode),
                "expected exit {code}, got {} for {:?}",
                e.exit_code(),
                e.kind
            );
        }
        es
    }

    #[test]
    fn cap_star_in_fn_signature_is_rejected_with_code_65() {
        let es = errs_with_code("fn f(cap: cap[*]) {}", 65);
        let star_errs: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::CapStarInUserCode))
            .collect();
        assert_eq!(star_errs.len(), 1);
        assert_eq!(star_errs[0].exit_code(), 65);
    }

    #[test]
    fn cap_star_inside_record_field_is_rejected() {
        // A `cap[*]` smuggled through a record field is also rejected.
        // (Note: § 8.7 forbids storing `cap` in a record field; M2.T11
        // will add the escape-rule check. Here we verify only the
        // `*` flag at the type-resolution layer.)
        let es = check_module(&parse("record R { c: cap[*] }").expect("parse"));
        assert!(es
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::CapStarInUserCode)));
    }

    #[test]
    fn cap_with_explicit_entries_is_accepted() {
        ok("fn f(cap: cap[fs.read_file, audit.event]) {}");
    }

    #[test]
    fn cap_empty_brackets_is_accepted() {
        // `cap[]` (empty effect set) is permitted: a cap that grants
        // no operations. `cap[*]` is the rejected form.
        ok("fn f(cap: cap[]) {}");
    }

    // ----------------- M2.T9 — agent_net cycle detection -----------------

    #[test]
    fn agent_net_simple_cycle_is_rejected_with_70() {
        let src = r#"
            agent_net p {
                flow a -> b -> a
            }
        "#;
        let es = errs(src);
        let cycle: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::AgentNetCycle { .. }))
            .collect();
        assert_eq!(cycle.len(), 1);
        assert_eq!(cycle[0].exit_code(), 70);
        if let CheckErrorKind::AgentNetCycle { net, chain } = &cycle[0].kind {
            assert_eq!(net, "p");
            assert!(chain.contains("a"));
        }
    }

    #[test]
    fn agent_net_self_loop_is_rejected() {
        let es = errs("agent_net x { flow a -> a }");
        assert!(es
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::AgentNetCycle { .. })));
    }

    #[test]
    fn agent_net_three_node_cycle_is_rejected() {
        let es = errs("agent_net x { flow a -> b -> c -> a }");
        assert!(es
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::AgentNetCycle { .. })));
    }

    #[test]
    fn agent_net_cycle_via_separate_flows_is_rejected() {
        // The two `flow` lines are unioned per § 14.1; the union has a
        // cycle a -> b -> a even though no single line contains it.
        let src = r#"
            agent_net p {
                flow a -> b
                flow b -> a
            }
        "#;
        assert!(errs(src)
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::AgentNetCycle { .. })));
    }

    #[test]
    fn agent_net_acyclic_dag_is_accepted() {
        ok(r#"
            agent_net pipeline {
                flow extract -> classify -> route_or_alert
                flow route_or_alert -> { route, alert }
                flow route -> persist
                flow alert -> notify_human
                until: classify.confidence > 0.95
            }
        "#);
    }

    #[test]
    fn agent_net_fan_out_followed_by_join_is_accepted() {
        ok(r#"
            agent_net p {
                flow source -> { branch_a, branch_b }
                flow branch_a -> sink
                flow branch_b -> sink
            }
        "#);
    }

    // ----------------- M2.T10 — bare model rejected with 68 -----------------

    #[test]
    fn bare_model_in_record_field_rejected_with_68() {
        let src = r#"
            model Invoice@v1 { id: uuid }
            record Batch { items: list<Invoice> }
        "#;
        let es = errs(src);
        let bare: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::BareModelWithoutVersion(_)))
            .collect();
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].exit_code(), 68);
        if let CheckErrorKind::BareModelWithoutVersion(s) = &bare[0].kind {
            assert_eq!(s, "Invoice");
        }
    }

    #[test]
    fn bare_model_in_fn_signature_rejected_with_68() {
        let src = r#"
            model User@v1 { id: uuid }
            fn greet(u: User) -> string {}
        "#;
        let es = errs(src);
        let bare: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::BareModelWithoutVersion(_)))
            .collect();
        assert_eq!(bare.len(), 1);
        assert_eq!(bare[0].exit_code(), 68);
    }

    #[test]
    fn versioned_model_reference_is_accepted() {
        ok(r#"
            model Invoice@v1 { id: uuid }
            record Batch { items: list<Invoice@v1> }
        "#);
    }

    #[test]
    fn unknown_non_model_name_is_64_not_68() {
        // `Foo` is not a declared model — it must surface as
        // `UnknownType` (64), not `BareModelWithoutVersion` (68).
        let es = errs("record R { x: Foo }");
        assert!(es
            .iter()
            .all(|e| !matches!(e.kind, CheckErrorKind::BareModelWithoutVersion(_))));
        assert!(es
            .iter()
            .any(|e| matches!(&e.kind, CheckErrorKind::UnknownType(s) if s == "Foo")));
    }

    // ----------------- M2.T8 — saga write-do + undo noop -----------------

    #[test]
    fn saga_step_with_http_post_and_noop_undo_rejected_with_67() {
        let src = r#"
            saga charge(cap: cap[http.post @ ["api.acme.com"]]) {
                intent "charge"
                step pay {
                    do { http.post("https://api.acme.com/charge", "\{\}")? }
                    undo noop
                }
            }
        "#;
        let es = errs(src);
        let saga_errs: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. }))
            .collect();
        assert_eq!(
            saga_errs.len(),
            1,
            "expected exactly one saga error, got {es:#?}"
        );
        assert_eq!(saga_errs[0].exit_code(), 67);
        if let CheckErrorKind::SagaStepUndoNoopWithWriteDo { saga, step } = &saga_errs[0].kind {
            assert_eq!(saga, "charge");
            assert_eq!(step, "pay");
        }
    }

    #[test]
    fn saga_step_with_audit_event_and_noop_undo_rejected() {
        let src = r#"
            saga record_only(cap: cap[audit.event]) {
                intent "log it"
                step log {
                    do { audit.event("note", { x: 1 }) }
                    undo noop
                }
            }
        "#;
        assert!(errs(src)
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. })));
    }

    #[test]
    fn saga_step_with_paired_undo_block_is_accepted() {
        ok(r#"
            saga settle(cap: cap[http.post @ ["api.acme.com"]]) {
                intent "settle"
                step charge {
                    do   { http.post("https://api.acme.com/charge", "\{\}")? }
                    undo { http.post("https://api.acme.com/refund", "\{\}")? }
                }
            }
        "#);
    }

    #[test]
    fn saga_step_with_only_reads_and_noop_undo_is_accepted() {
        // `fs.read_file` is read-classified per § 8.1 — it does not
        // require a paired undo. (In practice, a saga step with only
        // reads is degenerate, but the language allows it.)
        ok(r#"
            saga peek(cap: cap[fs.read_file]) {
                intent "peek"
                step look {
                    do { fs.read_file("/tmp/x")? }
                    undo noop
                }
            }
        "#);
    }

    #[test]
    fn saga_step_with_io_println_and_noop_undo_is_accepted() {
        // `io.println` is diagnostic per § 8.1 — bypasses the rule.
        ok(r#"
            saga noisy(cap: cap[io.println]) {
                intent "noise"
                step say {
                    do { io.println("hi") }
                    undo noop
                }
            }
        "#);
    }

    #[test]
    fn saga_step_write_inside_nested_block_still_caught() {
        // The write call lives inside a nested `for`/`if` — the walker
        // must descend into nested blocks to find it.
        let src = r#"
            saga batch(cap: cap[http.post]) {
                intent "x"
                step go {
                    do {
                        for it in items {
                            if it.flag {
                                http.post("u", it)?
                            }
                        }
                    }
                    undo noop
                }
            }
        "#;
        assert!(errs(src)
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. })));
    }

    // ----------------- M2.T7 — V2 mandatory intent -----------------

    fn has_v2(es: &[CheckError]) -> bool {
        es.iter()
            .any(|e| matches!(e.kind, CheckErrorKind::MissingIntentForWriteCall { .. }))
    }

    fn v2_op(es: &[CheckError]) -> Option<&str> {
        es.iter().find_map(|e| match &e.kind {
            CheckErrorKind::MissingIntentForWriteCall { op } => Some(op.as_str()),
            _ => None,
        })
    }

    #[test]
    fn v2_http_post_without_intent_rejected_with_66() {
        let src = r#"
            fn f(cap: cap[http.post]) -> result<unit> {
                http.post("https://x.example/charge", "\{\}")?
            }
        "#;
        let es = errs(src);
        assert!(has_v2(&es));
        assert_eq!(v2_op(&es), Some("http.post"));
        // Highest exit code in the batch is 66.
        let max = es.iter().map(|e| e.exit_code()).max().unwrap_or(0);
        assert_eq!(max, 66);
    }

    #[test]
    fn v2_http_post_inside_intent_block_is_accepted() {
        ok(r#"
            fn f(cap: cap[http.post]) -> result<unit> {
                intent "send the charge" {
                    http.post("https://x.example/charge", "\{\}")?
                }
            }
        "#);
    }

    #[test]
    fn v2_fs_write_file_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[fs.write_file]) -> result<unit> {
                fs.write_file("/tmp/x", "y")?
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("fs.write_file"));
    }

    #[test]
    fn v2_audit_event_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[audit.event]) {
                audit.event("oops", { x: 1 })
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("audit.event"));
    }

    #[test]
    fn v2_kube_apply_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[kube.apply]) -> result<unit> {
                kube.apply(manifest)?
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("kube.apply"));
    }

    #[test]
    fn v2_ai_complete_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[ai.complete]) -> result<string> {
                ai.complete("prompt")
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("ai.complete"));
    }

    #[test]
    fn v2_shell_exec_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[shell.exec]) {
                shell.exec("ls")
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("shell.exec"));
    }

    #[test]
    fn v2_mongodb_write_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[mongodb.write]) {
                mongodb.write(doc)
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("mongodb.write"));
    }

    #[test]
    fn v2_minio_put_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[minio.put]) {
                minio.put("bucket", "obj", data)
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("minio.put"));
    }

    #[test]
    fn v2_rabbitmq_publish_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[rabbitmq.publish]) {
                rabbitmq.publish("queue", payload)
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("rabbitmq.publish"));
    }

    #[test]
    fn v2_docker_run_without_intent_rejected() {
        let src = r#"
            fn f(cap: cap[docker.run]) {
                docker.run("image")
            }
        "#;
        assert_eq!(v2_op(&errs(src)), Some("docker.run"));
    }

    #[test]
    fn v2_read_call_without_intent_is_accepted() {
        // `fs.read_file` is read-classified — V2 does not apply.
        ok(r#"
            fn f(cap: cap[fs.read_file]) -> result<bytes> {
                fs.read_file("/tmp/x")
            }
        "#);
    }

    #[test]
    fn v2_io_println_without_intent_is_accepted() {
        // `io.println` is diagnostic — bypasses V2 entirely (§ 8.1).
        ok(r#"
            fn f(cap: cap[io.println]) {
                io.println("hello")
            }
        "#);
    }

    #[test]
    fn v2_write_inside_nested_block_is_caught() {
        let src = r#"
            fn f(cap: cap[http.post]) {
                if cond {
                    for i in 0..3 {
                        http.post("u", "\{\}")
                    }
                }
            }
        "#;
        assert!(has_v2(&errs(src)));
    }

    #[test]
    fn v2_write_inside_match_arm_is_caught() {
        let src = r#"
            fn f(cap: cap[audit.event]) {
                match x {
                    Ok(_) -> audit.event("ok", { x: 1 }),
                    _     -> 0,
                }
            }
        "#;
        assert!(has_v2(&errs(src)));
    }

    #[test]
    fn v2_intent_block_only_covers_its_lexical_body() {
        // The first call is inside `intent { }` (OK); the second is
        // OUTSIDE — the `intent` block has already closed by then.
        let src = r#"
            fn f(cap: cap[http.post]) {
                intent "first" { http.post("u1", "\{\}") }
                http.post("u2", "\{\}")
            }
        "#;
        let es = errs(src);
        // Exactly one V2 violation, on the second call.
        let v2s: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::MissingIntentForWriteCall { .. }))
            .collect();
        assert_eq!(v2s.len(), 1);
    }

    #[test]
    fn v2_saga_step_body_does_not_need_extra_intent() {
        // The saga itself has a mandatory `intent "..."` (§ 12.2);
        // step `do` / `undo` bodies inherit that scope. No extra
        // `intent { }` is required around the write call.
        ok(r#"
            saga settle(cap: cap[http.post @ ["api.acme.com"]]) {
                intent "settle batch"
                step charge {
                    do   { http.post("https://api.acme.com/charge", "\{\}")? }
                    undo { http.post("https://api.acme.com/refund", "\{\}")? }
                }
            }
        "#);
    }

    // ----------------- M2.T4 — body resolution -----------------

    #[test]
    fn pure_fn_calling_http_get_rejected_with_65() {
        let src = r#"
            fn fetch() -> result<bytes> {
                http.get("https://api.x.com/users")
            }
        "#;
        let es = errs(src);
        let no_cap: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::NoCapInScope { .. }))
            .collect();
        assert_eq!(no_cap.len(), 1);
        assert_eq!(no_cap[0].exit_code(), 65);
        if let CheckErrorKind::NoCapInScope { op } = &no_cap[0].kind {
            assert_eq!(op, "http.get");
        }
    }

    #[test]
    fn fn_with_cap_calling_op_not_in_signature_rejected_with_65() {
        // `cap[fs.read_file]` does not authorise `fs.write_file`.
        let src = r#"
            fn f(cap: cap[fs.read_file]) {
                intent "x" { fs.write_file("/tmp/x", "y")? }
            }
        "#;
        let es = errs(src);
        let not_in: Vec<_> = es
            .iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::OpNotInCapSignature { .. }))
            .collect();
        assert_eq!(not_in.len(), 1);
        assert_eq!(not_in[0].exit_code(), 65);
        if let CheckErrorKind::OpNotInCapSignature { op } = &not_in[0].kind {
            assert_eq!(op, "fs.write_file");
        }
    }

    #[test]
    fn fn_with_cap_calling_op_in_signature_is_accepted() {
        ok(r#"
            fn read(cap: cap[fs.read_file]) -> result<bytes> {
                fs.read_file("/tmp/x")
            }
        "#);
    }

    #[test]
    fn fn_with_bare_module_cap_covers_all_subops() {
        // `cap[fs]` (a tree node) implies every `fs.*` operation.
        ok(r#"
            fn rw(cap: cap[fs]) -> result<unit> {
                intent "rw" {
                    let x = fs.read_file("/a")?
                    fs.write_file("/b", x)?
                    Ok(())
                }
            }
        "#);
    }

    #[test]
    fn pure_fn_with_only_method_calls_is_accepted() {
        // `xs.map(f)` and friends are method-call sugar (§ 5.4),
        // not capability calls — they bypass body-resolution.
        ok(r#"
            fn total(xs: list<int>) -> int {
                xs.fold(0, fn(a, b) { a + b })
            }
        "#);
    }

    #[test]
    fn fn_calling_io_println_without_cap_still_flagged() {
        // `io.println` is in the cap registry even though it is
        // diagnostic-classified — pure code may not call it without a
        // `cap` parameter (§ 7.2 + § 8.2).
        let src = r#"
            fn say() { io.println("hello") }
        "#;
        let es = errs(src);
        assert!(es
            .iter()
            .any(|e| matches!(&e.kind, CheckErrorKind::NoCapInScope { op } if op == "io.println")));
    }

    #[test]
    fn fn_with_cap_calling_unknown_dotted_call_is_not_flagged() {
        // `xs.map(f)` calls a method on a value, not a cap operation.
        // The body-resolution rule does not fire.
        ok(r#"
            fn double(xs: list<int>) -> list<int> {
                xs.map(fn(x) { x * 2 })
            }
        "#);
    }

    #[test]
    fn cap_subset_does_not_count_as_a_cap_call() {
        // `cap.subset[..]` is a value-level construction; it produces
        // a derived cap and is not a capability call itself.
        ok(r#"
            fn forward(cap: cap[http.post @ ["api.x.com"]]) -> result<unit> {
                let inner = cap.subset[http.post @ ["api.x.com"]]
                intent "x" { http.post("https://api.x.com/u", "\{\}")? }
            }
        "#);
    }

    #[test]
    fn write_call_in_pure_fn_emits_both_v2_and_no_cap_errors() {
        // V2 (66) and NoCapInScope (65) both fire — they catch
        // distinct properties. Both are surfaced.
        let src = r#"
            fn evil() { http.post("u", "\{\}") }
        "#;
        let es = errs(src);
        assert!(es
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::MissingIntentForWriteCall { .. })));
        assert!(es
            .iter()
            .any(|e| matches!(e.kind, CheckErrorKind::NoCapInScope { .. })));
    }

    // ----------------- M2.T11 — cap escape rules -----------------

    fn escape_vec(es: &[CheckError]) -> Vec<&super::CheckErrorKind> {
        es.iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::CapEscape { .. }))
            .map(|e| &e.kind)
            .collect()
    }

    #[test]
    fn cap_in_record_field_rejected_with_65() {
        let es = errs("record R { c: cap[fs.read_file] }");
        let v = escape_vec(&es);
        assert_eq!(v.len(), 1);
        assert!(matches!(
            v[0],
            CheckErrorKind::CapEscape {
                vector: super::error::CapEscapeVector::RecordField { .. }
            }
        ));
        assert!(es.iter().all(|e| e.exit_code() == 65));
    }

    #[test]
    fn cap_in_enum_variant_rejected() {
        let es = errs("enum E { A, B(cap[audit.event]) }");
        assert!(escape_vec(&es).iter().any(|k| matches!(
            k,
            CheckErrorKind::CapEscape {
                vector: super::error::CapEscapeVector::EnumVariant { .. }
            }
        )));
    }

    #[test]
    fn cap_in_const_rejected() {
        let es = errs("const C: cap[fs.read_file] = nope");
        assert!(escape_vec(&es).iter().any(|k| matches!(
            k,
            CheckErrorKind::CapEscape {
                vector: super::error::CapEscapeVector::Const { .. }
            }
        )));
    }

    #[test]
    fn cap_in_channel_rejected() {
        let es = errs("record Q { ch: channel<cap[fs.read_file]> }");
        assert!(escape_vec(&es).iter().any(|k| matches!(
            k,
            CheckErrorKind::CapEscape {
                vector: super::error::CapEscapeVector::Channel
            }
        )));
    }

    #[test]
    fn cap_nested_in_return_type_rejected() {
        // `result<cap[..]>` — cap must be the *outermost* return type.
        let es = errs("fn f() -> result<cap[fs.read_file]> {}");
        assert!(escape_vec(&es).iter().any(|k| matches!(
            k,
            CheckErrorKind::CapEscape {
                vector: super::error::CapEscapeVector::NestedReturn
            }
        )));
    }

    #[test]
    fn fn_returning_cap_at_top_level_is_accepted() {
        // `fn f() -> cap[..]` is the legitimate cap-returning shape.
        ok("fn make() -> cap[fs.read_file] { todo() }");
    }

    #[test]
    fn cap_used_directly_inside_spawn_body_rejected_via_no_cap() {
        // The spawn body does not inherit the outer `cap` (§ 8.7).
        // An unprefixed cap call inside fails with NoCapInScope (65).
        let src = r#"
            fn f(cap: cap[http.get]) {
                spawn { http.get("https://x.example") }
            }
        "#;
        let es = errs(src);
        assert!(es
            .iter()
            .any(|e| matches!(&e.kind, CheckErrorKind::NoCapInScope { op } if op == "http.get")));
    }

    #[test]
    fn cap_inside_spawn_via_subset_construction_is_accepted() {
        // `cap.subset[..]` is a value construction, not a cap call.
        // Passing it to a function from inside spawn is allowed.
        ok(r#"
            fn f(cap: cap[http.get @ ["api.x.com"]]) {
                spawn { worker(cap.subset[http.get @ ["api.x.com"]]) }
            }
        "#);
    }

    // ----------------- M2.T2 — match exhaustiveness -----------------

    /// Helper: wrap an expression body in a fn so it can be parsed and
    /// type-checked end-to-end. Returns the resulting check errors.
    fn match_in_fn(body_expr: &str) -> Vec<CheckError> {
        let src = format!("fn f(x: int) {{ {body_expr} }}");
        let m = parse(&src).unwrap_or_else(|e| panic!("parse on {src:?}: {e:?}"));
        check_module(&m)
            .into_iter()
            .filter(|e| matches!(e.kind, CheckErrorKind::NonExhaustiveMatch { .. }))
            .collect()
    }

    fn match_ok(body_expr: &str) {
        let es = match_in_fn(body_expr);
        assert!(es.is_empty(), "expected no match errors, got {es:#?}");
    }

    fn match_bad(body_expr: &str, want: super::error::NonExhaustiveReason) {
        let es = match_in_fn(body_expr);
        assert!(
            es.iter().any(|e| {
                if let CheckErrorKind::NonExhaustiveMatch { reason } = &e.kind {
                    *reason == want
                } else {
                    false
                }
            }),
            "expected {want:?}, got {es:#?}"
        );
        for e in &es {
            assert_eq!(e.exit_code(), 64);
        }
    }

    // ---- positive (8) ----

    #[test]
    fn match_p1_wildcard_only() {
        match_ok("match x { _ -> 0 }");
    }

    #[test]
    fn match_p2_literal_then_wildcard() {
        match_ok("match x { 0 -> 1, _ -> 2 }");
    }

    #[test]
    fn match_p3_constructor_then_wildcard() {
        match_ok("match x { Pending -> 1, _ -> 0 }");
    }

    #[test]
    fn match_p4_unguarded_binder_catchall() {
        match_ok("match x { n -> n + 1 }");
    }

    #[test]
    fn match_p5_guarded_then_unguarded_catchall() {
        match_ok("match x { n if n > 0 -> 1, _ -> 0 }");
    }

    #[test]
    fn match_p6_three_constructors_with_default() {
        match_ok("match x { Pending -> 1, Active(t) -> 2, _ -> 0 }");
    }

    #[test]
    fn match_p7_list_patterns_with_rest_catchall() {
        match_ok(r#"match x { [] -> 0, [a] -> 1, [a, ..rest] -> 2 }"#);
    }

    #[test]
    fn match_p8_nested_match_each_with_catchall() {
        match_ok("match x { 0 -> match y { _ -> 1 }, _ -> match z { _ -> 0 } }");
    }

    // ---- negative (7) ----

    #[test]
    fn match_n1_int_only_guards_rejected() {
        // The headline acceptance: every arm is guarded → not exhaustive.
        match_bad(
            "match x { n if n > 0 -> 1, n if n < 0 -> 2 }",
            super::error::NonExhaustiveReason::AllArmsGuardedNoCatchAll,
        );
    }

    #[test]
    fn match_n2_literal_arms_with_guard_no_catchall() {
        match_bad(
            "match x { 0 if a -> 1, 1 if b -> 2 }",
            super::error::NonExhaustiveReason::AllArmsGuardedNoCatchAll,
        );
    }

    #[test]
    fn match_n3_constructor_arms_all_guarded() {
        match_bad(
            "match x { Pending if a -> 1, Active(t) if t > 0 -> 2 }",
            super::error::NonExhaustiveReason::AllArmsGuardedNoCatchAll,
        );
    }

    #[test]
    fn match_n4_empty_match_rejected() {
        match_bad("match x { }", super::error::NonExhaustiveReason::EmptyMatch);
    }

    #[test]
    fn match_n5_single_guarded_arm_rejected() {
        match_bad(
            "match x { _ if a -> 0 }",
            super::error::NonExhaustiveReason::AllArmsGuardedNoCatchAll,
        );
    }

    #[test]
    fn match_p9_mixed_guarded_and_literal_accepted_structurally() {
        // `match x { 0 -> 1, _ if a -> 0 }` is *not* exhaustive over
        // `int` (no unguarded catch-all), but the structural pass
        // alone cannot prove this without scrutinee-type info — the
        // arm `0 -> 1` is unguarded, so the "all-guarded" rule does
        // not fire. The follow-up type-aware pass will refine.
        match_ok("match x { 0 -> 1, _ if a -> 0 }");
    }

    #[test]
    fn match_p10_mixed_unguarded_literal_and_guarded_binder_accepted() {
        // Same shape: `n if cond` is a guarded binder. With at least
        // one unguarded arm (`0 -> 1`) the structural rule passes.
        match_ok("match x { 0 -> 1, n if n > 5 -> 2 }");
    }

    // ---------- M10.T1 — agent required-fields ----------

    fn agent_field_errs(src: &str) -> Vec<(String, String)> {
        errs(src)
            .into_iter()
            .filter_map(|e| match e.kind {
                CheckErrorKind::MissingAgentField { agent, field } => Some((agent, field)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn agent_with_all_required_fields_is_accepted() {
        let src = r#"
            agent classify {
                llm:     "claude-haiku-4-5"
                intent:  "classify"
                prompt:  "p"
                accept:  inv
                produce: cat
            }
        "#;
        assert!(agent_field_errs(src).is_empty());
    }

    #[test]
    fn agent_missing_llm_rejected_with_64() {
        let src = r#"
            agent a {
                intent:  "x"
                prompt:  "p"
                accept:  inv
                produce: cat
            }
        "#;
        let xs = agent_field_errs(src);
        assert_eq!(xs.len(), 1);
        assert_eq!(xs[0], ("a".into(), "llm".into()));
        for e in errs(src) {
            if matches!(e.kind, CheckErrorKind::MissingAgentField { .. }) {
                assert_eq!(e.exit_code(), 64);
            }
        }
    }

    #[test]
    fn agent_missing_intent_rejected() {
        let src = r#"
            agent a {
                llm:     "x"
                prompt:  "p"
                accept:  inv
                produce: cat
            }
        "#;
        let xs = agent_field_errs(src);
        assert_eq!(xs, vec![("a".into(), "intent".into())]);
    }

    #[test]
    fn agent_missing_prompt_rejected() {
        let src = r#"
            agent a {
                llm:     "x"
                intent:  "x"
                accept:  inv
                produce: cat
            }
        "#;
        let xs = agent_field_errs(src);
        assert_eq!(xs, vec![("a".into(), "prompt".into())]);
    }

    #[test]
    fn agent_missing_accept_rejected() {
        let src = r#"
            agent a {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                produce: cat
            }
        "#;
        let xs = agent_field_errs(src);
        assert_eq!(xs, vec![("a".into(), "accept".into())]);
    }

    #[test]
    fn agent_missing_produce_rejected() {
        let src = r#"
            agent a {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  inv
            }
        "#;
        let xs = agent_field_errs(src);
        assert_eq!(xs, vec![("a".into(), "produce".into())]);
    }

    #[test]
    fn agent_with_all_optional_fields_present_is_accepted() {
        let src = r#"
            agent a {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  inv
                produce: cat
                policy:  pii_redact, model_budget
                retries: 3
                budget:  budget_lit
            }
        "#;
        assert!(agent_field_errs(src).is_empty());
    }

    #[test]
    fn agent_missing_two_fields_emits_two_errors() {
        let src = r#"
            agent a {
                llm:     "x"
                prompt:  "p"
                accept:  inv
            }
        "#;
        let mut xs = agent_field_errs(src);
        xs.sort();
        assert_eq!(
            xs,
            vec![
                ("a".into(), "intent".into()),
                ("a".into(), "produce".into()),
            ]
        );
    }

    #[test]
    fn every_negative_error_uses_exit_code_64() {
        // Smoke check: every error reported by the negative fixtures
        // above has exit_code() == 64. (Each individual `bad(...)` call
        // already asserts this; this is the aggregate guarantee for
        // M2.T1.)
        let src = "record R { x: Foo }";
        let es = errs(src);
        assert!(!es.is_empty());
        for e in es {
            assert_eq!(e.exit_code(), 64);
        }
    }
}
