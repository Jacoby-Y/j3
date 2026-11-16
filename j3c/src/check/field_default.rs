//! Struct field defaults (`hp: int = 100`): a literal checked against the field's type, refused on
//! a field whose type names a type parameter.

use super::*;

pub(super) fn field_default(
    cx: &IrCx,
    path: &str,
    ty: Ty,
    field_ty: &Type,
    params: &[crate::parse::Ident],
    default: &Option<Expr>,
) -> Result<Option<Operand>, CheckError> {
    let Some(expr) = default else {
        return Ok(None);
    };
    if mentions_type_param(field_ty, params) {
        return Err(CheckError::new(
            path,
            expr.span,
            "default on a generic field",
        ));
    }
    Ok(Some(literal_default(cx, path, ty, expr)?))
}

pub(super) fn mentions_type_param(ty: &Type, params: &[crate::parse::Ident]) -> bool {
    match &ty.kind {
        TypeKind::Named(n) => params.iter().any(|p| p.name == *n),
        TypeKind::Generic { name, args } => {
            params.iter().any(|p| p.name == *name)
                || args.iter().any(|a| mentions_type_param(a, params))
        }
        TypeKind::Ref(inner) | TypeKind::Slice(inner) => mentions_type_param(inner, params),
        TypeKind::Array { elem, .. } => mentions_type_param(elem, params),
        TypeKind::Tuple(es) => es.iter().any(|e| mentions_type_param(e, params)),
        TypeKind::Dyn(_) => false,
        TypeKind::Fn { params: ps, ret } | TypeKind::Closure { params: ps, ret, .. } => {
            ps.iter().any(|p| mentions_type_param(p, params))
                || ret.as_ref().is_some_and(|r| mentions_type_param(r, params))
        }
    }
}

pub(super) fn literal_default(cx: &IrCx, path: &str, ty: Ty, expr: &Expr) -> Result<Operand, CheckError> {
    let mismatch = |span| CheckError::new(path, span, "field default type mismatch");
    let folded = expr.folded_literal();
    let expr = &folded;
    match &expr.kind {
        ExprKind::Bool(v) => {
            if ty != cx.common.bool {
                return Err(mismatch(expr.span));
            }
            Ok(Operand::bool_const(expr.span, ty, *v))
        }
        ExprKind::BigInt(v) if ty == cx.common.u64 => Ok(Operand::int(expr.span, ty, *v)),
        ExprKind::BigInt(v) => Err(crate::check::infer::big_int_err(path, expr.span, *v)),
        ExprKind::Int(v) => {
            if *v == 0 && matches!(cx.ty_kind(ty), TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)) {
                return Ok(Operand::null(expr.span, ty));
            }
            if is_int(cx, ty) {
                return Ok(Operand::int(expr.span, ty, *v as u64));
            }
            if ty == cx.common.f32 {
                return Ok(Operand::f32_const(expr.span, ty, *v as f32));
            }
            if is_float(cx, ty) {
                return Ok(Operand::f64_const(expr.span, ty, *v as f64));
            }
            Err(mismatch(expr.span))
        }
        ExprKind::Float(v) => {
            if ty == cx.common.f32 {
                return Ok(Operand::f32_const(expr.span, ty, *v as f32));
            }
            if is_float(cx, ty) {
                return Ok(Operand::f64_const(expr.span, ty, *v));
            }
            Err(mismatch(expr.span))
        }
        ExprKind::String(bytes) if ty == cx.common.string => {
            Ok(Operand::bytes(expr.span, ty, bytes))
        }
        _ => Err(CheckError::new(
            path,
            expr.span,
            "field default must be a literal",
        )),
    }
}
