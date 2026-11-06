use super::*;

pub(super) fn collect_struct_names(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let items: Vec<StructItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Struct(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    for s in items {
        if mods[i].structs.contains_key(&s.name.name)
            || mods[i].types.contains_key(&s.name.name)
            || mods[i].struct_temps.contains_key(&s.name.name)
            || mods[i].enum_temps.contains_key(&s.name.name)
        {
            return Err(CheckError::new(
                path.clone(),
                s.name.span,
                format!("duplicate type `{}`", s.name.name),
            ));
        }
        if !s.type_params.is_empty() {
            mods[i].struct_temps.insert(s.name.name.clone(), s);
            continue;
        }
        let sname = cx.intern_name(&s.name.name);
        let id = cx.alloc_struct(StructDef {
            module: names.clone(),
            name: sname,
            doc: s.doc.clone(),
            fields: Vec::new(),
            packed: s.packed,
            debug: s.debug,
            span: s.span,
            ty_args: Vec::new(),
            // Only the synthesized script struct is a script value. Other structs in the
            // file are ordinary types the script uses.
            script: if mods[i].ast.is_script && s.name.name == script::script_name(&path) {
                Some(ScriptMeta {
                    path: path.clone(),
                    prop_spans: mods[i]
                        .ast
                        .items
                        .iter()
                        .filter_map(|it| match it {
                            Item::Props(b) => {
                                Some(b.fields.iter().map(|f| f.span).collect::<Vec<_>>())
                            }
                            _ => None,
                        })
                        .flatten()
                        .collect(),
                })
            } else {
                None
            },
        });
        let ty = cx.ty_struct(id);
        mods[i].structs.insert(s.name.name.clone(), id);
        mods[i].types.insert(s.name.name.clone(), ty);
        mods[i].ir.structs.push(id);
    }
    Ok(())
}

pub(super) fn fill_struct_fields(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let items: Vec<StructItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Struct(s) => Some(s.clone()),
            _ => None,
        })
        .collect();
    for s in items {
        if !s.type_params.is_empty() {
            for f in &s.fields {
                if let Some(expr) = &f.default {
                    if mentions_type_param(&f.ty, &s.type_params) {
                        return Err(CheckError::new(
                            path.clone(),
                            expr.span,
                            "default on a generic field",
                        ));
                    }
                    if !matches!(
                        expr.kind,
                        ExprKind::Int(_) | ExprKind::Float(_) | ExprKind::Bool(_)
                    ) {
                        return Err(CheckError::new(
                            path.clone(),
                            expr.span,
                            "field default must be a literal",
                        ));
                    }
                }
            }
            continue;
        }
        let id = mods[i].structs[&s.name.name];
        let mut fields = Vec::new();
        let mut seen = HashMap::new();
        for f in &s.fields {
            if seen.contains_key(&f.name.name) {
                return Err(CheckError::new(
                    path.clone(),
                    f.name.span,
                    format!("duplicate field `{}`", f.name.name),
                ));
            }
            seen.insert(f.name.name.clone(), ());
            let ty = resolve_field_ty(cx, &f.ty, &path, mods, i, &names)?;
            reject_arena_field(cx, &path, f.name.span, ty)?;
            reject_closure(cx, &path, f.ty.span, ty, "a struct field")?;
            let fname = cx.intern_name(&f.name.name);
            let default = field_default(cx, &path, ty, &f.ty, &s.type_params, &f.default)?;
            fields.push(FieldDef {
                name: fname,
                doc: f.doc.clone(),
                ty,
                embedded: f.embedded,
                default,
            });
        }
        cx.struct_def_mut(id).fields = fields;
    }
    Ok(())
}

pub(super) fn is_arena_header(cx: &IrCx, ty: Ty) -> bool {
    let id = match cx.ty_kind(ty) {
        TyKind::Struct(id) => *id,
        _ => return false,
    };
    let def = cx.struct_def(id);
    if cx.name_str(def.name) != "Arena" {
        return false;
    }
    def.module.len() == 2
        && cx.name_str(def.module[0]) == "j3"
        && cx.name_str(def.module[1]) == "memory"
}

