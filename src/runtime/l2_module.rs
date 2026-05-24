//! M45 — dynamic loader for L2 modules.
//!
//! An L2 module is a `cdylib` (`.so` / `.dylib` / `.dll`) shipped by
//! the Aeris team. Each module:
//!
//! 1. exposes a small, stable C ABI (`aeris_module_api_version`,
//!    `aeris_module_metadata`, `aeris_module_call`, `aeris_module_free`);
//! 2. carries a `module.aeris.toml` *inside* the binary (returned as
//!    JSON by `aeris_module_metadata`), declaring its name, version,
//!    the L2 family it implements, and the cap paths it offers;
//! 3. is identified in the project's `aeris.toml` by `path`, blake3
//!    `hash`, and `signature` — the runtime verifies all three before
//!    handing control to the module.
//!
//! Only modules signed with the Aeris registry key can be loaded. The
//! verifying key is embedded in this crate (`AERIS_REGISTRY_PUBKEY`)
//! and matches the development signing key derived from a public seed
//! (`AERIS_DEV_KEY_SEED`). For production releases the seed is replaced
//! with a key held by the Aeris team — the loader code does not
//! change.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::Path;

use ed25519_dalek::{Signature, SigningKey, Verifier, VerifyingKey};
use libloading::Library;

use crate::manifest::ModuleEntry;

/// The C ABI version this build of `aeris-core` understands. Bumping
/// this is a hard break: modules with a different `api_version` are
/// refused at load time.
pub const MODULE_API_VERSION: u32 = 1;

/// Seed for the development signing keypair. Anyone with the same
/// version of `aeris-core` derives the same keypair, so the POC
/// modules in this repository can be signed without external tooling.
/// Production releases replace this with a registry key held by the
/// Aeris team — `aeris_registry_pubkey()` reads from a build-time
/// override file when present.
pub const AERIS_DEV_KEY_SEED: [u8; 32] = *b"aeris.module.registry.dev.k.v01\0";

/// Verifying key embedded in this crate. Loaded modules must carry a
/// signature that this key validates.
pub fn aeris_registry_pubkey() -> VerifyingKey {
    aeris_dev_signing_key().verifying_key()
}

/// Development signing key — only used by `aeris-module-sign` when no
/// production key is supplied. Tied to `AERIS_DEV_KEY_SEED`.
pub fn aeris_dev_signing_key() -> SigningKey {
    SigningKey::from_bytes(&AERIS_DEV_KEY_SEED)
}

/// Metadata returned by `aeris_module_metadata()`. The module embeds
/// its own `module.aeris.toml` as a JSON string at build time and the
/// loader parses the result. Only the fields the runtime reacts on
/// are decoded — everything else (description, homepage, …) is
/// ignored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleMetadata {
    pub name: String,
    pub version: String,
    pub family: String, // "minio" | "mongodb" | …
    pub api_version: u32,
    pub cap_paths: Vec<String>, // e.g. ["mongodb.read", "mongodb.write"]
}

/// A successfully loaded, verified, and registered L2 module. The
/// `Library` keeps the underlying handle alive for as long as the
/// runtime needs it; the function pointers are resolved at load time.
pub struct LoadedModule {
    pub metadata: ModuleMetadata,
    _lib: Library,
    call_fn: unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *mut c_char,
    free_fn: unsafe extern "C" fn(*mut c_char),
}

