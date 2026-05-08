//! Hand-rolled TOML reader covering the shape of `lockset.toml`
//! (M7.T1).
//!
//! Aeris ships as a single static binary (thesis § 2). Pulling in
//! the upstream `toml` crate adds ~1 MB once `serde` and friends
//! land, so we restrict ourselves to the subset the spec uses
//! (§ 24.1):
//!
//! - `[section]` and `[parent.child]` headers
//! - `key = value` assignments at section level
//! - scalars: string `"…"`, integer, boolean
//! - arrays of scalars
//! - inline tables `{ k = v, k = v, … }`
//! - line comments starting with `#`
//!
//! Anything beyond this surface is rejected at parse time with a
//! clear error so that a real-world TOML file doesn't silently
//! mis-parse.

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum TomlValue {
    String(String),
    Int(i64),
    Bool(bool),
    Array(Vec<TomlValue>),
    Table(BTreeMap<String, TomlValue>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TomlError {
    pub message: String,
    pub line: u32,
    pub col: u32,
}

impl std::fmt::Display for TomlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

/// Parse a TOML document into a flat `Table`. Section headers like
/// `[deps]` create a nested table reachable as `root["deps"]`.
pub fn parse(src: &str) -> Result<BTreeMap<String, TomlValue>, TomlError> {
    let mut p = Parser::new(src);
    p.parse_document()
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
    src: &'a str,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            src,
        }
    }

    fn err(&self, msg: impl Into<String>) -> TomlError {
        TomlError {
            message: msg.into(),
            line: self.line,
            col: self.col,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn peek(&self) -> u8 {
        if self.eof() {
            0
        } else {
            self.bytes[self.pos]
        }
    }

    fn bump(&mut self) -> u8 {
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

    fn skip_ws_inline(&mut self) {
        while !self.eof() && matches!(self.peek(), b' ' | b'\t') {
            self.bump();
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            while !self.eof() && matches!(self.peek(), b' ' | b'\t' | b'\n' | b'\r') {
                self.bump();
            }
            if self.peek() == b'#' {
                while !self.eof() && self.peek() != b'\n' {
                    self.bump();
                }
            } else {
                break;
            }
        }
    }

    fn parse_document(&mut self) -> Result<BTreeMap<String, TomlValue>, TomlError> {
        let mut root: BTreeMap<String, TomlValue> = BTreeMap::new();
        let mut current_path: Vec<String> = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.eof() {
                break;
            }
            if self.peek() == b'[' {
                current_path = self.parse_section_header()?;
                ensure_table_path(&mut root, &current_path)?;
                continue;
            }
            // key = value at the active path
            let key = self.parse_key()?;
            self.skip_ws_inline();
            if self.peek() != b'=' {
                return Err(self.err("expected `=`"));
            }
            self.bump();
            self.skip_ws_inline();
            let v = self.parse_value()?;
            insert_at_path(&mut root, &current_path, &key, v)?;
            self.skip_ws_inline();
            if !self.eof() && self.peek() == b'#' {
                while !self.eof() && self.peek() != b'\n' {
                    self.bump();
                }
            }
            // The following newline (or EOF) is consumed by the next
            // skip_ws_and_comments cycle.
        }
        Ok(root)
    }

    fn parse_section_header(&mut self) -> Result<Vec<String>, TomlError> {
        self.bump(); // '['
        self.skip_ws_inline();
        let mut parts = Vec::new();
        loop {
            let key = self.parse_simple_key()?;
            parts.push(key);
            self.skip_ws_inline();
            match self.peek() {
                b'.' => {
                    self.bump();
                    self.skip_ws_inline();
                }
                b']' => {
                    self.bump();
                    break;
                }
                _ => return Err(self.err("expected `.` or `]` in section header")),
            }
        }
        Ok(parts)
    }

    /// Single bare or quoted key segment (no dot continuation). Used
    /// inside section headers and inline-table key positions where
    /// `.` is the section / nested-key separator, not part of the
    /// key itself.
    fn parse_simple_key(&mut self) -> Result<String, TomlError> {
        self.skip_ws_inline();
        if self.peek() == b'"' {
            return self.parse_string_inner();
        }
        let start = self.pos;
        while !self.eof()
            && (self.peek().is_ascii_alphanumeric() || self.peek() == b'_' || self.peek() == b'-')
        {
            self.bump();
        }
        if start == self.pos {
            return Err(self.err("expected key"));
        }
        Ok(self.src[start..self.pos].to_string())
    }

    fn parse_key(&mut self) -> Result<String, TomlError> {
        // A dotted key like `http.allow` parses as a single string
        // here; `insert_at_path` later splits on `.` to walk into
        // nested tables. Plain keys (no dots) work the same way.
        let mut buf = String::new();
        loop {
            self.skip_ws_inline();
            if self.peek() == b'"' {
                buf.push_str(&self.parse_string_inner()?);
            } else {
                let start = self.pos;
                while !self.eof()
                    && (self.peek().is_ascii_alphanumeric()
                        || self.peek() == b'_'
                        || self.peek() == b'-')
                {
                    self.bump();
                }
                if start == self.pos {
                    return Err(self.err("expected key"));
                }
                buf.push_str(&self.src[start..self.pos]);
            }
            self.skip_ws_inline();
            if self.peek() == b'.' {
                self.bump();
                buf.push('.');
                continue;
            }
            return Ok(buf);
        }
    }

    fn parse_value(&mut self) -> Result<TomlValue, TomlError> {
        self.skip_ws_inline();
        match self.peek() {
            b'"' => self.parse_string_inner().map(TomlValue::String),
            b'[' => self.parse_array(),
            b'{' => self.parse_inline_table(),
            b't' | b'f' => self.parse_bool(),
            b'-' | b'0'..=b'9' => self.parse_int(),
            _ => Err(self.err("unexpected start of value")),
        }
    }

    fn parse_string_inner(&mut self) -> Result<String, TomlError> {
        if self.peek() != b'"' {
            return Err(self.err("expected `\"`"));
        }
        self.bump();
        let mut out = String::new();
        loop {
            if self.eof() {
                return Err(self.err("unterminated string"));
            }
            match self.peek() {
                b'"' => {
                    self.bump();
                    return Ok(out);
                }
                b'\\' => {
                    self.bump();
                    match self.bump() {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'n' => out.push('\n'),
                        b't' => out.push('\t'),
                        b'r' => out.push('\r'),
                        c => return Err(self.err(format!("bad escape \\{}", c as char))),
                    }
                }
                b'\n' => return Err(self.err("newline in string")),
                _ => {
                    let start = self.pos;
                    let len = utf8_char_len(self.peek());
                    for _ in 0..len {
                        self.bump();
                    }
                    out.push_str(&self.src[start..self.pos]);
                }
            }
        }
    }

    fn parse_array(&mut self) -> Result<TomlValue, TomlError> {
        self.bump(); // '['
        let mut out = Vec::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == b']' {
                self.bump();
                return Ok(TomlValue::Array(out));
            }
            let v = self.parse_value()?;
            out.push(v);
            self.skip_ws_and_comments();
            match self.peek() {
                b',' => {
                    self.bump();
                }
                b']' => {
                    self.bump();
                    return Ok(TomlValue::Array(out));
                }
                _ => return Err(self.err("expected `,` or `]` in array")),
            }
        }
    }

    fn parse_inline_table(&mut self) -> Result<TomlValue, TomlError> {
        self.bump(); // '{'
        let mut t: BTreeMap<String, TomlValue> = BTreeMap::new();
        loop {
            self.skip_ws_and_comments();
            if self.peek() == b'}' {
                self.bump();
                return Ok(TomlValue::Table(t));
            }
            let key = self.parse_simple_key()?;
            self.skip_ws_inline();
            if self.peek() != b'=' {
                return Err(self.err("expected `=` in inline table"));
            }
            self.bump();
            self.skip_ws_inline();
            let v = self.parse_value()?;
            t.insert(key, v);
            self.skip_ws_and_comments();
            match self.peek() {
                b',' => {
                    self.bump();
                }
                b'}' => {
                    self.bump();
                    return Ok(TomlValue::Table(t));
                }
                _ => return Err(self.err("expected `,` or `}` in inline table")),
            }
        }
    }

    fn parse_bool(&mut self) -> Result<TomlValue, TomlError> {
        if self.src[self.pos..].starts_with("true") {
            for _ in 0..4 {
                self.bump();
            }
            return Ok(TomlValue::Bool(true));
        }
        if self.src[self.pos..].starts_with("false") {
            for _ in 0..5 {
                self.bump();
            }
            return Ok(TomlValue::Bool(false));
        }
        Err(self.err("expected `true` or `false`"))
    }

    fn parse_int(&mut self) -> Result<TomlValue, TomlError> {
        let start = self.pos;
        if self.peek() == b'-' {
            self.bump();
        }
        while !self.eof() && (self.peek().is_ascii_digit() || self.peek() == b'_') {
            self.bump();
        }
        let raw: String = self.src[start..self.pos]
            .chars()
            .filter(|c| *c != '_')
            .collect();
        let n = raw
            .parse::<i64>()
            .map_err(|_| self.err("invalid integer"))?;
        Ok(TomlValue::Int(n))
    }
}

fn utf8_char_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first & 0xE0 == 0xC0 {
        2
    } else if first & 0xF0 == 0xE0 {
        3
    } else if first & 0xF8 == 0xF0 {
        4
    } else {
        1
    }
}

