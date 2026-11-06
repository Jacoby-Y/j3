//! Dev lowering helpers: the `Fb` body builder and the IR snippets the rewrite emits.

use super::*;

/// A tiny function-body builder: locals, then statements.
pub(super) struct Fb {
    pub(super) locals: Vec<LocalDecl>,
    pub(super) params: Vec<LocalId>,
    pub(super) stmts: Vec<Stmt>,
    pub(super) void: Ty,
    pub(super) next: u32,
}

impl Fb {
    pub(super) fn new(cx: &IrCx) -> Fb {
        Fb {
            locals: Vec::new(),
            params: Vec::new(),
            stmts: Vec::new(),
            void: cx.common.void,
            next: 0,
        }
    }

    pub(super) fn with_locals(cx: &IrCx, body: &Body) -> Fb {
        let mut fb = Fb::new(cx);
        fb.locals = body.locals.clone();
        fb.next = body.locals.len() as u32;
        fb
    }

    pub(super) fn local(&mut self, name: Name, ty: Ty, kind: LocalKind) -> Place {
        let id = LocalId(self.locals.len() as u32);
        self.locals.push(LocalDecl {
            id,
            doc: String::new(),
            name,
            ty,
            access: Access::let_binding(),
            kind,
            span: Span::dummy(),
        });
        if kind == LocalKind::Param {
            self.params.push(id);
        }
        self.next += 1;
        Place::local(id, ty, Access::let_binding(), Span::dummy())
    }

    pub(super) fn tmp(&mut self, cx: &mut IrCx, ty: Ty) -> Place {
        let name = cx.intern_name(&format!("_dev{}", self.next));
        self.local(name, ty, LocalKind::Tmp)
    }

    pub(super) fn assign(&mut self, place: Place, rvalue: Rvalue) {
        self.stmts.push(Stmt::assign(Span::dummy(), self.void, place, rvalue));
    }

    pub(super) fn expr(&mut self, rvalue: Rvalue) {
        self.stmts.push(Stmt::expr(Span::dummy(), self.void, rvalue));
    }

    /// Store `rvalue` in a new temp and return a copy of it.
    pub(super) fn value(&mut self, cx: &mut IrCx, rvalue: Rvalue) -> Operand {
        let place = self.tmp(cx, rvalue.ty);
        self.assign(place.clone(), rvalue);
        Operand::Copy(place)
    }

    /// `place as ptr`, via `&place`.
    pub(super) fn address(&mut self, cx: &mut IrCx, place: Place) -> Operand {
        let ref_ty = cx.ty_ref(place.ty);
        let r = self.value(cx, Rvalue::ref_place(Span::dummy(), ref_ty, place));
        self.value(cx, Rvalue::cast(Span::dummy(), cx.common.ptr, r))
    }

    pub(super) fn finish(self) -> Body {
        Body {
            locals: self.locals,
            params: self.params,
            block: Block { stmts: self.stmts },
        }
    }
}

pub(super) fn deref(cx: &IrCx, place: Place) -> Place {
    let inner = cx.ref_inner(place.ty).expect("deref of a non-ref");
    place.project(Proj::Deref, inner, Access::through_ref(), Span::dummy())
}

pub(super) fn handle_of(cx: &IrCx, script_place: Place, sid: StructId) -> Place {
    let ptr = cx.common.ptr;
    script_place.project(
        Proj::Field {
            field: FieldId {
                struct_id: sid,
                index: 0,
            },
            ty: ptr,
        },
        ptr,
        Access::read_only_view(),
        Span::dummy(),
    )
}

pub(super) fn string_const(cx: &IrCx, text: &str) -> Operand {
    Operand::bytes(Span::dummy(), cx.common.string, text.as_bytes())
}

/// A `[]T` slice over `items` (each an operand of type `elem`), built in a stack array.
pub(super) fn slice_of(cx: &mut IrCx, fb: &mut Fb, elem: Ty, items: Vec<Operand>) -> Operand {
    let slice_ty = cx.ty_slice(elem);
    let ref_ty = cx.ty_ref(elem);
    let int = cx.common.int;
    let n = items.len() as u64;
    let data = if items.is_empty() {
        Operand::null(Span::dummy(), ref_ty)
    } else {
        let array_ty = cx.ty_array(elem, n);
        let array = fb.tmp(cx, array_ty);
        fb.assign(
            array.clone(),
            Rvalue::aggregate(Span::dummy(), array_ty, items),
        );
        let first = array.project(
            Proj::Index {
                index: Operand::int(Span::dummy(), int, 0),
                ty: elem,
            },
            elem,
            Access::let_binding(),
            Span::dummy(),
        );
        fb.value(cx, Rvalue::ref_place(Span::dummy(), ref_ty, first))
    };
    fb.value(
        cx,
        Rvalue::aggregate(
            Span::dummy(),
            slice_ty,
            vec![data, Operand::int(Span::dummy(), int, n)],
        ),
    )
}

