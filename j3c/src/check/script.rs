//! Lower script declarations to an ordinary struct, methods, and globals.

use std::collections::HashSet;
use std::path::Path;

use crate::parse::{
    self, ExprKind, FieldItem, Ident, Item, ModLetItem, Param, StructItem, Type, TypeKind,
};

use super::{CheckError, ModData, ParsedFile};

pub fn prepare(files: &mut [ParsedFile]) -> Result<(), CheckError> {
    for file in files {
        if !file.ast.is_script {
            continue;
        }
        let name = script_name(&file.path);
        let mut props = Vec::new();
        let mut state = Vec::new();
        let mut methods = Vec::new();
        let mut shared = Vec::new();
        let mut seen = HashSet::new();
        let mut has_props = false;
        let mut has_shared = false;
        for item in &file.ast.items {
            match item {
                Item::Props(b) => {
                    if has_props {
                        return Err(CheckError::new(
                            &file.path,
                            b.span,
                            "a script has one `props` block",
                        ));
                    }
                    has_props = true;
                    for f in &b.fields {
                        take_name(&mut seen, &file.path, &f.name, &name)?;
                        if let Some(value) = &f.default {
                            literal(&file.path, value, "prop default")?;
                        }
                        props.push(f.clone());
                    }
                }
                Item::Shared(b) => {
                    if has_shared {
                        return Err(CheckError::new(
                            &file.path,
                            b.span,
                            "a script has one `shared` block",
                        ));
                    }
                    has_shared = true;
                    for f in &b.fields {
                        take_name(&mut seen, &file.path, &f.name, &name)?;
                        if let Some(value) = &f.default {
                            literal(&file.path, value, "shared initializer")?;
                        }
                        shared.push(Item::Let(ModLetItem {
                            span: f.span,
                            doc: f.doc.clone(),
                            name: f.name.clone(),
                            ty: f.ty.clone(),
                            value: f.default.clone(),
                            thread: false,
                            uniform: false,
                        }));
                    }
                }
                Item::State(s) => {
                    take_name(&mut seen, &file.path, &s.name, &name)?;
                    literal(&file.path, &s.value, "state initializer")?;
                    let ty = match &s.ty {
                        Some(ty) => ty.clone(),
                        None => inferred_type(&s.value).ok_or_else(|| {
                            CheckError::new(
                                &file.path,
                                s.value.span,
                                "cannot infer script state type",
                            )
                        })?,
                    };
                    state.push(FieldItem {
                        span: s.span,
                        doc: String::new(),
                        name: s.name.clone(),
                        ty,
                        embedded: false,
                        flat: false,
                        default: Some(s.value.clone()),
                    });
                }
                Item::Fn(f) => {
                    take_name(&mut seen, &file.path, &f.name, &name)?;
                    if f.params.iter().any(|p| p.name.name == "this") {
                        return Err(CheckError::new(
                            &file.path,
                            f.name.span,
                            "`this` is reserved for the script instance",
                        ));
                    }
                    let mut method = f.clone();
                    let this_ty = Type {
                        span: f.name.span,
                        kind: TypeKind::Ref(Box::new(Type {
                            span: f.name.span,
                            kind: TypeKind::Named(name.clone()),
                        })),
                    };
                    method.params.insert(
                        0,
                        Param {
                            name: Ident {
                                name: "this".to_string(),
                                span: f.name.span,
                            },
                            ty: this_ty,
                        },
                    );
                    methods.push(method);
                }
                Item::Const(c) => {
                    take_name(&mut seen, &file.path, &c.name, &name)?;
                }
                Item::Struct(s) => {
                    take_name(&mut seen, &file.path, &s.name, &name)?;
                }
                Item::Enum(e) => {
                    take_name(&mut seen, &file.path, &e.name, &name)?;
                }
                // `extend` is rejected by the script parser
                Item::Use(_) | Item::UseLib(_) | Item::Let(_) | Item::Trait(_) | Item::Extend(_) => {}
            }
        }
        let mut fields = props;
        fields.extend(state);
        let span = file.ast.span;
        let synthetic = StructItem {
            span,
            doc: String::new(),
            vis_pub: true,
            debug: false,
            packed: false,
            gpu: parse::GpuStruct::None,
            name: Ident { name, span },
            type_params: Vec::new(),
            bounds: Vec::new(),
            fields,
            methods,
        };
        file.ast.items.retain(|item| !matches!(item, Item::Fn(_)));
        file.ast.items.push(Item::Struct(synthetic));
        file.ast.items.extend(shared);
    }
    Ok(())
}

