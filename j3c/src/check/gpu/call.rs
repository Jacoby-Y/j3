//! Calls in shader code: `@` builtins, constructors (`vec3(..)`), scalar conversions, and shader fns.

use crate::parse::{Expr, ExprKind, Type, UnaryOp};
use crate::span::Span;

use super::builtins::{self, Only};
use super::gen::Gen;
use super::ty::*;
use super::R;

impl<'a> Gen<'a> {
    pub fn call(&mut self, e: &Expr, func: &Expr, type_args: &[Type], args: &[Expr]) -> R<(String, GTy)> {
        if let Some(t) = type_args.first() {
            return self.err_help(t.span, "shader fns have no type arguments", "shader code has no generics");
        }
        let module = self.cur.module.clone();
        match &func.kind {
            ExprKind::Name(n) => {
                if let Some(b) = n.name.strip_prefix('@') {
                    return self.builtin_call(b, args, e.span);
                }
                match builtin_type(&n.name) {
                    Some(t @ GTy::Scalar(_)) if args.len() == 1 => return self.cast(&args[0], t, e.span),
                    Some(GTy::Scalar(_)) => return self.err(e.span, format!("`{}(x)` takes one value", n.name)),
                    Some(t @ (GTy::Vec(..) | GTy::Mat(_))) => return self.ctor(t, args, e.span),
                    Some(_) => return self.err(e.span, format!("`{}` cannot be built with a constructor", n.name)),
                    None => {}
                }
                if matches!(n.name.as_str(), "int" | "i64" | "u64" | "f64" | "u8" | "i8" | "u16" | "i16") {
                    return self.err_help(n.span, format!("`{}(..)` converts to a type shaders do not have", n.name), "convert to `i32`, `u32` or `f32`");
                }
                self.user_call(&module, None, &n.name, None, args, e.span)
            }
            ExprKind::Field { base, field } => {
                if let ExprKind::Name(q) = &base.kind {
                    if self.local(&q.name).is_none() && self.ix.is_alias(&module, &q.name) {
                        return self.user_call(&module, Some(&q.name), &field.name, None, args, e.span);
                    }
                }
                // `x.f(..)` is `f(x, ..)` when `f` is a shader fn (UFCS); any other method does not exist
                let known = self.ix.find(&module, None, &field.name, |m| m.fns.contains_key(field.name.as_str())).is_some();
                if !known {
                    return self.err_help(field.span, format!("`.{}(..)` is a method call; shader code has none", field.name), "call a `#[shader]` fn, an `@` builtin, or use a swizzle like `.xy`");
                }
                self.user_call(&module, None, &field.name, Some(base), args, e.span)
            }
            _ => self.err(func.span, "only named fns can be called in shader code"),
        }
    }

    fn user_call(&mut self, module: &str, qual: Option<&str>, name: &str, recv: Option<&Expr>, args: &[Expr], span: Span) -> R<(String, GTy)> {
        if self.cur.in_const {
            return self.err(span, "a shader `const` initializer cannot call fns");
        }
        let Some(owner) = self.ix.find(module, qual, name, |m| m.fns.contains_key(name)) else {
            let cpu = self.ix.find(module, qual, name, |m| m.cpu_fns.contains(name)).is_some();
            if cpu {
                return self.err_help(span, format!("`{name}` is a CPU fn; shader code can call only `#[shader]` fns and `@` builtins"), format!("write `#[shader]` above `fn {name}` (it then exists only on the GPU), or inline it"));
            }
            let mut names: Vec<String> = self.ix.module(module).map(|m| m.fns.keys().map(|k| k.to_string()).collect()).unwrap_or_default();
            names.sort();
            let e = self.e(span, format!("unknown fn `{name}` in shader code"));
            return Err(match crate::check::error::suggest(name, &names) {
                Some(h) => e.help(format!("did you mean `{h}`?")),
                None => e.help("shader code calls `#[shader]` fns, `@` builtins, and constructors like `vec3(..)`"),
            });
        };
        let f = owner.fns[name];
        let owner_key = owner.key.clone();
        if matches!(f.shader, Some(crate::parse::GpuStage::Vertex | crate::parse::GpuStage::Fragment)) {
            return self.err_help(span, format!("`{name}` is a shader entry and cannot be called"), "move shared code into a plain `#[shader]` fn");
        }
        let sig = self.emit_fn(&owner_key, f, span)?;
        let all: Vec<&Expr> = recv.into_iter().chain(args.iter()).collect();
        if all.len() != sig.params.len() {
            return self.err(span, format!("`{name}` takes {} arguments, found {}", sig.params.len(), all.len()));
        }
        let mut parts = Vec::new();
        for (a, (pty, inout)) in all.iter().zip(sig.params.iter()) {
            if *inout {
                let target = match &a.kind {
                    ExprKind::Unary { op: UnaryOp::Ref, expr } => expr,
                    _ => *a,
                };
                let (t, ty) = self.place(target)?;
                if ty != *pty {
                    return self.err(a.span, format!("expected {}, found {}", pty.name(), ty.name()));
                }
                parts.push(t);
            } else {
                let v = self.expr(a, Some(pty))?;
                parts.push(self.coerce(v, pty, a.span)?);
            }
        }
        Ok((format!("{}({})", sig.glsl, parts.join(", ")), sig.ret.clone()))
    }

