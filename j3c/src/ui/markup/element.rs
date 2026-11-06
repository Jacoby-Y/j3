//! Elements and their attributes.

use super::*;

impl<'a> P<'a> {
    pub(super) fn element(&mut self, top: bool) -> R<Option<Elem>> {
        let lo = self.i;
        let mut j = lo + 1;
        while j < self.b.len() && is_tag_char(self.b[j]) {
            j += 1;
        }
        let written = self.s[lo + 1..j].to_string();
        // `<textarea>` is an `<input type="textarea">` (void, like `<input>`)
        let tag = if written == "textarea" { "input".to_string() } else { written.clone() };
        self.i = j;
        if top && (tag == "script" || tag == "style") {
            self.ws();
            if !self.at(">") {
                return err(lo, j, format!("`<{tag}>` takes no attributes"));
            }
            self.i += 1;
            let close = format!("</{tag}>");
            let Some(k) = self.s[self.i..].find(&close) else {
                return err(lo, j, format!("`<{tag}>` is never closed"));
            };
            let r = (self.i, self.i + k);
            let slot = if tag == "script" { &mut self.out.script } else { &mut self.out.style };
            if slot.is_some() {
                return err(lo, j, format!("a component has one `<{tag}>`"));
            }
            *slot = Some(r);
            self.i += k + close.len();
            return Ok(None);
        }
        let mut attrs = Vec::new();
        let self_close;
        loop {
            self.ws();
            if self.i >= self.b.len() {
                return err(lo, j, format!("`<{tag}>` is never closed (the tag ends the file)"));
            }
            if self.at("/>") {
                self.i += 2;
                self_close = true;
                break;
            }
            if self.at(">") {
                self.i += 1;
                self_close = false;
                break;
            }
            attrs.push(self.attr()?);
        }
        if written == "textarea" {
            attrs.retain(|a: &Attr| a.name != "type");
            attrs.insert(0, Attr { name: "type".into(), range: (lo, j), value: Value::Str(vec![Part::Lit("textarea".into())]) });
        }
        let open = (lo, self.i);
        let comp = tag.rsplit('.').next().is_some_and(|l| l.starts_with(|c: char| c.is_ascii_uppercase()));
        let mut children = Vec::new();
        if !self_close && !matches!(tag.as_str(), "input" | "img" | "br") {
            let (mut nodes, end) = self.nodes(false)?;
            match end {
                End::Close(t, r) => {
                    if t != tag {
                        if matches!(t.as_str(), "input" | "textarea") {
                            return err(r.0, r.1, close_msg(&t));
                        }
                        return err(r.0, r.1, format!("expected `</{tag}>`, found `</{t}>`"));
                    }
                }
                End::Eof => return err(lo, open.1, format!("`<{tag}>` is never closed")),
                End::Else(_, r) | End::EndIf(r) | End::EndEach(r) | End::Case(_, r) | End::EndMatch(r) => {
                    return err(open.0, open.1, format!("`<{tag}>` is never closed before the block ends at {}", r.0));
                }
            }
            children.append(&mut nodes);
        } else if matches!(tag.as_str(), "input" | "img" | "br") && !self_close && self.at(&format!("</{written}>")) {
            self.i += written.len() + 3;
        } else if tag == "input" && !self_close {
            // `<textarea><br /></textarea>`: a text box takes no content, say so at the `<br>`
            let rest = &self.s[self.i..];
            let t = rest.trim_start();
            let lo = self.i + (rest.len() - t.len());
            if t.starts_with("<br") && !t[3..].starts_with(is_tag_char_str) {
                if let Some(e) = t.find('>') {
                    if t[e + 1..].trim_start().starts_with(&format!("</{written}>")) {
                        return err(lo, lo + e + 1, format!("`<br>` is not allowed inside `<{written}>`: a text box takes no content"));
                    }
                }
            }
        }
        Ok(Some(Elem { tag, comp, attrs, children, open, id: Cell::new(0) }))
    }

    fn attr(&mut self) -> R<Attr> {
        let lo = self.i;
        while self.i < self.b.len() && (is_name_char(self.b[self.i]) || matches!(self.b[self.i], b':' | b'|' | b'.')) {
            self.i += 1;
        }
        if self.i == lo {
            return err(lo, lo + 1, "expected an attribute name");
        }
        let name = self.s[lo..self.i].to_string();
        if !self.at("=") {
            return Ok(Attr { name, range: (lo, self.i), value: Value::None });
        }
        self.i += 1;
        let value = match self.b.get(self.i).copied() {
            Some(b'{') => {
                let e = self.brace_end(self.i)?;
                let r = trim_range(self.s, self.i + 1, e);
                // `on:mousedown|stop={}` only stops the bubble walk, so it may be empty
                if r.0 == r.1 && !name.ends_with("|stop") {
                    return err(self.i, e + 1, "empty `{}`");
                }
                self.i = e + 1;
                Value::Expr(r)
            }
            Some(q @ (b'"' | b'\'')) => {
                self.i += 1;
                let mut parts = Vec::new();
                let mut lit = String::new();
                loop {
                    let Some(&c) = self.b.get(self.i) else {
                        return err(lo, self.i, "this attribute value is never closed");
                    };
                    if c == q {
                        self.i += 1;
                        break;
                    }
                    if c == b'{' {
                        if !lit.is_empty() {
                            parts.push(Part::Lit(decode(&std::mem::take(&mut lit))));
                        }
                        let e = self.brace_end(self.i)?;
                        let r = trim_range(self.s, self.i + 1, e);
                        if r.0 == r.1 {
                            return err(self.i, e + 1, "empty `{}`");
                        }
                        parts.push(Part::Hole(r));
                        self.i = e + 1;
                        continue;
                    }
                    let ch = self.s[self.i..].chars().next().unwrap();
                    lit.push(ch);
                    self.i += ch.len_utf8();
                }
                if !lit.is_empty() || parts.is_empty() {
                    parts.push(Part::Lit(decode(&lit)));
                }
                Value::Str(parts)
            }
            _ => return err(self.i, self.i + 1, "expected `\"text\"` or `{expr}` after `=`"),
        };
        Ok(Attr { name, range: (lo, self.i), value })
    }
}
