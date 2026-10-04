//! J3 compiler library.
//!
//! Pipeline: parse → typecheck → typed IR → emit (LLVM default, C optional).

pub mod check;
pub mod cli;
pub mod preflight;
pub mod compile;
pub mod devrun;
pub mod diag;
pub mod doc;
pub mod editor;
pub mod emit;
pub mod fmt;
pub mod gen;
pub mod gltf;
pub mod helptext;
pub mod intern;
pub mod ir;
pub mod lsp;
pub mod mcp;
pub mod online;
pub mod parse;
pub mod png;
pub mod project;
pub mod scaffold;
pub mod scene;
pub mod scenefile;
pub mod span;
pub mod stdlib;
