//! Aeris pretty-printer.
//!
//! Realises `aeris fmt` (`docs/language.md` § 25.2) for the parser
//! surface available at M1.T10. The formatter is **total** within
//! its scope: every input that parses produces an output that
//! re-parses to the same AST and is **idempotent** —
//! `format(parse(format(parse(s)))) == format(parse(s))`.
//!
//! Function bodies are still captured as `RawSpan` by `parse_module`;
//! `format_module` therefore takes the original source and slices the
//! body bytes verbatim. Anything the parser fully structures (item
//! signatures, expressions, patterns, contracts) is rebuilt
//! canonically.

use super::ast::{
    AgentDecl, AgentNetDecl, AssignOp, BinOp, Block, CapEntry, CapNarrowKind, CapPath, ConstDecl,
    DeclField, ElseBranch, EnumDecl, EnumVariant, Expr, FlowDecl, FlowStage, FnDecl, Item,
    LambdaParam, ListPatElem, MatchArm, ModelDecl, Module, Param, Pattern, PolicyDecl, RawSpan,
    RecordDecl, RecordField, RecordLit, RecordPatField, SagaDecl, SagaStep, Stmt, Type,
    TypeAliasDecl, UnOp, UndoForm, UseDecl, VariantData, Visibility,
};

/// Canonical form of a single expression. Exposed for `aeris fmt` and
/// for round-trip fixtures (M1.T10).
pub fn format_expression(e: &Expr) -> String {
    let mut out = String::new();
    fmt_expr_at(e, &mut out, 0, 0);
    out
}

/// Canonical form of an entire module. The original source is required
/// because function bodies are still represented as `RawSpan` ranges.
pub fn format_module(m: &Module, source: &str) -> String {
    let mut out = String::new();
    for u in &m.uses {
        fmt_use(u, &mut out, source);
        out.push('\n');
    }
    if !m.uses.is_empty() && !m.items.is_empty() {
        out.push('\n');
    }
    let mut first = true;
    for it in &m.items {
        if !first {
            out.push('\n');
        }
        first = false;
        fmt_item(it, &mut out, source, 0);
        out.push('\n');
    }
    out
}

// ====================================================================
//  Expressions — precedence-aware
// ====================================================================
//
//  Levels mirror `parser.rs`. Higher = tighter binding.
//
//    0  assign (right-assoc)
//    1  range
//    2  or
//    3  and
//    4  is / as
//    5  cmp (==, !=, <, <=, >, >=)
//    6  bitops (& | ^)
//    7  shift (<< >>)
//    8  addsub (+ -)
//    9  muldiv (* / %)
//   10  prefix unary
//   11  postfix (call, index, field, ?)
//
//  The fmt routine takes (outer_prec, side) where side=0 means left and
//  side=1 means right. A right-side child at the same precedence as its
//  parent must be parenthesised because all our binops are left-assoc.

fn binop_prec(op: BinOp) -> u8 {
    match op {
        BinOp::Or => 2,
        BinOp::And => 3,
        BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => 5,
        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor => 6,
        BinOp::Shl | BinOp::Shr => 7,
        BinOp::Add | BinOp::Sub => 8,
        BinOp::Mul | BinOp::Div | BinOp::Rem => 9,
    }
}

fn binop_str(op: BinOp) -> &'static str {
    match op {
        BinOp::Or => "or",
        BinOp::And => "and",
        BinOp::Eq => "==",
        BinOp::Ne => "!=",
        BinOp::Lt => "<",
        BinOp::Le => "<=",
        BinOp::Gt => ">",
        BinOp::Ge => ">=",
        BinOp::BitAnd => "&",
        BinOp::BitOr => "|",
        BinOp::BitXor => "^",
        BinOp::Shl => "<<",
        BinOp::Shr => ">>",
        BinOp::Add => "+",
        BinOp::Sub => "-",
        BinOp::Mul => "*",
        BinOp::Div => "/",
        BinOp::Rem => "%",
    }
}

fn assign_str(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Eq => "=",
        AssignOp::AddEq => "+=",
        AssignOp::SubEq => "-=",
        AssignOp::MulEq => "*=",
        AssignOp::DivEq => "/=",
        AssignOp::RemEq => "%=",
    }
}

