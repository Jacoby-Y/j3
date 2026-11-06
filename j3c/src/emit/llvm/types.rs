//! LLVM struct/enum type definitions, constant initializers and function signatures.

use super::*;

pub(super) fn ll_ret(cx: &IrCx, fn_ty: Ty) -> Result<String, String> {
    let (_, ret) = cx.fn_sig(fn_ty).ok_or("not a fn type")?;
    if ret == cx.common.void {
        Ok("void".to_string())
    } else {
        ll_type(cx, ret)
    }
}

pub(super) fn ll_params(cx: &IrCx, fn_ty: Ty) -> Result<String, String> {
    let (params, _) = cx.fn_sig(fn_ty).ok_or("not a fn type")?;
    let mut s = String::new();
    for (i, p) in params.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&ll_type(cx, *p)?);
    }
    Ok(s)
}


pub(super) fn emit_struct_types(cx: &IrCx, modules: &[Module], out: &mut String) -> Result<(), String> {
    let mut ids = Vec::new();
    for module in modules {
        for id in &module.structs {
            if !ids.contains(id) {
                ids.push(*id);
            }
        }
    }
    let mut ordered = Vec::new();
    let mut seen = HashSet::new();
    for id in ids {
        visit_struct(cx, id, &mut seen, &mut ordered);
    }
    for id in &ordered {
        let def = cx.struct_def(*id);
        let name = llvm_ty::struct_name(cx, *id);
        let packed = def.packed;
        let mut fields = Vec::new();
        for f in &def.fields {
            fields.push(ll_type(cx, f.ty)?);
        }
        let inner = fields.join(", ");
        if packed {
            out.push_str(&format!("%{name} = type <{{ {inner} }}>\n"));
        } else {
            out.push_str(&format!("%{name} = type {{ {inner} }}\n"));
        }
    }
    if !ordered.is_empty() {
        out.push('\n');
    }
    Ok(())
}

fn visit_struct(cx: &IrCx, id: StructId, seen: &mut HashSet<StructId>, ordered: &mut Vec<StructId>) {
    if !seen.insert(id) {
        return;
    }
    for field in &cx.struct_def(id).fields {
        if let TyKind::Struct(dep) = cx.ty_kind(field.ty) {
            visit_struct(cx, *dep, seen, ordered);
        }
        if let TyKind::Array { elem, .. } = cx.ty_kind(field.ty) {
            if let TyKind::Struct(dep) = cx.ty_kind(*elem) {
                visit_struct(cx, *dep, seen, ordered);
            }
        }
    }
    ordered.push(id);
}

pub(super) fn emit_enum_types(cx: &IrCx, modules: &[Module], out: &mut String) -> Result<(), String> {
    let mut any = false;
    for module in modules {
        for id in &module.enums {
            let def = cx.enum_def(*id);
            if def.is_c_like() {
                continue;
            }
            any = true;
            let ty = cx
                .lookup_ty(&TyKind::Enum(*id))
                .ok_or_else(|| "enum type not interned".to_string())?;
            let sz = abi::size(cx, ty);
            let rest = if sz > 4 { sz - 4 } else { 0 };
            let name = llvm_ty::enum_name(cx, *id);
            // a vector payload makes the enum 16-aligned: a zero-length vector array carries that alignment
            let lead = if abi::align(cx, ty) > 8 { "[0 x <4 x float>], " } else { "" };
            out.push_str(&format!("%{name} = type {{ {lead}i32, [{rest} x i8] }}\n"));
        }
    }
    if any {
        out.push('\n');
    }
    Ok(())
}

pub(super) fn ll_const_init(
    cx: &IrCx,
    ty: Ty,
    init: &ConstInit,
    gname: &str,
    pre: &mut String,
) -> Result<String, String> {
    match init {
        ConstInit::Scalar(Operand::Const(c)) if matches!(&c.kind, ConstKind::Bytes(_)) => {
            // A string in static data: its bytes get a private global emitted before this one.
            let ConstKind::Bytes(bytes) = &c.kind else { unreachable!() };
            let n = pre.matches('\n').count();
            pre.push_str(&format!(
                "@.cb.{gname}.{n} = private unnamed_addr constant [{} x i8] {}\n",
                bytes.len(),
                llvm_expr::ll_bytes_lit(bytes)
            ));
            Ok(format!("{{ ptr @.cb.{gname}.{n}, i64 {} }}", bytes.len()))
        }
        ConstInit::Scalar(op) => {
            let mut dummy = LlvmFn::const_only(cx);
            dummy.operand_const(op)
        }
        ConstInit::Fields(fs) => {
            let mut parts = Vec::new();
            match cx.ty_kind(ty) {
                TyKind::Struct(id) => {
                    let def = cx.struct_def(*id);
                    for f in &def.fields {
                        let fname = cx.name_str(f.name);
                        let child = fs
                            .iter()
                            .find(|(n, _)| n == fname)
                            .map(|(_, v)| v);
                        match child {
                            Some(v) => parts.push(format!(
                                "{} {}",
                                ll_type(cx, f.ty)?,
                                ll_const_init(cx, f.ty, v, gname, pre)?
                            )),
                            None => parts.push(format!("{} zeroinitializer", ll_type(cx, f.ty)?)),
                        }
                    }
                    Ok(format!("{{ {} }}", parts.join(", ")))
                }
                _ => Ok("zeroinitializer".to_string()),
            }
        }
        ConstInit::Elems(es) => {
            let (elem, _) = match cx.ty_kind(ty) {
                TyKind::Array { elem, .. } => (*elem, ()),
                _ => return Ok("zeroinitializer".to_string()),
            };
            let mut parts = Vec::new();
            for v in es {
                parts.push(format!(
                    "{} {}",
                    ll_type(cx, elem)?,
                    ll_const_init(cx, elem, v, gname, pre)?
                ));
            }
            Ok(format!("[ {} ]", parts.join(", ")))
        }
    }
}

/// `thread_local ` for a `#[thread]` global, else empty.
pub(super) fn tls(g: &crate::ir::entity::GlobalDef) -> &'static str {
    if g.thread {
        "thread_local "
    } else {
        ""
    }
}