/// Pass `value` (of type `ty`) to the VM: its address, or for `&Door`, the address of the
/// VM instance pointer the handle holds.
pub(super) fn vm_arg(
    cx: &mut IrCx,
    fb: &mut Fb,
    scripts: &HashMap<StructId, ScriptInfo>,
    value: Operand,
    ty: Ty,
) -> Result<(Operand, u64), String> {
    if let Some(sid) = script_ref(cx, ty, scripts) {
        let Operand::Copy(place) = value else {
            return Err("dev build: a script reference must be a place".to_string());
        };
        let handle = handle_of(cx, deref(cx, place), sid);
        let slot = fb.value(cx, Rvalue::use_op(Span::dummy(), Operand::Copy(handle)));
        let Operand::Copy(slot) = slot else { unreachable!() };
        return Ok((fb.address(cx, slot), 8));
    }
    if mentions_script(cx, ty, scripts) {
        return Err(format!(
            "dev builds pass a script instance only as `&{}` (found `{}`)",
            "Script",
            crate::ir::dump::ty_string(cx, ty)
        ));
    }
    let slot = match value {
        Operand::Copy(place) if matches!(place.root, PlaceRoot::Local(_)) && place.proj.is_empty() => {
            place
        }
        other => {
            let copied = fb.value(cx, Rvalue::use_op(Span::dummy(), other));
            let Operand::Copy(place) = copied else { unreachable!() };
            place
        }
    };
    let size = crate::ir::abi::size(cx, ty);
    Ok((fb.address(cx, slot), size))
}

/// Replace a `pub` script fn's body with a call into the VM.
pub(super) fn stub(
    cx: &mut IrCx,
    rt: &Runtime,
    scripts: &HashMap<StructId, ScriptInfo>,
    id: FuncId,
) -> Result<(), String> {
    let key = bc::fn_key(cx, id);
    let func = cx.func(id).clone();
    let FuncKind::J3 { body } = &func.kind else {
        return Ok(());
    };
    let (_, ret) = cx.fn_sig(func.ty).map(|(p, r)| (p.clone(), r)).ok_or("stub signature")?;
    if mentions_script(cx, ret, scripts) {
        return Err(format!(
            "`{}` returns a script instance; dev builds can't return those to native code yet",
            bc::fn_label(cx, id)
        ));
    }
    let mut fb = Fb::new(cx);
    let mut params = Vec::new();
    for pid in &body.params {
        let decl = body.local(*pid);
        params.push(fb.local(decl.name, decl.ty, LocalKind::Param));
    }
    let arg_ty = cx.ty_struct(rt.arg);
    let int = cx.common.int;
    let mut args = Vec::new();
    for param in params.iter().skip(1) {
        let (value, size) = vm_arg(cx, &mut fb, scripts, Operand::Copy(param.clone()), param.ty)?;
        args.push(fb.value(
            cx,
            Rvalue::aggregate(
                Span::dummy(),
                arg_ty,
                vec![value, Operand::int(Span::dummy(), int, size)],
            ),
        ));
    }
    let args = slice_of(cx, &mut fb, arg_ty, args);
    let this_sid = script_ref(cx, params[0].ty, scripts).ok_or("stub without `this`")?;
    let handle = handle_of(cx, deref(cx, params[0].clone()), this_sid);
    let handle = fb.value(cx, Rvalue::use_op(Span::dummy(), Operand::Copy(handle)));
    let void = cx.common.void;
    let (result_ptr, result) = if ret == void {
        (Operand::null(Span::dummy(), cx.common.ptr), None)
    } else {
        let slot = fb.tmp(cx, ret);
        (fb.address(cx, slot.clone()), Some(slot))
    };
    let u64_ty = cx.intern_ty(TyKind::Int(IntTy::U64));
    let callee = cx.func(rt.dev_call).ty;
    fb.expr(Rvalue::call(
        Span::dummy(),
        void,
        Operand::fn_addr(Span::dummy(), callee, rt.dev_call),
        vec![
            Operand::int(Span::dummy(), u64_ty, key),
            handle,
            args,
            result_ptr,
        ],
    ));
    fb.stmts.push(Stmt::return_value(
        Span::dummy(),
        void,
        result.map(Operand::Copy),
    ));
    let body = fb.finish();
    let f = cx.func_mut(id);
    f.kind = FuncKind::J3 { body };
    f.span = Span::dummy();
    Ok(())
}
