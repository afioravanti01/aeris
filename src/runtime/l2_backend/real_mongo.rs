//! Filesystem-backed `MongoBackend` (M22.T5).
//!
//! Each collection is a JSONL file under `[l2.mongodb].uri`
//! (which must use the `file://path` scheme). `write` appends a
//! natural-JSON line per doc, optionally deduped by the saga
//! `__aeris_idem` sentinel. `read` scans the file and returns the
//! list of decoded docs as `Value::Record`s — the M22.T5 query
//! arg is forwarded verbatim into the trace event but not applied
//! (a `$eq`-style matcher follows in the live-driver variant).
//!
//! A `mongodb://…` URI is reserved for the future `mongodb` crate
//! variant and surfaces `err.config: … — TCP backend not wired
//! yet`.

use std::path::PathBuf;
use std::rc::Rc;

use super::MongoBackend;
use crate::runtime::eval::{record_event, Env, EvalError, EvalErrorKind};
use crate::runtime::json::decode_natural_object;
use crate::runtime::l2_runtime::{sdk_error_to_raised, L2Runtime};
use crate::runtime::value::{RecordValue, Value};
use crate::syntax::token::Span;

pub struct RealMongo {
    uri: Option<String>,
    #[allow(dead_code)]
    runtime: Rc<L2Runtime>,
}

impl RealMongo {
    pub fn new(uri: Option<String>, runtime: Rc<L2Runtime>) -> Self {
        Self { uri, runtime }
    }

    fn root(&self, span: Span) -> Result<PathBuf, EvalError> {
        let uri = self.uri.as_deref().ok_or_else(|| {
            EvalError::new(
                EvalErrorKind::Raised(Value::Str(
                    "err.config: [l2.mongodb].uri required for backend = \"real\"".into(),
                )),
                span,
            )
        })?;
        if let Some(rest) = uri.strip_prefix("file://") {
            return Ok(PathBuf::from(rest));
        }
        if uri.starts_with("mongodb://") || uri.starts_with("mongodb+srv://") {
            return Err(EvalError::new(
                EvalErrorKind::Raised(Value::Str(format!(
                    "err.config: mongodb uri = \"{uri}\" — TCP backend not wired yet \
                     (use file://… for local storage; mongodb crate lands in M22.T5-bis)"
                ))),
                span,
            ));
        }
        Ok(PathBuf::from(uri))
    }

    fn collection_path(&self, coll: &str, span: Span) -> Result<PathBuf, EvalError> {
        Ok(self.root(span)?.join(format!("{coll}.jsonl")))
    }

    fn idem_of(doc: &Value) -> Option<String> {
        if let Value::Record(r) = doc {
            for (k, v) in &r.fields {
                if k == "__aeris_idem" {
                    if let Value::Str(s) = v {
                        return Some(s.clone());
                    }
                }
            }
        }
        None
    }
}

fn value_to_natural_json_line(v: &Value) -> String {
    // Reuse the runtime's natural-JSON encoder via a public helper.
    // Defined locally here to avoid leaking that helper into the
    // crate API — `crate::runtime::eval::value_to_natural_json` is
    // intentionally private; we walk the small subset we need.
    fn enc(v: &Value, out: &mut String) {
        match v {
            Value::Unit => out.push_str("null"),
            Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Value::Int(n) => out.push_str(&n.to_string()),
            Value::Float(f) => out.push_str(&f.to_string()),
            Value::Str(s)
            | Value::Decimal(s)
            | Value::Uuid(s)
            | Value::Date(s)
            | Value::Timestamp(s)
            | Value::Duration(s) => {
                out.push('"');
                for c in s.chars() {
                    match c {
                        '"' => out.push_str("\\\""),
                        '\\' => out.push_str("\\\\"),
                        '\n' => out.push_str("\\n"),
                        '\r' => out.push_str("\\r"),
                        '\t' => out.push_str("\\t"),
                        c => out.push(c),
                    }
                }
                out.push('"');
            }
            Value::Record(r) => {
                out.push('{');
                for (i, (k, v)) in r.fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push('"');
                    out.push_str(k);
                    out.push_str("\":");
                    enc(v, out);
                }
                out.push('}');
            }
            Value::List(xs) | Value::Set(xs) | Value::Tuple(xs) => {
                out.push('[');
                for (i, x) in xs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    enc(x, out);
                }
                out.push(']');
            }
            _ => out.push_str("null"),
        }
    }
    let mut s = String::new();
    enc(v, &mut s);
    s
}

