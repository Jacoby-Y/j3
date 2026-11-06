//! C emit: local declarations and function prototypes.

use super::*;

pub(crate) fn emit_local_decls(w: &mut CWriter, body: &Body) -> Result<String, String> {
    let mut out = String::new();
    for decl in &body.locals {
        if decl.kind == crate::ir::body::LocalKind::Param {
            continue;
        }
        let name = w.local_name(decl.id);
        if let Some((elem, len)) = c_ty::array_parts(w.cx, decl.ty) {
            let elem_c = c_type(w.cx, elem)?;
            out.push_str(&format!("    {elem_c} {name}[{len}];\n"));
        } else {
            let ty = c_type(w.cx, decl.ty)?;
            if ty.ends_with('*') {
                out.push_str(&format!("    {ty}{name};\n"));
            } else {
                out.push_str(&format!("    {ty} {name};\n"));
            }
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    Ok(out)
}

pub(crate) fn proto(cx: &IrCx, func: &crate::ir::entity::Func) -> Result<String, String> {
    let (params, ret) = match cx.fn_sig(func.ty) {
        Some(s) => s,
        None => return Err("func without fn type".to_string()),
    };
    let ret_c = c_type(cx, ret)?;
    let name = func_c_name(cx, func);
    let linkage = match func.vis {
        Vis::Private => "static ",
        Vis::Pub => "",
    };
    let mut s = format!("{linkage}{ret_c} {name}(");
    match &func.kind {
        FuncKind::J3 { body } => {
            if body.params.is_empty() {
                s.push_str("void");
            } else {
                for (i, local_id) in body.params.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    let decl = body.local(*local_id);
                    let ty = c_type(cx, decl.ty)?;
                    let n = format!(
                        "j3l_{}_{}",
                        c_ty::sanitize(cx.name_str(decl.name)),
                        decl.id.0
                    );
                    s.push_str(&ty);
                    if !ty.ends_with('*') {
                        s.push(' ');
                    }
                    s.push_str(&n);
                }
            }
        }
        FuncKind::Decl | FuncKind::Lib { .. } => {
            if params.is_empty() {
                s.push_str("void");
            } else {
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&c_type(cx, *p)?);
                }
            }
        }
    }
    s.push(')');
    Ok(s)
}
