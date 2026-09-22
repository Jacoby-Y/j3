# Plan 012: Default fields on Hit, Texture, and Mesh

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm the four structs
> still have no `=` defaults. On a mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: direction
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

Default field values already typecheck (`default_fields_fill_omitted` in
`compile.rs`). The roadmap named `Hit`, `Texture`, and `Mesh` as the structs
that repeat `{ ok: false, x: 0, ... }`. They still require every field.
Adding `= false` / `= 0` does not change existing literals. Omitted fields
become legal and mean the same zeros those literals already write.
`game_headless.j3` has a second `Texture` that must match `game.j3`.

## Current state

Language, already tested (`j3c/src/compile.rs` around line 909):

```j3
struct Hit {
    ok: bool = false;
    x: int = 0;
}
fn main() {
    let a = Hit {};
    let b = Hit { ok: true };
}
```

Checker (`j3c/src/check/mod.rs` `literal_default`, around line 2379): an int
literal `0` is a null pointer when the field type is `ptr`. An int literal is
allowed for any int type, including `i32` and `u32`. So `raw: ptr = 0` and
`w: i32 = 0` and `vao: u32 = 0` are valid defaults. A non-literal default is
an error. Do not use a call as a default.

Structs to edit. Do not remove fields. Do not reorder them. Add ` = false` or
` = 0` after the type, before the semicolon. Leave the `///` comments.

`j3c/stdlib/j3/voxel.j3` `pub struct Hit` (line 22):

- `ok: bool` → `ok: bool = false`
- `x`, `y`, `z`, `px`, `py`, `pz`, `face` are `int` → `= 0` on each

`j3c/stdlib/j3/game.j3` `pub struct Texture` (line 569):

- `raw: ptr` → `raw: ptr = 0`
- `w: i32` → `w: i32 = 0`
- `h: i32` → `h: i32 = 0`

`j3c/stdlib/j3/game_headless.j3` `pub struct Texture` (line 205): same three
fields and the same defaults. The comments differ (`Backend texture pointer`
vs `SDL texture pointer`). Do not copy comments across the two files.

`j3c/stdlib/j3/gfx.j3` `pub struct Mesh` (line 250):

- `vao: u32 = 0`
- `vbo: u32 = 0`
- `count: int = 0`

There is another `struct` earlier in `game.j3` that is not `Texture` (pixels).
Do not add defaults to `Sound`, `Music`, `Font`, `Camera`, or `Procs`.

Existing call sites that fill every field must keep compiling. Do not sweep
call sites to `Hit {}` in this plan. A wrong default would change meaning
only at a site that omits a field, and this plan adds no such site except
the new test.

