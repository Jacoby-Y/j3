//! Places, operands, rvalues, and statements as C.

use crate::emit::c_ty::{self, c_type};
use crate::ir::body::Body;
use crate::ir::cx::IrCx;
use crate::ir::entity::{FuncKind, Vis};
use crate::ir::ids::LocalId;
use crate::ir::op::{BinOp, UnaryOp};
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Place, PlaceRoot, Proj};
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};

struct DeferFrame {
    blocks: Vec<crate::ir::body::Block>,
    is_loop: bool,
}

pub(crate) struct CWriter<'a> {
    pub cx: &'a IrCx,
    pub body: Option<&'a Body>,
    pub tmp: u32,
    frames: Vec<DeferFrame>,
}

impl<'a> CWriter<'a> {
    pub fn new(cx: &'a IrCx) -> CWriter<'a> {
        CWriter {
            cx,
            body: None,
            tmp: 0,
            frames: Vec::new(),
        }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        let n = self.tmp;
        self.tmp += 1;
        format!("{prefix}{n}")
    }

    pub fn local_name(&self, id: LocalId) -> String {
        match self.body {
            Some(body) if (id.0 as usize) < body.locals.len() => {
                let name = self.cx.name_str(body.local(id).name);
                format!("j3l_{}_{}", c_ty::sanitize(name), id.0)
            }
            _ => format!("j3l_{}", id.0),
        }
    }

    pub fn place(&mut self, place: &Place) -> Result<String, String> {
        let (mut text, mut ty) = match place.root {
            PlaceRoot::Local(id) => {
                let ty = match self.body {
                    Some(body) => body.local(id).ty,
                    None => return Err("place emit needs a function body".to_string()),
                };
                (self.local_name(id), ty)
            }
            PlaceRoot::Global(id) => {
                let g = self.cx.global(id);
                (global_c_name(self.cx, g), g.ty)
            }
        };

        for proj in &place.proj {
            match proj {
                Proj::Deref => {
                    text = format!("(*{text})");
                    ty = match self.cx.ref_inner(ty) {
                        Some(inner) => inner,
                        None => return Err("deref of non-ref in C emit".to_string()),
                    };
                }
                Proj::Field { field, .. } => {
                    let fname = self.cx.name_str(self.cx.field_def(*field).name);
                    text = format!("{text}.{fname}");
                    ty = self.cx.field_def(*field).ty;
                }
                Proj::Index { index, .. } => {
                    let idx = self.operand(index)?;
                    if c_ty::is_slice(self.cx, ty) {
                        text = format!("{text}.ptr[{idx}]");
                    } else {
                        text = format!("{text}[{idx}]");
                    }
                    ty = match self.cx.elem_ty(ty) {
                        Some(elem) => elem,
                        None => return Err("index of non-array/slice in C emit".to_string()),
                    };
                }
                Proj::Ptr => {
                    text = format!("{text}.ptr");
                    ty = match self.cx.elem_ty(ty) {
                        Some(elem) => match self.cx.lookup_ty(&crate::ir::ty::TyKind::Ref(elem)) {
                            Some(p) => p,
                            None => return Err("slice ptr type missing".to_string()),
                        },
                        None => return Err("ptr of non-slice in C emit".to_string()),
                    };
                }
                Proj::Len => {
                    text = format!("{text}.len");
                    ty = self.cx.common.int;
                }
                Proj::EnumTag => {
                    text = format!("{text}.tag");
                    ty = self.cx.common.i32;
                }
                Proj::EnumPayload { variant, ty: pty } => {
                    text = format!("{text}.u.v{}", variant.index);
                    ty = *pty;
                }
            }
        }
        let _ = ty;
        Ok(text)
    }

    pub fn operand(&mut self, operand: &Operand) -> Result<String, String> {
        match operand {
            Operand::Copy(place) => self.place(place),
            Operand::Const(c) => self.const_val(c.ty, &c.kind),
        }
    }

    fn const_val(&self, ty: Ty, kind: &ConstKind) -> Result<String, String> {
        match kind {
            ConstKind::Void => Ok("((void)0)".to_string()),
            ConstKind::Bool(true) => Ok("true".to_string()),
            ConstKind::Bool(false) => Ok("false".to_string()),
            ConstKind::Int(bits) => {
                if c_ty::is_int(self.cx, ty) {
                    match self.cx.ty_kind(ty) {
                        TyKind::Int(int_ty) if !int_ty.is_signed() => Ok(format!("{bits}ull")),
                        _ => Ok(format!("{}ll", *bits as i64)),
                    }
                } else {
                    Ok(format!("{}ll", *bits as i64))
                }
            }
            ConstKind::F32Bits(bits) => Ok(format!(
                "((union {{ uint32_t bits; float f; }}){{ .bits = {bits}u }}).f"
            )),
            ConstKind::F64Bits(bits) => Ok(format!(
                "((union {{ uint64_t bits; double f; }}){{ .bits = {bits}ull }}).f"
            )),
            ConstKind::FnAddr(id) => {
                let func = self.cx.func(*id);
                match &func.kind {
                    FuncKind::Lib { symbol, .. } => Ok(self.cx.name_str(*symbol).to_string()),
                    _ => Ok(func_c_name(self.cx, func)),
                }
            }
            ConstKind::Null => Ok("NULL".to_string()),
            ConstKind::Bytes(bytes) => {
                let ty_name = c_type(self.cx, ty)?;
                let lit = c_bytes_literal(bytes);
                Ok(format!(
                    "({ty_name}){{ .ptr = (uint8_t *){lit}, .len = {len}ll }}",
                    len = bytes.len()
                ))
            }
        }
    }

    pub fn rvalue(&mut self, rvalue: &Rvalue) -> Result<String, String> {
        match &rvalue.kind {
            RvalueKind::Use(op) => self.operand(op),
            RvalueKind::Ref { place } => {
                let p = self.place(place)?;
                Ok(format!("&({p})"))
            }
            RvalueKind::Binary { op, lhs, rhs } => {
                let l = self.operand(lhs)?;
                let r = self.operand(rhs)?;
                if c_ty::is_int(self.cx, rvalue.ty) && crate::ir::wrap::wrapping_bin(*op) {
                    wrap_int(self.cx, rvalue.ty, *op, &l, &r)
                } else if *op == BinOp::Rem {
                    match self.cx.ty_kind(rvalue.ty) {
                        TyKind::Float(FloatTy::F32) => Ok(format!("__builtin_fmodf({l}, {r})")),
                        TyKind::Float(FloatTy::F64) => Ok(format!("__builtin_fmod({l}, {r})")),
                        _ => Ok(format!("({l} % {r})")),
                    }
                } else {
                    Ok(format!("({l} {} {r})", op.as_str()))
                }
            }
            RvalueKind::Unary { op, arg } => {
                let a = self.operand(arg)?;
                if *op == UnaryOp::Neg && c_ty::is_int(self.cx, rvalue.ty) {
                    wrap_neg(self.cx, rvalue.ty, &a)
                } else {
                    let prefix = match op {
                        UnaryOp::Neg => "-",
                        UnaryOp::Not => "!",
                        UnaryOp::BitNot => "~",
                    };
                    Ok(format!("({prefix}{a})"))
                }
            }
            RvalueKind::Call { func, args } => {
                let f = self.operand(func)?;
                let mut s = String::new();
                s.push_str(&f);
                s.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&self.operand(arg)?);
                }
                s.push(')');
                Ok(s)
            }
            RvalueKind::Syscall { nr, args } => {
                let mut s = String::from("j3_syscall(");
                s.push_str(&self.syscall_word(nr)?);
                let mut filled = Vec::new();
                for arg in args {
                    filled.push(self.syscall_word(arg)?);
                }
                while filled.len() < 6 {
                    filled.push("0ll".to_string());
                }
                for a in filled {
                    s.push_str(", ");
                    s.push_str(&a);
                }
                s.push(')');
                Ok(s)
            }
            RvalueKind::Aggregate { fields } => self.aggregate(rvalue.ty, fields),
            RvalueKind::Enum { variant, payload } => self.enum_val(*variant, payload),
            RvalueKind::Cast { from } => {
                let t = c_type(self.cx, rvalue.ty)?;
                let v = self.operand(from)?;
                if byte_view(self.cx, from.ty()) && byte_view(self.cx, rvalue.ty) {
                    return Ok(format!("({t}){{ .ptr = ({v}).ptr, .len = ({v}).len }}"));
                }
                Ok(format!("({t})({v})"))
            }
            RvalueKind::Interpolate { .. } => {
                Err("interpolate must be lowered to InterpFill before C emit".to_string())
            }
        }
    }

    fn interp_append(
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

    fn interp_lit_text(
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

    fn syscall_word(&mut self, operand: &Operand) -> Result<String, String> {
        let e = self.operand(operand)?;
        if c_ty::is_ptr_word(self.cx, operand.ty()) {
            Ok(format!("(int64_t)(uintptr_t)(void *)({e})"))
        } else {
            Ok(format!("(int64_t)({e})"))
        }
    }

    fn aggregate(&mut self, ty: Ty, fields: &[Operand]) -> Result<String, String> {
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
                    s.push_str(self.cx.name_str(field.name));
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

    fn enum_val(
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

    fn line_dir(&self, span: crate::span::Span, indent: usize) -> String {
        if span.is_dummy() {
            return String::new();
        }
        let Some(file) = self.cx.file(span.file) else {
            return String::new();
        };
        let line = crate::parse::line_of(&file.src, span.start);
        let path = file.path.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{}#line {line} \"{path}\"\n", "    ".repeat(indent))
    }

    pub fn stmt(&mut self, stmt: &Stmt, indent: usize) -> Result<String, String> {
        let pad = "    ".repeat(indent);
        let dir = self.line_dir(stmt.span, indent);
        let body = match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                if let RvalueKind::Aggregate { fields } = &rvalue.kind {
                    if let TyKind::Struct(id) = self.cx.ty_kind(rvalue.ty) {
                        let def = self.cx.struct_def(*id);
                        if def.fields.len() != fields.len() {
                            return Err("aggregate field count != struct".to_string());
                        }
                        if def
                            .fields
                            .iter()
                            .any(|f| c_ty::array_parts(self.cx, f.ty).is_some())
                        {
                            let p = self.place(place)?;
                            let ty_name = c_type(self.cx, rvalue.ty)?;
                            let mut parts = Vec::new();
                            for (i, field) in def.fields.iter().enumerate() {
                                if c_ty::array_parts(self.cx, field.ty).is_some() {
                                    continue;
                                }
                                let fname = self.cx.name_str(field.name);
                                let v = self.operand(&fields[i])?;
                                parts.push(format!(".{fname} = {v}"));
                            }
                            let mut s = String::new();
                            if parts.is_empty() {
                                s.push_str(&format!("{pad}{p} = ({ty_name}){{0}};\n"));
                            } else {
                                s.push_str(&format!(
                                    "{pad}{p} = ({ty_name}){{ {} }};\n",
                                    parts.join(", ")
                                ));
                            }
                            for (i, field) in def.fields.iter().enumerate() {
                                if c_ty::array_parts(self.cx, field.ty).is_none() {
                                    continue;
                                }
                                let fname = self.cx.name_str(field.name);
                                let src = self.operand(&fields[i])?;
                                let dest = format!("({p}).{fname}");
                                s.push_str(&format!(
                                    "{pad}memcpy({dest}, {src}, sizeof({dest}));\n"
                                ));
                            }
                            Ok::<String, String>(s)
                        } else {
                            let p = self.place(place)?;
                            let v = self.rvalue(rvalue)?;
                            let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                                format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                            } else {
                                format!("{pad}{p} = {v};\n")
                            };
                            Ok(line)
                        }
                    } else {
                        let p = self.place(place)?;
                        let v = self.rvalue(rvalue)?;
                        let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                            format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                        } else {
                            format!("{pad}{p} = {v};\n")
                        };
                        Ok(line)
                    }
                } else {
                    let p = self.place(place)?;
                    let v = self.rvalue(rvalue)?;
                    let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                        format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                    } else {
                        format!("{pad}{p} = {v};\n")
                    };
                    Ok(line)
                }
            }
            StmtKind::AssignOp { place, op, rhs } => {
                let p = self.place(place)?;
                let r = self.operand(rhs)?;
                let op_s = match op.assign_op_str() {
                    Some(s) => s,
                    None => return Err("binary op cannot be assign-op in C".to_string()),
                };
                if c_ty::is_int(self.cx, place.ty) && crate::ir::wrap::wrapping_bin(*op) {
                    let tmp = self.fresh("j3_ap_");
                    let ty = c_type(self.cx, place.ty)?;
                    let wrap = wrap_int(self.cx, place.ty, *op, &format!("(*{tmp})"), &r)?;
                    let mut s = String::new();
                    s.push_str(&format!("{pad}{{\n"));
                    s.push_str(&format!("{pad}    {ty} *{tmp} = &({p});\n"));
                    s.push_str(&format!("{pad}    *{tmp} = {wrap};\n"));
                    s.push_str(&format!("{pad}}}\n"));
                    Ok(s)
                } else {
                    Ok(format!("{pad}{p} {op_s} {r};\n"))
                }
            }
            StmtKind::Expr(rvalue) => {
                let v = self.rvalue(rvalue)?;
                Ok(format!("{pad}(void)({v});\n"))
            }
            StmtKind::Return { value } => {
                match value {
                    Some(v) => {
                        let e = self.operand(v)?;
                        let ty = c_type(self.cx, v.ty())?;
                        let tmp = self.fresh("j3_ret_");
                        let mut s = format!("{pad}{ty} {tmp} = {e};\n");
                        s.push_str(&self.emit_defers_through(indent, None)?);
                        s.push_str(&format!("{pad}return {tmp};\n"));
                        Ok(s)
                    }
                    None => {
                        let mut s = self.emit_defers_through(indent, None)?;
                        s.push_str(&format!("{pad}return;\n"));
                        Ok(s)
                    }
                }
            }
            StmtKind::Break => {
                let mut s = self.emit_defers_through(indent, Some(true))?;
                s.push_str(&format!("{pad}break;\n"));
                Ok(s)
            }
            StmtKind::Continue => {
                let mut s = self.emit_defers_through(indent, Some(true))?;
                s.push_str(&format!("{pad}continue;\n"));
                Ok(s)
            }
            StmtKind::Abort(k) => Ok(format!("{pad}J3_FAIL(\"{}\");\n", k.message())),
            StmtKind::Div0 => Ok(format!("{pad}J3_FAIL(\"{}\");\n", crate::ir::stmt::AbortKind::Div0.message())),
            StmtKind::Block(inner) => {
                let mut s = format!("{pad}{{\n");
                s.push_str(&self.emit_block(inner, false, indent + 1)?);
                s.push_str(&pad);
                s.push_str("}\n");
                Ok(s)
            }
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let c = self.operand(cond)?;
                let mut s = format!("{pad}if ({c}) {{\n");
                s.push_str(&self.emit_block(then_block, false, indent + 1)?);
                s.push_str(&pad);
                s.push('}');
                if let Some(else_block) = else_block {
                    s.push_str(" else {\n");
                    s.push_str(&self.emit_block(else_block, false, indent + 1)?);
                    s.push_str(&pad);
                    s.push('}');
                }
                s.push('\n');
                Ok(s)
            }
            StmtKind::While { cond, body } => {
                let c = self.operand(cond)?;
                let mut s = format!("{pad}while ({c}) {{\n");
                s.push_str(&self.emit_block(body, true, indent + 1)?);
                s.push_str(&pad);
                s.push_str("}\n");
                Ok(s)
            }
            StmtKind::Defer { body } => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.blocks.push(body.clone());
                }
                Ok(String::new())
            }
            StmtKind::ForRange { index, end, body } => {
                let p = self.place(index)?;
                let e = self.operand(end)?;
                let step = wrap_int(self.cx, index.ty, BinOp::Add, &p, "1ll")?;
                let mut s = format!("{pad}for (; {p} < {e}; {p} = {step}) {{\n");
                s.push_str(&self.emit_block(body, true, indent + 1)?);
                s.push_str(&pad);
                s.push_str("}\n");
                Ok(s)
            }
            StmtKind::InterpFill { buf, used, parts } => {
                let buf_s = self.place(buf)?;
                let used_s = self.place(used)?;
                let cap = crate::ir::wrap::INTERP_CAP;
                let mut s = String::new();
                for part in parts {
                    match part {
                        crate::ir::rvalue::InterpPart::Lit(bytes) => {
                            let lit = c_bytes_literal(bytes);
                            s.push_str(&format!(
                                "{pad}if (({used_s}) + {len}ll > {cap}ll) {{ J3_FAIL(\"string interpolation overflowed its buffer\"); }}\n\
                                 {pad}memcpy(({buf_s}) + ({used_s}), {lit}, {len}ull);\n\
                                 {pad}{used_s} = ({used_s}) + {len}ll;\n",
                                len = bytes.len()
                            ));
                        }
                        crate::ir::rvalue::InterpPart::Value(op) => {
                            let e = self.operand(op)?;
                            s.push_str(&self.interp_append(&pad, &buf_s, &used_s, &e, op.ty())?);
                        }
                    }
                }
                Ok(s)
            }
        }?;
        Ok(format!("{dir}{body}"))
    }

    pub fn emit_block(
        &mut self,
        block: &crate::ir::body::Block,
        is_loop: bool,
        indent: usize,
    ) -> Result<String, String> {
        self.frames.push(DeferFrame {
            blocks: Vec::new(),
            is_loop,
        });
        let mut s = String::new();
        for inner in &block.stmts {
            s.push_str(&self.stmt(inner, indent)?);
        }
        s.push_str(&self.flush_current(indent)?);
        self.frames.pop();
        Ok(s)
    }

    fn flush_current(&mut self, indent: usize) -> Result<String, String> {
        let blocks = match self.frames.last_mut() {
            Some(f) => f.blocks.drain(..).rev().collect::<Vec<_>>(),
            None => return Ok(String::new()),
        };
        self.write_defer_blocks(&blocks, indent)
    }

    /// `None` = all frames (return). `Some(true)` = through the nearest loop.
    fn emit_defers_through(
        &mut self,
        indent: usize,
        stop_at_loop: Option<bool>,
    ) -> Result<String, String> {
        let mut blocks = Vec::new();
        for frame in self.frames.iter().rev() {
            for b in frame.blocks.iter().rev() {
                blocks.push(b.clone());
            }
            if stop_at_loop == Some(true) && frame.is_loop {
                break;
            }
        }
        self.write_defer_blocks(&blocks, indent)
    }

    fn write_defer_blocks(
        &mut self,
        blocks: &[crate::ir::body::Block],
        indent: usize,
    ) -> Result<String, String> {
        let mut s = String::new();
        for block in blocks {
            for inner in &block.stmts {
                s.push_str(&self.stmt(inner, indent)?);
            }
        }
        Ok(s)
    }
}

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

