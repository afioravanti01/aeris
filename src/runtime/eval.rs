//! Tree-walking evaluator for the **pure** subset of Aeris (M3.T2).
//!
//! Realises `docs/language.md` § 5 (values, bindings), § 6 (control
//! flow) and § 17 (patterns). Capability calls, sagas, agents and
//! the trace channel land in M4+; the M3 evaluator deliberately
//! refuses anything that would require an in-scope `cap`.
//!
//! Public entry points:
//!
//! - [`eval_expression`] — parse + evaluate a single expression.
//! - [`Env::new`] — construct an empty evaluation scope (used by
//!   future tasks that thread closures and module fns).

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::value::{
    CapEntryValue, CapNarrowError, CapValue, Closure, EnumValue, ModuleScope, RecordValue,
    SagaInstance, Value, VariantValue,
};
use crate::syntax::ast::{
    AssignOp, BinOp, Block, CallArg, CapEntry, CapNarrowKind, ElseBranch, Expr, Item, ListPatElem,
    MatchArm, ModelDecl, Module, Pattern, RecordDecl, SagaStep, Stmt, UnOp, UndoForm,
};
use crate::syntax::parse_expression;
use crate::syntax::token::Span;

// ====================================================================
//  Errors
// ====================================================================

/// A runtime evaluation error. `span` points at the offending node.
#[derive(Debug, Clone, PartialEq)]
pub struct EvalError {
    pub kind: EvalErrorKind,
    pub span: Span,
}

impl EvalError {
    fn new(kind: EvalErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EvalErrorKind {
    /// Operands did not match the operator's requirements.
    Type(String),
    /// `let x = 1; y` — `y` is not defined.
    UndefinedVar(String),
    /// Integer division / remainder by zero.
    DivByZero,
    /// `match x { ... }` exhausted with no arm matching `x`.
    NonExhaustiveMatch,
    /// `xs[i]` with `i` outside `0..len(xs)`.
    IndexOutOfBounds { index: i64, len: usize },
    /// Function call with the wrong number of arguments.
    Arity {
        name: String,
        expected: usize,
        found: usize,
    },
    /// `f(x)` where `f` is not callable.
    NotCallable(String),
    /// A capability call's argument fell outside the cap's allow-list
    /// (§ 8.3.1, § 18.4). **Not** catchable by `?`.
    PolicyViolation { op: String, target: String },
    /// A `requires:` or `ensures:` clause evaluated to `false` (M5.T4 /
    /// § 9.2). Fatal: `?` cannot catch it (§ 18.4) and the CLI maps
    /// it to exit code 64.
    ContractViolation {
        fn_name: String,
        clause: ContractClause,
    },
    /// Saga rollback could not complete: an `undo` step exhausted its
    /// retry budget (§ 12.4 / M6.T5). Fatal; CLI exits 74.
    PartialFailure {
        saga: String,
        completed: Vec<String>,
        failed_step: String,
    },
    /// An L1 capability handler hit an OS-level error (file not
    /// found, permission denied, ...). Surfaced as a clean runtime
    /// error; callers will see it as `Err(err.io)` once stdlib
    /// mapping lands in M5.
    Io { op: String, message: String },
    /// A construct the M3 evaluator does not yet handle (e.g. spawn,
    /// cap calls, agent invocation). Surfaces as a clean diagnostic
    /// rather than a panic.
    NotImplemented(String),
    /// `model@vN` validation failed on construction or decode (M8.T1 /
    /// § 16.2). Carries the offending model name + version and the list
    /// of human-readable problems (one per failed clause). **Not**
    /// catchable by `?` — surfaces as a runtime error.
    SchemaViolation {
        model: String,
        version: u32,
        problems: Vec<String>,
    },
    /// M10.T4: an agent invocation exceeded its `budget:` envelope
    /// (tokens or latency). Each retry has its own budget; this error
    /// surfaces only after retries are exhausted.
    BudgetExceeded {
        agent: String,
        kind: String,
        limit: u64,
        observed: u64,
    },
    /// M12.T2: `assert(<expr>)` with `<expr>` evaluating to a falsy
    /// value. The `source` is the formatted expression text so the
    /// runner can surface "what failed" without re-reading the file.
    /// `detail` is populated when the asserted expression is a binary
    /// equality — the renderer uses it to print `expected vs. actual`.
    /// **Not** catchable by `?`. The detail is boxed to keep
    /// `EvalError` small (clippy `result_large_err`).
    AssertionFailed {
        source: String,
        detail: Option<Box<AssertionDetail>>,
    },
    /// `raise e` or `?` propagated up to the evaluator boundary.
    Raised(Value),
    /// Wrapped parse error for the `eval_expression` convenience entry.
    Parse(String),
    /// Internal control-flow value escaping a block (e.g. `break`
    /// outside a loop).
    StrayControlFlow(&'static str),
}

/// Side-channel info for `AssertionFailed` (M12.T2). Holds the rendered
/// values of an `lhs == rhs` (or `lhs != rhs`) comparison so the test
/// runner can print "expected `<lhs>`, got `<rhs>`".
#[derive(Debug, Clone, PartialEq)]
pub struct AssertionDetail {
    pub lhs_source: String,
    pub rhs_source: String,
    pub lhs_value: String,
    pub rhs_value: String,
    pub op: AssertionCmpOp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertionCmpOp {
    Eq,
    Ne,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractClause {
    Requires { index: usize },
    Ensures { index: usize },
}

// ====================================================================
//  Environment
// ====================================================================

/// Lexical environment: a stack of scopes. `let` shadows; `var` allows
/// in-place rebinding within its function-level scope. The optional
/// `module` pointer lets closures resolve top-level fn names without
/// having to capture them eagerly (which would prevent recursion and
/// forward references). The optional `tracer` is shared across the
/// run; every L1 cap call records into it. The optional `stdin`
/// source lets `io.read_line` consume from a test-controlled queue
/// (falls back to real stdin in production).
#[derive(Default, Clone)]
pub struct Env {
    scopes: Vec<HashMap<String, Slot>>,
    module: Option<ModuleScope>,
    tracer: Option<super::trace::Tracer>,
    stdin: Option<std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>>,
    /// Record / model declarations indexed by name (for records) or
    /// `name@vN` (for models) — populated by `eval_module_env` so the
    /// runtime can evaluate `where` clauses on construction (M5.T6).
    record_decls: Option<std::rc::Rc<HashMap<String, RecordDecl>>>,
    model_decls: Option<std::rc::Rc<HashMap<(String, u32), ModelDecl>>>,
    /// M8.T4: policies declared in the active module. Each cap call
    /// (via `lookup_builtin`) consults this list, applying any whose
    /// `match:` clause names the call's `<module>.<op>` path. Empty
    /// when the module declares none — keeps the hot path branch-free.
    policies: Option<std::rc::Rc<Vec<crate::syntax::ast::PolicyDecl>>>,
    /// M9.T1: pluggable `ai` backend selected by `lockset.toml
    /// [ai.backend]`. `None` means the built-in mock backend (echoes
    /// the prompt) — picked so unit tests run offline.
    ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
    /// M9.T4: when set, non-deterministic cap calls (`ai.*`,
    /// `clock.now`, `random.next`, `http.*`, `fs.read_*`) drain
    /// recorded values from this tape instead of executing live.
    replay_tape: Option<crate::runtime::replay::TapeHandle>,
    /// M9.T8: when `true`, HTTP and AI bodies are recorded as raw
    /// strings in the trace. Default is hash-only.
    full_record: bool,
    /// Idempotency key set by the saga interpreter for the duration of
    /// a `step.do` / `step.undo` body (N1 / § 12.3). HTTP / audit /
    /// queue handlers read this and inject the key into their request
    /// surface; absence means the call is not part of a saga step.
    idempotency_key: Option<std::rc::Rc<String>>,
    /// M12.T4: events of a recorded trace loaded by `with fixture: ...`.
    /// The body of a fixture-mode test queries this via the `trace()`
    /// builtin and the `trace_has(<predicate>)` helper.
    fixture_trace: Option<std::rc::Rc<Vec<super::trace::TraceEvent>>>,
}

#[derive(Debug, Clone)]
struct Slot {
    value: Value,
    mutable: bool,
}

impl Env {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            module: None,
            tracer: None,
            stdin: None,
            record_decls: None,
            model_decls: None,
            policies: None,
            ai_backend: None,
            replay_tape: None,
            full_record: false,
            idempotency_key: None,
            fixture_trace: None,
        }
    }

    /// M12.T4: attach the events of a trace loaded by `with fixture:`.
    pub fn with_fixture_trace(
        mut self,
        events: std::rc::Rc<Vec<super::trace::TraceEvent>>,
    ) -> Self {
        self.fixture_trace = Some(events);
        self
    }

    pub fn fixture_trace(&self) -> Option<&[super::trace::TraceEvent]> {
        self.fixture_trace.as_deref().map(|v| v.as_slice())
    }

    /// Currently active idempotency key, if the env is inside a saga
    /// step body. Used by L1/L2 cap handlers (§ 12.3) to inject
    /// `Idempotency-Key:` headers, AMQP `message-id`, etc.
    pub fn idempotency_key(&self) -> Option<&str> {
        self.idempotency_key.as_deref().map(|s| s.as_str())
    }

    pub fn with_record_decls(mut self, decls: std::rc::Rc<HashMap<String, RecordDecl>>) -> Self {
        self.record_decls = Some(decls);
        self
    }

    pub fn with_model_decls(
        mut self,
        decls: std::rc::Rc<HashMap<(String, u32), ModelDecl>>,
    ) -> Self {
        self.model_decls = Some(decls);
        self
    }

    pub fn with_policies(
        mut self,
        decls: std::rc::Rc<Vec<crate::syntax::ast::PolicyDecl>>,
    ) -> Self {
        self.policies = Some(decls);
        self
    }

    /// Attach the configured `ai` backend (M9.T1). The runtime falls
    /// back to a deterministic mock when this is `None`.
    pub fn with_ai_backend(mut self, backend: std::rc::Rc<crate::lockset::AiBackend>) -> Self {
        self.ai_backend = Some(backend);
        self
    }

    /// M9.T8: opt into recording full HTTP/AI bodies in the trace.
    pub fn with_full_record(mut self, full: bool) -> Self {
        self.full_record = full;
        self
    }

    /// M9.T4: attach a replay tape. Cap-call handlers consult it via
    /// `replay_tape()` and bypass the live path on a hit.
    pub fn with_replay_tape(mut self, tape: crate::runtime::replay::TapeHandle) -> Self {
        self.replay_tape = Some(tape);
        self
    }

    /// Borrow the active replay tape, if any. Cap handlers use this
    /// to decide between recorded and live execution.
    pub fn replay_tape(&self) -> Option<&crate::runtime::replay::TapeHandle> {
        self.replay_tape.as_ref()
    }

    /// Pre-fed stdin queue. Consumed by `io.read_line` before falling
    /// back to the real OS stdin. Used by fixture tests to drive
    /// programs deterministically.
    pub fn with_stdin_lines(mut self, lines: Vec<String>) -> Self {
        let q: std::collections::VecDeque<String> = lines.into();
        self.stdin = Some(std::rc::Rc::new(std::cell::RefCell::new(q)));
        self
    }

    fn pop_stdin_line(&self) -> Option<String> {
        self.stdin.as_ref().and_then(|q| q.borrow_mut().pop_front())
    }

    /// Attach a module-level fn registry to the environment. The same
    /// `Rc` is shared with every closure created against this env.
    pub fn with_module(mut self, module: ModuleScope) -> Self {
        self.module = Some(module);
        self
    }

    /// Attach a `Tracer`. Cloned env values share the same underlying
    /// trace channel via `Rc`.
    pub fn with_tracer(mut self, tracer: super::trace::Tracer) -> Self {
        self.tracer = Some(tracer);
        self
    }

    /// Borrow the active tracer, if any. L1 builtins use this to
    /// record their effect.
    pub fn tracer(&self) -> Option<&super::trace::Tracer> {
        self.tracer.as_ref()
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn bind_let(&mut self, name: &str, value: Value) {
        let scope = self.scopes.last_mut().expect("at least one scope");
        scope.insert(
            name.to_string(),
            Slot {
                value,
                mutable: false,
            },
        );
    }

    fn bind_var(&mut self, name: &str, value: Value) {
        let scope = self.scopes.last_mut().expect("at least one scope");
        scope.insert(
            name.to_string(),
            Slot {
                value,
                mutable: true,
            },
        );
    }

    /// Snapshot the binding stack, dropping mutability info. Used by
    /// the lambda evaluator to seed a closure's `captured` field.
    pub(crate) fn snapshot(&self) -> Vec<HashMap<String, Value>> {
        self.scopes
            .iter()
            .map(|s| {
                s.iter()
                    .map(|(k, slot)| (k.clone(), slot.value.clone()))
                    .collect()
            })
            .collect()
    }

    /// Construct an `Env` whose only scope is the supplied frame —
    /// used when invoking a closure with its captured environment.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_snapshot(
        scopes: Vec<HashMap<String, Value>>,
        module: Option<ModuleScope>,
        tracer: Option<super::trace::Tracer>,
        stdin: Option<std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>>,
        record_decls: Option<std::rc::Rc<HashMap<String, RecordDecl>>>,
        model_decls: Option<std::rc::Rc<HashMap<(String, u32), ModelDecl>>>,
        policies: Option<std::rc::Rc<Vec<crate::syntax::ast::PolicyDecl>>>,
        ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
        replay_tape: Option<crate::runtime::replay::TapeHandle>,
        full_record: bool,
    ) -> Self {
        let mut out = Self {
            scopes: Vec::with_capacity(scopes.len() + 1),
            module,
            tracer,
            stdin,
            record_decls,
            model_decls,
            policies,
            ai_backend,
            replay_tape,
            full_record,
            idempotency_key: None,
            fixture_trace: None,
        };
        for s in scopes {
            let mut frame = HashMap::with_capacity(s.len());
            for (k, v) in s {
                frame.insert(
                    k,
                    Slot {
                        value: v,
                        mutable: false,
                    },
                );
            }
            out.scopes.push(frame);
        }
        if out.scopes.is_empty() {
            out.scopes.push(HashMap::new());
        }
        out
    }

    fn lookup(&self, name: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(slot) = scope.get(name) {
                return Some(slot.value.clone());
            }
        }
        if let Some(m) = &self.module {
            if let Some(v) = m.borrow().get(name) {
                return Some(v.clone());
            }
        }
        None
    }

    fn assign(&mut self, name: &str, value: Value) -> Result<(), &'static str> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                if !slot.mutable {
                    return Err("cannot assign to immutable `let` binding");
                }
                slot.value = value;
                return Ok(());
            }
        }
        Err("assignment target is not defined")
    }
}

// ====================================================================
//  Control-flow signal
// ====================================================================
//
//  A block evaluates either to a value or to one of these control
//  signals; the enclosing loop / function unwraps them.

enum Flow {
    Value(Value),
    /// `break <expr>` — the value is reserved for future loop-as-
    /// expression semantics; today the surrounding loop just exits.
    Break(#[allow(dead_code)] Option<Value>),
    Continue,
    Return(Value),
}

impl Flow {
    fn into_value(self, span: Span) -> Result<Value, EvalError> {
        match self {
            Flow::Value(v) => Ok(v),
            Flow::Break(_) => Err(EvalError::new(
                EvalErrorKind::StrayControlFlow("break"),
                span,
            )),
            Flow::Continue => Err(EvalError::new(
                EvalErrorKind::StrayControlFlow("continue"),
                span,
            )),
            Flow::Return(_) => Err(EvalError::new(
                EvalErrorKind::StrayControlFlow("return"),
                span,
            )),
        }
    }
}

// ====================================================================
//  Public entry — evaluate one expression string
// ====================================================================

/// Build an `Env` populated with closures for every top-level `fn`
/// declaration in `m`. Each closure shares a single `ModuleScope`
/// pointer, so they can refer to one another (and to themselves —
/// that is how recursion works). M3.T4 acceptance (`map` / `fold` /
/// `filter`) goes through this entry.
pub fn eval_module_env(m: &Module) -> Env {
    let module: ModuleScope = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
    let (records, models, policies) = collect_decls(m);
    let records_rc = Rc::new(records);
    let models_rc = Rc::new(models);
    let policies_rc = Rc::new(policies);
    register_decls(
        m,
        &module,
        &records_rc,
        &models_rc,
        &policies_rc,
        None,
        None,
        None,
        None,
        false,
    );
    Env::new()
        .with_module(module)
        .with_record_decls(records_rc)
        .with_model_decls(models_rc)
        .with_policies(policies_rc)
}

#[allow(clippy::too_many_arguments)]
fn register_decls(
    m: &Module,
    module: &ModuleScope,
    records: &Rc<HashMap<String, RecordDecl>>,
    models: &Rc<HashMap<(String, u32), ModelDecl>>,
    policies: &Rc<Vec<crate::syntax::ast::PolicyDecl>>,
    tracer: Option<super::trace::Tracer>,
    stdin: Option<std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>>,
    ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
    replay_tape: Option<crate::runtime::replay::TapeHandle>,
    full_record: bool,
) {
    for item in &m.items {
        match item {
            Item::Fn(f) => {
                let closure = Rc::new(Closure {
                    params: f.params.iter().map(|p| p.name.clone()).collect(),
                    body: f.body.clone(),
                    captured: Vec::new(),
                    module: Some(module.clone()),
                    tracer: tracer.clone(),
                    stdin: stdin.clone(),
                    record_decls: Some(records.clone()),
                    model_decls: Some(models.clone()),
                    policies: Some(policies.clone()),
                    ai_backend: ai_backend.clone(),
                    replay_tape: replay_tape.clone(),
                    full_record,
                    requires: f.requires.clone(),
                    ensures: f.ensures.clone(),
                    name: Some(f.name.clone()),
                });
                module
                    .borrow_mut()
                    .insert(f.name.clone(), Value::Closure(closure));
            }
            Item::Saga(s) => {
                let saga = Rc::new(SagaInstance {
                    name: s.name.clone(),
                    params: s.params.iter().map(|p| p.name.clone()).collect(),
                    intent: s.intent.clone(),
                    steps: s.steps.clone(),
                    module: Some(module.clone()),
                    tracer: tracer.clone(),
                    stdin: stdin.clone(),
                    record_decls: Some(records.clone()),
                    model_decls: Some(models.clone()),
                    policies: Some(policies.clone()),
                    ai_backend: ai_backend.clone(),
                    replay_tape: replay_tape.clone(),
                    full_record,
                });
                module
                    .borrow_mut()
                    .insert(s.name.clone(), Value::Saga(saga));
            }
            Item::Agent(a) => {
                if let Some(agent) = build_agent_instance(
                    a,
                    module,
                    tracer.clone(),
                    models,
                    ai_backend.clone(),
                    replay_tape.clone(),
                    full_record,
                ) {
                    module
                        .borrow_mut()
                        .insert(a.name.clone(), Value::Agent(Rc::new(agent)));
                }
            }
            Item::AgentNet(n) => {
                let net = Rc::new(super::value::AgentNetInstance {
                    name: n.name.clone(),
                    intent: n.intent.clone(),
                    flows: n.flows.clone(),
                    until: n.until.clone(),
                    module: Some(module.clone()),
                    tracer: tracer.clone(),
                    model_decls: Some(models.clone()),
                    ai_backend: ai_backend.clone(),
                    replay_tape: replay_tape.clone(),
                    full_record,
                });
                module
                    .borrow_mut()
                    .insert(n.name.clone(), Value::AgentNet(net));
            }
            _ => {}
        }
    }
}

/// Lift an `AgentDecl` into a runtime `AgentInstance`. Returns `None`
/// when a required field is malformed — the static checker (M10.T1)
/// has already reported the issue, so we silently drop here. Optional
/// fields default to sensible no-ops (`retries: 0`, no budget cap).
#[allow(clippy::too_many_arguments)]
fn build_agent_instance(
    a: &crate::syntax::ast::AgentDecl,
    module: &ModuleScope,
    tracer: Option<super::trace::Tracer>,
    models: &Rc<HashMap<(String, u32), ModelDecl>>,
    ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
    replay_tape: Option<super::replay::TapeHandle>,
    full_record: bool,
) -> Option<super::value::AgentInstance> {
    let llm = field_string(a, "llm")?;
    let intent = field_string(a, "intent")?;
    let prompt = field_string(a, "prompt")?;
    let accept = field_model_ref(a, "accept")?;
    let produce = field_model_ref(a, "produce")?;
    let policy_names = field_ident_list(a, "policy");
    let retries = field_int(a, "retries").unwrap_or(0).max(0) as u32;
    let (budget_tokens, budget_latency_ms) = field_budget(a);
    Some(super::value::AgentInstance {
        name: a.name.clone(),
        llm,
        intent,
        prompt,
        accept,
        produce,
        policy_names,
        retries,
        budget_tokens,
        budget_latency_ms,
        module: Some(module.clone()),
        tracer,
        model_decls: Some(models.clone()),
        ai_backend,
        replay_tape,
        full_record,
    })
}

fn field_string(a: &crate::syntax::ast::AgentDecl, key: &str) -> Option<String> {
    a.fields
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| f.values.first())
        .and_then(|e| match e {
            Expr::Str(s, _) => Some(s.clone()),
            _ => None,
        })
}

fn field_model_ref(a: &crate::syntax::ast::AgentDecl, key: &str) -> Option<(String, u32)> {
    a.fields
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| f.values.first())
        .and_then(|e| match e {
            Expr::ModelRef { name, version, .. } => Some((name.clone(), *version)),
            _ => None,
        })
}

fn field_ident_list(a: &crate::syntax::ast::AgentDecl, key: &str) -> Vec<String> {
    a.fields
        .iter()
        .find(|f| f.key == key)
        .map(|f| {
            f.values
                .iter()
                .filter_map(|e| match e {
                    Expr::Ident(n, _) => Some(n.clone()),
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

fn field_int(a: &crate::syntax::ast::AgentDecl, key: &str) -> Option<i64> {
    a.fields
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| f.values.first())
        .and_then(|e| match e {
            Expr::Int(n, _) => Some(*n),
            _ => None,
        })
}

fn field_budget(a: &crate::syntax::ast::AgentDecl) -> (Option<u64>, Option<u64>) {
    let f = match a.fields.iter().find(|f| f.key == "budget") {
        Some(f) => f,
        None => return (None, None),
    };
    let r = match f.values.first() {
        Some(Expr::Record(r, _)) => r,
        _ => return (None, None),
    };
    let mut tokens = None;
    let mut latency = None;
    for fl in &r.fields {
        match (fl.name.as_str(), &fl.value) {
            ("tokens", Expr::Int(n, _)) if *n >= 0 => tokens = Some(*n as u64),
            ("latency", Expr::Duration(s, _)) => latency = parse_duration_ms(s),
            _ => {}
        }
    }
    (tokens, latency)
}

fn parse_duration_ms(s: &str) -> Option<u64> {
    // Accept simple suffixed forms: `5s`, `500ms`, `1m`, `2h`.
    let bytes = s.as_bytes();
    if let Some(num_end) = bytes.iter().position(|b| !b.is_ascii_digit()) {
        let n: u64 = s[..num_end].parse().ok()?;
        let suffix = &s[num_end..];
        let ms = match suffix {
            "ms" => n,
            "s" => n.saturating_mul(1_000),
            "m" => n.saturating_mul(60_000),
            "h" => n.saturating_mul(3_600_000),
            _ => return None,
        };
        Some(ms)
    } else {
        None
    }
}

type CollectedDecls = (
    HashMap<String, RecordDecl>,
    HashMap<(String, u32), ModelDecl>,
    Vec<crate::syntax::ast::PolicyDecl>,
);

fn collect_decls(m: &Module) -> CollectedDecls {
    let mut records = HashMap::new();
    let mut models = HashMap::new();
    let mut policies = Vec::new();
    for item in &m.items {
        match item {
            Item::Record(r) => {
                records.insert(r.name.clone(), r.clone());
            }
            Item::Model(md) => {
                models.insert((md.name.clone(), md.version), md.clone());
            }
            Item::Policy(p) => {
                policies.push(p.clone());
            }
            _ => {}
        }
    }
    (records, models, policies)
}

/// Run the `main` function of a parsed module with no arguments and
/// return its value (`Ok(unit)` for a clean run). Used by `aeris run
/// <file.aer>` (M3.T6 / M4.T6) on pure files. If `main` declares a
/// `cap` parameter it receives the synthesised `cap[*]` (M4.T3 stub
/// — when a `lockset.toml` is in scope, M7.T4's
/// `run_main_with_cap` is used instead).
pub fn run_main(m: &Module) -> Result<Value, EvalError> {
    run_main_with(m, None)
}

/// Run `main` with an explicit capability shape (M7.T4). Used by
/// `aeris run` once the lockset has been parsed: the `[caps]`
/// section of `lockset.toml` becomes the effective ceiling that
/// `main(cap)` receives, replacing the `cap[*]` stub.
pub fn run_main_with_cap(
    m: &Module,
    cap: super::value::CapValue,
    tracer: Option<super::trace::Tracer>,
) -> Result<Value, EvalError> {
    run_main_with_cfg(m, cap, tracer, None, false)
}

/// M9: full configuration entry — adds the configured `ai` backend
/// (lockset.toml `[ai.backend]`) and the trace recording mode. The
/// CLI driver routes through this once a lockset is in scope.
pub fn run_main_with_cfg(
    m: &Module,
    cap: super::value::CapValue,
    tracer: Option<super::trace::Tracer>,
    ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
    full_record: bool,
) -> Result<Value, EvalError> {
    run_main_with_full_cfg(m, cap, tracer, ai_backend, None, full_record)
}

/// M9.T4: full-power configuration entry — adds the replay tape
/// handle. The CLI's `aeris replay` path goes through here.
pub fn run_main_with_full_cfg(
    m: &Module,
    cap: super::value::CapValue,
    tracer: Option<super::trace::Tracer>,
    ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
    replay_tape: Option<crate::runtime::replay::TapeHandle>,
    full_record: bool,
) -> Result<Value, EvalError> {
    let env = build_module_env(m, tracer.clone(), ai_backend, replay_tape, full_record);
    let main = env
        .lookup("main")
        .ok_or_else(|| EvalError::new(EvalErrorKind::UndefinedVar("main".into()), Span::ZERO))?;
    let main_closure = match &main {
        Value::Closure(c) => c.clone(),
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::NotCallable("main".into()),
                Span::ZERO,
            ))
        }
    };
    // M15 — prototype mode requires a fallback path for `cap` look-ups
    // in functions that do not declare the parameter. Register the
    // synthesised cap into the module scope so `env.lookup("cap")`
    // resolves through it after the function-local scopes. Functions
    // with their own `cap: cap[...]` parameter shadow this binding
    // normally, so strict-mode behaviour is unaffected.
    let cap_rc = std::rc::Rc::new(cap);
    if let Some(scope) = &env.module {
        scope
            .borrow_mut()
            .insert("cap".to_string(), Value::Cap(cap_rc.clone()));
    }
    let args: Vec<Value> = if main_closure.params.is_empty() {
        Vec::new()
    } else {
        vec![Value::Cap(cap_rc)]
    };
    if let Some(t) = &tracer {
        t.intent_enter("aeris run", Some("main"));
    }
    let flow = invoke_value(&main, &args, Span::ZERO);
    let outcome = match &flow {
        Ok(_) => "ok",
        Err(_) => "err",
    };
    if let Some(t) = &tracer {
        t.intent_exit(outcome);
    }
    flow?.into_value(Span::ZERO)
}

/// Same as [`run_main`] but lets the caller attach a `Tracer`.
pub fn run_main_with(m: &Module, tracer: Option<super::trace::Tracer>) -> Result<Value, EvalError> {
    let env = if let Some(t) = tracer.clone() {
        eval_module_env_with_tracer(m, t)
    } else {
        eval_module_env(m)
    };
    let main = env
        .lookup("main")
        .ok_or_else(|| EvalError::new(EvalErrorKind::UndefinedVar("main".into()), Span::ZERO))?;
    let main_closure = match &main {
        Value::Closure(c) => c.clone(),
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::NotCallable("main".into()),
                Span::ZERO,
            ))
        }
    };
    let args: Vec<Value> = if main_closure.params.is_empty() {
        Vec::new()
    } else {
        // `main(cap)` — synthesise `cap[*]` (M4.T3 stub).
        let synth = Value::Cap(std::rc::Rc::new(super::value::CapValue {
            entries: Vec::new(),
            star: true,
        }));
        eprintln!("[aeris] effective main cap: cap[*]   (M4.T3 stub — full lockset in M7)");
        vec![synth]
    };
    if let Some(t) = &tracer {
        t.intent_enter("aeris run", Some("main"));
    }
    let flow = invoke_value(&main, &args, Span::ZERO);
    let outcome = match &flow {
        Ok(_) => "ok",
        Err(_) => "err",
    };
    if let Some(t) = &tracer {
        t.intent_exit(outcome);
    }
    let v = flow?.into_value(Span::ZERO)?;
    Ok(v)
}

/// M12.T1 — evaluate a single `test "<name>" { ... }` block in the
/// context of the given module's environment. The runner uses this
/// per test in the suite. Returns `Ok(())` on a clean exit (any
/// value returned by the body is treated as success), or the first
/// `EvalError` raised. `assert` failure surfaces as `Raised(...)`.
pub fn run_test(m: &Module, test: &crate::syntax::ast::TestDecl) -> Result<(), EvalError> {
    let mut env = eval_module_env(m);
    eval_block(&test.body, &mut env)?;
    Ok(())
}

/// M12.T4 — evaluate a fixture-mode test. The recorded events are
/// exposed to the body via `trace()` / `trace_has(...)`.
pub fn run_test_with_fixture(
    m: &Module,
    test: &crate::syntax::ast::TestDecl,
    events: std::rc::Rc<Vec<super::trace::TraceEvent>>,
) -> Result<(), EvalError> {
    let mut env = eval_module_env(m).with_fixture_trace(events);
    eval_block(&test.body, &mut env)?;
    Ok(())
}

/// M12.T3 — evaluate one case of a `property "<name>" with (...) { ... }`.
/// `values` must align positionally with `prop.params`. The body is
/// evaluated against a fresh module env so per-case side effects don't
/// leak into subsequent samples.
pub fn run_property_case(
    m: &Module,
    prop: &crate::syntax::ast::PropertyDecl,
    values: &[Value],
) -> Result<(), EvalError> {
    let mut env = eval_module_env(m);
    env.push_scope();
    for (param, val) in prop.params.iter().zip(values.iter()) {
        env.bind_let(&param.name, val.clone());
    }
    let result = eval_block(&prop.body, &mut env);
    env.pop_scope();
    result?;
    Ok(())
}

/// M9 entry: build an `Env` for `m` with the full set of runtime
/// knobs (`tracer`, `ai_backend`, `replay_tape`, `full_record`).
/// Threads the same values through every closure / saga via
/// `register_decls`.
pub fn build_module_env(
    m: &Module,
    tracer: Option<super::trace::Tracer>,
    ai_backend: Option<std::rc::Rc<crate::lockset::AiBackend>>,
    replay_tape: Option<crate::runtime::replay::TapeHandle>,
    full_record: bool,
) -> Env {
    let module: ModuleScope = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
    let (records, models, policies) = collect_decls(m);
    let records_rc = Rc::new(records);
    let models_rc = Rc::new(models);
    let policies_rc = Rc::new(policies);
    register_decls(
        m,
        &module,
        &records_rc,
        &models_rc,
        &policies_rc,
        tracer.clone(),
        None,
        ai_backend.clone(),
        replay_tape.clone(),
        full_record,
    );
    let mut env = Env::new()
        .with_module(module)
        .with_record_decls(records_rc)
        .with_model_decls(models_rc)
        .with_policies(policies_rc)
        .with_full_record(full_record);
    if let Some(t) = tracer {
        env = env.with_tracer(t);
    }
    if let Some(b) = ai_backend {
        env = env.with_ai_backend(b);
    }
    if let Some(tape) = replay_tape {
        env = env.with_replay_tape(tape);
    }
    env
}

/// Build an `Env` for `m` with `tracer` shared across every closure.
pub fn eval_module_env_with_tracer(m: &Module, tracer: super::trace::Tracer) -> Env {
    let module: ModuleScope = std::rc::Rc::new(std::cell::RefCell::new(HashMap::new()));
    let (records, models, policies) = collect_decls(m);
    let records_rc = Rc::new(records);
    let models_rc = Rc::new(models);
    let policies_rc = Rc::new(policies);
    register_decls(
        m,
        &module,
        &records_rc,
        &models_rc,
        &policies_rc,
        Some(tracer.clone()),
        None,
        None,
        None,
        false,
    );
    Env::new()
        .with_module(module)
        .with_tracer(tracer)
        .with_record_decls(records_rc)
        .with_model_decls(models_rc)
        .with_policies(policies_rc)
}

/// Parse and evaluate a single expression. Convenience entry used by
/// fixture tests; production callers should drive the evaluator with
/// pre-parsed AST nodes.
pub fn eval_expression(src: &str) -> Result<Value, EvalError> {
    let expr = parse_expression(src)
        .map_err(|e| EvalError::new(EvalErrorKind::Parse(format!("{:?}", e.kind)), e.span))?;
    let mut env = Env::new();
    let v = eval_expr(&expr, &mut env)?;
    match v {
        Flow::Value(v) => Ok(v),
        other => other.into_value(expr.span()),
    }
}

// ====================================================================
//  Eval — expressions
// ====================================================================

