use super::*;

pub(super) fn collect_funcs(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let names = mods[i].names.clone();
    let types = mods[i].types.clone();
    let items = mods[i].ast.items.clone();

    for it in &items {
        match it {
            Item::Fn(f) if !f.type_params.is_empty() => {
                register_generic_fn(&mut mods[i], &path, f)?;
            }
            Item::Fn(f) => {
                if mods[i].gen_fns.contains_key(&f.name.name) {
                    return Err(CheckError::new(path, f.name.span, format!("duplicate fn `{}`", f.name.name)));
                }
                let existing = mods[i].funcs.clone();
                let id = alloc_fn_mod(cx, &mut mods[i], &path, &names, &types, f, None, &existing)?;
                mods[i].funcs.insert(f.name.name.clone(), id);
                mods[i].ir.funcs.push(id);
            }
            Item::Struct(s) => {
                if !s.type_params.is_empty() {
                    continue;
                }
                let sid = mods[i].structs[&s.name.name];
                for m in &s.methods {
                    let key = (sid.0, m.name.name.clone());
                    if mods[i].methods.contains_key(&key) || mods[i].method_temps.contains_key(&key)
                    {
                        return Err(CheckError::new(
                            path.clone(),
                            m.name.span,
                            format!("duplicate method `{}` on `{}`", m.name.name, s.name.name),
                        ));
                    }
                    if !m.type_params.is_empty() {
                        mods[i].method_temps.insert(key, m.clone());
                        continue;
                    }
                    let id = alloc_fn_mod(
                        cx,
                        &mut mods[i],
                        &path,
                        &names,
                        &types,
                        m,
                        Some(sid),
                        &HashMap::new(),
                    )?;
                    mods[i].methods.insert(key, id);
                    mods[i].ir.funcs.push(id);
                }
            }
            Item::UseLib(u) => {
                if mods[i].lib_ns.contains_key(&u.alias.name) {
                    return Err(CheckError::new(
                        path.clone(),
                        u.alias.span,
                        format!("duplicate `use lib` alias `{}`", u.alias.name),
                    ));
                }
                let lib = cx.intern_name(&u.lib);
                let mut ns = HashMap::new();
                for d in &u.decls {
                    if ns.contains_key(&d.name.name) {
                        return Err(CheckError::new(
                            path.clone(),
                            d.name.span,
                            format!("duplicate lib fn `{}`", d.name.name),
                        ));
                    }
                    let mut params = Vec::new();
                    for p in &d.params {
                        params.push(resolve_type(cx, &p.ty, &path, &types)?);
                    }
                    let ret = match &d.ret {
                        Some(t) => resolve_type(cx, t, &path, &types)?,
                        None => cx.common.void,
                    };
                    let ty = cx.ty_fn(params, ret);
                    let fname = cx.intern_name(&d.name.name);
                    let symbol = match &d.symbol {
                        Some(s) => cx.intern_name(s),
                        None => fname,
                    };
                    let id = cx.alloc_func(Func {
                        doc: String::new(),
                        id: FuncId(0),
                        module: names.clone(),
                        name: fname,
                        ty,
                        vis: Vis::Private,
                        method_of: None,
                        kind: FuncKind::Lib { lib, symbol },
                        span: d.span,
                    });
                    ns.insert(d.name.name.clone(), id);
                    mods[i].ir.funcs.push(id);
                }
                mods[i].lib_ns.insert(u.alias.name.clone(), ns);
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn alloc_fn_mod(
    cx: &mut IrCx,
    m: &mut ModData,
    path: &str,
    names: &[Name],
    types: &HashMap<String, Ty>,
    f: &FnItem,
    method_of: Option<StructId>,
    existing: &HashMap<String, FuncId>,
) -> Result<FuncId, CheckError> {
    let empty = HashMap::new();
    let mut env = GenEnv {
        types,
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
    alloc_fn(cx, path, names, f, method_of, existing, &mut env)
}

pub(crate) fn alloc_fn(
    cx: &mut IrCx,
    path: &str,
    names: &[Name],
    f: &FnItem,
    method_of: Option<StructId>,
    existing: &HashMap<String, FuncId>,
    env: &mut GenEnv<'_>,
) -> Result<FuncId, CheckError> {
    if existing.contains_key(&f.name.name) {
        return Err(CheckError::new(
            path,
            f.name.span,
            format!("duplicate fn `{}`", f.name.name),
        ));
    }
    let mut params = Vec::new();
    for p in &f.params {
        params.push(resolve_type_g(cx, &p.ty, path, env)?);
    }
    let ret = match &f.ret {
        Some(t) => {
            let r = resolve_type_g(cx, t, path, env)?;
            reject_closure(cx, path, t.span, r, "returned")?;
            r
        }
        None => cx.common.void,
    };
    // A `.j3s` script cannot hold a closure (the VM has no fn values).
    if method_of.is_some_and(|s| cx.struct_def(s).script.is_some())
        && f.params.iter().zip(&params).any(|(_, p)| cx.closure_sig(*p).is_some())
    {
        let at = f.params.iter().zip(&params).find(|(_, p)| cx.closure_sig(**p).is_some()).map(|(p, _)| p.ty.span).unwrap();
        return Err(CheckError::new(path, at, "closures are not supported in .j3s scripts")
            .help("scripts run on the VM, which has no function values; move this code into a `.j3` module"));
    }
    let ty = cx.ty_fn(params, ret);
    let vis = if f.vis_pub { Vis::Pub } else { Vis::Private };
    let fname = cx.intern_name(&f.name.name);
    Ok(cx.alloc_func(Func {
        id: FuncId(0),
        doc: f.doc.clone(),
        module: names.to_vec(),
        name: fname,
        ty,
        vis,
        method_of,
        kind: FuncKind::Decl,
        span: f.span,
    }))
}

pub(crate) fn find_method(
    cx: &IrCx,
    ty: Ty,
    name: &str,
    methods: &HashMap<(u32, String), FuncId>,
) -> Option<FuncId> {
    let mut t = ty;
    while let Some(inner) = cx.ref_inner(t) {
        t = inner;
    }
    match cx.ty_kind(t) {
        TyKind::Struct(id) => methods.get(&(id.0, name.to_string())).copied(),
        _ => None,
    }
}

pub(crate) fn find_operator(
    cx: &IrCx,
    ty: Ty,
    op: parse::BinOp,
    methods: &HashMap<(u32, String), FuncId>,
) -> Option<FuncId> {
    let name = match op {
        parse::BinOp::Add => "operator+",
        parse::BinOp::Sub => "operator-",
        parse::BinOp::Mul => "operator*",
        parse::BinOp::Div => "operator/",
        parse::BinOp::Eq => "operator==",
        parse::BinOp::Ne => "operator!=",
        parse::BinOp::Lt => "operator<",
        parse::BinOp::Le => "operator<=",
        parse::BinOp::Gt => "operator>",
        parse::BinOp::Ge => "operator>=",
        _ => return None,
    };
    find_method(cx, ty, name, methods)
}

pub(super) fn find_main(mods: &[ModData]) -> Result<FuncId, CheckError> {
    let mut found = None;
    for m in mods {
        if m.key == "j3.sys" || m.key.starts_with("j3.") {
            continue;
        }
        if let Some(&id) = m.funcs.get("main") {
            if found.is_some() {
                return Err(CheckError::new(
                    m.path.clone(),
                    Span::dummy(),
                    "multiple `fn main` in the program",
                ));
            }
            found = Some((id, m.path.clone()));
        }
    }
    match found {
        Some((id, _)) => Ok(id),
        None => Err(CheckError::new(
            "<program>",
            Span::dummy(),
            "program needs `fn main()`",
        )),
    }
}

pub(super) fn dummy_main(cx: &mut IrCx) -> FuncId {
    let ty = cx.ty_fn(Vec::new(), cx.common.void);
    let name = cx.intern_name("main");
    cx.alloc_func(Func {
        id: FuncId(0),
        doc: String::new(),
        module: Vec::new(),
        name,
        ty,
        vis: Vis::Private,
        method_of: None,
        kind: FuncKind::Decl,
        span: Span::dummy(),
    })
}
