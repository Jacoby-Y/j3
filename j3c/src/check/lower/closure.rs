//! Closure literals, calls through a closure, and plain-fn thunks (`speculative/lang/closures.j3`).
//!
//! `fn(x: int): int { x + k }` becomes a ctx struct holding `&k`, a generated fn `outer$closureN(ctx, x)`
//! whose captured names are aliases for `(*ctx.c0)`, and the value `{ $f: outer$closureN, $ctx: &ctx }`.
//! The ctx lives in the creating frame, so a closure cannot outlive it (the type rules enforce that).

use super::*;
use crate::ir::entity::{FieldDef, Func, StructDef, Vis};
use crate::parse::ClosureLit;

/// The signature a closure literal is checked against.
enum Want {
    Closure(Vec<Ty>, Ty),
    Plain(Vec<Ty>, Ty),
    Nothing,
}

/// `Box.push<int>`: the fn's name with its owner and type arguments, so instances get distinct closure fns.
pub(super) fn fn_label(cx: &IrCx, id: FuncId, subst: &HashMap<String, Ty>) -> String {
    let f = cx.func(id);
    let mut s = String::new();
    if let Some(sid) = f.method_of {
        s.push_str(cx.name_str(cx.struct_def(sid).name));
        s.push('.');
    }
    s.push_str(cx.name_str(f.name));
    if !subst.is_empty() {
        let mut keys: Vec<&String> = subst.keys().collect();
        keys.sort();
        let tys: Vec<String> = keys.iter().map(|k| { let mut t = String::new(); dump_ty(cx, subst[*k], &mut t); t }).collect();
        s.push_str(&format!("<{}>", tys.join(",")));
    }
    s
}

const NO_SCRIPTS: &str = "closures are not supported in .j3s scripts";

impl<'a> Lower<'a> {
    /// A name that a new local would shadow: in this body, or outside the closure being lowered.
    pub(super) fn is_declared(&self, name: &str) -> bool {
        self.locals.contains_key(name) || self.closure_outer.contains(name)
    }

    /// A `break`/`continue` with no loop to leave; inside a closure, say the closure is its own function.
    pub(super) fn loop_err(&self, span: Span, msg: String) -> CheckError {
        let e = CheckError::new(self.path, span, msg);
        if !self.in_closure {
            return e;
        }
        e.note("a closure body is its own function; `break` and `continue` cannot leave it")
            .help("loop inside the closure, or return a value and let the caller decide")
    }

    pub(super) fn script_closure_err(&self, span: Span) -> CheckError {
        CheckError::new(self.path, span, NO_SCRIPTS)
            .help("scripts run on the VM, which has no function values; move this code into a `.j3` module")
    }

    fn want_of(&self, expected: Option<Ty>) -> Want {
        let Some(t) = expected else { return Want::Nothing };
        if let Some((p, r)) = self.cx.closure_sig(t) {
            return Want::Closure(p.clone(), *r);
        }
        match self.cx.fn_sig(t) {
            Some((p, r)) => Want::Plain(p.clone(), r),
            None => Want::Nothing,
        }
    }

    /// Why a closure was passed where a plain `fn`/`ptr` is wanted, when that is the mismatch.
    pub(super) fn closure_mismatch(&self, span: Span, have: Ty, want: Ty) -> Option<CheckError> {
        self.cx.closure_sig(have)?;
        if !matches!(self.cx.ty_kind(want), TyKind::Fn { .. } | TyKind::Ptr) {
            return None;
        }
        Some(
            CheckError::new(
                self.path,
                span,
                format!(
                    "expected `{}`, found `{}`: a closure carries its captures and cannot become a plain fn or ptr",
                    self.ty_str(want),
                    self.ty_str(have)
                ),
            )
            .help("take a `closure(..)` parameter instead, or pass a named fn"),
        )
    }

