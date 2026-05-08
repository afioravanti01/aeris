//! Type resolution (M2.T1).
//!
//! Walks every type annotation in the parsed module and validates it
//! against:
//!
//! - the **primitive** type names (§ 4.1);
//! - the **stdlib generic container** registry (§ 4.2);
//! - the **user declarations** (records, enums, models, type aliases);
//! - the **generic parameters** in scope at the use site.
//!
//! Errors are collected into `Vec<CheckError>` — the checker never
//! aborts on the first failure so that `aeris check` can print a
//! complete diagnostic batch (M2.T12). Cyclic type aliases are
//! detected after the first pass (DFS with visited marks).

use std::collections::{HashMap, HashSet};

use super::effects;
use super::error::{CapEscapeVector, CheckError, CheckErrorKind, NonExhaustiveReason};
use crate::syntax::ast::{
    AgentNetDecl, ConstDecl, EnumDecl, FlowStage, Item, ModelDecl, Module, RecordDecl, RecordField,
    SagaDecl, Type, TypeAliasDecl, UndoForm, VariantData,
};
use crate::syntax::token::Span;

/// Top-level entry point for M2.T1.
pub fn check_module(m: &Module) -> Vec<CheckError> {
    let mut cx = Cx::default();
    cx.collect_decls(m);
    cx.check_decls(m);
    cx.detect_alias_cycles();
    cx.errors
}

// --------------------------------------------------------------------
// Static stdlib registry
// --------------------------------------------------------------------

/// Primitive type names (§ 4.1). They take no type arguments.
const PRIMITIVES: &[&str] = &[
    "bool",
    "int",
    "i8",
    "i16",
    "i32",
    "i64",
    "u8",
    "u16",
    "u32",
    "u64",
    "f32",
    "f64",
    "decimal",
    "string",
    "bytes",
    "char",
    "uuid",
    "date",
    "timestamp",
    "duration",
    "unit",
];