fn fmt_expr_at(e: &Expr, out: &mut String, outer: u8, side: u8) {
    match e {
        // ---- atomic literals ----
        Expr::Int(n, _) => out.push_str(&n.to_string()),
        Expr::Float(f, _) => out.push_str(&format_float(*f)),
        Expr::Bool(b, _) => out.push_str(if *b { "true" } else { "false" }),
        Expr::Str(s, _) => {
            out.push('"');
            out.push_str(&escape_str(s));
            out.push('"');
        }
        Expr::Bytes(b, _) => {
            out.push_str("b\"");
            for byte in b {
                match *byte {
                    b'"' => out.push_str("\\\""),
                    b'\\' => out.push_str("\\\\"),
                    b'\n' => out.push_str("\\n"),
                    b'\t' => out.push_str("\\t"),
                    b'\r' => out.push_str("\\r"),
                    0x20..=0x7e => out.push(*byte as char),
                    n => out.push_str(&format!("\\x{n:02x}")),
                }
            }
            out.push('"');
        }
        Expr::Char(c, _) => {
            out.push('\'');
            match c {
                '\n' => out.push_str("\\n"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                '\\' => out.push_str("\\\\"),
                '\'' => out.push_str("\\'"),
                _ => out.push(*c),
            }
            out.push('\'');
        }
        Expr::Date(s, _) | Expr::Timestamp(s, _) | Expr::Duration(s, _) => out.push_str(s),
        Expr::Unit(_) => out.push_str("()"),
        Expr::Ident(name, _) => out.push_str(name),
        Expr::ModelRef { name, version, .. } => {
            out.push_str(name);
            out.push('@');
            out.push('v');
            out.push_str(&version.to_string());
        }

        // ---- compound literals ----
        Expr::Tuple(elems, _) => {
            out.push('(');
            for (i, x) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_expr_at(x, out, 0, 0);
            }
            out.push(')');
        }
        Expr::List(elems, _) => {
            out.push('[');
            for (i, x) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_expr_at(x, out, 0, 0);
            }
            out.push(']');
        }
        Expr::Record(rl, _) => fmt_record_lit(rl, out),

        // ---- operators ----
        Expr::Binary { op, lhs, rhs, .. } => {
            let prec = binop_prec(*op);
            let needs_paren = prec < outer || (prec == outer && side == 1);
            if needs_paren {
                out.push('(');
            }
            fmt_expr_at(lhs, out, prec, 0);
            out.push(' ');
            out.push_str(binop_str(*op));
            out.push(' ');
            fmt_expr_at(rhs, out, prec, 1);
            if needs_paren {
                out.push(')');
            }
        }
        Expr::Unary { op, expr, .. } => {
            // prefix unary at level 10; rendered without parens around the operand
            // unless the operand is another binary at lower precedence.
            let s = match op {
                UnOp::Neg => "-",
                UnOp::Not => "not ",
            };
            let needs_paren = 10 < outer;
            if needs_paren {
                out.push('(');
            }
            out.push_str(s);
            fmt_expr_at(expr, out, 10, 0);
            if needs_paren {
                out.push(')');
            }
        }

        // ---- postfix (always level 11; no outer parens) ----
        Expr::Field { base, name, .. } => {
            fmt_expr_at(base, out, 11, 0);
            out.push('.');
            out.push_str(name);
        }
        Expr::Call { callee, args, .. } => {
            fmt_expr_at(callee, out, 11, 0);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                if let Some(n) = &a.name {
                    out.push_str(n);
                    out.push_str(": ");
                }
                fmt_expr_at(&a.value, out, 0, 0);
            }
            out.push(')');
        }
        Expr::Index { base, index, .. } => {
            fmt_expr_at(base, out, 11, 0);
            out.push('[');
            fmt_expr_at(index, out, 0, 0);
            out.push(']');
        }
        Expr::Try { expr, .. } => {
            fmt_expr_at(expr, out, 11, 0);
            out.push('?');
        }

        // ---- coercion / refinement ----
        Expr::Cast { expr, ty, .. } => {
            let prec = 4;
            let needs_paren = prec < outer || (prec == outer && side == 1);
            if needs_paren {
                out.push('(');
            }
            fmt_expr_at(expr, out, prec, 0);
            out.push_str(" as ");
            fmt_type(ty, out);
            if needs_paren {
                out.push(')');
            }
        }
        Expr::IsCheck { expr, pat, .. } => {
            let prec = 4;
            let needs_paren = prec < outer || (prec == outer && side == 1);
            if needs_paren {
                out.push('(');
            }
            fmt_expr_at(expr, out, prec, 0);
            out.push_str(" is ");
            fmt_pattern(pat, out);
            if needs_paren {
                out.push(')');
            }
        }

        // ---- ranges ----
        Expr::Range {
            start,
            end,
            inclusive,
            ..
        } => {
            let prec = 1;
            let needs_paren = prec < outer;
            if needs_paren {
                out.push('(');
            }
            if let Some(s) = start {
                fmt_expr_at(s, out, prec + 1, 0);
            }
            out.push_str(if *inclusive { "..=" } else { ".." });
            if let Some(e) = end {
                fmt_expr_at(e, out, prec + 1, 1);
            }
            if needs_paren {
                out.push(')');
            }
        }

        // ---- control flow ----
        Expr::If {
            cond,
            then_blk,
            else_,
            ..
        } => {
            out.push_str("if ");
            fmt_expr_at(cond, out, 0, 0);
            out.push(' ');
            fmt_block_inline(then_blk, out);
            if let Some(b) = else_ {
                out.push_str(" else ");
                match b {
                    ElseBranch::ElseIf(e) => fmt_expr_at(e, out, 0, 0),
                    ElseBranch::Else(blk) => fmt_block_inline(blk, out),
                }
            }
        }
        Expr::Match {
            scrutinee, arms, ..
        } => {
            out.push_str("match ");
            fmt_expr_at(scrutinee, out, 0, 0);
            out.push_str(" {");
            for (i, a) in arms.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push(' ');
                fmt_match_arm(a, out);
            }
            out.push_str(" }");
        }
        Expr::Block(b, _) => fmt_block_inline(b, out),

        // ---- language-level constructs ----
        Expr::Lambda {
            params,
            ret_ty,
            body,
            ..
        } => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_lambda_param(p, out);
            }
            out.push(')');
            if let Some(t) = ret_ty {
                out.push_str(" -> ");
                fmt_type(t, out);
            }
            out.push(' ');
            fmt_block_inline(body, out);
        }
        Expr::Spawn { body, .. } => {
            out.push_str("spawn ");
            fmt_block_inline(body, out);
        }
        Expr::Await { expr, .. } => {
            out.push_str("await ");
            fmt_expr_at(expr, out, 10, 0);
        }
        Expr::Raise { expr, .. } => {
            out.push_str("raise ");
            fmt_expr_at(expr, out, 0, 0);
        }
        Expr::Return { expr, .. } => {
            out.push_str("return");
            if let Some(e) = expr {
                out.push(' ');
                fmt_expr_at(e, out, 0, 0);
            }
        }
        Expr::Break { label, expr, .. } => {
            out.push_str("break");
            if let Some(l) = label {
                out.push_str(" '");
                out.push_str(l);
            }
            if let Some(e) = expr {
                out.push(' ');
                fmt_expr_at(e, out, 0, 0);
            }
        }
        Expr::Continue { label, .. } => {
            out.push_str("continue");
            if let Some(l) = label {
                out.push_str(" '");
                out.push_str(l);
            }
        }
        Expr::IntentBlock { label, body, .. } => {
            out.push_str("intent \"");
            out.push_str(&escape_str(label));
            out.push_str("\" ");
            fmt_block_inline(body, out);
        }

        // ---- assignment (right-assoc) ----
        Expr::Assign {
            op, target, value, ..
        } => {
            let prec = 0;
            let needs_paren = prec < outer;
            if needs_paren {
                out.push('(');
            }
            fmt_expr_at(target, out, 11, 0);
            out.push(' ');
            out.push_str(assign_str(*op));
            out.push(' ');
            fmt_expr_at(value, out, prec, 1);
            if needs_paren {
                out.push(')');
            }
        }

        // ---- cap-narrow ----
        Expr::CapNarrow { kind, entries, .. } => {
            out.push_str(match kind {
                CapNarrowKind::Subset => "cap.subset[",
                CapNarrowKind::TestSubset => "cap.test_subset[",
            });
            fmt_cap_entries(entries, out);
            out.push(']');
        }
    }
}

