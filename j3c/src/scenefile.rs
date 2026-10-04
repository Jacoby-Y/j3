//! `.j3scene` text as Rust sees it: the objects of a v1 or v2 file with typed fields and their
//! script params. The one reader for everything outside the editor program (`scene3d` in J3 is
//! the real one): `-e scene diff`, scene script checks, asset renames. The v2 grammar is
//! "Scene file format" in editor/README.md; keep the two readers in step.

use std::fmt::Write;

pub const GROUP: i64 = 0;
pub const MESH: i64 = 1;
pub const CAMERA: i64 = 2;
pub const INSTANCE: i64 = 3;
pub const LIGHT: i64 = 4;

/// One script prop line (`param.<id>.<field>` in v1, `param.<field>=` on the object line in v2).
#[derive(Clone, Debug)]
pub struct Param {
    pub field: String,
    /// ' ' (int or bool), 'f', 's', or 'r' (object).
    pub kind: char,
    pub value: String,
    pub line: usize,
}

#[derive(Clone, Debug)]
pub struct Object {
    /// Line the object starts on (its first `node.<i>.` line, or its v2 line).
    pub line: usize,
    /// Line its `script` is set on (the same as `line` in v2).
    pub script_line: usize,
    pub id: i64,
    pub parent: i64,
    pub kind: i64,
    pub name: String,
    pub asset: String,
    pub texture: String,
    pub script: String,
    pub scene: String,
    pub material: String,
    pub effects: String,
    pub tags: String,
    pub double_sided: bool,
    pub alpha: i64,
    pub pos: [f64; 3],
    /// Radians.
    pub rot: [f64; 3],
    pub scale: [f64; 3],
    pub tint: [f64; 3],
    pub color: [f64; 3],
    pub fov: f64,
    pub near: f64,
    pub far: f64,
    pub range: f64,
    pub opacity: f64,
    pub params: Vec<Param>,
}

impl Object {
    /// A fresh object with every field at its default.
    pub fn new(slot: usize) -> Object { Object::blank(slot, 0) }

    fn blank(slot: usize, line: usize) -> Object {
        Object {
            line,
            script_line: line,
            id: slot as i64 + 1,
            parent: 0,
            kind: GROUP,
            name: "Object".into(),
            asset: String::new(),
            texture: String::new(),
            script: String::new(),
            scene: String::new(),
            material: String::new(),
            effects: String::new(),
            tags: String::new(),
            double_sided: false,
            alpha: 0,
            pos: [0.0; 3],
            rot: [0.0; 3],
            scale: [1.0; 3],
            tint: [1.0; 3],
            color: [1.0, 0.9, 0.75],
            fov: 60.0,
            near: 0.15,
            far: 200.0,
            range: 8.0,
            opacity: 1.0,
            params: Vec::new(),
        }
    }

