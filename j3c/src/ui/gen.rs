//! `.j3ui` components to J3 source text, in the shape `speculative/12_ui_runtime/README.md`
//! describes. All components of a project become one module, `<package>.ui`.
//!
//! J3 from the file (script bodies, `{expr}`, handlers) is copied verbatim; the only edits are
//! token-level (`this.` before an instance `let` or script `fn`, `emit x` to `ev.x = true`),
//! found with the real lexer. The text then goes through `parse::parse` like any file.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::map::{Out, Seg};
use super::pass::{self, Pass};
use super::markup::{self, err, trim_range, Attr, EachNode, Elem, IfNode, LetNode, Markup, Node, Part, Range, Script, UiError, Value, R};
use crate::parse::lex::lex;
use crate::parse::TokenKind;
use crate::span::FileId;

pub struct SrcFile {
    /// As shown in errors, e.g. `ui/Game.j3ui`.
    pub path: String,
    /// The component name (file stem).
    pub name: String,
    pub text: String,
}

pub struct Theme {
    pub path: String,
    pub text: String,
}

pub struct Built {
    pub text: String,
    pub segs: Vec<Seg>,
    /// Component name and its byte range in `text`.
    pub sections: Vec<(String, usize, usize)>,
}

#[derive(Debug)]
pub struct GenError {
    pub file: usize,
    pub err: UiError,
}

/// The `<glyph action>` names the runtime knows (`dom/glyph.j3`).
pub const GLYPH_ACTIONS: &[&str] = &["confirm", "back", "prev", "next", "up", "down", "left", "right", "page-up", "page-down", "scroll"];

pub const ELEMENTS: &[&str] = &[
    "div", "span", "p", "h1", "h2", "small", "header", "nav", "main", "section", "label", "button", "input", "textarea", "select", "option", "dl", "dt", "dd", "canvas", "dialog", "aside", "img", "svg", "path", "h3", "h4", "h5", "h6", "ul", "ol", "li", "a", "hr", "strong", "em", "b", "i", "pre", "code", "br", "progress", "meter", "details", "summary", "glyph",
];
pub const MOUSE: &[&str] = &["click", "mousedown", "mouseup", "mousemove", "wheel", "input", "change", "submit", "close", "contextmenu", "trigger", "edit", "dragstart", "dragend", "dragenter", "dragleave", "dragover", "drop", "toggle"];

/// The literal `type` of an `<input>` ("text" when it has none), `None` when it is an expression.
fn input_type(e: &Elem) -> Option<String> {
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
fn media_code(name: &str) -> Option<(bool, i32)> {
    Some(match name {
        "src" => (false, 0),
        "sprite" => (false, 1),
        "viewBox" => (false, 2),
        "d" => (false, 3),
        "fill" => (false, 4),
        "stroke" => (false, 5),
        "frame" => (true, 0),
        "fps" => (true, 1),
        "stroke-width" => (true, 2),
        _ => return None,
    })
}

/// J3 names the generated module already declares or imports.
const RESERVED: [&str; 7] = ["Arena", "List", "Css", "Dom", "Vec2", "Keyed", "THEME"];

fn is_range(e: &Elem) -> bool {
    e.tag == "input" && input_type(e).as_deref() == Some("range")
}

fn is_radio(e: &Elem) -> bool {
    e.tag == "input" && input_type(e).as_deref() == Some("radio")
}

struct Comp {
    /// Dotted path from `ui/` (`game.Bar`): what markup writes.
    name: String,
    /// The J3 name (`game_Bar`).
    id: String,
    fi: usize,
    mk: Markup,
    sc: Script,
    scope: usize,
}

#[derive(Default)]
struct Names {
    props: Vec<String>,
    lets: Vec<String>,
    /// (script name, module-level J3 name) of each `const`.
    consts: Vec<(String, String)>,
    fns: Vec<String>,
    /// Script fns that `emit`, directly or by calling one that does.
    emitting: Vec<String>,
    events: Vec<String>,
    /// (event, payload name) in declaration order.
    payload: Vec<(String, String)>,
    /// `$slots.x`: each `<slot>` this component declares ("" is the default) and the use sites that fill it.
    slot_has: Vec<(String, Vec<usize>)>,
}

/// The generated `bool` method behind `$slots.<name>` (the default slot is `default`).
fn slot_has_fn(name: &str) -> String {
    format!("slotHas_{}", if name.is_empty() { "default" } else { name })
}

fn is_reserved(n: &str) -> bool {
    const FIXED: &[&str] = &["d", "p", "ev", "this", "mem", "ready", "css", "dom", "init", "update", "paint", "new", "styles", "render", "ps", "cs", "inputs", "j3Reset", "slotSite", "slotOwn", "slotI", "sev", "destroy", "mounting", "event", "dm", "comp"];
    if FIXED.contains(&n) || n.starts_with(pass::PREFIX) {
        return true;
    }
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    for pre in ["n", "s", "e", "b", "m", "i", "a", "x", "c", "if", "cnt", "slotNow", "slotWas", "pass", "size", "ua", "us", "up", "ul", "uh", "pv", "bh"] {
        if let Some(rest) = n.strip_prefix(pre) {
            if digits(rest) {
                return true;
            }
        }
    }
    n.starts_with("tag_") || n.starts_with("cls_") || n.starts_with("slotHas_")
}

/// Payload types an event can carry: each has a literal to fill the field on frames without an emit.
const EV_TYPES: &[&str] = &["int", "bool", "string", "float", "f32", "f64", "i32", "u8"];

fn ev_default(ty: &str) -> &'static str {
    match ty {
        "bool" => "false",
        "string" => "\"\"",
        "int" | "i32" | "u8" => "0",
        _ => "0.0",
    }
}

/// `pick: false, pick_slot: 0, close: false`: every field of the component's `Events` struct.
fn ev_init(sc: &Script, src: &str) -> String {
    let mut v = Vec::new();
    for (e, _) in &sc.events {
        v.push(format!("{e}: false"));
        for p in sc.ev_params.iter().filter(|p| p.ev == *e) {
            v.push(format!("{e}_{}: {}", p.name, ev_default(src[p.ty.0..p.ty.1].trim())));
        }
    }
    v.join(", ")
}

fn san(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

fn lit(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => o.push_str("\\\\"),
            '"' => o.push_str("\\\""),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

fn ge(file: usize, e: UiError) -> GenError {
    GenError { file, err: e }
}

fn width(nodes: &[Node]) -> usize {
    nodes
        .iter()
        .map(|n| match n {
            Node::Text { .. } | Node::Let(_) => 0,
            Node::Elem(_) | Node::Each(_) => 1,
            Node::If(i) => i.branches.iter().map(|b| width(&b.nodes)).sum(),
        })
        .sum()
}

fn holds(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Text { .. } | Node::Let(_) => false,
        Node::Elem(e) => e.comp || e.attrs.iter().any(|a| a.name.starts_with("use:") || a.name == "bind:this") || holds(&e.children),
        Node::If(i) => i.branches.iter().any(|b| holds(&b.nodes)),
        Node::Each(e) => holds(&e.body),
    })
}

