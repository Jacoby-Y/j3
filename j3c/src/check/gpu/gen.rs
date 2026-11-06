//! The shader generator: type-checks shader code and writes its GLSL in one walk (a `Gen` per stage, so each
//! stage holds only what it reaches). Type and name lookup live here; fns, statements and expressions are
//! the sibling files.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::emit::glsl::names::mangle;
use crate::parse::{GpuStruct, Type, TypeKind};
use crate::span::Span;

use super::builtins::Only;
use super::index::Index;
use super::ty::*;
use super::{err_at, R};
use crate::check::CheckError;

pub struct Field {
    pub name: String,
    pub glsl: String,
    pub ty: GTy,
    pub flat: bool,
}

pub struct SInfo {
    pub kind: GpuStruct,
    pub glsl: String,
    pub fields: Vec<Field>,
}

pub struct FnSig {
    pub glsl: String,
    pub params: Vec<(GTy, bool)>,
    pub ret: GTy,
}

pub struct Local {
    pub glsl: String,
    pub ty: GTy,
}

#[derive(Default)]
pub struct Cur {
    pub module: String,
    pub scopes: Vec<HashMap<String, Local>>,
    pub ret: GTy,
    /// `true` for a loop, `false` for a `match` (a `break` there would leave the `switch`, not the loop).
    pub ctl: Vec<bool>,
    pub in_const: bool,
}

impl Default for GTy {
    fn default() -> Self {
        GTy::Void
    }
}

pub struct Gen<'a> {
    pub ix: &'a Index<'a>,
    pub stage: Only,
    pub cur: Cur,
    pub body: String,
    pub depth: usize,
    pub structs_out: Vec<String>,
    pub consts_out: Vec<String>,
    pub uniforms_out: Vec<(String, String)>,
    pub fns_out: Vec<String>,
    pub sinfo: HashMap<String, Rc<SInfo>>,
    pub sigs: HashMap<String, Rc<FnSig>>,
    pub stack: Vec<String>,
    consts_done: HashMap<String, (String, GTy)>,
    taken: HashMap<String, String>,
    gnames: HashMap<String, String>,
    building: HashSet<String>,
}

/// CPU types and what to write in shader code instead: `(name, what is wrong, the fix)`.
const CPU_ONLY: &[(&str, &str, &str)] = &[
    ("int", "`int` is 64-bit and shaders have no 64-bit integers", "write `i32`"),
    ("i64", "64-bit integers do not exist in shader code", "write `i32`"),
    ("u64", "64-bit integers do not exist in shader code", "write `u32`"),
    ("f64", "64-bit floats do not exist in shader code", "write `f32`"),
    ("i8", "8-bit integers do not exist in shader code", "write `i32`"),
    ("i16", "16-bit integers do not exist in shader code", "write `i32`"),
    ("u8", "8-bit integers do not exist in shader code", "write `u32`"),
    ("u16", "16-bit integers do not exist in shader code", "write `u32`"),
    ("float", "`float` is not a J3 type", "write `f32`"),
    ("string", "strings do not exist in shader code", "shaders compute with numbers; pass text as a texture"),
    ("ptr", "pointers do not exist in shader code", "only a fn parameter may be `&T`; it becomes `inout`"),
    ("cstr", "pointers do not exist in shader code", "only a fn parameter may be `&T`; it becomes `inout`"),
    ("vec4f", "`vec4f` is the CPU SIMD type", "write `vec4`"),
    ("vec4i", "`vec4i` is the CPU SIMD type", "write `ivec4`"),
    ("Vec2", "`Vec2` is a CPU type", "write `vec2` (CPU types appear only in `#[vertex]` struct fields)"),
    ("Vec3", "`Vec3` is a CPU type", "write `vec3` (CPU types appear only in `#[vertex]` struct fields)"),
    ("Mat4", "`Mat4` is a CPU type", "write `mat4` (CPU types appear only in `#[vertex]` struct fields)"),
];

