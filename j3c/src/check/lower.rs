use std::collections::{HashMap, HashSet};

use super::error::CheckError;
use super::{
    alloc_fn, field_path, find_operator, instantiate_struct, is_float, is_int, is_numeric,
    resolve_type_g, ConstPayload, ConstVal, GenEnv, ModData,
};
use crate::intern::Name;
use crate::ir::build::BodyBuilder;
use crate::ir::cx::IrCx;
use crate::ir::entity::FuncKind;
use crate::ir::ids::{EnumId, FuncId, GlobalId, LocalId, StructId, VariantId};
use crate::ir::op::{BinOp, UnaryOp};
use crate::ir::operand::Operand;
use crate::ir::place::Place;
use crate::ir::rvalue::{Rvalue, RvalueKind};
use crate::ir::dump::dump_ty;
use crate::ir::ty::{Ty, TyKind};
use crate::parse::{self, Expr, ExprKind, FnItem, Ident, InterpPart, Item, MatchArm, Pat, Stmt};
use crate::span::Span;

struct OriginNs {
    path: String,
    funcs: HashMap<String, FuncId>,
    types: HashMap<String, Ty>,
    consts: HashMap<String, ConstVal>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
}

fn generic_origins(mods: &[ModData]) -> HashMap<String, OriginNs> {
    let mut out = HashMap::new();
    for m in mods {
        for it in &m.ast.items {
            let Item::Struct(s) = it else { continue };
            if s.type_params.is_empty() {
                continue;
            }
            let mut funcs = m.funcs.clone();
            for (name, id) in &m.imports {
                funcs.insert(name.clone(), *id);
            }
            out.insert(
                s.name.name.clone(),
                OriginNs {
                    path: m.path.clone(),
                    funcs,
                    types: m.types.clone(),
                    consts: m.consts.clone(),
                    lib_ns: m.lib_ns.clone(),
                    mod_ns: m.mod_ns.clone(),
                    mod_types: m.mod_types.clone(),
                    globals: m.globals.clone(),
                    struct_temps: m.struct_temps.clone(),
                    struct_temp_env: m.struct_temp_env.clone(),
                },
            );
        }
    }
    out
}

fn merge_map<V: Clone>(base: &HashMap<String, V>, extra: &HashMap<String, V>) -> HashMap<String, V> {
    let mut out = base.clone();
    for (k, v) in extra {
        out.entry(k.clone()).or_insert_with(|| v.clone());
    }
    out
}

pub(crate) fn lower_module(
    cx: &mut IrCx,
    mods: &mut [ModData],
    i: usize,
    main_id: FuncId,
) -> Result<(), CheckError> {
    let origins = generic_origins(mods);
    let path = mods[i].path.clone();
    let mut funcs = mods[i].funcs.clone();
    for (name, id) in &mods[i].imports {
        funcs.insert(name.clone(), *id);
    }
    if !funcs.contains_key("main") {
        funcs.insert("main".to_string(), main_id);
    }
    let consts = mods[i].consts.clone();
    let types = mods[i].types.clone();
    let mut methods = mods[i].methods.clone();
    let lib_ns = mods[i].lib_ns.clone();
    let mod_ns = mods[i].mod_ns.clone();
    let mod_types = mods[i].mod_types.clone();
    let globals = mods[i].globals.clone();
    let enum_temps = mods[i].enum_temps.clone();
    let mut enum_insts = mods[i].insts.clone();
    let mut struct_temps = HashMap::new();
    let mut struct_temp_env = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.struct_temps {
            struct_temps.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &m.struct_temp_env {
            struct_temp_env.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }
    let mut struct_insts = HashMap::new();
    for m in mods.iter() {
        for (k, v) in &m.struct_insts {
            struct_insts.entry(k.clone()).or_insert(*v);
        }
    }
    let mut method_temps = mods[i].method_temps.clone();
    let mut method_insts = mods[i].method_insts.clone();
    let mut fn_substs = mods[i].fn_substs.clone();
    let mut pending_mono = mods[i].pending_mono.clone();
    let module_names = mods[i].names.clone();
    let items = mods[i].ast.items.clone();

    let mut pending: Vec<(FuncId, FnItem)> = Vec::new();
    for it in &items {
        match it {
            Item::Fn(f) => pending.push((mods[i].funcs[&f.name.name], f.clone())),
            Item::Struct(s) => {
                if !s.type_params.is_empty() {
                    continue;
                }
                let sid = mods[i].structs[&s.name.name];
                for m in &s.methods {
                    if !m.type_params.is_empty() {
                        continue;
                    }
                    pending.push((mods[i].methods[&(sid.0, m.name.name.clone())], m.clone()));
                }
            }
            _ => {}
        }
    }

    let mut queue: Vec<(FuncId, FnItem, HashMap<String, Ty>)> = pending
        .into_iter()
        .map(|(id, f)| (id, f, HashMap::new()))
        .collect();
    queue.append(&mut pending_mono);
    let mut n = 0;
    while n < queue.len() {
        let (id, f, subst) = queue[n].clone();
        n += 1;
        if matches!(cx.func(id).kind, FuncKind::J3 { .. }) {
            continue;
        }
        let body = lower_fn(
            cx,
            &path,
            id,
            &f,
            &origins,
            &funcs,
            &consts,
            &types,
            &mut methods,
            &lib_ns,
            &mod_ns,
            &mod_types,
            &globals,
            &enum_temps,
            &mut enum_insts,
            &struct_temps,
            &struct_temp_env,
            &mut struct_insts,
            &mut method_temps,
            &mut method_insts,
            &mut fn_substs,
            &mut queue,
            &module_names,
            &mut mods[i].ir.enums,
            &mut mods[i].ir.structs,
            &mut mods[i].ir.funcs,
            subst,
        )?;
        cx.func_mut(id).kind = FuncKind::J3 { body };
    }
    mods[i].insts = enum_insts;
    mods[i].methods = methods;
    mods[i].struct_insts = struct_insts;
    mods[i].method_temps = method_temps;
    mods[i].method_insts = method_insts;
    mods[i].fn_substs = fn_substs;
    Ok(())
}

struct Lower<'a> {
    cx: &'a mut IrCx,
    path: &'a str,
    b: BodyBuilder,
    locals: HashMap<String, LocalId>,
    aliases: HashMap<String, Place>,
    funcs: &'a HashMap<String, FuncId>,
    consts: &'a HashMap<String, ConstVal>,
    types: &'a HashMap<String, Ty>,
    methods: &'a mut HashMap<(u32, String), FuncId>,
    lib_ns: &'a HashMap<String, HashMap<String, FuncId>>,
    mod_ns: &'a HashMap<String, HashMap<String, FuncId>>,
    mod_types: &'a HashMap<String, HashMap<String, Ty>>,
    globals: &'a HashMap<String, crate::ir::ids::GlobalId>,
    enum_temps: &'a HashMap<String, parse::EnumItem>,
    enum_insts: &'a mut HashMap<(String, Vec<Ty>), EnumId>,
    struct_temps: &'a HashMap<String, parse::StructItem>,
    struct_temp_env: &'a HashMap<String, HashMap<String, Ty>>,
    struct_insts: &'a mut HashMap<(String, Vec<Ty>), StructId>,
    method_temps: &'a mut HashMap<(u32, String), parse::FnItem>,
    method_insts: &'a mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: &'a mut HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: &'a mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    module_names: &'a [Name],
    ir_enums: &'a mut Vec<EnumId>,
    ir_structs: &'a mut Vec<StructId>,
    ir_funcs: &'a mut Vec<FuncId>,
    type_subst: HashMap<String, Ty>,
    ret: Ty,
    tmp: u32,
}

struct MergedEnv {
    funcs: HashMap<String, FuncId>,
    consts: HashMap<String, ConstVal>,
    types: HashMap<String, Ty>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, crate::ir::ids::GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
}

fn lower_fn(
    cx: &mut IrCx,
    path: &str,
    id: FuncId,
    f: &FnItem,
    origins: &HashMap<String, OriginNs>,
    funcs: &HashMap<String, FuncId>,
    consts: &HashMap<String, ConstVal>,
    types: &HashMap<String, Ty>,
    methods: &mut HashMap<(u32, String), FuncId>,
    lib_ns: &HashMap<String, HashMap<String, FuncId>>,
    mod_ns: &HashMap<String, HashMap<String, FuncId>>,
    mod_types: &HashMap<String, HashMap<String, Ty>>,
    globals: &HashMap<String, crate::ir::ids::GlobalId>,
    enum_temps: &HashMap<String, parse::EnumItem>,
    enum_insts: &mut HashMap<(String, Vec<Ty>), EnumId>,
    struct_temps: &HashMap<String, parse::StructItem>,
    struct_temp_env: &HashMap<String, HashMap<String, Ty>>,
    struct_insts: &mut HashMap<(String, Vec<Ty>), StructId>,
    method_temps: &mut HashMap<(u32, String), parse::FnItem>,
    method_insts: &mut HashMap<(u32, String, Vec<Ty>), FuncId>,
    fn_substs: &mut HashMap<FuncId, HashMap<String, Ty>>,
    pending_mono: &mut Vec<(FuncId, parse::FnItem, HashMap<String, Ty>)>,
    module_names: &[Name],
    ir_enums: &mut Vec<EnumId>,
    ir_structs: &mut Vec<StructId>,
    ir_funcs: &mut Vec<FuncId>,
    type_subst: HashMap<String, Ty>,
) -> Result<crate::ir::body::Body, CheckError> {
    let mut body_path = path.to_string();
    let mut inst_note: Option<String> = None;
    let merged: Option<MergedEnv> = if let Some(sid) = cx.func(id).method_of {
        let sname = cx.name_str(cx.struct_def(sid).name).to_string();
        if let Some(o) = origins.get(&sname) {
            if o.path != path {
                inst_note = Some(format!("while instantiating `{sname}` (from {path})"));
                body_path = o.path.clone();
            }
            Some(MergedEnv {
                funcs: merge_map(&o.funcs, funcs),
                consts: merge_map(&o.consts, consts),
                types: merge_map(&o.types, types),
                lib_ns: merge_map(&o.lib_ns, lib_ns),
                mod_ns: merge_map(&o.mod_ns, mod_ns),
                mod_types: merge_map(&o.mod_types, mod_types),
                globals: merge_map(&o.globals, globals),
                struct_temps: merge_map(&o.struct_temps, struct_temps),
                struct_temp_env: merge_map(&o.struct_temp_env, struct_temp_env),
            })
        } else {
            None
        }
    } else {
        None
    };
    let use_funcs = merged.as_ref().map(|m| &m.funcs).unwrap_or(funcs);
    let use_consts = merged.as_ref().map(|m| &m.consts).unwrap_or(consts);
    let use_types = merged.as_ref().map(|m| &m.types).unwrap_or(types);
    let use_lib_ns = merged.as_ref().map(|m| &m.lib_ns).unwrap_or(lib_ns);
    let use_mod_ns = merged.as_ref().map(|m| &m.mod_ns).unwrap_or(mod_ns);
    let use_mod_types = merged.as_ref().map(|m| &m.mod_types).unwrap_or(mod_types);
    let use_globals = merged.as_ref().map(|m| &m.globals).unwrap_or(globals);
    let use_struct_temps = merged
        .as_ref()
        .map(|m| &m.struct_temps)
        .unwrap_or(struct_temps);
    let use_struct_temp_env = merged
        .as_ref()
        .map(|m| &m.struct_temp_env)
        .unwrap_or(struct_temp_env);
    let (param_tys, ret) = match cx.fn_sig(cx.func(id).ty) {
        Some((ps, r)) => (ps.clone(), r),
        None => (Vec::new(), cx.common.void),
    };
    let mut b = BodyBuilder::new(cx.common.void);
    let mut locals = HashMap::new();
    for (p, ty) in f.params.iter().zip(param_tys.into_iter()) {
        let n = cx.intern_name(&p.name.name);
        let pid = b.param(n, ty, p.name.span);
        if locals.contains_key(&p.name.name) {
            return Err(CheckError::new(
                path,
                p.name.span,
                format!("duplicate parameter `{}`", p.name.name),
            ));
        }
        locals.insert(p.name.name.clone(), pid);
    }
    let mut l = Lower {
        cx,
        path: &body_path,
        b,
        locals,
        aliases: HashMap::new(),
        funcs: use_funcs,
        consts: use_consts,
        types: use_types,
        methods,
        lib_ns: use_lib_ns,
        mod_ns: use_mod_ns,
        mod_types: use_mod_types,
        globals: use_globals,
        enum_temps,
        enum_insts,
        struct_temps: use_struct_temps,
        struct_temp_env: use_struct_temp_env,
        struct_insts,
        method_temps,
        method_insts,
        fn_substs,
        pending_mono,
        module_names,
        ir_enums,
        ir_structs,
        ir_funcs,
        type_subst,
        ret,
        tmp: 0,
    };
    for stmt in &f.body.stmts {
        l.stmt(stmt).map_err(|e| match &inst_note {
            Some(n) => e.note(n.clone()),
            None => e,
        })?;
    }
    Ok(l.b.finish())
}