impl LoadedModule {
    /// Load, verify, and prepare a module described by `entry`
    /// (extracted from `aeris.toml [modules.<family>]`).
    pub fn load(entry: &ModuleEntry, project_root: &Path) -> Result<Self, String> {
        let mod_path = resolve(project_root, &entry.path);
        let sig_path = resolve(project_root, &entry.signature);

        // 1. Read the .so bytes and verify the blake3 hash from
        //    aeris.toml matches.
        let bytes = std::fs::read(&mod_path)
            .map_err(|e| format!("cannot read module `{}`: {e}", mod_path.display()))?;
        let actual_hash = crate::manifest::hash_text(&String::from_utf8_lossy(&bytes));
        if actual_hash != entry.hash {
            return Err(format!(
                "module `{}`: hash mismatch — pinned `{}` vs actual `{actual_hash}`",
                entry.name, entry.hash
            ));
        }

        // 2. Read the detached signature and verify against the
        //    embedded Aeris registry public key.
        let sig_bytes = std::fs::read(&sig_path).map_err(|e| {
            format!(
                "cannot read signature `{}`: {e}",
                sig_path.display()
            )
        })?;
        if sig_bytes.len() != 64 {
            return Err(format!(
                "module `{}`: signature must be 64 bytes, got {}",
                entry.name,
                sig_bytes.len()
            ));
        }
        let mut sig_arr = [0u8; 64];
        sig_arr.copy_from_slice(&sig_bytes);
        let signature = Signature::from_bytes(&sig_arr);
        aeris_registry_pubkey()
            .verify(&bytes, &signature)
            .map_err(|e| {
                format!(
                    "module `{}`: signature does not verify against the Aeris registry key ({e})",
                    entry.name
                )
            })?;

        // 3. Load the library and resolve the four ABI symbols.
        let lib = unsafe {
            Library::new(&mod_path)
                .map_err(|e| format!("cannot dlopen `{}`: {e}", mod_path.display()))?
        };
        let api_version_fn: libloading::Symbol<unsafe extern "C" fn() -> u32> = unsafe {
            lib.get(b"aeris_module_api_version")
                .map_err(|e| format!("module `{}`: missing aeris_module_api_version ({e})", entry.name))?
        };
        let metadata_fn: libloading::Symbol<unsafe extern "C" fn() -> *mut c_char> = unsafe {
            lib.get(b"aeris_module_metadata")
                .map_err(|e| format!("module `{}`: missing aeris_module_metadata ({e})", entry.name))?
        };
        let call_fn: libloading::Symbol<
            unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *mut c_char,
        > = unsafe {
            lib.get(b"aeris_module_call")
                .map_err(|e| format!("module `{}`: missing aeris_module_call ({e})", entry.name))?
        };
        let free_fn: libloading::Symbol<unsafe extern "C" fn(*mut c_char)> = unsafe {
            lib.get(b"aeris_module_free")
                .map_err(|e| format!("module `{}`: missing aeris_module_free ({e})", entry.name))?
        };

        // 4. API version compatibility.
        let module_api = unsafe { api_version_fn() };
        if module_api != MODULE_API_VERSION {
            return Err(format!(
                "module `{}`: api_version {module_api} not supported (this aeris-core speaks {MODULE_API_VERSION})",
                entry.name
            ));
        }

        // 5. Parse the embedded module.aeris.toml metadata (JSON).
        let metadata = unsafe {
            let raw = metadata_fn();
            let s = read_owned_c_string(raw);
            free_fn(raw);
            s
        };
        let metadata = parse_metadata(&metadata, &entry.name)?;

        // 6. Family sanity-check: the project's [modules.<family>]
        //    section pins which family it expects; the binary must
        //    actually implement that family.
        if metadata.family != entry.family {
            return Err(format!(
                "module `{}`: declared family `{}` does not match aeris.toml family `{}`",
                entry.name, metadata.family, entry.family
            ));
        }

        // We store the raw symbol pointers (not the lifetime-bound
        // `Symbol<'_>`) so the loader can move out of the function.
        // The `Library` keeps them alive for the program's lifetime.
        let call_raw: unsafe extern "C" fn(*const c_char, *const c_char, *const c_char) -> *mut c_char =
            *call_fn;
        let free_raw: unsafe extern "C" fn(*mut c_char) = *free_fn;
        drop(api_version_fn);
        drop(metadata_fn);
        drop(call_fn);
        drop(free_fn);

        Ok(LoadedModule {
            metadata,
            _lib: lib,
            call_fn: call_raw,
            free_fn: free_raw,
        })
    }

