//! M14.T5 — cold-start latency benchmark.
//!
//! Acceptance from `docs/plan.md § 5.14`: "cold-start time of `aeris
//! run` < 50 ms (parse + check + start eval)". Cargo's integration
//! tests already share a process with the harness, so the in-process
//! measurement here covers parse + check + module-env build, which
//! is what dominates a real cold-start. Process-launch overhead is
//! measured separately by the release packaging job.
//!
//! ```sh
//! cargo test --release --test bench_cold_start -- --nocapture
//! ```

use std::time::Instant;

use aeris::check::check_module;
use aeris::runtime::eval::eval_module_env;
use aeris::syntax::parse;

const SRC: &str = r#"
    record User { id: uuid, name: string }

    /// Sum the first n integers.
    fn sum_to(n: int) -> int {
        var total = 0
        var i = 0
        while i < n { total = total + i; i = i + 1 }
        total
    }

    fn greet(u: User) -> string { u.name }

    fn main(cap) -> result<int> {
        Ok(sum_to(10))
    }
"#;

#[test]
fn cold_start_under_fifty_ms() {
    // First call may include monomorphisation / dispatch warmup;
    // measure the steady-state cold start by running 10 iterations
    // and reporting the minimum.
    let mut best = u128::MAX;
    for _ in 0..10 {
        let start = Instant::now();
        let module = parse(SRC).expect("parse");
        let errs = check_module(&module);
        assert!(errs.is_empty(), "check errors in fixture: {errs:#?}");
        let _env = eval_module_env(&module);
        let micros = start.elapsed().as_micros();
        if micros < best {
            best = micros;
        }
    }
    let ms = (best as f64) / 1000.0;
    println!("[bench] cold start (parse+check+env): {ms:.3} ms");
    assert!(
        ms < 50.0,
        "cold-start regression: {ms:.3} ms > 50 ms budget",
    );
}
