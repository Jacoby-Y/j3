# Plan 010: Clone check env maps only when a method has a foreign origin

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm `lower_fn` still
> clones nine maps before the method-origin `if`. On a mismatch, STOP.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: perf
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`lower_fn` clones `funcs`, `consts`, `types`, `lib_ns`, `mod_ns`,
`mod_types`, `globals`, `struct_temps`, and `struct_temp_env` for every
function. It replaces those clones only when the function is a method whose
struct has an entry in `origins`. Every other function pays nine `HashMap`
clones on every check, including LSP. Behavior must stay the same. This is
not a monomorphization redesign.

## Current state

`j3c/src/check/lower.rs` `lower_fn` starts around line 213. The clones:

```242:270:j3c/src/check/lower.rs
    let mut use_funcs = funcs.clone();
    let mut use_consts = consts.clone();
    let mut use_types = types.clone();
    let mut use_lib_ns = lib_ns.clone();
    let mut use_mod_ns = mod_ns.clone();
    let mut use_mod_types = mod_types.clone();
    let mut use_globals = globals.clone();
    let mut use_struct_temps = struct_temps.clone();
    let mut use_struct_temp_env = struct_temp_env.clone();
    let mut body_path = path.to_string();
    let mut inst_note: Option<String> = None;
    if let Some(sid) = cx.func(id).method_of {
        let sname = cx.name_str(cx.struct_def(sid).name).to_string();
        if let Some(o) = origins.get(&sname) {
            use_funcs = merge_map(&o.funcs, funcs);
            // same merge_map for the other eight
            if o.path != path {
                inst_note = Some(...);
                body_path = o.path.clone();
            }
        }
    }
```

`merge_map` is at line 67: `fn merge_map<V: Clone>(base: &HashMap<String, V>, extra: &HashMap<String, V>) -> HashMap<String, V>`.

Those maps are only read after that, as references on `Lower` (around lines 295–306). `lower_fn` ends around line 327 with `Ok(l.b.finish())`. Do not split `Lower` or move the statement loop.

`body_path` and `inst_note` still need to be computed in both paths. They are not the maps.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Goldens | `cargo test ir_goldens_01_through_08_and_samples -- --test-threads=1` | exit 0 |
| Projects | `cargo test defense_project_typechecks gallery_project_typechecks minecraft_project_typechecks -- --test-threads=1` | exit 0, or three separate `cargo test` invocations |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/check/lower.rs` (`lower_fn` from the clones through the `Lower { ... }` literal)

**Out of scope**:
- `lower_module`'s clones (around lines 83–107). Leave them.
- Monomorphization, `pending_mono`, CFG
- Any signature change of `lower_fn`

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: Own the merged maps only in the origin branch

Delete the nine eager `let mut use_* = ....clone()` lines.

Build an optional owned bundle when the method-origin branch actually merges:

```rust
struct MergedEnv {
    funcs: HashMap<String, FuncId>,
    consts: HashMap<String, ConstVal>,
    types: HashMap<String, Ty>,
    lib_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_ns: HashMap<String, HashMap<String, FuncId>>,
    mod_types: HashMap<String, HashMap<String, Ty>>,
    globals: HashMap<String, crate::ir::ids::GlobalId>,
    struct_temps: HashMap<String, parse::StructItem>,
    struct_temp_env: HashMap<String, HashMap<String, Ty>>,
}
```

Those are the types of the nine parameters `lower_fn` clones today (`funcs` through `struct_temp_env`). `methods`, `enum_temps`, and the `mut` maps are not in this list.

Put the struct next to `lower_fn` in the same file, private.

```rust
let merged: Option<MergedEnv> = if let Some(sid) = cx.func(id).method_of {
    let sname = cx.name_str(cx.struct_def(sid).name).to_string();
    if let Some(o) = origins.get(&sname) {
        if o.path != path {
            inst_note = Some(format!("while instantiating `{sname}` (from {path})"));
            body_path = o.path.clone();
        }
        Some(MergedEnv {
            funcs: merge_map(&o.funcs, funcs),
            // the other merge_map calls, same arguments as today
        })
    } else {
        None
    }
} else {
    None
};

let use_funcs = merged.as_ref().map(|m| &m.funcs).unwrap_or(funcs);
let use_consts = merged.as_ref().map(|m| &m.consts).unwrap_or(consts);
// same for the other seven
```

`merged` must live until `l.stmt` finishes. Do not clone in the `None` path.

Keep `inst_note` / `body_path` behavior identical, including the case
`method_of` is `Some` but `origins` has no entry (no merge, no note).

The `Lower` literal keeps `funcs: use_funcs` and the other reference fields.
`use_funcs` is now `&HashMap<...>`, so drop the extra `&` if the field was
`funcs: &use_funcs` and `use_funcs` is already a reference. Match whatever
compiles without changing `Lower`'s field types.

### Step 2: Tests that lock lowering

No new test is required if goldens pass: a wrong merge changes IR text.

**Verify**: `cargo test ir_goldens_01_through_08_and_samples -- --test-threads=1` → exit 0.
**Verify**: `cargo test defense_project_typechecks -- --test-threads=1` → exit 0.
**Verify**: `cargo test gallery_project_typechecks -- --test-threads=1` → exit 0.
**Verify**: `cargo test minecraft_project_typechecks -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- Existing goldens and the three project typecheck tests.
- Do not add a benchmark.

## Done criteria

- [ ] `rg "funcs.clone\\(\\)" j3c/src/check/lower.rs` does not find a clone of the `lower_fn` parameter `funcs` before the origin `if`. `merge_map` still runs inside the `Some(o)` branch.
- [ ] `cargo test ir_goldens_01_through_08_and_samples` exits 0
- [ ] `cargo test` exits 0
- [ ] Only `j3c/src/check/lower.rs` and `plans/README.md` change
- [ ] Plan 010 row is DONE

## STOP conditions

- The borrow checker rejects `merged.as_ref()` held across `l.stmt` after two small adjustments (reordering `let`s). STOP. Do not clone "just in case" to silence it.
- A golden diff appears. STOP. The change is not behavior-preserving. Do not update goldens.
- You find `use_funcs.insert` or any other mutation after the merge. STOP. The maps are not read-only and this plan's shape is wrong.

## Maintenance notes

- A new env map on `Lower` must be added to `MergedEnv` and to the `unwrap_or` line in the same change.
- `lower_module` still clones once per module. That is outside this plan.
