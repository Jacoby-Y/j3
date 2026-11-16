//! A backtick string that needed a buffer lives in that 512-byte buffer in its function's stack frame
//! (`lower/interp.rs`, recorded in `IrCx.frame_interps`). Letting it outlive the frame is a compile
//! error: J3 strings are borrowed views with no implicit allocation, so there is no safe place to
//! copy it to. Runs once over the lowered IR of the whole program.
//!
//! Each function gets a summary of its params (`Summary`): which ones it keeps (stores through a
//! pointer, a slice, into a global, into memory it allocated, or passes on to a function that keeps
//! them), which ones it returns, and which ones it stores into another param's memory. Summaries
//! are computed over the call graph to a fixed point (recursion included). A function's body is
//! one dataflow (`flow.rs`) over what each local may hold: the frame buffer, a param's data, a
//! pointer to another local. Branches join, loops run to a fixed point, a whole-local store
//! replaces what the local held, a field store adds to it.
//!
//! Caught: returning one (directly, through a local, a struct/array/tuple holding one, a local set
//! in one branch, a `match` value, a local struct whose field was set to one); storing one through
//! a reference, a slice, or into a global (outside the entry `main`, whose frame lasts the run, and
//! `#[thread]` globals, which are per-thread scratch); passing one (or a local holding one) to a
//! function that keeps it, at any depth (`List.push`, `Map.put`, `Arena.new`, a boxed closure
//! capture), noting where the callee keeps it. Only fns reachable from a fn with a frame buffer are
//! summarized, callees first.
//!
//! Not caught: calls through a fn value, closure, `dyn` method table, or C function (assumed to keep
//! nothing); a kept one in `main` (its frame lasts the run, though a loop refills the same buffer);
//! fields deeper than one level (a nested field is tracked as its first-level field); paths (a
//! value kept on one branch and read on another still counts).

mod flow;
mod val;

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::check::CheckError;
use crate::ir::body::{Body, LocalKind};
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::ids::FuncId;
use crate::ir::ty::{Ty, TyKind};
use crate::span::Span;

use val::Tok;

/// What a function does with its params ([`val::Tok::param`] tokens, narrowed by field).
#[derive(Clone, Default, PartialEq, Debug)]
struct Summary {
    /// Kept past the call.
    esc: BTreeSet<Tok>,
    /// Where each kept one is kept (for the note).
    site: HashMap<Tok, Span>,
    /// May come back in the result: (0 for the whole result, else result field + 1, param token).
    ret: BTreeSet<(u32, Tok)>,
    /// The result may point at memory of unknown origin.
    ret_u: bool,
    /// (stored, pointer, field + 1 or 0 for the whole): one param's data stored where another
    /// param token points.
    into: BTreeSet<(Tok, Tok, u32)>,
}

/// Where frame data went.
#[derive(Clone, Copy)]
enum Sink {
    Return,
    Store,
    /// Passed to this fn, which keeps it at the span.
    Call(FuncId, Option<Span>),
}

struct Ctx<'a> {
    ir: &'a IrCx,
    sums: HashMap<FuncId, Summary>,
}

