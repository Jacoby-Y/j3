use super::*;

impl<'a> Lower<'a> {
    pub(super) fn call(
        &mut self,
        span: Span,
        func: &Expr,
        type_args: &[parse::Type],
        args: &[Expr],
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        if let ExprKind::Field { base, field } = &func.kind {
            if let Some(ename) = self.enum_ctor_name(base) {
                if args.len() != 1 {
                    return Err(CheckError::new(
                        self.path,
                        span,
                        "enum variant takes one payload",
                    ));
                }
                return self.construct_variant(span, &ename, &field.name, Some(&args[0]), expected);
            }
        }
        match &func.kind {
            ExprKind::Name(n) => {
                if let Some(simd) = n.name.strip_prefix('@').filter(|s| parse::syntax::INTRINSIC_SIMD.contains(s)) {
                    return self.simd_intrinsic(span, simd, args);
                }
                if n.name.starts_with('@') {
                    return self.closure_intrinsic(span, n, args, expected);
                }
                if simd::VEC_CTORS.contains(&n.name.as_str()) {
                    return self.vec_ctor(span, &n.name, args);
                }
                if let Some(sid) = self.script_sid {
                    if let Some(id) = self.methods.get(&(sid.0, n.name.clone())).copied() {
                        let this = self.locals["this"];
                        let recv = self.b.local_place(this, n.span);
                        return self.call_id(span, id, Some(recv), args);
                    }
                }
                if args.len() == 1 {
                    if let Some(to) = self.conv_ty(&n.name) {
                        let from = self.operand(&args[0], None)?;
                        self.half_cast(span, to, &from)?;
                        return Ok(Rvalue::cast(span, to, from));
                    }
                }
                if let Some(id) = self.funcs.get(&n.name).copied() {
                    return self.call_id(span, id, None, args);
                }
                if let Some(m) = self.generic_named(&n.name) {
                    return self.call_generic(span, &m, &n.name, type_args, None, args);
                }
                if let Some(place) = self.aliases.get(&n.name).cloned() {
                    return self.call_fn_op(span, Operand::Copy(place), args);
                }
                if let Some(&local) = self.locals.get(&n.name) {
                    let place = self.b.local_place(local, n.span);
                    return self.call_fn_op(span, Operand::Copy(place), args);
                }
                Err(self.unknown(n.span, "function", &n.name, &[]))
            }
            ExprKind::Field { base, field } => {
                if let ExprKind::Name(n) = &base.kind {
                    if let Some(ns) = self
                        .lib_ns
                        .get(&n.name)
                        .or_else(|| self.mod_ns.get(&n.name))
                    {
                        let id = match ns.get(&field.name).copied() {
                            Some(id) => id,
                            None if self.generic_in_alias(&n.name, &field.name).is_some() => {
                                let m = self.generic_in_alias(&n.name, &field.name).unwrap();
                                return self.call_generic(span, &m, &field.name, type_args, None, args);
                            }
                            None => {
                                let mut e = CheckError::new(
                                    self.path,
                                    field.span,
                                    format!("`{}` has no `{}`", n.name, field.name),
                                );
                                let cands: Vec<String> = ns.keys().cloned().collect();
                                if let Some(s) = crate::check::error::suggest(&field.name, &cands) {
                                    e = e.help(format!("did you mean `{}.{s}`?", n.name));
                                }
                                return Err(e);
                            }
                        };
                        return self.call_id(span, id, None, args);
                    }
                    if let Some(sid) = self.struct_id_named(&n.name) {
                        let targs = self.resolve_type_args(type_args)?;
                        let id = self.lookup_method(sid, &field.name, targs, field.span)?;
                        return self.call_id(span, id, None, args);
                    }
                }
                if let ExprKind::TypePath { name, args: targs } = &base.kind {
                    let sid = self.instantiate_struct_named(&name.name, targs, name.span)?;
                    let extra = self.resolve_type_args(type_args)?;
                    let id = self.lookup_method(sid, &field.name, extra, field.span)?;
                    return self.call_id(span, id, None, args);
                }
                let recv = self.receiver_place(base)?;
                let mut recv_ty = recv.ty;
                while let Some(inner) = self.cx.ref_inner(recv_ty) {
                    recv_ty = inner;
                }
                let sid = match self.cx.ty_kind(recv_ty) {
                    TyKind::Struct(id) => Some(*id),
                    _ => None,
                };
                if let Some(id) = self.ufcs_target(sid, &field.name, recv.ty) {
                    return self.call_id(span, id, Some(recv), args);
                }
                let has_method = sid.is_some_and(|s| {
                    let key = (s.0, field.name.clone());
                    self.methods.contains_key(&key) || self.method_temps.contains_key(&key)
                });
                if !has_method {
                    if let Some(id) = self.impl_target(recv.ty, &field.name) {
                        return self.call_id(span, id, Some(recv), args);
                    }
                    if let Some(m) = self.generic_named(&field.name) {
                        return self.call_generic(span, &m, &field.name, type_args, Some(recv), args);
                    }
                }
                let sid = match sid {
                    Some(sid) => sid,
                    None => {
                        let e = CheckError::new(
                            self.path,
                            field.span,
                            format!("no method `{}`", field.name),
                        );
                        return Err(self.ufcs_hint(e, &field.name));
                    }
                };
                let targs = self.resolve_type_args(type_args)?;
                if field.name == "script" && targs.len() == 1 && self.is_scene(sid) {
                    let id = self.scene_script_find(targs[0], field.span)?;
                    return self.call_id(span, id, Some(recv), args);
                }
                let id = self
                    .lookup_method(sid, &field.name, targs, field.span)
                    .map_err(|e| self.ufcs_hint(e, &field.name))?;
                self.call_id(span, id, Some(recv), args)
            }
            _ => Err(CheckError::new(
                self.path,
                func.span,
                "cannot call this expression",
            )),
        }
    }

