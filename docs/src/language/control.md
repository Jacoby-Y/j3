# Control Flow

Conditions take no parentheses, and braces are always required.

## if

```rust
if hp <= 0 && !invincible {
    printLn("dead");
} else if invincible {
    printLn("cheat");
} else {
    printLn(`hp={hp}`);
}
```

`if` is also an expression. Used as a value, it needs a final `else`, and only the chosen arm runs:

```rust
let abs = if n < 0 { -n } else { n };
let tier = if xp > 500 { 3 } else if xp > 100 { 2 } else { 1 };
```

`&&` and `||` short-circuit.

## while

```rust
let n = 3;
while n > 0 {
    n -= 1;
}
```

## for

`for` walks a range or a collection. `a..b` stops before `b`; `a..=b` includes it.

```rust
for i in 0..5 { }          // 0 1 2 3 4
for i in 0..=5 { }         // 0 1 2 3 4 5
for n in xs { }            // each element of an array or slice
for i, n in xs { }         // with its index
for cp in s.runes() { }    // an iterator
```

An iterator is a struct with `fn next(this: &Self): Option<T>`; the loop calls it until `None`. The expression after `in` runs once: a call's result is a temporary, while a local is advanced in place (`for cp in &r { }` leaves `r` where the loop stopped). Nothing is allocated per step. An iterator loop has no index form.

```rust
struct Down {
    n: int;
    fn next(this: &Down): Option<int> {
        if this.n == 0 { return Option.None; }
        this.n -= 1;
        return Option.Some(this.n + 1);
    }
}

for v in Down { n: 3 } { }   // 3 2 1
```

`break` and `continue` work on the innermost loop. Put a label on a `while` or `for` to reach an outer one: `break 'rows;` leaves it, `continue 'rows;` moves to its next step. The `defer`s of the loops left run first, innermost first.

```rust
'rows: for r in 0..3 {
    for c in 0..3 {
        if c == 1 { continue 'rows; }   // next row
        if r == 2 { break 'rows; }      // leave both loops
        printLn(`r{r} c{c}`);
    }
}
```

## match

`match` lists every variant, each written with its enum name, so adding a variant points you at every `match` that needs it. A final `_ { }` arm covers the variants you leave out instead (a `_` that covers none is a warning).

```rust
match dir {
    Dir.N { return "north"; }
    Dir.E { return "east"; }
    Dir.S { return "south"; }
    Dir.W { return "west"; }
}
```

Inside a payload, `_` matches anything (`Result.Err(_)`). Payloads are covered in [Enums and Errors](enums.md).

`match` on an integer takes int literals, char literals or int `const`s as arms. It needs a final `_` arm, and a repeated value is a compile error:

```rust
const MAX = 255;

match code {
    0 { printLn("zero"); }
    -1 { printLn("minus one"); }
    MAX { printLn("max"); }
    _ { printLn(`other {code}`); }
}
```

`match` on a `string` takes string literals as arms (with `|` and guards) and needs a final `_` arm; a repeated literal is a compile error. It compares the length, then the bytes, arm by arm:

```rust
match word {
    "let" { kw(Tok.Let); }
    "fn" | "func" { kw(Tok.Fn); }
    _ { name(word); }
}
```

An arm can list alternatives with `|`, take an inclusive range `lo..=hi`, and add a guard `if cond`. When the guard is false the later arms are tried, and a guarded arm does not count toward "every variant". Ranges and values may not overlap each other (unless one of them is guarded). Every `|` alternative binds the same names with the same types:

```rust
match ch {
    'a'..='z' | 'A'..='Z' | '_' { name(); }
    '0'..='9' { digit(); }
    _ { other(); }
}

match tok {
    Tok.Plus | Tok.Minus { add(); }
    Tok.Num(n) if n < 0 { negative(n); }
    Tok.Num(n) | Tok.Size(n) { number(n); }
    _ { }
}
```

Patterns nest in any position: payloads, tuples (`(Op.Mul, 0)`), and structs. `Span { lo, hi }` or `Span { lo, .. }` takes a struct apart, in a `match` arm or a `let` (`let Span { lo, hi: end } = span;`). A variant counts as covered only when its arms cover every value of its fields.

### match as a value

`match` is also an expression. An arm's last expression, written with no `;`, is its value, and every arm gives the same type. An arm that leaves (`return`, `break`, `continue`, `panic`) gives no value and fits any type. A `match` at the start of a statement stays a statement.

```rust
let a = match s {
    Shape.Circle(r) { 3 * r * r }
    Shape.Rect(w, h) { w * h }
    Shape.Dot { return -1; }
};
```

### if let and let else

`if let` is a one-arm `match`. Its names exist only in the first block, it chains with `else if let` / `else if`, and it is a value like `if`. `let .. else` binds the names for the rest of the block; its `else` must leave.

```rust
fn radius(s: Shape): int {
    let Shape.Circle(r) = s else { return 0; };
    return r;
}

fn describe(s: Shape): string {
    return if let Shape.Rect(w, h) = s {
        if w == h { "square" } else { "rect" }
    } else if let Shape.Circle(_) = s {
        "circle"
    } else {
        "dot"
    };
}
```

## defer

`defer` runs a statement when its block ends, including on an early `return`. Several defers run last-first.

```rust
fn main() {
    let mem = Arena.create(16 * 1024 * 1024);
    defer mem.destroy();

    let frame = mem.checkpoint();
    defer mem.rewind(frame);    // runs before destroy
}
```
