# Plan 008: Incremental builds (one object per module, cached)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has uncommitted work on top of
> `b086a5b`, so check by comparing the "Current state" excerpts below to the
> live files, not by SHA. On a mismatch, STOP.

## Status

- **Priority**: P3
- **Effort**: L
- **Risk**: MED (linkage, debug info)
- **Depends on**: none
- **Category**: performance / dx
- **Planned at**: 2026-09-25

## Why this matters

A one-line change re-emits and recompiles the whole program. EDITOR.md "Performance >
Builds" measured about 0.7 s on the example project: 0.18 s emit plus 0.45 s clang.
Editor Play, MCP `play`, and `j3c run` pay it on every code edit. Most of the stdlib
(gfx, scene3d, ui, game) never changes between edits, so recompiling it is wasted work.

## Current state

**One IR text, one clang call that compiles and links.** `j3c/src/emit/mod.rs:27-45`:
```rust
pub fn emit_text(cx: &IrCx, modules: &[Module], opts: &EmitOpts) -> Result<String, String> {
    match opts.backend {
        Backend::C => emit_program(cx, modules),
        Backend::Llvm => emit_llvm(cx, modules, opts),
    }
}
pub fn compile_text(text: &str, opts: &EmitOpts) -> Result<std::path::PathBuf, String> {
    ... Backend::Llvm => host::compile_llvm(text, &opts.cc, &opts.clang, &opts.target, &opts.cpu, &opts.flags),
```

`j3c/src/emit/llvm.rs:18` `emit_llvm(cx, modules, opts)` writes, in order:
- struct/enum types for all modules
- `declare`s for `use lib` functions (deduped, lines 49-62)
- every module's globals as `internal global` (lines 67-80)
- every J3 function (lines 92-110)
- `define i32 @main()` (line 114)

Debug info is one `DICompileUnit` (`j3c/src/emit/llvm_expr.rs:1368`, `!llvm.dbg.cu` at
1691), prepared over all modules (`DebugOut::prepare(cx, modules)`).

`j3c/src/emit/host.rs:72-151` (`compile_llvm`) writes the `.ll` to a temp file and runs one
`clang -O<opt> -ffunction-sections -fdata-sections <gc-flag> [-g] -o exe file.ll <link args>`.
There's a whole-program cache: `build_key` (host.rs:153) hashes the IR text plus the
command, and a `<exe>.j3c-stamp` skips clang when nothing changed. `cli.rs:599-630`
(`source_key`/`up_to_date`) skips even the emit step when no source file changed.

Timings print with `J3C_TIMINGS=1` (`cli.rs:633` `timing`, `host.rs` "clang:").

## Design

**Phase A (the lazy big win): split only the clang step.**
Keep emit whole (0.18 s is fine) but produce **one `.ll` text per J3 module** instead of
one per program, compile each with `clang -c` into `target/j3c-obj/<module>-<hash>.o`
(hash = the module's `.ll` text plus flags), and skip any whose `.o` exists. Then link the
objects with one `clang` call (plus the existing link args). The stdlib modules' IR doesn't
change when user code changes, so their objects are reused. Compile misses in parallel with
`std::thread::scope` (one thread per missing object, capped at the CPU count).

What each per-module `.ll` needs:
- **Types:** all struct and enum type definitions (textual; duplicated per file is legal
  and cheap). Reuse `emit_struct_types`/`emit_enum_types` unchanged.
- **Defined here:** the module's globals and functions. Change `internal global` to plain
  (external) linkage with the same mangled names. They're already unique
  (`global_c_name`). Functions: check what linkage `emit_func` gives (internal/private
  functions must become external or `linkonce_odr`).
- **Used from elsewhere:** a `declare` for every function and an `external global` for
  every global the module references but doesn't define. Collect them while emitting (the
  function printer knows each callee) or conservatively declare everything the program
  defines that the module mentions (a scan of `@name` tokens in the module's text is the
  laziest correct way).
- **Generic instances:** find which module owns each monomorphized function in `modules`
  (read `ir/build.rs`/`ir/module.rs`). If an instance can be referenced from several
  modules but is emitted once, the declare/define scheme works as is. If the same instance
  can appear in two modules, emit it as `linkonce_odr`.
- **Shared constants:** the `@.j3_fmt_*` private strings go in each file that uses them
  (private is per file, so this is fine).
- **`main`:** only in the entry module's file.
- **`use lib` declares:** in each file that calls them.
- **Debug info:** one `DICompileUnit` per file. `DebugOut::prepare` takes the module list,
  so prepare it per module. If that's hard, Phase A applies only when `debug = false`
  (release/opt builds) and debug builds stay whole-program. That's a fine first cut, since
  it's the 0.45 s clang step that matters.

