//! Dev lowering: the `j3s_dev` thunk per native fn a script may call.

use super::*;

/// `fn(args: ptr, ret: ptr)` that reads each argument from an 8-byte-aligned slot in
/// `args`, calls `target`, and writes the result to `ret`. Same slots `Vm.callNative` packs.
pub(super) fn thunk(cx: &mut IrCx, target: FuncId) -> Result<FuncId, String> {
    let func = cx.func(target).clone();
    let (params, ret) = cx
        .fn_sig(func.ty)
        .map(|(p, r)| (p.clone(), r))
        .ok_or("thunk target without a signature")?;
    let ptr = cx.common.ptr;
    let int = cx.common.int;
    let void = cx.common.void;
    let mut fb = Fb::new(cx);
    let args_name = cx.intern_name("args");
    let ret_name = cx.intern_name("ret");
    let args = fb.local(args_name, ptr, LocalKind::Param);
    let out = fb.local(ret_name, ptr, LocalKind::Param);
    let base = fb.value(cx, Rvalue::cast(Span::dummy(), int, Operand::Copy(args)));
    let mut call_args = Vec::new();
    let mut offset = 0u64;
    for ty in &params {
        let at = fb.value(
            cx,
            Rvalue::binary(
                Span::dummy(),
                int,
                BinOp::Add,
                base.clone(),
                Operand::int(Span::dummy(), int, offset),
            ),
        );
        let raw = fb.value(cx, Rvalue::cast(Span::dummy(), ptr, at));
        let ref_ty = cx.ty_ref(*ty);
        let typed = fb.value(cx, Rvalue::cast(Span::dummy(), ref_ty, raw));
        let Operand::Copy(typed) = typed else { unreachable!() };
        call_args.push(Operand::Copy(deref(cx, typed)));
        offset += crate::ir::abi::size(cx, *ty).div_ceil(8) * 8;
    }
    let call = Rvalue::call(
        Span::dummy(),
        ret,
        Operand::fn_addr(Span::dummy(), func.ty, target),
        call_args,
    );
    if ret == void {
        fb.expr(call);
    } else {
        let ref_ty = cx.ty_ref(ret);
        let typed = fb.value(cx, Rvalue::cast(Span::dummy(), ref_ty, Operand::Copy(out)));
        let Operand::Copy(typed) = typed else { unreachable!() };
        fb.assign(deref(cx, typed), call);
    }
    let body = fb.finish();
    let ty = cx.ty_fn(vec![ptr, ptr], void);
    let dev_name = vec![cx.intern_name(DEV_MODULE)];
    let name = format!("thunk_{:016x}", bc::fn_key(cx, target));
    Ok(add_func(cx, &dev_name, &name, ty, body))
}

pub(super) fn add_func(cx: &mut IrCx, module: &[Name], name: &str, ty: Ty, body: Body) -> FuncId {
    let name = cx.intern_name(name);
    cx.alloc_func(Func {
        id: FuncId(0),
        doc: String::new(),
        module: module.to_vec(),
        name,
        ty,
        vis: Vis::Private,
        method_of: None,
        kind: FuncKind::J3 { body },
        span: Span::dummy(),
    })
}
