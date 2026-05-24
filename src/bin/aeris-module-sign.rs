//! `aeris-module-sign <module.so> [<out.sig>]`
//!
//! Sign a compiled L2 module with the Aeris dev signing key (the
//! one derived from `AERIS_DEV_KEY_SEED`). Writes the 64-byte
//! detached ed25519 signature next to the module (`<module>.sig`)
//! unless an explicit output path is given.
//!
//! Module authors use this once per release. In production the
//! key seed is replaced; this binary is unchanged.

use std::path::PathBuf;
use std::process::ExitCode;

use aeris::runtime::l2_module::aeris_dev_signing_key;
use ed25519_dalek::Signer;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let module_path = match args.next() {
        Some(p) => PathBuf::from(p),
        None => {
            eprintln!("usage: aeris-module-sign <module.so> [<out.sig>]");
            return ExitCode::from(1);
        }
    };
    let out_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let mut p = module_path.clone();
            let stem = p.file_name().map(|s| s.to_owned()).unwrap_or_default();
            let mut name = stem.to_string_lossy().into_owned();
            name.push_str(".sig");
            p.set_file_name(name);
            p
        });

    let bytes = match std::fs::read(&module_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("aeris-module-sign: cannot read {}: {e}", module_path.display());
            return ExitCode::from(1);
        }
    };

    let signing_key = aeris_dev_signing_key();
    let signature = signing_key.sign(&bytes);
    if let Err(e) = std::fs::write(&out_path, signature.to_bytes()) {
        eprintln!(
            "aeris-module-sign: cannot write {}: {e}",
            out_path.display()
        );
        return ExitCode::from(1);
    }

    // Convenience: also print the blake3 hash so the caller can
    // paste it straight into `aeris.toml [modules.<family>].hash`.
    let hash = aeris::manifest::hash_text(&String::from_utf8_lossy(&bytes));
    println!("signed: {}", module_path.display());
    println!("  → signature: {}", out_path.display());
    println!("  → hash:      {hash}");
    ExitCode::SUCCESS
}
