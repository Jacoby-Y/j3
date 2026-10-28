# Plan 016: Move editor and asset logic into J3

## Status

- **Priority:** P2
- **Effort:** L; five independently reviewable phases
- **Depends on:** plan 001 and plan 004 (both marked done in `plans/README.md`)
- **Planned:** 2026-09-25

## Goal and boundary

Make J3 the source of truth for asset conversion, PNG processing, behavior catalog data, scene asset references, and grid placement. Keep Rust responsible for compiling J3, launching processes, MCP JSON-RPC and base64 transport, project path validation, and atomic file operations. Remove each Rust implementation once its J3 replacement passes the same behavior checks; do not leave two maintained codecs or scene rules.

Before implementing, compare the paths below with the live tree. Capture representative outputs from the existing importer, catalog generator, asset rename, and grid tool so behavior changes are visible. The working tree has many existing uncommitted files; preserve them.

## Current state

| Logic | Rust today | J3 already available |
|---|---|---|
| glTF/GLB to scene and mesh, including PNG color bake | `j3c/src/gltf.rs` (`import`, `convert_primitive`, `write_scene`) | `j3.gfx.mesh`, `j3.scene3d`, `j3.gfx.canvas.decodePng` |
| PNG decode and screenshot recompression | `j3c/src/png.rs` (`decode`, `compact`, `deflate_fixed`); MCP calls `compact` | `j3.gfx.canvas.decodePng`; `Canvas.savePng` writes stored DEFLATE |
| Behavior field/default catalog | `j3c/src/editor.rs::generate_catalog` parses `j3.editor.toml` | The editor reads the generated catalog through `j3.data.save` |
| Asset reference changes | `j3c/src/editor.rs::rename_asset` rewrites closed scene files | `j3c/editor/src/assets.j3::renameInTabs` and `renameInData` update open scenes |
| Grid placement | `j3c/src/mcp.rs::grid_requests` computes transforms and builds `add` requests | `j3c/editor/src/tool.j3` owns `add`, `ground`, `ontop`, and `scatter` |

## Phases

### 1. J3 JSON foundation and grid placement

1. Add a small `j3.json` parser for the subset needed by glTF and MCP grid arguments: objects, arrays, strings with escapes, numbers, booleans, and null. Use an arena-backed flat node table or token tree so no recursive by-value J3 type is needed. Report byte offsets on malformed input. Keep `j3c/src/lsp/json.rs` for compiler/LSP/MCP transport.
2. Add `grid` to `j3c/editor/src/tool.j3`. Rust passes a compact JSON argument as one tab-safe request field; J3 validates all pieces, computes `x*cell`, `floor*height`, and `turn*90`, then adds them as one undoable operation. Preserve the 2,000-piece limit, shape/asset rules, names, parent, and failure behavior. Validate the complete request before mutating the scene.
3. Reduce `mcp.rs` to JSON schema/argument forwarding and reply formatting. Delete `grid_requests` and its Rust placement tests after equivalent J3 tests pass. Expose the same operation through the scene CLI, rather than making it MCP-only.

**Verify:** J3 JSON fixtures cover escapes, nesting, invalid input, and numeric edges. Compare grid node transforms, names, errors, and single-step undo in file mode and with a live editor. Run `cargo test mcp::` and the J3 scene-tool checks.

### 2. PNG has one J3 implementation

1. Extend `Canvas.savePng` (or a reusable J3 PNG encoder beside it) with row filters and fixed-Huffman/LZ77 compression, retaining its public call shape. Preserve pixel bytes and the existing supported 8-bit PNG formats. Keep `decodePng` as the shared decoder for games and the later glTF importer.
2. Make editor renders and game screenshots write the compact PNG directly. Remove the two `png::compact` calls in `mcp.rs`, then remove `compact`, `deflate_fixed`, `refilter`, and related Rust encode helpers. Keep Rust `base64` for MCP image content.
3. Keep Rust `png::decode` only until phase 5 has moved glTF baking. Then remove the Rust decoder and its inflate implementation. Reuse existing `png_check` fixtures for the J3 encoder/decoder.

**Verify:** decoded pixels from editor renders and game screenshots match before/after; flat-shaded screenshots remain compact enough for MCP; PNGs open in a standard decoder. Run the J3 PNG checks and `cargo test png::` during the transition, then remove Rust-only tests with the removed code.

### 3. Behavior catalog in the J3 editor

