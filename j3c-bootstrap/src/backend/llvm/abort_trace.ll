  %tb = alloca [96 x i8], align 8
  %tn = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %tb, i64 96, ptr @.j3_crash_base, ptr @main)
  %tn64 = sext i32 %tn to i64
  %tw = call i64 @write(i32 2, ptr %tb, i64 %tn64)
  %tfp = call ptr @llvm.frameaddress.p0(i32 0)
  call void @j3_walk(ptr %tfp, i1 false)
