//! One shader fn: signature, recursion check, body, and the text of the GLSL fn.

use std::collections::HashMap;
use std::rc::Rc;

use crate::emit::glsl::names::mangle;
use crate::parse::{Block, FnItem, Stmt, TypeKind};
use crate::span::Span;

use super::gen::{Cur, FnSig, Gen, Local};
use super::ty::*;
use super::R;

impl<'a> Gen<'a> {
    /// Emit `f` (and, first, everything it calls) unless already done; the signature either way.
    /// `at` is where the call sits, for the recursion error.
    pub fn emit_fn(&mut self, module: &str, f: &'a FnItem, at: Span) -> R<Rc<FnSig>> {
        let name = &f.name.name;
        let key = format!("{module}::{name}");
        if let Some(s) = self.sigs.get(&key) {
            return Ok(s.clone());
        }
        if let Some(pos) = self.stack.iter().position(|k| *k == key) {
            let mut cycle: Vec<&str> = self.stack[pos..].iter().map(|k| k.rsplit("::").next().unwrap_or(k)).collect();
            cycle.push(name);
            return self.err_help(at, format!("recursion: {}", cycle.join(" -> ")), "GLSL has no recursion; write the loop out with `while` or `for i in 0..N`");
        }
        if !f.type_params.is_empty() {
            return self.err_help(f.name.span, format!("`{name}` is generic"), "shader fns have no generics; write one fn per type");
        }
        let glsl = self.global_name(module, name);
        let mut params = Vec::new();
        let mut plist = Vec::new();
        let mut locals = HashMap::new();
        for p in &f.params {
            let (t, inout) = match &p.ty.kind {
                TypeKind::Ref(inner) => (self.ty_of(inner, module)?, true),
                _ => (self.ty_of(&p.ty, module)?, false),
            };
            if matches!(t, GTy::Sampler(_)) && inout {
                return self.err(p.ty.span, "a sampler cannot be an `inout` parameter");
            }
            let g = mangle(&p.name.name);
            plist.push(format!("{}{} {g}", if inout { "inout " } else { "" }, t.glsl()));
            locals.insert(p.name.name.clone(), Local { glsl: g, ty: t.clone() });
            params.push((t, inout));
        }
        let ret = match &f.ret {
            Some(t) => self.ty_of(t, module)?,
            None => GTy::Void,
        };
        let sig = Rc::new(FnSig { glsl: glsl.clone(), params, ret: ret.clone() });
        self.stack.push(key.clone());
        let saved = std::mem::replace(&mut self.cur, Cur { module: module.to_string(), ret: ret.clone(), ..Cur::default() });
        let saved_body = std::mem::take(&mut self.body);
        let saved_depth = std::mem::replace(&mut self.depth, 1);
        self.cur.scopes.push(locals);
        let result = self.block_stmts(&f.body);
        self.cur = saved;
        let text = std::mem::replace(&mut self.body, saved_body);
        self.depth = saved_depth;
        self.stack.pop();
        result?;
        if ret != GTy::Void && !ends_with_return(&f.body) {
            return self.err_help(f.body.span, format!("fn `{name}` can reach its end without returning a {}", ret.name()), "end every path with `return`");
        }
        self.fns_out.push(format!("// {}\n{} {glsl}({}) {{\n{text}}}\n", self.ix.at(f.span), ret.glsl(), plist.join(", ")));
        self.sigs.insert(key, sig.clone());
        Ok(sig)
    }
}

/// Whether every path through `b` ends in `return` (the last statement, or an `if`/`else` or `match` with `_`).
fn ends_with_return(b: &Block) -> bool {
    match b.stmts.last() {
        Some(Stmt::Return(_)) => true,
        Some(Stmt::Block(x)) => ends_with_return(x),
        Some(Stmt::If(i)) => ends_with_return(&i.then_block) && i.else_block.as_ref().is_some_and(ends_with_return),
        Some(Stmt::Match(m)) => m.arms.iter().any(|a| a.path.len() == 1 && a.path[0].name == "_") && m.arms.iter().all(|a| ends_with_return(&a.body)),
        _ => false,
    }
}
