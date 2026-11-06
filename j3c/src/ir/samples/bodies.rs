//! Sample builders: struct allocation and the small hand-built fn bodies.

use super::*;

pub(super) fn alloc_struct(cx: &mut IrCx, module: &[Name], name: &str, debug: bool) -> StructId {
    let name = cx.intern_name(name);
    cx.alloc_struct(StructDef {
        module: module.to_vec(),
        name,
        doc: String::new(),
        fields: Vec::new(),
        packed: false,
        debug,
        span: Span::dummy(),
        ty_args: Vec::new(),
        script: None,
    })
}

pub(super) fn scale_body(
    cx: &mut IrCx,
    vec2_ref: Ty,
    f32: Ty,
    x_field: FieldId,
    y_field: FieldId,
    span: Span,
) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let this = b.param(cx.intern_name("this"), vec2_ref, span);
    let s = b.param(cx.intern_name("s"), f32, span);

    let this_p = b.local_place(this, span);
    let s_p = b.local_place(s, span);
    let vec = b.deref(cx, this_p, span);
    let x = b.field(cx, vec.clone(), x_field, span);
    let y = b.field(cx, vec, y_field, span);

    b.assign_op(span, x, BinOp::Mul, Operand::Copy(s_p.clone()));
    b.assign_op(span, y, BinOp::Mul, Operand::Copy(s_p));
    b.finish()
}

pub(super) fn bump_body(cx: &mut IrCx, slice_int: Ty, int_ty: Ty, span: Span) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let xs = b.param(cx.intern_name("xs"), slice_int, span);
    let i = b.param(cx.intern_name("i"), int_ty, span);

    let xs_p = b.local_place(xs, span);
    let i_p = b.local_place(i, span);
    let elem = b.index(cx, xs_p, Operand::Copy(i_p), span);
    b.assign_op(span, elem, BinOp::Add, Operand::int(span, int_ty, 1));
    b.finish()
}

pub(super) fn tick_body(
    cx: &mut IrCx,
    game_ref: Ty,
    mem_field: FieldId,
    running_field: FieldId,
    checkpoint_id: FuncId,
    checkpoint_ty: Ty,
    int_ty: Ty,
    bool_ty: Ty,
    span: Span,
) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let g = b.param(cx.intern_name("g"), game_ref, span);

    let g_p = b.local_place(g, span);
    let game = b.deref(cx, g_p, span);
    let mem_ptr = b.field(cx, game.clone(), mem_field, span);
    // `g.mem` is `&Arena`. Deref to the header, then take `&` for the method.
    let header = b.deref(cx, mem_ptr, span);
    let arena_ref_name = cx.intern_name("arena_ref");
    let this_arg = b.ref_tmp(cx, arena_ref_name, header, span);

    let call = Rvalue::call(
        span,
        int_ty,
        Operand::fn_addr(span, checkpoint_ty, checkpoint_id),
        vec![this_arg],
    );
    let frame = b.tmp(cx.intern_name("frame"), int_ty, span);
    b.assign(span, b.local_place(frame, span), call);

    let running = b.field(cx, game, running_field, span);
    let false_val = Operand::Const(crate::ir::operand::Const {
        ty: bool_ty,
        span,
        kind: crate::ir::operand::ConstKind::Bool(false),
    });
    b.assign(span, running, Rvalue::use_op(span, false_val));
    b.finish()
}

pub(super) fn checkpoint_body(cx: &mut IrCx, arena_ref: Ty, used_field: FieldId, span: Span) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let this = b.param(cx.intern_name("this"), arena_ref, span);
    let header = b.deref(cx, b.local_place(this, span), span);
    let used = b.field(cx, header, used_field, span);
    b.return_op(span, Some(Operand::Copy(used)));
    b.finish()
}

pub(super) fn exit_block(cx: &IrCx, span: Span, code: u64) -> Block {
    let nr = Operand::int(span, cx.common.int, 60);
    let arg = Operand::int(span, cx.common.int, code);
    let call = Rvalue::syscall(span, cx.common.int, nr, vec![arg]);
    Block {
        stmts: vec![Stmt::expr(span, cx.common.void, call)],
    }
}

pub(super) fn fail_if_ne(
    b: &mut BodyBuilder,
    cx: &mut IrCx,
    span: Span,
    lhs: Operand,
    rhs: Operand,
    tmp_name: &str,
) {
    let name = cx.intern_name(tmp_name);
    let tmp = b.let_local(name, cx.common.bool, span);
    b.assign(
        span,
        b.local_place(tmp, span),
        Rvalue::binary(span, cx.common.bool, BinOp::Ne, lhs, rhs),
    );
    b.if_then(
        span,
        Operand::Copy(b.local_place(tmp, span)),
        exit_block(cx, span, 1),
    );
}