/// Stdlib generic containers (§ 4.2). `None` for `tuple` means
/// variable arity; the parser only ever emits `Type::Tuple` for the
/// `(T1, T2)` surface form, so the named `tuple<...>` form is
/// accepted permissively here for forward compatibility.
fn stdlib_arity(name: &str) -> Option<Arity> {
    match name {
        "list" | "set" | "option" | "result" | "channel" | "handle" | "range" => {
            Some(Arity::Exact(1))
        }
        "map" => Some(Arity::Exact(2)),
        "tuple" => Some(Arity::AtLeast(0)),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Arity {
    Exact(usize),
    AtLeast(usize),
}

impl Arity {
    fn matches(self, n: usize) -> bool {
        match self {
            Arity::Exact(k) => n == k,
            Arity::AtLeast(k) => n >= k,
        }
    }

    fn expected(self) -> usize {
        match self {
            Arity::Exact(k) | Arity::AtLeast(k) => k,
        }
    }
}

// --------------------------------------------------------------------
// Resolution context
// --------------------------------------------------------------------

#[derive(Default)]
struct Cx {
    /// Records / enums / type aliases by name.
    types: HashMap<String, TypeDecl>,
    /// Models keyed by `(name, version)`.
    models: HashMap<(String, u32), Span>,
    errors: Vec<CheckError>,
}

#[derive(Debug, Clone)]
enum TypeDecl {
    Record {
        generics: Vec<String>,
        span: Span,
    },
    Enum {
        generics: Vec<String>,
        span: Span,
    },
    Alias {
        generics: Vec<String>,
        target: Type,
        span: Span,
    },
}

impl TypeDecl {
    fn generics(&self) -> &[String] {
        match self {
            TypeDecl::Record { generics, .. }
            | TypeDecl::Enum { generics, .. }
            | TypeDecl::Alias { generics, .. } => generics,
        }
    }

    fn span(&self) -> Span {
        match self {
            TypeDecl::Record { span, .. }
            | TypeDecl::Enum { span, .. }
            | TypeDecl::Alias { span, .. } => *span,
        }
    }
}

impl Cx {
    fn err(&mut self, kind: CheckErrorKind, span: Span) {
        self.errors.push(CheckError::new(kind, span));
    }

    // ---------- Pass 1: collect names ----------

    fn collect_decls(&mut self, m: &Module) {
        for item in &m.items {
            match item {
                Item::Record(r) => self.register_type(
                    &r.name,
                    TypeDecl::Record {
                        generics: r.generics.clone(),
                        span: r.span,
                    },
                ),
                Item::Enum(e) => self.register_type(
                    &e.name,
                    TypeDecl::Enum {
                        generics: e.generics.clone(),
                        span: e.span,
                    },
                ),
                Item::TypeAlias(a) => self.register_type(
                    &a.name,
                    TypeDecl::Alias {
                        generics: a.generics.clone(),
                        target: a.aliased.clone(),
                        span: a.span,
                    },
                ),
                Item::Model(md) => {
                    let key = (md.name.clone(), md.version);
                    if let std::collections::hash_map::Entry::Vacant(slot) = self.models.entry(key)
                    {
                        slot.insert(md.span);
                    } else {
                        self.err(
                            CheckErrorKind::ModelVersionConflict {
                                name: md.name.clone(),
                                version: md.version,
                            },
                            md.span,
                        );
                    }
                }
                // Saga / agent / agent_net / policy / fn / const are
                // not type declarations; their bodies are checked by
                // later passes (M2.T7+).
                _ => {}
            }
        }
    }

    fn register_type(&mut self, name: &str, decl: TypeDecl) {
        if self.types.contains_key(name) {
            self.err(CheckErrorKind::DuplicateDecl(name.to_string()), decl.span());
        } else {
            self.types.insert(name.to_string(), decl);
        }
    }

    // ---------- Pass 2: validate per-decl shape and types ----------

    fn check_decls(&mut self, m: &Module) {
        for item in &m.items {
            match item {
                Item::Record(r) => self.check_record(r),
                Item::Enum(e) => self.check_enum(e),
                Item::Model(md) => self.check_model(md),
                Item::TypeAlias(a) => self.check_type_alias(a),
                Item::Const(c) => self.check_const(c),
                Item::Fn(f) => self.check_fn_signature(f),
                Item::AgentNet(n) => self.check_agent_net(n),
                Item::Saga(s) => self.check_saga(s),
                Item::Agent(a) => self.check_agent(a),
                Item::Policy(_) => {
                    // M8 wires policy semantics; field-presence is
                    // forgiving for now.
                }
            }
        }
    }

    // ---------- M10.T1: agent required-fields check ----------

    fn check_agent(&mut self, a: &crate::syntax::ast::AgentDecl) {
        // Spec § 13.1: `llm`, `intent`, `prompt`, `accept`, `produce`
        // are mandatory; `policy`, `retries`, `budget` are optional.
        for required in ["llm", "intent", "prompt", "accept", "produce"] {
            if !a.fields.iter().any(|f| f.key == required) {
                self.err(
                    CheckErrorKind::MissingAgentField {
                        agent: a.name.clone(),
                        field: required.into(),
                    },
                    a.span,
                );
            }
        }
    }

    // ---------- M2.T8: saga rule (write-do without undo) ----------

    fn check_saga(&mut self, s: &SagaDecl) {
        for step in &s.steps {
            if matches!(step.undo, UndoForm::Noop(_))
                && effects::block_has_write_call(&step.do_block)
            {
                self.err(
                    CheckErrorKind::SagaStepUndoNoopWithWriteDo {
                        saga: s.name.clone(),
                        step: step.name.clone(),
                    },
                    step.span,
                );
            }
        }
    }

    fn check_generics_unique(&mut self, decl: &str, generics: &[String], span: Span) {
        let mut seen = HashSet::new();
        for g in generics {
            if !seen.insert(g.clone()) {
                self.err(
                    CheckErrorKind::DuplicateGeneric {
                        decl: decl.to_string(),
                        name: g.clone(),
                    },
                    span,
                );
            }
        }
    }

    fn check_field_uniqueness(&mut self, decl: &str, fields: &[RecordField]) {
        let mut seen = HashSet::new();
        for f in fields {
            if !seen.insert(f.name.clone()) {
                self.err(
                    CheckErrorKind::DuplicateField {
                        decl: decl.to_string(),
                        field: f.name.clone(),
                    },
                    f.span,
                );
            }
        }
    }

    fn check_record(&mut self, r: &RecordDecl) {
        self.check_generics_unique(&r.name, &r.generics, r.span);
        self.check_field_uniqueness(&r.name, &r.fields);
        for f in &r.fields {
            self.check_type(&f.ty, &r.generics);
            // M2.T11 (§ 8.7): cap cannot be stored in a record field.
            if type_contains_cap(&f.ty) {
                self.err(
                    CheckErrorKind::CapEscape {
                        vector: CapEscapeVector::RecordField {
                            record: r.name.clone(),
                            field: f.name.clone(),
                        },
                    },
                    f.span,
                );
            }
        }
    }

    fn check_enum(&mut self, e: &EnumDecl) {
        self.check_generics_unique(&e.name, &e.generics, e.span);
        let mut seen = HashSet::new();
        for v in &e.variants {
            if !seen.insert(v.name.clone()) {
                self.err(
                    CheckErrorKind::DuplicateVariant {
                        decl: e.name.clone(),
                        variant: v.name.clone(),
                    },
                    v.span,
                );
            }
            let mut variant_has_cap = false;
            match &v.data {
                VariantData::Unit => {}
                VariantData::Tuple(elems) => {
                    for t in elems {
                        self.check_type(t, &e.generics);
                        if type_contains_cap(t) {
                            variant_has_cap = true;
                        }
                    }
                }
                VariantData::Record(fields) => {
                    self.check_field_uniqueness(&e.name, fields);
                    for f in fields {
                        self.check_type(&f.ty, &e.generics);
                        if type_contains_cap(&f.ty) {
                            variant_has_cap = true;
                        }
                    }
                }
            }
            // M2.T11: cap inside an enum variant is a record-field-class
            // escape (§ 8.7).
            if variant_has_cap {
                self.err(
                    CheckErrorKind::CapEscape {
                        vector: CapEscapeVector::EnumVariant {
                            enum_name: e.name.clone(),
                            variant: v.name.clone(),
                        },
                    },
                    v.span,
                );
            }
        }
    }

    fn check_model(&mut self, m: &ModelDecl) {
        self.check_field_uniqueness(&m.name, &m.fields);
        for f in &m.fields {
            self.check_type(&f.ty, &[]);
        }
    }

    fn check_type_alias(&mut self, a: &TypeAliasDecl) {
        self.check_generics_unique(&a.name, &a.generics, a.span);
        self.check_type(&a.aliased, &a.generics);
    }

    fn check_const(&mut self, c: &ConstDecl) {
        if let Some(t) = &c.ty {
            self.check_type(t, &[]);
            // M2.T11: cap cannot be assigned to a const (§ 8.7).
            if type_contains_cap(t) {
                self.err(
                    CheckErrorKind::CapEscape {
                        vector: CapEscapeVector::Const {
                            name: c.name.clone(),
                        },
                    },
                    c.span,
                );
            }
        }
    }

    fn check_fn_signature(&mut self, f: &crate::syntax::ast::FnDecl) {
        self.check_generics_unique(&f.name, &f.generics, f.span);
        for p in &f.params {
            self.check_type(&p.ty, &f.generics);
        }
        if let Some(ret) = &f.return_ty {
            self.check_type(ret, &f.generics);
            // M2.T11: cap return is allowed only when the *outermost*
            // form of the return type is `cap[..]` (§ 8.7). A cap
            // nested inside `result<...>`, `option<...>`, etc. is an
            // escape.
            if !matches!(ret, Type::Cap { .. }) && type_contains_cap(ret) {
                self.err(
                    CheckErrorKind::CapEscape {
                        vector: CapEscapeVector::NestedReturn,
                    },
                    ret.span(),
                );
            }
        }
        // M2.T7: V2 mandatory `intent` on write-effectful calls.
        for v in effects::collect_v2_violations(&f.body, false) {
            self.err(
                CheckErrorKind::MissingIntentForWriteCall { op: v.op },
                v.span,
            );
        }
        // M2.T4: body-resolution against the in-scope `cap` parameter.
        let cap_set = extract_cap_paths(f);
        for r in effects::collect_cap_resolution_errors(&f.body, cap_set.as_ref()) {
            let kind = match r.kind {
                effects::CapResolutionKind::NoCapInScope => {
                    CheckErrorKind::NoCapInScope { op: r.op }
                }
                effects::CapResolutionKind::OpNotInCap => {
                    CheckErrorKind::OpNotInCapSignature { op: r.op }
                }
            };
            self.err(kind, r.span);
        }
        // M2.T2: structural match exhaustiveness rule (§ 17.2).
        for v in effects::collect_match_violations(&f.body) {
            let reason = match v.kind {
                effects::MatchViolationKind::EmptyMatch => NonExhaustiveReason::EmptyMatch,
                effects::MatchViolationKind::AllGuardedNoCatchAll => {
                    NonExhaustiveReason::AllArmsGuardedNoCatchAll
                }
            };
            self.err(CheckErrorKind::NonExhaustiveMatch { reason }, v.span);
        }
    }

    // ---------- M2.T9: agent_net cycle detection ----------

    /// `agent_net` declares a typed dataflow graph (§ 14.1). Cycles
    /// are forbidden — iteration is encoded via `until:` instead. We
    /// build the union of all `flow` edges and DFS for back-edges.
    fn check_agent_net(&mut self, n: &AgentNetDecl) {
        let mut adj: HashMap<String, Vec<String>> = HashMap::new();
        for flow in &n.flows {
            for window in flow.stages.windows(2) {
                let froms = stage_names(&window[0]);
                let tos = stage_names(&window[1]);
                for f in &froms {
                    for t in &tos {
                        adj.entry(f.clone()).or_default().push(t.clone());
                    }
                }
            }
        }
        let nodes: Vec<String> = {
            let mut s: HashSet<String> = HashSet::new();
            for (k, vs) in &adj {
                s.insert(k.clone());
                for v in vs {
                    s.insert(v.clone());
                }
            }
            let mut v: Vec<String> = s.into_iter().collect();
            v.sort();
            v
        };
        let mut state: HashMap<String, NodeState> = HashMap::new();
        for node in &nodes {
            state.insert(node.clone(), NodeState::White);
        }
        for node in &nodes {
            if state.get(node) == Some(&NodeState::White) {
                let mut stack: Vec<String> = Vec::new();
                if let Some(chain) = dfs_find_cycle(node, &adj, &mut state, &mut stack) {
                    self.err(
                        CheckErrorKind::AgentNetCycle {
                            net: n.name.clone(),
                            chain,
                        },
                        n.span,
                    );
                    return;
                }
            }
        }
    }

    // ---------- Type recursion ----------

    fn check_type(&mut self, t: &Type, generics_in_scope: &[String]) {
        match t {
            Type::Named { name, span } => self.check_named(name, *span, generics_in_scope),
            Type::Generic {
                name, args, span, ..
            } => {
                self.check_generic(name, args.len(), *span);
                // M2.T11: `channel<cap[..]>` — cap cannot be sent
                // through a channel (§ 8.7).
                if name == "channel" {
                    if let Some(arg) = args.first() {
                        if type_contains_cap(arg) {
                            self.err(
                                CheckErrorKind::CapEscape {
                                    vector: CapEscapeVector::Channel,
                                },
                                *span,
                            );
                        }
                    }
                }
                for a in args {
                    self.check_type(a, generics_in_scope);
                }
            }
            Type::Model {
                name,
                version,
                span,
            } => {
                if !self.models.contains_key(&(name.clone(), *version)) {
                    self.err(
                        CheckErrorKind::UnknownType(format!("{name}@v{version}")),
                        *span,
                    );
                }
            }
            Type::Tuple { elems, .. } => {
                for e in elems {
                    self.check_type(e, generics_in_scope);
                }
            }
            Type::Cap {
                entries,
                star,
                span,
                ..
            } => {
                // M2.T5: `cap[*]` is forbidden in user code (§ 8.4).
                if *star {
                    self.err(CheckErrorKind::CapStarInUserCode, *span);
                }
                // Allow-list strings carry no types; cap-tree paths are
                // validated against the registry in M2.T3 / M2.T6.
                let _ = entries;
            }
            Type::Fn { params, ret, .. } => {
                for p in params {
                    self.check_type(p, generics_in_scope);
                }
                self.check_type(ret, generics_in_scope);
            }
        }
    }

    fn check_named(&mut self, name: &str, span: Span, generics_in_scope: &[String]) {
        if PRIMITIVES.contains(&name) {
            return;
        }
        if generics_in_scope.iter().any(|g| g == name) {
            return;
        }
        if let Some(arity) = stdlib_arity(name) {
            // Bare `list` (no `<T>`) — treat as missing-args error.
            // For variable-arity (`tuple`), 0 args is acceptable: we
            // simply don't catch it here.
            match arity {
                Arity::Exact(_) => self.err(CheckErrorKind::ArityRequired(name.to_string()), span),
                Arity::AtLeast(_) => {}
            }
            return;
        }
        if self.types.contains_key(name) {
            // Named user type; arity checked in `check_generic` when
            // type arguments are supplied. Bare uses of generic types
            // are deliberately permitted (parametric default via
            // unification, even though we don't infer here).
            return;
        }
        // M2.T10: a bare name that matches a declared model is missing
        // its mandatory `@vN` tag (§ 16.1). Emit code 68 instead of a
        // generic "unknown type".
        if self.models.keys().any(|(n, _)| n == name) {
            self.err(
                CheckErrorKind::BareModelWithoutVersion(name.to_string()),
                span,
            );
            return;
        }
        self.err(CheckErrorKind::UnknownType(name.to_string()), span);
    }

    fn check_generic(&mut self, name: &str, arg_count: usize, span: Span) {
        if let Some(arity) = stdlib_arity(name) {
            if !arity.matches(arg_count) {
                self.err(
                    CheckErrorKind::WrongTypeArity {
                        name: name.to_string(),
                        expected: arity.expected(),
                        found: arg_count,
                    },
                    span,
                );
            }
            return;
        }
        if let Some(decl) = self.types.get(name) {
            let expected = decl.generics().len();
            if arg_count != expected {
                self.err(
                    CheckErrorKind::WrongTypeArity {
                        name: name.to_string(),
                        expected,
                        found: arg_count,
                    },
                    span,
                );
            }
            return;
        }
        self.err(CheckErrorKind::UnknownType(name.to_string()), span);
    }

    // ---------- Pass 3: cyclic type aliases ----------

    fn detect_alias_cycles(&mut self) {
        // We only chase aliases through `Type::Named` heads. `type A =
        // list<A>` is therefore *not* a cycle (the recursion is
        // mediated by `list`, which is a stdlib container).
        let alias_targets: Vec<(String, Type, Span)> = self
            .types
            .iter()
            .filter_map(|(name, decl)| match decl {
                TypeDecl::Alias { target, span, .. } => Some((name.clone(), target.clone(), *span)),
                _ => None,
            })
            .collect();

        for (start, _, span) in &alias_targets {
            if self.alias_chain_visits_self(start) {
                self.err(CheckErrorKind::CyclicTypeAlias(start.clone()), *span);
            }
        }
    }

    fn alias_chain_visits_self(&self, start: &str) -> bool {
        let mut seen: HashSet<String> = HashSet::new();
        let mut cur = start.to_string();
        loop {
            if !seen.insert(cur.clone()) {
                // We hit a name we've already visited. If `start`
                // itself is in the visited set we report it.
                return seen.contains(start);
            }
            let next = match self.types.get(&cur) {
                Some(TypeDecl::Alias {
                    target: Type::Named { name, .. },
                    ..
                }) => name.clone(),
                _ => return false,
            };
            cur = next;
        }
    }
}

// --------------------------------------------------------------------
// agent_net helpers
// --------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeState {
    White,
    Gray,
    Black,
}

/// Whether `t` contains a `cap[..]` anywhere in its tree. The check
/// is purely structural — it does not chase aliases (a `type X = cap[..]`
/// then used as `R { c: X }` would not be caught at this layer; the
/// alias surface should be expanded post-resolution by M5+).
fn type_contains_cap(t: &Type) -> bool {
    match t {
        Type::Cap { .. } => true,
        Type::Named { .. } | Type::Model { .. } => false,
        Type::Generic { args, .. } => args.iter().any(type_contains_cap),
        Type::Tuple { elems, .. } => elems.iter().any(type_contains_cap),
        Type::Fn { params, ret, .. } => {
            params.iter().any(type_contains_cap) || type_contains_cap(ret)
        }
    }
}

/// Extract the `(module, op)` set declared in the `cap` parameter of
/// a function. Returns `None` if the function has no parameter named
/// `cap` (it is then statically pure per § 7.2). `cap[*]` produces an
/// empty set: M2.T5 already rejected the construct, so further calls
/// are surfaced as `OpNotInCapSignature`.
fn extract_cap_paths(
    f: &crate::syntax::ast::FnDecl,
) -> Option<std::collections::HashSet<(String, String)>> {
    let cap_param = f.params.iter().find(|p| p.name == "cap")?;
    let entries = match &cap_param.ty {
        Type::Cap { entries, .. } => entries,
        _ => return Some(std::collections::HashSet::new()),
    };
    let mut out: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for e in entries {
        match e.path.segments.as_slice() {
            // `<module>` alone: implies every leaf — § 8.3 "a capability
            // tree node implies its leaves". For now we treat a bare
            // module name as a wildcard that always matches; the
            // narrowing linter (V1, M12.T6) will rewrite it to the
            // concrete leaves used.
            [m] => {
                out.insert((m.clone(), "*".to_string()));
            }
            [m, op] => {
                out.insert((m.clone(), op.clone()));
            }
            _ => {}
        }
    }
    Some(out)
}

fn stage_names(s: &FlowStage) -> Vec<String> {
    match s {
        FlowStage::Single(n) => vec![n.clone()],
        FlowStage::FanOut(ns) => ns.clone(),
    }
}

/// DFS from `node`. On finding a back-edge (Gray target), build the
/// cycle path and return it as `"a -> b -> a"` for the error message.
fn dfs_find_cycle(
    node: &str,
    adj: &HashMap<String, Vec<String>>,
    state: &mut HashMap<String, NodeState>,
    stack: &mut Vec<String>,
) -> Option<String> {
    state.insert(node.to_string(), NodeState::Gray);
    stack.push(node.to_string());
    if let Some(succs) = adj.get(node) {
        for s in succs {
            match state.get(s).copied().unwrap_or(NodeState::White) {
                NodeState::White => {
                    if let Some(c) = dfs_find_cycle(s, adj, state, stack) {
                        return Some(c);
                    }
                }
                NodeState::Gray => {
                    // Back-edge — cycle. Build chain from where `s` first
                    // appears in the stack up to the current node.
                    let cut = stack.iter().position(|x| x == s).unwrap_or(0);
                    let mut chain = stack[cut..].to_vec();
                    chain.push(s.clone());
                    return Some(chain.join(" -> "));
                }
                NodeState::Black => {}
            }
        }
    }
    state.insert(node.to_string(), NodeState::Black);
    stack.pop();
    None
}