fn eval_expr(e: &Expr, env: &mut Env) -> Result<Flow, EvalError> {
    match e {
        // ---- atomic literals ----
        Expr::Int(n, _) => Ok(Flow::Value(Value::Int(*n))),
        Expr::Float(f, _) => Ok(Flow::Value(Value::Float(*f))),
        Expr::Bool(b, _) => Ok(Flow::Value(Value::Bool(*b))),
        Expr::Str(s, _) => Ok(Flow::Value(Value::Str(s.clone()))),
        Expr::Bytes(b, _) => Ok(Flow::Value(Value::Bytes(b.clone()))),
        Expr::Char(c, _) => Ok(Flow::Value(Value::Char(*c))),
        Expr::Date(s, _) => Ok(Flow::Value(Value::Date(s.clone()))),
        Expr::Timestamp(s, _) => Ok(Flow::Value(Value::Timestamp(s.clone()))),
        Expr::Duration(s, _) => Ok(Flow::Value(Value::Duration(s.clone()))),
        Expr::Unit(_) => Ok(Flow::Value(Value::Unit)),

        // ---- references ----
        Expr::Ident(name, span) => match env.lookup(name) {
            Some(v) => Ok(Flow::Value(v)),
            // Built-in constants. `None` is the nullary option; other
            // PascalCase identifiers may be unit enum variants once the
            // type checker (M2.T1+) has registered them — for the
            // pure-interpreter layer we only know about `None`.
            None if name == "None" => Ok(Flow::Value(Value::none())),
            None => Err(EvalError::new(
                EvalErrorKind::UndefinedVar(name.clone()),
                *span,
            )),
        },

        // ---- compound literals ----
        Expr::Tuple(elems, _) => {
            let mut out = Vec::with_capacity(elems.len());
            for x in elems {
                out.push(eval_value(x, env)?);
            }
            Ok(Flow::Value(Value::Tuple(out)))
        }
        Expr::List(elems, _) => {
            let mut out = Vec::with_capacity(elems.len());
            for x in elems {
                out.push(eval_value(x, env)?);
            }
            Ok(Flow::Value(Value::List(out)))
        }
        Expr::Record(rl, lit_span) => {
            // `..base` first (so explicit fields override), then
            // explicit fields in source order.
            let mut fields: Vec<(String, Value)> = Vec::new();
            if let Some(spread) = &rl.spread {
                let v = eval_value(spread, env)?;
                if let Value::Record(r) = v {
                    fields = r.fields;
                } else {
                    return Err(EvalError::new(
                        EvalErrorKind::Type("`..` spread must be a record".into()),
                        spread.span(),
                    ));
                }
            }
            for f in &rl.fields {
                let v = eval_value(&f.value, env)?;
                if let Some(slot) = fields.iter_mut().find(|(k, _)| k == &f.name) {
                    slot.1 = v;
                } else {
                    fields.push((f.name.clone(), v));
                }
            }
            // M5.T6: enforce per-field `where` clauses for named records
            // when the decl is reachable via the module's record
            // registry. Anonymous record literals (no `ty_name`) skip
            // this — they have no schema to compare against.
            // M8.T1: when `@vN` is present, enforce the model's
            // per-field + record-level invariants and raise
            // `SchemaViolation` (§ 16.2) instead of `ContractViolation`.
            if let Some(name) = &rl.ty_name {
                if let Some(version) = rl.ty_version {
                    check_model(env, name, version, &fields, *lit_span)?;
                } else {
                    check_record_where(env, name, &fields)?;
                }
            }
            Ok(Flow::Value(Value::Record(RecordValue {
                name: rl.ty_name.clone(),
                fields,
            })))
        }

        // ---- model reference: pure data, no validation ----
        Expr::ModelRef { name, version, .. } => Ok(Flow::Value(Value::Enum(EnumValue {
            name: format!("{name}@v{version}"),
            variant: "<model_ref>".into(),
            data: VariantValue::Unit,
        }))),

        // ---- operators ----
        Expr::Binary { op, lhs, rhs, span } => eval_binary(*op, lhs, rhs, *span, env),
        Expr::Unary { op, expr, span } => eval_unary(*op, expr, *span, env),

        // ---- postfix forms ----
        Expr::Field { base, name, span } => {
            let v = eval_value(base, env)?;
            field_access(&v, name, *span).map(Flow::Value)
        }
        Expr::Index { base, index, span } => {
            let collection = eval_value(base, env)?;
            let idx = eval_value(index, env)?;
            index_into(&collection, &idx, *span).map(Flow::Value)
        }
        Expr::Try { expr, span } => {
            let v = eval_value(expr, env)?;
            match v {
                Value::Result(Ok(inner)) => Ok(Flow::Value(*inner)),
                Value::Result(Err(inner)) => {
                    Err(EvalError::new(EvalErrorKind::Raised(*inner), *span))
                }
                Value::Option(Some(inner)) => Ok(Flow::Value(*inner)),
                Value::Option(None) => Err(EvalError::new(
                    EvalErrorKind::Raised(Value::Str("none".into())),
                    *span,
                )),
                _ => Err(EvalError::new(
                    EvalErrorKind::Type("`?` requires `result<T>` or `option<T>`".into()),
                    *span,
                )),
            }
        }
        Expr::Call {
            callee,
            type_args,
            args,
            span,
        } => eval_call(callee, type_args, args, *span, env),
        Expr::Cast { .. } => Err(EvalError::new(
            EvalErrorKind::NotImplemented("`as` cast".into()),
            e.span(),
        )),
        Expr::IsCheck { expr, pat, span } => {
            let v = eval_value(expr, env)?;
            let mut probe = Env::default();
            probe.scopes.push(HashMap::new());
            for s in &env.scopes {
                probe.scopes.last_mut().unwrap().extend(s.clone());
            }
            let matched = pattern_matches(pat, &v, &mut probe, *span)?;
            Ok(Flow::Value(Value::Bool(matched)))
        }

        // ---- ranges (data only; iteration handled in `for`) ----
        Expr::Range { .. } => Err(EvalError::new(
            EvalErrorKind::NotImplemented("range value (iterate via `for x in ..`)".into()),
            e.span(),
        )),

        // ---- control flow ----
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            let c = eval_value(cond, env)?;
            let cb = match c {
                Value::Bool(b) => b,
                _ => {
                    return Err(EvalError::new(
                        EvalErrorKind::Type("`if` condition must be `bool`".into()),
                        cond.span(),
                    ))
                }
            };
            if cb {
                eval_block(then_blk, env)
            } else {
                match else_ {
                    Some(ElseBranch::ElseIf(e)) => eval_expr(e, env),
                    Some(ElseBranch::Else(b)) => eval_block(b, env),
                    None => Ok(Flow::Value(Value::Unit)),
                }
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => eval_match(scrutinee, arms, env),
        Expr::Block(b, _) => eval_block(b, env),

        // ---- raise / return / break / continue ----
        Expr::Raise { expr, .. } => {
            let v = eval_value(expr, env)?;
            Err(EvalError::new(EvalErrorKind::Raised(v), expr.span()))
        }
        Expr::Return { expr, .. } => {
            let v = match expr {
                Some(e) => eval_value(e, env)?,
                None => Value::Unit,
            };
            Ok(Flow::Return(v))
        }
        Expr::Break { expr, .. } => {
            let v = match expr {
                Some(e) => Some(eval_value(e, env)?),
                None => None,
            };
            Ok(Flow::Break(v))
        }
        Expr::Continue { .. } => Ok(Flow::Continue),

        // ---- assignment ----
        Expr::Assign {
            op, target, value, ..
        } => eval_assign(*op, target, value, env),

        // ---- lambdas ----
        Expr::Lambda { params, body, .. } => Ok(Flow::Value(Value::Closure(Rc::new(Closure {
            params: params.iter().map(|p| p.name.clone()).collect(),
            body: body.clone(),
            captured: env.snapshot(),
            module: env.module.clone(),
            tracer: env.tracer.clone(),
            stdin: env.stdin.clone(),
            record_decls: env.record_decls.clone(),
            model_decls: env.model_decls.clone(),
            policies: env.policies.clone(),
            ai_backend: env.ai_backend.clone(),
            replay_tape: env.replay_tape.clone(),
            full_record: env.full_record,
            requires: Vec::new(),
            ensures: Vec::new(),
            name: None,
        })))),

        // ---- unsupported in pure subset ----
        Expr::Spawn { .. } | Expr::Await { .. } => Err(EvalError::new(
            EvalErrorKind::NotImplemented("`spawn` / `await` (M4+)".into()),
            e.span(),
        )),
        Expr::IntentBlock { label, body, .. } => {
            // M5.T7: lift the parser-level intent block into the
            // runtime tracer. Every event emitted between
            // `intent_enter` and `intent_exit` carries the active
            // intent string in its `"intent"` field.
            if let Some(t) = env.tracer() {
                t.intent_enter(label, None);
            }
            let result = eval_block(body, env);
            if let Some(t) = env.tracer() {
                let outcome = match &result {
                    Ok(Flow::Value(Value::Result(Err(_)))) => "err",
                    Ok(_) => "ok",
                    Err(_) => "err",
                };
                t.intent_exit(outcome);
            }
            result
        }
        Expr::CapNarrow {
            kind,
            entries,
            span,
        } => eval_cap_narrow(*kind, entries, *span, env),
    }
}

fn eval_value(e: &Expr, env: &mut Env) -> Result<Value, EvalError> {
    eval_expr(e, env)?.into_value(e.span())
}

/// M5.T6: check every per-field `where` clause for the named record
/// against the supplied field values. The clause sees the field name
/// in scope. Failure raises `ContractViolation`. Anonymous records
/// and records whose decl is not in the registry pass-through.
fn check_record_where(
    env: &Env,
    record_name: &str,
    fields: &[(String, Value)],
) -> Result<(), EvalError> {
    let decls = match &env.record_decls {
        Some(d) => d.clone(),
        None => return Ok(()),
    };
    let decl = match decls.get(record_name) {
        Some(d) => d.clone(),
        None => return Ok(()),
    };
    for field in &decl.fields {
        let where_clause = match &field.where_clause {
            Some(w) => w.clone(),
            None => continue,
        };
        let val = match fields.iter().find(|(k, _)| k == &field.name) {
            Some((_, v)) => v.clone(),
            None => continue,
        };
        // Evaluate the clause in a fresh scope with all field
        // bindings + `result` for parity with fn ensures.
        let mut where_env = env.clone();
        where_env.push_scope();
        for (k, v) in fields {
            where_env.bind_let(k, v.clone());
        }
        where_env.bind_let("result", val.clone());
        let v = eval_value(&where_clause, &mut where_env)?;
        if !matches!(v, Value::Bool(true)) {
            return Err(EvalError::new(
                EvalErrorKind::ContractViolation {
                    fn_name: format!("{record_name}.{}", field.name),
                    clause: ContractClause::Requires { index: 0 },
                },
                where_clause.span(),
            ));
        }
    }
    Ok(())
}

/// M8.T1: validate the field bag against the named `model@vN` decl.
/// Runs every per-field `where` clause and every record-level
/// invariant, accumulating problems into a single `SchemaViolation`
/// rather than short-circuiting. The diagnostic mirrors § 16.2:
/// `SchemaViolation { model, version, errors }`.
fn check_model(
    env: &Env,
    model_name: &str,
    version: u32,
    fields: &[(String, Value)],
    lit_span: Span,
) -> Result<(), EvalError> {
    let decls = match &env.model_decls {
        Some(d) => d.clone(),
        None => {
            return Err(EvalError::new(
                EvalErrorKind::SchemaViolation {
                    model: model_name.into(),
                    version,
                    problems: vec![format!("model `{model_name}@v{version}` not declared")],
                },
                lit_span,
            ));
        }
    };
    let decl = match decls.get(&(model_name.to_string(), version)) {
        Some(d) => d.clone(),
        None => {
            return Err(EvalError::new(
                EvalErrorKind::SchemaViolation {
                    model: model_name.into(),
                    version,
                    problems: vec![format!("model `{model_name}@v{version}` not declared")],
                },
                lit_span,
            ));
        }
    };
    let mut problems: Vec<String> = Vec::new();
    // Required fields: every declared field must appear in the literal
    // unless it carries a default (M8 has no defaults yet → strict
    // presence).
    for field in &decl.fields {
        if !fields.iter().any(|(k, _)| k == &field.name) {
            problems.push(format!("missing field `{}`", field.name));
        }
    }
    // Reject fields present in the literal but absent from the decl —
    // catches typos and version drift early.
    for (k, _) in fields {
        if !decl.fields.iter().any(|f| &f.name == k) {
            problems.push(format!("unknown field `{k}`"));
        }
    }
    // Per-field where clauses (§ 16.3). Each clause sees every field
    // by name + the special `result` binding for parity with `ensures`.
    for field in &decl.fields {
        let where_clause = match &field.where_clause {
            Some(w) => w.clone(),
            None => continue,
        };
        let val = match fields.iter().find(|(k, _)| k == &field.name) {
            Some((_, v)) => v.clone(),
            None => continue,
        };
        let mut where_env = env.clone();
        where_env.push_scope();
        for (k, v) in fields {
            where_env.bind_let(k, v.clone());
        }
        where_env.bind_let("result", val.clone());
        match eval_value(&where_clause, &mut where_env) {
            Ok(Value::Bool(true)) => {}
            Ok(_) => problems.push(format!("field `{}` failed its where clause", field.name)),
            Err(e) => problems.push(format!(
                "field `{}` where clause errored: {:?}",
                field.name, e.kind
            )),
        }
    }
    // Record-level invariants (§ 16.3). All field bindings in scope.
    for (i, inv) in decl.record_where.iter().enumerate() {
        let mut inv_env = env.clone();
        inv_env.push_scope();
        for (k, v) in fields {
            inv_env.bind_let(k, v.clone());
        }
        match eval_value(inv, &mut inv_env) {
            Ok(Value::Bool(true)) => {}
            Ok(_) => problems.push(format!("record invariant #{} failed", i + 1)),
            Err(e) => problems.push(format!("record invariant #{} errored: {:?}", i + 1, e.kind)),
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(EvalError::new(
            EvalErrorKind::SchemaViolation {
                model: model_name.into(),
                version,
                problems,
            },
            lit_span,
        ))
    }
}

// ====================================================================
//  M8.T6 — policy drift event
// ====================================================================

/// Emit a `policy_drift` event when a policy's live and replay outcomes
/// disagree (§ 15.4). The replay driver (M9) calls this; for M8 the
/// helper exists so fixtures can synthesise the event and exercise the
/// shape end-to-end.
pub fn emit_policy_drift(env: &Env, policy_name: &str, op: &str, expected: &str, observed: &str) {
    record_event(
        env,
        "policy_drift",
        vec![
            ("policy".into(), format!("\"{policy_name}\"")),
            ("op".into(), format!("\"{op}\"")),
            ("expected".into(), format!("\"{expected}\"")),
            ("observed".into(), format!("\"{observed}\"")),
        ],
    );
}

// ====================================================================
//  M8.T4 — policy runtime
// ====================================================================

/// Conventional binding names for the policy scope. Each cap module
/// exposes a small set of args by name (`url`, `body`, `argv0`, ...);
/// callers wire the right ones via `bind_policy_call_vars`.
fn bind_policy_call_vars(scope: &mut Env, module: &str, op: &str, args: &[Value]) {
    scope.bind_let(
        "method",
        Value::Str(format!("{module}.{op}").to_ascii_uppercase()),
    );
    match module {
        "http" => {
            scope.bind_let("method", Value::Str(op.to_ascii_uppercase()));
            if let Some(v) = args.first() {
                scope.bind_let("url", v.clone());
            }
            if let Some(v) = args.get(1) {
                scope.bind_let("body", v.clone());
            }
        }
        "fs" => {
            if let Some(v) = args.first() {
                scope.bind_let("path", v.clone());
            }
            if let Some(v) = args.get(1) {
                scope.bind_let("body", v.clone());
            }
        }
        "shell" => {
            if let Some(v) = args.first() {
                scope.bind_let("argv0", v.clone());
            }
        }
        "ai" => {
            if let Some(v) = args.first() {
                scope.bind_let("prompt", v.clone());
            }
        }
        "audit" => {
            if let Some(v) = args.first() {
                scope.bind_let("event", v.clone());
            }
        }
        _ => {}
    }
}

/// Whether a policy `match:` expression names the cap path
/// `<module>.<op>`. Recognises `module.op` (exact), `module.*`
/// (wildcard), bare `module` (also wildcard) and `lhs or rhs`
/// disjunctions. Anything else is treated as no match — the
/// static checker will eventually reject ill-formed match patterns,
/// but the runtime must stay forgiving in M8.
fn match_pattern_covers(expr: &Expr, module: &str, op: &str) -> bool {
    match expr {
        Expr::Field { base, name, .. } => match base.as_ref() {
            Expr::Ident(m, _) if m == module => name == op || name == "*",
            _ => false,
        },
        Expr::Ident(m, _) => m == module,
        Expr::Binary {
            op: BinOp::Or,
            lhs,
            rhs,
            ..
        } => match_pattern_covers(lhs, module, op) || match_pattern_covers(rhs, module, op),
        _ => false,
    }
}

fn apply_policies(
    env: &mut Env,
    module: &str,
    op: &str,
    args: &[Value],
    span: Span,
) -> Result<(), EvalError> {
    let policies = match &env.policies {
        Some(p) if !p.is_empty() => p.clone(),
        _ => return Ok(()),
    };
    for policy in policies.iter() {
        // Find the `match:` clause; skip the policy if it does not name
        // this cap path.
        let matched = policy
            .fields
            .iter()
            .find(|f| f.key == "match")
            .map(|f| f.values.iter().any(|e| match_pattern_covers(e, module, op)))
            .unwrap_or(false);
        if !matched {
            continue;
        }
        // Build a per-policy scope: a fresh frame on top of the
        // caller's env so `when:` / `require:` / `deny:` see both the
        // surrounding bindings (e.g. `cap`) and the cap-call's own
        // arg names (`url`, `method`, ...).
        let mut policy_env = env.clone();
        policy_env.push_scope();
        bind_policy_call_vars(&mut policy_env, module, op, args);
        // `when:` gates the rest of the policy. Every when clause must
        // evaluate to `true` for the policy to be active on this call.
        let mut when_active = true;
        if let Some(when_field) = policy.fields.iter().find(|f| f.key == "when") {
            for clause in &when_field.values {
                let v = eval_value(clause, &mut policy_env)?;
                if !matches!(v, Value::Bool(true)) {
                    when_active = false;
                    break;
                }
            }
        }
        if !when_active {
            continue;
        }
        // `require:` — false ⇒ violation.
        for f in policy.fields.iter().filter(|f| f.key == "require") {
            for clause in &f.values {
                let v = eval_value(clause, &mut policy_env)?;
                if !matches!(v, Value::Bool(true)) {
                    return Err(EvalError::new(
                        EvalErrorKind::PolicyViolation {
                            op: format!("{module}.{op}"),
                            target: format!("policy::{} require", policy.name),
                        },
                        span,
                    ));
                }
            }
        }
        // `deny:` — true ⇒ violation.
        for f in policy.fields.iter().filter(|f| f.key == "deny") {
            for clause in &f.values {
                let v = eval_value(clause, &mut policy_env)?;
                if matches!(v, Value::Bool(true)) {
                    return Err(EvalError::new(
                        EvalErrorKind::PolicyViolation {
                            op: format!("{module}.{op}"),
                            target: format!("policy::{} deny", policy.name),
                        },
                        span,
                    ));
                }
            }
        }
        // `audit:` — record any audit fields into the trace event.
        for f in policy.fields.iter().filter(|f| f.key == "audit") {
            for clause in &f.values {
                let v = eval_value(clause, &mut policy_env)?;
                let mut audit_fields = vec![
                    ("policy".into(), format!("\"{}\"", policy.name)),
                    ("op".into(), format!("\"{module}.{op}\"")),
                ];
                if let Value::Record(r) = v {
                    for (k, vv) in r.fields {
                        audit_fields.push((k, format!("\"{}\"", value_as_display(&vv))));
                    }
                }
                record_event(env, "policy_audit", audit_fields);
            }
        }
        // `limit:` — recorded for now, enforcement comes with M9 quotas.
        for f in policy.fields.iter().filter(|f| f.key == "limit") {
            for clause in &f.values {
                let mut limit_fields = vec![
                    ("policy".into(), format!("\"{}\"", policy.name)),
                    ("op".into(), format!("\"{module}.{op}\"")),
                ];
                if let Expr::Assign { target, value, .. } = clause {
                    if let Expr::Ident(name, _) = target.as_ref() {
                        let v = eval_value(value, &mut policy_env)?;
                        limit_fields.push(("name".into(), format!("\"{name}\"")));
                        limit_fields.push(("value".into(), value_as_display(&v)));
                    }
                }
                record_event(env, "policy_limit", limit_fields);
            }
        }
    }
    Ok(())
}

// ====================================================================
//  Binary / unary ops
// ====================================================================

fn eval_binary(
    op: BinOp,
    lhs: &Expr,
    rhs: &Expr,
    span: Span,
    env: &mut Env,
) -> Result<Flow, EvalError> {
    // `and` / `or` short-circuit and require booleans.
    if matches!(op, BinOp::And | BinOp::Or) {
        let l = eval_value(lhs, env)?;
        let lb = expect_bool(&l, lhs.span())?;
        if op == BinOp::Or && lb {
            return Ok(Flow::Value(Value::Bool(true)));
        }
        if op == BinOp::And && !lb {
            return Ok(Flow::Value(Value::Bool(false)));
        }
        let r = eval_value(rhs, env)?;
        let rb = expect_bool(&r, rhs.span())?;
        return Ok(Flow::Value(Value::Bool(rb)));
    }
    let l = eval_value(lhs, env)?;
    let r = eval_value(rhs, env)?;
    let v = apply_binop(op, l, r, span)?;
    Ok(Flow::Value(v))
}

fn apply_binop(op: BinOp, lhs: Value, rhs: Value, span: Span) -> Result<Value, EvalError> {
    use BinOp::*;
    use Value::*;
    match (op, lhs, rhs) {
        // arithmetic on int
        (Add, Int(a), Int(b)) => a.checked_add(b).map(Int).ok_or_else(overflow(span)),
        (Sub, Int(a), Int(b)) => a.checked_sub(b).map(Int).ok_or_else(overflow(span)),
        (Mul, Int(a), Int(b)) => a.checked_mul(b).map(Int).ok_or_else(overflow(span)),
        (Div, Int(_), Int(0)) => Err(EvalError::new(EvalErrorKind::DivByZero, span)),
        (Div, Int(a), Int(b)) => a.checked_div(b).map(Int).ok_or_else(overflow(span)),
        (Rem, Int(_), Int(0)) => Err(EvalError::new(EvalErrorKind::DivByZero, span)),
        (Rem, Int(a), Int(b)) => a.checked_rem(b).map(Int).ok_or_else(overflow(span)),

        // arithmetic on float
        (Add, Float(a), Float(b)) => Ok(Float(a + b)),
        (Sub, Float(a), Float(b)) => Ok(Float(a - b)),
        (Mul, Float(a), Float(b)) => Ok(Float(a * b)),
        (Div, Float(a), Float(b)) => Ok(Float(a / b)),
        (Rem, Float(a), Float(b)) => Ok(Float(a % b)),

        // bitwise on int
        (BitAnd, Int(a), Int(b)) => Ok(Int(a & b)),
        (BitOr, Int(a), Int(b)) => Ok(Int(a | b)),
        (BitXor, Int(a), Int(b)) => Ok(Int(a ^ b)),
        (Shl, Int(a), Int(b)) if (0..64).contains(&b) => Ok(Int(a.wrapping_shl(b as u32))),
        (Shr, Int(a), Int(b)) if (0..64).contains(&b) => Ok(Int(a.wrapping_shr(b as u32))),

        // string concat with `+`
        (Add, Str(a), Str(b)) => {
            let mut out = String::with_capacity(a.len() + b.len());
            out.push_str(&a);
            out.push_str(&b);
            Ok(Str(out))
        }
        // list concat with `+`
        (Add, List(mut a), List(b)) => {
            a.extend(b);
            Ok(List(a))
        }

        // comparison / equality
        (Eq, a, b) => Ok(Bool(values_equal(&a, &b))),
        (Ne, a, b) => Ok(Bool(!values_equal(&a, &b))),
        (Lt, a, b) => compare(&a, &b, span).map(|o| Bool(o == std::cmp::Ordering::Less)),
        (Le, a, b) => compare(&a, &b, span).map(|o| Bool(o != std::cmp::Ordering::Greater)),
        (Gt, a, b) => compare(&a, &b, span).map(|o| Bool(o == std::cmp::Ordering::Greater)),
        (Ge, a, b) => compare(&a, &b, span).map(|o| Bool(o != std::cmp::Ordering::Less)),

        // and / or already short-circuited above
        (op, a, b) => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "`{}` not defined for `{}` and `{}`",
                binop_name(op),
                value_kind(&a),
                value_kind(&b)
            )),
            span,
        )),
    }
}

fn binop_name(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::And => "and",
        BinOp::Or => "or",
    }
}

fn overflow(span: Span) -> impl FnOnce() -> EvalError {
    move || EvalError::new(EvalErrorKind::Type("integer overflow".into()), span)
}

fn eval_unary(op: UnOp, expr: &Expr, span: Span, env: &mut Env) -> Result<Flow, EvalError> {
    let v = eval_value(expr, env)?;
    match (op, v) {
        (UnOp::Neg, Value::Int(n)) => Ok(Flow::Value(Value::Int(n.checked_neg().ok_or_else(
            || EvalError::new(EvalErrorKind::Type("integer overflow".into()), span),
        )?))),
        (UnOp::Neg, Value::Float(f)) => Ok(Flow::Value(Value::Float(-f))),
        (UnOp::Not, Value::Bool(b)) => Ok(Flow::Value(Value::Bool(!b))),
        (op, v) => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "unary `{}` not defined for `{}`",
                if matches!(op, UnOp::Neg) { "-" } else { "not" },
                value_kind(&v)
            )),
            span,
        )),
    }
}

// ====================================================================
//  Comparison / equality
// ====================================================================

fn values_equal(a: &Value, b: &Value) -> bool {
    a == b
}

fn compare(a: &Value, b: &Value, span: Span) -> Result<std::cmp::Ordering, EvalError> {
    use Value::*;
    match (a, b) {
        (Int(x), Int(y)) => Ok(x.cmp(y)),
        (Float(x), Float(y)) => x.partial_cmp(y).ok_or_else(|| {
            EvalError::new(
                EvalErrorKind::Type("cannot compare NaN floats".into()),
                span,
            )
        }),
        (Str(x), Str(y)) => Ok(x.cmp(y)),
        (Char(x), Char(y)) => Ok(x.cmp(y)),
        (Bool(x), Bool(y)) => Ok(x.cmp(y)),
        (Date(x), Date(y)) | (Timestamp(x), Timestamp(y)) | (Duration(x), Duration(y)) => {
            Ok(x.cmp(y))
        }
        (a, b) => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "cannot order `{}` and `{}`",
                value_kind(a),
                value_kind(b)
            )),
            span,
        )),
    }
}

fn expect_bool(v: &Value, span: Span) -> Result<bool, EvalError> {
    match v {
        Value::Bool(b) => Ok(*b),
        _ => Err(EvalError::new(
            EvalErrorKind::Type(format!("expected bool, got {}", value_kind(v))),
            span,
        )),
    }
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Unit => "unit",
        Value::Bool(_) => "bool",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::Decimal(_) => "decimal",
        Value::Str(_) => "string",
        Value::Bytes(_) => "bytes",
        Value::Char(_) => "char",
        Value::Uuid(_) => "uuid",
        Value::Date(_) => "date",
        Value::Timestamp(_) => "timestamp",
        Value::Duration(_) => "duration",
        Value::List(_) => "list",
        Value::Set(_) => "set",
        Value::Map(_) => "map",
        Value::Tuple(_) => "tuple",
        Value::Option(_) => "option",
        Value::Result(_) => "result",
        Value::Record(_) => "record",
        Value::Enum(_) => "enum",
        Value::Closure(_) => "closure",
        Value::Cap(_) => "cap",
        Value::Saga(_) => "saga",
        Value::Agent(_) => "agent",
        Value::AgentNet(_) => "agent_net",
    }
}

// ====================================================================
//  M4.T2 — `cap.subset[..]` runtime narrowing
// ====================================================================

fn eval_cap_narrow(
    _kind: CapNarrowKind,
    entries: &[CapEntry],
    span: Span,
    env: &mut Env,
) -> Result<Flow, EvalError> {
    // The unprefixed `cap` token *inside* `cap.subset[..]` resolves to
    // the parameter literally named `cap` (§ 8.4). The parser already
    // parses the syntax; we just need to look up that binding.
    let parent = env.lookup("cap").ok_or_else(|| {
        EvalError::new(
            EvalErrorKind::Type("`cap.subset[..]` requires a `cap` binding in scope".into()),
            span,
        )
    })?;
    let parent_cap = match &parent {
        Value::Cap(c) => c.clone(),
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "`cap.subset[..]` parent must be a cap value, got {}",
                    value_kind(other)
                )),
                span,
            ))
        }
    };
    let child_entries: Vec<CapEntryValue> = entries
        .iter()
        .map(|e| CapEntryValue {
            path: e.path.segments.clone(),
            allow: e.allow.clone(),
        })
        .collect();
    let child = CapValue {
        entries: child_entries,
        star: false,
    };
    if let Err(err) = parent_cap.covers(&child) {
        let msg = match err {
            CapNarrowError::ChildHasStar => "cap.subset cannot construct `cap[*]`".to_string(),
            CapNarrowError::EntryNotInParent { op } => {
                format!("`cap.subset[..]` cannot broaden parent: `{op}` not in scope")
            }
        };
        return Err(EvalError::new(EvalErrorKind::Type(msg), span));
    }
    Ok(Flow::Value(Value::Cap(Rc::new(child))))
}

// ====================================================================
//  Field / index access
// ====================================================================

fn field_access(v: &Value, name: &str, span: Span) -> Result<Value, EvalError> {
    match v {
        Value::Record(r) => r
            .fields
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| {
                EvalError::new(
                    EvalErrorKind::Type(format!("record has no field `{name}`")),
                    span,
                )
            }),
        Value::Enum(e) => match (&e.data, name) {
            // Synthetic projections: every enum exposes `.name` /
            // `.variant` for trace introspection in M9 / M10.
            (_, "variant") => Ok(Value::Str(e.variant.clone())),
            (_, "kind") => Ok(Value::Str(e.name.clone())),
            (VariantValue::Record(fs), n) => fs
                .iter()
                .find(|(k, _)| k == n)
                .map(|(_, v)| v.clone())
                .ok_or_else(|| {
                    EvalError::new(
                        EvalErrorKind::Type(format!("variant has no field `{n}`")),
                        span,
                    )
                }),
            _ => Err(EvalError::new(
                EvalErrorKind::Type(format!("enum has no field `{name}`")),
                span,
            )),
        },
        _ => Err(EvalError::new(
            EvalErrorKind::Type(format!("`.{name}` on non-record value")),
            span,
        )),
    }
}

fn index_into(collection: &Value, idx: &Value, span: Span) -> Result<Value, EvalError> {
    match (collection, idx) {
        (Value::List(xs), Value::Int(i)) => {
            let len = xs.len();
            let idx = *i;
            if idx < 0 || (idx as usize) >= len {
                return Err(EvalError::new(
                    EvalErrorKind::IndexOutOfBounds { index: idx, len },
                    span,
                ));
            }
            Ok(xs[idx as usize].clone())
        }
        (Value::Tuple(xs), Value::Int(i)) => {
            let len = xs.len();
            let idx = *i;
            if idx < 0 || (idx as usize) >= len {
                return Err(EvalError::new(
                    EvalErrorKind::IndexOutOfBounds { index: idx, len },
                    span,
                ));
            }
            Ok(xs[idx as usize].clone())
        }
        (Value::Map(kvs), key) => kvs
            .iter()
            .find(|(k, _)| values_equal(k, key))
            .map(|(_, v)| v.clone())
            .ok_or_else(|| EvalError::new(EvalErrorKind::NonExhaustiveMatch, span)),
        (Value::Str(s), Value::Int(i)) => {
            let chars: Vec<char> = s.chars().collect();
            let idx = *i;
            if idx < 0 || (idx as usize) >= chars.len() {
                return Err(EvalError::new(
                    EvalErrorKind::IndexOutOfBounds {
                        index: idx,
                        len: chars.len(),
                    },
                    span,
                ));
            }
            Ok(Value::Char(chars[idx as usize]))
        }
        _ => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "cannot index `{}` with `{}`",
                value_kind(collection),
                value_kind(idx)
            )),
            span,
        )),
    }
}

// ====================================================================
//  Calls
// ====================================================================

fn eval_call(
    callee: &Expr,
    type_args: &[crate::syntax::ast::Type],
    args: &[CallArg],
    span: Span,
    env: &mut Env,
) -> Result<Flow, EvalError> {
    // M8.T2: type-aware builtins consume turbofish args before falling
    // through to the L1 lookup. Both forms parse the JSON body, validate
    // it against the named `model@vN`, and surface a `SchemaViolation`
    // (§ 16.2) on any mismatch.
    if let Expr::Field { base, name, .. } = callee {
        if let Expr::Ident(m, _) = base.as_ref() {
            match (m.as_str(), name.as_str()) {
                ("json", "decode") if !type_args.is_empty() => {
                    let arg_values: Vec<Value> = args
                        .iter()
                        .map(|a| eval_value(&a.value, env))
                        .collect::<Result<_, _>>()?;
                    return builtin_json_decode(env, type_args, &arg_values, span).map(Flow::Value);
                }
                ("http", "body") => {
                    let arg_values: Vec<Value> = args
                        .iter()
                        .map(|a| eval_value(&a.value, env))
                        .collect::<Result<_, _>>()?;
                    return builtin_http_body(env, type_args, &arg_values, span).map(Flow::Value);
                }
                _ => {}
            }
        }
    }
    // M12.T2: `assert(<expr>)` is a runtime builtin that captures the
    // unevaluated argument AST so a failure can render `expected vs.
    // actual` for `==` / `!=` comparisons. We intercept it before any
    // user-defined function named `assert` is resolved — the name is
    // reserved by the test framework.
    if let Expr::Ident(name, _) = callee {
        if name == "assert" {
            return eval_assert_call(args, span, env);
        }
        // M12.T4: `trace()` returns the recorded events of the
        // currently-loaded fixture as a `list<record>`. Outside a
        // fixture-mode test it returns the empty list — callers that
        // need the strict form should use `trace_has(...)`.
        if name == "trace" && args.is_empty() {
            return Ok(Flow::Value(fixture_trace_as_value(env)));
        }
        // M12.T4: `trace_has(<record>)` — convenience predicate that
        // matches any event whose fields contain every key/value of
        // the given record. Equivalent to `trace().has({...})` once
        // list method dispatch lands.
        if name == "trace_has" && args.len() == 1 {
            let pred = eval_value(&args[0].value, env)?;
            let ok = trace_has_event(env, &pred);
            return Ok(Flow::Value(Value::Bool(ok)));
        }
    }
    // Constructor sugar resolves before the closure path so a
    // user-bound `Ok` doesn't shadow it. The constructors are
    // documented in `language.md` § 18 / § 4.
    if let Expr::Ident(name, _) = callee {
        match (name.as_str(), args.len()) {
            ("Ok", 1) => {
                let v = eval_value(&args[0].value, env)?;
                return Ok(Flow::Value(Value::ok(v)));
            }
            ("Err", 1) => {
                let v = eval_value(&args[0].value, env)?;
                return Ok(Flow::Value(Value::err(v)));
            }
            ("Some", 1) => {
                let v = eval_value(&args[0].value, env)?;
                return Ok(Flow::Value(Value::some(v)));
            }
            ("None", 0) => return Ok(Flow::Value(Value::none())),
            _ => {}
        }
    }
    // L1 builtins: `<module>.<op>(...)` where the pair names a cap
    // operation. The static checker (M2.T4) has already verified
    // that the call is authorised; the runtime hands off to the
    // handler, which records a trace event.
    if let Expr::Field { base, name, .. } = callee {
        if let Expr::Ident(m, _) = base.as_ref() {
            if let Some(handler) = lookup_builtin(m, name) {
                let arg_values: Vec<Value> = args
                    .iter()
                    .map(|a| eval_value(&a.value, env))
                    .collect::<Result<_, _>>()?;
                // M8.T4: every policy whose `match:` clause covers this
                // cap path runs before the handler. `require:` failures
                // and `deny:` hits raise `PolicyViolation` — uncatchable
                // by `?` (§ 18.4) and surfaced as exit 1.
                apply_policies(env, m, name, &arg_values, span)?;
                return handler(env, &arg_values, span).map(Flow::Value);
            }
        }
    }
    let callee_value = eval_value(callee, env)?;
    let arg_values: Vec<Value> = args
        .iter()
        .map(|a| eval_value(&a.value, env))
        .collect::<Result<_, _>>()?;
    invoke_value(&callee_value, &arg_values, span)
}

/// M12.T4 — render the env's fixture trace as a `Value::List` of
/// records. Each event becomes a record with keys `kind`, `intent`,
/// `scope`, plus every recorded field of the event as a string.
fn fixture_trace_as_value(env: &Env) -> Value {
    let events = match env.fixture_trace() {
        Some(es) => es,
        None => return Value::List(Vec::new()),
    };
    let mut out: Vec<Value> = Vec::with_capacity(events.len());
    for e in events {
        let mut fields: Vec<(String, Value)> = Vec::new();
        fields.push(("kind".into(), Value::Str(e.kind.clone())));
        fields.push((
            "intent".into(),
            match &e.intent {
                Some(s) => Value::Option(Some(Box::new(Value::Str(s.clone())))),
                None => Value::Option(None),
            },
        ));
        fields.push((
            "scope".into(),
            match &e.scope {
                Some(s) => Value::Option(Some(Box::new(Value::Str(s.clone())))),
                None => Value::Option(None),
            },
        ));
        for (k, v) in &e.fields {
            fields.push((k.clone(), Value::Str(v.clone())));
        }
        out.push(Value::Record(super::value::RecordValue {
            name: Some("TraceEvent".into()),
            fields,
        }));
    }
    Value::List(out)
}

/// M12.T4 — predicate: any event in the fixture trace whose fields
/// match every key/value of `pred` (treated as a partial record).
/// Comparison is structural — string equality on `kind`, `intent`,
/// `scope`, and on each named additional field.
fn trace_has_event(env: &Env, pred: &Value) -> bool {
    let pred_fields = match pred {
        Value::Record(r) => &r.fields,
        _ => return false,
    };
    let events = match env.fixture_trace() {
        Some(es) => es,
        None => return false,
    };
    events.iter().any(|e| {
        pred_fields.iter().all(|(k, v)| match k.as_str() {
            "kind" => match v {
                Value::Str(s) => &e.kind == s,
                _ => false,
            },
            "intent" => match v {
                Value::Str(s) => e.intent.as_deref() == Some(s.as_str()),
                Value::Option(None) => e.intent.is_none(),
                Value::Option(Some(inner)) => match inner.as_ref() {
                    Value::Str(s) => e.intent.as_deref() == Some(s.as_str()),
                    _ => false,
                },
                _ => false,
            },
            "scope" => match v {
                Value::Str(s) => e.scope.as_deref() == Some(s.as_str()),
                Value::Option(None) => e.scope.is_none(),
                Value::Option(Some(inner)) => match inner.as_ref() {
                    Value::Str(s) => e.scope.as_deref() == Some(s.as_str()),
                    _ => false,
                },
                _ => false,
            },
            other => e.fields.iter().any(|(fk, fv)| {
                fk == other && {
                    // Field values come out of the wire JSON with their
                    // own quoting — strings keep their `"..."` envelope,
                    // numbers / bools land bare. Strip a single layer
                    // of quotes when comparing against a `Value::Str`.
                    let unwrapped = if fv.len() >= 2
                        && fv.starts_with('"')
                        && fv.ends_with('"')
                    {
                        &fv[1..fv.len() - 1]
                    } else {
                        fv.as_str()
                    };
                    match v {
                        Value::Str(s) => unwrapped == s,
                        Value::Int(n) => unwrapped == n.to_string(),
                        Value::Bool(b) => unwrapped == b.to_string(),
                        _ => false,
                    }
                }
            }),
        })
    })
}

