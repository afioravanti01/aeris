//! Aeris test harness: parallel `aeris test` runner, property
//! generators, golden-trace differ.
//!
//! Realises `docs/language.md` § 21 (tests / properties) and
//! `docs/plan.md` § 6 (test artifacts).

pub mod property;

use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread;

use crate::runtime::eval::{run_test, run_test_with_fixture, AssertionCmpOp, EvalError, EvalErrorKind};
use crate::syntax::ast::{Item, Module};

pub use property::{run_property, PropertyOutcome, DEFAULT_CASES};

/// Status of a single `test "..." { ... }` body within a suite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestStatus {
    Passed,
    /// The body raised an evaluator error or a contract / policy /
    /// schema violation. The string is the rendered failure reason
    /// (M12.T2 will refine the format with `expected vs. actual`).
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct TestOutcome {
    pub suite: String,
    pub name: String,
    pub status: TestStatus,
}

#[derive(Debug, Clone, Default)]
pub struct RunReport {
    pub outcomes: Vec<TestOutcome>,
    /// Suite-level failures: a `.test.aer` file that failed to parse.
    pub parse_failures: Vec<(String, String)>,
}

impl RunReport {
    pub fn passed(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| o.status == TestStatus::Passed)
            .count()
    }

    pub fn failed(&self) -> usize {
        self.outcomes.len() - self.passed() + self.parse_failures.len()
    }

    /// Process exit code per § 21.2 / `docs/plan.md` § 5.12 M12.T1:
    /// `0` if every test passed and every suite parsed; `1` otherwise.
    pub fn exit_code(&self) -> u8 {
        if self.failed() == 0 {
            0
        } else {
            1
        }
    }
}

/// Discover every `*.test.aer` file under `root` (recursive). Returns
/// `(suite_name, absolute_path)` pairs. The suite name is the file
/// stem with the trailing `.test` removed — `tests/foo.test.aer` →
/// suite `foo`.
pub fn discover_suites(root: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    walk(root, &mut out);
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn walk(dir: &Path, out: &mut Vec<(String, PathBuf)>) {
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return,
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk(&p, out);
            continue;
        }
        if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
            if let Some(stem) = name.strip_suffix(".test.aer") {
                out.push((stem.to_string(), p));
            }
        }
    }
}

/// Run every test discovered under `root`, in parallel across the
/// available CPUs (one suite per worker; tests *within* a suite run
/// sequentially because they share the same module env). Returns the
/// aggregate report.
pub fn run_suites(root: &Path) -> RunReport {
    let suites = discover_suites(root);
    run_suites_explicit(&suites)
}

/// M43 — configuration threaded into every test body by the CLI.
/// `cmd_test` builds this from `aeris.toml`; library callers can
/// hand it any subset they need.
#[derive(Clone, Default)]
pub struct SuiteConfig {
    pub cap: Option<crate::runtime::value::CapValue>,
    pub ai_backend: Option<std::rc::Rc<crate::manifest::AiBackend>>,
    pub l2_backends: Option<std::rc::Rc<crate::runtime::l2_backend::L2Backends>>,
    pub active_policy_names: Option<Vec<String>>,
}

/// Run an explicit list of `(suite_name, path)` pairs. Used by the
/// CLI when the user passes a single file or glob, and by the unit
/// tests in this module.
pub fn run_suites_explicit(suites: &[(String, PathBuf)]) -> RunReport {
    run_suites_explicit_with_cfg(suites, &SuiteConfig::default())
}

/// M43 — same as [`run_suites_explicit`] but applies `cfg` to each
/// test body. Sequential (single thread): `SuiteConfig` holds
/// `Rc` fields (not `Send`) and the typical demo / project size
/// is small enough that the parallelism loss is negligible. The
/// no-cfg `run_suites_explicit` keeps the parallel path.
pub fn run_suites_explicit_with_cfg(
    suites: &[(String, PathBuf)],
    cfg: &SuiteConfig,
) -> RunReport {
    if suites.is_empty() {
        return RunReport::default();
    }
    let mut report = RunReport::default();
    for (suite, path) in suites {
        match run_one_suite_with_cfg(suite, path, cfg) {
            SuiteResult::Ok(mut outcomes) => report.outcomes.append(&mut outcomes),
            SuiteResult::ParseError { suite, message } => {
                report.parse_failures.push((suite, message));
            }
        }
    }
    report.outcomes.sort_by(|a, b| {
        a.suite
            .cmp(&b.suite)
            .then_with(|| a.name.cmp(&b.name))
    });
    report.parse_failures.sort_by(|a, b| a.0.cmp(&b.0));
    report
}

