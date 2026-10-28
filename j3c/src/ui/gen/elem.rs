use super::*;

impl<'a> G<'a> {
    pub(super) fn elem(&mut self, e: &Elem, slot: String, depth: usize) -> R<()> {
        if e.tag == "option" { option_visuals(&e.children)?; }
        if !ELEMENTS.contains(&e.tag.as_str()) {
            return Err(UiError { lo: e.open.0, hi: e.open.0 + 1 + e.tag.len(), msg: format!("unknown element `<{}>`", e.tag), help: Some(format!("elements: {}. A capitalised tag is a component (ui/Name.j3ui).", ELEMENTS.join(" "))) });
        }
        let k = self.nk();
        let n = format!("n{k}");
        let tag = self.tag(&e.tag);
        let scope = format!("SCOPE_{}", self.c.id);
        self.upd.set(self.fi, e.open.0, e.open.1);
        self.upd.line(&format!("let {n} = d.open({slot}, {tag}, {scope});"));
        let accepted = Self::accepted(&e.tag);
        let mut draw: Option<Range> = None;
        // `bind:spans` runs after every other attribute: replacing the text drops the box's spans, so the spans must come last
        let mut spans: Option<&Attr> = None;
        self.virtual_attrs(e, &n)?;
        self.transitions(e, &n)?;
        for a in &e.attrs {
            let ok = accepted.contains(&a.name.as_str()) || (a.name.starts_with("use:") && a.name.len() > 4) || (a.name.starts_with("class:") && a.name.len() > 6) || (a.name.starts_with("style:") && a.name.len() > 6);
            if a.name == "slot" && self.slots.slotted.contains(&addr(e)) {
                continue; // goes to the parent component's slot
            }
            if e.tag == "glyph" && matches!(a.name.as_str(), "action" | "key" | "pad") {
                continue; // read by the `glyph` arm below
            }
            if matches!(a.name.as_str(), "virtual" | "row-height" | "est-height" | "stick-to-bottom" | "keep" | "bind:at-end") {
                continue; // read by `virtual_attrs`
            }
            if matches!(a.name.as_str(), "fly-x" | "fly-y" | "scale-from" | "slide-axis" | "transition-easing") || a.name.starts_with("transition:") || a.name.starts_with("in:") || a.name.starts_with("out:") {
                continue; // read by `transitions`
            }
            if a.name == "animate-easing" {
                if !e.attrs.iter().any(|x| x.name == "animate:flip") {
                    return err(a.range.0, a.range.1, "`animate-easing` needs `animate:flip` on the same element");
                }
                continue; // read with `animate:flip`
            }
            if a.name == "animate:flip" && !self.flip_rows.contains(&addr(e)) {
                let why = if self.in_virt > 0 { "a `virtual` list scrolls its rows, which would flip" } else { "it must be the row element of a keyed `{#each}`" };
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`animate:flip` is not allowed here: {why}"), help: Some("`{#each items as it (it.id)}<li animate:flip={200}>...</li>{/each}`".into()) });
            }
            if !ok {
                let list = accepted.join(", ");
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown attribute `{}` on `<{}>`", a.name, e.tag), help: Some(format!("`<{}>` accepts: {list}", e.tag)) });
            }
            self.upd.set(self.fi, a.range.0, a.range.1);
            if a.name == "on:draw" {
                match a.value {
                    Value::Expr(r) => draw = Some(r),
                    _ => return err(a.range.0, a.range.1, "`on:draw` needs `={ statements }`"),
                }
                continue;
            }
            if a.name == "bind:spans" {
                spans = Some(a);
                continue;
            }
            self.attr(e, a, &n)?;
        }
        if let Some(a) = spans {
            self.upd.set(self.fi, a.range.0, a.range.1);
            self.attr(e, a, &n)?;
        }
        match e.tag.as_str() {
            // gauges: no `value` is an indeterminate progress (-1), `max` is 1 like a browser's
            "progress" | "meter" => {
                if let Some(c) = e.children.first() {
                    let at = match c { Node::Elem(x) => x.open, _ => e.open };
                    return err(at.0, at.1, format!("`<{}>` is painted by the runtime and takes no children", e.tag));
                }
                if e.tag == "progress" && !e.attrs.iter().any(|a| a.name == "value") {
                    self.upd.line(&format!("d.attrValue({n}, -1.0);"));
                }
                if !e.attrs.iter().any(|a| a.name == "max") {
                    self.upd.line(&format!("d.attrMax({n}, 1.0);"));
                }
            }
            // an input prompt: the runtime picks the label for the device in use (`dom/glyph.j3`)
            "glyph" => {
                if let Some(c) = e.children.first() {
                    let at = match c { Node::Elem(x) => x.open, _ => e.open };
                    return err(at.0, at.1, "`<glyph>` is filled by the runtime and takes no children");
                }
                let find = |name: &str| e.attrs.iter().find(|a| a.name == name);
                if find("action").is_none() && find("key").is_none() && find("pad").is_none() {
                    return err(e.open.0, e.open.1, "`<glyph>` needs `action=\"confirm\"`, or `key=\"Space\"` and/or `pad=\"x\"`");
                }
                if let Some(a) = find("action") {
                    if let Value::Str(p) = &a.value {
                        if let [Part::Lit(l)] = p.as_slice() {
                            if !GLYPH_ACTIONS.contains(&l.as_str()) {
                                return err(a.range.0, a.range.1, format!("unknown glyph action `{l}`")).map_err(|e| e.help(format!("built-in actions: {}; a game's own action needs `key=` / `pad=` or `d.setGlyphBinding`", GLYPH_ACTIONS.join(" "))));
                            }
                        }
                    }
                }
                self.upd.set(self.fi, e.open.0, e.open.1);
                self.upd.pad();
                self.upd.s(&format!("d.glyph({n}"));
                for name in ["action", "key", "pad"] {
                    self.upd.s(", ");
                    match find(name).map(|a| (a, &a.value)) {
                        None => self.upd.s("\"\""),
                        Some((_, Value::Str(p))) => self.string(Mode::U, p, find(name).unwrap().range)?,
                        Some((_, Value::Expr(r))) => self.expr(Mode::U, *r)?,
                        Some((a, Value::None)) => return err(a.range.0, a.range.1, format!("`{name}` needs a value")),
                    }
                }
                self.upd.s(");\n");
            }
            // the disclosure button: focusable, Enter / Space click it (`inputs.begin` flips the `<details>`)
            "summary" => self.upd.line(&format!("d.attrTabIndex({n});")),
            _ => {}
        }
        if self.pass_root == addr(e) {
            self.pass_apply(e, &n);
        }
        let canvas = e.tag == "canvas";
        // paint
        let pn = format!("n{k}");
        self.pnt.set(self.fi, e.open.0, e.open.1);
        self.pnt.line(&format!("let {pn} = d.paintNext({slot});"));
        self.pnt.line(&format!("if {pn} >= 0 {{"));
        self.pnt.ind += 1;
        self.pnt.line(&format!("d.paintBox({pn});"));
        if let (true, Some(r)) = (canvas, draw) {
            self.pnt.line(&format!("if d.beginCanvas({pn}) {{"));
            self.pnt.ind += 1;
            self.pnt.line(&format!("let size{k} = d.size({pn});"));
            self.rename.insert(0, ("size".into(), format!("size{k}")));
            self.locals.push(format!("size{k}"));
            self.stmts(Mode::P, r, false)?;
            self.locals.pop();
            self.rename.remove(0);
            self.pnt.line(&format!("d.endCanvas({pn});"));
            self.pnt.ind -= 1;
            self.pnt.line("}");
        }
        self.pnt.line(&format!("let pass{k} = 0;"));
        self.pnt.line(&format!("while d.paintChildren(pass{k}) {{"));
        self.pnt.ind += 1;
        let mut j = 0;
        self.seq(&e.children, &mut j, depth, &None)?;
        self.pnt.line(&format!("pass{k} += 1;"));
        self.pnt.ind -= 1;
        self.pnt.line("}");
        self.pnt.set(self.fi, e.open.0, e.open.1);
        self.pnt.line(&format!("d.paintEnd({pn});"));
        self.pnt.ind -= 1;
        self.pnt.line("}");
        self.upd.set(self.fi, e.open.0, e.open.1);
        self.upd.line("d.close();");
        Ok(())
    }