    /// The fields that differ from a fresh object and apply to this kind, as (key, text) in the
    /// v2 spelling; position is left out (callers compare it as a vector). Both file versions
    /// give the same list for the same scene.
    pub fn fields(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut put = |k: &str, v: String| out.push((k.to_string(), v));
        let d = Object::blank(0, 0);
        let vec = |v: [f64; 3]| format!("{},{},{}", num(v[0]), num(v[1]), num(v[2]));
        put("kind", kind_word(self.kind).into());
        put("name", self.name.clone());
        let rot = self.rot.map(|r| trimmed(deg(r), 3));
        if rot.iter().any(|d| d != "0") { put("rot", rot.join(",")); }
        if vec(self.scale) != vec(d.scale) { put("scale", vec(self.scale)); }
        let (mesh, inst) = (self.kind == MESH, self.kind == INSTANCE);
        if mesh && !self.asset.is_empty() { put("asset", self.asset.clone()); }
        if mesh && !self.material.is_empty() { put("material", self.material.clone()); }
        if mesh && !self.texture.is_empty() { put("texture", self.texture.clone()); }
        if inst && !self.scene.is_empty() { put("scene", self.scene.clone()); }
        if (mesh || inst) && vec(self.tint) != vec(d.tint) { put("tint", vec(self.tint)); }
        if self.kind == LIGHT {
            if vec(self.color) != vec(d.color) { put("color", vec(self.color)); }
            if num(self.range) != num(d.range) { put("range", num(self.range)); }
        }
        if self.kind == CAMERA {
            for (k, v, base) in [("fov", self.fov, d.fov), ("near", self.near, d.near), ("far", self.far, d.far)] {
                if num(v) != num(base) { put(k, num(v)); }
            }
            if !self.effects.is_empty() { put("fx", self.effects.clone()); }
        }
        if mesh && self.alpha != 0 { put("alpha", self.alpha.to_string()); }
        if mesh && self.alpha != 0 && num(self.opacity) != "1" { put("opacity", num(self.opacity)); }
        if mesh && self.double_sided { put("doubleSided", "1".into()); }
        if !self.script.is_empty() { put("script", self.script.clone()); }
        if !self.tags.is_empty() { put("tags", self.tags.clone()); }
        for p in &self.params { put(&format!("param.{}", p.field), p.value.clone()); }
        out
    }
}

pub fn kind_word(kind: i64) -> &'static str {
    match kind {
        MESH => "mesh",
        CAMERA => "camera",
        INSTANCE => "place",
        LIGHT => "light",
        _ => "group",
    }
}

fn deg(rad: f64) -> f64 { rad * 57.295_779_513_082_32 }

/// Six decimals, trailing zeros trimmed (`1.5`, `2`).
pub fn num(x: f64) -> String { trimmed(x, 6) }

fn trimmed(x: f64, places: usize) -> String {
    let mut s = String::new();
    let _ = write!(s, "{x:.places$}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

fn float(s: &str) -> f64 { s.trim().parse().unwrap_or(0.0) }

fn int(s: &str) -> i64 { s.trim().parse::<i64>().or_else(|_| s.trim().parse::<f64>().map(|f| f as i64)).unwrap_or(0) }

/// Read a scene file. Problems are collected (the first is returned beside the objects) and the
/// rest of the file is still read.
pub fn parse(text: &str) -> (Vec<Object>, Option<String>) {
    if text.starts_with("j3scene") { parse_v2(text) } else { (parse_v1(text), None) }
}

fn set_v1(o: &mut Object, field: &str, value: &str) {
    match field {
        "id" => o.id = int(value),
        "parent" => o.parent = int(value),
        "kind" => o.kind = int(value),
        "name" => o.name = value.into(),
        "asset" => o.asset = value.into(),
        "texture" => o.texture = value.into(),
        "script" => o.script = value.into(),
        "scene" => o.scene = value.into(),
        "material" => o.material = value.into(),
        "effects" => o.effects = value.into(),
        "tags" => o.tags = value.into(),
        "doubleSided" => o.double_sided = int(value) != 0,
        "alpha" => o.alpha = int(value),
        "px" => o.pos[0] = float(value),
        "py" => o.pos[1] = float(value),
        "pz" => o.pos[2] = float(value),
        "rx" => o.rot[0] = float(value),
        "ry" => o.rot[1] = float(value),
        "rz" => o.rot[2] = float(value),
        "sx" => o.scale[0] = float(value),
        "sy" => o.scale[1] = float(value),
        "sz" => o.scale[2] = float(value),
        "tr" => o.tint[0] = float(value),
        "tg" => o.tint[1] = float(value),
        "tb" => o.tint[2] = float(value),
        "cr" => o.color[0] = float(value),
        "cg" => o.color[1] = float(value),
        "cb" => o.color[2] = float(value),
        "fov" => o.fov = float(value),
        "near" => o.near = float(value),
        "far" => o.far = float(value),
        "range" => o.range = float(value),
        "opacity" => o.opacity = float(value),
        _ => {}
    }
}

fn parse_v1(text: &str) -> Vec<Object> {
    let mut objects: Vec<Object> = Vec::new();
    let mut params: Vec<(i64, Param)> = Vec::new();
    for (n, line) in text.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("node.") {
            let Some((slot, rest)) = rest.split_once('.') else { continue };
            let Ok(slot) = slot.parse::<usize>() else { continue };
            let Some((key, value)) = rest.split_once('=') else { continue };
            if slot > text.len() { continue; }
            while objects.len() <= slot { objects.push(Object::blank(objects.len(), n + 1)); }
            let field = key.split(' ').next().unwrap_or(key);
            set_v1(&mut objects[slot], field, value);
            if field == "script" { objects[slot].script_line = n + 1; }
        } else if let Some(rest) = line.strip_prefix("param.") {
            let Some((id, field)) = rest.split_once('.') else { continue };
            let Ok(id) = id.parse::<i64>() else { continue };
            let Some((key, value)) = field.split_once('=') else { continue };
            let (name, kind) = match key.split_once(' ') {
                Some((name, k)) => (name, k.chars().next().unwrap_or(' ')),
                None => (key, ' '),
            };
            params.push((id, Param { field: name.into(), kind, value: value.into(), line: n + 1 }));
        }
    }
    for (id, p) in params {
        if let Some(o) = objects.iter_mut().find(|o| o.id == id) { o.params.push(p); }
    }
    objects
}

/// Split on spaces outside quotes.
pub fn tokens(s: &str) -> Vec<&str> {
    let (mut out, mut start, mut quoted, mut escaped) = (Vec::new(), None, false, false);
    for (i, c) in s.char_indices() {
        if start.is_none() && c != ' ' { start = Some(i); }
        if quoted {
            if escaped { escaped = false; } else if c == '\\' { escaped = true; } else if c == '"' { quoted = false; }
        } else if c == '"' {
            quoted = true;
        } else if c == ' ' {
            if let Some(a) = start.take() { out.push(&s[a..i]); }
        }
    }
    if let Some(a) = start { out.push(&s[a..]); }
    out
}

fn unquote(t: &str) -> String {
    let Some(inner) = t.strip_prefix('"') else { return t.to_string() };
    let inner = inner.strip_suffix('"').unwrap_or(inner);
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' { if let Some(n) = chars.next() { out.push(n); } } else { out.push(c); }
    }
    out
}

