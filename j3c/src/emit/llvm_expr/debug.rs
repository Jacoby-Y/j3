use super::*;

/// Line locations plus named locals (`DILocalVariable` / `dbg.declare`).
pub(crate) struct DebugOut {
    pub(super) on: bool,
    pub(super) next: u32,
    pub(super) body: String,
    pub(super) subs: HashMap<FuncId, u32>,
    pub(super) locs: HashMap<(u32, u32), u32>,
    pub(super) files: HashMap<FileId, u32>,
    pub(super) tys: HashMap<Ty, u32>,
    pub(super) expr: u32,
    pub(super) empty: u32,
}

impl DebugOut {
    pub fn off() -> DebugOut {
        DebugOut {
            on: false,
            next: 0,
            body: String::new(),
            subs: HashMap::new(),
            locs: HashMap::new(),
            files: HashMap::new(),
            tys: HashMap::new(),
            expr: 0,
            empty: 0,
        }
    }

    pub fn prepare(cx: &IrCx, modules: &[Module], keep: Option<&HashSet<FuncId>>) -> DebugOut {
        let mut files: Vec<(crate::span::FileId, u32)> = Vec::new();
        let mut seen = HashSet::new();
        for module in modules {
            for id in &module.funcs {
                let func = cx.func(*id);
                if !matches!(func.kind, FuncKind::J3 { .. }) || func.span.is_dummy() || keep.is_some_and(|k| !k.contains(id)) {
                    continue;
                }
                if cx.file(func.span.file).is_none() {
                    continue;
                }
                if seen.insert(func.span.file) {
                    files.push((func.span.file, 0));
                }
            }
        }
        if files.is_empty() {
            return DebugOut::off();
        }
        let mut dbg = DebugOut {
            on: true,
            next: 0,
            body: String::new(),
            subs: HashMap::new(),
            locs: HashMap::new(),
            files: HashMap::new(),
            tys: HashMap::new(),
            expr: 0,
            empty: 0,
        };
        let cu = dbg.alloc();
        let dwarf = dbg.alloc();
        let ver = dbg.alloc();
        let empty = dbg.alloc();
        let subty = dbg.alloc();
        let types = dbg.alloc();
        for slot in &mut files {
            slot.1 = dbg.alloc();
        }
        let file0 = files[0].1;
        dbg.body.push_str(&format!(
            "!{cu} = distinct !DICompileUnit(language: DW_LANG_C99, file: !{file0}, producer: \"j3c\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug, enums: !{empty}, retainedTypes: !{empty}, globals: !{empty}, imports: !{empty})\n"
        ));
        dbg.body.push_str(&format!(
            "!{dwarf} = !{{i32 7, !\"Dwarf Version\", i32 4}}\n"
        ));
        dbg.body.push_str(&format!(
            "!{ver} = !{{i32 2, !\"Debug Info Version\", i32 3}}\n"
        ));
        dbg.empty = empty;
        dbg.body.push_str(&format!("!{empty} = !{{}}\n"));
        let expr = dbg.alloc();
        dbg.expr = expr;
        dbg.body.push_str(&format!("!{expr} = !DIExpression()\n"));
        dbg.body.push_str(&format!(
            "!{subty} = !DISubroutineType(types: !{types})\n"
        ));
        dbg.body.push_str(&format!("!{types} = !{{null}}\n"));
        for (fid, md) in &files {
            let file = match cx.file(*fid) {
                Some(f) => f,
                None => continue,
            };
            let (name, dir) = file_parts(&file.path);
            dbg.body.push_str(&format!(
                "!{md} = !DIFile(filename: \"{}\", directory: \"{}\")\n",
                ll_md_str(&name),
                ll_md_str(&dir)
            ));
            dbg.files.insert(*fid, *md);
        }
        for module in modules {
            for id in &module.funcs {
                let func = cx.func(*id);
                if !matches!(func.kind, FuncKind::J3 { .. }) || func.span.is_dummy() || keep.is_some_and(|k| !k.contains(id)) {
                    continue;
                }
                let file = match cx.file(func.span.file) {
                    Some(f) => f,
                    None => continue,
                };
                let file_md = match files.iter().find(|(f, _)| *f == func.span.file) {
                    Some((_, md)) => *md,
                    None => continue,
                };
                let line = file.line(func.span.start).max(1) as u32;
                let sp = dbg.alloc();
                let name = ll_md_str(cx.name_str(func.name));
                let link = ll_md_str(&c_expr::func_c_name(cx, func));
                dbg.body.push_str(&format!(
                    "!{sp} = distinct !DISubprogram(name: \"{name}\", linkageName: \"{link}\", scope: !{file_md}, file: !{file_md}, line: {line}, type: !{subty}, scopeLine: {line}, flags: DIFlagPrototyped, spFlags: DISPFlagDefinition, unit: !{cu}, retainedNodes: !{empty})\n"
                ));
                dbg.subs.insert(func.id, sp);
            }
        }
        if dbg.subs.is_empty() {
            return DebugOut::off();
        }
        dbg
    }

