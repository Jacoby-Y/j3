# Plan 09: Package tools (commands and dist hooks), then Steam out of `j3c`

**Status (2026-11-16): done** (chunks 1–7; Steam is out of `j3c` and lives in the `steam` package, tag `v0.1.0` in the local repo `../j3-packages/j3-steam`, no remote yet). Open follow-ups: the unresolved items under "Found while doing plan 10". Follow-up in the package repo: [plan 10](10-steam-package.md).

## Goal

Packages can ship **tools**: J3 programs that `j3c` builds for the host and runs on request. Two
kinds:

- **Commands**, run by the developer: `j3c pkg steam check --sdk DIR` (like `npx` / `cargo run -p`).
- **Dist outputs**, run by `j3c build --release` when the project's `[dist] build_outputs` names one
  the package provides (`"steam"`).

Then every Steam piece leaves `j3c`: `j3.steam` (stdlib), `-e steam-check`, `[steam]` parsing,
`dist/steam.rs`, `dev_env`. `j3c` ends up knowing nothing about Steam.

The trust rule stays as `packages.md` states it: **nothing runs on fetch**. Package code runs only
when the developer types `j3c pkg ...`, or when their own `j3.toml` names a dist output the package
provides. `j3c add` / `fetch` / `update` never run anything.

## Current state (verified 2026-11-16)

