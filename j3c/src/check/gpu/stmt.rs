//! Statements of the shader subset. What is not here is an error with the fix: `defer`, labels, `for x in slice`,
//! enum `match`, `use`.

use crate::parse::{Block, Expr, ExprKind, IfStmt, Stmt, UnaryOp};

use crate::emit::glsl::num::bare;
use super::gen::Gen;
use super::ty::*;
use super::expr::swizzle_assign_check;
use super::R;

impl<'a> Gen<'a> {
    /// A `{ .. }` of its own scope; the braces are the caller's.
    pub fn block(&mut self, b: &Block) -> R<()> {
        self.push_scope();
        self.depth += 1;
        let r = self.block_stmts(b);
        self.depth -= 1;
        self.pop_scope();
        r
    }

    pub fn block_stmts(&mut self, b: &Block) -> R<()> {
        for s in &b.stmts {
            self.stmt(s)?;
        }
        Ok(())
    }

    fn cond(&mut self, e: &Expr) -> R<String> {
        let (t, ty) = self.expr(e, None)?;
        if ty != BOOL {
            return self.err_help(e.span, format!("a condition must be bool, found {}", ty.name()), "compare it: `x != 0.0`");
        }
        Ok(bare(&t).to_string())
    }

    fn stmt(&mut self, s: &Stmt) -> R<()> {
        match s {
            Stmt::Let(l) => {
                let want = match &l.ty {
                    Some(t) => Some(self.ty_of(t, &self.cur.module.clone())?),
                    None => None,
                };
                let (text, vt) = self.expr(&l.value, want.as_ref())?;
                let ty = want.unwrap_or_else(|| vt.settle());
                if matches!(ty, GTy::Void | GTy::Sampler(_)) {
                    return self.err(l.value.span, format!("a `let` cannot hold {}", ty.name()));
                }
                let text = self.coerce((text, vt), &ty, l.value.span)?;
                let g = self.declare(&l.name.name, ty.clone());
                self.line(&format!("{} {g} = {};", ty.glsl(), bare(&text)));
            }
            Stmt::Return(r) => match (&r.value, self.cur.ret.clone()) {
                (None, GTy::Void) => self.line("return;"),
                (None, t) => return self.err(r.span, format!("this fn returns {}; give `return` a value", t.name())),
                (Some(v), GTy::Void) => return self.err(v.span, "this fn returns nothing"),
                (Some(v), t) => {
                    let (text, vt) = self.expr(v, Some(&t))?;
                    let text = self.coerce((text, vt), &t, v.span)?;
                    self.line(&format!("return {};", bare(&text)));
                }
            },
            Stmt::Assign(a) => {
                let (pt, pty) = self.place(&a.place)?;
                let (vt, vty) = self.expr(&a.value, Some(&pty))?;
                let v = self.coerce((vt, vty), &pty, a.value.span)?;
                self.line(&format!("{pt} = {};", bare(&v)));
            }
            Stmt::AssignOp(a) => {
                let (pt, pty) = self.place(&a.place)?;
                let r = self.expr(&a.value, None)?;
                let (_, rt, res) = self.arith(a.op, (pt.clone(), pty.clone()), r, a.span)?;
                if res != pty {
                    return self.err(a.span, format!("`{}=` on {} would make {}", op_sym(a.op), pty.name(), res.name()));
                }
                self.line(&format!("{pt} {}= {};", op_sym(a.op), bare(&rt)));
            }
            Stmt::Expr(e) => {
                if !matches!(e.kind, ExprKind::Call { .. }) {
                    return self.err(e.span, "this expression does nothing");
                }
                let (t, _) = self.expr(e, None)?;
                self.line(&format!("{t};"));
            }
            Stmt::If(i) => self.if_stmt(i, "")?,
            Stmt::While(w) => {
                let c = self.cond(&w.cond)?;
                self.line(&format!("while ({c}) {{"));
                self.cur.ctl.push(true);
                self.block(&w.body)?;
                self.cur.ctl.pop();
                self.line("}");
            }
            Stmt::For(f) => {
                let a = self.expr(&f.start, None)?;
                let b = self.expr(&f.end, None)?;
                let (a, b) = (self.range_bound(a, f.start.span)?, self.range_bound(b, f.end.span)?);
                self.push_scope();
                let v = self.declare(&f.name.name, I32);
                let cmp = if f.inclusive { "<=" } else { "<" };
                self.line(&format!("for (int {v} = {a}; {v} {cmp} {b}; {v}++) {{"));
                self.cur.ctl.push(true);
                self.block(&f.body)?;
                self.cur.ctl.pop();
                self.pop_scope();
                self.line("}");
            }
            Stmt::Match(m) => self.match_stmt(m)?,
            Stmt::Block(b) => {
                self.line("{");
                self.block(b)?;
                self.line("}");
            }
            Stmt::Break(span, label) | Stmt::Continue(span, label) => {
                let is_break = matches!(s, Stmt::Break(..));
                if label.is_some() {
                    return self.err_help(*span, "labels do not exist in GLSL", "restructure with a `bool` flag");
                }
                match (is_break, self.cur.ctl.last().copied(), self.cur.ctl.contains(&true)) {
                    (true, Some(true), _) => self.line("break;"),
                    (true, Some(false), _) => return self.err_help(*span, "`break` inside a `match` would leave the match, not the loop (GLSL `switch`)", "set a flag and `break` after the `match`"),
                    (false, _, true) => self.line("continue;"),
                    _ => return self.err(*span, "`break` and `continue` belong inside a loop"),
                }
            }
            Stmt::ForEach(f) => return self.err_help(f.span, "there are no slices or collections in shader code", "count with `for i in 0..N` and index a fixed array"),
            Stmt::Defer(d) => return self.err_help(d.span, "`defer` does not exist in shader code", "write the statements where they should run"),
            Stmt::Labeled(l) => return self.err_help(l.span, "labels do not exist in GLSL", "restructure with a `bool` flag"),
            Stmt::UseLocal(u) => return self.err(u.span, "`use` does not exist in shader code"),
        }
        Ok(())
    }