enum SuiteResult {
    Ok(Vec<TestOutcome>),
    ParseError { suite: String, message: String },
}

fn run_one_suite_with_cfg(
    suite: &str,
    path: &Path,
    cfg: &SuiteConfig,
) -> SuiteResult {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return SuiteResult::ParseError {
                suite: suite.to_string(),
                message: format!("cannot read {}: {e}", path.display()),
            }
        }
    };
    let module = match crate::syntax::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            return SuiteResult::ParseError {
                suite: suite.to_string(),
                message: format!(
                    "{}:{}:{}: {:?}",
                    path.display(),
                    e.span.line,
                    e.span.col,
                    e.kind
                ),
            }
        }
    };
    let fixtures_dir = path.parent().map(|p| p.join("fixtures"));
    SuiteResult::Ok(run_module_tests_in_dir_with_cfg(
        suite,
        &module,
        fixtures_dir.as_deref(),
        cfg,
    ))
}

#[allow(dead_code)]
fn run_one_suite(suite: &str, path: &Path) -> SuiteResult {
    let src = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            return SuiteResult::ParseError {
                suite: suite.to_string(),
                message: format!("cannot read {}: {e}", path.display()),
            }
        }
    };
    let module = match crate::syntax::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            return SuiteResult::ParseError {
                suite: suite.to_string(),
                message: format!(
                    "{}:{}:{}: {:?}",
                    path.display(),
                    e.span.line,
                    e.span.col,
                    e.kind
                ),
            }
        }
    };
    // M12.T3: counter-examples persist alongside the suite under
    // `<dir>/fixtures/`. The conventional `tests/fixtures/` falls out
    // when the suite lives in `tests/`.
    let fixtures_dir = path
        .parent()
        .map(|p| p.join("fixtures"))
        .filter(|p| {
            // Use the dir if it exists or can be created lazily by the
            // property runner. We pass the path either way; the runner
            // creates the directory on first persist.
            let _ = p;
            true
        });
    SuiteResult::Ok(run_module_tests_in_dir(
        suite,
        &module,
        fixtures_dir.as_deref(),
    ))
}

/// Run every `Item::Test(_)` in a parsed module, returning one
/// `TestOutcome` per test. Each test runs against a fresh module env
/// so per-test side effects don't leak (matching the spec's "test
/// capability" isolation, § 21.1).
pub fn run_module_tests(suite: &str, m: &Module) -> Vec<TestOutcome> {
    run_module_tests_in_dir(suite, m, None)
}

/// Variant that pins the directory used to persist / replay property
/// counter-examples (M12.T3). Falls back to `tests/fixtures/` when
/// `None` is passed and the path exists.
pub fn run_module_tests_in_dir(
    suite: &str,
    m: &Module,
    fixtures_dir: Option<&Path>,
) -> Vec<TestOutcome> {
    run_module_tests_in_dir_with_cfg(suite, m, fixtures_dir, &SuiteConfig::default())
}

