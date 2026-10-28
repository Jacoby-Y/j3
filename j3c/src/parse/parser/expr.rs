use super::*;

impl<'a> Parser<'a> {
    pub(super) fn expr(&mut self) -> Result<Expr, ParseError> {
        self.expr_or()
    }

    pub(super) fn expr_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_and()?;
        while self.cur().kind == TokenKind::OrOr {
            let _ = self.bump();
            let rhs = self.expr_and()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    pub(super) fn expr_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_eq()?;
        while self.cur().kind == TokenKind::AndAnd {
            let _ = self.bump();
            let rhs = self.expr_eq()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    pub(super) fn expr_eq(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_rel()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_rel()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    pub(super) fn expr_rel(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_add()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_add()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    pub(super) fn expr_add(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_mul()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_mul()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    pub(super) fn expr_mul(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_as()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                TokenKind::Percent => BinOp::Rem,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_as()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    pub(super) fn expr_as(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.unary()?;
        while self.cur().kind == TokenKind::As {
            let _ = self.bump();
            let ty = self.ty()?;
            let span = join(expr.span, ty.span);
            expr = Expr {
                span,
                kind: ExprKind::Cast {
                    expr: Box::new(expr),
                    ty,
                },
            };
        }
        Ok(expr)
    }

    pub(super) fn unary(&mut self) -> Result<Expr, ParseError> {
        let start = self.cur().span;
        match self.cur().kind {
            TokenKind::Minus => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Amp => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Ref,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Star => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Deref,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Not => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    },
                })
            }
            _ => self.postfix(),
        }
    }

    pub(super) fn if_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::If, "expected `if`")?;
        let cond = self.expr()?;
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let then_expr = self.expr()?;
        self.expect(TokenKind::RBrace, "expected `}`")?;
        self.expect(TokenKind::Else, "if-expression needs `else`")?;
        let else_expr = if self.cur().kind == TokenKind::If {
            self.if_expr()?
        } else {
            self.expect(TokenKind::LBrace, "expected `{`")?;
            let e = self.expr()?;
            self.expect(TokenKind::RBrace, "expected `}`")?;
            e
        };
        Ok(Expr {
            span: join(start, self.prev_end(start)),
            kind: ExprKind::If {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
        })
    }

    pub(super) fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(TokenKind::LParen, "expected `(`")?;
        let mut args = Vec::new();
        if self.cur().kind != TokenKind::RParen {
            loop {
                args.push(self.expr()?);
                if self.cur().kind == TokenKind::Comma {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::RParen {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RParen, "expected `)` after arguments")?;
        Ok(args)
    }

    pub(super) fn path(&mut self) -> Result<Vec<Ident>, ParseError> {
        let mut parts = Vec::new();
        parts.push(self.ident()?);
        while self.cur().kind == TokenKind::Dot && self.look(1).kind == TokenKind::Ident {
            let _ = self.bump();
            parts.push(self.ident()?);
        }
        Ok(parts)
    }
}
