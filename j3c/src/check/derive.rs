//! `#[derive(Eq, Hash, Ord, Clone)]`: field-by-field methods written as J3 source and parsed with
//! the ordinary parser (`parse::parse_items_at`), every span on the derive name, so an error in a
//! generated body (a field without `hash`) points at `#[derive(Hash)]`. A struct gets methods; an
//! enum gets `#[impl]` fns (enums have no methods), matching each variant, and a generic enum gets
//! generic fns `Enum.method` instantiated per instance (`TraitEnv.enum_derives`). `Debug` is `#[Debug]`.
//!
//! Semantics, the same as writing them by hand:
//! - `==`: every field `==` (an enum: same variant, then its fields).
//! - `<`: fields in order, the first that differs decides (an enum: variant order, then fields).
//! - `hash`: `h = hash(f0); h = h * P ^ hash(fk) ..; h ^ (h >> 29)`, as a tuple of the fields
//!   (`check/lower/tuple.rs`); an enum seeds `h` with the variant number.
//! - `clone`: a copy (`*this`); J3 values copy by value, so this meets a `Clone` bound.

use super::*;
use crate::parse::{EnumItem, Ident, StructItem};

const MUL: &str = "1099511628211";

pub(super) fn expand(mods: &mut [ModData]) -> Result<(), CheckError> {
    for m in mods.iter_mut() {
        let mut extra = Vec::new();
        for it in m.ast.items.iter_mut() {
            match it {
                Item::Struct(s) if !s.derives.is_empty() => derive_struct(&m.path, s)?,
                Item::Enum(e) if !e.derives.is_empty() => extra.extend(derive_enum(&m.path, e)?),
                _ => {}
            }
        }
        m.ast.items.extend(extra);
    }
    Ok(())
}

fn parse_at(path: &str, src: &str, at: &Ident) -> Result<Vec<Item>, CheckError> {
    crate::parse::parse_items_at(src, at.span)
        .map_err(|e| CheckError::new(path, at.span, format!("internal: derived `{}` does not parse: {}", at.name, e.message)))
}

fn self_ty(name: &str, params: &[Ident]) -> String {
    if params.is_empty() {
        return name.to_string();
    }
    let ps: Vec<&str> = params.iter().map(|p| p.name.as_str()).collect();
    format!("{name}<{}>", ps.join(", "))
}

/// `h = a; h = h * P ^ b ..; return h ^ (h >> 29);` over the hash expressions `parts`.
fn hash_body(seed: Option<String>, parts: &[String]) -> String {
    let mut it = seed.into_iter().chain(parts.iter().cloned());
    let Some(first) = it.next() else { return "return u64(0);".to_string() };
    let mut s = format!("let h: u64 = {first}; ");
    for p in it {
        s.push_str(&format!("h = (h * {MUL}) ^ {p}; "));
    }
    // `h >> 29`, written as a division: the parser joins `> >` into `>>` by adjacent spans, and
    // every span here is the derive's
    s.push_str("return h ^ (h / 536870912);");
    s
}

/// `if a < b { return true; } if b < a { return false; } .. return false;`
fn less_body(pairs: &[(String, String)]) -> String {
    let mut s = String::new();
    for (a, b) in pairs {
        s.push_str(&format!("if {a} < {b} {{ return true; }} if {b} < {a} {{ return false; }} "));
    }
    s
}

fn derive_struct(path: &str, s: &mut StructItem) -> Result<(), CheckError> {
    let t = self_ty(&s.name.name, &s.type_params);
    let fields: Vec<String> = s.fields.iter().map(|f| f.name.name.clone()).collect();
    for d in std::mem::take(&mut s.derives) {
        let (method, src) = match d.name.as_str() {
            "Eq" => {
                let eqs: Vec<String> = fields.iter().map(|f| format!("this.{f} == other.{f}")).collect();
                let body = if eqs.is_empty() { "true".to_string() } else { eqs.join(" && ") };
                ("operator==", format!("fn operator==(this: {t}, other: {t}): bool {{ return {body}; }}"))
            }
            "Hash" => {
                let parts: Vec<String> = fields.iter().map(|f| format!("this.{f}.hash()")).collect();
                ("hash", format!("fn hash(this: &{t}): u64 {{ {} }}", hash_body(None, &parts)))
            }
            "Ord" => {
                let pairs: Vec<(String, String)> = fields.iter().map(|f| (format!("this.{f}"), format!("other.{f}"))).collect();
                ("operator<", format!("fn operator<(this: {t}, other: {t}): bool {{ {}return false; }}", less_body(&pairs)))
            }
            "Clone" => ("clone", format!("fn clone(this: &{t}): {t} {{ return *this; }}")),
            other => unreachable!("the parser only accepts known derives, got `{other}`"),
        };
        if s.methods.iter().any(|m| m.name.name == method) {
            return Err(CheckError::new(path, d.span, format!("`{}` derives `{}` and also defines `{method}`", s.name.name, d.name))
                .help("keep one: remove the derive or the hand-written method"));
        }
        // `Pair<A, B>`: the derived method needs the trait of every parameter a field holds
        // (`where A: Eq, B: Eq`), so its body checks once at the definition and calls check the
        // instance. Written as trait keys, which `bind_traits` keeps: the user may not import them.
        let wheres: Vec<(Ident, Vec<Ident>)> = match d.name.as_str() {
            "Clone" => Vec::new(),
            tr => s
                .type_params
                .iter()
                .filter(|p| s.fields.iter().any(|f| crate::check::lower::mentions(&f.ty, std::slice::from_ref(&p.name))))
                .map(|p| (Ident { name: p.name.clone(), span: d.span }, vec![Ident { name: format!("j3.cmp::{tr}"), span: d.span }]))
                .collect(),
        };
        for item in parse_at(path, &src, &d)? {
            if let Item::Fn(mut f) = item {
                f.where_bounds = wheres.clone();
                s.methods.push(f);
            }
        }
    }
    Ok(())
}

