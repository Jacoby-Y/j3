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

The type is `closure(A, B): R`. A closure value points into the stack of the function that made it, so it can only be a parameter or a `let`. The compiler rejects it as a struct field, a global, an array element, a generic argument (`List<closure(int)>`), a return type, a `&` target, or a cast, with a message that says what to do instead. For the same reason it never converts to a plain `fn(..)` or `ptr`: `thread.spawn` and `pool.submit` still take `fn(ptr)` plus a struct.

A named `fn` can be passed where a `closure(..)` is wanted, and a literal that captures nothing can be passed where a plain `fn(..)` is wanted. Inside a closure body, `return` leaves the closure, `defer` runs when it ends, and `break`/`continue` cannot cross its edge. A name the closure uses cannot be redeclared inside it (shadowing is an error everywhere). Closures are not available in `.j3s` scripts.

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
