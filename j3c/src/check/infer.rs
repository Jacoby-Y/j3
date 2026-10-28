use super::*;

pub(super) fn infer_omitted_rets(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let types = mods[i].types.clone();
    let consts = mods[i].consts.clone();
    let methods = mods[i].methods.clone();
    let lib_ns = mods[i].lib_ns.clone();
    let mut funcs = mods[i].funcs.clone();
    for (name, id) in &mods[i].imports {
        funcs.insert(name.clone(), *id);
    }
    let items = mods[i].ast.items.clone();

    let mut pending: Vec<(FuncId, FnItem)> = Vec::new();
    for it in &items {
        match it {
            Item::Fn(f) if f.ret.is_none() => {
                pending.push((mods[i].funcs[&f.name.name], f.clone()));
            }
            Item::Struct(s) => {
                if !s.type_params.is_empty() {
                    continue;
                }
                let sid = mods[i].structs[&s.name.name];
                for m in &s.methods {
                    if m.ret.is_none() && m.type_params.is_empty() {
                        pending.push((mods[i].methods[&(sid.0, m.name.name.clone())], m.clone()));
                    }
                }
            }
            _ => {}
        }
    }

    for (id, f) in pending {
        let (params, old_ret) = match cx.fn_sig(cx.func(id).ty) {
            Some(s) => (s.0.clone(), s.1),
            None => {
                return Err(CheckError::new(
                    path.clone(),
                    f.span,
                    "function is missing a type",
                ));
            }
        };
        let inferred = infer_fn_ret(
            cx, &f, &params, &types, &funcs, &methods, &lib_ns, &consts, &path,
        )?;
        if inferred == old_ret {
            continue;
        }
        let ty = cx.ty_fn(params, inferred);
        cx.func_mut(id).ty = ty;
    }
    Ok(())
}

