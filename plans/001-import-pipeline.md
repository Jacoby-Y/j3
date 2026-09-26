# Plan 001: Import pipeline

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report. Do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has a lot of uncommitted work on top of
> `b086a5b` (all of `j3c/editor/`, `j3c/src/png.rs`, `gltf.rs`, `mcp.rs`, and
> `scene.rs` are untracked), so the SHA says nothing. Compare every "Current
> state" excerpt below with the live file. If an excerpt no longer matches in
> substance, STOP and report which one.

## Status

- **Priority**: P1
- **Effort**: L (four phases, each S–M; each phase ships on its own)
- **Risk**: MED (a new inflate in J3 without bit operators; a new `Node` field touches the scene format)
- **Depends on**: none
- **Category**: feature / bug
- **Planned at**: uncommitted tree on `b086a5b`, 2026-09-25

## Why this matters

Building `examples/editor_village` through MCP hit five import problems (EDITOR.md "MCP Fixes" #2–#6):

- **#2** Without SDL2_image, every texture silently renders white: in the editor, in `render`, and in games. It happened on this machine, and nothing said so.
- **#3** A bad glTF gave "bad array" with no position and no file name.
- **#4** `import` takes one file per call, so a Kenney kit is about 50 calls.
- **#5** Palette kits (Kenney's `colormap.png`) needed a Python script. It sampled the texture at each triangle's UV centroid, wrote vertex colors, and cleared the `texture s=` field.
- **#6** Teal greens and a red roof meant copying `.j3mesh` files and hue-shifting their vertex colors by hand.

## Current state

### Language limits that shape the J3 work

The surface language has no `%`, `&`, `|`, `^`, `<<`, or `>>`. `j3c/src/parse/ast.rs:376`:

```rust
pub enum BinOp {
    Add, Sub, Mul, Div, Eq, Ne, Lt, Le, Gt, Ge, And, Or,
}
```

(The IR `BinOp` in `j3c/src/ir/op.rs:5` has `Rem`/`BitAnd`/`Shl`, but the parser never produces them.) `canvas.j3` already works around this. Line 17 says "Four-bit XOR lookup keeps PNG checksums independent of new language operators", and `x - (x / n) * n` is used for remainders throughout (for example `scene3d_check.j3:99`). The J3 decoder must do the same: bit reads by multiply and divide by powers of two.

### #2 Image loading

`j3c/stdlib/j3/game.j3:1193`, `ensureImage()` returns false when `libSDL2_image-2.0.so.0` can't be opened. `loadRgba` (line 1965) then quietly returns an empty image:

```j3
pub fn loadRgba(path: string, arena: &Arena): Rgba {
    if !ensureImage() {
        return rgbaEmpty(arena);
    }
    let surf = IMG_Load(cstr(path));
```

`loadTexture` (1979) panics and `tryLoadTextureC` (1998) returns an empty `Texture` in the same case. `textureFromCanvas(canvas: Canvas): Texture` (2022) already uploads RGBA8 bytes for both GL and SDL renderers, with filtering picked from `canvas.mode` (`Mode.Pixel` gives nearest). `Rgba` is `game.j3:966` (`pixels: []u8; w: i32; h: i32`). `game_headless.j3:480` `loadRgba` always returns an empty image.

Every 3D texture goes through `scene3d.j3:2278` `loadImage` → `game.loadRgba`, so `loadRgba` is the choke point for the editor, `render`, `play`, and games.

`j3c/stdlib/j3/gfx/canvas.j3` is headless and already imported by `game.j3:6` and `game_headless.j3:5`. It writes PNGs (`savePng`, line 599, stored DEFLATE blocks only) but cannot read them. `Canvas` is line 135 (`pixels :[] u8; w : i32; h : i32; mode : Mode;`). `Mode` is line 108 (`Pixel, Smooth`).

`j3c/src/preflight.rs:114` reports calls to `loadRgba`/`tryLoadTexture` as errors when SDL2_image is missing (`api()` at line 288 maps them to the `image` Api). Once PNG decodes in J3, that probe is wrong for PNGs.

### #3 JSON errors

`j3c/src/lsp/json.rs:50`:

```rust
pub fn parse(src: &str) -> Result<Json, String> {
    let mut p = Parser { b: src.as_bytes(), i: 0 };
    let v = p.value()?;
    p.skip_ws();
    if p.i != p.b.len() {
        return Err("trailing JSON".to_string());
    }
    Ok(v)
}
```

The errors are bare strings: "bad JSON value" (99), "bad array" (191), "bad object" (217), and so on. `Parser.i` is the byte position at failure. `gltf.rs:106` and `:109` call `json::parse(..)?` with no file name.

### #4 Import one file per call

`j3c/src/mcp.rs:419` `Server::import` reads one `path`, calls `crate::editor::import_asset(root, &fragment, &source)` (`editor.rs:891`), then `keep_import` (`editor.rs:876`, which moves the fragment to `assets/imported/<name>.j3scene`) and `generate_assets_catalog`. It places the result with an `add instance scene=<file>` request unless `place` is false. The schema is at `mcp.rs:858`:

```json
{"name":"import", ... "required":["path"],"properties":{
   "path":{"type":"string", ...}, "place":{"type":"boolean","default":true},
   "scene", "name", "parent", "pos"}}
```

### #5 Bake

`j3c/src/gltf.rs:584` `convert_primitive` builds each primitive's `.j3mesh` in memory. The header is `# j3mesh 2` when textured (then rows are `px py pz nx ny nz r g b emissive u v`), else `# j3mesh 1`. It returns `Look { asset, texture, .. }`. `color` is the sRGB-converted base color factor (`srgb`, line 10). `write_images` (354) has already written each needed image to `assets/imported/<prefix>_image<i>.png|jpg` and returns their project-relative paths. `import` (770) → `convert` (791) builds a `Ctx` (560). No PNG decoder exists in Rust. `j3c/src/png.rs` only has `inflate_stored` (57, stored blocks only), `refilter`, `deflate_fixed`, `adler32`, `crc32`, and `chunk`, which are used by `compact` (7) to shrink renders.

### #6 Recolor

`Node` (`scene3d.j3:47`) has `color: Vec3` (default `1, 0.9, 0.75` in `blank`, line 173) that only lights use. `store` writes `cr cg cb` only for `LIGHT` (line ~1106). `gfx.Surface.tint` (`gfx.j3:2044`, "Multiplies the albedo") exists. Materials (`.j3mat`) set it, and plain meshes draw with `gfx.plainSurface()`. `scene3d.j3:3094`:

```j3
fn look(n: Node) {
    if n.mat > 0 {
        let m = MATS[n.mat - 1];
        gfx.surface(m.look);
        ...
    } else {
        gfx.surface(gfx.plainSurface());
```

`drawNode` (3105) sets `gfx.surface(s)` again for materials. `buildTrace` (~2920) bakes `look.tint` into traced triangle colors, and `traceSignature` (2874) hashes material tints so the BVH rebuilds. Placed scenes are copied in `graft` (1495). Per-placement overrides (`over.<inst>.<src>.<key>`) are applied by `applyOverrides` (1597), which calls `setField` (221), so any field `setField` knows can be overridden. Editor side: `tool.j3:816` handles `set color=`, `history.j3:125` `sameNode` lists every compared field, `overrides.j3:93` writes overrides for changed fields, and `ui_inspector.j3:387` shows `colorRow(ui, 4, &n.color)` for lights.

The village's nature models are one mesh node per glTF primitive (for example `flower_redA_mesh0_prim0` stem, `_prim1` petals). So tinting one mesh node inside an imported scene recolors only that part.

## Design decisions

- **Two PNG decoders, on purpose.** Bake runs inside `gltf.rs` (Rust) at import time, where the vertices already are. A Rust decoder is about 150 lines with real bit operators. Running a J3 program from import (like `gen.rs` does) would cost a compile per import. Games and the editor need decoding at run time in J3 with no SDL2_image, so J3 needs its own. Both are checked against the same fixture files.
- **Rejected: dlopen `libz.so.1`.** It would skip inflate in J3, but it adds a runtime library the Windows/macOS `sys` targets may lack. The point of #2 is "works with nothing extra installed".
- **Supported PNGs**: 8-bit depth, not interlaced, color types 0 (gray), 2 (RGB), 3 (palette, with `tRNS`), 4 (gray+alpha), and 6 (RGBA). Anything else, and JPEG, still needs SDL2_image. Checksums (CRC and Adler) are not verified. `// ponytail:` note it.
- **Inflate algorithm**: zlib's `contrib/puff` (canonical Huffman decoded one bit at a time from `count[]`/`symbol[]` tables). It is the smallest correct one. `// ponytail:` bit-at-a-time decode; add a first-9-bits lookup table if texture loads show up in a profile.
- **Recolor is one field**: `tint` (r,g,b, 1 = unchanged, above 1 allowed) on `MESH` and `INSTANCE` nodes. It multiplies the surface tint at draw and trace. On an instance it multiplies into every placed mesh under it at `graft`. Hue shift and color remap are skipped. Tint with values above 1 makes teal from green (`0.7,1,1.8`) and red from a brown roof (`1.6,0.6,0.6`). `// ponytail:` add hue shift if multiply can't reach a color someone needs. A separate field, not the light `color`, because `color` defaults to warm white and is stored only for lights.
- **Import many**: `paths: []`, or `path` naming a folder (its `.gltf`/`.glb`/`.j3mesh` files, not recursive) or a file name with `*` wildcards. It places only when exactly one model was imported.

## Commands you will need

From `j3c/`. `cargo build` and `cargo test` copy the binary to `~/.bin/j3c`. That is expected. Leave it alone. Never run the editor or a game window. `render` uses SDL's offscreen driver and is fine under `timeout`.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| JSON | `cargo test lsp::json` | exit 0 |
| Rust PNG | `cargo test png::` | exit 0 |
| glTF | `cargo test gltf::` | exit 0 |
| MCP | `cargo test mcp::` | exit 0 |
| J3 PNG check | `cargo test png_check_j3_runs` | exit 0 |
| Scene check | `cargo test scene3d_check_j3_runs` | exit 0 |
| Stdlib typechecks | `cargo test stdlib_game_j3_typechecks` | exit 0 |
| Full | `cargo test` | exit 0 |
| Fixture inspection | `python3 ../plans/scratch_inspect.py` (see Step 2; delete afterwards) | prints color type, block type, filters |

## Scope

**In scope**:
- `j3c/src/lsp/json.rs` (error position)
- `j3c/src/gltf.rs` (file-name prefix, bake)
- `j3c/src/png.rs` (`decode`, full `inflate`)
- `j3c/src/editor.rs` (`import_asset` gains `bake: bool`, and its one caller at ~1314)
- `j3c/src/mcp.rs` (`import`: many files, `bake`; `check`: JPEG warning; tool schema text)
- `j3c/src/preflight.rs` (stop probing SDL2_image for image loads)
- `j3c/src/compile.rs` (one test: `png_check_j3_runs`)
- `j3c/stdlib/j3/gfx/canvas.j3` (`decodePng`, `loadPng`)
- `j3c/stdlib/j3/game.j3`, `game_headless.j3` (fallbacks and a one-time warning)
- `j3c/stdlib/j3/scene3d.j3` (`tint`)
- `j3c/editor/src/tool.j3`, `history.j3`, `overrides.j3`, `ui_inspector.j3` (`tint`)
- `j3c/src/scene.rs` USAGE text (`tint`)
- New: `j3c/examples/png_check.j3`, `j3c/examples/png_check/*.png|*.rgba` fixtures
- `j3c/examples/scene3d_check.j3` (tint assertions)
- `EDITOR.md` line 65 only ("SDL2_image is needed for imported PNG/JPEG textures" becomes "for JPEG textures")

**Out of scope**:
- JPEG decoding, 16-bit or interlaced PNG
- Hue shift and color remap (see Design decisions)
- The editor's own Import dialog getting a bake option (MCP only for now)
- Deleting the now-unneeded `_image<i>.png` after a bake (kept; small)
- EDITOR.md "MCP Fixes" list edits (reviewer)
- Plan 004's files beyond the `mcp.rs` import tool and schema

## Git workflow

Branch `plan-001-import-pipeline` from the current tree. Never commit to `main`. One commit per phase, for example `Give JSON errors a byte offset and file name.`, `Decode PNG in Rust and bake textures at import.`, `Import many models per call.`, `Decode PNG in J3 when SDL2_image is missing.`, `Tint meshes and placed scenes.` Do not push.

## Steps

### Phase A: JSON errors (#3)

#### Step 1: Position and context in `json::parse`, file name in gltf

In `json.rs` `parse`, keep the parser alive after the error and decorate it once, so every error site benefits:

```rust
pub fn parse(src: &str) -> Result<Json, String> {
    let mut p = Parser { b: src.as_bytes(), i: 0 };
    let r = p.value().and_then(|v| {
        p.skip_ws();
        if p.i != p.b.len() { Err("trailing JSON".to_string()) } else { Ok(v) }
    });
    r.map_err(|e| format!("{e} at byte {} near `{}`", p.i, near(p.b, p.i)))
}
```

`near(b, i)`: bytes `i.saturating_sub(20)..min(i + 20, len)`, `String::from_utf8_lossy`, and `\n`/`\r`/`\t` turned into spaces. If the borrow checker objects to the closure, write it as two statements. Behavior must be identical.

In `gltf.rs` `read_source` (line ~78), map both `json::parse` errors to `format!("{}: {e}", path.display())`.

Add to `json.rs` tests: `parse("[1, 2,, 3]")` is `Err` containing `"at byte 6"` and `"near `"`.

**Verify**: `cargo test lsp::json` and `cargo test gltf::` pass. Also `cargo test mcp::`, because `handshake_and_tool_list` sends `{not json` and only checks the error code, so it must still pass.

### Phase B: Rust decoder, bake, many-file import (#5, #4)

#### Step 2: Fixtures

Create `j3c/examples/png_check/` with PNG and raw-RGBA pairs made by Pillow (installed: `python3 -c "import PIL"` works). Write this script to a scratch file, not into the repo, and run it from the repo root:

```python
from PIL import Image
import random
d = "j3c/examples/png_check/"
random.seed(7)
rgb = Image.new("RGB", (37, 23))
rgb.putdata([((x * 7) % 256, (y * 11) % 256, random.randrange(256)) for y in range(23) for x in range(37)])
rgb.save(d + "rgb.png", optimize=True)
pal = Image.new("P", (19, 17))
pal.putpalette([v for i in range(16) for v in (i * 16, 255 - i * 16, (i * 40) % 256)])
pal.putdata([(x + y) % 16 for y in range(17) for x in range(19)])
pal.info["transparency"] = 3
pal.save(d + "pal.png", transparency=3)
import shutil; shutil.copy("examples/dungeon/assets/tiles.png", d + "tiles.png")
for name in ["rgb", "pal", "tiles"]:
    open(d + name + ".rgba", "wb").write(Image.open(d + name + ".png").convert("RGBA").tobytes())
```

Then inspect the fixtures with a second throwaway script (zlib + struct). For each PNG, print the color type, the first DEFLATE block type (`(z[2] >> 1) & 3`), and the set of row filter bytes. Expected:

- `tiles.png`: ctype 6, block type 1 (fixed Huffman)
- `rgb.png`: ctype 2, block type 2 (dynamic), and several filter types
- `pal.png`: ctype 3

If `rgb.png` has no rows with filter 3 or 4, re-save with `Image.save(..., compress_level=9)` and check again. If Pillow still won't produce them, note it in the commit message and continue. Paeth and Average are still covered by `cart_image0.png` in the manual check in Step 9.

**Verify**: 6 files exist in `j3c/examples/png_check/`. `tiles.rgba` is 4096 bytes (64×16×4).

#### Step 3: `png::decode` in Rust

In `j3c/src/png.rs` add:

- `fn inflate(z: &[u8]) -> Option<Vec<u8>>`: raw DEFLATE (the caller skips the 2-byte zlib header), with stored, fixed, and dynamic blocks, puff-style. Replace `inflate_stored` with it in `compact`, and delete `inflate_stored`. `compact` still returns `None` for non-canvas PNGs because `refilter` rejects rows whose filter byte isn't 0.
- `pub fn decode(png: &[u8]) -> Option<(usize, usize, Vec<u8>)>`: RGBA8. It walks chunks the way `compact` does (IHDR, PLTE, tRNS, IDAT concatenated) and accepts only depth 8, interlace 0, and color types 0/2/3/4/6. It checks `raw.len() == h * (row + 1)`, unfilters filter types 0–4 (Paeth per the spec) with `bpp = channels`, then expands to RGBA. A palette index past PLTE gives `None`. `tRNS` for type 3 sets alpha per index. `tRNS` for types 0 and 2 is ignored (`// ponytail:` keyed-color transparency is ignored).

Test `decode_matches_fixtures` in `png.rs`: for `rgb`, `pal`, and `tiles`, `decode(include_bytes!("../examples/png_check/<n>.png"))` gives the width, height, and bytes equal to `include_bytes!("../examples/png_check/<n>.rgba")`. Also build a 3×2 stored PNG by hand (or reuse the test helper pattern already in this file, if one exists), then check `decode(&compact(&p).unwrap())` gives back the same pixels. That covers fixed Huffman plus Sub/Up.

**Verify**: `cargo test png::` exit 0.

#### Step 4: `bake` in gltf import

- `gltf::import(project, scene, source, bake: bool)` passes it into `Ctx` as `bake: bool`, plus `baked: HashMap<String, Option<(usize, usize, Vec<u8>)>>` so each image is read and decoded once.
- In `convert_primitive`, when `ctx.bake && textured`, look up the texture (`fs::read(ctx.project.join(&texture))` → `crate::png::decode`).
  - On `Some`, write `# j3mesh 1` rows with no UVs. Every vertex of a triangle gets `color * texel` at the triangle's UV centroid: `u = cu - cu.floor()` and the same for `v`, then `x = min((u * w) as usize, w - 1)`, `y = min((v * h) as usize, h - 1)` (glTF V points down the image, so there is no flip), and texel RGB is divided by 255. Return `Look` with `texture: String::new()`.
  - On `None` (JPEG or an unsupported PNG), `warn("bake needs 8-bit PNG textures; some kept their texture")` and write the textured mesh as today.
- `editor::import_asset(root, scene, raw, bake: bool)` passes it to `gltf::import`. `import_mesh` ignores it. The host caller at `editor.rs:~1314` passes `false`.

Test in `gltf.rs` tests, `bake_samples_texture_at_centroid`:
1. Copy `examples/png_check/pal.png` into the temp project beside the `.gltf`.
2. Build a one-triangle glTF like the existing `gltf()` helper, plus a `TEXCOORD_0` accessor (3 × VEC2 float) whose centroid falls in a known pixel, and `"uri":"pal.png"`.
3. Import with `bake = true`.
4. Assert the written `.j3mesh` starts with `# j3mesh 1`, its first row's `r g b` equal that pixel / 255 (to 1e-3, taking the value from `pal.rgba`), and the scene fragment has `texture s=` empty.

**Verify**: `cargo test gltf::` exit 0.

#### Step 5: MCP `import` takes many files and `bake`

Rewrite `Server::import` (`mcp.rs:419`):

1. Collect sources. Take each string of `paths` (array) and `path` (string). For each, resolve it relative to `root` unless absolute:
   - a directory gives its `.gltf`/`.glb`/`.j3mesh` files, sorted, not recursive;
   - a file name containing `*` gives the matching files in its parent directory (write a 10-line `fn wild(pat: &str, name: &str) -> bool` with `*` only);
   - else the path itself.

   No sources is an error, `"no .gltf, .glb, or .j3mesh files matched"`. More than 500 is an error.
2. For each source `i`: use fragment `target/j3-editor/mcp-import-{pid}-{i}.j3scene`, then `import_asset(root, &fragment, src, bake)` and `keep_import`. On success, the line is `imported <src> as <file>`, followed by `  warning: ...` lines. On error, the line is `failed <src>: <e>`. Continue after failures, and remove a failed fragment if it exists.
3. `generate_assets_catalog` once at the end.
4. Place only when there is exactly one source, it succeeded, and `place` is not false. Keep the current placing code.
5. The answer is all lines. `error: true` when any failed.

Schema at `mcp.rs:858`: drop `"required":["path"]`, add `"paths":{"type":"array","items":{"type":"string"}}`, and add `"bake":{"type":"boolean","default":false,"description":"Sample each textured triangle's texture at its UV center into vertex colors and drop the texture (for palette kits like Kenney's)."}`. The description says that `path` may be a folder or contain `*`, that one line comes back per model, and that only a single model is placed.

Test: extend `values_become_request_text` or add `wildcard_matches`: `wild("wall*.glb", "wall-door.glb")` is true, `wild("wall*.glb", "roof.glb")` is false, and `wild("*", "x")` is true.

**Verify**: `cargo test mcp::` exit 0. The `handshake_and_tool_list` test still lists the same `TOOL_NAMES`.

### Phase C: PNG in J3 when SDL2_image is missing (#2)

#### Step 6: `canvas.decodePng` / `canvas.loadPng`

Append to `j3c/stdlib/j3/gfx/canvas.j3`, keeping its spacing style. Follow the same algorithm as Step 3's Rust code, so the two read alike:

```j3
/// Decode an 8-bit, non-interlaced PNG (gray, RGB, palette, gray+alpha, RGBA) to RGBA8.
/// Returns a canvas with `w == 0` when the bytes are not one of those.
pub fn decodePng(bytes :[] u8, arena : &Arena) : Canvas
/// `decodePng` of a file; `w == 0` when it is missing or unsupported.
pub fn loadPng(path : string, arena : &Arena) : Canvas
```

- Failure value: `Canvas { pixels : arena.alloc::< u8 >(0), w : 0, h : 0, mode : Mode.Pixel }`.
- Success: `Canvas.create(arena, w, h, Mode.Pixel)` then fill `pixels`. `Pixel` means `textureFromCanvas` uses nearest filtering, which matches SDL's default scale quality for `IMG_Load` textures.
- Big-endian u32 read: `b0 * 16777216 + b1 * 65536 + b2 * 256 + b3`, as `int`.
- Bit reader without bit operators. It is a struct `{ src; at; buf : int; cnt : int; bad : bool }` with a `#[singleton] let POW2 :[33] int` table:

  ```text
  need(n): while cnt < n { if at >= src.len { bad = true; return 0; } buf += src[at] * POW2[cnt]; at += 1; cnt += 8; }
           v = buf - (buf / POW2[n]) * POW2[n]; buf = buf / POW2[n]; cnt -= n; return v
  ```

  Stored blocks: set `buf = 0; cnt = 0` (drop to the byte boundary), then read LEN/NLEN as bytes.
- Huffman, as in puff: `count[16]`, `symbol[288]`. Decode loop: `code = code + need(1); count = h.count[len]; if code - count < first { return symbol[index + code - first]; } index += count; first = (first + count) * 2; code = code * 2;` for `len` 1..15.
- Length and distance base/extra tables are `#[singleton]` arrays (the RFC 1951 tables, as in puff).
- Output goes into one `[]u8` of exactly `h * (row + 1)` bytes. Any write past its end sets `bad` and returns a failure. Back-references copy byte by byte (they may overlap).
- Unfilter per row with `bpp = channels`. Sums wrap: `v = raw + pred; if v >= 256 { v -= 256; }`. Paeth uses `abs` written out with `if`.
- Expand to RGBA per color type, using `PLTE`, and `tRNS` for type 3 (default alpha 255).
- `// ponytail:` CRC and Adler are not checked, and 16-bit/interlaced/low-bit-depth PNGs return `w == 0`.

New `j3c/examples/png_check.j3` (module `png_check`, pattern of `scene3d_check.j3`). For `rgb`, `pal`, and `tiles` in `examples/png_check/`, `canvas.loadPng` must equal the `.rgba` bytes read with `j3.io.readFile`, and `panic` with the name on mismatch. The paths are relative to `j3c/`, which is the cwd of `cargo test`. Then round-trip: draw on a 5×3 `Canvas`, `savePng` to `/tmp/j3_png_check.png` (stored blocks), `loadPng`, and compare. `loadPng("/nonexistent.png")` has `w == 0`. Print `ok`.

In `j3c/src/compile.rs` tests, add `png_check_j3_runs`, a copy of `scene3d_check_j3_runs` (line ~900) with the path swapped.

**Verify**: `cargo test png_check_j3_runs` exit 0, stdout `ok`.

#### Step 7: Use it in `game.loadRgba`, the texture loaders, and headless

In `game.j3`:

- `loadRgba` (1965): when `!ensureImage()` or `IMG_Load` returns 0, try `canvas.loadPng(path, arena)`. When it has `w > 0`, return `Rgba { pixels: c.pixels, w: c.w, h: c.h }`. Otherwise call `warnNoImage(path)` and return empty.
- `tryLoadTextureC` (1998): same fallback. Build the path string from the cstr with the existing `cstrBytes` helper, or change the fallback to live in `tryLoadTexture(path: string)` if that's simpler. `loadPng` then goes to `textureFromCanvas`.
- `loadTexture` (1979): when `!ensureImage()`, try the same, and panic with the soname only when `loadPng` fails.
- `warnNoImage(path)`: prints once per process, via a `bool` on `Platform` (line 646) or a `#[singleton]`: `SDL2_image is not installed: <path> did not load (PNG loads without it; JPEG needs it)`. Use whatever print function `game.j3` already uses. If it has none, `use j3.io.printLn;` (check there is no import cycle with `cargo test stdlib_game_j3_typechecks`).

In `game_headless.j3:480`, `loadRgba` returns `canvas.loadPng` as `Rgba` (same conversion). Leave the headless texture functions alone.

In `preflight.rs` around line 114, skip the library probe for image APIs: `if probe_libs && api.link_name != "SDL2_image" && !host_lib_present(...)`. Keep the file-exists check below it.

In `mcp.rs` `check` (line 501), before building `head`, add a warning line when `!crate::emit::host::host_lib_present("SDL2_image", &[])` and any file under `assets/` ends in `.jpg`/`.jpeg`: `warning: SDL2_image is not installed, so JPEG textures show white (PNG works)`. Count it as a warning, not an error.

Change `EDITOR.md` line 65 to say SDL2_image is needed only for JPEG textures.

**Verify**:
- `cargo test stdlib_game_j3_typechecks` and `cargo test png_check_j3_runs` exit 0.
- `cargo test` exit 0.
- `ls /usr/lib/x86_64-linux-gnu/libSDL2_image*` shows nothing on this machine (that is the case under test). If it exists, say so in the report and still continue.

#### Step 8: See a texture render without SDL2_image

After `cargo build --release` (or the `cargo test` copy), in a scratch dir:

```sh
mkdir -p $S/p/scenes $S/p/assets && printf '[package]\nname="p"\n' > $S/p/j3.toml
cp examples/editor_village/assets/imported/cart_image0.png $S/p/assets/t.png   # dynamic Huffman, Sub/Up/Paeth rows
cd $S/p && timeout 120 j3c -e scene --scene scenes/main.j3scene add cube name=Box texture=assets/t.png
timeout 120 j3c -e render --scene scenes/main.j3scene --focus Box --out target/r.png
```

Open `target/r.png` with the Read tool.

**Verify**: the cube shows the Kenney colormap's colored squares, not flat white. Delete `$S/p`.

### Phase D: Tint (#6)

#### Step 9: `tint` on nodes

In `scene3d.j3`:

- `Node` gets `tint: Vec3;` with a doc comment ("`MESH`/`INSTANCE` color multiplier, 1 = unchanged; on an instance it multiplies everything it places"). `blank` sets it to `Vec3.xyz(1, 1, 1)`. Fix every other `Node { ... }` literal the compiler reports.
- `setField` handles `tr`, `tg`, and `tb`.
- `write` (around the LIGHT block at ~1106): for `MESH` or `INSTANCE` whose tint isn't exactly `1,1,1`, write `tr tg tb` with `lineFloat`.
- `graft` (1495): after `n.owner = owner;`, when `host >= 0`, set `n.tint` to the componentwise product of `n.tint` and `scene.nodes[host].tint`. Nested instances then compose, because the inner graft's host is the already multiplied copy. Overrides run after grafting, so an overridden placed `tr` replaces the product. That is acceptable, because the editor writes the value it shows.
- `fn tinted(s: Surface, n: Node): Surface` returns the surface with `tint` multiplied componentwise by `n.tint`. Use it in `look` (both branches), in `drawNode`'s `gfx.surface(s)`, and in `buildTrace` after `look` is chosen. Add `n.tint.x * 29.3 + n.tint.y * 31.7 + n.tint.z * 37.1` to `traceSignature` for mesh nodes.

Editor:

- `tool.j3` `setKey`: add `tint` (`parseVec(value, &v, true)`, so one number sets all three), valid on `MESH` and `INSTANCE`, else `bad(out, key, "tint is for meshes and placed scenes")`. Add `tint` to the list of keys in the error text around line 865. `get` prints `fieldVec(out, "tint", n.tint)` for `MESH` and `INSTANCE`.
- `history.j3:125` `sameNode`: add `&& sameVec(a.tint, b.tint)`.
- `overrides.j3` near line 93: when `!sameVec(n.tint, b.tint)`, `overrideFloat` `tr`, `tg`, `tb`.
- `ui_inspector.j3`: for `MESH` and `INSTANCE`, `caption(ui, "Tint (1 is unchanged; above 1 brightens)")` and `colorRow(ui, <unused slot id>, &n.tint)` → `modified(w)`. For an `INSTANCE`, also `scene3d.refresh(&w.scene, w.path)` so the placed copies re-multiply. Pick a slot id `colorRow` isn't using: grep `colorRow(ui,`.
- `mcp.rs` `scene_set` description and `scene.rs` USAGE line 21: add `tint [r,g,b] (meshes and placed scenes; 1 unchanged)`.

`scene3d_check.j3`, before `printLn("ok")`: a part scene with a mesh at tint `0.5,1,1`, stored and placed by an instance with tint `1,2,1`. After `load`, the placed mesh's tint is `0.5,2,1`. A mesh with tint `1,1,1` writes no `tr` line (read the stored file with `io.readFile` and search it with a byte loop or the `contains` helper pattern from `tool.j3`).

**Verify**: `cargo test scene3d_check_j3_runs` and `cargo test` exit 0. Then check a render in a scratch project:

```sh
timeout 120 j3c -e scene add cube name=A tint=0.3,1,1.8
timeout 120 j3c -e render --focus A --out target/t.png
```

The cube shows teal.

## Test plan

- `json.rs`: an error message carries the byte offset and nearby text.
- `png.rs` `decode_matches_fixtures`: fixed and dynamic Huffman, palette with `tRNS`, RGB, and a `compact` round trip.
- `gltf.rs` `bake_samples_texture_at_centroid`.
- `mcp.rs` wildcard matching.
- `png_check.j3` via `png_check_j3_runs`: the same fixtures through the J3 decoder, plus a stored-block round trip and a missing file.
- `scene3d_check.j3`: tint store/load/graft.
- Manual: the Step 8 and Step 9 renders.

## Done criteria

- [ ] `cargo test` exits 0
- [ ] `cargo test png_check_j3_runs`, `scene3d_check_j3_runs`, `png::`, `gltf::`, `mcp::`, `lsp::json` each exit 0
- [ ] Step 8 render shows the texture with no SDL2_image installed
- [ ] MCP `import` with a folder imports every model in it and returns one line per model
- [ ] `bake: true` writes `# j3mesh 1` meshes and empty `texture s=`
- [ ] `scene_set` with `tint` changes a mesh's color in `render`
- [ ] No new crates. `Cargo.toml` unchanged
- [ ] `plans/README.md` not modified by you

## STOP conditions

- A "Current state" excerpt doesn't match the live file in substance.
- J3 turns out to have `%` or bit operators after all (someone landed EDITOR.md item 13). Don't STOP: use them and say so in the report.
- `png_check.j3` passes for `tiles` but fails for `rgb` after two honest debugging passes on the Huffman code. STOP and report which block/row fails. Do not paper over it with SDL.
- Adding `tint` to `Node` breaks loading an existing scene in `examples/editor_village/scenes/` (`j3c -e scene --project examples/editor_village check --all` reports new errors). STOP.
- `use j3.io` in `game.j3` creates an import cycle and there's no existing print helper. STOP and report (don't restructure modules).
- The live-editor path for tint needs more than `refresh` on instance change (placed copies don't update). Report it and leave the inspector part out rather than reworking `expand`.

## Maintenance notes

- `png.rs::decode` and `canvas.decodePng` implement the same algorithm. A fix in one belongs in the other, and both are locked to the same fixtures.
- When `%`/bit operators land in J3, the bit reader's divide-by-`POW2` can become shifts. Behavior must not change.
- `tint` is a new saved field (`tr tg tb`). Anything that copies or diffs `Node` fields (history, overrides, clipboard paste in `history.j3:389`) must include it. Reviewers: grep for `sameVec(a.color` to find the siblings.
- `bake` leaves the original `_image<i>.png` in `assets/imported/`. Delete it by hand if it's unwanted.
