use super::*;

pub(crate) fn resolve_type(
    cx: &mut IrCx,
    ty: &Type,
    path: &str,
    types: &HashMap<String, Ty>,
) -> Result<Ty, CheckError> {
    match &ty.kind {
        TypeKind::Ref(inner) => {
            let inner = resolve_type(cx, inner, path, types)?;
            Ok(cx.ty_ref(inner))
        }
        TypeKind::Named(name) => resolve_named(cx, name, ty.span, path, types),
        TypeKind::Generic { name, .. } => Err(CheckError::new(
            path,
            ty.span,
            format!("generic type `{name}` needs instantiation"),
        )),
        TypeKind::Array { len, elem } => {
            let elem = resolve_type(cx, elem, path, types)?;
            Ok(cx.ty_array(elem, *len))
        }
        TypeKind::Slice(elem) => {
            let elem = resolve_type(cx, elem, path, types)?;
            Ok(cx.ty_slice(elem))
        }
        TypeKind::Fn { params, ret } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(resolve_type(cx, p, path, types)?);
            }
            let r = match ret {
                Some(t) => resolve_type(cx, t, path, types)?,
                None => cx.common.void,
            };
            Ok(cx.ty_fn(ps, r))
        }
    }
}

pub(crate) fn resolve_type_g(
    cx: &mut IrCx,
    ty: &Type,
    path: &str,
    env: &mut GenEnv<'_>,
) -> Result<Ty, CheckError> {
    match &ty.kind {
        TypeKind::Ref(inner) => {
            let inner = resolve_type_g(cx, inner, path, env)?;
            Ok(cx.ty_ref(inner))
        }
        TypeKind::Named(name) => {
            if let Some(&t) = env.subst.get(name) {
                return Ok(t);
            }
            if let Some(&t) = env.types.get(name) {
                return Ok(t);
            }
            if env.enum_temps.contains_key(name) || env.struct_temps.contains_key(name) {
                return Err(CheckError::new(
                    path,
                    ty.span,
                    format!("`{name}` needs type arguments"),
                ));
            }
            resolve_named(cx, name, ty.span, path, env.types)
        }
        TypeKind::Generic { name, args } => {
            let mut resolved = Vec::new();
            for a in args {
                resolved.push(resolve_type_g(cx, a, path, env)?);
            }
            if env.enum_temps.contains_key(name) {
                instantiate_enum(cx, path, ty.span, name, resolved, env)
            } else if env.struct_temps.contains_key(name) {
                instantiate_struct(cx, path, ty.span, name, resolved, env)
            } else {
                Err(CheckError::new(
                    path,
                    ty.span,
                    format!("unknown generic type `{name}`"),
                ))
            }
        }
        TypeKind::Array { len, elem } => {
            let elem = resolve_type_g(cx, elem, path, env)?;
            Ok(cx.ty_array(elem, *len))
        }
        TypeKind::Slice(elem) => {
            let elem = resolve_type_g(cx, elem, path, env)?;
            Ok(cx.ty_slice(elem))
        }
        TypeKind::Fn { params, ret } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(resolve_type_g(cx, p, path, env)?);
            }
            let r = match ret {
                Some(t) => resolve_type_g(cx, t, path, env)?,
                None => cx.common.void,
            };
            Ok(cx.ty_fn(ps, r))
        }
    }
}

