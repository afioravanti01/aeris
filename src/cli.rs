//! Aeris CLI dispatch.
//!
//! See `docs/language.md` § 25 for the command surface.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use crate::loader::{load_with_imports, LoadError};
use crate::syntax::ast::Module;

/// Resolve the entry file and all transitively reachable
/// `use "./…"` / `use alias from "./…"` imports into a single
/// merged `Module`. On any IO / parse / cycle error, the helper
/// prints the diagnostic and returns the right exit code so the
/// caller can early-return.
fn load_module_with_imports(path: &str) -> Result<Module, ExitCode> {
    match load_with_imports(Path::new(path)) {
        Ok(m) => Ok(m),
        Err(LoadError::Io { path, err }) => {
            eprintln!("aeris: cannot read {}: {}", path.display(), err);
            Err(ExitCode::from(1))
        }
        Err(LoadError::Parse { path, err, .. }) => {
            eprintln!(
                "aeris: parse error in {} at line {}, col {}: {:?}",
                path.display(),
                err.span.line,
                err.span.col,
                err.kind
            );
            Err(ExitCode::from(64))
        }
        Err(LoadError::Cycle { path }) => {
            eprintln!(
                "aeris: import cycle: {} is reached from itself",
                path.display()
            );
            Err(ExitCode::from(64))
        }
    }
}

const VERSION: &str = "0.3.0";

const TEMPLATE_MANIFEST: &str = include_str!("templates/aeris.toml");
const TEMPLATE_MAIN_AER: &str = include_str!("templates/main.aer");

#[derive(Parser)]
#[command(
    name = "aeris",
    version = VERSION,
    about = "Aeris — single-binary DSL for ops, governance, pipelines and AI agents",
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the Aeris version and exit
    Version,
    /// Scaffold a new Aeris project in the current directory
    Init,
    /// Install a project under `$HOME/.aeris/projects/<name>`.
    /// `<path>` is a directory holding an `aeris.toml`; the
    /// `[project] name` value becomes the install directory name.
    /// Re-installing the same name replaces the previous copy.
    Install { path: String },
    /// Compile and run an .aer file. Trailing arguments after the
    /// file path are forwarded to `main` as a `list<string>` when
    /// `main` declares a non-`cap` parameter (M34.T2).
    Run {
        file: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Type & capability check, no run
    Check {
        file: Option<String>,
        /// Print a manpage-style description for the named exit code
        /// (M13.T6). Codes 64–71 are documented; the file argument is
        /// ignored when this flag is set.
        #[arg(long)]
        explain: Option<u8>,
    },
    /// Format an .aer file or directory
    Fmt {
        path: String,
        /// If set, do not rewrite — exit 1 if `path` is not formatted
        /// (CI mode, M12.T7).
        #[arg(long)]
        check: bool,
        /// V1 narrow-caps linter (M12.T6). Print a per-fn cap-narrowing
        /// diff for every function whose declared `cap[...]` is broader
        /// than its body actually uses. Linter only — never rewrites.
        #[arg(long = "narrow-caps")]
        narrow_caps: bool,
        /// One-shot migrator for M16. Rewrites the legacy
        /// `\(<expr>)` interpolation form into `{ <expr> }` in every
        /// `*.aer` under `path`. Idempotent: a second run is a no-op.
        #[arg(long = "migrate-strings")]
        migrate_strings: bool,
    },
    /// Run tests
    Test { path: Option<String> },
    /// `aeris lock` — recompute `aeris.toml` / `.aeris/surface.lock`.
    /// `--check` exits 69 if the lock is stale (CI mode, M7.T7).
    Lock {
        #[arg(long)]
        check: bool,
        /// Optional `aeris.toml` path; defaults to the cwd.
        #[arg(default_value = "aeris.toml")]
        file: String,
    },
    /// `aeris replay <trace_file> <source>` — re-run the program
    /// against a recorded JSONL trace (M9.T4–T7). `--live` re-issues
    /// `ai.*` and `http.*` while keeping `clock` / `random` pinned to
    /// the recording.
    Replay {
        /// Path to the recorded `.jsonl` trace.
        trace: String,
        /// Path to the `.aer` source to replay against.
        source: String,
        #[arg(long)]
        live: bool,
    },
    /// `aeris trace <subcommand>` — trace utilities. Today the only
    /// subcommand is `diff` (M13.T1).
    Trace {
        #[command(subcommand)]
        sub: TraceSub,
    },
    /// `aeris doc <file>` — extract `///` doc comments and emit one
    /// JSONL line per documented top-level decl (M13.T2 / § 25.1).
    Doc { file: String },
    /// `aeris module <subcommand>` — manage the L2 modules pinned in
    /// `aeris.toml [modules.*]` (M45).
    Module {
        #[command(subcommand)]
        sub: ModuleSub,
    },
}

#[derive(Subcommand)]
enum ModuleSub {
    /// List the modules declared in `aeris.toml` and whether each
    /// passes hash + signature verification.
    List,
    /// Verify one module by family name (`aeris module verify mongodb`)
    /// or all of them when no argument is given. Exit 1 on any
    /// failure (CI-friendly).
    Verify { family: Option<String> },
}

#[derive(Subcommand)]
enum TraceSub {
    /// Diff two recorded JSONL traces by `(scope, ordinal)`. § 20.4.
    Diff { a: String, b: String },
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => cmd_version(),
        Command::Init => cmd_init(),
        Command::Install { path } => cmd_install(&path),
        Command::Check { file, explain } => match (explain, file) {
            (Some(code), _) => cmd_check_explain(code),
            (None, Some(path)) => cmd_check(&path),
            (None, None) => {
                eprintln!("aeris: `aeris check <file>` or `aeris check --explain <code>`");
                ExitCode::from(1)
            }
        },
        Command::Run { file, args } => cmd_run(&file, &args),
        Command::Lock { check, file } => cmd_lock(&file, check),
        Command::Replay {
            trace,
            source,
            live,
        } => cmd_replay(&trace, &source, live),
        Command::Test { path } => cmd_test(path.as_deref()),
        Command::Trace { sub } => match sub {
            TraceSub::Diff { a, b } => cmd_trace_diff(&a, &b),
        },
        Command::Doc { file } => cmd_doc(&file),
        Command::Module { sub } => match sub {
            ModuleSub::List => cmd_module_list(),
            ModuleSub::Verify { family } => cmd_module_verify(family.as_deref()),
        },
        Command::Fmt {
            path,
            check,
            narrow_caps,
            migrate_strings,
        } => {
            if migrate_strings {
                cmd_fmt_migrate_strings(&path)
            } else if narrow_caps {
                cmd_fmt_narrow_caps(&path)
            } else {
                cmd_fmt(&path, check)
            }
        }
    }
}

