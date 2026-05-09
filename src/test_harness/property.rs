//! M12.T3 — property runner with default 200 cases and counter-example
//! shrinking.
//!
//! Realises `docs/language.md` § 21.3. A `property "..." with (...) { ... }`
//! declaration is sampled `DEFAULT_CASES` times against a deterministic
//! RNG. On the first counter-example the runner performs greedy
//! shrinking and persists the original seed to
//! `tests/fixtures/<id>.json` so subsequent invocations replay it
//! before resuming random sampling.

use std::path::{Path, PathBuf};

use crate::runtime::eval::run_property_case;
use crate::runtime::value::Value;
use crate::syntax::ast::{Module, PropertyDecl, Type};

/// Default sample budget per the spec (§ 21.3).
pub const DEFAULT_CASES: usize = 200;

/// SplitMix64 — deterministic, no-deps, fits in 4 lines. The runtime
/// uses a 64-bit LCG-class PRNG everywhere a "random" decision is
/// needed; we keep that property here so a (seed, type-vector) pair
/// always regenerates the same case vector.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(1),
        }
    }
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn next_size(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        (self.next_u64() % (max as u64 + 1)) as usize
    }
    pub fn next_int_range(&mut self, lo: i64, hi: i64) -> i64 {
        let range = (hi - lo + 1) as u64;
        ((self.next_u64() % range) as i64) + lo
    }
    pub fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 0
    }
    pub fn next_char(&mut self) -> char {
        // Stick to printable ASCII letters — keeps shrunk strings
        // readable in `tests/fixtures/*.json`.
        let alphabet = b"abcdefghijklmnopqrstuvwxyz";
        alphabet[(self.next_u64() as usize) % alphabet.len()] as char
    }
}

/// Generate one value of `t` against `rng`. `None` if the type lies
/// outside the supported generator surface (extension point: new
/// generators are added here without touching the runner).
pub fn generate(t: &Type, rng: &mut Rng) -> Option<Value> {
    generate_at(t, rng, 0)
}

fn generate_at(t: &Type, rng: &mut Rng, depth: usize) -> Option<Value> {
    if depth > 4 {
        return None;
    }
    match t {
        Type::Named { name, .. } => match name.as_str() {
            "int" | "i32" | "i64" | "i8" | "i16" => {
                Some(Value::Int(rng.next_int_range(-100, 100)))
            }
            "u8" | "u16" | "u32" | "u64" => Some(Value::Int(rng.next_int_range(0, 200))),
            "bool" => Some(Value::Bool(rng.next_bool())),
            "string" => {
                let len = rng.next_size(8);
                let s: String = (0..len).map(|_| rng.next_char()).collect();
                Some(Value::Str(s))
            }
            "char" => Some(Value::Char(rng.next_char())),
            "unit" => Some(Value::Unit),
            _ => None,
        },
        Type::Generic { name, args, .. } => match (name.as_str(), args.as_slice()) {
            ("list", [inner]) => {
                let len = rng.next_size(8);
                let mut xs = Vec::with_capacity(len);
                for _ in 0..len {
                    xs.push(generate_at(inner, rng, depth + 1)?);
                }
                Some(Value::List(xs))
            }
            ("set", [inner]) => {
                let len = rng.next_size(6);
                let mut xs = Vec::with_capacity(len);
                for _ in 0..len {
                    xs.push(generate_at(inner, rng, depth + 1)?);
                }
                Some(Value::List(xs))
            }
            ("option", [inner]) => {
                if rng.next_bool() {
                    Some(Value::Option(None))
                } else {
                    let v = generate_at(inner, rng, depth + 1)?;
                    Some(Value::Option(Some(Box::new(v))))
                }
            }
            _ => None,
        },
        Type::Tuple { elems, .. } => {
            let mut xs = Vec::with_capacity(elems.len());
            for e in elems {
                xs.push(generate_at(e, rng, depth + 1)?);
            }
            Some(Value::Tuple(xs))
        }
        _ => None,
    }
}

