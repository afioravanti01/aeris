//! Structured `aeris.toml` model + semantic validation (M7.T1, T4).
//!
//! Realises `docs/language.md` § 24.1. Parses the raw TOML produced
//! by `super::toml::parse`, walks the canonical sections
//! (`[project]`, `[deps]`, `[caps]`, `[ai.backend]`, `[policies]`)
//! and produces a strongly-typed `Manifest` value that the runtime
//! consumes — `main`'s synthesised cap (M7.T4 — replaces M4.T3
//! stub) is built directly from `Manifest.caps`.
//!
//! All validation failures map to **exit code 69** (§ 25.3 — lockfile
//! drift / hash mismatch / malformed pin). The CLI driver renders the
//! `ManifestError::message` and propagates that exit code.

use std::collections::BTreeMap;

use super::toml::{TomlError, TomlValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub project: ProjectInfo,
    pub deps: BTreeMap<String, DepEntry>,
    pub caps: CapsCeiling,
    pub ai_backend: Option<AiBackend>,
    pub policies: Vec<String>,
    /// M22.T2 — per-family L2 backend selection + connection
    /// settings. Default per-family is `Mock` so projects that
    /// don't opt in keep the historical no-network behaviour.
    pub l2_backends: L2BackendsConfig,
    /// M41 — runtime output configuration. Controls where the
    /// JSONL trace, the audit log and the surface lock are
    /// written. Default: `.aeris/` next to the project root,
    /// trace on.
    pub runtime: RuntimeConfig,
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

/// § 8.4.1 — capability enforcement mode. Three levels, finest to
/// coarsest discipline:
///
/// * `Strict` — every effectful function must declare an enclosing
///   `cap` parameter; the runtime allow-list from the manifest is
///   enforced on every cap call; `intent` is mandatory on writes;
///   sagas need explicit `undo` on every write step.
///
/// * `Loose` — body-resolution `NoCapInScope` (E65) is suppressed
///   for functions that omit `cap`. Functions that declare `cap` are
///   still checked normally. The runtime allow-list (§ 8.3.1, N4)
///   stays in force. `intent` and saga `undo` remain mandatory.
///
/// * `Off` — script-friendly mode. The whole cap discipline is
///   relaxed: no `cap` parameters needed, no runtime allow-list,
///   no E65/E66/E67/E71. The trace, replay, models and policies
///   stay available but become voluntary annotations. Intended for
///   single-author scripts and prototypes where audit is not a
///   concern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnforceMode {
    Off,
    Loose,
    Strict,
}

impl EnforceMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "off" => Some(Self::Off),
            "loose" => Some(Self::Loose),
            "strict" => Some(Self::Strict),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapsCeiling {
    /// § 8.4.1 — capability enforcement mode.
    pub enforce: EnforceMode,
    pub http_allow: Vec<String>,
    pub fs_allow_read: Vec<String>,
    pub fs_allow_write: Vec<String>,
    pub kube_contexts: Vec<String>,
    pub ai_models: Vec<String>,
}

impl CapsCeiling {
    /// Back-compat alias: returns `true` when the mode is `Strict`.
    /// Existing call sites that thought of the bool flag can still
    /// read it without caring about the new third value.
    pub fn required(&self) -> bool {
        matches!(self.enforce, EnforceMode::Strict)
    }
}

