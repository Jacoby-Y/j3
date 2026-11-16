# Memory and Arenas

J3 has no garbage collector and no `malloc`/`free` pairs to match up. Values live on the stack, and anything bigger or longer-lived comes from an **arena**.

## Arenas

An `Arena` (`j3.memory`) is a bump allocator: it hands out memory by moving a cursor forward, and frees everything at once by moving it back.

```rust
use j3.memory.{ Arena, List };

fn main() {
    let mem = Arena.create(16 * 1024 * 1024);   // 16 MB to start
    defer mem.destroy();

    let pos = mem.alloc::<Vec2>(256);           // []Vec2, 256 long
    let vel = mem.alloc::<Vec2>(256);
}
```

When an arena fills up it adds another chunk. Old pointers stay valid when it grows.

Keep the arena itself in one `let` and pass `&Arena` around. Never copy an `Arena` by value into a struct: store `&Arena`.

```rust
struct Game {
    mem: &Arena;
    world: World;
}
```

## Checkpoint and rewind

`checkpoint()` remembers where the cursor is. `rewind(mark)` moves it back, freeing everything allocated since. This is how a game throws away per-frame scratch memory for free:

```rust
fn tick(g: &Game, dt: f32) {
    let frame = g.mem.checkpoint();
    defer g.mem.rewind(frame);

    // anything allocated from g.mem here is gone after this frame
}
```

Memory allocated *before* the checkpoint is untouched, so long-lived data and per-frame scratch can share one arena as long as the long-lived data comes first.

## The scratch arena

`memory.scratch(cap)` gives a process-wide scratch arena (one per thread), and `memory.rewindScratch()` resets it. `j3.app` resets it every frame for you.

## Lists

`List<T>` is a growable array that allocates from an arena:

```rust
let hits = List<int>.new(&mem);
hits.push(3);
hits.push(7);
hits.remove(0);       // keeps order
hits.swapRemove(0);   // O(1): moves the last item into the hole

for i in 0..hits.len {
    printLn(`{hits.items[i]}`);
}
```

`items` and `len` are public fields; `hits.items[0..hits.len]` (or `hits.slice()`) is the live slice. `List` also has `pop`, `last`, `insert(i, x)`, `clear`, `extend`, `reserve`, and `indexOf` / `contains` for types with `==`. When the arena rewinds past where the list allocated, the list's memory goes with it.

`mem.new(value)` allocates one value and returns a `&T` to it, which is how trees of `&T` are built (`mem.new(Expr.Add(mem.new(Expr.Num(2)), r))`).

## Other collections

All of these take their memory from an arena and are not shared across threads:

| Module | What |
|---|---|
| `j3.data.map` | `Map<K, V>`: `put`, `get`, `getOr`, `getPtr`, `getOrPut`, `each`, `entries`, `sortedKeys` |
| `j3.data.set` | `Set<T>`: `add`, `has`, `remove`, `each`, `sorted` |
| `j3.data.pool` | `Pool<T>` with typed `Id<T>` handles (`Id<A>` is not an `Id<B>`; ids are `Map` keys) |
| `j3.data.intern` | `Interner`: `string` to `u32` and back |
| `j3.cmp` | `sort(xs)`, `sortBy(xs, less)`: stable merge sort |

`speculative/lang/collections.j3` uses each of them.

## Entities are indices

The J3 style for game objects is **struct of arrays + integer ids**: keep each field in its own slice, and refer to an entity by its index rather than a pointer. Indices survive reallocation and compaction; pointers don't.

```rust
struct World {
    pos: []Vec2;
    vel: []Vec2;
    count: int;
}

fn spawn(world: &World, p: Vec2, v: Vec2): int {
    let id = world.count;
    world.pos[id] = p;
    world.vel[id] = v;
    world.count += 1;
    return id;
}
```

`j3.game.ecs` builds on the same idea with generational ids, typed columns, and deferred commands.
