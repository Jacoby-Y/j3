//! Does a function body reach its end? On the AST, so a missing `return` is a compile error
//! instead of the runtime abort the backends insert ("function ended without returning a value").

use super::ast::*;

/// True when control never runs off the end of `block` (out of the function).
pub fn diverges(block: &Block) -> bool {
    block.stmts.iter().any(|s| stmt_diverges(s, false))
}

/// True when control never reaches the end of `block`: it returns, exits, or jumps with
/// `break`/`continue` (a `match` arm of type "never", the `else` of `let .. else`).
pub fn leaves(block: &Block) -> bool {
    block.stmts.iter().any(|s| stmt_diverges(s, true))
}

/// An arm tail that never gives a value: `panic(..)`, or a `match` whose arms all leave.
pub fn expr_diverges(e: &Expr) -> bool {
    expr_leaves(e, true)
}

fn expr_leaves(e: &Expr, jumps: bool) -> bool {
    match &e.kind {
        ExprKind::Match(m) => match_diverges(m, jumps),
        _ => is_exit_call(e),
    }
}

// enum matches list every variant and int matches need `_`, so the arms cover every case
fn match_diverges(m: &MatchStmt, jumps: bool) -> bool {
    let arm = |a: &MatchArm| {
        a.body.stmts.iter().any(|s| stmt_diverges(s, jumps)) || a.tail.as_ref().is_some_and(|t| expr_leaves(t, jumps))
    };
    !m.arms.is_empty() && m.arms.iter().all(arm)
}

fn stmt_diverges(s: &Stmt, jumps: bool) -> bool {
    match s {
        Stmt::Return(_) => true,
        Stmt::Break(..) | Stmt::Continue(..) => jumps,
        Stmt::If(s) => s.else_block.as_ref().is_some_and(|e| {
            s.then_block.stmts.iter().any(|x| stmt_diverges(x, jumps)) && e.stmts.iter().any(|x| stmt_diverges(x, jumps))
        }),
        Stmt::Match(m) => match_diverges(m, jumps),
        Stmt::While(w) => endless(w, None),
        Stmt::Labeled(l) => match &*l.body {
            Stmt::While(w) => endless(w, Some(&l.label.name)),
            _ => false,
        },
        Stmt::Block(b) => b.stmts.iter().any(|x| stmt_diverges(x, jumps)),
        Stmt::Expr(e) => expr_leaves(e, jumps),
        _ => false,
    }
}

/// `while true { .. }` with no `break` that leaves it.
fn endless(w: &WhileStmt, label: Option<&str>) -> bool {
    matches!(w.cond.kind, ExprKind::Bool(true)) && !breaks_out(&w.body, 0, label)
}

/// A `break` in `block` that leaves the loop `depth` levels out (0 = the loop owning `block`).
fn breaks_out(block: &Block, depth: u32, label: Option<&str>) -> bool {
    block.stmts.iter().any(|s| stmt_breaks(s, depth, label))
}

fn stmt_breaks(s: &Stmt, depth: u32, label: Option<&str>) -> bool {
    match s {
        Stmt::Break(_, None) => depth == 0,
        Stmt::Break(_, Some(l)) => Some(l.name.as_str()) == label,
        Stmt::If(s) => {
            breaks_out(&s.then_block, depth, label)
                || s.else_block.as_ref().is_some_and(|b| breaks_out(b, depth, label))
        }
        Stmt::Match(m) => match_breaks(m, depth, label),
        Stmt::Expr(Expr { kind: ExprKind::Match(m), .. }) => match_breaks(m, depth, label),
        Stmt::LetPat(l) => l.else_block.as_ref().is_some_and(|b| breaks_out(b, depth, label)),
        Stmt::Block(b) => breaks_out(b, depth, label),
        Stmt::Defer(d) => breaks_out(&d.body, depth, label),
        Stmt::While(w) => breaks_out(&w.body, depth + 1, label),
        Stmt::For(f) => breaks_out(&f.body, depth + 1, label),
        Stmt::ForEach(f) => breaks_out(&f.body, depth + 1, label),
        Stmt::Labeled(l) => stmt_breaks(&l.body, depth, label),
        _ => false,
    }
}

fn match_breaks(m: &MatchStmt, depth: u32, label: Option<&str>) -> bool {
    m.arms.iter().any(|a| {
        breaks_out(&a.body, depth, label)
            || matches!(&a.tail, Some(Expr { kind: ExprKind::Match(t), .. }) if match_breaks(t, depth, label))
    })
}

/// `panic(..)`, `sys.panic(..)`, `exit(..)`: calls that never come back.
fn is_exit_call(e: &Expr) -> bool {
    let ExprKind::Call { func, .. } = &e.kind else { return false };
    let name = match &func.kind {
        ExprKind::Name(n) => &n.name,
        ExprKind::Field { field, .. } => &field.name,
        _ => return false,
    };
    matches!(name.as_str(), "panic" | "panicAt" | "exit")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(src: &str) -> bool {
        let text = format!("module t;\nfn f(): int {{ {src} }}\n");
        let file = crate::parse::parse(&text, crate::span::FileId(1)).expect("parse");
        let Some(Item::Fn(f)) = file.items.iter().find(|i| matches!(i, Item::Fn(_))) else { panic!() };
        diverges(&f.body)
    }

    #[test]
    fn shapes() {
        assert!(body("return 1;"));
        assert!(!body("let x = 1;"));
        assert!(body("if true { return 1; } else { return 2; }"));
        assert!(!body("if true { return 1; }"));
        assert!(body("while true { }"));
        assert!(!body("while true { break; }"));
        assert!(body("while true { for i in 0..3 { break; } }"));
        assert!(!body("'a: while true { for i in 0..3 { break 'a; } }"));
        assert!(body("panic(\"x\");"));
        assert!(!body("for i in 0..3 { return 1; }"));
    }
}
