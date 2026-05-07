//! Tokens — the output of the lexer.
//!
//! Realises `docs/language.md` §§ 2.3–2.6.

use std::fmt;

/// A source span: byte offsets plus (line, col) for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end:   u32,
    pub line:  u32,
    pub col:   u32,
}

impl Span {
    pub const ZERO: Span = Span { start: 0, end: 0, line: 1, col: 1 };
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Identifiers / keywords / labels
    Ident(String),
    Keyword(Keyword),
    Label(String),       // 'name (used by `break 'outer`)

    // Literals
    Int(i64),
    Float(f64),
    Str(String),         // interpolations \(...) preserved verbatim for the parser
    Bytes(Vec<u8>),
    Char(char),
    Date(String),        // 2026-05-07
    Timestamp(String),   // 2026-05-07T08:30:00Z
    Duration(String),    // 3s, 500ms, 2h, 7d, ...

    // Punctuation / brackets
    LParen,    RParen,
    LBrace,    RBrace,
    LBracket,  RBracket,
    Comma,     Semicolon,
    Colon,     ColonColon,
    Dot,       DotDot,    DotDotEq,
    Arrow,     FatArrow,
    Question,  At,        Hash,

    // Comparison / equality
    Eq, EqEq, BangEq, Lt, LtEq, Gt, GtEq,

    // Arithmetic / bitwise
    Plus, Minus, Star, Slash, Percent,
    Amp, Pipe, Caret, LtLt, GtGt,

    // Compound assignment
    PlusEq, MinusEq, StarEq, SlashEq, PercentEq,

    // Trivia (retained for the formatter / `aeris doc`)
    LineComment(String),
    BlockComment(String),
    DocComment(String),

    // End-of-input sentinel
    Eof,
}

/// Reserved keywords listed in `docs/language.md` § 2.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Keyword {
    Agent,    AgentNet, And,      As,       Await,    Break,
    Cap,      Const,    Continue, Deny,     Do,       Else,
    Ensures,  Enum,     False,    Flow,     Fn,       For,
    From,     If,       In,       Intent,   Is,       Let,
    Limit,    Match,    Model,    Not,      Or,       Policy,
    Property, Pub,      Raise,    Record,   Require,  Requires,
    Return,   Saga,     Spawn,    Step,     Test,     True,
    Type,     Undo,     Until,    Use,      Var,      When,
    Where,    While,    With,
}

impl Keyword {
    /// Returns `Some(keyword)` if `s` matches a reserved keyword exactly.
    pub fn from_ident(s: &str) -> Option<Self> {
        Some(match s {
            "agent"     => Keyword::Agent,
            "agent_net" => Keyword::AgentNet,
            "and"       => Keyword::And,
            "as"        => Keyword::As,
            "await"     => Keyword::Await,
            "break"     => Keyword::Break,
            "cap"       => Keyword::Cap,
            "const"     => Keyword::Const,
            "continue"  => Keyword::Continue,
            "deny"      => Keyword::Deny,
            "do"        => Keyword::Do,
            "else"      => Keyword::Else,
            "ensures"   => Keyword::Ensures,
            "enum"      => Keyword::Enum,
            "false"     => Keyword::False,
            "flow"      => Keyword::Flow,
            "fn"        => Keyword::Fn,
            "for"       => Keyword::For,
            "from"      => Keyword::From,
            "if"        => Keyword::If,
            "in"        => Keyword::In,
            "intent"    => Keyword::Intent,
            "is"        => Keyword::Is,
            "let"       => Keyword::Let,
            "limit"     => Keyword::Limit,
            "match"     => Keyword::Match,
            "model"     => Keyword::Model,
            "not"       => Keyword::Not,
            "or"        => Keyword::Or,
            "policy"    => Keyword::Policy,
            "property"  => Keyword::Property,
            "pub"       => Keyword::Pub,
            "raise"     => Keyword::Raise,
            "record"    => Keyword::Record,
            "require"   => Keyword::Require,
            "requires"  => Keyword::Requires,
            "return"    => Keyword::Return,
            "saga"      => Keyword::Saga,
            "spawn"     => Keyword::Spawn,
            "step"      => Keyword::Step,
            "test"      => Keyword::Test,
            "true"      => Keyword::True,
            "type"      => Keyword::Type,
            "undo"      => Keyword::Undo,
            "until"     => Keyword::Until,
            "use"       => Keyword::Use,
            "var"       => Keyword::Var,
            "when"      => Keyword::When,
            "where"     => Keyword::Where,
            "while"     => Keyword::While,
            "with"      => Keyword::With,
            _ => return None,
        })
    }