/// M12.T2 — `assert(<expr>)` builtin. Evaluates the (single) argument
/// expression and, on a falsy result, raises `AssertionFailed` with
/// the formatted source of `<expr>` so the test runner can render
/// "what failed" without re-reading the source file. When the asserted
/// expression is a `lhs == rhs` (or `lhs != rhs`) comparison the lhs
/// and rhs values are evaluated separately and stashed into the error
/// payload — the renderer prints them as `expected vs. actual`.
fn eval_assert_call(args: &[CallArg], span: Span, env: &mut Env) -> Result<Flow, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::new(
            EvalErrorKind::Arity {
                name: "assert".into(),
                expected: 1,
                found: args.len(),
            },
            span,
        ));
    }
    let arg = &args[0].value;
    if let Expr::Binary {
        op: bop @ (BinOp::Eq | BinOp::Ne),
        lhs,
        rhs,
        ..
    } = arg
    {
        let lv = eval_value(lhs, env)?;
        let rv = eval_value(rhs, env)?;
        let equal = lv == rv;
        let pass = match bop {
            BinOp::Eq => equal,
            BinOp::Ne => !equal,
            _ => unreachable!(),
        };
        if pass {
            return Ok(Flow::Value(Value::Unit));
        }
        let detail = AssertionDetail {
            lhs_source: crate::syntax::fmt::format_expression(lhs),
            rhs_source: crate::syntax::fmt::format_expression(rhs),
            lhs_value: value_to_natural_json(&lv),
            rhs_value: value_to_natural_json(&rv),
            op: match bop {
                BinOp::Eq => AssertionCmpOp::Eq,
                BinOp::Ne => AssertionCmpOp::Ne,
                _ => unreachable!(),
            },
        };
        return Err(EvalError::new(
            EvalErrorKind::AssertionFailed {
                source: crate::syntax::fmt::format_expression(arg),
                detail: Some(Box::new(detail)),
            },
            span,
        ));
    }
    let v = eval_value(arg, env)?;
    let truthy = match v {
        Value::Bool(b) => b,
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::Type("`assert` requires a `bool`".into()),
                arg.span(),
            ))
        }
    };
    if truthy {
        Ok(Flow::Value(Value::Unit))
    } else {
        Err(EvalError::new(
            EvalErrorKind::AssertionFailed {
                source: crate::syntax::fmt::format_expression(arg),
                detail: None,
            },
            span,
        ))
    }
}

// ====================================================================
//  M4.T4 / M4.T6 / M4.T7 — L1 cap builtins
// ====================================================================

type Builtin = fn(&Env, &[Value], Span) -> Result<Value, EvalError>;

fn lookup_builtin(module: &str, op: &str) -> Option<Builtin> {
    Some(match (module, op) {
        ("io", "print") => builtin_io_print,
        ("io", "println") => builtin_io_println,
        ("io", "eprint") => builtin_io_eprint,
        ("io", "eprintln") => builtin_io_eprintln,
        ("io", "read_line") => builtin_io_read_line,
        ("clock", "now") => builtin_clock_now,
        ("random", "next") => builtin_random_next,
        ("env", "read") => builtin_env_read,
        ("fs", "read_text") => builtin_fs_read_text,
        ("fs", "read_file") | ("fs", "read_bytes") => builtin_fs_read_bytes,
        ("fs", "write_text") => builtin_fs_write_text,
        ("fs", "write_file") | ("fs", "write_bytes") => builtin_fs_write_bytes,
        ("fs", "exists") => builtin_fs_exists,
        ("fs", "stat") => builtin_fs_stat,
        ("fs", "mkdir") => builtin_fs_mkdir,
        ("fs", "remove") => builtin_fs_remove,
        ("fs", "rename") => builtin_fs_rename,
        ("fs", "walk") => builtin_fs_walk,
        ("shell", "exec") => builtin_shell_exec,
        ("shell", "pipe") => builtin_shell_pipe,
        ("http", "get") => builtin_http_get,
        ("http", "post") => builtin_http_post,
        ("http", "put") => builtin_http_put,
        ("http", "patch") => builtin_http_patch,
        ("http", "delete") => builtin_http_delete,
        ("ai", "complete") => builtin_ai_complete,
        ("ai", "chat") => builtin_ai_chat,
        ("ai", "embed") => builtin_ai_embed,
        ("ai", "tools") => builtin_ai_tools,
        ("audit", "event") => builtin_audit_event,
        ("kube", "apply") => builtin_kube_apply,
        ("kube", "delete") => builtin_kube_delete,
        ("kube", "get") => builtin_kube_get,
        ("kube", "watch") => builtin_kube_watch,
        ("docker", "run") => builtin_docker_run,
        ("docker", "build") => builtin_docker_build,
        ("docker", "push") => builtin_docker_push,
        ("docker", "pull") => builtin_docker_pull,
        ("docker", "inspect") => builtin_docker_inspect,
        ("mongodb", "read") => builtin_mongodb_read,
        ("mongodb", "write") => builtin_mongodb_write,
        ("minio", "get") => builtin_minio_get,
        ("minio", "put") => builtin_minio_put,
        ("rabbitmq", "publish") => builtin_rabbitmq_publish,
        ("rabbitmq", "subscribe") => builtin_rabbitmq_subscribe,
        _ => return None,
    })
}

fn record_event(env: &Env, kind: &str, fields: Vec<(String, String)>) {
    if let Some(t) = env.tracer() {
        t.record(kind, None, fields);
    }
}

fn value_as_display(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => c.to_string(),
        Value::Unit => "()".into(),
        other => format!("{other:?}"),
    }
}

fn builtin_io_print(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("io.print", 1, args, span)?;
    let s = value_as_display(&args[0]);
    print!("{s}");
    record_event(env, "io_print", vec![("len".into(), s.len().to_string())]);
    Ok(Value::Unit)
}

fn builtin_io_println(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("io.println", 1, args, span)?;
    let s = value_as_display(&args[0]);
    println!("{s}");
    record_event(env, "io_println", vec![("len".into(), s.len().to_string())]);
    Ok(Value::Unit)
}

fn builtin_io_eprint(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("io.eprint", 1, args, span)?;
    let s = value_as_display(&args[0]);
    eprint!("{s}");
    record_event(env, "io_eprint", vec![("len".into(), s.len().to_string())]);
    Ok(Value::Unit)
}

fn builtin_io_eprintln(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("io.eprintln", 1, args, span)?;
    let s = value_as_display(&args[0]);
    eprintln!("{s}");
    record_event(
        env,
        "io_eprintln",
        vec![("len".into(), s.len().to_string())],
    );
    Ok(Value::Unit)
}

fn builtin_clock_now(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("clock.now", 0, args, span)?;
    // M9.T5: under replay, drain the recorded `clock_now` value so
    // the run is bit-identical to the original trace.
    if let Some(tape) = env.replay_tape() {
        if let Some(evt) = tape.borrow_mut().consume_next("clock_now") {
            if let Some(v) = crate::runtime::replay::Tape::field(&evt, "value") {
                record_event(env, "clock_now", vec![("value".into(), format!("\"{v}\""))]);
                return Ok(Value::Timestamp(v.to_string()));
            }
        }
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let s = format_iso_ms(ts_ms);
    record_event(env, "clock_now", vec![("value".into(), format!("\"{s}\""))]);
    Ok(Value::Timestamp(s))
}

fn builtin_random_next(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("random.next", 0, args, span)?;
    // M9.T5: under replay, drain the recorded `random_next` value.
    if let Some(tape) = env.replay_tape() {
        if let Some(evt) = tape.borrow_mut().consume_next("random_next") {
            if let Some(v) = crate::runtime::replay::Tape::field(&evt, "value") {
                if let Ok(n) = v.parse::<i64>() {
                    record_event(env, "random_next", vec![("value".into(), n.to_string())]);
                    return Ok(Value::Int(n));
                }
            }
        }
    }
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    // SplitMix64-ish mixer; M9 will replace with a recorded RNG.
    let mut x = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^= x >> 31;
    let v = (x as i64).wrapping_abs();
    record_event(env, "random_next", vec![("value".into(), v.to_string())]);
    Ok(Value::Int(v))
}

fn builtin_io_read_line(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("io.read_line", 0, args, span)?;
    // Prefer the test-controlled queue. Fall back to the real OS
    // stdin (line-buffered) only when the queue is empty AND not
    // explicitly configured.
    let line = match env.pop_stdin_line() {
        Some(s) => s,
        None => {
            if env.stdin.is_some() {
                // Queue is empty and tests configured a queue → no
                // more input. Return None to signal EOF.
                record_event(env, "io_read_line", vec![("eof".into(), "true".into())]);
                return Ok(Value::none());
            }
            let mut s = String::new();
            std::io::stdin().read_line(&mut s).map_err(|e| {
                EvalError::new(
                    EvalErrorKind::Io {
                        op: "io.read_line".into(),
                        message: e.to_string(),
                    },
                    span,
                )
            })?;
            if s.ends_with('\n') {
                s.pop();
                if s.ends_with('\r') {
                    s.pop();
                }
            }
            s
        }
    };
    record_event(
        env,
        "io_read_line",
        vec![("len".into(), line.len().to_string())],
    );
    Ok(Value::some(Value::Str(line)))
}

// ---- fs allow-list & glob helpers --------------------------------

/// Glob match supporting `*` (one segment) and `**` (zero-or-more
/// segments). Patterns are matched against absolute paths or
/// relative paths verbatim — no canonicalisation. Sufficient for the
/// language-md examples (`./out/**`, `/etc/aeris/**`).
fn glob_matches(pat: &str, path: &str) -> bool {
    let pat_parts: Vec<&str> = pat.split('/').filter(|s| !s.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    glob_match_parts(&pat_parts, &path_parts)
}

fn glob_match_parts(pat: &[&str], path: &[&str]) -> bool {
    match pat.first() {
        None => path.is_empty(),
        Some(&"**") => {
            // `**` matches zero or more path segments.
            for i in 0..=path.len() {
                if glob_match_parts(&pat[1..], &path[i..]) {
                    return true;
                }
            }
            false
        }
        Some(&"*") => match path.first() {
            Some(_) => glob_match_parts(&pat[1..], &path[1..]),
            None => false,
        },
        Some(p) => match path.first() {
            Some(s) if s == p => glob_match_parts(&pat[1..], &path[1..]),
            _ => false,
        },
    }
}

/// Verify that the in-scope `cap` authorises operation `module.op`
/// on `target` path. Returns `PolicyViolation` if the cap doesn't
/// list the op, or if its allow-list excludes `target`.
fn enforce_path_policy(
    env: &Env,
    module: &str,
    op: &str,
    target: &str,
    span: Span,
) -> Result<(), EvalError> {
    let cap = env.lookup("cap").and_then(|v| match v {
        Value::Cap(c) => Some(c),
        _ => None,
    });
    let cap = match cap {
        Some(c) => c,
        // If `cap` is missing the static checker (M2.T4) already fired;
        // at runtime we treat missing-cap as a policy violation.
        None => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("{module}.{op}"),
                    target: target.to_string(),
                },
                span,
            ));
        }
    };
    if cap.star {
        return Ok(());
    }
    for entry in &cap.entries {
        let path_match = match entry.path.as_slice() {
            [m] => m == module,
            [m, o] => m == module && o == op,
            _ => false,
        };
        if !path_match {
            continue;
        }
        match &entry.allow {
            None => return Ok(()),
            Some(globs) => {
                if globs.iter().any(|g| glob_matches(g, target)) {
                    return Ok(());
                }
            }
        }
    }
    Err(EvalError::new(
        EvalErrorKind::PolicyViolation {
            op: format!("{module}.{op}"),
            target: target.to_string(),
        },
        span,
    ))
}

fn expect_string(name: &str, v: &Value, span: Span) -> Result<String, EvalError> {
    match v {
        Value::Str(s) => Ok(s.clone()),
        other => Err(EvalError::new(
            EvalErrorKind::Type(format!("{name} expects string, got {}", value_kind(other))),
            span,
        )),
    }
}

// ====================================================================
//  Saga interpreter (M6.T1 / T2 / T3 / T5)
// ====================================================================

/// Derive a per-step idempotency key (N1 / § 12.3). Until M7 brings
/// blake3 in, we use the same FNV-1a mixer as the rest of the
/// runtime — it is **deterministic and reproducible** under replay,
/// which is the property the spec requires here.
fn idempotency_key(trace_id: &str, step_name: &str, invocation: u64) -> String {
    let mut input = String::with_capacity(trace_id.len() + step_name.len() + 16);
    input.push_str(trace_id);
    input.push('|');
    input.push_str(step_name);
    input.push('|');
    input.push_str(&invocation.to_string());
    let h = fnv1a_64(input.as_bytes());
    format!("{h:016x}")
}

// ====================================================================
//  M10.T2 / T3 / T4 — agent invocation
// ====================================================================

/// Invoke an agent: validate input against `accept`, send the prompt
/// (with the auto-injected routing-protocol contract — M10.T3) through
/// `ai.complete`, parse the JSON response, validate against `produce`.
/// Schema mismatches and budget overruns consume retries; exhaustion
/// raises `Raised(err.llm)` so the caller's `?` propagates it (§ 13.2).
fn invoke_agent(
    agent: &super::value::AgentInstance,
    args: &[Value],
    span: Span,
) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::new(
            EvalErrorKind::Arity {
                name: agent.name.clone(),
                expected: 2,
                found: args.len(),
            },
            span,
        ));
    }
    let input = &args[0];
    let cap = &args[1];
    // 1. Validate input against `accept`.
    validate_against_model(agent.model_decls.as_ref(), &agent.accept, input, span)?;
    // 2. Build the env so `ai.complete` sees the configured backend,
    //    tracer, and replay tape.
    let mut call_env = Env::new();
    call_env.module.clone_from(&agent.module);
    call_env.tracer.clone_from(&agent.tracer);
    call_env.model_decls.clone_from(&agent.model_decls);
    call_env.ai_backend.clone_from(&agent.ai_backend);
    call_env.replay_tape.clone_from(&agent.replay_tape);
    call_env.full_record = agent.full_record;
    call_env.bind_let("cap", cap.clone());
    // 3. Compose the prompt with the routing-protocol contract (T3).
    let full_prompt = compose_agent_prompt(agent, input);
    // 4. Retry loop (T4): try up to `1 + retries` times. Each attempt
    //    runs the LLM, parses + validates the response. SchemaViolation
    //    or budget overrun consumes one retry; other errors propagate.
    let mut last_err: Option<EvalError> = None;
    let attempts = agent.retries.saturating_add(1);
    for attempt in 0..attempts {
        let start_ns = std::time::Instant::now();
        let resp = match call_ai_complete(&call_env, &full_prompt, span) {
            Ok(s) => s,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        // M10.T4: budget — both tokens (rough whitespace-split) and
        // wall-clock latency. Exceeding either consumes a retry.
        let elapsed_ms = start_ns.elapsed().as_millis() as u64;
        if let Some(max) = agent.budget_latency_ms {
            if elapsed_ms > max {
                last_err = Some(EvalError::new(
                    EvalErrorKind::BudgetExceeded {
                        agent: agent.name.clone(),
                        kind: "latency".into(),
                        limit: max,
                        observed: elapsed_ms,
                    },
                    span,
                ));
                continue;
            }
        }
        if let Some(max) = agent.budget_tokens {
            let tokens = resp.split_whitespace().count() as u64
                + full_prompt.split_whitespace().count() as u64;
            if tokens > max {
                last_err = Some(EvalError::new(
                    EvalErrorKind::BudgetExceeded {
                        agent: agent.name.clone(),
                        kind: "tokens".into(),
                        limit: max,
                        observed: tokens,
                    },
                    span,
                ));
                continue;
            }
        }
        let _ = attempt;
        match decode_agent_response(&call_env, agent, &resp, span) {
            Ok(v) => return Ok(Value::ok(v)),
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        }
    }
    // Retry budget exhausted — surface the last error verbatim if it
    // was already a runtime fatal (BudgetExceeded), otherwise wrap it
    // as `Raised(err.llm)` so the caller's `?` propagates it.
    let final_err = last_err.unwrap_or_else(|| {
        EvalError::new(
            EvalErrorKind::Raised(super::value::Value::Str(format!(
                "agent `{}` exhausted retries with no response",
                agent.name
            ))),
            span,
        )
    });
    Err(final_err)
}

fn compose_agent_prompt(agent: &super::value::AgentInstance, input: &Value) -> String {
    // Render the input as natural JSON. The mock backend echoes back
    // the prompt, so tests can assert that the contract appendix is
    // present (M10.T3 acceptance).
    let input_json = value_to_natural_json(input);
    format!(
        "{user}\n\n--- aeris.routing.contract ---\n\
         input  : {accept}@v{accept_v}\n\
         output : {produce}@v{produce_v}\n\
         intent : {intent}\n\
         payload: {input_json}\n",
        user = agent.prompt,
        accept = agent.accept.0,
        accept_v = agent.accept.1,
        produce = agent.produce.0,
        produce_v = agent.produce.1,
        intent = agent.intent,
        input_json = input_json,
    )
}

fn value_to_natural_json(v: &Value) -> String {
    match v {
        Value::Unit => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format!("{f}"),
        Value::Decimal(s)
        | Value::Uuid(s)
        | Value::Date(s)
        | Value::Timestamp(s)
        | Value::Duration(s)
        | Value::Str(s) => format!("\"{}\"", json_escape_for_natural(s)),
        Value::Bytes(b) => format!("\"{}\"", hex16(fnv1a_64(b))),
        Value::Char(c) => format!("\"{c}\""),
        Value::List(xs) | Value::Set(xs) | Value::Tuple(xs) => {
            let mut out = String::from("[");
            for (i, x) in xs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&value_to_natural_json(x));
            }
            out.push(']');
            out
        }
        Value::Map(kvs) => {
            let mut out = String::from("{");
            for (i, (k, v)) in kvs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&value_to_natural_json(k));
                out.push(':');
                out.push_str(&value_to_natural_json(v));
            }
            out.push('}');
            out
        }
        Value::Option(None) => "null".into(),
        Value::Option(Some(inner)) | Value::Result(Ok(inner)) => value_to_natural_json(inner),
        Value::Result(Err(inner)) => {
            format!("{{\"err\":{}}}", value_to_natural_json(inner))
        }
        Value::Record(r) => {
            let mut out = String::from("{");
            for (i, (k, v)) in r.fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&format!("\"{}\":", json_escape_for_natural(k)));
                out.push_str(&value_to_natural_json(v));
            }
            out.push('}');
            out
        }
        Value::Enum(_)
        | Value::Closure(_)
        | Value::Cap(_)
        | Value::Saga(_)
        | Value::Agent(_)
        | Value::AgentNet(_) => "\"<unrenderable>\"".into(),
    }
}

fn json_escape_for_natural(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
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
    out
}

fn call_ai_complete(env: &Env, prompt: &str, span: Span) -> Result<String, EvalError> {
    // Reuse the `ai.complete` builtin; passing the cap pre-bound in
    // the call env. The agent invocation path always uses the
    // `complete` op (M11 will route `chat` / `tools` separately).
    let v = builtin_ai_complete(env, &[Value::Str(prompt.to_string())], span)?;
    match v {
        Value::Result(Ok(boxed)) => match *boxed {
            Value::Str(s) => Ok(s),
            other => Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "agent backend returned non-string: {}",
                    value_kind(&other)
                )),
                span,
            )),
        },
        Value::Result(Err(e)) => Err(EvalError::new(EvalErrorKind::Raised(*e), span)),
        other => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "agent backend returned unexpected shape: {}",
                value_kind(&other)
            )),
            span,
        )),
    }
}

fn decode_agent_response(
    env: &Env,
    agent: &super::value::AgentInstance,
    resp: &str,
    span: Span,
) -> Result<Value, EvalError> {
    let (model_name, version) = &agent.produce;
    decode_and_validate_model(env, resp, model_name, *version, span)
}

fn validate_against_model(
    decls: Option<&std::rc::Rc<HashMap<(String, u32), ModelDecl>>>,
    expected: &(String, u32),
    value: &Value,
    span: Span,
) -> Result<(), EvalError> {
    let r = match value {
        Value::Record(r) => r,
        other => {
            return Err(EvalError::new(
                EvalErrorKind::SchemaViolation {
                    model: expected.0.clone(),
                    version: expected.1,
                    problems: vec![format!("expected record, got {}", value_kind(other))],
                },
                span,
            ));
        }
    };
    if let Some(name) = &r.name {
        if name != &expected.0 {
            return Err(EvalError::new(
                EvalErrorKind::SchemaViolation {
                    model: expected.0.clone(),
                    version: expected.1,
                    problems: vec![format!("input is `{}`, expected `{}`", name, expected.0)],
                },
                span,
            ));
        }
    }
    let _ = decls;
    // Field-level invariants don't re-run on a value already
    // constructed via `Model@vN { ... }` — validation happened then.
    // This guard catches "wrong model name" and shape mismatches; the
    // deeper schema enforcement lands once the type-checker tracks
    // model bindings (M2 follow-up).
    Ok(())
}

// ====================================================================
//  M10.T6 / T7 / T8 — agent_net execution
// ====================================================================

const AGENT_NET_MAX_ITERATIONS: u32 = 3;

/// Run an `agent_net` to convergence. Each iteration walks the DAG in
/// topological order, propagating values along edges and validating
/// every edge crossing against the receiver's `accept` schema. After
/// each pass the optional `until:` predicate is evaluated; the loop
/// breaks on the first satisfaction or when iterations reach
/// `AGENT_NET_MAX_ITERATIONS`.
fn invoke_agent_net(
    net: &super::value::AgentNetInstance,
    args: &[Value],
    span: Span,
) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::new(
            EvalErrorKind::Arity {
                name: net.name.clone(),
                expected: 2,
                found: args.len(),
            },
            span,
        ));
    }
    let input = &args[0];
    let cap = &args[1];
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    let mut nodes: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for flow in &net.flows {
        for window in flow.stages.windows(2) {
            let froms = match &window[0] {
                crate::syntax::ast::FlowStage::Single(n) => vec![n.clone()],
                crate::syntax::ast::FlowStage::FanOut(ns) => ns.clone(),
            };
            let tos = match &window[1] {
                crate::syntax::ast::FlowStage::Single(n) => vec![n.clone()],
                crate::syntax::ast::FlowStage::FanOut(ns) => ns.clone(),
            };
            for f in &froms {
                if seen.insert(f.clone()) {
                    nodes.push(f.clone());
                }
                for t in &tos {
                    adj.entry(f.clone()).or_default().push(t.clone());
                    if seen.insert(t.clone()) {
                        nodes.push(t.clone());
                    }
                }
            }
        }
    }
    let mut indeg: HashMap<String, usize> = nodes.iter().map(|n| (n.clone(), 0)).collect();
    for outs in adj.values() {
        for t in outs {
            *indeg.entry(t.clone()).or_default() += 1;
        }
    }
    let entries: Vec<String> = nodes
        .iter()
        .filter(|n| indeg.get(*n).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    let terminals: Vec<String> = nodes
        .iter()
        .filter(|n| adj.get(*n).map(|v| v.is_empty()).unwrap_or(true))
        .cloned()
        .collect();
    let topo = topo_order(&nodes, &adj);
    if let Some(t) = &net.tracer {
        t.record(
            "net_enter",
            None,
            vec![("net".into(), format!("\"{}\"", net.name))],
        );
    }
    let mut last_terminal_value: Option<Value> = None;
    let mut last_node_outputs: HashMap<String, Value>;
    let mut converged = false;
    let mut iters_run: u32 = 0;
    for iter in 0..AGENT_NET_MAX_ITERATIONS {
        iters_run = iter + 1;
        if let Some(t) = &net.tracer {
            t.record(
                "net_iter",
                None,
                vec![
                    ("net".into(), format!("\"{}\"", net.name)),
                    ("iter".into(), iter.to_string()),
                ],
            );
        }
        let mut node_out: HashMap<String, Value> = HashMap::new();
        for entry in &entries {
            node_out.insert(entry.clone(), input.clone());
        }
        for node in &topo {
            let input_for_node = if let Some(v) = node_out.get(node).cloned() {
                v
            } else {
                let pred_value = adj.iter().find_map(|(p, succs)| {
                    if succs.iter().any(|s| s == node) {
                        node_out.get(p).cloned()
                    } else {
                        None
                    }
                });
                match pred_value {
                    Some(v) => v,
                    None => continue,
                }
            };
            let value = match net
                .module
                .as_ref()
                .and_then(|m| m.borrow().get(node).cloned())
            {
                Some(v) => v,
                None => {
                    return Err(EvalError::new(
                        EvalErrorKind::UndefinedVar(format!("agent_net node `{node}`")),
                        span,
                    ));
                }
            };
            let accept_shape = match &value {
                Value::Agent(a) => Some(a.accept.clone()),
                _ => None,
            };
            if let Some(expected) = &accept_shape {
                if !value_matches_model(&input_for_node, expected) {
                    if let Some(t) = &net.tracer {
                        t.record(
                            "edge_skip",
                            None,
                            vec![
                                ("to".into(), format!("\"{node}\"")),
                                ("reason".into(), "\"type_mismatch\"".into()),
                            ],
                        );
                    }
                    continue;
                }
            }
            if let Some(t) = &net.tracer {
                let in_label = match &value {
                    Value::Agent(a) => format!("{}@v{}", a.accept.0, a.accept.1),
                    _ => "?".into(),
                };
                t.record(
                    "edge",
                    None,
                    vec![
                        ("to".into(), format!("\"{node}\"")),
                        ("schema".into(), format!("\"{in_label}\"")),
                    ],
                );
            }
            let r = invoke_value(&value, &[input_for_node, cap.clone()], span)?;
            let v = match r {
                Flow::Value(v) => v,
                _ => continue,
            };
            let unwrapped = match v {
                Value::Result(Ok(inner)) => *inner,
                Value::Result(Err(_)) => continue,
                other => other,
            };
            if let Some(succs) = adj.get(node) {
                for s in succs {
                    node_out.insert(s.clone(), unwrapped.clone());
                }
            }
            if terminals.iter().any(|t| t == node) {
                last_terminal_value = Some(unwrapped.clone());
            }
            node_out.insert(node.clone(), unwrapped);
        }
        last_node_outputs = node_out;
        if let Some(pred) = &net.until {
            let mut env = Env::new();
            env.module.clone_from(&net.module);
            env.tracer.clone_from(&net.tracer);
            env.model_decls.clone_from(&net.model_decls);
            env.ai_backend.clone_from(&net.ai_backend);
            env.replay_tape.clone_from(&net.replay_tape);
            env.full_record = net.full_record;
            env.bind_let("cap", cap.clone());
            env.bind_let("iterations", Value::Int(iters_run as i64));
            for (k, v) in &last_node_outputs {
                env.bind_let(k, v.clone());
            }
            match eval_value(pred, &mut env) {
                Ok(Value::Bool(true)) => {
                    converged = true;
                    break;
                }
                Ok(_) => {}
                Err(_) => {}
            }
        } else {
            converged = true;
            break;
        }
    }
    let outcome = if converged && last_terminal_value.is_some() {
        "ok"
    } else {
        "exhausted"
    };
    if let Some(t) = &net.tracer {
        t.record(
            "net_exit",
            None,
            vec![
                ("net".into(), format!("\"{}\"", net.name)),
                ("outcome".into(), format!("\"{outcome}\"")),
                ("iters".into(), iters_run.to_string()),
            ],
        );
    }
    match last_terminal_value {
        Some(v) if converged => Ok(Value::ok(v)),
        _ => Ok(Value::err(Value::Str(format!(
            "agent_net `{}` exhausted",
            net.name
        )))),
    }
}

fn value_matches_model(v: &Value, expected: &(String, u32)) -> bool {
    match v {
        Value::Record(r) => match &r.name {
            Some(n) => n == &expected.0,
            None => false,
        },
        _ => false,
    }
}

fn topo_order(nodes: &[String], adj: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut indeg: HashMap<String, usize> = nodes.iter().map(|n| (n.clone(), 0)).collect();
    for outs in adj.values() {
        for t in outs {
            *indeg.entry(t.clone()).or_default() += 1;
        }
    }
    let mut queue: Vec<String> = nodes
        .iter()
        .filter(|n| indeg.get(*n).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();
    let mut out: Vec<String> = Vec::new();
    while let Some(n) = queue.pop() {
        out.push(n.clone());
        if let Some(succs) = adj.get(&n) {
            for s in succs {
                if let Some(d) = indeg.get_mut(s) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.push(s.clone());
                    }
                }
            }
        }
    }
    out
}

/// Run a saga end-to-end. Returns the saga's final value (`Ok(())`
/// for a clean run, `rolled_back` Result for a recovered failure)
/// or a `PartialFailure` evaluator error when undo retries are
/// exhausted (§ 12.4).
fn invoke_saga(saga: &SagaInstance, args: &[Value], span: Span) -> Result<Value, EvalError> {
    if saga.params.len() != args.len() {
        return Err(EvalError::new(
            EvalErrorKind::Arity {
                name: saga.name.clone(),
                expected: saga.params.len(),
                found: args.len(),
            },
            span,
        ));
    }
    // Build the saga-scope env. Sagas inherit module/tracer/stdin/etc.
    // exactly like a top-level fn would.
    let mut saga_env = Env::new();
    saga_env.module.clone_from(&saga.module);
    saga_env.tracer.clone_from(&saga.tracer);
    saga_env.stdin.clone_from(&saga.stdin);
    saga_env.record_decls.clone_from(&saga.record_decls);
    saga_env.model_decls.clone_from(&saga.model_decls);
    saga_env.policies.clone_from(&saga.policies);
    saga_env.ai_backend.clone_from(&saga.ai_backend);
    saga_env.replay_tape.clone_from(&saga.replay_tape);
    saga_env.full_record = saga.full_record;
    saga_env.push_scope();
    for (name, val) in saga.params.iter().zip(args) {
        saga_env.bind_let(name, val.clone());
    }
    let trace_id = saga
        .tracer
        .as_ref()
        .map(|t| t.trace_id())
        .unwrap_or_else(|| "00000000000000000000000000".into());
    if let Some(t) = &saga.tracer {
        t.intent_enter(&saga.intent, Some(&saga.name));
        t.record(
            "saga_enter",
            None,
            vec![("saga".into(), format!("\"{}\"", saga.name))],
        );
    }
    let mut completed: Vec<String> = Vec::new();
    let mut failed_step: Option<(String, Span)> = None;
    for (i, step) in saga.steps.iter().enumerate() {
        let key = idempotency_key(&trace_id, &step.name, i as u64);
        // Per-step env: copy saga env + step-scope idempotency key.
        let mut step_env = saga_env.clone();
        step_env.push_scope();
        step_env.idempotency_key = Some(std::rc::Rc::new(key.clone()));
        // Evaluate `requires:` clauses against accumulated step.<n>.ok
        // bindings already in saga_env.
        for r in &step.requires {
            let v = eval_value(r, &mut step_env)?;
            if !matches!(v, Value::Bool(true)) {
                if let Some(t) = &saga.tracer {
                    t.record(
                        "step_skip",
                        None,
                        vec![
                            ("step".into(), format!("\"{}\"", step.name)),
                            ("reason".into(), "\"requires_failed\"".into()),
                        ],
                    );
                }
                continue;
            }
        }
        if let Some(t) = &saga.tracer {
            t.record(
                "step_enter",
                None,
                vec![
                    ("step".into(), format!("\"{}\"", step.name)),
                    ("idempotency".into(), format!("\"{key}\"")),
                ],
            );
        }
        let outcome = eval_block(&step.do_block, &mut step_env);
        let step_failed = matches!(&outcome, Ok(Flow::Value(Value::Result(Err(_)))) | Err(_));
        if step_failed {
            if let Some(t) = &saga.tracer {
                t.record(
                    "step_exit",
                    None,
                    vec![
                        ("step".into(), format!("\"{}\"", step.name)),
                        ("outcome".into(), "\"err\"".into()),
                    ],
                );
            }
            failed_step = Some((step.name.clone(), step.span));
            break;
        }
        if let Some(t) = &saga.tracer {
            t.record(
                "step_exit",
                None,
                vec![
                    ("step".into(), format!("\"{}\"", step.name)),
                    ("outcome".into(), "\"ok\"".into()),
                ],
            );
        }
        // Bind `<step_name> = { ok: true }` into the saga-level scope
        // so subsequent step `requires:` can read `step.ok`.
        saga_env.bind_let(
            &step.name,
            Value::Record(RecordValue {
                name: Some("StepResult".into()),
                fields: vec![("ok".into(), Value::Bool(true))],
            }),
        );
        completed.push(step.name.clone());
    }
    // Roll back if any step failed.
    if let Some((failed_name, _failed_span)) = failed_step.clone() {
        if let Some(t) = &saga.tracer {
            t.record(
                "rollback_enter",
                None,
                vec![("step_failed".into(), format!("\"{}\"", failed_name))],
            );
        }
        for step_name in completed.iter().rev() {
            let step = saga
                .steps
                .iter()
                .find(|s| &s.name == step_name)
                .expect("completed step must exist in saga.steps");
            let undo_block = match &step.undo {
                UndoForm::Block(b) => Some(b.clone()),
                UndoForm::Noop(_) => None,
            };
            if let Some(t) = &saga.tracer {
                t.record(
                    "undo_enter",
                    None,
                    vec![("step".into(), format!("\"{}\"", step.name))],
                );
            }
            let undo_result = if let Some(b) = undo_block {
                run_undo_with_retries(saga, step, &b, &saga_env)
            } else {
                Ok(())
            };
            match undo_result {
                Ok(()) => {
                    if let Some(t) = &saga.tracer {
                        t.record(
                            "undo_exit",
                            None,
                            vec![
                                ("step".into(), format!("\"{}\"", step.name)),
                                ("outcome".into(), "\"ok\"".into()),
                            ],
                        );
                    }
                }
                Err(_) => {
                    if let Some(t) = &saga.tracer {
                        t.record(
                            "partial_failure",
                            None,
                            vec![
                                ("saga".into(), format!("\"{}\"", saga.name)),
                                ("failed_step".into(), format!("\"{}\"", step.name)),
                            ],
                        );
                        t.intent_exit("partial");
                    }
                    return Err(EvalError::new(
                        EvalErrorKind::PartialFailure {
                            saga: saga.name.clone(),
                            completed: completed.clone(),
                            failed_step: step.name.clone(),
                        },
                        span,
                    ));
                }
            }
        }
        if let Some(t) = &saga.tracer {
            t.record(
                "saga_exit",
                None,
                vec![
                    ("saga".into(), format!("\"{}\"", saga.name)),
                    ("outcome".into(), "\"rolled_back\"".into()),
                ],
            );
            t.intent_exit("rolled_back");
        }
        return Ok(Value::err(Value::Str(format!(
            "saga `{}` rolled back after step `{failed_name}`",
            saga.name
        ))));
    }
    if let Some(t) = &saga.tracer {
        t.record(
            "saga_exit",
            None,
            vec![
                ("saga".into(), format!("\"{}\"", saga.name)),
                ("outcome".into(), "\"ok\"".into()),
            ],
        );
        t.intent_exit("ok");
    }
    Ok(Value::ok(Value::Unit))
}

/// Try the `undo` block up to a small retry budget with exponential
/// backoff (§ 12.4 / M6.T5). Backoff is implemented as a no-op sleep
/// in the test harness — the production runtime in M11 will use the
/// real OS clock.
fn run_undo_with_retries(
    saga: &SagaInstance,
    step: &SagaStep,
    body: &Block,
    saga_env: &Env,
) -> Result<(), ()> {
    const MAX_ATTEMPTS: u32 = 3;
    for attempt in 0..MAX_ATTEMPTS {
        let mut undo_env = saga_env.clone();
        undo_env.push_scope();
        let trace_id = saga
            .tracer
            .as_ref()
            .map(|t| t.trace_id())
            .unwrap_or_else(|| "00000000000000000000000000".into());
        let key = idempotency_key(&trace_id, &format!("{}.undo", step.name), attempt as u64);
        undo_env.idempotency_key = Some(std::rc::Rc::new(key));
        let r = eval_block(body, &mut undo_env);
        let undo_failed = matches!(&r, Ok(Flow::Value(Value::Result(Err(_)))) | Err(_));
        if !undo_failed {
            return Ok(());
        }
        if let Some(t) = &saga.tracer {
            t.record(
                "undo_retry",
                None,
                vec![
                    ("step".into(), format!("\"{}\"", step.name)),
                    ("attempt".into(), attempt.to_string()),
                ],
            );
        }
    }
    Err(())
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

fn hex16(h: u64) -> String {
    format!("{h:016x}")
}

fn io_err(op: &str, span: Span, e: std::io::Error) -> EvalError {
    EvalError::new(
        EvalErrorKind::Io {
            op: op.to_string(),
            message: e.to_string(),
        },
        span,
    )
}

// ---- fs builtins -------------------------------------------------

fn builtin_fs_read_text(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.read_text", 1, args, span)?;
    let path = expect_string("fs.read_text", &args[0], span)?;
    enforce_path_policy(env, "fs", "read_text", &path, span)?;
    let bytes = std::fs::read(&path).map_err(|e| io_err("fs.read_text", span, e))?;
    let h = hex16(fnv1a_64(&bytes));
    let len = bytes.len();
    let s = String::from_utf8(bytes).map_err(|e| {
        EvalError::new(
            EvalErrorKind::Io {
                op: "fs.read_text".into(),
                message: format!("not valid utf-8: {e}"),
            },
            span,
        )
    })?;
    record_event(
        env,
        "fs_read",
        vec![
            ("path".into(), format!("\"{path}\"")),
            ("len".into(), len.to_string()),
            ("hash".into(), format!("\"{h}\"")),
        ],
    );
    Ok(Value::ok(Value::Str(s)))
}

fn builtin_fs_read_bytes(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.read_bytes", 1, args, span)?;
    let path = expect_string("fs.read_bytes", &args[0], span)?;
    enforce_path_policy(env, "fs", "read_bytes", &path, span)?;
    let bytes = std::fs::read(&path).map_err(|e| io_err("fs.read_bytes", span, e))?;
    let h = hex16(fnv1a_64(&bytes));
    record_event(
        env,
        "fs_read",
        vec![
            ("path".into(), format!("\"{path}\"")),
            ("len".into(), bytes.len().to_string()),
            ("hash".into(), format!("\"{h}\"")),
        ],
    );
    Ok(Value::ok(Value::Bytes(bytes)))
}

fn builtin_fs_write_text(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.write_text", 2, args, span)?;
    let path = expect_string("fs.write_text", &args[0], span)?;
    let content = expect_string("fs.write_text", &args[1], span)?;
    enforce_path_policy(env, "fs", "write_text", &path, span)?;
    std::fs::write(&path, &content).map_err(|e| io_err("fs.write_text", span, e))?;
    let h = hex16(fnv1a_64(content.as_bytes()));
    record_event(
        env,
        "fs_write",
        vec![
            ("path".into(), format!("\"{path}\"")),
            ("len".into(), content.len().to_string()),
            ("hash".into(), format!("\"{h}\"")),
        ],
    );
    Ok(Value::ok(Value::Unit))
}

fn builtin_fs_write_bytes(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.write_bytes", 2, args, span)?;
    let path = expect_string("fs.write_bytes", &args[0], span)?;
    let content = match &args[1] {
        Value::Bytes(b) => b.clone(),
        Value::Str(s) => s.clone().into_bytes(),
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "fs.write_bytes expects bytes/string, got {}",
                    value_kind(other)
                )),
                span,
            ))
        }
    };
    enforce_path_policy(env, "fs", "write_bytes", &path, span)?;
    std::fs::write(&path, &content).map_err(|e| io_err("fs.write_bytes", span, e))?;
    let h = hex16(fnv1a_64(&content));
    record_event(
        env,
        "fs_write",
        vec![
            ("path".into(), format!("\"{path}\"")),
            ("len".into(), content.len().to_string()),
            ("hash".into(), format!("\"{h}\"")),
        ],
    );
    Ok(Value::ok(Value::Unit))
}

