//! Filesystem-backed `MinioBackend` implementation (M22.T4).
//!
//! Semantica: bucket = sub-directory of `endpoint`, key = file
//! path inside the bucket directory. Every op is a real
//! filesystem call (`std::fs::*`) — the trace event continues to
//! record the intent, and the cap allow-list keeps gating which
//! buckets the program may touch.
//!
//! Wiring: this is the impl selected when `[l2.minio] backend =
//! "real"` and `endpoint` points to a local path (either bare
//! `./storage` or `file:///abs/path`). When the endpoint is an
//! HTTP URL the backend currently raises
//! `err.config: minio endpoint = "<url>" — HTTP backend not
//! wired yet (use file://… for local storage; rust-s3 SDK lands
//! in M22.T4-bis)`. The trait shape is identical so a future
//! S3-based variant slots in here unchanged.
//!
//! Idempotency: `fs::write` overwriting with identical bytes is
//! semantically a no-op; the M6 saga key (read from
//! `env.idempotency_key()`) is appended to the trace event so a
//! re-run is visible in the JSONL but produces the same
//! observable state.

use std::path::PathBuf;
use std::rc::Rc;

use super::MinioBackend;
use crate::runtime::eval::{record_event, Env, EvalError, EvalErrorKind};
use crate::runtime::l2_runtime::{sdk_error_to_raised, L2Runtime};
use crate::runtime::value::Value;
use crate::syntax::token::Span;

pub struct RealMinio {
    endpoint: Option<String>,
    /// Held for symmetry with future async SDK-backed variants
    /// (T4-bis: `rust-s3`). FS ops are sync today; keeping the
    /// runtime reference here makes the constructor signature
    /// stable across the upgrade.
    #[allow(dead_code)]
    runtime: Rc<L2Runtime>,
}

impl RealMinio {
    pub fn new(endpoint: Option<String>, runtime: Rc<L2Runtime>) -> Self {
        Self { endpoint, runtime }
    }

    fn root(&self, span: Span) -> Result<PathBuf, EvalError> {
        let endpoint = self.endpoint.as_deref().ok_or_else(|| {
            EvalError::new(
                EvalErrorKind::Raised(Value::Str(
                    "err.config: [l2.minio].endpoint required for backend = \"real\"".into(),
                )),
                span,
            )
        })?;
        if let Some(rest) = endpoint.strip_prefix("file://") {
            return Ok(PathBuf::from(rest));
        }
        if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
            return Err(EvalError::new(
                EvalErrorKind::Raised(Value::Str(format!(
                    "err.config: minio endpoint = \"{endpoint}\" — HTTP backend not wired yet \
                     (use file://… for local storage; rust-s3 SDK lands in M22.T4-bis)"
                ))),
                span,
            ));
        }
        Ok(PathBuf::from(endpoint))
    }

    fn bucket_dir(&self, bucket: &str, span: Span) -> Result<PathBuf, EvalError> {
        Ok(self.root(span)?.join(bucket))
    }

    fn object_path(&self, bucket: &str, key: &str, span: Span) -> Result<PathBuf, EvalError> {
        Ok(self.bucket_dir(bucket, span)?.join(key))
    }
}

impl MinioBackend for RealMinio {
    fn get(&self, env: &Env, bucket: &str, key: &str, span: Span) -> Result<Value, EvalError> {
        let path = self.object_path(bucket, key, span)?;
        let bytes = std::fs::read(&path)
            .map_err(|e| sdk_error_to_raised("minio", "get", &e.to_string(), span))?;
        record_event(
            env,
            "minio_get",
            vec![
                ("bucket".into(), format!("\"{bucket}\"")),
                ("key".into(), format!("\"{key}\"")),
                ("backend".into(), "\"real-fs\"".into()),
                ("size".into(), bytes.len().to_string()),
            ],
        );
        Ok(Value::ok(Value::Bytes(bytes)))
    }