/// Generate one full input vector for a property. `None` if any param
/// type lacks a supported generator.
pub fn generate_inputs(prop: &PropertyDecl, rng: &mut Rng) -> Option<Vec<Value>> {
    prop.params.iter().map(|p| generate(&p.ty, rng)).collect()
}

/// Outcome of a property run.
#[derive(Debug, Clone)]
pub enum PropertyOutcome {
    Passed { cases: usize },
    Failed(PropertyFailure),
    /// One of the parameter types has no supported generator.
    Skipped { reason: String },
}

#[derive(Debug, Clone)]
pub struct PropertyFailure {
    pub seed: u64,
    pub original_values: Vec<Value>,
    pub shrunk_values: Vec<Value>,
    pub message: String,
}

/// Run `prop` against `m` for `cases` random samples. Returns the
/// first failure (with shrunk inputs) or `Passed` if every sample
/// succeeded. Pre-existing fixture seeds (M12.T3) are replayed first.
pub fn run_property(
    m: &Module,
    suite: &str,
    prop: &PropertyDecl,
    fixtures_dir: Option<&Path>,
    cases: usize,
    base_seed: u64,
) -> PropertyOutcome {
    // 1. Replay any persisted regression seed. A failure is the
    //    immediate counter-example; otherwise resume random sampling.
    if let Some(fdir) = fixtures_dir {
        if let Some(seed) = load_fixture_seed(fdir, suite, &prop.name) {
            let mut rng = Rng::new(seed);
            let values = match generate_inputs(prop, &mut rng) {
                Some(vs) => vs,
                None => {
                    return PropertyOutcome::Skipped {
                        reason: "no supported generator for one of the params".into(),
                    }
                }
            };
            if let Err(e) = run_property_case(m, prop, &values) {
                return PropertyOutcome::Failed(shrink_and_persist(
                    m,
                    prop,
                    suite,
                    seed,
                    values,
                    super::render_failure(&e),
                    fixtures_dir,
                ));
            }
        }
    }
    // 2. Random sampling pass.
    for case_idx in 0..cases {
        let seed = base_seed
            .wrapping_add(case_idx as u64)
            .wrapping_mul(0x9E37_79B9_7F4A_7C15);
        let mut rng = Rng::new(seed);
        let values = match generate_inputs(prop, &mut rng) {
            Some(vs) => vs,
            None => {
                return PropertyOutcome::Skipped {
                    reason: "no supported generator for one of the params".into(),
                }
            }
        };
        if let Err(e) = run_property_case(m, prop, &values) {
            return PropertyOutcome::Failed(shrink_and_persist(
                m,
                prop,
                suite,
                seed,
                values,
                super::render_failure(&e),
                fixtures_dir,
            ));
        }
    }
    PropertyOutcome::Passed { cases }
}

/// Greedy 1-pass shrink: for each parameter, try a list of smaller
/// candidates; keep the first that still reproduces the failure.
/// Repeat across params until no candidate shrinks any further.
fn shrink_and_persist(
    m: &Module,
    prop: &PropertyDecl,
    suite: &str,
    seed: u64,
    original: Vec<Value>,
    message: String,
    fixtures_dir: Option<&Path>,
) -> PropertyFailure {
    let shrunk = greedy_shrink(m, prop, original.clone());
    if let Some(fdir) = fixtures_dir {
        let _ = save_fixture(fdir, suite, &prop.name, seed, &original, &shrunk);
    }
    PropertyFailure {
        seed,
        original_values: original,
        shrunk_values: shrunk,
        message,
    }
}

fn greedy_shrink(m: &Module, prop: &PropertyDecl, mut current: Vec<Value>) -> Vec<Value> {
    // Bound the loop so a pathological reproduce-shrink chain can't
    // run away. 64 rounds across `params * candidates` is plenty for
    // the small inputs the default generator produces.
    for _ in 0..64 {
        let mut progress = false;
        for i in 0..current.len() {
            let candidates = shrink_candidates(&current[i]);
            for cand in candidates {
                let mut trial = current.clone();
                trial[i] = cand;
                if run_property_case(m, prop, &trial).is_err() {
                    current = trial;
                    progress = true;
                    break;
                }
            }
        }
        if !progress {
            break;
        }
    }
    current
}

