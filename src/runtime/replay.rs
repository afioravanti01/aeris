//! M9.T4–T7: replay tape. Re-runs a program against the
//! non-deterministic events captured during a prior run (`ai.*`,
//! `clock.now`, `random.next`, `http.*`, `fs.read_*`). The mechanism
//! is FIFO per-kind: each call drains the head entry of the matching
//! kind, in source order. Mismatches between live and replay surface
//! as `policy_drift` events when the caller re-evaluates the recorded
//! shape (M8.T6).
//!
//! Realises `docs/language.md` § 20.3.

use std::cell::RefCell;
use std::rc::Rc;

use super::trace::TraceEvent;

/// Replay strategy selected at the CLI surface.
///
/// - `FromFixtures` (default): every recorded kind is replayed; no
///   network or LLM contact (§ 20.3).
/// - `Live`: `clock.now` and `random.next` are replayed (so the
///   deterministic subset stays bit-identical) but `ai.*` and
///   `http.*` go live again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayMode {
    FromFixtures,
    Live,
}

impl ReplayMode {
    /// Whether `kind` should be drained from the tape under this mode.
    /// Returns `false` for live-only kinds.
    pub fn replays(self, kind: &str) -> bool {
        match self {
            ReplayMode::FromFixtures => matches!(
                kind,
                "ai_call" | "clock_now" | "random_next" | "http_call" | "fs_read"
            ),
            ReplayMode::Live => matches!(kind, "clock_now" | "random_next"),
        }
    }
}

/// In-memory replay tape: a queue of recorded events with a cursor.
/// Each `consume_next(kind)` advances past any earlier events whose
/// kind doesn't replay under the active mode, then returns the
/// matching head — or `None` if the tape is exhausted.
#[derive(Debug, Clone)]
pub struct Tape {
    entries: Vec<TraceEvent>,
    cursor: usize,
    mode: ReplayMode,
}

impl Tape {
    pub fn new(entries: Vec<TraceEvent>, mode: ReplayMode) -> Self {
        Self {
            entries,
            cursor: 0,
            mode,
        }
    }

    pub fn from_events(events: Vec<TraceEvent>) -> Self {
        Self::new(events, ReplayMode::FromFixtures)
    }

    pub fn mode(&self) -> ReplayMode {
        self.mode
    }

    /// Drain the next event whose `kind` matches and that the active
    /// mode replays. Live-only kinds are skipped, never consumed.
    pub fn consume_next(&mut self, kind: &str) -> Option<TraceEvent> {
        if !self.mode.replays(kind) {
            return None;
        }
        while self.cursor < self.entries.len() {
            let evt = &self.entries[self.cursor];
            if evt.kind == kind {
                let out = evt.clone();
                self.cursor += 1;
                return Some(out);
            }
            // Skip events that are not interesting to this consumer
            // (e.g. `intent_enter`, `step_enter`). They stay in the
            // tape because they don't drive non-determinism.
            self.cursor += 1;
        }
        None
    }

    /// Read a recorded field by name as a borrowed slice. Strips a
    /// single layer of outer JSON quoting if present. Use
    /// `field_unescaped` when the consumer needs the natural string
    /// content (e.g. the agent runtime, which feeds the value back
    /// into `decode_natural_object`).
    pub fn field<'a>(evt: &'a TraceEvent, key: &str) -> Option<&'a str> {
        evt.fields.iter().find(|(k, _)| k == key).map(|(_, v)| {
            let s = v.as_str();
            if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
                &s[1..s.len() - 1]
            } else {
                s
            }
        })
    }

    /// Read a recorded string field with JSON escapes resolved
    /// (`\"`, `\\`, `\n`, ...). Returns `None` if the field is absent.
    pub fn field_unescaped(evt: &TraceEvent, key: &str) -> Option<String> {
        let raw = Self::field(evt, key)?;
        Some(unescape_json_string(raw))
    }
}