fn ensure_table_path(
    root: &mut BTreeMap<String, TomlValue>,
    path: &[String],
) -> Result<(), TomlError> {
    if path.is_empty() {
        return Ok(());
    }
    let mut cursor: &mut BTreeMap<String, TomlValue> = root;
    for part in path {
        let entry = cursor
            .entry(part.clone())
            .or_insert_with(|| TomlValue::Table(BTreeMap::new()));
        match entry {
            TomlValue::Table(inner) => {
                cursor = inner;
            }
            _ => {
                return Err(TomlError {
                    message: format!("section `{part}` collides with a non-table value"),
                    line: 0,
                    col: 0,
                })
            }
        }
    }
    Ok(())
}

fn insert_at_path(
    root: &mut BTreeMap<String, TomlValue>,
    section: &[String],
    key: &str,
    value: TomlValue,
) -> Result<(), TomlError> {
    let mut cursor: &mut BTreeMap<String, TomlValue> = root;
    for part in section {
        let entry = cursor
            .entry(part.clone())
            .or_insert_with(|| TomlValue::Table(BTreeMap::new()));
        cursor = match entry {
            TomlValue::Table(inner) => inner,
            _ => {
                return Err(TomlError {
                    message: format!("section `{part}` is not a table"),
                    line: 0,
                    col: 0,
                })
            }
        };
    }
    // Support dotted keys like `http.allow` inside a section: split
    // on `.` and walk into nested tables.
    let dotted: Vec<&str> = key.split('.').collect();
    if dotted.len() == 1 {
        cursor.insert(key.to_string(), value);
    } else {
        for part in &dotted[..dotted.len() - 1] {
            let entry = cursor
                .entry(part.to_string())
                .or_insert_with(|| TomlValue::Table(BTreeMap::new()));
            cursor = match entry {
                TomlValue::Table(inner) => inner,
                _ => {
                    return Err(TomlError {
                        message: format!("dotted key `{part}` collides with a value"),
                        line: 0,
                        col: 0,
                    })
                }
            };
        }
        cursor.insert(dotted.last().unwrap().to_string(), value);
    }
    Ok(())
}

