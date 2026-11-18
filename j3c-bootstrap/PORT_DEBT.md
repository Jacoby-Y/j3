# Port debt: Rust changes the port has to mirror

The Rust `j3c` is frozen for the port (`AGENTS.md`, plan 11 A8). Every Rust change made anyway gets one
line here, newest last:

`- <Rust commit>: <what changed> (<j3c/src/... files>) -> <j3c-bootstrap/src/... file to mirror it> | already mirrored`

Mark a line done by changing its target to `already mirrored` once the bootstrap file does the same
thing; delete done lines when stage 0 is bumped past them.

**Stage-0 bump rule:** `tools/stage0.sh` may move `stage0/j3c` to a newer commit only when every line
here says `already mirrored`, or each open line has a matching entry in `tests/known/<mode>.txt`.
Stdlib-only edits (`j3c/stdlib/`) need no line, but still need a bump for the oracle to see them.

## Ledger

- pre-freeze: `[package] bin` key, the program file name, default `name` (`j3c/src/project.rs`,
  `j3c/src/pkgtool.rs`, `j3c/src/devrun.rs`) -> mirrored by the port of `j3c-bootstrap/src/project/manifest.j3`
  (and `cmd/run.j3` for the `<bin>-dev` name)
- pre-freeze: `j3c dump-ast FILE`, S-expression AST on stdout, lex errors as `j3c fmt` prints them, parse errors as the rendered diagnostic, rc 1
  (`j3c/src/parse/dump.rs`, `j3c/src/cli/dump.rs`) -> mirrored by the port of `j3c-bootstrap/src/syntax/dump.j3`
  and `j3c-bootstrap/src/cmd/dump.j3`

## Stage-0 bugs (miscompiles in the Rust `j3c` the port works around; not Rust changes)

- (a) enum ABI align: a payload enum lowers to LLVM `{ i32, [N x i8] }` (align 4) while `ir/abi.rs` gives it its
  payload's align 8, so a struct with such a field at offset 4 mod 8, or whose size ends 4 mod 8, is laid out
  smaller by LLVM than the `memcpy` sizes assume (garbage `this`, lost fields). Workaround: payload-enum fields
  8-aligned, bools last, a `pad: u32` where needed (`ir.Func`, `GlobalDef`, `BinaryRv`, `IndexProj`, `LlvmFn`,
  `DeferFrame`, `syntax.ast.LetPatStmt`)
- (b) SIGSEGV returning a large payload struct out of a `match` arm (`StmtKind.Assign(a) { return a; }`). Workaround:
  work on the payload inside the arm
- (c) destroying and recreating a scratch `Arena` in a loop crashes only in split per-module builds (`[emit] chunks = 0`
  hides it). Workaround: per-fn LLVM buffers stay in the Session arena (`backend/llvm.j3` `emitFuncs`)
- (d) a `u64` inside `{..}` interpolation prints signed (18054884314459144840 prints -391859759250406776).
  Workaround: `Text.pushU64` (`backend/llvm/rvalue.j3` `constBits`)
- (a) also applied to sema: `ConstVal` (+ `pad`), `ConstDecl`, `FieldDecl`, `FnDecl`, `TForEach`, `TLetPat`, `traits.Member` (+ `pad`);
  and (C2b wave 7) `ir.UnaryRv` (operand first + `pad`), `ir.AssignOpStmt` (operand before `op` + `pad`),
  `project.manifest.Emit` (`chunks` first). None open from the last layout scan.
- (a) again in C2e (wave 17, `{#each}` segfault in `List<EachCx>.push`): an `Option` / payload-enum field right after
  a `bool`, `u8` or `i32` field can make `@sizeof` bigger than LLVM's struct (96 against 88 for the repro), so a
  by-value copy writes past its slot. Rule: payload enums before the small fields, bools last. Reordered:
  `ui.gen.EachHead`, `Virt`, `G`. Repro: repo-root `target/w18_layout_repro.j3` (`stage0/j3c run`: SIGSEGV in
  `List.push`; `7 7 3` with `flag` moved behind the two `Option`s). Gate: `tests/unit/layout_test.j3` compares
  `@sizeof` with the LLVM stride for 34 `src/` structs (the 31 pub ones a wave 18 scan found with the field pattern, plus `EachHead`, `EachCx`, `G`): none differ.
- Open (C2e wave 18, Rust side, not looked into): `stage0/j3c run --headless --frames 3 speculative/14_ui_slots` builds a
  program that dies with SIGSEGV in `Text.pushBytes` (from `Card.update`); the same with `~/.bin/j3c`. `run --fast` with
  stage 0 and the j3c-boot build both pass, and `dump-llvm` is byte-equal, so it is the split per-module build
  (compare (c)). `diff.sh run` on it is order-dependent: `run` says `up to date` on the other compiler's binary.
- Rust bug (not copied): a pattern warning in a part file prints the module main file as its path (`check/lower/pat/check.rs:320` uses `self.path`; `check/mod.rs:251` fixes the path for errors only). stage0 `src/util.j3:4:51`, boot `src/util/more.j3:4:51`: the one known `check` stderr difference.
- PORT_DEBT: j3c-boot links SDL2/ALSA/pulse/wayland because load uses j3.scene3d's `.j3scene` reader; fix = split the reader
  out of j3.scene3d in the stdlib (Rust tree change + stage0 re-pin), planned for C3/C5.