    fn if_stmt(&mut self, i: &IfStmt, lead: &str) -> R<()> {
        let c = self.cond(&i.cond)?;
        self.line(&format!("{lead}if ({c}) {{"));
        self.block(&i.then_block)?;
        match &i.else_block {
            None => self.line("}"),
            Some(b) => match b.stmts.as_slice() {
                [Stmt::If(n)] => self.if_stmt(n, "} else ")?,
                _ => {
                    self.line("} else {");
                    self.block(b)?;
                    self.line("}");
                }
            },
        }
        Ok(())
    }

    /// A `for` bound is an `i32` (a bare literal takes that type).
    fn range_bound(&mut self, v: (String, GTy), span: crate::span::Span) -> R<String> {
        if !matches!(v.1, GTy::Scalar(Base::I) | GTy::LitInt) {
            return self.err_help(span, format!("a range bound is i32 in shader code, found {}", v.1.name()), "convert it: `n as i32`");
        }
        Ok(v.0)
    }

    fn match_stmt(&mut self, m: &crate::parse::MatchStmt) -> R<()> {
        let (st, sty) = self.expr(&m.expr, None)?;
        let sty = sty.settle();
        if !matches!(sty, GTy::Scalar(Base::I | Base::U)) {
            return self.err_help(m.expr.span, format!("shader `match` is on an i32 or u32, found {}", sty.name()), "enum and struct patterns do not exist in shader code; use `if`");
        }
        self.line(&format!("switch ({st}) {{"));
        self.cur.ctl.push(false);
        let mut seen = Vec::new();
        let mut has_default = false;
        for arm in &m.arms {
            let head = match &arm.value {
                Some(v) => {
                    let n = lit_value(v).ok_or_else(|| self.e(v.span, "a `match` arm is an integer literal"))?;
                    if seen.contains(&n) {
                        return self.err(v.span, format!("duplicate arm `{n}`"));
                    }
                    if sty == U32 && n < 0 {
                        return self.err(v.span, "a u32 `match` has no negative arms");
                    }
                    seen.push(n);
                    format!("case {n}{}:", if sty == U32 { "u" } else { "" })
                }
                None if arm.path.len() == 1 && arm.path[0].name == "_" => {
                    if has_default {
                        return self.err(arm.span, "a second `_` arm");
                    }
                    has_default = true;
                    "default:".to_string()
                }
                None => return self.err_help(arm.span, "enum patterns do not exist in shader code", "`match` on an i32 with integer arms and `_`"),
            };
            self.depth += 1;
            self.line(&format!("{head} {{"));
            self.block(&arm.body)?;
            self.depth += 1;
            self.line("break;");
            self.depth -= 1;
            self.line("}");
            self.depth -= 1;
        }
        self.cur.ctl.pop();
        if !has_default {
            return self.err_help(m.span, "a shader `match` needs a `_` arm", "add `_ { }`");
        }
        self.line("}");
        Ok(())
    }

    /// An assignable place: a local, or a field or index of one.
    pub(super) fn place(&mut self, e: &Expr) -> R<(String, GTy)> {
        let mut root = e;
        loop {
            match &root.kind {
                ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => root = base,
                ExprKind::Unary { op: UnaryOp::Deref, expr } => root = expr,
                _ => break,
            }
        }
        match &root.kind {
            ExprKind::Name(n) if self.local(&n.name).is_some() => {}
            ExprKind::Name(n) => return self.err_help(n.span, format!("`{}` is not a local and cannot be assigned", n.name), "uniforms and consts are read-only in shader code; copy it with `let`"),
            _ => return self.err(e.span, "this cannot be assigned to"),
        }
        let r = self.expr(e, None)?;
        swizzle_assign_check(self, e, &r.1)?;
        Ok(r)
    }
}

fn op_sym(op: crate::parse::BinOp) -> &'static str {
    super::ops::sym(op)
}

/// `3` or `-3` as a number.
fn lit_value(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::Int(v) => Some(*v),
        ExprKind::Unary { op: UnaryOp::Neg, expr } => lit_value(expr).map(|v| -v),
        _ => None,
    }
}