// ====================================================================
//  Tests
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_ok(src: &str) -> BTreeMap<String, TomlValue> {
        parse(src).unwrap_or_else(|e| panic!("parse failed: {e}"))
    }

    #[test]
    fn empty_doc() {
        assert!(parse_ok("").is_empty());
    }

    #[test]
    fn comment_only_doc() {
        assert!(parse_ok("# just a comment\n").is_empty());
    }

    #[test]
    fn single_string_at_top_level() {
        let r = parse_ok(r#"name = "aeris""#);
        assert_eq!(r.get("name"), Some(&TomlValue::String("aeris".into())));
    }

    #[test]
    fn integer_value() {
        let r = parse_ok("count = 42");
        assert_eq!(r.get("count"), Some(&TomlValue::Int(42)));
    }

    #[test]
    fn integer_with_underscores() {
        let r = parse_ok("n = 1_000_000");
        assert_eq!(r.get("n"), Some(&TomlValue::Int(1_000_000)));
    }

    #[test]
    fn negative_integer() {
        let r = parse_ok("k = -3");
        assert_eq!(r.get("k"), Some(&TomlValue::Int(-3)));
    }

    #[test]
    fn boolean_true_and_false() {
        let r = parse_ok("a = true\nb = false");
        assert_eq!(r.get("a"), Some(&TomlValue::Bool(true)));
        assert_eq!(r.get("b"), Some(&TomlValue::Bool(false)));
    }

    #[test]
    fn array_of_strings() {
        let r = parse_ok(r#"hosts = ["a", "b", "c"]"#);
        match r.get("hosts").unwrap() {
            TomlValue::Array(xs) => {
                assert_eq!(xs.len(), 3);
                assert_eq!(xs[0], TomlValue::String("a".into()));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn empty_array() {
        let r = parse_ok("xs = []");
        match r.get("xs").unwrap() {
            TomlValue::Array(xs) => assert!(xs.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn section_header() {
        let src = r#"
            [project]
            name = "a"
        "#;
        let r = parse_ok(src);
        match r.get("project").unwrap() {
            TomlValue::Table(t) => {
                assert_eq!(t.get("name"), Some(&TomlValue::String("a".into())));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn nested_section_header() {
        let src = r#"
            [ai.backend]
            kind = "http"
        "#;
        let r = parse_ok(src);
        let ai = match r.get("ai").unwrap() {
            TomlValue::Table(t) => t,
            _ => panic!(),
        };
        let backend = match ai.get("backend").unwrap() {
            TomlValue::Table(t) => t,
            _ => panic!(),
        };
        assert_eq!(backend.get("kind"), Some(&TomlValue::String("http".into())));
    }

    #[test]
    fn dotted_key_within_section() {
        let src = r#"
            [caps]
            http.allow = ["x"]
        "#;
        let r = parse_ok(src);
        let caps = match r.get("caps").unwrap() {
            TomlValue::Table(t) => t,
            _ => panic!(),
        };
        let http = match caps.get("http").unwrap() {
            TomlValue::Table(t) => t,
            _ => panic!(),
        };
        assert!(matches!(http.get("allow"), Some(TomlValue::Array(_))));
    }

    #[test]
    fn inline_table_in_deps() {
        let src = r#"
            [deps]
            utils = { path = "./lib/utils.aer", hash = "blake3:abcd" }
        "#;
        let r = parse_ok(src);
        let deps = match r.get("deps").unwrap() {
            TomlValue::Table(t) => t,
            _ => panic!(),
        };
        let utils = match deps.get("utils").unwrap() {
            TomlValue::Table(t) => t,
            _ => panic!(),
        };
        assert_eq!(
            utils.get("path"),
            Some(&TomlValue::String("./lib/utils.aer".into()))
        );
        assert_eq!(
            utils.get("hash"),
            Some(&TomlValue::String("blake3:abcd".into()))
        );
    }

    #[test]
    fn comments_between_sections() {
        let src = r#"
            # top
            [a]
            x = 1
            # mid
            [b]
            y = 2
        "#;
        let r = parse_ok(src);
        assert!(r.contains_key("a"));
        assert!(r.contains_key("b"));
    }

    #[test]
    fn full_lockset_parses() {
        // The canonical fixture from `language.md` § 24.1.
        let src = r#"
            [project]
            name   = "settle-pipeline"
            aeris  = "0.2.0"

            [deps]
            deploy = { source = "github.com/acmecorp/aeris-devops", version = "1.2.0", hash   = "blake3:7e2c" }
            utils  = { path   = "./lib/utils.aer", hash   = "blake3:9b18" }

            [caps]
            http.allow      = ["api.acme.com", "api.stripe.com"]
            fs.allow_read   = ["/etc/aeris/**", "./data/**"]
            fs.allow_write  = ["./out/**", "./.aeris/**"]
            kube.contexts   = ["prod-eu-1"]
            ai.models       = ["claude-opus-4-7", "claude-haiku-4-5"]

            [ai.backend]
            kind  = "http"
            url   = "https://api.anthropic.com"
            auth  = "env:ANTHROPIC_API_KEY"

            [policies]
            active = ["production_egress", "model_budget"]
        "#;
        let r = parse_ok(src);
        assert!(r.contains_key("project"));
        assert!(r.contains_key("deps"));
        assert!(r.contains_key("caps"));
        assert!(r.contains_key("ai"));
        assert!(r.contains_key("policies"));
    }

    #[test]
    fn malformed_unterminated_string_errors() {
        assert!(parse(r#"name = "abc"#).is_err());
    }

    #[test]
    fn malformed_missing_equals_errors() {
        assert!(parse("name 42").is_err());
    }

    #[test]
    fn malformed_array_missing_bracket_errors() {
        assert!(parse("xs = [1, 2").is_err());
    }

    #[test]
    fn malformed_section_no_close_errors() {
        assert!(parse("[project\n").is_err());
    }
}
