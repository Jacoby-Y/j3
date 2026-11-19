# Port debt: Rust changes the port has to mirror

Since plan 11 C6 the Rust tree exists only at the git tag `rust-final`; `j3c/src/...` paths below are
there, and `j3c-boot` is the old name of `target/j3c`. The ledger is history plus the open differences
from stage 0.

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
  (and `driver.devOut` / `cmd/devrun.j3` `devOut` for the `<bin>-dev` name, C4) | already mirrored
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
- Kept (C5 decision, closed): j3c-boot links SDL2/ALSA/pulse/wayland because load uses j3.scene3d's `.j3scene` reader and
  the in-process scene tools (`use j3_editor.tool`) pull `j3.gfx`/`j3.game`. No headless op calls GL or SDL. No stdlib
  split, no stage0 re-pin; revisit only if j3c-boot must run on a machine without SDL2.
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
- Closed (C5 wave 25): `ui/scan.j3` `scan` merges `extra` (the overlay paths) as Rust `scan.rs:48-53`.
- Closed (C5 wave 26): `gen::before_build` is real (`cmd/gen.j3` `beforeBuild`, honours `J3_GEN_RUNNING`).
- Note (C3 wave 20, checked C4): `cmd/stamp.j3` `StampFlags` has no `--emit` field, so the backend is not in
  `inputs.<kind>.key` (it is in `src.key` through `Options.emit`). Still only LLVM builds a program: `Bc.emit` is an
  error by design (Rust has no `--emit bc`; scripts reach `.j3sb` through `run` / `dump-bc`). Add the field if a
  second program backend ever lands.
- Not copied (C3, by decision): the compiler id stamp is `~/.cache/j3/compiler-boot.id` (Rust: `compiler.id`). Its
  text is `size sec.nsec` (`fsx.statNs`, x86-64 Linux; seconds elsewhere); sharing Rust's file would make each compiler
  re-hash itself after the other ran. Object and stamp keys are `xxh3_64`, never equal to Rust's.
- Not copied (C3, by decision): the object cache's job pool is forked worker processes (`cache/jobs.j3`: emit +
  clang per missed module, reaped with `proc.waitAny`), where Rust has pool threads (`emit/incr.rs`). The Session
  arena is single-threaded; measured 9 s against 62 s serial for the self build. The time note keeps Rust's words
  (`N modules on W threads`). Two j3c-boot runs on one project race like Rust's (no `io.flock` on the cache dir yet).
- Closed (C5 wave 26): `version --verbose` prints `docs  <docsId>`. Not copied, by design: the id hashes this binary's
  embedded stdlib and page text (FNV-1a 64), never Rust's `.rs`-source hash; `diff.sh tool` masks it. The `binary`
  line is j3c-boot's own path.
- Open (C3 wave 20): no C fallback when clang is missing (`resolve_backend`; no C backend, B8a). Closed in C5:
  `opensslLibDir` has its caller (`dist/apk`). Not copied: `doctor` runs `clang --version` itself as Rust does, so
  `clangAvailable` still has no caller there. Android builds are checked by argv only (`tests/unit/dist_apk_test.j3`).
- Stage-0 quirk (C3 wave 20; listed here, not a miscompile): a top-level fn named `less` in a file that calls
  `j3.cmp.sortBy` shadows `sortBy`'s `less` parameter when the instance is lowered (the `MergedEnv` rule above):
  a type error inside `cmp.j3`. Workaround: do not name a top-level fn `less` (or any stdlib generic's parameter name).
- Stage-0 bug (C4 wave 23): a `Set<u64>` in `devmode` as well as in `ir/validate.j3` makes stage 0 report "no method
  `new` on `Set`" in validate.j3. Workaround: `Map<u64, bool>` in `devmode`.
- Not copied (C4, internal): `devmode` gives `(*this).handle` `through_ref` access where Rust gives `read_only_view`
  (Rust never validates dev IR; the port does, `tests/unit/devmode_lower_test.j3`). Not in any compared output.
- Not copied (C4, `j3.proc` limits): the dev runner (`cmd/devrun.j3`) pipes the child's stdout and stderr and passes
  them through (`devrun.pump`; Rust inherits stdio), and closes its stdin. The
  `cli::timing` lines are not printed (Rust skips them too while the `[n/6]` progress is on).
- Not copied (C4, `j3.sys` has no `signal`): `run --hot` (`cmd/hot.j3`) has no SIGINT/SIGTERM trap, and a rebuild reads
  the new session's `[emit]` (Rust keeps the first `EmitOpts`: differs only when `j3.toml` changes mid-session).
  `pkgtool.run` sets the tool's env with `setenv` on this process before the fork (`proc` takes no env).
- Open (C4): `devmode.lower`'s "a script calls `X`, which the program can't link" has no corpus case (no valid
  source found that reaches it); the two other devmode refusals are `tests/proj/bc_refuse_dev_*` (`diff.sh bc`).
- Deferred (user, 2026-11-18): the LSP server and its scanners/`.j3ui` LSP, the MCP server, `j3c editor`, `-e android`,
  `-e web` and asset import (glTF, online, packs) are stubs (`not ported yet`, rc 1): plans/12-deferred-tools.md.
- Not copied (C5, B12.2 deferred): the open editor is reached through Rust's file protocol (`target/j3-editor/events`
  + `replies`, `lock` try-lock), not `j3.instance`: that needs an edit in the frozen `j3c/editor/src`.
