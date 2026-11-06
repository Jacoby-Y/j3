//! The shader language (`j3c doc shaders`): `#[shader]` fns, `#[vertex]` / `#[varying]` structs and
//! `#[uniform]`s are checked against the GPU subset and written as GLSL here, before normal checking. Those
//! items are then removed from the file, so the CPU lowering, both backends and the `.j3s` VM never see them,
//! and `const PROG = @shader(vs, fs);` becomes a `Source` value holding the GLSL text. Spans stay the J3 source.
//!
//! One walk both types and writes (`gen.rs`; fns `func.rs`, statements `stmt.rs`, expressions `expr.rs`, calls
//! `call.rs`, operators `ops.rs`), so an unsupported construct is an error where it is written, never a
//! driver log. The text pieces and the ES conversion are `emit/glsl/`.

mod builtins;
mod call;
mod entry;
mod expr;
mod func;
mod gen;
mod index;
mod layout;
mod ops;
mod stmt;
mod ty;

use crate::parse::{ConstItem, Expr, ExprKind, GpuStage, GpuStruct, Ident, Item, ModLetItem, Type, TypeKind};
use crate::span::Span;

use super::{CheckError, ParsedFile};
use index::Index;

thread_local! {
    /// Names `prepare` removed (shader fns, uniforms, ...), to explain a CPU "unknown" error about one.
    static GONE: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Add the reason to an "unknown fn / name" error when the name was a shader item.
pub(super) fn explain_gone(e: &mut CheckError) {
    let Some(name) = e.message.split('`').nth(1).map(str::to_string) else { return };
    if !(e.message.starts_with("unknown function") || e.message.starts_with("unknown name")) {
        return;
    }
    if GONE.with(|g| g.borrow().contains(&name)) {
        e.helps.push(format!("`{name}` is a shader item (`#[shader]` or `#[uniform]`); shader code only runs on the GPU, and a uniform is set from CPU code by name with GL calls"));
    }
}

pub(super) type R<T> = Result<T, CheckError>;

pub(super) fn err_at(ix: &Index, span: Span, msg: impl Into<String>) -> CheckError {
    CheckError::new(ix.path(span), span, msg)
}

/// One `@shader(vs, fs)` program as emitted: what `j3c dump-glsl` prints.
#[derive(Clone, Debug)]
pub struct ShaderProgram {
    pub module: String,
    pub name: String,
    /// `file:line` of the const.
    pub at: String,
    pub vert: String,
    pub frag: String,
    pub uniforms: Vec<String>,
    /// `(location, components, gl type, byte offset, stride, divisor, integer, normalized)` per attribute.
    pub attribs: Vec<[u32; 8]>,
}

/// Attribute slots in a `Source` (the GL minimum for vertex attributes) and ints per slot.
const MAX_ATTRIBS: usize = layout::MAX_ATTRIBS as usize;
const ATTR_INTS: usize = 8;

/// Check every shader item, emit the programs, and rewrite the files for the normal checker.
pub fn prepare(files: &mut [ParsedFile]) -> Result<Vec<ShaderProgram>, CheckError> {
    if !index::has_gpu(files) {
        return Ok(Vec::new());
    }
    let programs = {
        let ix = Index::new(files);
        analyze(files, &ix)?
    };
    // names that stop existing for the CPU, so `use a.k;` / `use a.{ k }` of them must go too
    let mut gone: Vec<(String, String)> = Vec::new();
    for f in files.iter().filter(|f| !f.ast.is_script) {
        let module = f.ast.module_key();
        for it in &f.ast.items {
            match it {
                Item::Fn(x) if x.shader.is_some() => gone.push((module.clone(), x.name.name.clone())),
                Item::Struct(s) if matches!(s.gpu, GpuStruct::Varying | GpuStruct::Plain) => gone.push((module.clone(), s.name.name.clone())),
                Item::Let(l) if l.uniform => gone.push((module.clone(), l.name.name.clone())),
                _ => {}
            }
        }
    }
    GONE.with(|g| *g.borrow_mut() = gone.iter().map(|(_, n)| n.clone()).collect());
    let is_gone = |m: &str, n: &str| gone.iter().any(|(gm, gn)| gm == m && gn == n);
    for f in files.iter_mut().filter(|f| !f.ast.is_script) {
        let module = f.ast.module_key();
        let items = std::mem::take(&mut f.ast.items);
        f.ast.items = items
            .into_iter()
            .filter_map(|it| match it {
                Item::Fn(x) if x.shader.is_some() => None,
                Item::Struct(s) if matches!(s.gpu, GpuStruct::Varying | GpuStruct::Plain) => None,
                Item::Let(l) if l.uniform => None,
                Item::Const(c) => match programs.iter().find(|p| p.0 == module && p.1 == c.name.name) {
                    Some((_, _, p)) => Some(Item::Let(source_let(&c, p))),
                    None => Some(Item::Const(c)),
                },
                Item::Use(mut u) => {
                    let parent = u.path[..u.path.len().saturating_sub(1)].iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
                    let whole = u.path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
                    match &mut u.names {
                        Some(names) => {
                            names.retain(|n| !is_gone(&whole, &n.name));
                            if names.is_empty() {
                                return None;
                            }
                        }
                        None if u.path.len() > 1 && is_gone(&parent, &u.path[u.path.len() - 1].name) => return None,
                        None => {}
                    }
                    Some(Item::Use(u))
                }
                other => Some(other),
            })
            .collect();
    }
    Ok(programs.into_iter().map(|(_, _, p)| p).collect::<Vec<_>>().into_iter().collect())
}

fn analyze(files: &[ParsedFile], ix: &Index) -> Result<Vec<(String, String, ShaderProgram)>, CheckError> {
    let mut out = Vec::new();
    for f in files.iter().filter(|f| !f.ast.is_script) {
        let module = f.ast.module_key();
        for it in &f.ast.items {
            let mut g = gen::Gen::new(ix, builtins::Only::Any);
            match it {
                Item::Let(l) if l.uniform => {
                    g.uniform_ty(l, &module)?;
                }
                Item::Struct(s) if s.gpu != GpuStruct::None => {
                    g.struct_ty(&module, &s.name.name, s.name.span)?;
                    if matches!(s.gpu, GpuStruct::Vertex | GpuStruct::VertexInstance) {
                        layout::check_layout(&g, s)?;
                    }
                }
                Item::Fn(x) if x.shader.is_some() => entry::check_alone(ix, &module, x)?,
                Item::Const(c) if shader_args(c).is_some() => {
                    let built = program(ix, &module, c)?;
                    out.push((module.clone(), c.name.name.clone(), built));
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

fn shader_args(c: &ConstItem) -> Option<&Vec<Expr>> {
    match &c.value.kind {
        ExprKind::Call { func, args, .. } if matches!(&func.kind, ExprKind::Name(n) if n.name == "@shader") => Some(args),
        _ => None,
    }
}

/// `vs` or `mod.vs`, as a shader fn of the stage wanted.
fn entry_fn<'a>(ix: &'a Index<'a>, module: &str, e: &Expr, stage: GpuStage) -> R<(String, &'a crate::parse::FnItem)> {
    let (qual, name) = match &e.kind {
        ExprKind::Name(n) => (None, n.name.as_str()),
        ExprKind::Field { base, field } => match &base.kind {
            ExprKind::Name(q) => (Some(q.name.as_str()), field.name.as_str()),
            _ => return Err(err_at(ix, e.span, "`@shader` takes fn names")),
        },
        _ => return Err(err_at(ix, e.span, "`@shader` takes fn names")),
    };
    let word = if stage == GpuStage::Vertex { "vertex" } else { "fragment" };
    let Some(owner) = ix.find(module, qual, name, |m| m.fns.contains_key(name)) else {
        return Err(err_at(ix, e.span, format!("`{name}` is not a shader fn")).help(format!("write `#[shader({word})]` above it")));
    };
    let f = owner.fns[name];
    if f.shader != Some(stage) {
        return Err(err_at(ix, e.span, format!("`{name}` is not a {word} shader")).help(format!("`@shader` takes the vertex fn first, then the fragment fn; mark `{name}` `#[shader({word})]`")));
    }
    Ok((owner.key.clone(), f))
}

fn program(ix: &Index, module: &str, c: &ConstItem) -> R<ShaderProgram> {
    let args = shader_args(c).expect("checked by the caller");
    if args.len() != 2 {
        return Err(err_at(ix, c.value.span, "`@shader` takes a vertex fn and a fragment fn").help("`const PROG = @shader(vs, fs);`"));
    }
    let (vm, vf) = entry_fn(ix, module, &args[0], GpuStage::Vertex)?;
    let (fm, ff) = entry_fn(ix, module, &args[1], GpuStage::Fragment)?;
    let title = format!("{module}.{}", c.name.name);
    let b = entry::pair(ix, &vm, vf, &fm, ff, &title, c.value.span)?;
    Ok(ShaderProgram {
        module: module.to_string(),
        name: c.name.name.clone(),
        at: ix.at(c.span),
        vert: b.vert,
        frag: b.frag,
        uniforms: b.uniforms,
        attribs: b.attribs.iter().map(|a| [a.loc, a.comps, a.gl_type, a.offset, a.stride, a.divisor, u32::from(a.is_int), u32::from(a.normalized)]).collect(),
    })
}

/// `let PROG: Source = Source { vert: .., frag: .., uniforms: .., count: n, data: [..] };` (a constant
/// global, so the text is static data and `PROG` costs nothing at run time).
fn source_let(c: &ConstItem, p: &ShaderProgram) -> ModLetItem {
    let span = c.span;
    let id = |name: &str| Ident { name: name.to_string(), span };
    let e = |kind| Expr { span, kind };
    let ty = c.ty.clone().unwrap_or(Type { span, kind: TypeKind::Named("Source".to_string()) });
    let sname = match &ty.kind {
        TypeKind::Named(n) => n.clone(),
        _ => "Source".to_string(),
    };
    let mut data = vec![0i64; MAX_ATTRIBS * ATTR_INTS];
    for (i, a) in p.attribs.iter().enumerate() {
        for (j, v) in a.iter().enumerate() {
            data[i * ATTR_INTS + j] = i64::from(*v);
        }
    }
    let fields = vec![
        (id("vert"), e(ExprKind::String(p.vert.clone().into_bytes()))),
        (id("frag"), e(ExprKind::String(p.frag.clone().into_bytes()))),
        (id("uniforms"), e(ExprKind::String(p.uniforms.join(" ").into_bytes()))),
        (id("count"), e(ExprKind::Int(p.attribs.len() as i64))),
        (id("data"), e(ExprKind::ArrayLit { elems: data.into_iter().map(|v| e(ExprKind::Int(v))).collect() })),
    ];
    ModLetItem {
        span,
        doc: c.doc.clone(),
        name: c.name.clone(),
        ty,
        value: Some(e(ExprKind::StructLit { name: id(&sname), type_args: Vec::new(), fields })),
        thread: false,
        uniform: false,
    }
}
