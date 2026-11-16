use super::*;

// Guide pages that are plain text live in pages/*.md.
pub(super) const TASKS: &str = include_str!("pages/tasks.md");
pub(super) const J3UI_PAGE: &str = include_str!("pages/j3ui.md");
pub(super) const CSS_PAGE: &str = include_str!("pages/css.md");
pub(super) const TESTING_PAGE: &str = include_str!("pages/testing.md");
/// `j3c doc threads`: the long-form guide. `j3.thread`'s own header stays short because the
/// module index prints it.
pub(super) const THREADS_PAGE: &str = include_str!("pages/threads.md");
/// `j3c doc term`: the `j3.term` stack (emulator and TUI directions, restore hook, limits).
pub(super) const TERM_PAGE: &str = include_str!("pages/term.md");
pub(super) const I18N_PAGE: &str = include_str!("pages/i18n.md");
/// `j3c doc ship`: release builds and `[dist] build_outputs` (`src/dist.rs`, `src/dist/deb.rs`).
pub(super) const SHIP_PAGE: &str = include_str!("pages/ship.md");
/// `j3c doc packages`: `[lib]`, path and git deps, `j3.lock`, fetch/update/add (`src/project/deps.rs`, `src/cli/pkg.rs`).
pub(super) const PACKAGES_PAGE: &str = include_str!("pages/packages.md");
/// `j3c doc shaders`: shader fns, `#[vertex]`/`#[varying]`/`#[uniform]`, `@shader`, `j3c dump-glsl` (`src/check/gpu/`).
pub(super) const SHADERS_PAGE: &str = include_str!("pages/shaders.md");
/// `j3c doc android`: APK builds, `[android]` keys, running on a device, and the limits (`src/dist/apk.rs`,
/// `src/android_run.rs`, `src/emit/android.rs`, `tools/android/build-sdl.sh`).
pub(super) const ANDROID_PAGE: &str = include_str!("pages/android.md");
/// `j3c doc j2`: j2 runtime scripts, host side; its `Language` section is `j3c doc j2 lang` (`stdlib/j3/j2*`).
pub(super) const J2_PAGE: &str = include_str!("pages/j2.md");
/// `j3c doc steam`: where Steam went (the `steam` package: the dependency line, `j3c pkg steam`, the `steam` dist
/// output, `[steam]`); the API guide is the package's README.
pub(super) const STEAM_PAGE: &str = include_str!("pages/steam.md");

/// `j3c doc urp`: the URP guide, embedded so it matches this j3c build.
pub(super) fn urp_page() -> String {
    format!(
        "`j3c doc urp` — URP render passes

In an editor scene, URP is a scene setting (`j3c -e scene set @scene urp=true`) and
`scene3d.draw` traces it with no extra calls (`j3c doc editor`, URP). The modules below are
separate renderers that a game drives itself.

{}",
        include_str!("../../stdlib/j3/gfx/urp/README.md")
    )
}

pub(super) fn names(list: &[&str]) -> String {
    list.iter().map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ")
}