    pub(super) fn closure_lit(
        &mut self,
        span: Span,
        lit: &ClosureLit,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        if self.script_sid.is_some() {
            return Err(self.script_closure_err(span));
        }
        let want = self.want_of(expected);
        let (want_params, want_ret) = match &want {
            Want::Closure(p, r) | Want::Plain(p, r) => (Some(p.clone()), Some(*r)),
            Want::Nothing => (None, None),
        };
        let mut ptys = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for (i, p) in lit.params.iter().enumerate() {
            if !seen.insert(&p.name.name) {
                return Err(CheckError::new(self.path, p.name.span, format!("duplicate parameter `{}`", p.name.name)));
            }
            if self.is_declared(&p.name.name) || self.aliases.contains_key(&p.name.name) {
                return Err(CheckError::new(
                    self.path,
                    p.name.span,
                    format!("shadowing `{}` is a compile error", p.name.name),
                )
                .help("a closure parameter cannot reuse the name of a variable it can see; pick another name"));
            }
            let ty = match (&p.ty, &want_params) {
                (Some(t), _) => self.resolve_ty(t)?,
                (None, Some(ps)) if ps.len() == lit.params.len() => ps[i],
                (None, Some(ps)) => {
                    return Err(CheckError::new(
                        self.path,
                        span,
                        format!("this closure has {} parameter(s) but {} expected", lit.params.len(), ps.len()),
                    ));
                }
                (None, None) => {
                    return Err(CheckError::new(
                        self.path,
                        p.name.span,
                        format!("closure parameter `{}` needs a type here", p.name.name),
                    )
                    .help(format!("write `{}: int` (or its type); types may be left out only where a `closure(..)` or `fn(..)` type is expected", p.name.name)));
                }
            };
            ptys.push(ty);
        }
        let ret = match (&lit.ret, want_ret) {
            (Some(t), _) => {
                let r = self.resolve_ty(t)?;
                reject_closure(self.cx, self.path, t.span, r, "returned")?;
                r
            }
            (None, Some(r)) => r,
            (None, None) => self.cx.common.void,
        };

        // Captured names, in first-use order.
        let mut used = Vec::new();
        let mut once = HashSet::new();
        crate::parse::walk::names_in_block(&lit.body, &mut |id| {
            if once.insert(id.name.clone()) {
                used.push(id.name.clone());
            }
        });
        let mut caps: Vec<(String, Place)> = Vec::new();
        for name in used {
            let place = if let Some(p) = self.aliases.get(&name) {
                p.clone()
            } else if let Some(&id) = self.locals.get(&name) {
                self.b.local_place(id, span)
            } else {
                continue;
            };
            if !place.access.take_ref {
                return Err(CheckError::new(self.path, span, format!("cannot capture `{name}`: its address cannot be taken")));
            }
            caps.push((name, place));
        }
        let plain = matches!(want, Want::Plain(..));
        if plain && !caps.is_empty() {
            return Err(CheckError::new(
                self.path,
                span,
                format!(
                    "this closure captures `{}`, so it cannot be a plain `{}`",
                    caps[0].0,
                    expected.map(|t| self.ty_str(t)).unwrap_or_default()
                ),
            )
            .help("take a `closure(..)` parameter instead, or stop using outer variables in the body"));
        }

        let n = self.closure_n;
        self.closure_n += 1;
        let label = format!("{}$closure{n}", self.fn_label);
        let fn_ty = if plain { self.cx.ty_fn(ptys.clone(), ret) } else { self.cx.closure_fn_ty(&ptys, ret) };
        let fname = self.cx.intern_name(&label);
        let fid = self.cx.alloc_func(Func {
            id: FuncId(0),
            doc: String::new(),
            module: self.module_names.to_vec(),
            name: fname,
            ty: fn_ty,
            vis: Vis::Private,
            method_of: None,
            kind: FuncKind::Decl,
            span,
        });
        self.ir_funcs.push(fid);

        // The ctx struct: one `&T` per captured name.
        let ctx = if caps.is_empty() {
            None
        } else {
            let mut fields = Vec::new();
            for (i, (_, pl)) in caps.iter().enumerate() {
                let ref_ty = self.cx.ty_ref(pl.ty);
                fields.push(FieldDef {
                    name: self.cx.intern_name(&format!("c{i}")),
                    doc: String::new(),
                    ty: ref_ty,
                    embedded: false,
                    default: None,
                });
            }
            let sname = self.cx.intern_name(&format!("{label}$ctx"));
            let sid = self.cx.alloc_struct(StructDef {
                module: self.module_names.to_vec(),
                name: sname,
                doc: String::new(),
                fields,
                packed: false,
                debug: false,
                span,
                ty_args: Vec::new(),
                script: None,
            });
            self.ir_structs.push(sid);
            Some(sid)
        };

        let body = self.closure_body(span, lit, &label, &ptys, ret, plain, ctx, &caps)?;
        self.cx.func_mut(fid).kind = FuncKind::J3 { body };

        if plain {
            return Ok(Rvalue::use_op(span, Operand::fn_addr(span, fn_ty, fid)));
        }
        let ptr = self.cx.common.ptr;
        let ctx_op = match ctx {
            None => Operand::null(span, ptr),
            Some(sid) => {
                let ctx_ty = self.cx.ty_struct(sid);
                let n = self.fresh("ctx");
                let tmp = self.b.tmp(n, ctx_ty, span);
                let tmp_place = self.b.local_place(tmp, span);
                for (i, (_, pl)) in caps.iter().enumerate() {
                    let field = self.b.field(self.cx, tmp_place.clone(), FieldId { struct_id: sid, index: i as u32 }, span);
                    let ref_ty = self.cx.ty_ref(pl.ty);
                    self.b.assign(span, field, Rvalue::ref_place(span, ref_ty, pl.clone()));
                }
                let rn = self.cx.intern_name("_ctxref");
                let r = self.b.ref_tmp(self.cx, rn, tmp_place, span);
                let n = self.fresh("ctxp");
                let cast = self.b.tmp(n, ptr, span);
                let cast_place = self.b.local_place(cast, span);
                self.b.assign(span, cast_place.clone(), Rvalue::cast(span, ptr, r));
                Operand::Copy(cast_place)
            }
        };
        let cty = self.cx.closure_ty(ptys, ret, self.module_names, span, self.ir_structs);
        Ok(Rvalue::aggregate(span, cty, vec![Operand::fn_addr(span, fn_ty, fid), ctx_op]))
    }

