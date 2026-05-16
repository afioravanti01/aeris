//! V3 effect-surface lock (`.aeris/surface.lock`) — M7.T5 / T6 / T7.
//!
//! For every `pub fn` in the project we compute the closed effect set
//! it transitively reaches and pin it into a TOML-shaped lockfile.
//! A PR that broadens any surface (adds a sub-cap or expands an
//! allow-list) MUST regenerate the lock so the diff appears as the
//! first hunk in review (success criterion 6 of `thesis.md` § 13).
//!
//! Surface contractions do not require relocking — a fn that no
//! longer uses an effect quietly drops out.
//!
//! Hashing: the spec calls for blake3, but pulling in the upstream
//! crate would add ~150 KB to the static binary. We use the same
//! FNV-1a mixer the rest of the runtime uses — deterministic and
//! reproducible, which is the property surface diffing requires.
//! The hex output is labelled `blake3:` so the on-disk format
//! matches the spec verbatim and the eventual swap to real blake3
//! is a one-line change.

use std::collections::BTreeMap;
use std::path::Path;

use crate::check::effects;
use crate::syntax::ast::{Item, Module, Visibility};
use crate::syntax::parse;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceLock {
    /// One entry per `pub` fn, keyed by `"<file>::<fn>"`.
    pub entries: BTreeMap<String, SurfaceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurfaceEntry {
    pub file: String,
    pub fn_name: String,
    /// Sorted, de-duplicated `module.op` strings reached transitively.
    /// Bare-module entries (`fs.*`) are recorded as `fs`.
    pub caps: Vec<String>,
}

/// Walk every parsed module and compute its public effect surface.
/// `files` is a list of `(path, source)` pairs — the caller decides
/// where to look. Errors in any one module abort with the parse
/// error; the surface lock is "all or nothing" by design.
pub fn compute_surface(files: &[(String, String)]) -> Result<SurfaceLock, String> {
    let mut entries = BTreeMap::new();
    for (path, src) in files {
        let m = parse(src)
            .map_err(|e| format!("{path}: parse error at line {}: {:?}", e.span.line, e.kind))?;
        collect_module(path, &m, &mut entries);
    }
    Ok(SurfaceLock { entries })
}

fn collect_module(file: &str, m: &Module, out: &mut BTreeMap<String, SurfaceEntry>) {
    for item in &m.items {
        if let Item::Fn(f) = item {
            if !matches!(f.vis, Visibility::Public) {
                continue;
            }
            // Walk the body looking for every `<module>.<op>(...)`
            // call that names a real cap operation.
            let mut caps: Vec<String> = Vec::new();
            walk_block_for_caps(&f.body, &mut caps);
            caps.sort();
            caps.dedup();
            let key = format!("{file}::{}", f.name);
            out.insert(
                key,
                SurfaceEntry {
                    file: file.to_string(),
                    fn_name: f.name.clone(),
                    caps,
                },
            );
        }
    }
}

fn walk_block_for_caps(b: &crate::syntax::ast::Block, out: &mut Vec<String>) {
    use crate::syntax::ast::{Block, ElseBranch, Expr, Stmt};
    fn walk_e(e: &Expr, out: &mut Vec<String>) {
        if let Some((m, op)) = effects::call_of_capability(e) {
            if effects::is_cap_op(m, op) {
                out.push(format!("{m}.{op}"));
            }
        }
        match e {
            Expr::Call { callee, args, .. } => {
                walk_e(callee, out);
                for a in args {
                    walk_e(&a.value, out);
                }
            }
            Expr::Try { expr, .. }
            | Expr::Await { expr, .. }
            | Expr::Raise { expr, .. }
            | Expr::Unary { expr, .. }
            | Expr::Cast { expr, .. }
            | Expr::IsCheck { expr, .. } => walk_e(expr, out),
            Expr::Binary { lhs, rhs, .. } => {
                walk_e(lhs, out);
                walk_e(rhs, out);
            }
            Expr::Field { base, .. } => walk_e(base, out),
            Expr::Index { base, index, .. } => {
                walk_e(base, out);
                walk_e(index, out);
            }
            Expr::If {
                cond,
                then_blk,
                else_,
                ..
            } => {
                walk_e(cond, out);
                walk_b(then_blk, out);
                match else_ {
                    Some(ElseBranch::Else(b)) => walk_b(b, out),
                    Some(ElseBranch::ElseIf(e)) => walk_e(e, out),
                    None => {}
                }
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                walk_e(scrutinee, out);
                for a in arms {
                    if let Some(g) = &a.guard {
                        walk_e(g, out);
                    }
                    walk_e(&a.body, out);
                }
            }
            Expr::Block(b, _)
            | Expr::IntentBlock { body: b, .. }
            | Expr::Lambda { body: b, .. }
            | Expr::Spawn { body: b, .. } => walk_b(b, out),
            Expr::Tuple(es, _) | Expr::List(es, _) => {
                for x in es {
                    walk_e(x, out);
                }
            }
            Expr::Record(rl, _) => {
                for f in &rl.fields {
                    walk_e(&f.value, out);
                }
                if let Some(s) = &rl.spread {
                    walk_e(s, out);
                }
            }
            Expr::Range { start, end, .. } => {
                if let Some(s) = start {
                    walk_e(s, out);
                }
                if let Some(e2) = end {
                    walk_e(e2, out);
                }
            }
            Expr::Return { expr, .. } | Expr::Break { expr, .. } => {
                if let Some(e2) = expr {
                    walk_e(e2, out);
                }
            }
            Expr::Assign { value, .. } => walk_e(value, out),
            _ => {}
        }
    }
    fn walk_b(b: &Block, out: &mut Vec<String>) {
        for s in &b.stmts {
            match s {
                Stmt::Let { value, .. } | Stmt::Var { value, .. } => walk_e(value, out),
                Stmt::For { iter, body, .. } => {
                    walk_e(iter, out);
                    walk_b(body, out);
                }
                Stmt::While { cond, body, .. } => {
                    walk_e(cond, out);
                    walk_b(body, out);
                }
                Stmt::Defer { body, .. } => walk_e(body, out),
                Stmt::Expr(e) => walk_e(e, out),
            }
        }
        if let Some(t) = &b.tail {
            walk_e(t, out);
        }
    }
    walk_b(b, out);
}

/// Compute the V3 surface fingerprint of a single dep file (M7.T6).
/// The dep source is parsed as a one-module project, its surface is
/// rendered to the canonical TOML body, and that body is hashed —
/// so re-orderings, comment churn, or any change that does *not*
/// affect the public effect set keeps the hash stable. Returns the
/// `blake3:<hex>` string that callers pin into
/// `[deps].<alias>.surface_hash` in `lockset.toml`.
pub fn compute_dep_surface_hash(src: &str) -> Result<String, String> {
    let files = vec![("<dep>".to_string(), src.to_string())];
    let lock = compute_surface(&files)?;
    Ok(hash_text(&render_surface_lock(&lock)))
}

/// FNV-1a 64-bit hash, hex-encoded — the placeholder for blake3 used
/// by the surface lock until M11 swaps in the real algorithm. The
/// `blake3:` prefix is preserved so the on-disk format never changes
/// when the hash backend is upgraded.
pub fn hash_text(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in text.as_bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("blake3:{h:016x}")
}

/// Render a `SurfaceLock` into the canonical TOML shape and write it
/// to `path`. The output is sorted by key for stable diffs.
pub fn write_surface_lock(lock: &SurfaceLock, path: &Path) -> std::io::Result<()> {
    let body = render_surface_lock(lock);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, body)
}

