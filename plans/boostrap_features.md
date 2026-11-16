# Plan 08: language and stdlib features for a self-hosted j3c

Status: DONE (2026-11-13). Every Part A and Part B item shipped (`j3c doc lang`, `speculative/lang/`,
`roadmap.md` Shipped). Still open, by choice or as follow-ups: the j2 AST stays a flat `Node` array (kinds
and ops are enums; see chunk notes in git history); no numeric trait, so arithmetic and casts on `T` are
checked per instance, not at the definition; the frame-string escape check assumes calls through fn
values, closures, `dyn` tables and C functions keep nothing; `j3.io` fs/stdio and `proc.waitAny` are
Linux only (ENOSYS stubs in `sys_windows`/`sys_macos`). The port of `j3c` itself is a later plan.

Goal: make J3 pleasant enough to rewrite `j3c` (about 75k lines of Rust in `j3c/src/`) in J3 itself.
This plan covers only what the language and stdlib need first. The port is a later plan.

Evidence that these gaps are real: `j3c/stdlib/j3/j2/` is a lexer, parser, compiler and VM written in
J3 (about 6.6k lines), and it already works around most of them. Node kinds are `pub const N_*: int`
on one fat `struct Node { kind, op, a, b, c, first, count, i, f, s }` instead of an enum
(`j2/parse.j3`). Characters are raw codes (`ch == 34`, `ch == 39` in `j2/lex.j3`). Read j2 before
designing each item, and port one j2 piece to the new feature as its proof (see each "Done when").

Rules for every item:
- Both backends (LLVM in `src/emit/llvm*`, C in `src/emit/c*`) produce the same output. The bytecode VM
  (`src/emit/bc`) either supports the feature or rejects it with a clear `.j3s` error.
- Add a `speculative/lang/<feature>.j3` sample and document it in `j3c doc lang`
  (`src/doc/pages/` / `src/helptext/`). `j3c fmt`, `j3c dump-syntax` (VS Code grammar) and `j3c lsp`
  learn the new syntax in the same change.
- Do not start a second parser. Extend `src/parse/`.
- Run `cargo test` from `j3c/`, then `./build.sh` when done.

Where the compiler stages live: lexer `src/parse/lex.rs` + `token.rs`, AST `src/parse/ast.rs`, parser
`src/parse/parser/`, checker `src/check/` (`types.rs`, `infer.rs`, `traits.rs`, `generic_fn.rs`,
`closure_ty.rs`), lowering to IR `src/check/lower/` (`enums.rs`, `stmt.rs`, `expr.rs`, `closure.rs`,
`interp.rs`), IR `src/ir/`, and the emitters in `src/emit/`.

---

## Part A: must-have

### A1. `match` as an expression, plus `if let` and `let else`

Today `match` is a statement only (`MatchStmt` in `ast.rs`), so producing a value means declaring a
`let` and assigning it in every arm.

Design:
- `let n = match op { Op.Add { 1 } Op.Sub { 2 } };` The arm block's tail expression is the arm's value,
  the same rule as `if` used as a value. Every arm must produce the same type. An arm that ends in
  `return`/`break`/`continue`/`panic` has type "never" and fits any type.
- `if let Option.Some(x) = find(k) { .. } else { .. }`, usable as a value just like `if`.
- `let Option.Some(x) = find(k) else { return Result.Err(..); };` The `else` block must diverge.
- Keep `match` as a statement too (a value of type `void`).

Touchpoints: add `Expr::Match`, `Expr::IfLet` and `Stmt::LetElse` to the AST. The checker's arm type
unification can follow the existing if-expr path. Lowering reuses the statement-match lowering in
`lower/enums.rs` and writes into a temp like if-expr does. Both emitters need nothing new if lowering
produces plain IR.

Tests: value match on an enum, an int and a payload; a diverging arm; `if let` with and without
`else`; `let else` with a non-diverging `else` (error); mismatched arm types (error naming both arms).

Done when: j2's parser dispatch can be written as `let node = match tok.kind { .. }`.

### A2. Wildcard `_` on enums, or-patterns, guards

Today an enum `match` must list every variant and `_` is rejected. A compiler's token, AST and IR
enums have 50 to 150 variants, and most passes care about a few.

Design:
- `_ { .. }` is allowed on an enum match. Keep the exhaustiveness check for matches without `_`. Add a
  lint (not an error) when `_` covers no variant.
- Or-patterns: `Tok.Plus | Tok.Minus { .. }`. Every alternative binds the same names with the same
  types.
