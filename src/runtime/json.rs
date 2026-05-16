//! Self-tagging JSON encoder/decoder for `Value` (M3.T1).
//!
//! Each value is wrapped in a `{"<tag>": <payload>}` object, keeping
//! the format unambiguous regardless of which Aeris type produced it.
//! This is **not** the user-facing `json.encode` / `json.decode<T>`
//! pair from `language.md` § 22 — those will use natural JSON guided
//! by a target type. The format here exists for runtime debugging
//! and round-trip tests; it never crosses an external trust
//! boundary.
//!
//! Tag table:
//!
//! | Variant       | Encoding                                                           |
//! |---------------|--------------------------------------------------------------------|
//! | `Unit`        | `{"unit":null}`                                                    |
//! | `Bool(b)`     | `{"bool":true}`                                                    |
//! | `Int(n)`      | `{"int":42}`                                                       |
//! | `Float(f)`    | `{"float":3.14}`                                                   |
//! | `Decimal(s)`  | `{"decimal":"12.50"}`                                              |
//! | `Str(s)`      | `{"str":"hello"}`                                                  |
//! | `Bytes(b)`    | `{"bytes":"ff00"}` — lower-case hex                                |
//! | `Char(c)`     | `{"char":"a"}`                                                     |
//! | `Uuid(u)`     | `{"uuid":"…"}`                                                     |
//! | `Date(d)`     | `{"date":"2026-05-07"}`                                            |
//! | `Timestamp(t)`| `{"ts":"…"}`                                                       |
//! | `Duration(d)` | `{"dur":"3s"}`                                                     |
//! | `List(vs)`    | `{"list":[v,…]}`                                                   |
//! | `Set(vs)`     | `{"set":[v,…]}`                                                    |
//! | `Map(kvs)`    | `{"map":[[k,v],…]}`                                                |
//! | `Tuple(vs)`   | `{"tuple":[v,…]}`                                                  |
//! | `Option None` | `{"none":null}`                                                    |
//! | `Option Some` | `{"some":v}`                                                       |
//! | `Result Ok`   | `{"ok":v}`                                                         |
//! | `Result Err`  | `{"err":v}`                                                        |
//! | `Record`      | `{"rec":{"name":"User"|null,"fields":[["k",v],…]}}`                |
//! | `Enum`        | `{"enum":{"name":"Status","variant":"Active","data":<variant>}}`   |

use super::value::{EnumValue, RecordValue, Value, VariantValue};

// ====================================================================
//  Encoder
// ====================================================================

/// Encode a `Value` to a self-tagging JSON string. Pure — no IO, no
/// allocations beyond the result string.
pub fn encode(v: &Value) -> String {
    let mut out = String::new();
    write_value(v, &mut out);
    out
}

/// Encode a `Value` to *natural* JSON: the user-facing `json.encode`
/// from `language.md` § 22. Records become objects, lists become
/// arrays, primitives are bare numbers/strings/booleans. The format
/// is lossy for variants that JSON cannot express (timestamps and
/// dates become strings; bytes become hex strings; tuples become
/// arrays) — round-tripping through `json.parse` is best-effort.
pub fn encode_natural(v: &Value) -> String {
    let mut out = String::new();
    write_natural(v, &mut out);
    out
}

fn write_natural(v: &Value, out: &mut String) {
    match v {
        Value::Unit => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(n) => out.push_str(&n.to_string()),
        Value::Float(f) => write_float(*f, out),
        Value::Decimal(s) => out.push_str(s),
        Value::Str(s) => write_natural_str(s, out),
        Value::Bytes(b) => {
            out.push('"');
            for byte in b {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push('"');
        }
        Value::Char(c) => write_natural_str(&c.to_string(), out),
        Value::Uuid(s) | Value::Date(s) | Value::Timestamp(s) | Value::Duration(s) => {
            write_natural_str(s, out);
        }
        Value::List(vs) | Value::Set(vs) | Value::Tuple(vs) => {
            out.push('[');
            for (i, v) in vs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_natural(v, out);
            }
            out.push(']');
        }
        Value::Map(kvs) => {
            out.push('{');
            for (i, (k, v)) in kvs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let key = match k {
                    Value::Str(s) => s.clone(),
                    other => format!("{other:?}"),
                };
                write_natural_str(&key, out);
                out.push(':');
                write_natural(v, out);
            }
            out.push('}');
        }
        Value::Option(None) => out.push_str("null"),
        Value::Option(Some(inner)) => write_natural(inner, out),
        Value::Result(Ok(inner)) => write_natural(inner, out),
        Value::Result(Err(inner)) => {
            out.push_str("{\"error\":");
            write_natural(inner, out);
            out.push('}');
        }
        Value::Record(r) => {
            out.push('{');
            for (i, (k, v)) in r.fields.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_natural_str(k, out);
                out.push(':');
                write_natural(v, out);
            }
            out.push('}');
        }
        Value::Enum(e) => match &e.data {
            VariantValue::Unit => write_natural_str(&e.variant, out),
            VariantValue::Tuple(vs) if vs.len() == 1 => {
                out.push('{');
                write_natural_str(&e.variant, out);
                out.push(':');
                write_natural(&vs[0], out);
                out.push('}');
            }
            VariantValue::Tuple(vs) => {
                out.push('{');
                write_natural_str(&e.variant, out);
                out.push_str(":[");
                for (i, v) in vs.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_natural(v, out);
                }
                out.push_str("]}");
            }
            VariantValue::Record(fields) => {
                out.push('{');
                write_natural_str(&e.variant, out);
                out.push_str(":{");
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_natural_str(k, out);
                    out.push(':');
                    write_natural(v, out);
                }
                out.push_str("}}");
            }
        },
        Value::Closure(_) | Value::Cap(_) | Value::Saga(_) => out.push_str("null"),
        _ => out.push_str("null"),
    }
}

