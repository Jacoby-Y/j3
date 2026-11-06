//! `provide` / `inject` resolution across components, and the use-graph cycle check.

use super::*;

pub(super) type Need = (String, String, Range);

/// What component `i` must be given: its own `inject`s and those of everything it uses that it doesn't `provide`.
pub(super) fn needs(i: usize, comps: &[Comp], edges: &[Vec<String>], files: &[SrcFile], memo: &mut Vec<Option<Vec<Need>>>) -> Result<Vec<Need>, GenError> {
    if let Some(v) = &memo[i] {
        return Ok(v.clone());
    }
    let c = &comps[i];
    let src = &files[c.fi].text;
    let e = |lo: usize, hi: usize, msg: String, help: Option<String>| ge(c.fi, UiError { lo, hi, msg, help });
    let norm = |t: &str| t.chars().filter(|x| !x.is_whitespace()).collect::<String>();
    let mut res: Vec<Need> = Vec::new();
    for (n, r, tr) in &c.sc.injects {
        if res.iter().any(|x| x.0 == *n) {
            return Err(e(r.0, r.1, format!("`inject {n}` is declared twice"), None));
        }
        res.push((n.clone(), norm(&src[tr.0..tr.1]), *r));
    }
    // the type of a name this component can provide, when it is written down
    let own = |n: &str, res: &[Need]| -> Option<Option<String>> {
        if let Some(x) = res.iter().find(|x| x.0 == n) {
            return Some(Some(x.1.clone()));
        }
        if let Some(p) = c.sc.props.iter().find(|p| p.name == n) {
            return Some(Some(norm(&src[p.ty.0..p.ty.1])));
        }
        c.sc.lets.iter().find(|l| l.name == n).map(|l| l.ty.map(|t| norm(&src[t.0..t.1])))
    };
    for (n, r) in &c.sc.provides {
        if own(n, &res).is_none() {
            return Err(e(r.0, r.1, format!("`provide {n}`: `{n}` is not a prop, `let`, or `inject` of this component"), None));
        }
    }
    let mut seen: Vec<&String> = Vec::new();
    for ch in &edges[i] {
        if seen.contains(&ch) {
            continue;
        }
        seen.push(ch);
        let at = find_tag(&c.mk.nodes, ch).unwrap_or((0, 0));
        let Some(ci) = comps.iter().position(|x| x.name == *ch) else { continue };
        for (n, ty, _) in needs(ci, comps, edges, files, memo)? {
            let mismatch = |have: &str| e(at.0, at.1, format!("`<{ch}>` injects `{n}: {ty}`, but `{}` has `{n}: {have}`", c.name), Some("provide and inject must name the same type".into()));
            if c.sc.provides.iter().any(|p| p.0 == n) {
                if let Some(Some(have)) = own(&n, &res) {
                    if have != ty {
                        return Err(mismatch(&have));
                    }
                }
            } else if let Some(x) = res.iter().find(|x| x.0 == n) {
                if x.1 != ty {
                    return Err(mismatch(&x.1.clone()));
                }
            } else if own(&n, &res).is_some() {
                return Err(e(at.0, at.1, format!("`<{ch}>` injects `{n}`, which `{}` already has as its own prop or `let`", c.name), Some(format!("add `provide {n};` to the script of `{}` to hand it down", c.name))));
            } else {
                res.push((n, ty, at));
            }
        }
    }
    memo[i] = Some(res.clone());
    Ok(res)
}

/// The chain `A -> B -> C` from `i` to a component that declares `inject name`.
pub(super) fn inject_path(i: usize, name: &str, comps: &[Comp], edges: &[Vec<String>], memo: &[Option<Vec<Need>>]) -> Vec<String> {
    let mut path = vec![comps[i].name.clone()];
    if comps[i].sc.injects.iter().any(|x| x.0 == name) {
        return path;
    }
    for ch in &edges[i] {
        let Some(ci) = comps.iter().position(|x| x.name == *ch) else { continue };
        if memo[ci].as_ref().is_some_and(|v| v.iter().any(|x| x.0 == name)) {
            path.extend(inject_path(ci, name, comps, edges, memo));
            break;
        }
    }
    path
}

/// `provide`/`inject`: each component gets a hidden prop for what its descendants inject and it doesn't
/// provide, and its parent passes it. A root with something left over has no provider.
pub(super) fn resolve_inject(comps: &mut [Comp], edges: &[Vec<String>], files: &[SrcFile], used_by_other: &HashSet<String>) -> Result<(), GenError> {
    let mut memo = vec![None; comps.len()];
    for i in 0..comps.len() {
        needs(i, comps, edges, files, &mut memo)?;
    }
    for i in 0..comps.len() {
        let need = memo[i].clone().unwrap_or_default();
        if !used_by_other.contains(&comps[i].name) {
            if let Some((n, ty, r)) = need.first() {
                let path = inject_path(i, n, comps, edges, &memo);
                return Err(ge(comps[i].fi, UiError { lo: r.0, hi: r.1, msg: format!("nothing provides `{n}: {ty}`"), help: Some(format!("{} (injects `{n}`); add `provide {n};` to a component on this path that has `{n}` as a prop or `let`", path.join(" -> "))) }));
            }
        }
        for (name, ty, r) in need {
            comps[i].sc.props.push(markup::Prop { name, name_r: r, ty: (0, 0), hidden: Some(ty), default: None });
        }
    }
    Ok(())
}

pub(super) fn cycle(comps: &[Comp], edges: &[Vec<String>], at: usize, path: &mut Vec<String>) -> Option<Vec<String>> {
    for n in &edges[at] {
        if path.contains(n) {
            let mut p = path.clone();
            p.push(n.clone());
            return Some(p);
        }
        if let Some(j) = comps.iter().position(|c| &c.name == n) {
            path.push(n.clone());
            if let Some(p) = cycle(comps, edges, j, path) {
                return Some(p);
            }
            path.pop();
        }
    }
    None
}

pub(super) fn find_tag(nodes: &[Node], tag: &str) -> Option<Range> {
    for n in nodes {
        match n {
            Node::Elem(e) => {
                if e.tag == tag {
                    return Some(e.open);
                }
                if let Some(r) = find_tag(&e.children, tag) {
                    return Some(r);
                }
            }
            Node::If(i) => {
                for b in &i.branches {
                    if let Some(r) = find_tag(&b.nodes, tag) {
                        return Some(r);
                    }
                }
            }
            Node::Each(e) => {
                if let Some(r) = find_tag(&e.body, tag) {
                    return Some(r);
                }
            }
            Node::Text { .. } | Node::Let(_) => {}
        }
    }
    None
}
