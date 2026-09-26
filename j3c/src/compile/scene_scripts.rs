//! Scripts attached to objects in scene files (`node.<i>.script s=<path>`).
//!
//! The loader scans every `.j3scene` under the project root. Each attached script joins the
//! program, and `j3.scene3d.scripts` is synthesized in memory (no file on disk): one
//! instance list per script type, filled from the objects' `param.<id>.<prop>` lines when
//! the scene starts, plus `start`/`update` for every instance and a `find` per type behind
//! `scene.script<Door>(node)`. A `node: NodeId` prop is the object the script is attached to
//! unless the scene sets it. Scene mistakes are reported against the scene file's line.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::parse::{self, Expr, ExprKind, FnItem, Item, Type, TypeKind};
use crate::span::FileId;

/// The synthesized module's name.
pub const MODULE: &str = "j3.scene3d.scripts";
/// What diagnostics call it.
pub const LABEL: &str = "(scene scripts)";

/// One object with a script.
#[derive(Clone, Debug)]
pub struct Attachment {
    /// Scene file, relative to the root.
    pub scene: String,
    pub line: usize,
    pub id: i64,
    pub script: String,
}

/// One `param.<id>.<field>` line.
#[derive(Clone, Debug)]
pub struct Param {
    pub field: String,
    /// ' ' (int or bool), 'f', 's', or 'r' (object).
    pub kind: char,
    pub value: String,
    pub line: usize,
}

#[derive(Default)]
pub struct Scan {
    pub attachments: Vec<Attachment>,
    /// (scene, object id) → its params.
    pub params: BTreeMap<(String, i64), Vec<Param>>,
    /// `[editor] scene` from `j3.editor.toml`, or "".
    pub main_scene: String,
}

fn scene_files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            if name != "target" && name != ".git" && !path.is_symlink() {
                scene_files(root, &path, out);
            }
        } else if name.ends_with(".j3scene") {
            out.push(path);
        }
    }
}

/// Find every script attachment and param under `root`.
pub fn scan(root: &Path) -> Scan {
    let mut out = Scan::default();
    let mut files = Vec::new();
    scene_files(root, root, &mut files);
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        let mut ids: BTreeMap<String, i64> = BTreeMap::new();
        let mut scripts: Vec<(String, String, usize)> = Vec::new();
        for (n, line) in text.lines().enumerate() {
            if let Some(rest) = line.strip_prefix("node.") {
                let Some((index, field)) = rest.split_once('.') else { continue };
                if let Some(id) = field.strip_prefix("id=") {
                    if let Ok(id) = id.trim().parse() {
                        ids.insert(index.to_string(), id);
                    }
                } else if let Some(script) = field.strip_prefix("script s=") {
                    if !script.is_empty() {
                        scripts.push((index.to_string(), script.to_string(), n + 1));
                    }
                }
            } else if let Some(rest) = line.strip_prefix("param.") {
                let Some((id, field)) = rest.split_once('.') else { continue };
                let Ok(id) = id.parse::<i64>() else { continue };
                let Some((key, value)) = field.split_once('=') else { continue };
                let (name, kind) = match key.split_once(' ') {
                    Some((name, k)) => (name, k.chars().next().unwrap_or(' ')),
                    None => (key, ' '),
                };
                out.params.entry((rel.clone(), id)).or_default().push(Param {
                    field: name.to_string(),
                    kind,
                    value: value.to_string(),
                    line: n + 1,
                });
            }
        }
        for (index, script, line) in scripts {
            if let Some(id) = ids.get(&index) {
                out.attachments.push(Attachment {
                    scene: rel.clone(),
                    line,
                    id: *id,
                    script,
                });
            }
        }
    }
    if let Ok(config) = std::fs::read_to_string(root.join("j3.editor.toml")) {
        if let Some(scene) = config
            .lines()
            .filter_map(|l| l.split_once('='))
            .find(|(k, _)| k.trim() == "scene")
            .map(|(_, v)| v.trim().trim_matches('"').to_string())
        {
            out.main_scene = scene;
        }
    }
    out
}

impl Scan {
    /// The distinct attached script paths, sorted.
    pub fn scripts(&self) -> Vec<String> {
        let mut out: Vec<String> = self.attachments.iter().map(|a| a.script.clone()).collect();
        out.sort();
        out.dedup();
        out
    }
}

/// How a prop's value comes from a scene.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PropKind {
    Float,
    Double,
    Int,
    Bool,
    Text,
    Object,
    Vector,
    /// Can't be set from a scene.
    Other,
}

fn type_name(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Named(n) => n.rsplit('.').next().unwrap_or(n).to_string(),
        _ => String::new(),
    }
}