fn fmt_record_lit(rl: &RecordLit, out: &mut String) {
    if let Some(n) = &rl.ty_name {
        out.push_str(n);
        if let Some(v) = rl.ty_version {
            out.push_str("@v");
            out.push_str(&v.to_string());
        }
        out.push(' ');
    }
    out.push('{');
    let mut sep_needed = false;
    if let Some(s) = &rl.spread {
        out.push_str(" ..");
        fmt_expr_at(s, out, 0, 0);
        sep_needed = true;
    }
    for f in &rl.fields {
        if sep_needed {
            out.push(',');
        }
        out.push(' ');
        out.push_str(&f.name);
        out.push_str(": ");
        fmt_expr_at(&f.value, out, 0, 0);
        sep_needed = true;
    }
    if sep_needed {
        out.push(' ');
    }
    out.push('}');
}

fn fmt_match_arm(a: &MatchArm, out: &mut String) {
    fmt_pattern(&a.pattern, out);
    if let Some(g) = &a.guard {
        out.push_str(" if ");
        fmt_expr_at(g, out, 0, 0);
    }
    out.push_str(" -> ");
    fmt_expr_at(&a.body, out, 0, 0);
}

fn fmt_lambda_param(p: &LambdaParam, out: &mut String) {
    out.push_str(&p.name);
    if let Some(t) = &p.ty {
        out.push_str(": ");
        fmt_type(t, out);
    }
}

fn fmt_block_inline(b: &Block, out: &mut String) {
    out.push('{');
    let mut first = true;
    for s in &b.stmts {
        if first {
            out.push(' ');
            first = false;
        } else {
            out.push_str("; ");
        }
        fmt_stmt(s, out);
    }
    if let Some(t) = &b.tail {
        if first {
            out.push(' ');
        } else {
            out.push_str("; ");
        }
        fmt_expr_at(t, out, 0, 0);
        first = false;
    }
    if !first {
        out.push(' ');
    }
    out.push('}');
}

fn fmt_stmt(s: &Stmt, out: &mut String) {
    match s {
        Stmt::Let {
            name, ty, value, ..
        } => {
            out.push_str("let ");
            out.push_str(name);
            if let Some(t) = ty {
                out.push_str(": ");
                fmt_type(t, out);
            }
            out.push_str(" = ");
            fmt_expr_at(value, out, 0, 0);
        }
        Stmt::Var {
            name, ty, value, ..
        } => {
            out.push_str("var ");
            out.push_str(name);
            if let Some(t) = ty {
                out.push_str(": ");
                fmt_type(t, out);
            }
            out.push_str(" = ");
            fmt_expr_at(value, out, 0, 0);
        }
        Stmt::For {
            var, iter, body, ..
        } => {
            out.push_str("for ");
            out.push_str(var);
            out.push_str(" in ");
            fmt_expr_at(iter, out, 0, 0);
            out.push(' ');
            fmt_block_inline(body, out);
        }
        Stmt::While { cond, body, .. } => {
            out.push_str("while ");
            fmt_expr_at(cond, out, 0, 0);
            out.push(' ');
            fmt_block_inline(body, out);
        }
        Stmt::Expr(e) => fmt_expr_at(e, out, 0, 0),
    }
}

// ====================================================================
//  Patterns
// ====================================================================

fn fmt_pattern(p: &Pattern, out: &mut String) {
    match p {
        Pattern::Wildcard(_) => out.push('_'),
        Pattern::Bind(n, _) => out.push_str(n),
        Pattern::Lit(e, _) => fmt_expr_at(e, out, 0, 0),
        Pattern::Constructor { name, args, .. } => {
            out.push_str(name);
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_pattern(a, out);
            }
            out.push(')');
        }
        Pattern::RecordCtor {
            name, fields, rest, ..
        } => {
            out.push_str(name);
            out.push_str(" {");
            let mut sep_needed = false;
            for f in fields {
                if sep_needed {
                    out.push(',');
                }
                out.push(' ');
                fmt_record_pat_field(f, out);
                sep_needed = true;
            }
            if *rest {
                if sep_needed {
                    out.push(',');
                }
                out.push_str(" ..");
                sep_needed = true;
            }
            if sep_needed {
                out.push(' ');
            }
            out.push('}');
        }
        Pattern::Tuple { elems, .. } => {
            out.push('(');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_pattern(e, out);
            }
            out.push(')');
        }
        Pattern::List { elems, .. } => {
            out.push('[');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                match e {
                    ListPatElem::Pat(p) => fmt_pattern(p, out),
                    ListPatElem::Rest(None) => out.push_str(".."),
                    ListPatElem::Rest(Some(n)) => {
                        out.push_str("..");
                        out.push_str(n);
                    }
                }
            }
            out.push(']');
        }
    }
}

fn fmt_record_pat_field(f: &RecordPatField, out: &mut String) {
    out.push_str(&f.name);
    if let Some(p) = &f.pat {
        out.push_str(": ");
        fmt_pattern(p, out);
    }
}

// ====================================================================
//  Types
// ====================================================================

fn fmt_type(t: &Type, out: &mut String) {
    match t {
        Type::Named { name, .. } => out.push_str(name),
        Type::Generic { name, args, .. } => {
            out.push_str(name);
            out.push('<');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_type(a, out);
            }
            out.push('>');
        }
        Type::Model { name, version, .. } => {
            out.push_str(name);
            out.push_str("@v");
            out.push_str(&version.to_string());
        }
        Type::Tuple { elems, .. } => {
            out.push('(');
            for (i, e) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_type(e, out);
            }
            out.push(')');
        }
        Type::Cap { entries, star, .. } => {
            out.push_str("cap[");
            if *star {
                out.push('*');
            } else {
                fmt_cap_entries(entries, out);
            }
            out.push(']');
        }
        Type::Fn { params, ret, .. } => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_type(p, out);
            }
            out.push_str(") -> ");
            fmt_type(ret, out);
        }
    }
}

fn fmt_cap_entries(entries: &[CapEntry], out: &mut String) {
    for (i, e) in entries.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        fmt_cap_path(&e.path, out);
        if let Some(allow) = &e.allow {
            out.push_str(" @ ");
            if allow.len() == 1 {
                out.push('"');
                out.push_str(&escape_str(&allow[0]));
                out.push('"');
            } else {
                out.push('[');
                for (j, s) in allow.iter().enumerate() {
                    if j > 0 {
                        out.push_str(", ");
                    }
                    out.push('"');
                    out.push_str(&escape_str(s));
                    out.push('"');
                }
                out.push(']');
            }
        }
    }
}

