use super::*;

impl<'a> G<'a> {
    pub(super) fn ifnode(&mut self, i: &IfNode, j: &mut usize, depth: usize, base: &Base) -> R<()> {
        let id = i.id.get();
        let stateful = id != 0;
        let (lo, hi) = i.range;
        self.upd.set(self.fi, lo, hi);
        let state;
        let mut bv = String::new();
        // `{#match X}`: X is evaluated once
        let mut mv = String::new();
        if let Some(sc) = i.scrut {
            mv = format!("m{}", self.nk());
            self.upd.pad();
            self.upd.s(&format!("let {mv} = "));
            self.expr(Mode::U, sc)?;
            self.upd.s(";\n");
        }
        if stateful {
            self.ifs.push((id, self.own.last().copied().unwrap_or(0)));
            state = format!("{}.if{id}", self.owner());
            let k = self.nk();
            bv = format!("b{k}");
            self.upd.line(&format!("let {bv} = -1;"));
            for (bi, b) in i.branches.iter().enumerate() {
                self.upd.pad();
                if bi > 0 {
                    self.upd.s("} else ");
                }
                self.branch_head(b, &mv, i.choice.as_deref().map(|set| (set, bi)))?;
                self.upd.ind += 1;
                self.upd.line(&format!("{bv} = {bi};"));
                self.upd.ind -= 1;
            }
            self.upd.line("}");
            self.upd.line(&format!("if {bv} != {state} {{"));
            self.upd.ind += 1;
            // leaving a branch destroys its components (onDestroy), entering one starts them fresh
            let mut first = true;
            for (bi, b) in i.branches.iter().enumerate() {
                let mut dead = Vec::new();
                self.resets(&b.nodes, true, &mut dead);
                if dead.is_empty() {
                    continue;
                }
                self.upd.line(&format!("{}if {state} == {bi} {{", if first { "" } else { "} else " }));
                first = false;
                self.upd.ind += 1;
                for r in dead {
                    self.upd.line(&r);
                }
                self.upd.ind -= 1;
            }
            if !first {
                self.upd.line("}");
            }
            self.upd.line(&format!("{state} = {bv};"));
            let mut first = true;
            for (bi, b) in i.branches.iter().enumerate() {
                let mut resets = Vec::new();
                self.resets(&b.nodes, false, &mut resets);
                if resets.is_empty() {
                    continue;
                }
                let pre = if first { "" } else { "} else " };
                first = false;
                self.upd.line(&format!("{pre}if {bv} == {bi} {{"));
                self.upd.ind += 1;
                for r in resets {
                    self.upd.line(&r);
                }
                self.upd.ind -= 1;
            }
            if !first {
                self.upd.line("}");
            }
            self.upd.ind -= 1;
            self.upd.line("}");
        }
        for (bi, b) in i.branches.iter().enumerate() {
            self.upd.set(self.fi, lo, hi);
            self.upd.pad();
            if bi > 0 {
                self.upd.s("} else ");
            }
            if stateful {
                self.upd.s(&format!("if {bv} == {bi} {{\n"));
            } else {
                self.branch_head(b, &mv, i.choice.as_deref().map(|set| (set, bi)))?;
            }
            self.upd.ind += 1;
            self.seq(&b.nodes, j, depth, base)?;
            self.upd.ind -= 1;
        }
        self.upd.set(self.fi, lo, hi);
        self.upd.line("}");
        Ok(())
    }

    /// `if cond {`, or for a `{#match}` case `if m == a || m == b {`; an else branch opens a bare `{`.
    pub(super) fn branch_head(&mut self, b: &markup::Branch, mv: &str, choice: Option<(&str, usize)>) -> R<()> {
        if let Some((set, index)) = choice {
            self.upd.s(&format!("if @uiChoice({set}, {mv}, {index})"));
        } else if let Some(c) = b.cond {
            self.upd.s("if ");
            self.expr(Mode::U, c)?;
        } else if !b.vals.is_empty() {
            self.upd.s("if ");
            for (q, v) in b.vals.iter().enumerate() {
                if q > 0 {
                    self.upd.s(" || ");
                }
                self.upd.s(&format!("{mv} == "));
                self.expr(Mode::U, *v)?;
            }
        } else {
            self.upd.s("{\n");
            return Ok(());
        }
        self.upd.s(" {\n");
        Ok(())
    }