/// `aeris fmt --narrow-caps <path>` (M12.T6 / § 8.5). Per-fn
/// capability minimisation linter. Walks every `*.aer` under `path`,
/// derives the actually-used `(module, op)` set + statically-extractable
/// allow-list, and prints a unified diff for every function whose
/// declared cap is broader than the body needs. Exits 0 when every
/// signature is already minimal; exits 1 to flag suggestions in CI.
fn cmd_fmt_narrow_caps(path: &str) -> ExitCode {
    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    let p = Path::new(path);
    if p.is_dir() {
        collect_aer_paths(p, &mut targets);
    } else if p.is_file() {
        targets.push(p.to_path_buf());
    } else {
        eprintln!("aeris: cannot lint `{path}` — no such file or directory");
        return ExitCode::from(1);
    }
    targets.sort();
    let mut any_suggestion = false;
    for target in &targets {
        let src = match fs::read_to_string(target) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aeris: cannot read {}: {e}", target.display());
                return ExitCode::from(1);
            }
        };
        let module = match crate::syntax::parse(&src) {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "aeris: parse error in {} at line {}, col {}: {:?}",
                    target.display(),
                    e.span.line,
                    e.span.col,
                    e.kind
                );
                return ExitCode::from(64);
            }
        };
        let diff = crate::check::render_narrowing_diff(&module);
        if !diff.is_empty() {
            any_suggestion = true;
            println!("# {}", target.display());
            print!("{diff}");
        }
    }
    if any_suggestion {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `aeris fmt --migrate-strings <path>` (M16.T4). Rewrites every
/// occurrence of the legacy interpolation form `\(<expr>)` inside a
/// double-quoted string into the M16 form `{ <expr> }`. The operation
/// is byte-level and idempotent: a string that has no `\(` is left
/// untouched. Files that do not contain `\(` at all are not rewritten.
fn cmd_fmt_migrate_strings(path: &str) -> ExitCode {
    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    let p = Path::new(path);
    if p.is_dir() {
        collect_aer_paths(p, &mut targets);
    } else if p.is_file() {
        targets.push(p.to_path_buf());
    } else {
        eprintln!("aeris: cannot migrate `{path}` — no such file or directory");
        return ExitCode::from(1);
    }
    targets.sort();
    let mut errors = false;
    let mut touched = 0usize;
    for target in &targets {
        let src = match fs::read_to_string(target) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aeris: cannot read {}: {e}", target.display());
                errors = true;
                continue;
            }
        };
        let migrated = match migrate_backslash_paren(&src) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aeris: cannot migrate {}: {e}", target.display());
                errors = true;
                continue;
            }
        };
        if migrated != src {
            if let Err(e) = fs::write(target, &migrated) {
                eprintln!("aeris: cannot write {}: {e}", target.display());
                errors = true;
            } else {
                eprintln!("aeris: migrated {}", target.display());
                touched += 1;
            }
        }
    }
    eprintln!("aeris: {touched}/{} files rewritten", targets.len());
    if errors {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// Byte-level transformer used by `cmd_fmt_migrate_strings`. Walks the
/// source, tracks whether we are inside a `"..."` literal (respecting
/// `\"` escapes), and replaces any `\(<expr>)` with `{<expr>}`. Outside
/// strings the input is passed through untouched.
fn migrate_backslash_paren(src: &str) -> Result<String, String> {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c != b'"' {
            out.push(c as char);
            i += 1;
            continue;
        }
        // Enter a string literal — copy verbatim until the matching
        // closing quote, but rewrite each `\(...)` encountered along
        // the way.
        out.push('"');
        i += 1;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\\' && i + 1 < bytes.len() && bytes[i + 1] == b'(' {
                // Found a legacy interpolation. Capture body up to the
                // matching `)` with paren nesting.
                i += 2;
                let body_start = i;
                let mut depth: u32 = 1;
                while i < bytes.len() && depth > 0 {
                    match bytes[i] {
                        b'(' => depth += 1,
                        b')' => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
                if depth != 0 {
                    return Err("unterminated `\\(...)` in string literal".into());
                }
                let body = &src[body_start..i];
                out.push('{');
                out.push_str(body);
                out.push('}');
                i += 1;
                continue;
            }
            if b == b'\\' && i + 1 < bytes.len() {
                // Pass through any other escape so this is purely a
                // syntactic rewrite of `\(...)` and nothing else.
                out.push('\\');
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            out.push(b as char);
            i += 1;
            if b == b'"' {
                break;
            }
        }
    }
    Ok(out)
}

/// `aeris fmt <path> [--check]` (M12.T5 / M12.T7). Formats the file
/// (or every `*.aer` under `path` if it's a directory) using the
/// canonical formatter. With `--check` it does not write — it exits 1
/// if the formatted body differs from the on-disk content. The
/// formatter is total and idempotent: `fmt(fmt(x)) == fmt(x)`.
fn cmd_fmt(path: &str, check_only: bool) -> ExitCode {
    let mut targets: Vec<std::path::PathBuf> = Vec::new();
    let p = Path::new(path);
    if p.is_dir() {
        collect_aer_paths(p, &mut targets);
    } else if p.is_file() {
        targets.push(p.to_path_buf());
    } else {
        eprintln!("aeris: cannot fmt `{path}` — no such file or directory");
        return ExitCode::from(1);
    }
    targets.sort();
    let mut drift = false;
    let mut errors = false;
    for target in &targets {
        let src = match fs::read_to_string(target) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("aeris: cannot read {}: {e}", target.display());
                errors = true;
                continue;
            }
        };
        let module = match crate::syntax::parse(&src) {
            Ok(m) => m,
            Err(e) => {
                eprintln!(
                    "aeris: parse error in {} at line {}, col {}: {:?}",
                    target.display(),
                    e.span.line,
                    e.span.col,
                    e.kind
                );
                errors = true;
                continue;
            }
        };
        let formatted = crate::syntax::fmt::format_module(&module, &src);
        if check_only {
            if formatted != src {
                eprintln!("aeris: {} is not formatted", target.display());
                drift = true;
            }
        } else if formatted != src {
            if let Err(e) = fs::write(target, &formatted) {
                eprintln!("aeris: cannot write {}: {e}", target.display());
                errors = true;
            } else {
                eprintln!("aeris: formatted {}", target.display());
            }
        }
    }
    if errors || (check_only && drift) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

/// `aeris check --explain <code>` (M13.T6 / § 25.3). Print the
/// manpage-style description for `code`. Exit 0 if the code is
/// known; exit 1 with a hint otherwise.
fn cmd_check_explain(code: u8) -> ExitCode {
    match crate::check::explain(code) {
        Some(body) => {
            print!("{body}");
            ExitCode::SUCCESS
        }
        None => {
            eprintln!("aeris: no `--explain` entry for E{code} (try 64..71)");
            ExitCode::from(1)
        }
    }
}

/// `aeris doc <file>` (M13.T2 / § 25.1). Extract `///` doc comments
/// preceding top-level decls and emit one JSONL line per documented
/// decl on stdout.
fn cmd_doc(file: &str) -> ExitCode {
    let src = match fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {file}: {e}");
            return ExitCode::from(1);
        }
    };
    match crate::syntax::doc::extract_docs(&src) {
        Ok(entries) => {
            print!("{}", crate::syntax::doc::render_jsonl(&entries));
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("aeris: doc extraction failed: {e}");
            ExitCode::from(1)
        }
    }
}

