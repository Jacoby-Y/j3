# Plan 012: Online asset browser (Poly Haven, ambientCG, Kenney)

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report — do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: `git rev-parse --short HEAD` should be
> `b086a5b`. The working tree has a lot of uncommitted and untracked work on
> top of it (the editor, `mcp.rs`, `gltf.rs`, and others are untracked). That
> is expected. Treat the files as they are now as the base, and check the
> "Current state" excerpts against them. If an excerpt no longer matches,
> STOP.

## Status

- **Priority**: P2
- **Effort**: L (Phase 1: M, Phase 2: M, Phase 3: S)
- **Risk**: MED (depends on third-party HTTP APIs and on `curl`/`unzip` being installed)
- **Depends on**: Plan 001 (import and bake) only for textures. Imported PNG/JPEG textures and ambientCG materials load through `game.loadRgba`, which needs SDL2_image unless plan 001 has landed. Phase 1 works without plan 001. Textures fall back to warnings.
- **Category**: feature
- **Planned at**: commit `b086a5b` + uncommitted tree, 2026-09-25

## Why this matters

The village sample was built from Kenney's Fantasy Town and Nature kits. The
agent fetched them by hand:

1. `curl` the kenney.nl page.
2. grep the HTML for the zip URL.
3. `unzip`.
4. Call MCP `import` once per `.glb`.

