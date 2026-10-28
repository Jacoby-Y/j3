use super::*;

pub(super) fn const_docs(cx: &IrCx, mods: &[ModData]) -> Vec<Export> {
    let mut out = Vec::new();
    for m in mods {
        let module = join_module(cx, &m.names);
        for it in &m.ast.items {
            let Item::Const(c) = it else {
                continue;
            };
            out.push(Export {
                module: module.clone(),
                name: c.name.name.clone(),
                kind: ExportKind::Const,
                doc: c.doc.clone(),
            });
        }
    }
    out
}

pub(super) fn program_exports(cx: &IrCx, mods: &[ModData]) -> Vec<Export> {
    let mut out = Vec::new();
    for m in mods {
        let module = join_module(cx, &m.names);
        let mut names: Vec<&String> = m.exports.iter().collect();
        names.sort();
        for name in names {
            let Some(kind) = export_kind(cx, m, name) else {
                continue;
            };
            out.push(Export {
                module: module.clone(),
                name: name.clone(),
                kind,
                doc: item_doc(m, name),
            });
        }
    }
    out
}

pub(super) fn item_doc(m: &ModData, name: &str) -> String {
    for it in &m.ast.items {
        match it {
            Item::Fn(f) if f.name.name == name => return f.doc.clone(),
            Item::Struct(s) if s.name.name == name => return s.doc.clone(),
            Item::Enum(e) if e.name.name == name => return e.doc.clone(),
            Item::Const(c) if c.name.name == name => return c.doc.clone(),
            _ => {}
        }
    }
    String::new()
}

pub(super) fn export_kind(cx: &IrCx, m: &ModData, name: &str) -> Option<ExportKind> {
    if m.funcs.contains_key(name) {
        return Some(ExportKind::Fn);
    }
    if let Some(ty) = m.types.get(name) {
        match cx.ty_kind(*ty) {
            TyKind::Struct(_) => return Some(ExportKind::Struct),
            TyKind::Enum(_) => return Some(ExportKind::Enum),
            _ => {}
        }
    }
    if m.struct_temps.contains_key(name) {
        return Some(ExportKind::Struct);
    }
    if m.enum_temps.contains_key(name) {
        return Some(ExportKind::Enum);
    }
    if m.consts.contains_key(name) {
        return Some(ExportKind::Const);
    }
    None
}

pub(super) fn join_module(cx: &IrCx, names: &[Name]) -> String {
    let mut s = String::new();
    for (i, n) in names.iter().enumerate() {
        if i > 0 {
            s.push('.');
        }
        s.push_str(cx.name_str(*n));
    }
    s
}

pub(super) fn share_methods(mods: &mut [ModData]) {
    let mut all = HashMap::new();
    let mut temps = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.methods {
            all.insert(k.clone(), *v);
        }
        for (k, v) in &m.method_temps {
            temps.insert(k.clone(), v.clone());
        }
    }
    for m in mods.iter_mut() {
        for (k, v) in &all {
            m.methods.insert(k.clone(), *v);
        }
        for (k, v) in &temps {
            m.method_temps.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
}

pub(super) fn collect_exports(mods: &mut [ModData], i: usize) {
    for it in &mods[i].ast.items {
        let (name, vis_pub) = match it {
            Item::Fn(f) => (&f.name.name, f.vis_pub),
            Item::Struct(s) => (&s.name.name, s.vis_pub),
            Item::Enum(e) => (&e.name.name, e.vis_pub),
            Item::Const(c) => (&c.name.name, c.vis_pub),
            _ => continue,
        };
        if vis_pub {
            mods[i].exports.insert(name.clone());
        }
    }
}