    /// Canonical lower-case spelling of the keyword.
    pub fn as_str(self) -> &'static str {
        match self {
            Keyword::Agent     => "agent",
            Keyword::AgentNet  => "agent_net",
            Keyword::And       => "and",
            Keyword::As        => "as",
            Keyword::Await     => "await",
            Keyword::Break     => "break",
            Keyword::Cap       => "cap",
            Keyword::Const     => "const",
            Keyword::Continue  => "continue",
            Keyword::Deny      => "deny",
            Keyword::Do        => "do",
            Keyword::Else      => "else",
            Keyword::Ensures   => "ensures",
            Keyword::Enum      => "enum",
            Keyword::False     => "false",
            Keyword::Flow      => "flow",
            Keyword::Fn        => "fn",
            Keyword::For       => "for",
            Keyword::From      => "from",
            Keyword::If        => "if",
            Keyword::In        => "in",
            Keyword::Intent    => "intent",
            Keyword::Is        => "is",
            Keyword::Let       => "let",
            Keyword::Limit     => "limit",
            Keyword::Match     => "match",
            Keyword::Model     => "model",
            Keyword::Not       => "not",
            Keyword::Or        => "or",
            Keyword::Policy    => "policy",
            Keyword::Property  => "property",
            Keyword::Pub       => "pub",
            Keyword::Raise     => "raise",
            Keyword::Record    => "record",
            Keyword::Require   => "require",
            Keyword::Requires  => "requires",
            Keyword::Return    => "return",
            Keyword::Saga      => "saga",
            Keyword::Spawn     => "spawn",
            Keyword::Step      => "step",
            Keyword::Test      => "test",
            Keyword::True      => "true",
            Keyword::Type      => "type",
            Keyword::Undo      => "undo",
            Keyword::Until     => "until",
            Keyword::Use       => "use",
            Keyword::Var       => "var",
            Keyword::When      => "when",
            Keyword::Where     => "where",
            Keyword::While     => "while",
            Keyword::With      => "with",
        }
    }
}

impl fmt::Display for Keyword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// All reserved keywords, in declaration order (used by tests).
pub const ALL_KEYWORDS: &[Keyword] = &[
    Keyword::Agent,    Keyword::AgentNet, Keyword::And,      Keyword::As,
    Keyword::Await,    Keyword::Break,    Keyword::Cap,      Keyword::Const,
    Keyword::Continue, Keyword::Deny,     Keyword::Do,       Keyword::Else,
    Keyword::Ensures,  Keyword::Enum,     Keyword::False,    Keyword::Flow,
    Keyword::Fn,       Keyword::For,      Keyword::From,     Keyword::If,
    Keyword::In,       Keyword::Intent,   Keyword::Is,       Keyword::Let,
    Keyword::Limit,    Keyword::Match,    Keyword::Model,    Keyword::Not,
    Keyword::Or,       Keyword::Policy,   Keyword::Property, Keyword::Pub,
    Keyword::Raise,    Keyword::Record,   Keyword::Require,  Keyword::Requires,
    Keyword::Return,   Keyword::Saga,     Keyword::Spawn,    Keyword::Step,
    Keyword::Test,     Keyword::True,     Keyword::Type,     Keyword::Undo,
    Keyword::Until,    Keyword::Use,      Keyword::Var,      Keyword::When,
    Keyword::Where,    Keyword::While,    Keyword::With,
];
