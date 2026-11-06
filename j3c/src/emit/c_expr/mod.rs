//! Places, operands, rvalues, and statements as C.

use crate::emit::c_ty::{self, c_type};
use crate::ir::body::Body;
use crate::ir::cx::IrCx;
use crate::ir::entity::{FuncKind, Vis};
use crate::ir::ids::LocalId;
use crate::ir::op::{BinOp, UnaryOp};
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Place, PlaceRoot, Proj};
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};

mod aggregate;
mod decls;
mod half;
mod names;
mod simd;
mod stmt;

pub(crate) use decls::{emit_local_decls, proto};
pub(crate) use names::{func_c_name, global_c_name, stable_ty};
pub(super) use names::c_bytes_literal;
pub use half::{uses_half, PRELUDE as HALF_PRELUDE};
pub use simd::{uses_vectors, vec_c_name, PRELUDE as SIMD_PRELUDE};

struct DeferFrame {
    blocks: Vec<crate::ir::body::Block>,
    is_loop: bool,
    /// A C `switch` sits between here and the loop: `break` must `goto` the loop's end.
    is_switch: bool,
    /// Loop frames: the id in `j3_brk_<id>`.
    loop_id: u32,
}

pub(crate) struct CWriter<'a> {
    pub cx: &'a IrCx,
    pub body: Option<&'a Body>,
    pub tmp: u32,
    frames: Vec<DeferFrame>,
    /// Loops whose `j3_brk_<id>:` label a `goto` uses.
    brk_used: std::collections::HashSet<u32>,
    /// Loops whose `j3_cont_<id>:` label (end of the body) a `goto` uses.
    cont_used: std::collections::HashSet<u32>,
    next_loop: u32,
}

