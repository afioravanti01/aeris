//! Multi-file module loader.
//!
//! Resolves `use "./lib/foo.aer"` and `use alias from "./lib/foo.aer"`
//! by reading each referenced file, parsing it, and **inlining** its
//! top-level items into the entry module. Cycles are rejected at
//! parse time (§ 3.2).
//!
//! The loader is intentionally minimal: it preserves the v0.3
//! single-namespace semantics — every `pub` declaration from a
//! sub-module becomes visible at the entry module's top level.
//! The aliased form (`use alias from "./…"`) currently exposes the
//! same flat namespace; the alias is captured for tooling (M33)
//! but does not introduce a `alias.foo` prefix in v0.3.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::syntax::ast::Module;
use crate::syntax::{parse, ParseError};

/// Why a multi-file load failed. The CLI converts each variant into
/// the right exit code (`64` for parse, `1` for IO, `64` for cycle).
#[derive(Debug)]
pub enum LoadError {
    Io {
        path: PathBuf,
        err: std::io::Error,
    },
    Parse {
        path: PathBuf,
        err: ParseError,
        src: String,
    },
    Cycle {
        path: PathBuf,
    },
}

/// Load the entry file and recursively inline every local-file
/// `use` clause reachable from it. The returned `Module` is the
/// concatenation of the entry's items plus every transitively
/// imported module's items, in declaration order: entry, then
/// each `use` target depth-first.
///
/// Diamond dependencies (A imports B and C, both import D) work
/// correctly: D is parsed and inlined once, then short-circuited
/// on the second visit. True cycles (A imports B which imports
/// A back) are rejected with `LoadError::Cycle`.
pub fn load_with_imports(entry: &Path) -> Result<Module, LoadError> {
    let mut in_progress: HashSet<PathBuf> = HashSet::new();
    let mut loaded: HashSet<PathBuf> = HashSet::new();
    load_recursive(entry, &mut in_progress, &mut loaded)
}

/// Walk an already-parsed `Module`'s `use "./…"` clauses and
/// inline the referenced files in place. Useful for callers that
/// need to keep the entry's parse intact (e.g. `aeris check`,
/// which uses `parse_recovering` to collect multiple errors and
/// then layers the loader on top).
///
/// Diamond and cycle semantics match `load_with_imports`.
pub fn inline_local_imports(module: &mut Module, base_dir: &Path) -> Result<(), LoadError> {
    let mut in_progress: HashSet<PathBuf> = HashSet::new();
    let mut loaded: HashSet<PathBuf> = HashSet::new();
    inline_into(module, base_dir, &mut in_progress, &mut loaded)
}

fn inline_into(
    module: &mut Module,
    base_dir: &Path,
    in_progress: &mut HashSet<PathBuf>,
    loaded: &mut HashSet<PathBuf>,
) -> Result<(), LoadError> {
    let local_imports: Vec<String> = module
        .uses
        .iter()
        .filter_map(|u| u.source_path.clone())
        .collect();

    for rel in local_imports {
        let sub_path = base_dir.join(&rel);
        let sub_module = load_recursive(&sub_path, in_progress, loaded)?;

        for item in sub_module.items {
            module.items.push(item);
        }
        for u in sub_module.uses {
            if u.source_path.is_some() {
                continue;
            }
            if u.imported_names.is_empty() {
                continue;
            }
            let already_present = module.uses.iter().any(|existing| {
                existing.source_path.is_none() && existing.imported_names == u.imported_names
            });
            if !already_present {
                module.uses.push(u);
            }
        }
    }
    Ok(())
}