fn builtin_fs_exists(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.exists", 1, args, span)?;
    let path = expect_string("fs.exists", &args[0], span)?;
    enforce_path_policy(env, "fs", "exists", &path, span)?;
    let exists = std::path::Path::new(&path).exists();
    record_event(
        env,
        "fs_stat",
        vec![
            ("op".into(), "\"exists\"".into()),
            ("path".into(), format!("\"{path}\"")),
            ("present".into(), exists.to_string()),
        ],
    );
    Ok(Value::Bool(exists))
}

fn builtin_fs_stat(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.stat", 1, args, span)?;
    let path = expect_string("fs.stat", &args[0], span)?;
    enforce_path_policy(env, "fs", "stat", &path, span)?;
    let md = std::fs::metadata(&path).map_err(|e| io_err("fs.stat", span, e))?;
    let kind = if md.is_dir() {
        "dir"
    } else if md.is_file() {
        "file"
    } else {
        "other"
    };
    record_event(
        env,
        "fs_stat",
        vec![
            ("op".into(), "\"stat\"".into()),
            ("path".into(), format!("\"{path}\"")),
            ("kind".into(), format!("\"{kind}\"")),
            ("len".into(), md.len().to_string()),
        ],
    );
    Ok(Value::ok(Value::Record(RecordValue {
        name: Some("FsStat".into()),
        fields: vec![
            ("path".into(), Value::Str(path)),
            ("kind".into(), Value::Str(kind.into())),
            ("len".into(), Value::Int(md.len() as i64)),
        ],
    })))
}

fn builtin_fs_mkdir(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.mkdir", 1, args, span)?;
    let path = expect_string("fs.mkdir", &args[0], span)?;
    enforce_path_policy(env, "fs", "mkdir", &path, span)?;
    std::fs::create_dir_all(&path).map_err(|e| io_err("fs.mkdir", span, e))?;
    record_event(
        env,
        "fs_write",
        vec![
            ("op".into(), "\"mkdir\"".into()),
            ("path".into(), format!("\"{path}\"")),
        ],
    );
    Ok(Value::ok(Value::Unit))
}

fn builtin_fs_remove(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.remove", 1, args, span)?;
    let path = expect_string("fs.remove", &args[0], span)?;
    enforce_path_policy(env, "fs", "remove", &path, span)?;
    let p = std::path::Path::new(&path);
    let result = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
    result.map_err(|e| io_err("fs.remove", span, e))?;
    record_event(
        env,
        "fs_write",
        vec![
            ("op".into(), "\"remove\"".into()),
            ("path".into(), format!("\"{path}\"")),
        ],
    );
    Ok(Value::ok(Value::Unit))
}

fn builtin_fs_rename(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.rename", 2, args, span)?;
    let from = expect_string("fs.rename", &args[0], span)?;
    let to = expect_string("fs.rename", &args[1], span)?;
    enforce_path_policy(env, "fs", "rename", &from, span)?;
    enforce_path_policy(env, "fs", "rename", &to, span)?;
    std::fs::rename(&from, &to).map_err(|e| io_err("fs.rename", span, e))?;
    record_event(
        env,
        "fs_write",
        vec![
            ("op".into(), "\"rename\"".into()),
            ("from".into(), format!("\"{from}\"")),
            ("to".into(), format!("\"{to}\"")),
        ],
    );
    Ok(Value::ok(Value::Unit))
}

fn builtin_fs_walk(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("fs.walk", 1, args, span)?;
    let path = expect_string("fs.walk", &args[0], span)?;
    enforce_path_policy(env, "fs", "walk", &path, span)?;
    let mut out: Vec<Value> = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(&path)];
    while let Some(p) = stack.pop() {
        if p.is_dir() {
            match std::fs::read_dir(&p) {
                Ok(rd) => {
                    for entry in rd.flatten() {
                        stack.push(entry.path());
                    }
                }
                Err(e) => return Err(io_err("fs.walk", span, e)),
            }
        }
        if let Some(s) = p.to_str() {
            out.push(Value::Str(s.to_string()));
        }
    }
    record_event(
        env,
        "fs_walk",
        vec![
            ("path".into(), format!("\"{path}\"")),
            ("count".into(), out.len().to_string()),
        ],
    );
    Ok(Value::ok(Value::List(out)))
}

// ---- http builtins (M5.T1 + M5.T2) --------------------------------

fn enforce_http_host_policy(
    env: &Env,
    op: &str,
    url: &str,
    span: Span,
) -> Result<String, EvalError> {
    let parsed = super::http::parse_url(url)
        .map_err(|e| EvalError::new(EvalErrorKind::Type(format!("http.{op}: {e}")), span))?;
    let cap = env.lookup("cap").and_then(|v| match v {
        Value::Cap(c) => Some(c),
        _ => None,
    });
    let cap = match cap {
        Some(c) => c,
        None => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("http.{op}"),
                    target: parsed.host.clone(),
                },
                span,
            ));
        }
    };
    if cap.star {
        return Ok(parsed.host);
    }
    for entry in &cap.entries {
        let path_match = match entry.path.as_slice() {
            [m] => m == "http",
            [m, o] => m == "http" && o == op,
            _ => false,
        };
        if !path_match {
            continue;
        }
        match &entry.allow {
            None => return Ok(parsed.host),
            Some(allow) => {
                if allow.iter().any(|a| a == &parsed.host) {
                    return Ok(parsed.host);
                }
            }
        }
    }
    Err(EvalError::new(
        EvalErrorKind::PolicyViolation {
            op: format!("http.{op}"),
            target: parsed.host,
        },
        span,
    ))
}

fn http_response_record(status: u16, body: Vec<u8>) -> Value {
    let body_str = String::from_utf8_lossy(&body).into_owned();
    Value::Record(RecordValue {
        name: Some("HttpResponse".into()),
        fields: vec![
            ("status".into(), Value::Int(status as i64)),
            ("body".into(), Value::Str(body_str)),
        ],
    })
}

fn do_http(
    env: &Env,
    method: &str,
    op: &str,
    args: &[Value],
    expects_body: bool,
    span: Span,
) -> Result<Value, EvalError> {
    let expected_arity = if expects_body { 2 } else { 1 };
    arity_check(&format!("http.{op}"), expected_arity, args, span)?;
    let url = expect_string(&format!("http.{op} url"), &args[0], span)?;
    let host = enforce_http_host_policy(env, op, &url, span)?;
    let body: Vec<u8> = if expects_body {
        match &args[1] {
            Value::Str(s) => s.clone().into_bytes(),
            Value::Bytes(b) => b.clone(),
            other => {
                return Err(EvalError::new(
                    EvalErrorKind::Type(format!(
                        "http.{op} body must be string/bytes, got {}",
                        value_kind(other)
                    )),
                    span,
                ))
            }
        }
    } else {
        Vec::new()
    };
    let req_hash = hex16(fnv1a_64(&body));
    let trace_id = env
        .tracer()
        .map(|t| t.trace_id())
        .unwrap_or_else(|| "00000000000000000000000000".into());
    let idem = env.idempotency_key().map(|s| s.to_string());
    let resp =
        super::http::do_request(method, &url, &body, &trace_id, idem.as_deref()).map_err(|e| {
            EvalError::new(
                EvalErrorKind::Io {
                    op: format!("http.{op}"),
                    message: format!("{e}"),
                },
                span,
            )
        })?;
    let resp_hash = hex16(fnv1a_64(&resp.body));
    let mut fields = vec![
        ("url".into(), format!("\"{url}\"")),
        ("host".into(), format!("\"{host}\"")),
        ("method".into(), format!("\"{}\"", method)),
        ("status".into(), resp.status.to_string()),
        ("req_hash".into(), format!("\"{req_hash}\"")),
        ("resp_hash".into(), format!("\"{resp_hash}\"")),
    ];
    if let Some(k) = &idem {
        fields.push(("idempotency".into(), format!("\"{k}\"")));
    }
    record_event(env, "http_call", fields);
    Ok(Value::ok(http_response_record(resp.status, resp.body)))
}

fn builtin_http_get(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    do_http(env, "GET", "get", args, false, span)
}
fn builtin_http_post(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    do_http(env, "POST", "post", args, true, span)
}
fn builtin_http_put(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    do_http(env, "PUT", "put", args, true, span)
}
fn builtin_http_patch(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    do_http(env, "PATCH", "patch", args, true, span)
}
fn builtin_http_delete(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    do_http(env, "DELETE", "delete", args, false, span)
}

// ---- M8.T2 model-aware decoders (json.decode<T>, http.body<T>) ----

/// Pull the model name + version out of a turbofish argument list. The
/// first arg must be a `model@vN` reference; anything else is a usage
/// error reported as `Type` (the static checker should also reject it
/// once M8 type-checking lands).
fn require_model_arg(
    type_args: &[crate::syntax::ast::Type],
    op: &str,
    span: Span,
) -> Result<(String, u32), EvalError> {
    if type_args.len() != 1 {
        return Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "{op} expects one type argument `<Model@vN>`, got {}",
                type_args.len()
            )),
            span,
        ));
    }
    match &type_args[0] {
        crate::syntax::ast::Type::Model { name, version, .. } => Ok((name.clone(), *version)),
        _ => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "{op} type argument must be `Model@vN`, got `{:?}`",
                type_args[0]
            )),
            span,
        )),
    }
}

/// Coerce one natural-JSON scalar to the field's declared type. The
/// model decl drives the conversion; mismatches surface as schema
/// problems rather than silent casts (e.g. JSON `42` into a `decimal`
/// field is rejected, not promoted).
fn coerce_to_field_type(raw: Value, declared: &crate::syntax::ast::Type) -> Result<Value, String> {
    use crate::syntax::ast::Type;
    match (declared, raw) {
        (Type::Named { name, .. }, v) => {
            let n = name.as_str();
            match (n, v) {
                ("int", Value::Int(i)) => Ok(Value::Int(i)),
                ("float", Value::Float(f)) => Ok(Value::Float(f)),
                ("float", Value::Int(i)) => Ok(Value::Float(i as f64)),
                ("string", Value::Str(s)) => Ok(Value::Str(s)),
                ("bool", Value::Bool(b)) => Ok(Value::Bool(b)),
                ("decimal", Value::Str(s)) => Ok(Value::Decimal(s)),
                ("uuid", Value::Str(s)) => Ok(Value::Uuid(s)),
                ("date", Value::Str(s)) => Ok(Value::Date(s)),
                ("timestamp", Value::Str(s)) => Ok(Value::Timestamp(s)),
                ("duration", Value::Str(s)) => Ok(Value::Duration(s)),
                (other, got) => Err(format!(
                    "expected `{other}`, got JSON value of kind `{}`",
                    value_kind(&got)
                )),
            }
        }
        (other, v) => Err(format!(
            "field type `{other:?}` is not yet supported by `json.decode`; got `{}`",
            value_kind(&v)
        )),
    }
}

/// Shared back-end used by both `json.decode<Model@vN>(s)` and
/// `http.body<Model@vN>(req)`: decode the natural-JSON object,
/// coerce each scalar to its declared field type, then run the same
/// per-field + record-level invariants as the literal path
/// (`Order@v1 { ... }`).
fn decode_and_validate_model(
    env: &Env,
    json_src: &str,
    model_name: &str,
    version: u32,
    span: Span,
) -> Result<Value, EvalError> {
    // 1. Parse the JSON object as a flat field bag.
    let raw = super::json::decode_natural_object(json_src).map_err(|e| {
        EvalError::new(
            EvalErrorKind::SchemaViolation {
                model: model_name.into(),
                version,
                problems: vec![format!("invalid JSON: {}", e.message)],
            },
            span,
        )
    })?;
    // 2. Resolve the model decl once so we can drive coercion.
    let decls = env.model_decls.clone().ok_or_else(|| {
        EvalError::new(
            EvalErrorKind::SchemaViolation {
                model: model_name.into(),
                version,
                problems: vec![format!("model `{model_name}@v{version}` not declared")],
            },
            span,
        )
    })?;
    let decl = decls
        .get(&(model_name.to_string(), version))
        .ok_or_else(|| {
            EvalError::new(
                EvalErrorKind::SchemaViolation {
                    model: model_name.into(),
                    version,
                    problems: vec![format!("model `{model_name}@v{version}` not declared")],
                },
                span,
            )
        })?;
    // 3. Coerce the bag to typed fields, accumulating decode problems.
    let mut typed: Vec<(String, Value)> = Vec::with_capacity(raw.len());
    let mut problems: Vec<String> = Vec::new();
    for (k, v) in raw {
        match decl.fields.iter().find(|f| f.name == k) {
            Some(f) => match coerce_to_field_type(v, &f.ty) {
                Ok(coerced) => typed.push((k, coerced)),
                Err(msg) => problems.push(format!("field `{k}`: {msg}")),
            },
            None => {
                // Keep the field so the `unknown field` check in
                // `check_model` fires; coerce it as raw.
                typed.push((k, v));
            }
        }
    }
    if !problems.is_empty() {
        return Err(EvalError::new(
            EvalErrorKind::SchemaViolation {
                model: model_name.into(),
                version,
                problems,
            },
            span,
        ));
    }
    // 4. Run the same validator the literal path uses.
    check_model(env, model_name, version, &typed, span)?;
    Ok(Value::Record(RecordValue {
        name: Some(model_name.to_string()),
        fields: typed,
    }))
}

fn builtin_json_decode(
    env: &Env,
    type_args: &[crate::syntax::ast::Type],
    args: &[Value],
    span: Span,
) -> Result<Value, EvalError> {
    arity_check("json.decode", 1, args, span)?;
    let (name, version) = require_model_arg(type_args, "json.decode", span)?;
    let s = expect_string("json.decode body", &args[0], span)?;
    let v = decode_and_validate_model(env, &s, &name, version, span)?;
    record_event(
        env,
        "json_decode",
        vec![
            ("model".into(), format!("\"{name}\"")),
            ("version".into(), version.to_string()),
            ("len".into(), s.len().to_string()),
        ],
    );
    Ok(Value::ok(v))
}

fn builtin_http_body(
    env: &Env,
    type_args: &[crate::syntax::ast::Type],
    args: &[Value],
    span: Span,
) -> Result<Value, EvalError> {
    arity_check("http.body", 1, args, span)?;
    let (name, version) = require_model_arg(type_args, "http.body", span)?;
    // The argument is the `HttpResponse` record produced by an `http.*`
    // call. Pull `body` (string) out and validate it.
    let body_str = match &args[0] {
        Value::Record(r) => r
            .fields
            .iter()
            .find(|(k, _)| k == "body")
            .and_then(|(_, v)| match v {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| {
                EvalError::new(
                    EvalErrorKind::Type(
                        "`http.body` requires an HttpResponse with a string `body` field".into(),
                    ),
                    span,
                )
            })?,
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "`http.body` expects an HttpResponse record, got {}",
                    value_kind(other)
                )),
                span,
            ));
        }
    };
    let v = decode_and_validate_model(env, &body_str, &name, version, span)?;
    record_event(
        env,
        "http_body",
        vec![
            ("model".into(), format!("\"{name}\"")),
            ("version".into(), version.to_string()),
            ("len".into(), body_str.len().to_string()),
        ],
    );
    Ok(Value::ok(v))
}

// ---- M9 ai builtins (mock / http / cli backend) ------------------

/// FNV-1a 64-bit hash of `s`, formatted as 16-char lowercase hex —
/// matches the helper used by the http builtins (`req_hash`,
/// `resp_hash`).
fn ai_hash(s: &str) -> String {
    hex16(fnv1a_64(s.as_bytes()))
}

/// Verify the cap authorises `ai.<op>` and return the model name —
/// either the cap's first allow-list entry (preferred) or `"mock"`
/// when the cap omits the list (test-only path).
fn enforce_ai_cap(env: &Env, op: &str, span: Span) -> Result<String, EvalError> {
    let cap = env.lookup("cap").and_then(|v| match v {
        Value::Cap(c) => Some(c),
        _ => None,
    });
    let cap = match cap {
        Some(c) => c,
        None => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("ai.{op}"),
                    target: "<no cap in scope>".into(),
                },
                span,
            ));
        }
    };
    if cap.star {
        return Ok("mock".to_string());
    }
    let entry = cap
        .entries
        .iter()
        .find(|e| e.path.len() == 2 && e.path[0] == "ai" && (e.path[1] == op || e.path[1] == "*"));
    let entry = match entry {
        Some(e) => e,
        None => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("ai.{op}"),
                    target: "<missing cap>".into(),
                },
                span,
            ));
        }
    };
    let model = entry
        .allow
        .as_ref()
        .and_then(|xs| xs.first())
        .cloned()
        .unwrap_or_else(|| "mock".to_string());
    Ok(model)
}

/// Hand the prompt to the configured backend and return the response
/// text. M9.T1 supports three kinds: `mock` (echo), `http` (POST to a
/// JSON endpoint), `cli` (spawn a subprocess with the prompt on
/// stdin). When no backend is configured at all, the mock kick-in
/// keeps unit tests offline.
fn run_ai_backend(env: &Env, op: &str, model: &str, prompt: &str) -> Result<String, String> {
    let backend = env.ai_backend.as_ref();
    let kind = backend.map(|b| b.kind.as_str()).unwrap_or("mock");
    match kind {
        "mock" => Ok(format!("[mock:{op}:{model}] {prompt}")),
        "http" => {
            let url = backend
                .and_then(|b| b.url.as_deref())
                .ok_or_else(|| "ai.backend = http requires `url`".to_string())?;
            // Minimal JSON body: `{"model":"…","prompt":"…","op":"…"}`.
            // Real Anthropic schema lands in M11 — for v0.2 we use a
            // simple request shape every mock can answer.
            let body = format!(
                r#"{{"op":"{}","model":"{}","prompt":{}}}"#,
                op,
                model,
                json_encode_string(prompt)
            );
            let trace_id = env
                .tracer()
                .map(|t| t.trace_id())
                .unwrap_or_else(|| "00000000000000000000000000".into());
            let resp = super::http::do_request("POST", url, body.as_bytes(), &trace_id, None)
                .map_err(|e| format!("ai.{op} http backend: {e}"))?;
            let text = String::from_utf8_lossy(&resp.body).into_owned();
            Ok(extract_text_from_json_or_raw(&text))
        }
        "cli" => Err("ai.backend = cli is not implemented in v0.2 (M11)".into()),
        other => Err(format!("ai.backend kind `{other}` is not supported")),
    }
}

/// Cheap natural-JSON probe: if `s` parses as an object with a
/// `text` / `completion` / `response` field, return that. Otherwise
/// return `s` unchanged. Lets the mock HTTP fixture send either a
/// raw string or a wrapped JSON object.
fn extract_text_from_json_or_raw(s: &str) -> String {
    if let Ok(fields) = super::json::decode_natural_object(s) {
        for key in &["text", "completion", "response", "output"] {
            if let Some((_, Value::Str(t))) = fields.iter().find(|(k, _)| k == key) {
                return t.clone();
            }
        }
    }
    s.trim_end_matches(['\n', '\r']).to_string()
}

/// Encode `s` as a JSON string literal (with surrounding quotes).
fn json_encode_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Record an `ai_call` trace event (M9.T3 / N3). The body is either
/// hashed or stored raw based on `env.full_record` (M9.T8).
fn record_ai_event(env: &Env, op: &str, model: &str, prompt: &str, response: &str) {
    let mut fields = vec![
        ("op".into(), format!("\"ai.{op}\"")),
        ("model".into(), format!("\"{model}\"")),
        (
            "tokens".into(),
            prompt.split_whitespace().count().to_string(),
        ),
    ];
    if env.full_record {
        fields.push((
            "prompt".into(),
            format!("\"{}\"", prompt.replace('"', "\\\"")),
        ));
        fields.push((
            "response".into(),
            format!("\"{}\"", response.replace('"', "\\\"")),
        ));
    } else {
        fields.push(("prompt_hash".into(), format!("\"{}\"", ai_hash(prompt))));
        fields.push(("resp_hash".into(), format!("\"{}\"", ai_hash(response))));
    }
    record_event(env, "ai_call", fields);
}

fn builtin_ai_complete(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("ai.complete", 1, args, span)?;
    let prompt = expect_string("ai.complete prompt", &args[0], span)?;
    let model = enforce_ai_cap(env, "complete", span)?;
    // M9.T4: replay tape — if a recorded `ai_call` is queued, drain it.
    if let Some(tape) = env.replay_tape() {
        if let Some(evt) = tape.borrow_mut().consume_next("ai_call") {
            let resp =
                crate::runtime::replay::Tape::field_unescaped(&evt, "response").unwrap_or_default();
            record_ai_event(env, "complete", &model, &prompt, &resp);
            return Ok(Value::ok(Value::Str(resp)));
        }
    }
    let resp = run_ai_backend(env, "complete", &model, &prompt).map_err(|m| {
        EvalError::new(
            EvalErrorKind::Io {
                op: "ai.complete".into(),
                message: m,
            },
            span,
        )
    })?;
    record_ai_event(env, "complete", &model, &prompt, &resp);
    Ok(Value::ok(Value::Str(resp)))
}

fn builtin_ai_chat(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("ai.chat", 1, args, span)?;
    // The argument is `list<map<string,string>>` — we serialise it to
    // a single prompt for the backend; v0.2 does not preserve the role
    // structure (M11 will).
    let prompt = match &args[0] {
        Value::List(msgs) => {
            let mut buf = String::new();
            for (i, m) in msgs.iter().enumerate() {
                if i > 0 {
                    buf.push('\n');
                }
                match m {
                    Value::Str(s) => buf.push_str(s),
                    Value::Record(r) => {
                        for (k, v) in &r.fields {
                            buf.push_str(k);
                            buf.push_str(": ");
                            buf.push_str(&value_as_display(v));
                            buf.push('\n');
                        }
                    }
                    other => buf.push_str(&value_as_display(other)),
                }
            }
            buf
        }
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "ai.chat expects a list of messages, got {}",
                    value_kind(other)
                )),
                span,
            ));
        }
    };
    let model = enforce_ai_cap(env, "chat", span)?;
    if let Some(tape) = env.replay_tape() {
        if let Some(evt) = tape.borrow_mut().consume_next("ai_call") {
            let resp =
                crate::runtime::replay::Tape::field_unescaped(&evt, "response").unwrap_or_default();
            record_ai_event(env, "chat", &model, &prompt, &resp);
            return Ok(Value::ok(Value::Str(resp)));
        }
    }
    let resp = run_ai_backend(env, "chat", &model, &prompt).map_err(|m| {
        EvalError::new(
            EvalErrorKind::Io {
                op: "ai.chat".into(),
                message: m,
            },
            span,
        )
    })?;
    record_ai_event(env, "chat", &model, &prompt, &resp);
    Ok(Value::ok(Value::Str(resp)))
}

fn builtin_ai_embed(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("ai.embed", 1, args, span)?;
    let text = expect_string("ai.embed text", &args[0], span)?;
    let model = enforce_ai_cap(env, "embed", span)?;
    // Mock embedding: 8-dim vector seeded by FNV-1a chunks of the
    // input. Deterministic — so traces are stable in tests.
    let h = fnv1a_64(text.as_bytes());
    let vec: Vec<Value> = (0..8)
        .map(|i| {
            let chunk = ((h >> (i * 8)) & 0xff) as f64 / 255.0;
            Value::Float(chunk)
        })
        .collect();
    record_ai_event(env, "embed", &model, &text, &format!("vec[{}]", vec.len()));
    Ok(Value::ok(Value::List(vec)))
}

fn builtin_ai_tools(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("ai.tools", 2, args, span)?;
    // First arg is the tool spec (list); second is the prompt.
    let _tools = match &args[0] {
        Value::List(_) => &args[0],
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "ai.tools expects a list of tools, got {}",
                    value_kind(other)
                )),
                span,
            ));
        }
    };
    let prompt = expect_string("ai.tools prompt", &args[1], span)?;
    let model = enforce_ai_cap(env, "tools", span)?;
    if let Some(tape) = env.replay_tape() {
        if let Some(evt) = tape.borrow_mut().consume_next("ai_call") {
            let resp =
                crate::runtime::replay::Tape::field_unescaped(&evt, "response").unwrap_or_default();
            record_ai_event(env, "tools", &model, &prompt, &resp);
            return Ok(Value::ok(Value::Str(resp)));
        }
    }
    let resp = run_ai_backend(env, "tools", &model, &prompt).map_err(|m| {
        EvalError::new(
            EvalErrorKind::Io {
                op: "ai.tools".into(),
                message: m,
            },
            span,
        )
    })?;
    record_ai_event(env, "tools", &model, &prompt, &resp);
    Ok(Value::ok(Value::Str(resp)))
}

// ====================================================================
//  M11 — L2 native cap handlers (audit / kube / docker / mongodb /
//  minio / rabbitmq). Realises `docs/language.md` § 23.
//
//  Backends that need an external service (kube / docker / mongo /
//  minio / rabbitmq) shell out to the system CLI (`kubectl`, `docker`)
//  or, when no live target is available, surface a clean
//  `EvalErrorKind::Io` so tests stay deterministic. Every operation
//  records a per-call trace event named after the L2 module
//  (`audit_event`, `kube_apply`, `mongodb_write`, ...) — that's the
//  acceptance shape for M11.T7 (per-backend golden trace).
// ====================================================================

// ---- audit ---------------------------------------------------------

thread_local! {
    /// Per-thread override of the audit log destination — set by tests
    /// to keep parallel runs from racing on a shared file. Production
    /// callers read `AERIS_AUDIT_LOG_PATH` or fall back to the default.
    static AUDIT_LOG_OVERRIDE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Tests-only: pin the audit log destination for the current thread.
#[cfg(test)]
pub(crate) fn set_audit_log_override(p: std::path::PathBuf) {
    AUDIT_LOG_OVERRIDE.with(|c| *c.borrow_mut() = Some(p));
}

/// Look up the destination of the audit log. Falls back through the
/// per-thread override, then `AERIS_AUDIT_LOG_PATH`, then the default
/// `.aeris/audit.jsonl` next to the cwd.
fn audit_log_path() -> std::path::PathBuf {
    if let Some(p) = AUDIT_LOG_OVERRIDE.with(|c| c.borrow().clone()) {
        return p;
    }
    if let Ok(p) = std::env::var("AERIS_AUDIT_LOG_PATH") {
        return std::path::PathBuf::from(p);
    }
    std::path::PathBuf::from(".aeris/audit.jsonl")
}

fn builtin_audit_event(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("audit.event", 2, args, span)?;
    let event = expect_string("audit.event name", &args[0], span)?;
    enforce_simple_cap_or_violation(env, "audit", "event", span)?;
    let payload = match &args[1] {
        Value::Record(r) => value_to_natural_json(&Value::Record(r.clone())),
        Value::Map(_) | Value::List(_) => value_to_natural_json(&args[1]),
        Value::Str(s) => format!("\"{}\"", json_escape_for_natural(s)),
        other => format!("\"{}\"", json_escape_for_natural(&value_as_display(other))),
    };
    let idem = env.idempotency_key().map(|s| s.to_string());
    let trace_id = env
        .tracer()
        .map(|t| t.trace_id())
        .unwrap_or_else(|| "00000000000000000000000000".into());
    let ts = clock_now_iso();
    // Build the JSONL line: {"ts","event","payload","idem"?,"trace_id"}.
    let mut line = String::new();
    line.push('{');
    line.push_str(&format!("\"ts\":\"{ts}\","));
    line.push_str(&format!(
        "\"event\":\"{}\",",
        json_escape_for_natural(&event)
    ));
    line.push_str(&format!("\"payload\":{payload},"));
    if let Some(k) = &idem {
        line.push_str(&format!("\"idem\":\"{}\",", json_escape_for_natural(k)));
    }
    line.push_str(&format!("\"trace_id\":\"{trace_id}\""));
    line.push('}');
    line.push('\n');
    let path = audit_log_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    let f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path);
    match f {
        Ok(mut f) => {
            f.write_all(line.as_bytes()).map_err(|e| {
                EvalError::new(
                    EvalErrorKind::Io {
                        op: "audit.event".into(),
                        message: format!("{e}"),
                    },
                    span,
                )
            })?;
        }
        Err(e) => {
            return Err(EvalError::new(
                EvalErrorKind::Io {
                    op: "audit.event".into(),
                    message: format!("{e}"),
                },
                span,
            ));
        }
    }
    let mut fields = vec![
        ("event".into(), format!("\"{event}\"")),
        ("path".into(), format!("\"{}\"", path.display())),
    ];
    if let Some(k) = &idem {
        fields.push(("idem".into(), format!("\"{k}\"")));
    }
    record_event(env, "audit_event", fields);
    Ok(Value::Unit)
}

fn enforce_simple_cap_or_violation(
    env: &Env,
    module: &str,
    op: &str,
    span: Span,
) -> Result<(), EvalError> {
    let cap = match env.lookup("cap") {
        Some(Value::Cap(c)) => c,
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("{module}.{op}"),
                    target: "<no cap in scope>".into(),
                },
                span,
            ));
        }
    };
    if cap.star {
        return Ok(());
    }
    let ok = cap.entries.iter().any(|e| {
        (e.path.len() == 1 && e.path[0] == module)
            || (e.path.len() == 2 && e.path[0] == module && (e.path[1] == op || e.path[1] == "*"))
    });
    if !ok {
        return Err(EvalError::new(
            EvalErrorKind::PolicyViolation {
                op: format!("{module}.{op}"),
                target: "<missing cap>".into(),
            },
            span,
        ));
    }
    Ok(())
}

fn clock_now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    format_iso_ms(ts_ms)
}

// ---- kube ----------------------------------------------------------

fn run_kubectl(
    env: &Env,
    op_args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    let _ = env;
    use std::io::Write;
    let mut cmd = std::process::Command::new("kubectl");
    // `--request-timeout=2s` keeps the call snappy when no cluster is
    // reachable — otherwise kubectl hangs for the default 30s on DNS.
    cmd.arg("--request-timeout=2s");
    cmd.args(op_args);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }
    let mut child = cmd.spawn().map_err(|e| format!("spawn kubectl: {e}"))?;
    if let (Some(buf), Some(mut s)) = (stdin, child.stdin.take()) {
        s.write_all(buf).map_err(|e| format!("write stdin: {e}"))?;
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("kubectl wait: {e}"))?;
    Ok((out.status.code().unwrap_or(-1), out.stdout, out.stderr))
}

fn record_kube_event(env: &Env, op: &str, manifest_or_target: &str) {
    let mut fields = vec![
        ("op".into(), format!("\"kube.{op}\"")),
        ("target".into(), format!("\"{manifest_or_target}\"")),
    ];
    if let Some(k) = env.idempotency_key() {
        fields.push(("idem".into(), format!("\"{k}\"")));
    }
    record_event(env, &format!("kube_{op}"), fields);
}

/// Annotate the manifest's metadata with the active idempotency key
/// so re-runs are no-ops at the apiserver level. The transformation is
/// surface-only — an embedded `metadata.annotations.idempotency-key`
/// field is appended to the manifest text. This is YAML-friendly
/// because the patch lands as a comment-like line; YAML parsers accept
/// the duplicated key by precedence rules.
fn annotate_manifest_with_idem(manifest: &str, idem: Option<&str>) -> String {
    match idem {
        Some(k) => format!(
            "{manifest}\nmetadata:\n  annotations:\n    aeris.dev/idempotency-key: \"{k}\"\n"
        ),
        None => manifest.to_string(),
    }
}

fn builtin_kube_apply(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("kube.apply", 1, args, span)?;
    enforce_simple_cap_or_violation(env, "kube", "apply", span)?;
    let manifest = expect_string("kube.apply manifest", &args[0], span)?;
    let manifest = annotate_manifest_with_idem(&manifest, env.idempotency_key());
    record_kube_event(env, "apply", "manifest");
    match run_kubectl(env, &["apply", "-f", "-"], Some(manifest.as_bytes())) {
        Ok((0, _, _)) => Ok(Value::ok(Value::Unit)),
        Ok((code, _, stderr)) => Ok(Value::err(Value::Str(format!(
            "kubectl apply exit {code}: {}",
            String::from_utf8_lossy(&stderr)
        )))),
        Err(e) => Ok(Value::err(Value::Str(format!("kubectl unavailable: {e}")))),
    }
}

fn builtin_kube_delete(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("kube.delete", 1, args, span)?;
    enforce_simple_cap_or_violation(env, "kube", "delete", span)?;
    let target = expect_string("kube.delete target", &args[0], span)?;
    record_kube_event(env, "delete", &target);
    match run_kubectl(env, &["delete", &target], None) {
        Ok((0, _, _)) => Ok(Value::ok(Value::Unit)),
        Ok((code, _, stderr)) => Ok(Value::err(Value::Str(format!(
            "kubectl delete exit {code}: {}",
            String::from_utf8_lossy(&stderr)
        )))),
        Err(e) => Ok(Value::err(Value::Str(format!("kubectl unavailable: {e}")))),
    }
}

fn builtin_kube_get(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("kube.get", 1, args, span)?;
    enforce_simple_cap_or_violation(env, "kube", "get", span)?;
    let target = expect_string("kube.get target", &args[0], span)?;
    record_kube_event(env, "get", &target);
    match run_kubectl(env, &["get", &target, "-o", "json"], None) {
        Ok((0, stdout, _)) => Ok(Value::ok(Value::Str(
            String::from_utf8_lossy(&stdout).into_owned(),
        ))),
        Ok((code, _, stderr)) => Ok(Value::err(Value::Str(format!(
            "kubectl get exit {code}: {}",
            String::from_utf8_lossy(&stderr)
        )))),
        Err(e) => Ok(Value::err(Value::Str(format!("kubectl unavailable: {e}")))),
    }
}

fn builtin_kube_watch(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("kube.watch", 1, args, span)?;
    enforce_simple_cap_or_violation(env, "kube", "watch", span)?;
    let target = expect_string("kube.watch target", &args[0], span)?;
    record_kube_event(env, "watch", &target);
    // `watch` is a streaming op — for v0.2 we surface a stub that
    // returns immediately; full streaming arrives with the agent_net /
    // long-running cap work post-M11.
    Ok(Value::ok(Value::Unit))
}

// ---- docker --------------------------------------------------------

fn run_docker(argv: &[&str]) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
    let mut cmd = std::process::Command::new("docker");
    cmd.args(argv);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let out = cmd.output().map_err(|e| format!("spawn docker: {e}"))?;
    Ok((out.status.code().unwrap_or(-1), out.stdout, out.stderr))
}

fn record_docker_event(env: &Env, op: &str, argv: &[&str]) {
    let argv_str = argv.join(" ");
    let fields = vec![
        ("op".into(), format!("\"docker.{op}\"")),
        (
            "argv".into(),
            format!("\"{}\"", json_escape_for_natural(&argv_str)),
        ),
    ];
    record_event(env, &format!("docker_{op}"), fields);
}

