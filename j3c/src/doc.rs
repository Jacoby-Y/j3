//! `j3c doc` — public stdlib surface from the same parser. No second frontend.

use std::path::{Path, PathBuf};

use crate::parse::{self, EnumItem, FieldItem, File, FnItem, Ident, Item, StructItem, Type, TypeKind};
use crate::span::FileId;
use crate::stdlib::stdlib_dir;

const HIDDEN: &[&str] = &["game_headless.j3", "sys_windows.j3", "sys_macos.j3"];

pub fn cmd(rest: &[String]) -> i32 {
    let mut args: Vec<&str> = Vec::new();
    for a in rest {
        if a == "--help" || a == "-h" {
            println!("usage: j3c doc [module] [item]");
            println!("    j3c doc                 list stdlib modules");
            println!("    j3c doc j3.app          public items and /// comments");
            println!("    j3c doc j3.app create   one item");
            println!("    j3c doc app             same as j3.app");
            println!("    j3c doc j3ui           `@j3ui` component syntax");
            return 0;
        }
        if a.starts_with('-') {
            eprintln!("j3c: unknown flag `{a}`");
            return 1;
        }
        args.push(a.as_str());
    }
    let text = match args.as_slice() {
        [] => index(),
        ["j3ui"] | ["@j3ui"] => Ok(j3ui_page()),
        [query] => module(query, None),
        [query, item] => module(query, Some(item)),
        _ => Err("usage: j3c doc [module] [item]".to_string()),
    };
    match text {
        Ok(s) => {
            print!("{s}");
            if !s.ends_with('\n') {
                println!();
            }
            0
        }
        Err(e) => {
            eprintln!("j3c: {e}");
            1
        }
    }
}

fn j3ui_page() -> String {
    String::from(
        r#"`@j3ui` — UI components

`@j3ui name(ui: &Ui, ...) { }` is a function. The first parameter is the `Ui` from `Ui.begin`. The body cannot see locals from another function. Call it like a function. A component inside another is that call. Learn the widgets with `j3c doc j3.ui`.

```
@j3ui shopRow(ui: &Ui, w: &World, kind: int, name: string, cost: int, maxed: bool) {
    row() {
        label(`{name}   {cost} leaves`);
        button("buy") {
            buy(w, kind);
        }
    }
}

@j3ui shop(ui: &Ui, w: &World) {
    panel(280, 248) {
        label("shop");
        shopRow(ui, w, 0, "grab speed", costSpeed(w.speed), w.speed >= 5);
    }
}

fn draw(w: &World) {
    let ui = Ui.begin(w.font, 16, 188);
    shop(ui, w);
}
```

A bare widget name gets the receiver (`label` becomes `ui.label`). Closers:

- `panel`, `row`, `scroll`, `card` — the call, the body, then `end`
- `pushId` — the call, the body, then `popId`
- `button` — `if ui.button(...) { body }`

`if`, `for`, `let`, and other calls stay J3. A `{ }` on any other widget is an error.

ASCII text is drawn from a glyph atlas, so a changing digit does not allocate a new string texture. `scroll(id, height)` skips rows that sit fully outside the window. `j3c doc j3.game drawText` describes the cache.
"#,
    )
}

pub fn index() -> Result<String, String> {
    let dir = stdlib_dir().join("j3");
    let mut files = read_dir_j3(&dir)?;
    files.sort();
    let mut out = String::from("stdlib modules (`j3c doc <module>`):\n\n");
    for path in files {
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if HIDDEN.iter().any(|h| *h == name) {
            continue;
        }
        let src = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let file = parse_file(&path, &src)?;
        out.push_str(&file.module_key());
        out.push('\n');
        push_doc(&mut out, &file.doc, "    ");
        out.push('\n');
    }
    Ok(out)
}

pub fn module(query: &str, item: Option<&str>) -> Result<String, String> {
    let (module, from_path) = split_query(query)?;
    let item_owned = item.map(|s| s.to_string()).or(from_path);
    let item = item_owned.as_deref();
    let path = find_module(&module)?;
    let src = std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let file = parse_file(&path, &src)?;
    let (text, hits) = render(&file, &file.doc, item);
    if item.is_some() && hits == 0 {
        return Err(format!(
            "no public item `{}` in {module}\n  = help: `j3c doc {module}`",
            item.unwrap_or("")
        ));
    }
    Ok(text)
}

