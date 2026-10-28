use super::*;

impl<'a> Parser<'a> {
    pub(super) fn file(&mut self) -> Result<File, ParseError> {
        let start = self.cur().span;
        let doc = self.take_doc();
        let module = if let Some(parts) = &self.script_module {
            if self.cur().kind == TokenKind::Module {
                return Err(self.err("scripts can't have a `module` line; the path names it"));
            }
            parts
                .iter()
                .map(|name| Ident {
                    name: name.clone(),
                    span: start,
                })
                .collect()
        } else {
            self.expect(TokenKind::Module, "expected `module`")?;
            let module = self.path()?;
            self.expect(TokenKind::Semi, "expected `;` after module path")?;
            module
        };
        let mut items = Vec::new();
        while self.cur().kind != TokenKind::Eof {
            items.push(self.item()?);
        }
        let end = self.prev_end(start);
        Ok(File {
            span: join(start, end),
            doc,
            module,
            is_script: self.script_module.is_some(),
            items,
        })
    }

    pub(super) fn item(&mut self) -> Result<Item, ParseError> {
        if self.script_module.is_some() {
            return self.script_item();
        }
        let doc = self.take_doc();
        let attrs = self.attrs()?;
        let vis_pub = if self.cur().kind == TokenKind::Pub {
            let _ = self.bump();
            true
        } else {
            false
        };
        match self.cur().kind {
            TokenKind::Use => {
                self.forbid_doc(&doc)?;
                self.forbid_attrs(&attrs, "use")?;
                if self.look(1).kind == TokenKind::Lib {
                    if vis_pub {
                        return Err(self.err("`pub use lib` is not valid"));
                    }
                    Ok(Item::UseLib(self.use_lib_item()?))
                } else {
                    if vis_pub {
                        return Err(self.err("`pub use` is not valid"));
                    }
                    Ok(Item::Use(self.use_item()?))
                }
            }
            TokenKind::Const => {
                self.forbid_attrs(&attrs, "const")?;
                Ok(Item::Const(self.const_item(vis_pub, doc)?))
            }
            TokenKind::Fn => {
                self.forbid_attrs(&attrs, "fn")?;
                Ok(Item::Fn(self.fn_item(vis_pub, doc)?))
            }
            TokenKind::At => {
                self.forbid_attrs(&attrs, "@j3ui")?;
                Ok(Item::Fn(self.j3ui_item(vis_pub, doc)?))
            }
            TokenKind::Struct => {
                if attrs.try_catch.is_some() {
                    return Err(self.err("`#[TryCatch]` cannot sit on `struct`"));
                }
                Ok(Item::Struct(self.struct_item(vis_pub, attrs.debug, doc)?))
            }
            TokenKind::Enum => Ok(Item::Enum(self.enum_item(vis_pub, attrs, doc)?)),
            TokenKind::Let => {
                if !attrs.singleton {
                    return Err(self.err("module `let` needs `#[singleton]`"));
                }
                if attrs.debug || attrs.try_catch.is_some() {
                    return Err(
                        self.err("`#[singleton]` cannot mix with `#[Debug]` / `#[TryCatch]`")
                    );
                }
                if vis_pub {
                    return Err(self.err("`pub` on singleton `let` is not used"));
                }
                Ok(Item::Let(self.mod_let_item(doc, attrs.thread)?))
            }
            TokenKind::If | TokenKind::While | TokenKind::For => {
                self.forbid_doc(&doc)?;
                Err(self.err("control items cannot appear at module scope"))
            }
            _ => {
                self.forbid_doc(&doc)?;
                Err(self.err("expected `use`, `const`, `fn`, `struct`, or `let`"))
            }
        }
    }

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
            singleton: false,
            thread: false,
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
                ATTR_TRY_CATCH => {
                    self.expect(TokenKind::LParen, "expected `(` after `#[TryCatch`")?;
                    let success = self.ident()?;
                    self.expect(TokenKind::Comma, "expected `,` in `#[TryCatch]`")?;
                    let failure = self.ident()?;
                    self.expect(TokenKind::RParen, "expected `)` after `#[TryCatch]`")?;
                    out.try_catch = Some((success, failure));
                }
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

    pub(super) fn forbid_attrs(&self, attrs: &ItemAttrs, what: &str) -> Result<(), ParseError> {
        if attrs.debug {
            return Err(self.err(&format!("`#[Debug]` cannot sit on `{what}`")));
        }
        if attrs.try_catch.is_some() {
            return Err(self.err(&format!("`#[TryCatch]` cannot sit on `{what}`")));
        }
        if attrs.singleton {
            return Err(self.err(&format!("`#[singleton]` cannot sit on `{what}`")));
        }
        if attrs.thread {
            return Err(self.err(&format!("`#[thread]` cannot sit on `{what}`")));
        }
        Ok(())
    }

