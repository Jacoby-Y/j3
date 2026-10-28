A headless program: a `Pool` builds chunks off the main thread while a loop that stands in
for the frame thread feeds it and collects results without ever blocking. This is the shape
of world meshing, generation, decoding, and baking. `j3c doc threads` is the full guide
(rules, which modules are safe, common mistakes); `speculative/13_threads.j3` shows every
primitive.

The idea: hand work over, do not share it.
- A job is a plain `fn(ptr)` plus one `ptr` to a struct main owns. Put everything the job needs
  in that struct: its input, the buffer it alone writes, and the `&Queue` its result goes to.
  There are no closures. The struct must outlive the job (here: slots in an arena).
- A worker writes only what it was given. Main reads a result only after it comes back through
  the results `Queue` (or after `pool.wait()`). Nothing else orders threads; never spin on a
  plain flag. `@atomicAdd` / `@atomicLoad` are for counters and cancel flags.
- Never block the frame thread. `pool.trySubmit` returns false when the job queue is full:
  keep the job and try again next frame. `results.tryPop` returns a result or nothing. `submit`,
  `wait`, `push`, `pop`, and `join` can all wait; use them in loaders and tools.
- Size the results queue at least as large as the jobs in flight, or workers block in `push`
  until main drains it.
- Version stamps: each result carries the version it was built from. Main drops a result whose
  version is no longer current (here chunk 3 is edited, so its first build is stale). To cancel
  a running job, put a flag in its struct, set it with `@atomicStore`, and have the job check
  `@atomicLoad` between steps.
- GL, SDL, windows, and audio are main-thread only, and so is `j3.gfx` / `j3.game` / `j3.scene3d`
  state. Workers compute (`gfx.emitFace`, `voxel`, `noise`, `canvas`, `math`, `game.path` are
  safe); main uploads and draws.
- `memory.scratch` is per thread and a worker rewinds it after every job, so copy results into
  the struct main gave you. One `Arena` is never shared between threads.
- Results arrive in any order. If they feed saves or replays, reduce in a fixed order
  (`pool.parallelFor(n, f, ctx)` splits `0..n` for you; have `f` write `out[i]`).
- Call `pool.shutdown()` before destroying the arena the pool lives in.
