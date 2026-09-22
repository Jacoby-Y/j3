# Plan 009: Darwin links with -dead_strip, not --gc-sections

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, update the status row for this plan
> in `plans/README.md`.
>
> **Drift check (run first)**: Not a git repository. Confirm both flag sites.
> On a mismatch, STOP.

## Status

- **Priority**: P3
- **Effort**: S
- **Risk**: LOW
- **Depends on**: none
- **Category**: migration
- **Planned at**: no git commit, working tree 2026-09-22

## Why this matters

`compile_llvm` and `compile_c` always pass `-Wl,--gc-sections`. That flag is
GNU ld. macOS ld64 rejects it. The repo already special-cases Apple targets
for rpath (`foreign_target` in `j3c/src/emit/host.rs`) and ships
`j3c/stdlib/j3/sys_macos.j3`. An empty target string means the host. This
machine is Linux, so an empty target must keep `--gc-sections`.

## Current state

`j3c/src/emit/host.rs`:

```104:109:j3c/src/emit/host.rs
    let mut cmd = Command::new(clang);
    cmd.arg("-Wno-override-module")
        .arg(format!("-O{opt}"))
        .arg("-ffunction-sections")
        .arg("-fdata-sections")
        .arg("-Wl,--gc-sections");
```

The C path repeats the same three section flags around lines 175–179.

```217:220:j3c/src/emit/host.rs
fn foreign_target(target: &str) -> bool {
    let t = target.to_ascii_lowercase();
    t.contains("windows") || t.contains("apple") || t.contains("darwin")
}
```

Windows/mingw accepts `--gc-sections`. Only Apple/Darwin changes.

`host.rs` has no `#[cfg(test)]` module. Add one. Do not invoke clang in the test.

## Commands you will need

From `j3c/`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Flag test | `cargo test link_gc_flag_matches_target -- --test-threads=1` | exit 0 |
| Still links hello | `cargo test hello_j3_prints_hello_world c_backend_still_runs_hello -- --test-threads=1` | exit 0 if one filter works; otherwise run the two names as two commands |
| Full | `cargo test` | exit 0 |

## Scope

**In scope**:
- `j3c/src/emit/host.rs`

**Out of scope**:
- rpath, `-l` search paths, `sys_macos.j3`
- Actually cross-linking to Darwin (no macOS SDK required)

## Git workflow

No git repository. Do not commit.

## Steps

### Step 1: One function, two call sites

Add:

```rust
fn link_gc_flag(target: &str) -> &'static str {
    let t = target.to_ascii_lowercase();
    if t.contains("apple") || t.contains("darwin") {
        "-Wl,-dead_strip"
    } else {
        "-Wl,--gc-sections"
    }
}
```

Replace both `.arg("-Wl,--gc-sections")` with `.arg(link_gc_flag(target))`.
`compile_llvm` and `compile_c` already have a `target: &str` parameter.
Empty `target` hits the else branch.

### Step 2: Test the function

`#[cfg(test)] mod tests` in `host.rs`:

```rust
#[test]
fn link_gc_flag_matches_target() {
    assert_eq!(super::link_gc_flag(""), "-Wl,--gc-sections");
    assert_eq!(super::link_gc_flag("x86_64-pc-linux-gnu"), "-Wl,--gc-sections");
    assert_eq!(super::link_gc_flag("x86_64-pc-windows-gnu"), "-Wl,--gc-sections");
    assert_eq!(super::link_gc_flag("arm64-apple-darwin"), "-Wl,-dead_strip");
    assert_eq!(super::link_gc_flag("x86_64-apple-darwin"), "-Wl,-dead_strip");
}
```

The function is private. `super::` works from a child module.

**Verify**: `cargo test link_gc_flag_matches_target -- --test-threads=1` → exit 0.
**Verify**: `cargo test hello_j3_prints_hello_world -- --test-threads=1` → exit 0.
**Verify**: `cargo test c_backend_still_runs_hello -- --test-threads=1` → exit 0.
**Verify**: `cargo test` → exit 0.

## Test plan

- `link_gc_flag_matches_target` only. No clang spawn.

## Done criteria

- [ ] `rg -- "--gc-sections" j3c/src/emit/host.rs` shows the string only inside `link_gc_flag`
- [ ] Both `compile_llvm` and `compile_c` pass `link_gc_flag(target)`
- [ ] The four asserts in the test exist and `cargo test link_gc_flag_matches_target` exits 0
- [ ] `cargo test` exits 0
- [ ] Plan 009 row is DONE

## STOP conditions

- `compile_llvm` or `compile_c` no longer takes `target: &str`. STOP and report the signature. Do not guess host OS with `cfg`.
- Hello fails to link after the change on this Linux machine. Revert the empty-string branch to `--gc-sections` if you pointed empty at `-dead_strip`. If empty already uses `--gc-sections` and hello still fails, STOP.

## Maintenance notes

- A new compile entry point that builds a `Command` for clang must use `link_gc_flag`.
- `-ffunction-sections` / `-fdata-sections` stay on Darwin. This plan only changes the linker flag.
