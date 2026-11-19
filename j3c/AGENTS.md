# j3c/: the stdlib and the editor program

The compiler is J3 now, in `../j3c-bootstrap/` (plan 11 C6; the Rust crate that lived in `src/` is at the
git tag `rust-final`). Its map, the language contract and the "do not build" list are in
`../j3c-bootstrap/AGENTS.md`. This folder keeps what the compiler embeds and runs:

- `stdlib/j3/`: the stdlib in J3, embedded in `j3c` (rebuild with `./build.sh` after an edit; `J3_STDLIB`
  replaces the embedded copy). Tests: `stdlib/tests/` (`tools/ci/stdlib-tests.sh`).
- `editor/`: the scene editor program, in J3 (`editor/README.md`); `j3c` builds it per version into
  `~/.cache/j3/editor-boot`, `J3_EDITOR_SRC=$PWD/j3c/editor/src` builds it from disk.
- `tests/`: J3 fixture programs and projects (`tests/programs/`, `tests/fixtures/`). `bench/`: benchmark projects.

## Stdlib

`j3c run` parses `sys` + `io` + the entry file. `j3.sys.start` calls user `main` then `exit(0)`.

Scenes: a `.j3scene` file is one line per object, rotation in degrees (format: `editor/README.md`, "Scene file
format"). `scene3d` (`parse`, `write`, `nodeText`) is the reader and writer; `../j3c-bootstrap/src/load/scene_parse.j3`
is a hand copy of the reader (so `j3c` does not link SDL2) and `../tools/j3scene_migrate.py` converts v1 files,
so a rule changed in one is changed in all three. Old files (`node.<i>.*`, radians) still load.

Optional `j3.gfx.urp.raytrace` is a fullscreen sphere/room path tracer with a
custom-shader hook (its README has a sample). Optional
`j3.gfx.urp.forest` routes `j3.gfx` camera color and depth through an external
framebuffer, samples the same alpha-cutout canopy shadow map for volumetric
light, and composites analytic glass and metal spheres.
Both need a GL context from `App.useGl()` and release GL resources before that
context closes. The URP entry points are in `stdlib/j3/gfx/urp/README.md`.

Scene editing that only the editor does lives in `stdlib/j3/scene3d/editor/` (`j3.scene3d.editor.overrides`, `props`, `paste`, `uses`): per-copy overrides, declaring and renaming props, pasting placed objects, and the placement loop check. Games load, draw, and read props with `j3.scene3d` alone.

Procedural 2D art lives in `stdlib/j3/gfx/canvas.j3`: RGBA8 `Canvas`, pixel/smooth modes, clipped primitives, seeded noise, and PNG export. `j3.game.textureFromCanvas` uploads it for SDL or GL; `updateTextureFromCanvas` updates a rectangle. `j3c doc recipes canvas` generates assets without SDL. Keep its hot drawing loops allocation-free and clip before iterating. The scaffolded `AGENTS.md` teaches agents to use the API and compose more readable game art.


| Path | What it is |
|------|------------|
| `stdlib/j3/` | Stdlib in J3. SSOT. Embedded in the `j3c` binary; optional OpenGL passes are in `urp/`. |
| `stdlib/j3/j2.j3`, `j2/*.j3` | `j3.j2`: the j2 runtime script language (`j3c doc j2`). One module over flat parts: `value` `heap` `str` `table` `array` `fmt`, compiler `code` `compile*`, VM `vm*`, `gc`, host API `host` `host_types` `load`, built-ins `lib*`; `j2/lex.j3` and `j2/parse.j3` (+ `parse/`) are their own modules `j3.j2.lex`/`j3.j2.parse`. Script input never panics (`Result<_, Error>`). Tests `stdlib/tests/j2_*_test.j3`, `j2_bench.j3` |
| `stdlib/j3/cmp.j3` | `Eq`, `Ord: Eq`, `Hash`, `Clone` traits, the primitive hashes, `string` `operator<`, `sort`/`sortBy` (stable merge sort, scratch-arena buffer); `data/map.j3` is `Map<K: Hash + Eq, V>` on top (`getPtr` `getOrPut` `each` `entries` `sortedKeys`), `data/set.j3` `Set<T>`, `data/pool.j3` `Pool<T>` + typed `Id<T>`, `data/intern.j3` `Interner`. `memory.j3` declares a private `Eq` (j3.cmp imports j3.memory) for `List.indexOf where T: Eq` |
| `stdlib/j3/io.j3`, `io/*.j3`, `sys/fs.j3` | `j3.io`: whole-file read/write and `listDir` (`io.j3`), `fs.j3` (`AtomicFile`/`writeFileAtomic`, `tempFile`/`tempDir`, `lockFile`/`tryLockFile` over `flock`, `copyFile`, `walk`), `stdio.j3` (buffered `Reader`/`Writer`, `stdin()`/`stdout()`, `readLine`, `readExact`); `io/path.j3` is its own module `j3.io.path` (`join` `normalize` `glob` `realpath` ...); `io/watch.j3` is `j3.io.watch`. The syscalls are the `sys/fs.j3` part of `j3.sys` (Linux); `sys_windows/fs.j3` has `rmdir`, `cwd`, `realpathTo` and ENOSYS stubs for the rest, `sys_macos` stubs all but `cwd`. `proc.waitAny` waits on one pidfd per child |
| `stdlib/j3/text.j3`, `text/*.j3` | `j3.text`: `Text` and its pushes (`fmt.j3`), string helpers and the `Split`/`Lines` cursors (`str.j3`), UTF-8 `decodeRune`/`isValidUtf8`/`Runes` (`utf8.j3`), `parseInt`/`parseU64`/`parseF64` (`parse.j3`: Eisel-Lemire, exact fallback in `decimal.j3`), shortest round-trip `writeF64`/`writeF32` (`ryu.j3`, Ryu over the one generated 10^e table `pow10.j3`). Tests `stdlib/tests/text_utils_test.j3` (1M random f64 round-trips) |
| `stdlib/j3/hash.j3`, `hash/{sha256,xxh3}.j3` | `j3.hash`: stable content hashes `fnv1a64`, `xxh3_64` (seed 0, equals xxHash 0.8 `XXH3_64bits`), `sha256` (`Sha256Sum.hex`), each with an incremental hasher (`Fnv1a64`, `Xxh3`, `Sha256`: `update`/`finish`). Tests `stdlib/tests/hash_test.j3` |
| `stdlib/j3/gpu.j3` | `Source` (GLSL text, uniforms, vertex attribs) and `Rgba8` for `@shader` consts |
| `stdlib/j3/game/p2shader.j3` | The p2 sprite shader written as `#[shader]` fns (`p2Vs`, `p2Fs`) |
