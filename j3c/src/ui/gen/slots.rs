use super::*;

/// A use of a component with children: which slots it fills.
pub(super) struct Site {
    /// 1-based; names the generated `slot<id>_<name>_update` methods on the using component.
    pub(super) id: usize,
    pub(super) child: String,
    /// The using component's J3 name.
    pub(super) owner: String,
    pub(super) names: Vec<String>,
}

#[derive(Default)]
pub(super) struct Slots {
    pub(super) list: Vec<Site>,
    /// Component elements with children, by address, to their `list` index.
    pub(super) by_addr: HashMap<usize, usize>,
    /// Direct children carrying a valid `slot="x"`.
    pub(super) slotted: HashSet<usize>,
    /// What the uses of each component pass as `class` / `style:` (see `pass`), by component name.
    pub(super) pass: HashMap<String, Pass>,
}

pub(super) fn addr(e: &Elem) -> usize {
    e as *const Elem as usize
}

/// The elements a `{#each}` body makes one row of (its own, and those of its `{#if}` branches).
pub(super) fn row_elems(nodes: &[Node], out: &mut Vec<usize>) {
    for n in nodes {
        match n {
            Node::Elem(e) => out.push(addr(e)),
            Node::If(i) => i.branches.iter().for_each(|b| row_elems(&b.nodes, out)),
            _ => {}
        }
    }
}

pub(super) fn lit_value(v: &Value) -> Option<String> {
    match v {
        Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => Some(p.iter().map(|x| if let Part::Lit(l) = x { l.as_str() } else { "" }).collect::<String>().trim().to_string()),
        _ => None,
    }
}

/// The slot a child of a component is passed to (`slot="x"`; "" is the default slot).
pub(super) fn slot_of(n: &Node) -> String {
    slot_elem(n).and_then(|e| e.attrs.iter().find(|a| a.name == "slot")).and_then(|a| lit_value(&a.value)).unwrap_or_default()
}

pub(super) fn slot_elem(n: &Node) -> Option<&Elem> {
    match n {
        Node::Elem(e) => Some(e),
        Node::If(i) if i.choice.is_some() => i.branches.first().and_then(|b| b.nodes.first()).and_then(slot_elem),
        _ => None,
    }
}

pub(super) fn mark_slotted(n: &Node, out: &mut HashSet<usize>) {
    match n {
        Node::Elem(e) => { out.insert(addr(e)); }
        Node::If(i) if i.choice.is_some() => {
            for b in &i.branches {
                mark_slotted(&b.nodes[0], out);
            }
        }
        _ => {}
    }
}

/// The `<slot>`s a component declares (name "" is the default), anywhere in its markup.
pub(super) fn declared(nodes: &[Node], out: &mut Vec<String>) -> R<()> {
    for n in nodes {
        match n {
            Node::Text { .. } => {}
            Node::Elem(e) => {
                if e.tag == "slot" {
                    let mut name = String::new();
                    for a in &e.attrs {
                        match (a.name.as_str(), lit_value(&a.value)) {
                            ("name", Some(v)) if v == "default" => return err(a.range.0, a.range.1, "`<slot name=\"default\">` is the default slot: use `<slot />`"),
                            ("name", Some(v)) => name = v,
                            _ => return err(a.range.0, a.range.1, "`<slot>` takes only a plain `name=\"...\"`"),
                        }
                    }
                    if out.contains(&name) {
                        let msg = if name.is_empty() { "a component has one default `<slot />`".to_string() } else { format!("two `<slot name=\"{name}\">` in one component") };
                        return err(e.open.0, e.open.1, msg);
                    }
                    out.push(name);
                }
                declared(&e.children, out)?;
            }
            Node::If(i) => i.branches.iter().try_for_each(|b| declared(&b.nodes, out))?,
            Node::Each(e) => declared(&e.body, out)?,
            Node::Let(_) => {}
        }
    }
    Ok(())
}

