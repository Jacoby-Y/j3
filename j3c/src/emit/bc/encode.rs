use super::*;

impl BcModule {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(*b"J3SB");
        u16_out(&mut out, VERSION);
        u16_out(&mut out, 0);
        u64_out(&mut out, self.layout_hash);
        u64_out(&mut out, self.signature_hash);
        string_out(&mut out, &self.name);
        string_out(&mut out, &self.path);
        u32_out(&mut out, self.instance_size);
        u32_out(&mut out, self.props);
        u32_out(&mut out, self.fields.len() as u32);
        for f in &self.fields {
            string_out(&mut out, &f.name);
            u32_out(&mut out, f.offset);
            u32_out(&mut out, f.size);
            bytes_out(&mut out, &f.default);
            u32_out(&mut out, f.default_const.unwrap_or(u32::MAX));
        }
        u32_out(&mut out, self.globals.len() as u32);
        for g in &self.globals {
            u64_out(&mut out, g.key);
            string_out(&mut out, &g.name);
            u32_out(&mut out, g.size);
            bytes_out(&mut out, &g.init);
            u32_out(&mut out, g.init_const.unwrap_or(u32::MAX));
        }
        u32_out(&mut out, self.constants.len() as u32);
        for c in &self.constants {
            bytes_out(&mut out, c);
        }
        u32_out(&mut out, self.natives.len() as u32);
        for (key, name) in &self.natives {
            u64_out(&mut out, *key);
            string_out(&mut out, name);
        }
        u32_out(&mut out, self.scripts.len() as u32);
        for path in &self.scripts {
            string_out(&mut out, path);
        }
        u32_out(&mut out, self.functions.len() as u32);
        for f in &self.functions {
            u64_out(&mut out, f.key);
            string_out(&mut out, &f.name);
            string_out(&mut out, &f.signature);
            u32_out(&mut out, u32::from(f.public));
            u32_out(&mut out, f.frame_size);
            u32_out(&mut out, f.regs);
            u32_out(&mut out, f.ret_size);
            u32_out(&mut out, u32::from(f.ret_aggregate));
            u32_out(&mut out, f.params.len() as u32);
            for p in &f.params {
                u32_out(&mut out, p.reg);
                u32_out(&mut out, p.offset);
                u32_out(&mut out, p.size);
                u32_out(&mut out, p.load as u32);
            }
            u32_out(&mut out, f.calls.len() as u32);
            for call in &f.calls {
                u32_out(&mut out, call.kind as u32);
                u64_out(&mut out, call.key);
                u32_out(&mut out, call.index);
                u32_out(&mut out, call.ret_size);
                u32_out(&mut out, call.ret_load as u32);
                u32_out(&mut out, u32::from(call.ret_aggregate));
                u32_out(&mut out, call.args.len() as u32);
                for arg in &call.args {
                    u32_out(&mut out, arg.reg);
                    u32_out(&mut out, arg.size);
                    u32_out(&mut out, u32::from(arg.aggregate));
                }
            }
            u32_out(&mut out, f.code.len() as u32);
            // Pad so the instruction array starts 8-aligned in the file.
            while out.len() % 8 != 0 {
                out.push(0);
            }
            for ins in &f.code {
                u16_out(&mut out, ins.op as u16);
                u16_out(&mut out, ins.aux);
                u32_out(&mut out, ins.dst);
                u32_out(&mut out, ins.a);
                u32_out(&mut out, ins.b);
                u64_out(&mut out, ins.imm);
            }
        }
        out
    }

    pub fn dump(&self) -> String {
        let mut out = format!(
            "j3sb {VERSION} {} ({})\nlayout {:016x} signatures {:016x}\ninstance {} bytes, {} props\n",
            self.name,
            self.path,
            self.layout_hash,
            self.signature_hash,
            self.instance_size,
            self.props
        );
        for f in &self.fields {
            out.push_str(&format!(
                "  field {} @{} size {}{}\n",
                f.name,
                f.offset,
                f.size,
                if f.default.is_empty() { "" } else { " default" }
            ));
        }
        for g in &self.globals {
            out.push_str(&format!("  shared {} size {}\n", g.name, g.size));
        }
        for (_, name) in &self.natives {
            out.push_str(&format!("  native {name}\n"));
        }
        for path in &self.scripts {
            out.push_str(&format!("  script {path}\n"));
        }
        for f in &self.functions {
            out.push_str(&format!(
                "\n{}fn {} [{}] frame {} regs {}\n",
                if f.public { "pub " } else { "" },
                f.name,
                f.signature,
                f.frame_size,
                f.regs
            ));
            for (pc, ins) in f.code.iter().enumerate() {
                out.push_str(&format!("  {pc:04} {}", dump_instr(ins, f)));
                out.push('\n');
            }
        }
        out
    }

    /// Write `target/j3s/<path>.j3sb` under `root`. Skips the write when the bytes match.
    pub fn write_to(&self, root: &std::path::Path) -> Result<std::path::PathBuf, String> {
        let path = bytecode_path(root, &self.path);
        let bytes = self.encode();
        if std::fs::read(&path).map(|old| old == bytes).unwrap_or(false) {
            return Ok(path);
        }
        std::fs::create_dir_all(path.parent().expect("bytecode parent"))
            .map_err(|e| format!("create {}: {e}", path.display()))?;
        std::fs::write(&path, bytes).map_err(|e| format!("write {}: {e}", path.display()))?;
        Ok(path)
    }
}

