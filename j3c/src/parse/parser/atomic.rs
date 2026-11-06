//! `@atomic*` arguments: the pointer, the values, and an optional trailing memory-order word.

use super::*;
use crate::ir::op::{AtomicOp, AtomicOrder};

impl<'a> Parser<'a> {
    /// Splits the order word off `args` (when present) and checks it against `op`.
    pub(super) fn atomic_order(
        &mut self,
        op: AtomicOp,
        args: &mut Vec<Expr>,
        name_span: Span,
    ) -> Result<AtomicOrder, ParseError> {
        let n = op.values() + 1;
        if args.len() == n {
            return Ok(AtomicOrder::SeqCst);
        }
        if args.len() != n + 1 {
            return Err(ParseError::new(
                name_span,
                format!(
                    "`@{}` takes a pointer and {} value(s), then an optional order (e.g. `acquire`)",
                    op.name(),
                    op.values()
                ),
            ));
        }
        let last = args.pop().unwrap();
        let valid = |op: AtomicOp| op.orders().iter().map(|o| o.name()).collect::<Vec<_>>().join(", ");
        let word = match &last.kind {
            ExprKind::Name(id) => AtomicOrder::from_name(&id.name),
            _ => None,
        };
        match word {
            Some(o) if op.orders().contains(&o) => Ok(o),
            Some(o) => Err(ParseError::new(
                last.span,
                format!("`@{}` cannot use order `{}`; use one of: {}", op.name(), o.name(), valid(op)),
            )),
            None => Err(ParseError::new(
                last.span,
                format!("`@{}` takes a memory order here; use one of: {}", op.name(), valid(op)),
            )),
        }
    }
}