    pub fn enabled(&self) -> bool {
        self.on
    }

    pub fn subprogram(&self, id: FuncId) -> Option<u32> {
        self.subs.get(&id).copied()
    }

    pub fn location(&mut self, scope: u32, line: u32) -> u32 {
        let line = line.max(1);
        if let Some(id) = self.locs.get(&(scope, line)) {
            return *id;
        }
        let id = self.alloc();
        self.body.push_str(&format!(
            "!{id} = !DILocation(line: {line}, column: 1, scope: !{scope})\n"
        ));
        self.locs.insert((scope, line), id);
        id
    }

    pub fn file_of(&self, file: FileId) -> Option<u32> {
        self.files.get(&file).copied()
    }

    pub fn local_var(
        &mut self,
        cx: &IrCx,
        scope: u32,
        file: u32,
        name: &str,
        ty: Ty,
        line: u32,
    ) -> u32 {
        let ty_md = self.di_type(cx, ty);
        let id = self.alloc();
        self.body.push_str(&format!(
            "!{id} = !DILocalVariable(name: \"{}\", scope: !{scope}, file: !{file}, line: {line}, type: !{ty_md})\n",
            ll_md_str(name)
        ));
        id
    }

    fn di_type(&mut self, cx: &IrCx, ty: Ty) -> u32 {
        if let Some(id) = self.tys.get(&ty).copied() {
            return id;
        }
        let id = self.alloc();
        self.tys.insert(ty, id);
        let kind = cx.ty_kind(ty).clone();
        match kind {
            TyKind::Void => self.basic(id, "void", 0, 5),
            TyKind::Bool => self.basic(id, "bool", 8, 2),
            TyKind::Int(int_ty) => {
                let (name, enc) = match int_ty {
                    IntTy::I64 => ("int", 5),
                    IntTy::I8 => ("i8", 5),
                    IntTy::I16 => ("i16", 5),
                    IntTy::I32 => ("i32", 5),
                    IntTy::U8 => ("u8", 7),
                    IntTy::U16 => ("u16", 7),
                    IntTy::U32 => ("u32", 7),
                    IntTy::U64 => ("u64", 7),
                };
                self.basic(id, name, int_ty.bits(), enc);
            }
            TyKind::Float(FloatTy::F32) => self.basic(id, "f32", 32, 4),
            TyKind::Float(FloatTy::F64) => self.basic(id, "float", 64, 4),
            TyKind::Ptr | TyKind::CStr | TyKind::Fn { .. } => {
                let base = self.di_type(cx, cx.common.u8);
                self.pointer(id, base);
            }
            TyKind::Ref(inner) => {
                let base = self.di_type(cx, inner);
                self.pointer(id, base);
            }
            TyKind::String => self.pair(cx, id, "string", cx.common.u8),
            TyKind::Slice { elem } => self.pair(cx, id, "slice", elem),
            TyKind::Array { elem, len } => {
                let base = self.di_type(cx, elem);
                let sub = self.alloc();
                self.body
                    .push_str(&format!("!{sub} = !DISubrange(count: {len})\n"));
                let elems = self.md_list(&[sub]);
                let bits = abi::size(cx, ty) * 8;
                let align = abi::align(cx, ty) * 8;
                self.body.push_str(&format!(
                    "!{id} = !DICompositeType(tag: DW_TAG_array_type, baseType: !{base}, size: {bits}, align: {align}, elements: !{elems})\n"
                ));
            }
            TyKind::Struct(sid) => {
                let sname = {
                    let n = cx.struct_def(sid).name;
                    cx.name_str(n).to_string()
                };
                let field_names: Vec<_> = cx.struct_def(sid).fields.iter().map(|f| f.name).collect::<Vec<_>>();
                let field_tys: Vec<Ty> = cx.struct_def(sid).fields.iter().map(|f| f.ty).collect();
                let fields: Vec<(String, Ty)> = field_names
                    .into_iter()
                    .map(|n| cx.name_str(n).to_string())
                    .zip(field_tys)
                    .collect();
                let mut members = Vec::new();
                for (i, (fname, fty)) in fields.iter().enumerate() {
                    if fname.is_empty() {
                        continue;
                    }
                    let Some(off) = abi::field_offset(cx, ty, i) else {
                        continue;
                    };
                    let base = self.di_type(cx, *fty);
                    let mid = self.alloc();
                    let fbits = abi::size(cx, *fty) * 8;
                    let falign = abi::align(cx, *fty) * 8;
                    self.body.push_str(&format!(
                        "!{mid} = !DIDerivedType(tag: DW_TAG_member, name: \"{}\", scope: !{id}, baseType: !{base}, size: {fbits}, align: {falign}, offset: {})\n",
                        ll_md_str(fname),
                        off * 8
                    ));
                    members.push(mid);
                }
                let elems = if members.is_empty() {
                    self.empty
                } else {
                    self.md_list(&members)
                };
                let bits = abi::size(cx, ty) * 8;
                let align = abi::align(cx, ty) * 8;
                self.body.push_str(&format!(
                    "!{id} = !DICompositeType(tag: DW_TAG_structure_type, name: \"{}\", size: {bits}, align: {align}, elements: !{elems})\n",
                    ll_md_str(&sname)
                ));
            }
            TyKind::Enum(eid) => {
                let c_like = cx.enum_def(eid).is_c_like();
                let ename = {
                    let n = cx.enum_def(eid).name;
                    cx.name_str(n).to_string()
                };
                if c_like {
                    self.basic(id, &ename, 32, 5);
                } else {
                    let variant_names: Vec<_> = cx
                        .enum_def(eid)
                        .variants
                        .iter()
                        .map(|v| v.name)
                        .collect::<Vec<_>>();
                    let variant_payloads: Vec<Option<Ty>> = cx
                        .enum_def(eid)
                        .variants
                        .iter()
                        .map(|v| v.payload)
                        .collect();
                    let variants_src: Vec<(String, Ty)> = variant_names
                        .into_iter()
                        .zip(variant_payloads)
                        .filter_map(|(n, p)| p.map(|ty| (cx.name_str(n).to_string(), ty)))
                        .collect();
                    let tag = self.di_type(cx, cx.common.i32);
                    let tag_m = self.alloc();
                    self.body.push_str(&format!(
                        "!{tag_m} = !DIDerivedType(tag: DW_TAG_member, name: \"tag\", scope: !{id}, baseType: !{tag}, size: 32, align: 32, offset: 0)\n"
                    ));
                    let union_id = self.alloc();
                    let mut variants = Vec::new();
                    let mut payload = 0u64;
                    let mut ua = 1u64;
                    for (vname, p) in &variants_src {
                        let s = abi::size(cx, *p);
                        if s > payload {
                            payload = s;
                        }
                        let pa = abi::align(cx, *p);
                        if pa > ua {
                            ua = pa;
                        }
                        let base = self.di_type(cx, *p);
                        let mid = self.alloc();
                        self.body.push_str(&format!(
                            "!{mid} = !DIDerivedType(tag: DW_TAG_member, name: \"{}\", scope: !{union_id}, baseType: !{base}, size: {}, align: {}, offset: 0)\n",
                            ll_md_str(vname),
                            s * 8,
                            pa * 8
                        ));
                        variants.push(mid);
                    }
                    let union_elems = if variants.is_empty() {
                        self.empty
                    } else {
                        self.md_list(&variants)
                    };
                    let ubits = payload * 8;
                    let ualign = ua * 8;
                    self.body.push_str(&format!(
                        "!{union_id} = !DICompositeType(tag: DW_TAG_union_type, name: \"payload\", size: {ubits}, align: {ualign}, elements: !{union_elems})\n"
                    ));
                    let poff = abi::field_offset(cx, ty, 1).unwrap_or(4) * 8;
                    let pay_m = self.alloc();
                    self.body.push_str(&format!(
                        "!{pay_m} = !DIDerivedType(tag: DW_TAG_member, name: \"payload\", scope: !{id}, baseType: !{union_id}, size: {ubits}, align: {ualign}, offset: {poff})\n"
                    ));
                    let elems = self.md_list(&[tag_m, pay_m]);
                    let bits = abi::size(cx, ty) * 8;
                    let align = abi::align(cx, ty) * 8;
                    self.body.push_str(&format!(
                        "!{id} = !DICompositeType(tag: DW_TAG_structure_type, name: \"{}\", size: {bits}, align: {align}, elements: !{elems})\n",
                        ll_md_str(&ename)
                    ));
                }
            }
        }
        id
    }

