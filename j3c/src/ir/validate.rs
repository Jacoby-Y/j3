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

mod places;

use places::validate_place;

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

    validate_block(cx, body, &body.block, ret, 0)
}

fn validate_block(
    cx: &IrCx,
    body: &Body,
    block: &Block,
    ret: Ty,
    in_loop: u32,
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
    in_loop: u32,
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
        StmtKind::Abort(_, vals) => {
            for v in vals {
                validate_operand(cx, body, v)?;
            }
        }
        StmtKind::Div0 => {}
        StmtKind::Break(n) | StmtKind::Continue(n) => {
            if *n >= in_loop {
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
        StmtKind::Switch { scrut, cases, default } => {
            validate_operand(cx, body, scrut)?;
            if !matches!(cx.ty_kind(scrut.ty()), crate::ir::ty::TyKind::Int(_)) {
                return Err(ValidateError::new("switch on a non-integer".to_string()));
            }
            let mut seen = std::collections::HashSet::new();
            for (v, b) in cases {
                if !seen.insert(*v) {
                    return Err(ValidateError::new(format!("duplicate switch case {v}")));
                }
                validate_block(cx, body, b, ret, in_loop)?;
            }
            validate_block(cx, body, default, ret, in_loop)?;
        }
        StmtKind::While { cond, body: inner } => {
            validate_operand(cx, body, cond)?;
            if cond.ty() != cx.common.bool {
                return Err(ValidateError::new("while cond is not bool".to_string()));
            }
            validate_block(cx, body, inner, ret, in_loop + 1)?;
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
            validate_block(cx, body, inner, ret, in_loop + 1)?;
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
                                "internal: call argument {} type mismatch (the checker should have caught this; please report it)",
                                i + 1
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
        RvalueKind::Simd { op, args } => {
            use crate::ir::simd::SimdOp;
            for arg in args {
                validate_operand(cx, body, arg)?;
            }
            let want = match op {
                SimdOp::Build => 4,
                SimdOp::Min | SimdOp::Max | SimdOp::Store => 2,
                _ => 1,
            };
            if args.len() != want {
                return Err(ValidateError::new(format!("simd.{} operand count", op.name())));
            }
            let result_is_vec = !matches!(op, SimdOp::Sum | SimdOp::Store);
            if result_is_vec != matches!(cx.ty_kind(rvalue.ty), TyKind::Vec(_)) {
                return Err(ValidateError::new(format!("simd.{} result type", op.name())));
            }
        }
        RvalueKind::Atomic { op, order, args } => {
            if !op.orders().contains(order) {
                return Err(ValidateError::new("@atomic order not valid for op".to_string()));
            }
            if args.len() != op.values() + 1 {
                return Err(ValidateError::new("@atomic operand count".to_string()));
            }
            for arg in args {
                validate_operand(cx, body, arg)?;
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

fn local(body: &Body, id: LocalId) -> Result<&crate::ir::body::LocalDecl, ValidateError> {
    if (id.0 as usize) >= body.locals.len() {
        return Err(ValidateError::new(format!("unknown local {}", id.0)));
    }
    Ok(body.local(id))
}
