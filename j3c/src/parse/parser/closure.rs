//! Closure syntax: the literal `fn(a: int, b): T { body }` and the type `closure(A, B): T`.

use super::*;
use crate::parse::ast::{ClosureLit, ClosureParam};

impl<'a> Parser<'a> {
    /// After `closure` has been read, at `(`.
    pub(super) fn closure_type(&mut self, start: Span) -> Result<Type, ParseError> {
        self.expect(TokenKind::LParen, "expected `(` after `closure`")?;
        let mut params = Vec::new();
        while self.cur().kind != TokenKind::RParen {
            params.push(self.ty()?);
            if self.cur().kind != TokenKind::Comma {
                break;
            }
            let _ = self.bump();
        }
        self.expect(TokenKind::RParen, "expected `)` after closure type params")?;
        let mut ret = None;
        if self.cur().kind == TokenKind::Colon {
            let _ = self.bump();
            ret = Some(Box::new(self.ty()?));
        }
        Ok(Type {
            span: join(start, self.prev_end(start)),
            kind: TypeKind::Closure { params, ret },
        })
    }

    /// At `fn` in expression position.
    pub(super) fn closure_lit(&mut self) -> Result<Expr, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Fn, "expected `fn`")?;
        if self.cur().kind != TokenKind::LParen {
            return Err(self.err(
                "expected `(` after `fn`: a closure is `fn(x: int): int { ... }`; named functions are declared at the top level",
            ));
        }
        let _ = self.bump();
        let mut params = Vec::new();
        while self.cur().kind != TokenKind::RParen {
            let name = self.ident()?;
            let mut ty = None;
            if self.cur().kind == TokenKind::Colon {
                let _ = self.bump();
                ty = Some(self.ty()?);
            }
            params.push(ClosureParam { name, ty });
            if self.cur().kind != TokenKind::Comma {
                break;
            }
            let _ = self.bump();
        }
        self.expect(TokenKind::RParen, "expected `)` after closure parameters")?;
        let mut ret = None;
        if self.cur().kind == TokenKind::Colon {
            let _ = self.bump();
            ret = Some(self.ty()?);
        }
        let body = self.block()?;
        Ok(Expr {
            span: join(start, body.span),
            kind: ExprKind::Closure(Box::new(ClosureLit { params, ret, body })),
        })
    }
}