fn unique(seen: &mut HashSet<String>, path: &str, name: &Ident) -> Result<(), CheckError> {
    if !seen.insert(name.name.clone()) {
        return Err(CheckError::new(
            path,
            name.span,
            format!("`{}` already in script scope", name.name),
        ));
    }
    Ok(())
}

/// Props, functions, consts, and struct/enum names share one namespace. The script type
/// (`Door` for `door.j3s`) is that struct, so another type cannot take the same name.
fn take_name(
    seen: &mut HashSet<String>,
    path: &str,
    name: &Ident,
    script: &str,
) -> Result<(), CheckError> {
    if name.name == script {
        return Err(CheckError::new(
            path,
            name.span,
            format!("`{script}` is the script type; pick another name"),
        ));
    }
    unique(seen, path, name)
}

fn literal(path: &str, expr: &parse::Expr, what: &str) -> Result<(), CheckError> {
    let expr = &expr.folded_literal();
    if matches!(
        expr.kind,
        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_) | ExprKind::String(_)
    ) {
        Ok(())
    } else {
        Err(
            CheckError::new(path, expr.span, format!("{what} must be a literal"))
                .help("use a number, bool, or string literal"),
        )
    }
}

fn inferred_type(expr: &parse::Expr) -> Option<Type> {
    let name = match expr.folded_literal().kind {
        ExprKind::Int(_) => "int",
        ExprKind::Float(_) => "f32",
        ExprKind::Bool(_) => "bool",
        ExprKind::String(_) => "string",
        _ => return None,
    };
    Some(Type {
        span: expr.span,
        kind: TypeKind::Named(name.to_string()),
    })
}

pub fn script_name(path: &str) -> String {
    let stem = Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Script");
    let mut out = String::new();
    for word in stem.split('_') {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.extend(chars);
        }
    }
    out
}