impl MongoBackend for RealMongo {
    fn read(
        &self,
        env: &Env,
        coll: &str,
        _query: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let path = self.collection_path(coll, span)?;
        let mut docs: Vec<Value> = Vec::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| sdk_error_to_raised("mongodb", "read", &e.to_string(), span))?;
            for line in text.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(fields) = decode_natural_object(line) {
                    docs.push(Value::Record(RecordValue {
                        name: None,
                        fields,
                    }));
                }
            }
        }
        record_event(
            env,
            "mongodb_read",
            vec![
                ("collection".into(), format!("\"{coll}\"")),
                ("backend".into(), "\"real-fs\"".into()),
                ("count".into(), docs.len().to_string()),
            ],
        );
        Ok(Value::ok(Value::List(docs)))
    }

    fn write(
        &self,
        env: &Env,
        coll: &str,
        doc: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let path = self.collection_path(coll, span)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| sdk_error_to_raised("mongodb", "write", &e.to_string(), span))?;
        }
        // Saga idempotency: inject the env's key into the doc when
        // missing, and skip the append if an identical idem is
        // already present in the file (M6 / § 12.3).
        let mut doc_owned = doc.clone();
        if let Some(key) = env.idempotency_key() {
            if Self::idem_of(&doc_owned).is_none() {
                if let Value::Record(r) = &mut doc_owned {
                    r.fields
                        .push(("__aeris_idem".into(), Value::Str(key.to_string())));
                }
            }
            // Scan existing file for the same idem.
            if path.exists() {
                let text = std::fs::read_to_string(&path).map_err(|e| {
                    sdk_error_to_raised("mongodb", "write", &e.to_string(), span)
                })?;
                for line in text.lines() {
                    if line.contains(&format!("\"__aeris_idem\":\"{key}\"")) {
                        record_event(
                            env,
                            "mongodb_write",
                            vec![
                                ("collection".into(), format!("\"{coll}\"")),
                                ("backend".into(), "\"real-fs\"".into()),
                                ("idem".into(), format!("\"{key}\"")),
                                ("duplicate".into(), "true".into()),
                            ],
                        );
                        return Ok(Value::ok(Value::Unit));
                    }
                }
            }
        }
        let line = value_to_natural_json_line(&doc_owned);
        let line_with_nl = format!("{line}\n");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| sdk_error_to_raised("mongodb", "write", &e.to_string(), span))?;
        use std::io::Write;
        file.write_all(line_with_nl.as_bytes())
            .map_err(|e| sdk_error_to_raised("mongodb", "write", &e.to_string(), span))?;
        let mut fields = vec![
            ("collection".into(), format!("\"{coll}\"")),
            ("backend".into(), "\"real-fs\"".into()),
        ];
        if let Some(key) = env.idempotency_key() {
            fields.push(("idem".into(), format!("\"{key}\"")));
        }
        record_event(env, "mongodb_write", fields);
        Ok(Value::ok(Value::Unit))
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
        let dir = std::env::temp_dir().join(format!("aeris-real-mongo-{label}-{pid}-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn backend(root: &PathBuf) -> RealMongo {
        RealMongo::new(Some(format!("file://{}", root.display())), l2_runtime::shared())
    }

    fn rec(fields: &[(&str, Value)]) -> Value {
        Value::Record(RecordValue {
            name: None,
            fields: fields.iter().map(|(k, v)| ((*k).into(), v.clone())).collect(),
        })
    }

    #[test]
    fn write_then_read_round_trips_a_doc() {
        let root = tmp_root("rt");
        let b = backend(&root);
        let env = Env::new();
        let doc = rec(&[("id", Value::Int(1)), ("name", Value::Str("a".into()))]);
        b.write(&env, "users", &doc, Span::ZERO).unwrap();
        let v = b.read(&env, "users", &Value::Unit, Span::ZERO).unwrap();
        match v {
            Value::Result(Ok(inner)) => match *inner {
                Value::List(xs) => {
                    assert_eq!(xs.len(), 1);
                    match &xs[0] {
                        Value::Record(r) => {
                            assert!(r.fields.iter().any(|(k, v)| k == "name" && matches!(v, Value::Str(s) if s == "a")));
                        }
                        other => panic!("expected Record, got {other:?}"),
                    }
                }
                other => panic!("expected List, got {other:?}"),
            },
            other => panic!("expected Ok(List), got {other:?}"),
        }
    }

    #[test]
    fn read_empty_collection_returns_empty_list() {
        let root = tmp_root("empty");
        let b = backend(&root);
        let env = Env::new();
        let v = b.read(&env, "missing", &Value::Unit, Span::ZERO).unwrap();
        match v {
            Value::Result(Ok(inner)) => match *inner {
                Value::List(xs) => assert_eq!(xs.len(), 0),
                other => panic!("expected List, got {other:?}"),
            },
            other => panic!("expected Ok(List), got {other:?}"),
        }
    }

    #[test]
    fn missing_uri_raises_err_config() {
        let b = RealMongo::new(None, l2_runtime::shared());
        let env = Env::new();
        let err = b
            .read(&env, "x", &Value::Unit, Span::ZERO)
            .unwrap_err();
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => {
                assert!(s.contains("err.config"), "{s}");
                assert!(s.contains("uri"), "{s}");
            }
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn mongodb_uri_raises_not_wired() {
        let b = RealMongo::new(Some("mongodb://localhost:27017".into()), l2_runtime::shared());
        let env = Env::new();
        let err = b
            .write(&env, "x", &Value::Unit, Span::ZERO)
            .unwrap_err();
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert!(s.contains("not wired yet"), "{s}"),
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }
}
