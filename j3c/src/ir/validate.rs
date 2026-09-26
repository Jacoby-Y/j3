//! Well-formedness for typed IR. A missing type is a compiler bug.

use crate::ir::body::{Block, Body};
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::ids::LocalId;
use crate::ir::module::Module;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Access, Place, PlaceRoot, Proj};
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::ir::ty::{Ty, TyKind};

pub struct ValidateError {
    pub message: String,
}

impl ValidateError {
    fn new(message: String) -> ValidateError {
        ValidateError { message }
    }
}

pub fn validate_module(cx: &IrCx, module: &Module) -> Result<(), ValidateError> {
    for id in &module.funcs {
        let func = cx.func(*id);
        match &func.kind {
            FuncKind::J3 { body } => {
                validate_body(cx, func.ty, body)?;
            }
            FuncKind::Decl | FuncKind::Lib { .. } => {
                if cx.fn_sig(func.ty).is_none() {
                    return Err(ValidateError::new(format!(
                        "func {} does not have a fn type",
                        cx.name_str(func.name)
                    )));
                }
            }
        }
    }
    Ok(())
}

fn validate_body(cx: &IrCx, fn_ty: Ty, body: &Body) -> Result<(), ValidateError> {
    let (params, ret) = match cx.fn_sig(fn_ty) {
        Some(sig) => sig,
        None => {
            return Err(ValidateError::new(
                "function type is not TyKind::Fn".to_string(),
            ));
        }
    };

    if body.params.len() != params.len() {
        return Err(ValidateError::new(format!(
            "param count {} != fn type {}",
            body.params.len(),
            params.len()
        )));
    }

    for (i, local_id) in body.params.iter().enumerate() {
        let decl = body.local(*local_id);
        if decl.ty != params[i] {
            return Err(ValidateError::new(format!(
                "param {} type mismatch",
                cx.name_str(decl.name)
            )));
        }
    }

    for decl in &body.locals {
        if decl.id.0 as usize >= body.locals.len() {
            return Err(ValidateError::new("local id out of range".to_string()));
        }
    }

    validate_block(cx, body, &body.block, ret, false)
}

fn validate_block(
    cx: &IrCx,
    body: &Body,
    block: &Block,
    ret: Ty,
    in_loop: bool,
) -> Result<(), ValidateError> {
    for stmt in &block.stmts {
        validate_stmt(cx, body, stmt, ret, in_loop)?;
    }
    Ok(())
}