impl<'a> CWriter<'a> {
    pub fn new(cx: &'a IrCx) -> CWriter<'a> {
        CWriter {
            cx,
            body: None,
            tmp: 0,
            frames: Vec::new(),
            brk_used: std::collections::HashSet::new(),
            cont_used: std::collections::HashSet::new(),
            next_loop: 0,
        }
    }

    fn fresh(&mut self, prefix: &str) -> String {
        let n = self.tmp;
        self.tmp += 1;
        format!("{prefix}{n}")
    }

    pub fn local_name(&self, id: LocalId) -> String {
        match self.body {
            Some(body) if (id.0 as usize) < body.locals.len() => {
                let name = self.cx.name_str(body.local(id).name);
                format!("j3l_{}_{}", c_ty::sanitize(name), id.0)
            }
            _ => format!("j3l_{}", id.0),
        }
    }

    pub fn place(&mut self, place: &Place) -> Result<String, String> {
        let (mut text, mut ty) = match place.root {
            PlaceRoot::Local(id) => {
                let ty = match self.body {
                    Some(body) => body.local(id).ty,
                    None => return Err("place emit needs a function body".to_string()),
                };
                (self.local_name(id), ty)
            }
            PlaceRoot::Global(id) => {
                let g = self.cx.global(id);
                (global_c_name(self.cx, g), g.ty)
            }
        };

        for proj in &place.proj {
            match proj {
                Proj::Lane { lane, ty: lane_ty } => {
                    text = self.lane_place(&text, ty, *lane)?;
                    ty = *lane_ty;
                }
                Proj::Deref => {
                    text = format!("(*{text})");
                    ty = match self.cx.ref_inner(ty) {
                        Some(inner) => inner,
                        None => return Err("deref of non-ref in C emit".to_string()),
                    };
                }
                Proj::Field { field, .. } => {
                    let fname = c_ty::sanitize(self.cx.name_str(self.cx.field_def(*field).name));
                    text = format!("{text}.{fname}");
                    ty = self.cx.field_def(*field).ty;
                }
                Proj::Index { index, .. } => {
                    let idx = self.operand(index)?;
                    if c_ty::is_slice(self.cx, ty) {
                        text = format!("{text}.ptr[{idx}]");
                    } else {
                        text = format!("{text}[{idx}]");
                    }
                    ty = match self.cx.elem_ty(ty) {
                        Some(elem) => elem,
                        None => return Err("index of non-array/slice in C emit".to_string()),
                    };
                }
                Proj::Ptr => {
                    text = format!("{text}.ptr");
                    ty = match self.cx.elem_ty(ty) {
                        Some(elem) => match self.cx.lookup_ty(&crate::ir::ty::TyKind::Ref(elem)) {
                            Some(p) => p,
                            None => return Err("slice ptr type missing".to_string()),
                        },
                        None => return Err("ptr of non-slice in C emit".to_string()),
                    };
                }
                Proj::Len => {
                    text = format!("{text}.len");
                    ty = self.cx.common.int;
                }
                Proj::EnumTag => {
                    text = format!("{text}.tag");
                    ty = self.cx.common.i32;
                }
                Proj::EnumPayload { variant, ty: pty } => {
                    text = format!("{text}.u.v{}", variant.index);
                    ty = *pty;
                }
            }
        }
        let _ = ty;
        Ok(text)
    }

    pub fn operand(&mut self, operand: &Operand) -> Result<String, String> {
        match operand {
            Operand::Copy(place) => self.place(place),
            Operand::Const(c) => self.const_val(c.ty, &c.kind),
        }
    }

    fn const_val(&self, ty: Ty, kind: &ConstKind) -> Result<String, String> {
        match kind {
            ConstKind::Void => Ok("((void)0)".to_string()),
            ConstKind::Bool(true) => Ok("true".to_string()),
            ConstKind::Bool(false) => Ok("false".to_string()),
            ConstKind::Int(bits) => {
                if c_ty::is_int(self.cx, ty) {
                    match self.cx.ty_kind(ty) {
                        TyKind::Int(int_ty) if !int_ty.is_signed() => Ok(format!("{bits}ull")),
                        _ => Ok(format!("{}ll", *bits as i64)),
                    }
                } else {
                    Ok(format!("{}ll", *bits as i64))
                }
            }
            ConstKind::F32Bits(bits) => Ok(format!(
                "((union {{ uint32_t bits; float f; }}){{ .bits = {bits}u }}).f"
            )),
            ConstKind::F64Bits(bits) => Ok(format!(
                "((union {{ uint64_t bits; double f; }}){{ .bits = {bits}ull }}).f"
            )),
            ConstKind::FnAddr(id) => {
                let func = self.cx.func(*id);
                match &func.kind {
                    FuncKind::Lib { symbol, .. } => Ok(self.cx.name_str(*symbol).to_string()),
                    _ => Ok(func_c_name(self.cx, func)),
                }
            }
            ConstKind::Null => Ok("NULL".to_string()),
            ConstKind::Bytes(bytes) => {
                let ty_name = c_type(self.cx, ty)?;
                let lit = c_bytes_literal(bytes);
                Ok(format!(
                    "({ty_name}){{ .ptr = (uint8_t *){lit}, .len = {len}ll }}",
                    len = bytes.len()
                ))
            }
        }
    }

    pub fn rvalue(&mut self, rvalue: &Rvalue) -> Result<String, String> {
        match &rvalue.kind {
            RvalueKind::Simd { op, args } => self.simd(rvalue, *op, args),
            RvalueKind::Use(op) => self.operand(op),
            RvalueKind::Ref { place } => {
                let p = self.place(place)?;
                Ok(format!("&({p})"))
            }
            RvalueKind::Binary { op, lhs, rhs } => {
                let l = self.operand(lhs)?;
                let r = self.operand(rhs)?;
                if matches!(self.cx.ty_kind(rvalue.ty), TyKind::Vec(_)) {
                    self.vec_binop(rvalue.ty, *op, &l, &r)
                } else if c_ty::is_int(self.cx, rvalue.ty) && crate::ir::wrap::wrapping_bin(*op) {
                    wrap_int(self.cx, rvalue.ty, *op, &l, &r)
                } else if *op == BinOp::Rem {
                    match self.cx.ty_kind(rvalue.ty) {
                        TyKind::Float(FloatTy::F32) => Ok(format!("__builtin_fmodf({l}, {r})")),
                        TyKind::Float(FloatTy::F64) => Ok(format!("__builtin_fmod({l}, {r})")),
                        _ => Ok(format!("({l} % {r})")),
                    }
                } else {
                    Ok(format!("({l} {} {r})", op.as_str()))
                }
            }
            RvalueKind::Unary { op, arg } => {
                let a = self.operand(arg)?;
                if matches!(self.cx.ty_kind(rvalue.ty), TyKind::Vec(_)) {
                    self.vec_unary(rvalue.ty, *op, &a)
                } else if *op == UnaryOp::Neg && c_ty::is_int(self.cx, rvalue.ty) {
                    wrap_neg(self.cx, rvalue.ty, &a)
                } else if *op == UnaryOp::BitNot {
                    // `~` promotes u8/u16 to int; cast back so the value is width-correct
                    Ok(format!("(({})~{a})", c_type(self.cx, rvalue.ty)?))
                } else {
                    let prefix = match op {
                        UnaryOp::Neg => "-",
                        UnaryOp::Not => "!",
                        UnaryOp::BitNot => "~",
                    };
                    Ok(format!("({prefix}{a})"))
                }
            }
            RvalueKind::Call { func, args } => {
                let f = self.operand(func)?;
                let mut s = String::new();
                s.push_str(&f);
                s.push('(');
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        s.push_str(", ");
                    }
                    s.push_str(&self.operand(arg)?);
                }
                s.push(')');
                Ok(s)
            }
            RvalueKind::Syscall { nr, args } => {
                let mut s = String::from("j3_syscall(");
                s.push_str(&self.syscall_word(nr)?);
                let mut filled = Vec::new();
                for arg in args {
                    filled.push(self.syscall_word(arg)?);
                }
                while filled.len() < 6 {
                    filled.push("0ll".to_string());
                }
                for a in filled {
                    s.push_str(", ");
                    s.push_str(&a);
                }
                s.push(')');
                Ok(s)
            }
            RvalueKind::Atomic { op, order, args } => {
                let p = self.operand(&args[0])?;
                // The pointee type; a bare `ptr` is an `int` cell.
                let val_ty = self.cx.ref_inner(args[0].ty()).unwrap_or(self.cx.common.int);
                let t = match self.cx.ty_kind(val_ty) {
                    TyKind::Bool => "uint8_t".to_string(),
                    _ => c_type(self.cx, val_ty)?,
                };
                let p = format!("({t} *)(void *)({p})");
                let mut vals = Vec::new();
                for a in &args[1..] {
                    vals.push(self.operand(a)?);
                }
                let o = order.c11();
                use crate::ir::op::AtomicOp;
                Ok(match op {
                    AtomicOp::Load => format!("__atomic_load_n({p}, {o})"),
                    AtomicOp::Store => format!("(__atomic_store_n({p}, {}, {o}), (int64_t)0)", vals[0]),
                    AtomicOp::Add => format!("__atomic_fetch_add({p}, {}, {o})", vals[0]),
                    AtomicOp::Sub => format!("__atomic_fetch_sub({p}, {}, {o})", vals[0]),
                    AtomicOp::Xchg => format!("__atomic_exchange_n({p}, {}, {o})", vals[0]),
                    AtomicOp::Cas => format!(
                        "({{ {t} j3_exp = {}; (uint8_t)__atomic_compare_exchange_n({p}, &j3_exp, {}, 0, {o}, {}); }})",
                        vals[0],
                        vals[1],
                        order.cas_failure().c11()
                    ),
                })
            }
            RvalueKind::Aggregate { fields } => self.aggregate(rvalue.ty, fields),
            RvalueKind::Enum { variant, payload } => self.enum_val(*variant, payload),
            RvalueKind::Cast { from } => {
                let t = c_type(self.cx, rvalue.ty)?;
                let v = self.operand(from)?;
                if let Some(text) = half::cast(self.cx, from.ty(), rvalue.ty, &v) {
                    return Ok(text);
                }
                if byte_view(self.cx, from.ty()) && byte_view(self.cx, rvalue.ty) {
                    return Ok(format!("({t}){{ .ptr = ({v}).ptr, .len = ({v}).len }}"));
                }
                Ok(format!("({t})({v})"))
            }
            RvalueKind::Interpolate { .. } => {
                Err("interpolate must be lowered to InterpFill before C emit".to_string())
            }
        }
    }
}

