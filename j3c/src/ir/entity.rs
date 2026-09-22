//! Structs, enums, functions. Layout is C; payloadless enums are i32.

use crate::intern::Name;
use crate::ir::body::Body;
use crate::ir::ids::{FieldId, FuncId, GlobalId, StructId, VariantId};
use crate::ir::operand::Operand;
use crate::ir::ty::Ty;
use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Vis {
    Pub,
    Private,
}

#[derive(Clone, Debug)]
pub struct FieldDef {
    pub name: Name,
    pub doc: String,
    pub ty: Ty,
    /// `use field: Type` embedding. Methods are not promoted.
    pub embedded: bool,
    /// Literal from `name: Type = 0`. Omitted struct-lit fields copy this.
    pub default: Option<Operand>,
}

#[derive(Clone, Debug)]
pub struct StructDef {
    pub module: Vec<Name>,
    pub name: Name,
    pub doc: String,
    pub fields: Vec<FieldDef>,
    pub packed: bool,
    pub debug: bool,
    pub span: Span,
}

impl StructDef {
    pub fn field(&self, id: FieldId) -> &FieldDef {
        &self.fields[id.index as usize]
    }
}

#[derive(Clone, Debug)]
pub struct VariantDef {
    pub name: Name,
    pub doc: String,
    /// `None` = payloadless. Any payload makes the whole enum a tagged union.
    pub payload: Option<Ty>,
}

#[derive(Clone, Copy, Debug)]
pub struct TryCatch {
    pub success: VariantId,
    pub failure: VariantId,
}

#[derive(Clone, Debug)]
pub struct EnumDef {
    pub module: Vec<Name>,
    pub name: Name,
    pub doc: String,
    pub variants: Vec<VariantDef>,
    pub try_catch: Option<TryCatch>,
    pub debug: bool,
    pub span: Span,
}

impl EnumDef {
    pub fn is_c_like(&self) -> bool {
        for variant in &self.variants {
            if variant.payload.is_some() {
                return false;
            }
        }
        true
    }
}

#[derive(Clone, Debug)]
pub enum FuncKind {
    /// Ordinary J3 function with a typed body.
    J3 { body: Body },
    /// `use lib` import. Linker resolves `symbol` in `lib`.
    Lib { lib: Name, symbol: Name },
    /// Declared, body filled in later (stdlib stubs in samples).
    Decl,
}

#[derive(Clone, Debug)]
pub struct GlobalDef {
    pub id: GlobalId,
    pub doc: String,
    pub module: Vec<Name>,
    pub name: Name,
    pub ty: Ty,
    pub span: Span,
    /// `None` = BSS zero. Const-folded singleton initializer.
    pub init: Option<ConstInit>,
}

#[derive(Clone, Debug)]
pub enum ConstInit {
    Scalar(Operand),
    Fields(Vec<(String, ConstInit)>),
    Elems(Vec<ConstInit>),
}

#[derive(Clone, Debug)]
pub struct Func {
    pub id: FuncId,
    pub doc: String,
    pub module: Vec<Name>,
    pub name: Name,
    pub ty: Ty,
    pub vis: Vis,
    pub method_of: Option<StructId>,
    pub kind: FuncKind,
    pub span: Span,
}