fn split_query(query: &str) -> Result<(String, Option<String>), String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("missing module\n  = help: `j3c doc`".to_string());
    }
    let mut parts: Vec<&str> = query.split('.').filter(|s| !s.is_empty()).collect();
    if parts.first() != Some(&"j3") {
        parts.insert(0, "j3");
    }
    if parts.len() < 2 {
        return Err(format!("no stdlib module `{query}`\n  = help: `j3c doc`"));
    }
    if parts.len() == 2 {
        return Ok((format!("{}.{}", parts[0], parts[1]), None));
    }
    let module = format!("{}.{}", parts[0], parts[1]);
    let item = parts[2..].join(".");
    Ok((module, Some(item)))
}

fn find_module(module: &str) -> Result<PathBuf, String> {
    let parts: Vec<&str> = module.split('.').collect();
    if parts.len() != 2 || parts[0] != "j3" || parts[1].is_empty() {
        return Err(format!("no stdlib module `{module}`\n  = help: `j3c doc`"));
    }
    let path = stdlib_dir().join("j3").join(format!("{}.j3", parts[1]));
    if path.is_file() {
        return Ok(path);
    }
    Err(format!("no stdlib module `{module}`\n  = help: `j3c doc`"))
}

fn read_dir_j3(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let ents = std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))?;
    let mut out = Vec::new();
    for ent in ents.flatten() {
        let p = ent.path();
        if p.extension().and_then(|e| e.to_str()) == Some("j3") {
            out.push(p);
        }
    }
    Ok(out)
}

fn parse_file(path: &Path, src: &str) -> Result<File, String> {
    parse::parse(src, FileId(1)).map_err(|e| parse::format_error(&path.display().to_string(), src, &e))
}

fn render(file: &File, blur: &str, filter: Option<&str>) -> (String, usize) {
    let mut out = String::new();
    out.push_str("module ");
    out.push_str(&file.module_key());
    out.push('\n');
    if !blur.is_empty() {
        push_doc(&mut out, blur, "    ");
    }
    out.push('\n');
    let mut hits = 0;
    for item in &file.items {
        hits += render_item(&mut out, item, filter);
    }
    (out, hits)
}

fn render_item(out: &mut String, item: &Item, filter: Option<&str>) -> usize {
    match item {
        Item::Fn(f) if f.vis_pub && keep(&f.name.name, filter) => {
            push_fn(out, f, None, "");
            1
        }
        Item::Const(c) if c.vis_pub && keep(&c.name.name, filter) => {
            out.push_str("const ");
            out.push_str(&c.name.name);
            if let Some(ty) = &c.ty {
                out.push_str(": ");
                out.push_str(&ty_str(ty));
            }
            out.push('\n');
            push_doc(out, &c.doc, "    ");
            out.push('\n');
            1
        }
        Item::Struct(s) if s.vis_pub => render_struct(out, s, filter),
        Item::Enum(e) if e.vis_pub => render_enum(out, e, filter),
        _ => 0,
    }
}

fn render_struct(out: &mut String, s: &StructItem, filter: Option<&str>) -> usize {
    let whole = filter == Some(s.name.name.as_str());
    let show_fields: Vec<&FieldItem> = s
        .fields
        .iter()
        .filter(|f| whole || filter.is_none() || filter == Some(f.name.name.as_str()))
        .collect();
    let show_methods: Vec<&FnItem> = s
        .methods
        .iter()
        .filter(|m| whole || filter.is_none() || filter == Some(m.name.name.as_str()))
        .collect();
    if filter.is_some() && !whole && show_fields.is_empty() && show_methods.is_empty() {
        return 0;
    }
    let owner = format!("{}{}", s.name.name, type_params(&s.type_params));
    out.push_str("struct ");
    out.push_str(&owner);
    out.push('\n');
    if whole || filter.is_none() {
        push_doc(out, &s.doc, "    ");
    }
    let n_fields = show_fields.len();
    let n_methods = show_methods.len();
    for f in &show_fields {
        out.push_str("    ");
        if f.embedded {
            out.push_str("use ");
        }
        out.push_str(&f.name.name);
        out.push_str(": ");
        out.push_str(&ty_str(&f.ty));
        if f.default.is_some() {
            out.push_str(" = ...");
        }
        out.push('\n');
        push_doc(out, &f.doc, "        ");
    }
    for m in &show_methods {
        push_fn(out, m, Some(&owner), "    ");
    }
    out.push('\n');
    1 + n_fields + n_methods
}

