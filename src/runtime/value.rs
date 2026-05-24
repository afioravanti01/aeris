//! Runtime value representation (M3.T1 / M3.T4).
//!
//! Realises `docs/language.md` § 4 (types) at the value level. Every
//! Aeris value is one variant of `Value`. The representation is
//! immutable — mutation is a fresh copy (§ 4.3 records-by-value).
//!
//! `Value` round-trips through JSON via `runtime::json::encode` /
//! `runtime::json::decode`. The format is self-tagging so that any
//! value (including unit, decimals, dates, records and enums) can be
//! re-parsed losslessly without external type information.
//!
//! Closures (M3.T4) are first-class values too — their structure is
//! defined here so the evaluator can pass them around like data.

/// One Aeris runtime value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `()` — the empty tuple.
    Unit,
    Bool(bool),
    /// Platform-sized signed; the lexer rejects literals outside `i64`.
    Int(i64),
    Float(f64),
    /// Arbitrary-precision fixed-point. Stored as the canonical decimal
    /// string until M5 introduces a real big-decimal backend.
    Decimal(String),
    Str(String),
    Bytes(Vec<u8>),
    Char(char),
    Uuid(String),
    /// Civil date `YYYY-MM-DD` (§ 2.4).
    Date(String),
    /// UTC instant `YYYY-MM-DDThh:mm:ss[.sss](Z|±hh:mm)` (§ 2.4).
    Timestamp(String),
    /// Duration literal preserved verbatim (`3s`, `500ms`, ...).
    Duration(String),

    /// Ordered, growable list (§ 4.2).
    List(Vec<Value>),
    /// Hash-set semantics — represented as a deduplicated `Vec` until
    /// M3.T2 introduces real hashing for the runtime.
    Set(Vec<Value>),
    /// Insertion-ordered key/value pairs.
    Map(Vec<(Value, Value)>),
    /// Heterogeneous fixed-arity tuple.
    Tuple(Vec<Value>),

    /// `option<T>` — `None` or `Some(v)`.
    Option(Option<Box<Value>>),
    /// `result<T>` — `Ok(v)` or `Err(err)`. The `err` shape is itself
    /// a `Value` (typically an `Enum` of `err`).
    Result(Result<Box<Value>, Box<Value>>),

    /// `record User { ... }` instance.
    Record(RecordValue),
    /// `enum Status { Active(timestamp) }` instance.
    Enum(EnumValue),
    /// First-class function value — a lambda or a top-level `fn`
    /// closed over the environment of its definition (§ 7.3, M3.T4).
    Closure(std::rc::Rc<Closure>),
    /// Capability value (§ 8.4, M4.T2). Carries the effect signature
    /// and allow-lists; `cap.subset[..]` derives a narrower value
    /// from this one. The `star` bit marks `main`'s synthesised cap
    /// (allowed only at the entry point).
    Cap(std::rc::Rc<CapValue>),
    /// A saga declaration captured as a callable value (M6.T1). The
    /// saga interpreter — not the regular closure invoker — handles
    /// forward execution, rollback, and the idempotency-key
    /// derivation (N1 / § 12).
    Saga(std::rc::Rc<SagaInstance>),
    /// An `agent` declaration captured as a callable value (M10.T2).
    /// Calling an agent validates the input against `accept`, sends
    /// the prompt + auto-injected contract through the configured
    /// `ai.complete` backend, and validates the JSON response
    /// against `produce` (§ 13.2).
    Agent(std::rc::Rc<AgentInstance>),
    /// An `agent_net` declaration captured as a callable value
    /// (M10.T6). Invocation runs the DAG to convergence, validating
    /// schemas at every edge crossing (§ 14).
    AgentNet(std::rc::Rc<AgentNetInstance>),
}

/// Runtime representation of an `agent_net` declaration. Composition
/// (a net referencing another net as a node) is resolved at call time
/// via the shared `module` scope.
#[derive(Clone)]
pub struct AgentNetInstance {
    pub name: String,
    pub intent: Option<String>,
    pub flows: Vec<crate::syntax::ast::FlowDecl>,
    pub until: Option<crate::syntax::ast::Expr>,
    pub module: Option<ModuleScope>,
    pub tracer: Option<crate::runtime::trace::Tracer>,
    pub model_decls: Option<
        std::rc::Rc<std::collections::HashMap<(String, u32), crate::syntax::ast::ModelDecl>>,
    >,
    pub ai_backend: Option<std::rc::Rc<crate::manifest::AiBackend>>,
    pub replay_tape: Option<crate::runtime::replay::TapeHandle>,
    pub full_record: bool,
    /// M33: see `SagaInstance.imported_modules`.
    pub imported_modules: std::rc::Rc<std::collections::HashSet<String>>,
    /// M22.T1: per-family L2 backend table snapshot, propagated
    /// into every step body so backend selection is stable across
    /// the agent_net's lifetime.
    pub l2_backends: std::rc::Rc<crate::runtime::l2_backend::L2Backends>,
}

