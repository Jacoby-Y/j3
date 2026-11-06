//! Attributes for shader code: `#[shader]`, `#[shader(vertex|fragment)]`, `#[vertex]`, `#[vertex(instance)]`,
//! `#[varying]`, `#[uniform]` on items and `#[flat]` on fields (`j3c doc shaders`).

use super::*;

impl<'a> Parser<'a> {
    /// Parse one of the shader attributes (the name is already read). `Ok(false)` when `name` is not one.
    pub(super) fn gpu_attr(&mut self, name: &str, id: &Ident, out: &mut ItemAttrs) -> Result<bool, ParseError> {
        let arg = |p: &mut Self| -> Result<Option<Ident>, ParseError> {
            if p.cur().kind != TokenKind::LParen {
                return Ok(None);
            }
            let _ = p.bump();
            let a = p.ident()?;
            p.expect(TokenKind::RParen, "expected `)` after the attribute argument")?;
            Ok(Some(a))
        };
        match name {
            "shader" => {
                out.shader = Some(match arg(self)? {
                    None => GpuStage::Helper,
                    Some(a) if a.name == "vertex" => GpuStage::Vertex,
                    Some(a) if a.name == "fragment" => GpuStage::Fragment,
                    Some(a) => {
                        return Err(ParseError::new(a.span, "`#[shader(..)]` takes `vertex` or `fragment`; a helper is plain `#[shader]`"))
                    }
                });
            }
            "vertex" => {
                out.gpu_struct = match arg(self)? {
                    None => GpuStruct::Vertex,
                    Some(a) if a.name == "instance" => GpuStruct::VertexInstance,
                    Some(a) => return Err(ParseError::new(a.span, "`#[vertex(..)]` takes `instance`")),
                };
            }
            "varying" | "uniform" | "flat" => {
                if arg(self)?.is_some() {
                    return Err(ParseError::new(id.span, format!("`#[{name}]` takes no arguments")));
                }
                match name {
                    "varying" => out.gpu_struct = GpuStruct::Varying,
                    "uniform" => out.uniform = true,
                    _ => out.flat = true,
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// Reject shader attributes on an item they do not fit (`#[shader]` on a `const`, `#[uniform]` on a fn, ...).
    pub(super) fn gpu_attr_site(&self, a: &ItemAttrs) -> Result<(), ParseError> {
        let next = self.cur().kind;
        let bad = |what: &str, ok: &str| Err(self.err(&format!("{what} only sits on {ok}")));
        if a.flat {
            return bad("`#[flat]`", "a field of a `#[varying]` struct");
        }
        if a.uniform && next != TokenKind::Let {
            return bad("`#[uniform]`", "a module `let`");
        }
        if a.gpu_struct != GpuStruct::None && next != TokenKind::Struct {
            return bad("`#[vertex]` and `#[varying]`", "a `struct`");
        }
        if let Some(s) = a.shader {
            let fits = match next {
                TokenKind::Fn => true,
                TokenKind::Struct => s == GpuStage::Helper,
                _ => false,
            };
            if !fits {
                return bad("`#[shader]`", "a `fn` (or, bare, a plain GPU `struct`)");
            }
        }
        Ok(())
    }

    /// `#[flat]` before a struct field.
    pub(super) fn field_attrs(&mut self) -> Result<bool, ParseError> {
        let mut flat = false;
        while self.cur().kind == TokenKind::Hash {
            let _ = self.bump();
            self.expect(TokenKind::LBracket, "expected `[` after `#`")?;
            let name = self.ident()?;
            if name.name != "flat" {
                return Err(ParseError::new(name.span, format!("unknown field attribute `#[{}]`; fields take `#[flat]`", name.name)));
            }
            flat = true;
            self.expect(TokenKind::RBracket, "expected `]` after attribute")?;
        }
        Ok(flat)
    }
}
