//! M13.T3 / M13.T4 — human-grade diagnostic renderer.
//!
//! Each `CheckError` carries a kind plus a source `Span`. The
//! renderer turns that into:
//!
//! 1. one **headline** line (`error[E65]: cap[*] forbidden in user code`)
//!    naming the rule and linking back to the `language.md` section
//!    that defines it (M13.T3 / thesis § 11.5);
//! 2. one **caret** line quoting the offending source range with a
//!    Rust-style `^^^^` underline (M13.T4).
//!
//! `aeris check --explain <code>` reuses the same rule catalogue to
//! print a manpage-style positive / negative example for each exit
//! code (M13.T6).

use super::error::{CapEscapeVector, CheckError, CheckErrorKind, NonExhaustiveReason};
use crate::syntax::token::Span;

/// Render a single diagnostic against `source`. The output is
/// multi-line and ends with a trailing newline so callers can
/// concatenate without inserting separators.
pub fn render_diagnostic(source: &str, err: &CheckError) -> String {
    let mut out = String::new();
    let info = rule_info(&err.kind);
    out.push_str(&format!(
        "error[E{code}]: {message}  (see language.md {section})\n",
        code = err.exit_code(),
        message = headline(&err.kind),
        section = info.section,
    ));
    out.push_str(&render_caret(source, err.span));
    if let Some(hint) = suggestion(&err.kind) {
        out.push_str(&format!("       = help: {hint}\n"));
    }
    out
}

/// M13.T5 — "Did you mean …?" / actionable hint per error kind.
/// Returns `None` when no suggestion is appropriate. The hint is a
/// single line that follows the caret block in the rendered output.
pub fn suggestion(k: &CheckErrorKind) -> Option<String> {
    match k {
        CheckErrorKind::UnknownType(name) => {
            // Suggest the closest primitive / stdlib container by
            // edit-distance — common typos like `intt`, `bigint` etc.
            closest_match(name, KNOWN_TYPES)
                .map(|s| format!("did you mean `{s}`?"))
        }
        CheckErrorKind::NoCapInScope { op } => {
            // The body uses `<module>.<op>` but no cap is in scope —
            // tell the user how to thread one through.
            Some(format!(
                "add a `cap: cap[{op}]` parameter to the enclosing function"
            ))
        }
        CheckErrorKind::OpNotInCapSignature { op } => Some(format!(
            "add `{op}` to the function's `cap[...]` parameter (or call inside an `intent` block)"
        )),
        CheckErrorKind::MissingIntentForWriteCall { op } => Some(format!(
            "wrap the call in an `intent \"...\" {{ {op}(...) }}` block"
        )),
        CheckErrorKind::CapStarInUserCode => {
            Some("only `main`'s synthesised cap may use `*`; list explicit operations instead".into())
        }
        CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. } => Some(
            "replace `undo noop` with an explicit `undo { ... }` block that compensates the `do`".into(),
        ),
        CheckErrorKind::BareModelWithoutVersion(name) => Some(format!(
            "add the version tag — `{name}@v1` (or whichever version applies)"
        )),
        CheckErrorKind::AgentNetCycle { net, .. } => Some(format!(
            "express iteration on `agent_net {net}` via `until: <expr>`, not a back-edge"
        )),
        CheckErrorKind::AllowListOutsideLockset { entry, family, .. } => Some(format!(
            "add `{entry}` to `[caps] {family}` in `lockset.toml`, or remove it from the signature"
        )),
        CheckErrorKind::CapEscape { vector } => match vector {
            CapEscapeVector::RecordField { .. } | CapEscapeVector::EnumVariant { .. } => {
                Some("pass the `cap` as a function argument instead of storing it".into())
            }
            CapEscapeVector::Const { .. } => Some(
                "construct the cap inside `main` and pass it down — module-level `const` cannot hold a cap".into(),
            ),
            CapEscapeVector::Channel => Some(
                "send a value derived from the cap (e.g. a request struct), not the cap itself".into(),
            ),
            CapEscapeVector::NestedReturn => Some(
                "return `cap[...]` directly — wrapping it in `result<...>` / `option<...>` is forbidden".into(),
            ),
        },
        _ => None,
    }
}

