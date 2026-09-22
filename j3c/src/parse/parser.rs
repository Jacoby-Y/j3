use crate::parse::ast::{
    AssignOpStmt, AssignStmt, BinOp, Block, ConstItem, DeferStmt, EnumItem, Expr, ExprKind,
    FieldItem, File, FnItem, ForEachStmt, ForStmt, Ident, IfStmt, InterpPart, Item, LetStmt,
    LibFnDecl, MatchArm, MatchStmt, ModLetItem, Param, Pat, ReturnStmt, Stmt, StructItem, Type,
    TypeKind, UnaryOp, UseItem, UseLibItem, UseLocalStmt, VariantItem, WhileStmt,
};

struct ItemAttrs {
    debug: bool,
    try_catch: Option<(Ident, Ident)>,
    singleton: bool,
}
use crate::parse::error::ParseError;
use crate::parse::syntax::{
    ATTR_DEBUG, ATTR_SINGLETON, ATTR_TRY_CATCH, INTRINSIC_SIZEOF, INTRINSIC_SLICE,
    INTRINSIC_SYSCALL,
};
use crate::parse::token::{Token, TokenKind};
use crate::span::Span;

pub fn parse_tokens(tokens: &[Token]) -> Result<File, ParseError> {
    let mut p = Parser {
        tokens,
        i: 0,
        j3ui_recv: None,
    };
    p.file()
}

struct Parser<'a> {
    tokens: &'a [Token],
    i: usize,
    /// First parameter name while parsing an `@j3ui` body.
    j3ui_recv: Option<String>,
}

impl<'a> Parser<'a> {
    fn file(&mut self) -> Result<File, ParseError> {
        let start = self.cur().span;
        let doc = self.take_doc();
        self.expect(TokenKind::Module, "expected `module`")?;
        let module = self.path()?;
        self.expect(TokenKind::Semi, "expected `;` after module path")?;
        let mut items = Vec::new();
        while self.cur().kind != TokenKind::Eof {
            items.push(self.item()?);
        }
        let end = self.prev_end(start);
        Ok(File {
            span: join(start, end),
            doc,
            module,
            items,
        })
    }