/// `aeris module list` — read `aeris.toml`, print one row per
/// `[modules.<family>]` entry with its path, hash and verification
/// status. Returns 0 even when modules fail to verify (use
/// `aeris module verify` for an exit-code signal).
fn cmd_module_list() -> ExitCode {
    let (manifest, project_root) = match load_manifest_from_cwd() {
        Ok(t) => t,
        Err(code) => return code,
    };
    if manifest.modules.is_empty() {
        println!("(no modules declared in aeris.toml)");
        return ExitCode::SUCCESS;
    }
    println!("{:<10} {:<14} {}", "FAMILY", "STATUS", "PATH");
    for (family, entry) in &manifest.modules {
        let status =
            match crate::runtime::l2_module::LoadedModule::load(entry, &project_root) {
                Ok(m) => format!("ok  v={}", m.metadata.version),
                Err(_) => "FAIL".to_string(),
            };
        println!("{family:<10} {status:<14} {}", entry.path);
    }
    ExitCode::SUCCESS
}

/// `aeris module verify [<family>]` — force-load every (or one)
/// module from `aeris.toml`. Prints one line per attempt; exits 1
/// if any module fails to verify.
fn cmd_module_verify(family_filter: Option<&str>) -> ExitCode {
    let (manifest, project_root) = match load_manifest_from_cwd() {
        Ok(t) => t,
        Err(code) => return code,
    };
    if manifest.modules.is_empty() {
        eprintln!("aeris: no modules declared in aeris.toml");
        return ExitCode::SUCCESS;
    }
    let mut any_failed = false;
    for (family, entry) in &manifest.modules {
        if let Some(f) = family_filter {
            if f != family {
                continue;
            }
        }
        match crate::runtime::l2_module::LoadedModule::load(entry, &project_root) {
            Ok(m) => {
                println!(
                    "ok    {family}  ({}@{}, api={}, caps=[{}])",
                    m.metadata.name,
                    m.metadata.version,
                    m.metadata.api_version,
                    m.metadata.cap_paths.join(", "),
                );
            }
            Err(e) => {
                println!("FAIL  {family}  {e}");
                any_failed = true;
            }
        }
    }
    if any_failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn load_manifest_from_cwd() -> Result<(crate::manifest::Manifest, std::path::PathBuf), ExitCode> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let manifest_path = cwd.join("aeris.toml");
    let body = match fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {}: {e}", manifest_path.display());
            return Err(ExitCode::from(1));
        }
    };
    match crate::manifest::parse_manifest(&body) {
        Ok(m) => Ok((m, cwd)),
        Err(e) => {
            eprintln!("aeris: {}: {e}", manifest_path.display());
            Err(ExitCode::from(69))
        }
    }
}

/// `aeris trace diff <a> <b>` (M13.T1 / § 20.4). Loads both JSONL
/// traces, aligns them by `(scope, ordinal)`, and prints diverging
/// fields plus missing / extra events. Exits 0 when the traces are
/// equal under the alignment, 1 when they diverge.
fn cmd_trace_diff(a_path: &str, b_path: &str) -> ExitCode {
    let a_text = match fs::read_to_string(a_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {a_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let b_text = match fs::read_to_string(b_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {b_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let a_events = match crate::runtime::replay::parse_trace_jsonl(&a_text) {
        Ok(es) => es,
        Err(e) => {
            eprintln!("aeris: invalid trace {a_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let b_events = match crate::runtime::replay::parse_trace_jsonl(&b_text) {
        Ok(es) => es,
        Err(e) => {
            eprintln!("aeris: invalid trace {b_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let rows = crate::runtime::trace_diff::diff_traces(&a_events, &b_events);
    if rows.is_empty() {
        eprintln!("aeris: traces match");
        return ExitCode::SUCCESS;
    }
    let report = crate::runtime::trace_diff::render_diff(&rows);
    print!("{report}");
    ExitCode::from(1)
}

/// Recursively gather every `*.aer` path under `dir`.
fn collect_aer_paths(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_aer_paths(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("aer") {
                out.push(p);
            }
        }
    }
}

/// `aeris test [path]` (M12.T1). Without an argument: discover every
/// `tests/**/*.test.aer` under the cwd. With an argument that names a
/// directory: discover under it. With an argument that names a single
/// `*.test.aer` file: run only that suite. With a *bare* suite name
/// (e.g. `aeris test foo`): match `tests/foo.test.aer`. Exit 0 if all
/// passed; exit 1 if any test failed or any suite failed to parse.
fn cmd_test(path: Option<&str>) -> ExitCode {
    let suites = match path {
        None => crate::test_harness::discover_suites(Path::new("tests")),
        Some(p) => {
            let direct = Path::new(p);
            if direct.is_file() {
                let stem = direct
                    .file_name()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.strip_suffix(".test.aer"))
                    .unwrap_or("suite")
                    .to_string();
                vec![(stem, direct.to_path_buf())]
            } else if direct.is_dir() {
                crate::test_harness::discover_suites(direct)
            } else {
                // Treat as a bare suite name resolved under `tests/`.
                let candidate = Path::new("tests").join(format!("{p}.test.aer"));
                if candidate.is_file() {
                    vec![(p.to_string(), candidate)]
                } else {
                    eprintln!("aeris: no test file matching `{p}` (looked at {})", candidate.display());
                    return ExitCode::from(1);
                }
            }
        }
    };
    // M43 — read `aeris.toml` from the cwd just like `cmd_run`,
    // and thread the synthesised cap + `[ai.backend]` + `[l2.*]`
    // + `[policies] active = [..]` into every test body. Without
    // this every cap-gated call (`http.*`, `ai.*`, `assert_semantic`,
    // L2 builtins) raises `PolicyViolation` because the test body
    // has no `cap` in scope.
    let manifest = fs::read_to_string("aeris.toml")
        .ok()
        .and_then(|s| crate::manifest::parse_manifest(&s).ok());
    let cfg = manifest
        .as_ref()
        .map(|l| {
            let l2_runtime = crate::runtime::l2_runtime::shared();
            // M45 — load modules from the manifest under the test
            // suite's project root (cwd). A loading failure aborts
            // the test session: the user needs to see it.
            let project_root = std::env::current_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            let l2_backends = match crate::runtime::l2_backend::L2Backends::from_manifest_with_modules(
                &l.l2_backends,
                l2_runtime,
                &l.modules,
                &project_root,
            ) {
                Ok(b) => std::rc::Rc::new(b),
                Err(e) => {
                    eprintln!("aeris: module loading failed: {e}");
                    std::process::exit(1);
                }
            };
            let active = if l.policies.is_empty() {
                None
            } else {
                Some(l.policies.clone())
            };
            crate::test_harness::SuiteConfig {
                cap: Some(l.synthesise_main_cap()),
                ai_backend: l.ai_backend.clone().map(std::rc::Rc::new),
                l2_backends: Some(l2_backends),
                active_policy_names: active,
            }
        })
        .unwrap_or_default();
    let report = crate::test_harness::run_suites_explicit_with_cfg(&suites, &cfg);
    for (suite, msg) in &report.parse_failures {
        eprintln!("aeris: suite `{suite}` failed to parse: {msg}");
    }
    for o in &report.outcomes {
        match &o.status {
            crate::test_harness::TestStatus::Passed => {
                println!("ok    {}::{}", o.suite, o.name);
            }
            crate::test_harness::TestStatus::Failed(reason) => {
                println!("FAIL  {}::{} — {}", o.suite, o.name, reason);
            }
        }
    }
    let total = report.outcomes.len();
    let passed = report.passed();
    let failed = report.failed();
    eprintln!("aeris: {passed}/{total} tests passed; {failed} failed");
    ExitCode::from(report.exit_code())
}

fn cmd_version() -> ExitCode {
    println!("aeris {VERSION}");
    ExitCode::SUCCESS
}

fn cmd_init() -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("aeris: cannot read current directory: {e}");
            return ExitCode::from(1);
        }
    };
    match scaffold_project(&cwd) {
        Ok(()) => {
            println!("aeris: scaffolded project in {}", cwd.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("aeris: {e}");
            ExitCode::from(1)
        }
    }
}

/// `aeris install <path>` — copy a project directory into the user's
/// shared store at `$HOME/.aeris/projects/<name>`, where `<name>` is
/// the `[project] name` declared in `<path>/aeris.toml`. Re-installing
/// the same name replaces the previous copy. Exit codes: 69 on any
/// manifest problem (missing/unreadable/invalid `aeris.toml`, unsafe
/// name), 1 on a filesystem error.
fn cmd_install(path: &str) -> ExitCode {
    let home = match std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        Some(h) => std::path::PathBuf::from(h),
        None => {
            eprintln!("aeris: cannot resolve $HOME");
            return ExitCode::from(1);
        }
    };
    let projects_root = home.join(".aeris").join("projects");
    match install_project(Path::new(path), &projects_root) {
        Ok(dest) => {
            println!("aeris: installed `{}`", dest.display());
            ExitCode::SUCCESS
        }
        Err(InstallError::Manifest(msg)) => {
            eprintln!("aeris: {msg}");
            ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR)
        }
        Err(InstallError::Io(msg)) => {
            eprintln!("aeris: {msg}");
            ExitCode::from(1)
        }
    }
}

#[derive(Debug)]
enum InstallError {
    /// Bad input: missing/unreadable/invalid `aeris.toml`, unsafe name.
    Manifest(String),
    /// Filesystem failure while copying or replacing the install dir.
    Io(String),
}

/// Testable core of `aeris install`. Reads `<source>/aeris.toml`, takes
/// `[project] name`, and copies `source` to `<projects_root>/<name>`,
/// replacing any previous install of the same name. Returns the
/// destination path on success.
fn install_project(
    source: &Path,
    projects_root: &Path,
) -> Result<std::path::PathBuf, InstallError> {
    if !source.is_dir() {
        return Err(InstallError::Manifest(format!(
            "`{}` is not a directory",
            source.display()
        )));
    }
    let manifest_path = source.join("aeris.toml");
    let body = fs::read_to_string(&manifest_path).map_err(|e| {
        InstallError::Manifest(format!("cannot read {}: {e}", manifest_path.display()))
    })?;
    let manifest = crate::manifest::parse_manifest(&body)
        .map_err(|e| InstallError::Manifest(format!("manifest error: {e}")))?;
    let name = manifest.project.name.trim();
    validate_project_name(name).map_err(InstallError::Manifest)?;

    let dest = projects_root.join(name);

    // Guard against installing a directory onto itself (e.g. running
    // `aeris install` on an already-installed project), which would
    // wipe the source before the copy.
    if let (Ok(s), Ok(d)) = (source.canonicalize(), dest.canonicalize()) {
        if s == d {
            return Err(InstallError::Io(format!(
                "source and destination are the same directory ({})",
                d.display()
            )));
        }
    }

    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| {
            InstallError::Io(format!("cannot replace {}: {e}", dest.display()))
        })?;
    }
    fs::create_dir_all(&dest)
        .map_err(|e| InstallError::Io(format!("cannot create {}: {e}", dest.display())))?;
    copy_dir_recursive(source, &dest).map_err(InstallError::Io)?;
    Ok(dest)
}

/// The install name becomes a single directory under
/// `$HOME/.aeris/projects`, so it must be exactly one safe path
/// component — no separators, no `.`/`..` traversal, not empty.
fn validate_project_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("`[project] name` is empty".into());
    }
    if name == "." || name == ".." {
        return Err(format!("`[project] name` cannot be `{name}`"));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err(format!(
            "`[project] name` `{name}` contains a path separator"
        ));
    }
    // Belt and braces: the name must resolve to exactly one normal
    // component, never an absolute root, prefix, or parent ref.
    let mut comps = Path::new(name).components();
    match (comps.next(), comps.next()) {
        (Some(std::path::Component::Normal(_)), None) => Ok(()),
        _ => Err(format!("`[project] name` `{name}` is not a valid directory name")),
    }
}