fn wrap_int(cx: &IrCx, ty: Ty, op: BinOp, lhs: &str, rhs: &str) -> Result<String, String> {
    let c = c_type(cx, ty)?;
    let u = unsigned_c(cx, ty)?;
    match op {
        BinOp::Div | BinOp::Rem => {
            let sop = op.as_str();
            match cx.ty_kind(ty) {
                TyKind::Int(int_ty) if int_ty.is_signed() => {
                    let min = match int_ty.bits() {
                        8 => "INT8_MIN",
                        16 => "INT16_MIN",
                        32 => "INT32_MIN",
                        64 => "INT64_MIN",
                        _ => return Err("bad int width".to_string()),
                    };
                    // `MIN / -1` wraps to MIN; `MIN % -1` is 0.
                    let wrapped = if op == BinOp::Div { format!("({c}){min}") } else { format!("({c})0") };
                    Ok(format!(
                        "((({lhs}) == ({c}){min} && ({rhs}) == ({c})-1) ? {wrapped} : ({c})(({lhs}) {sop} ({rhs})))"
                    ))
                }
                _ => Ok(format!("(({c})(({u})({lhs}) {sop} ({u})({rhs})))")),
            }
        }
        BinOp::Shl | BinOp::Shr => {
            let (bits, signed) = match cx.ty_kind(ty) {
                TyKind::Int(i) => (i.bits(), i.is_signed()),
                _ => (64, false),
            };
            // signed `>>` keeps the sign (LLVM `ashr`); the amount is masked to the width
            let base = if op == BinOp::Shr && signed { &c } else { &u };
            Ok(format!(
                "(({c})(({base})({lhs}) {} (({u})({rhs}) & {})))",
                op.as_str(),
                bits - 1
            ))
        }
        _ => Ok(format!(
            "(({c})(({u})({lhs}) {} ({u})({rhs})))",
            op.as_str()
        )),
    }
}

fn wrap_neg(cx: &IrCx, ty: Ty, arg: &str) -> Result<String, String> {
    let c = c_type(cx, ty)?;
    let u = unsigned_c(cx, ty)?;
    Ok(format!("(({c})(({u})0 - ({u})({arg})))"))
}

fn unsigned_c(cx: &IrCx, ty: Ty) -> Result<String, String> {
    match cx.ty_kind(ty) {
        TyKind::Int(int_ty) => match int_ty.bits() {
            8 => Ok("uint8_t".to_string()),
            16 => Ok("uint16_t".to_string()),
            32 => Ok("uint32_t".to_string()),
            64 => Ok("uint64_t".to_string()),
            _ => Err("bad int width".to_string()),
        },
        _ => Err("unsigned_c on non-int".to_string()),
    }
}

/// `string` and `[]u8` are both `{ptr, len}` and may be cast either way.
fn byte_view(cx: &IrCx, ty: Ty) -> bool {
    match cx.ty_kind(ty) {
        TyKind::String => true,
        TyKind::Slice { elem } => *elem == cx.common.u8,
        _ => false,
    }
}
