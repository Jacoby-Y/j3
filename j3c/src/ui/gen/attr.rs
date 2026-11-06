use super::*;

impl<'a> G<'a> {
    pub(super) fn attr(&mut self, e: &Elem, a: &Attr, n: &str) -> R<()> {
        let name = a.name.as_str();
        // The slices run in the order of the old else-if chain; the first that matches wins.
        let _handled = self.attr_class(e, a, n, name)? || self.attr_elem(e, a, n, name)? || self.attr_value(e, a, n, name)? || self.attr_extra(e, a, n, name)? || self.attr_bind(e, a, n, name)? || self.attr_event(e, a, n, name)?;
        Ok(())
    }

    pub(super) fn attr_class(&mut self, e: &Elem, a: &Attr, n: &str, name: &str) -> R<bool> {
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
        } else if name == "window-drag" {
            // a client-side title bar: the box moves the window (`j3.ui.windrag`)
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, "`window-drag` takes no value");
            }
            self.upd.line(&format!("d.attrWindowDrag({n});"));
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
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}
