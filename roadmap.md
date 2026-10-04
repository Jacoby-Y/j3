# J3 roadmap

Non-goals: no GC, borrow checker, traits, drop, capturing closures, or comptime interpreter in v1. Do not CFG-ify check, start a second parser, grow `j3c/src/stdlib/mod.rs`, or add libLLVM / crates.io.

`[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

- **Language:** typed IR, LLVM (default) and C printers, `--fast`, `--release`. Short-circuit, if-expr arms, `defer`, `+=`, interpolation, wrapping ints, inclusive `..=`, default fields, `@j3ui`, scripts (`.j3s`, bytecode VM plus AOT).
- **Tooling:** `j3c lsp` (hover, goto, completion, signature help, outline, import quick-fix, format, cross-buffer diagnostics), `j3c fmt`, `j3c doc`, `j3c size`, the scene editor and MCP server.
- **Runtime checks:** every check names itself as `file:line: reason` and exits 102; `panic` exits 101. Debug builds on x86-64 Linux install a crash handler, symbolized by `j3c run`.
- **Stdlib:** `App` stages, ECS, collide, space, physics (light: 2D AABB and a 3D voxel-stopping body, no contact solver), path, anim, input, assets, save, particles, camera rig, UI, `j3.gfx` (GL 3.3, one shadow map, 4 point lights, instancing), voxel grid, math, mesh formats, SDL audio with groups, fades, and positional channels, `scene3d`.

## Left

- **Scale:** one translation unit per `cc`, and mono re-walks the AST. Measure check vs emit vs clang before touching it.
- **Rename and `j3.toml` deps.** Language tests stay `cargo test` plus a playable example until packages exist.
- **Ship a Windows binary:** mingw `clang` and SDL import libs. A missing lib stays a link error.
- **Pack / install:** rpath and asset roots matter once there is a build to hand someone.
- **Later:** Wasm, consoles, a J3 shader language, SIMD.

## Build speed

fps-shooter (54 modules, ~45k lines with stdlib); `J3C_TIMINGS=1` shows the stages. Shipped: check 0.80s to 0.17s, cold build 1.45s to 0.75s (emit inside the parallel clang workers, `mold` when present), a struct edit recompiles 3 of 54 modules, and a rebuilt `j3c` no longer invalidates objects.

Open: `lower_module` clones per-module maps (0.06s), the LSP rechecks everything per edit, and there is no function-level pruning.

## Binary size

Shipped: `--release` (opt `s`, stripped, no unwind tables on Linux), one `@j3_abort` for all checks in whole-program builds. fps-shooter 4.5 MB to 1.3 MB.

Open:
- A small 3D game still links ~117 KB of `j3.scene3d` and ~66 KB of `j3.gfx` it never runs (URP tracing, PNG decode). Find the roots and split flag-only code into its own import.
- `--icf=safe` is wired for `ld.lld` but unmeasured; drop it if it gains under 1%.
- macOS release flags are pinned by a unit test but untested against a real ld64.

## Panics

Open:
1. The `.j3s` VM's `abort` op and the C backend do not print `file:line`.
2. Traces stop at 32 frames and need `nm` + `addr2line`.
3. The crash handler is x86-64 Linux only.

Out of scope: exceptions, unwinding, `defer` on panic.

## Language friction

Not blockers; the games compile without any of this.

- **Literals on the left of `+ - * /`.** An untyped left literal becomes `int` or `f64`, so `2 * far` fails where `far * 2` works. This costs ~37 `f32(0) -` sites and ~70 `f32(<number>)` casts. Fix: an untyped left literal takes the right operand's or the destination's type. Do not change the no-context default away from `f64`.
- **Stdlib only:** `Vec2.xy` (mirror of `Vec3.xyz`).
- **Not worth it yet:** int `match`, UFCS, `f16`, labeled break, block-scoping every `let`.

## Multithreading

Goal: let a game move CPU-heavy work (world meshing, generation, pathfinding, asset decoding, baking) off the frame thread without the language growing a memory model. Keep it small: threads, locks, a queue. No async, no scheduler, no shared GC.

### Starting point

Threads already work with no compiler change. A J3 function passes as a `fn(ptr): ptr` to libc's `pthread_create` through `use lib "c"`, runs concurrently, and joins. What is missing is a safe, portable stdlib surface, and the answers to the questions below.

What exists: `fn(T)` values (non-capturing), C FFI, `mmapAnon`, `@syscall`.
What does not: atomics, a thread API, thread-safe stdlib guarantees, a Windows path.

### Rules of the model

1. **Share by handing off.** A worker owns the buffers it was given; the owner reads results only after a completion signal. A queue and a semaphore are the only cross-thread ordering tool. Do not spin on a plain flag: LLVM may hoist the load.
2. **Static functions only.** The thread entry is a plain `fn(ptr)`; its state arrives as one `ptr` to a struct the spawner owns. No capturing closures.
3. **One arena per thread.** `Arena` is not shared. Workers allocate from their own arena or from buffers handed in. The main arena is never touched concurrently.
4. **Platform calls stay on the owner thread.** GL, SDL, window, audio, and file dialogs are main-thread only. Workers compute; the main thread uploads and draws.
5. **Races are the user's to avoid.** No compiler enforcement in v1. Byte-wise reads of data another thread is editing are tolerated only where a version stamp makes the result discardable.

### Stdlib: `j3.thread`

| Item | What |
|---|---|
| `spawn(f: fn(ptr), arg: ptr): Thread` | start a thread; `join(&t)` waits |
| `Mutex` | `lock`, `unlock`, `tryLock` |
| `Sem` | counting semaphore: `post`, `wait`, `tryWait` |
| `Queue` | fixed-capacity ring of slots with a mutex and a semaphore; single or multiple producers; blocking `pop` for workers, non-blocking `tryPop` for the frame thread |
| `Pool` | N workers draining one `Queue` of jobs (`fn(ptr)` plus a `ptr`); `submit`, `shutdown` |
| `cores()` | logical core count, to size a pool |

Per-OS files follow `sys` / `sys_windows` / `sys_macos`: pthreads on Linux and macOS, `CreateThread` and `SRWLOCK` on Windows. The Linux version is built first.

### Phases

1. **Primitives.** `Thread`, `Mutex`, `Sem` on Linux, with a test that hammers a shared counter under a mutex and a producer/consumer test. Check `--fast`, debug, and `--release` builds, and that the crash handler reports a fault from a worker.
2. **Queue and Pool.** Job slots owned by the pool, so submitting never allocates. Document the hand-off rule with one complete recipe in `j3c doc recipes`.
3. **Thread-safety audit of the stdlib.** Mark each module as pure, per-instance, or global. Known suspects are shared scratch buffers (for example the `text.cstr` buffer), lazily built tables, and `printLn`. Anything global gets a lock or a per-thread copy. Pure modules (`math`, `gfx.voxel`, `gfx.mesh` emit, `gfx.noise`, `gfx.canvas` on its own arena) are documented as safe on any thread.
4. **Windows and macOS** variants, and `j3.toml` / `[emit]` target selection.
5. **Debugging aids.** Thread names in crash traces and panics, and a `j3c run` note when a worker panics.
6. **Scripts.** The `.j3s` VM stays single-threaded and runs on the owner thread. A script can request work through a native job queue, but never runs on a worker.

### Patterns the library should make easy

- **Producer/consumer with versioning.** Main thread dispatches `(job, version)`, a worker computes into its own buffer, main thread accepts the result only if the version is still current, then uploads. Used for chunked or sectioned meshing, generation, and baking.
- **Fork/join over a range.** Split `0..n` across the pool and wait on a countdown semaphore.
- **Background loading.** Decode images, audio, and models on a worker; main thread creates GPU and audio objects.

### Risks and open questions

- **No atomics.** Mutex and semaphore are enough for queues, but a counter read by the frame thread costs a lock. Decide whether to add `@atomic_load`, `@atomic_store`, and `@atomic_add` intrinsics to the LLVM printer (cheap there; the C and VM backends need matching forms).
- **Memory ordering.** State the guarantee: everything written before `post` or `unlock` is visible after the matching `wait` or `lock`, and nothing else.
- **Allocation cost.** Per-thread arenas need a reset story so a worker's scratch is reused, not remapped, per job.
- **Hot reload.** `j3c run` swaps script code into a running game. Workers must be quiescent at the swap, or scripts must never be called from them.
- **Determinism.** Pool scheduling is nondeterministic. Anything that feeds saves, replays, or netcode must reduce results in a fixed order.
- **Older libc.** Before glibc 2.34 pthreads live in `libpthread`; link it when the target is older.

### Out of scope

`async`/`await`, green threads, work stealing, thread-local storage, data-parallel kernels, a race detector in the compiler. Revisit only after a game needs one.