    /// The `panicAt` next to `j3.sys.panic`, when `id` is that `panic`.
    pub(super) fn panic_at_for(&self, id: FuncId) -> Option<FuncId> {
        let f = self.cx.func(id);
        if self.cx.name_str(f.name) != "panic" || f.method_of.is_some() {
            return None;
        }
        let is_sys = |m: &[crate::intern::Name]| {
            m.len() == 2 && self.cx.name_str(m[0]) == "j3" && self.cx.name_str(m[1]) == "sys"
        };
        if !is_sys(&f.module) {
            return None;
        }
        self.cx.funcs().iter().find(|g| {
            self.cx.name_str(g.name) == "panicAt" && g.module == f.module
        }).map(|g| g.id)
    }

    pub(super) fn call_id(
        &mut self,
        span: Span,
        id: FuncId,
        recv: Option<Place>,
        args: &[Expr],
    ) -> Result<Rvalue, CheckError> {
        // `panic(msg)` reports where it was called: call `panicAt(file, line, msg)` instead.
        if let (None, 1) = (&recv, args.len()) {
            if let Some(at) = self.panic_at_for(id) {
                let fty = self.cx.func(at).ty;
                let ret = self.cx.fn_sig(fty).map(|s| s.1).ok_or_else(|| {
                    CheckError::new(self.path, span, "not a function")
                })?;
                let line = self
                    .cx
                    .file(span.file)
                    .map(|f| f.line(span.start))
                    .unwrap_or(0);
                // Last two path components (`src/main.j3`): short, and the same on every machine.
                let short: Vec<&str> = self.path.rsplit(['/', '\\']).take(2).collect();
                let short = short.into_iter().rev().collect::<Vec<_>>().join("/");
                let file = Operand::bytes(span, self.cx.common.string, short.as_bytes());
                let line = Operand::int(span, self.cx.common.int, line as u64);
                let msg = self.operand(&args[0], Some(self.cx.common.string))?;
                return Ok(Rvalue::call(
                    span,
                    ret,
                    Operand::fn_addr(span, fty, at),
                    vec![file, line, msg],
                ));
            }
        }
        let fty = self.cx.func(id).ty;
        let (params, ret) = match self.cx.fn_sig(fty) {
            Some(s) => (s.0.clone(), s.1),
            None => {
                return Err(CheckError::new(self.path, span, "not a function"));
            }
        };
        let mut ops = Vec::new();
        let mut arg_i = 0;
        if let Some(recv) = recv {
            if params.is_empty() {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "method has no `this` parameter",
                ));
            }
            ops.push(self.auto_this(span, recv, params[0])?);
            arg_i = 1;
        }
        if params.len() - arg_i != args.len() {
            let name = self.cx.name_str(self.cx.func(id).name).to_string();
            let shown: Vec<String> = params[arg_i..].iter().map(|t| self.ty_str(*t)).collect();
            let ret_s = if ret == self.cx.common.void { String::new() } else { format!(": {}", self.ty_str(ret)) };
            return Err(CheckError::new(
                self.path,
                span,
                format!("`{name}` takes {} args, got {}", params.len() - arg_i, args.len()),
            )
            .note(format!("fn {name}({}){ret_s}", shown.join(", "))));
        }
        for (i, arg) in args.iter().enumerate() {
            ops.push(self.operand(arg, Some(params[arg_i + i]))?);
        }
        Ok(Rvalue::call(
            span,
            ret,
            Operand::fn_addr(span, fty, id),
            ops,
        ))
    }

    pub(super) fn call_fn_op(
        &mut self,
        span: Span,
        func: Operand,
        args: &[Expr],
    ) -> Result<Rvalue, CheckError> {
        if self.cx.closure_sig(func.ty()).is_some() {
            return self.call_closure(span, func, args);
        }
        let (params, ret) = match self.cx.fn_sig(func.ty()) {
            Some(s) => (s.0.clone(), s.1),
            None => {
                return Err(CheckError::new(self.path, span, "not a function"));
            }
        };
        if params.len() != args.len() {
            return Err(CheckError::new(
                self.path,
                span,
                format!("takes {} args, got {}", params.len(), args.len()),
            ));
        }
        let mut ops = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            ops.push(self.operand(arg, Some(params[i]))?);
        }
        Ok(Rvalue::call(span, ret, func, ops))
    }

    /// How a receiver of type `have` becomes a first parameter of type `want` (methods and UFCS).
    pub(super) fn this_adjust(&self, have: Ty, want: Ty) -> Option<ThisAdjust> {
        if have == want {
            Some(ThisAdjust::Same)
        } else if self.cx.ref_inner(want) == Some(have) {
            Some(ThisAdjust::TakeRef)
        } else if self.cx.ref_inner(have) == Some(want) {
            Some(ThisAdjust::Deref)
        } else {
            None
        }
    }

    pub(super) fn auto_this(&mut self, span: Span, recv: Place, want: Ty) -> Result<Operand, CheckError> {
        match self.this_adjust(recv.ty, want) {
            Some(ThisAdjust::Same) => Ok(Operand::Copy(recv)),
            Some(ThisAdjust::TakeRef) => {
                if !recv.access.take_ref {
                    return Err(CheckError::new(
                        self.path,
                        span,
                        "cannot take `&` of this receiver",
                    ));
                }
                let rv = Rvalue::ref_place(span, want, recv);
                let n = self.fresh("this");
                let id = self.b.tmp(n, want, span);
                let place = self.b.local_place(id, span);
                self.b.assign(span, place.clone(), rv);
                Ok(Operand::Copy(place))
            }
            Some(ThisAdjust::Deref) => {
                let deref = self.b.deref(self.cx, recv, span);
                Ok(Operand::Copy(deref))
            }
            None => Err(CheckError::new(
                self.path,
                span,
                "method receiver type mismatch",
            )),
        }
    }

    pub(super) fn receiver_place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        if let Ok(p) = self.try_place(expr) {
            return Ok(p);
        }
        let op = self.operand(expr, None)?;
        let n = self.fresh("recv");
        let id = self.b.tmp(n, op.ty(), expr.span);
        let place = self.b.local_place(id, expr.span);
        self.b
            .assign(expr.span, place.clone(), Rvalue::use_op(expr.span, op));
        Ok(place)
    }

    pub(super) fn syscall(&mut self, span: Span, args: &[Expr]) -> Result<Rvalue, CheckError> {
        if args.is_empty() {
            return Err(CheckError::new(self.path, span, "@syscall needs a number"));
        }
        let int_ty = self.cx.common.int;
        let nr = self.operand(&args[0], Some(int_ty))?;
        let mut rest = Vec::new();
        for a in &args[1..] {
            rest.push(self.operand(a, None)?);
        }
        Ok(Rvalue::syscall(span, int_ty, nr, rest))
    }

    pub(super) fn atomic(
        &mut self,
        span: Span,
        op: crate::ir::op::AtomicOp,
        order: crate::ir::op::AtomicOrder,
        args: &[Expr],
    ) -> Result<Rvalue, CheckError> {
        use crate::ir::op::AtomicOp;
        let int_ty = self.cx.common.int;
        let mut ops = vec![self.operand(&args[0], None)?];
        // A `&T` atomics work on T; a bare `ptr` keeps the old meaning, an `int` cell.
        let val_ty = self.cx.ref_inner(ops[0].ty()).unwrap_or(int_ty);
        let ok = match self.cx.ty_kind(val_ty) {
            TyKind::Int(crate::ir::IntTy::I64 | crate::ir::IntTy::I32 | crate::ir::IntTy::U32 | crate::ir::IntTy::U64) => true,
            TyKind::Bool | TyKind::Ptr => !matches!(op, AtomicOp::Add | AtomicOp::Sub),
            _ => false,
        };
        if !ok {
            let what = crate::ir::dump::ty_string(self.cx, val_ty);
            return Err(CheckError::new(
                self.path,
                args[0].span,
                format!("`@{}` cannot work on `{what}`", op.name()),
            )
            .help("use a `&int`, `&i32`, `&u32` or `&u64`; `bool` and `ptr` take load, store, exchange and cas only"));
        }
        for a in &args[1..] {
            ops.push(self.operand(a, Some(val_ty))?);
        }
        let ty = match op {
            AtomicOp::Cas => self.cx.common.bool,
            AtomicOp::Store => int_ty,
            _ => val_ty,
        };
        Ok(Rvalue::atomic(span, ty, op, order, ops))
    }

    pub(super) fn lookup_method(
        &mut self,
        sid: StructId,
        name: &str,
        type_args: Vec<Ty>,
        span: Span,
    ) -> Result<FuncId, CheckError> {
        if type_args.is_empty() {
            return match self.methods.get(&(sid.0, name.to_string())).copied() {
                Some(id) => self.script_method_visible(sid, id, name, span),
                None => Err(self.no_method(span, sid, name)),
            };
        }
        let key = (sid.0, name.to_string(), type_args.clone());
        if let Some(&id) = self.method_insts.get(&key) {
            return self.script_method_visible(sid, id, name, span);
        }
        let temp = match self.method_temps.get(&(sid.0, name.to_string())).cloned() {
            Some(f) => f,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("no generic method `{name}`"),
                ));
            }
        };
        if self.cx.struct_def(sid).script.is_some() && self.script_sid != Some(sid) && !temp.vis_pub
        {
            return Err(CheckError::new(
                self.path,
                span,
                format!("`{name}` is private to this script"),
            )
            .help("add `pub` to expose this function"));
        }
        if temp.type_params.len() != type_args.len() {
            return Err(CheckError::new(
                self.path,
                span,
                format!(
                    "`{name}` takes {} type args, got {}",
                    temp.type_params.len(),
                    type_args.len()
                ),
            ));
        }
        self.check_call_bounds(span, &format!("`{name}`"), &temp.type_params, &temp.bounds, &type_args)?;
        let mut subst = HashMap::new();
        for (pname, ty) in &self.cx.struct_def(sid).ty_args {
            subst.insert(pname.clone(), *ty);
        }
        for (p, a) in temp.type_params.iter().zip(type_args.iter()) {
            subst.insert(p.name.clone(), *a);
        }
        let id = {
            let mut types = self.types.clone();
            let struct_name = self.cx.name_str(self.cx.struct_def(sid).name).to_string();
            let mut enum_temps = self.enum_temps.clone();
            let mut struct_temps = self.struct_temps.clone();
            if let Some(env) = self.struct_temp_env.get(&struct_name) {
                for (k, v) in &env.types {
                    types.entry(k.clone()).or_insert(*v);
                }
                for (k, v) in &env.enum_temps {
                    enum_temps.entry(k.clone()).or_insert_with(|| v.clone());
                }
                for (k, v) in &env.struct_temps {
                    struct_temps.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
            let mut env = GenEnv {
                types: &types,
                subst: &subst,
                enum_temps: &enum_temps,
                enum_insts: self.enum_insts,
                struct_temps: &struct_temps,
                struct_temp_env: self.struct_temp_env,
                struct_insts: self.struct_insts,
                methods: self.methods,
                method_temps: self.method_temps,
                method_insts: self.method_insts,
                fn_substs: self.fn_substs,
                pending_mono: self.pending_mono,
                module: self.module_names,
                ir_enums: self.ir_enums,
                ir_structs: self.ir_structs,
                ir_funcs: self.ir_funcs,
            };
            alloc_fn(
                self.cx,
                self.path,
                self.module_names,
                &temp,
                Some(sid),
                &HashMap::new(),
                &mut env,
            )?
        };
        self.method_insts.insert(key, id);
        self.ir_funcs.push(id);
        self.fn_substs.insert(id, subst.clone());
        self.pending_mono.push((id, temp, subst));
        self.script_method_visible(sid, id, name, span)
    }

    pub(super) fn script_method_visible(
        &self,
        sid: StructId,
        id: FuncId,
        name: &str,
        span: Span,
    ) -> Result<FuncId, CheckError> {
        if let Some(meta) = &self.cx.struct_def(sid).script {
            if self.script_sid != Some(sid) && self.cx.func(id).vis != crate::ir::entity::Vis::Pub {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("`{name}` is private to {}", meta.path),
                )
                .help("add `pub` to expose this function"));
            }
        }
        Ok(id)
    }
}
