//! Where a `closure(..)` type may appear. A closure value points into the stack of the function that
//! made it, so it can be a parameter or a `let` and nothing else. The resolver rejects every other
//! place a type can be written, so no flow analysis is needed (`speculative/lang/closures.j3`).

use super::*;

const HOW: &str = "a closure can only be a parameter or a `let`; pass it down as a `closure(..)` parameter, \
or use a plain `fn(..)` with a `ptr` context if it must be stored";

/// `ty` must not be a closure type: `place` says where it was written ("a struct field", ...).
pub(crate) fn reject_closure(
    cx: &IrCx,
    path: &str,
    span: Span,
    ty: Ty,
    place: &str,
) -> Result<(), CheckError> {
    if cx.closure_sig(ty).is_none() {
        return Ok(());
    }
    Err(CheckError::new(
        path,
        span,
        format!("a closure cannot be {place}: it points into the stack of the function that made it"),
    )
    .help(HOW))
}

/// Closure types in `resolve_type` / `resolve_type_ref` contexts (consts, globals, enum payloads, `use lib`).
pub(crate) fn closure_not_here(path: &str, span: Span) -> CheckError {
    CheckError::new(
        path,
        span,
        "a closure type is not allowed here: it points into the stack of the function that made it",
    )
    .help(HOW)
}
