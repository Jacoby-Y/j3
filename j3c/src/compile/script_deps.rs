//! Find `@script` edges in the parsed module graph.

use crate::parse::{Block, Expr, ExprKind, File, InterpPart, Item, Stmt};
use crate::span::Span;

pub struct ScriptRef {
    pub path: String,
    pub span: Span,
}

pub fn find(file: &File) -> Vec<ScriptRef> {
    let mut out = Vec::new();
    for item in &file.items {
        match item {
            Item::Const(c) => expr(&c.value, &mut out),
            Item::Fn(f) => block(&f.body, &mut out),
            Item::Struct(s) => {
                for field in &s.fields {
                    if let Some(value) = &field.default {
                        expr(value, &mut out);
                    }
                }
                for method in &s.methods {
                    block(&method.body, &mut out);
                }
            }
            Item::Let(l) => {
                if let Some(value) = &l.value {
                    expr(value, &mut out);
                }
            }
            Item::Props(b) | Item::Shared(b) => {
                for field in &b.fields {
                    if let Some(value) = &field.default {
                        expr(value, &mut out);
                    }
                }
            }
            Item::State(s) => expr(&s.value, &mut out),
            Item::Use(_) | Item::UseLib(_) | Item::Enum(_) => {}
        }
    }
    out
}

fn block(b: &Block, out: &mut Vec<ScriptRef>) {
    for stmt in &b.stmts {
        match stmt {
            Stmt::Let(s) => expr(&s.value, out),
            Stmt::Return(s) => {
                if let Some(e) = &s.value {
                    expr(e, out);
                }
            }
            Stmt::Assign(s) => {
                expr(&s.place, out);
                expr(&s.value, out);
            }
            Stmt::AssignOp(s) => {
                expr(&s.place, out);
                expr(&s.value, out);
            }
            Stmt::Expr(e) => expr(e, out),
            Stmt::If(s) => {
                expr(&s.cond, out);
                block(&s.then_block, out);
                if let Some(b) = &s.else_block {
                    block(b, out);
                }
            }
            Stmt::While(s) => {
                expr(&s.cond, out);
                block(&s.body, out);
            }
            Stmt::For(s) => {
                expr(&s.start, out);
                expr(&s.end, out);
                block(&s.body, out);
            }
            Stmt::ForEach(s) => {
                expr(&s.iter, out);
                block(&s.body, out);
            }
            Stmt::Match(s) => {
                expr(&s.expr, out);
                for arm in &s.arms {
                    block(&arm.body, out);
                }
            }
            Stmt::Defer(s) => block(&s.body, out),
            Stmt::Block(b) => block(b, out),
            Stmt::UseLocal(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
        }
    }
}

fn expr(e: &Expr, out: &mut Vec<ScriptRef>) {
    match &e.kind {
        ExprKind::ScriptType(path) => out.push(ScriptRef {
            path: path.clone(),
            span: e.span,
        }),
        ExprKind::ScriptLit { path, fields } => {
            out.push(ScriptRef {
                path: path.clone(),
                span: e.span,
            });
            for (_, value) in fields {
                expr(value, out);
            }
        }
        ExprKind::StructLit { fields, .. } | ExprKind::SliceLit { fields, .. } => {
            for (_, value) in fields {
                expr(value, out);
            }
        }
        ExprKind::ArrayLit { elems } | ExprKind::Syscall { args: elems } => {
            for value in elems {
                expr(value, out);
            }
        }
        ExprKind::Index { base, index } => {
            expr(base, out);
            expr(index, out);
        }
        ExprKind::Slice { base, start, end } => {
            expr(base, out);
            if let Some(e) = start {
                expr(e, out);
            }
            if let Some(e) = end {
                expr(e, out);
            }
        }
        ExprKind::Field { base, .. }
        | ExprKind::Cast { expr: base, .. }
        | ExprKind::Unary { expr: base, .. }
        | ExprKind::Question { expr: base } => expr(base, out),
        ExprKind::Call { func, args, .. } => {
            expr(func, out);
            for arg in args {
                expr(arg, out);
            }
        }
        ExprKind::SliceFrom { ptr, len } => {
            expr(ptr, out);
            expr(len, out);
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            expr(lhs, out);
            expr(rhs, out);
        }
        ExprKind::Interpolate { parts } => {
            for part in parts {
                if let InterpPart::Expr(e) = part {
                    expr(e, out);
                }
            }
        }
        ExprKind::If {
            cond,
            then_expr,
            else_expr,
        } => {
            expr(cond, out);
            expr(then_expr, out);
            expr(else_expr, out);
        }
        ExprKind::Name(_)
        | ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::TypePath { .. }
        | ExprKind::SizeOf(_) => {}
    }
}
