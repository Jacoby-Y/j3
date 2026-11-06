# J3 roadmap

Non-goals: no GC, borrow checker, trait objects (`dyn`), drop, escaping closures, or comptime interpreter in v1. Do not CFG-ify check, start a second parser, grow `j3c/src/stdlib/mod.rs`, or add libLLVM / crates.io.

`[emit] target` is one triple or an array (`sys.j3` / `sys_windows.j3` / `sys_macos.j3`). `build` writes one binary per triple; `run` uses the host triple.

## Shipped

- **Language:** typed IR, LLVM (default) and C printers, `--fast`, `--release`. Short-circuit, if-expr arms, `defer`, `+=`, interpolation, wrapping ints, inclusive `..=`, default fields, `@j3ui`, scripts (`.j3s`, bytecode VM plus AOT). Small gaps (labels, `/=` `%=`, int `match`, block-scoped `let`, `0b` and bitwise ops), UFCS, closures (`closure(..)`, see `j3c doc lang`), generic fns, traits (`#[impl]`, `j3.cmp`, `j3.data.map`), atomics on more types with memory orders, `f16` storage, SIMD `vec4f`/`vec4i` (LLVM and C), the shader language (`#[shader]`, `@shader`, `j3c doc shaders`), and path deps (`[deps]`).
- **Tooling:** `j3c lsp` (hover, goto, completion, signature help, outline, import quick-fix, format, cross-buffer diagnostics), `j3c fmt`, `j3c doc`, `j3c size`, the scene editor and MCP server.
- **Runtime checks:** every check names itself as `file:line: reason` and exits 102; `panic` exits 101. Debug builds on x86-64 Linux install a crash handler, symbolized by `j3c run`.
- **Threads (Linux):** `j3.thread` (pool, queue, mutex), `#[thread]` globals, `@atomic*`. See Multithreading.
- **Network and processes (Linux):** `j3.net.tcp`, `j3.net.dns` (`getaddrinfo`), `j3.net.tls` (system OpenSSL 3 through `dlopen`, always verified), `j3.net.http` (HTTP/1.1 client, chunked, redirects, streaming lines for SSE), `j3.data.json`, `j3.proc` (spawn with pipes, PATH lookup) on fork/exec/pipe in `j3.sys`. `j3c doc recipes http`, `examples/openrouter-chat`.
- **UI components:** `.j3ui` (markup, J3 script, scoped CSS) compiled by `j3c` onto `j3.ui.css`/`dom` and the batched `j3.gfx.paint`; `examples/idle-breakout` is the golden reference (plan 022; editor support is stage 08); plan 023 added `{#let}`, data grid, rich textarea, caret popups, flex/selectors/inline boxes, `margin: auto`, `aspect-ratio`, `outline`, `cursor`, `text-decoration`, `font-style`, `:not()` / `:nth-child()` / `:last-child`, `j3c golden`, `J3_STEADY` and `j3.ui.testkit`; plan 026 added grid, transitions and `animate:flip`, drag-and-drop, more selectors and elements, transforms, `@property`, i18n, the F12 inspector, `run --hot`, CSS lint, gamepad focus and the a11y gaps; plan 027 added rotated overflow clips, RTL number neutrals, spacing transitions, `em` in transforms, `url()` custom properties, scroll boxes that are not stacking contexts, positive `tabindex`, radio groups and `<glyph>` input prompts; the screen reader bridge (plan 024) is left).
- **Stdlib:** `App` stages, ECS, collide, space, physics (light: 2D AABB and a 3D voxel-stopping body, no contact solver), path, anim, input, assets, save, particles, camera rig, UI, `j3.gfx` (GL 3.3, one shadow map, 4 point lights, instancing), voxel grid, math, mesh formats, SDL audio with groups, fades, and positional channels, `scene3d`.

## Left

- **Scale:** one translation unit per `cc`, and mono re-walks the AST. Measure check vs emit vs clang before touching it.
- **Package deps.** Path deps (`[deps]`) shipped; still open: versions or a registry, a dep's assets, hot reload watching dep files.
- **Ship a Windows binary:** mingw `clang` and SDL import libs. A missing lib stays a link error.
- **Pack / install:** rpath and asset roots matter once there is a build to hand someone.
- **Network follow-ups:** macOS and Windows for `j3.net.tcp`/`dns`/`tls` and `j3.proc` (stubs today), keep-alive and pooling, gzip, a non-blocking poll-style HTTP request for the frame thread, an HTTP server, WebSockets, `SSL_CTX` shared across requests, IP-literal certificate checks.
- **Later:** Wasm, consoles.
- **Idle frame skipping (plan 029 chunk 26):** `App` sleeps while a `.j3ui` UI is still (`App.idleSkip`, `J3_IDLE_SKIP`); left: skip the draw callback on safety ticks (they repaint unchanged frames), name more timers instead of the 250 ms tick, a per-`Dom` opt-out for apps that mix 3D and UI, check by hand in a real window (caret blink, hover, resize, drag).
- **J3 shader language follow-ups:** the language, checker, GLSL emitter, `@shader(vs, fs)` and `j3c dump-glsl` shipped (`j3c doc shaders`), and the p2 shader is J3 (`game/p2shader.j3`); still open: port `post.j3`'s blur, bright and fx effects, which are GLSL strings, uniform blocks / std140, MRT, `textureGrad`, integer textures, `gl_FragDepth`, geometry/compute, LSP hover for shader types, `j3c fmt` of `#[..]` attributes (it prints `# [..]`), a clearer error when CPU code names a `#[uniform]` (it says unknown name).
- **SIMD follow-ups:** `vec4f` / `vec4i` run on LLVM and C (the VM rejects them with a span); no lane masks or compares, no vector consts or global initializers, no wider vectors.