/// Numbers child instances, stateful `{#if}`s and `{#each}`s in source order.
fn number(nodes: &[Node], cn: &mut usize, ifn: &mut usize, en: &mut usize) {
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

fn children_of(nodes: &[Node], out: &mut Vec<String>) {
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

/// A use of a component with children: which slots it fills.
struct Site {
    /// 1-based; names the generated `slot<id>_<name>_update` methods on the using component.
    id: usize,
    child: String,
    /// The using component's J3 name.
    owner: String,
    names: Vec<String>,
}

#[derive(Default)]
struct Slots {
    list: Vec<Site>,
    /// Component elements with children, by address, to their `list` index.
    by_addr: HashMap<usize, usize>,
    /// Direct children carrying a valid `slot="x"`.
    slotted: HashSet<usize>,
    /// What the uses of each component pass as `class` / `style:` (see `pass`), by component name.
    pass: HashMap<String, Pass>,
}

fn addr(e: &Elem) -> usize {
    e as *const Elem as usize
}

/// The elements a `{#each}` body makes one row of (its own, and those of its `{#if}` branches).
fn row_elems(nodes: &[Node], out: &mut Vec<usize>) {
    for n in nodes {
        match n {
            Node::Elem(e) => out.push(addr(e)),
            Node::If(i) => i.branches.iter().for_each(|b| row_elems(&b.nodes, out)),
            _ => {}
        }
    }
}

fn lit_value(v: &Value) -> Option<String> {
    match v {
        Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => Some(p.iter().map(|x| if let Part::Lit(l) = x { l.as_str() } else { "" }).collect::<String>().trim().to_string()),
        _ => None,
    }
}

/// The slot a child of a component is passed to (`slot="x"`; "" is the default slot).
fn slot_of(n: &Node) -> String {
    slot_elem(n).and_then(|e| e.attrs.iter().find(|a| a.name == "slot")).and_then(|a| lit_value(&a.value)).unwrap_or_default()
}

fn slot_elem(n: &Node) -> Option<&Elem> {
    match n {
        Node::Elem(e) => Some(e),
        Node::If(i) if i.choice.is_some() => i.branches.first().and_then(|b| b.nodes.first()).and_then(slot_elem),
        _ => None,
    }
}

fn mark_slotted(n: &Node, out: &mut HashSet<usize>) {
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
fn declared(nodes: &[Node], out: &mut Vec<String>) -> R<()> {
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
fn scan_sites(nodes: &[Node], all: &[Comp], decl: &[Vec<String>], owner: &str, s: &mut Slots) -> R<()> {
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
fn lower_choices(nodes: &mut [Node], src: &str, known: &[String], sets: &mut std::collections::BTreeMap<String, Vec<String>>) -> R<()> {
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

pub fn generate(files: &[SrcFile], package: &str, theme: Option<&Theme>) -> Result<Built, GenError> {
    let mut comps: Vec<Comp> = Vec::new();
    let mut order: Vec<usize> = (0..files.len()).collect();
    order.sort_by(|a, b| files[*a].name.cmp(&files[*b].name));
    for (scope, fi) in order.into_iter().enumerate() {
        let f = &files[fi];
        let segs: Vec<&str> = f.name.split('.').collect();
        let last = segs[segs.len() - 1];
        if !(last.starts_with(|c: char| c.is_ascii_uppercase()) && last.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')) {
            return Err(ge(fi, UiError { lo: 0, hi: 0, msg: format!("`{}` is not a component name", f.name), help: Some("component files are named like their component: Capitalised letters, digits, `_`, e.g. UpgradeRow.j3ui".into()) }));
        }
        if let Some(bad) = segs[..segs.len() - 1].iter().find(|g| !(g.starts_with(|c: char| c.is_ascii_alphabetic()) && g.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'))) {
            return Err(ge(fi, UiError { lo: 0, hi: 0, msg: format!("folder `{bad}` in `{}` is not a valid name segment", f.path), help: Some("folders under ui/ become part of the component name: start with a letter, then letters, digits, `_`".into()) }));
        }
        let id = f.name.replace('.', "_");
        for other in &comps {
            let same = other.id == id;
            if same || ["Props", "Events"].iter().any(|x| format!("{}{x}", other.id) == id || format!("{id}{x}") == other.id) {
                return Err(ge(fi, UiError { lo: 0, hi: 0, msg: format!("`{}` and `{}` both generate the J3 name `{}`", f.path, files[other.fi].path, if same { id.clone() } else { format!("{id}` / `{}", other.id) }), help: Some("rename one of the files or folders".into()) }));
            }
        }
        if RESERVED.contains(&id.as_str()) || id.starts_with("SCOPE_") || id.starts_with("CSS_") || id.starts_with("Of_") {
            return Err(ge(fi, UiError { lo: 0, hi: 0, msg: format!("`{}` generates the J3 name `{id}`, which the generated module already uses", f.path), help: Some("rename the component".into()) }));
        }
        let mk = markup::parse(&f.text).map_err(|e| ge(fi, e))?;
        let sc = match mk.script {
            Some(r) => markup::parse_script(&f.text, r).map_err(|e| ge(fi, e))?,
            None => Script::default(),
        };
        comps.push(Comp { name: f.name.clone(), id, fi, mk, sc, scope: scope + 1 });
    }
    // who uses whom
    let known = comps.iter().map(|c| c.name.clone()).collect::<Vec<_>>();
    let mut choices = std::collections::BTreeMap::new();
    for c in &mut comps {
        lower_choices(&mut c.mk.nodes, &files[c.fi].text, &known, &mut choices).map_err(|e| ge(c.fi, e))?;
    }
    let mut used_by_other: HashSet<String> = HashSet::new();
    let mut edges: Vec<Vec<String>> = Vec::new();
    for c in &comps {
        let mut ch = Vec::new();
        children_of(&c.mk.nodes, &mut ch);
        for n in &ch {
            used_by_other.insert(n.clone());
        }
        edges.push(ch);
    }
    for (i, c) in comps.iter().enumerate() {
        if let Some(path) = cycle(&comps, &edges, i, &mut vec![c.name.clone()]) {
            let open = find_tag(&c.mk.nodes, &path[1]).unwrap_or((0, 0));
            return Err(ge(c.fi, UiError { lo: open.0, hi: open.1, msg: format!("component `{}` contains itself", c.name), help: Some(path.join(" -> ")) }));
        }
    }
    let _ = known;
    resolve_inject(&mut comps, &edges, files, &used_by_other)?;
    let mut slots = Slots::default();
    let mut decl: Vec<Vec<String>> = Vec::new();
    for c in &comps {
        let mut d = Vec::new();
        declared(&c.mk.nodes, &mut d).map_err(|e| ge(c.fi, e))?;
        decl.push(d);
    }
    for c in &comps {
        scan_sites(&c.mk.nodes, &comps, &decl, &c.id, &mut slots).map_err(|e| ge(c.fi, e))?;
    }
    let roots: HashMap<String, &[Node]> = comps.iter().map(|c| (c.name.clone(), c.mk.nodes.as_slice())).collect();
    for c in &comps {
        pass::scan(&c.mk.nodes, &roots, &mut slots.pass).map_err(|e| ge(c.fi, e))?;
    }
    let mut body = Out::with_ctx(0);
    let mut sections = Vec::new();
    let mut uses_list = false;
    let mut uses_keyed = false;
    let mut uses_virt = false;
    let mut any_root = false;
    for (i, c) in comps.iter().enumerate() {
        let _ = i;
        let root = !used_by_other.contains(&c.name);
        any_root |= root;
        let mut g = G::new(&comps, files, theme.map(|t| t.path.as_str()), c, root, &slots);
        let o = g.run().map_err(|e| ge(c.fi, e))?;
        uses_list |= g.uses_list;
        uses_keyed |= g.uses_keyed;
        uses_virt |= g.uses_virt;
        let lo = body.text.len();
        body.append(o);
        sections.push((c.name.clone(), lo, body.text.len()));
    }
    // module header
    let mut head = Out::with_ctx(0);
    head.s(&format!("// Generated by j3c from {} .j3ui file(s). Edit the .j3ui, not this.\n", comps.len()));
    head.s(&format!("module {package}.ui;\n\n"));
    head.s(if uses_list { "use j3.memory.{ Arena, List };\n" } else { "use j3.memory.Arena;\n" });
    if any_root {
        head.s("use j3.math.Vec2;\n");
    }
    if uses_keyed {
        head.s("use j3.ui.keyed.Keyed;\n");
    }
    if uses_virt {
        head.s("use j3.ui.vlist;\nuse j3.ui.vstate.VList;\n");
    }
    head.s("use j3.ui.css.Css;\nuse j3.ui.cssparse.{ parse, parseAt };\nuse j3.ui.uastyle;\nuse j3.ui.dom.Dom;\nuse j3.ui.domhot.{ hotDue, hotFile, hotWarns };\nuse j3.ui.inputs;\n");
    // every component's `use`, split into one `use path.Name;` per name, each once
    let mut seen: HashSet<String> = HashSet::new();
    for c in &comps {
        let src = &files[c.fi].text;
        for &(lo, hi) in &c.sc.uses {
            for full in use_names(&src[lo..hi]) {
                if seen.insert(full.clone()) {
                    head.set(c.fi, lo, hi);
                    head.s(&format!("use {full};\n"));
                }
            }
        }
    }
    head.set(0, 0, 0);
    head.s("\n");
    if any_root {
        match theme {
            Some(t) => head.s(&format!("const THEME: string = {};\n\n", lit(&t.text))),
            None => head.s("const THEME: string = \"\";\n\n"),
        }
    }
    for (set, candidates) in &choices {
        let variants: Vec<String> = candidates.iter().map(|n| n.replace('.', "_")).collect();
        head.s(&format!("pub enum {set} {{ {} }}\n", variants.join(", ")));
    }
    let shift = head.text.len();
    head.append(body);
    for s in sections.iter_mut() {
        s.1 += shift;
        s.2 += shift;
    }
    Ok(Built { text: head.text, segs: head.segs, sections })
}

type Need = (String, String, Range);

/// What component `i` must be given: its own `inject`s and those of everything it uses that it doesn't `provide`.
fn needs(i: usize, comps: &[Comp], edges: &[Vec<String>], files: &[SrcFile], memo: &mut Vec<Option<Vec<Need>>>) -> Result<Vec<Need>, GenError> {
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
fn inject_path(i: usize, name: &str, comps: &[Comp], edges: &[Vec<String>], memo: &[Option<Vec<Need>>]) -> Vec<String> {
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
fn resolve_inject(comps: &mut [Comp], edges: &[Vec<String>], files: &[SrcFile], used_by_other: &HashSet<String>) -> Result<(), GenError> {
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

fn cycle(comps: &[Comp], edges: &[Vec<String>], at: usize, path: &mut Vec<String>) -> Option<Vec<String>> {
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

fn find_tag(nodes: &[Node], tag: &str) -> Option<Range> {
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

// ---------------------------------------------------------------------------------------
// token-level rewrite of copied J3

struct Rw<'a> {
    src: &'a str,
    fi: usize,
    names: &'a Names,
    locals: &'a [String],
    /// Names to write differently (a keyed `{#each}` binds its item twice, and J3 has no shadowing).
    rename: &'a [(String, String)],
    emit_ok: bool,
    /// Inside a script `fn`: `emit` writes the component's `sev` (merged into the update's events), not the local `ev`.
    sev: bool,
    /// Calling a script fn that emits is fine here: handlers and fns, not a markup expression (it would emit on every update), `on:draw` or `onDestroy`.
    calls_emit: bool,
}

impl<'a> Rw<'a> {
    fn declared(&self, n: &str) -> bool {
        self.names.props.iter().chain(&self.names.lets).chain(&self.names.fns).chain(self.locals).any(|x| x == n) || self.names.consts.iter().any(|x| x.0 == n) || self.rename.iter().any(|x| x.0 == n)
    }
}

fn rewrite(rw: &Rw, out: &mut Out, used: &mut BTreeSet<String>, lo: usize, hi: usize, flat: bool) -> R<()> {
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

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    U,
    P,
}

type Base = Option<(String, usize)>;

struct G<'a> {
    all: &'a [Comp],
    files: &'a [SrcFile],
    theme_path: Option<&'a str>,
    c: &'a Comp,
    src: &'a str,
    fi: usize,
    root: bool,
    names: Names,
    locals: Vec<String>,
    rename: Vec<(String, String)>,
    /// `{#let}`s in scope: (markup name, J3 name, expression).
    lets: Vec<(String, String, Range)>,
    upd: Out,
    pnt: Out,
    used_u: BTreeSet<String>,
    used_p: BTreeSet<String>,
    k: usize,
    tags: Vec<String>,
    classes: Vec<String>,
    /// (instance id, component name, container): the container is 0 for the component itself, else
    /// the `{#each}` whose row struct holds it
    insts: Vec<(usize, String, usize)>,
    /// (state id, container)
    ifs: Vec<(usize, usize)>,
    eachs: Vec<EachDef>,
    acts: Vec<ActDef>,
    binds: Vec<BindDef>,
    /// `(locals, lets)` in scope where each container's body starts: a `bind:this` place can be reset later only from there.
    bases: Vec<(usize, usize)>,
    /// The enclosing `{#each}`s that have a row struct: `r<id>` is the row being built.
    own: Vec<usize>,
    /// `r<id>.uid` of the enclosing keyed rows.
    gstk: Vec<String>,
    idx: Vec<String>,
    slots: &'a Slots,
    /// The enclosing `{#each}`s (head, number): a slot method re-binds the loop item from its index.
    ectx: Vec<(EachHead, usize, usize, bool)>,
    /// The `slot<id>_<name>_*` methods of this component's component uses.
    slot_fns: Out,
    /// Has a `<slot>`: `slotSite`, `slotOwn` and `slotI` say whose content to run.
    has_slot: bool,
    /// The `<slot>`s someone fills: (number, method name part, `(site, owner)` of each use). `update` ends by
    /// telling the owner when one that ran last frame did not run this frame.
    slot_marks: Vec<(usize, String, Vec<(usize, String)>)>,
    /// Has slot methods: `emit` in them lands in `sev`, merged into the update's events.
    owns_slot: bool,
    /// A `virtual` element's settings for the one `{#each}` it holds.
    virt: Option<Virt>,
    /// Inside a `virtual` `{#each}` (slot content can't be re-bound there).
    in_virt: usize,
    /// Addresses of the elements that may carry `animate:flip`: the row elements of keyed, non-`virtual` `{#each}`s.
    flip_rows: Vec<usize>,
    pub uses_list: bool,
    pub uses_keyed: bool,
    pub uses_virt: bool,
    /// Address of this component's root element when some use passes `class` / `style:` to it.
    pass_root: usize,
}

/// What a `virtual` element says about its list (`virtual_attrs`): the node variable, `row-height` (none:
/// rows are measured), `est-height`, `stick-to-bottom`, `keep={keys}` and `bind:at-end={place}`.
struct Virt {
    n: String,
    rh: Option<Value>,
    est: Option<Value>,
    stick: bool,
    keep: Option<Range>,
    at_end: Option<Range>,
}

/// An `{#each}`. With `rows` its state (component instances, `{#if}` branches, nested lists) lives in
/// one `<Comp>_R<id>` struct per row, so it can follow a key; without, the loop keeps none.
/// A `use:name` on an element: the state it keeps in its container (`ua<k>` handle, `us<k>` mounted, `up<k>` last parameter).
struct ActDef {
    k: usize,
    container: usize,
    name: String,
    /// Type of the parameter (None: `use:name` without one).
    ty: Option<String>,
    destroy: bool,
}

/// A `bind:this={place}` whose place is still reachable after the build (a component field, or an element of one): reset to -1 when the element goes.
struct BindDef {
    k: usize,
    container: usize,
    place: Range,
}

struct EachDef {
    id: usize,
    container: usize,
    keyed: bool,
    rows: bool,
    virt: bool,
}

fn field_name(prefix: &str, list: &[String], orig: &str) -> String {
    let pos = list.iter().position(|x| x == orig).unwrap_or(0);
    let base = san(orig);
    if list[..pos].iter().any(|x| san(x) == base) {
        format!("{prefix}{base}_{pos}")
    } else {
        format!("{prefix}{base}")
    }
}

impl<'a> G<'a> {
    fn new(all: &'a [Comp], files: &'a [SrcFile], theme_path: Option<&'a str>, c: &'a Comp, root: bool, slots: &'a Slots) -> G<'a> {
        G {
            all,
            files,
            theme_path,
            c,
            src: &files[c.fi].text,
            fi: c.fi,
            root,
            names: Names::default(),
            locals: Vec::new(),
            rename: Vec::new(),
            lets: Vec::new(),
            upd: Out::with_ctx(c.fi),
            pnt: Out::with_ctx(c.fi),
            used_u: BTreeSet::new(),
            used_p: BTreeSet::new(),
            k: 0,
            tags: Vec::new(),
            classes: Vec::new(),
            insts: Vec::new(),
            ifs: Vec::new(),
            eachs: Vec::new(),
            acts: Vec::new(),
            binds: Vec::new(),
            bases: Vec::new(),
            own: Vec::new(),
            gstk: Vec::new(),
            idx: Vec::new(),
            slots,
            ectx: Vec::new(),
            slot_fns: Out::with_ctx(c.fi),
            has_slot: false,
            slot_marks: Vec::new(),
            owns_slot: false,
            virt: None,
            in_virt: 0,
            flip_rows: Vec::new(),
            uses_list: false,
            uses_keyed: false,
            uses_virt: false,
            pass_root: 0,
        }
    }

    fn nk(&mut self) -> usize {
        self.k += 1;
        self.k
    }

    fn out(&mut self, m: Mode) -> &mut Out {
        match m {
            Mode::U => &mut self.upd,
            Mode::P => &mut self.pnt,
        }
    }

    fn rw(&mut self, m: Mode, lo: usize, hi: usize, flat: bool, emit_ok: bool, calls: bool) -> R<()> {
        let rw = Rw { src: self.src, fi: self.fi, names: &self.names, locals: &self.locals, rename: &self.rename, emit_ok, sev: false, calls_emit: calls };
        let (out, used) = match m {
            Mode::U => (&mut self.upd, &mut self.used_u),
            Mode::P => (&mut self.pnt, &mut self.used_p),
        };
        rewrite(&rw, out, used, lo, hi, flat)
    }

    /// An expression, copied in place (no newline, no semicolon).
    fn expr(&mut self, m: Mode, r: Range) -> R<()> {
        self.rw(m, r.0, r.1, false, true, false)
    }

    /// Handler statements on their own lines.
    fn stmts(&mut self, m: Mode, r: Range, emit_ok: bool) -> R<()> {
        let (lo, hi) = trim_range(self.src, r.0, r.1);
        if lo == hi {
            return Ok(());
        }
        self.out(m).pad();
        self.rw(m, lo, hi, false, emit_ok, emit_ok)?;
        let last = self.src[lo..hi].chars().last().unwrap_or(';');
        if last != ';' && last != '}' {
            self.out(m).s(";");
        }
        self.out(m).s("\n");
        Ok(())
    }

    fn tag(&mut self, t: &str) -> String {
        if !self.tags.iter().any(|x| x == t) {
            self.tags.push(t.to_string());
        }
        format!("this.{}", field_name("tag_", &self.tags, t))
    }

    fn cls(&mut self, t: &str) -> String {
        if !self.classes.iter().any(|x| x == t) {
            self.classes.push(t.to_string());
        }
        format!("this.{}", field_name("cls_", &self.classes, t))
    }

    fn slot(base: &Base, j: usize) -> String {
        match base {
            Some((iv, k)) => format!("{iv} * {k} + {j}"),
            None => j.to_string(),
        }
    }

    /// A string expression from attribute or text parts: a literal, or a template.
    fn string(&mut self, m: Mode, parts: &[Part], at: Range) -> R<()> {
        if parts.iter().all(|p| matches!(p, Part::Lit(_))) {
            let mut s = String::new();
            for p in parts {
                if let Part::Lit(l) = p {
                    s.push_str(l);
                }
            }
            self.out(m).s(&lit(&s));
            return Ok(());
        }
        self.out(m).s("`");
        for p in parts {
            match p {
                Part::Lit(l) => {
                    if l.contains('`') || l.contains('\n') {
                        return err(at.0, at.1, "text next to `{...}` can't hold a backtick");
                    }
                    self.out(m).s(l);
                }
                Part::Hole(r) => {
                    self.out(m).s("{");
                    self.rw(m, r.0, r.1, true, true, false)?;
                    self.out(m).s("}");
                }
            }
        }
        self.out(m).s("`");
        Ok(())
    }

    fn check_local(&self, name: &str, r: Range) -> R<()> {
        let rw = Rw { src: self.src, fi: self.fi, names: &self.names, locals: &self.locals, rename: &self.rename, emit_ok: false, sev: false, calls_emit: false };
        if rw.declared(name) || is_reserved(name) {
            return err(r.0, r.1, format!("`{name}` shadows a prop, `let`, `fn`, or name used by the generated code")).map_err(|e| e.help("pick another name"));
        }
        Ok(())
    }

    fn run(&mut self) -> R<Out> {
        let c = self.c;
        let sc = &c.sc;
        // names
        let mut seen: Vec<String> = Vec::new();
        let mut claim = |n: &str, r: Range, what: &str| -> R<()> {
            if is_reserved(n) {
                return err(r.0, r.1, format!("`{n}` is reserved by the generated code ({what})"));
            }
            if seen.iter().any(|x| x == n) {
                return err(r.0, r.1, format!("`{n}` is declared twice in this component"));
            }
            seen.push(n.to_string());
            Ok(())
        };
        for p in &sc.props {
            claim(&p.name, p.name_r, "prop")?;
            self.names.props.push(p.name.clone());
        }
        for l in &sc.lets {
            claim(&l.name, l.name_r, "let")?;
            self.names.lets.push(l.name.clone());
        }
        for l in &sc.consts {
            claim(&l.name, l.name_r, "const")?;
            self.names.consts.push((l.name.clone(), format!("{}_{}", c.id, l.name)));
        }
        for f in &sc.fns {
            claim(&f.name, f.name_r, "fn")?;
            self.names.fns.push(f.name.clone());
            let ptxt = &self.src[f.params.0..f.params.1];
            let ok = match f.name.as_str() {
                "onMount" | "onDestroy" => f.params.0 == f.params.1 && f.ret.is_none(),
                "onFrame" => f.param_names.len() == 1 && !ptxt.contains(',') && ptxt.split(':').nth(1).map(str::trim) == Some("f32") && f.ret.is_none(),
                _ => true,
            };
            if !ok {
                let sig = if f.name == "onFrame" { "fn onFrame(dt: f32)" } else { "no arguments and no return type" };
                return err(f.name_r.0, f.name_r.1, format!("`{}` has a fixed signature: {sig}", f.name));
            }
        }
        // fns that emit, directly or through another fn (token scan; the rewrite validates each `emit`)
        let bodies: Vec<Vec<_>> = sc.fns.iter().map(|f| lex(&self.src[f.body.0..f.body.1], FileId(0)).unwrap_or_default()).collect();
        loop {
            let mut grew = false;
            for (f, toks) in sc.fns.iter().zip(&bodies) {
                let hit = (0..toks.len()).any(|i| {
                    let t = &toks[i];
                    let stmt = i == 0 || matches!(toks[i - 1].kind, TokenKind::Semi | TokenKind::LBrace | TokenKind::RBrace);
                    let called = t.kind == TokenKind::Ident && self.names.emitting.contains(&t.text) && toks.get(i + 1).is_some_and(|n| n.kind == TokenKind::LParen) && (i == 0 || !matches!(toks[i - 1].kind, TokenKind::Dot | TokenKind::ColonColon));
                    called || (t.text == "emit" && stmt && toks.get(i + 1).is_some_and(|n| n.kind == TokenKind::Ident))
                });
                if hit && !self.names.emitting.contains(&f.name) {
                    self.names.emitting.push(f.name.clone());
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }
        // `$slots.x`: the slots this component declares, and which use sites fill each
        let mut decl = Vec::new();
        declared(&c.mk.nodes, &mut decl)?;
        for d in decl {
            let sites = self.slots.list.iter().filter(|s| s.child == c.name && s.names.contains(&d)).map(|s| s.id).collect();
            self.names.slot_has.push((d, sites));
        }
        // a prop default is a constant: literals and the script's `const`s, not props, lets or fns
        for p in sc.props.iter().filter_map(|p| p.default.map(|r| (p, r))) {
            let (p, r) = p;
            let toks = lex(&self.src[r.0..r.1], FileId(0)).map_err(|e| UiError { lo: r.0 + e.span.start as usize, hi: r.0 + (e.span.end as usize).max(e.span.start as usize + 1), msg: e.message, help: None })?;
            for (i, t) in toks.iter().enumerate() {
                let member = i > 0 && matches!(toks[i - 1].kind, TokenKind::Dot | TokenKind::ColonColon);
                if t.kind == TokenKind::Ident && !member && (self.names.props.contains(&t.text) || self.names.lets.contains(&t.text) || self.names.fns.contains(&t.text)) {
                    return Err(UiError { lo: r.0 + t.span.start as usize, hi: r.0 + t.span.end as usize, msg: format!("the default of prop `{}` can't use `{}`", p.name, t.text), help: Some("a default is a constant: a literal or a `const` of this component".into()) });
                }
            }
        }
        let mut ev_seen: Vec<&str> = Vec::new();
        for (e, r) in &sc.events {
            if ev_seen.contains(&e.as_str()) {
                return err(r.0, r.1, format!("event `{e}` is declared twice"));
            }
            ev_seen.push(e);
            self.names.events.push(e.clone());
        }
        let mut fields: Vec<String> = sc.events.iter().map(|e| e.0.clone()).collect();
        for p in &sc.ev_params {
            let ty = self.src[p.ty.0..p.ty.1].trim();
            if !EV_TYPES.contains(&ty) {
                return err(p.ty.0, p.ty.1, format!("event payload type `{ty}` isn't supported")).map_err(|e| e.help(format!("payload types: {}", EV_TYPES.join(", "))));
            }
            let f = format!("{}_{}", p.ev, p.name);
            if fields.contains(&f) {
                return err(p.name_r.0, p.name_r.1, format!("payload `{}` of `{}` collides with another event field `{f}`", p.name, p.ev));
            }
            fields.push(f);
            self.names.payload.push((p.ev.clone(), p.name.clone()));
        }
        let (mut cn, mut ifn, mut en) = (0, 0, 0);
        number(&c.mk.nodes, &mut cn, &mut ifn, &mut en);

        // script fns first: they may be used from markup but are independent of it
        let mut fns = Out::with_ctx(self.fi);
        for f in &sc.fns {
            self.script_fn(f, &mut fns)?;
        }

        // markup
        self.upd.ind = 2;
        self.pnt.ind = 2;
        if self.slots.pass.contains_key(&c.name) {
            self.pass_root = pass::single_root(&c.mk.nodes).map(addr).unwrap_or(0);
        }
        let mut j = 0;
        self.seq(&c.mk.nodes, &mut j, 0, &None)?;
        self.act_sweep(0, "this")?;
        self.assemble(fns)
    }

    fn script_fn(&mut self, f: &markup::FnDef, o: &mut Out) -> R<()> {
        for (n, r) in &f.param_names {
            self.check_local(n, *r)?;
        }
        let saved = self.locals.len();
        self.locals.extend(f.param_names.iter().map(|x| x.0.clone()));
        let mut body = Out::with_ctx(self.fi);
        body.set(self.fi, f.name_r.0, f.name_r.1);
        let mut used = BTreeSet::new();
        let rw = Rw { src: self.src, fi: self.fi, names: &self.names, locals: &self.locals, rename: &self.rename, emit_ok: f.name != "onDestroy", sev: true, calls_emit: f.name != "onDestroy" };
        rewrite(&rw, &mut body, &mut used, f.body.0, f.body.1, false)?;
        self.locals.truncate(saved);
        let name = &self.c.id;
        o.set(self.fi, f.name_r.0, f.name_r.1);
        o.s(&format!("\n    fn {}(this: &{}", f.name, name));
        if f.params.0 < f.params.1 {
            o.s(", ");
            o.copy(self.fi, self.src, f.params.0, f.params.1);
        }
        o.s(")");
        if let Some(r) = f.ret {
            o.copy(self.fi, self.src, r.0, r.1);
        }
        o.s(" {\n");
        // `d` in a script fn: the Dom the component was last built with
        let toks = lex(&self.src[f.body.0..f.body.1], FileId(0)).unwrap_or_default();
        if toks.iter().enumerate().any(|(i, t)| t.kind == TokenKind::Ident && t.text == "d" && (i == 0 || !matches!(toks[i - 1].kind, TokenKind::Dot | TokenKind::ColonColon))) {
            o.s("        let d = (this.dm as ptr) as &Dom;\n");
        }
        for p in &used {
            o.s(&format!("        let {p} = this.p.{p};\n"));
        }
        o.append(body);
        o.s("\n    }\n");
        Ok(())
    }

    // ---- the walk

    fn seq<'n>(&mut self, nodes: impl IntoIterator<Item = &'n Node>, j: &mut usize, depth: usize, base: &Base) -> R<()> {
        // `{#let}`s end with the block that holds them
        let (nl, nr) = (self.lets.len(), self.rename.len());
        let r = self.seq_in(nodes, j, depth, base);
        self.lets.truncate(nl);
        self.rename.truncate(nr);
        r
    }

    /// `{#let name = expr}`: a J3 local (own name, so sibling blocks never collide) for the rest of the block.
    fn let_node(&mut self, l: &LetNode) -> R<()> {
        if self.lets.iter().any(|x| x.0 == l.name) {
            return err(l.name_r.0, l.name_r.1, format!("`{}` is already declared by an earlier `{{#let}}` in this scope", l.name));
        }
        self.check_local(&l.name, l.name_r)?;
        let j3 = format!("lt{}", self.nk());
        self.upd.set(self.fi, l.range.0, l.range.1);
        self.upd.pad();
        self.upd.s(&format!("let {j3} = "));
        self.expr(Mode::U, l.expr)?;
        self.upd.s(";\n");
        self.lets.push((l.name.clone(), j3.clone(), l.expr));
        self.rename.push((l.name.clone(), j3));
        Ok(())
    }

    fn seq_in<'n>(&mut self, nodes: impl IntoIterator<Item = &'n Node>, j: &mut usize, depth: usize, base: &Base) -> R<()> {
        for n in nodes {
            match n {
                Node::Let(l) => self.let_node(l)?,
                Node::Text { parts, range } => {
                    self.upd.set(self.fi, range.0, range.1);
                    self.upd.pad();
                    self.upd.s("d.text(");
                    self.string(Mode::U, parts, *range)?;
                    self.upd.s(");\n");
                }
                Node::Elem(e) => {
                    let slot = Self::slot(base, *j);
                    *j += 1;
                    if e.comp {
                        self.comp(e, slot, depth)?;
                    } else if e.tag == "slot" {
                        self.slot_elem(e, slot, depth)?;
                    } else {
                        self.elem(e, slot, depth)?;
                    }
                }
                Node::If(i) => self.ifnode(i, j, depth, base)?,
                Node::Each(e) => {
                    let slot = Self::slot(base, *j);
                    *j += 1;
                    self.each(e, slot, depth)?;
                }
            }
        }
        Ok(())
    }

    pub fn accepted(tag: &str) -> Vec<&'static str> {
        let mut v = vec!["id", "class", "class:name", "on:click", "on:mousedown", "on:mouseup", "on:mousemove", "on:wheel", "on:close", "on:contextmenu", "popover", "bind:tooltip", "closedby", "title", "left", "top", "panel", "panel-handle", "panel-resize", "menuitem", "tabindex", "aria-label", "aria-live", "aria-describedby", "aria-selected", "role", "focus-group", "nav-up", "nav-down", "nav-left", "nav-right", "transition:fade", "transition:fly", "transition:slide", "transition:scale", "in:fade", "in:fly", "in:slide", "in:scale", "out:fade", "out:fly", "out:slide", "out:scale", "fly-x", "fly-y", "scale-from", "slide-axis", "transition-easing", "animate:flip", "animate-easing", "anim-delay", "virtual", "row-height", "est-height", "stick-to-bottom", "keep", "bind:at-end", "grid", "on:edit", "draggable", "drag-data", "drag-id", "drag-ghost", "on:dragstart", "on:dragend", "on:dragenter", "on:dragleave", "on:dragover", "on:drop", "bind:this"];
        match tag {
            "dialog" => v.extend(["modal", "autoselect"]),
            "label" => v.push("for"),
            "button" => v.extend(["disabled", "default"]),
            "input" | "textarea" => v.extend(["disabled", "type", "min", "max", "step", "value", "high", "checked", "placeholder", "maxlength", "rows", "rich", "name", "bind:value", "bind:spans", "bind:high", "bind:checked", "bind:group", "on:input", "on:change", "on:submit", "on:trigger", "triggers"]),
            "select" => v.extend(["disabled", "multiple", "value", "placeholder", "bind:value", "on:input"]),
            "option" => v.extend(["disabled", "value"]),
            "canvas" => v.push("on:draw"),
            "img" => v.extend(["src", "alt", "sprite", "frame", "fps"]),
            "progress" => v.extend(["value", "max"]),
            "meter" => v.extend(["value", "min", "max", "low", "high", "optimum"]),
            "glyph" => v.extend(["action", "key", "pad"]),
            "details" => v.extend(["open", "bind:open", "on:toggle"]),
            "svg" => v.push("viewBox"),
            "path" => v.extend(["d", "fill", "stroke", "stroke-width"]),
            _ => {}
        }
        v
    }

    /// `transition:` / `in:` / `out:` (fade, fly, slide, scale) and their parameters become one `d.trans(...)` call.
    /// Kinds are `Trans.ik` / `ok` in `j3.ui.dom`: 0 none, 1 fade, 2 fly, 3 slide, 4 scale.
    fn transitions(&mut self, e: &Elem, n: &str) -> R<()> {
        const KINDS: [&str; 4] = ["fade", "fly", "slide", "scale"];
        let mut dir: [Option<(usize, Value)>; 2] = [None, None]; // [enter, leave]
        for a in &e.attrs {
            let (rest, ins, outs) = if let Some(r) = a.name.strip_prefix("transition:") { (r, true, true) } else if let Some(r) = a.name.strip_prefix("in:") { (r, true, false) } else if let Some(r) = a.name.strip_prefix("out:") { (r, false, true) } else { continue };
            let Some(k) = KINDS.iter().position(|x| *x == rest) else {
                return err(a.range.0, a.range.1, format!("unknown transition `{rest}`: use fade, fly, slide or scale"));
            };
            if !matches!(a.value, Value::None | Value::Expr(_)) {
                return err(a.range.0, a.range.1, format!("`{}` takes `={{milliseconds}}`", a.name));
            }
            for (i, on) in [ins, outs].into_iter().enumerate() {
                if on {
                    if dir[i].is_some() {
                        return err(a.range.0, a.range.1, format!("`{}` repeats the {} transition of this element: use `transition:` for both directions, or one `in:` and one `out:`", a.name, if i == 0 { "enter" } else { "leave" }));
                    }
                    dir[i] = Some((k + 1, a.value.clone()));
                }
            }
        }
        let uses = |k: usize| dir.iter().any(|d| matches!(d, Some((x, _)) if *x == k));
        for (name, kind) in [("fly-x", 2), ("fly-y", 2), ("scale-from", 4), ("slide-axis", 3), ("transition-easing", 0)] {
            if let Some(a) = e.attrs.iter().find(|x| x.name == name) {
                if kind == 0 && dir.iter().all(|d| d.is_none()) {
                    return err(a.range.0, a.range.1, "`transition-easing` needs `transition:`, `in:` or `out:` on the same element");
                }
                if kind != 0 && !uses(kind) {
                    return err(a.range.0, a.range.1, format!("`{name}` needs a `{}` transition on the same element", KINDS[kind - 1]));
                }
            }
        }
        if dir.iter().all(|d| d.is_none()) {
            return Ok(());
        }
        let lit = |name: &str| -> Option<(String, Range)> {
            let a = e.attrs.iter().find(|x| x.name == name)?;
            match &a.value {
                Value::Str(p) => match p.as_slice() {
                    [Part::Lit(l)] => Some((l.clone(), a.range)),
                    _ => Some((String::new(), a.range)),
                },
                _ => Some((String::new(), a.range)),
            }
        };
        let ease = match lit("transition-easing") {
            None => -1,
            Some((l, r)) => match ["ease", "linear", "ease-in", "ease-out", "ease-in-out", "bounce", "spring"].iter().position(|k| *k == l) {
                Some(i) => i as i32,
                None => return err(r.0, r.1, "`transition-easing` takes one of \"ease\", \"linear\", \"ease-in\", \"ease-out\", \"ease-in-out\", \"bounce\", \"spring\""),
            },
        };
        let axis = match lit("slide-axis") {
            None => 0,
            Some((l, r)) => match l.as_str() {
                "y" => 0,
                "x" => 1,
                _ => return err(r.0, r.1, "`slide-axis` takes \"x\" or \"y\""),
            },
        };
        self.upd.pad();
        self.upd.s(&format!("d.trans({n}"));
        for d in &dir {
            match d {
                None => self.upd.s(", 0, 0.0"),
                Some((k, v)) => {
                    self.upd.s(&format!(", {k}, "));
                    match v {
                        Value::Expr(r) => {
                            self.upd.s("f32(");
                            self.expr(Mode::U, *r)?;
                            self.upd.s(")");
                        }
                        _ => self.upd.s("150.0"),
                    }
                }
            }
        }
        self.upd.s(&format!(", {ease}"));
        for (name, def) in [("fly-x", "0.0"), ("fly-y", "20.0"), ("scale-from", "0.5")] {
            self.upd.s(", ");
            match e.attrs.iter().find(|x| x.name == name) {
                None => self.upd.s(def),
                Some(a) => {
                    let Value::Expr(r) = a.value else { return err(a.range.0, a.range.1, format!("`{name}` takes `={{number}}`")) };
                    self.upd.set(self.fi, a.range.0, a.range.1);
                    self.upd.s("f32(");
                    self.expr(Mode::U, r)?;
                    self.upd.s(")");
                }
            }
        }
        self.upd.s(&format!(", {axis});\n"));
        Ok(())
    }

    fn elem(&mut self, e: &Elem, slot: String, depth: usize) -> R<()> {
        if e.tag == "option" { option_visuals(&e.children)?; }
        if !ELEMENTS.contains(&e.tag.as_str()) {
            return Err(UiError { lo: e.open.0, hi: e.open.0 + 1 + e.tag.len(), msg: format!("unknown element `<{}>`", e.tag), help: Some(format!("elements: {}. A capitalised tag is a component (ui/Name.j3ui).", ELEMENTS.join(" "))) });
        }
        let k = self.nk();
        let n = format!("n{k}");
        let tag = self.tag(&e.tag);
        let scope = format!("SCOPE_{}", self.c.id);
        self.upd.set(self.fi, e.open.0, e.open.1);
        self.upd.line(&format!("let {n} = d.open({slot}, {tag}, {scope});"));
        let accepted = Self::accepted(&e.tag);
        let mut draw: Option<Range> = None;
        // `bind:spans` runs after every other attribute: replacing the text drops the box's spans, so the spans must come last
        let mut spans: Option<&Attr> = None;
        self.virtual_attrs(e, &n)?;
        self.transitions(e, &n)?;
        for a in &e.attrs {
            let ok = accepted.contains(&a.name.as_str()) || (a.name.starts_with("use:") && a.name.len() > 4) || (a.name.starts_with("class:") && a.name.len() > 6) || (a.name.starts_with("style:") && a.name.len() > 6);
            if a.name == "slot" && self.slots.slotted.contains(&addr(e)) {
                continue; // goes to the parent component's slot
            }
            if e.tag == "glyph" && matches!(a.name.as_str(), "action" | "key" | "pad") {
                continue; // read by the `glyph` arm below
            }
            if matches!(a.name.as_str(), "virtual" | "row-height" | "est-height" | "stick-to-bottom" | "keep" | "bind:at-end") {
                continue; // read by `virtual_attrs`
            }
            if matches!(a.name.as_str(), "fly-x" | "fly-y" | "scale-from" | "slide-axis" | "transition-easing") || a.name.starts_with("transition:") || a.name.starts_with("in:") || a.name.starts_with("out:") {
                continue; // read by `transitions`
            }
            if a.name == "animate-easing" {
                if !e.attrs.iter().any(|x| x.name == "animate:flip") {
                    return err(a.range.0, a.range.1, "`animate-easing` needs `animate:flip` on the same element");
                }
                continue; // read with `animate:flip`
            }
            if a.name == "animate:flip" && !self.flip_rows.contains(&addr(e)) {
                let why = if self.in_virt > 0 { "a `virtual` list scrolls its rows, which would flip" } else { "it must be the row element of a keyed `{#each}`" };
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`animate:flip` is not allowed here: {why}"), help: Some("`{#each items as it (it.id)}<li animate:flip={200}>...</li>{/each}`".into()) });
            }
            if !ok {
                let list = accepted.join(", ");
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown attribute `{}` on `<{}>`", a.name, e.tag), help: Some(format!("`<{}>` accepts: {list}", e.tag)) });
            }
            self.upd.set(self.fi, a.range.0, a.range.1);
            if a.name == "on:draw" {
                match a.value {
                    Value::Expr(r) => draw = Some(r),
                    _ => return err(a.range.0, a.range.1, "`on:draw` needs `={ statements }`"),
                }
                continue;
            }
            if a.name == "bind:spans" {
                spans = Some(a);
                continue;
            }
            self.attr(e, a, &n)?;
        }
        if let Some(a) = spans {
            self.upd.set(self.fi, a.range.0, a.range.1);
            self.attr(e, a, &n)?;
        }
        match e.tag.as_str() {
            // gauges: no `value` is an indeterminate progress (-1), `max` is 1 like a browser's
            "progress" | "meter" => {
                if let Some(c) = e.children.first() {
                    let at = match c { Node::Elem(x) => x.open, _ => e.open };
                    return err(at.0, at.1, format!("`<{}>` is painted by the runtime and takes no children", e.tag));
                }
                if e.tag == "progress" && !e.attrs.iter().any(|a| a.name == "value") {
                    self.upd.line(&format!("d.attrValue({n}, -1.0);"));
                }
                if !e.attrs.iter().any(|a| a.name == "max") {
                    self.upd.line(&format!("d.attrMax({n}, 1.0);"));
                }
            }
            // an input prompt: the runtime picks the label for the device in use (`dom/glyph.j3`)
            "glyph" => {
                if let Some(c) = e.children.first() {
                    let at = match c { Node::Elem(x) => x.open, _ => e.open };
                    return err(at.0, at.1, "`<glyph>` is filled by the runtime and takes no children");
                }
                let find = |name: &str| e.attrs.iter().find(|a| a.name == name);
                if find("action").is_none() && find("key").is_none() && find("pad").is_none() {
                    return err(e.open.0, e.open.1, "`<glyph>` needs `action=\"confirm\"`, or `key=\"Space\"` and/or `pad=\"x\"`");
                }
                if let Some(a) = find("action") {
                    if let Value::Str(p) = &a.value {
                        if let [Part::Lit(l)] = p.as_slice() {
                            if !GLYPH_ACTIONS.contains(&l.as_str()) {
                                return err(a.range.0, a.range.1, format!("unknown glyph action `{l}`")).map_err(|e| e.help(format!("built-in actions: {}; a game's own action needs `key=` / `pad=` or `d.setGlyphBinding`", GLYPH_ACTIONS.join(" "))));
                            }
                        }
                    }
                }
                self.upd.set(self.fi, e.open.0, e.open.1);
                self.upd.pad();
                self.upd.s(&format!("d.glyph({n}"));
                for name in ["action", "key", "pad"] {
                    self.upd.s(", ");
                    match find(name).map(|a| (a, &a.value)) {
                        None => self.upd.s("\"\""),
                        Some((_, Value::Str(p))) => self.string(Mode::U, p, find(name).unwrap().range)?,
                        Some((_, Value::Expr(r))) => self.expr(Mode::U, *r)?,
                        Some((a, Value::None)) => return err(a.range.0, a.range.1, format!("`{name}` needs a value")),
                    }
                }
                self.upd.s(");\n");
            }
            // the disclosure button: focusable, Enter / Space click it (`inputs.begin` flips the `<details>`)
            "summary" => self.upd.line(&format!("d.attrTabIndex({n});")),
            _ => {}
        }
        if self.pass_root == addr(e) {
            self.pass_apply(e, &n);
        }
        let canvas = e.tag == "canvas";
        // paint
        let pn = format!("n{k}");
        self.pnt.set(self.fi, e.open.0, e.open.1);
        self.pnt.line(&format!("let {pn} = d.paintNext({slot});"));
        self.pnt.line(&format!("if {pn} >= 0 {{"));
        self.pnt.ind += 1;
        self.pnt.line(&format!("d.paintBox({pn});"));
        if let (true, Some(r)) = (canvas, draw) {
            self.pnt.line(&format!("if d.beginCanvas({pn}) {{"));
            self.pnt.ind += 1;
            self.pnt.line(&format!("let size{k} = d.size({pn});"));
            self.rename.insert(0, ("size".into(), format!("size{k}")));
            self.locals.push(format!("size{k}"));
            self.stmts(Mode::P, r, false)?;
            self.locals.pop();
            self.rename.remove(0);
            self.pnt.line(&format!("d.endCanvas({pn});"));
            self.pnt.ind -= 1;
            self.pnt.line("}");
        }
        self.pnt.line(&format!("let pass{k} = 0;"));
        self.pnt.line(&format!("while d.paintChildren(pass{k}) {{"));
        self.pnt.ind += 1;
        let mut j = 0;
        self.seq(&e.children, &mut j, depth, &None)?;
        self.pnt.line(&format!("pass{k} += 1;"));
        self.pnt.ind -= 1;
        self.pnt.line("}");
        self.pnt.set(self.fi, e.open.0, e.open.1);
        self.pnt.line(&format!("d.paintEnd({pn});"));
        self.pnt.ind -= 1;
        self.pnt.line("}");
        self.upd.set(self.fi, e.open.0, e.open.1);
        self.upd.line("d.close();");
        Ok(())
    }

    /// `virtual` on a scroll box: its one child must be a keyed `{#each}`, which `each` then lowers to the rows
    /// near the viewport. `row-height={26}` makes every row that tall; without it (or `row-height="auto"`) rows
    /// are measured, starting from `est-height={40}`. `stick-to-bottom`, `keep={keys}` and
    /// `bind:at-end={place}` are described in `j3c doc j3ui`.
    fn virtual_attrs(&mut self, e: &Elem, n: &str) -> R<()> {
        let find = |name: &str| e.attrs.iter().find(|a| a.name == name);
        let Some(va) = find("virtual") else {
            for x in ["row-height", "est-height", "stick-to-bottom", "keep", "bind:at-end"] {
                if let Some(r) = find(x) {
                    return err(r.range.0, r.range.1, format!("`{x}` needs `virtual` on the same element"));
                }
            }
            return Ok(());
        };
        if e.comp {
            return err(va.range.0, va.range.1, "`virtual` goes on a plain element (the scroll box), not a component");
        }
        if e.children.len() != 1 || !matches!(e.children[0], Node::Each(_)) {
            return err(va.range.0, va.range.1, "a `virtual` element holds exactly one child, a keyed `{#each}`").map_err(|x| x.help("`<div virtual row-height={26}>{#each rows as r (r.id)} ... {/each}</div>`, with nothing else inside"));
        }
        let Node::Each(h) = &e.children[0] else { unreachable!() };
        if !parse_each_head(self.src, h.head).map_or(false, |h| h.key.is_some() && h.range.is_none()) {
            return err(va.range.0, va.range.1, "the `{#each}` in a `virtual` element needs a key and a list").map_err(|x| x.help("`{#each rows as r (r.id)}`; a `0..n` range has no list to window"));
        }
        if e.attrs.iter().filter(|x| x.name.starts_with("style:")).count() > 6 {
            return err(va.range.0, va.range.1, "a `virtual` element takes at most 6 `style:` attributes (two are its spacers)");
        }
        let num = |a: &Attr, what: &str| match &a.value {
            Value::Expr(r) => Ok(Value::Expr(*r)),
            Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l.trim().parse::<f64>().is_ok()) => Ok(Value::Str(p.clone())),
            _ => err(a.range.0, a.range.1, format!("`{what}` needs a number: `{what}={{26}}`")),
        };
        // `row-height="auto"` is the same as no `row-height`
        let rh = match find("row-height") {
            Some(a) if matches!(&a.value, Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l.trim() == "auto")) => None,
            Some(a) => Some(num(a, "row-height")?),
            None => None,
        };
        let est = match find("est-height") {
            Some(a) if rh.is_some() => return err(a.range.0, a.range.1, "`est-height` is the guess for measured rows; this list has a `row-height`"),
            Some(a) => Some(num(a, "est-height")?),
            None => None,
        };
        let stick = match find("stick-to-bottom") {
            Some(a) if !matches!(a.value, Value::None) => return err(a.range.0, a.range.1, "`stick-to-bottom` takes no value"),
            Some(_) => true,
            None => false,
        };
        let keep = match find("keep") {
            Some(a) => match a.value {
                Value::Expr(r) => Some(r),
                _ => return err(a.range.0, a.range.1, "`keep` needs `={keys}`, a `[]string` of row keys").map_err(|x| x.help("`<div virtual keep={state.pinned}>`; the keys are the `{#each}` key formatted as text")),
            },
            None => None,
        };
        let at_end = match find("bind:at-end") {
            Some(a) => match a.value {
                Value::Expr(r) => {
                    self.place_check(r)?;
                    Some(r)
                }
                _ => return err(a.range.0, a.range.1, "`bind:at-end` needs `={place}`, a `bool` to write to"),
            },
            None => None,
        };
        self.virt = Some(Virt { n: n.to_string(), rh, est, stick, keep, at_end });
        Ok(())
    }

    /// `call(`{key}`)` for row `ix` of the list `x<k>`, with the item (and index) names bound to that row.
    /// `tag` makes the temporaries unique in their scope.
    fn virt_key(&mut self, item: &str, index: &Option<String>, key: Range, int: bool, call: &str, ix: &str, tag: &str) -> R<()> {
        let kn: String = tag.chars().take_while(|c| c.is_ascii_digit()).collect();
        let nren = self.rename.len();
        self.upd.line(&format!("let kr{tag} = x{kn}[{ix}];"));
        self.rename.push((item.to_string(), format!("kr{tag}")));
        if let Some(x) = index {
            self.upd.line(&format!("let ks{tag} = {ix};"));
            self.rename.push((x.clone(), format!("ks{tag}")));
        }
        self.upd.set(self.fi, key.0, key.1);
        self.upd.pad();
        let (open, close) = if int { (format!("{call}Int("), ");\n") } else { (format!("{call}(`{{"), "}`);\n") };
        self.upd.s(&open);
        let r = self.expr(Mode::U, key);
        self.upd.s(close);
        self.rename.truncate(nren);
        r
    }

    /// A number attribute value as J3 text (`dflt` when absent).
    fn virt_num(&mut self, v: &Option<Value>, dflt: &str) -> R<()> {
        match v {
            Some(Value::Expr(r)) => self.expr(Mode::U, *r)?,
            Some(Value::Str(p)) => {
                if let [Part::Lit(l)] = p.as_slice() {
                    self.upd.s(l.trim());
                }
            }
            _ => self.upd.s(dflt),
        }
        Ok(())
    }

    /// A string attribute: literal text, `"a {b}"`, or `{expr}`, passed to `d.<call>(n, s)`.
    fn str_attr(&mut self, a: &Attr, call: &str, n: &str) -> R<()> {
        self.upd.pad();
        self.upd.s(&format!("d.{call}({n}, "));
        match &a.value {
            // a template string dies when `update` returns, and `value` is kept as a view
            Value::Str(p) if call == "attrText" && p.iter().any(|x| matches!(x, Part::Hole(_))) => {
                return err(a.range.0, a.range.1, "`value` text can't have `{}` holes").map_err(|e| e.help("use `value={expr}` with a `string` that outlives the frame"));
            }
            Value::Str(p) => self.string(Mode::U, p, a.range)?,
            Value::Expr(r) => self.expr(Mode::U, *r)?,
            Value::None => return err(a.range.0, a.range.1, format!("`{}` needs a value", a.name)),
        }
        self.upd.s(");\n");
        Ok(())
    }

    fn num_attr(&mut self, a: &Attr, call: &str, n: &str) -> R<()> {
        self.upd.pad();
        self.upd.s(&format!("d.{call}({n}, f32("));
        match &a.value {
            Value::Expr(r) => self.expr(Mode::U, *r)?,
            Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l.trim().parse::<f64>().is_ok()) => {
                if let [Part::Lit(l)] = p.as_slice() {
                    self.upd.s(l.trim());
                }
            }
            _ => return err(a.range.0, a.range.1, format!("`{}` needs a number: `{}={{expr}}`", a.name, a.name)),
        }
        self.upd.s("));\n");
        Ok(())
    }

    /// `style:prop={number}` or `style:prop="css {value}"` (or `={"text"}`): an inline style, `d.styleNum` / `d.styleStr`.
    /// Checks `style:prop` and says whether its value is a number (`={n}`) or text.
    fn style_val(&self, a: &Attr, prop: &str) -> R<bool> {
        let longhand = css_props().split(' ').any(|p| p == prop);
        let shorthand = ["padding", "margin", "inset", "border", "border-color", "border-width", "border-radius", "border-top", "border-right", "border-bottom", "border-left", "overflow", "gap", "background", "text-stroke", "text-wave"].contains(&prop);
        if prop.starts_with("animation") || prop.starts_with("transition") || prop.starts_with('-') || prop.starts_with("grid-") || !(longhand || shorthand) {
            return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`style:{prop}` is not a CSS property that can be set inline"), help: Some("any property of `j3c doc css` except `animation`, `transition`, grid properties and custom properties".into()) });
        }
        match &a.value {
            Value::Expr(r) if !matches!(self.src[r.0..r.1].trim_start().chars().next(), Some('"' | '`')) => {
                // `gap` is in the longhand list (column gap) but `style:gap={n}` would set only that one, so a shorthand is text
                if !longhand || shorthand {
                    return err(a.range.0, a.range.1, format!("`style:{prop}` is a shorthand: give it text, `style:{prop}=\"{{v}}px\"`"));
                }
                Ok(true)
            }
            Value::None => err(a.range.0, a.range.1, format!("`style:{prop}` needs a value: `style:{prop}={{number}}` or `style:{prop}=\"text\"`")),
            _ => Ok(false),
        }
    }

    fn style_attr(&mut self, e: &Elem, a: &Attr, prop: &str, n: &str) -> R<()> {
        let num = self.style_val(a, prop)?;
        let k = e.attrs.iter().filter(|x| x.name.starts_with("style:")).position(|x| std::ptr::eq(x, a)).unwrap_or(0);
        if k >= 8 {
            return err(a.range.0, a.range.1, "an element takes at most 8 `style:` attributes");
        }
        self.upd.pad();
        match &a.value {
            Value::Expr(r) if num => {
                self.upd.s(&format!("d.styleNum({n}, {k}, {}, f32(", lit(prop)));
                self.expr(Mode::U, *r)?;
                self.upd.s("));\n");
            }
            Value::Expr(r) => {
                self.upd.s(&format!("d.styleStr({n}, {k}, {}, ", lit(prop)));
                self.expr(Mode::U, *r)?;
                self.upd.s(");\n");
            }
            Value::Str(p) => {
                self.upd.s(&format!("d.styleStr({n}, {k}, {}, ", lit(prop)));
                self.string(Mode::U, p, a.range)?;
                self.upd.s(");\n");
            }
            Value::None => unreachable!("checked by style_val"),
        }
        Ok(())
    }

    fn attr(&mut self, e: &Elem, a: &Attr, n: &str) -> R<()> {
        let name = a.name.as_str();
        if name == "class" {
            match &a.value {
                Value::Str(p) => {
                    // whole literal words are static; a word with a `{hole}` is joined into one dynamic text
                    let (mut dynp, mut word, mut buf): (Vec<Part>, Vec<Part>, String) = (Vec::new(), Vec::new(), String::new());
                    for part in p.iter().cloned().chain([Part::Lit(" ".into())]) {
                        match part {
                            Part::Hole(_) => {
                                if !buf.is_empty() {
                                    word.push(Part::Lit(std::mem::take(&mut buf)));
                                }
                                word.push(part);
                            }
                            Part::Lit(l) => {
                                for ch in l.chars() {
                                    if !ch.is_whitespace() {
                                        buf.push(ch);
                                        continue;
                                    }
                                    if !buf.is_empty() {
                                        word.push(Part::Lit(std::mem::take(&mut buf)));
                                    }
                                    match word.as_slice() {
                                        [] => {}
                                        [Part::Lit(w)] => {
                                            let f = self.cls(w);
                                            self.upd.line(&format!("d.clsId({n}, {f});"));
                                        }
                                        _ => {
                                            if !dynp.is_empty() {
                                                dynp.push(Part::Lit(" ".into()));
                                            }
                                            dynp.append(&mut word);
                                        }
                                    }
                                    word.clear();
                                }
                            }
                        }
                    }
                    if !dynp.is_empty() {
                        self.upd.pad();
                        self.upd.s(&format!("d.clsDyn({n}, "));
                        self.string(Mode::U, &dynp, a.range)?;
                        self.upd.s(");\n");
                    }
                }
                Value::None => {}
                _ => return Err(UiError { lo: a.range.0, hi: a.range.1, msg: "`class` takes text".into(), help: Some("`class=\"a b{x}\"`; switch a class with `class:name={condition}`".into()) }),
            }
        } else if let Some(prop) = name.strip_prefix("style:") {
            self.style_attr(e, a, prop, n)?;
        } else if let Some(c) = name.strip_prefix("class:") {
            let Value::Expr(r) = a.value else {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`{name}` needs a condition"), help: Some(format!("{name}={{condition}}")) });
            };
            let f = self.cls(c);
            self.upd.pad();
            self.upd.s(&format!("d.clsIfId({n}, {f}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(");\n");
        } else if name == "disabled" || name == "checked" || name == "multiple" {
            let call = match name {
                "disabled" => "attrDisabled",
                "multiple" => "attrMultiple",
                _ if is_radio(e) => "attrRadio",
                _ => "attrChecked",
            };
            self.upd.pad();
            self.upd.s(&format!("d.{call}({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                _ => return err(a.range.0, a.range.1, format!("`{name}` takes `{{condition}}`")),
            }
            self.upd.s(");\n");
        } else if name == "popover" || name == "modal" {
            // an overlay: the element is in the tree while it is open (`{#if open}`)
            let kind = if name == "modal" { 1 } else { 2 };
            match &a.value {
                Value::Str(p) if name == "popover" && matches!(p.as_slice(), [Part::Lit(l)] if l == "context") => self.upd.line(&format!("d.attrContext({n});")),
                Value::Str(p) if name == "popover" && matches!(p.as_slice(), [Part::Lit(l)] if l == "caret") => self.upd.line(&format!("d.attrCaret({n});")),
                Value::None => self.upd.line(&format!("d.attrOverlay({n}, {kind});")),
                Value::Expr(r) => {
                    self.upd.pad();
                    self.upd.s("if ");
                    self.expr(Mode::U, *r)?;
                    self.upd.s(&format!(" {{ d.attrOverlay({n}, {kind}); }}\n"));
                }
                _ => return err(a.range.0, a.range.1, format!("`{name}` takes `{{condition}}`")),
            }
        } else if name == "panel" {
            // a draggable panel; `panel="dock"` lets it snap to the workspace edges
            if matches!(a.value, Value::None) {
                self.upd.line(&format!("d.attrPanel({n}, \"\");"));
            } else {
                self.str_attr(a, "attrPanel", n)?;
            }
        } else if name == "panel-handle" || name == "panel-resize" {
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, format!("`{name}` takes no value"));
            }
            // dom.PK_HANDLE / PK_GRIP
            let role = if name == "panel-handle" { 2 } else { 3 };
            self.upd.line(&format!("d.attrPanelRole({n}, {role});"));
        } else if name == "draggable" {
            self.upd.pad();
            self.upd.s(&format!("d.attrDraggable({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                _ => return err(a.range.0, a.range.1, "`draggable` takes `{condition}`"),
            }
            self.upd.s(");\n");
        } else if matches!(name, "drag-data" | "drag-id" | "drag-ghost") {
            if !e.attrs.iter().any(|x| x.name == "draggable") {
                return err(a.range.0, a.range.1, format!("`{name}` needs `draggable` on the same element"));
            }
            match name {
                "drag-data" => self.str_attr(a, "attrDragData", n)?,
                "drag-ghost" => {
                    if !matches!(&a.value, Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l == "none")) {
                        return err(a.range.0, a.range.1, "`drag-ghost` takes only `\"none\"`").map_err(|e| e.help("the page paints its own drag image; leave the attribute out for the default ghost"));
                    }
                    self.str_attr(a, "attrDragGhost", n)?;
                }
                _ => {
                    let Value::Expr(r) = a.value else {
                        return err(a.range.0, a.range.1, "`drag-id` needs `={integer}`");
                    };
                    self.upd.pad();
                    self.upd.s(&format!("d.attrDragId({n}, "));
                    self.expr(Mode::U, r)?;
                    self.upd.s(");\n");
                }
            }
        } else if name == "menuitem" {
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, "`menuitem` takes no value".to_string());
            }
            self.upd.line(&format!("d.attrMenuItem({n});"));
        } else if name == "tabindex" {
            // focusable like a button: -1 skips Tab, 0 is tree order, a positive integer goes first (ascending); `={expr}` is an int
            let lit = match &a.value {
                Value::None => Some(0),
                Value::Str(p) => match p.as_slice() { [Part::Lit(l)] => l.trim().parse::<i64>().ok().filter(|v| *v >= -1), _ => None },
                _ => None,
            };
            if let Value::Expr(r) = a.value {
                self.upd.s(&format!("d.attrTabIndexN({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            } else {
                match lit {
                    None => return err(a.range.0, a.range.1, "`tabindex` takes `-1`, `0`, a positive integer or `={int}`".to_string()),
                    Some(-1) => self.upd.line(&format!("d.attrTabIndexOff({n});")),
                    Some(0) => self.upd.line(&format!("d.attrTabIndex({n});")),
                    Some(v) => self.upd.line(&format!("d.attrTabIndexN({n}, {v});")),
                }
            }
        } else if name == "grid" {
            // the window's data grid (`j3.ui.grid`): `grid={g.sized(rows.len)}`, a `&Grid`, on the `virtual` scroll box
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`grid` needs `={g.sized(rows.len)}`, a `&Grid` (`j3.ui.gridstate.Grid`)");
            };
            if !e.attrs.iter().any(|x| x.name == "virtual") {
                return err(a.range.0, a.range.1, "`grid` goes on the `virtual` scroll box (`<div virtual row-height={h} grid={..}>`)");
            }
            self.upd.pad();
            self.upd.s(&format!("d.gridBind({n}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(");\n");
        } else if name == "autoselect" || name == "default" {
            // bare flags: the dialog selects its first text box when it opens; the button Enter clicks (`Dom.attrFlag`)
            if !matches!(a.value, Value::None) {
                return err(a.range.0, a.range.1, format!("`{name}` takes no value"));
            }
            self.upd.line(&format!("d.attrFlag({n}, {});", if name == "default" { 3 } else { 1 }));
        } else if name == "closedby" {
            self.str_attr(a, "attrClosedBy", n)?;
        } else if name == "title" {
            self.str_attr(a, "attrTitle", n)?;
        } else if name == "left" || name == "top" {
            self.num_attr(a, if name == "left" { "attrLeft" } else { "attrTop" }, n)?;
        } else if name == "id" {
            self.str_attr(a, "attrId", n)?;
        } else if name == "for" {
            // an id is a static name (`attrId`), so the label's target is one too
            match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => self.str_attr(a, "attrFor", n)?,
                _ => return err(a.range.0, a.range.1, "for= takes a static id").map_err(|e| e.help("write `for=\"name\"`, the `id=\"name\"` of the control")),
            }
        } else if name == "type" {
            self.str_attr(a, "attrType", n)?;
        } else if name == "maxlength" || name == "rows" {
            self.num_attr(a, if name == "rows" { "attrRows" } else { "attrMaxLen" }, n)?;
        } else if e.tag == "meter" && matches!(name, "low" | "high" | "optimum") {
            // `Dom.attrMeter`: 0 low, 1 high, 2 optimum
            let which = match name { "low" => 0, "high" => 1, _ => 2 };
            self.num_attr(a, "attrMeter", &format!("{n}, {which}"))?;
        } else if name == "open" {
            self.upd.pad();
            self.upd.s(&format!("d.attrOpen({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                _ => return err(a.range.0, a.range.1, "`open` takes `={condition}` or no value"),
            }
            self.upd.s(");\n");
        } else if name == "bind:this" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`bind:this` needs `={place}`, an `int` to write the element's handle to");
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.handle({n});\n"));
            self.upd.hint = None;
            if self.bases.last().copied().unwrap_or((0, 0)) == (self.locals.len(), self.lets.len()) {
                let k = self.nk();
                let o = self.owner();
                self.upd.line(&format!("{o}.bh{k} = d.handle({n});"));
                self.binds.push(BindDef { k, container: self.own.last().copied().unwrap_or(0), place: r });
            }
        } else if let Some(an) = name.strip_prefix("use:") {
            self.action(a, an, n)?;
        } else if name == "bind:open" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`bind:open` needs `={place}`, a `bool` to write to");
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            self.upd.line(&format!("if d.userToggled({n}) {{"));
            self.upd.ind += 1;
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.isOpen({n});\n"));
            self.upd.ind -= 1;
            self.upd.line("}");
            self.upd.pad();
            self.upd.s(&format!("d.attrOpen({n}, "));
            self.expr(Mode::U, r)?;
            self.upd.s(");\n");
            self.upd.hint = None;
        } else if matches!(name, "min" | "max" | "step" | "high") || (name == "value" && (is_range(e) || matches!(e.tag.as_str(), "progress" | "meter"))) {
            let call = match name {
                "min" => "attrMin",
                "max" => "attrMax",
                "step" => "attrStep",
                "high" => "attrHigh",
                _ => "attrValue",
            };
            if name == "high" {
                self.upd.line(&format!("d.attrMultiple({n}, true);"));
            }
            self.num_attr(a, call, n)?;
        } else if let Some(code) = media_code(name) {
            // `<img>` / `<svg>` / `<path>` data: `media.M_*` string kinds and `media.N_*` number kinds
            let target = format!("{n}, {}", code.1);
            self.upd.hint = Some("image and shape attributes are text (`src=\"a.png\"`) or `={expr}`; `frame`, `fps` and `stroke-width` take numbers");
            if code.0 {
                self.num_attr(a, "attrMediaNum", &target)?;
            } else {
                self.str_attr(a, "attrMedia", &target)?;
            }
            self.upd.hint = None;
        } else if name == "alt" || name == "aria-label" {
            // nothing draws it; `j3.ui.a11y` reads it as the accessible name
            self.str_attr(a, "attrAria", n)?;
        } else if name == "role" {
            self.str_attr(a, "attrRole", n)?;
        } else if name == "aria-live" {
            // "polite", "assertive" or "off"; `j3.ui.a11y.liveChanges`
            self.str_attr(a, "attrLive", n)?;
        } else if name == "aria-describedby" {
            // ids are static names (`attrId`), so is this
            match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => self.str_attr(a, "attrDescBy", n)?,
                _ => return err(a.range.0, a.range.1, "aria-describedby= takes static ids").map_err(|e| e.help("write `aria-describedby=\"hint\"`, the `id=\"hint\"` of the element that describes this one (several ids separated by spaces)")),
            }
        } else if name == "aria-selected" {
            self.upd.pad();
            self.upd.s(&format!("d.attrSelected({n}, "));
            match &a.value {
                Value::None => self.upd.s("true"),
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if l == "true" || l == "false") => {
                    if let [Part::Lit(l)] = p.as_slice() {
                        self.upd.s(l);
                    }
                }
                _ => return err(a.range.0, a.range.1, "`aria-selected` takes `=\"true\"`, `=\"false\"` or `={condition}`"),
            }
            self.upd.s(");\n");
        } else if name == "focus-group" || name.starts_with("nav-") {
            // gamepad / arrow-key focus (`j3.ui.padnav`): names of ids, so static like `for`
            let which = match name { "focus-group" => 0, "nav-up" => 1, "nav-down" => 2, "nav-left" => 3, _ => 4 };
            match &a.value {
                Value::Str(p) if p.iter().all(|x| matches!(x, Part::Lit(_))) => self.str_attr(a, "attrNav", &format!("{n}, {which}"))?,
                _ => return err(a.range.0, a.range.1, format!("{name}= takes a static string")).map_err(|e| e.help("write `nav-down=\"id\"`, the `id=\"name\"` of the element to focus; `focus-group=\"wrap trap arrows\"` takes any of the three words")),
            }
        } else if name == "name" {
            self.str_attr(a, "attrName", n)?;
        } else if name == "value" && is_radio(e) {
            // a radio's value is read by its `bind:group`, which writes it into the variable
        } else if name == "value" || name == "placeholder" {
            self.str_attr(a, if name == "value" { "attrText" } else { "attrPlaceholder" }, n)?;
        } else if name == "animate:flip" {
            let ms = match &a.value {
                Value::None => None,
                Value::Expr(r) => Some(*r),
                _ => return err(a.range.0, a.range.1, "`animate:flip` takes `={milliseconds}`"),
            };
            let ease = match e.attrs.iter().find(|x| x.name == "animate-easing") {
                None => 3, // ease-out: `Css.eases` seeds the presets in this order
                Some(x) => {
                    let lit = match &x.value {
                        Value::Str(p) => match p.as_slice() {
                            [Part::Lit(l)] => l.as_str(),
                            _ => "",
                        },
                        _ => "",
                    };
                    match ["ease", "linear", "ease-in", "ease-out", "ease-in-out", "bounce", "spring"].iter().position(|k| *k == lit) {
                        Some(i) => i,
                        None => return err(x.range.0, x.range.1, "`animate-easing` takes one of \"ease\", \"linear\", \"ease-in\", \"ease-out\", \"ease-in-out\", \"bounce\", \"spring\""),
                    }
                }
            };
            self.upd.pad();
            self.upd.s(&format!("d.flip({n}, "));
            match ms {
                None => self.upd.s("200.0"),
                Some(r) => {
                    self.upd.s("f32(");
                    self.expr(Mode::U, r)?;
                    self.upd.s(")");
                }
            }
            self.upd.s(&format!(", {ease});\n"));
        } else if name == "anim-delay" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`anim-delay` takes `={milliseconds}`");
            };
            self.upd.pad();
            self.upd.s(&format!("d.animDelay({n}, f32("));
            self.expr(Mode::U, r)?;
            self.upd.s("));\n");
        } else if name == "bind:tooltip" {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, "`bind:tooltip` needs `={boolean_place}`");
            };
            self.place_check(r)?;
            if !tooltip_child(&e.children) {
                return err(a.range.0, a.range.1, "`bind:tooltip` needs a direct child with bare `popover`, usually `{#if card}<div popover>...</div>{/if}` inside an owner wrapper");
            }
            self.upd.pad();
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = d.hoverCard({n});\n"));
        } else if name == "triggers" {
            // the characters that start a trigger for `on:trigger` (a literal: the box keeps it as a view)
            if !e.attrs.iter().any(|x| x.name == "on:trigger") {
                return err(a.range.0, a.range.1, "`triggers` needs `on:trigger` on the same element");
            }
            match &a.value {
                Value::Str(p) if matches!(p.as_slice(), [Part::Lit(l)] if !l.is_empty()) => self.str_attr(a, "attrTriggers", n)?,
                _ => return err(a.range.0, a.range.1, "`triggers` takes a literal such as `triggers=\"@:#\"`"),
            }
        } else if name == "rich" || name == "bind:spans" {
            // a `rich` textarea keeps styled spans and atomic tokens beside its text (`j3.ui.spans.Span`)
            if input_type(e).as_deref() != Some("textarea") {
                return err(a.range.0, a.range.1, format!("`{name}` is for `<textarea rich>`"));
            }
            if name == "rich" {
                if !matches!(a.value, Value::None) {
                    return err(a.range.0, a.range.1, "`rich` takes no value");
                }
                self.upd.line(&format!("d.attrRich({n}, true);"));
            } else {
                if !e.attrs.iter().any(|x| x.name == "rich") {
                    return err(a.range.0, a.range.1, "`bind:spans` needs `rich` on the `<textarea>`");
                }
                let Value::Expr(r) = a.value else {
                    return err(a.range.0, a.range.1, "`bind:spans` needs `={place}` holding a `[]Span` (`use j3.ui.spans.Span;`)");
                };
                self.place_check(r)?;
                self.upd.line(&format!("if d.changed({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" = d.spansValue({n});\n"));
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.attrSpans({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            }
        } else if name == "bind:group" {
            self.bind_group(e, a, n)?;
        } else if matches!(name, "bind:checked" | "bind:value" | "bind:high") {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, format!("`{name}` needs `={{place}}`, a name, field, or index to write to"));
            };
            self.place_check(r)?;
            self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
            if name == "bind:checked" && is_radio(e) {
                // a radio flips by itself (and its group-mates with it): write back every one that flipped
                self.upd.line(&format!("if d.userToggled({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" = d.isChecked({n});\n"));
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.attrRadio({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            } else if name == "bind:checked" {
                self.upd.line(&format!("if d.clicked({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(" = !");
                self.expr(Mode::U, r)?;
                self.upd.s(";\n");
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.attrChecked({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            } else {
                // a range binds a number, anything else (text, search, color, select) a string
                let (get, set) = match (name, input_type(e).as_deref()) {
                    ("bind:high", _) => ("highValue", "attrHigh"),
                    (_, _) if is_range(e) => ("value", "attrValue"),
                    (_, Some(_)) => ("strValue", "attrText"),
                    (_, None) => return err(a.range.0, a.range.1, "`bind:value` needs a literal `type` (`type=\"text\"`), so the compiler knows what it binds"),
                };
                if name == "bind:high" {
                    self.upd.line(&format!("d.attrMultiple({n}, true);"));
                }
                self.upd.line(&format!("if d.changed({n}) {{"));
                self.upd.ind += 1;
                self.upd.pad();
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" = d.{get}({n});\n"));
                self.upd.ind -= 1;
                self.upd.line("}");
                self.upd.pad();
                self.upd.s(&format!("d.{set}({n}, "));
                self.expr(Mode::U, r)?;
                self.upd.s(");\n");
            }
            self.upd.hint = None;
        } else if let Some(ev) = name.strip_prefix("on:") {
            let Value::Expr(r) = a.value else {
                return err(a.range.0, a.range.1, format!("`{name}` needs `={{ statements }}`"));
            };
            if !MOUSE.contains(&ev) {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown event `{name}` on `<{}>`", e.tag), help: Some(format!("events: {}{}", MOUSE.join(" "), if e.tag == "canvas" { " draw" } else { "" })) });
            }
            let mut extra: Vec<&str> = Vec::new();
            let head = match ev {
                "click" => format!("if d.clicked({n}) {{"),
                "wheel" => {
                    extra.push("wheel");
                    format!("if d.wheelAt({n}) != 0.0 {{")
                }
                "input" => format!("if d.changed({n}) {{"),
                "change" if !is_radio(e) => return err(a.range.0, a.range.1, "`on:change` is for `<input type=\"radio\">` (the radio that just got checked); a text box or slider has `on:input`"),
                "change" => format!("if d.changed({n}) {{"),
                "submit" => {
                    self.upd.line(&format!("d.attrFlag({n}, 2);"));
                    format!("if d.submitted({n}) {{")
                }
                "trigger" => {
                    extra.extend(["trigger", "tlo", "thi"]);
                    if !e.attrs.iter().any(|x| x.name == "triggers") {
                        self.upd.line(&format!("d.attrTriggers({n}, \"@:\");"));
                    }
                    format!("if d.triggered({n}) {{")
                }
                "close" => format!("if d.closeRequested({n}) {{"),
                "toggle" => {
                    extra.push("open");
                    format!("if d.toggled({n}) {{")
                }
                "edit" => {
                    if !e.attrs.iter().any(|x| x.name == "grid") {
                        return err(a.range.0, a.range.1, "`on:edit` needs `grid={..}` on the same element");
                    }
                    extra.extend(["kind", "row", "col", "text"]);
                    format!("if d.gridEdit({n}) {{")
                }
                "contextmenu" => {
                    extra.push("mouse");
                    format!("if d.contextAt({n}) {{")
                }
                "dragstart" | "dragend" => {
                    if !e.attrs.iter().any(|x| x.name == "draggable") {
                        return err(a.range.0, a.range.1, format!("`{name}` needs `draggable` on the same element"));
                    }
                    extra.extend(["data", "dragId"]);
                    if ev == "dragend" {
                        extra.push("dropped");
                        format!("if d.dragEnded({n}) {{")
                    } else {
                        format!("if d.dragStarted({n}) {{")
                    }
                }
                "dragenter" | "dragleave" | "dragover" | "drop" => {
                    // dom.attrDragTarget bits: 1 enter, 2 leave, 4 over, 8 drop
                    let (bit, call) = match ev {
                        "dragenter" => (1, "dragEntered"),
                        "dragleave" => (2, "dragLeft"),
                        "dragover" => (4, "dragOver"),
                        _ => (8, "dropAt"),
                    };
                    self.upd.line(&format!("d.attrDragTarget({n}, {bit});"));
                    extra.extend(["mouse", "data", "dragId"]);
                    if ev == "dragover" {
                        extra.push("accept");
                    }
                    format!("if d.{call}({n}) {{")
                }
                _ => {
                    extra.push("mouse");
                    let call = match ev {
                        "mousedown" => "mouseDown",
                        "mouseup" => "mouseUp",
                        _ => "mouseMove",
                    };
                    format!("if d.{call}({n}) {{")
                }
            };
            self.upd.line(&head);
            self.upd.ind += 1;
            if ev == "wheel" {
                self.upd.line(&format!("let wheel = d.wheelAt({n});"));
            } else if ev == "toggle" {
                self.upd.line(&format!("let open = d.isOpen({n});"));
            } else if ev == "contextmenu" {
                self.upd.line("let mouse = d.mouseAt();");
            } else if ev == "trigger" {
                self.upd.line(&format!("let trigger = d.triggerText({n});"));
                self.upd.line("let tlo = d.trigLo;");
                self.upd.line("let thi = d.trigHi;");
            } else if ev == "edit" {
                self.upd.line(&format!("let kind = d.gridOf({n}).evKind;"));
                self.upd.line(&format!("let row = d.gridOf({n}).evRow;"));
                self.upd.line(&format!("let col = d.gridOf({n}).evCol;"));
                self.upd.line(&format!("let text = d.gridOf({n}).evText;"));
            } else if matches!(ev, "dragstart" | "dragend" | "dragenter" | "dragleave" | "dragover" | "drop") {
                if matches!(ev, "dragenter" | "dragleave" | "dragover" | "drop") {
                    self.upd.line(&format!("let mouse = d.localMouse({n});"));
                }
                self.upd.line("let data = d.dragData();");
                self.upd.line("let dragId = d.dragIdOf();");
                if ev == "dragend" {
                    self.upd.line("let dropped = d.dragDropped();");
                } else if ev == "dragover" {
                    self.upd.line("let accept = true;");
                }
            } else if !matches!(ev, "click" | "input" | "change" | "submit" | "close" | "toggle") {
                self.upd.line(&format!("let mouse = d.localMouse({n});"));
            }
            if e.tag == "canvas" {
                self.upd.line(&format!("let size = d.size({n});"));
                extra.push("size");
            }
            let saved = self.locals.len();
            self.locals.extend(extra.iter().map(|s| s.to_string()));
            self.stmts(Mode::U, r, true)?;
            self.locals.truncate(saved);
            if ev == "dragover" {
                self.upd.line(&format!("if !accept {{ d.dragReject({n}); }}"));
            }
            self.upd.ind -= 1;
            self.upd.line("}");
        }
        Ok(())
    }

    /// `bind:group={x}` on a radio: when the user checks this radio `x` becomes its `value`, and the radio is checked while `x` equals it.
    /// A quoted `value="a"` makes `x` a `string`; `value={expr}` an `int` (or a `string` when `x` is a `let` declared as one).
    fn bind_group(&mut self, e: &Elem, a: &Attr, n: &str) -> R<()> {
        let Value::Expr(r) = a.value else {
            return err(a.range.0, a.range.1, "`bind:group` needs `={place}`, the variable the checked radio's `value` goes to");
        };
        if !is_radio(e) {
            return err(a.range.0, a.range.1, "`bind:group` is for `<input type=\"radio\">`").map_err(|e| e.help("write `type=\"radio\"` as a literal on the same element"));
        }
        self.place_check(r)?;
        let Some(v) = e.attrs.iter().find(|x| x.name == "value") else {
            return err(a.range.0, a.range.1, "`bind:group` needs a `value` on the same radio").map_err(|e| e.help("`value=\"a\"` for a string group, `value={1}` for an int one"));
        };
        let place = self.src[r.0..r.1].trim();
        let declared = self.c.sc.lets.iter().find(|l| l.name == place).map(|l| match (l.ty, l.guess) {
            (Some(t), _) => self.src[t.0..t.1].trim().to_string(),
            (None, Some(g)) => g.to_string(),
            (None, None) => String::new(),
        });
        let is_str = match &v.value {
            Value::Str(p) => {
                if p.iter().any(|x| matches!(x, Part::Hole(_))) {
                    return err(v.range.0, v.range.1, "a radio's `value` text can't have `{}` holes").map_err(|e| e.help("the variable keeps it past the frame: use `value={expr}` with a `string` that outlives it"));
                }
                if matches!(declared.as_deref(), Some("int" | "float" | "f32" | "bool")) {
                    return err(v.range.0, v.range.1, format!("`{place}` is {}, but `value=\"..\"` is text", declared.unwrap())).map_err(|e| e.help("write `value={1}` for an int group"));
                }
                true
            }
            Value::Expr(_) => declared.as_deref() == Some("string"),
            Value::None => return err(v.range.0, v.range.1, "`value` needs a value"),
        };
        let val = |g: &mut Self| -> R<()> {
            match &v.value {
                Value::Str(p) => g.string(Mode::U, p, v.range),
                Value::Expr(x) => g.expr(Mode::U, *x),
                Value::None => Ok(()),
            }
        };
        self.upd.hint = Some("`bind:` writes to the expression, so it must be assignable; props are read-only (bind to a field behind a prop, or to a `let`)");
        self.upd.line(&format!("if d.changed({n}) {{"));
        self.upd.ind += 1;
        self.upd.pad();
        self.expr(Mode::U, r)?;
        self.upd.s(" = ");
        val(self)?;
        self.upd.s(";\n");
        self.upd.ind -= 1;
        self.upd.line("}");
        self.upd.hint = None;
        self.upd.pad();
        self.upd.s(&format!("d.attrGroup({n}, "));
        if is_str {
            self.upd.s("d.textIs(");
            self.expr(Mode::U, r)?;
            self.upd.s(", ");
            val(self)?;
            self.upd.s(")");
        } else {
            self.upd.s("(");
            self.expr(Mode::U, r)?;
            self.upd.s(") == (");
            val(self)?;
            self.upd.s(")");
        }
        self.upd.s(");\n");
        Ok(())
    }

    /// `use:name={param}` / `use:name` on element `n`: `fn name(node: int, param: T)` runs on the first build after the element's first layout
    /// (so it can measure), `name_update(node, param)` when the parameter changes, `name_destroy(node)` once the element is gone.
    fn action(&mut self, a: &Attr, an: &str, n: &str) -> R<()> {
        let (lo, hi) = a.range;
        let Some(f) = self.c.sc.fns.iter().find(|f| f.name == an) else {
            let help = if an.contains('.') { "a dotted module function isn't supported: wrap it in a script `fn`".to_string() } else { format!("add `fn {an}(node: int, param: T) {{ ... }}` to the script") };
            return Err(UiError { lo, hi, msg: format!("`use:{an}` needs a script fn `{an}`"), help: Some(help) });
        };
        let src = self.src;
        let tys = |f: &markup::FnDef| -> Vec<String> {
            let t = src[f.params.0..f.params.1].trim();
            if t.is_empty() { return Vec::new(); }
            t.split(',').map(|x| x.split(':').nth(1).unwrap_or("").trim().to_string()).collect()
        };
        let want = if matches!(a.value, Value::None) { 1 } else { 2 };
        let t = tys(f);
        let sig = if want == 1 { format!("fn {an}(node: int)") } else { format!("fn {an}(node: int, param: T)") };
        if t.len() != want || t[0] != "int" || f.ret.is_some() {
            return Err(UiError { lo: f.name_r.0, hi: f.name_r.1, msg: format!("`use:{an}` calls `{sig}`, with no return type"), help: Some("the first parameter is the element's handle".into()) });
        }
        let ty = if want == 2 {
            if !EV_TYPES.contains(&t[1].as_str()) {
                return Err(UiError { lo: f.name_r.0, hi: f.name_r.1, msg: format!("the parameter of `{an}` is `{}`, but a `use:` parameter is a plain value", t[1]), help: Some(format!("types: {}", EV_TYPES.join(", "))) });
            }
            Some(t[1].clone())
        } else {
            None
        };
        let upn = format!("{an}_update");
        let update = self.c.sc.fns.iter().find(|f| f.name == upn);
        if let Some(u) = update {
            if tys(u) != t || u.ret.is_some() {
                return Err(UiError { lo: u.name_r.0, hi: u.name_r.1, msg: format!("`{upn}` must take the same parameters as `{an}`: `{}`", sig.replace(an, &upn)), help: None });
            }
            if want == 1 {
                return Err(UiError { lo: u.name_r.0, hi: u.name_r.1, msg: format!("`{upn}` never runs: `use:{an}` has no parameter to change"), help: None });
            }
        }
        let dn = format!("{an}_destroy");
        let destroy = self.c.sc.fns.iter().find(|f| f.name == dn);
        if let Some(u) = destroy {
            if tys(u) != ["int"] || u.ret.is_some() {
                return Err(UiError { lo: u.name_r.0, hi: u.name_r.1, msg: format!("`{dn}` must be `fn {dn}(node: int)`"), help: None });
            }
        }
        let k = self.nk();
        let o = self.owner();
        let h = format!("uh{k}");
        self.upd.line(&format!("let {h} = d.handle({n});"));
        if let (Some(ty), Value::Expr(r)) = (&ty, &a.value) {
            self.upd.pad();
            self.upd.s(&format!("let pv{k}: {ty} = "));
            self.expr(Mode::U, *r)?;
            self.upd.s(";\n");
        }
        let args = if ty.is_some() { format!("{h}, pv{k}") } else { h.clone() };
        let gone = if destroy.is_some() { format!("if {o}.us{k} == 1 {{ this.{dn}({o}.ua{k}); }} ") } else { String::new() };
        self.upd.line(&format!("if {o}.ua{k} != {h} {{ {gone}{o}.ua{k} = {h}; {o}.us{k} = 0; }}"));
        let keep = match ty.as_deref() { Some("string") => format!("{o}.up{k} = d.strPut({o}.up{k}, pv{k}); {o}.ul{k} = pv{k}.len; "), Some(_) => format!("{o}.up{k} = pv{k}; "), None => String::new() };
        self.upd.line(&format!("else if {o}.us{k} == 0 {{ {o}.us{k} = 1; {keep}this.{an}({args}); }}"));
        if update.is_some() {
            let diff = if ty.as_deref() == Some("string") { format!("!d.strHas({o}.up{k}, {o}.ul{k}, pv{k})") } else { format!("{o}.up{k} != pv{k}") };
            self.upd.line(&format!("else if {diff} {{ {keep}this.{upn}({args}); }}"));
        }
        self.acts.push(ActDef { k, container: self.own.last().copied().unwrap_or(0), name: an.to_string(), ty, destroy: destroy.is_some() });
        Ok(())
    }

    /// End of a container's build (`who` is `this` or the row): run `name_destroy` for the `use:`s of container `cont` whose element is
    /// gone, and put `bind:this` places whose element went back to -1.
    fn act_sweep(&mut self, cont: usize, who: &str) -> R<()> {
        for i in 0..self.acts.len() {
            let a = &self.acts[i];
            if a.container != cont {
                continue;
            }
            let k = a.k;
            let call = if a.destroy { format!("if {who}.us{k} == 1 {{ this.{}_destroy({who}.ua{k}); }} ", a.name) } else { String::new() };
            self.upd.line(&format!("if {who}.ua{k} >= 0 && !d.alive({who}.ua{k}) {{ {call}{who}.ua{k} = -1; {who}.us{k} = 0; }}"));
        }
        // a place something else has written since is left alone
        let mine: Vec<(usize, Range)> = self.binds.iter().filter(|b| b.container == cont).map(|b| (b.k, b.place)).collect();
        for (k, r) in mine {
            self.upd.pad();
            self.upd.s(&format!("if {who}.bh{k} >= 0 && !d.alive({who}.bh{k}) {{ if "));
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" == {who}.bh{k} {{ "));
            self.expr(Mode::U, r)?;
            self.upd.s(&format!(" = -1; }} {who}.bh{k} = -1; }}\n"));
        }
        Ok(())
    }

    /// `bind:` takes a place: a name, then `.field` and `[index]` steps.
    fn place_check(&self, r: Range) -> R<()> {
        let bad = || err(r.0, r.1, "`bind:` needs a place: a name, a field, or an index").map_err(|e| e.help(format!("found `{}`", &self.src[r.0..r.1])));
        let toks = lex(&self.src[r.0..r.1], FileId(0)).map_err(|e| UiError { lo: r.0 + e.span.start as usize, hi: r.0 + e.span.end as usize, msg: e.message, help: None })?;
        let mut k = 0;
        if !matches!(toks.first().map(|t| t.kind), Some(TokenKind::Ident) | Some(TokenKind::This)) {
            return bad();
        }
        k += 1;
        while toks[k].kind != TokenKind::Eof {
            match toks[k].kind {
                TokenKind::Dot if toks[k + 1].kind == TokenKind::Ident => k += 2,
                TokenKind::LBracket => {
                    let mut depth = 0;
                    while toks[k].kind != TokenKind::Eof {
                        match toks[k].kind {
                            TokenKind::LBracket => depth += 1,
                            TokenKind::RBracket => {
                                depth -= 1;
                                if depth == 0 {
                                    break;
                                }
                            }
                            _ => {}
                        }
                        k += 1;
                    }
                    if toks[k].kind == TokenKind::Eof {
                        return bad();
                    }
                    k += 1;
                }
                _ => return bad(),
            }
        }
        Ok(())
    }

    fn comp(&mut self, e: &Elem, slot: String, depth: usize) -> R<()> {
        let Some(child) = self.all.iter().find(|c| c.name == e.tag) else {
            let known = self.all.iter().map(|c| c.name.clone()).collect::<Vec<_>>().join(", ");
            return Err(UiError { lo: e.open.0, hi: e.open.0 + 1 + e.tag.len(), msg: format!("unknown component `<{}>`", e.tag), help: Some(format!("known components: {known}")) });
        };
        let id = e.id.get();
        self.insts.push((id, child.id.clone(), self.own.last().copied().unwrap_or(0)));
        let inst = format!("{}.c{id}", self.owner());
        // validate attributes
        let cprops: Vec<&str> = child.sc.props.iter().filter(|p| p.hidden.is_none()).map(|p| p.name.as_str()).collect();
        let cevents: Vec<&str> = child.sc.events.iter().map(|p| p.0.as_str()).collect();
        for a in &e.attrs {
            if (a.name == "slot" && self.slots.slotted.contains(&addr(e))) || pass::is_pass(a) {
                continue; // `class` / `style:` go to the child's root element (`pass`)
            }
            if let Some(ev) = a.name.strip_prefix("on:") {
                if !cevents.contains(&ev) {
                    let help = if cevents.is_empty() { format!("`{}` declares no events", e.tag) } else { format!("`{}` events: {}", e.tag, cevents.join(", ")) };
                    return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`<{}>` has no event `{ev}`", e.tag), help: Some(help) });
                }
            } else if a.name == "animate:flip" || a.name == "animate-easing" {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`{}` goes on the row element, not a component (`<{}>`)", a.name, e.tag), help: Some("wrap it: `{#each xs as x (x.id)}<div animate:flip><Row .../></div>{/each}`".into()) });
            } else if a.name == "bind:this" {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: "`bind:this` on a component isn't supported yet".into(), help: Some("put `bind:this` on an element inside it, or on a wrapper `<div>` around the component".into()) });
            } else if a.name.contains(':') {
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("`{}` isn't supported on a component", a.name), help: Some("only props and `on:event` handlers go on `<Component>`".into()) });
            } else if !cprops.contains(&a.name.as_str()) {
                let help = if cprops.is_empty() { format!("`{}` takes no props", e.tag) } else { format!("`{}` props: {}", e.tag, cprops.join(", ")) };
                return Err(UiError { lo: a.range.0, hi: a.range.1, msg: format!("unknown prop `{}` for `<{}>`", a.name, e.tag), help: Some(help) });
            }
        }
        for p in child.sc.props.iter().filter(|p| p.hidden.is_none() && p.default.is_none()).map(|p| &p.name) {
            if !e.attrs.iter().any(|a| a.name == *p) {
                return Err(UiError { lo: e.open.0, hi: e.open.1, msg: format!("missing prop `{p}` for `<{}>`", e.tag), help: Some(format!("`{}` props: {}", e.tag, cprops.join(", "))) });
            }
        }
        let site = self.slots.by_addr.get(&addr(e)).copied();
        if let Some(si) = site {
            let slots = self.slots;
            for name in &slots.list[si].names {
                let nodes: Vec<&Node> = e.children.iter().filter(|n| slot_of(n) == *name).collect();
                self.slot_method(slots.list[si].id, name, &nodes, depth, e.open)?;
            }
        }
        let k = self.nk();
        self.upd.set(self.fi, e.open.0, e.open.1);
        self.upd.line(&format!("let s{k} = d.enter({slot});"));
        if let Some(si) = site {
            let iv = self.idx.last().cloned().unwrap_or_else(|| "0".into());
            self.upd.line(&format!("{inst}.slotSite = {};", self.slots.list[si].id));
            self.upd.line(&format!("{inst}.slotOwn = this as int;"));
            self.upd.line(&format!("{inst}.slotI = {iv};"));
        }
        self.upd.pad();
        self.upd.s(&format!("let e{k} = {inst}.update(d, {}Props {{ ", child.id));
        for (i, p) in child.sc.props.iter().enumerate() {
            if i > 0 {
                self.upd.s(", ");
            }
            if p.hidden.is_some() {
                // `provide`d by this component (a `let` or a prop) or handed down to it
                self.upd.set(self.fi, e.open.0, e.open.1);
                if self.names.lets.contains(&p.name) {
                    self.upd.s(&format!("{0}: this.{0}", p.name));
                } else {
                    self.used_u.insert(p.name.clone());
                    self.upd.s(&format!("{0}: {0}", p.name));
                }
                continue;
            }
            let Some(a) = e.attrs.iter().find(|a| a.name == p.name) else {
                // omitted: the child's default, a constant, sent every frame like any prop (the child's `const`s renamed)
                let r = p.default.expect("required props are checked above");
                let names = Names { consts: child.sc.consts.iter().map(|l| (l.name.clone(), format!("{}_{}", child.id, l.name))).collect(), ..Names::default() };
                let rw = Rw { src: &self.files[child.fi].text, fi: child.fi, names: &names, locals: &[], rename: &[], emit_ok: false, sev: false, calls_emit: false };
                self.upd.s(&format!("{}: ", p.name));
                rewrite(&rw, &mut self.upd, &mut BTreeSet::new(), r.0, r.1, false)?;
                continue;
            };
            self.upd.set(self.fi, a.range.0, a.range.1);
            self.upd.s(&format!("{}: ", p.name));
            match &a.value {
                Value::Expr(r) => self.expr(Mode::U, *r)?,
                Value::Str(parts) => self.string(Mode::U, parts, a.range)?,
                Value::None => self.upd.s("true"),
            }
        }
        self.upd.set(self.fi, e.open.0, e.open.1);
        if let Some(pi) = self.slots.pass.get(&child.name).copied() {
            self.pass_args(e, &pi, !child.sc.props.is_empty())?;
        }
        self.upd.s(" });\n");
        self.upd.line(&format!("d.leave(s{k});"));
        for a in &e.attrs {
            if let Some(ev) = a.name.strip_prefix("on:") {
                let Value::Expr(r) = a.value else {
                    return err(a.range.0, a.range.1, format!("`{}` needs `={{ statements }}`", a.name));
                };
                self.upd.set(self.fi, a.range.0, a.range.1);
                self.upd.line(&format!("if e{k}.{ev} {{"));
                self.upd.ind += 1;
                let nren = self.rename.len();
                let ps: Vec<String> = child.sc.ev_params.iter().filter(|p| p.ev == ev).map(|p| format!("{0}: e{k}.{ev}_{0}", p.name)).collect();
                if !ps.is_empty() {
                    self.upd.line(&format!("let evt{k}_{ev} = {}Event_{ev} {{ {} }};", child.id, ps.join(", ")));
                    self.rename.push(("event".into(), format!("evt{k}_{ev}")));
                }
                self.stmts(Mode::U, r, true)?;
                self.rename.truncate(nren);
                self.upd.ind -= 1;
                self.upd.line("}");
            }
        }
        self.pnt.set(self.fi, e.open.0, e.open.1);
        self.pnt.line(&format!("let s{k} = d.enter({slot});"));
        if site.is_some() {
            self.pnt.line(&format!("{inst}.slotOwn = this as int;"));
        }
        self.pnt.line(&format!("{inst}.paint(d);"));
        self.pnt.line(&format!("d.leave(s{k});"));
        Ok(())
    }

    /// What the uses of this component passed, applied after the root's own attributes (a passed style wins).
    fn pass_apply(&mut self, e: &Elem, n: &str) {
        let p = self.slots.pass[&self.c.name];
        self.upd.set(self.fi, e.open.0, e.open.1);
        for l in pass::apply(&p, n) {
            self.upd.line(&l);
        }
    }

    /// The hidden `j3pass_*` fields of the `Props` literal for a use of a component that takes `class` / `style:`:
    /// the parent's scope (only with a class), the class text, and the `style:` values (see `pass`).
    fn pass_args(&mut self, e: &Elem, pi: &Pass, comma: bool) -> R<()> {
        let at = |a: &Attr| a.range;
        let cls: Vec<&Attr> = e.attrs.iter().filter(|a| a.name == "class" || a.name.starts_with("class:")).collect();
        let pre = pass::PREFIX;
        if comma {
            self.upd.s(", ");
        }
        self.upd.s(&format!("{pre}_scope: {}, {pre}_cls: ", if cls.is_empty() { "0".to_string() } else { format!("SCOPE_{}", self.c.id) }));
        let mut parts: Vec<Part> = Vec::new();
        let mut conds: Vec<(&str, markup::Range)> = Vec::new();
        for a in &cls {
            if let Some(c) = a.name.strip_prefix("class:") {
                match a.value {
                    Value::Expr(r) if !c.is_empty() => conds.push((c, r)),
                    _ => return err(a.range.0, a.range.1, format!("`{}` needs a condition: {}={{condition}}", a.name, a.name)),
                }
            } else {
                match &a.value {
                    Value::Str(p) => parts.extend(p.iter().cloned()),
                    Value::None => {}
                    _ => return Err(UiError { lo: a.range.0, hi: a.range.1, msg: "`class` takes text".into(), help: Some("`class=\"a b{x}\"`; switch a class with `class:name={condition}`".into()) }),
                }
            }
        }
        if conds.is_empty() {
            let r = cls.first().map(|a| at(a)).unwrap_or(e.open);
            self.string(Mode::U, &parts, r)?;
        } else {
            // one template: the written words, then each `class:x` as `x` or nothing
            self.upd.s("`");
            for p in &parts {
                match p {
                    Part::Lit(l) if l.contains('`') || l.contains('\n') => return err(e.open.0, e.open.1, "text next to `{...}` can't hold a backtick"),
                    Part::Lit(l) => self.upd.s(l),
                    Part::Hole(r) => {
                        self.upd.s("{");
                        self.rw(Mode::U, r.0, r.1, true, true, false)?;
                        self.upd.s("}");
                    }
                }
            }
            for (c, r) in conds {
                self.upd.s(" {if ");
                self.expr(Mode::U, r)?;
                self.upd.s(&format!(" {{ {} }} else {{ \"\" }}}}", lit(c)));
            }
            self.upd.s("`");
        }
        let styles: Vec<&Attr> = e.attrs.iter().filter(|a| a.name.starts_with("style:")).collect();
        for k in 0..pi.styles {
            self.upd.s(", ");
            let Some(a) = styles.get(k) else {
                self.upd.s(&pass::unused(k));
                continue;
            };
            let prop = &a.name["style:".len()..];
            let num = self.style_val(a, prop)?;
            self.upd.s(&format!("{pre}_n{k}: {}, ", lit(prop)));
            match &a.value {
                Value::Expr(r) if num => {
                    self.upd.s(&format!("{pre}_k{k}: 1, {pre}_f{k}: f32("));
                    self.expr(Mode::U, *r)?;
                    self.upd.s(&format!("), {pre}_s{k}: \"\""));
                }
                Value::Expr(r) => {
                    self.upd.s(&format!("{pre}_k{k}: 2, {pre}_f{k}: 0.0, {pre}_s{k}: "));
                    self.expr(Mode::U, *r)?;
                }
                Value::Str(p) => {
                    self.upd.s(&format!("{pre}_k{k}: 2, {pre}_f{k}: 0.0, {pre}_s{k}: "));
                    self.string(Mode::U, p, a.range)?;
                }
                Value::None => unreachable!("checked by style_val"),
            }
        }
        Ok(())
    }

    /// The content a use of a component passes to one of its slots, as a method pair on this
    /// component (`this` is the owner, so the content sees its props, lets and fns). The child
    /// calls it through `slotOwn`; an enclosing `{#each}` item is re-bound from the index `slotI`.
    fn slot_method(&mut self, site: usize, name: &str, nodes: &[&Node], depth: usize, at: Range) -> R<()> {
        self.owns_slot = true;
        let (mut upd, mut pnt) = (Out::with_ctx(self.fi), Out::with_ctx(self.fi));
        upd.ind = 2;
        pnt.ind = 2;
        let (su, sp) = (std::mem::replace(&mut self.upd, upd), std::mem::replace(&mut self.pnt, pnt));
        let (uu, up) = (std::mem::take(&mut self.used_u), std::mem::take(&mut self.used_p));
        let mut res = Ok(());
        let ectx = self.ectx.clone();
        if self.in_virt > 0 {
            res = err(at.0, at.1, "slot content inside a `virtual` list isn't supported");
        } else if ectx.len() > 1 {
            res = err(at.0, at.1, "slot content inside two nested `{#each}` blocks isn't supported yet");
        }
        for (h, k, eid, rows) in &ectx {
            res = res.and_then(|_| self.rebind(h, *k, *eid, *rows));
        }
        // the `{#let}`s in scope at the use are computed again here (this is another fn)
        for (_, j3, ex) in self.lets.clone() {
            res = res.and_then(|_| {
                self.upd.pad();
                self.upd.s(&format!("let {j3} = "));
                self.expr(Mode::U, ex)?;
                self.upd.s(";\n");
                Ok(())
            });
        }
        let mut j = 0;
        res = res.and_then(|_| self.seq(nodes.iter().copied(), &mut j, depth, &None));
        let (bu, bp) = (std::mem::replace(&mut self.upd, su), std::mem::replace(&mut self.pnt, sp));
        let (used_u, used_p) = (std::mem::replace(&mut self.used_u, uu), std::mem::replace(&mut self.used_p, up));
        res?;
        let iv = self.idx.last().cloned().unwrap_or_else(|| "slotI".into());
        let n = san(if name.is_empty() { "default" } else { name });
        let id = self.c.id.clone();
        for (m, body, used) in [("update", bu, used_u), ("paint", bp, used_p)] {
            let mut o = Out::with_ctx(self.fi);
            o.set(self.fi, at.0, at.1);
            o.s(&format!("\n    fn slot{site}_{n}_{m}(this: &{id}, d: &Dom, {iv}: int) {{\n"));
            if m == "update" && !self.c.sc.events.is_empty() {
                o.s("        let ev = &this.sev;\n");
            }
            for p in &used {
                o.s(&format!("        let {p} = this.p.{p};\n"));
            }
            o.append(body);
            o.s("    }\n");
            self.slot_fns.append(o);
        }
        // the child stopped rendering the slot: the content's state dies with it, as in Svelte
        let (mut dead, mut fresh) = (Vec::new(), Vec::new());
        for n in nodes {
            self.resets(std::slice::from_ref(*n), true, &mut dead);
            self.resets(std::slice::from_ref(*n), false, &mut fresh);
        }
        let mut o = Out::with_ctx(self.fi);
        o.s(&format!("\n    fn slot{site}_{n}_leave(this: &{id}, d: &Dom, {iv}: int) {{\n"));
        for (_, _, eid, rows) in &ectx {
            if *rows {
                o.s(&format!("        let r{eid} = this.e{eid}.items[{iv}];\n"));
            }
        }
        for l in dead.iter().chain(&fresh) {
            o.s(&format!("        {l}\n"));
        }
        o.s("    }\n");
        self.slot_fns.append(o);
        Ok(())
    }

    /// The loop item of an enclosing `{#each}`, from its index (the slot method's parameter).
    fn rebind(&mut self, h: &EachHead, k: usize, eid: usize, rows: bool) -> R<()> {
        let iv = self.idx.last().cloned().unwrap_or_default();
        let item = &h.item.0;
        if rows {
            self.upd.line(&format!("let r{eid} = this.e{eid}.items[{iv}];"));
            self.pnt.line(&format!("let r{eid} = this.e{eid}.items[{iv}];"));
        }
        if let Some((a, _)) = h.range {
            if self.src[a.0..a.1].trim() != "0" {
                self.upd.pad();
                self.upd.s(&format!("let a{k} = "));
                self.expr(Mode::U, a)?;
                self.upd.s(";\n");
                self.upd.line(&format!("let {item} = a{k} + {iv};"));
            }
        } else {
            self.upd.pad();
            self.upd.s(&format!("let x{k} = "));
            self.expr(Mode::U, h.expr)?;
            self.upd.s(";\n");
            self.upd.line(&format!("let {item} = {}x{k}[{iv}];", if h.by_ref { "&" } else { "" }));
        }
        Ok(())
    }

    /// `<slot>` / `<slot name="x">fallback</slot>`: run the content the owner passed, else the fallback.
    fn slot_elem(&mut self, e: &Elem, slot: String, depth: usize) -> R<()> {
        self.has_slot = true;
        let name = e.attrs.iter().find(|a| a.name == "name").and_then(|a| lit_value(&a.value)).unwrap_or_default();
        let k = self.nk();
        let slots = self.slots;
        let n = san(if name.is_empty() { "default" } else { &name });
        // every use of this component that fills this slot
        let calls: Vec<(usize, String)> = slots.list.iter().filter(|s| s.child == self.c.name && s.names.contains(&name)).map(|s| (s.id, s.owner.clone())).collect();
        for m in [Mode::U, Mode::P] {
            let call = if m == Mode::U { "update" } else { "paint" };
            let fi = self.fi;
            let o = self.out(m);
            o.set(fi, e.open.0, e.open.1);
            o.line(&format!("let s{k} = d.enter({slot});"));
            if m == Mode::U && !calls.is_empty() {
                o.line(&format!("this.slotNow{k} = true;"));
            }
            for (i, (id, owner)) in calls.iter().enumerate() {
                o.line(&format!("{}if this.slotSite == {id} {{", if i > 0 { "} else " } else { "" }));
                o.ind += 1;
                o.line(&format!("((this.slotOwn as ptr) as &{owner}).slot{id}_{n}_{call}(d, this.slotI);"));
                o.ind -= 1;
            }
            if !calls.is_empty() {
                o.line("} else {");
                o.ind += 1;
            }
        }
        if !calls.is_empty() {
            self.slot_marks.push((k, n.clone(), calls.clone()));
        }
        let mut j = 0;
        self.seq(&e.children, &mut j, depth, &None)?;
        for m in [Mode::U, Mode::P] {
            let o = self.out(m);
            if !calls.is_empty() {
                o.ind -= 1;
                o.line("}");
            }
            o.line(&format!("d.leave(s{k});"));
        }
        Ok(())
    }

    fn ifnode(&mut self, i: &IfNode, j: &mut usize, depth: usize, base: &Base) -> R<()> {
        let id = i.id.get();
        let stateful = id != 0;
        let (lo, hi) = i.range;
        self.upd.set(self.fi, lo, hi);
        let state;
        let mut bv = String::new();
        // `{#match X}`: X is evaluated once
        let mut mv = String::new();
        if let Some(sc) = i.scrut {
            mv = format!("m{}", self.nk());
            self.upd.pad();
            self.upd.s(&format!("let {mv} = "));
            self.expr(Mode::U, sc)?;
            self.upd.s(";\n");
        }
        if stateful {
            self.ifs.push((id, self.own.last().copied().unwrap_or(0)));
            state = format!("{}.if{id}", self.owner());
            let k = self.nk();
            bv = format!("b{k}");
            self.upd.line(&format!("let {bv} = -1;"));
            for (bi, b) in i.branches.iter().enumerate() {
                self.upd.pad();
                if bi > 0 {
                    self.upd.s("} else ");
                }
                self.branch_head(b, &mv, i.choice.as_deref().map(|set| (set, bi)))?;
                self.upd.ind += 1;
                self.upd.line(&format!("{bv} = {bi};"));
                self.upd.ind -= 1;
            }
            self.upd.line("}");
            self.upd.line(&format!("if {bv} != {state} {{"));
            self.upd.ind += 1;
            // leaving a branch destroys its components (onDestroy), entering one starts them fresh
            let mut first = true;
            for (bi, b) in i.branches.iter().enumerate() {
                let mut dead = Vec::new();
                self.resets(&b.nodes, true, &mut dead);
                if dead.is_empty() {
                    continue;
                }
                self.upd.line(&format!("{}if {state} == {bi} {{", if first { "" } else { "} else " }));
                first = false;
                self.upd.ind += 1;
                for r in dead {
                    self.upd.line(&r);
                }
                self.upd.ind -= 1;
            }
            if !first {
                self.upd.line("}");
            }
            self.upd.line(&format!("{state} = {bv};"));
            let mut first = true;
            for (bi, b) in i.branches.iter().enumerate() {
                let mut resets = Vec::new();
                self.resets(&b.nodes, false, &mut resets);
                if resets.is_empty() {
                    continue;
                }
                let pre = if first { "" } else { "} else " };
                first = false;
                self.upd.line(&format!("{pre}if {bv} == {bi} {{"));
                self.upd.ind += 1;
                for r in resets {
                    self.upd.line(&r);
                }
                self.upd.ind -= 1;
            }
            if !first {
                self.upd.line("}");
            }
            self.upd.ind -= 1;
            self.upd.line("}");
        }
        for (bi, b) in i.branches.iter().enumerate() {
            self.upd.set(self.fi, lo, hi);
            self.upd.pad();
            if bi > 0 {
                self.upd.s("} else ");
            }
            if stateful {
                self.upd.s(&format!("if {bv} == {bi} {{\n"));
            } else {
                self.branch_head(b, &mv, i.choice.as_deref().map(|set| (set, bi)))?;
            }
            self.upd.ind += 1;
            self.seq(&b.nodes, j, depth, base)?;
            self.upd.ind -= 1;
        }
        self.upd.set(self.fi, lo, hi);
        self.upd.line("}");
        Ok(())
    }

    /// `if cond {`, or for a `{#match}` case `if m == a || m == b {`; an else branch opens a bare `{`.
    fn branch_head(&mut self, b: &markup::Branch, mv: &str, choice: Option<(&str, usize)>) -> R<()> {
        if let Some((set, index)) = choice {
            self.upd.s(&format!("if @uiChoice({set}, {mv}, {index})"));
        } else if let Some(c) = b.cond {
            self.upd.s("if ");
            self.expr(Mode::U, c)?;
        } else if !b.vals.is_empty() {
            self.upd.s("if ");
            for (q, v) in b.vals.iter().enumerate() {
                if q > 0 {
                    self.upd.s(" || ");
                }
                self.upd.s(&format!("{mv} == "));
                self.expr(Mode::U, *v)?;
            }
        } else {
            self.upd.s("{\n");
            return Ok(());
        }
        self.upd.s(" {\n");
        Ok(())
    }

    /// Statements that reset what a branch holds when it is entered (Svelte destroys it), or with
    /// `destroy` the `destroy()` calls for its components when it is left. The branch's own level only:
    /// rows of a list in it are destroyed (and dropped) as a whole.
    fn resets(&self, nodes: &[Node], destroy: bool, out: &mut Vec<String>) {
        let own = self.owner();
        for n in nodes {
            match n {
                Node::Text { .. } | Node::Let(_) => {}
                Node::Elem(e) => {
                    if e.comp {
                        let id = e.id.get();
                        if destroy {
                            out.push(format!("{own}.c{id}.destroy();"));
                        } else {
                            out.push(format!("{own}.c{id}.j3Reset();"));
                        }
                    }
                    self.resets(&e.children, destroy, out);
                }
                Node::If(i) => {
                    let id = i.id.get();
                    if id != 0 && !destroy {
                        out.push(format!("{own}.if{id} = -1;"));
                    }
                    for b in &i.branches {
                        self.resets(&b.nodes, destroy, out);
                    }
                }
                Node::Each(e) => {
                    let id = e.id.get();
                    let Ok(h) = parse_each_head(self.src, e.head) else { continue };
                    if h.key.is_none() && !holds(&e.body) {
                        continue;
                    }
                    if destroy {
                        out.push(format!("for q{id} in 0..{own}.e{id}.len {{ {own}.e{id}.items[q{id}].destroy(); }}"));
                    } else {
                        out.push(format!("for q{id} in 0..{own}.e{id}.len {{ {own}.fp{id}.push({own}.e{id}.items[q{id}]); }}"));
                        out.push(format!("{own}.e{id}.len = 0;"));
                        if h.key.is_some() {
                            out.push(format!("{own}.kd{id}.reset();"));
                        }
                    }
                }
            }
        }
    }

    /// The struct that holds the state of what is being built: the component, or the `{#each}` row.
    fn owner(&self) -> String {
        match self.own.last() {
            Some(id) => format!("r{id}"),
            None => "this".into(),
        }
    }

    fn each(&mut self, e: &EachNode, slot: String, depth: usize) -> R<()> {
        let src = self.src;
        let lo = e.range.0;
        let head = parse_each_head(src, e.head)?;
        self.check_local(&head.item.0, head.item.1)?;
        if let Some((n, r)) = &head.index {
            self.check_local(n, *r)?;
        }
        if head.by_ref && head.range.is_some() {
            return err(head.item.1 .0 - 1, head.item.1 .1, "a range loop gives numbers; `&x` is for a list of structs").map_err(|e| e.help("`{#each items as &item}`"));
        }
        if head.range.is_some() && head.index.is_some() {
            let r = head.index.as_ref().unwrap().1;
            return Err(UiError { lo: r.0, hi: r.1, msg: "a range loop has no index".into(), help: Some("the item is the index: `{#each 0..n as i}`".into()) });
        }
        let id = e.id.get();
        let keyed = head.key.is_some();
        let rows = keyed || holds(&e.body);
        let own = self.owner();
        let virt = self.virt.take();
        self.eachs.push(EachDef { id, container: self.own.last().copied().unwrap_or(0), keyed, rows, virt: virt.is_some() });
        self.uses_virt |= virt.is_some();
        self.uses_list |= rows;
        self.uses_keyed |= keyed;
        let k = self.nk();
        let sv = format!("s{k}");
        let mv = format!("m{k}");
        self.upd.set(self.fi, lo, e.head.1);
        self.upd.line(&format!("let {sv} = d.enter({slot});"));
        let item = head.item.0.clone();
        let amp = if head.by_ref { "&" } else { "" };
        let mut bind: Option<String> = None;
        // the iteration variable and where `mv` comes from
        let iv: String;
        if let Some((a, b)) = head.range {
            let zero = src[a.0..a.1].trim() == "0";
            self.upd.pad();
            if zero {
                self.upd.s(&format!("let {mv} = "));
                self.expr(Mode::U, b)?;
                self.upd.s(";\n");
                iv = item.clone();
            } else {
                self.upd.s(&format!("let a{k} = "));
                self.expr(Mode::U, a)?;
                self.upd.s(";\n");
                self.upd.pad();
                self.upd.s(&format!("let {mv} = "));
                self.expr(Mode::U, b)?;
                self.upd.s(&format!(" - a{k};\n"));
                iv = format!("i{k}");
                bind = Some(format!("let {item} = a{k} + {iv};"));
            }
        } else {
            self.upd.pad();
            self.upd.s(&format!("let x{k} = "));
            self.expr(Mode::U, head.expr)?;
            self.upd.s(";\n");
            if let Some(va) = &virt {
                // the rows alive (`lo..hi` plus kept ones); the box pads for the rest, and `vl.at(i)` is a row's index
                let vl = format!("{own}.vl{id}");
                self.upd.pad();
                self.upd.s(&format!("{vl}.cfg(f32("));
                self.virt_num(&va.rh, "0.0")?;
                self.upd.s("), f32(");
                self.virt_num(&va.est, "40.0")?;
                self.upd.s(&format!("), {}, {});\n", va.stick, va.keep.is_some()));
                if let Some(r) = va.keep {
                    if head.key_int {
                        return err(r.0, r.1, "`keep` lists text keys, but this `{#each}` has an int key (`: int`)").map_err(|x| x.help("drop `: int` from the key, or drop `keep`"));
                    }
                    self.upd.pad();
                    self.upd.s(&format!("{vl}.keep = "));
                    self.expr(Mode::U, r)?;
                    self.upd.s(";\n");
                }
                if let Some(r) = va.at_end {
                    self.upd.pad();
                    self.upd.s("if ");
                    self.expr(Mode::U, r)?;
                    self.upd.s(&format!(" && !{vl}.atEnd {{ {vl}.toEnd = true; }}\n"));
                }
                {
                    // the list changed: carry the table over. A list that only grew at its tail (checked on its
                    // first and last old row) formats just the new keys; anything else formats every key once.
                    let key = head.key.expect("virtual lists are keyed");
                    let ki_ = head.key_int;
                    let at = src[..lo].matches('\n').count() + 1;
                    let item_n = item.clone();
                    let index_n = head.index.as_ref().map(|x| x.0.clone());
                    self.upd.line(&format!("if {vl}.stale(x{k}.len) {{"));
                    self.upd.ind += 1;
                    self.upd.line(&format!("let ap{k} = {vl}.grew(x{k}.len);"));
                    self.upd.line(&format!("if ap{k} {{"));
                    self.upd.ind += 1;
                    self.upd.line(&format!("{vl}.pk.begin();"));
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.pk.add"), "0", &format!("{k}a"))?;
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.pk.add"), &format!("{vl}.n - 1"), &format!("{k}b"))?;
                    self.upd.line(&format!("ap{k} = {vl}.probeOk();"));
                    self.upd.ind -= 1;
                    self.upd.line("}");
                    self.upd.line(&format!("if ap{k} {{"));
                    self.upd.ind += 1;
                    self.upd.line(&format!("for kq{k} in {vl}.n..x{k}.len {{"));
                    self.upd.ind += 1;
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.all.push"), &format!("kq{k}"), &format!("{k}c"))?;
                    self.upd.ind -= 1;
                    self.upd.set(self.fi, lo, e.head.1);
                    self.upd.line("}");
                    self.upd.line(&format!("{vl}.appended(x{k}.len);"));
                    self.upd.ind -= 1;
                    self.upd.line("} else {");
                    self.upd.ind += 1;
                    self.upd.line(&format!("{vl}.all.begin();"));
                    self.upd.line(&format!("for kq{k} in 0..x{k}.len {{"));
                    self.upd.ind += 1;
                    self.virt_key(&item_n, &index_n, key, ki_, &format!("{vl}.all.add"), &format!("kq{k}"), &format!("{k}d"))?;
                    self.upd.ind -= 1;
                    self.upd.set(self.fi, lo, e.head.1);
                    self.upd.line("}");
                    self.upd.line(&format!("{vl}.all.finish({});", lit(&format!("{}:{at}", self.files[self.fi].path))));
                    self.upd.line(&format!("{vl}.rebuilt(x{k}.len);"));
                    self.upd.ind -= 1;
                    self.upd.line("}");
                    self.upd.ind -= 1;
                    self.upd.line("}");
                }
                self.upd.line(&format!("let w{k} = vlist.step(d, {}, &{vl}, x{k}.len);", va.n));
                self.upd.line(&format!("let {mv} = {vl}.cnt;"));
                if let Some(r) = va.at_end {
                    self.upd.pad();
                    self.expr(Mode::U, r)?;
                    self.upd.s(&format!(" = {vl}.atEnd;\n"));
                }
                iv = format!("i{k}");
                bind = Some(format!("let ax{k} = {vl}.at({iv}); let {item} = {amp}x{k}[ax{k}];"));
                if let Some((x, _)) = &head.index {
                    bind = Some(format!("let ax{k} = {vl}.at({iv}); let {item} = {amp}x{k}[ax{k}]; let {x} = ax{k};"));
                }
            } else {
                self.upd.line(&format!("let {mv} = x{k}.len;"));
                iv = head.index.as_ref().map(|x| x.0.clone()).unwrap_or_else(|| format!("i{k}"));
                bind = Some(format!("let {item} = {amp}x{k}[{iv}];"));
            }
        }
        self.upd.line(&format!("{own}.cnt{id} = {mv};"));
        let saved = self.locals.len();
        let rn = format!("{}_R{id}", self.c.id);
        let er = format!("{own}.e{id}");
        let parent = self.gstk.last().cloned().unwrap_or_else(|| "-1".into());
        if let Some(key) = head.key {
            // match this frame's keys to last frame's rows, then reorder the rows to match
            let kd = format!("{own}.kd{id}");
            let at = src[..lo].matches('\n').count() + 1;
            // the key sees the item under other names (the body binds it again below)
            let (kv, ki) = (format!("kv{k}"), format!("ki{k}"));
            let nren = self.rename.len();
            self.upd.line(&format!("{kd}.begin();"));
            self.upd.line(&format!("for {kv} in 0..{mv} {{"));
            self.upd.ind += 1;
            if let Some((a, _)) = head.range {
                if src[a.0..a.1].trim() == "0" {
                    self.rename.push((item.clone(), kv.clone()));
                } else {
                    self.upd.line(&format!("let {ki} = a{k} + {kv};"));
                    self.rename.push((item.clone(), ki));
                }
            } else {
                let at = if virt.is_some() { format!("kj{k}") } else { kv.clone() };
                if virt.is_some() {
                    self.upd.line(&format!("let kj{k} = {own}.vl{id}.at({kv});"));
                }
                self.upd.line(&format!("let {ki} = x{k}[{at}];"));
                self.rename.push((item.clone(), ki));
                if let Some((n, _)) = &head.index {
                    self.rename.push((n.clone(), if virt.is_some() { at } else { kv }));
                }
            }
            self.upd.set(self.fi, key.0, key.1);
            self.upd.pad();
            self.upd.s(&if head.key_int { format!("{kd}.addInt(") } else { format!("{kd}.add(`{{") });
            let r = self.expr(Mode::U, key);
            self.upd.s(if head.key_int { ");\n" } else { "}`);\n" });
            self.rename.truncate(nren);
            r?;
            self.upd.ind -= 1;
            self.upd.set(self.fi, lo, e.head.1);
            self.upd.line("}");
            self.upd.line(&format!("{kd}.finish({});", lit(&format!("{}:{at}", self.files[self.fi].path))));
            if virt.is_some() {
                self.upd.line(&format!("{own}.vl{id}.verify(&{kd});"));
            }
            self.upd.line(&format!("if {kd}.changed {{"));
            self.upd.ind += 1;
            self.upd.line(&format!("for dq{k} in 0..{kd}.dead.len {{ let dr{k} = {er}.items[{kd}.dead.items[dq{k}]]; dr{k}.destroy(); {own}.fp{id}.push(dr{k}); }}"));
            self.upd.line(&format!("{own}.eb{id}.len = 0;"));
            self.upd.line(&format!("for rq{k} in 0..{mv} {{"));
            self.upd.ind += 1;
            self.upd.line(&format!("let f{k} = {kd}.from.items[rq{k}];"));
            self.upd.line(&format!("if f{k} >= 0 {{ {own}.eb{id}.push({er}.items[f{k}]); }} else {{ {own}.eb{id}.push({rn}.take(&{own}.fp{id}, this.mem, d, d.groupNew({parent}), this as int)); }}"));
            self.upd.ind -= 1;
            self.upd.line("}");
            self.upd.line(&format!("let t{k} = {er};"));
            self.upd.line(&format!("{er} = {own}.eb{id};"));
            self.upd.line(&format!("{own}.eb{id} = t{k};"));
            self.upd.ind -= 1;
            self.upd.line("}");
        } else if rows {
            self.upd.line(&format!("while {er}.len < {mv} {{ {er}.push({rn}.take(&{own}.fp{id}, this.mem, d, -1, this as int)); }}"));
            self.upd.line(&format!("if {er}.len > {mv} {{"));
            self.upd.ind += 1;
            self.upd.line(&format!("for zq{k} in {mv}..{er}.len {{ {er}.items[zq{k}].destroy(); {own}.fp{id}.push({er}.items[zq{k}]); }}"));
            self.upd.line(&format!("{er}.len = {mv};"));
            self.upd.ind -= 1;
            self.upd.line("}");
        }
        self.locals.push(item);
        if let Some((n, _)) = &head.index {
            self.locals.push(n.clone());
        }
        self.upd.line(&format!("for {iv} in 0..{mv} {{"));
        self.upd.ind += 1;
        if let Some(b) = &bind {
            self.upd.line(b);
        }
        if rows {
            self.upd.line(&format!("let r{id} = {er}.items[{iv}];"));
        }
        if keyed {
            self.upd.line(&format!("d.keyBegin(r{id}.uid);"));
        }
        self.pnt.set(self.fi, lo, e.head.1);
        self.pnt.line(&format!("let {sv} = d.enter({slot});"));
        self.pnt.line(&format!("let {mv} = {own}.cnt{id};"));
        self.pnt.line(&format!("for {iv} in 0..{mv} {{"));
        self.pnt.ind += 1;
        if rows {
            self.pnt.line(&format!("let r{id} = {er}.items[{iv}];"));
        }
        if keyed {
            self.pnt.line(&format!("d.paintGroup(r{id}.uid);"));
        }
        if rows {
            self.own.push(id);
        }
        if keyed {
            self.gstk.push(format!("r{id}.uid"));
        }
        self.idx.push(iv.clone());
        self.ectx.push((head.clone(), k, id, rows));
        // a keyed row's nodes are told apart by the row (`keyBegin`), not by an index in the slot
        let base = if keyed { None } else { Some((iv.clone(), width(&e.body))) };
        let mut j = 0;
        self.in_virt += virt.is_some() as usize;
        if keyed && virt.is_none() {
            row_elems(&e.body, &mut self.flip_rows);
        }
        if rows {
            self.bases.push((self.locals.len(), self.lets.len()));
        }
        let mut res = self.seq(&e.body, &mut j, depth + 1, &base);
        if rows {
            self.bases.pop();
            if res.is_ok() {
                res = self.act_sweep(id, &format!("r{id}"));
            }
        }
        self.in_virt -= virt.is_some() as usize;
        self.ectx.pop();
        self.idx.pop();
        if keyed {
            self.gstk.pop();
        }
        if rows {
            self.own.pop();
        }
        self.locals.truncate(saved);
        res?;
        if keyed {
            self.upd.line("d.keyEnd();");
            self.pnt.line("d.paintGroupEnd();");
        }
        self.upd.ind -= 1;
        self.upd.set(self.fi, lo, e.head.1);
        self.upd.line("}");
        if let Some(va) = &virt {
            // kept rows outside the window are hidden now that the rows are built
            self.upd.line(&format!("vlist.finish(d, {}, &{own}.vl{id});", va.n));
        }
        self.upd.line(&format!("d.leave({sv});"));
        self.pnt.ind -= 1;
        self.pnt.line("}");
        self.pnt.line(&format!("d.leave({sv});"));
        Ok(())
    }

    // ---- assembly

    fn assemble(&mut self, fns: Out) -> R<Out> {
        let c = self.c;
        let name = c.id.clone();
        let src = self.src;
        let fi = self.fi;
        let mut o = Out::with_ctx(fi);
        o.s(&format!("// ---- {}\n", self.files[fi].path));
        o.s(&format!("pub const SCOPE_{name}: int = {};\n", c.scope));
        if let Some((a, b)) = c.mk.style {
            o.set(fi, a, b);
            o.s(&format!("const CSS_{name}: string = {};\n", lit(src[a..b].trim_matches('\n'))));
            // lines above the trimmed style text: `file:line` of rules and warnings are the `.j3ui`'s own
            let top = a + (src[a..b].len() - src[a..b].trim_start_matches('\n').len());
            o.s(&format!("const LINE0_{name}: int = {};\n", src[..top].matches('\n').count()));
        } else {
            o.s(&format!("const CSS_{name}: string = \"\";\nconst LINE0_{name}: int = 0;\n"));
        }
        o.set(fi, 0, 0);
        o.s(&format!("\npub struct {name}Props {{\n"));
        for p in &c.sc.props {
            o.set(fi, p.name_r.0, p.name_r.1);
            o.s(&format!("    {}: ", p.name));
            match &p.hidden {
                Some(t) => o.s(t),
                None => o.copy(fi, src, p.ty.0, p.ty.1),
            }
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        if let Some(pi) = self.slots.pass.get(&c.name) {
            o.s(&pass::fields(pi));
        }
        o.s(&format!("}}\n\npub struct {name}Events {{\n"));
        for (e, r) in &c.sc.events {
            o.set(fi, r.0, r.1);
            o.s(&format!("    {e}: bool;\n"));
            for p in c.sc.ev_params.iter().filter(|p| p.ev == *e) {
                o.set(fi, p.name_r.0, p.name_r.1);
                o.s(&format!("    {e}_{}: ", p.name));
                o.copy(fi, src, p.ty.0, p.ty.1);
                o.s(";\n");
            }
        }
        o.set(fi, 0, 0);
        o.s("}\n\n");
        // `const`: a literal is a module const; anything else (an array) is a singleton filled by `styles`
        // (J3 consts hold scalars only), which runs before the first `update`
        for l in &c.sc.consts {
            let ty = match (l.ty, l.guess) {
                (Some(t), _) => src[t.0..t.1].to_string(),
                (None, Some(g)) => g.to_string(),
                (None, None) => return Err(UiError { lo: l.name_r.0, hi: l.name_r.1, msg: format!("can't tell the type of `const {}`", l.name), help: Some(format!("write it out: `const {}: Type = ...;`", l.name)) }),
            };
            o.set(fi, l.name_r.0, l.name_r.1);
            if l.guess.is_some() {
                o.s(&format!("const {name}_{}: {ty} = ", l.name));
                o.copy(fi, src, l.init.0, l.init.1);
                o.s(";\n\n");
            } else {
                o.s(&format!("#[singleton]\nlet {name}_{}: {ty};\n\n", l.name));
            }
        }
        o.set(fi, 0, 0);
        // what `on:name` sees as `event`
        for (e, _) in c.sc.events.iter().filter(|e| c.sc.ev_params.iter().any(|p| p.ev == e.0)) {
            o.s(&format!("pub struct {name}Event_{e} {{\n"));
            for p in c.sc.ev_params.iter().filter(|p| p.ev == *e) {
                o.set(fi, p.name_r.0, p.name_r.1);
                o.s(&format!("    {}: ", p.name));
                o.copy(fi, src, p.ty.0, p.ty.1);
                o.s(";\n");
            }
            o.set(fi, 0, 0);
            o.s("}\n\n");
        }
        for ed in self.eachs.iter().filter(|e| e.rows) {
            let id = ed.id;
            o.s(&format!("struct {name}_R{id} {{\n    uid: int;\n    comp: int;\n    dom: &Dom;\n{}", self.state_fields(id)));
            o.s(&format!("\n    fn make(mem: &Arena, d: &Dom, uid: int, comp: int): &{name}_R{id} {{\n        let a = mem.alloc::<{name}_R{id}>(1);\n        a[0] = {name}_R{id} {{\n            uid: uid,\n            comp: comp,\n            dom: d,\n{}        }};\n        return &a[0];\n    }}\n", self.state_inits(id)));
            o.s(&format!("\n    fn reset(this: &{name}_R{id}, uid: int, comp: int) {{\n        this.uid = uid;\n        this.comp = comp;\n{}    }}\n", self.state_resets(id, "this")));
            o.s(&format!("\n    fn take(pool: &List<&{name}_R{id}>, mem: &Arena, d: &Dom, uid: int, comp: int): &{name}_R{id} {{\n        if pool.len > 0 {{\n            pool.len -= 1;\n            let r = pool.items[pool.len];\n            r.reset(uid, comp);\n            return r;\n        }}\n        return {name}_R{id}.make(mem, d, uid, comp);\n    }}\n"));
            o.s(&format!("\n    fn destroy(this: &{name}_R{id}) {{\n{}        if this.uid >= 0 {{\n            this.dom.groupFree(this.uid);\n            this.uid = -1;\n        }}\n    }}\n}}\n\n", self.state_destroy(id, "this")));
        }
        o.s(&format!("pub struct {name} {{\n    mem: &Arena;\n    ready: bool;\n    dm: int;\n    p: &{name}Props;\n"));
        if self.root {
            o.s("    css: &Css;\n    dom: Dom;\n");
        }
        for l in &c.sc.lets {
            o.set(fi, l.name_r.0, l.name_r.1);
            o.s(&format!("    {}: ", l.name));
            match (l.ty, l.guess) {
                (Some(t), _) => o.copy(fi, src, t.0, t.1),
                (None, Some(g)) => o.s(g),
                (None, None) => {
                    return Err(UiError { lo: l.name_r.0, hi: l.name_r.1, msg: format!("can't tell the type of `let {}`", l.name), help: Some(format!("write it out: `let {}: Type = ...;`", l.name)) });
                }
            }
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        for t in &self.tags {
            o.s(&format!("    {}: int;\n", field_name("tag_", &self.tags, t)));
        }
        for t in &self.classes {
            o.s(&format!("    {}: int;\n", field_name("cls_", &self.classes, t)));
        }
        o.s(&self.state_fields(0));
        if self.has_slot {
            o.s("    slotSite: int;\n    slotOwn: int;\n    slotI: int;\n");
            for (k, _, _) in &self.slot_marks {
                o.s(&format!("    slotNow{k}: bool;\n    slotWas{k}: bool;\n"));
            }
        }
        let bridge = (self.owns_slot || !self.names.emitting.is_empty()) && !c.sc.events.is_empty();
        if bridge {
            o.s(&format!("    sev: {name}Events;\n"));
        }
        // new
        o.s(&format!("\n    fn new(mem: &Arena): {name} {{\n        let ps = mem.alloc::<{name}Props>(1);\n"));
        if self.root {
            o.s("        let cs = mem.alloc::<Css>(1);\n        cs[0] = Css.new(mem);\n        let css = &cs[0];\n");
            // the built-in popup sheet goes first, so the theme overrides it
            o.s("        uastyle.load(css);\n");
            if let Some(tp) = self.theme_path {
                o.s(&format!("        parse(css, THEME, {}, 0);\n", lit(tp)));
            }
            o.s(&format!("        {name}.styles(css);\n"));
        }
        o.s(&format!("        return {name} {{\n            mem: mem, ready: false, dm: 0, p: &ps[0],\n"));
        if self.root {
            o.s("            css: css, dom: Dom.new(mem, css),\n");
        }
        for l in &c.sc.lets {
            o.set(fi, l.init.0, l.init.1);
            o.s(&format!("            {}: ", l.name));
            o.copy(fi, src, l.init.0, l.init.1);
            o.s(",\n");
        }
        o.set(fi, 0, 0);
        for t in &self.tags {
            o.s(&format!("            {}: 0,\n", field_name("tag_", &self.tags, t)));
        }
        for t in &self.classes {
            o.s(&format!("            {}: 0,\n", field_name("cls_", &self.classes, t)));
        }
        o.s(&self.state_inits(0));
        if self.has_slot {
            o.s("            slotSite: 0, slotOwn: 0, slotI: 0,\n");
            for (k, _, _) in &self.slot_marks {
                o.s(&format!("            slotNow{k}: false, slotWas{k}: false,\n"));
            }
        }
        if bridge {
            o.s(&format!("            sev: {name}Events {{ {} }},\n", ev_init(&c.sc, src)));
        }
        o.s("        };\n    }\n");
        // reset: `new` again without allocating (a pooled `{#each}` row reuses its components)
        o.s(&format!("\n    fn j3Reset(this: &{name}) {{\n        let mem = this.mem;\n        this.ready = false;\n"));
        for l in &c.sc.lets {
            o.set(fi, l.init.0, l.init.1);
            o.s(&format!("        this.{} = ", l.name));
            o.copy(fi, src, l.init.0, l.init.1);
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        o.s(&self.state_resets(0, "this"));
        if self.has_slot {
            o.s("        this.slotSite = 0;\n        this.slotOwn = 0;\n        this.slotI = 0;\n");
            for (k, _, _) in &self.slot_marks {
                o.s(&format!("        this.slotNow{k} = false;\n        this.slotWas{k} = false;\n"));
            }
        }
        if bridge {
            o.s(&format!("        this.sev = {name}Events {{ {} }};\n", ev_init(&c.sc, src)));
        }
        o.s("    }\n");
        // styles
        o.s(&format!("\n    fn styles(css: &Css) {{\n        parseAt(css, CSS_{name}, {}, SCOPE_{name}, LINE0_{name});\n", lit(&self.style_path())));
        for l in c.sc.consts.iter().filter(|l| l.guess.is_none()) {
            o.set(fi, l.init.0, l.init.1);
            o.s(&format!("        {name}_{} = ", l.name));
            o.copy(fi, src, l.init.0, l.init.1);
            o.s(";\n");
        }
        o.set(fi, 0, 0);
        let mut done: Vec<&str> = Vec::new();
        for (_, child, _) in &self.insts {
            if !done.contains(&child.as_str()) {
                done.push(child);
                o.s(&format!("        {child}.styles(css);\n"));
            }
        }
        o.s("    }\n");
        // init
        o.s(&format!("\n    fn init(this: &{name}, d: &Dom) {{\n        this.ready = true;\n        this.dm = d as int;\n"));
        for t in &self.tags {
            o.s(&format!("        this.{} = d.tag({});\n", field_name("tag_", &self.tags, t), lit(t)));
        }
        for t in &self.classes {
            o.s(&format!("        this.{} = d.css.intern({});\n", field_name("cls_", &self.classes, t), lit(t)));
        }
        o.s("    }\n");
        o.append(fns);
        let sf = std::mem::take(&mut self.slot_fns);
        o.append(sf);
        if self.root {
            o.set(fi, 0, 0);
            o.s(&format!("\n    fn render(this: &{name}, size: Vec2, p: {name}Props) {{\n        let d = &this.dom;\n        if hotDue(this.css) {{\n            this.hot();\n        }}\n        d.beginFrame(size);\n        inputs.begin(d);\n        let ev = this.update(d, p);\n        d.layout();\n        d.setPaintHook({name}PaintHook, this as ptr);\n        d.paintAll();\n        inputs.overlay(d);\n        d.endFrame();\n    }}\n"));
            // style hot reload: compare each style source on disk with the embedded text
            o.s(&format!("\n    fn hot(this: &{name}) {{\n        let css = this.css;\n        let ch = false;\n"));
            if let Some(tp) = self.theme_path {
                o.s(&format!("        if hotFile(css, {}, THEME, 0, false) {{\n            ch = true;\n        }}\n", lit(tp)));
            }
            for k in self.all {
                let text = &self.files[k.fi].text;
                let (a, b) = k.mk.style.unwrap_or((text.len(), text.len()));
                let mut h: i64 = 7;
                for &x in text.as_bytes()[..a].iter().chain(text.as_bytes()[b..].iter()) {
                    h = (h * 31 + x as i64) % 1_000_000_007;
                }
                o.s(&format!("        if hotFile(css, {}, CSS_{}, {h}, true) {{\n            ch = true;\n        }}\n", lit(&self.files[k.fi].path), k.id));
            }
            if self.theme_path.is_some() {
                o.s(&format!("        if ch {{\n            css.reset();\n            css.hotWarn = 0;\n            uastyle.load(css);\n            parse(css, THEME, {}, 0);\n            {name}.styles(css);\n            this.dom.restyle();\n            hotWarns(css);\n        }}\n    }}\n", lit(self.theme_path.unwrap())));
            } else {
                o.s(&format!("        if ch {{\n            css.reset();\n            css.hotWarn = 0;\n            uastyle.load(css);\n            {name}.styles(css);\n            this.dom.restyle();\n            hotWarns(css);\n        }}\n    }}\n"));
            }
        }
        o.set(fi, 0, 0);
        let has = |f: &str| c.sc.fns.iter().any(|x| x.name == f);
        o.s(&format!("\n    fn update(this: &{name}, d: &Dom, p: {name}Props): {name}Events {{\n        let mounting = !this.ready;\n        if mounting {{\n            this.init(d);\n        }}\n        *this.p = p;\n"));
        for p in &self.used_u {
            o.s(&format!("        let {p} = p.{p};\n"));
        }
        o.s(&format!("        let ev = {name}Events {{ {} }};\n", ev_init(&c.sc, src)));
        if has("onFrame") {
            o.s("        this.onFrame(d.dt());\n");
        }
        let upd = std::mem::take(&mut self.upd);
        o.append(upd);
        o.set(fi, 0, 0);
        for (k, n, calls) in &self.slot_marks {
            o.s(&format!("        if this.slotWas{k} && !this.slotNow{k} {{\n"));
            for (i, (id, owner)) in calls.iter().enumerate() {
                o.s(&format!("            {}if this.slotSite == {id} {{\n                ((this.slotOwn as ptr) as &{owner}).slot{id}_{n}_leave(d, this.slotI);\n            }}", if i > 0 { " else " } else { "" }));
                if i + 1 == calls.len() {
                    o.s("\n");
                }
            }
            o.s(&format!("        }}\n        this.slotWas{k} = this.slotNow{k};\n        this.slotNow{k} = false;\n"));
        }
        // before the merge below: an `emit` in onMount lands in `sev` too
        if has("onMount") {
            o.s("        if mounting {\n            this.onMount();\n        }\n");
        }
        if bridge {
            for (e, _) in &c.sc.events {
                let copy: String = c.sc.ev_params.iter().filter(|p| p.ev == *e).map(|p| format!("            ev.{e}_{0} = this.sev.{e}_{0};\n", p.name)).collect();
                o.s(&format!("        if this.sev.{e} {{\n            ev.{e} = true;\n{copy}            this.sev.{e} = false;\n        }}\n"));
            }
        }
        o.s("        return ev;\n    }\n");
        // children first, then this one; `ready` makes a second call (a branch left twice) a no-op
        o.s(&format!("\n    fn destroy(this: &{name}) {{\n        if !this.ready {{\n            return;\n        }}\n"));
        o.s(&self.state_destroy(0, "this"));
        if has("onDestroy") {
            o.s("        this.onDestroy();\n");
        }
        o.s("        this.ready = false;\n    }\n");
        o.s(&format!("\n    fn paint(this: &{name}, d: &Dom) {{\n"));
        for p in &self.used_p {
            o.s(&format!("        let {p} = this.p.{p};\n"));
        }
        let pnt = std::mem::take(&mut self.pnt);
        o.append(pnt);
        o.set(fi, 0, 0);
        o.s("    }\n");
        for (n, sites) in &self.names.slot_has {
            let test = if sites.is_empty() { "false".to_string() } else { sites.iter().map(|id| format!("this.slotSite == {id}")).collect::<Vec<_>>().join(" || ") };
            o.s(&format!("\n    fn {}(this: &{name}): bool {{\n        return {test};\n    }}\n", slot_has_fn(n)));
        }
        o.s("}\n\n");
        if self.root {
            // `<canvas>` bodies are generated code: the runtime paints the boxes and calls back here to reach each body (`Dom.paintAll`)
            o.s(&format!("fn {name}PaintHook(p: ptr) {{\n    let r = p as &{name};\n    r.paint(&r.dom);\n}}\n\n"));
        }
        Ok(o)
    }

    /// Struct fields for the state held by container `cont' (0 is the component, else a row).
    fn state_fields(&self, cont: usize) -> String {
        let name = &self.c.id;
        let mut o = String::new();
        for (id, child, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("    c{id}: {child};\n"));
            }
        }
        for (id, c) in &self.ifs {
            if *c == cont {
                o.push_str(&format!("    if{id}: int;\n"));
            }
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("    ua{0}: int;\n    us{0}: int;\n", a.k));
            match a.ty.as_deref() {
                Some("string") => o.push_str(&format!("    up{0}: []u8;\n    ul{0}: int;\n", a.k)),
                Some(t) => o.push_str(&format!("    up{}: {t};\n", a.k)),
                None => {}
            }
        }
        for b in self.binds.iter().filter(|b| b.container == cont) {
            o.push_str(&format!("    bh{}: int;\n", b.k));
        }
        for e in self.eachs.iter().filter(|e| e.container == cont) {
            let id = e.id;
            o.push_str(&format!("    cnt{id}: int;\n"));
            if e.rows {
                o.push_str(&format!("    e{id}: List<&{name}_R{id}>;\n    fp{id}: List<&{name}_R{id}>;\n"));
            }
            if e.keyed {
                o.push_str(&format!("    eb{id}: List<&{name}_R{id}>;\n    kd{id}: Keyed;\n"));
            }
            if e.virt {
                o.push_str(&format!("    vl{id}: VList;\n"));
            }
        }
        o
    }

    /// Initial values for `state_fields`, as struct-literal lines (`mem` is in scope).
    fn state_inits(&self, cont: usize) -> String {
        let name = &self.c.id;
        let mut o = String::new();
        for (id, child, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("            c{id}: {child}.new(mem),\n"));
            }
        }
        for (id, c) in &self.ifs {
            if *c == cont {
                o.push_str(&format!("            if{id}: -1,\n"));
            }
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("            ua{0}: -1,\n            us{0}: 0,\n", a.k));
            match a.ty.as_deref() {
                Some("string") => o.push_str(&format!("            up{0}: mem.alloc::<u8>(0),\n            ul{0}: 0,\n", a.k)),
                Some(t) => o.push_str(&format!("            up{}: {},\n", a.k, ev_default(t))),
                None => {}
            }
        }
        for b in self.binds.iter().filter(|b| b.container == cont) {
            o.push_str(&format!("            bh{}: -1,\n", b.k));
        }
        for e in self.eachs.iter().filter(|e| e.container == cont) {
            let id = e.id;
            o.push_str(&format!("            cnt{id}: 0,\n"));
            if e.rows {
                o.push_str(&format!("            e{id}: List<&{name}_R{id}>.new(mem),\n            fp{id}: List<&{name}_R{id}>.new(mem),\n"));
            }
            if e.keyed {
                o.push_str(&format!("            eb{id}: List<&{name}_R{id}>.new(mem),\n            kd{id}: Keyed.new(mem),\n"));
            }
            if e.virt {
                o.push_str(&format!("            vl{id}: VList.new(mem),\n"));
            }
        }
        o
    }

    /// Fresh state for container `cont` without allocating: components reset, `{#if}`s unset, lists emptied
    /// (their dead rows go to the pool).
    fn state_resets(&self, cont: usize, who: &str) -> String {
        let mut o = String::new();
        for (id, _, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("        {who}.c{id}.j3Reset();\n"));
            }
        }
        for (id, c) in &self.ifs {
            if *c == cont {
                o.push_str(&format!("        {who}.if{id} = -1;\n"));
            }
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("        {who}.ua{0} = -1;\n        {who}.us{0} = 0;\n", a.k));
            if a.ty.as_deref() == Some("string") {
                o.push_str(&format!("        {who}.ul{} = 0;\n", a.k));
            }
        }
        for b in self.binds.iter().filter(|b| b.container == cont) {
            o.push_str(&format!("        {who}.bh{} = -1;\n", b.k));
        }
        for e in self.eachs.iter().filter(|e| e.container == cont) {
            let id = e.id;
            o.push_str(&format!("        {who}.cnt{id} = 0;\n"));
            if e.rows {
                o.push_str(&format!("        for q{id} in 0..{who}.e{id}.len {{\n            {who}.fp{id}.push({who}.e{id}.items[q{id}]);\n        }}\n        {who}.e{id}.len = 0;\n"));
            }
            if e.keyed {
                o.push_str(&format!("        {who}.eb{id}.len = 0;\n        {who}.kd{id}.reset();\n"));
            }
            if e.virt {
                o.push_str(&format!("        {who}.vl{id}.reset();\n"));
            }
        }
        o
    }

    /// `destroy()` calls for what container `cont` holds: components, then every row of its lists.
    fn state_destroy(&self, cont: usize, who: &str) -> String {
        let mut o = String::new();
        // a row reaches its component through `comp`
        let comp = if cont == 0 { "this".to_string() } else { format!("(({who}.comp as ptr) as &{})", self.c.id) };
        for a in self.acts.iter().filter(|a| a.container == cont && a.destroy) {
            o.push_str(&format!("        if {who}.ua{0} >= 0 && {who}.us{0} == 1 {{\n            {comp}.{1}_destroy({who}.ua{0});\n        }}\n", a.k, a.name));
        }
        for a in self.acts.iter().filter(|a| a.container == cont) {
            o.push_str(&format!("        {who}.ua{0} = -1;\n        {who}.us{0} = 0;\n", a.k));
        }
        for (id, _, c) in &self.insts {
            if *c == cont {
                o.push_str(&format!("        {who}.c{id}.destroy();\n"));
            }
        }
        for e in self.eachs.iter().filter(|e| e.container == cont && e.rows) {
            let id = e.id;
            o.push_str(&format!("        for q{id} in 0..{who}.e{id}.len {{\n            {who}.e{id}.items[q{id}].destroy();\n        }}\n"));
        }
        o
    }

    fn style_path(&self) -> String {
        self.files[self.fi].path.clone()
    }
}

