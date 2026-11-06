//! Expressions: names, literals, fields and swizzles, indexing, `if` values, struct and array literals.
//! Each returns the GLSL text with its type; calls are `call.rs`, operators `ops.rs`.

use crate::emit::glsl::num;
use crate::parse::{Expr, ExprKind, UnaryOp};
use crate::span::Span;

use super::gen::Gen;
use super::ty::*;
use super::R;

impl<'a> Gen<'a> {
    pub fn expr(&mut self, e: &Expr, want: Option<&GTy>) -> R<(String, GTy)> {
        match &e.kind {
            ExprKind::Int(v) => {
                if *v > u32::MAX as i64 || *v < i32::MIN as i64 {
                    return self.err_help(e.span, "this integer does not fit in 32 bits", "shader integers are i32 or u32");
                }
                Ok((v.to_string(), GTy::LitInt))
            }
            ExprKind::Float(v) => Ok((num::float(*v), GTy::LitFloat)),
            ExprKind::Bool(b) => Ok((b.to_string(), BOOL)),
            ExprKind::Name(n) => self.name(&n.name, n.span),
            ExprKind::Unary { op, expr } => self.unary(*op, expr, e.span),
            ExprKind::Binary { op, lhs, rhs } => {
                let l = self.expr(lhs, None)?;
                let r = self.expr(rhs, None)?;
                let (lt, rt, res) = self.arith(*op, l, r, e.span)?;
                Ok((format!("({lt} {} {rt})", super::ops::sym(*op)), res))
            }
            ExprKind::Cast { expr, ty } => {
                let to = self.ty_of(ty, &self.cur.module.clone())?;
                self.cast(expr, to, e.span)
            }
            ExprKind::Call { func, type_args, args } => self.call(e, func, type_args, args),
            ExprKind::Field { base, field } => self.field(base, &field.name, field.span),
            ExprKind::Index { base, index } => self.index(base, index, e.span),
            ExprKind::If { cond, then_expr, else_expr } => {
                let (c, ct) = self.expr(cond, None)?;
                if ct != BOOL {
                    return self.err(cond.span, format!("a condition must be bool, found {}", ct.name()));
                }
                let a = self.expr(then_expr, want)?;
                let b = self.expr(else_expr, want)?;
                let ty = if a.1 == b.1 { a.1.clone() } else if matches!(a.1, GTy::LitInt | GTy::LitFloat) { b.1.clone() } else { a.1.clone() };
                let (at, bt) = (self.coerce(a, &ty, then_expr.span)?, self.coerce(b, &ty, else_expr.span)?);
                Ok((format!("({c} ? {at} : {bt})"), ty))
            }
            ExprKind::StructLit { name, type_args, fields } => {
                if !type_args.is_empty() {
                    return self.err(name.span, "shader structs are not generic");
                }
                let module = self.cur.module.clone();
                let ty = self.struct_ty(&module, &name.name, name.span)?;
                let GTy::Struct { key, glsl } = &ty else { unreachable!() };
                let info = self.sinfo[key].clone();
                let mut parts = Vec::new();
                for f in &info.fields {
                    let Some((_, v)) = fields.iter().find(|(n, _)| n.name == f.name) else {
                        return self.err(e.span, format!("`{}` needs its field `{}`", name.name, f.name));
                    };
                    let val = self.expr(v, Some(&f.ty))?;
                    parts.push(self.coerce(val, &f.ty, v.span)?);
                }
                for (n, _) in fields {
                    if !info.fields.iter().any(|f| f.name == n.name) {
                        return self.err(n.span, format!("`{}` has no field `{}`", name.name, n.name));
                    }
                }
                Ok((format!("{glsl}({})", parts.join(", ")), ty))
            }
            ExprKind::ArrayLit { elems } => {
                let Some(GTy::Array(elem, n)) = want else {
                    return self.err_help(e.span, "an array literal needs a known type", "write `let a: [4]f32 = [..]`");
                };
                if elems.len() as u64 != *n {
                    return self.err(e.span, format!("the type has {n} elements, the literal has {}", elems.len()));
                }
                let mut parts = Vec::new();
                for x in elems {
                    let v = self.expr(x, Some(elem))?;
                    parts.push(self.coerce(v, elem, x.span)?);
                }
                Ok((format!("{}[{n}]({})", elem.glsl(), parts.join(", ")), want.cloned().unwrap()))
            }
            ExprKind::String(_) | ExprKind::Interpolate { .. } => self.err_help(e.span, "strings do not exist in shader code", "shaders compute with numbers"),
            ExprKind::Slice { .. } | ExprKind::SliceLit { .. } | ExprKind::SliceFrom { .. } => self.err_help(e.span, "slices do not exist in shader code", "use a fixed array `[N]T`"),
            ExprKind::Closure(_) => self.err_help(e.span, "closures do not exist in shader code", "write a `#[shader]` fn and call it"),
            ExprKind::Atomic { .. } => self.err_help(e.span, "atomics do not exist in shader code", "each invocation computes independently"),
            ExprKind::Question { .. } => self.err(e.span, "`?` does not exist in shader code (no results or options)"),
            ExprKind::Syscall { .. } | ExprKind::SizeOf(_) | ExprKind::TypePath { .. } | ExprKind::ScriptType(_) | ExprKind::ScriptLit { .. } | ExprKind::UiChoice { .. } => {
                self.err(e.span, "this is CPU-only and does not exist in shader code")
            }
        }
    }