fn fmt_cap_path(p: &CapPath, out: &mut String) {
    for (i, seg) in p.segments.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str(seg);
    }
}

// ====================================================================
//  Items
// ====================================================================

fn fmt_item(i: &Item, out: &mut String, source: &str, indent: usize) {
    match i {
        Item::Fn(f) => fmt_fn(f, out, source, indent),
        Item::Record(r) => fmt_record_decl(r, out, indent),
        Item::Enum(e) => fmt_enum_decl(e, out, indent),
        Item::Model(m) => fmt_model_decl(m, out, indent),
        Item::TypeAlias(t) => fmt_type_alias(t, out, indent),
        Item::Const(c) => fmt_const(c, out, source, indent),
        Item::Saga(s) => fmt_saga(s, out, indent),
        Item::Agent(a) => fmt_agent(a, out, indent),
        Item::AgentNet(n) => fmt_agent_net(n, out, indent),
        Item::Policy(p) => fmt_policy(p, out, indent),
        Item::Test(t) => fmt_test(t, out, source, indent),
        Item::Property(p) => fmt_property(p, out, source, indent),
    }
}

fn fmt_test(t: &crate::syntax::ast::TestDecl, out: &mut String, _source: &str, indent: usize) {
    push_indent(out, indent);
    out.push_str(&format!("test \"{}\" ", t.name));
    fmt_block_inline(&t.body, out);
    out.push('\n');
}

fn fmt_property(
    p: &crate::syntax::ast::PropertyDecl,
    out: &mut String,
    _source: &str,
    indent: usize,
) {
    push_indent(out, indent);
    out.push_str(&format!("property \"{}\" with (", p.name));
    for (i, param) in p.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&param.name);
        out.push_str(": ");
        fmt_type(&param.ty, out);
    }
    out.push_str(") ");
    fmt_block_inline(&p.body, out);
    out.push('\n');
}

fn fmt_use(u: &UseDecl, out: &mut String, source: &str) {
    out.push_str(slice(source, u.raw));
}

fn fmt_visibility(v: Visibility, out: &mut String) {
    if matches!(v, Visibility::Public) {
        out.push_str("pub ");
    }
}

fn fmt_fn(f: &FnDecl, out: &mut String, _source: &str, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(f.vis, out);
    out.push_str("fn ");
    out.push_str(&f.name);
    fmt_generics(&f.generics, out);
    out.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        fmt_param(p, out);
    }
    out.push(')');
    if let Some(t) = &f.return_ty {
        out.push_str(" -> ");
        fmt_type(t, out);
    }
    for r in &f.requires {
        out.push_str(" requires: ");
        fmt_expr_at(r, out, 0, 0);
    }
    for e in &f.ensures {
        out.push_str(" ensures: ");
        fmt_expr_at(e, out, 0, 0);
    }
    out.push(' ');
    fmt_block_inline(&f.body, out);
}

fn fmt_param(p: &Param, out: &mut String) {
    out.push_str(&p.name);
    out.push_str(": ");
    fmt_type(&p.ty, out);
}

fn fmt_generics(g: &[String], out: &mut String) {
    if g.is_empty() {
        return;
    }
    out.push('<');
    for (i, n) in g.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(n);
    }
    out.push('>');
}

fn fmt_record_decl(r: &RecordDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(r.vis, out);
    out.push_str("record ");
    out.push_str(&r.name);
    fmt_generics(&r.generics, out);
    out.push_str(" {");
    fmt_record_fields(&r.fields, out, indent);
    if r.fields.is_empty() {
        out.push('}');
    } else {
        out.push('\n');
        push_indent(out, indent);
        out.push('}');
    }
}

fn fmt_record_fields(fs: &[RecordField], out: &mut String, indent: usize) {
    for f in fs {
        out.push('\n');
        push_indent(out, indent + 1);
        out.push_str(&f.name);
        out.push_str(": ");
        fmt_type(&f.ty, out);
        if let Some(w) = &f.where_clause {
            out.push_str(" where ");
            fmt_expr_at(w, out, 0, 0);
        }
    }
}

fn fmt_enum_decl(e: &EnumDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(e.vis, out);
    out.push_str("enum ");
    out.push_str(&e.name);
    fmt_generics(&e.generics, out);
    out.push_str(" {");
    for v in &e.variants {
        out.push('\n');
        push_indent(out, indent + 1);
        fmt_enum_variant(v, out, indent + 1);
        out.push(',');
    }
    if e.variants.is_empty() {
        out.push('}');
    } else {
        out.push('\n');
        push_indent(out, indent);
        out.push('}');
    }
}

fn fmt_enum_variant(v: &EnumVariant, out: &mut String, indent: usize) {
    out.push_str(&v.name);
    match &v.data {
        VariantData::Unit => {}
        VariantData::Tuple(elems) => {
            out.push('(');
            for (i, t) in elems.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                fmt_type(t, out);
            }
            out.push(')');
        }
        VariantData::Record(fields) => {
            out.push_str(" {");
            fmt_record_fields(fields, out, indent);
            if fields.is_empty() {
                out.push('}');
            } else {
                out.push('\n');
                push_indent(out, indent);
                out.push('}');
            }
        }
    }
}

fn fmt_model_decl(m: &ModelDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(m.vis, out);
    out.push_str("model ");
    out.push_str(&m.name);
    out.push_str("@v");
    out.push_str(&m.version.to_string());
    out.push_str(" {");
    fmt_record_fields(&m.fields, out, indent);
    for w in &m.record_where {
        out.push('\n');
        push_indent(out, indent + 1);
        out.push_str("where: ");
        fmt_expr_at(w, out, 0, 0);
    }
    if m.fields.is_empty() && m.record_where.is_empty() {
        out.push('}');
    } else {
        out.push('\n');
        push_indent(out, indent);
        out.push('}');
    }
}

fn fmt_type_alias(t: &TypeAliasDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(t.vis, out);
    out.push_str("type ");
    out.push_str(&t.name);
    fmt_generics(&t.generics, out);
    out.push_str(" = ");
    fmt_type(&t.aliased, out);
}