- Rust quirk (copied): an instance body is lowered with the origin module's names plus those of the module whose queue
  lowers it (`check/lower/setup.rs` `MergedEnv`), and a bare call resolves fns before locals, so `boot(mem)` in
  `App<W>.create` calls the user's `fn boot`, not the `boot` parameter, when the user module has one
  (`sema/body.j3` `lookupName`). Types and module aliases merge the same way (`resolveTyIn`, `prefixOf`); tracked in
  `tests/proj/merged_names`, `merged_types`. Not copied: Rust merges per map, so an origin const/type `X` + a user fn
  `X` + a local `X` calls the user fn; the port calls the local.
- Rust bug copied: two consts that name each other, read from shader code (`const A = B2; const B2 = A;`), overflow
  the stack in `const_ref` (`check/gpu`) / `constRef` (`shader`); stage0 rc 134, j3c-boot signal 11. Fix both with a
  `building` guard when Rust may change.
- Rust rule not copied (older than C2d): `alias.Enum.Variant` with the enum not imported by name. Rust
  `ctor_enum_id` (`check/lower/enums.rs`) looks the enum name up in the local `types` (or the expected type) and says
  "`Col` is not an enum"; the port (`sema/body_variant.j3` `enumCtor`) finds it in the alias's module and accepts
  the constructor. Both reject it as a match arm.
- Rust quirk (not copied): "did you mean" ties follow Rust `HashMap` order (C2c review r2#7); the port sorts candidates.
- Open (C2c review r3#3): a `validate` failure in j3c-boot is rendered as a plain driver error, not Rust's ICE text; no
  corpus file reaches it.
- Rust bug not copied (C2e review r1#2, r2 note): a `.j3ui` error whose span ends at a non-ASCII byte makes stage0 die
  with an internal compiler error (rc 70, `src/diag.rs:58`, "end byte index N is not a char boundary"); j3c-boot
  prints the proper error, rc 1. Repro, each as the only line of `ui/Main.j3ui` under `dump-ui ui`: `<div é="1">`,
  `<div a=é>`, `<dév>`, `<div class:é={x}>`, `<div a=&nbsp;"x">`. No `tests/bad-ui` case (it would sit in
  `known/ui.txt` for good); add them when Rust widens the span to the char.
- Open (C2e review r1#5): `ui/scan.j3` `scan` takes `extra` (unsaved editor buffers, Rust's overlay paths) and
  ignores it; the LSP milestone has to merge them in.
- Open (C3 contracts, wave 19): `gen::before_build` (Rust `gen.rs:148`, called from `build_inner`, `run_user`, `run_loaded`,
  `unchanged_project`) is the no-op `cmd/gen.j3` `beforeBuild` until C5: j3c-boot does not run a project's stale
  `assets/gen/*.j3` generators before a build (`examples/editor_village` is the one corpus project with any).
- Open (C3 wave 20, for C4): `cmd/stamp.j3` `StampFlags` has no `--emit` field, so the backend is not in
  `inputs.<kind>.key` (it is in `src.key` through `Options.emit`). Only LLVM builds today and `build` / `run` skip
  the stat shortcut for `--emit c`; add the field when the `bc` backend lands.
- Not copied (C3, by decision): the compiler id stamp is `~/.cache/j3/compiler-boot.id` (Rust: `compiler.id`). Its
  text is `size sec.nsec` (`fsx.statNs`, x86-64 Linux; seconds elsewhere); sharing Rust's file would make each compiler
  re-hash itself after the other ran. Object and stamp keys are `xxh3_64`, never equal to Rust's.
- Not copied (C3, by decision): the object cache's job pool is forked worker processes (`cache/jobs.j3`: emit +
  clang per missed module, reaped with `proc.waitAny`), where Rust has pool threads (`emit/incr.rs`). The Session
  arena is single-threaded; measured 9 s against 62 s serial for the self build. The time note keeps Rust's words
  (`N modules on W threads`). Two j3c-boot runs on one project race like Rust's (no `io.flock` on the cache dir yet).
- Open (C3 wave 20, for C5): `version --verbose` prints `docs: not in j3c-boot yet` (Rust hashes its doc and help
  sources, `stdlib::docs_id`) and its own path on the `binary` line; the other lines are equal.
- Open (C3 wave 20, for C5): no C fallback when clang is missing (`resolve_backend`); `opensslLibDir` and
  `clangAvailable` have no caller yet (apk, doctor); Android builds are checked by argv only.
- Stage-0 quirk (C3 wave 20; listed here, not a miscompile): a top-level fn named `less` in a file that calls
  `j3.cmp.sortBy` shadows `sortBy`'s `less` parameter when the instance is lowered (the `MergedEnv` rule above):
  a type error inside `cmp.j3`. Workaround: do not name a top-level fn `less` (or any stdlib generic's parameter name).
