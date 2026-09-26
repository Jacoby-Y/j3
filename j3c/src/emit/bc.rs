//! Versioned bytecode printer for checked script IR (`.j3sb`).
//!
//! A register machine. Register 0 holds the frame pointer (the function's local byte area);
//! every other register is an 8-byte slot. Scalar locals that are never addressed live in
//! registers; everything else lives in the frame at an `abi.rs` offset, so instance fields,
//! slices, and structs have the same layout the native program uses. Ops are typed here, so
//! the VM (`stdlib/j3/script.j3`) never looks at a type at runtime. Integer registers hold
//! the value sign- or zero-extended to 64 bits; narrow results are re-extended after math.

use std::collections::HashMap;

use crate::ir::abi;
use crate::ir::body::{Block, Body};
use crate::ir::cx::IrCx;
use crate::ir::dump;
use crate::ir::entity::{ConstInit, Func, FuncKind, Vis};
use crate::ir::ids::{FieldId, FuncId, GlobalId};
use crate::ir::module::Module;
use crate::ir::op::{BinOp, UnaryOp};
use crate::ir::operand::{ConstKind, Operand};
use crate::ir::place::{Place, PlaceRoot, Proj};
use crate::ir::rvalue::{InterpPart, Rvalue, RvalueKind};
use crate::ir::stmt::{Stmt, StmtKind};
use crate::ir::ty::{FloatTy, IntTy, Ty, TyKind};

pub const VERSION: u16 = 2;
pub const NO_REG: u32 = u32::MAX;
/// Register 0 is the frame pointer.
const FP: u32 = 0;

/// Opcodes. The numbers are the file format; `stdlib/j3/script.j3` has the same table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Op {
    Const = 1,
    Mov = 2,
    Addr = 3,
    Idx = 4,
    Global = 5,
    Str = 6,
    Ld8s = 7,
    Ld8u = 8,
    Ld16s = 9,
    Ld16u = 10,
    Ld32s = 11,
    Ld32u = 12,
    Ld64 = 13,
    St8 = 14,
    St16 = 15,
    St32 = 16,
    St64 = 17,
    Copy = 18,
    Add = 19,
    Sub = 20,
    Mul = 21,
    Divs = 22,
    Rems = 23,
    Divu = 24,
    Remu = 25,
    Neg = 26,
    Not = 27,
    Eq = 28,
    Ne = 29,
    Lts = 30,
    Les = 31,
    Ltu = 32,
    Leu = 33,
    Sext8 = 34,
    Sext16 = 35,
    Sext32 = 36,
    Zext8 = 37,
    Zext16 = 38,
    Zext32 = 39,
    Fadd = 40,
    Fsub = 41,
    Fmul = 42,
    Fdiv = 43,
    Fneg = 44,
    Feq = 45,
    Fne = 46,
    Flt = 47,
    Fle = 48,
    Dadd = 49,
    Dsub = 50,
    Dmul = 51,
    Ddiv = 52,
    Dneg = 53,
    Deq = 54,
    Dne = 55,
    Dlt = 56,
    Dle = 57,
    I2f = 58,
    U2f = 59,
    F2i = 60,
    F2u = 61,
    I2d = 62,
    U2d = 63,
    D2i = 64,
    D2u = 65,
    F2d = 66,
    D2f = 67,
    Jmp = 68,
    Jz = 69,
    Jnz = 70,
    /// Jump when `!(a < b)`, signed. The rest of the `Jn*` family is the same shape.
    JnLts = 71,
    JnLes = 72,
    JnLtu = 73,
    JnLeu = 74,
    JnEq = 75,
    JnNe = 76,
    JnFlt = 77,
    JnFle = 78,
    JnDlt = 79,
    JnDle = 80,
    Call = 81,
    Ret = 82,
    RetV = 83,
    RetA = 84,
    Abort = 85,
    Div0 = 86,
    AppLit = 87,
    AppStr = 88,
    AppInt = 89,
    AppF64 = 90,
    AppBool = 91,
    Land = 92,
    Lor = 93,
}

impl Op {
    fn name(self) -> &'static str {
        match self {
            Op::Const => "const",
            Op::Mov => "mov",
            Op::Addr => "addr",
            Op::Idx => "idx",
            Op::Global => "global",
            Op::Str => "str",
            Op::Ld8s => "ld8s",
            Op::Ld8u => "ld8u",
            Op::Ld16s => "ld16s",
            Op::Ld16u => "ld16u",
            Op::Ld32s => "ld32s",
            Op::Ld32u => "ld32u",
            Op::Ld64 => "ld64",
            Op::St8 => "st8",
            Op::St16 => "st16",
            Op::St32 => "st32",
            Op::St64 => "st64",
            Op::Copy => "copy",
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Mul => "mul",
            Op::Divs => "divs",
            Op::Rems => "rems",
            Op::Divu => "divu",
            Op::Remu => "remu",
            Op::Neg => "neg",
            Op::Not => "not",
            Op::Eq => "eq",
            Op::Ne => "ne",
            Op::Lts => "lts",
            Op::Les => "les",
            Op::Ltu => "ltu",
            Op::Leu => "leu",
            Op::Sext8 => "sext8",
            Op::Sext16 => "sext16",
            Op::Sext32 => "sext32",
            Op::Zext8 => "zext8",
            Op::Zext16 => "zext16",
            Op::Zext32 => "zext32",
            Op::Fadd => "fadd",
            Op::Fsub => "fsub",
            Op::Fmul => "fmul",
            Op::Fdiv => "fdiv",
            Op::Fneg => "fneg",
            Op::Feq => "feq",
            Op::Fne => "fne",
            Op::Flt => "flt",
            Op::Fle => "fle",
            Op::Dadd => "dadd",
            Op::Dsub => "dsub",
            Op::Dmul => "dmul",
            Op::Ddiv => "ddiv",
            Op::Dneg => "dneg",
            Op::Deq => "deq",
            Op::Dne => "dne",
            Op::Dlt => "dlt",
            Op::Dle => "dle",
            Op::I2f => "i2f",
            Op::U2f => "u2f",
            Op::F2i => "f2i",
            Op::F2u => "f2u",
            Op::I2d => "i2d",
            Op::U2d => "u2d",
            Op::D2i => "d2i",
            Op::D2u => "d2u",
            Op::F2d => "f2d",
            Op::D2f => "d2f",
            Op::Jmp => "jmp",
            Op::Jz => "jz",
            Op::Jnz => "jnz",
            Op::JnLts => "jn.lts",
            Op::JnLes => "jn.les",
            Op::JnLtu => "jn.ltu",
            Op::JnLeu => "jn.leu",
            Op::JnEq => "jn.eq",
            Op::JnNe => "jn.ne",
            Op::JnFlt => "jn.flt",
            Op::JnFle => "jn.fle",
            Op::JnDlt => "jn.dlt",
            Op::JnDle => "jn.dle",
            Op::Call => "call",
            Op::Ret => "ret",
            Op::RetV => "retv",
            Op::RetA => "reta",
            Op::Abort => "abort",
            Op::Div0 => "div0",
            Op::AppLit => "app.lit",
            Op::AppStr => "app.str",
            Op::AppInt => "app.int",
            Op::AppF64 => "app.f64",
            Op::AppBool => "app.bool",
            Op::Land => "land",
            Op::Lor => "lor",
        }
    }

    fn is_jump(self) -> bool {
        matches!(
            self,
            Op::Jmp
                | Op::Jz
                | Op::Jnz
                | Op::JnLts
                | Op::JnLes
                | Op::JnLtu
                | Op::JnLeu
                | Op::JnEq
                | Op::JnNe
                | Op::JnFlt
                | Op::JnFle
                | Op::JnDlt
                | Op::JnDle
        )
    }
}

/// One instruction. Encoded as 24 bytes in the same layout as the VM's `Instr` struct.
#[derive(Clone, Debug)]
pub struct Instr {
    pub op: Op,
    pub aux: u16,
    pub dst: u32,
    pub a: u32,
    pub b: u32,
    pub imm: u64,
}

impl Instr {
    fn new(op: Op, dst: u32, a: u32, b: u32, imm: u64) -> Instr {
        Instr {
            op,
            aux: 0,
            dst,
            a,
            b,
            imm,
        }
    }
}

/// How a call site reaches its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// A function in this module, by index.
    Local = 0,
    /// A function in another script, by key.
    Script = 1,
    /// A native function, through the program's thunk table, by key.
    Native = 2,
}