    fn item(&mut self) -> Result<Item, ParseError> {
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
                Ok(Item::Let(self.mod_let_item(doc)?))
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

    fn take_doc(&mut self) -> String {
        let mut lines = Vec::new();
        while self.cur().kind == TokenKind::Doc {
            lines.push(self.cur().text.clone());
            let _ = self.bump();
        }
        lines.join("\n")
    }

    fn forbid_doc(&self, doc: &str) -> Result<(), ParseError> {
        if doc.is_empty() {
            Ok(())
        } else {
            Err(self.err("doc comment must sit on a fn, struct, enum, const, let, field, or variant"))
        }
    }

    fn attrs(&mut self) -> Result<ItemAttrs, ParseError> {
        let mut out = ItemAttrs {
            debug: false,
            try_catch: None,
            singleton: false,
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

    fn forbid_attrs(&self, attrs: &ItemAttrs, what: &str) -> Result<(), ParseError> {
        if attrs.debug {
            return Err(self.err(&format!("`#[Debug]` cannot sit on `{what}`")));
        }
        if attrs.try_catch.is_some() {
            return Err(self.err(&format!("`#[TryCatch]` cannot sit on `{what}`")));
        }
        if attrs.singleton {
            return Err(self.err(&format!("`#[singleton]` cannot sit on `{what}`")));
        }
        Ok(())
    }

    fn mod_let_item(&mut self, doc: String) -> Result<ModLetItem, ParseError> {
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
        })
    }

    fn use_lib_item(&mut self) -> Result<UseLibItem, ParseError> {
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

    fn lib_fn_decl(&mut self) -> Result<LibFnDecl, ParseError> {
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

    fn struct_item(&mut self, vis_pub: bool, debug: bool, doc: String) -> Result<StructItem, ParseError> {
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

    fn enum_item(&mut self, vis_pub: bool, attrs: ItemAttrs, doc: String) -> Result<EnumItem, ParseError> {
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

    fn use_item(&mut self) -> Result<UseItem, ParseError> {
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

    fn const_item(&mut self, vis_pub: bool, doc: String) -> Result<ConstItem, ParseError> {
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

    fn j3ui_item(&mut self, vis_pub: bool, doc: String) -> Result<FnItem, ParseError> {
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
            params,
            ret: None,
            body,
        })
    }

    fn j3ui_widget_stmt(&mut self) -> Result<Stmt, ParseError> {
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

    fn j3ui_closer(
        &self,
        recv: &str,
        name: Ident,
        args: Vec<Expr>,
        body: Block,
        span: Span,
    ) -> Result<Stmt, ParseError> {
        let call = self.method_call(recv, name.clone(), args, name.span);
        match name.name.as_str() {
            "panel" | "row" | "scroll" | "card" => {
                let end = self.method_call(recv, ident_at("end", name.span), Vec::new(), name.span);
                let mut stmts = vec![Stmt::Expr(call)];
                stmts.extend(body.stmts);
                stmts.push(Stmt::Expr(end));
                Ok(Stmt::Block(Block { span, stmts }))
            }
            "pushId" => {
                let pop = self.method_call(recv, ident_at("popId", name.span), Vec::new(), name.span);
                let mut stmts = vec![Stmt::Expr(call)];
                stmts.extend(body.stmts);
                stmts.push(Stmt::Expr(pop));
                Ok(Stmt::Block(Block { span, stmts }))
            }
            "button" => Ok(Stmt::If(IfStmt {
                span,
                cond: call,
                then_block: body,
                else_block: None,
            })),
            _ => Err(ParseError::new(
                name.span,
                format!("`{}` does not take a body", name.name),
            )),
        }
    }

    fn method_call(&self, recv: &str, name: Ident, args: Vec<Expr>, span: Span) -> Expr {
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

    fn fn_item(&mut self, vis_pub: bool, doc: String) -> Result<FnItem, ParseError> {
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

    fn type_params(&mut self) -> Result<Vec<Ident>, ParseError> {
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

    fn type_args(&mut self) -> Result<Vec<Type>, ParseError> {
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

    fn fn_name(&mut self) -> Result<Ident, ParseError> {
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

    fn param_list(&mut self) -> Result<Vec<Param>, ParseError> {
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

    fn block(&mut self) -> Result<Block, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let mut stmts = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            stmts.push(self.stmt()?);
        }
        self.expect(TokenKind::RBrace, "expected `}`")?;
        Ok(Block {
            span: join(start, self.prev_end(start)),
            stmts,
        })
    }

    fn stmt(&mut self) -> Result<Stmt, ParseError> {
        let doc = self.take_doc();
        if !doc.is_empty() && self.cur().kind != TokenKind::Let {
            return Err(self.err(
                "doc comment must sit on a fn, struct, enum, const, let, field, or variant",
            ));
        }
        if self.j3ui_recv.is_some()
            && self.cur().kind == TokenKind::Ident
            && self.look(1).kind == TokenKind::LParen
            && j3ui_widget(&self.cur().text)
        {
            return self.j3ui_widget_stmt();
        }
        match self.cur().kind {
            TokenKind::LBrace => Ok(Stmt::Block(self.block()?)),
            TokenKind::Let => {
                let start = self.cur().span;
                let _ = self.bump();
                let name = self.ident()?;
                let mut ty = None;
                if self.cur().kind == TokenKind::Colon {
                    let _ = self.bump();
                    ty = Some(self.ty()?);
                }
                self.expect(TokenKind::Eq, "expected `=` in let")?;
                let value = self.expr()?;
                self.expect(TokenKind::Semi, "expected `;` after let")?;
                Ok(Stmt::Let(LetStmt {
                    span: join(start, self.prev_end(start)),
                    doc,
                    name,
                    ty,
                    value,
                }))
            }
            TokenKind::Return => {
                let start = self.cur().span;
                let _ = self.bump();
                let mut value = None;
                if self.cur().kind != TokenKind::Semi {
                    value = Some(self.expr()?);
                }
                self.expect(TokenKind::Semi, "expected `;` after return")?;
                Ok(Stmt::Return(ReturnStmt {
                    span: join(start, self.prev_end(start)),
                    value,
                }))
            }
            TokenKind::If => Ok(Stmt::If(self.if_stmt()?)),
            TokenKind::While => Ok(Stmt::While(self.while_stmt()?)),
            TokenKind::For => self.for_stmt(),
            TokenKind::Match => Ok(Stmt::Match(self.match_stmt()?)),
            TokenKind::Use => {
                let start = self.cur().span;
                let _ = self.bump();
                let name = self.ident()?;
                self.expect(TokenKind::Semi, "expected `;` after `use`")?;
                Ok(Stmt::UseLocal(UseLocalStmt {
                    span: join(start, self.prev_end(start)),
                    name,
                }))
            }
            TokenKind::Defer => Ok(Stmt::Defer(self.defer_stmt()?)),
            TokenKind::Break => {
                let span = self.cur().span;
                let _ = self.bump();
                self.expect(TokenKind::Semi, "expected `;` after break")?;
                Ok(Stmt::Break(span))
            }
            TokenKind::Continue => {
                let span = self.cur().span;
                let _ = self.bump();
                self.expect(TokenKind::Semi, "expected `;` after continue")?;
                Ok(Stmt::Continue(span))
            }
            _ => {
                let start = self.cur().span;
                let place = self.expr()?;
                match self.cur().kind {
                    TokenKind::Eq => {
                        let _ = self.bump();
                        let value = self.expr()?;
                        self.expect(TokenKind::Semi, "expected `;` after assign")?;
                        Ok(Stmt::Assign(AssignStmt {
                            span: join(start, self.prev_end(start)),
                            place,
                            value,
                        }))
                    }
                    TokenKind::PlusEq
                    | TokenKind::MinusEq
                    | TokenKind::StarEq
                    | TokenKind::SlashEq => {
                        let kind = self.cur().kind;
                        if kind == TokenKind::SlashEq {
                            return Err(self.err("`/=` not in this subset"));
                        }
                        let op = match kind {
                            TokenKind::PlusEq => BinOp::Add,
                            TokenKind::MinusEq => BinOp::Sub,
                            TokenKind::StarEq => BinOp::Mul,
                            _ => BinOp::Add,
                        };
                        let _ = self.bump();
                        let value = self.expr()?;
                        self.expect(TokenKind::Semi, "expected `;` after assign-op")?;
                        Ok(Stmt::AssignOp(AssignOpStmt {
                            span: join(start, self.prev_end(start)),
                            place,
                            op,
                            value,
                        }))
                    }
                    _ => {
                        self.expect(TokenKind::Semi, "expected `;` after expression")?;
                        Ok(Stmt::Expr(place))
                    }
                }
            }
        }
    }

    fn ty(&mut self) -> Result<Type, ParseError> {
        let start = self.cur().span;
        if self.cur().kind == TokenKind::Amp {
            let _ = self.bump();
            let inner = self.ty()?;
            return Ok(Type {
                span: join(start, inner.span),
                kind: TypeKind::Ref(Box::new(inner)),
            });
        }
        if self.cur().kind == TokenKind::LBracket {
            let _ = self.bump();
            if self.cur().kind == TokenKind::RBracket {
                let _ = self.bump();
                let elem = self.ty()?;
                return Ok(Type {
                    span: join(start, elem.span),
                    kind: TypeKind::Slice(Box::new(elem)),
                });
            }
            let ntok = self.cur().clone();
            if ntok.kind != TokenKind::Int {
                return Err(self.err("expected array length or `[]`"));
            }
            let _ = self.bump();
            let len = parse_int(&ntok.text).map_err(|m| ParseError::new(ntok.span, m))?;
            if len < 0 {
                return Err(ParseError::new(ntok.span, "array length must be >= 0"));
            }
            self.expect(TokenKind::RBracket, "expected `]` after array length")?;
            let elem = self.ty()?;
            return Ok(Type {
                span: join(start, elem.span),
                kind: TypeKind::Array {
                    len: len as u64,
                    elem: Box::new(elem),
                },
            });
        }
        if self.cur().kind == TokenKind::Fn {
            let _ = self.bump();
            self.expect(TokenKind::LParen, "expected `(` after `fn`")?;
            let mut params = Vec::new();
            if self.cur().kind != TokenKind::RParen {
                loop {
                    params.push(self.ty()?);
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
            self.expect(TokenKind::RParen, "expected `)` after fn type params")?;
            let mut ret = None;
            if self.cur().kind == TokenKind::Colon {
                let _ = self.bump();
                ret = Some(Box::new(self.ty()?));
            }
            return Ok(Type {
                span: join(start, self.prev_end(start)),
                kind: TypeKind::Fn { params, ret },
            });
        }
        let name = self.ident()?;
        if self.cur().kind == TokenKind::Lt {
            let _ = self.bump();
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
            return Ok(Type {
                span: join(name.span, self.prev_end(name.span)),
                kind: TypeKind::Generic {
                    name: name.name,
                    args,
                },
            });
        }
        Ok(Type {
            span: name.span,
            kind: TypeKind::Named(name.name),
        })
    }

    fn if_stmt(&mut self) -> Result<IfStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::If, "expected `if`")?;
        let cond = self.expr()?;
        let then_block = self.block()?;
        let mut else_block = None;
        if self.cur().kind == TokenKind::Else {
            let _ = self.bump();
            if self.cur().kind == TokenKind::If {
                let inner = self.if_stmt()?;
                else_block = Some(Block {
                    span: inner.span,
                    stmts: vec![Stmt::If(inner)],
                });
            } else {
                else_block = Some(self.block()?);
            }
        }
        Ok(IfStmt {
            span: join(start, self.prev_end(start)),
            cond,
            then_block,
            else_block,
        })
    }

    fn while_stmt(&mut self) -> Result<WhileStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::While, "expected `while`")?;
        let cond = self.expr()?;
        let body = self.block()?;
        Ok(WhileStmt {
            span: join(start, body.span),
            cond,
            body,
        })
    }

    fn for_stmt(&mut self) -> Result<Stmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::For, "expected `for`")?;
        let first = self.ident()?;
        if self.cur().kind == TokenKind::Comma {
            let _ = self.bump();
            let name = self.ident()?;
            self.expect(TokenKind::In, "expected `in`")?;
            let iter = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::ForEach(ForEachStmt {
                span: join(start, body.span),
                index: Some(first),
                name,
                iter,
                body,
            }));
        }
        self.expect(TokenKind::In, "expected `in`")?;
        let iter = self.expr()?;
        if self.cur().kind == TokenKind::DotDot || self.cur().kind == TokenKind::DotDotEq {
            let inclusive = self.cur().kind == TokenKind::DotDotEq;
            let _ = self.bump();
            let end = self.expr()?;
            let body = self.block()?;
            return Ok(Stmt::For(ForStmt {
                span: join(start, body.span),
                name: first,
                start: iter,
                end,
                inclusive,
                body,
            }));
        }
        let body = self.block()?;
        Ok(Stmt::ForEach(ForEachStmt {
            span: join(start, body.span),
            index: None,
            name: first,
            iter,
            body,
        }))
    }

    fn match_stmt(&mut self) -> Result<MatchStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Match, "expected `match`")?;
        let expr = self.expr()?;
        self.expect(TokenKind::LBrace, "expected `{` after match")?;
        let mut arms = Vec::new();
        while self.cur().kind != TokenKind::RBrace && self.cur().kind != TokenKind::Eof {
            arms.push(self.match_arm()?);
        }
        self.expect(TokenKind::RBrace, "expected `}` after match")?;
        Ok(MatchStmt {
            span: join(start, self.prev_end(start)),
            expr,
            arms,
        })
    }

    fn match_arm(&mut self) -> Result<MatchArm, ParseError> {
        let start = self.cur().span;
        let path = self.path()?;
        let mut payload = None;
        if self.cur().kind == TokenKind::LParen {
            let _ = self.bump();
            payload = Some(self.pat()?);
            self.expect(TokenKind::RParen, "expected `)` after match payload")?;
        }
        let body = self.block()?;
        Ok(MatchArm {
            span: join(start, body.span),
            path,
            payload,
            body,
        })
    }

    fn pat(&mut self) -> Result<Pat, ParseError> {
        if self.cur().kind == TokenKind::Ident && self.cur().text == "_" {
            let _ = self.bump();
            return Ok(Pat::Wild);
        }
        let path = self.path()?;
        if path.len() == 1
            && self.cur().kind != TokenKind::LParen
            && self.cur().kind != TokenKind::Dot
        {
            return Ok(Pat::Bind(path.into_iter().next().unwrap()));
        }
        let mut inner = None;
        if self.cur().kind == TokenKind::LParen {
            let _ = self.bump();
            inner = Some(Box::new(self.pat()?));
            self.expect(TokenKind::RParen, "expected `)` after pattern")?;
        }
        Ok(Pat::Variant {
            path,
            payload: inner,
        })
    }

    fn defer_stmt(&mut self) -> Result<DeferStmt, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::Defer, "expected `defer`")?;
        let body = if self.cur().kind == TokenKind::LBrace {
            self.block()?
        } else {
            let stmt = self.stmt()?;
            Block {
                span: stmt_span(&stmt),
                stmts: vec![stmt],
            }
        };
        Ok(DeferStmt {
            span: join(start, body.span),
            body,
        })
    }

    fn expr(&mut self) -> Result<Expr, ParseError> {
        self.expr_or()
    }

    fn expr_or(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_and()?;
        while self.cur().kind == TokenKind::OrOr {
            let _ = self.bump();
            let rhs = self.expr_and()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::Or,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn expr_and(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_eq()?;
        while self.cur().kind == TokenKind::AndAnd {
            let _ = self.bump();
            let rhs = self.expr_eq()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op: BinOp::And,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn expr_eq(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_rel()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::EqEq => BinOp::Eq,
                TokenKind::Ne => BinOp::Ne,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_rel()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn expr_rel(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_add()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::Lt => BinOp::Lt,
                TokenKind::Le => BinOp::Le,
                TokenKind::Gt => BinOp::Gt,
                TokenKind::Ge => BinOp::Ge,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_add()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn expr_add(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_mul()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::Plus => BinOp::Add,
                TokenKind::Minus => BinOp::Sub,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_mul()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn expr_mul(&mut self) -> Result<Expr, ParseError> {
        let mut lhs = self.expr_as()?;
        loop {
            let op = match self.cur().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
            let _ = self.bump();
            let rhs = self.expr_as()?;
            let span = join(lhs.span, rhs.span);
            lhs = Expr {
                span,
                kind: ExprKind::Binary {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            };
        }
        Ok(lhs)
    }

    fn expr_as(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.unary()?;
        while self.cur().kind == TokenKind::As {
            let _ = self.bump();
            let ty = self.ty()?;
            let span = join(expr.span, ty.span);
            expr = Expr {
                span,
                kind: ExprKind::Cast {
                    expr: Box::new(expr),
                    ty,
                },
            };
        }
        Ok(expr)
    }

    fn unary(&mut self) -> Result<Expr, ParseError> {
        let start = self.cur().span;
        match self.cur().kind {
            TokenKind::Minus => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Neg,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Amp => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Ref,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Star => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Deref,
                        expr: Box::new(expr),
                    },
                })
            }
            TokenKind::Not => {
                let _ = self.bump();
                let expr = self.unary()?;
                Ok(Expr {
                    span: join(start, expr.span),
                    kind: ExprKind::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(expr),
                    },
                })
            }
            _ => self.postfix(),
        }
    }

    fn postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.primary()?;
        loop {
            match self.cur().kind {
                TokenKind::Dot => {
                    let _ = self.bump();
                    let field = self.ident()?;
                    let span = join(expr.span, field.span);
                    expr = Expr {
                        span,
                        kind: ExprKind::Field {
                            base: Box::new(expr),
                            field,
                        },
                    };
                    if self.cur().kind == TokenKind::ColonColon {
                        expr = self.turbofish_call(expr)?;
                    }
                }
                TokenKind::ColonColon => {
                    expr = self.turbofish_call(expr)?;
                }
                TokenKind::LParen => {
                    let args = self.args()?;
                    let span = join(expr.span, self.prev_end(expr.span));
                    expr = Expr {
                        span,
                        kind: ExprKind::Call {
                            func: Box::new(expr),
                            type_args: Vec::new(),
                            args,
                        },
                    };
                }
                TokenKind::Question => {
                    let _ = self.bump();
                    expr = Expr {
                        span: join(expr.span, self.prev_end(expr.span)),
                        kind: ExprKind::Question {
                            expr: Box::new(expr),
                        },
                    };
                }
                TokenKind::LBracket => {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::Colon {
                        let _ = self.bump();
                        self.expect(TokenKind::RBracket, "expected `]` after `:`")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Slice {
                                base: Box::new(expr),
                                start: None,
                                end: None,
                            },
                        };
                        continue;
                    }
                    if self.cur().kind == TokenKind::DotDot {
                        let _ = self.bump();
                        let end = if self.cur().kind == TokenKind::RBracket {
                            None
                        } else {
                            Some(Box::new(self.expr()?))
                        };
                        self.expect(TokenKind::RBracket, "expected `]` after slice")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Slice {
                                base: Box::new(expr),
                                start: None,
                                end,
                            },
                        };
                        continue;
                    }
                    let first = self.expr()?;
                    if self.cur().kind == TokenKind::DotDot {
                        let _ = self.bump();
                        let end = if self.cur().kind == TokenKind::RBracket {
                            None
                        } else {
                            Some(Box::new(self.expr()?))
                        };
                        self.expect(TokenKind::RBracket, "expected `]` after slice")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Slice {
                                base: Box::new(expr),
                                start: Some(Box::new(first)),
                                end,
                            },
                        };
                    } else {
                        self.expect(TokenKind::RBracket, "expected `]` after index")?;
                        expr = Expr {
                            span: join(expr.span, self.prev_end(expr.span)),
                            kind: ExprKind::Index {
                                base: Box::new(expr),
                                index: Box::new(first),
                            },
                        };
                    }
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn primary(&mut self) -> Result<Expr, ParseError> {
        let tok = self.cur().clone();
        match tok.kind {
            TokenKind::Ident | TokenKind::This => {
                let _ = self.bump();
                if tok.text == "true" {
                    return Ok(Expr {
                        span: tok.span,
                        kind: ExprKind::Bool(true),
                    });
                }
                if tok.text == "false" {
                    return Ok(Expr {
                        span: tok.span,
                        kind: ExprKind::Bool(false),
                    });
                }
                let name = Ident {
                    name: tok.text,
                    span: tok.span,
                };
                if self.cur().kind == TokenKind::Lt {
                    let saved = self.i;
                    match self.type_args() {
                        Ok(type_args) => {
                            if self.cur().kind == TokenKind::Dot {
                                return Ok(Expr {
                                    span: join(name.span, self.prev_end(name.span)),
                                    kind: ExprKind::TypePath {
                                        name,
                                        args: type_args,
                                    },
                                });
                            }
                            if self.looks_like_struct_lit() {
                                return self.struct_lit(name, type_args);
                            }
                            self.i = saved;
                        }
                        Err(_) => {
                            self.i = saved;
                        }
                    }
                }
                if self.looks_like_struct_lit() {
                    return self.struct_lit(name, Vec::new());
                }
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Name(name),
                })
            }
            TokenKind::Int => {
                let _ = self.bump();
                let value = parse_int(&tok.text).map_err(|m| ParseError::new(tok.span, m))?;
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Int(value),
                })
            }
            TokenKind::Float => {
                let _ = self.bump();
                let value = tok.text.parse::<f64>().map_err(|_| {
                    ParseError::new(tok.span, format!("invalid float `{}`", tok.text))
                })?;
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Float(value),
                })
            }
            TokenKind::String => {
                let _ = self.bump();
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::String(tok.text.into_bytes()),
                })
            }
            TokenKind::Interpolate => {
                let _ = self.bump();
                let parts = split_interp(&tok.text, tok.span)?;
                Ok(Expr {
                    span: tok.span,
                    kind: ExprKind::Interpolate { parts },
                })
            }
            TokenKind::At => {
                let start = tok.span;
                let _ = self.bump();
                let name = self.ident()?;
                match name.name.as_str() {
                    INTRINSIC_SYSCALL => {
                        let args = self.args()?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::Syscall { args },
                        })
                    }
                    INTRINSIC_SIZEOF => {
                        self.expect(TokenKind::LParen, "expected `(` after `@sizeof`")?;
                        let ty = self.ty()?;
                        self.expect(TokenKind::RParen, "expected `)` after `@sizeof`")?;
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::SizeOf(ty),
                        })
                    }
                    INTRINSIC_SLICE => {
                        let args = self.args()?;
                        if args.len() != 2 {
                            return Err(ParseError::new(
                                name.span,
                                "`@slice` takes a pointer and a length",
                            ));
                        }
                        let mut args = args.into_iter();
                        let ptr = args.next().unwrap();
                        let len = args.next().unwrap();
                        Ok(Expr {
                            span: join(start, self.prev_end(start)),
                            kind: ExprKind::SliceFrom {
                                ptr: Box::new(ptr),
                                len: Box::new(len),
                            },
                        })
                    }
                    other => Err(ParseError::new(
                        name.span,
                        format!("unknown intrinsic `@{other}`"),
                    )),
                }
            }
            TokenKind::LParen => {
                let _ = self.bump();
                let expr = self.expr()?;
                self.expect(TokenKind::RParen, "expected `)`")?;
                Ok(expr)
            }
            TokenKind::LBracket => {
                let start = tok.span;
                let _ = self.bump();
                if self.cur().kind == TokenKind::RBracket {
                    let _ = self.bump();
                    let elem = self.ty()?;
                    if !self.looks_like_struct_lit() {
                        return Err(self.err("expected `{ ptr, len }` after `[]T`"));
                    }
                    self.expect(TokenKind::LBrace, "expected `{` in slice literal")?;
                    let mut fields = Vec::new();
                    if self.cur().kind != TokenKind::RBrace {
                        loop {
                            let field = self.ident()?;
                            self.expect(TokenKind::Colon, "expected `:` after field name")?;
                            let value = self.expr()?;
                            fields.push((field, value));
                            if self.cur().kind == TokenKind::Comma {
                                let _ = self.bump();
                                if self.cur().kind == TokenKind::RBrace {
                                    break;
                                }
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(TokenKind::RBrace, "expected `}` after slice literal")?;
                    return Ok(Expr {
                        span: join(start, self.prev_end(start)),
                        kind: ExprKind::SliceLit { elem, fields },
                    });
                }
                let mut elems = Vec::new();
                if self.cur().kind != TokenKind::RBracket {
                    loop {
                        elems.push(self.expr()?);
                        if self.cur().kind == TokenKind::Comma {
                            let _ = self.bump();
                            if self.cur().kind == TokenKind::RBracket {
                                break;
                            }
                            continue;
                        }
                        break;
                    }
                }
                self.expect(TokenKind::RBracket, "expected `]` after array literal")?;
                Ok(Expr {
                    span: join(start, self.prev_end(start)),
                    kind: ExprKind::ArrayLit { elems },
                })
            }
            TokenKind::If => self.if_expr(),
            _ => Err(self.err("expected expression")),
        }
    }

    fn if_expr(&mut self) -> Result<Expr, ParseError> {
        let start = self.cur().span;
        self.expect(TokenKind::If, "expected `if`")?;
        let cond = self.expr()?;
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let then_expr = self.expr()?;
        self.expect(TokenKind::RBrace, "expected `}`")?;
        self.expect(TokenKind::Else, "if-expression needs `else`")?;
        self.expect(TokenKind::LBrace, "expected `{`")?;
        let else_expr = self.expr()?;
        self.expect(TokenKind::RBrace, "expected `}`")?;
        Ok(Expr {
            span: join(start, self.prev_end(start)),
            kind: ExprKind::If {
                cond: Box::new(cond),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            },
        })
    }

    fn args(&mut self) -> Result<Vec<Expr>, ParseError> {
        self.expect(TokenKind::LParen, "expected `(`")?;
        let mut args = Vec::new();
        if self.cur().kind != TokenKind::RParen {
            loop {
                args.push(self.expr()?);
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
        self.expect(TokenKind::RParen, "expected `)` after arguments")?;
        Ok(args)
    }

    fn path(&mut self) -> Result<Vec<Ident>, ParseError> {
        let mut parts = Vec::new();
        parts.push(self.ident()?);
        while self.cur().kind == TokenKind::Dot && self.look(1).kind == TokenKind::Ident {
            let _ = self.bump();
            parts.push(self.ident()?);
        }
        Ok(parts)
    }

    fn looks_like_struct_lit(&self) -> bool {
        if self.cur().kind != TokenKind::LBrace {
            return false;
        }
        match self.look(1).kind {
            TokenKind::RBrace => true,
            TokenKind::Ident | TokenKind::This => self.look(2).kind == TokenKind::Colon,
            _ => false,
        }
    }

    fn turbofish_call(&mut self, func: Expr) -> Result<Expr, ParseError> {
        self.expect(TokenKind::ColonColon, "expected `::`")?;
        let type_args = self.type_args()?;
        if self.cur().kind != TokenKind::LParen {
            return Err(self.err("turbofish needs a call"));
        }
        let args = self.args()?;
        Ok(Expr {
            span: join(func.span, self.prev_end(func.span)),
            kind: ExprKind::Call {
                func: Box::new(func),
                type_args,
                args,
            },
        })
    }

    fn struct_lit(&mut self, name: Ident, type_args: Vec<Type>) -> Result<Expr, ParseError> {
        self.expect(TokenKind::LBrace, "expected `{` in struct literal")?;
        let mut fields = Vec::new();
        if self.cur().kind != TokenKind::RBrace {
            loop {
                let field = self.ident()?;
                self.expect(TokenKind::Colon, "expected `:` after field name")?;
                let value = self.expr()?;
                fields.push((field, value));
                if self.cur().kind == TokenKind::Comma {
                    let _ = self.bump();
                    if self.cur().kind == TokenKind::RBrace {
                        break;
                    }
                    continue;
                }
                break;
            }
        }
        self.expect(TokenKind::RBrace, "expected `}` after struct literal")?;
        Ok(Expr {
            span: join(name.span, self.prev_end(name.span)),
            kind: ExprKind::StructLit {
                name,
                type_args,
                fields,
            },
        })
    }

    fn ident(&mut self) -> Result<Ident, ParseError> {
        let tok = self.cur().clone();
        if tok.kind != TokenKind::Ident && tok.kind != TokenKind::This {
            return Err(self.err("expected identifier"));
        }
        let _ = self.bump();
        Ok(Ident {
            name: tok.text,
            span: tok.span,
        })
    }

    fn expect(&mut self, kind: TokenKind, message: &str) -> Result<&Token, ParseError> {
        if self.cur().kind == kind {
            Ok(self.bump())
        } else {
            Err(self.err(message))
        }
    }

    fn err(&self, message: &str) -> ParseError {
        ParseError::new(self.cur().span, message)
    }

    fn cur(&self) -> &Token {
        &self.tokens[self.i]
    }

    fn look(&self, n: usize) -> &Token {
        let i = self.i + n;
        if i < self.tokens.len() {
            &self.tokens[i]
        } else {
            self.tokens.last().unwrap()
        }
    }

    fn bump(&mut self) -> &Token {
        let i = self.i;
        if self.tokens[i].kind != TokenKind::Eof {
            self.i += 1;
        }
        &self.tokens[i]
    }

    fn prev_end(&self, fallback: Span) -> Span {
        if self.i == 0 {
            return fallback;
        }
        self.tokens[self.i - 1].span
    }
}

fn join(a: Span, b: Span) -> Span {
    Span {
        file: a.file,
        start: a.start,
        end: b.end.max(a.end),
    }
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Let(s) => s.span,
        Stmt::Return(s) => s.span,
        Stmt::Assign(s) => s.span,
        Stmt::AssignOp(s) => s.span,
        Stmt::Expr(e) => e.span,
        Stmt::If(s) => s.span,
        Stmt::While(s) => s.span,
        Stmt::For(s) => s.span,
        Stmt::ForEach(s) => s.span,
        Stmt::UseLocal(s) => s.span,
        Stmt::Match(s) => s.span,
        Stmt::Defer(s) => s.span,
        Stmt::Break(s) | Stmt::Continue(s) => *s,
        Stmt::Block(b) => b.span,
    }
}

fn parse_int(text: &str) -> Result<i64, String> {
    if let Some(hex) = text.strip_prefix("0x") {
        match i64::from_str_radix(hex, 16) {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("invalid hex literal `{text}`")),
        }
    } else {
        match text.parse::<i64>() {
            Ok(v) => Ok(v),
            Err(_) => Err(format!("invalid integer `{text}`")),
        }
    }
}