impl Ctx<'_> {
    /// A J3 body we have (or will have) a summary for.
    fn known(&self, id: FuncId) -> bool {
        matches!(self.ir.func(id).kind, FuncKind::J3 { .. })
    }

    /// A method of a generic struct instance (`List<T>.push`, `Map<K, V>.put`).
    fn is_container(&self, id: FuncId) -> bool {
        self.ir.func(id).method_of.is_some_and(|sid| !self.ir.struct_def(sid).ty_args.is_empty())
    }

    /// Which params of `id` are struct values (a field token reads them without a load).
    fn struct_params(&self, id: FuncId) -> Vec<bool> {
        match &self.ir.func(id).kind {
            FuncKind::J3 { body } => body.params.iter().map(|p| matches!(self.ir.ty_kind(body.local(*p).ty), TyKind::Struct(_))).collect(),
            _ => Vec::new(),
        }
    }

    fn fn_name(&self, id: FuncId) -> String {
        let f = self.ir.func(id);
        match f.method_of {
            Some(sid) => format!("{}.{}", self.ir.name_str(self.ir.struct_def(sid).name), self.ir.name_str(f.name)),
            None => self.ir.name_str(f.name).to_string(),
        }
    }

    fn same_line(&self, a: Span, b: Span) -> bool {
        a.file == b.file && self.ir.file(a.file).is_some_and(|f| f.line(a.start) == f.line(b.start))
    }

    fn loc(&self, span: Span) -> Option<String> {
        let (path, line) = self.ir.file(span.file)?.loc(span.start);
        Some(format!("{path}:{line}"))
    }

    /// The backtick string at `span` reaches `sink` at `at`.
    fn error(&self, span: Span, at: Span, sink: &Sink) -> CheckError {
        let path = self.ir.file(span.file).map(|f| f.path.clone()).unwrap_or_default();
        let what = match sink {
            Sink::Return => "returned".to_string(),
            Sink::Store => "stored where it outlives the function".to_string(),
            Sink::Call(id, _) => format!("passed to `{}`, which keeps it", self.fn_name(*id)),
        };
        let mut e = CheckError::new(path, span, format!("a backtick string lives in this function's stack frame and cannot be {what}"));
        if let (false, Some(loc)) = (self.same_line(span, at), self.loc(at)) {
            let verb = match sink {
                Sink::Return => "returned".to_string(),
                Sink::Store => "stored".to_string(),
                Sink::Call(id, _) => format!("passed to `{}`", self.fn_name(*id)),
            };
            e = e.note(format!("{verb} at {loc}"));
        }
        if let Sink::Call(id, Some(site)) = sink {
            if let (false, Some(at)) = (self.same_line(span, *site), self.loc(*site)) {
                e = e.note(format!("`{}` keeps it at {at}", self.fn_name(*id)));
            }
        }
        e.help("write it into a `j3.text.Text` the caller owns: take `t: &Text`, `t.fmt(`..`)`, return `t.str()`")
    }
}

/// Can a value of `ty` hold a pointer (a string, slice, reference, raw pointer, or a struct of them)?
fn has_ptr(cx: &IrCx, memo: &mut HashMap<Ty, bool>, ty: Ty) -> bool {
    if let Some(b) = memo.get(&ty) {
        return *b;
    }
    // a type reaches itself only through a reference, which answers `true` before recursing
    memo.insert(ty, false);
    let b = match cx.ty_kind(ty) {
        TyKind::Ptr | TyKind::CStr | TyKind::String | TyKind::Ref(_) | TyKind::Slice { .. } => true,
        TyKind::Array { elem, .. } => has_ptr(cx, memo, *elem),
        TyKind::Struct(sid) => {
            let tys: Vec<Ty> = cx.struct_def(*sid).fields.iter().map(|f| f.ty).collect();
            tys.into_iter().any(|t| has_ptr(cx, memo, t))
        }
        TyKind::Enum(eid) => {
            let tys: Vec<Ty> = cx.enum_def(*eid).variants.iter().filter_map(|v| v.payload).collect();
            tys.into_iter().any(|t| has_ptr(cx, memo, t))
        }
        TyKind::Void | TyKind::Bool | TyKind::Int(_) | TyKind::Float(_) | TyKind::Fn { .. } | TyKind::Half | TyKind::Vec(_) => false,
    };
    memo.insert(ty, b);
    b
}

/// The frame interpolation buffers of `body`.
fn frame_bufs(cx: &IrCx, body: &Body) -> HashSet<u32> {
    if cx.frame_interps.is_empty() {
        return HashSet::new();
    }
    body.locals
        .iter()
        .filter(|d| d.kind == LocalKind::Tmp && matches!(cx.ty_kind(d.ty), TyKind::Array { .. }) && cx.frame_interps.contains(&d.span))
        .map(|d| d.id.0)
        .collect()
}

