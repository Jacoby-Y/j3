# Arrays, Slices, and Strings

## Fixed arrays

`[N]T` is an array of `N` values that lives on the stack. Its size is part of its type.

```rust
let xs: [4]int = [10, 20, 30, 40];
xs[0] = 11;
printLn(`{xs.len}`);    // 4
```

An array is a value: assigning one copies it, and a function can take and return one (`fn corners(w: int): [4]int { return [0, w, w * 2, w * 3]; }`), on both backends.

## Slices

`[]T` is a slice: a pointer and a length. It views memory someone else owns and never allocates. An array turns into a slice when you pass it where one is expected.

```rust
fn sum(xs: []int): int {
    let total = 0;
    for n in xs {
        total += n;
    }
    return total;
}

let all: []int = xs[:];      // whole array
let mid: []int = xs[1..3];   // elements 1 and 2
mid[0] = 21;                 // writes xs[1]
sum(xs);                     // array passed as a slice
```

`xs[a..b]` is half-open, like ranges in `for`.

## Bounds checks

Every `xs[i]` is checked. An index out of range stops the program with the file and line (exit code 102). `j3c build --fast` removes those checks when you need the last bit of speed.

## Strings

`string` is a read-only view of UTF-8 bytes, just like a `[]u8` you cannot write to. `s.len` is its length in bytes.

```rust
let name: string = "Coby";
printLn(`{name} is {name.len} bytes`);
```

To build or change text, use `j3.text.Text`:

```rust
let t = Text.scratch().add("score: ").add(`{score}`).str();
```

A `Text.scratch()` result lasts until the next frame. `t.fmt(`x={x}`)` appends a backtick string straight into a `Text` (see [String interpolation](types.md#string-interpolation)).

`j3.text` also reads strings: `startsWith`, `endsWith`, `trim`, `lastIndexOf`, `replace`, `split(s, ",")` and `lines(s)` (iterators), `parseInt(s, 10)` / `parseU64` / `parseF64` (each an `Option`; floats round correctly), `writeF64` (the shortest text that reads back exactly), and UTF-8 with `decodeRune(s, i): (u32, int)`, `encodeRune`, `isValidUtf8` and `for cp in text.runes(s) { }`. See `j3c doc j3.text`.

## Growable lists

Arrays and slices have a fixed length. For a list that grows, use `List<T>` from `j3.memory`. It allocates from an arena, so it is covered in [Memory and Arenas](memory.md#lists).