- Packages: `[dependencies]` path/git, `j3.lock`, `src/project/deps.rs` (`Dep { name, root }`, where
  `root` is the dep's `src/`), `src/cli/pkg.rs` (`fetch`, `update`, `add`). Source only, no hooks.
- Unknown `j3.toml` **tables** are ignored. Only known tables check their keys (`project/steam.rs`
  warns on unknown `steam.*` keys). So once `j3c` stops parsing `[steam]`, the table can stay in the
  user's `j3.toml` and the package reads it.
- `[dist] build_outputs` is a closed enum (`project.rs:155`, `Output::{Zip,Deb,Apk,Steam}`). An
  unknown name is an error (`project.rs:475`).
- `dist::package` (`src/dist.rs:20`) gets `built: &[(triple, exe path)]`, stages per target, and
  calls `steam::plan` / `stage` / `finish`.
- `dev_env` (`dist/steam.rs:259`) is injected into `devrun.rs:212`, `emit/host.rs:431`,
  `editor/play.rs:76` and `mcp/ops.rs:296`. It sets `SteamAppId`, `SteamGameId` and `J3_STEAM_LIB`
  (from `[steam] sdk`).
- There is no TOML reader in the stdlib; `j3.data.json` exists.
- **Plan 06 (web)** designs the same shape for `[platform]` link/run scripts: a J3 program in a
  package, run by `j3c` with flags, exit code = the build's. **This plan builds the shared runner
  (`src/pkgtool.rs`); plan 06 chunk 2 uses it** instead of its own `j3c run --file` call.
- `j3c-bootstrap/src/steamcheck*`, `project/steam.j3` and `dist/steam.j3` are stubs (ports of the
  Rust files). They go when the Rust goes.

## Found while doing plan 10

- `j3c doc` cannot read package modules: in a project with the dep, `j3c doc steam.game` prints the stdlib's `j3.steam.game` and `j3c doc steam devEnv` hits the built-in `steam` page. Fix: doc lookup for dependencies, or at least drop the built-in page (chunks 6/7). **Partly resolved (chunks 6–7):** `j3.steam` is gone and `j3c doc steam` is now a short page pointing at the package; doc lookup for dependency modules is still open.
- A tool entry cannot `use` sibling files in `tools/` (only the package's `src/` and the entry's own parts folder). The steam package put tool code in `src/tool/`. Fix: document that in `j3c doc packages`, or resolve modules for tool entries from `tools/` too. **Resolved (chunk 7):** documented in `j3c doc packages`.
- `j3.io` has no chmod or mode-preserving copy (`copyFile` always writes 0644), so tools that ship executables shell out to `cp --preserve=mode` / `chmod`. Fix: add `io.chmod` / a mode-keeping `copyFile`.
- The manifest JSON loses a bare number's spelling: `glibc_max = 2.30` reaches tools as `2.3`, and `app_id = 1e3` passes where j3c says no. Quoted values are exact. Fix: keep the source text of bare numbers in `to_json`, or document "quote it".
- `project/steam.rs` still validates `[steam]` natively; with the dep present both j3c and the package warn on an unknown key. It must leave j3c together with chunk 6 (the package's `src/tool/config.j3` has the same keys and messages). **Resolved (chunk 6):** `project/steam.rs` is deleted.

## Decisions

1. **Declared in the package's `j3.toml`:**
   ```toml
   [commands]
   check = "tools/check.j3"          # j3c pkg steam check ...
   upload = { file = "tools/upload.j3", help = "build depots and print the steamcmd line" }

   [dist_outputs]
   steam = "tools/dist.j3"           # [dist] build_outputs = ["steam"] in the game
   ```
   Paths are relative to the package root (beside `j3.toml`, not `src/`). A command name uses
   letters, digits, `-` and `_`. `help` is optional (one line, shown by `j3c pkg steam`).
   `Dep` gains `pkg_root: PathBuf` (the folder with `j3.toml`), `commands: Vec<Tool>` and
   `dist_outputs: Vec<Tool>`.

2. **One runner, `src/pkgtool.rs`:** `build(dep, tool) -> exe path` and `run(dep, tool, args, env) -> exit code`.
   - Build: the tool file is the entry, the package is the project (so `use steam.x;` resolves to
     the package's own modules exactly as in its `src/`), the host triple, `opt = 0`, no window.
   - Cache: `~/.cache/j3/tools/<pkg>-<hash>/<tool>`, where the hash covers the `j3c` version, the
     tool name, and every `.j3` file under the package's `src/` and `tools/`. `J3_PKG_DIR` moves the
     cache, like the pkgs cache. Rebuilding prints `building steam:check (first run)` to stderr.
   - Run: cwd = the **user's** project root. Env:
     - `J3_PROJECT_ROOT`, `J3_PKG_ROOT`, `J3_PKG_NAME`, `J3C` (absolute path of the running
       `j3c`, so a tool can call `j3c -e ...` or `j3c build`)
     - `J3_TOOL_API=1` (bump it when the contract changes; a tool checks it)
     - `J3_MANIFEST`: a path to `target/.j3c/manifest.json` (decision 3)

   Stdout, stderr and the exit code pass straight through.

3. **Config reaches tools as JSON, not TOML.** `j3c` writes the parsed `j3.toml` (every table,
   unknown ones included) to `target/.j3c/manifest.json` before it runs a tool, and also prints it
   with `j3c manifest --json` (useful for any script). Tools read it with `j3.data.json`. No second
   TOML parser. `toml.rs` already has `toml_doc`; add `to_json`.

4. **`j3c pkg` CLI:**
   - `j3c pkg` lists every dep's commands and dist outputs with their help.
   - `j3c pkg NAME` lists that dep's commands.
   - `j3c pkg NAME CMD [args...]` runs one; args after `CMD` go to the tool untouched.
   - `j3c pkg NAME CMD --help` is passed through (the tool answers it).

   An unknown dep or command is an error that lists what exists. Exit code = the tool's.

5. **Dist outputs.**
   - `build_outputs` accepts any name. The built-ins (`zip`, `deb`, `apk`) win. Any other name must
     be in exactly one dep's `[dist_outputs]`; otherwise the error says
     `dist.build_outputs: no package provides "steam" (add one under [dependencies])`.
   - After `dist::package` stages its built-ins, each package output runs once with:
     - `--dist <abs target/dist>`
     - `--built <triple>=<abs exe>`, repeated per built target (an empty triple means the host)
     - `--stage <triple>=<abs staged folder>` (exe plus `dist::include` files, the same tree the
       zip packs; write it for package outputs even when zip is off)
     - `--name`, `--version`, `--release` (when set)
   - A non-zero exit fails the build with that code and the output's name. The folder the tool
     prints on its **last stdout line** goes into the build's "wrote" list.

6. **Dev env moves into the library, not into a hook.** `j3c run` / golden / `play` / editor play
   already run with cwd = project root. The `steam` package reads `[steam]` itself at init in dev
   builds (plan 10). `j3c` loses `dev_env` and gains nothing. A generic `[run] env = {...}` table is
   **not** added (YAGNI; add it when a second package needs run env).

7. **Not in this plan:** package tools for `j3c check` / `run` hooks, a registry, version ranges,
   tools in non-host languages, sandboxing tools (they run as the user, like `cargo run`; say so).

## Chunks

Build and test from `j3c/` with `J3C_NO_INSTALL=1`. Every chunk leaves `cargo test` green.

1. [x] **Manifest to JSON, and `sys.setenv`.** (done: Windows uses SetEnvironmentVariableA)
   - Files: `src/project/toml.rs` (`to_json(&TomlTable) -> String`: strings, ints, bools, arrays,
     inline tables, nested tables), `src/cli/mod.rs` (`j3c manifest [--json] [path]`; plain
     `manifest` is still the help topic), `src/helptext/manifest.md` (one line).
   - `stdlib/j3/sys.j3` gets `setenv(name, value)` (libc `setenv`, overwrite). Add it to
     `sys_windows.j3` (`_putenv_s`) and `sys_macos.j3`. The steam package needs it to set
     `SteamAppId` before init in dev (decision 6). There is only `env` / `environ` today.
   - Test (`project/tests.rs`): a `j3.toml` with `[package]`, `[steam]` (strings), `[dist]` (array)
     and an inline table round-trips to the expected JSON text. A `stdlib/tests` program sets then
     reads back a variable.

2. [x] **Parse `[commands]` / `[dist_outputs]` into `Dep`.**
   - Files: `src/project/deps.rs` (read the dep's own `j3.toml` when present; `pkg_root`, `Tool { name, file, help }`).
   - Errors (each names the dep and key): a bad command name, a missing file, a non-string value,
     two deps providing the same dist output.
   - Test (`project/tests.rs`): a fixture package with both tables resolves. A missing tool file is
     the named error.

3. [x] **`src/pkgtool.rs` runner and `j3c pkg`.**
   - Files: `src/pkgtool.rs` (decision 2), `src/cli/pkg.rs` (`pkg_cmd`, decision 4), `src/cli/mod.rs`
     (dispatch, top-level help line), `src/lib.rs`.
   - First check how `FILE_ONLY` / `--file` (`project.rs:312`) loads an entry file. The tool entry
     must load with the **package** as the project, not the file alone, so `use <pkgname>.x;` works.
     If no current path does that, add `Loader` support for "project X, entry file Y" (small: the
     project's deps plus itself under its own name).
   - Test (`cli/tests.rs`): fixture package `tests/programs/tool_pkg/`. Its `tools/echo.j3` prints
     its args, `J3_PROJECT_ROOT`, `J3_TOOL_API`, and the `[tool_pkg] greeting` value read from
     `J3_MANIFEST` with `j3.data.json`, then exits 3. A fixture game depends on it by path.
     `j3c pkg tool_pkg echo a b` prints all of that and returns 3. The second run does not rebuild
     (check the cache exe's mtime). Editing the tool rebuilds it. `j3c pkg` lists `echo`.
     `j3c pkg nope` is the named error.

4. [x] **Dist output hooks.**
   - Files: `src/project.rs` (`Output::Pkg(String)`; the error at `:475` becomes decision 5's),
     `src/dist.rs` (stage folders for package outputs, call `pkgtool::run` per output, collect the
     printed folder).
   - Test: the fixture package gets `[dist_outputs] fake = "tools/dist.j3"`, which writes
     `<dist>/fake/args.txt` with its args and prints that folder. `j3c build --release` of the
     fixture game with `build_outputs = ["zip", "fake"]` writes both. An exit of 4 fails the build
     with 4. `build_outputs = ["nope"]` is the named error.

5. [x] **Docs for tools** (before any Steam removal, so the package plan can point at them).
   - `src/doc/pages/packages.md`: `[commands]`, `[dist_outputs]`, the runner's env, the manifest
     JSON, the cache, and the trust note ("tools run as you, only when you run `j3c pkg` or name the
     output; never on fetch").
   - Also: `src/helptext/manifest.md`, `j3c new --lib` template comments (commented-out
     `[commands]` example), `editors/vscode/toml.js` (the two new tables), `docs/src/` (book
     packages page).
   - Tell plan 06 (edit its decision 4 and chunk 2) that link/run scripts go through `pkgtool::run`.

6. [x] **Remove Steam from `j3c`.** (done: no stub file; a loader check in `imports_of` gives the moved message for any `use j3.steam[.x]`, ponytail-marked for removal) Only once plan 10's package passes its own tests and
   `j3c pkg steam check` matches `j3c -e steam-check` output on the same SDK (or the fake SDK the
   Rust tests build).
   - Delete:
     - `j3c/stdlib/j3/steam.j3`, `j3c/stdlib/j3/steam/` and their `src/stdlib/files.rs` rows
     - `stdlib/tests/steam_*_test.j3` and the `steam_*` tests in `compile/tests/stdlib_cmp.rs`
     - `src/steamcheck.rs`, `src/steamcheck/` and the `-e steam-check` dispatch and help lines
     - `src/project/steam.rs` (and `Manifest.steam`, its `project/tests.rs` cases)
     - `src/dist/steam.rs` and `dist/steam/tests.rs` (`Output::Steam`, `steam_dev_env` and its
       four call sites)
     - the matching `j3c-bootstrap/src/` stubs
   - Keep a stub: `j3c/stdlib/j3/steam.j3` with only a module doc and a top-level compile error
     naming the fix (`j3.steam moved to the "steam" package: [dependencies] steam = { git = ..., tag = ... }; j3c doc steam`).
     If the checker has no "error on import" attribute, a doc-only module whose one public item is
     missing does the same job (every use errors on an unknown name). Pick whichever gives the
     clearer message, and test it. Remove the stub after one release.
   - `j3c -e steam-check` stays one release as a line pointing at `j3c pkg steam check`.
   - The `.gitignore` steam entries: check what they cover first.
   - Test: `cargo test`; `j3c check` of a program with `use j3.steam;` prints the moved message.

7. [x] **Docs after removal.** (done; `./build.sh` left to the orchestrator)
   - Keep a short `j3c doc steam` page: add the dep, `j3c pkg steam` for its commands, its README
     for the API. `doc/mod.rs` keeps the `steam` / `steamworks` search words, pointing there.
   - Move `helptext/recipes/steam.md` and `steam_main.j3` to the package; the recipe list loses
     `steam`.
   - Rewrite the Steam rows in `AGENTS.md` (root) and `j3c/AGENTS.md` (the rows at lines 92, 127,
     128, 129).
   - Update `doc/pages/ship.md`, `helptext/guide.md`, `helptext/manifest.md` (`[steam]` keys now
     belong to the package), `docs/src/compiler/steam.md` and `shipping.md`, `docs/src/stdlib.md`,
     `docs/src/SUMMARY.md`, `README.md`, `THIRD_PARTY.md`, `CHANGELOG.md` (breaking change entry).
   - Reword the comments that name `j3.steam` / `[steam]` in `ui/inputs.j3`, `ui/padin.j3`,
     `ui/dom/sim.j3`, `data/save.j3`, `game/addons.j3`, `net/tls.j3` and `doc/pages/j3ui.md`. The
     hooks themselves stay: they're general (on-screen keyboard, pad input, an exe-folder lib).
   - Plan 01 moves to the package (it's about verifying the bindings). Delete it here, add a line
     to `plans/README.md`.
   - Finish: `cargo test`, `j3c doc search steam` lands on the page, then `./build.sh`.

## Risks

- **Tool builds need the host toolchain.** The same one `j3c run` needs, so no new requirement.
- **Contract drift.** The env names, flags and manifest JSON are now API. `J3_TOOL_API` and
  `packages.md` are the contract; a change bumps the number.
- **`--stage` folders** double the disk use of a dist build when zip is also on. Fine.