fn write_natural_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_value(v: &Value, out: &mut String) {
    match v {
        Value::Unit => out.push_str("{\"unit\":null}"),
        Value::Bool(b) => {
            out.push_str("{\"bool\":");
            out.push_str(if *b { "true" } else { "false" });
            out.push('}');
        }
        Value::Int(n) => {
            out.push_str("{\"int\":");
            out.push_str(&n.to_string());
            out.push('}');
        }
        Value::Float(f) => {
            out.push_str("{\"float\":");
            write_float(*f, out);
            out.push('}');
        }
        Value::Decimal(s) => write_str_tag("decimal", s, out),
        Value::Str(s) => write_str_tag("str", s, out),
        Value::Bytes(b) => {
            out.push_str("{\"bytes\":\"");
            for byte in b {
                out.push_str(&format!("{byte:02x}"));
            }
            out.push_str("\"}");
        }
        Value::Char(c) => write_str_tag("char", &c.to_string(), out),
        Value::Uuid(s) => write_str_tag("uuid", s, out),
        Value::Date(s) => write_str_tag("date", s, out),
        Value::Timestamp(s) => write_str_tag("ts", s, out),
        Value::Duration(s) => write_str_tag("dur", s, out),
        Value::List(vs) => write_array_tag("list", vs, out),
        Value::Set(vs) => write_array_tag("set", vs, out),
        Value::Tuple(vs) => write_array_tag("tuple", vs, out),
        Value::Map(kvs) => {
            out.push_str("{\"map\":[");
            for (i, (k, v)) in kvs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('[');
                write_value(k, out);
                out.push(',');
                write_value(v, out);
                out.push(']');
            }
            out.push_str("]}");
        }
        Value::Option(None) => out.push_str("{\"none\":null}"),
        Value::Option(Some(inner)) => {
            out.push_str("{\"some\":");
            write_value(inner, out);
            out.push('}');
        }
        Value::Result(Ok(inner)) => {
            out.push_str("{\"ok\":");
            write_value(inner, out);
            out.push('}');
        }
        Value::Result(Err(inner)) => {
            out.push_str("{\"err\":");
            write_value(inner, out);
            out.push('}');
        }
        Value::Record(r) => write_record(r, out),
        Value::Enum(e) => write_enum(e, out),
        Value::Closure(c) => {
            // Closures capture an opaque environment; for the
            // self-tagging round-trip format we record only the
            // public surface (param names + a sentinel marker).
            // `decode` rejects this tag — closures are not
            // round-trippable through JSON.
            out.push_str("{\"closure\":[");
            for (i, p) in c.params.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_string(p, out);
            }
            out.push_str("]}");
        }
        Value::Saga(s) => {
            // Saga values reference an opaque AST and are not
            // round-trippable through JSON; the encoding records the
            // public shape (name + step count) only.
            out.push_str("{\"saga\":{\"name\":");
            write_json_string(&s.name, out);
            out.push_str(",\"steps\":");
            out.push_str(&s.steps.len().to_string());
            out.push_str("}}");
        }
        Value::Agent(a) => {
            out.push_str("{\"agent\":{\"name\":");
            write_json_string(&a.name, out);
            out.push_str(",\"llm\":");
            write_json_string(&a.llm, out);
            out.push_str("}}");
        }
        Value::AgentNet(n) => {
            out.push_str("{\"agent_net\":{\"name\":");
            write_json_string(&n.name, out);
            out.push_str("}}");
        }
        Value::Cap(c) => {
            // Cap values carry effect-shape metadata; the JSON form
            // is human-debuggable but not round-trippable.
            out.push_str("{\"cap\":{");
            write_kv_raw(out, "star", if c.star { "true" } else { "false" });
            out.push_str(",\"entries\":[");
            for (i, e) in c.entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('{');
                out.push_str("\"path\":[");
                for (j, seg) in e.path.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    write_json_string(seg, out);
                }
                out.push(']');
                if let Some(a) = &e.allow {
                    out.push_str(",\"allow\":[");
                    for (j, s) in a.iter().enumerate() {
                        if j > 0 {
                            out.push(',');
                        }
                        write_json_string(s, out);
                    }
                    out.push(']');
                }
                out.push('}');
            }
            out.push_str("]}}");
        }
    }
}