pub(super) fn reject_arena_field(cx: &IrCx, path: &str, span: Span, ty: Ty) -> Result<(), CheckError> {
    if !is_arena_header(cx, ty) {
        return Ok(());
    }
    Err(CheckError::new(
        path,
        span,
        "do not store Arena by value; keep one `let` header and pass `&Arena`",
    )
    .help("after Arena.create, only `&Arena`"))
}

pub(super) fn resolve_field_ty(
    cx: &mut IrCx,
    ty: &Type,
    path: &str,
    mods: &mut [ModData],
    i: usize,
    names: &[Name],
) -> Result<Ty, CheckError> {
    let empty = HashMap::new();
    let m = &mut mods[i];
    let mut env = GenEnv {
        types: &m.types,
        subst: &empty,
        enum_temps: &m.enum_temps,
        enum_insts: &mut m.insts,
        struct_temps: &m.struct_temps,
        struct_temp_env: &m.struct_temp_env,
        struct_insts: &mut m.struct_insts,
        methods: &mut m.methods,
        method_temps: &mut m.method_temps,
        method_insts: &mut m.method_insts,
        fn_substs: &mut m.fn_substs,
        pending_mono: &mut m.pending_mono,
        module: names,
        ir_enums: &mut m.ir.enums,
        ir_structs: &mut m.ir.structs,
        ir_funcs: &mut m.ir.funcs,
    };
    resolve_type_g(cx, ty, path, &mut env)
}

