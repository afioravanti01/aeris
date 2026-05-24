//! M45 — end-to-end test for the dynamic module loader.
//!
//! Layout:
//!   1. Build the POC `aeris-mongo-mock` cdylib (cargo handles it
//!      if the artefact is already there; otherwise we shell out
//!      to `cargo build --release` from inside the module dir).
//!   2. Sign it with the dev key via `aeris-module-sign`.
//!   3. Set up a temporary project with an `aeris.toml` that pins
//!      the module by path + hash + signature.
//!   4. `aeris run` a `main.aer` that calls `mongodb.write` then
//!      `mongodb.read`.
//!   5. Assert the JSONL store has the expected line.

use std::path::PathBuf;
use std::process::Command;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn aeris_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_aeris"))
}

fn aeris_module_sign_binary() -> PathBuf {
    // `CARGO_BIN_EXE_<name>` works for bins declared in `[[bin]]`.
    PathBuf::from(env!("CARGO_BIN_EXE_aeris-module-sign"))
}

fn module_dylib_path() -> PathBuf {
    // Build the POC up-front so the test is hermetic.
    let mod_dir = repo_root().join("modules/aeris-mongo-mock");
    let status = Command::new("cargo")
        .args(["build", "--release"])
        .current_dir(&mod_dir)
        .status()
        .expect("spawn cargo build for aeris-mongo-mock");
    assert!(status.success(), "cargo build failed for aeris-mongo-mock");

    let suffix = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };
    mod_dir
        .join("target/release")
        .join(format!("libaeris_mongo_mock.{suffix}"))
}

fn tmp_dir(label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("aeris-m45-{label}-{pid}-{nanos}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn loaded_module_handles_mongodb_write_then_read() {
    let project = tmp_dir("project");
    let module_src = module_dylib_path();
    let module_dst = project.join("aeris-mongo-mock.dylib");
    let sig_dst = project.join("aeris-mongo-mock.dylib.sig");

    std::fs::copy(&module_src, &module_dst).expect("copy dylib");

    // Sign the copy in place so the signature matches the bytes
    // the loader will read.
    let sign_out = Command::new(aeris_module_sign_binary())
        .arg(&module_dst)
        .arg(&sig_dst)
        .output()
        .expect("spawn aeris-module-sign");
    assert!(
        sign_out.status.success(),
        "sign failed: {}",
        String::from_utf8_lossy(&sign_out.stderr)
    );
    // The signer prints the blake3 hash on the third line — easier
    // to scrape than recomputing it in the test.
    let stdout = String::from_utf8_lossy(&sign_out.stdout);
    let hash_line = stdout
        .lines()
        .find(|l| l.contains("hash:"))
        .expect("hash line in signer output");
    let hash = hash_line
        .split_whitespace()
        .last()
        .expect("hash token")
        .to_string();

    let aeris_toml = format!(
        r#"[project]
name = "m45-poc"
aeris = "0.3.0"

[caps]
enforce = "off"

[modules.mongodb]
path      = "aeris-mongo-mock.dylib"
hash      = "{hash}"
signature = "aeris-mongo-mock.dylib.sig"
"#
    );
    std::fs::write(project.join("aeris.toml"), aeris_toml).unwrap();

    std::fs::write(
        project.join("main.aer"),
        r#"
use mongodb, io

fn main(cap) -> result<unit> {
  intent "write then read via dynamically loaded mongodb module" {
    mongodb.write("invoices", { id: 1, amount: 42, customer: "alice" })?
    mongodb.write("invoices", { id: 2, amount: 99, customer: "bob"   })?
    let xs = mongodb.read("invoices", {})?
    io.println("read {len(xs)} docs from invoices")
  }
  Ok(())
}
"#,
    )
    .unwrap();

    let out = Command::new(aeris_binary())
        .arg("run")
        .arg(project.join("main.aer"))
        .current_dir(&project)
        .output()
        .expect("spawn aeris run");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "aeris run failed: exit {:?}\nstdout: {stdout}\nstderr: {stderr}",
        out.status.code()
    );
    assert!(
        stdout.contains("read 2 docs from invoices"),
        "unexpected stdout: {stdout}"
    );

    // The mock module appends to .aeris/mongo-store/invoices.jsonl
    // relative to the *process* cwd (we set it to `project`).
    let store = project.join(".aeris/mongo-store/invoices.jsonl");
    assert!(
        store.is_file(),
        "store not created at {}",
        store.display()
    );
    let body = std::fs::read_to_string(&store).unwrap();
    let lines: Vec<&str> = body.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2, "expected two appended docs, got {}", lines.len());
    assert!(lines[0].contains("\"customer\":\"alice\""));
    assert!(lines[1].contains("\"customer\":\"bob\""));
}

#[test]
fn loader_rejects_tampered_module() {
    // Same setup as the happy-path test, but we mutate the dylib
    // *after* signing — the hash check should fire and the run
    // should fail before any user code executes.
    let project = tmp_dir("tamper");
    let module_src = module_dylib_path();
    let module_dst = project.join("aeris-mongo-mock.dylib");
    let sig_dst = project.join("aeris-mongo-mock.dylib.sig");

    std::fs::copy(&module_src, &module_dst).unwrap();
    let sign_out = Command::new(aeris_module_sign_binary())
        .arg(&module_dst)
        .arg(&sig_dst)
        .output()
        .expect("spawn signer");
    assert!(sign_out.status.success());
    let stdout = String::from_utf8_lossy(&sign_out.stdout);
    let hash = stdout
        .lines()
        .find(|l| l.contains("hash:"))
        .and_then(|l| l.split_whitespace().last())
        .expect("hash")
        .to_string();

    // Tamper: append a stray byte to the dylib after the signer ran.
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&module_dst)
        .unwrap();
    f.write_all(b"\x00").unwrap();
    drop(f);

    let aeris_toml = format!(
        r#"[project]
name = "m45-tamper"
aeris = "0.3.0"

[caps]
enforce = "off"

[modules.mongodb]
path      = "aeris-mongo-mock.dylib"
hash      = "{hash}"
signature = "aeris-mongo-mock.dylib.sig"
"#
    );
    std::fs::write(project.join("aeris.toml"), aeris_toml).unwrap();
    std::fs::write(
        project.join("main.aer"),
        "use io\nfn main(cap) { io.println(\"never reached\") }\n",
    )
    .unwrap();

    let out = Command::new(aeris_binary())
        .arg("run")
        .arg(project.join("main.aer"))
        .current_dir(&project)
        .output()
        .expect("spawn aeris run");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "aeris run unexpectedly succeeded on a tampered module\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("hash mismatch") || stderr.contains("module loading failed"),
        "expected a hash-mismatch diagnostic, got:\n{stderr}"
    );
}