`minecraft_project_typechecks` compiles a program that uses `Hit`.
`defense_project_typechecks` and `gallery_project_typechecks` pull in `j3.game`.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| New test | `cargo test omitted_hit_texture_mesh_use_zeros -- --test-threads=1` | exit 0 |
| Projects | `cargo test minecraft_project_typechecks defense_project_typechecks gallery_project_typechecks -- --test-threads=1` | exit 0, or three commands |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/stdlib/j3/voxel.j3` (`Hit` only)
- `j3c/stdlib/j3/game.j3` (`Texture` only)
- `j3c/stdlib/j3/game_headless.j3` (`Texture` only)
- `j3c/stdlib/j3/gfx.j3` (`Mesh` only)
- `j3c/src/compile.rs` (one test)

**Out of scope**:
- Rewriting `Hit { ok: false, x: 0, ... }` call sites in examples or stdlib
- `Slot.none(dummy)` in `res.j3`
- Left-hand numeric literals
- `roadmap.md` (plan 007). Do not edit it here even to mention this landing.

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Add the defaults

Edit the four structs as listed. Example shape:

```j3
pub struct Hit {
    /// True when the ray struck a solid cell within reach.
    ok: bool = false;
    /// Grid X of the hit solid cell.
    x: int = 0;
    // ...same for y, z, px, py, pz, face
}
```

### Step 2: Test omitted fields

Add `omitted_hit_texture_mesh_use_zeros` in `compile.rs` `mod tests`.
`parse_file` is in that module.

The test cannot `use j3.voxel` from a one-file `check_files` snippet unless
the loader runs. Use `compile_path` on a temp `.j3` file so stdlib imports
resolve, the same way `inclusive_for_runs` uses a file. Write the file under
`std::env::temp_dir()`, delete it before the test returns (including on
assert failure: delete before asserting, or use a scope that always removes).

```j3
module t;
use j3.voxel.Hit;
use j3.game.Texture;
use j3.gfx.Mesh;
fn main() {
    let h = Hit {};
    let tex = Texture {};
    let mesh = Mesh {};
    if h.ok || h.x != 0 || h.face != 0 {
        j3.io.printLn("hit");
    }
    if tex.w != 0 || tex.h != 0 {
        j3.io.printLn("tex");
    }
    if mesh.count != 0 || mesh.vao != 0 {
        j3.io.printLn("mesh");
    }
}
```

`ptr` comparison: do not compare `tex.raw` unless a nearby test already
compares `ptr` to `0`. If `tex.raw != 0` fails to typecheck, drop that
compare and keep `w` / `h` / `mesh.vao`. STOP if `Hit {}` itself fails to
compile.

`compile_path` the temp file, `exec` it (the `exec` helper in this test
module). Assert status 0 and stdout empty.

`j3.gfx.Mesh` pulls OpenGL declarations. `compile_path` typechecks and emits;
it does not need a GPU to emit. If `exec` fails at link time because `j3.gfx`
needs `-lGL` and this temp file is not a project with `[link]`, do not add
link flags. Switch the test to `compile_path(...).expect("compile")` only
(typecheck + the fact that emit is not required) and drop `exec`. Still
assert it compiles. A link failure is not a default-field failure. Record
in the test comment why it does not run.

If `use j3.gfx.Mesh` fails to find `Mesh`, the module path is `j3.gfx` and
the struct is `Mesh` (see `gfx.j3` line 250 `pub struct Mesh`). STOP if the
import path is different rather than guessing.

### Step 3: Existing projects

**Verify**: `cargo test minecraft_project_typechecks -- --test-threads=1` → exit 0.
**Verify**: `cargo test defense_project_typechecks -- --test-threads=1` → exit 0.
**Verify**: `cargo test gallery_project_typechecks -- --test-threads=1` → exit 0.
**Verify**: `cargo test omitted_hit_texture_mesh_use_zeros -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- `omitted_hit_texture_mesh_use_zeros` covers `Hit {}`, `Texture {}`, `Mesh {}`.
- Existing full-literal call sites stay covered by `minecraft_project_typechecks` and the project tests.
- Pattern: `default_fields_fill_omitted` for the language; `inclusive_for_runs` for a file plus `exec`.

## Done criteria

- [ ] `Hit`, both `Texture`s, and `Mesh` have a default on every field, and no other struct in those files gained a default
- [ ] `cargo test omitted_hit_texture_mesh_use_zeros` exits 0
- [ ] `cargo test minecraft_project_typechecks defense` is not one filter; each named project test exits 0
- [ ] `cargo test` exits 0
- [ ] No example or other stdlib call site was rewritten to omit fields
- [ ] Plan 012 row is DONE

## STOP conditions

- `literal_default` rejects `ptr = 0` or `u32 = 0`. The excerpt in this plan would be wrong. STOP. Do not weaken the checker.
- `game.j3` and `game_headless.j3` `Texture` field lists differ (different names or types). STOP. Do not invent a default for a field that exists on only one side.
- Compiling the temp file fails for a reason other than link (`unknown` import, type error). STOP and report the diagnostic.
- You want to omit fields at existing call sites to "prove" the default. That is a follow-up. STOP if you think the plan is unfinished without that sweep.

## Maintenance notes

- A new field on `Hit`, `Texture`, or `Mesh` needs a default or every `{}` literal breaks. Reviewers should check both `Texture` definitions.
- Headless and SDL `Texture` must stay field-for-field. `08_game_loop` uses the headless game module.
- Call-site cleanup (`Hit {}` instead of eight zeros) is deliberately not in this plan.
