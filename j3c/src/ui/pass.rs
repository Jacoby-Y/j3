//! `class`, `class:name` and `style:prop` written on a component tag (`<kit.Card class="wide" style:width={w}>`):
//! they go to the component's root element.
//!
//! The component is generated once and shared by every use, so the values travel as hidden `j3pass_*` fields of
//! its `Props`, sent like any prop. A component gets the fields only when some use passes something; every use of
//! it then fills them (unused slots: kind 0). The root element then applies them each frame with `d.scopePass`,
//! `d.clsPass` and `d.stylePass`, which compare to the last value first (no restyle when nothing changed).
//!
//! Scoping: the root belongs to the child's scope, so the parent's scoped rules would not match it. The use sends
//! the parent's scope id (`j3pass_scope`, only when it passes a class) and the root node carries it as a second
//! scope: rules of that sheet match it when their rightmost compound names a class (`cssmatch.computeStyle`).
//!
//! Styles: the passed `style:` attributes take the slots after the root's own (`style:` slot `own + k`), and later
//! slots win, so a passed value beats the root's own for the same property.

use std::collections::HashMap;

use super::markup::{err, Attr, Elem, Node, Part, UiError, R};

/// Slots 0..8 of `dynattr`; slot 8 is the virtual list's row hide.
pub const MAX_STYLE: usize = 8;

/// What the uses of one component pass.
#[derive(Clone, Copy, Default)]
pub struct Pass {
    /// Some use passes `class` / `class:name`.
    pub cls: bool,
    /// The most `style:` attributes any use passes.
    pub styles: usize,
    /// `style:` attributes on the root element itself (the passed ones start after them).
    pub own: usize,
}

pub fn is_pass(a: &Attr) -> bool {
    a.name == "class" || a.name.starts_with("class:") || a.name.starts_with("style:")
}

/// Hidden `Props` field names start with this; user names may not.
pub const PREFIX: &str = "j3pass";

fn blank(n: &Node) -> bool {
    matches!(n, Node::Let(_)) || matches!(n, Node::Text { parts, .. } if parts.iter().all(|p| matches!(p, Part::Lit(l) if l.trim().is_empty())))
}

/// The one element a component's markup is, or why it isn't.
pub fn single_root(nodes: &[Node]) -> Result<&Elem, String> {
    let real: Vec<&Node> = nodes.iter().filter(|n| !blank(n)).collect();
    match real.as_slice() {
        [] => Err("its markup is empty".into()),
        [Node::Elem(e)] if e.comp => Err(format!("its root is the component `<{}>`", e.tag)),
        [Node::Elem(e)] if e.tag == "slot" => Err("its root is a `<slot />`".into()),
        [Node::Elem(e)] => Ok(e),
        [Node::If(i)] => Err(if i.scrut.is_some() { "its root is a `{#match}`" } else { "its root is an `{#if}`" }.into()),
        [Node::Each(_)] => Err("its root is an `{#each}`".into()),
        [Node::Text { .. }] => Err("its root is text".into()),
        more => Err(format!("it has {} root nodes", more.len())),
    }
}

pub fn own_styles(root: &Elem) -> usize {
    root.attrs.iter().filter(|a| a.name.starts_with("style:")).count()
}

/// Walks the markup of one component and records, per child component, what its uses pass. Errors at the use.
pub fn scan(nodes: &[Node], roots: &HashMap<String, &[Node]>, out: &mut HashMap<String, Pass>) -> R<()> {
    for n in nodes {
        match n {
            Node::Text { .. } => {}
            Node::Elem(e) => {
                let passed: Vec<&Attr> = e.attrs.iter().filter(|a| is_pass(a)).collect();
                if e.comp && !passed.is_empty() {
                    if let Some(child) = roots.get(&e.tag) {
                        let root = single_root(child).map_err(|why| UiError {
                            lo: passed[0].range.0,
                            hi: passed[0].range.1,
                            msg: format!("`{}` on `<{}>` has no element to go to: {why}", passed[0].name, e.tag),
                            help: Some(format!("`class` and `style:` land on the root element of `{}`, so its markup must be exactly one element (not a component, `<slot />`, `{{#if}}`, `{{#each}}` or text); pass a prop instead", e.tag)),
                        })?;
                        let styles = passed.iter().filter(|a| a.name.starts_with("style:")).count();
                        let own = own_styles(root);
                        if own + styles > MAX_STYLE {
                            let a = passed.iter().find(|a| a.name.starts_with("style:")).unwrap();
                            return err(a.range.0, a.range.1, format!("`<{}>` passes {styles} `style:` and its root has {own} of its own: at most {MAX_STYLE} together", e.tag));
                        }
                        let p = out.entry(e.tag.clone()).or_default();
                        p.cls |= passed.iter().any(|a| !a.name.starts_with("style:"));
                        p.styles = p.styles.max(styles);
                        p.own = own;
                    }
                }
                scan(&e.children, roots, out)?;
            }
            Node::If(i) => i.branches.iter().try_for_each(|b| scan(&b.nodes, roots, out))?,
            Node::Each(e) => scan(&e.body, roots, out)?,
            Node::Let(_) => {}
        }
    }
    Ok(())
}

/// The `Props` fields of a component that takes passthrough.
pub fn fields(p: &Pass) -> String {
    let mut s = format!("    {PREFIX}_scope: int;\n    {PREFIX}_cls: string;\n");
    for k in 0..p.styles {
        s.push_str(&format!("    {PREFIX}_k{k}: int;\n    {PREFIX}_n{k}: string;\n    {PREFIX}_f{k}: f32;\n    {PREFIX}_s{k}: string;\n"));
    }
    s
}

/// What the root element does with them, after its own attributes (`n` is its node).
pub fn apply(p: &Pass, n: &str) -> Vec<String> {
    let mut v = vec![format!("d.scopePass({n}, p.{PREFIX}_scope);")];
    if p.cls {
        v.push(format!("d.clsPass({n}, p.{PREFIX}_cls);"));
    }
    for k in 0..p.styles {
        v.push(format!("d.stylePass({n}, {}, p.{PREFIX}_k{k}, p.{PREFIX}_n{k}, p.{PREFIX}_f{k}, p.{PREFIX}_s{k});", p.own + k));
    }
    v
}

/// Slot `k`'s fields for a use that passes no such style.
pub fn unused(k: usize) -> String {
    format!("{PREFIX}_k{k}: 0, {PREFIX}_n{k}: \"\", {PREFIX}_f{k}: 0.0, {PREFIX}_s{k}: \"\"")
}