    fn put(
        &self,
        env: &Env,
        bucket: &str,
        key: &str,
        content: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let bytes: Vec<u8> = match content {
            Value::Str(s) => s.as_bytes().to_vec(),
            Value::Bytes(b) => b.clone(),
            other => {
                return Err(EvalError::new(
                    EvalErrorKind::Type(format!(
                        "minio.put: content must be string or bytes, got {}",
                        crate::runtime::eval::value_kind(other)
                    )),
                    span,
                ));
            }
        };
        let path = self.object_path(bucket, key, span)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| sdk_error_to_raised("minio", "put", &e.to_string(), span))?;
        }
        std::fs::write(&path, &bytes)
            .map_err(|e| sdk_error_to_raised("minio", "put", &e.to_string(), span))?;
        let mut fields = vec![
            ("bucket".into(), format!("\"{bucket}\"")),
            ("key".into(), format!("\"{key}\"")),
            ("backend".into(), "\"real-fs\"".into()),
            ("size".into(), bytes.len().to_string()),
        ];
        if let Some(k) = env.idempotency_key() {
            fields.push(("idem".into(), format!("\"{k}\"")));
        }
        record_event(env, "minio_put", fields);
        Ok(Value::ok(Value::Unit))
    }

    fn mb(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        let dir = self.bucket_dir(bucket, span)?;
        std::fs::create_dir_all(&dir)
            .map_err(|e| sdk_error_to_raised("minio", "mb", &e.to_string(), span))?;
        record_event(
            env,
            "minio_mb",
            vec![
                ("bucket".into(), format!("\"{bucket}\"")),
                ("backend".into(), "\"real-fs\"".into()),
            ],
        );
        Ok(Value::ok(Value::Unit))
    }

    fn bucket_exists(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        let dir = self.bucket_dir(bucket, span)?;
        let exists = dir.is_dir();
        record_event(
            env,
            "minio_bucket_exists",
            vec![
                ("bucket".into(), format!("\"{bucket}\"")),
                ("backend".into(), "\"real-fs\"".into()),
                ("exists".into(), exists.to_string()),
            ],
        );
        Ok(Value::Bool(exists))
    }

    fn list(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        let dir = self.bucket_dir(bucket, span)?;
        let mut keys: Vec<Value> = Vec::new();
        if dir.is_dir() {
            let entries = std::fs::read_dir(&dir)
                .map_err(|e| sdk_error_to_raised("minio", "list", &e.to_string(), span))?;
            for entry in entries.flatten() {
                if let Some(name) = entry.file_name().to_str() {
                    keys.push(Value::Str(name.to_string()));
                }
            }
            keys.sort_by(|a, b| match (a, b) {
                (Value::Str(x), Value::Str(y)) => x.cmp(y),
                _ => std::cmp::Ordering::Equal,
            });
        }
        record_event(
            env,
            "minio_list",
            vec![
                ("bucket".into(), format!("\"{bucket}\"")),
                ("backend".into(), "\"real-fs\"".into()),
                ("count".into(), keys.len().to_string()),
            ],
        );
        Ok(Value::List(keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::l2_runtime;
    use std::path::PathBuf;

    fn tmp_root(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("aeris-real-minio-{label}-{pid}-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir tmp root");
        dir
    }

    fn backend(root: &PathBuf) -> RealMinio {
        RealMinio::new(Some(root.to_string_lossy().into_owned()), l2_runtime::shared())
    }

    #[test]
    fn put_writes_file_to_bucket_dir() {
        let root = tmp_root("put");
        let b = backend(&root);
        let env = Env::new();
        b.put(&env, "kb-assets", "a.txt", &Value::Str("hello".into()), Span::ZERO)
            .unwrap();
        let path = root.join("kb-assets").join("a.txt");
        assert!(path.exists(), "file not written: {}", path.display());
        let got = std::fs::read_to_string(&path).unwrap();
        assert_eq!(got, "hello");
    }

    #[test]
    fn get_round_trips_bytes_after_put() {
        let root = tmp_root("rt");
        let b = backend(&root);
        let env = Env::new();
        b.put(
            &env,
            "kb-html",
            "x.html",
            &Value::Bytes(vec![1, 2, 3, 4, 5]),
            Span::ZERO,
        )
        .unwrap();
        let v = b.get(&env, "kb-html", "x.html", Span::ZERO).unwrap();
        match v {
            Value::Result(Ok(inner)) => match *inner {
                Value::Bytes(bs) => assert_eq!(bs, vec![1, 2, 3, 4, 5]),
                other => panic!("expected Bytes, got {other:?}"),
            },
            other => panic!("expected Ok(Bytes), got {other:?}"),
        }
    }

    #[test]
    fn mb_creates_bucket_directory() {
        let root = tmp_root("mb");
        let b = backend(&root);
        let env = Env::new();
        b.mb(&env, "kb-index", Span::ZERO).unwrap();
        assert!(root.join("kb-index").is_dir());
    }

    #[test]
    fn bucket_exists_returns_true_after_mb() {
        let root = tmp_root("exists");
        let b = backend(&root);
        let env = Env::new();
        b.mb(&env, "new", Span::ZERO).unwrap();
        let v = b.bucket_exists(&env, "new", Span::ZERO).unwrap();
        assert_eq!(v, Value::Bool(true));
        let v = b.bucket_exists(&env, "missing", Span::ZERO).unwrap();
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn list_returns_keys_sorted() {
        let root = tmp_root("list");
        let b = backend(&root);
        let env = Env::new();
        b.put(&env, "k", "b.md", &Value::Str("".into()), Span::ZERO).unwrap();
        b.put(&env, "k", "a.md", &Value::Str("".into()), Span::ZERO).unwrap();
        b.put(&env, "k", "c.md", &Value::Str("".into()), Span::ZERO).unwrap();
        let v = b.list(&env, "k", Span::ZERO).unwrap();
        match v {
            Value::List(items) => {
                let names: Vec<&str> = items
                    .iter()
                    .filter_map(|x| match x {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(names, vec!["a.md", "b.md", "c.md"]);
            }
            other => panic!("expected List, got {other:?}"),
        }
    }

    #[test]
    fn missing_endpoint_raises_err_config() {
        let b = RealMinio::new(None, l2_runtime::shared());
        let env = Env::new();
        let err = b.mb(&env, "x", Span::ZERO).unwrap_err();
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.contains("err.config"), "{s}");
                assert!(s.contains("endpoint"), "{s}");
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn http_endpoint_raises_not_yet_wired() {
        let b = RealMinio::new(
            Some("http://localhost:9000".into()),
            l2_runtime::shared(),
        );
        let env = Env::new();
        let err = b.mb(&env, "x", Span::ZERO).unwrap_err();
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.contains("not wired yet"), "{s}");
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }
}
