//! JSONL trace channel (M4.T1).
//!
//! Realises `docs/language.md` § 20 — every Aeris run emits a stream
//! of self-contained JSON events, one per line, into
//! `.aeris/traces/<trace_id>.jsonl`. The recorder is **always-on**
//! (thesis § 8.5 / § 20.2): there is no opt-out switch in production
//! builds. Tests use the in-memory sink to assert event shape
//! without touching the filesystem.
//!
//! The tracer also owns the active **intent stack** and **scope
//! stack** so that every event emitted between `intent_enter` and
//! `intent_exit` automatically carries the active intent string and
//! the current scope path (`saga.step`, `function`, `net.agent`).
//!
//! ULID format (§ 20.1): 26 Crockford-base32 characters. The first
//! 10 characters encode 48 bits of milliseconds since the Unix
//! epoch; the last 16 characters carry 80 bits of randomness. The
//! `Tracer` ensures monotonic IDs within a single run by bumping a
//! per-instance counter when consecutive events fall in the same
//! millisecond.

use std::cell::RefCell;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

// ====================================================================
//  ULID generation
// ====================================================================

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Encode 128 bits as 26-char Crockford base32 (most-significant
/// first). The leading 48 bits of timestamp land in the first 10
/// chars (with 2 padding zero bits in the topmost char), the last 80
/// bits of randomness land in the remaining 16.
fn encode_ulid(ts_ms: u64, rand_high: u64, rand_low: u16) -> String {
    let mut out = [0u8; 26];
    // 10 chars × 5 bits = 50 bits hold the 48-bit timestamp plus 2
    // leading zero pad bits. Char `i` (0-indexed) reads the bits at
    // shift `45 - 5*i` of `ts`.
    let ts = ts_ms & ((1u64 << 48) - 1);
    for (i, slot) in out.iter_mut().enumerate().take(10) {
        let shift = 45_i32 - 5 * i as i32;
        let v = if !(0..64).contains(&shift) {
            0
        } else {
            ((ts >> shift) & 0x1f) as usize
        };
        *slot = CROCKFORD[v];
    }
    // 16 chars × 5 bits = 80 bits hold the random tail; char `j`
    // reads bits at shift `75 - 5*j` of `rand`.
    let rand: u128 = (u128::from(rand_high) << 16) | u128::from(rand_low);
    for (j, slot) in out.iter_mut().enumerate().skip(10).take(16) {
        let j = j - 10;
        let shift = 75_i32 - 5 * j as i32;
        let v = if shift < 0 {
            0
        } else {
            ((rand >> shift) & 0x1f) as usize
        };
        *slot = CROCKFORD[v];
    }
    String::from_utf8(out.to_vec()).unwrap()
}

/// Best-effort entropy without an external `rand` crate. Mixes the
/// process-time nanos with a per-call counter into a SplitMix64
/// pipeline. Sufficient for trace-id uniqueness within a single run;
/// adversarial uses must wait for the proper RNG in M4.T7.
fn cheap_random_64(counter: u64) -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let mut x = now ^ counter.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

// ====================================================================
//  Trace event
// ====================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TraceEvent {
    pub trace_id: String,
    pub ts: String,
    pub kind: String,
    pub intent: Option<String>,
    pub scope: Option<String>,
    pub fields: Vec<(String, String)>,
}

impl TraceEvent {
    /// Render the event as one JSON line, terminated with `\n`. No
    /// dependence on `serde`; we control the shape here so the
    /// canonical layout is stable.
    pub fn to_jsonl_line(&self) -> String {
        let mut out = String::new();
        out.push('{');
        write_kv_str(&mut out, "trace_id", &self.trace_id);
        out.push(',');
        write_kv_str(&mut out, "ts", &self.ts);
        out.push(',');
        write_kv_str(&mut out, "kind", &self.kind);
        if let Some(i) = &self.intent {
            out.push(',');
            write_kv_str(&mut out, "intent", i);
        }
        if let Some(s) = &self.scope {
            out.push(',');
            write_kv_str(&mut out, "scope", s);
        }
        if !self.fields.is_empty() {
            out.push(',');
            out.push_str("\"fields\":{");
            for (i, (k, v)) in self.fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_kv_raw(&mut out, k, v);
            }
            out.push('}');
        }
        out.push('}');
        out.push('\n');
        out
    }
}

