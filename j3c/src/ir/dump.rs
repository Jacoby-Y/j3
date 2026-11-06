//! Human-readable IR dump. No spans (they are dummy until a parser exists).

use std::fmt::Write;

use crate::ir::body::{Body, LocalKind};
use crate::ir::cx::IrCx;
use crate::ir::entity::{Func, FuncKind, Vis};
use crate::ir::ids::LocalId;
use crate::ir::module::Module;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Place, PlaceRoot, Proj};
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};

pub fn dump_module(cx: &IrCx, module: &Module) -> String {
    let mut out = String::new();
    dump_module_into(cx, module, &mut out);
    out
}

pub fn dump_program(cx: &IrCx, modules: &[Module]) -> String {
    let mut out = String::new();
    for (i, module) in modules.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        dump_module_into(cx, module, &mut out);
    }
    out
}

pub fn dump_module_into(cx: &IrCx, module: &Module, out: &mut String) {
    out.push_str("module ");
    for (i, part) in module.name.iter().enumerate() {
        if i > 0 {
            out.push('.');
        }
        out.push_str(cx.name_str(*part));
    }
    out.push_str(";\n\n");

    for id in &module.structs {
        let def = cx.struct_def(*id);
        if def.debug {
            out.push_str("#[Debug]\n");
        }
        if def.packed {
            out.push_str("packed ");
        }
        out.push_str("struct ");
        out.push_str(cx.name_str(def.name));
        out.push_str(" {\n");
        for field in &def.fields {
            out.push_str("    ");
            if field.embedded {
                out.push_str("use ");
            }
            out.push_str(cx.name_str(field.name));
            out.push_str(": ");
            dump_ty(cx, field.ty, out);
            out.push_str(";\n");
        }
        out.push_str("}\n\n");
    }

    for id in &module.funcs {
        dump_func(cx, cx.func(*id), out);
        out.push('\n');
    }
}

pub(crate) fn dump_func(cx: &IrCx, func: &Func, out: &mut String) {
    match func.vis {
        Vis::Pub => out.push_str("pub "),
        Vis::Private => {}
    }
    out.push_str("fn ");
    out.push_str(cx.name_str(func.name));

    match cx.fn_sig(func.ty) {
        Some((params, ret)) => {
            out.push('(');
            match &func.kind {
                FuncKind::J3 { body } => {
                    for (i, local_id) in body.params.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        let decl = body.local(*local_id);
                        out.push_str(cx.name_str(decl.name));
                        out.push_str(": ");
                        dump_ty(cx, decl.ty, out);
                    }
                }
                FuncKind::Decl | FuncKind::Lib { .. } => {
                    for (i, ty) in params.iter().enumerate() {
                        if i > 0 {
                            out.push_str(", ");
                        }
                        dump_ty(cx, *ty, out);
                    }
                }
            }
            out.push(')');
            if ret != cx.common.void {
                out.push_str(": ");
                dump_ty(cx, ret, out);
            }
        }
        None => {
            out.push_str("(/* bad fn type */)");
        }
    }

    match &func.kind {
        FuncKind::J3 { body } => {
            out.push_str(" {\n");
            dump_body(cx, body, out);
            out.push_str("}\n");
        }
        FuncKind::Decl => {
            out.push_str(";\n");
        }
        FuncKind::Lib { lib, symbol } => {
            out.push_str(" = lib \"");
            out.push_str(cx.name_str(*lib));
            out.push_str("\" \"");
            out.push_str(cx.name_str(*symbol));
            out.push_str("\";\n");
        }
    }
}

fn dump_body(cx: &IrCx, body: &Body, out: &mut String) {
    for decl in &body.locals {
        if decl.kind == LocalKind::Param {
            continue;
        }
        out.push_str("    let ");
        match decl.kind {
            LocalKind::Tmp => out.push_str("tmp "),
            LocalKind::Const => out.push_str("const "),
            LocalKind::Let | LocalKind::Param => {}
        }
        out.push_str(cx.name_str(decl.name));
        out.push_str(": ");
        dump_ty(cx, decl.ty, out);
        out.push_str(";\n");
    }
    if body.locals.iter().any(|d| d.kind != LocalKind::Param) {
        out.push('\n');
    }
    for stmt in &body.block.stmts {
        dump_stmt(cx, body, stmt, 1, out);
    }
}

