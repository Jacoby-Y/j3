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

mod block;
mod element;
mod script;
mod types;

pub use script::*;
pub use types::*;

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