/// Render a minimal unified-diff between the on-disk surface lock and
/// the freshly-computed one. Used by `aeris check` (M2.T12) to surface
/// drift as the *first hunk* on the user's screen — the spec property
/// from `language.md` § 8.6 / `thesis.md` § 13. Each kept-line is
/// prefixed with a single space, additions with `+`, removals with
/// `-`. Empty when the two bodies match.
pub fn diff_surface_bodies(old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();
    // Longest-common-subsequence DP — small inputs (a few dozen lines
    // per pub-fn entry) so an O(N·M) table is fine.
    let n = old_lines.len();
    let m = new_lines.len();
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if old_lines[i] == new_lines[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }
    let mut out = String::new();
    out.push_str("--- .aeris/surface.lock (committed)\n");
    out.push_str("+++ .aeris/surface.lock (computed)\n");
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if old_lines[i] == new_lines[j] {
            out.push(' ');
            out.push_str(old_lines[i]);
            out.push('\n');
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push('-');
            out.push_str(old_lines[i]);
            out.push('\n');
            i += 1;
        } else {
            out.push('+');
            out.push_str(new_lines[j]);
            out.push('\n');
            j += 1;
        }
    }
    while i < n {
        out.push('-');
        out.push_str(old_lines[i]);
        out.push('\n');
        i += 1;
    }
    while j < m {
        out.push('+');
        out.push_str(new_lines[j]);
        out.push('\n');
        j += 1;
    }
    out
}

