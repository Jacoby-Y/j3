use super::*;

pub(super) fn render_module(m: &Module, item: Option<&str>) -> Result<String, String> {
    let filter = Filter::parse(item);
    if let Filter::Name(name) = filter {
        let quals: Vec<String> = symbols(&m.file)
            .into_iter()
            .filter(|s| s.qual == name || s.qual.ends_with(&format!(".{name}")))
            .map(|s| s.qual)
            .collect();
        if quals.len() > 1 {
            let mut out = format!("`{name}` names more than one item in {}:\n", m.key);
            for q in quals {
                out.push_str(&format!("    j3c doc {} {q}\n", m.key));
            }
            return Ok(out);
        }
    }
    let cx = Cx { src: &m.src, origin: &m.origin, parts: &m.parts, embedded: m.embedded, filter };
    let mut out = format!("module {}\n", m.key);
    if m.scan.is_some() {
        out.push_str("    Made by j3c from this project's scenes when a program uses it; never written to disk.\n");
    }
    if matches!(filter, Filter::All) {
        push_doc(&mut out, &m.file.doc, "    ");
    }
    out.push('\n');
    let mut hits = 0;
    for it in &m.file.items {
        hits += render_item(&mut out, &cx, it);
    }
    if let (Some(scan), Filter::All) = (&m.scan, filter) {
        out.push_str(&attached(scan));
    }
    if item.is_some() && hits == 0 {
        return Err(format!(
            "no public item `{}` in {}\n  = help: `j3c doc {} --short` lists its names",
            item.unwrap_or(""),
            m.key,
            m.key
        ));
    }
    Ok(out)
}

/// The scripts module's source: which scene objects carry which script.
pub(super) fn attached(scan: &scene_scripts::Scan) -> String {
    let scripts = scan.scripts();
    if scripts.is_empty() {
        return "No scene attaches a script yet (`script=scripts/x.j3s` on an object line).\n".to_string();
    }
    let mut out = String::from(
        "attached scripts (one instance per object; find one with\n    `const Spin = @script(\"scripts/spin.j3s\");` then `scene.script::<Spin>(node)`)\n",
    );
    for script in scripts {
        out.push_str(&format!("    {script}\n"));
        for a in scan.attachments.iter().filter(|a| a.script == script) {
            out.push_str(&format!("        {}:{}  object #{}\n", a.scene, a.line, a.id));
        }
    }
    out
}

#[derive(Clone, Copy)]
pub(super) enum Filter<'a> {
    All,
    /// A top-level item, a type, or any member with this name.
    Name(&'a str),
    /// `Type.member`.
    Member(&'a str, &'a str),
}

impl<'a> Filter<'a> {
    fn parse(item: Option<&'a str>) -> Filter<'a> {
        match item {
            None => Filter::All,
            Some(s) => match s.split_once('.') {
                Some((t, m)) => Filter::Member(t, m),
                None => Filter::Name(s),
            },
        }
    }

    fn top(self, name: &str) -> bool {
        match self {
            Filter::All => true,
            Filter::Name(n) => n == name,
            Filter::Member(..) => false,
        }
    }

    /// Whether a type shows all of itself, and whether it shows `member`.
    fn whole(self, ty: &str) -> bool {
        matches!(self, Filter::All) || matches!(self, Filter::Name(n) if n == ty)
    }

    fn member(self, ty: &str, member: &str) -> bool {
        self.whole(ty)
            || matches!(self, Filter::Name(n) if n == member)
            || matches!(self, Filter::Member(t, m) if t == ty && m == member)
    }
}

/// What rendering needs besides the item: the source for defaults and `at` lines.
pub(super) struct Cx<'a> {
    pub(super) src: &'a str,
    pub(super) origin: &'a str,
    pub(super) parts: &'a [(usize, String)],
    pub(super) embedded: bool,
    pub(super) filter: Filter<'a>,
}

impl Cx<'_> {
    /// `at file:line` under an item, only when one item was asked for.
    fn at(&self, out: &mut String, span: Span, indent: &str) {
        if matches!(self.filter, Filter::All) || self.origin.is_empty() {
            return;
        }
        let end = (span.start as usize).min(self.src.len());
        let (from, origin) = match self.parts.iter().rev().find(|(start, _)| *start <= end) {
            Some((start, origin)) => (*start, origin.as_str()),
            None => (0, self.origin),
        };
        let line = self.src[from..end].matches('\n').count() + 1;
        let note = if self.embedded { " (embedded in j3c)" } else { "" };
        out.push_str(&format!("{indent}at {origin}:{line}{note}\n"));
    }
}

/// A literal's text as written, or `...` when it is long.
pub(super) fn expr_text(src: &str, e: &Expr) -> String {
    let text = src.get(e.span.start as usize..e.span.end as usize).unwrap_or("").trim();
    if text.is_empty() || text.len() > 60 || text.contains('\n') {
        "...".to_string()
    } else {
        text.to_string()
    }
}

/// Generated helpers (`find__scripts_door`) are public to the compiler, not to readers.
pub(super) fn shown(name: &str) -> bool {
    !name.contains("__")
}