fn dump_stmt(cx: &IrCx, body: &Body, stmt: &Stmt, indent: usize, out: &mut String) {
    write_indent(indent, out);
    match &stmt.kind {
        StmtKind::Assign { place, rvalue } => {
            dump_place(cx, body, place, out);
            out.push_str(" = ");
            dump_rvalue(cx, body, rvalue, out);
            out.push_str(";\n");
        }
        StmtKind::AssignOp { place, op, rhs } => {
            dump_place(cx, body, place, out);
            out.push(' ');
            match op.assign_op_str() {
                Some(s) => out.push_str(s),
                None => out.push_str("?="),
            }
            out.push(' ');
            dump_operand(cx, body, rhs, out);
            out.push_str(";\n");
        }
        StmtKind::Expr(rvalue) => {
            dump_rvalue(cx, body, rvalue, out);
            out.push_str(";\n");
        }
        StmtKind::Return { value } => {
            out.push_str("return");
            if let Some(v) = value {
                out.push(' ');
                dump_operand(cx, body, v, out);
            }
            out.push_str(";\n");
        }
        StmtKind::Break(0) => out.push_str("break;\n"),
        StmtKind::Continue(0) => out.push_str("continue;\n"),
        StmtKind::Break(n) => out.push_str(&format!("break {n};\n")),
        StmtKind::Continue(n) => out.push_str(&format!("continue {n};\n")),
        StmtKind::Abort(..) => out.push_str("abort;\n"),
        StmtKind::Div0 => out.push_str("abort div0;\n"),
        StmtKind::Block(inner) => {
            out.push_str("{\n");
            for s in &inner.stmts {
                dump_stmt(cx, body, s, indent + 1, out);
            }
            write_indent(indent, out);
            out.push_str("}\n");
        }
        StmtKind::Defer { body: inner } => {
            out.push_str("defer {\n");
            for s in &inner.stmts {
                dump_stmt(cx, body, s, indent + 1, out);
            }
            write_indent(indent, out);
            out.push_str("}\n");
        }
        StmtKind::InterpFill { buf, used, parts } => {
            out.push_str("interp ");
            dump_place(cx, body, buf, out);
            out.push_str(" used ");
            dump_place(cx, body, used, out);
            out.push_str(" `");
            for part in parts {
                match part {
                    crate::ir::rvalue::InterpPart::Lit(b) => {
                        out.push_str(&String::from_utf8_lossy(b));
                    }
                    crate::ir::rvalue::InterpPart::Value(op) => {
                        out.push('{');
                        dump_operand(cx, body, op, out);
                        out.push('}');
                    }
                }
            }
            out.push_str("`;\n");
        }
        StmtKind::Switch { scrut, cases, default } => {
            out.push_str("switch ");
            dump_operand(cx, body, scrut, out);
            out.push_str(" {\n");
            let arms = cases.iter().map(|(v, b)| (v.to_string(), b));
            for (label, b) in arms.chain(std::iter::once(("_".to_string(), default))) {
                write_indent(indent + 1, out);
                out.push_str(&label);
                out.push_str(" {\n");
                for s in &b.stmts {
                    dump_stmt(cx, body, s, indent + 2, out);
                }
                write_indent(indent + 1, out);
                out.push_str("}\n");
            }
            write_indent(indent, out);
            out.push_str("}\n");
        }
        StmtKind::If {
            cond,
            then_block,
            else_block,
        } => {
            out.push_str("if ");
            dump_operand(cx, body, cond, out);
            out.push_str(" {\n");
            for s in &then_block.stmts {
                dump_stmt(cx, body, s, indent + 1, out);
            }
            write_indent(indent, out);
            out.push('}');
            if let Some(else_block) = else_block {
                out.push_str(" else {\n");
                for s in &else_block.stmts {
                    dump_stmt(cx, body, s, indent + 1, out);
                }
                write_indent(indent, out);
                out.push('}');
            }
            out.push('\n');
        }
        StmtKind::While { cond, body: inner } => {
            out.push_str("while ");
            dump_operand(cx, body, cond, out);
            out.push_str(" {\n");
            for s in &inner.stmts {
                dump_stmt(cx, body, s, indent + 1, out);
            }
            write_indent(indent, out);
            out.push_str("}\n");
        }
        StmtKind::ForRange {
            index,
            end,
            body: inner,
        } => {
            out.push_str("for ");
            dump_place(cx, body, index, out);
            out.push_str(" .. ");
            dump_operand(cx, body, end, out);
            out.push_str(" {\n");
            for s in &inner.stmts {
                dump_stmt(cx, body, s, indent + 1, out);
            }
            write_indent(indent, out);
            out.push_str("}\n");
        }
    }
}