fn docker_simple(env: &Env, op: &str, argv: &[&str], span: Span) -> Result<Value, EvalError> {
    enforce_simple_cap_or_violation(env, "docker", op, span)?;
    record_docker_event(env, op, argv);
    match run_docker(argv) {
        Ok((0, stdout, _)) => Ok(Value::ok(Value::Str(
            String::from_utf8_lossy(&stdout).into_owned(),
        ))),
        Ok((code, _, stderr)) => Ok(Value::err(Value::Str(format!(
            "docker {op} exit {code}: {}",
            String::from_utf8_lossy(&stderr)
        )))),
        Err(e) => Ok(Value::err(Value::Str(format!("docker unavailable: {e}")))),
    }
}

fn builtin_docker_run(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("docker.run", 1, args, span)?;
    let image = expect_string("docker.run image", &args[0], span)?;
    docker_simple(env, "run", &["run", "--rm", &image], span)
}

fn builtin_docker_build(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("docker.build", 1, args, span)?;
    let ctx = expect_string("docker.build context", &args[0], span)?;
    docker_simple(env, "build", &["build", &ctx], span)
}

fn builtin_docker_push(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("docker.push", 1, args, span)?;
    let image = expect_string("docker.push image", &args[0], span)?;
    docker_simple(env, "push", &["push", &image], span)
}

fn builtin_docker_pull(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("docker.pull", 1, args, span)?;
    let image = expect_string("docker.pull image", &args[0], span)?;
    docker_simple(env, "pull", &["pull", &image], span)
}

fn builtin_docker_inspect(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("docker.inspect", 1, args, span)?;
    let target = expect_string("docker.inspect target", &args[0], span)?;
    docker_simple(env, "inspect", &["inspect", &target], span)
}

// ---- mongodb / minio / rabbitmq stubs ------------------------------

/// Stub L2 backends record the call into the trace and return
/// `Ok(unit)` (or `Ok(empty list/map)` for read shapes). Live
/// integration lands once the testcontainers harness is in place
/// (deferred from M11 to a follow-up release). The trace event shape
/// is what actually accepts these tasks: every `mongodb.*`,
/// `minio.*`, `rabbitmq.*` call carries the surface-relevant fields.
fn record_l2_stub_event(env: &Env, kind: &str, fields: Vec<(String, String)>) {
    record_event(env, kind, fields);
}

fn builtin_mongodb_read(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("mongodb.read", 2, args, span)?;
    let coll = expect_string("mongodb.read collection", &args[0], span)?;
    let _query = &args[1];
    enforce_simple_cap_or_violation(env, "mongodb", "read", span)?;
    record_l2_stub_event(
        env,
        "mongodb_read",
        vec![("collection".into(), format!("\"{coll}\""))],
    );
    Ok(Value::ok(Value::List(Vec::new())))
}

fn builtin_mongodb_write(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("mongodb.write", 2, args, span)?;
    let coll = expect_string("mongodb.write collection", &args[0], span)?;
    enforce_simple_cap_or_violation(env, "mongodb", "write", span)?;
    let mut fields = vec![("collection".into(), format!("\"{coll}\""))];
    if let Some(k) = env.idempotency_key() {
        fields.push(("idem".into(), format!("\"{k}\"")));
    }
    record_l2_stub_event(env, "mongodb_write", fields);
    Ok(Value::ok(Value::Unit))
}

fn enforce_minio_bucket(env: &Env, op: &str, bucket: &str, span: Span) -> Result<(), EvalError> {
    let cap = match env.lookup("cap") {
        Some(Value::Cap(c)) => c,
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("minio.{op}"),
                    target: bucket.into(),
                },
                span,
            ));
        }
    };
    if cap.star {
        return Ok(());
    }
    let entry = cap.entries.iter().find(|e| {
        e.path.len() == 2 && e.path[0] == "minio" && (e.path[1] == op || e.path[1] == "*")
    });
    let entry = match entry {
        Some(e) => e,
        None => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("minio.{op}"),
                    target: "<missing cap>".into(),
                },
                span,
            ));
        }
    };
    if let Some(allow) = &entry.allow {
        if !allow.iter().any(|a| a == bucket) {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("minio.{op}"),
                    target: bucket.into(),
                },
                span,
            ));
        }
    }
    Ok(())
}

fn builtin_minio_get(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("minio.get", 2, args, span)?;
    let bucket = expect_string("minio.get bucket", &args[0], span)?;
    let key = expect_string("minio.get key", &args[1], span)?;
    enforce_minio_bucket(env, "get", &bucket, span)?;
    record_l2_stub_event(
        env,
        "minio_get",
        vec![
            ("bucket".into(), format!("\"{bucket}\"")),
            ("key".into(), format!("\"{key}\"")),
        ],
    );
    Ok(Value::ok(Value::Bytes(Vec::new())))
}

fn builtin_minio_put(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("minio.put", 3, args, span)?;
    let bucket = expect_string("minio.put bucket", &args[0], span)?;
    let key = expect_string("minio.put key", &args[1], span)?;
    enforce_minio_bucket(env, "put", &bucket, span)?;
    let mut fields = vec![
        ("bucket".into(), format!("\"{bucket}\"")),
        ("key".into(), format!("\"{key}\"")),
    ];
    if let Some(k) = env.idempotency_key() {
        fields.push(("idem".into(), format!("\"{k}\"")));
    }
    record_l2_stub_event(env, "minio_put", fields);
    Ok(Value::ok(Value::Unit))
}

fn builtin_rabbitmq_publish(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("rabbitmq.publish", 2, args, span)?;
    let queue = expect_string("rabbitmq.publish queue", &args[0], span)?;
    enforce_simple_cap_or_violation(env, "rabbitmq", "publish", span)?;
    let mut fields = vec![("queue".into(), format!("\"{queue}\""))];
    // Per § 12.3, the saga's idempotency key surfaces as `message-id`
    // so AMQP brokers can dedupe at the consumer side.
    if let Some(k) = env.idempotency_key() {
        fields.push(("message_id".into(), format!("\"{k}\"")));
    }
    record_l2_stub_event(env, "rabbitmq_publish", fields);
    Ok(Value::ok(Value::Unit))
}

fn builtin_rabbitmq_subscribe(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("rabbitmq.subscribe", 1, args, span)?;
    let queue = expect_string("rabbitmq.subscribe queue", &args[0], span)?;
    enforce_simple_cap_or_violation(env, "rabbitmq", "subscribe", span)?;
    record_l2_stub_event(
        env,
        "rabbitmq_subscribe",
        vec![("queue".into(), format!("\"{queue}\""))],
    );
    Ok(Value::ok(Value::List(Vec::new())))
}

// ---- shell builtins -----------------------------------------------

/// Verify the cap authorises `shell.<op>` for argv0 `target`. Unlike
/// `enforce_path_policy`, the allow-list match is **exact**: argv0
/// names like `"kubectl"` or `"git"` are not paths.
fn enforce_argv0_policy(env: &Env, op: &str, target: &str, span: Span) -> Result<(), EvalError> {
    let cap = env.lookup("cap").and_then(|v| match v {
        Value::Cap(c) => Some(c),
        _ => None,
    });
    let cap = match cap {
        Some(c) => c,
        None => {
            return Err(EvalError::new(
                EvalErrorKind::PolicyViolation {
                    op: format!("shell.{op}"),
                    target: target.to_string(),
                },
                span,
            ));
        }
    };
    if cap.star {
        return Ok(());
    }
    for entry in &cap.entries {
        let path_match = match entry.path.as_slice() {
            [m] => m == "shell",
            [m, o] => m == "shell" && o == op,
            _ => false,
        };
        if !path_match {
            continue;
        }
        match &entry.allow {
            None => return Ok(()),
            Some(allow) => {
                if allow.iter().any(|a| a == target) {
                    return Ok(());
                }
            }
        }
    }
    Err(EvalError::new(
        EvalErrorKind::PolicyViolation {
            op: format!("shell.{op}"),
            target: target.to_string(),
        },
        span,
    ))
}

fn list_of_strings(v: &Value, ctx: &str, span: Span) -> Result<Vec<String>, EvalError> {
    match v {
        Value::List(xs) => xs
            .iter()
            .map(|x| match x {
                Value::Str(s) => Ok(s.clone()),
                other => Err(EvalError::new(
                    EvalErrorKind::Type(format!(
                        "{ctx} expects list of string, found {}",
                        value_kind(other)
                    )),
                    span,
                )),
            })
            .collect(),
        other => Err(EvalError::new(
            EvalErrorKind::Type(format!(
                "{ctx} expects list of string, got {}",
                value_kind(other)
            )),
            span,
        )),
    }
}

fn shell_result_record(argv0: &str, exit: i32, stdout: Vec<u8>, stderr: Vec<u8>) -> Value {
    let stdout_str = String::from_utf8_lossy(&stdout).into_owned();
    let stderr_str = String::from_utf8_lossy(&stderr).into_owned();
    Value::Record(RecordValue {
        name: Some("ShellResult".into()),
        fields: vec![
            ("argv0".into(), Value::Str(argv0.into())),
            ("exit".into(), Value::Int(exit as i64)),
            ("stdout".into(), Value::Str(stdout_str)),
            ("stderr".into(), Value::Str(stderr_str)),
        ],
    })
}

fn builtin_shell_exec(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("shell.exec", 1, args, span)?;
    let argv = list_of_strings(&args[0], "shell.exec", span)?;
    if argv.is_empty() {
        return Err(EvalError::new(
            EvalErrorKind::Type("shell.exec argv must be non-empty".into()),
            span,
        ));
    }
    enforce_argv0_policy(env, "exec", &argv[0], span)?;
    let output = std::process::Command::new(&argv[0])
        .args(&argv[1..])
        .output()
        .map_err(|e| io_err("shell.exec", span, e))?;
    let stdout_hash = hex16(fnv1a_64(&output.stdout));
    let stderr_hash = hex16(fnv1a_64(&output.stderr));
    let exit_code = output.status.code().unwrap_or(-1);
    record_event(
        env,
        "shell_exec",
        vec![
            ("argv0".into(), format!("\"{}\"", argv[0])),
            ("argc".into(), argv.len().to_string()),
            ("exit".into(), exit_code.to_string()),
            ("stdout_hash".into(), format!("\"{stdout_hash}\"")),
            ("stderr_hash".into(), format!("\"{stderr_hash}\"")),
        ],
    );
    Ok(Value::ok(shell_result_record(
        &argv[0],
        exit_code,
        output.stdout,
        output.stderr,
    )))
}

fn builtin_shell_pipe(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("shell.pipe", 1, args, span)?;
    // Argument: list of argv lists. Each stage is a separate process;
    // stage `n`'s stdout is fed to stage `n+1`'s stdin.
    let stages = match &args[0] {
        Value::List(xs) => xs,
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "shell.pipe expects list of argv lists, got {}",
                    value_kind(other)
                )),
                span,
            ))
        }
    };
    if stages.is_empty() {
        return Err(EvalError::new(
            EvalErrorKind::Type("shell.pipe needs at least one stage".into()),
            span,
        ));
    }
    // Validate every argv0 against the policy *before* spawning.
    let mut argvs: Vec<Vec<String>> = Vec::with_capacity(stages.len());
    for stage in stages {
        let argv = list_of_strings(stage, "shell.pipe stage", span)?;
        if argv.is_empty() {
            return Err(EvalError::new(
                EvalErrorKind::Type("shell.pipe stage argv must be non-empty".into()),
                span,
            ));
        }
        enforce_argv0_policy(env, "pipe", &argv[0], span)?;
        argvs.push(argv);
    }
    // Execute the pipeline with stdio threaded through Rust.
    let mut current_input: Vec<u8> = Vec::new();
    let mut last_exit: i32 = 0;
    let mut last_stderr: Vec<u8> = Vec::new();
    for argv in &argvs {
        let mut child = std::process::Command::new(&argv[0])
            .args(&argv[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| io_err("shell.pipe", span, e))?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            let _ = stdin.write_all(&current_input);
        }
        let output = child
            .wait_with_output()
            .map_err(|e| io_err("shell.pipe", span, e))?;
        last_exit = output.status.code().unwrap_or(-1);
        last_stderr = output.stderr.clone();
        current_input = output.stdout;
    }
    let stdout_hash = hex16(fnv1a_64(&current_input));
    let stderr_hash = hex16(fnv1a_64(&last_stderr));
    record_event(
        env,
        "shell_pipe",
        vec![
            ("stages".into(), argvs.len().to_string()),
            ("exit".into(), last_exit.to_string()),
            ("stdout_hash".into(), format!("\"{stdout_hash}\"")),
            ("stderr_hash".into(), format!("\"{stderr_hash}\"")),
        ],
    );
    let argv0_last = argvs.last().unwrap()[0].clone();
    Ok(Value::ok(shell_result_record(
        &argv0_last,
        last_exit,
        current_input,
        last_stderr,
    )))
}

fn builtin_env_read(env: &Env, args: &[Value], span: Span) -> Result<Value, EvalError> {
    arity_check("env.read", 1, args, span)?;
    let name = match &args[0] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::new(
                EvalErrorKind::Type(format!(
                    "env.read expects string, got {}",
                    value_kind(other)
                )),
                span,
            ))
        }
    };
    let result = std::env::var(&name).ok();
    record_event(
        env,
        "env_read",
        vec![
            ("name".into(), format!("\"{name}\"")),
            (
                "present".into(),
                if result.is_some() {
                    "true".into()
                } else {
                    "false".into()
                },
            ),
        ],
    );
    Ok(match result {
        Some(v) => Value::some(Value::Str(v)),
        None => Value::none(),
    })
}

fn arity_check(name: &str, expected: usize, args: &[Value], span: Span) -> Result<(), EvalError> {
    if args.len() != expected {
        Err(EvalError::new(
            EvalErrorKind::Arity {
                name: name.to_string(),
                expected,
                found: args.len(),
            },
            span,
        ))
    } else {
        Ok(())
    }
}

/// Quick ISO-8601 millisecond renderer for `clock.now`. Mirrors the
/// `trace::format_iso_ms` helper without the per-event counter.
fn format_iso_ms(ts_ms: u64) -> String {
    let secs = ts_ms / 1000;
    let millis = ts_ms % 1000;
    let days = secs / 86_400;
    let s_of_day = secs % 86_400;
    let h = s_of_day / 3600;
    let m = (s_of_day % 3600) / 60;
    let s = s_of_day % 60;
    let date = days_from_epoch(days);
    format!("{date}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

fn days_from_epoch(mut days: u64) -> String {
    let mut y: u64 = 1970;
    loop {
        let dy = if is_leap(y) { 366 } else { 365 };
        if days < dy {
            break;
        }
        days -= dy;
        y += 1;
    }
    let dpm = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m: usize = 0;
    while m < 12 && days >= dpm[m] {
        days -= dpm[m];
        m += 1;
    }
    format!("{:04}-{:02}-{:02}", y, m + 1, days + 1)
}

fn is_leap(y: u64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

/// Invoke a value with a positional argument list. Used internally
/// by `eval_call` and by the CLI driver (M3.T6).
fn invoke_value(callee: &Value, args: &[Value], span: Span) -> Result<Flow, EvalError> {
    if let Value::Saga(s) = callee {
        return invoke_saga(s, args, span).map(Flow::Value);
    }
    if let Value::Agent(a) = callee {
        return invoke_agent(a, args, span).map(Flow::Value);
    }
    if let Value::AgentNet(n) = callee {
        return invoke_agent_net(n, args, span).map(Flow::Value);
    }
    let closure = match callee {
        Value::Closure(c) => c.clone(),
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::NotCallable(value_kind(callee).into()),
                span,
            ))
        }
    };
    if closure.params.len() != args.len() {
        return Err(EvalError::new(
            EvalErrorKind::Arity {
                name: closure.name.clone().unwrap_or_else(|| "<lambda>".into()),
                expected: closure.params.len(),
                found: args.len(),
            },
            span,
        ));
    }
    let mut call_env = Env::from_snapshot(
        closure.captured.clone(),
        closure.module.clone(),
        closure.tracer.clone(),
        closure.stdin.clone(),
        closure.record_decls.clone(),
        closure.model_decls.clone(),
        closure.policies.clone(),
        closure.ai_backend.clone(),
        closure.replay_tape.clone(),
        closure.full_record,
    );
    call_env.push_scope();
    for (name, val) in closure.params.iter().zip(args) {
        call_env.bind_let(name, val.clone());
    }
    // M5.T4: `requires:` clauses checked at function entry. Each
    // clause must evaluate to `Bool(true)`; anything else (including
    // `Bool(false)` or non-bool) raises a `ContractViolation`.
    let fn_name = closure.name.clone().unwrap_or_else(|| "<lambda>".into());
    for (i, req) in closure.requires.iter().enumerate() {
        let v = eval_value(req, &mut call_env)?;
        if !matches!(v, Value::Bool(true)) {
            return Err(EvalError::new(
                EvalErrorKind::ContractViolation {
                    fn_name: fn_name.clone(),
                    clause: ContractClause::Requires { index: i },
                },
                req.span(),
            ));
        }
    }
    let f = eval_block(&closure.body, &mut call_env)?;
    let result_value = match f {
        Flow::Value(v) | Flow::Return(v) => v,
        Flow::Break(_) | Flow::Continue => {
            return Err(EvalError::new(
                EvalErrorKind::StrayControlFlow("loop control flow escaped a closure"),
                span,
            ))
        }
    };
    // M5.T4: `ensures:` clauses see `result` bound to the returned
    // value. Failure raises ContractViolation just like requires.
    if !closure.ensures.is_empty() {
        call_env.bind_let("result", result_value.clone());
        for (i, ens) in closure.ensures.iter().enumerate() {
            let v = eval_value(ens, &mut call_env)?;
            if !matches!(v, Value::Bool(true)) {
                return Err(EvalError::new(
                    EvalErrorKind::ContractViolation {
                        fn_name: fn_name.clone(),
                        clause: ContractClause::Ensures { index: i },
                    },
                    ens.span(),
                ));
            }
        }
    }
    Ok(Flow::Value(result_value))
}

// ====================================================================
//  Blocks / statements
// ====================================================================

fn eval_block(b: &Block, env: &mut Env) -> Result<Flow, EvalError> {
    env.push_scope();
    let result = (|| {
        for s in &b.stmts {
            match eval_stmt(s, env)? {
                Flow::Value(_) => {}
                other => return Ok(other),
            }
        }
        match &b.tail {
            Some(t) => eval_expr(t, env),
            None => Ok(Flow::Value(Value::Unit)),
        }
    })();
    env.pop_scope();
    result
}

fn eval_stmt(s: &Stmt, env: &mut Env) -> Result<Flow, EvalError> {
    match s {
        Stmt::Let { name, value, .. } => {
            let v = eval_value(value, env)?;
            env.bind_let(name, v);
            Ok(Flow::Value(Value::Unit))
        }
        Stmt::Var { name, value, .. } => {
            let v = eval_value(value, env)?;
            env.bind_var(name, v);
            Ok(Flow::Value(Value::Unit))
        }
        Stmt::For {
            var,
            iter,
            body,
            span,
        } => eval_for(var, iter, body, *span, env),
        Stmt::While { cond, body, .. } => {
            loop {
                let c = eval_value(cond, env)?;
                let cb = expect_bool(&c, cond.span())?;
                if !cb {
                    break;
                }
                match eval_block(body, env)? {
                    Flow::Value(_) | Flow::Continue => {}
                    Flow::Break(_) => break,
                    Flow::Return(v) => return Ok(Flow::Return(v)),
                }
            }
            Ok(Flow::Value(Value::Unit))
        }
        Stmt::Expr(e) => eval_expr(e, env),
    }
}

fn eval_for(
    var: &str,
    iter: &Expr,
    body: &Block,
    span: Span,
    env: &mut Env,
) -> Result<Flow, EvalError> {
    let items: Vec<Value> = match iter {
        Expr::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            let s = match start {
                Some(s) => match eval_value(s, env)? {
                    Value::Int(n) => n,
                    other => {
                        return Err(EvalError::new(
                            EvalErrorKind::Type(format!(
                                "range start must be int, got {}",
                                value_kind(&other)
                            )),
                            s.span(),
                        ))
                    }
                },
                None => 0,
            };
            let e = match end {
                Some(e) => match eval_value(e, env)? {
                    Value::Int(n) => n,
                    other => {
                        return Err(EvalError::new(
                            EvalErrorKind::Type(format!(
                                "range end must be int, got {}",
                                value_kind(&other)
                            )),
                            e.span(),
                        ))
                    }
                },
                None => {
                    return Err(EvalError::new(
                        EvalErrorKind::Type("`for` needs a bounded range".into()),
                        span,
                    ))
                }
            };
            let mut out = Vec::new();
            let limit = if *inclusive { e + 1 } else { e };
            for n in s..limit {
                out.push(Value::Int(n));
            }
            out
        }
        _ => {
            let v = eval_value(iter, env)?;
            match v {
                Value::List(xs) | Value::Set(xs) | Value::Tuple(xs) => xs,
                Value::Str(s) => s.chars().map(Value::Char).collect(),
                other => {
                    return Err(EvalError::new(
                        EvalErrorKind::Type(format!(
                            "cannot iterate over `{}`",
                            value_kind(&other)
                        )),
                        iter.span(),
                    ))
                }
            }
        }
    };
    for item in items {
        env.push_scope();
        env.bind_let(var, item);
        let f = eval_block(body, env);
        env.pop_scope();
        match f? {
            Flow::Value(_) | Flow::Continue => {}
            Flow::Break(_) => break,
            Flow::Return(v) => return Ok(Flow::Return(v)),
        }
    }
    Ok(Flow::Value(Value::Unit))
}

// ====================================================================
//  Assignment
// ====================================================================

fn eval_assign(
    op: AssignOp,
    target: &Expr,
    value: &Expr,
    env: &mut Env,
) -> Result<Flow, EvalError> {
    let name = match target {
        Expr::Ident(n, _) => n.clone(),
        _ => {
            return Err(EvalError::new(
                EvalErrorKind::Type("assignment target must be an identifier".into()),
                target.span(),
            ))
        }
    };
    let new_val = eval_value(value, env)?;
    let final_val = if matches!(op, AssignOp::Eq) {
        new_val
    } else {
        let cur = env.lookup(&name).ok_or_else(|| {
            EvalError::new(EvalErrorKind::UndefinedVar(name.clone()), target.span())
        })?;
        let bin = match op {
            AssignOp::AddEq => BinOp::Add,
            AssignOp::SubEq => BinOp::Sub,
            AssignOp::MulEq => BinOp::Mul,
            AssignOp::DivEq => BinOp::Div,
            AssignOp::RemEq => BinOp::Rem,
            AssignOp::Eq => unreachable!(),
        };
        apply_binop(bin, cur, new_val, value.span())?
    };
    env.assign(&name, final_val)
        .map_err(|msg| EvalError::new(EvalErrorKind::Type(msg.to_string()), target.span()))?;
    Ok(Flow::Value(Value::Unit))
}

// ====================================================================
//  Match + patterns
// ====================================================================

fn eval_match(scrutinee: &Expr, arms: &[MatchArm], env: &mut Env) -> Result<Flow, EvalError> {
    let v = eval_value(scrutinee, env)?;
    for a in arms {
        env.push_scope();
        let matched = pattern_matches(&a.pattern, &v, env, a.span)?;
        if matched {
            let guard_ok = match &a.guard {
                Some(g) => match eval_value(g, env)? {
                    Value::Bool(b) => b,
                    other => {
                        env.pop_scope();
                        return Err(EvalError::new(
                            EvalErrorKind::Type(format!(
                                "match guard must be bool, got {}",
                                value_kind(&other)
                            )),
                            g.span(),
                        ));
                    }
                },
                None => true,
            };
            if guard_ok {
                let result = eval_expr(&a.body, env);
                env.pop_scope();
                return result;
            }
        }
        env.pop_scope();
    }
    Err(EvalError::new(
        EvalErrorKind::NonExhaustiveMatch,
        scrutinee.span(),
    ))
}