1. Move the `j3.editor.toml` behavior-section/field parser and catalog serialization from `editor.rs::generate_catalog` into a J3 editor module. It must preserve field order, defaults, duplicate and malformed-field errors, `choice` options, and the catalog file format consumed by inspector and scene tools. Keep the compiler's reserved-keyword check at the Rust/compiler boundary rather than duplicating the keyword list in J3.
2. Add a headless editor `catalog` mode. Reorder startup as needed: Rust builds or locates the cached frontend, invokes catalog mode, generates the behavior dispatch, then launches the window or scene tool. Forward J3 diagnostics. Avoid a second config parser in Rust. The editor's Rescan path uses this same mode or the same J3 function.
3. Leave generation of `src/behaviors.j3` in Rust for now: it produces J3 source and depends on compiler/module naming. It should consume the validated behavior names from the J3 catalog instead of independently re-parsing the config.

**Verify:** existing catalog fixtures and a multiline `fields` array produce byte-equivalent catalogs; malformed defaults and duplicate names fail before editor launch; editor startup, Rescan, scene checks, and Play still see the same behavior fields.

### 4. Asset reference rewriting in J3

1. Extract a pure J3 `rewriteReferences(bytes, old, new)` operation from the open-tab logic in `assets.j3`. It must replace exact `node.*` and `param.*` asset values while preserving unrelated lines and unknown fields. Use it for both open scenes and closed scene files so rename semantics have one owner.
2. Add a headless editor operation for closed files. Rust keeps project-relative path validation, directory traversal, rename/trash operations, temporary files, and final atomic swaps; J3 supplies rewritten scene contents and the changed-file count. Prepare all rewritten files before committing the asset move, so a J3 or write error leaves the project usable.
3. Remove Rust's line-rewrite loop from `editor.rs::rename_asset`; keep its filesystem and event handling. Check placed-scene paths, mesh/texture/material references, and `param.*` asset fields, including open unsaved tabs.

**Verify:** old/new path replacement is exact, unknown lines survive, open and closed scenes agree, unsafe paths and extension changes are rejected, and rename failure does not leave mixed references. Run the existing editor rename tests and new J3 rewrite fixtures.

### 5. glTF import in J3

1. Add an importer module in the bundled J3 editor, using `j3.json` for glTF, `j3.gfx.canvas.decodePng` for `bake`, `j3.gfx.mesh` for output, and `j3.scene3d` for scene creation. Handle `.gltf`, `.glb`, and `.j3mesh`. Preserve supported accessors, indexed/non-indexed triangles, generated normals, transforms, materials, alpha, textures, shared meshes, hierarchy, warnings, and output names. Preserve the current explicit errors for unsupported compression, sparse accessors, and unsafe external URIs.
2. Add headless `import` mode to the cached editor binary. `editor.rs::import_asset` keeps source/path validation and process orchestration, then reads the J3 import result. Both UI import and MCP import use it; `bake: true` works through the same route. Do not compile a new J3 importer for each file.
3. Compare the generated `.j3scene`, `.j3mesh`, image files, and warnings against fixtures from the Rust importer. Use decoded mesh/scene values where harmless float formatting differs. Check that failure does not leave partial imported assets. Once parity holds, remove `j3c/src/gltf.rs` and Rust PNG decoding. Move the remaining Rust base64 helper into MCP/protocol code, delete `j3c/src/png.rs`, and update `j3c/src/lib.rs` and editor docs.

**Verify:** the existing glTF fixtures cover shared meshes, missing UVs, embedded/external images, matrix and quaternion transforms, and baked palette textures. Import a model through both editor UI and MCP, then render the resulting scene headless and inspect its image. Run `cargo test` from `j3c/` and the J3 acceptance programs on both emit backends.

## Done criteria

- Each of the five behaviors above has one J3 implementation used by editor, scene CLI, and MCP where applicable.
- Rust retains only compiler, process, filesystem transaction, and protocol responsibilities described in the boundary.
- `gltf.rs` and the Rust PNG codec are removed; Rust grid transform and scene-reference rewrite logic are removed.
- Existing import, PNG, catalog, rename, grid, scene, and MCP behavior checks pass; editor import and render work headlessly.
- `j3c/AGENTS.md`, `EDITOR.md`, and `plans/README.md` reflect the resulting ownership and status.

## Risks to check during implementation

- J3 currently has file read/write but limited directory operations; keep enumeration and atomic swaps in Rust, or add only the minimal portable J3 file API needed.
- The J3 JSON reader must bound nesting and input sizes for untrusted glTF/GLB files; preserve external URI path checks.
- The catalog mode must run before tools read `target/j3-editor/catalog` without recursively launching the editor frontend.
- Import output must be transactional enough that a failed conversion cannot be mistaken for a complete reusable scene.