fn dump_rvalue(cx: &IrCx, body: &Body, rvalue: &Rvalue, out: &mut String) {
    match &rvalue.kind {
        RvalueKind::Simd { op, args } => {
            out.push_str("simd.");
            out.push_str(op.name());
            if let crate::ir::simd::SimdOp::Shuffle(ix) = op {
                let _ = write!(out, "<{}{}{}{}>", ix[0], ix[1], ix[2], ix[3]);
            }
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump_operand(cx, body, arg, out);
            }
            out.push(')');
        }
        RvalueKind::Use(op) => dump_operand(cx, body, op, out),
        RvalueKind::Ref { place } => {
            out.push('&');
            dump_place(cx, body, place, out);
        }
        RvalueKind::Binary { op, lhs, rhs } => {
            dump_operand(cx, body, lhs, out);
            out.push(' ');
            out.push_str(op.as_str());
            out.push(' ');
            dump_operand(cx, body, rhs, out);
        }
        RvalueKind::Unary { op, arg } => {
            out.push_str(op.as_str());
            dump_operand(cx, body, arg, out);
        }
        RvalueKind::Call { func, args } => {
            dump_operand(cx, body, func, out);
            out.push('(');
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump_operand(cx, body, arg, out);
            }
            out.push(')');
        }
        RvalueKind::Syscall { nr, args } => {
            out.push_str("@syscall(");
            dump_operand(cx, body, nr, out);
            for arg in args {
                out.push_str(", ");
                dump_operand(cx, body, arg, out);
            }
            out.push(')');
        }
        RvalueKind::Atomic { op, order, args } => {
            out.push_str(&format!("@{}.{}(", op.name(), order.name()));
            for (i, arg) in args.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump_operand(cx, body, arg, out);
            }
            out.push(')');
        }
        RvalueKind::Aggregate { fields } => {
            dump_ty(cx, rvalue.ty, out);
            out.push_str(" { ");
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump_operand(cx, body, field, out);
            }
            out.push_str(" }");
        }
        RvalueKind::Cast { from } => {
            dump_ty(cx, rvalue.ty, out);
            out.push('(');
            dump_operand(cx, body, from, out);
            out.push(')');
        }
        RvalueKind::Enum { variant, payload } => {
            let def = cx.enum_def(variant.enum_id);
            out.push_str(cx.name_str(def.name));
            out.push('.');
            out.push_str(cx.name_str(def.variants[variant.index as usize].name));
            if let Some(p) = payload {
                out.push('(');
                dump_operand(cx, body, p, out);
                out.push(')');
            }
        }
        RvalueKind::Interpolate { parts } => {
            out.push('`');
            for part in parts {
                match part {
                    crate::ir::rvalue::InterpPart::Lit(bytes) => {
                        for b in bytes {
                            if *b >= 32 && *b < 127 {
                                out.push(*b as char);
                            } else {
                                let _ = write!(out, "\\x{:02x}", b);
                            }
                        }
                    }
                    crate::ir::rvalue::InterpPart::Value(op) => {
                        out.push('{');
                        dump_operand(cx, body, op, out);
                        out.push('}');
                    }
                }
            }
            out.push('`');
        }
    }
}

fn dump_operand(cx: &IrCx, body: &Body, operand: &Operand, out: &mut String) {
    match operand {
        Operand::Copy(place) => {
            out.push_str("copy ");
            dump_place(cx, body, place, out);
        }
        Operand::Const(c) => match &c.kind {
            ConstKind::Void => out.push_str("void"),
            ConstKind::Bool(v) => {
                if *v {
                    out.push_str("true");
                } else {
                    out.push_str("false");
                }
            }
            ConstKind::Int(bits) => {
                let _ = write!(out, "{}", bits);
            }
            ConstKind::F32Bits(bits) => {
                let _ = write!(out, "f32({:#x})", bits);
            }
            ConstKind::F64Bits(bits) => {
                let _ = write!(out, "f64({:#x})", bits);
            }
            ConstKind::FnAddr(id) => {
                out.push_str(cx.name_str(cx.func(*id).name));
            }
            ConstKind::Null => out.push_str("null"),
            ConstKind::Bytes(bytes) => {
                out.push('"');
                for b in bytes {
                    if *b == b'\\' {
                        out.push_str("\\\\");
                    } else if *b == b'"' {
                        out.push_str("\\\"");
                    } else if *b >= 32 && *b < 127 {
                        out.push(*b as char);
                    } else {
                        let _ = write!(out, "\\x{:02x}", b);
                    }
                }
                out.push('"');
            }
        },
    }
}

