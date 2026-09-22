//! J3 stdlib. Source of truth is `stdlib/j3/*.j3`.
//! This module is an IR bootstrap until the parser exists. Delete it then.

use crate::intern::Name;
use crate::ir::body::Body;
use crate::ir::build::BodyBuilder;
use crate::ir::cx::IrCx;
use crate::ir::entity::{Func, FuncKind, Vis};
use crate::ir::ids::FuncId;
use crate::ir::module::Module;
use crate::ir::operand::Operand;
use crate::ir::rvalue::Rvalue;
use crate::ir::ty::Ty;
use crate::ir::validate::validate_module;
use crate::span::Span;

const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 60;

pub struct HelloProgram {
    pub cx: IrCx,
    pub modules: Vec<Module>,
}

/// `printLn("Hello, world!")` via `j3.sys.start`.
pub fn hello_world() -> HelloProgram {
    let mut cx = IrCx::new();
    let span = Span::dummy();
    let _ = cx.ty_ref(cx.common.u8);

    let sys_path = names(&mut cx, &["j3", "sys"]);
    let io_path = names(&mut cx, &["j3", "io"]);
    let hello_path = names(&mut cx, &["hello"]);

    let void = cx.common.void;
    let main_ty = cx.ty_fn(Vec::new(), void);
    let main_id = intern_func(
        &mut cx,
        &hello_path,
        "main",
        main_ty,
        Vis::Private,
        FuncKind::Decl,
        span,
    );

    let (sys_mod, write_id) = build_sys(&mut cx, &sys_path, main_id, main_ty, span);
    let io_mod = build_io(&mut cx, &io_path, write_id, span);
    let (print_id, print_ty) = lookup_named(&cx, &io_mod, "printLn");

    let hello_main = hello_main_body(&mut cx, print_id, print_ty, span);
    cx.func_mut(main_id).kind = FuncKind::J3 { body: hello_main };

    let mut hello_mod = Module::new(hello_path);
    hello_mod.funcs.push(main_id);

    expect_ok(&cx, &sys_mod, "j3.sys");
    expect_ok(&cx, &io_mod, "j3.io");
    expect_ok(&cx, &hello_mod, "hello");

    HelloProgram {
        cx,
        modules: vec![sys_mod, io_mod, hello_mod],
    }
}

fn expect_ok(cx: &IrCx, module: &Module, label: &str) {
    match validate_module(cx, module) {
        Ok(()) => {}
        Err(e) => panic!("{label} IR: {}", e.message),
    }
}

fn names(cx: &mut IrCx, parts: &[&str]) -> Vec<Name> {
    let mut v = Vec::new();
    for p in parts {
        v.push(cx.intern_name(p));
    }
    v
}

fn intern_func(
    cx: &mut IrCx,
    module: &[Name],
    name: &str,
    ty: Ty,
    vis: Vis,
    kind: FuncKind,
    span: Span,
) -> FuncId {
    let name = cx.intern_name(name);
    cx.alloc_func(Func {
        doc: String::new(),
        id: FuncId(0),
        module: module.to_vec(),
        name,
        ty,
        vis,
        method_of: None,
        kind,
        span,
    })
}

fn lookup_named(cx: &IrCx, module: &Module, name: &str) -> (FuncId, Ty) {
    for id in &module.funcs {
        let f = cx.func(*id);
        if cx.name_str(f.name) == name {
            return (*id, f.ty);
        }
    }
    panic!("{name} missing");
}

