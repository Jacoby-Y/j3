//! Single layout: `@sizeof`, arena bump, and later LLVM GEP.
//! Matches System V LP64 / gcc for the types we emit.

use crate::ir::cx::IrCx;
use crate::ir::ty::{FloatTy, Ty, TyKind};

pub fn size(cx: &IrCx, ty: Ty) -> u64 {
    match cx.ty_kind(ty) {
        TyKind::Void => 0,
        TyKind::Bool => 1,
        TyKind::Int(int_ty) => int_ty.width_bytes(),
        TyKind::Half => 2,
        TyKind::Float(FloatTy::F32) => 4,
        TyKind::Float(FloatTy::F64) => 8,
        TyKind::Ptr | TyKind::CStr | TyKind::Ref(_) | TyKind::Fn { .. } => 8,
        TyKind::String | TyKind::Slice { .. } | TyKind::Vec(_) => 16,
        TyKind::Array { elem, len } => size(cx, *elem) * *len,
        TyKind::Struct(id) => {
            let def = cx.struct_def(*id);
            if def.packed {
                let mut off = 0u64;
                for f in &def.fields {
                    off += size(cx, f.ty);
                }
                return off;
            }
            let mut off = 0u64;
            let mut a = 1u64;
            for f in &def.fields {
                let fa = align(cx, f.ty);
                if fa > a {
                    a = fa;
                }
                off = align_up(off, fa) + size(cx, f.ty);
            }
            align_up(off, a)
        }
        TyKind::Enum(id) => {
            let def = cx.enum_def(*id);
            if def.is_c_like() {
                return 4;
            }
            // `int32_t tag; union { T v; } u;` — padding after tag.
            let mut payload = 0u64;
            let mut ua = 1u64;
            for v in &def.variants {
                if let Some(p) = v.payload {
                    let s = size(cx, p);
                    if s > payload {
                        payload = s;
                    }
                    let pa = align(cx, p);
                    if pa > ua {
                        ua = pa;
                    }
                }
            }
            let sa = if ua > 4 { ua } else { 4 };
            let u_off = align_up(4, ua);
            align_up(u_off + payload, sa)
        }
    }
}

pub fn align(cx: &IrCx, ty: Ty) -> u64 {
    match cx.ty_kind(ty) {
        TyKind::Void => 1,
        TyKind::Bool => 1,
        TyKind::Int(int_ty) => int_ty.width_bytes(),
        TyKind::Half => 2,
        TyKind::Float(FloatTy::F32) => 4,
        TyKind::Float(FloatTy::F64) => 8,
        TyKind::Ptr | TyKind::CStr | TyKind::Ref(_) | TyKind::Fn { .. } => 8,
        TyKind::String | TyKind::Slice { .. } => 8,
        TyKind::Vec(_) => 16,
        TyKind::Array { elem, .. } => align(cx, *elem),
        TyKind::Struct(id) => {
            let def = cx.struct_def(*id);
            if def.packed {
                return 1;
            }
            let mut a = 1u64;
            for f in &def.fields {
                let fa = align(cx, f.ty);
                if fa > a {
                    a = fa;
                }
            }
            a
        }
        TyKind::Enum(id) => {
            if cx.enum_def(*id).is_c_like() {
                return 4;
            }
            let mut a = 4u64;
            for v in &cx.enum_def(*id).variants {
                if let Some(p) = v.payload {
                    let pa = align(cx, p);
                    if pa > a {
                        a = pa;
                    }
                }
            }
            a
        }
    }
}

fn align_up(n: u64, a: u64) -> u64 {
    if a == 0 {
        return n;
    }
    (n + a - 1) / a * a
}

/// SysV LP64 field offset. Slice/string are `{ ptr @ 0, len @ 8 }`.
/// Tagged enum: `i32 tag` at 0, union at `align_up(4, payload_align)`.
/// C-like enum is `i32` (not `{n}ll`).
pub fn field_offset(cx: &IrCx, ty: Ty, field: usize) -> Option<u64> {
    match cx.ty_kind(ty) {
        TyKind::String | TyKind::Slice { .. } => match field {
            0 => Some(0),
            1 => Some(8),
            _ => None,
        },
        TyKind::Struct(id) => {
            let def = cx.struct_def(*id);
            if field >= def.fields.len() {
                return None;
            }
            if def.packed {
                let mut off = 0u64;
                for f in def.fields.iter().take(field) {
                    off += size(cx, f.ty);
                }
                return Some(off);
            }
            let mut off = 0u64;
            for f in def.fields.iter().take(field) {
                off = align_up(off, align(cx, f.ty)) + size(cx, f.ty);
            }
            Some(align_up(off, align(cx, def.fields[field].ty)))
        }
        TyKind::Enum(id) => {
            let def = cx.enum_def(*id);
            if def.is_c_like() {
                return None;
            }
            match field {
                0 => Some(0),
                1 => {
                    let mut ua = 1u64;
                    for v in &def.variants {
                        if let Some(p) = v.payload {
                            let pa = align(cx, p);
                            if pa > ua {
                                ua = pa;
                            }
                        }
                    }
                    Some(align_up(4, ua))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::cx::IrCx;

    #[test]
    fn slice_string_are_ptr_then_len() {
        let mut cx = IrCx::new();
        assert_eq!(size(&cx, cx.common.string), 16);
        assert_eq!(align(&cx, cx.common.string), 8);
        assert_eq!(field_offset(&cx, cx.common.string, 0), Some(0));
        assert_eq!(field_offset(&cx, cx.common.string, 1), Some(8));
        let sl = cx.ty_slice(cx.common.u8);
        assert_eq!(field_offset(&cx, sl, 0), Some(0));
        assert_eq!(field_offset(&cx, sl, 1), Some(8));
    }
}
