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

mod call;
mod codegen;
mod encode;
mod op;
mod stmt;

use codegen::*;
pub use op::*;

pub const VERSION: u16 = 2;
pub const NO_REG: u32 = u32::MAX;
/// Register 0 is the frame pointer.
const FP: u32 = 0;

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
