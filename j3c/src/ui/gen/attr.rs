use super::*;

impl<'a> G<'a> {
    pub(super) fn attr(&mut self, e: &Elem, a: &Attr, n: &str) -> R<()> {
        let name = a.name.as_str();
        if name == "class" {
            match &a.value {
                Value::Str(p) => {
                    // whole literal words are static; a word with a `{hole}` is joined into one dynamic text
                    let (mut dynp, mut word, mut buf): (Vec<Part>, Vec<Part>, String) = (Vec::new(), Vec::new(), String::new());
                    for part in p.iter().cloned().chain([Part::Lit(" ".into())]) {
                        match part {
                            Part::Hole(_) => {
                                if !buf.is_empty() {
                                    word.push(Part::Lit(std::mem::take(&mut buf)));
                                }
                                word.push(part);
                            }
                            Part::Lit(l) => {
                                for ch in l.chars() {
                                    if !ch.is_whitespace() {
                                        buf.push(ch);
                                        continue;
                                    }
                                    if !buf.is_empty() {
                                        word.push(Part::Lit(std::mem::take(&mut buf)));
                                    }
                                    match word.as_slice() {
                                        [] => {}
                                        [Part::Lit(w)] => {
                                            let f = self.cls(w);
                                            self.upd.line(&format!("d.clsId({n}, {f});"));
                                        }
                                        _ => {
                                            if !dynp.is_empty() {
                                                dynp.push(Part::Lit(" ".into()));
                                            }
                                            dynp.append(&mut word);
                                        }
                                    }
                                    word.clear();
                                }
                            }
                        }
                    }
                    if !dynp.is_empty() {
                        self.upd.pad();
                        self.upd.s(&format!("d.clsDyn({n}, "));
                        self.string(Mode::U, &dynp, a.range)?;
                        self.upd.s(");\n");
                    }
                }
                Value::None => {}
                _ => return Err(UiError { lo: a.range.0, hi: a.range.1, msg: "`class` takes text".into(), help: Some("`class=\"a b{x}\"`; switch a class with `class:name={condition}`".into()) }),
            }
        } else if let Some(prop) = name.strip_prefix("style:") {
            self.style_attr(e, a, prop, n)?;
        } else if let Some(c) = name.strip_prefix("class:") {
            let Value::Expr(r) = a.value else {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`{name}` needs a condition"), help: Some(format!("{name}={{condition}}")) });
            };
            let f = self.cls(c);
            self.upd.pad();
            self.upd.s(&format!("d.clsIfId({n}, {f}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(");\n");
        } else if name == "disabled" || name == "checked" || name == "multiple" {
            let call = match name {
                "disabled" => "attrDisabled",
                "multiple" => "attrMultiple",
                _ if is_radio(e) => "attrRadio",
                _ => "attrChecked",
            };
            self.upd.pad();
            self.upd.s(&format!("d.{call}({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                _ => return err(a.range.0, a.range.1, format!("`{name}` takes `{{condition}}`")),
            }
            self.upd.s(");\n");
        } else if name == "popover" || name == "modal" {
            // an overlay: the element is in the tree while it is open (`{#if open}`)
            let kind = if name == "modal" { 1 } else { 2 };
            match &a.value {
                Value::Str(p) if name == "popover" && matches!(p.as_slice(), [Part::Lit(l)] if l == "context") => self.upd.line(&format!("d.attrContext({n});")),
                Value::Str(p) if name == "popover" && matches!(p.as_slice(), [Part::Lit(l)] if l == "caret") => self.upd.line(&format!("d.attrCaret({n});")),
                Value::None => self.upd.line(&format!("d.attrOverlay({n}, {kind});")),
                Value::Expr(r) => {
                    self.upd.pad();
                    self.upd.s("if ");
                    self.expr(Mode::U, *r)?;
                    self.upd.s(&format!(" {{ d.attrOverlay({n}, {kind}); }}\n"));
                }
                _ => return err(a.range.0, a.range.1, format!("`{name}` takes `{{condition}}`")),
            }
        } else if name == "panel" {
            // a draggable panel; `panel="dock"` lets it snap to the workspace edges
            if matches!(a.value, Value::None) {
                self.upd.line(&format!("d.attrPanel({n}, \"\");"));
            } else {
                self.str_attr(a, "attrPanel", n)?;
            }
        } else if name == "panel-handle" || name == "panel-resize" {
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, format!("`{name}` takes no value"));
            }
            // dom.PK_HANDLE / PK_GRIP
            let role = if name == "panel-handle" { 2 } else { 3 };
            self.upd.line(&format!("d.attrPanelRole({n}, {role});"));
        } else if name == "draggable" {
            self.upd.pad();
            self.upd.s(&format!("d.attrDraggable({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                _ => return err(a.range.0, a.range.1, "`draggable` takes `{condition}`"),
            }
            self.upd.s(");\n");
        } else if matches!(name, "drag-data" | "drag-id" | "drag-ghost") {
            if !e.attrs.iter().any(|x| x.name == "draggable") {
                return err(a.range.0, a.range.1, format!("`{name}` needs `draggable` on the same element"));
            }
            match name {
                "drag-data" => self.str_attr(a, "attrDragData", n)?,
                "drag-ghost" => {
                    if !matches!(&a.value, Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l == "none")) {
                        return err(a.range.0, a.range.1, "`drag-ghost` takes only `\"none\"`").map_err(|e| e.help("the page paints its own drag image; leave the attribute out for the default ghost"));
                    }
                    self.str_attr(a, "attrDragGhost", n)?;
                }
                _ => {
                    let Value::Expr(r) = a.value else {
                        return err(a.range.0, a.range.1, "`drag-id` needs `={integer}`");
                    };
                    self.upd.pad();
                    self.upd.s(&format!("d.attrDragId({n}, "));
                    self.expr(Mode::U, r)?;
                    self.upd.s(");\n");
                }
            }
        } else if name == "menuitem" {
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, "`menuitem` takes no value".to_string());
            }
            self.upd.line(&format!("d.attrMenuItem({n});"));
        } else if name == "tabindex" {
            // focusable like a button: -1 skips Tab, 0 is tree order, a positive integer goes first (ascending); `={expr}` is an int
            let lit = match &a.value {
                Value::None => Some(0),
                Value::Str(p) => match p.as_slice() { [Part::Lit(l)] => l.trim().parse::<i64>().ok().filter(|v| *v >= -1), _ => None },
                _ => None,
            };
            if let Value::Expr(r) = a.value {
                self.upd.s(&format!("d.attrTabIndexN({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            } else {
                match lit {
                    None => return err(a.range.0, a.range.1, "`tabindex` takes `-1`, `0`, a positive integer or `={int}`".to_string()),
                    Some(-1) => self.upd.line(&format!("d.attrTabIndexOff({n});")),
                    Some(0) => self.upd.line(&format!("d.attrTabIndex({n});")),
                    Some(v) => self.upd.line(&format!("d.attrTabIndexN({n}, {v});")),
                }
            }
        } else if name == "grid" {
            // the window's data grid (`j3.ui.grid`): `grid={g.sized(rows.len)}`, a `&Grid`, on the `virtual` scroll box
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`grid` needs `={g.sized(rows.len)}`, a `&Grid` (`j3.ui.gridstate.Grid`)");
            };
            if !e.attrs.iter().any(|x| x.name == "virtual") {
                return err(a.range.0, a.range.1, "`grid` goes on the `virtual` scroll box (`<div virtual row-height={h} grid={..}>`)");
            }
            self.upd.pad();
            self.upd.s(&format!("d.gridBind({n}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(");\n");
        } else if name == "autoselect" || name == "default" {
            // bare flags: the dialog selects its first text box when it opens; the button Enter clicks (`Dom.attrFlag`)
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, format!("`{name}` takes no value"));
            }
            self.upd.line(&format!("d.attrFlag({n}, {});", if name == "default" { 3 } else { 1 }));
        } else if name == "closedby" {
            self.str_attr(a, "attrClosedBy", n)?;
        } else if name == "title" {
            self.str_attr(a, "attrTitle", n)?;
        } else if name == "left" || name == "top" {
            self.num_attr(a, if name == "left" { "attrLeft" } else { "attrTop" }, n)?;
        } else if name == "id" {
            self.str_attr(a, "attrId", n)?;
        } else if name == "for" {
            // an id is a static name (`attrId`), so the label's target is one too
            match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => self.str_attr(a, "attrFor", n)?,
                _ => return err(a.range.0, a.range.1, "for= takes a static id").map_err(|e| e.help("write `for=\"name\"`, the `id=\"name\"` of the control")),
            }
        } else if name == "type" {
            self.str_attr(a, "attrType", n)?;
        } else if name == "maxlength" || name == "rows" {
            self.num_attr(a, if name == "rows" { "attrRows" } else { "attrMaxLen" }, n)?;
        } else if e.tag == "meter" && matches!(name, "low" | "high" | "optimum") {
            // `Dom.attrMeter`: 0 low, 1 high, 2 optimum
            let which = match name { "low" => 0, "high" => 1, _ => 2 };
            self.num_attr(a, "attrMeter", &format!("{n}, {which}"))?;
        } else if name == "open" {
            self.upd.pad();
            self.upd.s(&format!("d.attrOpen({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                _ => return err(a.range.0, a.range.1, "`open` takes `={condition}` or no value"),
            }
            self.upd.s(");\n");
        } else if name == "bind:this" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`bind:this` needs `={place}`, an `int` to write the element's handle to");
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.handle({n});\n"));
            self.upd.hint = None;
            if self.bases.last().copied().unwrap_or((0, 0)) == (self.locals.len(), self.lets.len()) {
                let k = self.nk();
                let o = self.owner();
                self.upd.line(&format!("{o}.bh{k} = d.handle({n});"));
                self.binds.push(BindDef { k, container: self.own.last().copied().unwrap_or(0), place: r });
            }
        } else if let Some(an) = name.strip_prefix("use:") {
            self.action(a, an, n)?;
        } else if name == "bind:open" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`bind:open` needs `={place}`, a `bool` to write to");
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            self.upd.line(&format!("if d.userToggled({n}) {{"));
            self.upd.ind += 1;
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.isOpen({n});\n"));
            self.upd.ind -= 1;
            self.upd.line("}");
            self.upd.pad();
            self.upd.s(&format!("d.attrOpen({n}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(");\n");
            self.upd.hint = None;
        } else if matches!(name, "min" | "max" | "step" | "high") || (name == "value" && (is_range(e) || matches!(e.tag.as_str(), "progress" | "meter"))) {
            let call = match name {
                "min" => "attrMin",
                "max" => "attrMax",
                "step" => "attrStep",
                "high" => "attrHigh",
                _ => "attrValue",
            };
            if name == "high" {
                self.upd.line(&format!("d.attrMultiple({n}, true);"));
            }
            self.num_attr(a, call, n)?;
        } else if let Some(code) = media_code(name) {
            // `<img>` / `<svg>` / `<path>` data: `media.M_*` string kinds and `media.N_*` number kinds
            let target = format!("{n}, {}", code.1);
            self.upd.hint = Some("image and shape attributes are text (`src=\"a.png\"`) or `={expr}`; `frame`, `fps` and `stroke-width` take numbers");
            if code.0 {
                self.num_attr(a, "attrMediaNum", &target)?;
            } else {
                self.str_attr(a, "attrMedia", &target)?;
            }
            self.upd.hint = None;
        } else if name == "alt" || name == "aria-label" {
            // nothing draws it; `j3.ui.a11y` reads it as the accessible name
            self.str_attr(a, "attrAria", n)?;
        } else if name == "role" {
            self.str_attr(a, "attrRole", n)?;
        } else if name == "aria-live" {
            // "polite", "assertive" or "off"; `j3.ui.a11y.liveChanges`
            self.str_attr(a, "attrLive", n)?;
        } else if name == "aria-describedby" {
            // ids are static names (`attrId`), so is this
            match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => self.str_attr(a, "attrDescBy", n)?,
                _ => return err(a.range.0, a.range.1, "aria-describedby= takes static ids").map_err(|e| e.help("write `aria-describedby=\"hint\"`, the `id=\"hint\"` of the element that describes this one (several ids separated by spaces)")),
            }
        } else if name == "aria-selected" {
            self.upd.pad();
            self.upd.s(&format!("d.attrSelected({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l == "true" || l == "false") => {
                    if let [Part::Lit(l)] = p.as_slice() {
                        self.upd.s(l);
                    }
                }
                _ => return err(a.range.0, a.range.1, "`aria-selected` takes `=\"true\"`, `=\"false\"` or `={condition}`"),
            }
            self.upd.s(");\n");
        } else if name == "focus-group" || name.starts_with("nav-") {
            // gamepad / arrow-key focus (`j3.ui.padnav`): names of ids, so static like `for`
            let which = match name { "focus-group" => 0, "nav-up" => 1, "nav-down" => 2, "nav-left" => 3, _ => 4 };
            match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => self.str_attr(a, "attrNav", &format!("{n}, {which}"))?,
                _ => return err(a.range.0, a.range.1, format!("{name}= takes a static string")).map_err(|e| e.help("write `nav-down=\"id\"`, the `id=\"name\"` of the element to focus; `focus-group=\"wrap trap arrows\"` takes any of the three words")),
            }
        } else if name == "name" {
            self.str_attr(a, "attrName", n)?;
        } else if name == "value" && is_radio(e) {
            // a radio's value is read by its `bind:group`, which writes it into the variable
        } else if name == "value" || name == "placeholder" {
            self.str_attr(a, if name == "value" { "attrText" } else { "attrPlaceholder" }, n)?;
        } else if name == "animate:flip" {
            let ms = match &a.value {
                Value::None => None,
                Value::Expr(r) => Some(*r),
                _ => return err(a.range.0, a.range.1, "`animate:flip` takes `={milliseconds}`"),
            };
            let ease = match e.attrs.iter().find(|x| x.name == "animate-easing") {
                None => 3, // ease-out: `Css.eases` seeds the presets in this order
                Some(x) => {
                    let lit = match &x.value {
                        Value::Str(p) => match p.as_slice() {
                            [Part::Lit(l)] => l.as_str(),
                            _ => "",
                        },
                        _ => "",
                    };
                    match ["ease", "linear", "ease-in", "ease-out", "ease-in-out", "bounce", "spring"].iter().position(|k| *k == lit) {
                        Some(i) => i,
                        None => return err(x.range.0, x.range.1, "`animate-easing` takes one of \"ease\", \"linear\", \"ease-in\", \"ease-out\", \"ease-in-out\", \"bounce\", \"spring\""),
                    }
                }
            };
            self.upd.pad();
            self.upd.s(&format!("d.flip({n}, "));
            match ms {
                None => self.upd.s("200.0"),
                Some(r) => {
                    self.upd.s("f32(");
                    self.expr(Mode::U, r)?;
                    self.upd.s(")");
                }
            }
            self.upd.s(&format!(", {ease});\n"));
        } else if name == "anim-delay" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`anim-delay` takes `={milliseconds}`");
            };
            self.upd.pad();
            self.upd.s(&format!("d.animDelay({n}, f32("));
            self.expr(Mode::U, r)?;
            self.upd.s("));\n");
        } else if name == "bind:tooltip" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`bind:tooltip` needs `={boolean_place}`");
            };
            self.place_check(r)?;
            if !tooltip_child(&e.children) {
                return err(a.range.0, a.range.1, "`bind:tooltip` needs a direct child with bare `popover`, usually `{#if card}<div popover>...</div>{/if}` inside an owner wrapper");
            }
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.hoverCard({n});\n"));
        } else if name == "triggers" {
            // the characters that start a trigger for `on:trigger` (a literal: the box keeps it as a view)
            if !e.attrs.iter().any(|x| x.name == "on:trigger") {
                return err(a.range.0, a.range.1, "`triggers` needs `on:trigger` on the same element");
            }
            match &a.value {
                Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if !l.is_empty()) => self.str_attr(a, "attrTriggers", n)?,
                _ => return err(a.range.0, a.range.1, "`triggers` takes a literal such as `triggers=\"@:#\"`"),
            }
        } else if name == "rich" || name == "bind:spans" {
            // a `rich` textarea keeps styled spans and atomic tokens beside its text (`j3.ui.spans.Span`)
            if input_type(e).as_deref() != Some("textarea") {
                return err(a.range.0, a.range.1, format!("`{name}` is for `<textarea rich>`"));
            }
            if name == "rich" {
                if !matches!(a.value, Value::None) {
                    return err(a.range.0, a.range.1, "`rich` takes no value");
                }
                self.upd.line(&format!("d.attrRich({n}, true);"));
            } else {
                if !e.attrs.iter().any(|x| x.name == "rich") {
                    return err(a.range.0, a.range.1, "`bind:spans` needs `rich` on the `<textarea>`");
                }
                let Value::Expr(r) = a.value else {
                    return err(a.range.0, a.range.1, "`bind:spans` needs `={place}` holding a `[]Span` (`use j3.ui.spans.Span;`)");
                };
                self.place_check(r)?;
                self.upd.line(&format!("if d.changed({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" = d.spansValue({n});\n"));
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.attrSpans({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            }
        } else if name == "bind:group" {
            self.bind_group(e, a, n)?;
        } else if matches!(name, "bind:checked" | "bind:value" | "bind:high") {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, format!("`{name}` needs `={{place}}`, a name, field, or index to write to"));
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            if name == "bind:checked" && is_radio(e) {
                // a radio flips by itself (and its group-mates with it): write back every one that flipped
                self.upd.line(&format!("if d.userToggled({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" = d.isChecked({n});\n"));
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.attrRadio({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            } else if name == "bind:checked" {
                self.upd.line(&format!("if d.clicked({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(" = !");
                self.expr(Mode::U, r)?;
                self.upd.s(";\n");
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.attrChecked({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            } else {
                // a range binds a number, anything else (text, search, color, select) a string
                let (get, set) = match (name, input_type(e).as_deref()) {
                    ("bind:high", _) => ("highValue", "attrHigh"),
                    (_, _) if is_range(e) => ("value", "attrValue"),
                    (_, Some(_)) => ("strValue", "attrText"),
                    (_, None) => return err(a.range.0, a.range.1, "`bind:value` needs a literal `type` (`type=\"text\"`), so the compiler knows what it binds"),
                };
                if name == "bind:high" {
                    self.upd.line(&format!("d.attrMultiple({n}, true);"));
                }
                self.upd.line(&format!("if d.changed({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" = d.{get}({n});\n"));
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.{set}({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            }
            self.upd.hint = None;
        } else if let Some(ev) = name.strip_prefix("on:") {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, format!("`{name}` needs `={{ statements }}`"));
            };
            if !MOUSE.contains(&ev) {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown event `{name}` on `<{}>`", e.tag), help: Some(format!("events: {}{}", MOUSE.join(" "), if e.tag == "canvas" { " draw" } else { "" })) });
            }
            let mut extra: Vec<&str> = Vec::new();
            let head = match ev {
                "click" => format!("if d.clicked({n}) {{"),
                "wheel" => {
                    extra.push("wheel");
                    format!("if d.wheelAt({n}) != 0.0 {{")
                }
                "input" => format!("if d.changed({n}) {{"),
                "change" if !is_radio(e) => return err(a.range.0, a.range.1, "`on:change` is for `<input type=\"radio\">` (the radio that just got checked); a text box or slider has `on:input`"),
                "change" => format!("if d.changed({n}) {{"),
                "submit" => {
                    self.upd.line(&format!("d.attrFlag({n}, 2);"));
                    format!("if d.submitted({n}) {{")
                }
                "trigger" => {
                    extra.extend(["trigger", "tlo", "thi"]);
                    if !e.attrs.iter().any(|x| x.name == "triggers") {
                        self.upd.line(&format!("d.attrTriggers({n}, \"@:\");"));
                    }
                    format!("if d.triggered({n}) {{")
                }
                "close" => format!("if d.closeRequested({n}) {{"),
                "toggle" => {
                    extra.push("open");
                    format!("if d.toggled({n}) {{")
                }
                "edit" => {
                    if !e.attrs.iter().any(|x| x.name == "grid") {
                        return err(a.range.0, a.range.1, "`on:edit` needs `grid={..}` on the same element");
                    }
                    extra.extend(["kind", "row", "col", "text"]);
                    format!("if d.gridEdit({n}) {{")
                }
                "contextmenu" => {
                    extra.push("mouse");
                    format!("if d.contextAt({n}) {{")
                }
                "dragstart" | "dragend" => {
                    if !e.attrs.iter().any(|x| x.name == "draggable") {
                        return err(a.range.0, a.range.1, format!("`{name}` needs `draggable` on the same element"));
                    }
                    extra.extend(["data", "dragId"]);
                    if ev == "dragend" {
                        extra.push("dropped");
                        format!("if d.dragEnded({n}) {{")
                    } else {
                        format!("if d.dragStarted({n}) {{")
                    }
                }
                "dragenter" | "dragleave" | "dragover" | "drop" => {
                    // dom.attrDragTarget bits: 1 enter, 2 leave, 4 over, 8 drop
                    let (bit, call) = match ev {
                        "dragenter" => (1, "dragEntered"),
                        "dragleave" => (2, "dragLeft"),
                        "dragover" => (4, "dragOver"),
                        _ => (8, "dropAt"),
                    };
                    self.upd.line(&format!("d.attrDragTarget({n}, {bit});"));
                    extra.extend(["mouse", "data", "dragId"]);
                    if ev == "dragover" {
                        extra.push("accept");
                    }
                    format!("if d.{call}({n}) {{")
                }
                _ => {
                    extra.push("mouse");
                    let call = match ev {
                        "mousedown" => "mouseDown",
                        "mouseup" => "mouseUp",
                        _ => "mouseMove",
                    };
                    format!("if d.{call}({n}) {{")
                }
            };
            self.upd.line(&head);
            self.upd.ind += 1;
            if ev == "wheel" {
                self.upd.line(&format!("let wheel = d.wheelAt({n});"));
            } else if ev == "toggle" {
                self.upd.line(&format!("let open = d.isOpen({n});"));
            } else if ev == "contextmenu" {
                self.upd.line("let mouse = d.mouseAt();");
            } else if ev == "trigger" {
                self.upd.line(&format!("let trigger = d.triggerText({n});"));
                self.upd.line("let tlo = d.trigLo;");
                self.upd.line("let thi = d.trigHi;");
            } else if ev == "edit" {
                self.upd.line(&format!("let kind = d.gridOf({n}).evKind;"));
                self.upd.line(&format!("let row = d.gridOf({n}).evRow;"));
                self.upd.line(&format!("let col = d.gridOf({n}).evCol;"));
                self.upd.line(&format!("let text = d.gridOf({n}).evText;"));
            } else if matches!(ev, "dragstart" | "dragend" | "dragenter" | "dragleave" | "dragover" | "drop") {
                if matches!(ev, "dragenter" | "dragleave" | "dragover" | "drop") {
                    self.upd.line(&format!("let mouse = d.localMouse({n});"));
                }
                self.upd.line("let data = d.dragData();");
                self.upd.line("let dragId = d.dragIdOf();");
                if ev == "dragend" {
                    self.upd.line("let dropped = d.dragDropped();");
                } else if ev == "dragover" {
                    self.upd.line("let accept = true;");
                }
            } else if !matches!(ev, "click" | "input" | "change" | "submit" | "close" | "toggle") {
                self.upd.line(&format!("let mouse = d.localMouse({n});"));
            }
            if e.tag == "canvas" {
                self.upd.line(&format!("let size = d.size({n});"));
                extra.push("size");
            }
            let saved = self.locals.len();
            self.locals.extend(extra.iter().map(|s| s.to_string()));
            self.stmts(Mode::U, r, true)?;
            self.locals.truncate(saved);
            if ev == "dragover" {
                self.upd.line(&format!("if !accept {{ d.dragReject({n}); }}"));
            }
            self.upd.ind -= 1;
            self.upd.line("}");
        }
        Ok(())
    }

    /// `bind:group={x}` on a radio: when the user checks this radio `x` becomes its `value`, and the radio is checked while `x` equals it.
    /// A quoted `value="a"` makes `x` a `string`; `value={expr}` an `int` (or a `string` when `x` is a `let` declared as one).
    pub(super) fn bind_group(&mut self, e: &Elem, a: &Attr, n: &str) -> R<()> {
        let Value::Expr(r) = a.value else {
            return err(a.range.0, a.range.1, "`bind:group` needs `={place}`, the variable the checked radio's `value` goes to");
        };
        if !is_radio(e) {
            return err(a.range.0, a.range.1, "`bind:group` is for `<input type=\"radio\">`").map_err(|e| e.help("write `type=\"radio\"` as a literal on the same element"));
        }
        self.place_check(r)?;
        let Some(v) = e.attrs.iter().find(|x| x.name == "value") else {
            return err(a.range.0, a.range.1, "`bind:group` needs a `value` on the same radio").map_err(|e| e.help("`value=\"a\"` for a string group, `value={1}` for an int one"));
        };
        let place = self.src[r.0..r.1].trim();
        let declared = self.c.sc.lets.iter().find(|l| l.name == place).map(|l| match (l.ty, l.guess) {
            (Some(t), _) => self.src[t.0..t.1].trim().to_string(),
            (None, Some(g)) => g.to_string(),
            (None, None) => String::new(),
        });
        let is_str = match &v.value {
            Value::Str(p) => {
                if p.iter().any(|x| matches!(x, Part::Hole(_))) {
                    return err(v.range.0, v.range.1, "a radio's `value` text can't have `{}` holes").map_err(|e| e.help("the variable keeps it past the frame: use `value={expr}` with a `string` that outlives it"));
                }
                if matches!(declared.as_deref(), Some("int" | "float" | "f32" | "bool")) {
                    return err(v.range.0, v.range.1, format!("`{place}` is {}, but `value=\"..\"` is text", declared.unwrap())).map_err(|e| e.help("write `value={1}` for an int group"));
                }
                true
            }
            Value::Expr(_) => declared.as_deref() == Some("string"),
            Value::None => return err(v.range.0, v.range.1, "`value` needs a value"),
        };
        let val = |g: &mut Self| -> R<()> {
            match &v.value {
                Value::Str(p) => g.string(Mode::U, p, v.range),
                Value::Expr(x) => g.expr(Mode::U, *x),
                Value::None => Ok(()),
            }
        };
        self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
        self.upd.line(&format!("if d.changed({n}) {{"));
        self.upd.ind += 1;
        self.upd.pad();
        self.expr(Mode::U, r)?;
        self.upd.s(" = ");
        val(self)?;
        self.upd.s(";\n");
        self.upd.ind -= 1;
        self.upd.line("}");
        self.upd.hint = None;
        self.upd.pad();
        self.upd.s(&format!("d.attrGroup({n}, "));
        if is_str {
            self.upd.s("d.textIs(");
            self.expr(Mode::U, r)?;
            self.upd.s(", ");
            val(self)?;
            self.upd.s(")");
        } else {
            self.upd.s("(");
            self.expr(Mode::U, r)?;
            self.upd.s(") == (");
            val(self)?;
            self.upd.s(")");
        }
        self.upd.s(");\n");
        Ok(())
    }
}