    fn builtin_call(&mut self, name: &str, args: &[Expr], span: Span) -> R<(String, GTy)> {
        if name == "shader" {
            return self.err_help(span, "`@shader` pairs entries at module level", "write `const PROG = @shader(vs, fs);` outside any fn");
        }
        let Some(b) = builtins::lookup(name) else {
            return self.err_help(span, format!("`@{name}` does not exist in shader code"), "the shader builtins are listed by `j3c doc shaders`");
        };
        if args.len() < b.min || args.len() > b.max {
            let n = if b.min == b.max { b.min.to_string() } else { format!("{} to {}", b.min, b.max) };
            return self.err(span, format!("`@{name}` takes {n} arguments, found {}", args.len()));
        }
        if b.only != Only::Any && self.stage != Only::Any && b.only != self.stage {
            let (want, have) = if b.only == Only::Fragment { ("fragment", "vertex") } else { ("vertex", "fragment") };
            return self.err_help(span, format!("`@{name}` exists only in {want} shaders"), format!("this code is reached from a {have} shader"));
        }
        let mut vals = Vec::new();
        for a in args {
            vals.push(self.expr(a, None)?);
        }
        // literals take the element type of the first typed numeric argument (`@clamp(x, 0, 1)` with x: f32)
        let anchor = vals.iter().find_map(|(_, t)| match t {
            GTy::Scalar(_) | GTy::Vec(..) => t.elem(),
            GTy::Mat(_) => Some(F32),
            _ => None,
        });
        let lit = anchor.unwrap_or(if vals.iter().any(|(_, t)| *t == GTy::LitFloat) { F32 } else { I32 });
        let mut texts = Vec::new();
        let mut tys = Vec::new();
        for (v, a) in vals.into_iter().zip(args) {
            let ty = if matches!(v.1, GTy::LitInt | GTy::LitFloat) { lit.clone() } else { v.1.clone() };
            texts.push(self.coerce(v, &ty, a.span)?);
            tys.push(ty);
        }
        let res = match b.result(&tys) {
            Ok(t) => t,
            Err(want) => {
                let have: Vec<String> = tys.iter().map(|t| t.name()).collect();
                return self.err_help(span, format!("`@{name}` does not take ({})", have.join(", ")), format!("it takes {want}"));
            }
        };
        let text = match b.rule {
            builtins::Rule::Discard => "discard".to_string(),
            builtins::Rule::FragCoord | builtins::Rule::VertexId => b.glsl.to_string(),
            _ => format!("{}({})", b.glsl, texts.join(", ")),
        };
        Ok((text, res))
    }

    /// `vec3(1.0, v2)`, `mat4(1.0)`: a scalar splat, or components adding up to the size.
    fn ctor(&mut self, to: GTy, args: &[Expr], span: Span) -> R<(String, GTy)> {
        let (elem, need) = match &to {
            GTy::Vec(b, n) => (GTy::Scalar(*b), *n as usize),
            GTy::Mat(n) => (F32, (*n as usize) * (*n as usize)),
            _ => unreachable!(),
        };
        if args.is_empty() {
            return self.err(span, format!("`{}(..)` needs arguments", to.name()));
        }
        let mut parts = Vec::new();
        let mut lanes = 0;
        for a in args {
            let v = self.expr(a, None)?;
            let Some(n) = v.1.lanes() else {
                return self.err(a.span, format!("cannot build {} from {}", to.name(), v.1.name()));
            };
            lanes += n as usize;
            let ty = if matches!(v.1, GTy::LitInt | GTy::LitFloat) { elem.clone() } else { v.1.clone() };
            parts.push(self.coerce(v, &ty, a.span)?);
        }
        let splat = args.len() == 1 && lanes == 1;
        if !splat && lanes != need {
            return self.err_help(span, format!("{} needs {need} components, found {lanes}", to.name()), "give one scalar for all components, or exactly the right count");
        }
        Ok((format!("{}({})", to.glsl(), parts.join(", ")), to))
    }
}
