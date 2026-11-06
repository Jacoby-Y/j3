//! Using closures: `c(args)`, a plain fn standing in for a closure (thunk), and the stdlib-only
//! `@closureFn` / `@closureCtx` / `@closureFrom`. Literals and captures are in `closure.rs`.

use super::*;
use crate::ir::entity::{Func, Vis};

impl<'a> Lower<'a> {
    /// `@closureFn(c)`, `@closureCtx(c)`: the two words of a closure as `ptr`s; `@closureFrom(f, ctx)`
    /// rebuilds one (its type comes from the expected type). Stdlib only: a hand-written trampoline
    /// uses them to carry a closure through a `ptr` context, and must finish before the creator returns.
    pub(super) fn closure_intrinsic(
        &mut self,
        span: Span,
        name: &Ident,
        args: &[Expr],
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        if let Some(n) = name.name.strip_prefix('@').filter(|n| crate::parse::syntax::INTRINSIC_GPU.contains(n)) {
            return Err(if n == "shader" {
                CheckError::new(self.path, name.span, "`@shader` pairs a vertex and a fragment fn in a module `const`")
                    .help("write `const PROG: Source = @shader(vs, fs);` at module level, with `use j3.gpu.{ Source };`")
            } else {
                CheckError::new(self.path, name.span, format!("`{}` is a shader builtin; it works only inside a `#[shader]` fn", name.name))
                    .help("mark the enclosing fn `#[shader]`, `#[shader(vertex)]` or `#[shader(fragment)]` (`j3c doc shaders`)")
            });
        }
        // Stdlib origin, not the module name: a user file may say `module j3;`.
        if !in_stdlib(self.path) {
            return Err(CheckError::new(self.path, name.span, format!("`{}` is for the standard library only", name.name))
                .help("take a `closure(..)` parameter and call it; a closure cannot be turned into a ptr"));
        }
        let ptr = self.cx.common.ptr;
        let want = if name.name == "@closureFrom" { 2 } else { 1 };
        if args.len() != want {
            return Err(CheckError::new(self.path, span, format!("`{}` takes {want} argument(s)", name.name)));
        }
        if want == 2 {
            let Some(ty) = expected.filter(|t| self.cx.closure_sig(*t).is_some()) else {
                return Err(CheckError::new(self.path, span, "`@closureFrom` needs a `closure(..)` type to build")
                    .help("annotate the `let` or parameter that receives it"));
            };
            let sig = self.cx.closure_sig(ty).cloned().expect("closure type");
            let f = self.operand(&args[0], Some(ptr))?;
            let ctx = self.operand(&args[1], Some(ptr))?;
            let fty = self.cx.closure_fn_ty(&sig.0, sig.1);
            let cast = Rvalue::cast(span, fty, f);
            let fp = self.materialize_rv(cast, span);
            return Ok(Rvalue::aggregate(span, ty, vec![Operand::Copy(fp), ctx]));
        }
        let c = self.operand(&args[0], None)?;
        if self.cx.closure_sig(c.ty()).is_none() {
            return Err(self.mismatch(args[0].span, &format!("`{}` takes a closure", name.name), c.ty(), ptr));
        }
        let place = self.materialize(c, span);
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => unreachable!("closure types are structs"),
        };
        if name.name == "@closureFn" {
            let f = self.b.field(self.cx, place, FieldId { struct_id: sid, index: 0 }, span);
            Ok(Rvalue::cast(span, ptr, Operand::Copy(f)))
        } else {
            let c = self.b.field(self.cx, place, FieldId { struct_id: sid, index: 1 }, span);
            Ok(Rvalue::use_op(span, Operand::Copy(c)))
        }
    }

    /// `c(args)` where `c` is a closure value: `c.$f(c.$ctx, args)`.
    pub(super) fn call_closure(&mut self, span: Span, func: Operand, args: &[Expr]) -> Result<Rvalue, CheckError> {
        let (params, ret) = self.cx.closure_sig(func.ty()).cloned().expect("closure type");
        let Operand::Copy(place) = func else {
            return Err(CheckError::new(self.path, span, "cannot call this expression"));
        };
        if params.len() != args.len() {
            return Err(CheckError::new(self.path, span, format!("takes {} args, got {}", params.len(), args.len())));
        }
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => unreachable!("closure types are structs"),
        };
        let f = self.b.field(self.cx, place.clone(), FieldId { struct_id: sid, index: 0 }, span);
        let c = self.b.field(self.cx, place, FieldId { struct_id: sid, index: 1 }, span);
        let mut ops = vec![Operand::Copy(c)];
        for (arg, ty) in args.iter().zip(&params) {
            ops.push(self.operand(arg, Some(*ty))?);
        }
        Ok(Rvalue::call(span, ret, Operand::Copy(f), ops))
    }

    /// A named fn where a `closure(..)` of the same signature is wanted: `{ thunk, null }`.
    pub(super) fn fn_to_closure(&mut self, span: Span, id: FuncId, want: Ty) -> Result<Option<Rvalue>, CheckError> {
        let Some(sig) = self.cx.closure_sig(want).cloned() else { return Ok(None) };
        let fty = self.cx.func(id).ty;
        match self.cx.fn_sig(fty) {
            Some((p, r)) if *p == sig.0 && r == sig.1 => {}
            _ => return Ok(None),
        }
        if self.script_sid.is_some() {
            return Err(self.script_closure_err(span));
        }
        let thunk = self.thunk_for(span, id, fty, &sig.0, sig.1);
        let tty = self.cx.closure_fn_ty(&sig.0, sig.1);
        let ptr = self.cx.common.ptr;
        Ok(Some(Rvalue::aggregate(
            span,
            want,
            vec![Operand::fn_addr(span, tty, thunk), Operand::null(span, ptr)],
        )))
    }

    /// `name$thunk(ctx, args..)` calling `name(args..)`; one per fn.
    fn thunk_for(&mut self, span: Span, id: FuncId, fty: Ty, params: &[Ty], ret: Ty) -> FuncId {
        if let Some(&t) = self.cx.closures.thunks.get(&id) {
            return t;
        }
        let void = self.cx.common.void;
        let mut b = BodyBuilder::new(void);
        let ptr = self.cx.common.ptr;
        let cn = self.cx.intern_name("$ctx");
        b.param(cn, ptr, span);
        let mut args = Vec::new();
        for (i, ty) in params.iter().enumerate() {
            let n = self.cx.intern_name(&format!("a{i}"));
            let p = b.param(n, *ty, span);
            args.push(Operand::Copy(b.local_place(p, span)));
        }
        let call = Rvalue::call(span, ret, Operand::fn_addr(span, fty, id), args);
        if ret == void {
            b.expr(span, call);
            b.return_op(span, None);
        } else {
            let n = self.cx.intern_name("_r");
            let t = b.tmp(n, ret, span);
            let place = b.local_place(t, span);
            b.assign(span, place.clone(), call);
            b.return_op(span, Some(Operand::Copy(place)));
        }
        let name = format!("{}$thunk", self.cx.name_str(self.cx.func(id).name));
        let tname = self.cx.intern_name(&name);
        let tty = self.cx.closure_fn_ty(params, ret);
        let module = self.cx.func(id).module.clone();
        let tid = self.cx.alloc_func(Func {
            id: FuncId(0),
            doc: String::new(),
            module,
            name: tname,
            ty: tty,
            vis: Vis::Private,
            method_of: None,
            kind: FuncKind::J3 { body: b.finish() },
            span,
        });
        self.ir_funcs.push(tid);
        self.cx.closures.thunks.insert(id, tid);
        tid
    }
}

/// Under the stdlib dir, judged lexically: `..` and `.` are resolved first so `<stdlib>/../x.j3` is not inside it.
fn in_stdlib(path: &str) -> bool {
    use std::path::Component;
    let mut norm = std::path::PathBuf::new();
    for c in std::path::Path::new(path).components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                norm.pop();
            }
            other => norm.push(other),
        }
    }
    norm.starts_with(crate::stdlib::stdlib_dir())
}
