use super::*;

/// Tables every function in a module appends to.
pub(super) struct Shared {
    pub(super) constants: Vec<Vec<u8>>,
    pub(super) natives: Vec<(u64, String)>,
    pub(super) scripts: Vec<String>,
    pub(super) globals: Vec<GlobalId>,
    pub(super) local_index: HashMap<FuncId, u32>,
}

pub(super) struct LoopFixups {
    pub(super) breaks: Vec<usize>,
    pub(super) continues: Vec<usize>,
    pub(super) scope_depth: usize,
}

/// Where a place's bytes start: a base register plus a constant offset.
#[derive(Clone, Copy)]
pub(super) struct Addr {
    pub(super) base: u32,
    pub(super) offset: u64,
}

pub(super) struct Codegen<'a> {
    pub(super) cx: &'a IrCx,
    pub(super) func: &'a Func,
    pub(super) body: &'a Body,
    pub(super) shared: &'a mut Shared,
    /// Frame offset of each local that lives in memory.
    pub(super) mem: Vec<Option<u32>>,
    /// Register of each scalar local that never has its address taken.
    pub(super) value: Vec<Option<u32>>,
    pub(super) uses: Vec<u32>,
    pub(super) frame_size: u32,
    pub(super) regs: u32,
    pub(super) prologue: Vec<Instr>,
    pub(super) const_regs: HashMap<u64, u32>,
    pub(super) code: Vec<Instr>,
    pub(super) calls: Vec<CallSite>,
    pub(super) loops: Vec<LoopFixups>,
    pub(super) scopes: Vec<Vec<Block>>,
    /// The register the last compare wrote, so a branch on it can fuse.
    pub(super) last_compare: Option<(usize, u32)>,
}

pub(super) fn aggregate_offset(cx: &IrCx, ty: Ty, index: usize) -> Result<u64, String> {
    match cx.ty_kind(ty) {
        TyKind::Array { elem, .. } => Ok(abi::size(cx, *elem) * index as u64),
        TyKind::Struct(_) | TyKind::Slice { .. } | TyKind::String => {
            abi::field_offset(cx, ty, index).ok_or_else(|| "aggregate field without ABI offset".to_string())
        }
        _ => Err("aggregate of a non-aggregate type".to_string()),
    }
}

/// Locals that must live in memory: their address is taken, or `InterpFill` writes them.
/// A ref local read through `Deref` keeps its register.
pub(super) fn addressed_locals(block: &Block, count: usize) -> Vec<bool> {
    let mut marked = vec![false; count];
    walk_block(block, &mut |place, force| {
        if let PlaceRoot::Local(id) = place.root {
            let deref_first = matches!(place.proj.first(), Some(Proj::Deref));
            if force && !deref_first {
                marked[id.0 as usize] = true;
            }
        }
    });
    marked
}

pub(super) fn count_block(block: &Block, uses: &mut [u32]) {
    walk_block(block, &mut |place, _| {
        if let PlaceRoot::Local(id) = place.root {
            uses[id.0 as usize] += 1;
        }
    });
}

/// Visit every place; `force` is true where the place's address is taken.
pub(super) fn walk_block(block: &Block, f: &mut dyn FnMut(&Place, bool)) {
    for stmt in &block.stmts {
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                walk_place(place, false, f);
                walk_rvalue(rvalue, f);
            }
            StmtKind::AssignOp { place, rhs, .. } => {
                walk_place(place, false, f);
                walk_operand(rhs, f);
            }
            StmtKind::Defer { body } | StmtKind::Block(body) => walk_block(body, f),
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                walk_operand(cond, f);
                walk_block(then_block, f);
                if let Some(other) = else_block {
                    walk_block(other, f);
                }
            }
            StmtKind::While { cond, body } => {
                walk_operand(cond, f);
                walk_block(body, f);
            }
            StmtKind::Switch { scrut, cases, default } => {
                walk_operand(scrut, f);
                for (_, b) in cases {
                    walk_block(b, f);
                }
                walk_block(default, f);
            }
            StmtKind::Return { value } => {
                if let Some(op) = value {
                    walk_operand(op, f);
                }
            }
            StmtKind::Expr(rv) => walk_rvalue(rv, f),
            StmtKind::ForRange { index, end, body } => {
                walk_place(index, false, f);
                walk_operand(end, f);
                walk_block(body, f);
            }
            StmtKind::InterpFill { buf, used, parts } => {
                walk_place(buf, true, f);
                walk_place(used, true, f);
                for part in parts {
                    if let InterpPart::Value(op) = part {
                        walk_operand(op, f);
                    }
                }
            }
            StmtKind::Break(_) | StmtKind::Continue(_) | StmtKind::Abort(..) | StmtKind::Div0 => {}
        }
    }
}