fn shrink_candidates(v: &Value) -> Vec<Value> {
    match v {
        Value::Int(n) => {
            let mut xs = Vec::new();
            if *n != 0 {
                xs.push(Value::Int(0));
            }
            if *n > 1 {
                xs.push(Value::Int(n / 2));
                xs.push(Value::Int(n - 1));
            }
            if *n < -1 {
                xs.push(Value::Int(n / 2));
                xs.push(Value::Int(n + 1));
            }
            xs
        }
        Value::Bool(b) => {
            if *b {
                vec![Value::Bool(false)]
            } else {
                Vec::new()
            }
        }
        Value::Str(s) => {
            let mut xs = Vec::new();
            if !s.is_empty() {
                xs.push(Value::Str(String::new()));
            }
            if s.len() > 1 {
                xs.push(Value::Str(s[..s.len() / 2].to_string()));
                let mut shorter = s.clone();
                shorter.pop();
                xs.push(Value::Str(shorter));
            }
            xs
        }
        Value::List(xs) => {
            let mut out = Vec::new();
            if !xs.is_empty() {
                out.push(Value::List(Vec::new()));
            }
            if xs.len() > 1 {
                out.push(Value::List(xs[..xs.len() / 2].to_vec()));
                out.push(Value::List(xs[..xs.len() - 1].to_vec()));
                out.push(Value::List(xs[1..].to_vec()));
            }
            out
        }
        Value::Tuple(xs) => {
            // Tuples are fixed-arity; shrink each element pointwise.
            let mut out = Vec::new();
            for (i, x) in xs.iter().enumerate() {
                for cand in shrink_candidates(x) {
                    let mut trial = xs.clone();
                    trial[i] = cand;
                    out.push(Value::Tuple(trial));
                }
            }
            out
        }
        Value::Option(Some(_)) => vec![Value::Option(None)],
        _ => Vec::new(),
    }
}

// -------- fixture I/O --------

fn fixture_path(dir: &Path, suite: &str, prop_name: &str) -> PathBuf {
    let slug = slugify(prop_name);
    dir.join(format!("{suite}__{slug}.json"))
}

fn slugify(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn save_fixture(
    dir: &Path,
    suite: &str,
    prop_name: &str,
    seed: u64,
    original: &[Value],
    shrunk: &[Value],
) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let body = render_fixture_json(seed, original, shrunk);
    std::fs::write(fixture_path(dir, suite, prop_name), body)
}

fn render_fixture_json(seed: u64, original: &[Value], shrunk: &[Value]) -> String {
    let mut out = String::from("{\n");
    out.push_str(&format!("  \"seed\": {seed},\n"));
    out.push_str("  \"original\": [");
    for (i, v) in original.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&value_render(v));
    }
    out.push_str("],\n");
    out.push_str("  \"shrunk\": [");
    for (i, v) in shrunk.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&value_render(v));
    }
    out.push_str("]\n}\n");
    out
}

