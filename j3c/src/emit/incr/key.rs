//! Object keys: compiler id, layout digest, and the stable IR text each module hashes to.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};

use super::*;
use crate::emit::c_expr::{self, stable_ty};
use crate::ir::body::Block;
use crate::ir::entity::ConstInit;
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::rvalue::{InterpPart, Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};

/// Hash of this j3c's own bytes: a relink or `touch` that changes nothing keeps the objects,
/// any real emitter change drops them. The hash is cached in `~/.cache/j3/compiler.id` by the
/// binary's size and mtime, so a build reads the binary only after j3c changed.
pub(crate) fn compiler_id() -> u64 {
    static ID: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *ID.get_or_init(|| {
        let Some(exe) = crate::editor::host_exe().ok() else { return version_id() };
        let Ok(meta) = fs::metadata(&exe) else { return version_id() };
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        let stamp = format!("{} {mtime}", meta.len());
        let file = crate::editor::cache_root().map(|d| d.join("compiler.id"));
        let cached = file.as_ref().and_then(|f| fs::read_to_string(f).ok());
        if let Some((s, id)) = cached.as_deref().and_then(|c| c.trim().rsplit_once(' ')) {
            if s == stamp {
                if let Ok(id) = u64::from_str_radix(id, 16) {
                    return id;
                }
            }
        }
        let Ok(bytes) = fs::read(&exe) else { return version_id() };
        let mut h = DefaultHasher::new();
        bytes.hash(&mut h);
        let id = h.finish();
        if let Some(f) = &file {
            if let Some(dir) = f.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(f, format!("{stamp} {id:016x}\n"));
        }
        id
    })
}

pub(super) fn version_id() -> u64 {
    let mut h = DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut h);
    h.finish()
}

/// `text` is the module's [`module_ir`].
pub(super) fn object_key(text: &str, opts: &EmitOpts, layouts: &Layouts) -> String {
    let mut h = DefaultHasher::new();
    opts.chunks.hash(&mut h);
    layouts.for_module(text).hash(&mut h);
    text.hash(&mut h);
    opts.cc.opt.hash(&mut h);
    opts.cc.debug.hash(&mut h);
    opts.cc.release.hash(&mut h);
    opts.target.hash(&mut h);
    opts.cpu.hash(&mut h);
    opts.flags.hash(&mut h);
    opts.clang.hash(&mut h);
    compiler_id().hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Field and variant layout of every struct and enum in the program. A module's IR only names
/// foreign types, so the offsets and tags of the types it touches (and what those embed) go in
/// its key, or an edit elsewhere leaves a stale object. Types are matched by name token, so a
/// shared name only ever causes an extra rebuild.
pub(super) struct Layouts {
    texts: Vec<String>,
    deps: Vec<Vec<usize>>,
    by_needle: HashMap<String, Vec<usize>>,
}

pub(super) fn tokens(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !(c.is_alphanumeric() || c == '_')).filter(|t| !t.is_empty())
}

pub(super) fn layout_digest(cx: &IrCx, modules: &[Module]) -> Layouts {
    let mut needles: Vec<String> = Vec::new();
    let mut texts = Vec::new();
    for module in modules {
        for id in &module.structs {
            let def = cx.struct_def(*id);
            let mut s = format!("{}.{}|{}|", module_label(cx, module), cx.name_str(def.name), def.packed);
            for f in &def.fields {
                s.push_str(&format!("{}:{}:{},", cx.name_str(f.name), f.embedded, stable_ty(cx, f.ty)));
            }
            needles.push(cx.name_str(def.name).to_string());
            texts.push(s);
        }
        for id in &module.enums {
            let def = cx.enum_def(*id);
            let mut s = format!("{}.{}|", module_label(cx, module), cx.name_str(def.name));
            for v in &def.variants {
                s.push_str(cx.name_str(v.name));
                if let Some(p) = v.payload {
                    s.push_str(&stable_ty(cx, p));
                }
                s.push(',');
            }
            needles.push(cx.name_str(def.name).to_string());
            texts.push(s);
        }
    }
    let mut by_needle: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, n) in needles.iter().enumerate() {
        by_needle.entry(n.clone()).or_default().push(i);
    }
    let deps = texts
        .iter()
        .map(|t| tokens(t).filter_map(|w| by_needle.get(w)).flatten().copied().collect())
        .collect();
    Layouts { texts, deps, by_needle }
}

impl Layouts {
    /// Layout text of the types `module_text` names, closed over embedded types.
    fn for_module(&self, module_text: &str) -> String {
        let mut seen = vec![false; self.texts.len()];
        let mut stack: Vec<usize> = tokens(module_text)
            .filter_map(|w| self.by_needle.get(w))
            .flatten()
            .copied()
            .collect();
        while let Some(i) = stack.pop() {
            if !std::mem::replace(&mut seen[i], true) {
                stack.extend(self.deps[i].iter().copied());
            }
        }
        (0..seen.len()).filter(|i| seen[*i]).map(|i| self.texts[i].as_str()).collect::<Vec<_>>().join(";")
    }
}