#[derive(Clone, Debug)]
pub struct CallArg {
    pub reg: u32,
    pub size: u32,
    /// The register holds the address of an aggregate, not the value.
    pub aggregate: bool,
}

#[derive(Clone, Debug)]
pub struct CallSite {
    pub kind: CallKind,
    pub key: u64,
    pub index: u32,
    pub ret_size: u32,
    /// Load op that widens a native scalar result into a register (0 for void/aggregate).
    pub ret_load: u16,
    pub ret_aggregate: bool,
    pub args: Vec<CallArg>,
}

#[derive(Clone, Debug)]
pub struct BcParam {
    /// Register the argument lands in, or `NO_REG` when the param lives in the frame.
    pub reg: u32,
    pub offset: u32,
    pub size: u32,
    /// Load op for a scalar param read from memory (0 for an aggregate).
    pub load: u16,
}

#[derive(Clone, Debug)]
pub struct BcFunction {
    pub name: String,
    pub key: u64,
    pub signature: String,
    pub public: bool,
    pub frame_size: u32,
    pub regs: u32,
    pub params: Vec<BcParam>,
    pub ret_size: u32,
    pub ret_aggregate: bool,
    pub calls: Vec<CallSite>,
    pub code: Vec<Instr>,
}

#[derive(Clone, Debug)]
pub struct BcField {
    pub name: String,
    pub offset: u32,
    pub size: u32,
    pub default: Vec<u8>,
    pub default_const: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct BcGlobal {
    pub key: u64,
    pub name: String,
    pub size: u32,
    pub init: Vec<u8>,
    pub init_const: Option<u32>,
}

#[derive(Clone, Debug)]
pub struct BcModule {
    pub name: String,
    /// Path from the project root, e.g. `scripts/door.j3s`.
    pub path: String,
    pub layout_hash: u64,
    pub signature_hash: u64,
    pub instance_size: u32,
    pub props: u32,
    pub fields: Vec<BcField>,
    pub globals: Vec<BcGlobal>,
    pub constants: Vec<Vec<u8>>,
    pub natives: Vec<(u64, String)>,
    /// Other scripts this one calls into; the VM loads them first.
    pub scripts: Vec<String>,
    pub functions: Vec<BcFunction>,
}

/// The project-relative path of a script module (`script.a.b` → `a/b.j3s`).
pub fn script_path(cx: &IrCx, module: &[crate::intern::Name]) -> String {
    let parts: Vec<&str> = module.iter().skip(1).map(|n| cx.name_str(*n)).collect();
    format!("{}.j3s", parts.join("/"))
}

pub fn is_script_module(cx: &IrCx, module: &[crate::intern::Name]) -> bool {
    module.first().map(|n| cx.name_str(*n)) == Some("script")
}

pub fn emit_script(cx: &IrCx, module: &Module) -> Result<BcModule, String> {
    let Some(&sid) = module
        .structs
        .iter()
        .find(|id| cx.struct_def(**id).script.is_some())
    else {
        return Err("module is not a script".to_string());
    };
    let def = cx.struct_def(sid);
    let meta = def.script.as_ref().expect("script metadata");
    let struct_ty = cx
        .lookup_ty(&TyKind::Struct(sid))
        .ok_or_else(|| "script type not interned".to_string())?;
    let name = module
        .name
        .iter()
        .map(|n| cx.name_str(*n))
        .collect::<Vec<_>>()
        .join(".");
    let mut constants = Vec::new();
    let mut fields = Vec::new();
    let mut layout_bytes = Vec::new();
    for (i, field) in def.fields.iter().enumerate() {
        let offset = abi::field_offset(cx, struct_ty, i)
            .ok_or_else(|| "script field has no ABI offset".to_string())? as u32;
        let size = abi::size(cx, field.ty) as u32;
        let (default, default_const) = field
            .default
            .as_ref()
            .and_then(|v| const_bytes(cx, v, size, &mut constants))
            .unwrap_or_default();
        let fname = cx.name_str(field.name).to_string();
        layout_bytes.extend(fname.as_bytes());
        layout_bytes.push(0);
        layout_bytes.extend(dump::ty_string(cx, field.ty).as_bytes());
        layout_bytes.extend(offset.to_le_bytes());
        fields.push(BcField {
            name: fname,
            offset,
            size,
            default,
            default_const,
        });
    }
    let instance_size = abi::size(cx, struct_ty) as u32;
    layout_bytes.extend((meta.prop_spans.len() as u32).to_le_bytes());
    layout_bytes.extend(instance_size.to_le_bytes());
    let mut globals = Vec::new();
    for id in &module.globals {
        let g = cx.global(*id);
        let size = abi::size(cx, g.ty) as u32;
        let (init, init_const) = match &g.init {
            Some(ConstInit::Scalar(v)) => {
                const_bytes(cx, v, size, &mut constants).unwrap_or_default()
            }
            _ => (Vec::new(), None),
        };
        let key = global_key(cx, *id);
        layout_bytes.extend(key.to_le_bytes());
        layout_bytes.extend(size.to_le_bytes());
        globals.push(BcGlobal {
            key,
            name: cx.name_str(g.name).to_string(),
            size,
            init,
            init_const,
        });
    }
    let layout_hash = hash(&layout_bytes);
    let local_index: HashMap<FuncId, u32> = module
        .funcs
        .iter()
        .filter(|id| matches!(cx.func(**id).kind, FuncKind::J3 { .. }))
        .enumerate()
        .map(|(i, id)| (*id, i as u32))
        .collect();
    let mut shared = Shared {
        constants,
        natives: Vec::new(),
        scripts: Vec::new(),
        globals: module.globals.clone(),
        local_index,
    };
    let mut functions = Vec::new();
    for id in &module.funcs {
        let func = cx.func(*id);
        let FuncKind::J3 { body } = &func.kind else {
            continue;
        };
        let emitted = Codegen::new(cx, func, body, &mut shared)
            .emit()
            .map_err(|e| format!("{}: fn {}: {e}", meta.path, cx.name_str(func.name)))?;
        functions.push(emitted);
    }
    let signatures = functions
        .iter()
        .filter(|f| f.public)
        .map(|f| format!("{}:{}", f.name, f.signature))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(BcModule {
        name,
        path: script_path(cx, &module.name),
        layout_hash,
        signature_hash: hash(signatures.as_bytes()),
        instance_size,
        props: meta.prop_spans.len() as u32,
        fields,
        globals,
        constants: shared.constants,
        natives: shared.natives,
        scripts: shared.scripts,
        functions,
    })
}

fn hash(bytes: &[u8]) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in bytes {
        h = (h ^ (*b as u64)).wrapping_mul(0x100000001b3);
    }
    h
}

fn module_str(cx: &IrCx, module: &[crate::intern::Name]) -> String {
    module
        .iter()
        .map(|n| cx.name_str(*n))
        .collect::<Vec<_>>()
        .join(".")
}

/// Stable name of a function across rebuilds: module, name, and signature.
pub fn fn_label(cx: &IrCx, id: FuncId) -> String {
    let f = cx.func(id);
    format!(
        "{}.{}:{}",
        module_str(cx, &f.module),
        cx.name_str(f.name),
        dump::ty_string(cx, f.ty)
    )
}

/// The key the VM and the dev thunk table use for a function.
pub fn fn_key(cx: &IrCx, id: FuncId) -> u64 {
    hash(fn_label(cx, id).as_bytes())
}

fn global_key(cx: &IrCx, id: GlobalId) -> u64 {
    let g = cx.global(id);
    hash(
        format!(
            "{}.{}:{}",
            module_str(cx, &g.module),
            cx.name_str(g.name),
            dump::ty_string(cx, g.ty)
        )
        .as_bytes(),
    )
}

/// Scalar register class of a type, or `None` for an aggregate that lives in memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Void,
    Bool,
    Int { bits: u8, signed: bool },
    F32,
    F64,
    /// Pointers, refs, cstr, fn addresses: 64-bit unsigned.
    Ptr,
}

