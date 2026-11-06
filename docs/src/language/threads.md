# Threads

`j3.thread` gives you threads on Linux: `spawn`/`join`, a `Pool` of workers, `Queue<T>`, `Mutex`, and `Cond`.

## The one rule

**Hand work over; don't share it.** Give a worker a struct that only it writes, wait for its result, then read it. The compiler does not check for data races, so this rule is the whole safety net.

## Picking a tool

| You want to... | Use |
|---|---|
| Run one thing in the background | `spawn(&t, name, f, arg)`, later `join(&t)` |
| Run many jobs on a few workers | `Pool.create(&arena, workers, queueCap)`, then `submit` / `trySubmit` |
| Do the same work over `0..n` | `pool.parallelForEach(n, fn(lo, hi) { .. })` (a closure; it waits for every range, which is why a closure is safe here). Lower level: `pool.parallelFor(n, f, ctx)` |
| Get results back to the frame loop | A `Queue<T>`: workers push, the main thread `tryPop`s |
| Share a counter or cancel flag | `@atomicAdd` / `@atomicLoad` / `@atomicStore` on an `int`, `i32`, `u32`, `u64`, `bool` or `ptr` (`@atomicAdd` only on the integers), with an optional order (`acquire`, `release`, `relaxed`, ...) |
| One value per thread | `#[singleton]` + `#[thread]` on a `let` |

A thread's entry point is a plain `fn(ptr)`: its state arrives as one pointer to a struct the spawner owns.

## The frame thread must never block

Feed a pool with `trySubmit` and drain results with `tryPop`, so a slow job never stalls a frame. Stamp each job with a version and drop results that come back stale.

## Main-thread only

The window, OpenGL, SDL, audio, and every `j3.game`, `j3.gfx`, `j3.scene3d`, `j3.ui`, and `j3.app` value belong to the main thread. Each thread needs its own `Arena`.

`j3c doc recipes threads` prints a complete program; `j3c doc threads` has the full guide.
