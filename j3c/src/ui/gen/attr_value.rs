//! `bind:this`, `use:`, `bind:open`, ranges, media sources, aria and focus attributes, `name`, `value` (one slice of the `attr` chain; `true` when it handled the attribute).

use super::*;

impl<'a> G<'a> {
    pub(super) fn attr_value(&mut self, e: &Elem, a: &Attr, n: &str, name: &str) -> R<bool> {
        if name == "bind:this" {
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
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}
