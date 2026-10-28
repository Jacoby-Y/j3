use super::*;

impl<'a> Lower<'a> {
    pub(super) fn struct_lit(
        &mut self,
        span: Span,
        name: &parse::Ident,
        type_args: &[parse::Type],
        fields: &[(parse::Ident, Expr)],
    ) -> Result<Rvalue, CheckError> {
        let ty = if type_args.is_empty() {
            match self.types.get(&name.name).copied() {
                Some(t) => t,
                None => {
                    return Err(CheckError::new(
                        self.path,
                        name.span,
                        format!("unknown type `{}`", name.name),
                    ));
                }
            }
        } else {
            let sid = self.instantiate_struct_named(&name.name, type_args, name.span)?;
            self.cx.ty_struct(sid)
        };
        let sid = match self.cx.ty_kind(ty) {
            TyKind::Struct(id) => *id,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    name.span,
                    format!("`{}` is not a struct", name.name),
                ));
            }
        };
        self.struct_lit_sid(span, &name.name, ty, sid, fields)
    }

    pub(super) fn script_lit(
        &mut self,
        span: Span,
        path: &str,
        fields: &[(parse::Ident, Expr)],
    ) -> Result<Rvalue, CheckError> {
        let key = format!("script.{}", path.trim_end_matches(".j3s").replace('/', "."));
        let sid = self
            .cx
            .structs()
            .iter()
            .enumerate()
            .find_map(|(i, s)| {
                let module = s.module.iter().map(|n| self.cx.name_str(*n)).collect::<Vec<_>>();
                if s.script.is_some() && module.join(".") == key {
                    Some(StructId(i as u32))
                } else {
                    None
                }
            })
            .ok_or_else(|| CheckError::new(self.path, span, format!("no script at {path}")))?;
        let ty = self.cx.ty_struct(sid);
        let name = self.cx.name_str(self.cx.struct_def(sid).name).to_string();
        self.struct_lit_sid(span, &name, ty, sid, fields)
    }

    pub(super) fn struct_lit_sid(
        &mut self,
        span: Span,
        name: &str,
        ty: Ty,
        sid: StructId,
        fields: &[(parse::Ident, Expr)],
    ) -> Result<Rvalue, CheckError> {
        let nfields = self.cx.struct_def(sid).fields.len();
        let mut ops: Vec<Option<Operand>> = vec![None; nfields];
        for (fname, value) in fields {
            let mut found = None;
            for (i, f) in self.cx.struct_def(sid).fields.iter().enumerate() {
                if self.cx.name_str(f.name) == fname.name {
                    found = Some((i, f.ty));
                    break;
                }
            }
            let (i, fty) = match found {
                Some(x) => x,
                None => {
                    return Err(CheckError::new(
                        self.path,
                        fname.span,
                        format!("`{name}` has no field `{}`", fname.name),
                    ));
                }
            };
            if let Some(meta) = &self.cx.struct_def(sid).script {
                if i >= meta.prop_spans.len() {
                    return Err(CheckError::new(
                        self.path,
                        fname.span,
                        format!("`{}` is script state, not a prop", fname.name),
                    )
                    .help("move it into `props` if callers should set it"));
                }
            }
            if ops[i].is_some() {
                return Err(CheckError::new(
                    self.path,
                    fname.span,
                    format!("duplicate field `{}`", fname.name),
                ));
            }
            ops[i] = Some(self.operand(value, Some(fty))?);
        }
        let mut out = Vec::new();
        for (i, op) in ops.into_iter().enumerate() {
            match op {
                Some(op) => out.push(op),
                None => {
                    if let Some(op) = self.cx.struct_def(sid).fields[i].default.clone() {
                        out.push(op);
                        continue;
                    }
                    let fname = self.cx.name_str(self.cx.struct_def(sid).fields[i].name);
                    if let Some(meta) = &self.cx.struct_def(sid).script {
                        if let Some(prop_span) = meta.prop_spans.get(i) {
                            let note = self
                                .cx
                                .file(prop_span.file)
                                .map(|file| {
                                    format!(
                                        "prop declared at {}:{}",
                                        file.path,
                                        file.line(prop_span.start)
                                    )
                                })
                                .unwrap_or_else(|| "required prop declared in script".to_string());
                            return Err(CheckError::new(
                                self.path,
                                span,
                                format!("missing required prop `{fname}`"),
                            )
                            .note(note)
                            .help(format!("add `{fname}: ...` to the script literal")));
                        }
                    }
                    return Err(CheckError::new(
                        self.path,
                        span,
                        format!("missing field `{fname}`"),
                    ));
                }
            }
        }
        Ok(Rvalue::aggregate(span, ty, out))
    }

    pub(super) fn array_lit(
        &mut self,
        span: Span,
        elems: &[Expr],
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let mut as_slice = None;
        let (elem_ty, len) = match expected {
            Some(t) => match self.cx.ty_kind(t) {
                TyKind::Array { elem, len } => {
                    if *len != elems.len() as u64 {
                        return Err(CheckError::new(
                            self.path,
                            span,
                            format!("array literal has {} elems, expected {len}", elems.len()),
                        ));
                    }
                    (*elem, *len)
                }
                TyKind::Slice { elem } => {
                    as_slice = Some(t);
                    (*elem, elems.len() as u64)
                }
                _ => {
                    return Err(CheckError::new(
                        self.path,
                        span,
                        "array literal needs an array or slice type",
                    ));
                }
            },
            None => {
                if elems.is_empty() {
                    return Err(CheckError::new(self.path, span, "cannot infer empty array"));
                }
                let first = self.operand(&elems[0], None)?;
                (first.ty(), elems.len() as u64)
            }
        };
        let ty = self.cx.ty_array(elem_ty, len);
        let mut fields = Vec::new();
        for e in elems {
            fields.push(self.operand(e, Some(elem_ty))?);
        }
        let arr = Rvalue::aggregate(span, ty, fields);
        if let Some(want) = as_slice {
            let n = self.fresh("a");
            let id = self.b.tmp(n, ty, span);
            let place = self.b.local_place(id, span);
            self.b.assign(span, place.clone(), arr);
            match self.array_to_slice(span, &place, want)? {
                Some(op) => Ok(Rvalue::use_op(span, op)),
                None => Err(CheckError::new(
                    self.path,
                    span,
                    "array literal does not convert to slice",
                )),
            }
        } else {
            Ok(arr)
        }
    }

    pub(super) fn slice_lit(
        &mut self,
        span: Span,
        elem: &parse::Type,
        fields: &[(parse::Ident, Expr)],
    ) -> Result<Rvalue, CheckError> {
        let elem_ty = self.resolve_ty(elem)?;
        let slice_ty = self.cx.ty_slice(elem_ty);
        let mut ptr = None;
        let mut len = None;
        for (name, value) in fields {
            match name.name.as_str() {
                "ptr" => {
                    let want = self.cx.ty_ref(elem_ty);
                    ptr = Some(self.operand(value, Some(want))?);
                }
                "len" => {
                    len = Some(self.operand(value, Some(self.cx.common.int))?);
                }
                other => {
                    return Err(CheckError::new(
                        self.path,
                        name.span,
                        format!("slice literal has no field `{other}`"),
                    ));
                }
            }
        }
        let ptr = match ptr {
            Some(p) => p,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "slice literal needs `ptr`",
                ));
            }
        };
        let len = match len {
            Some(n) => n,
            None => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "slice literal needs `len`",
                ));
            }
        };
        Ok(Rvalue::aggregate(span, slice_ty, vec![ptr, len]))
    }

    pub(super) fn conv_ty(&self, name: &str) -> Option<Ty> {
        Some(match name {
            "i8" => self.cx.common.i8,
            "i16" => self.cx.common.i16,
            "i32" => self.cx.common.i32,
            "i64" | "int" => self.cx.common.int,
            "u8" => self.cx.common.u8,
            "u16" => self.cx.common.u16,
            "u32" => self.cx.common.u32,
            "u64" => self.cx.common.u64,
            "f32" => self.cx.common.f32,
            "f64" | "float" => self.cx.common.float,
            _ => return None,
        })
    }

    pub(super) fn slice_rvalue(
        &mut self,
        span: Span,
        base: &Expr,
        start: Option<&Expr>,
        end: Option<&Expr>,
    ) -> Result<Rvalue, CheckError> {
        let mut place = self.try_place(base)?;
        while self.cx.ref_inner(place.ty).is_some() {
            place = self.b.deref(self.cx, place, span);
        }
        let elem = match self.cx.elem_ty(place.ty) {
            Some(e) => e,
            None => {
                return Err(CheckError::new(self.path, span, "slice of non-array/slice"));
            }
        };
        let full_len = self.seq_len(&place, span)?;
        let start_op = match start {
            Some(e) => self.operand(e, Some(self.cx.common.int))?,
            None => Operand::int(span, self.cx.common.int, 0),
        };
        let end_op = match end {
            Some(e) => self.operand(e, Some(self.cx.common.int))?,
            None => full_len.clone(),
        };
        self.slice_bounds(span, &start_op, &end_op, &full_len)?;
        let is_string = place.ty == self.cx.common.string;
        let start_place = self.b.index(self.cx, place, start_op.clone(), span);
        if !start_place.access.take_ref {
            return Err(CheckError::new(
                self.path,
                span,
                "cannot take a slice of this place",
            ));
        }
        let ptr_ty = self.cx.ty_ref(elem);
        let ptr_rv = Rvalue::ref_place(span, ptr_ty, start_place);
        let ptr_n = self.fresh("ptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), ptr_rv);
        let len_n = self.fresh("sl");
        let len_id = self.b.tmp(len_n, self.cx.common.int, span);
        let len_place = self.b.local_place(len_id, span);
        self.b.assign(
            span,
            len_place.clone(),
            Rvalue::binary(span, self.cx.common.int, BinOp::Sub, end_op, start_op),
        );
        let slice_ty = if is_string {
            self.cx.common.string
        } else {
            self.cx.ty_slice(elem)
        };
        Ok(Rvalue::aggregate(
            span,
            slice_ty,
            vec![Operand::Copy(ptr_place), Operand::Copy(len_place)],
        ))
    }

    pub(super) fn array_to_slice(
        &mut self,
        span: Span,
        place: &Place,
        want: Ty,
    ) -> Result<Option<Operand>, CheckError> {
        let (elem, _) = match self.cx.ty_kind(place.ty) {
            TyKind::Array { elem, .. } => (*elem, ()),
            _ => return Ok(None),
        };
        match self.cx.ty_kind(want) {
            TyKind::Slice { elem: se } if *se == elem => {}
            _ => return Ok(None),
        }
        let rv = self.slice_rvalue_place(span, place.clone(), None, None)?;
        let n = self.fresh("sl");
        let id = self.b.tmp(n, rv.ty, span);
        let p = self.b.local_place(id, span);
        self.b.assign(span, p.clone(), rv);
        Ok(Some(Operand::Copy(p)))
    }

    pub(super) fn slice_rvalue_place(
        &mut self,
        span: Span,
        place: Place,
        start: Option<Operand>,
        end: Option<Operand>,
    ) -> Result<Rvalue, CheckError> {
        let dummy = Expr {
            span,
            kind: ExprKind::Name(parse::Ident {
                name: String::new(),
                span,
            }),
        };
        let _ = dummy;
        let elem = self.cx.elem_ty(place.ty).expect("array/slice");
        let full_len = self.seq_len(&place, span)?;
        let start_op = start.unwrap_or_else(|| Operand::int(span, self.cx.common.int, 0));
        let end_op = end.unwrap_or_else(|| full_len.clone());
        self.slice_bounds(span, &start_op, &end_op, &full_len)?;
        let is_string = place.ty == self.cx.common.string;
        let start_place = self.b.index(self.cx, place, start_op.clone(), span);
        let ptr_ty = self.cx.ty_ref(elem);
        let ptr_rv = Rvalue::ref_place(span, ptr_ty, start_place);
        let ptr_n = self.fresh("ptr");
        let ptr_id = self.b.tmp(ptr_n, ptr_ty, span);
        let ptr_place = self.b.local_place(ptr_id, span);
        self.b.assign(span, ptr_place.clone(), ptr_rv);
        let len_n = self.fresh("sl");
        let len_id = self.b.tmp(len_n, self.cx.common.int, span);
        let len_place = self.b.local_place(len_id, span);
        self.b.assign(
            span,
            len_place.clone(),
            Rvalue::binary(span, self.cx.common.int, BinOp::Sub, end_op, start_op),
        );
        let slice_ty = if is_string {
            self.cx.common.string
        } else {
            self.cx.ty_slice(elem)
        };
        Ok(Rvalue::aggregate(
            span,
            slice_ty,
            vec![Operand::Copy(ptr_place), Operand::Copy(len_place)],
        ))
    }

    pub(super) fn slice_from(
        &mut self,
        span: Span,
        ptr: &Expr,
        len: &Expr,
        expected: Option<Ty>,
    ) -> Result<Rvalue, CheckError> {
        let slice_ty = match expected {
            Some(t) if matches!(self.cx.ty_kind(t), TyKind::Slice { .. }) => t,
            _ => {
                return Err(CheckError::new(
                    self.path,
                    span,
                    "`@slice` needs an expected slice type",
                ));
            }
        };
        let elem = self.cx.elem_ty(slice_ty).unwrap();
        let ptr_ty = self.cx.ty_ref(elem);
        let p = self.operand(ptr, None)?;
        let p = if p.ty() == ptr_ty {
            p
        } else if p.ty() == self.cx.common.ptr {
            let rv = Rvalue::cast(span, ptr_ty, p);
            Operand::Copy(self.materialize_rv(rv, span))
        } else {
            return Err(CheckError::new(
                self.path,
                span,
                "`@slice` pointer type mismatch",
            ));
        };
        let n = self.operand(len, Some(self.cx.common.int))?;
        Ok(Rvalue::aggregate(span, slice_ty, vec![p, n]))
    }
}
