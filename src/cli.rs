//! Aeris CLI dispatch.
//!
//! See `docs/language.md` § 25 for the command surface.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

const VERSION: &str = "0.2.0-dev";

const TEMPLATE_LOCKSET: &str = include_str!("templates/lockset.toml");
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
    /// Compile and run an .aer file
    Run { file: String },
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
    },
    /// Run tests
    Test { path: Option<String> },
    /// `aeris lock` — recompute `lockset.toml` / `.aeris/surface.lock`.
    /// `--check` exits 69 if the lock is stale (CI mode, M7.T7).
    Lock {
        #[arg(long)]
        check: bool,
        /// Optional `lockset.toml` path; defaults to the cwd.
        #[arg(default_value = "lockset.toml")]
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
        Command::Check { file, explain } => match (explain, file) {
            (Some(code), _) => cmd_check_explain(code),
            (None, Some(path)) => cmd_check(&path),
            (None, None) => {
                eprintln!("aeris: `aeris check <file>` or `aeris check --explain <code>`");
                ExitCode::from(1)
            }
        },
        Command::Run { file } => cmd_run(&file),
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
        Command::Fmt {
            path,
            check,
            narrow_caps,
        } => {
            if narrow_caps {
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
    let report = crate::test_harness::run_suites_explicit(&suites);
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

/// `aeris run <file>` — pure-interpreter driver (M3.T6). Exit codes
/// follow `language.md` § 25.3:
///   0  → `main()` returned cleanly (any value, typically `Ok(())`)
///   64 → parse / type / check error
///   1  → uncaught `Err(...)` or `raise <value>`
fn cmd_run(path: &str) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {path}: {e}");
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
    // M7.T4 + M15: when a `lockset.toml` sits next to the source file,
    // use its `[caps]` ceiling as `main`'s synthesised cap, and route
    // the static checker through `check_module_with_lockset` so the
    // `required` flag (§ 8.4.1) is honoured.
    let lockset_path = Path::new(path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("lockset.toml");
    let lockset_for_check = fs::read_to_string(&lockset_path)
        .ok()
        .and_then(|s| crate::lockset::parse_lockset(&s).ok());
    let check_errs = match &lockset_for_check {
        Some(l) => crate::check::check_module_with_lockset(&module, &l.caps),
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
    let composed = lockset_for_check.as_ref().map(|l| {
        eprint!("{}", l.describe_main_cap());
        let cap = l.synthesise_main_cap();
        let backend = l.ai_backend.clone().map(std::rc::Rc::new);
        (cap, backend)
    });
    let outcome = if let Some((cap, backend)) = composed {
        crate::runtime::eval::run_main_with_cfg(&module, cap, None, backend, false)
    } else {
        crate::runtime::run_main(&module)
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
    // Compose `main`'s cap from a co-located `lockset.toml` if any —
    // otherwise fall back to `cap[*]`. Replay does not require a
    // lockset (the original run's recording stands in for it).
    let lockset_path = Path::new(source_path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("lockset.toml");
    let cap = if lockset_path.exists() {
        fs::read_to_string(&lockset_path)
            .ok()
            .and_then(|s| crate::lockset::parse_lockset(&s).ok())
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
    let outcome = crate::syntax::parse_recovering(&src);
    let mut max_exit: u8 = 0;
    // M2.T12: surface drift is the first hunk — printed *before* any
    // parse / type / cap diagnostics so reviewers see authority changes
    // first (`thesis.md` § 13 / `language.md` § 8.6).
    let project_root = Path::new(path).parent().unwrap_or(Path::new("."));
    let lockset_path = project_root.join("lockset.toml");
    let lockset_loaded = fs::read_to_string(&lockset_path)
        .ok()
        .and_then(|s| crate::lockset::parse_lockset(&s).ok());
    if lockset_loaded.is_some() {
        emit_surface_drift_hunk(project_root);
    }
    for err in &outcome.errors {
        eprintln!(
            "aeris: parse error at line {}, col {}: {:?}",
            err.span.line, err.span.col, err.kind
        );
        max_exit = max_exit.max(64);
    }
    // M2.T6: when `lockset.toml` sits next to the source, run the
    // allow-list intersection check (§ 8.3.2). Out-of-ceiling entries
    // are surfaced with exit code 71. A missing lockset is not an
    // error here — `aeris check` falls back to the standalone pass.
    let check_errs = match lockset_loaded {
        Some(l) => crate::check::check_module_with_lockset(&outcome.module, &l.caps),
        None => crate::check::check_module(&outcome.module),
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
/// lockset, validates the structure, and (in `--check`) compares the
/// computed `.aeris/surface.lock` against the committed file. Exit
/// 69 on any drift / parse / validation failure.
fn cmd_lock(path: &str, check: bool) -> ExitCode {
    let src = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: cannot read {path}: {e}");
            return ExitCode::from(crate::lockset::EXIT_LOCKSET_ERROR);
        }
    };
    let lockset = match crate::lockset::parse_lockset(&src) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("aeris: lockset error: {e}");
            return ExitCode::from(crate::lockset::EXIT_LOCKSET_ERROR);
        }
    };
    eprintln!(
        "aeris: lockset `{}` ok ({} deps, {} policies)",
        lockset.project.name,
        lockset.deps.len(),
        lockset.policies.len()
    );
    let project_root = Path::new(path).parent().unwrap_or(Path::new("."));
    // M7.T2: re-hash every `path = "..."` dep and compare against
    // the pinned `blake3:...`. Mismatch (or missing file) → exit 69.
    if let Err(errs) = crate::lockset::verify_local_deps(&lockset, project_root) {
        for e in errs {
            eprintln!("aeris: {e}");
        }
        return ExitCode::from(crate::lockset::EXIT_LOCKSET_ERROR);
    }
    // Compute the surface lock from src/**/*.aer (best effort: we
    // walk the conventional `src/` tree if present).
    let src_dir = project_root.join("src");
    let mut files: Vec<(String, String)> = Vec::new();
    if src_dir.exists() {
        collect_aer_files(&src_dir, &mut files);
    }
    let surface = match crate::lockset::compute_surface(&files) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("aeris: surface compute failed: {e}");
            return ExitCode::from(crate::lockset::EXIT_LOCKSET_ERROR);
        }
    };
    let surface_path = project_root.join(".aeris/surface.lock");
    let new_body = crate::lockset::surface::render_surface_lock(&surface);
    if check {
        let on_disk = fs::read_to_string(&surface_path).unwrap_or_default();
        if on_disk != new_body {
            eprintln!("aeris: surface.lock is stale (run `aeris lock` to refresh)");
            return ExitCode::from(crate::lockset::EXIT_LOCKSET_ERROR);
        }
        eprintln!("aeris: surface.lock matches");
    } else if let Err(e) = crate::lockset::write_surface_lock(&surface, &surface_path) {
        eprintln!("aeris: cannot write surface.lock: {e}");
        return ExitCode::from(crate::lockset::EXIT_LOCKSET_ERROR);
    } else {
        eprintln!("aeris: wrote {}", surface_path.display());
    }
    ExitCode::SUCCESS
}

/// M2.T12: when `aeris check` runs in a project (lockset.toml present),
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
    let surface = match crate::lockset::compute_surface(&files) {
        Ok(s) => s,
        Err(_) => return,
    };
    let computed = crate::lockset::surface::render_surface_lock(&surface);
    let on_disk = fs::read_to_string(project_root.join(".aeris/surface.lock")).unwrap_or_default();
    let diff = crate::lockset::diff_surface_bodies(&on_disk, &computed);
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
    let lockset = root.join("lockset.toml");
    let src_dir = root.join("src");
    let main_aer = src_dir.join("main.aer");

    if lockset.exists() || main_aer.exists() {
        return Err("project files already exist; refusing to overwrite".into());
    }

    fs::create_dir_all(&src_dir).map_err(|e| format!("cannot create src/: {e}"))?;
    fs::write(&lockset, TEMPLATE_LOCKSET).map_err(|e| format!("cannot write lockset.toml: {e}"))?;
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
                    do { http.post("u", "{}")? }
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
    /// `lockset::surface::tests`. Here we verify the project-wiring:
    /// the diff is non-empty when the on-disk lock does not match the
    /// computed one, and empty when they match.)
    #[test]
    fn surface_drift_diff_nonempty_when_committed_lock_is_stale() {
        let dir = unique_dir("m2t12-stale");
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            src_dir.join("main.aer"),
            "pub fn settle(cap: cap[http.post]) { intent \"x\" { http.post(\"u\", \"{}\") } }",
        )
        .unwrap();
        fs::write(
            dir.join("lockset.toml"),
            "[project]\nname = \"x\"\naeris = \"0.2.0\"\n",
        )
        .unwrap();
        // No .aeris/surface.lock on disk → committed body is empty,
        // computed body is non-empty → diff fires.
        let mut files: Vec<(String, String)> = Vec::new();
        collect_aer_files(&src_dir, &mut files);
        let surface = crate::lockset::compute_surface(&files).unwrap();
        let computed = crate::lockset::surface::render_surface_lock(&surface);
        let diff = crate::lockset::diff_surface_bodies("", &computed);
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
            "pub fn settle(cap: cap[http.post]) { intent \"x\" { http.post(\"u\", \"{}\") } }",
        )
        .unwrap();
        let mut files: Vec<(String, String)> = Vec::new();
        collect_aer_files(&src_dir, &mut files);
        let surface = crate::lockset::compute_surface(&files).unwrap();
        let computed = crate::lockset::surface::render_surface_lock(&surface);
        let diff = crate::lockset::diff_surface_bodies(&computed, &computed);
        assert!(diff.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn surface_drift_renders_first_hunk_format() {
        // Sanity: when stale, the diff includes the unified-diff
        // header (`---` / `+++`) and a `+`-prefixed "settle" entry.
        let computed = "[\"src/main.aer::settle\"]\nfile = \"src/main.aer\"\nfn   = \"settle\"\ncaps = [\"http.post\"]\n\n";
        let diff = crate::lockset::diff_surface_bodies("", computed);
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
                intent "p" { http.post("https://api.acme.com/x", "{}") }
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
                intent "p" { http.post("https://api.acme.com/x", "{}") }
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
}