impl std::fmt::Debug for AgentNetInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentNetInstance")
            .field("name", &self.name)
            .field("flows", &self.flows.len())
            .finish()
    }
}

impl PartialEq for AgentNetInstance {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Runtime representation of an `agent` declaration.
#[derive(Clone)]
pub struct AgentInstance {
    pub name: String,
    pub llm: String,
    pub intent: String,
    pub prompt: String,
    pub accept: (String, u32),
    pub produce: (String, u32),
    pub policy_names: Vec<String>,
    pub retries: u32,
    pub budget_tokens: Option<u64>,
    pub budget_latency_ms: Option<u64>,
    pub module: Option<ModuleScope>,
    pub tracer: Option<crate::runtime::trace::Tracer>,
    pub model_decls: Option<
        std::rc::Rc<std::collections::HashMap<(String, u32), crate::syntax::ast::ModelDecl>>,
    >,
    pub ai_backend: Option<std::rc::Rc<crate::manifest::AiBackend>>,
    pub replay_tape: Option<crate::runtime::replay::TapeHandle>,
    pub full_record: bool,
    /// M33: see `SagaInstance.imported_modules`.
    pub imported_modules: std::rc::Rc<std::collections::HashSet<String>>,
    /// M22.T1: per-family L2 backend table snapshot.
    pub l2_backends: std::rc::Rc<crate::runtime::l2_backend::L2Backends>,
}

impl std::fmt::Debug for AgentInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentInstance")
            .field("name", &self.name)
            .field("llm", &self.llm)
            .field("accept", &self.accept)
            .field("produce", &self.produce)
            .finish()
    }
}

impl PartialEq for AgentInstance {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Runtime representation of a `saga` declaration. Sagas are callable
/// like fns (`saga settle(cap)` → `settle(cap)`); the interpreter
/// re-enters the saga driver on each invocation.
#[derive(Clone)]
pub struct SagaInstance {
    pub name: String,
    pub params: Vec<String>,
    pub intent: String,
    pub steps: Vec<crate::syntax::ast::SagaStep>,
    pub module: Option<ModuleScope>,
    pub tracer: Option<crate::runtime::trace::Tracer>,
    pub stdin: Option<std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>>,
    pub record_decls:
        Option<std::rc::Rc<std::collections::HashMap<String, crate::syntax::ast::RecordDecl>>>,
    pub model_decls: Option<
        std::rc::Rc<std::collections::HashMap<(String, u32), crate::syntax::ast::ModelDecl>>,
    >,
    pub policies: Option<std::rc::Rc<Vec<crate::syntax::ast::PolicyDecl>>>,
    pub ai_backend: Option<std::rc::Rc<crate::manifest::AiBackend>>,
    pub replay_tape: Option<crate::runtime::replay::TapeHandle>,
    pub full_record: bool,
    /// M33: module names brought into scope by `use` declarations
    /// in the file that declares this saga. Propagated to step
    /// bodies so `<module>.<op>(...)` is gated the same way as
    /// inside a regular `fn`.
    pub imported_modules: std::rc::Rc<std::collections::HashSet<String>>,
    /// M22.T1: per-family L2 backend table snapshot — every step
    /// body invokes the same backends as the call site.
    pub l2_backends: std::rc::Rc<crate::runtime::l2_backend::L2Backends>,
}

impl std::fmt::Debug for SagaInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SagaInstance")
            .field("name", &self.name)
            .field("steps", &self.steps.len())
            .finish()
    }
}

impl PartialEq for SagaInstance {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

/// Runtime representation of a capability. `entries` is the flat list
/// of `(module, op, allow_list)` triples authorised by this cap.
/// `star = true` for `main`'s synthesised cap; user code can never
/// hold a `star` cap (M2.T5 rejects the construct).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapValue {
    pub entries: Vec<CapEntryValue>,
    pub star: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapEntryValue {
    /// One or two segments — `["fs"]` covers `fs.*`; `["fs", "read_file"]`
    /// is the leaf form.
    pub path: Vec<String>,
    /// `None` means "every endpoint of this op is allowed". `Some(xs)`
    /// is the explicit allow-list (single-element OK).
    pub allow: Option<Vec<String>>,
}

impl CapValue {
    /// `true` iff `child` is authorised by `parent` — every entry of
    /// `child` must be covered by some entry of `parent`, and every
    /// allow-list element of the child must be in the parent's
    /// (or the parent must list `None`, meaning unbounded).
    pub fn covers(&self, child: &CapValue) -> Result<(), CapNarrowError> {
        if child.star {
            return Err(CapNarrowError::ChildHasStar);
        }
        if self.star {
            return Ok(());
        }
        for c in &child.entries {
            if !self.entry_covers(c) {
                return Err(CapNarrowError::EntryNotInParent {
                    op: c.path.join("."),
                });
            }
        }
        Ok(())
    }

