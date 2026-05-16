//! Release acceptance: the five mechanically-verifiable criteria of
//! `docs/thesis.md` § 13. The sixth criterion ("compliance officer
//! reads a saga signature in under 30 seconds") is a manual review
//! and lives in `RELEASE.md`.
//!
//! Each test is self-contained and runs against the same public APIs
//! a downstream user would call. The intent is that a release engineer
//! can run `cargo test --test release_thesis_section_13` and read a
//! green suite as evidence the language keeps its promises.

use std::path::Path;

use aeris::check::{check_module, check_module_with_manifest};
use aeris::manifest::{parse_manifest, surface, verify_local_deps};
use aeris::runtime::eval::run_main_with;
use aeris::runtime::Tracer;
use aeris::syntax::parse;

fn load_golden(rel: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing golden {}: {e}", path.display()))
        .lines()
        .map(String::from)
        .collect()
}

fn run_saga_collect_kinds(src: &str) -> Vec<String> {
    let m = parse(src).expect("parse");
    let tracer = Tracer::in_memory();
    let _ = run_main_with(&m, Some(tracer.clone()));
    tracer.events().iter().map(|e| e.kind.clone()).collect()
}

/// Criterion 2 — every effectful call site sits inside an `intent`.
/// V2 is enforced statically by `check_module`; a program with an
/// out-of-intent `http.post` must be rejected with exit 66
/// (`language.md` § 10.1).
#[test]
fn criterion_2_v2_rejects_effectful_call_outside_intent() {
    let bad = r#"
        fn settle(cap: cap[http.post]) {
            http.post("https://api.acme.com/x", "\{\}")
        }
    "#;
    let m = parse(bad).expect("parse");
    let errs = check_module(&m);
    assert!(
        errs.iter().any(|e| e.exit_code() == 66),
        "expected V2 violation (exit 66), got {errs:?}"
    );
    // The dual: the same code under `intent { ... }` must pass.
    let good = r#"
        fn settle(cap: cap[http.post]) {
            intent "settle" {
                http.post("https://api.acme.com/x", "\{\}")
            }
        }
    "#;
    let m = parse(good).expect("parse");
    let errs = check_module(&m);
    assert!(
        !errs.iter().any(|e| e.exit_code() == 66),
        "in-intent call should not raise V2: {errs:?}"
    );
}

/// Criterion 3 — for the deterministic subset, the in-memory tracer's
/// event-kind sequence is stable across re-runs of the same source.
/// This is the foundation of bit-identical replay (`aeris replay`
/// guarantees more — clock / random pin-down — but the kind sequence
/// is the surface symptom of determinism).
#[test]
fn criterion_3_deterministic_subset_produces_stable_kind_sequence() {
    let src = r#"
        fn main(cap: cap[]) -> result<unit> {
            Ok(())
        }
    "#;
    let kinds_a = run_saga_collect_kinds(src);
    let kinds_b = run_saga_collect_kinds(src);
    assert_eq!(
        kinds_a, kinds_b,
        "deterministic subset diverged across two runs: {kinds_a:?} vs {kinds_b:?}"
    );
}

/// Criterion 4 — a saga always lands in `ok` / `rolled_back` /
/// `PartialFailure`, never an undefined half-state. We assert the
/// three golden traces shipped by M6 carry the expected marker each.
#[test]
fn criterion_4_saga_outcomes_are_one_of_three_clean_states() {
    let success = load_golden("aeris-tests/golden/m6/saga_success.jsonl");
    let rollback = load_golden("aeris-tests/golden/m6/saga_rollback.jsonl");
    let partial = load_golden("aeris-tests/golden/m6/saga_partial_failure.jsonl");

    assert!(success.iter().any(|k| k == "saga_exit"));
    assert!(!success.iter().any(|k| k == "rollback_enter"));

    assert!(rollback.iter().any(|k| k == "rollback_enter"));
    assert!(rollback.iter().any(|k| k == "saga_exit"));
    assert!(!rollback.iter().any(|k| k == "partial_failure"));

    assert!(partial.iter().any(|k| k == "partial_failure"));
}

/// Criterion 5 — a byte-swap of a pinned dep blocks execution. We
/// stage a local-path dep, pin its current hash, mutate the file, and
/// confirm `verify_local_deps` returns an error pointing at the
/// hash mismatch.
#[test]
fn criterion_5_manifest_byte_swap_blocks_execution() {
    let id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "aeris-release-c5-{}-{id}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();

    let dep_path = dir.join("dep.aer");
    let original = "pub fn f() -> int { 1 }\n";
    std::fs::write(&dep_path, original).unwrap();
    let pinned = surface::hash_text(original);

    let manifest_body = format!(
        r#"
            [project]
            name  = "release-c5"
            aeris = "0.2.0"

            [deps]
            d = {{ path = "./dep.aer", hash = "{pinned}" }}
        "#
    );
    let manifest_path = dir.join("aeris.toml");
    std::fs::write(&manifest_path, &manifest_body).unwrap();
    let manifest = parse_manifest(&manifest_body).expect("manifest parses");

    // First call: hash matches → no error.
    verify_local_deps(&manifest, &dir).expect("clean manifest must verify");

    // Byte-swap the dep, keep the hash pinned: the byte-swap must
    // surface as a `deps.d: hash mismatch` error.
    std::fs::write(&dep_path, "pub fn f() -> int { 99 }\n").unwrap();
    let err = verify_local_deps(&manifest, &dir)
        .expect_err("byte-swap must trigger a hash mismatch");
    let combined: String = err
        .iter()
        .map(|e| format!("{e}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        combined.contains("hash mismatch"),
        "expected hash mismatch error, got: {combined}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Criterion 6 — surface drift surfaces as the first hunk on `aeris
/// check`. We invoke the same helpers the CLI driver uses and confirm
/// the rendered diff starts with the unified-diff header and contains
/// the newly-introduced effect site.
#[test]
fn criterion_6_surface_diff_is_first_hunk_when_committed_lock_is_stale() {
    let computed = surface::render_surface_lock(
        &surface::compute_surface(&[(
            "src/main.aer".to_string(),
            "pub fn settle(cap: cap[http.post]) { intent \"x\" { http.post(\"u\", \"\\{\\}\") } }"
                .to_string(),
        )])
        .unwrap(),
    );
    // Committed body is empty (lock not yet generated) → computed
    // body is non-empty → diff non-empty and unified-diff shaped.
    let diff = aeris::manifest::diff_surface_bodies("", &computed);
    assert!(!diff.is_empty(), "expected drift diff, got empty");
    assert!(
        diff.starts_with("--- "),
        "expected diff to open with `---`, got:\n{diff}"
    );
    assert!(diff.contains("+++ "));
    assert!(diff.contains("settle"));
}

/// Sanity guard: the manifest-aware check path agrees with the bare
/// `check_module` on the in-intent fixture. This protects criterion 2
/// from drifting if the `[caps] required` flag changes meaning.
#[test]
fn criterion_2_manifest_aware_path_agrees_on_in_intent_pattern() {
    let src = r#"
        fn settle(cap: cap[http.post]) {
            intent "x" { http.post("u", "\{\}") }
        }
    "#;
    let m = parse(src).expect("parse");
    let manifest_src = r#"
        [project]
        name  = "release-c2"
        aeris = "0.2.0"
    "#;
    let manifest = parse_manifest(manifest_src).expect("manifest parses");
    let errs = check_module_with_manifest(&m, &manifest.caps);
    assert!(
        !errs.iter().any(|e| e.exit_code() == 66),
        "in-intent pattern must not trigger V2 under manifest, got {errs:?}"
    );
}