fn class(cx: &IrCx, ty: Ty) -> Option<Class> {
    Some(match cx.ty_kind(ty) {
        TyKind::Void => Class::Void,
        TyKind::Bool => Class::Bool,
        TyKind::Int(i) => {
            let (bits, signed) = match i {
                IntTy::I8 => (8, true),
                IntTy::U8 => (8, false),
                IntTy::I16 => (16, true),
                IntTy::U16 => (16, false),
                IntTy::I32 => (32, true),
                IntTy::U32 => (32, false),
                IntTy::I64 => (64, true),
                IntTy::U64 => (64, false),
            };
            Class::Int { bits, signed }
        }
        TyKind::Float(FloatTy::F32) => Class::F32,
        TyKind::Float(FloatTy::F64) => Class::F64,
        TyKind::Ptr | TyKind::CStr | TyKind::Ref(_) | TyKind::Fn { .. } => Class::Ptr,
        _ => return None,
    })
}

fn is_scalar(cx: &IrCx, ty: Ty) -> bool {
    matches!(class(cx, ty), Some(c) if c != Class::Void)
}

/// The load op that reads a value of this class from memory into a canonical register.
fn load_op(c: Class) -> Op {
    match c {
        Class::Bool => Op::Ld8u,
        Class::Int { bits: 8, signed: true } => Op::Ld8s,
        Class::Int { bits: 8, .. } => Op::Ld8u,
        Class::Int { bits: 16, signed: true } => Op::Ld16s,
        Class::Int { bits: 16, .. } => Op::Ld16u,
        Class::Int { bits: 32, signed: true } => Op::Ld32s,
        Class::Int { bits: 32, .. } => Op::Ld32u,
        Class::F32 => Op::Ld32u,
        _ => Op::Ld64,
    }
}

fn store_op(size: u64) -> Op {
    match size {
        1 => Op::St8,
        2 => Op::St16,
        4 => Op::St32,
        _ => Op::St64,
    }
}

/// The op that re-extends a narrow integer result, if any.
fn ext_op(c: Class) -> Option<Op> {
    match c {
        Class::Int { bits: 8, signed: true } => Some(Op::Sext8),
        Class::Int { bits: 8, .. } => Some(Op::Zext8),
        Class::Int { bits: 16, signed: true } => Some(Op::Sext16),
        Class::Int { bits: 16, .. } => Some(Op::Zext16),
        Class::Int { bits: 32, signed: true } => Some(Op::Sext32),
        Class::Int { bits: 32, .. } => Some(Op::Zext32),
        _ => None,
    }
}

/// A constant's register bits in canonical form.
fn canonical(c: Class, bits: u64) -> u64 {
    match c {
        Class::Bool => u64::from(bits != 0),
        Class::Int { bits: 64, .. } => bits,
        Class::Int { bits: w, signed } => {
            let shift = 64 - w as u32;
            if signed {
                (((bits << shift) as i64) >> shift) as u64
            } else {
                (bits << shift) >> shift
            }
        }
        Class::F32 => bits & 0xffff_ffff,
        _ => bits,
    }
}

fn const_bytes(
    cx: &IrCx,
    op: &Operand,
    size: u32,
    constants: &mut Vec<Vec<u8>>,
) -> Option<(Vec<u8>, Option<u32>)> {
    let Operand::Const(c) = op else { return None };
    if let ConstKind::Bytes(bytes) = &c.kind {
        if size != 16 {
            return None;
        }
        let index = constants.len() as u32;
        constants.push(bytes.clone());
        let mut image = vec![0u8; 16];
        image[8..16].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
        return Some((image, Some(index)));
    }
    let raw = match &c.kind {
        ConstKind::Bool(v) => u64::from(*v),
        ConstKind::Int(v) => *v,
        ConstKind::F32Bits(v) => *v as u64,
        ConstKind::F64Bits(v) => *v,
        ConstKind::Null => 0,
        _ => return None,
    };
    let _ = cx;
    Some((raw.to_le_bytes()[..(size as usize).min(8)].to_vec(), None))
}

impl BcModule {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(*b"J3SB");
        u16_out(&mut out, VERSION);
        u16_out(&mut out, 0);
        u64_out(&mut out, self.layout_hash);
        u64_out(&mut out, self.signature_hash);
        string_out(&mut out, &self.name);
        string_out(&mut out, &self.path);
        u32_out(&mut out, self.instance_size);
        u32_out(&mut out, self.props);
        u32_out(&mut out, self.fields.len() as u32);
        for f in &self.fields {
            string_out(&mut out, &f.name);
            u32_out(&mut out, f.offset);
            u32_out(&mut out, f.size);
            bytes_out(&mut out, &f.default);
            u32_out(&mut out, f.default_const.unwrap_or(u32::MAX));
        }
        u32_out(&mut out, self.globals.len() as u32);
        for g in &self.globals {
            u64_out(&mut out, g.key);
            string_out(&mut out, &g.name);
            u32_out(&mut out, g.size);
            bytes_out(&mut out, &g.init);
            u32_out(&mut out, g.init_const.unwrap_or(u32::MAX));
        }
        u32_out(&mut out, self.constants.len() as u32);
        for c in &self.constants {
            bytes_out(&mut out, c);
        }
        u32_out(&mut out, self.natives.len() as u32);
        for (key, name) in &self.natives {
            u64_out(&mut out, *key);
            string_out(&mut out, name);
        }
        u32_out(&mut out, self.scripts.len() as u32);
        for path in &self.scripts {
            string_out(&mut out, path);
        }
        u32_out(&mut out, self.functions.len() as u32);
        for f in &self.functions {
            u64_out(&mut out, f.key);
            string_out(&mut out, &f.name);
            string_out(&mut out, &f.signature);
            u32_out(&mut out, u32::from(f.public));
            u32_out(&mut out, f.frame_size);
            u32_out(&mut out, f.regs);
            u32_out(&mut out, f.ret_size);
            u32_out(&mut out, u32::from(f.ret_aggregate));
            u32_out(&mut out, f.params.len() as u32);
            for p in &f.params {
                u32_out(&mut out, p.reg);
                u32_out(&mut out, p.offset);
                u32_out(&mut out, p.size);
                u32_out(&mut out, p.load as u32);
            }
            u32_out(&mut out, f.calls.len() as u32);
            for call in &f.calls {
                u32_out(&mut out, call.kind as u32);
                u64_out(&mut out, call.key);
                u32_out(&mut out, call.index);
                u32_out(&mut out, call.ret_size);
                u32_out(&mut out, call.ret_load as u32);
                u32_out(&mut out, u32::from(call.ret_aggregate));
                u32_out(&mut out, call.args.len() as u32);
                for arg in &call.args {
                    u32_out(&mut out, arg.reg);
                    u32_out(&mut out, arg.size);
                    u32_out(&mut out, u32::from(arg.aggregate));
                }
            }
            u32_out(&mut out, f.code.len() as u32);
            // Pad so the instruction array starts 8-aligned in the file.
            while out.len() % 8 != 0 {
                out.push(0);
            }
            for ins in &f.code {
                u16_out(&mut out, ins.op as u16);
                u16_out(&mut out, ins.aux);
                u32_out(&mut out, ins.dst);
                u32_out(&mut out, ins.a);
                u32_out(&mut out, ins.b);
                u64_out(&mut out, ins.imm);
            }
        }
        out
    }

    pub fn dump(&self) -> String {
        let mut out = format!(
            "j3sb {VERSION} {} ({})\nlayout {:016x} signatures {:016x}\ninstance {} bytes, {} props\n",
            self.name,
            self.path,
            self.layout_hash,
            self.signature_hash,
            self.instance_size,
            self.props
        );
        for f in &self.fields {
            out.push_str(&format!(
                "  field {} @{} size {}{}\n",
                f.name,
                f.offset,
                f.size,
                if f.default.is_empty() { "" } else { " default" }
            ));
        }
        for g in &self.globals {
            out.push_str(&format!("  shared {} size {}\n", g.name, g.size));
        }
        for (_, name) in &self.natives {
            out.push_str(&format!("  native {name}\n"));
        }
        for path in &self.scripts {
            out.push_str(&format!("  script {path}\n"));
        }
        for f in &self.functions {
            out.push_str(&format!(
                "\n{}fn {} [{}] frame {} regs {}\n",
                if f.public { "pub " } else { "" },
                f.name,
                f.signature,
                f.frame_size,
                f.regs
            ));
            for (pc, ins) in f.code.iter().enumerate() {
                out.push_str(&format!("  {pc:04} {}", dump_instr(ins, f)));
                out.push('\n');
            }
        }
        out
    }

    /// Write `target/j3s/<path>.j3sb` under `root`. Skips the write when the bytes match.
    pub fn write_to(&self, root: &std::path::Path) -> Result<std::path::PathBuf, String> {
        let path = bytecode_path(root, &self.path);
        let bytes = self.encode();
        if std::fs::read(&path).map(|old| old == bytes).unwrap_or(false) {
            return Ok(path);
        }
        std::fs::create_dir_all(path.parent().expect("bytecode parent"))
            .map_err(|e| format!("create {}: {e}", path.display()))?;
        std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
        Ok(path)
    }
}

