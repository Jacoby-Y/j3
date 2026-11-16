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

A generic method's type arguments are inferred from its arguments the same way as a generic function's: `mem.new(Expr.Num(1))` is `mem.new::<Expr>(..)`.

Generics are monomorphized: each `Pair<int>` and `Pair<f32>` is compiled as its own type, so there is no runtime cost. A generic struct's methods are compiled per instance only when something calls them, so `List<Tok>` compiles even though `Tok` has no `==`; calling `indexOf` (which says `where T: Eq`) on it is the error. Errors inside an instance's body name it and the call: `while instantiating List<Tok>.indexOf (called at ..)`.

## Generic functions

Free functions take type parameters too.

```rust
fn first<T>(xs: []T): T { return xs[0]; }
fn swap<T>(a: &T, b: &T) { let t = *a; *a = *b; *b = t; }
fn maxOf<T: Ord>(a: T, b: T): T { if a > b { return a; } return b; }

let n = first(nums);         // T = int
let m = maxOf(f, 2);         // f is f32, so the literal is an f32
let k = nums.first();        // UFCS infers T from the receiver
let z = zero::<f32>();       // T only in the return type: write it
```

`T` comes from the arguments. A bare number takes the type of the other arguments, or `int`/`float` when nothing else says. A closure parameter works (`fn apply<T>(x: T, f: closure(T): T): T`), and the written types of a closure literal count toward `T`. If `T` cannot be found the compiler asks for `name::<Type>(..)`.

Like generic structs, each combination of types is compiled once, shared by every module that uses it. The body is checked once at the definition against the bounds: `a > b` needs `T: Ord` (see Traits), and an unbounded `T` can only be stored, copied and passed. What no bound can say (arithmetic, casts) is checked per instance, with a note naming the call. A generic function that calls itself with a bigger type (`f<T>` calling `f<List<T>>`) stops with an error at 32 levels. Write the return type; it is not inferred. Generic functions cannot be called from `.j3s` scripts.

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

Traits are bounds on type parameters, and `dyn Trait` picks an implementation at run time (see [Traits](traits.md)). A plain `fn(..)` carries no state. A callback used and dropped inside one call can be a stack closure; one that must outlive the call (a stored handler, a thread job) is a boxed closure `fn[mem](..) { }` living in an arena, or a function pointer plus a state pointer (see [Functions and Modules](functions.md)). The thread pool takes both: `pool.submitBoxed(fn[mem]() { .. })`, or `pool.parallelFor(n, f, ctx)`, which calls `f` with your `ctx`.
