//! Generic free fns, call side. `first(xs)` for `fn first<T>(xs: []T): T` infers `T` from the
//! arguments (or `first::<int>(xs)`), then reuses one instance per (fn, type args) across all
//! modules. The body is checked per instance with `T` substituted, as generic methods are.

use super::*;
use crate::emit::stable_ty;

/// A generic fn that keeps instantiating itself with bigger types stops here.
const MAX_DEPTH: u32 = 32;

pub(super) fn mod_origin_key(module: &str) -> String {
    format!("mod@{module}")
}

/// "while instantiating `first<int>` (called at src/main.j3:12)", for errors in an instance's body.
pub(super) fn site_note(cx: &IrCx, id: FuncId) -> Option<String> {
    let (span, _) = cx.inst_sites.get(&id)?;
    let (path, line) = cx.file(span.file)?.loc(span.start);
    Some(format!(
        "while instantiating `{}` (called at {path}:{line})",
        cx.name_str(cx.func(id).name)
    ))
}

/// What the lowering of one fn knows about generic free fns.
pub(super) struct GenCtx<'a> {
    temps: &'a HashMap<String, FnItem>,
    origins: &'a HashMap<String, OriginNs>,
    names: &'a HashMap<String, String>,
    mods: &'a HashMap<String, String>,
    insts: &'a mut HashMap<(String, Vec<Ty>), FuncId>,
    pub traits: &'a TraitEnv,
    /// How deep the fn being lowered sits in a chain of instantiations.
    pub depth: u32,
}

impl<'a> GenCtx<'a> {
    pub fn new(
        temps: &'a HashMap<String, FnItem>,
        origins: &'a HashMap<String, OriginNs>,
        names: &'a HashMap<String, String>,
        mods: &'a HashMap<String, String>,
        insts: &'a mut HashMap<(String, Vec<Ty>), FuncId>,
        traits: &'a TraitEnv,
    ) -> GenCtx<'a> {
        GenCtx { temps, origins, names, mods, insts, traits, depth: 0 }
    }
}

/// The template without its body: `alloc_fn` reads only the signature.
fn signature_of(f: &FnItem) -> FnItem {
    FnItem {
        span: f.span,
        doc: f.doc.clone(),
        vis_pub: f.vis_pub,
        name: f.name.clone(),
        type_params: Vec::new(),
        bounds: Vec::new(),
        is_impl: false,
        shader: None,
        params: f.params.clone(),
        ret: f.ret.clone(),
        body: parse::Block { span: f.body.span, stmts: Vec::new() },
    }
}

/// Does `t` write any of the names in `tps`?
fn mentions(t: &parse::Type, tps: &[String]) -> bool {
    use parse::TypeKind as K;
    match &t.kind {
        K::Named(n) => tps.contains(n),
        K::Generic { args, .. } => args.iter().any(|a| mentions(a, tps)),
        K::Ref(i) | K::Slice(i) => mentions(i, tps),
        K::Array { elem, .. } => mentions(elem, tps),
        K::Fn { params, ret } | K::Closure { params, ret } => {
            params.iter().any(|p| mentions(p, tps)) || ret.as_deref().is_some_and(|r| mentions(r, tps))
        }
    }
}

/// An argument lowered before the type arguments are known, or left for after.
enum Slot {
    Done(Operand),
    /// A bare number or a closure literal: typed by the parameter once `T` is known.
    Later,
}

impl<'a> Lower<'a> {
    /// The module key of a generic fn callable as `name(..)` here.
    pub(super) fn generic_named(&self, name: &str) -> Option<String> {
        self.gen.names.get(name).cloned()
    }

    /// The module key and fn when `alias.name(..)` names a generic fn of a `use`d module.
    pub(super) fn generic_in_alias(&self, alias: &str, name: &str) -> Option<String> {
        let key = self.gen.mods.get(alias)?;
        let f = self.gen.temps.get(&format!("{key}::{name}"))?;
        f.vis_pub.then(|| key.clone())
    }

