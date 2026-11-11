# Introduction

J3 is an ahead-of-time compiled language for making games. It looks a little like Rust and a little like Zig, but it is smaller than either: there is no garbage collector, no borrow checker, and no escaping closures. What you get instead is plain data, explicit memory, and a compiler that turns your program into a native binary through LLVM.

```rust
module hello;

use j3.io.printLn;

fn main() {
    let name = "world";
    printLn(`Hello, {name}!`);
}
```

The compiler, `j3c`, is a single Rust binary with no dependencies. It carries the whole standard library inside it, so the version of the docs you read with `j3c doc` always matches the compiler you run.

## What's in the box

- **The language**: structs with methods, tagged-union enums, slices, arenas, generics with trait bounds, closures, and a `?` operator for errors.
- **The standard library**: windows, input, audio, 2D and 3D rendering, an ECS, physics, UI components styled with CSS, networking, threads, and more, all written in J3.
- **Tools**: a formatter, a language server, a 3D scene editor, a script VM for hot-reloaded `.j3s` files, the j2 runtime for player mods, and packaging for zip, `.deb`, and Android.

## How to read this book

[Getting Started](getting-started.md) gets a program running. [The Language](language/types.md) part walks through J3 from values up to threads; each chapter builds on the last. [The Compiler](compiler/cli.md) part covers `j3c` itself: commands, the `j3.toml` manifest, build modes, testing, and shipping.

When you need an exact function name, ask the compiler:

```text
j3c doc                  # tasks and stdlib modules
j3c doc search <word>    # find a name in any module
j3c doc <module> --short # one module on one screen
j3c doc recipes          # complete, checked example programs
```