/// M43 — same as [`run_module_tests_in_dir`] but applies `cfg` to
/// every test body. Each test gets a fresh `TestConfig` cloned
/// from `cfg`.
pub fn run_module_tests_in_dir_with_cfg(
    suite: &str,
    m: &Module,
    fixtures_dir: Option<&Path>,
    cfg: &SuiteConfig,
) -> Vec<TestOutcome> {
    let test_cfg = || crate::runtime::eval::TestConfig {
        cap: cfg.cap.clone(),
        tracer: None,
        ai_backend: cfg.ai_backend.clone(),
        l2_backends: cfg.l2_backends.clone(),
        active_policy_names: cfg.active_policy_names.clone(),
    };
    let mut out: Vec<TestOutcome> = Vec::new();
    for item in &m.items {
        match item {
            Item::Test(t) => {
                let status = match &t.fixture {
                    None => match crate::runtime::eval::run_test_with_cfg(m, t, &test_cfg()) {
                        Ok(()) => TestStatus::Passed,
                        Err(e) => TestStatus::Failed(render_failure(&e)),
                    },
                    Some(id) => match load_fixture_events(fixtures_dir, id) {
                        Ok(events) => match run_test_with_fixture(m, t, events) {
                            Ok(()) => TestStatus::Passed,
                            Err(e) => TestStatus::Failed(render_failure(&e)),
                        },
                        Err(reason) => TestStatus::Failed(format!(
                            "fixture `{id}` could not be loaded: {reason}"
                        )),
                    },
                };
                out.push(TestOutcome {
                    suite: suite.to_string(),
                    name: t.name.clone(),
                    status,
                });
            }
            Item::Property(p) => {
                let seed = property_base_seed(suite, &p.name);
                let outcome =
                    run_property(m, suite, p, fixtures_dir, DEFAULT_CASES, seed);
                let status = match outcome {
                    PropertyOutcome::Passed { cases: _ } => TestStatus::Passed,
                    PropertyOutcome::Failed(f) => TestStatus::Failed(format!(
                        "property failed (seed {}): {}\n  shrunk inputs: {}",
                        f.seed,
                        f.message,
                        render_values(&f.shrunk_values)
                    )),
                    PropertyOutcome::Skipped { reason } => TestStatus::Failed(format!(
                        "property skipped: {reason}"
                    )),
                };
                out.push(TestOutcome {
                    suite: suite.to_string(),
                    name: p.name.clone(),
                    status,
                });
            }
            _ => {}
        }
    }
    out
}

/// M12.T4: load a JSONL fixture trace from `<fixtures_dir>/<id>.jsonl`.
/// `fixtures_dir` defaults to `tests/fixtures/` when `None`.
fn load_fixture_events(
    fixtures_dir: Option<&Path>,
    id: &str,
) -> Result<std::rc::Rc<Vec<crate::runtime::trace::TraceEvent>>, String> {
    let dir = fixtures_dir.map(|p| p.to_path_buf()).unwrap_or_else(|| {
        std::path::PathBuf::from("tests").join("fixtures")
    });
    let path = dir.join(format!("{id}.jsonl"));
    let body = std::fs::read_to_string(&path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let events = crate::runtime::replay::parse_trace_jsonl(&body)
        .map_err(|e| format!("invalid trace at {}: {e}", path.display()))?;
    Ok(std::rc::Rc::new(events))
}

fn property_base_seed(suite: &str, name: &str) -> u64 {
    // FNV-1a over the (suite, name) tuple — stable across runs so a
    // property's sample set is deterministic per its location.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in suite.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h ^= u64::from(b'\0');
    for &b in name.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn render_values(vs: &[crate::runtime::value::Value]) -> String {
    let mut out = String::from("[");
    for (i, v) in vs.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&format!("{v:?}"));
    }
    out.push(']');
    out
}

/// M12.T2 failure renderer. Knows how to surface an
/// `AssertionFailed` with `expected vs. actual` plus the source span;
/// falls back to a generic format for every other evaluator error so
/// the runner is robust to future error variants.
pub(crate) fn render_failure(e: &EvalError) -> String {
    match &e.kind {
        EvalErrorKind::AssertionFailed {
            source,
            detail: Some(d),
        } => {
            let op_str = match d.op {
                AssertionCmpOp::Eq => "==",
                AssertionCmpOp::Ne => "!=",
            };
            format!(
                "line {}, col {}: assertion failed: `{source}`\n  expected: {} {} {}\n  actual:   {} (`{}`) vs {} (`{}`)",
                e.span.line,
                e.span.col,
                d.lhs_source,
                op_str,
                d.rhs_source,
                d.lhs_value,
                d.lhs_source,
                d.rhs_value,
                d.rhs_source,
            )
        }
        EvalErrorKind::AssertionFailed { source, detail: None } => {
            format!(
                "line {}, col {}: assertion failed: `{source}`",
                e.span.line, e.span.col
            )
        }
        other => format!("line {}, col {}: {other:?}", e.span.line, e.span.col),
    }
}

