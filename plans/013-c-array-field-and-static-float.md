# Plan 013: C backend can initialize array fields and static floats

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: `git rev-parse --short HEAD` on the worktree
> base should be `373b0d2` unless this prompt told you to merge another
> branch. Confirm the excerpts below. On a mismatch, STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: MED
- **Depends on**: none
- **Category**: bug
- **Planned at**: commit `373b0d2`, 2026-09-22

## Why this matters

`speculative/02_data.j3` runs on LLVM and does not compile as C. `math.j3`
`Mat4.identity` / `perspective` / `ortho` build a struct whose only field is
`m: [16]f32`. The C printer emits:

```c
j3l__t1_1 = (j3_j3_math_Mat4_4){ .m = j3l__t0_0 };
```

`j3l__t0_0` is `float[16]`. In that initializer it decays to `float *`. Clang
reports `incompatible types when initializing type 'float' using type 'float *'`.
That is why plan 001's dual-backend test stops on `02_data`.

A second C bug shows up when `j3.game` is in the program. `HUD_QUAD` is a
`#[singleton]` `[24]f32`. Static initializers go through `emit_const_init`,
which prints each f32 as a union compound literal:

```c
((union { uint32_t bits; float f; }){ .bits = 1065353216u }).f
```

That is not a C11 constant expression, so `static float hud[24] = { ... }`
is rejected (`initializer element is not constant`). Runtime float constants
may keep the union form. Static initializers must not.

## Current state

`j3c/src/emit/c_expr.rs` assign of an array already uses `memcpy`. Assign of
anything else, including a struct aggregate, is `place = rvalue;`:

```423:431:j3c/src/emit/c_expr.rs
            StmtKind::Assign { place, rvalue } => {
                let p = self.place(place)?;
                let v = self.rvalue(rvalue)?;
                let line = if c_ty::array_parts(self.cx, place.ty).is_some() {
                    format!("{pad}memcpy({p}, {v}, sizeof({p}));\n")
                } else {
                    format!("{pad}{p} = {v};\n")
                };
```

Struct aggregates (`aggregate`, around line 330) always emit
`(Type){ .field = operand }`. `c_ty.rs` line 1 says arrays are not a C type;
they are declared `T name[N]`. `array_parts` is `c_ty::array_parts`.

`Mat4` in `j3c/stdlib/j3/math.j3` around line 322:

```j3
pub struct Mat4 {
    m: [16]f32;
    fn identity(): Mat4 {
        return Mat4 {
            m: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
        };
    }
```

`j3c dump-c speculative/02_data.j3` contains three assignments of the form
`.m = j3l__t0_…` (identity, perspective, ortho). The bytes are copied into
the temp with `memcpy` first; the struct initializer is the line that is
illegal.

Float constants in expressions (`const_val`, around line 144):

```rust
ConstKind::F32Bits(bits) => Ok(format!(
    "((union {{ uint32_t bits; float f; }}){{ .bits = {bits}u }}).f"
)),
```

Leave that path alone. It is correct for non-static expressions.

Static init is `emit_const_init` in `j3c/src/emit/c.rs` around line 228.
`ConstInit::Scalar` calls `CWriter::operand`, so f32 singletons get the union
form. `HUD_QUAD` is `j3c/stdlib/j3/game.j3` around line 2194:

```j3
#[singleton]
let HUD_QUAD: [24]f32 = [
    0, 1, 0, 1, 1, 1, 1, 1, 1, 0, 1, 0, 0, 1, 0, 1, 1, 0, 1, 0, 0, 0, 0, 0,
];
```

Globals are emitted around line 107 as `static {elem} {name}[{len}] = {init};`.

An existing test rejects GNU statement expressions. Do not emit `({ ... })`.
See `basics_j3_runs` in `j3c/src/compile.rs`: `assert!(!c.contains("({")`.

Style: small files, no macros, no crates.io, explicit matches. Match the
existing `memcpy` assign.

## Commands you will need

