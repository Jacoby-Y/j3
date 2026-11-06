//! `{#if}` / `{#match}` / `{#each}` / `{#let}` blocks and their closing tags.

use super::*;

impl<'a> P<'a> {
    pub(super) fn block_end(&mut self) -> R<End> {
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

    pub(super) fn block(&mut self) -> R<Node> {
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
}