impl<'a> Lower<'a> {
    fn ty_str(&self, ty: Ty) -> String {
        let mut s = String::new();
        dump_ty(self.cx, ty, &mut s);
        s
    }

    fn mismatch(&self, span: Span, what: &str, have: Ty, want: Ty) -> CheckError {
        CheckError::new(
            self.path,
            span,
            format!(
                "{what}: expected `{}`, found `{}`",
                self.ty_str(want),
                self.ty_str(have)
            ),
        )
    }

    fn unknown(&self, span: Span, kind: &str, name: &str, extra: &[String]) -> CheckError {
        let mut cands: Vec<String> = self.locals.keys().cloned().collect();
        cands.extend(self.aliases.keys().cloned());
        cands.extend(self.funcs.keys().cloned());
        cands.extend(self.types.keys().cloned());
        cands.extend(self.consts.keys().cloned());
        cands.extend(self.globals.keys().cloned());
        cands.extend(extra.iter().cloned());
        let mut e = CheckError::new(self.path, span, format!("unknown {kind} `{name}`"));
        if let Some(s) = super::error::suggest(name, &cands) {
            e = e.help(format!("did you mean `{s}`?"));
        }
        e
    }

    fn no_method(&self, span: Span, sid: StructId, name: &str) -> CheckError {
        let mut names: Vec<String> = self
            .methods
            .keys()
            .filter(|(id, _)| *id == sid.0)
            .map(|(_, n)| n.clone())
            .collect();
        names.extend(
            self.method_temps
                .keys()
                .filter(|(id, _)| *id == sid.0)
                .map(|(_, n)| n.clone()),
        );
        let mut e = CheckError::new(self.path, span, format!("no method `{name}`"));
        if let Some(s) = super::error::suggest(name, &names) {
            e = e.help(format!("did you mean `{s}`?"));
        }
        e
    }

    fn fresh(&mut self, hint: &str) -> Name {
        let n = self.tmp;
        self.tmp += 1;
        self.cx.intern_name(&format!("_{hint}{n}"))
    }

