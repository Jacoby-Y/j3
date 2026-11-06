/// Opcodes. The numbers are the file format; `stdlib/j3/script/opcodes.j3` has the same table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u16)]
pub enum Op {
    Const = 1,
    Mov = 2,
    Addr = 3,
    Idx = 4,
    Global = 5,
    Str = 6,
    Ld8s = 7,
    Ld8u = 8,
    Ld16s = 9,
    Ld16u = 10,
    Ld32s = 11,
    Ld32u = 12,
    Ld64 = 13,
    St8 = 14,
    St16 = 15,
    St32 = 16,
    St64 = 17,
    Copy = 18,
    Add = 19,
    Sub = 20,
    Mul = 21,
    Divs = 22,
    Rems = 23,
    Divu = 24,
    Remu = 25,
    Neg = 26,
    Not = 27,
    Eq = 28,
    Ne = 29,
    Lts = 30,
    Les = 31,
    Ltu = 32,
    Leu = 33,
    Sext8 = 34,
    Sext16 = 35,
    Sext32 = 36,
    Zext8 = 37,
    Zext16 = 38,
    Zext32 = 39,
    Fadd = 40,
    Fsub = 41,
    Fmul = 42,
    Fdiv = 43,
    Fneg = 44,
    Feq = 45,
    Fne = 46,
    Flt = 47,
    Fle = 48,
    Dadd = 49,
    Dsub = 50,
    Dmul = 51,
    Ddiv = 52,
    Dneg = 53,
    Deq = 54,
    Dne = 55,
    Dlt = 56,
    Dle = 57,
    I2f = 58,
    U2f = 59,
    F2i = 60,
    F2u = 61,
    I2d = 62,
    U2d = 63,
    D2i = 64,
    D2u = 65,
    F2d = 66,
    D2f = 67,
    Jmp = 68,
    Jz = 69,
    Jnz = 70,
    /// Jump when `!(a < b)`, signed. The rest of the `Jn*` family is the same shape.
    JnLts = 71,
    JnLes = 72,
    JnLtu = 73,
    JnLeu = 74,
    JnEq = 75,
    JnNe = 76,
    JnFlt = 77,
    JnFle = 78,
    JnDlt = 79,
    JnDle = 80,
    Call = 81,
    Ret = 82,
    RetV = 83,
    RetA = 84,
    Abort = 85,
    Div0 = 86,
    AppLit = 87,
    AppStr = 88,
    AppInt = 89,
    AppF64 = 90,
    AppBool = 91,
    Land = 92,
    Lor = 93,
    Band = 94,
    Bor = 95,
    Bxor = 96,
    Bnot = 97,
}

