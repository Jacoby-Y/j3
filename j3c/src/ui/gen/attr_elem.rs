//! Attributes of specific elements: tabindex, grid, ids, `for`, `type`, meter, `open` (one slice of the `attr` chain; `true` when it handled the attribute).

use super::*;

impl<'a> G<'a> {
    pub(super) fn attr_elem(&mut self, e: &Elem, a: &Attr, n: &str, name: &str) -> R<bool> {
        if name == "tabindex" {
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
        } else {
            return Ok(false);
        }
        Ok(true)
    }
}
