//! Aeris runtime: tree-walk evaluator, JSONL tracer, replay,
//! L1 stdlib, and L2 native cap handlers.
//!
//! Realises `docs/language.md` §§ 11–14 (sequencing, sagas, agents,
//! agent_net), § 19 (concurrency), § 20 (tracing/replay),
//! §§ 22–23 (L1 / L2 modules).

pub mod eval;
pub mod http;
pub mod json;
pub mod net_server;
pub mod replay;
pub mod trace;
pub mod trace_diff;
pub mod value;

pub use eval::{eval_expression, eval_module_env, run_main, Env, EvalError, EvalErrorKind};
pub use trace::{TraceEvent, Tracer};
pub use value::{
    AgentInstance, AgentNetInstance, Closure, EnumValue, RecordValue, Value, VariantValue,
};