fn write_kv_str(out: &mut String, key: &str, value: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    write_json_string(value, out);
}

/// Like `write_kv_str` but the value is already a JSON expression
/// (number, boolean, object, array). Used when the caller wants to
/// emit non-string field values.
fn write_kv_raw(out: &mut String, key: &str, value_as_json: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(value_as_json);
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

// ====================================================================
//  Tracer
// ====================================================================

/// One-per-run tracer. Owns the writer, the intent / scope stacks,
/// and the monotonic ULID generator. Cheap to clone (the writer
/// lives behind a shared `RefCell` so multiple call sites can emit).
pub struct Tracer {
    trace_id: String,
    inner: std::rc::Rc<RefCell<Inner>>,
}

struct Inner {
    writer: Box<dyn Write>,
    intent_stack: Vec<String>,
    scope_stack: Vec<String>,
    last_ts_ms: u64,
    counter: u64,
    events: Vec<TraceEvent>,
}

impl Tracer {
    /// Build a tracer that writes JSONL events into `writer`. The
    /// trace id is generated from the current time and a fresh seed.
    pub fn new(writer: Box<dyn Write>) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let high = cheap_random_64(0);
        let low = cheap_random_64(1) as u16;
        let trace_id = encode_ulid(now, high, low);
        Self {
            trace_id,
            inner: std::rc::Rc::new(RefCell::new(Inner {
                writer,
                intent_stack: Vec::new(),
                scope_stack: Vec::new(),
                last_ts_ms: now,
                counter: 0,
                events: Vec::new(),
            })),
        }
    }

    /// In-memory tracer used by tests. The recorded events are kept
    /// in a `Vec<TraceEvent>` for direct inspection in addition to
    /// being written to the sink.
    pub fn in_memory() -> Self {
        Self::new(Box::new(Vec::<u8>::new()))
    }

    pub fn trace_id(&self) -> String {
        self.trace_id.clone()
    }

    /// Returns a clone-on-read snapshot of every event emitted so far.
    /// Tests use this to assert the trace shape.
    pub fn events(&self) -> Vec<TraceEvent> {
        self.inner.borrow().events.clone()
    }

    /// Push an intent onto the active stack and emit `intent_enter`.
    pub fn intent_enter(&self, intent: &str, scope: Option<&str>) {
        let scope_path = self.push_scope_for_intent(scope);
        self.inner
            .borrow_mut()
            .intent_stack
            .push(intent.to_string());
        self.record("intent_enter", scope_path, vec![]);
    }

    /// Pop the active intent and emit `intent_exit`.
    pub fn intent_exit(&self, outcome: &str) {
        let scope_path = self.current_scope();
        self.inner.borrow_mut().intent_stack.pop();
        self.record(
            "intent_exit",
            scope_path,
            vec![("outcome".into(), format!("\"{outcome}\""))],
        );
        self.pop_scope();
    }

    /// Push a scope name onto the stack (used by `intent_enter` and
    /// by saga / agent / fn entry points in M5+).
    fn push_scope_for_intent(&self, scope: Option<&str>) -> Option<String> {
        if let Some(s) = scope {
            self.inner.borrow_mut().scope_stack.push(s.to_string());
        }
        self.current_scope()
    }

    fn pop_scope(&self) {
        self.inner.borrow_mut().scope_stack.pop();
    }

    /// `"a.b.c"` for nested scopes; `None` if we are at module level.
    fn current_scope(&self) -> Option<String> {
        let i = self.inner.borrow();
        if i.scope_stack.is_empty() {
            None
        } else {
            Some(i.scope_stack.join("."))
        }
    }

    /// Emit a trace event tagged with the active intent. `fields` is a
    /// list of `(key, json_value_string)` pairs — values are emitted
    /// raw, so callers must JSON-quote string values themselves.
    pub fn record(&self, kind: &str, scope: Option<String>, fields: Vec<(String, String)>) {
        let mut inner = self.inner.borrow_mut();
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(inner.last_ts_ms);
        // Monotonicity bump per § 20.1: events within the same
        // millisecond carry an incrementing counter so their `ts`
        // strings sort lexicographically.
        if now_ms <= inner.last_ts_ms {
            inner.counter += 1;
        } else {
            inner.counter = 0;
            inner.last_ts_ms = now_ms;
        }
        let ts = format_iso_ms(inner.last_ts_ms, inner.counter);
        let intent = inner.intent_stack.last().cloned();
        let evt = TraceEvent {
            trace_id: self.trace_id.clone(),
            ts,
            kind: kind.to_string(),
            intent,
            scope,
            fields,
        };
        let line = evt.to_jsonl_line();
        let _ = inner.writer.write_all(line.as_bytes());
        inner.events.push(evt);
    }
}

