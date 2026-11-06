//! `animate:`, `anim-delay`, `bind:tooltip`, `triggers`, `noprimary`, `rich` and `bind:spans` (one slice of the `attr` chain; `true` when it handled the attribute).

use super::*;

impl<'a> G<'a> {
    pub(super) fn attr_extra(&mut self, e: &Elem, a: &Attr, n: &str, name: &str) -> R<bool> {
        if name == "animate:flip" {
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
        } else if name == "noprimary" {
            // the box neither sets PRIMARY on select nor pastes it on a middle click
            let text_box = e.tag == "textarea" || (e.tag == "input" && matches!(input_type(e).as_deref(), Some("text" | "search" | "password" | "email" | "number" | "textarea")));
            if !text_box {
                return err(a.range.0, a.range.1, "`noprimary` is for text boxes (`<input type=\"text\">`, `<textarea>`)");
            }
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, "`noprimary` takes no value");
            }
            self.upd.line(&format!("d.attrNoPrimary({n}, true);"));
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
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}
