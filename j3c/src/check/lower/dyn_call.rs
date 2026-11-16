//! `dyn Trait` (`speculative/lang/dyn.j3`): a fat pointer `{ $data, $vt }` (`ir/dyn_ty.rs`).
//! `&value` where a `dyn Trait` is wanted checks the value's type against the trait (the same
//! structural match a bound uses) and points `$vt` at the one table global for that (type, trait),
//! made by whichever module converts first. Each entry is a thunk `(ptr, P..): R` that casts the
//! data pointer back to `&T` and calls the type's method, its `#[impl]` fn, or the trait's default
//! body; thunks are ordinary fns on the module list, so the lazy-method walker and the emitters see
//! everything a table names. Supertrait methods are in the table too, and so is each supertrait's
//! table for the same type (`dyn_up.rs`), for `dyn Sub` to `dyn Super`. A `dyn` trait's methods take
//! `this: &Self` and use `Self` nowhere else.

use super::*;
use crate::check::{default_fn_name, resolve_type, trait_short, with_supers, BoundReq};
use crate::emit::stable_ty;
use crate::ir::dyn_ty::{DynSlot, DynTable};
use crate::ir::entity::{ConstInit, Func, GlobalDef, Vis};
use crate::ir::place::Access;
use bounds::{check_bounds, BoundCx};

impl<'a> Lower<'a> {
    fn dyn_script_err(&self, span: Span) -> CheckError {
        CheckError::new(self.path, span, "`dyn` values are not supported in scripts")
            .help("scripts run on the VM, which has no function tables; move this code into a `.j3` module")
    }

    /// `rv` stored in a fresh temporary.
    pub(super) fn tmp_op(&mut self, span: Span, rv: Rvalue) -> Operand {
        Operand::Copy(self.tmp_place(span, rv))
    }

