//! J3 compiler library.
//!
//! Pipeline: parse → typecheck → typed IR → emit (LLVM default, C optional).

pub mod android_run;
pub mod check;
pub mod cli;
pub mod preflight;
pub mod compile;
pub mod config;
pub mod csslint;
pub mod devrun;
pub mod hot;
pub mod i18n;
pub mod diag;
pub mod dist;
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
pub mod phase;
pub mod golden;
pub mod imgdiff;
pub mod png;
pub mod project;
pub mod scaffold;
pub mod scene;
pub mod scenefile;
pub mod span;
pub mod ui;
pub mod stdlib;
