//! Parser: types (`&T`, `[N]T`, `[]T`, `fn(..): R`, `closure(..)`, `dyn Trait`, `Name<..>`, `(A, B)`).

use super::*;

impl<'a> Parser<'a> {
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
            let len = match parse_int(&ntok.text).map_err(|m| ParseError::new(ntok.span, m))? {
                ExprKind::Int(v) => v,
                _ => return Err(ParseError::new(ntok.span, "array length is too large")),
            };
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
        if self.cur().kind == TokenKind::LParen {
            let _ = self.bump();
            let mut elems = vec![self.ty()?];
            while self.cur().kind == TokenKind::Comma && self.look(1).kind != TokenKind::RParen {
                let _ = self.bump();
                elems.push(self.ty()?);
            }
            if self.cur().kind == TokenKind::Comma {
                let _ = self.bump();
            }
            if elems.len() < 2 {
                return Err(ParseError::new(start, "a tuple type has two or more elements"));
            }
            self.expect(TokenKind::RParen, "expected `,` or `)` in a tuple type")?;
            return Ok(Type { span: join(start, self.prev_end(start)), kind: TypeKind::Tuple(elems) });
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
        if name.name == "closure" && self.cur().kind == TokenKind::LParen {
            return self.closure_type(name.span, false);
        }
        if name.name == TYPE_BOXED && self.cur().kind == TokenKind::Ident && self.cur().text == TYPE_CLOSURE {
            let _ = self.bump();
            return self.closure_type(name.span, true);
        }
        if name.name == TYPE_DYN && self.cur().kind == TokenKind::Ident {
            let tr = self.ident()?;
            if !self.dyns.contains(&tr.name) {
                self.dyns.push(tr.name.clone());
            }
            return Ok(Type { span: join(start, tr.span), kind: TypeKind::Dyn(tr) });
        }
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