fn build_sys(
    cx: &mut IrCx,
    module: &[Name],
    main_id: FuncId,
    main_ty: Ty,
    span: Span,
) -> (Module, FuncId) {
    let void = cx.common.void;
    let int_ty = cx.common.int;
    let i32_ty = cx.common.i32;
    let ptr_ty = cx.common.ptr;
    let string_ty = cx.common.string;

    let write_ty = cx.ty_fn(vec![int_ty, ptr_ty, int_ty], int_ty);
    let write_body = write_body(cx, int_ty, ptr_ty, span);
    let write_id = intern_func(
        cx,
        module,
        "write",
        write_ty,
        Vis::Pub,
        FuncKind::J3 { body: write_body },
        span,
    );

    let exit_ty = cx.ty_fn(vec![i32_ty], void);
    let exit_body = exit_body(cx, i32_ty, span);
    let exit_id = intern_func(
        cx,
        module,
        "exit",
        exit_ty,
        Vis::Pub,
        FuncKind::J3 { body: exit_body },
        span,
    );

    let panic_ty = cx.ty_fn(vec![string_ty], void);
    let panic_body = panic_body(cx, write_id, write_ty, exit_id, exit_ty, span);
    let panic_id = intern_func(
        cx,
        module,
        "panic",
        panic_ty,
        Vis::Pub,
        FuncKind::J3 { body: panic_body },
        span,
    );

    let start_ty = cx.ty_fn(Vec::new(), void);
    let start_body = start_body(cx, main_id, main_ty, exit_id, exit_ty, span);
    let start_id = intern_func(
        cx,
        module,
        "start",
        start_ty,
        Vis::Pub,
        FuncKind::J3 { body: start_body },
        span,
    );

    let mut m = Module::new(module.to_vec());
    m.funcs.push(write_id);
    m.funcs.push(exit_id);
    m.funcs.push(panic_id);
    m.funcs.push(start_id);
    (m, write_id)
}

fn write_body(cx: &mut IrCx, int_ty: Ty, ptr_ty: Ty, span: Span) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let n_fd = cx.intern_name("fd");
    let n_buf = cx.intern_name("buf");
    let n_n = cx.intern_name("n");
    let n_r = cx.intern_name("r");
    let fd = b.param(n_fd, int_ty, span);
    let buf = b.param(n_buf, ptr_ty, span);
    let n = b.param(n_n, int_ty, span);
    let nr = Operand::int(span, int_ty, SYS_WRITE);
    let call = Rvalue::syscall(
        span,
        int_ty,
        nr,
        vec![
            Operand::Copy(b.local_place(fd, span)),
            Operand::Copy(b.local_place(buf, span)),
            Operand::Copy(b.local_place(n, span)),
        ],
    );
    let tmp = b.tmp(n_r, int_ty, span);
    b.assign(span, b.local_place(tmp, span), call);
    b.return_op(span, Some(Operand::Copy(b.local_place(tmp, span))));
    b.finish()
}

fn exit_body(cx: &mut IrCx, i32_ty: Ty, span: Span) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    let n_code = cx.intern_name("code");
    let code = b.param(n_code, i32_ty, span);
    let int_ty = cx.common.int;
    let nr = Operand::int(span, int_ty, SYS_EXIT);
    b.expr(
        span,
        Rvalue::syscall(
            span,
            int_ty,
            nr,
            vec![Operand::Copy(b.local_place(code, span))],
        ),
    );
    b.finish()
}

