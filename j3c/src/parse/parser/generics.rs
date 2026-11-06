//! Parser: type params and args, fn names, parameter lists.

use super::*;
use crate::parse::token::OVERLOADABLE;

impl<'a> Parser<'a> {
    /// `<K: Hash + Eq, V>`: the names and, at the same index, each one's bounds (see `traits.rs`).
    pub(super) fn type_params(&mut self) -> Result<(Vec<Ident>, Vec<Vec<Ident>>), ParseError> {
        if self.cur().kind != TokenKind::Lt {
            return Ok((Vec::new(), Vec::new()));
        }
        let _ = self.bump();
        let mut type_params = Vec::new();
        let mut bounds = Vec::new();
        loop {
            type_params.push(self.ident()?);
            bounds.push(self.bound_list()?);
            if self.cur().kind == TokenKind::Comma {
                let _ = self.bump();
                if self.cur().kind == TokenKind::Gt {
                    break;
                }
                continue;
            }
            break;
        }
        self.expect(TokenKind::Gt, "expected `>` after type parameters")?;
        Ok((type_params, bounds))
    }

    pub(super) fn type_args(&mut self) -> Result<Vec<Type>, ParseError> {
        self.expect(TokenKind::Lt, "expected `<`")?;
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
        Ok(args)
    }

    pub(super) fn fn_name(&mut self) -> Result<Ident, ParseError> {
        if self.cur().kind == TokenKind::Operator {
            let start = self.cur().span;
            let _ = self.bump();
            let op = match self.cur().kind.operator_src() {
                Some(o) if OVERLOADABLE.contains(&o) => o,
                _ => {
                    return Err(self.err(&format!(
                        "expected an overloadable operator after `operator`: {}",
                        OVERLOADABLE.join(" ")
                    )))
                }
            };
            let end = self.cur().span;
            let _ = self.bump();
            return Ok(Ident {
                name: format!("operator{op}"),
                span: join(start, end),
            });
        }
        self.ident()
    }

    pub(super) fn param_list(&mut self) -> Result<Vec<Param>, ParseError> {
        if self.cur().kind == TokenKind::Dot {
            return Err(self.err("methods are declared inside the struct body (`struct T { fn name(this: &T, ...) { ... } }`), not as `fn T.name(`; see `j3c doc lang`"));
        }
        self.expect(TokenKind::LParen, "expected `(` after fn name")?;
        let mut params = Vec::new();
        if self.cur().kind != TokenKind::RParen {
            loop {
                let pname = self.ident()?;
                self.expect(TokenKind::Colon, "expected `:` after parameter name")?;
                let pty = self.ty()?;
                params.push(Param {
                    name: pname,
                    ty: pty,
                });
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
        self.expect(TokenKind::RParen, "expected `)` after parameters")?;
        Ok(params)
    }
}