    /// Lowers the body into its own `Body`, swapping this `Lower`'s per-body state out and back.
    #[allow(clippy::too_many_arguments)]
    fn closure_body(
        &mut self,
        span: Span,
        lit: &ClosureLit,
        label: &str,
        ptys: &[Ty],
        ret: Ty,
        plain: bool,
        ctx: Option<StructId>,
        caps: &[(String, Place)],
    ) -> Result<crate::ir::body::Body, CheckError> {
        let void = self.cx.common.void;
        let mut outer: HashSet<String> = self.closure_outer.clone();
        outer.extend(self.locals.keys().cloned());
        outer.extend(self.aliases.keys().cloned());
        let saved_b = std::mem::replace(&mut self.b, BodyBuilder::new(void));
        let saved_locals = std::mem::take(&mut self.locals);
        let saved_aliases = std::mem::take(&mut self.aliases);
        let saved_loops = std::mem::take(&mut self.loops);
        let saved_label = self.pending_label.take();
        let saved_ret = std::mem::replace(&mut self.ret, ret);
        let saved_outer = std::mem::replace(&mut self.closure_outer, outer);
        let saved_fn = std::mem::replace(&mut self.fn_label, label.to_string());
        let saved_n = std::mem::replace(&mut self.closure_n, 0);
        let saved_in = std::mem::replace(&mut self.in_closure, true);
        let saved_indirect = std::mem::replace(&mut self.store_indirect, false);

        let r = (|| {
            let ptr = self.cx.common.ptr;
            let ctx_param = if plain {
                None
            } else {
                let n = self.cx.intern_name("$ctx");
                Some(self.b.param(n, ptr, span))
            };
            for (p, ty) in lit.params.iter().zip(ptys) {
                let n = self.cx.intern_name(&p.name.name);
                let id = self.b.param(n, *ty, p.name.span);
                self.locals.insert(p.name.name.clone(), id);
            }
            if let (Some(cp), Some(sid)) = (ctx_param, ctx) {
                let ctx_ty = self.cx.ty_struct(sid);
                let ref_ty = self.cx.ty_ref(ctx_ty);
                let n = self.cx.intern_name("_ctx");
                let typed = self.b.tmp(n, ref_ty, span);
                let typed_place = self.b.local_place(typed, span);
                let raw = self.b.local_place(cp, span);
                self.b.assign(span, typed_place.clone(), Rvalue::cast(span, ref_ty, Operand::Copy(raw)));
                for (i, (name, orig)) in caps.iter().enumerate() {
                    let base = self.b.deref(self.cx, typed_place.clone(), span);
                    let field = self.b.field(self.cx, base, FieldId { struct_id: sid, index: i as u32 }, span);
                    let mut place = self.b.deref(self.cx, field, span);
                    place.access = orig.access;
                    self.aliases.insert(name.clone(), place);
                }
            }
            for stmt in &lit.body.stmts {
                self.stmt(stmt)?;
            }
            Ok(())
        })();

        let body_b = std::mem::replace(&mut self.b, saved_b);
        self.locals = saved_locals;
        self.aliases = saved_aliases;
        self.loops = saved_loops;
        self.pending_label = saved_label;
        self.ret = saved_ret;
        self.closure_outer = saved_outer;
        self.fn_label = saved_fn;
        self.closure_n = saved_n;
        self.in_closure = saved_in;
        self.store_indirect = saved_indirect;
        r?;
        Ok(body_b.finish())
    }
}