#[derive(Clone)]
struct EachHead {
    expr: Range,
    range: Option<(Range, Range)>,
    item: (String, Range),
    /// `as &x`: the item is a pointer to the element, so the body can change it in place.
    by_ref: bool,
    index: Option<(String, Range)>,
    /// `(expr)` after the names: what identifies a row across frames.
    key: Option<Range>,
    /// `(expr : int)`: an int key, kept as raw bytes (no formatting).
    key_int: bool,
}

fn parse_each_head(src: &str, r: Range) -> R<EachHead> {
    let bad = |msg: &str| err(r.0, r.1, msg.to_string()).map_err(|e| e.help("`{#each items as item}`, `{#each items as &item}`, `{#each items as item, i}`, `{#each items as item (item.id)}`, or `{#each 0..n as i}`"));
    let toks = lex(&src[r.0..r.1], FileId(0)).map_err(|e| UiError { lo: r.0 + e.span.start as usize, hi: r.0 + e.span.end as usize, msg: e.message, help: None })?;
    let abs = |t: &crate::parse::Token| (r.0 + t.span.start as usize, r.0 + t.span.end as usize);
    // the last top-level `as`
    let mut depth = 0i32;
    let mut at = None;
    for (k, t) in toks.iter().enumerate() {
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
            TokenKind::As if depth == 0 => at = Some(k),
            _ => {}
        }
    }
    let Some(at) = at else { return bad("`{#each}` needs `as`") };
    let mut rest = &toks[at + 1..];
    let by_ref = rest.first().is_some_and(|t| t.kind == TokenKind::Amp);
    if by_ref {
        rest = &rest[1..];
    }
    // `item`, `item, index`, each with an optional `(key)` to the end
    let (names, tail) = match rest {
        [a, c, b, t @ ..] if a.kind == TokenKind::Ident && c.kind == TokenKind::Comma && b.kind == TokenKind::Ident => (2, t),
        [a, t @ ..] if a.kind == TokenKind::Ident => (1, t),
        _ => return bad("expected `as item` or `as item, index` at the end of `{#each}`"),
    };
    let mut key_int = false;
    let key = match tail {
        [e] if e.kind == TokenKind::Eof => None,
        [l, mid @ .., rp, e] if l.kind == TokenKind::LParen && rp.kind == TokenKind::RParen && e.kind == TokenKind::Eof => {
            let mut k = trim_range(src, abs(l).1, abs(rp).0);
            if let [.., c, i] = mid {
                if c.kind == TokenKind::Colon && i.kind == TokenKind::Ident && i.text == "int" {
                    key_int = true;
                    k = trim_range(src, abs(l).1, abs(c).0);
                }
            }
            if k.0 == k.1 {
                return bad("the `(key)` is empty");
            }
            let mut d = 0i32;
            for t in mid {
                match t.kind {
                    TokenKind::LParen => d += 1,
                    TokenKind::RParen => d -= 1,
                    _ => {}
                }
                if d < 0 {
                    return bad("unbalanced `)` in the `(key)`");
                }
            }
            Some(k)
        }
        _ => return bad("expected `as item`, `as item, index` or `as item (key)` (`(key : int)` for an int key) at the end of `{#each}`"),
    };
    let item = (rest[0].text.clone(), abs(&rest[0]));
    let index = if names == 2 { Some((rest[2].text.clone(), abs(&rest[2]))) } else { None };
    let end = abs(&toks[at]).0;
    let expr = trim_range(src, r.0, end);
    if expr.0 == expr.1 {
        return bad("`{#each}` has nothing before `as`");
    }
    let mut depth = 0i32;
    let mut range = None;
    for t in &toks[..at] {
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => depth -= 1,
            TokenKind::DotDotEq if depth == 0 => {
                let a = abs(t);
                return err(a.0, a.1, "use `a..b` (b excluded); `..=` isn't supported in `{#each}`");
            }
            TokenKind::DotDot if depth == 0 => {
                let a = abs(t);
                range = Some((trim_range(src, expr.0, a.0), trim_range(src, a.1, expr.1)));
            }
            _ => {}
        }
    }
    Ok(EachHead { expr, range, item, by_ref, index, key, key_int })
}