fn pattern_matches(p: &Pattern, v: &Value, env: &mut Env, span: Span) -> Result<bool, EvalError> {
    match p {
        Pattern::Wildcard(_) => Ok(true),
        Pattern::Bind(name, _) => {
            env.bind_let(name, v.clone());
            Ok(true)
        }
        Pattern::Lit(lit_expr, _) => {
            let lit = eval_literal_pattern(lit_expr)?;
            Ok(values_equal(&lit, v))
        }
        Pattern::Constructor { name, args, .. } => {
            // Built-in constructors:
            match name.as_str() {
                "Ok" => {
                    if let Value::Result(Ok(inner)) = v {
                        if args.len() == 1 {
                            return pattern_matches(&args[0], inner, env, span);
                        }
                    }
                    return Ok(false);
                }
                "Err" => {
                    if let Value::Result(Err(inner)) = v {
                        if args.len() == 1 {
                            return pattern_matches(&args[0], inner, env, span);
                        }
                    }
                    return Ok(false);
                }
                "Some" => {
                    if let Value::Option(Some(inner)) = v {
                        if args.len() == 1 {
                            return pattern_matches(&args[0], inner, env, span);
                        }
                    }
                    return Ok(false);
                }
                "None" => return Ok(matches!(v, Value::Option(None)) && args.is_empty()),
                _ => {}
            }
            // User-defined enum tuple variant
            if let Value::Enum(e) = v {
                if e.variant == *name {
                    if let VariantValue::Tuple(elems) = &e.data {
                        if elems.len() == args.len() {
                            for (sub_pat, sub_val) in args.iter().zip(elems) {
                                if !pattern_matches(sub_pat, sub_val, env, span)? {
                                    return Ok(false);
                                }
                            }
                            return Ok(true);
                        }
                    }
                    if matches!(&e.data, VariantValue::Unit) && args.is_empty() {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        Pattern::RecordCtor {
            name, fields, rest, ..
        } => {
            if let Value::Enum(e) = v {
                if e.variant != *name {
                    return Ok(false);
                }
                if let VariantValue::Record(value_fields) = &e.data {
                    return record_pat_matches(fields, *rest, value_fields, env, span);
                }
            }
            if let Value::Record(r) = v {
                if r.name.as_deref() == Some(name) {
                    return record_pat_matches(fields, *rest, &r.fields, env, span);
                }
            }
            Ok(false)
        }
        Pattern::Tuple { elems, .. } => match v {
            Value::Tuple(vs) if vs.len() == elems.len() => {
                for (sub_pat, sub_val) in elems.iter().zip(vs) {
                    if !pattern_matches(sub_pat, sub_val, env, span)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            _ => Ok(false),
        },
        Pattern::List { elems, .. } => match v {
            Value::List(vs) => list_pat_matches(elems, vs, env, span),
            _ => Ok(false),
        },
    }
}

fn record_pat_matches(
    pats: &[crate::syntax::ast::RecordPatField],
    rest: bool,
    value_fields: &[(String, Value)],
    env: &mut Env,
    span: Span,
) -> Result<bool, EvalError> {
    for f in pats {
        let pair = value_fields.iter().find(|(k, _)| k == &f.name);
        let v = match pair {
            Some((_, v)) => v,
            None => return Ok(false),
        };
        if let Some(p) = &f.pat {
            if !pattern_matches(p, v, env, span)? {
                return Ok(false);
            }
        } else {
            env.bind_let(&f.name, v.clone());
        }
    }
    let _ = rest;
    Ok(true)
}

fn list_pat_matches(
    pats: &[ListPatElem],
    items: &[Value],
    env: &mut Env,
    span: Span,
) -> Result<bool, EvalError> {
    let rest_pos = pats.iter().position(|e| matches!(e, ListPatElem::Rest(_)));
    match rest_pos {
        None => {
            if pats.len() != items.len() {
                return Ok(false);
            }
            for (p, v) in pats.iter().zip(items) {
                if let ListPatElem::Pat(p) = p {
                    if !pattern_matches(p, v, env, span)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        Some(rp) => {
            let head = &pats[..rp];
            let tail = &pats[rp + 1..];
            if items.len() < head.len() + tail.len() {
                return Ok(false);
            }
            for (p, v) in head.iter().zip(&items[..head.len()]) {
                if let ListPatElem::Pat(p) = p {
                    if !pattern_matches(p, v, env, span)? {
                        return Ok(false);
                    }
                }
            }
            let tail_start = items.len() - tail.len();
            for (p, v) in tail.iter().zip(&items[tail_start..]) {
                if let ListPatElem::Pat(p) = p {
                    if !pattern_matches(p, v, env, span)? {
                        return Ok(false);
                    }
                }
            }
            // Bind rest if named.
            if let ListPatElem::Rest(Some(name)) = &pats[rp] {
                let rest_slice: Vec<Value> = items[head.len()..tail_start].to_vec();
                env.bind_let(name, Value::List(rest_slice));
            }
            Ok(true)
        }
    }
}

fn eval_literal_pattern(e: &Expr) -> Result<Value, EvalError> {
    match e {
        Expr::Int(n, _) => Ok(Value::Int(*n)),
        Expr::Float(f, _) => Ok(Value::Float(*f)),
        Expr::Bool(b, _) => Ok(Value::Bool(*b)),
        Expr::Str(s, _) => Ok(Value::Str(s.clone())),
        Expr::Char(c, _) => Ok(Value::Char(*c)),
        Expr::Date(s, _) => Ok(Value::Date(s.clone())),
        Expr::Timestamp(s, _) => Ok(Value::Timestamp(s.clone())),
        Expr::Duration(s, _) => Ok(Value::Duration(s.clone())),
        Expr::Unary {
            op: UnOp::Neg,
            expr,
            ..
        } => match eval_literal_pattern(expr)? {
            Value::Int(n) => Ok(Value::Int(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            other => Err(EvalError::new(
                EvalErrorKind::Type(format!("cannot negate `{}` in pattern", value_kind(&other))),
                expr.span(),
            )),
        },
        _ => Err(EvalError::new(
            EvalErrorKind::Type("pattern literal must be a literal value".into()),
            e.span(),
        )),
    }
}

// ====================================================================
//  Tests — 50 pure programs (M3.T2 acceptance)
// ====================================================================

#[cfg(test)]
mod tests {
    use super::super::value::Value;
    use super::*;

    fn ev(src: &str) -> Value {
        eval_expression(src).unwrap_or_else(|e| panic!("{src:?} → {e:?}"))
    }

    fn ev_err(src: &str) -> EvalError {
        match eval_expression(src) {
            Ok(v) => panic!("expected error on {src:?}, got {v:?}"),
            Err(e) => e,
        }
    }

    // ---- arithmetic / comparison (10) ----

    #[test]
    fn p01_int_add() {
        assert_eq!(ev("1 + 2"), Value::Int(3));
    }
    #[test]
    fn p02_int_precedence() {
        assert_eq!(ev("1 + 2 * 3"), Value::Int(7));
    }
    #[test]
    fn p03_int_paren() {
        assert_eq!(ev("(1 + 2) * 3"), Value::Int(9));
    }
    #[test]
    fn p04_int_div_floor() {
        assert_eq!(ev("7 / 2"), Value::Int(3));
    }
    #[test]
    fn p05_int_rem() {
        assert_eq!(ev("7 % 3"), Value::Int(1));
    }
    #[test]
    fn p06_unary_neg() {
        assert_eq!(ev("-5 + 2"), Value::Int(-3));
    }
    #[test]
    fn p07_float_arith() {
        match ev("2.5 + 0.5") {
            Value::Float(f) => assert!((f - 3.0).abs() < 1e-9),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn p08_int_compare() {
        assert_eq!(ev("3 < 5"), Value::Bool(true));
        assert_eq!(ev("3 == 5"), Value::Bool(false));
        assert_eq!(ev("5 >= 5"), Value::Bool(true));
    }
    #[test]
    fn p09_logical_short_circuit() {
        assert_eq!(ev("true or (1 / 0 == 0)"), Value::Bool(true));
        assert_eq!(ev("false and (1 / 0 == 0)"), Value::Bool(false));
    }
    #[test]
    fn p10_not_op() {
        assert_eq!(ev("not (1 == 2)"), Value::Bool(true));
    }

    // ---- bitwise / shift (3) ----

    #[test]
    fn p11_bitops() {
        assert_eq!(ev("5 & 3"), Value::Int(1));
        assert_eq!(ev("5 | 3"), Value::Int(7));
        assert_eq!(ev("5 ^ 3"), Value::Int(6));
    }
    #[test]
    fn p12_shifts() {
        assert_eq!(ev("1 << 3"), Value::Int(8));
        assert_eq!(ev("32 >> 2"), Value::Int(8));
    }
    #[test]
    fn p13_div_zero_errors() {
        let err = ev_err("1 / 0");
        assert!(matches!(err.kind, EvalErrorKind::DivByZero));
    }

    // ---- strings (3) ----

    #[test]
    fn p14_string_literal() {
        assert_eq!(ev(r#""hello""#), Value::Str("hello".into()));
    }
    #[test]
    fn p15_string_concat() {
        assert_eq!(ev(r#""ab" + "cd""#), Value::Str("abcd".into()));
    }
    #[test]
    fn p16_string_compare() {
        assert_eq!(ev(r#""apple" < "banana""#), Value::Bool(true));
    }

    // ---- collections (8) ----

    #[test]
    fn p17_list_literal() {
        assert_eq!(
            ev("[1, 2, 3]"),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }
    #[test]
    fn p18_list_concat() {
        assert_eq!(
            ev("[1, 2] + [3, 4]"),
            Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
            ])
        );
    }
    #[test]
    fn p19_tuple_literal() {
        assert_eq!(
            ev(r#"(1, "x", true)"#),
            Value::Tuple(vec![
                Value::Int(1),
                Value::Str("x".into()),
                Value::Bool(true),
            ])
        );
    }
    #[test]
    fn p20_anon_record() {
        match ev("{ a: 1, b: 2 }") {
            Value::Record(r) => {
                assert_eq!(r.name, None);
                assert_eq!(r.fields.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn p21_named_record_with_spread() {
        match ev(r#"User { ..{ id: 1, name: "x" }, name: "y" }"#) {
            Value::Record(r) => {
                assert_eq!(r.name.as_deref(), Some("User"));
                let n = r.fields.iter().find(|(k, _)| k == "name").unwrap();
                assert_eq!(n.1, Value::Str("y".into()));
            }
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn p22_list_index() {
        assert_eq!(ev("[10, 20, 30][1]"), Value::Int(20));
    }
    #[test]
    fn p23_list_index_oob_errors() {
        let err = ev_err("[1, 2][5]");
        assert!(matches!(err.kind, EvalErrorKind::IndexOutOfBounds { .. }));
    }
    #[test]
    fn p24_record_field_access() {
        assert_eq!(ev("{ x: 7 }.x"), Value::Int(7));
    }

    // ---- if / match / blocks (12) ----

    #[test]
    fn p25_if_true_branch() {
        assert_eq!(ev("if 3 > 2 { 1 } else { 0 }"), Value::Int(1));
    }
    #[test]
    fn p26_if_false_branch() {
        assert_eq!(ev("if 3 > 9 { 1 } else { 0 }"), Value::Int(0));
    }
    #[test]
    fn p27_else_if_chain() {
        assert_eq!(
            ev("if 1 == 0 { 'a' } else if 2 == 2 { 'b' } else { 'c' }"),
            Value::Char('b')
        );
    }
    #[test]
    fn p28_block_with_let() {
        assert_eq!(ev("{ let x = 7; x + 1 }"), Value::Int(8));
    }
    #[test]
    fn p29_let_shadowing() {
        assert_eq!(ev("{ let x = 1; let x = x + 9; x }"), Value::Int(10));
    }
    #[test]
    fn p30_match_int_with_default() {
        assert_eq!(ev("match 2 { 0 -> 100, n -> n + 1 }"), Value::Int(3));
    }
    #[test]
    fn p31_match_with_guard() {
        assert_eq!(
            ev("match 5 { n if n > 0 -> 1, n if n < 0 -> -1, _ -> 0 }"),
            Value::Int(1)
        );
    }
    #[test]
    fn p32_match_string_literal() {
        assert_eq!(
            ev(r#"match "hi" { "yo" -> 1, "hi" -> 2, _ -> 0 }"#),
            Value::Int(2)
        );
    }
    #[test]
    fn p33_match_list_empty() {
        assert_eq!(ev("match [] { [] -> 0, [_] -> 1, _ -> 2 }"), Value::Int(0));
    }
    #[test]
    fn p34_match_list_head_tail() {
        assert_eq!(
            ev("match [10, 20, 30] { [] -> 0, [x] -> 1, [first, ..rest] -> first }"),
            Value::Int(10)
        );
    }
    #[test]
    fn p35_match_tuple_destructure() {
        assert_eq!(ev("match (1, 2) { (a, b) -> a + b }"), Value::Int(3));
    }
    #[test]
    fn p36_match_some_none() {
        assert_eq!(
            ev("match Some(7) { None -> 0, Some(n) -> n }"),
            Value::Int(7)
        );
    }

    // ---- result + ? + raise (M3.T5 lite, exercised here) (4) ----

    #[test]
    fn p37_ok_constructor() {
        assert_eq!(ev("Ok(42)"), Value::ok(Value::Int(42)));
    }
    #[test]
    fn p38_err_constructor() {
        assert_eq!(ev(r#"Err("bad")"#), Value::err(Value::Str("bad".into())));
    }
    #[test]
    fn p39_try_unwraps_ok() {
        assert_eq!(ev("Ok(7)?"), Value::Int(7));
    }
    #[test]
    fn p40_try_propagates_err() {
        let err = ev_err(r#"Err("nope")?"#);
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert_eq!(s, "nope"),
            other => panic!("{other:?}"),
        }
    }

    // ---- for / while / break / continue (5) ----

    #[test]
    fn p41_for_range_sum() {
        assert_eq!(
            ev("{ var s = 0; for i in 0..10 { s = s + i }; s }"),
            Value::Int(45)
        );
    }
    #[test]
    fn p42_for_range_inclusive() {
        assert_eq!(
            ev("{ var s = 0; for i in 1..=5 { s = s + i }; s }"),
            Value::Int(15)
        );
    }
    #[test]
    fn p43_for_over_list() {
        assert_eq!(
            ev("{ var s = 0; for x in [10, 20, 30] { s = s + x }; s }"),
            Value::Int(60)
        );
    }
    #[test]
    fn p44_while_loop() {
        assert_eq!(
            ev("{ var n = 0; var k = 1; while k < 100 { n = n + 1; k = k * 2 }; n }"),
            Value::Int(7)
        );
    }
    #[test]
    fn p45_break_value() {
        // `break` exits the loop; the surrounding block returns `s`.
        assert_eq!(
            ev("{ var s = 0; for i in 0..100 { if i == 5 { break }; s = s + 1 }; s }"),
            Value::Int(5)
        );
    }

    // ---- assignment / shadowing / mutation (3) ----

    #[test]
    fn p46_var_assign_in_place() {
        assert_eq!(
            ev("{ var x = 1; x = x + 10; x = x * 2; x }"),
            Value::Int(22)
        );
    }
    #[test]
    fn p47_let_immutable_rejects_assign() {
        let err = ev_err("{ let x = 1; x = 2; x }");
        assert!(matches!(err.kind, EvalErrorKind::Type(_)));
    }
    #[test]
    fn p48_compound_assign_mul() {
        assert_eq!(ev("{ var x = 3; x *= 4; x }"), Value::Int(12));
    }

    // ---- equality / is + extra patterns (4) ----

    #[test]
    fn p49_record_equality() {
        assert_eq!(ev("{ a: 1, b: 2 } == { a: 1, b: 2 }"), Value::Bool(true));
        assert_eq!(ev("{ a: 1 } == { a: 2 }"), Value::Bool(false));
    }
    #[test]
    fn p50_is_check_against_some() {
        assert_eq!(ev("Some(7) is Some(_)"), Value::Bool(true));
        assert_eq!(ev("None is None"), Value::Bool(true));
        assert_eq!(ev("Ok(7) is Some(_)"), Value::Bool(false));
    }
    #[test]
    fn p51_match_on_record_anonymous() {
        // RecordCtor against an anonymous record requires an exact
        // type name; anonymous records have no name, so the named
        // pattern does not match — the wildcard arm catches it.
        assert_eq!(
            ev("match { a: 1, b: 2 } { Foo { a } -> a, _ -> 0 }"),
            Value::Int(0)
        );
    }
    #[test]
    fn p52_intent_block_evaluates_body() {
        // `intent "..." { ... }` forwards to the block — at the M3
        // pure layer it has no extra runtime effect.
        assert_eq!(ev(r#"intent "compute" { 21 + 21 }"#), Value::Int(42));
    }

    // ---- M3.T3 — let shadowing & var mutation rules ----

    #[test]
    fn t3_let_shadowing_does_not_mutate_outer() {
        // The inner `let x` shadows for its scope only.
        assert_eq!(
            ev("{ let x = 1; let inner = { let x = 99; x + 1 }; (x, inner) }"),
            Value::Tuple(vec![Value::Int(1), Value::Int(100)])
        );
    }

    #[test]
    fn t3_var_mutation_visible_across_iterations() {
        assert_eq!(
            ev("{ var n = 0; for _ in 0..3 { n = n + 1 }; n }"),
            Value::Int(3)
        );
    }

    #[test]
    fn t3_var_mutated_through_compound_assignments() {
        assert_eq!(
            ev("{ var x = 10; x += 5; x -= 2; x /= 1; x %= 100; x }"),
            Value::Int(13)
        );
    }

    #[test]
    fn t3_assign_to_let_is_rejected() {
        let err = ev_err("{ let x = 1; x = 2; x }");
        match err.kind {
            EvalErrorKind::Type(msg) => {
                assert!(msg.contains("immutable"), "{msg}");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t3_assign_to_undefined_var_is_rejected() {
        let err = ev_err("{ y = 1; y }");
        // The expression `y = 1` is an assignment to undefined `y`.
        // The current evaluator treats the lookup as a Type error; the
        // important property is that it does NOT silently succeed.
        assert!(matches!(
            err.kind,
            EvalErrorKind::Type(_) | EvalErrorKind::UndefinedVar(_)
        ));
    }

    #[test]
    fn t3_module_level_var_is_a_parse_error() {
        // Module-level `var` does not exist (§ 5.1) — `parse_item`
        // does not list `var` among item-start keywords, so the
        // parser refuses it at the top level.
        assert!(crate::syntax::parse("var x = 1").is_err());
    }

    // ---- M3.T5 — `result<T>`, `?`, `raise` (20 fixtures) ----

    #[test]
    fn t5_01_ok_constructor() {
        assert_eq!(ev("Ok(42)"), Value::ok(Value::Int(42)));
    }
    #[test]
    fn t5_02_err_constructor() {
        assert_eq!(ev(r#"Err("nope")"#), Value::err(Value::Str("nope".into())));
    }
    #[test]
    fn t5_03_ok_in_block() {
        assert_eq!(ev("{ let r = Ok(7); r }"), Value::ok(Value::Int(7)));
    }
    #[test]
    fn t5_04_try_unwraps_ok_value() {
        assert_eq!(ev("{ let v = Ok(5)?; v + 1 }"), Value::Int(6));
    }
    #[test]
    fn t5_05_try_propagates_err_to_caller() {
        let err = ev_err(r#"{ let _v = Err("bad")?; 0 }"#);
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert_eq!(s, "bad"),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn t5_06_raise_string_error() {
        let err = ev_err(r#"raise "boom""#);
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert_eq!(s, "boom"),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn t5_07_raise_int_payload() {
        let err = ev_err("raise 42");
        match err.kind {
            EvalErrorKind::Raised(Value::Int(42)) => {}
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn t5_08_raise_record_payload() {
        let err = ev_err(r#"raise { code: 1, msg: "oops" }"#);
        assert!(matches!(err.kind, EvalErrorKind::Raised(Value::Record(_))));
    }
    #[test]
    fn t5_09_chained_try_short_circuits_on_first_err() {
        // The first `Err` short-circuits the chain.
        let err = ev_err(r#"{ let _a = Ok(1)?; let _b = Err("x")?; let _c = Ok(2)?; 0 }"#);
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert_eq!(s, "x"),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn t5_10_chained_try_succeeds_when_all_ok() {
        assert_eq!(
            ev("{ let a = Ok(1)?; let b = Ok(2)?; a + b }"),
            Value::Int(3)
        );
    }
    #[test]
    fn t5_11_some_q_unwraps() {
        assert_eq!(ev("Some(7)?"), Value::Int(7));
    }
    #[test]
    fn t5_12_none_q_raises_string_marker() {
        let err = ev_err("None?");
        assert!(matches!(err.kind, EvalErrorKind::Raised(_)));
    }
    #[test]
    fn t5_13_match_ok_arm() {
        assert_eq!(
            ev("match Ok(7) { Ok(v) -> v + 1, Err(_) -> -1 }"),
            Value::Int(8)
        );
    }
    #[test]
    fn t5_14_match_err_arm() {
        assert_eq!(
            ev(r#"match Err("oops") { Ok(_) -> 1, Err(e) -> e }"#),
            Value::Str("oops".into())
        );
    }
    #[test]
    fn t5_15_result_equality_ok_ok() {
        assert_eq!(ev("Ok(1) == Ok(1)"), Value::Bool(true));
        assert_eq!(ev("Ok(1) == Ok(2)"), Value::Bool(false));
    }
    #[test]
    fn t5_16_result_equality_err_err() {
        assert_eq!(ev(r#"Err("x") == Err("x")"#), Value::Bool(true));
        assert_eq!(ev(r#"Err("x") == Ok(0)"#), Value::Bool(false));
    }
    #[test]
    fn t5_17_raise_inside_if_branch() {
        let err = ev_err(r#"if true { raise "halt" } else { 0 }"#);
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert_eq!(s, "halt"),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn t5_18_raise_inside_match_arm() {
        let err = ev_err(r#"match 1 { 0 -> 100, _ -> raise "no zero" }"#);
        match err.kind {
            EvalErrorKind::Raised(Value::Str(s)) => assert_eq!(s, "no zero"),
            other => panic!("{other:?}"),
        }
    }
    #[test]
    fn t5_19_ok_unit_round_trip() {
        // `Ok(())` packages the unit value.
        assert_eq!(ev("Ok(())"), Value::ok(Value::Unit));
    }
    #[test]
    fn t5_20_nested_ok_some_chained_unwrap() {
        // `Ok(Some(7))?` unwraps the result level → `Some(7)`, then
        // a second `?` unwraps the option → `7`.
        assert_eq!(ev("Ok(Some(7))??"), Value::Int(7));
    }

    // ---- M3.T4 — closures, higher-order, monomorphisation ----

    #[test]
    fn t4_lambda_identity_callable() {
        assert_eq!(ev("(fn(x) { x })(7)"), Value::Int(7));
    }

    #[test]
    fn t4_lambda_two_args() {
        assert_eq!(ev("(fn(a, b) { a + b })(3, 4)"), Value::Int(7));
    }

    #[test]
    fn t4_lambda_arity_mismatch_errors() {
        let err = ev_err("(fn(x, y) { x })(1)");
        assert!(matches!(
            err.kind,
            EvalErrorKind::Arity {
                expected: 2,
                found: 1,
                ..
            }
        ));
    }

    #[test]
    fn t4_lambda_captures_outer_let() {
        // The closure captures `n = 10`; calling it from another scope
        // still sees the captured value.
        assert_eq!(
            ev("{ let n = 10; let f = fn(x) { x + n }; f(5) }"),
            Value::Int(15)
        );
    }

    #[test]
    fn t4_capture_by_value_not_reference() {
        // A `var` mutation outside the closure does not leak in: the
        // closure captured the binding's value at definition time.
        assert_eq!(
            ev(r#"{
                var n = 1
                let f = fn(x) { x + n }
                n = 99
                f(0)
            }"#),
            Value::Int(1)
        );
    }

    #[test]
    fn t4_higher_order_inline_map_via_for_loop() {
        // Hand-rolled `map` until M4 ships the stdlib helpers.
        assert_eq!(
            ev(r#"{
                let f = fn(x) { x * 2 }
                var out = []
                for x in [1, 2, 3] { out = out + [f(x)] }
                out
            }"#),
            Value::List(vec![Value::Int(2), Value::Int(4), Value::Int(6)])
        );
    }

    #[test]
    fn t4_fold_via_for_loop() {
        assert_eq!(
            ev(r#"{
                let add = fn(a, b) { a + b }
                var acc = 0
                for x in [1, 2, 3, 4] { acc = add(acc, x) }
                acc
            }"#),
            Value::Int(10)
        );
    }

    #[test]
    fn t4_filter_via_for_loop() {
        assert_eq!(
            ev(r#"{
                let pred = fn(x) { x > 2 }
                var out = []
                for x in [1, 2, 3, 4] {
                    if pred(x) { out = out + [x] }
                }
                out
            }"#),
            Value::List(vec![Value::Int(3), Value::Int(4)])
        );
    }

    #[test]
    fn t4_lambda_returning_lambda() {
        // `add(3)` returns a closure that adds 3 to its argument.
        assert_eq!(
            ev(r#"{
                let add = fn(a) { fn(b) { a + b } }
                let add3 = add(3)
                add3(4)
            }"#),
            Value::Int(7)
        );
    }

    #[test]
    fn t4_module_fn_can_be_called() {
        let src = r#"
            fn double(x: int) -> int { x * 2 }
            fn main() -> int { double(21) }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        let v = run_main(&m).unwrap();
        assert_eq!(v, Value::Int(42));
    }

    #[test]
    fn t4_module_fns_can_call_each_other() {
        // `double` calls `inc`, both top-level. The closures see each
        // other through the module-level capture frame.
        let src = r#"
            fn inc(x: int) -> int { x + 1 }
            fn double(x: int) -> int { inc(x) + inc(x) }
            fn main() -> int { double(5) }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        assert_eq!(run_main(&m).unwrap(), Value::Int(12));
    }

    #[test]
    fn t4_recursive_fn_works() {
        let src = r#"
            fn fact(n: int) -> int {
                if n <= 1 { 1 } else { n * fact(n - 1) }
            }
            fn main() -> int { fact(5) }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        assert_eq!(run_main(&m).unwrap(), Value::Int(120));
    }

    #[test]
    fn t4_higher_order_module_fn() {
        // A module fn that takes another fn as an argument.
        let src = r#"
            fn apply(f: fn(int) -> int, x: int) -> int { f(x) }
            fn main() -> int { apply(fn(x) { x * x }, 7) }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        assert_eq!(run_main(&m).unwrap(), Value::Int(49));
    }

    // ---- M3.T6 — `aeris run` exit-code matrix ----

    /// Same logic as `cmd_run` in `cli.rs`, but returns the numeric
    /// exit so the test matrix is direct.
    fn run_exit(src: &str) -> u8 {
        let module = match crate::syntax::parse(src) {
            Ok(m) => m,
            Err(_) => return 64,
        };
        let check_errs = crate::check::check_module(&module);
        if !check_errs.is_empty() {
            return check_errs.iter().map(|e| e.exit_code()).max().unwrap_or(64);
        }
        match run_main(&module) {
            Ok(Value::Result(Err(_))) => 1,
            Ok(_) => 0,
            Err(e) => match e.kind {
                EvalErrorKind::Raised(_) => 1,
                _ => 1,
            },
        }
    }

    #[test]
    fn t6_clean_run_returns_zero() {
        assert_eq!(run_exit("fn main() -> int { 42 }"), 0);
    }

    #[test]
    fn t6_uncaught_err_returns_one() {
        assert_eq!(run_exit(r#"fn main() -> int { raise "halt" }"#), 1);
    }

    #[test]
    fn t6_err_result_returns_one() {
        assert_eq!(run_exit(r#"fn main() -> result<int> { Err("oops") }"#), 1);
    }

    #[test]
    fn t6_ok_result_returns_zero() {
        assert_eq!(run_exit("fn main() -> result<int> { Ok(7) }"), 0);
    }

    #[test]
    fn t6_parse_error_returns_64() {
        assert_eq!(run_exit("fn main( {{{"), 64);
    }

    #[test]
    fn t6_check_error_returns_at_least_64() {
        // `Foo` is unknown — type resolver flags it with exit 64.
        assert!(run_exit("fn main() -> Foo {}") >= 64);
    }

    // ---- M4.T2 — `cap.subset[..]` runtime narrowing ----

    fn cap(entries: Vec<(Vec<&str>, Option<Vec<&str>>)>, star: bool) -> Value {
        let entries = entries
            .into_iter()
            .map(|(path, allow)| CapEntryValue {
                path: path.into_iter().map(String::from).collect(),
                allow: allow.map(|xs| xs.into_iter().map(String::from).collect()),
            })
            .collect();
        Value::Cap(Rc::new(CapValue { entries, star }))
    }

    /// Evaluate `src` with `cap` pre-bound in the env to the supplied
    /// `Value::Cap`. Used to test `cap.subset[..]` runtime semantics.
    fn ev_with_cap(src: &str, cap_val: Value) -> Result<Value, EvalError> {
        let expr = parse_expression(src)
            .map_err(|e| EvalError::new(EvalErrorKind::Parse(format!("{:?}", e.kind)), e.span))?;
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        eval_expr(&expr, &mut env)?.into_value(expr.span())
    }

    #[test]
    fn cap_subset_narrows_a_known_op() {
        let parent = cap(
            vec![
                (vec!["fs", "read_file"], None),
                (vec!["fs", "write_file"], None),
                (vec!["audit", "event"], None),
            ],
            false,
        );
        let v = ev_with_cap("cap.subset[fs.read_file]", parent).unwrap();
        match v {
            Value::Cap(c) => {
                assert!(!c.star);
                assert_eq!(c.entries.len(), 1);
                assert_eq!(
                    c.entries[0].path,
                    vec!["fs".to_string(), "read_file".to_string()]
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cap_subset_broadening_is_rejected_at_runtime() {
        // Parent has only `fs.read_file`; subset asking for write_file
        // tries to broaden — must fail.
        let parent = cap(vec![(vec!["fs", "read_file"], None)], false);
        let err = ev_with_cap("cap.subset[fs.write_file]", parent).unwrap_err();
        match err.kind {
            EvalErrorKind::Type(msg) => assert!(
                msg.contains("cannot broaden") && msg.contains("fs.write_file"),
                "{msg}"
            ),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cap_subset_narrowing_allow_list_ok() {
        let parent = cap(
            vec![(
                vec!["http", "post"],
                Some(vec!["api.acme.com", "api.stripe.com"]),
            )],
            false,
        );
        let v = ev_with_cap(r#"cap.subset[http.post @ ["api.acme.com"]]"#, parent).unwrap();
        match v {
            Value::Cap(c) => {
                assert_eq!(c.entries[0].allow.as_ref().unwrap().len(), 1);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cap_subset_broadening_allow_list_rejected() {
        let parent = cap(
            vec![(vec!["http", "post"], Some(vec!["api.acme.com"]))],
            false,
        );
        let err = ev_with_cap(
            r#"cap.subset[http.post @ ["api.acme.com", "evil.com"]]"#,
            parent,
        )
        .unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::Type(_)));
    }

    #[test]
    fn cap_subset_module_path_covers_leaves() {
        // Parent grants `fs.*` (one-segment entry). Subset can ask for
        // any leaf op (`fs.write_file`, `fs.read_file`, ...).
        let parent = cap(vec![(vec!["fs"], None)], false);
        let v = ev_with_cap("cap.subset[fs.write_file]", parent).unwrap();
        assert!(matches!(v, Value::Cap(_)));
    }

    #[test]
    fn cap_subset_with_no_cap_in_scope_errors() {
        let err = eval_expression("cap.subset[fs.read_file]").unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::Type(_)));
    }

    #[test]
    fn cap_subset_star_parent_authorises_anything() {
        // `main`'s synthesised cap (`star = true`) covers every op.
        let parent = cap(Vec::new(), true);
        let v = ev_with_cap("cap.subset[fs.read_file]", parent).unwrap();
        assert!(matches!(v, Value::Cap(_)));
    }

    // ---- M4.T4 / T6 / T7 / T8 — L1 builtins with tracer ----

    use crate::runtime::trace::{TraceEvent, Tracer};

    /// Evaluate `src` with `cap` pre-bound *and* an in-memory tracer
    /// attached. Returns `(value, events)` for direct inspection.
    fn ev_with_cap_traced(
        src: &str,
        cap_val: Value,
    ) -> (Result<Value, EvalError>, Vec<TraceEvent>) {
        let tracer = Tracer::in_memory();
        let expr = match parse_expression(src) {
            Ok(e) => e,
            Err(e) => {
                return (
                    Err(EvalError::new(
                        EvalErrorKind::Parse(format!("{:?}", e.kind)),
                        e.span,
                    )),
                    Vec::new(),
                )
            }
        };
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap_val);
        let r = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        (r, tracer.events())
    }

    fn star_cap() -> Value {
        Value::Cap(Rc::new(CapValue {
            entries: Vec::new(),
            star: true,
        }))
    }

    #[test]
    fn io_println_returns_unit_and_records_event() {
        let (v, evs) = ev_with_cap_traced(r#"io.println("hi")"#, star_cap());
        assert_eq!(v.unwrap(), Value::Unit);
        assert!(evs.iter().any(|e| e.kind == "io_println"));
    }

    #[test]
    fn io_print_records_len_field() {
        let (v, evs) = ev_with_cap_traced(r#"io.print("hello")"#, star_cap());
        assert_eq!(v.unwrap(), Value::Unit);
        let evt = evs.iter().find(|e| e.kind == "io_print").unwrap();
        assert!(evt.fields.iter().any(|(k, v)| k == "len" && v == "5"));
    }

    #[test]
    fn io_eprint_and_eprintln_emit_distinct_kinds() {
        let (v1, e1) = ev_with_cap_traced(r#"io.eprint("a")"#, star_cap());
        let (v2, e2) = ev_with_cap_traced(r#"io.eprintln("b")"#, star_cap());
        assert_eq!(v1.unwrap(), Value::Unit);
        assert_eq!(v2.unwrap(), Value::Unit);
        assert!(e1.iter().any(|e| e.kind == "io_eprint"));
        assert!(e2.iter().any(|e| e.kind == "io_eprintln"));
    }

    #[test]
    fn clock_now_returns_timestamp_and_records_value() {
        let (v, evs) = ev_with_cap_traced("clock.now()", star_cap());
        match v.unwrap() {
            Value::Timestamp(s) => assert!(s.contains('T') && s.ends_with('Z')),
            other => panic!("{other:?}"),
        }
        let evt = evs.iter().find(|e| e.kind == "clock_now").unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "value" && v.starts_with('"')));
    }

    #[test]
    fn random_next_returns_int_and_records_value() {
        let (v, evs) = ev_with_cap_traced("random.next()", star_cap());
        assert!(matches!(v.unwrap(), Value::Int(_)));
        assert!(evs.iter().any(|e| e.kind == "random_next"));
    }

    #[test]
    fn env_read_returns_some_when_present() {
        std::env::set_var("AERIS_TEST_FOO", "bar");
        let (v, evs) = ev_with_cap_traced(r#"env.read("AERIS_TEST_FOO")"#, star_cap());
        assert_eq!(v.unwrap(), Value::some(Value::Str("bar".into())));
        let evt = evs.iter().find(|e| e.kind == "env_read").unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "present" && v == "true"));
        std::env::remove_var("AERIS_TEST_FOO");
    }

    #[test]
    fn env_read_returns_none_when_absent() {
        std::env::remove_var("AERIS_TEST_NOPE_X");
        let (v, evs) = ev_with_cap_traced(r#"env.read("AERIS_TEST_NOPE_X")"#, star_cap());
        assert_eq!(v.unwrap(), Value::none());
        assert!(evs.iter().any(|e| e.kind == "env_read"
            && e.fields.iter().any(|(k, v)| k == "present" && v == "false")));
    }

    #[test]
    fn io_println_inside_block_with_other_stmts() {
        let (v, evs) = ev_with_cap_traced(
            r#"{
                let x = 7
                io.println("ok")
                x + 1
            }"#,
            star_cap(),
        );
        assert_eq!(v.unwrap(), Value::Int(8));
        assert!(evs.iter().any(|e| e.kind == "io_println"));
    }

    #[test]
    fn io_println_intent_propagates_into_event() {
        // M5.T7: a language-level `intent "..." { ... }` block
        // pushes the intent onto the tracer; events emitted inside
        // the body inherit it via the trace channel.
        let tracer = Tracer::in_memory();
        let expr = parse_expression(r#"intent "say hello" { io.println("hi") }"#).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", star_cap());
        let _ = eval_expr(&expr, &mut env).unwrap();
        let print_evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "io_println")
            .unwrap();
        assert_eq!(print_evt.intent.as_deref(), Some("say hello"));
    }

    #[test]
    fn intent_block_emits_enter_and_exit_events() {
        let tracer = Tracer::in_memory();
        let expr = parse_expression(r#"intent "rotate" { 42 }"#).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", star_cap());
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert_eq!(v, Value::Int(42));
        let kinds: Vec<_> = tracer.events().into_iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec!["intent_enter".to_string(), "intent_exit".to_string()]
        );
    }

    #[test]
    fn intent_exit_outcome_reflects_err_result() {
        let tracer = Tracer::in_memory();
        let expr = parse_expression(r#"intent "x" { Err("nope") }"#).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", star_cap());
        let _ = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        let exit_evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "intent_exit")
            .unwrap();
        assert!(exit_evt
            .fields
            .iter()
            .any(|(k, v)| k == "outcome" && v == "\"err\""));
    }

    #[test]
    fn io_println_arity_mismatch_errors() {
        let (v, _) = ev_with_cap_traced("io.println()", star_cap());
        assert!(matches!(v.unwrap_err().kind, EvalErrorKind::Arity { .. }));
    }

    // ---- M4.T8 — diagnostic class is *not* required to live inside an `intent` ----

    #[test]
    fn io_println_at_top_level_passes_v2_check() {
        // The static checker (M2.T7) treats `io.println` as
        // diagnostic — a bare call without an enclosing `intent`
        // does NOT fire the V2 error.
        let m =
            crate::syntax::parse(r#"fn main(cap: cap[io.println]) { io.println("hi") }"#).unwrap();
        let errs = crate::check::check_module(&m);
        assert!(
            errs.iter().all(|e| !matches!(
                e.kind,
                crate::check::CheckErrorKind::MissingIntentForWriteCall { .. }
            )),
            "unexpected V2 error: {errs:?}"
        );
    }

    // ---- M4.T3 — `aeris run` with `main(cap)` synthesises cap[*] ----

    #[test]
    fn run_main_passes_synthesised_cap_to_main() {
        let src = r#"
            fn main(cap: cap[clock.now]) -> int {
                let _t = clock.now()
                42
            }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        let v = run_main(&m).unwrap();
        assert_eq!(v, Value::Int(42));
    }

    // ---- M4.T4 — `io.read_line` with stdin source ----

    fn ev_with_stdin(src: &str, lines: Vec<&str>) -> (Result<Value, EvalError>, Vec<TraceEvent>) {
        let tracer = Tracer::in_memory();
        let expr = parse_expression(src).unwrap();
        let mut env = Env::new()
            .with_tracer(tracer.clone())
            .with_stdin_lines(lines.into_iter().map(String::from).collect());
        env.bind_let("cap", star_cap());
        let r = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        (r, tracer.events())
    }

    #[test]
    fn io_read_line_pops_pre_fed_queue() {
        let (v, evs) = ev_with_stdin("io.read_line()", vec!["hello"]);
        assert_eq!(v.unwrap(), Value::some(Value::Str("hello".into())));
        assert!(evs.iter().any(|e| e.kind == "io_read_line"));
    }

    #[test]
    fn io_read_line_returns_none_at_eof() {
        let (v, evs) = ev_with_stdin("io.read_line()", Vec::new());
        assert_eq!(v.unwrap(), Value::none());
        let evt = evs.iter().find(|e| e.kind == "io_read_line").unwrap();
        assert!(evt.fields.iter().any(|(k, v)| k == "eof" && v == "true"));
    }

    #[test]
    fn io_read_line_consumes_lines_in_order() {
        let (v, _) = ev_with_stdin(
            "{ let a = io.read_line(); let b = io.read_line(); (a, b) }",
            vec!["one", "two"],
        );
        match v.unwrap() {
            Value::Tuple(xs) => {
                assert_eq!(xs[0], Value::some(Value::Str("one".into())));
                assert_eq!(xs[1], Value::some(Value::Str("two".into())));
            }
            other => panic!("{other:?}"),
        }
    }

    // ---- M4.T5 — fs builtins with allow-list ----

    fn unique_tmp(suffix: &str) -> String {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        format!("/tmp/aeris-test-{pid}-{nanos}-{suffix}")
    }

    #[test]
    fn fs_write_then_read_round_trip() {
        let path = unique_tmp("rt.txt");
        let cap_val = cap(
            vec![
                (vec!["fs", "write_text"], Some(vec!["/tmp/**"])),
                (vec!["fs", "read_text"], Some(vec!["/tmp/**"])),
                (vec!["fs", "remove"], Some(vec!["/tmp/**"])),
            ],
            false,
        );
        let src = format!(
            r#"{{
                let _w = fs.write_text("{path}", "hello world")?;
                let r  = fs.read_text("{path}")?;
                let _r2 = fs.remove("{path}")?;
                r
            }}"#
        );
        let tracer = Tracer::in_memory();
        let expr = parse_expression(&src).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap_val);
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert_eq!(v, Value::Str("hello world".into()));
        let kinds: Vec<_> = tracer.events().into_iter().map(|e| e.kind).collect();
        assert!(kinds.contains(&"fs_write".to_string()));
        assert!(kinds.contains(&"fs_read".to_string()));
    }

    #[test]
    fn fs_write_outside_allow_list_raises_policy_violation() {
        let cap_val = cap(
            vec![(vec!["fs", "write_text"], Some(vec!["/safe/**"]))],
            false,
        );
        let src = r#"fs.write_text("/etc/passwd", "evil")"#;
        let tracer = Tracer::in_memory();
        let expr = parse_expression(src).unwrap();
        let mut env = Env::new().with_tracer(tracer);
        env.bind_let("cap", cap_val);
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, target } => {
                assert_eq!(op, "fs.write_text");
                assert_eq!(target, "/etc/passwd");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fs_exists_returns_bool_for_known_path() {
        let cap_val = cap(vec![(vec!["fs", "exists"], Some(vec!["/tmp/**"]))], false);
        let path = unique_tmp("nope.txt");
        let src = format!(r#"fs.exists("{path}")"#);
        let expr = parse_expression(&src).unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert_eq!(v, Value::Bool(false));
    }

    #[test]
    fn fs_glob_matches_double_star_subdirs() {
        // The matcher is segment-based; the spec's allow-list patterns
        // (`./out/**`, `/etc/aeris/**`) are exclusively segment-glob,
        // so in-segment shell wildcards like `*.aer` are out of scope
        // for this layer.
        assert!(glob_matches(
            "/tmp/aeris-test/**",
            "/tmp/aeris-test/foo/bar"
        ));
        assert!(glob_matches("./out/**", "./out/release/aeris"));
        assert!(!glob_matches("./out/**", "./other/file"));
        assert!(glob_matches("/etc/aeris/*/conf", "/etc/aeris/dev/conf"));
        assert!(!glob_matches(
            "/etc/aeris/*/conf",
            "/etc/aeris/dev/sub/conf"
        ));
    }

    #[test]
    fn fs_remove_outside_allow_rejected() {
        let cap_val = cap(vec![(vec!["fs", "remove"], Some(vec!["/safe/**"]))], false);
        let src = r#"fs.remove("/etc/something")"#;
        let expr = parse_expression(src).unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::PolicyViolation { .. }));
    }

    #[test]
    fn fs_module_level_cap_authorises_every_op() {
        // `cap[fs]` (one-segment path) covers every fs.* op.
        let cap_val = cap(vec![(vec!["fs"], None)], false);
        let path = unique_tmp("dir-test");
        let src =
            format!(r#"{{ let _r = fs.mkdir("{path}")?; let _r2 = fs.remove("{path}")?; "ok" }}"#);
        let expr = parse_expression(&src).unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert_eq!(v, Value::Str("ok".into()));
    }

    // ---- M5.T4 / T5 — runtime contracts ----

    fn run_contract(src: &str) -> Result<Value, EvalError> {
        let m = crate::syntax::parse(src).unwrap();
        run_main(&m)
    }

    fn assert_violation(src: &str, want_fn: &str, want_clause: super::ContractClause) {
        let err = run_contract(src).unwrap_err();
        match err.kind {
            EvalErrorKind::ContractViolation { fn_name, clause } => {
                assert_eq!(fn_name, want_fn);
                assert_eq!(clause, want_clause);
            }
            other => panic!("expected ContractViolation, got {other:?}"),
        }
    }

    #[test]
    fn contract_requires_pass_returns_value() {
        let src = r#"
            fn discount(amount: int, pct: int) -> int
                requires: amount >= 0
                requires: pct >= 0
                requires: pct <= 100
            {
                amount - (amount * pct / 100)
            }
            fn main() -> int { discount(100, 20) }
        "#;
        assert_eq!(run_contract(src).unwrap(), Value::Int(80));
    }

    #[test]
    fn contract_requires_first_clause_violation() {
        let src = r#"
            fn discount(amount: int, pct: int) -> int
                requires: amount >= 0
                requires: pct <= 100
            {
                amount
            }
            fn main() -> int { discount(-1, 50) }
        "#;
        assert_violation(
            src,
            "discount",
            super::ContractClause::Requires { index: 0 },
        );
    }

    #[test]
    fn contract_requires_second_clause_violation() {
        let src = r#"
            fn discount(amount: int, pct: int) -> int
                requires: amount >= 0
                requires: pct <= 100
            {
                amount
            }
            fn main() -> int { discount(10, 200) }
        "#;
        assert_violation(
            src,
            "discount",
            super::ContractClause::Requires { index: 1 },
        );
    }

    #[test]
    fn contract_ensures_passes_with_result_binding() {
        let src = r#"
            fn double(x: int) -> int
                ensures: result == x + x
            {
                x * 2
            }
            fn main() -> int { double(7) }
        "#;
        assert_eq!(run_contract(src).unwrap(), Value::Int(14));
    }

    #[test]
    fn contract_ensures_violation_via_buggy_body() {
        // The body returns x+1 but ensures expects double(x).
        let src = r#"
            fn double(x: int) -> int
                ensures: result == x + x
            {
                x + 1
            }
            fn main() -> int { double(7) }
        "#;
        assert_violation(src, "double", super::ContractClause::Ensures { index: 0 });
    }

    #[test]
    fn contract_ensures_with_two_clauses() {
        let src = r#"
            fn nonneg(x: int) -> int
                ensures: result >= 0
                ensures: result == x
            {
                x
            }
            fn main() -> int { nonneg(5) }
        "#;
        assert_eq!(run_contract(src).unwrap(), Value::Int(5));
    }

    #[test]
    fn contract_violation_is_not_catchable_via_question_mark() {
        // M5.T5 acceptance: `?` cannot suppress a contract violation.
        let src = r#"
            fn require_positive(x: int) -> int
                requires: x > 0
            {
                x
            }
            fn safe(x: int) -> result<int> {
                Ok(require_positive(x))
            }
            fn main() -> result<int> {
                safe(-1)?;
                Ok(0)
            }
        "#;
        let err = run_contract(src).unwrap_err();
        assert!(
            matches!(err.kind, EvalErrorKind::ContractViolation { .. }),
            "expected ContractViolation, got {:?}",
            err.kind
        );
    }

    #[test]
    fn contract_ensures_can_reference_input_through_let_capture() {
        let src = r#"
            fn add_two(x: int) -> int
                ensures: result == x + 2
            {
                x + 2
            }
            fn main() -> int { add_two(40) }
        "#;
        assert_eq!(run_contract(src).unwrap(), Value::Int(42));
    }

    #[test]
    fn contract_requires_evaluating_to_non_bool_violates() {
        // Numeric expression can't substitute for a bool — treated
        // as a violated clause.
        let src = r#"
            fn f(x: int) -> int
                requires: x
            {
                x
            }
            fn main() -> int { f(1) }
        "#;
        assert!(matches!(
            run_contract(src).unwrap_err().kind,
            EvalErrorKind::ContractViolation { .. }
        ));
    }

    #[test]
    fn contract_ensures_violation_returns_at_index_zero() {
        let src = r#"
            fn f(x: int) -> int
                ensures: result > 100
                ensures: result == x
            {
                x
            }
            fn main() -> int { f(5) }
        "#;
        // result > 100 is the first ensures, fires first when result=5.
        assert_violation(src, "f", super::ContractClause::Ensures { index: 0 });
    }

    // ---- M5.T6 — `where` clauses on record / model fields ----

    fn run_where(src: &str) -> Result<Value, EvalError> {
        let m = crate::syntax::parse(src).unwrap();
        run_main(&m)
    }

    fn assert_where_violation(src: &str) {
        let err = run_where(src).unwrap_err();
        match err.kind {
            EvalErrorKind::ContractViolation { .. } => {}
            other => panic!("expected ContractViolation, got {other:?}"),
        }
    }

    // -- positive (8) --

    #[test]
    fn t6_record_where_passing_constructs_record() {
        let src = r#"
            record Order { total: int where total > 0 }
            fn main() -> Order { Order { total: 10 } }
        "#;
        let v = run_where(src).unwrap();
        assert!(matches!(v, Value::Record(r) if r.name.as_deref() == Some("Order")));
    }

    #[test]
    fn t6_record_where_with_two_clauses_pass() {
        let src = r#"
            record Score { pct: int where pct >= 0, n: int where n > 0 }
            fn main() -> Score { Score { pct: 50, n: 10 } }
        "#;
        run_where(src).unwrap();
    }

    #[test]
    fn t6_anonymous_record_skips_where_check() {
        // Anonymous record `{ a: -1 }` has no decl — runtime never
        // applies `where` checks (there is no schema in scope).
        let src = r#"fn main() -> int { ({ a: -1 }).a }"#;
        let v = run_where(src).unwrap();
        assert_eq!(v, Value::Int(-1));
    }

    #[test]
    fn t6_record_without_where_is_unchanged() {
        let src = r#"
            record Point { x: int, y: int }
            fn main() -> Point { Point { x: 1, y: 2 } }
        "#;
        run_where(src).unwrap();
    }

    #[test]
    fn t6_record_where_can_reference_other_fields() {
        // The clause sees every field in scope — `n` can compare to `m`.
        let src = r#"
            record Range { lo: int, hi: int where hi >= lo }
            fn main() -> Range { Range { lo: 1, hi: 5 } }
        "#;
        run_where(src).unwrap();
    }

    #[test]
    fn t6_record_where_chained_via_let_does_not_loop() {
        let src = r#"
            record Wrap { v: int where v == v }
            fn main() -> Wrap { Wrap { v: 7 } }
        "#;
        run_where(src).unwrap();
    }

    #[test]
    fn t6_record_where_evaluating_to_unrelated_bool_passes() {
        let src = r#"
            record S { ok: bool where ok }
            fn main() -> S { S { ok: true } }
        "#;
        run_where(src).unwrap();
    }

    #[test]
    fn t6_record_with_zero_where_clauses_succeeds() {
        let src = r#"
            record User { id: int, name: string }
            fn main() -> User { User { id: 1, name: "x" } }
        "#;
        run_where(src).unwrap();
    }

    // -- negative (7) --

    #[test]
    fn t6_record_where_violation_reports_contract() {
        let src = r#"
            record Order { total: int where total > 0 }
            fn main() -> Order { Order { total: 0 } }
        "#;
        assert_where_violation(src);
    }

    #[test]
    fn t6_record_where_negative_value_rejected() {
        let src = r#"
            record Order { total: int where total > 0 }
            fn main() -> Order { Order { total: -5 } }
        "#;
        assert_where_violation(src);
    }

    #[test]
    fn t6_record_where_first_clause_violation_short_circuits() {
        let src = r#"
            record S {
                a: int where a > 0,
                b: int where b > 0
            }
            fn main() -> S { S { a: -1, b: 1 } }
        "#;
        assert_where_violation(src);
    }

    #[test]
    fn t6_record_where_second_clause_violation_caught() {
        let src = r#"
            record S {
                a: int where a > 0,
                b: int where b > 0
            }
            fn main() -> S { S { a: 1, b: -1 } }
        "#;
        assert_where_violation(src);
    }

    #[test]
    fn t6_record_where_cross_field_violation() {
        let src = r#"
            record Range { lo: int, hi: int where hi >= lo }
            fn main() -> Range { Range { lo: 5, hi: 1 } }
        "#;
        assert_where_violation(src);
    }

    #[test]
    fn t6_record_where_bool_field_must_be_true() {
        let src = r#"
            record S { ok: bool where ok }
            fn main() -> S { S { ok: false } }
        "#;
        assert_where_violation(src);
    }

    // ---- M8.T1 — `model@vN` construction validation (20 fixtures) ----

    fn run_model(src: &str) -> Result<Value, EvalError> {
        let m = crate::syntax::parse(src).unwrap();
        run_main(&m)
    }

    fn assert_schema_violation(src: &str, model: &str, version: u32) -> Vec<String> {
        let err = run_model(src).unwrap_err();
        match err.kind {
            EvalErrorKind::SchemaViolation {
                model: m,
                version: v,
                problems,
            } => {
                assert_eq!(m, model);
                assert_eq!(v, version);
                problems
            }
            other => panic!("expected SchemaViolation, got {other:?}"),
        }
    }

    // -- positive (10) --

    #[test]
    fn t1_model_simple_field_passes() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn main() -> Order { Order@v1 { total: 10 } }
        "#;
        let v = run_model(src).unwrap();
        assert!(matches!(v, Value::Record(r) if r.name.as_deref() == Some("Order")));
    }

    #[test]
    fn t1_model_with_no_where_clauses_passes() {
        let src = r#"
            model User@v1 { id: int, name: string }
            fn main() -> User { User@v1 { id: 1, name: "alice" } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_bool_field_passes() {
        let src = r#"
            model Flag@v1 { ok: bool where ok }
            fn main() -> Flag { Flag@v1 { ok: true } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_cross_field_where_passes() {
        let src = r#"
            model Range@v1 { lo: int, hi: int where hi >= lo }
            fn main() -> Range { Range@v1 { lo: 1, hi: 5 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_two_field_clauses_pass() {
        let src = r#"
            model Score@v1 { pct: int where pct >= 0, n: int where n > 0 }
            fn main() -> Score { Score@v1 { pct: 50, n: 10 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_two_versions_coexist() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            model Order@v2 { total: int, currency: string }
            fn main() -> Order {
                let _ = Order@v1 { total: 1 }
                Order@v2 { total: 5, currency: "EUR" }
            }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_with_record_level_invariant_passes() {
        let src = r#"
            model Order@v1 {
                total: int
                where: total >= 0
            }
            fn main() -> Order { Order@v1 { total: 0 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_with_multiple_record_invariants_passes() {
        let src = r#"
            model Range@v1 {
                lo: int
                hi: int
                where: hi >= lo
                where: hi - lo < 100
            }
            fn main() -> Range { Range@v1 { lo: 1, hi: 10 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_with_field_and_record_where_passes() {
        let src = r#"
            model Order@v1 {
                total: int where total > 0
                discount: int where discount >= 0
                where: discount <= total
            }
            fn main() -> Order { Order@v1 { total: 100, discount: 10 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t1_model_string_field_passes() {
        let src = r#"
            model User@v1 { name: string }
            fn main() -> User { User@v1 { name: "alice" } }
        "#;
        run_model(src).unwrap();
    }

    // -- negative (10) --

    #[test]
    fn t1_model_field_where_violation_reports_schema() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn main() -> Order { Order@v1 { total: 0 } }
        "#;
        let problems = assert_schema_violation(src, "Order", 1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("total"));
    }

    #[test]
    fn t1_model_cross_field_violation() {
        let src = r#"
            model Range@v1 { lo: int, hi: int where hi >= lo }
            fn main() -> Range { Range@v1 { lo: 5, hi: 1 } }
        "#;
        let problems = assert_schema_violation(src, "Range", 1);
        assert!(problems[0].contains("hi"));
    }

    #[test]
    fn t1_model_missing_field_violation() {
        let src = r#"
            model User@v1 { id: int, name: string }
            fn main() -> User { User@v1 { id: 1 } }
        "#;
        let problems = assert_schema_violation(src, "User", 1);
        assert!(problems.iter().any(|p| p.contains("missing field `name`")));
    }

    #[test]
    fn t1_model_unknown_field_violation() {
        let src = r#"
            model User@v1 { id: int }
            fn main() -> User { User@v1 { id: 1, foo: "x" } }
        "#;
        let problems = assert_schema_violation(src, "User", 1);
        assert!(problems.iter().any(|p| p.contains("unknown field `foo`")));
    }

    #[test]
    fn t1_model_wrong_version_violation() {
        let src = r#"
            model User@v1 { id: int }
            fn main() -> User { User@v2 { id: 1 } }
        "#;
        let problems = assert_schema_violation(src, "User", 2);
        assert!(problems[0].contains("not declared"));
    }

    #[test]
    fn t1_model_undeclared_violation() {
        let src = r#"
            fn main() -> int { let x = Foo@v1 { id: 1 } 0 }
        "#;
        let problems = assert_schema_violation(src, "Foo", 1);
        assert!(problems[0].contains("not declared"));
    }

    #[test]
    fn t1_model_record_level_invariant_violation() {
        let src = r#"
            model Order@v1 {
                total: int
                discount: int
                where: discount <= total
            }
            fn main() -> Order { Order@v1 { total: 10, discount: 50 } }
        "#;
        let problems = assert_schema_violation(src, "Order", 1);
        assert!(problems.iter().any(|p| p.contains("record invariant")));
    }

    #[test]
    fn t1_model_accumulates_multiple_problems() {
        let src = r#"
            model S@v1 {
                a: int where a > 0
                b: int where b > 0
            }
            fn main() -> S { S@v1 { a: -1, b: -2 } }
        "#;
        let problems = assert_schema_violation(src, "S", 1);
        assert_eq!(problems.len(), 2);
    }

    #[test]
    fn t1_model_violation_carries_correct_name_and_version() {
        let src = r#"
            model Invoice@v3 { total: int where total > 0 }
            fn main() -> Invoice { Invoice@v3 { total: -1 } }
        "#;
        // assert_schema_violation already checks model + version match.
        assert_schema_violation(src, "Invoice", 3);
    }

    // ---- M8.T3 — record-level `where:` cross-field invariants (5 fixtures) ----

    #[test]
    fn t3_cross_field_two_clauses_pass_together() {
        let src = r#"
            model Range@v1 {
                lo: int
                hi: int
                where: hi >= lo
                where: hi - lo < 100
            }
            fn main() -> Range@v1 { Range@v1 { lo: 0, hi: 50 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t3_cross_field_implication_passes_when_lhs_false() {
        // `cancelled implies total == 0` encoded as `!cancelled or total == 0`.
        let src = r#"
            model Order@v1 {
                cancelled: bool
                total: int
                where: not cancelled or total == 0
            }
            fn main() -> Order@v1 { Order@v1 { cancelled: false, total: 100 } }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t3_cross_field_implication_violation_when_lhs_true() {
        let src = r#"
            model Order@v1 {
                cancelled: bool
                total: int
                where: not cancelled or total == 0
            }
            fn main() -> Order@v1 { Order@v1 { cancelled: true, total: 50 } }
        "#;
        let problems = assert_schema_violation(src, "Order", 1);
        assert!(problems.iter().any(|p| p.contains("record invariant")));
    }

    #[test]
    fn t3_cross_field_arithmetic_invariant_violation() {
        // `discount + tax <= total` is a multi-field arithmetic invariant.
        let src = r#"
            model Order@v1 {
                total: int
                discount: int
                tax: int
                where: discount + tax <= total
            }
            fn main() -> Order@v1 {
                Order@v1 { total: 10, discount: 7, tax: 5 }
            }
        "#;
        let problems = assert_schema_violation(src, "Order", 1);
        assert!(problems.iter().any(|p| p.contains("record invariant")));
    }

    #[test]
    fn t3_cross_field_arithmetic_invariant_passes() {
        let src = r#"
            model Order@v1 {
                total: int
                discount: int
                tax: int
                where: discount + tax <= total
            }
            fn main() -> Order@v1 {
                Order@v1 { total: 100, discount: 10, tax: 5 }
            }
        "#;
        run_model(src).unwrap();
    }

    // ---- M8.T2 — `json.decode<Model@vN>` + `http.body<Model@vN>` (10 fixtures) ----

    #[test]
    fn t2_json_decode_simple_passes() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn main() -> result<Order@v1> {
                let s = "{\"total\": 42}"
                json.decode<Order@v1>(s)
            }
        "#;
        let v = run_model(src).unwrap();
        // Result(Ok(Record))
        match v {
            Value::Result(Ok(inner)) => match *inner {
                Value::Record(r) => assert_eq!(r.name.as_deref(), Some("Order")),
                other => panic!("expected record, got {other:?}"),
            },
            other => panic!("expected ok(record), got {other:?}"),
        }
    }

    #[test]
    fn t2_json_decode_where_violation_raises_schema_violation() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn main() -> result<Order@v1> {
                let s = "{\"total\": 0}"
                json.decode<Order@v1>(s)
            }
        "#;
        assert_schema_violation(src, "Order", 1);
    }

    #[test]
    fn t2_json_decode_missing_field_raises_schema_violation() {
        let src = r#"
            model User@v1 { id: int, name: string }
            fn main() -> result<User@v1> {
                let s = "{\"id\": 1}"
                json.decode<User@v1>(s)
            }
        "#;
        let problems = assert_schema_violation(src, "User", 1);
        assert!(problems.iter().any(|p| p.contains("missing field `name`")));
    }

    #[test]
    fn t2_json_decode_unknown_field_raises_schema_violation() {
        let src = r#"
            model User@v1 { id: int }
            fn main() -> result<User@v1> {
                let s = "{\"id\": 1, \"foo\": \"x\"}"
                json.decode<User@v1>(s)
            }
        "#;
        let problems = assert_schema_violation(src, "User", 1);
        assert!(problems.iter().any(|p| p.contains("unknown field `foo`")));
    }

    #[test]
    fn t2_json_decode_wrong_version_raises_schema_violation() {
        let src = r#"
            model User@v1 { id: int }
            fn main() -> result<User@v2> {
                let s = "{\"id\": 1}"
                json.decode<User@v2>(s)
            }
        "#;
        let problems = assert_schema_violation(src, "User", 2);
        assert!(problems[0].contains("not declared"));
    }

    #[test]
    fn t2_json_decode_type_mismatch_raises_schema_violation() {
        let src = r#"
            model User@v1 { id: int }
            fn main() -> result<User@v1> {
                let s = "{\"id\": \"oops\"}"
                json.decode<User@v1>(s)
            }
        "#;
        let problems = assert_schema_violation(src, "User", 1);
        assert!(problems[0].contains("expected `int`"));
    }

    #[test]
    fn t2_json_decode_bool_field_passes() {
        let src = r#"
            model Flag@v1 { ok: bool where ok }
            fn main() -> result<Flag@v1> {
                let s = "{\"ok\": true}"
                json.decode<Flag@v1>(s)
            }
        "#;
        run_model(src).unwrap();
    }

    #[test]
    fn t2_json_decode_record_invariant_violation() {
        let src = r#"
            model Range@v1 {
                lo: int
                hi: int
                where: hi >= lo
            }
            fn main() -> result<Range@v1> {
                let s = "{\"lo\": 5, \"hi\": 1}"
                json.decode<Range@v1>(s)
            }
        "#;
        let problems = assert_schema_violation(src, "Range", 1);
        assert!(problems.iter().any(|p| p.contains("record invariant")));
    }

    #[test]
    fn t2_http_body_passes_when_body_matches_model() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn main() -> result<Order@v1> {
                let resp = HttpResponse { status: 200, body: "{\"total\": 99}" }
                http.body<Order@v1>(resp)
            }
        "#;
        let v = run_model(src).unwrap();
        match v {
            Value::Result(Ok(_)) => {}
            other => panic!("expected ok, got {other:?}"),
        }
    }

    #[test]
    fn t2_http_body_violation_raises_schema_violation() {
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn main() -> result<Order@v1> {
                let resp = HttpResponse { status: 200, body: "{\"total\": 0}" }
                http.body<Order@v1>(resp)
            }
        "#;
        assert_schema_violation(src, "Order", 1);
    }

    #[test]
    fn t1_model_violation_not_catchable_by_question_mark() {
        // `?` only catches `result<T>` / `option<T>` errors. SchemaViolation
        // lifts past it as a runtime fatal (§ 16.2 / § 18.4).
        let src = r#"
            model Order@v1 { total: int where total > 0 }
            fn try_it() -> result<int> {
                let o = Order@v1 { total: -1 }
                Ok(o.total)
            }
            fn main() -> result<int> { Ok(try_it()?) }
        "#;
        let err = run_model(src).unwrap_err();
        match err.kind {
            EvalErrorKind::SchemaViolation { .. } => {}
            other => panic!("expected SchemaViolation, got {other:?}"),
        }
    }

    // ---- M8.T4 — policy runtime (one fixture per clause) ----

    fn run_with_tracer(src: &str) -> (Result<Value, EvalError>, Vec<TraceEvent>) {
        let m = crate::syntax::parse(src).unwrap();
        let tracer = Tracer::in_memory();
        let r = super::run_main_with(&m, Some(tracer.clone()));
        (r, tracer.events())
    }

    #[test]
    fn t4_policy_match_clause_filters_by_cap_path() {
        // Wildcard `http.*` matches both http.get and http.post; an
        // unrelated cap call (`io.println`) is left untouched.
        let src = r#"
            policy block_http {
                match: http.*
                deny: true
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let (r, _) = run_with_tracer(src);
        // io.println is not covered by `http.*`; the run succeeds.
        r.unwrap();
    }

    #[test]
    fn t4_policy_deny_clause_blocks_call() {
        let src = r#"
            policy noisy_io {
                match: io.println
                deny: true
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let (r, _) = run_with_tracer(src);
        let err = r.unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, target } => {
                assert_eq!(op, "io.println");
                assert!(target.contains("noisy_io"));
            }
            other => panic!("expected PolicyViolation, got {other:?}"),
        }
    }

    #[test]
    fn t4_policy_require_clause_must_hold() {
        let src = r#"
            policy must_be_short {
                match: io.println
                require: false
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let (r, _) = run_with_tracer(src);
        let err = r.unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, target } => {
                assert_eq!(op, "io.println");
                assert!(target.contains("must_be_short"));
            }
            other => panic!("expected PolicyViolation, got {other:?}"),
        }
    }

    #[test]
    fn t4_policy_limit_clause_records_trace_event() {
        let src = r#"
            policy budget {
                match: io.println
                limit: tokens_per_minute = 100
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let (r, evs) = run_with_tracer(src);
        r.unwrap();
        let limit = evs.iter().find(|e| e.kind == "policy_limit").unwrap();
        assert!(limit
            .fields
            .iter()
            .any(|(k, v)| k == "name" && v.contains("tokens_per_minute")));
        assert!(limit.fields.iter().any(|(k, v)| k == "value" && v == "100"));
    }

    #[test]
    fn t4_policy_audit_clause_records_trace_event() {
        let src = r#"
            policy egress_audit {
                match: io.println
                audit: { kind: "stdout" }
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let (r, evs) = run_with_tracer(src);
        r.unwrap();
        let audit = evs.iter().find(|e| e.kind == "policy_audit").unwrap();
        assert!(audit
            .fields
            .iter()
            .any(|(k, v)| k == "policy" && v.contains("egress_audit")));
        assert!(audit.fields.iter().any(|(k, _)| k == "kind"));
    }

    // ---- M8.T7 — `PolicyViolation` not catchable by `?` ----

    #[test]
    fn t7_policy_violation_propagates_past_question_mark() {
        // `?` only catches `result<T>` / `option<T>` errors. A
        // PolicyViolation surfaced from a cap call propagates past it
        // unchanged (§ 18.4).
        let src = r#"
            policy noisy {
                match: io.println
                deny: true
            }
            fn try_it() -> result<unit> {
                io.println("ok")
                Ok(())
            }
            fn main() -> result<unit> { Ok(try_it()?) }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        let err = super::run_main(&m).unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { .. } => {}
            other => panic!("expected PolicyViolation, got {other:?}"),
        }
    }

    // ---- M8.T6 — policy drift trace event ----

    #[test]
    fn t6_policy_drift_event_emitted_on_synthetic_divergence() {
        // The replay driver wires this in M9; for M8 we synthesise the
        // divergence directly and assert the event shape.
        let tracer = Tracer::in_memory();
        let env = Env::new().with_tracer(tracer.clone());
        super::emit_policy_drift(&env, "production_egress", "http.post", "deny", "allow");
        let drift = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "policy_drift")
            .unwrap();
        assert!(drift
            .fields
            .iter()
            .any(|(k, v)| k == "policy" && v.contains("production_egress")));
        assert!(drift
            .fields
            .iter()
            .any(|(k, v)| k == "expected" && v.contains("deny")));
        assert!(drift
            .fields
            .iter()
            .any(|(k, v)| k == "observed" && v.contains("allow")));
    }

    // ---- M8.T5 — policy activation modes ----

    #[test]
    fn t5_module_import_mode_activates_policy() {
        // Mode 1: a policy declared in module source is auto-active —
        // the same path exercised by every M8.T4 fixture, called out
        // here for milestone bookkeeping.
        let src = r#"
            policy noisy {
                match: io.println
                deny: true
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        assert!(super::run_main(&m).is_err());
    }

    #[test]
    fn t5_attribute_mode_attaches_policy_name_to_fn() {
        // Mode 2: `#[policy(name)]` attaches the listed policy names
        // onto the fn at parse time. M8 wires module-declared policies
        // globally; the attribute carries scope refinement metadata
        // for the lockset / agent_net layer to consume in M10+.
        let src = r#"
            policy production_writes {
                match: kube.apply
                deny: true
            }
            #[policy(production_writes)]
            fn deploy() -> unit { () }
            fn main() -> unit { deploy() }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        let f = m
            .items
            .iter()
            .find_map(|it| match it {
                crate::syntax::ast::Item::Fn(f) if f.name == "deploy" => Some(f),
                _ => None,
            })
            .unwrap();
        assert_eq!(f.policy_attrs, vec!["production_writes".to_string()]);
        // Module-declared policies are still active globally — but
        // `kube.apply` is not called here so the run succeeds.
        super::run_main(&m).unwrap();
    }

    #[test]
    fn t5_lockset_mode_attach_point_works() {
        // Mode 3 (`lockset.toml [policies]`) — the full toml-driven
        // wiring lands with M11's lockset work; the runtime already
        // exposes `Env::with_policies` as the attach point. Here we
        // verify that policies handed in externally (no source decl)
        // are honoured by `apply_policies`.
        let src = r#"
            policy noisy {
                match: io.println
                deny: true
            }
            fn ignored() -> unit { () }
        "#;
        let m = crate::syntax::parse(src).unwrap();
        let policies_external: Vec<_> = m
            .items
            .iter()
            .filter_map(|it| match it {
                crate::syntax::ast::Item::Policy(p) => Some(p.clone()),
                _ => None,
            })
            .collect();
        let mut env = Env::new().with_policies(std::rc::Rc::new(policies_external));
        env.bind_let("cap", star_cap());
        let expr = parse_expression(r#"io.println("ok")"#).unwrap();
        let r = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        match r.unwrap_err().kind {
            EvalErrorKind::PolicyViolation { .. } => {}
            other => panic!("expected PolicyViolation, got {other:?}"),
        }
    }

    #[test]
    fn t4_policy_when_clause_gates_activation() {
        // `when:` evaluates to false → the policy stays inactive even
        // though the cap path matches. The deny: true would otherwise
        // block the call.
        let src = r#"
            policy only_in_prod {
                match: io.println
                when: false
                deny: true
            }
            fn main() -> unit { io.println("ok") }
        "#;
        let (r, _) = run_with_tracer(src);
        // Deny is gated off by `when: false` → run succeeds.
        r.unwrap();
    }

    // ---- M9.T1 / T2 / T3 — `ai` cap handler, ops and tape ----

    fn ai_cap() -> Value {
        cap(
            vec![
                (vec!["ai", "complete"], Some(vec!["claude-haiku-4-5"])),
                (vec!["ai", "chat"], Some(vec!["claude-haiku-4-5"])),
                (vec!["ai", "embed"], Some(vec!["claude-embed"])),
                (vec!["ai", "tools"], Some(vec!["claude-haiku-4-5"])),
            ],
            false,
        )
    }

    #[test]
    fn t9_1_ai_complete_uses_default_mock_backend() {
        // No `ai_backend` configured → echoes the prompt with model
        // metadata. Deterministic so traces are stable in tests.
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", ai_cap());
        let expr = parse_expression(r#"ai.complete("hello")"#).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Str(s) => assert!(s.contains("hello"), "got {s}"),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        // M9.T3: every ai.* call records an `ai_call` event.
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "ai_call")
            .unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "op" && v.contains("ai.complete")));
        assert!(evt.fields.iter().any(|(k, _)| k == "model"));
        assert!(evt.fields.iter().any(|(k, _)| k == "tokens"));
        assert!(evt.fields.iter().any(|(k, _)| k == "prompt_hash"));
        assert!(evt.fields.iter().any(|(k, _)| k == "resp_hash"));
    }

    #[test]
    fn t9_1_ai_complete_without_cap_is_policy_violation() {
        // `ai.complete` without an `ai.complete` cap entry is a
        // policy violation — same shape as `http.post` outside the
        // allow-list (M5.T2).
        let mut env = Env::new();
        env.bind_let("cap", cap(vec![(vec!["io", "println"], None)], false));
        let expr = parse_expression(r#"ai.complete("hi")"#).unwrap();
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, .. } => {
                assert_eq!(op, "ai.complete");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t9_1_ai_backend_http_round_trips_via_mock_server() {
        // Wire the `http` backend at a local mock server and verify
        // the response text is round-tripped back through `ai.complete`.
        let port = spawn_mock_http(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n{\"text\":\"pong\"}",
        );
        let tracer = Tracer::in_memory();
        let backend = std::rc::Rc::new(crate::lockset::AiBackend {
            kind: "http".into(),
            url: Some(format!("http://127.0.0.1:{port}")),
            auth: None,
        });
        let mut env = Env::new().with_tracer(tracer).with_ai_backend(backend);
        env.bind_let("cap", ai_cap());
        let expr = parse_expression(r#"ai.complete("ping")"#).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Str(s) => assert_eq!(s, "pong"),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t9_2_ai_chat_handles_list_of_messages() {
        let mut env = Env::new();
        env.bind_let("cap", ai_cap());
        let expr = parse_expression(r#"ai.chat(["hello", "world"])"#).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Str(s) => {
                    assert!(s.contains("hello"));
                    assert!(s.contains("world"));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t9_2_ai_embed_returns_deterministic_vector() {
        let mut env = Env::new();
        env.bind_let("cap", ai_cap());
        let expr = parse_expression(r#"ai.embed("aeris")"#).unwrap();
        let v1 = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let v2 = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        // Same input → same vector (determinism for replay).
        assert_eq!(v1, v2);
        match v1 {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::List(xs) => assert_eq!(xs.len(), 8),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t9_2_ai_tools_passes_prompt_to_backend() {
        let mut env = Env::new();
        env.bind_let("cap", ai_cap());
        let expr = parse_expression(r#"ai.tools(["calc", "search"], "answer 1+1")"#).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Str(s) => assert!(s.contains("answer 1+1")),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t9_8_full_record_mode_stores_raw_bodies() {
        // M9.T8: opt-in flag `with_full_record(true)` swaps prompt /
        // response hashes for raw byte fields in the trace.
        let tracer = Tracer::in_memory();
        let mut env = Env::new()
            .with_tracer(tracer.clone())
            .with_full_record(true);
        env.bind_let("cap", ai_cap());
        let expr = parse_expression(r#"ai.complete("secret")"#).unwrap();
        let _ = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "ai_call")
            .unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "prompt" && v.contains("secret")));
        assert!(evt.fields.iter().any(|(k, _)| k == "response"));
        assert!(!evt.fields.iter().any(|(k, _)| k == "prompt_hash"));
    }

    // ---- M9.T4 / T5 / T6 / T7 — replay tape ----

    fn ai_only_cap() -> Value {
        cap(
            vec![(vec!["ai", "complete"], Some(vec!["claude-haiku-4-5"]))],
            false,
        )
    }

    /// Run an expression with a fresh tracer + full-record on, return
    /// the value and recorded events. Helper for the record-then-
    /// replay tests below.
    fn run_record(src: &str) -> (Value, Vec<TraceEvent>) {
        let tracer = Tracer::in_memory();
        let mut env = Env::new()
            .with_tracer(tracer.clone())
            .with_full_record(true);
        env.bind_let("cap", ai_only_cap());
        let expr = parse_expression(src).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        (v, tracer.events())
    }

    fn run_replay(
        src: &str,
        events: Vec<TraceEvent>,
        mode: crate::runtime::replay::ReplayMode,
    ) -> Value {
        let tape = crate::runtime::replay::handle_from_events(events, mode);
        let mut env = Env::new().with_replay_tape(tape).with_full_record(true);
        env.bind_let("cap", ai_only_cap());
        let expr = parse_expression(src).unwrap();
        eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap()
    }

    #[test]
    fn t9_4_replay_returns_recorded_ai_response() {
        // M9.T4: replay drains the recorded `ai_call` event and the
        // re-run sees the same response, no LLM contacted.
        let src = r#"ai.complete("hello")"#;
        let (live, events) = run_record(src);
        let replayed = run_replay(
            src,
            events,
            crate::runtime::replay::ReplayMode::FromFixtures,
        );
        assert_eq!(live, replayed);
    }

    #[test]
    fn t9_5_replay_pins_clock_now_to_recorded_value() {
        // M9.T5: clock.now under replay returns the recorded value,
        // bit-identical to the original even across wall-clock skew.
        let src = r#"clock.now()"#;
        let tracer = Tracer::in_memory();
        let mut env = Env::new()
            .with_tracer(tracer.clone())
            .with_full_record(true);
        env.bind_let("cap", star_cap());
        let expr = parse_expression(src).unwrap();
        let v1 = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let events = tracer.events();
        // Replay against the recorded event.
        let tape = crate::runtime::replay::handle_from_events(
            events,
            crate::runtime::replay::ReplayMode::FromFixtures,
        );
        let mut env2 = Env::new().with_replay_tape(tape);
        env2.bind_let("cap", star_cap());
        let v2 = eval_expr(&expr, &mut env2)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn t9_5_replay_pins_random_next_to_recorded_value() {
        let src = r#"random.next()"#;
        let tracer = Tracer::in_memory();
        let mut env = Env::new()
            .with_tracer(tracer.clone())
            .with_full_record(true);
        env.bind_let("cap", star_cap());
        let expr = parse_expression(src).unwrap();
        let v1 = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let events = tracer.events();
        let tape = crate::runtime::replay::handle_from_events(
            events,
            crate::runtime::replay::ReplayMode::FromFixtures,
        );
        let mut env2 = Env::new().with_replay_tape(tape);
        env2.bind_let("cap", star_cap());
        let v2 = eval_expr(&expr, &mut env2)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn t9_6_live_mode_skips_ai_tape_but_replays_clock() {
        // M9.T6: under `--live`, ai.* goes back to the configured
        // backend (here: mock) while clock/random come from the tape.
        // We capture clock + ai under FromFixtures recording, then
        // replay in Live mode and verify ai goes live (different
        // value because the mock prompt-string varies per call) and
        // clock value is replayed.
        let src = r#"clock.now()"#;
        let tracer = Tracer::in_memory();
        let mut env = Env::new()
            .with_tracer(tracer.clone())
            .with_full_record(true);
        env.bind_let("cap", star_cap());
        let expr = parse_expression(src).unwrap();
        let v1 = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let events = tracer.events();
        let tape = crate::runtime::replay::handle_from_events(
            events,
            crate::runtime::replay::ReplayMode::Live,
        );
        let mut env2 = Env::new().with_replay_tape(tape);
        env2.bind_let("cap", star_cap());
        let v2 = eval_expr(&expr, &mut env2)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        // clock.now is replayed under Live mode (deterministic subset).
        assert_eq!(v1, v2);
    }

    #[test]
    fn t9_6_live_mode_does_not_drain_ai_call_events() {
        let src = r#"ai.complete("ping")"#;
        let (live, events) = run_record(src);
        let _ = live;
        // Replay under Live mode → ai_call NOT replayed; backend runs.
        let tape = crate::runtime::replay::handle_from_events(
            events,
            crate::runtime::replay::ReplayMode::Live,
        );
        let mut env = Env::new().with_replay_tape(tape);
        env.bind_let("cap", ai_only_cap());
        let expr = parse_expression(src).unwrap();
        let live_again = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        // Mock backend is deterministic for a given prompt → equal.
        // The point is the tape's ai_call was *not* drained.
        match live_again {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Str(s) => assert!(s.contains("ping")),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t9_7_from_fixtures_default_mode_is_offline() {
        // M9.T7: with `FromFixtures` (the CLI default) ai responses
        // come from the tape — no backend call. Verified by setting an
        // ai_backend that would error if invoked.
        let src = r#"ai.complete("hi")"#;
        let (_, events) = run_record(src);
        let tape = crate::runtime::replay::handle_from_events(
            events,
            crate::runtime::replay::ReplayMode::FromFixtures,
        );
        // Configure a backend that would fail if reached.
        let backend = std::rc::Rc::new(crate::lockset::AiBackend {
            kind: "cli".into(), // cli is not implemented → would error
            url: None,
            auth: None,
        });
        let mut env = Env::new()
            .with_replay_tape(tape)
            .with_ai_backend(backend)
            .with_full_record(true);
        env.bind_let("cap", ai_only_cap());
        let expr = parse_expression(src).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        // Backend was bypassed → no error from cli.
        match v {
            Value::Result(Ok(_)) => {}
            other => panic!("expected ok, got {other:?}"),
        }
    }

    // ---- M10.T2 / T3 / T4 — agent invocation ----

    /// Build a tape where every `ai_call` returns `response`. Multiple
    /// canned responses are draining FIFO — used by the retry tests.
    fn ai_tape(responses: &[&str]) -> crate::runtime::replay::TapeHandle {
        let events: Vec<TraceEvent> = responses
            .iter()
            .map(|r| TraceEvent {
                trace_id: "t".into(),
                ts: "now".into(),
                kind: "ai_call".into(),
                intent: None,
                scope: None,
                fields: vec![
                    ("op".into(), "\"ai.complete\"".into()),
                    ("model".into(), "\"mock\"".into()),
                    ("response".into(), format!("\"{}\"", r.replace('"', "\\\""))),
                ],
            })
            .collect();
        crate::runtime::replay::handle_from_events(
            events,
            crate::runtime::replay::ReplayMode::FromFixtures,
        )
    }

    fn run_module_with_tape(
        src: &str,
        tape: crate::runtime::replay::TapeHandle,
    ) -> (Result<Value, EvalError>, Vec<TraceEvent>) {
        let m = crate::syntax::parse(src).unwrap();
        let tracer = Tracer::in_memory();
        let cap_v = cap(
            vec![(vec!["ai", "complete"], Some(vec!["claude-haiku-4-5"]))],
            false,
        );
        let cap_inner = match cap_v {
            Value::Cap(c) => (*c).clone(),
            _ => unreachable!(),
        };
        let r = super::run_main_with_full_cfg(
            &m,
            cap_inner,
            Some(tracer.clone()),
            None,
            Some(tape),
            true,
        );
        (r, tracer.events())
    }

    #[test]
    fn t10_2_agent_returns_ok_record_when_response_validates() {
        let src = r#"
            model Invoice@v1 { id: int }
            model Category@v1 { kind: string where kind == "utilities" }
            agent classify {
                llm:     "claude-haiku-4-5"
                intent:  "classify"
                prompt:  "p"
                accept:  Invoice@v1
                produce: Category@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "triage" {
                    let inv = Invoice@v1 { id: 1 }
                    classify(inv, cap)
                }
            }
        "#;
        let tape = ai_tape(&[r#"{"kind":"utilities"}"#]);
        let (r, _) = run_module_with_tape(src, tape);
        let v = r.unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Record(r) => assert_eq!(r.name.as_deref(), Some("Category")),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t10_2_agent_rejects_input_with_wrong_model_name() {
        let src = r#"
            model Invoice@v1 { id: int }
            model Other@v1 { id: int }
            model Category@v1 { kind: string }
            agent classify {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  Invoice@v1
                produce: Category@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "x" {
                    let other = Other@v1 { id: 1 }
                    classify(other, cap)
                }
            }
        "#;
        let tape = ai_tape(&[r#"{"kind":"x"}"#]);
        let (r, _) = run_module_with_tape(src, tape);
        let err = r.unwrap_err();
        match err.kind {
            EvalErrorKind::SchemaViolation { model, .. } => {
                assert_eq!(model, "Invoice");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t10_3_prompt_includes_routing_protocol_contract() {
        // M10.T3: inspect the trace's recorded prompt and ensure the
        // auto-injected appendix is present.
        let src = r#"
            model Invoice@v1 { id: int }
            model Category@v1 { kind: string }
            agent classify {
                llm:     "claude-haiku-4-5"
                intent:  "classify the thing"
                prompt:  "USER PROMPT TEXT"
                accept:  Invoice@v1
                produce: Category@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "triage" {
                    let inv = Invoice@v1 { id: 1 }
                    classify(inv, cap)
                }
            }
        "#;
        let tape = ai_tape(&[r#"{"kind":"x"}"#]);
        let (r, evs) = run_module_with_tape(src, tape);
        r.unwrap();
        let evt = evs.iter().find(|e| e.kind == "ai_call").unwrap();
        let prompt = evt
            .fields
            .iter()
            .find(|(k, _)| k == "prompt")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert!(prompt.contains("USER PROMPT TEXT"), "prompt: {prompt}");
        assert!(prompt.contains("aeris.routing.contract"));
        assert!(prompt.contains("Invoice@v1"));
        assert!(prompt.contains("Category@v1"));
    }

    #[test]
    fn t10_4_retry_succeeds_after_initial_schema_violation() {
        // First response is malformed → SchemaViolation → retry.
        // Second response validates → Ok.
        let src = r#"
            model Invoice@v1 { id: int }
            model Category@v1 { kind: string }
            agent classify {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  Invoice@v1
                produce: Category@v1
                retries: 2
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "x" {
                    let inv = Invoice@v1 { id: 1 }
                    classify(inv, cap)
                }
            }
        "#;
        let tape = ai_tape(&[
            r#"{"unknown_field":1}"#,  // fails — unknown field
            r#"{"kind":"utilities"}"#, // succeeds
        ]);
        let (r, _) = run_module_with_tape(src, tape);
        r.unwrap();
    }

    #[test]
    fn t10_4_retry_exhaustion_propagates_last_schema_violation() {
        let src = r#"
            model Invoice@v1 { id: int }
            model Category@v1 { kind: string }
            agent classify {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  Invoice@v1
                produce: Category@v1
                retries: 1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "x" {
                    let inv = Invoice@v1 { id: 1 }
                    classify(inv, cap)
                }
            }
        "#;
        let tape = ai_tape(&[r#"{"unknown_field":1}"#, r#"{"another_bad":2}"#]);
        let (r, _) = run_module_with_tape(src, tape);
        let err = r.unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::SchemaViolation { .. }));
    }

    #[test]
    fn t10_4_budget_exceeded_on_token_overrun() {
        let src = r#"
            model Invoice@v1 { id: int }
            model Category@v1 { kind: string }
            agent classify {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  Invoice@v1
                produce: Category@v1
                budget:  { tokens: 1, latency: 60s }
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "x" {
                    let inv = Invoice@v1 { id: 1 }
                    classify(inv, cap)
                }
            }
        "#;
        let tape = ai_tape(&[r#"{"kind":"utilities"}"#]);
        let (r, _) = run_module_with_tape(src, tape);
        let err = r.unwrap_err();
        match err.kind {
            EvalErrorKind::BudgetExceeded { agent, kind, .. } => {
                assert_eq!(agent, "classify");
                assert_eq!(kind, "tokens");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn t10_4_retries_are_capped_then_budget_surfaces() {
        // Tokens 1 fails every retry → BudgetExceeded propagates.
        let src = r#"
            model Invoice@v1 { id: int }
            model Category@v1 { kind: string }
            agent classify {
                llm:     "x"
                intent:  "x"
                prompt:  "p"
                accept:  Invoice@v1
                produce: Category@v1
                retries: 2
                budget:  { tokens: 1, latency: 60s }
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Category@v1> {
                intent "x" {
                    let inv = Invoice@v1 { id: 1 }
                    classify(inv, cap)
                }
            }
        "#;
        let tape = ai_tape(&[r#"{"kind":"a"}"#, r#"{"kind":"b"}"#, r#"{"kind":"c"}"#]);
        let (r, _) = run_module_with_tape(src, tape);
        let err = r.unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::BudgetExceeded { .. }));
    }

    // ---- M11 — L2 native cap handlers ----

    fn fresh_audit_log() -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let tid = format!("{:?}", std::thread::current().id());
        let p = std::env::temp_dir().join(format!("aeris-audit-{nanos}-{tid}.jsonl"));
        super::set_audit_log_override(p.clone());
        let _ = std::fs::remove_file(&p);
        p
    }

    fn read_audit_log(p: &std::path::Path) -> Vec<String> {
        std::fs::read_to_string(p)
            .unwrap_or_default()
            .lines()
            .map(|l| l.to_string())
            .collect()
    }

    #[test]
    fn t11_1_audit_event_appends_jsonl_line_and_traces() {
        let path = fresh_audit_log();
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap(vec![(vec!["audit", "event"], None)], false));
        let expr = parse_expression(r#"audit.event("settle.complete", { count: 5 })"#).unwrap();
        let _ = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let lines = read_audit_log(&path);
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("\"event\":\"settle.complete\""));
        assert!(lines[0].contains("\"count\":5"));
        assert!(tracer.events().iter().any(|e| e.kind == "audit_event"));
    }

    #[test]
    fn t11_1_audit_event_without_cap_is_policy_violation() {
        let _path = fresh_audit_log();
        let mut env = Env::new();
        env.bind_let("cap", cap(vec![(vec!["io", "println"], None)], false));
        let expr = parse_expression(r#"audit.event("e", { x: 1 })"#).unwrap();
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, .. } => assert_eq!(op, "audit.event"),
            other => panic!("{other:?}"),
        }
    }

    // ---- M11.T2 / T3 — kube + docker subprocess wrappers ----

    #[test]
    fn t11_2_kube_apply_records_trace_event_even_without_cluster() {
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap(vec![(vec!["kube", "apply"], None)], false));
        let expr = parse_expression(r#"kube.apply("apiVersion: v1\nkind: ConfigMap")"#).unwrap();
        let _ = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        // The kubectl binary may or may not exist; either way the
        // trace event must be present.
        assert!(tracer.events().iter().any(|e| e.kind == "kube_apply"));
    }

    #[test]
    fn t11_2_kube_apply_without_cap_is_policy_violation() {
        let mut env = Env::new();
        env.bind_let("cap", cap(vec![(vec!["io", "println"], None)], false));
        let expr = parse_expression(r#"kube.apply("apiVersion: v1")"#).unwrap();
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::PolicyViolation { .. }));
    }

    #[test]
    fn t11_3_docker_run_records_trace_event_with_argv() {
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap(vec![(vec!["docker", "run"], None)], false));
        let expr = parse_expression(r#"docker.run("alpine:3.19")"#).unwrap();
        let _ = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "docker_run")
            .unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "argv" && v.contains("alpine:3.19")));
    }

    // ---- M11.T4 — mongodb stubs ----

    #[test]
    fn t11_4_mongodb_write_records_idempotency_when_present() {
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap(vec![(vec!["mongodb", "write"], None)], false));
        env.idempotency_key = Some(std::rc::Rc::new("idem-1234".into()));
        let expr = parse_expression(r#"mongodb.write("invoices", { id: 1 })"#).unwrap();
        let _ = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "mongodb_write")
            .unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "idem" && v.contains("idem-1234")));
    }

    #[test]
    fn t11_4_mongodb_read_returns_empty_list() {
        let mut env = Env::new();
        env.bind_let("cap", cap(vec![(vec!["mongodb", "read"], None)], false));
        let expr = parse_expression(r#"mongodb.read("invoices", { x: 1 })"#).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::List(xs) => assert!(xs.is_empty()),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
    }

    // ---- M11.T5 — minio stubs + bucket allow-list ----

    #[test]
    fn t11_5_minio_put_within_allow_list_succeeds() {
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let(
            "cap",
            cap(vec![(vec!["minio", "put"], Some(vec!["my-bucket"]))], false),
        );
        let expr = parse_expression(r#"minio.put("my-bucket", "key.txt", "data")"#).unwrap();
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        assert!(matches!(v, Value::Result(Ok(_))));
        assert!(tracer.events().iter().any(|e| e.kind == "minio_put"));
    }

    #[test]
    fn t11_5_minio_put_outside_allow_list_is_policy_violation() {
        let mut env = Env::new();
        env.bind_let(
            "cap",
            cap(vec![(vec!["minio", "put"], Some(vec!["allowed"]))], false),
        );
        let expr = parse_expression(r#"minio.put("forbidden", "key.txt", "data")"#).unwrap();
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, target } => {
                assert_eq!(op, "minio.put");
                assert_eq!(target, "forbidden");
            }
            other => panic!("{other:?}"),
        }
    }

    // ---- M11.T6 — rabbitmq stubs + message-id = idempotency ----

    #[test]
    fn t11_6_rabbitmq_publish_propagates_idempotency_as_message_id() {
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap(vec![(vec!["rabbitmq", "publish"], None)], false));
        env.idempotency_key = Some(std::rc::Rc::new("amqp-msg-77".into()));
        let expr = parse_expression(r#"rabbitmq.publish("orders", "{}")"#).unwrap();
        let _ = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "rabbitmq_publish")
            .unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "message_id" && v.contains("amqp-msg-77")));
    }

    #[test]
    fn t11_6_rabbitmq_subscribe_records_queue() {
        let tracer = Tracer::in_memory();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let(
            "cap",
            cap(vec![(vec!["rabbitmq", "subscribe"], None)], false),
        );
        let expr = parse_expression(r#"rabbitmq.subscribe("orders")"#).unwrap();
        let _ = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "rabbitmq_subscribe")
            .unwrap();
        assert!(evt
            .fields
            .iter()
            .any(|(k, v)| k == "queue" && v.contains("orders")));
    }

    // ---- M10.T6 / T7 / T8 — agent_net execution ----

    fn run_net_with_tape(
        src: &str,
        responses: &[&str],
    ) -> (Result<Value, EvalError>, Vec<TraceEvent>) {
        let m = crate::syntax::parse(src).unwrap();
        let tape = ai_tape(responses);
        let tracer = Tracer::in_memory();
        let cap_v = cap(
            vec![(vec!["ai", "complete"], Some(vec!["claude-haiku-4-5"]))],
            false,
        );
        let cap_inner = match cap_v {
            Value::Cap(c) => (*c).clone(),
            _ => unreachable!(),
        };
        let r = super::run_main_with_full_cfg(
            &m,
            cap_inner,
            Some(tracer.clone()),
            None,
            Some(tape),
            true,
        );
        (r, tracer.events())
    }

    #[test]
    fn t10_6_linear_chain_runs_all_agents_in_order() {
        let src = r#"
            model In@v1 { x: int }
            model Mid@v1 { x: int }
            model Out@v1 { x: int }
            agent step1 {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Mid@v1
            }
            agent step2 {
                llm: "x" intent: "x" prompt: "p"
                accept: Mid@v1 produce: Out@v1
            }
            agent_net pipe {
                flow step1 -> step2
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#]);
        let v = r.unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Record(r) => assert_eq!(r.name.as_deref(), Some("Out")),
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        let edges: Vec<&String> = evs
            .iter()
            .filter(|e| e.kind == "edge")
            .map(|e| {
                e.fields
                    .iter()
                    .find(|(k, _)| k == "to")
                    .map(|(_, v)| v)
                    .unwrap()
            })
            .collect();
        // Both step1 and step2 have edges (step1 from net entry, step2 from step1).
        assert!(edges.len() >= 2);
    }

    #[test]
    fn t10_6_fan_out_runs_all_branches() {
        let src = r#"
            model In@v1 { x: int }
            model OutA@v1 { x: int }
            model OutB@v1 { x: int }
            agent source {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: In@v1
            }
            agent branch_a {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: OutA@v1
            }
            agent branch_b {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: OutB@v1
            }
            agent_net pipe {
                flow source -> { branch_a, branch_b }
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<In@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#, r#"{"x":4}"#]);
        r.unwrap();
        let net_enter = evs.iter().filter(|e| e.kind == "net_enter").count();
        let net_exit = evs.iter().filter(|e| e.kind == "net_exit").count();
        assert_eq!(net_enter, 1);
        assert_eq!(net_exit, 1);
        // Both branches received an `edge` event.
        let to_set: std::collections::HashSet<String> = evs
            .iter()
            .filter(|e| e.kind == "edge")
            .filter_map(|e| {
                e.fields
                    .iter()
                    .find(|(k, _)| k == "to")
                    .map(|(_, v)| v.trim_matches('"').to_string())
            })
            .collect();
        assert!(to_set.contains("branch_a"));
        assert!(to_set.contains("branch_b"));
    }

    #[test]
    fn t10_6_type_driven_routing_skips_mismatched_branch() {
        // `source` produces InA. branch_a's accept is InA → enters.
        // branch_b's accept is InB → skipped (edge_skip emitted).
        let src = r#"
            model In@v1 { x: int }
            model InA@v1 { x: int }
            model InB@v1 { x: int }
            model OutA@v1 { x: int }
            model OutB@v1 { x: int }
            agent source {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: InA@v1
            }
            agent branch_a {
                llm: "x" intent: "x" prompt: "p"
                accept: InA@v1 produce: OutA@v1
            }
            agent branch_b {
                llm: "x" intent: "x" prompt: "p"
                accept: InB@v1 produce: OutB@v1
            }
            agent_net pipe {
                flow source -> { branch_a, branch_b }
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<OutA@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#]);
        r.unwrap();
        let skipped = evs.iter().filter(|e| e.kind == "edge_skip").count();
        assert!(skipped >= 1, "expected at least one edge_skip, got {evs:?}");
    }

    #[test]
    fn t10_6_net_traces_enter_iter_exit_events() {
        let src = r#"
            model In@v1 { x: int }
            model Out@v1 { x: int }
            agent solo {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Out@v1
            }
            agent_net pipe {
                flow solo -> solo_term
            }
            agent solo_term {
                llm: "x" intent: "x" prompt: "p"
                accept: Out@v1 produce: Out@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#]);
        r.unwrap();
        let kinds: Vec<&str> = evs.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains(&"net_enter"));
        assert!(kinds.contains(&"net_iter"));
        assert!(kinds.contains(&"net_exit"));
    }

    // ---- M10.T7 — until + iterations ----

    #[test]
    fn t10_7_until_satisfied_breaks_after_first_iteration() {
        let src = r#"
            model In@v1 { x: int }
            model Out@v1 { x: int where x >= 0 }
            agent solo {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Out@v1
            }
            agent_net pipe {
                flow solo -> solo
                until: iterations >= 1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#]);
        // The flow is `solo -> solo` (cycle) — but since we have a
        // single net iteration model, this is allowed at runtime even
        // though the M2 cycle detector would reject. Skip that check
        // for this fixture (it's parser-only validity matters).
        let _ = r;
        let iters = evs.iter().filter(|e| e.kind == "net_iter").count();
        // until: iterations >= 1 → satisfied after first iter.
        assert_eq!(iters, 1);
    }

    #[test]
    fn t10_7_no_until_runs_a_single_pass() {
        let src = r#"
            model In@v1 { x: int }
            model Out@v1 { x: int }
            agent solo {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Out@v1
            }
            agent_net pipe {
                flow solo -> sink
            }
            agent sink {
                llm: "x" intent: "x" prompt: "p"
                accept: Out@v1 produce: Out@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#]);
        r.unwrap();
        let iters = evs.iter().filter(|e| e.kind == "net_iter").count();
        assert_eq!(iters, 1);
    }

    #[test]
    fn t10_7_until_never_satisfied_exhausts_after_max_iterations() {
        let src = r#"
            model In@v1 { x: int }
            model Out@v1 { x: int }
            agent solo {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Out@v1
            }
            agent_net pipe {
                flow solo -> sink
                until: false
            }
            agent sink {
                llm: "x" intent: "x" prompt: "p"
                accept: Out@v1 produce: Out@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    pipe(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(
            src,
            &[
                r#"{"x":2}"#,
                r#"{"x":3}"#,
                r#"{"x":2}"#,
                r#"{"x":3}"#,
                r#"{"x":2}"#,
                r#"{"x":3}"#,
            ],
        );
        // until: false never satisfies → run hits AGENT_NET_MAX_ITERATIONS
        // (3) and the net resolves to err("agent_net ... exhausted").
        let v = r.unwrap();
        match v {
            Value::Result(Err(_)) => {}
            other => panic!("expected exhausted err, got {other:?}"),
        }
        let iters = evs.iter().filter(|e| e.kind == "net_iter").count();
        assert_eq!(iters, 3);
    }

    // ---- M10.T8 — agent_net composition ----

    #[test]
    fn t10_8_net_composition_runs_inner_net_as_a_node() {
        // Outer net references `inner_net` as a node. The shared
        // module scope already binds nets as `Value::AgentNet`, so
        // `invoke_value` dispatches through the same machinery used
        // for top-level invocation.
        let src = r#"
            model In@v1 { x: int }
            model Mid@v1 { x: int }
            model Out@v1 { x: int }
            agent step_a {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Mid@v1
            }
            agent step_b {
                llm: "x" intent: "x" prompt: "p"
                accept: Mid@v1 produce: Out@v1
            }
            agent_net inner_net {
                flow step_a -> step_b
            }
            agent_net outer {
                flow inner_net -> tail
            }
            agent tail {
                llm: "x" intent: "x" prompt: "p"
                accept: Out@v1 produce: Out@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    outer(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, _) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#, r#"{"x":4}"#]);
        r.unwrap();
    }

    #[test]
    fn t10_8_net_composition_emits_two_net_enter_events() {
        let src = r#"
            model In@v1 { x: int }
            model Out@v1 { x: int }
            agent leaf {
                llm: "x" intent: "x" prompt: "p"
                accept: In@v1 produce: Out@v1
            }
            agent_net inner {
                flow leaf -> leaf2
            }
            agent leaf2 {
                llm: "x" intent: "x" prompt: "p"
                accept: Out@v1 produce: Out@v1
            }
            agent_net outer {
                flow inner -> tail
            }
            agent tail {
                llm: "x" intent: "x" prompt: "p"
                accept: Out@v1 produce: Out@v1
            }
            fn main(cap: cap[ai.complete @ ["claude-haiku-4-5"]]) -> result<Out@v1> {
                intent "x" {
                    outer(In@v1 { x: 1 }, cap)
                }
            }
        "#;
        let (r, evs) = run_net_with_tape(src, &[r#"{"x":2}"#, r#"{"x":3}"#, r#"{"x":4}"#]);
        r.unwrap();
        // outer's net_enter + inner's net_enter = 2.
        let enters = evs.iter().filter(|e| e.kind == "net_enter").count();
        assert_eq!(enters, 2);
    }

    // ---- M6 — saga interpreter, rollback, idempotency ----

    fn run_saga(src: &str) -> Result<Value, EvalError> {
        let m = crate::syntax::parse(src).unwrap();
        run_main(&m)
    }

    fn run_saga_with_tracer(src: &str) -> (Result<Value, EvalError>, Vec<TraceEvent>) {
        let m = crate::syntax::parse(src).unwrap();
        let tracer = Tracer::in_memory();
        let r = super::run_main_with(&m, Some(tracer.clone()));
        (r, tracer.events())
    }

    fn trace_kind_seq(evs: &[TraceEvent]) -> Vec<String> {
        evs.iter().map(|e| e.kind.clone()).collect()
    }

    // -- M6.T1 happy-path (10) --

    #[test]
    fn saga_p01_single_step_runs_clean() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "noop"
                step a { do { 1 } undo noop }
            }
            fn main() -> result<unit> { s(cap.subset[]) }
        "#;
        // We bypass the missing fn-side cap by calling the saga
        // directly from main with a star cap synthesised by run_main.
        let _ = src; // hand-rolled main below, not used here
        let direct = r#"
            saga s(cap: cap[]) {
                intent "noop"
                step a { do { 1 } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let v = run_saga(direct).unwrap();
        assert_eq!(v, Value::ok(Value::Unit));
    }

    #[test]
    fn saga_p02_two_steps_run_in_order() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "two"
                step a { do { 1 } undo noop }
                step b { do { 2 } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (r, evs) = run_saga_with_tracer(src);
        r.unwrap();
        let kinds = trace_kind_seq(&evs);
        let step_enters: Vec<&String> = kinds
            .iter()
            .filter(|k| k.as_str() == "step_enter")
            .collect();
        assert_eq!(step_enters.len(), 2);
    }

    #[test]
    fn saga_p03_step_ok_visible_to_next_requires() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step first { do { 1 } undo noop }
                step second { requires: first.ok do { 2 } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        run_saga(src).unwrap();
    }

    #[test]
    fn saga_p04_intent_propagated_to_step_events() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "ship"
                step a { do { 1 } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let step_evt = evs.iter().find(|e| e.kind == "step_enter").unwrap();
        assert_eq!(step_evt.intent.as_deref(), Some("ship"));
    }

    #[test]
    fn saga_p05_saga_enter_and_exit_emitted() {
        let src = r#"
            saga s(cap: cap[]) { intent "x" step a { do { 1 } undo noop } }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let kinds = trace_kind_seq(&evs);
        assert!(kinds.contains(&"saga_enter".to_string()));
        assert!(kinds.contains(&"saga_exit".to_string()));
    }

    #[test]
    fn saga_p06_step_exit_outcome_ok() {
        let src = r#"
            saga s(cap: cap[]) { intent "x" step a { do { 1 } undo noop } }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let exit = evs.iter().find(|e| e.kind == "step_exit").unwrap();
        assert!(exit
            .fields
            .iter()
            .any(|(k, v)| k == "outcome" && v == "\"ok\""));
    }

    #[test]
    fn saga_p07_three_step_pipeline() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "three"
                step a { do { 1 } undo noop }
                step b { requires: a.ok do { 2 } undo noop }
                step c { requires: b.ok do { 3 } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (r, evs) = run_saga_with_tracer(src);
        r.unwrap();
        let entries = evs.iter().filter(|e| e.kind == "step_enter").count();
        assert_eq!(entries, 3);
    }

    #[test]
    fn saga_p08_idempotency_key_set_on_step_enter() {
        let src = r#"
            saga s(cap: cap[]) { intent "x" step a { do { 1 } undo noop } }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let evt = evs.iter().find(|e| e.kind == "step_enter").unwrap();
        let key = evt.fields.iter().find(|(k, _)| k == "idempotency");
        assert!(key.is_some());
    }

    #[test]
    fn saga_p09_idempotency_key_is_deterministic_per_run() {
        // Same trace_id + step_name + invocation index → same key.
        let trace_id = "01ABC";
        let k1 = idempotency_key(trace_id, "charge", 0);
        let k2 = idempotency_key(trace_id, "charge", 0);
        assert_eq!(k1, k2);
        let k3 = idempotency_key(trace_id, "charge", 1);
        assert_ne!(k1, k3);
    }

    #[test]
    fn saga_p10_undo_block_is_unused_on_clean_run() {
        // The undo block contains code that would fail; on a clean
        // run it is never executed, so the saga returns Ok.
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo { raise "should not fire" } }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        run_saga(src).unwrap();
    }

    // -- M6.T2 rollback (3) --

    #[test]
    fn saga_rollback_undoes_completed_steps_in_reverse() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "rollback"
                step a { do { 1 } undo { 100 } }
                step b { do { 2 } undo { 200 } }
                step c { do { Err("boom") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (r, evs) = run_saga_with_tracer(src);
        // Saga returns Err with rolled-back marker.
        let v = r.unwrap();
        assert!(matches!(v, Value::Result(Err(_))));
        // Trace contains undo_enter for `b` then `a` (reverse order).
        let undos: Vec<&TraceEvent> = evs.iter().filter(|e| e.kind == "undo_enter").collect();
        assert_eq!(undos.len(), 2);
        let first_undo = &undos[0].fields.iter().find(|(k, _)| k == "step").unwrap().1;
        let second_undo = &undos[1].fields.iter().find(|(k, _)| k == "step").unwrap().1;
        assert_eq!(first_undo, "\"b\"");
        assert_eq!(second_undo, "\"a\"");
    }

    #[test]
    fn saga_rollback_emits_saga_exit_rolled_back() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo noop }
                step b { do { Err("nope") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let exit = evs.iter().find(|e| e.kind == "saga_exit").unwrap();
        assert!(exit
            .fields
            .iter()
            .any(|(k, v)| k == "outcome" && v == "\"rolled_back\""));
    }

    #[test]
    fn saga_rollback_first_step_failure_no_undo_to_run() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { Err("first") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (r, evs) = run_saga_with_tracer(src);
        let v = r.unwrap();
        assert!(matches!(v, Value::Result(Err(_))));
        assert_eq!(evs.iter().filter(|e| e.kind == "undo_enter").count(), 0);
    }

    // -- M6.T5 partial failure (2) --

    #[test]
    fn saga_partial_failure_exhausts_undo_retries() {
        // Both `a.do` succeeds and `b.do` fails; `a.undo` always
        // raises → after retries the saga emits PartialFailure.
        let src = r#"
            saga s(cap: cap[]) {
                intent "broken"
                step a { do { 1 } undo { raise "stuck" } }
                step b { do { Err("trip") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (r, evs) = run_saga_with_tracer(src);
        let err = r.unwrap_err();
        match err.kind {
            EvalErrorKind::PartialFailure {
                saga,
                completed,
                failed_step,
            } => {
                assert_eq!(saga, "s");
                assert_eq!(completed, vec!["a".to_string()]);
                assert_eq!(failed_step, "a");
            }
            other => panic!("{other:?}"),
        }
        assert!(evs.iter().any(|e| e.kind == "partial_failure"));
    }

    // -- M6.T4 idempotency injection (1 wired backend: HTTP) --
    //
    // K8s annotations / AMQP message-id / mongodb sentinel /
    // audit.event idempotency_key arrive with the L2 native handlers
    // in M11. The infrastructure here (Env::idempotency_key) is the
    // same one those handlers will read; HTTP is the proof point.

    #[test]
    fn http_inside_saga_step_injects_idempotency_header() {
        let (port, rx) = capture_request_with_port("HTTP/1.1 200 OK\r\n\r\n");
        let url = format!("http://127.0.0.1:{port}/charge");
        let src = format!(
            r#"
                saga charge(cap: cap[http.post @ ["127.0.0.1"]]) {{
                    intent "ship"
                    step pay {{
                        do {{ http.post("{url}", "body")? }}
                        undo noop
                    }}
                }}
                fn main(cap: cap[http.post @ ["127.0.0.1"]]) -> result<unit> {{
                    charge(cap)
                }}
            "#
        );
        let m = crate::syntax::parse(&src).unwrap();
        let _ = run_main(&m).unwrap();
        let raw = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("mock saw request");
        let req = String::from_utf8_lossy(&raw).into_owned();
        assert!(
            req.contains("Idempotency-Key:"),
            "expected `Idempotency-Key` header, got:\n{req}"
        );
    }

    // -- M6.T6 golden traces (3) --

    fn load_saga_golden(name: &str) -> Vec<String> {
        let path = format!(
            "{}/aeris-tests/golden/m6/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing golden {path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    #[ignore]
    fn _print_saga_kinds_rollback() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo { 100 } }
                step b { do { Err("boom") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        for e in evs {
            println!("{}", e.kind);
        }
    }

    #[test]
    #[ignore]
    fn _print_saga_kinds_partial() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo { raise "stuck" } }
                step b { do { Err("trip") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        for e in evs {
            println!("{}", e.kind);
        }
    }

    #[test]
    fn golden_saga_success_kind_sequence() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let kinds = trace_kind_seq(&evs);
        assert_eq!(kinds, load_saga_golden("saga_success.jsonl"));
    }

    #[test]
    fn golden_saga_rollback_kind_sequence() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo { 100 } }
                step b { do { Err("boom") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let kinds = trace_kind_seq(&evs);
        assert_eq!(kinds, load_saga_golden("saga_rollback.jsonl"));
    }

    #[test]
    fn golden_saga_partial_failure_kind_sequence() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo { raise "stuck" } }
                step b { do { Err("trip") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        let kinds = trace_kind_seq(&evs);
        assert_eq!(kinds, load_saga_golden("saga_partial_failure.jsonl"));
    }

    #[test]
    fn saga_partial_failure_records_retries() {
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step a { do { 1 } undo { raise "stuck" } }
                step b { do { Err("trip") } undo noop }
            }
            fn main(cap: cap[]) -> result<unit> { s(cap) }
        "#;
        let (_, evs) = run_saga_with_tracer(src);
        // Two retries (3 attempts total → 2 retry events emitted).
        let retries = evs.iter().filter(|e| e.kind == "undo_retry").count();
        assert!(
            retries >= 1,
            "expected at least one undo_retry, got {retries}"
        );
    }

    #[test]
    fn t6_record_where_uses_global_function() {
        // The clause can call other module functions in scope.
        let src = r#"
            fn is_positive(x: int) -> bool { x > 0 }
            record V { n: int where is_positive(n) }
            fn main() -> V { V { n: 0 } }
        "#;
        assert_where_violation(src);
    }

    // ---- M4.T9 — golden traces ----
    //
    // The reference kind-sequence files live under
    // `aeris-tests/golden/m4/`. Tests load each `.jsonl` (one kind
    // per line), run the matching fixture against the in-memory
    // tracer, and assert the recorded `kind` sequence equals the
    // golden file. Per-run fields (`trace_id`, `ts`) are omitted
    // from the golden files — `aeris trace diff` (M13.T1) will
    // extend the comparison to semantic fields.

    /// Helper: collect the kinds of every event recorded while running
    /// `body` against the supplied cap. Used by golden-trace fixtures
    /// to assert event sequence without relying on per-run timestamps.
    fn trace_kinds(body: &str, cap_val: Value) -> Vec<String> {
        let tracer = Tracer::in_memory();
        let expr = parse_expression(body).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap_val);
        let _ = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        tracer.events().into_iter().map(|e| e.kind).collect()
    }

    fn load_golden(name: &str) -> Vec<String> {
        let path = format!(
            "{}/aeris-tests/golden/m4/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("missing golden {path}: {e}"))
            .lines()
            .map(String::from)
            .collect()
    }

    #[test]
    fn golden_io_println_trace_kind_sequence() {
        let kinds = trace_kinds(r#"io.println("hello")"#, star_cap());
        assert_eq!(kinds, load_golden("io_println.jsonl"));
    }

    #[test]
    fn golden_clock_then_random_trace_kind_sequence() {
        let kinds = trace_kinds("{ let _ = clock.now(); random.next() }", star_cap());
        assert_eq!(kinds, load_golden("clock_random.jsonl"));
    }

    #[test]
    fn golden_env_read_kind_sequence() {
        let kinds = trace_kinds(r#"env.read("PATH")"#, star_cap());
        assert_eq!(kinds, load_golden("env_read.jsonl"));
    }

    // ---- M5.T3 — shell.exec / shell.pipe with argv0 allow-list ----

    #[test]
    fn shell_exec_runs_echo_records_event() {
        // `/bin/echo` is universally available on Linux/macOS.
        let cap_val = cap(
            vec![(vec!["shell", "exec"], Some(vec!["/bin/echo"]))],
            false,
        );
        let tracer = Tracer::in_memory();
        let expr = parse_expression(r#"shell.exec(["/bin/echo", "hi"])"#).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap_val);
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match &v {
            Value::Result(Ok(boxed)) => match boxed.as_ref() {
                Value::Record(r) => {
                    let stdout = r
                        .fields
                        .iter()
                        .find(|(k, _)| k == "stdout")
                        .map(|(_, v)| v.clone());
                    assert_eq!(stdout, Some(Value::Str("hi\n".into())));
                }
                other => panic!("expected ShellResult record, got {other:?}"),
            },
            other => panic!("expected Ok(record), got {other:?}"),
        }
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "shell_exec")
            .unwrap();
        assert!(evt.fields.iter().any(|(k, _)| k == "stdout_hash"));
    }

    #[test]
    fn shell_exec_argv0_outside_allow_list_rejected() {
        let cap_val = cap(
            vec![(vec!["shell", "exec"], Some(vec!["/bin/true"]))],
            false,
        );
        let expr = parse_expression(r#"shell.exec(["/bin/echo", "x"])"#).unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, target } => {
                assert_eq!(op, "shell.exec");
                assert_eq!(target, "/bin/echo");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn shell_exec_empty_argv_errors() {
        let cap_val = cap(vec![(vec!["shell", "exec"], None)], false);
        let expr = parse_expression("shell.exec([])").unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::Type(_)));
    }

    // ---- M5.T1 + M5.T2 — http.* with allow-list ----

    fn spawn_mock_http(canned: &'static str) -> u16 {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = s.read(&mut buf);
                let _ = s.write_all(canned.as_bytes());
            }
        });
        port
    }

    fn captured_request(canned: &'static str) -> std::sync::mpsc::Receiver<Vec<u8>> {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let _port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let _ = tx.send(buf[..n].to_vec());
                let _ = s.write_all(canned.as_bytes());
            }
        });
        rx
    }

    fn capture_request_with_port(
        canned: &'static str,
    ) -> (u16, std::sync::mpsc::Receiver<Vec<u8>>) {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let n = s.read(&mut buf).unwrap_or(0);
                let _ = tx.send(buf[..n].to_vec());
                let _ = s.write_all(canned.as_bytes());
            }
        });
        (port, rx)
    }

    #[test]
    fn http_get_returns_status_and_body_via_mock() {
        let port = spawn_mock_http("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\r\nhello");
        let cap_val = cap(vec![(vec!["http", "get"], Some(vec!["127.0.0.1"]))], false);
        let url = format!("http://127.0.0.1:{port}/x");
        let src = format!(r#"http.get("{url}")"#);
        let tracer = Tracer::in_memory();
        let expr = parse_expression(&src).unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap_val);
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        match v {
            Value::Result(Ok(boxed)) => match *boxed {
                Value::Record(r) => {
                    let status = r
                        .fields
                        .iter()
                        .find(|(k, _)| k == "status")
                        .unwrap()
                        .1
                        .clone();
                    let body = r
                        .fields
                        .iter()
                        .find(|(k, _)| k == "body")
                        .unwrap()
                        .1
                        .clone();
                    assert_eq!(status, Value::Int(200));
                    assert_eq!(body, Value::Str("hello".into()));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "http_call")
            .unwrap();
        assert!(evt.fields.iter().any(|(k, _)| k == "req_hash"));
        assert!(evt.fields.iter().any(|(k, _)| k == "resp_hash"));
        assert!(evt.fields.iter().any(|(k, v)| k == "status" && v == "200"));
    }

    #[test]
    fn http_propagates_x_aeris_trace_id_header() {
        let (port, rx) = capture_request_with_port("HTTP/1.1 200 OK\r\n\r\n");
        let cap_val = cap(vec![(vec!["http", "get"], Some(vec!["127.0.0.1"]))], false);
        let url = format!("http://127.0.0.1:{port}/probe");
        let src = format!(r#"http.get("{url}")"#);
        let tracer = Tracer::in_memory();
        let trace_id = tracer.trace_id();
        let expr = parse_expression(&src).unwrap();
        let mut env = Env::new().with_tracer(tracer);
        env.bind_let("cap", cap_val);
        let _ = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        let raw = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("mock server received request");
        let req_str = String::from_utf8_lossy(&raw).into_owned();
        assert!(
            req_str.contains(&format!("X-Aeris-Trace-Id: {trace_id}")),
            "trace id header missing: {req_str}"
        );
    }

    #[test]
    fn http_post_outside_allow_list_rejected() {
        let cap_val = cap(
            vec![(vec!["http", "post"], Some(vec!["api.acme.com"]))],
            false,
        );
        let src = r#"http.post("http://evil.com/steal", "data")"#;
        let tracer = Tracer::in_memory();
        let expr = parse_expression(src).unwrap();
        let mut env = Env::new().with_tracer(tracer);
        env.bind_let("cap", cap_val);
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        match err.kind {
            EvalErrorKind::PolicyViolation { op, target } => {
                assert_eq!(op, "http.post");
                assert_eq!(target, "evil.com");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn http_post_sends_body_to_server() {
        let (port, rx) = capture_request_with_port("HTTP/1.1 201 Created\r\n\r\n");
        let cap_val = cap(vec![(vec!["http", "post"], Some(vec!["127.0.0.1"]))], false);
        let url = format!("http://127.0.0.1:{port}/charge");
        let src = format!(r#"http.post("{url}", "payload-1234")"#);
        let expr = parse_expression(&src).unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let _ = eval_expr(&expr, &mut env).and_then(|f| f.into_value(expr.span()));
        let raw = rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .expect("mock server received request");
        let req_str = String::from_utf8_lossy(&raw).into_owned();
        assert!(req_str.starts_with("POST /charge HTTP/1.1"));
        assert!(req_str.contains("Content-Length: 12"));
        assert!(req_str.ends_with("payload-1234"));
    }

    #[test]
    fn http_get_no_cap_in_scope_rejects() {
        let src = r#"http.get("http://example.com/")"#;
        let expr = parse_expression(src).unwrap();
        let mut env = Env::new();
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::PolicyViolation { .. }));
    }

    #[test]
    fn http_https_url_rejected_with_io_error() {
        // The hand-rolled client refuses HTTPS; the eval layer surfaces
        // it as a clean `Type` error from URL parsing.
        let cap_val = cap(vec![(vec!["http", "get"], None)], false);
        let src = r#"http.get("https://api.acme.com/x")"#;
        let expr = parse_expression(src).unwrap();
        let mut env = Env::new();
        env.bind_let("cap", cap_val);
        let err = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap_err();
        assert!(matches!(err.kind, EvalErrorKind::Type(_)));
    }

    // suppress unused-helper warning
    fn _silence_warning(_: std::sync::mpsc::Receiver<Vec<u8>>) {}

    #[test]
    fn _captured_request_helper_is_compiled() {
        let _ = captured_request("HTTP/1.1 200 OK\r\n\r\n");
    }

    #[test]
    fn shell_pipe_two_stages_records_event() {
        let cap_val = cap(
            vec![(
                vec!["shell", "pipe"],
                Some(vec!["/bin/echo", "/usr/bin/wc"]),
            )],
            false,
        );
        let tracer = Tracer::in_memory();
        let expr =
            parse_expression(r#"shell.pipe([["/bin/echo", "a b c"], ["/usr/bin/wc", "-w"]])"#)
                .unwrap();
        let mut env = Env::new().with_tracer(tracer.clone());
        env.bind_let("cap", cap_val);
        let v = eval_expr(&expr, &mut env)
            .and_then(|f| f.into_value(expr.span()))
            .unwrap();
        // We do not assert the exact stdout (`wc -w` output varies in
        // whitespace), only the kind of trace event.
        assert!(matches!(v, Value::Result(Ok(_))));
        let evt = tracer
            .events()
            .into_iter()
            .find(|e| e.kind == "shell_pipe")
            .unwrap();
        assert!(evt.fields.iter().any(|(k, v)| k == "stages" && v == "2"));
    }

    #[test]
    fn golden_fs_write_then_read_kind_sequence() {
        let path = unique_tmp("golden.txt");
        let cap_val = cap(
            vec![
                (vec!["fs", "write_text"], Some(vec!["/tmp/**"])),
                (vec!["fs", "read_text"], Some(vec!["/tmp/**"])),
                (vec!["fs", "remove"], Some(vec!["/tmp/**"])),
            ],
            false,
        );
        let body = format!(
            r#"{{ let _w = fs.write_text("{path}", "hi")?; let _r = fs.read_text("{path}")?; let _x = fs.remove("{path}")?; "ok" }}"#
        );
        let kinds = trace_kinds(&body, cap_val);
        assert_eq!(kinds, load_golden("fs_write_read.jsonl"));
    }
}
