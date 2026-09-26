# Plan 006: URP tracing performance

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report. Do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: `git rev-parse --short HEAD` should be
> `b086a5b`. There is a lot of uncommitted work on top of it (`git status`
> shows `gfx.j3`, `scene3d.j3`, `bvh.j3`, `examples/editor_village/` and
> more as modified or untracked). That is expected. The commit alone is not
> enough. Drift-check by comparing every "Current state" excerpt below with
> the live file. If an excerpt no longer matches, STOP.
>
> **Running things**: never open a visible window. Every game run uses
> `--headless` (or `SDL_VIDEODRIVER=offscreen`) and is wrapped in `timeout`.
> Do not loop on live repros. Measure once per step as described, then move
> on. The user checks the windowed result.

## Status

- **Priority**: P1
- **Effort**: L (phased; Phase A alone is S)
- **Risk**: MED (shader edits can't be unit-tested; a GLSL error breaks every 3D draw)
- **Depends on**: soft dependency on plan 007 Steps 1–2 (bench preset hook + frame profiler). If they have not landed, Step 1 below adds the preset hook and uses wall-clock timing.
- **Category**: performance
- **Planned at**: commit `b086a5b` + uncommitted work, 2026-09-25

## Why this matters

`examples/editor_village` runs at about 90 FPS headless with URP (2 bounces, 1 sample). With **Traced sun shadows** on, the user sees 5–10 FPS. The scene is low poly. The cost comes from every lit pixel walking the whole BVH toward the sun, and from hidden and overdrawn pixels paying the same cost (EDITOR.md "Findings from the village").

What the shader actually does (read in `litFrag`). The "up to 8 sun rays" in EDITOR.md are **not 8 samples**. `sunReach` casts **one** ray toward the sun, and its `for (int k = 0; k < 8; k++)` loop only continues that ray past see-through surfaces, one segment per see-through hit. For an opaque occluder the loop returns after the first segment. The real per-pixel costs are:

1. `traceRay` is a **closest-hit** search. A shadow only needs *any* opaque hit, but today the traversal keeps walking until the stack is empty, looking for a nearer triangle.
2. The traversal pushes children in fixed order (`first`, `first + 1`) with no near-first ordering, so `best` shrinks slowly.
3. The main surface computes the 3×3 PCF shadow map and then **throws it away** in favor of `sunReach` for every pixel with `ndl > 0`, at any distance, whether the map was already certain or not.
4. It runs in the forward pass, so overdrawn fragments trace too.

The cheapest fixes are shader-only (Phase A). Structural fixes (render scale, a screen-space shadow pass, a program split) come after, each gated on measured numbers.

## Current state

All GLSL lives in one J3 string on **`j3c/stdlib/j3/gfx.j3:1433`** (`fn litFrag()`, lines 1432–1435). The excerpts below are unescaped (`\n` removed) for reading. In the file they are one line.

Shadow/trace uniforms and BVH walk (`gfx.j3:1433`):

```glsl
float traceRay(vec3 ro, vec3 rd, float reach, out int hit, out vec2 bary) {
  vec3 inv = 1.0 / (rd + vec3(equal(rd, vec3(0.0))) * 0.000001);
  float best = reach;
  hit = -1;
  bary = vec2(0.0);
  int stack[32];
  int sp = 0;
  stack[sp++] = 0;
  for (int guard = 0; guard < 4096; guard++) {
    if (sp == 0) break;
    int n = stack[--sp];
    vec4 lo = nodeAt(n, 0);
    vec4 hi = nodeAt(n, 1);
    ...
    if (t1 < max(t0, 0.0) || t0 > best) continue;
    int count = int(hi.w);
    int first = int(lo.w);
    if (count > 0) {
      for (int k = 0; k < 4; k++) { ... Möller–Trumbore on triAt(t, 0..2) ... }
    } else if (sp < 30) {
      stack[sp++] = first;
      stack[sp++] = first + 1;
    }
  }
  return best;
}
```

Note `else if (sp < 30)`: a deeper tree silently drops subtrees.

```glsl
vec3 sunReach(vec3 p, vec3 n) {
  vec3 L = normalize(uSun);
  if (uTraceShadows < 0.5) { ... 3x3 PCF shadow map ... return vec3(lit / 9.0) * passedLight(proj); }
  vec3 pass = vec3(1.0);
  vec3 ro = p + n * 0.003;
  for (int k = 0; k < 8; k++) {
    int h;
    vec2 b;
    float d = traceRay(ro, L, 1000.0, h, b);
    if (h < 0) return pass;
    vec4 m0 = triAt(h, 6);
    vec4 m1 = triAt(h, 7);
    vec4 m2 = triAt(h, 8);
    bool entering = dot(L, cross(triAt(h, 1).xyz, triAt(h, 2).xyz)) < 0.0;
    if (m1.w > 1.001) {
      if (entering) pass *= m0.rgb * (1.0 - m2.z) * (1.0 - schlick(m1.w, 1.0));
    } else if (m2.y < 0.999) {
      if (entering) pass *= 1.0 - m2.y;
    } else {
      return vec3(0.0);
    }
    if (max(pass.r, max(pass.g, pass.b)) < 0.01) return vec3(0.0);
    ro += L * (d + 0.003);
  }
  return pass;
}
```

Main surface (`main()`, same line):

```glsl
  vis /= 9.0;
  if (proj.z > 1.0 || proj.x < 0.0 || proj.x > 1.0 || proj.y < 0.0 || proj.y > 1.0) {
    vis = 1.0;
  }
  vec3 sunLight = vec3(vis) * passedLight(proj);
  // URP with traced shadows: every surface traces its sun ray, so glass casts tinted shadows.
  if (uTraceShadowAll > 0.5 && ndl > 0.0) sunLight = sunReach(vPos, N);
  ...
  float dist = length(vPos - uEye);
```

Triangle texels (`triAt(t, k)`, 9 texels per triangle): 0 = v0, 1 = e1, 2 = e2, 3–5 = corner normals, 6 = albedo+mirror, 7 = glow+ior, 8 = rough, opacity, cloud. Nodes: 2 texels (`nodeAt`), `lo.w` = first tri or left child, `hi.w` = tri count (0 = inner, right child = left + 1). Both textures are `TRACE_W = 1024` texels wide (`gfx.j3:2483`).

Uniform plumbing in `drawMesh` (`gfx.j3:2321–2333`):

```j3
        let tracing = G.trace.count > 0 && (sf.reflect > 0.001 || sf.ior > 1.001);
        glUniform1f(G.lTraceOn, if tracing { 1 } else { 0 });
        glUniform1f(G.lTraceShadowAll, if G.trace.count > 0 && G.traceShadows { 1 } else { 0 });
        if G.trace.count > 0 {
            ...
            glUniform1f(G.lTraceShadows, if G.traceShadows { 1 } else { 0 });
        }
```

`traceUse` (`gfx.j3:2533`) sets `G.trace`, `G.traceBounces`, `G.traceSamples`, `G.traceShadows`. Uniform locations are looked up once in `init` (`gfx.j3:1585–1690`, e.g. `G.lTraceShadowAll = uloc(G.lit, "uTraceShadowAll");` at 1639). The instanced program `G.inst` links the **same** `litFrag()` (`gfx.j3:1590`) but never gets trace uniforms. That is fine here.

Programs: `G.lit = link(compile(GL_VERTEX_SHADER, litVert()), compile(GL_FRAGMENT_SHADER, litFrag()));` (`gfx.j3:1589`). There is one lit fragment program for plain and traced drawing.

Scene side (`j3c/stdlib/j3/scene3d.j3`):
- `pub const TRACE_MAX: int = 65536;` (2824). `buildTrace` (2901) clamps `if total > TRACE_MAX { total = TRACE_MAX; }` (2909) and gives no warning.
- `traceScene` (2842) rebuilds the whole BVH whenever `traceSignature` (2874) changes. Comment at 2841: `// ponytail: whole rebuild on any change, 4 scenes cached; refit the BVH if big scenes lag while dragging.`
- `drawWith` (3013) draws opaque nodes in scene order, then see-through ones far-to-near. It calls `gfx.grab()` before **each** glass node (`if gfx.inCameraPass() && back.mat > 0 && MATS[back.mat - 1].look.ior > 1.001 { gfx.grab(); }`).
- `Env` (around line 94) has `urp`, `bounces`, `samples`, `tracedShadows`. They are saved as `scene.env.urp*` keys in `environment`/`setEnvironment` (1167/1188).

`j3c/stdlib/j3/gfx/bvh.j3`: midpoint split on the longest centroid axis, `LEAF = 4`, `NODE_FLOATS = 8`, and children are always allocated after their parent (`let left = b.count; b.count += 2;`). Depth is not tracked.

Render targets: `gfx.makeTarget(w, h): Target { fbo, color, depth, w, h }` (`gfx.j3:2686`, color RGBA8 + depth texture), `gfx.setOutputFbo(id)` (2745), `gfx.readPixels`. Nothing draws a target to the window (EDITOR.md "MCP Fixes" #14). `j3c/editor/src/render.j3:236–238` shows the pattern for drawing into a target: `gfx.setOutputFbo(target.fbo); ... gfx.setViewRect(0, win.y - f32(target.h), f32(target.w), f32(target.h));`.

Village draw loop, `examples/editor_village/src/main.j3:56–73`:

```j3
    gfx.shadowPass(env.sunDir, Vec3.xyz(p.x - r, 0 - r, p.z - r), Vec3.xyz(p.x + r, r, p.z + r));
    if env.shadows { scene3d.draw(&w.scene); }
    scene3d.lights(&w.scene);
    gfx.cameraPassLookFov(player.look(&w.player), env.sunDir, env.sky, env.sunColor, s.fov);
    scene3d.drawSky(&w.scene);
    scene3d.draw(&w.scene);
    settings.panel(s, w.font);
```

Village settings (`examples/editor_village/src/settings.j3:37–81`): defaults are `urp: true, bounces: 2, samples: 1, tracedShadows: false, shadows: true, shadowRange: 45`. The `ultra` preset turns `tracedShadows` on (along with bounces 4 and samples 4). There is no way to pick a preset without the UI.

Headless runs: `j3c run <project> --frames N [--screenshot F.png] --headless` (`j3c/src/cli.rs:174–208`) sets `J3_FRAMES` and the offscreen SDL driver. `j3.app` stops after N frames (`j3c/stdlib/j3/app.j3:351–424`).

## Design

Phases, cheapest first. Each step measures before and after. A step that does not pay off is reverted, and its numbers are recorded in the commit message.

- **Phase A (shader only):** any-hit shadow query, near-first traversal, and gating which pixels trace (penumbra of the shadow map, plus a distance limit). These target the headline.
- **Phase B (cheap structure):** BVH cap warning and raise, depth guard, `gfx.present(target)` with a render-scale setting (MCP Fixes #14), and only the nearest glass layer shading when tracing.
- **Phase C (bigger, gated on Phase A/B numbers):** split plain and traced programs, opaque depth prepass, a half-res screen-space sun-shadow pass with depth-aware upsample that is reused while nothing moves, BVH refit instead of rebuild, compact triangle storage, frame accumulation, and a per-instance box proxy.

Not in this plan: the editor's "Trace in editor" toggle and preview settings (separate UI work), `j3.gfx.urp.*` modules (separate renderers), and draw-call and CPU work (plan 007).

## Commands you will need

`J3C=/home/jacoby/Documents/code/ai-stuff/j3/j3c/target/release/j3c` (also `~/.bin/j3c`). Rebuild it after stdlib edits only if the stdlib is embedded. Check with `grep -n include_str j3c/src/stdlib/mod.rs`. If it is embedded, run `cd j3c && cargo build --release`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Typecheck village | `timeout 120 $J3C check examples/editor_village` | exit 0 |
| Build village | `timeout 300 $J3C build examples/editor_village` | exit 0, `examples/editor_village/target/editor_village` exists |
| Bench run (N frames, preset P) | `cd examples/editor_village && /usr/bin/time -f "%e" timeout 300 env VILLAGE_PRESET=P J3_FRAMES=N SDL_VIDEODRIVER=offscreen SDL_AUDIODRIVER=dummy ./target/editor_village` | exit 0, prints elapsed seconds (and a `gfx:` line if plan 007 landed) |
| Screenshot | same as bench plus `J3_SCREENSHOT=/tmp/claude-1000/v_<name>.png` | PNG written |
| Compiler tests | `cd j3c && timeout 900 cargo test -- --test-threads=1` | exit 0 |
| One test | `cd j3c && cargo test bvh_check_j3_runs -- --test-threads=1` | exit 0 |

**FPS (wall clock)**: run N=30 and N=630 with the same preset. FPS = `600 / (t630 − t30)`. The subtraction removes startup (scene load, BVH build, shader compile). With plan 007's profiler, use its printed `gpu=` average instead and also record wall FPS.

## Scope

**In scope**:
- `j3c/stdlib/j3/gfx.j3` (lit shader, trace uniforms, `present`, prepass, screen shadow pass)
- `j3c/stdlib/j3/scene3d.j3` (`Env` fields, `traceScene`, `buildTrace`, `drawWith` glass order, TRACE_MAX)
- `j3c/stdlib/j3/gfx/bvh.j3` (depth tracking, optional refit helper)
- `j3c/examples/bvh_check.j3` and a test in `j3c/src/compile.rs`
- `examples/editor_village/src/settings.j3`, `main.j3` (preset hook, render scale, new settings)
- `EDITOR.md` "Performance" section: update the bullets that change (only at the end)

**Out of scope**:
- `j3c/stdlib/j3/gfx/urp/*` (separate renderers)
- Editor UI (`j3c/editor/src/*`) except that it must still compile (`drawWith` callers)
- Draw batching, culling, world-matrix caching, profiler (plan 007)
- New dependencies of any kind

## Git workflow

Branch `plan-006-urp-tracing-performance`. Commit after each step that stays (message: what changed + before/after FPS). Do not push. Do not update `plans/README.md`.

## Steps

### Step 1: Baseline measurement

1. If `VILLAGE_PRESET` is not already read (plan 007 Step 1), add it. In `examples/editor_village/src/settings.j3`, declare `use lib "c" as libc { fn getenv(name: cstr): cstr; fn strlen(s: cstr): int; }` (the same pattern as `j3c/stdlib/j3/app.j3:5–38`). At the end of `defaults()`, read `VILLAGE_PRESET`: `low`/`medium`/`high`/`ultra` call the preset. `traced` keeps the defaults but sets `tracedShadows = true`. Anything else changes nothing.
2. Build, then bench with N=30 and N=630 for presets `(unset)` and `traced`. Record FPS for both.
3. Add one temporary `printLn` in `buildTrace` with `total` **before** the clamp and `tree.count`. Run 30 frames and record the numbers. Remove the print afterwards. This tells whether the village exceeds `TRACE_MAX`: if it does, traced shadows today miss geometry. Record it.
4. Take reference screenshots at frame 60: `v_base.png` (unset) and `v_traced.png` (traced).

**Verify**: the numbers are written down in the commit message of Step 2 (no commit for Step 1 beyond the preset hook: `Add VILLAGE_PRESET bench hook to the village.`).
**Expected**: default FPS is roughly 90 and `traced` is much lower (5–30 headless, depending on the GPU). If `traced` is within 20% of the default headless, STOP (see STOP conditions).

### Step 2: Any-hit shadow query

In `litFrag`, add `bool occluded(vec3 ro, vec3 rd, out bool glassy)`. It is a copy of `traceRay`'s loop with these changes:
- There is no `best`/`hit`/`bary`. The box test is `if (t1 < max(t0, 0.0)) continue;`.
- On a triangle hit with `d > 0.0005`, read `m1 = triAt(t, 7)` and `m2 = triAt(t, 8)`. If `m1.w > 1.001 || m2.y < 0.999`, set `glassy = true` and continue. Otherwise `return true;`.
- Return `false` at the end.

In `sunReach`'s traced branch, before the `for (int k ...)` loop:

```glsl
  bool glassy = false;
  if (occluded(p + n * 0.003, L, glassy)) return vec3(0.0);
  if (!glassy) return vec3(1.0);
```

Leave the existing loop unchanged below it. It now only runs for rays that cross see-through surfaces and nothing opaque.

**Verify**: build, and bench `traced` N=30/630. Screenshot `v_traced_anyhit.png` at frame 60 and compare with `cmp v_traced.png v_traced_anyhit.png`.
**Expected**: FPS for `traced` goes up (shadowed pixels exit early). The image is identical or differs only by a few pixels. If `cmp` differs, look at both PNGs with the Read tool. Shadows must be in the same places. A visible difference is a STOP.

### Step 3: Near-first traversal

In `occluded` and `traceRay`, when an inner node passes its box test, read both children's boxes. Compute each child's entry `t0` with the same slab test, and push only the children that are hit (and, for `traceRay`, whose `t0 <= best`), **farther first** so the nearer one pops next. Keep the `sp < 30` guard. The popped node's own box test becomes redundant, but keep it for the root (push the root only if its box is hit).

**Verify**: bench `traced` and `(unset)` (mirrors and glass also use `traceRay`). Screenshots vs Step 2.
**Expected**: `traced` FPS improves or stays flat. Images are identical. If FPS drops by more than 3% for either preset, revert this step and record the numbers.

### Step 4: Trace only where the shadow map is unsure, and only near the camera

Add uniform `uniform float uTraceNear;` (meters, 0 = no limit) to `litFrag`, with a location `G.lTraceNear` and a gfx setter `pub fn traceShadowRange(near: f32)` (stored in `G.traceNear`, sent in `drawMesh` next to `lTraceShadowAll`). In `main()`, move `float dist = length(vPos - uEye);` above the sun block and replace the traced line with:

```glsl
  bool inMap = !(proj.z > 1.0 || proj.x < 0.0 || proj.x > 1.0 || proj.y < 0.0 || proj.y > 1.0);
  bool unsure = !inMap || (vis > 0.001 && vis < 0.999) || passedLight(proj) != vec3(1.0);
  bool near = uTraceNear <= 0.0 || dist < uTraceNear;
  if (uTraceShadowAll > 0.5 && ndl > 0.0 && unsure && near) sunLight = sunReach(vPos, N);
```

`unsure` means outside the map box, in the PCF penumbra, or under a translucent caster. Where the map says fully lit or fully shadowed, trust it. When the scene's shadow map is off (`env.shadows` false), the map is empty and `vis` is 1 everywhere, so nothing would trace. `scene3d` must then pass the map-off state. The simplest way is to send `uTraceNear = 0` and force `unsure = true` with a second uniform `uMapOn` (0/1) that `scene3d.shadows`/games set through `gfx.shadowMapOn(on)`. Default is on. The village and `scene3d.shadows` set it from `env.shadows`.

Scene setting: add `Env.traceNear: f32`, saved as `scene.env.urpShadowNear` (default 40) in `environment`/`setEnvironment`. `traceScene` calls `gfx.traceShadowRange(settings.traceNear)`. The village sets it from a new `Settings.traceNear` (default 40, a slider "Traced shadow distance" 0–200 next to the traced toggle).

**Verify**: bench `traced`. Screenshot `v_traced_gated.png`. Compare with the Read tool against `v_traced.png`.
**Expected**: a large FPS gain (most pixels skip tracing). Shadows look the same up close, and far shadows come from the map. **Target: `traced` headless ≥ 50% of the `(unset)` FPS.** If shadow edges show visible seams (hard map edge vs traced edge), widen the penumbra test. Compute a second `vis` over a 3×3 with 2-texel spacing just for `unsure`. Record which version was kept.

### Step 5: BVH depth guard and the 65536 cap

1. `bvh.j3`: add `depth: int` to `Bvh`, computed during `build` (store depth per work item as a 4th int). In `j3c/examples/bvh_check.j3`, assert `depth <= 29` for its random cases, since the shader stack drops pushes at `sp >= 30`. Add a `bvh_check_j3_runs` test in `j3c/src/compile.rs` copying `scene3d_check_j3_runs` (it prints `ok`).
2. `scene3d.buildTrace`: when `total` (before clamping) exceeds `TRACE_MAX`, or `tree.depth > 29`, print one line the first time per scene key: `scene3d: URP traces 65536 of 81234 triangles; raise TRACE_MAX or split the scene` / `scene3d: URP tree is N deep; traced rays may skip geometry`.
3. Raise `TRACE_MAX` to `262144`. The tris texture is `262144 × 9 / 1024 = 2304` rows (37.7 MB RGBA32F), well under any GL 3.3 max texture size (≥ 1024 is guaranteed; check `GL_MAX_TEXTURE_SIZE` 0x0D33 via `glGetFloatv` once in `traceUpload`. If `rows` exceeds it, clamp `count` and print the same warning).
4. If the depth test fails for the village's real tree (printed in the step 1 style), change `bvh.split`'s fallback. When one side of the midpoint cut holds more than 7/8 of the triangles, split at the median instead (sort not needed: use the existing partition around the mean of the centroids, and fall back to halving).

**Verify**: `cargo test bvh_check_j3_runs`, and a village 30-frame run shows no warning (or shows one, with the true triangle count recorded).
**Expected**: test passes. The village either fits or prints the warning once.

### Step 6: `gfx.present(target)` and render scale (MCP Fixes #14)

1. gfx: load `glBlitFramebuffer`. Add constants `GL_READ_FRAMEBUFFER 0x8CA8`, `GL_DRAW_FRAMEBUFFER 0x8CA9`, `GL_COLOR_BUFFER_BIT` (already used as `GL_COLOR_BUFFER_BIT`) and `GL_LINEAR`. Add:

   ```j3
   /// Stretch `t`'s color over the whole window with linear filtering; then draw UI on top.
   pub fn present(t: Target)
   ```

   It binds `t.fbo` as READ and 0 as DRAW, calls `glBlitFramebuffer(0, 0, t.w, t.h, 0, 0, win.x, win.y, GL_COLOR_BUFFER_BIT, GL_LINEAR)`, and rebinds 0.
2. Village: `Settings.renderScale` (default 1, slider "Render scale" 0.5–1, and presets: low 0.75, ultra 1). `World` keeps a `target: Target` plus its size. In `draw`, when `renderScale < 0.99`: (re)make the target when the window size or scale changed (`freeTarget` the old one), `gfx.setOutputFbo(t.fbo)`, `gfx.setViewRect(0, win.y - th, tw, th)` (the `render.j3:238` pattern), draw the shadow and camera passes as now, then `gfx.setOutputFbo(0)`, `gfx.setViewRect(0, 0, 0, 0)`, `gfx.present(t)`, then `settings.panel`. At scale 1, the path is unchanged.
3. Mention `gfx.present` in `j3c/AGENTS.md`'s `j3.gfx` sentence (one clause).

**Verify**: bench `traced` with `VILLAGE_SCALE`… (add `renderScale` to the preset hook: preset `traced_half` = traced + scale 0.5). Screenshot at scale 0.5. Read the PNG: it fills the frame, is softer, and has no black borders.
**Expected**: `traced_half` FPS ≈ 2–4× `traced` if the frame is fragment-bound.

### Step 7: Only the nearest glass layer traces

When URP tracing is on, each glass fragment already traces through every layer behind it (`follow`), and a glass fragment's traced output is written opaque (`a = 1.0` in the `uIor > 1.001` branch). So in `scene3d.drawWith`, when `gfx` is tracing (add `pub fn tracing(): bool { return G.trace.count > 0; }`), and there is at least one glass node (`MATS[...].look.ior > 1.001`, `alphaOf == BLEND`):
1. Before the see-through loop, call `gfx.grab()` once, then draw every glass node depth-only. Add `pub fn depthOnly(on: bool)` in gfx that toggles `glColorMask` (load `glColorMask`) and forces depth writes (`glDepthMask(1)`) for blended draws. Use the normal lit program, so depth values match exactly.
2. Then run the existing far-to-near loop, skipping the per-glass `grab()` when tracing. Glass behind the nearest layer now fails `GL_LEQUAL` and does not shade. Non-glass see-through nodes still blend as before.

Without tracing, keep today's path exactly (screen-space glass needs every layer and every grab).

**Verify**: build a scene with 6 nested glass cubes. Take `j3c/examples` or make one quickly with `j3c -e scene` in `/tmp/claude-1000/glass6/`, URP on, bounces 2, and measure headless FPS before/after with the same wall-clock method (EDITOR.md measured 100 → 50 FPS when going 1 → 6 cubes). Render one frame before and after, and compare with the Read tool.
**Expected**: 6-cube FPS ≥ 85% of the 1-cube FPS. The image looks the same apart from noise.

### Step 8 (Phase C, gated): Split plain and traced lit programs

Gate: build `litFrag` with the trace code removed (temporarily: with URP off in the village, preset `low` minus shadows change, i.e. `VILLAGE_PRESET=low`). Compare FPS with an otherwise unchanged build. **Only if the plain shader is ≥ 5% faster**, do the split:
- `litFrag(traced: bool)` prepends `#define URP 1` after `#version 330 core\n` when traced. `#ifdef URP` guards `traceRay`, `occluded`, `follow`, `jitter`, the traced parts of `sunReach`, and the `tracing` branches in `main()`. The plain `sunReach` is just the shadow-map path.
- Move the ~45 `G.l*` uniform locations into `struct LitLocs { prog: u32; proj: i32; ... }` with two instances `G.plain` and `G.traced`, filled by one `litLocs(prog)` function. `drawMesh` picks `G.traced` when `G.trace.count > 0`, else `G.plain`, and uses its fields. `lightLocs` gets the same treatment (or keeps the `inst` flag plus a third set).
- `samplerUnits` runs for both.

**Verify**: `check` + bench `(unset)`, `low`, `traced`. All screenshots are unchanged.
**Expected**: `low` gets faster by the gated amount, and the others don't regress more than 2%. If the gate fails, record "skipped: plain N FPS vs combined M FPS".

### Step 9 (Phase C, gated): Half-res screen-space sun shadows, reused while still

Gate: after Steps 2–7, `traced` headless is still below 60% of `(unset)` (or the user reports < 30 FPS in the window). Requires Step 6's target path, because the camera must render into a `Target` to read its depth.

1. **Opaque depth prepass** (camera pass only, when traced shadows are on): add `G.pre = link(litVert, preFrag)`. `preFrag` only discards cutout texels. Add `invariant gl_Position;` to `litVert` so the two programs' depths match. `gfx.prepass(on)` routes `drawMesh` to `G.pre` with `glColorMask` off. `scene3d.drawWith` (when `gfx.wantsPrepass()`) draws the opaque and cutout nodes once through the prepass, then again normally. Blended nodes are skipped in the prepass.
2. **Shadow trace pass**: after the prepass, `gfx.traceSunScreen()` renders a full-screen triangle at half the target size into `G.sunTex` (RGBA16F, 0x881A; rgb = light, a = linear view depth) with a new small program. It reads `t.depth`, reconstructs the world position with `inverse(uProj * uView)` (uploaded from J3; add `Mat4.inverse` only if `math.j3` lacks a general inverse; `affineInverse` is not enough for a projection), and uses the nearest of the 4 full-res depths under the texel. It offsets along the sun, `ro = P + L * (0.002 + 0.001 * depth)`, and calls the same `occluded` + see-through loop (share the GLSL text by building both shaders from one J3 string constant). Only pixels in the map's `unsure` set trace (reuse the Step 4 test by also binding the shadow map). Other pixels copy the map's value.
3. **Lit pass**: new uniforms `uSunScreen`/`uSunScreenOn`. When on, and the fragment is opaque, `sunLight` comes from a depth-aware 2×2 upsample of `uSunScreen`: bilinear weights × `1 / (1 + abs(za - zi) * 50 / za)`, normalized. When all 4 weights are tiny, fall back to `sunReach`. Blended and glass fragments keep calling `sunReach`.
4. **Reuse while still**: `traceSunScreen` skips rendering and keeps last frame's texture when the view matrix, projection, sun direction, the `Traced` identity (`tris` id + `count`), and the target size are all unchanged. Compare the 16+16+3 floats with the values stored last frame.

**Verify**: bench `traced`. Screenshots at frames 60 and 61 are identical (reuse path). Read `v_traced_screen.png` against `v_traced.png`: the same shadows, no halos at silhouettes wider than one pixel.
**Expected**: **`traced` headless ≥ 80% of `(unset)` FPS** while standing still. Walking (no reuse) should still be ≥ 60%. Measure this with a preset `traced_walk` that makes the player walk forward: `player.update` input can't be faked, so instead add `VILLAGE_SPIN=1`, which turns the look yaw 0.5°/frame in `update`.

### Step 10 (Phase C, gated): Refit instead of rebuild

Gate: dragging or animated behaviors cause visible hitches (the user reports it, or a scene with a moving behavior shows a `buildTrace` time > 5 ms with a temporary timer via `game.ticks()`).

Keep, per trace slot, `order`, the tree, a per-triangle source `(node slot, vertex offset)`, and a structure key (a sum over traced meshes of `vao`, `mat`, vertex count, plus `scene.len`). When the signature changes but the structure key doesn't, recompute the world corners for each triangle, rewrite the surface texels, and refit boxes. Leaves take the bounds of their triangles, and inner nodes are refit in **reverse index order** (children always have larger indices; see `bvh.build`). Then re-upload both textures. Do a full rebuild when the structure key changes. Add `bvh.refit(b, corners)` in `bvh.j3` and cover it in `bvh_check.j3`: after moving all corners by +1 on x, every node still holds its children.

**Expected**: moving one object rebuilds in < 1/5 of the previous time for the village. `cargo test bvh_check_j3_runs` passes.

### Step 11 (Phase C, gated): Compact triangle storage

Gate: only if a real scene needs more than `TRACE_MAX` = 262144 triangles, or the tris texture memory is a reported problem. Otherwise record "skipped".
Layout 9 → 6 texels: 0 = v0 + material index, 1 = e1, 2 = e2, 3 = albedo.rgb + mirror, 4 = n0.xy n1.xy (octahedral), 5 = n2.xy + unused. A new `uMats` texture holds per material (3 texels: glow+ior, rough, opacity, cloud). Update `triAt` callers (`sunReach`, `follow`, `shadeHit`) and `buildTrace`. Then raise `TRACE_MAX` to 524288.

### Step 12 (Phase C, optional): Frame accumulation for rough surfaces

Only with Step 6's target path and when `samples > 1` matters to the user. Ping-pong two `Target`s. When still (same test as Step 9.4), blend the new frame into the history with weight `1 / (n + 1)` (a full-screen pass), and pass a `uFrame` seed into `hash13` in `jitter`. Present the history. Reset `n` on any change. With this, `samples = 1` per frame converges to many. Record whether it was done.

### Step 13 (Phase C, optional): Per-instance box proxy

Only if, after Steps 2–9, profiling shows traversal still dominates in scenes with many instances. Two-level structure: top BVH over placed-node world boxes, bottom BVHs per shared mesh in local space, with the ray transformed by the instance's inverse matrix. This is L effort and changes `buildTrace`, `bvh.j3`, and every `traceRay` call. Write it as its own plan instead of doing it here. Record the measured reason.

### Step 14: Docs

Update EDITOR.md "Performance → URP" and "Findings from the village". Replace the fixed items with one line each saying what was done and the measured FPS. Correct "up to 8 sun rays" to "one sun ray, continued through up to 8 see-through surfaces".

## Test plan

- `bvh_check_j3_runs` (new) locks BVH invariants, the depth bound, and refit (if Step 10 is done).
- Existing `cargo test` stays green, including `scene3d_check_j3_runs`.
- Shader changes have no unit tests. Screenshot comparisons (`cmp`, then Read the PNGs on mismatch) after each step are the regression check. Keep the PNGs under `/tmp/claude-1000/` and do not commit them.

## Done criteria

- [ ] Step 1 numbers are recorded: default FPS, `traced` FPS, village triangle count vs TRACE_MAX
- [ ] After Phase A (Steps 2–4): `traced` headless FPS ≥ 50% of default headless FPS (wall-clock method, 600 frames after 30 warm-up)
- [ ] Default (no traced shadows) headless FPS has not dropped more than 3% from Step 1
- [ ] `traced` screenshots show the same shadows as the Step 1 reference (checked with Read)
- [ ] A clamp or deep tree prints a warning, and `TRACE_MAX` is 262144
- [ ] `gfx.present` exists, and the village has a working render-scale slider
- [ ] 6 nested glass cubes at bounces 2 run at ≥ 85% of 1 cube's FPS with URP on (Step 7)
- [ ] Every gated step is either done with before/after numbers in its commit, or recorded as skipped with the gate's numbers
- [ ] `cd j3c && cargo test` exits 0, and `j3c check examples/editor_village` exits 0
- [ ] The user confirms, in the real window, that traced shadows now run at ≥ 30 FPS on their machine (hand this over; do not attempt it)

## STOP conditions

- Any "Current state" excerpt doesn't match the live file (drift).
- Step 1: `traced` is already within 20% of the default headless FPS. The headless GPU path differs from the user's, and the plan's numbers can't be trusted. Report both numbers.
- A GLSL compile or link error you can't fix in two tries (gfx prints the log via `shaderLog`/`programLog`).
- A screenshot comparison shows shadows or reflections in different places after a step that should be image-identical (Steps 2, 3, 8).
- The village prints the TRACE_MAX warning *and* raising to 262144 is still not enough. Report the count and do Step 11 only after the reviewer agrees.
- Any step needs a new library, a compiler change, or edits to `j3c/stdlib/j3/gfx/urp/*`.

## Maintenance notes

- `occluded` and `traceRay` share the traversal. Keep their near-first logic identical when either changes.
- The Step 4 `unsure` test couples traced shadows to the shadow map. A game that turns the map off must say so (`gfx.shadowMapOn(false)`), or traced shadows disappear. `scene3d.shadows` does it for scenes.
- `bvh.build` must keep "children after parent" if refit (Step 10) is done.
- If the lit program is split (Step 8), every new lit uniform goes into `LitLocs` once.
- Reviewers: check that `invariant gl_Position` is in `litVert` if the prepass (Step 9) or glass depth fill (Step 7) shows z-fighting.

## Execution notes (2026-09-25)

Village, 1280×720 headless, RTX 5060 Ti, camera-pass GPU ms from `J3STATS` (which now calls `glFinish` in `--frames` runs; before that it showed CPU time only and the Step 1 wall-clock method was needed).

| Step | Default | Traced |
|------|---------|--------|
| Baseline (TRACE_MAX 65536) | 1.6 ms, 323 FPS | 45 ms, 21 FPS |
| TRACE_MAX 262144 (village has 197,992 triangles, so traced shadows used to miss two thirds of it) | 1.7 | 49 |
| Any-hit `occluded` | 3.2 (bigger shader) | 50 |
| Gating: map unsure + `urpShadowNear` 40 | 3.1 | 22 |
| SAH BVH (12 bins) | 3.1 | 19.5 |
| Shift indexing, stack 48 | 2.9 | 14.8 |
| Lit program split (plain / sun / full, `TRACE_LEVEL`) | 0.2, ~600 FPS | 1.7–2.2, ~250 FPS |

Deviations:
- Step 3 (near-first): reverted, slower (default 1.7 → 3.2 ms).
- Step 4: no `gfx.shadowMapOn`; gfx sets `mapUsed` when anything draws into the shadow pass. The near distance goes through `traceUse(..., near)`, not a separate setter. `urpShadowNear` is also in the editor inspector, `scene_set`, and `j3c -e scene`.
- Step 5: the SAH split replaced the midpoint split (about 20% faster traces, rebuild 116 → 168 ms, depth 27 → 31), so the shader stack went from 32 to 48 (pushes stop at 46) and the warning threshold to 45. There's no `GL_MAX_TEXTURE_SIZE` check (ponytail note at `TRACE_MAX`).
- Step 6 (render scale): skipped. Traced is about 2 ms of GPU time.
- Step 7 (nearest glass layer): reverted. 6 nested cubes went 18.8 → 2.9 ms, but at 2 bounces rays run out before reaching the inner cubes, so they vanish.
- Step 8: done as three variants (plain, sun, full), not two. A fourth (mirrors without the traced-sun code) and a single shadow tap at hits measured no gain.
- Steps 9–13: gates not met (headless traced about 4 ms a frame; no hitch reports; 198k < 262144) or optional.
- Follow-up (user report: the village pond drops under 30 FPS with URP): `follow`'s path stack in the program was the cost, not traversal (a heatmap showed 10–60 nodes visited per water pixel). Mirrors now use a stackless `mirrorRay` in their own variant (`LIT_MIRROR`; variants are flags now, compiled on first use), and `traceRay` visits the nearer child first. Pond view (`VILLAGE_POND=1`) went from 8.0 to 1.2 ms; spawn views are pixel-identical. Glass still uses `follow` (about 5 ms in the glass test scene).
- Follow-up 2 (ultra at the pond, 20–50 FPS in the window): 4 samples on 0.01-rough water and a traced sun ray at every reflection hit. `samplesFor` now ramps linearly to the full count at roughness 0.2, and `sunReach(p, n, gated)` applies the map-unsure and `uTraceNear` test for hits (the main surface passes `gated` true, having tested already). Pond ultra 8.0 to 2.9 ms; the glass scene 5.2 to 3.9 ms; spawn default and traced are pixel-identical.