/// `use a.b.{ X, Y };` to `["a.b.X", "a.b.Y"]`; anything odd (aliases) is kept whole.
fn use_names(text: &str) -> Vec<String> {
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

/// Attributes and directives an element accepts (for editor completion).
pub fn element_attrs(tag: &str) -> Vec<&'static str> {
    G::accepted(tag)
}

/// The longhand CSS property names (`PROP_NAMES` in the embedded `ui/css.j3`), space separated.
fn css_props() -> &'static str {
    let src = crate::stdlib::files().find(|(n, _)| *n == "ui/css.j3").map_or("", |f| f.1);
    src.split("PROP_NAMES: string = \"").nth(1).and_then(|r| r.split('"').next()).unwrap_or("")
}

/// Conditionals do not add a DOM parent; components/elements do.
fn tooltip_child(nodes: &[Node]) -> bool {
    nodes.iter().any(|n| match n {
        Node::Elem(e) => !e.comp && e.attrs.iter().any(|a| a.name == "popover" && matches!(a.value, Value::None)),
        Node::If(b) => b.branches.iter().any(|arm| tooltip_child(&arm.nodes)),
        _ => false,
    })
}

/// Option content is mirrored into popup rows: keep controls, handlers and component identities out.
fn option_visuals(nodes: &[Node]) -> R<()> {
    for n in nodes {
        match n {
            Node::Text { .. } => {},
            Node::Elem(e) => {
                if e.comp || !["div", "span", "p", "small", "h1", "h2", "img", "svg", "path"].contains(&e.tag.as_str()) {
                    return err(e.open.0, e.open.1, "option content must be visual markup (text, div/span/p/small/h1/h2, img, svg, path); put controls outside the select");
                }
                for a in &e.attrs {
                    if !(a.name == "class" || a.name.starts_with("class:") || a.name.starts_with("style:") || ["src", "alt", "sprite", "frame", "fps", "viewBox", "d", "fill", "stroke", "stroke-width"].contains(&a.name.as_str())) {
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