/// Where a script's bytecode lives under a project root.
pub fn bytecode_path(root: &std::path::Path, script: &str) -> std::path::PathBuf {
    root.join("target/j3s")
        .join(script)
        .with_extension("j3sb")
}

fn reg_name(r: u32) -> String {
    if r == NO_REG {
        "_".to_string()
    } else if r == FP {
        "fp".to_string()
    } else {
        format!("r{r}")
    }
}

fn dump_instr(ins: &Instr, f: &BcFunction) -> String {
    let name = ins.op.name();
    let d = reg_name(ins.dst);
    let a = reg_name(ins.a);
    let b = reg_name(ins.b);
    match ins.op {
        Op::Const => format!("{name} {d}, {:#x}", ins.imm),
        Op::Global => format!("{name} {d}, shared#{}", ins.imm),
        Op::Str => format!("{name} [{a}], const#{}", ins.imm),
        Op::Addr => format!("{name} {d}, {a}+{}", ins.imm),
        Op::Idx => format!("{name} {d}, {a}+{b}*{}", ins.imm),
        Op::Ld8s | Op::Ld8u | Op::Ld16s | Op::Ld16u | Op::Ld32s | Op::Ld32u | Op::Ld64 => {
            format!("{name} {d}, [{a}+{}]", ins.imm)
        }
        Op::St8 | Op::St16 | Op::St32 | Op::St64 => format!("{name} [{a}+{}], {b}", ins.imm),
        Op::Copy => format!("{name} [{a}], [{b}], {}", ins.imm),
        Op::Jmp => format!("{name} {:04}", ins.imm),
        Op::Jz | Op::Jnz => format!("{name} {a}, {:04}", ins.imm),
        op if op.is_jump() => format!("{name} {a}, {b}, {:04}", ins.imm),
        Op::Call => {
            let site = &f.calls[ins.imm as usize];
            let target = match site.kind {
                CallKind::Local => format!("local#{}", site.index),
                CallKind::Script => format!("script {:016x}", site.key),
                CallKind::Native => format!("native {:016x}", site.key),
            };
            let args = site
                .args
                .iter()
                .map(|a| {
                    if a.aggregate {
                        format!("[{}]", reg_name(a.reg))
                    } else {
                        reg_name(a.reg)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {d} = {target}({args})")
        }
        Op::Ret | Op::Abort | Op::Div0 => name.to_string(),
        Op::RetV => format!("{name} {a}"),
        Op::RetA => format!("{name} [{a}], {}", ins.imm),
        Op::AppLit => format!("{name} [{d}], [{a}], const#{}", ins.imm),
        Op::AppStr | Op::AppInt | Op::AppF64 | Op::AppBool => format!("{name} [{d}], [{a}], {b}"),
        Op::Mov
        | Op::Neg
        | Op::Not
        | Op::Fneg
        | Op::Dneg
        | Op::Sext8
        | Op::Sext16
        | Op::Sext32
        | Op::Zext8
        | Op::Zext16
        | Op::Zext32
        | Op::I2f
        | Op::U2f
        | Op::F2i
        | Op::F2u
        | Op::I2d
        | Op::U2d
        | Op::D2i
        | Op::D2u
        | Op::F2d
        | Op::D2f => format!("{name} {d}, {a}"),
        _ => format!("{name} {d}, {a}, {b}"),
    }
}

fn u16_out(out: &mut Vec<u8>, n: u16) {
    out.extend(n.to_le_bytes());
}
fn u32_out(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
fn u64_out(out: &mut Vec<u8>, n: u64) {
    out.extend(n.to_le_bytes());
}
fn bytes_out(out: &mut Vec<u8>, bytes: &[u8]) {
    u32_out(out, bytes.len() as u32);
    out.extend(bytes);
}
fn string_out(out: &mut Vec<u8>, s: &str) {
    bytes_out(out, s.as_bytes());
}

/// Tables every function in a module appends to.
struct Shared {
    constants: Vec<Vec<u8>>,
    natives: Vec<(u64, String)>,
    scripts: Vec<String>,
    globals: Vec<GlobalId>,
    local_index: HashMap<FuncId, u32>,
}

struct LoopFixups {
    breaks: Vec<usize>,
    continues: Vec<usize>,
    scope_depth: usize,
}

/// Where a place's bytes start: a base register plus a constant offset.
#[derive(Clone, Copy)]
struct Addr {
    base: u32,
    offset: u64,
}

struct Codegen<'a> {
    cx: &'a IrCx,
    func: &'a Func,
    body: &'a Body,
    shared: &'a mut Shared,
    /// Frame offset of each local that lives in memory.
    mem: Vec<Option<u32>>,
    /// Register of each scalar local that never has its address taken.
    value: Vec<Option<u32>>,
    uses: Vec<u32>,
    frame_size: u32,
    regs: u32,
    prologue: Vec<Instr>,
    const_regs: HashMap<u64, u32>,
    code: Vec<Instr>,
    calls: Vec<CallSite>,
    loops: Vec<LoopFixups>,
    scopes: Vec<Vec<Block>>,
    /// The register the last compare wrote, so a branch on it can fuse.
    last_compare: Option<(usize, u32)>,
}

impl<'a> Codegen<'a> {
    fn new(cx: &'a IrCx, func: &'a Func, body: &'a Body, shared: &'a mut Shared) -> Codegen<'a> {
        let addressed = addressed_locals(&body.block, body.locals.len());
        let mut uses = vec![0u32; body.locals.len()];
        count_block(&body.block, &mut uses);
        let mut mem = Vec::new();
        let mut value = Vec::new();
        let mut frame_size = 0u32;
        let mut regs = 1u32;
        for (i, local) in body.locals.iter().enumerate() {
            if is_scalar(cx, local.ty) && !addressed[i] {
                value.push(Some(regs));
                regs += 1;
                mem.push(None);
            } else {
                let align = (abi::align(cx, local.ty) as u32).max(8);
                frame_size = align_up(frame_size, align);
                mem.push(Some(frame_size));
                frame_size += (abi::size(cx, local.ty) as u32).max(8);
                value.push(None);
            }
        }
        Codegen {
            cx,
            func,
            body,
            shared,
            mem,
            value,
            uses,
            frame_size,
            regs,
            prologue: Vec::new(),
            const_regs: HashMap::new(),
            code: Vec::new(),
            calls: Vec::new(),
            loops: Vec::new(),
            scopes: Vec::new(),
            last_compare: None,
        }
    }

    fn emit(mut self) -> Result<BcFunction, String> {
        self.block(&self.body.block)?;
        if !matches!(
            self.code.last().map(|i| i.op),
            Some(Op::Ret | Op::RetV | Op::RetA)
        ) {
            self.push(Instr::new(Op::Ret, NO_REG, NO_REG, NO_REG, 0));
        }
        let shift = self.prologue.len() as u64;
        for ins in &mut self.code {
            if ins.op.is_jump() {
                ins.imm += shift;
            }
        }
        let mut code = std::mem::take(&mut self.prologue);
        code.append(&mut self.code);
        let mut params = Vec::new();
        for id in &self.body.params {
            let local = self.body.local(*id);
            let size = abi::size(self.cx, local.ty) as u32;
            let load = class(self.cx, local.ty)
                .filter(|c| *c != Class::Void)
                .map(|c| load_op(c) as u16)
                .unwrap_or(0);
            params.push(BcParam {
                reg: self.value[id.0 as usize].unwrap_or(NO_REG),
                offset: self.mem[id.0 as usize].unwrap_or(0),
                size,
                load,
            });
        }
        let (_, ret) = self
            .cx
            .fn_sig(self.func.ty)
            .ok_or_else(|| "function without a signature".to_string())?;
        Ok(BcFunction {
            name: self.cx.name_str(self.func.name).to_string(),
            key: fn_key(self.cx, self.func.id),
            signature: dump::ty_string(self.cx, self.func.ty),
            public: self.func.vis == Vis::Pub,
            frame_size: align_up(self.frame_size, 8),
            regs: self.regs,
            params,
            ret_size: abi::size(self.cx, ret) as u32,
            ret_aggregate: !is_scalar(self.cx, ret) && class(self.cx, ret) != Some(Class::Void),
            calls: self.calls,
            code,
        })
    }

    fn reg(&mut self) -> u32 {
        let r = self.regs;
        self.regs += 1;
        r
    }

    fn push(&mut self, ins: Instr) -> usize {
        let at = self.code.len();
        self.code.push(ins);
        self.last_compare = None;
        at
    }

    fn patch(&mut self, at: usize, target: usize) {
        self.code[at].imm = target as u64;
    }

    fn here(&self) -> usize {
        self.code.len()
    }

    fn const_reg(&mut self, bits: u64) -> u32 {
        if let Some(reg) = self.const_regs.get(&bits) {
            return *reg;
        }
        let dst = self.reg();
        self.const_regs.insert(bits, dst);
        self.prologue.push(Instr::new(Op::Const, dst, NO_REG, NO_REG, bits));
        dst
    }

    fn temp_mem(&mut self, ty: Ty) -> Addr {
        let align = (abi::align(self.cx, ty) as u32).max(8);
        self.frame_size = align_up(self.frame_size, align);
        let offset = self.frame_size;
        self.frame_size += (abi::size(self.cx, ty) as u32).max(8);
        Addr {
            base: FP,
            offset: offset as u64,
        }
    }

    fn materialize(&mut self, at: Addr) -> u32 {
        if at.offset == 0 && at.base != FP {
            return at.base;
        }
        let dst = self.reg();
        self.push(Instr::new(Op::Addr, dst, at.base, NO_REG, at.offset));
        dst
    }

    fn load(&mut self, at: Addr, ty: Ty) -> Result<u32, String> {
        let c = class(self.cx, ty).ok_or("load of an aggregate")?;
        let dst = self.reg();
        self.push(Instr::new(load_op(c), dst, at.base, NO_REG, at.offset));
        Ok(dst)
    }

    fn store(&mut self, at: Addr, value: u32, ty: Ty) {
        let size = abi::size(self.cx, ty);
        if size == 0 {
            return;
        }
        if is_scalar(self.cx, ty) {
            self.push(Instr::new(store_op(size), NO_REG, at.base, value, at.offset));
        } else {
            let dst = self.materialize(at);
            self.push(Instr::new(Op::Copy, NO_REG, dst, value, size));
        }
    }

    fn place_addr(&mut self, place: &Place) -> Result<Addr, String> {
        let mut projs = place.proj.as_slice();
        let (mut at, mut ty) = match place.root {
            PlaceRoot::Local(id) => {
                let local = self.body.local(id);
                match (self.mem[id.0 as usize], self.value[id.0 as usize]) {
                    (Some(offset), _) => (
                        Addr {
                            base: FP,
                            offset: offset as u64,
                        },
                        local.ty,
                    ),
                    (None, Some(reg)) => {
                        let Some((Proj::Deref, rest)) = projs.split_first() else {
                            return Err("address of a register local".to_string());
                        };
                        projs = rest;
                        let inner = self
                            .cx
                            .ref_inner(local.ty)
                            .ok_or_else(|| "deref of a non-ref local".to_string())?;
                        (Addr { base: reg, offset: 0 }, inner)
                    }
                    (None, None) => return Err("local without storage".to_string()),
                }
            }
            PlaceRoot::Global(id) => {
                let index = self
                    .shared
                    .globals
                    .iter()
                    .position(|g| *g == id)
                    .ok_or_else(|| {
                        format!(
                            "`{}` is another module's value; scripts reach only their own `shared`",
                            self.cx.name_str(self.cx.global(id).name)
                        )
                    })?;
                let dst = self.reg();
                self.push(Instr::new(Op::Global, dst, NO_REG, NO_REG, index as u64));
                (Addr { base: dst, offset: 0 }, self.cx.global(id).ty)
            }
        };
        for proj in projs {
            match proj {
                Proj::Deref => {
                    let dst = self.reg();
                    self.push(Instr::new(Op::Ld64, dst, at.base, NO_REG, at.offset));
                    at = Addr { base: dst, offset: 0 };
                    ty = self.cx.ref_inner(ty).ok_or("deref of a non-ref place")?;
                }
                Proj::Field { field, ty: field_ty } => {
                    at.offset += abi::field_offset(self.cx, ty, field.index as usize)
                        .ok_or("field without an ABI offset")?;
                    ty = *field_ty;
                }
                Proj::Index { index, ty: elem } => {
                    let data = if matches!(
                        self.cx.ty_kind(ty),
                        TyKind::Slice { .. } | TyKind::String
                    ) {
                        let dst = self.reg();
                        self.push(Instr::new(Op::Ld64, dst, at.base, NO_REG, at.offset));
                        dst
                    } else {
                        self.materialize(at)
                    };
                    let idx = self.operand(index)?;
                    let dst = self.reg();
                    self.push(Instr::new(
                        Op::Idx,
                        dst,
                        data,
                        idx,
                        abi::size(self.cx, *elem),
                    ));
                    at = Addr { base: dst, offset: 0 };
                    ty = *elem;
                }
                Proj::Ptr => {
                    ty = self.cx.common.ptr;
                }
                Proj::Len => {
                    at.offset += 8;
                    ty = self.cx.common.int;
                }
                Proj::EnumTag => {
                    ty = self.cx.common.i32;
                }
                Proj::EnumPayload { ty: payload, .. } => {
                    at.offset += abi::field_offset(self.cx, ty, 1)
                        .ok_or("enum payload without an ABI offset")?;
                    ty = *payload;
                }
            }
        }
        Ok(at)
    }

    fn local_reg(&self, place: &Place) -> Option<u32> {
        if !place.proj.is_empty() {
            return None;
        }
        let PlaceRoot::Local(id) = place.root else {
            return None;
        };
        self.value[id.0 as usize]
    }

    /// A scalar operand's register, or an aggregate operand's address register.
    fn operand(&mut self, operand: &Operand) -> Result<u32, String> {
        match operand {
            Operand::Const(c) => {
                if let ConstKind::Bytes(bytes) = &c.kind {
                    let index = self.shared.constants.len() as u64;
                    self.shared.constants.push(bytes.clone());
                    let at = self.temp_mem(self.cx.common.string);
                    let dst = self.materialize(at);
                    self.push(Instr::new(Op::Str, NO_REG, dst, NO_REG, index));
                    return Ok(dst);
                }
                let bits = match c.kind {
                    ConstKind::Void | ConstKind::Null => 0,
                    ConstKind::Bool(v) => u64::from(v),
                    ConstKind::Int(v) => v,
                    ConstKind::F32Bits(v) => v as u64,
                    ConstKind::F64Bits(v) => v,
                    ConstKind::FnAddr(id) => {
                        return Err(format!(
                            "taking the address of `{}` isn't supported in scripts",
                            self.cx.name_str(self.cx.func(id).name)
                        ))
                    }
                    ConstKind::Bytes(_) => unreachable!(),
                };
                let c = class(self.cx, c.ty).unwrap_or(Class::Ptr);
                Ok(self.const_reg(canonical(c, bits)))
            }
            Operand::Copy(place) => {
                if !is_scalar(self.cx, place.ty) {
                    let at = self.place_addr(place)?;
                    return Ok(self.materialize(at));
                }
                if let Some(reg) = self.local_reg(place) {
                    return Ok(reg);
                }
                let at = self.place_addr(place)?;
                self.load(at, place.ty)
            }
        }
    }

    fn block(&mut self, block: &Block) -> Result<(), String> {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            self.stmt(stmt)?;
        }
        let defers = self.scopes.pop().expect("scope");
        for defer in defers.iter().rev() {
            self.block(defer)?;
        }
        Ok(())
    }

    fn unwind_to(&mut self, depth: usize) -> Result<(), String> {
        let defers: Vec<Block> = self
            .scopes
            .iter()
            .skip(depth)
            .rev()
            .flat_map(|ds| ds.iter().rev().cloned())
            .collect();
        for defer in &defers {
            self.block(defer)?;
        }
        Ok(())
    }

    /// Emit "jump to `target` when `cond` is false", fusing a compare that just wrote it.
    fn jump_false(&mut self, cond: &Operand, fuse: bool) -> Result<usize, String> {
        let reg = self.operand(cond)?;
        if let (true, Some((at, written)), Operand::Copy(place)) = (fuse, self.last_compare, cond) {
            let single_use = match place.root {
                PlaceRoot::Local(id) if place.proj.is_empty() => self.uses[id.0 as usize] <= 2,
                _ => false,
            };
            if written == reg && single_use && at + 1 == self.code.len() {
                let cmp = self.code.pop().expect("compare");
                let fused = match cmp.op {
                    Op::Lts => Op::JnLts,
                    Op::Les => Op::JnLes,
                    Op::Ltu => Op::JnLtu,
                    Op::Leu => Op::JnLeu,
                    Op::Eq => Op::JnEq,
                    Op::Ne => Op::JnNe,
                    Op::Flt => Op::JnFlt,
                    Op::Fle => Op::JnFle,
                    Op::Dlt => Op::JnDlt,
                    Op::Dle => Op::JnDle,
                    _ => unreachable!(),
                };
                return Ok(self.push(Instr::new(fused, NO_REG, cmp.a, cmp.b, 0)));
            }
        }
        Ok(self.push(Instr::new(Op::Jz, NO_REG, reg, NO_REG, 0)))
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                if let Some(reg) = self.local_reg(place) {
                    self.scalar_into(rvalue, reg)?;
                } else if is_scalar(self.cx, place.ty) {
                    let value = self.scalar(rvalue)?;
                    let at = self.place_addr(place)?;
                    self.store(at, value, place.ty);
                } else {
                    let at = self.place_addr(place)?;
                    let dst = self.materialize(at);
                    self.aggregate_into(dst, rvalue)?;
                }
            }
            StmtKind::AssignOp { place, op, rhs } => {
                let value = self.operand(rhs)?;
                if let Some(reg) = self.local_reg(place) {
                    self.binary(reg, *op, place.ty, reg, value)?;
                } else {
                    let at = self.place_addr(place)?;
                    let old = self.load(at, place.ty)?;
                    let new = self.reg();
                    self.binary(new, *op, place.ty, old, value)?;
                    self.store(at, new, place.ty);
                }
            }
            StmtKind::Defer { body } => self
                .scopes
                .last_mut()
                .expect("defer scope")
                .push(body.clone()),
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let fail = self.jump_false(cond, true)?;
                self.block(then_block)?;
                if let Some(else_block) = else_block {
                    let end = self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, 0));
                    let here = self.here();
                    self.patch(fail, here);
                    self.block(else_block)?;
                    let here = self.here();
                    self.patch(end, here);
                } else {
                    let here = self.here();
                    self.patch(fail, here);
                }
            }
            StmtKind::While { cond, body } => {
                let header = self.here();
                let exit = self.jump_false(cond, false)?;
                self.loops.push(LoopFixups {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scopes.len(),
                });
                self.block(body)?;
                self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, header as u64));
                let end = self.here();
                self.patch(exit, end);
                let fixes = self.loops.pop().expect("while loop");
                for at in fixes.breaks {
                    self.patch(at, end);
                }
                for at in fixes.continues {
                    self.patch(at, header);
                }
            }
            StmtKind::ForRange { index, end, body } => {
                let signed = matches!(
                    class(self.cx, index.ty),
                    Some(Class::Int { signed: true, .. })
                );
                let header = self.here();
                let (index_reg, index_mem) = match self.local_reg(index) {
                    Some(reg) => (reg, None),
                    None => {
                        let at = self.place_addr(index)?;
                        (self.load(at, index.ty)?, Some(at))
                    }
                };
                let end_reg = self.operand(end)?;
                let jump = if signed { Op::JnLts } else { Op::JnLtu };
                let exit = self.push(Instr::new(jump, NO_REG, index_reg, end_reg, 0));
                self.loops.push(LoopFixups {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scopes.len(),
                });
                self.block(body)?;
                let increment = self.here();
                let one = self.const_reg(1);
                match index_mem {
                    None => {
                        self.binary(index_reg, BinOp::Add, index.ty, index_reg, one)?;
                    }
                    Some(at) => {
                        let old = self.load(at, index.ty)?;
                        let new = self.reg();
                        self.binary(new, BinOp::Add, index.ty, old, one)?;
                        self.store(at, new, index.ty);
                    }
                }
                self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, header as u64));
                let finish = self.here();
                self.patch(exit, finish);
                let fixes = self.loops.pop().expect("range loop");
                for at in fixes.breaks {
                    self.patch(at, finish);
                }
                for at in fixes.continues {
                    self.patch(at, increment);
                }
            }
            StmtKind::Return { value } => {
                self.unwind_to(0)?;
                match value {
                    None => {
                        self.push(Instr::new(Op::Ret, NO_REG, NO_REG, NO_REG, 0));
                    }
                    Some(v) if class(self.cx, v.ty()) == Some(Class::Void) => {
                        self.push(Instr::new(Op::Ret, NO_REG, NO_REG, NO_REG, 0));
                    }
                    Some(v) => {
                        let reg = self.operand(v)?;
                        if is_scalar(self.cx, v.ty()) {
                            self.push(Instr::new(Op::RetV, NO_REG, reg, NO_REG, 0));
                        } else {
                            let size = abi::size(self.cx, v.ty());
                            self.push(Instr::new(Op::RetA, NO_REG, reg, NO_REG, size));
                        }
                    }
                }
            }
            StmtKind::Break | StmtKind::Continue => {
                let depth = self
                    .loops
                    .last()
                    .ok_or_else(|| "break/continue outside a loop".to_string())?
                    .scope_depth;
                self.unwind_to(depth)?;
                let at = self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, 0));
                let fixes = self.loops.last_mut().expect("loop");
                if matches!(stmt.kind, StmtKind::Break) {
                    fixes.breaks.push(at);
                } else {
                    fixes.continues.push(at);
                }
            }
            StmtKind::Expr(rvalue) => {
                if is_scalar(self.cx, rvalue.ty) {
                    self.scalar(rvalue)?;
                } else if class(self.cx, rvalue.ty) == Some(Class::Void) {
                    self.void_rvalue(rvalue)?;
                } else {
                    let at = self.temp_mem(rvalue.ty);
                    let dst = self.materialize(at);
                    self.aggregate_into(dst, rvalue)?;
                }
            }
            StmtKind::Abort => {
                self.push(Instr::new(Op::Abort, NO_REG, NO_REG, NO_REG, 0));
            }
            StmtKind::Div0 => {
                self.push(Instr::new(Op::Div0, NO_REG, NO_REG, NO_REG, 0));
            }
            StmtKind::Block(block) => self.block(block)?,
            StmtKind::InterpFill { buf, used, parts } => {
                let buf_at = self.place_addr(buf)?;
                let buf = self.materialize(buf_at);
                let used_at = self.place_addr(used)?;
                let used = self.materialize(used_at);
                for part in parts {
                    match part {
                        InterpPart::Lit(bytes) => self.append_literal(buf, used, bytes),
                        InterpPart::Value(op) => self.append_value(buf, used, op)?,
                    }
                }
            }
        }
        Ok(())
    }

    fn void_rvalue(&mut self, rv: &Rvalue) -> Result<(), String> {
        match &rv.kind {
            RvalueKind::Call { func, args } => self.call(NO_REG, rv.ty, func, args),
            RvalueKind::Use(_) => Ok(()),
            RvalueKind::Syscall { .. } => Err("`@syscall` isn't supported in scripts".to_string()),
            _ => Err("unsupported void expression".to_string()),
        }
    }

    /// Evaluate a scalar rvalue into a fresh or existing register.
    fn scalar(&mut self, rv: &Rvalue) -> Result<u32, String> {
        if let RvalueKind::Use(op) = &rv.kind {
            return self.operand(op);
        }
        let dst = self.reg();
        self.scalar_into(rv, dst)?;
        Ok(dst)
    }

    fn scalar_into(&mut self, rv: &Rvalue, dst: u32) -> Result<(), String> {
        match &rv.kind {
            RvalueKind::Use(op) => {
                let src = self.operand(op)?;
                if src != dst {
                    self.push(Instr::new(Op::Mov, dst, src, NO_REG, 0));
                }
            }
            RvalueKind::Ref { place } => {
                let at = self.place_addr(place)?;
                self.push(Instr::new(Op::Addr, dst, at.base, NO_REG, at.offset));
            }
            RvalueKind::Binary { op, lhs, rhs } => {
                let a = self.operand(lhs)?;
                let b = self.operand(rhs)?;
                self.binary(dst, *op, lhs.ty(), a, b)?;
            }
            RvalueKind::Unary { op, arg } => {
                let a = self.operand(arg)?;
                let c = class(self.cx, arg.ty()).ok_or("unary on an aggregate")?;
                match (op, c) {
                    (UnaryOp::Not, _) => {
                        self.push(Instr::new(Op::Not, dst, a, NO_REG, 0));
                    }
                    (UnaryOp::Neg, Class::F32) => {
                        self.push(Instr::new(Op::Fneg, dst, a, NO_REG, 0));
                    }
                    (UnaryOp::Neg, Class::F64) => {
                        self.push(Instr::new(Op::Dneg, dst, a, NO_REG, 0));
                    }
                    (UnaryOp::Neg, _) => {
                        self.push(Instr::new(Op::Neg, dst, a, NO_REG, 0));
                        self.extend(dst, c);
                    }
                    (UnaryOp::BitNot, _) => {
                        return Err("bit operators aren't supported in scripts".to_string())
                    }
                }
            }
            RvalueKind::Cast { from } => {
                let a = self.operand(from)?;
                self.cast(dst, from.ty(), rv.ty, a)?;
            }
            RvalueKind::Call { func, args } => self.call(dst, rv.ty, func, args)?,
            RvalueKind::Syscall { .. } => {
                return Err("`@syscall` isn't supported in scripts".to_string())
            }
            RvalueKind::Aggregate { .. } | RvalueKind::Enum { .. } => {
                return Err("aggregate value in a scalar slot".to_string())
            }
            RvalueKind::Interpolate { .. } => {
                return Err("interpolate must be lowered before bytecode".to_string())
            }
        }
        Ok(())
    }

    fn extend(&mut self, reg: u32, c: Class) {
        if let Some(op) = ext_op(c) {
            self.push(Instr::new(op, reg, reg, NO_REG, 0));
        }
    }

    fn binary(&mut self, dst: u32, op: BinOp, ty: Ty, a: u32, b: u32) -> Result<(), String> {
        let c = class(self.cx, ty).ok_or("binary op on an aggregate")?;
        let (code, swap, compare) = match c {
            Class::F32 | Class::F64 => {
                let d = c == Class::F64;
                let pick = |f: Op, g: Op| if d { g } else { f };
                match op {
                    BinOp::Add => (pick(Op::Fadd, Op::Dadd), false, false),
                    BinOp::Sub => (pick(Op::Fsub, Op::Dsub), false, false),
                    BinOp::Mul => (pick(Op::Fmul, Op::Dmul), false, false),
                    BinOp::Div => (pick(Op::Fdiv, Op::Ddiv), false, false),
                    BinOp::Eq => (pick(Op::Feq, Op::Deq), false, true),
                    BinOp::Ne => (pick(Op::Fne, Op::Dne), false, true),
                    BinOp::Lt => (pick(Op::Flt, Op::Dlt), false, true),
                    BinOp::Le => (pick(Op::Fle, Op::Dle), false, true),
                    BinOp::Gt => (pick(Op::Flt, Op::Dlt), true, true),
                    BinOp::Ge => (pick(Op::Fle, Op::Dle), true, true),
                    _ => return Err(format!("`{op:?}` on a float isn't supported in scripts")),
                }
            }
            _ => {
                let signed = matches!(c, Class::Int { signed: true, .. });
                match op {
                    BinOp::Add => (Op::Add, false, false),
                    BinOp::Sub => (Op::Sub, false, false),
                    BinOp::Mul => (Op::Mul, false, false),
                    BinOp::Div => (if signed { Op::Divs } else { Op::Divu }, false, false),
                    BinOp::Rem => (if signed { Op::Rems } else { Op::Remu }, false, false),
                    BinOp::Eq => (Op::Eq, false, true),
                    BinOp::Ne => (Op::Ne, false, true),
                    BinOp::Lt => (if signed { Op::Lts } else { Op::Ltu }, false, true),
                    BinOp::Le => (if signed { Op::Les } else { Op::Leu }, false, true),
                    BinOp::Gt => (if signed { Op::Lts } else { Op::Ltu }, true, true),
                    BinOp::Ge => (if signed { Op::Les } else { Op::Leu }, true, true),
                    BinOp::And => (Op::Land, false, false),
                    BinOp::Or => (Op::Lor, false, false),
                    BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr => {
                        return Err("bit operators aren't supported in scripts".to_string())
                    }
                }
            }
        };
        let (a, b) = if swap { (b, a) } else { (a, b) };
        let at = self.push(Instr::new(code, dst, a, b, 0));
        if compare {
            self.last_compare = Some((at, dst));
        } else {
            self.extend(dst, c);
        }
        Ok(())
    }

    fn cast(&mut self, dst: u32, from: Ty, to: Ty, a: u32) -> Result<(), String> {
        let f = class(self.cx, from).ok_or("cast from an aggregate")?;
        let t = class(self.cx, to).ok_or("cast to an aggregate")?;
        let signed = |c: Class| matches!(c, Class::Int { signed: true, .. });
        let op = match (f, t) {
            (Class::F32, Class::F32) | (Class::F64, Class::F64) => Op::Mov,
            (Class::F32, Class::F64) => Op::F2d,
            (Class::F64, Class::F32) => Op::D2f,
            (Class::F32, _) => {
                if signed(t) || !matches!(t, Class::Int { bits: 64, .. }) {
                    Op::F2i
                } else {
                    Op::F2u
                }
            }
            (Class::F64, _) => {
                if signed(t) || !matches!(t, Class::Int { bits: 64, .. }) {
                    Op::D2i
                } else {
                    Op::D2u
                }
            }
            (_, Class::F32) => {
                if signed(f) {
                    Op::I2f
                } else {
                    Op::U2f
                }
            }
            (_, Class::F64) => {
                if signed(f) {
                    Op::I2d
                } else {
                    Op::U2d
                }
            }
            (_, Class::Bool) => {
                let zero = self.const_reg(0);
                self.push(Instr::new(Op::Ne, dst, a, zero, 0));
                return Ok(());
            }
            _ => Op::Mov,
        };
        self.push(Instr::new(op, dst, a, NO_REG, 0));
        self.extend(dst, t);
        Ok(())
    }

    /// Evaluate an aggregate rvalue into the memory at `dst` (an address register).
    fn aggregate_into(&mut self, dst: u32, rv: &Rvalue) -> Result<(), String> {
        match &rv.kind {
            RvalueKind::Use(op) => {
                let src = self.operand(op)?;
                let size = abi::size(self.cx, rv.ty);
                if src != dst && size > 0 {
                    self.push(Instr::new(Op::Copy, NO_REG, dst, src, size));
                }
            }
            RvalueKind::Call { func, args } => self.call(dst, rv.ty, func, args)?,
            RvalueKind::Aggregate { fields } => {
                for (i, field) in fields.iter().enumerate() {
                    let offset = aggregate_offset(self.cx, rv.ty, i)?;
                    let value = self.operand(field)?;
                    self.store(Addr { base: dst, offset }, value, field.ty());
                }
            }
            RvalueKind::Enum { variant, payload } => {
                let tag = self.const_reg(variant.index as u64);
                self.push(Instr::new(Op::St32, NO_REG, dst, tag, 0));
                if let Some(payload) = payload {
                    let offset = abi::field_offset(self.cx, rv.ty, 1)
                        .ok_or("enum payload without an ABI offset")?;
                    let value = self.operand(payload)?;
                    self.store(Addr { base: dst, offset }, value, payload.ty());
                }
            }
            RvalueKind::Syscall { .. } => {
                return Err("`@syscall` isn't supported in scripts".to_string())
            }
            RvalueKind::Interpolate { .. } => {
                return Err("interpolate must be lowered before bytecode".to_string())
            }
            _ => return Err("scalar value in an aggregate slot".to_string()),
        }
        Ok(())
    }

    fn call(&mut self, dst: u32, ret: Ty, func: &Operand, args: &[Operand]) -> Result<(), String> {
        let id = match func {
            Operand::Const(c) => match c.kind {
                ConstKind::FnAddr(id) => id,
                _ => return Err("call of a non-function constant".to_string()),
            },
            Operand::Copy(_) => {
                return Err("calling a function value isn't supported in scripts".to_string())
            }
        };
        let target = self.cx.func(id);
        let (kind, index) = if let Some(index) = self.shared.local_index.get(&id) {
            (CallKind::Local, *index)
        } else if is_script_module(self.cx, &target.module) {
            let path = script_path(self.cx, &target.module);
            if !self.shared.scripts.contains(&path) {
                self.shared.scripts.push(path);
            }
            (CallKind::Script, 0)
        } else {
            let key = fn_key(self.cx, id);
            if !self.shared.natives.iter().any(|(k, _)| *k == key) {
                self.shared.natives.push((key, fn_label(self.cx, id)));
            }
            (CallKind::Native, 0)
        };
        let mut lowered = Vec::new();
        for arg in args {
            lowered.push(CallArg {
                reg: self.operand(arg)?,
                size: abi::size(self.cx, arg.ty()) as u32,
                aggregate: !is_scalar(self.cx, arg.ty()),
            });
        }
        let ret_class = class(self.cx, ret);
        let site = self.calls.len() as u64;
        self.calls.push(CallSite {
            kind,
            key: fn_key(self.cx, id),
            index,
            ret_size: abi::size(self.cx, ret) as u32,
            ret_load: match ret_class {
                Some(Class::Void) | None => 0,
                Some(c) => load_op(c) as u16,
            },
            ret_aggregate: ret_class.is_none(),
            args: lowered,
        });
        self.push(Instr::new(Op::Call, dst, NO_REG, NO_REG, site));
        Ok(())
    }

    fn append_literal(&mut self, buf: u32, used: u32, bytes: &[u8]) {
        let index = self.shared.constants.len() as u64;
        self.shared.constants.push(bytes.to_vec());
        self.push(Instr::new(Op::AppLit, buf, used, NO_REG, index));
    }

    fn append_value(&mut self, buf: u32, used: u32, op: &Operand) -> Result<(), String> {
        let ty = op.ty();
        if matches!(self.cx.ty_kind(ty), TyKind::String) {
            let value = self.operand(op)?;
            self.push(Instr::new(Op::AppStr, buf, used, value, 0));
            return Ok(());
        }
        match class(self.cx, ty) {
            Some(Class::Bool) => {
                let value = self.operand(op)?;
                self.push(Instr::new(Op::AppBool, buf, used, value, 0));
                return Ok(());
            }
            Some(Class::Int { .. }) => {
                let value = self.operand(op)?;
                self.push(Instr::new(Op::AppInt, buf, used, value, 0));
                return Ok(());
            }
            Some(Class::F32) => {
                let value = self.operand(op)?;
                let wide = self.reg();
                self.push(Instr::new(Op::F2d, wide, value, NO_REG, 0));
                self.push(Instr::new(Op::AppF64, buf, used, wide, 0));
                return Ok(());
            }
            Some(Class::F64) => {
                let value = self.operand(op)?;
                self.push(Instr::new(Op::AppF64, buf, used, value, 0));
                return Ok(());
            }
            _ => {}
        }
        if let TyKind::Struct(id) = self.cx.ty_kind(ty) {
            let def = self.cx.struct_def(*id);
            if !def.debug {
                return Err("interpolate of a non-#[Debug] struct".to_string());
            }
            let Operand::Copy(base) = op else {
                return Err("interpolated struct must be a place".to_string());
            };
            self.append_literal(buf, used, format!("{} {{ ", self.cx.name_str(def.name)).as_bytes());
            for (index, field) in def.fields.iter().enumerate() {
                if index > 0 {
                    self.append_literal(buf, used, b", ");
                }
                self.append_literal(
                    buf,
                    used,
                    format!("{}: ", self.cx.name_str(field.name)).as_bytes(),
                );
                let place = base.clone().project(
                    Proj::Field {
                        field: FieldId {
                            struct_id: *id,
                            index: index as u32,
                        },
                        ty: field.ty,
                    },
                    field.ty,
                    base.access,
                    base.span,
                );
                self.append_value(buf, used, &Operand::Copy(place))?;
            }
            self.append_literal(buf, used, b" }");
            return Ok(());
        }
        Err("interpolate of this type isn't supported in scripts".to_string())
    }
}

