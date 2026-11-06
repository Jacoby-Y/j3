//! `extend Type { fn .. }`: methods of a struct declared elsewhere in the same module (a part file).
//! `extend` is a contextual word (an identifier followed by an identifier at item start), so a
//! variable called `extend` still parses. Merged into the struct by `parse::extend::merge`.

use super::*;
use crate::parse::ast::ExtendItem;

impl<'a> Parser<'a> {
    /// At `extend Name`: the word `extend` and then an identifier.
    pub(super) fn at_extend_item(&self) -> bool {
        self.cur().kind == TokenKind::Ident && self.cur().text == ITEM_EXTEND && self.look(1).kind == TokenKind::Ident
    }

    pub(super) fn extend_item(&mut self) -> Result<ExtendItem, ParseError> {
        let start = self.cur().span;
        let _ = self.bump();
        let name = self.ident()?;
        let (type_params, bounds) = self.type_params()?;
        if bounds.iter().any(|b| !b.is_empty()) {
            return Err(self.err("bounds go on the struct, not on `extend`"));
        }
        self.expect(TokenKind::LBrace, "expected `{` after the type in `extend`")?;
        let mut methods = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            let doc = self.take_doc();
            if self.cur().kind == TokenKind::Pub {
                return Err(self.err("methods are public already; drop `pub`"));
            }
            if self.cur().kind != TokenKind::Fn {
                return Err(self.err("expected `fn` in an `extend` body"));
            }
            methods.push(self.fn_item(false, doc)?);
        }
        self.expect(TokenKind::RBrace, "expected `}` after `extend` body")?;
        Ok(ExtendItem { span: join(start, self.prev_end(start)), name, type_params, methods })
    }
}
