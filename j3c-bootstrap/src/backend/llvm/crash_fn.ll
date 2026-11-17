define void @j3_crash(i32 %sig, ptr %info, ptr %uc) {
entry:
%buf = alloca [96 x i8], align 8
%n = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %buf, i64 96, ptr @.j3_crash_hdr, i32 %sig, ptr @main)
%n64 = sext i32 %n to i64
%w = call i64 @write(i32 2, ptr %buf, i64 %n64)
%pcp = getelementptr i8, ptr %uc, i64 168
%pc = load ptr, ptr %pcp, align 8
%m = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %buf, i64 96, ptr @.j3_crash_pc, ptr %pc)
%m64 = sext i32 %m to i64
%w1 = call i64 @write(i32 2, ptr %buf, i64 %m64)
%fp0 = call ptr @llvm.frameaddress.p0(i32 0)
call void @j3_walk(ptr %fp0, i1 true)
%code = add i32 %sig, 128
call void @_exit(i32 %code)
unreachable
}

