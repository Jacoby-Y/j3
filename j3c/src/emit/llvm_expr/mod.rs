//! Function bodies as LLVM IR. Locals are alloca; clang -O mem2reg's them.

use std::collections::{HashMap, HashSet};

use crate::emit::c_expr;
use crate::emit::c_ty;
use crate::emit::llvm_ty::{self, ll_type};
use crate::ir::abi;
use crate::ir::body::{Body, LocalKind};
use crate::ir::cx::IrCx;
use crate::ir::entity::{Func, FuncKind, Vis};
use crate::ir::ids::{FuncId, LocalId};
use crate::ir::module::Module;
use crate::span::Span;
use crate::ir::op::{BinOp, UnaryOp};
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Place, PlaceRoot, Proj};
use crate::ir::rvalue::{InterpPart, Rvalue, RvalueKind};
use crate::ir::stmt::{AbortKind, Stmt, StmtKind};
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};
use crate::span::FileId;
use crate::ir::wrap;

mod binop;
mod debug;
mod interp;
mod rvalue;
mod stmt;

pub(super) use debug::*;

static STR_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

struct DeferFrame {
    blocks: Vec<crate::ir::body::Block>,
    is_loop: bool,
    cont: Option<String>,
    brk: Option<String>,
}

pub(crate) struct LlvmFn<'a> {
    cx: &'a IrCx,
    body: Option<&'a Body>,
    text: String,
    globals: String,
    tmp: u32,
    terminated: bool,
    frames: Vec<DeferFrame>,
    dbg: Option<&'a mut DebugOut>,
    scope: u32,
    line: u32,
    file: String,
    /// Split builds link two objects, so every definition is visible to the other one.
    pub(crate) external: bool,
    /// Whole-program builds share one abort routine: a check site passes `file << 40 | line << 8 | kind`
    /// and this list (the file part) is printed once by `abort_routine`. `None` keeps a message per site.
    pub(crate) abort_files: Option<&'a mut Vec<String>>,
    /// Release builds: no function unwinds (J3 has no exceptions), so no unwind tables.
    pub(crate) nounwind: bool,
    /// ` optsize` / ` optsize minsize` for `-Os` / `-Oz` objects that LTO optimizes again: the
    /// link-time backend only keeps the size goal through these attributes.
    pub(crate) size_attr: &'static str,
}

