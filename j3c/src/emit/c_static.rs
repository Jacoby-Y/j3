//! C static data: global initializers, with byte data as sized arrays.

use crate::emit::c_expr::{self, CWriter};
use crate::emit::c_ty;
use crate::ir::cx::IrCx;
use crate::ir::entity::ConstInit;
use crate::ir::operand::{ConstKind, Operand};

/// Global initializers. Bytes (strings, `@embed` data) become their own exactly sized
/// `static const unsigned char` arrays in `arrays`, written before the global that points at them:
/// no NUL is added and no string-literal length limit applies.
#[derive(Default)]
pub(super) struct Statics {
    pub(super) arrays: String,
    n: usize,
}

impl Statics {
    pub(super) fn init(&mut self, cx: &IrCx, init: &ConstInit) -> Result<String, String> {
        match init {
            ConstInit::Scalar(op) => match op {
                Operand::Const(c) => match &c.kind {
                    ConstKind::Bytes(bytes) if bytes.is_empty() => {
                        Ok("{ .ptr = (uint8_t *)\"\", .len = 0ll }".to_string())
                    }
                    ConstKind::Bytes(bytes) => {
                        let name = format!("j3_static_bytes{}", self.n);
                        self.n += 1;
                        let len = bytes.len();
                        self.arrays.push_str(&format!(
                            "static const unsigned char {name}[{len}] = {};\n",
                            c_expr::c_bytes_literal(bytes)
                        ));
                        Ok(format!("{{ .ptr = (uint8_t *){name}, .len = {len}ll }}"))
                    }
                    ConstKind::F32Bits(bits) => static_f32(*bits),
                    ConstKind::F64Bits(bits) => static_f64(*bits),
                    _ => {
                        let mut w = CWriter::new(cx);
                        w.operand(op)
                    }
                },
                _ => {
                    let mut w = CWriter::new(cx);
                    w.operand(op)
                }
            },
            ConstInit::Fields(fs) => {
                let mut parts = Vec::new();
                for (name, v) in fs {
                    let fname = c_ty::sanitize(name);
                    parts.push(format!(".{fname} = {}", self.init(cx, v)?));
                }
                Ok(format!("{{ {} }}", parts.join(", ")))
            }
            ConstInit::Elems(es) => {
                let mut parts = Vec::new();
                for v in es {
                    parts.push(self.init(cx, v)?);
                }
                Ok(format!("{{ {} }}", parts.join(", ")))
            }
        }
    }
}

fn static_f32(bits: u32) -> Result<String, String> {
    let value = f32::from_bits(bits);
    if value == 0.0 {
        if bits & 0x8000_0000 != 0 {
            return Ok("-0.0f".to_string());
        }
        return Ok("0.0f".to_string());
    }
    if !value.is_finite() {
        return Err(format!("non-finite f32 in static init: bits={bits}"));
    }
    for prec in [1usize, 8, 9, 12, 17] {
        let mut s = format!("{:.*}", prec, value);
        if s.parse::<f32>().ok().map(|v| v.to_bits()) == Some(bits) {
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                s.push_str(".0");
            }
            s.push('f');
            return Ok(s);
        }
    }
    Err(format!("f32 static init did not round-trip: bits={bits}"))
}

fn static_f64(bits: u64) -> Result<String, String> {
    let value = f64::from_bits(bits);
    if value == 0.0 {
        if bits & 0x8000_0000_0000_0000 != 0 {
            return Ok("-0.0".to_string());
        }
        return Ok("0.0".to_string());
    }
    if !value.is_finite() {
        return Err(format!("non-finite f64 in static init: bits={bits}"));
    }
    for prec in [1usize, 8, 9, 12, 17] {
        let mut s = format!("{:.*}", prec, value);
        if s.parse::<f64>().ok().map(|v| v.to_bits()) == Some(bits) {
            if !s.contains('.') && !s.contains('e') && !s.contains('E') {
                s.push_str(".0");
            }
            return Ok(s);
        }
    }
    Err(format!("f64 static init did not round-trip: bits={bits}"))
}