pub fn resolve_aliases(mods: &mut [ModData]) -> Result<(), CheckError> {
    let aliases: Vec<(usize, String, String, crate::span::Span)> = mods
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            m.ast.items.iter().filter_map(move |item| {
                let Item::Const(c) = item else { return None };
                let ExprKind::ScriptType(path) = &c.value.kind else {
                    return None;
                };
                Some((i, c.name.name.clone(), path.clone(), c.name.span))
            })
        })
        .collect();
    for (i, alias, rel, span) in aliases {
        let key = format!("script.{}", rel.trim_end_matches(".j3s").replace('/', "."));
        let Some(src) = mods.iter().position(|m| m.key == key && m.ast.is_script) else {
            return Err(CheckError::new(
                &mods[i].path,
                span,
                format!("no script at {rel}"),
            ));
        };
        let type_name = script_name(&rel);
        let Some(&sid) = mods[src].structs.get(&type_name) else {
            return Err(CheckError::new(
                &mods[i].path,
                span,
                format!("script {rel} has no type"),
            ));
        };
        if mods[i].types.contains_key(&alias) {
            return Err(CheckError::new(
                &mods[i].path,
                span,
                format!("`{alias}` already in scope"),
            ));
        }
        let ty = mods[src].types[&type_name];
        mods[i].types.insert(alias.clone(), ty);
        mods[i].structs.insert(alias, sid);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check;
    use crate::span::FileId;

    fn check(script: &str, main: &str) -> Result<check::Program, CheckError> {
        let script_module = [
            "script".to_string(),
            "scripts".to_string(),
            "door".to_string(),
        ];
        let script_ast = parse::parse_script(script, FileId(1), &script_module).unwrap();
        let main_ast = parse::parse(main, FileId(2)).unwrap();
        check::check_files(vec![
            ParsedFile {
                path: "scripts/door.j3s".to_string(),
                src: script.to_string(),
                ast: script_ast,
                file_id: FileId(1),
            },
            ParsedFile {
                path: "main.j3".to_string(),
                src: main.to_string(),
                ast: main_ast,
                file_id: FileId(2),
            },
        ])
    }

    const SCRIPT: &str = "props { hinge: int; gain: f32 = 1.0; }\nshared { hits: int = 0; }\nlet open = false;\npub fn toggle() { open = !open; hits += 1; }\nfn smooth(): int { return 1; }\n";
    const MAIN: &str = "module demo; const Door = @script(\"scripts/door.j3s\"); fn main() { let door = Door { hinge: 1 }; door.toggle(); }";

    #[test]
    fn script_lowers_to_struct_methods_and_globals() {
        let program = check(SCRIPT, MAIN).expect("script checks");
        let s = program
            .cx
            .structs()
            .iter()
            .find(|s| s.script.is_some())
            .unwrap();
        assert_eq!(s.fields.len(), 3);
        assert_eq!(s.script.as_ref().unwrap().prop_spans.len(), 2);
        assert!(program
            .cx
            .globals()
            .iter()
            .any(|g| program.cx.name_str(g.name) == "hits"));
    }

    #[test]
    fn missing_prop_has_note_and_help() {
        let main = "module demo; const Door = @script(\"scripts/door.j3s\"); fn main() { let door = Door {}; }";
        let err = check(SCRIPT, main).err().expect("required prop error");
        assert!(
            err.message.contains("missing required prop `hinge`"),
            "{err:?}"
        );
        assert!(err.notes.iter().any(|n| n.contains("prop declared at")));
        assert!(err.helps.iter().any(|h| h.contains("hinge: ...")));
    }

    #[test]
    fn state_is_not_constructor_prop() {
        let main = "module demo; const Door = @script(\"scripts/door.j3s\"); fn main() { let door = Door { hinge: 1, open: true }; }";
        let err = check(SCRIPT, main).err().expect("state prop error");
        assert!(err.message.contains("script state, not a prop"), "{err:?}");
        assert!(err.helps.iter().any(|h| h.contains("props")));
    }

    #[test]
    fn props_are_read_only() {
        let script = "props { hinge: int; } pub fn change() { hinge = 2; }";
        let err = check(script, MAIN).err().expect("readonly prop error");
        assert!(err.message.contains("read-only"), "{err:?}");
        assert!(err.helps.iter().any(|h| h.contains("copy it into a `let`")));
    }

    #[test]
    fn private_method_is_not_callable_outside() {
        let main = "module demo; const Door = @script(\"scripts/door.j3s\"); fn main() { let door = Door { hinge: 1 }; door.smooth(); }";
        let err = check(SCRIPT, main).err().expect("private method error");
        assert!(err.message.contains("private to"), "{err:?}");
        assert!(err.helps.iter().any(|h| h.contains("add `pub`")));
    }

    #[test]
    fn fields_are_private_outside() {
        let main = "module demo; const Door = @script(\"scripts/door.j3s\"); fn main() { let door = Door { hinge: 1 }; let x = door.open; }";
        let err = check(SCRIPT, main).err().expect("private field error");
        assert!(err.message.contains("script fields are private"), "{err:?}");
    }

    #[test]
    fn script_values_cannot_be_copied() {
        let main = "module demo; const Door = @script(\"scripts/door.j3s\"); fn main() { let door = Door { hinge: 1 }; let other = door; }";
        let err = check(SCRIPT, main).err().expect("copy error");
        assert!(err.message.contains("can't be copied"), "{err:?}");
        assert!(err.helps.iter().any(|h| h.contains("&Door")));
    }

    #[test]
    fn script_type_name_is_reserved() {
        let err = check("struct Door { n: int; }\n", "module demo; fn main() {}")
            .err()
            .expect("script type name");
        assert!(err.message.contains("is the script type"), "{err:?}");
    }

    #[test]
    fn script_struct_shares_the_namespace() {
        let err = check(
            "struct Hinge { n: int; }\nfn Hinge(): int { return 1; }\n",
            "module demo; fn main() {}",
        )
        .err()
        .expect("duplicate name");
        assert!(err.message.contains("already in script scope"), "{err:?}");
    }

    #[test]
    fn script_initializers_must_be_literals() {
        for (script, kind) in [
            ("props { hinge: int = 1 + 2; }", "prop default"),
            ("shared { hits: int = 1 + 2; }", "shared initializer"),
            ("let open = 1 + 2;", "state initializer"),
        ] {
            let err = check(script, "module demo; fn main() {}")
                .err()
                .expect("literal error");
            assert!(err.message.contains(kind), "{err:?}");
            assert!(err.helps.iter().any(|h| h.contains("literal")));
        }
    }
}
