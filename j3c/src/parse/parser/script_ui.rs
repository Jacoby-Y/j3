use super::*;

impl<'a> Parser<'a> {
    pub(super) fn script_item(&mut self) -> Result<Item, ParseError> {
        let doc = self.take_doc();
        if self.cur().kind == TokenKind::Module {
            return Err(self.err("scripts can't have a `module` line; the path names it"));
        }
        let vis_pub = if self.cur().kind == TokenKind::Pub {
            let _ = self.bump();
            true
        } else {
            false
        };
        match self.cur().kind {
            TokenKind::Use if self.look(1).kind == TokenKind::Lib => {
                Err(self.err("scripts can't use `use lib`; import a J3 wrapper"))
            }
            TokenKind::Use if !vis_pub => {
                self.forbid_doc(&doc)?;
                Ok(Item::Use(self.use_item()?))
            }
            TokenKind::Const if !vis_pub => Ok(Item::Const(self.const_item(false, doc)?)),
            TokenKind::Fn => Ok(Item::Fn(self.fn_item(vis_pub, doc)?)),
            TokenKind::Struct => Ok(Item::Struct(self.struct_item(vis_pub, false, doc)?)),
            TokenKind::Enum => Ok(Item::Enum(self.enum_item(
                vis_pub,
                ItemAttrs { debug: false, try_catch: None, singleton: false, thread: false, impl_fn: false, shader: None, gpu_struct: GpuStruct::None, uniform: false, flat: false },
                doc,
            )?)),
            TokenKind::Ident if !vis_pub && self.cur().text == SCRIPT_PROPS => {
                self.forbid_doc(&doc)?;
                Ok(Item::Props(self.script_block(SCRIPT_PROPS)?))
            }
            TokenKind::Ident if !vis_pub && self.cur().text == SCRIPT_SHARED => {
                self.forbid_doc(&doc)?;
                Ok(Item::Shared(self.script_block(SCRIPT_SHARED)?))
            }
            TokenKind::Ident if self.at_extend_item() => {
                Err(self.err("scripts can't use `extend`; put the methods in the struct body"))
            }
            TokenKind::Ident if self.at_trait_item() => {
                Err(self.err("traits live in .j3 modules; scripts cannot declare them"))
            }
            TokenKind::Let if !vis_pub => {
                self.forbid_doc(&doc)?;
                Ok(Item::State(self.script_let()?))
            }
            _ => Err(self.err(
                "scripts allow only `use`, `const`, `props`, `shared`, `let`, `fn`, `struct`, and `enum` at top level",
            )),
        }
    }

