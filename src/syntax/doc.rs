//! M13.T2 — `aeris doc <file>` extractor.
//!
//! Walks the lexer token stream of an `.aer` file and pulls every
//! `///` doc-comment cluster that prefixes a top-level declaration.
//! Emits one `DocEntry` per documented item, ready to be serialised
//! as JSONL by the CLI driver.
//!
//! The extractor is purposely minimal: doc clusters are attached to
//! whichever named decl follows them. Free-floating `///` comments
//! (no following item) are dropped without warning so authors can
//! park exploratory notes without breaking the build.

use super::lexer::tokenize;
use super::token::{Keyword, TokenKind};

/// One extracted documentation block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocEntry {
    /// `fn`, `record`, `enum`, `model`, `type`, `const`, `saga`,
    /// `agent`, `agent_net`, `policy`, `test`, `property`.
    pub kind: String,
    /// The declared name (and `@vN` suffix for models).
    pub name: String,
    /// Joined doc text. Lines are concatenated with `\n`; the leading
    /// `///` and one optional space are stripped per line.
    pub doc: String,
}

/// Extract every documented top-level decl from `src`. Lex errors
/// short-circuit with `Err`; downstream callers map this to the
/// CLI's exit code.
pub fn extract_docs(src: &str) -> Result<Vec<DocEntry>, String> {
    let tokens = tokenize(src).map_err(|e| format!("lex error: {e:?}"))?;
    let mut out: Vec<DocEntry> = Vec::new();
    let mut buffer: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i].kind {
            TokenKind::DocComment(line) => {
                buffer.push(strip_doc_prefix(line));
                i += 1;
            }
            // Plain comments and whitespace don't reset the doc buffer
            // — `///` blocks may be interleaved with line / block
            // comments without losing attribution.
            TokenKind::LineComment(_) | TokenKind::BlockComment(_) => {
                i += 1;
            }
            TokenKind::Keyword(kw) if is_item_kw(*kw) => {
                if let Some(entry) = scan_item(&tokens[i..], &buffer) {
                    out.push(entry);
                }
                buffer.clear();
                i += 1;
            }
            // `pub` precedes a decl keyword — keep the buffer alive.
            TokenKind::Keyword(Keyword::Pub) => {
                i += 1;
            }
            // Anything else terminates an unattached doc cluster: a
            // standalone expression at the top level (which the parser
            // would reject) or a stray symbol.
            _ => {
                buffer.clear();
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Render entries as one JSONL line per `DocEntry`. The order is
/// preserved from the input source so snapshot tests stay stable.
pub fn render_jsonl(entries: &[DocEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push('{');
        out.push_str(&format!("\"kind\":\"{}\",", json_escape(&e.kind)));
        out.push_str(&format!("\"name\":\"{}\",", json_escape(&e.name)));
        out.push_str(&format!("\"doc\":\"{}\"", json_escape(&e.doc)));
        out.push_str("}\n");
    }
    out
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn strip_doc_prefix(raw: &str) -> String {
    // The lexer hands us the body after `///` already, but we still
    // strip a leading space so `/// hello` and `///hello` produce the
    // same `hello`.
    let s = raw.strip_prefix(' ').unwrap_or(raw);
    s.to_string()
}

fn is_item_kw(kw: Keyword) -> bool {
    matches!(
        kw,
        Keyword::Fn
            | Keyword::Record
            | Keyword::Enum
            | Keyword::Model
            | Keyword::Type
            | Keyword::Const
            | Keyword::Saga
            | Keyword::Pipeline
            | Keyword::Agent
            | Keyword::AgentNet
            | Keyword::Policy
            | Keyword::Test
            | Keyword::Property
    )
}

fn item_kw_str(kw: Keyword) -> &'static str {
    match kw {
        Keyword::Fn => "fn",
        Keyword::Record => "record",
        Keyword::Enum => "enum",
        Keyword::Model => "model",
        Keyword::Type => "type",
        Keyword::Const => "const",
        Keyword::Saga => "saga",
        Keyword::Pipeline => "pipeline",
        Keyword::Agent => "agent",
        Keyword::AgentNet => "agent_net",
        Keyword::Policy => "policy",
        Keyword::Test => "test",
        Keyword::Property => "property",
        _ => "?",
    }
}

fn scan_item(rest: &[super::token::Token], buffer: &[String]) -> Option<DocEntry> {
    if buffer.is_empty() {
        return None;
    }
    let kw = match &rest[0].kind {
        TokenKind::Keyword(kw) => *kw,
        _ => return None,
    };
    // The item's name is the first thing after the keyword that looks
    // like an identifier or a string literal (for `test` / `property`).
    let mut idx = 1;
    while idx < rest.len() {
        match &rest[idx].kind {
            TokenKind::Ident(name) => {
                let mut full = name.clone();
                // For `model M @ vN`, append the version tag.
                if matches!(kw, Keyword::Model) && idx + 2 < rest.len() {
                    if let TokenKind::At = rest[idx + 1].kind {
                        if let TokenKind::Ident(v) = &rest[idx + 2].kind {
                            full.push('@');
                            full.push_str(v);
                        }
                    }
                }
                return Some(DocEntry {
                    kind: item_kw_str(kw).to_string(),
                    name: full,
                    doc: buffer.join("\n"),
                });
            }
            TokenKind::Str(s) if matches!(kw, Keyword::Test | Keyword::Property) => {
                return Some(DocEntry {
                    kind: item_kw_str(kw).to_string(),
                    name: s.clone(),
                    doc: buffer.join("\n"),
                });
            }
            _ => idx += 1,
        }
    }
    None
}

// ====================================================================
//  Tests
// ====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_doc_for_fn() {
        let src = r#"
            /// Add two integers.
            fn add(a: int, b: int) -> int { a + b }
        "#;
        let docs = extract_docs(src).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].kind, "fn");
        assert_eq!(docs[0].name, "add");
        assert_eq!(docs[0].doc, "Add two integers.");
    }

    #[test]
    fn extract_doc_for_record() {
        let src = r#"
            /// Application user.
            record User { id: uuid }
        "#;
        let docs = extract_docs(src).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].kind, "record");
        assert_eq!(docs[0].name, "User");
    }

    #[test]
    fn extract_multi_line_doc() {
        let src = r#"
            /// Settle a batch of invoices.
            ///
            /// Forward execution charges every invoice; rollback is
            /// triggered if any single charge fails.
            fn settle() {}
        "#;
        let docs = extract_docs(src).unwrap();
        assert_eq!(docs[0].name, "settle");
        assert!(docs[0].doc.contains("Settle"));
        assert!(docs[0].doc.contains("rollback"));
        assert_eq!(docs[0].doc.lines().count(), 4);
    }

    #[test]
    fn extract_doc_for_enum_model_type_const() {
        let src = r#"
            /// statuses
            enum Status { A, B }

            /// invoice schema
            model Invoice@v1 { id: uuid }

            /// email is a refined string
            type Email = string

            /// world greeting
            const GREETING: string = "hello"
        "#;
        let docs = extract_docs(src).unwrap();
        let names: Vec<&str> = docs.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"Status"));
        assert!(names.contains(&"Invoice@v1"));
        assert!(names.contains(&"Email"));
        assert!(names.contains(&"GREETING"));
    }

    #[test]
    fn extract_doc_for_test_and_property() {
        let src = r#"
            /// Smoke test for adder.
            test "add commutes" { let _ = 1 }

            /// Property: addition is commutative.
            property "comm" with (a: int, b: int) { let _ = a }
        "#;
        let docs = extract_docs(src).unwrap();
        assert!(docs.iter().any(|d| d.kind == "test" && d.name == "add commutes"));
        assert!(docs.iter().any(|d| d.kind == "property" && d.name == "comm"));
    }

    #[test]
    fn pub_keyword_does_not_break_attribution() {
        let src = r#"
            /// publicly visible adder
            pub fn add(a: int, b: int) -> int { a + b }
        "#;
        let docs = extract_docs(src).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].name, "add");
    }

    #[test]
    fn floating_doc_with_no_decl_is_dropped() {
        let src = "/// orphan\n";
        let docs = extract_docs(src).unwrap();
        assert!(docs.is_empty());
    }

    #[test]
    fn line_comment_between_doc_and_decl_keeps_attribution() {
        // A `// note` between `///` and the decl must not steal the
        // doc buffer — only top-level tokens that *aren't* trivia
        // reset attribution.
        let src = r#"
            /// docstring
            // implementation note
            fn f() {}
        "#;
        let docs = extract_docs(src).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].doc, "docstring");
    }

    #[test]
    fn render_jsonl_emits_one_line_per_entry() {
        let entries = vec![
            DocEntry {
                kind: "fn".into(),
                name: "a".into(),
                doc: "doc-a".into(),
            },
            DocEntry {
                kind: "record".into(),
                name: "B".into(),
                doc: "doc-b".into(),
            },
        ];
        let s = render_jsonl(&entries);
        let lines: Vec<&str> = s.trim_end().split('\n').collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"kind\":\"fn\""));
        assert!(lines[0].contains("\"name\":\"a\""));
        assert!(lines[1].contains("\"name\":\"B\""));
    }

    #[test]
    fn jsonl_output_escapes_quotes_and_newlines() {
        let entry = DocEntry {
            kind: "fn".into(),
            name: "f".into(),
            doc: "line 1\nline \"two\"".into(),
        };
        let s = render_jsonl(&[entry]);
        assert!(s.contains("line 1\\nline \\\"two\\\""));
    }

    #[test]
    fn snapshot_against_three_decl_module() {
        let src = r#"
            /// First fn.
            fn f1() {}

            /// Record `R`.
            record R { x: int }

            /// Saga top-level.
            saga settle(cap: cap[http.post]) {
                intent "x"
                step a { do { } undo noop }
            }
        "#;
        let docs = extract_docs(src).unwrap();
        let jsonl = render_jsonl(&docs);
        // Stable substrings — full body printed once for clarity.
        assert!(jsonl.contains("\"kind\":\"fn\",\"name\":\"f1\""));
        assert!(jsonl.contains("\"kind\":\"record\",\"name\":\"R\""));
        assert!(jsonl.contains("\"kind\":\"saga\",\"name\":\"settle\""));
    }

    #[test]
    fn unterminated_string_lex_error_propagates() {
        // The doc extractor goes through the lexer; any lex failure
        // surfaces as a clean `Err` rather than a panic.
        let bad = "\"abc";
        assert!(extract_docs(bad).is_err());
    }
}