fn fmt_const(c: &ConstDecl, out: &mut String, source: &str, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(c.vis, out);
    out.push_str("const ");
    out.push_str(&c.name);
    if let Some(t) = &c.ty {
        out.push_str(": ");
        fmt_type(t, out);
    }
    out.push_str(" = ");
    out.push_str(slice(source, c.init).trim());
}

fn fmt_saga(s: &SagaDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(s.vis, out);
    out.push_str("saga ");
    out.push_str(&s.name);
    out.push('(');
    for (i, p) in s.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        fmt_param(p, out);
    }
    out.push_str(") {\n");
    push_indent(out, indent + 1);
    out.push_str("intent \"");
    out.push_str(&escape_str(&s.intent));
    out.push('"');
    for st in &s.steps {
        out.push('\n');
        fmt_saga_step(st, out, indent + 1);
    }
    out.push('\n');
    push_indent(out, indent);
    out.push('}');
}

fn fmt_saga_step(s: &SagaStep, out: &mut String, indent: usize) {
    push_indent(out, indent);
    out.push_str("step ");
    out.push_str(&s.name);
    out.push_str(" {");
    for r in &s.requires {
        out.push('\n');
        push_indent(out, indent + 1);
        out.push_str("requires: ");
        fmt_expr_at(r, out, 0, 0);
    }
    out.push('\n');
    push_indent(out, indent + 1);
    out.push_str("do ");
    fmt_block_inline(&s.do_block, out);
    out.push('\n');
    push_indent(out, indent + 1);
    out.push_str("undo ");
    match &s.undo {
        UndoForm::Block(b) => fmt_block_inline(b, out),
        UndoForm::Noop(_) => out.push_str("noop"),
    }
    out.push('\n');
    push_indent(out, indent);
    out.push('}');
}

fn fmt_agent(a: &AgentDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(a.vis, out);
    out.push_str("agent ");
    out.push_str(&a.name);
    out.push_str(" {");
    for f in &a.fields {
        out.push('\n');
        push_indent(out, indent + 1);
        fmt_decl_field(f, out);
    }
    if a.fields.is_empty() {
        out.push('}');
    } else {
        out.push('\n');
        push_indent(out, indent);
        out.push('}');
    }
}

fn fmt_decl_field(f: &DeclField, out: &mut String) {
    out.push_str(&f.key);
    out.push_str(": ");
    for (i, v) in f.values.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        fmt_expr_at(v, out, 0, 0);
    }
}

fn fmt_agent_net(n: &AgentNetDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(n.vis, out);
    out.push_str("agent_net ");
    out.push_str(&n.name);
    out.push_str(" {");
    let mut wrote = false;
    if let Some(s) = &n.intent {
        out.push('\n');
        push_indent(out, indent + 1);
        out.push_str("intent \"");
        out.push_str(&escape_str(s));
        out.push('"');
        wrote = true;
    }
    for f in &n.flows {
        out.push('\n');
        push_indent(out, indent + 1);
        out.push_str("flow ");
        fmt_flow(f, out);
        wrote = true;
    }
    if let Some(u) = &n.until {
        out.push('\n');
        push_indent(out, indent + 1);
        out.push_str("until: ");
        fmt_expr_at(u, out, 0, 0);
        wrote = true;
    }
    if !wrote {
        out.push('}');
    } else {
        out.push('\n');
        push_indent(out, indent);
        out.push('}');
    }
}

fn fmt_flow(f: &FlowDecl, out: &mut String) {
    for (i, st) in f.stages.iter().enumerate() {
        if i > 0 {
            out.push_str(" -> ");
        }
        match st {
            FlowStage::Single(n) => out.push_str(n),
            FlowStage::FanOut(names) => {
                out.push('{');
                for (j, n) in names.iter().enumerate() {
                    if j > 0 {
                        out.push_str(", ");
                    } else {
                        out.push(' ');
                    }
                    out.push_str(n);
                }
                out.push_str(" }");
            }
        }
    }
}

fn fmt_policy(p: &PolicyDecl, out: &mut String, indent: usize) {
    push_indent(out, indent);
    fmt_visibility(p.vis, out);
    out.push_str("policy ");
    out.push_str(&p.name);
    out.push_str(" {");
    for f in &p.fields {
        out.push('\n');
        push_indent(out, indent + 1);
        fmt_decl_field(f, out);
    }
    if p.fields.is_empty() {
        out.push('}');
    } else {
        out.push('\n');
        push_indent(out, indent);
        out.push('}');
    }
}

// ====================================================================
//  Helpers
// ====================================================================

fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push_str("    ");
    }
}

fn slice(source: &str, r: RawSpan) -> &str {
    let s = r.span.start as usize;
    let e = r.span.end as usize;
    let s = s.min(source.len());
    let e = e.min(source.len()).max(s);
    &source[s..e]
}

fn escape_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

fn format_float(f: f64) -> String {
    if f == f.trunc() && f.is_finite() && f.abs() < 1e16 {
        format!("{f:.1}")
    } else {
        format!("{f}")
    }
}

// ====================================================================
//  Tests — M1.T10 acceptance: 100 fixtures round-trip idempotently.
// ====================================================================

#[cfg(test)]
mod tests {
    use super::super::parse_expression;
    use super::*;

    /// `format(parse(format(parse(s)))) == format(parse(s))` for every
    /// fixture (§ 25.2 "`aeris fmt` is total"). Equality is checked at
    /// the **string** level — AST spans depend on input bytes (e.g.
    /// `1_000` vs `1000`), so post-format spans differ even when the
    /// semantics are unchanged.
    fn roundtrip(src: &str) {
        let e1 = parse_expression(src)
            .unwrap_or_else(|err| panic!("parse_expression({src:?}) failed: {err:?}"));
        let s1 = format_expression(&e1);
        let e2 =
            parse_expression(&s1).unwrap_or_else(|err| panic!("re-parse({s1:?}) failed: {err:?}"));
        let s2 = format_expression(&e2);
        assert_eq!(s1, s2, "fmt not idempotent for {src:?}");
    }

