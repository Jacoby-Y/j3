//! Hand-built IR that locks in places. Parser does not exist yet.
//!
//! Covers:
//! - `this.x *= s` through `&Vec2` (auto-deref is `Proj::Deref`)
//! - `xs[i] += 1` binds the index once
//! - `g.mem.checkpoint()` refs the Arena header, it does not copy it

use crate::intern::Name;
use crate::ir::body::{Block, Body};
use crate::ir::build::BodyBuilder;
use crate::ir::cx::IrCx;
use crate::ir::entity::{FieldDef, Func, FuncKind, StructDef, Vis};
use crate::ir::ids::{FieldId, FuncId, StructId};
use crate::ir::module::Module;
use crate::ir::op::BinOp;
use crate::ir::operand::Operand;
use crate::ir::rvalue::Rvalue;
use crate::ir::stmt::Stmt;
use crate::ir::ty::Ty;
use crate::span::Span;

pub struct PlaceSamples {
    pub cx: IrCx,
    pub module: Module,
}

pub fn place_samples() -> PlaceSamples {
    let mut cx = IrCx::new();
    let span = Span::dummy();
    let module_path: Vec<Name> = vec![cx.intern_name("ir"), cx.intern_name("samples")];

    let vec2_id = alloc_struct(&mut cx, &module_path, "Vec2", true);
    let x_name = cx.intern_name("x");
    let y_name = cx.intern_name("y");
    let f32 = cx.common.f32;
    cx.struct_def_mut(vec2_id).fields = vec![
        FieldDef {
            doc: String::new(),
            name: x_name,
            ty: f32,
            embedded: false,
            default: None,
        },
        FieldDef {
            doc: String::new(),
            name: y_name,
            ty: f32,
            embedded: false,
            default: None,
        },
    ];
    let vec2_ty = cx.ty_struct(vec2_id);
    let vec2_ref = cx.ty_ref(vec2_ty);
    let x_field = FieldId {
        struct_id: vec2_id,
        index: 0,
    };
    let y_field = FieldId {
        struct_id: vec2_id,
        index: 1,
    };

    let arena_id = alloc_struct(&mut cx, &module_path, "Arena", false);
    let ptr_name = cx.intern_name("ptr");
    let used_name = cx.intern_name("used");
    let cap_name = cx.intern_name("cap");
    let ptr_ty = cx.common.ptr;
    let int_ty = cx.common.int;
    cx.struct_def_mut(arena_id).fields = vec![
        FieldDef {
            doc: String::new(),
            name: ptr_name,
            ty: ptr_ty,
            embedded: false,
            default: None,
        },
        FieldDef {
            doc: String::new(),
            name: used_name,
            ty: int_ty,
            embedded: false,
            default: None,
        },
        FieldDef {
            doc: String::new(),
            name: cap_name,
            ty: int_ty,
            embedded: false,
            default: None,
        },
    ];
    let arena_ty = cx.ty_struct(arena_id);
    let arena_ref = cx.ty_ref(arena_ty);

    let game_id = alloc_struct(&mut cx, &module_path, "Game", false);
    let mem_name = cx.intern_name("mem");
    let running_name = cx.intern_name("running");
    let bool_ty = cx.common.bool;
    cx.struct_def_mut(game_id).fields = vec![
        FieldDef {
            doc: String::new(),
            name: mem_name,
            ty: arena_ref,
            embedded: false,
            default: None,
        },
        FieldDef {
            doc: String::new(),
            name: running_name,
            ty: bool_ty,
            embedded: false,
            default: None,
        },
    ];
    let game_ty = cx.ty_struct(game_id);
    let game_ref = cx.ty_ref(game_ty);
    let mem_field = FieldId {
        struct_id: game_id,
        index: 0,
    };
    let running_field = FieldId {
        struct_id: game_id,
        index: 1,
    };

    let checkpoint_ty = cx.ty_fn(vec![arena_ref], int_ty);
    let used_field = FieldId {
        struct_id: arena_id,
        index: 1,
    };
    let checkpoint_body = checkpoint_body(&mut cx, arena_ref, used_field, span);
    let checkpoint_name = cx.intern_name("checkpoint");
    let checkpoint_id = cx.alloc_func(Func {
        doc: String::new(),
        id: FuncId(0),
        module: module_path.clone(),
        name: checkpoint_name,
        ty: checkpoint_ty,
        vis: Vis::Pub,
        method_of: Some(arena_id),
        kind: FuncKind::J3 {
            body: checkpoint_body,
        },
        span,
    });

    let void = cx.common.void;
    let scale_ty = cx.ty_fn(vec![vec2_ref, f32], void);
    let scale_body = scale_body(&mut cx, vec2_ref, f32, x_field, y_field, span);
    let scale_name = cx.intern_name("scale");
    let scale_id = cx.alloc_func(Func {
        doc: String::new(),
        id: FuncId(0),
        module: module_path.clone(),
        name: scale_name,
        ty: scale_ty,
        vis: Vis::Private,
        method_of: Some(vec2_id),
        kind: FuncKind::J3 { body: scale_body },
        span,
    });

    let slice_int = cx.ty_slice(int_ty);
    let bump_ty = cx.ty_fn(vec![slice_int, int_ty], void);
    let bump_body = bump_body(&mut cx, slice_int, int_ty, span);
    let bump_name = cx.intern_name("bump");
    let bump_id = cx.alloc_func(Func {
        doc: String::new(),
        id: FuncId(0),
        module: module_path.clone(),
        name: bump_name,
        ty: bump_ty,
        vis: Vis::Private,
        method_of: None,
        kind: FuncKind::J3 { body: bump_body },
        span,
    });

    let tick_ty = cx.ty_fn(vec![game_ref], void);
    let tick_body = tick_body(
        &mut cx,
        game_ref,
        mem_field,
        running_field,
        checkpoint_id,
        checkpoint_ty,
        int_ty,
        bool_ty,
        span,
    );
    let tick_name = cx.intern_name("tick");
    let tick_id = cx.alloc_func(Func {
        doc: String::new(),
        id: FuncId(0),
        module: module_path.clone(),
        name: tick_name,
        ty: tick_ty,
        vis: Vis::Private,
        method_of: None,
        kind: FuncKind::J3 { body: tick_body },
        span,
    });

    let main_ty = cx.ty_fn(Vec::new(), void);
    let main_body = main_body(
        &mut cx,
        arena_ty,
        game_ty,
        vec2_ty,
        slice_int,
        int_ty,
        f32,
        bool_ty,
        running_field,
        x_field,
        y_field,
        scale_id,
        scale_ty,
        bump_id,
        bump_ty,
        tick_id,
        tick_ty,
        span,
    );
    let main_name = cx.intern_name("main");
    let main_id = cx.alloc_func(Func {
        doc: String::new(),
        id: FuncId(0),
        module: module_path.clone(),
        name: main_name,
        ty: main_ty,
        vis: Vis::Pub,
        method_of: None,
        kind: FuncKind::J3 { body: main_body },
        span,
    });

    let mut module = Module::new(module_path);
    module.structs.push(vec2_id);
    module.structs.push(arena_id);
    module.structs.push(game_id);
    module.funcs.push(checkpoint_id);
    module.funcs.push(scale_id);
    module.funcs.push(bump_id);
    module.funcs.push(tick_id);
    module.funcs.push(main_id);

    PlaceSamples { cx, module }
}

