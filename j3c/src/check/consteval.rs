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
        // `@embed*` consts are read-only globals; `collect_embeds` makes them.
        if matches!(c.value.kind, ExprKind::ScriptType(_) | ExprKind::Embed { .. }) {
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
        // A `u64` const folds on the 64 bits as unsigned: literals above `i64::MAX` (`0xFFFF_FFFF_FFFF_FFFF & X`),
        // `/`, `%` and `>>` unsigned. Other int consts fold as `int`.
        if let Some(t) = &c.ty {
            if resolve_type(cx, t, &path, &mods[i].types)? == cx.common.u64 && !matches!(c.value.kind, ExprKind::Float(_)) {
                let v = eval_int(&c.value, &mods[i].consts, &path, true)?;
                mods[i].consts.insert(c.name.name.clone(), ConstVal { ty: cx.common.u64, kind: ConstPayload::Int(v) });
                continue;
            }
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
    unsigned: bool,
) -> Result<Option<i64>, CheckError> {
    if unsigned && matches!(op, parse::BinOp::Div | parse::BinOp::Rem | parse::BinOp::Shr) {
        let (a, b) = (a as u64, b as u64);
        return match op {
            parse::BinOp::Shr => Ok(Some(a.wrapping_shr(b as u32) as i64)),
            _ if b == 0 => Err(CheckError::new(path, span, "division by zero in const initializer")),
            parse::BinOp::Div => Ok(Some((a / b) as i64)),
            _ => Ok(Some((a % b) as i64)),
        };
    }
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

pub(super) fn eval_const(
    expr: &parse::Expr,
    consts: &HashMap<String, ConstVal>,
    cx: &IrCx,
    path: &str,
) -> Result<ConstVal, CheckError> {
    if let parse::ExprKind::Char(v) = expr.kind {
        let ty = if v < 0x80 { cx.common.u8 } else { cx.common.u32 };
        return Ok(ConstVal { ty, kind: ConstPayload::Int(v as i64) });
    }
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
        parse::ExprKind::BigInt(v) => Err(crate::check::infer::big_int_err(path, expr.span, *v)),
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
            let a = eval_int(lhs, consts, path, false)?;
            let b = eval_int(rhs, consts, path, false)?;
            let v = match fold_const_bin(*op, a, b, path, expr.span, false)? {
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
    unsigned: bool,
) -> Result<i64, CheckError> {
    match &expr.kind {
        parse::ExprKind::Int(v) => Ok(*v),
        parse::ExprKind::BigInt(v) if unsigned => Ok(*v as i64),
        parse::ExprKind::BigInt(v) => Err(crate::check::infer::big_int_err(path, expr.span, *v)),
        parse::ExprKind::Char(v) => Ok(*v as i64),
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
            let a = eval_int(lhs, consts, path, unsigned)?;
            let b = eval_int(rhs, consts, path, unsigned)?;
            match fold_const_bin(*op, a, b, path, expr.span, unsigned)? {
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
        } => Ok(eval_int(expr, consts, path, unsigned)?.wrapping_neg()),
        parse::ExprKind::Unary {
            op: parse::UnaryOp::BitNot,
            expr,
        } => Ok(!eval_int(expr, consts, path, unsigned)?),
        _ => Err(CheckError::new(
            path,
            expr.span,
            "const initializer must be an integer expression",
        )),
    }
}