const KNOWN_TYPES: &[&str] = &[
    "bool", "int", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64", "decimal",
    "string", "bytes", "char", "uuid", "date", "timestamp", "duration", "unit", "list", "set",
    "map", "option", "result", "channel", "handle", "range",
];

/// Pick the candidate from `pool` with the smallest Damerau-style edit
/// distance to `target`, provided it's "close enough" (< 1/3 of the
/// target length, with a floor of 1). Returns `None` otherwise so the
/// renderer suppresses the hint instead of suggesting nonsense.
fn closest_match(target: &str, pool: &[&'static str]) -> Option<&'static str> {
    let max_dist = target.len().div_ceil(3).max(1);
    pool.iter()
        .map(|cand| (*cand, levenshtein(target, cand)))
        .filter(|(_, d)| *d <= max_dist)
        .min_by_key(|(_, d)| *d)
        .map(|(cand, _)| cand)
}

fn levenshtein(a: &str, b: &str) -> usize {
    let av: Vec<char> = a.chars().collect();
    let bv: Vec<char> = b.chars().collect();
    let n = av.len();
    let m = bv.len();
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut cur: Vec<usize> = vec![0; m + 1];
    for i in 1..=n {
        cur[0] = i;
        for j in 1..=m {
            let cost = if av[i - 1] == bv[j - 1] { 0 } else { 1 };
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m]
}

/// Quote the line that contains `span` and underline the offending
/// range with `^^^^`. Mirrors the Rust compiler's diagnostic style:
///
/// ```text
///   --> line 3, col 18
///   3 | fn f(cap: cap[*]) {}
///     |               ^
/// ```
pub fn render_caret(source: &str, span: Span) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let line_idx = (span.line.saturating_sub(1)) as usize;
    let col = span.col.saturating_sub(1) as usize;
    let mut out = String::new();
    out.push_str(&format!("  --> line {}, col {}\n", span.line, span.col));
    if line_idx >= lines.len() {
        return out;
    }
    let line = lines[line_idx];
    let line_no = format!("{:>4}", span.line);
    out.push_str(&format!("  {line_no} | {line}\n"));
    let pad = " ".repeat(col.min(line.len()));
    let len = (span.end.saturating_sub(span.start)).max(1) as usize;
    let underline = "^".repeat(len.max(1));
    out.push_str(&format!("       | {pad}{underline}\n"));
    out
}

/// Headline for the diagnostic — the *what*.
fn headline(k: &CheckErrorKind) -> String {
    match k {
        CheckErrorKind::UnknownType(name) => format!("unknown type `{name}`"),
        CheckErrorKind::WrongTypeArity {
            name,
            expected,
            found,
        } => format!(
            "type `{name}` takes {expected} type argument{plural}, but {found} given",
            plural = if *expected == 1 { "" } else { "s" }
        ),
        CheckErrorKind::ArityRequired(name) => format!("type `{name}` needs type arguments"),
        CheckErrorKind::UnboundGeneric(name) => format!("unbound generic `{name}`"),
        CheckErrorKind::CyclicTypeAlias(name) => format!("cyclic type alias `{name}`"),
        CheckErrorKind::DuplicateDecl(name) => format!("duplicate declaration `{name}`"),
        CheckErrorKind::DuplicateField { decl, field } => {
            format!("duplicate field `{field}` in `{decl}`")
        }
        CheckErrorKind::DuplicateVariant { decl, variant } => {
            format!("duplicate variant `{variant}` in enum `{decl}`")
        }
        CheckErrorKind::DuplicateGeneric { decl, name } => {
            format!("duplicate generic parameter `{name}` on `{decl}`")
        }
        CheckErrorKind::ModelVersionConflict { name, version } => {
            format!("model `{name}@v{version}` declared more than once")
        }
        CheckErrorKind::CapStarInUserCode => {
            "`cap[*]` is forbidden outside `main`'s synthesised cap".into()
        }
        CheckErrorKind::AgentNetCycle { net, chain } => {
            format!("cycle in `agent_net {net}`: {chain}")
        }
        CheckErrorKind::BareModelWithoutVersion(name) => {
            format!("model `{name}` referenced without `@vN`")
        }
        CheckErrorKind::SagaStepUndoNoopWithWriteDo { saga, step } => format!(
            "saga `{saga}` step `{step}`: write-effectful `do` requires a paired `undo`"
        ),
        CheckErrorKind::MissingIntentForWriteCall { op } => {
            format!("write-effectful `{op}` outside any `intent` block")
        }
        CheckErrorKind::NoCapInScope { op } => {
            format!("call to `{op}` has no `cap` in scope")
        }
        CheckErrorKind::OpNotInCapSignature { op } => {
            format!("`{op}` not authorised by the in-scope `cap`")
        }
        CheckErrorKind::CapEscape { vector } => match vector {
            CapEscapeVector::RecordField { record, field } => {
                format!("`cap` cannot be stored in record field `{record}.{field}`")
            }
            CapEscapeVector::EnumVariant { enum_name, variant } => {
                format!("`cap` cannot be stored in enum variant `{enum_name}::{variant}`")
            }
            CapEscapeVector::Const { name } => {
                format!("`cap` cannot be assigned to const `{name}`")
            }
            CapEscapeVector::Channel => "`cap` cannot be sent through a channel".into(),
            CapEscapeVector::NestedReturn => {
                "`cap` cannot be nested inside a non-cap return type".into()
            }
        },
        CheckErrorKind::MissingAgentField { agent, field } => {
            format!("agent `{agent}` is missing required field `{field}`")
        }
        CheckErrorKind::NonExhaustiveMatch { reason } => match reason {
            NonExhaustiveReason::EmptyMatch => "non-exhaustive match: no arms".into(),
            NonExhaustiveReason::AllArmsGuardedNoCatchAll => {
                "non-exhaustive match: every arm is guarded; add a catch-all".into()
            }
        },
        CheckErrorKind::AllowListOutsideLockset { op, entry, family } => format!(
            "`{op} @ \"{entry}\"` is outside the lockset ceiling `[caps] {family}`"
        ),
    }
}

/// Catalogue entry for a rule. The renderer surfaces `section`
/// inline with each diagnostic; `aeris check --explain <code>` has its
/// own positive / negative examples per exit code.
struct RuleInfo {
    section: &'static str,
}

fn rule_info(k: &CheckErrorKind) -> RuleInfo {
    let section = rule_section(k);
    RuleInfo { section }
}

fn rule_section(k: &CheckErrorKind) -> &'static str {
    match k {
        CheckErrorKind::UnknownType(_)
        | CheckErrorKind::WrongTypeArity { .. }
        | CheckErrorKind::ArityRequired(_)
        | CheckErrorKind::UnboundGeneric(_)
        | CheckErrorKind::CyclicTypeAlias(_)
        | CheckErrorKind::DuplicateDecl(_)
        | CheckErrorKind::DuplicateField { .. }
        | CheckErrorKind::DuplicateVariant { .. }
        | CheckErrorKind::DuplicateGeneric { .. } => "§ 4",
        CheckErrorKind::NonExhaustiveMatch { .. } => "§ 17.2",
        CheckErrorKind::ModelVersionConflict { .. } => "§ 16.1",
        CheckErrorKind::MissingAgentField { .. } => "§ 13.1",
        CheckErrorKind::CapStarInUserCode => "§ 8.4",
        CheckErrorKind::NoCapInScope { .. } => "§ 8.2",
        CheckErrorKind::OpNotInCapSignature { .. } => "§ 8.3",
        CheckErrorKind::CapEscape { .. } => "§ 8.7",
        CheckErrorKind::MissingIntentForWriteCall { .. } => "§ 10.1",
        CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. } => "§ 12.2",
        CheckErrorKind::BareModelWithoutVersion(_) => "§ 16.1",
        CheckErrorKind::AgentNetCycle { .. } => "§ 14.1",
        CheckErrorKind::AllowListOutsideLockset { .. } => "§ 8.3.2",
    }
}

