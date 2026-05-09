//! M13.T1 — `aeris trace diff <a> <b>`.
//!
//! Realises `docs/language.md` § 20.4: aligns events by
//! `(scope, ordinal)` and reports diverging fields, missing events
//! (in `a` but not `b`) and extra events (in `b` but not `a`).
//!
//! Used by the CLI for regression bisects: a passing trace and a
//! failing trace get aligned on their shared scopes; the first
//! diverging field is the bisect's anchor.

use std::collections::BTreeMap;

use super::trace::TraceEvent;

/// One row of the diff report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffEntry {
    /// `a[i]` has fields that differ from `b[i]` at the same
    /// `(scope, ordinal)`. `details` is one human-readable line per
    /// diverging dimension (`kind`, `intent`, or a named field).
    Divergence {
        scope: String,
        ordinal: usize,
        details: Vec<String>,
    },
    /// `a` has an event at `(scope, ordinal)` that `b` does not.
    MissingInB {
        scope: String,
        ordinal: usize,
        kind: String,
    },
    /// `b` has an event at `(scope, ordinal)` that `a` does not.
    ExtraInB {
        scope: String,
        ordinal: usize,
        kind: String,
    },
}

/// Compute the per-scope alignment diff between two traces. Returns
/// the rows in stable scope-then-ordinal order so output is
/// reproducible.
pub fn diff_traces(a: &[TraceEvent], b: &[TraceEvent]) -> Vec<DiffEntry> {
    let by_scope_a = group_by_scope(a);
    let by_scope_b = group_by_scope(b);
    let mut all_scopes: Vec<&str> = by_scope_a
        .keys()
        .chain(by_scope_b.keys())
        .map(|s| s.as_str())
        .collect();
    all_scopes.sort();
    all_scopes.dedup();
    let mut out: Vec<DiffEntry> = Vec::new();
    for scope in all_scopes {
        let empty: Vec<&TraceEvent> = Vec::new();
        let xs = by_scope_a.get(scope).unwrap_or(&empty);
        let ys = by_scope_b.get(scope).unwrap_or(&empty);
        let max = xs.len().max(ys.len());
        for i in 0..max {
            match (xs.get(i), ys.get(i)) {
                (Some(x), Some(y)) => {
                    let details = compare_events(x, y);
                    if !details.is_empty() {
                        out.push(DiffEntry::Divergence {
                            scope: scope.to_string(),
                            ordinal: i,
                            details,
                        });
                    }
                }
                (Some(x), None) => out.push(DiffEntry::MissingInB {
                    scope: scope.to_string(),
                    ordinal: i,
                    kind: x.kind.clone(),
                }),
                (None, Some(y)) => out.push(DiffEntry::ExtraInB {
                    scope: scope.to_string(),
                    ordinal: i,
                    kind: y.kind.clone(),
                }),
                (None, None) => unreachable!(),
            }
        }
    }
    out
}

fn group_by_scope(events: &[TraceEvent]) -> BTreeMap<String, Vec<&TraceEvent>> {
    let mut out: BTreeMap<String, Vec<&TraceEvent>> = BTreeMap::new();
    for e in events {
        let key = e.scope.clone().unwrap_or_default();
        out.entry(key).or_default().push(e);
    }
    out
}

fn compare_events(a: &TraceEvent, b: &TraceEvent) -> Vec<String> {
    let mut details = Vec::new();
    if a.kind != b.kind {
        details.push(format!("kind: `{}` vs `{}`", a.kind, b.kind));
    }
    if a.intent != b.intent {
        details.push(format!(
            "intent: {:?} vs {:?}",
            a.intent.as_deref().unwrap_or(""),
            b.intent.as_deref().unwrap_or("")
        ));
    }
    // Compare field-by-field. We use a BTreeMap because event field
    // order is by emission, not stable across runs — tracer-driven
    // ordering counts as a divergence too, but we don't surface it
    // unless an actual value differs.
    let amap: BTreeMap<&str, &str> = a
        .fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let bmap: BTreeMap<&str, &str> = b
        .fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    for (k, av) in &amap {
        match bmap.get(k) {
            Some(bv) if av != bv => {
                details.push(format!("field `{k}`: `{av}` vs `{bv}`"));
            }
            None => details.push(format!("field `{k}` only in a: `{av}`")),
            _ => {}
        }
    }
    for (k, bv) in &bmap {
        if !amap.contains_key(k) {
            details.push(format!("field `{k}` only in b: `{bv}`"));
        }
    }
    details
}

/// Render a diff into a human-readable report. Empty when the two
/// traces align byte-for-byte.
pub fn render_diff(rows: &[DiffEntry]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for row in rows {
        match row {
            DiffEntry::Divergence {
                scope,
                ordinal,
                details,
            } => {
                out.push_str(&format!(
                    "~ [{scope}#{ordinal}] divergence:\n"
                ));
                for d in details {
                    out.push_str(&format!("    - {d}\n"));
                }
            }
            DiffEntry::MissingInB {
                scope,
                ordinal,
                kind,
            } => {
                out.push_str(&format!(
                    "- [{scope}#{ordinal}] missing in b: kind=`{kind}`\n"
                ));
            }
            DiffEntry::ExtraInB {
                scope,
                ordinal,
                kind,
            } => {
                out.push_str(&format!(
                    "+ [{scope}#{ordinal}] extra in b: kind=`{kind}`\n"
                ));
            }
        }
    }
    out
}