fn aggregate_offset(cx: &IrCx, ty: Ty, index: usize) -> Result<u64, String> {
    match cx.ty_kind(ty) {
        TyKind::Array { elem, .. } => Ok(abi::size(cx, *elem) * index as u64),
        TyKind::Struct(_) | TyKind::Slice { .. } | TyKind::String => {
            abi::field_offset(cx, ty, index).ok_or_else(|| "aggregate field without ABI offset".to_string())
        }
        _ => Err("aggregate of a non-aggregate type".to_string()),
    }
}

/// Locals that must live in memory: their address is taken, or `InterpFill` writes them.
/// A ref local read through `Deref` keeps its register.
fn addressed_locals(block: &Block, count: usize) -> Vec<bool> {
    let mut marked = vec![false; count];
    walk_block(block, &mut |place, force| {
        if let PlaceRoot::Local(id) = place.root {
            let deref_first = matches!(place.proj.first(), Some(Proj::Deref));
            if force && !deref_first {
                marked[id.0 as usize] = true;
            }
        }
    });
    marked
}

fn count_block(block: &Block, uses: &mut [u32]) {
    walk_block(block, &mut |place, _| {
        if let PlaceRoot::Local(id) = place.root {
            uses[id.0 as usize] += 1;
        }
    });
}