    pub(super) fn script_block(&mut self, label: &str) -> Result<ScriptBlockItem, ParseError> {
        let start = self.cur().span;
        let name = self.ident()?;
        if name.name != label {
            return Err(ParseError::new(name.span, format!("expected `{label}`")));
        }
        self.expect(TokenKind::LBrace, &format!("expected `{{` after `{label}`"))?;
        let mut fields = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            let doc = self.take_doc();
            let name = self.ident()?;
            self.expect(TokenKind::Colon, "expected `:` after script field")?;
            let ty = self.ty()?;
            let default = if self.cur().kind == TokenKind::Eq {
                let _ = self.bump();
                Some(self.expr()?)
            } else {
                None
            };
            self.expect(TokenKind::Semi, "expected `;` after script field")?;
            fields.push(FieldItem {
                span: join(name.span, self.prev_end(name.span)),
                doc,
                name,
                ty,
                embedded: false,
                flat: false,
                default,
            });
        }
        self.expect(TokenKind::RBrace, &format!("expected `}}` after `{label}`"))?;
        Ok(ScriptBlockItem {
            span: join(start, self.prev_end(start)),
            fields,
        })
    }

    pub(super) fn script_let(&mut self) -> Result<ScriptLetItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Let, "expected `let`")?;
        let name = self.ident()?;
        let ty = if self.cur().kind == TokenKind::Colon {
            let _ = self.bump();
            Some(self.ty()?)
        } else {
            None
        };
        self.expect(
            TokenKind::Eq,
            "script state `let` needs a literal initializer",
        )?;
        let value = self.expr()?;
        self.expect(TokenKind::Semi, "expected `;` after script state")?;
        Ok(ScriptLetItem {
            span: join(start, self.prev_end(start)),
            name,
            ty,
            value,
        })
    }

    pub(super) fn j3ui_item(&mut self, vis_pub: bool, doc: String) -> Result<FnItem, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::At, "expected `@j3ui`")?;
        let kind = self.ident()?;
        if kind.name != "j3ui" {
            return Err(ParseError::new(
                kind.span,
                format!("unknown intrinsic `@{}`", kind.name),
            ));
        }
        let name = self.fn_name()?;
        let params = self.param_list()?;
        if params.is_empty() {
            return Err(ParseError::new(
                name.span,
                "`@j3ui` needs a Ui receiver as its first parameter",
            ));
        }
        let recv = params[0].name.name.clone();
        let saved = self.j3ui_recv.clone();
        self.j3ui_recv = Some(recv);
        let body = self.block()?;
        self.j3ui_recv = saved;
        Ok(FnItem {
            span: join(start, body.span),
            doc,
            vis_pub,
            name,
            type_params: Vec::new(),
            bounds: Vec::new(),
            is_impl: false,
            shader: None,
            params,
            ret: None,
            body,
        })
    }

    pub(super) fn j3ui_widget_stmt(&mut self) -> Result<Stmt, ParseError> {
        let recv = match &self.j3ui_recv {
            Some(name) => name.clone(),
            None => return Err(self.err("`@j3ui` widget outside a component")),
        };
        let start = self.cur().span;
        let name = self.ident()?;
        let args = self.args()?;
        if self.cur().kind == TokenKind::LBrace {
            let body = self.block()?;
            let span = join(start, body.span);
            return self.j3ui_closer(&recv, name, args, body, span);
        }
        self.expect(TokenKind::Semi, "expected `;` after widget")?;
        let span = join(start, self.prev_end(start));
        Ok(Stmt::Expr(self.method_call(&recv, name, args, span)))
    }

    pub(super) fn j3ui_closer(
        &self,
        recv: &str,
        name: Ident,
        args: Vec<Expr>,
        body: Block,
        span: Span,
    ) -> Result<Stmt, ParseError> {
        let call = self.method_call(recv, name.clone(), args, name.span);
        let widget = name.name.as_str();
        if J3UI_END.contains(&widget) {
            let end = self.method_call(recv, ident_at("end", name.span), Vec::new(), name.span);
            let mut stmts = vec![Stmt::Expr(call)];
            stmts.extend(body.stmts);
            stmts.push(Stmt::Expr(end));
            return Ok(Stmt::Block(Block { span, stmts }));
        }
        if J3UI_POP.contains(&widget) {
            let pop = self.method_call(recv, ident_at("popId", name.span), Vec::new(), name.span);
            let mut stmts = vec![Stmt::Expr(call)];
            stmts.extend(body.stmts);
            stmts.push(Stmt::Expr(pop));
            return Ok(Stmt::Block(Block { span, stmts }));
        }
        if J3UI_IF.contains(&widget) {
            return Ok(Stmt::If(IfStmt {
                span,
                cond: call,
                then_block: body,
                else_block: None,
            }));
        }
        Err(ParseError::new(
            name.span,
            format!("`{}` does not take a body", name.name),
        ))
    }

    pub(super) fn method_call(&self, recv: &str, name: Ident, args: Vec<Expr>, span: Span) -> Expr {
        let base = Expr {
            span: name.span,
            kind: ExprKind::Name(Ident {
                name: recv.to_string(),
                span: name.span,
            }),
        };
        let field = Expr {
            span: name.span,
            kind: ExprKind::Field {
                base: Box::new(base),
                field: name,
            },
        };
        Expr {
            span,
            kind: ExprKind::Call {
                func: Box::new(field),
                type_args: Vec::new(),
                args,
            },
        }
    }
}
