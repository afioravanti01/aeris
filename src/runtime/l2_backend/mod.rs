//! L2 native handler backends.
//!
//! Each L2 family (`audit`, `kube`, `docker`, `mongodb`, `minio`,
//! `rabbitmq`) exposes a trait whose methods correspond 1:1 to the
//! user-facing ops. The interpreter's `builtin_*` functions in
//! `super::eval` handle arity checks, argument extraction, and cap
//! enforcement; once those pass they dispatch to the trait method
//! held in `Env::l2_backends`.
//!
//! Today every family ships a single `Mock*` implementation whose
//! body is the verbatim move of the corresponding `builtin_*`
//! function (see `super::eval::mock_*`). M22.T4–T8 add real
//! backends (`RealMinio`, `RealMongo`, …) without touching the
//! dispatch site — only the table entry changes.

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;

use super::eval::{self, EvalError, Env};
use super::l2_module::LoadedModule;
use super::l2_runtime::L2Runtime;
use super::value::Value;
use crate::manifest::{BackendKind, L2BackendsConfig, ModuleEntry};
use crate::syntax::token::Span;

mod loaded;
mod real_minio;
mod real_mongo;
mod real_rabbit;
pub use loaded::{
    LoadedAudit, LoadedDocker, LoadedKube, LoadedMinio, LoadedMongo, LoadedRabbit,
};
pub use real_minio::RealMinio;
pub use real_mongo::RealMongo;
pub use real_rabbit::RealRabbit;

// ---- audit ---------------------------------------------------------

pub trait AuditBackend {
    fn event(
        &self,
        env: &Env,
        name: &str,
        payload: &Value,
        span: Span,
    ) -> Result<Value, EvalError>;
}

pub struct MockAudit;

impl AuditBackend for MockAudit {
    fn event(
        &self,
        env: &Env,
        name: &str,
        payload: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_audit_event(env, name, payload, span)
    }
}

/// `audit.event` is special among L2 families: the historical
/// "mock" impl already writes the JSONL line to `audit.jsonl`
/// (now resolved against the project root by M41). `RealAudit`
/// is therefore semantically identical to `MockAudit` today and
/// exists so the dispatch table is uniform. A future trace-only
/// variant of Mock — promised by the documentation, not yet
/// shipped — will diverge from Real then.
pub struct RealAudit;

impl AuditBackend for RealAudit {
    fn event(
        &self,
        env: &Env,
        name: &str,
        payload: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_audit_event(env, name, payload, span)
    }
}

// ---- kube ----------------------------------------------------------

