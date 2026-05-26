//! M45 — bridges from L2 backend traits to a dynamically loaded
//! module. One thin wrapper per family: serialise the call args
//! into JSON, hand them to `LoadedModule::call`, decode the JSON
//! reply back into the Aeris `Value` shape.
//!
//! Every wrapper trusts the module to be well-behaved on its
//! declared cap surface. The runtime enforces caps and `intent`
//! upstream, before the dispatch site reaches us.

use std::rc::Rc;

use super::{
    AuditBackend, DockerBackend, KubeBackend, MinioBackend, MongoBackend, RabbitBackend,
};
use crate::runtime::eval::{record_event, Env, EvalError, EvalErrorKind};
use crate::runtime::json::decode_natural_object;
use crate::runtime::l2_module::LoadedModule;
use crate::runtime::value::{RecordValue, Value};
use crate::syntax::token::Span;

// ---- shared helpers ----------------------------------------------

fn env_json(env: &Env) -> String {
    let mut out = String::from("{");
    let trace_id = env
        .tracer()
        .map(|t| t.trace_id())
        .unwrap_or_else(|| "00000000000000000000000000".into());
    out.push_str(&format!("\"trace_id\":\"{trace_id}\""));
    if let Some(k) = env.idempotency_key() {
        out.push_str(&format!(",\"idempotency_key\":\"{k}\""));
    }
    out.push('}');
    out
}

fn decode_reply(reply: &str, family_op: &str, span: Span) -> Result<Value, EvalError> {
    let fields = decode_natural_object(reply).map_err(|e| {
        EvalError::new(
            EvalErrorKind::Type(format!(
                "module reply for `{family_op}` is not a JSON object: {}",
                e.message
            )),
            span,
        )
    })?;
    for (k, v) in fields {
        if k == "err" {
            let msg = match v {
                Value::Str(s) => s,
                other => format!("{other:?}"),
            };
            return Err(EvalError::new(
                EvalErrorKind::Raised(Value::Str(format!("err.module.{family_op}: {msg}"))),
                span,
            ));
        }
        if k == "ok" {
            return Ok(Value::ok(v));
        }
    }
    Err(EvalError::new(
        EvalErrorKind::Type(format!(
            "module reply for `{family_op}` has neither `ok` nor `err` field"
        )),
        span,
    ))
}

fn value_to_json_arg(v: &Value) -> String {
    crate::runtime::eval::value_to_natural_json_public(v)
}

fn record_module_event(env: &Env, family_op: &str, module_name: &str) {
    record_event(
        env,
        &family_op.replace('.', "_"),
        vec![
            ("module".into(), format!("\"{module_name}\"")),
            ("backend".into(), "\"loaded-module\"".into()),
        ],
    );
}

// ---- audit -------------------------------------------------------

pub struct LoadedAudit {
    pub module: Rc<LoadedModule>,
}

impl AuditBackend for LoadedAudit {
    fn event(
        &self,
        env: &Env,
        name: &str,
        payload: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = format!(
            "{{\"name\":\"{name}\",\"payload\":{}}}",
            value_to_json_arg(payload)
        );
        let reply = self
            .module
            .call("audit.event", &args, &env_json(env))
            .map_err(|e| {
                EvalError::new(
                    EvalErrorKind::Io {
                        op: "audit.event".into(),
                        message: e,
                    },
                    span,
                )
            })?;
        record_module_event(env, "audit.event", &self.module.metadata.name);
        let _ = decode_reply(&reply, "audit.event", span)?;
        Ok(Value::Unit)
    }
}

// ---- kube --------------------------------------------------------

pub struct LoadedKube {
    pub module: Rc<LoadedModule>,
}

impl KubeBackend for LoadedKube {
    fn apply(&self, env: &Env, manifest: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "kube.apply", "manifest", manifest, span)
    }
    fn delete(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "kube.delete", "target", target, span)
    }
    fn get(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "kube.get", "target", target, span)
    }
    fn watch(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "kube.watch", "target", target, span)
    }
}

// ---- docker ------------------------------------------------------

pub struct LoadedDocker {
    pub module: Rc<LoadedModule>,
}

impl DockerBackend for LoadedDocker {
    fn run(
        &self,
        env: &Env,
        image: &str,
        name: Option<&str>,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = match name {
            Some(n) => format!(
                "{{\"image\":\"{}\",\"name\":\"{}\"}}",
                escape_json(image),
                escape_json(n)
            ),
            None => format!("{{\"image\":\"{}\"}}", escape_json(image)),
        };
        proxy_raw(&self.module, env, "docker.run", &args, span)
    }
    fn build(
        &self,
        env: &Env,
        ctx: &str,
        tag: Option<&str>,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = match tag {
            Some(t) => format!(
                "{{\"context\":\"{}\",\"tag\":\"{}\"}}",
                escape_json(ctx),
                escape_json(t)
            ),
            None => format!("{{\"context\":\"{}\"}}", escape_json(ctx)),
        };
        proxy_raw(&self.module, env, "docker.build", &args, span)
    }
    fn push(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.push", "image", image, span)
    }
    fn pull(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.pull", "image", image, span)
    }
    fn inspect(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.inspect", "target", target, span)
    }
    fn logs(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.logs", "name", name, span)
    }
    fn stop(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.stop", "name", name, span)
    }
    fn rm(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.rm", "target", target, span)
    }
    fn rmi(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "docker.rmi", "image", image, span)
    }
}