    /// Fixture catalogue (≥ 100 distinct expressions). Each is compact;
    /// the assertion is structural (formatter idempotent on each).
    const FIXTURES: &[&str] = &[
        // literals (10)
        "0",
        "42",
        "1_000",
        "0xff",
        "0b1010",
        "true",
        "false",
        "\"hello\"",
        "'a'",
        "()",
        // numeric edge cases (5)
        "1 + 2",
        "1 + 2 * 3",
        "(1 + 2) * 3",
        "2 + 3 + 4",
        "10 - 4 - 2",
        // arithmetic with mixed precedence (10)
        "a + b * c",
        "a * b + c",
        "a / b % c",
        "a + b - c + d",
        "-a + b",
        "a * (b + c)",
        "(a + b) * (c + d)",
        "a + (b - c) * d",
        "a - -b",
        "not flag",
        // comparison / logic (10)
        "a == b",
        "a != b",
        "a < b",
        "a <= b",
        "a > b",
        "a >= b",
        "x < y and y < z",
        "a or b and c",
        "(a or b) and c",
        "not (a == b)",
        // bitwise / shift (5)
        "a & b",
        "a | b",
        "a ^ b",
        "a << 2",
        "n >> 1",
        // postfix chains (10)
        "x.y",
        "a.b.c.d",
        "f()",
        "f(1, 2)",
        "f(name: \"ada\")",
        "xs[0]",
        "xs[i + 1]",
        "xs.map(f)",
        "parsed.map(g)?",
        "f()?.g(2)",
        // is / as / try / index (5)
        "r is Ok(v)",
        "n as i64",
        "f()?",
        "(a + b) as decimal",
        "xs[0].name",
        // ranges (4)
        "0..10",
        "0..=n",
        "..n",
        "lo..=hi",
        // collections (8)
        "[]",
        "[1]",
        "[1, 2, 3]",
        "(1, 2)",
        "(1, 2, 3)",
        "{ a: 1 }",
        "{ a: 1, b: 2 }",
        "User { id: 1, name: \"x\" }",
        // record spread (3)
        "User { ..u, age: 37 }",
        "{ ..base, x: 1 }",
        "{ a: 1, b: 2, c: 3 }",
        // if / match / block (8)
        "if x > 0 { 1 } else { -1 }",
        "if a { 1 } else if b { 2 } else { 3 }",
        "match s { Pending -> 1, _ -> 0 }",
        "match s { Active(t) -> t, _ -> 0 }",
        "match xs { [] -> 0, [x] -> 1, [x, ..rest] -> 2 }",
        "match n { x if x > 0 -> 1, _ -> 0 }",
        "{ let x = 1; x + 1 }",
        "{ let x = 1; let y = 2; x + y }",
        // lambda / spawn / await (6)
        "fn(x: int) -> int { x + 1 }",
        "fn(x) { x }",
        "fn(a, b) { a + b }",
        "spawn { compute(cap) }",
        "await h",
        "await h.field",
        // raise / return / break / continue (6)
        "raise err.user(\"bad\")",
        "raise e",
        "return x",
        "return",
        "break",
        "continue",
        // intent block (2)
        "intent \"x\" { f() }",
        "intent \"rotate cert\" { audit() }",
        // assignment (5)
        "x = 1",
        "x += 1",
        "x -= 2",
        "x *= 3",
        "x %= 4",
        // ModelRef (2)
        "Invoice@v1",
        "Order@v42",
        // cap.subset (3)
        "cap.subset[fs.read_file]",
        "cap.subset[http.post @ \"api.acme.com\"]",
        "cap.test_subset[audit.event]",
        // duration / date / timestamp (3)
        "3s",
        "2026-05-07",
        "2026-05-07T08:30:00Z",
        // higher-order (3)
        "items.fold(0, fn(acc, it) { acc + it.amount })",
        "xs.map(fn(x) { x + 1 })",
        "xs.filter(fn(x) { x > 0 }).map(fn(x) { x * 2 })",
        // chained postfix (4)
        "f(x).g(y).h(z)",
        "obj.method(a, b)",
        "lookup(k)?.value",
        "config.servers[0].host",
    ];

    #[test]
    fn fixture_count_is_at_least_100() {
        assert!(FIXTURES.len() >= 100, "got {}", FIXTURES.len());
    }

    #[test]
    fn all_fixtures_round_trip() {
        for src in FIXTURES {
            roundtrip(src);
        }
    }

    // Sanity checks for individual high-risk constructs.

    #[test]
    fn precedence_is_preserved_without_extra_parens() {
        let s = format_expression(&parse_expression("1 + 2 * 3").unwrap());
        assert_eq!(s, "1 + 2 * 3");
        let s = format_expression(&parse_expression("(1 + 2) * 3").unwrap());
        assert_eq!(s, "(1 + 2) * 3");
        let s = format_expression(&parse_expression("a or b and c").unwrap());
        assert_eq!(s, "a or b and c");
    }

    #[test]
    fn left_assoc_does_not_drop_parens() {
        // `a + (b + c)` re-formats with explicit parens because all our
        // binops are left-assoc.
        let s = format_expression(&parse_expression("a + (b + c)").unwrap());
        assert_eq!(s, "a + (b + c)");
    }

    // ----------------- M12.T5: 200 module-level fmt idempotency -----------------

    fn module_idempotent(src: &str) {
        let m1 = match crate::syntax::parse(src) {
            Ok(m) => m,
            Err(e) => panic!("first parse failed for {src:?}: {e:?}"),
        };
        let s1 = format_module(&m1, src);
        let m2 = match crate::syntax::parse(&s1) {
            Ok(m) => m,
            Err(e) => panic!("re-parse failed for {s1:?} (from {src:?}): {e:?}"),
        };
        let s2 = format_module(&m2, &s1);
        assert_eq!(s1, s2, "fmt not idempotent for {src:?}\nS1=\n{s1}\nS2=\n{s2}");
    }