/// Visit every place; `force` is true where the place's address is taken.
fn walk_block(block: &Block, f: &mut dyn FnMut(&Place, bool)) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                walk_place(place, false, f);
                walk_rvalue(rvalue, f);
            }
            StmtKind::AssignOp { place, rhs, .. } => {
                walk_place(place, false, f);
                walk_operand(rhs, f);
            }
            StmtKind::Defer { body } | StmtKind::Block(body) => walk_block(body, f),
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                walk_operand(cond, f);
                walk_block(then_block, f);
                if let Some(other) = else_block {
                    walk_block(other, f);
                }
            }
            StmtKind::While { cond, body } => {
                walk_operand(cond, f);
                walk_block(body, f);
            }
            StmtKind::Return { value } => {
                if let Some(op) = value {
                    walk_operand(op, f);
                }
            }
            StmtKind::Expr(rv) => walk_rvalue(rv, f),
            StmtKind::ForRange { index, end, body } => {
                walk_place(index, false, f);
                walk_operand(end, f);
                walk_block(body, f);
            }
            StmtKind::InterpFill { buf, used, parts } => {
                walk_place(buf, true, f);
                walk_place(used, true, f);
                for part in parts {
                    if let InterpPart::Value(op) = part {
                        walk_operand(op, f);
                    }
                }
            }
            StmtKind::Break | StmtKind::Continue | StmtKind::Abort | StmtKind::Div0 => {}
        }
    }
}