/// Manpage-style content for `aeris check --explain <code>`. Stable
/// strings — keep them tight; the M13.T6 acceptance is "manpage-style
/// content for codes 64–71".
pub fn explain(code: u8) -> Option<String> {
    let (title, section, summary, example_bad, example_good) = match code {
        64 => (
            "parse / type / declaration error",
            "§ 4",
            "The program contains a syntactic, type-resolution, or pattern-exhaustiveness problem detected before execution.",
            "record R { x: bigint }",
            "record R { x: int }",
        ),
        65 => (
            "capability error",
            "§ 8",
            "A capability call lacks an in-scope `cap`, requests an operation outside its declared shape, uses `cap[*]` in user code, or escapes through a forbidden vector.",
            "fn f() { http.get(\"https://x.example\") }",
            "fn f(cap: cap[http.get]) { http.get(\"https://x.example\") }",
        ),
        66 => (
            "missing intent on write-effectful call",
            "§ 10.1",
            "Every write-effectful capability call must execute inside a lexical `intent \"...\"` block (V2).",
            "fn f(cap: cap[http.post]) { http.post(\"u\", \"{}\") }",
            "fn f(cap: cap[http.post]) { intent \"send\" { http.post(\"u\", \"{}\") } }",
        ),
        67 => (
            "saga step lacks paired undo",
            "§ 12.2",
            "A saga step whose `do` reaches a write-effectful capability must declare an explicit `undo` block; `undo noop` is rejected.",
            "step a { do { http.post(\"u\", \"{}\")? } undo noop }",
            "step a { do { http.post(\"u\", \"{}\")? } undo { http.post(\"u/refund\", \"{}\")? } }",
        ),
        68 => (
            "model version missing or conflicting",
            "§ 16.1",
            "Every reference to a `model` must carry an explicit `@vN` tag; declaring two `model X@vN` for the same N is also rejected.",
            "model Invoice@v1 { id: uuid }\nrecord B { x: Invoice }",
            "model Invoice@v1 { id: uuid }\nrecord B { x: Invoice@v1 }",
        ),
        69 => (
            "lockfile drift / hash mismatch / malformed lockset",
            "§ 24",
            "`lockset.toml` is malformed, a dep's hash does not match its bytes, or the surface lock is stale.",
            "deps.utils.hash = \"sha256:abc\"",
            "deps.utils.hash = \"blake3:9b18\"",
        ),
        70 => (
            "cycle in agent_net",
            "§ 14.1",
            "`agent_net` declares a typed dataflow DAG; iteration must be expressed via `until:`, never via a back-edge.",
            "agent_net p { flow a -> b -> a }",
            "agent_net p { flow a -> b until: a.done }",
        ),
        71 => (
            "allow-list violation (signature outside lockset ceiling)",
            "§ 8.3.2",
            "A function signature requested an `@ \"<endpoint>\"` value outside the project's `[caps]` ceiling.",
            "fn f(cap: cap[http.post @ \"evil.com\"]) {}",
            "fn f(cap: cap[http.post @ \"api.acme.com\"]) {}",
        ),
        _ => return None,
    };
    Some(format!(
        "E{code} — {title} (language.md {section})\n\n{summary}\n\n  bad:\n    {example_bad}\n\n  good:\n    {example_good}\n",
        code = code,
        title = title,
        section = section,
        summary = summary,
        example_bad = example_bad,
        example_good = example_good,
    ))
}