/// Stable text of one module: the IR dump, each function's symbol and local
/// types, callee symbols, and globals. Intern ids are not included.
pub(super) fn module_ir(cx: &IrCx, module: &Module) -> String {
    let mut s = crate::ir::dump::dump_module(cx, module);
    for id in &module.structs {
        let def = cx.struct_def(*id);
        for (n, t) in &def.ty_args {
            s.push_str(n);
            s.push('=');
            s.push_str(&stable_ty(cx, *t));
            s.push(';');
        }
        for f in &def.fields {
            s.push_str(&stable_ty(cx, f.ty));
            s.push(';');
        }
    }
    for id in &module.enums {
        let def = cx.enum_def(*id);
        for v in &def.variants {
            if let Some(p) = v.payload {
                s.push_str(&stable_ty(cx, p));
                s.push(';');
            }
        }
    }
    for id in &module.globals {
        let g = cx.global(*id);
        s.push_str(cx.name_str(g.name));
        s.push(':');
        s.push_str(&stable_ty(cx, g.ty));
        if let Some(init) = &g.init {
            hash_init_into(cx, init, &mut s);
        }
        s.push(';');
    }
    for id in &module.funcs {
        let func = cx.func(*id);
        s.push_str(&c_expr::func_c_name(cx, func));
        s.push(';');
        if let FuncKind::J3 { body } = &func.kind {
            for decl in &body.locals {
                s.push_str(&stable_ty(cx, decl.ty));
                s.push(';');
            }
            hash_block_into(cx, &body.block, &mut s);
        }
    }
    s
}

pub(super) fn hash_init_into(cx: &IrCx, init: &ConstInit, out: &mut String) {
    match init {
        ConstInit::Scalar(op) => hash_operand_into(cx, op, out),
        ConstInit::Fields(fs) => {
            for (n, v) in fs {
                out.push_str(n);
                hash_init_into(cx, v, out);
            }
        }
        ConstInit::Elems(es) => {
            for v in es {
                hash_init_into(cx, v, out);
            }
        }
    }
}

pub(super) fn hash_block_into(cx: &IrCx, block: &Block, out: &mut String) {
    for stmt in &block.stmts {
        hash_stmt_into(cx, stmt, out);
    }
}

pub(super) fn hash_stmt_into(cx: &IrCx, stmt: &Stmt, out: &mut String) {
    match &stmt.kind {
        StmtKind::Assign { rvalue, .. } => hash_rvalue_into(cx, rvalue, out),
        StmtKind::AssignOp { rhs, .. } => hash_operand_into(cx, rhs, out),
        StmtKind::Defer { body } | StmtKind::Block(body) => hash_block_into(cx, body, out),
        StmtKind::If {
            cond,
            then_block,
            else_block,
        } => {
            hash_operand_into(cx, cond, out);
            hash_block_into(cx, then_block, out);
            if let Some(e) = else_block {
                hash_block_into(cx, e, out);
            }
        }
        StmtKind::While { cond, body } => {
            hash_operand_into(cx, cond, out);
            hash_block_into(cx, body, out);
        }
        StmtKind::Switch { scrut, cases, default } => {
            hash_operand_into(cx, scrut, out);
            for (v, b) in cases {
                out.push_str(&format!("case {v};"));
                hash_block_into(cx, b, out);
            }
            hash_block_into(cx, default, out);
        }
        StmtKind::Return { value } => {
            if let Some(v) = value {
                hash_operand_into(cx, v, out);
            }
        }
        StmtKind::Expr(rv) => hash_rvalue_into(cx, rv, out),
        StmtKind::ForRange { end, body, .. } => {
            hash_operand_into(cx, end, out);
            hash_block_into(cx, body, out);
        }
        StmtKind::InterpFill { parts, .. } => {
            for p in parts {
                if let InterpPart::Value(op) = p {
                    hash_operand_into(cx, op, out);
                }
            }
        }
        StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Abort(..) | StmtKind::Div0 => {}
    }
}

pub(super) fn hash_rvalue_into(cx: &IrCx, rv: &Rvalue, out: &mut String) {
    match &rv.kind {
        RvalueKind::Use(op) | RvalueKind::Cast { from: op } => hash_operand_into(cx, op, out),
        RvalueKind::Ref { .. } => {}
        RvalueKind::Binary { lhs, rhs, .. } => {
            hash_operand_into(cx, lhs, out);
            hash_operand_into(cx, rhs, out);
        }
        RvalueKind::Unary { arg, .. } => hash_operand_into(cx, arg, out),
        RvalueKind::Call { func, args } => {
            hash_operand_into(cx, func, out);
            for a in args {
                hash_operand_into(cx, a, out);
            }
        }
        RvalueKind::Syscall { nr, args } => {
            hash_operand_into(cx, nr, out);
            for a in args {
                hash_operand_into(cx, a, out);
            }
        }
        RvalueKind::Simd { op, args } => {
            out.push_str(&format!("{op:?}"));
            for a in args {
                hash_operand_into(cx, a, out);
            }
        }
        RvalueKind::Atomic { op, order, args } => {
            out.push_str(op.name());
            out.push_str(order.name());
            for a in args {
                hash_operand_into(cx, a, out);
            }
        }
        RvalueKind::Aggregate { fields } => {
            for f in fields {
                hash_operand_into(cx, f, out);
            }
        }
        RvalueKind::Enum { payload, .. } => {
            if let Some(p) = payload {
                hash_operand_into(cx, p, out);
            }
        }
        RvalueKind::Interpolate { parts } => {
            for p in parts {
                if let InterpPart::Value(op) = p {
                    hash_operand_into(cx, op, out);
                }
            }
        }
    }
}

pub(super) fn hash_operand_into(cx: &IrCx, op: &Operand, out: &mut String) {
    // Global initializers are not in the IR dump, so every constant's value goes in here, or a
    // `const SRC: [N]string` edit (shader text) leaves a stale object.
    if let Operand::Const(c) = op {
        match &c.kind {
            ConstKind::FnAddr(id) => out.push_str(&c_expr::func_c_name(cx, cx.func(*id))),
            kind => out.push_str(&format!("{kind:?}")),
        }
        out.push(';');
    }
}
