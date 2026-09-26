# Plan 003: J3 language gaps found building the village

> **Executor instructions**: Follow this plan step by step. Run every
> verification command and confirm the expected result before moving to the
> next step. If anything in the "STOP conditions" section occurs, stop and
> report. Do not improvise. When done, do NOT update `plans/README.md`
> (the reviewer does that).
>
> **Drift check (run first)**: the tree has uncommitted work on top of
> `b086a5b`, so a SHA check is not enough. Compare every "Current state"
> excerpt below against the live file. Line numbers may be off by a few; the
> code must match. If an excerpt's code is gone or different, STOP.

## Status

- **Priority**: P2
- **Effort**: M
- **Risk**: LOW
- **Depends on**: none
- **Category**: language
- **Planned at**: 2026-09-25

## Why this matters

Building `examples/editor_village` hit three gaps (EDITOR.md, MCP Fixes item 13):

1. **No `%`.** `let a = 7 % 3;` fails with `error: unexpected character '%'`.
   Grid code, wrap-around indices, and animation phases all need it.
2. **No `text.contains` or `text.find`.** `examples/editor_village/src/player.j3:113-123`
   has a hand-written `has(s, part)` helper for this.
3. **`module.CONST` is not a value.** With `use j3.gfx.mesh;`, `let k = mesh.STRIDE;`
   fails with ``error: `mesh` is not a place``. Calls like `scene3d.load(...)`
   work. The village worked around it with `use j3.scene3d.{ INSTANCE }`.

All three were confirmed with `j3c check` on small files before this plan was written.

## Current state

### (a) `%`: the IR and both printers already have it. The front end does not.

`BinOp::Rem` exists in the IR (`j3c/src/ir/op.rs:10`, `as_str` "%", `assign_op_str` "%=").
Folding and wrap rules include it (`j3c/src/ir/wrap.rs:11-44`):

```rust
pub fn wrapping_bin(op: BinOp) -> bool {
    matches!(
        op,
        BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem | BinOp::Shl | BinOp::Shr
    )
}
...
        BinOp::Rem => {
            if b == 0 {
                None
            } else {
                Some(a.wrapping_rem(b))
            }
        }
```

