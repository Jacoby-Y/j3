Shaders in J3: GPU code written as J3 fns

A shader is a J3 fn with `#[shader(vertex)]` or `#[shader(fragment)]`. The compiler checks it against the GPU
subset and writes GLSL text, so a mistake is an error at `j3c check` with a span and a fix, not a driver log
when the window opens. The same parser, the same checker, no separate file format.

    use j3.gpu.{ Source };
    use j3.math.{ Vec2 };

    #[vertex] struct Quad { q: Vec2; }                       // per-vertex input: a CPU struct, also the shader's input
    #[vertex(instance)] struct Inst { rect: vec4f; k: Vec2; _pad: Vec2; }      // per-instance (divisor 1)
    #[varying] struct V { position: vec4; uv: vec2; #[flat] rect: vec4; }   // vertex -> fragment

    #[uniform] let uScreen: vec2;
    #[uniform] let uTex: sampler2D;

    #[shader(vertex)]
    fn vs(q: Quad, i: Inst): V {
        let p = i.rect.xy + q.q * i.rect.zw;
        return V { position: vec4(p / uScreen * 2.0 - 1.0, 0.0, 1.0), uv: q.q, rect: i.rect };
    }

    #[shader(fragment)]
    fn fs(v: V): vec4 {
        return @texture(uTex, v.uv);
    }

    const PROG: Source = @shader(vs, fs);      // PROG.vert and PROG.frag are the GLSL text

Run `j3c dump-glsl [path] [--es]` to read what was emitted (`--es` is the ES 3.0 text the driver gets on Android or
with `J3_GLES=1`). A complete program with a matrix attribute, an integer varying, `inout`, a uniform array and
`texelFetch` is `j3c/stdlib/tests/gpu_glsl_test.j3`; `speculative/lang/shader.j3` is the CPU side.

Items
    `#[shader(vertex)]`, `#[shader(fragment)]`: entry fns. A vertex fn takes `#[vertex]` structs (any number, none is
    fine with `@vertexId()`) and returns the `#[varying]` struct; a fragment fn takes that struct (or nothing) and
    returns the color as `vec4`. `#[shader]` alone is a helper any shader fn can call; it stops existing for the CPU.
    `#[vertex]` struct: fields are CPU types (`f32 i32 u32 Vec2 Vec3 vec4f Mat4 Rgba8`) and the shader sees their GPU twin
    (`float int uint vec2 vec3 vec4 mat4 vec4`; `j3.gpu.Rgba8` is four bytes read as a 0..1 `vec4`, a normalized attribute). Locations are assigned in order, one per field (`Mat4` takes four), never
    written. The struct's CPU layout must be the tightly packed one; otherwise it is an error naming the field and
    the `_pad: Vec2` to insert (`vec4f` is 16-aligned). A field named `_...` is padding: bytes, no attribute.
    `#[varying]` struct: first field `position: vec4` (the clip position; in the fragment fn it holds `gl_FragCoord`).
    The rest are f32/i32/u32 or vectors of them; integers and `#[flat]` fields are not interpolated.
    `#[shader] struct`: a plain GPU struct of shader types. `#[uniform] let name: T;`: a uniform with exactly that name
    (a name GLSL reserves, like `texture` or `mix`, is an error: the CPU binds uniforms by name, so it cannot be
    renamed). CPU code sets it with GL calls by name; it has no CPU storage.
    `const PROG: Source = @shader(vs, fs);`: the pair. `Source` (`j3c doc j3.gpu`) holds `vert`, `frag`, the uniform
    names in `uniforms` (space separated), and the vertex attributes: `PROG.attrib(i)` for `i < PROG.count` gives
    `loc comps glType offset stride divisor isInt normalized`, what `glVertexAttribPointer` takes. Text is static data.