pub(crate) fn instantiate_enum(
    cx: &mut IrCx,
    path: &str,
    span: Span,
    name: &str,
    args: Vec<Ty>,
    env: &mut GenEnv<'_>,
) -> Result<Ty, CheckError> {
    let key = (name.to_string(), args.clone());
    if let Some(&id) = env.enum_insts.get(&key) {
        return Ok(cx.ty_enum(id));
    }
    let temp = match env.enum_temps.get(name) {
        Some(t) => t.clone(),
        None => {
            return Err(CheckError::new(
                path,
                span,
                format!("unknown generic type `{name}`"),
            ));
        }
    };
    if temp.type_params.len() != args.len() {
        return Err(CheckError::new(
            path,
            span,
            format!(
                "`{name}` takes {} type args, got {}",
                temp.type_params.len(),
                args.len()
            ),
        ));
    }
    let mut subst = HashMap::new();
    for (p, a) in temp.type_params.iter().zip(args.iter()) {
        subst.insert(p.name.clone(), *a);
    }
    let mut merged = env.subst.clone();
    merged.extend(subst);
    let mut variants = Vec::new();
    for v in &temp.variants {
        let payload = match &v.payload {
            Some(t) => {
                let mut nested = GenEnv {
                    types: env.types,
                    subst: &merged,
                    enum_temps: env.enum_temps,
                    enum_insts: env.enum_insts,
                    struct_temps: env.struct_temps,
                    struct_temp_env: env.struct_temp_env,
                    struct_insts: env.struct_insts,
                    methods: env.methods,
                    method_temps: env.method_temps,
                    method_insts: env.method_insts,
                    fn_substs: env.fn_substs,
                    pending_mono: env.pending_mono,
                    module: env.module,
                    ir_enums: env.ir_enums,
                    ir_structs: env.ir_structs,
                    ir_funcs: env.ir_funcs,
                };
                Some(resolve_type_g(cx, t, path, &mut nested)?)
            }
            None => None,
        };
        variants.push(VariantDef {
            name: cx.intern_name(&v.name.name),
            doc: v.doc.clone(),
            payload,
        });
    }
    let ename = cx.intern_name(&temp.name.name);
    let id = cx.alloc_enum(EnumDef {
        module: env.module.to_vec(),
        name: ename,
        doc: temp.doc.clone(),
        variants: Vec::new(),
        try_catch: None,
        debug: temp.debug,
        span: temp.span,
    });
    let try_catch = enum_try_catch(cx, id, &temp, path, &variants)?;
    let def = cx.enum_def_mut(id);
    def.variants = variants;
    def.try_catch = try_catch;
    for v in &cx.enum_def(id).variants {
        if let Some(ty) = v.payload {
            if occurs_value(cx, ty, OccurRoot::Enum(id), &mut HashSet::new()) {
                return Err(CheckError::new(
                    path,
                    temp.span,
                    format!(
                        "recursive field `{}` must go through `&` or `ptr`",
                        cx.name_str(v.name)
                    ),
                ));
            }
        }
    }
    env.enum_insts.insert(key, id);
    env.ir_enums.push(id);
    Ok(cx.ty_enum(id))
}

