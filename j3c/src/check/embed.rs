//! `const X = @embed(..)` / `@embedBytes(..)` / `@embedDir(..)`: read-only globals holding the bytes
//! the loader read (`IrCx.embeds`), so every use shares one copy in static data.

use super::*;
use crate::ir::entity::ConstInit;
use crate::parse::{EmbedFile, EmbedKind};

pub(super) fn collect_embeds(cx: &mut IrCx, mods: &mut [ModData], i: usize) -> Result<(), CheckError> {
    let path = mods[i].path.clone();
    let types = mods[i].types.clone();
    let consts: Vec<ConstItem> = mods[i]
        .ast
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Const(c) if matches!(c.value.kind, ExprKind::Embed { .. }) => Some(c.clone()),
            _ => None,
        })
        .collect();
    for c in consts {
        let ExprKind::Embed { kind, .. } = c.value.kind else { unreachable!() };
        let m = &mods[i];
        if m.globals.contains_key(&c.name.name) || m.funcs.contains_key(&c.name.name) || m.consts.contains_key(&c.name.name) {
            return Err(CheckError::new(path.clone(), c.name.span, format!("`{}` already in scope", c.name.name)));
        }
        let files = embedded(cx, &path, c.value.span)?;
        let span = c.value.span;
        let (ty, init) = match kind {
            EmbedKind::Text | EmbedKind::Bytes => {
                let ty = embed_ty(cx, kind);
                (ty, ConstInit::Scalar(Operand::bytes(span, ty, &files[0].data)))
            }
            EmbedKind::Dir => {
                let sid = file_struct(cx, &c.name.name, &mods[i].names, span);
                mods[i].ir.structs.push(sid);
                let s = cx.common.string;
                let rows = files
                    .iter()
                    .map(|f| {
                        ConstInit::Fields(vec![
                            ("path".to_string(), ConstInit::Scalar(Operand::bytes(span, s, f.name.as_bytes()))),
                            ("data".to_string(), ConstInit::Scalar(Operand::bytes(span, s, &f.data))),
                        ])
                    })
                    .collect::<Vec<_>>();
                let row_ty = cx.ty_struct(sid);
                (cx.ty_array(row_ty, rows.len() as u64), ConstInit::Elems(rows))
            }
        };
        if let Some(t) = &c.ty {
            if resolve_type(cx, t, &path, &types)? != ty {
                return Err(CheckError::new(path.clone(), t.span, format!("`{}` gives a `{}`", embed_call(kind), embed_ty_name(kind)))
                    .help("leave the type off: `const NAME = @embed..(..);`"));
            }
        }
        let gname = cx.intern_name(&c.name.name);
        let id = cx.alloc_global(GlobalDef {
            id: GlobalId(0),
            doc: c.doc.clone(),
            module: mods[i].names.clone(),
            name: gname,
            ty,
            span: c.span,
            init: Some(init),
            thread: false,
            readonly: true,
        });
        mods[i].globals.insert(c.name.name.clone(), id);
        mods[i].ir.globals.push(id);
    }
    Ok(())
}

/// The files the loader read for the `@embed*` at `span`.
pub(super) fn embedded(cx: &IrCx, path: &str, span: Span) -> Result<std::sync::Arc<Vec<EmbedFile>>, CheckError> {
    cx.embeds
        .get(&span)
        .cloned()
        .ok_or_else(|| CheckError::new(path, span, "`@embed` files are read by the project loader; this file was not loaded by it"))
}

/// `string` for `@embed`, `[]u8` for `@embedBytes` (interned at check start).
pub(super) fn embed_ty(cx: &IrCx, kind: EmbedKind) -> Ty {
    match kind {
        EmbedKind::Bytes => cx.lookup_ty(&TyKind::Slice { elem: cx.common.u8 }).expect("[]u8 interned"),
        _ => cx.common.string,
    }
}

pub(super) fn embed_call(kind: EmbedKind) -> &'static str {
    match kind {
        EmbedKind::Text => "@embed",
        EmbedKind::Bytes => "@embedBytes",
        EmbedKind::Dir => "@embedDir",
    }
}

fn embed_ty_name(kind: EmbedKind) -> &'static str {
    match kind {
        EmbedKind::Text => "string",
        EmbedKind::Bytes => "[]u8",
        EmbedKind::Dir => "[N]{ path: string, data: string }",
    }
}

/// `{ path: string, data: string }`, one per `@embedDir` const (named `EmbedFile.NAME`).
fn file_struct(cx: &mut IrCx, cname: &str, module: &[Name], span: Span) -> StructId {
    let s = cx.common.string;
    let fields = ["path", "data"]
        .iter()
        .map(|n| FieldDef { name: cx.intern_name(n), doc: String::new(), ty: s, embedded: false, default: None })
        .collect();
    let name = cx.intern_name(&format!("EmbedFile.{cname}"));
    cx.alloc_struct(StructDef {
        module: module.to_vec(),
        name,
        doc: "One file of an `@embedDir` table.".to_string(),
        fields,
        packed: false,
        debug: false,
        span,
        ty_args: Vec::new(),
        script: None,
    })
}

pub(super) fn embed_dir_err(path: &str, span: Span) -> CheckError {
    CheckError::new(path, span, "`@embedDir` makes a table, so it only works as a const")
        .help("write `const FILES = @embedDir(\"dir\", \"*.ext\");` and use `FILES[i].path` / `FILES[i].data`")
}
