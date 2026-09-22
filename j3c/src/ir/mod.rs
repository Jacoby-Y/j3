//! Typed IR.
//!
//! After typecheck, every node has a concrete [`ty::Ty`]. There is no
//! `Option<Ty>`. If a pass cannot name a type, that is a compiler bug.
//!
//! **Places** are first-class (speculation.md §2). Field and index of `&T`
//! name a slot; they do not memcpy. `+=` binds a place once. Local `use`
//! aliases are places. Auto-deref is extra [`place::Proj::Deref`] entries,
//! not a load into a temporary.

pub mod abi;
pub mod body;
pub mod build;
pub mod cx;
pub mod dump;
pub mod entity;
pub mod fast;
pub mod ids;
pub mod module;
pub mod op;
pub mod operand;
pub mod place;
pub mod rvalue;
pub mod samples;
pub mod stmt;
pub mod ty;
pub mod validate;
pub mod wrap;

pub use body::{Body, LocalDecl, LocalKind};
pub use cx::IrCx;
pub use module::Module;
pub use place::{Access, Place, Proj};
pub use ty::{FloatTy, IntTy, Ty, TyKind};