## Later (UI rendering)

From plan 029 phase C (measurements, plan since deleted). Idle skip shipped (chunk 26); the rest was skipped by user decision on 2026-11-01, since the non-DOM scenes are already sub-ms. Hotspot: `bench2d ui 1000` is 2001 draws, gpu ~5.9 ms, cpu ~5.9 ms (two draw calls per card; the batch breaks per card). Others: `paint-mixed` 0.44 ms, `big_ui_test` page 14 0.7 ms per frame.

- **Retained instance ranges per stacking context** (`ui/dom/retain.j3`): rebuild a context's `p2` instances only when its style, layout, text or motion changed. The main CPU cost in the `ui 1000` hotspot.
- **Partial GPU upload:** keep `p2`'s VBO and upload changed ranges with `glBufferSubData`; orphan when it grows. GLES 3.0 compatible. The GPU half of the `ui 1000` hotspot.
- **Layer texture cache:** `beginLayer`, backdrop blur and big shadows render to a cached texture with an LRU cap. `big_ui_test` page 13 spends 0.2 ms in blur and 0.2 ms in layers.
- **Damage regions:** persistent offscreen buffer, scissor to the changed rects, full redraw on resize or many rects.
- **Specialized paint shaders:** split `p2` into rect, round rect, text, gradient and shadow variants in the J3 shader language, batching by variant without breaking paint order.
- **SIMD in hot CPU loops:** instance transform and clip math, `p2TextRun` glyph positions, flex and grid sizing. Profile first.
- **SDF text atlas:** one atlas per font for all sizes; check small-size text in the goldens before switching.

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

- **Stdlib only:** `Vec2.xy` (mirror of `Vec3.xyz`).
- **`f16` follow-ups:** storage only; no arithmetic (convert with `f32(h)`), no interpolation, no `const` of type `f16`, not a `j3.gpu` vertex attribute (half attributes), not in `.j3s`.
- **Generic fn follow-ups:** the return type is never inferred; methods of non-generic structs cannot infer their own type params (still `::<T>`); instance symbol names embed the type text, so two same-named types from different modules as `T` could collide when `T` is not in the signature.
- **Trait follow-ups:** bodies are checked per instance, not once against the bounds (needs a placeholder type through lowering); `#[impl]` fns are found by `x.m()` in every module, not only where a trait with `m` is imported; a bound is a bare name (`T: Eq`, not `T: cmp.Eq`); `j3c fmt` spaces generics and attributes badly (`fn f < T >`, `# [impl]`); no `where`, default methods, supertraits, generic traits or `Default`/`Debug` traits. `Map` has no `free` (arena memory) and no closure `forEach`; a `Set` and `List.indexOf` (needs lazy generic-struct methods) are not written.
- **UFCS follow-ups:** `j3c lsp` completion after `x.` does not list matching free fns yet (it needs the fns visible at the cursor and a first-parameter type test).

## Multithreading

Goal: let a game move CPU-heavy work (world meshing, generation, pathfinding, asset decoding, baking) off the frame thread without the language growing a memory model. Threads, locks, a queue, a pool. No async, no scheduler, no shared GC.

### Shipped (Linux)

`j3.thread` (`j3c doc threads` is the guide, `j3c doc recipes threads` a program, `speculative/13_threads.j3`, run on both backends): `spawn`/`join`, `Mutex`, `Cond`, `Queue<T>` (blocking and non-blocking), `Pool` (`submit`, non-blocking `trySubmit`, `pendingJobs`, `wait`, `parallelFor`, `shutdown`), `cores()`. A frame thread feeds the pool with `trySubmit` and drains a results `Queue` with `tryPop`, dropping results whose version stamp went stale; `j3c doc recipes threads` is that program. It is pthreads through `use lib "c"`; libc is always linked, so there is no raw `clone` path.

Language: `#[thread]` on a module `let` (one copy per thread; `thread_local` in LLVM, `_Thread_local` in C) and `@atomicLoad` / `@atomicStore` / `@atomicAdd` / `@atomicSub` / `@atomicExchange` / `@atomicCas` on `int`, `i32`, `u32`, `u64`, `bool`, `ptr` through a `&T`, with an optional memory order (`relaxed`, `acquire`, `release`, `acqrel`, `seqcst` default). The VM rejects atomics; `#[thread]` is ignored there since scripts run on the owner thread only.

Stdlib: `memory.SCRATCH`, `text.cstrScratch`, and `sys.THREAD_NAME` are `#[thread]`. A panic on a named thread prints `[name] ` first. Every thread gets a crash stack, so a stack overflow in a worker still reaches the crash handler (its trace has addresses but not the thread name).

### Rules of the model

1. **Share by handing off.** A worker owns the buffers it was given; the owner reads results only after a completion signal. Everything written before `unlock`, `signal`, `push`, or an atomic store is visible after the matching `lock`, `wait`, `pop`, or atomic load, and nothing else is ordered. Do not spin on a plain flag.
2. **Static functions only.** The thread entry is a plain `fn(ptr)`; its state arrives as one `ptr` to a struct the spawner owns. A capturing closure cannot be a job (`closure(..)` never converts to `fn(ptr)`); only helpers that finish before returning take one (`speculative/lang/closures.j3`).
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
- **Job priority and cancel.** There is neither; `j3c doc threads` documents the workaround (few jobs in flight, urgent ones first; a per-job `@atomicStore` flag for cancel). Add a real cancel only if a game needs one.

### Out of scope

`async`/`await`, green threads, work stealing, data-parallel kernels, a race detector in the compiler. Revisit only after a game needs one.
