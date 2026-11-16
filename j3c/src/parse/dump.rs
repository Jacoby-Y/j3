//! `j3c dump-ast`: the parsed AST as S-expressions, the parse oracle for the J3 port
//! (`plans/11-pre-bootstrap.md` A5). `fmt` reprints tokens, so it hides tree shape; this does not.
//! No spans and no `Debug`: the bootstrap reproduces this byte for byte. Every enum match is
//! exhaustive and every struct destructured without `..`, so a new AST node or field fails to
//! compile here. Grammar: `j3c/AGENTS.md` (`dump-ast`).

use super::ast::*;

/// `file` printed: items and statements one per line (two-space indent), expressions inline.
pub fn dump(file: &File) -> String {
    let File { span: _, doc, module, is_script, items, embeds: _, dyns } = file;
    let mut w = W { out: String::new(), depth: 0 };
    w.open("file");
    w.doc(doc);
    w.list("module", module.iter().map(|m| m.name.as_str()));
    w.flag(*is_script, "script");
    w.list("dyns", dyns.iter().map(String::as_str));
    w.depth += 1;
    for it in items {
        w.nl();
        w.item(it);
    }
    w.depth -= 1;
    w.out.push_str(")\n");
    w.out
}

struct W {
    out: String,
    depth: usize,
}

fn bin(op: BinOp) -> &'static str {
    match op {
        BinOp::Add => "+", BinOp::Sub => "-", BinOp::Mul => "*", BinOp::Div => "/", BinOp::Rem => "%",
        BinOp::Shl => "<<", BinOp::Shr => ">>", BinOp::BitAnd => "&", BinOp::BitOr => "|", BinOp::BitXor => "^",
        BinOp::Eq => "==", BinOp::Ne => "!=", BinOp::Lt => "<", BinOp::Le => "<=", BinOp::Gt => ">",
        BinOp::Ge => ">=", BinOp::And => "&&", BinOp::Or => "||",
    }
}

