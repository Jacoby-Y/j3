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

`Self` is the type being checked. The first parameter is `this: Self` or `this: &Self`; the rest and the return type must match exactly. A trait has no method bodies, no `where`, no type parameters of its own, and no supertraits. Traits are exported and imported like functions (`pub trait`, `use m.Hash;`), and share one namespace with types.

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

Bodies are still checked once per instance, with `T` replaced. A bound is the contract the caller sees, not a limit inside the body: calling a method outside the bounds works until an instance lacks it. The methods of a generic struct (`Map<K, V>`) are lowered with each instance, so they use the struct's bounds rather than adding their own.

## Comparison operators

Without `operator!=`, `a != b` is `!(a == b)`. Without `operator>`, `operator<=` or `operator>=`, they use `operator<`: `a > b` is `b < a`, `a <= b` is `!(b < a)`, `a >= b` is `!(a < b)`. `<=` and `>=` are `!(b < a)` and `!(a < b)`, so `Ord` assumes a total order; if a field can be NaN, define `<=` and `>=` yourself. An operator you write always wins. With no `operator==` (and no built-in one), `==` and `!=` on a struct is still an error. So a type needs `operator==` for `Eq` and `operator<` for `Ord`, and gets the other four operators free.

## In the standard library

`j3.cmp` declares `Eq` (`operator==`), `Ord` (`operator<`) and `Hash` (`fn hash(this: Self): u64`). It gives `Hash` to `int`, `i32`, `u32`, `u8`, `u64`, `bool`, `f32`, `f64` and `string` through `#[impl]` fns (a splitmix64 mixer for numbers, FNV-1a plus the mixer for strings, so the low bits are safe to mask), and has `combine(a, b)` to build a hash from fields, plus the helpers `hashOf`, `contains`, `indexOf`, `minOf` and `maxOf`. `Entity` has `hash`. `j3.data.map` is `Map<K: Hash + Eq, V>`: `Map<string, int>.new(&arena, 16)`, `put`, `get` (an `Option`), `getOr`, `has`, `remove`, `clear`, and slot iteration (`for i in 0..m.cap { if m.live(i) { m.keyAt(i); m.valAt(i); } }`). Like `List`, it takes its memory from an arena and is not shared across threads. `j3c doc j3.cmp` and `j3c doc j3.data.map` list them.

## Limits

No `dyn` or trait objects, no default methods, no generic traits, no closure types in a trait method, and no traits in `.j3s` scripts (the VM has no generic instances). A script may call a plain function that uses them, but `x.hash()` in a script does not find `#[impl]` fns. A bound is a bare trait name, imported with `use`.