/// `grain` is `assets/materials/grain.j3mat`; a value with a `/` or `.` is a path as it is.
fn expand(value: &str, folder: &str, ext: &str) -> String {
    if value.is_empty() || value.contains('/') || value.contains('.') { value.into() } else { format!("{folder}{value}{ext}") }
}

fn vec3(s: &str, uniform: bool) -> Option<[f64; 3]> {
    let v: Vec<f64> = s.split(',').map(float).collect();
    match v.len() {
        3 => Some([v[0], v[1], v[2]]),
        1 if uniform => Some([v[0]; 3]),
        _ => None,
    }
}

/// `"text"` is a string, `#7` an object, a number with a `.` a float, any other an integer.
fn literal(v: &str) -> (char, String) {
    if v.starts_with('"') { ('s', unquote(v)) }
    else if let Some(id) = v.strip_prefix('#') { ('r', id.into()) }
    else if v.contains('.') { ('f', v.into()) }
    else { (' ', v.into()) }
}

const SHAPES: [&str; 9] = ["cube", "sphere", "cylinder", "cone", "capsule", "torus", "quad", "plane", "sky"];

fn is_data_line(line: &str) -> bool {
    let first: String = line.chars().take_while(|c| !matches!(c, ' ' | '=' | '"')).collect();
    let rest = &line[first.len()..];
    !first.is_empty() && (rest.starts_with('=') || (rest.len() > 2 && rest.starts_with(' ') && rest[1..].starts_with(['f', 's', 'r']) && rest[2..].starts_with('=')))
}