- Guards: `Expr.Lit(n) if n == 0 { .. }`. A guarded arm does not count toward exhaustiveness.
- Ranges in int matches: `48..=57 { .. }` (pairs with A4 char literals: `'0'..='9'`). Check that ranges
  don't overlap each other or literal arms.

Touchpoints: `MatchArm` gets `pats: Vec<Pat>` and `guard: Option<Expr>`. The exhaustiveness check
lives where the "lists every variant" error is raised (search `src/check/` for it). An int match
currently compiles to a `switch`; ranges and guards fall back to an if-chain, or the emitter expands
small ranges into cases.

Tests: `_` with 1 and N missing variants; or-patterns with matching and mismatched bindings (error);
a guard plus a fallthrough arm; overlapping ranges (error); identical output on LLVM and C.

### A3. Multi-field variants and struct patterns

Today an enum variant has at most one payload (`payload: Option<Pat>` in `ast.rs`), so
`Bin(op, lhs, rhs)` needs a wrapper struct. Struct fields can't be destructured in a pattern.

Design:
- `enum Expr { Bin(BinOp, &Expr, &Expr), Lit(int), Name(string) }`, matched with
  `Expr.Bin(op, l, r) { .. }`. Nested patterns and `_` work in any position.
- Struct patterns: `Expr.Call(CallExpr { callee, args }) { .. }` and `let Span { lo, hi } = s;`. Use
  `..` to skip the rest (`Node { kind, .. }`).
- Optional: named variant fields (`Bin { op: BinOp, l: &Expr, r: &Expr }`). Do this only if
  positional fields are clearly not enough.

Touchpoints: the enum layout in `ir/ty.rs` and `ir/abi.rs`, where the payload becomes a struct of the
fields (keep alignment rules: the enum is 16-aligned if a field is a vector). `Pat::Variant` takes
`Vec<Pat>`. Add `Pat::Struct`. C and LLVM type printers (`emit/c_ty.rs`, `emit/llvm_ty.rs`).
`#[Debug]` printing of multi-field variants. `#[TryCatch]` keeps working with single-payload variants.

Tests: a 3-field variant built, matched and printed; a nested variant inside a variant; a struct
pattern with `..`; the size and layout of an enum mixing `f32`, `vec4f` and pointers on both backends.

### A4. Char literals

Today `'` only starts a loop label (`'rows:`), so a lexer compares against magic numbers.

Design: `'a'`, `'\n'`, `'\''`, `'\\'`, `'\x7f'`, `'\u{1F600}'`. A char literal has type `u8` when it
is ASCII and is used as a byte. A non-ASCII char (or `'\u{..}'`) is a `u32` code point. Like number
literals, it takes the type of the other operand (`ch == 'a'` with `ch: u8`).

Lexing ambiguity: `'x'` is a char when the closing `'` comes right after one char or one escape.
Otherwise `'ident` is a label. Labels are only valid before `while`/`for` and after
`break`/`continue`, so the lexer can decide by lookahead without context.

Touchpoints: `lex.rs` (new token kind), `token.rs`, literal typing in `lower/lit.rs`,
`j3c dump-syntax`.

Tests: every escape; `'a'` vs `'rows: for`; a char in a match arm and in an A2 range; a too-long
literal (error). Done when `j2/lex.j3` uses char literals instead of `34`, `39`, `92`.

### A5. Arena-friendly recursive data and typed handles

Pointers in fields already work (`mem: &Arena` in `ui/dom.j3`), but building a tree node is verbose,
and index-based IRs (the Rust IR uses ids, `src/ir/ids.rs`) have no type safety in J3: j2 uses bare
`int` for node references.

Design:
- `arena.new(value): &T` allocates one `T` and copies `value` in:
  `let e = mem.new(Expr.Bin(op, l, r));`. This is a stdlib generic method on `Arena` and needs no
  language change unless generic inference fails. Fix inference if it does.
- Typed handles: a stdlib generic `struct Id<T> { i: u32; }` plus a `Pool<T>`/`Table<T>`
  (List-backed) with `add(v): Id<T>` and `get(id): &T`. `Id<Expr>` and `Id<Stmt>` must not mix.
  Needs `operator==` and `hash` so handles can be map keys.
- Check that a recursive type through a pointer (`enum Expr { Bin(&Expr, &Expr) }`) type-checks, gets
  a layout, and prints in `#[Debug]` without infinite recursion (print pointers as addresses, or stop at
  a depth).

Tests: build and walk a 10k-node tree; `Id<A>` passed where `Id<B>` is expected (error); a handle used
as a `Map` key.

### A6. A growable string builder and formatting without the 512-byte cap