// ---- mongodb -----------------------------------------------------

pub struct LoadedMongo {
    pub module: Rc<LoadedModule>,
}

impl MongoBackend for LoadedMongo {
    fn read(
        &self,
        env: &Env,
        coll: &str,
        query: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = format!(
            "{{\"collection\":\"{coll}\",\"query\":{}}}",
            value_to_json_arg(query)
        );
        proxy_raw(&self.module, env, "mongodb.read", &args, span)
    }
    fn write(
        &self,
        env: &Env,
        coll: &str,
        doc: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = format!(
            "{{\"collection\":\"{coll}\",\"doc\":{}}}",
            value_to_json_arg(doc)
        );
        proxy_raw(&self.module, env, "mongodb.write", &args, span)
    }
}

// ---- minio -------------------------------------------------------

pub struct LoadedMinio {
    pub module: Rc<LoadedModule>,
}

impl MinioBackend for LoadedMinio {
    fn get(&self, env: &Env, bucket: &str, key: &str, span: Span) -> Result<Value, EvalError> {
        let args = format!("{{\"bucket\":\"{bucket}\",\"key\":\"{key}\"}}");
        proxy_raw(&self.module, env, "minio.get", &args, span)
    }
    fn put(
        &self,
        env: &Env,
        bucket: &str,
        key: &str,
        content: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = format!(
            "{{\"bucket\":\"{bucket}\",\"key\":\"{key}\",\"content\":{}}}",
            value_to_json_arg(content)
        );
        proxy_raw(&self.module, env, "minio.put", &args, span)
    }
    fn mb(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "minio.mb", "bucket", bucket, span)
    }
    fn bucket_exists(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(
            &self.module,
            env,
            "minio.bucket_exists",
            "bucket",
            bucket,
            span,
        )
    }
    fn list(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "minio.list", "bucket", bucket, span)
    }
}

// ---- rabbitmq ----------------------------------------------------

pub struct LoadedRabbit {
    pub module: Rc<LoadedModule>,
}

impl RabbitBackend for LoadedRabbit {
    fn publish(
        &self,
        env: &Env,
        queue: &str,
        msg: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        let args = format!(
            "{{\"queue\":\"{queue}\",\"msg\":{}}}",
            value_to_json_arg(msg)
        );
        proxy_raw(&self.module, env, "rabbitmq.publish", &args, span)
    }
    fn subscribe(&self, env: &Env, queue: &str, span: Span) -> Result<Value, EvalError> {
        proxy_str(&self.module, env, "rabbitmq.subscribe", "queue", queue, span)
    }
}

// ---- internal helpers --------------------------------------------

fn proxy_str(
    module: &Rc<LoadedModule>,
    env: &Env,
    family_op: &str,
    arg_name: &str,
    arg_value: &str,
    span: Span,
) -> Result<Value, EvalError> {
    let args = format!("{{\"{arg_name}\":\"{}\"}}", escape_json(arg_value));
    proxy_raw(module, env, family_op, &args, span)
}

fn proxy_raw(
    module: &Rc<LoadedModule>,
    env: &Env,
    family_op: &str,
    args: &str,
    span: Span,
) -> Result<Value, EvalError> {
    let reply = module.call(family_op, args, &env_json(env)).map_err(|e| {
        EvalError::new(
            EvalErrorKind::Io {
                op: family_op.into(),
                message: e,
            },
            span,
        )
    })?;
    record_module_event(env, family_op, &module.metadata.name);
    decode_reply(&reply, family_op, span)
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Helper for [`super::L2Backends::from_manifest`] — given a loaded
/// module and the family it implements, return the trait object
/// ready to be stored in the dispatch table. Returns `Err` when the
/// family is unknown (should be unreachable thanks to the parser
/// gate).
#[allow(unused_imports)]
pub(crate) fn install_module(
    family: &str,
    module: Rc<LoadedModule>,
) -> Result<InstalledBackend, String> {
    match family {
        "audit" => Ok(InstalledBackend::Audit(Rc::new(LoadedAudit { module }))),
        "kube" => Ok(InstalledBackend::Kube(Rc::new(LoadedKube { module }))),
        "docker" => Ok(InstalledBackend::Docker(Rc::new(LoadedDocker { module }))),
        "mongodb" => Ok(InstalledBackend::Mongo(Rc::new(LoadedMongo { module }))),
        "minio" => Ok(InstalledBackend::Minio(Rc::new(LoadedMinio { module }))),
        "rabbitmq" => Ok(InstalledBackend::Rabbit(Rc::new(LoadedRabbit { module }))),
        other => Err(format!("unknown L2 family `{other}`")),
    }
}

pub(crate) enum InstalledBackend {
    Audit(Rc<dyn AuditBackend>),
    Kube(Rc<dyn KubeBackend>),
    Docker(Rc<dyn DockerBackend>),
    Mongo(Rc<dyn MongoBackend>),
    Minio(Rc<dyn MinioBackend>),
    Rabbit(Rc<dyn RabbitBackend>),
}
