//! Traits, collection side (`speculative/lang/traits.j3`). A trait is a list of signatures; a type
//! satisfies it by having the methods (own methods, or `#[impl]` fns for foreign types).
//! This file finds traits, resolves the names in bounds, and builds the `#[impl]` table.
//! `lower/bounds.rs` is the other half: it checks a type against a trait.

use super::*;
use crate::parse::Ident;

/// One type parameter with bounds and the type it was given.
pub(crate) struct BoundReq {
    pub param: String,
    pub arg: Ty,
    /// Trait keys (`module::Name`), each with the span of the bound where it is written.
    pub bounds: Vec<Ident>,
}

/// A generic struct/enum instance whose parameters have bounds, waiting for the trait tables.
pub(crate) struct BoundSite {
    pub path: String,
    pub span: Span,
    /// "`Map`", for "needed by `K` of `Map`".
    pub needed_by: String,
    pub reqs: Vec<BoundReq>,
}

pub(crate) struct TraitDef {
    pub item: parse::TraitItem,
    pub path: String,
    /// Types in scope where the trait is declared (its signatures resolve there).
    pub types: HashMap<String, Ty>,
}

pub(crate) struct ImplFn {
    pub id: FuncId,
    pub path: String,
    pub span: Span,
}

/// Read by every module, written by none, after `build_env`.
#[derive(Default)]
pub(crate) struct TraitEnv {
    /// By key `module::Name`.
    pub defs: HashMap<String, TraitDef>,
    /// `#[impl]` fns by (method name, first-parameter type without `&`).
    pub impls: HashMap<(String, Ty), ImplFn>,
}

pub(crate) fn ty_text(cx: &IrCx, ty: Ty) -> String {
    let mut s = String::new();
    crate::ir::dump::dump_ty(cx, ty, &mut s);
    s
}

pub(crate) fn trait_key(module: &str, name: &str) -> String {
    format!("{module}::{name}")
}

/// `module::Name` → `Name`.
pub(crate) fn trait_short(key: &str) -> &str {
    key.rsplit("::").next().unwrap_or(key)
}

/// Queues the bound checks of a new instance `name<args>` of a template with `bounds`.
pub(crate) fn queue_bounds(
    cx: &mut IrCx,
    path: &str,
    span: Span,
    name: &str,
    params: &[Ident],
    bounds: &[Vec<Ident>],
    args: &[Ty],
) {
    let reqs: Vec<BoundReq> = params
        .iter()
        .zip(bounds)
        .zip(args)
        .filter(|((_, b), _)| !b.is_empty())
        .map(|((p, b), a)| BoundReq { param: p.name.clone(), arg: *a, bounds: b.clone() })
        .collect();
    if !reqs.is_empty() {
        cx.bound_queue.push(BoundSite { path: path.to_string(), span, needed_by: format!("`{name}`"), reqs });
    }
}

/// Traits of module `i`, and `#[impl]` fns renamed to a unique symbol (`hash<impl0>`), since
/// several of them share one method name.
pub(super) fn collect_traits(mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let key = mods[i].key.clone();
    let mut items = std::mem::take(&mut mods[i].ast.items);
    let types: HashSet<String> = items
        .iter()
        .filter_map(|it| match it {
            Item::Struct(s) => Some(s.name.name.clone()),
            Item::Enum(e) => Some(e.name.name.clone()),
            _ => None,
        })
        .collect();
    let mut n_impl = 0;
    let mut result = Ok(());
    for it in items.iter_mut() {
        match it {
            Item::Trait(t) => {
                if let Err(e) = register_trait(&mut mods[i], &path, &key, &types, t) {
                    result = Err(e);
                    break;
                }
            }
            Item::Fn(f) if f.is_impl => {
                if !f.type_params.is_empty() {
                    result = Err(CheckError::new(path.clone(), f.name.span, "an `#[impl]` fn cannot be generic")
                        .help("write one `#[impl]` fn per type"));
                    break;
                }
                if f.params.is_empty() {
                    result = Err(CheckError::new(path.clone(), f.name.span, "an `#[impl]` fn needs a first parameter: the value it implements the method for")
                        .help("`#[impl] fn hash(x: int): u64 { .. }` gives `int` a method `hash`"));
                    break;
                }
                let orig = f.name.name.clone();
                f.name.name = format!("{orig}<impl{n_impl}>");
                n_impl += 1;
                mods[i].impl_names.insert(f.name.name.clone(), orig);
            }
            _ => {}
        }
    }
    mods[i].ast.items = items;
    result
}

