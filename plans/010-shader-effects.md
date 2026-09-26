# Plan 010: Shader effects (.j3fx)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report. Do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: `git rev-parse --short HEAD` should be
> `b086a5b`. The tree has a lot of uncommitted work on top of it (the
> editor, `scene3d.j3`, `gfx.j3` changes, and more). That work is the base of
> this plan. Compare every "Current state" excerpt below to the live file. On
> a mismatch that changes what a step does, STOP.

## Status

- **Priority**: P2
- **Effort**: L (four phases; phase 1 alone is M)
- **Risk**: MED (touches the one lit shader every scene draws with)
- **Depends on**: none
- **Category**: feature
- **Planned at**: commit `b086a5b` plus uncommitted work, 2026-09-25

## Why this matters

Every mesh in a J3 scene is drawn with one fixed lit shader. A project cannot
get toon bands, rim light, dissolve, scrolling UVs, wind, underwater tint, a
vignette, or pixelation without forking `gfx.j3`. `EDITOR.md` ("Still left" >
Shader effects) lists this. The design below was agreed with the user:

- A `.j3fx` asset: a `key=value` header, a `---` line, then GLSL.
- Material slot: a hook in the lit fragment shader after lighting and glow,
  before fog.
- Camera slot: an ordered list of full-screen passes on a camera node.
- The same `scene3d` code runs in the editor, in `j3c -e render`, and in the
  game, so Play looks the same as the editor.

## Current state

**Programs are compiled once, and a failure panics.** `j3c/stdlib/j3/gfx.j3`:

```j3
// gfx.j3:1370
fn compile(kind: u32, src: string): u32 {
    ...
    if ok == 0 {
        shaderLog(sh);      // shaderLog -> dumpLog -> panic(log)
    }
```

`link` at `gfx.j3:1388` does the same with `programLog`. `init` at
`gfx.j3:1585` builds four programs, then caches about 44 uniform locations of
the lit program in `G` fields named `G.l<Name>`:

```j3
// gfx.j3:1591
    G.lit = link(compile(GL_VERTEX_SHADER, litVert()), compile(GL_FRAGMENT_SHADER, litFrag()));
    G.inst = link(compile(GL_VERTEX_SHADER, litVertInst()), compile(GL_FRAGMENT_SHADER, litFrag()));
    ...
    G.lTint = uloc(G.lit, "uTint");   // gfx.j3:1620
    G.lGlow = uloc(G.lit, "uGlow");
    ...
    lightLocs(G.lit, false);          // G.lP / G.lC / G.lR arrays
    samplerUnits(G.lit);              // gfx.j3:2357, sampler -> texture unit
```

`G.lLineProj`, `G.lLineView`, and `G.lLineColor` belong to the line program,
not the lit one. The instanced program (`G.inst`) never caches locations; it
calls `uloc` on every draw (`drawInstanced`, `gfx.j3:2133`).

**The lit shader is one string.** `litFrag()` at `gfx.j3:1432` returns the
whole fragment shader, uniforms first (`uTint`, `uGlow`, `uAlbedo`, `uSun`,
`uLP[8]`, ...), then helpers, then `void main(){`. The end of `main`:

```glsl
    rgb = mix(rgb, env * uTint, mirror);
  }
  rgb += uGlow;
  rgb = mix(rgb, uSky, fog);
  // Highlights stay bright on see-through surfaces.
  oColor = vec4(rgb, max(a, clamp(max(hi.r, max(hi.g, hi.b)), 0.0, 1.0)));
}
```

In `main`, `N` is the normal, `V` the view direction, `alb` the albedo,
`vPos` the world position, `vUv` the mesh UV, and `a` the alpha. `main`
returns early for `uFlat` (outline marks) and `vEmi` (emissive vertices).
Vertex shaders: `litVert()` at `gfx.j3:1427` and `litVertInst()` at
`gfx.j3:1437`. Both compute `vec4 world = <model> * vec4(aPos, 1.0);`, then
`vNrm = mat3(<model>) * aNrm;`.

**A material's look reaches the shader per draw through `gfx.Surface`.**
`gfx.j3:2042` `pub struct Surface { tint, glow, spec, shine, reflect, edge,
ior, rough, thickness, cloud, uvScale, uvOffset }`. `plainSurface()` at
`gfx.j3:2072` and the `G` initializer (`surf: Surface {` at `gfx.j3:746`) are
the only two struct literals. `gfx.surface(s)` stores it in `G.surf`.
`drawMesh` (`gfx.j3:2232`) uses `G.lit` and sets **every** lit uniform on
every draw, including proj/view/light/sun/sky/eye/fog:

