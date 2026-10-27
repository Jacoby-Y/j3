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
        }
    }

    fn fresh(&mut self) -> String {
        let n = self.tmp;
        self.tmp += 1;
        format!("%{n}")
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

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        if self.terminated {
            return Ok(());
        }
        self.note_span(stmt.span);
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                let (ptr, _) = self.place_ptr(place)?;
                let (v, ty) = self.rvalue(rvalue)?;
                if matches!(self.cx.ty_kind(place.ty), TyKind::Array { .. }) {
                    let n = llvm_ty::size(self.cx, place.ty);
                    self.inst(&format!(
                        "call void @llvm.memcpy.p0.p0.i64(ptr {ptr}, ptr {v}, i64 {n}, i1 false)"
                    ));
                } else {
                    self.store(&ty, &v, &ptr)?;
                }
            }
            StmtKind::AssignOp { place, op, rhs } => {
                let (ptr, pty) = self.place_ptr(place)?;
                let ll = ll_type(self.cx, place.ty)?;
                let cur = self.load(&ll, &ptr);
                let (r, _) = self.operand(rhs)?;
                let out = self.binop(place.ty, place.ty, *op, &cur, &r)?;
                self.store(&ll, &out, &ptr)?;
                let _ = pty;
            }
            StmtKind::Expr(rvalue) => {
                let _ = self.rvalue(rvalue)?;
            }
            StmtKind::Return { value } => match value {
                Some(v) => {
                    let (e, ty) = self.operand(v)?;
                    let e = if matches!(self.cx.ty_kind(v.ty()), TyKind::Array { .. }) {
                        self.load(&ty, &e)
                    } else {
                        e
                    };
                    let slot = self.fresh();
                    self.inst(&format!("{slot} = alloca {ty}"));
                    self.store(&ty, &e, &slot)?;
                    self.emit_defers_through(false)?;
                    let loaded = self.load(&ty, &slot);
                    self.term(&format!("ret {ty} {loaded}"));
                }
                None => {
                    self.emit_defers_through(false)?;
                    self.term("ret void");
                }
            },
            StmtKind::Break => {
                self.emit_defers_through(true)?;
                let lab = self.loop_target(false)?;
                self.term(&format!("br label %{lab}"));
            }
            StmtKind::Continue => {
                self.emit_defers_through(true)?;
                let lab = self.loop_target(true)?;
                self.term(&format!("br label %{lab}"));
            }
            StmtKind::Abort(k) => {
                self.exit_abort(*k);
            }
            StmtKind::Div0 => {
                self.exit_abort(AbortKind::Div0);
            }
            StmtKind::Block(inner) => {
                self.emit_block(inner, false)?;
            }
            StmtKind::Defer { body } => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.blocks.push(body.clone());
                }
            }
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let c = self.bool_val(cond)?;
                let t = self.label();
                let e = self.label();
                let j = self.label();
                match else_block {
                    Some(_) => self.term(&format!("br i1 {c}, label %{t}, label %{e}")),
                    None => self.term(&format!("br i1 {c}, label %{t}, label %{j}")),
                }
                self.block(&t);
                self.emit_block(then_block, false)?;
                if !self.terminated {
                    self.term(&format!("br label %{j}"));
                }
                if let Some(eb) = else_block {
                    self.block(&e);
                    self.emit_block(eb, false)?;
                    if !self.terminated {
                        self.term(&format!("br label %{j}"));
                    }
                }
                self.block(&j);
            }
            StmtKind::While { cond, body } => {
                let hdr = self.label();
                let bod = self.label();
                let end = self.label();
                self.block(&hdr);
                let c = self.bool_val(cond)?;
                self.term(&format!("br i1 {c}, label %{bod}, label %{end}"));
                self.block(&bod);
                self.emit_block_loop(body, hdr.clone(), end.clone())?;
                if !self.terminated {
                    self.term(&format!("br label %{hdr}"));
                }
                self.block(&end);
            }
            StmtKind::ForRange { index, end, body } => {
                let hdr = self.label();
                let bod = self.label();
                let inc = self.label();
                let fin = self.label();
                let (ip, _) = self.place_ptr(index)?;
                let (ev, _) = self.operand(end)?;
                self.block(&hdr);
                let iv = self.load("i64", &ip);
                let ok = self.fresh();
                self.inst(&format!("{ok} = icmp slt i64 {iv}, {ev}"));
                self.term(&format!("br i1 {ok}, label %{bod}, label %{fin}"));
                self.block(&bod);
                self.emit_block_loop(body, inc.clone(), fin.clone())?;
                if !self.terminated {
                    self.term(&format!("br label %{inc}"));
                }
                self.block(&inc);
                let cur = self.load("i64", &ip);
                let nxt = self.binop(index.ty, index.ty, BinOp::Add, &cur, "1")?;
                self.store("i64", &nxt, &ip)?;
                self.term(&format!("br label %{hdr}"));
                self.block(&fin);
            }
            StmtKind::InterpFill { buf, used, parts } => {
                self.interp_fill(buf, used, parts)?;
            }
        }
        Ok(())
    }

    fn bool_val(&mut self, op: &Operand) -> Result<String, String> {
        let (v, ty) = self.operand(op)?;
        if ty == "i1" {
            return Ok(v);
        }
        let c = self.fresh();
        self.inst(&format!("{c} = icmp ne {ty} {v}, 0"));
        Ok(c)
    }

    fn place_ptr(&mut self, place: &Place) -> Result<(String, Ty), String> {
        let (mut ptr, mut ty) = match place.root {
            PlaceRoot::Local(id) => {
                let ty = match self.body {
                    Some(body) => body.local(id).ty,
                    None => return Err("place needs a function body".to_string()),
                };
                (self.local_ptr(id), ty)
            }
            PlaceRoot::Global(id) => {
                let g = self.cx.global(id);
                (format!("@{}", c_expr::global_c_name(self.cx, g)), g.ty)
            }
        };
        for proj in &place.proj {
            match proj {
                Proj::Deref => {
                    ptr = self.load("ptr", &ptr);
                    ty = self
                        .cx
                        .ref_inner(ty)
                        .ok_or("deref of non-ref")?;
                }
                Proj::Field { field, .. } => {
                    let idx = field.index;
                    let sty = ll_type(self.cx, ty)?;
                    let n = self.fresh();
                    self.inst(&format!(
                        "{n} = getelementptr {sty}, ptr {ptr}, i32 0, i32 {idx}"
                    ));
                    ptr = n;
                    ty = self.cx.field_def(*field).ty;
                }
                Proj::Index { index, .. } => {
                    let (idx, _) = self.operand(index)?;
                    if llvm_ty::is_ptr(self.cx, ty) {
                        return Err("index of ptr".to_string());
                    }
                    if matches!(self.cx.ty_kind(ty), TyKind::Slice { .. } | TyKind::String) {
                        let base = self.fresh();
                        self.inst(&format!(
                            "{base} = getelementptr %j3_string, ptr {ptr}, i32 0, i32 0"
                        ));
                        let p = self.load("ptr", &base);
                        let elem = self.cx.elem_ty(ty).ok_or("index elem")?;
                        let et = ll_type(self.cx, elem)?;
                        let n = self.fresh();
                        self.inst(&format!(
                            "{n} = getelementptr {et}, ptr {p}, i64 {idx}"
                        ));
                        ptr = n;
                        ty = elem;
                    } else {
                        let elem = self.cx.elem_ty(ty).ok_or("index elem")?;
                        let at = ll_type(self.cx, ty)?;
                        let n = self.fresh();
                        self.inst(&format!(
                            "{n} = getelementptr {at}, ptr {ptr}, i64 0, i64 {idx}"
                        ));
                        ptr = n;
                        ty = elem;
                    }
                }
                Proj::Ptr => {
                    let n = self.fresh();
                    self.inst(&format!(
                        "{n} = getelementptr %j3_string, ptr {ptr}, i32 0, i32 0"
                    ));
                    ptr = n;
                    ty = self
                        .cx
                        .elem_ty(ty)
                        .and_then(|e| self.cx.lookup_ty(&TyKind::Ref(e)))
                        .ok_or("ptr of non-slice")?;
                }
                Proj::Len => {
                    let n = self.fresh();
                    self.inst(&format!(
                        "{n} = getelementptr %j3_string, ptr {ptr}, i32 0, i32 1"
                    ));
                    ptr = n;
                    ty = self.cx.common.int;
                }
                Proj::EnumTag => {
                    let n = self.fresh();
                    self.inst(&format!("{n} = getelementptr i8, ptr {ptr}, i64 0"));
                    ptr = n;
                    ty = self.cx.common.i32;
                }
                Proj::EnumPayload { ty: pty, .. } => {
                    let off = abi::field_offset(self.cx, ty, 1).unwrap_or(4);
                    let n = self.fresh();
                    self.inst(&format!("{n} = getelementptr i8, ptr {ptr}, i64 {off}"));
                    ptr = n;
                    ty = *pty;
                }
            }
        }
        Ok((ptr, ty))
    }

    pub fn operand_const(&mut self, op: &Operand) -> Result<String, String> {
        match op {
            Operand::Const(c) => self.const_bits(c.ty, &c.kind),
            Operand::Copy(_) => Err("const init needs a constant".to_string()),
        }
    }

    fn operand(&mut self, op: &Operand) -> Result<(String, String), String> {
        match op {
            Operand::Copy(place) => {
                let (ptr, ty) = self.place_ptr(place)?;
                if matches!(self.cx.ty_kind(ty), TyKind::Array { .. }) {
                    return Ok((ptr, ll_type(self.cx, ty)?));
                }
                let ll = ll_type(self.cx, ty)?;
                Ok((self.load(&ll, &ptr), ll))
            }
            Operand::Const(c) => {
                let ll = ll_type(self.cx, c.ty)?;
                Ok((self.const_val(c.ty, &c.kind)?, ll))
            }
        }
    }

    fn const_bits(&self, ty: Ty, kind: &ConstKind) -> Result<String, String> {
        match kind {
            ConstKind::Bool(true) => Ok("1".to_string()),
            ConstKind::Bool(false) => Ok("0".to_string()),
            ConstKind::Int(bits) => {
                if let TyKind::Int(int_ty) = self.cx.ty_kind(ty) {
                    if int_ty.is_signed() {
                        return Ok(format!("{}", *bits as i64));
                    }
                }
                Ok(format!("{bits}"))
            }
            ConstKind::F32Bits(b) => {
                // LLVM `0x` hex is IEEE *double* bits even for `float`.
                let f = f32::from_bits(*b);
                Ok(format!("0x{:016X}", (f as f64).to_bits()))
            }
            ConstKind::F64Bits(b) => Ok(format!("0x{b:016X}")),
            ConstKind::Null => Ok("null".to_string()),
            ConstKind::Void => Ok("undef".to_string()),
            ConstKind::FnAddr(_) | ConstKind::Bytes(_) => {
                Err("const init of fn/bytes not in global scalar".to_string())
            }
        }
    }

    fn const_val(&mut self, ty: Ty, kind: &ConstKind) -> Result<String, String> {
        match kind {
            ConstKind::FnAddr(id) => {
                let func = self.cx.func(*id);
                let name = match &func.kind {
                    FuncKind::Lib { symbol, .. } => c_ty::sanitize(self.cx.name_str(*symbol)),
                    _ => c_expr::func_c_name(self.cx, func),
                };
                Ok(format!("@{name}"))
            }
            ConstKind::Bytes(bytes) => {
                let lit = self.bytes_global(bytes);
                let s = self.fresh();
                let t = self.fresh();
                self.inst(&format!(
                    "{s} = insertvalue %j3_string poison, ptr {lit}, 0"
                ));
                self.inst(&format!(
                    "{t} = insertvalue %j3_string {s}, i64 {}, 1",
                    bytes.len()
                ));
                let _ = ty;
                Ok(t)
            }
            other => self.const_bits(ty, other),
        }
    }

    fn bytes_global(&mut self, bytes: &[u8]) -> String {
        let n = STR_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let name = format!("@.b{n}");
        let lit = ll_bytes_lit(bytes);
        // Module-level constants are prepended by rewriting: emit as a commentless
        // function-local via alloca+store for tiny lits, or a private global appended
        // at the top by collecting. We inject into `text` at the start via a marker.
        self.globals.push_str(&format!(
            "{name} = private unnamed_addr constant [{} x i8] {lit}\n",
            bytes.len()
        ));
        let p = self.fresh();
        self.inst(&format!(
            "{p} = getelementptr [{} x i8], ptr {name}, i64 0, i64 0",
            bytes.len()
        ));
        p
    }

    fn rvalue(&mut self, rv: &Rvalue) -> Result<(String, String), String> {
        let ll = ll_type(self.cx, rv.ty)?;
        match &rv.kind {
            RvalueKind::Use(op) => self.operand(op),
            RvalueKind::Ref { place } => {
                let (ptr, _) = self.place_ptr(place)?;
                Ok((ptr, "ptr".to_string()))
            }
            RvalueKind::Binary { op, lhs, rhs } => {
                let (l, _) = self.operand(lhs)?;
                let (r, _) = self.operand(rhs)?;
                let v = self.binop(rv.ty, lhs.ty(), *op, &l, &r)?;
                Ok((v, ll))
            }
            RvalueKind::Unary { op, arg } => {
                let (a, at) = self.operand(arg)?;
                let v = match op {
                    UnaryOp::Not => {
                        let c = self.fresh();
                        self.inst(&format!("{c} = icmp eq {at} {a}, 0"));
                        let z = self.fresh();
                        self.inst(&format!("{z} = zext i1 {c} to i8"));
                        z
                    }
                    UnaryOp::Neg if crate::emit::c_ty::is_int(self.cx, rv.ty) => {
                        self.binop(rv.ty, rv.ty, BinOp::Sub, "0", &a)?
                    }
                    UnaryOp::Neg => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = fneg {ll} {a}"));
                        v
                    }
                    UnaryOp::BitNot => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = xor {ll} {a}, -1"));
                        v
                    }
                };
                Ok((v, ll))
            }
            RvalueKind::Call { func, args } => {
                let (f, _) = self.operand(func)?;
                let mut alist = String::new();
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        alist.push_str(", ");
                    }
                    let (v, t) = self.operand(a)?;
                    let v = if matches!(self.cx.ty_kind(a.ty()), TyKind::Array { .. }) {
                        self.load(&t, &v)
                    } else {
                        v
                    };
                    alist.push_str(&format!("{t} {v}"));
                }
                if rv.ty == self.cx.common.void {
                    self.inst(&format!("call void {f}({alist})"));
                    Ok(("undef".to_string(), "void".to_string()))
                } else {
                    let v = self.fresh();
                    self.inst(&format!("{v} = call {ll} {f}({alist})"));
                    if matches!(self.cx.ty_kind(rv.ty), TyKind::Array { .. }) {
                        // Array rvalues use storage addresses inside a function.
                        let slot = self.fresh();
                        self.inst(&format!("{slot} = alloca {ll}"));
                        self.store(&ll, &v, &slot)?;
                        Ok((slot, ll))
                    } else {
                        Ok((v, ll))
                    }
                }
            }
            RvalueKind::Syscall { nr, args } => {
                let (n, _) = self.operand(nr)?;
                let mut filled = Vec::new();
                for a in args {
                    filled.push(self.syscall_word(a)?);
                }
                while filled.len() < 6 {
                    filled.push("0".to_string());
                }
                let v = self.fresh();
                self.inst(&format!(
                    "{v} = call i64 (i64, ...) @syscall(i64 {n}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {}, i64 {})",
                    filled[0], filled[1], filled[2], filled[3], filled[4], filled[5]
                ));
                Ok((v, "i64".to_string()))
            }
            RvalueKind::Atomic { op, args } => {
                let (p, _) = self.operand(&args[0])?;
                let mut vals = Vec::new();
                for a in &args[1..] {
                    vals.push(self.syscall_word(a)?);
                }
                use crate::ir::op::AtomicOp;
                match op {
                    AtomicOp::Load => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = load atomic i64, ptr {p} seq_cst, align 8"));
                        Ok((v, "i64".to_string()))
                    }
                    AtomicOp::Store => {
                        self.inst(&format!("store atomic i64 {}, ptr {p} seq_cst, align 8", vals[0]));
                        Ok(("0".to_string(), "i64".to_string()))
                    }
                    AtomicOp::Add => {
                        let v = self.fresh();
                        self.inst(&format!("{v} = atomicrmw add ptr {p}, i64 {} seq_cst", vals[0]));
                        Ok((v, "i64".to_string()))
                    }
                    AtomicOp::Cas => {
                        let r = self.fresh();
                        self.inst(&format!(
                            "{r} = cmpxchg ptr {p}, i64 {}, i64 {} seq_cst seq_cst",
                            vals[0], vals[1]
                        ));
                        let ok = self.fresh();
                        self.inst(&format!("{ok} = extractvalue {{ i64, i1 }} {r}, 1"));
                        let z = self.fresh();
                        self.inst(&format!("{z} = zext i1 {ok} to i8"));
                        Ok((z, "i8".to_string()))
                    }
                }
            }
            RvalueKind::Aggregate { fields } => self.aggregate(rv.ty, fields),
            RvalueKind::Enum { variant, payload } => {
                if self.cx.enum_def(variant.enum_id).is_c_like() {
                    return Ok((format!("{}", variant.index), "i32".to_string()));
                }
                let slot = self.fresh();
                self.inst(&format!("{slot} = alloca {ll}"));
                self.inst(&format!("store i32 {}, ptr {slot}", variant.index));
                if let Some(p) = payload {
                    let (v, vt) = self.operand(p)?;
                    let off = abi::field_offset(self.cx, rv.ty, 1).unwrap_or(4);
                    let pp = self.fresh();
                    self.inst(&format!("{pp} = getelementptr i8, ptr {slot}, i64 {off}"));
                    self.store(&vt, &v, &pp)?;
                }
                Ok((self.load(&ll, &slot), ll))
            }
            RvalueKind::Cast { from } => {
                let (v, ft) = self.operand(from)?;
                Ok((self.cast(from.ty(), rv.ty, &v, &ft, &ll)?, ll))
            }
            RvalueKind::Interpolate { .. } => {
                Err("interpolate must be InterpFill before LLVM emit".to_string())
            }
        }
    }

    fn syscall_word(&mut self, op: &Operand) -> Result<String, String> {
        let (v, ty) = self.operand(op)?;
        if ty == "ptr" {
            let n = self.fresh();
            self.inst(&format!("{n} = ptrtoint ptr {v} to i64"));
            Ok(n)
        } else if ty == "i64" {
            Ok(v)
        } else {
            let n = self.fresh();
            self.inst(&format!("{n} = sext {ty} {v} to i64"));
            Ok(n)
        }
    }

    fn cast(&mut self, from: Ty, to: Ty, v: &str, ft: &str, tt: &str) -> Result<String, String> {
        if ft == tt {
            return Ok(v.to_string());
        }
        let n = self.fresh();
        if llvm_ty::is_ptr(self.cx, from) && llvm_ty::is_ptr(self.cx, to) {
            return Ok(v.to_string());
        }
        if llvm_ty::is_ptr(self.cx, from) && tt == "i64" {
            self.inst(&format!("{n} = ptrtoint ptr {v} to i64"));
            return Ok(n);
        }
        if ft == "i64" && llvm_ty::is_ptr(self.cx, to) {
            self.inst(&format!("{n} = inttoptr i64 {v} to ptr"));
            return Ok(n);
        }
        if c_ty::is_int(self.cx, from) && c_ty::is_int(self.cx, to) {
            let fb = match self.cx.ty_kind(from) {
                TyKind::Int(i) => i.bits(),
                _ => 64,
            };
            let tb = match self.cx.ty_kind(to) {
                TyKind::Int(i) => i.bits(),
                _ => 64,
            };
            if tb > fb {
                let op = if matches!(self.cx.ty_kind(from), TyKind::Int(i) if i.is_signed()) {
                    "sext"
                } else {
                    "zext"
                };
                self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            } else {
                self.inst(&format!("{n} = trunc {ft} {v} to {tt}"));
            }
            return Ok(n);
        }
        if c_ty::is_int(self.cx, from) && matches!(self.cx.ty_kind(to), TyKind::Float(_)) {
            let op = if matches!(self.cx.ty_kind(from), TyKind::Int(i) if i.is_signed()) {
                "sitofp"
            } else {
                "uitofp"
            };
            self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            return Ok(n);
        }
        if matches!(self.cx.ty_kind(from), TyKind::Float(_)) && c_ty::is_int(self.cx, to) {
            let op = if matches!(self.cx.ty_kind(to), TyKind::Int(i) if i.is_signed()) {
                "fptosi"
            } else {
                "fptoui"
            };
            self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            return Ok(n);
        }
        if matches!(self.cx.ty_kind(from), TyKind::Float(_))
            && matches!(self.cx.ty_kind(to), TyKind::Float(_))
        {
            let op = if tt == "double" { "fpext" } else { "fptrunc" };
            self.inst(&format!("{n} = {op} {ft} {v} to {tt}"));
            return Ok(n);
        }
        if from == self.cx.common.string || matches!(self.cx.ty_kind(from), TyKind::Slice { .. }) {
            if to == self.cx.common.string || matches!(self.cx.ty_kind(to), TyKind::Slice { .. }) {
                return Ok(v.to_string());
            }
        }
        Err(format!("cast {ft} -> {tt} not in LLVM emit"))
    }

    fn aggregate(&mut self, ty: Ty, fields: &[Operand]) -> Result<(String, String), String> {
        let ll = ll_type(self.cx, ty)?;
        match self.cx.ty_kind(ty) {
            TyKind::Slice { .. } | TyKind::String => {
                let (p, _) = self.operand(&fields[0])?;
                let (n, _) = self.operand(&fields[1])?;
                let a = self.fresh();
                let b = self.fresh();
                self.inst(&format!(
                    "{a} = insertvalue %j3_string poison, ptr {p}, 0"
                ));
                self.inst(&format!("{b} = insertvalue %j3_string {a}, i64 {n}, 1"));
                Ok((b, "%j3_string".to_string()))
            }
            TyKind::Struct(_) | TyKind::Array { .. } => {
                let slot = self.fresh();
                self.inst(&format!("{slot} = alloca {ll}"));
                for (i, f) in fields.iter().enumerate() {
                    let (v, vt) = self.operand(f)?;
                    let p = self.fresh();
                    self.inst(&format!(
                        "{p} = getelementptr {ll}, ptr {slot}, i32 0, i32 {i}"
                    ));
                    if matches!(self.cx.ty_kind(f.ty()), TyKind::Array { .. }) {
                        let n = llvm_ty::size(self.cx, f.ty());
                        self.inst(&format!(
                            "call void @llvm.memcpy.p0.p0.i64(ptr {p}, ptr {v}, i64 {n}, i1 false)"
                        ));
                    } else {
                        self.store(&vt, &v, &p)?;
                    }
                }
                if matches!(self.cx.ty_kind(ty), TyKind::Array { .. }) {
                    Ok((slot, ll))
                } else {
                    Ok((self.load(&ll, &slot), ll))
                }
            }
            _ => Err("aggregate of this type".to_string()),
        }
    }

    fn binop(
        &mut self,
        result_ty: Ty,
        lhs_ty: Ty,
        op: BinOp,
        l: &str,
        r: &str,
    ) -> Result<String, String> {
        let ll = ll_type(self.cx, result_ty)?;
        let v = self.fresh();
        if matches!(
            op,
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge
        ) {
            let (ipred, fpred) = match op {
                BinOp::Eq => ("eq", "oeq"),
                BinOp::Ne => ("ne", "one"),
                BinOp::Lt => ("slt", "olt"),
                BinOp::Le => ("sle", "ole"),
                BinOp::Gt => ("sgt", "ogt"),
                BinOp::Ge => ("sge", "oge"),
                _ => unreachable!(),
            };
            if llvm_ty::is_ptr(self.cx, lhs_ty) {
                self.inst(&format!("{v} = icmp {ipred} ptr {l}, {r}"));
            } else if matches!(self.cx.ty_kind(lhs_ty), TyKind::Float(_)) {
                let ft = ll_type(self.cx, lhs_ty)?;
                self.inst(&format!("{v} = fcmp {fpred} {ft} {l}, {r}"));
            } else {
                let it = ll_type(self.cx, lhs_ty)?;
                let pred = if matches!(self.cx.ty_kind(lhs_ty), TyKind::Int(i) if !i.is_signed()) {
                    match op {
                        BinOp::Lt => "ult",
                        BinOp::Le => "ule",
                        BinOp::Gt => "ugt",
                        BinOp::Ge => "uge",
                        other => {
                            let _ = other;
                            ipred
                        }
                    }
                } else {
                    ipred
                };
                self.inst(&format!("{v} = icmp {pred} {it} {l}, {r}"));
            }
            let z = self.fresh();
            self.inst(&format!("{z} = zext i1 {v} to i8"));
            return Ok(z);
        }
        if matches!(self.cx.ty_kind(result_ty), TyKind::Float(_)) {
            let inst = match op {
                BinOp::Add => "fadd",
                BinOp::Sub => "fsub",
                BinOp::Mul => "fmul",
                BinOp::Div => "fdiv",
                BinOp::Rem => "frem",
                _ => return Err("bad float op".to_string()),
            };
            self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
            return Ok(v);
        }
        if matches!(op, BinOp::And | BinOp::Or) {
            let inst = if op == BinOp::And { "and" } else { "or" };
            self.inst(&format!("{v} = {inst} i8 {l}, {r}"));
            return Ok(v);
        }
        if wrap::wrapping_bin(op) && c_ty::is_int(self.cx, result_ty) {
            return self.wrap_int(result_ty, op, l, r);
        }
        let inst = match op {
            BinOp::Add => "add",
            BinOp::Sub => "sub",
            BinOp::Mul => "mul",
            BinOp::BitAnd => "and",
            BinOp::BitOr => "or",
            BinOp::BitXor => "xor",
            BinOp::Shl => "shl",
            BinOp::Shr => "lshr",
            _ => return Err(format!("binop {}", op.as_str())),
        };
        self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
        Ok(v)
    }

    fn wrap_int(&mut self, ty: Ty, op: BinOp, l: &str, r: &str) -> Result<String, String> {
        let ll = ll_type(self.cx, ty)?;
        match op {
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Shl | BinOp::Shr => {
                let inst = match op {
                    BinOp::Add => "add",
                    BinOp::Sub => "sub",
                    BinOp::Mul => "mul",
                    BinOp::Shl => "shl",
                    BinOp::Shr => {
                        if matches!(self.cx.ty_kind(ty), TyKind::Int(i) if i.is_signed()) {
                            "ashr"
                        } else {
                            "lshr"
                        }
                    }
                    _ => unreachable!(),
                };
                let v = self.fresh();
                self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
                Ok(v)
            }
            BinOp::Div | BinOp::Rem => {
                let signed = matches!(self.cx.ty_kind(ty), TyKind::Int(i) if i.is_signed());
                if signed {
                    let bits = match self.cx.ty_kind(ty) {
                        TyKind::Int(i) => i.bits(),
                        _ => 64,
                    };
                    let min = match bits {
                        8 => "-128",
                        16 => "-32768",
                        32 => "-2147483648",
                        _ => "-9223372036854775808",
                    };
                    let ismin = self.fresh();
                    let ism1 = self.fresh();
                    let ov = self.fresh();
                    self.inst(&format!("{ismin} = icmp eq {ll} {l}, {min}"));
                    self.inst(&format!("{ism1} = icmp eq {ll} {r}, -1"));
                    self.inst(&format!("{ov} = and i1 {ismin}, {ism1}"));
                    let ov_bb = self.label();
                    let norm_bb = self.label();
                    let join = self.label();
                    self.term(&format!("br i1 {ov}, label %{ov_bb}, label %{norm_bb}"));
                    self.block(&ov_bb);
                    let wrapped = if op == BinOp::Div { min } else { "0" };
                    self.term(&format!("br label %{join}"));
                    self.block(&norm_bb);
                    let norm = self.fresh();
                    if op == BinOp::Div {
                        self.inst(&format!("{norm} = sdiv {ll} {l}, {r}"));
                    } else {
                        self.inst(&format!("{norm} = srem {ll} {l}, {r}"));
                    }
                    self.term(&format!("br label %{join}"));
                    self.block(&join);
                    let v = self.fresh();
                    self.inst(&format!(
                        "{v} = phi {ll} [ {wrapped}, %{ov_bb} ], [ {norm}, %{norm_bb} ]"
                    ));
                    Ok(v)
                } else {
                    let inst = if op == BinOp::Div { "udiv" } else { "urem" };
                    let v = self.fresh();
                    self.inst(&format!("{v} = {inst} {ll} {l}, {r}"));
                    Ok(v)
                }
            }
            _ => Err("wrap op".to_string()),
        }
    }

    fn interp_fill(
        &mut self,
        buf: &Place,
        used: &Place,
        parts: &[InterpPart],
    ) -> Result<(), String> {
        let (bp, _) = self.place_ptr(buf)?;
        let (up, _) = self.place_ptr(used)?;
        let cap = wrap::INTERP_CAP;
        for part in parts {
            match part {
                InterpPart::Lit(bytes) => {
                    self.interp_memcpy(&bp, &up, bytes, cap)?;
                }
                InterpPart::Value(op) => {
                    let (e, et) = self.operand(op)?;
                    self.interp_value(&bp, &up, &e, op.ty(), &et, cap)?;
                }
            }
        }
        Ok(())
    }

    fn interp_memcpy(
        &mut self,
        buf: &str,
        used: &str,
        bytes: &[u8],
        cap: u64,
    ) -> Result<(), String> {
        let n = bytes.len() as u64;
        let u = self.load("i64", used);
        let sum = self.fresh();
        self.inst(&format!("{sum} = add i64 {u}, {n}"));
        let ok = self.fresh();
        self.inst(&format!("{ok} = icmp sle i64 {sum}, {cap}"));
        let t = self.label();
        let e = self.label();
        self.term(&format!("br i1 {ok}, label %{t}, label %{e}"));
        self.block(&e);
        self.exit_abort(AbortKind::Fmt);
        self.block(&t);
        let dst = self.fresh();
        self.inst(&format!("{dst} = getelementptr i8, ptr {buf}, i64 {u}"));
        if !bytes.is_empty() {
            let src = self.bytes_global(bytes);
            self.inst(&format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {src}, i64 {n}, i1 false)"
            ));
        }
        self.store("i64", &sum, used)?;
        Ok(())
    }

    fn interp_value(
        &mut self,
        buf: &str,
        used: &str,
        e: &str,
        ty: Ty,
        et: &str,
        cap: u64,
    ) -> Result<(), String> {
        if ty == self.cx.common.string {
            let p = self.fresh();
            let n = self.fresh();
            self.inst(&format!("{p} = extractvalue %j3_string {e}, 0"));
            self.inst(&format!("{n} = extractvalue %j3_string {e}, 1"));
            let u = self.load("i64", used);
            let sum = self.fresh();
            self.inst(&format!("{sum} = add i64 {u}, {n}"));
            let ok = self.fresh();
            self.inst(&format!("{ok} = icmp sle i64 {sum}, {cap}"));
            let t = self.label();
            let bad = self.label();
            self.term(&format!("br i1 {ok}, label %{t}, label %{bad}"));
            self.block(&bad);
            self.exit_abort(AbortKind::Fmt);
            self.block(&t);
            let dst = self.fresh();
            self.inst(&format!("{dst} = getelementptr i8, ptr {buf}, i64 {u}"));
            self.inst(&format!(
                "call void @llvm.memcpy.p0.p0.i64(ptr {dst}, ptr {p}, i64 {n}, i1 false)"
            ));
            self.store("i64", &sum, used)?;
            return Ok(());
        }
        if ty == self.cx.common.bool {
            let c = self.fresh();
            self.inst(&format!("{c} = icmp ne i8 {e}, 0"));
            let p = self.fresh();
            self.inst(&format!(
                "{p} = select i1 {c}, ptr @.j3_true, ptr @.j3_false"
            ));
            return self.interp_snprintf(buf, used, "@.j3_fmt_s", "ptr", &p, cap);
        }
        if c_ty::is_int(self.cx, ty) {
            let mut val = e.to_string();
            if et != "i64" {
                let n = self.fresh();
                let signed = matches!(self.cx.ty_kind(ty), TyKind::Int(IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64));
                let op = if signed { "sext" } else { "zext" };
                if et == "i64" {
                    val = e.to_string();
                } else {
                    self.inst(&format!("{n} = {op} {et} {e} to i64"));
                    val = n;
                }
            }
            return self.interp_snprintf(buf, used, "@.j3_fmt_i64", "i64", &val, cap);
        }
        if matches!(self.cx.ty_kind(ty), TyKind::Float(_)) {
            let mut val = e.to_string();
            if et != "double" {
                let n = self.fresh();
                self.inst(&format!("{n} = fpext {et} {e} to double"));
                val = n;
            }
            return self.interp_snprintf(buf, used, "@.j3_fmt_f", "double", &val, cap);
        }
        if let TyKind::Struct(id) = self.cx.ty_kind(ty) {
            let def = self.cx.struct_def(*id);
            if !def.debug {
                return Err("interpolate of non-#[Debug] struct".to_string());
            }
            let name = self.cx.name_str(def.name).to_string();
            self.interp_memcpy(buf, used, format!("{name} {{ ").as_bytes(), cap)?;
            let fields = def.fields.clone();
            for (i, field) in fields.iter().enumerate() {
                if i > 0 {
                    self.interp_memcpy(buf, used, b", ", cap)?;
                }
                let fname = self.cx.name_str(field.name).to_string();
                self.interp_memcpy(buf, used, format!("{fname}: ").as_bytes(), cap)?;
                let slot = self.fresh();
                let ll = ll_type(self.cx, ty)?;
                self.inst(&format!("{slot} = alloca {ll}"));
                self.store(&ll, e, &slot)?;
                let fp = self.fresh();
                self.inst(&format!(
                    "{fp} = getelementptr {ll}, ptr {slot}, i32 0, i32 {i}"
                ));
                let ft = ll_type(self.cx, field.ty)?;
                let fv = self.load(&ft, &fp);
                self.interp_value(buf, used, &fv, field.ty, &ft, cap)?;
            }
            self.interp_memcpy(buf, used, b" }", cap)?;
            return Ok(());
        }
        Err("interpolate of this type is not implemented".to_string())
    }

    fn interp_snprintf(
        &mut self,
        buf: &str,
        used: &str,
        fmt: &str,
        vty: &str,
        val: &str,
        cap: u64,
    ) -> Result<(), String> {
        let u = self.load("i64", used);
        let room = self.fresh();
        self.inst(&format!("{room} = sub i64 {cap}, {u}"));
        let ok = self.fresh();
        self.inst(&format!("{ok} = icmp sgt i64 {room}, 0"));
        let t = self.label();
        let bad = self.label();
        self.term(&format!("br i1 {ok}, label %{t}, label %{bad}"));
        self.block(&bad);
        self.exit_abort(AbortKind::Fmt);
        self.block(&t);
        let dst = self.fresh();
        self.inst(&format!("{dst} = getelementptr i8, ptr {buf}, i64 {u}"));
        let w = self.fresh();
        self.inst(&format!(
            "{w} = call i32 (ptr, i64, ptr, ...) @snprintf(ptr {dst}, i64 {room}, ptr {fmt}, {vty} {val})"
        ));
        let wi = self.fresh();
        self.inst(&format!("{wi} = sext i32 {w} to i64"));
        let badw = self.fresh();
        self.inst(&format!("{badw} = icmp slt i64 {wi}, 0"));
        let ge = self.fresh();
        self.inst(&format!("{ge} = icmp sge i64 {wi}, {room}"));
        let fail = self.fresh();
        self.inst(&format!("{fail} = or i1 {badw}, {ge}"));
        let ok2 = self.label();
        let bad2 = self.label();
        self.term(&format!("br i1 {fail}, label %{bad2}, label %{ok2}"));
        self.block(&bad2);
        self.exit_abort(AbortKind::Fmt);
        self.block(&ok2);
        let sum = self.fresh();
        self.inst(&format!("{sum} = add i64 {u}, {wi}"));
        self.store("i64", &sum, used)?;
        Ok(())
    }
}