fn parse_v2(text: &str) -> (Vec<Object>, Option<String>) {
    let mut objects: Vec<Object> = Vec::new();
    let mut error: Option<String> = None;
    let mut open = 0usize;
    let mut stack = [0i64; 64];
    let mut auto = 1;
    for line in text.lines() {
        let body = line.trim_start_matches(' ');
        if let Some((_, id)) = body.rsplit_once(" #") { auto = auto.max(int(id) + 1); }
    }
    for (n, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        let body = line.trim_start_matches(' ');
        let indent = line.len() - body.len();
        let mut fail = |msg: String| { if error.is_none() { error = Some(format!("line {}: {msg}", n + 1)); } };
        if body.is_empty() || body.starts_with("//") || (n == 0 && body.starts_with("j3scene")) { continue; }
        if indent == 0 && body.starts_with("env ") && body[4..].split(' ').next().is_some_and(|t| t.contains('=')) { continue; }
        if indent == 0 && is_data_line(body) { continue; }
        let mut depth = indent / 2;
        if indent % 2 != 0 { fail("indent must be a multiple of two spaces".into()); depth = 0; }
        else if depth > open || depth >= 64 { fail("indented more than one level below its parent".into()); depth = 0; }
        let toks = tokens(body);
        let (id_tok, body_toks) = match toks.last() {
            Some(t) if t.len() > 1 && t.starts_with('#') && t[1..].bytes().all(|b| b.is_ascii_digit()) => (Some(int(&t[1..])), &toks[..toks.len() - 1]),
            _ => (None, &toks[..]),
        };
        let mut o = Object::blank(objects.len(), n + 1);
        o.id = id_tok.unwrap_or_else(|| { auto += 1; auto - 1 });
        o.parent = if depth > 0 { stack[depth - 1] } else { 0 };
        stack[depth] = o.id;
        open = depth + 1;
        let Some(name) = body_toks.first() else { continue };
        o.name = unquote(name);
        match body_toks.get(1) {
            Some(&"group") => o.kind = GROUP,
            Some(&"mesh") => o.kind = MESH,
            Some(&"place") => o.kind = INSTANCE,
            Some(&"light") => o.kind = LIGHT,
            Some(&"camera") => o.kind = CAMERA,
            Some(w) if SHAPES.contains(w) => { o.kind = MESH; o.asset = format!("builtin:{w}"); }
            Some(w) => fail(format!("unknown kind {w}")),
            None => fail("object needs a kind after its name".into()),
        }
        for t in body_toks.iter().skip(2) {
            let eq = if t.starts_with('"') { None } else { t.find('=') };
            let Some(eq) = eq else {
                if o.kind == MESH { o.asset = unquote(t); } else if o.kind == INSTANCE { o.scene = unquote(t); }
                continue;
            };
            let (key, value) = (&t[..eq], &t[eq + 1..]);
            let text = || unquote(value);
            match key {
                "pos" => o.pos = vec3(value, false).unwrap_or(o.pos),
                "rot" => { if let Some(d) = vec3(value, false) { o.rot = d.map(f64::to_radians); } }
                "scale" => o.scale = vec3(value, true).unwrap_or(o.scale),
                "tint" => o.tint = vec3(value, true).unwrap_or(o.tint),
                "color" => o.color = vec3(value, true).unwrap_or(o.color),
                "range" => o.range = float(value),
                "fov" => o.fov = float(value),
                "near" => o.near = float(value),
                "far" => o.far = float(value),
                "opacity" => o.opacity = float(value),
                "material" => o.material = expand(&text(), "assets/materials/", ".j3mat"),
                "fx" | "effects" => o.effects = text().split('|').map(|p| expand(p, "assets/fx/", ".j3fx")).collect::<Vec<_>>().join("|"),
                "texture" => o.texture = text(),
                "script" => o.script = text(),
                "tags" => o.tags = text(),
                "asset" => o.asset = text(),
                "scene" => o.scene = text(),
                "doubleSided" => o.double_sided = int(value) != 0,
                "alpha" => o.alpha = match value { "cutout" => 1, "blend" => 2, "opaque" => 0, v => int(v) },
                k if k.starts_with("param.") || k.starts_with("fx.") => {
                    let (kind, value) = literal(value);
                    let field = k.strip_prefix("param.").unwrap_or(k);
                    o.params.push(Param { field: field.into(), kind, value, line: n + 1 });
                }
                k if k.starts_with("prop.") || k.starts_with("over.") => {}
                k => fail(format!("unknown key {k}")),
            }
        }
        objects.push(o);
    }
    (objects, error)
}