pub(super) fn walk_rvalue(rv: &Rvalue, f: &mut dyn FnMut(&Place, bool)) {
    match &rv.kind {
        RvalueKind::Use(op) | RvalueKind::Unary { arg: op, .. } | RvalueKind::Cast { from: op } => {
            walk_operand(op, f)
        }
        RvalueKind::Ref { place } => walk_place(place, true, f),
        RvalueKind::Binary { lhs, rhs, .. } => {
            walk_operand(lhs, f);
            walk_operand(rhs, f);
        }
        RvalueKind::Call { func, args } => {
            walk_operand(func, f);
            for arg in args {
                walk_operand(arg, f);
            }
        }
        RvalueKind::Syscall { nr, args } => {
            walk_operand(nr, f);
            for arg in args {
                walk_operand(arg, f);
            }
        }
        RvalueKind::Atomic { args, .. } | RvalueKind::Simd { args, .. } => {
            for arg in args {
                walk_operand(arg, f);
            }
        }
        RvalueKind::Aggregate { fields } => {
            for field in fields {
                walk_operand(field, f);
            }
        }
        RvalueKind::Enum { payload, .. } => {
            if let Some(op) = payload {
                walk_operand(op, f);
            }
        }
        RvalueKind::Interpolate { parts } => {
            for part in parts {
                if let InterpPart::Value(op) = part {
                    walk_operand(op, f);
                }
            }
        }
    }
}

pub(super) fn walk_operand(op: &Operand, f: &mut dyn FnMut(&Place, bool)) {
    if let Operand::Copy(place) = op {
        walk_place(place, false, f);
    }
}

pub(super) fn walk_place(place: &Place, force: bool, f: &mut dyn FnMut(&Place, bool)) {
    f(place, force);
    for proj in &place.proj {
        if let Proj::Index { index, .. } = proj {
            walk_operand(index, f);
        }
    }
}

