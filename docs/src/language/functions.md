# Functions and Modules

## Functions

```rust
fn add(a: int, b: int): int {
    return a + b;
}

fn sayHello() {               // `: void` is implied
    printLn("Hello!");
}
```

Parameters are passed by value. To let a function change something, pass a pointer with `&` and write through it with `*`:

```rust
fn increment(n: &int) {
    *n += 1;
}

fn main() {
    let count = 0;
    increment(&count);
}
```

## Closures

A function literal `fn(x: int): int { ... }` is a closure. It captures the variables it names **by address**, so a write inside it is visible afterwards, and a loop variable is seen at the value of the current iteration:

```rust
fn apply(f: closure(int): int, x: int): int { return f(x); }

fn main() {
    let total = 0;
    let base = 10;
    let add = fn(x: int): int { total += x; return base + x; };
    add(1);
    add(2);                                  // total is now 3
    printLn(`{apply(fn(x) { return x * base; }, 4)}`);   // 40: the types come from `apply`
}
```

The type is `closure(A, B): R`. A closure value points into the stack of the function that made it, so it can only be a parameter or a `let`. The compiler rejects it as a struct field, a global, an array element, a generic argument (`List<closure(int)>`), a return type, a `&` target, or a cast, with a message that says what to do instead. For the same reason it never converts to a plain `fn(..)` or `ptr`. A closure that must outlive its call is a boxed closure (below).

A named `fn` can be passed where a `closure(..)` is wanted, and a literal that captures nothing can be passed where a plain `fn(..)` is wanted. Inside a closure body, `return` leaves the closure, `defer` runs when it ends, and `break`/`continue` cannot cross its edge. A name the closure uses cannot be redeclared inside it (shadowing is an error everywhere). Closures are not available in `.j3s` scripts.

### Boxed closures

`fn[mem](..) { }` is a boxed closure: it copies the values it captures into arena `mem` (an `Arena` or `&Arena`) when it is made. Its type is `boxed closure(A): R`, a value like any other: a struct field, a return value, a `List` element, a pool job (`pool.submitBoxed(fn[mem]() { .. })`).

```rust
fn adder(mem: &Arena, k: int): boxed closure(int): int {
    return fn[mem](x: int): int { return x + k; };
}

let base = 100;
let snap = fn[mem](x: int): int { return base + x; };
base = 0;                                // snap(1) is still 101
```

Captures are copies taken at creation, so a later write to the original is not seen, and a write inside the closure changes its own copy, kept between calls. It never frees itself and lives as long as `mem`. It can be passed where a `closure(..)` of the same signature is wanted, never the other way. It cannot capture a stack closure, and a captured pointer (`&x`, `this`) is copied as a pointer, so keeping its target alive is up to you. A `#[singleton] let ON_HIT: boxed closure(int): int;` can hold one: it starts empty (calling it before it is assigned crashes) and takes no initializer, so set it at startup and call it as `ON_HIT(3)`. A `const` cannot hold one (a const is compile-time data; the captures live in an arena made at run time), and neither can `.j3s` scripts or shader code. See `speculative/lang/boxed_closures.j3`.

## Modules

One file is one module. The `module` line names it, and that name is how other files import it.

```rust
module mygame.player;

use j3.io.printLn;               // one name
use j3.memory.{ Arena, List };   // several names
use j3.game;                     // the module itself: game.poll(), game.Key.Escape
```

Only items marked `pub` can be named from another module:

```rust
pub struct Player { hp: int; }
pub fn spawn(): Player { return Player { hp: 10 }; }
fn helper() {}                    // private to this file
```

Imports cannot form a cycle; the compiler reports it.

### Splitting a big module

A module can span several files. Files in a folder next to `X.j3` (that is, `X/*.j3`) that begin with the same `module` line are more of the same module: they share its names and its imports. Put the `use` lines in `X.j3`.

```text
src/
  world.j3          module mygame.world;  (all the `use` lines)
  world/
    spawn.j3        module mygame.world;
    save.j3         module mygame.world;
```

A file in that folder with a *different* `module` line is its own module, and the folder becomes part of its name (`mygame.world.tiles`).
