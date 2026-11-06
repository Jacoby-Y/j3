# Values and Types

## Variables

`let` declares a local. Every local is mutable, and its type is inferred unless you write one.

```rust
let hp = 3;            // int
let speed: f32 = 4.5;  // annotated
hp -= 1;               // fine: let is mutable
```

A `let` lasts until the end of its block. A later sibling block can reuse the name, but a name still in scope cannot be shadowed:

```rust
let far: f32 = 2.0;
if far > 1.0 { let a = 1; printLn(`a={a}`); }
if far > 1.0 { let a = 2; printLn(`a={a}`); }   // fine: the first `a` has ended
```

## Primitive types

| Type | What it is |
|---|---|
| `int` | 64-bit signed integer (same as `i64`) |
| `float` | 64-bit float (same as `f64`) |
| `i8` `i16` `i32` `i64` | Signed integers |
| `u8` `u16` `u32` `u64` | Unsigned integers |
| `f32` `f64` | Floats. Gameplay math uses `f32`. |
| `f16` | Half float, storage only (2 bytes). Convert with `f16(x)` / `f32(h)`; no arithmetic. |
| `bool` | `true` or `false` |
| `string` | A read-only view of UTF-8 bytes |
| `void` | No value |
| `ptr`, `cstr` | Raw pointer and C string, for talking to C |

## Numbers and conversions

A literal with no context is an `int`, or an `f64` if it has a dot. A literal next to a typed value takes that value's type, so with `far: f32`, both `2 * far` and `far * 0.5` are `f32`.

Variables never convert on their own. Convert explicitly by calling the type:

```rust
let n: int = 3;
let far: f32 = 10.0;
// let x = far * n;      // error: f32 * int
let x = far * f32(n);    // ok
let k = int(far);
```

`f16` is for compact storage (colors, UVs, weights): keep it in structs and arrays, and convert to `f32` to compute. `f16(x)` takes a float or int and rounds to nearest even (too big becomes infinity, NaN stays NaN); `f32(h)` and `f64(h)` are exact. An untyped literal stored in an `f16` is rounded at compile time. `+`, `<`, `==`, `-h`, `+=` and interpolation on an `f16` are compile errors that say to convert first. Both backends keep the 16 bits as an integer and convert with the same rounding, with or without `_Float16` in the C compiler, so results match bit for bit. Not available in `.j3s` scripts or as a `j3.gpu` vertex attribute.

Integer overflow wraps. `%` truncates like C (`-7 % 3 == -1`); use `math.wrapi(-7, 3)` when you want an index that wraps around to `2`. Dividing by zero stops the program.

The compound assignments `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, and `^=` exist.

`&`, `|`, `^` and unary `~` work on integers of one type (`x & 0xff` types the literal; `0b1010` and `0xff` are integer literals). Precedence, tightest first: unary `- ! ~ &x`, `* / %`, `+ -`, `<< >>`, `&`, `^`, `|`, comparisons, `&&`, `||`. Unlike C, `x & MASK == 0` is `(x & MASK) == 0`. `~` on an unsigned type keeps its width (`~u8(0) == 255`). `<`, `<=`, `>`, `>=` take numbers; a `string` has no order (a compile error). Compare with `==` / `!=`, or `text.same` from `j3.text`. Bools use `&&` / `||`.

## Constants

`const` declares a module-level constant. The value can be an integer expression, a float, a bool, or a string.

```rust
const MAX_ENEMIES = 64;
const GRAVITY = -9.8;
const TITLE = "Dungeon";
```

A typed array constant is a read-only table stored in the binary:

```rust
const LEVEL_XP: [4]int = [0, 100, 250, 500];

fn xpFor(level: int): int {
    return LEVEL_XP[level];   // bounds-checked
}
```

Writing into a constant table (`LEVEL_XP[0] = 1`) is a compile error. Constant tables are private to their module.

There are no mutable globals. For process-wide state, see [`#[singleton]`](attributes.md#singletons).

## String interpolation

A backtick string fills `{holes}` with values:

```rust
let name = "Ada";
let hp = 7;
printLn(`{name} has {hp} hp`);
```

A struct in a hole needs the [`#[Debug]`](attributes.md) attribute. The result lives in the calling function's stack frame and holds up to 512 bytes; going past that stops the program. For longer text, build it with `j3.text.Text`.
