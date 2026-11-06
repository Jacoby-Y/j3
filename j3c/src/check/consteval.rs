use super::*;

pub(super) fn collect_consts(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let items: Vec<ConstItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Const(c) => Some(c.clone()),
            _ => None,
        })
        .collect();

    for c in items {
        if matches!(c.value.kind, ExprKind::ScriptType(_)) {
            continue;
        }
        // Array consts are read-only static data; `collect_globals` makes them.
        if matches!(c.value.kind, ExprKind::ArrayLit { .. }) {
            continue;
        }
        if mods[i].consts.contains_key(&c.name.name) {
            return Err(CheckError::new(
                path.clone(),
                c.name.span,
                format!("duplicate const `{}`", c.name.name),
            ));
        }
        let mut val = eval_const(&c.value, &mods[i].consts, cx, &path)?;
        if let Some(t) = &c.ty {
            let types = mods[i].types.clone();
            let ty = resolve_type(cx, t, &path, &types)?;
            if ty != val.ty {
                if is_int(cx, ty) && matches!(val.kind, ConstPayload::Int(_)) {
                    val.ty = ty;
                } else if is_float(cx, ty)
                    && matches!(val.kind, ConstPayload::Float(_) | ConstPayload::Int(_))
                {
                    if let ConstPayload::Int(v) = val.kind {
                        val.kind = ConstPayload::Float(v as f64);
                    }
                    val.ty = ty;
                } else {
                    return Err(CheckError::new(
                        path.clone(),
                        c.span,
                        "const type does not match initializer",
                    ));
                }
            }
        }
        mods[i].consts.insert(c.name.name.clone(), val);
    }
    Ok(())
}

pub(super) fn fold_const_bin(
    op: parse::BinOp,
    a: i64,
    b: i64,
    path: &str,
    span: Span,
) -> Result<Option<i64>, CheckError> {
    let ir_op = match op {
        parse::BinOp::Add => crate::ir::op::BinOp::Add,
        parse::BinOp::Sub => crate::ir::op::BinOp::Sub,
        parse::BinOp::Mul => crate::ir::op::BinOp::Mul,
        parse::BinOp::Div => crate::ir::op::BinOp::Div,
        parse::BinOp::Rem => crate::ir::op::BinOp::Rem,
        // amount masked to 0..63, as the backends mask it to the type's width
        parse::BinOp::Shl => return Ok(Some(a.wrapping_shl(b as u32))),
        parse::BinOp::Shr => return Ok(Some(a.wrapping_shr(b as u32))),
        parse::BinOp::BitAnd => return Ok(Some(a & b)),
        parse::BinOp::BitOr => return Ok(Some(a | b)),
        parse::BinOp::BitXor => return Ok(Some(a ^ b)),
        _ => return Ok(None),
    };
    match crate::ir::wrap::fold_bin(ir_op, a, b) {
        Some(v) => Ok(Some(v)),
        None if matches!(op, parse::BinOp::Div | parse::BinOp::Rem) && b == 0 => Err(CheckError::new(
            path,
            span,
            "division by zero in const initializer",
        )),
        None => Ok(None),
    }
}

pub(super) fn collect_globals(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let types = mods[i].types.clone();
    // `#[singleton] let` items, then `const NAME: [N]T = [..]` (read-only).
    let mut items: Vec<(ModLetItem, bool)> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Let(l) => Some((l.clone(), false)),
            _ => None,
        })
        .collect();
    for it in &mods[i].ast.items {
        if let Item::Const(c) = it {
            if !matches!(c.value.kind, ExprKind::ArrayLit { .. }) {
                continue;
            }
            let Some(ty) = &c.ty else {
                return Err(CheckError::new(
                    path.clone(),
                    c.name.span,
                    "array const needs a type: `const NAME: [N]T = [..];`",
                ));
            };
            items.push((
                ModLetItem {
                    span: c.span,
                    doc: c.doc.clone(),
                    name: c.name.clone(),
                    ty: ty.clone(),
                    value: Some(c.value.clone()),
                    thread: false,
                    uniform: false,
                },
                true,
            ));
        }
    }
    for (l, readonly) in items {
        if mods[i].globals.contains_key(&l.name.name)
            || mods[i].funcs.contains_key(&l.name.name)
            || mods[i].consts.contains_key(&l.name.name)
        {
            return Err(CheckError::new(
                path.clone(),
                l.name.span,
                format!("`{}` already in scope", l.name.name),
            ));
        }
        let ty = resolve_type(cx, &l.ty, &path, &types)?;
        if readonly && !matches!(cx.ty_kind(ty), TyKind::Array { .. }) {
            return Err(CheckError::new(path.clone(), l.ty.span, "array const needs an array type `[N]T`"));
        }
        if l.value.is_some() && matches!(cx.ty_kind(ty), TyKind::Vec(_)) {
            return Err(CheckError::new(path.clone(), l.name.span, "a vec4f/vec4i global cannot have an initializer yet")
                .help("declare it without a value (it starts as zeros) and assign it at startup"));
        }
        let init = match &l.value {
            Some(v) => {
                let folded = fold_global_init(cx, v, ty, &types, &mods[i].consts, &path)?;
                // A read-only table stays real data even when all zero.
                if !readonly && const_init_is_zero(&folded) {
                    None
                } else {
                    Some(folded)
                }
            }
            None => None,
        };
        let gname = cx.intern_name(&l.name.name);
        let id = cx.alloc_global(GlobalDef {
            id: GlobalId(0),
            doc: l.doc.clone(),
            module: names.clone(),
            name: gname,
            ty,
            span: l.span,
            init,
            thread: l.thread,
            readonly,
        });
        mods[i].globals.insert(l.name.name, id);
        mods[i].ir.globals.push(id);
    }
    Ok(())
}

