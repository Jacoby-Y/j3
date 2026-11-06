//! IR types as C type names. Arrays are not a C type — declare them as `T name[N]`.

use crate::intern::Name;
use crate::ir::cx::IrCx;
use crate::ir::ids::StructId;
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};

pub fn sanitize(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    if out.is_empty() {
        out.push_str("x");
    }
    if out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

pub fn module_ident(cx: &IrCx, parts: &[Name]) -> String {
    let mut s = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            s.push('_');
        }
        s.push_str(&sanitize(cx.name_str(*part)));
    }
    if s.is_empty() {
        s.push_str("mod");
    }
    s
}

pub fn enum_c_name(cx: &IrCx, id: crate::ir::ids::EnumId) -> String {
    let def = cx.enum_def(id);
    format!(
        "j3_{}_{}_{}",
        module_ident(cx, &def.module),
        sanitize(cx.name_str(def.name)),
        id.0
    )
}

pub fn struct_c_name(cx: &IrCx, id: StructId) -> String {
    let def = cx.struct_def(id);
    format!(
        "j3_{}_{}_{}",
        module_ident(cx, &def.module),
        sanitize(cx.name_str(def.name)),
        id.0
    )
}

pub fn struct_name(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Struct(id) => Ok(struct_c_name(cx, *id)),
        other => Err(format!("not a struct: {other:?}")),
    }
}

/// Stable C typedef name. Not intern id — those change if builtins move.
pub fn slice_name(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::String => Ok("j3_string".to_string()),
        TyKind::Slice { elem } => Ok(format!("j3_sl_{}", type_frag(cx, *elem)?)),
        other => Err(format!("not a slice: {other:?}")),
    }
}

fn type_frag(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Void => Ok("void".to_string()),
        TyKind::Bool => Ok("bool".to_string()),
        TyKind::Int(IntTy::I8) => Ok("i8".to_string()),
        TyKind::Int(IntTy::I16) => Ok("i16".to_string()),
        TyKind::Int(IntTy::I32) => Ok("i32".to_string()),
        TyKind::Int(IntTy::I64) => Ok("i64".to_string()),
        TyKind::Int(IntTy::U8) => Ok("u8".to_string()),
        TyKind::Int(IntTy::U16) => Ok("u16".to_string()),
        TyKind::Int(IntTy::U32) => Ok("u32".to_string()),
        TyKind::Int(IntTy::U64) => Ok("u64".to_string()),
        TyKind::Float(FloatTy::F32) => Ok("f32".to_string()),
        TyKind::Float(FloatTy::F64) => Ok("f64".to_string()),
        TyKind::Half => Ok("f16".to_string()),
        TyKind::Vec(v) => Ok(v.name().to_string()),
        TyKind::Ptr => Ok("ptr".to_string()),
        TyKind::CStr => Ok("cstr".to_string()),
        TyKind::String => Ok("string".to_string()),
        TyKind::Ref(inner) => Ok(format!("r_{}", type_frag(cx, *inner)?)),
        TyKind::Slice { elem } => Ok(format!("sl_{}", type_frag(cx, *elem)?)),
        TyKind::Array { elem, len } => Ok(format!("a{len}_{}", type_frag(cx, *elem)?)),
        TyKind::Struct(_) => {
            let full = struct_name(cx, ty)?;
            Ok(full.trim_start_matches("j3_").to_string())
        }
        TyKind::Enum(id) => {
            let def = cx.enum_def(*id);
            Ok(format!(
                "{}_{}",
                module_ident(cx, &def.module),
                sanitize(cx.name_str(def.name))
            ))
        }
        TyKind::Fn { .. } => Ok(format!("fn{}", ty.0)),
    }
}

pub fn is_slice(cx: &IrCx, ty: Ty) -> bool {
    matches!(cx.ty_kind(ty), TyKind::Slice { .. } | TyKind::String)
}

pub fn is_int(cx: &IrCx, ty: Ty) -> bool {
    matches!(cx.ty_kind(ty), TyKind::Int(_))
}

pub fn is_ptr_word(cx: &IrCx, ty: Ty) -> bool {
    matches!(cx.ty_kind(ty), TyKind::Ref(_) | TyKind::Ptr | TyKind::CStr)
}

/// C type for values that are not C arrays (`int64_t buf[4]` is special-cased).
pub fn c_type(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Void => Ok("void".to_string()),
        TyKind::Bool => Ok("bool".to_string()),
        TyKind::Int(IntTy::I8) => Ok("int8_t".to_string()),
        TyKind::Int(IntTy::I16) => Ok("int16_t".to_string()),
        TyKind::Int(IntTy::I32) => Ok("int32_t".to_string()),
        TyKind::Int(IntTy::I64) => Ok("int64_t".to_string()),
        TyKind::Int(IntTy::U8) => Ok("uint8_t".to_string()),
        TyKind::Int(IntTy::U16) => Ok("uint16_t".to_string()),
        TyKind::Int(IntTy::U32) => Ok("uint32_t".to_string()),
        TyKind::Int(IntTy::U64) => Ok("uint64_t".to_string()),
        TyKind::Float(FloatTy::F32) => Ok("float".to_string()),
        TyKind::Float(FloatTy::F64) => Ok("double".to_string()),
        TyKind::Half => Ok("uint16_t".to_string()),
        TyKind::Vec(v) => Ok(super::c_expr::vec_c_name(*v).to_string()),
        TyKind::Ptr => Ok("void *".to_string()),
        TyKind::CStr => Ok("char *".to_string()),
        TyKind::String => slice_name(cx, ty),
        TyKind::Ref(inner) => {
            let inner_c = c_type(cx, *inner)?;
            Ok(format!("{inner_c} *"))
        }
        TyKind::Slice { .. } => slice_name(cx, ty),
        TyKind::Array { .. } => {
            Err("C array types are emitted as `T name[N]`, not as a standalone type".to_string())
        }
        TyKind::Fn { .. } => fn_typedef_name(cx, ty),
        TyKind::Struct(_) => struct_name(cx, ty),
        TyKind::Enum(id) => {
            let def = cx.enum_def(*id);
            if def.is_c_like() {
                Ok("int32_t".to_string())
            } else {
                Ok(enum_c_name(cx, *id))
            }
        }
    }
}

pub fn fn_typedef_name(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Fn { .. } => Ok(format!("j3_fn{}", ty.0)),
        other => Err(format!("not a fn type: {other:?}")),
    }
}

pub fn fn_typedef(cx: &IrCx, ty: Ty) -> Result<String, String> {
    let (params, ret) = match cx.ty_kind(ty) {
        TyKind::Fn { params, ret } => (params, *ret),
        other => return Err(format!("not a fn type: {other:?}")),
    };
    let ret_c = c_type(cx, ret)?;
    let name = fn_typedef_name(cx, ty)?;
    let mut s = format!("typedef {ret_c} (*{name})(");
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&c_type(cx, *p)?);
    }
    if params.is_empty() {
        s.push_str("void");
    }
    s.push_str(");\n");
    Ok(s)
}

pub fn array_parts(cx: &IrCx, ty: Ty) -> Option<(Ty, u64)> {
    match cx.ty_kind(ty) {
        TyKind::Array { elem, len } => Some((*elem, *len)),
        _ => None,
    }
}
