# Shaders

GPU code is J3 code. A fn marked `#[shader(vertex)]` or `#[shader(fragment)]` is checked against a small GPU subset and
turned into GLSL text when you compile, so a mistake is an error with a span and a fix in `j3c check`, not a driver log
when the window opens.

```rust
use j3.gpu.{ Source };
use j3.math.Vec2;

#[vertex] struct Quad { q: Vec2; }
#[varying] struct V { position: vec4; uv: vec2; }

#[uniform] let uScreen: vec2;
#[uniform] let uTex: sampler2D;

#[shader(vertex)]
fn vs(q: Quad): V {
    return V { position: vec4(q.q / uScreen * 2.0 - 1.0, 0.0, 1.0), uv: q.q };
}

#[shader(fragment)]
fn fs(v: V): vec4 {
    return @texture(uTex, v.uv);
}

const PROG: Source = @shader(vs, fs);
```

`PROG.vert` and `PROG.frag` are the two GLSL strings (static data, `#version 330 core`; the game's shader compile calls
turn them into ES 3.0 text at run time). `PROG.uniforms` names the uniforms, which you set from the CPU by exactly those
names. `PROG.attrib(i)` for `i < PROG.count` is the vertex layout: location, components, GL type, byte offset, stride
and divisor.

## What the attributes mean

| Attribute | On | Meaning |
|---|---|---|
| `#[shader(vertex)]` `#[shader(fragment)]` | `fn` | An entry. A vertex fn takes `#[vertex]` structs and returns a `#[varying]` struct; a fragment fn takes that struct and returns the color as `vec4` |
| `#[shader]` | `fn`, `struct` | A helper fn, or a plain GPU struct. It exists only on the GPU |
| `#[vertex]` `#[vertex(instance)]` | `struct` | A vertex buffer layout. Fields are `f32 i32 u32 Vec2 Vec3 vec4f Mat4 Rgba8` (`j3.gpu.Rgba8`: four bytes, a normalized `vec4`); the CPU struct is the buffer's element. Its packed layout must equal the CPU layout (else the error says which `_pad` to add) |
| `#[varying]` | `struct` | Passed from vertex to fragment. The first field is `position: vec4` |
| `#[flat]` | varying field | No interpolation (integer fields are always flat) |
| `#[uniform]` | module `let` | A uniform with exactly this name. No storage on the CPU |

## The subset

Types are `f32 i32 u32 bool`, `vec2..4`, `ivec2..4`, `uvec2..4`, `mat2..4`, `sampler2D`, structs and fixed arrays. They
exist only in shader code; the CPU keeps `Vec2 Vec3 Mat4 vec4f`. Operators follow GLSL (`vec * f32`, `mat * vec`,
swizzles). A literal takes the other operand's type; a variable never converts (`f32(n)`).

Statements: `let`, assignment, `if`, `while`, `for i in a..b` (an `i32`), `match` on an integer with `_`, `break`,
`continue`, `return`, `if` as a value. A `&T` parameter is `inout`.

Everything else is an error naming the fix: pointers, heap, strings, slices, `defer`, labels, closures, generics, 64-bit
`int`, recursion (the cycle is named), method calls on structs, calls to ordinary fns (mark them `#[shader]`), and
globals that are not `#[uniform]`.

Builtins are `@` calls from one table: `@mix @clamp @dot @cross @length @distance @normalize @min @max @abs @sign @floor
@ceil @fract @pow @exp @log @sqrt @sin @cos @tan @atan @step @smoothstep @mod @fwidth @dFdx @dFdy @texture @textureLod
@texelFetch @textureSize @reflect @refract @transpose @inverse`, with `@fragCoord()`, `@discard()` (fragment) and
`@vertexId()` (vertex). All of them exist in both GLSL 3.30 and ES 3.00.

## Looking at the output

`j3c dump-glsl [path]` prints each program (add `--es` for the ES 3.0 text). Only the fns an entry reaches are emitted,
in a fixed order, each under a `// file:line` comment. Names GLSL reserves (`input`, `sample`, `mix`) get a `j3_` prefix;
uniform names are never renamed (a uniform called `texture` is an error). Sample: `speculative/lang/shader.j3`; the same
programs compiled by a real driver, as 3.3 core and as ES, are `j3c/stdlib/tests/gpu_glsl_test.j3`. The full reference is
`j3c doc shaders`.