Types
    `f32 i32 u32 bool` (`int`, `i64`, `f64`, `u8`... are errors: say `i32`), `vec2 vec3 vec4`, `ivec2..4`, `uvec2..4`,
    `bvec2..4`, `mat2 mat3 mat4`, `sampler2D`, `sampler2DShadow`, structs of those, fixed arrays `[N]T`. They exist only in
    shader code; CPU code keeps `Vec2 Vec3 Mat4 vec4f`. Operators are GLSL's: component-wise, `vec * f32`, `f32 * vec`,
    `mat * vec`, swizzles (`.xy .rgb .stpq`, assignable without repeats), constructors `vec3(v2, 1.0)`, `vec4(0.0)`,
    `x as f32` / `f32(x)`. A number literal takes the other operand's type; alone it is `i32` or `f32`. A variable never
    converts: write `f32(n)`.

Statements
    `let`, assignment and `+= -= *= /= %= &= |= ^=`, `if`/`else`, `while`, `for i in a..b` and `a..=b` (an `i32`, bounds
    are `i32`), `match` on an `i32`/`u32` with integer arms and a required `_` (a `switch`; `break` inside it is an error:
    GLSL's would leave the match), `break`, `continue`, `return`, `if` as a value (`?:`). A fn that returns a value must end
    every path with `return`. A `&T` parameter becomes `inout`; pass `&local`.
    Not in shader code, each an error with the fix: pointers, heap, strings, slices, `defer`, labels, closures, generics,
    traits, atomics, `vec4f`, method calls on structs (free fns and UFCS to shader fns are fine), `for x in xs`, any
    non-`#[shader]` fn, any global that is not `#[uniform]`, recursion (direct or mutual: the cycle is named), enum match.

Builtins
    `@mix @clamp @dot @cross @length @distance @normalize @min @max @abs @sign @floor @ceil @fract @pow @exp @log
    @sqrt @sin @cos @tan @atan @step @smoothstep @mod @fwidth @dFdx @dFdy @texture @textureLod @texelFetch
    @textureSize @reflect @refract @transpose @inverse`, plus `@fragCoord()` and `@discard()` (fragment only) and
    `@vertexId()` (vertex only). The table is `check/gpu/builtins.rs`; everything in it exists in GLSL 3.30 and ES 3.00,
    so a shader that checks is valid on both. Not yet: `textureGrad`, integer textures, `gl_FragDepth`, MRT, uniform
    blocks / std140, geometry, compute.

GLSL names
    Locals, params, fields, fns and structs keep their J3 names unless GLSL reserves them (`input`, `output`, `sample`,
    `texture`, `mix`, `sin`, anything `gl_`, `v_`, `a_`): those get a `j3_` prefix. Two modules defining the same fn name get
    the module name as a prefix. Only fns the entry reaches are emitted, in a fixed order, each under a `// file:line`
    comment so a driver log line maps back with `j3c dump-glsl`. The text starts `#version 330 core`; the game's shader
    compile paths (`game.prepGl`, `j3.gfx.glsl.prep`) turn it into `#version 300 es` plus precision lines at run time.

Using a Source
    Hand `PROG.vert` / `PROG.frag` to the existing compile calls. Look uniforms up by the names the `#[uniform] let`s have
    (`glGetUniformLocation`); `PROG.uniforms` lists them. Describe vertex buffers from `PROG.attrib(i)` and
    `glVertexAttribDivisor(loc, divisor)` instead of numbering attributes by hand. `j3.gfx.shader` and `j3.game.p2` still
    own GL state.

Real examples
    `j3c/stdlib/j3/game/p2shader.j3`: the 2D painter's program (quad plus a 64-byte instance with `Rgba8` colors,
    flat varyings, shared helpers, a `kind` ladder); `hudInit` in `game/p2.j3` loops over `P2_PROG.attrib(i)` and
    looks uniforms up by name, panicking on a miss. `j3c/stdlib/j3/gfx/post/programs.j3`: the fullscreen triangle from
    `@vertexId()` and the source-over pass (`OVER_PROG`); the other post fragments are still GLSL text reading `v_uv`.

Testing
    `j3c check` hits every rule. `cargo test shaders` (in `j3c/`) compares the emitted text with goldens and compiles it
    in a 3.3 core and an ES 3.0 context under `SDL_VIDEODRIVER=offscreen` (it prints `skipped: ...` with no GL). By hand:
    `J3_GLES=1 __GL_SHADER_DISK_CACHE=0 SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy timeout 60 j3c run
    j3c/stdlib/tests/gpu_glsl_test.j3 --headless`.
