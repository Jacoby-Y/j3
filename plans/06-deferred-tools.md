# Plan 12: Deferred tools in j3c-boot (LSP, MCP, editor, android, web, asset import)

**Status (2026-11-18): TODO.** Not ported during plan 11 milestone C5, by the user's choice: these tools have hardly been used since they were first written.

**Consequence:** once plan 11 C6 replaces the Rust `j3c` with `j3c-boot`, these stop working until this plan is done:
- `j3c lsp` (the VS Code / Cursor extension in `editors/vscode/` loses diagnostics, hover, completion, formatting);
- `j3c -e mcp` (the MCP server agents use for scenes; `-e mcp --tools` and `--help` already work);
- `j3c editor` (the windowed editor, `--new`, play from the editor);
- `j3c -e android run`, `j3c -e web serve|run`;
- asset import: MCP `import`, `asset_online_search`, `asset_fetch`, glTF import from the editor, Kenney/Poly Haven/ambientCG packs.

Every other tool is ported and byte-gated against stage 0 (`doc`, `help`, `guide`, `new`, `dump-syntax`, `-e scene`, `-e render`, `-e gen`, `-e imgdiff`, `golden`, `doctor`, `-e script-props`, `i18n`, `check` lint, `dist`). Either do this plan before C6, or ship C6 knowing the list above is broken.

## Today

Each deferred route prints `j3c-boot: CMD: not ported yet` on stderr and exits 1 (`base/diag.notPortedCmd`). Each deferred library fn returns a neutral value (`Err("... not ported yet")`, `""`, `false`, `None`, an empty list) and keeps its final signature, so filling a body is the whole job. Stub bodies end in `// STUB LSPS` / `LSPU` / `MCP` / `EDDEV` / `ASSETS` (`grep -rn "// STUB " j3c-bootstrap/src`). The five dispatch cases that reach them (`lsp-eof`, `e-mcp-ping`, `e-android-help`, `e-web-help`, `editor-help`) have the stub error as their expected output in `tests/expect/tool/` (plan 15); re-bless each (`diff.sh tool --bless`) when its route lands and review the new text.

Paths below are under `j3c-bootstrap/src/` (port) and `j3c/src/` (Rust).

## Decisions already made (do not reopen)