fn derive_enum(path: &str, e: &mut EnumItem) -> Result<Vec<Item>, CheckError> {
    let derives = std::mem::take(&mut e.derives);
    let en = e.name.name.clone();
    // `Opt<T>` gets generic fns `Opt.operator==<T: Eq>` (no source name has a `.`), found per
    // instance through `TraitEnv.enum_derives`; a plain enum gets `#[impl]` fns.
    let generic = !e.type_params.is_empty();
    let t = self_ty(&en, &e.type_params);
    let tps: Vec<&str> = e.type_params.iter().map(|p| p.name.as_str()).collect();
    let head = |method: &str| if generic { format!("fn derived<{}>", tps.join(", ")) } else { format!("#[impl] fn {method}") };
    // `E.V(a0, a1)` with the binding prefix `p`; `E.V` without fields.
    let pat = |v: &crate::parse::VariantItem, p: &str| {
        if v.fields.is_empty() {
            format!("{en}.{}", v.name.name)
        } else {
            let names: Vec<String> = (0..v.fields.len()).map(|k| format!("{p}{k}")).collect();
            format!("{en}.{}({})", v.name.name, names.join(", "))
        }
    };
    let single = e.variants.len() == 1;
    let c_like = e.variants.iter().all(|v| v.fields.is_empty());
    let mut out = Vec::new();
    for d in derives {
        let src = match d.name.as_str() {
            // a C-like enum compares natively
            "Eq" if c_like => continue,
            "Eq" => {
                let mut arms = String::new();
                for v in &e.variants {
                    let eqs: Vec<String> = (0..v.fields.len()).map(|k| format!("a{k} == b{k}")).collect();
                    let body = if eqs.is_empty() { "true".to_string() } else { eqs.join(" && ") };
                    let other = if single { String::new() } else { "_ { return false; } ".to_string() };
                    arms.push_str(&format!("{} {{ match b {{ {} {{ return {body}; }} {other}}} }} ", pat(v, "a"), pat(v, "b")));
                }
                format!("{}(a: {t}, b: {t}): bool {{ match a {{ {arms}}} }}", head("operator=="))
            }
            "Hash" => {
                let mut arms = String::new();
                for (k, v) in e.variants.iter().enumerate() {
                    let parts: Vec<String> = (0..v.fields.len()).map(|f| format!("a{f}.hash()")).collect();
                    arms.push_str(&format!("{} {{ {} }} ", pat(v, "a"), hash_body(Some(format!("u64({}) * {MUL}", k + 1)), &parts)));
                }
                format!("{}(a: {t}): u64 {{ match a {{ {arms}}} }}", head("hash"))
            }
            "Ord" => {
                let rank = |x: &str| {
                    let arms: String = e.variants.iter().enumerate().map(|(k, v)| format!("{} {{ r{x} = {k}; }} ", wild(&en, v))).collect();
                    format!("let r{x} = 0; match {x} {{ {arms}}}")
                };
                let mut arms = String::new();
                for v in &e.variants {
                    let pairs: Vec<(String, String)> = (0..v.fields.len()).map(|k| (format!("a{k}"), format!("b{k}"))).collect();
                    let other = if single { String::new() } else { "_ {} ".to_string() };
                    arms.push_str(&format!("{} {{ match b {{ {} {{ {}}} {other}}} }} ", pat(v, "a"), pat(v, "b"), less_body(&pairs)));
                }
                format!(
                    "{}(a: {t}, b: {t}): bool {{ {} {} if ra != rb {{ return ra < rb; }} match a {{ {arms}}} return false; }}",
                    head("operator<"),
                    rank("a"),
                    rank("b")
                )
            }
            "Clone" => format!("{}(a: {t}): {t} {{ return a; }}", head("clone")),
            other => unreachable!("the parser only accepts known derives, got `{other}`"),
        };
        let mut items = parse_at(path, &src, &d)?;
        if generic {
            let method = match d.name.as_str() {
                "Eq" => "operator==",
                "Ord" => "operator<",
                "Hash" => "hash",
                _ => "clone",
            };
            // every parameter a payload holds needs the trait (`Clone` copies, so none)
            let bounds: Vec<Vec<Ident>> = e
                .type_params
                .iter()
                .map(|p| match d.name.as_str() {
                    "Clone" => Vec::new(),
                    tr if e.variants.iter().flat_map(|v| &v.fields).any(|f| crate::check::lower::mentions(f, std::slice::from_ref(&p.name))) => {
                        vec![Ident { name: format!("j3.cmp::{tr}"), span: d.span }]
                    }
                    _ => Vec::new(),
                })
                .collect();
            for it in items.iter_mut() {
                if let Item::Fn(f) = it {
                    f.name.name = format!("{en}.{method}");
                    f.bounds = bounds.clone();
                }
            }
        }
        out.extend(items);
    }
    Ok(out)
}

/// `E.V(_, _)` or `E.V`: a variant matched without binding.
fn wild(en: &str, v: &crate::parse::VariantItem) -> String {
    if v.fields.is_empty() {
        format!("{en}.{}", v.name.name)
    } else {
        format!("{en}.{}({})", v.name.name, vec!["_"; v.fields.len()].join(", "))
    }
}
