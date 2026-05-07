//! Aeris syntax: lexer, parser, AST, formatter.
//!
//! Realises `docs/language.md` §§ 2–6 (lexical structure, types,
//! values, control flow) and § 26 (grammar).

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod token;

pub use lexer::{tokenize, LexError, LexErrorKind, Lexer};
pub use parser::{parse, ParseError, ParseErrorKind};
pub use token::{Keyword, Span, Token, TokenKind, ALL_KEYWORDS};