/// Line locations plus named locals (`DILocalVariable` / `dbg.declare`).
pub(crate) struct DebugOut {
    on: bool,
    next: u32,
    body: String,
    subs: HashMap<FuncId, u32>,
    locs: HashMap<(u32, u32), u32>,
    files: HashMap<FileId, u32>,
    tys: HashMap<Ty, u32>,
    expr: u32,
    empty: u32,
}

impl DebugOut {
    pub fn off() -> DebugOut {
        DebugOut {
            on: false,
            next: 0,
            body: String::new(),
            subs: HashMap::new(),
            locs: HashMap::new(),
            files: HashMap::new(),
            tys: HashMap::new(),
            expr: 0,
            empty: 0,
        }
    }

    pub fn prepare(cx: &IrCx, modules: &[Module], keep: Option<&HashSet<FuncId>>) -> DebugOut {
        let mut files: Vec<(crate::span::FileId, u32)> = Vec::new();
        let mut seen = HashSet::new();
        for module in modules {
            for id in &module.funcs {
                let func = cx.func(*id);
                if !matches!(func.kind, FuncKind::J3 { .. }) || func.span.is_dummy() || keep.is_some_and(|k| !k.contains(id)) {
                    continue;
                }
                if cx.file(func.span.file).is_none() {
                    continue;
                }
                if seen.insert(func.span.file) {
                    files.push((func.span.file, 0));
                }
            }
        }
        if files.is_empty() {
            return DebugOut::off();
        }
        let mut dbg = DebugOut {
            on: true,
            next: 0,
            body: String::new(),
            subs: HashMap::new(),
            locs: HashMap::new(),
            files: HashMap::new(),
            tys: HashMap::new(),
            expr: 0,
            empty: 0,
        };
        let cu = dbg.alloc();
        let dwarf = dbg.alloc();
        let ver = dbg.alloc();
        let empty = dbg.alloc();
        let subty = dbg.alloc();
        let types = dbg.alloc();
        for slot in &mut files {
            slot.1 = dbg.alloc();
        }
        let file0 = files[0].1;
        dbg.body.push_str(&format!(
            "!{cu} = distinct !DICompileUnit(language: DW_LANG_C99, file: !{file0}, producer: \"j3c\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug, enums: !{empty}, retainedTypes: !{empty}, globals: !{empty}, imports: !{empty})\n"
        ));
        dbg.body.push_str(&format!(
            "!{dwarf} = !{{i32 7, !\"Dwarf Version\", i32 4}}\n"
        ));
        dbg.body.push_str(&format!(
            "!{ver} = !{{i32 2, !\"Debug Info Version\", i32 3}}\n"
        ));
        dbg.empty = empty;
        dbg.body.push_str(&format!("!{empty} = !{{}}\n"));
        let expr = dbg.alloc();
        dbg.expr = expr;
        dbg.body.push_str(&format!("!{expr} = !DIExpression()\n"));
        dbg.body.push_str(&format!(
            "!{subty} = !DISubroutineType(types: !{types})\n"
        ));
        dbg.body.push_str(&format!("!{types} = !{{null}}\n"));
        for (fid, md) in &files {
            let file = match cx.file(*fid) {
                Some(f) => f,
                None => continue,
            };
            let (name, dir) = file_parts(&file.path);
            dbg.body.push_str(&format!(
                "!{md} = !DIFile(filename: \"{}\", directory: \"{}\")\n",
                ll_md_str(&name),
                ll_md_str(&dir)
            ));
            dbg.files.insert(*fid, *md);
        }
        for module in modules {
            for id in &module.funcs {
                let func = cx.func(*id);
                if !matches!(func.kind, FuncKind::J3 { .. }) || func.span.is_dummy() || keep.is_some_and(|k| !k.contains(id)) {
                    continue;
                }
                let file = match cx.file(func.span.file) {
                    Some(f) => f,
                    None => continue,
                };
                let file_md = match files.iter().find(|(f, _)| *f == func.span.file) {
                    Some((_, md)) => *md,
                    None => continue,
                };
                let line = file.line(func.span.start).max(1) as u32;
                let sp = dbg.alloc();
                let name = ll_md_str(cx.name_str(func.name));
                let link = ll_md_str(&c_expr::func_c_name(cx, func));
                dbg.body.push_str(&format!(
                    "!{sp} = distinct !DISubprogram(name: \"{name}\", linkageName: \"{link}\", scope: !{file_md}, file: !{file_md}, line: {line}, type: !{subty}, scopeLine: {line}, flags: DIFlagPrototyped, spFlags: DISPFlagDefinition, unit: !{cu}, retainedNodes: !{empty})\n"
                ));
                dbg.subs.insert(func.id, sp);
            }
        }
        if dbg.subs.is_empty() {
            return DebugOut::off();
        }
        dbg
    }