/// Recursively copy `src` into the already-existing directory `dst`.
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    let rd = fs::read_dir(src).map_err(|e| format!("cannot read {}: {e}", src.display()))?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("cannot read entry in {}: {e}", src.display()))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let ft = entry
            .file_type()
            .map_err(|e| format!("cannot stat {}: {e}", from.display()))?;
        if ft.is_dir() {
            fs::create_dir_all(&to)
                .map_err(|e| format!("cannot create {}: {e}", to.display()))?;
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to).map_err(|e| {
                format!("cannot copy {} to {}: {e}", from.display(), to.display())
            })?;
        }
    }
    Ok(())
}

/// `aeris run <file>` — pure-interpreter driver (M3.T6). Exit codes
/// follow `language.md` § 25.3:
///   0  → `main()` returned cleanly (any value, typically `Ok(())`)
///   64 → parse / type / check error
///   1  → uncaught `Err(...)` or `raise <value>`
fn cmd_run(path: &str, argv: &[String]) -> ExitCode {
    let module = match load_module_with_imports(path) {
        Ok(m) => m,
        Err(code) => return code,
    };
    // Source string used for diagnostic context rendering. Diagnostics
    // from inlined sub-modules may show the wrong context lines — the
    // error `kind` and `(line, col)` still point at the right file in
    // the loader's message above.
    let src = fs::read_to_string(path).unwrap_or_default();
    // M7.T4 + M15: when a `aeris.toml` sits next to the source file,
    // use its `[caps]` ceiling as `main`'s synthesised cap, and route
    // the static checker through `check_module_with_manifest` so the
    // `required` flag (§ 8.4.1) is honoured. M8.T5 then filters the
    // module's declared policies through `[policies] active = [..]`.
    let manifest_path = Path::new(path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("aeris.toml");
    let manifest_for_check = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| crate::manifest::parse_manifest(&s).ok());
    let check_errs = match &manifest_for_check {
        Some(l) => crate::check::check_module_with_manifest(&module, &l.caps),
        None => crate::check::check_module(&module),
    };
    if !check_errs.is_empty() {
        let mut max_exit: u8 = 0;
        for err in &check_errs {
            eprint!("{}", crate::check::render_diagnostic(&src, err));
            max_exit = max_exit.max(err.exit_code());
        }
        return ExitCode::from(max_exit);
    }
    // M41 — `project_root` is the directory that holds `main.aer`
    // (or `aeris.toml`, if present); every relative output path
    // resolves against it. Falls back to the shell cwd only when
    // the source path has no parent (e.g. `aeris run main.aer`
    // launched in the same directory).
    let project_root: std::path::PathBuf = Path::new(path)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let composed = manifest_for_check.as_ref().map(|l| {
        eprint!("{}", l.describe_main_cap());
        let cap = l.synthesise_main_cap();
        let backend = l.ai_backend.clone().map(std::rc::Rc::new);
        let policies = l.policies.clone();
        // M22.T4 — build the L2 backend table from `[l2.*]`.
        // M45 — install dynamically loaded modules from
        // `[modules.<family>]`; loading verifies blake3 hash and the
        // ed25519 signature against the Aeris registry public key.
        let l2_runtime = crate::runtime::l2_runtime::shared();
        let l2_backends_result =
            crate::runtime::l2_backend::L2Backends::from_manifest_with_modules(
                &l.l2_backends,
                l2_runtime,
                &l.modules,
                &project_root,
            );
        let l2_backends = match l2_backends_result {
            Ok(b) => std::rc::Rc::new(b),
            Err(e) => {
                eprintln!("aeris: module loading failed: {e}");
                std::process::exit(1);
            }
        };
        (cap, backend, policies, l2_backends)
    });
    // M41 — resolve `<output_dir>` against `project_root`, then pin
    // the audit log under it and (when `runtime.trace`) open a
    // per-run JSONL trace file. A boot banner on stderr surfaces
    // the resolved trace path so users can `grep -F trace_id=…`
    // immediately.
    let runtime_cfg = manifest_for_check
        .as_ref()
        .map(|l| l.runtime.clone())
        .unwrap_or_default();
    let output_dir = {
        let configured = std::path::PathBuf::from(&runtime_cfg.output_dir);
        if configured.is_absolute() {
            configured
        } else {
            project_root.join(configured)
        }
    };
    let _ = std::fs::create_dir_all(&output_dir);
    crate::runtime::eval::set_audit_log_override(output_dir.join("audit.jsonl"));
    let tracer = if runtime_cfg.trace {
        let traces_dir = output_dir.join("traces");
        let _ = std::fs::create_dir_all(&traces_dir);
        // Build the tracer first so we get its ULID, then point the
        // writer at `<traces_dir>/<id>.jsonl`. Failure to open the
        // file is non-fatal: we fall back to in-memory tracing and
        // warn so the run still completes.
        let probe = crate::runtime::Tracer::new(Box::new(Vec::<u8>::new()));
        let trace_id = probe.trace_id();
        let trace_path = traces_dir.join(format!("{trace_id}.jsonl"));
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&trace_path)
        {
            Ok(file) => {
                eprintln!(
                    "[aeris] trace_id = {trace_id} → {}",
                    trace_path.display()
                );
                Some(crate::runtime::Tracer::new(Box::new(file)))
            }
            Err(e) => {
                eprintln!(
                    "[aeris] cannot open trace file {} ({e}); tracing in memory only",
                    trace_path.display()
                );
                Some(probe)
            }
        }
    } else {
        None
    };
    let outcome = if let Some((cap, backend, policies, l2_backends)) = composed {
        // M42 — one code path: forward both `[ai.backend]` /
        // `[l2.*]` overrides and the `[policies] active = [..]`
        // filter through the same entry. Prior split routed
        // policy-enabled runs through a stub that dropped
        // `ai_backend` and `l2_backends`, silently falling back to
        // mock — see M42 in `docs/plan.md § 5`.
        let active_policies = if policies.is_empty() {
            None
        } else {
            Some(policies.as_slice())
        };
        crate::runtime::eval::run_main_with_full_cfg_argv_full(
            &module,
            cap,
            tracer,
            backend,
            None,
            false,
            argv,
            Some(l2_backends),
            active_policies,
        )
    } else {
        crate::runtime::eval::run_main_with_argv(&module, tracer, argv)
    };
    match outcome {
        Ok(v) => {
            // Surface a non-Ok result as exit 1 (uncaught Err); a
            // successful `Ok(...)` (or any other plain value) is exit 0.
            if let crate::runtime::Value::Result(Err(payload)) = &v {
                eprintln!("aeris: uncaught Err({payload:?})");
                return ExitCode::from(1);
            }
            ExitCode::SUCCESS
        }
        Err(err) => match err.kind {
            crate::runtime::EvalErrorKind::Raised(_) => {
                eprintln!(
                    "aeris: uncaught raise at line {}, col {}",
                    err.span.line, err.span.col
                );
                ExitCode::from(1)
            }
            // M5.T5: contract violations are fatal and not catchable
            // by `?`; `aeris run` exits 64 (§ 9.2 / § 25.3).
            crate::runtime::EvalErrorKind::ContractViolation { fn_name, clause } => {
                eprintln!(
                    "aeris: contract violation in `{fn_name}` ({clause:?}) at line {}, col {}",
                    err.span.line, err.span.col
                );
                ExitCode::from(64)
            }
            // M5.T2 + fs allow-list violations.
            crate::runtime::EvalErrorKind::PolicyViolation { op, target } => {
                eprintln!(
                    "aeris: policy violation: `{op}` not authorised for `{target}` (line {}, col {})",
                    err.span.line, err.span.col
                );
                ExitCode::from(1)
            }
            // M8.T1: model validation failed at construction or decode
            // (§ 16.2). Not catchable by `?`. CLI surfaces it with the
            // bag of problems and exits 1.
            crate::runtime::EvalErrorKind::SchemaViolation {
                model,
                version,
                problems,
            } => {
                eprintln!(
                    "aeris: schema violation for `{model}@v{version}` at line {}, col {}:",
                    err.span.line, err.span.col
                );
                for p in problems {
                    eprintln!("  - {p}");
                }
                ExitCode::from(1)
            }
            // M10.T4: agent overran its `budget:` (tokens or latency).
            // Surfaced as exit 1 — not catchable by `?` because the
            // budget is the agent's hard wall, not a recoverable error.
            crate::runtime::EvalErrorKind::BudgetExceeded {
                agent,
                kind,
                limit,
                observed,
            } => {
                eprintln!(
                    "aeris: agent `{agent}` exceeded {kind} budget: limit {limit}, observed {observed}"
                );
                ExitCode::from(1)
            }
            // M6.T5: saga rollback could not complete — exit 74
            // (§ 12.4 / § 25.3).
            crate::runtime::EvalErrorKind::PartialFailure {
                saga,
                completed,
                failed_step,
            } => {
                eprintln!(
                    "aeris: saga `{saga}` partial failure: undo of `{failed_step}` exhausted retries (completed: {completed:?})"
                );
                ExitCode::from(74)
            }
            crate::runtime::EvalErrorKind::ModuleNotImported { module, op } => {
                eprintln!(
                    "aeris: module `{module}` used without `use` (call to `{module}.{op}` at line {}, col {})",
                    err.span.line, err.span.col
                );
                eprintln!("        add `use {module}` at the top of the file");
                ExitCode::from(72)
            }
            other => {
                eprintln!(
                    "aeris: runtime error at line {}, col {}: {other:?}",
                    err.span.line, err.span.col
                );
                ExitCode::from(1)
            }
        },
    }
}

