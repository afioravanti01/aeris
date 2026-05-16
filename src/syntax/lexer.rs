//! Aeris lexer.
//!
//! Realises `docs/language.md` § 2 (lexical structure):
//! § 2.3 keywords, § 2.4 literals, § 2.5 comments, § 2.6 operators.
//!
//! The lexer is byte-driven over a UTF-8 input. Identifiers and
//! keywords are ASCII (§ 2.2); string and byte-string contents may
//! contain any UTF-8.

use super::token::{Keyword, Span, Token, TokenKind};

/// Convert a source string into a flat token vector terminated by `Eof`.
///
/// Trivia (comments) is preserved in the stream so the formatter and
/// `aeris doc` can recover them. Whitespace and newlines are skipped.
pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let mut lx = Lexer::new(src);
    let mut out = Vec::new();
    loop {
        let tok = lx.next_token()?;
        let is_eof = matches!(tok.kind, TokenKind::Eof);
        out.push(tok);
        if is_eof {
            return Ok(out);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexErrorKind {
    UnexpectedChar(char),
    UnterminatedString,
    UnterminatedBlockComment,
    UnterminatedChar,
    InvalidEscape(char),
    InvalidCharLiteral,
    InvalidNumber,
    IntegerOverflow,
}

pub struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    /// Produce the next token. Returns `Eof` once the input is exhausted.
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        self.skip_whitespace();

        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        if self.eof() {
            return Ok(self.make_token(TokenKind::Eof, start_pos, start_line, start_col));
        }

        let kind = self.lex_one()?;
        Ok(self.make_token(kind, start_pos, start_line, start_col))
    }

    // ---------- core helpers ----------

    fn make_token(&self, kind: TokenKind, start: usize, line: u32, col: u32) -> Token {
        Token {
            kind,
            span: Span {
                start: start as u32,
                end: self.pos as u32,
                line,
                col,
            },
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> u8 {
        if self.pos < self.bytes.len() {
            self.bytes[self.pos]
        } else {
            0
        }
    }

    fn peek_at(&self, off: usize) -> u8 {
        let i = self.pos + off;
        if i < self.bytes.len() {
            self.bytes[i]
        } else {
            0
        }
    }

    fn advance(&mut self) -> u8 {
        let b = self.peek();
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        self.pos += 1;
        b
    }

    fn span_now(&self, start: usize) -> Span {
        Span {
            start: start as u32,
            end: self.pos as u32,
            line: self.line,
            col: self.col,
        }
    }

    fn err(&self, kind: LexErrorKind, start: usize) -> LexError {
        LexError {
            kind,
            span: self.span_now(start),
        }
    }

    fn skip_whitespace(&mut self) {
        while !self.eof() {
            match self.peek() {
                b' ' | b'\t' | b'\n' | b'\r' => {
                    self.advance();
                }
                _ => break,
            }
        }
    }

    // ---------- single-token dispatch ----------

    fn lex_one(&mut self) -> Result<TokenKind, LexError> {
        let start = self.pos;
        let b = self.peek();

        // Comments first (so `/` followed by `/` or `*` is consumed before `/` arithmetic).
        if b == b'/' {
            match self.peek_at(1) {
                b'/' => return self.lex_line_comment(),
                b'*' => return self.lex_block_comment(start),
                _ => {}
            }
        }

        // Bytes literal `b"..."` — must precede the identifier path because `b`
        // is itself a valid identifier start.
        if b == b'b' && self.peek_at(1) == b'"' {
            return self.lex_bytes(start);
        }

        // Identifiers / keywords / underscore
        if is_ident_start(b) {
            return Ok(self.lex_ident_or_keyword());
        }

        // Numbers, dates, durations, timestamps
        if b.is_ascii_digit() {
            return self.lex_number_like(start);
        }

        // Strings / chars / labels
        match b {
            b'"' => return self.lex_string(start),
            b'\'' => return self.lex_quote(start),
            _ => {}
        }

        // Punctuation and operators
        let kind = match b {
            b'(' => {
                self.advance();
                TokenKind::LParen
            }
            b')' => {
                self.advance();
                TokenKind::RParen
            }
            b'{' => {
                self.advance();
                TokenKind::LBrace
            }
            b'}' => {
                self.advance();
                TokenKind::RBrace
            }
            b'[' => {
                self.advance();
                TokenKind::LBracket
            }
            b']' => {
                self.advance();
                TokenKind::RBracket
            }
            b',' => {
                self.advance();
                TokenKind::Comma
            }
            b';' => {
                self.advance();
                TokenKind::Semicolon
            }
            b'@' => {
                self.advance();
                TokenKind::At
            }
            b'#' => {
                self.advance();
                TokenKind::Hash
            }
            b'?' => {
                self.advance();
                if self.peek() == b'?' {
                    self.advance();
                    TokenKind::QuestionQuestion
                } else {
                    TokenKind::Question
                }
            }
            b'^' => {
                self.advance();
                TokenKind::Caret
            }
            b'~' => {
                // Reserved for future use; not in the language now.
                return Err(self.err(LexErrorKind::UnexpectedChar('~'), start));
            }

            b':' => {
                self.advance();
                if self.peek() == b':' {
                    self.advance();
                    TokenKind::ColonColon
                } else {
                    TokenKind::Colon
                }
            }
            b'.' => {
                self.advance();
                if self.peek() == b'.' {
                    self.advance();
                    if self.peek() == b'=' {
                        self.advance();
                        TokenKind::DotDotEq
                    } else {
                        TokenKind::DotDot
                    }
                } else {
                    TokenKind::Dot
                }
            }
            b'-' => {
                self.advance();
                match self.peek() {
                    b'>' => {
                        self.advance();
                        TokenKind::Arrow
                    }
                    b'=' => {
                        self.advance();
                        TokenKind::MinusEq
                    }
                    _ => TokenKind::Minus,
                }
            }
            b'=' => {
                self.advance();
                match self.peek() {
                    b'=' => {
                        self.advance();
                        TokenKind::EqEq
                    }
                    b'>' => {
                        self.advance();
                        TokenKind::FatArrow
                    }
                    _ => TokenKind::Eq,
                }
            }
            b'!' => {
                self.advance();
                if self.peek() == b'=' {
                    self.advance();
                    TokenKind::BangEq
                } else {
                    return Err(self.err(LexErrorKind::UnexpectedChar('!'), start));
                }
            }
            b'<' => {
                self.advance();
                match self.peek() {
                    b'=' => {
                        self.advance();
                        TokenKind::LtEq
                    }
                    b'<' => {
                        self.advance();
                        TokenKind::LtLt
                    }
                    _ => TokenKind::Lt,
                }
            }
            b'>' => {
                self.advance();
                match self.peek() {
                    b'=' => {
                        self.advance();
                        TokenKind::GtEq
                    }
                    b'>' => {
                        self.advance();
                        TokenKind::GtGt
                    }
                    _ => TokenKind::Gt,
                }
            }
            b'+' => {
                self.advance();
                if self.peek() == b'=' {
                    self.advance();
                    TokenKind::PlusEq
                } else {
                    TokenKind::Plus
                }
            }
            b'*' => {
                self.advance();
                if self.peek() == b'=' {
                    self.advance();
                    TokenKind::StarEq
                } else {
                    TokenKind::Star
                }
            }
            b'/' => {
                self.advance();
                if self.peek() == b'=' {
                    self.advance();
                    TokenKind::SlashEq
                } else {
                    TokenKind::Slash
                }
            }
            b'%' => {
                self.advance();
                if self.peek() == b'=' {
                    self.advance();
                    TokenKind::PercentEq
                } else {
                    TokenKind::Percent
                }
            }
            b'&' => {
                self.advance();
                TokenKind::Amp
            }
            b'|' => {
                self.advance();
                TokenKind::Pipe
            }

            other => {
                let ch = char_at(self.bytes, self.pos);
                self.advance_one_char();
                return Err(LexError {
                    kind: LexErrorKind::UnexpectedChar(ch.unwrap_or(other as char)),
                    span: self.span_now(start),
                });
            }
        };

        Ok(kind)
    }

    fn advance_one_char(&mut self) {
        // Advance past one full UTF-8 char even if multi-byte (used in error path).
        let b = self.peek();
        let len = utf8_char_len(b);
        for _ in 0..len {
            self.advance();
        }
    }

    // ---------- comments ----------

    fn lex_line_comment(&mut self) -> Result<TokenKind, LexError> {
        // already at first '/'; consume '//'
        self.advance();
        self.advance();
        let is_doc = self.peek() == b'/';
        if is_doc {
            self.advance();
        }
        let start = self.pos;
        while !self.eof() && self.peek() != b'\n' {
            self.advance();
        }
        let body = self.src[start..self.pos].trim_start().to_string();
        Ok(if is_doc {
            TokenKind::DocComment(body)
        } else {
            TokenKind::LineComment(body)
        })
    }

    fn lex_block_comment(&mut self, start_for_err: usize) -> Result<TokenKind, LexError> {
        // already at '/*'
        self.advance();
        self.advance();
        let body_start = self.pos;
        let mut depth: u32 = 1;
        while depth > 0 {
            if self.eof() {
                return Err(self.err(LexErrorKind::UnterminatedBlockComment, start_for_err));
            }
            if self.peek() == b'/' && self.peek_at(1) == b'*' {
                self.advance();
                self.advance();
                depth += 1;
            } else if self.peek() == b'*' && self.peek_at(1) == b'/' {
                let body_end = self.pos;
                self.advance();
                self.advance();
                depth -= 1;
                if depth == 0 {
                    let body = self.src[body_start..body_end].to_string();
                    return Ok(TokenKind::BlockComment(body));
                }
            } else {
                self.advance_one_char();
            }
        }
        // Unreachable: loop returns when depth hits 0.
        Err(self.err(LexErrorKind::UnterminatedBlockComment, start_for_err))
    }

    // ---------- identifiers & keywords ----------

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        while is_ident_continue(self.peek()) {
            self.advance();
        }
        let s = &self.src[start..self.pos];
        if let Some(kw) = Keyword::from_ident(s) {
            TokenKind::Keyword(kw)
        } else {
            TokenKind::Ident(s.to_string())
        }
    }

    // ---------- numbers, dates, timestamps, durations ----------

    fn lex_number_like(&mut self, start: usize) -> Result<TokenKind, LexError> {
        // Hex / binary literals — never become dates/durations.
        if self.peek() == b'0' && (self.peek_at(1) == b'x' || self.peek_at(1) == b'X') {
            return self.lex_radix_int(start, 16);
        }
        if self.peek() == b'0' && (self.peek_at(1) == b'b' || self.peek_at(1) == b'B') {
            return self.lex_radix_int(start, 2);
        }

        // Consume the run of digits (with `_` separators).
        let digits_start = self.pos;
        while self.peek().is_ascii_digit() || self.peek() == b'_' {
            self.advance();
        }
        let digits_end = self.pos;
        let digit_count = self.src[digits_start..digits_end]
            .chars()
            .filter(|c| c.is_ascii_digit())
            .count();

        // Date pattern: `\d{4}-\d{2}-\d{2}` — only when the digit run is exactly 4 ASCII digits
        // and the next char is `-` followed by `\d\d-\d\d`.
        if digit_count == 4 && self.peek() == b'-' && self.has_date_tail() {
            return self.finish_date_or_timestamp(start);
        }

        // Float: either `<digits>.<digits>` (followed by digit) or `<digits>e[+-]<digits>`.
        if self.peek() == b'.' && self.peek_at(1).is_ascii_digit() {
            self.advance(); // '.'
            while self.peek().is_ascii_digit() || self.peek() == b'_' {
                self.advance();
            }
            self.maybe_lex_exponent();
            return self.parse_float(start);
        }
        if matches!(self.peek(), b'e' | b'E') && self.peek_after_sign_is_digit(1) {
            self.maybe_lex_exponent();
            return self.parse_float(start);
        }

        // Duration: `<digits><unit>` where unit ∈ { ns, us, ms, s, m, h, d, w }.
        if let Some(unit_len) = self.try_match_duration_unit() {
            let unit_end = self.pos + unit_len;
            // Ensure no further identifier-continue char abuts the unit.
            let next_after = if unit_end < self.bytes.len() {
                self.bytes[unit_end]
            } else {
                0
            };
            if !is_ident_continue(next_after) {
                for _ in 0..unit_len {
                    self.advance();
                }
                let s = self.src[start..self.pos].to_string();
                return Ok(TokenKind::Duration(s));
            }
        }

        // Plain integer.
        let raw = &self.src[start..self.pos];
        let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
        let n = cleaned
            .parse::<i64>()
            .map_err(|_| self.err(LexErrorKind::IntegerOverflow, start))?;
        Ok(TokenKind::Int(n))
    }

    fn lex_radix_int(&mut self, start: usize, radix: u32) -> Result<TokenKind, LexError> {
        // consume '0' and prefix char
        self.advance();
        self.advance();
        let body_start = self.pos;
        while self.peek() == b'_' || (self.peek() as char).is_digit(radix) {
            self.advance();
        }
        let raw = &self.src[body_start..self.pos];
        if raw.is_empty() || raw.chars().all(|c| c == '_') {
            return Err(self.err(LexErrorKind::InvalidNumber, start));
        }
        let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
        let n = i64::from_str_radix(&cleaned, radix)
            .map_err(|_| self.err(LexErrorKind::IntegerOverflow, start))?;
        Ok(TokenKind::Int(n))
    }

    fn maybe_lex_exponent(&mut self) {
        if matches!(self.peek(), b'e' | b'E') {
            self.advance();
            if matches!(self.peek(), b'+' | b'-') {
                self.advance();
            }
            while self.peek().is_ascii_digit() || self.peek() == b'_' {
                self.advance();
            }
        }
    }

    fn peek_after_sign_is_digit(&self, off: usize) -> bool {
        let b = self.peek_at(off);
        if matches!(b, b'+' | b'-') {
            self.peek_at(off + 1).is_ascii_digit()
        } else {
            b.is_ascii_digit()
        }
    }

    fn parse_float(&mut self, start: usize) -> Result<TokenKind, LexError> {
        let raw = &self.src[start..self.pos];
        let cleaned: String = raw.chars().filter(|c| *c != '_').collect();
        let f = cleaned
            .parse::<f64>()
            .map_err(|_| self.err(LexErrorKind::InvalidNumber, start))?;
        Ok(TokenKind::Float(f))
    }

    /// Returns Some(len) where `len` is the byte length of a duration unit
    /// matched at the current position, or None if no valid unit is present.
    fn try_match_duration_unit(&self) -> Option<usize> {
        let rest = &self.bytes[self.pos..];
        // 2-char first: ns, us, ms
        if rest.len() >= 2 {
            let two = &rest[..2];
            if two == b"ns" || two == b"us" || two == b"ms" {
                return Some(2);
            }
        }
        // 1-char: s, m, h, d, w
        if !rest.is_empty() && matches!(rest[0], b's' | b'm' | b'h' | b'd' | b'w') {
            return Some(1);
        }
        None
    }

    /// Whether the bytes after the current `-` form `\d\d-\d\d`.
    fn has_date_tail(&self) -> bool {
        // current position points to '-'
        let b = self.bytes;
        let i = self.pos;
        b.len() >= i + 6
            && b[i] == b'-'
            && b[i + 1].is_ascii_digit()
            && b[i + 2].is_ascii_digit()
            && b[i + 3] == b'-'
            && b[i + 4].is_ascii_digit()
            && b[i + 5].is_ascii_digit()
    }

    fn finish_date_or_timestamp(&mut self, start: usize) -> Result<TokenKind, LexError> {
        // We've consumed 4 digits; consume `-\d\d-\d\d`.
        for _ in 0..6 {
            self.advance();
        }
        // Optional timestamp tail: `T\d\d:\d\d:\d\d(.\d+)?(Z|[+-]\d\d:\d\d)`
        if self.peek() == b'T' && self.is_time_after_t() {
            self.advance(); // T
                            // hh:mm:ss
            for _ in 0..2 {
                self.advance();
            }
            if self.peek() != b':' {
                return Err(self.err(LexErrorKind::InvalidNumber, start));
            }
            self.advance();
            for _ in 0..2 {
                self.advance();
            }
            if self.peek() != b':' {
                return Err(self.err(LexErrorKind::InvalidNumber, start));
            }
            self.advance();
            for _ in 0..2 {
                self.advance();
            }
            // optional fractional seconds
            if self.peek() == b'.' {
                self.advance();
                while self.peek().is_ascii_digit() {
                    self.advance();
                }
            }
            // zone designator
            match self.peek() {
                b'Z' => {
                    self.advance();
                }
                b'+' | b'-' => {
                    self.advance();
                    for _ in 0..2 {
                        self.advance();
                    }
                    if self.peek() == b':' {
                        self.advance();
                        for _ in 0..2 {
                            self.advance();
                        }
                    }
                }
                _ => {
                    return Err(self.err(LexErrorKind::InvalidNumber, start));
                }
            }
            let s = self.src[start..self.pos].to_string();
            return Ok(TokenKind::Timestamp(s));
        }
        let s = self.src[start..self.pos].to_string();
        Ok(TokenKind::Date(s))
    }

    fn is_time_after_t(&self) -> bool {
        // Need at least: T hh : mm : ss → 9 bytes including T
        let b = self.bytes;
        let i = self.pos;
        b.len() >= i + 9
            && b[i] == b'T'
            && b[i + 1].is_ascii_digit()
            && b[i + 2].is_ascii_digit()
            && b[i + 3] == b':'
            && b[i + 4].is_ascii_digit()
            && b[i + 5].is_ascii_digit()
            && b[i + 6] == b':'
            && b[i + 7].is_ascii_digit()
            && b[i + 8].is_ascii_digit()
    }

    // ---------- strings / bytes / chars / labels ----------

    fn lex_string(&mut self, start: usize) -> Result<TokenKind, LexError> {
        self.advance(); // opening "
        // We start in plain-text mode and switch to a `StrInterp`
        // token the first time we see an unescaped `{`. Until then
        // we accumulate into `text` and return the simpler `Str`
        // variant on close.
        let mut text = String::new();
        let mut segments: Vec<crate::syntax::token::StrSegment> = Vec::new();
        let mut has_interp = false;
        loop {
            if self.eof() {
                return Err(self.err(LexErrorKind::UnterminatedString, start));
            }
            match self.peek() {
                b'"' => {
                    self.advance();
                    if has_interp {
                        if !text.is_empty() {
                            segments.push(crate::syntax::token::StrSegment::Text(text));
                        }
                        return Ok(TokenKind::StrInterp(segments));
                    }
                    return Ok(TokenKind::Str(text));
                }
                b'\\' => {
                    self.advance();
                    let esc = self.peek();
                    match esc {
                        b'n' => {
                            text.push('\n');
                            self.advance();
                        }
                        b't' => {
                            text.push('\t');
                            self.advance();
                        }
                        b'r' => {
                            text.push('\r');
                            self.advance();
                        }
                        b'\\' => {
                            text.push('\\');
                            self.advance();
                        }
                        b'"' => {
                            text.push('"');
                            self.advance();
                        }
                        b'\'' => {
                            text.push('\'');
                            self.advance();
                        }
                        b'0' => {
                            text.push('\0');
                            self.advance();
                        }
                        // M16 — `\{` and `\}` are the only way to embed
                        // literal curly braces in a string. There is no
                        // `{{`/`}}` doubling rule.
                        b'{' => {
                            text.push('{');
                            self.advance();
                        }
                        b'}' => {
                            text.push('}');
                            self.advance();
                        }
                        other => {
                            return Err(self.err(LexErrorKind::InvalidEscape(other as char), start));
                        }
                    }
                }
                b'{' => {
                    // M16 — open interpolation. Capture the raw source
                    // between `{` and the matching `}` so the parser can
                    // re-lex it as an expression. We track depth so
                    // record literals and blocks nest correctly.
                    let interp_open = self.pos;
                    self.advance();
                    let inner_start = self.pos;
                    let mut depth: u32 = 1;
                    while depth > 0 {
                        if self.eof() {
                            return Err(self.err(LexErrorKind::UnterminatedString, start));
                        }
                        let c = self.peek();
                        if c == b'"' {
                            // A nested string would need its own escape
                            // logic; v0.3 disallows that for now to keep
                            // the lexer simple. The migrator (M16.T4)
                            // never produces nested strings.
                            return Err(self.err(LexErrorKind::UnterminatedString, start));
                        }
                        if c == b'{' {
                            depth += 1;
                        } else if c == b'}' {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        let len = utf8_char_len(c);
                        for _ in 0..len {
                            self.advance();
                        }
                        continue;
                    }
                    let raw = self.src[inner_start..self.pos].to_string();
                    if raw.trim().is_empty() {
                        return Err(self.err(LexErrorKind::InvalidEscape('{'), interp_open));
                    }
                    self.advance(); // closing `}`
                    if !text.is_empty() {
                        segments.push(crate::syntax::token::StrSegment::Text(std::mem::take(
                            &mut text,
                        )));
                    }
                    segments.push(crate::syntax::token::StrSegment::Interp {
                        source: raw,
                        offset: inner_start,
                    });
                    has_interp = true;
                }
                b'}' => {
                    // A bare `}` outside an interpolation is a typo —
                    // require the explicit `\}` escape (M16 § 11.1).
                    return Err(self.err(LexErrorKind::InvalidEscape('}'), self.pos));
                }
                b => {
                    // Multi-byte UTF-8 chars push themselves verbatim.
                    let len = utf8_char_len(b);
                    let ch_start = self.pos;
                    for _ in 0..len {
                        self.advance();
                    }
                    text.push_str(&self.src[ch_start..self.pos]);
                }
            }
        }
    }

    fn lex_bytes(&mut self, start: usize) -> Result<TokenKind, LexError> {
        self.advance(); // 'b'
        self.advance(); // '"'
        let mut out: Vec<u8> = Vec::new();
        loop {
            if self.eof() {
                return Err(self.err(LexErrorKind::UnterminatedString, start));
            }
            match self.peek() {
                b'"' => {
                    self.advance();
                    return Ok(TokenKind::Bytes(out));
                }
                b'\\' => {
                    self.advance();
                    match self.peek() {
                        b'n' => {
                            out.push(b'\n');
                            self.advance();
                        }
                        b't' => {
                            out.push(b'\t');
                            self.advance();
                        }
                        b'r' => {
                            out.push(b'\r');
                            self.advance();
                        }
                        b'\\' => {
                            out.push(b'\\');
                            self.advance();
                        }
                        b'"' => {
                            out.push(b'"');
                            self.advance();
                        }
                        b'0' => {
                            out.push(0);
                            self.advance();
                        }
                        b'x' => {
                            self.advance();
                            let h = self.lex_hex_byte(start)?;
                            out.push(h);
                        }
                        other => {
                            return Err(self.err(LexErrorKind::InvalidEscape(other as char), start));
                        }
                    }
                }
                b => {
                    out.push(b);
                    self.advance();
                }
            }
        }
    }

    fn lex_hex_byte(&mut self, start: usize) -> Result<u8, LexError> {
        let h1 = hex_digit_value(self.peek())
            .ok_or_else(|| self.err(LexErrorKind::InvalidEscape('x'), start))?;
        self.advance();
        let h2 = hex_digit_value(self.peek())
            .ok_or_else(|| self.err(LexErrorKind::InvalidEscape('x'), start))?;
        self.advance();
        Ok((h1 << 4) | h2)
    }

    /// Lex either a char literal (`'a'`, `'\n'`) or a label (`'name`).
    fn lex_quote(&mut self, start: usize) -> Result<TokenKind, LexError> {
        self.advance(); // '

        // Distinguish char vs label. A char literal has form '<c>' or '\<esc>' (terminated by ').
        // A label has form '<ident> with no closing '.
        if self.peek() == b'\\' {
            return self.lex_char_after_quote(start);
        }
        // If the second char ahead is a closing quote, it's a single-char literal.
        if self.peek_at(1) == b'\'' {
            let c = char_at(self.bytes, self.pos)
                .ok_or_else(|| self.err(LexErrorKind::InvalidCharLiteral, start))?;
            self.advance_one_char();
            self.advance(); // closing '
            return Ok(TokenKind::Char(c));
        }
        // Otherwise, label: '<ident-continue+>
        let label_start = self.pos;
        if !is_ident_start(self.peek()) {
            return Err(self.err(LexErrorKind::InvalidCharLiteral, start));
        }
        while is_ident_continue(self.peek()) {
            self.advance();
        }
        let s = self.src[label_start..self.pos].to_string();
        Ok(TokenKind::Label(s))
    }

    fn lex_char_after_quote(&mut self, start: usize) -> Result<TokenKind, LexError> {
        self.advance(); // '\\'
        let c = match self.peek() {
            b'n' => {
                self.advance();
                '\n'
            }
            b't' => {
                self.advance();
                '\t'
            }
            b'r' => {
                self.advance();
                '\r'
            }
            b'\\' => {
                self.advance();
                '\\'
            }
            b'\'' => {
                self.advance();
                '\''
            }
            b'"' => {
                self.advance();
                '"'
            }
            b'0' => {
                self.advance();
                '\0'
            }
            other => {
                return Err(self.err(LexErrorKind::InvalidEscape(other as char), start));
            }
        };
        if self.peek() != b'\'' {
            return Err(self.err(LexErrorKind::UnterminatedChar, start));
        }
        self.advance();
        Ok(TokenKind::Char(c))
    }
}

// ---------- byte / char helpers ----------

fn is_ident_start(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphabetic()
}

fn is_ident_continue(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

fn hex_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn utf8_char_len(first_byte: u8) -> usize {
    if first_byte < 0x80 {
        1
    } else if first_byte & 0xE0 == 0xC0 {
        2
    } else if first_byte & 0xF0 == 0xE0 {
        3
    } else if first_byte & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}

fn char_at(bytes: &[u8], pos: usize) -> Option<char> {
    if pos >= bytes.len() {
        return None;
    }
    let len = utf8_char_len(bytes[pos]);
    let end = pos + len;
    if end > bytes.len() {
        return None;
    }
    std::str::from_utf8(&bytes[pos..end])
        .ok()
        .and_then(|s| s.chars().next())
}

// ====================================================================
// Tests
// ====================================================================

#[cfg(test)]
mod tests {
    use super::super::token::{Keyword, TokenKind, ALL_KEYWORDS};
    use super::*;

    /// Strip trivia (line / block / doc comments) and the trailing `Eof`.
    fn kinds_of(src: &str) -> Vec<TokenKind> {
        let toks = tokenize(src).expect("tokenize ok");
        let mut out: Vec<TokenKind> = toks
            .into_iter()
            .map(|t| t.kind)
            .filter(|k| {
                !matches!(
                    k,
                    TokenKind::LineComment(_)
                        | TokenKind::BlockComment(_)
                        | TokenKind::DocComment(_)
                )
            })
            .collect();
        // Drop the trailing Eof so test expectations stay tight.
        if matches!(out.last(), Some(TokenKind::Eof)) {
            out.pop();
        }
        out
    }

    // ---------- M1.T1 — keywords ----------

    #[test]
    fn every_keyword_lexes_as_keyword() {
        for kw in ALL_KEYWORDS {
            let src = kw.as_str();
            let toks = kinds_of(src);
            assert_eq!(
                toks,
                vec![TokenKind::Keyword(*kw)],
                "keyword {} did not tokenize as Keyword({:?})",
                kw.as_str(),
                kw
            );
        }
    }

    #[test]
    fn keyword_in_position_is_keyword_not_ident() {
        // A user variable named `step` must lex as a keyword, not an identifier.
        let toks = kinds_of("step");
        assert_eq!(toks, vec![TokenKind::Keyword(Keyword::Step)]);

        let toks = kinds_of("let saga = 1");
        assert_eq!(
            toks,
            vec![
                TokenKind::Keyword(Keyword::Let),
                TokenKind::Keyword(Keyword::Saga),
                TokenKind::Eq,
                TokenKind::Int(1),
            ]
        );
    }

    #[test]
    fn near_keyword_idents_are_idents() {
        // `sagas`, `step_count`, `_saga`, etc. are plain identifiers.
        for s in ["sagas", "step_count", "_saga", "saga_", "Saga"] {
            let toks = kinds_of(s);
            assert_eq!(toks, vec![TokenKind::Ident(s.to_string())], "ident {s}");
        }
    }

    // ---------- M1.T2 — literals ----------

    #[test]
    fn integer_literals() {
        assert_eq!(kinds_of("0"), vec![TokenKind::Int(0)]);
        assert_eq!(kinds_of("42"), vec![TokenKind::Int(42)]);
        assert_eq!(kinds_of("42_000"), vec![TokenKind::Int(42_000)]);
        assert_eq!(kinds_of("0xff"), vec![TokenKind::Int(0xff)]);
        assert_eq!(kinds_of("0xFF_00"), vec![TokenKind::Int(0xff00)]);
        assert_eq!(kinds_of("0b1010"), vec![TokenKind::Int(0b1010)]);
        assert_eq!(kinds_of("0b1010_0001"), vec![TokenKind::Int(0b1010_0001)]);
    }

    #[test]
    fn float_literals() {
        match &kinds_of("2.5")[0] {
            TokenKind::Float(f) => assert!((f - 2.5).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
        match &kinds_of("1.5e-3")[0] {
            TokenKind::Float(f) => assert!((f - 1.5e-3).abs() < 1e-12),
            other => panic!("expected float, got {other:?}"),
        }
        match &kinds_of("1_000.000_5")[0] {
            TokenKind::Float(f) => assert!((f - 1000.0005).abs() < 1e-9),
            other => panic!("expected float, got {other:?}"),
        }
    }

    #[test]
    fn bool_literals_are_keywords() {
        assert_eq!(kinds_of("true"), vec![TokenKind::Keyword(Keyword::True)]);
        assert_eq!(kinds_of("false"), vec![TokenKind::Keyword(Keyword::False)]);
    }

    #[test]
    fn string_literal_simple() {
        assert_eq!(
            kinds_of("\"hello\""),
            vec![TokenKind::Str("hello".to_string())]
        );
    }

    #[test]
    fn string_literal_escapes() {
        assert_eq!(
            kinds_of(r#""a\nb\tc\\d\"e""#),
            vec![TokenKind::Str("a\nb\tc\\d\"e".to_string())]
        );
    }

    #[test]
    fn m16_string_interpolation_single_var_produces_two_segments() {
        use crate::syntax::token::StrSegment;
        let toks = kinds_of(r#""hi {name}""#);
        match &toks[0] {
            TokenKind::StrInterp(segs) => {
                assert_eq!(segs.len(), 2);
                assert!(matches!(&segs[0], StrSegment::Text(t) if t == "hi "));
                match &segs[1] {
                    StrSegment::Interp { source, .. } => assert_eq!(source, "name"),
                    _ => panic!("expected Interp, got {:?}", segs[1]),
                }
            }
            other => panic!("expected StrInterp, got {other:?}"),
        }
    }

    #[test]
    fn m16_string_interpolation_balances_nested_braces() {
        use crate::syntax::token::StrSegment;
        // `{ a: 1 }` inside a string is a record literal expression;
        // the lexer captures the whole thing as a single interp body.
        let toks = kinds_of(r#""x = {{ a: 1 }}""#);
        // outer braces here are LITERAL because `\{`/`\}` escapes are
        // the only way to embed `{`/`}`. So this raw input means: open
        // interpolation, body `{ a: 1 }`, close interpolation.
        match &toks[0] {
            TokenKind::StrInterp(segs) => {
                assert_eq!(segs.len(), 2);
                match &segs[1] {
                    StrSegment::Interp { source, .. } => {
                        assert_eq!(source.trim(), "{ a: 1 }");
                    }
                    _ => panic!("expected Interp, got {:?}", segs[1]),
                }
            }
            other => panic!("expected StrInterp, got {other:?}"),
        }
    }

    #[test]
    fn m16_string_literal_escape_for_braces() {
        let toks = kinds_of(r#""body \{\}""#);
        assert_eq!(toks, vec![TokenKind::Str("body {}".to_string())]);
    }

    #[test]
    fn m16_string_without_braces_is_plain_str() {
        // No `{` ⇒ TokenKind::Str, not StrInterp.
        assert_eq!(
            kinds_of(r#""no interp here""#),
            vec![TokenKind::Str("no interp here".to_string())]
        );
    }

    #[test]
    fn m16_empty_interpolation_is_lex_error() {
        let res = super::tokenize(r#""bad {} here""#);
        assert!(res.is_err(), "expected error on `{{}}`");
    }

    #[test]
    fn m16_bare_close_brace_is_lex_error() {
        let res = super::tokenize(r#""oops }""#);
        assert!(res.is_err(), "expected error on bare `}}` outside interp");
    }

    #[test]
    fn m16_legacy_backslash_paren_now_lex_error() {
        // `\(...)` was an escape in v0.2.0; M16 removes it.
        let res = super::tokenize(r#""x = \(name)""#);
        assert!(res.is_err(), "legacy `\\(...)` must now be rejected");
    }

    #[test]
    fn bytes_literal() {
        assert_eq!(
            kinds_of(r#"b"raw""#),
            vec![TokenKind::Bytes(b"raw".to_vec())]
        );
        assert_eq!(
            kinds_of(r#"b"\xff\x00""#),
            vec![TokenKind::Bytes(vec![0xff, 0x00])]
        );
    }

    #[test]
    fn char_literal() {
        assert_eq!(kinds_of("'a'"), vec![TokenKind::Char('a')]);
        assert_eq!(kinds_of("'\\n'"), vec![TokenKind::Char('\n')]);
        assert_eq!(kinds_of("'\\''"), vec![TokenKind::Char('\'')]);
    }

    #[test]
    fn label_vs_char_disambiguation() {
        let toks = kinds_of("'outer");
        assert_eq!(toks, vec![TokenKind::Label("outer".to_string())]);
    }

    // ---------- M1.T3 — date / timestamp / duration ----------

    #[test]
    fn date_literal_is_one_token_not_subtraction() {
        let toks = kinds_of("2026-05-07");
        assert_eq!(toks, vec![TokenKind::Date("2026-05-07".to_string())]);
        // Crucially, NOT three Ints around two Minus tokens.
        for t in &toks {
            assert!(!matches!(t, TokenKind::Minus));
        }
    }

    #[test]
    fn integer_subtraction_with_spaces_still_works() {
        let toks = kinds_of("2026 - 5 - 7");
        assert_eq!(
            toks,
            vec![
                TokenKind::Int(2026),
                TokenKind::Minus,
                TokenKind::Int(5),
                TokenKind::Minus,
                TokenKind::Int(7),
            ]
        );
    }

    #[test]
    fn integer_subtraction_no_spaces_is_subtraction_when_pattern_does_not_fit() {
        // `2026-5-7` does NOT match `\d{4}-\d{2}-\d{2}` (single-digit components).
        // It must lex as 2026, -, 5, -, 7.
        let toks = kinds_of("2026-5-7");
        assert_eq!(
            toks,
            vec![
                TokenKind::Int(2026),
                TokenKind::Minus,
                TokenKind::Int(5),
                TokenKind::Minus,
                TokenKind::Int(7),
            ]
        );
    }

    #[test]
    fn timestamp_literal() {
        assert_eq!(
            kinds_of("2026-05-07T08:30:00Z"),
            vec![TokenKind::Timestamp("2026-05-07T08:30:00Z".to_string())]
        );
        assert_eq!(
            kinds_of("2026-05-07T08:30:00.123Z"),
            vec![TokenKind::Timestamp("2026-05-07T08:30:00.123Z".to_string())]
        );
        assert_eq!(
            kinds_of("2026-05-07T08:30:00+02:00"),
            vec![TokenKind::Timestamp(
                "2026-05-07T08:30:00+02:00".to_string()
            )]
        );
    }

    #[test]
    fn duration_units() {
        for src in ["3s", "500ms", "2h", "7d", "1ns", "1us", "5m", "2w"] {
            let toks = kinds_of(src);
            assert_eq!(
                toks,
                vec![TokenKind::Duration(src.to_string())],
                "duration {src}"
            );
        }
    }

    #[test]
    fn duration_does_not_eat_following_identifier() {
        // `5min` is NOT a duration of 5 minutes followed by `in`; `min` is one
        // identifier that follows the int 5 (since `m` would be a unit but the
        // next char is ident-continue).
        let toks = kinds_of("5min");
        assert_eq!(
            toks,
            vec![TokenKind::Int(5), TokenKind::Ident("min".to_string())]
        );
    }

    // ---------- M1.T4 — comments ----------

    #[test]
    fn line_comment_body_is_retained() {
        let toks = tokenize("// hello\n42").unwrap();
        let line_comment = toks.iter().find_map(|t| match &t.kind {
            TokenKind::LineComment(s) => Some(s.clone()),
            _ => None,
        });
        assert_eq!(line_comment, Some("hello".to_string()));
    }

    #[test]
    fn doc_comment_is_distinct_from_line_comment() {
        let toks = tokenize("/// docstring\nfn f() {}").unwrap();
        let kinds: Vec<&TokenKind> = toks.iter().map(|t| &t.kind).collect();
        assert!(matches!(kinds[0], TokenKind::DocComment(s) if s == "docstring"));
    }

    #[test]
    fn block_comment_nesting() {
        let src = "/* outer /* inner */ outer */ x";
        let toks = tokenize(src).unwrap();
        // Expect: BlockComment, Ident("x"), Eof
        let kinds: Vec<&TokenKind> = toks.iter().map(|t| &t.kind).collect();
        match kinds[0] {
            TokenKind::BlockComment(body) => {
                assert!(body.contains("outer /* inner */ outer"));
            }
            other => panic!("expected BlockComment, got {other:?}"),
        }
        assert_eq!(kinds[1], &TokenKind::Ident("x".to_string()));
    }

    #[test]
    fn block_comment_unterminated_errors() {
        let err = tokenize("/* never closed").unwrap_err();
        assert_eq!(err.kind, LexErrorKind::UnterminatedBlockComment);
    }

    // ---------- punctuation & operators (smoke) ----------

    #[test]
    fn operators_smoke() {
        let toks = kinds_of("a + b - c * d / e % f");
        assert_eq!(
            toks,
            vec![
                TokenKind::Ident("a".to_string()),
                TokenKind::Plus,
                TokenKind::Ident("b".to_string()),
                TokenKind::Minus,
                TokenKind::Ident("c".to_string()),
                TokenKind::Star,
                TokenKind::Ident("d".to_string()),
                TokenKind::Slash,
                TokenKind::Ident("e".to_string()),
                TokenKind::Percent,
                TokenKind::Ident("f".to_string()),
            ]
        );
    }

    #[test]
    fn arrow_and_fat_arrow() {
        assert_eq!(kinds_of("->"), vec![TokenKind::Arrow]);
        assert_eq!(kinds_of("=>"), vec![TokenKind::FatArrow]);
        assert_eq!(kinds_of("=="), vec![TokenKind::EqEq]);
        assert_eq!(kinds_of("!="), vec![TokenKind::BangEq]);
        assert_eq!(kinds_of("<="), vec![TokenKind::LtEq]);
        assert_eq!(kinds_of(">="), vec![TokenKind::GtEq]);
        assert_eq!(kinds_of(".."), vec![TokenKind::DotDot]);
        assert_eq!(kinds_of("..="), vec![TokenKind::DotDotEq]);
    }

    #[test]
    fn cap_tree_path_lexes_naturally() {
        // The capability narrowing syntax: `cap[fs.read_file @ "x"]`
        let toks = kinds_of(r#"cap[fs.read_file @ "x"]"#);
        assert_eq!(
            toks,
            vec![
                TokenKind::Keyword(Keyword::Cap),
                TokenKind::LBracket,
                TokenKind::Ident("fs".to_string()),
                TokenKind::Dot,
                TokenKind::Ident("read_file".to_string()),
                TokenKind::At,
                TokenKind::Str("x".to_string()),
                TokenKind::RBracket,
            ]
        );
    }

    // ---------- end-to-end: a saga signature ----------

    #[test]
    fn saga_signature_tokenizes() {
        let src = r#"saga settle(batch: list<Invoice@v1>, cap: cap[http.post @ ["api.acme.com"]])"#;
        let toks = kinds_of(src);
        // Just check it parses and the first token is `saga`.
        assert_eq!(toks.first(), Some(&TokenKind::Keyword(Keyword::Saga)));
        assert!(toks.contains(&TokenKind::Keyword(Keyword::Cap)));
        assert!(toks.contains(&TokenKind::At));
        assert!(toks.contains(&TokenKind::Str("api.acme.com".to_string())));
    }
}