    /// 200 module-level fixtures. Each must satisfy `fmt(fmt(x)) ==
    /// fmt(x)`. Categories cover every top-level item the parser
    /// supports, plus mixed-decl modules and edge-case formatting.
    const MODULE_FIXTURES: &[&str] = &[
        // ---- records (20) ----
        "record R { x: int }",
        "record R { x: int, y: int }",
        "record User { id: uuid, name: string }",
        "record AllPrim { a: bool, b: int, c: f64, d: string }",
        "record Wrapper<T> { value: T }",
        "record Pair<A, B> { l: A, r: B }",
        "record Box<T> { items: list<T> }",
        "record Cache<K, V> { kv: map<K, V> }",
        "record Empty {}",
        "record Tuples { p: (int, string), q: (int, int, int) }",
        "record OptIn { v: option<int> }",
        "record Result1 { v: result<int> }",
        "record Listy { xs: list<int> }",
        "record Nested { xs: list<list<int>> }",
        "record Setty { tags: set<string> }",
        "record Mappy { kv: map<string, int> }",
        "record FnField { f: fn(int) -> int }",
        "record TupleField { p: (int, string) }",
        "record UnitField { z: () }",
        "pub record User { id: uuid }",
        // ---- enums (15) ----
        "enum Color { Red, Green, Blue }",
        "enum E { A, B(int), C(string, int) }",
        "enum E { A, B { x: int, y: int } }",
        "enum Either<L, R> { Left(L), Right(R) }",
        "enum Status { Pending, Active, Closed }",
        "enum Tree<T> { Leaf, Node(Tree<T>, T, Tree<T>) }",
        "enum E { A }",
        "enum E { Single(int) }",
        "enum Mix { A, B(int), C { x: int } }",
        "pub enum Color { R, G, B }",
        "enum O<T> { None, Some(T) }",
        "enum Tag { OK, ERR }",
        "enum One { Solo }",
        "enum E { Recursive(list<int>) }",
        "enum Many { A, B, C, D, E, F }",
        // ---- models (10) ----
        "model M@v1 { id: uuid }",
        "model Doc@v42 { text: string }",
        "model Order@v1 { lines: list<int> }",
        "model Invoice@v1 { id: uuid, total: decimal }",
        "model User@v1 { id: uuid, name: string }",
        "model Order@v2 { id: uuid, lines: list<int>, total: decimal }",
        "pub model M@v1 { id: uuid }",
        "model Empty@v1 {}",
        "model A@v1 { x: int } model A@v2 { x: int, y: int }",
        "model Big@v3 { a: int, b: int, c: int, d: int, e: int }",
        // ---- type aliases (10) ----
        "type Email = string",
        "type Ids = list<uuid>",
        "type Pair<A, B> = (A, B)",
        "type IntList = list<int>",
        "type StrMap = map<string, string>",
        "type Maybe<T> = option<T>",
        "type Outcome<T> = result<T>",
        "type Func = fn(int) -> int",
        "pub type Email = string",
        "type Box2<T> = list<list<T>>",
        // ---- consts (10) ----
        "const PI: decimal = 3.14",
        "const MAX: int = 100",
        "const NAME: string = \"aeris\"",
        "const ENABLED: bool = true",
        "const ZERO: int = 0",
        "const NEG: int = -5",
        "const HEX: int = 0xff",
        "const BIN: int = 0b1010",
        "pub const GREETING: string = \"hello\"",
        "const D: duration = 3s",
        // ---- fn signatures (30) ----
        "fn add(a: int, b: int) -> int { a + b }",
        "fn id<T>(x: T) -> T { x }",
        "fn first<T>(xs: list<T>) -> option<T> { None }",
        "fn map<T, U>(xs: list<T>, f: fn(T) -> U) -> list<U> { [] }",
        "fn doit(x: int) {}",
        "fn pure() {}",
        "fn many(a: int, b: int, c: int) -> int { a + b + c }",
        "fn ret_unit() -> () { () }",
        "fn ret_tuple() -> (int, int) { (1, 2) }",
        "fn ret_list() -> list<int> { [1, 2, 3] }",
        "fn ret_opt() -> option<int> { Some(1) }",
        "fn ret_res() -> result<int> { Ok(1) }",
        "fn cap_param(cap: cap[fs.read_file]) {}",
        "fn cap_param2(cap: cap[fs.read_file, audit.event]) {}",
        "fn cap_alw(cap: cap[http.post @ \"api.acme.com\"]) {}",
        "fn cap_alw2(cap: cap[http.post @ [\"api.acme.com\", \"api.stripe.com\"]]) {}",
        "fn nested(cap: cap[fs.read_file], n: int) -> int { n }",
        "pub fn add(a: int, b: int) -> int { a + b }",
        "fn f(x: int) -> int { x }",
        "fn g(x: int, y: int) -> int { x + y }",
        "fn double(x: int) -> int { x * 2 }",
        "fn neg(x: int) -> int { -x }",
        "fn flag(b: bool) -> bool { not b }",
        "fn empty() -> string { \"\" }",
        "fn dash(s: string) -> string { s }",
        "fn lst(xs: list<int>) -> int { 0 }",
        "fn mp(m: map<string, int>) -> int { 0 }",
        "fn pair(p: (int, int)) -> int { 0 }",
        "fn opt(x: option<int>) -> int { 0 }",
        "fn rs(x: result<int>) -> int { 0 }",
        // ---- fn bodies (30) ----
        "fn f() { let x = 1 }",
        "fn f() { let x = 1; let y = 2 }",
        "fn f() { let x = 1; x }",
        "fn f() { var x = 1; x = 2 }",
        "fn f() -> int { 1 + 2 }",
        "fn f() -> int { if true { 1 } else { 0 } }",
        "fn f(x: int) -> int { match x { 0 -> 0, _ -> 1 } }",
        "fn f() { for i in 0..10 { } }",
        "fn f() { while false { } }",
        "fn f() -> int { (1 + 2) * 3 }",
        "fn f(xs: list<int>) -> int { xs[0] }",
        "fn f() { let f2 = fn(x: int) -> int { x + 1 } }",
        "fn f() -> int { return 1 }",
        "fn f() -> int { 1 }",
        "fn f() { intent \"x\" { } }",
        "fn f(cap: cap[io.println]) { intent \"x\" { io.println(\"hi\") } }",
        "fn f() -> result<int> { Ok(1) }",
        "fn f() -> result<int> { Err(\"x\") }",
        "fn f() -> option<int> { None }",
        "fn f() -> option<int> { Some(1) }",
        "fn f() -> int { match true { true -> 1, false -> 0 } }",
        "fn f(x: int) -> int { match x { n if n > 0 -> 1, _ -> 0 } }",
        "fn f() -> int { let xs = [1, 2, 3]; xs[0] }",
        "fn f() -> int { let p = (1, 2); 0 }",
        "fn f() -> int { let r = User { id: 1, name: \"x\" }; 0 }",
        "fn f() -> int { let mp = { a: 1, b: 2 }; 0 }",
        "fn f() -> bool { 1 == 1 }",
        "fn f() -> bool { 1 != 2 }",
        "fn f() -> bool { 1 < 2 and 2 < 3 }",
        "fn f() -> bool { 1 < 2 or 3 < 4 }",
        // ---- contracts (10) ----
        "fn pos(x: int) -> int requires: x > 0 ensures: result > 0 { x }",
        "fn nn(x: int) -> int requires: x >= 0 { x }",
        "fn p(x: int) -> int ensures: result == x { x }",
        "fn p2(x: int) -> int requires: x > 0 ensures: result > 0 ensures: result == x { x }",
        "fn r(x: int) requires: x > 0 { }",
        "fn small(x: int) -> int requires: x < 10 { x }",
        "fn even(x: int) -> int requires: x % 2 == 0 { x }",
        "fn rng(x: int) -> int requires: x >= 0 requires: x <= 100 { x }",
        "fn dbl(x: int) -> int ensures: result == x + x { x + x }",
        "fn id(x: int) -> int ensures: result == x { x }",
        // ---- saga (10) ----
        "saga s(cap: cap[http.post]) { intent \"x\" step a { do { } undo noop } }",
        "saga charge(cap: cap[http.post @ \"api.acme.com\"]) { intent \"charge\" step pay { do { } undo noop } }",
        "saga two(cap: cap[http.post]) { intent \"x\" step a { do { } undo noop } step b { do { } undo noop } }",
        "saga full(cap: cap[http.post]) { intent \"full\" step a { do { } undo { } } }",
        "saga inv(cap: cap[fs.read_file]) { intent \"read\" step r { do { } undo noop } }",
        "saga noisy(cap: cap[io.println]) { intent \"noise\" step say { do { } undo noop } }",
        "pub saga p(cap: cap[http.post]) { intent \"x\" step a { do { } undo noop } }",
        "saga audit_only(cap: cap[audit.event]) { intent \"x\" step a { do { } undo { } } }",
        "saga peek(cap: cap[fs.read_file]) { intent \"peek\" step look { do { } undo noop } }",
        "saga nested(cap: cap[fs.read_file]) { intent \"nested\" step a { do { } undo noop } step b { do { } undo noop } step c { do { } undo noop } }",
        // ---- agent / agent_net / policy / test / property (15) ----
        "agent a { llm: \"x\" intent: \"x\" prompt: \"p\" accept: inv produce: cat }",
        "agent_net p { flow a -> b }",
        "agent_net p { flow a -> b -> c }",
        "agent_net p { flow a -> { b, c } }",
        "agent_net p { flow a -> b flow b -> c }",
        "agent_net p { flow a -> b until: a }",
        "policy p { match: http.post }",
        "policy p { match: http.* deny: true }",
        "policy p { match: fs.* require: false }",
        "test \"trivial\" { let x = 1 }",
        "test \"with assert\" { assert(true) }",
        "test \"with fixture\" with fixture: \"f1\" { assert(true) }",
        "property \"pure\" with (a: int) { assert(a == a) }",
        "property \"two\" with (a: int, b: int) { assert(a + b == b + a) }",
        "property \"bool\" with (b: bool) { assert(b == b) }",
        // ---- mixed multi-item modules (20) ----
        "record R { x: int } record S { y: int }",
        "record R { x: int } enum E { A }",
        "type T = int fn f() -> T { 1 }",
        "const X: int = 1 fn f() -> int { X }",
        "model M@v1 { id: uuid } record Batch { items: list<M@v1> }",
        "fn a() {} fn b() {}",
        "record User { id: uuid } fn id() -> uuid { todo() }",
        "enum Status { A, B } record S { st: Status }",
        "type Email = string record User { e: Email }",
        "fn a() {} fn b() {} fn c() {}",
        "record R { x: int } enum E { A } type T = int",
        "const A: int = 1 const B: int = 2",
        "fn helper(x: int) -> int { x } fn run(c: cap[fs.read_file]) -> result<unit> { Ok(()) }",
        "model Doc@v1 { id: uuid } fn save(d: Doc@v1) -> result<unit> { Ok(()) }",
        "record A { x: int } record B { y: int } record C { z: int }",
        "enum E { A } enum F { B } enum G { C }",
        "type X = int type Y = string type Z = bool",
        "test \"a\" { } test \"b\" { } test \"c\" { }",
        "agent_net p { flow a -> b } fn main() {}",
        "policy p { match: http.* } fn use_p() {}",
        // ---- edge cases (20) ----
        "fn f() { let _ = 1 }",
        "fn f() { let x: int = 1 }",
        "fn f() -> int { let x: int = 1; x }",
        "fn f() { let p: (int, int) = (1, 2) }",
        "fn f(x: int, y: int, z: int) -> int { x + y + z }",
        "fn f(a: int, b: int, c: int, d: int) -> int { 0 }",
        "fn f<T>(x: T) -> T { x }",
        "fn f<T, U>(x: T, y: U) -> (T, U) { (x, y) }",
        "fn f<T, U, V>(x: T, y: U, z: V) -> int { 0 }",
        "fn f() -> int { 1 + 2 + 3 + 4 + 5 }",
        "fn f() -> bool { 1 < 2 < 3 }",
        "fn f() { if true { } }",
        "fn f() { if true { } else { } }",
        "fn f() { if true { } else if false { } else { } }",
        "fn f(x: int) -> int { match x { 0 -> 0, 1 -> 1, _ -> 2 } }",
        "fn f(x: int) -> int { match x { n -> n } }",
        "fn f(xs: list<int>) -> int { match xs { [] -> 0, [x] -> x, _ -> 0 } }",
        "fn f() { let _ = [1, 2, 3, 4, 5] }",
        "fn f() { let _ = (1, 2, 3) }",
        "fn f() { let r = User { id: 1, name: \"x\", age: 10 } }",
    ];

    #[test]
    fn module_fixture_count_is_at_least_200() {
        assert!(
            MODULE_FIXTURES.len() >= 200,
            "got {} module fixtures",
            MODULE_FIXTURES.len()
        );
    }

    #[test]
    fn all_module_fixtures_are_idempotent() {
        for src in MODULE_FIXTURES {
            module_idempotent(src);
        }
    }

    #[test]
    fn block_inline_form_uses_semicolons() {
        let s = format_expression(&parse_expression("{ let x = 1; x + 1 }").unwrap());
        assert_eq!(s, "{ let x = 1; x + 1 }");
    }
}