That took several turns, and nothing recorded the license. `EDITOR.md` ("Still
left" > "Online asset browser") plans a **Get assets** tab for Poly Haven,
ambientCG, and Kenney. It uses the host's `curl` and the existing JSON parser,
caches in `~/.cache/j3/data/assets/`, and hands files to the existing importer.

This plan builds that in three phases:

1. Host fetch and cache, plus MCP tools. No UI yet, so agents can use it right away.
2. The editor tab.
3. Importer extras: sky HDRIs, and explicit warnings for normal/roughness maps.

## Current state

All paths are relative to the repo root, `/home/jacoby/Documents/code/ai-stuff/j3`.

**No crates.** `j3c/Cargo.toml` says `# No crates.io deps.` The JSON parser is
`j3c/src/lsp/json.rs`. `pub enum Json { Null, Bool, Num(f64), Str, Arr(Vec<Json>), Obj(Vec<(String, Json)>) }`
is at lines 4–11, `Json::obj`/`get`/`as_str`/`as_f64`/`as_i64`/`stringify` at
13–48, and `pub fn parse(src: &str) -> Result<Json, String>` at line 50.

**Cache dir.** `j3c/src/editor.rs:1004-1011`:

```rust
/// `~/.cache/j3/editor`: editor builds, and which project the editor last opened.
pub(crate) fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .map(|c| c.join("j3/editor"))
}
```

**Subprocess precedent.** The zenity picker is at `j3c/src/editor.rs:1209-1224`.
It runs in a thread spawned from the host loop (`1277-1279`) and reports back
through `event(root, kind, scene, message)` (`850-854`), which appends to
`target/j3-editor/events`.

**Import.** `j3c/src/editor.rs:891-907` `import_asset(root, scene_fragment, raw_path)`
takes `.gltf`/`.glb`/`.j3mesh`, absolute or project-relative.
`keep_import` (`876-889`) moves the fragment to `assets/imported/<name>.j3scene`.
glTF images are copied into the project as `assets/imported/<prefix>_image<i>.<png|jpg>`
(`j3c/src/gltf.rs:354-415`), so importing straight from a cache directory is fine.

**Host loop.** `J3EDITOR ...` lines are handled in `launch` at
`j3c/src/editor.rs:1256-1338`. `J3EDITOR IMPORT <path>\t<tab>[\tscene]` is at
`1304-1333`. It emits an `import` event, or a `placed` event for reusable scenes.

**MCP.** `j3c/src/mcp.rs`:
- The tool dispatch is `Server::call` at `206-313`. It includes `"import" => self.import(&root, args)` and `"asset_search" => asset_search(&root, args)` (lines 300–301).
- `Server::import` (`419-441`) imports and then places the result with a `scene_request` `add instance`.
- `TOOL_NAMES` (`769-772`) must equal the tool list order. The test `handshake_and_tool_list` (around line 889) asserts `names == TOOL_NAMES`.
- `TOOLS` is a JSON string (`778-876`).
- `USAGE` (`22-33`) and `EDITOR.md` "Agent use" (line 57) list the tools by name.

**Editor frontend.**
- The left panel's asset section is `j3c/editor/src/ui_tree.j3:258-300` (`section(ui, "ASSETS")`, Import button, `Pick file...`, `Import as a reusable scene`, then a scroll list).
- Events are handled in `j3c/editor/src/host.j3` (`kind` dispatch around lines 55–110; `picked` at 92–96).
- Catalogs are `save`-format files that the host writes (`assets.count=N`, `assets.<i>.path s=...`, see `generate_assets_catalog` at `j3c/src/editor.rs:753-765`). The editor reads them in `j3c/editor/src/files.j3:254-264` (`reloadCatalogs`) and `21-40` (`readAssets`).
- World fields such as `importPath: Text` and `importAsScene: bool` are in `j3c/editor/src/types.j3` (lines 214 and 356).

**Materials.** A `.j3mat` is `save` format. Keys read in `fillMaterial`
(`j3c/stdlib/j3/scene3d.j3:2147-2183`) include `tint`, `shine f=`, `rough f=`,
`uv.scale.x f=`, and `texture s=<path>`, which is loaded by `game.loadRgba`
(SDL2_image). There are no normal-map keys, and the forward shader in
`j3c/stdlib/j3/gfx.j3` has no tangent space. The sky is a `builtin:sky` node
whose texture is an equirect image (`skyImage`, `scene3d.j3:3067`).

**glTF material.** `j3c/src/gltf.rs:418-466` reads `baseColorFactor`,
`baseColorTexture`, `alphaMode`, and `doubleSided`. It silently ignores
`normalTexture`, `metallicRoughnessTexture`, and `occlusionTexture`.

**The APIs, checked on 2026-09-25 with `curl`:**
- Poly Haven list: `GET https://api.polyhaven.com/assets?type=models` (also `textures`, `hdris`). The response is an object keyed by id. `"ArmChair_01": {"name":"Arm Chair 01","type":2,"tags":[...],"categories":[...],"authors":{"Kirill Sannikov":"All"},"thumbnail_url":"https://cdn.polyhaven.com/asset_img/thumbs/ArmChair_01.png?width=256&height=256&v=abbc4732",...}`. 521 models.
- Poly Haven files: `GET https://api.polyhaven.com/files/<id>`.
  - For a model: `.gltf["1k"].gltf = {"url": ".../ArmChair_01_1k.gltf", "include": {"ArmChair_01.bin": {"url": ...}, "textures/Armchair_01_diff_1k.jpg": {"url": ...}, ...}}`. Each include key is the relative path the `.gltf` expects.
  - For an HDRI: `.hdri["1k"].hdr.url` (about 1.4 MB `.hdr`). `.tonemapped.url` is a huge 8k JPG; do not use it.
- ambientCG: `GET https://ambientcg.com/api/v2/full_json?q=<q>&type=Material&limit=50&include=previewData,downloadData`.
  - Each `foundAssets[i]` has `assetId` and `displayName`, `previewImage["256-PNG"]` (thumbnail URL), and `downloadFolders.default.downloadFiletypeCategories.zip.downloads[]`, with `{attribute:"1K-JPG", downloadLink:"https://ambientcg.com/get?file=Ground111_1K-JPG.zip", size}`.
  - The zip holds `<id>_1K-JPG_Color.jpg`, `_NormalGL.jpg`, `_Roughness.jpg`, and so on.
- Kenney: no API. The page `https://kenney.nl/assets/<slug>` contains exactly one match of `https://kenney.nl/media/pages/assets/<slug>/<hash>/<file>.zip`. The hash changes when the pack updates. These slugs were verified: `fantasy-town-kit`, `nature-kit`, `castle-kit`, `furniture-kit`, `platformer-kit`, `city-kit-suburban`, `city-kit-commercial`, `survival-kit`, `pirate-kit`, `graveyard-kit`, `tower-defense-kit`, `car-kit`, `food-kit`, `mini-dungeon`, `space-kit`. The zips hold `Models/GLB format/*.glb` (town) or `Models/GLTF format/*.glb` (nature), `Previews/*.png`, and `License.txt` (CC0).

**Tools on this machine:** `/usr/bin/curl`, `/usr/bin/unzip`, `/usr/bin/tar`.

## Design

### Decisions

- **HTTP**: a `curl` subprocess, as `EDITOR.md` says.
  - Arguments: `curl -fsSL --proto =https --proto-redir =https --connect-timeout 15 --max-time 600 -A "j3c/<CARGO_PKG_VERSION> (+https://github.com/Jacoby-Y)" -o <dest>.part <url>`. On success, rename `.part` to `<dest>`.
  - Poly Haven asks API users to send a unique User-Agent.
  - Always use `Command` arguments, never a shell string. Ids and queries come from the network or the agent.
- **Unzip**: the `unzip` binary on Linux and macOS (`unzip -q -o <zip> -d <dir>`), and `tar -xf <zip> -C <dir>` on Windows (bsdtar ships in System32 since Windows 10 1803). Put this in one `extract(zip, dir)` function. A missing tool is an error that names it.
  - Why not a Rust zip reader: zips use dynamic-Huffman DEFLATE. `j3c/src/png.rs` has only stored and fixed-Huffman code. A real inflater is about 250 lines plus tests. The subprocess is about 10 lines and matches the curl/zenity pattern. Both `unzip` and bsdtar refuse `../` entries, so zip-slip is covered.
  - `ponytail: unzip/tar subprocess; write an inflater in png.rs if a platform lacks both.`
- **One new host module**: `j3c/src/online.rs`. Its pure parsing functions take `&str` and return plain structs, so tests run offline. Networking stays in two small functions, `download` and `extract`.
- **Cache**: `<cache root>/j3/data/assets/<source>/`.
  - `polyhaven/<type>.json` is the catalog list, refetched when older than 24 hours (file mtime).
  - `<source>/<id>/` holds the downloads, plus a `done` marker written last, so a half-finished download is retried.
  - ambientCG searches are not cached (one request per search). Kenney search is a const list, so it needs no network.
- **Ids are sanitized**: `[A-Za-z0-9_-]` only, 1–80 characters. Anything else is rejected before it becomes a path or URL.
- **Credits**: every fetch appends one line to `<project>/assets/CREDITS.txt`, unless a line starting with the same `<source>:<id>` already exists:
  `polyhaven:ArmChair_01  Arm Chair 01 — Kirill Sannikov — CC0 — https://polyhaven.com/a/ArmChair_01`
  Authors: Poly Haven uses the `authors` keys joined with `, `. ambientCG and Kenney use `ambientCG` and `Kenney`. The license is always `CC0` for these three sources.
- **What a fetch produces**:

  | source / kind | result |
  |---|---|
  | Poly Haven model | Download the 1k `.gltf` and every `include` file into `polyhaven/<id>/` (keeping their relative paths), then import like the `import` tool. |
  | Poly Haven HDRI | Phase 3: `.hdr` → tone-mapped PNG at `assets/skies/<id>.png`, for a `builtin:sky` node's texture. Before Phase 3, return the error "sky HDRIs are not supported yet". |
  | Poly Haven texture | Out of scope (use ambientCG for materials). Return an error that says so. |
  | ambientCG material | Download the `1K-JPG` zip and extract it. Copy `*_Color.jpg` to `assets/textures/<id>_color.jpg`. Write `assets/materials/<id>.j3mat`: `texture s=assets/textures/<id>_color.jpg`, `shine f=0.1`. Return the `.j3mat` path. |
  | Kenney pack, no `file` | Download and extract the zip. Return the pack's model files (`.glb`/`.gltf` under `Models/`; prefer the `GLB format` folder when both exist) as absolute cache paths, plus the preview folder. |
  | Kenney pack + `file` | As above, then import that one file (matched by exact file name, else by unique stem) like the `import` tool. |

- **MCP tools** (Phase 1). These are the agent's path, and they need no editor.
  - `asset_online_search {query, source?: any|polyhaven|ambientcg|kenney, kind?: any|model|material|sky}`. At most 40 results. Text lines are `source  id  kind  name  (author)`. Structured content is `{"assets":[{source,id,kind,name,author,thumb}]}`. Read-only.
  - `asset_fetch {source, id, file?, place?=true, scene?, name?, parent?, pos?}`. It returns what was written, any import warnings, and the credit line. When it imports a model, it reuses `Server::import`'s placing code. Split that method into `import_path(root, source_path, args)` and call it from both tools.
- **Editor tab** (Phase 2). A `GET ASSETS` section toggled by a button in the ASSETS section. It is not a new pane type: a source cycle button, a query field, and a result list.
  - The frontend prints `J3EDITOR FETCH <source>\t<query>`. The host thread writes `target/j3-editor/online` in `save` format (`online.count=N`, `online.<i>.id s=`, `.name s=`, `.kind s=`, `.source s=`, `.thumb s=<absolute cached PNG or empty>`). Then it emits event `online` with the message `N results`.
  - Clicking a row prints `J3EDITOR GET <source>\t<id>\t<tab>[\tscene]`. The host thread fetches, then imports through the same code as `J3EDITOR IMPORT`, emitting the same `import`/`placed`/`msg` events. For a Kenney pack with no `file`, the host writes the pack's models into the same `online` file (`kind s=model`, `id s=<slug>/<file>`) so the user drills in. Clicking one then fetches with `file`.
  - Thumbnails are downloaded by the host into `<source>/thumbs/<id>.png`. The editor draws them with `game.tryLoadTexture`. When that returns an empty handle (no SDL2_image), it shows names only.
- **Out of scope**: Sketchfab (it needs a token and has per-model licenses), normal maps in the renderer (needs tangents; record it under "Rendering gaps" in `EDITOR.md`), roughness *maps* (decoding JPEG in Rust with no crates is not worth it), Poly Haven textures, and resolutions other than 1k.

## Commands you will need

Run from `j3c/`. `cargo test` copies `j3c` to `/home/jacoby/.bin/j3c`. Leave that wrapper alone.

| Purpose | Command | Expected on success |
|---|---|---|
| Unit tests for the new module | `timeout 600 cargo test --lib online:: -- --test-threads=1` | exit 0 |
| MCP tests | `timeout 600 cargo test --lib mcp:: -- --test-threads=1` | exit 0 |
| Editor host tests | `timeout 600 cargo test --lib editor:: -- --test-threads=1` | exit 0 |
| Network smoke test (ignored by default) | `timeout 600 cargo test --lib online::tests::network -- --ignored --test-threads=1` | exit 0 (needs internet) |
| Full suite | `timeout 900 cargo test` | exit 0 |
| Frontend typecheck | `J3_EDITOR_SRC=editor/src timeout 300 cargo run -q -- -e scene --help` builds the frontend | usage text, no build error |
| Live MCP check | see Step 5 | JSON-RPC replies |

Never launch the GUI editor. `-e scene` / `-e render` run the frontend headless.

## Scope

**In scope**:
- `j3c/src/online.rs` (new)
- `j3c/src/lib.rs` (`pub mod online;`)
- `j3c/src/editor.rs`: `cache_root`, `FETCH`/`GET` handlers, and sharing the import path
- `j3c/src/mcp.rs`: two tools, the `import_path` split, and `USAGE`/`TOOLS`/`TOOL_NAMES`
- `j3c/src/gltf.rs`: Phase 3 warnings only
- `j3c/src/png.rs`: Phase 3 `pub fn encode_rgba` only
- `j3c/editor/src/{types,ui_tree,host,assets}.j3`: Phase 2
- Docs: `EDITOR.md` (Agent use tools list; mark the Online asset browser done or partly done; add "normal maps" under Rendering gaps), `j3c/editor/README.md` (Assets section), and a map row for `src/online.rs` in `j3c/AGENTS.md`

**Out of scope**:
- Changing how `game.loadRgba` loads images (plan 001)
- Renderer or shader work
- Sketchfab
- Any crates
- `plans/README.md`

## Git workflow

Branch `plan-012-online-asset-browser` off the current tree. Commit once per
phase. Do not push. Messages:
1. `Fetch Poly Haven, ambientCG, and Kenney assets from the host and MCP.`
2. `Add a Get assets section to the editor.`
3. `Import Poly Haven sky HDRIs and warn about PBR maps.`

## Steps

### Phase 1: host fetch, cache, MCP

#### Step 1: `cache_root`

In `j3c/src/editor.rs`:
1. Add `pub(crate) fn cache_root() -> Option<PathBuf>`, returning `<XDG_CACHE_HOME or $HOME/.cache>/j3`.
2. Make `cache_dir()` return `cache_root().map(|r| r.join("editor"))`.

Nothing else changes. (Plan 013 adds a Windows fallback inside `cache_root`.)

**Verify**: `timeout 600 cargo test --lib editor:: -- --test-threads=1` → exit 0.

#### Step 2: `online.rs` pure parts plus tests

Create `j3c/src/online.rs` with a module doc comment (one paragraph, no crates,
curl/unzip subprocesses). Contents:

```rust
pub(crate) struct Hit { pub source: &'static str, pub id: String, pub kind: &'static str, pub name: String, pub author: String, pub thumb: String }
pub(crate) fn safe_id(id: &str) -> Result<&str, String>              // [A-Za-z0-9_-]{1,80}
pub(crate) fn polyhaven_hits(list_json: &str, kind: &'static str, query: &str) -> Result<Vec<Hit>, String>
pub(crate) fn polyhaven_model_files(files_json: &str) -> Result<Vec<(String, String)>, String> // (relative path, url); the .gltf first
pub(crate) fn polyhaven_hdr_url(files_json: &str) -> Result<String, String>                    // Phase 3 uses it
pub(crate) fn ambientcg_hits(json: &str) -> Result<Vec<Hit>, String>
pub(crate) fn ambientcg_zip_url(json: &str, id: &str) -> Result<String, String>                // attribute "1K-JPG"
pub(crate) const KENNEY: &[(&str, &str)] = &[("fantasy-town-kit", "Fantasy Town Kit"), ...]   // the 15 slugs above
pub(crate) fn kenney_hits(query: &str) -> Vec<Hit>
pub(crate) fn kenney_zip_url(html: &str, slug: &str) -> Result<String, String>
pub(crate) fn url_encode(s: &str) -> String                                                    // RFC 3986 unreserved kept
pub(crate) fn credit(root: &Path, key: &str, line: &str) -> Result<(), String>
```

Rules:
- **Matching**: lowercase substring of `query` in the id, name, tags, or categories. An empty query matches everything. Sort by name, cap at 40.
- **`kenney_zip_url`**: find `https://kenney.nl/media/pages/assets/{slug}/`, take characters up to the first `"` or `'`, and require the result to end in `.zip`. Anything else is an error that names the slug.
- **Poly Haven include paths**: reject any key that contains `..`, starts with `/`, or contains `\`. The key becomes a path under the cache dir.
- **URLs**: every URL taken from JSON must start with `https://`.

Tests in `#[cfg(test)] mod tests`:
- Small inline JSON/HTML fixtures, cut down from the shapes in "Current state".
- `safe_id` rejects `../x`, `a/b`, and the empty string.
- `kenney_zip_url` on a fixture with the real Fantasy Town URL.
- A Poly Haven include key with `..` is rejected.
- `credit` writes once and skips the second call with the same key (use a temp dir, like `editor.rs` tests do).

**Verify**: `timeout 600 cargo test --lib online:: -- --test-threads=1` → exit 0 with at least 6 tests.

#### Step 3: `online.rs` network parts

Add these functions:
- `download(url, dest) -> Result<(), String>`: skip when `dest` exists; otherwise run curl as in Design. On a non-zero exit, return an error with curl's stderr (trimmed, at most 300 characters). "No such file" becomes `"curl not found; install curl"`.
- `extract(zip, dir)`: `unzip` or, on `cfg!(windows)`, `tar`, as in Design.
- `search(source, query, kind) -> Result<Vec<Hit>, String>`: `any` concatenates the sources and skips failing ones, returning an error only if all fail.
- `fetch(root, source, id, file) -> Result<Fetched, String>`, with `enum Fetched { Model(PathBuf), Material(String), Pack(Vec<PathBuf>), Sky(String) }`. Write the credit line here.
- `thumb(hit) -> Option<PathBuf>`: download the thumbnail to `<source>/thumbs/<id>.png` (Phase 2 uses it).

The `done` marker: write `<id>/done` after all files are in place, and treat the directory as cached only when `done` exists.

Add `#[test] #[ignore] fn network()`. Its body:
1. Kenney search `town` finds `fantasy-town-kit`.
2. `fetch` of `kenney` / `fantasy-town-kit` into a temp project returns at least 50 models.
3. Poly Haven search `chair` returns at least 1 hit.
4. `fetch` of `ArmChair_01` returns a `.gltf` path whose `.bin` exists next to it.
5. ambientCG search `brick` returns at least 1 hit.

Point `XDG_CACHE_HOME` at a temp dir for the test.

**Verify**:
- `timeout 600 cargo test --lib online:: -- --test-threads=1` → exit 0 (the network test stays ignored).
- `timeout 600 cargo test --lib online::tests::network -- --ignored --test-threads=1` → exit 0.

#### Step 4: MCP tools

In `j3c/src/mcp.rs`:
1. Split `Server::import` into `import(root, args)` → `import_path(root, &need_str(args,"path")?, args)`. Behavior stays the same.
2. Add `"asset_online_search" => online_search(args)` and `"asset_fetch" => self.fetch(&root, args)`.
3. `fetch` returns different text per result:
   - `Model(p)` → `self.import_path(root, p, args)`, with the credit line appended to the text.
   - `Material(rel)` → `"made {rel} (set material=<it> on a mesh)"`.
   - `Pack(files)` → the file names, one per line (at most 200), plus `call asset_fetch again with file=<name> to import one, or import with the full path`. Put the absolute paths in structured content.
4. Add both tools to `TOOLS` (write clear descriptions: sources, CC0, credits file, what `file` does) and to `TOOL_NAMES` after `asset_search`. Mark `asset_online_search` `readOnlyHint`.
5. Update `USAGE` and `INSTRUCTIONS`, one sentence: "Need a model or material the project lacks? asset_online_search, then asset_fetch."

**Verify**: `timeout 600 cargo test --lib mcp:: -- --test-threads=1` → exit 0. `handshake_and_tool_list` now sees 15 tools.

#### Step 5: Live check and docs

From `j3c/`, with a scratch project so the village stays untouched:

```sh
rm -rf /tmp/j3-fetch && timeout 60 cargo run -q -- new /tmp/j3-fetch >/dev/null
printf '%s\n' \
 '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}' \
 '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"asset_online_search","arguments":{"source":"kenney","query":"nature"}}}' \
 '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"asset_fetch","arguments":{"source":"kenney","id":"nature-kit","file":"tree_default.glb","place":false}}}' \
 | timeout 300 cargo run -q -- -e mcp --project /tmp/j3-fetch
```

If `j3c new` has a different signature, use whatever `j3c new --help` says and
note that in the report. If `tree_default.glb` is not in the pack, use any
name that id 3 without `file` lists.

**Expected**:
- id 2 lists `kenney nature-kit`.
- id 3 says `imported ... as assets/imported/tree_default.j3scene`.
- `cat /tmp/j3-fetch/assets/CREDITS.txt` shows one `kenney:nature-kit` line.

Docs:
- `EDITOR.md` Agent use: add the two tools to the list.
- `j3c/AGENTS.md`: add a map row: `src/online.rs` | Poly Haven / ambientCG / Kenney search and fetch through `curl` + `unzip`, cache in `~/.cache/j3/assets`, CREDITS.txt.

Commit Phase 1.

### Phase 2: editor section

#### Step 6: Host handlers

In `launch` (`j3c/src/editor.rs`), add two branches. Each spawns a thread like `PICK`:

- **`J3EDITOR FETCH <source>\t<query>`**:
  1. Run `online::search`.
  2. Download thumbnails (ignore failures).
  3. Write `target/j3-editor/online` in the `save` format from Design. Escape newlines and tabs out of names the same way `event` does.
  4. Emit `event(root, "online", "", "<N> results")`. On error, emit `event(root, "msg", "", e)`.
- **`J3EDITOR GET <source>\t<id>\t<tab>[\tscene]`**: run `online::fetch`.
  - `Model(p)`: follow exactly the same steps as the `IMPORT` branch. Extract that branch's body into `fn import_into(root, path, tab, as_scene, file_name)` and call it from both places. `import_count` becomes an `AtomicU64` or is passed in. Keep fragment names unique.
  - `Material(rel)`: emit `refresh` with the message `Made <rel>`.
  - `Pack(files)`: write the files into `online` as rows and emit `online`.

**Verify**: `timeout 600 cargo test --lib editor:: -- --test-threads=1` → exit 0.

#### Step 7: Frontend

1. **`types.j3`**: add these World fields:
   - `onlineOpen: bool`
   - `onlineSource: string` (`"any"`)
   - `onlineQuery: Text`
   - `online: Save`, plus its arena, following `w.assets`/`w.catMem`
   - `onlineThumbs` (a small array of texture handles, 40)

   Initialize them where `importPath` is initialized.
2. **`ui_tree.j3`**, after `check("Import as a reusable scene", ...)`: add a `button("Get assets...")` that toggles `w.onlineOpen`. When open, draw these instead of the local asset list:
   - A row with a source button that cycles `any → polyhaven → ambientcg → kenney` (use `nextOption` from `util.j3`), the `field("search", &w.onlineQuery)`, and `button("Search")`, which prints `J3EDITOR FETCH`.
   - A scroll list. Each row shows the thumbnail (when loaded), the name, and the kind. Clicking a row prints `J3EDITOR GET ...` with `w.path` and the `importAsScene` flag, then shows `say(w, "Downloading...")`.
   - Refuse in a material tab with the same message as `importFile`.
3. **`host.j3`**: on `kind == "online"`, reload `target/j3-editor/online` into `w.online` (like `reloadCatalogs`). Free and reload the thumbnails with `game.tryLoadTexture`. Show the message with `say`.
4. Build the commands with `Text` like `importFile` in `files.j3:239-252`.

**Verify**: `J3_EDITOR_SRC=editor/src timeout 300 cargo run -q -- -e scene --help` → usage text, no J3 compile error. Also run `J3_EDITOR_SRC=editor/src timeout 300 cargo run -q -- -e render --project ../examples/editor_village` (or whatever `-e render --help` says for picking a project). A PNG is written, which proves the frontend still runs. Do not open the GUI; the user tests the tab.

#### Step 8: Docs

`j3c/editor/README.md` Assets section: add a short **Get assets** paragraph (sources, CC0, CREDITS.txt, needs `curl` and `unzip`, thumbnails need SDL2_image). In `EDITOR.md`, move "Online asset browser" out of "Still left" down to the remaining items (Sketchfab, normal/roughness maps).

Commit Phase 2.

### Phase 3: importer extras

#### Step 9: Sky HDRIs

1. **`png.rs`**: add `pub fn encode_rgba(w, h, rgba: &[u8]) -> Vec<u8>`. It builds filter-0 rows, then uses the existing `deflate_fixed`, `adler32`, and `chunk`, like `compact` does.
2. **`online.rs`**: add `hdr_to_png(bytes) -> Result<Vec<u8>, String>` for Radiance `.hdr`:
   - The header ends at an empty line. Only `-Y h +X w` resolution lines are accepted.
   - Scanlines are new-style RLE or flat RGBE.
   - Convert with `v = mant * 2^(e-136)`, apply Reinhard `v/(1+v)`, raise to the power `1/2.2`, and store as u8 with alpha 255.
3. **`fetch`** for Poly Haven kind `sky`: download the 1k `.hdr`, write `assets/skies/<id>.png`, and return `Sky(rel)`.
   - MCP text: `made <rel>; add a builtin:sky mesh and set texture=<rel>`.
   - Editor: emit `refresh`.
4. **Tests**:
   - A hand-built flat 2×1 `.hdr` fixture decodes to the expected bytes.
   - One RLE scanline fixture (width ≥ 8) decodes the same as its flat form.
   - `encode_rgba` output starts with the PNG signature. Round-trip it through `png::compact` returning `None` (it is not a stored PNG), or decode it with the fixed-Huffman test helper already in `png.rs` tests.

**Verify**:
- `timeout 600 cargo test --lib online:: -- --test-threads=1` → exit 0.
- `timeout 600 cargo test --lib png:: -- --test-threads=1` → exit 0.
- Network: `asset_fetch {source:"polyhaven", id:"kloofendal_48d_partly_cloudy_puresky"}` on `/tmp/j3-fetch` writes a 1024×512 PNG (`file` reports the size).

#### Step 10: PBR map warnings

In `gltf.rs` `material`, call `warn(warnings, "normal, metal/roughness, and occlusion maps were ignored")` once when any of `normalTexture`, `pbrMetallicRoughness.metallicRoughnessTexture`, or `occlusionTexture` is present. Add a test next to the existing gltf tests, using a minimal glTF with a `normalTexture`, if such tests exist. Otherwise assert through `import` on a tiny inline `.gltf` written to a temp dir.

Add to `EDITOR.md` "Rendering gaps": "No normal or roughness maps (Poly Haven models look flat)".

**Verify**: `timeout 900 cargo test` → exit 0. Commit Phase 3.

## Test plan

- Offline unit tests in `online.rs` cover parsing, id safety, the Kenney scrape, credits, and HDR decode. They are the regression locks.
- `mcp::tests::handshake_and_tool_list` locks the tool list.
- One `#[ignore]` network test, plus the Step 5 JSON-RPC pipe, cover the live path. Run them once per phase and report the output.
- Frontend: typecheck and headless render only. The user tests the GUI.

## Done criteria

- [ ] `timeout 900 cargo test` exits 0
- [ ] The ignored `network` test passes once (paste the summary line in the report)
- [ ] The Step 5 pipe imports a Kenney model into `/tmp/j3-fetch` and writes `assets/CREDITS.txt`
- [ ] `rg "Command::new\(\"sh\"|bash -c" j3c/src/online.rs` finds nothing
- [ ] `Cargo.toml` `[dependencies]` is still empty
- [ ] `EDITOR.md`, `j3c/editor/README.md`, and `j3c/AGENTS.md` are updated as listed
- [ ] `plans/README.md` is not modified by you

## STOP conditions

- An API response shape differs from "Current state" (for example, Poly Haven `files` has no `gltf["1k"].gltf.include`, or ambientCG has no `downloadFolders`). STOP and paste the top-level keys.
- A Kenney page has zero or more than one `.zip` match for its slug. STOP.
- The Poly Haven glTF import fails for a reason other than missing SDL2_image textures (for example, a required extension). STOP and paste the error.
- Supporting this would need a crate, or a shell string built from input. STOP.
- `Server::import`'s behavior changes (the existing MCP tests fail after the split). STOP.

## Maintenance notes

- The Kenney list is hand-kept in `online.rs` `KENNEY`. A renamed slug shows up as the Step 3 "zero matches" error, so drop it or fix it.
- The cache is safe to delete: `rm -rf ~/.cache/j3/assets`.
- Plan 013 (other platforms) touches `cache_root` (Windows `LOCALAPPDATA`) and `extract` (it already branches on `cfg!(windows)`).
- When the renderer gains normal maps, extend the ambientCG material writer (`*_NormalGL.jpg`) and the glTF importer. The Phase 3 warning is where to start.
- Sketchfab needs a user token and a per-model license check. Do not add it without a design pass.
