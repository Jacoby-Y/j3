# Generics and Function Pointers

## Generic structs

Structs can take type parameters. Their methods can use them.

```rust
struct Pair<T> {
    a: T;
    b: T;

    fn first(this: &Pair<T>): T {
        return this.a;
    }
}

let p = Pair<int> { a: 1, b: 2 };
let x = p.first();
```

Methods can also take their own type parameters, passed with `::<T>` at the call. The arena's `alloc` is one:

```rust
let enemies = mem.alloc::<Enemy>(256);    // []Enemy
```

Generics are monomorphized: each `Pair<int>` and `Pair<f32>` is compiled as its own type, so there is no runtime cost.

## Generic functions

Free functions take type parameters too.

```rust
fn first<T>(xs: []T): T { return xs[0]; }
fn swap<T>(a: &T, b: &T) { let t = *a; *a = *b; *b = t; }
pub fn maxOf<T>(a: T, b: T): T { if a > b { return a; } return b; }

let n = first(nums);         // T = int
let m = maxOf(f, 2);         // f is f32, so the literal is an f32
let k = nums.first();        // UFCS infers T from the receiver
let z = zero::<f32>();       // T only in the return type: write it
```

`T` comes from the arguments. A bare number takes the type of the other arguments, or `int`/`float` when nothing else says. A closure parameter works (`fn apply<T>(x: T, f: closure(T): T): T`), and the written types of a closure literal count toward `T`. If `T` cannot be found the compiler asks for `name::<Type>(..)`.

Like generic structs, each combination of types is compiled once, shared by every module that uses it. The body is checked per instance with `T` replaced, so `a > b` compiles for `int` and fails with a note naming the call for a `string`. A generic function that calls itself with a bigger type (`f<T>` calling `f<List<T>>`) stops with an error at 32 levels. Write the return type; it is not inferred. Generic functions cannot be called from `.j3s` scripts.

## Function pointers

`fn(A, B): R` is the type of a pointer to a function. Any named function can be passed as one.

```rust
fn inc(x: int): int { return x + 1; }

fn twice(f: fn(int): int, x: int): int {
    return f(f(x));
}

twice(inc, 3);   // 5
```

A slice of them makes a simple system list:

```rust
let systems: []fn(&World, f32) = [moveSystem, collideSystem];
for sys in systems {
    sys(&world, dt);
}
```

## What's not here

J3 has traits as bounds on type parameters (see [Traits](traits.md)), but no `dyn` or trait objects. A plain `fn(..)` carries no state, so a callback that must outlive the call (a thread job) takes the state as a pointer argument next to the function pointer; a callback that is used and dropped inside one call can be a closure (see Functions and Modules). The thread pool works this way: `pool.parallelFor(n, f, ctx)` calls `f` with your `ctx`.
