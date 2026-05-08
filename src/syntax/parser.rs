//! Aeris parser — top-level declarations.
//!
//! Realises `docs/language.md` § 26 (grammar) for the M1.T5 surface:
//! `fn`, `record`, `enum`, `model`, `type`, `const`, with `pub`
//! visibility. Function bodies, contract clauses, where-clauses and
//! constant initialisers are captured as `RawSpan` for later phases
//! (M1.T6 fills expression bodies; M1.T7 parses `cap[..]` allow-lists;
//! M1.T9 parses contract expressions).

use super::ast::{
    AgentDecl, AgentNetDecl, AssignOp, BinOp, Block, CallArg, CapEntry, CapNarrowKind, CapPath,
    ConstDecl, DeclField, ElseBranch, EnumDecl, EnumVariant, Expr, FlowDecl, FlowStage, FnDecl,
    Item, LambdaParam, ListPatElem, MatchArm, ModelDecl, Module, Param, Pattern, PolicyDecl,
    RawSpan, RecordDecl, RecordField, RecordLit, RecordLitField, RecordPatField, SagaDecl,
    SagaStep, Stmt, Type, TypeAliasDecl, UnOp, UndoForm, UseDecl, VariantData, Visibility,
};
use super::lexer::{tokenize, LexError};
use super::token::{Keyword, Span, Token, TokenKind};

pub fn parse(src: &str) -> Result<Module, ParseError> {
    let tokens = tokenize(src).map_err(ParseError::from_lex)?;
    let tokens = strip_trivia(tokens);
    let mut p = Parser::new(tokens);
    p.parse_module()
}

/// Parse a module collecting **every** error rather than aborting at
/// the first one (M1.T11). On a malformed item the parser pushes the
/// error and synchronises to the next top-level item-start keyword,
/// so a single typo does not cascade into an avalanche of follow-on
/// errors. Used by `aeris check` and by future LSP / IDE plumbing.
pub fn parse_recovering(src: &str) -> ParseOutcome {
    let tokens = match tokenize(src) {
        Ok(toks) => toks,
        Err(err) => {
            return ParseOutcome {
                module: Module {
                    uses: Vec::new(),
                    items: Vec::new(),
                },
                errors: vec![ParseError::from_lex(err)],
            };
        }
    };
    let tokens = strip_trivia(tokens);
    let mut p = Parser::new(tokens);
    let (module, errors) = p.parse_module_recovering();
    ParseOutcome { module, errors }
}

/// Output of `parse_recovering`. `module` always exists (even when
/// `errors` is non-empty); items that failed to parse are simply
/// dropped from `module.items`.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseOutcome {
    pub module: Module,
    pub errors: Vec<ParseError>,
}