pub(super) fn lang_page() -> String {
    let keywords: Vec<&str> = keyword_names().collect();
    format!(
        "`j3c doc lang` — the language

`int` is i64. `float` is f64. Gameplay math uses `f32`. A number with no context is `int`, or `f64` when it has a dot. `let` is mutable. A local's type is inferred when you omit it. A `let` ends with its block: a sibling block can reuse the name, but shadowing a name still in scope is an error.

A number literal takes the other operand's type, on either side: with `far: f32`, `2 * far`, `(2 + 1) * far` and `far * 0.5` are `f32`. With no typed operand it takes the destination's (`let h: f32 = 1.5 + 2;`); int-only literal math folds as `int` first, so `let q: f32 = 7 / 2;` is `3`. A float literal never becomes an int; `1e9`, `2.5e-3`, `1E+10` are float literals too. An int literal above `i64::MAX` (`0xFFFFFFFFFFFFFFFF`, `18446744073709551615`) is accepted only where a `u64` is expected (a typed `let`, `const NAME: u64`, a field, an argument); elsewhere it is an error. A `const NAME: u64` folds on all 64 bits (`0xFFFF_FFFF_FFFF_FFFF & X` works; `/ % >>` are unsigned), matches as an int arm above `i64::MAX`, and with no other type to take stays a `u64`. A variable never converts: with `n: int`, `far * n` is a type error; write `far * f32(n)`. `f32(x)`, `int(x)`, and friends convert explicitly.

`const NAME = value` is a module constant. The value is an int expression (`+ - * / %`), a float, a bool (`true`, `false`, or `!` of one), or a string. `-1.5` is the float it looks like.
`const NAME: [N]T = [a, b, c];` is a read-only table in static data (ints, floats, bools, strings, structs of those; nested `[2][2]int` too). It needs its type; a wrong length is a compile error. `NAME[i]` is bounds-checked, `NAME.len`, `for x in NAME`, `NAME[a..b]` and passing it as a `[]T` all work. `NAME[i] = v`, `NAME[i].f = v`, `+=` and `&NAME[i]` are compile errors. A `[]T` made from it is not read-only: a callee that writes through the slice changes the table (it is ordinary module data), so keep such callees read-only. It is private to its module (not importable with `use`).

One file is one module. `use j3.io.printLn` brings in one name. `use j3.memory.{{ Arena, List }}` brings in several. `pub` is what another module can name. A module can be spread over several files: the files in the folder beside `X.j3` (`X/*.j3`) that say the same `module` line are more of it and share its names and `use`s (put the `use`s in `X.j3`).

`if` and `while` take no parentheses. Braces are required. An `if` used as a value needs a final `else`; `else if` chains work (`let x = if a {{ 1 }} else if b {{ 2 }} else {{ 3 }};`), and only the taken arm runs. `&&` and `||` stop at the first decisive operand.

`for i in 0..n` stops before `n`. `for i in 0..=n` includes `n`. `for n in xs` walks a slice or an array. The index name ends when the loop ends. `for x in it` also takes an iterator: a struct with `fn next(this: &Self): Option<T>` (`for cp in s.runes()`). The iterator expression runs once; a call result is a temporary, a local is advanced in place (`for cp in &r` keeps `r.at` readable). `j3.text` has `split(s, \",\")` and `lines(s)` as iterators (`for part in split(s, \",\") {{ }}`). `break`, `continue` and labels work as in any loop; nothing is allocated per step.

`let buf: [64]u8;` (a type and no value) starts as zeros: numbers 0, bools `false`, pointers null, strings empty, arrays and structs of those (field defaults do not apply). A reference or enum has no zero value, so it needs a value. `x <<= n` and `x >>= n` shift in place like `+=`. Example: `speculative/lang/literals.j3`.

`match` lists every variant, or ends with `_ {{ }}` for the rest (a `_` that covers none is a warning; an arm after `_` is an error). Write `Enum.Name`, not a bare name. A C-like enum (no payloads) compares with `==`, `int(e)` is its variant's index, and a `const` table may hold its variants (`const KW: [2]Tok = [Tok.Let, Tok.Fn];`). A `match` on an integer takes int or char literals or int `const`s as arms and needs `_` (`match op {{ 0 {{ }} -1 {{ }} OP_ADD {{ }} _ {{ }} }}`); a repeated value is an error. A `match` on a `string` takes string literals and needs `_` (`match w {{ \"let\" {{ }} \"fn\" | \"func\" {{ }} _ {{ }} }}`; length first, then bytes, arm by arm). `A | B {{ }}` lists alternatives, each binding the same names with the same types (`Shape.Dot(n) | Shape.Pair(Tok.Num(n))`). `'a'..='z' {{ }}` is an inclusive range; ranges and values may not overlap. `Tok.Num(n) if n < 0 {{ }}` is a guard, checked with the names bound: when false, the later arms are tried, and a guarded arm does not count toward every variant. Single values with no guards compile to a `switch`, the rest to an `if` chain. Example: `speculative/lang/patterns.j3`.

A variant holds several positional fields: `enum Expr {{ Bin(BinOp, &Expr, &Expr), Lit(int) }}`, built `Expr.Bin(op, l, r)` and matched `Expr.Bin(op, l, r) {{ }}`; the count must match. Patterns nest in any field (`E.Wrap(Inner.Pair(x, _))`), a literal tests a field (`E.Bin(_, 0, _)`, `true`/`false` on a bool). `Span {{ lo, hi: h }}` destructures a struct (`lo` is short for `lo: lo`); without a trailing `..` every field is listed. `let Span {{ lo, .. }} = s;` destructures without `match`; its pattern must always match (names, `_`, struct patterns). A variant counts as covered only when its arms cover every value of its fields; the error names an example such as `E.Bin(Op.Sub, _, _)`. `#[Debug]` enums print as `Expr.Lit(3)`; an array field prints as `[3, 4]` (up to 64 elements); a pointer field prints as its type (`&Expr`), so recursive types end. Example: `speculative/lang/variants.j3`.

Tuples: `(int, string)` is a type, `(a, b)` a value (two or more elements; `(x)` is just `x`), `t.0` / `t.1` its elements (`t.0.1` nests). `fn divmod(a: int, b: int): (int, int) {{ return (a / b, a % b); }}` returns two values; `let (q, r) = divmod(7, 2);` destructures, and tuple patterns nest and go anywhere a pattern does: `match` arms on a tuple (`(Op.Add, 0) {{ }}`, checked for exhaustiveness), inside a variant (`Option.Some((k, v))`), and `for (k, v) in xs`. A tuple is a plain struct, one type per element list in the whole program. `==`, `!=` and `hash()` work when every element has them (element by element), so a tuple can be a `Map` key; interpolation prints `(1, x)`. No other operators or traits. In `.j3s` scripts `hash()` of a primitive element is missing (scripts cannot call `#[impl]` fns). Example: `speculative/lang/tuples.j3`.

`match` is also a value: `let n = match op {{ Op.Add {{ 1 }} Op.Sub {{ let k = 2; k * 3 }} }};`. An arm's last expression, with no `;`, is its value, and every arm gives the same type (the error names both arms). An arm that leaves (`return`, `break`, `continue`, `panic`) gives no value and fits any type; any other arm without a value is an error. A `match` at the start of a statement is a statement (no `;`, its arm values are run and dropped). `if let Shape.Rect(w, h) = s {{ .. }} else {{ .. }}` is a one-arm `match` with a `_` arm for the `else`: its names are bound only in the first block, it chains with `else if let` / `else if`, and it is a value like `if` (then the `else` is required). `let Shape.Circle(r) = s else {{ return 0; }};` binds `r` for the rest of the block; the pattern must be able to fail, and the `else` must leave. Example: `speculative/lang/match_expr.j3`.

A char literal `'a'` is a `u8` when ASCII, else a `u32` code point (`'é'`, `'\\u{{1F600}}'`); like a number it takes the other operand's int type (`ch == 'a'` with `ch: int`). Escapes `\\n \\r \\t \\0 \\\\ \\' \\\"`, `\\x7f` (ASCII only), `\\u{{..}}`. A char is one character or escape then `'`; otherwise `'name` is a label, and `'ab'` is an error. Example: `speculative/lang/chars.j3`.

`defer` runs at the end of its block, including on `return`. `break` and `continue` act on the innermost loop. A label names an outer one: `'rows: for r in 0..h {{ for x in xs {{ if x == 0 {{ continue 'rows; }} }} }}` (`break 'rows;` too; labels go on `while` and `for`). The `defer`s between run first, innermost first. An unknown label, or one whose loop has ended, is a compile error.

A backtick string `hello {{name}}` fills the hole. A struct in a hole needs `#[Debug]`. The result lives in the calling function's stack frame and holds at most 512 bytes; past that the program stops (exit 102, \"string interpolation overflowed its buffer\"). So it cannot leave that function: returning it (directly, through a `let`, a branch or `match`, or inside a struct, array or tuple), storing it through a reference, a slice or into a global (except from `main`), or passing it to a function that keeps it at any depth (`List.push`, `Map.put`, your own fn that stores its param) is a compile error, with a note where the callee keeps it; text that outlives the call goes into a `Text` the caller owns (`fn label(t: &Text, n: int): string {{ return t.fmt(`n={{n}}`).str(); }}`). Calls through a fn value, closure, `dyn` method or C function are assumed to keep nothing. A hole alone, `{{s}}` with `s: string`, is `s` itself and has no limit. For longer text append to a `j3.text.Text`: `t.fmt(`define {{ty}} @{{name}}(`)` writes each part straight into `t` with no limit and no intermediate string (every primitive and `#[Debug]` value prints the same bytes as in a plain backtick string; a long `string` inside a `#[Debug]` struct is fine there). `fmt` returns `t`, so calls chain; `Text` also has `pushU64`, `pushHex`, `pushF64` / `pushF32` (the shortest text that reads back exactly), `pushChar` (UTF-8), `indent(n)`, `pushLine`, and `flushTo(fd)` to write a big output in pieces (`j3c doc j3.text`). Backtick strings take no `\\n` escapes: `t.pushLine(..)` or `t.push('\\n')` ends a line. `Text.scratch().add(..).str()` lasts until the next frame. Example: `speculative/lang/text_fmt.j3`.\n\n\
Files at compile time: `const SRC = @embed(\"data/help.txt\");` is the file as a read-only `string` in static data, `const BIN = @embedBytes(\"icon.png\");` a `[]u8` (any bytes), and `const STD = @embedDir(\"stdlib\", \"*.j3\");` a table `[N]{{ path, data }}` of every matching file below the folder (subfolders searched, dot entries skipped, sorted by `path`, which is `/`-separated below the folder; a pattern with `/` matches that whole path). Arguments are string literals. A path is relative to the source file, else to the package root; a missing file is a compile error naming it. Each `const` is one copy in the binary however often it is used; `@embed` / `@embedBytes` inside a function body work too, but `@embedDir` only as a `const` (the table type has no name: leave the `const` type off). Editing an embedded file rebuilds. A `.j3s` script can hold `@embed` and `@embedBytes` but not an `@embedDir` table. Example: `speculative/lang/embed.j3`.

`+=` `-=` `*=` `/=` `%=` write the place once. `%` is the remainder and truncates like C: `-7 % 3 == -1`; `math.wrapi(-7, 3) == 2` wraps an index. `<<` and `>>` shift integers (`1 << 20` works in a `const`; `>>` keeps the sign of a signed type; the amount is taken mod the bit width). `&` `|` `^` and unary `~` work on integers of one type (`x & 0xff` types the literal; `0b1010` too); `&=` `|=` `^=` exist. Tightest first: unary `- ! ~ &x`, `* / %`, `+ -`, `<< >>`, `&`, `^`, `|`, comparisons, `&&`, `||`, so unlike C `x & MASK == 0` is `(x & MASK) == 0`. `~` on an unsigned type keeps its width (`~u8(0) == 255`). `< <= > >=` take numbers; a `string` has no order (a compile error): compare bytes in a loop. `==` / `!=` on strings compare length then bytes (no allocation). Scripts (`.j3s`) have `& | ^ ~` but not shifts. `s as []u8` and `bytes as string` reinterpret without copying (`child.write(\"q\" as []u8)`). Integer overflow wraps. Divide or `%` by zero aborts. `--fast` strips that abort and the bounds-check aborts. A failed check prints the values (`index out of bounds: index 5, len 3`) and, in a dev build on x86-64 Linux, the `called from:` functions. A function with a return type must end every path in `return` (or `panic(..)`, or `while true` with no `break`): one that can run off its end is a compile error.

`operator+` is a method. `this` is the receiver, so the struct is the left operand: `v * 2.0`, not `2.0 * v`. `.` follows a pointer, and works on any value, not just a variable: `f().x`, `(a + b).x`, `xs[a..b].len`, `make().inner.field` (the value is held in a temporary, so you cannot assign to it). A method `this: &T` takes the address at the call. A field may end with `= value`. A struct literal that omits the field uses that value.

`extend T {{ fn m(this: &T) {{ .. }} }}` adds methods to a struct declared in the same module, from any file of it: the main file or a part file (`ui/dom.j3` + `ui/dom/*.j3` saying `module j3.ui.dom;`), so a big type's methods need no wrappers. Exactly as if written in the struct body (no `pub`: methods are public). A generic struct: `extend Box<T> {{ .. }}`, parameters named as in the struct. Errors: no such struct, a type of another module (`methods of Dom go in module j3.ui.dom`), an enum (use a free fn), a method already there, other parameter names. Not in `.j3s`. Example: `speculative/lang/extend.j3`.

UFCS: `x.f(args)` calls a free `f(x, args)` when the type has no method `f` (the method wins). `f` must be visible by bare name (declared here or `use m.{{ f }}`); its first parameter takes the receiver, or `&T` (the call takes the address). Works on primitives, slices and strings too: `far.abs()`.

Generic free fns: `fn first<T>(xs: []T): T`, `pub fn maxOf<T: Ord>(a: T, b: T): T`, `fn swap<T>(a: &T, b: &T)`. `T` is inferred from the arguments (a bare number takes the other argument's type, else `int`/`float`; a closure literal's written types count), or written `first::<int>(xs)`; `xs.first()` works through UFCS. Each distinct set of types is compiled once for the whole program. The body is checked once at the definition against the bounds (see Traits): an unbounded `T` may be stored, copied, passed and `@sizeof`'d, but `a > b` needs `T: Ord`. Other errors inside a body (arithmetic on `T`, a cast) are found per instance and name the call that instantiated it. Write the return type (it is not inferred), and a `T` seen only in the return type needs `::<..>`. Not callable from `.j3s` scripts. Example: `speculative/lang/generic_fns.j3`. A generic method (`fn new<T>(this: &Arena, value: T): &T`) infers its type args the same way: `mem.new(Expr.Num(1))`.

Collections for compiler-style code (`speculative/lang/collections.j3`): `List` (`push pop last insert remove swapRemove clear extend slice reserve indexOf contains`), `j3.cmp.sort` / `sortBy(xs, less)` (stable merge sort, O(n log n)), `Map` (`getPtr getOrPut each(fn(k, v)) entries(&arena) sortedKeys(&arena) free` beside `get put remove`; `for (k, v) in m.entries(&a)`, `*m.getPtr(k) += 1`), `j3.data.set.Set<T>`, `j3.data.intern.Interner` (string to `u32`), `Arena.new(value): &T` for trees of `&T` (a recursive enum through `&` is fine; `#[Debug]` prints a pointer as its type), and `j3.data.pool.Pool<T>` with typed `Id<T>` handles (`Id<A>` is not an `Id<B>`; ids are `Map` keys).

Traits: a bound on a type parameter, `fn contains<T: Eq>(xs: []T, x: T): bool`, `struct Map<K: Hash + Eq, V>`, `fn join<U: Ord>(..)` on a method. `trait Hash {{ fn hash(this: Self): u64; }}` is a list of signatures: `Self` is the type being checked, and the first parameter is `this: Self` or `this: &Self` (no generic traits). A method with a body is a default: `fn ne(this: Self, o: Self): bool {{ return !(this == o); }}`; a type that meets the trait's other methods but has no `ne` of its own gets that body (`x.ne(y)`, directly or under a bound; its own method wins; two traits giving one type a default of the same name is an ambiguity error). `trait Ord: Eq {{ .. }}` makes `Eq` a supertrait: meeting `Ord` needs `Eq`'s methods too (the error says which supertrait failed), so `T: Ord` may use `==`; `j3.cmp.Ord` is `Ord: Eq`. `#[derive(Eq, Hash, Ord, Debug, Clone)]` on a struct or enum writes those methods field by field (`==` every field; `<` the first field that differs, an enum by variant order then fields; `hash` the fields folded as a tuple of them hashes; `clone` a copy; `Debug` is `#[Debug]`): methods on a struct (on a generic one with `where A: Eq` for each parameter a field holds), `#[impl]` fns for a non-generic enum, and for a generic enum (`enum Opt<T>`) methods each instance has when its type arguments meet the trait (`Opt<int> == Opt<int>`, a `Map<Opt<int>, V>` key; `Opt<S>` with no `==` on `S` is the error). An error inside one (a field without `hash`) points at the derive. Example: `speculative/lang/derive.j3`. A type satisfies a trait by having the methods (no `impl Trait for T`). For a type you cannot edit write `#[impl] fn hash(x: int): u64 {{ .. }}`: a free fn found by `x.hash()`, one per (name, type), allowed in the trait's module or the type's module (the orphan rule); an own method wins. `==`/`<` bounds are met by built-in types too (numbers, `bool`, `string` and `ptr` for `operator==`, numbers for `operator<`; `j3.cmp` gives `string` a byte-order `operator<` by `#[impl]`, and `<` on strings calls it). The call or the `Map<K, V>` mention is checked, and the error names the type, the trait and the missing signature. A generic body is checked once at its definition, used or not: each type parameter is a stand-in that has only its bounds' methods (supertraits and default methods included), so `x.size()` or `a < b` outside the bounds is an error there (\"type parameter `T` has no `<`: its bounds `Hash` do not give it\", help \"add the bound: `T: Hash + Ord`\"), as is passing `T` to a generic whose bound it lacks, or reading a field of `T`. An unbounded `T` may be stored, copied, passed and `@sizeof`'d. Arithmetic, casts and anything else no bound can give are still checked per instance. A plain fn (UFCS) does not satisfy a bound. Methods of a generic struct are lowered per instance and only when something calls them (lazy generic-struct methods); a method needing more than the struct's bounds says so after its signature: `fn indexOf(this: &List<T>, x: T): int where T: Eq {{ .. }}` (`where` names the struct's parameters, only on generic-struct methods; a method's own parameters take `<U: Trait>`). `List<Tok>` of a type without `==` compiles; calling `indexOf` on it is the error (\"`Tok` does not satisfy `Eq` (needed by `T` of `indexOf`)\"). Errors inside an instance's body name the instance and the call (while instantiating `List<Tok>.indexOf`, called at ..). A plain struct's methods are always checked. Import traits like fns: `use j3.cmp.Eq;`. Without `operator!=`, `a != b` is `!(a == b)`; without `operator>`, `<=`, `>=` they use `operator<` (`a > b` is `b < a`, `a <= b` is `!(b < a)`); `Ord` assumes a total order (with NaN fields define `<=`/`>=` yourself); an explicit operator wins, and no `operator==` is still an error. Not in `.j3s` scripts (no `trait`, and `x.m()` there does not find `#[impl]` fns). Stdlib: `j3.cmp` (`Eq` `Ord` `Hash`, hashes for primitives and `string`, `hashOf` `contains` `minOf`) and `j3.data.map` (`Map<K, V>`). Example: `speculative/lang/traits.j3`.

`dyn Trait` is one interface, several implementations, chosen at run time: a data pointer plus that type's method table (16 bytes, by value). Make one from a reference: `let b: dyn Backend = &llvm;` (also a `&T` argument, field, return, or `List<dyn Backend>.push(&c)`); the type is checked against the trait then, like a bound, and the error names the missing method and the trait. `b.emit(3)` calls through the table; `&b` and fields of type `dyn` work the same. The table holds the trait's methods, its supertraits' (so a `dyn Backend` with `trait Backend: Named` has `name()`), and a default body where the type has no method of its own. One table per (type, trait) in the whole program. A `dyn` trait's methods take `this: &Self` and mention `Self` nowhere else (an error at the `dyn` use names the method); a generic method on the type does not count. `dyn` of an imported trait needs the trait imported (`use m.Backend;`). A `dyn Sub` converts to a `dyn Super` of any supertrait (`let n: dyn Named = backend;`, an argument too): the `Sub` table holds a pointer-returning entry for the same type's `Super` table, so it works whatever type is behind it; another trait is an error. `==` on two `dyn` values is an error: give the trait a method that says what equal means. The data is borrowed: a `dyn` must not outlive what it points at. Not in `.j3s` scripts or shader code. Example: `speculative/lang/dyn.j3`.

Methods live inside the struct body; there is no `fn Type.name(` outside it:

    struct State {{
        n: int;
        ys: [3]int;
        fn add(this: &State, k: int) {{ this.n += k; }}
    }}

`s.add(2)` calls it. A fixed-array field has no `= default` (a field default must be a literal), so write it in every struct literal: `State {{ n: 0, ys: [0, 0, 0] }}`. `List<T>` (`j3.memory`) has `push`, `remove(i)` (keeps order), and `swapRemove(i)` (O(1), moves the last element in); its `items` and `len` fields are public, and `list.items[0..list.len]` is the live slice.

`?` unwraps a `#[TryCatch(Ok, Err)]` enum: the failure variant returns from the function, the success variant continues. `j3.option.Option` and `j3.result.Result` are that attribute. The compiler does not special-case the names.

`#[singleton]` is one process value. Its initializer is a constant, and it cannot be a call. Add `#[thread]` for one value per thread (`j3c doc threads`).

`[N]T` is a stack array, copied by value: a fn may take and return one (`fn corners(): [4]int`). `[]T` is a pointer and a length. `xs[i]` checks the bound. `xs[1..3]` and `xs[:]` are views. `string` is a read-only byte view. Mutate bytes with `j3c doc j3.text`.

`use position: Vec2` on a struct embeds those fields. A statement `use transform` names them in this block, and assigning one writes the field.

Threads are `j3.thread`: hand a worker a struct only it writes, then read the result after a signal (`j3c doc recipes threads`, `j3c doc threads`). `#[thread]` and `@atomic*` are the language side.

`use lib \"SDL2\" as sdl {{ fn SDL_Init(flags: u32): i32; }}` declares C functions. `@syscall`, `@sizeof`, `@slice` and `@embed*` (above) are compiler calls. `@atomicLoad(p)`, `@atomicStore(p, v)`, `@atomicAdd(p, v)` / `@atomicSub(p, v)` / `@atomicExchange(p, v)` (return the old value) and `@atomicCas(p, old, new)` (returns whether it swapped) work through a `&T` for `int`, `i32`, `u32`, `u64`, `bool` and `ptr` (no add or sub on the last two); a last argument picks the order: `relaxed`, `acquire`, `release`, `acqrel`, `seqcst` (default). `j3c doc threads` says which. `@j3ui` is `j3c doc j3ui`.\n\n\
`f16` (storage only, 2 bytes, align 2; LLVM, C, not `.j3s`): hold it in locals, fields, arrays, `List<f16>`, params. `f16(x)` from a float or int rounds to nearest even (overflow is infinity, NaN stays NaN); `f32(h)` / `f64(h)` are exact; an untyped literal stored in an `f16` rounds at compile time. `+ - * / < ==`, `-h`, `+=`, and interpolation are errors: write `f16(f32(a) + f32(b))`. Bit-identical on both backends. Example: `speculative/lang/f16.j3`.

SIMD (LLVM and C backends, not `.j3s` scripts): `vec4f` (4 x f32) and `vec4i` (4 x i32), 16 bytes, align 16, passed by value. `vec4f(x, y, z, w)` or `vec4f(x)` splats. Lanes `.x .y .z .w` and `v[0..3]` (a constant index) read and write: `v.x = 1.0`, `arr[2].y += 3.0`. `+ - * /` work lane-wise between two vectors or with a scalar of the lane type; `vec4i` also has `& | ^ ~ << >>` (shifts mask the amount to 0..31) and its `/` aborts on a zero lane. `==` and `<` are errors (no masks yet). `@vmin(a, b)` / `@vmax(a, b)` (`a < b ? a : b`: a NaN or a tie gives `b`), `@vsqrt`, `@vabs`, `@vsum(a)` (fixed order `(x + y) + (z + w)`), `@vshuffle(a, i0, i1, i2, i3)`, `@vfloat(vec4i)`, `@vint(vec4f)` (truncates, saturates, NaN is 0), `@vload(xs, i)` / `@vstore(xs, i, v)` on `[]f32` / `[]i32` (bounds-checked, unaligned). Results are bit-identical on both backends at every opt level. An enum payload may hold a vector (the enum is then 16-aligned). `j3.math`: `Vec3.toVec4f(w)` and `vec3FromVec4f(v)`. Not yet: vector consts and global initializers, `.j3s` scripts (a compile error). Example: `speculative/lang/simd.j3`.

Closures
    `fn(a: int): int {{ return a + k; }}` captures the variables it names by address, so a write inside is visible after. Its type is `closure(int): int`, legal only as a parameter or a `let`: it points into the creating function's stack, so it cannot be a field, global, array element, generic argument, return type, `&` target, or cast. Types may be left out where a `closure(..)` or `fn(..)` type is expected: `apply(fn(x) {{ return x + k; }}, 3)`. A named `fn` converts to a closure; a closure never converts to a plain `fn(..)` or `ptr` (`thread.spawn` still takes `fn(ptr)` plus a struct). A literal that captures nothing may stand for a plain `fn`. `return` leaves the closure, `break` and `continue` cannot cross it, and a name it uses cannot be redeclared inside. Not in `.j3s` scripts. Example: `speculative/lang/closures.j3`.
    `fn[mem](a: int): int {{ return a + k; }}` is a boxed closure: it copies the values it captures into arena `mem` (an `Arena` or `&Arena`) when it is made, so a later write to the original is not seen, and a write inside changes its own copy, kept between calls. Its type is `boxed closure(int): int`, a value like any other: a struct field, a return value, a `List` element, a generic argument, a pool job (`pool.submitBoxed(fn[mem]() {{ .. }})`). It never frees itself and lives as long as `mem`. It may be passed where a `closure(..)` of the same signature is wanted, never the other way. It cannot capture a stack closure (an error), and a captured pointer (`&x`, `this`) is copied as a pointer: keeping its target alive is up to you. A `#[singleton] let ON_HIT: boxed closure(int): int;` holds one: it starts empty (calling it before you assign it crashes) and takes no initializer; set it at startup (`ON_HIT = fn[mem](x: int): int {{ .. }};`) and call it as `ON_HIT(3)`. Not a `const` (that is compile-time data; the captures live in an arena made at run time), not in `.j3s` scripts or shader code. Example: `speculative/lang/boxed_closures.j3`.

No heap or refcounted closure environments (boxed closures live in an arena), GC, or borrow checker.

Scripts
    A `.j3s` file is a script: J3 with the same checker, and no `module` line (its path names it). Each value of a script is one instance with its own props and state.
    `props {{ hinge: NodeId; openAngle: f32 = 90.0; }}` is what the importer passes in; props are read-only in the script, and one with no default is required.
    `shared {{ opened: int = 0; }}` is one value every instance reads and writes. A top-level `let` is per-instance state. Every default and initializer is a literal. A script may also declare `struct` and `enum`; those names share the script's namespace, and the script type's name is taken.
    Only `pub fn`s are callable from outside. Outside code can't read an instance's fields or copy one; pass `&Door`.
    `const Door = @script(\"scripts/door.j3s\");` (path from the project root) gives the type; `Door {{ hinge: h }}` makes one, and `door.toggle()` calls it. A script can hold other scripts the same way.
    In a scene, `script=scripts/door.j3s` on an object's line attaches one and `param.<prop>=value` beside it sets its props. `use j3.scene3d.scripts;` then `scripts.start(&scene)` and `scripts.update(&scene, dt)` run every attached script's `pub fn start(scene: &Scene)` and `pub fn update(scene: &Scene, dt: f32)`; `scene.script::<Door>(node)` finds one. A `node: NodeId` prop is the object the script is on.
    `j3c run` runs scripts on a bytecode VM: editing a script re-runs without compiling the program, and while it runs a code edit swaps in. `j3c build` compiles every script in. `j3c dump-bc scripts/door.j3s` shows the bytecode.
    `j3c run path/to/tool.j3s` runs that file once: it makes one instance and calls `pub fn main()`. Arguments after the path are `j3.sys.arg` (`arg(0)` is the program). A scene script is still attached with `script=` on an object, not run this way.

Types
    {types}

Keywords
    {keywords}

Attributes
    {attrs}

Intrinsics
    {intrinsics}

The standard library is `j3c doc`. The window loop is `j3c doc j3.app`. Draw, input, and audio are `j3c doc j3.game`. Generate RGBA assets without a window with `j3c doc j3.gfx.canvas`.
",
        types = names(BUILTIN_TYPES),
        keywords = names(&keywords),
        attrs = names(ATTRIBUTES),
        intrinsics = names(INTRINSICS),
    )
}