The LLVM printer handles float and int Rem (`j3c/src/emit/llvm_expr.rs:991-1000`, `1048-1095`).
Float uses `frem`. Signed int branches around `MIN / -1` (plan `4260bfd`, "Branch around signed
MIN/-1 before sdiv") and gives `0` for Rem:

```rust
                    let wrapped = if op == BinOp::Div { min } else { "0" };
```

The C printer's `wrap_int` (`j3c/src/emit/c_expr.rs:735-760`) has a **bug** for Rem: the
`MIN, -1` case returns `MIN` for both ops. `MIN % -1` must be `0`:

```rust
        BinOp::Div | BinOp::Rem => {
            let sop = op.as_str();
            match cx.ty_kind(ty) {
                TyKind::Int(int_ty) if int_ty.is_signed() => {
                    ...
                    Ok(format!(
                        "((({lhs}) == ({c}){min} && ({rhs}) == ({c})-1) ? ({c}){min} : ({c})(({lhs}) {sop} ({rhs})))"
                    ))
```

C float binaries print `({l} {op} {r})` (`c_expr.rs:176-184`). For float Rem that is `a % b`,
which C rejects. It needs `fmodf` / `fmod`.

The front end has no `%`:

- Lexer, `j3c/src/parse/lex.rs:131-134`. There is no `'%'` arm, so it falls through to
  `unexpected character`:
  ```rust
                '+' => self.assign_op(start, TokenKind::Plus, TokenKind::PlusEq),
                '-' => self.assign_op(start, TokenKind::Minus, TokenKind::MinusEq),
                '*' => self.assign_op(start, TokenKind::Star, TokenKind::StarEq),
                '/' => self.assign_op(start, TokenKind::Slash, TokenKind::SlashEq),
  ```
- Tokens, `j3c/src/parse/token.rs:65-66` (`Slash`, `SlashEq`). `operator_class` is at about
  line 159 and `operator_src` at about line 191. `j3c/src/parse/syntax.rs:88` and `:99` list
  operator tokens for the TextMate dump.
- AST `BinOp`, `j3c/src/parse/ast.rs:376-390`: `Add, Sub, Mul, Div, Eq, ... Or`. No `Rem`.
- Parser `expr_mul`, `j3c/src/parse/parser.rs:1203-1210`:
  ```rust
            let op = match self.cur().kind {
                TokenKind::Star => BinOp::Mul,
                TokenKind::Slash => BinOp::Div,
                _ => break,
            };
  ```
- Compound assign, `parser.rs:803-816`. `/=` is rejected, and so `%=` stays out of this plan too:
  ```rust
                        if kind == TokenKind::SlashEq {
                            return Err(self.err("`/=` not in this subset"));
                        }
  ```
  The reason is that AssignOp lowering (`check/lower.rs` about line 500) never calls
  `abort_if_zero`, so `/=` and `%=` would skip the `/0` abort.
- Check. `j3c/src/check/mod.rs:565-588` `fold_const_bin` maps only Add/Sub/Mul/Div, and gives
  the `/0` error only for Div. `mod.rs:1347` `peek_ty` arithmetic arm is
  `Add | Sub | Mul | Div`. `mod.rs:1571` `find_operator` maps names and ends with `_ => return None`.
- Lower. `j3c/src/check/lower.rs:1576` arithmetic arm `Add | Sub | Mul | Div`. The int `/0` abort is
  added at `:1598` and `:1652`:
  ```rust
                if matches!(op, parse::BinOp::Div) && is_int(self.cx, ty) {
                    self.abort_if_zero(span, &r)?;
                }
  ```
  `ast_binop` at `lower.rs:3174-3189` is an exhaustive match.
- libm. LLVM lowers `frem` on x86-64 to a call to `fmod`/`fmodf`, which lives in libm. Libraries
  are linked from `; use lib "NAME"` / `/* use lib "NAME" */` markers in the emitted text
  (`j3c/src/emit/host.rs:440-457` `libs_from_c`). `link_lib("m", ..)` already returns `-lm`
  (`host.rs:320-322`).

Existing locks: `wrap::tests::div_zero_does_not_panic_host`, `compile.rs`
`signed_div_min_is_a_branch` (line ~774), `div_by_zero_aborts`, `const_div_by_zero_is_error`,
`fast_keeps_div0`.

### (b) `j3.text` search

`j3c/stdlib/j3/text.j3` exports `cstr`, `Text`, `same` (line 192), `cstrBytes`. There is no
search. The village helper (`examples/editor_village/src/player.j3:113-123`):

```j3
/// Whether `part` appears in `s`.
fn has(s: string, part: string): bool {
    let i = 0;
    while i + part.len <= s.len {
        let k = 0;
        while k < part.len && s[i + k] == part[k] { k += 1; }
        if k == part.len { return true; }
        i += 1;
    }
    return false;
}
```

### (c) `module.CONST`

A whole-module `use j3.gfx.mesh;` inserts only functions under the alias
(`j3c/src/check/mod.rs:1742-1757`, `UsePhase::Vals`):

```rust
                    let mut exported = HashMap::new();
                    for (k, v) in &mods[src_i].funcs {
                        if mods[src_i].exports.contains(k) {
                            exported.insert(k.clone(), *v);
                        }
                    }
                    mods[i].mod_ns.insert(alias.clone(), exported.clone());
                    mods[i].lib_ns.insert(alias, exported);
```

Calls resolve `alias.f(...)` through `lib_ns`/`mod_ns` in `lower.rs` `call` (about line 1138).
A value `alias.NAME` goes `operand` → `try_place` → `ExprKind::Field` → `try_place(base)` → `Name`
arm, which errors at `lower.rs:1347-1351` (``format!("`{}` is not a place", n.name)``).
Named imports copy a `ConstVal` into `mods[i].consts` (`mod.rs:1856-1872`). `collect_consts`
runs for every module (`mod.rs:208-210`) before `resolve_all_uses(.., Vals)` (`mod.rs:220`),
so source consts are ready when the alias is bound.

Const lookups by name: `lower.rs:775` (`rvalue` Name arm → `const_rvalue`), `lower.rs:1696`
`fold_int` (Name arm), and `mod.rs:1268` `peek_ty` (Name arm). `peek_ty`'s Field arm is at
`mod.rs:1285`. `lower.rs:843` is the `rvalue` Field arm. `lower.rs:385` `unknown` builds did-you-mean
candidates from `self.consts.keys()`.

Test helpers in `j3c/src/compile.rs` `mod tests`: `honesty(name)` (about line 1518), `run_ok`
(about line 1524, runs LLVM only), and `both_backends_run_01_through_08` (line ~701), which shows
how to run one program on both backends.

## Design

- **`%`** reuses the IR `Rem` everywhere. The front end treats it exactly like `/`: same
  precedence, int `/0` abort, and a const-fold error. No `operator%` overload and no `%=`
  (parity with `/=`).
- **Float `%` in C** prints `__builtin_fmodf(l, r)` / `__builtin_fmod(l, r)`. clang and gcc
  know these builtins, so no prototype is needed and nothing clashes with a user `use lib "m"`
  binding.
- **libm.** After printing, if the text contains a float remainder (`" frem "` in LLVM,
  `"__builtin_fmod"` in C), append one marker line (`; use lib "m"` or `/* use lib "m" */`).
  `libs_from_c` then adds `-lm`. Add the comment
  `// ponytail: text scan for frem; flag it in the printer if this grows`.
- **Module consts** are stored under a dotted key `"alias.NAME"` in the same `consts` map.
  An identifier can never contain `.`, so the key cannot collide with a real name. It flows
  through every existing `consts` clone and merge (`MergedEnv`, generic origins) with no new
  plumbing. A local, alias, or global named like the module still wins. That matches how a
  local shadows a module name in calls.

## Commands you will need

Run from `j3c/`. `cargo build`/`cargo test` copy the binary to `~/.bin/j3c`, which is expected.

| Purpose | Command | Expected on success |
|---------|---------|---------------------|
| Build | `timeout 300 cargo build --release` | exit 0 |
| New tests | `timeout 300 cargo test gaps_ -- --test-threads=1` | all `gaps_*` pass |
| Div locks | `timeout 300 cargo test div -- --test-threads=1` | pass |
| Full | `timeout 600 cargo test` | exit 0 |
| Spot check | `timeout 60 ./target/release/j3c run examples/honesty/rem.j3` | prints `ok` |
| Grammar | `timeout 60 ./target/release/j3c dump-syntax ../editors/vscode/syntaxes/j3.tmLanguage.json` | exit 0, file gains `%` |

## Scope

**In scope**:
- `j3c/src/parse/{lex.rs,token.rs,syntax.rs,ast.rs,parser.rs}`: the `%` token and `BinOp::Rem`
- `j3c/src/check/{mod.rs,lower.rs}`: Rem typing, folding, the `/0` abort, and module consts
- `j3c/src/emit/c_expr.rs`: C float Rem, and C `MIN % -1` giving 0
- `j3c/src/emit/llvm.rs`, `j3c/src/emit/c.rs`: the libm marker
- `j3c/stdlib/j3/text.j3`: `find`, `contains`
- `j3c/examples/honesty/{rem.j3,text_find.j3,module_const.j3}` (new)
- `j3c/src/compile.rs` tests
- `editors/vscode/syntaxes/j3.tmLanguage.json` (regenerated by `dump-syntax`)
- `examples/editor_village/src/player.j3`: replace `has` with `text.contains` (optional, last step)

**Out of scope**:
- `%=` and `/=` (they need the `/0` abort in AssignOp lowering; that is a separate plan)
- `operator%` user overloads
- Module consts inside `const` initializers (`const X = mesh.STRIDE + 1;`), inside `match`
  patterns, and module-qualified `let` globals. They can follow this pattern later.
- LSP hover/goto on `mesh.STRIDE`
- A signed-remainder convention change: `%` truncates like C (`-7 % 3 == -1`), on both backends.

## Git workflow

Branch `plan-003-j3-language-gaps`. One commit per step group is fine. Do not push.
Messages like `Add % for int and float.`, `Add text.find and text.contains.`,
`Resolve module.CONST as a value.`

## Steps

### Step 1: Lex and parse `%`

1. `token.rs`: add `Percent` after `SlashEq`. In `operator_class`, add it to the arithmetic arm
   (`Plus | Minus | Star | Slash | Percent`). In `operator_src`, add `TokenKind::Percent => Some("%")`.
2. `syntax.rs`: add `TokenKind::Percent` next to `TokenKind::Slash` in the single-char list
   (about line 99).
3. `lex.rs`: add `'%' => self.one(TokenKind::Percent, start),` after the `'/'` arm. `one` is
   already used for `@ # ?`.
4. `ast.rs`: add `Rem` after `Div` in `BinOp`.
5. `parser.rs` `expr_mul`: add `TokenKind::Percent => BinOp::Rem,`.

**Verify**: `timeout 300 cargo build --release`. The compiler lists every non-exhaustive
`parse::BinOp` match (at least `ast_binop` in `lower.rs`). Fix them in Step 2.

### Step 2: Type, fold, and abort

1. `lower.rs` `ast_binop`: add `parse::BinOp::Rem => BinOp::Rem`.
2. `lower.rs:1576`: widen the arm to `Add | Sub | Mul | Div | Rem`. At `:1598` and `:1652`,
   change `matches!(op, parse::BinOp::Div)` to `matches!(op, parse::BinOp::Div | parse::BinOp::Rem)`.
3. `mod.rs` `peek_ty` arithmetic arm (`:1347`): add `| parse::BinOp::Rem`.
4. `mod.rs` `fold_const_bin`: add `parse::BinOp::Rem => crate::ir::op::BinOp::Rem`. Make the
   zero check `matches!(op, parse::BinOp::Div | parse::BinOp::Rem)`. Keep the message
   `"division by zero in const initializer"` so `const_div_by_zero_is_error` still matches it.
5. `find_operator` needs no change (`_ => return None`).

**Verify**: `timeout 300 cargo build --release` exits 0. Then write `/tmp/rem_t.j3`:

```sh
cat > /tmp/rem_t.j3 <<'EOF'
module t;
use j3.io.printLn;
fn main() {
    let a = 7;
    let b = 3;
    let r = a % b;
    printLn(`{r}`);
}
EOF
timeout 60 ./target/release/j3c run /tmp/rem_t.j3
```

It prints `1`.

### Step 3: Printers

1. `c_expr.rs` `wrap_int`, signed branch: the value for the `MIN, -1` case is `({c}){min}` for
   Div and `({c})0` for Rem. Build it as `let wrapped = if op == BinOp::Div { format!("({c}){min}") } else { format!("({c})0") };`.
2. `c_expr.rs` `RvalueKind::Binary` (`:176`): before the generic `({l} {op} {r})`, if
   `*op == BinOp::Rem` and the type is `TyKind::Float(_)`, print `__builtin_fmodf({l}, {r})`
   for f32 and `__builtin_fmod({l}, {r})` for f64. Use `c_ty`/`c_type` to tell the width, as
   the file does elsewhere. The `AssignOp` arm (`:508`) cannot see Rem (the parser never makes
   `%=`). Leave it alone.
3. libm marker: at the end of `emit_llvm` (`llvm.rs`, before `Ok(out)`), add
   `if out.contains(" frem ") { out.push_str("; use lib \"m\"\n"); }`. At the end of
   `emit_program` in `c.rs`, add `if out.contains("__builtin_fmod") { out.push_str("/* use lib \"m\" */\n"); }`.
   Add the ponytail comment from Design.
4. The LLVM int path already produces `srem`/`urem` with the MIN branch. Do not touch it.

**Verify**: `timeout 300 cargo build --release`. Then
`timeout 60 ./target/release/j3c run --emit c /tmp/rem_t.j3` prints `1`. Floats are covered by
Step 6's `rem.j3` on both backends.

### Step 4: `text.find` / `text.contains`

Add to `j3c/stdlib/j3/text.j3` after `same`:

```j3
/// Byte index of the first `part` in `s`, or -1. An empty `part` is found at 0.
pub fn find(s: string, part: string): int {
    let i = 0;
    while i + part.len <= s.len {
        let k = 0;
        while k < part.len && s[i + k] == part[k] { k += 1; }
        if k == part.len { return i; }
        i += 1;
    }
    return -1;
}

/// Whether `part` appears in `s`.
pub fn contains(s: string, part: string): bool {
    return find(s, part) >= 0;
}
```

`&&` short-circuits (`short_circuit_skips_oob` locks that), so `s[i + k]` is never out of bounds.

**Verify**: `timeout 60 ./target/release/j3c doc j3.text` lists `find` and `contains`.

### Step 5: Module consts as values

1. `mod.rs`: add `pub(crate) fn qual_key(alias: &str, name: &str) -> String { format!("{alias}.{name}") }`.
2. `mod.rs` `resolve_all_uses`, `UsePhase::Vals`, whole-module branch (`:1742-1757`): after
   `mod_ns`/`lib_ns` insert, copy each exported const:
   ```rust
   let consts: Vec<(String, ConstVal)> = mods[src_i].consts.iter()
       .filter(|(k, _)| mods[src_i].exports.contains(*k))
       .map(|(k, v)| (qual_key(&alias, k), v.clone())).collect();
   for (k, v) in consts { mods[i].consts.insert(k, v); }
   ```
   Clone `alias` before the `lib_ns.insert(alias, ..)` move. `exports` never holds a dotted
   name, so dotted keys are never re-exported.
3. `lower.rs`: add a helper
   ```rust
   /// `alias.NAME` for a const exported by a `use`d module. Locals, aliases, and globals win.
   fn qualified_const(&self, expr: &Expr) -> Option<ConstVal> {
       let ExprKind::Field { base, field } = &expr.kind else { return None };
       let ExprKind::Name(n) = &base.kind else { return None };
       if self.locals.contains_key(&n.name) || self.aliases.contains_key(&n.name) || self.globals.contains_key(&n.name) {
           return None;
       }
       self.consts.get(&super::qual_key(&n.name, &field.name)).cloned()
   }
   ```
   Adjust the path to `qual_key` to match how `lower.rs` imports from `check/mod.rs`.
4. Use it in three places:
   - `fold_int` (`:1696`): a new arm `ExprKind::Field { .. } => match self.qualified_const(expr) { Some(ConstVal { kind: ConstPayload::Int(v), .. }) => Some(v), _ => None },`.
     This covers ints in `==`, arithmetic, and interpolation.
   - `rvalue` Field arm (`:843`), as the first check:
     `if let Some(c) = self.qualified_const(expr) { return Ok(self.const_rvalue(expr.span, &c)); }`.
     This covers float and string consts. Match `const_rvalue`'s real signature.
   - `unknown` (`:390`): `cands.extend(self.consts.keys().filter(|k| !k.contains('.')).cloned());`
     so a dotted key never shows up as a did-you-mean.
5. `mod.rs` `peek_ty` Field arm (`:1285`), before `peek_ty(base)`:
   ```rust
   if let ExprKind::Name(n) = &base.kind {
       if !locals.contains_key(&n.name) {
           if let Some(c) = consts.get(&qual_key(&n.name, &field.name)) { return Ok(c.ty); }
       }
   }
   ```
6. A module base with a missing const (`mesh.NOPE`) would still say ``is not a place``. In the
   `rvalue` Field arm, right after the `qualified_const` check, handle it: if `base` is a
   `Name` that is in `self.mod_ns` and is not a local, alias, or global, return
   ``CheckError::new(self.path, field.span, format!("`{}` has no pub const `{}`", n.name, field.name))``.

**Verify**:

```sh
cat > /tmp/mc_t.j3 <<'EOF'
module t;
use j3.io.printLn;
use j3.gfx.mesh;
fn main() {
    let k = mesh.STRIDE;
    let u = mesh.STRIDE_UV;
    printLn(`{k} {u}`);
}
EOF
timeout 300 cargo build --release && timeout 60 ./target/release/j3c run /tmp/mc_t.j3
```

It prints `10 12`.

### Step 6: Tests

Add three honesty programs. Each prints `ok` only when every check holds.

`j3c/examples/honesty/rem.j3`:
```j3
module honesty.rem;

use j3.io.printLn;

const R: int = 17 % 5;

fn main() {
    let a = 7;
    let b = 3;
    let n = -7;
    let min = -9223372036854775807 - 1;
    let m1 = -1;
    let f: f32 = 7.5;
    let g: f32 = 2.0;
    let d: f64 = -7.5;
    let e: f64 = 2.0;
    let w32: i32 = 10;
    if a % b == 1 && n % b == -1 && min % m1 == 0 && R == 2
        && f % g == 1.5 && d % e == -1.5 && w32 % 4 == 2 && (a + 2) * 3 % 4 == 3 {
        printLn("ok");
    }
}
```
If any literal form above does not typecheck (for example `f64` annotation spelling), adjust it
to what the checker accepts without changing the tested values.

`j3c/examples/honesty/text_find.j3`:
```j3
module honesty.text_find;

use j3.io.printLn;
use j3.text;

fn main() {
    if text.find("hello", "ll") == 2 && text.find("hello", "lo!") == -1
        && text.find("hi", "") == 0 && text.find("", "a") == -1
        && text.contains("plant_bush_2", "bush") && !text.contains("tree", "trees") {
        printLn("ok");
    }
}
```

`j3c/examples/honesty/module_const.j3`:
```j3
module honesty.module_const;

use j3.io.printLn;
use j3.gfx.mesh;

fn stride(): int {
    return mesh.STRIDE;
}

fn main() {
    let uv = mesh.STRIDE_UV;
    if stride() == 10 && uv - mesh.STRIDE == 2 {
        printLn(`ok`);
    }
}
```

In `compile.rs` `mod tests`, add a helper next to `run_ok`:

```rust
fn run_both_ok(path: &Path, stdout: &str) {
    let prog = compile_path(path).expect("compile");
    for backend in [emit::Backend::Llvm, emit::Backend::C] {
        let mut o = emit::EmitOpts::default();
        o.backend = backend;
        if backend == emit::Backend::Llvm && !emit::clang_available(&o.clang) { continue; }
        let run = emit::run_program(&prog.cx, &prog.modules, &o).expect("run");
        assert_eq!(run.status, 0, "{backend:?} stderr {}", String::from_utf8_lossy(&run.stderr));
        assert_eq!(String::from_utf8_lossy(&run.stdout), stdout, "{backend:?}");
    }
}
```

Tests (all names start with `gaps_` so one filter runs them):

- `gaps_rem_runs_on_both_backends`: `run_both_ok(&honesty("rem.j3"), "ok\n")`.
- `gaps_rem_by_zero_aborts`: like `div_by_zero_aborts`. Inline source
  `let a = 1; let b = 0; let _ = a % b;` through `check::check_files`. Assert
  `dump_program` contains `abort div0;`.
- `gaps_const_rem_by_zero_is_error`: `const X = 1 % 0;` gives an error containing `division by zero`.
- `gaps_text_find_runs`: `run_both_ok(&honesty("text_find.j3"), "ok\n")`.
- `gaps_module_const_runs`: `run_both_ok(&honesty("module_const.j3"), "ok\n")`.
- `gaps_module_const_missing_is_named`: inline `use j3.gfx.mesh; ... let k = mesh.NOPE;` fails and
  the message contains `has no pub const`. For the inline test, `check_files` only has one file,
  so use `compile_path` on a temp file as `signed_div_min_is_a_branch` does. That test loads the
  stdlib.
- Extend `wrap::tests` with `assert_eq!(fold_bin(BinOp::Rem, i64::MIN, -1), Some(0));`.

**Verify**: `timeout 300 cargo test gaps_ -- --test-threads=1` passes all. Then
`timeout 300 cargo test div -- --test-threads=1` passes.

### Step 7: Grammar, docs, and the village helper

1. `timeout 60 ./target/release/j3c dump-syntax ../editors/vscode/syntaxes/j3.tmLanguage.json`.
   The diff adds `%` to the arithmetic operator pattern.
2. Do not edit `AGENTS.md` or `EDITOR.md`. The reviewer updates MCP Fixes item 13.
3. `examples/editor_village/src/player.j3`: delete `has` (lines 113-123), add `use j3.text;`,
   and replace `has(` with `text.contains(`.
   Then run `timeout 120 ./target/release/j3c check ../examples/editor_village`, which should exit 0.
   Skip this sub-step if the directory is gone.

**Verify**: `timeout 600 cargo test` exits 0.

## Test plan

- `gaps_rem_runs_on_both_backends` locks int/float `%`, negative truncation, `MIN % -1 == 0`
  (it was `MIN` in C before), const folding, and the libm link for `frem`/`fmod`.
- `gaps_rem_by_zero_aborts` / `gaps_const_rem_by_zero_is_error` lock the `/0` rule for `%`.
- `gaps_text_find_runs` covers match, miss, empty needle, empty haystack, and a needle longer than the rest.
- `gaps_module_const_runs` covers `mod.CONST` in a return (via `peek_ty`), in a `let`, and in arithmetic.
- `gaps_module_const_missing_is_named` covers the error text.
- Existing: `signed_div_min_is_a_branch`, `div_by_zero_aborts`, `const_div_by_zero_is_error`,
  `fast_keeps_div0`, `both_backends_run_01_through_08`, `mesh_fmt_j3_runs` (it already does `use j3.gfx.mesh;`).

## Done criteria

- [ ] `j3c run examples/honesty/rem.j3` and `j3c run --emit c examples/honesty/rem.j3` print `ok`
- [ ] `cargo test gaps_` passes; `cargo test` exits 0
- [ ] `j3c doc j3.text` shows `find` and `contains`
- [ ] `let k = mesh.STRIDE;` typechecks with only `use j3.gfx.mesh;`
- [ ] `%=` still gives a parse error (not silently accepted)
- [ ] No file outside Scope changed
- [ ] `plans/README.md` not modified

## STOP conditions

- An excerpt in Current state no longer matches (for example, `BinOp::Rem` is gone from the IR, or
  `fold_const_bin` has a different shape). STOP and describe it.
- `rem.j3` gives different stdout on LLVM and C after Step 3. STOP. Do not change the expected values.
- The link fails with `undefined reference to fmod`/`fmodf` even with the marker. STOP. Report
  the link command (`J3C_TIMINGS=1`, plus the clang stderr).
- Dotted const keys show up in LSP completion or `j3c doc` output. STOP. The fix belongs in
  that listing (filter `.`), and must not become a new map unless the reviewer agrees.
- A stdlib or example module stops compiling because a local/param shares a module alias name
  and now resolves differently. STOP and name it.

## Maintenance notes

- `%` follows `/` everywhere. Anyone who adds `/=` should add `%=` in the same change, with
  `abort_if_zero` in AssignOp lowering.
- The libm marker is a text scan (`" frem "`, `"__builtin_fmod"`). If printers start emitting
  those strings for other reasons, switch to a flag set in the printer.
- Module consts live in `consts` under `"alias.NAME"`. Anything that lists `consts` keys to
  users must filter names containing `.`. Const initializers (`collect_consts`) run before
  `use` resolution, so `const X = mesh.STRIDE;` is still unsupported.
