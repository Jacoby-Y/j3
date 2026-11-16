//! Blocks that can end in a value: `match` arm bodies (`Op.Add { 1 }`) and `if let`, which is
//! parsed as a `match` with a `_` arm for its `else`.

use super::*;

impl<'a> Parser<'a> {
    /// `{ stmts tail }`: statements, then an optional expression with no `;` (the arm's value).
    /// A `match` or `if let` at the end is the value; elsewhere it is a statement.
    pub(super) fn arm_block(&mut self) -> Result<(Block, Option<Expr>), ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let mut stmts = Vec::new();
        let mut tail = None;
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            let at = self.i;
            match self.cur().kind {
                TokenKind::Match | TokenKind::If if self.cur().kind == TokenKind::Match || self.look(1).kind == TokenKind::Let => {
                    let m = if self.cur().kind == TokenKind::Match { self.match_stmt()? } else { self.if_let()?.0 };
                    if self.cur().kind == TokenKind::RBrace {
                        tail = Some(Expr { span: m.span, kind: ExprKind::Match(Box::new(m)) });
                    } else {
                        stmts.push(Stmt::Match(m));
                    }
                }
                // an `if` statement, else an if-expression as the value
                TokenKind::If => match self.stmt() {
                    Ok(s) => stmts.push(s),
                    Err(e) => {
                        let end = self.i;
                        self.i = at;
                        match self.if_expr() {
                            Ok(v) if self.cur().kind == TokenKind::RBrace => tail = Some(v),
                            _ => {
                                self.i = end;
                                return Err(e);
                            }
                        }
                    }
                },
                TokenKind::Let
                | TokenKind::Return
                | TokenKind::While
                | TokenKind::For
                | TokenKind::Use
                | TokenKind::Defer
                | TokenKind::Label
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::LBrace
                | TokenKind::Doc => stmts.push(self.stmt()?),
                _ => match self.expr() {
                    Ok(v) if self.cur().kind == TokenKind::RBrace => tail = Some(v),
                    _ => {
                        self.i = at;
                        stmts.push(self.stmt()?);
                    }
                },
            }
        }
        self.expect(TokenKind::RBrace, "expected `}`")?;
        Ok((Block { span: join(start, self.prev_end(start)), stmts }, tail))
    }

    /// `if let P = e { .. } else ..` as `match e { P { .. } _ { .. } }`; also says whether an
    /// `else` was written (an if-expression needs one).
    pub(super) fn if_let(&mut self) -> Result<(MatchStmt, bool), ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::If, "expected `if`")?;
        self.expect(TokenKind::Let, "expected `let`")?;
        let pats = self.or_pats()?;
        self.expect(TokenKind::Eq, "expected `=` after an `if let` pattern")?;
        let expr = self.expr()?;
        let (body, tail) = self.arm_block()?;
        let then_arm = MatchArm { span: join(start, body.span), pats, guard: None, body, tail };
        let has_else = self.cur().kind == TokenKind::Else;
        let else_start = self.cur().span;
        let (body, tail) = if !has_else {
            (Block { span: else_start, stmts: Vec::new() }, None)
        } else {
            let _ = self.bump();
            if self.cur().kind == TokenKind::If {
                self.else_if()?
            } else {
                self.arm_block()?
            }
        };
        let else_arm = MatchArm { span: join(else_start, body.span), pats: vec![Pat::Wild(else_start)], guard: None, body, tail };
        Ok((MatchStmt { span: join(start, self.prev_end(start)), expr, arms: vec![then_arm, else_arm] }, has_else))
    }

    /// The `if ..` after an `if let`'s `else`: another `if let` (the arm's value), an `if`
    /// statement, else an if-expression (the value).
    fn else_if(&mut self) -> Result<(Block, Option<Expr>), ParseError> {
        let start = self.cur().span;
        if self.look(1).kind == TokenKind::Let {
            let (m, _) = self.if_let()?;
            let span = m.span;
            return Ok((Block { span, stmts: Vec::new() }, Some(Expr { span, kind: ExprKind::Match(Box::new(m)) })));
        }
        let at = self.i;
        match self.if_stmt() {
            Ok(s) => Ok((Block { span: s.span, stmts: vec![Stmt::If(s)] }, None)),
            Err(e) => {
                let end = self.i;
                self.i = at;
                match self.if_expr() {
                    Ok(v) => Ok((Block { span: start, stmts: Vec::new() }, Some(v))),
                    Err(_) => {
                        self.i = end;
                        Err(e)
                    }
                }
            }
        }
    }

    /// An `if` statement or an `if let` (a `Stmt::Match`).
    pub(super) fn if_any_stmt(&mut self) -> Result<Stmt, ParseError> {
        if self.look(1).kind == TokenKind::Let {
            return Ok(Stmt::Match(self.if_let()?.0));
        }
        Ok(Stmt::If(self.if_stmt()?))
    }
}