    pub fn enabled(&self) -> bool {
        self.on
    }

    pub fn subprogram(&self, id: FuncId) -> Option<u32> {
        self.subs.get(&id).copied()
    }

    pub fn location(&mut self, scope: u32, line: u32) -> u32 {
        let line = line.max(1);
        if let Some(id) = self.locs.get(&(scope, line)) {
            return *id;
        }
        let id = self.alloc();
        self.body.push_str(&format!(
            "!{id} = !DILocation(line: {line}, column: 1, scope: !{scope})\n"
        ));
        self.locs.insert((scope, line), id);
        id
    }

    pub fn file_of(&self, file: FileId) -> Option<u32> {
        self.files.get(&file).copied()
    }

    pub fn local_var(
        &mut self,
        cx: &IrCx,
        scope: u32,
        file: u32,
        name: &str,
        ty: Ty,
        line: u32,
    ) -> u32 {
        let ty_md = self.di_type(cx, ty);
        let id = self.alloc();
        self.body.push_str(&format!(
            "!{id} = !DILocalVariable(name: \"{}\", scope: !{scope}, file: !{file}, line: {line}, type: !{ty_md})\n",
            ll_md_str(name)
        ));
        id
    }

    fn di_type(&mut self, cx: &IrCx, ty: Ty) -> u32 {
        if let Some(id) = self.tys.get(&ty).copied() {
            return id;
        }
        let id = self.alloc();
        self.tys.insert(ty, id);
        let kind = cx.ty_kind(ty).clone();
        match kind {
            TyKind::Void => self.basic(id, "void", 0, 5),
            TyKind::Bool => self.basic(id, "bool", 8, 2),
            TyKind::Int(int_ty) => {
                let (name, enc) = match int_ty {
                    IntTy::I64 => ("int", 5),
                    IntTy::I8 => ("i8", 5),
                    IntTy::I16 => ("i16", 5),
                    IntTy::I32 => ("i32", 5),
                    IntTy::U8 => ("u8", 7),
                    IntTy::U16 => ("u16", 7),
                    IntTy::U32 => ("u32", 7),
                    IntTy::U64 => ("u64", 7),
                };
                self.basic(id, name, int_ty.bits(), enc);
            }
            TyKind::Float(FloatTy::F32) => self.basic(id, "f32", 32, 4),
            TyKind::Float(FloatTy::F64) => self.basic(id, "float", 64, 4),
            TyKind::Ptr | TyKind::CStr | TyKind::Fn { .. } => {
                let base = self.di_type(cx, cx.common.u8);
                self.pointer(id, base);
            }
            TyKind::Ref(inner) => {
                let base = self.di_type(cx, inner);
                self.pointer(id, base);
            }
            TyKind::String => self.pair(cx, id, "string", cx.common.u8),
            TyKind::Slice { elem } => self.pair(cx, id, "slice", elem),
            TyKind::Array { elem, len } => {
                let base = self.di_type(cx, elem);
                let sub = self.alloc();
                self.body
                    .push_str(&format!("!{sub} = !DISubrange(count: {len})\n"));
                let elems = self.md_list(&[sub]);
                let bits = abi::size(cx, ty) * 8;
                let align = abi::align(cx, ty) * 8;
                self.body.push_str(&format!(
                    "!{id} = !DICompositeType(tag: DW_TAG_array_type, baseType: !{base}, size: {bits}, align: {align}, elements: !{elems})\n"
                ));
            }
            TyKind::Struct(sid) => {
                let sname = {
                    let n = cx.struct_def(sid).name;
                    cx.name_str(n).to_string()
                };
                let field_names: Vec<_> = cx.struct_def(sid).fields.iter().map(|f| f.name).collect::<Vec<_>>();
                let field_tys: Vec<Ty> = cx.struct_def(sid).fields.iter().map(|f| f.ty).collect();
                let fields: Vec<(String, Ty)> = field_names
                    .into_iter()
                    .map(|n| cx.name_str(n).to_string())
                    .zip(field_tys)
                    .collect();
                let mut members = Vec::new();
                for (i, (fname, fty)) in fields.iter().enumerate() {
                    if fname.is_empty() {
                        continue;
                    }
                    let Some(off) = abi::field_offset(cx, ty, i) else {
                        continue;
                    };
                    let base = self.di_type(cx, *fty);
                    let mid = self.alloc();
                    let fbits = abi::size(cx, *fty) * 8;
                    let falign = abi::align(cx, *fty) * 8;
                    self.body.push_str(&format!(
                        "!{mid} = !DIDerivedType(tag: DW_TAG_member, name: \"{}\", scope: !{id}, baseType: !{base}, size: {fbits}, align: {falign}, offset: {})\n",
                        ll_md_str(fname),
                        off * 8
                    ));
                    members.push(mid);
                }
                let elems = if members.is_empty() {
                    self.empty
                } else {
                    self.md_list(&members)
                };
                let bits = abi::size(cx, ty) * 8;
                let align = abi::align(cx, ty) * 8;
                self.body.push_str(&format!(
                    "!{id} = !DICompositeType(tag: DW_TAG_structure_type, name: \"{}\", size: {bits}, align: {align}, elements: !{elems})\n",
                    ll_md_str(&sname)
                ));
            }
            TyKind::Enum(eid) => {
                let c_like = cx.enum_def(eid).is_c_like();
                let ename = {
                    let n = cx.enum_def(eid).name;
                    cx.name_str(n).to_string()
                };
                if c_like {
                    self.basic(id, &ename, 32, 5);
                } else {
                    let variant_names: Vec<_> = cx
                        .enum_def(eid)
                        .variants
                        .iter()
                        .map(|v| v.name)
                        .collect::<Vec<_>>();
                    let variant_payloads: Vec<Option<Ty>> = cx
                        .enum_def(eid)
                        .variants
                        .iter()
                        .map(|v| v.payload)
                        .collect();
                    let variants_src: Vec<(String, Ty)> = variant_names
                        .into_iter()
                        .zip(variant_payloads)
                        .filter_map(|(n, p)| p.map(|ty| (cx.name_str(n).to_string(), ty)))
                        .collect();
                    let tag = self.di_type(cx, cx.common.i32);
                    let tag_m = self.alloc();
                    self.body.push_str(&format!(
                        "!{tag_m} = !DIDerivedType(tag: DW_TAG_member, name: \"tag\", scope: !{id}, baseType: !{tag}, size: 32, align: 32, offset: 0)\n"
                    ));
                    let union_id = self.alloc();
                    let mut variants = Vec::new();
                    let mut payload = 0u64;
                    let mut ua = 1u64;
                    for (vname, p) in &variants_src {
                        let s = abi::size(cx, *p);
                        if s > payload {
                            payload = s;
                        }
                        let pa = abi::align(cx, *p);
                        if pa > ua {
                            ua = pa;
                        }
                        let base = self.di_type(cx, *p);
                        let mid = self.alloc();
                        self.body.push_str(&format!(
                            "!{mid} = !DIDerivedType(tag: DW_TAG_member, name: \"{}\", scope: !{union_id}, baseType: !{base}, size: {}, align: {}, offset: 0)\n",
                            ll_md_str(vname),
                            s * 8,
                            pa * 8
                        ));
                        variants.push(mid);
                    }
                    let union_elems = if variants.is_empty() {
                        self.empty
                    } else {
                        self.md_list(&variants)
                    };
                    let ubits = payload * 8;
                    let ualign = ua * 8;
                    self.body.push_str(&format!(
                        "!{union_id} = !DICompositeType(tag: DW_TAG_union_type, name: \"payload\", size: {ubits}, align: {ualign}, elements: !{union_elems})\n"
                    ));
                    let poff = abi::field_offset(cx, ty, 1).unwrap_or(4) * 8;
                    let pay_m = self.alloc();
                    self.body.push_str(&format!(
                        "!{pay_m} = !DIDerivedType(tag: DW_TAG_member, name: \"payload\", scope: !{id}, baseType: !{union_id}, size: {ubits}, align: {ualign}, offset: {poff})\n"
                    ));
                    let elems = self.md_list(&[tag_m, pay_m]);
                    let bits = abi::size(cx, ty) * 8;
                    let align = abi::align(cx, ty) * 8;
                    self.body.push_str(&format!(
                        "!{id} = !DICompositeType(tag: DW_TAG_structure_type, name: \"{}\", size: {bits}, align: {align}, elements: !{elems})\n",
                        ll_md_str(&ename)
                    ));
                }
            }
        }
        id
    }

