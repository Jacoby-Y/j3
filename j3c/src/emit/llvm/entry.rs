//! The abort routine and the host `main` / `SDL_main` entry glue.

use super::*;

/// `@j3_abort(i64 file << 40 | line << 8 | kind, i64 a, i64 b)`: prints `file:line: message`,
/// then `a` and `b` for a kind with values (`index out of bounds: index 5, len 3`), to stderr
/// and exits 102. One copy replaces a string and a call sequence at every bounds, divide, and
/// format check. `trace` (debug x86-64 Linux, beside the crash handler) also prints the
/// frame-pointer walk, which `host::symbolize` turns into a `called from` list.
pub(super) fn abort_routine(files: &[String], trace: bool) -> String {
    use crate::ir::stmt::AbortKind;
    let mut out = String::from("\n");
    let table = |name: &str, texts: Vec<String>, out: &mut String| {
        let mut items = Vec::new();
        for (i, s) in texts.iter().enumerate() {
            let mut body = String::new();
            for b in s.bytes() {
                if b.is_ascii_graphic() && b != b'"' && b != b'\\' || b == b' ' {
                    body.push(b as char);
                } else {
                    body.push_str(&format!("\\{b:02X}"));
                }
            }
            out.push_str(&format!(
                "@{name}.s{i} = private unnamed_addr constant [{} x i8] c\"{body}\"\n",
                s.len()
            ));
            items.push(format!("%j3_string {{ ptr @{name}.s{i}, i64 {} }}", s.len()));
        }
        out.push_str(&format!(
            "@{name} = private unnamed_addr constant [{} x %j3_string] [{}]\n",
            texts.len(),
            items.join(", ")
        ));
    };
    table("j3_abort_files", files.iter().map(|f| format!("{f}:")).collect(), &mut out);
    table("j3_abort_kinds", AbortKind::ALL.iter().map(|k| format!(": {}", k.message())).collect(), &mut out);
    table("j3_abort_da", AbortKind::ALL.iter().map(|k| k.detail()[0].to_string()).collect(), &mut out);
    table("j3_abort_db", AbortKind::ALL.iter().map(|k| k.detail()[1].to_string()).collect(), &mut out);
    out.push_str("@j3_abort_nl = private unnamed_addr constant [1 x i8] c\"\\0A\"\n");
    out.push_str(
        "define internal void @j3_abort_str(ptr %table, i64 %i) noinline cold nounwind {
entry:
  %p = getelementptr %j3_string, ptr %table, i64 %i, i32 0
  %l = getelementptr %j3_string, ptr %table, i64 %i, i32 1
  %ptr = load ptr, ptr %p
  %len = load i64, ptr %l
  %w = call i64 @write(i32 2, ptr %ptr, i64 %len)
  ret void
}

define internal void @j3_abort_num(i64 %v) noinline cold nounwind {
entry:
  %buf = alloca [24 x i8], align 1
  %neg = icmp slt i64 %v, 0
  %nv = sub i64 0, %v
  %mag = select i1 %neg, i64 %nv, i64 %v
  br label %digits
digits:
  %n = phi i64 [ %mag, %entry ], [ %q, %digits ]
  %pos = phi i64 [ 24, %entry ], [ %p1, %digits ]
  %p1 = sub i64 %pos, 1
  %r = urem i64 %n, 10
  %q = udiv i64 %n, 10
  %c = add i64 %r, 48
  %c8 = trunc i64 %c to i8
  %dp = getelementptr i8, ptr %buf, i64 %p1
  store i8 %c8, ptr %dp
  %more = icmp ne i64 %q, 0
  br i1 %more, label %digits, label %sign
sign:
  %mp = sub i64 %p1, 1
  %mptr = getelementptr i8, ptr %buf, i64 %mp
  br i1 %neg, label %minus, label %done
minus:
  store i8 45, ptr %mptr
  br label %done
done:
  %start = select i1 %neg, i64 %mp, i64 %p1
  %sp = getelementptr i8, ptr %buf, i64 %start
  %len = sub i64 24, %start
  %w = call i64 @write(i32 2, ptr %sp, i64 %len)
  ret void
}

define internal void @j3_abort(i64 %v, i64 %a, i64 %b) noinline cold noreturn nounwind {
entry:
  %kind = and i64 %v, 255
  %line = lshr i64 %v, 8
  %line32 = and i64 %line, 4294967295
  %file = lshr i64 %v, 40
  call void @j3_abort_str(ptr @j3_abort_files, i64 %file)
  call void @j3_abort_num(i64 %line32)
  call void @j3_abort_str(ptr @j3_abort_kinds, i64 %kind)
  %dl = getelementptr %j3_string, ptr @j3_abort_da, i64 %kind, i32 1
  %dlen = load i64, ptr %dl
  %has = icmp ne i64 %dlen, 0
  br i1 %has, label %vals, label %end
vals:
  call void @j3_abort_str(ptr @j3_abort_da, i64 %kind)
  call void @j3_abort_num(i64 %a)
  call void @j3_abort_str(ptr @j3_abort_db, i64 %kind)
  call void @j3_abort_num(i64 %b)
  br label %end
end:
  %wn = call i64 @write(i32 2, ptr @j3_abort_nl, i64 1)
",
    );
    if trace {
        out.push_str(crate::emit::llvm_crash::ABORT_TRACE);
    }
    out.push_str("  call void @_exit(i32 102)\n  unreachable\n}\n");
    out
}