fn panic_body(
    cx: &mut IrCx,
    write_id: FuncId,
    write_ty: Ty,
    exit_id: FuncId,
    exit_ty: Ty,
    span: Span,
) -> Body {
    let string_ty = cx.common.string;
    let int_ty = cx.common.int;
    let ptr_ty = cx.common.ptr;
    let i32_ty = cx.common.i32;
    let n_msg = cx.intern_name("msg");
    let n_buf = cx.intern_name("buf");
    let n_nl = cx.intern_name("nl");
    let n_nlbuf = cx.intern_name("nlbuf");
    let mut b = BodyBuilder::new(cx.common.void);
    let msg = b.param(n_msg, string_ty, span);

    let msg_p = b.local_place(msg, span);
    let ptr_place = b.slice_ptr(cx, msg_p.clone(), span);
    let len_place = b.slice_len(cx, msg_p, span);
    let buf_tmp = b.tmp(n_buf, ptr_ty, span);
    b.assign(
        span,
        b.local_place(buf_tmp, span),
        Rvalue::cast(span, ptr_ty, Operand::Copy(ptr_place)),
    );

    b.expr(
        span,
        Rvalue::call(
            span,
            int_ty,
            Operand::fn_addr(span, write_ty, write_id),
            vec![
                Operand::int(span, int_ty, 2),
                Operand::Copy(b.local_place(buf_tmp, span)),
                Operand::Copy(len_place),
            ],
        ),
    );

    let nl = b.let_local(n_nl, string_ty, span);
    b.assign(
        span,
        b.local_place(nl, span),
        Rvalue::use_op(span, Operand::bytes(span, string_ty, b"\n")),
    );
    let nl_p = b.local_place(nl, span);
    let nl_ptr = b.slice_ptr(cx, nl_p.clone(), span);
    let nl_len = b.slice_len(cx, nl_p, span);
    let nl_buf = b.tmp(n_nlbuf, ptr_ty, span);
    b.assign(
        span,
        b.local_place(nl_buf, span),
        Rvalue::cast(span, ptr_ty, Operand::Copy(nl_ptr)),
    );
    b.expr(
        span,
        Rvalue::call(
            span,
            int_ty,
            Operand::fn_addr(span, write_ty, write_id),
            vec![
                Operand::int(span, int_ty, 2),
                Operand::Copy(b.local_place(nl_buf, span)),
                Operand::Copy(nl_len),
            ],
        ),
    );

    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, exit_ty, exit_id),
            vec![Operand::int(span, i32_ty, 1)],
        ),
    );
    b.finish()
}

fn start_body(
    cx: &mut IrCx,
    main_id: FuncId,
    main_ty: Ty,
    exit_id: FuncId,
    exit_ty: Ty,
    span: Span,
) -> Body {
    let mut b = BodyBuilder::new(cx.common.void);
    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, main_ty, main_id),
            vec![],
        ),
    );
    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, exit_ty, exit_id),
            vec![Operand::int(span, cx.common.i32, 0)],
        ),
    );
    b.finish()
}

fn build_io(cx: &mut IrCx, module: &[Name], write_id: FuncId, span: Span) -> Module {
    let string_ty = cx.common.string;
    let int_ty = cx.common.int;
    let ptr_ty = cx.common.ptr;
    let write_ty = cx.ty_fn(vec![int_ty, ptr_ty, int_ty], int_ty);
    let print_ty = cx.ty_fn(vec![string_ty], cx.common.void);
    let print_body = print_ln_body(cx, write_id, write_ty, span);
    let print_id = intern_func(
        cx,
        module,
        "printLn",
        print_ty,
        Vis::Pub,
        FuncKind::J3 { body: print_body },
        span,
    );
    let mut m = Module::new(module.to_vec());
    m.funcs.push(print_id);
    m
}

fn print_ln_body(cx: &mut IrCx, write_id: FuncId, write_ty: Ty, span: Span) -> Body {
    let string_ty = cx.common.string;
    let int_ty = cx.common.int;
    let ptr_ty = cx.common.ptr;
    let n_s = cx.intern_name("s");
    let n_buf = cx.intern_name("buf");
    let n_nl = cx.intern_name("nl");
    let n_nlbuf = cx.intern_name("nlbuf");
    let mut b = BodyBuilder::new(cx.common.void);
    let s = b.param(n_s, string_ty, span);
    let s_p = b.local_place(s, span);
    let ptr_place = b.slice_ptr(cx, s_p.clone(), span);
    let len_place = b.slice_len(cx, s_p, span);
    let buf = b.tmp(n_buf, ptr_ty, span);
    b.assign(
        span,
        b.local_place(buf, span),
        Rvalue::cast(span, ptr_ty, Operand::Copy(ptr_place)),
    );
    b.expr(
        span,
        Rvalue::call(
            span,
            int_ty,
            Operand::fn_addr(span, write_ty, write_id),
            vec![
                Operand::int(span, int_ty, 1),
                Operand::Copy(b.local_place(buf, span)),
                Operand::Copy(len_place),
            ],
        ),
    );

    let nl = b.let_local(n_nl, string_ty, span);
    b.assign(
        span,
        b.local_place(nl, span),
        Rvalue::use_op(span, Operand::bytes(span, string_ty, b"\n")),
    );
    let nl_p = b.local_place(nl, span);
    let nl_ptr = b.slice_ptr(cx, nl_p.clone(), span);
    let nl_len = b.slice_len(cx, nl_p, span);
    let nl_buf = b.tmp(n_nlbuf, ptr_ty, span);
    b.assign(
        span,
        b.local_place(nl_buf, span),
        Rvalue::cast(span, ptr_ty, Operand::Copy(nl_ptr)),
    );
    b.expr(
        span,
        Rvalue::call(
            span,
            int_ty,
            Operand::fn_addr(span, write_ty, write_id),
            vec![
                Operand::int(span, int_ty, 1),
                Operand::Copy(b.local_place(nl_buf, span)),
                Operand::Copy(nl_len),
            ],
        ),
    );
    b.finish()
}

