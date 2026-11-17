# Rust files that are not ported

`tools/coverage.sh` reads the `- PATH: reason` lines: PATH is relative to `j3c/src/` and may be a shell
glob (`*` also crosses `/`). Every other Rust file is named on a `Port of:` line in `src/`.

- scenefile.rs: scenes are read and written only through `j3.scene3d` (repo rule).
- lsp/json.rs: replaced by `j3.data.json`.
- ir/samples.rs: hand-built IR samples; tests are J3 source.
- ir/samples/*: the sample bodies (same reason).
- stdlib/files.rs: the hand-kept stdlib file list; replaced by `@embedDir`.
- emit/c.rs: no C backend (plan 11 B8a); LLVM is the one codegen backend.
- emit/c_*.rs: the C backend's types and statics (B8a).
- emit/c_expr/*: the C backend's expressions and statements (B8a).

`csslint_main.j3` (not Rust) goes too: `cmd/csslint` calls `j3.ui.css` in-process.
