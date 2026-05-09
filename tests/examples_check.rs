//! M14.T7 — every example under `examples/` must pass `aeris check`.
//!
//! Walks every `*.aer` file under `examples/`, parses it (with the
//! adjacent `lockset.toml` if present), runs the static checker
//! (M2.T1+) and asserts the diagnostic batch is empty. Templated
//! after the `aeris check` CLI driver so the test catches the same
//! drift the user would hit.

use std::path::Path;

use aeris::check::{check_module, check_module_with_lockset};
use aeris::lockset::parse_lockset;
use aeris::syntax::parse;

fn each_example<F: FnMut(&Path)>(mut f: F) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    if !root.exists() {
        return;
    }
    for entry in walkdir(&root) {
        if entry.extension().and_then(|s| s.to_str()) == Some("aer") {
            f(&entry);
        }
    }
}

fn walkdir(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let rd = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(_) => return out,
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            out.extend(walkdir(&p));
        } else {
            out.push(p);
        }
    }
    out
}

#[test]
fn every_example_checks_clean() {
    let mut checked = 0usize;
    each_example(|p| {
        let src = std::fs::read_to_string(p).expect("read");
        let module = parse(&src).unwrap_or_else(|e| {
            panic!("{}: parse error at line {}: {:?}", p.display(), e.span.line, e.kind)
        });
        let lockset_path = p.parent().unwrap().join("lockset.toml");
        let errs = if lockset_path.exists() {
            let toml = std::fs::read_to_string(&lockset_path).expect("read lockset");
            let lockset = parse_lockset(&toml).expect("parse lockset");
            check_module_with_lockset(&module, &lockset.caps)
        } else {
            check_module(&module)
        };
        assert!(
            errs.is_empty(),
            "{}: expected clean check, got {errs:#?}",
            p.display()
        );
        checked += 1;
    });
    assert!(checked >= 3, "expected at least 3 examples, found {checked}");
}
