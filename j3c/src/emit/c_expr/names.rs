//! C emit: symbol names, interned-id-free type text, and byte-string literals.

use super::*;

pub(crate) fn global_c_name(cx: &IrCx, g: &crate::ir::entity::GlobalDef) -> String {
    format!(
        "j3g_{}_{}",
        c_ty::module_ident(cx, &g.module),
        c_ty::sanitize(cx.name_str(g.name))
    )
}

/// Linker symbol. The suffix is a hash of the signature (and a method's type
/// arguments), not [`crate::ir::ids::FuncId`]: ids are assigned from the entry
/// file outward, so a new function there would rename the stdlib and bust an
/// object cache.
pub(crate) fn func_c_name(cx: &IrCx, func: &crate::ir::entity::Func) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    c_ty::module_ident(cx, &func.module).hash(&mut h);
    cx.name_str(func.name).hash(&mut h);
    if let Some(sid) = func.method_of {
        let def = cx.struct_def(sid);
        c_ty::module_ident(cx, &def.module).hash(&mut h);
        cx.name_str(def.name).hash(&mut h);
        for (n, t) in &def.ty_args {
            n.hash(&mut h);
            stable_ty(cx, *t).hash(&mut h);
        }
    }
    if let Some((params, ret)) = cx.fn_sig(func.ty) {
        for p in params {
            stable_ty(cx, *p).hash(&mut h);
        }
        stable_ty(cx, ret).hash(&mut h);
    }
    format!(
        "j3_{}_{}_{:016x}",
        c_ty::module_ident(cx, &func.module),
        c_ty::sanitize(cx.name_str(func.name)),
        h.finish()
    )
}

/// Type text that does not use intern ids, so two compiles of the same source match.
pub(crate) fn stable_ty(cx: &IrCx, ty: Ty) -> String {
    fn go(cx: &IrCx, ty: Ty, depth: usize) -> String {
        if depth > 32 {
            return "rec".to_string();
        }
        let d = depth + 1;
        match cx.ty_kind(ty) {
            TyKind::Void => "void".to_string(),
            TyKind::Bool => "bool".to_string(),
            TyKind::Int(IntTy::I8) => "i8".to_string(),
            TyKind::Int(IntTy::I16) => "i16".to_string(),
            TyKind::Int(IntTy::I32) => "i32".to_string(),
            TyKind::Int(IntTy::I64) => "int".to_string(),
            TyKind::Int(IntTy::U8) => "u8".to_string(),
            TyKind::Int(IntTy::U16) => "u16".to_string(),
            TyKind::Int(IntTy::U32) => "u32".to_string(),
            TyKind::Int(IntTy::U64) => "u64".to_string(),
            TyKind::Float(FloatTy::F32) => "f32".to_string(),
            TyKind::Float(FloatTy::F64) => "float".to_string(),
            TyKind::Half => "f16".to_string(),
            TyKind::Vec(v) => v.name().to_string(),
            TyKind::Ptr => "ptr".to_string(),
            TyKind::CStr => "cstr".to_string(),
            TyKind::String => "string".to_string(),
            TyKind::Ref(inner) => format!("&{}", go(cx, *inner, d)),
            TyKind::Array { elem, len } => format!("[{len}]{}", go(cx, *elem, d)),
            TyKind::Slice { elem } => format!("[]{}", go(cx, *elem, d)),
            TyKind::Fn { params, ret } => {
                let mut s = String::from("fn(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        s.push(',');
                    }
                    s.push_str(&go(cx, *p, d));
                }
                s.push(')');
                s.push_str(&go(cx, *ret, d));
                s
            }
            TyKind::Struct(id) => {
                let def = cx.struct_def(*id);
                let mut s = format!(
                    "{}.{}",
                    c_ty::module_ident(cx, &def.module),
                    cx.name_str(def.name)
                );
                if !def.ty_args.is_empty() {
                    s.push('<');
                    for (i, (n, t)) in def.ty_args.iter().enumerate() {
                        if i > 0 {
                            s.push(',');
                        }
                        s.push_str(n);
                        s.push('=');
                        s.push_str(&go(cx, *t, d));
                    }
                    s.push('>');
                }
                s
            }
            TyKind::Enum(id) => {
                let def = cx.enum_def(*id);
                let mut s = format!(
                    "{}.{}{{",
                    c_ty::module_ident(cx, &def.module),
                    cx.name_str(def.name)
                );
                for v in &def.variants {
                    s.push_str(cx.name_str(v.name));
                    if let Some(p) = v.payload {
                        s.push(':');
                        s.push_str(&go(cx, p, d));
                    }
                    s.push(';');
                }
                s.push('}');
                s
            }
        }
    }
    go(cx, ty, 0)
}

pub(in crate::emit) fn c_bytes_literal(bytes: &[u8]) -> String {
    let mut s = String::from("\"");
    for b in bytes {
        match *b {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            // `?` would start a trigraph (`??=`).
            b'?' => s.push_str("\\?"),
            c if (32..127).contains(&c) => s.push(c as char),
            // Fixed three-digit octal: a following digit is never swallowed.
            c => s.push_str(&format!("\\{:03o}", c)),
        }
    }
    s.push('"');
    s
}
