//! IR types as LLVM type names. Opaque pointers (`ptr`).

use crate::emit::c_ty;
use crate::ir::cx::IrCx;
use crate::ir::ids::{EnumId, StructId};
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};

pub fn ll_type(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Void => Ok("void".to_string()),
        TyKind::Bool => Ok("i8".to_string()),
        TyKind::Int(IntTy::I8) | TyKind::Int(IntTy::U8) => Ok("i8".to_string()),
        TyKind::Int(IntTy::I16) | TyKind::Int(IntTy::U16) => Ok("i16".to_string()),
        TyKind::Int(IntTy::I32) | TyKind::Int(IntTy::U32) => Ok("i32".to_string()),
        TyKind::Int(IntTy::I64) | TyKind::Int(IntTy::U64) => Ok("i64".to_string()),
        TyKind::Float(FloatTy::F32) => Ok("float".to_string()),
        TyKind::Float(FloatTy::F64) => Ok("double".to_string()),
        TyKind::Ptr | TyKind::CStr | TyKind::Ref(_) | TyKind::Fn { .. } => Ok("ptr".to_string()),
        TyKind::String | TyKind::Slice { .. } => Ok("%j3_string".to_string()),
        TyKind::Array { elem, len } => Ok(format!("[{len} x {}]", ll_type(cx, *elem)?)),
        TyKind::Struct(id) => Ok(format!("%{}", struct_name(cx, *id))),
        TyKind::Enum(id) => {
            let def = cx.enum_def(*id);
            if def.is_c_like() {
                Ok("i32".to_string())
            } else {
                Ok(format!("%{}", enum_name(cx, *id)))
            }
        }
    }
}

pub fn struct_name(cx: &IrCx, id: StructId) -> String {
    c_ty::struct_c_name(cx, id)
}

pub fn enum_name(cx: &IrCx, id: EnumId) -> String {
    c_ty::enum_c_name(cx, id)
}

#[allow(dead_code)]
pub fn is_agg(cx: &IrCx, ty: Ty) -> bool {
    match cx.ty_kind(ty) {
        TyKind::Struct(_) | TyKind::Slice { .. } | TyKind::String | TyKind::Array { .. } => true,
        TyKind::Enum(id) => !cx.enum_def(*id).is_c_like(),
        _ => false,
    }
}

pub fn is_ptr(cx: &IrCx, ty: Ty) -> bool {
    matches!(
        cx.ty_kind(ty),
        TyKind::Ref(_) | TyKind::Ptr | TyKind::CStr | TyKind::Fn { .. }
    )
}

pub fn align(cx: &IrCx, ty: Ty) -> u64 {
    crate::ir::abi::align(cx, ty)
}

pub fn size(cx: &IrCx, ty: Ty) -> u64 {
    crate::ir::abi::size(cx, ty)
}
