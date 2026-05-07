//! Aeris parser — top-level declarations.
//!
//! Realises `docs/language.md` § 26 (grammar) for the M1.T5 surface:
//! `fn`, `record`, `enum`, `model`, `type`, `const`, with `pub`
//! visibility. Function bodies, contract clauses, where-clauses and
//! constant initialisers are captured as `RawSpan` for later phases
//! (M1.T6 fills expression bodies; M1.T7 parses `cap[..]` allow-lists;
//! M1.T9 parses contract expressions).

use super::ast::{
    ConstDecl, EnumDecl, EnumVariant, FnDecl, Item, ModelDecl, Module, Param, RawSpan, RecordDecl,
    RecordField, Type, TypeAliasDecl, UseDecl, VariantData, Visibility,
};
use super::lexer::{tokenize, LexError};
use super::token::{Keyword, Span, Token, TokenKind};

pub fn parse(src: &str) -> Result<Module, ParseError> {
    let tokens = tokenize(src).map_err(ParseError::from_lex)?;
    let tokens = strip_trivia(tokens);
    let mut p = Parser::new(tokens);
    p.parse_module()
}

fn strip_trivia(tokens: Vec<Token>) -> Vec<Token> {
    tokens
        .into_iter()
        .filter(|t| {
            !matches!(
                t.kind,
                TokenKind::LineComment(_) | TokenKind::BlockComment(_) | TokenKind::DocComment(_)
            )
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

impl ParseError {
    fn from_lex(err: LexError) -> Self {
        Self {
            kind: ParseErrorKind::Lex(format!("{:?}", err.kind)),
            span: err.span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    Lex(String),
    Expected(String),
    UnexpectedEof,
    InvalidModelVersion,
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    // -------- helpers --------

    fn peek(&self) -> &TokenKind {
        &self.tokens[self.pos].kind
    }

    fn peek_token(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn peek_at(&self, off: usize) -> Option<&TokenKind> {
        self.tokens.get(self.pos + off).map(|t| &t.kind)
    }

    fn advance(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn at_eof(&self) -> bool {
        matches!(self.peek(), TokenKind::Eof)
    }

    fn err(&self, kind: ParseErrorKind) -> ParseError {
        ParseError {
            kind,
            span: self.peek_token().span,
        }
    }

    fn expect_kind(&mut self, expected: &TokenKind) -> Result<Token, ParseError> {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(expected) {
            Ok(self.advance())
        } else {
            Err(self.err(ParseErrorKind::Expected(format!("{expected:?}"))))
        }
    }

    fn expect_kw(&mut self, kw: Keyword) -> Result<Token, ParseError> {
        match self.peek() {
            TokenKind::Keyword(k) if *k == kw => Ok(self.advance()),
            _ => Err(self.err(ParseErrorKind::Expected(kw.as_str().to_string()))),
        }
    }

    fn at_kw(&self, kw: Keyword) -> bool {
        matches!(self.peek(), TokenKind::Keyword(k) if *k == kw)
    }

    fn eat_kw(&mut self, kw: Keyword) -> bool {
        if self.at_kw(kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_ident(&mut self) -> Result<(String, Span), ParseError> {
        match self.peek() {
            TokenKind::Ident(_) => {
                let t = self.advance();
                if let TokenKind::Ident(name) = t.kind {
                    Ok((name, t.span))
                } else {
                    unreachable!()
                }
            }
            _ => Err(self.err(ParseErrorKind::Expected("identifier".into()))),
        }
    }

    fn span_join(start: Span, end: Span) -> Span {
        Span {
            start: start.start,
            end: end.end,
            line: start.line,
            col: start.col,
        }
    }

    // -------- module --------

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        let mut uses = Vec::new();
        while self.at_kw(Keyword::Use) {
            uses.push(self.parse_use());
        }
        let mut items = Vec::new();
        while !self.at_eof() {
            items.push(self.parse_item()?);
        }
        Ok(Module { uses, items })
    }

    fn parse_use(&mut self) -> UseDecl {
        // Consume `use` first so `skip_until_top_level` does not see it as
        // an item-start keyword and bail with an empty range.
        let start_tok = self.advance();
        let body = self.skip_until_top_level();
        let span = Self::span_join(start_tok.span, body.span);
        UseDecl {
            raw: RawSpan { span },
            span,
        }
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        let vis = if self.eat_kw(Keyword::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };
        match self.peek() {
            TokenKind::Keyword(Keyword::Fn) => self.parse_fn(vis).map(Item::Fn),
            TokenKind::Keyword(Keyword::Record) => self.parse_record(vis).map(Item::Record),
            TokenKind::Keyword(Keyword::Enum) => self.parse_enum(vis).map(Item::Enum),
            TokenKind::Keyword(Keyword::Model) => self.parse_model(vis).map(Item::Model),
            TokenKind::Keyword(Keyword::Type) => self.parse_type_alias(vis).map(Item::TypeAlias),
            TokenKind::Keyword(Keyword::Const) => self.parse_const(vis).map(Item::Const),
            _ => Err(self.err(ParseErrorKind::Expected(
                "fn / record / enum / model / type / const".into(),
            ))),
        }
    }

    // -------- fn --------

    fn parse_fn(&mut self, vis: Visibility) -> Result<FnDecl, ParseError> {
        let start = self.expect_kw(Keyword::Fn)?.span;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics()?;
        self.expect_kind(&TokenKind::LParen)?;
        let mut params = Vec::new();
        if !matches!(self.peek(), TokenKind::RParen) {
            params.push(self.parse_param()?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                if matches!(self.peek(), TokenKind::RParen) {
                    break;
                }
                params.push(self.parse_param()?);
            }
        }
        self.expect_kind(&TokenKind::RParen)?;

        let return_ty = if matches!(self.peek(), TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };

        // `requires:` / `ensures:` clauses are skipped at this milestone.
        while self.at_kw(Keyword::Requires) || self.at_kw(Keyword::Ensures) {
            self.advance();
            self.expect_kind(&TokenKind::Colon)?;
            self.skip_contract_expr();
        }

        let body = self.skip_balanced_brace_group()?;
        Ok(FnDecl {
            vis,
            name,
            generics,
            params,
            return_ty,
            body,
            span: Self::span_join(start, body.span),
        })
    }

    fn parse_param(&mut self) -> Result<Param, ParseError> {
        let start = self.peek_token().span;
        let name = if self.at_kw(Keyword::Cap) {
            self.advance();
            "cap".to_string()
        } else {
            self.expect_ident()?.0
        };
        self.expect_kind(&TokenKind::Colon)?;
        let ty = self.parse_type()?;
        Ok(Param {
            name,
            ty: ty.clone(),
            span: Self::span_join(start, ty.span()),
        })
    }

    fn skip_contract_expr(&mut self) {
        let mut depth: i32 = 0;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::LBrace if depth == 0 => break,
                TokenKind::Keyword(Keyword::Requires | Keyword::Ensures) if depth == 0 => break,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth -= 1;
                    if depth < 0 {
                        break;
                    }
                    self.advance();
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // -------- record --------

    fn parse_record(&mut self, vis: Visibility) -> Result<RecordDecl, ParseError> {
        let start = self.expect_kw(Keyword::Record)?.span;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace) {
            fields.push(self.parse_record_field()?);
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            }
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(RecordDecl {
            vis,
            name,
            generics,
            fields,
            span: Self::span_join(start, end),
        })
    }

    fn parse_record_field(&mut self) -> Result<RecordField, ParseError> {
        let start = self.peek_token().span;
        let (name, _) = self.expect_ident()?;
        self.expect_kind(&TokenKind::Colon)?;
        let ty = self.parse_type()?;
        let where_clause = if self.eat_kw(Keyword::Where) {
            Some(self.skip_field_where_expr())
        } else {
            None
        };
        let end_span = where_clause.map_or(ty.span(), |w| w.span);
        Ok(RecordField {
            name,
            ty,
            where_clause,
            span: Self::span_join(start, end_span),
        })
    }

    /// Capture a field-level or record-level `where` expression as raw tokens.
    /// The expression terminates at the first comma, closing brace, `where`
    /// keyword (start of the next clause), or `<ident> :` pair (start of the
    /// next field) at top depth.
    fn skip_field_where_expr(&mut self) -> RawSpan {
        let start_span = self.peek_token().span;
        let mut last_span = start_span;
        let mut depth: i32 = 0;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    last_span = self.advance().span;
                }
                TokenKind::RBrace if depth == 0 => break,
                TokenKind::Comma if depth == 0 => break,
                TokenKind::Keyword(Keyword::Where) if depth == 0 => break,
                TokenKind::Ident(_)
                    if depth == 0 && matches!(self.peek_at(1), Some(TokenKind::Colon)) =>
                {
                    break
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    depth -= 1;
                    last_span = self.advance().span;
                }
                _ => {
                    last_span = self.advance().span;
                }
            }
        }
        RawSpan {
            span: Self::span_join(start_span, last_span),
        }
    }

    // -------- enum --------

    fn parse_enum(&mut self, vis: Visibility) -> Result<EnumDecl, ParseError> {
        let start = self.expect_kw(Keyword::Enum)?.span;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut variants = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace) {
            variants.push(self.parse_variant()?);
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            }
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(EnumDecl {
            vis,
            name,
            generics,
            variants,
            span: Self::span_join(start, end),
        })
    }

    fn parse_variant(&mut self) -> Result<EnumVariant, ParseError> {
        let start = self.peek_token().span;
        let (name, _) = self.expect_ident()?;
        let data = match self.peek() {
            TokenKind::LParen => {
                self.advance();
                let mut elems = Vec::new();
                if !matches!(self.peek(), TokenKind::RParen) {
                    elems.push(self.parse_variant_tuple_elem()?);
                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        if matches!(self.peek(), TokenKind::RParen) {
                            break;
                        }
                        elems.push(self.parse_variant_tuple_elem()?);
                    }
                }
                self.expect_kind(&TokenKind::RParen)?;
                VariantData::Tuple(elems)
            }
            TokenKind::LBrace => {
                self.advance();
                let mut fields = Vec::new();
                while !matches!(self.peek(), TokenKind::RBrace) {
                    fields.push(self.parse_record_field()?);
                    if matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                    }
                }
                self.expect_kind(&TokenKind::RBrace)?;
                VariantData::Record(fields)
            }
            _ => VariantData::Unit,
        };
        let end_span = self.tokens[self.pos.saturating_sub(1)].span;
        Ok(EnumVariant {
            name,
            data,
            span: Self::span_join(start, end_span),
        })
    }

    /// Variant tuple elements may be `name: Type` or just `Type` per § 4.4.
    /// We accept both and discard the optional name in M1.T5 (it can be
    /// recovered from source for a future refinement).
    fn parse_variant_tuple_elem(&mut self) -> Result<Type, ParseError> {
        if matches!(self.peek(), TokenKind::Ident(_))
            && matches!(self.peek_at(1), Some(TokenKind::Colon))
        {
            self.advance();
            self.advance();
        }
        self.parse_type()
    }

    // -------- model --------

    fn parse_model(&mut self, vis: Visibility) -> Result<ModelDecl, ParseError> {
        let start = self.expect_kw(Keyword::Model)?.span;
        let (name, _) = self.expect_ident()?;
        self.expect_kind(&TokenKind::At)?;
        let version = self.parse_model_version()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        let mut record_where = Vec::new();
        loop {
            match self.peek() {
                TokenKind::RBrace => break,
                TokenKind::Keyword(Keyword::Where) => {
                    self.advance();
                    self.expect_kind(&TokenKind::Colon)?;
                    record_where.push(self.skip_field_where_expr());
                    if matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                    }
                }
                _ => {
                    fields.push(self.parse_record_field()?);
                    if matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                    }
                }
            }
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(ModelDecl {
            vis,
            name,
            version,
            fields,
            record_where,
            span: Self::span_join(start, end),
        })
    }

    fn parse_model_version(&mut self) -> Result<u32, ParseError> {
        match self.peek() {
            TokenKind::Ident(s) if s.starts_with('v') && s.len() > 1 => {
                let s = s.clone();
                self.advance();
                s[1..]
                    .parse::<u32>()
                    .map_err(|_| self.err(ParseErrorKind::InvalidModelVersion))
            }
            _ => Err(self.err(ParseErrorKind::InvalidModelVersion)),
        }
    }

    // -------- type alias --------

    fn parse_type_alias(&mut self, vis: Visibility) -> Result<TypeAliasDecl, ParseError> {
        let start = self.expect_kw(Keyword::Type)?.span;
        let (name, _) = self.expect_ident()?;
        let generics = self.parse_generics()?;
        self.expect_kind(&TokenKind::Eq)?;
        let aliased = self.parse_type()?;
        let end_span = aliased.span();
        Ok(TypeAliasDecl {
            vis,
            name,
            generics,
            aliased,
            span: Self::span_join(start, end_span),
        })
    }

    // -------- const --------

    fn parse_const(&mut self, vis: Visibility) -> Result<ConstDecl, ParseError> {
        let start = self.expect_kw(Keyword::Const)?.span;
        let (name, _) = self.expect_ident()?;
        let ty = if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        self.expect_kind(&TokenKind::Eq)?;
        let init = self.skip_until_top_level();
        Ok(ConstDecl {
            vis,
            name,
            ty,
            init,
            span: Self::span_join(start, init.span),
        })
    }

    // -------- generics: <T, U> --------

    fn parse_generics(&mut self) -> Result<Vec<String>, ParseError> {
        if !matches!(self.peek(), TokenKind::Lt) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut out = Vec::new();
        if !matches!(self.peek(), TokenKind::Gt) {
            out.push(self.expect_ident()?.0);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                if matches!(self.peek(), TokenKind::Gt) {
                    break;
                }
                out.push(self.expect_ident()?.0);
            }
        }
        self.expect_kind(&TokenKind::Gt)?;
        Ok(out)
    }

    // -------- types --------

    fn parse_type(&mut self) -> Result<Type, ParseError> {
        match self.peek() {
            TokenKind::LParen => self.parse_tuple_or_paren_type(),
            TokenKind::Keyword(Keyword::Cap) => self.parse_cap_type(),
            TokenKind::Keyword(Keyword::Fn) => self.parse_fn_type(),
            TokenKind::Ident(_) => self.parse_named_or_generic_or_model_type(),
            _ => Err(self.err(ParseErrorKind::Expected("type".into()))),
        }
    }

    fn parse_tuple_or_paren_type(&mut self) -> Result<Type, ParseError> {
        let start = self.expect_kind(&TokenKind::LParen)?.span;
        if matches!(self.peek(), TokenKind::RParen) {
            let end = self.advance().span;
            return Ok(Type::Tuple {
                elems: Vec::new(),
                span: Self::span_join(start, end),
            });
        }
        let first = self.parse_type()?;
        if matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            let mut elems = vec![first];
            if !matches!(self.peek(), TokenKind::RParen) {
                elems.push(self.parse_type()?);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::RParen) {
                        break;
                    }
                    elems.push(self.parse_type()?);
                }
            }
            let end = self.expect_kind(&TokenKind::RParen)?.span;
            return Ok(Type::Tuple {
                elems,
                span: Self::span_join(start, end),
            });
        }
        self.expect_kind(&TokenKind::RParen)?;
        Ok(first)
    }

    fn parse_cap_type(&mut self) -> Result<Type, ParseError> {
        let start = self.expect_kw(Keyword::Cap)?.span;
        self.expect_kind(&TokenKind::LBracket)?;
        let inner_start = self.peek_token().span;
        let mut depth = 1i32;
        let mut last_span = inner_start;
        while depth > 0 {
            match self.peek() {
                TokenKind::Eof => return Err(self.err(ParseErrorKind::UnexpectedEof)),
                TokenKind::LBracket => {
                    depth += 1;
                    last_span = self.advance().span;
                }
                TokenKind::RBracket => {
                    depth -= 1;
                    let t = self.advance();
                    if depth == 0 {
                        return Ok(Type::Cap {
                            raw: RawSpan {
                                span: Self::span_join(inner_start, last_span),
                            },
                            span: Self::span_join(start, t.span),
                        });
                    }
                    last_span = t.span;
                }
                _ => {
                    last_span = self.advance().span;
                }
            }
        }
        Err(self.err(ParseErrorKind::UnexpectedEof))
    }

    fn parse_fn_type(&mut self) -> Result<Type, ParseError> {
        let start = self.expect_kw(Keyword::Fn)?.span;
        self.expect_kind(&TokenKind::LParen)?;
        let mut params = Vec::new();
        if !matches!(self.peek(), TokenKind::RParen) {
            params.push(self.parse_type()?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                if matches!(self.peek(), TokenKind::RParen) {
                    break;
                }
                params.push(self.parse_type()?);
            }
        }
        self.expect_kind(&TokenKind::RParen)?;
        self.expect_kind(&TokenKind::Arrow)?;
        let ret = self.parse_type()?;
        let end_span = ret.span();
        Ok(Type::Fn {
            params,
            ret: Box::new(ret),
            span: Self::span_join(start, end_span),
        })
    }

    fn parse_named_or_generic_or_model_type(&mut self) -> Result<Type, ParseError> {
        let (name, name_span) = self.expect_ident()?;
        if matches!(self.peek(), TokenKind::At) {
            self.advance();
            let version = self.parse_model_version()?;
            let end = self.tokens[self.pos.saturating_sub(1)].span;
            return Ok(Type::Model {
                name,
                version,
                span: Self::span_join(name_span, end),
            });
        }
        if matches!(self.peek(), TokenKind::Lt) {
            self.advance();
            let mut args = Vec::new();
            if !matches!(self.peek(), TokenKind::Gt) {
                args.push(self.parse_type()?);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::Gt) {
                        break;
                    }
                    args.push(self.parse_type()?);
                }
            }
            let end = self.expect_kind(&TokenKind::Gt)?.span;
            return Ok(Type::Generic {
                name,
                args,
                span: Self::span_join(name_span, end),
            });
        }
        Ok(Type::Named {
            name,
            span: name_span,
        })
    }

    // -------- skip helpers --------

    fn skip_balanced_brace_group(&mut self) -> Result<RawSpan, ParseError> {
        let start_tok = self.expect_kind(&TokenKind::LBrace)?;
        let mut depth = 1i32;
        let mut last_span = start_tok.span;
        while depth > 0 {
            match self.peek() {
                TokenKind::Eof => return Err(self.err(ParseErrorKind::UnexpectedEof)),
                TokenKind::LBrace => {
                    depth += 1;
                    last_span = self.advance().span;
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    last_span = self.advance().span;
                }
                _ => {
                    last_span = self.advance().span;
                }
            }
        }
        Ok(RawSpan {
            span: Self::span_join(start_tok.span, last_span),
        })
    }

    fn skip_until_top_level(&mut self) -> RawSpan {
        let start_tok_span = self.peek_token().span;
        let mut last_span = start_tok_span;
        let mut depth: i32 = 0;
        loop {
            match self.peek() {
                TokenKind::Eof => break,
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    last_span = self.advance().span;
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                    last_span = self.advance().span;
                }
                TokenKind::Keyword(kw) if depth == 0 && is_item_start_keyword(*kw) => break,
                _ => {
                    last_span = self.advance().span;
                }
            }
        }
        RawSpan {
            span: Self::span_join(start_tok_span, last_span),
        }
    }
}