```j3
    } else {
        glUseProgram(G.lit);          // gfx.j3:2280
        uMat(G.lProj, G.proj);
        ...
        let sf = G.surf;
        glUniform3f(G.lTint, sf.tint.x, sf.tint.y, sf.tint.z);
        glUniform3f(G.lGlow, sf.glow.x, sf.glow.y, sf.glow.z);
```

`beginCamera` (`gfx.j3:1930`) binds `G.outputFbo`, sets the viewport from
the view rect (`glViewport(i32(at.x), i32(win.y - at.y - sz.y), w, h)`), and
sets a few uniforms on `G.lit` only. `makeTarget` (`gfx.j3:2686`) makes a
color+depth FBO. `setOutputFbo` (`gfx.j3:2745`) routes the camera pass into
one. `examples/minecraft/src/post.j3` is a full-screen pass: a
`gl_VertexID` triangle vertex shader, an empty VAO, and its own compile.

**Materials.** `j3c/stdlib/j3/scene3d.j3:2065`:

```j3
struct Material {
    path: string;
    look: Surface;
    mode: int;
    ...
    image: Tex;
}
#[singleton]
let MATS: [64]Material;
```

`materialSlot` (`:2088`) loads on first use. `refreshMaterial` (`:2105`)
rereads the file. `previewMaterial` (`:2131`) shows unsaved editor edits.
`fillMaterial` (`:2147`) reads `.j3mat` keys (`tint`, `glow`, `spec`, ...,
`texture`, `noise`) with `save.getFloat` / `save.getStr` / `rgbKey`. Per
draw, `look(n)` (`:3094`) calls `gfx.surface(m.look)`, and `drawNode`
(`:3105`) copies `m.look`, sets `thickness`, and calls `gfx.surface(s)` again.

**Scene props** are the typed-value precedent: `PropDecl { name, kind,
value }` (`scene3d.j3:1726`), and the kinds the editor offers are
`float|int|bool|text|material|mesh|scene` (`editor/src/ui_props.j3:17`).

**Node fields** are parsed in `setField` (`scene3d.j3:221`) and written in
`write` (`material` line at `scene3d.j3:1101`). Undo compares nodes with
`sameNode` in `editor/src/history.j3:125` and `editor/src/watch.j3:139`.

**Editor.**
- `editor/src/watch.j3:42` watches only `.j3scene` and `.j3mat`. `:79` calls
  `scene3d.refreshMaterial(path)` when a `.j3mat` changes.
- `editor/src/ui_material.j3:80` `materialPanel` has sections MATERIAL, COLOR,
  SURFACE, GLOW, GLASS AND TRANSPARENCY, and TILING. The helpers are
  `matUnit`, `matWhole`, and `matColorRow` (`:69`), plus `putMatF` / `putMatI`
  in `materials.j3`.
- `editor/src/assets.j3:120` `assetForMaterial` only accepts images.
- `editor/src/console.j3:21` `conAdd(w, CON_ERROR, line)` writes to the
  console.
- The asset list comes from the host. `j3c/src/editor.rs:646`
  `collect_assets` keeps `["j3mesh", "png", "jpg", "jpeg", "j3scene",
  "j3mat"]`.
- Rendering: `editor/src/viewport.j3:146` (`drawViewport`) and
  `editor/src/render.j3:233` (`drawShot`, used by `j3c -e render` and MCP
  `render`). Both call `gfx.cameraPass` and then `scene3d.drawWith`.
- The game template is `game_template` in `j3c/src/editor.rs:354`. Its
  `draw()` calls `gfx.cameraPassLookFov(...)` and then
  `scene3d.draw(&w.scene)`. `examples/3d-editor-scene/src/main.j3` has the
  same `draw()`.

**No hash in the stdlib.** Nothing in `j3c/stdlib/j3/` hashes bytes, so the
program cache compares the file text instead (see Design).

## Design

### File format

```
kind=material
param.bands=int:3
param.rim=color:1,0.8,0.4
param.strength=float:0.5
param.on=bool:true
---
vec3 effect(Surface s, vec3 lit) {
  float l = max(dot(lit, vec3(0.299, 0.587, 0.114)), 0.001);
  vec3 toon = lit * (floor(l * float(uFx_bands)) / float(uFx_bands)) / l;
  float rim = pow(1.0 - max(dot(s.normal, s.view), 0.0), 3.0);
  return uFx_on ? toon + uFx_rim * rim * uFx_strength : lit;
}
```

- Header lines are `key=value`. Blank lines and lines that start with `#`
  are skipped. `kind` is `material` (default) or `camera`.
  `param.<name>=<type>:<default>`: the name uses letters, digits, and `_`.
  The types are `float`, `int`, `bool`, and `color` (`r,g,b` in 0..1).
  ponytail: 8 params and 16 floats per effect. Raise the limits if someone
  needs more.