/// `aeris replay <trace_file> <source>` — re-run the program against
/// a recorded JSONL trace (M9.T4–T7). The default mode is read-only
/// (`FromFixtures`); `--live` re-issues network/LLM calls while
/// pinning `clock` / `random` to the recorded values.
fn cmd_replay(trace_path: &str, source_path: &str, live: bool) -> ExitCode {
    let trace_text = match fs::read_to_string(trace_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read trace {trace_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let events = match crate::runtime::replay::parse_trace_jsonl(&trace_text) {
        Ok(es) => es,
        Err(e) => {
            eprintln!("aeris: invalid trace: {e}");
            return ExitCode::from(1);
        }
    };
    let src = match fs::read_to_string(source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {source_path}: {e}");
            return ExitCode::from(1);
        }
    };
    let module = match crate::syntax::parse(&src) {
        Ok(m) => m,
        Err(e) => {
            eprintln!(
                "aeris: parse error at line {}, col {}: {:?}",
                e.span.line, e.span.col, e.kind
            );
            return ExitCode::from(64);
        }
    };
    let mode = if live {
        crate::runtime::replay::ReplayMode::Live
    } else {
        crate::runtime::replay::ReplayMode::FromFixtures
    };
    let tape = crate::runtime::replay::handle_from_events(events, mode);
    // Compose `main`'s cap from a co-located `aeris.toml` if any —
    // otherwise fall back to `cap[*]`. Replay does not require a
    // manifest (the original run's recording stands in for it).
    let manifest_path = Path::new(source_path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("aeris.toml");
    let cap = if manifest_path.exists() {
        fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| crate::manifest::parse_manifest(&s).ok())
            .map(|l| l.synthesise_main_cap())
            .unwrap_or_else(|| crate::runtime::value::CapValue {
                entries: Vec::new(),
                star: true,
            })
    } else {
        crate::runtime::value::CapValue {
            entries: Vec::new(),
            star: true,
        }
    };
    let outcome =
        crate::runtime::eval::run_main_with_full_cfg(&module, cap, None, None, Some(tape), false);
    match outcome {
        Ok(_) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!(
                "aeris: replay error at line {}, col {}: {:?}",
                err.span.line, err.span.col, err.kind
            );
            ExitCode::from(1)
        }
    }
}

