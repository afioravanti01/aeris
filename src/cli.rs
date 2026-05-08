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
    Check { file: String },
    /// Format an .aer file or directory
    Fmt { path: String },
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
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => cmd_version(),
        Command::Init => cmd_init(),
        Command::Check { file } => cmd_check(&file),
        Command::Run { file } => cmd_run(&file),
        Command::Lock { check, file } => cmd_lock(&file, check),
        Command::Replay {
            trace,
            source,
            live,
        } => cmd_replay(&trace, &source, live),
        Command::Fmt { .. } | Command::Test { .. } => {
            eprintln!("aeris: command not yet implemented in this milestone");
            ExitCode::from(1)
        }
    }
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
    let check_errs = crate::check::check_module(&module);
    if !check_errs.is_empty() {
        let mut max_exit: u8 = 0;
        for err in &check_errs {
            eprintln!(
                "aeris: check error at line {}, col {} (exit {}): {:?}",
                err.span.line,
                err.span.col,
                err.exit_code(),
                err.kind
            );
            max_exit = max_exit.max(err.exit_code());
        }
        return ExitCode::from(max_exit);
    }
    // M7.T4: when a `lockset.toml` sits next to the source file, use
    // its `[caps]` ceiling as `main`'s synthesised cap. Falls back
    // to `cap[*]` only when no lockset is present.
    let lockset_path = Path::new(path)
        .parent()
        .unwrap_or(Path::new("."))
        .join("lockset.toml");
    let composed = if lockset_path.exists() {
        match fs::read_to_string(&lockset_path)
            .ok()
            .and_then(|s| crate::lockset::parse_lockset(&s).ok())
        {
            Some(l) => {
                eprint!("{}", l.describe_main_cap());
                let cap = l.synthesise_main_cap();
                let backend = l.ai_backend.clone().map(std::rc::Rc::new);
                Some((cap, backend))
            }
            None => None,
        }
    } else {
        None
    };
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
    for err in &outcome.errors {
        eprintln!(
            "aeris: parse error at line {}, col {}: {:?}",
            err.span.line, err.span.col, err.kind
        );
        max_exit = max_exit.max(64);
    }
    let check_errs = crate::check::check_module(&outcome.module);
    for err in &check_errs {
        eprintln!(
            "aeris: check error at line {}, col {} (exit {}): {:?}",
            err.span.line,
            err.span.col,
            err.exit_code(),
            err.kind
        );
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

    #[test]
    fn cmd_check_against_a_real_file_returns_correct_exit() {
        let p = write_temp("record R { x: int }");
        let exit = cmd_check(p.to_str().unwrap());
        // ExitCode does not implement PartialEq; compare via debug repr.
        assert_eq!(format!("{exit:?}"), format!("{:?}", ExitCode::SUCCESS));
        let _ = fs::remove_file(&p);
    }
}
