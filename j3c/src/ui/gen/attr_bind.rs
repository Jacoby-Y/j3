//! `bind:group`, `bind:checked`/`value`/`high`, `bind:scrollX`/`scrollY` (one slice of the `attr` chain; `true` when it handled the attribute).

use super::*;

impl<'a> G<'a> {
    pub(super) fn attr_bind(&mut self, e: &Elem, a: &Attr, n: &str, name: &str) -> R<bool> {
        if name == "bind:group" {
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
        } else if name == "bind:scrollX" || name == "bind:scrollY" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, format!("`{name}` needs `={{place}}`, an `f32` offset in px"));
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.bindScroll({n}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(", {});\n", name == "bind:scrollX"));
            self.upd.hint = None;
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}
