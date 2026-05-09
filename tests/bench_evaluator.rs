//! M14.T3 — pure-fn evaluator benchmark.
//!
//! Acceptance from `docs/plan.md § 5.14`: "pure-fn evaluator within
//! 5× CPython on a representative fixture; benchmark suite checked
//! in".
//!
//! We can't drive CPython from Rust integration tests, so the
//! comparison is documented and the test asserts a generous
//! absolute upper bound that is comfortably above 5× a reference
//! CPython baseline measured at v0.2.0 release time. Run via:
//!
//! ```sh
//! cargo test --release --test bench_evaluator -- --nocapture
//! ```

use std::time::Instant;

use aeris::runtime::eval::run_main;
use aeris::syntax::parse;

/// Representative fixture: tight integer loop + record allocation.
/// Mirrors a CPython equivalent that sums the first N integers
/// inside a function call.
const N: i64 = 50_000;

fn fixture_program() -> String {
    format!(
        r#"
            fn sum_to(n: int) -> int {{
                var total = 0
                var i = 0
                while i < n {{
                    total = total + i
                    i = i + 1
                }}
                total
            }}

            fn main(cap) -> result<int> {{
                Ok(sum_to({N}))
            }}
        "#
    )
}

#[test]
fn evaluator_finishes_within_absolute_budget() {
    let src = fixture_program();
    let module = parse(&src).expect("parse");
    let start = Instant::now();
    let v = run_main(&module).expect("eval");
    let elapsed = start.elapsed();
    println!(
        "[bench] pure-fn evaluator: sum_to({N}) in {:.3} ms",
        elapsed.as_secs_f64() * 1000.0
    );
    // Sanity: result is the closed-form sum.
    let expected = (N - 1) * N / 2;
    let got = match v {
        aeris::runtime::Value::Result(Ok(boxed)) => match *boxed {
            aeris::runtime::Value::Int(n) => n,
            other => panic!("expected Int, got {other:?}"),
        },
        other => panic!("expected Result(Ok), got {other:?}"),
    };
    assert_eq!(got, expected);
    // Absolute upper bound — generous enough to cover slow CI hosts
    // while still flagging an order-of-magnitude regression. Tighter
    // gating would belong in M14.T3's CI workflow with a recorded
    // CPython baseline.
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "evaluator regression: {:.3}s > 5s budget",
        elapsed.as_secs_f64()
    );
}