fn prop_kind(ty: &Type) -> PropKind {
    match type_name(ty).as_str() {
        "f32" => PropKind::Float,
        "f64" | "float" => PropKind::Double,
        "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => PropKind::Int,
        "bool" => PropKind::Bool,
        "string" => PropKind::Text,
        "NodeId" => PropKind::Object,
        "Vec3" => PropKind::Vector,
        _ => PropKind::Other,
    }
}

fn kind_words(kind: PropKind) -> &'static str {
    match kind {
        PropKind::Float | PropKind::Double => "a number (` f=`)",
        PropKind::Int => "a whole number",
        PropKind::Bool => "0 or 1",
        PropKind::Text => "text (` s=`)",
        PropKind::Object => "an object (` r=<id>`)",
        PropKind::Vector => "three numbers (`.x`, `.y`, `.z`)",
        PropKind::Other => "nothing a scene can give",
    }
}

fn param_fits(kind: PropKind, param: &Param) -> bool {
    match kind {
        PropKind::Float | PropKind::Double => param.kind == 'f' || param.kind == ' ',
        PropKind::Int => param.kind == ' ',
        PropKind::Bool => param.kind == ' ' && (param.value == "0" || param.value == "1"),
        PropKind::Text => param.kind == 's',
        PropKind::Object => param.kind == 'r',
        PropKind::Vector => param.kind == 'f',
        PropKind::Other => false,
    }
}

/// A script's props and hooks, from its parsed file.
struct ScriptShape {
    props: Vec<(String, Type, Option<Expr>)>,
    fns: Vec<FnItem>,
}

fn shape(root: &Path, rel: &str) -> Result<Option<ScriptShape>, String> {
    let path = root.join(rel);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let parts = super::script_module_parts(rel)?;
    let file = parse::parse_script(&text, FileId(0), &parts)
        .map_err(|e| parse::format_error(rel, &text, &e))?;
    let mut props = Vec::new();
    let mut fns = Vec::new();
    for item in file.items {
        match item {
            Item::Props(b) => {
                for f in b.fields {
                    props.push((f.name.name, f.ty, f.default));
                }
            }
            Item::Fn(f) => fns.push(f),
            _ => {}
        }
    }
    Ok(Some(ScriptShape { props, fns }))
}

fn scene_error(scene: &str, line: usize, text: &str, message: &str, help: &str) -> String {
    let mut out = format!("error: {message}\n  --> {scene}:{line}\n");
    if !text.is_empty() {
        let width = line.to_string().len();
        out.push_str(&format!("{:width$} |\n{line} | {text}\n{:width$} |\n", "", ""));
    }
    if !help.is_empty() {
        let width = line.to_string().len();
        out.push_str(&format!("{:width$} = help: {help}\n", ""));
    }
    out
}

fn scene_line(root: &Path, scene: &str, line: usize) -> String {
    std::fs::read_to_string(root.join(scene))
        .ok()
        .and_then(|t| t.lines().nth(line.saturating_sub(1)).map(str::to_string))
        .unwrap_or_default()
}

fn type_text(ty: &Type) -> String {
    match &ty.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::Ref(inner) => format!("&{}", type_text(inner)),
        _ => "?".to_string(),
    }
}

/// Hooks the scene host calls must have these exact shapes.
fn check_hook(f: &FnItem, want: &[&str]) -> bool {
    let got: Vec<String> = f.params.iter().map(|p| type_text(&p.ty)).collect();
    f.vis_pub && f.ret.is_none() && got.len() == want.len() && got.iter().zip(want).all(|(g, w)| g == w)
}

