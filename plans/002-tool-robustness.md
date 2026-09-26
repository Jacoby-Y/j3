# Plan 002: Agent tool robustness

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has a lot of uncommitted and untracked
> work on top of `b086a5b` (the editor, `scene.rs`, `mcp.rs`, `gen.rs`,
> `examples/editor_village/` are all untracked or modified). A SHA check tells
> you nothing. Instead, open each file in "Current state" and confirm the
> quoted excerpts still match (line numbers may move by a few lines). On a
> real mismatch (the code is different, not just shifted), STOP.

## Status

- **Priority**: P1
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: bug
- **Planned at**: uncommitted tree on `b086a5b`, 2026-09-25

## Why this matters

Six rough edges from building `examples/editor_village` through MCP
(EDITOR.md "MCP Fixes" #1, #9, #10, #15, #16, #17). Each cost the agent a
detour or a wrong picture:

- **#1** After `cargo build`, the long-running `j3c -e mcp` server answers
  every scene/render call with "No such file or directory".
- **#9** `check` warns "no camera" on every house part-scene.
- **#10** The iso overview render of the village is a pale tile in fog.
- **#15** Changing the main scene in `j3.editor.toml` needs a hand edit of
  the generated `src/behaviors.j3`.
- **#16** `~/.cache/j3/editor` holds ~87 builds (196 MB).
- **#17** MCP `play` only stops and screenshots `j3.app` games.

Root causes found while planning (read these before touching code; two of
them differ from what EDITOR.md guessed):

- **#1 is not `cleanup_frontends`.** Cleanup only deletes builds older than
  14 days, so it never touched the server's build. `j3c/.cargo/copy-j3c.sh`
  installs every build with `mv -f "$dest.new" "$dest"`, which unlinks the old
  inode. The running server's `std::env::current_exe()` then reads
  `/proc/self/exe` as `/home/jacoby/.bin/j3c (deleted)`, and
  `editor::frontend` fails at `fs::metadata(host)` with exactly
  `No such file or directory (os error 2)`. Every tool that calls
  `current_exe()` breaks the same way.
- **#16 is the 14-day age gate.** The frontend cache key hashes the j3c
  binary's size and mtime, so every `cargo build` makes a new key. On this
  machine: 40 builds dated 2026-09-23, 45 dated 2026-09-24, all younger than
  14 days, so none are eligible. Plus two orphaned `*.tmp-<pid>-0` dirs from
  builds that were killed before their cleanup closure ran (cleanup skips any
  name that is not 16 hex chars).
- **#10 is fog, not the ground plane.** `frames` in `render.j3` already
  skips `builtin:plane`. The village's 1032 objects span x,z ±120 m, so
  `framed` puts the orbit ~375 m out, past `scene.env.fogEnd f=260.0`, and
  every pixel is fully fogged.

## Current state

### #1 stale host binary

`j3c/src/editor.rs:910-929` (`frontend`):

```rust
pub(crate) fn frontend(host: &Path) -> Result<(PathBuf, fs::File), String> {
    ...
    let mut hasher = DefaultHasher::new();
    sources.hash(&mut hasher);
    let meta = fs::metadata(host).map_err(|e| e.to_string())?;
    (meta.len(), meta.modified().ok()).hash(&mut hasher);
```

Every caller gets `host` from `current_exe()`:

```
j3c/src/cli.rs:608:    if let Some(meta) = std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok()) {
j3c/src/mcp.rs:529:        let host = std::env::current_exe().map_err(|e| e.to_string())?;
j3c/src/mcp.rs:583:    let host = std::env::current_exe().map_err(|e| e.to_string())?;
j3c/src/scene.rs:166:    let host = std::env::current_exe().map_err(|e| e.to_string())?;
j3c/src/scene.rs:282:    let host = std::env::current_exe().map_err(|e| e.to_string())?;
j3c/src/editor.rs:1238:    let host = std::env::current_exe().map_err(|e| e.to_string())?;
j3c/src/gen.rs:94:    let host = match std::env::current_exe() {
```

`j3c/.cargo/copy-j3c.sh` (last two lines):

```sh
install -D -m 755 "$out" "$dest.new"
mv -f "$dest.new" "$dest"
```

### #16 cache cleanup

`j3c/src/editor.rs:1034-1064` (`cleanup_frontends`), the part that matters:

```rust
    for (position, (modified, path)) in builds.into_iter().enumerate() {
        if position < 3 || path == current { continue; }
        let Ok(age) = now.duration_since(modified) else { continue };
        if age < std::time::Duration::from_secs(14 * 24 * 60 * 60) { continue; }
        let Ok(lock) = cache_lock(&path) else { continue };
        if lock.try_lock().is_err() { continue; }
        // Recheck after locking: another process may have replaced this build.
        let old = fs::metadata(path.join("target/j3_editor"))
        ...
        if old.is_some_and(|age| age >= std::time::Duration::from_secs(14 * 24 * 60 * 60)) {
            let _ = fs::remove_dir_all(path);
        }
```

Builds are filtered to names of exactly 16 hex digits, so
`<key>.tmp-<pid>-<n>` dirs (created at `editor.rs:941`) are never removed if
the build process dies. `frontend` calls `cleanup_frontends(&cache, &dir)`
on every call (`editor.rs:994`), after taking a shared lock on the build it
returns. The existing test is `cache_cleanup_keeps_recent_and_in_use_builds`
at `editor.rs:1427` (builds aged 15–18 days).

### #9 "no camera" on part scenes

`j3c/editor/src/tool.j3:988` and `:1057`:

```j3
fn check(scene: &Scene, catalog: &Save, path: string, out: &Text) {
    let cameras = 0;
    ...
    if cameras == 0 { issue(out, false, 0, "the scene has no camera; the game falls back to a default view"); }
}
```

Dispatch at `tool.j3:69`: `else if same(op, "check") { check(scene, catalog, path, out); }`.
Missing-file guard at `tool.j3:1484`:
`if !scene.found && !same(request, "check") && !(request.len > 3 && same(cut(request, 0, 4), "add\t")) {`

Rust callers of the check op: `mcp.rs:510` (`scene::run_tool(root, rel, &["check".into()])`)
and `scene.rs` `execute` (`j3c -e scene check [--all]`), both via
`scene::run_tool` at `scene.rs:273`. Instances are stored as lines like
`node.10.scene s=scenes/house_a.j3scene`. The main scene helper is private:
`editor.rs:673 fn is_main_scene(root: &Path, rel: &str) -> bool`.

### #10 overview render fog

`j3c/editor/src/render.j3:69` `pub struct Lens { cam, fov, near, far, label }`.
`lensFor` (line 173) returns early for a camera shot; the view/focus path ends
with `lens.cam = framed(scene, root, view);` (~line 215). `drawShot` line 240:
`gfx.setFog(env.fogStart, env.fogEnd);`. Lens literals:
`render.j3:298` and `live.j3:177`
(`Lens { cam: ..., fov: 60, near: 0.05, far: 1000, label: "" }`).
The lit shader computes `fog = clamp((dist - uFogStart) / max(uFogEnd - uFogStart, 0.001), 0.0, 1.0)`,
so a huge start/end means no fog.

### #15 generated dispatch goes stale

`j3c/src/editor.rs:767 generate_dispatch(root, package)` writes `src/behaviors.j3`
(including `pub const MAIN_SCENE`) and already returns early without writing
when the text is unchanged (`editor.rs:810`). It is only called from the
editor (`execute`, `create_behavior`, `rescan`). It is a no-op for projects
with no behaviors that don't `use <pkg>.behaviors`, and errors if a
non-generated `src/behaviors.j3` exists.

Every build/run/check compiles a project through one function,
`j3c/src/cli.rs:291`:

```rust
fn compile_input(input: &Input) -> Result<(compile::Program, Option<Manifest>), String> {
    match input {
        Input::Hello => Ok((compile::compile_path(&compile::default_hello_path())?, None)),
        Input::File(p) => Ok((compile::compile_path(p)?, None)),
        Input::Project(m) => Ok((compile::compile_manifest(m)?, Some(m.clone()))),
    }
}
```

(`check_cmd` → `compile_user` → `compile_entry` → `compile_input`. MCP
`play` shells out to `j3c build`, MCP `check` to `j3c check --json`.)

### #17 frame/screenshot hooks live in App.run

`j3c/stdlib/j3/app.j3:5-8` (libc `getenv`/`strlen`), `:34 fn env`, `:41 fn screenshot`,
and in `run` (`app.j3:351-425`):

```j3
        // `J3_FRAMES`: stop after that many frames; `J3_SCREENSHOT`: save the last one first.
        let stopAt = 0;
        let frames = env("J3_FRAMES");
        ...
        let shotPath = env("J3_SCREENSHOT");
        if shotPath.len > 0 && stopAt == 0 { stopAt = 60; }
        let drawn = 0;
        while true {
            ...
            drawn += 1;
            if stopAt > 0 && drawn >= stopAt {
                if shotPath.len > 0 { screenshot(shotPath); }
                break;
            }
            game.present();
        }
```

`j3c/stdlib/j3/game.j3:3400`:

```j3
pub fn present() {
    if platform.gl {
        sdl.SDL_GL_SwapWindow(platform.window);
    } else {
        sdl.SDL_RenderPresent(platform.renderer);
    }
}
```

`game.j3` already has `use j3.gfx.canvas.{ Canvas, Mode };` (line 6),
`pub fn readWindow(out: []u8): bool` (line 3362), `pub fn requestQuit()`
(line 1597). It has no `use lib "c"` block. `mcp.rs:636` says
"Only j3.app games stop after `frames`."

## Commands you will need

Run from `j3c/`. Note `cargo build`/`cargo test` replace `~/.bin/j3c`
through `.cargo/copy-j3c.sh`; that is expected. Never launch the editor or a
game window. Wrap anything that runs a game or renderer in `timeout 60`.

| Purpose | Command | Expected |
|---------|---------|----------|
| The plan's check | `cargo test robust_ -- --test-threads=1` | all `robust_*` tests pass |
| Editor module tests | `cargo test editor:: -- --test-threads=1` | pass |
| Scene/MCP tests | `cargo test scene:: mcp:: -- --test-threads=1` | pass |
| Full suite | `cargo test -- --test-threads=1` | exit 0 |
| Overview render (optional, headless) | `timeout 60 j3c -e render --project ../examples/editor_village --view iso --out target/j3-editor/iso.png` | `ok` line; open the PNG with the Read tool |
| Part-scene check | `timeout 120 j3c -e scene --project ../examples/editor_village check --all` | no "no camera" line for `scenes/house_*.j3scene` |

## Scope

**In scope**:
- `j3c/src/editor.rs` (`host_exe` helper, `frontend` callers, `cleanup_frontends`, `is_main_scene` visibility, tests)
- `j3c/src/scene.rs` (`run_tool` host + part flag, `placed_elsewhere`)
- `j3c/src/mcp.rs` (host helper at 529/583, restart hint, `play` verdict text)
- `j3c/src/gen.rs:94`, `j3c/src/cli.rs:291` and `:608`
- `j3c/editor/src/tool.j3` (check `part` flag), `render.j3` + `live.j3` (Lens `fog`)
- `j3c/stdlib/j3/game.j3` (`present` hooks), `j3c/stdlib/j3/app.j3` (remove its copy)

**Out of scope**:
- Holding `in-use.lock` for the MCP server lifetime (EDITOR.md's idea). Not
  needed once the host path is fixed: every call re-locks, and a deleted build
  just rebuilds on the next call.
- "Retry once" in `run_tool`. Same reason.
- Percentile framing / ignoring huge imported flat meshes in `sceneBounds`.
  `builtin:plane` is already skipped; the village problem was fog.
- Moving the overview shadow box to the framed bounds (shadows beyond
  `shadowSize` of the origin stay missing in overview shots).
- `game_headless.j3` (no window, nothing to screenshot).
- Other MCP fixes (#2–#8, #11–#14).

## Git workflow

The tree has uncommitted work you must keep. Do NOT use a separate worktree
(it would not contain the untracked editor sources). In the main checkout:
`git switch -c plan-002-tool-robustness`. Stage only the files you changed
(`git add <paths>`), never `git add -A`. Don't commit to `main`. Don't push.
Message: `Make the agent tools survive rebuilds, and quiet false check/render/play problems.`

## Steps

### Step 1: One host-binary helper (#1)

In `j3c/src/editor.rs` add:

```rust
/// This j3c binary. After a rebuild replaces it on disk, Linux reports the old path with
/// " (deleted)"; `/proc/self/exe` still opens the running (old) binary, so a long-lived
/// `j3c -e mcp` keeps working with the toolchain it started with.
// ponytail: the server keeps using the old j3c until restarted; mcp.rs says so.
pub(crate) fn host_exe() -> Result<PathBuf, String> {
    Ok(host_or_self(std::env::current_exe().map_err(|e| e.to_string())?))
}

fn host_or_self(path: PathBuf) -> PathBuf {
    if path.exists() || !cfg!(target_os = "linux") { return path; }
    PathBuf::from("/proc/self/exe")
}
```

Replace every `std::env::current_exe()` listed in Current state with it:
`mcp.rs:529`, `mcp.rs:583`, `scene.rs:166`, `scene.rs:282`, `editor.rs:1238`,
`gen.rs:94` (keep its error-to-report shape), and `cli.rs:608`
(`crate::editor::host_exe().ok().and_then(|p| std::fs::metadata(p).ok())`).
`fs::metadata("/proc/self/exe")` follows the link to the still-open inode, so
`frontend`'s hash is unchanged and the existing build is reused.

In `mcp.rs` `Server::handle`, `"tools/call"` arm (~line 182, right after
`let answer = match self.call(name, &args) { ... };`, made `let mut`), push one extra
`text_block` onto `answer.content` when
`std::env::current_exe().map(|p| !p.exists()).unwrap_or(false)`:
`"note: j3c was rebuilt since this server started; restart the MCP server to use the new one."`
Keep it to that one `if`.

Add test `robust_host_falls_back_to_running_binary` in `editor.rs` tests:
`host_or_self(PathBuf::from("/nonexistent/j3c (deleted)"))` equals
`/proc/self/exe` on Linux, `fs::metadata` of the result is `Ok`, and
`host_or_self(std::env::current_exe().unwrap())` returns the input unchanged.
Gate with `#[cfg(target_os = "linux")]`.

**Verify**: `cargo test robust_host -- --test-threads=1` passes;
`rg -n "current_exe\(\)" src` shows only `host_exe` itself, its test, and the
one-line restart hint.

### Step 2: Cache cleanup by count, plus stale temp dirs (#16)

In `cleanup_frontends`:

1. Drop both 14-day age checks. A build is removed when it is not among the 3
   newest, is not `current`, and `try_lock` (exclusive) succeeds. Keep the
   lock-then-recheck shape: after locking, recheck that `target/j3_editor`
   still exists before `remove_dir_all` (a concurrent `frontend` that found
   the exe missing already rebuilds, see `editor.rs:988-992`).
2. Also remove directories whose name contains `.tmp-` and whose own mtime is
   more than one hour old (`fs::metadata(path).modified()`). No lock; a live
   build finishes in minutes.

Rewrite `cache_cleanup_keeps_recent_and_in_use_builds` as
`robust_cache_cleanup_keeps_newest_and_locked`: five builds with exe mtimes
1–5 minutes ago (names `...01`–`...05`, newest = `01`), shared-lock `05`,
call with `current = 04`. Expect all five still present (`01`–`03` newest,
`04` current, `05` locked). Drop the lock and call with `current = 01`:
`04` and `05` removed, `01 02 03` kept. Add a
`<16hex>.tmp-1-0` dir with mtime 2 h ago (set it with
`fs::File::open(dir)?.set_times(...)`) and one fresh `.tmp-` dir; the old
one is removed, the fresh one kept.

**Verify**: `cargo test robust_cache -- --test-threads=1` passes.

### Step 3: "no camera" only where it matters (#9)

1. `editor.rs`: make `is_main_scene` `pub(crate)`.
2. `scene.rs`: add

   ```rust
   /// A scene other scenes place and the game never starts in: it needs no camera.
   pub(crate) fn placed_elsewhere(root: &Path, rel: &str) -> bool {
       if crate::editor::is_main_scene(root, rel) { return false; }
       let needle = format!(".scene s={rel}");
       all_scenes(root).unwrap_or_default().iter()
           .filter(|other| other.as_str() != rel)
           .any(|other| fs::read_to_string(root.join(other))
               .is_ok_and(|t| t.lines().any(|l| l.ends_with(&needle))))
   }
   ```

   `is_main_scene` errors to `false` when there is no `j3.editor.toml`; that is fine.
3. `run_tool`: when `args == ["check"]` and `placed_elsewhere(root, rel)`,
   send `check\tpart` instead. Both Rust callers get it for free.
4. `tool.j3`: dispatch `check(scene, catalog, path, args.len > 1 && same(args[1], "part"), out)`;
   add the `part: bool` parameter; guard the last line with `&& !part`. At
   line 1484 replace `!same(request, "check")` with a prefix test so
   `check\tpart` is still treated as a check:
   `!(request.len >= 5 && same(cut(request, 0, 5), "check"))`.

Add test `robust_part_scene_is_placed_elsewhere` in `scene.rs` tests: temp
project with `j3.editor.toml` (`[editor]\nscene = "scenes/main.j3scene"\n`),
`scenes/main.j3scene` containing `node.0.scene s=scenes/house.j3scene`,
`scenes/house.j3scene`, and `scenes/lonely.j3scene`. Expect `house` true,
`main` false, `lonely` false.

**Verify**: `cargo test robust_part -- --test-threads=1` passes. Then
`timeout 120 j3c -e scene --project ../examples/editor_village check --all`
prints no "no camera" line under any `scenes/house_*` header.

### Step 4: No fog in overview renders (#10)

In `render.j3`:

1. Add `fog: bool;` to `Lens` (doc: "false for framed views: fog would hide a scene framed from far out").
2. `lensFor`: set `lens.fog = true;` at the top; set `lens.fog = false;` right before `lens.cam = framed(...)`.
3. `drawShot`: replace line 240 with
   `if lens.fog { gfx.setFog(env.fogStart, env.fogEnd); } else { gfx.setFog(100000, 100001); }`.
4. Add `fog: true` to both `Lens { ... }` literals (`render.j3:298`, `live.j3:177`).

**Verify**: `J3_EDITOR_SRC` is not needed; the frontend is embedded, so run
`cargo build` first. Then optionally the "Overview render" command from the
table and look at the PNG: houses and trees are visible with color, not a
pale square. One run only; if it still looks washed out, report instead of
iterating.

### Step 5: Keep `behaviors.j3` current on every build (#15)

In `cli.rs` `compile_input`, before `compile::compile_manifest(m)`:

```rust
Input::Project(m) => {
    // The editor's generated dispatch follows j3.editor.toml (main scene, behaviors).
    if m.root.join("j3.editor.toml").is_file() {
        if let Err(e) = crate::editor::generate_dispatch(&m.root, &m.name) {
            eprintln!("warning: src/behaviors.j3: {e}");
        }
    }
    Ok((compile::compile_manifest(m)?, Some(m.clone())))
}
```

`generate_dispatch` already skips the write when nothing changed, returns
`Ok` for projects without behaviors, and refuses to overwrite a hand-written
file (that becomes the warning). Make it `pub(crate)` if it isn't.

Add test `robust_dispatch_follows_editor_toml` in `editor.rs` tests: temp
project (`j3.toml` with `name = "t"`, `src/main.j3` containing
`use t.behaviors;`, `j3.editor.toml` with scene `scenes/a.j3scene`), call
`generate_dispatch`, assert the file contains `"scenes/a.j3scene"`; rewrite
the toml to `scenes/b.j3scene`, call again, assert `"scenes/b.j3scene"`.

**Verify**: `cargo test robust_dispatch -- --test-threads=1` passes.

### Step 6: Frame limit and screenshot in `game.present` (#17)

1. In `game.j3` add a libc block like `app.j3:5-8` (`getenv`, `strlen`),
   a private `env(name)` like `app.j3:34`, and move `screenshot(path)` from
   `app.j3:41-61` verbatim (it needs `Canvas`, `Mode`, `Result`, `printLn`;
   add the imports `game.j3` lacks).
2. Add two zero-initialized singletons next to the other `#[singleton]`s:
   `PRESENTED: int = 0` and `STOP_AT: int = 0`.
3. Change `present`:

   ```j3
   pub fn present() {
       if PRESENTED == 0 { STOP_AT = framesWanted(); }
       PRESENTED += 1;
       // `J3_FRAMES`: quit after that many frames; `J3_SCREENSHOT`: save the last one first.
       let last = STOP_AT > 0 && PRESENTED >= STOP_AT;
       if last { let shot = env("J3_SCREENSHOT"); if shot.len > 0 { screenshot(shot); } }
       (existing swap / RenderPresent)
       if last { requestQuit(); }
   }
   ```

   `framesWanted()` parses `J3_FRAMES` digits as `app.j3:353-358` does and
   returns 60 when it is 0 but `J3_SCREENSHOT` is set (same as App today).
   The screenshot is taken before the swap, like App did.
4. In `app.j3 run`, delete the `stopAt`/`shotPath`/`drawn` block and the
   `if stopAt > 0 ...` break; always call `game.present()`. The loop already
   breaks on `game.quitRequested()` after the next `poll`. Delete `env`,
   `screenshot`, the libc block, and any imports now unused. Update the doc
   comment at `app.j3:32` to say `game.present` handles it.
5. `mcp.rs:636`: change the hint to
   `"Does the loop call game.present and stop on game.quitRequested()?"`.

**Verify**: `cargo test -- --test-threads=1` passes (App-based tests such as
`game_loop_j3_runs` still run). Optionally, one headless run of a non-App
program is enough: `timeout 60 j3c run --frames 5 --screenshot /tmp/j3shot.png <a game that calls game.present in its own loop>`
exits by itself and writes the PNG. Don't loop on it.

## Test plan

- `cargo test robust_ -- --test-threads=1` is the plan's single check; it runs
  the four new/rewritten Rust tests (host fallback, cache cleanup, part scene,
  dispatch refresh).
- `#10` and `#17` have no unit test (they need GL/SDL); the optional one-shot
  commands above cover them. Existing App tests guard #17's App path.

## Done criteria

- [ ] `cargo test robust_ -- --test-threads=1` passes (4 tests)
- [ ] `cargo test -- --test-threads=1` exits 0
- [ ] `rg -n "current_exe\(\)" j3c/src` only in `host_exe`, its test, and the mcp restart hint
- [ ] `rg -n "14 \* 24" j3c/src/editor.rs` finds nothing
- [ ] `rg -n "J3_FRAMES|J3_SCREENSHOT" j3c/stdlib/j3/app.j3` finds nothing; `game.j3` has them
- [ ] `j3c -e scene check --all` on the village shows no "no camera" for house scenes
- [ ] Only in-scope files changed; `plans/README.md` untouched

## STOP conditions

- `current_exe()` on this machine does not report a missing path after a
  rebuild (e.g. the install script changed to copy in place). Report; the
  fallback is then moot and #1 has another cause.
- `generate_dispatch` errors on `examples/*` projects that have a hand-written
  `src/behaviors.j3` and the warning would fire on every build. Report the
  project instead of silencing it.
- Moving the screenshot into `game.j3` creates an import cycle error. Report
  the cycle; do not restructure modules.
- Any existing test that asserted the 14-day behavior besides the one you
  rewrite.

## Maintenance notes

- New code that needs "the j3c binary" must call `editor::host_exe()`, not
  `current_exe()`.
- The frontend cache now keeps 3 builds plus any in use. If people run
  several j3c versions side by side, raise the count, not an age gate.
- A game that never calls `game.present` (pure headless) is not stopped by
  `J3_FRAMES`; `play`'s timeout still kills it.
- `part` is a Rust-side decision; the editor UI's own checks (if it ever
  calls `check` live) still warn on part scenes.