/// One module's LLVM text. Types are duplicated. This module's functions and
/// globals are defined (external linkage); everything else is declared. `@main`
/// is only in the module that owns `j3.sys.start` or user `main`.
///
/// `chunk` is (index, functions): a module with giant functions is split over several objects,

/// The C entry symbol: `SDL_main` in an Android shared library (SDLActivity calls it), else `main`.
pub(super) fn host_entry_name(triple: &str) -> &'static str {
    if crate::emit::host::target_os(triple) == crate::emit::host::TargetOs::Android {
        "SDL_main"
    } else {
        "main"
    }
}

/// Module that should define host `@main`: `j3.sys.start` if present, else the last `main`.
pub(super) fn entry_module(cx: &IrCx, modules: &[Module]) -> Result<usize, String> {
    let mut start = None;
    let mut main = None;
    for (i, module) in modules.iter().enumerate() {
        let is_sys = module.name.len() == 2
            && cx.name_str(module.name[0]) == "j3"
            && cx.name_str(module.name[1]) == "sys";
        for id in &module.funcs {
            let name = cx.name_str(cx.func(*id).name);
            if name == "start" && is_sys {
                start = Some(i);
            }
            if name == "main" {
                main = Some(i);
            }
        }
    }
    start
        .or(main)
        .ok_or_else(|| "LLVM emit needs j3.sys.start or fn main()".to_string())
}


pub(super) fn sys_func(cx: &IrCx, modules: &[Module], name: &str) -> Option<String> {
    for module in modules {
        let is_sys = module.name.len() == 2
            && cx.name_str(module.name[0]) == "j3"
            && cx.name_str(module.name[1]) == "sys";
        if !is_sys {
            continue;
        }
        for id in &module.funcs {
            let func = cx.func(*id);
            if cx.name_str(func.name) == name {
                return Some(c_expr::func_c_name(cx, func));
            }
        }
    }
    None
}

pub(super) fn host_entry(cx: &IrCx, modules: &[Module]) -> Result<String, String> {
    let mut start = None;
    let mut main = None;
    for module in modules {
        let is_sys = module.name.len() == 2
            && cx.name_str(module.name[0]) == "j3"
            && cx.name_str(module.name[1]) == "sys";
        for id in &module.funcs {
            let func = cx.func(*id);
            let name = cx.name_str(func.name);
            if name == "start" && is_sys {
                start = Some(c_expr::func_c_name(cx, func));
            }
            if name == "main" {
                main = Some(c_expr::func_c_name(cx, func));
            }
        }
    }
    start
        .or(main)
        .ok_or_else(|| "LLVM emit needs j3.sys.start or fn main()".to_string())
}
