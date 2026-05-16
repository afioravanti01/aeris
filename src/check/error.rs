//! Errors produced by `check::`.
//!
//! Each variant maps to one of the exit codes enumerated in
//! `docs/language.md` § 25.3:
//!
//!   64 — parse / type error
//!   65 — capability error (missing / over-broad / `cap[*]` in user code)
//!   66 — intent missing on write-effectful call
//!   67 — saga step lacks paired undo
//!   68 — model version conflict
//!   69 — lockfile drift (hash mismatch)
//!   70 — cycle in `agent_net`
//!   71 — allow-list violation (signature outside manifest ceiling)

use crate::syntax::token::Span;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckError {
    pub kind: CheckErrorKind,
    pub span: Span,
}

impl CheckError {
    pub fn new(kind: CheckErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// CLI exit code. `aeris check` returns the highest code among
    /// reported errors.
    pub fn exit_code(&self) -> u8 {
        match self.kind {
            CheckErrorKind::UnknownType(_)
            | CheckErrorKind::WrongTypeArity { .. }
            | CheckErrorKind::ArityRequired(_)
            | CheckErrorKind::UnboundGeneric(_)
            | CheckErrorKind::CyclicTypeAlias(_)
            | CheckErrorKind::DuplicateDecl(_)
            | CheckErrorKind::DuplicateField { .. }
            | CheckErrorKind::DuplicateVariant { .. }
            | CheckErrorKind::DuplicateGeneric { .. }
            | CheckErrorKind::ModelVersionConflict { .. }
            | CheckErrorKind::MissingAgentField { .. }
            | CheckErrorKind::NonExhaustiveMatch { .. } => 64,
            CheckErrorKind::CapStarInUserCode
            | CheckErrorKind::NoCapInScope { .. }
            | CheckErrorKind::OpNotInCapSignature { .. }
            | CheckErrorKind::CapEscape { .. } => 65,
            CheckErrorKind::MissingIntentForWriteCall { .. } => 66,
            CheckErrorKind::SagaStepUndoNoopWithWriteDo { .. } => 67,
            CheckErrorKind::BareModelWithoutVersion(_) => 68,
            CheckErrorKind::AgentNetCycle { .. } => 70,
            CheckErrorKind::AllowListOutsideLockset { .. } => 71,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckErrorKind {
    /// `Type::Named { name }` does not resolve to any primitive,
    /// stdlib container, generic parameter in scope, or top-level
    /// declaration.
    UnknownType(String),
    /// A stdlib container received the wrong number of type arguments
    /// (e.g. `list<T, U>` or `map<T>`).
    WrongTypeArity {
        name: String,
        expected: usize,
        found: usize,
    },
    /// A stdlib container that requires arguments was used bare
    /// (e.g. `list` instead of `list<int>`).
    ArityRequired(String),
    /// A generic-parameter use that is not bound by an enclosing
    /// `<T, U, ...>` list (e.g. body of `fn f() -> T`).
    UnboundGeneric(String),
    /// `type A = B`, `type B = A` (or longer chain).
    CyclicTypeAlias(String),
    /// Two top-level declarations share the same name.
    DuplicateDecl(String),
    /// Two fields of the same record / enum variant / model share the
    /// same name.
    DuplicateField { decl: String, field: String },
    /// Two variants of the same enum share the same name.
    DuplicateVariant { decl: String, variant: String },
    /// `<T, T>` — repeated generic-parameter name on a single decl.
    DuplicateGeneric { decl: String, name: String },
    /// `Invoice@v1` and `Invoice@v2` declared in the same module —
    /// this is *allowed* when both are deliberate; we still surface a
    /// diagnostic if a `model X@vN` is later overwritten by another
    /// `model X@vN` with the same N (true conflict).
    ModelVersionConflict { name: String, version: u32 },
    /// `cap[*]` appears in user source. Forbidden everywhere except
    /// `main`'s synthesised cap (§ 8.4, § 8.7). Exit code 65.
    CapStarInUserCode,
    /// Cycle detected in an `agent_net` declaration. The string is the
    /// chain of node names involved in the cycle, joined by `→`
    /// (§ 14.1). Exit code 70.
    AgentNetCycle { net: String, chain: String },
    /// `Type::Named { name }` resolves to a `model` declaration but is
    /// missing the mandatory `@vN` version tag (§ 16.1). Exit code 68.
    BareModelWithoutVersion(String),
    /// A saga `step` whose `do` block reaches a write-classified
    /// capability declared its `undo` as `noop` (§ 12.2). Exit
    /// code 67. Forces a paired compensation per thesis § 8.2.
    SagaStepUndoNoopWithWriteDo { saga: String, step: String },
    /// V2 enforcement (§ 10.1): a write-classified call appears
    /// without any enclosing `intent` block. Exit code 66.
    /// `op` is the surface `<module>.<operation>` form.
    MissingIntentForWriteCall { op: String },
    /// Body-resolution failure (§ 8.2). A `<module>.<op>(...)` call
    /// appears in a function body that has no `cap` parameter in
    /// lexical scope. Exit code 65.
    NoCapInScope { op: String },
    /// Body-resolution failure (§ 8.2 / § 8.3). The function does
    /// have a `cap` parameter, but its effect signature does not list
    /// the requested `<module>.<op>` pair. Exit code 65.
    OpNotInCapSignature { op: String },
    /// Cap-escape rule violation (§ 8.7). Exit code 65.
    CapEscape { vector: CapEscapeVector },
    /// M10.T1: an `agent` declaration is missing one of the required
    /// fields (`llm`, `intent`, `prompt`, `accept`, `produce`). Exit
    /// code 64.
    MissingAgentField { agent: String, field: String },
    /// M2.T6 (§ 8.3.2): a function or saga signature requested an
    /// allow-list entry that is not present in the project's
    /// `aeris.toml [caps]` ceiling. Exit code 71. `op` is the
    /// `<module>.<operation>` form (e.g. `http.post`); `entry` is the
    /// concrete allow-list entry that lies outside the ceiling
    /// (e.g. `"evil.com"`); `family` names the manifest section that
    /// would have to authorise it (e.g. `"http.allow"`).
    AllowListOutsideLockset {
        op: String,
        entry: String,
        family: String,
    },
    /// `match` exhaustiveness failure (§ 17.2). The structural form
    /// covers two cases the checker can prove without type
    /// information:
    ///
    /// * an empty match (`match x { }`);
    /// * a match whose arms are *all* guarded and there is no
    ///   unguarded catch-all (the "int + only-guards" form).
    ///
    /// Full enum / list exhaustiveness lands once the scrutinee type
    /// is available (post-M2.T1 type inference, planned for the
    /// follow-up to M2.T1). Exit code 64.
    NonExhaustiveMatch { reason: NonExhaustiveReason },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonExhaustiveReason {
    /// `match x { }` — the match has no arms at all.
    EmptyMatch,
    /// All arms are guarded; there is no `_` or unguarded plain binder
    /// to act as a catch-all (§ 17.2 "int + only-guards" rule).
    AllArmsGuardedNoCatchAll,
}

/// One of the six escape vectors of § 8.7.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapEscapeVector {
    /// `record R { c: cap[..] }` — cap stored in a record field.
    RecordField { record: String, field: String },
    /// `enum E { V(cap[..]) }` or `enum E { V { c: cap[..] } }`.
    EnumVariant { enum_name: String, variant: String },
    /// `const X: cap[..] = ...` — cap bound at module level.
    Const { name: String },
    /// `channel<cap[..]>` — cap sent through a channel.
    Channel,
    /// `fn f() -> result<cap[..]>` — cap nested inside a non-cap return type.
    /// Plain `fn f() -> cap[..]` is allowed.
    NestedReturn,
}