fn load_recursive(
    path: &Path,
    in_progress: &mut HashSet<PathBuf>,
    loaded: &mut HashSet<PathBuf>,
) -> Result<Module, LoadError> {
    let canonical = path.canonicalize().map_err(|err| LoadError::Io {
        path: path.to_path_buf(),
        err,
    })?;
    if in_progress.contains(&canonical) {
        return Err(LoadError::Cycle { path: canonical });
    }
    if loaded.contains(&canonical) {
        // Already inlined via another path; return an empty
        // module so the caller does not duplicate items.
        return Ok(Module {
            uses: Vec::new(),
            items: Vec::new(),
        });
    }
    in_progress.insert(canonical.clone());

    let src = fs::read_to_string(&canonical).map_err(|err| LoadError::Io {
        path: canonical.clone(),
        err,
    })?;
    let mut module = parse(&src).map_err(|err| LoadError::Parse {
        path: canonical.clone(),
        err,
        src: src.clone(),
    })?;

    let base_dir = canonical
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let local_imports: Vec<String> = module
        .uses
        .iter()
        .filter_map(|u| u.source_path.clone())
        .collect();

    for rel in local_imports {
        let sub_path = base_dir.join(&rel);
        let sub_module = load_recursive(&sub_path, in_progress, loaded)?;

        // Inline items in declaration order. `sub_module.items`
        // is empty when the sub-module was already inlined via
        // another import path (diamond case).
        for item in sub_module.items {
            module.items.push(item);
        }

        // Forward stdlib / handler `use` clauses from the sub-module
        // so the entry module sees the same names in scope. Path
        // imports were already followed above; selective re-exports
        // and version-pinned external imports do not affect runtime
        // resolution and are skipped.
        for u in sub_module.uses {
            if u.source_path.is_some() {
                continue;
            }
            if u.imported_names.is_empty() {
                continue;
            }
            let already_present = module.uses.iter().any(|existing| {
                existing.source_path.is_none() && existing.imported_names == u.imported_names
            });
            if !already_present {
                module.uses.push(u);
            }
        }
    }

    in_progress.remove(&canonical);
    loaded.insert(canonical);
    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Per-test scratch dir under `std::env::temp_dir()` with a
    /// unique suffix. The dir is cleaned up on `Drop`.
    struct Scratch {
        path: PathBuf,
    }

    impl Scratch {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "aeris_loader_{label}_{nanos}_{n}_{pid}",
                pid = std::process::id()
            ));
            std::fs::create_dir_all(&dir).unwrap();
            Self { path: dir }
        }
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn write(dir: &Path, name: &str, src: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(src.as_bytes()).unwrap();
        p
    }

    #[test]
    fn anonymous_path_import_inlines_pub_fn() {
        let dir = Scratch::new("anon");
        write(
            dir.path(),
            "lib/helpers.aer",
            r#"
                pub fn doubled(x: int) -> int { x * 2 }
            "#,
        );
        let main = write(
            dir.path(),
            "main.aer",
            r#"
                use "./lib/helpers.aer"

                fn main() -> int { doubled(21) }
            "#,
        );
        let module = load_with_imports(&main).expect("load ok");
        let fn_names: Vec<&str> = module
            .items
            .iter()
            .filter_map(|it| match it {
                crate::syntax::ast::Item::Fn(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(fn_names.contains(&"doubled"));
        assert!(fn_names.contains(&"main"));
    }

    #[test]
    fn named_path_import_also_inlines() {
        let dir = Scratch::new("named");
        write(
            dir.path(),
            "lib/utils.aer",
            r#"
                pub fn greet(name: string) -> string {
                  "Hello, " + name + "!"
                }
            "#,
        );
        let main = write(
            dir.path(),
            "main.aer",
            r#"
                use utils from "./lib/utils.aer"

                fn main() -> string { greet("Aeris") }
            "#,
        );
        let module = load_with_imports(&main).expect("load ok");
        let has_greet = module.items.iter().any(|it| {
            matches!(it, crate::syntax::ast::Item::Fn(f) if f.name == "greet")
        });
        assert!(has_greet, "greet should be inlined");
    }

    #[test]
    fn transitive_imports_are_followed() {
        let dir = Scratch::new("transitive");
        write(
            dir.path(),
            "lib/a.aer",
            r#"
                use "./b.aer"
                pub fn a_fn() -> int { b_fn() + 1 }
            "#,
        );
        write(
            dir.path(),
            "lib/b.aer",
            r#"
                pub fn b_fn() -> int { 1 }
            "#,
        );
        let main = write(
            dir.path(),
            "main.aer",
            r#"
                use "./lib/a.aer"
                fn main() -> int { a_fn() }
            "#,
        );
        let module = load_with_imports(&main).expect("load ok");
        let names: Vec<&str> = module
            .items
            .iter()
            .filter_map(|it| match it {
                crate::syntax::ast::Item::Fn(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"a_fn"));
        assert!(names.contains(&"b_fn"));
    }

    #[test]
    fn cycle_is_rejected() {
        let dir = Scratch::new("cycle");
        write(
            dir.path(),
            "lib/a.aer",
            r#"
                use "./b.aer"
                pub fn a_fn() -> int { 1 }
            "#,
        );
        write(
            dir.path(),
            "lib/b.aer",
            r#"
                use "./a.aer"
                pub fn b_fn() -> int { 1 }
            "#,
        );
        let main = write(
            dir.path(),
            "main.aer",
            r#"
                use "./lib/a.aer"
                fn main() -> int { a_fn() + b_fn() }
            "#,
        );
        let err = load_with_imports(&main).unwrap_err();
        assert!(matches!(err, LoadError::Cycle { .. }));
    }

    #[test]
    fn diamond_imports_are_loaded_once() {
        // main → A, main → B, A → D, B → D. D must be inlined
        // exactly once; the second visit short-circuits.
        let dir = Scratch::new("diamond");
        write(
            dir.path(),
            "lib/d.aer",
            r#"
                pub fn d_fn() -> int { 7 }
            "#,
        );
        write(
            dir.path(),
            "lib/a.aer",
            r#"
                use "./d.aer"
                pub fn a_fn() -> int { d_fn() + 1 }
            "#,
        );
        write(
            dir.path(),
            "lib/b.aer",
            r#"
                use "./d.aer"
                pub fn b_fn() -> int { d_fn() + 2 }
            "#,
        );
        let main = write(
            dir.path(),
            "main.aer",
            r#"
                use "./lib/a.aer"
                use "./lib/b.aer"
                fn main() -> int { a_fn() + b_fn() }
            "#,
        );
        let module = load_with_imports(&main).expect("load ok");
        let d_count = module
            .items
            .iter()
            .filter(|it| matches!(it, crate::syntax::ast::Item::Fn(f) if f.name == "d_fn"))
            .count();
        assert_eq!(d_count, 1, "d_fn should be inlined exactly once");
    }

    #[test]
    fn stdlib_uses_in_sublib_are_forwarded() {
        let dir = Scratch::new("stdlib");
        write(
            dir.path(),
            "lib/utils.aer",
            r#"
                use io
                pub fn shout(s: string) -> string { s }
            "#,
        );
        let main = write(
            dir.path(),
            "main.aer",
            r#"
                use "./lib/utils.aer"
                fn main() -> string { shout("hi") }
            "#,
        );
        let module = load_with_imports(&main).expect("load ok");
        // The `use io` from the sub-module should be visible in the
        // merged module's use list.
        let has_io = module
            .uses
            .iter()
            .any(|u| u.imported_names.iter().any(|n| n == "io"));
        assert!(has_io);
    }
}