fn render_enum(out: &mut String, e: &EnumItem, filter: Option<&str>) -> usize {
    let whole = filter == Some(e.name.name.as_str());
    let variants: Vec<_> = e
        .variants
        .iter()
        .filter(|v| whole || filter.is_none() || filter == Some(v.name.name.as_str()))
        .collect();
    if filter.is_some() && !whole && variants.is_empty() {
        return 0;
    }
    out.push_str("enum ");
    out.push_str(&e.name.name);
    out.push_str(&type_params(&e.type_params));
    out.push('\n');
    if whole || filter.is_none() {
        push_doc(out, &e.doc, "    ");
    }
    for v in &variants {
        out.push_str("    ");
        out.push_str(&v.name.name);
        if let Some(ty) = &v.payload {
            out.push('(');
            out.push_str(&ty_str(ty));
            out.push(')');
        }
        out.push('\n');
        push_doc(out, &v.doc, "        ");
    }
    out.push('\n');
    1 + variants.len()
}

fn push_fn(out: &mut String, f: &FnItem, owner: Option<&str>, indent: &str) {
    out.push_str(indent);
    out.push_str("fn ");
    if let Some(owner) = owner {
        out.push_str(owner);
        out.push('.');
    }
    out.push_str(&f.name.name);
    out.push_str(&type_params(&f.type_params));
    out.push('(');
    for (i, p) in f.params.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(&p.name.name);
        out.push_str(": ");
        out.push_str(&ty_str(&p.ty));
    }
    out.push(')');
    if let Some(ret) = &f.ret {
        out.push_str(": ");
        out.push_str(&ty_str(ret));
    }
    out.push('\n');
    let doc_indent = format!("{indent}    ");
    push_doc(out, &f.doc, &doc_indent);
}

fn keep(name: &str, filter: Option<&str>) -> bool {
    filter.map(|f| f == name).unwrap_or(true)
}

fn type_params(ps: &[Ident]) -> String {
    if ps.is_empty() {
        return String::new();
    }
    let mut s = String::from("<");
    for (i, p) in ps.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&p.name);
    }
    s.push('>');
    s
}

fn ty_str(t: &Type) -> String {
    match &t.kind {
        TypeKind::Named(n) => n.clone(),
        TypeKind::Generic { name, args } => {
            let args: Vec<String> = args.iter().map(ty_str).collect();
            format!("{name}<{}>", args.join(", "))
        }
        TypeKind::Ref(inner) => format!("&{}", ty_str(inner)),
        TypeKind::Array { len, elem } => format!("[{len}]{}", ty_str(elem)),
        TypeKind::Slice(elem) => format!("[]{}", ty_str(elem)),
        TypeKind::Fn { params, ret } => {
            let params: Vec<String> = params.iter().map(ty_str).collect();
            match ret {
                Some(r) => format!("fn({}): {}", params.join(", "), ty_str(r)),
                None => format!("fn({})", params.join(", ")),
            }
        }
    }
}

fn push_doc(out: &mut String, doc: &str, indent: &str) {
    for line in doc.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        out.push_str(indent);
        out.push_str(line);
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_lists_app_and_gfx() {
        let text = index().expect("index");
        assert!(text.contains("j3.app\n"), "{text}");
        assert!(text.contains("Bevy-shaped runner"), "{text}");
        assert!(text.contains("j3.gfx\n"), "{text}");
        assert!(text.contains("j3.mesh\n"), "{text}");
        assert_eq!(text.matches("j3.game\n").count(), 1, "{text}");
        assert!(text.contains("game_headless.j3 is the test stand-in"), "{text}");
    }

    #[test]
    fn app_create_is_documented() {
        let text = module("j3.app", Some("create")).expect("doc");
        assert!(text.contains("fn App<W>.create("), "{text}");
        assert!(text.contains("Boots"), "{text}");
        assert!(!text.contains("fn App<W>.run("), "{text}");
    }

    #[test]
    fn short_name_and_dotted_item() {
        let a = module("app", None).expect("app");
        assert!(a.contains("struct App<W>"), "{a}");
        let b = module("j3.math.sqrt", None).expect("sqrt");
        assert!(b.contains("fn sqrt(x: f32): f32"), "{b}");
    }

    #[test]
    fn j3ui_page_lists_the_contract() {
        let text = j3ui_page();
        assert!(text.contains("@j3ui shopRow"), "{text}");
        assert!(text.contains("cannot see locals"), "{text}");
        assert!(text.contains("scroll"), "{text}");
    }

    #[test]
    fn missing_item_errors() {
        let err = module("j3.app", Some("nope")).unwrap_err();
        assert!(err.contains("nope"), "{err}");
    }
}