impl W {
    /// `(head`; a head with a leading space starts the group after a space: `" x"` writes ` (x`.
    fn open(&mut self, head: &str) {
        match head.strip_prefix(' ') {
            Some(h) => {
                self.out.push_str(" (");
                self.out.push_str(h);
            }
            None => {
                self.out.push('(');
                self.out.push_str(head);
            }
        }
    }
    fn close(&mut self) {
        self.out.push(')');
    }
    fn nl(&mut self) {
        self.out.push('\n');
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
    }
    fn atom(&mut self, s: &str) {
        self.out.push(' ');
        self.out.push_str(s);
    }
    fn flag(&mut self, on: bool, s: &str) {
        if on {
            self.atom(s);
        }
    }
    /// `"..."`: `\\ \" \n \t \r` escaped, other bytes outside 0x20..0x7e as `\xHH`.
    fn str(&mut self, s: &[u8]) {
        self.out.push_str(" \"");
        for &b in s {
            match b {
                b'\\' => self.out.push_str("\\\\"),
                b'"' => self.out.push_str("\\\""),
                b'\n' => self.out.push_str("\\n"),
                b'\t' => self.out.push_str("\\t"),
                b'\r' => self.out.push_str("\\r"),
                0x20..=0x7e => self.out.push(b as char),
                _ => self.out.push_str(&format!("\\x{b:02x}")),
            }
        }
        self.out.push('"');
    }
    fn doc(&mut self, doc: &str) {
        if !doc.is_empty() {
            self.open(" doc");
            self.str(doc.as_bytes());
            self.close();
        }
    }
    /// `(head a b)`, left out when empty.
    fn list<'a>(&mut self, head: &str, xs: impl Iterator<Item = &'a str>) {
        let mut xs = xs.peekable();
        if xs.peek().is_some() {
            self.out.push_str(" (");
            self.out.push_str(head);
            xs.for_each(|x| self.atom(x));
            self.close();
        }
    }
    fn path(&mut self, p: &[Ident]) {
        let s: Vec<&str> = p.iter().map(|i| i.name.as_str()).collect();
        self.atom(&s.join("."));
    }
    fn opt_ty(&mut self, t: &Option<Type>) {
        match t {
            Some(t) => self.ty(t),
            None => self.atom("_"),
        }
    }
    fn opt_expr(&mut self, e: &Option<Expr>) {
        match e {
            Some(e) => self.expr(e),
            None => self.atom("_"),
        }
    }
    fn tparams(&mut self, names: &[Ident], bounds: &[Vec<Ident>]) {
        if names.is_empty() {
            return;
        }
        self.open(" tparams");
        for (i, n) in names.iter().enumerate() {
            match bounds.get(i).filter(|b| !b.is_empty()) {
                Some(b) => {
                    self.open(" ");
                    self.out.push_str(&n.name);
                    b.iter().for_each(|x| self.atom(&x.name));
                    self.close();
                }
                None => self.atom(&n.name),
            }
        }
        self.close();
    }
    fn params(&mut self, ps: &[Param]) {
        self.open(" params");
        for Param { name, ty } in ps {
            self.open(" ");
            self.out.push_str(&name.name);
            self.ty(ty);
            self.close();
        }
        self.close();
    }
    fn fields(&mut self, fs: &[(Ident, Expr)]) {
        for (n, e) in fs {
            self.open(" field ");
            self.out.push_str(&n.name);
            self.expr(e);
            self.close();
        }
    }
    fn field_items(&mut self, fs: &[FieldItem]) {
        for FieldItem { span: _, doc, name, ty, embedded, flat, default } in fs {
            self.nl();
            self.open("field");
            self.doc(doc);
            self.flag(*embedded, "embedded");
            self.flag(*flat, "flat");
            self.atom(&name.name);
            self.ty(ty);
            if let Some(d) = default {
                self.expr(d);
            }
            self.close();
        }
    }
    fn methods(&mut self, ms: &[FnItem]) {
        for m in ms {
            self.nl();
            self.func(m);
        }
    }

    fn item(&mut self, it: &Item) {
        match it {
            Item::Use(UseItem { span: _, path, names }) => {
                self.open("use");
                self.path(path);
                if let Some(ns) = names {
                    self.open(" names");
                    ns.iter().for_each(|n| self.atom(&n.name));
                    self.close();
                }
            }
            Item::UseLib(UseLibItem { span: _, lib, alias, decls }) => {
                self.open("use-lib");
                self.str(lib.as_bytes());
                self.atom(&alias.name);
                self.depth += 1;
                for LibFnDecl { span: _, name, params, ret, symbol } in decls {
                    self.nl();
                    self.open("lib-fn ");
                    self.out.push_str(&name.name);
                    self.params(params);
                    self.opt_ty(ret);
                    match symbol {
                        Some(s) => self.str(s.as_bytes()),
                        None => self.atom("_"),
                    }
                    self.close();
                }
                self.depth -= 1;
            }
            Item::Const(ConstItem { span: _, doc, vis_pub, name, ty, value }) => {
                self.open("const");
                self.doc(doc);
                self.flag(*vis_pub, "pub");
                self.atom(&name.name);
                self.opt_ty(ty);
                self.expr(value);
            }
            Item::Fn(f) => return self.func(f),
            Item::Struct(StructItem {
                span: _, doc, vis_pub, debug, packed, gpu, derives, name, type_params, bounds, fields, methods,
            }) => {
                self.open("struct");
                self.doc(doc);
                self.flag(*vis_pub, "pub");
                self.flag(*debug, "debug");
                self.flag(*packed, "packed");
                match gpu {
                    GpuStruct::None => {}
                    GpuStruct::Vertex => self.atom("vertex"),
                    GpuStruct::VertexInstance => self.atom("vertex-instance"),
                    GpuStruct::Varying => self.atom("varying"),
                    GpuStruct::Plain => self.atom("shader"),
                }
                self.list("derives", derives.iter().map(|d| d.name.as_str()));
                self.atom(&name.name);
                self.tparams(type_params, bounds);
                self.depth += 1;
                self.field_items(fields);
                self.methods(methods);
                self.depth -= 1;
            }
            Item::Extend(ExtendItem { span: _, name, type_params, methods }) => {
                self.open("extend");
                self.atom(&name.name);
                self.tparams(type_params, &[]);
                self.depth += 1;
                self.methods(methods);
                self.depth -= 1;
            }
            Item::Enum(EnumItem { span: _, doc, vis_pub, debug, try_catch, derives, name, type_params, bounds, variants }) => {
                self.open("enum");
                self.doc(doc);
                self.flag(*vis_pub, "pub");
                self.flag(*debug, "debug");
                if let Some((a, b)) = try_catch {
                    self.list("try-catch", [a.name.as_str(), b.name.as_str()].into_iter());
                }
                self.list("derives", derives.iter().map(|d| d.name.as_str()));
                self.atom(&name.name);
                self.tparams(type_params, bounds);
                self.depth += 1;
                for VariantItem { span: _, doc, name, fields } in variants {
                    self.nl();
                    self.open("case");
                    self.doc(doc);
                    self.atom(&name.name);
                    fields.iter().for_each(|t| self.ty(t));
                    self.close();
                }
                self.depth -= 1;
            }
            Item::Trait(TraitItem { span: _, doc, vis_pub, name, supers, methods }) => {
                self.open("trait");
                self.doc(doc);
                self.flag(*vis_pub, "pub");
                self.atom(&name.name);
                self.list("supers", supers.iter().map(|s| s.name.as_str()));
                self.depth += 1;
                for TraitMethod { span: _, name, params, ret, body } in methods {
                    self.nl();
                    self.open("trait-fn ");
                    self.out.push_str(&name.name);
                    self.params(params);
                    self.opt_ty(ret);
                    match body {
                        Some(b) => self.block(b),
                        None => self.atom("_"),
                    }
                    self.close();
                }
                self.depth -= 1;
            }
            Item::Let(ModLetItem { span: _, doc, name, ty, value, thread, uniform }) => {
                self.open("mod-let");
                self.doc(doc);
                self.flag(*thread, "thread");
                self.flag(*uniform, "uniform");
                self.atom(&name.name);
                self.ty(ty);
                self.opt_expr(value);
            }
            Item::Props(ScriptBlockItem { span: _, fields }) | Item::Shared(ScriptBlockItem { span: _, fields }) => {
                self.open(if matches!(it, Item::Props(_)) { "props" } else { "shared" });
                self.depth += 1;
                self.field_items(fields);
                self.depth -= 1;
            }
            Item::State(ScriptLetItem { span: _, name, ty, value }) => {
                self.open("state");
                self.atom(&name.name);
                self.opt_ty(ty);
                self.expr(value);
            }
        }
        self.close();
    }

    fn func(&mut self, f: &FnItem) {
        let FnItem { span: _, doc, vis_pub, name, type_params, bounds, where_bounds, is_impl, shader, params, ret, body } = f;
        self.open("fn");
        self.doc(doc);
        self.flag(*vis_pub, "pub");
        self.flag(*is_impl, "impl");
        match shader {
            None => {}
            Some(GpuStage::Helper) => self.atom("shader"),
            Some(GpuStage::Vertex) => self.atom("shader-vertex"),
            Some(GpuStage::Fragment) => self.atom("shader-fragment"),
        }
        self.atom(&name.name);
        self.tparams(type_params, bounds);
        if !where_bounds.is_empty() {
            self.open(" where");
            for (n, b) in where_bounds {
                self.list(&n.name, b.iter().map(|x| x.name.as_str()));
            }
            self.close();
        }
        self.params(params);
        self.opt_ty(ret);
        self.block(body);
        self.close();
    }

    fn ty(&mut self, t: &Type) {
        let Type { span: _, kind } = t;
        let fn_like = |w: &mut W, params: &[Type], ret: &Option<Box<Type>>| {
            w.out.push_str(" (");
            params.iter().enumerate().for_each(|(i, p)| {
                if i > 0 { w.out.push(' ') }
                w.ty_bare(p)
            });
            w.close();
            match ret {
                Some(r) => w.ty(r),
                None => w.atom("_"),
            }
        };
        match kind {
            TypeKind::Named(n) => self.atom(n),
            TypeKind::Generic { name, args } => {
                self.open(" gen ");
                self.out.push_str(name);
                args.iter().for_each(|a| self.ty(a));
                self.close();
            }
            TypeKind::Ref(t) => { self.open(" ref"); self.ty(t); self.close(); }
            TypeKind::Array { len, elem } => { self.open(&format!(" array {len}")); self.ty(elem); self.close(); }
            TypeKind::Slice(t) => { self.open(" slice"); self.ty(t); self.close(); }
            TypeKind::Fn { params, ret } => { self.open(" fn-type"); fn_like(self, params, ret); self.close(); }
            TypeKind::Closure { params, ret, boxed } => {
                self.open(" closure-type");
                self.flag(*boxed, "boxed");
                fn_like(self, params, ret);
                self.close();
            }
            TypeKind::Tuple(ts) => { self.open(" tuple-type"); ts.iter().for_each(|t| self.ty(t)); self.close(); }
            TypeKind::Dyn(n) => { self.open(" dyn"); self.atom(&n.name); self.close(); }
        }
    }
    /// A type with no leading space (first inside a group).
    fn ty_bare(&mut self, t: &Type) {
        let at = self.out.len();
        self.ty(t);
        self.out.remove(at);
    }

    fn block(&mut self, b: &Block) {
        let Block { span: _, stmts } = b;
        self.open(" block");
        self.depth += 1;
        for s in stmts {
            self.nl();
            self.stmt(s);
        }
        self.depth -= 1;
        self.close();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Let(LetStmt { span: _, doc, name, ty, value }) => {
                self.open("let");
                self.doc(doc);
                self.atom(&name.name);
                self.opt_ty(ty);
                self.expr(value);
            }
            Stmt::Return(ReturnStmt { span: _, value }) => {
                self.open("return");
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Assign(AssignStmt { span: _, place, value }) => {
                self.open("=");
                self.expr(place);
                self.expr(value);
            }
            Stmt::AssignOp(AssignOpStmt { span: _, place, op, value }) => {
                self.open(&format!("{}=", bin(*op)));
                self.expr(place);
                self.expr(value);
            }
            Stmt::Expr(e) => { self.open("expr"); self.expr(e); }
            Stmt::If(IfStmt { span: _, cond, then_block, else_block }) => {
                self.open("if");
                self.expr(cond);
                self.block(then_block);
                if let Some(b) = else_block {
                    self.block(b);
                }
            }
            Stmt::While(WhileStmt { span: _, cond, body }) => { self.open("while"); self.expr(cond); self.block(body); }
            Stmt::For(ForStmt { span: _, name, start, end, inclusive, body }) => {
                self.open("for");
                self.atom(&name.name);
                self.open(if *inclusive { " ..=" } else { " .." });
                self.expr(start);
                self.expr(end);
                self.close();
                self.block(body);
            }
            Stmt::ForEach(ForEachStmt { span: _, index, name, iter, body }) => {
                self.open("for-each");
                self.atom(index.as_ref().map_or("_", |i| i.name.as_str()));
                self.atom(&name.name);
                self.expr(iter);
                self.block(body);
            }
            Stmt::UseLocal(UseLocalStmt { span: _, name }) => { self.open("use-local"); self.atom(&name.name); }
            Stmt::Match(m) => return self.match_(m),
            Stmt::LetPat(LetPatStmt { span: _, pat, value, else_block }) => {
                self.open("let-pat");
                self.pat(pat);
                self.expr(value);
                if let Some(b) = else_block {
                    self.block(b);
                }
            }
            Stmt::Defer(DeferStmt { span: _, body }) => { self.open("defer"); self.block(body); }
            Stmt::Labeled(LabeledStmt { span: _, label, body }) => {
                self.open("label");
                self.atom(&label.name);
                self.out.push(' ');
                self.stmt(body);
            }
            Stmt::Break(_, l) | Stmt::Continue(_, l) => {
                self.open(if matches!(s, Stmt::Break(..)) { "break" } else { "continue" });
                if let Some(l) = l {
                    self.atom(&l.name);
                }
            }
            Stmt::Block(b) => {
                let at = self.out.len();
                self.block(b);
                self.out.remove(at);
                return;
            }
        }
        self.close();
    }

    fn match_(&mut self, m: &MatchStmt) {
        let MatchStmt { span: _, expr, arms } = m;
        self.open("match");
        self.expr(expr);
        self.depth += 1;
        for MatchArm { span: _, pats, guard, body, tail } in arms {
            self.nl();
            self.open("arm");
            self.open(" pats");
            pats.iter().for_each(|p| self.pat(p));
            self.close();
            if let Some(g) = guard {
                self.open(" guard");
                self.expr(g);
                self.close();
            }
            self.block(body);
            if let Some(t) = tail {
                self.open(" tail");
                self.expr(t);
                self.close();
            }
            self.close();
        }
        self.depth -= 1;
        self.close();
    }

    fn pat(&mut self, p: &Pat) {
        match p {
            Pat::Bind(n) => { self.open(" bind"); self.atom(&n.name); }
            Pat::Wild(_) => self.open(" wild"),
            Pat::Variant { span: _, path, payload } => {
                self.open(" variant");
                self.path(path);
                payload.iter().for_each(|p| self.pat(p));
            }
            Pat::Struct { span: _, path, fields, rest } => {
                self.open(" struct-pat");
                self.path(path);
                for (n, p) in fields {
                    self.open(" ");
                    self.out.push_str(&n.name);
                    self.pat(p);
                    self.close();
                }
                self.flag(*rest, "..");
            }
            Pat::Tuple { span: _, elems } => { self.open(" tuple-pat"); elems.iter().for_each(|p| self.pat(p)); }
            Pat::Lit(e) => { self.open(" lit"); self.expr(e); }
            Pat::Range { lo, hi } => { self.open(" range"); self.expr(lo); self.expr(hi); }
        }
        self.close();
    }

    fn expr(&mut self, e: &Expr) {
        let Expr { span: _, kind } = e;
        match kind {
            ExprKind::Name(n) => return self.atom(&n.name),
            ExprKind::Int(v) => return self.atom(&v.to_string()),
            ExprKind::BigInt(v) => { self.open(" bigint"); self.atom(&v.to_string()); }
            ExprKind::Zeroed => self.open(" zeroed"),
            ExprKind::Char(c) => { self.open(" char"); self.atom(&c.to_string()); }
            ExprKind::Float(f) => { self.open(" float"); self.atom(&format!("0x{:016x}", f.to_bits())); }
            ExprKind::Bool(b) => { self.open(" bool"); self.atom(if *b { "true" } else { "false" }); }
            ExprKind::String(s) => return self.str(s),
            ExprKind::StructLit { name, type_args, fields } => {
                self.open(" struct-lit");
                self.atom(&name.name);
                if !type_args.is_empty() {
                    self.open(" targs");
                    type_args.iter().for_each(|t| self.ty(t));
                    self.close();
                }
                self.fields(fields);
            }
            ExprKind::TypePath { name, args } => {
                self.open(" type-path");
                self.atom(&name.name);
                args.iter().for_each(|t| self.ty(t));
            }
            ExprKind::ArrayLit { elems } => { self.open(" array-lit"); elems.iter().for_each(|e| self.expr(e)); }
            ExprKind::SliceLit { elem, fields } => { self.open(" slice-lit"); self.ty(elem); self.fields(fields); }
            ExprKind::Index { base, index } => { self.open(" index"); self.expr(base); self.expr(index); }
            ExprKind::Slice { base, start, end } => {
                self.open(" slice-expr");
                self.expr(base);
                for x in [start, end] {
                    match x {
                        Some(x) => self.expr(x),
                        None => self.atom("_"),
                    }
                }
            }
            ExprKind::Field { base, field } => { self.open(" ."); self.expr(base); self.atom(&field.name); }
            ExprKind::Call { func, type_args, args } => {
                self.open(" call");
                self.expr(func);
                if !type_args.is_empty() {
                    self.open(" targs");
                    type_args.iter().for_each(|t| self.ty(t));
                    self.close();
                }
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::Syscall { args } => { self.open(" syscall"); args.iter().for_each(|a| self.expr(a)); }
            ExprKind::Atomic { op, order, args } => {
                self.open(" atomic");
                self.atom(op.name());
                self.atom(order.name());
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::ScriptType(s) => { self.open(" script-type"); self.str(s.as_bytes()); }
            ExprKind::Embed { kind, path, glob } => {
                self.open(" embed");
                self.atom(match kind {
                    EmbedKind::Text => "text",
                    EmbedKind::Bytes => "bytes",
                    EmbedKind::Dir => "dir",
                });
                self.str(path.as_bytes());
                self.str(glob.as_bytes());
            }
            ExprKind::ScriptLit { path, fields } => { self.open(" script-lit"); self.str(path.as_bytes()); self.fields(fields); }
            ExprKind::SizeOf(t) => { self.open(" sizeof"); self.ty(t); }
            ExprKind::UiChoice { set, value, index } => {
                self.open(" ui-choice");
                self.ty(set);
                self.expr(value);
                self.expr(index);
            }
            ExprKind::SliceFrom { ptr, len } => { self.open(" slice-from"); self.expr(ptr); self.expr(len); }
            ExprKind::Cast { expr, ty } => { self.open(" as"); self.expr(expr); self.ty(ty); }
            ExprKind::Binary { op, lhs, rhs } => { self.open(" "); self.out.push_str(bin(*op)); self.expr(lhs); self.expr(rhs); }
            ExprKind::Unary { op, expr } => {
                self.open(" ");
                self.out.push_str(match op {
                    UnaryOp::Neg => "-",
                    UnaryOp::BitNot => "~",
                    UnaryOp::Ref => "&",
                    UnaryOp::Deref => "*",
                    UnaryOp::Not => "!",
                });
                self.expr(expr);
            }
            ExprKind::Interpolate { parts } => {
                self.open(" interp");
                for p in parts {
                    match p {
                        InterpPart::Lit(s) => self.str(s),
                        InterpPart::Expr(e) => self.expr(e),
                    }
                }
            }
            ExprKind::Tuple(es) => { self.open(" tuple"); es.iter().for_each(|e| self.expr(e)); }
            ExprKind::If { cond, then_expr, else_expr } => {
                self.open(" if-expr");
                self.expr(cond);
                self.expr(then_expr);
                self.expr(else_expr);
            }
            ExprKind::Question { expr } => { self.open(" ?"); self.expr(expr); }
            ExprKind::Match(m) => {
                self.out.push(' ');
                return self.match_(m);
            }
            ExprKind::Closure(c) => {
                let ClosureLit { arena, params, ret, body } = &**c;
                self.open(" fn-lit");
                if let Some(a) = arena {
                    self.open(" arena");
                    self.expr(a);
                    self.close();
                }
                self.open(" params");
                for ClosureParam { name, ty } in params {
                    self.open(" ");
                    self.out.push_str(&name.name);
                    self.opt_ty(ty);
                    self.close();
                }
                self.close();
                self.opt_ty(ret);
                self.block(body);
            }
        }
        self.close();
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dump_ast_shapes() {
        let src = "module m;\nfn f(a: int): int {\n    let g = fn(x: int): int { return x * 2; };\n    let r = (a + 1) * g(a) - a % 3;\n    let s = id::<int>(r);\n    return match s { 0 | 1 { -1 } _ if s > 9 { 2 } _ { 0 } };\n}\n";
        let ast = super::super::parse(src, crate::span::FileId(1)).expect("parses");
        let want = "(file (module m)
  (fn f (params (a int)) int (block
    (let g _ (fn-lit (params (x int)) int (block
      (return (* x 2)))))
    (let r _ (- (* (+ a 1) (call g a)) (% a 3)))
    (let s _ (call id (targs int) r))
    (return (match s
      (arm (pats (lit 0) (lit 1)) (block) (tail (- 1)))
      (arm (pats (wild)) (guard (> s 9)) (block) (tail 2))
      (arm (pats (wild)) (block) (tail 0)))))))
";
        assert_eq!(super::dump(&ast), want);
    }
}