impl Default for CapsCeiling {
    fn default() -> Self {
        // Crate-internal default: strict mode preserves the original
        // M0–M14 behaviour for tests and consumers that build a
        // `CapsCeiling` directly. The user-facing `aeris init`
        // template is opinionated towards `enforce = "off"` (scripts).
        Self {
            enforce: EnforceMode::Strict,
            http_allow: Vec::new(),
            fs_allow_read: Vec::new(),
            fs_allow_write: Vec::new(),
            kube_contexts: Vec::new(),
            ai_models: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiBackend {
    pub kind: String,
    pub url: Option<String>,
    pub auth: Option<String>,
    /// Shell command (M9.T1 `cli` kind). Split on whitespace to form
    /// argv; the prompt is piped to stdin and stdout becomes the
    /// completion text. `Some("python tools/chat.py")` is a typical
    /// shape; only relevant when `kind = "cli"`.
    pub cmd: Option<String>,
}

// ---- L2 backend configuration (M22.T2) -----------------------------

/// Selection of the L2 backend for a given family.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BackendKind {
    /// Drive the SDK / subprocess / FS-backed impl that performs
    /// real I/O. Default when no `[l2.<module>]` table is present
    /// (M22 + flip-to-real follow-up). Per-family `*BackendConfig`
    /// fields carry the connection settings the implementation
    /// needs.
    #[default]
    Real,
    /// Trace-only stub: enforce caps, emit the `<module>_<op>`
    /// event, return `Ok(())` without contacting anything.
    /// Demos / tests that don't have the live service at hand can
    /// opt back into this with `backend = "mock"`.
    Mock,
    /// Drain answers from the active replay tape (M9). Useful for
    /// `aeris replay` against a previously-recorded `Real` run.
    Replay,
}

impl BackendKind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "mock" => Some(Self::Mock),
            "real" => Some(Self::Real),
            "replay" => Some(Self::Replay),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MinioBackendConfig {
    pub backend: BackendKind,
    pub endpoint: Option<String>,
    pub region: Option<String>,
    pub access_key_env: Option<String>,
    pub secret_key_env: Option<String>,
    pub path_style: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MongoBackendConfig {
    pub backend: BackendKind,
    pub uri: Option<String>,
    pub auth_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DockerBackendConfig {
    pub backend: BackendKind,
    /// Defaults to `unix:///var/run/docker.sock` on Unix in the real
    /// backend; surfaced here so a project can override it (e.g.
    /// `tcp://docker.internal:2375`).
    pub host: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KubeBackendConfig {
    pub backend: BackendKind,
    pub kubeconfig: Option<String>,
    pub context: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RabbitBackendConfig {
    pub backend: BackendKind,
    pub uri: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuditBackendConfig {
    pub backend: BackendKind,
    /// Override the mock backend's audit-log path. `None` means
    /// the runtime's default location (`.aeris/audit.log`).
    pub path: Option<String>,
}

// ---- runtime configuration (M41) -----------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeConfig {
    /// Directory where the runtime writes its outputs (trace JSONL,
    /// audit log, surface lock). Relative paths are resolved
    /// against the project root, not the process cwd. Default:
    /// `.aeris`.
    pub output_dir: String,
    /// When `true` (default), every run opens
    /// `<output_dir>/traces/<trace_id>.jsonl` and the JSONL tracer
    /// is plumbed through the runtime. `false` disables disk
    /// tracing (the in-memory channel is still available to tests).
    pub trace: bool,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            output_dir: ".aeris".to_string(),
            trace: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct L2BackendsConfig {
    pub audit: AuditBackendConfig,
    pub kube: KubeBackendConfig,
    pub docker: DockerBackendConfig,
    pub mongodb: MongoBackendConfig,
    pub minio: MinioBackendConfig,
    pub rabbitmq: RabbitBackendConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    pub message: String,
}

impl std::fmt::Display for ManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl ManifestError {
    fn new(s: impl Into<String>) -> Self {
        Self { message: s.into() }
    }
}

impl From<TomlError> for ManifestError {
    fn from(e: TomlError) -> Self {
        ManifestError::new(format!("aeris.toml: {e}"))
    }
}

/// `aeris check` / `aeris run` exit code for any manifest-related
/// failure (§ 25.3).
pub const EXIT_MANIFEST_ERROR: u8 = 69;

impl Manifest {
    /// Compose `main`'s synthesised cap from this manifest's `[caps]`
    /// ceiling (M7.T4 — replaces the M4.T3 `cap[*]` stub).
    /// Each non-empty allow-list becomes a `(module, op, allow)`
    /// entry. The resulting `CapValue` carries `star = false` so
    /// `cap.subset[..]` narrowing checks fire normally.
    pub fn synthesise_main_cap(&self) -> crate::runtime::value::CapValue {
        use crate::runtime::value::{CapEntryValue, CapValue};
        // `enforce = "off"` → cap[*]. Every runtime allow-list check
        // (`enforce_*_policy`) short-circuits on `star = true`, so the
        // script can reach any host, path, or model. The trace still
        // records the call; only the gate disappears.
        if matches!(self.caps.enforce, EnforceMode::Off) {
            return CapValue {
                entries: Vec::new(),
                star: true,
            };
        }
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
            // The four canonical L2 cap leaves (§ 8.1) plus the M19
            // / M28 higher-level builtins that the static checker
            // treats as distinct cap paths (`src/check/effects.rs`).
            // Every alias delegates internally to `ai.complete` for
            // authority, but the static-check / `cap.subset[..]` view
            // sees them as separate entries — so the manifest must
            // synthesise an entry per alias too, otherwise
            // `cap.subset[ai.decide @ [...]]` cannot find a match in
            // the parent.
            for op in [
                "complete",
                "chat",
                "embed",
                "tools",
                "session",
                "session_ask",
                "decide",
                "usage",
                "network",
            ] {
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
        if matches!(self.caps.enforce, EnforceMode::Off) {
            return String::from(
                "[aeris] effective main cap: cap[*]  (enforce = \"off\" — no runtime allow-list)\n",
            );
        }
        let mut out = String::from("[aeris] effective main cap from manifest:\n");
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
/// the value pinned in `aeris.toml [deps].<alias>.hash` (M7.T2).
/// Returns the list of mismatched aliases on failure — caller maps
/// to exit code 69. `project_root` is the directory containing
/// `aeris.toml`; relative `path` deps resolve against it.
pub fn verify_local_deps(
    manifest: &Manifest,
    project_root: &std::path::Path,
) -> Result<(), Vec<ManifestError>> {
    let mut errors: Vec<ManifestError> = Vec::new();
    for (alias, dep) in &manifest.deps {
        if let DepSource::LocalPath(p) = &dep.source {
            let abs = project_root.join(p);
            let bytes = match std::fs::read(&abs) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(ManifestError::new(format!(
                        "deps.{alias}: cannot read `{}`: {e}",
                        abs.display()
                    )));
                    continue;
                }
            };
            let text = String::from_utf8_lossy(&bytes);
            let computed = super::surface::hash_text(&text);
            if computed != dep.hash {
                errors.push(ManifestError::new(format!(
                    "deps.{alias}: hash mismatch — pinned `{}` vs actual `{computed}`",
                    dep.hash
                )));
            }
            // M7.T6 — when the dep also pins a `surface_hash`, recompute
            // the V3 effect-surface fingerprint and fail on drift so a
            // dep upgrade that broadens the surface forces a lockfile
            // diff (the gating signal of `language.md` § 24.3 / § 8.6).
            if let Some(pinned) = &dep.surface_hash {
                match super::surface::compute_dep_surface_hash(&text) {
                    Ok(actual) => {
                        if &actual != pinned {
                            errors.push(ManifestError::new(format!(
                                "deps.{alias}: surface_hash mismatch — pinned `{pinned}` vs actual `{actual}` (run `aeris lock` to refresh)"
                            )));
                        }
                    }
                    Err(e) => {
                        errors.push(ManifestError::new(format!(
                            "deps.{alias}: cannot compute surface_hash: {e}"
                        )));
                    }
                }
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

/// Parse + semantically validate a `aeris.toml` source. The
/// `[project]` section is required; all others are optional.
pub fn parse_manifest(src: &str) -> Result<Manifest, ManifestError> {
    let root = super::toml::parse(src)?;
    let project = parse_project(&root)?;
    let deps = parse_deps(&root)?;
    let caps = parse_caps(&root)?;
    let ai_backend = parse_ai_backend(&root)?;
    let policies = parse_policies(&root)?;
    let l2_backends = parse_l2_backends(&root)?;
    let runtime = parse_runtime(&root)?;
    Ok(Manifest {
        project,
        deps,
        caps,
        ai_backend,
        policies,
        l2_backends,
        runtime,
    })
}

const RUNTIME_KEYS: &[&str] = &["output_dir", "trace"];

fn parse_runtime(
    root: &BTreeMap<String, TomlValue>,
) -> Result<RuntimeConfig, ManifestError> {
    let table = match root.get("runtime") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(ManifestError::new("`[runtime]` must be a table")),
        None => return Ok(RuntimeConfig::default()),
    };
    reject_unknown_keys("runtime", table, RUNTIME_KEYS)?;
    let mut out = RuntimeConfig::default();
    if let Some(s) = optional_string(table, "output_dir") {
        if s.is_empty() {
            return Err(ManifestError::new(
                "runtime.output_dir: cannot be empty",
            ));
        }
        out.output_dir = s;
    }
    if let Some(b) = optional_bool(table, "runtime", "trace")? {
        out.trace = b;
    }
    Ok(out)
}

fn parse_project(root: &BTreeMap<String, TomlValue>) -> Result<ProjectInfo, ManifestError> {
    let project_table = match root.get("project") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(ManifestError::new("`[project]` must be a table")),
        None => return Err(ManifestError::new("missing required `[project]` section")),
    };
    let name = required_string(project_table, "project", "name")?;
    let aeris = required_string(project_table, "project", "aeris")?;
    Ok(ProjectInfo { name, aeris })
}

fn parse_deps(
    root: &BTreeMap<String, TomlValue>,
) -> Result<BTreeMap<String, DepEntry>, ManifestError> {
    let table = match root.get("deps") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(ManifestError::new("`[deps]` must be a table")),
        None => return Ok(BTreeMap::new()),
    };
    let mut out = BTreeMap::new();
    for (alias, raw) in table {
        let inner = match raw {
            TomlValue::Table(t) => t,
            _ => {
                return Err(ManifestError::new(format!(
                    "deps.{alias}: must be an inline table"
                )))
            }
        };
        let hash = required_string(inner, &format!("deps.{alias}"), "hash")?;
        if !hash.starts_with("blake3:") {
            return Err(ManifestError::new(format!(
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

fn parse_caps(root: &BTreeMap<String, TomlValue>) -> Result<CapsCeiling, ManifestError> {
    let table = match root.get("caps") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(ManifestError::new("`[caps]` must be a table")),
        // No `[caps]` block at all: preserve M0–M14 strict behaviour
        // (an absent manifest already produced strict checking).
        None => return Ok(CapsCeiling::default()),
    };
    let mut out = CapsCeiling::default();
    // New form: `enforce = "off" | "loose" | "strict"`.
    if let Some(v) = table.get("enforce") {
        match v {
            TomlValue::String(s) => {
                out.enforce = EnforceMode::parse(s).ok_or_else(|| {
                    ManifestError::new(format!(
                        "caps.enforce: must be one of \"off\", \"loose\", \"strict\" (got `{s}`)"
                    ))
                })?;
            }
            _ => return Err(ManifestError::new("caps.enforce: must be a string")),
        }
    }
    // Back-compat: `required = true|false` maps onto `enforce`. If
    // both are present, the explicit `enforce` wins (last one wins
    // in iteration order, but here `enforce` is checked first and
    // `required` only fires if `enforce` did not).
    if let Some(v) = table.get("required") {
        match v {
            TomlValue::Bool(true) => out.enforce = EnforceMode::Strict,
            TomlValue::Bool(false) => out.enforce = EnforceMode::Loose,
            _ => return Err(ManifestError::new("caps.required: must be a boolean")),
        }
    }
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

fn parse_ai_backend(root: &BTreeMap<String, TomlValue>) -> Result<Option<AiBackend>, ManifestError> {
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
        cmd: optional_string(backend, "cmd"),
    }))
}

fn parse_policies(root: &BTreeMap<String, TomlValue>) -> Result<Vec<String>, ManifestError> {
    match root.get("policies") {
        Some(TomlValue::Table(t)) => Ok(optional_string_array(t, "active")),
        Some(_) => Err(ManifestError::new("`[policies]` must be a table")),
        None => Ok(Vec::new()),
    }
}

// ---- L2 backend parsing (M22.T2) -----------------------------------

const L2_MINIO_KEYS: &[&str] = &[
    "backend",
    "endpoint",
    "region",
    "access_key_env",
    "secret_key_env",
    "path_style",
];
const L2_MONGODB_KEYS: &[&str] = &["backend", "uri", "auth_source"];
const L2_DOCKER_KEYS: &[&str] = &["backend", "host"];
const L2_KUBE_KEYS: &[&str] = &["backend", "kubeconfig", "context"];
const L2_RABBITMQ_KEYS: &[&str] = &["backend", "uri"];
const L2_AUDIT_KEYS: &[&str] = &["backend", "path"];

fn parse_l2_backends(
    root: &BTreeMap<String, TomlValue>,
) -> Result<L2BackendsConfig, ManifestError> {
    let l2_table = match root.get("l2") {
        Some(TomlValue::Table(t)) => t,
        Some(_) => return Err(ManifestError::new("`[l2]` must be a table")),
        None => return Ok(L2BackendsConfig::default()),
    };
    let mut out = L2BackendsConfig::default();
    if let Some(t) = expect_optional_table(l2_table, "l2.minio")? {
        reject_unknown_keys("l2.minio", t, L2_MINIO_KEYS)?;
        out.minio = MinioBackendConfig {
            backend: parse_backend_kind(t, "l2.minio")?,
            endpoint: optional_string(t, "endpoint"),
            region: optional_string(t, "region"),
            access_key_env: optional_string(t, "access_key_env"),
            secret_key_env: optional_string(t, "secret_key_env"),
            path_style: optional_bool(t, "l2.minio", "path_style")?,
        };
    }
    if let Some(t) = expect_optional_table(l2_table, "l2.mongodb")? {
        reject_unknown_keys("l2.mongodb", t, L2_MONGODB_KEYS)?;
        out.mongodb = MongoBackendConfig {
            backend: parse_backend_kind(t, "l2.mongodb")?,
            uri: optional_string(t, "uri"),
            auth_source: optional_string(t, "auth_source"),
        };
    }
    if let Some(t) = expect_optional_table(l2_table, "l2.docker")? {
        reject_unknown_keys("l2.docker", t, L2_DOCKER_KEYS)?;
        out.docker = DockerBackendConfig {
            backend: parse_backend_kind(t, "l2.docker")?,
            host: optional_string(t, "host"),
        };
    }
    if let Some(t) = expect_optional_table(l2_table, "l2.kube")? {
        reject_unknown_keys("l2.kube", t, L2_KUBE_KEYS)?;
        out.kube = KubeBackendConfig {
            backend: parse_backend_kind(t, "l2.kube")?,
            kubeconfig: optional_string(t, "kubeconfig"),
            context: optional_string(t, "context"),
        };
    }
    if let Some(t) = expect_optional_table(l2_table, "l2.rabbitmq")? {
        reject_unknown_keys("l2.rabbitmq", t, L2_RABBITMQ_KEYS)?;
        out.rabbitmq = RabbitBackendConfig {
            backend: parse_backend_kind(t, "l2.rabbitmq")?,
            uri: optional_string(t, "uri"),
        };
    }
    if let Some(t) = expect_optional_table(l2_table, "l2.audit")? {
        reject_unknown_keys("l2.audit", t, L2_AUDIT_KEYS)?;
        out.audit = AuditBackendConfig {
            backend: parse_backend_kind(t, "l2.audit")?,
            path: optional_string(t, "path"),
        };
    }
    // Reject `[l2.<anything-else>]` so typos don't silently default
    // to mock. The known set is the six families above.
    let known: &[&str] = &["minio", "mongodb", "docker", "kube", "rabbitmq", "audit"];
    for key in l2_table.keys() {
        if !known.contains(&key.as_str()) {
            return Err(ManifestError::new(format!(
                "unknown L2 family `[l2.{key}]` — known families: {}",
                known.join(", ")
            )));
        }
    }
    Ok(out)
}

fn parse_backend_kind(
    t: &BTreeMap<String, TomlValue>,
    section: &str,
) -> Result<BackendKind, ManifestError> {
    let s = match t.get("backend") {
        Some(TomlValue::String(s)) => s.as_str(),
        Some(_) => {
            return Err(ManifestError::new(format!(
                "{section}.backend: must be a string"
            )))
        }
        None => return Ok(BackendKind::Mock),
    };
    BackendKind::parse(s).ok_or_else(|| {
        ManifestError::new(format!(
            "{section}.backend: must be one of \"mock\", \"real\", \"replay\" — got `{s}`"
        ))
    })
}

fn expect_optional_table<'a>(
    parent: &'a BTreeMap<String, TomlValue>,
    section: &str,
) -> Result<Option<&'a BTreeMap<String, TomlValue>>, ManifestError> {
    let leaf = section.rsplit('.').next().unwrap_or(section);
    match parent.get(leaf) {
        Some(TomlValue::Table(t)) => Ok(Some(t)),
        Some(_) => Err(ManifestError::new(format!(
            "`[{section}]` must be a table"
        ))),
        None => Ok(None),
    }
}

fn reject_unknown_keys(
    section: &str,
    t: &BTreeMap<String, TomlValue>,
    allowed: &[&str],
) -> Result<(), ManifestError> {
    for key in t.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(ManifestError::new(format!(
                "{section}: unknown key `{key}` — allowed: {}",
                allowed.join(", ")
            )));
        }
    }
    Ok(())
}

fn optional_bool(
    t: &BTreeMap<String, TomlValue>,
    section: &str,
    key: &str,
) -> Result<Option<bool>, ManifestError> {
    match t.get(key) {
        Some(TomlValue::Bool(b)) => Ok(Some(*b)),
        Some(_) => Err(ManifestError::new(format!(
            "{section}.{key}: must be a boolean"
        ))),
        None => Ok(None),
    }
}

// ---- helpers ------------------------------------------------------

fn required_string(
    t: &BTreeMap<String, TomlValue>,
    section: &str,
    key: &str,
) -> Result<String, ManifestError> {
    match t.get(key) {
        Some(TomlValue::String(s)) => Ok(s.clone()),
        Some(_) => Err(ManifestError::new(format!(
            "{section}.{key}: must be a string"
        ))),
        None => Err(ManifestError::new(format!(
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
) -> Result<Vec<String>, ManifestError> {
    match t.get(key) {
        Some(TomlValue::Array(xs)) => xs
            .iter()
            .map(|v| match v {
                TomlValue::String(s) => Ok(s.clone()),
                _ => Err(ManifestError::new(format!(
                    "{section}.{key}: array element must be a string"
                ))),
            })
            .collect(),
        Some(_) => Err(ManifestError::new(format!(
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
) -> Result<&'a BTreeMap<String, TomlValue>, ManifestError> {
    match v {
        TomlValue::Table(t) => Ok(t),
        _ => Err(ManifestError::new(format!("{section}: must be a table"))),
    }
}

// ====================================================================
//  Tests — 20 manifest fixtures (M7.T1 acceptance)
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> Manifest {
        parse_manifest(src).unwrap_or_else(|e| panic!("expected ok, got {e}"))
    }

    fn bad(src: &str) -> ManifestError {
        match parse_manifest(src) {
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
    fn m9t1_ai_backend_cli_cmd_parses() {
        // M9.T1 — the `cli` backend is selected by `kind = "cli"` and
        // takes an inline `cmd` whose tokens become argv[0..]. Both
        // url and auth are optional in this mode.
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.2.0"

            [ai.backend]
            kind = "cli"
            cmd  = "python tools/chat.py --model haiku"
        "#);
        let b = l.ai_backend.unwrap();
        assert_eq!(b.kind, "cli");
        assert_eq!(b.cmd.as_deref(), Some("python tools/chat.py --model haiku"));
        assert!(b.url.is_none());
        assert!(b.auth.is_none());
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
    fn p09_full_canonical_manifest() {
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
        assert!(e.message.contains("aeris.toml:") || e.message.contains("string"));
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
        let manifest = parse_manifest(&toml_src).unwrap();
        let r = verify_local_deps(&manifest, &dir);
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
        let manifest = parse_manifest(toml_src).unwrap();
        let errs = verify_local_deps(&manifest, &dir).unwrap_err();
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
        let manifest = parse_manifest(toml_src).unwrap();
        let errs = verify_local_deps(&manifest, &dir).unwrap_err();
        assert!(errs[0].message.contains("cannot read"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- M7.T6 — surface_hash for deps ----

    #[test]
    fn m7t6_dep_surface_hash_match_passes() {
        let dir = std::env::temp_dir().join(format!("aeris-m7t6-ok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = "pub fn f() -> int { 1 }\n";
        std::fs::write(dir.join("u.aer"), src).unwrap();
        let content_hash = super::super::surface::hash_text(src);
        let surface_hash = super::super::surface::compute_dep_surface_hash(src).unwrap();
        let toml_src = format!(
            r#"
                [project]
                name  = "x"
                aeris = "0.2.0"

                [deps]
                u = {{ path = "u.aer", hash = "{content_hash}", surface_hash = "{surface_hash}" }}
            "#
        );
        let manifest = parse_manifest(&toml_src).unwrap();
        let r = verify_local_deps(&manifest, &dir);
        assert!(r.is_ok(), "{r:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m7t6_dep_surface_hash_mismatch_is_manifest_error() {
        let dir = std::env::temp_dir().join(format!("aeris-m7t6-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = "pub fn f() -> int { 1 }\n";
        std::fs::write(dir.join("u.aer"), src).unwrap();
        let content_hash = super::super::surface::hash_text(src);
        // Stale surface_hash — does NOT match the dep's actual surface.
        let stale = "blake3:deadbeefdeadbeef";
        let toml_src = format!(
            r#"
                [project]
                name  = "x"
                aeris = "0.2.0"

                [deps]
                u = {{ path = "u.aer", hash = "{content_hash}", surface_hash = "{stale}" }}
            "#
        );
        let manifest = parse_manifest(&toml_src).unwrap();
        let errs = verify_local_deps(&manifest, &dir).unwrap_err();
        assert!(
            errs.iter().any(|e| e.message.contains("surface_hash mismatch")),
            "expected surface_hash error, got: {errs:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn m7t6_dep_upgrade_broadening_surface_forces_relock() {
        // Simulate a dep upgrade: V1 has no effects, V2 reaches into
        // `fs.read_text`. After the upgrade the content hash is dutifully
        // refreshed by `aeris lock`, but `surface_hash` is left stale —
        // verify_local_deps must surface the broadening as the gating
        // diff so review notices the new effect.
        let dir = std::env::temp_dir().join(format!("aeris-m7t6-upgrade-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let v1 = "pub fn f() -> int { 1 }\n";
        let v2 = "pub fn f(c: cap[fs.read_text]) -> result<string> { fs.read_text(\"/etc/host\") }\n";
        // Pin V1's surface_hash.
        let v1_surface = super::super::surface::compute_dep_surface_hash(v1).unwrap();
        let v2_surface = super::super::surface::compute_dep_surface_hash(v2).unwrap();
        assert_ne!(v1_surface, v2_surface, "V2 must broaden V1's surface");
        // The user runs `aeris lock` and updates the content hash to V2
        // but forgets to refresh `surface_hash` (left at V1).
        std::fs::write(dir.join("u.aer"), v2).unwrap();
        let v2_content = super::super::surface::hash_text(v2);
        let toml_src = format!(
            r#"
                [project]
                name  = "x"
                aeris = "0.2.0"

                [deps]
                u = {{ path = "u.aer", hash = "{v2_content}", surface_hash = "{v1_surface}" }}
            "#
        );
        let manifest = parse_manifest(&toml_src).unwrap();
        let errs = verify_local_deps(&manifest, &dir).unwrap_err();
        assert!(
            errs.iter().any(|e| e.message.contains("surface_hash mismatch")),
            "expected surface_hash drift, got: {errs:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- M7.T4 — main cap composition from manifest ----

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

    // ---- M22.T2 — L2 backend selection ----

    #[test]
    fn manifest_defaults_l2_backend_to_real() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"
        "#);
        assert_eq!(l.l2_backends.minio.backend, BackendKind::Real);
        assert_eq!(l.l2_backends.mongodb.backend, BackendKind::Real);
        assert_eq!(l.l2_backends.docker.backend, BackendKind::Real);
        assert_eq!(l.l2_backends.kube.backend, BackendKind::Real);
        assert_eq!(l.l2_backends.rabbitmq.backend, BackendKind::Real);
        assert_eq!(l.l2_backends.audit.backend, BackendKind::Real);
    }

    #[test]
    fn manifest_parses_explicit_mock_backend() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [l2.kube]
            backend = "mock"
        "#);
        assert_eq!(l.l2_backends.kube.backend, BackendKind::Mock);
        // Other families still default to Real.
        assert_eq!(l.l2_backends.minio.backend, BackendKind::Real);
    }

    #[test]
    fn manifest_parses_l2_minio_real_block() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [l2.minio]
            backend        = "real"
            endpoint       = "http://localhost:9000"
            region         = "us-east-1"
            access_key_env = "MINIO_AK"
            secret_key_env = "MINIO_SK"
            path_style     = true
        "#);
        let m = &l.l2_backends.minio;
        assert_eq!(m.backend, BackendKind::Real);
        assert_eq!(m.endpoint.as_deref(), Some("http://localhost:9000"));
        assert_eq!(m.region.as_deref(), Some("us-east-1"));
        assert_eq!(m.access_key_env.as_deref(), Some("MINIO_AK"));
        assert_eq!(m.secret_key_env.as_deref(), Some("MINIO_SK"));
        assert_eq!(m.path_style, Some(true));
    }

    #[test]
    fn manifest_rejects_unknown_l2_key() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [l2.minio]
            backend = "real"
            bogus   = "nope"
        "#);
        assert!(
            e.message.contains("l2.minio") && e.message.contains("bogus"),
            "unexpected error message: {e}"
        );
    }

    #[test]
    fn manifest_rejects_unknown_l2_family() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [l2.mystery]
            backend = "real"
        "#);
        assert!(
            e.message.contains("unknown L2 family") && e.message.contains("mystery"),
            "unexpected error message: {e}"
        );
    }

    #[test]
    fn manifest_rejects_invalid_backend_kind() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [l2.minio]
            backend = "weird"
        "#);
        assert!(
            e.message.contains("must be one of") && e.message.contains("weird"),
            "unexpected error message: {e}"
        );
    }

    // ---- M41 — [runtime] section ----

    #[test]
    fn manifest_defaults_runtime_when_section_absent() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"
        "#);
        assert_eq!(l.runtime.output_dir, ".aeris");
        assert!(l.runtime.trace);
    }

    #[test]
    fn manifest_parses_runtime_output_dir_and_trace_off() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [runtime]
            output_dir = "build/observability"
            trace      = false
        "#);
        assert_eq!(l.runtime.output_dir, "build/observability");
        assert!(!l.runtime.trace);
    }

    #[test]
    fn manifest_rejects_empty_output_dir() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [runtime]
            output_dir = ""
        "#);
        assert!(e.message.contains("cannot be empty"), "{}", e.message);
    }

    #[test]
    fn manifest_rejects_unknown_runtime_key() {
        let e = bad(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [runtime]
            output_dir = ".aeris"
            mystery    = "nope"
        "#);
        assert!(
            e.message.contains("runtime") && e.message.contains("mystery"),
            "{}",
            e.message
        );
    }

    #[test]
    fn manifest_parses_replay_backend_for_mongodb() {
        let l = ok(r#"
            [project]
            name  = "x"
            aeris = "0.3.0"

            [l2.mongodb]
            backend = "replay"
            uri     = "mongodb://example/"
        "#);
        assert_eq!(l.l2_backends.mongodb.backend, BackendKind::Replay);
        assert_eq!(l.l2_backends.mongodb.uri.as_deref(), Some("mongodb://example/"));
    }
}