    /// Statements that reset what a branch holds when it is entered (Svelte destroys it), or with
    /// `destroy` the `destroy()` calls for its components when it is left. The branch's own level only:
    /// rows of a list in it are destroyed (and dropped) as a whole.
    pub(super) fn resets(&self, nodes: &[Node], destroy: bool, out: &mut Vec<String>) {
        let own = self.owner();
        for n in nodes {
            match n {
                Node::Text { .. } | Node::Let(_) => {}
                Node::Elem(e) => {
                    if e.comp {
                        let id = e.id.get();
                        if destroy {
                            out.push(format!("{own}.c{id}.destroy();"));
                        } else {
                            out.push(format!("{own}.c{id}.j3Reset();"));
                        }
                    }
                    self.resets(&e.children, destroy, out);
                }
                Node::If(i) => {
                    let id = i.id.get();
                    if id != 0 && !destroy {
                        out.push(format!("{own}.if{id} = -1;"));
                    }
                    for b in &i.branches {
                        self.resets(&b.nodes, destroy, out);
                    }
                }
                Node::Each(e) => {
                    let id = e.id.get();
                    let Ok(h) = parse_each_head(self.src, e.head) else { continue };
                    if h.key.is_none() && !holds(&e.body) {
                        continue;
                    }
                    if destroy {
                        out.push(format!("for q{id} in 0..{own}.e{id}.len {{ {own}.e{id}.items[q{id}].destroy(); }}"));
                    } else {
                        out.push(format!("for q{id} in 0..{own}.e{id}.len {{ {own}.fp{id}.push({own}.e{id}.items[q{id}]); }}"));
                        out.push(format!("{own}.e{id}.len = 0;"));
                        if h.key.is_some() {
                            out.push(format!("{own}.kd{id}.reset();"));
                        }
                    }
                }
            }
        }
    }

    /// The struct that holds the state of what is being built: the component, or the `{#each}` row.
    pub(super) fn owner(&self) -> String {
        match self.own.last() {
            Some(id) => format!("r{id}"),
            None => "this".into(),
        }
    }

