define void @j3_walk(ptr %fp0, i1 %skip) {
entry:
  %buf = alloca [96 x i8], align 8
  br label %walk
walk:
  %i = phi i32 [ 0, %entry ], [ %i2, %next ]
  %fp = phi ptr [ %fp0, %entry ], [ %up, %next ]
  %ip = ptrtoint ptr %fp to i64
  %al = and i64 %ip, 7
  %bad = icmp ne i64 %al, 0
  %null = icmp eq i64 %ip, 0
  %deep = icmp sge i32 %i, 32
  %stop1 = or i1 %bad, %null
  %stop = or i1 %stop1, %deep
  br i1 %stop, label %done, label %step
step:
  %up = load ptr, ptr %fp, align 8
  %rp = getelementptr i8, ptr %fp, i64 8
  %ra = load ptr, ptr %rp, align 8
  %upi = ptrtoint ptr %up to i64
  %grows = icmp ugt i64 %upi, %ip
  br i1 %grows, label %maybe, label %done
maybe:
  %isret = icmp ne ptr %ra, null
  %first = icmp eq i32 %i, 0
  %skipthis = and i1 %first, %skip
  %show = xor i1 %skipthis, true
  %pr = and i1 %isret, %show
  br i1 %pr, label %print, label %next
print:
  %k = call i32 (ptr, i64, ptr, ...) @snprintf(ptr %buf, i64 96, ptr @.j3_crash_at, ptr %ra)
  %k64 = sext i32 %k to i64
  %w2 = call i64 @write(i32 2, ptr %buf, i64 %k64)
  br label %next
next:
  %i2 = add i32 %i, 1
  br label %walk
done:
  ret void
}

