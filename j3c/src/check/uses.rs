use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum UsePhase {
    Types,
    Vals,
}

pub(super) fn resolve_all_uses(mods: &mut [ModData], phase: UsePhase) -> Result<(), CheckError> {
    let keys: Vec<String> = mods.iter().map(|m| m.key.clone()).collect();
    let mut pending: Vec<(usize, UseItem)> = Vec::new();
    for (i, m) in mods.iter().enumerate() {
        for it in &m.ast.items {
            if let Item::Use(u) = it {
                pending.push((i, u.clone()));
            }
        }
    }

    for (i, u) in pending {
        let path = mods[i].path.clone();
        let (mod_key, names) = use_target(&u, &keys, &path)?;
        let src_i = match mods.iter().position(|m| m.key == mod_key) {
            Some(j) => j,
            None => {
                return Err(CheckError::new(
                    path,
                    u.span,
                    format!("module `{mod_key}` was not loaded"),
                ));
            }
        };
        if names.is_empty() {
            let alias = match u.path.last() {
                Some(p) => p.name.clone(),
                None => {
                    return Err(CheckError::new(path, u.span, "empty use path"));
                }
            };
            match phase {
                UsePhase::Types => {
                    if mods[i].mod_types.contains_key(&alias) {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{alias}` already in scope"),
                        ));
                    }
                    let mut exported = HashMap::new();
                    for (k, v) in &mods[src_i].types {
                        if mods[src_i].exports.contains(k) {
                            exported.insert(k.clone(), *v);
                        }
                    }
                    mods[i].mod_types.insert(alias, exported);
                }
                UsePhase::Vals => {
                    if mods[i].mod_ns.contains_key(&alias) || mods[i].lib_ns.contains_key(&alias) {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{alias}` already in scope"),
                        ));
                    }
                    let mut exported = HashMap::new();
                    for (k, v) in &mods[src_i].funcs {
                        if mods[src_i].exports.contains(k) {
                            exported.insert(k.clone(), *v);
                        }
                    }
                    let consts: Vec<(String, ConstVal)> = mods[src_i]
                        .consts
                        .iter()
                        .filter(|(k, _)| mods[src_i].exports.contains(*k))
                        .map(|(k, v)| (qual_key(&alias, k), v.clone()))
                        .collect();
                    for (k, v) in consts {
                        mods[i].consts.insert(k, v);
                    }
                    mods[i].gen_mods.insert(alias.clone(), mod_key.clone());
                    mods[i].mod_ns.insert(alias.clone(), exported.clone());
                    mods[i].lib_ns.insert(alias, exported);
                }
            }
            continue;
        }
        for name in names {
            match phase {
                UsePhase::Types => {
                    if mods[i].types.contains_key(&name)
                        || mods[i].struct_temps.contains_key(&name)
                        || mods[i].enum_temps.contains_key(&name)
                    {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{name}` already in scope"),
                        ));
                    }
                    let exists = mods[src_i].types.contains_key(&name)
                        || mods[src_i].enum_temps.contains_key(&name)
                        || mods[src_i].struct_temps.contains_key(&name);
                    if exists && !mods[src_i].exports.contains(&name) {
                        return Err(CheckError::new(
                            path,
                            u.span,
                            format!("`{name}` is private in `{mod_key}`"),
                        ));
                    }
                    if let Some(&ty) = mods[src_i].types.get(&name) {
                        mods[i].types.insert(name.clone(), ty);
                        if let Some(&sid) = mods[src_i].structs.get(&name) {
                            mods[i].structs.insert(name.clone(), sid);
                        }
                        continue;
                    }
                    if let Some(t) = mods[src_i].enum_temps.get(&name).cloned() {
                        mods[i].enum_temps.insert(name, t);
                        continue;
                    }
                    if let Some(t) = mods[src_i].struct_temps.get(&name).cloned() {
                        mods[i].struct_temps.insert(name, t);
                        continue;
                    }
                }
                UsePhase::Vals => {
                    if mods[src_i].traits.contains_key(&name) {
                        continue; // imported by `bind_traits`
                    }
                    if import_generic_fn(mods, i, src_i, &name, &mod_key, u.span)? {
                        continue;
                    }
                    if let Some(&id) = mods[src_i].funcs.get(&name) {
                        if !mods[src_i].exports.contains(&name) {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` is private in `{mod_key}`"),
                            ));
                        }
                        if mods[i].imports.contains_key(&name) || mods[i].funcs.contains_key(&name)
                        {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` already in scope"),
                            ));
                        }
                        mods[i].imports.insert(name, id);
                        continue;
                    }
                    if mods[i].types.get(&name) == mods[src_i].types.get(&name)
                        && mods[src_i].types.contains_key(&name)
                    {
                        if let Some(&sid) = mods[src_i].structs.get(&name) {
                            let methods: Vec<((u32, String), FuncId)> = mods[src_i]
                                .methods
                                .iter()
                                .filter(|((s, _), _)| *s == sid.0)
                                .map(|(k, v)| (k.clone(), *v))
                                .collect();
                            for (k, v) in methods {
                                mods[i].methods.insert(k, v);
                            }
                            let temps: Vec<((u32, String), parse::FnItem)> = mods[src_i]
                                .method_temps
                                .iter()
                                .filter(|((s, _), _)| *s == sid.0)
                                .map(|(k, v)| (k.clone(), v.clone()))
                                .collect();
                            for (k, v) in temps {
                                mods[i].method_temps.insert(k, v);
                            }
                        }
                        continue;
                    }
                    if mods[i].struct_temps.contains_key(&name)
                        && mods[src_i].struct_temps.contains_key(&name)
                    {
                        continue;
                    }
                    if mods[i].enum_temps.contains_key(&name)
                        && mods[src_i].enum_temps.contains_key(&name)
                    {
                        continue;
                    }
                    if let Some(c) = mods[src_i].consts.get(&name).cloned() {
                        if !mods[src_i].exports.contains(&name) {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` is private in `{mod_key}`"),
                            ));
                        }
                        if mods[i].consts.contains_key(&name) {
                            return Err(CheckError::new(
                                path,
                                u.span,
                                format!("`{name}` already in scope"),
                            ));
                        }
                        mods[i].consts.insert(name, c);
                        continue;
                    }
                    let mut e = CheckError::new(path, u.span, format!("`{mod_key}` has no `{name}`"));
                    let cands: Vec<String> = mods[src_i].exports.iter().cloned().collect();
                    if let Some(s) = super::error::suggest(&name, &cands) {
                        e = e.help(format!("did you mean `{s}`?"));
                    }
                    return Err(e);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn use_target(
    u: &UseItem,
    loaded: &[String],
    path: &str,
) -> Result<(String, Vec<String>), CheckError> {
    let parts: Vec<String> = u.path.iter().map(|p| p.name.clone()).collect();
    if let Some(names) = &u.names {
        return Ok((
            parts.join("."),
            names.iter().map(|n| n.name.clone()).collect(),
        ));
    }
    let as_mod = parts.join(".");
    if loaded.iter().any(|k| k == &as_mod) {
        return Ok((as_mod, Vec::new()));
    }
    if parts.len() >= 2 {
        let mod_key = parts[..parts.len() - 1].join(".");
        let name = parts[parts.len() - 1].clone();
        if loaded.iter().any(|k| k == &mod_key) {
            return Ok((mod_key, vec![name]));
        }
    }
    Err(CheckError::new(
        path,
        u.span,
        format!("cannot resolve `use {}`", parts.join(".")),
    ))
}
