//! Symbols, call signatures and interface accessors against `steam_api_flat.h`.

use super::header::{j3_kind, type_of, Decl, Types, K};
use super::scan::Scan;
use super::Finding;
use std::collections::{BTreeMap, HashMap, HashSet};

/// By-value struct results `inputabi.j3` takes apart itself (one register, a register pair, or a hidden pointer).
const STRUCT_RETURNS: [&str; 3] = [
    "SteamAPI_ISteamInput_GetDigitalActionData",
    "SteamAPI_ISteamInput_GetAnalogActionData",
    "SteamAPI_ISteamInput_GetMotionData",
];

pub fn check(sc: &Scan, decls: &HashMap<String, Decl>, types: &Types, out: &mut Vec<Finding>) {
    let mut missing = HashSet::new();
    for s in &sc.sites {
        let Some(d) = decls.get(&s.sym) else {
            if missing.insert(s.sym.clone()) {
                let hint = similar(&s.sym, decls);
                out.push(Finding::err(format!("{}: {} is not in steam_api_flat.h{hint}", s.loc, s.sym)));
            }
            continue;
        };
        if let Some(c) = &s.cast {
            signature(&s.loc.to_string(), &s.sym, c.params.as_slice(), c.ret.as_deref(), d, types, out);
        }
    }
    for (file, line, text) in sc.odd.iter().map(|(l, t)| (&l.file, l.line, t)) {
        out.push(Finding::warn(format!("{file}:{line}: symbol literal in a form steam-check cannot read, not compared: {text}")));
    }
    accessors(sc, decls, out);
}

/// Header names with the same interface prefix and a related method name, for a "renamed?" hint.
fn similar(sym: &str, decls: &HashMap<String, Decl>) -> String {
    let Some((prefix, method)) = sym.rsplit_once('_') else { return String::new() };
    let m = method.to_lowercase();
    let mut c: Vec<&str> = decls
        .keys()
        .filter(|k| k.rsplit_once('_').is_some_and(|(p, n)| p == prefix && (n.to_lowercase().contains(&m) || m.contains(&n.to_lowercase()))))
        .map(String::as_str)
        .collect();
    c.sort();
    c.truncate(3);
    if c.is_empty() { String::new() } else { format!(" (renamed? header has {})", c.join(", ")) }
}

fn signature(loc: &str, sym: &str, params: &[String], ret: Option<&str>, d: &Decl, types: &Types, out: &mut Vec<Finding>) {
    let shown = d.show(sym);
    let rk = types.kind(&type_of(&d.ret, false));
    let by_value = rk == K::Struct;
    if by_value && !STRUCT_RETURNS.contains(&sym) {
        out.push(Finding::err(format!("{loc}: {sym} returns `{}` by value; inputabi.j3 handles only {}", d.ret, STRUCT_RETURNS.join(", "))));
        return;
    }
    // inputabi.j3 may pass a hidden result pointer first (sret) for a by-value struct result.
    let extra = if by_value && params.len() == d.params.len() + 1 { 1 } else { 0 };
    if params.len() != d.params.len() + extra {
        out.push(Finding::err(format!("{loc}: {sym}: J3 cast has {} params, SDK has {}: {shown}", params.len(), d.params.len())));
        return;
    }
    for (i, (j, c)) in params[extra..].iter().zip(&d.params).enumerate() {
        let ck = types.kind(&type_of(c, true));
        if let Some((is_err, why)) = compare(j_kind_of(j), ck, false) {
            out.push(Finding::new(is_err, format!("{loc}: {sym} param {} `{c}`: J3 {j} vs SDK {ck:?}: {why}", i + 1)));
        }
    }
    if by_value {
        return;
    }
    match ret {
        None => {}
        Some(r) => {
            if let Some((is_err, why)) = compare(j_kind_of(r), rk, true) {
                out.push(Finding::new(is_err, format!("{loc}: {sym} return: J3 {r} vs SDK `{}` ({rk:?}): {why}", d.ret)));
            }
        }
    }
}

fn j_kind_of(t: &str) -> K {
    j3_kind(t.trim())
}

/// `None` when J3's `j` can carry the C `c`; else (is it an error, why).
fn compare(j: K, c: K, is_ret: bool) -> Option<(bool, &'static str)> {
    if j == c || j == K::Unknown || c == K::Unknown {
        return None;
    }
    if c == K::Void {
        return Some((true, "SDK returns nothing"));
    }
    if j == K::Struct || c == K::Struct {
        return Some((true, "struct against a scalar"));
    }
    if j.is_float() != c.is_float() {
        return Some((true, "float against integer"));
    }
    if j == K::Ptr || c == K::Ptr {
        let wide = |k: K| matches!(k, K::I64 | K::U64);
        return if wide(j) || wide(c) { Some((false, "pointer and 8-byte integer have the same ABI; use one name")) } else { Some((true, "pointer against a narrower type")) };
    }
    match j.size().cmp(&c.size()) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater if !is_ret => Some((false, "J3 passes a wider integer than the SDK reads")),
        _ => Some((true, "integer sizes differ")),
    }
}

/// `SteamAPI_SteamUser_v023` into (`SteamAPI_SteamUser`, 23).
fn split_ver(name: &str) -> Option<(&str, u32)> {
    let i = name.rfind("_v")?;
    let n = &name[i + 2..];
    (!n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())).then(|| (&name[..i], n.parse().unwrap_or(0)))
}

fn accessors(sc: &Scan, decls: &HashMap<String, Decl>, out: &mut Vec<Finding>) {
    let mut hdr: BTreeMap<&str, Vec<u32>> = BTreeMap::new();
    for (name, d) in decls {
        if let (Some((fam, v)), true) = (split_ver(name), d.params.is_empty()) {
            if fam.starts_with("SteamAPI_Steam") {
                hdr.entry(fam).or_default().push(v);
            }
        }
    }
    let mut ours: BTreeMap<&str, Vec<(u32, &str)>> = BTreeMap::new();
    for (row, loc) in &sc.accessors {
        match split_ver(row) {
            Some((fam, v)) => ours.entry(fam).or_default().push((v, row)),
            None => out.push(Finding::err(format!("{loc}: accessor row {row} does not end in _vNNN"))),
        }
    }
    for (fam, rows) in &ours {
        let loc = sc.accessors.iter().find(|(r, _)| split_ver(r).is_some_and(|(f, _)| f == *fam)).map(|(_, l)| l.to_string()).unwrap_or_default();
        let Some(h) = hdr.get_mut(fam) else {
            out.push(Finding::err(format!("{loc}: header has no {fam}_vNNN accessor (interface renamed or removed?)")));
            continue;
        };
        h.sort();
        let newest = rows.iter().map(|r| r.0).max().unwrap_or(0);
        let list = |v: &[u32]| v.iter().map(|n| format!("v{n:03}")).collect::<Vec<_>>().join(", ");
        if !rows.iter().any(|(v, _)| h.contains(v)) {
            out.push(Finding::err(format!("{loc}: none of our {fam} rows ({}) exist in the header, which has {}", list(&rows.iter().map(|r| r.0).collect::<Vec<_>>()), list(h))));
        } else if h.last().is_some_and(|&n| n > newest) {
            let n = h.last().copied().unwrap_or(0);
            out.push(Finding::warn(format!("{loc}: header has {fam}_v{n:03}, our newest row is v{newest:03}; add an addAccessor row above it after checking the methods that interface changed")));
        }
    }
}