fn alloc_struct(cx: &mut IrCx, module: &[Name], name: &str, debug: bool) -> StructId {
    let name = cx.intern_name(name);
    cx.alloc_struct(StructDef {
        module: module.to_vec(),
        name,
        doc: String::new(),
        fields: Vec::new(),
        packed: false,
        debug,
        span: Span::dummy(),
    })
}

fn scale_body(
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

fn bump_body(cx: &mut IrCx, slice_int: Ty, int_ty: Ty, span: Span) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let xs = b.param(cx.intern_name("xs"), slice_int, span);
    let i = b.param(cx.intern_name("i"), int_ty, span);

    let xs_p = b.local_place(xs, span);
    let i_p = b.local_place(i, span);
    let elem = b.index(cx, xs_p, Operand::Copy(i_p), span);
    b.assign_op(span, elem, BinOp::Add, Operand::int(span, int_ty, 1));
    b.finish()
}

fn tick_body(
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

fn checkpoint_body(cx: &mut IrCx, arena_ref: Ty, used_field: FieldId, span: Span) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let this = b.param(cx.intern_name("this"), arena_ref, span);
    let header = b.deref(cx, b.local_place(this, span), span);
    let used = b.field(cx, header, used_field, span);
    b.return_op(span, Some(Operand::Copy(used)));
    b.finish()
}

