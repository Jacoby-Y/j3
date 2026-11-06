//! SIMD ops on `vec4f` / `vec4i` that are not plain operators. `+ - * /` and the bit ops are
//! ordinary [`BinOp`](crate::ir::op::BinOp)s on vector operands; a scalar operand is splatted first.
//! Lane reads and writes are the place projection [`Proj::Lane`](crate::ir::place::Proj::Lane).

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum SimdOp {
    /// One lane value in all four lanes.
    Splat,
    /// Four lane values `x y z w`.
    Build,
    /// `a < b ? a : b` per lane (a NaN lane gives `b`; equal lanes, even `-0.0` vs `0.0`, give `b`).
    Min,
    /// `a > b ? a : b` per lane.
    Max,
    /// `vec4f` only: IEEE square root.
    Sqrt,
    /// Absolute value per lane (`vec4i`: `i32::MIN` wraps to itself).
    Abs,
    /// Horizontal add, fixed tree `(x + y) + (z + w)`; result is the lane type.
    Sum,
    /// `[a.i0, a.i1, a.i2, a.i3]`, indices 0..3.
    Shuffle([u8; 4]),
    /// `vec4i` to `vec4f`.
    ToFloat,
    /// `vec4f` to `vec4i`, truncating toward zero; saturates, NaN gives 0.
    ToInt,
    /// `args[0]` is `&lane` of `xs[i]`; reads four lanes with `align 4`.
    Load,
    /// `args[0]` is `&lane` of `xs[i]`, `args[1]` the vector. Type is `void`.
    Store,
}

impl SimdOp {
    pub fn name(self) -> &'static str {
        match self {
            SimdOp::Splat => "splat",
            SimdOp::Build => "build",
            SimdOp::Min => "min",
            SimdOp::Max => "max",
            SimdOp::Sqrt => "sqrt",
            SimdOp::Abs => "abs",
            SimdOp::Sum => "sum",
            SimdOp::Shuffle(_) => "shuffle",
            SimdOp::ToFloat => "tofloat",
            SimdOp::ToInt => "toint",
            SimdOp::Load => "load",
            SimdOp::Store => "store",
        }
    }
}

/// Lane letters in order.
pub const LANE_NAMES: [&str; 4] = ["x", "y", "z", "w"];

pub fn lane_index(name: &str) -> Option<u8> {
    LANE_NAMES.iter().position(|n| *n == name).map(|i| i as u8)
}
