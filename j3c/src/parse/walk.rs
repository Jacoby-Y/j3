//! Exhaustive walk over the names a block mentions. No `_` arms: a new AST node must be added here
//! (closure capture analysis depends on seeing every name).

use super::ast::*;

/// Calls `f` for every identifier used as a value or `use`d local anywhere in `block`,
/// closure literals nested inside included.
pub fn names_in_block(block: &Block, f: &mut dyn FnMut(&Ident)) {
    for s in &block.stmts {
        names_in_stmt(s, f);
    }
}

fn names_in_stmt(stmt: &Stmt, f: &mut dyn FnMut(&Ident)) {
    match stmt {
        Stmt::Let(s) => names_in_expr(&s.value, f),
        Stmt::Return(s) => {
            if let Some(v) = &s.value {
                names_in_expr(v, f);
            }
        }
        Stmt::Assign(s) => {
            names_in_expr(&s.place, f);
            names_in_expr(&s.value, f);
        }
        Stmt::AssignOp(s) => {
            names_in_expr(&s.place, f);
            names_in_expr(&s.value, f);
        }
        Stmt::Expr(e) => names_in_expr(e, f),
        Stmt::If(s) => {
            names_in_expr(&s.cond, f);
            names_in_block(&s.then_block, f);
            if let Some(b) = &s.else_block {
                names_in_block(b, f);
            }
        }
        Stmt::While(s) => {
            names_in_expr(&s.cond, f);
            names_in_block(&s.body, f);
        }
        Stmt::For(s) => {
            names_in_expr(&s.start, f);
            names_in_expr(&s.end, f);
            names_in_block(&s.body, f);
        }
        Stmt::ForEach(s) => {
            names_in_expr(&s.iter, f);
            names_in_block(&s.body, f);
        }
        Stmt::UseLocal(s) => f(&s.name),
        Stmt::Match(s) => {
            names_in_expr(&s.expr, f);
            for arm in &s.arms {
                if let Some(v) = &arm.value {
                    names_in_expr(v, f);
                }
                names_in_block(&arm.body, f);
            }
        }
        Stmt::Defer(s) => names_in_block(&s.body, f),
        Stmt::Labeled(s) => names_in_stmt(&s.body, f),
        Stmt::Break(..) | Stmt::Continue(..) => {}
        Stmt::Block(b) => names_in_block(b, f),
    }
}

fn names_in_expr(expr: &Expr, f: &mut dyn FnMut(&Ident)) {
    match &expr.kind {
        ExprKind::Name(n) => f(n),
        ExprKind::Int(_)
        | ExprKind::Float(_)
        | ExprKind::Bool(_)
        | ExprKind::String(_)
        | ExprKind::TypePath { .. }
        | ExprKind::ScriptType(_)
        | ExprKind::SizeOf(_) => {}
        ExprKind::StructLit { fields, .. }
        | ExprKind::SliceLit { fields, .. }
        | ExprKind::ScriptLit { fields, .. } => {
            for (_, e) in fields {
                names_in_expr(e, f);
            }
        }
        ExprKind::ArrayLit { elems } => elems.iter().for_each(|e| names_in_expr(e, f)),
        ExprKind::Index { base, index } => {
            names_in_expr(base, f);
            names_in_expr(index, f);
        }
        ExprKind::Slice { base, start, end } => {
            names_in_expr(base, f);
            for e in [start, end].into_iter().flatten() {
                names_in_expr(e, f);
            }
        }
        ExprKind::Field { base, .. } => names_in_expr(base, f),
        ExprKind::Call { func, args, .. } => {
            names_in_expr(func, f);
            args.iter().for_each(|e| names_in_expr(e, f));
        }
        ExprKind::Syscall { args } | ExprKind::Atomic { args, .. } => {
            args.iter().for_each(|e| names_in_expr(e, f))
        }
        ExprKind::UiChoice { value, index, .. } => {
            names_in_expr(value, f);
            names_in_expr(index, f);
        }
        ExprKind::SliceFrom { ptr, len } => {
            names_in_expr(ptr, f);
            names_in_expr(len, f);
        }
        ExprKind::Cast { expr, .. } | ExprKind::Unary { expr, .. } | ExprKind::Question { expr } => {
            names_in_expr(expr, f)
        }
        ExprKind::Binary { lhs, rhs, .. } => {
            names_in_expr(lhs, f);
            names_in_expr(rhs, f);
        }
        ExprKind::Interpolate { parts } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    names_in_expr(e, f);
                }
            }
        }
        ExprKind::If { cond, then_expr, else_expr } => {
            names_in_expr(cond, f);
            names_in_expr(then_expr, f);
            names_in_expr(else_expr, f);
        }
        ExprKind::Closure(c) => names_in_block(&c.body, f),
    }
}