fn exit_block(cx: &IrCx, span: Span, code: u64) -> Block {
    let nr = Operand::int(span, cx.common.int, 60);
    let arg = Operand::int(span, cx.common.int, code);
    let call = Rvalue::syscall(span, cx.common.int, nr, vec![arg]);
    Block {
        stmts: vec![Stmt::expr(span, cx.common.void, call)],
    }
}

fn fail_if_ne(
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

fn main_body(
    cx: &mut IrCx,
    arena_ty: Ty,
    game_ty: Ty,
    vec2_ty: Ty,
    slice_int: Ty,
    int_ty: Ty,
    f32: Ty,
    bool_ty: Ty,
    running_field: FieldId,
    x_field: FieldId,
    y_field: FieldId,
    scale_id: FuncId,
    scale_ty: Ty,
    bump_id: FuncId,
    bump_ty: Ty,
    tick_id: FuncId,
    tick_ty: Ty,
    span: Span,
) -> Body {
    let n_arena = cx.intern_name("arena");
    let n_g = cx.intern_name("g");
    let n_v = cx.intern_name("v");
    let n_buf = cx.intern_name("buf");
    let n_xs = cx.intern_name("xs");
    let n_arena_ref = cx.intern_name("arena_p");
    let n_g_ref = cx.intern_name("g_p");
    let n_v_ref = cx.intern_name("v_p");
    let n_buf0 = cx.intern_name("buf0");
    let ptr_ty = cx.common.ptr;
    let arr_ty = cx.ty_array(int_ty, 4);

    let mut b = BodyBuilder::new(cx.common.void);

    let arena = b.let_local(n_arena, arena_ty, span);
    b.assign(
        span,
        b.local_place(arena, span),
        Rvalue::aggregate(
            span,
            arena_ty,
            vec![
                Operand::null(span, ptr_ty),
                Operand::int(span, int_ty, 7),
                Operand::int(span, int_ty, 9),
            ],
        ),
    );

    let arena_p = b.ref_tmp(cx, n_arena_ref, b.local_place(arena, span), span);
    let g = b.let_local(n_g, game_ty, span);
    b.assign(
        span,
        b.local_place(g, span),
        Rvalue::aggregate(
            span,
            game_ty,
            vec![arena_p, Operand::bool_const(span, bool_ty, true)],
        ),
    );

    let g_arg = b.ref_tmp(cx, n_g_ref, b.local_place(g, span), span);
    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, tick_ty, tick_id),
            vec![g_arg],
        ),
    );

    let g_place = b.local_place(g, span);
    let running = b.field(cx, g_place, running_field, span);
    b.if_then(span, Operand::Copy(running), exit_block(cx, span, 1));

    let v = b.let_local(n_v, vec2_ty, span);
    b.assign(
        span,
        b.local_place(v, span),
        Rvalue::aggregate(
            span,
            vec2_ty,
            vec![
                Operand::f32_const(span, f32, 10.0),
                Operand::f32_const(span, f32, 20.0),
            ],
        ),
    );
    let v_arg = b.ref_tmp(cx, n_v_ref, b.local_place(v, span), span);
    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, scale_ty, scale_id),
            vec![v_arg, Operand::f32_const(span, f32, 2.0)],
        ),
    );

    let v_place = b.local_place(v, span);
    let vx = b.field(cx, v_place.clone(), x_field, span);
    let vy = b.field(cx, v_place, y_field, span);
    fail_if_ne(
        &mut b,
        cx,
        span,
        Operand::Copy(vx),
        Operand::f32_const(span, f32, 20.0),
        "bad_x",
    );
    fail_if_ne(
        &mut b,
        cx,
        span,
        Operand::Copy(vy),
        Operand::f32_const(span, f32, 40.0),
        "bad_y",
    );

    let buf = b.let_local(n_buf, arr_ty, span);
    for i in 0..4 {
        let idx = Operand::int(span, int_ty, i);
        let elem = b.index(cx, b.local_place(buf, span), idx, span);
        b.assign(
            span,
            elem,
            Rvalue::use_op(span, Operand::int(span, int_ty, 0)),
        );
    }
    let buf0_place = b.index(
        cx,
        b.local_place(buf, span),
        Operand::int(span, int_ty, 0),
        span,
    );
    let buf0 = b.ref_tmp(cx, n_buf0, buf0_place, span);
    let xs = b.let_local(n_xs, slice_int, span);
    b.assign(
        span,
        b.local_place(xs, span),
        Rvalue::aggregate(span, slice_int, vec![buf0, Operand::int(span, int_ty, 4)]),
    );
    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, bump_ty, bump_id),
            vec![
                Operand::Copy(b.local_place(xs, span)),
                Operand::int(span, int_ty, 0),
            ],
        ),
    );
    let buf0_after = b.index(
        cx,
        b.local_place(buf, span),
        Operand::int(span, int_ty, 0),
        span,
    );
    fail_if_ne(
        &mut b,
        cx,
        span,
        Operand::Copy(buf0_after),
        Operand::int(span, int_ty, 1),
        "bad_buf",
    );

    b.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::dump::dump_module;
    use crate::ir::validate::validate_module;

    #[test]
    fn place_samples_validate() {
        let samples = place_samples();
        match validate_module(&samples.cx, &samples.module) {
            Ok(()) => {}
            Err(e) => panic!("{}", e.message),
        }
    }

    #[test]
    fn place_samples_dump_mentions_deref_and_assign_op() {
        let samples = place_samples();
        let dump = dump_module(&samples.cx, &samples.module);
        assert!(dump.contains("(*this).x *= copy s"), "{dump}");
        assert!(dump.contains("xs[copy i] += 1"), "{dump}");
        assert!(dump.contains("&(*(*g).mem)"), "{dump}");
        assert!(dump.contains("(*g).running = false"), "{dump}");
    }

    #[test]
    fn emit_c_compiles_and_run_exits_zero() {
        let samples = place_samples();
        match validate_module(&samples.cx, &samples.module) {
            Ok(()) => {}
            Err(e) => panic!("{}", e.message),
        }
        let c = crate::emit::emit_c(&samples.cx, &samples.module).expect("emit c");
        assert!(c.contains("j3_ir_samples_scale_"), "{c}");
        assert!(c.contains("j3_sl_i64"), "{c}");
        assert!(c.contains("j3_ir_samples_Vec2"), "{c}");
        assert!(c.contains("int main(void)"), "{c}");
        let run = crate::emit::compile_and_run(&c).expect("cc");
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );

        let mut opts = crate::emit::EmitOpts::default();
        opts.backend = crate::emit::Backend::Llvm;
        if !crate::emit::clang_available(&opts.clang) {
            opts.backend = crate::emit::Backend::C;
        }
        let ll_run = crate::emit::run_program(&samples.cx, std::slice::from_ref(&samples.module), &opts)
            .expect("llvm run");
        assert_eq!(
            ll_run.status,
            0,
            "llvm stderr {}",
            String::from_utf8_lossy(&ll_run.stderr)
        );
    }
}