impl<'a> LlvmFn<'a> {
    pub fn new(cx: &'a IrCx, body: &'a Body, dbg: Option<&'a mut DebugOut>) -> LlvmFn<'a> {
        LlvmFn {
            cx,
            body: Some(body),
            text: String::new(),
            globals: String::new(),
            tmp: 0,
            terminated: false,
            frames: Vec::new(),
            dbg,
            scope: 0,
            line: 1,
            file: String::new(),
            external: false,
            abort_files: None,
            nounwind: false,
            size_attr: "",
        }
    }

    pub fn const_only(cx: &'a IrCx) -> LlvmFn<'a> {
        LlvmFn {
            cx,
            body: None,
            text: String::new(),
            globals: String::new(),
            tmp: 0,
            terminated: false,
            frames: Vec::new(),
            dbg: None,
            scope: 0,
            line: 1,
            file: String::new(),
            external: false,
            abort_files: None,
            nounwind: false,
            size_attr: "",
        }
    }

    fn fresh(&mut self) -> String {
        let n = self.tmp;
        self.tmp += 1;
        // Named, not `%<n>`: labels share this counter, and strict LLVMs (the NDK's, Apple's)
        // reject gaps in unnamed numbering.
        format!("%t{n}")
    }

    fn label(&mut self) -> String {
        let n = self.tmp;
        self.tmp += 1;
        format!("b{n}")
    }

    fn inst(&mut self, s: &str) {
        if self.terminated {
            return;
        }
        self.text.push_str("  ");
        self.text.push_str(s);
        if let Some(id) = self.dbg_id() {
            self.text.push_str(&format!(", !dbg !{id}"));
        }
        self.text.push('\n');
    }

    fn dbg_id(&mut self) -> Option<u32> {
        let scope = self.scope;
        if scope == 0 {
            return None;
        }
        let line = self.line.max(1);
        Some(self.dbg.as_mut()?.location(scope, line))
    }

    fn note_span(&mut self, span: Span) {
        if span.is_dummy() {
            return;
        }
        let file = match self.cx.file(span.file) {
            Some(f) => f,
            None => return,
        };
        let (path, line) = file.loc(span.start);
        self.file = path;
        if line > 0 {
            self.line = line as u32;
        }
    }

    fn exit_abort(&mut self, kind: AbortKind) {
        if self.terminated {
            return;
        }
        if let Some(files) = self.abort_files.as_mut() {
            let at = match files.iter().position(|f| *f == self.file) {
                Some(i) => i,
                None => {
                    files.push(self.file.clone());
                    files.len() - 1
                }
            };
            let packed = (at as u64) << 40 | (self.line as u64) << 8 | kind.code();
            self.inst(&format!("call void @j3_abort(i64 {packed})"));
            self.inst("unreachable");
            self.terminated = true;
            return;
        }
        let msg = format!("{}:{}: {}\n", self.file, self.line, kind.message());
        let p = self.bytes_global(msg.as_bytes());
        let w = self.fresh();
        self.inst(&format!("{w} = call i64 @write(i32 2, ptr {p}, i64 {})", msg.len()));
        self.inst("call void @_exit(i32 102)");
        self.inst("unreachable");
        self.terminated = true;
    }

    fn term(&mut self, s: &str) {
        if self.terminated {
            return;
        }
        self.inst(s);
        self.terminated = true;
    }

    fn block(&mut self, name: &str) {
        if !self.terminated {
            self.term(&format!("br label %{name}"));
        }
        self.text.push_str(name);
        self.text.push_str(":\n");
        self.terminated = false;
    }

    fn local_ptr(&self, id: LocalId) -> String {
        format!("%l{}", id.0)
    }

    pub fn emit_func(&mut self, func: &Func) -> Result<String, String> {
        let body = self.body.ok_or("func body missing")?;
        let (params, ret) = self.cx.fn_sig(func.ty).ok_or("func without fn type")?;
        let ret_ll = if ret == self.cx.common.void {
            "void".to_string()
        } else {
            ll_type(self.cx, ret)?
        };
        let name = c_expr::func_c_name(self.cx, func);
        let linkage = match func.vis {
            _ if self.external => "",
            Vis::Private => "internal ",
            Vis::Pub => "",
        };
        self.scope = self
            .dbg
            .as_ref()
            .and_then(|d| d.subprogram(func.id))
            .unwrap_or(0);
        self.note_span(func.span);
        let mut dbg_attr = if self.scope != 0 {
            format!(" #0 !dbg !{}", self.scope)
        } else {
            String::new()
        };
        if self.nounwind {
            dbg_attr.insert_str(0, " nounwind");
        }
        dbg_attr.insert_str(0, self.size_attr);
        let mut args = String::new();
        for (i, p) in params.iter().enumerate() {
            if i > 0 {
                args.push_str(", ");
            }
            args.push_str(&format!("{} %a{i}", ll_type(self.cx, *p)?));
        }
        self.text.push_str(&format!(
            "define {linkage}{ret_ll} @{name}({args}){dbg_attr} {{\n"
        ));
        self.text.push_str("entry:\n");
        self.terminated = false;

        for decl in &body.locals {
            let ty = ll_type(self.cx, decl.ty)?;
            let a = llvm_ty::align(self.cx, decl.ty);
            self.inst(&format!(
                "{} = alloca {ty}, align {a}",
                self.local_ptr(decl.id)
            ));
        }
        self.declare_locals(func, body);
        for (i, id) in body.params.iter().enumerate() {
            let ty = body.local(*id).ty;
            let ll = ll_type(self.cx, ty)?;
            self.store(&ll, &format!("%a{i}"), &self.local_ptr(*id))?;
        }

        self.emit_block(&body.block, false)?;
        if !self.terminated {
            if ret == self.cx.common.void {
                self.term("ret void");
            } else {
                self.exit_abort(AbortKind::NoReturn);
            }
        }
        self.text.push_str("}\n");
        let mut out = std::mem::take(&mut self.globals);
        out.push_str(&self.text);
        self.text.clear();
        Ok(out)
    }

    fn declare_locals(&mut self, func: &Func, body: &Body) {
        if self.scope == 0 {
            return;
        }
        let mut named = Vec::new();
        for decl in &body.locals {
            if decl.kind == LocalKind::Tmp {
                continue;
            }
            let name = self.cx.name_str(decl.name).to_string();
            if name.is_empty() {
                continue;
            }
            let span = if decl.span.is_dummy() {
                func.span
            } else {
                decl.span
            };
            let Some(file) = self.cx.file(span.file) else {
                continue;
            };
            let line = file.line(span.start).max(1) as u32;
            let scope = self.scope;
            let ty = decl.ty;
            let ptr = self.local_ptr(decl.id);
            let Some(dbg) = self.dbg.as_mut() else {
                return;
            };
            let Some(file_md) = dbg.file_of(span.file) else {
                continue;
            };
            let var = dbg.local_var(self.cx, scope, file_md, &name, ty, line);
            named.push((ptr, var));
        }
        let expr = self.dbg.as_ref().map(|d| d.expr).unwrap_or(0);
        if expr == 0 {
            return;
        }
        for (ptr, var) in named {
            self.inst(&format!(
                "call void @llvm.dbg.declare(metadata ptr {ptr}, metadata !{var}, metadata !{expr})"
            ));
        }
    }

    fn store(&mut self, ty: &str, val: &str, ptr: &str) -> Result<(), String> {
        self.inst(&format!("store {ty} {val}, ptr {ptr}"));
        Ok(())
    }

    fn load(&mut self, ty: &str, ptr: &str) -> String {
        let v = self.fresh();
        self.inst(&format!("{v} = load {ty}, ptr {ptr}"));
        v
    }

    fn emit_block_loop(
        &mut self,
        block: &crate::ir::body::Block,
        cont: String,
        brk: String,
    ) -> Result<(), String> {
        self.frames.push(DeferFrame {
            blocks: Vec::new(),
            is_loop: true,
            cont: Some(cont),
            brk: Some(brk),
        });
        for s in &block.stmts {
            self.stmt(s)?;
        }
        self.flush_current()?;
        self.frames.pop();
        Ok(())
    }

    fn emit_block(&mut self, block: &crate::ir::body::Block, is_loop: bool) -> Result<(), String> {
        let _ = is_loop;
        self.frames.push(DeferFrame {
            blocks: Vec::new(),
            is_loop: false,
            cont: None,
            brk: None,
        });
        for s in &block.stmts {
            self.stmt(s)?;
        }
        self.flush_current()?;
        self.frames.pop();
        Ok(())
    }

    fn flush_current(&mut self) -> Result<(), String> {
        let blocks = match self.frames.last_mut() {
            Some(f) => f.blocks.drain(..).rev().collect::<Vec<_>>(),
            None => return Ok(()),
        };
        for b in blocks {
            for s in &b.stmts {
                self.stmt(s)?;
            }
        }
        Ok(())
    }

    fn emit_defers_through(&mut self, stop_at_loop: bool) -> Result<(), String> {
        let mut blocks = Vec::new();
        for frame in self.frames.iter().rev() {
            for b in frame.blocks.iter().rev() {
                blocks.push(b.clone());
            }
            if stop_at_loop && frame.is_loop {
                break;
            }
        }
        for b in blocks {
            for s in &b.stmts {
                self.stmt(s)?;
            }
        }
        Ok(())
    }

    fn loop_target(&self, cont: bool) -> Result<String, String> {
        for frame in self.frames.iter().rev() {
            if frame.is_loop {
                let lab = if cont { &frame.cont } else { &frame.brk };
                return lab
                    .clone()
                    .ok_or_else(|| "loop frame missing label".to_string());
            }
        }
        Err("break/continue outside a loop".to_string())
    }

}

#[cfg(test)]
mod tests {
    use crate::check::{check_files, ParsedFile};
    use crate::{emit, parse, span::FileId};

    #[test]
    fn arrays_cross_call_boundaries_by_value() {
        let opts = emit::EmitOpts::default();
        if !emit::clang_available(&opts.clang) {
            return;
        }
        let src = r#"
module test;
fn pair(n: int): [2]int { return [n, n + 1]; }
fn identity(xs: [2]int): [2]int { return xs; }
fn forwarded(n: int): [2]int { return pair(n); }
fn main() {
    let xs = forwarded(41);
    let ys = identity(xs);
    xs[0] = 0;
    if ys[0] != 41 || ys[1] != 42 || xs[0] != 0 {
        @syscall(60, 1);
    }
}
"#;
        let file_id = FileId(1);
        let prog = check_files(vec![ParsedFile {
            path: "array_calls.j3".to_string(),
            src: src.to_string(),
            ast: parse::parse(src, file_id).expect("parse array calls"),
            file_id,
        }])
        .expect("check array calls");
        let run = emit::run_program(&prog.cx, &prog.modules, &opts).expect("emit array calls");
        assert_eq!(run.status, 0, "{}", String::from_utf8_lossy(&run.stderr));
    }
}

/// LLVM `c"..."` literal for raw bytes.
pub(super) fn ll_bytes_lit(bytes: &[u8]) -> String {
    let mut lit = String::from("c\"");
    for b in bytes {
        match *b {
            b'\\' => lit.push_str("\\\\"),
            b'"' => lit.push_str("\\22"),
            0 => lit.push_str("\\00"),
            c if (32..127).contains(&c) => lit.push(c as char),
            c => lit.push_str(&format!("\\{c:02X}")),
        }
    }
    lit.push('"');
    lit
}