fn write_kv_raw(out: &mut String, key: &str, value: &str) {
    out.push('"');
    out.push_str(key);
    out.push_str("\":");
    out.push_str(value);
}

fn write_str_tag(tag: &str, s: &str, out: &mut String) {
    out.push_str("{\"");
    out.push_str(tag);
    out.push_str("\":");
    write_json_string(s, out);
    out.push('}');
}

fn write_array_tag(tag: &str, vs: &[Value], out: &mut String) {
    out.push_str("{\"");
    out.push_str(tag);
    out.push_str("\":[");
    for (i, v) in vs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_value(v, out);
    }
    out.push_str("]}");
}

fn write_record(r: &RecordValue, out: &mut String) {
    out.push_str("{\"rec\":{\"name\":");
    match &r.name {
        Some(n) => write_json_string(n, out),
        None => out.push_str("null"),
    }
    out.push_str(",\"fields\":[");
    for (i, (k, v)) in r.fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        write_json_string(k, out);
        out.push(',');
        write_value(v, out);
        out.push(']');
    }
    out.push_str("]}}");
}

fn write_enum(e: &EnumValue, out: &mut String) {
    out.push_str("{\"enum\":{\"name\":");
    write_json_string(&e.name, out);
    out.push_str(",\"variant\":");
    write_json_string(&e.variant, out);
    out.push_str(",\"data\":");
    match &e.data {
        VariantValue::Unit => out.push_str("{\"unit_v\":null}"),
        VariantValue::Tuple(vs) => write_array_tag("tuple_v", vs, out),
        VariantValue::Record(fs) => {
            out.push_str("{\"rec_v\":[");
            for (i, (k, v)) in fs.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('[');
                write_json_string(k, out);
                out.push(',');
                write_value(v, out);
                out.push(']');
            }
            out.push_str("]}");
        }
    }
    out.push_str("}}");
}

fn write_json_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_float(f: f64, out: &mut String) {
    if f.is_nan() || f.is_infinite() {
        // JSON cannot express NaN/Infinity; emit a sentinel string so
        // the round-trip surfaces an explicit decoder error.
        out.push_str("\"NaN\"");
        return;
    }
    // `{:?}` always includes a decimal point for floats, which keeps
    // the round-trip stable: `42.0` → `42.0` (not `42`).
    out.push_str(&format!("{f:?}"));
}

// ====================================================================
//  Decoder
// ====================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub message: String,
    pub at: usize,
}

impl DecodeError {
    fn new(p: &Parser, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            at: p.pos,
        }
    }
}

/// Decode a self-tagging JSON string produced by `encode`. Strict —
/// rejects unknown tags and trailing input.
pub fn decode(s: &str) -> Result<Value, DecodeError> {
    let mut p = Parser::new(s);
    p.skip_ws();
    let v = p.parse_tagged_value()?;
    p.skip_ws();
    if p.pos != s.len() {
        return Err(DecodeError::new(&p, "trailing input after value"));
    }
    Ok(v)
}

/// M8.T2: parse a natural JSON object into a flat field bag. Each
/// value lands as a primitive `Value` (Int / Float / Bool / Str /
/// Unit) — the caller coerces against the target model's declared
/// field types. Nested objects, arrays, and the JSON `null`-as-Unit
/// rule mirror the spec's surface (§ 16.2). `null` becomes
/// `Value::Unit` so the caller can flag it against the model's
/// declared shape.
pub fn decode_natural_object(s: &str) -> Result<Vec<(String, Value)>, DecodeError> {
    let mut p = Parser::new(s);
    p.skip_ws();
    let pairs = p.parse_natural_object_inline()?;
    p.skip_ws();
    if p.pos != s.len() {
        return Err(DecodeError::new(&p, "trailing input after object"));
    }
    Ok(pairs)
}

