# Plan 10: The `steam` package (`../j3-packages/j3-steam`)

**Status (2026-11-16): DONE.** Tag `v0.1.0` in the local repo `../j3-packages/j3-steam` (no remote yet;
the `git` URL in the snippet below is a placeholder until it is pushed). Plan 09 chunks 6–7 are done (Steam is out of `j3c`).

## Goal

Everything Steam lives in one package repo: the bindings (today's `j3.steam`), the SDK checker,
the depot/`.vdf` dist output, the dev env, the docs, the tests and the verification checklist
(today's plan 01). A game opts in with:

```toml
[dependencies]
steam = { git = "https://github.com/<you>/j3-steam", tag = "v0.1.0" }

[dist]
build_outputs = ["zip", "steam"]

[steam]
app_id = "480"
sdk = "~/sdk/steamworks"
```

```j3
use steam;                  // was: use j3.steam;
use steam.game.attach;      // was: use j3.steam.game.attach;
```

and gets `j3c pkg steam check --sdk DIR`.

The folder is `j3-steam`, the package name is `steam` (names can't have `-`). The package is
self-contained: it reads no `j3c` source at runtime and depends only on the public stdlib.

## Note for the agent doing this

The `hello`/`qr` package `AGENTS.md` (from `j3c new --lib`) says never to read the `j3c` repo.
**That rule does not apply to this plan**: the job is to port code out of
`../../j3/j3c/stdlib/j3/steam*` and `../../j3/j3c/src/{steamcheck,dist/steam.rs,project/steam.rs}`.
Write the package's own `AGENTS.md` without that rule. Say instead that the package's source of
truth is itself, and that `j3c` changes it needs go through the j3 repo.

## Layout

As built (differs from the first draft: parts need a folder, and tool entries cannot `use` files in `tools/`):

```
j3-steam/
  j3.toml                 [package] name = "steam", j3 = ">=0.1.0"; [lib];
                          [commands] check; [dist_outputs] steam
  src/steam.j3            module steam;           (was j3/steam.j3)
  src/steam/*.j3          its parts, each `module steam;` (was j3/steam/<part>.j3) + devenv.j3 (devEnv)
  src/game.j3             module steam.game;      (was j3/steam/game.j3)
  src/ui.j3               module steam.ui;        (was j3/steam/ui.j3)
  src/tool/config.j3      steam.tool.config: [steam] from $J3_MANIFEST (shared by both tools)
  src/tool/dist.j3        steam.tool.dist (+ dist/glibc.j3): port of src/dist/steam.rs; over glibc_max warns (Rust parity)
  src/tool/check.j3       steam.tool.check (+ check/{scan,header,calls,consts,probe,text}.j3): port of src/steamcheck/
  tools/check.j3          thin entry for `j3c pkg steam check`
  tools/dist.j3           thin entry for the `steam` dist output
  tools/build-sniper.sh   embedded by the dist tool; tools/README.txt.in its README template
  tests/                  a project: [dependencies] steam = "..", one module per test, run.sh (one process each)
  tests/check/            fake SDK headers for the checker tests
  examples/spacewar/      app 480: the old recipe program
  README.md               the old `j3c doc steam` page + the recipe, adapted
  VERIFY.md               the old plans/01-steam.md, commands updated
  AGENTS.md               map, how to test, new-part rules
```

Tool code is in `src/tool/` (not `tools/`) because a tool entry can only `use` the package's `src/` modules
and its own parts folder; it also lets `tests/` call it directly.

## Chunks

Work in `../j3-packages/j3-steam` with `git init` first (each chunk is a commit). Use the
installed `j3c` once plan 09 chunk 5 has been built with `./build.sh`. Wrap runs in `timeout`.
Every run is headless (`--headless`), and nothing needs a real Steam client.

1. **Copy and rename the bindings.**
   - Run `j3c new j3-steam --lib`, then fix `[package] name = "steam"` (or create the files by
     hand; check what `new` does with a `-` in the name first).
   - Copy `j3c/stdlib/j3/steam.j3` → `src/steam.j3` and `steam/*.j3` → `src/*.j3`. Then rewrite:
     - `module j3.steam` → `module steam`
     - `use j3.steam` → `use steam`
     - doc comments saying `j3.steam` → `steam`
     - `[steam] sdk` wording: unchanged
   - Check the `FILES` / `registerParts` lists in `steam.j3` (the j3c `AGENTS.md` row says a new
     part goes there): any module path strings inside them need the rename too.
   - `j3c check` in the package must pass. If something only worked because it was in the stdlib
     (a private name, an attribute only the stdlib may use, embedding), **stop** and write it down
     here: it becomes a plan 09 item, not a workaround.

2. **Tests.**
   - `tests/` is a project depending on `steam = ".."`. Each `stdlib/tests/steam_*_test.j3` becomes
     `tests/src/<name>.j3` with `pub fn run()` instead of `fn main` (and the `use j3.steam` rename).
   - `tests/src/main.j3` calls each one and prints `<name> ok`. `tests/run.sh` runs
     `j3c run tests --headless` once per backend: these were LLVM only (`stdlib_cmp.rs` says why),
     so keep LLVM.
   - Match today's outputs exactly (`steam_test ok`, ...), so a diff against the cargo tests'
     expected strings is the check.

3. **Dev env in the library** (replaces `j3c`'s `dev_env`).
   - In `steam.init` (before the lib loads, i.e. before `fns.load` reads `J3_STEAM_LIB`), under
     `if build.DEV`:
     - when `SteamAppId` is unset and `j3.toml` is in the cwd, read `[steam] app_id` and `sdk`
       from it, then `sys.setenv` `SteamAppId`, `SteamGameId`, and `J3_STEAM_LIB` (the SDK's
       `redistributable_bin/linux64/libsteam_api.so`, or `win64/steam_api64.dll` on Windows),
       each only if unset.
     - reading: a tiny line scanner for those two keys in the `[steam]` table (`key = "value"`
       lines only). No TOML parser. Name its ceiling in a comment: inline tables and multi-line
       values are not read.
   - Release builds don't do this. Steam sets the app id when it launches the game.
   - Test (`tests/`): a temp dir with a `j3.toml` holding `[steam] app_id = "480"`, cwd there,
     `SteamAppId` unset → after the dev-env step, `sys.env("SteamAppId") == "480"`. Expose the step
     as `pub fn devEnv(dir)` so the test doesn't need init to succeed.

4. **`tools/config.j3` and `tools/dist.j3`** (port of `dist/steam.rs`, `project/steam.rs`).
   - `config.j3`: read `$J3_MANIFEST` with `j3.data.json` and check the `[steam]` keys exactly as
     `project/steam.rs` does: same keys, same messages (prefix `steam:` instead of `j3c: j3.toml:`),
     `~/` and project-relative path resolution, `glibc_max` default `"2.31"`.
   - `dist.j3`: the plan 09 decision-5 flags (`--dist`, `--built`, `--stage`, `--name`, `--version`,
     `--release`). Lay out `target/dist/steam/{content/<os>, scripts/app_build_<id>.vdf,
     scripts/README.txt, scripts/build-sniper.sh}` from the `--stage` folders. Copy
     `libsteam_api.so` / `steam_api64.dll` and `runtime_libs`. Run the GLIBC scan (`objdump -T`
     via `j3.proc`, error over `glibc_max`). Print the steam folder as the last line.
   - Port `dist/steam/tests.rs` to `tests/`: given a fake SDK and fake staged folders, the vdf text
     and the tree match what the Rust test expects. Copy the expected strings over verbatim.
   - Then end to end: `examples/spacewar` with `build_outputs = ["steam"]`, then
     `j3c build --release` writes the same tree today's `j3c` writes for the same project. Diff
     the two trees (build once with the old `j3c` first and keep the output in scratch).

5. **`tools/check.j3`** (port of `src/steamcheck/`, about 1.2k lines of Rust).
   - Same CLI: `--sdk DIR [--no-compile] [--no-windows]`.
   - Sources: `$J3_PKG_ROOT/src/*.j3` instead of the embedded stdlib. File names in messages
     become `src/pack.j3:57`.
   - Port file by file (`scan` → `header` → `calls` → `consts` → `probe`). The probe writes C++,
     runs `clang++` (pack 4) and mingw `g++` (pack 8) with `j3.proc`, and parses the diagnostics.
     `consts.rs` `CONSTS` / `UNCHECKED` become tables in `tools/check/consts.j3`.
   - Port `steamcheck/tests.rs` to `tests/` as `tests/check/` fixtures: the fake
     `steam_api_flat.h` SDKs the Rust tests write. Each Rust test is one check with the same
     expected findings.
   - Parity: run both the old `j3c -e steam-check` and `j3c pkg steam check` on the same fixture
     SDKs, and on the real SDK if one is on the machine. The findings must be the same (paths
     aside).
   - `[commands] check = { file = "tools/check.j3", help = "verify the bindings against a Steamworks SDK" }`.

6. **Docs, example, release.**
   - `README.md`: today's `j3c/src/doc/pages/steam.md` + `helptext/recipes/steam.md`, with the
     renames, the dependency line, `j3c pkg steam check`, the dist output, and the dev env note.
     Drop anything about `j3c -e steam-check`.
   - `examples/spacewar` is `helptext/recipes/steam_main.j3`. `j3c check examples/spacewar` and a
     headless `J3_STEAM_FAKE=1` run must pass.
   - `VERIFY.md` = `plans/01-steam.md` with commands updated. `AGENTS.md`: the map (the j3c
     `AGENTS.md` row 92 text is a good start), how to run tests, "new part: add to `FILES`,
     register callbacks, add rows `check` reads".
   - Tag `v0.1.0`, then hand back to plan 09 chunk 6 with the tag or URL. Plan 09 chunk 7 points
     `j3c doc steam` at this repo.

## Done when

- `tests/run.sh` prints every `<name> ok`.
- `j3c pkg steam check` matches the old checker on every fixture.
- `j3c build --release` of `examples/spacewar` matches the old dist tree.
- A game that only swaps `use j3.steam` for `use steam` plus the dependency builds and runs the
  same, fake mode included.

## Orchestrated chunks

- [x] A. Plan chunks 1–3: package skeleton (git init), bindings copied and renamed, `tests/` project + `tests/run.sh`, dev env (`devEnv`). Done: parts in `src/steam/` (loader needs the folder), `game.j3`/`ui.j3` flat; run.sh runs one process per test; devEnv in `src/steam/devenv.j3`.
- [x] B. Plan chunk 4: `tools/config.j3`, `tools/dist.j3`, `tests/` port of `dist/steam/tests.rs`, `examples/spacewar` built end to end vs the old tree. Done: tool code in `src/tool/` (tool entries can't `use` siblings in `tools/`), thin `tools/dist.j3`; over `glibc_max` warns (Rust parity); old/new trees and modes identical.
- [x] C. Plan chunk 5: `tools/check.j3` + parts, fixtures from `steamcheck/tests.rs`, parity with `j3c -e steam-check`. Done: `src/tool/check.j3` + parts, `tools/check.j3` entry; 67 parity runs identical (paths aside).
- [x] D. Plan chunk 6: README, VERIFY, AGENTS, example check/run, tag `v0.1.0`; plan status updates in j3. Done: README says the built-in `steam` dist output shadows the package's until plan 09 chunk 6.