fn split_interp(text: &str, span: Span) -> Result<Vec<InterpPart>, ParseError> {
    let mut parts = Vec::new();
    let mut lit = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '{' {
            if !lit.is_empty() {
                parts.push(InterpPart::Lit(std::mem::take(&mut lit)));
            }
            i += 1;
            let hole_start = i;
            let mut depth = 1;
            while i < chars.len() && depth > 0 {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                i += 1;
            }
            if depth != 0 {
                return Err(ParseError::new(span, "unterminated interpolate hole"));
            }
            if hole_start == i {
                return Err(ParseError::new(span, "empty interpolate hole"));
            }
            let hole: String = chars[hole_start..i].iter().collect();
            let expr = parse_hole_expr(&hole, span)?;
            parts.push(InterpPart::Expr(expr));
            i += 1;
            continue;
        }
        let ch = chars[i];
        let mut buf = [0u8; 4];
        let s = ch.encode_utf8(&mut buf);
        lit.extend(s.as_bytes());
        i += 1;
    }
    if !lit.is_empty() {
        parts.push(InterpPart::Lit(lit));
    }
    Ok(parts)
}

fn ident_at(name: &str, span: Span) -> Ident {
    Ident {
        name: name.to_string(),
        span,
    }
}

fn j3ui_widget(name: &str) -> bool {
    matches!(
        name,
        "label"
            | "labelBold"
            | "gap"
            | "button"
            | "buttonOff"
            | "bar"
            | "image"
            | "slider"
            | "range"
            | "toggle"
            | "check"
            | "stepper"
            | "chip"
            | "rule"
            | "wash"
            | "drop"
            | "panel"
            | "card"
            | "row"
            | "scroll"
            | "pushId"
            | "popId"
            | "end"
    )
}

fn parse_hole_expr(src: &str, span: Span) -> Result<Expr, ParseError> {
    let tokens = crate::parse::lex::lex(src, span.file)?;
    let mut p = Parser {
        tokens: &tokens,
        i: 0,
        j3ui_recv: None,
    };
    let expr = p.expr()?;
    if p.cur().kind != TokenKind::Eof {
        return Err(ParseError::new(span, "trailing junk in interpolate hole"));
    }
    Ok(expr)
}