struct Parser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn new(src: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
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
        self.pos += 1;
        b
    }

    fn skip_ws(&mut self) {
        while !self.eof() && matches!(self.peek(), b' ' | b'\n' | b'\r' | b'\t') {
            self.bump();
        }
    }

    fn expect_byte(&mut self, b: u8, ctx: &str) -> Result<(), DecodeError> {
        self.skip_ws();
        if self.peek() != b {
            return Err(DecodeError::new(
                self,
                format!("expected `{}` while {ctx}", b as char),
            ));
        }
        self.bump();
        Ok(())
    }

    fn eat_literal(&mut self, lit: &str) -> bool {
        if self.src[self.pos..].starts_with(lit) {
            self.pos += lit.len();
            true
        } else {
            false
        }
    }

    /// M8.T2: read a single JSON scalar (string / number / bool / null)
    /// from the natural-JSON surface. Numbers without a `.` / `e` are
    /// treated as `Int`; otherwise `Float`. `null` lands as `Unit`,
    /// letting the caller flag a missing field against the model.
    /// Nested objects/arrays land as `Record` / `List` (M9.T4 — the
    /// trace replay path needs nested decoding).
    fn parse_natural_value(&mut self) -> Result<Value, DecodeError> {
        self.skip_ws();
        match self.peek() {
            b'"' => Ok(Value::Str(self.parse_string()?)),
            b't' | b'f' => Ok(Value::Bool(self.parse_bool()?)),
            b'n' => {
                self.parse_null()?;
                Ok(Value::Unit)
            }
            b'{' => {
                let pairs = self.parse_natural_object_inline()?;
                Ok(Value::Record(super::value::RecordValue {
                    name: None,
                    fields: pairs,
                }))
            }
            b'[' => {
                self.bump();
                let mut out = Vec::new();
                self.skip_ws();
                if self.peek() == b']' {
                    self.bump();
                    return Ok(Value::List(out));
                }
                loop {
                    self.skip_ws();
                    out.push(self.parse_natural_value()?);
                    self.skip_ws();
                    match self.peek() {
                        b',' => {
                            self.bump();
                        }
                        b']' => {
                            self.bump();
                            break;
                        }
                        _ => {
                            return Err(DecodeError::new(self, "expected `,` or `]` in array"));
                        }
                    }
                }
                Ok(Value::List(out))
            }
            b'-' | b'0'..=b'9' => {
                // Peek ahead to decide int vs float without committing.
                let start = self.pos;
                let mut i = self.pos;
                if self.bytes.get(i) == Some(&b'-') {
                    i += 1;
                }
                let mut is_float = false;
                while i < self.bytes.len()
                    && matches!(
                        self.bytes[i],
                        b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-'
                    )
                {
                    if matches!(self.bytes[i], b'.' | b'e' | b'E') {
                        is_float = true;
                    }
                    i += 1;
                }
                let raw = &self.src[start..i];
                self.pos = i;
                if is_float {
                    raw.parse::<f64>()
                        .map(Value::Float)
                        .map_err(|e| DecodeError::new(self, format!("invalid number `{raw}`: {e}")))
                } else {
                    raw.parse::<i64>()
                        .map(Value::Int)
                        .map_err(|e| DecodeError::new(self, format!("invalid number `{raw}`: {e}")))
                }
            }
            _ => Err(DecodeError::new(self, "expected JSON scalar")),
        }
    }

    /// Parse the body of a JSON object — `{ "k": v, ... }` — into a
    /// flat key/value list. Caller has already verified the leading
    /// `{`; we consume up to and including the matching `}`.
    fn parse_natural_object_inline(&mut self) -> Result<Vec<(String, Value)>, DecodeError> {
        self.expect_byte(b'{', "starting object")?;
        self.skip_ws();
        let mut out: Vec<(String, Value)> = Vec::new();
        if self.peek() == b'}' {
            self.bump();
            return Ok(out);
        }
        loop {
            self.skip_ws();
            let k = self.parse_string()?;
            self.skip_ws();
            self.expect_byte(b':', "after key")?;
            self.skip_ws();
            let v = self.parse_natural_value()?;
            out.push((k, v));
            self.skip_ws();
            match self.peek() {
                b',' => {
                    self.bump();
                    self.skip_ws();
                }
                b'}' => {
                    self.bump();
                    break;
                }
                _ => return Err(DecodeError::new(self, "expected `,` or `}`")),
            }
        }
        Ok(out)
    }

    fn parse_tagged_value(&mut self) -> Result<Value, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'{', "starting tagged value")?;
        self.skip_ws();
        let tag = self.parse_string()?;
        self.skip_ws();
        self.expect_byte(b':', "after tag")?;
        let v = match tag.as_str() {
            "unit" => {
                self.parse_null()?;
                Value::Unit
            }
            "bool" => Value::Bool(self.parse_bool()?),
            "int" => Value::Int(self.parse_int()?),
            "float" => Value::Float(self.parse_float()?),
            "decimal" => Value::Decimal(self.parse_string()?),
            "str" => Value::Str(self.parse_string()?),
            "bytes" => Value::Bytes(self.parse_hex_bytes()?),
            "char" => {
                let s = self.parse_string()?;
                let mut it = s.chars();
                let c = it
                    .next()
                    .ok_or_else(|| DecodeError::new(self, "empty char"))?;
                if it.next().is_some() {
                    return Err(DecodeError::new(self, "char must hold exactly one scalar"));
                }
                Value::Char(c)
            }
            "uuid" => Value::Uuid(self.parse_string()?),
            "date" => Value::Date(self.parse_string()?),
            "ts" => Value::Timestamp(self.parse_string()?),
            "dur" => Value::Duration(self.parse_string()?),
            "list" => Value::List(self.parse_array()?),
            "set" => Value::Set(self.parse_array()?),
            "tuple" => Value::Tuple(self.parse_array()?),
            "map" => Value::Map(self.parse_kv_pairs()?),
            "none" => {
                self.parse_null()?;
                Value::Option(None)
            }
            "some" => Value::Option(Some(Box::new(self.parse_tagged_value()?))),
            "ok" => Value::Result(Ok(Box::new(self.parse_tagged_value()?))),
            "err" => Value::Result(Err(Box::new(self.parse_tagged_value()?))),
            "rec" => Value::Record(self.parse_record_body()?),
            "enum" => Value::Enum(self.parse_enum_body()?),
            other => return Err(DecodeError::new(self, format!("unknown tag `{other}`"))),
        };
        self.skip_ws();
        self.expect_byte(b'}', "closing tagged value")?;
        Ok(v)
    }

    fn parse_null(&mut self) -> Result<(), DecodeError> {
        self.skip_ws();
        if !self.eat_literal("null") {
            return Err(DecodeError::new(self, "expected `null`"));
        }
        Ok(())
    }

    fn parse_bool(&mut self) -> Result<bool, DecodeError> {
        self.skip_ws();
        if self.eat_literal("true") {
            Ok(true)
        } else if self.eat_literal("false") {
            Ok(false)
        } else {
            Err(DecodeError::new(self, "expected `true` or `false`"))
        }
    }

    fn parse_int(&mut self) -> Result<i64, DecodeError> {
        self.skip_ws();
        let start = self.pos;
        if self.peek() == b'-' {
            self.bump();
        }
        while !self.eof() && self.peek().is_ascii_digit() {
            self.bump();
        }
        let raw = &self.src[start..self.pos];
        raw.parse::<i64>()
            .map_err(|e| DecodeError::new(self, format!("invalid int `{raw}`: {e}")))
    }

    fn parse_float(&mut self) -> Result<f64, DecodeError> {
        self.skip_ws();
        // Reject the `"NaN"` sentinel emitted by the encoder.
        if self.peek() == b'"' {
            return Err(DecodeError::new(self, "non-finite float not supported"));
        }
        let start = self.pos;
        if self.peek() == b'-' {
            self.bump();
        }
        while !self.eof() && matches!(self.peek(), b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-') {
            self.bump();
        }
        let raw = &self.src[start..self.pos];
        raw.parse::<f64>()
            .map_err(|e| DecodeError::new(self, format!("invalid float `{raw}`: {e}")))
    }

    fn parse_string(&mut self) -> Result<String, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'"', "starting string")?;
        let mut out = String::new();
        loop {
            if self.eof() {
                return Err(DecodeError::new(self, "unterminated string"));
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
                        b'/' => out.push('/'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'u' => {
                            let mut code: u32 = 0;
                            for _ in 0..4 {
                                let h = self.bump();
                                let v = match h {
                                    b'0'..=b'9' => (h - b'0') as u32,
                                    b'a'..=b'f' => (h - b'a' + 10) as u32,
                                    b'A'..=b'F' => (h - b'A' + 10) as u32,
                                    _ => return Err(DecodeError::new(self, "invalid \\u escape")),
                                };
                                code = (code << 4) | v;
                            }
                            let c = char::from_u32(code)
                                .ok_or_else(|| DecodeError::new(self, "invalid \\u code point"))?;
                            out.push(c);
                        }
                        b => {
                            return Err(DecodeError::new(
                                self,
                                format!("bad escape \\{}", b as char),
                            ))
                        }
                    }
                }
                _ => {
                    let start = self.pos;
                    let lead = self.peek();
                    let len = utf8_len(lead);
                    for _ in 0..len {
                        self.bump();
                    }
                    out.push_str(&self.src[start..self.pos]);
                }
            }
        }
    }

    fn parse_hex_bytes(&mut self) -> Result<Vec<u8>, DecodeError> {
        let s = self.parse_string()?;
        if s.len() % 2 != 0 {
            return Err(DecodeError::new(self, "hex byte string has odd length"));
        }
        let mut out = Vec::with_capacity(s.len() / 2);
        let bytes = s.as_bytes();
        for i in (0..bytes.len()).step_by(2) {
            let h =
                hex_digit(bytes[i]).ok_or_else(|| DecodeError::new(self, "invalid hex digit"))?;
            let l = hex_digit(bytes[i + 1])
                .ok_or_else(|| DecodeError::new(self, "invalid hex digit"))?;
            out.push((h << 4) | l);
        }
        Ok(out)
    }

    fn parse_array(&mut self) -> Result<Vec<Value>, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'[', "starting array")?;
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == b']' {
            self.bump();
            return Ok(out);
        }
        loop {
            out.push(self.parse_tagged_value()?);
            self.skip_ws();
            match self.peek() {
                b',' => {
                    self.bump();
                }
                b']' => {
                    self.bump();
                    return Ok(out);
                }
                _ => return Err(DecodeError::new(self, "expected `,` or `]`")),
            }
        }
    }

    fn parse_kv_pairs(&mut self) -> Result<Vec<(Value, Value)>, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'[', "starting kv list")?;
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == b']' {
            self.bump();
            return Ok(out);
        }
        loop {
            self.skip_ws();
            self.expect_byte(b'[', "starting kv pair")?;
            let k = self.parse_tagged_value()?;
            self.skip_ws();
            self.expect_byte(b',', "between kv pair")?;
            let v = self.parse_tagged_value()?;
            self.skip_ws();
            self.expect_byte(b']', "closing kv pair")?;
            out.push((k, v));
            self.skip_ws();
            match self.peek() {
                b',' => {
                    self.bump();
                }
                b']' => {
                    self.bump();
                    return Ok(out);
                }
                _ => return Err(DecodeError::new(self, "expected `,` or `]` in kv list")),
            }
        }
    }

    fn parse_record_body(&mut self) -> Result<RecordValue, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'{', "starting record body")?;
        self.skip_ws();
        let key1 = self.parse_string()?;
        if key1 != "name" {
            return Err(DecodeError::new(self, "expected `name` first"));
        }
        self.expect_byte(b':', "after name key")?;
        self.skip_ws();
        let name = if self.eat_literal("null") {
            None
        } else {
            Some(self.parse_string()?)
        };
        self.skip_ws();
        self.expect_byte(b',', "between record keys")?;
        self.skip_ws();
        let key2 = self.parse_string()?;
        if key2 != "fields" {
            return Err(DecodeError::new(self, "expected `fields` second"));
        }
        self.expect_byte(b':', "after fields key")?;
        let fields = self.parse_named_fields()?;
        self.skip_ws();
        self.expect_byte(b'}', "closing record body")?;
        Ok(RecordValue { name, fields })
    }

    fn parse_named_fields(&mut self) -> Result<Vec<(String, Value)>, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'[', "starting fields")?;
        let mut out = Vec::new();
        self.skip_ws();
        if self.peek() == b']' {
            self.bump();
            return Ok(out);
        }
        loop {
            self.skip_ws();
            self.expect_byte(b'[', "starting field pair")?;
            let k = self.parse_string()?;
            self.skip_ws();
            self.expect_byte(b',', "between field key and value")?;
            let v = self.parse_tagged_value()?;
            self.skip_ws();
            self.expect_byte(b']', "closing field pair")?;
            out.push((k, v));
            self.skip_ws();
            match self.peek() {
                b',' => {
                    self.bump();
                }
                b']' => {
                    self.bump();
                    return Ok(out);
                }
                _ => return Err(DecodeError::new(self, "expected `,` or `]` in fields")),
            }
        }
    }

    fn parse_enum_body(&mut self) -> Result<EnumValue, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'{', "starting enum body")?;
        self.skip_ws();
        let key1 = self.parse_string()?;
        if key1 != "name" {
            return Err(DecodeError::new(self, "expected `name` first"));
        }
        self.expect_byte(b':', "after name key")?;
        let name = self.parse_string()?;
        self.skip_ws();
        self.expect_byte(b',', "between enum keys")?;
        self.skip_ws();
        let key2 = self.parse_string()?;
        if key2 != "variant" {
            return Err(DecodeError::new(self, "expected `variant`"));
        }
        self.expect_byte(b':', "after variant key")?;
        let variant = self.parse_string()?;
        self.skip_ws();
        self.expect_byte(b',', "between enum keys")?;
        self.skip_ws();
        let key3 = self.parse_string()?;
        if key3 != "data" {
            return Err(DecodeError::new(self, "expected `data`"));
        }
        self.expect_byte(b':', "after data key")?;
        let data = self.parse_variant_data()?;
        self.skip_ws();
        self.expect_byte(b'}', "closing enum body")?;
        Ok(EnumValue {
            name,
            variant,
            data,
        })
    }

    fn parse_variant_data(&mut self) -> Result<VariantValue, DecodeError> {
        self.skip_ws();
        self.expect_byte(b'{', "starting variant data")?;
        self.skip_ws();
        let tag = self.parse_string()?;
        self.skip_ws();
        self.expect_byte(b':', "after variant data tag")?;
        let v = match tag.as_str() {
            "unit_v" => {
                self.parse_null()?;
                VariantValue::Unit
            }
            "tuple_v" => VariantValue::Tuple(self.parse_array()?),
            "rec_v" => VariantValue::Record(self.parse_named_fields()?),
            other => {
                return Err(DecodeError::new(
                    self,
                    format!("unknown variant data tag `{other}`"),
                ))
            }
        };
        self.skip_ws();
        self.expect_byte(b'}', "closing variant data")?;
        Ok(v)
    }
}

