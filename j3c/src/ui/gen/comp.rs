use super::*;

impl<'a> G<'a> {
    /// `bind:` takes a place: a name, then `.field` and `[index]` steps.
    pub(super) fn place_check(&self, r: Range) -> R<()> {
        let bad = || err(r.0, r.1, "`bind:` needs a place: a name, a field, or an index").map_err(|e| e.help(format!("found `{}`", &self.src[r.0..r.1])));
        let toks = lex(&self.src[r.0..r.1], FileId(0)).map_err(|e| UiError { lo: r.0 + e.span.start as usize, hi: r.0 + e.span.end as usize, msg: e.message, help: None })?;
        let mut k = 0;
        if !matches!(toks.first().map(|t| t.kind), Some(TokenKind::Ident) | Some(TokenKind::This)) {
            return bad();
        }
        k += 1;
        while toks[k].kind != TokenKind::Eof {
            match toks[k].kind {
                TokenKind::Dot if toks[k + 1].kind == TokenKind::Ident => k += 2,
                TokenKind::LBracket => {
                    let mut depth = 0;
                    while toks[k].kind != TokenKind::Eof {
                        match toks[k].kind {
                            TokenKind::LBracket => depth += 1,
                            TokenKind::RBracket => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        k += 1;
                    }
                    if toks[k].kind == TokenKind::Eof {
                        return bad();
                    }
                    k += 1;
                }
                _ => return bad(),
            }
        }
        Ok(())
    }

    pub(super) fn comp(&mut self, e: &Elem, slot: String, depth: usize) -> R<()> {
        let Some(child) = self.all.iter().find(|c| c.name == e.tag) else {
            let known = self.all.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", ");
            return Err(UiError { lo: e.open.0, hi: e.open.0 + 1 + e.tag.len(), msg: format!("unknown component `<{}>`", e.tag), help: Some(format!("known components: {known}")) });
        };
        let id = e.id.get();
        self.insts.push((id, child.id.clone(), self.own.last().copied().unwrap_or(0)));
        let inst = format!("{}.c{id}", self.owner());
        // validate attributes
        let cprops: Vec<&str> = child.sc.props.iter().filter(|p| p.hidden.is_none()).map(|p| p.name.as_str()).collect();
        let cevents: Vec<&str> = child.sc.events.iter().map(|p| p.0.as_str()).collect();
        for a in &e.attrs {
            if (a.name == "slot" && self.slots.slotted.contains(&addr(e))) || pass::is_pass(a) {
                continue; // `class` / `style:` go to the child's root element (`pass`)
            }
            if let Some(ev) = a.name.strip_prefix("on:") {
                if !cevents.contains(&ev) {
                    let help = if cevents.is_empty() { format!("`{}` declares no events", e.tag) } else { format!("`{}` events: {}", e.tag, cevents.join(", ")) };
                    return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`<{}>` has no event `{ev}`", e.tag), help: Some(help) });
                }
            } else if a.name == "animate:flip" || a.name == "animate-easing" {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`{}` goes on the row element, not a component (`<{}>`)", a.name, e.tag), help: Some("wrap it: `{#each xs as x (x.id)}<div animate:flip><Row .../></div>{/each}`".into()) });
            } else if a.name == "bind:this" {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: "`bind:this` on a component isn't supported yet".into(), help: Some("put `bind:this` on an element inside it, or on a wrapper `<div>` around the component".into()) });
            } else if a.name.contains(':') {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`{}` isn't supported on a component", a.name), help: Some("only props and `on:event` handlers go on `<Component>`".into()) });
            } else if !cprops.contains(&a.name.as_str()) {
                let help = if cprops.is_empty() { format!("`{}` takes no props", e.tag) } else { format!("`{}` props: {}", e.tag, cprops.join(", ")) };
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown prop `{}` for `<{}>`", a.name, e.tag), help: Some(help) });
            }
        }
        for p in child.sc.props.iter().filter(|p| p.hidden.is_none() && p.default.is_none()).map(|p| &p.name) {
            if !e.attrs.iter().any(|a| a.name == *p) {
                return Err(UiError { lo: e.open.0, hi: e.open.1, msg: format!("missing prop `{p}` for `<{}>`", e.tag), help: Some(format!("`{}` props: {}", e.tag, cprops.join(", "))) });
            }
        }
        let site = self.slots.by_addr.get(&addr(e)).copied();
        if let Some(si) = site {
            let slots = self.slots;
            for name in &slots.list[si].names {
                let nodes: Vec<&Node> = e.children.iter().filter(|n| slot_of(n) == *name).collect();
                self.slot_method(slots.list[si].id, name, &nodes, depth, e.open)?;
            }
        }
        let k = self.nk();
        self.upd.set(self.fi, e.open.0, e.open.1);
        self.upd.line(&format!("let s{k} = d.enter({slot});"));
        if let Some(si) = site {
            let iv = self.idx.last().cloned().unwrap_or_else(|| "0".into());
            self.upd.line(&format!("{inst}.slotSite = {};", self.slots.list[si].id));
            self.upd.line(&format!("{inst}.slotOwn = this as int;"));
            self.upd.line(&format!("{inst}.slotI = {iv};"));
        }
        self.upd.pad();
        self.upd.s(&format!("let e{k} = {inst}.update(d, {}Props {{ ", child.id));
        for (i, p) in child.sc.props.iter().enumerate() {
            if i > 0 {
                self.upd.s(", ");
            }
            if p.hidden.is_some() {
                // `provide`d by this component (a `let` or a prop) or handed down to it
                self.upd.set(self.fi, e.open.0, e.open.1);
                if self.names.lets.contains(&p.name) {
                    self.upd.s(&format!("{0}: this.{0}", p.name));
                } else {
                    self.used_u.insert(p.name.clone());
                    self.upd.s(&format!("{0}: {0}", p.name));
                }
                continue;
            }
            let Some(a) = e.attrs.iter().find(|a| a.name == p.name) else {
                // omitted: the child's default, a constant, sent every frame like any prop (the child's `const`s renamed)
                let r = p.default.expect("required props are checked above");
                let names = Names { consts: child.sc.consts.iter().map(|l| (l.name.clone(), format!("{}_{}", child.id, l.name))).collect(), ..Names::default() };
                let rw = Rw { src: &self.files[child.fi].text, fi: child.fi, names: &names, locals: &[], rename: &[], emit_ok: false, sev: false, calls_emit: false };
                self.upd.s(&format!("{}: ", p.name));
                rewrite(&rw, &mut self.upd, &mut BTreeSet::new(), r.0, r.1, false)?;
                continue;
            };
            self.upd.set(self.fi, a.range.0, a.range.1);
            self.upd.s(&format!("{}: ", p.name));
            match &a.value {
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                Value::Str(parts) => self.string(Mode::U, parts, a.range)?,
                Value::None => self.upd.s("true"),
            }
        }
        self.upd.set(self.fi, e.open.0, e.open.1);
        if let Some(pi) = self.slots.pass.get(&child.name).copied() {
            self.pass_args(e, &pi, !child.sc.props.is_empty())?;
        }
        self.upd.s(" });\n");
        self.upd.line(&format!("d.leave(s{k});"));
        for a in &e.attrs {
            if let Some(ev) = a.name.strip_prefix("on:") {
                let Value::Expr(r) = a.value else {
                    return err(a.range.0, a.range.1, format!("`{}` needs `={{ statements }}`", a.name));
                };
                self.upd.set(self.fi, a.range.0, a.range.1);
                self.upd.line(&format!("if e{k}.{ev} {{"));
                self.upd.ind += 1;
                let nren = self.rename.len();
                let ps: Vec<String> = child.sc.ev_params.iter().filter(|p| p.ev == ev).map(|p| format!("{0}: e{k}.{ev}_{0}", p.name)).collect();
                if !ps.is_empty() {
                    self.upd.line(&format!("let evt{k}_{ev} = {}Event_{ev} {{ {} }};", child.id, ps.join(", ")));
                    self.rename.push(("event".into(), format!("evt{k}_{ev}")));
                }
                self.stmts(Mode::U, r, true)?;
                self.rename.truncate(nren);
                self.upd.ind -= 1;
                self.upd.line("}");
            }
        }
        self.pnt.set(self.fi, e.open.0, e.open.1);
        self.pnt.line(&format!("let s{k} = d.enter({slot});"));
        if site.is_some() {
            self.pnt.line(&format!("{inst}.slotOwn = this as int;"));
        }
        self.pnt.line(&format!("{inst}.paint(d);"));
        self.pnt.line(&format!("d.leave(s{k});"));
        Ok(())
    }

    /// What the uses of this component passed, applied after the root's own attributes (a passed style wins).
    pub(super) fn pass_apply(&mut self, e: &Elem, n: &str) {
        let p = self.slots.pass[&self.c.name];
        self.upd.set(self.fi, e.open.0, e.open.1);
        for l in pass::apply(&p, n) {
            self.upd.line(&l);
        }
    }

    /// The hidden `j3pass_*` fields of the `Props` literal for a use of a component that takes `class` / `style:`:
    /// the parent's scope (only with a class), the class text, and the `style:` values (see `pass`).
    pub(super) fn pass_args(&mut self, e: &Elem, pi: &Pass, comma: bool) -> R<()> {
        let at = |a: &Attr| a.range;
        let cls: Vec<&Attr> = e.attrs.iter().filter(|a| a.name == "class" || a.name.starts_with("class:")).collect();
        let pre = pass::PREFIX;
        if comma {
            self.upd.s(", ");
        }
        self.upd.s(&format!("{pre}_scope: {}, {pre}_cls: ", if cls.is_empty() { "0".to_string() } else { format!("SCOPE_{}", self.c.id) }));
        let mut parts: Vec<Part> = Vec::new();
        let mut conds: Vec<(&str, markup::Range)> = Vec::new();
        for a in &cls {
            if let Some(c) = a.name.strip_prefix("class:") {
                match a.value {
                    Value::Expr(r) if !c.is_empty() => conds.push((c, r)),
                    _ => return err(a.range.0, a.range.1, format!("`{}` needs a condition: {}={{condition}}", a.name, a.name)),
                }
            } else {
                match &a.value {
                    Value::Str(p) => parts.extend(p.iter().cloned()),
                    Value::None => {}
                    _ => return Err(UiError { lo: a.range.0, hi: a.range.1, msg: "`class` takes text".into(), help: Some("`class=\"a b{x}\"`; switch a class with `class:name={condition}`".into()) }),
                }
            }
        }
        if conds.is_empty() {
            let r = cls.first().map(|a| at(a)).unwrap_or(e.open);
            self.string(Mode::U, &parts, r)?;
        } else {
            // one template: the written words, then each `class:x` as `x` or nothing
            self.upd.s("`");
            for p in &parts {
                match p {
                    Part::Lit(l) if l.contains('`') || l.contains('\n') => return err(e.open.0, e.open.1, "text next to `{...}` can't hold a backtick"),
                    Part::Lit(l) => self.upd.s(l),
                    Part::Hole(r) => {
                        self.upd.s("{");
                        self.rw(Mode::U, r.0, r.1, true, true, false)?;
                        self.upd.s("}");
                    }
                }
            }
            for (c, r) in conds {
                self.upd.s(" {if ");
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" {{ {} }} else {{ \"\" }}}}", lit(c)));
            }
            self.upd.s("`");
        }
        let styles: Vec<&Attr> = e.attrs.iter().filter(|a| a.name.starts_with("style:")).collect();
        for k in 0..pi.styles {
            self.upd.s(", ");
            let Some(a) = styles.get(k) else {
                self.upd.s(&pass::unused(k));
                continue;
            };
            let prop = &a.name["style:".len()..];
            let num = self.style_val(a, prop)?;
            self.upd.s(&format!("{pre}_n{k}: {}, ", lit(prop)));
            match &a.value {
                Value::Expr(r) if num => {
                    self.upd.s(&format!("{pre}_k{k}: 1, {pre}_f{k}: f32("));
                    self.expr(Mode::U, *r)?;
                    self.upd.s(&format!("), {pre}_s{k}: \"\""));
                }
                Value::Expr(r) => {
                    self.upd.s(&format!("{pre}_k{k}: 2, {pre}_f{k}: 0.0, {pre}_s{k}: "));
                    self.expr(Mode::U, *r)?;
                }
                Value::Str(p) => {
                    self.upd.s(&format!("{pre}_k{k}: 2, {pre}_f{k}: 0.0, {pre}_s{k}: "));
                    self.string(Mode::U, p, a.range)?;
                }
                Value::None => unreachable!("checked by style_val"),
            }
        }
        Ok(())
    }

    /// The content a use of a component passes to one of its slots, as a method pair on this
    /// component (`this` is the owner, so the content sees its props, lets and fns). The child
    /// calls it through `slotOwn`; an enclosing `{#each}` item is re-bound from the index `slotI`.
    pub(super) fn slot_method(&mut self, site: usize, name: &str, nodes: &[&Node], depth: usize, at: Range) -> R<()> {
        self.owns_slot = true;
        let (mut upd, mut pnt) = (Out::with_ctx(self.fi), Out::with_ctx(self.fi));
        upd.ind = 2;
        pnt.ind = 2;
        let (su, sp) = (std::mem::replace(&mut self.upd, upd), std::mem::replace(&mut self.pnt, pnt));
        let (uu, up) = (std::mem::take(&mut self.used_u), std::mem::take(&mut self.used_p));
        let mut res = Ok(());
        let ectx = self.ectx.clone();
        if self.in_virt > 0 {
            res = err(at.0, at.1, "slot content inside a `virtual` list isn't supported");
        } else if ectx.len() > 1 {
            res = err(at.0, at.1, "slot content inside two nested `{#each}` blocks isn't supported yet");
        }
        for (h, k, eid, rows) in &ectx {
            res = res.and_then(|_| self.rebind(h, *k, *eid, *rows));
        }
        // the `{#let}`s in scope at the use are computed again here (this is another fn)
        for (_, j3, ex) in self.lets.clone() {
            res = res.and_then(|_| {
                self.upd.pad();
                self.upd.s(&format!("let {j3} = "));
                self.expr(Mode::U, ex)?;
                self.upd.s(";\n");
                Ok(())
            });
        }
        let mut j = 0;
        res = res.and_then(|_| self.seq(nodes.iter().copied(), &mut j, depth, &None));
        let (bu, bp) = (std::mem::replace(&mut self.upd, su), std::mem::replace(&mut self.pnt, sp));
        let (used_u, used_p) = (std::mem::replace(&mut self.used_u, uu), std::mem::replace(&mut self.used_p, up));
        res?;
        let iv = self.idx.last().cloned().unwrap_or_else(|| "slotI".into());
        let n = san(if name.is_empty() { "default" } else { name });
        let id = self.c.id.clone();
        for (m, body, used) in [("update", bu, used_u), ("paint", bp, used_p)] {
            let mut o = Out::with_ctx(self.fi);
            o.set(self.fi, at.0, at.1);
            o.s(&format!("\n    fn slot{site}_{n}_{m}(this: &{id}, d: &Dom, {iv}: int) {{\n"));
            if m == "update" && !self.c.sc.events.is_empty() {
                o.s("        let ev = &this.sev;\n");
            }
            for p in &used {
                o.s(&format!("        let {p} = this.p.{p};\n"));
            }
            o.append(body);
            o.s("    }\n");
            self.slot_fns.append(o);
        }
        // the child stopped rendering the slot: the content's state dies with it, as in Svelte
        let (mut dead, mut fresh) = (Vec::new(), Vec::new());
        for n in nodes {
            self.resets(std::slice::from_ref(*n), true, &mut dead);
            self.resets(std::slice::from_ref(*n), false, &mut fresh);
        }
        let mut o = Out::with_ctx(self.fi);
        o.s(&format!("\n    fn slot{site}_{n}_leave(this: &{id}, d: &Dom, {iv}: int) {{\n"));
        for (_, _, eid, rows) in &ectx {
            if *rows {
                o.s(&format!("        let r{eid} = this.e{eid}.items[{iv}];\n"));
            }
        }
        for l in dead.iter().chain(&fresh) {
            o.s(&format!("        {l}\n"));
        }
        o.s("    }\n");
        self.slot_fns.append(o);
        Ok(())
    }

    /// The loop item of an enclosing `{#each}`, from its index (the slot method's parameter).
    pub(super) fn rebind(&mut self, h: &EachHead, k: usize, eid: usize, rows: bool) -> R<()> {
        let iv = self.idx.last().cloned().unwrap_or_default();
        let item = &h.item.0;
        if rows {
            self.upd.line(&format!("let r{eid} = this.e{eid}.items[{iv}];"));
            self.pnt.line(&format!("let r{eid} = this.e{eid}.items[{iv}];"));
        }
        if let Some((a, _)) = h.range {
            if self.src[a.0..a.1].trim() != "0" {
                self.upd.pad();
                self.upd.s(&format!("let a{k} = "));
                self.expr(Mode::U, a)?;
                self.upd.s(";\n");
                self.upd.line(&format!("let {item} = a{k} + {iv};"));
            }
        } else {
            self.upd.pad();
            self.upd.s(&format!("let x{k} = "));
            self.expr(Mode::U, h.expr)?;
            self.upd.s(";\n");
            self.upd.line(&format!("let {item} = {}x{k}[{iv}];", if h.by_ref { "&" } else { "" }));
        }
        Ok(())
    }

    /// `<slot>` / `<slot name="x">fallback</slot>`: run the content the owner passed, else the fallback.
    pub(super) fn slot_elem(&mut self, e: &Elem, slot: String, depth: usize) -> R<()> {
        self.has_slot = true;
        let name = e.attrs.iter().find(|a| a.name == "name").and_then(|a| lit_value(&a.value)).unwrap_or_default();
        let k = self.nk();
        let slots = self.slots;
        let n = san(if name.is_empty() { "default" } else { &name });
        // every use of this component that fills this slot
        let calls: Vec<(usize, String)> = slots.list.iter().filter(|s| s.child == self.c.name && s.names.contains(&name)).map(|s| (s.id, s.owner.clone())).collect();
        for m in [Mode::U, Mode::P] {
            let call = if m == Mode::U { "update" } else { "paint" };
            let fi = self.fi;
            let o = self.out(m);
            o.set(fi, e.open.0, e.open.1);
            o.line(&format!("let s{k} = d.enter({slot});"));
            if m == Mode::U && !calls.is_empty() {
                o.line(&format!("this.slotNow{k} = true;"));
            }
            for (i, (id, owner)) in calls.iter().enumerate() {
                o.line(&format!("{}if this.slotSite == {id} {{", if i > 0 { "} else " } else { "" }));
                o.ind += 1;
                o.line(&format!("((this.slotOwn as ptr) as &{owner}).slot{id}_{n}_{call}(d, this.slotI);"));
                o.ind -= 1;
            }
            if !calls.is_empty() {
                o.line("} else {");
                o.ind += 1;
            }
        }
        if !calls.is_empty() {
            self.slot_marks.push((k, n.clone(), calls.clone()));
        }
        let mut j = 0;
        self.seq(&e.children, &mut j, depth, &None)?;
        for m in [Mode::U, Mode::P] {
            let o = self.out(m);
            if !calls.is_empty() {
                o.ind -= 1;
                o.line("}");
            }
            o.line(&format!("d.leave(s{k});"));
        }
        Ok(())
    }
}
