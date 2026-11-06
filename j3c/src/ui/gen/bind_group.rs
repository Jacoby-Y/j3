//! `bind:group` on radios (split from `attr.rs`, whose `attr` is one long match on the name).

use super::*;

impl<'a> G<'a> {
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