// ====================================================================
//  Helpers
// ====================================================================

fn utf8_len(first_byte: u8) -> usize {
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

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ====================================================================
//  Tests — round-trip per Value variant
// ====================================================================

#[cfg(test)]
mod tests {
    use super::super::value::{EnumValue, RecordValue, Value, VariantValue};
    use super::*;

    fn roundtrip(v: Value) {
        let encoded = encode(&v);
        let decoded =
            decode(&encoded).unwrap_or_else(|e| panic!("decode error: {e:?} on {encoded:?}"));
        assert_eq!(decoded, v, "round-trip drift on {encoded:?}");
    }

    // ---- primitives ----

    #[test]
    fn rt_unit() {
        roundtrip(Value::Unit);
    }
    #[test]
    fn rt_bool_true() {
        roundtrip(Value::Bool(true));
    }
    #[test]
    fn rt_bool_false() {
        roundtrip(Value::Bool(false));
    }
    #[test]
    fn rt_int_zero() {
        roundtrip(Value::Int(0));
    }
    #[test]
    fn rt_int_neg() {
        roundtrip(Value::Int(-12345));
    }
    #[test]
    fn rt_int_max() {
        roundtrip(Value::Int(i64::MAX));
    }
    #[test]
    fn rt_int_min() {
        roundtrip(Value::Int(i64::MIN));
    }
    #[test]
    fn rt_float_simple() {
        roundtrip(Value::Float(2.5));
    }
    #[test]
    fn rt_float_negative() {
        roundtrip(Value::Float(-1.5e-3));
    }
    #[test]
    fn rt_float_whole_keeps_decimal() {
        // `42.0` must stay a float, not collapse to an int — the
        // encoder uses `{:?}` to preserve the `.0`.
        let s = encode(&Value::Float(42.0));
        assert_eq!(s, "{\"float\":42.0}");
        roundtrip(Value::Float(42.0));
    }
    #[test]
    fn rt_decimal() {
        roundtrip(Value::Decimal("12.500000000000".to_string()));
    }
    #[test]
    fn rt_string_simple() {
        roundtrip(Value::Str("hello".to_string()));
    }
    #[test]
    fn rt_string_with_escapes() {
        roundtrip(Value::Str("line1\nline2\t\"quoted\"\\back".to_string()));
    }
    #[test]
    fn rt_string_unicode() {
        roundtrip(Value::Str("héllo ☃ こんにちは".to_string()));
    }
    #[test]
    fn rt_bytes() {
        roundtrip(Value::Bytes(vec![0xff, 0x00, 0x42, 0xab]));
    }
    #[test]
    fn rt_bytes_empty() {
        roundtrip(Value::Bytes(Vec::new()));
    }
    #[test]
    fn rt_char() {
        roundtrip(Value::Char('a'));
    }
    #[test]
    fn rt_char_unicode() {
        roundtrip(Value::Char('☃'));
    }
    #[test]
    fn rt_uuid() {
        roundtrip(Value::Uuid(
            "01931f55-3b70-7c42-ab00-1f0c0e1d8a9b".to_string(),
        ));
    }
    #[test]
    fn rt_date() {
        roundtrip(Value::Date("2026-05-07".to_string()));
    }
    #[test]
    fn rt_timestamp() {
        roundtrip(Value::Timestamp("2026-05-07T08:30:00Z".to_string()));
    }
    #[test]
    fn rt_duration() {
        roundtrip(Value::Duration("3s".to_string()));
    }

    // ---- collections ----

    #[test]
    fn rt_list_empty() {
        roundtrip(Value::List(Vec::new()));
    }
    #[test]
    fn rt_list_ints() {
        roundtrip(Value::List(vec![
            Value::Int(1),
            Value::Int(2),
            Value::Int(3),
        ]));
    }
    #[test]
    fn rt_list_mixed_collections() {
        roundtrip(Value::List(vec![
            Value::Bool(true),
            Value::Str("x".into()),
            Value::List(vec![Value::Int(1)]),
        ]));
    }
    #[test]
    fn rt_set() {
        roundtrip(Value::Set(vec![Value::Int(1), Value::Int(2)]));
    }
    #[test]
    fn rt_tuple_pair() {
        roundtrip(Value::Tuple(vec![Value::Str("ok".into()), Value::Int(42)]));
    }
    #[test]
    fn rt_tuple_unit_arity_zero() {
        roundtrip(Value::Tuple(Vec::new()));
    }
    #[test]
    fn rt_map_empty() {
        roundtrip(Value::Map(Vec::new()));
    }
    #[test]
    fn rt_map_str_to_int() {
        roundtrip(Value::Map(vec![
            (Value::Str("a".into()), Value::Int(1)),
            (Value::Str("b".into()), Value::Int(2)),
        ]));
    }
    #[test]
    fn rt_map_keyed_by_tuple() {
        roundtrip(Value::Map(vec![(
            Value::Tuple(vec![Value::Int(1), Value::Int(2)]),
            Value::Bool(true),
        )]));
    }

    // ---- option / result ----

    #[test]
    fn rt_option_none() {
        roundtrip(Value::none());
    }
    #[test]
    fn rt_option_some_int() {
        roundtrip(Value::some(Value::Int(7)));
    }
    #[test]
    fn rt_option_some_some() {
        roundtrip(Value::some(Value::some(Value::Int(7))));
    }
    #[test]
    fn rt_result_ok() {
        roundtrip(Value::ok(Value::Int(42)));
    }
    #[test]
    fn rt_result_err() {
        roundtrip(Value::err(Value::Str("bad".into())));
    }

    // ---- records ----

    #[test]
    fn rt_record_named() {
        roundtrip(Value::Record(RecordValue {
            name: Some("User".into()),
            fields: vec![
                ("id".into(), Value::Uuid("…".into())),
                ("age".into(), Value::Int(36)),
            ],
        }));
    }
    #[test]
    fn rt_record_anonymous_empty() {
        roundtrip(Value::Record(RecordValue {
            name: None,
            fields: Vec::new(),
        }));
    }
    #[test]
    fn rt_record_anonymous_map_form() {
        roundtrip(Value::Record(RecordValue {
            name: None,
            fields: vec![("a".into(), Value::Int(1)), ("b".into(), Value::Int(2))],
        }));
    }

    // ---- enums ----

    #[test]
    fn rt_enum_unit_variant() {
        roundtrip(Value::Enum(EnumValue {
            name: "Color".into(),
            variant: "Red".into(),
            data: VariantValue::Unit,
        }));
    }
    #[test]
    fn rt_enum_tuple_variant() {
        roundtrip(Value::Enum(EnumValue {
            name: "Status".into(),
            variant: "Active".into(),
            data: VariantValue::Tuple(vec![Value::Timestamp("2026-05-07T08:30:00Z".into())]),
        }));
    }
    #[test]
    fn rt_enum_record_variant() {
        roundtrip(Value::Enum(EnumValue {
            name: "Status".into(),
            variant: "Banned".into(),
            data: VariantValue::Record(vec![
                ("reason".into(), Value::Str("spam".into())),
                ("until".into(), Value::none()),
            ]),
        }));
    }

    // ---- nested combinations ----

    #[test]
    fn rt_record_with_nested_list_and_enum() {
        roundtrip(Value::Record(RecordValue {
            name: Some("Order".into()),
            fields: vec![
                (
                    "lines".into(),
                    Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
                ),
                (
                    "status".into(),
                    Value::Enum(EnumValue {
                        name: "OrderStatus".into(),
                        variant: "Paid".into(),
                        data: VariantValue::Unit,
                    }),
                ),
            ],
        }));
    }

    // ---- error cases ----

    #[test]
    fn decode_unknown_tag_fails() {
        assert!(decode("{\"frob\":42}").is_err());
    }

    #[test]
    fn decode_trailing_input_fails() {
        assert!(decode("{\"int\":1}EXTRA").is_err());
    }

    #[test]
    fn decode_truncated_string_fails() {
        assert!(decode("{\"str\":\"abc").is_err());
    }
}