/// Finds every component use that passes children and checks them against the child's slots.
pub(super) fn scan_sites(nodes: &[Node], all: &[Comp], decl: &[Vec<String>], owner: &str, s: &mut Slots) -> R<()> {
    for n in nodes {
        match n {
            Node::Text { .. } => {}
            Node::Elem(e) => {
                if e.comp && !e.children.is_empty() {
                    if let Some(ci) = all.iter().position(|c| c.name == e.tag) {
                        let want = &decl[ci];
                        if want.is_empty() {
                            return err(e.open.0, e.open.1, format!("`<{}>` takes no children (it has no `<slot />`)", e.tag));
                        }
                        let mut names: Vec<String> = Vec::new();
                        for ch in &e.children {
                            let mut at = e.open;
                            if let Some(ce) = slot_elem(ch) {
                                if let Some(a) = ce.attrs.iter().find(|a| a.name == "slot") {
                                    if lit_value(&a.value).is_none_or(|v| v.is_empty()) {
                                        return err(a.range.0, a.range.1, "`slot` takes a plain name: `slot=\"footer\"`");
                                    }
                                    mark_slotted(ch, &mut s.slotted);
                                    at = a.range;
                                }
                            }
                            let nm = slot_of(ch);
                            if !want.contains(&nm) {
                                let have = want.iter().map(|w| if w.is_empty() { "default".to_string() } else { w.clone() }).collect::<Vec<_>>().join(", ");
                                let msg = if nm.is_empty() { format!("`<{}>` has no default `<slot />` for these children", e.tag) } else { format!("`<{}>` has no `<slot name=\"{nm}\">`", e.tag) };
                                return Err(UiError { lo: at.0, hi: at.1, msg, help: Some(format!("`{}` slots: {have}", e.tag)) });
                            }
                            if !names.contains(&nm) {
                                names.push(nm);
                            }
                        }
                        s.by_addr.insert(addr(e), s.list.len());
                        s.list.push(Site { id: s.list.len() + 1, child: e.tag.clone(), owner: owner.to_string(), names });
                    }
                }
                scan_sites(&e.children, all, decl, owner, s)?;
            }
            Node::If(i) => i.branches.iter().try_for_each(|b| scan_sites(&b.nodes, all, decl, owner, s))?,
            Node::Each(e) => scan_sites(&e.body, all, decl, owner, s)?,
            Node::Let(_) => {}
        }
    }
    Ok(())
}

/// Expand before all shared scans so candidates behave exactly like ordinary match use sites.
pub(super) fn lower_choices(nodes: &mut [Node], src: &str, known: &[String], sets: &mut std::collections::BTreeMap<String, Vec<String>>) -> R<()> {
    for n in nodes {
        match n {
            Node::Elem(e) => {
                lower_choices(&mut e.children, src, known, sets)?;
                if e.tag != "component" {
                    continue;
                }
                let mut seen = HashSet::new();
                for a in &e.attrs {
                    if !seen.insert(&a.name) {
                        return err(a.range.0, a.range.1, format!("duplicate attribute `{}` on `<component>`", a.name));
                    }
                    if a.name.starts_with(pass::PREFIX) {
                        return err(a.range.0, a.range.1, format!("reserved attribute `{}`", a.name));
                    }
                }
                let expr = |name: &str| -> R<Range> {
                    match e.attrs.iter().find(|a| a.name == name) {
                        Some(Attr { value: Value::Expr(r), .. }) if !src[r.0..r.1].trim().is_empty() => Ok(*r),
                        _ => err(e.open.0, e.open.1, format!("`<component>` needs `{name}={{...}}`")),
                    }
                };
                let scrut = expr("this")?;
                let of = expr("of")?;
                let mut candidates = Vec::new();
                for name in src[of.0..of.1].split(',').map(str::trim) {
                    if !known.iter().any(|k| k == name) {
                        return err(of.0, of.1, format!("unknown component `{name}` in `of` (expected a comma-separated component list)"));
                    }
                    if candidates.iter().any(|k| k == name) {
                        return err(of.0, of.1, format!("duplicate component `{name}` in `of`"));
                    }
                    candidates.push(name.to_string());
                }
                let set = format!("Of_{}", candidates.iter().map(|n| n.replace('.', "_")).collect::<Vec<_>>().join("_"));
                if let Some(prev) = sets.insert(set.clone(), candidates.clone()) {
                    if prev != candidates {
                        return err(of.0, of.1, format!("candidate lists generate the same enum `{set}`; rename a component"));
                    }
                }
                let branches = candidates.into_iter().map(|tag| {
                    let mut child = e.clone();
                    child.tag = tag;
                    child.comp = true;
                    child.attrs.retain(|a| a.name != "this" && a.name != "of");
                    markup::Branch { cond: None, vals: Vec::new(), nodes: vec![Node::Elem(child)] }
                }).collect();
                *n = Node::If(IfNode { branches, scrut: Some(scrut), choice: Some(set), range: e.open, id: std::cell::Cell::new(0) });
            }
            Node::If(i) => {
                for b in &mut i.branches {
                    lower_choices(&mut b.nodes, src, known, sets)?;
                }
            }
            Node::Each(e) => lower_choices(&mut e.body, src, known, sets)?,
            Node::Text { .. } | Node::Let(_) => {}
        }
    }
    Ok(())
}