impl Clone for Tracer {
    fn clone(&self) -> Self {
        Self {
            trace_id: self.trace_id.clone(),
            inner: self.inner.clone(),
        }
    }
}

/// Format `ts_ms` as ISO-8601 UTC with millisecond precision. The
/// `counter` suffix is appended only when non-zero — it preserves
/// monotonicity for events emitted within the same millisecond
/// without breaking parsers that ignore unknown fractional digits.
fn format_iso_ms(ts_ms: u64, counter: u64) -> String {
    // Naive but dependency-free conversion. Aeris's trace consumers
    // use the string lexicographically, so any monotonic encoding is
    // sufficient — chrono's full calendar arithmetic lands in M9.
    let secs = ts_ms / 1000;
    let millis = ts_ms % 1000;
    let date_part = days_from_epoch(secs / 86_400);
    let s_of_day = secs % 86_400;
    let h = s_of_day / 3600;
    let m = (s_of_day % 3600) / 60;
    let s = s_of_day % 60;
    // Always include the per-event counter as a fixed-width 6-digit
    // suffix so lex order tracks emission order. Without the suffix
    // the transition from counter=0 ("...mmmZ") to counter=1
    // ("...mmm000001Z") reverses ordering (`'Z' > '0'`).
    format!(
        "{}T{:02}:{:02}:{:02}.{:03}{:06}Z",
        date_part, h, m, s, millis, counter
    )
}