pub(super) fn fold_global_init(
    cx: &IrCx,
    expr: &Expr,
    expected: Ty,
    types: &HashMap<String, Ty>,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<ConstInit, CheckError> {
    match &expr.kind {
        ExprKind::Int(v) => {
            if matches!(cx.ty_kind(expected), TyKind::Ptr | TyKind::CStr) && *v == 0 {
                return Ok(ConstInit::Scalar(Operand::null(expr.span, expected)));
            }
            if is_int(cx, expected) || expected == cx.common.int {
                return Ok(ConstInit::Scalar(Operand::int(
                    expr.span, expected, *v as u64,
                )));
            }
            if expected == cx.common.f32 {
                return Ok(ConstInit::Scalar(Operand::f32_const(
                    expr.span, expected, *v as f32,
                )));
            }
            if is_float(cx, expected) {
                return Ok(ConstInit::Scalar(Operand::f64_const(
                    expr.span, expected, *v as f64,
                )));
            }
            Err(CheckError::new(
                path,
                expr.span,
                "singleton init type mismatch",
            ))
        }
        ExprKind::Float(v) => {
            if expected == cx.common.f32 {
                Ok(ConstInit::Scalar(Operand::f32_const(
                    expr.span, expected, *v as f32,
                )))
            } else if is_float(cx, expected) {
                Ok(ConstInit::Scalar(Operand::f64_const(
                    expr.span, expected, *v,
                )))
            } else {
                Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton init type mismatch",
                ))
            }
        }
        ExprKind::Bool(v) => {
            if expected != cx.common.bool {
                return Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton init type mismatch",
                ));
            }
            Ok(ConstInit::Scalar(Operand::bool_const(
                expr.span, expected, *v,
            )))
        }
        ExprKind::Interpolate { .. } | ExprKind::String(_)
            if matches!(cx.ty_kind(expected), TyKind::String) =>
        {
            let bytes = match &expr.kind {
                ExprKind::String(b) => Some(b.clone()),
                ExprKind::Interpolate { parts } => parts.iter().try_fold(Vec::new(), |mut acc, p| match p {
                    parse::InterpPart::Lit(b) => {
                        acc.extend_from_slice(b);
                        Some(acc)
                    }
                    _ => None,
                }),
                _ => None,
            };
            match bytes {
                Some(b) => Ok(ConstInit::Scalar(Operand::bytes(expr.span, expected, &b))),
                None => Err(CheckError::new(path, expr.span, "singleton init must be a constant")),
            }
        }
        ExprKind::Name(n) => match consts.get(&n.name) {
            Some(ConstVal {
                kind: ConstPayload::Int(v),
                ..
            }) => fold_global_init(
                cx,
                &Expr {
                    span: n.span,
                    kind: ExprKind::Int(*v),
                },
                expected,
                types,
                consts,
                path,
            ),
            _ => Err(CheckError::new(
                path,
                n.span,
                "singleton init must be a constant",
            )),
        },
        ExprKind::Unary {
            op: parse::UnaryOp::Neg,
            expr: inner,
        } => match fold_global_init(cx, inner, expected, types, consts, path)? {
            ConstInit::Scalar(Operand::Const(c)) => match c.kind {
                crate::ir::operand::ConstKind::Int(bits) => {
                    let neg = (bits as i64).wrapping_neg();
                    Ok(ConstInit::Scalar(Operand::int(
                        expr.span, expected, neg as u64,
                    )))
                }
                crate::ir::operand::ConstKind::F32Bits(b) => Ok(ConstInit::Scalar(
                    Operand::f32_const(expr.span, expected, -f32::from_bits(b)),
                )),
                crate::ir::operand::ConstKind::F64Bits(b) => Ok(ConstInit::Scalar(
                    Operand::f64_const(expr.span, expected, -f64::from_bits(b)),
                )),
                _ => Err(CheckError::new(
                    path,
                    expr.span,
                    "singleton init must be a constant",
                )),
            },
            _ => Err(CheckError::new(
                path,
                expr.span,
                "singleton init must be a constant",
            )),
        },
        ExprKind::StructLit { name, fields, .. } => {
            let sid = match cx.ty_kind(expected) {
                TyKind::Struct(id) => *id,
                _ => {
                    return Err(CheckError::new(
                        path,
                        name.span,
                        "singleton init type mismatch",
                    ));
                }
            };
            let def = cx.struct_def(sid);
            let mut out = Vec::new();
            for f in &def.fields {
                let fname = cx.name_str(f.name).to_string();
                let found = fields.iter().find(|(n, _)| n.name == fname);
                let val = match found {
                    Some((_, e)) => fold_global_init(cx, e, f.ty, types, consts, path)?,
                    None => zero_init_for(cx, f.ty, expr.span),
                };
                out.push((fname, val));
            }
            Ok(ConstInit::Fields(out))
        }
        ExprKind::ArrayLit { elems } => {
            let (elem_ty, len) = match cx.ty_kind(expected) {
                TyKind::Array { elem, len } => (*elem, *len),
                _ => {
                    return Err(CheckError::new(
                        path,
                        expr.span,
                        "singleton init type mismatch",
                    ));
                }
            };
            if elems.len() as u64 != len {
                return Err(CheckError::new(
                    path,
                    expr.span,
                    format!("array length mismatch: the type has {len} elements, the literal has {}", elems.len()),
                ));
            }
            let mut out = Vec::new();
            for e in elems {
                out.push(fold_global_init(cx, e, elem_ty, types, consts, path)?);
            }
            Ok(ConstInit::Elems(out))
        }
        ExprKind::Call { .. } | ExprKind::Syscall { .. } | ExprKind::Atomic { .. } => Err(CheckError::new(
            path,
            expr.span,
            "singleton init cannot call",
        )),
        _ => Err(CheckError::new(
            path,
            expr.span,
            "singleton init must be a constant",
        )),
    }
}

