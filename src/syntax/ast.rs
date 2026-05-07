//! Aeris AST.
//!
//! Realises `docs/language.md` §§ 4 (types), 7 (functions), 16 (models),
//! and § 26 (grammar). Bodies, contracts, where-clauses and constant
//! initialisers are captured as `RawSpan` for later phases (M1.T6 fills
//! expression bodies; M1.T7 parses `cap[..]` allow-lists).

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
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub raw: RawSpan,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub vis: Visibility,
    pub name: String,
    pub generics: Vec<String>,
    pub params: Vec<Param>,
    pub return_ty: Option<Type>,
    pub body: RawSpan,
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
    pub where_clause: Option<RawSpan>,
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
    pub record_where: Vec<RawSpan>,
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
    /// `cap[..]` — body parsed in M1.T7.
    Cap { raw: RawSpan, span: Span },
    /// `fn(T1, T2) -> T3`.
    Fn {
        params: Vec<Type>,
        ret: Box<Type>,
        span: Span,
    },
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
