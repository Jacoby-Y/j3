//! Traits: `trait Hash { fn hash(this: Self): u64; }`, supertraits `trait Ord: Eq { .. }`, default
//! bodies `fn ne(this: Self, o: Self): bool { return !(this == o); }`, and the bounds `<T: Hash + Eq>`.
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

    /// `where T: Eq, U: Hash + Eq` before a fn body, or nothing. `where` counts only when
    /// `name :` follows, so a variable called `where` still parses.
    pub(super) fn where_clause(&mut self) -> Result<Vec<(Ident, Vec<Ident>)>, ParseError> {
        let mut out: Vec<(Ident, Vec<Ident>)> = Vec::new();
        let at = |p: &Self| {
            p.cur().kind == TokenKind::Ident
                && p.cur().text == FN_WHERE
                && p.look(1).kind == TokenKind::Ident
                && p.look(2).kind == TokenKind::Colon
        };
        if !at(self) {
            return Ok(out);
        }
        let _ = self.bump();
        loop {
            let p = self.ident()?;
            if out.iter().any(|(o, _)| o.name == p.name) {
                return Err(ParseError::new(p.span, format!("`{}` is listed twice in `where`; join the bounds with `+`", p.name)));
            }
            let b = self.bound_list()?;
            out.push((p, b));
            if self.cur().kind != TokenKind::Comma {
                break;
            }
            let _ = self.bump();
        }
        Ok(out)
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
        // `trait Ord: Eq + Hash`
        let supers = self.bound_list()?;
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
            let sig_end = self.prev_end(mstart);
            let body = if self.cur().kind == TokenKind::LBrace {
                Some(self.block()?)
            } else {
                self.expect(TokenKind::Semi, "expected `;` or a default body `{ .. }` after a trait method signature")?;
                None
            };
            methods.push(TraitMethod {
                span: join(mstart, sig_end),
                name: mname,
                params,
                ret,
                body,
            });
        }
        self.expect(TokenKind::RBrace, "expected `}` after trait")?;
        Ok(TraitItem {
            span: join(start, self.prev_end(start)),
            doc,
            vis_pub,
            name,
            supers,
            methods,
        })
    }
}