pub(super) fn zero_init_for(cx: &IrCx, ty: Ty, span: Span) -> ConstInit {
    if matches!(cx.ty_kind(ty), TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)) {
        return ConstInit::Scalar(Operand::null(span, ty));
    }
    if ty == cx.common.bool {
        return ConstInit::Scalar(Operand::bool_const(span, ty, false));
    }
    if is_int(cx, ty) {
        return ConstInit::Scalar(Operand::int(span, ty, 0));
    }
    if ty == cx.common.f32 {
        return ConstInit::Scalar(Operand::f32_const(span, ty, 0.0));
    }
    if is_float(cx, ty) {
        return ConstInit::Scalar(Operand::f64_const(span, ty, 0.0));
    }
    if let TyKind::Struct(id) = cx.ty_kind(ty) {
        let mut fields = Vec::new();
        for f in &cx.struct_def(*id).fields {
            fields.push((
                cx.name_str(f.name).to_string(),
                zero_init_for(cx, f.ty, span),
            ));
        }
        return ConstInit::Fields(fields);
    }
    if let TyKind::Array { elem, len } = cx.ty_kind(ty) {
        let z = zero_init_for(cx, *elem, span);
        return ConstInit::Elems(vec![z; *len as usize]);
    }
    ConstInit::Scalar(Operand::int(span, ty, 0))
}

pub(super) fn const_init_is_zero(init: &ConstInit) -> bool {
    match init {
        ConstInit::Scalar(Operand::Const(c)) => match &c.kind {
            crate::ir::operand::ConstKind::Int(0) => true,
            crate::ir::operand::ConstKind::Bool(false) => true,
            crate::ir::operand::ConstKind::Null => true,
            crate::ir::operand::ConstKind::F32Bits(0) => true,
            crate::ir::operand::ConstKind::F64Bits(0) => true,
            _ => false,
        },
        ConstInit::Scalar(_) => false,
        ConstInit::Fields(fs) => fs.iter().all(|(_, v)| const_init_is_zero(v)),
        ConstInit::Elems(es) => es.iter().all(const_init_is_zero),
    }
}