impl<'a> Gen<'a> {
    pub fn new(ix: &'a Index<'a>, stage: Only) -> Gen<'a> {
        Gen {
            ix,
            stage,
            cur: Cur::default(),
            body: String::new(),
            depth: 1,
            structs_out: Vec::new(),
            consts_out: Vec::new(),
            uniforms_out: Vec::new(),
            fns_out: Vec::new(),
            sinfo: HashMap::new(),
            sigs: HashMap::new(),
            stack: Vec::new(),
            consts_done: HashMap::new(),
            taken: HashMap::new(),
            gnames: HashMap::new(),
            building: HashSet::new(),
        }
    }

    /// An error with the generic pointer to the docs; use `err_help` when the fix is known.
    pub fn err<T>(&self, span: Span, msg: impl Into<String>) -> R<T> {
        Err(self.e(span, msg).help("`j3c doc shaders` lists what shader code supports"))
    }

    pub fn e(&self, span: Span, msg: impl Into<String>) -> CheckError {
        err_at(self.ix, span, msg)
    }

    /// A CheckError with a help line, as one call.
    pub fn err_help<T>(&self, span: Span, msg: impl Into<String>, help: impl Into<String>) -> R<T> {
        Err(self.e(span, msg).help(help))
    }

    /// The emitted name of a module-level thing: its own name unless GLSL reserves it or another thing took it.
    pub fn global_name(&mut self, module: &str, name: &str) -> String {
        let key = format!("{module}::{name}");
        if let Some(n) = self.gnames.get(&key) {
            return n.clone();
        }
        let mut n = mangle(name);
        if self.taken.get(&n).is_some_and(|k| *k != key) {
            n = format!("{}_{n}", module.replace('.', "_"));
        }
        self.taken.insert(n.clone(), key.clone());
        self.gnames.insert(key, n.clone());
        n
    }

    /// A type written in shader code. `param` lets `&T` through (it becomes `inout`).
    pub fn ty_of(&mut self, t: &Type, module: &str) -> R<GTy> {
        match &t.kind {
            TypeKind::Named(n) => {
                if let Some(b) = builtin_type(n) {
                    return Ok(b);
                }
                if let Some((_, why, fix)) = CPU_ONLY.iter().find(|(w, ..)| w == n) {
                    return self.err_help(t.span, (*why).to_string(), (*fix).to_string());
                }
                self.struct_ty(module, n, t.span)
            }
            TypeKind::Array { len, elem } => {
                let e = self.ty_of(elem, module)?;
                if matches!(e, GTy::Array(..)) {
                    return self.err_help(t.span, "arrays of arrays do not exist in GLSL ES", "flatten to one `[N]T` and index with `i * W + j`");
                }
                if matches!(e, GTy::Sampler(_)) {
                    return self.err(t.span, "arrays of samplers are not supported");
                }
                if *len == 0 {
                    return self.err(t.span, "an array needs at least one element");
                }
                Ok(GTy::Array(Box::new(e), *len))
            }
            TypeKind::Ref(_) => self.err_help(t.span, "pointers do not exist in shader code", "only a fn parameter may be `&T`; it becomes an `inout` parameter"),
            TypeKind::Slice(_) => self.err_help(t.span, "slices do not exist in shader code", "use a fixed array `[N]T`"),
            TypeKind::Generic { name, .. } => self.err_help(t.span, format!("`{name}<..>` is a generic type"), "shader code has no generics; write the concrete type"),
            TypeKind::Fn { .. } | TypeKind::Closure { .. } => self.err_help(t.span, "function values and closures do not exist in shader code", "call the `#[shader]` fn directly"),
        }
    }

    /// A struct used from shader code: built once, emitted before its first use.
    pub fn struct_ty(&mut self, module: &str, name: &str, span: Span) -> R<GTy> {
        let Some(owner) = self.ix.find(module, None, name, |m| m.structs.contains_key(name)) else {
            return self.err_help(span, format!("unknown type `{name}` in shader code"), "shader types are f32 i32 u32 bool vec2..4 ivec/uvec/bvec mat2..4 sampler2D and `#[shader]` structs");
        };
        let s = owner.structs[name];
        let owner_key = owner.key.clone();
        let key = format!("{owner_key}::{name}");
        if let Some(info) = self.sinfo.get(&key) {
            return Ok(GTy::Struct { key, glsl: info.glsl.clone() });
        }
        if s.gpu == GpuStruct::None {
            return self.err_help(span, format!("`{name}` is a CPU struct and cannot be used in shader code"), format!("write `#[shader] struct {name}` (then it exists only on the GPU), or `#[vertex]` for a vertex buffer layout"));
        }
        if !s.type_params.is_empty() {
            return self.err(s.name.span, "a shader struct cannot be generic");
        }
        if !self.building.insert(key.clone()) {
            return self.err(s.name.span, format!("struct `{name}` contains itself"));
        }
        let glsl = self.global_name(&owner_key, name);
        let mut fields = Vec::new();
        let mut decl = format!("struct {glsl} {{\n");
        for f in &s.fields {
            if f.name.name.starts_with('_') && matches!(s.gpu, GpuStruct::Vertex | GpuStruct::VertexInstance) {
                continue; // CPU-only padding (layout.rs)
            }
            let ty = match s.gpu {
                GpuStruct::Vertex | GpuStruct::VertexInstance => super::layout::cpu_to_gpu(self, f)?,
                _ => self.ty_of(&f.ty, &owner_key)?,
            };
            if s.gpu == GpuStruct::Varying {
                super::entry::check_varying_field(self, f, &ty)?;
            }
            if f.embedded || f.default.is_some() {
                return self.err(f.span, "shader struct fields have no `use` or default");
            }
            let fg = mangle(&f.name.name);
            decl.push_str(&format!("    {} {fg};\n", ty.glsl()));
            fields.push(Field { name: f.name.name.clone(), glsl: fg, ty, flat: f.flat });
        }
        decl.push_str("};");
        if fields.is_empty() {
            return self.err(s.name.span, format!("struct `{name}` has no fields"));
        }
        if s.gpu == GpuStruct::Varying {
            super::entry::check_varying_head(self, s, &fields)?;
        }
        self.building.remove(&key);
        self.structs_out.push(decl);
        self.sinfo.insert(key.clone(), Rc::new(SInfo { kind: s.gpu, glsl: glsl.clone(), fields }));
        Ok(GTy::Struct { key, glsl })
    }

    /// A `#[uniform]` visible from `module` as `qual.name` or `name`; declared once in the stage that reads it.
    pub fn uniform(&mut self, module: &str, qual: Option<&str>, name: &str) -> R<Option<(String, GTy)>> {
        let Some(owner) = self.ix.find(module, qual, name, |m| m.uniforms.contains_key(name)) else { return Ok(None) };
        let u = owner.uniforms[name];
        let owner_key = owner.key.clone();
        let ty = self.uniform_ty(u, &owner_key)?;
        if !self.uniforms_out.iter().any(|(n, _)| n == name) {
            self.uniforms_out.push((name.to_string(), format!("uniform {} {name};", glsl_decl(&ty))));
        }
        Ok(Some((name.to_string(), ty)))
    }

    /// The type of a uniform, with the rules that make its name usable (the CPU binds it by exactly this name).
    pub fn uniform_ty(&mut self, u: &crate::parse::ModLetItem, owner: &str) -> R<GTy> {
        let name = &u.name.name;
        if crate::emit::glsl::names::is_reserved(name) {
            return self.err_help(u.name.span, format!("uniform `{name}` has a name GLSL reserves"), format!("the CPU binds uniforms by exactly this name, so it cannot be renamed for you; call it `u{}{}`", name[..1].to_uppercase(), &name[1..]));
        }
        let ty = self.ty_of(&u.ty, owner)?;
        if matches!(ty, GTy::Struct { .. } | GTy::Void) || matches!(&ty, GTy::Array(e, _) if matches!(**e, GTy::Struct { .. })) {
            return self.err_help(u.ty.span, "a uniform is a scalar, vector, matrix, sampler, or array of those", "uniform blocks are not supported; declare one `#[uniform] let` per value");
        }
        Ok(ty)
    }

    /// A module `const` visible from `module`; emitted once as a GLSL `const`.
    pub fn const_ref(&mut self, module: &str, qual: Option<&str>, name: &str) -> R<Option<(String, GTy)>> {
        let Some(owner) = self.ix.find(module, qual, name, |m| m.consts.contains_key(name)) else { return Ok(None) };
        let c = owner.consts[name];
        let owner_key = owner.key.clone();
        let key = format!("{owner_key}::{name}");
        if let Some(v) = self.consts_done.get(&key) {
            return Ok(Some(v.clone()));
        }
        let saved = std::mem::replace(&mut self.cur, Cur { module: owner_key.clone(), in_const: true, ..Cur::default() });
        self.cur.scopes.push(HashMap::new());
        let want = match &c.ty {
            Some(t) => Some(self.ty_of(t, &owner_key)?),
            None => None,
        };
        let (text, vt) = self.expr(&c.value, want.as_ref())?;
        let ty = match &want {
            Some(w) => w.clone(),
            None => vt.settle(),
        };
        let text = self.coerce((text, vt), &ty, c.value.span)?;
        self.cur = saved;
        let glsl = self.global_name(&owner_key, name);
        let (head, tail) = match &ty {
            GTy::Array(e, n) => (e.glsl(), format!("[{n}]")),
            t => (t.glsl(), String::new()),
        };
        self.consts_out.push(format!("const {head} {glsl}{tail} = {text};"));
        self.consts_done.insert(key, (glsl.clone(), ty.clone()));
        Ok(Some((glsl, ty)))
    }

    pub fn push_scope(&mut self) {
        self.cur.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.cur.scopes.pop();
    }

    pub fn local(&self, name: &str) -> Option<&Local> {
        self.cur.scopes.iter().rev().find_map(|s| s.get(name))
    }

    pub fn declare(&mut self, name: &str, ty: GTy) -> String {
        let glsl = mangle(name);
        if let Some(s) = self.cur.scopes.last_mut() {
            s.insert(name.to_string(), Local { glsl: glsl.clone(), ty });
        }
        glsl
    }

    pub fn line(&mut self, text: &str) {
        for _ in 0..self.depth {
            self.body.push_str("    ");
        }
        self.body.push_str(text);
        self.body.push('\n');
    }
}

/// `float[4]` for a declaration prefix; the name goes after it (`float[4] k`).
pub fn glsl_decl(t: &GTy) -> String {
    t.glsl()
}
