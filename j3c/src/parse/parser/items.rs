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
            self.expect(TokenKind::Module, "expected `module`: a file starts with `module <name>;` (its file name, e.g. `module main;`)")?;
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
        self.gpu_attr_site(&attrs)?;
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
                let mut f = self.fn_item(vis_pub, doc)?;
                f.is_impl = attrs.impl_fn;
                f.shader = attrs.shader;
                Ok(Item::Fn(f))
            }
            TokenKind::Ident if self.at_trait_item() => {
                self.forbid_attrs(&attrs, "trait")?;
                Ok(Item::Trait(self.trait_item(vis_pub, doc)?))
            }
            TokenKind::Ident if self.at_extend_item() => {
                self.forbid_attrs(&attrs, "extend")?;
                // A doc before `extend` is a part file's header (`ui/dom/*.j3` style); it documents nothing, so drop it.
                let _ = doc;
                if vis_pub {
                    return Err(self.err("`pub extend` is not valid; the methods are public like in a struct body"));
                }
                Ok(Item::Extend(self.extend_item()?))
            }
            TokenKind::At => {
                self.forbid_attrs(&attrs, "@j3ui")?;
                Ok(Item::Fn(self.j3ui_item(vis_pub, doc)?))
            }
            TokenKind::Struct => {
                if attrs.impl_fn {
                    return Err(self.err("`#[impl]` cannot sit on `struct`; it marks a free fn"));
                }
                if attrs.try_catch.is_some() {
                    return Err(self.err("`#[TryCatch]` cannot sit on `struct`"));
                }
                let mut s = self.struct_item(vis_pub, attrs.debug, doc)?;
                s.gpu = match (attrs.gpu_struct, attrs.shader) {
                    (GpuStruct::None, Some(_)) => GpuStruct::Plain,
                    (g, _) => g,
                };
                Ok(Item::Struct(s))
            }
            TokenKind::Enum => {
                if attrs.impl_fn {
                    return Err(self.err("`#[impl]` cannot sit on `enum`; it marks a free fn"));
                }
                Ok(Item::Enum(self.enum_item(vis_pub, attrs, doc)?))
            }
            TokenKind::Let => {
                if attrs.uniform {
                    let mut l = self.mod_let_item(doc, false)?;
                    if l.value.is_some() {
                        return Err(ParseError::new(l.name.span, "a `#[uniform]` has no initializer; the CPU sets it by name"));
                    }
                    l.uniform = true;
                    return Ok(Item::Let(l));
                }
                if !attrs.singleton {
                    return Err(self.err("module `let` needs `#[singleton]`"));
                }
                if attrs.debug || attrs.try_catch.is_some() || attrs.impl_fn {
                    return Err(
                        self.err("`#[singleton]` cannot mix with `#[Debug]` / `#[TryCatch]` / `#[impl]`")
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
            uniform: false,
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
        let (type_params, bounds) = self.type_params()?;
        if self.cur().kind == TokenKind::Semi {
            let _ = self.bump();
            return Ok(StructItem {
                span: join(start, self.prev_end(start)),
                doc,
                vis_pub,
                debug,
                packed: false,
                gpu: GpuStruct::None,
                name,
                type_params,
                bounds,
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
            let flat = self.field_attrs()?;
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
                flat,
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
            gpu: GpuStruct::None,
            name,
            type_params,
            bounds,
            fields,
            methods,
        })
    }

    pub(super) fn enum_item(&mut self, vis_pub: bool, attrs: ItemAttrs, doc: String) -> Result<EnumItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Enum, "expected `enum`")?;
        let name = self.ident()?;
        let (type_params, bounds) = self.type_params()?;
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
            bounds,
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
        let (type_params, bounds) = self.type_params()?;
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
            bounds,
            is_impl: false,
            shader: None,
            params,
            ret,
            body,
        })
    }
}