    fn stmt(&mut self, stmt: &Stmt) -> Result<(), CheckError> {
        match stmt {
            Stmt::Let(s) => {
                if self.locals.contains_key(&s.name.name) {
                    return Err(CheckError::new(
                        self.path,
                        s.name.span,
                        format!("shadowing `{}` is a compile error", s.name.name),
                    ));
                }
                let want = match &s.ty {
                    Some(t) => Some(self.resolve_ty(t)?),
                    None => None,
                };
                let rv = self.rvalue(&s.value, want)?;
                if let Some(want) = want {
                    if rv.ty != want {
                        return Err(self.mismatch(
                            s.span,
                            "let type does not match initializer",
                            rv.ty,
                            want,
                        ));
                    }
                }
                let n = self.cx.intern_name(&s.name.name);
                let id = self.b.let_local(n, rv.ty, s.name.span);
                self.b.set_doc(id, s.doc.clone());
                let place = self.b.local_place(id, s.name.span);
                self.b.assign(s.span, place, rv);
                self.locals.insert(s.name.name.clone(), id);
                Ok(())
            }
            Stmt::Return(s) => match &s.value {
                None => {
                    if self.ret != self.cx.common.void {
                        return Err(CheckError::new(self.path, s.span, "return needs a value"));
                    }
                    self.b.return_op(s.span, None);
                    Ok(())
                }
                Some(e) => {
                    if self.ret == self.cx.common.void {
                        return Err(CheckError::new(
                            self.path,
                            s.span,
                            "return value in void function",
                        ));
                    }
                    let op = self.operand(e, Some(self.ret))?;
                    if op.ty() != self.ret {
                        return Err(self.mismatch(s.span, "return type mismatch", op.ty(), self.ret));
                    }
                    self.b.return_op(s.span, Some(op));
                    Ok(())
                }
            },
            Stmt::Assign(s) => {
                let place = self.place(&s.place)?;
                if !place.access.assign {
                    return Err(CheckError::new(
                        self.path,
                        s.span,
                        "cannot assign to this place",
                    ));
                }
                let rv = self.rvalue(&s.value, Some(place.ty))?;
                if rv.ty != place.ty {
                    return Err(self.mismatch(s.span, "assign type mismatch", rv.ty, place.ty));
                }
                self.b.assign(s.span, place, rv);
                Ok(())
            }
            Stmt::AssignOp(s) => {
                let place = self.place(&s.place)?;
                if !place.access.assign {
                    return Err(CheckError::new(
                        self.path,
                        s.span,
                        "cannot assign-op this place",
                    ));
                }
                if let Some(id) = find_operator(self.cx, place.ty, s.op, self.methods) {
                    let fty = self.cx.func(id).ty;
                    let (params, ret) = match self.cx.fn_sig(fty) {
                        Some(sig) => (sig.0.clone(), sig.1),
                        None => {
                            return Err(CheckError::new(self.path, s.span, "not a function"));
                        }
                    };
                    if params.len() != 2 {
                        return Err(CheckError::new(
                            self.path,
                            s.span,
                            "binary operator needs two parameters",
                        ));
                    }
                    let rhs = self.operand(&s.value, Some(params[1]))?;
                    let lhs = Operand::Copy(place.clone());
                    let call = Rvalue::call(
                        s.span,
                        ret,
                        Operand::fn_addr(s.span, fty, id),
                        vec![lhs, rhs],
                    );
                    if call.ty != place.ty {
                        return Err(self.mismatch(
                            s.span,
                            "assign-op type mismatch",
                            call.ty,
                            place.ty,
                        ));
                    }
                    self.b.assign(s.span, place, call);
                    return Ok(());
                }
                let rhs = self.operand(&s.value, Some(place.ty))?;
                let ir_op = ast_binop(s.op);
                self.b.assign_op(s.span, place, ir_op, rhs);
                Ok(())
            }
            Stmt::Expr(e) => {
                let rv = self.rvalue(e, None)?;
                self.b.expr(e.span, rv);
                Ok(())
            }
            Stmt::If(s) => {
                let cond = self.operand(&s.cond, Some(self.cx.common.bool))?;
                let then_block = self.lower_block(&s.then_block)?;
                let else_block = match &s.else_block {
                    Some(b) => Some(self.lower_block(b)?),
                    None => None,
                };
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::If {
                        cond,
                        then_block,
                        else_block,
                    },
                });
                Ok(())
            }
            Stmt::While(s) => {
                let saved = self.b.begin_block();
                let cond = self.operand(&s.cond, Some(self.cx.common.bool))?;
                let not_n = self.fresh("not");
                let not_id = self.b.tmp(not_n, self.cx.common.bool, s.cond.span);
                let not_place = self.b.local_place(not_id, s.cond.span);
                self.b.assign(
                    s.cond.span,
                    not_place.clone(),
                    Rvalue {
                        ty: self.cx.common.bool,
                        span: s.cond.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Not,
                            arg: cond,
                        },
                    },
                );
                let br = self.b.begin_block();
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::Break,
                });
                let br_block = self.b.end_block(br);
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::If {
                        cond: Operand::Copy(not_place),
                        then_block: br_block,
                        else_block: None,
                    },
                });
                for stmt in &s.body.stmts {
                    self.stmt(stmt)?;
                }
                let body = self.b.end_block(saved);
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::While {
                        cond: Operand::bool_const(s.span, self.cx.common.bool, true),
                        body,
                    },
                });
                Ok(())
            }
            Stmt::UseLocal(s) => self.use_local(s),
            Stmt::ForEach(s) => self.for_each(s),
            Stmt::For(s) => {
                if self.locals.contains_key(&s.name.name) {
                    return Err(CheckError::new(
                        self.path,
                        s.name.span,
                        format!("shadowing `{}` is a compile error", s.name.name),
                    ));
                }
                let start = self.operand(&s.start, Some(self.cx.common.int))?;
                let mut end = self.operand(&s.end, Some(self.cx.common.int))?;
                if s.inclusive {
                    let one = Operand::int(s.end.span, self.cx.common.int, 1);
                    let n = self.fresh("inc");
                    let tid = self.b.tmp(n, self.cx.common.int, s.end.span);
                    let p = self.b.local_place(tid, s.end.span);
                    self.b.assign(
                        s.span,
                        p.clone(),
                        Rvalue::binary(s.end.span, self.cx.common.int, BinOp::Add, end, one),
                    );
                    end = Operand::Copy(p);
                }
                let n = self.cx.intern_name(&s.name.name);
                let id = self.b.let_local(n, self.cx.common.int, s.name.span);
                let place = self.b.local_place(id, s.name.span);
                self.b
                    .assign(s.span, place.clone(), Rvalue::use_op(s.start.span, start));
                self.locals.insert(s.name.name.clone(), id);
                let body = self.lower_block(&s.body)?;
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::ForRange {
                        index: place,
                        end,
                        body,
                    },
                });
                self.locals.remove(&s.name.name);
                Ok(())
            }
            Stmt::Defer(s) => {
                let body = self.lower_block(&s.body)?;
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: s.span,
                    kind: crate::ir::stmt::StmtKind::Defer { body },
                });
                Ok(())
            }
            Stmt::Break(span) => {
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: *span,
                    kind: crate::ir::stmt::StmtKind::Break,
                });
                Ok(())
            }
            Stmt::Continue(span) => {
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: *span,
                    kind: crate::ir::stmt::StmtKind::Continue,
                });
                Ok(())
            }
            Stmt::Match(s) => self.lower_match(s),
            Stmt::Block(b) => {
                let body = self.lower_block(b)?;
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: b.span,
                    kind: crate::ir::stmt::StmtKind::Block(body),
                });
                Ok(())
            }
        }
    }

    fn operand(&mut self, expr: &Expr, expected: Option<Ty>) -> Result<Operand, CheckError> {
        if let Some(v) = self.fold_int(expr) {
            if v == 0 {
                if let Some(want) = expected {
                    if self.is_ptr_ty(want) {
                        return Ok(Operand::null(expr.span, want));
                    }
                }
            }
            return Ok(self.num_from_int(expr.span, v, expected));
        }
        if let ExprKind::Float(v) = expr.kind {
            return Ok(self.num_from_float(expr.span, v, expected));
        }
        if let Ok(place) = self.try_place(expr) {
            if let Some(want) = expected {
                if place.ty == want {
                    return Ok(Operand::Copy(place));
                }
                if let Some(op) = self.array_to_slice(expr.span, &place, want)? {
                    return Ok(op);
                }
                if let Some(op) = self.text_to_string(expr.span, &place, want)? {
                    return Ok(op);
                }
                return Err(self.mismatch(
                    expr.span,
                    "type mismatch",
                    place.ty,
                    want,
                ));
            }
            return Ok(Operand::Copy(place));
        }
        let rv = self.rvalue(expr, expected)?;
        if let Some(want) = expected {
            if rv.ty != want {
                return Err(self.mismatch(expr.span, "type mismatch", rv.ty, want));
            }
        }
        let n = self.fresh("t");
        let id = self.b.tmp(n, rv.ty, expr.span);
        let place = self.b.local_place(id, expr.span);
        self.b.assign(expr.span, place.clone(), rv);
        Ok(Operand::Copy(place))
    }

    fn rvalue(&mut self, expr: &Expr, expected: Option<Ty>) -> Result<Rvalue, CheckError> {
        if let Some(v) = self.fold_int(expr) {
            if v == 0 {
                if let Some(want) = expected {
                    if self.is_ptr_ty(want) {
                        return Ok(Rvalue::use_op(expr.span, Operand::null(expr.span, want)));
                    }
                }
            }
            return Ok(Rvalue::use_op(
                expr.span,
                self.num_from_int(expr.span, v, expected),
            ));
        }
        match &expr.kind {
            ExprKind::Name(n) => {
                if let Some(place) = self.aliases.get(&n.name).cloned() {
                    return Ok(Rvalue::use_op(expr.span, Operand::Copy(place)));
                }
                if let Some(&local) = self.locals.get(&n.name) {
                    let place = self.b.local_place(local, n.span);
                    return Ok(Rvalue::use_op(expr.span, Operand::Copy(place)));
                }
                if let Some(&gid) = self.globals.get(&n.name) {
                    let ty = self.cx.global(gid).ty;
                    let place =
                        Place::global(gid, ty, crate::ir::place::Access::let_binding(), n.span);
                    return Ok(Rvalue::use_op(expr.span, Operand::Copy(place)));
                }
                if let Some(c) = self.consts.get(&n.name) {
                    return Ok(self.const_rvalue(expr.span, c));
                }
                if let Some(&id) = self.funcs.get(&n.name) {
                    let fty = self.cx.func(id).ty;
                    if let Some(want) = expected {
                        if want != fty {
                            return Err(CheckError::new(
                                self.path,
                                n.span,
                                "function type does not match expected type",
                            ));
                        }
                    }
                    return Ok(Rvalue::use_op(
                        expr.span,
                        Operand::fn_addr(expr.span, fty, id),
                    ));
                }
                Err(self.unknown(n.span, "name", &n.name, &[]))
            }
            ExprKind::Bool(v) => Ok(Rvalue::use_op(
                expr.span,
                Operand::bool_const(expr.span, self.cx.common.bool, *v),
            )),
            ExprKind::Int(v) => Ok(Rvalue::use_op(
                expr.span,
                self.num_from_int(expr.span, *v, expected),
            )),
            ExprKind::Float(v) => Ok(Rvalue::use_op(
                expr.span,
                self.num_from_float(expr.span, *v, expected),
            )),
            ExprKind::StructLit {
                name,
                type_args,
                fields,
            } => self.struct_lit(expr.span, name, type_args, fields),
            ExprKind::TypePath { name, .. } => Err(CheckError::new(
                self.path,
                name.span,
                format!("`{}` is a type, not a value", name.name),
            )),
            ExprKind::SizeOf(ty) => {
                let t = self.resolve_ty(ty)?;
                Ok(Rvalue::use_op(
                    expr.span,
                    Operand::int(
                        expr.span,
                        self.cx.common.int,
                        crate::ir::abi::size(self.cx, t),
                    ),
                ))
            }
            ExprKind::SliceFrom { ptr, len } => self.slice_from(expr.span, ptr, len, expected),
            ExprKind::ArrayLit { elems } => self.array_lit(expr.span, elems, expected),
            ExprKind::SliceLit { elem, fields } => self.slice_lit(expr.span, elem, fields),
            ExprKind::Index { .. } => {
                let place = self.place(expr)?;
                Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
            }
            ExprKind::Slice { base, start, end } => {
                self.slice_rvalue(expr.span, base, start.as_deref(), end.as_deref())
            }
            ExprKind::String(bytes) => Ok(Rvalue::use_op(
                expr.span,
                Operand::bytes(expr.span, self.cx.common.string, bytes),
            )),
            ExprKind::Field { base, field } => {
                if let Some(ename) = self.enum_ctor_name(base) {
                    return self.construct_variant(expr.span, &ename, &field.name, None, expected);
                }
                if field.name == "len" {
                    if let Ok(p) = self.try_place(base) {
                        let mut p = p;
                        while self.cx.ref_inner(p.ty).is_some() {
                            p = self.b.deref(self.cx, p, field.span);
                        }
                        if let TyKind::Array { len, .. } = self.cx.ty_kind(p.ty) {
                            return Ok(Rvalue::use_op(
                                expr.span,
                                Operand::int(expr.span, self.cx.common.int, *len),
                            ));
                        }
                    }
                }
                let place = self.place(expr)?;
                Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
            }
            ExprKind::Call {
                func,
                type_args,
                args,
            } => self.call(expr.span, func, type_args, args, expected),
            ExprKind::Syscall { args } => self.syscall(expr.span, args),
            ExprKind::Cast { expr: inner, ty } => {
                let to = self.resolve_ty(ty)?;
                let from = self.operand(inner, None)?;
                Ok(Rvalue::cast(expr.span, to, from))
            }
            ExprKind::Binary { op, lhs, rhs } => self.binary(expr.span, *op, lhs, rhs, expected),
            ExprKind::If {
                cond,
                then_expr,
                else_expr,
            } => {
                let c = self.operand(cond, Some(self.cx.common.bool))?;
                let saved = self.b.begin_block();
                let then_rv = self.rvalue(then_expr, expected)?;
                let ty = then_rv.ty;
                let n = self.fresh("if");
                let id = self.b.tmp(n, ty, expr.span);
                let place = self.b.local_place(id, expr.span);
                self.b.assign(then_expr.span, place.clone(), then_rv);
                let then_block = self.b.end_block(saved);
                let saved = self.b.begin_block();
                let else_rv = self.rvalue(else_expr, Some(ty))?;
                if else_rv.ty != ty {
                    return Err(CheckError::new(
                        self.path,
                        expr.span,
                        "if-expression branches have different types",
                    ));
                }
                self.b.assign(else_expr.span, place.clone(), else_rv);
                let else_block = self.b.end_block(saved);
                self.b.push(crate::ir::stmt::Stmt {
                    ty: self.cx.common.void,
                    span: expr.span,
                    kind: crate::ir::stmt::StmtKind::If {
                        cond: c,
                        then_block,
                        else_block: Some(else_block),
                    },
                });
                Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
            }
            ExprKind::Unary { op, expr: inner } => match op {
                parse::UnaryOp::Neg => {
                    let arg = self.operand(inner, expected)?;
                    if !is_numeric(self.cx, arg.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            expr.span,
                            "unary `-` needs a number",
                        ));
                    }
                    Ok(Rvalue {
                        ty: arg.ty(),
                        span: expr.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Neg,
                            arg,
                        },
                    })
                }
                parse::UnaryOp::Not => {
                    let arg = self.operand(inner, Some(self.cx.common.bool))?;
                    Ok(Rvalue {
                        ty: self.cx.common.bool,
                        span: expr.span,
                        kind: RvalueKind::Unary {
                            op: UnaryOp::Not,
                            arg,
                        },
                    })
                }
                parse::UnaryOp::Ref => {
                    if let ExprKind::Name(n) = &inner.kind {
                        if self.consts.contains_key(&n.name) {
                            return Err(CheckError::new(
                                self.path,
                                expr.span,
                                "cannot take `&` of a const",
                            ));
                        }
                    }
                    let place = self.place(inner)?;
                    if !place.access.take_ref || !place.access.assign {
                        return Err(CheckError::new(
                            self.path,
                            expr.span,
                            "cannot take `&` of this place",
                        ));
                    }
                    let ref_ty = self.cx.ty_ref(place.ty);
                    Ok(Rvalue::ref_place(expr.span, ref_ty, place))
                }
                parse::UnaryOp::Deref => {
                    let place = self.place(expr)?;
                    Ok(Rvalue::use_op(expr.span, Operand::Copy(place)))
                }
            },
            ExprKind::Interpolate { parts } => self.interpolate(expr.span, parts),
            ExprKind::Question { expr: inner } => self.question(expr.span, inner),
        }
    }

    fn const_rvalue(&self, span: Span, c: &ConstVal) -> Rvalue {
        match &c.kind {
            ConstPayload::Int(v) => Rvalue::use_op(span, Operand::int(span, c.ty, *v as u64)),
            ConstPayload::Float(v) => {
                if c.ty == self.cx.common.f32 {
                    Rvalue::use_op(span, Operand::f32_const(span, c.ty, *v as f32))
                } else {
                    Rvalue::use_op(span, Operand::f64_const(span, c.ty, *v))
                }
            }
            ConstPayload::String(b) => Rvalue::use_op(span, Operand::bytes(span, c.ty, b)),
        }
    }

    fn interpolate(&mut self, span: Span, parts: &[InterpPart]) -> Result<Rvalue, CheckError> {
        if let Some(bytes) = self.fold_interp(parts) {
            return Ok(Rvalue::use_op(
                span,
                Operand::bytes(span, self.cx.common.string, &bytes),
            ));
        }
        let mut ir_parts = Vec::new();
        for part in parts {
            match part {
                InterpPart::Lit(b) => {
                    ir_parts.push(crate::ir::rvalue::InterpPart::Lit(b.clone()));
                }
                InterpPart::Expr(e) => {
                    let op = self.operand(e, None)?;
                    if !self.interp_ok(op.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            e.span,
                            "this value cannot appear in interpolate (need primitive, string, or #[Debug])",
                        ));
                    }
                    ir_parts.push(crate::ir::rvalue::InterpPart::Value(op));
                }
            }
        }
        let cap = crate::ir::wrap::INTERP_CAP;
        let buf_ty = self.cx.ty_array(self.cx.common.u8, cap);
        let buf_n = self.fresh("ibuf");
        let buf_id = self.b.tmp(buf_n, buf_ty, span);
        let buf = self.b.local_place(buf_id, span);
        let used_n = self.fresh("in");
        let used_id = self.b.tmp(used_n, self.cx.common.int, span);
        let used = self.b.local_place(used_id, span);
        self.b.assign(
            span,
            used.clone(),
            Rvalue::use_op(span, Operand::int(span, self.cx.common.int, 0)),
        );
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::InterpFill {
                buf: buf.clone(),
                used: used.clone(),
                parts: ir_parts,
            },
        });
        let start = self.b.index(
            self.cx,
            buf,
            Operand::int(span, self.cx.common.int, 0),
            span,
        );
        let ptr_ty = self.cx.ty_ref(self.cx.common.u8);
        let ptr_n = self.fresh("iptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), Rvalue::ref_place(span, ptr_ty, start));
        Ok(Rvalue::aggregate(
            span,
            self.cx.common.string,
            vec![Operand::Copy(ptr_place), Operand::Copy(used)],
        ))
    }

    fn fold_interp(&self, parts: &[InterpPart]) -> Option<Vec<u8>> {
        let mut out = Vec::new();
        for part in parts {
            match part {
                InterpPart::Lit(b) => out.extend(b),
                InterpPart::Expr(e) => match self.fold_interp_expr(e) {
                    Some(b) => out.extend(b),
                    None => return None,
                },
            }
        }
        Some(out)
    }

    fn fold_interp_expr(&self, expr: &Expr) -> Option<Vec<u8>> {
        if let Some(v) = self.fold_int(expr) {
            return Some(v.to_string().into_bytes());
        }
        match &expr.kind {
            ExprKind::String(b) => Some(b.clone()),
            ExprKind::Bool(v) => Some(if *v {
                b"true".to_vec()
            } else {
                b"false".to_vec()
            }),
            ExprKind::Name(n) => match self.consts.get(&n.name) {
                Some(ConstVal {
                    kind: ConstPayload::String(b),
                    ..
                }) => Some(b.clone()),
                Some(ConstVal {
                    kind: ConstPayload::Int(v),
                    ..
                }) => Some(v.to_string().into_bytes()),
                Some(ConstVal {
                    kind: ConstPayload::Float(v),
                    ..
                }) => Some(v.to_string().into_bytes()),
                None => None,
            },
            _ => None,
        }
    }

    fn call(
        &mut self,
        span: Span,
        func: &Expr,
        type_args: &[parse::Type],
        args: &[Expr],
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        if let ExprKind::Field { base, field } = &func.kind {
            if let Some(ename) = self.enum_ctor_name(base) {
                if args.len() != 1 {
                    return Err(CheckError::new(
                        self.path,
                        span,
                        "enum variant takes one payload",
                    ));
                }
                return self.construct_variant(span, &ename, &field.name, Some(&args[0]), expected);
            }
        }
        match &func.kind {
            ExprKind::Name(n) => {
                if args.len() == 1 {
                    if let Some(to) = self.conv_ty(&n.name) {
                        let from = self.operand(&args[0], None)?;
                        return Ok(Rvalue::cast(span, to, from));
                    }
                }
                if let Some(id) = self.funcs.get(&n.name).copied() {
                    return self.call_id(span, id, None, args);
                }
                if let Some(place) = self.aliases.get(&n.name).cloned() {
                    return self.call_fn_op(span, Operand::Copy(place), args);
                }
                if let Some(&local) = self.locals.get(&n.name) {
                    let place = self.b.local_place(local, n.span);
                    return self.call_fn_op(span, Operand::Copy(place), args);
                }
                Err(self.unknown(n.span, "function", &n.name, &[]))
            }
            ExprKind::Field { base, field } => {
                if let ExprKind::Name(n) = &base.kind {
                    if let Some(ns) = self
                        .lib_ns
                        .get(&n.name)
                        .or_else(|| self.mod_ns.get(&n.name))
                    {
                        let id = match ns.get(&field.name).copied() {
                            Some(id) => id,
                            None => {
                                return Err(CheckError::new(
                                    self.path,
                                    field.span,
                                    format!("`{}` has no `{}`", n.name, field.name),
                                ));
                            }
                        };
                        return self.call_id(span, id, None, args);
                    }
                    if let Some(sid) = self.struct_id_named(&n.name) {
                        let targs = self.resolve_type_args(type_args)?;
                        let id = self.lookup_method(sid, &field.name, targs, field.span)?;
                        return self.call_id(span, id, None, args);
                    }
                }
                if let ExprKind::TypePath { name, args: targs } = &base.kind {
                    let sid = self.instantiate_struct_named(&name.name, targs, name.span)?;
                    let extra = self.resolve_type_args(type_args)?;
                    let id = self.lookup_method(sid, &field.name, extra, field.span)?;
                    return self.call_id(span, id, None, args);
                }
                let recv = self.receiver_place(base)?;
                let mut recv_ty = recv.ty;
                while let Some(inner) = self.cx.ref_inner(recv_ty) {
                    recv_ty = inner;
                }
                let sid = match self.cx.ty_kind(recv_ty) {
                    TyKind::Struct(id) => *id,
                    _ => {
                        return Err(CheckError::new(
                            self.path,
                            field.span,
                            format!("no method `{}`", field.name),
                        ));
                    }
                };
                let targs = self.resolve_type_args(type_args)?;
                let id = self.lookup_method(sid, &field.name, targs, field.span)?;
                self.call_id(span, id, Some(recv), args)
            }
            _ => Err(CheckError::new(
                self.path,
                func.span,
                "cannot call this expression",
            )),
        }
    }

    fn call_id(
        &mut self,
        span: Span,
        id: FuncId,
        recv: Option<Place>,
        args: &[Expr],
    ) -> Result<Rvalue, CheckError> {
        let fty = self.cx.func(id).ty;
        let (params, ret) = match self.cx.fn_sig(fty) {
            Some(s) => (s.0.clone(), s.1),
            None => {
                return Err(CheckError::new(self.path, span, "not a function"));
            }
        };
        let mut ops = Vec::new();
        let mut arg_i = 0;
        if let Some(recv) = recv {
            if params.is_empty() {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "method has no `this` parameter",
                ));
            }
            ops.push(self.auto_this(span, recv, params[0])?);
            arg_i = 1;
        }
        if params.len() - arg_i != args.len() {
            return Err(CheckError::new(
                self.path,
                span,
                format!("takes {} args, got {}", params.len() - arg_i, args.len()),
            ));
        }
        for (i, arg) in args.iter().enumerate() {
            ops.push(self.operand(arg, Some(params[arg_i + i]))?);
        }
        Ok(Rvalue::call(
            span,
            ret,
            Operand::fn_addr(span, fty, id),
            ops,
        ))
    }

    fn call_fn_op(
        &mut self,
        span: Span,
        func: Operand,
        args: &[Expr],
    ) -> Result<Rvalue, CheckError> {
        let (params, ret) = match self.cx.fn_sig(func.ty()) {
            Some(s) => (s.0.clone(), s.1),
            None => {
                return Err(CheckError::new(self.path, span, "not a function"));
            }
        };
        if params.len() != args.len() {
            return Err(CheckError::new(
                self.path,
                span,
                format!("takes {} args, got {}", params.len(), args.len()),
            ));
        }
        let mut ops = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            ops.push(self.operand(arg, Some(params[i]))?);
        }
        Ok(Rvalue::call(span, ret, func, ops))
    }

    fn auto_this(&mut self, span: Span, recv: Place, want: Ty) -> Result<Operand, CheckError> {
        if recv.ty == want {
            return Ok(Operand::Copy(recv));
        }
        if self.cx.ref_inner(want) == Some(recv.ty) {
            if !recv.access.take_ref {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "cannot take `&` of this receiver",
                ));
            }
            let rv = Rvalue::ref_place(span, want, recv);
            let n = self.fresh("this");
            let id = self.b.tmp(n, want, span);
            let place = self.b.local_place(id, span);
            self.b.assign(span, place.clone(), rv);
            return Ok(Operand::Copy(place));
        }
        if self.cx.ref_inner(recv.ty) == Some(want) {
            let deref = self.b.deref(self.cx, recv, span);
            return Ok(Operand::Copy(deref));
        }
        Err(CheckError::new(
            self.path,
            span,
            "method receiver type mismatch",
        ))
    }

    fn receiver_place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        if let Ok(p) = self.try_place(expr) {
            return Ok(p);
        }
        let op = self.operand(expr, None)?;
        let n = self.fresh("recv");
        let id = self.b.tmp(n, op.ty(), expr.span);
        let place = self.b.local_place(id, expr.span);
        self.b
            .assign(expr.span, place.clone(), Rvalue::use_op(expr.span, op));
        Ok(place)
    }

    fn syscall(&mut self, span: Span, args: &[Expr]) -> Result<Rvalue, CheckError> {
        if args.is_empty() {
            return Err(CheckError::new(self.path, span, "@syscall needs a number"));
        }
        let int_ty = self.cx.common.int;
        let nr = self.operand(&args[0], Some(int_ty))?;
        let mut rest = Vec::new();
        for a in &args[1..] {
            rest.push(self.operand(a, None)?);
        }
        Ok(Rvalue::syscall(span, int_ty, nr, rest))
    }

    fn place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        match self.try_place(expr) {
            Ok(p) => Ok(p),
            Err(e) => Err(e),
        }
    }

    fn try_place(&mut self, expr: &Expr) -> Result<Place, CheckError> {
        match &expr.kind {
            ExprKind::Name(n) => {
                if let Some(place) = self.aliases.get(&n.name).cloned() {
                    return Ok(place);
                }
                if let Some(id) = self.locals.get(&n.name).copied() {
                    return Ok(self.b.local_place(id, n.span));
                }
                if let Some(&gid) = self.globals.get(&n.name) {
                    let ty = self.cx.global(gid).ty;
                    return Ok(Place::global(
                        gid,
                        ty,
                        crate::ir::place::Access::let_binding(),
                        n.span,
                    ));
                }
                Err(CheckError::new(
                    self.path,
                    n.span,
                    format!("`{}` is not a place", n.name),
                ))
            }
            ExprKind::String(bytes) => {
                let n = self.fresh("s");
                let id = self.b.tmp(n, self.cx.common.string, expr.span);
                let place = self.b.local_place(id, expr.span);
                self.b.assign(
                    expr.span,
                    place.clone(),
                    Rvalue::use_op(
                        expr.span,
                        Operand::bytes(expr.span, self.cx.common.string, bytes),
                    ),
                );
                Ok(place)
            }
            ExprKind::Index { base, index } => {
                let mut place = self.try_place(base)?;
                while self.cx.ref_inner(place.ty).is_some() {
                    place = self.b.deref(self.cx, place, expr.span);
                }
                if self.cx.elem_ty(place.ty).is_none() {
                    return Err(CheckError::new(
                        self.path,
                        expr.span,
                        "index of non-array/slice",
                    ));
                }
                let idx = self.operand(index, Some(self.cx.common.int))?;
                let idx = self.bind_operand(idx, expr.span);
                let len = self.seq_len(&place, expr.span)?;
                self.bounds_check(expr.span, &idx, &len)?;
                Ok(self.b.index(self.cx, place, idx, expr.span))
            }
            ExprKind::Field { base, field } => {
                let mut place = self.try_place(base)?;
                while self.cx.ref_inner(place.ty).is_some() {
                    place = self.b.deref(self.cx, place, field.span);
                }
                match self.cx.ty_kind(place.ty) {
                    TyKind::Struct(sid) => {
                        let sid = *sid;
                        match field_path(self.cx, sid, &field.name) {
                            Some(path) => {
                                let mut place = place;
                                for fid in path {
                                    place = self.b.field(self.cx, place, fid, field.span);
                                }
                                Ok(place)
                            }
                            None => Err(CheckError::new(
                                self.path,
                                field.span,
                                format!("no field `{}`", field.name),
                            )),
                        }
                    }
                    TyKind::String | TyKind::Slice { .. } => match field.name.as_str() {
                        "ptr" => Ok(self.b.slice_ptr(self.cx, place, field.span)),
                        "len" => Ok(self.b.slice_len(self.cx, place, field.span)),
                        other => Err(CheckError::new(
                            self.path,
                            field.span,
                            format!("no field `.{other}`"),
                        )),
                    },
                    _ => Err(CheckError::new(
                        self.path,
                        field.span,
                        format!("no field `{}`", field.name),
                    )),
                }
            }
            ExprKind::Unary {
                op: parse::UnaryOp::Deref,
                expr: inner,
            } => {
                let inner_place = self.try_place(inner)?;
                Ok(self.b.deref(self.cx, inner_place, expr.span))
            }
            _ => Err(CheckError::new(
                self.path,
                expr.span,
                "expression is not a place",
            )),
        }
    }

    fn lower_block(&mut self, block: &parse::Block) -> Result<crate::ir::body::Block, CheckError> {
        let saved = self.b.begin_block();
        for stmt in &block.stmts {
            self.stmt(stmt)?;
        }
        Ok(self.b.end_block(saved))
    }

    fn binary(
        &mut self,
        span: Span,
        op: parse::BinOp,
        lhs: &Expr,
        rhs: &Expr,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let ir_op = ast_binop(op);
        match op {
            parse::BinOp::And | parse::BinOp::Or => {
                let l = self.operand(lhs, Some(self.cx.common.bool))?;
                let bool_ty = self.cx.common.bool;
                let n = self.fresh("sc");
                let id = self.b.tmp(n, bool_ty, span);
                let place = self.b.local_place(id, span);
                if op == parse::BinOp::And {
                    let saved = self.b.begin_block();
                    let r = self.operand(rhs, Some(bool_ty))?;
                    self.b.assign(span, place.clone(), Rvalue::use_op(span, r));
                    let then_block = self.b.end_block(saved);
                    let saved = self.b.begin_block();
                    self.b.assign(
                        span,
                        place.clone(),
                        Rvalue::use_op(span, Operand::bool_const(span, bool_ty, false)),
                    );
                    let else_block = self.b.end_block(saved);
                    self.b.push(crate::ir::stmt::Stmt {
                        ty: self.cx.common.void,
                        span,
                        kind: crate::ir::stmt::StmtKind::If {
                            cond: l,
                            then_block,
                            else_block: Some(else_block),
                        },
                    });
                } else {
                    let saved = self.b.begin_block();
                    self.b.assign(
                        span,
                        place.clone(),
                        Rvalue::use_op(span, Operand::bool_const(span, bool_ty, true)),
                    );
                    let then_block = self.b.end_block(saved);
                    let saved = self.b.begin_block();
                    let r = self.operand(rhs, Some(bool_ty))?;
                    self.b.assign(span, place.clone(), Rvalue::use_op(span, r));
                    let else_block = self.b.end_block(saved);
                    self.b.push(crate::ir::stmt::Stmt {
                        ty: self.cx.common.void,
                        span,
                        kind: crate::ir::stmt::StmtKind::If {
                            cond: l,
                            then_block,
                            else_block: Some(else_block),
                        },
                    });
                }
                let _ = ir_op;
                Ok(Rvalue::use_op(span, Operand::Copy(place)))
            }
            parse::BinOp::Eq
            | parse::BinOp::Ne
            | parse::BinOp::Lt
            | parse::BinOp::Le
            | parse::BinOp::Gt
            | parse::BinOp::Ge => {
                if bare_num(lhs) && !bare_num(rhs) {
                    let r = self.operand(rhs, None)?;
                    if is_numeric(self.cx, r.ty()) {
                        let l = self.operand(lhs, Some(r.ty()))?;
                        if l.ty() != r.ty() {
                            return Err(CheckError::new(self.path, span, "compare type mismatch"));
                        }
                        return Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r));
                    }
                    if self.is_ptr_ty(r.ty()) && self.fold_int(lhs) == Some(0) {
                        let l = Operand::null(lhs.span, r.ty());
                        return Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r));
                    }
                    return Err(CheckError::new(self.path, span, "compare type mismatch"));
                }
                let l = self.operand(lhs, None)?;
                if let Some(id) = find_operator(self.cx, l.ty(), op, self.methods) {
                    let fty = self.cx.func(id).ty;
                    let (params, ret) = match self.cx.fn_sig(fty) {
                        Some(s) => (s.0.clone(), s.1),
                        None => {
                            return Err(CheckError::new(self.path, span, "not a function"));
                        }
                    };
                    if params.len() != 2 {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "binary operator needs two parameters",
                        ));
                    }
                    if l.ty() != params[0] {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "operator `this` type mismatch",
                        ));
                    }
                    let r = self.operand(rhs, Some(params[1]))?;
                    return Ok(Rvalue::call(
                        span,
                        ret,
                        Operand::fn_addr(span, fty, id),
                        vec![l, r],
                    ));
                }
                let r_expected = Some(l.ty());
                let r = if self.is_ptr_ty(l.ty()) && self.fold_int(rhs) == Some(0) {
                    Operand::null(rhs.span, l.ty())
                } else {
                    self.operand(rhs, r_expected)?
                };
                if l.ty() != r.ty() {
                    if self.is_ptr_ty(r.ty()) && self.fold_int(lhs) == Some(0) {
                        let l = Operand::null(lhs.span, r.ty());
                        return Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r));
                    }
                    return Err(CheckError::new(self.path, span, "compare type mismatch"));
                }
                Ok(Rvalue::binary(span, self.cx.common.bool, ir_op, l, r))
            }
            parse::BinOp::Add | parse::BinOp::Sub | parse::BinOp::Mul | parse::BinOp::Div => {
                if bare_num(lhs) && !bare_num(rhs) {
                    let right_expect = match expected {
                        Some(t) if is_numeric(self.cx, t) => Some(t),
                        _ => None,
                    };
                    let r = self.operand(rhs, right_expect)?;
                    if !is_numeric(self.cx, r.ty()) {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "arithmetic type mismatch",
                        ));
                    }
                    let l = self.operand(lhs, Some(r.ty()))?;
                    if l.ty() != r.ty() {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "arithmetic type mismatch",
                        ));
                    }
                    if matches!(op, parse::BinOp::Div) && is_int(self.cx, r.ty()) {
                        self.abort_if_zero(span, &r)?;
                    }
                    return Ok(Rvalue::binary(span, r.ty(), ir_op, l, r));
                }
                let l = self.operand(lhs, None)?;
                if let Some(id) = find_operator(self.cx, l.ty(), op, self.methods) {
                    let fty = self.cx.func(id).ty;
                    let (params, ret) = match self.cx.fn_sig(fty) {
                        Some(s) => (s.0.clone(), s.1),
                        None => {
                            return Err(CheckError::new(self.path, span, "not a function"));
                        }
                    };
                    if params.len() != 2 {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "binary operator needs two parameters",
                        ));
                    }
                    if l.ty() != params[0] {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            "operator `this` type mismatch",
                        ));
                    }
                    let r = self.operand(rhs, Some(params[1]))?;
                    return Ok(Rvalue::call(
                        span,
                        ret,
                        Operand::fn_addr(span, fty, id),
                        vec![l, r],
                    ));
                }
                let ty = if is_numeric(self.cx, l.ty()) {
                    l.ty()
                } else if let Some(t) = expected {
                    if is_numeric(self.cx, t) {
                        t
                    } else {
                        self.cx.common.int
                    }
                } else {
                    self.cx.common.int
                };
                if l.ty() != ty {
                    return Err(CheckError::new(self.path, span, "arithmetic type mismatch"));
                }
                let r = self.operand(rhs, Some(ty))?;
                if r.ty() != ty {
                    return Err(CheckError::new(self.path, span, "arithmetic type mismatch"));
                }
                if matches!(op, parse::BinOp::Div) && is_int(self.cx, ty) {
                    self.abort_if_zero(span, &r)?;
                }
                let _ = expected;
                Ok(Rvalue::binary(span, ty, ir_op, l, r))
            }
        }
    }

    fn abort_if_zero(&mut self, span: Span, rhs: &Operand) -> Result<(), CheckError> {
        let bool_ty = self.cx.common.bool;
        let n = self.fresh("dz");
        let id = self.b.tmp(n, bool_ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(
            span,
            p.clone(),
            Rvalue::binary(
                span,
                bool_ty,
                BinOp::Eq,
                rhs.clone(),
                Operand::int(span, rhs.ty(), 0),
            ),
        );
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Div0,
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond: Operand::Copy(p),
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    fn fold_int(&self, expr: &Expr) -> Option<i64> {
        match &expr.kind {
            ExprKind::Int(v) => Some(*v),
            ExprKind::Name(n) => match self.consts.get(&n.name) {
                Some(ConstVal {
                    kind: ConstPayload::Int(v),
                    ..
                }) => Some(*v),
                _ => None,
            },
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.fold_int(lhs)?;
                let b = self.fold_int(rhs)?;
                crate::ir::wrap::fold_bin(ast_binop(*op), a, b)
            }
            ExprKind::Unary {
                op: parse::UnaryOp::Neg,
                expr,
            } => Some(crate::ir::wrap::fold_neg(self.fold_int(expr)?)),
            _ => None,
        }
    }

    fn num_from_int(&self, span: Span, v: i64, expected: Option<Ty>) -> Operand {
        match expected {
            Some(t) if v == 0 && self.is_ptr_ty(t) => Operand::null(span, t),
            Some(t) if is_int(self.cx, t) => Operand::int(span, t, v as u64),
            Some(t) if t == self.cx.common.f32 => Operand::f32_const(span, t, v as f32),
            Some(t) if is_float(self.cx, t) => Operand::f64_const(span, t, v as f64),
            _ => Operand::int(span, self.cx.common.int, v as u64),
        }
    }

    fn num_from_float(&self, span: Span, v: f64, expected: Option<Ty>) -> Operand {
        match expected {
            Some(t) if t == self.cx.common.f32 => Operand::f32_const(span, t, v as f32),
            Some(t) if is_float(self.cx, t) => Operand::f64_const(span, t, v),
            _ => Operand::f64_const(span, self.cx.common.float, v),
        }
    }

    fn interp_ok(&self, ty: Ty) -> bool {
        if ty == self.cx.common.string || ty == self.cx.common.bool {
            return true;
        }
        if is_int(self.cx, ty) || is_float(self.cx, ty) {
            return true;
        }
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => self.cx.struct_def(*id).debug,
            _ => false,
        }
    }

    fn struct_lit(
        &mut self,
        span: Span,
        name: &parse::Ident,
        type_args: &[parse::Type],
        fields: &[(parse::Ident, Expr)],
    ) -> Result<Rvalue, CheckError> {
        let ty = if type_args.is_empty() {
            match self.types.get(&name.name).copied() {
                Some(t) => t,
                None => {
                    return Err(CheckError::new(
                        self.path,
                        name.span,
                        format!("unknown type `{}`", name.name),
                    ));
                }
            }
        } else {
            let sid = self.instantiate_struct_named(&name.name, type_args, name.span)?;
            self.cx.ty_struct(sid)
        };
        let sid = match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => *id,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    name.span,
                    format!("`{}` is not a struct", name.name),
                ));
            }
        };
        let nfields = self.cx.struct_def(sid).fields.len();
        let mut ops: Vec<Option<Operand>> = vec![None; nfields];
        for (fname, value) in fields {
            let mut found = None;
            for (i, f) in self.cx.struct_def(sid).fields.iter().enumerate() {
                if self.cx.name_str(f.name) == fname.name {
                    found = Some((i, f.ty));
                    break;
                }
            }
            let (i, fty) = match found {
                Some(x) => x,
                None => {
                    return Err(CheckError::new(
                        self.path,
                        fname.span,
                        format!("`{}` has no field `{}`", name.name, fname.name),
                    ));
                }
            };
            if ops[i].is_some() {
                return Err(CheckError::new(
                    self.path,
                    fname.span,
                    format!("duplicate field `{}`", fname.name),
                ));
            }
            ops[i] = Some(self.operand(value, Some(fty))?);
        }
        let mut out = Vec::new();
        for (i, op) in ops.into_iter().enumerate() {
            match op {
                Some(op) => out.push(op),
                None => {
                    if let Some(op) = self.cx.struct_def(sid).fields[i].default.clone() {
                        out.push(op);
                        continue;
                    }
                    let fname = self.cx.name_str(self.cx.struct_def(sid).fields[i].name);
                    return Err(CheckError::new(
                        self.path,
                        span,
                        format!("missing field `{fname}`"),
                    ));
                }
            }
        }
        Ok(Rvalue::aggregate(span, ty, out))
    }

    fn array_lit(
        &mut self,
        span: Span,
        elems: &[Expr],
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let mut as_slice = None;
        let (elem_ty, len) = match expected {
            Some(t) => match self.cx.ty_kind(t) {
                TyKind::Array { elem, len } => {
                    if *len != elems.len() as u64 {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            format!("array literal has {} elems, expected {len}", elems.len()),
                        ));
                    }
                    (*elem, *len)
                }
                TyKind::Slice { elem } => {
                    as_slice = Some(t);
                    (*elem, elems.len() as u64)
                }
                _ => {
                    return Err(CheckError::new(
                        self.path,
                        span,
                        "array literal needs an array or slice type",
                    ));
                }
            },
            None => {
                if elems.is_empty() {
                    return Err(CheckError::new(self.path, span, "cannot infer empty array"));
                }
                let first = self.operand(&elems[0], None)?;
                (first.ty(), elems.len() as u64)
            }
        };
        let ty = self.cx.ty_array(elem_ty, len);
        let mut fields = Vec::new();
        for e in elems {
            fields.push(self.operand(e, Some(elem_ty))?);
        }
        let arr = Rvalue::aggregate(span, ty, fields);
        if let Some(want) = as_slice {
            let n = self.fresh("a");
            let id = self.b.tmp(n, ty, span);
            let place = self.b.local_place(id, span);
            self.b.assign(span, place.clone(), arr);
            match self.array_to_slice(span, &place, want)? {
                Some(op) => Ok(Rvalue::use_op(span, op)),
                None => Err(CheckError::new(
                    self.path,
                    span,
                    "array literal does not convert to slice",
                )),
            }
        } else {
            Ok(arr)
        }
    }

    fn slice_lit(
        &mut self,
        span: Span,
        elem: &parse::Type,
        fields: &[(parse::Ident, Expr)],
    ) -> Result<Rvalue, CheckError> {
        let elem_ty = self.resolve_ty(elem)?;
        let slice_ty = self.cx.ty_slice(elem_ty);
        let mut ptr = None;
        let mut len = None;
        for (name, value) in fields {
            match name.name.as_str() {
                "ptr" => {
                    let want = self.cx.ty_ref(elem_ty);
                    ptr = Some(self.operand(value, Some(want))?);
                }
                "len" => {
                    len = Some(self.operand(value, Some(self.cx.common.int))?);
                }
                other => {
                    return Err(CheckError::new(
                        self.path,
                        name.span,
                        format!("slice literal has no field `{other}`"),
                    ));
                }
            }
        }
        let ptr = match ptr {
            Some(p) => p,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "slice literal needs `ptr`",
                ));
            }
        };
        let len = match len {
            Some(n) => n,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "slice literal needs `len`",
                ));
            }
        };
        Ok(Rvalue::aggregate(span, slice_ty, vec![ptr, len]))
    }

    fn conv_ty(&self, name: &str) -> Option<Ty> {
        Some(match name {
            "i8" => self.cx.common.i8,
            "i16" => self.cx.common.i16,
            "i32" => self.cx.common.i32,
            "i64" | "int" => self.cx.common.int,
            "u8" => self.cx.common.u8,
            "u16" => self.cx.common.u16,
            "u32" => self.cx.common.u32,
            "u64" => self.cx.common.u64,
            "f32" => self.cx.common.f32,
            "f64" | "float" => self.cx.common.float,
            _ => return None,
        })
    }

    fn slice_rvalue(
        &mut self,
        span: Span,
        base: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
    ) -> Result<Rvalue, CheckError> {
        let mut place = self.try_place(base)?;
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, span);
        }
        let elem = match self.cx.elem_ty(place.ty) {
            Some(e) => e,
            None => {
                return Err(CheckError::new(self.path, span, "slice of non-array/slice"));
            }
        };
        let full_len = self.seq_len(&place, span)?;
        let start_op = match start {
            Some(e) => self.operand(e, Some(self.cx.common.int))?,
            None => Operand::int(span, self.cx.common.int, 0),
        };
        let end_op = match end {
            Some(e) => self.operand(e, Some(self.cx.common.int))?,
            None => full_len.clone(),
        };
        self.slice_bounds(span, &start_op, &end_op, &full_len)?;
        let is_string = place.ty == self.cx.common.string;
        let start_place = self.b.index(self.cx, place, start_op.clone(), span);
        if !start_place.access.take_ref {
            return Err(CheckError::new(
                self.path,
                span,
                "cannot take a slice of this place",
            ));
        }
        let ptr_ty = self.cx.ty_ref(elem);
        let ptr_rv = Rvalue::ref_place(span, ptr_ty, start_place);
        let ptr_n = self.fresh("ptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), ptr_rv);
        let len_n = self.fresh("sl");
        let len_id = self.b.tmp(len_n, self.cx.common.int, span);
        let len_place = self.b.local_place(len_id, span);
        self.b.assign(
            span,
            len_place.clone(),
            Rvalue::binary(span, self.cx.common.int, BinOp::Sub, end_op, start_op),
        );
        let slice_ty = if is_string {
            self.cx.common.string
        } else {
            self.cx.ty_slice(elem)
        };
        Ok(Rvalue::aggregate(
            span,
            slice_ty,
            vec![Operand::Copy(ptr_place), Operand::Copy(len_place)],
        ))
    }

    fn bind_operand(&mut self, op: Operand, span: Span) -> Operand {
        match &op {
            Operand::Const(_) => op,
            Operand::Copy(p) if p.proj.is_empty() => op,
            _ => {
                let n = self.fresh("i");
                let id = self.b.tmp(n, op.ty(), span);
                let p = self.b.local_place(id, span);
                self.b.assign(span, p.clone(), Rvalue::use_op(span, op));
                Operand::Copy(p)
            }
        }
    }

    fn text_to_string(
        &mut self,
        span: Span,
        place: &Place,
        want: Ty,
    ) -> Result<Option<Operand>, CheckError> {
        if want != self.cx.common.string {
            return Ok(None);
        }
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => return Ok(None),
        };
        if self.cx.name_str(self.cx.struct_def(sid).name) != "Text" {
            return Ok(None);
        }
        let path = match field_path(self.cx, sid, "bytes") {
            Some(p) => p,
            None => return Ok(None),
        };
        let mut p = place.clone();
        for fid in path {
            p = self.b.field(self.cx, p, fid, span);
        }
        let rv = Rvalue::cast(span, want, Operand::Copy(p));
        let n = self.fresh("txt");
        let id = self.b.tmp(n, want, span);
        let out = self.b.local_place(id, span);
        self.b.assign(span, out.clone(), rv);
        Ok(Some(Operand::Copy(out)))
    }

    fn array_to_slice(
        &mut self,
        span: Span,
        place: &Place,
        want: Ty,
    ) -> Result<Option<Operand>, CheckError> {
        let (elem, _) = match self.cx.ty_kind(place.ty) {
            TyKind::Array { elem, .. } => (*elem, ()),
            _ => return Ok(None),
        };
        match self.cx.ty_kind(want) {
            TyKind::Slice { elem: se } if *se == elem => {}
            _ => return Ok(None),
        }
        let rv = self.slice_rvalue_place(span, place.clone(), None, None)?;
        let n = self.fresh("sl");
        let id = self.b.tmp(n, rv.ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(span, p.clone(), rv);
        Ok(Some(Operand::Copy(p)))
    }

    fn slice_rvalue_place(
        &mut self,
        span: Span,
        place: Place,
        start: Option<Operand>,
        end: Option<Operand>,
    ) -> Result<Rvalue, CheckError> {
        let dummy = Expr {
            span,
            kind: ExprKind::Name(parse::Ident {
                name: String::new(),
                span,
            }),
        };
        let _ = dummy;
        let elem = self.cx.elem_ty(place.ty).expect("array/slice");
        let full_len = self.seq_len(&place, span)?;
        let start_op = start.unwrap_or_else(|| Operand::int(span, self.cx.common.int, 0));
        let end_op = end.unwrap_or_else(|| full_len.clone());
        self.slice_bounds(span, &start_op, &end_op, &full_len)?;
        let is_string = place.ty == self.cx.common.string;
        let start_place = self.b.index(self.cx, place, start_op.clone(), span);
        let ptr_ty = self.cx.ty_ref(elem);
        let ptr_rv = Rvalue::ref_place(span, ptr_ty, start_place);
        let ptr_n = self.fresh("ptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), ptr_rv);
        let len_n = self.fresh("sl");
        let len_id = self.b.tmp(len_n, self.cx.common.int, span);
        let len_place = self.b.local_place(len_id, span);
        self.b.assign(
            span,
            len_place.clone(),
            Rvalue::binary(span, self.cx.common.int, BinOp::Sub, end_op, start_op),
        );
        let slice_ty = if is_string {
            self.cx.common.string
        } else {
            self.cx.ty_slice(elem)
        };
        Ok(Rvalue::aggregate(
            span,
            slice_ty,
            vec![Operand::Copy(ptr_place), Operand::Copy(len_place)],
        ))
    }

    fn seq_len(&mut self, place: &Place, span: Span) -> Result<Operand, CheckError> {
        match self.cx.ty_kind(place.ty) {
            TyKind::Array { len, .. } => Ok(Operand::int(span, self.cx.common.int, *len)),
            TyKind::Slice { .. } | TyKind::String => {
                let p = self.b.slice_len(self.cx, place.clone(), span);
                Ok(Operand::Copy(p))
            }
            _ => Err(CheckError::new(self.path, span, "value has no length")),
        }
    }

    fn bounds_check(
        &mut self,
        span: Span,
        index: &Operand,
        len: &Operand,
    ) -> Result<(), CheckError> {
        let int = self.cx.common.int;
        let bool_ty = self.cx.common.bool;
        let lt = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(
                    span,
                    bool_ty,
                    BinOp::Lt,
                    index.clone(),
                    Operand::int(span, int, 0),
                ),
            );
            Operand::Copy(p)
        };
        let ge = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Ge, index.clone(), len.clone()),
            );
            Operand::Copy(p)
        };
        let cond = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Or, lt, ge),
            );
            Operand::Copy(p)
        };
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Abort,
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond,
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    fn slice_bounds(
        &mut self,
        span: Span,
        start: &Operand,
        end: &Operand,
        len: &Operand,
    ) -> Result<(), CheckError> {
        self.bounds_check_inclusive(span, start, len)?;
        self.bounds_check_inclusive(span, end, len)?;
        let bool_ty = self.cx.common.bool;
        let n = self.fresh("bc");
        let id = self.b.tmp(n, bool_ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(
            span,
            p.clone(),
            Rvalue::binary(span, bool_ty, BinOp::Lt, end.clone(), start.clone()),
        );
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Abort,
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond: Operand::Copy(p),
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    fn bounds_check_inclusive(
        &mut self,
        span: Span,
        index: &Operand,
        len: &Operand,
    ) -> Result<(), CheckError> {
        let int = self.cx.common.int;
        let bool_ty = self.cx.common.bool;
        let lt = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(
                    span,
                    bool_ty,
                    BinOp::Lt,
                    index.clone(),
                    Operand::int(span, int, 0),
                ),
            );
            Operand::Copy(p)
        };
        let gt = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Gt, index.clone(), len.clone()),
            );
            Operand::Copy(p)
        };
        let cond = {
            let n = self.fresh("bc");
            let id = self.b.tmp(n, bool_ty, span);
            let p = self.b.local_place(id, span);
            self.b.assign(
                span,
                p.clone(),
                Rvalue::binary(span, bool_ty, BinOp::Or, lt, gt),
            );
            Operand::Copy(p)
        };
        let saved = self.b.begin_block();
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::Abort,
        });
        let then_block = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond,
                then_block,
                else_block: None,
            },
        });
        Ok(())
    }

    fn for_each(&mut self, s: &parse::ForEachStmt) -> Result<(), CheckError> {
        if let Some(i) = &s.index {
            if self.locals.contains_key(&i.name) {
                return Err(CheckError::new(
                    self.path,
                    i.span,
                    format!("shadowing `{}` is a compile error", i.name),
                ));
            }
        }
        if self.locals.contains_key(&s.name.name) {
            return Err(CheckError::new(
                self.path,
                s.name.span,
                format!("shadowing `{}` is a compile error", s.name.name),
            ));
        }
        let mut iter = self.try_place(&s.iter)?;
        while self.cx.ref_inner(iter.ty).is_some() {
            iter = self.b.deref(self.cx, iter, s.span);
        }
        let elem = match self.cx.elem_ty(iter.ty) {
            Some(e) => e,
            None => {
                return Err(CheckError::new(
                    self.path,
                    s.iter.span,
                    "`for` over non-array/slice",
                ));
            }
        };
        let end = self.seq_len(&iter, s.span)?;
        let iname = match &s.index {
            Some(i) => i.name.clone(),
            None => format!("_i{}", self.tmp),
        };
        let ispan = match &s.index {
            Some(i) => i.span,
            None => s.span,
        };
        let n = self.cx.intern_name(&iname);
        let iid = self.b.let_local(n, self.cx.common.int, ispan);
        let iplace = self.b.local_place(iid, ispan);
        self.b.assign(
            s.span,
            iplace.clone(),
            Rvalue::use_op(s.span, Operand::int(s.span, self.cx.common.int, 0)),
        );
        if s.index.is_some() {
            self.locals.insert(iname.clone(), iid);
        }
        let nn = self.cx.intern_name(&s.name.name);
        let nid = self.b.let_local(nn, elem, s.name.span);
        self.locals.insert(s.name.name.clone(), nid);
        let saved = self.b.begin_block();
        let elem_place = self
            .b
            .index(self.cx, iter, Operand::Copy(iplace.clone()), s.span);
        let nplace = self.b.local_place(nid, s.name.span);
        self.b.assign(
            s.span,
            nplace,
            Rvalue::use_op(s.span, Operand::Copy(elem_place)),
        );
        for stmt in &s.body.stmts {
            self.stmt(stmt)?;
        }
        let body = self.b.end_block(saved);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span: s.span,
            kind: crate::ir::stmt::StmtKind::ForRange {
                index: iplace,
                end,
                body,
            },
        });
        if s.index.is_some() {
            self.locals.remove(&iname);
        }
        self.locals.remove(&s.name.name);
        Ok(())
    }

    fn use_local(&mut self, s: &parse::UseLocalStmt) -> Result<(), CheckError> {
        let mut place = match self.try_place(&Expr {
            span: s.name.span,
            kind: ExprKind::Name(s.name.clone()),
        }) {
            Ok(p) => p,
            Err(_) => {
                return Err(CheckError::new(
                    self.path,
                    s.name.span,
                    format!("unknown name `{}`", s.name.name),
                ));
            }
        };
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, s.span);
        }
        let sid = match self.cx.ty_kind(place.ty) {
            TyKind::Struct(id) => *id,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    s.name.span,
                    "`use` of a local only works on a struct",
                ));
            }
        };
        let names = embed_alias_names(self.cx, sid);
        for name in names {
            if self.locals.contains_key(&name) || self.aliases.contains_key(&name) {
                return Err(CheckError::new(
                    self.path,
                    s.name.span,
                    format!("shadowing `{name}` is a compile error"),
                ));
            }
            let path = field_path(self.cx, sid, &name).expect("embed name");
            let mut p = place.clone();
            for fid in path {
                p = self.b.field(self.cx, p, fid, s.span);
            }
            self.aliases.insert(name, p);
        }
        Ok(())
    }

    fn resolve_ty(&mut self, t: &parse::Type) -> Result<Ty, CheckError> {
        let path = self.path;
        resolve_type_g(
            self.cx,
            t,
            path,
            &mut GenEnv {
                types: self.types,
                subst: &self.type_subst,
                enum_temps: self.enum_temps,
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
            },
        )
    }

    fn struct_id_named(&self, name: &str) -> Option<StructId> {
        let ty = self.types.get(name)?;
        match self.cx.ty_kind(*ty) {
            TyKind::Struct(id) => Some(*id),
            _ => None,
        }
    }

    fn resolve_type_args(&mut self, args: &[parse::Type]) -> Result<Vec<Ty>, CheckError> {
        let mut out = Vec::new();
        for a in args {
            out.push(self.resolve_ty(a)?);
        }
        Ok(out)
    }

    fn instantiate_struct_named(
        &mut self,
        name: &str,
        args: &[parse::Type],
        span: Span,
    ) -> Result<StructId, CheckError> {
        let resolved = self.resolve_type_args(args)?;
        let path = self.path;
        let ty = instantiate_struct(
            self.cx,
            path,
            span,
            name,
            resolved,
            &mut GenEnv {
                types: self.types,
                subst: &self.type_subst,
                enum_temps: self.enum_temps,
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
            },
        )?;
        match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => Ok(*id),
            _ => Err(CheckError::new(self.path, span, "not a struct")),
        }
    }

    fn lookup_method(
        &mut self,
        sid: StructId,
        name: &str,
        type_args: Vec<Ty>,
        span: Span,
    ) -> Result<FuncId, CheckError> {
        if type_args.is_empty() {
            return match self.methods.get(&(sid.0, name.to_string())).copied() {
                Some(id) => Ok(id),
                None => Err(self.no_method(span, sid, name)),
            };
        }
        let key = (sid.0, name.to_string(), type_args.clone());
        if let Some(&id) = self.method_insts.get(&key) {
            return Ok(id);
        }
        let temp = match self.method_temps.get(&(sid.0, name.to_string())).cloned() {
            Some(f) => f,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("no generic method `{name}`"),
                ));
            }
        };
        if temp.type_params.len() != type_args.len() {
            return Err(CheckError::new(
                self.path,
                span,
                format!(
                    "`{name}` takes {} type args, got {}",
                    temp.type_params.len(),
                    type_args.len()
                ),
            ));
        }
        let mut subst = HashMap::new();
        for (pname, ty) in &self.cx.struct_def(sid).ty_args {
            subst.insert(pname.clone(), *ty);
        }
        for (p, a) in temp.type_params.iter().zip(type_args.iter()) {
            subst.insert(p.name.clone(), *a);
        }
        let id = {
            let mut types = self.types.clone();
            let struct_name = self.cx.name_str(self.cx.struct_def(sid).name).to_string();
            if let Some(env) = self.struct_temp_env.get(&struct_name) {
                for (k, v) in env {
                    types.entry(k.clone()).or_insert(*v);
                }
            }
            let mut env = GenEnv {
                types: &types,
                subst: &subst,
                enum_temps: self.enum_temps,
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
            alloc_fn(
                self.cx,
                self.path,
                self.module_names,
                &temp,
                Some(sid),
                &HashMap::new(),
                &mut env,
            )?
        };
        self.method_insts.insert(key, id);
        self.ir_funcs.push(id);
        self.fn_substs.insert(id, subst.clone());
        self.pending_mono.push((id, temp, subst));
        Ok(id)
    }

    fn slice_from(
        &mut self,
        span: Span,
        ptr: &Expr,
        len: &Expr,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let slice_ty = match expected {
            Some(t) if matches!(self.cx.ty_kind(t), TyKind::Slice { .. }) => t,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "`@slice` needs an expected slice type",
                ));
            }
        };
        let elem = self.cx.elem_ty(slice_ty).unwrap();
        let ptr_ty = self.cx.ty_ref(elem);
        let p = self.operand(ptr, None)?;
        let p = if p.ty() == ptr_ty {
            p
        } else if p.ty() == self.cx.common.ptr {
            let rv = Rvalue::cast(span, ptr_ty, p);
            Operand::Copy(self.materialize_rv(rv, span))
        } else {
            return Err(CheckError::new(
                self.path,
                span,
                "`@slice` pointer type mismatch",
            ));
        };
        let n = self.operand(len, Some(self.cx.common.int))?;
        Ok(Rvalue::aggregate(span, slice_ty, vec![p, n]))
    }

    fn is_ptr_ty(&self, ty: Ty) -> bool {
        matches!(
            self.cx.ty_kind(ty),
            TyKind::Ptr | TyKind::CStr | TyKind::Ref(_)
        )
    }

    fn enum_ctor_name(&self, expr: &Expr) -> Option<String> {
        match &expr.kind {
            ExprKind::Name(n) => {
                if self.enum_temps.contains_key(&n.name) {
                    return Some(n.name.clone());
                }
                if let Some(&ty) = self.types.get(&n.name) {
                    if matches!(self.cx.ty_kind(ty), TyKind::Enum(_)) {
                        return Some(n.name.clone());
                    }
                }
                None
            }
            ExprKind::Field { base, field } => {
                let ExprKind::Name(n) = &base.kind else {
                    return None;
                };
                let types = self.mod_types.get(&n.name)?;
                let ty = types.get(&field.name)?;
                if matches!(self.cx.ty_kind(*ty), TyKind::Enum(_)) {
                    Some(field.name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn ctor_enum_id(
        &self,
        name: &str,
        expected: Option<Ty>,
        span: Span,
    ) -> Result<EnumId, CheckError> {
        if let Some(ty) = expected {
            if let TyKind::Enum(id) = self.cx.ty_kind(ty) {
                if self.cx.name_str(self.cx.enum_def(*id).name) == name {
                    return Ok(*id);
                }
            }
        }
        if let Some(&ty) = self.types.get(name) {
            if let TyKind::Enum(id) = self.cx.ty_kind(ty) {
                return Ok(*id);
            }
        }
        if self.enum_temps.contains_key(name) {
            return Err(CheckError::new(
                self.path,
                span,
                format!("`{name}` needs an expected type"),
            ));
        }
        Err(CheckError::new(
            self.path,
            span,
            format!("`{name}` is not an enum"),
        ))
    }

    fn find_variant(
        &self,
        id: EnumId,
        name: &str,
        span: Span,
    ) -> Result<(VariantId, Option<Ty>), CheckError> {
        let def = self.cx.enum_def(id);
        for (i, v) in def.variants.iter().enumerate() {
            if self.cx.name_str(v.name) == name {
                return Ok((
                    VariantId {
                        enum_id: id,
                        index: i as u32,
                    },
                    v.payload,
                ));
            }
        }
        Err(CheckError::new(
            self.path,
            span,
            format!("unknown variant `{name}`"),
        ))
    }

    fn construct_variant(
        &mut self,
        span: Span,
        enum_name: &str,
        variant: &str,
        payload: Option<&Expr>,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let id = self.ctor_enum_id(enum_name, expected, span)?;
        let (vid, pty) = self.find_variant(id, variant, span)?;
        let payload_op = match (pty, payload) {
            (None, None) => None,
            (Some(ty), Some(e)) => Some(self.operand(e, Some(ty))?),
            (Some(_), None) => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("`{enum_name}.{variant}` needs a payload"),
                ));
            }
            (None, Some(_)) => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    format!("`{enum_name}.{variant}` has no payload"),
                ));
            }
        };
        let ty = self.cx.ty_enum(id);
        if let Some(want) = expected {
            if ty != want {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "enum type does not match expected type",
                ));
            }
        }
        Ok(Rvalue::enum_new(span, ty, vid, payload_op))
    }

    fn enum_id_of(&self, ty: Ty, span: Span, what: &str) -> Result<EnumId, CheckError> {
        match self.cx.ty_kind(ty) {
            TyKind::Enum(id) => Ok(*id),
            _ => Err(CheckError::new(
                self.path,
                span,
                format!("{what} on non-enum"),
            )),
        }
    }

    fn materialize(&mut self, op: Operand, span: Span) -> Place {
        if let Operand::Copy(p) = op {
            return p;
        }
        let n = self.fresh("t");
        let id = self.b.tmp(n, op.ty(), span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), Rvalue::use_op(span, op));
        place
    }

    fn bind_bool(&mut self, span: Span, rv: Rvalue) -> Operand {
        let n = self.fresh("c");
        let id = self.b.tmp(n, self.cx.common.bool, span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), rv);
        Operand::Copy(place)
    }

    fn eq_ops(&mut self, span: Span, lhs: Operand, rhs: Operand) -> Operand {
        self.bind_bool(
            span,
            Rvalue::binary(span, self.cx.common.bool, BinOp::Eq, lhs, rhs),
        )
    }

    fn and_ops(&mut self, span: Span, lhs: Operand, rhs: Operand) -> Operand {
        self.bind_bool(
            span,
            Rvalue::binary(span, self.cx.common.bool, BinOp::And, lhs, rhs),
        )
    }

    fn is_variant(&mut self, place: &Place, variant: VariantId, span: Span) -> Operand {
        if self.cx.enum_def(variant.enum_id).is_c_like() {
            let ty = place.ty;
            let expected = Rvalue::enum_new(span, ty, variant, None);
            let n = self.fresh("ev");
            let id = self.b.tmp(n, ty, span);
            let tmp = self.b.local_place(id, span);
            self.b.assign(span, tmp.clone(), expected);
            self.eq_ops(span, Operand::Copy(place.clone()), Operand::Copy(tmp))
        } else {
            let tag = self.b.enum_tag(self.cx, place.clone(), span);
            self.eq_ops(
                span,
                Operand::Copy(tag),
                Operand::int(span, self.cx.common.i32, variant.index as u64),
            )
        }
    }

    fn payload_place(&self, place: &Place, variant: VariantId, ty: Ty, span: Span) -> Place {
        self.b.enum_payload(place.clone(), variant, ty, span)
    }

    fn variant_from_path(
        &self,
        enum_id: EnumId,
        path: &[Ident],
        span: Span,
    ) -> Result<(VariantId, Option<Ty>), CheckError> {
        let ename = self.cx.name_str(self.cx.enum_def(enum_id).name).to_string();
        let vname = match path.len() {
            2 => {
                if path[0].name != ename {
                    return Err(CheckError::new(
                        self.path,
                        path[0].span,
                        format!("expected `{ename}`, got `{}`", path[0].name),
                    ));
                }
                &path[1].name
            }
            _ => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "match arm must be `Type.Variant`",
                ));
            }
        };
        self.find_variant(enum_id, vname, span)
    }

    fn check_exhaustive(
        &self,
        enum_id: EnumId,
        arms: &[MatchArm],
        span: Span,
    ) -> Result<(), CheckError> {
        let n = self.cx.enum_def(enum_id).variants.len();
        let mut full = vec![false; n];
        let mut nested: Vec<HashSet<u32>> = vec![HashSet::new(); n];
        let mut nested_enum: Vec<Option<EnumId>> = vec![None; n];
        for (i, v) in self.cx.enum_def(enum_id).variants.iter().enumerate() {
            if let Some(pty) = v.payload {
                if let TyKind::Enum(id) = self.cx.ty_kind(pty) {
                    nested_enum[i] = Some(*id);
                }
            }
        }
        for arm in arms {
            let (vid, payload_ty) = self.variant_from_path(enum_id, &arm.path, arm.span)?;
            let i = vid.index as usize;
            match &arm.payload {
                None => {
                    if payload_ty.is_some() {
                        return Err(CheckError::new(
                            self.path,
                            arm.span,
                            "match arm is missing a payload",
                        ));
                    }
                    full[i] = true;
                }
                Some(Pat::Wild) | Some(Pat::Bind(_)) => {
                    if payload_ty.is_none() {
                        return Err(CheckError::new(
                            self.path,
                            arm.span,
                            "match arm has an unexpected payload",
                        ));
                    }
                    full[i] = true;
                }
                Some(Pat::Variant { path, .. }) => {
                    let pty = match payload_ty {
                        Some(t) => t,
                        None => {
                            return Err(CheckError::new(
                                self.path,
                                arm.span,
                                "match arm has an unexpected payload",
                            ));
                        }
                    };
                    let nid = self.enum_id_of(pty, arm.span, "nested match")?;
                    let (nv, _) = self.variant_from_path(nid, path, arm.span)?;
                    nested[i].insert(nv.index);
                }
            }
        }
        for i in 0..n {
            if full[i] {
                continue;
            }
            if let Some(eid) = nested_enum[i] {
                let need = self.cx.enum_def(eid).variants.len();
                if nested[i].len() == need {
                    continue;
                }
            }
            return Err(CheckError::new(self.path, span, "match is not exhaustive"));
        }
        Ok(())
    }

    fn arm_cond(
        &mut self,
        scrut: &Place,
        enum_id: EnumId,
        arm: &MatchArm,
    ) -> Result<Operand, CheckError> {
        let (vid, payload_ty) = self.variant_from_path(enum_id, &arm.path, arm.span)?;
        let outer = self.is_variant(scrut, vid, arm.span);
        match &arm.payload {
            Some(Pat::Variant { path, .. }) => {
                let pty = match payload_ty {
                    Some(t) => t,
                    None => {
                        return Err(CheckError::new(
                            self.path,
                            arm.span,
                            "match arm has an unexpected payload",
                        ));
                    }
                };
                let nid = self.enum_id_of(pty, arm.span, "nested match")?;
                let (nv, _) = self.variant_from_path(nid, path, arm.span)?;
                let inner_place = self.payload_place(scrut, vid, pty, arm.span);
                let inner = self.is_variant(&inner_place, nv, arm.span);
                Ok(self.and_ops(arm.span, outer, inner))
            }
            _ => Ok(outer),
        }
    }

    fn lower_match_arm(
        &mut self,
        scrut: &Place,
        enum_id: EnumId,
        arm: &MatchArm,
    ) -> Result<crate::ir::body::Block, CheckError> {
        let saved_locals = self.locals.clone();
        let saved_aliases = self.aliases.clone();
        let saved = self.b.begin_block();
        if let Some(Pat::Bind(n)) = &arm.payload {
            if self.locals.contains_key(&n.name) || self.aliases.contains_key(&n.name) {
                return Err(CheckError::new(
                    self.path,
                    n.span,
                    format!("shadowing `{}` is a compile error", n.name),
                ));
            }
            let (vid, payload_ty) = self.variant_from_path(enum_id, &arm.path, arm.span)?;
            let pty = match payload_ty {
                Some(t) => t,
                None => {
                    return Err(CheckError::new(
                        self.path,
                        arm.span,
                        "match arm has an unexpected payload",
                    ));
                }
            };
            let src = self.payload_place(scrut, vid, pty, n.span);
            let id = self.b.let_local(self.cx.intern_name(&n.name), pty, n.span);
            let dest = self.b.local_place(id, n.span);
            self.b
                .assign(n.span, dest, Rvalue::use_op(n.span, Operand::Copy(src)));
            self.locals.insert(n.name.clone(), id);
        }
        for stmt in &arm.body.stmts {
            self.stmt(stmt)?;
        }
        let block = self.b.end_block(saved);
        self.locals = saved_locals;
        self.aliases = saved_aliases;
        Ok(block)
    }

    fn lower_match(&mut self, s: &parse::MatchStmt) -> Result<(), CheckError> {
        let op = self.operand(&s.expr, None)?;
        let enum_id = self.enum_id_of(op.ty(), s.span, "match")?;
        self.check_exhaustive(enum_id, &s.arms, s.span)?;
        let scrut = self.materialize(op, s.span);
        let mut else_block = None;
        for arm in s.arms.iter().rev() {
            let then_block = self.lower_match_arm(&scrut, enum_id, arm)?;
            let cond = self.arm_cond(&scrut, enum_id, arm)?;
            let saved = self.b.begin_block();
            self.b.push(crate::ir::stmt::Stmt {
                ty: self.cx.common.void,
                span: arm.span,
                kind: crate::ir::stmt::StmtKind::If {
                    cond,
                    then_block,
                    else_block,
                },
            });
            else_block = Some(self.b.end_block(saved));
        }
        if let Some(block) = else_block {
            for stmt in block.stmts {
                self.b.push(stmt);
            }
        }
        Ok(())
    }

    fn question(&mut self, span: Span, inner: &Expr) -> Result<Rvalue, CheckError> {
        let op = self.operand(inner, None)?;
        let id = self.enum_id_of(op.ty(), span, "`?`")?;
        let tc = match self.cx.enum_def(id).try_catch {
            Some(tc) => tc,
            None => {
                return Err(CheckError::new(self.path, span, "`?` needs `#[TryCatch]`"));
            }
        };
        if self.ret == self.cx.common.void {
            return Err(CheckError::new(self.path, span, "`?` in a void function"));
        }
        let ret_id = self.enum_id_of(self.ret, span, "enclosing `?` return")?;
        let ret_tc = match self.cx.enum_def(ret_id).try_catch {
            Some(tc) => tc,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "enclosing fn must return a `#[TryCatch]` enum",
                ));
            }
        };
        let success_ty = self.cx.enum_def(id).variants[tc.success.index as usize].payload;
        let fail_ty = self.cx.enum_def(id).variants[tc.failure.index as usize].payload;
        let ret_fail_ty = self.cx.enum_def(ret_id).variants[ret_tc.failure.index as usize].payload;
        if fail_ty != ret_fail_ty {
            return Err(CheckError::new(
                self.path,
                span,
                "failure payloads must match",
            ));
        }
        let value_ty = success_ty.unwrap_or(self.cx.common.void);
        let scrut = self.materialize(op, span);
        let n = self.fresh("q");
        let rid = self.b.tmp(n, value_ty, span);
        let rplace = self.b.local_place(rid, span);
        let saved = self.b.begin_block();
        if let Some(pty) = success_ty {
            let src = self.payload_place(&scrut, tc.success, pty, span);
            self.b.assign(
                span,
                rplace.clone(),
                Rvalue::use_op(span, Operand::Copy(src)),
            );
        }
        let then_block = self.b.end_block(saved);
        let saved = self.b.begin_block();
        let ret_op = if id == ret_id {
            Operand::Copy(scrut.clone())
        } else {
            match fail_ty {
                Some(pty) => {
                    let src = self.payload_place(&scrut, tc.failure, pty, span);
                    let rv =
                        Rvalue::enum_new(span, self.ret, ret_tc.failure, Some(Operand::Copy(src)));
                    Operand::Copy(self.materialize_rv(rv, span))
                }
                None => {
                    let rv = Rvalue::enum_new(span, self.ret, ret_tc.failure, None);
                    Operand::Copy(self.materialize_rv(rv, span))
                }
            }
        };
        self.b.return_op(span, Some(ret_op));
        let else_block = self.b.end_block(saved);
        let cond = self.is_variant(&scrut, tc.success, span);
        self.b.push(crate::ir::stmt::Stmt {
            ty: self.cx.common.void,
            span,
            kind: crate::ir::stmt::StmtKind::If {
                cond,
                then_block,
                else_block: Some(else_block),
            },
        });
        Ok(Rvalue::use_op(span, Operand::Copy(rplace)))
    }

    fn materialize_rv(&mut self, rv: Rvalue, span: Span) -> Place {
        let n = self.fresh("t");
        let id = self.b.tmp(n, rv.ty, span);
        let place = self.b.local_place(id, span);
        self.b.assign(span, place.clone(), rv);
        place
    }
}