fn c_bytes_literal(bytes: &[u8]) -> String {
    let mut s = String::from("\"");
    for b in bytes {
        match *b {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            b'\0' => s.push_str("\\0"),
            c if (32..127).contains(&c) => s.push(c as char),
            c => s.push_str(&format!("\\x{:02x}", c)),
        }
    }
    s.push('"');
    s
}

fn wrap_int(cx: &IrCx, ty: Ty, op: BinOp, lhs: &str, rhs: &str) -> Result<String, String> {
    let c = c_type(cx, ty)?;
    let u = unsigned_c(cx, ty)?;
    match op {
        BinOp::Div | BinOp::Rem => {
            let sop = op.as_str();
            match cx.ty_kind(ty) {
                TyKind::Int(int_ty) if int_ty.is_signed() => {
                    let min = match int_ty.bits() {
                        8 => "INT8_MIN",
                        16 => "INT16_MIN",
                        32 => "INT32_MIN",
                        64 => "INT64_MIN",
                        _ => return Err("bad int width".to_string()),
                    };
                    // `MIN / -1` wraps to MIN; `MIN % -1` is 0.
                    let wrapped = if op == BinOp::Div { format!("({c}){min}") } else { format!("({c})0") };
                    Ok(format!(
                        "((({lhs}) == ({c}){min} && ({rhs}) == ({c})-1) ? {wrapped} : ({c})(({lhs}) {sop} ({rhs})))"
                    ))
                }
                _ => Ok(format!("(({c})(({u})({lhs}) {sop} ({u})({rhs})))")),
            }
        }
        _ => Ok(format!(
            "(({c})(({u})({lhs}) {} ({u})({rhs})))",
            op.as_str()
        )),
    }
}

fn wrap_neg(cx: &IrCx, ty: Ty, arg: &str) -> Result<String, String> {
    let c = c_type(cx, ty)?;
    let u = unsigned_c(cx, ty)?;
    Ok(format!("(({c})(({u})0 - ({u})({arg})))"))
}

fn unsigned_c(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Int(int_ty) => match int_ty.bits() {
            8 => Ok("uint8_t".to_string()),
            16 => Ok("uint16_t".to_string()),
            32 => Ok("uint32_t".to_string()),
            64 => Ok("uint64_t".to_string()),
            _ => Err("bad int width".to_string()),
        },
        _ => Err("unsigned_c on non-int".to_string()),
    }
}

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

/// `string` and `[]u8` are both `{ptr, len}` and may be cast either way.
fn byte_view(cx: &IrCx, ty: Ty) -> bool {
    match cx.ty_kind(ty) {
        TyKind::String => true,
        TyKind::Slice { elem } => *elem == cx.common.u8,
        _ => false,
    }
}
