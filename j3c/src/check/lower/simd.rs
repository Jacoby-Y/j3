//! `vec4f` / `vec4i`: constructors, `@v*` intrinsics, lanes, and lane-wise operators.
//! IR shapes are in `ir/simd.rs`; the LLVM text is `emit/llvm_expr/simd.rs`.

use super::*;
use crate::ir::simd::{lane_index, SimdOp};
use crate::ir::ty::VecTy;

pub(crate) const VEC_CTORS: [&str; 2] = ["vec4f", "vec4i"];

pub(crate) fn is_vec(cx: &IrCx, ty: Ty) -> bool {
    matches!(cx.ty_kind(ty), TyKind::Vec(_))
}

const NO_COMPARE: &str = "compare lanes (`a.x < b.x`) or use `@vmin` / `@vmax`; there are no lane masks yet";

impl<'a> Lower<'a> {
    fn vec_err(&self, span: Span, msg: impl Into<String>) -> CheckError {
        CheckError::new(self.path, span, msg)
    }

    /// Assign `rv` to a fresh temp and return it as an operand.
    fn simd_tmp(&mut self, span: Span, rv: Rvalue) -> Operand {
        let n = self.fresh("v");
        let id = self.b.tmp(n, rv.ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(span, p.clone(), rv);
        Operand::Copy(p)
    }

    pub(super) fn vec_cmp_error(&self, span: Span, op: parse::BinOp) -> CheckError {
        let sym = ast_binop(op).as_str();
        self.vec_err(span, format!("`{sym}` does not compare vectors")).help(NO_COMPARE)
    }

    /// `vec4f(a, b, c, d)` / `vec4f(x)`; same for `vec4i`.
    /// Scripts run on the VM, which has no vector registers or ops.
    fn no_scripts(&self, span: Span) -> Result<(), CheckError> {
        if self.script_sid.is_some() {
            return Err(self.vec_err(span, "vec4f is not supported in scripts yet")
                .help("scripts run on the VM; use four `f32` values, or move this code into a `.j3` module"));
        }
        Ok(())
    }

    pub(super) fn vec_ctor(&mut self, span: Span, name: &str, args: &[Expr]) -> Result<Rvalue, CheckError> {
        self.no_scripts(span)?;
        let vt = self.cx.builtin_named(name).expect("vector type name");
        let lane = self.cx.vec_lane(vt).expect("vector type");
        let (op, want) = match args.len() {
            1 => (SimdOp::Splat, 1),
            4 => (SimdOp::Build, 4),
            n => {
                return Err(self
                    .vec_err(span, format!("`{name}` takes 4 lanes or 1 value to splat, found {n}"))
                    .help(format!("write `{name}(x, y, z, w)` or `{name}(x)`")))
            }
        };
        let mut ops = Vec::with_capacity(want);
        for a in args {
            let o = self.operand(a, Some(lane))?;
            if o.ty() != lane {
                return Err(self.mismatch(a.span, &format!("`{name}` lane"), o.ty(), lane));
            }
            ops.push(o);
        }
        Ok(Rvalue::simd(span, vt, op, ops))
    }

    pub(super) fn splat(&mut self, span: Span, vt: Ty, x: Operand) -> Operand {
        self.simd_tmp(span, Rvalue::simd(span, vt, SimdOp::Splat, vec![x]))
    }

    /// Right operand of a vector op: a literal takes the lane type, anything else keeps its own.
    pub(super) fn vec_rhs(&mut self, rhs: &Expr, vt: Ty) -> Result<Operand, CheckError> {
        let lane = self.cx.vec_lane(vt);
        if bare_num(rhs) {
            self.operand(rhs, lane)
        } else {
            self.operand(rhs, None)
        }
    }

    /// `l op r` where at least one side is a vector; a scalar of the lane type is splatted.
    pub(super) fn vec_arith(&mut self, span: Span, op: parse::BinOp, l: Operand, r: Operand) -> Result<Rvalue, CheckError> {
        use parse::BinOp as B;
        let vt = if is_vec(self.cx, l.ty()) { l.ty() } else { r.ty() };
        let lane = self.cx.vec_lane(vt).expect("vector type");
        let (mut l, mut r) = (l, r);
        if l.ty() == lane {
            l = self.splat(span, vt, l);
        }
        if r.ty() == lane {
            r = self.splat(span, vt, r);
        }
        if l.ty() != vt || r.ty() != vt {
            let (a, b) = (self.ty_str(l.ty()), self.ty_str(r.ty()));
            let mut e = self.vec_err(span, format!("`{}` needs matching operands: found `{a}` and `{b}`", ast_binop(op).as_str()));
            if is_vec(self.cx, l.ty()) && is_vec(self.cx, r.ty()) {
                e = e.help("convert with `@vfloat` (vec4i to vec4f) or `@vint` (vec4f to vec4i)");
            } else {
                e = e.help(format!("a scalar operand must be `{}`", self.ty_str(lane)));
            }
            return Err(e);
        }
        let is_f = vt == self.cx.common.vec4f;
        match op {
            B::Add | B::Sub | B::Mul | B::Div => {}
            B::Shl | B::Shr | B::BitAnd | B::BitOr | B::BitXor if !is_f => {}
            B::Shl | B::Shr | B::BitAnd | B::BitOr | B::BitXor => {
                return Err(self.vec_err(span, format!("`{}` takes integers", ast_binop(op).as_str())).help("use `vec4i`"));
            }
            _ => {
                return Err(self.vec_err(span, format!("`{}` is not defined on vectors", ast_binop(op).as_str())));
            }
        }
        if op == B::Div && !is_f {
            return self.vec_int_div(span, vt, l, r);
        }
        Ok(Rvalue::binary(span, vt, ast_binop(op), l, r))
    }

    /// `vec4i` `/`: each lane divides like a scalar `i32` (zero divisor aborts, `MIN / -1` wraps).
    fn vec_int_div(&mut self, span: Span, vt: Ty, l: Operand, r: Operand) -> Result<Rvalue, CheckError> {
        let i32t = self.cx.common.i32;
        let l = self.bind_operand(l, span);
        let l = self.materialize(l, span);
        let r = self.bind_operand(r, span);
        let r = self.materialize(r, span);
        let mut lanes = Vec::new();
        for k in 0..4u8 {
            let lp = self.b.vec_lane(self.cx, l.clone(), k, span);
            let rp = self.b.vec_lane(self.cx, r.clone(), k, span);
            self.abort_if_zero(span, &Operand::Copy(rp.clone()))?;
            let q = self.simd_tmp(span, Rvalue::binary(span, i32t, BinOp::Div, Operand::Copy(lp), Operand::Copy(rp)));
            lanes.push(q);
        }
        Ok(Rvalue::simd(span, vt, SimdOp::Build, lanes))
    }

    /// Lane `.x .y .z .w` / `v[0..3]` of a vector place.
    pub(super) fn vec_lane_place(&mut self, place: Place, lane: u8, span: Span) -> Place {
        self.b.vec_lane(self.cx, place, lane, span)
    }

    pub(super) fn vec_index_place(&mut self, place: Place, index: &Expr, span: Span) -> Result<Place, CheckError> {
        match (&index.kind, self.fold_int(index)) {
            (_, Some(k)) if (0..4).contains(&k) => Ok(self.vec_lane_place(place, k as u8, span)),
            (_, Some(k)) => Err(self.vec_err(index.span, format!("lane index {k} is out of range for a 4-lane vector"))
                .help("lanes are 0..3, or `.x .y .z .w`")),
            _ => Err(self.vec_err(index.span, "a vector index must be a constant 0..3")
                .help("write `v.x`, `v[2]`; to pick a lane at run time, store the lanes in an array")),
        }
    }

    pub(super) fn vec_field_place(&mut self, place: Place, field: &Ident) -> Result<Place, CheckError> {
        match lane_index(&field.name) {
            Some(k) => Ok(self.vec_lane_place(place, k, field.span)),
            None => Err(self.vec_err(field.span, format!("no field `{}` on a vector", field.name)).help("lanes are `.x .y .z .w`")),
        }
    }

    fn vec_arg(&mut self, name: &str, arg: &Expr, want: Option<VecTy>) -> Result<Operand, CheckError> {
        let o = self.operand(arg, None)?;
        match (self.cx.ty_kind(o.ty()), want) {
            (TyKind::Vec(v), Some(w)) if *v != w => {
                Err(self.vec_err(arg.span, format!("`@{name}` takes a `{}`, found `{}`", w.name(), v.name())))
            }
            (TyKind::Vec(_), _) => Ok(o),
            _ => Err(self.vec_err(arg.span, format!("`@{name}` takes a vector, found `{}`", self.ty_str(o.ty())))),
        }
    }

    fn arity(&self, span: Span, name: &str, args: &[Expr], n: usize) -> Result<(), CheckError> {
        if args.len() == n {
            return Ok(());
        }
        Err(self.vec_err(span, format!("`@{name}` takes {n} argument{}, found {}", if n == 1 { "" } else { "s" }, args.len())))
    }

    pub(super) fn simd_intrinsic(&mut self, span: Span, name: &str, args: &[Expr]) -> Result<Rvalue, CheckError> {
        self.no_scripts(span)?;
        let c = &self.cx.common;
        let (vec4f, vec4i) = (c.vec4f, c.vec4i);
        match name {
            "vmin" | "vmax" => {
                self.arity(span, name, args, 2)?;
                let a = self.vec_arg(name, &args[0], None)?;
                let b = self.operand(&args[1], None)?;
                if b.ty() != a.ty() {
                    return Err(self.mismatch(args[1].span, &format!("`@{name}` operand"), b.ty(), a.ty())
                        .help(format!("both operands must be vectors of one type; splat a scalar with `{}(x)`", self.ty_str(a.ty()))));
                }
                let op = if name == "vmin" { SimdOp::Min } else { SimdOp::Max };
                Ok(Rvalue::simd(span, a.ty(), op, vec![a, b]))
            }
            "vsqrt" => {
                self.arity(span, name, args, 1)?;
                let a = self.vec_arg(name, &args[0], Some(VecTy::F32x4))?;
                Ok(Rvalue::simd(span, vec4f, SimdOp::Sqrt, vec![a]))
            }
            "vabs" => {
                self.arity(span, name, args, 1)?;
                let a = self.vec_arg(name, &args[0], None)?;
                Ok(Rvalue::simd(span, a.ty(), SimdOp::Abs, vec![a]))
            }
            "vsum" => {
                self.arity(span, name, args, 1)?;
                let a = self.vec_arg(name, &args[0], None)?;
                let lane = self.cx.vec_lane(a.ty()).expect("vector");
                Ok(Rvalue::simd(span, lane, SimdOp::Sum, vec![a]))
            }
            "vfloat" => {
                self.arity(span, name, args, 1)?;
                let a = self.vec_arg(name, &args[0], Some(VecTy::I32x4))?;
                Ok(Rvalue::simd(span, vec4f, SimdOp::ToFloat, vec![a]))
            }
            "vint" => {
                self.arity(span, name, args, 1)?;
                let a = self.vec_arg(name, &args[0], Some(VecTy::F32x4))?;
                Ok(Rvalue::simd(span, vec4i, SimdOp::ToInt, vec![a]))
            }
            "vshuffle" => {
                self.arity(span, name, args, 5)?;
                let a = self.vec_arg(name, &args[0], None)?;
                let mut ix = [0u8; 4];
                for (k, e) in args[1..].iter().enumerate() {
                    match self.fold_int(e) {
                        Some(v) if (0..4).contains(&v) => ix[k] = v as u8,
                        _ => {
                            return Err(self.vec_err(e.span, "`@vshuffle` indices must be constants 0..3")
                                .help("write `@vshuffle(v, 3, 2, 1, 0)`"))
                        }
                    }
                }
                Ok(Rvalue::simd(span, a.ty(), SimdOp::Shuffle(ix), vec![a]))
            }
            "vload" => {
                self.arity(span, name, args, 2)?;
                let (vt, ptr) = self.vec_mem(span, name, &args[0], &args[1], false)?;
                Ok(Rvalue::simd(span, vt, SimdOp::Load, vec![ptr]))
            }
            "vstore" => {
                self.arity(span, name, args, 3)?;
                let (vt, ptr) = self.vec_mem(span, name, &args[0], &args[1], true)?;
                let v = self.operand(&args[2], None)?;
                if v.ty() != vt {
                    return Err(self.mismatch(args[2].span, "`@vstore` value", v.ty(), vt));
                }
                Ok(Rvalue::simd(span, self.cx.common.void, SimdOp::Store, vec![ptr, v]))
            }
            _ => Err(self.vec_err(span, format!("unknown intrinsic `@{name}`"))),
        }
    }

    /// `xs[i..i+4]` of an `[]f32` / `[]i32` (or array): checks `i` and `i + 3` against the length,
    /// then returns the vector type and a `&lane` pointing at `xs[i]`.
    fn vec_mem(&mut self, span: Span, name: &str, xs: &Expr, i: &Expr, store: bool) -> Result<(Ty, Operand), CheckError> {
        let mut place = self.base_place(xs)?;
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, xs.span);
        }
        let elem = match self.cx.ty_kind(place.ty) {
            TyKind::Slice { elem } | TyKind::Array { elem, .. } => Some(*elem),
            _ => None,
        };
        let vt = match elem {
            Some(e) if e == self.cx.common.f32 => self.cx.common.vec4f,
            Some(e) if e == self.cx.common.i32 => self.cx.common.vec4i,
            _ => {
                return Err(self.vec_err(xs.span, format!("`@{name}` takes an `[]f32` or `[]i32`, found `{}`", self.ty_str(place.ty)))
                    .help("the slice's element type picks `vec4f` or `vec4i`"))
            }
        };
        if store && !place.access.assign {
            return Err(self.vec_err(xs.span, "cannot store into this slice"));
        }
        let int = self.cx.common.int;
        let idx = self.operand(i, Some(int))?;
        if idx.ty() != int {
            return Err(self.mismatch(i.span, &format!("`@{name}` index"), idx.ty(), int));
        }
        let idx = self.bind_operand(idx, span);
        let len = self.seq_len(&place, span)?;
        self.bounds_check(span, &idx, &len)?;
        let last = self.simd_tmp(span, Rvalue::binary(span, int, BinOp::Add, idx.clone(), Operand::int(span, int, 3)));
        self.bounds_check(span, &last, &len)?;
        let lane_place = self.b.index(self.cx, place, idx, span);
        let ref_ty = self.cx.ty_ref(lane_place.ty);
        let ptr = self.simd_tmp(span, Rvalue::ref_place(span, ref_ty, lane_place));
        Ok((vt, ptr))
    }
}
