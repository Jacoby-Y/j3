//! Calls in [`super::Flow`]: a known callee's summary applied to the arguments.

use std::collections::HashSet;

use crate::ir::operand::{ConstKind, Operand};
use crate::ir::rvalue::Rvalue;

use super::super::val::{set_add, SVal, Set, Tok};
use super::super::Sink;
use super::{deref, op_span, Flow, State};

impl Flow<'_> {
    pub(super) fn call(&mut self, r: &Rvalue, func: &Operand, args: &[Operand], st: &mut State) -> SVal {
        let vals: Vec<SVal> = args.iter().map(|a| self.operand(a, st)).collect();
        let callee = match func {
            Operand::Const(c) => match c.kind {
                ConstKind::FnAddr(id) if self.cx.known(id) => Some(id),
                _ => None,
            },
            Operand::Copy(_) => None,
        };
        let has_ptr = self.has_ptr(r.ty);
        // an unknown callee (fn value, closure, method table, C) keeps nothing; its result points anywhere
        let Some(id) = callee else {
            return if has_ptr { SVal::tok(Tok::U) } else { SVal::default() };
        };
        let cx = self.cx;
        let Some(sum) = cx.sums.get(&id) else {
            return SVal::default();
        };
        let by_value = cx.struct_params(id);
        let span_of = |t: Tok| args.get(t.id()).map_or(r.span, |a| op_span(a, r.span));
        for &(src, dst, slot) in &sum.into {
            let (v, target) = (arg(&vals, &by_value, src, st), arg(&vals, &by_value, dst, st));
            let keep = if cx.is_container(id) { Some(r.span) } else { None };
            self.store_through(&target, slot.checked_sub(1), &v, span_of(src), Sink::Call(id, keep), st);
        }
        for &t in &sum.esc {
            // a container method (`List.push`) is kept where it is called, not inside the container
            let keep = if cx.is_container(id) { Some(r.span) } else { sum.site.get(&t).copied() };
            let v = arg(&vals, &by_value, t, st);
            self.escape(&v, span_of(t), &Sink::Call(id, keep), keep.or(Some(span_of(t))), st, &mut HashSet::new());
        }
        if !has_ptr {
            return SVal::default();
        }
        let mut res = SVal::default();
        for &(slot, t) in &sum.ret {
            let v = arg(&vals, &by_value, t, st);
            match slot {
                0 => set_add(&mut res.whole, &v),
                f => res.add_field(f - 1, &v),
            }
        }
        if sum.ret_u {
            set_add(&mut res.whole, &[Tok::U]);
        }
        res
    }
}

/// A callee's param token as the caller sees it: the argument, a field address in what it points
/// at, or the value in a field (of a by-value struct, or of what a pointer points at).
fn arg(vals: &[SVal], by_value: &[bool], t: Tok, st: &State) -> Set {
    let Some(v) = vals.get(t.id()) else { return Set::new() };
    match t.field() {
        None => v.flat(),
        Some(f) if t.is_addr() => {
            let mut out = Set::new();
            set_add(&mut out, &v.flat().iter().map(|x| x.addr(f)).collect::<Vec<_>>());
            out
        }
        Some(f) if by_value.get(t.id()).copied().unwrap_or(false) => v.select(f),
        Some(f) => deref(&v.flat(), st).select(f),
    }
}