/// Parse a single expression. The whole input must be consumed.
///
/// This is the M1.T6 entry point used by tests and by future phases
/// (M1.T8 saga / agent / agent_net bodies, M1.T9 contract clauses,
/// M2 type-checker). `parse_module` still captures function bodies as
/// `RawSpan`; the dedicated body-of-fn parse will wire this in once
/// `check::` consumes it.
pub fn parse_expression(src: &str) -> Result<Expr, ParseError> {
    let tokens = tokenize(src).map_err(ParseError::from_lex)?;
    let tokens = strip_trivia(tokens);
    let mut p = Parser::new(tokens);
    let e = p.parse_expr()?;
    if !p.at_eof() {
        return Err(p.err(ParseErrorKind::Expected("end of expression".into())));
    }
    Ok(e)
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
    /// When false, `Ident { ... }` is *not* parsed as a record literal —
    /// the `{` belongs to a surrounding control-flow head. Mirrors
    /// Rust's parsing-context rule for `if`, `while`, `for`, `match`.
    allow_struct_lit: bool,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            allow_struct_lit: true,
        }
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

    /// Recovering variant of `parse_module`: collects errors instead of
    /// aborting, advancing to the next item-start keyword on failure.
    fn parse_module_recovering(&mut self) -> (Module, Vec<ParseError>) {
        let mut uses = Vec::new();
        while self.at_kw(Keyword::Use) {
            uses.push(self.parse_use());
        }
        let mut items = Vec::new();
        let mut errors: Vec<ParseError> = Vec::new();
        while !self.at_eof() {
            let saved = self.pos;
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(err) => {
                    errors.push(err);
                    // Ensure forward progress: if `parse_item` failed
                    // without consuming anything, step over the
                    // offending token before syncing.
                    if self.pos == saved && !self.at_eof() {
                        self.advance();
                    }
                    self.sync_to_next_item();
                }
            }
        }
        (Module { uses, items }, errors)
    }

    /// Advance until the next top-level item-start keyword (or EOF),
    /// staying outside any nested brace/paren/bracket group. Used for
    /// error recovery (M1.T11): one error per malformed item, no
    /// cascading errors.
    fn sync_to_next_item(&mut self) {
        let mut depth: i32 = 0;
        while !self.at_eof() {
            match self.peek() {
                TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => {
                    depth += 1;
                    self.advance();
                }
                TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                    if depth > 0 {
                        depth -= 1;
                    }
                    self.advance();
                }
                TokenKind::Keyword(kw) if depth == 0 && is_item_start_keyword(*kw) => break,
                _ => {
                    self.advance();
                }
            }
        }
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
        // M8.T5: zero or more `#[policy(name1, name2)]` attributes may
        // prefix a fn decl. We collect the names here so `parse_fn` can
        // attach them to the resulting `FnDecl`. Any other item kind
        // sees `attrs` ignored — the static checker will eventually
        // reject misplaced attrs (out of M8 scope).
        let mut policy_attrs: Vec<String> = Vec::new();
        while matches!(self.peek(), TokenKind::Hash) {
            self.advance(); // #
            self.expect_kind(&TokenKind::LBracket)?;
            // Today we only recognise `policy(...)` — every other key
            // is rejected so misuse is loud. `policy` is a global
            // keyword, so we match it directly rather than via
            // `expect_ident`.
            match self.peek() {
                TokenKind::Keyword(Keyword::Policy) => {
                    self.advance();
                }
                _ => {
                    return Err(self.err(ParseErrorKind::Expected(
                        "`#[policy(...)]` attribute".into(),
                    )));
                }
            }
            self.expect_kind(&TokenKind::LParen)?;
            if !matches!(self.peek(), TokenKind::RParen) {
                let (n, _) = self.expect_ident()?;
                policy_attrs.push(n);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::RParen) {
                        break;
                    }
                    let (n, _) = self.expect_ident()?;
                    policy_attrs.push(n);
                }
            }
            self.expect_kind(&TokenKind::RParen)?;
            self.expect_kind(&TokenKind::RBracket)?;
        }
        let vis = if self.eat_kw(Keyword::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };
        match self.peek() {
            TokenKind::Keyword(Keyword::Fn) => {
                self.parse_fn_with_attrs(vis, policy_attrs).map(Item::Fn)
            }
            TokenKind::Keyword(Keyword::Record) => self.parse_record(vis).map(Item::Record),
            TokenKind::Keyword(Keyword::Enum) => self.parse_enum(vis).map(Item::Enum),
            TokenKind::Keyword(Keyword::Model) => self.parse_model(vis).map(Item::Model),
            TokenKind::Keyword(Keyword::Type) => self.parse_type_alias(vis).map(Item::TypeAlias),
            TokenKind::Keyword(Keyword::Const) => self.parse_const(vis).map(Item::Const),
            TokenKind::Keyword(Keyword::Saga) => self.parse_saga(vis).map(Item::Saga),
            TokenKind::Keyword(Keyword::Agent) => self.parse_agent(vis).map(Item::Agent),
            TokenKind::Keyword(Keyword::AgentNet) => self.parse_agent_net(vis).map(Item::AgentNet),
            TokenKind::Keyword(Keyword::Policy) => self.parse_policy(vis).map(Item::Policy),
            _ => Err(self.err(ParseErrorKind::Expected(
                "fn / record / enum / model / type / const / saga / agent / agent_net / policy"
                    .into(),
            ))),
        }
    }

    // -------- fn --------

    fn parse_fn_with_attrs(
        &mut self,
        vis: Visibility,
        policy_attrs: Vec<String>,
    ) -> Result<FnDecl, ParseError> {
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

        // `requires:` / `ensures:` clauses parse as expressions (M1.T9).
        // Multiple clauses of the same kind are unioned; ordering between
        // the two is preserved per `language.md` § 9.1.
        let mut requires = Vec::new();
        let mut ensures = Vec::new();
        while self.at_kw(Keyword::Requires) || self.at_kw(Keyword::Ensures) {
            let is_requires = self.at_kw(Keyword::Requires);
            self.advance();
            self.expect_kind(&TokenKind::Colon)?;
            let e = self.parse_expr()?;
            if is_requires {
                requires.push(e);
            } else {
                ensures.push(e);
            }
        }

        let body = self.parse_block()?;
        let body_span = body.span;
        Ok(FnDecl {
            vis,
            name,
            generics,
            params,
            return_ty,
            requires,
            ensures,
            policy_attrs,
            body,
            span: Self::span_join(start, body_span),
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
        // Field-level where is `where <expr>` with no colon. The
        // record-level form `where: <expr>` (§ 16.3) shares the keyword
        // — peek ahead so we don't swallow it on its way to the
        // model-body loop.
        let where_clause = if matches!(self.peek(), TokenKind::Keyword(Keyword::Where))
            && !matches!(self.peek_at(1), Some(TokenKind::Colon))
        {
            self.advance();
            Some(self.parse_expr()?)
        } else {
            None
        };
        let end_span = where_clause.as_ref().map_or(ty.span(), |e| e.span());
        Ok(RecordField {
            name,
            ty,
            where_clause,
            span: Self::span_join(start, end_span),
        })
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
                    record_where.push(self.parse_expr()?);
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

    // -------- saga (M1.T8) --------

    fn parse_saga(&mut self, vis: Visibility) -> Result<SagaDecl, ParseError> {
        let start = self.expect_kw(Keyword::Saga)?.span;
        let (name, _) = self.expect_ident()?;
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
        self.expect_kind(&TokenKind::LBrace)?;
        // Saga-level intent — mandatory per § 12.2.
        self.expect_kw(Keyword::Intent)?;
        let intent = self.expect_string_literal()?;
        let mut steps = Vec::new();
        while self.at_kw(Keyword::Step) {
            steps.push(self.parse_saga_step()?);
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(SagaDecl {
            vis,
            name,
            params,
            intent,
            steps,
            span: Self::span_join(start, end),
        })
    }

    fn parse_saga_step(&mut self) -> Result<SagaStep, ParseError> {
        let start = self.expect_kw(Keyword::Step)?.span;
        let (name, _) = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut requires = Vec::new();
        while self.at_kw(Keyword::Requires) {
            self.advance();
            self.expect_kind(&TokenKind::Colon)?;
            requires.push(self.parse_expr()?);
        }
        self.expect_kw(Keyword::Do)?;
        let do_block = self.parse_block()?;
        self.expect_kw(Keyword::Undo)?;
        let undo = if let TokenKind::Ident(s) = self.peek() {
            if s == "noop" {
                let span = self.advance().span;
                UndoForm::Noop(span)
            } else {
                return Err(self.err(ParseErrorKind::Expected("`{` or `noop`".into())));
            }
        } else if matches!(self.peek(), TokenKind::LBrace) {
            UndoForm::Block(self.parse_block()?)
        } else {
            return Err(self.err(ParseErrorKind::Expected("`{` or `noop`".into())));
        };
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(SagaStep {
            name,
            requires,
            do_block,
            undo,
            span: Self::span_join(start, end),
        })
    }

    // -------- agent (M1.T8) --------

    fn parse_agent(&mut self, vis: Visibility) -> Result<AgentDecl, ParseError> {
        let start = self.expect_kw(Keyword::Agent)?.span;
        let (name, _) = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace) {
            fields.push(self.parse_decl_field()?);
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(AgentDecl {
            vis,
            name,
            fields,
            span: Self::span_join(start, end),
        })
    }

    // -------- agent_net (M1.T8) --------

    fn parse_agent_net(&mut self, vis: Visibility) -> Result<AgentNetDecl, ParseError> {
        let start = self.expect_kw(Keyword::AgentNet)?.span;
        let (name, _) = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut intent: Option<String> = None;
        let mut flows: Vec<FlowDecl> = Vec::new();
        let mut until: Option<Expr> = None;
        loop {
            match self.peek() {
                TokenKind::RBrace => break,
                TokenKind::Keyword(Keyword::Intent) => {
                    self.advance();
                    intent = Some(self.expect_string_literal()?);
                }
                TokenKind::Keyword(Keyword::Flow) => {
                    flows.push(self.parse_flow()?);
                }
                TokenKind::Keyword(Keyword::Until) => {
                    self.advance();
                    self.expect_kind(&TokenKind::Colon)?;
                    until = Some(self.parse_expr()?);
                }
                _ => {
                    return Err(self.err(ParseErrorKind::Expected(
                        "`intent`, `flow`, `until` or `}`".into(),
                    )))
                }
            }
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(AgentNetDecl {
            vis,
            name,
            intent,
            flows,
            until,
            span: Self::span_join(start, end),
        })
    }

    fn parse_flow(&mut self) -> Result<FlowDecl, ParseError> {
        let start = self.expect_kw(Keyword::Flow)?.span;
        let mut stages = Vec::new();
        stages.push(self.parse_flow_stage()?);
        let mut last_span = start;
        while matches!(self.peek(), TokenKind::Arrow) {
            self.advance();
            let stage = self.parse_flow_stage()?;
            last_span = match &stage {
                FlowStage::Single(_) => self.tokens[self.pos.saturating_sub(1)].span,
                FlowStage::FanOut(_) => self.tokens[self.pos.saturating_sub(1)].span,
            };
            stages.push(stage);
        }
        Ok(FlowDecl {
            stages,
            span: Self::span_join(start, last_span),
        })
    }

    fn parse_flow_stage(&mut self) -> Result<FlowStage, ParseError> {
        if matches!(self.peek(), TokenKind::LBrace) {
            self.advance();
            let mut names = Vec::new();
            if !matches!(self.peek(), TokenKind::RBrace) {
                names.push(self.expect_ident()?.0);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::RBrace) {
                        break;
                    }
                    names.push(self.expect_ident()?.0);
                }
            }
            self.expect_kind(&TokenKind::RBrace)?;
            Ok(FlowStage::FanOut(names))
        } else {
            let (n, _) = self.expect_ident()?;
            Ok(FlowStage::Single(n))
        }
    }

    // -------- policy (M1.T8) --------

    fn parse_policy(&mut self, vis: Visibility) -> Result<PolicyDecl, ParseError> {
        let start = self.expect_kw(Keyword::Policy)?.span;
        let (name, _) = self.expect_ident()?;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace) {
            fields.push(self.parse_decl_field()?);
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(PolicyDecl {
            vis,
            name,
            fields,
            span: Self::span_join(start, end),
        })
    }

    // -------- shared `<key>: <values>` field parser --------

    fn parse_decl_field(&mut self) -> Result<DeclField, ParseError> {
        let start = self.peek_token().span;
        let key = self.parse_decl_field_key()?;
        self.expect_kind(&TokenKind::Colon)?;
        let mut values = Vec::new();
        values.push(self.parse_expr()?);
        // List-separator commas: `policy: a, b`. We stop if the next
        // token after `,` looks like the start of a new field
        // (`<ident-or-key> :`) or the closing brace, since fields
        // themselves are not comma-delimited.
        while matches!(self.peek(), TokenKind::Comma) && !self.next_token_after_comma_starts_field()
        {
            self.advance();
            values.push(self.parse_expr()?);
        }
        let last = values.last().unwrap().span();
        Ok(DeclField {
            key,
            values,
            span: Self::span_join(start, last),
        })
    }

    fn next_token_after_comma_starts_field(&self) -> bool {
        match self.peek_at(1) {
            Some(TokenKind::RBrace) => true,
            Some(TokenKind::Ident(_)) | Some(TokenKind::Keyword(_)) => {
                matches!(self.peek_at(2), Some(TokenKind::Colon))
            }
            _ => false,
        }
    }

    /// Field-key recognition. Most agent / policy keys are plain
    /// identifiers; a handful (`match`, `intent`, `policy`, `require`,
    /// `deny`, `limit`, `when`) collide with global keywords (§ 2.3
    /// carve-out for structural-block field markers). We accept those
    /// keywords here as field labels.
    fn parse_decl_field_key(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            TokenKind::Ident(_) => Ok(self.expect_ident()?.0),
            TokenKind::Keyword(kw)
                if matches!(
                    kw,
                    Keyword::Match
                        | Keyword::Intent
                        | Keyword::Policy
                        | Keyword::Require
                        | Keyword::Deny
                        | Keyword::Limit
                        | Keyword::When
                ) =>
            {
                let s = kw.as_str().to_string();
                self.advance();
                Ok(s)
            }
            _ => Err(self.err(ParseErrorKind::Expected("field key".into()))),
        }
    }

    // -------- generics: <T, U> --------

    fn parse_generics(&mut self) -> Result<Vec<String>, ParseError> {
        if !matches!(self.peek(), TokenKind::Lt) {
            return Ok(Vec::new());
        }
        self.advance();
        let mut out = Vec::new();
        if !self.at_close_angle() {
            out.push(self.expect_ident()?.0);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                if self.at_close_angle() {
                    break;
                }
                out.push(self.expect_ident()?.0);
            }
        }
        self.expect_close_angle()?;
        Ok(out)
    }

    /// Whether the next token closes a generic argument list. Accepts
    /// both a bare `>` and the leading half of `>>` / `>=`, which the
    /// lexer fuses into a single token.
    fn at_close_angle(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Gt | TokenKind::GtGt | TokenKind::GtEq
        )
    }

    /// Consume one closing `>` for a generic argument list. If the
    /// current token is `>>` or `>=` (lexer-fused), it is split: the
    /// first `>` is consumed, the second character is left as a fresh
    /// token so the outer parser can consume it later. Returns the
    /// span of the consumed `>`.
    /// M8.T2: speculative parse of a turbofish-style type argument
    /// list `<T1, T2, ...>` immediately preceding a `(`. Returns
    /// `Some(types)` only if the lookahead succeeds AND a `(` is
    /// waiting after the closing `>`. On any failure the parser is
    /// rewound — both `pos` and any in-place token mutations made by
    /// `expect_close_angle` (which splits `>>` / `>=`) are restored.
    fn try_parse_turbofish(&mut self) -> Option<Vec<Type>> {
        if !matches!(self.peek(), TokenKind::Lt) {
            return None;
        }
        let snapshot = self.tokens.clone();
        let saved_pos = self.pos;
        self.advance(); // <
        let mut tys = Vec::new();
        loop {
            match self.parse_type() {
                Ok(ty) => tys.push(ty),
                Err(_) => {
                    self.tokens = snapshot;
                    self.pos = saved_pos;
                    return None;
                }
            }
            match self.peek() {
                TokenKind::Comma => {
                    self.advance();
                }
                TokenKind::Gt | TokenKind::GtGt | TokenKind::GtEq => break,
                _ => {
                    self.tokens = snapshot;
                    self.pos = saved_pos;
                    return None;
                }
            }
        }
        if self.expect_close_angle().is_err() {
            self.tokens = snapshot;
            self.pos = saved_pos;
            return None;
        }
        if !matches!(self.peek(), TokenKind::LParen) {
            self.tokens = snapshot;
            self.pos = saved_pos;
            return None;
        }
        Some(tys)
    }

    fn expect_close_angle(&mut self) -> Result<Span, ParseError> {
        let cur_span = self.peek_token().span;
        match self.peek() {
            TokenKind::Gt => Ok(self.advance().span),
            TokenKind::GtGt => {
                let first = Span {
                    start: cur_span.start,
                    end: cur_span.start + 1,
                    line: cur_span.line,
                    col: cur_span.col,
                };
                let rest = Span {
                    start: cur_span.start + 1,
                    end: cur_span.end,
                    line: cur_span.line,
                    col: cur_span.col + 1,
                };
                self.tokens[self.pos] = Token {
                    kind: TokenKind::Gt,
                    span: rest,
                };
                Ok(first)
            }
            TokenKind::GtEq => {
                let first = Span {
                    start: cur_span.start,
                    end: cur_span.start + 1,
                    line: cur_span.line,
                    col: cur_span.col,
                };
                let rest = Span {
                    start: cur_span.start + 1,
                    end: cur_span.end,
                    line: cur_span.line,
                    col: cur_span.col + 1,
                };
                self.tokens[self.pos] = Token {
                    kind: TokenKind::Eq,
                    span: rest,
                };
                Ok(first)
            }
            _ => Err(self.err(ParseErrorKind::Expected(">".into()))),
        }
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
        // `cap[*]` parses but is flagged for `check::` (M2.T5).
        if matches!(self.peek(), TokenKind::Star) {
            self.advance();
            let end = self.expect_kind(&TokenKind::RBracket)?.span;
            return Ok(Type::Cap {
                entries: Vec::new(),
                star: true,
                span: Self::span_join(start, end),
            });
        }
        let entries = self.parse_cap_entry_list()?;
        let end = self.expect_kind(&TokenKind::RBracket)?.span;
        Ok(Type::Cap {
            entries,
            star: false,
            span: Self::span_join(start, end),
        })
    }

    /// Comma-separated `CapEntry` list. Trailing comma is permitted.
    /// Closing bracket is consumed by the caller.
    fn parse_cap_entry_list(&mut self) -> Result<Vec<CapEntry>, ParseError> {
        let mut out = Vec::new();
        if matches!(self.peek(), TokenKind::RBracket) {
            return Ok(out);
        }
        out.push(self.parse_cap_entry()?);
        while matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            if matches!(self.peek(), TokenKind::RBracket) {
                break;
            }
            out.push(self.parse_cap_entry()?);
        }
        Ok(out)
    }

    fn parse_cap_entry(&mut self) -> Result<CapEntry, ParseError> {
        let path = self.parse_cap_path()?;
        let path_span = path.span;
        let (allow, end_span) = if matches!(self.peek(), TokenKind::At) {
            self.advance();
            let (list, span) = self.parse_cap_allow_list()?;
            (Some(list), span)
        } else {
            (None, path_span)
        };
        Ok(CapEntry {
            path,
            allow,
            span: Self::span_join(path_span, end_span),
        })
    }

    fn parse_cap_path(&mut self) -> Result<CapPath, ParseError> {
        let (head, head_span) = self.expect_ident()?;
        let mut segments = vec![head];
        let mut last_span = head_span;
        if matches!(self.peek(), TokenKind::Dot) {
            self.advance();
            let (op, op_span) = self.expect_ident()?;
            segments.push(op);
            last_span = op_span;
        }
        Ok(CapPath {
            segments,
            span: Self::span_join(head_span, last_span),
        })
    }

    /// `@ "x"` — single string OR `@ ["x", "y"]` — bracketed list (§ 8.3.1).
    fn parse_cap_allow_list(&mut self) -> Result<(Vec<String>, Span), ParseError> {
        match self.peek() {
            TokenKind::Str(_) => {
                let t = self.advance();
                let s = match t.kind {
                    TokenKind::Str(s) => s,
                    _ => unreachable!(),
                };
                Ok((vec![s], t.span))
            }
            TokenKind::LBracket => {
                self.advance();
                let mut out = Vec::new();
                if !matches!(self.peek(), TokenKind::RBracket) {
                    out.push(self.expect_string_literal()?);
                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        if matches!(self.peek(), TokenKind::RBracket) {
                            break;
                        }
                        out.push(self.expect_string_literal()?);
                    }
                }
                let end = self.expect_kind(&TokenKind::RBracket)?.span;
                Ok((out, end))
            }
            _ => Err(self.err(ParseErrorKind::Expected(
                "string literal or `[ \"...\", ... ]`".into(),
            ))),
        }
    }

    fn expect_string_literal(&mut self) -> Result<String, ParseError> {
        match self.peek() {
            TokenKind::Str(_) => {
                let t = self.advance();
                if let TokenKind::Str(s) = t.kind {
                    Ok(s)
                } else {
                    unreachable!()
                }
            }
            _ => Err(self.err(ParseErrorKind::Expected("string literal".into()))),
        }
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
            if !self.at_close_angle() {
                args.push(self.parse_type()?);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if self.at_close_angle() {
                        break;
                    }
                    args.push(self.parse_type()?);
                }
            }
            let end = self.expect_close_angle()?;
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

    // =================================================================
    //  Expression parser (M1.T6)
    //
    //  Precedence ladder (high → low) per `language.md` § 2.6:
    //
    //    .   ?   (...)   [...]                     postfix     parse_postfix
    //    -   not                                   prefix      parse_unary
    //    *   /   %                                 muldiv      parse_muldiv
    //    +   -                                     addsub      parse_addsub
    //    <<  >>                                    shift       parse_shift
    //    &   |   ^                                 bitops      parse_bitops
    //    ==  !=  <  <=  >  >=                      cmp         parse_cmp
    //    is  as                                    is_as       parse_is_as
    //    and                                       and         parse_and
    //    or                                        or          parse_or
    //    ..  ..=                                   range       parse_range
    //    =   +=   -=   *=   /=   %=                assign      parse_assign
    //
    //  `parse_expr` is the top-level entry that returns an `Expr`. It is
    //  used for raw-expression fixtures and for sub-expressions inside
    //  blocks, control-flow heads, match arms, contracts, etc.
    // =================================================================

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_assign()
    }

    // ------- assign (right-assoc) -------

    fn parse_assign(&mut self) -> Result<Expr, ParseError> {
        let lhs = self.parse_range()?;
        let op = match self.peek() {
            TokenKind::Eq => Some(AssignOp::Eq),
            TokenKind::PlusEq => Some(AssignOp::AddEq),
            TokenKind::MinusEq => Some(AssignOp::SubEq),
            TokenKind::StarEq => Some(AssignOp::MulEq),
            TokenKind::SlashEq => Some(AssignOp::DivEq),
            TokenKind::PercentEq => Some(AssignOp::RemEq),
            _ => None,
        };
        if let Some(op) = op {
            self.advance();
            let rhs = self.parse_assign()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            return Ok(Expr::Assign {
                op,
                target: Box::new(lhs),
                value: Box::new(rhs),
                span,
            });
        }
        Ok(lhs)
    }

    // ------- range -------

    fn parse_range(&mut self) -> Result<Expr, ParseError> {
        // Prefix range with no start: `..end` / `..=end` / `..` (full).
        if matches!(self.peek(), TokenKind::DotDot | TokenKind::DotDotEq) {
            let inclusive = matches!(self.peek(), TokenKind::DotDotEq);
            let start_span = self.advance().span;
            if self.expr_can_start() {
                let end = self.parse_or()?;
                let span = Self::span_join(start_span, end.span());
                return Ok(Expr::Range {
                    start: None,
                    end: Some(Box::new(end)),
                    inclusive,
                    span,
                });
            }
            return Ok(Expr::Range {
                start: None,
                end: None,
                inclusive,
                span: start_span,
            });
        }

        let lhs = self.parse_or()?;
        match self.peek() {
            TokenKind::DotDot | TokenKind::DotDotEq => {
                let inclusive = matches!(self.peek(), TokenKind::DotDotEq);
                self.advance();
                if self.expr_can_start() {
                    let rhs = self.parse_or()?;
                    let span = Self::span_join(lhs.span(), rhs.span());
                    Ok(Expr::Range {
                        start: Some(Box::new(lhs)),
                        end: Some(Box::new(rhs)),
                        inclusive,
                        span,
                    })
                } else {
                    let span = lhs.span();
                    Ok(Expr::Range {
                        start: Some(Box::new(lhs)),
                        end: None,
                        inclusive,
                        span,
                    })
                }
            }
            _ => Ok(lhs),
        }
    }

    // ------- logical -------

    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_and()?;
        while self.at_kw(Keyword::Or) {
            self.advance();
            let rhs = self.parse_and()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op: BinOp::Or,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_is_as()?;
        while self.at_kw(Keyword::And) {
            self.advance();
            let rhs = self.parse_is_as()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op: BinOp::And,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    // ------- is / as -------

    fn parse_is_as(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_cmp()?;
        loop {
            if self.at_kw(Keyword::Is) {
                self.advance();
                let pat = self.parse_pattern()?;
                let span = Self::span_join(lhs.span(), pat.span());
                lhs = Expr::IsCheck {
                    expr: Box::new(lhs),
                    pat: Box::new(pat),
                    span,
                };
            } else if self.at_kw(Keyword::As) {
                self.advance();
                let ty = self.parse_type()?;
                let ty_span = ty.span();
                let span = Self::span_join(lhs.span(), ty_span);
                lhs = Expr::Cast {
                    expr: Box::new(lhs),
                    ty,
                    span,
                };
            } else {
                break;
            }
        }
        Ok(lhs)
    }

    // ------- comparison / equality -------

    fn parse_cmp(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_bitops()?;
        loop {
            let op = match self.peek() {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::BangEq => BinOp::Ne,
                TokenKind::Lt => BinOp::Lt,
                TokenKind::LtEq => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::GtEq => BinOp::Ge,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_bitops()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    // ------- bitwise & | ^ -------

    fn parse_bitops(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_shift()?;
        loop {
            let op = match self.peek() {
                TokenKind::Amp => BinOp::BitAnd,
                TokenKind::Pipe => BinOp::BitOr,
                TokenKind::Caret => BinOp::BitXor,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_shift()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    // ------- shift << >> -------

    fn parse_shift(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_addsub()?;
        loop {
            let op = match self.peek() {
                TokenKind::LtLt => BinOp::Shl,
                TokenKind::GtGt => BinOp::Shr,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_addsub()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    // ------- + - -------

    fn parse_addsub(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_muldiv()?;
        loop {
            let op = match self.peek() {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_muldiv()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    // ------- * / % -------

    fn parse_muldiv(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            self.advance();
            let rhs = self.parse_unary()?;
            let span = Self::span_join(lhs.span(), rhs.span());
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    // ------- prefix unary -------

    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek() {
            TokenKind::Minus => {
                let start = self.advance().span;
                let inner = self.parse_unary()?;
                let span = Self::span_join(start, inner.span());
                Ok(Expr::Unary {
                    op: UnOp::Neg,
                    expr: Box::new(inner),
                    span,
                })
            }
            TokenKind::Keyword(Keyword::Not) => {
                let start = self.advance().span;
                let inner = self.parse_unary()?;
                let span = Self::span_join(start, inner.span());
                Ok(Expr::Unary {
                    op: UnOp::Not,
                    expr: Box::new(inner),
                    span,
                })
            }
            _ => self.parse_postfix(),
        }
    }

    // ------- postfix . ?  (...) [...] -------

    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut e = self.parse_primary()?;
        loop {
            match self.peek() {
                TokenKind::Dot => {
                    self.advance();
                    // Accept `*` as a wildcard suffix — used by the
                    // policy `match: http.*` surface (§ 15.1). Outside
                    // a policy this is just a field named `"*"` and
                    // resolves to "no such field" at runtime.
                    let (name, name_span) = if matches!(self.peek(), TokenKind::Star) {
                        let t = self.advance();
                        ("*".to_string(), t.span)
                    } else {
                        self.expect_ident()?
                    };
                    let span = Self::span_join(e.span(), name_span);
                    e = Expr::Field {
                        base: Box::new(e),
                        name,
                        span,
                    };
                }
                TokenKind::LParen => {
                    self.advance();
                    let mut args = Vec::new();
                    if !matches!(self.peek(), TokenKind::RParen) {
                        args.push(self.parse_call_arg()?);
                        while matches!(self.peek(), TokenKind::Comma) {
                            self.advance();
                            if matches!(self.peek(), TokenKind::RParen) {
                                break;
                            }
                            args.push(self.parse_call_arg()?);
                        }
                    }
                    let end = self.expect_kind(&TokenKind::RParen)?.span;
                    let span = Self::span_join(e.span(), end);
                    e = Expr::Call {
                        callee: Box::new(e),
                        type_args: Vec::new(),
                        args,
                        span,
                    };
                }
                TokenKind::Lt => {
                    let saved_pos = self.pos;
                    match self.try_parse_turbofish() {
                        Some(type_args) if matches!(self.peek(), TokenKind::LParen) => {
                            self.advance(); // (
                            let mut args = Vec::new();
                            if !matches!(self.peek(), TokenKind::RParen) {
                                args.push(self.parse_call_arg()?);
                                while matches!(self.peek(), TokenKind::Comma) {
                                    self.advance();
                                    if matches!(self.peek(), TokenKind::RParen) {
                                        break;
                                    }
                                    args.push(self.parse_call_arg()?);
                                }
                            }
                            let end = self.expect_kind(&TokenKind::RParen)?.span;
                            let span = Self::span_join(e.span(), end);
                            e = Expr::Call {
                                callee: Box::new(e),
                                type_args,
                                args,
                                span,
                            };
                        }
                        _ => {
                            self.pos = saved_pos;
                            break;
                        }
                    }
                }
                TokenKind::LBracket => {
                    self.advance();
                    let saved = self.allow_struct_lit;
                    self.allow_struct_lit = true;
                    let index = self.parse_expr()?;
                    self.allow_struct_lit = saved;
                    let end = self.expect_kind(&TokenKind::RBracket)?.span;
                    let span = Self::span_join(e.span(), end);
                    e = Expr::Index {
                        base: Box::new(e),
                        index: Box::new(index),
                        span,
                    };
                }
                TokenKind::Question => {
                    let q = self.advance().span;
                    let span = Self::span_join(e.span(), q);
                    e = Expr::Try {
                        expr: Box::new(e),
                        span,
                    };
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn parse_call_arg(&mut self) -> Result<CallArg, ParseError> {
        let start = self.peek_token().span;
        // Named argument form: `name: expr`.
        if let TokenKind::Ident(_) = self.peek() {
            if matches!(self.peek_at(1), Some(TokenKind::Colon)) {
                let (name, _) = self.expect_ident()?;
                self.advance(); // :
                let value = self.parse_expr()?;
                let span = Self::span_join(start, value.span());
                return Ok(CallArg {
                    name: Some(name),
                    value,
                    span,
                });
            }
        }
        let value = self.parse_expr()?;
        let span = Self::span_join(start, value.span());
        Ok(CallArg {
            name: None,
            value,
            span,
        })
    }

    // ------- primary -------

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.peek_token().clone();
        match &tok.kind {
            TokenKind::Int(n) => {
                self.advance();
                Ok(Expr::Int(*n, tok.span))
            }
            TokenKind::Float(f) => {
                self.advance();
                Ok(Expr::Float(*f, tok.span))
            }
            TokenKind::Str(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Str(s, tok.span))
            }
            TokenKind::Bytes(b) => {
                let b = b.clone();
                self.advance();
                Ok(Expr::Bytes(b, tok.span))
            }
            TokenKind::Char(c) => {
                self.advance();
                Ok(Expr::Char(*c, tok.span))
            }
            TokenKind::Date(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Date(s, tok.span))
            }
            TokenKind::Timestamp(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Timestamp(s, tok.span))
            }
            TokenKind::Duration(s) => {
                let s = s.clone();
                self.advance();
                Ok(Expr::Duration(s, tok.span))
            }
            TokenKind::Keyword(Keyword::True) => {
                self.advance();
                Ok(Expr::Bool(true, tok.span))
            }
            TokenKind::Keyword(Keyword::False) => {
                self.advance();
                Ok(Expr::Bool(false, tok.span))
            }
            TokenKind::LParen => self.parse_paren_or_tuple(),
            TokenKind::LBracket => self.parse_list_literal(),
            TokenKind::LBrace => {
                if self.brace_starts_record_lit() {
                    self.parse_anon_record_lit()
                } else {
                    let blk = self.parse_block()?;
                    let span = blk.span;
                    Ok(Expr::Block(blk, span))
                }
            }
            TokenKind::Keyword(Keyword::If) => self.parse_if_expr(),
            TokenKind::Keyword(Keyword::Match) => self.parse_match_expr(),
            TokenKind::Keyword(Keyword::Fn) => self.parse_lambda(),
            TokenKind::Keyword(Keyword::Spawn) => self.parse_spawn(),
            TokenKind::Keyword(Keyword::Await) => self.parse_await(),
            TokenKind::Keyword(Keyword::Raise) => self.parse_raise(),
            TokenKind::Keyword(Keyword::Return) => self.parse_return(),
            TokenKind::Keyword(Keyword::Break) => self.parse_break(),
            TokenKind::Keyword(Keyword::Continue) => self.parse_continue(),
            TokenKind::Keyword(Keyword::Cap) => self.parse_cap_primary(),
            TokenKind::Keyword(Keyword::Intent) => self.parse_intent_block(),
            TokenKind::Ident(_) => self.parse_ident_or_record_lit(),
            _ => Err(self.err(ParseErrorKind::Expected("expression".into()))),
        }
    }

    fn parse_intent_block(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Intent)?.span;
        let label = self.expect_string_literal()?;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::IntentBlock {
            label,
            body,
            span: Self::span_join(start, end),
        })
    }

    fn parse_paren_or_tuple(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kind(&TokenKind::LParen)?.span;
        // `()` — unit.
        if matches!(self.peek(), TokenKind::RParen) {
            let end = self.advance().span;
            return Ok(Expr::Unit(Self::span_join(start, end)));
        }
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = true;
        let first = self.parse_expr()?;
        if matches!(self.peek(), TokenKind::Comma) {
            self.advance();
            let mut elems = vec![first];
            // Allow trailing comma: `(a,)` is still a 1-tuple.
            if !matches!(self.peek(), TokenKind::RParen) {
                elems.push(self.parse_expr()?);
                while matches!(self.peek(), TokenKind::Comma) {
                    self.advance();
                    if matches!(self.peek(), TokenKind::RParen) {
                        break;
                    }
                    elems.push(self.parse_expr()?);
                }
            }
            let end = self.expect_kind(&TokenKind::RParen)?.span;
            self.allow_struct_lit = saved;
            return Ok(Expr::Tuple(elems, Self::span_join(start, end)));
        }
        self.expect_kind(&TokenKind::RParen)?;
        self.allow_struct_lit = saved;
        Ok(first)
    }

    fn parse_list_literal(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kind(&TokenKind::LBracket)?.span;
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = true;
        let mut elems = Vec::new();
        if !matches!(self.peek(), TokenKind::RBracket) {
            elems.push(self.parse_expr()?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                if matches!(self.peek(), TokenKind::RBracket) {
                    break;
                }
                elems.push(self.parse_expr()?);
            }
        }
        let end = self.expect_kind(&TokenKind::RBracket)?.span;
        self.allow_struct_lit = saved;
        Ok(Expr::List(elems, Self::span_join(start, end)))
    }

    fn parse_ident_or_record_lit(&mut self) -> Result<Expr, ParseError> {
        let (name, name_span) = self.expect_ident()?;
        // Optional `@vN` suffix (§ 4.5). When followed by `{ ... }`
        // we are looking at the model-literal shape `Invoice@v1 { id: ... }`
        // (M8.T1); otherwise it is a value-position model reference.
        let mut model_version: Option<u32> = None;
        let mut last_span = name_span;
        if matches!(self.peek(), TokenKind::At) {
            if let Some(TokenKind::Ident(s)) = self.peek_at(1) {
                if s.len() > 1 && s.starts_with('v') && s[1..].chars().all(|c| c.is_ascii_digit()) {
                    self.advance(); // @
                    let v_tok = self.advance();
                    let v_str = match v_tok.kind {
                        TokenKind::Ident(s) => s,
                        _ => unreachable!(),
                    };
                    let version = v_str[1..]
                        .parse::<u32>()
                        .map_err(|_| self.err(ParseErrorKind::InvalidModelVersion))?;
                    model_version = Some(version);
                    last_span = v_tok.span;
                }
            }
        }
        if self.allow_struct_lit
            && matches!(self.peek(), TokenKind::LBrace)
            && self.brace_starts_record_lit()
        {
            return self.parse_record_lit_with_name_version(name, model_version, name_span);
        }
        if let Some(v) = model_version {
            return Ok(Expr::ModelRef {
                name,
                version: v,
                span: Self::span_join(name_span, last_span),
            });
        }
        Ok(Expr::Ident(name, name_span))
    }

    fn parse_record_lit_with_name_version(
        &mut self,
        ty_name: String,
        ty_version: Option<u32>,
        start: Span,
    ) -> Result<Expr, ParseError> {
        let _ = self.expect_kind(&TokenKind::LBrace)?;
        let (fields, spread) = self.parse_record_lit_body()?;
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(Expr::Record(
            RecordLit {
                ty_name: Some(ty_name),
                ty_version,
                fields,
                spread,
            },
            Self::span_join(start, end),
        ))
    }

    /// Anonymous `{ a: 1, b: 2 }` record / map literal at primary position.
    fn parse_anon_record_lit(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kind(&TokenKind::LBrace)?.span;
        let (fields, spread) = self.parse_record_lit_body()?;
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(Expr::Record(
            RecordLit {
                ty_name: None,
                ty_version: None,
                fields,
                spread,
            },
            Self::span_join(start, end),
        ))
    }

    /// Body of a record literal. `..expr` may appear at any position;
    /// fields and the spread are unioned into the resulting `RecordLit`.
    fn parse_record_lit_body(
        &mut self,
    ) -> Result<(Vec<RecordLitField>, Option<Box<Expr>>), ParseError> {
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = true;
        let mut fields = Vec::new();
        let mut spread: Option<Box<Expr>> = None;
        while !matches!(self.peek(), TokenKind::RBrace) {
            if matches!(self.peek(), TokenKind::DotDot) {
                self.advance();
                let e = self.parse_expr()?;
                spread = Some(Box::new(e));
            } else {
                let field_start = self.peek_token().span;
                let (name, _) = self.expect_ident()?;
                self.expect_kind(&TokenKind::Colon)?;
                let value = self.parse_expr()?;
                let span = Self::span_join(field_start, value.span());
                fields.push(RecordLitField { name, value, span });
            }
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.allow_struct_lit = saved;
        Ok((fields, spread))
    }

    /// Look one token past `{`: a record literal starts with `<ident> :`,
    /// or with `..`. Anything else means the brace is the head of a block.
    fn brace_starts_record_lit(&self) -> bool {
        if !matches!(self.peek(), TokenKind::LBrace) {
            return false;
        }
        match self.peek_at(1) {
            Some(TokenKind::Ident(_)) => matches!(self.peek_at(2), Some(TokenKind::Colon)),
            Some(TokenKind::DotDot) => true,
            _ => false,
        }
    }

    /// `cap` keyword as a primary. `cap.subset[..]` / `cap.test_subset[..]`
    /// (§ 8.4) parse a `CapEntry` list — the same grammar as `cap[..]`
    /// in signatures (M1.T7) — minus the `*` form, since narrowing must
    /// always restrict. All other `cap.<x>` forms fall back to the
    /// generic field-access path so the postfix loop can chain calls.
    fn parse_cap_primary(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Cap)?.span;
        if matches!(self.peek(), TokenKind::Dot) {
            if let Some(TokenKind::Ident(name)) = self.peek_at(1) {
                if (name == "subset" || name == "test_subset")
                    && matches!(self.peek_at(2), Some(TokenKind::LBracket))
                {
                    let kind = if name == "subset" {
                        CapNarrowKind::Subset
                    } else {
                        CapNarrowKind::TestSubset
                    };
                    self.advance(); // .
                    self.advance(); // subset / test_subset
                    self.expect_kind(&TokenKind::LBracket)?;
                    let entries = self.parse_cap_entry_list()?;
                    let end = self.expect_kind(&TokenKind::RBracket)?.span;
                    let span = Self::span_join(start, end);
                    return Ok(Expr::CapNarrow {
                        kind,
                        entries,
                        span,
                    });
                }
            }
        }
        Ok(Expr::Ident("cap".to_string(), start))
    }

    // ------- if -------

    fn parse_if_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::If)?.span;
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = false;
        let cond = self.parse_expr()?;
        self.allow_struct_lit = saved;
        let then_blk = self.parse_block()?;
        let mut end_span = then_blk.span;
        let else_ = if self.eat_kw(Keyword::Else) {
            if self.at_kw(Keyword::If) {
                let nested = self.parse_if_expr()?;
                end_span = nested.span();
                Some(ElseBranch::ElseIf(Box::new(nested)))
            } else {
                let blk = self.parse_block()?;
                end_span = blk.span;
                Some(ElseBranch::Else(blk))
            }
        } else {
            None
        };
        Ok(Expr::If {
            cond: Box::new(cond),
            then_blk,
            else_,
            span: Self::span_join(start, end_span),
        })
    }

    // ------- match -------

    fn parse_match_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Match)?.span;
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = false;
        let scrutinee = self.parse_expr()?;
        self.allow_struct_lit = saved;
        self.expect_kind(&TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !matches!(self.peek(), TokenKind::RBrace) {
            arms.push(self.parse_match_arm()?);
            if matches!(self.peek(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        Ok(Expr::Match {
            scrutinee: Box::new(scrutinee),
            arms,
            span: Self::span_join(start, end),
        })
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.peek_token().span;
        let pattern = self.parse_pattern()?;
        let guard = if self.at_kw(Keyword::If) {
            self.advance();
            let saved = self.allow_struct_lit;
            self.allow_struct_lit = false;
            let g = self.parse_expr()?;
            self.allow_struct_lit = saved;
            Some(g)
        } else {
            None
        };
        self.expect_kind(&TokenKind::Arrow)?;
        let body = self.parse_expr()?;
        let span = Self::span_join(start, body.span());
        Ok(MatchArm {
            pattern,
            guard,
            body,
            span,
        })
    }

    // ------- patterns -------

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.peek_token().clone();
        match &tok.kind {
            // wildcard / bind
            TokenKind::Ident(name) if name == "_" => {
                self.advance();
                Ok(Pattern::Wildcard(tok.span))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                match self.peek() {
                    TokenKind::LParen => {
                        self.advance();
                        let mut args = Vec::new();
                        if !matches!(self.peek(), TokenKind::RParen) {
                            args.push(self.parse_pattern()?);
                            while matches!(self.peek(), TokenKind::Comma) {
                                self.advance();
                                if matches!(self.peek(), TokenKind::RParen) {
                                    break;
                                }
                                args.push(self.parse_pattern()?);
                            }
                        }
                        let end = self.expect_kind(&TokenKind::RParen)?.span;
                        Ok(Pattern::Constructor {
                            name,
                            args,
                            span: Self::span_join(tok.span, end),
                        })
                    }
                    TokenKind::LBrace => {
                        self.advance();
                        let mut fields = Vec::new();
                        let mut rest = false;
                        loop {
                            if matches!(self.peek(), TokenKind::RBrace) {
                                break;
                            }
                            if matches!(self.peek(), TokenKind::DotDot) {
                                self.advance();
                                rest = true;
                                if matches!(self.peek(), TokenKind::Comma) {
                                    self.advance();
                                }
                                break;
                            }
                            let f_start = self.peek_token().span;
                            let (fname, fname_span) = self.expect_ident()?;
                            let pat = if matches!(self.peek(), TokenKind::Colon) {
                                self.advance();
                                Some(self.parse_pattern()?)
                            } else {
                                None
                            };
                            let f_end = pat.as_ref().map_or(fname_span, |p| p.span());
                            fields.push(RecordPatField {
                                name: fname,
                                pat,
                                span: Self::span_join(f_start, f_end),
                            });
                            if matches!(self.peek(), TokenKind::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        let end = self.expect_kind(&TokenKind::RBrace)?.span;
                        Ok(Pattern::RecordCtor {
                            name,
                            fields,
                            rest,
                            span: Self::span_join(tok.span, end),
                        })
                    }
                    _ => {
                        // PascalCase ident in pattern position — `None`,
                        // `Pending`, `Red`, etc. — is a unit-constructor
                        // pattern per § 2.2 / § 17.1, *not* a fresh
                        // binder. snake_case names remain binds.
                        if name.chars().next().is_some_and(|c| c.is_ascii_uppercase()) {
                            Ok(Pattern::Constructor {
                                name,
                                args: Vec::new(),
                                span: tok.span,
                            })
                        } else {
                            Ok(Pattern::Bind(name, tok.span))
                        }
                    }
                }
            }
            // literals
            TokenKind::Int(_)
            | TokenKind::Float(_)
            | TokenKind::Str(_)
            | TokenKind::Char(_)
            | TokenKind::Date(_)
            | TokenKind::Timestamp(_)
            | TokenKind::Duration(_)
            | TokenKind::Keyword(Keyword::True)
            | TokenKind::Keyword(Keyword::False) => {
                let lit = self.parse_primary()?;
                let span = lit.span();
                Ok(Pattern::Lit(lit, span))
            }
            // negative-int literal: `-5`
            TokenKind::Minus => {
                let lit = self.parse_unary()?;
                let span = lit.span();
                Ok(Pattern::Lit(lit, span))
            }
            // tuple
            TokenKind::LParen => {
                self.advance();
                let mut elems = Vec::new();
                if !matches!(self.peek(), TokenKind::RParen) {
                    elems.push(self.parse_pattern()?);
                    while matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                        if matches!(self.peek(), TokenKind::RParen) {
                            break;
                        }
                        elems.push(self.parse_pattern()?);
                    }
                }
                let end = self.expect_kind(&TokenKind::RParen)?.span;
                Ok(Pattern::Tuple {
                    elems,
                    span: Self::span_join(tok.span, end),
                })
            }
            // list
            TokenKind::LBracket => {
                self.advance();
                let mut elems = Vec::new();
                while !matches!(self.peek(), TokenKind::RBracket) {
                    if matches!(self.peek(), TokenKind::DotDot) {
                        self.advance();
                        let bind = if let TokenKind::Ident(_) = self.peek() {
                            Some(self.expect_ident()?.0)
                        } else {
                            None
                        };
                        elems.push(ListPatElem::Rest(bind));
                    } else {
                        elems.push(ListPatElem::Pat(self.parse_pattern()?));
                    }
                    if matches!(self.peek(), TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                let end = self.expect_kind(&TokenKind::RBracket)?.span;
                Ok(Pattern::List {
                    elems,
                    span: Self::span_join(tok.span, end),
                })
            }
            _ => Err(self.err(ParseErrorKind::Expected("pattern".into()))),
        }
    }

    // ------- block -------

    fn parse_block(&mut self) -> Result<Block, ParseError> {
        let start = self.expect_kind(&TokenKind::LBrace)?.span;
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = true;
        let mut stmts = Vec::new();
        let mut tail: Option<Box<Expr>> = None;
        while !matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
            let stmt_start = self.peek_token().span;
            // statement-start keywords
            match self.peek() {
                TokenKind::Keyword(Keyword::Let) => {
                    self.advance();
                    let (name, _) = self.expect_ident_or_underscore()?;
                    let ty = if matches!(self.peek(), TokenKind::Colon) {
                        self.advance();
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    self.expect_kind(&TokenKind::Eq)?;
                    let value = self.parse_expr()?;
                    let end = value.span();
                    stmts.push(Stmt::Let {
                        name,
                        ty,
                        value,
                        span: Self::span_join(stmt_start, end),
                    });
                    self.eat_kind(&TokenKind::Semicolon);
                    continue;
                }
                TokenKind::Keyword(Keyword::Var) => {
                    self.advance();
                    let (name, _) = self.expect_ident_or_underscore()?;
                    let ty = if matches!(self.peek(), TokenKind::Colon) {
                        self.advance();
                        Some(self.parse_type()?)
                    } else {
                        None
                    };
                    self.expect_kind(&TokenKind::Eq)?;
                    let value = self.parse_expr()?;
                    let end = value.span();
                    stmts.push(Stmt::Var {
                        name,
                        ty,
                        value,
                        span: Self::span_join(stmt_start, end),
                    });
                    self.eat_kind(&TokenKind::Semicolon);
                    continue;
                }
                TokenKind::Keyword(Keyword::For) => {
                    self.advance();
                    let (var, _) = self.expect_ident_or_underscore()?;
                    self.expect_kw(Keyword::In)?;
                    let no_struct = self.allow_struct_lit;
                    self.allow_struct_lit = false;
                    let iter = self.parse_expr()?;
                    self.allow_struct_lit = no_struct;
                    let body = self.parse_block()?;
                    let end = body.span;
                    stmts.push(Stmt::For {
                        var,
                        iter,
                        body,
                        span: Self::span_join(stmt_start, end),
                    });
                    self.eat_kind(&TokenKind::Semicolon);
                    continue;
                }
                TokenKind::Keyword(Keyword::While) => {
                    self.advance();
                    let no_struct = self.allow_struct_lit;
                    self.allow_struct_lit = false;
                    let cond = self.parse_expr()?;
                    self.allow_struct_lit = no_struct;
                    let body = self.parse_block()?;
                    let end = body.span;
                    stmts.push(Stmt::While {
                        cond,
                        body,
                        span: Self::span_join(stmt_start, end),
                    });
                    self.eat_kind(&TokenKind::Semicolon);
                    continue;
                }
                _ => {}
            }
            // expression-statement / tail
            let e = self.parse_expr()?;
            if self.eat_kind(&TokenKind::Semicolon) {
                stmts.push(Stmt::Expr(e));
            } else if matches!(self.peek(), TokenKind::RBrace | TokenKind::Eof) {
                tail = Some(Box::new(e));
                break;
            } else {
                stmts.push(Stmt::Expr(e));
            }
        }
        let end = self.expect_kind(&TokenKind::RBrace)?.span;
        self.allow_struct_lit = saved;
        Ok(Block {
            stmts,
            tail,
            span: Self::span_join(start, end),
        })
    }

    fn expect_ident_or_underscore(&mut self) -> Result<(String, Span), ParseError> {
        if let TokenKind::Ident(name) = self.peek() {
            if name == "_" {
                let t = self.advance();
                return Ok(("_".to_string(), t.span));
            }
        }
        self.expect_ident()
    }

    fn eat_kind(&mut self, kind: &TokenKind) -> bool {
        if std::mem::discriminant(self.peek()) == std::mem::discriminant(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    // ------- lambda / spawn / await / raise / return / break / continue -------

    fn parse_lambda(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Fn)?.span;
        self.expect_kind(&TokenKind::LParen)?;
        let mut params = Vec::new();
        if !matches!(self.peek(), TokenKind::RParen) {
            params.push(self.parse_lambda_param()?);
            while matches!(self.peek(), TokenKind::Comma) {
                self.advance();
                if matches!(self.peek(), TokenKind::RParen) {
                    break;
                }
                params.push(self.parse_lambda_param()?);
            }
        }
        self.expect_kind(&TokenKind::RParen)?;
        let ret_ty = if matches!(self.peek(), TokenKind::Arrow) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::Lambda {
            params,
            ret_ty,
            body,
            span: Self::span_join(start, end),
        })
    }

    fn parse_lambda_param(&mut self) -> Result<LambdaParam, ParseError> {
        let start = self.peek_token().span;
        let (name, name_span) = self.expect_ident()?;
        let ty = if matches!(self.peek(), TokenKind::Colon) {
            self.advance();
            Some(self.parse_type()?)
        } else {
            None
        };
        let end = ty.as_ref().map_or(name_span, |t| t.span());
        Ok(LambdaParam {
            name,
            ty,
            span: Self::span_join(start, end),
        })
    }

    fn parse_spawn(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Spawn)?.span;
        let body = self.parse_block()?;
        let end = body.span;
        Ok(Expr::Spawn {
            body,
            span: Self::span_join(start, end),
        })
    }

    fn parse_await(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Await)?.span;
        let inner = self.parse_unary()?;
        let span = Self::span_join(start, inner.span());
        Ok(Expr::Await {
            expr: Box::new(inner),
            span,
        })
    }

    fn parse_raise(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Raise)?.span;
        let inner = self.parse_expr()?;
        let span = Self::span_join(start, inner.span());
        Ok(Expr::Raise {
            expr: Box::new(inner),
            span,
        })
    }

    fn parse_return(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Return)?.span;
        if self.expr_can_start() {
            let e = self.parse_expr()?;
            let span = Self::span_join(start, e.span());
            Ok(Expr::Return {
                expr: Some(Box::new(e)),
                span,
            })
        } else {
            Ok(Expr::Return {
                expr: None,
                span: start,
            })
        }
    }

    fn parse_break(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Break)?.span;
        let label = if let TokenKind::Label(s) = self.peek() {
            let s = s.clone();
            self.advance();
            Some(s)
        } else {
            None
        };
        let (inner, end) = if self.expr_can_start() {
            let e = self.parse_expr()?;
            let span = e.span();
            (Some(Box::new(e)), span)
        } else {
            (None, start)
        };
        Ok(Expr::Break {
            label,
            expr: inner,
            span: Self::span_join(start, end),
        })
    }

    fn parse_continue(&mut self) -> Result<Expr, ParseError> {
        let start = self.expect_kw(Keyword::Continue)?.span;
        let (label, end) = if let TokenKind::Label(s) = self.peek() {
            let s = s.clone();
            let sp = self.advance().span;
            (Some(s), sp)
        } else {
            (None, start)
        };
        Ok(Expr::Continue {
            label,
            span: Self::span_join(start, end),
        })
    }

    /// Whether the current token can begin a fresh expression. Used by
    /// `parse_range` (to detect `expr..` with no end), `parse_return`,
    /// and `parse_break` — none of which require a following expression.
    fn expr_can_start(&self) -> bool {
        matches!(
            self.peek(),
            TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::Str(_)
                | TokenKind::Bytes(_)
                | TokenKind::Char(_)
                | TokenKind::Date(_)
                | TokenKind::Timestamp(_)
                | TokenKind::Duration(_)
                | TokenKind::Ident(_)
                | TokenKind::LParen
                | TokenKind::LBracket
                | TokenKind::LBrace
                | TokenKind::Minus
                | TokenKind::Keyword(
                    Keyword::True
                        | Keyword::False
                        | Keyword::If
                        | Keyword::Match
                        | Keyword::Fn
                        | Keyword::Spawn
                        | Keyword::Await
                        | Keyword::Raise
                        | Keyword::Return
                        | Keyword::Break
                        | Keyword::Continue
                        | Keyword::Cap
                        | Keyword::Intent
                        | Keyword::Not,
                )
        )
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
            Item::Saga(_) => "saga",
            Item::Agent(_) => "agent",
            Item::AgentNet(_) => "agent_net",
            Item::Policy(_) => "policy",
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

// ====================================================================
// M1.T7 — capability-type fixtures.
//
// Covers `cap[entry, ...]` in signatures and `cap.subset[entry, ...]`
// in expression position. Both forms `@ "x"` and `@ ["x", "y"]` are
// accepted; `cap[*]` parses with `star = true` (rejected by `check::`
// in M2.T5).
// ====================================================================

#[cfg(test)]
mod cap_tests {
    use super::super::ast::{CapNarrowKind, Expr, Item, Type};
    use super::*;

    fn cap_ty(src: &str) -> (Vec<super::super::ast::CapEntry>, bool) {
        let m = match parse(src) {
            Ok(m) => m,
            Err(e) => panic!("parse: {e:?} on {src:?}"),
        };
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!("expected fn"),
        };
        match &f.params[0].ty {
            Type::Cap { entries, star, .. } => (entries.clone(), *star),
            other => panic!("expected Cap, got {other:?}"),
        }
    }

    #[test]
    fn single_segment_cap() {
        let (es, star) = cap_ty("fn f(cap: cap[audit]) {}");
        assert!(!star);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].path.segments, vec!["audit".to_string()]);
        assert!(es[0].allow.is_none());
    }

    #[test]
    fn two_segment_cap() {
        let (es, _) = cap_ty("fn f(cap: cap[fs.write_file]) {}");
        assert_eq!(es[0].path.segments, vec!["fs", "write_file"]);
    }

    #[test]
    fn cap_with_single_string_allow() {
        let (es, _) = cap_ty(r#"fn f(cap: cap[http.get @ "api.acme.com"]) {}"#);
        assert_eq!(es[0].path.segments, vec!["http", "get"]);
        assert_eq!(
            es[0].allow.as_ref().unwrap(),
            &vec!["api.acme.com".to_string()]
        );
    }

    #[test]
    fn cap_with_list_allow() {
        let (es, _) =
            cap_ty(r#"fn f(cap: cap[http.post @ ["api.acme.com", "api.stripe.com"]]) {}"#);
        assert_eq!(
            es[0].allow.as_ref().unwrap(),
            &vec!["api.acme.com".to_string(), "api.stripe.com".to_string()]
        );
    }

    #[test]
    fn cap_with_multiple_entries() {
        let src = r#"fn f(cap: cap[
            http.post  @ ["api.acme.com"],
            kube.apply @ ["prod-eu-1"],
            audit.event,
        ]) {}"#;
        let (es, _) = cap_ty(src);
        assert_eq!(es.len(), 3);
        assert_eq!(es[0].path.segments, vec!["http", "post"]);
        assert_eq!(es[1].path.segments, vec!["kube", "apply"]);
        assert_eq!(es[2].path.segments, vec!["audit", "event"]);
        assert!(es[2].allow.is_none());
    }

    #[test]
    fn cap_star_parses_with_flag() {
        let (es, star) = cap_ty("fn f(cap: cap[*]) {}");
        assert!(star);
        assert!(es.is_empty());
    }

    #[test]
    fn cap_empty_brackets_parses() {
        let (es, star) = cap_ty("fn f(cap: cap[]) {}");
        assert!(!star);
        assert!(es.is_empty());
    }

    #[test]
    fn cap_subset_in_expression_position() {
        let e = parse_expression(r#"cap.subset[http.post @ ["api.acme.com"]]"#)
            .expect("parse cap.subset");
        match e {
            Expr::CapNarrow { kind, entries, .. } => {
                assert_eq!(kind, CapNarrowKind::Subset);
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].path.segments, vec!["http", "post"]);
                assert_eq!(
                    entries[0].allow.as_ref().unwrap(),
                    &vec!["api.acme.com".to_string()]
                );
            }
            other => panic!("expected CapNarrow, got {other:?}"),
        }
    }

    #[test]
    fn cap_test_subset_distinct_kind() {
        let e = parse_expression("cap.test_subset[fs.read_file]").expect("parse");
        match e {
            Expr::CapNarrow { kind, .. } => assert_eq!(kind, CapNarrowKind::TestSubset),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn cap_with_trailing_comma_allowed() {
        let (es, _) = cap_ty("fn f(cap: cap[fs.read_file, audit.event,]) {}");
        assert_eq!(es.len(), 2);
    }

    #[test]
    fn cap_invalid_path_three_segments_rejected() {
        // `fs.write_file.txt` — § 8.1 fixes the tree at exactly two levels.
        // Our parser stops after the second ident, so the trailing `.txt`
        // shows up as a stray token before `]`, producing a parse error.
        assert!(parse("fn f(cap: cap[fs.write_file.txt]) {}").is_err());
    }

    #[test]
    fn cap_allow_list_requires_string_literals() {
        // `@ [foo]` (bareword) must fail — only string literals are valid.
        assert!(parse("fn f(cap: cap[http.get @ [foo]]) {}").is_err());
    }
}

// ====================================================================
// M1.T8 — saga / agent / agent_net / policy / intent block.
//
// Each test asserts the golden AST shape per `docs/plan.md` § 5.1.
// ====================================================================

#[cfg(test)]
mod top_level_tests {
    use super::super::ast::{Expr, FlowStage, Item, UndoForm};
    use super::*;

    fn parse_one(src: &str) -> Item {
        let m = parse(src).unwrap_or_else(|e| panic!("parse: {e:?} on {src:?}"));
        m.items.into_iter().next().expect("one item expected")
    }

    // ---------- saga ----------

    #[test]
    fn saga_minimum_shape() {
        // Step names use plain identifiers (§ 2.3 reserves `record`,
        // so we use `issue` / `log` here). `undo` takes a block or the
        // bareword `noop` directly — no colon (§ 26 grammar).
        let src = r#"
            saga rotate(cap: cap[http.post @ ["vault"], audit.event]) {
                intent "rotate the production webhook secret"

                step issue {
                    do   { http.post("/rotate", "{}")? }
                    undo { http.post("/revoke", "{}")? }
                }

                step log {
                    requires: issue.ok
                    do   { audit.event("ok", { actor: "ops" }) }
                    undo noop
                }
            }
        "#;
        let s = match parse_one(src) {
            Item::Saga(s) => s,
            other => panic!("expected saga, got {other:?}"),
        };
        assert_eq!(s.name, "rotate");
        assert_eq!(s.intent, "rotate the production webhook secret");
        assert_eq!(s.params.len(), 1);
        assert_eq!(s.params[0].name, "cap");
        assert_eq!(s.steps.len(), 2);
        assert_eq!(s.steps[0].name, "issue");
        assert!(s.steps[0].requires.is_empty());
        assert!(matches!(s.steps[0].undo, UndoForm::Block(_)));
        assert_eq!(s.steps[1].name, "log");
        assert_eq!(s.steps[1].requires.len(), 1);
        assert!(matches!(s.steps[1].undo, UndoForm::Noop(_)));
    }

    #[test]
    fn saga_missing_intent_fails() {
        // saga with no `intent "..."` after `{` is a parse error per § 12.2.
        let src = r#"saga r(cap: cap[]) { step a { do {} undo noop } }"#;
        assert!(parse(src).is_err());
    }

    // ---------- agent ----------

    #[test]
    fn agent_minimum_shape() {
        let src = r#"
            agent classify {
                llm:     "claude-opus-4-7"
                intent:  "Classify the invoice"
                prompt:  "Classify invoice as one of four categories."
                accept:  Invoice@v1
                produce: Category@v1
                policy:  pii_redact, model_budget
                retries: 3
                budget:  { tokens: 4_000, latency: 5s }
            }
        "#;
        let a = match parse_one(src) {
            Item::Agent(a) => a,
            other => panic!("expected agent, got {other:?}"),
        };
        assert_eq!(a.name, "classify");
        let keys: Vec<&str> = a.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(
            keys,
            vec!["llm", "intent", "prompt", "accept", "produce", "policy", "retries", "budget"]
        );
        // accept: Invoice@v1 → ModelRef
        let accept = a.fields.iter().find(|f| f.key == "accept").unwrap();
        assert_eq!(accept.values.len(), 1);
        match &accept.values[0] {
            Expr::ModelRef { name, version, .. } => {
                assert_eq!(name, "Invoice");
                assert_eq!(*version, 1);
            }
            other => panic!("expected ModelRef, got {other:?}"),
        }
        // policy: pii_redact, model_budget → two values
        let policy = a.fields.iter().find(|f| f.key == "policy").unwrap();
        assert_eq!(policy.values.len(), 2);
        // retries: 3 → Int
        let retries = a.fields.iter().find(|f| f.key == "retries").unwrap();
        assert!(matches!(retries.values[0], Expr::Int(3, _)));
        // budget: { ... } → Record literal
        let budget = a.fields.iter().find(|f| f.key == "budget").unwrap();
        assert!(matches!(budget.values[0], Expr::Record(_, _)));
    }

    // ---------- agent_net ----------

    #[test]
    fn agent_net_minimum_shape() {
        let src = r#"
            agent_net invoice_pipeline {
                intent "extract → classify → route"

                flow extract -> classify -> route_or_alert
                flow route_or_alert -> { route, alert }

                until: classify.confidence > 0.95 or iterations >= 3
            }
        "#;
        let n = match parse_one(src) {
            Item::AgentNet(n) => n,
            other => panic!("expected agent_net, got {other:?}"),
        };
        assert_eq!(n.name, "invoice_pipeline");
        assert_eq!(n.intent.as_deref(), Some("extract → classify → route"));
        assert_eq!(n.flows.len(), 2);
        // flow 0: extract -> classify -> route_or_alert
        match &n.flows[0].stages[..] {
            [FlowStage::Single(a), FlowStage::Single(b), FlowStage::Single(c)] => {
                assert_eq!(
                    (a.as_str(), b.as_str(), c.as_str()),
                    ("extract", "classify", "route_or_alert")
                );
            }
            other => panic!("unexpected stages: {other:?}"),
        }
        // flow 1: route_or_alert -> { route, alert }
        match &n.flows[1].stages[..] {
            [FlowStage::Single(_), FlowStage::FanOut(branches)] => {
                assert_eq!(branches, &vec!["route".to_string(), "alert".to_string()]);
            }
            other => panic!("unexpected stages: {other:?}"),
        }
        assert!(n.until.is_some());
    }

    // ---------- policy ----------

    #[test]
    fn policy_minimum_shape() {
        let src = r#"
            policy production_egress {
                match: http_egress
                deny:  evil
                audit: { url: u, method: m }
            }
        "#;
        let p = match parse_one(src) {
            Item::Policy(p) => p,
            other => panic!("expected policy, got {other:?}"),
        };
        assert_eq!(p.name, "production_egress");
        let keys: Vec<&str> = p.fields.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, vec!["match", "deny", "audit"]);
        assert!(matches!(p.fields[2].values[0], Expr::Record(_, _)));
    }

    // ---------- intent block ----------

    #[test]
    fn intent_block_in_expression_position() {
        let e = parse_expression(r#"intent "rotate cert" { audit_log() }"#).unwrap();
        match e {
            Expr::IntentBlock { label, body, .. } => {
                assert_eq!(label, "rotate cert");
                assert!(body.tail.is_some());
            }
            other => panic!("{other:?}"),
        }
    }

    // ---------- multi-construct module ----------

    // ---------- M1.T9 — requires / ensures ----------

    #[test]
    fn fn_with_requires_and_ensures_clauses() {
        let m = parse(
            r#"
                fn discount(amount: decimal, pct: decimal) -> decimal
                    requires: amount >= 0
                    requires: pct >= 0 and pct <= 1
                    ensures:  result >= 0 and result <= amount
                {
                    amount * (1 - pct)
                }
            "#,
        )
        .expect("module parses");
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert_eq!(f.requires.len(), 2);
        assert_eq!(f.ensures.len(), 1);
        // requires[0]: amount >= 0
        match &f.requires[0] {
            Expr::Binary { op, .. } => assert_eq!(*op, super::super::ast::BinOp::Ge),
            other => panic!("{other:?}"),
        }
        // ensures[0]: result >= 0 and result <= amount
        match &f.ensures[0] {
            Expr::Binary { op, .. } => assert_eq!(*op, super::super::ast::BinOp::And),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn fn_without_contracts_has_empty_lists() {
        let m = parse("fn add(a: int, b: int) -> int {}").unwrap();
        let f = match &m.items[0] {
            Item::Fn(f) => f,
            _ => panic!(),
        };
        assert!(f.requires.is_empty());
        assert!(f.ensures.is_empty());
    }

    #[test]
    fn saga_step_requires_is_parsed_expression() {
        // Step `log` requires `issue.ok` — a field access expression.
        let src = r#"
            saga s(cap: cap[]) {
                intent "x"
                step issue { do {} undo noop }
                step log {
                    requires: issue.ok
                    do {}
                    undo noop
                }
            }
        "#;
        let m = parse(src).unwrap();
        let s = match &m.items[0] {
            Item::Saga(s) => s,
            _ => panic!(),
        };
        assert_eq!(s.steps[1].requires.len(), 1);
        match &s.steps[1].requires[0] {
            Expr::Field { name, .. } => assert_eq!(name, "ok"),
            other => panic!("{other:?}"),
        }
    }

    // ---------- M1.T11 — error recovery ----------

    #[test]
    fn recovery_skips_one_bad_item_keeps_others() {
        // The middle item is malformed; the surrounding items still
        // parse, and we get exactly one error — not a cascade.
        let src = r#"
            fn good_one() {}
            strudel Bad {}
            fn good_two() {}
        "#;
        let outcome = super::parse_recovering(src);
        assert_eq!(outcome.module.items.len(), 2);
        assert_eq!(outcome.errors.len(), 1);
        let names: Vec<&str> = outcome
            .module
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Fn(f) => Some(f.name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, vec!["good_one", "good_two"]);
    }

    #[test]
    fn recovery_collects_multiple_errors() {
        // Two independent malformed items; each contributes one error.
        let src = r#"
            strudel Bad1 {}
            fn ok() {}
            wibble Bad2 {}
            record R { id: uuid }
        "#;
        let outcome = super::parse_recovering(src);
        assert_eq!(outcome.errors.len(), 2);
        assert_eq!(outcome.module.items.len(), 2);
    }

    #[test]
    fn recovery_handles_missing_brace_inside_record() {
        // The unfinished record swallows tokens until it finds a fresh
        // item-start; `fn after` survives as a parsed item.
        let src = r#"
            record R { id: uuid
            fn after() {}
        "#;
        let outcome = super::parse_recovering(src);
        // We tolerate either ordering — what matters is that `fn after`
        // ends up in the module and at least one error is reported.
        assert!(!outcome.errors.is_empty());
        let has_fn = outcome
            .module
            .items
            .iter()
            .any(|it| matches!(it, Item::Fn(f) if f.name == "after"));
        assert!(
            has_fn,
            "expected `fn after` to recover, got {:?}",
            outcome.module.items
        );
    }

    #[test]
    fn recovery_no_errors_on_clean_input() {
        let outcome = super::parse_recovering(
            r#"
                fn a() {}
                record R { id: uuid }
                policy egress { match: x }
            "#,
        );
        assert!(outcome.errors.is_empty());
        assert_eq!(outcome.module.items.len(), 3);
    }

    #[test]
    fn module_with_saga_agent_net_policy() {
        let src = r#"
            policy egress { match: http_get  deny: not_allowed }

            agent classify {
                llm: "x"
                accept: Invoice@v1
                produce: Category@v1
            }

            agent_net p {
                flow extract -> classify
                until: iterations >= 3
            }

            saga ship(cap: cap[]) {
                intent "ship release"
                step apply { do { ok() } undo noop }
            }
        "#;
        let m = parse(src).expect("module parses");
        assert_eq!(m.items.len(), 4);
        assert!(matches!(m.items[0], Item::Policy(_)));
        assert!(matches!(m.items[1], Item::Agent(_)));
        assert!(matches!(m.items[2], Item::AgentNet(_)));
        assert!(matches!(m.items[3], Item::Saga(_)));
    }
}

// ====================================================================
// M1.T6 — expression fixtures (40+ per `docs/plan.md` § 5.1).
//
// Each test is one fixture. Tests are grouped by category and assert
// either the structural shape of the AST (via `dump`) or a specific
// invariant (precedence, associativity, postfix chaining).
// ====================================================================

#[cfg(test)]
mod expr_tests {
    use super::super::ast::{
        AssignOp, BinOp, CapNarrowKind, ElseBranch, Expr, ListPatElem, Pattern, Stmt, UnOp,
    };
    use super::*;

    fn p(src: &str) -> Expr {
        match parse_expression(src) {
            Ok(e) => e,
            Err(e) => panic!("parse_expression({src:?}) failed: {e:?}"),
        }
    }

    /// S-expression dump of an `Expr` for compact precedence assertions.
    fn dump(e: &Expr) -> String {
        match e {
            Expr::Int(n, _) => n.to_string(),
            Expr::Float(f, _) => format!("{f}"),
            Expr::Bool(b, _) => b.to_string(),
            Expr::Str(s, _) => format!("\"{}\"", s.replace('"', "\\\"")),
            Expr::Bytes(_, _) => "<bytes>".to_string(),
            Expr::Char(c, _) => format!("'{c}'"),
            Expr::Date(s, _) => format!("date:{s}"),
            Expr::Timestamp(s, _) => format!("ts:{s}"),
            Expr::Duration(s, _) => format!("dur:{s}"),
            Expr::Unit(_) => "()".into(),
            Expr::Ident(n, _) => n.clone(),
            Expr::Tuple(es, _) => format!(
                "(tuple {})",
                es.iter().map(dump).collect::<Vec<_>>().join(" ")
            ),
            Expr::List(es, _) => format!(
                "(list {})",
                es.iter().map(dump).collect::<Vec<_>>().join(" ")
            ),
            Expr::Record(rl, _) => {
                let mut parts = Vec::new();
                if let Some(n) = &rl.ty_name {
                    parts.push(format!(":ty {n}"));
                }
                for f in &rl.fields {
                    parts.push(format!("{}={}", f.name, dump(&f.value)));
                }
                if let Some(s) = &rl.spread {
                    parts.push(format!("..{}", dump(s)));
                }
                format!("(rec {})", parts.join(" "))
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                format!("({} {} {})", binop_str(*op), dump(lhs), dump(rhs))
            }
            Expr::Unary { op, expr, .. } => {
                let s = match op {
                    UnOp::Neg => "neg",
                    UnOp::Not => "not",
                };
                format!("({s} {})", dump(expr))
            }
            Expr::Field { base, name, .. } => format!("(. {} {name})", dump(base)),
            Expr::Call { callee, args, .. } => {
                let mut parts = vec![format!("call {}", dump(callee))];
                for a in args {
                    parts.push(match &a.name {
                        Some(n) => format!("{n}={}", dump(&a.value)),
                        None => dump(&a.value),
                    });
                }
                format!("({})", parts.join(" "))
            }
            Expr::Index { base, index, .. } => format!("(idx {} {})", dump(base), dump(index)),
            Expr::Try { expr, .. } => format!("(? {})", dump(expr)),
            Expr::Cast { expr, ty, .. } => format!("(as {} {})", dump(expr), ty_name(ty)),
            Expr::IsCheck { expr, pat, .. } => format!("(is {} {})", dump(expr), pat_dump(pat)),
            Expr::Range {
                start,
                end,
                inclusive,
                ..
            } => {
                let op = if *inclusive { "..=" } else { ".." };
                let s = start.as_ref().map_or("_".into(), |x| dump(x));
                let e = end.as_ref().map_or("_".into(), |x| dump(x));
                format!("({op} {s} {e})")
            }
            Expr::If {
                cond,
                then_blk,
                else_,
                ..
            } => {
                let e = match else_ {
                    Some(ElseBranch::ElseIf(e)) => format!(" else {}", dump(e)),
                    Some(ElseBranch::Else(b)) => format!(" else {}", block_dump(b)),
                    None => String::new(),
                };
                format!("(if {} {}{e})", dump(cond), block_dump(then_blk))
            }
            Expr::Match {
                scrutinee, arms, ..
            } => {
                let arms_s: Vec<_> = arms
                    .iter()
                    .map(|a| {
                        let g = a
                            .guard
                            .as_ref()
                            .map_or(String::new(), |g| format!(" if {}", dump(g)));
                        format!("[{}{} -> {}]", pat_dump(&a.pattern), g, dump(&a.body))
                    })
                    .collect();
                format!("(match {} {})", dump(scrutinee), arms_s.join(" "))
            }
            Expr::Block(b, _) => block_dump(b),
            Expr::Lambda { params, body, .. } => {
                let ps: Vec<_> = params
                    .iter()
                    .map(|p| match &p.ty {
                        Some(t) => format!("{}:{}", p.name, ty_name(t)),
                        None => p.name.clone(),
                    })
                    .collect();
                format!("(lam ({}) {})", ps.join(" "), block_dump(body))
            }
            Expr::Spawn { body, .. } => format!("(spawn {})", block_dump(body)),
            Expr::Await { expr, .. } => format!("(await {})", dump(expr)),
            Expr::Raise { expr, .. } => format!("(raise {})", dump(expr)),
            Expr::Return { expr: Some(e), .. } => format!("(ret {})", dump(e)),
            Expr::Return { expr: None, .. } => "(ret)".into(),
            Expr::Break { label, expr, .. } => {
                let l = label.as_ref().map_or(String::new(), |s| format!(" '{s}"));
                let e = expr
                    .as_ref()
                    .map_or(String::new(), |e| format!(" {}", dump(e)));
                format!("(break{l}{e})")
            }
            Expr::Continue { label, .. } => {
                let l = label.as_ref().map_or(String::new(), |s| format!(" '{s}"));
                format!("(continue{l})")
            }
            Expr::Assign {
                op, target, value, ..
            } => {
                let s = match op {
                    AssignOp::Eq => "=",
                    AssignOp::AddEq => "+=",
                    AssignOp::SubEq => "-=",
                    AssignOp::MulEq => "*=",
                    AssignOp::DivEq => "/=",
                    AssignOp::RemEq => "%=",
                };
                format!("({s} {} {})", dump(target), dump(value))
            }
            Expr::CapNarrow { kind, .. } => match kind {
                CapNarrowKind::Subset => "(cap.subset)".into(),
                CapNarrowKind::TestSubset => "(cap.test_subset)".into(),
            },
            Expr::IntentBlock { label, body, .. } => {
                format!("(intent \"{label}\" {})", block_dump(body))
            }
            Expr::ModelRef { name, version, .. } => format!("{name}@v{version}"),
        }
    }

    fn block_dump(b: &super::super::ast::Block) -> String {
        let mut parts = Vec::new();
        for s in &b.stmts {
            parts.push(match s {
                Stmt::Let {
                    name, value, ty, ..
                } => match ty {
                    Some(t) => format!("(let {name}:{} {})", ty_name(t), dump(value)),
                    None => format!("(let {name} {})", dump(value)),
                },
                Stmt::Var { name, value, .. } => format!("(var {name} {})", dump(value)),
                Stmt::For {
                    var, iter, body, ..
                } => format!("(for {var} {} {})", dump(iter), block_dump(body)),
                Stmt::While { cond, body, .. } => {
                    format!("(while {} {})", dump(cond), block_dump(body))
                }
                Stmt::Expr(e) => dump(e),
            });
        }
        if let Some(t) = &b.tail {
            parts.push(format!("=> {}", dump(t)));
        }
        format!("{{{}}}", parts.join(" "))
    }

    fn pat_dump(p: &Pattern) -> String {
        match p {
            Pattern::Wildcard(_) => "_".into(),
            Pattern::Bind(n, _) => n.clone(),
            Pattern::Lit(e, _) => dump(e),
            Pattern::Constructor { name, args, .. } => {
                let a: Vec<_> = args.iter().map(pat_dump).collect();
                format!("{name}({})", a.join(","))
            }
            Pattern::RecordCtor {
                name, fields, rest, ..
            } => {
                let mut parts: Vec<_> = fields
                    .iter()
                    .map(|f| match &f.pat {
                        Some(p) => format!("{}: {}", f.name, pat_dump(p)),
                        None => f.name.clone(),
                    })
                    .collect();
                if *rest {
                    parts.push("..".into());
                }
                format!("{name}{{{}}}", parts.join(", "))
            }
            Pattern::Tuple { elems, .. } => {
                let a: Vec<_> = elems.iter().map(pat_dump).collect();
                format!("({})", a.join(","))
            }
            Pattern::List { elems, .. } => {
                let a: Vec<_> = elems
                    .iter()
                    .map(|e| match e {
                        ListPatElem::Pat(p) => pat_dump(p),
                        ListPatElem::Rest(None) => "..".into(),
                        ListPatElem::Rest(Some(n)) => format!("..{n}"),
                    })
                    .collect();
                format!("[{}]", a.join(","))
            }
        }
    }

    fn binop_str(op: BinOp) -> &'static str {
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

    fn ty_name(t: &super::super::ast::Type) -> String {
        use super::super::ast::Type::*;
        match t {
            Named { name, .. } => name.clone(),
            Generic { name, args, .. } => {
                let a: Vec<_> = args.iter().map(ty_name).collect();
                format!("{name}<{}>", a.join(","))
            }
            Model { name, version, .. } => format!("{name}@v{version}"),
            Tuple { elems, .. } => format!(
                "({})",
                elems.iter().map(ty_name).collect::<Vec<_>>().join(",")
            ),
            Cap { .. } => "cap[..]".into(),
            Fn { params, ret, .. } => format!(
                "fn({}) -> {}",
                params.iter().map(ty_name).collect::<Vec<_>>().join(","),
                ty_name(ret)
            ),
        }
    }

    // ---------- (1–8) literals ----------

    #[test]
    fn lit_int() {
        assert_eq!(dump(&p("42")), "42");
    }

    #[test]
    fn lit_float() {
        assert!(dump(&p("2.5")).starts_with("2.5"));
    }

    #[test]
    fn lit_bool() {
        assert_eq!(dump(&p("true")), "true");
        assert_eq!(dump(&p("false")), "false");
    }

    #[test]
    fn lit_string() {
        assert_eq!(dump(&p(r#""hello""#)), r#""hello""#);
    }

    #[test]
    fn lit_string_interpolation_preserved() {
        // Interpolation segments are kept verbatim per language.md § 2.4.
        let e = p(r#""hi \(name)""#);
        match e {
            Expr::Str(s, _) => assert_eq!(s, "hi \\(name)"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lit_bytes() {
        match p(r#"b"\xff\x00""#) {
            Expr::Bytes(b, _) => assert_eq!(b, vec![0xff, 0x00]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn lit_char_date_duration() {
        assert_eq!(dump(&p("'a'")), "'a'");
        assert_eq!(dump(&p("2026-05-07")), "date:2026-05-07");
        assert_eq!(dump(&p("3s")), "dur:3s");
    }

    #[test]
    fn lit_unit() {
        assert_eq!(dump(&p("()")), "()");
    }

    // ---------- (9–12) idents and field access ----------

    #[test]
    fn ident_simple() {
        assert_eq!(dump(&p("x")), "x");
    }

    #[test]
    fn field_access() {
        assert_eq!(dump(&p("fs.read_file")), "(. fs read_file)");
    }

    #[test]
    fn field_access_chain() {
        assert_eq!(dump(&p("a.b.c.d")), "(. (. (. a b) c) d)");
    }

    // ---------- (13–17) function calls ----------

    #[test]
    fn call_empty() {
        assert_eq!(dump(&p("f()")), "(call f)");
    }

    #[test]
    fn call_positional() {
        assert_eq!(dump(&p("f(1, 2)")), "(call f 1 2)");
    }

    #[test]
    fn call_named_args() {
        assert_eq!(
            dump(&p(r#"new(name: "ada", age: 36)"#)),
            "(call new name=\"ada\" age=36)"
        );
    }

    #[test]
    fn call_method_style() {
        // `xs.map(f)` is field-access-then-call per § 5.4.
        assert_eq!(dump(&p("xs.map(f)")), "(call (. xs map) f)");
    }

    #[test]
    fn call_with_lambda_arg() {
        // Higher-order: `xs.fold(0, fn(acc, x) { acc + x })`.
        let s = dump(&p("xs.fold(0, fn(acc, x) { acc + x })"));
        assert!(s.starts_with("(call (. xs fold) 0 (lam (acc x) "), "{s}");
        assert!(s.contains("(+ acc x)"));
    }

    // ---------- (18–19) try / index ----------

    #[test]
    fn postfix_try() {
        assert_eq!(dump(&p("f()?")), "(? (call f))");
    }

    #[test]
    fn postfix_index() {
        assert_eq!(dump(&p("xs[0]")), "(idx xs 0)");
    }

    // ---------- (20–22) prefix unary ----------

    #[test]
    fn unary_neg() {
        assert_eq!(dump(&p("-x")), "(neg x)");
    }

    #[test]
    fn unary_not() {
        assert_eq!(dump(&p("not flag")), "(not flag)");
    }

    #[test]
    fn unary_paren_around_addsub() {
        assert_eq!(dump(&p("-(a + b)")), "(neg (+ a b))");
    }

    // ---------- (23–27) binary precedence / associativity ----------

    #[test]
    fn bin_addsub_left_assoc() {
        assert_eq!(dump(&p("a + b - c + d")), "(+ (- (+ a b) c) d)");
    }

    #[test]
    fn bin_mul_binds_tighter_than_add() {
        assert_eq!(dump(&p("1 + 2 * 3")), "(+ 1 (* 2 3))");
    }

    #[test]
    fn bin_paren_overrides_precedence() {
        assert_eq!(dump(&p("(1 + 2) * 3")), "(* (+ 1 2) 3)");
    }

    #[test]
    fn bin_div_rem_left_assoc() {
        assert_eq!(dump(&p("a / b % c")), "(% (/ a b) c)");
    }

    #[test]
    fn bin_addsub_vs_unary() {
        // `-a + b` is `(-a) + b`, not `-(a + b)`.
        assert_eq!(dump(&p("-a + b")), "(+ (neg a) b)");
    }

    // ---------- (28–30) comparison / logical ----------

    #[test]
    fn cmp_equality() {
        assert_eq!(dump(&p("a == b")), "(== a b)");
    }

    #[test]
    fn logical_and_binds_tighter_than_or() {
        assert_eq!(dump(&p("a or b and c")), "(or a (and b c))");
    }

    #[test]
    fn cmp_chained_with_and() {
        assert_eq!(dump(&p("x < y and y < z")), "(and (< x y) (< y z))");
    }

    // ---------- (31–32) bitwise / shift ----------

    #[test]
    fn bitops_left_assoc_within_level() {
        assert_eq!(dump(&p("a & b | c ^ d")), "(^ (| (& a b) c) d)");
    }

    #[test]
    fn shift_left_right() {
        assert_eq!(dump(&p("n << 1")), "(<< n 1)");
        assert_eq!(dump(&p("m >> 2")), "(>> m 2)");
    }

    // ---------- (33–34) is / as ----------

    #[test]
    fn is_constructor_pattern() {
        assert_eq!(dump(&p("r is Ok(v)")), "(is r Ok(v))");
    }

    #[test]
    fn as_cast_to_named_type() {
        assert_eq!(dump(&p("n as i64")), "(as n i64)");
    }

    // ---------- (35–36) ranges ----------

    #[test]
    fn range_half_open() {
        assert_eq!(dump(&p("0..10")), "(.. 0 10)");
    }

    #[test]
    fn range_inclusive() {
        assert_eq!(dump(&p("0..=n")), "(..= 0 n)");
    }

    // ---------- (37–40) if / match / block ----------

    #[test]
    fn if_else_expression() {
        assert_eq!(
            dump(&p("if x > 0 { 1 } else { -1 }")),
            "(if (> x 0) {=> 1} else {=> (neg 1)})"
        );
    }

    #[test]
    fn if_else_if_chain() {
        let s = dump(&p("if a { 1 } else if b { 2 } else { 3 }"));
        // Encodes as nested If in the ElseIf branch.
        assert!(s.starts_with("(if a {=> 1} else (if b {=> 2}"), "{s}");
        assert!(s.contains("else {=> 3}"));
    }

    #[test]
    fn match_constructor_arms() {
        // PascalCase identifiers in pattern position parse as unit
        // constructors, so `Pending` becomes `Pending()` in the dump.
        let s = dump(&p("match s { Pending -> 1, Active(t) -> 2, _ -> 0 }"));
        assert!(s.contains("[Pending() -> 1]"), "{s}");
        assert!(s.contains("[Active(t) -> 2]"), "{s}");
        assert!(s.contains("[_ -> 0]"), "{s}");
    }

    #[test]
    fn match_list_pattern_with_rest() {
        let s = dump(&p(
            r#"match xs { [] -> "empty", [x] -> "one", [x, ..rest] -> "many" }"#,
        ));
        assert!(s.contains("[[] ->"), "{s}");
        assert!(s.contains("[[x] ->"), "{s}");
        assert!(s.contains("[[x,..rest] ->"), "{s}");
    }

    #[test]
    fn match_with_guard() {
        let s = dump(&p("match n { x if x > 0 -> 1, _ -> 0 }"));
        assert!(s.contains("[x if (> x 0) -> 1]"), "{s}");
    }

    #[test]
    fn block_with_let_bindings_and_tail() {
        assert_eq!(
            dump(&p("{ let x = 1; let y = 2; x + y }")),
            "{(let x 1) (let y 2) => (+ x y)}"
        );
    }

    // ---------- (41–43) lambda / spawn ----------

    #[test]
    fn lambda_typed() {
        let s = dump(&p("fn(x: int) -> int { x + 1 }"));
        assert!(s.starts_with("(lam (x:int) "), "{s}");
        assert!(s.contains("(+ x 1)"));
    }

    #[test]
    fn lambda_untyped() {
        assert_eq!(dump(&p("fn(x) { x }")), "(lam (x) {=> x})");
    }

    #[test]
    fn spawn_block() {
        let s = dump(&p("spawn { compute(cap) }"));
        assert!(s.starts_with("(spawn "));
        assert!(s.contains("(call compute cap)"));
    }

    // ---------- (44–46) await / raise / return ----------

    #[test]
    fn await_handle() {
        assert_eq!(dump(&p("await h")), "(await h)");
    }

    #[test]
    fn raise_user_err() {
        let s = dump(&p(r#"raise err.user("bad")"#));
        assert_eq!(s, "(raise (call (. err user) \"bad\"))");
    }

    #[test]
    fn return_value() {
        assert_eq!(dump(&p("return x")), "(ret x)");
    }

    // ---------- (47–48) collections ----------

    #[test]
    fn list_literal() {
        assert_eq!(dump(&p("[1, 2, 3]")), "(list 1 2 3)");
    }

    #[test]
    fn tuple_literal() {
        assert_eq!(dump(&p("(1, 2)")), "(tuple 1 2)");
    }

    // ---------- (49–50) record literals ----------

    #[test]
    fn record_anonymous() {
        assert_eq!(dump(&p("{ a: 1, b: 2 }")), "(rec a=1 b=2)");
    }

    #[test]
    fn record_with_type_and_spread() {
        let s = dump(&p("User { ..u, age: 37 }"));
        assert_eq!(s, "(rec :ty User age=37 ..u)");
    }

    // ---------- (51–52) cap.subset / assignment ----------

    #[test]
    fn cap_subset_captured_raw() {
        match p("cap.subset[fs.read_file]") {
            Expr::CapNarrow { kind, .. } => assert_eq!(kind, CapNarrowKind::Subset),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn assign_compound() {
        assert_eq!(dump(&p("x += 1")), "(+= x 1)");
    }

    // ---------- (53–55) postfix integration ----------

    #[test]
    fn postfix_chain_call_field_try() {
        // `parsed.map(canonicalise)?` — call → field → try in correct order.
        assert_eq!(
            dump(&p("parsed.map(canonicalise)?")),
            "(? (call (. parsed map) canonicalise))"
        );
    }

    #[test]
    fn postfix_index_then_method() {
        assert_eq!(dump(&p("xs[0].name")), "(. (idx xs 0) name)");
    }

    #[test]
    fn nested_lambda_in_call() {
        let s = dump(&p("items.fold(0, fn(acc, it) { acc + it.amount })"));
        assert!(s.contains("(. it amount)"), "{s}");
        assert!(s.contains("(lam (acc it) "));
    }

    // ---------- error case ----------

    #[test]
    fn dangling_operator_fails() {
        assert!(parse_expression("1 +").is_err());
    }
}
