//! `|`, `^`, `&` as binary operators. Binding order, tightest first: `<< >>`, `&`, `^`, `|`, then
//! comparisons. (C puts `&` below `==`; here `x & MASK == 0` is `(x & MASK) == 0`.) A `&` seen
//! here follows a finished operand, so it is binary; prefix address-of is parsed in `unary`.
use super::*;

impl<'a> Parser<'a> {
    pub(super) fn expr_bitor(&mut self) -> Result<Expr, ParseError> {
        self.bit_level(TokenKind::Pipe, BinOp::BitOr, Self::expr_bitxor)
    }

    fn expr_bitxor(&mut self) -> Result<Expr, ParseError> {
        self.bit_level(TokenKind::Caret, BinOp::BitXor, Self::expr_bitand)
    }

    fn expr_bitand(&mut self) -> Result<Expr, ParseError> {
        self.bit_level(TokenKind::Amp, BinOp::BitAnd, Self::expr_shift)
    }

    fn bit_level(
        &mut self,
        tok: TokenKind,
        op: BinOp,
        next: fn(&mut Self) -> Result<Expr, ParseError>,
    ) -> Result<Expr, ParseError> {
        let mut lhs = next(self)?;
        while self.cur().kind == tok {
            let _ = self.bump();
            let rhs = next(self)?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) },
            };
        }
        Ok(lhs)
    }
}