fn validate_stmt(
    cx: &IrCx,
    body: &Body,
    stmt: &Stmt,
    ret: Ty,
    in_loop: bool,
) -> Result<(), ValidateError> {
    if stmt.ty != cx.common.void {
        return Err(ValidateError::new(
            "statement type must be void".to_string(),
        ));
    }

    match &stmt.kind {
        StmtKind::Assign { place, rvalue } => {
            validate_place(cx, body, place)?;
            validate_rvalue(cx, body, rvalue)?;
            if !place.access.assign {
                return Err(ValidateError::new(
                    "assign through a non-assignable place".to_string(),
                ));
            }
            if place.ty != rvalue.ty {
                return Err(ValidateError::new(
                    "assign place type != rvalue type".to_string(),
                ));
            }
        }
        StmtKind::AssignOp { place, op, rhs } => {
            validate_place(cx, body, place)?;
            validate_operand(cx, body, rhs)?;
            if !place.access.assign {
                return Err(ValidateError::new(
                    "assign-op through a non-assignable place".to_string(),
                ));
            }
            if op.assign_op_str().is_none() {
                return Err(ValidateError::new(
                    "binary op cannot be used as assign-op".to_string(),
                ));
            }
            if place.ty != rhs.ty() {
                return Err(ValidateError::new(
                    "assign-op place type != rhs type".to_string(),
                ));
            }
        }
        StmtKind::Expr(rvalue) => {
            validate_rvalue(cx, body, rvalue)?;
        }
        StmtKind::Return { value } => match value {
            None => {
                if ret != cx.common.void {
                    return Err(ValidateError::new(
                        "return without value in non-void function".to_string(),
                    ));
                }
            }
            Some(v) => {
                validate_operand(cx, body, v)?;
                if v.ty() != ret {
                    return Err(ValidateError::new(
                        "return value type != function return type".to_string(),
                    ));
                }
            }
        },
        StmtKind::Abort | StmtKind::Div0 => {}
        StmtKind::Break | StmtKind::Continue => {
            if !in_loop {
                return Err(ValidateError::new(
                    "break/continue outside a loop".to_string(),
                ));
            }
        }
        StmtKind::Block(inner) => {
            validate_block(cx, body, inner, ret, in_loop)?;
        }
        StmtKind::Defer { body: inner } => {
            validate_block(cx, body, inner, ret, in_loop)?;
        }
        StmtKind::If {
            cond,
            then_block,
            else_block,
        } => {
            validate_operand(cx, body, cond)?;
            if cond.ty() != cx.common.bool {
                return Err(ValidateError::new("if cond is not bool".to_string()));
            }
            validate_block(cx, body, then_block, ret, in_loop)?;
            if let Some(else_block) = else_block {
                validate_block(cx, body, else_block, ret, in_loop)?;
            }
        }
        StmtKind::While { cond, body: inner } => {
            validate_operand(cx, body, cond)?;
            if cond.ty() != cx.common.bool {
                return Err(ValidateError::new("while cond is not bool".to_string()));
            }
            validate_block(cx, body, inner, ret, true)?;
        }
        StmtKind::ForRange {
            index,
            end,
            body: inner,
        } => {
            validate_place(cx, body, index)?;
            validate_operand(cx, body, end)?;
            if !index.access.assign {
                return Err(ValidateError::new(
                    "for index is not assignable".to_string(),
                ));
            }
            if index.ty != cx.common.int || end.ty() != cx.common.int {
                return Err(ValidateError::new("for range is not int".to_string()));
            }
            validate_block(cx, body, inner, ret, true)?;
        }
        StmtKind::InterpFill { buf, used, parts } => {
            validate_place(cx, body, buf)?;
            validate_place(cx, body, used)?;
            if !buf.access.assign || !used.access.assign {
                return Err(ValidateError::new(
                    "interp fill needs assignable buf/used".to_string(),
                ));
            }
            if used.ty != cx.common.int {
                return Err(ValidateError::new("interp used is not int".to_string()));
            }
            match cx.ty_kind(buf.ty) {
                TyKind::Array { elem, .. } if *elem == cx.common.u8 => {}
                _ => {
                    return Err(ValidateError::new(
                        "interp buf is not [N]u8".to_string(),
                    ));
                }
            }
            for part in parts {
                if let crate::ir::rvalue::InterpPart::Value(op) = part {
                    validate_operand(cx, body, op)?;
                }
            }
        }
    }

    Ok(())
}