    pub(super) fn each(&mut self, e: &EachNode, slot: String, depth: usize) -> R<()> {
        let src = self.src;
        let lo = e.range.0;
        let head = parse_each_head(src, e.head)?;
        self.check_local(&head.item.0, head.item.1)?;
        if let Some((n, r)) = &head.index {
            self.check_local(n, *r)?;
        }
        if head.by_ref && head.range.is_some() {
            return err(head.item.1 .0 - 1, head.item.1 .1, "a range loop gives numbers; `&x` is for a list of structs").map_err(|e| e.help("`{#each items as &item}`"));
        }
        if head.range.is_some() && head.index.is_some() {
            let r = head.index.as_ref().unwrap().1;
            return Err(UiError { lo: r.0, hi: r.1, msg: "a range loop has no index".into(), help: Some("the item is the index: `{#each 0..n as i}`".into()) });
        }
        let id = e.id.get();
        let keyed = head.key.is_some();
        let rows = keyed || holds(&e.body);
        let own = self.owner();
        let virt = self.virt.take();
        self.eachs.push(EachDef { id, container: self.own.last().copied().unwrap_or(0), keyed, rows, virt: virt.is_some() });
        self.uses_virt |= virt.is_some();
        self.uses_list |= rows;
        self.uses_keyed |= keyed;
        let k = self.nk();
        let sv = format!("s{k}");
        let mv = format!("m{k}");
        self.upd.set(self.fi, lo, e.head.1);
        self.upd.line(&format!("let {sv} = d.enter({slot});"));
        let item = head.item.0.clone();
        let amp = if head.by_ref { "&" } else { "" };
        let mut bind: Option<String> = None;
        // the iteration variable and where `mv` comes from
        let iv: String;
        if let Some((a, b)) = head.range {
            let zero = src[a.0..a.1].trim() == "0";
            self.upd.pad();
            if zero {
                self.upd.s(&format!("let {mv} = "));
                self.expr(Mode::U, b)?;
                self.upd.s(";\n");
                iv = item.clone();
            } else {
                self.upd.s(&format!("let a{k} = "));
                self.expr(Mode::U, a)?;
                self.upd.s(";\n");
                self.upd.pad();
                self.upd.s(&format!("let {mv} = "));
                self.expr(Mode::U, b)?;
                self.upd.s(&format!(" - a{k};\n"));
                iv = format!("i{k}");
                bind = Some(format!("let {item} = a{k} + {iv};"));
            }
        } else {
            self.upd.pad();
            self.upd.s(&format!("let x{k} = "));
            self.expr(Mode::U, head.expr)?;
            self.upd.s(";\n");
            if let Some(va) = &virt {
                // the rows alive (`lo..hi` plus kept ones); the box pads for the rest, and `vl.at(i)` is a row's index
                let vl = format!("{own}.vl{id}");
                self.upd.pad();
                self.upd.s(&format!("{vl}.cfg(f32("));
                self.virt_num(&va.rh, "0.0")?;
                self.upd.s("), f32(");
                self.virt_num(&va.est, "40.0")?;
                self.upd.s(&format!("), {}, {});\n", va.stick, va.keep.is_some()));
                if let Some(r) = va.keep {
                    if head.key_int {
                        return err(r.0, r.1, "`keep` lists text keys, but this `{#each}` has an int key (`: int`)").map_err(|x| x.help("drop `: int` from the key, or drop `keep`"));
                    }
                    self.upd.pad();
                    self.upd.s(&format!("{vl}.keep = "));
                    self.expr(Mode::U, r)?;
                    self.upd.s(";\n");
                }
                if let Some(r) = va.at_end {
                    self.upd.pad();
                    self.upd.s("if ");
                    self.expr(Mode::U, r)?;
                    self.upd.s(&format!(" && !{vl}.atEnd {{ {vl}.toEnd = true; }}\n"));
                }
                {
                    // the list changed: carry the table over. A list that only grew at its tail (checked on its
                    // first and last old row) formats just the new keys; anything else formats every key once.
                    let key = head.key.expect("virtual lists are keyed");
                    let ki_ = head.key_int;
                    let at = src[..lo].matches('\n').count() + 1;
                    let item_n = item.clone();
                    let index_n = head.index.as_ref().map(|x| x.0.clone());
                    self.upd.line(&format!("if {vl}.stale(x{k}.len) {{"));
                    self.upd.ind += 1;
                    self.upd.line(&format!("let ap{k} = {vl}.grew(x{k}.len);"));
                    self.upd.line(&format!("if ap{k} {{"));
                    self.upd.ind += 1;
                    self.upd.line(&format!("{vl}.pk.begin();"));
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.pk.add"), "0", &format!("{k}a"))?;
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.pk.add"), &format!("{vl}.n - 1"), &format!("{k}b"))?;
                    self.upd.line(&format!("ap{k} = {vl}.probeOk();"));
                    self.upd.ind -= 1;
                    self.upd.line("}");
                    self.upd.line(&format!("if ap{k} {{"));
                    self.upd.ind += 1;
                    self.upd.line(&format!("for kq{k} in {vl}.n..x{k}.len {{"));
                    self.upd.ind += 1;
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.all.push"), &format!("kq{k}"), &format!("{k}c"))?;
                    self.upd.ind -= 1;
                    self.upd.set(self.fi, lo, e.head.1);
                    self.upd.line("}");
                    self.upd.line(&format!("{vl}.appended(x{k}.len);"));
                    self.upd.ind -= 1;
                    self.upd.line("} else {");
                    self.upd.ind += 1;
                    self.upd.line(&format!("{vl}.all.begin();"));
                    self.upd.line(&format!("for kq{k} in 0..x{k}.len {{"));
                    self.upd.ind += 1;
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.all.add"), &format!("kq{k}"), &format!("{k}d"))?;
                    self.upd.ind -= 1;
                    self.upd.set(self.fi, lo, e.head.1);
                    self.upd.line("}");
                    self.upd.line(&format!("{vl}.all.finish({});", lit(&format!("{}:{at}", self.files[self.fi].path))));
                    self.upd.line(&format!("{vl}.rebuilt(x{k}.len);"));
                    self.upd.ind -= 1;
                    self.upd.line("}");
                    self.upd.ind -= 1;
                    self.upd.line("}");
                }
                self.upd.line(&format!("let w{k} = vlist.step(d, {}, &{vl}, x{k}.len);", va.n));
                self.upd.line(&format!("let {mv} = {vl}.cnt;"));
                if let Some(r) = va.at_end {
                    self.upd.pad();
                    self.expr(Mode::U, r)?;
                    self.upd.s(&format!(" = {vl}.atEnd;\n"));
                }
                iv = format!("i{k}");
                bind = Some(format!("let ax{k} = {vl}.at({iv}); let {item} = {amp}x{k}[ax{k}];"));
                if let Some((x, _)) = &head.index {
                    bind = Some(format!("let ax{k} = {vl}.at({iv}); let {item} = {amp}x{k}[ax{k}]; let {x} = ax{k};"));
                }
            } else {
                self.upd.line(&format!("let {mv} = x{k}.len;"));
                iv = head.index.as_ref().map(|x| x.0.clone()).unwrap_or_else(|| format!("i{k}"));
                bind = Some(format!("let {item} = {amp}x{k}[{iv}];"));
            }
        }
        self.upd.line(&format!("{own}.cnt{id} = {mv};"));
        let saved = self.locals.len();
        let rn = format!("{}_R{id}", self.c.id);
        let er = format!("{own}.e{id}");
        let parent = self.gstk.last().cloned().unwrap_or_else(|| "-1".into());
        if let Some(key) = head.key {
            // match this frame's keys to last frame's rows, then reorder the rows to match
            let kd = format!("{own}.kd{id}");
            let at = src[..lo].matches('\n').count() + 1;
            // the key sees the item under other names (the body binds it again below)
            let (kv, ki) = (format!("kv{k}"), format!("ki{k}"));
            let nren = self.rename.len();
            self.upd.line(&format!("{kd}.begin();"));
            self.upd.line(&format!("for {kv} in 0..{mv} {{"));
            self.upd.ind += 1;
            if let Some((a, _)) = head.range {
                if src[a.0..a.1].trim() == "0" {
                    self.rename.push((item.clone(), kv.clone()));
                } else {
                    self.upd.line(&format!("let {ki} = a{k} + {kv};"));
                    self.rename.push((item.clone(), ki));
                }
            } else {
                let at = if virt.is_some() { format!("kj{k}") } else { kv.clone() };
                if virt.is_some() {
                    self.upd.line(&format!("let kj{k} = {own}.vl{id}.at({kv});"));
                }
                self.upd.line(&format!("let {ki} = x{k}[{at}];"));
                self.rename.push((item.clone(), ki));
                if let Some((n, _)) = &head.index {
                    self.rename.push((n.clone(), if virt.is_some() { at } else { kv }));
                }
            }
            self.upd.set(self.fi, key.0, key.1);
            self.upd.pad();
            self.upd.s(&if head.key_int { format!("{kd}.addInt(") } else { format!("{kd}.add(`{{") });
            let r = self.expr(Mode::U, key);
            self.upd.s(if head.key_int { ");\n" } else { "}`);\n" });
            self.rename.truncate(nren);
            r?;
            self.upd.ind -= 1;
            self.upd.set(self.fi, lo, e.head.1);
            self.upd.line("}");
            self.upd.line(&format!("{kd}.finish({});", lit(&format!("{}:{at}", self.files[self.fi].path))));
            if virt.is_some() {
                self.upd.line(&format!("{own}.vl{id}.verify(&{kd});"));
            }
            self.upd.line(&format!("if {kd}.changed {{"));
            self.upd.ind += 1;
            self.upd.line(&format!("for dq{k} in 0..{kd}.dead.len {{ let dr{k} = {er}.items[{kd}.dead.items[dq{k}]]; dr{k}.destroy(); {own}.fp{id}.push(dr{k}); }}"));
            self.upd.line(&format!("{own}.eb{id}.len = 0;"));
            self.upd.line(&format!("for rq{k} in 0..{mv} {{"));
            self.upd.ind += 1;
            self.upd.line(&format!("let f{k} = {kd}.from.items[rq{k}];"));
            self.upd.line(&format!("if f{k} >= 0 {{ {own}.eb{id}.push({er}.items[f{k}]); }} else {{ {own}.eb{id}.push({rn}.take(&{own}.fp{id}, this.mem, d, d.groupNew({parent}), this as int)); }}"));
            self.upd.ind -= 1;
            self.upd.line("}");
            self.upd.line(&format!("let t{k} = {er};"));
            self.upd.line(&format!("{er} = {own}.eb{id};"));
            self.upd.line(&format!("{own}.eb{id} = t{k};"));
            self.upd.ind -= 1;
            self.upd.line("}");
        } else if rows {
            self.upd.line(&format!("while {er}.len < {mv} {{ {er}.push({rn}.take(&{own}.fp{id}, this.mem, d, -1, this as int)); }}"));
            self.upd.line(&format!("if {er}.len > {mv} {{"));
            self.upd.ind += 1;
            self.upd.line(&format!("for zq{k} in {mv}..{er}.len {{ {er}.items[zq{k}].destroy(); {own}.fp{id}.push({er}.items[zq{k}]); }}"));
            self.upd.line(&format!("{er}.len = {mv};"));
            self.upd.ind -= 1;
            self.upd.line("}");
        }
        self.locals.push(item);
        if let Some((n, _)) = &head.index {
            self.locals.push(n.clone());
        }
        self.upd.line(&format!("for {iv} in 0..{mv} {{"));
        self.upd.ind += 1;
        if let Some(b) = &bind {
            self.upd.line(b);
        }
        if rows {
            self.upd.line(&format!("let r{id} = {er}.items[{iv}];"));
        }
        if keyed {
            self.upd.line(&format!("d.keyBegin(r{id}.uid);"));
        }
        self.pnt.set(self.fi, lo, e.head.1);
        self.pnt.line(&format!("let {sv} = d.enter({slot});"));
        self.pnt.line(&format!("let {mv} = {own}.cnt{id};"));
        self.pnt.line(&format!("for {iv} in 0..{mv} {{"));
        self.pnt.ind += 1;
        if rows {
            self.pnt.line(&format!("let r{id} = {er}.items[{iv}];"));
        }
        if keyed {
            self.pnt.line(&format!("d.paintGroup(r{id}.uid);"));
        }
        if rows {
            self.own.push(id);
        }
        if keyed {
            self.gstk.push(format!("r{id}.uid"));
        }
        self.idx.push(iv.clone());
        self.ectx.push((head.clone(), k, id, rows));
        // a keyed row's nodes are told apart by the row (`keyBegin`), not by an index in the slot
        let base = if keyed { None } else { Some((iv.clone(), width(&e.body))) };
        let mut j = 0;
        self.in_virt += virt.is_some() as usize;
        if keyed && virt.is_none() {
            row_elems(&e.body, &mut self.flip_rows);
        }
        if rows {
            self.bases.push((self.locals.len(), self.lets.len()));
        }
        let mut res = self.seq(&e.body, &mut j, depth + 1, &base);
        if rows {
            self.bases.pop();
            if res.is_ok() {
                res = self.act_sweep(id, &format!("r{id}"));
            }
        }
        self.in_virt -= virt.is_some() as usize;
        self.ectx.pop();
        self.idx.pop();
        if keyed {
            self.gstk.pop();
        }
        if rows {
            self.own.pop();
        }
        self.locals.truncate(saved);
        res?;
        if keyed {
            self.upd.line("d.keyEnd();");
            self.pnt.line("d.paintGroupEnd();");
        }
        self.upd.ind -= 1;
        self.upd.set(self.fi, lo, e.head.1);
        self.upd.line("}");
        if let Some(va) = &virt {
            // kept rows outside the window are hidden now that the rows are built
            self.upd.line(&format!("vlist.finish(d, {}, &{own}.vl{id});", va.n));
        }
        self.upd.line(&format!("d.leave({sv});"));
        self.pnt.ind -= 1;
        self.pnt.line("}");
        self.pnt.line(&format!("d.leave({sv});"));
        Ok(())
    }