pub trait KubeBackend {
    fn apply(&self, env: &Env, manifest: &str, span: Span) -> Result<Value, EvalError>;
    fn delete(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError>;
    fn get(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError>;
    fn watch(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError>;
}

pub struct MockKube;

impl KubeBackend for MockKube {
    fn apply(&self, env: &Env, manifest: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_kube_apply(env, manifest, span)
    }
    fn delete(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_kube_delete(env, target, span)
    }
    fn get(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_kube_get(env, target, span)
    }
    fn watch(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_kube_watch(env, target, span)
    }
}

/// M22.T7 — real `kube.*` via the system `kubectl` binary. The
/// typed `kube`/`k8s-openapi` SDK variant is the M22.T7-bis
/// follow-up; today shelling out covers every op the M11 surface
/// exposed.
pub struct RealKube;

impl KubeBackend for RealKube {
    fn apply(&self, env: &Env, manifest: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_kube_apply(env, manifest, span)
    }
    fn delete(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_kube_delete(env, target, span)
    }
    fn get(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_kube_get(env, target, span)
    }
    fn watch(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_kube_watch(env, target, span)
    }
}

// ---- docker --------------------------------------------------------

pub trait DockerBackend {
    fn run(&self, env: &Env, image: &str, name: Option<&str>, span: Span)
        -> Result<Value, EvalError>;
    fn build(&self, env: &Env, ctx: &str, tag: Option<&str>, span: Span)
        -> Result<Value, EvalError>;
    fn push(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError>;
    fn pull(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError>;
    fn inspect(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError>;
    fn logs(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError>;
    fn stop(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError>;
    fn rm(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError>;
    fn rmi(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError>;
}

pub struct MockDocker;

impl DockerBackend for MockDocker {
    fn run(
        &self,
        env: &Env,
        image: &str,
        name: Option<&str>,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_docker_run(env, image, name, span)
    }
    fn build(
        &self,
        env: &Env,
        ctx: &str,
        tag: Option<&str>,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_docker_build(env, ctx, tag, span)
    }
    fn push(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_push(env, image, span)
    }
    fn pull(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_pull(env, image, span)
    }
    fn inspect(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_inspect(env, target, span)
    }
    fn logs(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_logs(env, name, span)
    }
    fn stop(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_stop(env, name, span)
    }
    fn rm(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_rm(env, target, span)
    }
    fn rmi(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_docker_rmi(env, image, span)
    }
}

/// M22.T6 — real `docker.*` via the system `docker` binary. The
/// typed `bollard` SDK variant is the M22.T6-bis follow-up.
pub struct RealDocker;

impl DockerBackend for RealDocker {
    fn run(
        &self,
        env: &Env,
        image: &str,
        name: Option<&str>,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::real_docker_run(env, image, name, span)
    }
    fn build(
        &self,
        env: &Env,
        ctx: &str,
        tag: Option<&str>,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::real_docker_build(env, ctx, tag, span)
    }
    fn push(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_push(env, image, span)
    }
    fn pull(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_pull(env, image, span)
    }
    fn inspect(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_inspect(env, target, span)
    }
    fn logs(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_logs(env, name, span)
    }
    fn stop(&self, env: &Env, name: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_stop(env, name, span)
    }
    fn rm(&self, env: &Env, target: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_rm(env, target, span)
    }
    fn rmi(&self, env: &Env, image: &str, span: Span) -> Result<Value, EvalError> {
        eval::real_docker_rmi(env, image, span)
    }
}

// ---- mongodb -------------------------------------------------------

pub trait MongoBackend {
    fn read(&self, env: &Env, coll: &str, query: &Value, span: Span) -> Result<Value, EvalError>;
    fn write(&self, env: &Env, coll: &str, doc: &Value, span: Span) -> Result<Value, EvalError>;
}

pub struct MockMongo;

impl MongoBackend for MockMongo {
    fn read(
        &self,
        env: &Env,
        coll: &str,
        query: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_mongodb_read(env, coll, query, span)
    }
    fn write(
        &self,
        env: &Env,
        coll: &str,
        doc: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_mongodb_write(env, coll, doc, span)
    }
}

// ---- minio ---------------------------------------------------------

pub trait MinioBackend {
    fn get(&self, env: &Env, bucket: &str, key: &str, span: Span) -> Result<Value, EvalError>;
    fn put(
        &self,
        env: &Env,
        bucket: &str,
        key: &str,
        content: &Value,
        span: Span,
    ) -> Result<Value, EvalError>;
    fn mb(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError>;
    fn bucket_exists(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError>;
    fn list(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError>;
}

pub struct MockMinio;

impl MinioBackend for MockMinio {
    fn get(&self, env: &Env, bucket: &str, key: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_minio_get(env, bucket, key, span)
    }
    fn put(
        &self,
        env: &Env,
        bucket: &str,
        key: &str,
        content: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_minio_put(env, bucket, key, content, span)
    }
    fn mb(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_minio_mb(env, bucket, span)
    }
    fn bucket_exists(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_minio_bucket_exists(env, bucket, span)
    }
    fn list(&self, env: &Env, bucket: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_minio_list(env, bucket, span)
    }
}

// ---- rabbitmq ------------------------------------------------------

pub trait RabbitBackend {
    fn publish(
        &self,
        env: &Env,
        queue: &str,
        msg: &Value,
        span: Span,
    ) -> Result<Value, EvalError>;
    fn subscribe(&self, env: &Env, queue: &str, span: Span) -> Result<Value, EvalError>;
}

pub struct MockRabbit;

impl RabbitBackend for MockRabbit {
    fn publish(
        &self,
        env: &Env,
        queue: &str,
        msg: &Value,
        span: Span,
    ) -> Result<Value, EvalError> {
        eval::mock_rabbitmq_publish(env, queue, msg, span)
    }
    fn subscribe(&self, env: &Env, queue: &str, span: Span) -> Result<Value, EvalError> {
        eval::mock_rabbitmq_subscribe(env, queue, span)
    }
}

// ---- master table --------------------------------------------------

/// One slot per L2 family. `Env` holds an `Rc<L2Backends>` and the
/// dispatch site in `super::eval::builtin_*` calls the relevant
/// trait method. The default is "every family is mock", which
/// matches the historical behaviour of M11 — the test suite stays
/// green without any per-test setup.
#[derive(Clone)]
pub struct L2Backends {
    pub audit: Rc<dyn AuditBackend>,
    pub kube: Rc<dyn KubeBackend>,
    pub docker: Rc<dyn DockerBackend>,
    pub mongodb: Rc<dyn MongoBackend>,
    pub minio: Rc<dyn MinioBackend>,
    pub rabbitmq: Rc<dyn RabbitBackend>,
}

impl Default for L2Backends {
    fn default() -> Self {
        Self {
            audit: Rc::new(MockAudit),
            kube: Rc::new(MockKube),
            docker: Rc::new(MockDocker),
            mongodb: Rc::new(MockMongo),
            minio: Rc::new(MockMinio),
            rabbitmq: Rc::new(MockRabbit),
        }
    }
}

impl L2Backends {
    /// M22.T4: instantiate the per-family backends from the
    /// manifest config. Each family resolves independently:
    /// `Mock` → the historical trace-only stub; `Real` → the
    /// live SDK / FS-backed impl when available, otherwise falls
    /// back to `Mock` (`Real` slots that aren't wired yet are
    /// documented per family in `docs/plan.md § 5.M22`). The
    /// shared `Rc<L2Runtime>` is plumbed into every `Real*` so
    /// async SDKs (T4-bis+) can `block_on` without each backend
    /// owning its own reactor.
    /// M45 — same as [`from_manifest`] but also installs dynamically
    /// loaded modules from `aeris.toml [modules.<family>]`. Loading
    /// fails the whole start-up: a bad signature or a hash mismatch
    /// surfaces as `Err(message)` so the CLI can report it before
    /// any user code runs.
    pub fn from_manifest_with_modules(
        cfg: &L2BackendsConfig,
        runtime: Rc<L2Runtime>,
        modules: &BTreeMap<String, ModuleEntry>,
        project_root: &Path,
    ) -> Result<Self, String> {
        let mut backends = Self::from_manifest(cfg, runtime);
        for (family, entry) in modules {
            let loaded = Rc::new(LoadedModule::load(entry, project_root)?);
            match loaded::install_module(family, loaded)? {
                loaded::InstalledBackend::Audit(b) => backends.audit = b,
                loaded::InstalledBackend::Kube(b) => backends.kube = b,
                loaded::InstalledBackend::Docker(b) => backends.docker = b,
                loaded::InstalledBackend::Mongo(b) => backends.mongodb = b,
                loaded::InstalledBackend::Minio(b) => backends.minio = b,
                loaded::InstalledBackend::Rabbit(b) => backends.rabbitmq = b,
            }
        }
        Ok(backends)
    }

    pub fn from_manifest(cfg: &L2BackendsConfig, runtime: Rc<L2Runtime>) -> Self {
        let minio: Rc<dyn MinioBackend> = match cfg.minio.backend {
            BackendKind::Real => Rc::new(RealMinio::new(
                cfg.minio.endpoint.clone(),
                runtime.clone(),
            )),
            BackendKind::Mock | BackendKind::Replay => Rc::new(MockMinio),
        };
        let mongodb: Rc<dyn MongoBackend> = match cfg.mongodb.backend {
            BackendKind::Real => Rc::new(RealMongo::new(
                cfg.mongodb.uri.clone(),
                runtime.clone(),
            )),
            BackendKind::Mock | BackendKind::Replay => Rc::new(MockMongo),
        };
        let docker: Rc<dyn DockerBackend> = match cfg.docker.backend {
            BackendKind::Real => Rc::new(RealDocker),
            BackendKind::Mock | BackendKind::Replay => Rc::new(MockDocker),
        };
        let kube: Rc<dyn KubeBackend> = match cfg.kube.backend {
            BackendKind::Real => Rc::new(RealKube),
            BackendKind::Mock | BackendKind::Replay => Rc::new(MockKube),
        };
        let rabbitmq: Rc<dyn RabbitBackend> = match cfg.rabbitmq.backend {
            BackendKind::Real => Rc::new(RealRabbit::new(
                cfg.rabbitmq.uri.clone(),
                runtime.clone(),
            )),
            BackendKind::Mock | BackendKind::Replay => Rc::new(MockRabbit),
        };
        let audit: Rc<dyn AuditBackend> = match cfg.audit.backend {
            BackendKind::Real => Rc::new(RealAudit),
            BackendKind::Mock | BackendKind::Replay => Rc::new(MockAudit),
        };
        let _ = &runtime; // reserved for async backends (T4-bis+)
        Self {
            audit,
            kube,
            docker,
            mongodb,
            minio,
            rabbitmq,
        }
    }
}