pub(super) fn align_up(value: u32, align: u32) -> u32 {
    (value + align - 1) / align * align
}
impl<'a> Codegen<'a> {
    pub(super) fn new(cx: &'a IrCx, func: &'a Func, body: &'a Body, shared: &'a mut Shared) -> Codegen<'a> {
        let addressed = addressed_locals(&body.block, body.locals.len());
        let mut uses = vec![0u32; body.locals.len()];
        count_block(&body.block, &mut uses);
        let mut mem = Vec::new();
        let mut value = Vec::new();
        let mut frame_size = 0u32;
        let mut regs = 1u32;
        for (i, local) in body.locals.iter().enumerate() {
            if is_scalar(cx, local.ty) && !addressed[i] {
                value.push(Some(regs));
                regs += 1;
                mem.push(None);
            } else {
                let align = (abi::align(cx, local.ty) as u32).max(8);
                frame_size = align_up(frame_size, align);
                mem.push(Some(frame_size));
                frame_size += (abi::size(cx, local.ty) as u32).max(8);
                value.push(None);
            }
        }
        Codegen {
            cx,
            func,
            body,
            shared,
            mem,
            value,
            uses,
            frame_size,
            regs,
            prologue: Vec::new(),
            const_regs: HashMap::new(),
            code: Vec::new(),
            calls: Vec::new(),
            loops: Vec::new(),
            scopes: Vec::new(),
            last_compare: None,
        }
    }

    pub(super) fn emit(mut self) -> Result<BcFunction, String> {
        if let Some(v) = self.body.locals.iter().find_map(|l| match self.cx.ty_kind(l.ty) {
            crate::ir::ty::TyKind::Vec(v) => Some(*v),
            _ => None,
        }) {
            return Err(format!("{} is not supported in scripts yet", v.name()));
        }
        self.block(&self.body.block)?;
        if !matches!(
            self.code.last().map(|i| i.op),
            Some(Op::Ret | Op::RetV | Op::RetA)
        ) {
            self.push(Instr::new(Op::Ret, NO_REG, NO_REG, NO_REG, 0));
        }
        let shift = self.prologue.len() as u64;
        for ins in &mut self.code {
            if ins.op.is_jump() {
                ins.imm += shift;
            }
        }
        let mut code = std::mem::take(&mut self.prologue);
        code.append(&mut self.code);
        let mut params = Vec::new();
        for id in &self.body.params {
            let local = self.body.local(*id);
            let size = abi::size(self.cx, local.ty) as u32;
            let load = class(self.cx, local.ty)
                .filter(|c| *c != Class::Void)
                .map(|c| load_op(c) as u16)
                .unwrap_or(0);
            params.push(BcParam {
                reg: self.value[id.0 as usize].unwrap_or(NO_REG),
                offset: self.mem[id.0 as usize].unwrap_or(0),
                size,
                load,
            });
        }
        let (_, ret) = self
            .cx
            .fn_sig(self.func.ty)
            .ok_or_else(|| "function without a signature".to_string())?;
        Ok(BcFunction {
            name: self.cx.name_str(self.func.name).to_string(),
            key: fn_key(self.cx, self.func.id),
            signature: dump::ty_string(self.cx, self.func.ty),
            public: self.func.vis == Vis::Pub,
            frame_size: align_up(self.frame_size, 8),
            regs: self.regs,
            params,
            ret_size: abi::size(self.cx, ret) as u32,
            ret_aggregate: !is_scalar(self.cx, ret) && class(self.cx, ret) != Some(Class::Void),
            calls: self.calls,
            code,
        })
    }

    pub(super) fn reg(&mut self) -> u32 {
        let r = self.regs;
        self.regs += 1;
        r
    }

    pub(super) fn push(&mut self, ins: Instr) -> usize {
        let at = self.code.len();
        self.code.push(ins);
        self.last_compare = None;
        at
    }

    pub(super) fn patch(&mut self, at: usize, target: usize) {
        self.code[at].imm = target as u64;
    }

    pub(super) fn here(&self) -> usize {
        self.code.len()
    }

    pub(super) fn const_reg(&mut self, bits: u64) -> u32 {
        if let Some(reg) = self.const_regs.get(&bits) {
            return *reg;
        }
        let dst = self.reg();
        self.const_regs.insert(bits, dst);
        self.prologue.push(Instr::new(Op::Const, dst, NO_REG, NO_REG, bits));
        dst
    }

    pub(super) fn temp_mem(&mut self, ty: Ty) -> Addr {
        let align = (abi::align(self.cx, ty) as u32).max(8);
        self.frame_size = align_up(self.frame_size, align);
        let offset = self.frame_size;
        self.frame_size += (abi::size(self.cx, ty) as u32).max(8);
        Addr {
            base: FP,
            offset: offset as u64,
        }
    }

    pub(super) fn materialize(&mut self, at: Addr) -> u32 {
        if at.offset == 0 && at.base != FP {
            return at.base;
        }
        let dst = self.reg();
        self.push(Instr::new(Op::Addr, dst, at.base, NO_REG, at.offset));
        dst
    }

    pub(super) fn load(&mut self, at: Addr, ty: Ty) -> Result<u32, String> {
        let c = class(self.cx, ty).ok_or("load of an aggregate")?;
        let dst = self.reg();
        self.push(Instr::new(load_op(c), dst, at.base, NO_REG, at.offset));
        Ok(dst)
    }

    pub(super) fn store(&mut self, at: Addr, value: u32, ty: Ty) {
        let size = abi::size(self.cx, ty);
        if size == 0 {
            return;
        }
        if is_scalar(self.cx, ty) {
            self.push(Instr::new(store_op(size), NO_REG, at.base, value, at.offset));
        } else {
            let dst = self.materialize(at);
            self.push(Instr::new(Op::Copy, NO_REG, dst, value, size));
        }
    }

    pub(super) fn place_addr(&mut self, place: &Place) -> Result<Addr, String> {
        let mut projs = place.proj.as_slice();
        let (mut at, mut ty) = match place.root {
            PlaceRoot::Local(id) => {
                let local = self.body.local(id);
                match (self.mem[id.0 as usize], self.value[id.0 as usize]) {
                    (Some(offset), _) => (
                        Addr {
                            base: FP,
                            offset: offset as u64,
                        },
                        local.ty,
                    ),
                    (None, Some(reg)) => {
                        let Some((Proj::Deref, rest)) = projs.split_first() else {
                            return Err("address of a register local".to_string());
                        };
                        projs = rest;
                        let inner = self
                            .cx
                            .ref_inner(local.ty)
                            .ok_or_else(|| "deref of a non-ref local".to_string())?;
                        (Addr { base: reg, offset: 0 }, inner)
                    }
                    (None, None) => return Err("local without storage".to_string()),
                }
            }
            PlaceRoot::Global(id) => {
                let index = self
                    .shared
                    .globals
                    .iter()
                    .position(|g| *g == id)
                    .ok_or_else(|| {
                        format!(
                            "`{}` is another module's value; scripts reach only their own `shared`",
                            self.cx.name_str(self.cx.global(id).name)
                        )
                    })?;
                let dst = self.reg();
                self.push(Instr::new(Op::Global, dst, NO_REG, NO_REG, index as u64));
                (Addr { base: dst, offset: 0 }, self.cx.global(id).ty)
            }
        };
        for proj in projs {
            match proj {
                Proj::Lane { .. } => return Err("vec4f is not supported in scripts yet".to_string()),
                Proj::Deref => {
                    let dst = self.reg();
                    self.push(Instr::new(Op::Ld64, dst, at.base, NO_REG, at.offset));
                    at = Addr { base: dst, offset: 0 };
                    ty = self.cx.ref_inner(ty).ok_or("deref of a non-ref place")?;
                }
                Proj::Field { field, ty: field_ty } => {
                    at.offset += abi::field_offset(self.cx, ty, field.index as usize)
                        .ok_or("field without an ABI offset")?;
                    ty = *field_ty;
                }
                Proj::Index { index, ty: elem } => {
                    let data = if matches!(
                        self.cx.ty_kind(ty),
                        TyKind::Slice { .. } | TyKind::String
                    ) {
                        let dst = self.reg();
                        self.push(Instr::new(Op::Ld64, dst, at.base, NO_REG, at.offset));
                        dst
                    } else {
                        self.materialize(at)
                    };
                    let idx = self.operand(index)?;
                    let dst = self.reg();
                    self.push(Instr::new(
                        Op::Idx,
                        dst,
                        data,
                        idx,
                        abi::size(self.cx, *elem),
                    ));
                    at = Addr { base: dst, offset: 0 };
                    ty = *elem;
                }
                Proj::Ptr => {
                    ty = self.cx.common.ptr;
                }
                Proj::Len => {
                    at.offset += 8;
                    ty = self.cx.common.int;
                }
                Proj::EnumTag => {
                    ty = self.cx.common.i32;
                }
                Proj::EnumPayload { ty: payload, .. } => {
                    at.offset += abi::field_offset(self.cx, ty, 1)
                        .ok_or("enum payload without an ABI offset")?;
                    ty = *payload;
                }
            }
        }
        Ok(at)
    }

    pub(super) fn local_reg(&self, place: &Place) -> Option<u32> {
        if !place.proj.is_empty() {
            return None;
        }
        let PlaceRoot::Local(id) = place.root else {
            return None;
        };
        self.value[id.0 as usize]
    }

    /// A scalar operand's register, or an aggregate operand's address register.
    pub(super) fn operand(&mut self, operand: &Operand) -> Result<u32, String> {
        match operand {
            Operand::Const(c) => {
                if let ConstKind::Bytes(bytes) = &c.kind {
                    let index = self.shared.constants.len() as u64;
                    self.shared.constants.push(bytes.clone());
                    let at = self.temp_mem(self.cx.common.string);
                    let dst = self.materialize(at);
                    self.push(Instr::new(Op::Str, NO_REG, dst, NO_REG, index));
                    return Ok(dst);
                }
                let bits = match c.kind {
                    ConstKind::Void | ConstKind::Null => 0,
                    ConstKind::Bool(v) => u64::from(v),
                    ConstKind::Int(v) => v,
                    ConstKind::F32Bits(v) => v as u64,
                    ConstKind::F64Bits(v) => v,
                    ConstKind::FnAddr(id) => {
                        return Err(format!(
                            "taking the address of `{}` isn't supported in scripts",
                            self.cx.name_str(self.cx.func(id).name)
                        ))
                    }
                    ConstKind::Bytes(_) => unreachable!(),
                };
                let c = class(self.cx, c.ty).unwrap_or(Class::Ptr);
                Ok(self.const_reg(canonical(c, bits)))
            }
            Operand::Copy(place) => {
                if !is_scalar(self.cx, place.ty) {
                    let at = self.place_addr(place)?;
                    return Ok(self.materialize(at));
                }
                if let Some(reg) = self.local_reg(place) {
                    return Ok(reg);
                }
                let at = self.place_addr(place)?;
                self.load(at, place.ty)
            }
        }
    }

    pub(super) fn block(&mut self, block: &Block) -> Result<(), String> {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            self.stmt(stmt)?;
        }
        let defers = self.scopes.pop().expect("scope");
        for defer in defers.iter().rev() {
            self.block(defer)?;
        }
        Ok(())
    }

    pub(super) fn unwind_to(&mut self, depth: usize) -> Result<(), String> {
        let defers: Vec<Block> = self
            .scopes
            .iter()
            .skip(depth)
            .rev()
            .flat_map(|ds| ds.iter().rev().cloned())
            .collect();
        for defer in &defers {
            self.block(defer)?;
        }
        Ok(())
    }

    /// Emit "jump to `target` when `cond` is false", fusing a compare that just wrote it.
    pub(super) fn jump_false(&mut self, cond: &Operand, fuse: bool) -> Result<usize, String> {
        let reg = self.operand(cond)?;
        if let (true, Some((at, written)), Operand::Copy(place)) = (fuse, self.last_compare, cond) {
            let single_use = match place.root {
                PlaceRoot::Local(id) if place.proj.is_empty() => self.uses[id.0 as usize] <= 2,
                _ => false,
            };
            if written == reg && single_use && at + 1 == self.code.len() {
                let cmp = self.code.pop().expect("compare");
                let fused = match cmp.op {
                    Op::Lts => Op::JnLts,
                    Op::Les => Op::JnLes,
                    Op::Ltu => Op::JnLtu,
                    Op::Leu => Op::JnLeu,
                    Op::Eq => Op::JnEq,
                    Op::Ne => Op::JnNe,
                    Op::Flt => Op::JnFlt,
                    Op::Fle => Op::JnFle,
                    Op::Dlt => Op::JnDlt,
                    Op::Dle => Op::JnDle,
                    _ => unreachable!(),
                };
                return Ok(self.push(Instr::new(fused, NO_REG, cmp.a, cmp.b, 0)));
            }
        }
        Ok(self.push(Instr::new(Op::Jz, NO_REG, reg, NO_REG, 0)))
    }
}