/// `aeris check <file>` — parse + type-resolve the input. Returns the
/// highest exit code among reported errors (§ 25.3) or 0 on success.
/// `parse_recovering` collects every parse error rather than aborting
/// at the first; the type checker likewise emits all diagnostics in a
/// single pass (M2.T1+). Surface-diff rendering (M7.T5) is wired in
/// once the surface lock is implemented.
fn cmd_check(path: &str) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {path}: {e}");
            return ExitCode::from(1);
        }
    };
    // Run `parse_recovering` on the entry file (preserves the
    // multi-error UX), then layer the multi-file loader on top: a
    // single `inline_local_imports` call walks every `use "./…"`
    // clause and inlines the referenced files in place. Sub-module
    // parse errors are fatal (exit 64).
    let outcome = crate::syntax::parse_recovering(&src);
    let mut module = outcome.module.clone();
    let project_root = Path::new(path).parent().unwrap_or(Path::new("."));
    let mut load_failed = false;
    if let Err(err) = crate::loader::inline_local_imports(&mut module, project_root) {
        match err {
            LoadError::Io { path, err } => {
                eprintln!("aeris: cannot read {}: {}", path.display(), err);
            }
            LoadError::Parse { path, err, .. } => {
                eprintln!(
                    "aeris: parse error in {} at line {}, col {}: {:?}",
                    path.display(),
                    err.span.line,
                    err.span.col,
                    err.kind
                );
            }
            LoadError::Cycle { path } => {
                eprintln!(
                    "aeris: import cycle: {} is reached from itself",
                    path.display()
                );
            }
        }
        load_failed = true;
    }
    let mut max_exit: u8 = if load_failed { 64 } else { 0 };
    // M2.T12: surface drift is the first hunk — printed *before* any
    // parse / type / cap diagnostics so reviewers see authority changes
    // first (`thesis.md` § 13 / `language.md` § 8.6).
    let manifest_path = project_root.join("aeris.toml");
    let manifest_loaded = fs::read_to_string(&manifest_path)
        .ok()
        .and_then(|s| crate::manifest::parse_manifest(&s).ok());
    if manifest_loaded.is_some() {
        emit_surface_drift_hunk(project_root);
    }
    for err in &outcome.errors {
        eprintln!(
            "aeris: parse error at line {}, col {}: {:?}",
            err.span.line, err.span.col, err.kind
        );
        max_exit = max_exit.max(64);
    }
    // M2.T6: when `aeris.toml` sits next to the source, run the
    // allow-list intersection check (§ 8.3.2). Out-of-ceiling entries
    // are surfaced with exit code 71. A missing manifest is not an
    // error here — `aeris check` falls back to the standalone pass.
    let check_errs = match manifest_loaded {
        Some(l) => crate::check::check_module_with_manifest(&module, &l.caps),
        None => crate::check::check_module(&module),
    };
    for err in &check_errs {
        // M13.T3 / M13.T4: human-grade renderer with section reference
        // and Rust-style caret underline.
        eprint!("{}", crate::check::render_diagnostic(&src, err));
        max_exit = max_exit.max(err.exit_code());
    }
    if max_exit == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(max_exit)
    }
}

/// `aeris lock` / `aeris lock --check` (M7.T1 + M7.T7). Loads the
/// manifest, validates the structure, and (in `--check`) compares the
/// computed `.aeris/surface.lock` against the committed file. Exit
/// 69 on any drift / parse / validation failure.
fn cmd_lock(path: &str, check: bool) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {path}: {e}");
            return ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR);
        }
    };
    let manifest = match crate::manifest::parse_manifest(&src) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("aeris: manifest error: {e}");
            return ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR);
        }
    };
    eprintln!(
        "aeris: manifest `{}` ok ({} deps, {} policies)",
        manifest.project.name,
        manifest.deps.len(),
        manifest.policies.len()
    );
    let project_root = Path::new(path).parent().unwrap_or(Path::new("."));
    // M7.T2: re-hash every `path = "..."` dep and compare against
    // the pinned `blake3:...`. Mismatch (or missing file) → exit 69.
    if let Err(errs) = crate::manifest::verify_local_deps(&manifest, project_root) {
        for e in errs {
            eprintln!("aeris: {e}");
        }
        return ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR);
    }
    // Compute the surface lock from src/**/*.aer (best effort: we
    // walk the conventional `src/` tree if present).
    let src_dir = project_root.join("src");
    let mut files: Vec<(String, String)> = Vec::new();
    if src_dir.exists() {
        collect_aer_files(&src_dir, &mut files);
    }
    let surface = match crate::manifest::compute_surface(&files) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: surface compute failed: {e}");
            return ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR);
        }
    };
    let surface_path = project_root.join(".aeris/surface.lock");
    let new_body = crate::manifest::surface::render_surface_lock(&surface);
    if check {
        let on_disk = fs::read_to_string(&surface_path).unwrap_or_default();
        if on_disk != new_body {
            eprintln!("aeris: surface.lock is stale (run `aeris lock` to refresh)");
            return ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR);
        }
        eprintln!("aeris: surface.lock matches");
    } else if let Err(e) = crate::manifest::write_surface_lock(&surface, &surface_path) {
        eprintln!("aeris: cannot write surface.lock: {e}");
        return ExitCode::from(crate::manifest::EXIT_MANIFEST_ERROR);
    } else {
        eprintln!("aeris: wrote {}", surface_path.display());
    }
    ExitCode::SUCCESS
}

