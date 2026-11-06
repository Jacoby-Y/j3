use crate::span::Span;

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
    pub payload: Option<Type>,
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
    pub methods: Vec<TraitMethod>,
}

#[derive(Clone, Debug)]
pub struct TraitMethod {
    pub span: Span,
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret: Option<Type>,
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
pub struct MatchStmt {
    pub span: Span,
    pub expr: Expr,
    pub arms: Vec<MatchArm>,
}

#[derive(Clone, Debug)]
pub struct MatchArm {
    pub span: Span,
    /// An integer arm (`3`, `-1`) has an empty `path` and the literal here.
    pub value: Option<Expr>,
    pub path: Vec<Ident>,
    pub payload: Option<Pat>,
    pub body: Block,
}

#[derive(Clone, Debug)]
pub enum Pat {
    Bind(Ident),
    Wild,
    Variant {
        path: Vec<Ident>,
        payload: Option<Box<Pat>>,
    },
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
    /// `closure(A, B): R`: a capturing function value; a param or `let` type only.
    Closure {
        params: Vec<Type>,
        ret: Option<Box<Type>>,
    },
}

#[derive(Clone, Debug)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

#[derive(Clone, Debug)]
pub enum ExprKind {
    Name(Ident),
    Int(i64),
    Float(f64),
    Bool(bool),
    String(Vec<u8>),
    StructLit {
        name: Ident,
        type_args: Vec<Type>,
        fields: Vec<(Ident, Expr)>,
    },
    TypePath {
        name: Ident,
        args: Vec<Type>,
    },
    ArrayLit {
        elems: Vec<Expr>,
    },
    /// `[]T { ptr: …, len: … }`
    SliceLit {
        elem: Type,
        fields: Vec<(Ident, Expr)>,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Slice {
        base: Box<Expr>,
        start: Option<Box<Expr>>,
        end: Option<Box<Expr>>,
    },
    Field {
        base: Box<Expr>,
        field: Ident,
    },
    Call {
        func: Box<Expr>,
        type_args: Vec<Type>,
        args: Vec<Expr>,
    },
    Syscall {
        args: Vec<Expr>,
    },
    /// `@atomicLoad(p)`, `@atomicStore(p, v)`, `@atomicAdd(p, v)`, `@atomicCas(p, old, new)`, each with
    /// an optional trailing order word (`acquire`); `order` is already validated for `op`.
    Atomic {
        op: crate::ir::op::AtomicOp,
        order: crate::ir::op::AtomicOrder,
        args: Vec<Expr>,
    },
    /// The type of a project-local `.j3s` file.
    ScriptType(String),
    /// `@script("path.j3s") { prop: value }`.
    ScriptLit {
        path: String,
        fields: Vec<(Ident, Expr)>,
    },
    SizeOf(Type),
    /// Compiler UI lowering: test an int index or a candidate-set enum against one arm.
    UiChoice { set: Type, value: Box<Expr>, index: Box<Expr> },
    SliceFrom {
        ptr: Box<Expr>,
        len: Box<Expr>,
    },
    Cast {
        expr: Box<Expr>,
        ty: Type,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Interpolate {
        parts: Vec<InterpPart>,
    },
    If {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
    },
    Question {
        expr: Box<Expr>,
    },
    /// `fn(a: int, b): T { body }`, a closure literal.
    Closure(Box<ClosureLit>),
}

#[derive(Clone, Debug)]
pub struct ClosureLit {
    pub params: Vec<ClosureParam>,
    pub ret: Option<Type>,
    pub body: Block,
}

/// A closure parameter; the type may be left out when the expected type gives it.
#[derive(Clone, Debug)]
pub struct ClosureParam {
    pub name: Ident,
    pub ty: Option<Type>,
}

#[derive(Clone, Debug)]
pub enum InterpPart {
    Lit(Vec<u8>),
    Expr(Expr),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnaryOp {
    Neg,
    BitNot,
    Ref,
    Deref,
    Not,
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

impl Expr {
    /// `-3` and `-0.5` as the literals they are (the parser keeps them as negation).
    pub fn folded_literal(&self) -> Expr {
        if let ExprKind::Unary {
            op: UnaryOp::Neg,
            expr,
        } = &self.kind
        {
            let kind = match expr.kind {
                ExprKind::Int(v) => Some(ExprKind::Int(v.wrapping_neg())),
                ExprKind::Float(v) => Some(ExprKind::Float(-v)),
                _ => None,
            };
            if let Some(kind) = kind {
                return Expr {
                    span: self.span,
                    kind,
                };
            }
        }
        self.clone()
    }
}