pub(crate) fn instantiate_struct(
    cx: &mut IrCx,
    path: &str,
    span: Span,
    name: &str,
    args: Vec<Ty>,
    env: &mut GenEnv<'_>,
) -> Result<Ty, CheckError> {
    let key = (name.to_string(), args.clone());
    if let Some(&id) = env.struct_insts.get(&key) {
        return Ok(cx.ty_struct(id));
    }
    let temp = match env.struct_temps.get(name) {
        Some(t) => t.clone(),
        None => {
            return Err(CheckError::new(
                path,
                span,
                format!("unknown generic type `{name}`"),
            ));
        }
    };
    if temp.type_params.len() != args.len() {
        return Err(CheckError::new(
            path,
            span,
            format!(
                "`{name}` takes {} type args, got {}",
                temp.type_params.len(),
                args.len()
            ),
        ));
    }
    let mut subst = HashMap::new();
    for (p, a) in temp.type_params.iter().zip(args.iter()) {
        subst.insert(p.name.clone(), *a);
    }
    // Modules share an instance of the same declaration and type arguments.
    // The declaration span distinguishes unrelated generics with the same name.
    if let Some(index) = cx.structs().iter().position(|s| {
        s.span == temp.span
            && s.ty_args.len() == args.len()
            && s.ty_args.iter().zip(&args).all(|((_, ty), arg)| ty == arg)
    }) {
        let id = StructId(index as u32);
        env.struct_insts.insert(key, id);
        return Ok(cx.ty_struct(id));
    }
    // The generic's own module wins on a shared name (`Arena` in `arena.j3s`
    // must not replace `j3.memory.Arena` inside `List`).
    let mut merged_types = env
        .struct_temp_env
        .get(name)
        .cloned()
        .unwrap_or_else(|| env.types.clone());
    for (k, v) in env.types.iter() {
        merged_types.entry(k.clone()).or_insert(*v);
    }
    let sname = cx.intern_name(&temp.name.name);
    let id = cx.alloc_struct(StructDef {
        module: env.module.to_vec(),
        name: sname,
        doc: temp.doc.clone(),
        fields: Vec::new(),
        packed: temp.packed,
        debug: temp.debug,
        span: temp.span,
        ty_args: temp
            .type_params
            .iter()
            .zip(args.iter())
            .map(|(p, a)| (p.name.clone(), *a))
            .collect(),
        script: None,
    });
    env.struct_insts.insert(key, id);
    env.ir_structs.push(id);
    let mut fields = Vec::new();
    let mut seen = HashMap::new();
    for f in &temp.fields {
        if seen.contains_key(&f.name.name) {
            return Err(CheckError::new(
                path,
                f.name.span,
                format!("duplicate field `{}`", f.name.name),
            ));
        }
        seen.insert(f.name.name.clone(), ());
        let mut nested = env_reborrow(env, &subst, &merged_types);
        let fty = resolve_type_g(cx, &f.ty, path, &mut nested)?;
        reject_arena_field(cx, path, f.name.span, fty)?;
        let default = field_default(cx, path, fty, &f.ty, &temp.type_params, &f.default)?;
        fields.push(FieldDef {
            name: cx.intern_name(&f.name.name),
            doc: f.doc.clone(),
            ty: fty,
            embedded: f.embedded,
            default,
        });
    }
    cx.struct_def_mut(id).fields = fields;
    for f in &cx.struct_def(id).fields {
        if occurs_value(cx, f.ty, OccurRoot::Struct(id), &mut HashSet::new()) {
            return Err(CheckError::new(
                path,
                temp.span,
                format!(
                    "recursive field `{}` must go through `&` or `ptr`",
                    cx.name_str(f.name)
                ),
            ));
        }
    }
    for m in &temp.methods {
        let mkey = (id.0, m.name.name.clone());
        if !m.type_params.is_empty() {
            env.method_temps.insert(mkey, m.clone());
            continue;
        }
        let module = env.module;
        let mut nested = env_reborrow(env, &subst, &merged_types);
        let fid = alloc_fn(cx, path, module, m, Some(id), &HashMap::new(), &mut nested)?;
        env.methods.insert(mkey, fid);
        env.ir_funcs.push(fid);
        env.fn_substs.insert(fid, subst.clone());
        env.pending_mono.push((fid, m.clone(), subst.clone()));
    }
    Ok(cx.ty_struct(id))
}

pub(super) fn env_reborrow<'a, 'b>(
    env: &'b mut GenEnv<'a>,
    subst: &'b HashMap<String, Ty>,
    types: &'b HashMap<String, Ty>,
) -> GenEnv<'b>
where
    'a: 'b,
{
    GenEnv {
        types,
        subst,
        enum_temps: env.enum_temps,
        enum_insts: env.enum_insts,
        struct_temps: env.struct_temps,
        struct_temp_env: env.struct_temp_env,
        struct_insts: env.struct_insts,
        methods: env.methods,
        method_temps: env.method_temps,
        method_insts: env.method_insts,
        fn_substs: env.fn_substs,
        pending_mono: env.pending_mono,
        module: env.module,
        ir_enums: env.ir_enums,
        ir_structs: env.ir_structs,
        ir_funcs: env.ir_funcs,
    }
}

pub(super) fn resolve_type_ref(
    cx: &IrCx,
    ty: &Type,
    path: &str,
    types: &HashMap<String, Ty>,
) -> Result<Ty, CheckError> {
    match &ty.kind {
        TypeKind::Ref(inner) => {
            let inner = resolve_type_ref(cx, inner, path, types)?;
            match cx.lookup_ty(&TyKind::Ref(inner)) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown `&` type")),
            }
        }
        TypeKind::Named(name) => resolve_named(cx, name, ty.span, path, types),
        TypeKind::Generic { name, .. } => Err(CheckError::new(
            path,
            ty.span,
            format!("generic type `{name}` needs instantiation"),
        )),
        TypeKind::Array { len, elem } => {
            let elem = resolve_type_ref(cx, elem, path, types)?;
            match cx.lookup_ty(&TyKind::Array { elem, len: *len }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown array type")),
            }
        }
        TypeKind::Slice(elem) => {
            let elem = resolve_type_ref(cx, elem, path, types)?;
            match cx.lookup_ty(&TyKind::Slice { elem }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown slice type")),
            }
        }
        TypeKind::Fn { params, ret } => {
            let mut ps = Vec::new();
            for p in params {
                ps.push(resolve_type_ref(cx, p, path, types)?);
            }
            let r = match ret {
                Some(t) => resolve_type_ref(cx, t, path, types)?,
                None => cx.common.void,
            };
            match cx.lookup_ty(&TyKind::Fn { params: ps, ret: r }) {
                Some(t) => Ok(t),
                None => Err(CheckError::new(path, ty.span, "unknown fn type")),
            }
        }
    }
}