fn dump_place(cx: &IrCx, body: &Body, place: &Place, out: &mut String) {
    let mut text = match place.root {
        PlaceRoot::Local(id) => local_name(cx, body, id),
        PlaceRoot::Global(id) => cx.name_str(cx.global(id).name).to_string(),
    };
    for proj in &place.proj {
        match proj {
            Proj::Deref => {
                text = format!("(*{})", text);
            }
            Proj::Field { field, .. } => {
                let fname = cx.name_str(cx.field_def(*field).name);
                text = format!("{}.{}", text, fname);
            }
            Proj::Index { index, .. } => {
                let mut idx = String::new();
                dump_operand(cx, body, index, &mut idx);
                text = format!("{}[{}]", text, idx);
            }
            Proj::Ptr => {
                text = format!("{}.ptr", text);
            }
            Proj::Len => {
                text = format!("{}.len", text);
            }
            Proj::EnumTag => {
                text = format!("{}.tag", text);
            }
            Proj::Lane { lane, .. } => {
                text = format!("{}.{}", text, crate::ir::simd::LANE_NAMES[*lane as usize]);
            }
            Proj::EnumPayload { variant, .. } => {
                text = format!("{}.u.v{}", text, variant.index);
            }
        }
    }
    out.push_str(&text);
}

fn local_name(cx: &IrCx, body: &Body, id: LocalId) -> String {
    if (id.0 as usize) < body.locals.len() {
        cx.name_str(body.local(id).name).to_string()
    } else {
        format!("_{}", id.0)
    }
}

pub fn ty_string(cx: &IrCx, ty: Ty) -> String {
    let mut out = String::new();
    dump_ty(cx, ty, &mut out);
    out
}

pub fn dump_ty(cx: &IrCx, ty: Ty, out: &mut String) {
    match cx.ty_kind(ty) {
        TyKind::Void => out.push_str("void"),
        TyKind::Bool => out.push_str("bool"),
        TyKind::Int(IntTy::I64) => out.push_str("int"),
        TyKind::Int(IntTy::I8) => out.push_str("i8"),
        TyKind::Int(IntTy::I16) => out.push_str("i16"),
        TyKind::Int(IntTy::I32) => out.push_str("i32"),
        TyKind::Int(IntTy::U8) => out.push_str("u8"),
        TyKind::Int(IntTy::U16) => out.push_str("u16"),
        TyKind::Int(IntTy::U32) => out.push_str("u32"),
        TyKind::Int(IntTy::U64) => out.push_str("u64"),
        TyKind::Float(FloatTy::F64) => out.push_str("float"),
        TyKind::Float(FloatTy::F32) => out.push_str("f32"),
        TyKind::Half => out.push_str("f16"),
        TyKind::Vec(v) => out.push_str(v.name()),
        TyKind::Ptr => out.push_str("ptr"),
        TyKind::CStr => out.push_str("cstr"),
        TyKind::String => out.push_str("string"),
        TyKind::Ref(inner) => {
            out.push('&');
            dump_ty(cx, *inner, out);
        }
        TyKind::Array { elem, len } => {
            let _ = write!(out, "[{}]", len);
            dump_ty(cx, *elem, out);
        }
        TyKind::Slice { elem } => {
            out.push_str("[]");
            dump_ty(cx, *elem, out);
        }
        TyKind::Fn { params, ret } => {
            out.push_str("fn(");
            for (i, p) in params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                dump_ty(cx, *p, out);
            }
            out.push(')');
            if *ret != cx.common.void {
                out.push_str(": ");
                dump_ty(cx, *ret, out);
            }
        }
        TyKind::Struct(id) => {
            out.push_str(cx.name_str(cx.struct_def(*id).name));
        }
        TyKind::Enum(id) => {
            out.push_str(cx.name_str(cx.enum_def(*id).name));
        }
    }
}

fn write_indent(n: usize, out: &mut String) {
    for _ in 0..n {
        out.push_str("    ");
    }
}
