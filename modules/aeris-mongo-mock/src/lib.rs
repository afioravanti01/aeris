//! POC L2 module for Aeris (M45).
//!
//! Implements the `mongodb` family by appending JSON documents to a
//! local JSONL file — same semantics as the in-tree `RealMongo`, but
//! delivered as a dynamically loadable `.so`. The point of this
//! crate is to exercise the M45 loader end-to-end: blake3 hash
//! check, ed25519 signature check, ABI symbol resolution, JSON RPC
//! over the C boundary.
//!
//! ABI (`extern "C"`):
//!   uint32_t      aeris_module_api_version(void);
//!   char*         aeris_module_metadata(void);
//!   char*         aeris_module_call(const char* op,
//!                                   const char* args_json,
//!                                   const char* env_json);
//!   void          aeris_module_free(char* ptr);
//!
//! All strings are NUL-terminated UTF-8, allocated by the module
//! and freed by `aeris_module_free`. Replies are JSON objects with
//! either `{"ok": <value>}` or `{"err": "<message>"}`.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::path::PathBuf;

/// Must match `aeris-core`'s `MODULE_API_VERSION`. Bumping it is a
/// hard break.
#[no_mangle]
pub extern "C" fn aeris_module_api_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn aeris_module_metadata() -> *mut c_char {
    // Mirrors the JSON shape parsed by `runtime::l2_module`.
    let json = r#"{
  "name": "aeris-mongo-mock",
  "version": "0.1.0",
  "family": "mongodb",
  "api_version": 1,
  "cap_paths": ["mongodb.read", "mongodb.write"]
}"#;
    CString::new(json).unwrap().into_raw()
}

#[no_mangle]
pub extern "C" fn aeris_module_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let _ = CString::from_raw(ptr);
    }
}

#[no_mangle]
pub extern "C" fn aeris_module_call(
    op: *const c_char,
    args_json: *const c_char,
    _env_json: *const c_char,
) -> *mut c_char {
    let reply = match (read_cstr(op), read_cstr(args_json)) {
        (Some(op), Some(args)) => dispatch(&op, &args),
        _ => err_reply("aeris_module_call: null op or args"),
    };
    CString::new(reply).unwrap_or_else(|_| CString::new("{\"err\":\"reply contained NUL\"}").unwrap()).into_raw()
}

fn read_cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    unsafe { Some(CStr::from_ptr(p).to_string_lossy().into_owned()) }
}

fn dispatch(op: &str, args: &str) -> String {
    match op {
        "mongodb.write" => mongodb_write(args),
        "mongodb.read" => mongodb_read(args),
        other => err_reply(&format!("aeris-mongo-mock: unknown op `{other}`")),
    }
}

/// Storage root: same convention as `RealMongo` — `file:///abs/path`
/// or a relative dir. The args object must carry `root` (the value
/// of `[modules.mongodb].root`); for the POC we hard-code
/// `./.aeris/mongo-store/` so the user doesn't have to plumb it
/// through the manifest.
fn store_root() -> PathBuf {
    PathBuf::from(".aeris/mongo-store")
}

fn collection_path(coll: &str) -> PathBuf {
    store_root().join(format!("{coll}.jsonl"))
}

fn mongodb_write(args_json: &str) -> String {
    let (coll, doc_json) = match split_write_args(args_json) {
        Some(t) => t,
        None => return err_reply("mongodb.write: malformed args (need {collection, doc})"),
    };
    let path = collection_path(&coll);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            return err_reply(&format!("mongodb.write: mkdir: {e}"));
        }
    }
    use std::io::Write;
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(f) => f,
        Err(e) => return err_reply(&format!("mongodb.write: open: {e}")),
    };
    let line = format!("{doc_json}\n");
    if let Err(e) = file.write_all(line.as_bytes()) {
        return err_reply(&format!("mongodb.write: write: {e}"));
    }
    "{\"ok\":null}".to_string()
}

fn mongodb_read(args_json: &str) -> String {
    let coll = match find_string_field(args_json, "collection") {
        Some(c) => c,
        None => return err_reply("mongodb.read: missing `collection`"),
    };
    let path = collection_path(&coll);
    if !path.exists() {
        return "{\"ok\":[]}".to_string();
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) => return err_reply(&format!("mongodb.read: read: {e}")),
    };
    let mut out = String::from("{\"ok\":[");
    let mut first = true;
    for line in text.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str(t);
    }
    out.push_str("]}");
    out
}

// ---- minimal JSON helpers (no serde dep) -------------------------

/// Parses `{"collection":"...","doc": <object>}` and returns the
/// collection name + the raw JSON of `doc`. Built by string scan to
/// keep the POC dependency-free.
fn split_write_args(args: &str) -> Option<(String, String)> {
    let coll = find_string_field(args, "collection")?;
    let doc = find_value_field(args, "doc")?;
    Some((coll, doc))
}

fn find_string_field(args: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let i = args.find(&needle)?;
    let rest = &args[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let bytes = after.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    // walk until the closing quote, honouring \" escapes
    let mut out = String::new();
    let mut i = 1;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return Some(out),
            b'\\' if i + 1 < bytes.len() => {
                out.push(bytes[i + 1] as char);
                i += 2;
            }
            c => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    None
}

/// Find a field whose value can be an object, array, or primitive;
/// return the raw JSON slice as a `String`. Brace/bracket balanced.
fn find_value_field(args: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\"");
    let i = args.find(&needle)?;
    let rest = &args[i + needle.len()..];
    let colon = rest.find(':')?;
    let after = rest[colon + 1..].trim_start();
    let bytes = after.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let (open, close) = match bytes[0] {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        b'"' => {
            // simple string
            return find_string_field(args, key);
        }
        _ => {
            // primitive: read until comma or closing }
            let end = bytes
                .iter()
                .position(|b| *b == b',' || *b == b'}' || *b == b']')
                .unwrap_or(bytes.len());
            return Some(after[..end].trim().to_string());
        }
    };
    let mut depth = 0i32;
    let mut end = 0;
    let mut in_str = false;
    let mut esc = false;
    for (i, b) in bytes.iter().enumerate() {
        if in_str {
            if esc {
                esc = false;
            } else if *b == b'\\' {
                esc = true;
            } else if *b == b'"' {
                in_str = false;
            }
            continue;
        }
        if *b == b'"' {
            in_str = true;
        } else if *b == open {
            depth += 1;
        } else if *b == close {
            depth -= 1;
            if depth == 0 {
                end = i + 1;
                break;
            }
        }
    }
    if end == 0 {
        None
    } else {
        Some(after[..end].to_string())
    }
}

fn err_reply(msg: &str) -> String {
    // JSON-escape the message
    let mut esc = String::with_capacity(msg.len());
    for c in msg.chars() {
        match c {
            '"' => esc.push_str("\\\""),
            '\\' => esc.push_str("\\\\"),
            '\n' => esc.push_str("\\n"),
            _ => esc.push(c),
        }
    }
    format!("{{\"err\":\"{esc}\"}}")
}
