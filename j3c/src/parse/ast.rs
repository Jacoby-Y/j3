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
    pub items: Vec<Item>,
}

#[derive(Clone, Debug)]
pub enum Item {
    Use(UseItem),
    UseLib(UseLibItem),
    Const(ConstItem),
    Fn(FnItem),
    Struct(StructItem),
    Enum(EnumItem),
    Let(ModLetItem),
}

#[derive(Clone, Debug)]
pub struct ModLetItem {
    pub span: Span,
    pub doc: String,
    pub name: Ident,
    pub ty: Type,
    pub value: Option<Expr>,
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
    pub name: Ident,
    pub type_params: Vec<Ident>,
    pub fields: Vec<FieldItem>,
    pub methods: Vec<FnItem>,
}

#[derive(Clone, Debug)]
pub struct FieldItem {
    pub span: Span,
    pub doc: String,
    pub name: Ident,
    pub ty: Type,
    pub embedded: bool,
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
    pub params: Vec<Param>,
    pub ret: Option<Type>,
    pub body: Block,
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
    Break(Span),
    Continue(Span),
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
    SizeOf(Type),
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