/// `value` as the short name of `<folder><name><ext>`, when it is one.
fn short_of<'a>(value: &'a str, folder: &str, ext: &str) -> Option<&'a str> {
    let mid = value.strip_prefix(folder)?.strip_suffix(ext)?;
    (!mid.is_empty() && !mid.contains('/') && !mid.contains('.')).then_some(mid)
}

fn quoted(v: &str) -> String {
    if v.contains([' ', '"', '\t']) { format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")) } else { v.to_string() }
}

/// A v2 file with every value `old` (a project path) changed to `new`: a mesh or scene path, a
/// `key=value` path, or the short form of a material or effect. None when nothing matched.
pub fn rename_v2(text: &str, old: &str, new: &str) -> Option<String> {
    let mut hit = false;
    let mut out = Vec::new();
    for line in text.lines() {
        let body = line.trim_start_matches(' ');
        if body.is_empty() || body.starts_with("//") || (line.len() == body.len() && is_data_line(body)) {
            out.push(line.to_string());
            continue;
        }
        let indent = &line[..line.len() - body.len()];
        let mut toks: Vec<String> = Vec::new();
        let mut changed = false;
        for (i, t) in tokens(body).into_iter().enumerate() {
            let (key, value) = match (i, if t.starts_with('"') { None } else { t.find('=') }) {
                (0, _) | (1, _) => ("", None),
                (_, Some(eq)) => (&t[..eq], Some(unquote(&t[eq + 1..]))),
                (_, None) => ("", Some(unquote(t))),
            };
            let Some(value) = value else { toks.push(t.to_string()); continue };
            let replaced = if value == old {
                Some(new.to_string())
            } else if key == "material" && short_of(old, "assets/materials/", ".j3mat") == Some(value.as_str()) {
                Some(short_of(new, "assets/materials/", ".j3mat").unwrap_or(new).to_string())
            } else if key == "fx" && value.split('|').any(|p| short_of(old, "assets/fx/", ".j3fx") == Some(p) || p == old) {
                Some(value.split('|').map(|p| {
                    if p == old { new } else if short_of(old, "assets/fx/", ".j3fx") == Some(p) { short_of(new, "assets/fx/", ".j3fx").unwrap_or(new) } else { p }
                }).collect::<Vec<_>>().join("|"))
            } else {
                None
            };
            match replaced {
                Some(v) => {
                    changed = true;
                    toks.push(if key.is_empty() { quoted(&v) } else { format!("{key}={}", quoted(&v)) });
                }
                None => toks.push(t.to_string()),
            }
        }
        if changed {
            hit = true;
            out.push(format!("{indent}{}", toks.join(" ")));
        } else {
            out.push(line.to_string());
        }
    }
    hit.then(|| out.join("\n") + "\n")
}

fn quote_if(v: &str, path: bool) -> String {
    if v.contains([' ', '"', '\t']) || (path && v.contains('=')) { quote(v) } else { v.to_string() }
}

fn quote(v: &str) -> String { format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")) }

fn name_text(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '"', '=', '\t']) || s.starts_with('#') || s.starts_with("//") { quote(s) } else { s.into() }
}

fn short(value: &str, folder: &str, ext: &str) -> String { short_of(value, folder, ext).unwrap_or(value).to_string() }

fn vec_text(v: [f64; 3], uniform: bool) -> String {
    let t = v.map(num);
    if uniform && t[0] == t[1] && t[1] == t[2] { t[0].clone() } else { t.join(",") }
}

