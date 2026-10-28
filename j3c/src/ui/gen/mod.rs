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

mod assemble;
mod attr;
mod comp;
mod control;
mod elem;
mod emit;
mod rewrite;
mod slots;

use control::*;
use rewrite::*;
use slots::*;

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
