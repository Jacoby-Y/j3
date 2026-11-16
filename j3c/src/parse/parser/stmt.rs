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
                if !matches!(self.look(1).kind, TokenKind::Eq | TokenKind::Colon) {
                    self.forbid_doc(&doc)?;
                    let pat = self.pat(false)?;
                    self.expect(TokenKind::Eq, "expected `=` after a `let` pattern")?;
                    let value = self.expr()?;
                    let else_block = if self.cur().kind == TokenKind::Else {
                        let _ = self.bump();
                        Some(self.block()?)
                    } else {
                        None
                    };
                    self.expect(TokenKind::Semi, "expected `;` after let")?;
                    return Ok(Stmt::LetPat(LetPatStmt { span: join(start, self.prev_end(start)), pat, value, else_block }));
                }
                let name = self.ident()?;
                let mut ty = None;
                if self.cur().kind == TokenKind::Colon {
                    let _ = self.bump();
                    ty = Some(self.ty()?);
                }
                // `let buf: [64]u8;`: zero-filled, the type says what
                let value = if ty.is_some() && self.cur().kind == TokenKind::Semi {
                    Expr { span: join(start, self.prev_end(start)), kind: ExprKind::Zeroed }
                } else {
                    self.expect(TokenKind::Eq, "expected `=` in let")?;
                    self.expr()?
                };
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
            TokenKind::If => self.if_any_stmt(),
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
            TokenKind::Label => {
                let start = self.cur().span;
                let label = self.label_ident();
                self.expect(TokenKind::Colon, "expected `:` after a loop label")?;
                if !matches!(self.cur().kind, TokenKind::While | TokenKind::For) {
                    return Err(self.err("a label goes on a `while` or `for` loop"));
                }
                let body = self.stmt()?;
                Ok(Stmt::Labeled(LabeledStmt {
                    span: join(start, stmt_span(&body)),
                    label,
                    body: Box::new(body),
                }))
            }
            TokenKind::Break | TokenKind::Continue => {
                let is_break = self.cur().kind == TokenKind::Break;
                let start = self.cur().span;
                let _ = self.bump();
                let label = if self.cur().kind == TokenKind::Label { Some(self.label_ident()) } else { None };
                self.expect(TokenKind::Semi, "expected `;` after break or continue")?;
                let span = join(start, self.prev_end(start));
                Ok(if is_break { Stmt::Break(span, label) } else { Stmt::Continue(span, label) })
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
                    | TokenKind::SlashEq
                    | TokenKind::PercentEq
                    | TokenKind::AmpEq
                    | TokenKind::PipeEq
                    | TokenKind::CaretEq
                    | TokenKind::ShlEq
                    | TokenKind::ShrEq => {
                        let op = match self.cur().kind {
                            TokenKind::PlusEq => BinOp::Add,
                            TokenKind::MinusEq => BinOp::Sub,
                            TokenKind::StarEq => BinOp::Mul,
                            TokenKind::SlashEq => BinOp::Div,
                            TokenKind::AmpEq => BinOp::BitAnd,
                            TokenKind::PipeEq => BinOp::BitOr,
                            TokenKind::CaretEq => BinOp::BitXor,
                            TokenKind::ShlEq => BinOp::Shl,
                            TokenKind::ShrEq => BinOp::Shr,
                            _ => BinOp::Rem,
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
                let inner = self.if_any_stmt()?;
                else_block = Some(Block {
                    span: stmt_span(&inner),
                    stmts: vec![inner],
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

    /// The current `'name` token as an `Ident` named `name`.
    fn label_ident(&mut self) -> Ident {
        let tok = self.bump().clone();
        Ident { name: tok.text[1..].to_string(), span: tok.span }
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
        if self.cur().kind == TokenKind::LParen {
            return self.for_tuple(start);
        }
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

    /// `for (k, v) in xs { .. }`: each element bound to a hidden name, then destructured by a
    /// `let (k, v) = ..;` at the top of the body.
    fn for_tuple(&mut self, start: Span) -> Result<Stmt, ParseError> {
        let pat = self.pat(false)?;
        self.expect(TokenKind::In, "expected `in`")?;
        let iter = self.expr()?;
        let mut body = self.block()?;
        let name = Ident { name: format!("$each{}", start.start), span: pat.span() };
        let value = Expr { span: name.span, kind: ExprKind::Name(name.clone()) };
        body.stmts.insert(0, Stmt::LetPat(LetPatStmt { span: pat.span(), pat, value, else_block: None }));
        Ok(Stmt::ForEach(ForEachStmt { span: join(start, body.span), index: None, name, iter, body }))
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

    /// `pat | pat if guard { body }`.
    pub(super) fn match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.cur().span;
        let pats = self.or_pats()?;
        let mut guard = None;
        if self.cur().kind == TokenKind::If {
            let _ = self.bump();
            guard = Some(self.expr()?);
        }
        let (body, tail) = self.arm_block()?;
        Ok(MatchArm { span: join(start, body.span), pats, guard, body, tail })
    }

    /// `A | B(x)`: an arm's alternatives.
    pub(super) fn or_pats(&mut self) -> Result<Vec<Pat>, ParseError> {
        let mut pats = vec![self.pat(true)?];
        while self.cur().kind == TokenKind::Pipe {
            let _ = self.bump();
            pats.push(self.pat(true)?);
        }
        Ok(pats)
    }

    /// One pattern. `top`: an arm's own pattern, where a single name is an enum path or an int
    /// `const` (resolved by the checker), never a binding.
    pub(super) fn pat(&mut self, top: bool) -> Result<Pat, ParseError> {
        let start = self.cur().span;
        if self.cur().kind == TokenKind::Ident && self.cur().text == "_" {
            let _ = self.bump();
            return Ok(Pat::Wild(start));
        }
        if self.cur().kind == TokenKind::LParen {
            let _ = self.bump();
            let mut elems = vec![self.pat(false)?];
            while self.cur().kind == TokenKind::Comma && self.look(1).kind != TokenKind::RParen {
                let _ = self.bump();
                elems.push(self.pat(false)?);
            }
            if self.cur().kind == TokenKind::Comma {
                let _ = self.bump();
            }
            if elems.len() < 2 {
                return Err(ParseError::new(start, "a tuple pattern has two or more elements"));
            }
            self.expect(TokenKind::RParen, "expected `,` or `)` in a tuple pattern")?;
            return Ok(Pat::Tuple { span: join(start, self.prev_end(start)), elems });
        }
        if self.cur().kind == TokenKind::Ident && matches!(self.cur().text.as_str(), "true" | "false") {
            let v = self.bump().text == "true";
            return Ok(Pat::Lit(Expr { span: start, kind: ExprKind::Bool(v) }));
        }
        if self.cur().kind == TokenKind::String {
            let tok = self.bump().clone();
            return Ok(Pat::Lit(Expr { span: tok.span, kind: ExprKind::String(tok.text.into_bytes()) }));
        }
        let lo = if matches!(self.cur().kind, TokenKind::Int | TokenKind::Minus | TokenKind::Char) {
            self.pat_lit()?
        } else {
            let path = self.path()?;
            let span = join(start, self.prev_end(start));
            if self.cur().kind != TokenKind::DotDotEq {
                if self.cur().kind == TokenKind::LBrace && (!top || self.struct_pat_ahead()) {
                    return self.struct_pat(start, path);
                }
                if path.len() == 1 && !top && self.cur().kind != TokenKind::LParen {
                    return Ok(Pat::Bind(path.into_iter().next().unwrap()));
                }
                let mut payload = Vec::new();
                if self.cur().kind == TokenKind::LParen {
                    let _ = self.bump();
                    loop {
                        payload.push(self.pat(false)?);
                        if self.cur().kind != TokenKind::Comma || self.look(1).kind == TokenKind::RParen {
                            break;
                        }
                        let _ = self.bump();
                    }
                    if self.cur().kind == TokenKind::Comma {
                        let _ = self.bump();
                    }
                    self.expect(TokenKind::RParen, "expected `,` or `)` in a pattern")?;
                }
                return Ok(Pat::Variant { span: join(start, self.prev_end(start)), path, payload });
            }
            path_expr(&path, span)
        };
        if self.cur().kind != TokenKind::DotDotEq {
            if self.cur().kind == TokenKind::DotDot {
                return Err(self.err("a range pattern includes its end: write `..=`"));
            }
            return Ok(Pat::Lit(lo));
        }
        let _ = self.bump();
        let hi = if matches!(self.cur().kind, TokenKind::Int | TokenKind::Minus | TokenKind::Char) {
            self.pat_lit()?
        } else {
            let s = self.cur().span;
            let path = self.path()?;
            path_expr(&path, join(s, self.prev_end(s)))
        };
        Ok(Pat::Range { lo, hi })
    }

    /// At an arm's `Path {`: is the `{ .. }` a struct pattern? Yes when what follows its `}` can
    /// only follow a pattern (`{` of the body, `|`, `if`), not start the next arm.
    fn struct_pat_ahead(&self) -> bool {
        let mut depth = 0usize;
        let mut n = 0;
        loop {
            match self.look(n).kind {
                TokenKind::LBrace => depth += 1,
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(self.look(n + 1).kind, TokenKind::LBrace | TokenKind::Pipe | TokenKind::If);
                    }
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            n += 1;
        }
    }

    /// `Path { a, b: pat, .. }`, at the `{`.
    fn struct_pat(&mut self, start: Span, path: Vec<Ident>) -> Result<Pat, ParseError> {
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let mut fields = Vec::new();
        let mut rest = false;
        while self.cur().kind != TokenKind::RBrace {
            if self.cur().kind == TokenKind::DotDot {
                let _ = self.bump();
                rest = true;
                break;
            }
            let name = self.ident()?;
            let pat = if self.cur().kind == TokenKind::Colon {
                let _ = self.bump();
                self.pat(false)?
            } else {
                Pat::Bind(name.clone())
            };
            fields.push((name, pat));
            if self.cur().kind != TokenKind::Comma {
                break;
            }
            let _ = self.bump();
        }
        let msg = if rest { "`..` ends a struct pattern: expected `}`" } else { "expected `,` or `}` in a struct pattern" };
        self.expect(TokenKind::RBrace, msg)?;
        Ok(Pat::Struct { span: join(start, self.prev_end(start)), path, fields, rest })
    }

    /// `3`, `-1`, `'a'` in a pattern.
    fn pat_lit(&mut self) -> Result<Expr, ParseError> {
        let start = self.cur().span;
        let neg = self.cur().kind == TokenKind::Minus;
        if neg {
            let _ = self.bump();
        }
        let lit = self.cur().clone();
        let kind = match lit.kind {
            TokenKind::Int => parse_int(&lit.text).map_err(|m| ParseError::new(lit.span, m))?,
            TokenKind::Char if !neg => ExprKind::Char(lit.text.parse().unwrap_or(0)),
            _ => return Err(self.err("expected an integer or char pattern")),
        };
        let _ = self.bump();
        let mut value = Expr { span: join(start, lit.span), kind };
        if neg {
            value = Expr { span: value.span, kind: ExprKind::Unary { op: UnaryOp::Neg, expr: Box::new(value) } };
        }
        Ok(value)
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
}