**When to use it:** only for `opt = 0` builds by default. At `-O2`, per-module compilation
loses cross-module inlining (the stdlib math and vector helpers are hot), so release builds
keep whole-program IR. `[emit] incremental = true|false` in j3.toml overrides this.

**Phase B (optional): skip emitting unchanged modules.** Cache each module's `.ll` text
keyed by a hash of the module's source file plus the hashes of every module it imports
(an interface change forces dependents to re-emit). Only if Phase A leaves emit as the
dominant cost.

C backend: out of scope (it stays whole-program).

## Commands you will need

| Command | Purpose |
|---|---|
| `cd j3c && cargo build --release && cargo test --release` | build and all tests |
| `J3C_TIMINGS=1 timeout 120 ./target/release/j3c build ../examples/3d-editor-scene` | per-phase timings |
| `touch`/edit one line in `examples/3d-editor-scene/src/behaviors/*.j3`, rebuild with timings | the incremental case |
| `timeout 60 ./target/release/j3c run --file examples/scene3d_check.j3` | runtime sanity |

## Scope

In: Phase A (per-module `.ll`, `clang -c`, object cache, parallel compile, link), the
opt-level policy, and the `incremental` toml key. Phase B only if measured to matter.
Out: the C backend, LTO, and caching across machines.

## Git workflow

Branch `plan-008-incremental-builds`. One commit for the per-module emitter (behind a
flag, off), one for the object cache and link, one for turning it on for opt 0.

## Steps

1. **Measure.** `J3C_TIMINGS=1` build of `examples/3d-editor-scene` and
   `examples/editor_village`, clean and after a one-line behavior edit. Record the numbers
   in Maintenance notes.
   Expected: about 0.18 s emit and 0.45 s clang on the example.
2. **Per-module emitter.** Add `emit_llvm_modules(cx, modules, opts) -> Vec<(String /*module name*/, String /*ll*/)>`
   next to `emit_llvm`, sharing the helpers. Leave `emit_llvm` unchanged.
   Verify: a unit test in `compile.rs` (next to the existing `emit_llvm` tests around line
   455) that compiles a two-module program. Run each `.ll` through
   `clang -c -x ir` (or `llc` if available) with `timeout`, link them, run the result, and
   check its output matches the whole-program build.
3. **Object cache plus link.** `host::compile_llvm_modules(parts, cc, clang, target, cpu, extra)`:
   hash, then `target/j3c-obj/`, parallel `clang -c -O<opt> [-g] -ffunction-sections -fdata-sections`,
   then one link call with `append_link_args`. Keep the `.j3c-stamp` whole-exe skip.
   Verify: build the example twice. The second run does zero `clang -c` calls (log each
   compile under `J3C_TIMINGS`).
4. **Policy.** In `emit::compile_text`'s caller (`cli.rs` build and run paths), use the
   per-module path when backend is LLVM, `opt == "0"`, and `incremental` isn't false, or
   when `incremental = true`. Debug builds: only if step 2 handled debug metadata;
   otherwise fall back to whole-program when `debug = true`.
   Verify: `cargo test --release` passes (it exercises both backends); the example runs
   and matches before/after screenshots through `--frames 30 --screenshot --headless`.
5. **Measure again.** A one-line edit in a behavior should rebuild in under about 0.35 s
   total on the example (emit 0.18 s plus one small `clang -c` plus link). Record it.
6. **Docs.** `j3c/AGENTS.md` (the emit/host rows), j3.toml docs for `incremental`, and
   EDITOR.md "Performance > Builds" with the new numbers.

## Test plan

- The two-module equivalence test (step 2).
- The existing `cargo test --release` suite, including the dual-backend acceptance
  programs.
- Headless screenshot equality on `examples/3d-editor-scene` between whole and incremental
  builds.

## Done criteria

- A one-line user-code change at opt 0 recompiles only the changed modules' objects.
- The rebuild is at least 40% faster than baseline.
- Whole-program builds are unchanged for opt ≥ 1 by default.

## STOP conditions

- Generic instances are emitted into more than one module and `linkonce_odr` isn't enough
  (duplicate symbol errors that need IR-level ownership changes).
- Debug metadata can't be split without rewriting `DebugOut`. Then ship non-debug
  incremental only and report.
- The link step alone takes more than the whole-program clang call did.

## Maintenance notes

- Baseline and after numbers: _fill in at steps 1 and 5_.
- `target/j3c-obj/` can be deleted any time. Add a size cap (keep the newest N per module)
  if it grows.
- Any new program-wide emitted symbol (like `@main` or the fmt strings) must decide which
  file it goes in.