    pub(super) fn call_generic(
        &mut self,
        span: Span,
        module: &str,
        name: &str,
        type_args: &[parse::Type],
        recv: Option<Place>,
        args: &[Expr],
    ) -> Result<Rvalue, CheckError> {
        let temps = self.gen.temps;
        let tmpl: &'a FnItem = &temps[&format!("{module}::{name}")];
        if self.script_sid.is_some() {
            return Err(CheckError::new(self.path, span, format!("generic fn `{name}` cannot be called from a .j3s script"))
                .help("scripts run on the VM, which has no generic instances; call it from a `.j3` module"));
        }
        let given = args.len() + recv.is_some() as usize;
        if given != tmpl.params.len() {
            return Err(CheckError::new(self.path, span, format!("takes {} args, got {given}", tmpl.params.len())));
        }
        let tps: Vec<String> = tmpl.type_params.iter().map(|p| p.name.clone()).collect();
        let mut bind: HashMap<String, Ty> = HashMap::new();
        if !type_args.is_empty() {
            if type_args.len() != tps.len() {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("`{name}` takes {} type args, got {}", tps.len(), type_args.len()),
                ));
            }
            for (p, t) in tps.iter().zip(self.resolve_type_args(type_args)?) {
                bind.insert(p.clone(), t);
            }
        }
        let skip = recv.is_some() as usize;
        if let Some(r) = &recv {
            // A receiver is adjusted (`&`, deref) to the first parameter, so match through that.
            let p = &tmpl.params[0].ty;
            let (p, a) = match (&p.kind, self.cx.ref_inner(r.ty)) {
                (parse::TypeKind::Ref(inner), None) => (&**inner, r.ty),
                (parse::TypeKind::Ref(_), Some(_)) => (p, r.ty),
                (_, Some(inner)) => (p, inner),
                (_, None) => (p, r.ty),
            };
            self.bind_arg(span, p, a, &tps, &mut bind)?;
        }
        let mut slots = Vec::new();
        for (arg, param) in args.iter().zip(&tmpl.params[skip..]) {
            if bare_num(arg) || matches!(arg.kind, ExprKind::Closure(_)) {
                slots.push(Slot::Later);
                continue;
            }
            // No `T` in the parameter, or `T` already fixed by an earlier arg: it is a plain destination type.
            let want = match &param.ty.kind {
                parse::TypeKind::Named(n) if tps.contains(n) => bind.get(n).copied(),
                _ if !mentions(&param.ty, &tps) => self.resolve_ty(&param.ty).ok(),
                _ => None,
            };
            let op = self.operand(arg, want)?;
            self.bind_arg(arg.span, &param.ty, op.ty(), &tps, &mut bind)?;
            slots.push(Slot::Done(op));
        }
        // A closure literal's written types (`fn(v: int): f32 {..}`) also bind what its parameter names.
        for (arg, param) in args.iter().zip(&tmpl.params[skip..]) {
            if let (ExprKind::Closure(c), parse::TypeKind::Closure { params: pp, ret: pr }) = (&arg.kind, &param.ty.kind) {
                for (cp, want) in c.params.iter().zip(pp) {
                    if let Some(t) = &cp.ty {
                        let ty = self.resolve_ty(t)?;
                        self.bind_arg(t.span, want, ty, &tps, &mut bind)?;
                    }
                }
                if let (Some(t), Some(want)) = (&c.ret, pr) {
                    let ty = self.resolve_ty(t)?;
                    self.bind_arg(t.span, want, ty, &tps, &mut bind)?;
                }
            }
        }
        // `maxOf(1, 2)`: with nothing else to go on, a bare number takes its default type.
        for (i, (arg, param)) in args.iter().zip(&tmpl.params[skip..]).enumerate() {
            let parse::TypeKind::Named(n) = &param.ty.kind else { continue };
            if matches!(slots[i], Slot::Later) && bare_num(arg) && tps.contains(n) && !bind.contains_key(n) {
                let op = self.operand(arg, None)?;
                bind.insert(n.clone(), op.ty());
                slots[i] = Slot::Done(op);
            }
        }
        let mut targs = Vec::new();
        for (tp, ident) in tps.iter().zip(&tmpl.type_params) {
            match bind.get(tp) {
                Some(&t) => targs.push(t),
                None => {
                    return Err(CheckError::new(self.path, span, format!("cannot infer type parameter `{tp}` of `{name}`"))
                        .note(format!("`{tp}` is declared at {}", self.line_of(ident.span)))
                        .help(format!("pass a value whose type is (or contains) `{tp}`, or write it: `{name}::<int>(..)`")));
                }
            }
        }
        self.check_call_bounds(span, &format!("`{name}`"), &tmpl.type_params, &tmpl.bounds, &targs)?;
        let id = self.generic_instance(span, module, name, tmpl, targs)?;
        let fty = self.cx.func(id).ty;
        let (params, ret) = match self.cx.fn_sig(fty) {
            Some((p, r)) => (p.clone(), r),
            None => return Err(CheckError::new(self.path, span, "not a function")),
        };
        let mut ops = Vec::new();
        if let Some(r) = recv {
            ops.push(self.auto_this(span, r, params[0])?);
        }
        for (i, (arg, slot)) in args.iter().zip(slots).enumerate() {
            let want = params[skip + i];
            ops.push(match slot {
                Slot::Done(op) => self.adapt_arg(arg.span, op, want)?,
                Slot::Later => {
                    let op = self.operand(arg, Some(want))?;
                    self.adapt_arg(arg.span, op, want)?
                }
            });
        }
        Ok(Rvalue::call(span, ret, Operand::fn_addr(span, fty, id), ops))
    }

    fn bind_arg(
        &self,
        span: Span,
        p: &parse::Type,
        a: Ty,
        tps: &[String],
        bind: &mut HashMap<String, Ty>,
    ) -> Result<(), CheckError> {
        self.unify(p, a, tps, bind).map_err(|c| {
            CheckError::new(
                self.path,
                span,
                format!(
                    "`{}` is both `{}` and `{}`",
                    c.name,
                    self.ty_str(c.first),
                    self.ty_str(c.second)
                ),
            )
            .help("every use of one type parameter must be the same type; convert one argument")
        })
    }

    /// An argument lowered without a destination type, checked against the instance's parameter.
    fn adapt_arg(&mut self, span: Span, op: Operand, want: Ty) -> Result<Operand, CheckError> {
        if op.ty() == want {
            return Ok(op);
        }
        if let Operand::Copy(place) = &op {
            if let Some(o) = self.array_to_slice(span, place, want)? {
                return Ok(o);
            }
            if let Some(o) = self.text_to_string(span, place, want)? {
                return Ok(o);
            }
        }
        Err(self.mismatch(span, "type mismatch", op.ty(), want))
    }

    fn line_of(&self, span: Span) -> String {
        match self.cx.file(span.file) {
            Some(f) => {
                let (p, l) = f.loc(span.start);
                format!("{p}:{l}")
            }
            None => String::new(),
        }
    }

    /// The instance of `name` for `targs`: made once, queued for lowering, shared by all modules.
    fn generic_instance(
        &mut self,
        span: Span,
        module: &str,
        name: &str,
        tmpl: &FnItem,
        targs: Vec<Ty>,
    ) -> Result<FuncId, CheckError> {
        let key = (format!("{module}::{name}"), targs);
        if let Some(&id) = self.gen.insts.get(&key) {
            return Ok(id);
        }
        let depth = self.gen.depth + 1;
        if depth > MAX_DEPTH {
            return Err(CheckError::new(self.path, span, format!("`{name}` instantiates itself with ever-deeper types"))
                .note(format!("{MAX_DEPTH} nested instances so far, e.g. `{name}<List<T>>` called from `{name}<T>`"))
                .help("make the recursive call use the same type arguments"));
        }
        let origins = self.gen.origins;
        let o = &origins[&mod_origin_key(module)];
        let mut subst = HashMap::new();
        for (p, t) in tmpl.type_params.iter().zip(&key.1) {
            subst.insert(p.name.clone(), *t);
        }
        // Distinct name per type args: it keys the symbol, so instances never collide.
        let tys: Vec<String> = key.1.iter().map(|t| stable_ty(self.cx, *t)).collect();
        let mut sig = signature_of(tmpl);
        sig.name.name = format!("{name}<{}>", tys.join(","));
        sig.vis_pub = true;
        let mut types = o.types.clone();
        for (k, v) in self.types {
            types.entry(k.clone()).or_insert(*v);
        }
        let mut enum_temps = o.enum_temps.clone();
        for (k, v) in self.enum_temps.iter() {
            enum_temps.entry(k.clone()).or_insert_with(|| v.clone());
        }
        let id = {
            let mut env = GenEnv {
                types: &types,
                subst: &subst,
                enum_temps: &enum_temps,
                enum_insts: self.enum_insts,
                struct_temps: self.struct_temps,
                struct_temp_env: self.struct_temp_env,
                struct_insts: self.struct_insts,
                methods: self.methods,
                method_temps: self.method_temps,
                method_insts: self.method_insts,
                fn_substs: self.fn_substs,
                pending_mono: self.pending_mono,
                module: self.module_names,
                ir_enums: self.ir_enums,
                ir_structs: self.ir_structs,
                ir_funcs: self.ir_funcs,
            };
            alloc_fn(self.cx, &o.path, &o.names, &sig, None, &HashMap::new(), &mut env)?
        };
        self.gen.insts.insert(key, id);
        self.ir_funcs.push(id);
        self.cx.inst_sites.insert(id, (span, depth));
        self.pending_mono.push((id, tmpl.clone(), subst));
        Ok(id)
    }
}
