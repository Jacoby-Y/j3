//! Where a `closure(..)` type may appear. A closure value points into the stack of the function that
//! made it, so it can be a parameter or a `let` and nothing else. The resolver rejects every other
//! place a type can be written, so no flow analysis is needed (`speculative/lang/closures.j3`).
//! A `boxed closure(..)` copied its captures into an arena, so it may go anywhere a value can,
//! a `#[singleton] let` too (empty until assigned), but not a `const` (`speculative/lang/boxed_closures.j3`).

use super::*;

const HOW: &str = "a closure can only be a parameter or a `let`; pass it down as a `closure(..)` parameter, \
or make it a `boxed closure(..)` with `fn[mem](..) { .. }` (captures copied into arena `mem`) if it must be stored";

/// `ty` must not be a closure type: `place` says where it was written ("a struct field", ...).
pub(crate) fn reject_closure(
    cx: &IrCx,
    path: &str,
    span: Span,
    ty: Ty,
    place: &str,
) -> Result<(), CheckError> {
    if cx.stack_closure_sig(ty).is_none() {
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
pub(crate) fn closure_not_here(path: &str, span: Span, boxed: bool) -> CheckError {
    if boxed {
        return CheckError::new(path, span, "a `boxed closure(..)` type is not allowed here")
            .note("a `const` is data fixed at compile time, and a boxed closure's captures live in an arena made at run time; an enum payload or a `use lib` signature cannot hold one either")
            .help("keep it in a `#[singleton] let` (assigned at startup), a struct field, a `List`, a `let`, a parameter or a return value");
    }
    CheckError::new(
        path,
        span,
        "a closure type is not allowed here: it points into the stack of the function that made it",
    )
    .help(HOW)
}