/// Convert days since 1970-01-01 to `YYYY-MM-DD`. Handles leap
/// years up to year 9999; sufficient for any wall-clock the
/// interpreter will see.
fn days_from_epoch(mut days: u64) -> String {
    let mut y: u64 = 1970;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let dpm = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m: usize = 0;
    while m < 12 && days >= dpm[m] {
        days -= dpm[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

// ====================================================================
//  Tests
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_is_26_crockford_chars() {
        let ulid = encode_ulid(1_714_000_000_000, 0x0123_4567_89AB_CDEF, 0x0123);
        assert_eq!(ulid.len(), 26);
        for c in ulid.chars() {
            assert!(CROCKFORD.contains(&(c as u8)), "non-Crockford char {c}");
        }
    }

    #[test]
    fn ulid_changes_per_call_within_same_ms() {
        let t = Tracer::in_memory();
        // Two events in the same ms should still sort monotonically
        // because the counter bumps the ts suffix.
        t.intent_enter("a", Some("scope_a"));
        t.intent_exit("ok");
        let evs = t.events();
        assert_eq!(evs.len(), 2);
        assert!(evs[0].ts <= evs[1].ts, "{} vs {}", evs[0].ts, evs[1].ts);
    }

    #[test]
    fn intent_enter_emits_event_with_intent_field() {
        let t = Tracer::in_memory();
        t.intent_enter("rotate the leaked TLS cert", Some("rotate_cert"));
        let evs = t.events();
        assert_eq!(evs.len(), 1);
        assert_eq!(evs[0].kind, "intent_enter");
        assert_eq!(evs[0].intent.as_deref(), Some("rotate the leaked TLS cert"));
        assert_eq!(evs[0].scope.as_deref(), Some("rotate_cert"));
    }

    #[test]
    fn intent_exit_emits_outcome_field() {
        let t = Tracer::in_memory();
        t.intent_enter("ship", Some("settle"));
        t.intent_exit("rolled_back");
        let evs = t.events();
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[1].kind, "intent_exit");
        assert!(evs[1]
            .fields
            .iter()
            .any(|(k, v)| k == "outcome" && v == "\"rolled_back\""));
    }

    #[test]
    fn nested_intents_are_lifo() {
        let t = Tracer::in_memory();
        t.intent_enter("outer", Some("a"));
        t.intent_enter("inner", Some("a.b"));
        // Events emitted *now* should carry the inner intent.
        t.record("probe", Some("a.b".into()), Vec::new());
        t.intent_exit("ok");
        // Events after the inner exit should fall back to outer.
        t.record("after", Some("a".into()), Vec::new());
        t.intent_exit("ok");
        let evs = t.events();
        let probe = evs.iter().find(|e| e.kind == "probe").unwrap();
        assert_eq!(probe.intent.as_deref(), Some("inner"));
        let after = evs.iter().find(|e| e.kind == "after").unwrap();
        assert_eq!(after.intent.as_deref(), Some("outer"));
    }

    #[test]
    fn jsonl_line_starts_and_ends_correctly() {
        let t = Tracer::in_memory();
        t.intent_enter("x", None);
        let line = t.events()[0].to_jsonl_line();
        assert!(line.starts_with('{'));
        assert!(line.ends_with("}\n"));
        assert!(line.contains("\"trace_id\":"));
        assert!(line.contains("\"ts\":"));
        assert!(line.contains("\"kind\":\"intent_enter\""));
        assert!(line.contains("\"intent\":\"x\""));
    }

    #[test]
    fn trace_id_is_stable_within_a_run() {
        let t = Tracer::in_memory();
        let id1 = t.events();
        let _ = id1;
        t.intent_enter("a", None);
        t.intent_enter("b", None);
        let evs = t.events();
        assert_eq!(evs[0].trace_id, evs[1].trace_id);
    }

    #[test]
    fn distinct_runs_get_distinct_trace_ids() {
        let a = Tracer::in_memory();
        // Sleep a millisecond between constructions to guarantee a
        // different timestamp prefix; on fast machines the random
        // tail alone is enough but this keeps the test deterministic.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = Tracer::in_memory();
        assert_ne!(a.trace_id(), b.trace_id());
    }

    #[test]
    fn ts_strings_are_monotonic_across_many_events() {
        let t = Tracer::in_memory();
        for i in 0..50 {
            t.record("tick", None, vec![("i".into(), i.to_string())]);
        }
        let evs = t.events();
        for w in evs.windows(2) {
            assert!(w[0].ts <= w[1].ts, "{} <= {}", w[0].ts, w[1].ts);
        }
    }

    #[test]
    fn date_formatter_handles_leap_year_boundary() {
        // 2024 is a leap year — Feb 29 is day 31+28 = day 59 (0-indexed).
        // Construct a ts_ms for 2024-02-29 00:00:00 UTC.
        let ts_ms: u64 = 1_709_164_800_000;
        let s = format_iso_ms(ts_ms, 0);
        assert!(s.starts_with("2024-02-29T"), "got {s}");
    }
}