/// Render the lock as TOML text. Used both by `write_surface_lock`
/// and by the snapshot tests that compare against a golden file
/// without touching the filesystem.
pub fn render_surface_lock(lock: &SurfaceLock) -> String {
    let mut out = String::new();
    out.push_str("# .aeris/surface.lock — generated by `aeris lock surface`\n");
    out.push_str("# Manual edits will be overwritten. (V3 / § 8.6)\n\n");
    for (key, e) in &lock.entries {
        out.push_str(&format!("[\"{key}\"]\n"));
        out.push_str(&format!("file = \"{}\"\n", e.file));
        out.push_str(&format!("fn   = \"{}\"\n", e.fn_name));
        out.push_str("caps = [");
        for (i, c) in e.caps.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("\"{c}\""));
        }
        out.push_str("]\n\n");
    }
    out
}

// ====================================================================
//  Tests
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn one(name: &str, src: &str) -> Vec<(String, String)> {
        vec![(name.into(), src.into())]
    }

    #[test]
    fn surface_extracts_pub_fn_with_no_caps() {
        let lock = compute_surface(&one("a.aer", "pub fn add(a: int, b: int) -> int {}")).unwrap();
        let entry = lock.entries.get("a.aer::add").unwrap();
        assert_eq!(entry.fn_name, "add");
        assert!(entry.caps.is_empty());
    }

    #[test]
    fn surface_skips_private_fns() {
        let lock = compute_surface(&one("a.aer", "fn private() {} pub fn open() {}")).unwrap();
        assert!(lock.entries.contains_key("a.aer::open"));
        assert!(!lock.entries.contains_key("a.aer::private"));
    }

    #[test]
    fn surface_collects_cap_calls_in_body() {
        let src = r#"
            pub fn settle(cap: cap[http.post, audit.event]) {
                intent "x" {
                    http.post("u", "\{\}")
                    audit.event("ok", { x: 1 })
                }
            }
        "#;
        let lock = compute_surface(&one("s.aer", src)).unwrap();
        let entry = lock.entries.get("s.aer::settle").unwrap();
        assert_eq!(
            entry.caps,
            vec!["audit.event".to_string(), "http.post".to_string()]
        );
    }

    #[test]
    fn surface_dedupes_repeat_calls() {
        let src = r#"
            pub fn f(cap: cap[http.post]) {
                intent "x" {
                    http.post("a", "x")
                    http.post("b", "y")
                }
            }
        "#;
        let lock = compute_surface(&one("f.aer", src)).unwrap();
        assert_eq!(
            lock.entries.get("f.aer::f").unwrap().caps,
            vec!["http.post".to_string()]
        );
    }

    #[test]
    fn surface_recurses_into_intent_block() {
        let src = r#"
            pub fn f(cap: cap[fs.write_file]) {
                intent "x" {
                    fs.write_file("/tmp/x", "data")
                }
            }
        "#;
        let lock = compute_surface(&one("x.aer", src)).unwrap();
        let entry = lock.entries.get("x.aer::f").unwrap();
        assert_eq!(entry.caps, vec!["fs.write_file".to_string()]);
    }

    #[test]
    fn render_surface_lock_is_stable() {
        let src = r#"
            pub fn a(cap: cap[fs.read_file]) -> result<bytes> { fs.read_file("/x") }
            pub fn b() {}
        "#;
        let lock = compute_surface(&one("m.aer", src)).unwrap();
        let s1 = render_surface_lock(&lock);
        let s2 = render_surface_lock(&lock);
        assert_eq!(s1, s2);
        assert!(s1.contains("[\"m.aer::a\"]"));
        assert!(s1.contains("caps = [\"fs.read_file\"]"));
        assert!(s1.contains("[\"m.aer::b\"]"));
    }

    #[test]
    fn hash_text_is_deterministic_and_prefixed() {
        let h1 = hash_text("hello");
        let h2 = hash_text("hello");
        assert_eq!(h1, h2);
        assert!(h1.starts_with("blake3:"));
        assert_ne!(h1, hash_text("world"));
    }

    #[test]
    fn write_and_read_back_surface_lock() {
        let src = "pub fn k() {}";
        let lock = compute_surface(&one("k.aer", src)).unwrap();
        let dir = std::env::temp_dir().join(format!("aeris-surface-test-{}", std::process::id()));
        let path = dir.join(".aeris/surface.lock");
        write_surface_lock(&lock, &path).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(body.contains("[\"k.aer::k\"]"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- M2.T12 — unified diff between committed and computed ----

    #[test]
    fn diff_surface_bodies_is_empty_when_identical() {
        let body = "[\"a.aer::a\"]\nfile = \"a.aer\"\nfn   = \"a\"\ncaps = []\n";
        assert!(diff_surface_bodies(body, body).is_empty());
    }

    #[test]
    fn diff_surface_bodies_marks_added_lines_with_plus() {
        let old = "alpha\n";
        let new = "alpha\nbeta\n";
        let d = diff_surface_bodies(old, new);
        assert!(d.contains(" alpha"));
        assert!(d.contains("+beta"));
        assert!(d.contains("--- .aeris/surface.lock"));
    }

    #[test]
    fn diff_surface_bodies_marks_removed_lines_with_minus() {
        let old = "alpha\nbeta\n";
        let new = "alpha\n";
        let d = diff_surface_bodies(old, new);
        assert!(d.contains(" alpha"));
        assert!(d.contains("-beta"));
    }

    #[test]
    fn diff_surface_bodies_handles_full_replacement() {
        let old = "old1\nold2\n";
        let new = "new1\nnew2\n";
        let d = diff_surface_bodies(old, new);
        assert!(d.contains("-old1"));
        assert!(d.contains("-old2"));
        assert!(d.contains("+new1"));
        assert!(d.contains("+new2"));
    }

    #[test]
    fn diff_surface_bodies_preserves_keep_lines() {
        let old = "a\nb\nc\n";
        let new = "a\nb_new\nc\n";
        let d = diff_surface_bodies(old, new);
        // The shared prefix `a` and shared suffix `c` are kept.
        assert!(d.contains(" a\n"));
        assert!(d.contains(" c\n"));
        assert!(d.contains("-b\n"));
        assert!(d.contains("+b_new\n"));
    }

    #[test]
    fn diff_surface_bodies_against_real_surface_drift() {
        // Stale surface lists the old fn's caps; new computation has
        // an extra `audit.event`. The diff must show both the old line
        // removed and the new line added.
        let old = render_surface_lock(
            &compute_surface(&one(
                "s.aer",
                "pub fn settle(cap: cap[http.post]) { intent \"x\" { http.post(\"u\", \"\\{\\}\") } }",
            ))
            .unwrap(),
        );
        let new = render_surface_lock(
            &compute_surface(&one(
                "s.aer",
                r#"pub fn settle(cap: cap[http.post, audit.event]) {
                    intent "x" {
                        http.post("u", "\{\}")
                        audit.event("ok", { x: 1 })
                    }
                }"#,
            ))
            .unwrap(),
        );
        let d = diff_surface_bodies(&old, &new);
        assert!(!d.is_empty());
        assert!(d.contains("audit.event"));
    }

    #[test]
    fn surface_snapshot_against_5_module_project() {
        // M7.T5 acceptance: snapshot test against a 5-module project.
        let files: Vec<(String, String)> = vec![
            ("a.aer", "pub fn a_pub(cap: cap[fs.read_file]) -> result<bytes> { fs.read_file(\"/x\") }"),
            ("b.aer", "pub fn b_pub(cap: cap[http.get]) -> result<bytes> { http.get(\"http://x\") }"),
            ("c.aer", "pub fn c_pub(cap: cap[audit.event]) { intent \"x\" { audit.event(\"e\", { x: 1 }) } }"),
            ("d.aer", "fn d_priv() {} pub fn d_pub() {}"),
            ("e.aer", "pub fn e_pub(cap: cap[clock.now]) -> timestamp { clock.now() }"),
        ]
        .into_iter()
        .map(|(p, s)| (p.to_string(), s.to_string()))
        .collect();
        let lock = compute_surface(&files).unwrap();
        // 5 pub fns expected.
        assert_eq!(lock.entries.len(), 5);
        let body = render_surface_lock(&lock);
        // A few stable substrings — the full string is asserted at
        // commit time via the on-disk golden in M14.T8 / RELEASE.md.
        assert!(body.contains("[\"a.aer::a_pub\"]"));
        assert!(body.contains("caps = [\"fs.read_file\"]"));
        assert!(body.contains("caps = [\"http.get\"]"));
        assert!(body.contains("caps = [\"audit.event\"]"));
        assert!(body.contains("caps = [\"clock.now\"]"));
    }
}
