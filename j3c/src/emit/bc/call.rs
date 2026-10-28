use super::*;

impl<'a> Codegen<'a> {
    /// Evaluate an aggregate rvalue into the memory at `dst` (an address register).
    pub(super) fn aggregate_into(&mut self, dst: u32, rv: &Rvalue) -> Result<(), String> {
        match &rv.kind {
            RvalueKind::Use(op) => {
                let src = self.operand(op)?;
                let size = abi::size(self.cx, rv.ty);
                if src != dst && size > 0 {
                    self.push(Instr::new(Op::Copy, NO_REG, dst, src, size));
                }
            }
            RvalueKind::Call { func, args } => self.call(dst, rv.ty, func, args)?,
            RvalueKind::Aggregate { fields } => {
                for (i, field) in fields.iter().enumerate() {
                    let offset = aggregate_offset(self.cx, rv.ty, i)?;
                    let value = self.operand(field)?;
                    self.store(Addr { base: dst, offset }, value, field.ty());
                }
            }
            RvalueKind::Enum { variant, payload } => {
                let tag = self.const_reg(variant.index as u64);
                self.push(Instr::new(Op::St32, NO_REG, dst, tag, 0));
                if let Some(payload) = payload {
                    let offset = abi::field_offset(self.cx, rv.ty, 1)
                        .ok_or("enum payload without an ABI offset")?;
                    let value = self.operand(payload)?;
                    self.store(Addr { base: dst, offset }, value, payload.ty());
                }
            }
            RvalueKind::Syscall { .. } => {
                return Err("`@syscall` isn't supported in scripts".to_string())
            }
            RvalueKind::Atomic { .. } => {
                return Err("atomics aren't supported in scripts".to_string())
            }
            RvalueKind::Interpolate { .. } => {
                return Err("interpolate must be lowered before bytecode".to_string())
            }
            _ => return Err("scalar value in an aggregate slot".to_string()),
        }
        Ok(())
    }

    pub(super) fn call(&mut self, dst: u32, ret: Ty, func: &Operand, args: &[Operand]) -> Result<(), String> {
        let id = match func {
            Operand::Const(c) => match c.kind {
                ConstKind::FnAddr(id) => id,
                _ => return Err("call of a non-function constant".to_string()),
            },
            Operand::Copy(_) => {
                return Err("calling a function value isn't supported in scripts".to_string())
            }
        };
        let target = self.cx.func(id);
        let (kind, index) = if let Some(index) = self.shared.local_index.get(&id) {
            (CallKind::Local, *index)
        } else if is_script_module(self.cx, &target.module) {
            let path = script_path(self.cx, &target.module);
            if !self.shared.scripts.contains(&path) {
                self.shared.scripts.push(path);
            }
            (CallKind::Script, 0)
        } else {
            let key = fn_key(self.cx, id);
            if !self.shared.natives.iter().any(|(k, _)| *k == key) {
                self.shared.natives.push((key, fn_label(self.cx, id)));
            }
            (CallKind::Native, 0)
        };
        let mut lowered = Vec::new();
        for arg in args {
            lowered.push(CallArg {
                reg: self.operand(arg)?,
                size: abi::size(self.cx, arg.ty()) as u32,
                aggregate: !is_scalar(self.cx, arg.ty()),
            });
        }
        let ret_class = class(self.cx, ret);
        let site = self.calls.len() as u64;
        self.calls.push(CallSite {
            kind,
            key: fn_key(self.cx, id),
            index,
            ret_size: abi::size(self.cx, ret) as u32,
            ret_load: match ret_class {
                Some(Class::Void) | None => 0,
                Some(c) => load_op(c) as u16,
            },
            ret_aggregate: ret_class.is_none(),
            args: lowered,
        });
        self.push(Instr::new(Op::Call, dst, NO_REG, NO_REG, site));
        Ok(())
    }

    pub(super) fn append_literal(&mut self, buf: u32, used: u32, bytes: &[u8]) {
        let index = self.shared.constants.len() as u64;
        self.shared.constants.push(bytes.to_vec());
        self.push(Instr::new(Op::AppLit, buf, used, NO_REG, index));
    }

    pub(super) fn append_value(&mut self, buf: u32, used: u32, op: &Operand) -> Result<(), String> {
        let ty = op.ty();
        if matches!(self.cx.ty_kind(ty), TyKind::String) {
            let value = self.operand(op)?;
            self.push(Instr::new(Op::AppStr, buf, used, value, 0));
            return Ok(());
        }
        match class(self.cx, ty) {
            Some(Class::Bool) => {
                let value = self.operand(op)?;
                self.push(Instr::new(Op::AppBool, buf, used, value, 0));
                return Ok(());
            }
            Some(Class::Int { .. }) => {
                let value = self.operand(op)?;
                self.push(Instr::new(Op::AppInt, buf, used, value, 0));
                return Ok(());
            }
            Some(Class::F32) => {
                let value = self.operand(op)?;
                let wide = self.reg();
                self.push(Instr::new(Op::F2d, wide, value, NO_REG, 0));
                self.push(Instr::new(Op::AppF64, buf, used, wide, 0));
                return Ok(());
            }
            Some(Class::F64) => {
                let value = self.operand(op)?;
                self.push(Instr::new(Op::AppF64, buf, used, value, 0));
                return Ok(());
            }
            _ => {}
        }
        if let TyKind::Struct(id) = self.cx.ty_kind(ty) {
            let def = self.cx.struct_def(*id);
            if !def.debug {
                return Err("interpolate of a non-#[Debug] struct".to_string());
            }
            let Operand::Copy(base) = op else {
                return Err("interpolated struct must be a place".to_string());
            };
            self.append_literal(buf, used, format!("{} {{ ", self.cx.name_str(def.name)).as_bytes());
            for (index, field) in def.fields.iter().enumerate() {
                if index > 0 {
                    self.append_literal(buf, used, b", ");
                }
                self.append_literal(
                    buf,
                    used,
                    format!("{}: ", self.cx.name_str(field.name)).as_bytes(),
                );
                let place = base.clone().project(
                    Proj::Field {
                        field: FieldId {
                            struct_id: *id,
                            index: index as u32,
                        },
                        ty: field.ty,
                    },
                    field.ty,
                    base.access,
                    base.span,
                );
                self.append_value(buf, used, &Operand::Copy(place))?;
            }
            self.append_literal(buf, used, b" }");
            return Ok(());
        }
        Err("interpolate of this type isn't supported in scripts".to_string())
    }
}
