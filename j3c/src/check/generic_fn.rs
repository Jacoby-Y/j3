//! Generic free fns, collection side: `fn first<T>(xs: []T): T` is kept as a template (like
//! `method_temps`) and instantiated from its callers in `lower/generic_fn.rs`.

use super::*;

/// Keeps `f` as a template of module `m`; nothing is allocated until a call instantiates it.
pub(super) fn register_generic_fn(m: &mut ModData, path: &str, f: &FnItem) -> Result<(), CheckError> {
    let name = &f.name.name;
    if m.funcs.contains_key(name) || m.gen_fns.contains_key(name) {
        return Err(CheckError::new(path, f.name.span, format!("duplicate fn `{name}`")));
    }
    for (k, p) in f.type_params.iter().enumerate() {
        if f.type_params[..k].iter().any(|q| q.name == p.name) {
            return Err(CheckError::new(path, p.span, format!("duplicate type parameter `{}`", p.name)));
        }
    }
    m.gen_fns.insert(name.clone(), f.clone());
    m.gen_names.insert(name.clone(), m.key.clone());
    Ok(())
}

/// `use m.name;` where `name` is a generic fn of `m`. Returns false when it is not one.
pub(super) fn import_generic_fn(
    mods: &mut [ModData],
    i: usize,
    src_i: usize,
    name: &str,
    mod_key: &str,
    span: Span,
) -> Result<bool, CheckError> {
    if !mods[src_i].gen_fns.contains_key(name) {
        return Ok(false);
    }
    let path = mods[i].path.clone();
    if !mods[src_i].exports.contains(name) {
        return Err(CheckError::new(path, span, format!("`{name}` is private in `{mod_key}`")));
    }
    if mods[i].imports.contains_key(name) || mods[i].funcs.contains_key(name) || mods[i].gen_names.contains_key(name) {
        return Err(CheckError::new(path, span, format!("`{name}` already in scope")));
    }
    mods[i].gen_names.insert(name.to_string(), mod_key.to_string());
    Ok(true)
}
