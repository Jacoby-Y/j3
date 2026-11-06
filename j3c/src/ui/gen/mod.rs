//! `.j3ui` components to J3 source text, in the shape `speculative/12_ui_runtime/README.md`
//! describes. All components of a project become one module, `<package>.ui`.
//!
//! J3 from the file (script bodies, `{expr}`, handlers) is copied verbatim; the only edits are
//! token-level (`this.` before an instance `let` or script `fn`, `emit x` to `ev.x = true`),
//! found with the real lexer. The text then goes through `parse::parse` like any file.

use std::collections::{BTreeSet, HashMap, HashSet};

use super::icons::{self, Icon};
use super::map::{Out, Seg};
use super::pass::{self, Pass};
use super::markup::{self, err, trim_range, Attr, EachNode, Elem, IfNode, LetNode, Markup, Node, Part, Range, Script, UiError, Value, R};
use crate::parse::lex::lex;
use crate::parse::TokenKind;
use crate::span::FileId;

mod assemble;
mod attr;
mod attr_bind;
mod attr_elem;
mod attr_event;
mod attr_extra;
mod attr_value;
mod bind_group;
mod comp;
mod control;
mod elem;
mod emit;
mod inject;
mod query;
mod rewrite;
mod slots;

use control::*;
use inject::*;
use query::*;
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
    "div", "span", "p", "h1", "h2", "small", "header", "nav", "main", "section", "label", "button", "input", "textarea", "select", "option", "dl", "dt", "dd", "canvas", "dialog", "aside", "img", "svg", "path", "h3", "h4", "h5", "h6", "ul", "ol", "li", "a", "hr", "strong", "em", "b", "i", "pre", "code", "br", "progress", "meter", "details", "summary", "glyph", "icon",
];
pub const MOUSE: &[&str] = &["click", "mousedown", "mouseup", "mousemove", "wheel", "input", "change", "submit", "close", "contextmenu", "trigger", "edit", "dragstart", "dragend", "dragenter", "dragleave", "dragover", "drop", "toggle"];

/// J3 names the generated module already declares or imports.
const RESERVED: [&str; 7] = ["Arena", "List", "Css", "Dom", "Vec2", "Keyed", "THEME"];

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

pub fn generate(files: &[SrcFile], package: &str, theme: Option<&Theme>, icons: &[Icon]) -> Result<Built, GenError> {
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
        let mut g = G::new(&comps, files, theme.map(|t| t.path.as_str()), icons, c, root, &slots);
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
    head.s("use j3.ui.css.Css;\nuse j3.ui.cssparse.{ parse, parseAt };\nuse j3.ui.uastyle;\nuse j3.ui.dom.Dom;\nuse j3.ui.domhot.{ hotDue, hotFile, hotWarns };\nuse j3.ui.inputs;\nuse j3.text.Text;\n");
    // every component's `use`, split into one `use path.Name;` per name, each once
    let mut seen: HashSet<String> = HashSet::from(["j3.text.Text".to_string()]);
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
    /// The project's `icons/*.svg` (before Bootstrap).
    icons: &'a [Icon],
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

/// Attributes and directives an element accepts (for editor completion).
pub fn element_attrs(tag: &str) -> Vec<&'static str> {
    G::accepted(tag)
}
