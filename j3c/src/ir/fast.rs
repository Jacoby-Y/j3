//! `--fast`: drop bounds-check `Abort` (and the `If` that only aborts). Same IR, same C printer.

use crate::ir::body::Block;
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::stmt::StmtKind;

pub fn strip_aborts(cx: &mut IrCx) {
    let n = cx.funcs().len();
    for i in 0..n {
        let id = crate::ir::ids::FuncId(i as u32);
        if let FuncKind::J3 { body } = &mut cx.func_mut(id).kind {
            strip_block(&mut body.block);
        }
    }
}

fn strip_block(block: &mut Block) {
    let mut out = Vec::new();
    for mut stmt in block.stmts.drain(..) {
        match &mut stmt.kind {
            StmtKind::Abort => continue,
            StmtKind::If {
                then_block,
                else_block,
                ..
            } => {
                let drop_if = is_abort_only(then_block)
                    && else_block.as_ref().map_or(true, |e| e.stmts.is_empty());
                strip_block(then_block);
                if let Some(e) = else_block.as_mut() {
                    strip_block(e);
                }
                if drop_if {
                    continue;
                }
                if then_block.stmts.is_empty()
                    && else_block.as_ref().map_or(true, |e| e.stmts.is_empty())
                {
                    continue;
                }
                out.push(stmt);
            }
            StmtKind::While { body, .. } => {
                strip_block(body);
                out.push(stmt);
            }
            StmtKind::ForRange { body, .. } => {
                strip_block(body);
                out.push(stmt);
            }
            StmtKind::Defer { body } => {
                strip_block(body);
                out.push(stmt);
            }
            StmtKind::Block(body) => {
                strip_block(body);
                out.push(stmt);
            }
            _ => out.push(stmt),
        }
    }
    block.stmts = out;
}

fn is_abort_only(block: &Block) -> bool {
    block.stmts.len() == 1 && matches!(block.stmts[0].kind, StmtKind::Abort)
}