/// The object line (without indent or note) the editor's writer would give `o`. Params and
/// per-copy overrides are not kept.
fn object_text(o: &Object) -> String {
    let d = Object::blank(0, 0);
    let mut out = name_text(&o.name);
    let kind = match o.kind {
        MESH if o.asset.strip_prefix("builtin:").is_some_and(|s| SHAPES.contains(&s)) => o.asset[8..].to_string(),
        MESH => if o.asset.is_empty() { "mesh".into() } else { format!("mesh {}", quote_if(&o.asset, true)) },
        INSTANCE => if o.scene.is_empty() { "place".into() } else { format!("place {}", quote_if(&o.scene, true)) },
        k => kind_word(k).into(),
    };
    out += &format!(" {kind}");
    let mut put = |k: &str, v: String| { out += &format!(" {k}={v}"); };
    if o.pos.map(num) != ["0"; 3].map(String::from) { put("pos", vec_text(o.pos, false)); }
    let rot = o.rot.map(|r| trimmed(deg(r), 3));
    if rot.iter().any(|r| r != "0") { put("rot", rot.join(",")); }
    if o.scale.map(num) != d.scale.map(num) { put("scale", vec_text(o.scale, true)); }
    let (mesh, inst) = (o.kind == MESH, o.kind == INSTANCE);
    if mesh && !o.material.is_empty() { put("material", quote_if(&short(&o.material, "assets/materials/", ".j3mat"), false)); }
    if mesh && !o.texture.is_empty() { put("texture", quote_if(&o.texture, false)); }
    if (mesh || inst) && o.tint.map(num) != d.tint.map(num) { put("tint", vec_text(o.tint, true)); }
    if o.kind == LIGHT {
        if o.color.map(num) != d.color.map(num) { put("color", vec_text(o.color, false)); }
        if num(o.range) != num(d.range) { put("range", num(o.range)); }
    }
    if o.kind == CAMERA {
        for (k, v, base) in [("fov", o.fov, d.fov), ("near", o.near, d.near), ("far", o.far, d.far)] {
            if num(v) != num(base) { put(k, num(v)); }
        }
        if !o.effects.is_empty() {
            let list: Vec<String> = o.effects.split('|').map(|p| short(p, "assets/fx/", ".j3fx")).collect();
            put("fx", quote_if(&list.join("|"), false));
        }
    }
    if mesh && o.alpha != 0 {
        put("alpha", ["opaque", "cutout", "blend"][o.alpha.clamp(0, 2) as usize].into());
        if num(o.opacity) != "1" { put("opacity", num(o.opacity)); }
    }
    if mesh && o.double_sided { put("doubleSided", "1".into()); }
    if !o.script.is_empty() { put("script", quote_if(&o.script, false)); }
    if !o.tags.is_empty() { put("tags", quote_if(&o.tags, false)); }
    out + &format!(" #{}", o.id)
}

