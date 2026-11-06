//! What the shader pass can see: every module's items by name, and how a name in one module reaches another
//! (`use a.b.{ x }`, `use a.b` then `b.x`). Built only when some file has shader code.

use std::collections::{HashMap, HashSet};

use crate::parse::{ConstItem, FnItem, Item, ModLetItem, StructItem};
use crate::span::{FileId, Span};

use super::super::ParsedFile;

#[derive(Default)]
pub struct GMod<'a> {
    pub key: String,
    /// Shader fns (`#[shader]`, `#[shader(vertex)]`, ...) by name.
    pub fns: HashMap<&'a str, &'a FnItem>,
    /// Plain CPU fns, to say so when shader code calls one.
    pub cpu_fns: HashSet<&'a str>,
    /// Structs of every kind (`gpu` says which).
    pub structs: HashMap<&'a str, &'a StructItem>,
    pub consts: HashMap<&'a str, &'a ConstItem>,
    pub uniforms: HashMap<&'a str, &'a ModLetItem>,
    /// Singleton `let`s: CPU globals.
    pub globals: HashSet<&'a str>,
    /// Name brought in by `use a.b.{ name }` -> module key.
    pub imports: HashMap<&'a str, String>,
    /// `use a.b` -> `b` -> module key.
    pub aliases: HashMap<&'a str, String>,
}

pub struct Index<'a> {
    pub mods: Vec<GMod<'a>>,
    by_key: HashMap<String, usize>,
    paths: HashMap<FileId, &'a str>,
    srcs: HashMap<FileId, &'a str>,
}

/// Whether any file has shader items or an `@shader` const (the pass is skipped otherwise).
pub fn has_gpu(files: &[ParsedFile]) -> bool {
    use crate::parse::{ExprKind, GpuStruct};
    files.iter().filter(|f| !f.ast.is_script).any(|f| {
        f.ast.items.iter().any(|it| match it {
            Item::Fn(x) => x.shader.is_some(),
            Item::Struct(s) => s.gpu != GpuStruct::None,
            Item::Let(l) => l.uniform,
            Item::Const(c) => matches!(&c.value.kind, ExprKind::Call { func, .. } if matches!(&func.kind, ExprKind::Name(n) if n.name == "@shader")),
            _ => false,
        })
    })
}

impl<'a> Index<'a> {
    pub fn new(files: &'a [ParsedFile]) -> Index<'a> {
        let mut ix = Index { mods: Vec::new(), by_key: HashMap::new(), paths: HashMap::new(), srcs: HashMap::new() };
        for f in files.iter().filter(|f| !f.ast.is_script) {
            ix.paths.insert(f.file_id, &f.path);
            ix.srcs.insert(f.file_id, &f.src);
            let key = f.ast.module_key();
            let at = *ix.by_key.entry(key.clone()).or_insert_with(|| {
                ix.mods.push(GMod { key, ..Default::default() });
                ix.mods.len() - 1
            });
            let m = &mut ix.mods[at];
            for it in &f.ast.items {
                match it {
                    Item::Fn(x) if x.shader.is_some() => {
                        m.fns.insert(&x.name.name, x);
                    }
                    Item::Fn(x) => {
                        m.cpu_fns.insert(&x.name.name);
                    }
                    Item::Struct(s) => {
                        m.structs.insert(&s.name.name, s);
                    }
                    Item::Const(c) => {
                        m.consts.insert(&c.name.name, c);
                    }
                    Item::Let(l) if l.uniform => {
                        m.uniforms.insert(&l.name.name, l);
                    }
                    Item::Let(l) => {
                        m.globals.insert(&l.name.name);
                    }
                    Item::Use(u) => {
                        let module = u.path.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
                        match &u.names {
                            Some(names) => {
                                for n in names {
                                    m.imports.insert(&n.name, module.clone());
                                }
                            }
                            None => {
                                // `use a.b` is a module (`b.x`) or the item `b` of module `a`; both readings are kept
                                if let Some(last) = u.path.last() {
                                    if u.path.len() > 1 {
                                        let parent = u.path[..u.path.len() - 1].iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(".");
                                        m.imports.insert(&last.name, parent);
                                    }
                                    m.aliases.insert(&last.name, module);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        for (i, m) in ix.mods.iter().enumerate() {
            ix.by_key.insert(m.key.clone(), i);
        }
        ix
    }

    pub fn module(&self, key: &str) -> Option<&GMod<'a>> {
        self.by_key.get(key).map(|&i| &self.mods[i])
    }

    /// The module that defines `name` as seen from `from`: `qual.name` through a `use` alias, else `from` itself,
    /// else what `from` imported by name. `has` says whether a module owns the thing being looked up.
    pub fn find(&self, from: &str, qual: Option<&str>, name: &str, has: impl Fn(&GMod<'a>) -> bool) -> Option<&GMod<'a>> {
        let me = self.module(from)?;
        let target = match qual {
            Some(q) => self.module(me.aliases.get(q)?)?,
            None if has(me) => me,
            None => self.module(me.imports.get(name)?)?,
        };
        has(target).then_some(target)
    }

    /// Whether `q` names a `use`d module in `from` (so `q.name` is a qualified name, not a field).
    pub fn is_alias(&self, from: &str, q: &str) -> bool {
        self.module(from).is_some_and(|m| m.aliases.contains_key(q))
    }

    pub fn path(&self, span: Span) -> String {
        self.paths.get(&span.file).copied().unwrap_or("").to_string()
    }

    /// `file:line` of a span, for the comment above each emitted fn.
    pub fn at(&self, span: Span) -> String {
        let line = self.srcs.get(&span.file).map_or(0, |s| s.as_bytes()[..(span.start as usize).min(s.len())].iter().filter(|&&b| b == b'\n').count() + 1);
        let path = self.path(span);
        let short = path.rsplit('/').next().unwrap_or(&path);
        format!("{short}:{line}")
    }
}