    fn basic(&mut self, id: u32, name: &str, bits: u32, encoding: u32) {
        self.body.push_str(&format!(
            "!{id} = !DIBasicType(name: \"{}\", size: {bits}, encoding: {encoding})\n",
            ll_md_str(name)
        ));
    }

    fn pointer(&mut self, id: u32, base: u32) {
        self.body.push_str(&format!(
            "!{id} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{base}, size: 64, align: 64)\n"
        ));
    }

    fn pair(&mut self, cx: &IrCx, id: u32, name: &str, pointee: Ty) {
        let base = self.di_type(cx, pointee);
        let ptr = self.alloc();
        self.pointer(ptr, base);
        let len = self.di_type(cx, cx.common.i64);
        let p = self.alloc();
        let n = self.alloc();
        self.body.push_str(&format!(
            "!{p} = !DIDerivedType(tag: DW_TAG_member, name: \"ptr\", scope: !{id}, baseType: !{ptr}, size: 64, align: 64, offset: 0)\n"
        ));
        self.body.push_str(&format!(
            "!{n} = !DIDerivedType(tag: DW_TAG_member, name: \"len\", scope: !{id}, baseType: !{len}, size: 64, align: 64, offset: 64)\n"
        ));
        let elems = self.md_list(&[p, n]);
        self.body.push_str(&format!(
            "!{id} = !DICompositeType(tag: DW_TAG_structure_type, name: \"{name}\", size: 128, align: 64, elements: !{elems})\n"
        ));
    }

    fn md_list(&mut self, ids: &[u32]) -> u32 {
        let id = self.alloc();
        let inner = ids
            .iter()
            .map(|n| format!("!{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.body.push_str(&format!("!{id} = !{{{inner}}}\n"));
        id
    }

    pub fn finish(&self) -> String {
        if !self.on {
            return String::new();
        }
        let mut out = String::new();
        out.push_str("!llvm.dbg.cu = !{!0}\n");
        out.push_str("!llvm.module.flags = !{!1, !2}\n");
        out.push_str(&self.body);
        out
    }

    fn alloc(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

fn file_parts(path: &str) -> (String, String) {
    let p = std::path::Path::new(path);
    let name = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string();
    let dir = p
        .parent()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(".")
        .to_string();
    (name, dir)
}

fn ll_md_str(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out
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
