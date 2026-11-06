# Keywords, Types, and Attributes

## Keywords

| Keyword | Use |
|---|---|
| `module` | Name the file's module |
| `use` | Import names; embed fields; bring fields into scope |
| `lib`, `as` | Declare C functions (`use lib "x" as y`); `as` also casts |
| `pub` | Export an item |
| `fn` | Function or method |
| `struct`, `enum` | Types |
| `const` | Module constant |
| `let` | Local variable (always mutable) |
| `if`, `else` | Branch, or a value |
| `while`, `for`, `in` | Loops |
| `match` | Branch on an enum |
| `return`, `break`, `continue` | Leave a function or loop |
| `defer` | Run at end of block |
| `operator` | Define an operator method |
| `this` | Method receiver |

## Types

| | |
|---|---|
| Integers | `int` (= `i64`), `i8`, `i16`, `i32`, `i64`, `u8`, `u16`, `u32`, `u64` |
| Floats | `float` (= `f64`), `f32`, `f64` |
| Other | `bool`, `void`, `string`, `ptr`, `cstr` |
| Arrays | `[N]T` (fixed), `[]T` (slice) |
| Pointers | `&T` |
| Functions | `fn(A, B): R` |

## Attributes

| Attribute | Meaning |
|---|---|
| `#[Debug]` | Printable in string interpolation |
| `#[TryCatch(Ok, Err)]` | Enum works with `?` |
| `#[singleton]` | Process-wide value |
| `#[thread]` | Per-thread (with `#[singleton]`) |
| `#[shader]` `#[vertex]` `#[varying]` `#[uniform]` `#[flat]` | Shader code and data ([Shaders](language/shaders.md)) |

## Intrinsics

`@sizeof`, `@slice`, `@syscall`, `@atomicLoad`, `@atomicStore`, `@atomicAdd`, `@atomicSub`, `@atomicExchange`, `@atomicCas`, `@vmin`, `@vmax`, `@vsqrt`, `@vabs`, `@vsum`, `@vshuffle`, `@vfloat`, `@vint`, `@vload`, `@vstore`, `@script`, `@j3ui`, `@uiChoice`, and the shader builtins and `@shader` ([Shaders](language/shaders.md))

## Operators

| | |
|---|---|
| Arithmetic | `+ - * / %` |
| Bits (integers) | `& \| ^ ~ << >>` |
| Compare | `== != < <= > >=` |
| Logic | `&& \|\| !` |
| Assign | `= += -= *= /= %= &= \|= ^=` |
| Pointers | `&x` (address), `*p` (deref) |
| Ranges | `a..b`, `a..=b`, `xs[a..b]`, `xs[:]` |
| Errors | `expr?` |

## Not in J3

No interfaces or inheritance; traits are structural (a type has the methods, no `impl` blocks). Closures cannot escape (no heap environments): a `closure(..)` is a parameter or a `let`. No garbage collector or borrow checker. No null and no exceptions. No mutable globals besides `#[singleton]`.
