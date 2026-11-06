//! C emit: string interpolation, syscall words, aggregates, and enum values.

use super::*;

impl<'a> CWriter<'a> {
    pub(super) fn interp_append(
        &mut self,
        pad: &str,
        buf: &str,
        used: &str,
        e: &str,
        ty: Ty,
    ) -> Result<String, String> {
        let cap = crate::ir::wrap::INTERP_CAP;
        if ty == self.cx.common.string {
            return Ok(format!(
                "{pad}if (({used}) + ({e}).len > {cap}ll) {{ J3_FAIL(\"string interpolation overflowed its buffer\"); }}\n\
                 {pad}memcpy(({buf}) + ({used}), ({e}).ptr, (size_t)({e}).len);\n\
                 {pad}{used} = ({used}) + ({e}).len;\n"
            ));
        }
        if c_ty::is_int(self.cx, ty) || ty == self.cx.common.bool || matches!(self.cx.ty_kind(ty), TyKind::Float(_)) {
            let fmt = if ty == self.cx.common.bool {
                "\"%s\", ({e}) ? \"true\" : \"false\""
            } else if matches!(self.cx.ty_kind(ty), TyKind::Float(_)) {
                "\"%g\", (double)({e})"
            } else {
                "\"%lld\", (long long)({e})"
            };
            let fmt = fmt.replace("{e}", e);
            return Ok(format!(
                "{pad}{{\n\
                 {pad}    int64_t j3_room = {cap}ll - ({used});\n\
                 {pad}    if (j3_room <= 0) {{ J3_FAIL(\"string interpolation overflowed its buffer\"); }}\n\
                 {pad}    int64_t j3_w = (int64_t)snprintf((char *)({buf}) + ({used}), (size_t)j3_room, {fmt});\n\
                 {pad}    if (j3_w < 0 || j3_w >= j3_room) {{ J3_FAIL(\"string interpolation overflowed its buffer\"); }}\n\
                 {pad}    {used} = ({used}) + j3_w;\n\
                 {pad}}}\n"
            ));
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => {
                let def = self.cx.struct_def(*id);
                if !def.debug {
                    return Err("interpolate of non-#[Debug] struct".to_string());
                }
                let mut s = String::new();
                let name = self.cx.name_str(def.name);
                s.push_str(&self.interp_lit_text(pad, buf, used, &format!("{name} {{ "))?);
                let fields = def.fields.clone();
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        s.push_str(&self.interp_lit_text(pad, buf, used, ", ")?);
                    }
                    let fname = self.cx.name_str(field.name);
                    s.push_str(&self.interp_lit_text(pad, buf, used, &format!("{fname}: "))?);
                    let child = format!("{e}.{fname}");
                    s.push_str(&self.interp_append(pad, buf, used, &child, field.ty)?);
                }
                s.push_str(&self.interp_lit_text(pad, buf, used, " }")?);
                Ok(s)
            }
            _ => Err("interpolate of this type is not implemented".to_string()),
        }
    }

    pub(super) fn interp_lit_text(
        &mut self,
        pad: &str,
        buf: &str,
        used: &str,
        text: &str,
    ) -> Result<String, String> {
        let bytes = text.as_bytes();
        let lit = c_bytes_literal(bytes);
        let cap = crate::ir::wrap::INTERP_CAP;
        Ok(format!(
            "{pad}if (({used}) + {len}ll > {cap}ll) {{ J3_FAIL(\"string interpolation overflowed its buffer\"); }}\n\
             {pad}memcpy(({buf}) + ({used}), {lit}, {len}ull);\n\
             {pad}{used} = ({used}) + {len}ll;\n",
            len = bytes.len()
        ))
    }

    pub(super) fn syscall_word(&mut self, operand: &Operand) -> Result<String, String> {
        let e = self.operand(operand)?;
        if c_ty::is_ptr_word(self.cx, operand.ty()) {
            Ok(format!("(int64_t)(uintptr_t)(void *)({e})"))
        } else {
            Ok(format!("(int64_t)({e})"))
        }
    }

    pub(super) fn aggregate(&mut self, ty: Ty, fields: &[Operand]) -> Result<String, String> {
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => {
                let def = self.cx.struct_def(*id);
                if def.fields.len() != fields.len() {
                    return Err("aggregate field count != struct".to_string());
                }
                if def
                    .fields
                    .iter()
                    .any(|f| c_ty::array_parts(self.cx, f.ty).is_some())
                {
                    return Err("array field in struct aggregate".into());
                }
                let mut s = String::new();
                s.push('(');
                s.push_str(&c_type(self.cx, ty)?);
                s.push(')');
                s.push_str("{ ");
                for (i, field) in def.fields.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push('.');
                    s.push_str(&c_ty::sanitize(self.cx.name_str(field.name)));
                    s.push_str(" = ");
                    s.push_str(&self.operand(&fields[i])?);
                }
                s.push_str(" }");
                Ok(s)
            }
            TyKind::Array { elem, len } => {
                if fields.len() as u64 != *len {
                    return Err("aggregate field count != array len".to_string());
                }
                let elem_c = c_type(self.cx, *elem)?;
                let mut s = format!("({elem_c}[{len}]){{ ");
                for (i, f) in fields.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&self.operand(f)?);
                }
                s.push_str(" }");
                Ok(s)
            }
            TyKind::Slice { .. } | TyKind::String => {
                if fields.len() != 2 {
                    return Err("slice aggregate is { ptr, len }".to_string());
                }
                let ptr = self.operand(&fields[0])?;
                let len = self.operand(&fields[1])?;
                Ok(format!(
                    "({}){{ .ptr = {ptr}, .len = {len} }}",
                    c_type(self.cx, ty)?
                ))
            }
            _ => Err("aggregate of this type not in C emit yet".to_string()),
        }
    }

    pub(super) fn enum_val(
        &mut self,
        variant: crate::ir::ids::VariantId,
        payload: &Option<Operand>,
    ) -> Result<String, String> {
        let def = self.cx.enum_def(variant.enum_id);
        if def.is_c_like() {
            return Ok(format!("((int32_t){})", variant.index));
        }
        let cname = c_ty::enum_c_name(self.cx, variant.enum_id);
        match payload {
            Some(p) => {
                let v = self.operand(p)?;
                Ok(format!(
                    "({cname}){{ .tag = {tag}, .u.v{idx} = {v} }}",
                    tag = variant.index,
                    idx = variant.index
                ))
            }
            None => Ok(format!("({cname}){{ .tag = {} }}", variant.index)),
        }
    }
}