- `---` ends the header. Everything after it is GLSL. Its first line is file
  line `headerLines + 2`.
- Each param becomes a `uniform <glsl type> uFx_<name>;` (`float`, `int`,
  `bool`, `vec3`), declared before the user GLSL.
- Built in for both kinds: `uniform float uTime;` (seconds,
  `game.ticks() / 1000`) and `uniform vec2 uResolution;` (view size in pixels).

**Material slot.** GLSL struct `Surface { vec3 pos; vec3 normal; vec3 view;
vec2 uv; vec3 albedo; float alpha; }`. The hook `vec3 effect(Surface s, vec3
lit)` runs as `rgb = effect(Surface(vPos, N, V, vUv * uUvScale + uUvOffset,
alb, a), rgb);` right after `rgb += uGlow;` and before fog. The stock shader
gets an identity `effect`, so there is only one shader layout. `discard` is
allowed in the hook, for dissolve. Phase 3 adds an optional
`vec3 displace(vec3 pos, vec3 nrm)` in world space.

**Camera slot.** The GLSL defines `vec4 effect(vec2 uv)`. Built in:
`uColor` (the previous pass's color, or the scene's), `uDepth` (the scene
depth), `uNear`, `uFar`, and `float linearDepth(vec2 uv)`. This differs from
the `effect(sampler2D color, sampler2D depth, vec2 uv)` sketch in
`EDITOR.md`. Uniforms are simpler and do the same job.

### Where values live

- Material: `.j3mat` keys `fx s=assets/fx/toon.j3fx`, then per param
  `fx.<name> f=` (float), `fx.<name>=` (int, bool as 0/1), and
  `fx.<name>.r/.g/.b` (color, the same as `tint`). Missing keys use the
  header default.
- Camera: node field `effects s=a.j3fx|b.j3fx` (ordered, `|`-separated, like
  choice options). Param values go in the existing per-object param table as
  `fx.<file stem>.<name>` (`scene3d.number` / `integer` / `flag` /
  `vector`), so copy, paste, placed-scene overrides, and undo already work.

### Programs and caching

- **Open decision (ask the user; see the gate before Step 2):**
  **per-effect programs** or **one uber-shader** that switches on an effect
  id. **Recommendation: per-effect programs.** Each effect is compiled on its
  own, so a GLSL error breaks only that effect. The stock shader stays as it
  is. Effects need no shared uniform budget. The cost is one program per
  effect, which is fine for a few dozen. An uber-shader would recompile
  everything on every save, and one bad effect would break every draw.
- gfx keeps a table of effect programs: `#[singleton] let FXP: [32]FxProg`.
  Each entry holds the program, its own `LitLocs` (the lit uniform
  locations), and its param locations, kinds, and float offsets. ponytail:
  32 programs. Raise it or switch to a growable arena if a project hits it.
- To make per-program locations possible, move the `G.l*` lit-program
  location fields (everything `uloc(G.lit, ...)` fills, plus the
  `lP`/`lC`/`lR` light arrays) into `struct LitLocs` with
  `fn litLocs(prog: u32): LitLocs`. `G.stock: LitLocs` holds the stock ones,
  and `G.L: LitLocs` holds the locations of the program the current draw
  uses. `drawMesh` already sets every uniform per draw, so switching programs
  only needs `glUseProgram(p)` plus `G.L = <its locs>`. `samplerUnits(p)` runs
  once per new program.
- scene3d keeps `#[singleton] let EFFECTS: [32]Effect`, keyed by path. Each
  entry keeps the file text it was compiled from. `effectOf(path)` rereads
  the file and recompiles only when the text differs. Nothing else in the
  stdlib hashes, so a byte compare stands in for "cached by the effect's
  hash". Several materials that use one effect share its program.
- Compile failures never panic. Add `tryCompile` / `tryLink` in gfx that
  return 0 and append the driver log to a `Text`. `compile` / `link` keep
  panicking for the built-in shaders.
- Error lines are mapped back to the `.j3fx` file. Mesa logs
  `0:<line>(<col>): error: ...` and NVIDIA logs `0(<line>) : error ...`.
  `fileLine = line - firstLine + bodyLine`, printed as
  `<path>:<fileLine>: <rest>`. A line that doesn't parse is passed through
  unchanged. An effect that fails is drawn with the stock program.
- Errors go to `io.printLn` (a game's stdout reaches the editor console
  through Play) and into a scene3d buffer that the editor drains into the
  console with `scene3d.takeEffectErrors`.

### Known limits (write them in the docs, don't fix them)

- URP traced hits and the shadow map ignore effects. Displaced meshes cast
  undisplaced shadows.
- Instanced draws ignore material effects until phase 3.
- Outline effects need a normal buffer. Not in this plan.
- Under-water switching (a camera inside a volume) is a behavior's job. It
  can set the camera's `effects` field.