fn walk_rvalue(rv: &Rvalue, f: &mut dyn FnMut(&Place, bool)) {
    match &rv.kind {
        RvalueKind::Use(op) | RvalueKind::Unary { arg: op, .. } | RvalueKind::Cast { from: op } => {
            walk_operand(op, f)
        }
        RvalueKind::Ref { place } => walk_place(place, true, f),
        RvalueKind::Binary { lhs, rhs, .. } => {
            walk_operand(lhs, f);
            walk_operand(rhs, f);
        }
        RvalueKind::Call { func, args } => {
            walk_operand(func, f);
            for arg in args {
                walk_operand(arg, f);
            }
        }
        RvalueKind::Syscall { nr, args } => {
            walk_operand(nr, f);
            for arg in args {
                walk_operand(arg, f);
            }
        }
        RvalueKind::Aggregate { fields } => {
            for field in fields {
                walk_operand(field, f);
            }
        }
        RvalueKind::Enum { payload, .. } => {
            if let Some(op) = payload {
                walk_operand(op, f);
            }
        }
        RvalueKind::Interpolate { parts } => {
            for part in parts {
                if let InterpPart::Value(op) = part {
                    walk_operand(op, f);
                }
            }
        }
    }
}

fn walk_operand(op: &Operand, f: &mut dyn FnMut(&Place, bool)) {
    if let Operand::Copy(place) = op {
        walk_place(place, false, f);
    }
}

fn walk_place(place: &Place, force: bool, f: &mut dyn FnMut(&Place, bool)) {
    f(place, force);
    for proj in &place.proj {
        if let Proj::Index { index, .. } = proj {
            walk_operand(index, f);
        }
    }
}

fn align_up(value: u32, align: u32) -> u32 {
    (value + align - 1) / align * align
}