    pub(super) fn tmp_place(&mut self, span: Span, rv: Rvalue) -> Place {
        let n = self.fresh("dyn");
        let id = self.b.tmp(n, rv.ty, span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), rv);
        place
    }

    /// `expr` where `want` (a `dyn Trait`) is expected: a `dyn` of that trait as is, or `&value`
    /// made into `{ data, &table }`.
    pub(super) fn to_dyn(&mut self, expr: &Expr, want: Ty) -> Result<Rvalue, CheckError> {
        let span = expr.span;
        let op = self.operand(expr, None)?;
        if op.ty() == want {
            return Ok(Rvalue::use_op(span, op));
        }
        let key = self.cx.dyn_trait(want).unwrap_or_default().to_string();
        if let Some(rv) = self.dyn_upcast(span, &op, &key)? {
            return Ok(rv);
        }
        let inner = self.cx.ref_inner(op.ty()).filter(|t| self.cx.dyn_trait(*t).is_none());
        let Some(inner) = inner else {
            return Err(self.mismatch(span, "type mismatch", op.ty(), want)
                .help(format!("a `dyn {}` is made from a reference to a value: `&value`", trait_short(&key))));
        };
        if self.script_sid.is_some() {
            return Err(self.dyn_script_err(span));
        }
        let vt = self.vtable(span, inner, &key)?;
        let ptr = self.cx.common.ptr;
        let data = self.tmp_op(span, Rvalue::cast(span, ptr, op));
        let gty = self.cx.global(vt).ty;
        let gref = self.cx.ty_ref(gty);
        let gplace = Place::global(vt, gty, Access::let_binding(), span);
        let r = self.tmp_op(span, Rvalue::ref_place(span, gref, gplace));
        let table = self.tmp_op(span, Rvalue::cast(span, ptr, r));
        Ok(Rvalue::aggregate(span, want, vec![data, table]))
    }

    /// `d.m(args)` on a `dyn Trait` (or a `&` of one): load slot `m` of the table and call it with
    /// the data pointer first.
    pub(super) fn call_dyn(&mut self, span: Span, mut recv: Place, name: &Ident, args: &[Expr]) -> Result<Rvalue, CheckError> {
        while self.cx.ref_inner(recv.ty).is_some() {
            recv = self.b.deref(self.cx, recv, span);
        }
        if self.script_sid.is_some() {
            return Err(self.dyn_script_err(span));
        }
        let key = self.cx.dyn_trait(recv.ty).unwrap_or_default().to_string();
        let table = self.dyn_table(span, &key)?;
        let Some(k) = table.slots.iter().position(|s| s.name == name.name) else {
            let have: Vec<String> = table.slots.iter().map(|s| format!("`{}`", s.name)).collect();
            return Err(CheckError::new(self.path, name.span, format!("no method `{}` on `{}`", name.name, self.ty_str(recv.ty)))
                .note(format!("a `dyn {}` has the trait's methods (and its supertraits'): {}", trait_short(&key), have.join(", "))));
        };
        let (TyKind::Struct(fat), TyKind::Struct(tsid)) = (self.cx.ty_kind(recv.ty).clone(), self.cx.ty_kind(table.ty).clone()) else {
            return Err(CheckError::new(self.path, span, "internal: a `dyn` type is not a struct"));
        };
        let data = self.b.field(self.cx, recv.clone(), FieldId { struct_id: fat, index: 0 }, span);
        let vt = self.b.field(self.cx, recv, FieldId { struct_id: fat, index: 1 }, span);
        let tref = self.cx.ty_ref(table.ty);
        let tp = self.tmp_place(span, Rvalue::cast(span, tref, Operand::Copy(vt)));
        let tp = self.b.deref(self.cx, tp, span);
        let slot = self.b.field(self.cx, tp, FieldId { struct_id: tsid, index: k as u32 }, span);
        let (params, ret) = match self.cx.fn_sig(table.slots[k].fn_ty) {
            Some((p, r)) => (p.clone(), r),
            None => return Err(CheckError::new(self.path, span, "internal: a table slot is not a fn")),
        };
        if params.len() - 1 != args.len() {
            return Err(CheckError::new(self.path, span, format!("`{}` takes {} args, got {}", name.name, params.len() - 1, args.len())));
        }
        let mut ops = vec![Operand::Copy(data)];
        for (arg, want) in args.iter().zip(&params[1..]) {
            ops.push(self.operand(arg, Some(*want))?);
        }
        Ok(Rvalue::call(span, ret, Operand::Copy(slot), ops))
    }

    /// The layout of trait `key`'s table: its methods, then each supertrait's (a name is kept once).
    /// Made once per trait; a method a `dyn` cannot call is the error here, at the `dyn` use.
    pub(super) fn dyn_table(&mut self, span: Span, key: &str) -> Result<DynTable, CheckError> {
        if let Some(t) = self.cx.dyns.tables.get(key) {
            return Ok(t.clone());
        }
        let traits = self.gen.traits;
        let short = trait_short(key).to_string();
        let mut slots: Vec<DynSlot> = Vec::new();
        for k in with_supers(traits, key) {
            let def = &traits.defs[&k];
            for m in &def.item.methods {
                if slots.iter().any(|s| s.name == m.name.name) {
                    continue;
                }
                let at = self.line_of(m.span);
                let by_ref = matches!(&m.params[0].ty.kind, parse::TypeKind::Ref(i) if matches!(&i.kind, parse::TypeKind::Named(n) if n == "Self"));
                if !by_ref {
                    return Err(CheckError::new(self.path, span, format!("`dyn {short}` cannot call `{}`: it takes `this: Self` by value", m.name.name))
                        .note(format!("`{}` is declared at {at}", m.name.name))
                        .help(format!("a `dyn` trait's methods take `this: &Self`; or use a generic `<T: {short}>` instead of `dyn`")));
                }
                let self_name = ["Self".to_string()];
                let rest = m.params[1..].iter().map(|p| &p.ty).chain(m.ret.as_ref());
                if rest.into_iter().any(|t| generic::mentions(t, &self_name)) {
                    return Err(CheckError::new(self.path, span, format!("`dyn {short}` cannot call `{}`: it uses `Self` outside `this`", m.name.name))
                        .note(format!("`{}` is declared at {at}; behind `dyn` the concrete type is not known", m.name.name))
                        .help(format!("take or return a concrete type (or a `dyn`), or use a generic `<T: {short}>` instead of `dyn`")));
                }
                let mut ps = vec![self.cx.common.ptr];
                for p in &m.params[1..] {
                    ps.push(resolve_type(self.cx, &p.ty, &def.path, &def.types)?);
                }
                let ret = match &m.ret {
                    Some(t) => resolve_type(self.cx, t, &def.path, &def.types)?,
                    None => self.cx.common.void,
                };
                let fn_ty = self.cx.ty_fn(ps, ret);
                slots.push(DynSlot { name: m.name.name.clone(), fn_ty, from: k.clone() });
            }
        }
        let ups: Vec<String> = with_supers(traits, key).into_iter().skip(1).collect();
        let ty = self.cx.dyn_table_struct(key, &slots, &ups);
        let table = DynTable { ty, slots, ups };
        self.cx.dyns.tables.insert(key.to_string(), table.clone());
        Ok(table)
    }

    /// The table global of `ty` for trait `key`, one per program.
    pub(super) fn vtable(&mut self, span: Span, ty: Ty, key: &str) -> Result<GlobalId, CheckError> {
        if let Some(&g) = self.cx.dyns.vtables.get(&(ty, key.to_string())) {
            return Ok(g);
        }
        let at = self.gen.traits.defs.get(key).map_or(span, |d| d.item.name.span);
        let req = BoundReq { param: String::new(), arg: ty, bounds: vec![Ident { name: key.to_string(), span: at }] };
        let bc = BoundCx { traits: self.gen.traits, methods: &*self.methods, method_temps: &*self.method_temps, funcs: Some(self.funcs) };
        check_bounds(&mut *self.cx, &bc, self.path, span, &format!("`dyn {}`", trait_short(key)), &[req])?;
        let table = self.dyn_table(span, key)?;
        let stem = format!("{}.{key}", stable_ty(self.cx, ty));
        let mut fields = Vec::new();
        for slot in &table.slots {
            let target = self.dyn_target(span, ty, slot)?;
            let thunk = self.dyn_thunk(span, ty, &stem, slot, target);
            fields.push((slot.name.clone(), ConstInit::Scalar(Operand::fn_addr(span, slot.fn_ty, thunk))));
        }
        for up in &table.ups {
            let thunk = self.dyn_up_thunk(span, ty, &stem, up)?;
            let fty = self.cx.func(thunk).ty;
            fields.push((format!("$up.{up}"), ConstInit::Scalar(Operand::fn_addr(span, fty, thunk))));
        }
        let name = self.cx.intern_name(&format!("$vt.{stem}"));
        let g = self.cx.alloc_global(GlobalDef {
            id: GlobalId(0),
            doc: String::new(),
            module: self.module_names.to_vec(),
            name,
            ty: table.ty,
            span,
            init: Some(ConstInit::Fields(fields)),
            thread: false,
            readonly: false,
        });
        self.cx.dyns.vtables.insert((ty, key.to_string()), g);
        self.cx.note_vtable(g);
        Ok(g)
    }

    /// What slot `slot` of `ty`'s table calls: own method, `#[impl]` fn, else the default body
    /// (`check_bounds` already said one of them is there).
    fn dyn_target(&mut self, span: Span, ty: Ty, slot: &DynSlot) -> Result<FuncId, CheckError> {
        if let TyKind::Struct(sid) = *self.cx.ty_kind(ty) {
            if let Some(&id) = self.methods.get(&(sid.0, slot.name.clone())) {
                return Ok(id);
            }
        }
        if let Some(i) = self.gen.traits.impls.get(&(slot.name.clone(), ty)) {
            return Ok(i.id);
        }
        if let Some(id) = self.enum_derived(span, ty, &slot.name)? {
            return Ok(id);
        }
        let (module, tname) = slot.from.rsplit_once("::").unwrap_or(("", &slot.from));
        let fname = default_fn_name(tname, &slot.name);
        let temps = self.gen.temps;
        let Some(tmpl) = temps.get(&format!("{module}::{fname}")) else {
            return Err(CheckError::new(self.path, span, format!("`{}` has no `{}` to put in a `dyn {tname}` table", self.ty_str(ty), slot.name))
                .help("built-in operators cannot be called through `dyn`; write the method on the type"));
        };
        self.generic_instance(span, module, &fname, tmpl, vec![ty])
    }

    /// `$dyn.<T>.<trait>.<m>(this: ptr, a0..)`: `target(this as &T, a0..)`.
    fn dyn_thunk(&mut self, span: Span, ty: Ty, stem: &str, slot: &DynSlot, target: FuncId) -> FuncId {
        let void = self.cx.common.void;
        let (params, ret) = self.cx.fn_sig(slot.fn_ty).map(|(p, r)| (p.clone(), r)).unwrap_or((Vec::new(), void));
        let want_recv = self.cx.fn_sig(self.cx.func(target).ty).and_then(|(p, _)| p.first().copied());
        let ptr = self.cx.common.ptr;
        let rty = self.cx.ty_ref(ty);
        let mut b = BodyBuilder::new(void);
        let this = b.param(self.cx.intern_name("$this"), ptr, span);
        let r = b.tmp(self.cx.intern_name("$self"), rty, span);
        let rp = b.local_place(r, span);
        b.assign(span, rp.clone(), Rvalue::cast(span, rty, Operand::Copy(b.local_place(this, span))));
        // the type's method may take `this: T` by value; the trait's always takes `&Self`
        let first = if want_recv == Some(rty) { rp } else { b.deref(self.cx, rp, span) };
        let mut args = vec![Operand::Copy(first)];
        for (i, t) in params.iter().enumerate().skip(1) {
            let p = b.param(self.cx.intern_name(&format!("a{i}")), *t, span);
            args.push(Operand::Copy(b.local_place(p, span)));
        }
        let call = Rvalue::call(span, ret, Operand::fn_addr(span, self.cx.func(target).ty, target), args);
        if ret == void {
            b.expr(span, call);
            b.return_op(span, None);
        } else {
            let t = b.tmp(self.cx.intern_name("_r"), ret, span);
            let place = b.local_place(t, span);
            b.assign(span, place.clone(), call);
            b.return_op(span, Some(Operand::Copy(place)));
        }
        let body = b.finish();
        lazy::wake(self.cx, &body.block, self.pending_mono);
        let name = self.cx.intern_name(&format!("$dyn.{stem}.{}", slot.name));
        let id = self.cx.alloc_func(Func {
            id: FuncId(0),
            doc: String::new(),
            module: self.module_names.to_vec(),
            name,
            ty: slot.fn_ty,
            vis: Vis::Private,
            method_of: None,
            kind: FuncKind::J3 { body },
            span,
        });
        self.ir_funcs.push(id);
        id
    }
}
