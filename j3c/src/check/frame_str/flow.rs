//! One function's dataflow for [`super`]: what each local may hold (a param's data, the frame
//! buffer, a pointer to another local; [`super::val`]), statement by statement over the structured
//! IR. Branches join, loops run to a fixed point, a whole-local store replaces what the local held.

use std::collections::{HashMap, HashSet};

use crate::ir::body::{Block, Body};
use crate::ir::op::AtomicOp;
use crate::ir::operand::Operand;
use crate::ir::place::{Place, PlaceRoot, Proj};
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::stmt::StmtKind;
use crate::ir::ty::{Ty, TyKind};
use crate::span::Span;

use super::val::{set_add, SVal, Set, Tok};
use super::{Ctx, Sink, Summary};

mod call;

/// Locals' values at one point; `live: false` after a `return`/`break`/`continue`.
#[derive(Clone, PartialEq)]
struct State {
    live: bool,
    vals: HashMap<u32, SVal>,
}

impl State {
    fn dead() -> State {
        State { live: false, vals: HashMap::new() }
    }

    fn get(&self, x: u32) -> SVal {
        self.vals.get(&x).cloned().unwrap_or_default()
    }

    fn add(&mut self, x: u32, v: &SVal) {
        if !v.is_empty() {
            self.vals.entry(x).or_default().add(v);
        }
    }

    fn join(&mut self, o: &State) {
        if !o.live {
            return;
        }
        if !self.live {
            *self = o.clone();
            return;
        }
        for (x, v) in &o.vals {
            self.add(*x, v);
        }
    }
}

/// How a place is reached: the projections that load through a pointer (`*r`, `xs[i]` on a slice
/// or string), and the field index of each `Field` projection.
struct Steps {
    deref: Vec<bool>,
    field: Vec<Option<u32>>,
}

impl Steps {
    fn last_deref(&self) -> Option<usize> {
        self.deref.iter().rposition(|d| *d)
    }

    /// The first field selected after projection `from`.
    fn field_after(&self, from: usize) -> Option<u32> {
        self.field[from..].iter().find_map(|f| *f)
    }
}

pub(super) struct Flow<'a> {
    pub cx: &'a Ctx<'a>,
    pub ptr_memo: &'a mut HashMap<Ty, bool>,
    pub body: &'a Body,
    /// Locals that are frame interpolation buffers (`&buf[0]` is [`Tok::buf`]).
    pub fbufs: HashSet<u32>,
    /// The program's `fn main`: its frame lasts the run.
    pub entry: bool,
    pub report: bool,
    pub out: Summary,
    pub errs: Vec<crate::check::CheckError>,
    /// Spans already reported: one frame string kept by several calls is one error.
    reported: HashSet<Span>,
    /// Enclosing loops: states at their `break`s and `continue`s.
    loops: Vec<(State, State)>,
}