## Commands you will need

From the repository root. Build the compiler once (the `j3fx` asset
extension in `editor.rs` needs it). After that, stdlib and editor edits are
picked up with no rebuild through `J3_STDLIB` and `J3_EDITOR_SRC`.

```sh
export R=$PWD
export J3C=$R/j3c/target/release/j3c
export J3_STDLIB=$R/j3c/stdlib
export J3_EDITOR_SRC=$R/j3c/editor/src
export T=$(mktemp -d)
```

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Build compiler | `cd $R/j3c && timeout 900 cargo build --release` | exit 0 |
| Stdlib checks (no GL) | `timeout 300 $J3C run $R/j3c/examples/scene3d_check.j3` | prints `ok` |
| Editor typechecks | see "Editor check" below | exit 0, no errors |
| Render a scene | `timeout 600 $J3C -e render --project $T/fx --focus Cube --size 64x64 --out target/fx.png` | prints the PNG path |
| Center pixel | `python3 $T/pixel.py $T/fx/target/fx.png` | `R G B` |
| Rust tests touched | `cd $R/j3c && timeout 900 cargo test editor:: -- --test-threads=1` | exit 0 |

**Editor check** (typecheck the editor sources with no window):
`timeout 600 $J3C -e scene --project $T/fx list`. It builds the frontend from
`J3_EDITOR_SRC` against `J3_STDLIB` and runs the headless tool. A compile
error prints the compiler output and exits non-zero.

**Fixture** (create once, after Step 1):

```sh
cp -r $R/examples/3d-editor-scene $T/fx && rm -rf $T/fx/target && mkdir -p $T/fx/assets/fx
printf 'kind=material\nparam.color=color:1,0,1\n---\nvec3 effect(Surface s, vec3 lit) {\n  return uFx_color;\n}\n' > $T/fx/assets/fx/flat.j3fx
printf 'fx s=assets/fx/flat.j3fx\n' > $T/fx/assets/materials/fxtest.j3mat
timeout 600 $J3C -e scene --project $T/fx set Cube material=assets/materials/fxtest.j3mat
```

**Pixel reader** (`$T/pixel.py`, stdlib only, prints the center pixel):

```python
import sys, zlib, struct
def rgba(path):
    b = open(path, 'rb').read(); i = 8; idat = b''; w = h = ct = 0
    while i < len(b):
        n, t = struct.unpack('>I4s', b[i:i+8]); d = b[i+8:i+8+n]; i += 12 + n
        if t == b'IHDR': w, h, _, ct = struct.unpack('>IIBB', d[:10])
        if t == b'IDAT': idat += d
    bpp = 4 if ct == 6 else 3; raw = zlib.decompress(idat); stride = w * bpp; rows = []; prev = bytearray(stride); p = 0
    for _ in range(h):
        f = raw[p]; line = bytearray(raw[p+1:p+1+stride]); p += 1 + stride
        for x in range(stride):
            a = line[x-bpp] if x >= bpp else 0; up = prev[x]; c = prev[x-bpp] if x >= bpp else 0
            if f == 1: line[x] = (line[x] + a) & 255
            elif f == 2: line[x] = (line[x] + up) & 255
            elif f == 3: line[x] = (line[x] + (a + up) // 2) & 255
            elif f == 4:
                pa, pb, pc = abs(up - c), abs(a - c), abs(a + up - 2 * c)
                line[x] = (line[x] + (a if pa <= pb and pa <= pc else up if pb <= pc else c)) & 255
        rows.append(line); prev = line
    return w, h, bpp, rows
w, h, bpp, rows = rgba(sys.argv[1]); x, y = w // 2, h // 2
print(*rows[y][x*bpp:x*bpp+3])
```

Never launch `j3c editor` (it opens a window). The user tests the GUI.

## Scope

**In scope**:
- `j3c/stdlib/j3/gfx.j3`: `LitLocs`, `tryCompile`/`tryLink`, the effect
  program table, the splice, `uTime`/`uResolution`, the post passes, and
  `Surface.fx`/`fxVals`
- `j3c/stdlib/j3/scene3d.j3`: `.j3fx` parse, the effect cache, material
  `fx`, the node `effects` field, `beginEffects`/`endEffects`, and the error
  buffer
- `j3c/examples/scene3d_check.j3`: parser and log-mapping checks
- `j3c/editor/src/`: `watch.j3`, `ui_material.j3`, `assets.j3`,
  `render.j3`, `viewport.j3`, `ui_inspector.j3`, `history.j3`, `tool.j3`,
  and `types.j3`/`main.j3` for new `World` fields
- `j3c/src/editor.rs`: `"j3fx"` in `collect_assets`, and the
  `game_template` `draw()`
