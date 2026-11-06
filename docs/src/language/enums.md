# Enums and Errors

## Plain enums

An enum without payloads is a set of named values (stored as an `i32`). Variants are always written with the enum name in front.

```rust
enum Dir {
    N, E, S, W
}

let d = Dir.N;
```

## Enums with payloads

Give any variant a payload and the enum becomes a tagged union. `match` pulls the payload out:

```rust
fn find(xs: []int, want: int): Option<int> {
    for n in xs {
        if n == want {
            return Option.Some(n);
        }
    }
    return Option.None;
}

match find(xs, 2) {
    Option.None { printLn("miss"); }
    Option.Some(n) { printLn(`hit {n}`); }
}
```

Patterns nest, and `_` inside a pattern matches anything:

```rust
match greet() {
    Result.Ok(msg) { printLn(msg); }
    Result.Err(IoError.NotFound) { printLn("missing"); }
    Result.Err(_) { panic("unexpected io"); }
}
```

## Errors and `?`

J3 has no exceptions and no null. A function that can fail returns an enum, usually `Option<T>` (`j3.option`) or `Result<T, E>` (`j3.result`).

`?` unwraps one: on success it gives you the value, on failure it returns the failure from the current function.

```rust
use j3.result.Result;

enum IoError { NotFound, Permission }

fn loadName(): Result<string, IoError> {
    return Result.Err(IoError.NotFound);
}

fn greet(): Result<string, IoError> {
    let name = loadName()?;          // returns the Err early
    return Result.Ok(`hello {name}`);
}
```

`?` is not tied to `Option` or `Result` by name. It works on any enum marked `#[TryCatch(Ok, Err)]`, naming its success and failure variants. That is exactly how the standard library defines those two, and you can do the same for your own types.

## Panics

`j3.sys.panic("message")` stops the program with exit code 101. It is a library call, not a keyword. Use it for "this can't happen", not for errors a caller should handle.
