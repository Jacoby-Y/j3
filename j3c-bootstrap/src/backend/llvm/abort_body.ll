define internal void @j3_abort_str(ptr %table, i64 %i) noinline cold nounwind {
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
