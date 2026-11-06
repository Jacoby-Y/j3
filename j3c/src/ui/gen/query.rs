//! Read-only queries over elements, nodes and script text that the generator asks.

use super::*;

/// The literal `type` of an `<input>` ("text" when it has none), `None` when it is an expression.
pub(super) fn input_type(e: &Elem) -> Option<String> {
    for a in &e.attrs {
        if a.name == "type" {
            return match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => Some(p.iter().map(|x| if let Part::Lit(l) = x { l.as_str() } else { "" }).collect()),
                _ => None,
            };
        }
    }
    Some("text".into())
}

/// `(is a number, code)` for the attributes that fill `j3.ui.media.Media` (`M_*` and `N_*` there).
pub(super) fn media_code(name: &str) -> Option<(bool, i32)> {
    Some(match name {
        "src" => (false, 0),
        "sprite" => (false, 1),
        "viewBox" => (false, 2),
        "d" => (false, 3),
        "fill" => (false, 4),
        "stroke" => (false, 5),
        "fill-rule" => (false, 6),
        "frame" => (true, 0),
        "fps" => (true, 1),
        "stroke-width" => (true, 2),
        _ => return None,
    })
}

pub(super) fn is_range(e: &Elem) -> bool {
    e.tag == "input" && input_type(e).as_deref() == Some("range")
}

pub(super) fn is_radio(e: &Elem) -> bool {
    e.tag == "input" && input_type(e).as_deref() == Some("radio")
}

pub(super) fn width(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Text { .. } | Node::Let(_) => 0,
            Node::Elem(_) | Node::Each(_) => 1,
            Node::If(i) => i.branches.iter().map(|b| width(&b.nodes)).sum(),
        })
        .sum()
}

pub(super) fn holds(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Text { .. } | Node::Let(_) => false,
        Node::Elem(e) => e.comp || e.attrs.iter().any(|a| a.name.starts_with("use:") || a.name == "bind:this") || holds(&e.children),
        Node::If(i) => i.branches.iter().any(|b| holds(&b.nodes)),
        Node::Each(e) => holds(&e.body),
    })
}

/// Numbers child instances, stateful `{#if}`s and `{#each}`s in source order.
pub(super) fn number(nodes: &[Node], cn: &mut usize, ifn: &mut usize, en: &mut usize) {
    for n in nodes {
        match n {
            Node::Text { .. } | Node::Let(_) => {}
            Node::Elem(e) => {
                if e.comp {
                    *cn += 1;
                    e.id.set(*cn);
                }
                number(&e.children, cn, ifn, en);
            }
            Node::If(i) => {
                if i.branches.iter().any(|b| holds(&b.nodes)) {
                    *ifn += 1;
                    i.id.set(*ifn);
                }
                for b in &i.branches {
                    number(&b.nodes, cn, ifn, en);
                }
            }
            Node::Each(e) => {
                *en += 1;
                e.id.set(*en);
                number(&e.body, cn, ifn, en);
            }
        }
    }
}

pub(super) fn children_of(nodes: &[Node], out: &mut Vec<String>) {
    for n in nodes {
        match n {
            Node::Text { .. } | Node::Let(_) => {}
            Node::Elem(e) => {
                if e.comp {
                    out.push(e.tag.clone());
                }
                children_of(&e.children, out);
            }
            Node::If(i) => i.branches.iter().for_each(|b| children_of(&b.nodes, out)),
            Node::Each(e) => children_of(&e.body, out),
        }
    }
}

/// `use a.b.{ X, Y };` to `["a.b.X", "a.b.Y"]`; anything odd (aliases) is kept whole.
pub(super) fn use_names(text: &str) -> Vec<String> {
    let toks = match lex(text, FileId(0)) {
        Ok(t) => t,
        Err(_) => return vec![text.trim().trim_start_matches("use").trim().trim_end_matches(';').trim().to_string()],
    };
    let mut path: Vec<String> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut in_set = false;
    for t in toks.iter().skip(1) {
        match t.kind {
            TokenKind::Ident if in_set => names.push(t.text.clone()),
            TokenKind::Ident => path.push(t.text.clone()),
            TokenKind::LBrace => in_set = true,
            TokenKind::Dot | TokenKind::Comma | TokenKind::RBrace | TokenKind::Semi | TokenKind::Eof => {}
            _ => return vec![text.trim().trim_start_matches("use").trim().trim_end_matches(';').trim().to_string()],
        }
    }
    if names.is_empty() {
        vec![path.join(".")]
    } else {
        names.iter().map(|n| format!("{}.{n}", path.join("."))).collect()
    }
}

/// The longhand CSS property names (`PROP_NAMES` in the embedded `ui/css/props.j3`), space separated.
pub(super) fn css_props() -> &'static str {
    let src = crate::stdlib::files().find(|(n, _)| *n == "ui/css/props.j3").map_or("", |f| f.1);
    src.split("PROP_NAMES: string = \"").nth(1).and_then(|r| r.split('"').next()).unwrap_or("")
}

/// Conditionals do not add a DOM parent; components/elements do.
pub(super) fn tooltip_child(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Elem(e) => !e.comp && e.attrs.iter().any(|a| a.name == "popover" && matches!(a.value, Value::None)),
        Node::If(b) => b.branches.iter().any(|arm| tooltip_child(&arm.nodes)),
        _ => false,
    })
}

/// Option content is mirrored into popup rows: keep controls, handlers and component identities out.
pub(super) fn option_visuals(nodes: &[Node]) -> R<()> {
    for n in nodes {
        match n {
            Node::Text { .. } => {},
            Node::Elem(e) => {
                if e.comp || !["div", "span", "p", "small", "h1", "h2", "img", "svg", "path", "icon"].contains(&e.tag.as_str()) {
                    return err(e.open.0, e.open.1, "option content must be visual markup (text, div/span/p/small/h1/h2, img, svg, path, icon); put controls outside the select");
                }
                for a in &e.attrs {
                    if !(a.name == "class" || a.name.starts_with("class:") || a.name.starts_with("style:") || ["src", "alt", "sprite", "frame", "fps", "viewBox", "d", "fill", "fill-rule", "stroke", "stroke-width", "name"].contains(&a.name.as_str())) {
                        return err(a.range.0, a.range.1, "option visual descendants cannot have events, bindings, focus or overlay attributes; handle selection with on:input on the select");
                    }
                }
                option_visuals(&e.children)?;
            },
            Node::If(b) => { for arm in &b.branches { option_visuals(&arm.nodes)?; } },
            Node::Each(e) => option_visuals(&e.body)?,
            Node::Let(_) => {}
        }
    }
    Ok(())
}