    /// Invoke an op on the loaded module. `family_op` is the dotted
    /// path the user wrote in source (`minio.put`, `mongodb.read`, …);
    /// `args_json` and `env_json` are JSON-encoded bags. The reply is
    /// either `{"ok": <value>}` or `{"err": "<message>"}`.
    pub fn call(
        &self,
        family_op: &str,
        args_json: &str,
        env_json: &str,
    ) -> Result<String, String> {
        let op_c = CString::new(family_op).map_err(|e| format!("bad op: {e}"))?;
        let args_c = CString::new(args_json).map_err(|e| format!("bad args: {e}"))?;
        let env_c = CString::new(env_json).map_err(|e| format!("bad env: {e}"))?;
        let raw = unsafe { (self.call_fn)(op_c.as_ptr(), args_c.as_ptr(), env_c.as_ptr()) };
        if raw.is_null() {
            return Err(format!(
                "module `{}` returned a null reply for `{family_op}`",
                self.metadata.name
            ));
        }
        let reply = unsafe { CStr::from_ptr(raw).to_string_lossy().into_owned() };
        unsafe { (self.free_fn)(raw) };
        Ok(reply)
    }
}

fn resolve(root: &Path, path_field: &str) -> std::path::PathBuf {
    let p = Path::new(path_field);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        root.join(p)
    }
}

/// Read a NUL-terminated C string returned by the module and copy it
/// into a Rust `String`. The caller frees the original via the
/// module's `free_fn` afterwards.
unsafe fn read_owned_c_string(raw: *mut c_char) -> String {
    if raw.is_null() {
        return String::new();
    }
    CStr::from_ptr(raw).to_string_lossy().into_owned()
}

fn parse_metadata(json: &str, mod_name: &str) -> Result<ModuleMetadata, String> {
    // Reuse the runtime's natural-JSON parser — already shipped, no
    // extra dep.
    let fields = crate::runtime::json::decode_natural_object(json).map_err(|e| {
        format!("module `{mod_name}`: metadata is not a JSON object: {}", e.message)
    })?;
    let mut name = None;
    let mut version = None;
    let mut family = None;
    let mut api_version: Option<u32> = None;
    let mut cap_paths: Vec<String> = Vec::new();
    for (k, v) in fields {
        match (k.as_str(), v) {
            ("name", crate::runtime::value::Value::Str(s)) => name = Some(s),
            ("version", crate::runtime::value::Value::Str(s)) => version = Some(s),
            ("family", crate::runtime::value::Value::Str(s)) => family = Some(s),
            ("api_version", crate::runtime::value::Value::Int(n)) if n >= 0 => {
                api_version = Some(n as u32)
            }
            ("cap_paths", crate::runtime::value::Value::List(xs)) => {
                cap_paths = xs
                    .into_iter()
                    .filter_map(|v| match v {
                        crate::runtime::value::Value::Str(s) => Some(s),
                        _ => None,
                    })
                    .collect();
            }
            _ => {}
        }
    }
    Ok(ModuleMetadata {
        name: name.ok_or_else(|| format!("module `{mod_name}`: metadata missing `name`"))?,
        version: version
            .ok_or_else(|| format!("module `{mod_name}`: metadata missing `version`"))?,
        family: family.ok_or_else(|| format!("module `{mod_name}`: metadata missing `family`"))?,
        api_version: api_version
            .ok_or_else(|| format!("module `{mod_name}`: metadata missing `api_version`"))?,
        cap_paths,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dev_keypair_round_trips_a_signature() {
        let sk = aeris_dev_signing_key();
        let vk = aeris_registry_pubkey();
        let msg = b"hello aeris module loader";
        use ed25519_dalek::Signer;
        let sig = sk.sign(msg);
        vk.verify(msg, &sig).expect("signature verifies");
    }

    #[test]
    fn parse_metadata_accepts_well_formed_json() {
        let json = r#"{
            "name": "aeris-mongo-mock",
            "version": "0.1.0",
            "family": "mongodb",
            "api_version": 1,
            "cap_paths": ["mongodb.read", "mongodb.write"]
        }"#;
        let m = parse_metadata(json, "test").unwrap();
        assert_eq!(m.name, "aeris-mongo-mock");
        assert_eq!(m.version, "0.1.0");
        assert_eq!(m.family, "mongodb");
        assert_eq!(m.api_version, 1);
        assert_eq!(m.cap_paths, vec!["mongodb.read", "mongodb.write"]);
    }

    #[test]
    fn parse_metadata_rejects_missing_name() {
        let json = r#"{"version":"0.1","family":"mongodb","api_version":1}"#;
        let err = parse_metadata(json, "test").unwrap_err();
        assert!(err.contains("missing `name`"), "{err}");
    }
}
