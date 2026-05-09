//! M14.T4 — JSONL trace throughput benchmark.
//!
//! Acceptance from `docs/plan.md § 5.14`: "≥ 100 k events/sec on a
//! representative SSD". Run with:
//!
//! ```sh
//! cargo test --release --test bench_trace -- --nocapture
//! ```

use std::time::Instant;

use aeris::runtime::trace::TraceEvent;

const N: usize = 200_000;

fn make_event(i: usize) -> TraceEvent {
    TraceEvent {
        trace_id: "01TEST00000000000000000000".into(),
        ts: "2026-01-01T00:00:00.000Z".into(),
        kind: "io_println".into(),
        intent: Some("bench".into()),
        scope: Some("main".into()),
        fields: vec![
            ("i".into(), i.to_string()),
            ("len".into(), "12".into()),
            ("hash".into(), "\"deadbeef\"".into()),
        ],
    }
}

#[test]
fn jsonl_serialisation_throughput_above_100k_events_per_second() {
    let events: Vec<TraceEvent> = (0..N).map(make_event).collect();
    let start = Instant::now();
    let mut total_bytes: usize = 0;
    for e in &events {
        let line = e.to_jsonl_line();
        total_bytes += line.len();
    }
    let elapsed = start.elapsed();
    let secs = elapsed.as_secs_f64();
    let throughput = (N as f64) / secs;
    let mib = (total_bytes as f64) / (1024.0 * 1024.0);
    println!(
        "[bench] trace JSONL: {N} events in {:.3}s = {throughput:.0} ev/s ({:.2} MiB)",
        secs, mib
    );
    assert!(
        throughput >= 100_000.0,
        "trace JSONL throughput regression: {throughput:.0} ev/s < 100_000",
    );
}