fn validate_rvalue(cx: &IrCx, body: &Body, rvalue: &Rvalue) -> Result<(), ValidateError> {
    match &rvalue.kind {
        RvalueKind::Use(op) => {
            validate_operand(cx, body, op)?;
            if op.ty() != rvalue.ty {
                return Err(ValidateError::new(
                    "use operand ty != rvalue ty".to_string(),
                ));
            }
        }
        RvalueKind::Ref { place } => {
            validate_place(cx, body, place)?;
            if !place.access.take_ref {
                return Err(ValidateError::new(
                    "ref of a place that cannot be borrowed (`const`)".to_string(),
                ));
            }
            match cx.ref_inner(rvalue.ty) {
                Some(inner) if inner == place.ty => {}
                _ => {
                    return Err(ValidateError::new(
                        "ref rvalue type is not &place.ty".to_string(),
                    ));
                }
            }
        }
        RvalueKind::Binary { lhs, rhs, .. } => {
            validate_operand(cx, body, lhs)?;
            validate_operand(cx, body, rhs)?;
        }
        RvalueKind::Unary { arg, .. } => {
            validate_operand(cx, body, arg)?;
        }
        RvalueKind::Call { func, args } => {
            validate_operand(cx, body, func)?;
            for arg in args {
                validate_operand(cx, body, arg)?;
            }
            match cx.fn_sig(func.ty()) {
                Some((params, ret)) => {
                    if params.len() != args.len() {
                        return Err(ValidateError::new("call argc mismatch".to_string()));
                    }
                    for (i, arg) in args.iter().enumerate() {
                        if arg.ty() != params[i] {
                            return Err(ValidateError::new(format!(
                                "call arg {} type mismatch",
                                i
                            )));
                        }
                    }
                    if ret != rvalue.ty {
                        return Err(ValidateError::new("call ret != rvalue ty".to_string()));
                    }
                }
                None => {
                    return Err(ValidateError::new("call of non-fn".to_string()));
                }
            }
        }
        RvalueKind::Syscall { nr, args } => {
            validate_operand(cx, body, nr)?;
            for arg in args {
                validate_operand(cx, body, arg)?;
            }
            if rvalue.ty != cx.common.int {
                return Err(ValidateError::new("@syscall type must be int".to_string()));
            }
        }
        RvalueKind::Aggregate { fields } => {
            for f in fields {
                validate_operand(cx, body, f)?;
            }
        }
        RvalueKind::Enum { variant, payload } => {
            let def = cx.enum_def(variant.enum_id);
            if (variant.index as usize) >= def.variants.len() {
                return Err(ValidateError::new("enum variant index oob".to_string()));
            }
            match (&def.variants[variant.index as usize].payload, payload) {
                (Some(_), Some(p)) => validate_operand(cx, body, p)?,
                (None, None) => {}
                _ => {
                    return Err(ValidateError::new(
                        "enum payload does not match variant".to_string(),
                    ));
                }
            }
        }
        RvalueKind::Cast { from } => {
            validate_operand(cx, body, from)?;
        }
        RvalueKind::Interpolate { parts } => {
            if rvalue.ty != cx.common.string {
                return Err(ValidateError::new(
                    "interpolate type must be string".to_string(),
                ));
            }
            for part in parts {
                if let crate::ir::rvalue::InterpPart::Value(op) = part {
                    validate_operand(cx, body, op)?;
                }
            }
        }
    }
    Ok(())
}

fn validate_operand(cx: &IrCx, body: &Body, operand: &Operand) -> Result<(), ValidateError> {
    match operand {
        Operand::Copy(place) => validate_place(cx, body, place),
        Operand::Const(c) => {
            if let ConstKind::FnAddr(id) = c.kind {
                if c.ty != cx.func(id).ty {
                    return Err(ValidateError::new(
                        "fn addr const type != func type".to_string(),
                    ));
                }
            }
            Ok(())
        }
    }
}