/// `objects` as a v2 file in the editor's layout: tree order, one blank line before a top-level
/// object with children (and before the next one), defaults left out.
pub fn write_v2(objects: &[Object], next: i64) -> String {
    use std::collections::HashMap;
    let ids: HashMap<i64, usize> = objects.iter().enumerate().map(|(i, o)| (o.id, i)).collect();
    let mut kids: HashMap<i64, Vec<usize>> = HashMap::new();
    let mut roots = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        if ids.contains_key(&o.parent) && o.parent != o.id { kids.entry(o.parent).or_default().push(i); } else { roots.push(i); }
    }
    fn walk(objects: &[Object], kids: &std::collections::HashMap<i64, Vec<usize>>, i: usize, depth: usize, out: &mut String, seen: &mut Vec<bool>) {
        seen[i] = true;
        out.push_str(&format!("{}{}\n", "  ".repeat(depth), object_text(&objects[i])));
        for &c in kids.get(&objects[i].id).into_iter().flatten() {
            if !seen[c] { walk(objects, kids, c, depth + 1, out, seen); }
        }
    }
    let mut out = format!("j3scene 2 next={next}\n");
    let mut seen = vec![false; objects.len()];
    let (mut first, mut prev_kids) = (true, false);
    for r in roots.into_iter().chain(0..objects.len()) {
        if seen[r] { continue; }
        let has_kids = kids.get(&objects[r].id).is_some_and(|k| !k.is_empty());
        if first || prev_kids || has_kids { out.push('\n'); }
        (first, prev_kids) = (false, has_kids);
        walk(objects, &kids, r, 0, &mut out, &mut seen);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = "scene.version=1\nnode.0.id=1\nnode.0.parent=0\nnode.0.kind=1\nnode.0.name s=Big Door\nnode.0.asset s=builtin:cube\nnode.0.script s=door.j3s\nnode.0.ry f=1.570796\nnode.0.sx f=2.0\nnode.0.sy f=2.0\nnode.0.sz f=2.0\nnode.0.fov f=75.0\nnode.1.id=2\nnode.1.parent=1\nnode.1.kind=0\nnode.1.name s=Child\nparam.1.speed f=3.0\nparam.1.label s=a b\n";
    const V2: &str = "j3scene 2 next=3\n\n\"Big Door\" cube rot=0,90,0 scale=2 script=door.j3s param.label=\"a b\" param.speed=3.0 #1\n  Child group #2\n";

    #[test]
    fn v1_and_v2_read_alike() {
        let (a, ea) = parse(V1);
        let (b, eb) = parse(V2);
        assert!(ea.is_none() && eb.is_none(), "{ea:?} {eb:?}");
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        for (x, y) in a.iter().zip(&b) {
            assert_eq!((x.id, x.parent, &x.name), (y.id, y.parent, &y.name));
            let (mut fx, mut fy) = (x.fields(), y.fields());
            fx.sort();
            fy.sort();
            assert_eq!(fx, fy);
        }
        assert_eq!(b[0].params.iter().map(|p| (p.field.as_str(), p.kind)).collect::<Vec<_>>(), [("label", 's'), ("speed", 'f')]);
    }

    #[test]
    fn writing_v2_gives_the_canonical_layout() {
        let canon = "j3scene 2 next=9\n\n\"Big Door\" cube pos=1.5,0,0 rot=0,90,0 scale=2 material=grain tint=0.5 script=door.j3s #1\n  Lamp light pos=0,3,0 range=12 #2\n\nKiosk place scenes/kiosk.j3scene #8\n";
        let (objects, error) = parse(canon);
        assert!(error.is_none());
        assert_eq!(write_v2(&objects, 9), canon);
    }

    #[test]
    fn renaming_an_asset_rewrites_v2_references() {
        let text = "j3scene 2 next=4\nA mesh assets/a.j3mesh material=grain #1\nB place assets/a.j3mesh #2\n  C cube material=\"assets/materials/other.j3mat\" #3\n";
        let out = rename_v2(text, "assets/a.j3mesh", "assets/b dir/a.j3mesh").unwrap();
        assert!(out.contains("A mesh \"assets/b dir/a.j3mesh\" material=grain #1"), "{out}");
        assert!(out.contains("B place \"assets/b dir/a.j3mesh\" #2"), "{out}");
        let out = rename_v2(text, "assets/materials/grain.j3mat", "assets/materials/sand.j3mat").unwrap();
        assert!(out.contains("material=sand"), "{out}");
        assert!(rename_v2(text, "assets/none.j3mesh", "x").is_none());
    }

    #[test]
    fn v2_mistakes_name_their_line() {
        assert!(parse("j3scene 2 next=2\nA group #1\n   B cube #2\n").1.unwrap().starts_with("line 3: indent"));
        assert!(parse("j3scene 2 next=2\nA cube posx=1 #1\n").1.unwrap().contains("unknown key posx"));
        assert!(parse("j3scene 2 next=2\nA blob #1\n").1.unwrap().contains("unknown kind blob"));
    }
}