/// Check every attachment against its script, and build the registry module's source.
pub fn registry(root: &Path, scan: &Scan) -> Result<String, String> {
    let mut errors = String::new();
    let mut shapes = BTreeMap::new();
    let known = super::script_paths(root);
    for a in &scan.attachments {
        if shapes.contains_key(&a.script) {
            continue;
        }
        match shape(root, &a.script)? {
            Some(s) => {
                shapes.insert(a.script.clone(), s);
            }
            None => {
                let help = crate::diag::did_you_mean(&a.script, &known)
                    .map(|s| format!("did you mean `{s}`?"))
                    .unwrap_or_default();
                errors.push_str(&scene_error(
                    &a.scene,
                    a.line,
                    &scene_line(root, &a.scene, a.line),
                    &format!("no script at {}", a.script),
                    &help,
                ));
            }
        }
    }
    for a in &scan.attachments {
        let Some(s) = shapes.get(&a.script) else { continue };
        let at = scene_line(root, &a.scene, a.line);
        for f in &s.fns {
            let (want, sig): (&[&str], &str) = match f.name.name.as_str() {
                "start" => (&["&Scene"], "pub fn start(scene: &Scene)"),
                "update" => (&["&Scene", "f32"], "pub fn update(scene: &Scene, dt: f32)"),
                _ => continue,
            };
            if !check_hook(f, want) {
                errors.push_str(&scene_error(
                    &a.scene,
                    a.line,
                    &at,
                    &format!(
                        "{} is attached here, so the scene calls its `{}`, which must be `{sig}`",
                        a.script, f.name.name
                    ),
                    &format!("change `{}` in {} to `{sig}`, or rename it", f.name.name, a.script),
                ));
            }
        }
        let params = scan
            .params
            .get(&(a.scene.clone(), a.id))
            .cloned()
            .unwrap_or_default();
        for (name, ty, default) in &s.props {
            let kind = prop_kind(ty);
            let own: Vec<&Param> = params
                .iter()
                .filter(|p| p.field == *name || p.field.starts_with(&format!("{name}.")))
                .collect();
            if own.is_empty() {
                if default.is_none() && !(name == "node" && kind == PropKind::Object) {
                    errors.push_str(&scene_error(
                        &a.scene,
                        a.line,
                        &at,
                        &format!("{} needs prop `{name}`, and this object doesn't set it", a.script),
                        &format!("set it in the inspector, or add `param.{}.{name}` lines", a.id),
                    ));
                }
                continue;
            }
            for p in own {
                if !param_fits(kind, p) {
                    errors.push_str(&scene_error(
                        &a.scene,
                        p.line,
                        &scene_line(root, &a.scene, p.line),
                        &format!(
                            "`{name}` is `{}` in {}, so the scene must give {}",
                            type_text(ty),
                            a.script,
                            kind_words(kind)
                        ),
                        "",
                    ));
                }
            }
        }
        for p in &params {
            let base = p.field.split('.').next().unwrap_or(&p.field);
            if p.field.starts_with("fx.") || s.props.iter().any(|(n, _, _)| n == base) {
                continue;
            }
            errors.push_str(&scene_error(
                &a.scene,
                p.line,
                &scene_line(root, &a.scene, p.line),
                &format!("`{}` isn't a prop of {}", p.field, a.script),
                &format!("props of {}: {}", a.script, prop_names(&s.props)),
            ));
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    Ok(source(scan, &shapes))
}

fn prop_names(props: &[(String, Type, Option<Expr>)]) -> String {
    if props.is_empty() {
        return "(none)".to_string();
    }
    props.iter().map(|(n, _, _)| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

/// Registry names for a script path: `scripts/door.j3s` → `scripts__door`.
pub fn ident(script: &str) -> String {
    script.trim_end_matches(".j3s").replace('/', "__")
}

fn literal(e: &Expr) -> String {
    match &e.folded_literal().kind {
        ExprKind::Int(v) => v.to_string(),
        ExprKind::Float(v) => {
            let s = format!("{v:?}");
            if s.contains('.') || s.contains('e') { s } else { format!("{s}.0") }
        }
        ExprKind::Bool(v) => v.to_string(),
        ExprKind::String(bytes) => format!("{:?}", String::from_utf8_lossy(bytes)),
        _ => "0".to_string(),
    }
}

fn getter(name: &str, ty: &Type, default: &Option<Expr>) -> Option<String> {
    let fallback = |zero: &str| default.as_ref().map(literal).unwrap_or_else(|| zero.to_string());
    let ty_name = type_name(ty);
    Some(match prop_kind(ty) {
        PropKind::Float => format!("scene3d.number(scene, id, {name:?}, {})", fallback("0.0")),
        PropKind::Double => format!("scene3d.number(scene, id, {name:?}, {}) as f64", fallback("0.0")),
        PropKind::Int => format!(
            "scene3d.integer(scene, id, {name:?}, {}) as {ty_name}",
            fallback("0")
        ),
        PropKind::Bool => format!("scene3d.flag(scene, id, {name:?}, {})", fallback("false")),
        PropKind::Text => format!("scene3d.label(scene, id, {name:?}, {})", fallback("\"\"")),
        // `node: NodeId` is the object the script is attached to unless the scene says otherwise.
        PropKind::Object if name == "node" => "NodeId { id: own(scene3d.object(scene, id, \"node\"), id) }".to_string(),
        PropKind::Object => format!("NodeId {{ id: scene3d.object(scene, id, {name:?}) }}"),
        PropKind::Vector => format!("scene3d.vector(scene, id, {name:?}, Vec3.xyz(0, 0, 0))"),
        PropKind::Other => return None,
    })
}

fn has_hook(s: &ScriptShape, name: &str) -> bool {
    s.fns.iter().any(|f| f.name.name == name)
}

fn source(scan: &Scan, shapes: &BTreeMap<String, ScriptShape>) -> String {
    let mut out = String::from(
        "// Made by j3c from the project's scenes (never written to disk).\n\
         module j3.scene3d.scripts;\n\n\
         use lib \"c\" as libc {\n    fn getenv(name: cstr): cstr;\n    fn strlen(s: cstr): int;\n}\n\n\
         use j3.scene3d;\nuse j3.scene3d.{ Scene, NodeId };\nuse j3.math.Vec3;\n\
         use j3.memory.Arena;\nuse j3.option.Option;\nuse j3.text.{ cstr, same };\n\n",
    );
    out.push_str(&format!(
        "/// The scene set in j3.editor.toml.\npub const MAIN_SCENE: string = {:?};\n\n",
        scan.main_scene
    ));
    out.push_str(
        "/// The scene to run: the one the editor's Play picked (`J3_SCENE`), else `MAIN_SCENE`.\n\
         pub fn scene(): string {\n    let raw = libc.getenv(cstr(\"J3_SCENE\"));\n    \
         if raw == 0 { return MAIN_SCENE; }\n    let len = libc.strlen(raw);\n    \
         if len < 1 { return MAIN_SCENE; }\n    return []u8 { ptr: raw as &u8, len: len } as string;\n}\n\n",
    );
    for (script, _) in shapes {
        let id = ident(script);
        out.push_str(&format!("const T__{id} = @script({script:?});\n"));
        out.push_str(&format!("#[singleton]\nlet LIVE__{id}: []T__{id};\n"));
        out.push_str(&format!("#[singleton]\nlet IDS__{id}: []int;\n"));
        out.push_str(&format!("#[singleton]\nlet COUNT__{id}: int;\n\n"));
    }
    out.push_str("fn own(set: int, id: int): int {\n    if set == 0 { return id; }\n    return set;\n}\n\n");
    out.push_str("/// Make an instance for every object with a script, then run each `start`.\npub fn start(scene: &Scene) {\n");
    for (script, _) in shapes {
        out.push_str(&format!("    COUNT__{} = 0;\n", ident(script)));
    }
    out.push_str("    for k in 0..scene.len {\n        let attached = scene.nodes[k].script;\n");
    for (script, _) in shapes {
        out.push_str(&format!(
            "        if same(attached, {script:?}) {{ COUNT__{} += 1; }}\n",
            ident(script)
        ));
    }
    out.push_str("    }\n");
    for (script, _) in shapes {
        let id = ident(script);
        out.push_str(&format!(
            "    LIVE__{id} = scene.arena.alloc::<T__{id}>(COUNT__{id});\n    IDS__{id} = scene.arena.alloc::<int>(COUNT__{id});\n    COUNT__{id} = 0;\n"
        ));
    }
    out.push_str("    for i in 0..scene.len {\n        let id = scene.nodes[i].id;\n        let path = scene.nodes[i].script;\n");
    for (script, _) in shapes {
        out.push_str(&format!(
            "        if same(path, {script:?}) {{ spawn__{}(scene, id); }}\n",
            ident(script)
        ));
    }
    out.push_str("    }\n");
    for (script, s) in shapes {
        if has_hook(s, "start") {
            let id = ident(script);
            out.push_str(&format!(
                "    for i in 0..COUNT__{id} {{ LIVE__{id}[i].start(scene); }}\n"
            ));
        }
    }
    out.push_str("}\n\n/// Run every instance's `update`.\npub fn update(scene: &Scene, dt: f32) {\n");
    for (script, s) in shapes {
        if has_hook(s, "update") {
            let id = ident(script);
            out.push_str(&format!(
                "    for i in 0..COUNT__{id} {{ LIVE__{id}[i].update(scene, dt); }}\n"
            ));
        }
    }
    out.push_str("}\n");
    for (script, s) in shapes {
        let id = ident(script);
        let fields: Vec<String> = s
            .props
            .iter()
            .filter_map(|(name, ty, default)| {
                getter(name, ty, default).map(|g| format!("{name}: {g}"))
            })
            .collect();
        out.push_str(&format!(
            "\nfn spawn__{id}(scene: &Scene, id: int) {{\n    LIVE__{id}[COUNT__{id}] = T__{id} {{ {} }};\n    IDS__{id}[COUNT__{id}] = id;\n    COUNT__{id} += 1;\n}}\n",
            fields.join(", ")
        ));
        // ponytail: linear scan per lookup; index by id if scenes grow past a few thousand objects.
        out.push_str(&format!(
            "\n/// `scene.script<...>(node)` for {script}.\npub fn find__{id}(scene: &Scene, node: NodeId): Option<&T__{id}> {{\n    \
             for i in 0..COUNT__{id} {{\n        \
             if IDS__{id}[i] == node.id {{ return Option.Some(&LIVE__{id}[i]); }}\n    }}\n    \
             return Option.None;\n}}\n"
        ));
    }
    out
}