    fn entry_covers(&self, child: &CapEntryValue) -> bool {
        for p in &self.entries {
            if path_covers(&p.path, &child.path) && allow_covers(&p.allow, &child.allow) {
                return true;
            }
        }
        false
    }
}

fn path_covers(parent: &[String], child: &[String]) -> bool {
    // `["fs"]` covers any `["fs", op]`; equality is also OK.
    if parent.len() == 1 {
        child.first() == parent.first()
    } else {
        parent == child
    }
}

fn allow_covers(parent: &Option<Vec<String>>, child: &Option<Vec<String>>) -> bool {
    match (parent, child) {
        (None, _) => true,        // parent is unbounded → covers everything
        (Some(_), None) => false, // parent narrows → child must too
        (Some(p), Some(c)) => c.iter().all(|x| p.contains(x)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapNarrowError {
    ChildHasStar,
    EntryNotInParent { op: String },
}

/// Shared module-level scope. Top-level `fn` declarations populate
/// this map after every closure has been created so that they can
/// refer to one another (and themselves, for recursion).
pub type ModuleScope = std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, Value>>>;

/// A captured callable. The evaluator holds these by `Rc` so closures
/// can be passed and stored by value without cloning the body. The
/// `captured` field is a snapshot of the lexical binding stack at
/// definition time — closures capture by value (a `var` mutation
/// outside the closure does not leak in). `module` is a shared
/// pointer to the module's top-level fn registry; it lets a closure
/// resolve a module fn that was defined after itself.
#[derive(Clone)]
pub struct Closure {
    pub params: Vec<String>,
    pub body: crate::syntax::ast::Block,
    pub captured: Vec<std::collections::HashMap<String, Value>>,
    pub module: Option<ModuleScope>,
    pub tracer: Option<crate::runtime::trace::Tracer>,
    pub stdin: Option<std::rc::Rc<std::cell::RefCell<std::collections::VecDeque<String>>>>,
    pub record_decls:
        Option<std::rc::Rc<std::collections::HashMap<String, crate::syntax::ast::RecordDecl>>>,
    pub model_decls: Option<
        std::rc::Rc<std::collections::HashMap<(String, u32), crate::syntax::ast::ModelDecl>>,
    >,
    pub policies: Option<std::rc::Rc<Vec<crate::syntax::ast::PolicyDecl>>>,
    pub ai_backend: Option<std::rc::Rc<crate::manifest::AiBackend>>,
    pub replay_tape: Option<crate::runtime::replay::TapeHandle>,
    pub full_record: bool,
    /// M33: module names brought into scope by `use` declarations in
    /// the file that defines this closure. Propagated through every
    /// call so a closure invoked in another env keeps its own import
    /// rules.
    pub imported_modules: std::rc::Rc<std::collections::HashSet<String>>,
    /// M22.T1: per-family L2 backend table snapshot at definition
    /// time. Re-entering the closure uses the same backends even if
    /// the caller swapped its own table.
    pub l2_backends: std::rc::Rc<crate::runtime::l2_backend::L2Backends>,
    /// `requires:` clauses checked at function entry (M5.T4 / § 9.1).
    /// Lambdas have an empty list — only top-level fns carry contracts.
    pub requires: Vec<crate::syntax::ast::Expr>,
    /// `ensures:` clauses checked at function exit, with the special
    /// `result` binding set to the returned value (§ 9.1).
    pub ensures: Vec<crate::syntax::ast::Expr>,
    /// `Some("name")` for a top-level `fn`; `None` for a lambda.
    pub name: Option<String>,
}

impl std::fmt::Debug for Closure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Closure")
            .field("params", &self.params)
            .field("name", &self.name)
            .finish()
    }
}

impl PartialEq for Closure {
    fn eq(&self, other: &Self) -> bool {
        // Identity equality via pointer — closures are not structurally
        // comparable (capture environments are opaque).
        std::ptr::eq(self, other)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordValue {
    /// `Some("User")` for nominal records, `None` for the anonymous
    /// `{ a: 1, b: 2 }` literal form (§ 2.4).
    pub name: Option<String>,
    /// Fields in declaration order.
    pub fields: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumValue {
    /// Enclosing enum name, e.g. `"Status"`.
    pub name: String,
    /// Variant name, e.g. `"Active"`.
    pub variant: String,
    pub data: VariantValue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantValue {
    Unit,
    Tuple(Vec<Value>),
    Record(Vec<(String, Value)>),
}

impl Value {
    /// Convenience constructor for `Some(v)`.
    pub fn some(v: Value) -> Value {
        Value::Option(Some(Box::new(v)))
    }
    /// Convenience constructor for `None`.
    pub fn none() -> Value {
        Value::Option(None)
    }
    /// Convenience constructor for `Ok(v)`.
    pub fn ok(v: Value) -> Value {
        Value::Result(Ok(Box::new(v)))
    }
    /// Convenience constructor for `Err(e)`.
    pub fn err(e: Value) -> Value {
        Value::Result(Err(Box::new(e)))
    }
}
