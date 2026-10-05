# J3 roadmap

Non-goals: no GC, borrow checker, traits, drop, capturing closures, or comptime interpreter in v1. Do not CFG-ify check, start a second parser, grow `j3c/src/stdlib/mod.rs`, or add libLLVM / crates.io.

`[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

- **Language:** typed IR, LLVM (default) and C printers, `--fast`, `--release`. Short-circuit, if-expr arms, `defer`, `+=`, interpolation, wrapping ints, inclusive `..=`, default fields, `@j3ui`, scripts (`.j3s`, bytecode VM plus AOT).
- **Tooling:** `j3c lsp` (hover, goto, completion, signature help, outline, import quick-fix, format, cross-buffer diagnostics), `j3c fmt`, `j3c doc`, `j3c size`, the scene editor and MCP server.
- **Runtime checks:** every check names itself as `file:line: reason` and exits 102; `panic` exits 101. Debug builds on x86-64 Linux install a crash handler, symbolized by `j3c run`.
- **Threads (Linux):** `j3.thread` (pool, queue, mutex), `#[thread]` globals, `@atomic*`. See Multithreading.
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

Goal: let a game move CPU-heavy work (world meshing, generation, pathfinding, asset decoding, baking) off the frame thread without the language growing a memory model. Threads, locks, a queue, a pool. No async, no scheduler, no shared GC.

### Shipped (Linux)

`j3.thread` (`j3c doc threads` is the guide, `j3c doc recipes threads` a program, `speculative/13_threads.j3`, run on both backends): `spawn`/`join`, `Mutex`, `Cond`, `Queue<T>` (blocking and non-blocking), `Pool` (`submit`, non-blocking `trySubmit`, `pendingJobs`, `wait`, `parallelFor`, `shutdown`), `cores()`. A frame thread feeds the pool with `trySubmit` and drains a results `Queue` with `tryPop`, dropping results whose version stamp went stale; `j3c doc recipes threads` is that program. It is pthreads through `use lib "c"`; libc is always linked, so there is no raw `clone` path.

Language: `#[thread]` on a module `let` (one copy per thread; `thread_local` in LLVM, `_Thread_local` in C) and `@atomicLoad` / `@atomicStore` / `@atomicAdd` / `@atomicCas` on an `int` through a pointer (sequentially consistent). The VM rejects atomics; `#[thread]` is ignored there since scripts run on the owner thread only.

Stdlib: `memory.SCRATCH`, `text.cstrScratch`, and `sys.THREAD_NAME` are `#[thread]`. A panic on a named thread prints `[name] ` first. Every thread gets a crash stack, so a stack overflow in a worker still reaches the crash handler (its trace has addresses but not the thread name).

### Rules of the model

1. **Share by handing off.** A worker owns the buffers it was given; the owner reads results only after a completion signal. Everything written before `unlock`, `signal`, `push`, or an atomic store is visible after the matching `lock`, `wait`, `pop`, or atomic load, and nothing else is ordered. Do not spin on a plain flag.
2. **Static functions only.** The thread entry is a plain `fn(ptr)`; its state arrives as one `ptr` to a struct the spawner owns. No capturing closures.
3. **One arena per thread.** `Arena` is not shared. `memory.scratch` is per thread; pool workers rewind it after each job.
4. **Platform calls stay on the owner thread.** GL, SDL, window, audio, and file dialogs are main-thread only, as is all `j3.game`, `j3.gfx`, `j3.scene3d`, `j3.ui`, and `j3.app` state. Workers compute; the main thread uploads and draws.
5. **Races are the user's to avoid.** No compiler enforcement. Byte-wise reads of data another thread is editing are tolerated only where a version stamp makes the result discardable.
6. **Determinism.** Pool scheduling is nondeterministic. Anything that feeds saves, replays, or netcode reduces results in a fixed order (`parallelFor` writing `out[i]` does).

### Left

- **Stdlib audit.** Done by reading module globals and imports: the safe-on-a-worker list, the main-thread-only modules, and the pure `gfx` functions (`emitFace`, `emitFaceAo`, `boxInClip`, `rayAabb`, `plainSurface`, `fxWidth`) are in `j3c doc threads` and each module's header. Not done: `printLn` is two `write`s, so lines from two threads can interleave; `gfx` methods on structs were not audited.
- **Windows and macOS:** `CreateThread` and `SRWLOCK` / pthreads variants selected like `sys` (`thread_windows.j3`).
- **Scripts:** the `.j3s` VM stays single-threaded. A script requesting work through a native job queue is not built.
- **Hot reload:** `j3c run` swaps script code into a running game. Safe today because scripts never run on workers; keep it that way.
- **Crash traces** do not name the thread (panics do).
- **Older libc:** before glibc 2.34 pthreads live in `libpthread`; link it when the target is older.
- **Atomics** are `int` only and have no relaxed or acquire/release forms.
- **Job priority and cancel.** There is neither; `j3c doc threads` documents the workaround (few jobs in flight, urgent ones first; a per-job `@atomicStore` flag for cancel). Add a real cancel only if a game needs one.

### Out of scope

`async`/`await`, green threads, work stealing, data-parallel kernels, a race detector in the compiler. Revisit only after a game needs one.
