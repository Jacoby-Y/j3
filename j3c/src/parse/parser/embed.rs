//! `@embed("p")`, `@embedBytes("p")`, `@embedDir("dir", "glob")`: string-literal arguments only.

use super::*;
use crate::parse::EmbedKind;

impl<'a> Parser<'a> {
    pub(super) fn embed(&mut self, name: &str, start: Span) -> Result<Expr, ParseError> {
        let kind = match name {
            "embed" => EmbedKind::Text,
            "embedBytes" => EmbedKind::Bytes,
            _ => EmbedKind::Dir,
        };
        self.expect(TokenKind::LParen, &format!("expected `(` after `@{name}`"))?;
        let path = self.embed_arg(name, "path")?;
        let glob = if kind == EmbedKind::Dir {
            self.expect(TokenKind::Comma, "expected `,` and a file pattern after the `@embedDir` folder")?;
            self.embed_arg(name, "pattern")?
        } else {
            String::new()
        };
        self.expect(TokenKind::RParen, &format!("expected `)` after `@{name}` arguments"))?;
        Ok(Expr { span: join(start, self.prev_end(start)), kind: ExprKind::Embed { kind, path, glob } })
    }

    fn embed_arg(&mut self, name: &str, what: &str) -> Result<String, ParseError> {
        let tok = self.cur().clone();
        if tok.kind != TokenKind::String {
            return Err(self.err(&format!("`@{name}` {what} must be a string literal")));
        }
        let _ = self.bump();
        if tok.text.is_empty() {
            return Err(ParseError::new(tok.span, format!("`@{name}` {what} is empty")));
        }
        Ok(tok.text)
    }
}