Today string interpolation aborts past 512 bytes (exit 102). `j3.text.Text` grows, but formatting into
it is `t.add("n = ").add(`{n}`)`, still through the capped interpolation. Code generation writes
megabytes of LLVM IR or C.

Design:
- `t.fmt(`define {ty} @{name}(`)` writes the interpolation straight into a `Text` with no
  intermediate buffer and no limit. The compiler lowers interpolation into "append each part to a
  destination", and the destination is either the 512-byte stack buffer (today's behaviour) or a
  `&Text`.
- `Text` gains `pushU64`, `pushHex`, `pushF64` (see B5 for exact round-trip), `pushChar` (a code point
  encoded as UTF-8), `indent(n)` and `pushLine`.
- A writer pattern for big outputs: `Text.flushTo(fd)` once it passes N KB, so emitting a 50 MB `.ll`
  file doesn't keep it all in memory.

Touchpoints: `lower/interp.rs` (a destination parameter), `stdlib/j3/text.j3`.

Tests: one 10 MB build that stays under a set memory limit; interpolation of every primitive and of
`#[Debug]` structs into `Text`; the output is byte-identical to today's capped path for short strings.

### A7. Embed files at compile time

Today the stdlib, doc pages and recipes are `include_str!` in Rust (`src/stdlib/files.rs`,
`src/helptext/`). A J3 j3c needs the same.

Design:
- `const SRC: string = @embed("stdlib/j3/io.j3");` makes the file a read-only string in static data.
  The path is relative to the source file (also allow the package root, the same way `@script` does).
  `@embedBytes(..)` gives `[]u8`.
- `@embedDir("stdlib", "*.j3")` gives a `const` table of `{ path: string, data: string }`, so the
  whole stdlib embeds in one line.
- The incremental build (`src/emit/incr*`) tracks embedded files as inputs so editing one rebuilds.

Touchpoints: a new intrinsic in the checker and the const evaluator (`check/consteval.rs`), emitters
for big byte arrays (LLVM `c"..."`, C a `static const unsigned char[]` with length, keeping
`.rodata` sized right), and a dependency entry in `incr`.

Tests: embed a binary file with NUL bytes and confirm an exact round-trip; embed a missing file
(error naming the path); edit the file and confirm a rebuild.

### A8. Collections for compiler workloads

Today `Map<K, V>` (`stdlib/j3/data/map.j3`) has `new get getOr has put remove clear` and slot-index
iteration (`live`/`keyAt`/`valAt`). `List<T>` has `push remove swapRemove`. `j3.cmp` has
`contains`/`indexOf` on slices. Missing pieces:

- `List`: `pop`, `last`, `insert`, `clear`, `extend(slice)`, `indexOf`/`contains` with `T: Eq`. This
  needs **lazy generic-struct methods**: a method's body is checked only for instances that call it,
  so `List<SomeNonEqType>` still compiles. This is a checker change in `check/generic_fn.rs` /
  `check/traits.rs` (the roadmap lists it as the blocker for `List.indexOf`).
- `Map`: an iterator form (`for (k, v) in m.entries()` once B3 tuples exist, or `m.each(fn(k, v))`
  with a closure), `getPtr(k): &V` for in-place updates, `free`, `len`.
- `Set<T>` on top of `Map<T, void>` or its own storage.
- `sort<T: Ord>(xs: []T)` and `sortBy(xs, fn(a: T, b: T): bool)`. Must be stable, so emitted output is
  deterministic.
- An ordered map (`SortedMap<K: Ord, V>`) or `Map.sortedKeys(arena)`, for deterministic output where
  Rust uses `BTreeMap`.
- `Interner`: `string` to `u32` symbol and back, backed by one arena.

Tests: each container under a 1M-op randomized test against a naive reference; `sort` stability;
`List<T>` of a type without `operator==` still compiles when `indexOf` is never called.

---

## Part B: strongly wanted

### B1. Dynamic dispatch

The backends (LLVM, C, GLSL, bytecode) and the LSP and MCP request handlers want "one interface,
several implementations". Today the options are a giant enum plus a match in every function, or a
hand-written struct of `fn` pointers taking a `ptr`.

Design (smallest that works): `dyn Trait` as a fat pointer (data `ptr` plus vtable). `let b: dyn
Backend = &llvmBackend;` builds the vtable from the trait's signatures at the conversion point, the
same structural matching traits already use. Calls go through the vtable. No generics inside a `dyn`
trait's methods. `this: &Self` only.

Touchpoints: the trait checker (`check/traits.rs`), one vtable global per (type, trait) pair in IR,
both emitters. `j3c doc lang` currently says "No `dyn` traits", so update it.

Tests: two implementations behind one `dyn`; a `List<dyn Backend>`; a type missing a method (error
naming it); identical output on both backends.

### B2. Escaping closures

Today a closure points into its creator's stack frame, so it can't be a field, a return value, a list
element or a thread job. That blocks visitors stored on a pass, callback tables and pool jobs with
captured state.

Design: an explicit, arena-backed escaping closure, keeping the current stack closure as the default.
`fn[mem](x: int): int { return x + k; }` copies the captured values into `mem` (by value, not by
address) and has type `boxed closure(int): int`, which can go anywhere a value can. It never frees by
itself. Its lifetime is its arena's. `thread.Pool.submit` gets an overload taking one.

Touchpoints: `check/closure_ty.rs`, `lower/closure.rs`, `ir/closure.rs`, the closure calling
convention in both emitters, and the restriction list in `j3c doc lang`.

Tests: return a boxed closure from a fn and call it later; store several in a `List`; run one as a
pool job; capture-by-value semantics (a later write to the original isn't seen).

### B3. Tuples and multiple returns

Checkers return pairs all the time (`(Ty, Span)`, `(value, rest)` in parsers).

Design: `(T, U)` types, `(a, b)` values, `.0`/`.1` access, destructuring `let (ty, span) = f();` and in
patterns (`Option.Some((k, v))`). Layout is a plain struct. No variadic tuples, no tuple traits beyond
`==`/`hash` when every element has them.

Touchpoints: the type grammar, an anonymous struct type in `ir/ty.rs` (interned by element types so
`(int, int)` is one type program-wide), patterns from A3.

Tests: return, destructure, nest, use as a `Map` key, and print with `#[Debug]`.

### B4. Traits checked against bounds, default methods, supertraits, derives

Today trait bodies are checked per instance (the roadmap lists this). There are no default methods,
no supertraits, and no derives beyond `#[Debug]`. A self-hosted IR has hundreds of types needing
`==`, `hash` and debug printing.

Design, in this order:
1. `#[derive(Eq, Hash, Ord, Debug, Clone)]` on structs and enums, generating the methods
   field by field. Biggest payoff, independent of the rest.
2. Default methods in a trait (`fn ne(this: Self, o: Self): bool { return !this.eq(o); }`).
3. Supertraits (`trait Ord: Eq`).
4. Checking generic bodies once against their bounds. Needs a placeholder type through lowering. Do
   this last: it improves error messages and compile time, not expressiveness.

Tests: derived `==`/`hash` agree with hand-written ones; `Map<DerivedKey, V>`; a generic body using a
method outside its bounds (error at the definition, once item 4 lands).

### B5. Text and number utilities

Today `j3.text` has `same`, `find`, `contains`, `fmtInt`, `fmtBytes`, and `Text`. Parsing and UTF-8
helpers are scattered (`data/json/parse.j3`, `j2/lib_text.j3`, `net/http/url.j3`).

Add to `j3.text` (pull in the existing private copies and delete them):
- `startsWith`, `endsWith`, `trim`/`trimStart`/`trimEnd`, `split(s, sep)` as an iterator or into an
  arena, `lines`, `replace`, `lastIndexOf`.
- `parseInt(s, base)` / `parseU64` / `parseF64`, returning `Option`, with overflow detection.
- UTF-8: `decodeRune(s, i): (u32, int)` (needs B3, or returns a struct), `encodeRune(t, cp)`,
  `isValidUtf8`, and code-point iteration (`for cp in s.runes()`).
- Exact float printing: shortest round-trip `f64`/`f32` to text (Ryu or Grisu with fallback). The
  IR printer needs bit-exact floats, and `Text.pushFloat` takes `f32` only today.

Tests: a round-trip of 1M random `f64`s through print and parse; UTF-8 against invalid sequences; split
edge cases (empty, leading or trailing separator).

### B6. OS, process and IO gaps

Already present: `io.readFile/writeFile/listDir/stat/rename/remove/makeDirs`, `sys.env/exit/arg`,
`sys.read` on fd 0, `j3.proc.run/start`, and `thread.Pool/Queue/Mutex`. Missing pieces:
- Buffered stdin and stdout (`io.stdin().readLine(..)`, `io.stdout()` with `flush`) for the LSP and
  MCP stdio loops, which use `Content-Length` framing. Today that's raw `sys.read`/`sys.write`.
- Atomic file replace (`io.writeFileAtomic`: write a temp file, then `rename`), temp files and dirs
  (`mkstemp`/`mkdtemp`), and file locks (`flock`) so two `j3c` processes don't corrupt
  `~/.cache/j3`.
- `io.copyFile`, recursive `listDir`/walk with a glob filter, and `realpath`/path join and normalize
  helpers (`j3.io.path`).
- Running N `clang` processes in parallel and waiting on whichever finishes first: `proc` has
  `start` and `waitpid`, so add `proc.waitAny(children)`.
- Windows and macOS keep stubs (plans 04 and 05). Only Linux has to work for the bootstrap.

Tests: two processes taking the same lock; an atomic write killed halfway leaves the old file intact;
a stdio echo server driven by a test harness.

### B7. Content hashing

The incremental cache (`src/emit/incr*`) keys objects by content. `j3.cmp.hash` is a 64-bit mixer
meant for hash tables, not a stable file fingerprint.

Add `j3.hash`: `fnv1a64(bytes)`, `xxh3_64(bytes)` (fast, stable across runs and platforms), and
`sha256(bytes)` (for git deps and `j3.lock`, which may already compare git hashes). Each should also
work incrementally (`Hasher.update`/`finish`).

Tests: the published test vectors for each algorithm.

---

## Order

1. A4 char literals, A2 `_`/or/guards/ranges (small, unblock lexers right away).
2. A3 multi-field variants, then A1 match expressions (A1's patterns build on A3).
3. A6 string builder and A7 embed (codegen and stdlib packaging).
4. A8 collections, including lazy generic-struct methods, plus A5 arena helpers and handles.
5. B3 tuples (B5 and A8 iteration get nicer with them), B4 derives.
6. B5 text utilities, B6 OS gaps, B7 hashing (stdlib only, can run in parallel with steps 1 to 5).
7. B1 `dyn`, B2 escaping closures (biggest design surface, least urgent).

Proof at the end: rewrite `j3c/stdlib/j3/j2/lex.j3` and `parse.j3` with real enums, char literals,
match expressions and tuples. If that rewrite is shorter and clearer, the language is ready to start
porting `j3c`, beginning with its lexer and parser (`src/parse/`).

---

## Chunks

1. [x] A4 char literals + A2 `_`/or-patterns/guards/ranges. Done when: tests for each pass on both backends, `j2/lex.j3` uses char literals.
2. [x] A3 multi-field variants + struct patterns (+ `#[Debug]`). Done when: 3-field variant, nested, struct `..` tests and layout test pass on LLVM and C.
3. [x] A1 `match` expr, `if let`, `let else`. Done when: listed tests pass; j2 parser dispatch written as `let node = match ..`.
4. [x] A6 `Text.fmt` interpolation into Text + push helpers + flushTo, A7 `@embed`/`@embedBytes`/`@embedDir` + incr tracking.
5. [x] A8 lazy generic-struct methods + List/Map/Set/sort/SortedMap/Interner, A5 `Arena.new`, `Id<T>`/`Pool<T>`, recursive Debug.
6. [x] B3 tuples + B4 derives, default methods, supertraits (bounds-checking generic bodies last, if feasible).
7. [x] B5 text/number/UTF-8/float round-trip + B7 `j3.hash` (fnv1a64, xxh3_64, sha256).
8. [x] B6 OS gaps: buffered stdio, atomic write, temp, flock, copyFile, walk, `j3.io.path`, `proc.waitAny`.
9. [x] B1 `dyn Trait`.
10. [x] B2 escaping (arena-boxed) closures + Pool.submit overload.
11. [x] Proof: rewrite `j2/lex.j3` and `j2/parse.j3` with enums, char literals, match exprs, tuples.
12. [x] File-size pass: every `.rs`/`.j3` in `j3c/src`, `j3c/stdlib`, `j3c/editor` under 600 lines, split at real seams, organized neatly.

### Follow-up chunks (gaps found during 1-12)

13. [x] Bugs: `j3c fmt` mangles `<<`/`>>`/`#[`/generic `<T>` spacing; interpolated string returned from a fn dangles (stack buffer).
14. [x] Literal/pattern gaps: `<<=`/`>>=` compound assigns, string literal match patterns, zeroed local arrays (`let buf: [64]u8;`), int literals > i64::MAX for u64, exponent float literals (`1e9`), `for x in` over custom iterators (`for cp in s.runes()`).
15. [x] B4 item 4: check generic bodies once against their bounds (definition-site errors).
16. [x] Small gaps: u64 consts > i64::MAX as match arms, BigInt inside const exprs, j3.text Split/Lines on the iterator protocol, derive on generic enums, `dyn Sub` -> `dyn Super` upcast, boxed closures in globals/consts, C backend returning `[N]T`, LSP hover/goto on `dyn` types and char literals.
