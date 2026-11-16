use crate::span::Span;

pub use super::expr::{BinOp, ClosureLit, ClosureParam, EmbedKind, Expr, ExprKind, InterpPart, UnaryOp};
pub use super::pat::{LetPatStmt, MatchArm, MatchStmt, Pat};

#[derive(Clone, Debug)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct File {
    pub span: Span,
    /// `///` lines immediately above `module`.
    pub doc: String,
    pub module: Vec<Ident>,
    /// Scripts get their module name from their path, not a `module` line.
    pub is_script: bool,
    pub items: Vec<Item>,
    /// What each `@embed*` in this file read, by the call's span; the loader fills it.
    pub embeds: Vec<(Span, std::sync::Arc<Vec<EmbedFile>>)>,
    /// Trait names this file writes as `dyn Name`; the checker makes a fat-pointer type for each.
    pub dyns: Vec<String>,
}

/// One file an `@embed*` read.
#[derive(Debug)]
pub struct EmbedFile {
    /// For `@embedDir`, the path below the folder (`/`-separated); else the path as written.
    pub name: String,
    /// Where it was read from, for the build stamp.
    pub disk: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Use(UseItem),
    UseLib(UseLibItem),
    Const(ConstItem),
    Fn(FnItem),
    Struct(StructItem),
    /// `extend T { fn .. }`: methods of `T` written outside its body; merged by `parse::extend::merge`.
    Extend(ExtendItem),
    Enum(EnumItem),
    Trait(TraitItem),
    Let(ModLetItem),
    Props(ScriptBlockItem),
    Shared(ScriptBlockItem),
    State(ScriptLetItem),
}

#[derive(Clone, Debug)]
pub struct ScriptBlockItem {
    pub span: Span,
    pub fields: Vec<FieldItem>,
}

#[derive(Clone, Debug)]
pub struct ScriptLetItem {
    pub span: Span,
    pub name: Ident,
    pub ty: Option<Type>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct ModLetItem {
    pub span: Span,
    pub doc: String,
    pub name: Ident,
    pub ty: Type,
    pub value: Option<Expr>,
    /// `#[thread]`: one copy per thread.
    pub thread: bool,
    /// `#[uniform] let name: vec2;`: a shader uniform, removed before checking (`check/gpu/`).
    pub uniform: bool,
}

/// `#[shader]` fn kinds (`j3c doc shaders`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuStage {
    /// `#[shader]`: a helper callable from entries.
    Helper,
    Vertex,
    Fragment,
}

/// Struct kinds that exist for shaders: `#[vertex]`, `#[vertex(instance)]`, `#[varying]`, `#[shader]`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuStruct {
    None,
    Vertex,
    VertexInstance,
    Varying,
    Plain,
}

#[derive(Clone, Debug)]
pub struct EnumItem {
    pub span: Span,
    pub doc: String,
    pub vis_pub: bool,
    pub debug: bool,
    pub try_catch: Option<(Ident, Ident)>,
    /// `#[derive(Eq, Hash, Ord, Clone)]` (`Debug` sets `debug`); `check/derive.rs` writes the fns.
    pub derives: Vec<Ident>,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    /// `T: A + B`, same index as `type_params`.
    pub bounds: Vec<Vec<Ident>>,
    pub variants: Vec<VariantItem>,
}

#[derive(Clone, Debug)]
pub struct VariantItem {
    pub span: Span,
    pub doc: String,
    pub name: Ident,
    /// `V(A, B)`: the positional fields; empty for a variant with no payload.
    pub fields: Vec<Type>,
}

#[derive(Clone, Debug)]
pub struct UseLibItem {
    pub span: Span,
    pub lib: String,
    pub alias: Ident,
    pub decls: Vec<LibFnDecl>,
}

#[derive(Clone, Debug)]
pub struct LibFnDecl {
    pub span: Span,
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub symbol: Option<String>,
}

#[derive(Clone, Debug)]
pub struct StructItem {
    pub span: Span,
    pub doc: String,
    pub vis_pub: bool,
    pub debug: bool,
    pub packed: bool,
    pub gpu: GpuStruct,
    /// `#[derive(..)]`, as on `EnumItem`.
    pub derives: Vec<Ident>,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub bounds: Vec<Vec<Ident>>,
    pub fields: Vec<FieldItem>,
    pub methods: Vec<FnItem>,
}

/// `extend Box<T> { methods }`: more methods of a struct declared in the same module.
#[derive(Clone, Debug)]
pub struct ExtendItem {
    pub span: Span,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub methods: Vec<FnItem>,
}