pub(super) fn j3ui_page() -> String {
    format!(
        r#"`@j3ui` — UI components (immediate mode; the retained, CSS-styled kind is `j3c doc j3ui`)

`@j3ui name(ui: &Ui, ...) {{ }}` is a function. The first parameter is the `Ui` from `Ui.begin`. The body cannot see locals from another function. Call it like a function. A component inside another is that call. Each widget's arguments are `j3c doc j3.ui`.

```
@j3ui shopRow(ui: &Ui, w: &World, kind: int, name: string, cost: int, maxed: bool) {{
    row() {{
        label(`{{name}}   {{cost}} leaves`);
        button("buy") {{
            buy(w, kind);
        }}
    }}
}}

@j3ui shop(ui: &Ui, w: &World) {{
    panel(280, 248) {{
        label("shop");
        shopRow(ui, w, 0, "grab speed", costSpeed(w.speed), w.speed >= 5);
    }}
}}

fn draw(w: &World) {{
    let ui = Ui.begin(w.font, 16, 188);
    shop(ui, w);
}}
```

A bare widget name gets the receiver (`label` becomes `ui.label`). These names are the ones the parser accepts:

- {end_names} — the call, the body, then `end`
- {pop_names} — the call, the body, then `popId`
- {if_names} — `if ui.button(...) {{ body }}`. The body runs on the click
- {call_names} — the call, then `;`. A `{{ }}` on these is an error

`buttonOff` draws a button that never clicks. `toggle`, `check`, `slider`, `range`, and `stepper` write through a pointer and return true when the value changes. `field` edits a `Text` buffer. Tab and shift-tab move focus across `button` and `field`. Focused buttons use the theme's hot color and activate on Enter or Space. `drag` stays true while the pointer holds it; `dragDelta()` is the motion since the press. `worldLabel` and `worldLabel3` draw at a world point and do not advance the cursor. `if`, `for`, `let`, and other calls stay J3.

ASCII text is drawn from a glyph atlas, so a changing digit does not allocate a new string texture. `scroll(id, height)` skips rows that sit fully outside the window. `j3c doc j3.game drawText` describes the cache.
"#,
        end_names = names(J3UI_END),
        pop_names = names(J3UI_POP),
        if_names = names(J3UI_IF),
        call_names = names(J3UI_CALL),
    )
}

