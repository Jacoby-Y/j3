use super::*;

pub(super) struct Rw<'a> {
    pub(super) src: &'a str,
    pub(super) fi: usize,
    pub(super) names: &'a Names,
    pub(super) locals: &'a [String],
    /// Names to write differently (a keyed `{#each}` binds its item twice, and J3 has no shadowing).
    pub(super) rename: &'a [(String, String)],
    pub(super) emit_ok: bool,
    /// Inside a script `fn`: `emit` writes the component's `sev` (merged into the update's events), not the local `ev`.
    pub(super) sev: bool,
    /// Calling a script fn that emits is fine here: handlers and fns, not a markup expression (it would emit on every update), `on:draw` or `onDestroy`.
    pub(super) calls_emit: bool,
}

impl<'a> Rw<'a> {
    pub(super) fn declared(&self, n: &str) -> bool {
        self.names.props.iter().chain(&self.names.lets).chain(&self.names.fns).chain(self.locals).any(|x| x == n) || self.names.consts.iter().any(|x| x.0 == n) || self.rename.iter().any(|x| x.0 == n)
    }
}

pub(super) fn rewrite(rw: &Rw, out: &mut Out, used: &mut BTreeSet<String>, lo: usize, hi: usize, flat: bool) -> R<()> {
    // `$slots` is not J3: lex it as `_slots` (same length) and recognise it by the `$` in the source
    let text = rw.src[lo..hi].replace("$slots", "_slots");
    let toks = lex(&text, FileId(0)).map_err(|e| UiError {
        lo: lo + e.span.start as usize,
        hi: lo + (e.span.end as usize).max(e.span.start as usize + 1),
        msg: e.message,
        help: None,
    })?;
    let put = |out: &mut Out, a: usize, b: usize| {
        if flat {
            out.copy_flat(rw.fi, rw.src, a, b)
        } else {
            out.copy(rw.fi, rw.src, a, b)
        }
    };
    let mut pos = lo;
    let mut k = 0;
    while k < toks.len() {
        let t = &toks[k];
        let (a, b) = (lo + t.span.start as usize, lo + t.span.end as usize);
        let prev = if k > 0 { Some(toks[k - 1].kind) } else { None };
        let next = toks.get(k + 1).map(|x| x.kind);
        match t.kind {
            TokenKind::Eof => break,
            TokenKind::Interpolate => {
                put(out, pos, a + 1);
                let mut litpos = a + 1;
                let bytes = rw.src.as_bytes();
                let mut i = a + 1;
                while i < b - 1 {
                    if bytes[i] == b'{' {
                        let mut depth = 1;
                        let mut j = i + 1;
                        while j < b - 1 {
                            if bytes[j] == b'{' {
                                depth += 1;
                            } else if bytes[j] == b'}' {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            j += 1;
                        }
                        put(out, litpos, i + 1);
                        rewrite(rw, out, used, i + 1, j, true)?;
                        litpos = j;
                        i = j;
                    }
                    i += 1;
                }
                pos = litpos;
            }
            TokenKind::Ident => {
                if t.text == "_slots" && rw.src.as_bytes()[a] == b'$' {
                    let name = (toks.get(k + 1).map(|x| x.kind) == Some(TokenKind::Dot)).then(|| toks.get(k + 2)).flatten().filter(|x| x.kind == TokenKind::Ident);
                    let Some(nm) = name else {
                        return err(a, b, "`$slots` needs a slot name: `$slots.footer` (or `$slots.default`)");
                    };
                    let want = if nm.text == "default" { "" } else { nm.text.as_str() };
                    let ne = lo + nm.span.end as usize;
                    let Some(_) = rw.names.slot_has.iter().find(|x| x.0 == want) else {
                        let have: Vec<String> = rw.names.slot_has.iter().map(|x| if x.0.is_empty() { "default".into() } else { x.0.clone() }).collect();
                        let help = if have.is_empty() { "this component has no `<slot>`".to_string() } else { format!("slots: {}", have.join(", ")) };
                        return Err(UiError { lo: a, hi: ne, msg: format!("`$slots.{}`: no `<slot name=\"{}\">` in this component", nm.text, nm.text), help: Some(help) });
                    };
                    put(out, pos, a);
                    out.at(rw.fi, a, &format!("this.{}()", slot_has_fn(want)));
                    pos = ne;
                    k += 3;
                    continue;
                }
                if t.text == "emit" && next == Some(TokenKind::Ident) && matches!(prev, None | Some(TokenKind::Semi) | Some(TokenKind::LBrace) | Some(TokenKind::RBrace)) {
                    let ev = &toks[k + 1];
                    let (ea, eb) = (lo + ev.span.start as usize, lo + ev.span.end as usize);
                    if !rw.emit_ok {
                        return err(a, eb, if rw.sev { "`emit` doesn't work in `onDestroy`: the component is going away" } else { "`emit` only works in an event handler or a script `fn`, not in `on:draw`" });
                    }
                    if !rw.names.events.contains(&ev.text) {
                        let help = if rw.names.events.is_empty() { "this component declares no events; add `events { name; }` to its <script>".to_string() } else { format!("declared events: {}", rw.names.events.join(", ")) };
                        return Err(UiError { lo: ea, hi: eb, msg: format!("`emit {}`: `{}` is not in `events {{ }}`", ev.text, ev.text), help: Some(help) });
                    }
                    let ps: Vec<&str> = rw.names.payload.iter().filter(|x| x.0 == ev.text).map(|x| x.1.as_str()).collect();
                    // `emit pick(a, b)`: the argument ranges between the parentheses, split at depth 1
                    let mut args: Vec<(usize, usize)> = Vec::new();
                    let mut after = k + 2;
                    if toks.get(after).map(|x| x.kind) == Some(TokenKind::LParen) {
                        let (mut depth, mut from, mut closed) = (0i32, after + 1, false);
                        for q in after..toks.len() {
                            match toks[q].kind {
                                TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket => depth += 1,
                                TokenKind::RBrace | TokenKind::RBracket => depth -= 1,
                                TokenKind::Comma if depth == 1 => {
                                    args.push((from, q));
                                    from = q + 1;
                                }
                                TokenKind::RParen => {
                                    depth -= 1;
                                    if depth == 0 {
                                        args.push((from, q));
                                        after = q + 1;
                                        closed = true;
                                        break;
                                    }
                                }
                                _ => {}
                            }
                        }
                        if !closed {
                            return err(ea, eb, "this `emit` call is never closed");
                        }
                        if args.len() == 1 && args[0].0 == args[0].1 {
                            args.clear();
                        }
                    }
                    if args.len() != ps.len() || args.iter().any(|x| x.0 == x.1) {
                        let want = if ps.is_empty() { format!("`{}` has no payload: write `emit {}`", ev.text, ev.text) } else { format!("`{}` takes ({})", ev.text, ps.join(", ")) };
                        return Err(UiError { lo: ea, hi: lo + toks[after - 1].span.end as usize, msg: format!("`emit {}` has {} argument(s), expected {}", ev.text, args.len(), ps.len()), help: Some(want) });
                    }
                    put(out, pos, a);
                    for (q, (x, y)) in args.iter().enumerate() {
                        out.at(rw.fi, a, &format!("{}.{}_{} = ", if rw.sev { "this.sev" } else { "ev" }, ev.text, ps[q]));
                        rewrite(rw, out, used, lo + toks[*x].span.start as usize, lo + toks[*y - 1].span.end as usize, flat)?;
                        out.at(rw.fi, a, "; ");
                    }
                    out.at(rw.fi, a, if rw.sev { "this.sev." } else { "ev." });
                    put(out, ea, eb);
                    out.at(rw.fi, eb, " = true");
                    pos = lo + toks[after - 1].span.end as usize;
                    k = after;
                    continue;
                }
                if matches!(prev, Some(TokenKind::Let) | Some(TokenKind::For)) {
                    if rw.declared(&t.text) || is_reserved(&t.text) {
                        return Err(UiError { lo: a, hi: b, msg: format!("`{}` shadows a prop, `let`, `fn`, or name used by the generated code", t.text), help: Some("pick another name for this local".into()) });
                    }
                } else if !matches!(prev, Some(TokenKind::Dot) | Some(TokenKind::ColonColon)) && next != Some(TokenKind::Colon) && !rw.locals.contains(&t.text) {
                    if let Some((_, to)) = rw.rename.iter().find(|x| x.0 == t.text) {
                        put(out, pos, a);
                        out.at(rw.fi, a, to);
                        pos = b;
                    } else if let Some((_, to)) = rw.names.consts.iter().find(|x| x.0 == t.text) {
                        if matches!(next, Some(TokenKind::Eq)) {
                            return err(a, b, format!("`{}` is a `const`; it can't be assigned", t.text));
                        }
                        put(out, pos, a);
                        out.at(rw.fi, a, to);
                        pos = b;
                    } else if rw.names.props.contains(&t.text) {
                        used.insert(t.text.clone());
                    } else if rw.names.lets.contains(&t.text) || rw.names.fns.contains(&t.text) {
                        if !rw.calls_emit && rw.names.emitting.contains(&t.text) {
                            return Err(UiError { lo: a, hi: b, msg: format!("`{}` emits an event, but there is no event scope here", t.text), help: Some("call it from an event handler or another script `fn`: a markup expression runs on every update, and `on:draw` and `onDestroy` have no event scope".into()) });
                        }
                        put(out, pos, a);
                        out.at(rw.fi, a, "this.");
                        pos = a;
                    }
                }
            }
            _ => {}
        }
        k += 1;
    }
    put(out, pos, hi);
    Ok(())
}

// ---------------------------------------------------------------------------------------
