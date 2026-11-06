# Structs and Methods

## Structs

A struct is plain data. Fields end with `;`, and a field may have a default.

```rust
struct Enemy {
    hp: int = 10;
    speed: f32 = 2.0;
    alive: bool = true;
}

let e = Enemy { speed: 3.5 };     // hp and alive use their defaults
```

Fixed-size array fields have no default, so write them in every literal:

```rust
struct State {
    n: int;
    ys: [3]int;
}

let s = State { n: 0, ys: [0, 0, 0] };
```

## Methods

Methods live inside the struct body. The receiver is the first parameter, named `this`.

```rust
struct Vec2 {
    x: f32;
    y: f32;

    // by value: small structs are copied
    fn length(this: Vec2): f32 {
        return sqrt(this.x * this.x + this.y * this.y);
    }

    // by pointer: changes the caller's value
    fn scale(this: &Vec2, s: f32) {
        this.x *= s;
        this.y *= s;
    }
}

let v = Vec2 { x: 3, y: 4 };
let len = v.length();
v.scale(0.5);         // `&v` is taken for you
```

There is no `fn Vec2.name(...)` syntax outside the body.

`.` follows pointers automatically, and works on any value, not just variables: `f().x`, `(a + b).x`, and `xs[a..b].len` are all fine. A temporary like `f().x` can be read but not assigned.

## Operators

Operators are methods named `operator+`, `operator-`, `operator*`, and so on:

```rust
struct Vec2 {
    x: f32;
    y: f32;

    fn operator+(this: Vec2, o: Vec2): Vec2 {
        return Vec2 { x: this.x + o.x, y: this.y + o.y };
    }

    fn operator*(this: Vec2, s: f32): Vec2 {
        return Vec2 { x: this.x * s, y: this.y * s };
    }
}

let next = pos + vel * dt;
```

## Embedding with `use`

`use` on a field embeds it: the inner struct's fields can be read straight through the outer one. This is composition, not inheritance.

```rust
struct Transform {
    use position: Vec2;
    use scale: Scale2;
}

let t = Transform {
    position: Vec2 { x: 10, y: 10 },
    scale: Scale2 { width: 1, height: 1 },
};

printLn(`{t.x} {t.width}`);   // same as t.position.x, t.scale.width
```

Only fields are promoted, not methods: write `t.position.length()`. Two embedded structs with a field of the same name is a compile error.

A `use` *statement* brings a value's fields into the current block as names. They are places, not copies, so assigning one writes the field:

```rust
use t;
width = 2;          // writes t.width
```

## Printing structs

Mark a struct `#[Debug]` to put it in a string interpolation hole:

```rust
#[Debug]
struct Vec2 { x: f32; y: f32; }

printLn(`pos = {pos}`);
```

## Free functions as methods (UFCS)

`x.f(args)` calls the free function `f(x, args)` when `x`'s type has no method `f`. The type's own method always wins.

```j3
fn sum(p: P): int { return p.x + p.y; }
fn bump(p: &P, by: int) { p.x += by; }

p.bump(10);          // takes &p, like a method with this: &P
let t = p.sum();
```

- `f` must be visible by its bare name: declared in this module, or imported with `use m.{ f }`. A name reachable only as `m.f` does not count.
- The first parameter takes the receiver by value, or as `&T`, and the receiver may itself be a `&T`.
- It works on any receiver: structs, enums, primitives (`far.abs()` with `use j3.math.{ abs }`), slices, strings.
- Generic free functions work too: `xs.first()` infers `T` from the receiver (see [Generics](generics.md)).
- Same IR as `f(x, args)`; the receiver is evaluated once, before the other arguments.

See `speculative/lang/ufcs.j3`.

## extend

`extend T { .. }` adds methods to a struct declared in the same module, in any file of that module: the main file or a part file (`X.j3` plus `X/*.j3` saying the same `module`). It is the same as writing the methods in the struct body, so big types keep their methods next to the code they use.

```j3
struct Counter { n: int; }

extend Counter {
    fn bump(this: &Counter, by: int) { this.n += by; }
}

extend Box<T> {                      // a generic struct: parameters named as in `struct Box<T>`
    fn first(this: &Box<T>): T { return this.v; }
}
```

- Methods are public like in a body, so `pub fn` is an error.
- Errors, each with a fix hint: no such struct, a struct of another module, an enum (write a free fn), a method name already in the body or another block, other type parameters than the struct has.
- An error inside a method points into the file where it is written. `j3c doc` lists these methods under their struct.
- Not available in `.j3s` scripts.

See `speculative/lang/extend.j3`.