    /// `transition:` / `in:` / `out:` (fade, fly, slide, scale) and their parameters become one `d.trans(...)` call.
    /// Kinds are `Trans.ik` / `ok` in `j3.ui.dom`: 0 none, 1 fade, 2 fly, 3 slide, 4 scale.
    pub(super) fn transitions(&mut self, e: &Elem, n: &str) -> R<()> {
        const KINDS: [&str; 4] = ["fade", "fly", "slide", "scale"];
        let mut dir: [Option<(usize, Value)>; 2] = [None, None]; // [enter, leave]
        for a in &e.attrs {
            let (rest, ins, outs) = if let Some(r) = a.name.strip_prefix("transition:") { (r, true, true) } else if let Some(r) = a.name.strip_prefix("in:") { (r, true, false) } else if let Some(r) = a.name.strip_prefix("out:") { (r, false, true) } else { continue };
            let Some(k) = KINDS.iter().position(|x| *x == rest) else {
                return err(a.range.0, a.range.1, format!("unknown transition `{rest}`: use fade, fly, slide or scale"));
            };
            if !matches!(a.value, Value::None | Value::Expr(_)) {
                return err(a.range.0, a.range.1, format!("`{}` takes `={{milliseconds}}`", a.name));
            }
            for (i, on) in [ins, outs].into_iter().enumerate() {
                if on {
                    if dir[i].is_some() {
                        return err(a.range.0, a.range.1, format!("`{}` repeats the {} transition of this element: use `transition:` for both directions, or one `in:` and one `out:`", a.name, if i == 0 { "enter" } else { "leave" }));
                    }
                    dir[i] = Some((k + 1, a.value.clone()));
                }
            }
        }
        let uses = |k: usize| dir.iter().any(|d| matches!(d, Some((x, _)) if *x == k));
        for (name, kind) in [("fly-x", 2), ("fly-y", 2), ("scale-from", 4), ("slide-axis", 3), ("transition-easing", 0)] {
            if let Some(a) = e.attrs.iter().find(|x| x.name == name) {
                if kind == 0 && dir.iter().all(|d| d.is_none()) {
                    return err(a.range.0, a.range.1, "`transition-easing` needs `transition:`, `in:` or `out:` on the same element");
                }
                if kind != 0 && !uses(kind) {
                    return err(a.range.0, a.range.1, format!("`{name}` needs a `{}` transition on the same element", KINDS[kind - 1]));
                }
            }
        }
        if dir.iter().all(|d| d.is_none()) {
            return Ok(());
        }
        let lit = |name: &str| -> Option<(String, Range)> {
            let a = e.attrs.iter().find(|x| x.name == name)?;
            match &a.value {
                Value::Str(p) => match p.as_slice() {
                    [Part::Lit(l)] => Some((l.clone(), a.range)),
                    _ => Some((String::new(), a.range)),
                },
                _ => Some((String::new(), a.range)),
            }
        };
        let ease = match lit("transition-easing") {
            None => -1,
            Some((l, r)) => match ["ease", "linear", "ease-in", "ease-out", "ease-in-out", "bounce", "spring"].iter().position(|k| *k == l) {
                Some(i) => i as i32,
                None => return err(r.0, r.1, "`transition-easing` takes one of \"ease\", \"linear\", \"ease-in\", \"ease-out\", \"ease-in-out\", \"bounce\", \"spring\""),
            },
        };
        let axis = match lit("slide-axis") {
            None => 0,
            Some((l, r)) => match l.as_str() {
                "y" => 0,
                "x" => 1,
                _ => return err(r.0, r.1, "`slide-axis` takes \"x\" or \"y\""),
            },
        };
        self.upd.pad();
        self.upd.s(&format!("d.trans({n}"));
        for d in &dir {
            match d {
                None => self.upd.s(", 0, 0.0"),
                Some((k, v)) => {
                    self.upd.s(&format!(", {k}, "));
                    match v {
                        Value::Expr(r) => {
                            self.upd.s("f32(");
                            self.expr(Mode::U, *r)?;
                            self.upd.s(")");
                        }
                        _ => self.upd.s("150.0"),
                    }
                }
            }
        }
        self.upd.s(&format!(", {ease}"));
        for (name, def) in [("fly-x", "0.0"), ("fly-y", "20.0"), ("scale-from", "0.5")] {
            self.upd.s(", ");
            match e.attrs.iter().find(|x| x.name == name) {
                None => self.upd.s(def),
                Some(a) => {
                    let Value::Expr(r) = a.value else { return err(a.range.0, a.range.1, format!("`{name}` takes `={{number}}`")) };
                    self.upd.set(self.fi, a.range.0, a.range.1);
                    self.upd.s("f32(");
                    self.expr(Mode::U, r)?;
                    self.upd.s(")");
                }
            }
        }
        self.upd.s(&format!(", {axis});\n"));
        Ok(())
    }