/// Where a script's bytecode lives under a project root.
pub fn bytecode_path(root: &std::path::Path, script: &str) -> std::path::PathBuf {
    root.join("target/j3s")
        .join(script)
        .with_extension("j3sb")
}

pub(super) fn reg_name(r: u32) -> String {
    if r == NO_REG {
        "_".to_string()
    } else if r == FP {
        "fp".to_string()
    } else {
        format!("r{r}")
    }
}

pub(super) fn dump_instr(ins: &Instr, f: &BcFunction) -> String {
    let name = ins.op.name();
    let d = reg_name(ins.dst);
    let a = reg_name(ins.a);
    let b = reg_name(ins.b);
    match ins.op {
        Op::Const => format!("{name} {d}, {:#x}", ins.imm),
        Op::Global => format!("{name} {d}, shared#{}", ins.imm),
        Op::Str => format!("{name} [{a}], const#{}", ins.imm),
        Op::Addr => format!("{name} {d}, {a}+{}", ins.imm),
        Op::Idx => format!("{name} {d}, {a}+{b}*{}", ins.imm),
        Op::Ld8s | Op::Ld8u | Op::Ld16s | Op::Ld16u | Op::Ld32s | Op::Ld32u | Op::Ld64 => {
            format!("{name} {d}, [{a}+{}]", ins.imm)
        }
        Op::St8 | Op::St16 | Op::St32 | Op::St64 => format!("{name} [{a}+{}], {b}", ins.imm),
        Op::Copy => format!("{name} [{a}], [{b}], {}", ins.imm),
        Op::Jmp => format!("{name} {:04}", ins.imm),
        Op::Jz | Op::Jnz => format!("{name} {a}, {:04}", ins.imm),
        op if op.is_jump() => format!("{name} {a}, {b}, {:04}", ins.imm),
        Op::Call => {
            let site = &f.calls[ins.imm as usize];
            let target = match site.kind {
                CallKind::Local => format!("local#{}", site.index),
                CallKind::Script => format!("script {:016x}", site.key),
                CallKind::Native => format!("native {:016x}", site.key),
            };
            let args = site
                .args
                .iter()
                .map(|a| {
                    if a.aggregate {
                        format!("[{}]", reg_name(a.reg))
                    } else {
                        reg_name(a.reg)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {d} = {target}({args})")
        }
        Op::Ret | Op::Abort | Op::Div0 => name.to_string(),
        Op::RetV => format!("{name} {a}"),
        Op::RetA => format!("{name} [{a}], {}", ins.imm),
        Op::AppLit => format!("{name} [{d}], [{a}], const#{}", ins.imm),
        Op::AppStr | Op::AppInt | Op::AppF64 | Op::AppBool => format!("{name} [{d}], [{a}], {b}"),
        Op::Mov
        | Op::Neg
        | Op::Not
        | Op::Fneg
        | Op::Dneg
        | Op::Sext8
        | Op::Sext16
        | Op::Sext32
        | Op::Zext8
        | Op::Zext16
        | Op::Zext32
        | Op::I2f
        | Op::U2f
        | Op::F2i
        | Op::F2u
        | Op::I2d
        | Op::U2d
        | Op::D2i
        | Op::D2u
        | Op::F2d
        | Op::D2f => format!("{name} {d}, {a}"),
        _ => format!("{name} {d}, {a}, {b}"),
    }
}

pub(super) fn u16_out(out: &mut Vec<u8>, n: u16) {
    out.extend(n.to_le_bytes());
}
pub(super) fn u32_out(out: &mut Vec<u8>, n: u32) {
    out.extend(n.to_le_bytes());
}
pub(super) fn u64_out(out: &mut Vec<u8>, n: u64) {
    out.extend(n.to_le_bytes());
}
pub(super) fn bytes_out(out: &mut Vec<u8>, bytes: &[u8]) {
    u32_out(out, bytes.len() as u32);
    out.extend(bytes);
}
pub(super) fn string_out(out: &mut Vec<u8>, s: &str) {
    bytes_out(out, s.as_bytes());
}