fn validate_place(cx: &IrCx, body: &Body, place: &Place) -> Result<(), ValidateError> {
    let (mut ty, access0) = match place.root {
        PlaceRoot::Local(id) => {
            let decl = local(body, id)?;
            (decl.ty, decl.access)
        }
        PlaceRoot::Global(id) => {
            let g = cx.global(id);
            (g.ty, Access::let_binding())
        }
    };

    for proj in &place.proj {
        match proj {
            Proj::Deref => match cx.ref_inner(ty) {
                Some(inner) => {
                    ty = inner;
                }
                None => {
                    return Err(ValidateError::new(
                        "Deref projection on non-ref type".to_string(),
                    ));
                }
            },
            Proj::Field {
                field,
                ty: field_ty,
            } => match cx.ty_kind(ty) {
                TyKind::Struct(sid) => {
                    if field.struct_id != *sid {
                        return Err(ValidateError::new(
                            "field id does not match struct".to_string(),
                        ));
                    }
                    let def = cx.field_def(*field);
                    if def.ty != *field_ty {
                        return Err(ValidateError::new(
                            "field projection ty != field def".to_string(),
                        ));
                    }
                    ty = def.ty;
                }
                _ => {
                    return Err(ValidateError::new(
                        "field projection on non-struct".to_string(),
                    ));
                }
            },
            Proj::Index { index, ty: elem_ty } => {
                validate_operand(cx, body, index)?;
                if index.ty() != cx.common.int {
                    return Err(ValidateError::new("index is not int".to_string()));
                }
                match cx.elem_ty(ty) {
                    Some(elem) => {
                        if elem != *elem_ty {
                            return Err(ValidateError::new(
                                "index projection ty != elem ty".to_string(),
                            ));
                        }
                        ty = elem;
                    }
                    None => {
                        return Err(ValidateError::new(
                            "index projection on non-array/slice/string".to_string(),
                        ));
                    }
                }
            }
            Proj::Ptr => match cx.elem_ty(ty) {
                Some(elem) => match cx.lookup_ty(&TyKind::Ref(elem)) {
                    Some(ptr_ty) => {
                        ty = ptr_ty;
                    }
                    None => {
                        return Err(ValidateError::new(
                            "slice ptr type &T was never interned".to_string(),
                        ));
                    }
                },
                None => {
                    return Err(ValidateError::new(
                        "Ptr projection on non-array/slice/string".to_string(),
                    ));
                }
            },
            Proj::Len => match cx.elem_ty(ty) {
                Some(_) => {
                    ty = cx.common.int;
                }
                None => {
                    return Err(ValidateError::new(
                        "Len projection on non-array/slice/string".to_string(),
                    ));
                }
            },
            Proj::EnumTag => match cx.ty_kind(ty) {
                TyKind::Enum(id) if !cx.enum_def(*id).is_c_like() => {
                    ty = cx.common.i32;
                }
                _ => {
                    return Err(ValidateError::new("EnumTag on non-tagged enum".to_string()));
                }
            },
            Proj::EnumPayload { variant, ty: pty } => match cx.ty_kind(ty) {
                TyKind::Enum(id) if *id == variant.enum_id => {
                    let def = cx.enum_def(*id);
                    let v = &def.variants[variant.index as usize];
                    match v.payload {
                        Some(p) if p == *pty => ty = p,
                        _ => {
                            return Err(ValidateError::new(
                                "EnumPayload does not match variant".to_string(),
                            ));
                        }
                    }
                }
                _ => {
                    return Err(ValidateError::new("EnumPayload on wrong type".to_string()));
                }
            },
        }
    }

    let access = fold_access(
        cx,
        access0,
        &place.proj,
        match place.root {
            PlaceRoot::Local(id) => local(body, id)?.ty,
            PlaceRoot::Global(id) => cx.global(id).ty,
        },
    );

    if ty != place.ty {
        return Err(ValidateError::new(
            "place.ty does not match projections".to_string(),
        ));
    }
    if access.assign != place.access.assign || access.take_ref != place.access.take_ref {
        return Err(ValidateError::new(
            "place.access does not match projections".to_string(),
        ));
    }
    Ok(())
}

fn fold_access(cx: &IrCx, start: Access, projs: &[Proj], start_ty: Ty) -> Access {
    let mut access = start;
    let mut ty = start_ty;
    for proj in projs {
        match proj {
            Proj::Deref => {
                if let Some(inner) = cx.ref_inner(ty) {
                    ty = inner;
                }
                access = Access::through_ref();
            }
            Proj::Field { field, ty: field_ty } => {
                if cx.struct_def(field.struct_id).script.as_ref()
                    .is_some_and(|meta| (field.index as usize) < meta.prop_spans.len()) {
                    access = Access::read_only_view();
                }
                ty = *field_ty;
            }
            Proj::Index { ty: elem_ty, .. } => {
                match cx.ty_kind(ty) {
                    TyKind::String => {
                        access = Access::read_only_view();
                    }
                    _ => {}
                }
                ty = *elem_ty;
            }
            Proj::Ptr => {
                match cx.ty_kind(ty) {
                    TyKind::String => {
                        access = Access::read_only_view();
                    }
                    _ => {}
                }
                if let Some(elem) = cx.elem_ty(ty) {
                    if let Some(ptr_ty) = cx.lookup_ty(&TyKind::Ref(elem)) {
                        ty = ptr_ty;
                    }
                }
            }
            Proj::Len => {
                match cx.ty_kind(ty) {
                    TyKind::String => {
                        access = Access::read_only_view();
                    }
                    _ => {}
                }
                ty = cx.common.int;
            }
            Proj::EnumTag => {
                ty = cx.common.i32;
            }
            Proj::EnumPayload { ty: pty, .. } => {
                ty = *pty;
            }
        }
    }
    access
}

fn local(body: &Body, id: LocalId) -> Result<&crate::ir::body::LocalDecl, ValidateError> {
    if (id.0 as usize) >= body.locals.len() {
        return Err(ValidateError::new(format!("unknown local {}", id.0)));
    }
    Ok(body.local(id))
}