/// M2.T12: when `aeris check` runs in a project (aeris.toml present),
/// compute the live effect surface from `src/**/*.aer` and compare it
/// to the committed `.aeris/surface.lock`. If the two differ, emit a
/// unified diff to stderr as the very first output of the check pass.
fn emit_surface_drift_hunk(project_root: &Path) {
    let src_dir = project_root.join("src");
    if !src_dir.exists() {
        return;
    }
    let mut files: Vec<(String, String)> = Vec::new();
    collect_aer_files(&src_dir, &mut files);
    let surface = match crate::manifest::compute_surface(&files) {
        Ok(s) => s,
        Err(_) => return,
    };
    let computed = crate::manifest::surface::render_surface_lock(&surface);
    let on_disk = fs::read_to_string(project_root.join(".aeris/surface.lock")).unwrap_or_default();
    let diff = crate::manifest::diff_surface_bodies(&on_disk, &computed);
    if !diff.is_empty() {
        eprintln!("aeris: surface drift — run `aeris lock` to refresh:");
        eprint!("{diff}");
        eprintln!();
    }
}

fn collect_aer_files(dir: &Path, out: &mut Vec<(String, String)>) {
    if let Ok(rd) = fs::read_dir(dir) {
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                collect_aer_files(&p, out);
            } else if p.extension().and_then(|s| s.to_str()) == Some("aer") {
                if let Ok(s) = fs::read_to_string(&p) {
                    let rel = p.strip_prefix(dir).unwrap_or(&p);
                    out.push((rel.to_string_lossy().into_owned(), s));
                }
            }
        }
    }
}