/// The editor's user guide, embedded so it matches this j3c build.
pub(super) fn editor_page() -> String {
    format!(
        "`j3c doc editor` — 3D scene editor

For agents editing an editor project (not the editor itself):
- Project-owned: `j3.editor.toml` (main scene), `scenes/*.j3scene`, `assets/`, `scripts/*.j3s` (attached to objects; `j3c doc lang` has a Scripts section).
- Generated, don't hand-edit: everything under `target/` (`target/j3-editor/`, `target/j3s/` bytecode).
- An object's script is `script=scripts/door.j3s` on its line; its props are `param.<prop>=value` beside it. `j3c check` reports a wrong prop type or a missing required prop against the scene line. `j3c -e script-props scripts/door.j3s` lists what the inspector edits.
- Edit scenes with `j3c -e scene` (by object name, `Parent/Child` path, or `#id`), not by hand: `j3c -e scene --help`.
- Look at what you built: `j3c -e render [--view iso|top|front] [--focus NAME] [--out F.png]`, and
  `j3c run --frames 120 --screenshot F.png --headless` for the running game.
- Place without guessing heights: `j3c -e scene ground|ontop|scatter`.
- Meshes from code: `assets/gen/<name>.j3` with `j3.gfx.meshgen`, made by `j3c -e gen` (`j3c doc j3.gfx.meshgen`).
- Reusable scenes can declare props: `j3c -e scene set @scene prop.speed=float:1`, then
  `set <copy> prop.speed=2`; code reads `scene3d.propNumber(&scene, scene3d.copyOf(&scene, id), \"speed\", 1)`.
- `j3c -e mcp` serves all of this as MCP tools; with the editor open, edits go to it live (one undo step each, unsaved until saved); with it closed they write the file and `undo` puts it back. `j3c -e mcp --tools` lists every tool's arguments.
- Heavy work off the frame thread (meshing, generation, decoding): `j3c doc recipes threads`, rules, safe modules, and mistakes in `j3c doc threads`. GL, SDL, and `j3.gfx`/`j3.game`/`j3.scene3d` state are main-thread only.
- Check loop, and what each step catches: `j3c doc recipes check`. Scene loading and drawing from code: `j3c doc recipes scene3d`.
- Scene files are line-based `key=value` saves. The editor picks up outside edits within a second (merging them into unsaved work), but `j3c -e mcp` edits it directly and is safer while it's open.
- Runtime API for scenes, fields, lights, and materials: `j3c doc j3.scene3d`. Build/run: `j3c check`, `j3c run`.
- Build errors from Play land in `target/j3-editor/build.log`.
- Changing the editor itself: `j3c/editor/src/` (window, one module per file; `main.j3` is the entry), `j3c/src/editor.rs` (host, project setup), `j3c/stdlib/j3/scene3d.j3` (format); `J3_EDITOR_SRC=path/to/editor/src` builds from disk.

{}",
        include_str!("../../editor/README.md")
    )
}