    fn name(&mut self, n: &str, span: Span) -> R<(String, GTy)> {
        if let Some(l) = self.local(n) {
            return Ok((l.glsl.clone(), l.ty.clone()));
        }
        let module = self.cur.module.clone();
        if let Some(r) = self.uniform(&module, None, n)? {
            return Ok(r);
        }
        if let Some(r) = self.const_ref(&module, None, n)? {
            return Ok(r);
        }
        let me = self.ix.module(&module);
        if me.is_some_and(|m| m.globals.contains(n)) {
            return self.err_help(span, format!("`{n}` is a CPU global; shader code reads only `#[uniform]`s and consts"), format!("declare `#[uniform] let {n}: <type>;` and set it by name from the CPU"));
        }
        if me.is_some_and(|m| m.cpu_fns.contains(n)) {
            return self.err(span, format!("`{n}` is a CPU fn; it is not a value in shader code"));
        }
        let mut names: Vec<String> = self.cur.scopes.iter().flat_map(|s| s.keys().cloned()).collect();
        names.sort();
        let hint = crate::check::error::suggest(n, &names);
        let e = self.e(span, format!("unknown name `{n}`"));
        Err(match hint {
            Some(h) => e.help(format!("did you mean `{h}`?")),
            None => e,
        })
    }

    fn unary(&mut self, op: UnaryOp, inner: &Expr, span: Span) -> R<(String, GTy)> {
        if matches!(op, UnaryOp::Ref | UnaryOp::Deref) {
            return self.err_help(span, "pointers do not exist in shader code", "only a call argument for a `&T` parameter may be written `&x`");
        }
        let (t, ty) = self.expr(inner, None)?;
        match op {
            UnaryOp::Neg if matches!(ty, GTy::LitInt | GTy::LitFloat | GTy::Scalar(Base::F | Base::I) | GTy::Vec(Base::F | Base::I, _) | GTy::Mat(_)) => Ok((format!("(-{t})"), ty)),
            UnaryOp::Not if ty == BOOL => Ok((format!("(!{t})"), ty)),
            UnaryOp::BitNot if matches!(ty, GTy::Scalar(Base::I | Base::U) | GTy::LitInt) => Ok((format!("(~{t})"), ty)),
            _ => self.err(span, format!("this operator does not apply to {}", ty.name())),
        }
    }

    /// `x as f32` and `f32(x)`: scalar to scalar.
    pub fn cast(&mut self, inner: &Expr, to: GTy, span: Span) -> R<(String, GTy)> {
        if !matches!(to, GTy::Scalar(_)) {
            return self.err_help(span, format!("cannot convert to {}", to.name()), "casts are between f32, i32, u32 and bool; build vectors with a constructor like `vec3(..)`");
        }
        let v = self.expr(inner, None)?;
        match &v.1 {
            GTy::LitInt if to != BOOL => {
                let ty = to.clone();
                Ok((self.coerce(v, &ty, span)?, to))
            }
            GTy::LitFloat if to == F32 => Ok((v.0, to)),
            GTy::LitInt | GTy::LitFloat => Ok((format!("{}({})", to.glsl(), v.0), to)),
            GTy::Scalar(_) if v.1 == to => Ok((v.0, to)),
            GTy::Scalar(_) => Ok((format!("{}({})", to.glsl(), v.0), to)),
            t => self.err(span, format!("cannot convert {} to {}; casts are between scalars", t.name(), to.name())),
        }
    }

