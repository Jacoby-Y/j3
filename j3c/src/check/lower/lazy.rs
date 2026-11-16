//! Lazy generic-struct methods. A non-generic method of a generic struct instance (`List<Foo>.indexOf`)
//! gets its signature when the instance is made, but its body waits in `IrCx.lazy_methods` until a
//! lowered body names it (a call, a fn value, a thunk). So `List<T>` of a type without `==` compiles
//! as long as nothing calls `indexOf`; a call lowers the body, and its errors carry a
//! "called at" note (`generic_fn::site_note` via `IrCx.inst_sites`). Methods nobody names are
//! dropped from the module lists after lowering (`drop_unused`).

use std::collections::HashMap;

use crate::ir::body::Block;
use crate::ir::cx::IrCx;
use crate::ir::ids::FuncId;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Place, Proj};
use crate::ir::rvalue::{InterpPart, Rvalue, RvalueKind};
use crate::ir::stmt::StmtKind;
use crate::ir::ty::Ty;
use crate::parse::FnItem;
use crate::span::Span;

/// Moves every lazy method named in `block` to `queue` (it will be lowered like any instance).
pub(super) fn wake(cx: &mut IrCx, block: &Block, queue: &mut Vec<(FuncId, FnItem, HashMap<String, Ty>)>) {
    if cx.lazy_methods.is_empty() {
        return;
    }
    let mut found = Vec::new();
    walk_block(block, &mut found);
    for (id, span) in found {
        if let Some((item, subst)) = cx.lazy_methods.remove(&id) {
            cx.inst_sites.entry(id).or_insert((span, 0));
            queue.push((id, item, subst));
        }
    }
}

/// Removes the methods no body ever named from the module fn lists, so nothing emits them.
pub(crate) fn drop_unused(cx: &IrCx, funcs: &mut Vec<FuncId>) {
    if !cx.lazy_methods.is_empty() {
        funcs.retain(|f| !cx.lazy_methods.contains_key(f));
    }
}

/// Every fn `block` names (calls and fn values), deduplicated: the call graph `check::frame_str` walks.
pub(crate) fn fn_refs(block: &Block) -> Vec<FuncId> {
    let mut found = Vec::new();
    walk_block(block, &mut found);
    let mut ids: Vec<FuncId> = found.into_iter().map(|(id, _)| id).collect();
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    ids
}

type Found = Vec<(FuncId, Span)>;

fn walk_block(b: &Block, out: &mut Found) {
    for s in &b.stmts {
        match &s.kind {
            StmtKind::Assign { place, rvalue } => {
                walk_place(place, out);
                walk_rvalue(rvalue, out);
            }
            StmtKind::AssignOp { place, rhs, .. } => {
                walk_place(place, out);
                walk_op(rhs, out);
            }
            StmtKind::Defer { body } | StmtKind::Block(body) => walk_block(body, out),
            StmtKind::If { cond, then_block, else_block } => {
                walk_op(cond, out);
                walk_block(then_block, out);
                if let Some(e) = else_block {
                    walk_block(e, out);
                }
            }
            StmtKind::Switch { scrut, cases, default } => {
                walk_op(scrut, out);
                for (_, c) in cases {
                    walk_block(c, out);
                }
                walk_block(default, out);
            }
            StmtKind::While { cond, body } => {
                walk_op(cond, out);
                walk_block(body, out);
            }
            StmtKind::Return { value } => {
                if let Some(v) = value {
                    walk_op(v, out);
                }
            }
            StmtKind::Expr(r) => walk_rvalue(r, out),
            StmtKind::ForRange { index, end, body } => {
                walk_place(index, out);
                walk_op(end, out);
                walk_block(body, out);
            }
            StmtKind::Abort(_, ops) => ops.iter().for_each(|o| walk_op(o, out)),
            StmtKind::InterpFill { buf, used, parts } => {
                walk_place(buf, out);
                walk_place(used, out);
                walk_parts(parts, out);
            }
            StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Div0 => {}
        }
    }
}

fn walk_rvalue(r: &Rvalue, out: &mut Found) {
    match &r.kind {
        RvalueKind::Use(o) | RvalueKind::Unary { arg: o, .. } | RvalueKind::Cast { from: o } => walk_op(o, out),
        RvalueKind::Ref { place } => walk_place(place, out),
        RvalueKind::Binary { lhs, rhs, .. } => {
            walk_op(lhs, out);
            walk_op(rhs, out);
        }
        RvalueKind::Call { func, args } => {
            walk_op(func, out);
            args.iter().for_each(|o| walk_op(o, out));
        }
        RvalueKind::Syscall { nr, args } => {
            walk_op(nr, out);
            args.iter().for_each(|o| walk_op(o, out));
        }
        RvalueKind::Atomic { args, .. } | RvalueKind::Simd { args, .. } | RvalueKind::Aggregate { fields: args } => {
            args.iter().for_each(|o| walk_op(o, out))
        }
        RvalueKind::Enum { payload, .. } => {
            if let Some(p) = payload {
                walk_op(p, out);
            }
        }
        RvalueKind::Interpolate { parts } => walk_parts(parts, out),
    }
}

fn walk_parts(parts: &[InterpPart], out: &mut Found) {
    for p in parts {
        if let InterpPart::Value(o) = p {
            walk_op(o, out);
        }
    }
}

fn walk_place(p: &Place, out: &mut Found) {
    for pr in &p.proj {
        if let Proj::Index { index, .. } = pr {
            walk_op(index, out);
        }
    }
}

fn walk_op(o: &Operand, out: &mut Found) {
    match o {
        Operand::Const(c) => {
            if let ConstKind::FnAddr(id) = c.kind {
                out.push((id, c.span));
            }
        }
        Operand::Copy(p) => walk_place(p, out),
    }
}
