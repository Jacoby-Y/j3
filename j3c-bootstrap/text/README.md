# text/

Non-code data embedded with `@embed` / `@embedDir` (plan 08 A7). Text that was Rust code (templates, help,
page prose) lives here with `@NAME@` placeholders (Rust `{{`/`}}` undone); a templated code file ends in `.in`
so no tool takes it for a source. Verbatim files (`j3c/src/doc/pages/*.md`, `helptext/{guide,manifest}.md`,
`helptext/recipes/*`, `ui/icons/bootstrap.txt`, the editor font) stay `@embed`ded from `j3c/` until plan 11 C6
moves them here.

- `new/`: `j3c new` (`cmd/new`): project templates (`*.in`), the AGENTS/SCENES/UI sections, `usage.txt`
  (`new --help`).
- `help/help.txt.in`: `j3c help` / `--help` / no argument (`cmd/guide`, Rust `print_help`).
- `doc/`: `j3c doc` (`doc/pages`): `lang.txt` and `j3ui_immediate.txt` (page templates), `editor.txt` and
  `urp.txt` (the lines before the embedded README), `usage.txt` (`doc --help`).
- `syntax/keywords.txt`: `WORD CLASS [SCOPE]` lines, read by `syntax/lex` (CLASS) and the textmate grammar
  (SCOPE); `type` lines are Rust's `BUILTIN_TYPES`. `tools/keywords.sh` checks it against the Rust tables.
- `syntax/j3ui.tmLanguage.json.in`: the `.j3ui` grammar (`dump-syntax --ui`), `@ELEMENTS@` filled from
  `ui/gen.elements()`.
- `errors.txt`: the error-code list (code, short title, long explanation; plan 11 B10). `base/diag` takes codes
  from it.
