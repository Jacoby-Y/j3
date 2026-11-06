//! `on:` event handlers (one slice of the `attr` chain; `true` when it handled the attribute).

use super::*;

impl<'a> G<'a> {
    pub(super) fn attr_event(&mut self, e: &Elem, a: &Attr, n: &str, name: &str) -> R<bool> {
        if let Some(ev) = name.strip_prefix("on:") {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, format!("`{name}` needs `={{ statements }}`"));
            };
            // `|stop` ends the bubble walk (dom.attrFlag bits: 4 wheel, 8 mousedown, 16 mouseup, 32 contextmenu)
            let ev = match ev {
                "wheel|stop" | "mousedown|stop" | "mouseup|stop" | "contextmenu|stop" => {
                    let base = &ev[..ev.len() - 5];
                    let bit = match base { "wheel" => 4, "mousedown" => 8, "mouseup" => 16, _ => 32 };
                    self.upd.line(&format!("d.attrFlag({n}, {bit});"));
                    base
                }
                _ => ev,
            };
            if !MOUSE.contains(&ev) {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown event `{name}` on `<{}>`", e.tag), help: Some(format!("events: {}{}", MOUSE.join(" "), if e.tag == "canvas" { " draw" } else { "" })) });
            }
            let head = match ev {
                "click" => format!("if d.clicked({n}) {{"),
                "wheel" => format!("if d.wheelAt({n}) != 0.0 {{"),
                "input" => format!("if d.changed({n}) {{"),
                "change" if !is_radio(e) => return err(a.range.0, a.range.1, "`on:change` is for `<input type=\"radio\">` (the radio that just got checked); a text box or slider has `on:input`"),
                "change" => format!("if d.changed({n}) {{"),
                "submit" => {
                    self.upd.line(&format!("d.attrFlag({n}, 2);"));
                    format!("if d.submitted({n}) {{")
                }
                "trigger" => {
                    if !e.attrs.iter().any(|x| x.name == "triggers") {
                        self.upd.line(&format!("d.attrTriggers({n}, \"@:\");"));
                    }
                    format!("if d.triggered({n}) {{")
                }
                "close" => format!("if d.closeRequested({n}) {{"),
                "toggle" => format!("if d.toggled({n}) {{"),
                "edit" => {
                    if !e.attrs.iter().any(|x| x.name == "grid") {
                        return err(a.range.0, a.range.1, "`on:edit` needs `grid={..}` on the same element");
                    }
                    format!("if d.gridEdit({n}) {{")
                }
                "contextmenu" => format!("if d.contextAt({n}) {{"),
                "dragstart" | "dragend" => {
                    if !e.attrs.iter().any(|x| x.name == "draggable") {
                        return err(a.range.0, a.range.1, format!("`{name}` needs `draggable` on the same element"));
                    }
                    if ev == "dragend" {
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
                    format!("if d.{call}({n}) {{")
                }
                "mousedown" => format!("if d.mouseDown({n}) {{"),
                "mouseup" => format!("if d.mouseUp({n}) {{"),
                _ => format!("if d.mouseMove({n}) {{"),
            };
            self.upd.line(&head);
            self.upd.ind += 1;
            // the names a handler sees; J3 has no shadowing, even across sibling blocks, so each is written `name_<node>_<event>`
            let mut lets: Vec<(&str, String)> = Vec::new();
            if ev == "wheel" {
                lets.push(("wheel", format!("d.wheelAt({n})")));
            } else if ev == "toggle" {
                lets.push(("open", format!("d.isOpen({n})")));
            } else if ev == "contextmenu" {
                lets.push(("mouse", "d.mouseAt()".into()));
            } else if ev == "trigger" {
                lets.push(("trigger", format!("d.triggerText({n})")));
                lets.push(("tlo", "d.trigLo".into()));
                lets.push(("thi", "d.trigHi".into()));
            } else if ev == "edit" {
                for (l, f) in [("kind", "evKind"), ("row", "evRow"), ("col", "evCol"), ("text", "evText")] {
                    lets.push((l, format!("d.gridOf({n}).{f}")));
                }
            } else if matches!(ev, "dragstart" | "dragend" | "dragenter" | "dragleave" | "dragover" | "drop") {
                if matches!(ev, "dragenter" | "dragleave" | "dragover" | "drop") {
                    lets.push(("mouse", format!("d.localMouse({n})")));
                }
                lets.push(("data", "d.dragData()".into()));
                lets.push(("dragId", "d.dragIdOf()".into()));
                if ev == "dragend" {
                    lets.push(("dropped", "d.dragDropped()".into()));
                } else if ev == "dragover" {
                    lets.push(("accept", "true".into()));
                }
            }
            if !matches!(ev, "click" | "input" | "change" | "submit" | "close" | "toggle" | "contextmenu" | "trigger" | "edit" | "dragstart" | "dragend" | "dragenter" | "dragleave" | "dragover" | "drop") {
                lets.push(("mouse", format!("d.localMouse({n})")));
            }
            if ev == "mousedown" {
                lets.push(("button", "d.downBtn".into()));
            } else if ev == "mouseup" {
                lets.push(("button", "d.upBtn".into()));
            }
            if e.tag == "canvas" {
                lets.push(("size", format!("d.size({n})")));
            }
            let (saved, nren) = (self.locals.len(), self.rename.len());
            for (l, v) in &lets {
                let to = format!("{l}_{n}_{ev}");
                self.upd.line(&format!("let {to} = {v};"));
                self.rename.insert(0, (l.to_string(), to.clone()));
                self.locals.push(to);
            }
            self.stmts(Mode::U, r, true)?;
            self.locals.truncate(saved);
            self.rename.drain(..self.rename.len() - nren);
            if ev == "dragover" {
                self.upd.line(&format!("if !accept_{n}_dragover {{ d.dragReject({n}); }}"));
            }
            self.upd.ind -= 1;
            self.upd.line("}");
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}
