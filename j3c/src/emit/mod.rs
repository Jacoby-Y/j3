//! Codegen printers. C and LLVM consume the same typed IR. Never parse J3 here.

mod c;
mod c_expr;
mod c_ty;
mod host;
mod llvm;
mod llvm_expr;
mod llvm_ty;
mod opts;

pub use c::{emit_c, emit_program};
pub use host::{
    compile_and_run, compile_and_run_with, compile_c, run_exe, run_exe_in, CcOpts, RunResult,
};
pub use llvm::emit_llvm;
pub use opts::{Backend, EmitOpts};

use crate::ir::body::Block;
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::module::Module;
use crate::ir::rvalue::RvalueKind;
use crate::ir::stmt::{Stmt, StmtKind};

pub fn emit_text(cx: &IrCx, modules: &[Module], opts: &EmitOpts) -> Result<String, String> {
    match opts.backend {
        Backend::C => emit_program(cx, modules),
        Backend::Llvm => emit_llvm(cx, modules, opts),
    }
}

pub fn compile_text(text: &str, opts: &EmitOpts) -> Result<std::path::PathBuf, String> {
    match opts.backend {
        Backend::C => host::compile_c(text, &opts.cc, &opts.target, &opts.clang),
        Backend::Llvm => host::compile_llvm(
            text,
            &opts.cc,
            &opts.clang,
            &opts.target,
            &opts.cpu,
            &opts.flags,
        ),
    }
}

pub fn run_program(
    cx: &IrCx,
    modules: &[Module],
    opts: &EmitOpts,
) -> Result<RunResult, String> {
    let text = emit_text(cx, modules, opts)?;
    let exe = compile_text(&text, opts)?;
    let result = host::run_exe_in(&exe, opts.cwd.as_deref());
    if opts.cc.out.is_none() {
        let _ = std::fs::remove_file(&exe);
    }
    result
}

pub fn clang_available(clang: &str) -> bool {
    std::process::Command::new(clang)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Default is LLVM. If clang is missing and the backend was not forced, use C.
pub fn resolve_backend(want: Backend, forced: bool, clang: &str) -> Result<Backend, String> {
    if want == Backend::C {
        return Ok(Backend::C);
    }
    if clang_available(clang) {
        return Ok(Backend::Llvm);
    }
    if forced {
        return Err(format!(
            "emit.backend=llvm but `{clang}` was not found; install clang or set [emit] backend = \"c\""
        ));
    }
    Ok(Backend::C)
}

pub(crate) fn contains_syscall(cx: &IrCx, modules: &[Module]) -> bool {
    for module in modules {
        for id in &module.funcs {
            let func = cx.func(*id);
            if let FuncKind::J3 { body } = &func.kind {
                if block_has_syscall(&body.block) {
                    return true;
                }
            }
        }
    }
    false
}

fn block_has_syscall(block: &Block) -> bool {
    block.stmts.iter().any(stmt_has_syscall)
}

fn stmt_has_syscall(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Assign { rvalue, .. } => matches!(rvalue.kind, RvalueKind::Syscall { .. }),
        StmtKind::Expr(rvalue) => matches!(rvalue.kind, RvalueKind::Syscall { .. }),
        StmtKind::Defer { body } | StmtKind::Block(body) => block_has_syscall(body),
        StmtKind::If {
            then_block,
            else_block,
            ..
        } => {
            block_has_syscall(then_block)
                || else_block.as_ref().map(block_has_syscall).unwrap_or(false)
        }
        StmtKind::While { body, .. } | StmtKind::ForRange { body, .. } => block_has_syscall(body),
        _ => false,
    }
}