fn unescape_json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('b') => out.push('\u{08}'),
            Some('f') => out.push('\u{0c}'),
            Some('u') => {
                let mut code: u32 = 0;
                for _ in 0..4 {
                    let h = chars.next().unwrap_or('0');
                    let v = match h {
                        '0'..='9' => (h as u32) - ('0' as u32),
                        'a'..='f' => (h as u32) - ('a' as u32) + 10,
                        'A'..='F' => (h as u32) - ('A' as u32) + 10,
                        _ => 0,
                    };
                    code = (code << 4) | v;
                }
                if let Some(ch) = char::from_u32(code) {
                    out.push(ch);
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

/// Convenience handle the runtime threads through the env. Wrapped in
/// `Rc<RefCell<...>>` so the tape's cursor advances across cap calls
/// while every closure shares the same view.
pub type TapeHandle = Rc<RefCell<Tape>>;

pub fn handle_from_events(events: Vec<TraceEvent>, mode: ReplayMode) -> TapeHandle {
    Rc::new(RefCell::new(Tape::new(events, mode)))
}

/// Parse a JSONL trace file (one event per non-empty line) into the
/// in-memory `TraceEvent` representation. Used by the CLI's
/// `aeris replay <trace_file>` path. Robust to blank lines and to
/// trailing whitespace; rejects lines that aren't well-formed JSON.
pub fn parse_trace_jsonl(content: &str) -> Result<Vec<TraceEvent>, String> {
    let mut out = Vec::new();
    for (i, line) in content.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let pairs = super::json::decode_natural_object(line)
            .map_err(|e| format!("trace line {}: {}", i + 1, e.message))?;
        let mut trace_id = String::new();
        let mut ts = String::new();
        let mut kind = String::new();
        let mut intent: Option<String> = None;
        let mut scope: Option<String> = None;
        let mut fields: Vec<(String, String)> = Vec::new();
        for (k, v) in pairs {
            match (k.as_str(), v) {
                ("trace_id", super::value::Value::Str(s)) => trace_id = s,
                ("ts", super::value::Value::Str(s)) => ts = s,
                ("kind", super::value::Value::Str(s)) => kind = s,
                ("intent", super::value::Value::Str(s)) => intent = Some(s),
                ("scope", super::value::Value::Str(s)) => scope = Some(s),
                ("fields", super::value::Value::Record(r)) => {
                    for (fk, fv) in r.fields {
                        fields.push((fk, value_to_raw_json(&fv)));
                    }
                }
                _ => {}
            }
        }
        if kind.is_empty() {
            return Err(format!("trace line {}: missing `kind`", i + 1));
        }
        out.push(TraceEvent {
            trace_id,
            ts,
            kind,
            intent,
            scope,
            fields,
        });
    }
    Ok(out)
}

/// Re-serialise a `Value` produced by the natural-JSON parser back
/// into the raw JSON fragment form used in `TraceEvent.fields`.
/// Strings are quoted; numbers/bools land bare. Callers that just
/// want the textual content (e.g. `Tape::field`) strip the quotes.
fn value_to_raw_json(v: &super::value::Value) -> String {
    use super::value::Value;
    match v {
        Value::Str(s) => format!("\"{}\"", json_escape(s)),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => b.to_string(),
        Value::Unit => "null".into(),
        other => format!("{other:?}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(kind: &str, fields: Vec<(&str, &str)>) -> TraceEvent {
        TraceEvent {
            trace_id: "t".into(),
            ts: "now".into(),
            kind: kind.into(),
            intent: None,
            scope: None,
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn from_fixtures_replays_recorded_kinds() {
        let events = vec![evt("ai_call", vec![("model", "\"haiku\"")])];
        let mut t = Tape::new(events, ReplayMode::FromFixtures);
        let e = t.consume_next("ai_call").unwrap();
        assert_eq!(Tape::field(&e, "model"), Some("haiku"));
    }

    #[test]
    fn live_mode_skips_ai_calls() {
        let events = vec![evt("ai_call", vec![("x", "1")])];
        let mut t = Tape::new(events, ReplayMode::Live);
        // ai_call is not replayed in live mode — caller goes live.
        assert!(t.consume_next("ai_call").is_none());
    }

    #[test]
    fn cursor_advances_past_skipped_events() {
        let events = vec![
            evt("intent_enter", vec![]),
            evt("clock_now", vec![("value", "\"42\"")]),
        ];
        let mut t = Tape::new(events, ReplayMode::FromFixtures);
        let e = t.consume_next("clock_now").unwrap();
        assert_eq!(Tape::field(&e, "value"), Some("42"));
        // Tape exhausted.
        assert!(t.consume_next("clock_now").is_none());
    }
}
