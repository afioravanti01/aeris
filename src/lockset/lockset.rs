//! Structured `lockset.toml` model + semantic validation (M7.T1, T4).
//!
//! Realises `docs/language.md` § 24.1. Parses the raw TOML produced
//! by `super::toml::parse`, walks the canonical sections
//! (`[project]`, `[deps]`, `[caps]`, `[ai.backend]`, `[policies]`)
//! and produces a strongly-typed `Lockset` value that the runtime
//! consumes — `main`'s synthesised cap (M7.T4 — replaces M4.T3
//! stub) is built directly from `Lockset.caps`.
//!
//! All validation failures map to **exit code 69** (§ 25.3 — lockfile
//! drift / hash mismatch / malformed pin). The CLI driver renders the
//! `LocksetError::message` and propagates that exit code.

use std::collections::BTreeMap;

use super::toml::{TomlError, TomlValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lockset {
    pub project: ProjectInfo,
    pub deps: BTreeMap<String, DepEntry>,
    pub caps: CapsCeiling,
    pub ai_backend: Option<AiBackend>,
    pub policies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectInfo {
    pub name: String,
    pub aeris: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DepSource {
    /// `path = "./lib/utils.aer"`
    LocalPath(String),
    /// `source = "github.com/acmecorp/aeris-devops", version = "1.2.0"`
    GitHub { repo: String, version: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DepEntry {
    pub alias: String,
    pub source: DepSource,
    /// `blake3:<hex>` — the spec name stays even though the runtime
    /// hashes with FNV-1a until blake3 lands. Mismatch → exit 69.
    pub hash: String,
    /// `surface_hash = "blake3:..."` — V3 effect-surface fingerprint
    /// of the dep. Optional today; populated by `aeris lock` once the
    /// dep is resolved.
    pub surface_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CapsCeiling {
    pub http_allow: Vec<String>,
    pub fs_allow_read: Vec<String>,
    pub fs_allow_write: Vec<String>,
    pub kube_contexts: Vec<String>,
    pub ai_models: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBackend {
    pub kind: String,
    pub url: Option<String>,
    pub auth: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocksetError {
    pub message: String,
}

impl std::fmt::Display for LocksetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl LocksetError {
    fn new(s: impl Into<String>) -> Self {
        Self { message: s.into() }
    }
}

impl From<TomlError> for LocksetError {
    fn from(e: TomlError) -> Self {
        LocksetError::new(format!("lockset.toml: {e}"))
    }
}

/// `aeris check` / `aeris run` exit code for any lockset-related
/// failure (§ 25.3).
pub const EXIT_LOCKSET_ERROR: u8 = 69;

impl Lockset {
    /// Compose `main`'s synthesised cap from this lockset's `[caps]`
    /// ceiling (M7.T4 — replaces the M4.T3 `cap[*]` stub).
    /// Each non-empty allow-list becomes a `(module, op, allow)`
    /// entry. The resulting `CapValue` carries `star = false` so
    /// `cap.subset[..]` narrowing checks fire normally.
    pub fn synthesise_main_cap(&self) -> crate::runtime::value::CapValue {
        use crate::runtime::value::{CapEntryValue, CapValue};
        let mut entries: Vec<CapEntryValue> = Vec::new();
        if !self.caps.http_allow.is_empty() {
            for op in ["get", "post", "put", "patch", "delete"] {
                entries.push(CapEntryValue {
                    path: vec!["http".into(), op.into()],
                    allow: Some(self.caps.http_allow.clone()),
                });
            }
        }
        if !self.caps.fs_allow_read.is_empty() {
            for op in [
                "read_file",
                "read_text",
                "read_bytes",
                "stat",
                "exists",
                "walk",
            ] {
                entries.push(CapEntryValue {
                    path: vec!["fs".into(), op.into()],
                    allow: Some(self.caps.fs_allow_read.clone()),
                });
            }
        }
        if !self.caps.fs_allow_write.is_empty() {
            for op in [
                "write_file",
                "write_text",
                "write_bytes",
                "mkdir",
                "remove",
                "rename",
            ] {
                entries.push(CapEntryValue {
                    path: vec!["fs".into(), op.into()],
                    allow: Some(self.caps.fs_allow_write.clone()),
                });
            }
        }
        if !self.caps.kube_contexts.is_empty() {
            for op in ["apply", "delete", "get", "watch"] {
                entries.push(CapEntryValue {
                    path: vec!["kube".into(), op.into()],
                    allow: Some(self.caps.kube_contexts.clone()),
                });
            }
        }
        if !self.caps.ai_models.is_empty() {
            for op in ["complete", "chat", "embed", "tools"] {
                entries.push(CapEntryValue {
                    path: vec!["ai".into(), op.into()],
                    allow: Some(self.caps.ai_models.clone()),
                });
            }
        }
        // `audit.event`, `clock.now`, `random.next`, `env.read`, and
        // diagnostic `io.*` carry no allow-list dimension; surface
        // them unconditionally so any saga writing audit events or
        // reading the clock works.
        for (m, op) in [
            ("audit", "event"),
            ("clock", "now"),
            ("random", "next"),
            ("env", "read"),
            ("io", "print"),
            ("io", "println"),
            ("io", "eprint"),
            ("io", "eprintln"),
            ("io", "read_line"),
        ] {
            entries.push(CapEntryValue {
                path: vec![m.into(), op.into()],
                allow: None,
            });
        }
        CapValue {
            entries,
            star: false,
        }
    }

    /// Human-readable description of the cap shape, printed on
    /// `aeris run` startup (§ 8.4).
    pub fn describe_main_cap(&self) -> String {
        let mut out = String::from("[aeris] effective main cap from lockset:\n");
        if !self.caps.http_allow.is_empty() {
            out.push_str(&format!("  http.* @ {:?}\n", self.caps.http_allow));
        }
        if !self.caps.fs_allow_read.is_empty() {
            out.push_str(&format!("  fs.read_* @ {:?}\n", self.caps.fs_allow_read));
        }
        if !self.caps.fs_allow_write.is_empty() {
            out.push_str(&format!("  fs.write_* @ {:?}\n", self.caps.fs_allow_write));
        }
        if !self.caps.kube_contexts.is_empty() {
            out.push_str(&format!("  kube.* @ {:?}\n", self.caps.kube_contexts));
        }
        if !self.caps.ai_models.is_empty() {
            out.push_str(&format!("  ai.* @ {:?}\n", self.caps.ai_models));
        }
        out.push_str("  audit.event, clock.now, random.next, env.read, io.*\n");
        out
    }
}

/// Verify each `path = "./..."` dep's actual file bytes hash to
/// the value pinned in `lockset.toml [deps].<alias>.hash` (M7.T2).
/// Returns the list of mismatched aliases on failure — caller maps
/// to exit code 69. `project_root` is the directory containing
/// `lockset.toml`; relative `path` deps resolve against it.
pub fn verify_local_deps(
    lockset: &Lockset,
    project_root: &std::path::Path,
) -> Result<(), Vec<LocksetError>> {
    let mut errors: Vec<LocksetError> = Vec::new();
    for (alias, dep) in &lockset.deps {
        if let DepSource::LocalPath(p) = &dep.source {
            let abs = project_root.join(p);
            let bytes = match std::fs::read(&abs) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(LocksetError::new(format!(
                        "deps.{alias}: cannot read `{}`: {e}",
                        abs.display()
                    )));
                    continue;
                }
            };
            let computed = super::surface::hash_text(&String::from_utf8_lossy(&bytes));
            if computed != dep.hash {
                errors.push(LocksetError::new(format!(
                    "deps.{alias}: hash mismatch — pinned `{}` vs actual `{computed}`",
                    dep.hash
                )));
            }
        }
        // GitHub deps (M7.T3) — fetch + cache lands when the runtime
        // gains a TLS stack; until then they are recorded but not
        // verified by `aeris lock`.
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Parse + semantically validate a `lockset.toml` source. The
/// `[project]` section is required; all others are optional.
pub fn parse_lockset(src: &str) -> Result<Lockset, LocksetError> {
    let root = super::toml::parse(src)?;
    let project = parse_project(&root)?;
    let deps = parse_deps(&root)?;
    let caps = parse_caps(&root)?;
    let ai_backend = parse_ai_backend(&root)?;
    let policies = parse_policies(&root)?;
    Ok(Lockset {
        project,
        deps,
        caps,
        ai_backend,
        policies,
    })
}

fn parse_project(root: &BTreeMap<String, TomlValue>) -> Result<ProjectInfo, LocksetError> {
    let project_table = match root.get("project") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(LocksetError::new("`[project]` must be a table")),
        None => return Err(LocksetError::new("missing required `[project]` section")),
    };
    let name = required_string(project_table, "project", "name")?;
    let aeris = required_string(project_table, "project", "aeris")?;
    Ok(ProjectInfo { name, aeris })
}

fn parse_deps(
    root: &BTreeMap<String, TomlValue>,
) -> Result<BTreeMap<String, DepEntry>, LocksetError> {
    let table = match root.get("deps") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(LocksetError::new("`[deps]` must be a table")),
        None => return Ok(BTreeMap::new()),
    };
    let mut out = BTreeMap::new();
    for (alias, raw) in table {
        let inner = match raw {
            TomlValue::Table(t) => t,
            _ => {
                return Err(LocksetError::new(format!(
                    "deps.{alias}: must be an inline table"
                )))
            }
        };
        let hash = required_string(inner, &format!("deps.{alias}"), "hash")?;
        if !hash.starts_with("blake3:") {
            return Err(LocksetError::new(format!(
                "deps.{alias}.hash: must start with `blake3:`"
            )));
        }
        let surface_hash = optional_string(inner, "surface_hash");
        let source = if let Some(p) = optional_string(inner, "path") {
            DepSource::LocalPath(p)
        } else {
            let src = required_string(inner, &format!("deps.{alias}"), "source")?;
            let ver = required_string(inner, &format!("deps.{alias}"), "version")?;
            DepSource::GitHub {
                repo: src,
                version: ver,
            }
        };
        out.insert(
            alias.clone(),
            DepEntry {
                alias: alias.clone(),
                source,
                hash,
                surface_hash,
            },
        );
    }
    Ok(out)
}

fn parse_caps(root: &BTreeMap<String, TomlValue>) -> Result<CapsCeiling, LocksetError> {
    let table = match root.get("caps") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(LocksetError::new("`[caps]` must be a table")),
        None => return Ok(CapsCeiling::default()),
    };
    let mut out = CapsCeiling::default();
    if let Some(http) = table.get("http") {
        let http = expect_table(http, "caps.http")?;
        out.http_allow = required_string_array(http, "caps.http", "allow").unwrap_or_default();
    }
    if let Some(fs) = table.get("fs") {
        let fs = expect_table(fs, "caps.fs")?;
        out.fs_allow_read = optional_string_array(fs, "allow_read");
        out.fs_allow_write = optional_string_array(fs, "allow_write");
    }
    if let Some(kube) = table.get("kube") {
        let kube = expect_table(kube, "caps.kube")?;
        out.kube_contexts = optional_string_array(kube, "contexts");
    }
    if let Some(ai) = table.get("ai") {
        let ai = expect_table(ai, "caps.ai")?;
        out.ai_models = optional_string_array(ai, "models");
    }
    Ok(out)
}

fn parse_ai_backend(root: &BTreeMap<String, TomlValue>) -> Result<Option<AiBackend>, LocksetError> {
    let ai = match root.get("ai") {
        Some(TomlValue::Table(t)) => t,
        _ => return Ok(None),
    };
    let backend = match ai.get("backend") {
        Some(TomlValue::Table(t)) => t,
        _ => return Ok(None),
    };
    let kind = required_string(backend, "ai.backend", "kind")?;
    Ok(Some(AiBackend {
        kind,
        url: optional_string(backend, "url"),
        auth: optional_string(backend, "auth"),
    }))
}

fn parse_policies(root: &BTreeMap<String, TomlValue>) -> Result<Vec<String>, LocksetError> {
    match root.get("policies") {
        Some(TomlValue::Table(t)) => Ok(optional_string_array(t, "active")),
        Some(_) => Err(LocksetError::new("`[policies]` must be a table")),
        None => Ok(Vec::new()),
    }
}

// ---- helpers ------------------------------------------------------

fn required_string(
    t: &BTreeMap<String, TomlValue>,
    section: &str,
    key: &str,
) -> Result<String, LocksetError> {
    match t.get(key) {
        Some(TomlValue::String(s)) => Ok(s.clone()),
        Some(_) => Err(LocksetError::new(format!(
            "{section}.{key}: must be a string"
        ))),
        None => Err(LocksetError::new(format!(
            "{section}: missing required key `{key}`"
        ))),
    }
}

fn optional_string(t: &BTreeMap<String, TomlValue>, key: &str) -> Option<String> {
    match t.get(key) {
        Some(TomlValue::String(s)) => Some(s.clone()),
        _ => None,
    }
}

fn required_string_array(
    t: &BTreeMap<String, TomlValue>,
    section: &str,
    key: &str,
) -> Result<Vec<String>, LocksetError> {
    match t.get(key) {
        Some(TomlValue::Array(xs)) => xs
            .iter()
            .map(|v| match v {
                TomlValue::String(s) => Ok(s.clone()),
                _ => Err(LocksetError::new(format!(
                    "{section}.{key}: array element must be a string"
                ))),
            })
            .collect(),
        Some(_) => Err(LocksetError::new(format!(
            "{section}.{key}: must be an array of strings"
        ))),
        None => Ok(Vec::new()),
    }
}

fn optional_string_array(t: &BTreeMap<String, TomlValue>, key: &str) -> Vec<String> {
    match t.get(key) {
        Some(TomlValue::Array(xs)) => xs
            .iter()
            .filter_map(|v| match v {
                TomlValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn expect_table<'a>(
    v: &'a TomlValue,
    section: &str,
) -> Result<&'a BTreeMap<String, TomlValue>, LocksetError> {
    match v {
        TomlValue::Table(t) => Ok(t),
        _ => Err(LocksetError::new(format!("{section}: must be a table"))),
    }
}

// ====================================================================
//  Tests — 20 lockset fixtures (M7.T1 acceptance)
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Lockset {
        parse_lockset(src).unwrap_or_else(|e| panic!("expected ok, got {e}"))
    }

    fn bad(src: &str) -> LocksetError {
        match parse_lockset(src) {
            Ok(_) => panic!("expected error, got ok"),
            Err(e) => e,
        }
    }

    // ---- positive fixtures (12) ----

    #[test]
    fn p01_minimum_project() {
        let l = ok(r#"
            [project]
            name  = "demo"
            aeris = "0.2.0"
        "#);
        assert_eq!(l.project.name, "demo");
        assert_eq!(l.project.aeris, "0.2.0");
        assert!(l.deps.is_empty());
    }

    #[test]
    fn p02_with_local_path_dep() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            utils = { path = "./lib/utils.aer", hash = "blake3:abcd" }
        "#);
        let dep = l.deps.get("utils").unwrap();
        assert_eq!(dep.source, DepSource::LocalPath("./lib/utils.aer".into()));
        assert_eq!(dep.hash, "blake3:abcd");
    }

    #[test]
    fn p03_with_github_dep() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            deploy = { source = "github.com/acmecorp/aeris-devops", version = "1.2.0", hash = "blake3:7e2c" }
        "#);
        let dep = l.deps.get("deploy").unwrap();
        match &dep.source {
            DepSource::GitHub { repo, version } => {
                assert_eq!(repo, "github.com/acmecorp/aeris-devops");
                assert_eq!(version, "1.2.0");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn p04_caps_http_allow() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            http.allow = ["api.acme.com", "api.stripe.com"]
        "#);
        assert_eq!(
            l.caps.http_allow,
            vec!["api.acme.com".to_string(), "api.stripe.com".to_string()]
        );
    }

    #[test]
    fn p05_caps_fs_allow_read_and_write() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            fs.allow_read  = ["/etc/aeris/**", "./data/**"]
            fs.allow_write = ["./out/**"]
        "#);
        assert_eq!(l.caps.fs_allow_read.len(), 2);
        assert_eq!(l.caps.fs_allow_write, vec!["./out/**".to_string()]);
    }

    #[test]
    fn p06_caps_kube_and_ai() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            kube.contexts = ["prod-eu-1"]
            ai.models     = ["claude-opus-4-7"]
        "#);
        assert_eq!(l.caps.kube_contexts, vec!["prod-eu-1".to_string()]);
        assert_eq!(l.caps.ai_models, vec!["claude-opus-4-7".to_string()]);
    }

    #[test]
    fn p07_ai_backend_http() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [ai.backend]
            kind = "http"
            url  = "https://api.anthropic.com"
            auth = "env:ANTHROPIC_API_KEY"
        "#);
        let b = l.ai_backend.unwrap();
        assert_eq!(b.kind, "http");
        assert_eq!(b.url.as_deref(), Some("https://api.anthropic.com"));
    }

    #[test]
    fn p08_policies_active_list() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [policies]
            active = ["a", "b", "c"]
        "#);
        assert_eq!(
            l.policies,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn p09_full_canonical_lockset() {
        // The exact shape from `language.md` § 24.1.
        let l = ok(r#"
            [project]
            name   = "settle-pipeline"
            aeris  = "0.2.0"

            [deps]
            deploy = { source = "github.com/acmecorp/aeris-devops", version = "1.2.0", hash = "blake3:7e2c" }
            utils  = { path   = "./lib/utils.aer", hash = "blake3:9b18" }

            [caps]
            http.allow      = ["api.acme.com", "api.stripe.com"]
            fs.allow_read   = ["/etc/aeris/**", "./data/**"]
            fs.allow_write  = ["./out/**", "./.aeris/**"]
            kube.contexts   = ["prod-eu-1"]
            ai.models       = ["claude-opus-4-7", "claude-haiku-4-5"]

            [ai.backend]
            kind = "http"
            url  = "https://api.anthropic.com"
            auth = "env:ANTHROPIC_API_KEY"

            [policies]
            active = ["production_egress", "model_budget"]
        "#);
        assert_eq!(l.deps.len(), 2);
        assert_eq!(l.caps.http_allow.len(), 2);
        assert_eq!(l.caps.fs_allow_read.len(), 2);
        assert_eq!(l.caps.ai_models.len(), 2);
    }

    #[test]
    fn p10_dep_with_surface_hash() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            u = { path = "./u.aer", hash = "blake3:00", surface_hash = "blake3:11" }
        "#);
        assert_eq!(
            l.deps.get("u").unwrap().surface_hash,
            Some("blake3:11".into())
        );
    }

    #[test]
    fn p11_no_deps_no_caps_no_policies() {
        let l = ok(r#"
            [project]
            name  = "barebones"
            aeris = "0.2.0"
        "#);
        assert!(l.deps.is_empty());
        assert!(l.caps.http_allow.is_empty());
        assert!(l.policies.is_empty());
        assert!(l.ai_backend.is_none());
    }

    #[test]
    fn p12_comments_inline_and_block() {
        let l = ok(r#"
            # top-level
            [project]
            name  = "x"   # inline
            aeris = "0.2.0"
            # trailing
        "#);
        assert_eq!(l.project.name, "x");
    }

    // ---- negative fixtures (8) — every error is exit-69 worthy ----

    #[test]
    fn n01_missing_project_section() {
        let e = bad(r#"[caps]
            http.allow = ["x"]
        "#);
        assert!(e.message.contains("missing required `[project]`"));
    }

    #[test]
    fn n02_project_missing_name() {
        let e = bad(r#"
            [project]
            aeris = "0.2.0"
        "#);
        assert!(e.message.contains("missing required key `name`"));
    }

    #[test]
    fn n03_project_missing_aeris() {
        let e = bad(r#"
            [project]
            name = "x"
        "#);
        assert!(e.message.contains("missing required key `aeris`"));
    }

    #[test]
    fn n04_dep_missing_hash() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            u = { path = "./u.aer" }
        "#);
        assert!(e.message.contains("missing required key `hash`"));
    }

    #[test]
    fn n05_dep_hash_wrong_prefix() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            u = { path = "./u.aer", hash = "sha256:abc" }
        "#);
        assert!(e.message.contains("must start with `blake3:`"));
    }

    #[test]
    fn n06_github_dep_missing_version() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            d = { source = "github.com/x/y", hash = "blake3:00" }
        "#);
        assert!(e.message.contains("missing required key `version`"));
    }

    #[test]
    fn n07_malformed_toml_unterminated_string() {
        let e = bad("[project]\nname = \"x\nun");
        assert!(e.message.contains("lockset.toml:") || e.message.contains("string"));
    }

    // ---- M7.T2 — local path dep hashing ----

    #[test]
    fn m7t2_local_path_hash_match() {
        let dir = std::env::temp_dir().join(format!("aeris-m7t2-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dep_path = dir.join("u.aer");
        std::fs::write(&dep_path, "fn x() {}").unwrap();
        let computed = super::super::surface::hash_text("fn x() {}");
        let toml_src = format!(
            r#"
                [project]
                name  = "x"
                aeris = "0.2.0"

                [deps]
                u = {{ path = "u.aer", hash = "{computed}" }}
            "#
        );
        let lockset = parse_lockset(&toml_src).unwrap();
        let r = verify_local_deps(&lockset, &dir);
        assert!(r.is_ok(), "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m7t2_local_path_hash_mismatch() {
        let dir = std::env::temp_dir().join(format!("aeris-m7t2-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("u.aer"), "fn x() {}").unwrap();
        let toml_src = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            u = { path = "u.aer", hash = "blake3:0000000000000000" }
        "#;
        let lockset = parse_lockset(toml_src).unwrap();
        let errs = verify_local_deps(&lockset, &dir).unwrap_err();
        assert!(errs[0].message.contains("hash mismatch"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m7t2_local_path_missing_file_errors() {
        let dir = std::env::temp_dir().join(format!("aeris-m7t2-miss-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let toml_src = r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [deps]
            u = { path = "missing.aer", hash = "blake3:00" }
        "#;
        let lockset = parse_lockset(toml_src).unwrap();
        let errs = verify_local_deps(&lockset, &dir).unwrap_err();
        assert!(errs[0].message.contains("cannot read"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- M7.T4 — main cap composition from lockset ----

    #[test]
    fn m7t4_synthesise_cap_from_http_allow() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            http.allow = ["api.acme.com"]
        "#);
        let cap = l.synthesise_main_cap();
        assert!(!cap.star);
        // 5 http verbs × 1 allow + auxiliaries (audit, clock, random, env, io.* = 5+9 = 14)
        assert!(cap.entries.iter().any(|e| {
            e.path == vec!["http".to_string(), "post".to_string()]
                && e.allow
                    .as_ref()
                    .is_some_and(|a| a.contains(&"api.acme.com".to_string()))
        }));
    }

    #[test]
    fn m7t4_describe_main_cap_renders_human_form() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [caps]
            http.allow = ["api.acme.com"]
            ai.models  = ["claude-opus-4-7"]
        "#);
        let s = l.describe_main_cap();
        assert!(s.contains("http.* @"));
        assert!(s.contains("ai.* @"));
        assert!(s.contains("audit.event"));
    }

    #[test]
    fn n08_ai_backend_missing_kind() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [ai.backend]
            url = "x"
        "#);
        assert!(e.message.contains("missing required key `kind`"));
    }
}