- Not copied (C5, by decision): the editor program is cached in `~/.cache/j3/editor-boot/<xxh3>` (Rust
  `~/.cache/j3/editor`; Rust's `cleanup_frontends` deletes hash dirs it does not know). `editor/frontend` holds its
  `in-use.lock` only while it publishes and cleans up (`j3.io` has exclusive locks only).
- Not copied (C5, B12.5): LSP hover/definition of a name defined twice (`new`, `push`, ...) resolves in sema decl
  order where Rust uses IR func order, and stdlib generic types show the declared, not an instance's, type args.
  Single-definition names match byte for byte.
- Not copied (C5, `lsp/proto`): an unknown method gets -32601 and a bad JSON body -32700 (Rust: `null` / skipped).
- Not copied (C5): `phase::run_ticking` lives in `toolchain.runTicking` (C3), not in `base/phase`.
- Not copied (C5, x86-64 Linux only): Rust `fs` file modes (`0666 & ~umask` writes, `0777 & ~umask` folders, copies
  keep the mode) go through raw `newfstatat`/`fchmodat` in `base/fsx` (`writeFile`, `makeDirs`, `copyFile`); other
  hosts keep `j3.io`'s modes. The `.deb` `ar` member mtime is the `control.tar.gz` mtime (no wall clock in `j3.sys`).
- Not copied (C5): `golden --bless` with no dir prints usage, rc 2 (Rust panics, rc 101); a tool killed by a signal
  prints `exit status: 128+N` in `doctor`, `golden`, `dist` (`j3.proc` folds signals; Rust `signal: N`).
- Rust bug (C5, not copied): stage0's `lsp/json.rs` reads raw UTF-8 inside JSON strings byte by byte, so `'é'` in a
  `didOpen` buffer gives a bogus parse error; j3c-boot uses `j3.data.json`.
- Not copied (plan 13 speed, `cache/jobs.j3`): clang fan-out is a fixed pool of forked workers fed unit indices on a pipe (costliest first; chunk clangs started from the worker after sequential chunk emits), with a per-unit scratch arena; Rust uses one pool thread per core plus a scoped thread per chunk.
- Not copied (plan 13 speed, `toolchain.spawnStart`/`spawnRun`): per-module clangs start with libc `posix_spawn`, not `proc.run` (fork); SIGPIPE is left as the process has it.
- Not copied (plan 13 speed, `cache/key.j3`): the module key is `xxh3_64(moduleIr)` streamed piece by piece (a `\n` after each fn's key text) and the layout digest is the per-entry hashes plus count; Rust hashes the joined text. Keys are j3c-boot's own.
- Not copied (plan 13 speed, `backend/llvm/names.j3`): `pushFuncName`'s hash input goes in a static 4 KB buffer (Rust: a fresh `DefaultHasher` stream).
- Not copied (plan 13 speed, `backend/llvm/prune.j3`): in-place line walks, name sets of declared names only, `out` pre-sized (Rust: `lines()` plus a `HashSet` of every token).
- Not copied (plan 13 speed, `backend/llvm.j3`): the module text is pre-reserved at 64 MB in `out.arena`, `DebugOut` lives there, and `LlvmFn` emit buffers are reused across fns.
- Not copied (plan 13 speed, `doc/`): `docsId` hashes with XXH3, not FNV-1a (the value never matched Rust); the `doc` index reads the embedded stdlib's heads as text instead of parsing it, and search/byName pre-filter by text; `doc/load.j3` caches the stdlib folders process-wide (Rust recomputes them per call).
- Not copied (plan 13 speed, memory): frame-string walks use a scratch arena plus `peek`/`derefSelect` instead of clones; token lists, lower scratch and UI generation get their own per-Session arenas; `room` grows pools 8x; `BodyBuilder.newIn` copies lists out at exact size; `Place.project` makes an exact copy; `dumpIrTo` streams its output.
- Not copied (plan 13 speed, `base/ids.j3`): pool growth releases the old buffer's pages with `madvise`.
- Not copied (plan 13 speed, `cmd/check`, `ir`): `check` drops validated `stdlib/j3/` IR bodies down to their params (process global `ir.dropStdBodies`/`keepBodies`, set only by `cmd/check`); Rust keeps them all.
- Not copied (plan 13 speed, `syntax/token.j3`): `keyword` is a hard-coded `kindOf`; `keywords.txt` stays the source for textmate scopes and the type and literal lists, and a unit test ties the two together.
- Not copied (plan 13 speed, `load/scene_parse.j3`): a hand copy of the stdlib `.j3scene` reader (`j3.scene3d.parse`) so j3c-boot does not link libSDL2; a rule changed in the stdlib reader must change there too (`diff.sh tool` scene/* and `diff.sh run` catch drift).
- Not copied (plan 13 speed, `scene/ops.j3`): scene tool requests run the editor program in its `cli` mode as a subprocess, as Rust does, reverting B12.1's in-process `j3_editor.tool` (cost: one editor build per new j3c-boot binary, one process per request).
- Not copied (`base/toml`): lines are read by the stdlib's `j3.data.toml.items` (one TOML reader for games and the compiler), so `j3.toml` reads full TOML lines where Rust read a subset: multi-line arrays with comments, `'literal'` and `"""multi-line"""` strings, dotted keys (`a.b = 1` under `[s]` is `b` in `[s.a]`), numbers like `1_000` / `1E+5` / `0xff` (kept as written, as Rust kept bare tokens). Differs from stage 0 on: broken headers (`[a b]`, `[]`, `[a..b]` read `bad table header`, Rust read `key outside a section` or accepted an empty part), text after a value (`b = x y`: ``unexpected `y` after the value``, Rust `expected a "string"`), `[[x]]` (its own error), and strings come back re-escaped (`\u00e9` as `é`; `unescape` gives the same text). `tests/unit/base_toml_test.j3` lists the cases (`ours`).