    pub(super) fn mod_let_item(&mut self, doc: String, thread: bool) -> Result<ModLetItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Let, "expected `let`")?;
        let name = self.ident()?;
        self.expect(TokenKind::Colon, "expected `:` after singleton name")?;
        let ty = self.ty()?;
        let value = if self.cur().kind == TokenKind::Eq {
            let _ = self.bump();
            Some(self.expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Semi, "expected `;` after singleton `let`")?;
        Ok(ModLetItem {
            span: join(start, self.prev_end(start)),
            doc,
            name,
            ty,
            value,
            thread,
        })
    }

    pub(super) fn use_lib_item(&mut self) -> Result<UseLibItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Use, "expected `use`")?;
        self.expect(TokenKind::Lib, "expected `lib`")?;
        let lib_tok = self.cur().clone();
        if lib_tok.kind != TokenKind::String {
            return Err(self.err("expected library name string"));
        }
        let _ = self.bump();
        self.expect(TokenKind::As, "expected `as` after `use lib`")?;
        let alias = self.ident()?;
        self.expect(TokenKind::LBrace, "expected `{` after `use lib` alias")?;
        let mut decls = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            decls.push(self.lib_fn_decl()?);
        }
        self.expect(TokenKind::RBrace, "expected `}` after `use lib`")?;
        Ok(UseLibItem {
            span: join(start, self.prev_end(start)),
            lib: lib_tok.text,
            alias,
            decls,
        })
    }

    pub(super) fn lib_fn_decl(&mut self) -> Result<LibFnDecl, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Fn, "expected `fn` in `use lib`")?;
        let name = self.ident()?;
        let params = self.param_list()?;
        let mut ret = None;
        if self.cur().kind == TokenKind::Colon {
            let _ = self.bump();
            ret = Some(self.ty()?);
        }
        let mut symbol = None;
        if self.cur().kind == TokenKind::Eq {
            let _ = self.bump();
            let tok = self.cur().clone();
            if tok.kind != TokenKind::String {
                return Err(self.err("expected symbol string after `=`"));
            }
            let _ = self.bump();
            symbol = Some(tok.text);
        }
        self.expect(TokenKind::Semi, "expected `;` after lib fn")?;
        Ok(LibFnDecl {
            span: join(start, self.prev_end(start)),
            name,
            params,
            ret,
            symbol,
        })
    }

    pub(super) fn struct_item(&mut self, vis_pub: bool, debug: bool, doc: String) -> Result<StructItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Struct, "expected `struct`")?;
        let name = self.ident()?;
        let type_params = self.type_params()?;
        if self.cur().kind == TokenKind::Semi {
            let _ = self.bump();
            return Ok(StructItem {
                span: join(start, self.prev_end(start)),
                doc,
                vis_pub,
                debug,
                packed: false,
                name,
                type_params,
                fields: Vec::new(),
                methods: Vec::new(),
            });
        }
        self.expect(TokenKind::LBrace, "expected `{` after struct name")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            let fdoc = self.take_doc();
            if self.cur().kind == TokenKind::RBrace || self.cur().kind == TokenKind::Eof {
                self.forbid_doc(&fdoc)?;
                break;
            }
            if self.cur().kind == TokenKind::Fn {
                methods.push(self.fn_item(false, fdoc)?);
                continue;
            }
            let embedded = if self.cur().kind == TokenKind::Use {
                let _ = self.bump();
                true
            } else {
                false
            };
            let fname = self.ident()?;
            self.expect(TokenKind::Colon, "expected `:` after field name")?;
            let fty = self.ty()?;
            let default = if self.cur().kind == TokenKind::Eq {
                let _ = self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(TokenKind::Semi, "expected `;` after field")?;
            let end_span = default.as_ref().map(|e| e.span).unwrap_or(fty.span);
            fields.push(FieldItem {
                span: join(fname.span, end_span),
                doc: fdoc,
                name: fname,
                ty: fty,
                embedded,
                default,
            });
        }
        self.expect(TokenKind::RBrace, "expected `}` after struct")?;
        Ok(StructItem {
            span: join(start, self.prev_end(start)),
            doc,
            vis_pub,
            debug,
            packed: false,
            name,
            type_params,
            fields,
            methods,
        })
    }

    pub(super) fn enum_item(&mut self, vis_pub: bool, attrs: ItemAttrs, doc: String) -> Result<EnumItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Enum, "expected `enum`")?;
        let name = self.ident()?;
        let mut type_params = Vec::new();
        if self.cur().kind == TokenKind::Lt {
            let _ = self.bump();
            loop {
                type_params.push(self.ident()?);
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
        }
        self.expect(TokenKind::LBrace, "expected `{` after enum name")?;
        let mut variants = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            let vdoc = self.take_doc();
            if self.cur().kind == TokenKind::RBrace || self.cur().kind == TokenKind::Eof {
                self.forbid_doc(&vdoc)?;
                break;
            }
            let vname = self.ident()?;
            let mut payload = None;
            if self.cur().kind == TokenKind::LParen {
                let _ = self.bump();
                payload = Some(self.ty()?);
                self.expect(TokenKind::RParen, "expected `)` after variant payload")?;
            }
            variants.push(VariantItem {
                span: vname.span,
                doc: vdoc,
                name: vname,
                payload,
            });
            if self.cur().kind == TokenKind::Comma {
                let _ = self.bump();
                continue;
            }
            break;
        }
        self.expect(TokenKind::RBrace, "expected `}` after enum")?;
        Ok(EnumItem {
            span: join(start, self.prev_end(start)),
            doc,
            vis_pub,
            debug: attrs.debug,
            try_catch: attrs.try_catch,
            name,
            type_params,
            variants,
        })
    }

    pub(super) fn use_item(&mut self) -> Result<UseItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Use, "expected `use`")?;
        let path = self.path()?;
        let mut names = None;
        if self.cur().kind == TokenKind::Dot && self.look(1).kind == TokenKind::LBrace {
            let _ = self.bump();
            let _ = self.bump();
            let mut list = Vec::new();
            loop {
                list.push(self.ident()?);
                if self.cur().kind == TokenKind::Comma {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::RBrace {
                        break;
                    }
                    continue;
                }
                break;
            }
            self.expect(TokenKind::RBrace, "expected `}` in use list")?;
            names = Some(list);
        }
        self.expect(TokenKind::Semi, "expected `;` after use")?;
        if names.is_none() && path.is_empty() {
            return Err(ParseError::new(start, "empty use path"));
        }
        Ok(UseItem {
            span: join(start, self.prev_end(start)),
            path,
            names,
        })
    }

    pub(super) fn const_item(&mut self, vis_pub: bool, doc: String) -> Result<ConstItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Const, "expected `const`")?;
        let name = self.ident()?;
        let mut ty = None;
        if self.cur().kind == TokenKind::Colon {
            let _ = self.bump();
            ty = Some(self.ty()?);
        }
        self.expect(TokenKind::Eq, "expected `=` in const")?;
        let value = self.expr()?;
        self.expect(TokenKind::Semi, "expected `;` after const")?;
        Ok(ConstItem {
            span: join(start, self.prev_end(start)),
            doc,
            vis_pub,
            name,
            ty,
            value,
        })
    }

    pub(super) fn fn_item(&mut self, vis_pub: bool, doc: String) -> Result<FnItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Fn, "expected `fn`")?;
        let name = self.fn_name()?;
        let type_params = self.type_params()?;
        let params = self.param_list()?;
        let mut ret = None;
        if self.cur().kind == TokenKind::Colon {
            let _ = self.bump();
            ret = Some(self.ty()?);
        }
        let body = self.block()?;
        Ok(FnItem {
            span: join(start, body.span),
            doc,
            vis_pub,
            name,
            type_params,
            params,
            ret,
            body,
        })
    }

    pub(super) fn type_params(&mut self) -> Result<Vec<Ident>, ParseError> {
        if self.cur().kind != TokenKind::Lt {
            return Ok(Vec::new());
        }
        let _ = self.bump();
        let mut type_params = Vec::new();
        loop {
            type_params.push(self.ident()?);
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
        Ok(type_params)
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
            let op = match self.cur().kind {
                TokenKind::Plus => "+",
                TokenKind::Minus => "-",
                TokenKind::Star => "*",
                TokenKind::Slash => "/",
                TokenKind::EqEq => "==",
                TokenKind::Ne => "!=",
                TokenKind::Lt => "<",
                TokenKind::Le => "<=",
                TokenKind::Gt => ">",
                TokenKind::Ge => ">=",
                _ => return Err(self.err("expected an operator after `operator`")),
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
