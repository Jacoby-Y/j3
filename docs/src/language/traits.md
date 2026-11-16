# Traits

A trait is a list of method signatures. A bound on a type parameter says which traits the type must satisfy, and the compiler checks that where the generic is used.

```rust
pub trait Hash { fn hash(this: Self): u64; }
pub trait Eq   { fn operator==(this: Self, other: Self): bool; }

fn contains<T: Eq>(xs: []T, x: T): bool {
    for i in 0..xs.len {
        if xs[i] == x { return true; }
    }
    return false;
}

struct Map<K: Hash + Eq, V> { ... }
```

`Self` is the type being checked. The first parameter is `this: Self` or `this: &Self`; the rest and the return type must match exactly. A trait has no `where` and no type parameters of its own. Traits are exported and imported like functions (`pub trait`, `use m.Hash;`), and share one namespace with types.

## Default methods and supertraits

A trait method with a body is a default: a type that meets the trait's other methods but has no method of that name gets the body (its own method wins). `trait Ord: Eq { .. }` makes `Eq` a supertrait: meeting `Ord` needs `Eq`'s methods too, so `T: Ord` may use `==`.

```rust
pub trait Eq { fn operator==(this: Self, other: Self): bool; }
pub trait Ord: Eq { fn operator<(this: Self, other: Self): bool; }
pub trait Named { fn name(this: Self): string; }
pub trait Weigh { fn weigh(this: Self): int; }
pub trait Labeled: Named + Weigh {
    fn tag(this: Self): int { return this.name().len * 100 + this.weigh(); }
}
```

Two traits giving one type a default of the same name is an ambiguity error.

## Derive

`#[derive(Eq, Hash, Ord, Debug, Clone)]` on a struct or enum writes those methods field by field: `==` compares every field (an enum: same variant, then its fields), `<` lets the first differing field decide (an enum: variant order first), `hash` folds the fields like a tuple of them, `clone` copies, and `Debug` is `#[Debug]`. A derived type can be a `Map` key and meets `j3.cmp`'s `Eq`, `Ord`, `Hash` and `Clone`. A generic enum derives too (`#[derive(Eq, Hash)] enum Opt<T> { Some(T), None }`): each instance has the methods when its type arguments do, so `Opt<int>` is a `Map` key and `==` on an `Opt<S>` whose `S` has no `==` is the error. An error inside one (a field without `hash`) points at the derive.

```rust
#[derive(Eq, Hash, Ord, Debug, Clone)]
struct Key {
    name: string;
    n: int;
}

#[derive(Eq, Hash, Ord, Debug)]
enum Shape { Dot, Circle(int), Rect(int, int) }
```

## Satisfying a trait

A type satisfies a trait by having the methods. There is no `impl Trait for T` block, so every existing `operator==` already satisfies `Eq`.

```rust
struct Vec3 {
    x: f32; y: f32; z: f32;
    fn hash(this: &Vec3): u64 { ... }     // Vec3 is Hash
}
```

For a type you cannot edit (`int`, `string`, a stdlib struct) write an `#[impl]` fn. It is a free fn whose first parameter is the receiver, and `x.hash()` finds it:

```rust
#[impl]
fn hash(x: int): u64 { return u64(x); }
```

There can be one `#[impl]` fn per (method name, type), so `hash` for `int` and `hash` for `string` live side by side. The orphan rule says an `#[impl]` fn sits in the module that declares a trait with that method, or in the module that defines the type. An own method wins over an `#[impl]` fn. A second `#[impl]` for the same method and type is an error that names the first.

`int`, floats, `bool`, `string`, `ptr` and C-like enums satisfy `operator==` without any help, and numbers satisfy `operator<`. An ordinary function (including one used through UFCS) does not satisfy a bound: bodies are lowered in the module that defines them, where the caller's function is not visible. The error says so and points at `#[impl]`.

## Where bounds are checked

At the call of a generic fn, at a mention like `Map<Vec3, int>`, and at the call of a method that has its own `<U: Trait>`. The error points there:

