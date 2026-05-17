//! Aeris AST.
//!
//! Realises `docs/language.md` §§ 4 (types), 5 (values & expressions),
//! 6 (control flow), 7 (functions), 16 (models), 17 (patterns),
//! and § 26 (grammar). Function bodies are still captured as `RawSpan`
//! by `parse_module`; the expression AST below is reached via
//! `parser::parse_expression` (M1.T6). M1.T7 parses `cap[..]`
//! allow-lists.

use super::token::Span;

/// A parsed source module: optional `use` lines followed by item declarations.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub uses: Vec<UseDecl>,
    pub items: Vec<Item>,
}

/// A range of tokens captured by the parser for a phase that hasn't run yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawSpan {
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Private,
}

// ----- top-level items -----

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Fn(FnDecl),
    Record(RecordDecl),
    Enum(EnumDecl),
    Model(ModelDecl),
    TypeAlias(TypeAliasDecl),
    Const(ConstDecl),
    Saga(SagaDecl),
    Agent(AgentDecl),
    AgentNet(AgentNetDecl),
    Policy(PolicyDecl),
    Test(TestDecl),
    Property(PropertyDecl),
    /// M26 — top-level statement. Executes during module load,
    /// before `main` (or as the program body when `main` is absent).
    /// Allows `let X = ...`, `env.set(...)`, `fs.mkdir(...)` outside
    /// any `fn`. Module-level `var` remains forbidden.
    TopStmt(Box<crate::syntax::ast::Stmt>),
}

/// Top-level `test "<name>" { <body> }` declaration (§ 21.1).
/// File-as-suite discovery is handled by the runner (M12.T1):
/// every `test` in `tests/foo.test.aer` belongs to the suite `foo`.
///
/// `fixture` — § 21.4 / M12.T4: an optional recording id. When set
/// the runner loads `tests/fixtures/<id>.jsonl` before evaluating
/// the body and exposes it via the `trace()` builtin so the test
/// can assert against the recorded events (e.g. saga rollback).
#[derive(Debug, Clone, PartialEq)]
pub struct TestDecl {
    pub name: String,
    pub fixture: Option<String>,
    pub body: Block,
    pub span: Span,
}

