//! Filesystem-backed `RabbitBackend` (M22.T8).
//!
//! Queue = file under `[l2.rabbitmq].uri` (`file://path`).
//! `publish` appends one JSON line per message;
//! `subscribe` reads the file and returns the list of decoded
//! messages. AMQP URIs are reserved for the future `lapin` SDK
//! variant.

use std::path::PathBuf;
use std::rc::Rc;

use super::RabbitBackend;
use crate::runtime::eval::{record_event, Env, EvalError, EvalErrorKind};
use crate::runtime::json::decode_natural_object;
use crate::runtime::l2_runtime::{sdk_error_to_raised, L2Runtime};
use crate::runtime::value::{RecordValue, Value};
use crate::syntax::token::Span;

pub struct RealRabbit {
    uri: Option<String>,
    #[allow(dead_code)]
    runtime: Rc<L2Runtime>,
}

impl RealRabbit {
    pub fn new(uri: Option<String>, runtime: Rc<L2Runtime>) -> Self {
        Self { uri, runtime }
    }

    fn root(&self, span: Span) -> Result<PathBuf, EvalError> {
        let uri = self.uri.as_deref().ok_or_else(|| {
            EvalError::new(
                EvalErrorKind::Raised(Value::Str(
                    "err.config: [l2.rabbitmq].uri required for backend = \"real\"".into(),
                )),
                span,
            )
        })?;
        if let Some(rest) = uri.strip_prefix("file://") {
            return Ok(PathBuf::from(rest));
        }
        if uri.starts_with("amqp://") || uri.starts_with("amqps://") {
            return Err(EvalError::new(
                EvalErrorKind::Raised(Value::Str(format!(
                    "err.config: rabbitmq uri = \"{uri}\" — AMQP backend not wired yet \
                     (use file://… for local storage; lapin SDK lands in M22.T8-bis)"
                ))),
                span,
            ));
        }
        Ok(PathBuf::from(uri))
    }

    fn queue_path(&self, queue: &str, span: Span) -> Result<PathBuf, EvalError> {
        Ok(self.root(span)?.join(format!("{queue}.jsonl")))
    }
}

fn encode_message(v: &Value) -> String {
    match v {
        Value::Str(s) => {
            let mut out = String::from("\"");
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        Value::Record(r) => {
            let mut out = String::from("{");
            for (i, (k, v)) in r.fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('"');
                out.push_str(k);
                out.push_str("\":");
                out.push_str(&encode_message(v));
            }
            out.push('}');
            out
        }
        Value::Int(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => "null".into(),
    }
}

impl RabbitBackend for RealRabbit {
    fn publish(
        &self,
        env: &Env,
        queue: &str,
        msg: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let path = self.queue_path(queue, span)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                sdk_error_to_raised("rabbitmq", "publish", &e.to_string(), span)
            })?;
        }
        let line = format!("{}\n", encode_message(msg));
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| sdk_error_to_raised("rabbitmq", "publish", &e.to_string(), span))?;
        use std::io::Write;
        file.write_all(line.as_bytes())
            .map_err(|e| sdk_error_to_raised("rabbitmq", "publish", &e.to_string(), span))?;
        let mut fields = vec![
            ("queue".into(), format!("\"{queue}\"")),
            ("backend".into(), "\"real-fs\"".into()),
        ];
        if let Some(k) = env.idempotency_key() {
            fields.push(("message_id".into(), format!("\"{k}\"")));
        }
        record_event(env, "rabbitmq_publish", fields);
        Ok(Value::ok(Value::Unit))
    }

    fn subscribe(&self, env: &Env, queue: &str, span: Span) -> Result<Value, EvalError> {
        let path = self.queue_path(queue, span)?;
        let mut msgs: Vec<Value> = Vec::new();
        if path.exists() {
            let text = std::fs::read_to_string(&path).map_err(|e| {
                sdk_error_to_raised("rabbitmq", "subscribe", &e.to_string(), span)
            })?;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed.starts_with('{') {
                    if let Ok(fields) = decode_natural_object(trimmed) {
                        msgs.push(Value::Record(RecordValue {
                            name: None,
                            fields,
                        }));
                        continue;
                    }
                }
                msgs.push(Value::Str(trimmed.trim_matches('"').to_string()));
            }
        }
        record_event(
            env,
            "rabbitmq_subscribe",
            vec![
                ("queue".into(), format!("\"{queue}\"")),
                ("backend".into(), "\"real-fs\"".into()),
                ("count".into(), msgs.len().to_string()),
            ],
        );
        Ok(Value::ok(Value::List(msgs)))
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
        let dir = std::env::temp_dir().join(format!("aeris-real-rabbit-{label}-{pid}-{nanos}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn backend(root: &PathBuf) -> RealRabbit {
        RealRabbit::new(
            Some(format!("file://{}", root.display())),
            l2_runtime::shared(),
        )
    }

    #[test]
    fn publish_then_subscribe_round_trips_records() {
        let root = tmp_root("rt");
        let b = backend(&root);
        let env = Env::new();
        let msg = Value::Record(RecordValue {
            name: None,
            fields: vec![("id".into(), Value::Int(1)), ("op".into(), Value::Str("ping".into()))],
        });
        b.publish(&env, "events", &msg, Span::ZERO).unwrap();
        b.publish(&env, "events", &msg, Span::ZERO).unwrap();
        let v = b.subscribe(&env, "events", Span::ZERO).unwrap();
        match v {
            Value::Result(Ok(inner)) => match *inner {
                Value::List(xs) => assert_eq!(xs.len(), 2),
                other => panic!("expected List, got {other:?}"),
            },
            other => panic!("expected Ok(List), got {other:?}"),
        }
    }

    #[test]
    fn subscribe_empty_queue_returns_empty_list() {
        let root = tmp_root("empty");
        let b = backend(&root);
        let env = Env::new();
        let v = b.subscribe(&env, "x", Span::ZERO).unwrap();
        match v {
            Value::Result(Ok(inner)) => match *inner {
                Value::List(xs) => assert_eq!(xs.len(), 0),
                other => panic!("expected List, got {other:?}"),
            },
            other => panic!("expected Ok(List), got {other:?}"),
        }
    }

    #[test]
    fn amqp_uri_raises_not_wired() {
        let b = RealRabbit::new(
            Some("amqp://localhost".into()),
            l2_runtime::shared(),
        );
        let env = Env::new();
        let err = b
            .publish(&env, "x", &Value::Str("y".into()), Span::ZERO)
            .unwrap_err();
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert!(s.contains("not wired yet"), "{s}"),
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }

    #[test]
    fn missing_uri_raises_err_config() {
        let b = RealRabbit::new(None, l2_runtime::shared());
        let env = Env::new();
        let err = b
            .subscribe(&env, "x", Span::ZERO)
            .unwrap_err();
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert!(s.contains("err.config"), "{s}"),
            other => panic!("expected Raised(Str), got {other:?}"),
        }
    }
}
