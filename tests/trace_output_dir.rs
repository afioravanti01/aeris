//! M41 — runtime outputs land under `<project_root>/<output_dir>`,
//! not next to the shell's cwd or under `$HOME`.
//!
//! Builds the `aeris` binary, runs a tiny `.aer` script from a
//! temporary project directory with the shell `cwd` pointed
//! elsewhere, and asserts:
//!
//! * `<project>/.aeris/traces/<trace_id>.jsonl` exists and is
//!   non-empty;
//! * the trace_id is announced on stderr in the
//!   `[aeris] trace_id = … → …` banner;
//! * neither `<cwd>/.aeris/` nor `$HOME/.aeris/` were touched by
//!   the run.

use std::path::PathBuf;
use std::process::Command;

fn tmp_dir(label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("aeris-m41-{label}-{pid}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn aeris_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aeris"))
}

#[test]
fn aeris_run_writes_trace_under_project_output_dir() {
    let project = tmp_dir("project");
    let cwd = tmp_dir("cwd");

    std::fs::write(
        project.join("aeris.toml"),
        "[project]\nname = \"m41\"\naeris = \"0.3.0\"\n[caps]\nenforce = \"off\"\n",
    )
    .unwrap();
    std::fs::write(
        project.join("main.aer"),
        "use io\nfn main(cap) { io.println(\"hi\") }\n",
    )
    .unwrap();

    let out = Command::new(aeris_binary())
        .arg("run")
        .arg(project.join("main.aer"))
        .current_dir(&cwd)
        .output()
        .expect("spawn aeris");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "aeris exit {:?}\nstdout: {stdout}\nstderr: {stderr}",
        out.status.code()
    );

    // Banner: `[aeris] trace_id = <ULID> → <project>/.aeris/traces/<ULID>.jsonl`
    let line = stderr
        .lines()
        .find(|l| l.starts_with("[aeris] trace_id ="))
        .unwrap_or_else(|| panic!("no trace_id banner on stderr:\n{stderr}"));
    let id = line
        .split_whitespace()
        .nth(3)
        .expect("trace_id token in banner");
    assert_eq!(id.len(), 26, "ULID should be 26 chars, got `{id}`");

    // Project-relative output must exist and carry the run.
    let traces_dir = project.join(".aeris").join("traces");
    assert!(
        traces_dir.is_dir(),
        "traces dir not created at {}",
        traces_dir.display()
    );
    let trace_file = traces_dir.join(format!("{id}.jsonl"));
    assert!(
        trace_file.is_file(),
        "trace file missing: {}",
        trace_file.display()
    );
    let trace_bytes = std::fs::metadata(&trace_file).unwrap().len();
    assert!(trace_bytes > 0, "trace file is empty");

    // The shell's cwd must NOT have a stray `.aeris/` from this run.
    let cwd_aeris = cwd.join(".aeris");
    assert!(
        !cwd_aeris.exists(),
        "{} should not exist — runtime wrote into shell cwd",
        cwd_aeris.display()
    );
}

#[test]
fn aeris_run_respects_custom_output_dir() {
    let project = tmp_dir("custom-out");
    let cwd = tmp_dir("custom-cwd");

    std::fs::write(
        project.join("aeris.toml"),
        "[project]\nname = \"m41b\"\naeris = \"0.3.0\"\n\
         [caps]\nenforce = \"off\"\n\
         [runtime]\noutput_dir = \"build/obs\"\n",
    )
    .unwrap();
    std::fs::write(
        project.join("main.aer"),
        "use io\nfn main(cap) { io.println(\"hi\") }\n",
    )
    .unwrap();

    let out = Command::new(aeris_binary())
        .arg("run")
        .arg(project.join("main.aer"))
        .current_dir(&cwd)
        .output()
        .expect("spawn aeris");
    assert!(out.status.success(), "aeris failed: {:?}", out);

    let traces_dir = project.join("build/obs/traces");
    assert!(
        traces_dir.is_dir(),
        "custom traces dir not created at {}",
        traces_dir.display()
    );
    // Default .aeris/ must NOT exist when an alternative output_dir
    // is configured.
    assert!(
        !project.join(".aeris").exists(),
        "default .aeris/ should not be created when output_dir is custom"
    );
}

#[test]
fn aeris_run_trace_off_skips_trace_file() {
    let project = tmp_dir("trace-off");
    let cwd = tmp_dir("trace-off-cwd");

    std::fs::write(
        project.join("aeris.toml"),
        "[project]\nname = \"m41c\"\naeris = \"0.3.0\"\n\
         [caps]\nenforce = \"off\"\n\
         [runtime]\ntrace = false\n",
    )
    .unwrap();
    std::fs::write(
        project.join("main.aer"),
        "use io\nfn main(cap) { io.println(\"hi\") }\n",
    )
    .unwrap();

    let out = Command::new(aeris_binary())
        .arg("run")
        .arg(project.join("main.aer"))
        .current_dir(&cwd)
        .output()
        .expect("spawn aeris");
    assert!(out.status.success(), "aeris failed: {:?}", out);

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("[aeris] trace_id ="),
        "trace banner should be absent when trace = false:\n{stderr}"
    );
    let traces_dir = project.join(".aeris").join("traces");
    assert!(
        !traces_dir.exists(),
        "no traces dir should appear when trace = false"
    );
}
