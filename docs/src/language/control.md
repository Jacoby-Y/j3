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

`match` must list every variant, each written with its enum name. There is no catch-all arm on an enum, so adding a variant to an enum points you at every `match` that needs it.

```rust
match dir {
    Dir.N { return "north"; }
    Dir.E { return "east"; }
    Dir.S { return "south"; }
    Dir.W { return "west"; }
}
```

Inside a payload, `_` matches anything (`Result.Err(_)`). Payloads are covered in [Enums and Errors](enums.md).

`match` on an integer takes int literals or int `const`s as arms. It needs a final `_` arm, and a repeated value is a compile error:

```rust
const MAX = 255;

match code {
    0 { printLn("zero"); }
    -1 { printLn("minus one"); }
    MAX { printLn("max"); }
    _ { printLn(`other {code}`); }
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
