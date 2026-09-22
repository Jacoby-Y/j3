//! J3 compiler library.
//!
//! Pipeline (speculation.md §16): parse → typecheck → typed IR → emit (LLVM default, C optional).

pub mod check;
pub mod cli;
pub mod compile;
pub mod diag;
pub mod doc;
pub mod emit;
pub mod fmt;
pub mod helptext;
pub mod intern;
pub mod ir;
pub mod lsp;
pub mod parse;
pub mod project;
pub mod scaffold;
pub mod span;
pub mod stdlib;