    /// `virtual` on a scroll box: its one child must be a keyed `{#each}`, which `each` then lowers to the rows
    /// near the viewport. `row-height={26}` makes every row that tall; without it (or `row-height="auto"`) rows
    /// are measured, starting from `est-height={40}`. `stick-to-bottom`, `keep={keys}` and
    /// `bind:at-end={place}` are described in `j3c doc j3ui`.
    pub(super) fn virtual_attrs(&mut self, e: &Elem, n: &str) -> R<()> {
        let find = |name: &str| e.attrs.iter().find(|a| a.name == name);
        let Some(va) = find("virtual") else {
            for x in ["row-height", "est-height", "stick-to-bottom", "keep", "bind:at-end"] {
                if let Some(r) = find(x) {
                    return err(r.range.0, r.range.1, format!("`{x}` needs `virtual` on the same element"));
                }
            }
            return Ok(());
        };
        if e.comp {
            return err(va.range.0, va.range.1, "`virtual` goes on a plain element (the scroll box), not a component");
        }
        if e.children.len() != 1 || !matches!(e.children[0], Node::Each(_)) {
            return err(va.range.0, va.range.1, "a `virtual` element holds exactly one child, a keyed `{#each}`").map_err(|x| x.help("`<div virtual row-height={26}>{#each rows as r (r.id)} ... {/each}</div>`, with nothing else inside"));
        }
        let Node::Each(h) = &e.children[0] else { unreachable!() };
        if !parse_each_head(self.src, h.head).map_or(false, |h| h.key.is_some() && h.range.is_none()) {
            return err(va.range.0, va.range.1, "the `{#each}` in a `virtual` element needs a key and a list").map_err(|x| x.help("`{#each rows as r (r.id)}`; a `0..n` range has no list to window"));
        }
        if e.attrs.iter().filter(|x| x.name.starts_with("style:")).count() > 6 {
            return err(va.range.0, va.range.1, "a `virtual` element takes at most 6 `style:` attributes (two are its spacers)");
        }
        let num = |a: &Attr, what: &str| match &a.value {
            Value::Expr(r) => Ok(Value::Expr(*r)),
            Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l.trim().parse::<f64>().is_ok()) => Ok(Value::Str(p.clone())),
            _ => err(a.range.0, a.range.1, format!("`{what}` needs a number: `{what}={{26}}`")),
        };
        // `row-height="auto"` is the same as no `row-height`
        let rh = match find("row-height") {
            Some(a) if matches!(&a.value, Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l.trim() == "auto")) => None,
            Some(a) => Some(num(a, "row-height")?),
            None => None,
        };
        let est = match find("est-height") {
            Some(a) if rh.is_some() => return err(a.range.0, a.range.1, "`est-height` is the guess for measured rows; this list has a `row-height`"),
            Some(a) => Some(num(a, "est-height")?),
            None => None,
        };
        let stick = match find("stick-to-bottom") {
            Some(a) if !matches!(a.value, Value::None) => return err(a.range.0, a.range.1, "`stick-to-bottom` takes no value"),
            Some(_) => true,
            None => false,
        };
        let keep = match find("keep") {
            Some(a) => match a.value {
                Value::Expr(r) => Some(r),
                _ => return err(a.range.0, a.range.1, "`keep` needs `={keys}`, a `[]string` of row keys").map_err(|x| x.help("`<div virtual keep={state.pinned}>`; the keys are the `{#each}` key formatted as text")),
            },
            None => None,
        };
        let at_end = match find("bind:at-end") {
            Some(a) => match a.value {
                Value::Expr(r) => {
                    self.place_check(r)?;
                    Some(r)
                }
                _ => return err(a.range.0, a.range.1, "`bind:at-end` needs `={place}`, a `bool` to write to"),
            },
            None => None,
        };
        self.virt = Some(Virt { n: n.to_string(), rh, est, stick, keep, at_end });
        Ok(())
    }

    /// `call(`{key}`)` for row `ix` of the list `x<k>`, with the item (and index) names bound to that row.
    /// `tag` makes the temporaries unique in their scope.
    pub(super) fn virt_key(&mut self, item: &str, index: &Option<String>, key: Range, int: bool, call: &str, ix: &str, tag: &str) -> R<()> {
        let kn: String = tag.chars().take_while(|c| c.is_ascii_digit()).collect();
        let nren = self.rename.len();
        self.upd.line(&format!("let kr{tag} = x{kn}[{ix}];"));
        self.rename.push((item.to_string(), format!("kr{tag}")));
        if let Some(x) = index {
            self.upd.line(&format!("let ks{tag} = {ix};"));
            self.rename.push((x.clone(), format!("ks{tag}")));
        }
        self.upd.set(self.fi, key.0, key.1);
        self.upd.pad();
        let (open, close) = if int { (format!("{call}Int("), ");\n") } else { (format!("{call}(`{{"), "}`);\n") };
        self.upd.s(&open);
        let r = self.expr(Mode::U, key);
        self.upd.s(close);
        self.rename.truncate(nren);
        r
    }

    /// A number attribute value as J3 text (`dflt` when absent).
    pub(super) fn virt_num(&mut self, v: &Option<Value>, dflt: &str) -> R<()> {
        match v {
            Some(Value::Expr(r)) => self.expr(Mode::U, *r)?,
            Some(Value::Str(p)) => {
                if let [Part::Lit(l)] = p.as_slice() {
                    self.upd.s(l.trim());
                }
            }
            _ => self.upd.s(dflt),
        }
        Ok(())
    }

    /// A string attribute: literal text, `"a {b}"`, or `{expr}`, passed to `d.<call>(n, s)`.
    pub(super) fn str_attr(&mut self, a: &Attr, call: &str, n: &str) -> R<()> {
        self.upd.pad();
        self.upd.s(&format!("d.{call}({n}, "));
        match &a.value {
            // a template string dies when `update` returns, and `value` is kept as a view
            Value::Str(p) if call == "attrText" && p.iter().any(|x| matches!(x, Part::Hole(_))) => {
                return err(a.range.0, a.range.1, "`value` text can't have `{}` holes").map_err(|e| e.help("use `value={expr}` with a `string` that outlives the frame"));
            }
            Value::Str(p) => self.string(Mode::U, p, a.range)?,
            Value::Expr(r) => self.expr(Mode::U, *r)?,
            Value::None => return err(a.range.0, a.range.1, format!("`{}` needs a value", a.name)),
        }
        self.upd.s(");\n");
        Ok(())
    }

    pub(super) fn num_attr(&mut self, a: &Attr, call: &str, n: &str) -> R<()> {
        self.upd.pad();
        self.upd.s(&format!("d.{call}({n}, f32("));
        match &a.value {
            Value::Expr(r) => self.expr(Mode::U, *r)?,
            Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l.trim().parse::<f64>().is_ok()) => {
                if let [Part::Lit(l)] = p.as_slice() {
                    self.upd.s(l.trim());
                }
            }
            _ => return err(a.range.0, a.range.1, format!("`{}` needs a number: `{}={{expr}}`", a.name, a.name)),
        }
        self.upd.s("));\n");
        Ok(())
    }

    /// `style:prop={number}` or `style:prop="css {value}"` (or `={"text"}`): an inline style, `d.styleNum` / `d.styleStr`.
    /// Checks `style:prop` and says whether its value is a number (`={n}`) or text.
    pub(super) fn style_val(&self, a: &Attr, prop: &str) -> R<bool> {
        let longhand = css_props().split(' ').any(|p| p == prop);
        let shorthand = ["padding", "margin", "inset", "border", "border-color", "border-width", "border-radius", "border-top", "border-right", "border-bottom", "border-left", "overflow", "gap", "background", "text-stroke", "text-wave"].contains(&prop);
        if prop.starts_with("animation") || prop.starts_with("transition") || prop.starts_with('-') || prop.starts_with("grid-") || !(longhand || shorthand) {
            return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`style:{prop}` is not a CSS property that can be set inline"), help: Some("any property of `j3c doc css` except `animation`, `transition`, grid properties and custom properties".into()) });
        }
        match &a.value {
            Value::Expr(r) if !matches!(self.src[r.0..r.1].trim_start().chars().next(), Some('"' | '`')) => {
                // `gap` is in the longhand list (column gap) but `style:gap={n}` would set only that one, so a shorthand is text
                if !longhand || shorthand {
                    return err(a.range.0, a.range.1, format!("`style:{prop}` is a shorthand: give it text, `style:{prop}=\"{{v}}px\"`"));
                }
                Ok(true)
            }
            Value::None => err(a.range.0, a.range.1, format!("`style:{prop}` needs a value: `style:{prop}={{number}}` or `style:{prop}=\"text\"`")),
            _ => Ok(false),
        }
    }

    pub(super) fn style_attr(&mut self, e: &Elem, a: &Attr, prop: &str, n: &str) -> R<()> {
        let num = self.style_val(a, prop)?;
        let k = e.attrs.iter().filter(|x| x.name.starts_with("style:")).position(|x| std::ptr::eq(x, a)).unwrap_or(0);
        if k >= 8 {
            return err(a.range.0, a.range.1, "an element takes at most 8 `style:` attributes");
        }
        self.upd.pad();
        match &a.value {
            Value::Expr(r) if num => {
                self.upd.s(&format!("d.styleNum({n}, {k}, {}, f32(", lit(prop)));
                self.expr(Mode::U, *r)?;
                self.upd.s("));\n");
            }
            Value::Expr(r) => {
                self.upd.s(&format!("d.styleStr({n}, {k}, {}, ", lit(prop)));
                self.expr(Mode::U, *r)?;
                self.upd.s(");\n");
            }
            Value::Str(p) => {
                self.upd.s(&format!("d.styleStr({n}, {k}, {}, ", lit(prop)));
                self.string(Mode::U, p, a.range)?;
                self.upd.s(");\n");
            }
            Value::None => unreachable!("checked by style_val"),
        }
        Ok(())
    }

    /// `use:name={param}` / `use:name` on element `n`: `fn name(node: int, param: T)` runs on the first build after the element's first layout
    /// (so it can measure), `name_update(node, param)` when the parameter changes, `name_destroy(node)` once the element is gone.
    pub(super) fn action(&mut self, a: &Attr, an: &str, n: &str) -> R<()> {
        let (lo, hi) = a.range;
        let Some(f) = self.c.sc.fns.iter().find(|f| f.name == an) else {
            let help = if an.contains('.') { "a dotted module function isn't supported: wrap it in a script `fn`".to_string() } else { format!("add `fn {an}(node: int, param: T) {{ ... }}` to the script") };
            return Err(UiError { lo, hi, msg: format!("`use:{an}` needs a script fn `{an}`"), help: Some(help) });
        };
        let src = self.src;
        let tys = |f: &markup::FnDef| -> Vec<String> {
            let t = src[f.params.0..f.params.1].trim();
            if t.is_empty() { return Vec::new(); }
            t.split(',').map(|x| x.split(':').nth(1).unwrap_or("").trim().to_string()).collect()
        };
        let want = if matches!(a.value, Value::None) { 1 } else { 2 };
        let t = tys(f);
        let sig = if want == 1 { format!("fn {an}(node: int)") } else { format!("fn {an}(node: int, param: T)") };
        if t.len() != want || t[0] != "int" || f.ret.is_some() {
            return Err(UiError { lo: f.name_r.0, hi: f.name_r.1, msg: format!("`use:{an}` calls `{sig}`, with no return type"), help: Some("the first parameter is the element's handle".into()) });
        }
        let ty = if want == 2 {
            if !EV_TYPES.contains(&t[1].as_str()) {
                return Err(UiError { lo: f.name_r.0, hi: f.name_r.1, msg: format!("the parameter of `{an}` is `{}`, but a `use:` parameter is a plain value", t[1]), help: Some(format!("types: {}", EV_TYPES.join(", "))) });
            }
            Some(t[1].clone())
        } else {
            None
        };
        let upn = format!("{an}_update");
        let update = self.c.sc.fns.iter().find(|f| f.name == upn);
        if let Some(u) = update {
            if tys(u) != t || u.ret.is_some() {
                return Err(UiError { lo: u.name_r.0, hi: u.name_r.1, msg: format!("`{upn}` must take the same parameters as `{an}`: `{}`", sig.replace(an, &upn)), help: None });
            }
            if want == 1 {
                return Err(UiError { lo: u.name_r.0, hi: u.name_r.1, msg: format!("`{upn}` never runs: `use:{an}` has no parameter to change"), help: None });
            }
        }
        let dn = format!("{an}_destroy");
        let destroy = self.c.sc.fns.iter().find(|f| f.name == dn);
        if let Some(u) = destroy {
            if tys(u) != ["int"] || u.ret.is_some() {
                return Err(UiError { lo: u.name_r.0, hi: u.name_r.1, msg: format!("`{dn}` must be `fn {dn}(node: int)`"), help: None });
            }
        }
        let k = self.nk();
        let o = self.owner();
        let h = format!("uh{k}");
        self.upd.line(&format!("let {h} = d.handle({n});"));
        if let (Some(ty), Value::Expr(r)) = (&ty, &a.value) {
            self.upd.pad();
            self.upd.s(&format!("let pv{k}: {ty} = "));
            self.expr(Mode::U, *r)?;
            self.upd.s(";\n");
        }
        let args = if ty.is_some() { format!("{h}, pv{k}") } else { h.clone() };
        let gone = if destroy.is_some() { format!("if {o}.us{k} == 1 {{ this.{dn}({o}.ua{k}); }} ") } else { String::new() };
        self.upd.line(&format!("if {o}.ua{k} != {h} {{ {gone}{o}.ua{k} = {h}; {o}.us{k} = 0; }}"));
        let keep = match ty.as_deref() { Some("string") => format!("{o}.up{k} = d.strPut({o}.up{k}, pv{k}); {o}.ul{k} = pv{k}.len; "), Some(_) => format!("{o}.up{k} = pv{k}; "), None => String::new() };
        self.upd.line(&format!("else if {o}.us{k} == 0 {{ {o}.us{k} = 1; {keep}this.{an}({args}); }}"));
        if update.is_some() {
            let diff = if ty.as_deref() == Some("string") { format!("!d.strHas({o}.up{k}, {o}.ul{k}, pv{k})") } else { format!("{o}.up{k} != pv{k}") };
            self.upd.line(&format!("else if {diff} {{ {keep}this.{upn}({args}); }}"));
        }
        self.acts.push(ActDef { k, container: self.own.last().copied().unwrap_or(0), name: an.to_string(), ty, destroy: destroy.is_some() });
        Ok(())
    }

    /// End of a container's build (`who` is `this` or the row): run `name_destroy` for the `use:`s of container `cont` whose element is
    /// gone, and put `bind:this` places whose element went back to -1.
    pub(super) fn act_sweep(&mut self, cont: usize, who: &str) -> R<()> {
        for i in 0..self.acts.len() {
            let a = &self.acts[i];
            if a.container != cont {
                continue;
            }
            let k = a.k;
            let call = if a.destroy { format!("if {who}.us{k} == 1 {{ this.{}_destroy({who}.ua{k}); }} ", a.name) } else { String::new() };
            self.upd.line(&format!("if {who}.ua{k} >= 0 && !d.alive({who}.ua{k}) {{ {call}{who}.ua{k} = -1; {who}.us{k} = 0; }}"));
        }
        // a place something else has written since is left alone
        let mine: Vec<(usize, Range)> = self.binds.iter().filter(|b| b.container == cont).map(|b| (b.k, b.place)).collect();
        for (k, r) in mine {
            self.upd.pad();
            self.upd.s(&format!("if {who}.bh{k} >= 0 && !d.alive({who}.bh{k}) {{ if "));
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" == {who}.bh{k} {{ "));
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = -1; }} {who}.bh{k} = -1; }}\n"));
        }
        Ok(())
    }
}