// ====================================================================
//  Tests — M13.T3 / M13.T4 / M13.T6 acceptance
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::check_module;
    use crate::syntax::parse;

    fn first(src: &str) -> (String, CheckError) {
        let m = parse(src).unwrap();
        let errs = check_module(&m);
        let err = errs.into_iter().next().expect("expected at least one error");
        (src.to_string(), err)
    }

    fn first_with_kind(
        src: &str,
        pred: impl Fn(&CheckErrorKind) -> bool,
    ) -> (String, CheckError) {
        let m = parse(src).unwrap();
        let err = check_module(&m)
            .into_iter()
            .find(|e| pred(&e.kind))
            .expect("expected matching error");
        (src.to_string(), err)
    }

    // ---- M13.T3: every rule references its language.md section ----

    fn rendered(src: &str, pred: impl Fn(&CheckErrorKind) -> bool) -> String {
        let (s, e) = first_with_kind(src, pred);
        render_diagnostic(&s, &e)
    }

    #[test]
    fn unknown_type_diagnostic_links_to_section_4() {
        let r = rendered(
            "record R { x: bigint }",
            |k| matches!(k, CheckErrorKind::UnknownType(_)),
        );
        assert!(r.contains("error[E64]"));
        assert!(r.contains("§ 4"));
        assert!(r.contains("unknown type `bigint`"));
    }

    #[test]
    fn cap_star_diagnostic_links_to_section_8_4() {
        let r = rendered("fn f(cap: cap[*]) {}", |k| {
            matches!(k, CheckErrorKind::CapStarInUserCode)
        });
        assert!(r.contains("error[E65]"));
        assert!(r.contains("§ 8.4"));
    }

    #[test]
    fn v2_diagnostic_links_to_section_10_1() {
        let r = rendered(
            "fn f(cap: cap[http.post]) { http.post(\"u\", \"{}\") }",
            |k| matches!(k, CheckErrorKind::MissingIntentForWriteCall { .. }),
        );
        assert!(r.contains("error[E66]"));
        assert!(r.contains("§ 10.1"));
    }

    #[test]
    fn saga_diagnostic_links_to_section_12_2() {
        let src = r#"
            saga s(cap: cap[http.post]) {
                intent "x"
                step a { do { http.post("u", "{}")? } undo noop }
            }
        "#;
        let r = rendered(src, |k| {
            matches!(k, CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. })
        });
        assert!(r.contains("error[E67]"));
        assert!(r.contains("§ 12.2"));
    }

    #[test]
    fn bare_model_diagnostic_links_to_section_16_1() {
        let src = "model Invoice@v1 { id: uuid }\nrecord B { x: Invoice }";
        let r = rendered(src, |k| {
            matches!(k, CheckErrorKind::BareModelWithoutVersion(_))
        });
        assert!(r.contains("error[E68]"));
        assert!(r.contains("§ 16.1"));
    }

    #[test]
    fn agent_net_cycle_diagnostic_links_to_section_14_1() {
        let r = rendered("agent_net p { flow a -> b -> a }", |k| {
            matches!(k, CheckErrorKind::AgentNetCycle { .. })
        });
        assert!(r.contains("error[E70]"));
        assert!(r.contains("§ 14.1"));
    }

    // ---- M13.T4: caret underline ----

    #[test]
    fn caret_underlines_offending_range() {
        let src = "record R { x: bigint }";
        let (s, e) = first(src);
        let r = render_diagnostic(&s, &e);
        // The `^` line is present and points at the type name.
        assert!(r.contains("^"));
        assert!(r.contains("--> line"));
        // The quoted source line appears in full.
        assert!(r.contains("record R { x: bigint }"));
    }

    #[test]
    fn caret_handles_multiline_source() {
        let src = "record R {\n    x: bigint\n}";
        let (s, e) = first(src);
        let r = render_diagnostic(&s, &e);
        assert!(r.contains("--> line 2"));
        assert!(r.contains("x: bigint"));
    }

    #[test]
    fn caret_renders_30_negative_fixtures_without_panic() {
        // The acceptance check from the plan: snapshot test on 30
        // negative fixtures. We render every diagnostic and check
        // each output names the right exit code and has a caret.
        let cases: &[(&str, u8)] = &[
            ("record R { x: bigint }", 64),
            ("record R { xs: list<int, string> }", 64),
            ("record R { kv: map<int> }", 64),
            ("record R { xs: list }", 64),
            ("record R { x: T }", 64),
            ("record R { a: int } record R { b: int }", 64),
            ("record R { a: int, a: string }", 64),
            ("enum E { A, A }", 64),
            ("record P<T, T> { x: T }", 64),
            ("type A = B\ntype B = A", 64),
            ("model M@v1 { a: int }\nmodel M@v1 { b: int }", 64),
            ("model M@v1 { id: uuid }\nrecord B { x: M }", 68),
            ("agent_net p { flow a -> b -> a }", 70),
            ("agent_net p { flow a -> a }", 70),
            ("fn f(cap: cap[*]) {}", 65),
            ("record R { c: cap[fs.read_file] }", 65),
            ("enum E { A, B(cap[audit.event]) }", 65),
            ("const C: cap[fs.read_file] = nope", 65),
            ("record Q { ch: channel<cap[fs.read_file]> }", 65),
            ("fn f() -> result<cap[fs.read_file]> {}", 65),
            ("fn f() { http.get(\"https://x\") }", 65),
            ("fn f(cap: cap[fs.read_file]) { intent \"x\" { fs.write_file(\"/x\", \"y\")? } }", 65),
            ("fn f(cap: cap[http.post]) { http.post(\"u\", \"{}\") }", 66),
            ("fn f(cap: cap[fs.write_file]) { fs.write_file(\"/x\", \"y\") }", 66),
            ("fn f(cap: cap[audit.event]) { audit.event(\"oops\", { x: 1 }) }", 66),
            ("fn f(cap: cap[ai.complete]) -> result<string> { ai.complete(\"p\") }", 66),
            ("fn f(cap: cap[shell.exec]) { shell.exec(\"ls\") }", 66),
            ("saga s(cap: cap[http.post]) { intent \"x\" step a { do { http.post(\"u\", \"{}\")? } undo noop } }", 67),
            ("saga r(cap: cap[audit.event]) { intent \"x\" step a { do { audit.event(\"x\", { a: 1 }) } undo noop } }", 67),
            ("agent a { intent: \"x\" prompt: \"p\" accept: i produce: o }", 64),
        ];
        for (src, expected_code) in cases {
            let m = parse(src).unwrap_or_else(|e| panic!("parse {src:?}: {e:?}"));
            let errs = check_module(&m);
            let matching: Vec<&CheckError> = errs
                .iter()
                .filter(|e| e.exit_code() == *expected_code)
                .collect();
            assert!(
                !matching.is_empty(),
                "no error with exit {expected_code} for {src:?} (got {errs:#?})"
            );
            let r = render_diagnostic(src, matching[0]);
            assert!(
                r.contains(&format!("error[E{expected_code}]")),
                "rendered output for {src:?} missing E{expected_code}: {r}"
            );
            assert!(r.contains("--> line"), "missing --> line in {r}");
            assert!(r.contains('^'), "missing caret in {r}");
        }
    }

    // ---- M13.T6: --explain ----

    #[test]
    fn explain_covers_codes_64_through_71() {
        for code in 64..=71u8 {
            assert!(explain(code).is_some(), "no explain entry for E{code}");
        }
    }

    #[test]
    fn explain_unknown_code_returns_none() {
        assert!(explain(0).is_none());
        assert!(explain(255).is_none());
    }

    #[test]
    fn explain_carries_section_and_examples() {
        let body = explain(66).unwrap();
        assert!(body.contains("E66"));
        assert!(body.contains("§ 10.1"));
        assert!(body.contains("bad:"));
        assert!(body.contains("good:"));
        assert!(body.contains("intent"));
    }

    #[test]
    fn explain_for_71_mentions_lockset_and_allow_list() {
        let body = explain(71).unwrap();
        assert!(body.contains("E71"));
        assert!(body.contains("allow-list"));
        assert!(body.contains("§ 8.3.2"));
    }

    // ---- M13.T5: "did you mean ...?" / actionable hints ----

    fn hint_for(src: &str, pred: impl Fn(&CheckErrorKind) -> bool) -> Option<String> {
        let m = parse(src).unwrap();
        let err = check_module(&m).into_iter().find(|e| pred(&e.kind))?;
        suggestion(&err.kind)
    }

    #[test]
    fn unknown_type_with_close_match_suggests_correction() {
        let h = hint_for("record R { x: intt }", |k| {
            matches!(k, CheckErrorKind::UnknownType(_))
        })
        .unwrap();
        assert!(h.contains("did you mean"));
        assert!(h.contains("`int`"));
    }

    #[test]
    fn unknown_type_with_no_close_match_yields_no_hint() {
        // `Bogus` is not within edit-distance of any primitive; the
        // renderer should suppress the hint rather than suggest noise.
        let h = hint_for("record R { x: Bogus }", |k| {
            matches!(k, CheckErrorKind::UnknownType(_))
        });
        assert!(h.is_none());
    }

    #[test]
    fn no_cap_in_scope_suggests_adding_cap_param() {
        let h = hint_for(
            "fn f() { http.get(\"https://x\") }",
            |k| matches!(k, CheckErrorKind::NoCapInScope { .. }),
        )
        .unwrap();
        assert!(h.contains("cap: cap[http.get]"));
    }

    #[test]
    fn op_not_in_cap_signature_suggests_adding_op() {
        let h = hint_for(
            "fn f(cap: cap[fs.read_file]) { intent \"x\" { fs.write_file(\"/x\", \"y\")? } }",
            |k| matches!(k, CheckErrorKind::OpNotInCapSignature { .. }),
        )
        .unwrap();
        assert!(h.contains("fs.write_file"));
        assert!(h.contains("cap[...]"));
    }

    #[test]
    fn missing_intent_suggests_wrapping_in_intent_block() {
        let h = hint_for(
            "fn f(cap: cap[http.post]) { http.post(\"u\", \"{}\") }",
            |k| matches!(k, CheckErrorKind::MissingIntentForWriteCall { .. }),
        )
        .unwrap();
        assert!(h.contains("intent"));
        assert!(h.contains("http.post"));
    }

    #[test]
    fn cap_star_suggests_explicit_operations() {
        let h = hint_for("fn f(cap: cap[*]) {}", |k| {
            matches!(k, CheckErrorKind::CapStarInUserCode)
        })
        .unwrap();
        assert!(h.contains("explicit"));
    }

    #[test]
    fn saga_undo_noop_suggests_explicit_undo_block() {
        let src = r#"
            saga s(cap: cap[http.post]) {
                intent "x"
                step a { do { http.post("u", "{}")? } undo noop }
            }
        "#;
        let h = hint_for(src, |k| {
            matches!(k, CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. })
        })
        .unwrap();
        assert!(h.contains("undo {"));
    }

    #[test]
    fn bare_model_suggests_adding_version_tag() {
        let h = hint_for(
            "model Invoice@v1 { id: uuid }\nrecord B { x: Invoice }",
            |k| matches!(k, CheckErrorKind::BareModelWithoutVersion(_)),
        )
        .unwrap();
        assert!(h.contains("Invoice@v1"));
    }

    #[test]
    fn agent_net_cycle_suggests_until_clause() {
        let h = hint_for("agent_net p { flow a -> b -> a }", |k| {
            matches!(k, CheckErrorKind::AgentNetCycle { .. })
        })
        .unwrap();
        assert!(h.contains("until:"));
    }

    #[test]
    fn cap_in_record_field_suggests_passing_as_argument() {
        let h = hint_for("record R { c: cap[fs.read_file] }", |k| {
            matches!(k, CheckErrorKind::CapEscape { .. })
        })
        .unwrap();
        assert!(h.contains("argument"));
    }

    #[test]
    fn rendered_diagnostic_includes_help_line_when_hint_available() {
        let m = parse("record R { x: intt }").unwrap();
        let err = check_module(&m)
            .into_iter()
            .find(|e| matches!(e.kind, CheckErrorKind::UnknownType(_)))
            .unwrap();
        let r = render_diagnostic("record R { x: intt }", &err);
        assert!(r.contains("help: did you mean"));
    }
}
