# text/

Non-code data embedded with `@embed` / `@embedDir` (plan 08 A7): doc pages (`doc/`), recipes
(`recipes/`), `j3c new` templates (`new/`) and help text (`guide/`). They are copied from
`j3c/src/doc/pages`, `j3c/src/helptext` and `j3c/src/scaffold/templates.rs` when those modules are
ported. Until then, the Rust tree has the only copy.

Also data, not code (plan 11 B10); the compiler reads the same file the docs show:
- `errors.txt`: the error-code list (code, short title, long explanation). `base/diag` takes codes from
  it, `j3c explain CODE` prints it.
- `syntax/keywords.txt`: keyword and type-name lists, read by `syntax/lex` and the textmate grammar.
- `new/j3.toml`: the default `j3.toml`, written by `j3c new` and shown by `j3c help manifest`.