fn is_item_start_keyword(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::Pub
            | Keyword::Fn
            | Keyword::Record
            | Keyword::Enum
            | Keyword::Model
            | Keyword::Type
            | Keyword::Const
            | Keyword::Use
            | Keyword::Saga
            | Keyword::Agent
            | Keyword::AgentNet
            | Keyword::Policy
            | Keyword::Test
            | Keyword::Property
    )
}

// ====================================================================
// Tests — 30 fixtures per `docs/plan.md` § 5.1 M1.T5 acceptance.
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> Module {
        match parse(src) {
            Ok(m) => m,
            Err(e) => panic!("parse error: {e:?} on {src:?}"),
        }
    }

    fn item_kind(item: &Item) -> &'static str {
        match item {
            Item::Fn(_) => "fn",
            Item::Record(_) => "record",
            Item::Enum(_) => "enum",
            Item::Model(_) => "model",
            Item::TypeAlias(_) => "type",
            Item::Const(_) => "const",
        }
    }

    // ---------- fn (5) ----------

    #[test]
    fn fn_empty() {
        let m = parse_ok("fn f() {}");
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert_eq!(f.name, "f");
        assert!(f.generics.is_empty());
        assert!(f.params.is_empty());
        assert!(f.return_ty.is_none());
        assert_eq!(f.vis, Visibility::Private);
    }

    #[test]
    fn fn_with_params_and_return() {
        let m = parse_ok("fn add(a: int, b: int) -> int {}");
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert_eq!(f.params.len(), 2);
        assert_eq!(f.params[0].name, "a");
        assert_eq!(f.params[1].name, "b");
        assert!(matches!(f.return_ty, Some(Type::Named { ref name, .. }) if name == "int"));
    }

    #[test]
    fn fn_pub_visibility() {
        let m = parse_ok("pub fn settle() {}");
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert_eq!(f.vis, Visibility::Public);
    }

    #[test]
    fn fn_with_generics() {
        let m = parse_ok("fn first<T>(xs: list<T>) -> option<T> {}");
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert_eq!(f.generics, vec!["T".to_string()]);
        match &f.params[0].ty {
            Type::Generic { name, args, .. } => {
                assert_eq!(name, "list");
                assert_eq!(args.len(), 1);
                assert!(matches!(args[0], Type::Named { ref name, .. } if name == "T"));
            }
            other => panic!("expected list<T>, got {other:?}"),
        }
    }

    #[test]
    fn fn_with_cap_param() {
        let m = parse_ok("fn rotate(cap: cap[fs.write_file, audit.event]) {}");
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.params[0].name, "cap");
        assert!(matches!(f.params[0].ty, Type::Cap { .. }));
    }

    // ---------- record (5) ----------

    #[test]
    fn record_empty() {
        let m = parse_ok("record Empty {}");
        let r = match &m.items[0] {
            Item::Record(r) => r,
            _ => panic!(),
        };
        assert_eq!(r.name, "Empty");
        assert!(r.fields.is_empty());
    }

    #[test]
    fn record_simple_fields() {
        let m = parse_ok("record User { id: uuid, name: string, age: int }");
        let r = match &m.items[0] {
            Item::Record(r) => r,
            _ => panic!(),
        };
        assert_eq!(r.fields.len(), 3);
        assert_eq!(r.fields[0].name, "id");
        assert_eq!(r.fields[1].name, "name");
        assert_eq!(r.fields[2].name, "age");
    }

    #[test]
    fn record_field_with_where_clause() {
        let m = parse_ok("record Order { total: decimal where total > 0 }");
        let r = match &m.items[0] {
            Item::Record(r) => r,
            _ => panic!(),
        };
        assert_eq!(r.fields.len(), 1);
        assert!(r.fields[0].where_clause.is_some());
    }

    #[test]
    fn record_with_generic_field() {
        let m = parse_ok("record Wrapper { items: list<int> }");
        let r = match &m.items[0] {
            Item::Record(r) => r,
            _ => panic!(),
        };
        assert!(matches!(&r.fields[0].ty, Type::Generic { name, .. } if name == "list"));
    }

    #[test]
    fn record_pub() {
        let m = parse_ok("pub record Public { x: int }");
        let r = match &m.items[0] {
            Item::Record(r) => r,
            _ => panic!(),
        };
        assert_eq!(r.vis, Visibility::Public);
    }

    // ---------- enum (5) ----------

    #[test]
    fn enum_unit_variants() {
        let m = parse_ok("enum Color { Red, Green, Blue }");
        let e = match &m.items[0] {
            Item::Enum(e) => e,
            _ => panic!(),
        };
        assert_eq!(e.variants.len(), 3);
        for v in &e.variants {
            assert!(matches!(v.data, VariantData::Unit));
        }
    }

    #[test]
    fn enum_tuple_variant() {
        let m = parse_ok("enum Status { Pending, Active(t: timestamp) }");
        let e = match &m.items[0] {
            Item::Enum(e) => e,
            _ => panic!(),
        };
        assert!(matches!(e.variants[0].data, VariantData::Unit));
        match &e.variants[1].data {
            VariantData::Tuple(elems) => assert_eq!(elems.len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn enum_record_variant() {
        // Note: language.md § 4.4 shows `until: option<date>` here, but `until`
        // is a reserved keyword (§ 2.3); this test uses `expires_on` instead.
        let m = parse_ok("enum E { Banned { reason: string, expires_on: option<date> } }");
        let e = match &m.items[0] {
            Item::Enum(e) => e,
            _ => panic!(),
        };
        match &e.variants[0].data {
            VariantData::Record(fields) => assert_eq!(fields.len(), 2),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn enum_mixed_variants() {
        let m = parse_ok("enum E { A, B(int), C { x: int } }");
        let e = match &m.items[0] {
            Item::Enum(e) => e,
            _ => panic!(),
        };
        assert_eq!(e.variants.len(), 3);
        assert!(matches!(e.variants[0].data, VariantData::Unit));
        assert!(matches!(e.variants[1].data, VariantData::Tuple(_)));
        assert!(matches!(e.variants[2].data, VariantData::Record(_)));
    }

    #[test]
    fn enum_generic() {
        let m = parse_ok("enum Either<L, R> { Left(L), Right(R) }");
        let e = match &m.items[0] {
            Item::Enum(e) => e,
            _ => panic!(),
        };
        assert_eq!(e.generics, vec!["L".to_string(), "R".to_string()]);
    }

    // ---------- model (5) ----------

    #[test]
    fn model_simple() {
        let m = parse_ok("model Invoice@v1 { id: uuid, amount: decimal }");
        let md = match &m.items[0] {
            Item::Model(md) => md,
            _ => panic!(),
        };
        assert_eq!(md.name, "Invoice");
        assert_eq!(md.version, 1);
        assert_eq!(md.fields.len(), 2);
    }

    #[test]
    fn model_field_where_clause() {
        let m = parse_ok("model Invoice@v1 { amount: decimal where amount > 0 }");
        let md = match &m.items[0] {
            Item::Model(md) => md,
            _ => panic!(),
        };
        assert!(md.fields[0].where_clause.is_some());
    }

    #[test]
    fn model_record_level_where() {
        let src = "model Order@v2 { total: decimal, status: Status, where: total > 0 }";
        let m = parse_ok(src);
        let md = match &m.items[0] {
            Item::Model(md) => md,
            _ => panic!(),
        };
        assert_eq!(md.fields.len(), 2);
        assert_eq!(md.record_where.len(), 1);
    }

    #[test]
    fn model_higher_version() {
        let m = parse_ok("model Doc@v42 { text: string }");
        let md = match &m.items[0] {
            Item::Model(md) => md,
            _ => panic!(),
        };
        assert_eq!(md.version, 42);
    }

    #[test]
    fn model_pub() {
        let m = parse_ok("pub model Open@v1 { x: int }");
        let md = match &m.items[0] {
            Item::Model(md) => md,
            _ => panic!(),
        };
        assert_eq!(md.vis, Visibility::Public);
    }

    // ---------- type alias (5) ----------

    #[test]
    fn type_alias_simple() {
        let m = parse_ok("type Email = string");
        let t = match &m.items[0] {
            Item::TypeAlias(t) => t,
            _ => panic!(),
        };
        assert_eq!(t.name, "Email");
        assert!(matches!(t.aliased, Type::Named { ref name, .. } if name == "string"));
    }

    #[test]
    fn type_alias_to_generic() {
        let m = parse_ok("type Ids = list<uuid>");
        let t = match &m.items[0] {
            Item::TypeAlias(t) => t,
            _ => panic!(),
        };
        assert!(matches!(t.aliased, Type::Generic { ref name, .. } if name == "list"));
    }

    #[test]
    fn type_alias_to_model() {
        let m = parse_ok("type LatestInvoice = Invoice@v3");
        let t = match &m.items[0] {
            Item::TypeAlias(t) => t,
            _ => panic!(),
        };
        assert!(matches!(t.aliased, Type::Model { version: 3, .. }));
    }

    #[test]
    fn type_alias_generic_lhs() {
        let m = parse_ok("type Pair<A, B> = (A, B)");
        let t = match &m.items[0] {
            Item::TypeAlias(t) => t,
            _ => panic!(),
        };
        assert_eq!(t.generics, vec!["A".to_string(), "B".to_string()]);
        assert!(matches!(t.aliased, Type::Tuple { ref elems, .. } if elems.len() == 2));
    }

    #[test]
    fn type_alias_pub() {
        let m = parse_ok("pub type UserId = uuid");
        let t = match &m.items[0] {
            Item::TypeAlias(t) => t,
            _ => panic!(),
        };
        assert_eq!(t.vis, Visibility::Public);
    }

    // ---------- const (3) ----------

    #[test]
    fn const_simple() {
        let m = parse_ok("const N = 42");
        let c = match &m.items[0] {
            Item::Const(c) => c,
            _ => panic!(),
        };
        assert_eq!(c.name, "N");
        assert!(c.ty.is_none());
    }

    #[test]
    fn const_with_type() {
        let m = parse_ok("const P: decimal = 2.71");
        let c = match &m.items[0] {
            Item::Const(c) => c,
            _ => panic!(),
        };
        assert!(matches!(c.ty, Some(Type::Named { ref name, .. }) if name == "decimal"));
    }

    #[test]
    fn const_pub() {
        let m = parse_ok("pub const NAME = \"aeris\"");
        let c = match &m.items[0] {
            Item::Const(c) => c,
            _ => panic!(),
        };
        assert_eq!(c.vis, Visibility::Public);
    }

    // ---------- mixed / module-shape (2) ----------

    #[test]
    fn module_with_use_and_multiple_items() {
        let src = r#"
            use io, json
            use http

            const N = 10

            record User { id: uuid, name: string }

            fn greet(u: User) {}
        "#;
        let m = parse_ok(src);
        assert_eq!(m.uses.len(), 2);
        assert_eq!(m.items.len(), 3);
        assert_eq!(item_kind(&m.items[0]), "const");
        assert_eq!(item_kind(&m.items[1]), "record");
        assert_eq!(item_kind(&m.items[2]), "fn");
    }

    #[test]
    fn doc_comments_are_stripped() {
        let src = "/// the user record\nrecord User { id: uuid }\n/// entry\nfn main() {}";
        let m = parse_ok(src);
        assert_eq!(m.items.len(), 2);
    }

    // ---------- error cases (2) ----------

    #[test]
    fn model_without_version_fails() {
        assert!(parse("model Invoice { id: uuid }").is_err());
    }

    #[test]
    fn unknown_top_level_keyword_fails() {
        assert!(parse("strudel Foo {}").is_err());
    }
}
