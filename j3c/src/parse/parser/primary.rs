use super::*;

impl<'a> Parser<'a> {
    pub(super) fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.primary()?;
        loop {
            match self.cur().kind {
                TokenKind::Dot => {
                    let _ = self.bump();
                    let field = self.ident()?;
                    let span = join(expr.span, field.span);
                    expr = Expr {
                        span,
                        kind: ExprKind::Field {
                            base: Box::new(expr),
                            field,
                        },
                    };
                    if self.cur().kind == TokenKind::ColonColon {
                        expr = self.turbofish_call(expr)?;
                    }
                }
                TokenKind::ColonColon => {
                    expr = self.turbofish_call(expr)?;
                }
                TokenKind::LParen => {
                    let args = self.args()?;
                    let span = join(expr.span, self.prev_end(expr.span));
                    expr = Expr {
                        span,
                        kind: ExprKind::Call {
                            func: Box::new(expr),
                            type_args: Vec::new(),
                            args,
                        },
                    };
                }
                TokenKind::LBrace if matches!(expr.kind, ExprKind::ScriptType(_)) => {
                    let ExprKind::ScriptType(path) = expr.kind else {
                        unreachable!()
                    };
                    let start = expr.span;
                    let fields = self.script_lit_fields()?;
                    expr = Expr {
                        span: join(start, self.prev_end(start)),
                        kind: ExprKind::ScriptLit { path, fields },
                    };
                }
                TokenKind::Question => {
                    let _ = self.bump();
                    expr = Expr {
                        span: join(expr.span, self.prev_end(expr.span)),
                        kind: ExprKind::Question {
                            expr: Box::new(expr),
                        },
                    };
                }
                TokenKind::LBracket => {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::Colon {
                        let _ = self.bump();
                        self.expect(TokenKind::RBracket, "expected `]` after `:`")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Slice {
                                base: Box::new(expr),
                                start: None,
                                end: None,
                            },
                        };
                        continue;
                    }
                    if self.cur().kind == TokenKind::DotDot {
                        let _ = self.bump();
                        let end = if self.cur().kind == TokenKind::RBracket {
                            None
                        } else {
                            Some(Box::new(self.expr()?))
                        };
                        self.expect(TokenKind::RBracket, "expected `]` after slice")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Slice {
                                base: Box::new(expr),
                                start: None,
                                end,
                            },
                        };
                        continue;
                    }
                    let first = self.expr()?;
                    if self.cur().kind == TokenKind::DotDot {
                        let _ = self.bump();
                        let end = if self.cur().kind == TokenKind::RBracket {
                            None
                        } else {
                            Some(Box::new(self.expr()?))
                        };
                        self.expect(TokenKind::RBracket, "expected `]` after slice")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Slice {
                                base: Box::new(expr),
                                start: Some(Box::new(first)),
                                end,
                            },
                        };
                    } else {
                        self.expect(TokenKind::RBracket, "expected `]` after index")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Index {
                                base: Box::new(expr),
                                index: Box::new(first),
                            },
                        };
                    }
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    pub(super) fn primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.cur().clone();
        match tok.kind {
            TokenKind::Fn => self.closure_lit(),
            TokenKind::Ident | TokenKind::This => {
                let _ = self.bump();
                if tok.text == "true" {
                    return Ok(Expr {
                        span: tok.span,
                        kind: ExprKind::Bool(true),
                    });
                }
                if tok.text == "false" {
                    return Ok(Expr {
                        span: tok.span,
                        kind: ExprKind::Bool(false),
                    });
                }
                let name = Ident {
                    name: tok.text,
                    span: tok.span,
                };
                if self.cur().kind == TokenKind::Lt {
                    let saved = self.i;
                    match self.type_args() {
                        Ok(type_args) => {
                            if self.cur().kind == TokenKind::Dot {
                                return Ok(Expr {
                                    span: join(name.span, self.prev_end(name.span)),
                                    kind: ExprKind::TypePath {
                                        name,
                                        args: type_args,
                                    },
                                });
                            }
                            if self.looks_like_struct_lit() {
                                return self.struct_lit(name, type_args);
                            }
                            self.i = saved;
                        }
                        Err(_) => {
                            self.i = saved;
                        }
                    }
                }
                if self.looks_like_struct_lit() {
                    return self.struct_lit(name, Vec::new());
                }
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Name(name),
                })
            }
            TokenKind::Int => {
                let _ = self.bump();
                let value = parse_int(&tok.text).map_err(|m| ParseError::new(tok.span, m))?;
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Int(value),
                })
            }
            TokenKind::Float => {
                let _ = self.bump();
                let value = tok.text.parse::<f64>().map_err(|_| {
                    ParseError::new(tok.span, format!("invalid float `{}`", tok.text))
                })?;
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Float(value),
                })
            }
            TokenKind::String => {
                let _ = self.bump();
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::String(tok.text.into_bytes()),
                })
            }
            TokenKind::Interpolate => {
                let _ = self.bump();
                let parts = split_interp(&tok.text, tok.span)?;
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Interpolate { parts },
                })
            }
            TokenKind::At => {
                let start = tok.span;
                let _ = self.bump();
                let name = self.ident()?;
                match name.name.as_str() {
                    INTRINSIC_SYSCALL => {
                        let args = self.args()?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::Syscall { args },
                        })
                    }
                    n if crate::ir::op::AtomicOp::from_name(n).is_some() => {
                        let op = crate::ir::op::AtomicOp::from_name(n).unwrap();
                        let mut args = self.args()?;
                        let order = self.atomic_order(op, &mut args, name.span)?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::Atomic { op, order, args },
                        })
                    }
                    n if INTRINSIC_CLOSURES.contains(&n) || INTRINSIC_SIMD.contains(&n) || INTRINSIC_GPU.contains(&n) => {
                        let args = self.args()?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::Call {
                                func: Box::new(Expr {
                                    span: name.span,
                                    kind: ExprKind::Name(Ident { name: format!("@{n}"), span: name.span }),
                                }),
                                type_args: Vec::new(),
                                args,
                            },
                        })
                    }
                    INTRINSIC_UI_CHOICE => {
                        self.expect(TokenKind::LParen, "expected `(` after `@uiChoice`")?;
                        let set = self.ty()?;
                        self.expect(TokenKind::Comma, "expected `,` after candidate-set type")?;
                        let value = Box::new(self.expr()?);
                        self.expect(TokenKind::Comma, "expected `,` after selector")?;
                        let index = Box::new(self.expr()?);
                        self.expect(TokenKind::RParen, "expected `)` after `@uiChoice`")?;
                        Ok(Expr { span: join(start, self.prev_end(start)), kind: ExprKind::UiChoice { set, value, index } })
                    }
                    INTRINSIC_SIZEOF => {
                        self.expect(TokenKind::LParen, "expected `(` after `@sizeof`")?;
                        let ty = self.ty()?;
                        self.expect(TokenKind::RParen, "expected `)` after `@sizeof`")?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::SizeOf(ty),
                        })
                    }
                    INTRINSIC_SLICE => {
                        let args = self.args()?;
                        if args.len() != 2 {
                            return Err(ParseError::new(
                                name.span,
                                "`@slice` takes a pointer and a length",
                            ));
                        }
                        let mut args = args.into_iter();
                        let ptr = args.next().unwrap();
                        let len = args.next().unwrap();
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::SliceFrom {
                                ptr: Box::new(ptr),
                                len: Box::new(len),
                            },
                        })
                    }
                    INTRINSIC_SCRIPT => {
                        self.expect(TokenKind::LParen, "expected `(` after `@script`")?;
                        let path = self.cur().clone();
                        if path.kind != TokenKind::String {
                            return Err(self.err("`@script` path must be a string literal"));
                        }
                        let _ = self.bump();
                        if !path.text.ends_with(".j3s") {
                            return Err(ParseError::new(
                                path.span,
                                "`@script` path must end in `.j3s`",
                            ));
                        }
                        self.expect(TokenKind::RParen, "expected `)` after `@script` path")?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::ScriptType(path.text),
                        })
                    }
                    other => Err(ParseError::new(
                        name.span,
                        format!("unknown intrinsic `@{other}`"),
                    )),
                }
            }
            TokenKind::LParen => {
                let _ = self.bump();
                let expr = self.expr()?;
                self.expect(TokenKind::RParen, "expected `)`")?;
                Ok(expr)
            }
            TokenKind::LBracket => {
                let start = tok.span;
                let _ = self.bump();
                if self.cur().kind == TokenKind::RBracket {
                    let _ = self.bump();
                    let elem = self.ty()?;
                    if !self.looks_like_struct_lit() {
                        return Err(self.err("expected `{ ptr, len }` after `[]T`"));
                    }
                    self.expect(TokenKind::LBrace, "expected `{` in slice literal")?;
                    let mut fields = Vec::new();
                    if self.cur().kind != TokenKind::RBrace {
                        loop {
                            let field = self.ident()?;
                            self.expect(TokenKind::Colon, "expected `:` after field name")?;
                            let value = self.expr()?;
                            fields.push((field, value));
                            if self.cur().kind == TokenKind::Comma {
                                let _ = self.bump();
                                if self.cur().kind == TokenKind::RBrace {
                                    break;
                                }
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace, "expected `}` after slice literal")?;
                    return Ok(Expr {
                        span: join(start, self.prev_end(start)),
                        kind: ExprKind::SliceLit { elem, fields },
                    });
                }
                let mut elems = Vec::new();
                if self.cur().kind != TokenKind::RBracket {
                    loop {
                        elems.push(self.expr()?);
                        if self.cur().kind == TokenKind::Comma {
                            let _ = self.bump();
                            if self.cur().kind == TokenKind::RBracket {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                self.expect(TokenKind::RBracket, "expected `]` after array literal")?;
                Ok(Expr {
                    span: join(start, self.prev_end(start)),
                    kind: ExprKind::ArrayLit { elems },
                })
            }
            TokenKind::If => self.if_expr(),
            _ => Err(self.err("expected expression")),
        }
    }

    pub(super) fn looks_like_struct_lit(&self) -> bool {
        if self.cur().kind != TokenKind::LBrace {
            return false;
        }
        match self.look(1).kind {
            TokenKind::RBrace => true,
            TokenKind::Ident | TokenKind::This => self.look(2).kind == TokenKind::Colon,
            _ => false,
        }
    }

    pub(super) fn turbofish_call(&mut self, func: Expr) -> Result<Expr, ParseError> {
        self.expect(TokenKind::ColonColon, "expected `::`")?;
        let type_args = self.type_args()?;
        if self.cur().kind != TokenKind::LParen {
            return Err(self.err("turbofish needs a call"));
        }
        let args = self.args()?;
        Ok(Expr {
            span: join(func.span, self.prev_end(func.span)),
            kind: ExprKind::Call {
                func: Box::new(func),
                type_args,
                args,
            },
        })
    }

    pub(super) fn struct_lit(&mut self, name: Ident, type_args: Vec<Type>) -> Result<Expr, ParseError> {
        self.expect(TokenKind::LBrace, "expected `{` in struct literal")?;
        let mut fields = Vec::new();
        if self.cur().kind != TokenKind::RBrace {
            loop {
                let field = self.ident()?;
                self.expect(TokenKind::Colon, "expected `:` after field name")?;
                let value = self.expr()?;
                fields.push((field, value));
                if self.cur().kind == TokenKind::Comma {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::RBrace {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RBrace, "expected `}` after struct literal")?;
        Ok(Expr {
            span: join(name.span, self.prev_end(name.span)),
            kind: ExprKind::StructLit {
                name,
                type_args,
                fields,
            },
        })
    }

    pub(super) fn script_lit_fields(&mut self) -> Result<Vec<(Ident, Expr)>, ParseError> {
        self.expect(TokenKind::LBrace, "expected `{` after `@script` type")?;
        let mut fields = Vec::new();
        if self.cur().kind != TokenKind::RBrace {
            loop {
                let field = self.ident()?;
                self.expect(TokenKind::Colon, "expected `:` after prop name")?;
                let value = self.expr()?;
                fields.push((field, value));
                if self.cur().kind == TokenKind::Comma {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::RBrace {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RBrace, "expected `}` after script literal")?;
        Ok(fields)
    }
}