fn scaffold_project(root: &Path) -> Result<(), String> {
    let manifest = root.join("aeris.toml");
    let src_dir = root.join("src");
    let main_aer = src_dir.join("main.aer");

    if manifest.exists() || main_aer.exists() {
        return Err("project files already exist; refusing to overwrite".into());
    }

    fs::create_dir_all(&src_dir).map_err(|e| format!("cannot create src/: {e}"))?;
    fs::write(&manifest, TEMPLATE_MANIFEST).map_err(|e| format!("cannot write aeris.toml: {e}"))?;
    fs::write(&main_aer, TEMPLATE_MAIN_AER)
        .map_err(|e| format!("cannot write src/main.aer: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// Inline reimplementation of `cmd_check` that returns the numeric
    /// exit code so unit tests can assert on it directly. Mirrors the
    /// production logic above; if either drifts the integration suite
    /// will catch it via the CLI binary.
    fn check_exit_code(src: &str) -> u8 {
        let outcome = crate::syntax::parse_recovering(src);
        let mut max_exit: u8 = 0;
        if !outcome.errors.is_empty() {
            max_exit = 64;
        }
        for err in crate::check::check_module(&outcome.module) {
            max_exit = max_exit.max(err.exit_code());
        }
        max_exit
    }

    #[test]
    fn cli_check_exit_code_is_zero_on_clean_input() {
        assert_eq!(check_exit_code("record R { x: int }"), 0);
    }

    #[test]
    fn cli_check_exit_code_is_64_on_unknown_type() {
        assert_eq!(check_exit_code("record R { x: Foo }"), 64);
    }

    #[test]
    fn cli_check_exit_code_is_65_on_cap_star() {
        assert_eq!(check_exit_code("fn f(cap: cap[*]) {}"), 65);
    }

    #[test]
    fn cli_check_exit_code_is_67_on_saga_undo_noop() {
        let src = r#"
            saga s(cap: cap[http.post]) {
                intent "x"
                step go {
                    do { http.post("u", "\{\}")? }
                    undo noop
                }
            }
        "#;
        assert_eq!(check_exit_code(src), 67);
    }

    #[test]
    fn cli_check_exit_code_is_70_on_agent_net_cycle() {
        assert_eq!(check_exit_code("agent_net p { flow a -> b -> a }"), 70);
    }

    #[test]
    fn cli_check_takes_max_exit_code_across_errors() {
        // Both a 64 and a 65 — `aeris check` returns 65 (max).
        let src = "fn f(cap: cap[*]) -> Foo {}";
        assert_eq!(check_exit_code(src), 65);
    }

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn write_temp(content: &str) -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("aeris-cli-test-{pid}-{id}.aer"));
        fs::write(&path, content).expect("write temp file");
        path
    }

    // ---- M2.T12: surface drift is the first hunk on `aeris check` ----

    fn unique_dir(tag: &str) -> std::path::PathBuf {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let p = std::env::temp_dir().join(format!("aeris-cli-{tag}-{pid}-{id}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).expect("create temp dir");
        p
    }

    /// Capture what `emit_surface_drift_hunk` writes to stderr by
    /// rendering the same diff via the public helpers and comparing.
    /// (Capturing process stderr from inside a unit test is awkward;
    /// the helpers it composes are themselves tested in
    /// `manifest::surface::tests`. Here we verify the project-wiring:
    /// the diff is non-empty when the on-disk lock does not match the
    /// computed one, and empty when they match.)
    #[test]
    fn surface_drift_diff_nonempty_when_committed_lock_is_stale() {
        let dir = unique_dir("m2t12-stale");
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            src_dir.join("main.aer"),
            "pub fn settle(cap: cap[http.post]) { intent \"x\" { http.post(\"u\", \"\\{\\}\") } }",
        )
        .unwrap();
        fs::write(
            dir.join("aeris.toml"),
            "[project]\nname = \"x\"\naeris = \"0.3.0\"\n",
        )
        .unwrap();
        // No .aeris/surface.lock on disk → committed body is empty,
        // computed body is non-empty → diff fires.
        let mut files: Vec<(String, String)> = Vec::new();
        collect_aer_files(&src_dir, &mut files);
        let surface = crate::manifest::compute_surface(&files).unwrap();
        let computed = crate::manifest::surface::render_surface_lock(&surface);
        let diff = crate::manifest::diff_surface_bodies("", &computed);
        assert!(!diff.is_empty(), "expected drift diff, got empty");
        assert!(diff.contains("settle"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn surface_drift_diff_empty_when_lock_matches() {
        let dir = unique_dir("m2t12-fresh");
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            src_dir.join("main.aer"),
            "pub fn settle(cap: cap[http.post]) { intent \"x\" { http.post(\"u\", \"\\{\\}\") } }",
        )
        .unwrap();
        let mut files: Vec<(String, String)> = Vec::new();
        collect_aer_files(&src_dir, &mut files);
        let surface = crate::manifest::compute_surface(&files).unwrap();
        let computed = crate::manifest::surface::render_surface_lock(&surface);
        let diff = crate::manifest::diff_surface_bodies(&computed, &computed);
        assert!(diff.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn surface_drift_renders_first_hunk_format() {
        // Sanity: when stale, the diff includes the unified-diff
        // header (`---` / `+++`) and a `+`-prefixed "settle" entry.
        let computed = "[\"src/main.aer::settle\"]\nfile = \"src/main.aer\"\nfn   = \"settle\"\ncaps = [\"http.post\"]\n\n";
        let diff = crate::manifest::diff_surface_bodies("", computed);
        assert!(diff.starts_with("--- .aeris/surface.lock (committed)"));
        assert!(diff.contains("+++ .aeris/surface.lock (computed)"));
        assert!(diff.contains("+[\"src/main.aer::settle\"]"));
    }

    // ---- M12.T5 / M12.T7 — `aeris fmt` and `aeris fmt --check` ----

    #[test]
    fn cmd_fmt_rewrites_an_unformatted_file() {
        let unformatted = "record   R{x:int}";
        let p = write_temp(unformatted);
        let exit = cmd_fmt(p.to_str().unwrap(), false);
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::SUCCESS));
        let got = fs::read_to_string(&p).unwrap();
        // After fmt the body must round-trip; in particular the
        // contents must equal the canonical form.
        let module = crate::syntax::parse(&got).unwrap();
        let canonical = crate::syntax::fmt::format_module(&module, &got);
        assert_eq!(got, canonical);
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn cmd_fmt_check_exits_one_when_drift_present() {
        let p = write_temp("record   R{x:int}");
        let exit = cmd_fmt(p.to_str().unwrap(), true);
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::from(1)));
        // --check must NOT have rewritten the file.
        let got = fs::read_to_string(&p).unwrap();
        assert_eq!(got, "record   R{x:int}");
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn cmd_fmt_check_exits_zero_on_already_formatted_file() {
        // Compute the canonical form first, write that to disk, and
        // verify --check reports clean.
        let module = crate::syntax::parse("record R { x: int }").unwrap();
        let canonical = crate::syntax::fmt::format_module(&module, "record R { x: int }");
        let p = write_temp(&canonical);
        let exit = cmd_fmt(p.to_str().unwrap(), true);
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::SUCCESS));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn cmd_fmt_narrow_caps_exits_one_on_broad_signature() {
        let p = write_temp(r#"
            fn pay(cap: cap[http]) {
                intent "p" { http.post("https://api.acme.com/x", "\{\}") }
            }
        "#);
        let exit = cmd_fmt_narrow_caps(p.to_str().unwrap());
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::from(1)));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn cmd_fmt_narrow_caps_exits_zero_on_already_minimal_signature() {
        let p = write_temp(r#"
            fn pay(cap: cap[http.post @ "api.acme.com"]) {
                intent "p" { http.post("https://api.acme.com/x", "\{\}") }
            }
        "#);
        let exit = cmd_fmt_narrow_caps(p.to_str().unwrap());
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::SUCCESS));
        let _ = fs::remove_file(&p);
    }

    #[test]
    fn cmd_fmt_handles_directories_recursively() {
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("aeris-fmt-dir-{pid}-{id}"));
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(dir.join("a.aer"), "record   A{x:int}").unwrap();
        fs::write(nested.join("b.aer"), "record   B{y:int}").unwrap();
        let exit = cmd_fmt(dir.to_str().unwrap(), false);
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::SUCCESS));
        let a = fs::read_to_string(dir.join("a.aer")).unwrap();
        let b = fs::read_to_string(nested.join("b.aer")).unwrap();
        assert!(a.contains("record A"));
        assert!(b.contains("record B"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cmd_check_against_a_real_file_returns_correct_exit() {
        let p = write_temp("record R { x: int }");
        let exit = cmd_check(p.to_str().unwrap());
        // ExitCode does not implement PartialEq; compare via debug repr.
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::SUCCESS));
        let _ = fs::remove_file(&p);
    }

    // ---- M16.T4 — `aeris fmt --migrate-strings` ----

    #[test]
    fn m16_migrate_single_interp() {
        let input = r#"fn main() -> string { "hi \(name)" }"#;
        let out = super::migrate_backslash_paren(input).unwrap();
        assert_eq!(out, r#"fn main() -> string { "hi {name}" }"#);
    }

    #[test]
    fn m16_migrate_nested_parens() {
        let input = r#""x = \(f(g(1, 2)))""#;
        let out = super::migrate_backslash_paren(input).unwrap();
        assert_eq!(out, r#""x = {f(g(1, 2))}""#);
    }

    #[test]
    fn m16_migrate_is_idempotent() {
        let input = r#""no legacy here {name}""#;
        let out1 = super::migrate_backslash_paren(input).unwrap();
        let out2 = super::migrate_backslash_paren(&out1).unwrap();
        assert_eq!(out1, input);
        assert_eq!(out2, out1);
    }

    #[test]
    fn m16_migrate_preserves_non_string_braces() {
        // Record literals and block expressions outside string tokens
        // must not be touched.
        let input = "fn main() -> int { let r = R { x: 1 }; r.x }";
        let out = super::migrate_backslash_paren(input).unwrap();
        assert_eq!(out, input);
    }

    // ---- `aeris install` ----

    fn write_project(dir: &Path, name: &str) {
        fs::write(
            dir.join("aeris.toml"),
            format!("[project]\nname = \"{name}\"\naeris = \"0.3.0\"\n"),
        )
        .unwrap();
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(dir.join("src/main.aer"), "fn main(cap) {}\n").unwrap();
    }

    #[test]
    fn install_copies_under_name_from_manifest() {
        let source = unique_dir("install-src");
        let projects = unique_dir("install-projects");
        write_project(&source, "demo-app");

        let dest = super::install_project(&source, &projects).expect("install ok");
        assert_eq!(dest, projects.join("demo-app"));
        assert!(dest.join("aeris.toml").is_file());
        assert!(dest.join("src/main.aer").is_file());
    }

    #[test]
    fn install_replaces_previous_copy() {
        let source = unique_dir("install-src");
        let projects = unique_dir("install-projects");
        write_project(&source, "demo-app");

        super::install_project(&source, &projects).expect("first install");
        // Add a stray file to the destination; a re-install must wipe it.
        let dest = projects.join("demo-app");
        fs::write(dest.join("stale.txt"), "old").unwrap();
        super::install_project(&source, &projects).expect("re-install");
        assert!(!dest.join("stale.txt").exists());
        assert!(dest.join("aeris.toml").is_file());
    }

    #[test]
    fn install_rejects_missing_manifest() {
        let source = unique_dir("install-src");
        let projects = unique_dir("install-projects");
        // No aeris.toml written.
        let err = super::install_project(&source, &projects);
        assert!(matches!(err, Err(super::InstallError::Manifest(_))));
    }

    #[test]
    fn install_rejects_path_traversal_name() {
        let source = unique_dir("install-src");
        let projects = unique_dir("install-projects");
        write_project(&source, "../escape");
        let err = super::install_project(&source, &projects);
        assert!(matches!(err, Err(super::InstallError::Manifest(_))));
        assert!(!projects.parent().unwrap().join("escape").exists());
    }

    #[test]
    fn validate_project_name_accepts_simple_names() {
        assert!(super::validate_project_name("demo").is_ok());
        assert!(super::validate_project_name("my-aeris-project").is_ok());
        assert!(super::validate_project_name("app_2").is_ok());
    }

    #[test]
    fn validate_project_name_rejects_unsafe_names() {
        assert!(super::validate_project_name("").is_err());
        assert!(super::validate_project_name(".").is_err());
        assert!(super::validate_project_name("..").is_err());
        assert!(super::validate_project_name("a/b").is_err());
        assert!(super::validate_project_name("../x").is_err());
        assert!(super::validate_project_name("/abs").is_err());
    }
}
