use super::*;

#[derive(Debug)]
pub struct Prop {
    pub name: String,
    pub name_r: Range,
    pub ty: Range,
    /// Added by `provide`/`inject` resolution: the type as text, and the parent supplies it.
    pub hidden: Option<String>,
    /// `props { gap: int = 8; }`: the source range of the default, used when a use site omits the prop.
    pub default: Option<Range>,
}

/// One payload field of an event: `pick(slot: int)`.
#[derive(Debug)]
pub struct EvParam {
    pub ev: String,
    pub name: String,
    pub name_r: Range,
    pub ty: Range,
}

#[derive(Debug)]
pub struct LetDef {
    pub name: String,
    pub name_r: Range,
    pub ty: Option<Range>,
    pub init: Range,
    /// Type read off a literal initialiser, when there is no annotation.
    pub guess: Option<&'static str>,
}

#[derive(Debug)]
pub struct FnDef {
    pub name: String,
    pub name_r: Range,
    pub params: Range,
    pub param_names: Vec<(String, Range)>,
    pub ret: Option<Range>,
    pub body: Range,
}

#[derive(Debug, Default)]
pub struct Script {
    pub uses: Vec<Range>,
    pub props: Vec<Prop>,
    pub events: Vec<(String, Range)>,
    pub ev_params: Vec<EvParam>,
    /// `provide a, b;`
    pub provides: Vec<(String, Range)>,
    /// `inject name: Type;` (name, type range)
    pub injects: Vec<(String, Range, Range)>,
    pub lets: Vec<LetDef>,
    /// `const NAME: Type = value;`
    pub consts: Vec<LetDef>,
    pub fns: Vec<FnDef>,
}