/// Top-level `property "<name>" with (<a>: <T>, ...) { <body> }`
/// declaration (§ 21.3). The runner samples values for the named
/// generators (200 cases by default), evaluates the body, and on the
/// first counter-example shrinks the input and persists the seed to
/// `tests/fixtures/<id>.json` (M12.T3).
#[derive(Debug, Clone, PartialEq)]
pub struct PropertyDecl {
    pub name: String,
    /// Generator parameters — `name: type`. The type drives the
    /// generator selection at runtime.
    pub params: Vec<Param>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub raw: RawSpan,
    /// Module-level names this `use` introduces into the file's
    /// scope. Filled by `parse_use` (M33.T1). Examples:
    /// - `use io, fs`               → `["io", "fs"]`
    /// - `use utils from "./x.aer"` → `["utils"]`
    /// - `use http as net`          → `["net"]`
    /// - `use "./x.aer"`            → `[]` (anonymous path import)
    /// - `use { a, b } from utils`  → `[]` (selective re-export)
    pub imported_names: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub return_ty: Option<Type>,
    /// `requires: <expr>` clauses, in source order (§ 9.1). Checked at
    /// function entry by the runtime (M5.T4).
    pub requires: Vec<Expr>,
    /// `ensures: <expr>` clauses, in source order. Each may reference
    /// the special identifier `result` for the returned value.
    pub ensures: Vec<Expr>,
    /// Names of policies activated by attribute on this fn (M8.T5 /
    /// § 15.3). Module-declared policies are always active; this list
    /// adds attribute-scoped activations on top.
    pub policy_attrs: Vec<String>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordDecl {
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub fields: Vec<RecordField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordField {
    pub name: String,
    pub ty: Type,
    /// Optional `where <expr>` refinement evaluated at construction
    /// (M5.T6 / § 9.1). The expression sees the field name in scope.
    pub where_clause: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub data: VariantData,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantData {
    Unit,
    Tuple(Vec<Type>),
    Record(Vec<RecordField>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelDecl {
    pub vis: Visibility,
    pub name: String,
    pub version: u32,
    pub fields: Vec<RecordField>,
    /// Record-level `where: <expr>` invariants checked after every
    /// per-field where (M5.T6 / § 16.3). All field bindings are in
    /// scope when each invariant evaluates.
    pub record_where: Vec<Expr>,
    /// M23 — optional parent reference. `model X@v2 extends X@v1 { ... }`
    /// stores `Some(("X", 1))` here; the runtime merges the parent's
    /// fields into `fields` after parse.
    pub extends: Option<(String, u32)>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeAliasDecl {
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub aliased: Type,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub vis: Visibility,
    pub name: String,
    pub ty: Option<Type>,
    pub init: RawSpan,
    pub span: Span,
}

// ----- saga / agent / agent_net / policy (M1.T8) -----

/// `saga <name>(<params>) { intent "..." <step>+ }` (§ 12.1).
#[derive(Debug, Clone, PartialEq)]
pub struct SagaDecl {
    pub vis: Visibility,
    pub name: String,
    pub params: Vec<Param>,
    /// Saga-level intent string (§ 12.2: mandatory, exactly one).
    pub intent: String,
    pub steps: Vec<SagaStep>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SagaStep {
    pub name: String,
    /// `requires:` clauses, parsed as expressions (M1.T9). Common shape
    /// is a previous-step `<step>.ok` reference (§ 12).
    pub requires: Vec<Expr>,
    pub do_block: Block,
    pub undo: UndoForm,
    pub span: Span,
}

/// `undo` is either a block or the literal keyword `noop` per § 12.2.
#[derive(Debug, Clone, PartialEq)]
pub enum UndoForm {
    Block(Block),
    Noop(Span),
}

/// `agent <name> { <field>+ }` (§ 13.1). Field semantics are validated
/// by `check::` (M2.T*); the parser accepts any `<ident>: <expr>` pair.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentDecl {
    pub vis: Visibility,
    pub name: String,
    pub fields: Vec<DeclField>,
    pub span: Span,
}

/// `agent_net <name> { intent? <flow>+ <until>? }` (§ 14.1).
#[derive(Debug, Clone, PartialEq)]
pub struct AgentNetDecl {
    pub vis: Visibility,
    pub name: String,
    pub intent: Option<String>,
    pub flows: Vec<FlowDecl>,
    /// `until: <expr>` — optional iteration predicate (§ 14.3).
    pub until: Option<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FlowDecl {
    /// `a -> b -> c` or `a -> { b, c }`. The pipeline is encoded as a
    /// sequence of stages, each either a single agent name or a fan-out
    /// branch set.
    pub stages: Vec<FlowStage>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FlowStage {
    Single(String),
    FanOut(Vec<String>),
}

/// `policy <name> { <field>+ }` (§ 15.1). Field keys are
/// `match | deny | require | limit | audit | when` — checked in M8.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyDecl {
    pub vis: Visibility,
    pub name: String,
    pub fields: Vec<DeclField>,
    pub span: Span,
}

/// A `<key>: <expr>(, <expr>)*` pair common to `agent { ... }` and
/// `policy { ... }`. The value is a list to encode `policy: a, b` and
/// the like; single-value fields produce a one-element vector.
#[derive(Debug, Clone, PartialEq)]
pub struct DeclField {
    pub key: String,
    pub values: Vec<Expr>,
    pub span: Span,
}

// ----- types -----

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// A bare name: `int`, `string`, `Foo`.
    Named { name: String, span: Span },
    /// `list<T>`, `result<T>`, `option<T>`, `map<K, V>`, ...
    Generic {
        name: String,
        args: Vec<Type>,
        span: Span,
    },
    /// `Invoice@v1`.
    Model {
        name: String,
        version: u32,
        span: Span,
    },
    /// `(T1, T2, ...)` — `Tuple { elems: [] }` is the unit type.
    Tuple { elems: Vec<Type>, span: Span },
    /// `cap[entry, ...]` — § 8.3 of `language.md`. `star = true` represents
    /// `cap[*]` (full authority, accepted by the parser but rejected by
    /// `check::` per M2.T5). Entries are empty when `star` is set.
    Cap {
        entries: Vec<CapEntry>,
        star: bool,
        span: Span,
    },
    /// `fn(T1, T2) -> T3`.
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
        span: Span,
    },
}

/// One entry of a `cap[...]` list. The capability path is one or two
/// idents (`audit.event`, `http.post`); the optional `@` allow-list
/// names the concrete endpoints reachable through the operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapEntry {
    pub path: CapPath,
    pub allow: Option<Vec<String>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapPath {
    /// Either one (`audit`) or two (`http.post`) segments.
    pub segments: Vec<String>,
    pub span: Span,
}

impl Type {
    pub fn span(&self) -> Span {
        match self {
            Type::Named { span, .. }
            | Type::Generic { span, .. }
            | Type::Model { span, .. }
            | Type::Tuple { span, .. }
            | Type::Cap { span, .. }
            | Type::Fn { span, .. } => *span,
        }
    }
}

// ----- expressions, statements, patterns (M1.T6) -----

/// Binary operator. Precedence is encoded in the parser, not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    /// `lhs ?? rhs` — null-coalescing. `Ok(v)`/`Some(v)` evaluates to
    /// `v`; `Err(_)`/`None` (and any "missing" value) evaluates to
    /// the right-hand side. `rhs` is short-circuited like `||`.
    Coalesce,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Eq,
    AddEq,
    SubEq,
    MulEq,
    DivEq,
    RemEq,
}

/// A call argument; `name:` is the optional named-argument form (§ 7.1).
#[derive(Debug, Clone, PartialEq)]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
    pub span: Span,
}

/// A `{ a: 1, b: 2, ..base }` literal. `ty_name` is `Some("User")` for
/// `User { ... }` (§ 4.3 structural-update form), `None` for the
/// anonymous `{ a: 1, b: 2 }` map/record form (§ 2.4). `ty_version`
/// is `Some(n)` for the `Invoice@v1 { ... }` model-literal shape
/// (§ 16.2 / M8.T1) — the runtime then checks the record against
/// the matching `model X@vN` declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordLit {
    pub ty_name: Option<String>,
    pub ty_version: Option<u32>,
    pub fields: Vec<RecordLitField>,
    pub spread: Option<Box<Expr>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordLitField {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// A lambda parameter. The type annotation is optional (`fn(x) { x }`),
/// unlike `fn`-decls (§ 7.1) where it is required.
#[derive(Debug, Clone, PartialEq)]
pub struct LambdaParam {
    pub name: String,
    pub ty: Option<Type>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch {
    /// `else if cond { ... } [else? ...]` — the box always wraps `Expr::If`.
    ElseIf(Box<Expr>),
    Else(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Trailing tail expression; if present, the block evaluates to it.
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    Let {
        name: String,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    Var {
        name: String,
        ty: Option<Type>,
        value: Expr,
        span: Span,
    },
    For {
        var: String,
        iter: Expr,
        body: Block,
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    /// `defer <stmt>` — registers a body to run LIFO at every function
    /// exit point (M17.T3). Captures `let` bindings by value; the body
    /// is subject to the same static checks as if inlined at the exit.
    Defer { body: Expr, span: Span },
    /// An expression used as a statement; the trailing `;` (if any) is consumed.
    Expr(Expr),
}

/// Match patterns (§ 17.1). Exhaustiveness is enforced in M2; this AST
/// is the structural carrier.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_`
    Wildcard(Span),
    /// `x` — a fresh binding.
    Bind(String, Span),
    /// Literal pattern: `0`, `"spam"`, `true`, `'a'`. Restricted to literal forms.
    Lit(Expr, Span),
    /// `Active(t)` — positional constructor.
    Constructor {
        name: String,
        args: Vec<Pattern>,
        span: Span,
    },
    /// `Banned { reason: "spam", .. }` or shorthand `Banned { reason }`.
    RecordCtor {
        name: String,
        fields: Vec<RecordPatField>,
        rest: bool,
        span: Span,
    },
    /// `(p1, p2, ...)` — tuple pattern.
    Tuple { elems: Vec<Pattern>, span: Span },
    /// `[]`, `[x]`, `[x, ..rest]`, `[first, .., last]`.
    List { elems: Vec<ListPatElem>, span: Span },
}

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Pattern::Wildcard(s)
            | Pattern::Bind(_, s)
            | Pattern::Lit(_, s)
            | Pattern::Constructor { span: s, .. }
            | Pattern::RecordCtor { span: s, .. }
            | Pattern::Tuple { span: s, .. }
            | Pattern::List { span: s, .. } => *s,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordPatField {
    pub name: String,
    /// `None` is shorthand: `Banned { reason }` binds a fresh `reason`.
    pub pat: Option<Pattern>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ListPatElem {
    Pat(Pattern),
    /// `..` or `..rest` — only one allowed per list pattern (M2 enforces).
    Rest(Option<String>),
}

/// Aeris expression AST (§§ 5–6 of `language.md`).
/// One piece of an interpolated string literal in the AST (M16).
/// `Text` holds the decoded literal portion; `Interp` holds the
/// already-parsed expression captured between `{` and `}`.
#[derive(Debug, Clone, PartialEq)]
pub enum StrInterpPart {
    Text(String),
    Interp(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    // ---- atomic literals ----
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    /// Plain string literal. `\{`/`\}` already decoded.
    Str(String, Span),
    /// String literal with at least one `{ <expr> }` interpolation
    /// segment (M16). Each `Part::Interp` carries an already-parsed
    /// expression; the runtime stringifies it and concatenates.
    StrInterp(Vec<StrInterpPart>, Span),
    Bytes(Vec<u8>, Span),
    Char(char, Span),
    Date(String, Span),
    Timestamp(String, Span),
    Duration(String, Span),
    /// `()` — the unit value.
    Unit(Span),

    // ---- compound literals ----
    /// `(a, b, c)` — arity ≥ 2. (Arity 1 is a parenthesised expression.)
    Tuple(Vec<Expr>, Span),
    /// `[a, b, c]`.
    List(Vec<Expr>, Span),
    /// `{ a: 1, b: 2 }` or `User { a: 1, ..base }`.
    Record(RecordLit, Span),

    // ---- references ----
    Ident(String, Span),

    // ---- operators ----
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnOp,
        expr: Box<Expr>,
        span: Span,
    },

    // ---- postfix forms ----
    Field {
        base: Box<Expr>,
        name: String,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        /// Optional turbofish-style type arguments — `f<T1, T2>(args)`.
        /// Empty for the common `f(args)` form. Only consumed by
        /// type-aware builtins (e.g. `json.decode<Invoice@v1>`).
        type_args: Vec<Type>,
        args: Vec<CallArg>,
        span: Span,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    /// `expr?` — propagate `Err` per § 18.2.
    Try {
        expr: Box<Expr>,
        span: Span,
    },
    /// `expr catch <name> { <block> }` — recovery operator (M17.T1).
    /// `expr` must be a `result<T>`; on `Err(e)` the handler runs with
    /// `e` bound to `name` and its value replaces the `Err`. Pure
    /// syntactic sugar over `match`; the static checker and runtime
    /// see only the desugared form.
    Catch {
        expr: Box<Expr>,
        binding: String,
        handler: Block,
        span: Span,
    },
    /// `every <delay> { <body> }` (M18.T2) — infinite loop with a
    /// `clock.sleep(<delay>)` between iterations. The body runs
    /// before the first sleep so an `every 1s` loop fires at t=0,
    /// 1s, 2s, ...
    Every {
        delay: Box<Expr>,
        body: Block,
        span: Span,
    },
    /// `retry <n>, delay: <d> { <body> }` (M18.T3) — re-run the body
    /// up to `n` times, sleeping `d` between attempts. The body must
    /// yield a `result<T>`; the first `Ok` wins, the last `Err`
    /// propagates if every attempt fails.
    Retry {
        attempts: Box<Expr>,
        delay: Box<Expr>,
        body: Block,
        span: Span,
    },
    /// `timeout <d> { <body> }` (M18.T4) — runs the body and records
    /// `timeout_fired` on the trace if elapsed wall-time exceeds `d`.
    /// v0.3 does not interrupt the body; cancellation requires the
    /// future `spawn`-channel rework.
    Timeout {
        budget: Box<Expr>,
        body: Block,
        span: Span,
    },

    // ---- coercion / refinement ----
    Cast {
        expr: Box<Expr>,
        ty: Type,
        span: Span,
    },
    /// `expr is Pattern` (§ 17.3).
    IsCheck {
        expr: Box<Expr>,
        pat: Box<Pattern>,
        span: Span,
    },

    // ---- ranges ----
    Range {
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
        inclusive: bool,
        span: Span,
    },

    // ---- control flow ----
    If {
        cond: Box<Expr>,
        then_blk: Block,
        else_: Option<ElseBranch>,
        span: Span,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Block(Block, Span),

    // ---- language-level constructs ----
    Lambda {
        params: Vec<LambdaParam>,
        ret_ty: Option<Type>,
        body: Block,
        span: Span,
    },
    Spawn {
        body: Block,
        span: Span,
    },
    Await {
        expr: Box<Expr>,
        span: Span,
    },
    Raise {
        expr: Box<Expr>,
        span: Span,
    },
    Return {
        expr: Option<Box<Expr>>,
        span: Span,
    },
    Break {
        label: Option<String>,
        expr: Option<Box<Expr>>,
        span: Span,
    },
    Continue {
        label: Option<String>,
        span: Span,
    },

    // ---- assignment ----
    Assign {
        op: AssignOp,
        target: Box<Expr>,
        value: Box<Expr>,
        span: Span,
    },

    /// `cap.subset[..]` / `cap.test_subset[..]` (§ 8.4). The bracket
    /// body is parsed as a list of `CapEntry`s; `cap.subset[*]` is
    /// rejected at parse time because the construction must always
    /// narrow.
    CapNarrow {
        kind: CapNarrowKind,
        entries: Vec<CapEntry>,
        span: Span,
    },
    /// `intent "..." { ... }` block (§ 10.2). The string is the *why*
    /// trace key; the body runs inside the active intent scope. This
    /// variant captures the body-level form; saga-level / agent-level
    /// intent declarations live on the corresponding decl nodes.
    IntentBlock {
        label: String,
        body: Block,
        span: Span,
    },
    /// `Invoice@v1` as a value-position model reference (used inside
    /// agent `accept:` / `produce:` fields and migration call sites).
    /// The type-position form is `Type::Model`.
    ModelRef {
        name: String,
        version: u32,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapNarrowKind {
    Subset,
    TestSubset,
}

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Int(_, s)
            | Expr::Float(_, s)
            | Expr::Bool(_, s)
            | Expr::Str(_, s)
            | Expr::StrInterp(_, s)
            | Expr::Bytes(_, s)
            | Expr::Char(_, s)
            | Expr::Date(_, s)
            | Expr::Timestamp(_, s)
            | Expr::Duration(_, s)
            | Expr::Unit(s)
            | Expr::Tuple(_, s)
            | Expr::List(_, s)
            | Expr::Record(_, s)
            | Expr::Ident(_, s)
            | Expr::Binary { span: s, .. }
            | Expr::Unary { span: s, .. }
            | Expr::Field { span: s, .. }
            | Expr::Call { span: s, .. }
            | Expr::Index { span: s, .. }
            | Expr::Try { span: s, .. }
            | Expr::Catch { span: s, .. }
            | Expr::Every { span: s, .. }
            | Expr::Retry { span: s, .. }
            | Expr::Timeout { span: s, .. }
            | Expr::Cast { span: s, .. }
            | Expr::IsCheck { span: s, .. }
            | Expr::Range { span: s, .. }
            | Expr::If { span: s, .. }
            | Expr::Match { span: s, .. }
            | Expr::Block(_, s)
            | Expr::Lambda { span: s, .. }
            | Expr::Spawn { span: s, .. }
            | Expr::Await { span: s, .. }
            | Expr::Raise { span: s, .. }
            | Expr::Return { span: s, .. }
            | Expr::Break { span: s, .. }
            | Expr::Continue { span: s, .. }
            | Expr::Assign { span: s, .. }
            | Expr::CapNarrow { span: s, .. }
            | Expr::IntentBlock { span: s, .. }
            | Expr::ModelRef { span: s, .. } => *s,
        }
    }
}
