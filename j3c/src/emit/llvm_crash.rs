//! Debug-build crash handler: on SIGSEGV/SIGABRT/SIGFPE/SIGBUS/SIGILL print the signal, the
//! faulting pc, and the frame-pointer chain as raw addresses (`j3 crash:` / `j3 at:` lines).
//! `host::symbolize` turns them into `function at file:line` after the process exits, so the
//! binary needs no symbol table. x86-64 Linux only; other targets keep the default crash.

pub fn supported(triple: &str) -> bool {
    triple.contains("x86_64") && triple.contains("linux")
}

/// Function attribute group so every debug function keeps a frame pointer.
pub const ATTRS: &str = "attributes #0 = { \"frame-pointer\"=\"all\" }\n";

fn cstr(name: &str, text: &str) -> String {
    let n = text.len() + 1;
    let body = text.replace('\n', "\\0A");
    format!("@{name} = private unnamed_addr constant [{n} x i8] c\"{body}\\00\"\n")
}

/// Module-level globals, declares, and `@j3_crash`; goes before `@main`.
pub fn definitions() -> String {
    let mut s = String::new();
    s.push_str("declare i32 @sigaction(i32, ptr, ptr)\n");
    s.push_str("declare i32 @sigaltstack(ptr, ptr)\n");
    s.push_str("declare void @llvm.memset.p0.i64(ptr, i8, i64, i1)\n");
    s.push_str("declare ptr @llvm.frameaddress.p0(i32)\n");
    s.push_str(&cstr(".j3_crash_hdr", "j3 crash: signal %d\nj3 base: %p\n"));
    s.push_str(&cstr(".j3_crash_pc", "j3 pc: %p\n"));
    s.push_str(&cstr(".j3_crash_at", "j3 at: %p\n"));
    s.push_str("@.j3_crash_stack = private global [65536 x i8] zeroinitializer, align 16\n\n");
    // (sig, siginfo*, ucontext*): x86-64 rip lives at ucontext + 168. The first walked frame is
    // the handler's own (its return address is the signal trampoline), so it is not printed.
    s.push_str(
        "define void @j3_crash(i32 %sig, ptr %info, ptr %uc) {\n\
entry:\n\
  %buf = alloca [96 x i8], align 8\n\
  %n = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %buf, i64 96, ptr @.j3_crash_hdr, i32 %sig, ptr @main)\n\
  %n64 = sext i32 %n to i64\n\
  %w = call i64 @write(i32 2, ptr %buf, i64 %n64)\n\
  %pcp = getelementptr i8, ptr %uc, i64 168\n\
  %pc = load ptr, ptr %pcp, align 8\n\
  %m = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %buf, i64 96, ptr @.j3_crash_pc, ptr %pc)\n\
  %m64 = sext i32 %m to i64\n\
  %w1 = call i64 @write(i32 2, ptr %buf, i64 %m64)\n\
  %fp0 = call ptr @llvm.frameaddress.p0(i32 0)\n\
  br label %walk\n\
walk:\n\
  %i = phi i32 [ 0, %entry ], [ %i2, %next ]\n\
  %fp = phi ptr [ %fp0, %entry ], [ %up, %next ]\n\
  %ip = ptrtoint ptr %fp to i64\n\
  %al = and i64 %ip, 7\n\
  %bad = icmp ne i64 %al, 0\n\
  %null = icmp eq i64 %ip, 0\n\
  %deep = icmp sge i32 %i, 32\n\
  %stop1 = or i1 %bad, %null\n\
  %stop = or i1 %stop1, %deep\n\
  br i1 %stop, label %done, label %step\n\
step:\n\
  %up = load ptr, ptr %fp, align 8\n\
  %rp = getelementptr i8, ptr %fp, i64 8\n\
  %ra = load ptr, ptr %rp, align 8\n\
  %upi = ptrtoint ptr %up to i64\n\
  %grows = icmp ugt i64 %upi, %ip\n\
  br i1 %grows, label %maybe, label %done\n\
maybe:\n\
  %isret = icmp ne ptr %ra, null\n\
  %notfirst = icmp ne i32 %i, 0\n\
  %pr = and i1 %isret, %notfirst\n\
  br i1 %pr, label %print, label %next\n\
print:\n\
  %k = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %buf, i64 96, ptr @.j3_crash_at, ptr %ra)\n\
  %k64 = sext i32 %k to i64\n\
  %w2 = call i64 @write(i32 2, ptr %buf, i64 %k64)\n\
  br label %next\n\
next:\n\
  %i2 = add i32 %i, 1\n\
  br label %walk\n\
done:\n\
  %code = add i32 %sig, 128\n\
  call void @_exit(i32 %code)\n\
  unreachable\n\
}\n\n",
    );
    s
}

/// Lines for the start of `@main`: an alternate stack (so stack overflow is caught) and one
/// `sigaction` per crash signal (`struct sigaction`: handler, 128-byte mask, flags at 136).
pub fn install() -> String {
    let mut s = String::new();
    s.push_str("  %ss = alloca [24 x i8], align 8\n");
    s.push_str("  store ptr @.j3_crash_stack, ptr %ss, align 8\n");
    s.push_str("  %ssf = getelementptr i8, ptr %ss, i64 8\n");
    s.push_str("  store i32 0, ptr %ssf, align 8\n");
    s.push_str("  %sss = getelementptr i8, ptr %ss, i64 16\n");
    s.push_str("  store i64 65536, ptr %sss, align 8\n");
    s.push_str("  %sa0 = call i32 @sigaltstack(ptr %ss, ptr null)\n");
    s.push_str("  %sa = alloca [152 x i8], align 8\n");
    s.push_str("  call void @llvm.memset.p0.i64(ptr %sa, i8 0, i64 152, i1 false)\n");
    s.push_str("  store ptr @j3_crash, ptr %sa, align 8\n");
    s.push_str("  %saf = getelementptr i8, ptr %sa, i64 136\n");
    // SA_SIGINFO | SA_ONSTACK | SA_RESETHAND
    s.push_str("  store i32 -2013265916, ptr %saf, align 8\n");
    for (i, sig) in [11, 6, 8, 7, 4].iter().enumerate() {
        s.push_str(&format!("  %sg{i} = call i32 @sigaction(i32 {sig}, ptr %sa, ptr null)\n"));
    }
    s
}