fn register_trait(
    m: &mut ModData,
    path: &str,
    key: &str,
    types: &HashSet<String>,
    t: &parse::TraitItem,
) -> Result<(), CheckError> {
    let name = &t.name.name;
    if m.traits.contains_key(name) {
        return Err(CheckError::new(path, t.name.span, format!("duplicate trait `{name}`")));
    }
    if types.contains(name) {
        return Err(CheckError::new(path, t.name.span, format!("`{name}` is both a trait and a type"))
            .help("traits and types share one namespace; rename one"));
    }
    if t.methods.is_empty() {
        return Err(CheckError::new(path, t.name.span, format!("trait `{name}` has no methods"))
            .help("list at least one signature: `fn hash(this: Self): u64;`"));
    }
    for (k, meth) in t.methods.iter().enumerate() {
        if t.methods[..k].iter().any(|o| o.name.name == meth.name.name) {
            return Err(CheckError::new(path, meth.name.span, format!("duplicate method `{}` in trait `{name}`", meth.name.name)));
        }
        let first_ok = meth.params.first().is_some_and(|p| {
            let inner = match &p.ty.kind {
                TypeKind::Ref(i) => &**i,
                _ => &p.ty,
            };
            matches!(&inner.kind, TypeKind::Named(n) if n == "Self")
        });
        if !first_ok {
            return Err(CheckError::new(path, meth.name.span, format!("`{}` must take `this: Self` or `this: &Self` first", meth.name.name))
                .note("a trait method is called as `value.method(..)`, so its first parameter is the receiver"));
        }
        for ty in meth.params.iter().map(|p| &p.ty).chain(meth.ret.as_ref()) {
            if let Some(at) = find_closure(ty) {
                return Err(CheckError::new(path, at, "closure types cannot appear in a trait method")
                    .note("a closure cannot satisfy a bound"));
            }
        }
    }
    m.traits.insert(name.clone(), trait_key(key, name));
    Ok(())
}

fn find_closure(t: &Type) -> Option<Span> {
    match &t.kind {
        TypeKind::Closure { .. } => Some(t.span),
        TypeKind::Named(_) => None,
        TypeKind::Generic { args, .. } => args.iter().find_map(find_closure),
        TypeKind::Ref(i) | TypeKind::Slice(i) => find_closure(i),
        TypeKind::Array { elem, .. } => find_closure(elem),
        TypeKind::Fn { params, ret } => params.iter().find_map(find_closure).or_else(|| ret.as_deref().and_then(find_closure)),
    }
}

/// `use m.Trait;` imports, then every bound's name rewritten to its trait key.
pub(super) fn bind_traits(mods: &mut [ModData]) -> Result<(), CheckError> {
    let keys: Vec<String> = mods.iter().map(|m| m.key.clone()).collect();
    for i in 0..mods.len() {
        let path = mods[i].path.clone();
        let uses: Vec<UseItem> = mods[i]
            .ast
            .items
            .iter()
            .filter_map(|it| if let Item::Use(u) = it { Some(u.clone()) } else { None })
            .collect();
        for u in uses {
            // Unresolvable uses are reported by the normal use pass.
            let Ok((mod_key, names)) = use_target(&u, &keys, &path) else { continue };
            let Some(src) = mods.iter().position(|m| m.key == mod_key) else { continue };
            for name in names {
                let Some(key) = mods[src].traits.get(&name).cloned() else { continue };
                if !mods[src].exports.contains(&name) {
                    return Err(CheckError::new(path, u.span, format!("`{name}` is private in `{mod_key}`")));
                }
                if mods[i].traits.contains_key(&name) {
                    return Err(CheckError::new(path, u.span, format!("`{name}` already in scope")));
                }
                mods[i].traits.insert(name, key);
            }
        }
    }
    for m in mods.iter_mut() {
        let types: HashSet<String> = m
            .ast
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Struct(s) => Some(s.name.name.clone()),
                Item::Enum(e) => Some(e.name.name.clone()),
                _ => None,
            })
            .collect();
        let mut items = std::mem::take(&mut m.ast.items);
        let mut result = Ok(());
        for it in items.iter_mut() {
            let r = match it {
                Item::Fn(f) => bind_list(&m.path, &m.traits, &types, &mut f.bounds),
                Item::Enum(e) => bind_list(&m.path, &m.traits, &types, &mut e.bounds),
                Item::Struct(s) => bind_list(&m.path, &m.traits, &types, &mut s.bounds).and_then(|_| {
                    s.methods.iter_mut().try_for_each(|f| bind_list(&m.path, &m.traits, &types, &mut f.bounds))
                }),
                _ => Ok(()),
            };
            if let Err(e) = r {
                result = Err(e);
                break;
            }
        }
        m.ast.items = items;
        result?;
    }
    Ok(())
}