    // ---- assembly
}

#[derive(Clone)]
pub(super) struct EachHead {
    pub(super) expr: Range,
    pub(super) range: Option<(Range, Range)>,
    pub(super) item: (String, Range),
    /// `as &x`: the item is a pointer to the element, so the body can change it in place.
    pub(super) by_ref: bool,
    pub(super) index: Option<(String, Range)>,
    /// `(expr)` after the names: what identifies a row across frames.
    pub(super) key: Option<Range>,
    /// `(expr : int)`: an int key, kept as raw bytes (no formatting).
    pub(super) key_int: bool,
}

pub(super) fn parse_each_head(src: &str, r: Range) -> R<EachHead> {
    let bad = |msg: &str| err(r.0, r.1, msg.to_string()).map_err(|e| e.help("`{#each items as item}`, `{#each items as &item}`, `{#each items as item, i}`, `{#each items as item (item.id)}`, or `{#each 0..n as i}`"));
    let toks = lex(&src[r.0..r.1], FileId(0)).map_err(|e| UiError { lo: r.0 + e.span.start as usize, hi: r.0 + e.span.end as usize, msg: e.message, help: None })?;
    let abs = |t: &crate::parse::Token| (r.0 + t.span.start as usize, r.0 + t.span.end as usize);
    // the last top-level `as`
    let mut depth = 0i32;
    let mut at = None;
    for (k, t) in toks.iter().enumerate() {
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
            TokenKind::As if depth == 0 => at = Some(k),
            _ => {}
        }
    }
    let Some(at) = at else { return bad("`{#each}` needs `as`") };
    let mut rest = &toks[at + 1..];
    let by_ref = rest.first().is_some_and(|t| t.kind == TokenKind::Amp);
    if by_ref {
        rest = &rest[1..];
    }
    // `item`, `item, index`, each with an optional `(key)` to the end
    let (names, tail) = match rest {
        [a, c, b, t @ ..] if a.kind == TokenKind::Ident && c.kind == TokenKind::Comma && b.kind == TokenKind::Ident => (2, t),
        [a, t @ ..] if a.kind == TokenKind::Ident => (1, t),
        _ => return bad("expected `as item` or `as item, index` at the end of `{#each}`"),
    };
    let mut key_int = false;
    let key = match tail {
        [e] if e.kind == TokenKind::Eof => None,
        [l, mid @ .., rp, e] if l.kind == TokenKind::LParen && rp.kind == TokenKind::RParen && e.kind == TokenKind::Eof => {
            let mut k = trim_range(src, abs(l).1, abs(rp).0);
            if let [.., c, i] = mid {
                if c.kind == TokenKind::Colon && i.kind == TokenKind::Ident && i.text == "int" {
                    key_int = true;
                    k = trim_range(src, abs(l).1, abs(c).0);
                }
            }
            if k.0 == k.1 {
                return bad("the `(key)` is empty");
            }
            let mut d = 0i32;
            for t in mid {
                match t.kind {
                    TokenKind::LParen => d += 1,
                    TokenKind::RParen => d -= 1,
                    _ => {}
                }
                if d < 0 {
                    return bad("unbalanced `)` in the `(key)`");
                }
            }
            Some(k)
        }
        _ => return bad("expected `as item`, `as item, index` or `as item (key)` (`(key : int)` for an int key) at the end of `{#each}`"),
    };
    let item = (rest[0].text.clone(), abs(&rest[0]));
    let index = if names == 2 { Some((rest[2].text.clone(), abs(&rest[2]))) } else { None };
    let end = abs(&toks[at]).0;
    let expr = trim_range(src, r.0, end);
    if expr.0 == expr.1 {
        return bad("`{#each}` has nothing before `as`");
    }
    let mut depth = 0i32;
    let mut range = None;
    for t in &toks[..at] {
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
            TokenKind::DotDotEq if depth == 0 => {
                let a = abs(t);
                return err(a.0, a.1, "use `a..b` (b excluded); `..=` isn't supported in `{#each}`");
            }
            TokenKind::DotDot if depth == 0 => {
                let a = abs(t);
                range = Some((trim_range(src, expr.0, a.0), trim_range(src, a.1, expr.1)));
            }
            _ => {}
        }
    }
    Ok(EachHead { expr, range, item, by_ref, index, key, key_int })
}