pub(super) fn render_item(out: &mut String, cx: &Cx, item: &Item) -> usize {
    match item {
        Item::Fn(f) if f.vis_pub && shown(&f.name.name) && cx.filter.top(&f.name.name) => {
            push_fn(out, f, None, "");
            cx.at(out, f.span, "    ");
            1
        }
        Item::Const(c) if c.vis_pub && shown(&c.name.name) && cx.filter.top(&c.name.name) => {
            out.push_str("const ");
            out.push_str(&c.name.name);
            if let Some(ty) = &c.ty {
                out.push_str(": ");
                out.push_str(&ty_str(ty));
            }
            out.push_str(&format!(" = {}\n", expr_text(cx.src, &c.value)));
            push_doc(out, &c.doc, "    ");
            cx.at(out, c.span, "    ");
            out.push('\n');
            1
        }
        Item::Struct(s) if s.vis_pub => render_struct(out, cx, s),
        Item::Enum(e) if e.vis_pub => render_enum(out, cx, e),
        _ => 0,
    }
}

pub(super) fn render_struct(out: &mut String, cx: &Cx, s: &StructItem) -> usize {
    let name = s.name.name.as_str();
    let whole = cx.filter.whole(name);
    let fields: Vec<&FieldItem> = s.fields.iter().filter(|f| cx.filter.member(name, &f.name.name)).collect();
    let methods: Vec<&FnItem> = s
        .methods
        .iter()
        .filter(|m| cx.filter.member(name, &m.name.name))
        .collect();
    if !whole && fields.is_empty() && methods.is_empty() {
        return 0;
    }
    let owner = format!("{}{}", name, type_params(&s.type_params));
    out.push_str("struct ");
    out.push_str(&owner);
    out.push('\n');
    if whole {
        push_doc(out, &s.doc, "    ");
        cx.at(out, s.span, "    ");
    }
    for f in &fields {
        out.push_str("    ");
        if f.embedded {
            out.push_str("use ");
        }
        out.push_str(&f.name.name);
        out.push_str(": ");
        out.push_str(&ty_str(&f.ty));
        if let Some(d) = &f.default {
            out.push_str(&format!(" = {}", expr_text(cx.src, d)));
        }
        out.push('\n');
        push_doc(out, &f.doc, "        ");
        if !whole {
            cx.at(out, f.span, "        ");
        }
    }
    for m in &methods {
        push_fn(out, m, Some(&owner), "    ");
        if !whole {
            cx.at(out, m.span, "        ");
        }
    }
    out.push('\n');
    1 + fields.len() + methods.len()
}

pub(super) fn render_enum(out: &mut String, cx: &Cx, e: &EnumItem) -> usize {
    let name = e.name.name.as_str();
    let whole = cx.filter.whole(name);
    let variants: Vec<_> = e.variants.iter().filter(|v| cx.filter.member(name, &v.name.name)).collect();
    if !whole && variants.is_empty() {
        return 0;
    }
    out.push_str("enum ");
    out.push_str(name);
    out.push_str(&type_params(&e.type_params));
    out.push('\n');
    if whole {
        push_doc(out, &e.doc, "    ");
        cx.at(out, e.span, "    ");
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
        if !whole {
            cx.at(out, v.span, "        ");
        }
    }
    out.push('\n');
    1 + variants.len()
}

/// `j3c doc <module> --short`: every public name on a screen or so.
pub(super) fn short_view(m: &Module) -> String {
    let mut out = format!("module {}", m.key);
    if let Some(first) = m.file.doc.lines().map(str::trim).find(|l| !l.is_empty()) {
        out.push_str(&format!(" — {first}"));
    }
    out.push_str("\n\n");
    let mut fns = Vec::new();
    let mut consts = Vec::new();
    let mut types = String::new();
    for item in &m.file.items {
        match item {
            Item::Fn(f) if f.vis_pub && shown(&f.name.name) => fns.push(f.name.name.clone()),
            Item::Const(c) if c.vis_pub && shown(&c.name.name) => consts.push(c.name.name.clone()),
            Item::Struct(s) if s.vis_pub => {
                let mut names: Vec<String> = s.fields.iter().map(|f| f.name.name.clone()).collect();
                names.extend(s.methods.iter().map(|f| format!("{}()", f.name.name)));
                types.push_str(&wrap(&format!("struct {}", s.name.name), &names));
            }
            Item::Enum(e) if e.vis_pub => {
                let names: Vec<String> = e.variants.iter().map(|v| v.name.name.clone()).collect();
                types.push_str(&wrap(&format!("enum {}", e.name.name), &names));
            }
            _ => {}
        }
    }
    if !fns.is_empty() {
        out.push_str(&wrap("fn", &fns));
    }
    if !consts.is_empty() {
        out.push_str(&wrap("const", &consts));
    }
    out.push_str(&types);
    out.push_str(&format!(
        "\nsignatures and comments: `j3c doc {0}`; one name: `j3c doc {0} <name>`\n",
        m.key
    ));
    out
}

/// `label: a b c` wrapped at 100 columns under a hanging indent.
pub(super) fn wrap(label: &str, names: &[String]) -> String {
    let mut out = format!("{label}:");
    let mut col = out.len();
    for n in names {
        if col + 1 + n.len() > 100 {
            out.push_str("\n   ");
            col = 3;
        }
        out.push(' ');
        out.push_str(n);
        col += 1 + n.len();
    }
    out.push('\n');
    out
}

pub(super) fn push_fn(out: &mut String, f: &FnItem, owner: Option<&str>, indent: &str) {
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

pub(super) fn type_params(ps: &[Ident]) -> String {
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

pub(super) fn ty_str(t: &Type) -> String {
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

/// `text` cut to about `n` chars at a word boundary.
pub(super) fn snippet(text: &str, n: usize) -> String {
    if text.chars().count() <= n {
        return text.to_string();
    }
    let cut: String = text.chars().take(n).collect();
    format!("{}…", cut.rsplit_once(' ').map_or(cut.as_str(), |(a, _)| a))
}

pub(super) fn push_doc(out: &mut String, doc: &str, indent: &str) {
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