- `EDITOR.md` (move Shader effects from "Still left" to done, with the
  limits) and `j3c/editor/README.md` (a short "Effects" section)

**Out of scope**:
- URP trace shader support for effects, shadow-pass displacement, normal
  buffers
- A shader graph, `#include`, or effects on sprites/2D
- Rewriting `examples/*/src/main.j3` (only the template changes)
- Plans 011+ (editor extensions)

## Git workflow

Branch `plan-010-shader-effects` from the current tree. Commit per phase:
`Shader effects: material hook (.j3fx)`, `Shader effects: camera passes`,
`Shader effects: vertex displace`, `Shader effects: inspector`. Do not push.
Do not update `plans/README.md`.

## Steps

### Phase 1: Material effects end to end

#### Step 1: Parse `.j3fx` (pure, no GL)

In `scene3d.j3`, add:

```j3
pub const FX_FLOAT: int = 0; pub const FX_INT: int = 1; pub const FX_BOOL: int = 2; pub const FX_COLOR: int = 3;
pub struct FxParam { name: string; kind: int; def: [3]f32; }
pub struct FxFile { ok: bool; error: string; camera: bool; params: []FxParam; glsl: string; bodyLine: int; }
pub fn parseEffect(bytes: []u8, arena: &Arena): FxFile
```

Reject a missing `---`, an unknown type, a bad name, or more than 8 params
or 16 floats. Report it as `ok=false` with `error` naming the file line.
`kind=camera` sets `camera`. Add `pub fn mapGlslLog(log: string, path:
string, firstLine: int, bodyLine: int, out: &Text)` as described in Design.

Add checks to `j3c/examples/scene3d_check.j3` (it must still print only
`ok`):
- the toon sample from Design parses: 4 params, `bodyLine == 7`, color default `1,0.8,0.4`
- no `---` gives `ok == false`
- `param.x=vec9:1` gives `ok == false`
- `mapGlslLog("0:103(5): error: foo\n", "a.j3fx", 100, 7, &t)` gives `a.j3fx:10: error: foo`
- `mapGlslLog("0(103) : error C1: foo\n", ...)` also gives line 10
- a line with no number is passed through unchanged

**Verify**: `timeout 300 $J3C run $R/j3c/examples/scene3d_check.j3` prints `ok`.

#### Gate: ask the user before Step 2

If the prompt that launched you does not record the user's answer to
"per-effect programs or one uber-shader", STOP here. Report Step 1 and ask.
The steps below assume per-effect programs.

#### Step 2: `LitLocs`, no behavior change

In `gfx.j3`, move every lit-program location (`G.lProj` ... `G.lGrabbed`,
`G.lP/lC/lR`; **not** `G.lLine*`) into `struct LitLocs`. Fill it with `fn
litLocs(prog: u32): LitLocs`. Add `G.stock` and `G.L` and set both in
`init`. Replace the `G.l<Name>` reads with `G.L.<name>`.
`uploadLightsTo(false)` reads `G.L`. Keep the instanced path as it is.

**Verify**: `rg -n "G\.l[A-Z]" j3c/stdlib/j3/gfx.j3 | rg -v "G\.lLine"` shows
nothing. The fixture render from the Commands table, **with the cube's
material set back to** `assets/materials/stone.j3mat`, still works. Run it
before and after this step, and the center pixel must match exactly.

#### Step 3: Splice point and effect programs

1. Split `litFrag()` into `litFragTop()` (uniforms and helpers, up to but not
   including `void main(){`) and `litFragMain()`. Add `uniform float uTime;
   uniform vec2 uResolution;` to the top. Add `FX_STRUCT` (the GLSL
   `Surface` struct) and `FX_IDENTITY` (`vec3 effect(Surface s, vec3 lit) {
   return lit; }`). In `litFragMain`, insert after `rgb += uGlow;`:
   `  rgb = effect(Surface(vPos, N, V, vUv * uUvScale + uUvOffset, alb, a), rgb);`.
   `litFrag()` = top + struct + identity + main.
2. Add `tryCompile(kind, src, log: &Text): u32` and `tryLink(vs, fs, log:
   &Text): u32`. They return 0 on failure and never panic.
3. Add `pub struct FxSlot` params (`name`, `kind`), then `pub fn
   effectProgram(glsl: string, params: []FxParam, log: &Text, firstLine:
   &int): int`. It builds top + struct + `uniform ... uFx_<name>;` lines +
   `glsl` + main, sets `firstLine` to the 1-based line of `glsl`'s first
   line, and compiles with the stock `litVert()`. It returns slot + 1, or 0.
   It fills `FXP[slot]` with `litLocs(prog)`, `samplerUnits(prog)`, and the
   param locations and offsets. `pub fn freeEffect(slot: int)` deletes the
   program. `shutdown` deletes all of them.