    fn field(&mut self, base: &Expr, field: &str, span: Span) -> R<(String, GTy)> {
        let module = self.cur.module.clone();
        if let ExprKind::Name(q) = &base.kind {
            if self.local(&q.name).is_none() && self.ix.is_alias(&module, &q.name) {
                if let Some(r) = self.uniform(&module, Some(&q.name), field)? {
                    return Ok(r);
                }
                if let Some(r) = self.const_ref(&module, Some(&q.name), field)? {
                    return Ok(r);
                }
                return self.err(span, format!("module `{}` has no uniform or const `{field}`", q.name));
            }
        }
        let (t, ty) = self.expr(base, None)?;
        match &ty {
            GTy::Struct { key, .. } => {
                let info = self.sinfo[key].clone();
                match info.fields.iter().find(|f| f.name == field) {
                    Some(f) => Ok((format!("{t}.{}", f.glsl), f.ty.clone())),
                    None => {
                        let names: Vec<String> = info.fields.iter().map(|f| f.name.clone()).collect();
                        let e = self.e(span, format!("`{}` has no field `{field}`", ty.name()));
                        Err(match crate::check::error::suggest(field, &names) {
                            Some(h) => e.help(format!("did you mean `{h}`?")),
                            None => e.help(format!("fields: {}", names.join(", "))),
                        })
                    }
                }
            }
            GTy::Vec(b, n) => match swizzle_len(field, *n) {
                Some(1) => Ok((format!("{t}.{field}"), GTy::Scalar(*b))),
                Some(k) => Ok((format!("{t}.{field}"), GTy::Vec(*b, k))),
                None => self.err_help(span, format!("`{field}` is not a swizzle of {}", ty.name()), format!("components of a {n}-vector are {}", &"xyzw"[..*n as usize])),
            },
            _ => self.err_help(span, format!("{} has no field `{field}`", ty.name()), "methods do not exist in shader code; call a `#[shader]` fn"),
        }
    }

    fn index(&mut self, base: &Expr, index: &Expr, span: Span) -> R<(String, GTy)> {
        let (bt, ty) = self.expr(base, None)?;
        let (it, ity) = self.expr(index, None)?;
        if !matches!(ity, GTy::Scalar(Base::I | Base::U) | GTy::LitInt) {
            return self.err_help(index.span, format!("an index is i32 or u32, found {}", ity.name()), "convert it: `i as i32`");
        }
        let (elem, len) = match &ty {
            GTy::Array(e, n) => ((**e).clone(), *n),
            GTy::Vec(b, n) => (GTy::Scalar(*b), *n as u64),
            GTy::Mat(n) => (GTy::Vec(Base::F, *n), *n as u64),
            t => return self.err(span, format!("{} cannot be indexed", t.name())),
        };
        if let ExprKind::Int(i) = index.kind {
            if i < 0 || i as u64 >= len {
                return self.err_help(index.span, format!("index {i} is out of range for {len} elements"), format!("valid indices are 0..{}", len - 1));
            }
        }
        Ok((format!("{bt}[{it}]"), elem))
    }
}

/// `v.xx = ..` repeats a component, which GLSL rejects.
pub fn swizzle_assign_check(g: &Gen, e: &Expr, ty: &GTy) -> R<()> {
    if let ExprKind::Field { field, .. } = &e.kind {
        if matches!(ty, GTy::Scalar(_) | GTy::Vec(..)) && swizzle_len(&field.name, 4).is_some() && swizzle_repeats(&field.name) {
            return g.err_help(field.span, format!("`.{}` repeats a component and cannot be assigned", field.name), "assign each component once, like `v.xy = ..`");
        }
    }
    Ok(())
}