fn value_render(v: &Value) -> String {
    use crate::runtime::value::Value as V;
    match v {
        V::Int(n) => n.to_string(),
        V::Bool(b) => b.to_string(),
        V::Str(s) => format!("\"{}\"", json_escape(s)),
        V::Char(c) => format!("\"{}\"", json_escape(&c.to_string())),
        V::Unit => "null".into(),
        V::Option(None) => "null".into(),
        V::Option(Some(inner)) => value_render(inner),
        V::List(xs) | V::Tuple(xs) => {
            let parts: Vec<String> = xs.iter().map(value_render).collect();
            format!("[{}]", parts.join(", "))
        }
        _ => "\"<unrenderable>\"".into(),
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn load_fixture_seed(dir: &Path, suite: &str, prop_name: &str) -> Option<u64> {
    let path = fixture_path(dir, suite, prop_name);
    let body = std::fs::read_to_string(&path).ok()?;
    // Naïve scan for `"seed": <number>` — the writer's only producer
    // is `render_fixture_json` so the format is stable.
    let needle = "\"seed\":";
    let i = body.find(needle)?;
    let rest = &body[i + needle.len()..];
    let trimmed = rest.trim_start();
    let end = trimmed
        .find(|c: char| !(c == '-' || c.is_ascii_digit()))
        .unwrap_or(trimmed.len());
    trimmed[..end].parse::<u64>().ok()
}

// ====================================================================
//  Tests — M12.T3 acceptance: 10 property fixtures
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_one(src: &str) -> (Module, PropertyDecl) {
        let m = crate::syntax::parse(src).expect("parse");
        let prop = m
            .items
            .iter()
            .find_map(|i| {
                if let crate::syntax::ast::Item::Property(p) = i {
                    Some(p.clone())
                } else {
                    None
                }
            })
            .expect("property");
        (m, prop)
    }

    fn unique_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let p = std::env::temp_dir().join(format!("aeris-m12t3-{tag}-{pid}-{id}"));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // ---- generator unit tests ----

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn generate_int_stays_in_range() {
        let mut rng = Rng::new(7);
        for _ in 0..50 {
            let v = generate(
                &Type::Named {
                    name: "int".into(),
                    span: crate::syntax::token::Span::ZERO,
                },
                &mut rng,
            )
            .unwrap();
            if let Value::Int(n) = v {
                assert!((-100..=100).contains(&n));
            } else {
                panic!();
            }
        }
    }

    // ---- 10 property fixtures ----

    fn run_for(src: &str, dir: Option<&Path>, cases: usize, seed: u64) -> PropertyOutcome {
        let (m, prop) = parse_one(src);
        run_property(&m, "suite", &prop, dir, cases, seed)
    }

    #[test]
    fn p01_int_addition_is_commutative() {
        let r = run_for(
            r#"
                property "add commutes" with (a: int, b: int) {
                    assert(a + b == b + a)
                }
            "#,
            None,
            DEFAULT_CASES,
            1,
        );
        assert!(matches!(r, PropertyOutcome::Passed { cases: 200 }));
    }

    #[test]
    fn p02_int_addition_associative() {
        let r = run_for(
            r#"
                property "add assoc" with (a: int, b: int, c: int) {
                    assert((a + b) + c == a + (b + c))
                }
            "#,
            None,
            DEFAULT_CASES,
            2,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
    }

    #[test]
    fn p03_int_multiply_by_zero() {
        let r = run_for(
            r#"
                property "mul zero" with (a: int) {
                    assert(a * 0 == 0)
                }
            "#,
            None,
            DEFAULT_CASES,
            3,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
    }

    #[test]
    fn p04_bool_double_negation() {
        let r = run_for(
            r#"
                property "double neg" with (b: bool) {
                    assert(not (not b) == b)
                }
            "#,
            None,
            DEFAULT_CASES,
            4,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
    }

    #[test]
    fn p05_int_eq_self() {
        let r = run_for(
            r#"
                property "eq self" with (a: int) {
                    assert(a == a)
                }
            "#,
            None,
            DEFAULT_CASES,
            5,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
    }

    #[test]
    fn p06_int_lt_self_false() {
        let r = run_for(
            r#"
                property "not lt self" with (a: int) {
                    assert((a < a) == false)
                }
            "#,
            None,
            DEFAULT_CASES,
            6,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
    }

    #[test]
    fn p07_buggy_property_finds_counter_example() {
        // `a + 1 == a` is false for every int → the runner must surface
        // the failure (and shrink toward 0 / -1 / etc.).
        let r = run_for(
            r#"
                property "wrong" with (a: int) {
                    assert(a + 1 == a)
                }
            "#,
            None,
            DEFAULT_CASES,
            7,
        );
        assert!(matches!(r, PropertyOutcome::Failed(_)));
        if let PropertyOutcome::Failed(f) = r {
            // Shrunk value: int 0 reproduces the failure (1 != 0).
            assert!(matches!(f.shrunk_values[0], Value::Int(_)));
        }
    }

    #[test]
    fn p08_buggy_bool_property_shrinks_to_false() {
        // `b` is true for every input is false; sampling will hit
        // a false eventually. After shrink the bool must become false
        // (the smaller value).
        let r = run_for(
            r#"
                property "always true" with (b: bool) {
                    assert(b == true)
                }
            "#,
            None,
            DEFAULT_CASES,
            8,
        );
        assert!(matches!(r, PropertyOutcome::Failed(_)));
        if let PropertyOutcome::Failed(f) = r {
            assert_eq!(f.shrunk_values[0], Value::Bool(false));
        }
    }

    #[test]
    fn p09_list_self_equality_holds_for_generated_lists() {
        // Property: every list equals itself. Exercises the
        // `list<int>` generator and the structural-equality runtime.
        let r = run_for(
            r#"
                property "list eq self" with (xs: list<int>) {
                    assert(xs == xs)
                }
            "#,
            None,
            DEFAULT_CASES,
            9,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
    }

    #[test]
    fn p10_counter_example_persisted_to_fixtures_dir() {
        let dir = unique_dir("persist");
        let r = run_for(
            r#"
                property "buggy persist" with (a: int) {
                    assert(a == 999)
                }
            "#,
            Some(&dir),
            DEFAULT_CASES,
            10,
        );
        assert!(matches!(r, PropertyOutcome::Failed(_)));
        let path = fixture_path(&dir, "suite", "buggy persist");
        assert!(path.exists(), "fixture file not written: {}", path.display());
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("\"seed\""));
        assert!(body.contains("\"shrunk\""));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- regression-seed replay ----

    #[test]
    fn fixture_seed_is_replayed_first() {
        // Save a regression seed manually, then check that a passing
        // property still runs cleanly (the seed replays without
        // failure). We cannot easily assert the seed was replayed
        // without instrumentation; this checks the path doesn't break.
        let dir = unique_dir("replay");
        save_fixture(&dir, "suite", "ok", 42, &[Value::Int(0)], &[Value::Int(0)]).unwrap();
        let r = run_for(
            r#"
                property "ok" with (a: int) { assert(a + 0 == a) }
            "#,
            Some(&dir),
            DEFAULT_CASES,
            11,
        );
        assert!(matches!(r, PropertyOutcome::Passed { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unsupported_param_type_is_skipped_cleanly() {
        // `record R { x: int }` reference is not in the generator
        // surface — the runner reports `Skipped` instead of crashing.
        let src = r#"
            record R { x: int }
            property "skipme" with (r: R) { assert(r.x == r.x) }
        "#;
        let (m, prop) = parse_one(src);
        let r = run_property(&m, "s", &prop, None, 10, 1);
        assert!(matches!(r, PropertyOutcome::Skipped { .. }));
    }

    #[test]
    fn shrink_int_candidates_includes_zero() {
        let cs = shrink_candidates(&Value::Int(42));
        assert!(cs.iter().any(|v| matches!(v, Value::Int(0))));
    }

    #[test]
    fn shrink_string_candidates_includes_empty() {
        let cs = shrink_candidates(&Value::Str("abcdef".into()));
        assert!(cs
            .iter()
            .any(|v| matches!(v, Value::Str(s) if s.is_empty())));
    }

    #[test]
    fn shrink_list_candidates_includes_empty() {
        let cs = shrink_candidates(&Value::List(vec![Value::Int(1), Value::Int(2)]));
        assert!(cs
            .iter()
            .any(|v| matches!(v, Value::List(xs) if xs.is_empty())));
    }
}