    fn basic(&mut self, id: u32, name: &str, bits: u32, encoding: u32) {
        self.body.push_str(&format!(
            "!{id} = !DIBasicType(name: \"{}\", size: {bits}, encoding: {encoding})\n",
            ll_md_str(name)
        ));
    }

    fn pointer(&mut self, id: u32, base: u32) {
        self.body.push_str(&format!(
            "!{id} = !DIDerivedType(tag: DW_TAG_pointer_type, baseType: !{base}, size: 64, align: 64)\n"
        ));
    }

    fn pair(&mut self, cx: &IrCx, id: u32, name: &str, pointee: Ty) {
        let base = self.di_type(cx, pointee);
        let ptr = self.alloc();
        self.pointer(ptr, base);
        let len = self.di_type(cx, cx.common.i64);
        let p = self.alloc();
        let n = self.alloc();
        self.body.push_str(&format!(
            "!{p} = !DIDerivedType(tag: DW_TAG_member, name: \"ptr\", scope: !{id}, baseType: !{ptr}, size: 64, align: 64, offset: 0)\n"
        ));
        self.body.push_str(&format!(
            "!{n} = !DIDerivedType(tag: DW_TAG_member, name: \"len\", scope: !{id}, baseType: !{len}, size: 64, align: 64, offset: 64)\n"
        ));
        let elems = self.md_list(&[p, n]);
        self.body.push_str(&format!(
            "!{id} = !DICompositeType(tag: DW_TAG_structure_type, name: \"{name}\", size: 128, align: 64, elements: !{elems})\n"
        ));
    }

    fn md_list(&mut self, ids: &[u32]) -> u32 {
        let id = self.alloc();
        let inner = ids
            .iter()
            .map(|n| format!("!{n}"))
            .collect::<Vec<_>>()
            .join(", ");
        self.body.push_str(&format!("!{id} = !{{{inner}}}\n"));
        id
    }

    pub fn finish(&self) -> String {
        if !self.on {
            return String::new();
        }
        let mut out = String::new();
        out.push_str("!llvm.dbg.cu = !{!0}\n");
        out.push_str("!llvm.module.flags = !{!1, !2}\n");
        out.push_str(&self.body);
        out
    }

    fn alloc(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
}

pub(super) fn file_parts(path: &str) -> (String, String) {
    let p = std::path::Path::new(path);
    let name = p
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(path)
        .to_string();
    let dir = p
        .parent()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(".")
        .to_string();
    (name, dir)
}

pub(super) fn ll_md_str(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out
}
