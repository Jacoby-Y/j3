//! Parser: doc comments and `#[...]` attributes before an item.

use super::*;

impl<'a> Parser<'a> {
    pub(super) fn take_doc(&mut self) -> String {
        let mut lines = Vec::new();
        while self.cur().kind == TokenKind::Doc {
            lines.push(self.cur().text.clone());
            let _ = self.bump();
        }
        lines.join("\n")
    }

    pub(super) fn forbid_doc(&self, doc: &str) -> Result<(), ParseError> {
        if doc.is_empty() {
            Ok(())
        } else {
            Err(self.err("doc comment must sit on a fn, struct, enum, const, let, field, or variant"))
        }
    }

    pub(super) fn attrs(&mut self) -> Result<ItemAttrs, ParseError> {
        let mut out = ItemAttrs {
            debug: false,
            try_catch: None,
            derives: Vec::new(),
            singleton: false,
            thread: false,
            impl_fn: false,
            shader: None,
            gpu_struct: GpuStruct::None,
            uniform: false,
            flat: false,
        };
        while self.cur().kind == TokenKind::Hash {
            let _ = self.bump();
            self.expect(TokenKind::LBracket, "expected `[` after `#`")?;
            let name = self.ident()?;
            match name.name.as_str() {
                ATTR_DEBUG => {
                    if self.cur().kind == TokenKind::LParen {
                        return Err(ParseError::new(name.span, "`#[Debug]` takes no arguments"));
                    }
                    out.debug = true;
                }
                ATTR_DERIVE => self.derive_list(&mut out)?,
                ATTR_SINGLETON => {
                    if self.cur().kind == TokenKind::LParen {
                        return Err(ParseError::new(
                            name.span,
                            "`#[singleton]` takes no arguments",
                        ));
                    }
                    out.singleton = true;
                }
                ATTR_THREAD => {
                    if self.cur().kind == TokenKind::LParen {
                        return Err(ParseError::new(name.span, "`#[thread]` takes no arguments"));
                    }
                    out.thread = true;
                }
                ATTR_IMPL => {
                    if self.cur().kind == TokenKind::LParen {
                        return Err(ParseError::new(name.span, "`#[impl]` takes no arguments"));
                    }
                    out.impl_fn = true;
                }
                ATTR_TRY_CATCH => {
                    self.expect(TokenKind::LParen, "expected `(` after `#[TryCatch`")?;
                    let success = self.ident()?;
                    self.expect(TokenKind::Comma, "expected `,` in `#[TryCatch]`")?;
                    let failure = self.ident()?;
                    self.expect(TokenKind::RParen, "expected `)` after `#[TryCatch]`")?;
                    out.try_catch = Some((success, failure));
                }
                n if self.gpu_attr(n, &name, &mut out)? => {}
                other => {
                    return Err(ParseError::new(
                        name.span,
                        format!("unknown attribute `#[{other}]`"),
                    ));
                }
            }
            self.expect(TokenKind::RBracket, "expected `]` after attribute")?;
        }
        Ok(out)
    }

    /// `(Eq, Hash, ..)` after `#[derive`. `Debug` is the same as `#[Debug]`.
    fn derive_list(&mut self, out: &mut ItemAttrs) -> Result<(), ParseError> {
        self.expect(TokenKind::LParen, "expected `(` after `#[derive`")?;
        loop {
            let d = self.ident()?;
            if !DERIVES.contains(&d.name.as_str()) {
                return Err(ParseError::new(d.span, format!("cannot derive `{}`; derivable: {}", d.name, DERIVES.join(", "))));
            }
            if out.derives.iter().any(|o| o.name == d.name) || (d.name == "Debug" && out.debug) {
                return Err(ParseError::new(d.span, format!("`{}` is derived twice", d.name)));
            }
            if d.name == "Debug" {
                out.debug = true;
            } else {
                out.derives.push(d);
            }
            if self.cur().kind != TokenKind::Comma {
                break;
            }
            let _ = self.bump();
        }
        self.expect(TokenKind::RParen, "expected `)` after the derive list")?;
        Ok(())
    }

    pub(super) fn forbid_attrs(&self, attrs: &ItemAttrs, what: &str) -> Result<(), ParseError> {
        if attrs.debug {
            return Err(self.err(&format!("`#[Debug]` cannot sit on `{what}`")));
        }
        if attrs.try_catch.is_some() {
            return Err(self.err(&format!("`#[TryCatch]` cannot sit on `{what}`")));
        }
        if !attrs.derives.is_empty() {
            return Err(self.err(&format!("`#[derive]` cannot sit on `{what}`; it marks a struct or enum")));
        }
        if attrs.singleton {
            return Err(self.err(&format!("`#[singleton]` cannot sit on `{what}`")));
        }
        if attrs.thread {
            return Err(self.err(&format!("`#[thread]` cannot sit on `{what}`")));
        }
        if attrs.impl_fn && what != "fn" {
            return Err(self.err(&format!("`#[impl]` cannot sit on `{what}`; it marks a free fn")));
        }
        Ok(())
    }
}