// ====================================================================
//  Tests
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn evt(kind: &str, scope: Option<&str>, fields: &[(&str, &str)]) -> TraceEvent {
        TraceEvent {
            trace_id: "01TEST".into(),
            ts: "2026-01-01T00:00:00.000Z".into(),
            kind: kind.into(),
            intent: None,
            scope: scope.map(|s| s.to_string()),
            fields: fields
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
        }
    }

    #[test]
    fn identical_traces_produce_empty_diff() {
        let a = vec![evt("step", Some("saga.charge"), &[("k", "v")])];
        let b = a.clone();
        assert!(diff_traces(&a, &b).is_empty());
    }

    #[test]
    fn single_field_divergence_is_reported() {
        let a = vec![evt("step", Some("saga.charge"), &[("amount", "10")])];
        let b = vec![evt("step", Some("saga.charge"), &[("amount", "11")])];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        match &d[0] {
            DiffEntry::Divergence {
                scope,
                ordinal,
                details,
            } => {
                assert_eq!(scope, "saga.charge");
                assert_eq!(*ordinal, 0);
                assert!(details.iter().any(|s| s.contains("amount")));
                assert!(details.iter().any(|s| s.contains("10") && s.contains("11")));
            }
            other => panic!("wrong row: {other:?}"),
        }
    }

    #[test]
    fn missing_event_in_b_is_reported() {
        let a = vec![
            evt("step", Some("saga.charge"), &[]),
            evt("step", Some("saga.charge"), &[]),
        ];
        let b = vec![evt("step", Some("saga.charge"), &[])];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        assert!(matches!(d[0], DiffEntry::MissingInB { ordinal: 1, .. }));
    }

    #[test]
    fn extra_event_in_b_is_reported() {
        let a = vec![evt("step", Some("saga.charge"), &[])];
        let b = vec![
            evt("step", Some("saga.charge"), &[]),
            evt("step", Some("saga.charge"), &[]),
        ];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        assert!(matches!(d[0], DiffEntry::ExtraInB { ordinal: 1, .. }));
    }

    #[test]
    fn diff_aligns_per_scope_independently() {
        // A diverges in saga.a, agrees in saga.b. Only one row.
        let a = vec![
            evt("e1", Some("saga.a"), &[("v", "1")]),
            evt("e2", Some("saga.b"), &[("x", "y")]),
        ];
        let b = vec![
            evt("e1", Some("saga.a"), &[("v", "2")]),
            evt("e2", Some("saga.b"), &[("x", "y")]),
        ];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        assert!(matches!(d[0], DiffEntry::Divergence { .. }));
        if let DiffEntry::Divergence { scope, .. } = &d[0] {
            assert_eq!(scope, "saga.a");
        }
    }

    #[test]
    fn missing_field_in_b_is_reported() {
        let a = vec![evt("e", Some("s"), &[("a", "1"), ("b", "2")])];
        let b = vec![evt("e", Some("s"), &[("a", "1")])];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        if let DiffEntry::Divergence { details, .. } = &d[0] {
            assert!(details.iter().any(|s| s.contains("only in a") && s.contains("`b`")));
        } else {
            panic!();
        }
    }

    #[test]
    fn extra_field_in_b_is_reported() {
        let a = vec![evt("e", Some("s"), &[("a", "1")])];
        let b = vec![evt("e", Some("s"), &[("a", "1"), ("b", "2")])];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        if let DiffEntry::Divergence { details, .. } = &d[0] {
            assert!(details.iter().any(|s| s.contains("only in b") && s.contains("`b`")));
        } else {
            panic!();
        }
    }

    #[test]
    fn kind_divergence_is_reported() {
        let a = vec![evt("step_enter", Some("s"), &[])];
        let b = vec![evt("step_exit", Some("s"), &[])];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        if let DiffEntry::Divergence { details, .. } = &d[0] {
            assert!(details.iter().any(|s| s.contains("kind")));
            assert!(details.iter().any(|s| s.contains("step_enter")));
            assert!(details.iter().any(|s| s.contains("step_exit")));
        } else {
            panic!();
        }
    }

    #[test]
    fn render_diff_is_empty_for_identical_traces() {
        assert!(render_diff(&[]).is_empty());
    }

    #[test]
    fn render_diff_marks_divergence_with_tilde() {
        let rows = vec![DiffEntry::Divergence {
            scope: "saga.a".into(),
            ordinal: 0,
            details: vec!["field `x`: `1` vs `2`".into()],
        }];
        let s = render_diff(&rows);
        assert!(s.contains("~ [saga.a#0]"));
        assert!(s.contains("field `x`"));
    }

    #[test]
    fn render_diff_marks_missing_with_minus_extra_with_plus() {
        let rows = vec![
            DiffEntry::MissingInB {
                scope: "s".into(),
                ordinal: 1,
                kind: "step".into(),
            },
            DiffEntry::ExtraInB {
                scope: "s".into(),
                ordinal: 2,
                kind: "step".into(),
            },
        ];
        let s = render_diff(&rows);
        assert!(s.contains("- [s#1]"));
        assert!(s.contains("+ [s#2]"));
    }

    #[test]
    fn events_without_scope_align_under_empty_key() {
        let a = vec![evt("e", None, &[("k", "v1")])];
        let b = vec![evt("e", None, &[("k", "v2")])];
        let d = diff_traces(&a, &b);
        assert_eq!(d.len(), 1);
        if let DiffEntry::Divergence { scope, .. } = &d[0] {
            assert_eq!(scope, "");
        } else {
            panic!();
        }
    }
}
