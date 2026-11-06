# Attributes, Intrinsics, and C

## Attributes

Attributes go on the line before a declaration.

| Attribute | On | Effect |
|---|---|---|
| `#[Debug]` | struct | Can be printed in a `` `{hole}` `` |
| `#[TryCatch(Ok, Err)]` | enum | Works with `?` (see [Enums and Errors](enums.md#errors-and-)) |
| `#[singleton]` | `let` | One value for the whole process |
| `#[thread]` | `let` (with `#[singleton]`) | One value per thread |
| `#[impl]` | free `fn` | Gives a type you cannot edit a trait's method (see [Traits](traits.md)) |
| `#[shader]` `#[shader(vertex)]` `#[shader(fragment)]` `#[vertex]` `#[varying]` `#[uniform]` `#[flat]` | `fn`, `struct`, field, `let` | GPU code and its data (see [Shaders](shaders.md)) |

### Singletons

There are no ordinary mutable globals. A `#[singleton]` is the one exception: a single process-wide value. Its initializer must be a constant, not a function call, and it cannot be `pub`.

```rust
#[singleton]
let FRAME_COUNT: int = 0;

#[singleton]
#[thread]
let JOBS_DONE: int = 0;       // one per thread
```

## Intrinsics

Intrinsics start with `@` and are handled by the compiler.

| Intrinsic | What it does |
|---|---|
| `@sizeof(T)` | Size of a type in bytes |
| `@slice(p, n)` | Build a slice from a pointer and a length |
| `@syscall(...)` | A raw system call |
| `@atomicLoad(p)` `@atomicStore(p, v)` | Atomic read and write through a `&T` (`int`, `i32`, `u32`, `u64`, `bool`, `ptr`) |
| `@atomicAdd(p, v)` `@atomicSub(p, v)` `@atomicExchange(p, v)` | Atomic update; returns the old value (no add or sub on `bool`, `ptr`) |
| `@atomicCas(p, old, new)` | Compare and swap; returns whether it swapped |
| `@vmin` `@vmax` `@vsqrt` `@vabs` `@vsum` `@vshuffle` `@vfloat` `@vint` `@vload` `@vstore` | SIMD on `vec4f` / `vec4i` (4 lanes, 16 bytes, align 16): lane-wise ops, horizontal sum, shuffle, conversions, slice load and store. `vec4f(x, y, z, w)` builds one, `.x .y .z .w` read and write lanes. Same bits on LLVM and C at every opt level: `@vmin`/`@vmax` give the second operand on NaN or a tie, `@vsum` adds `(x + y) + (z + w)`, `@vint` truncates, saturates and maps NaN to 0, shifts mask the amount to 0..31. Not in `.j3s` scripts |
| a last argument to any of them | Memory order: `relaxed`, `acquire`, `release`, `acqrel`, `seqcst` (default); load takes no `release`, store no `acquire` |
| `@script("path.j3s")` | The type of a script (see [Scripts](scripts.md)) |
| `@j3ui` | Immediate-mode UI functions (`j3c doc j3ui-immediate`) |

## Calling C

`use lib` declares functions from a shared library. Each one is written as a J3 signature with no body:

```rust
use lib "SDL2" as sdl {
    fn SDL_Init(flags: u32): i32;
    fn SDL_Quit();
}

fn main() {
    sdl.SDL_Init(0x20);
    defer sdl.SDL_Quit();
}
```

The library is linked by name (`"SDL2"` links `-lSDL2`); add any search paths under `[link]` in [`j3.toml`](../compiler/manifest.md). Use `cstr` for C strings and `ptr` for opaque pointers.

## Raw casts

`as` reinterprets a value at the bit level, for the rare low-level code that needs it (pointer arithmetic, FFI):

```rust
let addr = p as int;
let header = (addr + 8) as ptr as &i32;
```

For number conversions, prefer the checked-looking call form (`f32(n)`, `int(x)`) from [Values and Types](types.md).
