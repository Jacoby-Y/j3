# Plan 005: First-person starter kit

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has a lot of uncommitted and untracked
> work on top of `b086a5b` (`scene3d.j3`, the editor, and
> `examples/editor_village/` are untracked). A SHA check tells you nothing.
> Open each file in "Current state" and confirm the quoted excerpts still
> match (line numbers may shift a little). On a real mismatch, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none (touches `editor.rs` near plan 002's edits, different functions)
- **Category**: feature
- **Planned at**: uncommitted tree on `b086a5b`, 2026-09-25

## Why this matters

EDITOR.md "MCP Fixes" #12: the editor's game template is a fixed camera. To
make the village walkable the agent wrote a first-person controller with
colliders (`examples/editor_village/src/player.j3`, 204 lines) and a
player-following draw (`main.j3` `draw`) from scratch. Every 3D scene-editor
game wants both. Move them into the stdlib, use them in a
`j3c new NAME --template fps` project, and shrink the village onto them.

Deliberately skipped: a stdlib settings panel (the village's
`settings.j3` is game-specific: presets, sun sliders). Add one when a second
game copies it.

## Current state

### The walker to generalize

`examples/editor_village/src/player.j3`:

- `struct Box { toLocal: Mat4; toWorld: Mat4; lo: Vec3; hi: Vec3; scale: f32; }` (line 21)
- `pub struct Player { pos: Vec3; yaw: f32; pitch: f32; fall: f32; boxes: []Box; count: int; }` (line 29)
- `pub fn spawn(scene: &Scene, mem: &Arena): Player` (line 40): starts at the
  first top-level camera (`n.kind == CAMERA && n.owner == 0`), feet at y 0,
  `yaw = atan2(forward.x, 0 - forward.z)`, `pitch = asin(forward.y)`, then `colliders`.
- `fn soft(path)` (line 61): village-specific walk-through list
  (`grass`, `flower`, `crops_`, `plant_bush`, `campfire`, `mushroom`).
- `fn colliders` (line 69): one box per top-level `INSTANCE` around its owned
  `MESH` nodes with `m.gpu.vao != 0` (bounds exist only after `upload`);
  trees (`has(n.scene, "tree")`) shrink to 20% width. Only instances collide;
  plain top-level meshes don't.
- `pub fn update(p, dt, looking, sensitivity, speed)` (line 129): mouse look
  (`game.mouseDelta() * 0.0025 * sensitivity`, pitch clamped ±1.45), WASD,
  Shift doubles speed, Space jumps when `pos.y <= 0`, `GRAVITY 22`,
  `JUMP 7.5`, floor at y 0, then `pushOut`.
- `fn pushOut` (line 163): sideways push out of each box in the box's local
  space, radius scaled by `1 / b.scale`.
- `pub fn look(p): Look` (line 202): `Look.create(pos + EYE up, yaw, pitch)`, `EYE 1.7`.

### The draw to generalize

`examples/editor_village/src/main.j3:56-73`:

```j3
fn draw(w: &World) {
    let s = &w.settings;
    let env = settings.environment(s, &w.scene);
    // ponytail: rewrites the scene's lighting lines every frame; only on change if it shows up in a profile.
    scene3d.setEnvironment(&w.scene, env);
    gfx.setFog(env.fogStart, env.fogEnd);
    gfx.setClip(0.1, env.fogEnd + 20);
    // The shadow box follows the player, so a smaller range means sharper shadows nearby.
    let r = s.shadowRange;
    let p = w.player.pos;
    gfx.shadowPass(env.sunDir, Vec3.xyz(p.x - r, 0 - r, p.z - r), Vec3.xyz(p.x + r, r, p.z + r));
    if env.shadows { scene3d.draw(&w.scene); }
    scene3d.lights(&w.scene);
    gfx.cameraPassLookFov(player.look(&w.player), env.sunDir, env.sky, env.sunColor, s.fov);
    scene3d.drawSky(&w.scene);
    scene3d.draw(&w.scene);
    settings.panel(s, w.font);
}
```

`settings.environment` (`settings.j3:96`) already copies `shadowRange` into
`env.shadowSize`, so `env` alone carries the shadow range.

`j3c/stdlib/j3/scene3d.j3`:

- `:1253 pub fn shadows(scene: &Scene, env: Env)` — fog + shadow pass, box centered on the origin.
- `:2809 pub fn draw(scene)` = `worlds` into `scratch(0)` + `drawWith`. Called twice per frame above, so world matrices are computed twice.
- `:2998 pub fn drawSky`, `:3013 pub fn drawWith(scene, transforms)`, `:1217 pub fn lights`, `:2252 pub fn isSky(n: Node)`.
- Imports already include `j3.gfx`, `Mat4`, `Vec3`, `scratch` (lines 8–15). `Look` is `j3.gfx.Look` (`gfx.j3:234`, field `eye`).
- `gfx.cameraPassLookFov(cam: Look, sun, sky, sunColor, degrees)` at `gfx.j3:1924`.

### The templates

`j3c/src/editor.rs:124 fn create_project(root: &Path) -> Result<(), String>`
(used by `j3c editor --new`) writes `j3.toml` (libs SDL2 + GL), `src/main.j3`
from `game_template(&name)` (`editor.rs:354`, fixed scene camera, no sky
draw), `src/behaviors/spin.j3`, `j3.editor.toml` with `[behavior.spin]`,
`scenes/main.j3scene` from `START_SCENE` (`editor.rs:167`, a camera + a
spinning `builtin:cube`), and `assets/gen/tree.j3`. It does NOT write
`src/behaviors.j3`; `execute` calls `generate_dispatch` afterwards
(`editor.rs:115`).

`j3c/src/scaffold.rs:7 pub fn cmd` parses `[name] [--app]`; `create(name, app)`
(`:58`) writes `j3.toml`, `src/main.j3`, `AGENTS.md`. `locate(name)` (`:141`)
returns a possibly relative root. Tests `app_project_typechecks` (`:344`) run
`crate::cli::run(["check".into(), path])` and expect 0.

### Stdlib registration and self-checks

`j3c/src/stdlib/mod.rs:6 const FILES` lists every stdlib file with
`include_str!`. `j3c/examples/scene3d_check.j3` prints `ok`;
`j3c/src/compile.rs:900 scene3d_check_j3_runs` compiles and runs it and
asserts stdout `ok\n`.

## Design

### `j3.scene3d.fpwalk` (new `j3c/stdlib/j3/scene3d/fpwalk.j3`)

A cleaned-up copy of the village `player.j3`, renamed `Walker`:

```j3
pub struct Walker { pos: Vec3; yaw: f32; pitch: f32; fall: f32; boxes: []Box; count: int; }
struct Box { toLocal: Mat4; toWorld: Mat4; lo: Vec3; hi: Vec3; scale: f32; path: string; }

pub fn spawn(scene: &Scene, mem: &Arena): Walker   // camera start + colliders (after upload)
pub fn empty(mem: &Arena, capacity: int): Walker    // at the origin, room for `capacity` boxes (spawn uses it)
pub fn addBox(w: &Walker, world: Mat4, lo: Vec3, hi: Vec3, scale: f32, path: string)
pub fn walkThrough(w: &Walker, part: string)        // drop boxes whose path contains `part`
pub fn trunks(w: &Walker, part: string, keep: f32)  // shrink those boxes' x/z to `keep` of their width
pub fn update(w: &Walker, dt: f32, looking: bool, sensitivity: f32, speed: f32)  // reads input, calls step
pub fn step(w: &Walker, mx: f32, mz: f32, speed: f32, jump: bool, dt: f32)       // no input: testable
pub fn look(w: &Walker): Look
```

Changes from the village code, and only these:

1. Colliders also cover top-level plain meshes: every node with
   `owner == 0`, `kind == MESH`, `gpu.vao != 0`, not `isSky`, and asset not
   `builtin:plane` gets a box of its own `boundLo/boundHi` in its world
   space (`path` = its asset). Instances keep the existing loop (`path` = `n.scene`).
   Without this, a starter scene of plain cubes has no collision.
2. `soft()` and the tree rule leave the module; the game calls
   `walkThrough` / `trunks` after `spawn`. Strings, not callbacks, so a game
   is three plain lines.
3. `spawn` allocates `boxes` for `scene.len` entries (it already does); `addBox`
   ignores calls past capacity. A zero-capacity walker (`boxes` len 0)
   is what `spawn` returns before `colliders` run.
4. `update` builds the world-space wish direction from WASD and yaw exactly as
   today, then calls `step(w, mx, mz, speed * (Shift ? 2 : 1), looking && Space pressed, dt)`.
   `step` normalizes, moves, jumps if `pos.y <= 0`, applies gravity, clamps
   to the floor, and calls `pushOut`.

Keep: `EYE 1.7`, `RADIUS 0.35`, `GRAVITY 22`, `JUMP 7.5`, the 0.0025 mouse
factor, ±1.45 pitch clamp, the private `has/min/max` helpers (`j3.text` has
no `contains` yet; MCP fix #13 will add it). Carry the existing
`// ponytail: one box per object ...` note and add
`// ponytail: flat floor at y = 0 and static boxes; stand-on-top and moving colliders when a game needs them.`

### `scene3d.drawFrame`

In `scene3d.j3`, next to `shadows`:

```j3
/// One frame of `scene` seen from `look`: fog, a sun shadow box of half size
/// `env.shadowSize` around the eye (so a smaller box is sharper nearby), lights, sky,
/// and the scene (URP per the scene's settings). World matrices are computed once.
// ponytail: clip is 0.1 to fogEnd + 20, like the village; take near/far when a game needs them.
pub fn drawFrame(scene: &Scene, look: gfx.Look, env: Env, fov: f32) {
    gfx.setFog(env.fogStart, env.fogEnd);
    gfx.setClip(0.1, env.fogEnd + 20);
    let r = env.shadowSize;
    let e = look.eye;
    gfx.shadowPass(env.sunDir, Vec3.xyz(e.x - r, 0 - r, e.z - r), Vec3.xyz(e.x + r, r, e.z + r));
    let transforms = scratch(0).alloc::<Mat4>(scene.len);
    worlds(scene, transforms);
    if env.shadows { drawWith(scene, transforms); }
    lights(scene);
    gfx.cameraPassLookFov(look, env.sunDir, env.sky, env.sunColor, fov);
    drawSky(scene);
    drawWith(scene, transforms);
}
```

Use whatever spelling `scene3d.j3` already uses for `Look` (add
`use j3.gfx.Look;` or write `gfx.Look` — match how the file names other gfx types).

### `j3c new NAME --template fps`

`scaffold.rs` accepts `--template fps` (and only `fps`; anything else is an
error listing `fps`). It resolves the root to an absolute path, then calls
`crate::editor::create_fps_project(&root)`, then writes `AGENTS.md` as `create`
does. `create_fps_project` in `editor.rs` is `create_project` with the fps
main and scene, followed by `generate_dispatch(root, &name)` so the project
compiles without opening the editor. Implement it by giving
`create_project` a `fps: bool` parameter (existing caller passes `false`) and
picking `fps_template(&name)` / `FPS_SCENE` when true; do not copy the function.

`fps_template(name)` (keep it this small):

```j3
module {name};

use {name}.behaviors;
use j3.app.App;
use j3.scene3d.fpwalk;
use j3.scene3d.fpwalk.Walker;
use j3.game;
use j3.game.Key;
use j3.gfx;
use j3.memory.Arena;
use j3.scene3d;
use j3.scene3d.Scene;

struct World {
    scene: Scene;
    walker: Walker;
    mem: &Arena;
}

fn boot(mem: &Arena): World {
    let scene = scene3d.load(behaviors.scene(), mem);
    return World { scene: scene, walker: fpwalk.spawn(&scene, mem), mem: mem };
}

fn startup(w: &World) {
    gfx.init();
    scene3d.upload(&w.scene);
    // Colliders need mesh bounds, which upload fills in.
    w.walker = fpwalk.spawn(&w.scene, w.mem);
    game.captureMouse(true);
    let i = 0;
    while i < w.scene.len {
        behaviors.start(&w.scene, w.scene.nodes[i].id);
        i += 1;
    }
}

fn update(w: &World, dt: f32) {
    if game.keyPressed(Key.Escape) { game.requestQuit(); }
    fpwalk.update(&w.walker, dt, true, 1, 4.5);
    let i = 0;
    while i < w.scene.len {
        behaviors.update(&w.scene, w.scene.nodes[i].id, dt);
        i += 1;
    }
}

fn draw(w: &World) {
    scene3d.drawFrame(&w.scene, fpwalk.look(&w.walker), scene3d.environment(&w.scene), 70);
}

fn main() {
    let app = App<World>.create("{name}", 960, 540, boot);
    app.useGl();
    app.startup(startup);
    app.always(update);
    app.draw(draw);
    app.run();
}
```

(Braces doubled inside `format!` as in `game_template`.)

`FPS_SCENE`, same line format as `START_SCENE`: `scene.count=5`,
`scene.nextId=6`; nodes:
1. `Camera` kind 2 at `0, 0, 8` (rotation 0; the walker adds eye height), fov 70, near 0.1, far 200.
2. `Ground` kind 1, `builtin:plane`, scale `40 1 40`.
3. `Crate` kind 1, `builtin:cube`, at `3 0.5 0`.
4. `Wall` kind 1, `builtin:cube`, at `0 1 -6`, scale `8 2 0.5`.
5. `Spinner` kind 1, `builtin:cube`, at `-3 0.5 0`, behavior `spin`.

Every node line set that `START_SCENE` writes (`id parent kind name asset
texture behavior px..sz`, plus `fov near far` on the camera) must be present.

## Commands you will need

From `j3c/`. `cargo build`/`test` also replace `~/.bin/j3c` (wrapper); expected.
Never launch a game window or the editor.

| Purpose | Command | Expected |
|---------|---------|----------|
| The plan's check | `cargo test fpwalk -- --test-threads=1` | `fpwalk_check_j3_runs` and `fpwalk_template_typechecks` pass |
| Village still compiles | `j3c check ../examples/editor_village` | exit 0 |
| Docs render | `j3c doc j3.scene3d.fpwalk` | lists `Walker`, `spawn`, `update`, `step`, `look`, `walkThrough`, `trunks`, `addBox` |
| Full suite | `cargo test -- --test-threads=1` | exit 0 |

## Scope

**In scope**:
- `j3c/stdlib/j3/scene3d/fpwalk.j3` (new), `j3c/src/stdlib/mod.rs` (register it)
- `j3c/stdlib/j3/scene3d.j3` (`drawFrame` only)
- `j3c/examples/fpwalk_check.j3` (new), `j3c/src/compile.rs` (one test)
- `j3c/src/editor.rs` (`create_project(root, fps)`, `create_fps_project`, `fps_template`, `FPS_SCENE`)
- `j3c/src/scaffold.rs` (`--template fps`, help text, one test)
- `examples/editor_village/src/main.j3`, delete `examples/editor_village/src/player.j3`
- One line each in `AGENTS.md` (stdlib list) and `j3c/AGENTS.md` if it lists stdlib modules

**Out of scope**:
- A stdlib settings panel; the village keeps `settings.j3`.
- Changing `scene3d.shadows` or `game_template` (the editor's default template).
- Standing on boxes, slopes, terrain height, moving colliders.
- `j3c editor --new --fps` (the editor can open the scaffolded project).

## Git workflow

The tree has uncommitted work you must keep. Do NOT use a separate worktree
(it would lack the untracked `scene3d.j3` and village). In the main checkout:
`git switch -c plan-005-fps-starter-kit`. Stage only files you changed or
created (`git add <paths>`, `git rm` for `player.j3` only if it is tracked;
it is currently untracked, so just delete it). Never `git add -A`. Don't
commit to `main`. Don't push.
Message: `Add j3.scene3d.fpwalk, scene3d.drawFrame, and j3c new --template fps.`

## Steps

### Step 1: `j3.scene3d.fpwalk`

Create `j3c/stdlib/j3/scene3d/fpwalk.j3` per Design (module `j3.scene3d.fpwalk`, file doc
comment first like other stdlib files). Register
`("fpwalk.j3", include_str!("../../stdlib/j3/scene3d/fpwalk.j3"))` in
`src/stdlib/mod.rs` `FILES` (keep the list's order style).

**Verify**: `cargo build` succeeds; `j3c doc j3.scene3d.fpwalk` lists the pub items.

### Step 2: The self-check

Create `j3c/examples/fpwalk_check.j3` (module `fpwalk_check`, prints `ok`,
panics with a message on failure, pattern of `scene3d_check.j3`). No GL, no
window: build a walker without a scene.

1. `let w = fpwalk.empty(&mem, 4);` then `w.pos = Vec3.xyz(-3, 0, 0);`.
2. `fpwalk.addBox(&w, Mat4.identity(), Vec3.xyz(-1, 0, -1), Vec3.xyz(1, 3, 1), 1, "crate")`.
3. Walk +x for 2 s in 0.05 s steps with `step(&w, 1, 0, 4, false, 0.05)`.
   Assert `w.pos.x <= -1.3` (outside the box by about `RADIUS`) and `w.pos.x >= -1.4`.
4. `walkThrough(&w, "crate")`; walk again 1 s; assert `w.pos.x > 0` (passed through).
5. Jump: `step(&w, 0, 0, 4, true, 0.05)`; assert `w.pos.y > 0`. Then 40 more
   `step(..., false, 0.05)`; assert `w.pos.y == 0`.
6. A box under a scale-2 transform (`Mat4.scale(2, 2, 2)`, `math.j3:466`): `lo -0.5`, `hi 0.5`, scale 2, so it covers x∈[-1,1] in the
   world; walking into it stops at about the same x as step 3.
7. `printLn("ok")`.

Add `fpwalk_check_j3_runs` to `src/compile.rs` tests, a copy of
`scene3d_check_j3_runs` with the path changed.

**Verify**: `cargo test fpwalk_check -- --test-threads=1` passes.

### Step 3: `scene3d.drawFrame`

Add it per Design.

**Verify**: `cargo test scene3d -- --test-threads=1` still passes (it only
typechecks the new function via `scene3d_check`).

### Step 4: Move the village onto the kit

In `examples/editor_village/src/main.j3`:
- `Player` → `Walker` (`use j3.scene3d.fpwalk; use j3.scene3d.fpwalk.Walker;`), drop `use editor_village.player...`.
- In `startup`, after `w.walker = fpwalk.spawn(&w.scene, w.mem);` add the
  village rules that `soft()` and the tree rule used to hold:
  `walkThrough` for `"grass"`, `"flower"`, `"crops_"`, `"plant_bush"`,
  `"campfire"`, `"mushroom"`, and `fpwalk.trunks(&w.walker, "tree", 0.2)`.
- `update`: `fpwalk.update(&w.walker, dt, !w.settings.open, w.settings.sensitivity, w.settings.walkSpeed);`
- `draw`: keep `settings.environment` and `setEnvironment` (and its ponytail
  note), replace the fog/clip/shadow/lights/camera/sky/draw lines with
  `scene3d.drawFrame(&w.scene, fpwalk.look(&w.walker), env, s.fov);`, keep
  `settings.panel(s, w.font);`. Drop now-unused imports (`gfx`, `Vec3` if unused).

Delete `examples/editor_village/src/player.j3`.

Note: the village now also collides with its top-level plain meshes. Its
ground is `builtin:plane` (skipped) and its sky is skipped; if `check` or a
quick look at the scene file shows other big top-level meshes, list them in
your report — don't add special cases.

**Verify**: `j3c check ../examples/editor_village` exits 0.

### Step 5: `j3c new NAME --template fps`

Per Design: `create_project(root, fps)`, `create_fps_project`,
`fps_template`, `FPS_SCENE` in `editor.rs`; the flag, help line
(`j3c new mygame --template fps   first-person 3D game with a scene (SDL2 + GL)`),
and "next:" line (`next: j3c check; j3c editor to edit scenes/main.j3scene`)
in `scaffold.rs`. Reject `--app` together with `--template`.

Add `fpwalk_template_typechecks` in `scaffold.rs` tests (pattern of
`app_project_typechecks`): create into a scratch dir, assert
`src/behaviors.j3`, `scenes/main.j3scene`, `j3.editor.toml`, `AGENTS.md`
exist and `src/main.j3` contains `drawFrame`, then
`crate::cli::run(["check".into(), made])` returns 0.

**Verify**: `cargo test fpwalk -- --test-threads=1` passes both tests.

### Step 6: Docs

Add `fpwalk` to the stdlib module list in the root `AGENTS.md` table row for
`j3c/stdlib/j3/`, and to `j3c/AGENTS.md` wherever it enumerates stdlib
modules (one short mention: "`j3.scene3d.fpwalk` is a first-person walker with box
colliders from scene objects; `scene3d.drawFrame` draws a scene from a `Look`").
Do not touch `EDITOR.md`.

**Verify**: `cargo test -- --test-threads=1` exits 0.

## Test plan

- `cargo test fpwalk -- --test-threads=1` is the plan's check: the J3
  self-check covers push-out, scaled boxes, walk-through, and jump/gravity;
  the scaffold test proves the template compiles against the new API.
- `j3c check ../examples/editor_village` proves the API covers the real game.
- No test opens a window. Don't run the village or the template; the user
  will.

## Done criteria

- [ ] `cargo test fpwalk -- --test-threads=1` passes (2 tests)
- [ ] `cargo test -- --test-threads=1` exits 0
- [ ] `j3c check ../examples/editor_village` exits 0 and `player.j3` is gone
- [ ] `j3c doc j3.scene3d.fpwalk` shows the API
- [ ] `rg -n "soft\(|\"tree\"" j3c/stdlib/j3/scene3d/fpwalk.j3` finds nothing (no village rules in the stdlib)
- [ ] Only in-scope files changed; `plans/README.md` untouched

## STOP conditions

- `j3.scene3d.fpwalk` can't import `j3.scene3d` without an import cycle. Report.
- The village's `player.j3` no longer matches the excerpts (someone changed
  the controller). Report the diff before generalizing.
- `fpwalk_check` step 3 lands inside the box or more than 0.1 past
  `-1 - RADIUS`: pushOut math differs from what this plan assumes. Report
  the numbers; don't retune constants.
- `j3c check` on the fps template fails for a missing library on this
  machine (SDL2 or GL). Report; don't drop GL from the template.

## Maintenance notes

- Game-specific collider rules belong in the game (`walkThrough`, `trunks`),
  never in `fpwalk.j3`.
- `drawFrame` is the one place a game draws a scene3d frame from a first-person
  view. Performance fixes from EDITOR.md (instancing, culling, cached world
  matrices) should land in `drawWith`/`drawFrame` so every game gets them.
- `create_project(root, fps)` is shared by `j3c editor --new` and
  `j3c new --template fps`; a third template should become a small enum, not
  another bool.
