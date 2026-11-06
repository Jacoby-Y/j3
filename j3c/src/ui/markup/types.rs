//! The `.j3ui` markup tree: ranges, errors, and nodes.

use std::cell::Cell;

pub type Range = (usize, usize);

#[derive(Debug, Clone)]
pub struct UiError {
    pub lo: usize,
    pub hi: usize,
    pub msg: String,
    pub help: Option<String>,
}

pub type R<T> = Result<T, UiError>;

pub fn err<T>(lo: usize, hi: usize, msg: impl Into<String>) -> R<T> {
    Err(UiError { lo, hi, msg: msg.into(), help: None })
}

impl UiError {
    pub fn help(mut self, h: impl Into<String>) -> UiError {
        self.help = Some(h.into());
        self
    }
}

#[derive(Debug, Clone)]
pub enum Part {
    Lit(String),
    Hole(Range),
}

#[derive(Debug, Clone)]
pub enum Value {
    None,
    Expr(Range),
    Str(Vec<Part>),
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub name: String,
    pub range: Range,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct Elem {
    pub tag: String,
    /// A capitalised tag: the component `ui/<Tag>.j3ui`.
    pub comp: bool,
    pub attrs: Vec<Attr>,
    pub children: Vec<Node>,
    /// The opening tag, `<div ...>`.
    pub open: Range,
    /// Child-instance number (components only), set by `gen`.
    pub id: Cell<usize>,
}

#[derive(Debug, Clone)]
pub struct Branch {
    pub cond: Option<Range>,
    /// `{:case a, b}` values of a `{#match}` (`cond` is `None` there; an `{:else}` has none).
    pub vals: Vec<Range>,
    pub nodes: Vec<Node>,
}

#[derive(Debug, Clone)]
pub struct IfNode {
    pub branches: Vec<Branch>,
    /// `{#match X}`: the value the cases compare.
    pub scrut: Option<Range>,
    /// Dynamic component candidate-set enum; lowered before component scans.
    pub choice: Option<String>,
    pub range: Range,
    /// State number when a branch holds components, else 0.
    pub id: Cell<usize>,
}

#[derive(Debug, Clone)]
pub struct EachNode {
    pub head: Range,
    pub body: Vec<Node>,
    pub range: Range,
    pub id: Cell<usize>,
}

/// `{#let name = expr}`: a local for the rest of the block.
#[derive(Debug, Clone)]
pub struct LetNode {
    pub name: String,
    pub name_r: Range,
    pub expr: Range,
    pub range: Range,
}

#[derive(Debug, Clone)]
pub enum Node {
    Let(LetNode),
    Text { parts: Vec<Part>, range: Range },
    Elem(Elem),
    If(IfNode),
    Each(EachNode),
}

#[derive(Debug, Default)]
pub struct Markup {
    pub nodes: Vec<Node>,
    pub script: Option<Range>,
    pub style: Option<Range>,
}
