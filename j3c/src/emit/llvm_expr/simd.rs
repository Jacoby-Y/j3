//! `vec4f` / `vec4i` in LLVM IR: `<4 x float>` / `<4 x i32>`.
//!
//! Every result is a pure function of the lanes, so opt levels agree bit for bit: no fast-math
//! flags, `@vmin`/`@vmax` are `fcmp` + `select` (not `llvm.minnum`, whose `-0.0` and NaN results
//! are target-defined), `@vsum` is the fixed tree `(x + y) + (z + w)` (not `vector.reduce.fadd`),
//! `@vint` saturates (`llvm.fptosi.sat`, no poison).

use super::*;
use crate::ir::simd::SimdOp;
use crate::ir::ty::VecTy;

impl<'a> LlvmFn<'a> {
    fn vec_of(&self, ty: Ty) -> Result<VecTy, String> {
        match self.cx.ty_kind(ty) {
            TyKind::Vec(v) => Ok(*v),
            _ => Err("vector op on a non-vector".to_string()),
        }
    }

    /// `<i32 n, i32 n, i32 n, i32 n>`.
    fn splat_i32(n: i32) -> String {
        format!("<i32 {n}, i32 {n}, i32 {n}, i32 {n}>")
    }

    /// `+ - * /` (float) and `+ - * & | ^ << >>` (int) lane-wise. Int `/` is lowered lane by lane in the checker.
    pub(super) fn vec_binop(&mut self, ty: Ty, op: BinOp, l: &str, r: &str) -> Result<String, String> {
        let v = self.vec_of(ty)?;
        let t = v.llvm();
        let out = self.fresh();
        let inst = match (v, op) {
            (VecTy::F32x4, BinOp::Add) => "fadd",
            (VecTy::F32x4, BinOp::Sub) => "fsub",
            (VecTy::F32x4, BinOp::Mul) => "fmul",
            (VecTy::F32x4, BinOp::Div) => "fdiv",
            (VecTy::I32x4, BinOp::Add) => "add",
            (VecTy::I32x4, BinOp::Sub) => "sub",
            (VecTy::I32x4, BinOp::Mul) => "mul",
            (VecTy::I32x4, BinOp::BitAnd) => "and",
            (VecTy::I32x4, BinOp::BitOr) => "or",
            (VecTy::I32x4, BinOp::BitXor) => "xor",
            (VecTy::I32x4, BinOp::Shl | BinOp::Shr) => {
                // a shift by 32 or more is poison in LLVM: mask the amount, like the scalar `i32` shift
                let m = self.fresh();
                self.inst(&format!("{m} = and {t} {r}, {}", Self::splat_i32(31)));
                let sh = if op == BinOp::Shl { "shl" } else { "ashr" };
                self.inst(&format!("{out} = {sh} {t} {l}, {m}"));
                return Ok(out);
            }
            _ => return Err(format!("`{}` on {}", op.as_str(), v.name())),
        };
        self.inst(&format!("{out} = {inst} {t} {l}, {r}"));
        Ok(out)
    }

    pub(super) fn vec_unary(&mut self, ty: Ty, op: UnaryOp, a: &str) -> Result<String, String> {
        let v = self.vec_of(ty)?;
        let t = v.llvm();
        let out = self.fresh();
        match (v, op) {
            (VecTy::F32x4, UnaryOp::Neg) => self.inst(&format!("{out} = fneg {t} {a}")),
            (VecTy::I32x4, UnaryOp::Neg) => self.inst(&format!("{out} = sub {t} zeroinitializer, {a}")),
            (VecTy::I32x4, UnaryOp::BitNot) => {
                self.inst(&format!("{out} = xor {t} {a}, {}", Self::splat_i32(-1)))
            }
            _ => return Err(format!("`{}` on {}", op.as_str(), v.name())),
        }
        Ok(out)
    }

