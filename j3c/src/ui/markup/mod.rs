//! The `.j3ui` container format: markup, `{...}` holes, blocks, and the `<script>` item list.
//!
//! This is not a J3 parser. It only finds where each `{...}` ends (brace counting that skips
//! strings and templates) and splits `<script>` into items with the real lexer. Every J3
//! fragment stays a byte range into the file; `gen.rs` copies those ranges verbatim and the
//! real parser reads the result.

use std::cell::Cell;

use crate::parse::lex::lex;
use crate::parse::TokenKind;
use crate::span::FileId;

mod script;

pub use script::*;

pub type Range = (usize, usize);

#[derive(Debug, Clone)]
pub struct UiError {
    pub lo: usize,
    pub hi: usize,
    pub msg: String,
    pub help: Option<String>,
}

pub type R<T> = Result<T, UiError>;

pub fn err<T>(lo: usize, hi: usize, msg: impl Into<String>) -> R<T> {
    Err(UiError { lo, hi, msg: msg.into(), help: None })
}

impl UiError {
    pub fn help(mut self, h: impl Into<String>) -> UiError {
        self.help = Some(h.into());
        self
    }
}

#[derive(Debug, Clone)]
pub enum Part {
    Lit(String),
    Hole(Range),
}

#[derive(Debug, Clone)]
pub enum Value {
    None,
    Expr(Range),
    Str(Vec<Part>),
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub range: Range,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct Elem {
    pub tag: String,
    /// A capitalised tag: the component `ui/<Tag>.j3ui`.
    pub comp: bool,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    /// The opening tag, `<div ...>`.
    pub open: Range,
    /// Child-instance number (components only), set by `gen`.
    pub id: Cell<usize>,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub cond: Option<Range>,
    /// `{:case a, b}` values of a `{#match}` (`cond` is `None` there; an `{:else}` has none).
    pub vals: Vec<Range>,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct IfNode {
    pub branches: Vec<Branch>,
    /// `{#match X}`: the value the cases compare.
    pub scrut: Option<Range>,
    /// Dynamic component candidate-set enum; lowered before component scans.
    pub choice: Option<String>,
    pub range: Range,
    /// State number when a branch holds components, else 0.
    pub id: Cell<usize>,
}

#[derive(Debug, Clone)]
pub struct EachNode {
    pub head: Range,
    pub body: Vec<Node>,
    pub range: Range,
    pub id: Cell<usize>,
}

/// `{#let name = expr}`: a local for the rest of the block.
#[derive(Debug, Clone)]
pub struct LetNode {
    pub name: String,
    pub name_r: Range,
    pub expr: Range,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum Node {
    Let(LetNode),
    Text { parts: Vec<Part>, range: Range },
    Elem(Elem),
    If(IfNode),
    Each(EachNode),
}

#[derive(Debug, Default)]
pub struct Markup {
    pub nodes: Vec<Node>,
    pub script: Option<Range>,
    pub style: Option<Range>,
}

enum End {
    Eof,
    Close(String, Range),
    Else(Option<Range>, Range),
    EndIf(Range),
    EndEach(Range),
    Case(Vec<Range>, Range),
    EndMatch(Range),
}

struct P<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
    out: Markup,
}

pub fn parse(src: &str) -> R<Markup> {
    let mut p = P { s: src, b: src.as_bytes(), i: 0, out: Markup::default() };
    let (mut nodes, end) = p.nodes(true)?;
    match end {
        End::Eof => {}
        End::Close(t, r) => return err(r.0, r.1, close_msg(&t)),
        End::Else(_, r) => return err(r.0, r.1, "`{:else}` outside `{#if}`"),
        End::EndIf(r) => return err(r.0, r.1, "`{/if}` without `{#if}`"),
        End::EndEach(r) => return err(r.0, r.1, "`{/each}` without `{#each}`"),
        End::Case(_, r) => return err(r.0, r.1, "`{:case}` outside `{#match}`"),
        End::EndMatch(r) => return err(r.0, r.1, "`{/match}` without `{#match}`"),
    }
    trim(&mut nodes);
    p.out.nodes = nodes;
    Ok(p.out)
}

/// Whitespace rules: runs collapse to one space, a text that is only whitespace is dropped,
/// and the first/last text of a child list loses its outer whitespace.
fn flush(nodes: &mut Vec<Node>, text: &mut Vec<Part>, lo: usize, hi: usize) {
    if !text.is_empty() {
        nodes.push(Node::Text { parts: std::mem::take(text), range: (lo, hi) });
    }
}

fn trim(nodes: &mut Vec<Node>) {
    for n in nodes.iter_mut() {
        match n {
            Node::Elem(e) => trim(&mut e.children),
            Node::If(i) => i.branches.iter_mut().for_each(|b| trim(&mut b.nodes)),
            Node::Each(e) => trim(&mut e.body),
            Node::Text { .. } | Node::Let(_) => {}
        }
    }
    let n = nodes.len();
    for (k, node) in nodes.iter_mut().enumerate() {
        if let Node::Text { parts, .. } = node {
            if k == 0 {
                if let Some(Part::Lit(s)) = parts.first_mut() {
                    *s = s.trim_start_matches(|c: char| c.is_ascii_whitespace()).to_string();
                }
            }
            if k + 1 == n {
                if let Some(Part::Lit(s)) = parts.last_mut() {
                    *s = s.trim_end_matches(|c: char| c.is_ascii_whitespace()).to_string();
                }
            }
            parts.retain(|p| !matches!(p, Part::Lit(s) if s.is_empty()));
        }
    }
    nodes.retain(|n| match n {
        Node::Text { parts, .. } => !parts.is_empty() && !matches!(parts.as_slice(), [Part::Lit(s)] if s.chars().all(|c| c.is_ascii_whitespace())),
        _ => true,
    });
}

/// A tag name also takes `.`: `<game.Bar>` is the component `ui/game/Bar.j3ui`.
/// The message for a close tag with no open element; a text box is void, so content written "inside" it ends here.
fn close_msg(t: &str) -> String {
    if matches!(t, "input" | "textarea") {
        format!("`<{t}>` is a text box and takes no content: nothing can go inside it")
    } else {
        format!("`</{t}>` closes nothing")
    }
}

fn is_tag_char_str(c: char) -> bool {
    c.is_ascii() && is_tag_char(c as u8)
}

fn is_tag_char(c: u8) -> bool {
    is_name_char(c) || c == b'.'
}

pub fn is_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

impl<'a> P<'a> {
    fn at(&self, pat: &str) -> bool {
        self.s[self.i..].starts_with(pat)
    }

    fn ws(&mut self) {
        while self.i < self.b.len() && self.b[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    /// `self.i` is at `{`; returns the index of the matching `}`.
    fn brace_end(&self, open: usize) -> R<usize> {
        let b = self.b;
        let mut i = open + 1;
        let mut depth = 1;
        while i < b.len() {
            match b[i] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(i);
                    }
                }
                b'"' => {
                    i += 1;
                    while i < b.len() && b[i] != b'"' {
                        if b[i] == b'\\' {
                            i += 1;
                        }
                        if b[i.min(b.len() - 1)] == b'\n' {
                            break;
                        }
                        i += 1;
                    }
                }
                b'`' => {
                    i += 1;
                    while i < b.len() && b[i] != b'`' && b[i] != b'\n' {
                        i += 1;
                    }
                }
                b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                    while i < b.len() && b[i] != b'\n' {
                        i += 1;
                    }
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        err(open, open + 1, "this `{` is never closed")
    }

    fn nodes(&mut self, top: bool) -> R<(Vec<Node>, End)> {
        let mut nodes: Vec<Node> = Vec::new();
        let mut text: Vec<Part> = Vec::new();
        let mut text_lo = self.i;
        loop {
            if self.i >= self.b.len() {
                flush(&mut nodes, &mut text, text_lo, self.i);
                return Ok((nodes, End::Eof));
            }
            let c = self.b[self.i];
            if c == b'<' {
                if self.at("<!--") {
                    flush(&mut nodes, &mut text, text_lo, self.i);
                    match self.s[self.i..].find("-->") {
                        Some(k) => self.i += k + 3,
                        None => return err(self.i, self.i + 4, "this comment is never closed"),
                    }
                    text_lo = self.i;
                    continue;
                }
                if self.at("</") {
                    flush(&mut nodes, &mut text, text_lo, self.i);
                    let lo = self.i;
                    let mut j = self.i + 2;
                    while j < self.b.len() && is_tag_char(self.b[j]) {
                        j += 1;
                    }
                    let tag = self.s[self.i + 2..j].to_string();
                    self.i = j;
                    self.ws();
                    if !self.at(">") {
                        return err(lo, j, "expected `>` to end the closing tag");
                    }
                    self.i += 1;
                    return Ok((nodes, End::Close(tag, (lo, self.i))));
                }
                if self.i + 1 < self.b.len() && self.b[self.i + 1].is_ascii_alphabetic() {
                    flush(&mut nodes, &mut text, text_lo, self.i);
                    if let Some(el) = self.element(top)? {
                        nodes.push(Node::Elem(el));
                    }
                    text_lo = self.i;
                    continue;
                }
            }
            if c == b'{' {
                let next = self.b.get(self.i + 1).copied().unwrap_or(0);
                if next == b'#' {
                    flush(&mut nodes, &mut text, text_lo, self.i);
                    nodes.push(self.block()?);
                    text_lo = self.i;
                    continue;
                }
                if next == b':' || next == b'/' {
                    flush(&mut nodes, &mut text, text_lo, self.i);
                    let end = self.block_end()?;
                    return Ok((nodes, end));
                }
                if next == b'@' {
                    return err(self.i, self.i + 2, "`{@...}` tags are not supported");
                }
                if text.is_empty() {
                    text_lo = self.i;
                }
                let e = self.brace_end(self.i)?;
                let (lo, hi) = trim_range(self.s, self.i + 1, e);
                if lo == hi {
                    return err(self.i, e + 1, "empty `{}`");
                }
                text.push(Part::Hole((lo, hi)));
                self.i = e + 1;
                continue;
            }
            // plain text up to the next tag or brace
            if text.is_empty() {
                text_lo = self.i;
            }
            let start = self.i;
            while self.i < self.b.len() {
                let c = self.b[self.i];
                if c == b'{' {
                    break;
                }
                if c == b'<' {
                    let n = self.b.get(self.i + 1).copied().unwrap_or(0);
                    if n.is_ascii_alphabetic() || n == b'/' || self.at("<!--") {
                        break;
                    }
                }
                self.i += 1;
            }
            // collapse first: an `&nbsp;` decoded before collapsing would count as whitespace
            text.push(Part::Lit(decode(&collapse(&self.s[start..self.i]))));
        }
    }

    fn block_end(&mut self) -> R<End> {
        let lo = self.i;
        let e = self.brace_end(lo)?;
        let inner = &self.s[lo + 1..e];
        self.i = e + 1;
        let r = (lo, e + 1);
        if inner == "/if" {
            return Ok(End::EndIf(r));
        }
        if inner == "/each" {
            return Ok(End::EndEach(r));
        }
        if inner == "/match" {
            return Ok(End::EndMatch(r));
        }
        if let Some(rest) = inner.strip_prefix(":case") {
            if rest.starts_with(|c: char| c.is_whitespace()) {
                // values split at top-level commas (not inside brackets, strings, or char/backtick text)
                let (mut vals, mut from, mut depth, mut q) = (Vec::new(), lo + 1 + ":case".len(), 0i32, lo + 1 + ":case".len());
                while q <= e {
                    let c = if q < e { self.b[q] } else { b',' };
                    match c {
                        b'(' | b'[' | b'{' => depth += 1,
                        b')' | b']' | b'}' => depth -= 1,
                        b'"' | b'`' => {
                            q += 1;
                            while q < e && self.b[q] != c {
                                if self.b[q] == b'\\' {
                                    q += 1;
                                }
                                q += 1;
                            }
                        }
                        b',' if depth == 0 => {
                            let (a, b) = trim_range(self.s, from, q);
                            if a == b {
                                return err(lo, e + 1, "`{:case}` needs a value (`{:case 1, 2}`)");
                            }
                            vals.push((a, b));
                            from = q + 1;
                        }
                        _ => {}
                    }
                    q += 1;
                }
                return Ok(End::Case(vals, r));
            }
        }
        if inner.trim() == ":else" {
            return Ok(End::Else(None, r));
        }
        if let Some(rest) = inner.strip_prefix(":else if") {
            if rest.starts_with(|c: char| c.is_whitespace()) {
                let at = lo + 1 + ":else if".len();
                let (a, b) = trim_range(self.s, at, e);
                if a == b {
                    return err(lo, e + 1, "`{:else if}` needs a condition");
                }
                return Ok(End::Else(Some((a, b)), r));
            }
        }
        err(lo, e + 1, "unknown block tag").map_err(|x| x.help("blocks: {#if}{:else if}{:else}{/if}, {#match X}{:case 1, 2}{:else}{/match} and {#each X as x, i}{/each}"))
    }

    fn block(&mut self) -> R<Node> {
        let lo = self.i;
        let e = self.brace_end(lo)?;
        let inner = &self.s[lo + 2..e];
        let kw_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
        let kw = &inner[..kw_end];
        let head = trim_range(self.s, lo + 2 + kw_end, e);
        self.i = e + 1;
        match kw {
            "if" => {
                if head.0 == head.1 {
                    return err(lo, e + 1, "`{#if}` needs a condition");
                }
                let mut branches = Vec::new();
                let mut cond = Some(head);
                loop {
                    let (nodes, end) = self.nodes(false)?;
                    branches.push(Branch { cond, vals: Vec::new(), nodes });
                    match end {
                        End::Else(c, r) => {
                            if cond.is_none() {
                                return err(r.0, r.1, "`{:else}` must be the last branch");
                            }
                            cond = c;
                        }
                        End::EndIf(r) => {
                            return Ok(Node::If(IfNode { branches, scrut: None, choice: None, range: (lo, r.1), id: Cell::new(0) }));
                        }
                        End::Eof => return err(lo, e + 1, "`{#if}` is never closed with `{/if}`"),
                        End::Close(t, r) => return err(r.0, r.1, format!("`</{t}>` found inside `{{#if}}`; close the block first")),
                        End::EndEach(r) => return err(r.0, r.1, "`{/each}` found where `{/if}` was expected"),
                        End::Case(_, r) => return err(r.0, r.1, "`{:case}` outside `{#match}`"),
                        End::EndMatch(r) => return err(r.0, r.1, "`{/match}` found where `{/if}` was expected"),
                    }
                }
            }
            "let" => {
                let bad = || err(lo, e + 1, "`{#let}` needs `name = expr`").map_err(|x| x.help("`{#let f = state.files[id]}`; it has no `{/let}`"));
                let h = &self.s[head.0..head.1];
                let Some(eq) = h.find('=') else { return bad() };
                let name = h[..eq].trim();
                let valid = !name.is_empty() && !name.starts_with(|c: char| c.is_ascii_digit()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                let expr = trim_range(self.s, head.0 + eq + 1, head.1);
                if !valid || expr.0 == expr.1 || h[eq + 1..].starts_with('=') {
                    return bad();
                }
                let a = head.0 + h.find(name).unwrap();
                Ok(Node::Let(LetNode { name: name.to_string(), name_r: (a, a + name.len()), expr, range: (lo, e + 1) }))
            }
            "each" => {
                if head.0 == head.1 {
                    return err(lo, e + 1, "`{#each}` needs `X as x`");
                }
                let (body, end) = self.nodes(false)?;
                match end {
                    End::EndEach(r) => Ok(Node::Each(EachNode { head, body, range: (lo, r.1), id: Cell::new(0) })),
                    End::Eof => err(lo, e + 1, "`{#each}` is never closed with `{/each}`"),
                    End::Else(_, r) => err(r.0, r.1, "`{#each}` has no `{:else}`"),
                    End::EndIf(r) => err(r.0, r.1, "`{/if}` found where `{/each}` was expected"),
                    End::Case(_, r) => err(r.0, r.1, "`{:case}` outside `{#match}`"),
                    End::EndMatch(r) => err(r.0, r.1, "`{/match}` found where `{/each}` was expected"),
                    End::Close(t, r) => err(r.0, r.1, format!("`</{t}>` found inside `{{#each}}`; close the block first")),
                }
            }
            "match" => {
                if head.0 == head.1 {
                    return err(lo, e + 1, "`{#match}` needs a value");
                }
                let mut branches: Vec<Branch> = Vec::new();
                let (src, mut seen): (&str, Vec<&str>) = (self.s, Vec::new());
                let (pre, mut end) = self.nodes(false)?;
                if pre.iter().any(|n| !matches!(n, Node::Text { parts, .. } if parts.iter().all(|p| matches!(p, Part::Lit(l) if l.trim().is_empty())))) {
                    return err(lo, e + 1, "`{#match}` holds only `{:case}` and `{:else}`; put content inside a case");
                }
                loop {
                    let vals = match end {
                        End::Case(v, r) => {
                            if branches.last().is_some_and(|b| b.vals.is_empty()) {
                                return err(r.0, r.1, "`{:case}` after `{:else}`; `{:else}` must be last");
                            }
                            for &(a, b) in &v {
                                let t = &src[a..b];
                                if seen.contains(&t) {
                                    return err(a, b, format!("duplicate `{{:case {t}}}`: the first one wins, so this branch would never run"));
                                }
                                seen.push(t);
                            }
                            v
                        }
                        End::Else(None, r) => {
                            if branches.last().is_some_and(|b| b.vals.is_empty()) {
                                return err(r.0, r.1, "`{:else}` must be the last branch");
                            }
                            Vec::new()
                        }
                        End::Else(Some(_), r) => return err(r.0, r.1, "`{:else if}` isn't allowed in `{#match}`; use `{:case a, b}` or `{:else}`"),
                        End::EndMatch(r) => {
                            if branches.is_empty() {
                                return err(lo, e + 1, "`{#match}` has no `{:case}`");
                            }
                            return Ok(Node::If(IfNode { branches, scrut: Some(head), choice: None, range: (lo, r.1), id: Cell::new(0) }));
                        }
                        End::Eof => return err(lo, e + 1, "`{#match}` is never closed with `{/match}`"),
                        End::Close(t, r) => return err(r.0, r.1, format!("`</{t}>` found inside `{{#match}}`; close the block first")),
                        End::EndIf(r) => return err(r.0, r.1, "`{/if}` found where `{/match}` was expected"),
                        End::EndEach(r) => return err(r.0, r.1, "`{/each}` found where `{/match}` was expected"),
                    };
                    let (nodes, next) = self.nodes(false)?;
                    branches.push(Branch { cond: None, vals, nodes });
                    end = next;
                }
            }
            _ => err(lo, e + 1, format!("unknown block `{{#{kw}}}`")).map_err(|x| x.help("blocks: #if, #match, #each and #let")),
        }
    }

    fn element(&mut self, top: bool) -> R<Option<Elem>> {
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
                if r.0 == r.1 {
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

pub fn trim_range(s: &str, lo: usize, hi: usize) -> Range {
    // ASCII whitespace only, like HTML: a no-break space (U+00A0) is content
    let t = &s[lo..hi];
    let a = lo + (t.len() - t.trim_start_matches(|c: char| c.is_ascii_whitespace()).len());
    let b = hi - (t.len() - t.trim_end_matches(|c: char| c.is_ascii_whitespace()).len());
    if b < a {
        (lo, lo)
    } else {
        (a, b)
    }
}

fn collapse(s: &str) -> String {
    let mut out = String::new();
    let mut ws = false;
    for c in s.chars() {
        if c.is_ascii_whitespace() {
            ws = true;
        } else {
            if ws {
                out.push(' ');
                ws = false;
            }
            out.push(c);
        }
    }
    if ws {
        out.push(' ');
    }
    // a leading space survives only when the original began with whitespace
    if s.starts_with(|c: char| c.is_ascii_whitespace()) && !out.starts_with(' ') {
        out.insert(0, ' ');
    }
    out
}

fn decode(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&lbrace;", "{")
        .replace("&rbrace;", "}")
        .replace("&nbsp;", "\u{a0}")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
}

// ---------------------------------------------------------------------------------------
// <script>

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn braces_skip_strings() {
        let m = parse("<div>{f(\"}\")} a {b}</div>").unwrap();
        let Node::Elem(e) = &m.nodes[0] else { panic!() };
        let Node::Text { parts, .. } = &e.children[0] else { panic!() };
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn unclosed_tag_has_position() {
        let e = parse("<div>\n<span>x</div>").unwrap_err();
        assert!(e.msg.contains("expected `</span>`"), "{}", e.msg);
    }
}