pub(super) fn eval_const(
    expr: &parse::Expr,
    consts: &HashMap<String, ConstVal>,
    cx: &IrCx,
    path: &str,
) -> Result<ConstVal, CheckError> {
    let folded = expr.folded_literal();
    match &folded.kind {
        parse::ExprKind::String(b) => Ok(ConstVal {
            ty: cx.common.string,
            kind: ConstPayload::String(b.clone()),
        }),
        parse::ExprKind::Int(v) => Ok(ConstVal {
            ty: cx.common.int,
            kind: ConstPayload::Int(*v),
        }),
        parse::ExprKind::Float(v) => Ok(ConstVal {
            ty: cx.common.float,
            kind: ConstPayload::Float(*v),
        }),
        parse::ExprKind::Bool(v) => Ok(ConstVal {
            ty: cx.common.bool,
            kind: ConstPayload::Bool(*v),
        }),
        parse::ExprKind::Name(n) => match consts.get(&n.name) {
            Some(c) => Ok(c.clone()),
            None => Err(CheckError::new(
                path,
                n.span,
                format!("unknown const `{}`", n.name),
            )),
        },
        parse::ExprKind::Binary { op, lhs, rhs } => {
            let a = eval_int(lhs, consts, path)?;
            let b = eval_int(rhs, consts, path)?;
            let v = match fold_const_bin(*op, a, b, path, expr.span)? {
                Some(v) => v,
                None => {
                    return Err(CheckError::new(
                        path,
                        expr.span,
                        "const initializer must be an integer expression",
                    ));
                }
            };
            Ok(ConstVal {
                ty: cx.common.int,
                kind: ConstPayload::Int(v),
            })
        }
        parse::ExprKind::Unary {
            op: parse::UnaryOp::Neg,
            expr: inner,
        } => {
            let v = eval_const(inner, consts, cx, path)?;
            match v.kind {
                ConstPayload::Int(n) => Ok(ConstVal {
                    ty: cx.common.int,
                    kind: ConstPayload::Int(n.wrapping_neg()),
                }),
                ConstPayload::Float(n) => Ok(ConstVal {
                    ty: v.ty,
                    kind: ConstPayload::Float(-n),
                }),
                _ => Err(CheckError::new(
                    path,
                    expr.span,
                    "const initializer must be a number",
                )),
            }
        }
        parse::ExprKind::Unary {
            op: parse::UnaryOp::BitNot,
            expr: inner,
        } => match eval_const(inner, consts, cx, path)?.kind {
            ConstPayload::Int(n) => Ok(ConstVal { ty: cx.common.int, kind: ConstPayload::Int(!n) }),
            _ => Err(CheckError::new(path, expr.span, "`~` in a const initializer needs an integer")),
        },
        parse::ExprKind::Unary {
            op: parse::UnaryOp::Not,
            expr: inner,
        } => {
            let v = eval_const(inner, consts, cx, path)?;
            match v.kind {
                ConstPayload::Bool(b) => Ok(ConstVal {
                    ty: cx.common.bool,
                    kind: ConstPayload::Bool(!b),
                }),
                _ => Err(CheckError::new(
                    path,
                    expr.span,
                    "const initializer must be a bool",
                )),
            }
        }
        parse::ExprKind::Call { func, .. }
            if matches!(&func.kind, parse::ExprKind::Name(n) if n.name == "vec4f" || n.name == "vec4i") =>
        {
            Err(CheckError::new(path, expr.span, "a vec4f/vec4i const is not supported yet")
                .help("build the vector in a function, or keep the four lanes as separate consts"))
        }
        _ => Err(CheckError::new(
            path,
            expr.span,
            "const initializer must be an int, float, bool, or string",
        )),
    }
}

pub(super) fn eval_int(
    expr: &parse::Expr,
    consts: &HashMap<String, ConstVal>,
    path: &str,
) -> Result<i64, CheckError> {
    match &expr.kind {
        parse::ExprKind::Int(v) => Ok(*v),
        parse::ExprKind::Name(n) => match consts.get(&n.name) {
            Some(ConstVal {
                kind: ConstPayload::Int(v),
                ..
            }) => Ok(*v),
            Some(_) => Err(CheckError::new(
                path,
                n.span,
                format!("`{}` is not an int const", n.name),
            )),
            None => Err(CheckError::new(
                path,
                n.span,
                format!("unknown const `{}`", n.name),
            )),
        },
        parse::ExprKind::Binary { op, lhs, rhs } => {
            let a = eval_int(lhs, consts, path)?;
            let b = eval_int(rhs, consts, path)?;
            match fold_const_bin(*op, a, b, path, expr.span)? {
                Some(v) => Ok(v),
                None => Err(CheckError::new(
                    path,
                    expr.span,
                    "const initializer must be an integer expression",
                )),
            }
        }
        parse::ExprKind::Unary {
            op: parse::UnaryOp::Neg,
            expr,
        } => Ok(eval_int(expr, consts, path)?.wrapping_neg()),
        parse::ExprKind::Unary {
            op: parse::UnaryOp::BitNot,
            expr,
        } => Ok(!eval_int(expr, consts, path)?),
        _ => Err(CheckError::new(
            path,
            expr.span,
            "const initializer must be an integer expression",
        )),
    }
}