fn embed_alias_names(cx: &IrCx, sid: StructId) -> Vec<String> {
    let mut out = Vec::new();
    for f in &cx.struct_def(sid).fields {
        out.push(cx.name_str(f.name).to_string());
        if f.embedded {
            if let TyKind::Struct(id) = cx.ty_kind(f.ty) {
                out.extend(embed_alias_names(cx, *id));
            }
        }
    }
    out
}

/// `10` / `10.0` with no name or call. Other sides of `+` can lend these a type.
fn bare_num(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Int(_) | ExprKind::Float(_) => true,
        ExprKind::Unary { op, expr: inner } if *op == parse::UnaryOp::Neg => bare_num(inner),
        _ => false,
    }
}

fn ast_binop(op: parse::BinOp) -> BinOp {
    match op {
        parse::BinOp::Add => BinOp::Add,
        parse::BinOp::Sub => BinOp::Sub,
        parse::BinOp::Mul => BinOp::Mul,
        parse::BinOp::Div => BinOp::Div,
        parse::BinOp::Eq => BinOp::Eq,
        parse::BinOp::Ne => BinOp::Ne,
        parse::BinOp::Lt => BinOp::Lt,
        parse::BinOp::Le => BinOp::Le,
        parse::BinOp::Gt => BinOp::Gt,
        parse::BinOp::Ge => BinOp::Ge,
        parse::BinOp::And => BinOp::And,
        parse::BinOp::Or => BinOp::Or,
    }
}
