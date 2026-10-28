use super::*;

impl<'a> Parser<'a> {
    pub(super) fn block(&mut self) -> Result<Block, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let mut stmts = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            stmts.push(self.stmt()?);
        }
        self.expect(TokenKind::RBrace, "expected `}`")?;
        Ok(Block {
            span: join(start, self.prev_end(start)),
            stmts,
        })
    }

    pub(super) fn stmt(&mut self) -> Result<Stmt, ParseError> {
        let doc = self.take_doc();
        if !doc.is_empty() && self.cur().kind != TokenKind::Let {
            return Err(self.err(
                "doc comment must sit on a fn, struct, enum, const, let, field, or variant",
            ));
        }
        if self.j3ui_recv.is_some()
            && self.cur().kind == TokenKind::Ident
            && self.look(1).kind == TokenKind::LParen
            && is_j3ui_widget(&self.cur().text)
        {
            return self.j3ui_widget_stmt();
        }
        match self.cur().kind {
            TokenKind::LBrace => Ok(Stmt::Block(self.block()?)),
            TokenKind::Let => {
                let start = self.cur().span;
                let _ = self.bump();
                let name = self.ident()?;
                let mut ty = None;
                if self.cur().kind == TokenKind::Colon {
                    let _ = self.bump();
                    ty = Some(self.ty()?);
                }
                self.expect(TokenKind::Eq, "expected `=` in let")?;
                let value = self.expr()?;
                self.expect(TokenKind::Semi, "expected `;` after let")?;
                Ok(Stmt::Let(LetStmt {
                    span: join(start, self.prev_end(start)),
                    doc,
                    name,
                    ty,
                    value,
                }))
            }
            TokenKind::Return => {
                let start = self.cur().span;
                let _ = self.bump();
                let mut value = None;
                if self.cur().kind != TokenKind::Semi {
                    value = Some(self.expr()?);
                }
                self.expect(TokenKind::Semi, "expected `;` after return")?;
                Ok(Stmt::Return(ReturnStmt {
                    span: join(start, self.prev_end(start)),
                    value,
                }))
            }
            TokenKind::If => Ok(Stmt::If(self.if_stmt()?)),
            TokenKind::While => Ok(Stmt::While(self.while_stmt()?)),
            TokenKind::For => self.for_stmt(),
            TokenKind::Match => Ok(Stmt::Match(self.match_stmt()?)),
            TokenKind::Use => {
                let start = self.cur().span;
                let _ = self.bump();
                let name = self.ident()?;
                self.expect(TokenKind::Semi, "expected `;` after `use`")?;
                Ok(Stmt::UseLocal(UseLocalStmt {
                    span: join(start, self.prev_end(start)),
                    name,
                }))
            }
            TokenKind::Defer => Ok(Stmt::Defer(self.defer_stmt()?)),
            TokenKind::Break => {
                let span = self.cur().span;
                let _ = self.bump();
                self.expect(TokenKind::Semi, "expected `;` after break")?;
                Ok(Stmt::Break(span))
            }
            TokenKind::Continue => {
                let span = self.cur().span;
                let _ = self.bump();
                self.expect(TokenKind::Semi, "expected `;` after continue")?;
                Ok(Stmt::Continue(span))
            }
            _ => {
                let start = self.cur().span;
                let place = self.expr()?;
                match self.cur().kind {
                    TokenKind::Eq => {
                        let _ = self.bump();
                        let value = self.expr()?;
                        self.expect(TokenKind::Semi, "expected `;` after assign")?;
                        Ok(Stmt::Assign(AssignStmt {
                            span: join(start, self.prev_end(start)),
                            place,
                            value,
                        }))
                    }
                    TokenKind::PlusEq
                    | TokenKind::MinusEq
                    | TokenKind::StarEq
                    | TokenKind::SlashEq => {
                        let kind = self.cur().kind;
                        if kind == TokenKind::SlashEq {
                            return Err(self.err("`/=` not in this subset"));
                        }
                        let op = match kind {
                            TokenKind::PlusEq => BinOp::Add,
                            TokenKind::MinusEq => BinOp::Sub,
                            TokenKind::StarEq => BinOp::Mul,
                            _ => BinOp::Add,
                        };
                        let _ = self.bump();
                        let value = self.expr()?;
                        self.expect(TokenKind::Semi, "expected `;` after assign-op")?;
                        Ok(Stmt::AssignOp(AssignOpStmt {
                            span: join(start, self.prev_end(start)),
                            place,
                            op,
                            value,
                        }))
                    }
                    _ => {
                        self.expect(TokenKind::Semi, "expected `;` after expression")?;
                        Ok(Stmt::Expr(place))
                    }
                }
            }
        }
    }

    pub(super) fn if_stmt(&mut self) -> Result<IfStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::If, "expected `if`")?;
        let cond = self.expr()?;
        let then_block = self.block()?;
        let mut else_block = None;
        if self.cur().kind == TokenKind::Else {
            let _ = self.bump();
            if self.cur().kind == TokenKind::If {
                let inner = self.if_stmt()?;
                else_block = Some(Block {
                    span: inner.span,
                    stmts: vec![Stmt::If(inner)],
                });
            } else {
                else_block = Some(self.block()?);
            }
        }
        Ok(IfStmt {
            span: join(start, self.prev_end(start)),
            cond,
            then_block,
            else_block,
        })
    }

    pub(super) fn while_stmt(&mut self) -> Result<WhileStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::While, "expected `while`")?;
        let cond = self.expr()?;
        let body = self.block()?;
        Ok(WhileStmt {
            span: join(start, body.span),
            cond,
            body,
        })
    }

    pub(super) fn for_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::For, "expected `for`")?;
        let first = self.ident()?;
        if self.cur().kind == TokenKind::Comma {
            let _ = self.bump();
            let name = self.ident()?;
            self.expect(TokenKind::In, "expected `in`")?;
            let iter = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::ForEach(ForEachStmt {
                span: join(start, body.span),
                index: Some(first),
                name,
                iter,
                body,
            }));
        }
        self.expect(TokenKind::In, "expected `in`")?;
        let iter = self.expr()?;
        if self.cur().kind == TokenKind::DotDot || self.cur().kind == TokenKind::DotDotEq {
            let inclusive = self.cur().kind == TokenKind::DotDotEq;
            let _ = self.bump();
            let end = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::For(ForStmt {
                span: join(start, body.span),
                name: first,
                start: iter,
                end,
                inclusive,
                body,
            }));
        }
        let body = self.block()?;
        Ok(Stmt::ForEach(ForEachStmt {
            span: join(start, body.span),
            index: None,
            name: first,
            iter,
            body,
        }))
    }

    pub(super) fn match_stmt(&mut self) -> Result<MatchStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Match, "expected `match`")?;
        let expr = self.expr()?;
        self.expect(TokenKind::LBrace, "expected `{` after match")?;
        let mut arms = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            arms.push(self.match_arm()?);
        }
        self.expect(TokenKind::RBrace, "expected `}` after match")?;
        Ok(MatchStmt {
            span: join(start, self.prev_end(start)),
            expr,
            arms,
        })
    }

    pub(super) fn match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.cur().span;
        let path = self.path()?;
        let mut payload = None;
        if self.cur().kind == TokenKind::LParen {
            let _ = self.bump();
            payload = Some(self.pat()?);
            self.expect(TokenKind::RParen, "expected `)` after match payload")?;
        }
        let body = self.block()?;
        Ok(MatchArm {
            span: join(start, body.span),
            path,
            payload,
            body,
        })
    }

    pub(super) fn pat(&mut self) -> Result<Pat, ParseError> {
        if self.cur().kind == TokenKind::Ident && self.cur().text == "_" {
            let _ = self.bump();
            return Ok(Pat::Wild);
        }
        let path = self.path()?;
        if path.len() == 1
            && self.cur().kind != TokenKind::LParen
            && self.cur().kind != TokenKind::Dot
        {
            return Ok(Pat::Bind(path.into_iter().next().unwrap()));
        }
        let mut inner = None;
        if self.cur().kind == TokenKind::LParen {
            let _ = self.bump();
            inner = Some(Box::new(self.pat()?));
            self.expect(TokenKind::RParen, "expected `)` after pattern")?;
        }
        Ok(Pat::Variant {
            path,
            payload: inner,
        })
    }

    pub(super) fn defer_stmt(&mut self) -> Result<DeferStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Defer, "expected `defer`")?;
        let body = if self.cur().kind == TokenKind::LBrace {
            self.block()?
        } else {
            let stmt = self.stmt()?;
            Block {
                span: stmt_span(&stmt),
                stmts: vec![stmt],
            }
        };
        Ok(DeferStmt {
            span: join(start, body.span),
            body,
        })
    }

    pub(super) fn ty(&mut self) -> Result<Type, ParseError> {
        let start = self.cur().span;
        if self.cur().kind == TokenKind::Amp {
            let _ = self.bump();
            let inner = self.ty()?;
            return Ok(Type {
                span: join(start, inner.span),
                kind: TypeKind::Ref(Box::new(inner)),
            });
        }
        if self.cur().kind == TokenKind::LBracket {
            let _ = self.bump();
            if self.cur().kind == TokenKind::RBracket {
                let _ = self.bump();
                let elem = self.ty()?;
                return Ok(Type {
                    span: join(start, elem.span),
                    kind: TypeKind::Slice(Box::new(elem)),
                });
            }
            let ntok = self.cur().clone();
            if ntok.kind != TokenKind::Int {
                return Err(self.err("expected array length or `[]`"));
            }
            let _ = self.bump();
            let len = parse_int(&ntok.text).map_err(|m| ParseError::new(ntok.span, m))?;
            if len < 0 {
                return Err(ParseError::new(ntok.span, "array length must be >= 0"));
            }
            self.expect(TokenKind::RBracket, "expected `]` after array length")?;
            let elem = self.ty()?;
            return Ok(Type {
                span: join(start, elem.span),
                kind: TypeKind::Array {
                    len: len as u64,
                    elem: Box::new(elem),
                },
            });
        }
        if self.cur().kind == TokenKind::Fn {
            let _ = self.bump();
            self.expect(TokenKind::LParen, "expected `(` after `fn`")?;
            let mut params = Vec::new();
            if self.cur().kind != TokenKind::RParen {
                loop {
                    params.push(self.ty()?);
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
            self.expect(TokenKind::RParen, "expected `)` after fn type params")?;
            let mut ret = None;
            if self.cur().kind == TokenKind::Colon {
                let _ = self.bump();
                ret = Some(Box::new(self.ty()?));
            }
            return Ok(Type {
                span: join(start, self.prev_end(start)),
                kind: TypeKind::Fn { params, ret },
            });
        }
        let name = self.ident()?;
        if self.cur().kind == TokenKind::Lt {
            let _ = self.bump();
            let mut args = Vec::new();
            loop {
                args.push(self.ty()?);
                if self.cur().kind == TokenKind::Comma {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::Gt {
                        break;
                    }
                    continue;
                }
                break;
            }
            self.expect(TokenKind::Gt, "expected `>` after type arguments")?;
            return Ok(Type {
                span: join(name.span, self.prev_end(name.span)),
                kind: TypeKind::Generic {
                    name: name.name,
                    args,
                },
            });
        }
        Ok(Type {
            span: name.span,
            kind: TypeKind::Named(name.name),
        })
    }
}