4. `Surface` gains `fx: int` (0 = stock) and `fxVals: [16]f32`. Update both
   literals (`gfx.j3:746`, `plainSurface`).
5. `drawMesh` camera branch: `let prog = G.lit; G.L = G.stock; if sf.fx > 0 {
   prog = FXP[sf.fx - 1].prog; G.L = FXP[sf.fx - 1].locs; }`. Then
   `glUseProgram(prog)`, set the params from `sf.fxVals` (`glUniform1f` /
   `1i` / `3f` by kind), and set `uTime` and `uResolution` through `G.L`
   (add both to `LitLocs`). Leave the shadow pass and `drawInstanced`
   unchanged.

**Verify**: the stock render center pixel is unchanged from Step 2's
baseline.

#### Step 4: Effects in scene3d materials

1. Add `struct Effect { path; text: []u8; file: FxFile; slot: int; }`,
   `EFFECTS: [32]Effect`, and `pub fn effectOf(path: string): int` (slot + 1
   into EFFECTS, 0 when missing, broken, or `camera`). It loads with
   `io.readFile`, compares the text, and on a change or first use parses and
   calls `gfx.effectProgram`. On failure it maps the log and prints and
   buffers it.
2. `Material` gains nothing new: `look.fx` / `look.fxVals` carry it. In
   `fillMaterial`, read `fx`. If it is set and `effectOf` works, set
   `m.look.fx` to the gfx slot and fill `fxVals` from `fx.<name>` keys or the
   header defaults.
3. `pub fn refreshEffect(path: string)`: recompile if the text changed, then
   `refreshMaterial` every loaded material whose `fx` key is `path`. Store
   the fx path on `Material` as `fxPath: string` for this.
4. `pub fn takeEffectErrors(mem: &Arena): string` returns the buffer and
   clears it. `pub fn effectParams(path: string, arena: &Arena): []FxParam`
   is for the inspector.
5. `drawNode` copies `m.look`, so `fx` and `fxVals` carry through.

**Verify**: build the Fixture (Commands). `timeout 600 $J3C -e render
--project $T/fx --focus Cube --size 64x64 --out target/fx.png` and then
`python3 $T/pixel.py $T/fx/target/fx.png` print a magenta pixel: red > 200,
green < 60, blue > 200. Fog may shift it a little. Then break the effect:
`printf 'kind=material\n---\nvec3 effect(Surface s, vec3 lit) {\n  return nope;\n}\n' > $T/fx/assets/fx/flat.j3fx`.
Render again: it still succeeds, and the pixel is **not** magenta (stock
fallback).

#### Step 5: Editor, `.j3fx` in assets, the material field, hot reload, errors

1. `j3c/src/editor.rs:646`: add `"j3fx"` to the extension list. In
   `ui_tree.j3:336`, tag `.j3fx` as `effect`. Rebuild (`cargo build
   --release`).
2. `assets.j3` `assetForMaterial`: a `.j3fx` sets `fx` on the material
   (`matBefore`, `save.putStr(&w.mat.file, "fx", path)`, `matChanged`) and
   updates a new `w.matEffect: Text` (add it to `types.j3` and `main.j3` like
   `matTexture`, and refresh it where `matTexture` is refreshed in `tabs.j3`
   and `materials.j3`).