pub(super) fn resolve_named(
    cx: &IrCx,
    name: &str,
    span: Span,
    path: &str,
    types: &HashMap<String, Ty>,
) -> Result<Ty, CheckError> {
    if let Some(t) = cx.builtin_named(name) {
        return Ok(t);
    }
    match types.get(name).copied() {
        Some(t) => Ok(t),
        None => Err(CheckError::new(
            path,
            span,
            format!("unknown type `{name}`"),
        )),
    }
}

pub(super) fn field_default(
    cx: &IrCx,
    path: &str,
    ty: Ty,
    field_ty: &Type,
    params: &[crate::parse::Ident],
    default: &Option<Expr>,
) -> Result<Option<Operand>, CheckError> {
    let Some(expr) = default else {
        return Ok(None);
    };
    if mentions_type_param(field_ty, params) {
        return Err(CheckError::new(
            path,
            expr.span,
            "default on a generic field",
        ));
    }
    Ok(Some(literal_default(cx, path, ty, expr)?))
}

pub(super) fn mentions_type_param(ty: &Type, params: &[crate::parse::Ident]) -> bool {
    match &ty.kind {
        TypeKind::Named(n) => params.iter().any(|p| p.name == *n),
        TypeKind::Generic { name, args } => {
            params.iter().any(|p| p.name == *name)
                || args.iter().any(|a| mentions_type_param(a, params))
        }
        TypeKind::Ref(inner) | TypeKind::Slice(inner) => mentions_type_param(inner, params),
        TypeKind::Array { elem, .. } => mentions_type_param(elem, params),
        TypeKind::Fn { params: ps, ret } => {
            ps.iter().any(|p| mentions_type_param(p, params))
                || ret.as_ref().is_some_and(|r| mentions_type_param(r, params))
        }
    }
}

pub(super) fn literal_default(cx: &IrCx, path: &str, ty: Ty, expr: &Expr) -> Result<Operand, CheckError> {
    let mismatch = |span| CheckError::new(path, span, "field default type mismatch");
    let folded = expr.folded_literal();
    let expr = &folded;
    match &expr.kind {
        ExprKind::Bool(v) => {
            if ty != cx.common.bool {
                return Err(mismatch(expr.span));
            }
            Ok(Operand::bool_const(expr.span, ty, *v))
        }
        ExprKind::Int(v) => {
            if *v == 0 && matches!(cx.ty_kind(ty), TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)) {
                return Ok(Operand::null(expr.span, ty));
            }
            if is_int(cx, ty) {
                return Ok(Operand::int(expr.span, ty, *v as u64));
            }
            if ty == cx.common.f32 {
                return Ok(Operand::f32_const(expr.span, ty, *v as f32));
            }
            if is_float(cx, ty) {
                return Ok(Operand::f64_const(expr.span, ty, *v as f64));
            }
            Err(mismatch(expr.span))
        }
        ExprKind::Float(v) => {
            if ty == cx.common.f32 {
                return Ok(Operand::f32_const(expr.span, ty, *v as f32));
            }
            if is_float(cx, ty) {
                return Ok(Operand::f64_const(expr.span, ty, *v));
            }
            Err(mismatch(expr.span))
        }
        ExprKind::String(bytes) if ty == cx.common.string => {
            Ok(Operand::bytes(expr.span, ty, bytes))
        }
        _ => Err(CheckError::new(
            path,
            expr.span,
            "field default must be a literal",
        )),
    }
}