fn bind_list(
    path: &str,
    traits: &HashMap<String, String>,
    types: &HashSet<String>,
    bounds: &mut [Vec<Ident>],
) -> Result<(), CheckError> {
    for b in bounds.iter_mut().flatten() {
        match traits.get(&b.name) {
            Some(key) => b.name = key.clone(),
            None if types.contains(&b.name) => {
                return Err(CheckError::new(path, b.span, format!("`{}` is not a trait", b.name))
                    .note("a bound names a trait: `fn f<T: Hash>(..)`"));
            }
            None => {
                return Err(CheckError::new(path, b.span, format!("unknown trait `{}`", b.name))
                    .help(format!("declare `trait {} {{ .. }}` here, or import it: `use j3.cmp.{};`", b.name, b.name)));
            }
        }
    }
    Ok(())
}

/// The trait table and the `#[impl]` table, once every fn has its type. Orphan and duplicate rules
/// are enforced here, in module order, so the same program always reports the same error.
pub(super) fn build_env(cx: &mut IrCx, mods: &[ModData]) -> Result<TraitEnv, CheckError> {
    let mut env = TraitEnv::default();
    for m in mods {
        for it in &m.ast.items {
            if let Item::Trait(t) = it {
                let key = trait_key(&m.key, &t.name.name);
                env.defs.insert(key, TraitDef { item: t.clone(), path: m.path.clone(), types: m.types.clone() });
            }
        }
    }
    for m in mods {
        for it in &m.ast.items {
            let Item::Fn(f) = it else { continue };
            if !f.is_impl {
                continue;
            }
            let orig = m.impl_names[&f.name.name].clone();
            let id = m.funcs[&f.name.name];
            let mut ty = match cx.fn_sig(cx.func(id).ty) {
                Some((ps, _)) => ps[0],
                None => continue,
            };
            while let Some(inner) = cx.ref_inner(ty) {
                ty = inner;
            }
            let ty_name = ty_text(cx, ty);
            let declares = m.ast.items.iter().any(|o| matches!(o, Item::Trait(t) if t.methods.iter().any(|x| x.name.name == orig)));
            let owns = match cx.ty_kind(ty) {
                TyKind::Struct(sid) => cx.struct_def(*sid).module == m.names,
                TyKind::Enum(eid) => cx.enum_def(*eid).module == m.names,
                _ => false,
            };
            if !declares && !owns {
                return Err(CheckError::new(
                    m.path.clone(),
                    f.name.span,
                    format!("orphan rule: `#[impl] fn {orig}` for `{ty_name}` is in neither the module that declares a trait with `{orig}` nor the module that defines `{ty_name}`"),
                )
                .help(format!("move it into the module of the trait, or the module of `{ty_name}`")));
            }
            if let Some(first) = env.impls.get(&(orig.clone(), ty)) {
                let at = match cx.file(first.span.file) {
                    Some(file) => {
                        let (p, l) = file.loc(first.span.start);
                        format!("{p}:{l}")
                    }
                    None => first.path.clone(),
                };
                return Err(CheckError::new(m.path.clone(), f.name.span, format!("`{orig}` is already implemented for `{ty_name}`"))
                    .note(format!("first `#[impl] fn {orig}` is at {at}"))
                    .help("keep one; an own method on the type also wins over `#[impl]` fns"));
            }
            env.impls.insert((orig, ty), ImplFn { id, path: m.path.clone(), span: f.name.span });
        }
    }
    Ok(env)
}