From `j3c/` in the worktree. `cargo test` copies `j3c` to `/home/jacoby/.bin/j3c`. Leave the wrapper alone.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| 02 on C | `cargo test c_backend_runs_02_data -- --test-threads=1` | exit 0 |
| 08 headless on C | `cargo test c_backend_runs_08_headless -- --test-threads=1` | exit 0 |
| HUD text | `cargo test hud_quad_static_init_has_no_union -- --test-threads=1` | exit 0 |
| No GNU stmts | `cargo test basics_j3_runs -- --test-threads=1` | exit 0 |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/emit/c_expr.rs` (struct aggregate assign with array fields)
- `j3c/src/emit/c.rs` (`emit_const_init` float scalars only)
- `j3c/src/compile.rs` (the three tests below)

**Out of scope**:
- LLVM printer
- Changing `const_val`'s union form for runtime expressions
- `j3c/stdlib/j3/math.j3` and `game.j3` (do not rewrite `Mat4` or `HUD_QUAD`)
- GNU statement expressions
- Plans 001–012's other edits. This branch starts from `373b0d2`.

## Git workflow

Commit on the worktree branch. Do not push. Do not update `plans/README.md`.
Message: `Make C struct array fields and static floats initializable.`

## Steps

### Step 1: Assign struct aggregates that contain arrays

In `StmtKind::Assign`, before the existing `place = rvalue` line, detect
`RvalueKind::Aggregate { fields }` whose `rvalue.ty` is `TyKind::Struct`.
Load the struct def. If any field's type has `c_ty::array_parts` `Some`, do
not call `self.rvalue` for that aggregate.

Emit:

1. An assignment of a compound literal that lists only the non-array fields,
   using the same `.name = operand` spelling as `aggregate`. If every field
   is an array, the literal is `(Type){0}`.
2. For each array field, in field order:
   `memcpy(place.field, source, sizeof(place.field));`
   `source` is `self.operand` of that field's operand. An array lvalue decays
   to the pointer `memcpy` expects. Do not take the address twice.

`place` is `self.place(place)`. Field name is `self.cx.name_str(field.name)`.
The C field expression is `({place}).{fname}` only if `place` is not a bare
identifier; a bare identifier can be `{place}.{fname}`. Using `({place}).{fname}`
for every place is safer and still valid. Use that.

If `self.rvalue` / `aggregate` is still invoked for a struct that has an array
field from Return or Expr, change `aggregate` so that case returns
`Err("array field in struct aggregate".into())` instead of a decaying
initializer. Then, if a test fails on that error, apply the same two-step
emit in that statement, writing into the destination place. Do not invent a
statement expression to keep a single expression.

### Step 2: Static float constants

In `emit_const_init`, `ConstInit::Scalar` of `ConstKind::F32Bits` / `F64Bits`
must not call `operand`. Print a C floating constant that round-trips:

- `+0.0` → `0.0f` / `0.0`
- `-0.0` (sign bit set, magnitude zero) → `-0.0f` / `-0.0`
- non-finite → `Err(...)` with the bits. Do not emit the union form here.
- otherwise format with increasing precision (`format!("{:.*}", prec, value)`
  for prec 1, 8, 9, 12, 17) until `str::parse` yields the same bits. Append
  `f` for f32. If the text has no `.` and no exponent, insert `.0` before the
  suffix so it is a floating constant, not an integer.

`ConstInit::Elems` of those scalars then becomes `{ 0.0f, 1.0f, ... }`, which
is a constant expression.

Leave `const_val` in `c_expr.rs` unchanged.

### Step 3: Tests

In `j3c/src/compile.rs` `mod tests`, add:

1. `c_backend_runs_02_data` — `compile_path(speculative/02_data.j3)`,
   `EmitOpts::default()` with `backend = Backend::C`, `run_program`. Status 0.
   Stdout exactly `len=4.47214 player.pos=Vec2 { x: 12, y: 9 }\n`. Include
   stderr in the assert message.

2. `c_backend_runs_08_headless` — `compile_path_headless(speculative/08_game_loop.j3)`,
   same C opts, status 0, stdout `bye\n`.

3. `hud_quad_static_init_has_no_union` — `compile_path` (SDL game, not
   headless) of `speculative/08_game_loop.j3`, `emit_program` only. Do not
   link. Find the line containing `HUD_QUAD`. Assert it exists and does not
   contain `union`.

`speculative()` already exists in that test module.

**Verify**: the three commands in the table, then `basics_j3_runs`, then `cargo test`.

## Test plan

- The three new tests are the regression locks.
- `basics_j3_runs` locks "no GNU statement expression".
- Pattern: `c_backend_still_runs_hello` and `game_loop_j3_runs`.

## Done criteria

- [ ] `cargo test c_backend_runs_02_data` exits 0
- [ ] `cargo test c_backend_runs_08_headless` exits 0
- [ ] `cargo test hud_quad_static_init_has_no_union` exits 0
- [ ] `cargo test basics_j3_runs` exits 0
- [ ] `cargo test` exits 0
- [ ] `rg "\\(\\{" j3c/src/emit/c_expr.rs` does not show a new statement-expression
- [ ] Only the three in-scope files change
- [ ] `plans/README.md` not modified by you

## STOP conditions

- The `Mat4` assign excerpt is gone (the printer no longer emits `.m = j3l__t0_0`). STOP and describe what it emits now.
- A non-finite float appears in a singleton initializer. STOP and name the global. Do not emit a union there.
- Fixing this requires a GNU statement expression to satisfy `basics_j3_runs`. STOP.
- `02` or headless `08` C run compiles but stdout differs from the strings above. STOP. Do not change the expected stdout.

## Maintenance notes

- A new struct with an array field must go through the Assign path, not `.field = array_lvalue`.
- Runtime f32 constants stay bit-preserving unions. Only static initializers use decimal/hex-looking float literals. Reviewers should check a non-zero finite value still round-trips (the precision loop).
- `HUD_QUAD` staying a float array in `game.j3` is intentional.
