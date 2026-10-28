use super::*;

impl<'a> Codegen<'a> {
    pub(super) fn stmt(&mut self, stmt: &Stmt) -> Result<(), String> {
        match &stmt.kind {
            StmtKind::Assign { place, rvalue } => {
                if let Some(reg) = self.local_reg(place) {
                    self.scalar_into(rvalue, reg)?;
                } else if is_scalar(self.cx, place.ty) {
                    let value = self.scalar(rvalue)?;
                    let at = self.place_addr(place)?;
                    self.store(at, value, place.ty);
                } else {
                    let at = self.place_addr(place)?;
                    let dst = self.materialize(at);
                    self.aggregate_into(dst, rvalue)?;
                }
            }
            StmtKind::AssignOp { place, op, rhs } => {
                let value = self.operand(rhs)?;
                if let Some(reg) = self.local_reg(place) {
                    self.binary(reg, *op, place.ty, reg, value)?;
                } else {
                    let at = self.place_addr(place)?;
                    let old = self.load(at, place.ty)?;
                    let new = self.reg();
                    self.binary(new, *op, place.ty, old, value)?;
                    self.store(at, new, place.ty);
                }
            }
            StmtKind::Defer { body } => self
                .scopes
                .last_mut()
                .expect("defer scope")
                .push(body.clone()),
            StmtKind::If {
                cond,
                then_block,
                else_block,
            } => {
                let fail = self.jump_false(cond, true)?;
                self.block(then_block)?;
                if let Some(else_block) = else_block {
                    let end = self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, 0));
                    let here = self.here();
                    self.patch(fail, here);
                    self.block(else_block)?;
                    let here = self.here();
                    self.patch(end, here);
                } else {
                    let here = self.here();
                    self.patch(fail, here);
                }
            }
            StmtKind::While { cond, body } => {
                let header = self.here();
                let exit = self.jump_false(cond, false)?;
                self.loops.push(LoopFixups {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scopes.len(),
                });
                self.block(body)?;
                self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, header as u64));
                let end = self.here();
                self.patch(exit, end);
                let fixes = self.loops.pop().expect("while loop");
                for at in fixes.breaks {
                    self.patch(at, end);
                }
                for at in fixes.continues {
                    self.patch(at, header);
                }
            }
            StmtKind::ForRange { index, end, body } => {
                let signed = matches!(
                    class(self.cx, index.ty),
                    Some(Class::Int { signed: true, .. })
                );
                let header = self.here();
                let (index_reg, index_mem) = match self.local_reg(index) {
                    Some(reg) => (reg, None),
                    None => {
                        let at = self.place_addr(index)?;
                        (self.load(at, index.ty)?, Some(at))
                    }
                };
                let end_reg = self.operand(end)?;
                let jump = if signed { Op::JnLts } else { Op::JnLtu };
                let exit = self.push(Instr::new(jump, NO_REG, index_reg, end_reg, 0));
                self.loops.push(LoopFixups {
                    breaks: Vec::new(),
                    continues: Vec::new(),
                    scope_depth: self.scopes.len(),
                });
                self.block(body)?;
                let increment = self.here();
                let one = self.const_reg(1);
                match index_mem {
                    None => {
                        self.binary(index_reg, BinOp::Add, index.ty, index_reg, one)?;
                    }
                    Some(at) => {
                        let old = self.load(at, index.ty)?;
                        let new = self.reg();
                        self.binary(new, BinOp::Add, index.ty, old, one)?;
                        self.store(at, new, index.ty);
                    }
                }
                self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, header as u64));
                let finish = self.here();
                self.patch(exit, finish);
                let fixes = self.loops.pop().expect("range loop");
                for at in fixes.breaks {
                    self.patch(at, finish);
                }
                for at in fixes.continues {
                    self.patch(at, increment);
                }
            }
            StmtKind::Return { value } => {
                self.unwind_to(0)?;
                match value {
                    None => {
                        self.push(Instr::new(Op::Ret, NO_REG, NO_REG, NO_REG, 0));
                    }
                    Some(v) if class(self.cx, v.ty()) == Some(Class::Void) => {
                        self.push(Instr::new(Op::Ret, NO_REG, NO_REG, NO_REG, 0));
                    }
                    Some(v) => {
                        let reg = self.operand(v)?;
                        if is_scalar(self.cx, v.ty()) {
                            self.push(Instr::new(Op::RetV, NO_REG, reg, NO_REG, 0));
                        } else {
                            let size = abi::size(self.cx, v.ty());
                            self.push(Instr::new(Op::RetA, NO_REG, reg, NO_REG, size));
                        }
                    }
                }
            }
            StmtKind::Break | StmtKind::Continue => {
                let depth = self
                    .loops
                    .last()
                    .ok_or_else(|| "break/continue outside a loop".to_string())?
                    .scope_depth;
                self.unwind_to(depth)?;
                let at = self.push(Instr::new(Op::Jmp, NO_REG, NO_REG, NO_REG, 0));
                let fixes = self.loops.last_mut().expect("loop");
                if matches!(stmt.kind, StmtKind::Break) {
                    fixes.breaks.push(at);
                } else {
                    fixes.continues.push(at);
                }
            }
            StmtKind::Expr(rvalue) => {
                if is_scalar(self.cx, rvalue.ty) {
                    self.scalar(rvalue)?;
                } else if class(self.cx, rvalue.ty) == Some(Class::Void) {
                    self.void_rvalue(rvalue)?;
                } else {
                    let at = self.temp_mem(rvalue.ty);
                    let dst = self.materialize(at);
                    self.aggregate_into(dst, rvalue)?;
                }
            }
            StmtKind::Abort(_) => {
                self.push(Instr::new(Op::Abort, NO_REG, NO_REG, NO_REG, 0));
            }
            StmtKind::Div0 => {
                self.push(Instr::new(Op::Div0, NO_REG, NO_REG, NO_REG, 0));
            }
            StmtKind::Block(block) => self.block(block)?,
            StmtKind::InterpFill { buf, used, parts } => {
                let buf_at = self.place_addr(buf)?;
                let buf = self.materialize(buf_at);
                let used_at = self.place_addr(used)?;
                let used = self.materialize(used_at);
                for part in parts {
                    match part {
                        InterpPart::Lit(bytes) => self.append_literal(buf, used, bytes),
                        InterpPart::Value(op) => self.append_value(buf, used, op)?,
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn void_rvalue(&mut self, rv: &Rvalue) -> Result<(), String> {
        match &rv.kind {
            RvalueKind::Call { func, args } => self.call(NO_REG, rv.ty, func, args),
            RvalueKind::Use(_) => Ok(()),
            RvalueKind::Syscall { .. } => Err("`@syscall` isn't supported in scripts".to_string()),
            RvalueKind::Atomic { .. } => Err("atomics aren't supported in scripts".to_string()),
            _ => Err("unsupported void expression".to_string()),
        }
    }

    /// Evaluate a scalar rvalue into a fresh or existing register.
    pub(super) fn scalar(&mut self, rv: &Rvalue) -> Result<u32, String> {
        if let RvalueKind::Use(op) = &rv.kind {
            return self.operand(op);
        }
        let dst = self.reg();
        self.scalar_into(rv, dst)?;
        Ok(dst)
    }

    pub(super) fn scalar_into(&mut self, rv: &Rvalue, dst: u32) -> Result<(), String> {
        match &rv.kind {
            RvalueKind::Use(op) => {
                let src = self.operand(op)?;
                if src != dst {
                    self.push(Instr::new(Op::Mov, dst, src, NO_REG, 0));
                }
            }
            RvalueKind::Ref { place } => {
                let at = self.place_addr(place)?;
                self.push(Instr::new(Op::Addr, dst, at.base, NO_REG, at.offset));
            }
            RvalueKind::Binary { op, lhs, rhs } => {
                let a = self.operand(lhs)?;
                let b = self.operand(rhs)?;
                self.binary(dst, *op, lhs.ty(), a, b)?;
            }
            RvalueKind::Unary { op, arg } => {
                let a = self.operand(arg)?;
                let c = class(self.cx, arg.ty()).ok_or("unary on an aggregate")?;
                match (op, c) {
                    (UnaryOp::Not, _) => {
                        self.push(Instr::new(Op::Not, dst, a, NO_REG, 0));
                    }
                    (UnaryOp::Neg, Class::F32) => {
                        self.push(Instr::new(Op::Fneg, dst, a, NO_REG, 0));
                    }
                    (UnaryOp::Neg, Class::F64) => {
                        self.push(Instr::new(Op::Dneg, dst, a, NO_REG, 0));
                    }
                    (UnaryOp::Neg, _) => {
                        self.push(Instr::new(Op::Neg, dst, a, NO_REG, 0));
                        self.extend(dst, c);
                    }
                    (UnaryOp::BitNot, _) => {
                        return Err("bit operators aren't supported in scripts".to_string())
                    }
                }
            }
            RvalueKind::Cast { from } => {
                let a = self.operand(from)?;
                self.cast(dst, from.ty(), rv.ty, a)?;
            }
            RvalueKind::Call { func, args } => self.call(dst, rv.ty, func, args)?,
            RvalueKind::Syscall { .. } => {
                return Err("`@syscall` isn't supported in scripts".to_string())
            }
            RvalueKind::Atomic { .. } => {
                return Err("atomics aren't supported in scripts".to_string())
            }
            RvalueKind::Aggregate { .. } | RvalueKind::Enum { .. } => {
                return Err("aggregate value in a scalar slot".to_string())
            }
            RvalueKind::Interpolate { .. } => {
                return Err("interpolate must be lowered before bytecode".to_string())
            }
        }
        Ok(())
    }

    pub(super) fn extend(&mut self, reg: u32, c: Class) {
        if let Some(op) = ext_op(c) {
            self.push(Instr::new(op, reg, reg, NO_REG, 0));
        }
    }

    pub(super) fn binary(&mut self, dst: u32, op: BinOp, ty: Ty, a: u32, b: u32) -> Result<(), String> {
        let c = class(self.cx, ty).ok_or("binary op on an aggregate")?;
        let (code, swap, compare) = match c {
            Class::F32 | Class::F64 => {
                let d = c == Class::F64;
                let pick = |f: Op, g: Op| if d { g } else { f };
                match op {
                    BinOp::Add => (pick(Op::Fadd, Op::Dadd), false, false),
                    BinOp::Sub => (pick(Op::Fsub, Op::Dsub), false, false),
                    BinOp::Mul => (pick(Op::Fmul, Op::Dmul), false, false),
                    BinOp::Div => (pick(Op::Fdiv, Op::Ddiv), false, false),
                    BinOp::Eq => (pick(Op::Feq, Op::Deq), false, true),
                    BinOp::Ne => (pick(Op::Fne, Op::Dne), false, true),
                    BinOp::Lt => (pick(Op::Flt, Op::Dlt), false, true),
                    BinOp::Le => (pick(Op::Fle, Op::Dle), false, true),
                    BinOp::Gt => (pick(Op::Flt, Op::Dlt), true, true),
                    BinOp::Ge => (pick(Op::Fle, Op::Dle), true, true),
                    _ => return Err(format!("`{op:?}` on a float isn't supported in scripts")),
                }
            }
            _ => {
                let signed = matches!(c, Class::Int { signed: true, .. });
                match op {
                    BinOp::Add => (Op::Add, false, false),
                    BinOp::Sub => (Op::Sub, false, false),
                    BinOp::Mul => (Op::Mul, false, false),
                    BinOp::Div => (if signed { Op::Divs } else { Op::Divu }, false, false),
                    BinOp::Rem => (if signed { Op::Rems } else { Op::Remu }, false, false),
                    BinOp::Eq => (Op::Eq, false, true),
                    BinOp::Ne => (Op::Ne, false, true),
                    BinOp::Lt => (if signed { Op::Lts } else { Op::Ltu }, false, true),
                    BinOp::Le => (if signed { Op::Les } else { Op::Leu }, false, true),
                    BinOp::Gt => (if signed { Op::Lts } else { Op::Ltu }, true, true),
                    BinOp::Ge => (if signed { Op::Les } else { Op::Leu }, true, true),
                    BinOp::And => (Op::Land, false, false),
                    BinOp::Or => (Op::Lor, false, false),
                    BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr => {
                        return Err("bit operators aren't supported in scripts".to_string())
                    }
                }
            }
        };
        let (a, b) = if swap { (b, a) } else { (a, b) };
        let at = self.push(Instr::new(code, dst, a, b, 0));
        if compare {
            self.last_compare = Some((at, dst));
        } else {
            self.extend(dst, c);
        }
        Ok(())
    }

    pub(super) fn cast(&mut self, dst: u32, from: Ty, to: Ty, a: u32) -> Result<(), String> {
        let f = class(self.cx, from).ok_or("cast from an aggregate")?;
        let t = class(self.cx, to).ok_or("cast to an aggregate")?;
        let signed = |c: Class| matches!(c, Class::Int { signed: true, .. });
        let op = match (f, t) {
            (Class::F32, Class::F32) | (Class::F64, Class::F64) => Op::Mov,
            (Class::F32, Class::F64) => Op::F2d,
            (Class::F64, Class::F32) => Op::D2f,
            (Class::F32, _) => {
                if signed(t) || !matches!(t, Class::Int { bits: 64, .. }) {
                    Op::F2i
                } else {
                    Op::F2u
                }
            }
            (Class::F64, _) => {
                if signed(t) || !matches!(t, Class::Int { bits: 64, .. }) {
                    Op::D2i
                } else {
                    Op::D2u
                }
            }
            (_, Class::F32) => {
                if signed(f) {
                    Op::I2f
                } else {
                    Op::U2f
                }
            }
            (_, Class::F64) => {
                if signed(f) {
                    Op::I2d
                } else {
                    Op::U2d
                }
            }
            (_, Class::Bool) => {
                let zero = self.const_reg(0);
                self.push(Instr::new(Op::Ne, dst, a, zero, 0));
                return Ok(());
            }
            _ => Op::Mov,
        };
        self.push(Instr::new(op, dst, a, NO_REG, 0));
        self.extend(dst, t);
        Ok(())
    }
}