pub fn parse_script(src: &str, r: Range) -> R<Script> {
    let base = r.0;
    let toks = lex(&src[r.0..r.1], FileId(0)).map_err(|e| UiError {
        lo: base + e.span.start as usize,
        hi: base + (e.span.end as usize).max(e.span.start as usize + 1),
        msg: e.message,
        help: None,
    })?;
    let sp = |k: usize| (base + toks[k].span.start as usize, base + toks[k].span.end as usize);
    let n = toks.len();
    let mut sc = Script::default();
    let mut i = 0;
    // index of the token closing the bracket opened at `open`
    let close = |open: usize| -> R<usize> {
        let mut depth = 0i32;
        for k in open..n {
            match toks[k].kind {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(k);
                    }
                }
                _ => {}
            }
        }
        let (a, b) = (base + toks[open].span.start as usize, base + toks[open].span.end as usize);
        err(a, b, "this bracket is never closed")
    };
    // first token at bracket depth 0 (from `from`) of one of `kinds`
    let find = |from: usize, kinds: &[TokenKind]| -> Option<usize> {
        let mut depth = 0i32;
        for k in from..n {
            let t = toks[k].kind;
            if depth == 0 && kinds.contains(&t) {
                return Some(k);
            }
            match t {
                TokenKind::LBrace | TokenKind::LParen | TokenKind::LBracket => depth += 1,
                TokenKind::RBrace | TokenKind::RParen | TokenKind::RBracket => depth -= 1,
                _ => {}
            }
        }
        None
    };
    while i < n && toks[i].kind != TokenKind::Eof {
        let t = &toks[i];
        match t.kind {
            TokenKind::Doc => i += 1,
            TokenKind::Use => {
                let Some(semi) = find(i, &[TokenKind::Semi]) else {
                    let (a, b) = sp(i);
                    return err(a, b, "`use` needs a closing `;`");
                };
                sc.uses.push((sp(i).0, sp(semi).1));
                i = semi + 1;
            }
            TokenKind::Ident if (t.text == "props" || t.text == "events") && toks.get(i + 1).map(|x| x.kind) == Some(TokenKind::LBrace) => {
                let end = close(i + 1)?;
                let mut k = i + 2;
                while k < end {
                    if toks[k].kind == TokenKind::Doc {
                        k += 1;
                        continue;
                    }
                    if toks[k].kind != TokenKind::Ident {
                        let (a, b) = sp(k);
                        return err(a, b, format!("expected a name inside `{} {{ }}`", t.text));
                    }
                    let name = toks[k].text.clone();
                    let name_r = sp(k);
                    k += 1;
                    if t.text == "events" {
                        sc.events.push((name.clone(), name_r));
                        if k < end && toks[k].kind == TokenKind::LParen {
                            let rp = close(k)?;
                            let mut q = k + 1;
                            while q < rp {
                                if toks[q].kind != TokenKind::Ident || toks.get(q + 1).map(|x| x.kind) != Some(TokenKind::Colon) {
                                    let (a, b) = sp(q);
                                    return err(a, b, format!("expected `name: Type` in the payload of `{name}`"));
                                }
                                let ty_lo = q + 2;
                                let stop = find(ty_lo, &[TokenKind::Comma, TokenKind::RParen]).filter(|s| *s > ty_lo).unwrap_or(rp).min(rp);
                                if stop <= ty_lo {
                                    let (a, b) = sp(q);
                                    return err(a, b, format!("payload `{}` needs a type", toks[q].text));
                                }
                                sc.ev_params.push(EvParam { ev: name.clone(), name: toks[q].text.clone(), name_r: sp(q), ty: (sp(ty_lo).0, sp(stop - 1).1) });
                                q = stop + 1;
                            }
                            k = rp + 1;
                        }
                        if k < end && matches!(toks[k].kind, TokenKind::Semi | TokenKind::Comma) {
                            k += 1;
                        } else if k < end {
                            let (a, b) = sp(k);
                            return err(a, b, "expected `;` after the event name");
                        }
                        continue;
                    }
                    if toks.get(k).map(|x| x.kind) != Some(TokenKind::Colon) || k >= end {
                        return err(name_r.0, name_r.1, format!("prop `{name}` needs a type: `{name}: Type;`"));
                    }
                    k += 1;
                    let ty_lo = k;
                    let mut angle = 0i32;
                    while k < end {
                        match toks[k].kind {
                            TokenKind::Lt => angle += 1,
                            TokenKind::Gt => angle -= 1,
                            TokenKind::Semi | TokenKind::Comma | TokenKind::Eq if angle <= 0 => break,
                            _ => {}
                        }
                        k += 1;
                    }
                    if k == ty_lo {
                        return err(name_r.0, name_r.1, format!("prop `{name}` needs a type"));
                    }
                    let ty = (sp(ty_lo).0, sp(k - 1).1);
                    let mut default = None;
                    if k < end && toks[k].kind == TokenKind::Eq {
                        let lo = k + 1;
                        let mut depth = 0i32;
                        k = lo;
                        while k < end {
                            match toks[k].kind {
                                TokenKind::LParen | TokenKind::LBrace | TokenKind::LBracket => depth += 1,
                                TokenKind::RParen | TokenKind::RBrace | TokenKind::RBracket => depth -= 1,
                                TokenKind::Semi | TokenKind::Comma if depth <= 0 => break,
                                _ => {}
                            }
                            k += 1;
                        }
                        if k == lo {
                            return err(name_r.0, name_r.1, format!("prop `{name}` has `=` but no default value"));
                        }
                        default = Some((sp(lo).0, sp(k - 1).1));
                    }
                    sc.props.push(Prop { name, name_r, ty, hidden: None, default });
                    if k < end {
                        k += 1;
                    }
                }
                i = end + 1;
            }
            TokenKind::Ident if (t.text == "provide" || t.text == "inject") && toks.get(i + 1).map(|x| x.kind) == Some(TokenKind::Ident) => {
                let Some(semi) = find(i, &[TokenKind::Semi]) else {
                    let (a, b) = sp(i);
                    return err(a, b, format!("`{}` needs a closing `;`", t.text));
                };
                if t.text == "provide" {
                    for k in i + 1..semi {
                        match toks[k].kind {
                            TokenKind::Ident if k == i + 1 || toks[k - 1].kind == TokenKind::Comma => sc.provides.push((toks[k].text.clone(), sp(k))),
                            TokenKind::Comma => {}
                            _ => {
                                let (a, b) = sp(k);
                                return err(a, b, "`provide` takes names: `provide demo, theme;`");
                            }
                        }
                    }
                } else {
                    if toks[i + 2].kind != TokenKind::Colon || i + 3 >= semi {
                        let (a, b) = sp(i + 1);
                        return err(a, b, format!("`inject {}` needs a type: `inject {}: Type;`", toks[i + 1].text, toks[i + 1].text));
                    }
                    sc.injects.push((toks[i + 1].text.clone(), sp(i + 1), (sp(i + 3).0, sp(semi - 1).1)));
                }
                i = semi + 1;
            }
            TokenKind::Let | TokenKind::Const => {
                let kw = if t.kind == TokenKind::Const { "const" } else { "let" };
                let Some(semi) = find(i, &[TokenKind::Semi]) else {
                    let (a, b) = sp(i);
                    return err(a, b, format!("`{kw}` needs a closing `;`"));
                };
                if toks.get(i + 1).map(|x| x.kind) != Some(TokenKind::Ident) {
                    let (a, b) = sp(i);
                    return err(a, b, format!("expected a name after `{kw}`"));
                }
                let name = toks[i + 1].text.clone();
                let name_r = sp(i + 1);
                let mut k = i + 2;
                let mut ty = None;
                if toks[k].kind == TokenKind::Colon {
                    let eq = find(k, &[TokenKind::Eq]).filter(|e| *e < semi);
                    let Some(eq) = eq else {
                        return err(name_r.0, name_r.1, format!("`{kw} {name}` needs `= value`"));
                    };
                    if eq == k + 1 {
                        return err(name_r.0, name_r.1, format!("`{kw} {name}:` needs a type"));
                    }
                    ty = Some((sp(k + 1).0, sp(eq - 1).1));
                    k = eq;
                }
                if toks[k].kind != TokenKind::Eq || k + 1 >= semi {
                    return err(name_r.0, name_r.1, format!("`{kw} {name}` needs `= value`"));
                }
                let init = (sp(k + 1).0, sp(semi - 1).1);
                let lit = &toks[k + 1..semi];
                let lit = match lit {
                    [t] | [_, t] if lit.len() == 1 || lit[0].kind == TokenKind::Minus => Some(t),
                    _ => None,
                };
                let guess = lit.and_then(|t| match t.kind {
                    TokenKind::Int => Some("int"),
                    TokenKind::Float => Some("float"),
                    TokenKind::String => Some("string"),
                    TokenKind::Ident if t.text == "true" || t.text == "false" => Some("bool"),
                    _ => None,
                });
                let d = LetDef { name, name_r, ty, init, guess };
                if kw == "const" { sc.consts.push(d) } else { sc.lets.push(d) }
                i = semi + 1;
            }
            TokenKind::Fn => {
                if toks.get(i + 1).map(|x| x.kind) != Some(TokenKind::Ident) || toks.get(i + 2).map(|x| x.kind) != Some(TokenKind::LParen) {
                    let (a, b) = sp(i);
                    return err(a, b, "expected `fn name(...)`");
                }
                let name = toks[i + 1].text.clone();
                let name_r = sp(i + 1);
                let rp = close(i + 2)?;
                let Some(lb) = find(rp + 1, &[TokenKind::LBrace]) else {
                    return err(name_r.0, name_r.1, format!("`fn {name}` has no body"));
                };
                let rb = close(lb)?;
                let mut param_names = Vec::new();
                for k in i + 3..rp {
                    if toks[k].kind == TokenKind::Ident && toks[k + 1].kind == TokenKind::Colon && (k == i + 3 || toks[k - 1].kind == TokenKind::Comma) {
                        param_names.push((toks[k].text.clone(), sp(k)));
                    }
                }
                let params = if rp == i + 3 { (sp(rp).0, sp(rp).0) } else { (sp(i + 3).0, sp(rp - 1).1) };
                let ret = if lb > rp + 1 { Some((sp(rp + 1).0, sp(lb - 1).1)) } else { None };
                let body = (sp(lb).1, sp(rb).0);
                sc.fns.push(FnDef { name, name_r, params, param_names, ret, body });
                i = rb + 1;
            }
            _ => {
                let (a, b) = sp(i);
                return err(a, b, "not allowed in `<script>`").map_err(|x| x.help("a component script holds `use`, `props { }`, `events { }`, `provide`, `inject`, `let`, `const`, and `fn`"));
            }
        }
    }
    Ok(sc)
}