3. `ui_material.j3`: add an `EFFECT` section before TILING. It has a path
   field (`w.matEffect`, placeholder `assets/fx/effect.j3fx`, caption "Type
   an effect path, or click one under Assets") and a `None` button that
   clears `fx`. For each `scene3d.effectParams(path)` it shows `matRange`
   (float, key `fx.<name>`, range -100..100), `matWhole` (int), a toggle
   (bool, `putMatI`), or `matColorRow` (color).
4. `watch.j3`: watch `.j3fx` too (`:42`), and on change call
   `scene3d.refreshEffect(path)` (`:79`). At the top of `pollWatch`, before
   the `EVERY` gate, drain `scene3d.takeEffectErrors(scratch(0))` line by
   line into `conAdd(w, CON_ERROR, line)`, and open the console the way
   build errors do (see `host.j3:69`).

**Verify**: the Editor check (Commands) exits 0.
`rg -n '"j3fx"' j3c/src/editor.rs` shows the new entry. `cd $R/j3c &&
timeout 900 cargo test editor:: -- --test-threads=1` exits 0. The user tests
the GUI: they open a material, click the `.j3fx`, and see the params, a
save, the live change, and a broken edit going to the console with the right
line.

Commit phase 1.

### Phase 2: Camera effects stack

#### Step 6: gfx post passes

1. `pub fn postProgram(glsl, params, log, firstLine): int` builds the
   full-screen program. The vertex shader is `post.j3`'s `gl_VertexID`
   triangle. The fragment is `#version 330 core`, `in vec2 vUv; out vec4
   oColor;`, `uColor` (unit 0), `uDepth` (unit 1), `uTime`, `uResolution`,
   `uNear`, `uFar`, `linearDepth`, the param uniforms, the user GLSL, then
   `void main(){ oColor = effect(vUv); }`. It uses the same `FXP` table with
   a `post: bool` flag and one empty VAO in `G`.
2. `pub fn postBegin()`: save `viewState()` and `G.outputFbo`. Make or resize
   two `Target`s to the view extent (`G.postA` with depth, `G.postB`). Set
   the output to `G.postA.fbo` and the view rect so the camera pass lands at
   0,0 (the same trick as `drawShot`).
3. `pub fn postPass(slot: int, vals: []f32, last: bool)`: reads the current
   source color (A's color first, then whichever was written last) and A's
   depth. It writes the other target, or, when `last`, the saved output FBO
   in the saved view rect (`glViewport` as in `beginCamera`). Depth test is
   off while it runs. After `last`, restore the view state and output FBO.

**Verify**: the stock fixture render center pixel is unchanged (no camera
effects set).

#### Step 7: Camera `effects` field and scene3d wrapper

1. `Node.effects: string`: add it to `blank`, `setField`, and `write` (only
   for `CAMERA` with a non-empty value), and to the node compares in
   `editor/src/history.j3:125` and `editor/src/watch.j3:139`.
2. `tool.j3` `set`: accept `effects=<a|b>` on cameras. Mirror how `material`
   is validated there and reject non-`.j3fx` entries.
3. `pub fn beginEffects(scene: &Scene, cam: int): bool` returns false (and
   does nothing) when the camera has no loadable `kind=camera` effects. Else
   it calls `gfx.postBegin()`. `pub fn endEffects(scene: &Scene, cam: int)`
   runs `gfx.postPass` for each effect in order, with values from
   `scene3d.number/integer/flag/vector(scene, cam, "fx.<stem>.<name>",
   default)`.
4. Call sites (Play must match the editor):
   - `game_template` `draw()` in `editor.rs:354`: find the main camera id in
     the existing fov loop, then `let fx = scene3d.beginEffects(&w.scene,
     cam);` before `cameraPassLookFov`, and `if fx {
     scene3d.endEffects(&w.scene, cam); }` after `scene3d.draw`.
   - `render.j3` `drawShot`: when the lens came from a camera node (store
     that id on `Lens`), wrap the camera pass the same way.
   - `viewport.j3` `drawViewport`: wrap when `w.viewCam != 0`. Also wrap
     `drawCameraPreview` for its camera. Draw the grid, markers, and gizmos
     after `endEffects` so tools stay crisp.

**Verify**:
```sh
printf 'kind=camera\n---\nvec4 effect(vec2 uv) { return vec4(0.0, 1.0, 0.0, 1.0); }\n' > $T/fx/assets/fx/green.j3fx
printf 'kind=camera\n---\nvec4 effect(vec2 uv) { return vec4(texture(uColor, uv).gbr, 1.0); }\n' > $T/fx/assets/fx/swap.j3fx
timeout 600 $J3C -e scene --project $T/fx set Camera effects='assets/fx/green.j3fx|assets/fx/swap.j3fx'
timeout 600 $J3C -e render --project $T/fx --camera Camera --size 64x64 --out target/cam.png
python3 $T/pixel.py $T/fx/target/cam.png
```
It prints `255 0 0`: green, then channel swap, so order and ping-pong both
work. With only `green.j3fx` it prints `0 255 0`. The Editor check exits 0.
`cd $R/j3c && timeout 900 cargo build --release && timeout 900 cargo test
editor:: -- --test-threads=1` exits 0. To check that the template compiles,
copy the template `draw()` into `$T/fx/src/main.j3` and run
`timeout 600 $J3C check $T/fx`. It exits 0.

Commit phase 2.

### Phase 3: Vertex displace hook

#### Step 8

1. If the effect GLSL contains `displace(`, `effectProgram` also builds a
   vertex shader. It is the `litVert` text with the param and `uTime`
   uniforms, the user GLSL (a vertex shader cannot see the fragment `Surface`
   struct or `effect`, so wrap the fragment-only part: put the user GLSL in
   the vertex stage between `#define J3_VERTEX` guards and document that
   `effect` must be inside `#ifndef J3_VERTEX`, **or**, simpler, require
   `displace` to sit in a `#ifdef J3_VERTEX` block, which the vertex stage
   defines). Pick the second. In `main`: `vec3 n = mat3(uModel) * aNrm;
   world.xyz = displace(world.xyz, n); vNrm = n;`. Do the same for a
   per-effect instanced variant built from `litVertInst()`.
2. `drawInstanced` uses `G.surf.fx`'s instanced program when there is one.
   It already calls `uloc` per draw, so it only needs the program id and the
   param uploads.
3. Document the limit: shadows and URP traces use the undisplaced mesh.

**Verify**: a `kind=material` effect with
`#ifdef J3_VERTEX\nvec3 displace(vec3 p, vec3 n) { return p + vec3(0.0, 100.0, 0.0); }\n#endif`
and an identity `effect` moves the cube out of view. The fixture render
center pixel is no longer the cube's color: it matches a render with the
cube's material set to stone and the cube moved away, or just differs from
the stone baseline. Step 1's scene3d_check still prints `ok`.

Commit phase 3.

### Phase 4: Inspector polish

#### Step 9

1. Camera inspector (`ui_inspector.j3`, camera section near `:380`): an
   `EFFECTS` list with one row per entry (name, up, down, x). Clicking a
   `.j3fx` under Assets while a camera is selected appends it (in
   `assets.j3` `useAsset`). Each entry shows its params with the same widget
   choice as the material section, stored through
   `scene3d.setNumber/setInteger/setFlag/setVector(&w.scene, id,
   "fx.<stem>.<name>", v)` and `modified(w)`. Placed copies go through the
   existing override path (`j3.scene3d.editor.overrides.overrideParam`).
2. Pull the per-kind widget choice into one `@j3ui fxParam(...)` in
   `ui_material.j3`, used by both the material and camera sections. Do not
   touch `ui_props.j3`: its kinds differ.
3. The asset right-click menu gets **New effect** (materials) and **New
   camera effect**. They write `assets/fx/effect<N>.j3fx` from a template
   (identity hook, one float param) the way `files.j3:291` makes a new
   material.
4. The material and camera sections show the effect's last error as a red
   caption. Keep the last error text per `Effect` entry and expose it as
   `scene3d.effectError(path)`.

**Verify**: the Editor check exits 0. `timeout 300 $J3C run
$R/j3c/examples/scene3d_check.j3` prints `ok`. Update `EDITOR.md` (Shader
effects moves out of "Still left", keeping the known limits) and
`j3c/editor/README.md` (an "Effects" section: the file format, both slots,
built-ins, and limits).

Commit phase 4.

## Test plan

- `scene3d_check.j3`: parser and log mapping (Step 1). This is the
  regression lock for the format.
- The render pixel checks: stock unchanged (Steps 2, 3, 6), material effect
  magenta and broken-effect fallback (Step 4), camera stack order (Step 7),
  displace (Step 8).
- `cargo test editor::` for the asset list and template.
- The GUI (inspector, hot reload, console lines) is tested by the user. Do
  not launch the editor.

## Done criteria

- [ ] `scene3d_check.j3` prints `ok` with the new checks
- [ ] The stock render pixel is identical before and after phase 1
- [ ] The magenta material effect renders; a broken effect falls back to stock and does not panic
- [ ] The two-pass camera stack renders `255 0 0`
- [ ] The editor frontend builds from `J3_EDITOR_SRC` (`-e scene list` exits 0)
- [ ] `cargo test editor::` exits 0
- [ ] `EDITOR.md` and `j3c/editor/README.md` describe `.j3fx` and its limits
- [ ] `plans/README.md` not modified by you

## STOP conditions

- The user's choice between per-effect programs and an uber-shader is
  unknown at the Step 2 gate.
- `litFrag()` no longer ends with `rgb += uGlow;` then `rgb = mix(rgb, uSky,
  fog);`. Describe what it is now.
- The stock render pixel changes after Step 2 or Step 3. The refactor broke
  something. Do not tune the expected value.
- `j3c -e render` cannot produce a PNG on this machine even before your
  changes (no EGL/offscreen). Report it. The pixel checks can't run.
- A J3 language limit blocks a design point (for example struct fields that
  are arrays of structs in a singleton). Describe it rather than
  restructuring the plan.
- Adding a node field breaks old scene files loading (the `scene3d_check`
  round trip fails).

## Maintenance notes

- Any new lit uniform must go into `LitLocs`, or effect programs silently
  miss it.
- The hook runs before fog on purpose. Effects that want to skip fog need a
  flag, which this plan doesn't add.
- Renaming a `.j3fx` through the editor does not rewrite `.j3mat` files that
  point at it (`rename_asset` only rewrites scenes). Add that if it bites.
- ponytail ceilings: 32 effect programs, 8 params / 16 floats per effect,
  and the program cache compares file text instead of hashing.
- URP (`gfx` trace path and `stdlib/j3/gfx/urp/`) ignores effects. Porting an
  effect there is a separate plan.
