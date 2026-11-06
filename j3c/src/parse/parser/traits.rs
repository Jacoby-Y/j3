//! Traits: `trait Hash { fn hash(this: Self): u64; }` and the bounds `<T: Hash + Eq>`.
//! `trait` is a contextual word (an identifier followed by an identifier at item start), so
//! a variable called `trait` still parses. See `speculative/lang/traits.j3`.

use super::*;

impl<'a> Parser<'a> {
    /// At `trait Name`: the word `trait` and then an identifier.
    pub(super) fn at_trait_item(&self) -> bool {
        self.cur().kind == TokenKind::Ident
            && self.cur().text == ITEM_TRAIT
            && self.look(1).kind == TokenKind::Ident
    }

    /// After a type parameter's name: `: A + B`, or nothing.
    pub(super) fn bound_list(&mut self) -> Result<Vec<Ident>, ParseError> {
        let mut out: Vec<Ident> = Vec::new();
        if self.cur().kind != TokenKind::Colon {
            return Ok(out);
        }
        let _ = self.bump();
        loop {
            let b = self.ident()?;
            if out.iter().any(|o| o.name == b.name) {
                return Err(ParseError::new(b.span, format!("bound `{}` is listed twice", b.name)));
            }
            out.push(b);
            if self.cur().kind != TokenKind::Plus {
                break;
            }
            let _ = self.bump();
        }
        Ok(out)
    }

    pub(super) fn trait_item(&mut self, vis_pub: bool, doc: String) -> Result<TraitItem, ParseError> {
        let start = self.cur().span;
        let _ = self.bump();
        let name = self.ident()?;
        if self.cur().kind == TokenKind::Lt {
            return Err(self.err("traits take no type parameters"));
        }
        self.expect(TokenKind::LBrace, "expected `{` after trait name")?;
        let mut methods = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            let mdoc = self.take_doc();
            let _ = mdoc; // ponytail: trait method docs are dropped; the trait's own doc is kept
            let mstart = self.cur().span;
            self.expect(TokenKind::Fn, "expected `fn` in a trait body")?;
            let mname = self.fn_name()?;
            if self.cur().kind == TokenKind::Lt {
                return Err(self.err("trait methods take no type parameters"));
            }
            let params = self.param_list()?;
            let mut ret = None;
            if self.cur().kind == TokenKind::Colon {
                let _ = self.bump();
                ret = Some(self.ty()?);
            }
            if self.cur().kind == TokenKind::LBrace {
                return Err(self.err("a trait method is a signature ending in `;`; traits have no default bodies"));
            }
            self.expect(TokenKind::Semi, "expected `;` after a trait method signature")?;
            methods.push(TraitMethod {
                span: join(mstart, self.prev_end(mstart)),
                name: mname,
                params,
                ret,
            });
        }
        self.expect(TokenKind::RBrace, "expected `}` after trait")?;
        Ok(TraitItem {
            span: join(start, self.prev_end(start)),
            doc,
            vis_pub,
            name,
            methods,
        })
    }
}