fn hello_main_body(cx: &mut IrCx, print_id: FuncId, print_ty: Ty, span: Span) -> Body {
    let string_ty = cx.common.string;
    let n_msg = cx.intern_name("msg");
    let mut b = BodyBuilder::new(cx.common.void);
    let msg = b.let_local(n_msg, string_ty, span);
    b.assign(
        span,
        b.local_place(msg, span),
        Rvalue::use_op(span, Operand::bytes(span, string_ty, b"Hello, world!")),
    );
    b.expr(
        span,
        Rvalue::call(
            span,
            cx.common.void,
            Operand::fn_addr(span, print_ty, print_id),
            vec![Operand::Copy(b.local_place(msg, span))],
        ),
    );
    b.finish()
}

pub fn stdlib_dir() -> std::path::PathBuf {
    if let Ok(dir) = std::env::var("J3_STDLIB") {
        return std::path::PathBuf::from(dir);
    }
    let baked = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("stdlib");
    if baked.join("j3").is_dir() {
        return baked;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let beside = dir.join("stdlib");
            if beside.join("j3").is_dir() {
                return beside;
            }
        }
    }
    baked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emit;
    use crate::ir::dump::dump_program;

    #[test]
    fn hello_prints_hello_world() {
        let prog = hello_world();
        let dump = dump_program(&prog.cx, &prog.modules);
        assert!(dump.contains("printLn"), "{dump}");
        assert!(dump.contains("@syscall("), "{dump}");
        assert!(dump.contains("Hello, world!"), "{dump}");
        assert!(dump.contains("fn start()"), "{dump}");

        let c = emit::emit_program(&prog.cx, &prog.modules).expect("emit");
        assert!(c.contains("j3_syscall("), "{c}");
        assert!(c.contains("j3_string"), "{c}");
        assert!(c.contains("int main(void)"), "{c}");

        let run = emit::compile_and_run(&c).expect("cc");
        assert_eq!(
            run.status,
            0,
            "stderr {}",
            String::from_utf8_lossy(&run.stderr)
        );
        let out = String::from_utf8_lossy(&run.stdout);
        assert_eq!(out, "Hello, world!\n");
    }

    #[test]
    fn stdlib_j3_files_exist() {
        let dir = stdlib_dir().join("j3");
        for name in [
            "sys.j3",
            "sys_windows.j3",
            "sys_macos.j3",
            "io.j3",
            "math.j3",
            "rand.j3",
            "memory.j3",
            "option.j3",
            "result.j3",
            "text.j3",
            "game.j3",
            "game_headless.j3",
            "ecs.j3",
            "app.j3",
            "collide.j3",
            "res.j3",
            "scene.j3",
            "assets.j3",
            "tile.j3",
            "ui.j3",
            "save.j3",
            "fx.j3",
            "debug.j3",
            "gfx.j3",
            "mesh.j3",
            "voxel.j3",
        ] {
            let p = dir.join(name);
            assert!(p.is_file(), "missing {}", p.display());
        }
    }
}