// ====================================================================
//  Tests — M12.T1 acceptance fixtures
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let p = std::env::temp_dir().join(format!("aeris-m12t1-{tag}-{pid}-{id}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn discover_finds_test_files_recursively() {
        let dir = unique_dir("discover");
        let nested = dir.join("nested");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(dir.join("alpha.test.aer"), "").unwrap();
        std::fs::write(nested.join("beta.test.aer"), "").unwrap();
        // Non-test files must be ignored.
        std::fs::write(dir.join("alpha.aer"), "").unwrap();
        std::fs::write(dir.join("README.md"), "").unwrap();
        let suites = discover_suites(&dir);
        let names: Vec<&str> = suites.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_test_body_passes() {
        let m = crate::syntax::parse(r#"test "ok" { }"#).expect("parse");
        let outs = run_module_tests("smoke", &m);
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].status, TestStatus::Passed);
        assert_eq!(outs[0].name, "ok");
    }

    #[test]
    fn body_that_raises_fails() {
        let src = r#"
            test "ok" { }
            test "boom" { raise "fail" }
        "#;
        let m = crate::syntax::parse(src).expect("parse");
        let outs = run_module_tests("smoke", &m);
        assert_eq!(outs.len(), 2);
        let by_name: std::collections::HashMap<&str, &TestStatus> =
            outs.iter().map(|o| (o.name.as_str(), &o.status)).collect();
        assert_eq!(by_name["ok"], &TestStatus::Passed);
        assert!(matches!(by_name["boom"], TestStatus::Failed(_)));
    }

    #[test]
    fn report_exit_code_is_zero_on_all_pass() {
        let dir = unique_dir("exit-zero");
        std::fs::write(
            dir.join("good.test.aer"),
            r#"test "trivial" { let x = 1 + 1 }"#,
        )
        .unwrap();
        let report = run_suites(&dir);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(report.passed(), 1);
        assert_eq!(report.failed(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn report_exit_code_is_one_on_any_failure() {
        let dir = unique_dir("exit-one");
        std::fs::write(
            dir.join("bad.test.aer"),
            r#"
                test "one passes" { }
                test "two raises" { raise "fail" }
            "#,
        )
        .unwrap();
        let report = run_suites(&dir);
        assert_eq!(report.exit_code(), 1);
        assert_eq!(report.passed(), 1);
        // failed() counts the one raising test.
        assert_eq!(report.failed(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parse_failure_in_a_suite_counts_as_failure() {
        let dir = unique_dir("parse-fail");
        std::fs::write(dir.join("broken.test.aer"), "this is not aeris syntax !!").unwrap();
        let report = run_suites(&dir);
        assert_eq!(report.exit_code(), 1);
        assert!(!report.parse_failures.is_empty());
        assert_eq!(report.parse_failures[0].0, "broken");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_directory_yields_zero_exit() {
        // No `.test.aer` files → exit 0 (no tests, no failures).
        let dir = unique_dir("empty");
        let report = run_suites(&dir);
        assert_eq!(report.exit_code(), 0);
        assert_eq!(report.outcomes.len(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- M12.T4: fixture mode (5 saga rollback fixtures) ----

    fn write_fixture_trace(dir: &Path, id: &str, lines: &[&str]) {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{id}.jsonl"));
        let body = lines.join("\n") + "\n";
        std::fs::write(path, body).unwrap();
    }

    fn run_in_dir(suite: &str, src: &str, fixtures_dir: &Path) -> Vec<TestOutcome> {
        let m = crate::syntax::parse(src).expect("parse");
        run_module_tests_in_dir(suite, &m, Some(fixtures_dir))
    }

    /// One canonical line shape produced by the JSONL tracer. The
    /// trace's extra string fields land under a nested `fields` object
    /// (cf. `runtime::trace::TraceEvent::to_jsonl_line`).
    fn evt_line(kind: &str, extras: &[(&str, &str)]) -> String {
        let mut out = String::from(
            r#"{"trace_id":"01TEST","ts":"2026-01-01T00:00:00.000Z""#,
        );
        out.push_str(&format!(",\"kind\":\"{kind}\""));
        if !extras.is_empty() {
            out.push_str(",\"fields\":{");
            for (i, (k, v)) in extras.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("\"{k}\":\"{v}\""));
            }
            out.push('}');
        }
        out.push('}');
        out
    }

    #[test]
    fn m12t4_fixture_mode_test_passes_when_trace_contains_event() {
        let dir = unique_dir("fixt-1");
        let fixt = dir.join("fixtures");
        let line = evt_line(
            "saga_exit",
            &[("saga", "settle"), ("outcome", "rolled_back")],
        );
        write_fixture_trace(&fixt, "rollback_v1", &[line.as_str()]);
        let src = r#"
            test "rollback v1" with fixture: "rollback_v1" {
                assert(trace_has({ kind: "saga_exit", outcome: "rolled_back" }))
            }
        "#;
        let outs = run_in_dir("suite", src, &fixt);
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].status, TestStatus::Passed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m12t4_fixture_mode_test_fails_when_trace_lacks_event() {
        let dir = unique_dir("fixt-2");
        let fixt = dir.join("fixtures");
        let line = evt_line("saga_exit", &[("outcome", "ok")]);
        write_fixture_trace(&fixt, "ok_v1", &[line.as_str()]);
        let src = r#"
            test "expect rollback" with fixture: "ok_v1" {
                assert(trace_has({ kind: "saga_exit", outcome: "rolled_back" }))
            }
        "#;
        let outs = run_in_dir("suite", src, &fixt);
        assert_eq!(outs.len(), 1);
        assert!(matches!(outs[0].status, TestStatus::Failed(_)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m12t4_fixture_mode_test_can_assert_multiple_events() {
        let dir = unique_dir("fixt-3");
        let fixt = dir.join("fixtures");
        write_fixture_trace(
            &fixt,
            "rollback_v2",
            &[
                evt_line("saga_enter", &[("saga", "settle")]).as_str(),
                evt_line("step_enter", &[("name", "charge")]).as_str(),
                evt_line(
                    "saga_exit",
                    &[("saga", "settle"), ("outcome", "rolled_back")],
                )
                .as_str(),
            ],
        );
        let src = r#"
            test "rollback v2" with fixture: "rollback_v2" {
                assert(trace_has({ kind: "saga_enter" }))
                assert(trace_has({ kind: "step_enter", name: "charge" }))
                assert(trace_has({ kind: "saga_exit", outcome: "rolled_back" }))
            }
        "#;
        let outs = run_in_dir("suite", src, &fixt);
        assert_eq!(outs[0].status, TestStatus::Passed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m12t4_fixture_missing_file_surfaces_clean_failure() {
        let dir = unique_dir("fixt-4");
        let fixt = dir.join("fixtures");
        std::fs::create_dir_all(&fixt).unwrap();
        let src = r#"
            test "no file" with fixture: "ghost" {
                assert(true)
            }
        "#;
        let outs = run_in_dir("suite", src, &fixt);
        match &outs[0].status {
            TestStatus::Failed(msg) => {
                assert!(msg.contains("ghost"));
                assert!(msg.contains("could not be loaded"));
            }
            other => panic!("expected fail, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m12t4_fixture_mode_trace_returns_full_events_list() {
        let dir = unique_dir("fixt-5");
        let fixt = dir.join("fixtures");
        write_fixture_trace(
            &fixt,
            "rollback_v3",
            &[
                evt_line("saga_enter", &[("saga", "settle")]).as_str(),
                evt_line(
                    "saga_exit",
                    &[("saga", "settle"), ("outcome", "rolled_back")],
                )
                .as_str(),
            ],
        );
        // `trace()` returns the full list of events. We can't iterate
        // it from .aer directly (no list methods yet), but we can
        // bind it and check it isn't empty via a couple of `trace_has`
        // calls. The point is: `trace()` does not panic and threads
        // events into the env.
        let src = r#"
            test "rollback v3" with fixture: "rollback_v3" {
                let events = trace()
                assert(trace_has({ kind: "saga_enter" }))
                assert(trace_has({ kind: "saga_exit" }))
            }
        "#;
        let outs = run_in_dir("suite", src, &fixt);
        assert_eq!(outs[0].status, TestStatus::Passed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- M12.T2: assert builtin + pretty failure rendering ----

    #[test]
    fn assert_true_passes() {
        let m = crate::syntax::parse(r#"test "ok" { assert(true) }"#).unwrap();
        let outs = run_module_tests("smoke", &m);
        assert_eq!(outs[0].status, TestStatus::Passed);
    }

    #[test]
    fn assert_false_fails_with_source_in_message() {
        let m = crate::syntax::parse(r#"test "boom" { assert(false) }"#).unwrap();
        let outs = run_module_tests("smoke", &m);
        match &outs[0].status {
            TestStatus::Failed(msg) => {
                assert!(msg.contains("assertion failed"));
                assert!(msg.contains("false"));
            }
            other => panic!("expected fail, got {other:?}"),
        }
    }

    #[test]
    fn assert_eq_failure_renders_expected_vs_actual() {
        let m = crate::syntax::parse(
            r#"test "compare" { assert(1 + 1 == 3) }"#,
        )
        .unwrap();
        let outs = run_module_tests("smoke", &m);
        match &outs[0].status {
            TestStatus::Failed(msg) => {
                assert!(msg.contains("expected"));
                assert!(msg.contains("actual"));
                assert!(msg.contains("=="));
                // Both rendered values must appear: 2 (lhs) and 3 (rhs).
                assert!(msg.contains("2"));
                assert!(msg.contains("3"));
            }
            other => panic!("expected fail, got {other:?}"),
        }
    }

    #[test]
    fn assert_ne_failure_renders_detail() {
        let m = crate::syntax::parse(
            r#"test "ne" { assert(1 != 1) }"#,
        )
        .unwrap();
        let outs = run_module_tests("smoke", &m);
        assert!(matches!(outs[0].status, TestStatus::Failed(_)));
        if let TestStatus::Failed(msg) = &outs[0].status {
            assert!(msg.contains("!="));
        }
    }

    #[test]
    fn assert_eq_passes_when_equal() {
        let m = crate::syntax::parse(
            r#"test "eq" { assert(2 + 2 == 4) }"#,
        )
        .unwrap();
        let outs = run_module_tests("smoke", &m);
        assert_eq!(outs[0].status, TestStatus::Passed);
    }

    #[test]
    fn assert_with_non_bool_arg_fails_with_type_error() {
        let m = crate::syntax::parse(r#"test "bad" { assert(42) }"#).unwrap();
        let outs = run_module_tests("smoke", &m);
        match &outs[0].status {
            TestStatus::Failed(msg) => assert!(msg.contains("Type")),
            other => panic!("expected type error, got {other:?}"),
        }
    }

    #[test]
    fn parallel_run_across_many_suites_aggregates_results() {
        // 8 suites, 2 tests each; one suite contains a raising test.
        let dir = unique_dir("parallel");
        for i in 0..8 {
            let body = if i == 3 {
                r#"
                    test "a" { }
                    test "b" { raise "x" }
                "#
            } else {
                r#"
                    test "a" { }
                    test "b" { let x = 1 }
                "#
            };
            std::fs::write(dir.join(format!("s{i}.test.aer")), body).unwrap();
        }
        let report = run_suites(&dir);
        // 16 tests total; 1 failure → exit 1.
        assert_eq!(report.outcomes.len(), 16);
        assert_eq!(report.exit_code(), 1);
        let failed: Vec<&TestOutcome> = report
            .outcomes
            .iter()
            .filter(|o| matches!(o.status, TestStatus::Failed(_)))
            .collect();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].suite, "s3");
        assert_eq!(failed[0].name, "b");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
