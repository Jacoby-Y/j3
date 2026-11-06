use super::*;

impl<'a> G<'a> {
    pub(super) fn new(all: &'a [Comp], files: &'a [SrcFile], theme_path: Option<&'a str>, icons: &'a [Icon], c: &'a Comp, root: bool, slots: &'a Slots) -> G<'a> {
        G {
            all,
            files,
            theme_path,
            icons,
            c,
            src: &files[c.fi].text,
            fi: c.fi,
            root,
            names: Names::default(),
            locals: Vec::new(),
            rename: Vec::new(),
            lets: Vec::new(),
            upd: Out::with_ctx(c.fi),
            pnt: Out::with_ctx(c.fi),
            used_u: BTreeSet::new(),
            used_p: BTreeSet::new(),
            k: 0,
            tags: Vec::new(),
            classes: Vec::new(),
            insts: Vec::new(),
            ifs: Vec::new(),
            eachs: Vec::new(),
            acts: Vec::new(),
            binds: Vec::new(),
            bases: Vec::new(),
            own: Vec::new(),
            gstk: Vec::new(),
            idx: Vec::new(),
            slots,
            ectx: Vec::new(),
            slot_fns: Out::with_ctx(c.fi),
            has_slot: false,
            slot_marks: Vec::new(),
            owns_slot: false,
            virt: None,
            in_virt: 0,
            flip_rows: Vec::new(),
            uses_list: false,
            uses_keyed: false,
            uses_virt: false,
            pass_root: 0,
        }
    }

    pub(super) fn nk(&mut self) -> usize {
        self.k += 1;
        self.k
    }

    pub(super) fn out(&mut self, m: Mode) -> &mut Out {
        match m {
            Mode::U => &mut self.upd,
            Mode::P => &mut self.pnt,
        }
    }

    pub(super) fn rw(&mut self, m: Mode, lo: usize, hi: usize, flat: bool, emit_ok: bool, calls: bool) -> R<()> {
        let rw = Rw { src: self.src, fi: self.fi, names: &self.names, locals: &self.locals, rename: &self.rename, emit_ok, sev: false, calls_emit: calls };
        let (out, used) = match m {
            Mode::U => (&mut self.upd, &mut self.used_u),
            Mode::P => (&mut self.pnt, &mut self.used_p),
        };
        rewrite(&rw, out, used, lo, hi, flat)
    }

    /// An expression, copied in place (no newline, no semicolon).
    pub(super) fn expr(&mut self, m: Mode, r: Range) -> R<()> {
        self.rw(m, r.0, r.1, false, true, false)
    }

    /// Handler statements on their own lines.
    pub(super) fn stmts(&mut self, m: Mode, r: Range, emit_ok: bool) -> R<()> {
        let (lo, hi) = trim_range(self.src, r.0, r.1);
        if lo == hi {
            return Ok(());
        }
        self.out(m).pad();
        self.rw(m, lo, hi, false, emit_ok, emit_ok)?;
        let last = self.src[lo..hi].chars().last().unwrap_or(';');
        if last != ';' && last != '}' {
            self.out(m).s(";");
        }
        self.out(m).s("\n");
        Ok(())
    }

    pub(super) fn tag(&mut self, t: &str) -> String {
        if !self.tags.iter().any(|x| x == t) {
            self.tags.push(t.to_string());
        }
        format!("this.{}", field_name("tag_", &self.tags, t))
    }

    pub(super) fn cls(&mut self, t: &str) -> String {
        if !self.classes.iter().any(|x| x == t) {
            self.classes.push(t.to_string());
        }
        format!("this.{}", field_name("cls_", &self.classes, t))
    }

    pub(super) fn slot(base: &Base, j: usize) -> String {
        match base {
            Some((iv, k)) => format!("{iv} * {k} + {j}"),
            None => j.to_string(),
        }
    }

    /// A string expression from attribute or text parts: a literal, or a template.
    pub(super) fn string(&mut self, m: Mode, parts: &[Part], _at: Range) -> R<()> {
        if parts.iter().all(|p| matches!(p, Part::Lit(_))) {
            let mut s = String::new();
            for p in parts {
                if let Part::Lit(l) = p {
                    s.push_str(l);
                }
            }
            self.out(m).s(&lit(&s));
            return Ok(());
        }
        // One hole: a backtick template (`{s}` of a string is `s` itself, any length).
        // Text around holes, or several holes: appended into a scratch `Text`, each hole its own
        // template, so only one number or struct, never the whole string, has to fit a template buffer.
        let single = parts.len() == 1;
        if !single {
            self.out(m).s("Text.scratch()");
        }
        for p in parts {
            match p {
                Part::Lit(l) => {
                    self.out(m).s(&format!(".add({})", lit(l)));
                }
                Part::Hole(r) => {
                    self.out(m).s(if single { "`{" } else { ".add(`{" });
                    self.rw(m, r.0, r.1, true, true, false)?;
                    self.out(m).s(if single { "}`" } else { "}`)" });
                }
            }
        }
        if !single {
            self.out(m).s(".str()");
        }
        Ok(())
    }

    pub(super) fn check_local(&self, name: &str, r: Range) -> R<()> {
        let rw = Rw { src: self.src, fi: self.fi, names: &self.names, locals: &self.locals, rename: &self.rename, emit_ok: false, sev: false, calls_emit: false };
        if rw.declared(name) || is_reserved(name) {
            return err(r.0, r.1, format!("`{name}` shadows a prop, `let`, `fn`, or name used by the generated code")).map_err(|e| e.help("pick another name"));
        }
        Ok(())
    }

    pub(super) fn run(&mut self) -> R<Out> {
        let c = self.c;
        let sc = &c.sc;
        // names
        let mut seen: Vec<String> = Vec::new();
        let mut claim = |n: &str, r: Range, what: &str| -> R<()> {
            if is_reserved(n) {
                return err(r.0, r.1, format!("`{n}` is reserved by the generated code ({what})"));
            }
            if seen.iter().any(|x| x == n) {
                return err(r.0, r.1, format!("`{n}` is declared twice in this component"));
            }
            seen.push(n.to_string());
            Ok(())
        };
        for p in &sc.props {
            claim(&p.name, p.name_r, "prop")?;
            self.names.props.push(p.name.clone());
        }
        for l in &sc.lets {
            claim(&l.name, l.name_r, "let")?;
            self.names.lets.push(l.name.clone());
        }
        for l in &sc.consts {
            claim(&l.name, l.name_r, "const")?;
            self.names.consts.push((l.name.clone(), format!("{}_{}", c.id, l.name)));
        }
        for f in &sc.fns {
            claim(&f.name, f.name_r, "fn")?;
            self.names.fns.push(f.name.clone());
            let ptxt = &self.src[f.params.0..f.params.1];
            let ok = match f.name.as_str() {
                "onMount" | "onDestroy" => f.params.0 == f.params.1 && f.ret.is_none(),
                "onFrame" => f.param_names.len() == 1 && !ptxt.contains(',') && ptxt.split(':').nth(1).map(str::trim) == Some("f32") && f.ret.is_none(),
                _ => true,
            };
            if !ok {
                let sig = if f.name == "onFrame" { "fn onFrame(dt: f32)" } else { "no arguments and no return type" };
                return err(f.name_r.0, f.name_r.1, format!("`{}` has a fixed signature: {sig}", f.name));
            }
        }
        // fns that emit, directly or through another fn (token scan; the rewrite validates each `emit`)
        let bodies: Vec<Vec<_>> = sc.fns.iter().map(|f| lex(&self.src[f.body.0..f.body.1], FileId(0)).unwrap_or_default()).collect();
        loop {
            let mut grew = false;
            for (f, toks) in sc.fns.iter().zip(&bodies) {
                let hit = (0..toks.len()).any(|i| {
                    let t = &toks[i];
                    let stmt = i == 0 || matches!(toks[i - 1].kind, TokenKind::Semi | TokenKind::LBrace | TokenKind::RBrace);
                    let called = t.kind == TokenKind::Ident && self.names.emitting.contains(&t.text) && toks.get(i + 1).is_some_and(|n| n.kind == TokenKind::LParen) && (i == 0 || !matches!(toks[i - 1].kind, TokenKind::Dot | TokenKind::ColonColon));
                    called || (t.text == "emit" && stmt && toks.get(i + 1).is_some_and(|n| n.kind == TokenKind::Ident))
                });
                if hit && !self.names.emitting.contains(&f.name) {
                    self.names.emitting.push(f.name.clone());
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        // `$slots.x`: the slots this component declares, and which use sites fill each
        let mut decl = Vec::new();
        declared(&c.mk.nodes, &mut decl)?;
        for d in decl {
            let sites = self.slots.list.iter().filter(|s| s.child == c.name && s.names.contains(&d)).map(|s| s.id).collect();
            self.names.slot_has.push((d, sites));
        }
        // a prop default is a constant: literals and the script's `const`s, not props, lets or fns
        for p in sc.props.iter().filter_map(|p| p.default.map(|r| (p, r))) {
            let (p, r) = p;
            let toks = lex(&self.src[r.0..r.1], FileId(0)).map_err(|e| UiError { lo: r.0 + e.span.start as usize, hi: r.0 + (e.span.end as usize).max(e.span.start as usize + 1), msg: e.message, help: None })?;
            for (i, t) in toks.iter().enumerate() {
                let member = i > 0 && matches!(toks[i - 1].kind, TokenKind::Dot | TokenKind::ColonColon);
                if t.kind == TokenKind::Ident && !member && (self.names.props.contains(&t.text) || self.names.lets.contains(&t.text) || self.names.fns.contains(&t.text)) {
                    return Err(UiError { lo: r.0 + t.span.start as usize, hi: r.0 + t.span.end as usize, msg: format!("the default of prop `{}` can't use `{}`", p.name, t.text), help: Some("a default is a constant: a literal or a `const` of this component".into()) });
                }
            }
        }
        let mut ev_seen: Vec<&str> = Vec::new();
        for (e, r) in &sc.events {
            if ev_seen.contains(&e.as_str()) {
                return err(r.0, r.1, format!("event `{e}` is declared twice"));
            }
            ev_seen.push(e);
            self.names.events.push(e.clone());
        }
        let mut fields: Vec<String> = sc.events.iter().map(|e| e.0.clone()).collect();
        for p in &sc.ev_params {
            let ty = self.src[p.ty.0..p.ty.1].trim();
            if !EV_TYPES.contains(&ty) {
                return err(p.ty.0, p.ty.1, format!("event payload type `{ty}` isn't supported")).map_err(|e| e.help(format!("payload types: {}", EV_TYPES.join(", "))));
            }
            let f = format!("{}_{}", p.ev, p.name);
            if fields.contains(&f) {
                return err(p.name_r.0, p.name_r.1, format!("payload `{}` of `{}` collides with another event field `{f}`", p.name, p.ev));
            }
            fields.push(f);
            self.names.payload.push((p.ev.clone(), p.name.clone()));
        }
        let (mut cn, mut ifn, mut en) = (0, 0, 0);
        number(&c.mk.nodes, &mut cn, &mut ifn, &mut en);

        // script fns first: they may be used from markup but are independent of it
        let mut fns = Out::with_ctx(self.fi);
        for f in &sc.fns {
            self.script_fn(f, &mut fns)?;
        }

        // markup
        self.upd.ind = 2;
        self.pnt.ind = 2;
        if self.slots.pass.contains_key(&c.name) {
            self.pass_root = pass::single_root(&c.mk.nodes).map(addr).unwrap_or(0);
        }
        let mut j = 0;
        self.seq(&c.mk.nodes, &mut j, 0, &None)?;
        self.act_sweep(0, "this")?;
        self.assemble(fns)
    }

    pub(super) fn script_fn(&mut self, f: &markup::FnDef, o: &mut Out) -> R<()> {
        for (n, r) in &f.param_names {
            self.check_local(n, *r)?;
        }
        let saved = self.locals.len();
        self.locals.extend(f.param_names.iter().map(|x| x.0.clone()));
        let mut body = Out::with_ctx(self.fi);
        body.set(self.fi, f.name_r.0, f.name_r.1);
        let mut used = BTreeSet::new();
        let rw = Rw { src: self.src, fi: self.fi, names: &self.names, locals: &self.locals, rename: &self.rename, emit_ok: f.name != "onDestroy", sev: true, calls_emit: f.name != "onDestroy" };
        rewrite(&rw, &mut body, &mut used, f.body.0, f.body.1, false)?;
        self.locals.truncate(saved);
        let name = &self.c.id;
        o.set(self.fi, f.name_r.0, f.name_r.1);
        o.s(&format!("\n    fn {}(this: &{}", f.name, name));
        if f.params.0 < f.params.1 {
            o.s(", ");
            o.copy(self.fi, self.src, f.params.0, f.params.1);
        }
        o.s(")");
        if let Some(r) = f.ret {
            o.copy(self.fi, self.src, r.0, r.1);
        }
        o.s(" {\n");
        // `d` in a script fn: the Dom the component was last built with
        let toks = lex(&self.src[f.body.0..f.body.1], FileId(0)).unwrap_or_default();
        if toks.iter().enumerate().any(|(i, t)| t.kind == TokenKind::Ident && t.text == "d" && (i == 0 || !matches!(toks[i - 1].kind, TokenKind::Dot | TokenKind::ColonColon))) {
            o.s("        let d = (this.dm as ptr) as &Dom;\n");
        }
        for p in &used {
            o.s(&format!("        let {p} = this.p.{p};\n"));
        }
        o.append(body);
        o.s("\n    }\n");
        Ok(())
    }

    // ---- the walk

    pub(super) fn seq<'n>(&mut self, nodes: impl IntoIterator<Item = &'n Node>, j: &mut usize, depth: usize, base: &Base) -> R<()> {
        // `{#let}`s end with the block that holds them
        let (nl, nr) = (self.lets.len(), self.rename.len());
        let r = self.seq_in(nodes, j, depth, base);
        self.lets.truncate(nl);
        self.rename.truncate(nr);
        r
    }

    /// `{#let name = expr}`: a J3 local (own name, so sibling blocks never collide) for the rest of the block.
    pub(super) fn let_node(&mut self, l: &LetNode) -> R<()> {
        if self.lets.iter().any(|x| x.0 == l.name) {
            return err(l.name_r.0, l.name_r.1, format!("`{}` is already declared by an earlier `{{#let}}` in this scope", l.name));
        }
        self.check_local(&l.name, l.name_r)?;
        let j3 = format!("lt{}", self.nk());
        self.upd.set(self.fi, l.range.0, l.range.1);
        self.upd.pad();
        self.upd.s(&format!("let {j3} = "));
        self.expr(Mode::U, l.expr)?;
        self.upd.s(";\n");
        self.lets.push((l.name.clone(), j3.clone(), l.expr));
        self.rename.push((l.name.clone(), j3));
        Ok(())
    }

    pub(super) fn seq_in<'n>(&mut self, nodes: impl IntoIterator<Item = &'n Node>, j: &mut usize, depth: usize, base: &Base) -> R<()> {
        for n in nodes {
            match n {
                Node::Let(l) => self.let_node(l)?,
                Node::Text { parts, range } => {
                    self.upd.set(self.fi, range.0, range.1);
                    self.upd.pad();
                    self.upd.s("d.text(");
                    self.string(Mode::U, parts, *range)?;
                    self.upd.s(");\n");
                }
                Node::Elem(e) => {
                    let slot = Self::slot(base, *j);
                    *j += 1;
                    if e.comp {
                        self.comp(e, slot, depth)?;
                    } else if e.tag == "slot" {
                        self.slot_elem(e, slot, depth)?;
                    } else {
                        self.elem(e, slot, depth)?;
                    }
                }
                Node::If(i) => self.ifnode(i, j, depth, base)?,
                Node::Each(e) => {
                    let slot = Self::slot(base, *j);
                    *j += 1;
                    self.each(e, slot, depth)?;
                }
            }
        }
        Ok(())
    }

    pub fn accepted(tag: &str) -> Vec<&'static str> {
        let mut v = vec!["id", "class", "class:name", "on:click", "on:mousedown", "on:mousedown|stop", "on:mouseup", "on:mouseup|stop", "on:mousemove", "on:wheel", "on:wheel|stop", "on:close", "on:contextmenu", "on:contextmenu|stop", "popover", "bind:tooltip", "closedby", "title", "left", "top", "panel", "panel-handle", "panel-resize", "window-drag", "menuitem", "tabindex", "aria-label", "aria-live", "aria-describedby", "aria-selected", "role", "focus-group", "nav-up", "nav-down", "nav-left", "nav-right", "transition:fade", "transition:fly", "transition:slide", "transition:scale", "in:fade", "in:fly", "in:slide", "in:scale", "out:fade", "out:fly", "out:slide", "out:scale", "fly-x", "fly-y", "scale-from", "slide-axis", "transition-easing", "animate:flip", "animate-easing", "anim-delay", "virtual", "row-height", "est-height", "stick-to-bottom", "keep", "bind:at-end", "bind:scrollX", "bind:scrollY", "grid", "on:edit", "draggable", "drag-data", "drag-id", "drag-ghost", "on:dragstart", "on:dragend", "on:dragenter", "on:dragleave", "on:dragover", "on:drop", "bind:this"];
        match tag {
            "dialog" => v.extend(["modal", "autoselect"]),
            "label" => v.push("for"),
            "button" => v.extend(["disabled", "default"]),
            "input" | "textarea" => v.extend(["disabled", "type", "min", "max", "step", "value", "high", "checked", "placeholder", "maxlength", "rows", "rich", "noprimary", "name", "bind:value", "bind:spans", "bind:high", "bind:checked", "bind:group", "on:input", "on:change", "on:submit", "on:trigger", "triggers"]),
            "select" => v.extend(["disabled", "multiple", "value", "placeholder", "bind:value", "on:input"]),
            "option" => v.extend(["disabled", "value"]),
            "canvas" => v.push("on:draw"),
            "img" => v.extend(["src", "alt", "sprite", "frame", "fps"]),
            "progress" => v.extend(["value", "max"]),
            "meter" => v.extend(["value", "min", "max", "low", "high", "optimum"]),
            "glyph" => v.extend(["action", "key", "pad"]),
            "icon" => v.push("name"),
            "details" => v.extend(["open", "bind:open", "on:toggle"]),
            "svg" => v.push("viewBox"),
            "path" => v.extend(["d", "fill", "fill-rule", "stroke", "stroke-width"]),
            _ => {}
        }
        v
    }
}