    pub(super) fn simd(&mut self, rv: &Rvalue, op: SimdOp, args: &[Operand]) -> Result<(String, String), String> {
        let ll = ll_type(self.cx, rv.ty)?;
        match op {
            SimdOp::Splat => {
                let v = self.vec_of(rv.ty)?;
                let (x, _) = self.operand(&args[0])?;
                let a = self.fresh();
                self.inst(&format!("{a} = insertelement {} poison, {} {x}, i32 0", v.llvm(), v.lane_llvm()));
                let out = self.fresh();
                self.inst(&format!(
                    "{out} = shufflevector {t} {a}, {t} poison, <4 x i32> zeroinitializer",
                    t = v.llvm()
                ));
                Ok((out, ll))
            }
            SimdOp::Build => {
                let v = self.vec_of(rv.ty)?;
                let mut cur = "poison".to_string();
                for (i, arg) in args.iter().enumerate() {
                    let (x, _) = self.operand(arg)?;
                    let n = self.fresh();
                    self.inst(&format!("{n} = insertelement {} {cur}, {} {x}, i32 {i}", v.llvm(), v.lane_llvm()));
                    cur = n;
                }
                Ok((cur, ll))
            }
            SimdOp::Min | SimdOp::Max => {
                let v = self.vec_of(rv.ty)?;
                let t = v.llvm();
                let (a, _) = self.operand(&args[0])?;
                let (b, _) = self.operand(&args[1])?;
                let c = self.fresh();
                let (cmp, pred) = match (v, op) {
                    (VecTy::F32x4, SimdOp::Min) => ("fcmp", "olt"),
                    (VecTy::F32x4, _) => ("fcmp", "ogt"),
                    (VecTy::I32x4, SimdOp::Min) => ("icmp", "slt"),
                    (VecTy::I32x4, _) => ("icmp", "sgt"),
                };
                self.inst(&format!("{c} = {cmp} {pred} {t} {a}, {b}"));
                let out = self.fresh();
                self.inst(&format!("{out} = select <4 x i1> {c}, {t} {a}, {t} {b}"));
                Ok((out, ll))
            }
            SimdOp::Sqrt => {
                let (a, _) = self.operand(&args[0])?;
                let out = self.fresh();
                self.inst(&format!("{out} = call <4 x float> @llvm.sqrt.v4f32(<4 x float> {a})"));
                Ok((out, ll))
            }
            SimdOp::Abs => {
                let v = self.vec_of(rv.ty)?;
                let t = v.llvm();
                let (a, _) = self.operand(&args[0])?;
                let out = self.fresh();
                if v == VecTy::F32x4 {
                    self.inst(&format!("{out} = call <4 x float> @llvm.fabs.v4f32(<4 x float> {a})"));
                } else {
                    let neg = self.fresh();
                    let lt = self.fresh();
                    self.inst(&format!("{neg} = sub {t} zeroinitializer, {a}"));
                    self.inst(&format!("{lt} = icmp slt {t} {a}, zeroinitializer"));
                    self.inst(&format!("{out} = select <4 x i1> {lt}, {t} {neg}, {t} {a}"));
                }
                Ok((out, ll))
            }
            SimdOp::Sum => {
                let v = self.vec_of(args[0].ty())?;
                let t = v.llvm();
                let add = if v == VecTy::F32x4 { "fadd" } else { "add" };
                let (a, _) = self.operand(&args[0])?;
                let s1 = self.fresh();
                self.inst(&format!("{s1} = shufflevector {t} {a}, {t} poison, <4 x i32> <i32 1, i32 0, i32 3, i32 2>"));
                let t1 = self.fresh();
                self.inst(&format!("{t1} = {add} {t} {a}, {s1}"));
                let s2 = self.fresh();
                self.inst(&format!("{s2} = shufflevector {t} {t1}, {t} poison, <4 x i32> <i32 2, i32 3, i32 0, i32 1>"));
                let t2 = self.fresh();
                self.inst(&format!("{t2} = {add} {t} {t1}, {s2}"));
                let out = self.fresh();
                self.inst(&format!("{out} = extractelement {t} {t2}, i32 0"));
                Ok((out, v.lane_llvm().to_string()))
            }
            SimdOp::Shuffle(ix) => {
                let v = self.vec_of(rv.ty)?;
                let t = v.llvm();
                let (a, _) = self.operand(&args[0])?;
                let out = self.fresh();
                self.inst(&format!(
                    "{out} = shufflevector {t} {a}, {t} poison, <4 x i32> <i32 {}, i32 {}, i32 {}, i32 {}>",
                    ix[0], ix[1], ix[2], ix[3]
                ));
                Ok((out, ll))
            }
            SimdOp::ToFloat => {
                let (a, _) = self.operand(&args[0])?;
                let out = self.fresh();
                self.inst(&format!("{out} = sitofp <4 x i32> {a} to <4 x float>"));
                Ok((out, ll))
            }
            SimdOp::ToInt => {
                let (a, _) = self.operand(&args[0])?;
                let out = self.fresh();
                self.inst(&format!(
                    "{out} = call <4 x i32> @llvm.fptosi.sat.v4i32.v4f32(<4 x float> {a})"
                ));
                Ok((out, ll))
            }
            SimdOp::Load => {
                let (p, _) = self.operand(&args[0])?;
                let out = self.fresh();
                self.inst(&format!("{out} = load {ll}, ptr {p}, align 4"));
                Ok((out, ll))
            }
            SimdOp::Store => {
                let (p, _) = self.operand(&args[0])?;
                let (x, xt) = self.operand(&args[1])?;
                self.inst(&format!("store {xt} {x}, ptr {p}, align 4"));
                Ok(("undef".to_string(), "void".to_string()))
            }
        }
    }
}