#[derive(Clone, Debug)]
pub struct FieldItem {
    pub span: Span,
    pub doc: String,
    pub name: Ident,
    pub ty: Type,
    pub embedded: bool,
    /// `#[flat] name: vec4;` on a `#[varying]` struct: no interpolation.
    pub flat: bool,
    /// `name: Type = 0`. A literal, checked later. Absent means the field is required.
    pub default: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct UseItem {
    pub span: Span,
    pub path: Vec<Ident>,
    /// `use j3.memory.{ Arena, List }` — `None` means the path is the whole use.
    pub names: Option<Vec<Ident>>,
}

#[derive(Clone, Debug)]
pub struct ConstItem {
    pub span: Span,
    pub doc: String,
    pub vis_pub: bool,
    pub name: Ident,
    pub ty: Option<Type>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct FnItem {
    pub span: Span,
    pub doc: String,
    pub vis_pub: bool,
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub bounds: Vec<Vec<Ident>>,
    /// `where T: Eq + Hash` after the signature of a generic struct's method: bounds on the
    /// struct's own type parameters that only this method needs (`List.indexOf`).
    pub where_bounds: Vec<(Ident, Vec<Ident>)>,
    /// `#[impl]`: a free fn that gives a type a trait's method (`speculative/lang/traits.j3`).
    pub is_impl: bool,
    /// `#[shader]` / `#[shader(vertex)]` / `#[shader(fragment)]`: GPU code, never lowered for the CPU.
    pub shader: Option<GpuStage>,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
}

/// `trait Name { fn m(this: Self): T; }`: a list of signatures a type must have.
#[derive(Clone, Debug)]
pub struct TraitItem {
    pub span: Span,
    pub doc: String,
    pub vis_pub: bool,
    pub name: Ident,
    /// `trait Ord: Eq`: a type meeting `Ord` must meet these too (keys after `bind_traits`).
    pub supers: Vec<Ident>,
    pub methods: Vec<TraitMethod>,
}

#[derive(Clone, Debug)]
pub struct TraitMethod {
    pub span: Span,
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    /// A default body: a type without its own method gets this one (`check/traits.rs`).
    pub body: Option<Block>,
}

#[derive(Clone, Debug)]
pub struct Param {
    pub name: Ident,
    pub ty: Type,
}

#[derive(Clone, Debug)]
pub struct Block {
    pub span: Span,
    pub stmts: Vec<Stmt>,
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let(LetStmt),
    Return(ReturnStmt),
    Assign(AssignStmt),
    AssignOp(AssignOpStmt),
    Expr(Expr),
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    ForEach(ForEachStmt),
    UseLocal(UseLocalStmt),
    Match(MatchStmt),
    /// `let Span { lo, hi } = s;`: a destructuring `let`.
    LetPat(LetPatStmt),
    Defer(DeferStmt),
    /// `'outer: while ...` / `'outer: for ...`.
    Labeled(LabeledStmt),
    /// `break;` or `break 'outer;` (the label without its quote).
    Break(Span, Option<Ident>),
    Continue(Span, Option<Ident>),
    Block(Block),
}

#[derive(Clone, Debug)]
pub struct IfStmt {
    pub span: Span,
    pub cond: Expr,
    pub then_block: Block,
    pub else_block: Option<Block>,
}

#[derive(Clone, Debug)]
pub struct LabeledStmt {
    pub span: Span,
    pub label: Ident,
    /// A `While`, `For` or `ForEach`.
    pub body: Box<Stmt>,
}

#[derive(Clone, Debug)]
pub struct WhileStmt {
    pub span: Span,
    pub cond: Expr,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct ForStmt {
    pub span: Span,
    pub name: Ident,
    pub start: Expr,
    pub end: Expr,
    /// `0..=n` visits `n`. Lowered as an exclusive end of `n + 1`.
    pub inclusive: bool,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct ForEachStmt {
    pub span: Span,
    pub index: Option<Ident>,
    pub name: Ident,
    pub iter: Expr,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct UseLocalStmt {
    pub span: Span,
    pub name: Ident,
}

#[derive(Clone, Debug)]
pub struct DeferStmt {
    pub span: Span,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub struct AssignStmt {
    pub span: Span,
    pub place: Expr,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct AssignOpStmt {
    pub span: Span,
    pub place: Expr,
    pub op: BinOp,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct LetStmt {
    pub span: Span,
    pub doc: String,
    pub name: Ident,
    pub ty: Option<Type>,
    pub value: Expr,
}

#[derive(Clone, Debug)]
pub struct ReturnStmt {
    pub span: Span,
    pub value: Option<Expr>,
}

#[derive(Clone, Debug)]
pub struct Type {
    pub span: Span,
    pub kind: TypeKind,
}

#[derive(Clone, Debug)]
pub enum TypeKind {
    Named(String),
    Generic {
        name: String,
        args: Vec<Type>,
    },
    Ref(Box<Type>),
    Array {
        len: u64,
        elem: Box<Type>,
    },
    Slice(Box<Type>),
    Fn {
        params: Vec<Type>,
        ret: Option<Box<Type>>,
    },
    /// `closure(A, B): R`: param or `let` only; `boxed closure(..)` (arena captures) goes anywhere.
    Closure {
        params: Vec<Type>,
        ret: Option<Box<Type>>,
        boxed: bool,
    },
    /// `(A, B)`: two or more elements, read as `.0`, `.1`.
    Tuple(Vec<Type>),
    /// `dyn Trait`: a data pointer plus the trait's method table (`check/lower/dyn_call.rs`).
    Dyn(Ident),
}

impl File {
    pub fn module_key(&self) -> String {
        let mut s = String::new();
        for (i, part) in self.module.iter().enumerate() {
            if i > 0 {
                s.push('.');
            }
            s.push_str(&part.name);
        }
        s
    }
}