impl<'a> Flow<'a> {
    pub fn new(cx: &'a Ctx<'a>, ptr_memo: &'a mut HashMap<Ty, bool>, body: &'a Body, fbufs: HashSet<u32>, entry: bool, report: bool) -> Flow<'a> {
        Flow {
            cx,
            ptr_memo,
            body,
            fbufs,
            entry,
            report,
            out: Summary::default(),
            errs: Vec::new(),
            reported: HashSet::new(),
            loops: Vec::new(),
        }
    }

    pub fn run(&mut self) {
        let mut st = State { live: true, vals: HashMap::new() };
        for (i, p) in self.body.params.iter().enumerate() {
            if self.has_ptr(self.body.local(*p).ty) {
                st.add(p.0, &SVal::tok(Tok::param(i)));
            }
        }
        let body = self.body;
        self.block(&body.block, &mut st);
    }

    fn has_ptr(&mut self, ty: Ty) -> bool {
        super::has_ptr(self.cx.ir, self.ptr_memo, ty)
    }

    fn block(&mut self, b: &Block, st: &mut State) {
        for s in &b.stmts {
            if !st.live {
                return;
            }
            match &s.kind {
                StmtKind::Assign { place, rvalue } => {
                    let v = self.rvalue(rvalue, st);
                    self.store(place, v, rvalue.span, st);
                }
                StmtKind::Expr(r) => {
                    self.rvalue(r, st);
                }
                StmtKind::Return { value } => {
                    if let Some(v) = value {
                        let val = self.operand(v, st);
                        self.ret(&val, op_span(v, s.span), st);
                    }
                    st.live = false;
                }
                StmtKind::If { then_block, else_block, .. } => {
                    let mut a = st.clone();
                    self.block(then_block, &mut a);
                    if let Some(e) = else_block {
                        self.block(e, st);
                    }
                    st.join(&a);
                }
                StmtKind::Switch { cases, default, .. } => {
                    let mut acc = State::dead();
                    for b in cases.iter().map(|(_, b)| b).chain([default]) {
                        let mut c = st.clone();
                        self.block(b, &mut c);
                        acc.join(&c);
                    }
                    *st = acc;
                }
                StmtKind::While { body, .. } | StmtKind::ForRange { body, .. } => self.loop_(body, st),
                StmtKind::Defer { body } => {
                    let mut d = st.clone();
                    self.block(body, &mut d);
                    st.join(&d);
                }
                StmtKind::Block(b) => self.block(b, st),
                StmtKind::Break(n) | StmtKind::Continue(n) => {
                    let depth = self.loops.len();
                    if let Some(l) = depth.checked_sub(*n as usize + 1).and_then(|i| self.loops.get_mut(i)) {
                        let into = if matches!(s.kind, StmtKind::Break(_)) { &mut l.0 } else { &mut l.1 };
                        into.join(st);
                    }
                    st.live = false;
                }
                StmtKind::AssignOp { .. } | StmtKind::InterpFill { .. } | StmtKind::Abort(..) | StmtKind::Div0 => {}
            }
        }
    }

    /// A loop body to a fixed point; after it, whatever reached the loop head or a `break`.
    fn loop_(&mut self, body: &Block, st: &mut State) {
        let mut head = st.clone();
        loop {
            self.loops.push((State::dead(), State::dead()));
            let mut s = head.clone();
            self.block(body, &mut s);
            let (brk, cont) = self.loops.pop().expect("pushed above");
            let mut next = head.clone();
            next.join(&s);
            next.join(&cont);
            if next == head {
                head.join(&brk);
                *st = head;
                return;
            }
            head = next;
        }
    }

    fn operand(&mut self, o: &Operand, st: &State) -> SVal {
        match o {
            Operand::Const(_) => SVal::default(),
            Operand::Copy(p) => {
                if !self.has_ptr(p.ty) {
                    return SVal::default();
                }
                let steps = self.steps(p);
                self.walk(p, &steps, p.proj.len(), st)
            }
        }
    }

    fn steps(&self, p: &Place) -> Steps {
        let ir = self.cx.ir;
        let mut ty = match p.root {
            PlaceRoot::Local(x) => self.body.local(x).ty,
            PlaceRoot::Global(g) => ir.global(g).ty,
        };
        let mut steps = Steps { deref: Vec::with_capacity(p.proj.len()), field: Vec::with_capacity(p.proj.len()) };
        // `.ptr` is `&elem`; a `Deref` right after it reads the element
        let mut ptr_elem: Option<Ty> = None;
        for pr in &p.proj {
            let mut field = None;
            let (deref, next) = match pr {
                Proj::Deref => (true, ptr_elem.take().or(ir.ref_inner(ty)).unwrap_or(ty)),
                Proj::Index { ty: t, .. } => (matches!(ir.ty_kind(ty), TyKind::Slice { .. } | TyKind::String), *t),
                Proj::Field { field: f, ty: t } => {
                    field = Some(f.index);
                    (false, *t)
                }
                Proj::Lane { ty: t, .. } | Proj::EnumPayload { ty: t, .. } => (false, *t),
                Proj::Ptr => {
                    ptr_elem = ir.elem_ty(ty);
                    (false, ty)
                }
                Proj::Len | Proj::EnumTag => (false, ty),
            };
            steps.deref.push(deref);
            steps.field.push(field);
            ty = next;
        }
        steps
    }

    /// The value at `p` after its first `upto` projections.
    fn walk(&self, p: &Place, steps: &Steps, upto: usize, st: &State) -> SVal {
        let mut v = match p.root {
            PlaceRoot::Local(x) => st.get(x.0),
            PlaceRoot::Global(_) => SVal::tok(Tok::U),
        };
        for i in 0..upto {
            if steps.deref[i] {
                v = deref(&v.flat(), st);
            } else if let Some(f) = steps.field[i] {
                v = SVal::of(v.select(f));
            } else if !v.fields.is_empty() {
                v = SVal::of(v.flat());
            }
        }
        v
    }

    /// The pointer a store into (or `&` of) `p` goes through, and the field of what it points at
    /// that `p` names; `None` when `p` is in this frame (no load through a pointer).
    fn through(&self, p: &Place, steps: &Steps, st: &State) -> Option<(Set, Option<u32>)> {
        let k = steps.last_deref()?;
        Some((self.walk(p, steps, k, st).flat(), steps.field_after(k + 1)))
    }

    /// `&p`: a frame buffer is [`Tok::buf`], a local is a pointer to it, else the pointer `p` goes through.
    fn address(&self, p: &Place, st: &State) -> Set {
        let steps = self.steps(p);
        if let Some((ptr, slot)) = self.through(p, &steps, st) {
            return match slot {
                Some(f) => {
                    let mut out = Set::new();
                    set_add(&mut out, &ptr.iter().map(|t| t.addr(f)).collect::<Vec<_>>());
                    out
                }
                None => ptr,
            };
        }
        match p.root {
            PlaceRoot::Local(x) if self.fbufs.contains(&x.0) => vec![Tok::buf(x.0)],
            PlaceRoot::Local(x) => {
                let t = Tok::loc(x.0);
                vec![steps.field_after(0).map_or(t, |f| t.refine(f))]
            }
            PlaceRoot::Global(_) => vec![Tok::U],
        }
    }

    fn rvalue(&mut self, r: &Rvalue, st: &mut State) -> SVal {
        match &r.kind {
            RvalueKind::Call { func, args } => return self.call(r, func, args, st),
            RvalueKind::Atomic { op, args, .. } => {
                self.atomic(*op, args, r.span, st);
            }
            _ => {}
        }
        if !self.has_ptr(r.ty) {
            return SVal::default();
        }
        match &r.kind {
            RvalueKind::Use(o) | RvalueKind::Unary { arg: o, .. } => self.operand(o, st),
            RvalueKind::Cast { from } => {
                let mut v = SVal::of(self.operand(from, st).flat());
                // a pointer made from a number points anywhere (an allocator's memory)
                if !self.has_ptr(from.ty()) {
                    set_add(&mut v.whole, &[Tok::U]);
                }
                v
            }
            RvalueKind::Ref { place } => SVal::of(self.address(place, st)),
            RvalueKind::Binary { lhs, rhs, .. } => {
                let mut v = SVal::of(self.operand(lhs, st).flat());
                set_add(&mut v.whole, &self.operand(rhs, st).flat());
                v
            }
            RvalueKind::Aggregate { fields } => {
                let is_struct = matches!(self.cx.ir.ty_kind(r.ty), TyKind::Struct(_));
                let mut v = SVal::default();
                for (i, f) in fields.iter().enumerate() {
                    let fv = self.operand(f, st).flat();
                    if is_struct {
                        v.add_field(i as u32, &fv);
                    } else {
                        set_add(&mut v.whole, &fv);
                    }
                }
                v
            }
            RvalueKind::Enum { payload, .. } => match payload {
                Some(p) => SVal::of(self.operand(p, st).flat()),
                None => SVal::default(),
            },
            RvalueKind::Atomic { .. } | RvalueKind::Syscall { .. } => SVal::tok(Tok::U),
            RvalueKind::Call { .. } | RvalueKind::Simd { .. } | RvalueKind::Interpolate { .. } => SVal::default(),
        }
    }

    /// `@atomicStore(p, v)` and friends store `v` through `p`.
    fn atomic(&mut self, op: AtomicOp, args: &[Operand], span: Span, st: &mut State) {
        if matches!(op, AtomicOp::Load) || args.len() < 2 {
            return;
        }
        let target = self.operand(&args[0], st).flat();
        let mut v = Set::new();
        for a in &args[1..] {
            set_add(&mut v, &self.operand(a, st).flat());
        }
        self.store_through(&target, None, &v, span, Sink::Store, st);
    }

    /// `place = v`.
    fn store(&mut self, place: &Place, v: SVal, span: Span, st: &mut State) {
        let steps = self.steps(place);
        if let Some((target, slot)) = self.through(place, &steps, st) {
            if !v.is_empty() {
                self.store_through(&target, slot, &v.flat(), span, Sink::Store, st);
            }
            return;
        }
        match place.root {
            PlaceRoot::Local(x) => {
                if place.proj.is_empty() {
                    st.vals.remove(&x.0);
                }
                if v.is_empty() {
                    return;
                }
                match (place.proj.is_empty(), steps.field_after(0)) {
                    (true, _) => st.add(x.0, &v),
                    (false, Some(f)) => {
                        let mut add = SVal::default();
                        add.add_field(f, &v.flat());
                        st.add(x.0, &add);
                    }
                    (false, None) => st.add(x.0, &SVal::of(v.flat())),
                }
            }
            // `main`'s frame lasts as long as the program's main thread; a `#[thread]` global is
            // per-thread scratch (the CSS token buffer), refilled before it is read again
            PlaceRoot::Global(g) if self.entry || self.cx.ir.global(g).thread => {}
            PlaceRoot::Global(_) => self.escape(&v.flat(), span, &Sink::Store, Some(span), st, &mut HashSet::new()),
        }
    }

    /// Stores `v` into field `slot` (or the whole) of what `target` points at.
    /// `sink`: how the error reads when `v` holds frame data and the memory outlives this frame.
    fn store_through(&mut self, target: &[Tok], slot: Option<u32>, v: &[Tok], span: Span, sink: Sink, st: &mut State) {
        if v.is_empty() {
            return;
        }
        let mut known = false;
        for &t in target {
            if let Some(x) = t.as_loc() {
                known = true;
                let mut add = SVal::default();
                match t.field().or(slot) {
                    Some(f) => add.add_field(f, v),
                    None => add.whole = v.to_vec(),
                }
                st.add(x as u32, &add);
            } else if t.is_param() {
                known = true;
                // a field address is its param's pointer with that field as the slot
                let dst = match (t.is_addr(), t.field()) {
                    (true, Some(f)) => (t.base(), f + 1),
                    _ => (t, slot.map_or(0, |f| f + 1)),
                };
                for &s in v.iter().filter(|s| s.is_param()) {
                    self.out.into.insert((s, dst.0, dst.1));
                }
                // a caller's memory outlives this frame
                let local: Set = v.iter().copied().filter(|s| s.frame().is_some() || s.as_loc().is_some()).collect();
                self.escape(&local, span, &sink, Some(span), st, &mut HashSet::new());
            } else if t.frame().is_some() {
                known = true;
            }
        }
        if !known || target.contains(&Tok::U) {
            self.escape(v, span, &sink, Some(span), st, &mut HashSet::new());
        }
    }

    /// `v` is kept past this frame (at `span`): its params escape, frame data is an error.
    /// `keep` is where it is kept, for callers' notes.
    fn escape(&mut self, v: &[Tok], span: Span, sink: &Sink, keep: Option<Span>, st: &State, seen: &mut HashSet<Tok>) {
        for &t in v {
            if t.is_param() {
                if self.out.esc.insert(t) {
                    if let Some(k) = keep {
                        self.out.site.insert(t, k);
                    }
                }
            } else if let Some(buf) = t.frame() {
                self.error(buf, span, sink);
            } else if t.as_loc().is_some() && seen.insert(t) {
                let held = held(st, t);
                self.escape(&held, span, sink, keep, st, seen);
            }
        }
    }

    fn ret(&mut self, v: &SVal, span: Span, st: &State) {
        let parts = std::iter::once((0, &v.whole)).chain(v.fields.iter().map(|(f, s)| (f + 1, s)));
        let mut locals = Vec::new();
        for (slot, set) in parts {
            for &t in set {
                if t.is_param() {
                    self.out.ret.insert((slot, t));
                } else if t == Tok::U {
                    self.out.ret_u = true;
                } else if let Some(buf) = t.frame() {
                    self.error(buf, span, &Sink::Return);
                } else {
                    locals.push(t);
                }
            }
        }
        // a pointer to a local: the caller cannot follow it, and what the local holds goes out with it
        if !locals.is_empty() {
            self.out.ret_u = true;
            let mut seen = HashSet::new();
            while let Some(t) = locals.pop() {
                if !seen.insert(t) {
                    continue;
                }
                for h in held(st, t) {
                    if let Some(buf) = h.frame() {
                        self.error(buf, span, &Sink::Return);
                    } else if h.is_param() {
                        self.out.ret.insert((0, h));
                    } else if h.as_loc().is_some() {
                        locals.push(h);
                    }
                }
            }
        }
    }

    /// Frame buffer `buf` reaches `sink` at `at`: one error per backtick string, at the string.
    fn error(&mut self, buf: u32, at: Span, sink: &Sink) {
        let span = self.body.local(crate::ir::ids::LocalId(buf)).span;
        // `main`'s frame lasts the run
        if self.report && (!self.entry || matches!(sink, Sink::Return)) && self.reported.insert(span) {
            self.errs.push(self.cx.error(span, at, sink));
        }
    }
}

/// What a pointer carrying `ptr` points at.
fn deref(ptr: &[Tok], st: &State) -> SVal {
    let mut out = SVal::default();
    for &t in ptr {
        match t.as_loc() {
            Some(y) => match t.field() {
                None => out.add(&st.get(y as u32)),
                Some(f) => set_add(&mut out.whole, &st.get(y as u32).select(f)),
            },
            None => set_add(&mut out.whole, &[t.loaded()]),
        }
    }
    out
}

/// What the local a pointer token `t` points into holds (narrowed to its field).
fn held(st: &State, t: Tok) -> Set {
    let Some(y) = t.as_loc() else { return Set::new() };
    let v = st.get(y as u32);
    match t.field() {
        Some(f) => v.select(f),
        None => v.flat(),
    }
}

fn op_span(o: &Operand, fallback: Span) -> Span {
    let s = match o {
        Operand::Const(c) => c.span,
        Operand::Copy(p) => p.span,
    };
    if s.is_dummy() {
        fallback
    } else {
        s
    }
}