pub(super) fn infer_fn_ret(
    cx: &IrCx,
    f: &FnItem,
    params: &[Ty],
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<Ty, CheckError> {
    let mut locals = HashMap::new();
    // Signature collection already instantiated generic parameter types.
    for (p, ty) in f.params.iter().zip(params) {
        locals.insert(p.name.name.clone(), *ty);
    }
    let mut seen: Option<Ty> = None;
    collect_return_tys(
        &f.body, cx, &locals, types, funcs, methods, lib_ns, consts, path, &mut seen,
    )?;
    Ok(seen.unwrap_or(cx.common.void))
}

pub(super) fn collect_return_tys(
    block: &parse::Block,
    cx: &IrCx,
    locals: &HashMap<String, Ty>,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
    seen: &mut Option<Ty>,
) -> Result<(), CheckError> {
    for stmt in &block.stmts {
        match stmt {
            parse::Stmt::Return(s) => match &s.value {
                None => {
                    if let Some(t) = *seen {
                        if t != cx.common.void {
                            return Err(CheckError::new(path, s.span, "return type mismatch"));
                        }
                    } else {
                        *seen = Some(cx.common.void);
                    }
                }
                Some(e) => {
                    let t = peek_ty(cx, e, locals, types, funcs, methods, lib_ns, consts, path)?;
                    match *seen {
                        None => *seen = Some(t),
                        Some(old) if old != t => {
                            return Err(CheckError::new(path, s.span, "return type mismatch"));
                        }
                        _ => {}
                    }
                }
            },
            parse::Stmt::If(s) => {
                collect_return_tys(
                    &s.then_block,
                    cx,
                    locals,
                    types,
                    funcs,
                    methods,
                    lib_ns,
                    consts,
                    path,
                    seen,
                )?;
                if let Some(b) = &s.else_block {
                    collect_return_tys(
                        b, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                    )?;
                }
            }
            parse::Stmt::While(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::For(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::ForEach(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::Defer(s) => {
                collect_return_tys(
                    &s.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::Block(b) => {
                collect_return_tys(
                    b, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                )?;
            }
            parse::Stmt::UseLocal(_) => {}
            parse::Stmt::Match(s) => {
                for arm in &s.arms {
                    collect_return_tys(
                        &arm.body, cx, locals, types, funcs, methods, lib_ns, consts, path, seen,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn peek_ty(
    cx: &IrCx,
    expr: &Expr,
    locals: &HashMap<String, Ty>,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<Ty, CheckError> {
    match &expr.kind {
        ExprKind::ScriptType(_) | ExprKind::ScriptLit { .. } => Err(CheckError::new(
            path,
            expr.span,
            "script type is not available here",
        )),
        ExprKind::Int(_) => Ok(cx.common.int),
        ExprKind::Float(_) => Ok(cx.common.float),
        ExprKind::Bool(_) => Ok(cx.common.bool),
        ExprKind::String(_) | ExprKind::Interpolate { .. } => Ok(cx.common.string),
        ExprKind::Name(n) => {
            if let Some(&t) = locals.get(&n.name) {
                return Ok(t);
            }
            if let Some(c) = consts.get(&n.name) {
                return Ok(c.ty);
            }
            Err(CheckError::new(
                path,
                n.span,
                format!("unknown name `{}`", n.name),
            ))
        }
        ExprKind::StructLit { name, .. } => match types.get(&name.name).copied() {
            Some(t) => Ok(t),
            None => Err(CheckError::new(
                path,
                name.span,
                format!("unknown type `{}`", name.name),
            )),
        },
        ExprKind::Field { base, field } => {
            if let ExprKind::Name(n) = &base.kind {
                if !locals.contains_key(&n.name) {
                    if let Some(c) = consts.get(&qual_key(&n.name, &field.name)) {
                        return Ok(c.ty);
                    }
                }
            }
            let mut base_ty = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            while let Some(inner) = cx.ref_inner(base_ty) {
                base_ty = inner;
            }
            match cx.ty_kind(base_ty) {
                TyKind::Struct(id) => match field_path(cx, *id, &field.name) {
                    Some(p) => Ok(cx.field_def(*p.last().unwrap()).ty),
                    None => Err(CheckError::new(
                        path,
                        field.span,
                        format!("no field `{}`", field.name),
                    )),
                },
                TyKind::Array { .. } if field.name == "len" => Ok(cx.common.int),
                TyKind::String | TyKind::Slice { .. } => match field.name.as_str() {
                    "ptr" => Ok(cx
                        .lookup_ty(&TyKind::Ref(cx.common.u8))
                        .unwrap_or(cx.common.ptr)),
                    "len" => Ok(cx.common.int),
                    _ => Err(CheckError::new(
                        path,
                        field.span,
                        format!("no field `{}`", field.name),
                    )),
                },
                _ => Err(CheckError::new(
                    path,
                    field.span,
                    format!("no field `{}`", field.name),
                )),
            }
        }
        ExprKind::Call { func, args, .. } => {
            if let ExprKind::Name(n) = &func.kind {
                if args.len() == 1 {
                    if let Some(t) = conv_ty_name(cx, &n.name) {
                        return Ok(t);
                    }
                }
            }
            let id = peek_callee(
                cx, func, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.fn_sig(cx.func(id).ty) {
                Some((_, ret)) => Ok(ret),
                None => Err(CheckError::new(path, func.span, "not a function")),
            }
        }
        ExprKind::Binary { op, lhs, rhs } => {
            let lt = peek_ty(cx, lhs, locals, types, funcs, methods, lib_ns, consts, path)?;
            match op {
                parse::BinOp::And
                | parse::BinOp::Or
                | parse::BinOp::Eq
                | parse::BinOp::Ne
                | parse::BinOp::Lt
                | parse::BinOp::Le
                | parse::BinOp::Gt
                | parse::BinOp::Ge => Ok(cx.common.bool),
                parse::BinOp::Add
                | parse::BinOp::Sub
                | parse::BinOp::Mul
                | parse::BinOp::Div
                | parse::BinOp::Rem => {
                    if let Some(id) = find_operator(cx, lt, *op, methods) {
                        return match cx.fn_sig(cx.func(id).ty) {
                            Some((_, ret)) => Ok(ret),
                            None => Err(CheckError::new(path, expr.span, "not a function")),
                        };
                    }
                    let _ = rhs;
                    if is_float(cx, lt) {
                        Ok(lt)
                    } else {
                        Ok(cx.common.int)
                    }
                }
            }
        }
        ExprKind::Unary { op, expr: inner } => match op {
            parse::UnaryOp::Neg => peek_ty(
                cx, inner, locals, types, funcs, methods, lib_ns, consts, path,
            ),
            parse::UnaryOp::Not => Ok(cx.common.bool),
            parse::UnaryOp::Ref => {
                let t = peek_ty(
                    cx, inner, locals, types, funcs, methods, lib_ns, consts, path,
                )?;
                match cx.lookup_ty(&TyKind::Ref(t)) {
                    Some(r) => Ok(r),
                    None => Err(CheckError::new(path, expr.span, "cannot name `&` type yet")),
                }
            }
            parse::UnaryOp::Deref => {
                let t = peek_ty(
                    cx, inner, locals, types, funcs, methods, lib_ns, consts, path,
                )?;
                match cx.ref_inner(t) {
                    Some(inner) => Ok(inner),
                    None => Err(CheckError::new(path, expr.span, "deref of non-ref")),
                }
            }
        },
        ExprKind::Cast { ty, .. } => resolve_type_ref(cx, ty, path, types),
        ExprKind::Question { expr } => {
            let t = peek_ty(
                cx, expr, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.ty_kind(t) {
                TyKind::Enum(id) => match &cx.enum_def(*id).try_catch {
                    Some(tc) => {
                        match cx.enum_def(*id).variants[tc.success.index as usize].payload {
                            Some(p) => Ok(p),
                            None => Ok(cx.common.void),
                        }
                    }
                    None => Err(CheckError::new(path, expr.span, "`?` needs `#[TryCatch]`")),
                },
                _ => Err(CheckError::new(path, expr.span, "`?` on non-enum")),
            }
        }
        ExprKind::Syscall { .. } => Ok(cx.common.int),
        ExprKind::Atomic { op, .. } => Ok(if *op == crate::ir::op::AtomicOp::Cas {
            cx.common.bool
        } else {
            cx.common.int
        }),
        ExprKind::SizeOf(_) => Ok(cx.common.int),
        ExprKind::UiChoice { .. } => Ok(cx.common.bool),
        ExprKind::SliceFrom { .. } => Err(CheckError::new(
            path,
            expr.span,
            "`@slice` needs an expected type",
        )),
        ExprKind::TypePath { name, .. } => Err(CheckError::new(
            path,
            expr.span,
            format!("`{}` is a type, not a value", name.name),
        )),
        ExprKind::SliceLit { elem, .. } => {
            let e = resolve_type_ref(cx, elem, path, types)?;
            match cx.lookup_ty(&TyKind::Slice { elem: e }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, expr.span, "unknown slice type")),
            }
        }
        ExprKind::ArrayLit { elems } => {
            if elems.is_empty() {
                return Err(CheckError::new(path, expr.span, "cannot infer empty array"));
            }
            let elem = peek_ty(
                cx, &elems[0], locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.lookup_ty(&TyKind::Array {
                elem,
                len: elems.len() as u64,
            }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, expr.span, "unknown array type")),
            }
        }
        ExprKind::Index { base, .. } => {
            let bt = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match cx.elem_ty(bt) {
                Some(e) => Ok(e),
                None => Err(CheckError::new(path, expr.span, "index of non-array/slice")),
            }
        }
        ExprKind::Slice { base, .. } => {
            let bt = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            let elem = match cx.elem_ty(bt) {
                Some(e) => e,
                None => {
                    return Err(CheckError::new(path, expr.span, "slice of non-array/slice"));
                }
            };
            match cx.lookup_ty(&TyKind::Slice { elem }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, expr.span, "unknown slice type")),
            }
        }
        ExprKind::If {
            then_expr,
            else_expr,
            ..
        } => {
            let t = peek_ty(
                cx, then_expr, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            let e = peek_ty(
                cx, else_expr, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            if t != e {
                return Err(CheckError::new(
                    path,
                    expr.span,
                    "if-expression branches have different types",
                ));
            }
            Ok(t)
        }
    }
}

pub(super) fn conv_ty_name(cx: &IrCx, name: &str) -> Option<Ty> {
    Some(match name {
        "i8" => cx.common.i8,
        "i16" => cx.common.i16,
        "i32" => cx.common.i32,
        "i64" | "int" => cx.common.int,
        "u8" => cx.common.u8,
        "u16" => cx.common.u16,
        "u32" => cx.common.u32,
        "u64" => cx.common.u64,
        "f32" => cx.common.f32,
        "f64" | "float" => cx.common.float,
        _ => return None,
    })
}

pub(super) fn peek_callee(
    cx: &IrCx,
    func: &Expr,
    locals: &HashMap<String, Ty>,
    types: &HashMap<String, Ty>,
    funcs: &HashMap<String, FuncId>,
    methods: &HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<FuncId, CheckError> {
    match &func.kind {
        ExprKind::Name(n) => match funcs.get(&n.name).copied() {
            Some(id) => Ok(id),
            None => Err(CheckError::new(
                path,
                n.span,
                format!("unknown function `{}`", n.name),
            )),
        },
        ExprKind::Field { base, field } => {
            if let ExprKind::Name(n) = &base.kind {
                if let Some(ns) = lib_ns.get(&n.name) {
                    return match ns.get(&field.name).copied() {
                        Some(id) => Ok(id),
                        None => Err(CheckError::new(
                            path,
                            field.span,
                            format!("`{}` has no `{}`", n.name, field.name),
                        )),
                    };
                }
            }
            let base_ty = peek_ty(
                cx, base, locals, types, funcs, methods, lib_ns, consts, path,
            )?;
            match find_method(cx, base_ty, &field.name, methods) {
                Some(id) => Ok(id),
                None => Err(CheckError::new(
                    path,
                    field.span,
                    format!("no method `{}`", field.name),
                )),
            }
        }
        _ => Err(CheckError::new(
            path,
            func.span,
            "cannot call this expression",
        )),
    }
}
