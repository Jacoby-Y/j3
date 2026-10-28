//! Function bodies: locals plus a structured block.

use crate::intern::Name;
use crate::ir::ids::LocalId;
use crate::ir::place::Access;
use crate::ir::stmt::Stmt;
use crate::ir::ty::Ty;
use crate::span::Span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LocalKind {
    Param,
    Let,
    Const,
    /// Compiler temporary (e.g. `ref` of a place before a call).
    Tmp,
}

#[derive(Clone, Debug)]
pub struct LocalDecl {
    pub id: LocalId,
    pub doc: String,
    pub name: Name,
    pub ty: Ty,
    pub access: Access,
    pub kind: LocalKind,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

impl Block {
    pub fn empty() -> Block {
        Block { stmts: Vec::new() }
    }
}

#[derive(Clone, Debug)]
pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub params: Vec<LocalId>,
    pub block: Block,
}

impl Body {
    pub fn local(&self, id: LocalId) -> &LocalDecl {
        &self.locals[id.0 as usize]
    }
}