/// Every frame string the program lets outlive its frame.
pub(crate) fn check(cx: &IrCx) -> Vec<CheckError> {
    let mut bufs: HashMap<FuncId, HashSet<u32>> = HashMap::new();
    for f in cx.funcs() {
        if let FuncKind::J3 { body } = &f.kind {
            let b = frame_bufs(cx, body);
            if !b.is_empty() {
                bufs.insert(f.id, b);
            }
        }
    }
    let mut framed: Vec<FuncId> = bufs.keys().copied().collect();
    framed.sort_by_key(|id| id.0);
    let mut memo = HashMap::new();
    let mut ctx = Ctx { ir: cx, sums: HashMap::new() };
    // only what the fns with a frame buffer can call needs a summary; callees go first, and a
    // changed summary re-runs the fns that call it, to a fixed point (recursion included)
    let (order, callers) = reach(cx, &framed);
    let mut work: Vec<FuncId> = order
        .into_iter()
        .filter(|id| body_of(cx, *id).params.iter().any(|p| has_ptr(cx, &mut memo, body_of(cx, *id).local(*p).ty)))
        .rev()
        .collect();
    let mut queued: HashSet<FuncId> = work.iter().copied().collect();
    let none = HashSet::new();
    while let Some(id) = work.pop() {
        queued.remove(&id);
        let fbufs = bufs.get(&id).unwrap_or(&none).clone();
        let mut fl = flow::Flow::new(&ctx, &mut memo, body_of(cx, id), fbufs, cx.entry == Some(id), false);
        fl.run();
        let out = std::mem::take(&mut fl.out);
        if ctx.sums.get(&id) != Some(&out) {
            ctx.sums.insert(id, out);
            for &c in callers.get(&id).map(|v| v.as_slice()).unwrap_or(&[]) {
                if queued.insert(c) {
                    work.push(c);
                }
            }
        }
    }
    let mut errs = Vec::new();
    for id in framed {
        let fbufs = bufs.remove(&id).unwrap_or_default();
        let mut fl = flow::Flow::new(&ctx, &mut memo, body_of(cx, id), fbufs, cx.entry == Some(id), true);
        fl.run();
        errs.append(&mut fl.errs);
    }
    errs
}

fn body_of(cx: &IrCx, id: FuncId) -> &Body {
    match &cx.func(id).kind {
        FuncKind::J3 { body } => body,
        _ => unreachable!("only J3 bodies are analyzed"),
    }
}

/// The J3 fns reachable from `roots` through the fns their bodies name, callees before callers
/// (post-order), and who names each.
fn reach(cx: &IrCx, roots: &[FuncId]) -> (Vec<FuncId>, HashMap<FuncId, Vec<FuncId>>) {
    let refs = |id: FuncId| -> Vec<FuncId> {
        match &cx.func(id).kind {
            FuncKind::J3 { body } => crate::check::lower::lazy::fn_refs(&body.block)
                .into_iter()
                .filter(|c| matches!(cx.func(*c).kind, FuncKind::J3 { .. }))
                .collect(),
            _ => Vec::new(),
        }
    };
    let (mut post, mut seen, mut callers) = (Vec::new(), HashSet::new(), HashMap::<FuncId, Vec<FuncId>>::new());
    for &r in roots {
        if !seen.insert(r) {
            continue;
        }
        let mut stack = vec![(r, refs(r))];
        while let Some(top) = stack.len().checked_sub(1) {
            match stack[top].1.pop() {
                Some(c) => {
                    callers.entry(c).or_default().push(stack[top].0);
                    if seen.insert(c) {
                        stack.push((c, refs(c)));
                    }
                }
                None => post.push(stack.pop().expect("non-empty").0),
            }
        }
    }
    (post, callers)
}