pub(super) fn check_embed_clashes(cx: &IrCx, mods: &[ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    for (name, sid) in &mods[i].structs {
        let _ = name;
        check_one_embed(cx, *sid, &path)?;
    }
    Ok(())
}

pub(super) fn check_one_embed(cx: &IrCx, sid: StructId, path: &str) -> Result<(), CheckError> {
    let def = cx.struct_def(sid);
    let span = def.span;
    let mut seen: HashMap<String, String> = HashMap::new();
    for f in &def.fields {
        seen.insert(
            cx.name_str(f.name).to_string(),
            cx.name_str(f.name).to_string(),
        );
    }
    for f in &def.fields {
        if !f.embedded {
            continue;
        }
        let inner = match cx.ty_kind(f.ty) {
            TyKind::Struct(id) => *id,
            _ => {
                return Err(CheckError::new(
                    path,
                    span,
                    format!("`use {}` must be a struct", cx.name_str(f.name)),
                ));
            }
        };
        for promoted in embed_names(cx, inner) {
            if let Some(prev) = seen.get(&promoted) {
                return Err(CheckError::new(
                    path,
                    span,
                    format!("promoted field `{promoted}` clashes with `{prev}`"),
                ));
            }
            seen.insert(promoted, cx.name_str(f.name).to_string());
        }
    }
    Ok(())
}

pub(super) fn embed_names(cx: &IrCx, sid: StructId) -> Vec<String> {
    let mut out = Vec::new();
    for f in &cx.struct_def(sid).fields {
        out.push(cx.name_str(f.name).to_string());
        if f.embedded {
            if let TyKind::Struct(id) = cx.ty_kind(f.ty) {
                out.extend(embed_names(cx, *id));
            }
        }
    }
    out
}

pub(super) fn check_all_occurs(cx: &IrCx, mods: &[ModData]) -> Result<(), CheckError> {
    for m in mods {
        for (name, sid) in &m.structs {
            let _ = name;
            let def = cx.struct_def(*sid);
            let span = def.span;
            for f in &def.fields {
                if occurs_value(cx, f.ty, OccurRoot::Struct(*sid), &mut HashSet::new()) {
                    return Err(CheckError::new(
                        m.path.clone(),
                        span,
                        format!(
                            "recursive field `{}` must go through `&` or `ptr`",
                            cx.name_str(f.name)
                        ),
                    ));
                }
            }
        }
        for eid in &m.ir.enums {
            let def = cx.enum_def(*eid);
            for v in &def.variants {
                if let Some(ty) = v.payload {
                    if occurs_value(cx, ty, OccurRoot::Enum(*eid), &mut HashSet::new()) {
                        return Err(CheckError::new(
                            m.path.clone(),
                            def.span,
                            format!(
                                "recursive field `{}` must go through `&` or `ptr`",
                                cx.name_str(v.name)
                            ),
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum OccurRoot {
    Struct(StructId),
    Enum(EnumId),
}

pub(super) fn occurs_value(cx: &IrCx, ty: Ty, root: OccurRoot, visiting: &mut HashSet<u32>) -> bool {
    match cx.ty_kind(ty) {
        TyKind::Struct(id) => {
            if matches!(root, OccurRoot::Struct(s) if s == *id) {
                return true;
            }
            if !visiting.insert(id.0) {
                return false;
            }
            for field in &cx.struct_def(*id).fields {
                if occurs_value(cx, field.ty, root, visiting) {
                    return true;
                }
            }
            visiting.remove(&id.0);
            false
        }
        TyKind::Array { elem, .. } => occurs_value(cx, *elem, root, visiting),
        TyKind::Enum(id) => {
            if matches!(root, OccurRoot::Enum(e) if e == *id) {
                return true;
            }
            if !visiting.insert(1_000_000 + id.0) {
                return false;
            }
            for v in &cx.enum_def(*id).variants {
                if let Some(p) = v.payload {
                    if occurs_value(cx, p, root, visiting) {
                        return true;
                    }
                }
            }
            visiting.remove(&(1_000_000 + id.0));
            false
        }
        _ => false,
    }
}

pub(super) fn collect_enums(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let items: Vec<EnumItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Enum(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    for e in items {
        if mods[i].types.contains_key(&e.name.name) || mods[i].enum_temps.contains_key(&e.name.name)
        {
            return Err(CheckError::new(
                path.clone(),
                e.name.span,
                format!("duplicate type `{}`", e.name.name),
            ));
        }
        if !e.type_params.is_empty() {
            mods[i].enum_temps.insert(e.name.name.clone(), e);
            continue;
        }
        let ename = cx.intern_name(&e.name.name);
        let id = cx.alloc_enum(EnumDef {
            module: names.clone(),
            name: ename,
            doc: e.doc.clone(),
            variants: Vec::new(),
            try_catch: None,
            debug: e.debug,
            span: e.span,
            ty_args: Vec::new(),
        });
        let ty = cx.ty_enum(id);
        mods[i].types.insert(e.name.name.clone(), ty);
        mods[i].ir.enums.push(id);
        let types = mods[i].types.clone();
        let mut variants = Vec::new();
        for v in &e.variants {
            let payload = match &v.payload {
                Some(t) => Some(resolve_type(cx, t, &path, &types)?),
                None => None,
            };
            variants.push(VariantDef {
                name: cx.intern_name(&v.name.name),
                doc: v.doc.clone(),
                payload,
            });
        }
        let try_catch = enum_try_catch(cx, id, &e, &path, &variants)?;
        let def = cx.enum_def_mut(id);
        def.variants = variants;
        def.try_catch = try_catch;
    }
    Ok(())
}

pub(super) fn enum_try_catch(
    cx: &IrCx,
    id: EnumId,
    e: &EnumItem,
    path: &str,
    variants: &[VariantDef],
) -> Result<Option<TryCatch>, CheckError> {
    let Some((s, f)) = &e.try_catch else {
        return Ok(None);
    };
    let success = find_variant_index(cx, variants, &s.name).ok_or_else(|| {
        CheckError::new(
            path,
            s.span,
            format!("`#[{}]` unknown variant `{}`", "TryCatch", s.name),
        )
    })?;
    let failure = find_variant_index(cx, variants, &f.name).ok_or_else(|| {
        CheckError::new(
            path,
            f.span,
            format!("`#[{}]` unknown variant `{}`", "TryCatch", f.name),
        )
    })?;
    Ok(Some(TryCatch {
        success: VariantId {
            enum_id: id,
            index: success,
        },
        failure: VariantId {
            enum_id: id,
            index: failure,
        },
    }))
}

pub(super) fn find_variant_index(cx: &IrCx, variants: &[VariantDef], name: &str) -> Option<u32> {
    for (i, v) in variants.iter().enumerate() {
        if cx.name_str(v.name) == name {
            return Some(i as u32);
        }
    }
    None
}