impl Op {
    pub(super) fn name(self) -> &'static str {
        match self {
            Op::Const => "const",
            Op::Mov => "mov",
            Op::Addr => "addr",
            Op::Idx => "idx",
            Op::Global => "global",
            Op::Str => "str",
            Op::Ld8s => "ld8s",
            Op::Ld8u => "ld8u",
            Op::Ld16s => "ld16s",
            Op::Ld16u => "ld16u",
            Op::Ld32s => "ld32s",
            Op::Ld32u => "ld32u",
            Op::Ld64 => "ld64",
            Op::St8 => "st8",
            Op::St16 => "st16",
            Op::St32 => "st32",
            Op::St64 => "st64",
            Op::Copy => "copy",
            Op::Add => "add",
            Op::Sub => "sub",
            Op::Mul => "mul",
            Op::Divs => "divs",
            Op::Rems => "rems",
            Op::Divu => "divu",
            Op::Remu => "remu",
            Op::Neg => "neg",
            Op::Not => "not",
            Op::Eq => "eq",
            Op::Ne => "ne",
            Op::Lts => "lts",
            Op::Les => "les",
            Op::Ltu => "ltu",
            Op::Leu => "leu",
            Op::Sext8 => "sext8",
            Op::Sext16 => "sext16",
            Op::Sext32 => "sext32",
            Op::Zext8 => "zext8",
            Op::Zext16 => "zext16",
            Op::Zext32 => "zext32",
            Op::Fadd => "fadd",
            Op::Fsub => "fsub",
            Op::Fmul => "fmul",
            Op::Fdiv => "fdiv",
            Op::Fneg => "fneg",
            Op::Feq => "feq",
            Op::Fne => "fne",
            Op::Flt => "flt",
            Op::Fle => "fle",
            Op::Dadd => "dadd",
            Op::Dsub => "dsub",
            Op::Dmul => "dmul",
            Op::Ddiv => "ddiv",
            Op::Dneg => "dneg",
            Op::Deq => "deq",
            Op::Dne => "dne",
            Op::Dlt => "dlt",
            Op::Dle => "dle",
            Op::I2f => "i2f",
            Op::U2f => "u2f",
            Op::F2i => "f2i",
            Op::F2u => "f2u",
            Op::I2d => "i2d",
            Op::U2d => "u2d",
            Op::D2i => "d2i",
            Op::D2u => "d2u",
            Op::F2d => "f2d",
            Op::D2f => "d2f",
            Op::Jmp => "jmp",
            Op::Jz => "jz",
            Op::Jnz => "jnz",
            Op::JnLts => "jn.lts",
            Op::JnLes => "jn.les",
            Op::JnLtu => "jn.ltu",
            Op::JnLeu => "jn.leu",
            Op::JnEq => "jn.eq",
            Op::JnNe => "jn.ne",
            Op::JnFlt => "jn.flt",
            Op::JnFle => "jn.fle",
            Op::JnDlt => "jn.dlt",
            Op::JnDle => "jn.dle",
            Op::Call => "call",
            Op::Ret => "ret",
            Op::RetV => "retv",
            Op::RetA => "reta",
            Op::Abort => "abort",
            Op::Div0 => "div0",
            Op::AppLit => "app.lit",
            Op::AppStr => "app.str",
            Op::AppInt => "app.int",
            Op::AppF64 => "app.f64",
            Op::AppBool => "app.bool",
            Op::Land => "land",
            Op::Lor => "lor",
            Op::Band => "band",
            Op::Bor => "bor",
            Op::Bxor => "bxor",
            Op::Bnot => "bnot",
        }
    }

    pub(super) fn is_jump(self) -> bool {
        matches!(
            self,
            Op::Jmp
                | Op::Jz
                | Op::Jnz
                | Op::JnLts
                | Op::JnLes
                | Op::JnLtu
                | Op::JnLeu
                | Op::JnEq
                | Op::JnNe
                | Op::JnFlt
                | Op::JnFle
                | Op::JnDlt
                | Op::JnDle
        )
    }
}

/// One instruction. Encoded as 24 bytes in the same layout as the VM's `Instr` struct.
#[derive(Clone, Debug)]
pub struct Instr {
    pub op: Op,
    pub aux: u16,
    pub dst: u32,
    pub a: u32,
    pub b: u32,
    pub imm: u64,
}

impl Instr {
    pub(super) fn new(op: Op, dst: u32, a: u32, b: u32, imm: u64) -> Instr {
        Instr {
            op,
            aux: 0,
            dst,
            a,
            b,
            imm,
        }
    }
}

/// How a call site reaches its target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallKind {
    /// A function in this module, by index.
    Local = 0,
    /// A function in another script, by key.
    Script = 1,
    /// A native function, through the program's thunk table, by key.
    Native = 2,
}

#[derive(Clone, Debug)]
pub struct CallArg {
    pub reg: u32,
    pub size: u32,
    /// The register holds the address of an aggregate, not the value.
    pub aggregate: bool,
}

#[derive(Clone, Debug)]
pub struct CallSite {
    pub kind: CallKind,
    pub key: u64,
    pub index: u32,
    pub ret_size: u32,
    /// Load op that widens a native scalar result into a register (0 for void/aggregate).
    pub ret_load: u16,
    pub ret_aggregate: bool,
    pub args: Vec<CallArg>,
}
