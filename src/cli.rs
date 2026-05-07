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
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Version => cmd_version(),
        Command::Init => cmd_init(),
        Command::Run { .. }
        | Command::Check { .. }
        | Command::Fmt { .. }
        | Command::Test { .. } => {
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