```
error: `Vec3` does not satisfy `Hash` (needed by `K` of `Map`)
  note: bound declared at j3/data/map.j3:5
  note: missing: fn hash(this: Self): u64
  help: add `fn hash(this: &Vec3): u64` to `struct Vec3`, or `#[impl] fn hash(x: Vec3): u64` in a module that declares `Hash` or defines `Vec3`
```

A generic body is checked once, at its definition, against its bounds: inside it `T` has only the methods its traits declare (supertraits and default methods included), so calling another method, comparing with an operator the bounds lack, or reading a field is an error there, even if nothing calls the function. Arithmetic and casts on `T` are still checked per instance. A generic struct's method that needs more than the struct's bounds says so with `where`: `fn indexOf(this: &List<T>, x: T): int where T: Eq { .. }`. The bound is checked where the method is called, so a `List` of a type without `==` is fine until `indexOf` is used.

## Comparison operators

Without `operator!=`, `a != b` is `!(a == b)`. Without `operator>`, `operator<=` or `operator>=`, they use `operator<`: `a > b` is `b < a`, `a <= b` is `!(b < a)`, `a >= b` is `!(a < b)`. `<=` and `>=` are `!(b < a)` and `!(a < b)`, so `Ord` assumes a total order; if a field can be NaN, define `<=` and `>=` yourself. An operator you write always wins. With no `operator==` (and no built-in one), `==` and `!=` on a struct is still an error. So a type needs `operator==` for `Eq` and `operator<` for `Ord`, and gets the other four operators free.

## In the standard library

`j3.cmp` declares `Eq` (`operator==`), `Ord: Eq` (`operator<`), `Hash` (`fn hash(this: Self): u64`) and `Clone` (`fn clone(this: Self): Self`), gives `string` a byte-order `operator<`, and has `sort(xs)` / `sortBy(xs, less)` (stable merge sort). It gives `Hash` to `int`, `i32`, `u32`, `u8`, `u64`, `bool`, `f32`, `f64` and `string` through `#[impl]` fns (a splitmix64 mixer for numbers, FNV-1a plus the mixer for strings, so the low bits are safe to mask), and has `combine(a, b)` to build a hash from fields, plus the helpers `hashOf`, `contains`, `indexOf`, `minOf` and `maxOf`. `Entity` has `hash`. `j3.data.map` is `Map<K: Hash + Eq, V>`: `Map<string, int>.new(&arena, 16)`, `put`, `get` (an `Option`), `getOr`, `has`, `remove`, `clear`, `getPtr` (`*m.getPtr(k) += 1`), `getOrPut`, `each(fn(k, v))`, `entries(&arena)` (`for (k, v) in m.entries(&a)`), `sortedKeys(&arena)`, `free`, and slot iteration (`for i in 0..m.cap { if m.live(i) { m.keyAt(i); m.valAt(i); } }`). Like `List`, it takes its memory from an arena and is not shared across threads. `j3c doc j3.cmp` and `j3c doc j3.data.map` list them.

## Limits

No generic traits, no closure types in a trait method, and no traits in `.j3s` scripts (the VM has no generic instances). A script may call a plain function that uses them, but `x.hash()` in a script does not find `#[impl]` fns. A bound is a bare trait name, imported with `use`.

## `dyn Trait`

`dyn Backend` holds a pointer to any value whose type has `Backend`'s methods, plus that type's method table, so one variable or `List<dyn Backend>` can hold several implementations. Make one from a reference (`let b: dyn Backend = &llvm;`); calls go through the table (`b.emit(3)`). The table includes supertrait methods and default bodies. A `dyn Backend` converts to a `dyn` of any supertrait (`let n: dyn Named = b;` with `trait Backend: Named`): the `Backend` table carries the same type's `Named` table, so this works for any type behind it; any other trait is an error. A `dyn` trait's methods take `this: &Self` and use `Self` nowhere else; `==` on `dyn` values is an error. See `speculative/lang/dyn.j3`.