| Question | Decision |
|---|---|
| Session reuse in servers | One fresh `driver.Session` per LSP recheck and per MCP `check`/`play`, freed when the request is done or a newer one replaces it. No arena pool, no reset in place. The server's own state lives in one long-lived arena made at start; copy anything that outlives a Session into it. A 1000-cycle create/check/free probe inside the real split-built `j3c-boot` had flat RSS (4431 pages at iteration 100 and 999 on `speculative/01_basics.j3`; 5629/5629 on the `[ui]` project `speculative/50_ui_has`). |
| Reaching the open editor (plan 11 B12.2) | Deferred: `j3.instance` needs edits in `j3c/editor/src`. Port Rust's file protocol: append a line to `target/j3-editor/events`, poll `target/j3-editor/replies` every 20 ms for 10 s; "live" = a try-lock on `target/j3-editor/lock` fails. A timeout is an error, never a fallback to editing the file. |
| Editor program cache | `editor/frontend.build` builds into `~/.cache/j3/editor-boot/<xxh3>`, never Rust's `~/.cache/j3/editor` (Rust's `cleanup_frontends` deletes hash dirs it does not know). |
| MCP `check`/`play` | Build through `driver.run` (B12.6), not by spawning `j3c build`. The reply's `structuredContent` gets `"diags":[{code,severity,path,line,col,msg}]`, written **last** so a test masks it with one `sed`. |
| Tool `run` fns | All 23 in `mcp/ops.j3`, signature `fn(state: ptr, args: &Json, arena: &Arena): Result<Answer, string>`; `state` is `&mcp.server.State`. `toolsCall` turns an `Err` into `answerFail`. Scene ops go through `scene/ops.runTool` in-process. `mcp/ops` must not `use j3c.scene` (the table imports it). |
| Hash order | Nothing printed or written depends on hash order: open LSP buffers are a list in uri order, `placedScenes` sorted, completion `finish` sorts (sortText, label) then dedups (label, kind) keeping the first. Tool cases keep one open buffer per LSP session and avoid duplicate-name hovers (Rust shows HashMap/IR order there). |
| JSON-RPC | Both servers are handler tables on `lsp/proto.serve` (Content-Length and Line framing). Unlike Rust, an unknown method gets -32601 and a bad body -32700; keep them out of byte-compared sessions. Servers print nothing on stdout outside replies. JSON key order = insertion order (Rust's). |
| `lsp.ui` imports | `lsp/ui` and `lsp/docs` never import `j3c.lsp`; `lsp/complete` and `lsp/query` never import `lsp/docs` or `lsp/ui`. |
| Numbers | `.j3mesh` floats (`{:.6}`) through `base/fixed.pushFixed`. |

## Per feature

### 1. LSP server (`j3c lsp`), Rust ~950 lines

| Port stub | Signature / role |
|---|---|
| `cmd/lsp.j3` | `pub fn lspCmd(rest: []string): int` (Rust `lsp::serve`) |
| `lsp.j3`, `lsp/handlers.j3`, `lsp/reply.j3` | headers only: handler table, initialize/didOpen/didChange/didClose/hover/definition/completion/signatureHelp/documentSymbol/codeAction/formatting/shutdown/exit, capability JSON |
| `lsp/diag.j3` | header only: diagnostics per open buffer |
| `lsp/docs.j3` | real `Doc { uri; path; text }`, `Cursor { uri; path; src; offset: u32 }`; stubs `uriToPath(uri, arena): Option<string>`, `pathToUri(path, arena): string`, `posJson(line, col: u32, arena): &Json`, `completionJson(c: &Completion, arena): &Json`, `overlaysFrom(docs: []Doc, arena): []Overlay`, `pathsMatch(a, b, arena): bool`, `docsKey(docs: []Doc): u64` |

Rust to port: `lsp/mod.rs` 553 (cache, initialize 24-153, dispatch 160-467, 470-553), `lsp/diag.rs` 174, `lsp/docs.rs` 137, the LSP part of `lsp/proto.rs` (5-87 of 150).

Get right: `publishDiagnostics` one notification per open buffer (warnings severity 2, the error to the buffer holding its file, `.css` none); `exit` rc 0 after `shutdown`, else 1; capability JSON byte-equal.

### 2. LSP scanners and `.j3ui`/CSS LSP, Rust 1,625 lines

| Port stub | Signature |
|---|---|
| `lsp/complete/scan.j3` | real `CallSite`, `PrefixAt`, `enum LexMode`; stubs `findCall(src, offset: int, arena): Option<CallSite>`, `prefixOf(src, offset, arena): PrefixAt`, `attrPrefix(src, offset, arena): Option<string>`, `lexicalMode(src, offset): LexMode`, `isIdentName(s): bool` |
| `lsp/complete/uses.j3` | real `UsePair`, `Uses`, `UseAt`; stubs `importEdits(s: &Session, src, messages: []string, arena): List<ImportEdit>`, `useAt(src, offset, arena): Option<UseAt>`, `completeUse(s, u: &UseAt, arena): List<Completion>`, `usesOf(s, src, arena): Uses` |
| `lsp/complete/script.j3` | stubs `scriptPathPrefix(src, offset, arena): Option<string>`, `scriptPathItems(path, prefix, arena): List<Completion>`, `scriptPropItems(s, path, src, prefixAt: int, prefix, arena): Option<List<Completion>>`, `blankLine(src, offset: u32, arena): string` |
| `lsp/ui.j3` | real `UiCache { key: u64; set: bool }` (add fields), `newCache()`; stubs `isUiRequest(params: &Json): bool`, `handle(method, params: &Json, docs: []Doc, cache: &UiCache, arena): &Json` |
| `lsp/ui/items.j3`, `lsp/ui/css.j3` | headers only |

Rust to port: `lsp/complete/scan.rs` 371, `uses.rs` 374, `script.rs` 149, `lsp/ui.rs` 238, `ui/items.rs` 320, `ui/css.rs` 173. Tests to port: `lsp/complete/tests.rs` 241, `lsp/ui/tests.rs` 211 (fixture `j3c/tests/fixtures/ui_lsp/`).

Get right: scanners are marked as fallback (B12.5) and skip char literals; import edits sorted by module; `.j3ui` hover/definition/members go through the span map (`ui/map.j3` `toGen`/`toSrc`); CSS tables read from the stdlib `ui/css*.j3` at run time.

**`tests/unit/lsp_complete_test.j3` is skipped** (it prints why) while `isIdentName` is a stub. Its cases also need `findCall`, `prefixOf`, `attrPrefix`, `useAt`, so the skip lifts on its own when this section lands; run `tests/unit.sh lsp_complete` then.

Already ported, build on it: `sema/query.j3` (`fnAt`, `findLocal`, `lookupFn`, `find*`, `tyString`, `sigText`, ...), `sema/exports.j3`, `lsp/query.j3` (`analyze` = fresh `Mode.Lib` Session to `Stage.Sema`, `hover`, `definition`, `formatFile`, `rangeOf`, `offsetAt`), `lsp/query/traits.j3`, `lsp/complete.j3` + `complete/{items,members,symbols}.j3` (`completeAt`, `completeBuffer`, `memberItems`, `signature`, `symbols`), `lsp/proto.j3` (the RPC loop), `load.j3` loading the analyzed path when it is not the entry. Known gap kept (B12.5): a name defined twice can hover to a different match than Rust (sema decl order vs Rust's IR func order).

### 3. MCP server (`j3c -e mcp`), Rust 1,198 lines

| Port stub | Signature |
|---|---|
| `mcp.j3` | `pub fn mcpCmd(rest: []string): int`: `--help` (Rust `USAGE`), `--tools`, `--project DIR` and the argument errors are real (cases `e-mcp-tools`, `e-mcp-help`, `e-mcp-bogus`, `e-mcp-project-novalue`); only the serve loop after them is the stub |
| `mcp/ops.j3` | 23 run fns, table order: `sceneList sceneGet sceneAdd sceneAddMany sceneApply sceneSplit sceneInline sceneFmt sceneSet sceneDelete place undo render check play console import assetSearch assetOnlineSearch assetFetch gen sceneStats sceneDiff`, each `(state: ptr, args: &Json, arena: &Arena): Result<Answer, string>` |
| `mcp/server.j3` | real `State { tools }` + `ping`, `tools/list`, `tools/call`; add root, follow, next, wasLive, coach, `initialize` (instructions, protocol versions, project notes), the edit/live notes and coach hints |

Rust to port: `mcp/mod.rs` 267, `mcp/server.rs` 469 (handle and `run_tool` 4-263; batch, live, `live_request`, render, import, fetch 267-469), `mcp/ops.rs` 462 (check 161-252, play 275-352). Tests: `mcp/tests.rs` 142 (coach, project notes, handshake, grid/batch requests, values to request text, import skips existing), plus a fake-editor unit test (a thread reading `events`, writing `replies`, holding the lock; never a real editor).

Already ported: `mcp/schema.j3` (the tool table types, generated `tools/list` and `--tools`, byte-equal with stage 0 in `tests/unit/tool_schema_test.j3`; `Answer`, `answerText`, `answerFail`, `textBlock`, `answerJson`), `scene.j3` `tools()` (each `run` already names its `mcp/ops` fn), `scene/ops.j3` (`findRoot`, `sceneRel`, `allScenes`, `placedScenes`, `checkScene`, `runTool`, `requestText`, `applyArgs`, `render`, `configValue`, `scenePath`, `BATCH`), `scene/reply.j3` (`parse`, `error`, `toJson`, `toText`), `scene/diff.j3`, `scene/history.j3`, `scene/patterns.j3`, `editor/assets.j3` (catalog, events, script props, asset rename/delete/create), `editor/frontend.j3` (`hostExe`, `cacheRoot`, `cacheDir`, `build`), `assets/png.j3` (`compact`, `base64` for render thumbnails), `cmd/gen.j3` (`runAll`), `cmd/golden.j3` `spawn` (env, cwd, process group).

Get right: `initialize` text, notes, coach hints and error texts byte-equal; `render` = gen, then the subprocess, then `png.compact` + base64.

### 4. `j3c editor` (host and play), Rust ~670 lines

| Port stub | Signature |
|---|---|
| `editor.j3` | `pub fn editorCmd(rest: []string): int` (`[--new [--fps]] [DIR]`); also gets `note_open` / `project_lock` |
| `editor/play.j3` | header only (start/stop the game; build through `driver.run`, show the `Diag`s, run the game as a subprocess) |

Rust to port: `editor/mod.rs` 218 (78-218), `editor/play.rs` 449. Tests: `editor/tests.rs` 26, 139 (create project, add editor files, validate scene, project lock).

Already ported: `editor/frontend.j3` (builds the editor program into `editor-boot`, `FONT`), `editor/assets.j3`, `cmd/new/templates.j3` (`createProject`, `addEditorFiles`, `addScenesSection`), `cmd/gen.j3`.

Get right: add files, take the project lock, generate the catalog, run gen, then launch; the `J3EDITOR ...` stdout protocol and forwarding game output into `events`. Never run in tests (opens a window); only `editor --help`.

### 5. `j3c -e android run`, Rust 252 lines

Port stub: `cmd/android.j3` `pub fn androidCmd(rest: []string): int`.

Rust to port: `android_run.rs` 252 (build, `adb install -r`, `am start`, `logcat --pid`; tests 240-252 `parse`, `parse_devices`, `pick_device`). Already ported: `dist/apk.j3` (`path`, `build`), `toolchain/android.j3` (`sdkRoot`, `opensslLibDir`, `highestVersionDir`). Never call adb in tests; only `-e android --help`.

### 6. `j3c -e web serve|run`, Rust 447 lines + `shell.html` 81

Port stub: `cmd/web.j3` `pub fn webCmd(rest: []string): int`.

Rust to port: `web/mod.rs` 203, `web/serve.rs` 244 (routes `/__j3/env`, `log`, `file`, `exit`; mime types; `file_under` refusing escapes; tests 180-244: port 0, a client thread over `j3.net.tcp`), `web/shell.html` (embed it from `j3c/src/web/` until C6). Never start Chrome; `serve` only in a unit test on port 0.

### 7. Asset import (glTF, online, packs), Rust 1,527 lines

| Port stub | Signature |
|---|---|
| `assets/gltf.j3` | `import(project, scene, source: string, bake: bool, arena): Result<List<string>, string>` (called by `editor/assets.importAsset` and MCP `import`), `importMesh(project, scene, source, arena): Result<List<string>, string>` |
| `assets/gltf_material.j3` | header only |
| `assets/online.j3` | real `Hit { source; id; kind; name; author; thumb }`, `enum Fetched { Model(string), Material(string), Pack([]string), Sky(string) }`; stubs `search(source, query, kind, arena): Result<List<Hit>, string>`, `fetch(root, source, id, file, arena): Result<Fetched, string>` (`file` "" = none) |
| `assets/pack.j3` | stubs `thumb(hit: &Hit, arena): string`, `packThumbs(source, id, models: []string, arena): List<string>` |

Rust to port: `gltf/mod.rs` 488, `gltf/read.rs` 274, `gltf/material.rs` 163, `online.rs` 326, `online/sources.rs` 108, `online/pack.rs` 168. Tests: `gltf/tests.rs` 124 (write the fixtures out as `tests/proj/gltf_*`), `online/tests.rs` 192 (canned JSON/HTML only).

Already ported: `base/zip.j3` (`unzip` with escape refusal, `safeRel` = Rust `online::safe_rel`), `assets/png.j3` (`decode`), `base/fixed.j3`, `editor/assets.j3` (`importAsset`, `keepImport`, `importName`).

Get right: output order follows the node walk; shared meshes and `bake` at the centroid as the tests say; cleanup on failure; `curl --proto =https` with the cache max-age. Never touch the network or `~/.cache/j3/assets` in tests.

## Gate for this plan

From the repo root, foreground, `J3C_NO_INSTALL=1`, everything under `timeout`:
- `diff.sh tool j3c-bootstrap/tests/tool/<f>.txt` 0 DIFF/0 CRASH for the case files `lsp`, `lsp_ui`, `mcp`, `eddev`, `assets` (they exist with headers only; write the cases). LSP sessions: Content-Length stdin files, one open buffer each, only names defined once. MCP: line-framed sessions on scratch copies of `examples/3d-editor-scene` and `examples/editor_village`, `check` with `mask=` on `"diags"`, `play` with `frames`. ASSETS: MCP `import` of `tests/proj/gltf_*` with `out=assets/imported/**`.
- The five dispatch cases' expected files (`tests/expect/tool/`) re-blessed with the real output, reviewed.
- `tests/unit.sh` incl. `lsp_*` (with `lsp_complete` no longer skipped), `mcp_*`, `editor_host_*`, `editor_play_*`, `gltf_*`, `online_*`, `pack_*`, `android_*`, `web_*`.
- Two RSS loops through the real binary: 1000 `didChange` edits of one buffer through `j3c-boot lsp`, and 1000 `tools/call scene_list` through `j3c-boot -e mcp`; RSS levels off.
- The plan 11 standing gate and `diff.sh self` stay green.

Never run in tests: `j3c editor` without `--help` (also `editor --new`); `-e android run`; `-e web run`; `-e web serve` outside a unit test on port 0; online search/fetch and MCP `asset_online_search`/`asset_fetch`; `j3c lsp` or `-e mcp` without stdin from a file and a `timeout`; MCP `play` without headless SDL env and `frames`; any MCP write outside a scratch copy; writing `~/.cache/j3/editor/` or `~/.cache/j3/tools` from `j3c-boot`.

When done: drop the matching `main.j3` "imported so it compiles" lines, and this plan.
